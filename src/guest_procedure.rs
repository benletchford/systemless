//! Architecture-neutral callable guest-procedure resolution.
//!
//! A universal procedure pointer is either raw 68k code or a RoutineDescriptor
//! whose records identify the target ISA, calling convention, and procedure
//! descriptor. Inside Macintosh: PowerPC System Software (1994), pp. 1-15--1-17
//! and 2-4--2-12. Resolution belongs above either CPU adapter so a caller can
//! distinguish a same-ISA call from a required Mixed Mode transition before it
//! constructs an ABI frame.

use crate::memory::{GuestAddressSpace, MacMemoryBus, MemoryBus};
use ppc::PpcMemory;

pub(crate) const ROUTINE_DESCRIPTOR_MIXED_MODE_TRAP: u16 = 0xAAFE;
pub(crate) const ROUTINE_DESCRIPTOR_VERSION: u8 = 7;
pub(crate) const ROUTINE_DESCRIPTOR_HEADER_SIZE: u32 = 12;
pub(crate) const ROUTINE_RECORD_SIZE: u32 = 20;
pub(crate) const ROUTINE_RECORD_ISA_OFFSET: u32 = 5;
pub(crate) const ROUTINE_RECORD_FLAGS_OFFSET: u32 = 6;
pub(crate) const ROUTINE_RECORD_PROC_DESCRIPTOR_OFFSET: u32 = 8;
pub(crate) const ROUTINE_RECORD_SELECTOR_OFFSET: u32 = 16;
pub(crate) const ROUTINE_RECORD_M68K_ISA: u8 = 0;
pub(crate) const ROUTINE_RECORD_POWERPC_ISA: u8 = 1;
pub(crate) const ROUTINE_FLAG_PROC_DESCRIPTOR_RELATIVE: u16 = 0x0001;
pub(crate) const ROUTINE_FLAG_FRAGMENT_NEEDS_PREPARING: u16 = 0x0002;
pub(crate) const ROUTINE_FLAG_USE_NATIVE_ISA: u16 = 0x0004;
pub(crate) const ROUTINE_FLAG_DONT_PASS_SELECTOR: u16 = 0x0008;
pub(crate) const ROUTINE_FLAG_DISPATCHED_DEFAULT: u16 = 0x0010;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum GuestIsa {
    M68k,
    PowerPc,
}

