//! Staged CFM section layout, relocation and metadata, without guest execution.
//! PowerPC System Software (1994), pp. 1-25–1-26 and 3-21–3-22.

use super::{CfmExport, CfmLoadError, CfmMemory};
use crate::guest_procedure::GuestProcedureMemory;
use crate::loader::pef::{
    apply_pef_relocations_detailed, instantiate_pef_sections, parse_pef_exported_symbols,
    parse_pef_header, parse_pef_loader_header, parse_pef_reloc_headers, parse_pef_sections,
    pef_reloc_chunk_stream, PefRelocContext, SECTION_KIND_CODE, SECTION_KIND_EXECUTABLE_DATA,
    SECTION_KIND_PATTERN_DATA, SECTION_KIND_UNPACKED_DATA,
};

/// Read precisely the PEF container extent, bounded by the logical resource
/// when known. Grow only after each mapped read succeeds, never from an
/// unchecked guest-declared packed size. PowerPC System Software, pp. 2-27–2-28.
pub(crate) fn read_resource_fragment(
    memory: &mut impl GuestProcedureMemory,
    address: u32,
    available: Option<u32>,
) -> Option<Vec<u8>> {
    fn extend(
        memory: &mut impl GuestProcedureMemory,
        address: u32,
        available: Option<u32>,
        bytes: &mut Vec<u8>,
        length: u32,
    ) -> Option<()> {
        if available.is_some_and(|size| size < length) {
            return None;
        }
        address.checked_add(length.checked_sub(1)?)?;
        while bytes.len() < length as usize {
            let byte = memory.procedure_read_u8(address + bytes.len() as u32)?;
            bytes.push(byte);
        }
        Some(())
    }
    let mut bytes = Vec::new();
    extend(memory, address, available, &mut bytes, 40)?;
    let header = parse_pef_header(&bytes)?;
    if header.architecture != *b"pwpc" || header.format_version != 1 {
        return None;
    }
    let table_len = 40u32.checked_add(u32::from(header.section_count).checked_mul(28)?)?;
    extend(memory, address, available, &mut bytes, table_len)?;
    let mut length = table_len;
    for section in parse_pef_sections(&bytes)? {
        length = length.max(section.container_offset.checked_add(section.packed_size)?);
    }
    extend(memory, address, available, &mut bytes, length)?;
    Some(bytes)
}

#[derive(Debug)]
pub(crate) struct CfmSection {
    pub(crate) index: usize,
    pub(crate) section_kind: u8,
    pub(crate) base: u32,
    pub(crate) bytes: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct CfmPreparedFragment {
    pub(crate) main_addr: u32,
    pub(crate) init_addr: u32,
    pub(crate) term_addr: u32,
    pub(crate) exports: Vec<CfmExport>,
}

/// A plan owns uncommitted bytes only. The process allocator must accept the
/// cursor transition and atomic publication before import registrations escape.
#[derive(Debug)]
pub(crate) struct CfmFragmentPlan {
    sections: Vec<CfmSection>,
    next_heap_cursor: u32,
    fragment: CfmPreparedFragment,
}

impl CfmFragmentPlan {
    pub(crate) fn prepare(
        fragment: &[u8],
        import_addrs: &[u32],
        heap_cursor: u32,
        heap_limit: u32,
        allocation_alignment: u32,
        mut allocation_bounds: impl FnMut(u32, u32, u32) -> Option<(u32, u32)>,
    ) -> Result<Self, CfmLoadError> {
        let header = parse_pef_header(fragment).ok_or(CfmLoadError::CorruptFragment)?;
        if header.architecture != *b"pwpc" || header.format_version != 1 {
            return Err(CfmLoadError::CorruptFragment);
        }
        let loader = parse_pef_loader_header(fragment).ok_or(CfmLoadError::CorruptFragment)?;
        if usize::try_from(loader.total_imported_symbol_count).ok() != Some(import_addrs.len()) {
            return Err(CfmLoadError::CorruptFragment);
        }
        let instantiated =
            instantiate_pef_sections(fragment).ok_or(CfmLoadError::CorruptFragment)?;
        let mut sections = Vec::with_capacity(instantiated.len());
        let mut cursor = heap_cursor;
        for section in instantiated {
            let alignment = if section.header.alignment < 31 {
                1u32 << section.header.alignment
            } else {
                return Err(CfmLoadError::CorruptFragment);
            };
            let size =
                u32::try_from(section.bytes.len()).map_err(|_| CfmLoadError::NoAddressSpace)?;
            let (base, next) =
                allocation_bounds(cursor, size, alignment).ok_or(CfmLoadError::NoAddressSpace)?;
            if base < cursor
                || base % alignment != 0
                || base.checked_add(size) != Some(next)
                || next >= heap_limit
            {
                return Err(CfmLoadError::NoAddressSpace);
            }
            sections.push(CfmSection {
                index: section.index,
                section_kind: section.header.section_kind,
                base,
                bytes: section.bytes,
            });
            cursor = next;
        }
        if !allocation_alignment.is_power_of_two() {
            return Err(CfmLoadError::NoAddressSpace);
        }
        cursor = cursor
            .checked_add(allocation_alignment - 1)
            .map(|value| value & !(allocation_alignment - 1))
            .filter(|value| *value < heap_limit)
            .ok_or(CfmLoadError::NoAddressSpace)?;
        let bases = section_bases(&sections);
        let code_base = first_base_for_kind(&sections, SECTION_KIND_CODE)
            .ok_or(CfmLoadError::CorruptFragment)?;
        let data_base = first_data_base(&sections).unwrap_or(code_base);
        let relocations = parse_pef_reloc_headers(fragment).ok_or(CfmLoadError::CorruptFragment)?;
        for relocation in &relocations {
            let stream = pef_reloc_chunk_stream(fragment, relocation)
                .ok_or(CfmLoadError::CorruptFragment)?;
            let section = sections
                .iter_mut()
                .find(|section| section.index == usize::from(relocation.section_index))
                .ok_or(CfmLoadError::CorruptFragment)?;
            let context = PefRelocContext {
                code_base,
                data_base,
                section_bases: &bases,
                import_addrs,
            };
            apply_pef_relocations_detailed(&mut section.bytes, stream, &context)
                .map_err(|_| CfmLoadError::CorruptFragment)?;
        }
        let prepared = CfmPreparedFragment {
            main_addr: fragment_special_tvector(
                &sections,
                loader.main_section,
                loader.main_offset,
            )?,
            init_addr: fragment_special_tvector(
                &sections,
                loader.init_section,
                loader.init_offset,
            )?,
            term_addr: fragment_special_tvector(
                &sections,
                loader.term_section,
                loader.term_offset,
            )?,
            exports: resolve_fragment_exports(fragment, &sections, import_addrs)?,
        };
        Ok(Self {
            sections,
            next_heap_cursor: cursor,
            fragment: prepared,
        })
    }

