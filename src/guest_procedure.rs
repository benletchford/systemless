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
        PpcMemory::read_u16_be(self, address)
    }

    fn procedure_read_u32(&mut self, address: u32) -> Option<u32> {
        PpcMemory::read_u32_be(self, address)
    }
}

impl GuestProcedureMemory for MacMemoryBus {
    fn procedure_read_u8(&mut self, address: u32) -> Option<u8> {
        // A native process's 68k compatibility context can resolve UPPs in
        // the attached shared address space above flat 68k RAM. The normal
        // bus read performs that translation and returns zero for an
        // unmapped address, which fails descriptor validation safely.
        Some(self.read_byte(address))
    }

    fn procedure_read_u16(&mut self, address: u32) -> Option<u16> {
        address.checked_add(2).map(|_| self.read_word(address))
    }

    fn procedure_read_u32(&mut self, address: u32) -> Option<u32> {
        address.checked_add(4).map(|_| self.read_long(address))
    }
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

pub(crate) fn is_routine_descriptor(
    memory: &mut impl GuestProcedureMemory,
    descriptor: u32,
) -> bool {
    memory.procedure_read_u16(descriptor) == Some(ROUTINE_DESCRIPTOR_MIXED_MODE_TRAP)
        && memory.procedure_read_u8(descriptor.wrapping_add(2)) == Some(ROUTINE_DESCRIPTOR_VERSION)
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
    if is_routine_descriptor(memory, pointer) {
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
        GuestIsa::PowerPc => resolve_powerpc_pointer(memory, pointer, default_powerpc_rtoc),
    })
}

fn resolve_routine_descriptor(
    memory: &mut impl GuestProcedureMemory,
    descriptor: u32,
    default_powerpc_rtoc: u32,
    selector: Option<u32>,
    preferred_isa: GuestIsa,
) -> Option<GuestProcedure> {
    let routine_count = memory.procedure_read_u16(descriptor.checked_add(10)?)? as i16;
    if routine_count < 0 {
        return None;
    }
    let mut preferred = RoutineCandidates::default();
    let mut alternate = RoutineCandidates::default();
    for index in 0..=u32::from(routine_count as u16) {
        let Some(record) = index
            .checked_mul(ROUTINE_RECORD_SIZE)
            .and_then(|offset| descriptor.checked_add(ROUTINE_DESCRIPTOR_HEADER_SIZE + offset))
        else {
            break;
        };
        let Some(isa_address) = record.checked_add(ROUTINE_RECORD_ISA_OFFSET) else {
            break;
        };
        let Some(isa_byte) = memory.procedure_read_u8(isa_address) else {
            break;
        };
        let isa = match isa_byte {
            ROUTINE_RECORD_M68K_ISA => GuestIsa::M68k,
            ROUTINE_RECORD_POWERPC_ISA => GuestIsa::PowerPc,
            _ => continue,
        };
        let Some(proc_info) = memory.procedure_read_u32(record) else {
            break;
        };
        let Some(flags_address) = record.checked_add(ROUTINE_RECORD_FLAGS_OFFSET) else {
            break;
        };
        let Some(routine_flags) = memory.procedure_read_u16(flags_address) else {
            break;
        };
        let Some(proc_descriptor_address) =
            record.checked_add(ROUTINE_RECORD_PROC_DESCRIPTOR_OFFSET)
        else {
            break;
        };
        let Some(mut proc_descriptor) = memory.procedure_read_u32(proc_descriptor_address) else {
            break;
        };
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
            GuestIsa::PowerPc => {
                resolve_powerpc_pointer(memory, proc_descriptor, default_powerpc_rtoc)
            }
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
            let record_selector = record
                .checked_add(ROUTINE_RECORD_SELECTOR_OFFSET)
                .and_then(|address| memory.procedure_read_u32(address));
            if record_selector == Some(selector) {
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
) -> GuestProcedure {
    if let (Some(entry), Some(rtoc)) = (
        memory.procedure_read_u32(pointer),
        memory.procedure_read_u32(pointer.wrapping_add(4)),
    ) {
        if entry != 0 && memory.procedure_read_u32(entry).is_some() {
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