impl GuestIsa {
    fn alternate(self) -> Self {
        match self {
            Self::M68k => Self::PowerPc,
            Self::PowerPc => Self::M68k,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum GuestProcedureRepresentation {
    RawCode,
    PowerPcTransitionVector { address: u32 },
    RoutineDescriptor { descriptor: u32, record: u32 },
}

/// One resolved callable target before either CPU adapter builds its frame.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct GuestProcedure {
    pub(crate) original_pointer: u32,
    pub(crate) representation: GuestProcedureRepresentation,
    pub(crate) isa: GuestIsa,
    pub(crate) entry: u32,
    pub(crate) rtoc: u32,
    pub(crate) proc_info: u32,
    pub(crate) routine_flags: u16,
}

impl GuestProcedure {
    pub(crate) fn raw_m68k(pointer: u32) -> Self {
        Self {
            original_pointer: pointer,
            representation: GuestProcedureRepresentation::RawCode,
            isa: GuestIsa::M68k,
            entry: pointer,
            rtoc: 0,
            proc_info: 0,
            routine_flags: 0,
        }
    }
}

pub(crate) trait GuestProcedureMemory {
    fn procedure_read_u8(&mut self, address: u32) -> Option<u8>;
    fn procedure_read_u16(&mut self, address: u32) -> Option<u16>;
    fn procedure_read_u32(&mut self, address: u32) -> Option<u32>;
}

impl GuestProcedureMemory for GuestAddressSpace {
    fn procedure_read_u8(&mut self, address: u32) -> Option<u8> {
        PpcMemory::read_u8(self, address)
    }

    fn procedure_read_u16(&mut self, address: u32) -> Option<u16> {
        address.checked_add(1)?;
        PpcMemory::read_u16_be(self, address)
    }

    fn procedure_read_u32(&mut self, address: u32) -> Option<u32> {
        address.checked_add(3)?;
        PpcMemory::read_u32_be(self, address)
    }
}

impl GuestProcedureMemory for MacMemoryBus {
    fn procedure_read_u8(&mut self, address: u32) -> Option<u8> {
        self.is_guest_address_mapped(address, 1)
            .then(|| self.read_byte(address))
    }

    fn procedure_read_u16(&mut self, address: u32) -> Option<u16> {
        address.checked_add(1)?;
        self.is_guest_address_mapped(address, 2)
            .then(|| self.read_word(address))
    }

    fn procedure_read_u32(&mut self, address: u32) -> Option<u32> {
        address.checked_add(3)?;
        self.try_read_long(address)
    }
}

/// Validate the whole structure, including reserved bytes, before interpreting
/// it. A missing later record must not turn an incomplete descriptor into a
/// callable earlier candidate. PowerPC System Software (1994), pp. 2-36–2-38.
fn read_procedure_structure<const N: usize>(
    memory: &mut impl GuestProcedureMemory,
    address: u32,
) -> Option<[u8; N]> {
    let mut bytes = [0; N];
    for (offset, byte) in bytes.iter_mut().enumerate() {
        *byte = memory.procedure_read_u8(address.checked_add(u32::try_from(offset).ok()?)?)?;
    }
    Some(bytes)
}

#[derive(Default)]
struct RoutineCandidates {
    first: Option<GuestProcedure>,
    selected: Option<GuestProcedure>,
    default: Option<GuestProcedure>,
}

impl RoutineCandidates {
    fn finish(self, has_selector: bool) -> Option<GuestProcedure> {
        if has_selector {
            self.selected.or(self.default).or(self.first)
        } else {
            self.first
        }
    }
}

/// Resolve a universal or same-ISA procedure pointer.
///
/// RoutineDescriptors prefer a record for `preferred_isa`, then select the
/// other ISA when a mode switch is required. A non-descriptor pointer uses
/// `raw_isa`; PowerPC raw pointers retain the native transition-vector probe,
/// while a raw universal procedure pointer for 68k is direct code.
pub(crate) fn resolve_guest_procedure(
    memory: &mut impl GuestProcedureMemory,
    pointer: u32,
    default_powerpc_rtoc: u32,
    selector: Option<u32>,
    preferred_isa: GuestIsa,
    raw_isa: GuestIsa,
) -> Option<GuestProcedure> {
    if pointer == 0 {
        return None;
    }
    // A recognized descriptor with an unsupported version is malformed, not
    // raw code. The complete header is checked by the descriptor decoder.
    if memory.procedure_read_u16(pointer) == Some(ROUTINE_DESCRIPTOR_MIXED_MODE_TRAP) {
        return resolve_routine_descriptor(
            memory,
            pointer,
            default_powerpc_rtoc,
            selector,
            preferred_isa,
        );
    }
    Some(match raw_isa {
        GuestIsa::M68k => GuestProcedure::raw_m68k(pointer),
        GuestIsa::PowerPc => resolve_powerpc_pointer(memory, pointer, default_powerpc_rtoc, false),
    })
}

fn resolve_routine_descriptor(
    memory: &mut impl GuestProcedureMemory,
    descriptor: u32,
    default_powerpc_rtoc: u32,
    selector: Option<u32>,
    preferred_isa: GuestIsa,
) -> Option<GuestProcedure> {
    let header = read_procedure_structure::<12>(memory, descriptor)?;
    if header[2] != ROUTINE_DESCRIPTOR_VERSION {
        return None;
    }
    let routine_count = i16::from_be_bytes([header[10], header[11]]);
    if routine_count < 0 {
        return None;
    }
    let mut preferred = RoutineCandidates::default();
    let mut alternate = RoutineCandidates::default();
    for index in 0..=u32::from(routine_count as u16) {
        let record = index
            .checked_mul(ROUTINE_RECORD_SIZE)
            .and_then(|offset| descriptor.checked_add(ROUTINE_DESCRIPTOR_HEADER_SIZE + offset))?;
        let bytes = read_procedure_structure::<20>(memory, record)?;
        let isa = match bytes[ROUTINE_RECORD_ISA_OFFSET as usize] {
            ROUTINE_RECORD_M68K_ISA => GuestIsa::M68k,
            ROUTINE_RECORD_POWERPC_ISA => GuestIsa::PowerPc,
            _ => continue,
        };
        let proc_info = u32::from_be_bytes(bytes[0..4].try_into().ok()?);
        let flags_offset = ROUTINE_RECORD_FLAGS_OFFSET as usize;
        let routine_flags =
            u16::from_be_bytes(bytes[flags_offset..flags_offset + 2].try_into().ok()?);
        let proc_offset = ROUTINE_RECORD_PROC_DESCRIPTOR_OFFSET as usize;
        let mut proc_descriptor =
            u32::from_be_bytes(bytes[proc_offset..proc_offset + 4].try_into().ok()?);
        if proc_descriptor == 0 {
            continue;
        }
        if (routine_flags & ROUTINE_FLAG_PROC_DESCRIPTOR_RELATIVE) != 0 {
            let Some(relative_proc_descriptor) = descriptor.checked_add(proc_descriptor) else {
                continue;
            };
            proc_descriptor = relative_proc_descriptor;
        }
        let mut procedure = match isa {
            GuestIsa::M68k => GuestProcedure::raw_m68k(proc_descriptor),
            GuestIsa::PowerPc => resolve_powerpc_pointer(
                memory,
                proc_descriptor,
                default_powerpc_rtoc,
                (routine_flags
                    & (ROUTINE_FLAG_FRAGMENT_NEEDS_PREPARING
                        | ROUTINE_FLAG_PROC_DESCRIPTOR_RELATIVE))
                    == 0,
            ),
        };
        procedure.original_pointer = descriptor;
        procedure.representation =
            GuestProcedureRepresentation::RoutineDescriptor { descriptor, record };
        procedure.proc_info = proc_info;
        procedure.routine_flags = routine_flags;

        let candidates = if isa == preferred_isa {
            &mut preferred
        } else if isa == preferred_isa.alternate() {
            &mut alternate
        } else {
            continue;
        };
        candidates.first.get_or_insert(procedure);
        if let Some(selector) = selector {
            let selector_offset = ROUTINE_RECORD_SELECTOR_OFFSET as usize;
            let record_selector = u32::from_be_bytes(
                bytes[selector_offset..selector_offset + 4]
                    .try_into()
                    .ok()?,
            );
            if record_selector == selector {
                candidates.selected.get_or_insert(procedure);
            }
            if (routine_flags & ROUTINE_FLAG_DISPATCHED_DEFAULT) != 0 {
                candidates.default.get_or_insert(procedure);
            }
        }
    }
    preferred
        .finish(selector.is_some())
        .or_else(|| alternate.finish(selector.is_some()))
}

fn resolve_powerpc_pointer(
    memory: &mut impl GuestProcedureMemory,
    pointer: u32,
    default_rtoc: u32,
    declared_transition_vector: bool,
) -> GuestProcedure {
    if let (Some(entry), Some(rtoc)) = (
        memory.procedure_read_u32(pointer),
        pointer
            .checked_add(4)
            .and_then(|address| memory.procedure_read_u32(address)),
    ) {
        // An absolute, prepared PowerPC record identifies its transition vector
        // (PowerPC System Software, p. 2-36). Its code can belong to a staged
        // companion not yet attached to this view; execution validates that
        // mapping after preparation. Untyped pointers still need the mapped
        // entry probe to distinguish a vector from raw instruction bytes.
        if entry != 0 && (declared_transition_vector || memory.procedure_read_u32(entry).is_some())
        {
            return GuestProcedure {
                original_pointer: pointer,
                representation: GuestProcedureRepresentation::PowerPcTransitionVector {
                    address: pointer,
                },
                isa: GuestIsa::PowerPc,
                entry,
                rtoc,
                proc_info: 0,
                routine_flags: 0,
            };
        }
    }
    GuestProcedure {
        original_pointer: pointer,
        representation: GuestProcedureRepresentation::RawCode,
        isa: GuestIsa::PowerPc,
        entry: pointer,
        rtoc: default_rtoc,
        proc_info: 0,
        routine_flags: 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const BASE: u32 = 0x0010_0000;

    fn descriptor_memory() -> GuestAddressSpace {
        let mut memory = GuestAddressSpace::new();
        memory.add_region(BASE, vec![0; 0x400]);
        memory
    }

    fn write_descriptor_header(memory: &mut GuestAddressSpace, last_record: u16) {
        memory
            .write_u16_be(BASE, ROUTINE_DESCRIPTOR_MIXED_MODE_TRAP)
            .unwrap();
        memory
            .write_u8(BASE + 2, ROUTINE_DESCRIPTOR_VERSION)
            .unwrap();
        memory.write_u16_be(BASE + 10, last_record).unwrap();
    }

    fn write_record(
        memory: &mut GuestAddressSpace,
        index: u32,
        isa: u8,
        flags: u16,
        proc_info: u32,
        procedure: u32,
        selector: u32,
    ) {
        let record = BASE + ROUTINE_DESCRIPTOR_HEADER_SIZE + index * ROUTINE_RECORD_SIZE;
        memory.write_u32_be(record, proc_info).unwrap();
        memory
            .write_u8(record + ROUTINE_RECORD_ISA_OFFSET, isa)
            .unwrap();
        memory
            .write_u16_be(record + ROUTINE_RECORD_FLAGS_OFFSET, flags)
            .unwrap();
        memory
            .write_u32_be(record + ROUTINE_RECORD_PROC_DESCRIPTOR_OFFSET, procedure)
            .unwrap();
        memory
            .write_u32_be(record + ROUTINE_RECORD_SELECTOR_OFFSET, selector)
            .unwrap();
    }

    fn resolve_both_views(
        memory: &mut GuestAddressSpace,
        pointer: u32,
        selector: Option<u32>,
        isa: GuestIsa,
    ) -> [Option<GuestProcedure>; 2] {
        let mut classic = MacMemoryBus::new(0x10000);
        classic.set_addressing_32_bit(true);
        classic.attach_guest_address_space(memory.shared_view());
        [
            resolve_guest_procedure(memory, pointer, 0x1234, selector, isa, isa),
            resolve_guest_procedure(&mut classic, pointer, 0x1234, selector, isa, isa),
        ]
    }

    #[test]
    fn unmapped_instruction_words_are_not_transition_vectors_in_either_view() {
        let mut memory = GuestAddressSpace::new();
        memory.add_region(
            BASE,
            [0x3860_0000u32.to_be_bytes(), 0x4e80_0020u32.to_be_bytes()].concat(),
        );
        let results = resolve_both_views(&mut memory, BASE, None, GuestIsa::PowerPc);
        assert_eq!(results[0], results[1]);
        let procedure = results[0].unwrap();
        assert_eq!(procedure.entry, BASE);
        assert_eq!(procedure.rtoc, 0x1234);
        assert_eq!(
            procedure.representation,
            GuestProcedureRepresentation::RawCode
        );
    }

    #[test]
    fn malformed_descriptors_never_publish_partial_candidates_in_either_view() {
        let mut complete = descriptor_memory();
        write_descriptor_header(&mut complete, 0);
        write_record(
            &mut complete,
            0,
            ROUTINE_RECORD_M68K_ISA,
            0,
            0,
            BASE + 0x100,
            7,
        );
        let bytes: Vec<u8> = (0..32)
            .map(|i| complete.read_u8(BASE + i).unwrap())
            .collect();
        for len in 2..32 {
            let mut memory = GuestAddressSpace::new();
            memory.add_region(BASE, bytes[..len].to_vec());
            assert_eq!(
                resolve_both_views(&mut memory, BASE, Some(7), GuestIsa::M68k),
                [None, None],
                "truncated at byte {len}"
            );
        }
        for gap in [3usize, 4, 8, 9, 16, 24, 28] {
            let mut memory = GuestAddressSpace::new();
            memory.add_region(BASE, bytes[..gap].to_vec());
            memory.add_region(BASE + gap as u32 + 1, bytes[gap + 1..].to_vec());
            assert_eq!(
                resolve_both_views(&mut memory, BASE, None, GuestIsa::M68k),
                [None, None],
                "hole at byte {gap}"
            );
        }
        for version in [0, 6, 8, 255] {
            let mut memory = GuestAddressSpace::new();
            let mut data = bytes.clone();
            data[2] = version;
            memory.add_region(BASE, data);
            assert_eq!(
                resolve_both_views(&mut memory, BASE, None, GuestIsa::M68k),
                [None, None],
                "unsupported version {version}"
            );
        }
        let mut memory = GuestAddressSpace::new();
        let mut data = bytes;
        data[11] = 1; // Declares a second record that is absent.
        memory.add_region(BASE, data);
        assert_eq!(
            resolve_both_views(&mut memory, BASE, Some(7), GuestIsa::M68k),
            [None, None]
        );
    }

    #[test]
    fn split_readonly_descriptors_preserve_relative_and_native_targets_in_both_views() {
        let mut complete = descriptor_memory();
        write_descriptor_header(&mut complete, 1);
        write_record(
            &mut complete,
            0,
            ROUTINE_RECORD_M68K_ISA,
            ROUTINE_FLAG_PROC_DESCRIPTOR_RELATIVE,
            0x1111,
            0x100,
            7,
        );
        write_record(
            &mut complete,
            1,
            ROUTINE_RECORD_POWERPC_ISA,
            ROUTINE_FLAG_USE_NATIVE_ISA,
            0x2222,
            BASE + 0x200,
            8,
        );
        let bytes: Vec<u8> = (0..52)
            .map(|i| complete.read_u8(BASE + i).unwrap())
            .collect();
        for split in 1..bytes.len() {
            let mut memory = GuestAddressSpace::new();
            memory.add_readonly_region(BASE, bytes[..split].to_vec());
            memory.add_readonly_region(BASE + split as u32, bytes[split..].to_vec());
            memory.add_readonly_region(BASE + 0x100, vec![0x4e, 0x75]);
            memory.add_readonly_region(
                BASE + 0x200,
                [(BASE + 0x300).to_be_bytes(), (BASE + 0x3a0).to_be_bytes()].concat(),
            );
            memory.add_readonly_region(BASE + 0x300, 0x4e80_0020u32.to_be_bytes().to_vec());
            for isa in [GuestIsa::M68k, GuestIsa::PowerPc] {
                let results = resolve_both_views(&mut memory, BASE, None, isa);
                assert_eq!(results[0], results[1], "split at {split}, {isa:?}");
                let procedure = results[0].unwrap();
                assert_eq!(procedure.isa, isa);
                let (entry, rtoc, proc_info) = if isa == GuestIsa::M68k {
                    (BASE + 0x100, 0, 0x1111)
                } else {
                    (BASE + 0x300, BASE + 0x3a0, 0x2222)
                };
                assert_eq!(
                    (procedure.entry, procedure.rtoc, procedure.proc_info),
                    (entry, rtoc, proc_info)
                );
            }
        }
    }

    #[test]
    fn relative_native_records_resolve_raw_code_and_vectors_consistently() {
        for vector in [false, true] {
            let mut memory = descriptor_memory();
            write_descriptor_header(&mut memory, 0);
            write_record(
                &mut memory,
                0,
                ROUTINE_RECORD_POWERPC_ISA,
                ROUTINE_FLAG_USE_NATIVE_ISA | ROUTINE_FLAG_PROC_DESCRIPTOR_RELATIVE,
                0,
                0x200,
                0,
            );
            let (entry, rtoc) = if vector {
                memory.write_u32_be(BASE + 0x200, BASE + 0x300).unwrap();
                memory.write_u32_be(BASE + 0x204, BASE + 0x380).unwrap();
                memory.write_u32_be(BASE + 0x300, 0x4e80_0020).unwrap();
                (BASE + 0x300, BASE + 0x380)
            } else {
                memory.write_u32_be(BASE + 0x200, 0x4e80_0020).unwrap();
                (BASE + 0x200, 0x1234)
            };
            let results = resolve_both_views(&mut memory, BASE, None, GuestIsa::PowerPc);
            assert_eq!(results[0], results[1]);
            let procedure = results[0].unwrap();
            assert_eq!((procedure.entry, procedure.rtoc), (entry, rtoc));
        }
    }

    #[test]
    fn declared_native_vector_does_not_require_code_in_the_decoding_view() {
        let mut memory = descriptor_memory();
        write_descriptor_header(&mut memory, 0);
        write_record(
            &mut memory,
            0,
            ROUTINE_RECORD_POWERPC_ISA,
            ROUTINE_FLAG_USE_NATIVE_ISA,
            0x1111,
            BASE + 0x200,
            0,
        );
        let entry = 0x2000_1000;
        memory.write_u32_be(BASE + 0x200, entry).unwrap();
        memory.write_u32_be(BASE + 0x204, 0x2000_2000).unwrap();
        assert_eq!(memory.read_u32_be(entry), None);
        let results = resolve_both_views(&mut memory, BASE, None, GuestIsa::PowerPc);
        assert_eq!(results[0], results[1]);
        let procedure = results[0].unwrap();
        assert_eq!((procedure.entry, procedure.rtoc), (entry, 0x2000_2000));
    }

    #[test]
    fn procedure_probes_never_wrap_into_low_memory() {
        let mut memory = GuestAddressSpace::new();
        memory.add_region(0, vec![7, 0x12, 0x34, 0x56]);
        memory.add_region(u32::MAX - 1, vec![0xaa, 0xfe]);
        let mut classic = MacMemoryBus::new(0x10000);
        classic.set_addressing_32_bit(true);
        classic.write_byte(0, 7);
        classic.attach_guest_address_space(memory.shared_view());
        assert_eq!(
            resolve_guest_procedure(
                &mut memory,
                u32::MAX - 1,
                0,
                None,
                GuestIsa::M68k,
                GuestIsa::M68k
            ),
            None
        );
        assert_eq!(
            resolve_guest_procedure(
                &mut classic,
                u32::MAX - 1,
                0,
                None,
                GuestIsa::M68k,
                GuestIsa::M68k
            ),
            None
        );
        for address in [u32::MAX - 2, u32::MAX - 1, u32::MAX] {
            assert_eq!(memory.procedure_read_u32(address), None);
            assert_eq!(classic.procedure_read_u32(address), None);
        }
        assert_eq!(memory.procedure_read_u16(u32::MAX), None);
        assert_eq!(classic.procedure_read_u16(u32::MAX), None);

        let mut memory = GuestAddressSpace::new();
        memory.add_region(0, 0x1234u32.to_be_bytes().to_vec());
        memory.add_region(BASE, 0x4e80_0020u32.to_be_bytes().to_vec());
        memory.add_region(u32::MAX - 3, BASE.to_be_bytes().to_vec());
        let results = resolve_both_views(&mut memory, u32::MAX - 3, None, GuestIsa::PowerPc);
        for result in results {
            let procedure = result.unwrap();
            assert_eq!(procedure.entry, u32::MAX - 3);
            assert_eq!(
                procedure.representation,
                GuestProcedureRepresentation::RawCode
            );
        }
    }

    #[test]
    fn fat_descriptor_prefers_the_callers_isa_and_retains_both_targets() {
        let mut memory = descriptor_memory();
        let m68k_entry = BASE + 0x180;
        let tvector = BASE + 0x1c0;
        let powerpc_entry = BASE + 0x200;
        let rtoc = BASE + 0x240;
        write_descriptor_header(&mut memory, 1);
        write_record(
            &mut memory,
            0,
            ROUTINE_RECORD_M68K_ISA,
            0,
            0x1111,
            m68k_entry,
            0,
        );
        write_record(
            &mut memory,
            1,
            ROUTINE_RECORD_POWERPC_ISA,
            ROUTINE_FLAG_USE_NATIVE_ISA,
            0x2222,
            tvector,
            0,
        );
        memory.write_u32_be(tvector, powerpc_entry).unwrap();
        memory.write_u32_be(tvector + 4, rtoc).unwrap();
        memory.write_u32_be(powerpc_entry, 0x4e80_0020).unwrap();

        let native = resolve_guest_procedure(
            &mut memory,
            BASE,
            0,
            None,
            GuestIsa::PowerPc,
            GuestIsa::M68k,
        )
        .unwrap();
        let classic =
            resolve_guest_procedure(&mut memory, BASE, 0, None, GuestIsa::M68k, GuestIsa::M68k)
                .unwrap();

        assert_eq!(
            (native.isa, native.entry, native.rtoc),
            (GuestIsa::PowerPc, powerpc_entry, rtoc)
        );
        assert_eq!((classic.isa, classic.entry), (GuestIsa::M68k, m68k_entry));
        assert_eq!(native.original_pointer, BASE);
        assert_eq!(classic.original_pointer, BASE);
    }

    #[test]
    fn descriptor_uses_other_isa_when_no_same_isa_record_exists() {
        let mut memory = descriptor_memory();
        let m68k_entry = BASE + 0x180;
        write_descriptor_header(&mut memory, 0);
        write_record(
            &mut memory,
            0,
            ROUTINE_RECORD_M68K_ISA,
            0,
            0x1234,
            m68k_entry,
            0,
        );

        let procedure = resolve_guest_procedure(
            &mut memory,
            BASE,
            BASE + 0x300,
            None,
            GuestIsa::PowerPc,
            GuestIsa::PowerPc,
        )
        .unwrap();

        assert_eq!(procedure.isa, GuestIsa::M68k);
        assert_eq!(procedure.entry, m68k_entry);
        assert_eq!(procedure.proc_info, 0x1234);
    }

    #[test]
    fn selector_resolution_prefers_exact_then_default_within_the_selected_isa() {
        let mut memory = descriptor_memory();
        write_descriptor_header(&mut memory, 1);
        write_record(
            &mut memory,
            0,
            ROUTINE_RECORD_M68K_ISA,
            ROUTINE_FLAG_DISPATCHED_DEFAULT,
            0x1111,
            BASE + 0x180,
            0x10,
        );
        write_record(
            &mut memory,
            1,
            ROUTINE_RECORD_M68K_ISA,
            0,
            0x2222,
            BASE + 0x1a0,
            0x20,
        );

        let exact = resolve_guest_procedure(
            &mut memory,
            BASE,
            0,
            Some(0x20),
            GuestIsa::M68k,
            GuestIsa::M68k,
        )
        .unwrap();
        let default = resolve_guest_procedure(
            &mut memory,
            BASE,
            0,
            Some(0x30),
            GuestIsa::M68k,
            GuestIsa::M68k,
        )
        .unwrap();

        assert_eq!(exact.entry, BASE + 0x1a0);
        assert_eq!(default.entry, BASE + 0x180);
    }

    #[test]
    fn selector_resolution_keeps_the_first_matching_record() {
        let mut memory = descriptor_memory();
        write_descriptor_header(&mut memory, 1);
        write_record(
            &mut memory,
            0,
            ROUTINE_RECORD_POWERPC_ISA,
            ROUTINE_FLAG_USE_NATIVE_ISA,
            0x1111,
            BASE + 0x180,
            0x20,
        );
        write_record(
            &mut memory,
            1,
            ROUTINE_RECORD_POWERPC_ISA,
            ROUTINE_FLAG_USE_NATIVE_ISA,
            0x2222,
            BASE + 0x1a0,
            0x20,
        );

        let procedure = resolve_guest_procedure(
            &mut memory,
            BASE,
            0,
            Some(0x20),
            GuestIsa::PowerPc,
            GuestIsa::PowerPc,
        )
        .unwrap();

        assert_eq!(procedure.entry, BASE + 0x180);
        assert_eq!(procedure.proc_info, 0x1111);
    }

    #[test]
    fn raw_universal_pointer_is_classified_by_the_call_site() {
        let mut memory = descriptor_memory();
        let pointer = BASE + 0x180;

        let classic = resolve_guest_procedure(
            &mut memory,
            pointer,
            BASE + 0x300,
            None,
            GuestIsa::PowerPc,
            GuestIsa::M68k,
        )
        .unwrap();
        let native = resolve_guest_procedure(
            &mut memory,
            pointer,
            BASE + 0x300,
            None,
            GuestIsa::PowerPc,
            GuestIsa::PowerPc,
        )
        .unwrap();

        assert_eq!(classic.isa, GuestIsa::M68k);
        assert_eq!(native.isa, GuestIsa::PowerPc);
    }
}