    pub(crate) fn next_heap_cursor(&self) -> u32 {
        self.next_heap_cursor
    }

    pub(crate) fn publish(&self, memory: &mut impl CfmMemory) -> bool {
        let writes: Vec<_> = self
            .sections
            .iter()
            .map(|section| (section.base, section.bytes.as_slice()))
            .collect();
        memory.publish_cfm_outputs(&writes)
    }

    pub(crate) fn into_fragment(self) -> CfmPreparedFragment {
        self.fragment
    }
}

pub(crate) fn section_bases(mapped: &[CfmSection]) -> Vec<Option<u32>> {
    let mut bases = Vec::new();
    for section in mapped {
        if bases.len() <= section.index {
            bases.resize(section.index + 1, None);
        }
        bases[section.index] = Some(section.base);
    }
    bases
}

pub(crate) fn first_base_for_kind(mapped: &[CfmSection], kind: u8) -> Option<u32> {
    mapped
        .iter()
        .find(|section| section.section_kind == kind)
        .map(|section| section.base)
}

pub(crate) fn first_data_base(mapped: &[CfmSection]) -> Option<u32> {
    mapped
        .iter()
        .find(|section| {
            matches!(
                section.section_kind,
                SECTION_KIND_UNPACKED_DATA
                    | SECTION_KIND_PATTERN_DATA
                    | SECTION_KIND_EXECUTABLE_DATA
            )
        })
        .map(|section| section.base)
}

pub(crate) fn resolve_fragment_exports(
    fragment: &[u8],
    mapped_sections: &[CfmSection],
    import_addrs: &[u32],
) -> Result<Vec<CfmExport>, CfmLoadError> {
    let loader = parse_pef_loader_header(fragment).ok_or(CfmLoadError::CorruptFragment)?;
    let exported_symbols = if loader.exported_symbol_count == 0 {
        Vec::new()
    } else {
        parse_pef_exported_symbols(fragment).ok_or(CfmLoadError::CorruptFragment)?
    };
    exported_symbols
        .into_iter()
        .map(|symbol| {
            let address = match symbol.section_index {
                section_index if section_index >= 0 => {
                    let section_index = usize::try_from(section_index)
                        .map_err(|_| CfmLoadError::CorruptFragment)?;
                    let section = mapped_sections
                        .iter()
                        .find(|section| section.index == section_index)
                        .ok_or(CfmLoadError::CorruptFragment)?;
                    let offset = usize::try_from(symbol.symbol_value)
                        .map_err(|_| CfmLoadError::CorruptFragment)?;
                    if offset >= section.bytes.len() {
                        return Err(CfmLoadError::CorruptFragment);
                    }
                    section
                        .base
                        .checked_add(symbol.symbol_value)
                        .ok_or(CfmLoadError::NoAddressSpace)?
                }
                // Inside Macintosh: PowerPC System Software (1994),
                // pp. 1-25--1-26: PEF uses -2 for an absolute export.
                -2 => symbol.symbol_value,
                // A re-export stores the imported-symbol index in the value
                // field. Resolve only through the already checked local
                // import address table; never index the raw fragment bytes.
                -3 => {
                    let import_index = usize::try_from(symbol.symbol_value)
                        .map_err(|_| CfmLoadError::CorruptFragment)?;
                    *import_addrs
                        .get(import_index)
                        .ok_or(CfmLoadError::CorruptFragment)?
                }
                _ => return Err(CfmLoadError::CorruptFragment),
            };
            Ok(CfmExport {
                name: symbol.name,
                class: symbol.class,
                address,
            })
        })
        .collect()
}

fn fragment_special_tvector(
    mapped_sections: &[CfmSection],
    section_index: i32,
    offset: u32,
) -> Result<u32, CfmLoadError> {
    if section_index < 0 {
        return Ok(0);
    }
    let section_index =
        usize::try_from(section_index).map_err(|_| CfmLoadError::CorruptFragment)?;
    let section = mapped_sections
        .iter()
        .find(|section| section.index == section_index)
        .ok_or(CfmLoadError::CorruptFragment)?;
    let offset_usize = usize::try_from(offset).map_err(|_| CfmLoadError::CorruptFragment)?;
    if offset_usize
        .checked_add(8)
        .filter(|end| *end <= section.bytes.len())
        .is_none()
    {
        return Err(CfmLoadError::CorruptFragment);
    }
    section
        .base
        .checked_add(offset)
        .ok_or(CfmLoadError::NoAddressSpace)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::guest_procedure::GuestProcedureMemory;
    use crate::loader::pef::tests::{
        block_copy_section, run_reloc, synthetic_loader_with_reloc_header, synthetic_pef,
    };
    use crate::memory::{GuestAddressSpace, MacMemoryBus};

    const HEAP: u32 = 0x0300_0000;
    const IMPORT: u32 = 0x0200_0000;

    fn fragment(chunks: &[u16]) -> Vec<u8> {
        synthetic_pef(
            synthetic_loader_with_reloc_header(chunks),
            block_copy_section(&[0; 12]),
            12,
            12,
        )
    }

    fn bounds(cursor: u32, size: u32, alignment: u32) -> Option<(u32, u32)> {
        let base = cursor.checked_add(alignment - 1)? & !(alignment - 1);
        Some((base, base.checked_add(size)?))
    }

    fn read(memory: &mut impl GuestProcedureMemory, address: u32, size: u32) -> Vec<Option<u8>> {
        (0..size)
            .map(|offset| memory.procedure_read_u8(address + offset))
            .collect()
    }

    #[test]
    fn resource_container_reads_respect_mapping_and_logical_extent_in_both_views() {
        for fault in 0..7 {
            let mut outcomes = Vec::new();
            for classic in [false, true] {
                let mut fragment = fragment(&[run_reloc(0x23, 1), run_reloc(0x25, 1)]);
                if fault == 4 {
                    fragment[8..12].copy_from_slice(b"m68k");
                }
                if fault == 5 {
                    fragment[40 + 20..40 + 24].copy_from_slice(&u32::MAX.to_be_bytes());
                }
                let available = match fault {
                    1 => Some(fragment.len() as u32 - 1),
                    2 => Some(39),
                    _ => None,
                };
                let address = if fault == 6 { u32::MAX - 30 } else { HEAP };
                let mut memory = GuestAddressSpace::new();
                let mapped = if fault == 3 {
                    fragment[..fragment.len() - 1].to_vec()
                } else {
                    fragment.clone()
                };
                memory.add_region(HEAP, mapped);
                let mut bus = MacMemoryBus::new(0x10000);
                bus.set_addressing_32_bit(true);
                bus.attach_guest_address_space(memory.shared_view());
                let result = if classic {
                    read_resource_fragment(&mut bus, address, available)
                } else {
                    read_resource_fragment(&mut memory, address, available)
                };
                assert_eq!(
                    result,
                    if fault == 0 { Some(fragment) } else { None },
                    "fault {fault}"
                );
                outcomes.push(result);
            }
            assert_eq!(outcomes[0], outcomes[1]);
        }
    }

    #[test]
    fn relocated_plan_publishes_atomically_through_both_memory_views() {
        let fragment = fragment(&[run_reloc(0x23, 1), run_reloc(0x25, 1)]);
        let plan = CfmFragmentPlan::prepare(
            &fragment,
            &[IMPORT],
            HEAP + 3,
            HEAP + 256,
            16,
            |cursor, size, alignment| {
                let (base, next) = bounds(cursor, size, alignment)?;
                // The allocator reserves [HEAP + 32, HEAP + 64) for system code.
                if base < HEAP + 64 && next > HEAP + 32 {
                    bounds(HEAP + 64, size, alignment)
                } else {
                    Some((base, next))
                }
            },
        )
        .unwrap();
        assert_eq!(plan.next_heap_cursor(), HEAP + 80);
        assert_eq!(plan.fragment.main_addr, HEAP + 64);
        assert_eq!((plan.fragment.init_addr, plan.fragment.term_addr), (0, 0));
        assert!(plan.fragment.exports.is_empty());
        for fault in 0..3 {
            let mut outcomes = Vec::new();
            for classic in [false, true] {
                let mut memory = GuestAddressSpace::new();
                memory.add_region(HEAP, vec![0xa5; if fault == 2 { 72 } else { 256 }]);
                if fault == 1 {
                    memory.add_readonly_region(HEAP + 72, vec![0xa5; 4]);
                }
                let before = read(&mut memory, HEAP, 256);
                let mut bus = MacMemoryBus::new(0x10000);
                bus.set_addressing_32_bit(true);
                bus.attach_guest_address_space(memory.shared_view());
                let published = if classic {
                    plan.publish(&mut bus)
                } else {
                    plan.publish(&mut memory)
                };
                let after = read(&mut memory, HEAP, 256);
                assert_eq!(published, fault == 0);
                if published {
                    let mut expected = vec![Some(0xa5); 256];
                    expected[16..24].fill(Some(0));
                    let relocated: Vec<_> = [HEAP + 16, HEAP + 64, IMPORT]
                        .into_iter()
                        .flat_map(u32::to_be_bytes)
                        .map(Some)
                        .collect();
                    expected[64..76].copy_from_slice(&relocated);
                    assert_eq!(after, expected);
                } else {
                    assert_eq!(after, before, "no section is published after a refusal");
                }
                if fault == 2 {
                    memory.add_region(HEAP + 72, vec![0xa5; 184]);
                    assert!(if classic {
                        plan.publish(&mut bus)
                    } else {
                        plan.publish(&mut memory)
                    });
                    assert_eq!(memory.procedure_read_u32(HEAP + 72), Some(IMPORT));
                }
                outcomes.push((published, after));
            }
            assert_eq!(outcomes[0], outcomes[1]);
        }
    }

    #[test]
    fn fragment_planning_rejects_invalid_layout_relocations_and_metadata() {
        for fault in 0..11 {
            let mut fragment = fragment(&[run_reloc(0x23, 1), run_reloc(0x25, 1)]);
            let mut imports = vec![IMPORT];
            let mut allocation_alignment = 16;
            match fault {
                0 => fragment[40 + 26] = 31,
                1 => fragment[0x80..0x84].copy_from_slice(&9u32.to_be_bytes()),
                2 => fragment[0x84..0x88].copy_from_slice(&8u32.to_be_bytes()),
                3 => fragment[0x80 + 52..0x80 + 56].copy_from_slice(&1u32.to_be_bytes()),
                4 => imports.clear(),
                5 => {
                    let malformed = [run_reloc(0x23, 1), run_reloc(0x25, 2)];
                    fragment = self::fragment(&malformed);
                }
                8 => allocation_alignment = 3,
                9 => fragment[8..12].copy_from_slice(b"m68k"),
                10 => fragment[12..16].copy_from_slice(&2u32.to_be_bytes()),
                _ => {}
            }
            let result = CfmFragmentPlan::prepare(
                &fragment,
                &imports,
                HEAP,
                HEAP + 256,
                allocation_alignment,
                |cursor, size, alignment| {
                    if fault == 6 {
                        return None;
                    }
                    if fault == 7 {
                        return Some((cursor - 1, cursor + size));
                    }
                    bounds(cursor, size, alignment)
                },
            );
            assert_eq!(
                result.err(),
                Some(if matches!(fault, 6..=8) {
                    CfmLoadError::NoAddressSpace
                } else {
                    CfmLoadError::CorruptFragment
                }),
                "fault {fault}"
            );
        }
        let fragment = fragment(&[run_reloc(0x23, 1), run_reloc(0x25, 1)]);
        assert_eq!(
            CfmFragmentPlan::prepare(&fragment, &[IMPORT], u32::MAX - 7, u32::MAX, 16, bounds)
                .err(),
            Some(CfmLoadError::NoAddressSpace)
        );
    }
}
