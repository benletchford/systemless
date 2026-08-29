//! Architecture-neutral Mixed Mode procedure-information decoding.
//!
//! The classic caller and native callee use different ABI adapters, but they
//! consume the same `ProcInfoType` contract. The layouts below follow Inside
//! Macintosh: PowerPC System Software (1994), pp. 2-14--2-20 and 2-27--2-32.

use crate::cpu::{CpuOps, Register};
use crate::error::{Error, Result};
use crate::guest_call::{
    GuestCallTarget, M68kResultTarget, PowerPcArguments, SharedGuestCallStack,
};
use crate::guest_procedure::{
    resolve_guest_procedure, GuestIsa, GuestProcedure, ROUTINE_FLAG_DONT_PASS_SELECTOR,
    ROUTINE_FLAG_USE_NATIVE_ISA,
};
use crate::memory::{MacMemoryBus, MemoryBus};

pub(crate) mod proc_info {
    pub(crate) const CALLING_CONVENTION_MASK: u32 = 0x0f;
    pub(crate) const PASCAL_STACK_BASED: u32 = 0;
    pub(crate) const C_STACK_BASED: u32 = 1;
    pub(crate) const REGISTER_BASED: u32 = 2;
    pub(crate) const THINK_C_STACK_BASED: u32 = 5;
    pub(crate) const D0_DISPATCHED_PASCAL_STACK_BASED: u32 = 8;
    pub(crate) const D0_DISPATCHED_C_STACK_BASED: u32 = 9;
    pub(crate) const D1_DISPATCHED_PASCAL_STACK_BASED: u32 = 12;
    pub(crate) const STACK_DISPATCHED_PASCAL_STACK_BASED: u32 = 14;
    pub(crate) const SPECIAL_CASE: u32 = 15;
    pub(crate) const RESULT_SIZE_PHASE: u32 = 4;
    pub(crate) const STACK_PARAMETER_PHASE: u32 = 6;
    pub(crate) const STACK_PARAMETER_WIDTH: u32 = 2;
    pub(crate) const DISPATCHED_SELECTOR_SIZE_PHASE: u32 = 6;
    pub(crate) const DISPATCHED_PARAMETER_PHASE: u32 = 8;
    pub(crate) const REGISTER_RESULT_LOCATION_PHASE: u32 = 6;
    pub(crate) const REGISTER_PARAMETER_PHASE: u32 = 11;
    pub(crate) const REGISTER_PARAMETER_WIDTH: u32 = 5;
    pub(crate) const REGISTER_PARAMETER_SIZE_MASK: u32 = 0x03;
    pub(crate) const REGISTER_PARAMETER_WHICH_SHIFT: u32 = 2;
    pub(crate) const REGISTER_PARAMETER_WHICH_MASK: u32 = 0x07;
    pub(crate) const REGISTER_CCR_C: u32 = 16;
    pub(crate) const REGISTER_CCR_V: u32 = 17;
    pub(crate) const REGISTER_CCR_Z: u32 = 18;
    pub(crate) const REGISTER_CCR_N: u32 = 19;
    pub(crate) const REGISTER_CCR_X: u32 = 20;
    pub(crate) const SIZE_NONE: u32 = 0;
    pub(crate) const SIZE_ONE: u32 = 1;
    pub(crate) const SIZE_TWO: u32 = 2;
    pub(crate) const SIZE_FOUR: u32 = 3;
    pub(crate) const MAX_STACK_PARAMETERS: usize = 13;
    pub(crate) const MAX_DISPATCHED_STACK_PARAMETERS: usize = 12;
    pub(crate) const MAX_REGISTER_PARAMETERS: usize = 4;
}

pub(crate) mod special_case {
    pub(crate) const HIGH_HOOK: u32 = 0;
    pub(crate) const EOL_HOOK: u32 = 1;
    pub(crate) const WIDTH_HOOK: u32 = 2;
    pub(crate) const NWIDTH_HOOK: u32 = 3;
    pub(crate) const DRAW_HOOK: u32 = 4;
    pub(crate) const HIT_TEST_HOOK: u32 = 5;
    pub(crate) const TE_FIND_WORD: u32 = 6;
    pub(crate) const PROTOCOL_HANDLER: u32 = 7;
    pub(crate) const SOCKET_LISTENER: u32 = 8;
    pub(crate) const TE_RECALC: u32 = 9;
    pub(crate) const TE_DO_TEXT: u32 = 10;
    pub(crate) const GNE_FILTER_PROC: u32 = 11;
    pub(crate) const MBAR_HOOK: u32 = 12;
    pub(crate) const SELECTOR_PHASE: u32 = 4;
    pub(crate) const SELECTOR_MASK: u32 = 0x3f;
    pub(crate) const ENCODED_MASK: u32 = 0x03ff;
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum NativeSpecialCaseResult {
    Void,
    Boolean,
    Word,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct NativeSpecialCaseSignature {
    pub(crate) argument_count: usize,
    pub(crate) result: NativeSpecialCaseResult,
}

/// Decode the native signature associated with a special-case ProcInfo.
///
/// The 68k side uses bespoke register and stack layouts, while Universal
/// Interfaces exposes ordinary native prototypes with output pointers where
/// the classic convention returns more than one register. The complete table
/// follows Inside Macintosh: PowerPC System Software (1994), pp. 2-30--2-32,
/// and Universal Interfaces 3.4 MixedMode.h, TextEdit.h, Events.h, Menus.h,
/// and AppleTalk.h.
pub(crate) fn native_special_case_signature(proc_info: u32) -> Option<NativeSpecialCaseSignature> {
    if convention(proc_info) != proc_info::SPECIAL_CASE
        || (proc_info & !special_case::ENCODED_MASK) != 0
    {
        return None;
    }
    let selector = (proc_info >> special_case::SELECTOR_PHASE) & special_case::SELECTOR_MASK;
    let (argument_count, result) = match selector {
        special_case::HIGH_HOOK => (2, NativeSpecialCaseResult::Void),
        special_case::EOL_HOOK => (3, NativeSpecialCaseResult::Boolean),
        special_case::WIDTH_HOOK => (5, NativeSpecialCaseResult::Word),
        special_case::NWIDTH_HOOK => (8, NativeSpecialCaseResult::Word),
        special_case::DRAW_HOOK => (5, NativeSpecialCaseResult::Void),
        special_case::HIT_TEST_HOOK => (9, NativeSpecialCaseResult::Boolean),
        special_case::TE_FIND_WORD => (6, NativeSpecialCaseResult::Void),
        special_case::PROTOCOL_HANDLER => (6, NativeSpecialCaseResult::Boolean),
        special_case::SOCKET_LISTENER => (7, NativeSpecialCaseResult::Boolean),
        special_case::TE_RECALC => (5, NativeSpecialCaseResult::Void),
        special_case::TE_DO_TEXT => (6, NativeSpecialCaseResult::Void),
        special_case::GNE_FILTER_PROC => (2, NativeSpecialCaseResult::Void),
        special_case::MBAR_HOOK => (1, NativeSpecialCaseResult::Word),
        _ => return None,
    };
    Some(NativeSpecialCaseSignature {
        argument_count,
        result,
    })
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ValueSize {
    One,
    Two,
    Four,
}

impl ValueSize {
    fn decode(code: u32) -> Option<Self> {
        match code {
            proc_info::SIZE_ONE => Some(Self::One),
            proc_info::SIZE_TWO => Some(Self::Two),
            proc_info::SIZE_FOUR => Some(Self::Four),
            _ => None,
        }
    }

    fn bytes(self) -> u8 {
        match self {
            Self::One => 1,
            Self::Two => 2,
            Self::Four => 4,
        }
    }

    fn stack_bytes(self) -> u32 {
        match self {
            Self::One | Self::Two => 2,
            Self::Four => 4,
        }
    }

    fn mask(self, value: u32) -> u32 {
        match self {
            Self::One => value & 0xff,
            Self::Two => value & 0xffff,
            Self::Four => value,
        }
    }
}

fn convention(proc_info: u32) -> u32 {
    proc_info & proc_info::CALLING_CONVENTION_MASK
}

fn result_size(proc_info: u32) -> Option<ValueSize> {
    ValueSize::decode((proc_info >> proc_info::RESULT_SIZE_PHASE) & 0x03)
}

fn stack_parameter_sizes(proc_info: u32, dispatched: bool) -> Option<Vec<ValueSize>> {
    let (phase, maximum) = if dispatched {
        (
            proc_info::DISPATCHED_PARAMETER_PHASE,
            proc_info::MAX_DISPATCHED_STACK_PARAMETERS,
        )
    } else {
        (
            proc_info::STACK_PARAMETER_PHASE,
            proc_info::MAX_STACK_PARAMETERS,
        )
    };
    let mut sizes = Vec::new();
    for index in 0..maximum {
        let shift = phase
            + u32::try_from(index)
                .ok()?
                .checked_mul(proc_info::STACK_PARAMETER_WIDTH)?;
        let code = (proc_info >> shift) & 0x03;
        if code == proc_info::SIZE_NONE {
            break;
        }
        sizes.push(ValueSize::decode(code)?);
    }
    Some(sizes)
}

fn selector_size(proc_info: u32) -> Option<Option<ValueSize>> {
    let code = (proc_info >> proc_info::DISPATCHED_SELECTOR_SIZE_PHASE) & 0x03;
    if code == proc_info::SIZE_NONE {
        Some(None)
    } else {
        Some(Some(ValueSize::decode(code)?))
    }
}

fn read_stack_value(bus: &MacMemoryBus, address: u32, size: ValueSize, think_c: bool) -> u32 {
    match size {
        ValueSize::One if think_c => u32::from(bus.read_byte(address)),
        ValueSize::One => u32::from(bus.read_byte(address.wrapping_add(1))),
        ValueSize::Two => u32::from(bus.read_word(address)),
        ValueSize::Four => bus.read_long(address),
    }
}

fn read_stack_arguments(
    bus: &MacMemoryBus,
    sp: u32,
    sizes: &[ValueSize],
    pascal: bool,
    think_c: bool,
) -> Option<(Vec<u32>, u32)> {
    let mut cursor = sp.checked_add(4)?;
    let mut values = Vec::with_capacity(sizes.len());
    if pascal {
        for size in sizes.iter().copied().rev() {
            values.push(read_stack_value(bus, cursor, size, false));
            cursor = cursor.checked_add(size.stack_bytes())?;
        }
        values.reverse();
    } else {
        for size in sizes.iter().copied() {
            values.push(read_stack_value(bus, cursor, size, think_c));
            cursor = cursor.checked_add(size.stack_bytes())?;
        }
    }
    Some((values, cursor.wrapping_sub(sp.wrapping_add(4))))
}

fn read_register(cpu: &impl CpuOps, encoded: u32) -> Option<u32> {
    let register = match encoded {
        0 => Register::D0,
        1 => Register::D1,
        2 => Register::D2,
        3 => Register::D3,
        4 => Register::A0,
        5 => Register::A1,
        6 => Register::A2,
        7 => Register::A3,
        _ => return None,
    };
    Some(cpu.read_reg(register))
}

fn choose_m68k_entry_target(
    bus: &mut MacMemoryBus,
    descriptor: u32,
    selector: Option<u32>,
) -> Option<GuestProcedure> {
    let current =
        resolve_guest_procedure(bus, descriptor, 0, selector, GuestIsa::M68k, GuestIsa::M68k)?;
    let native = resolve_guest_procedure(
        bus,
        descriptor,
        0,
        selector,
        GuestIsa::PowerPc,
        GuestIsa::M68k,
    );
    if let Some(native) = native {
        if native.isa == GuestIsa::PowerPc
            && (native.routine_flags & ROUTINE_FLAG_USE_NATIVE_ISA) != 0
        {
            return Some(native);
        }
    }
    Some(current)
}

fn descriptor_selector(
    cpu: &impl CpuOps,
    bus: &MacMemoryBus,
    sp: u32,
    proc_info: u32,
) -> Option<Option<u32>> {
    let convention = convention(proc_info);
    let Some(size) = selector_size(proc_info)? else {
        return Some(None);
    };
    let selector = match convention {
        proc_info::D0_DISPATCHED_PASCAL_STACK_BASED | proc_info::D0_DISPATCHED_C_STACK_BASED => {
            cpu.read_reg(Register::D0)
        }
        proc_info::D1_DISPATCHED_PASCAL_STACK_BASED => cpu.read_reg(Register::D1),
        proc_info::STACK_DISPATCHED_PASCAL_STACK_BASED => {
            let parameter_sizes = stack_parameter_sizes(proc_info, true)?;
            let parameter_bytes = parameter_sizes
                .iter()
                .try_fold(0u32, |total, size| total.checked_add(size.stack_bytes()))?;
            let address = sp.checked_add(4)?.checked_add(parameter_bytes)?;
            read_stack_value(bus, address, size, false)
        }
        _ => return Some(None),
    };
    Some(Some(size.mask(selector)))
}

fn register_result_target(proc_info: u32, size: Option<ValueSize>) -> Option<M68kResultTarget> {
    let encoded = (proc_info >> proc_info::REGISTER_RESULT_LOCATION_PHASE) & 0x1f;
    if let Some(size) = size {
        return match encoded {
            0..=3 => Some(M68kResultTarget::Data {
                index: u8::try_from(encoded).ok()?,
                size: size.bytes(),
            }),
            4..=7 => Some(M68kResultTarget::Address {
                index: u8::try_from(encoded - 4).ok()?,
                size: size.bytes(),
            }),
            8..=11 => Some(M68kResultTarget::Data {
                index: u8::try_from(encoded - 4).ok()?,
                size: size.bytes(),
            }),
            12..=14 => Some(M68kResultTarget::Address {
                index: u8::try_from(encoded - 8).ok()?,
                size: size.bytes(),
            }),
            _ => None,
        };
    }
    let mask = match encoded {
        proc_info::REGISTER_CCR_C => 0x01,
        proc_info::REGISTER_CCR_V => 0x02,
        proc_info::REGISTER_CCR_Z => 0x04,
        proc_info::REGISTER_CCR_N => 0x08,
        proc_info::REGISTER_CCR_X => 0x10,
        _ => return None,
    };
    Some(M68kResultTarget::Ccr { mask })
}

/// Handle the private `$AAFE` instruction at the start of a routine
/// descriptor. Returns only after either redirecting directly to a selected
/// 68k record or parking the classic caller for native execution.
pub(crate) fn enter_m68k_routine_descriptor(
    cpu: &mut impl CpuOps,
    bus: &mut MacMemoryBus,
    calls: &SharedGuestCallStack,
) -> Result<()> {
    let descriptor = cpu.read_reg(Register::PC).wrapping_sub(2);
    let sp = cpu.read_reg(Register::A7);
    let return_pc = bus.read_long(sp);
    let initial = choose_m68k_entry_target(bus, descriptor, None).ok_or(Error::Halted)?;
    let selector = descriptor_selector(cpu, bus, sp, initial.proc_info).ok_or(Error::Halted)?;
    let target = choose_m68k_entry_target(bus, descriptor, selector).ok_or(Error::Halted)?;

    if target.isa == GuestIsa::M68k {
        cpu.write_reg(Register::PC, target.entry);
        return Ok(());
    }

    let convention = convention(target.proc_info);
    if convention == proc_info::SPECIAL_CASE {
        return Err(Error::Halted);
    }
    let result_size = result_size(target.proc_info);
    let mut arguments = Vec::new();
    let mut result = None;
    let final_sp;

    match convention {
        proc_info::REGISTER_BASED => {
            for index in 0..proc_info::MAX_REGISTER_PARAMETERS {
                let shift = proc_info::REGISTER_PARAMETER_PHASE
                    + u32::try_from(index)
                        .map_err(|_| Error::Halted)?
                        .checked_mul(proc_info::REGISTER_PARAMETER_WIDTH)
                        .ok_or(Error::Halted)?;
                let field = (target.proc_info >> shift) & 0x1f;
                let size_code = field & proc_info::REGISTER_PARAMETER_SIZE_MASK;
                if size_code == proc_info::SIZE_NONE {
                    break;
                }
                let size = ValueSize::decode(size_code).ok_or(Error::Halted)?;
                let encoded = (field >> proc_info::REGISTER_PARAMETER_WHICH_SHIFT)
                    & proc_info::REGISTER_PARAMETER_WHICH_MASK;
                arguments.push(size.mask(read_register(cpu, encoded).ok_or(Error::Halted)?));
            }
            result = register_result_target(target.proc_info, result_size);
            if result_size.is_some() && result.is_none() {
                return Err(Error::Halted);
            }
            final_sp = sp.checked_add(4).ok_or(Error::Halted)?;
        }
        proc_info::PASCAL_STACK_BASED
        | proc_info::C_STACK_BASED
        | proc_info::THINK_C_STACK_BASED
        | proc_info::D0_DISPATCHED_PASCAL_STACK_BASED
        | proc_info::D0_DISPATCHED_C_STACK_BASED
        | proc_info::D1_DISPATCHED_PASCAL_STACK_BASED
        | proc_info::STACK_DISPATCHED_PASCAL_STACK_BASED => {
            let dispatched = matches!(
                convention,
                proc_info::D0_DISPATCHED_PASCAL_STACK_BASED
                    | proc_info::D0_DISPATCHED_C_STACK_BASED
                    | proc_info::D1_DISPATCHED_PASCAL_STACK_BASED
                    | proc_info::STACK_DISPATCHED_PASCAL_STACK_BASED
            );
            let pascal = matches!(
                convention,
                proc_info::PASCAL_STACK_BASED
                    | proc_info::D0_DISPATCHED_PASCAL_STACK_BASED
                    | proc_info::D1_DISPATCHED_PASCAL_STACK_BASED
                    | proc_info::STACK_DISPATCHED_PASCAL_STACK_BASED
            );
            let think_c = convention == proc_info::THINK_C_STACK_BASED;
            let sizes = stack_parameter_sizes(target.proc_info, dispatched).ok_or(Error::Halted)?;
            let (stack_arguments, argument_bytes) =
                read_stack_arguments(bus, sp, &sizes, pascal, think_c).ok_or(Error::Halted)?;

            let selector_bytes = if convention == proc_info::STACK_DISPATCHED_PASCAL_STACK_BASED {
                selector_size(target.proc_info)
                    .ok_or(Error::Halted)?
                    .map_or(0, ValueSize::stack_bytes)
            } else {
                0
            };

            if dispatched {
                let selector = selector.ok_or(Error::Halted)?;
                arguments.push(selector);
                arguments.extend(stack_arguments);
                if (target.routine_flags & ROUTINE_FLAG_DONT_PASS_SELECTOR) != 0 {
                    arguments.remove(0);
                }
            } else {
                arguments = stack_arguments;
            }

            if pascal {
                final_sp = sp
                    .checked_add(4)
                    .and_then(|address| address.checked_add(argument_bytes))
                    .and_then(|address| address.checked_add(selector_bytes))
                    .ok_or(Error::Halted)?;
                if let Some(size) = result_size {
                    let address = if size == ValueSize::One {
                        final_sp.checked_add(1).ok_or(Error::Halted)?
                    } else {
                        final_sp
                    };
                    result = Some(M68kResultTarget::Memory {
                        address,
                        size: size.bytes(),
                    });
                }
            } else {
                final_sp = sp.checked_add(4).ok_or(Error::Halted)?;
                if let Some(size) = result_size {
                    result = Some(M68kResultTarget::Data {
                        index: 0,
                        size: size.bytes(),
                    });
                }
            }
        }
        _ => return Err(Error::Halted),
    }

    let arguments = PowerPcArguments::from_slice(&arguments).ok_or(Error::Halted)?;
    let started = calls.begin_m68k_to_powerpc(
        GuestCallTarget {
            isa: target.isa,
            entry: target.entry,
            rtoc: target.rtoc,
        },
        arguments,
        return_pc,
        final_sp,
        result,
    );
    if !started {
        return Err(Error::Halted);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::guest_procedure::{
        ROUTINE_DESCRIPTOR_HEADER_SIZE, ROUTINE_DESCRIPTOR_MIXED_MODE_TRAP,
        ROUTINE_DESCRIPTOR_VERSION, ROUTINE_FLAG_DONT_PASS_SELECTOR, ROUTINE_FLAG_USE_NATIVE_ISA,
        ROUTINE_RECORD_FLAGS_OFFSET, ROUTINE_RECORD_ISA_OFFSET, ROUTINE_RECORD_M68K_ISA,
        ROUTINE_RECORD_POWERPC_ISA, ROUTINE_RECORD_PROC_DESCRIPTOR_OFFSET,
        ROUTINE_RECORD_SELECTOR_OFFSET,
    };
    use crate::trap::test_helpers::{setup, MockCpu, TEST_SP};
    use ppc::PpcCpu;

    const DESCRIPTOR: u32 = 0x0002_0000;
    const TVECTOR: u32 = 0x0002_1000;
    const POWERPC_ENTRY: u32 = 0x0002_2000;
    const POWERPC_RTOC: u32 = 0x0002_3000;
    const M68K_ENTRY: u32 = 0x0002_4000;
    const RETURN_PC: u32 = 0x0002_5000;
    const PPC_RETURN_PC: u32 = 0x01f0_4000;

    fn write_header(bus: &mut MacMemoryBus, last_record: u16) {
        bus.write_word(DESCRIPTOR, ROUTINE_DESCRIPTOR_MIXED_MODE_TRAP);
        bus.write_byte(DESCRIPTOR + 2, ROUTINE_DESCRIPTOR_VERSION);
        bus.write_word(DESCRIPTOR + 10, last_record);
    }

    fn write_record(
        bus: &mut MacMemoryBus,
        index: u32,
        isa: u8,
        flags: u16,
        proc_info: u32,
        procedure: u32,
        selector: u32,
    ) {
        let record = DESCRIPTOR + ROUTINE_DESCRIPTOR_HEADER_SIZE + index * 20;
        bus.write_long(record, proc_info);
        bus.write_byte(record + ROUTINE_RECORD_ISA_OFFSET, isa);
        bus.write_word(record + ROUTINE_RECORD_FLAGS_OFFSET, flags);
        bus.write_long(record + ROUTINE_RECORD_PROC_DESCRIPTOR_OFFSET, procedure);
        bus.write_long(record + ROUTINE_RECORD_SELECTOR_OFFSET, selector);
    }

    #[test]
    fn special_case_procinfo_uses_native_interface_signatures() {
        use NativeSpecialCaseResult::{Boolean, Void, Word};

        let cases = [
            (special_case::HIGH_HOOK, 2, Void),
            (special_case::EOL_HOOK, 3, Boolean),
            (special_case::WIDTH_HOOK, 5, Word),
            (special_case::NWIDTH_HOOK, 8, Word),
            (special_case::DRAW_HOOK, 5, Void),
            (special_case::HIT_TEST_HOOK, 9, Boolean),
            (special_case::TE_FIND_WORD, 6, Void),
            (special_case::PROTOCOL_HANDLER, 6, Boolean),
            (special_case::SOCKET_LISTENER, 7, Boolean),
            (special_case::TE_RECALC, 5, Void),
            (special_case::TE_DO_TEXT, 6, Void),
            (special_case::GNE_FILTER_PROC, 2, Void),
            (special_case::MBAR_HOOK, 1, Word),
        ];
        for (selector, argument_count, result) in cases {
            let proc_info = proc_info::SPECIAL_CASE | (selector << special_case::SELECTOR_PHASE);
            assert_eq!(
                native_special_case_signature(proc_info),
                Some(NativeSpecialCaseSignature {
                    argument_count,
                    result,
                })
            );
        }

        assert_eq!(
            native_special_case_signature(
                proc_info::SPECIAL_CASE | (13 << special_case::SELECTOR_PHASE)
            ),
            None
        );
        assert_eq!(
            native_special_case_signature(proc_info::SPECIAL_CASE | (1 << 10)),
            None
        );
        assert_eq!(
            native_special_case_signature(proc_info::PASCAL_STACK_BASED),
            None
        );
    }

    fn install_powerpc_target(bus: &mut MacMemoryBus) {
        bus.write_long(TVECTOR, POWERPC_ENTRY);
        bus.write_long(TVECTOR + 4, POWERPC_RTOC);
        bus.write_long(POWERPC_ENTRY, 0x4e80_0020);
    }

    fn enter(cpu: &mut MockCpu, bus: &mut MacMemoryBus, calls: &SharedGuestCallStack) {
        cpu.write_reg(Register::PC, DESCRIPTOR + 2);
        cpu.write_reg(Register::A7, TEST_SP);
        bus.write_long(TEST_SP, RETURN_PC);
        enter_m68k_routine_descriptor(cpu, bus, calls).unwrap();
    }

    fn stack_proc_info(convention: u32, result: u32, sizes: &[u32]) -> u32 {
        let mut value = convention | (result << proc_info::RESULT_SIZE_PHASE);
        let phase = if matches!(
            convention,
            proc_info::D0_DISPATCHED_PASCAL_STACK_BASED
                | proc_info::D0_DISPATCHED_C_STACK_BASED
                | proc_info::D1_DISPATCHED_PASCAL_STACK_BASED
                | proc_info::STACK_DISPATCHED_PASCAL_STACK_BASED
        ) {
            proc_info::DISPATCHED_PARAMETER_PHASE
        } else {
            proc_info::STACK_PARAMETER_PHASE
        };
        for (index, size) in sizes.iter().copied().enumerate() {
            value |=
                size << (phase + u32::try_from(index).unwrap() * proc_info::STACK_PARAMETER_WIDTH);
        }
        value
    }

    #[test]
    fn same_isa_descriptor_redirects_without_creating_a_transition() {
        let (_, mut cpu, mut bus) = setup();
        let calls = SharedGuestCallStack::default();
        write_header(&mut bus, 0);
        write_record(
            &mut bus,
            0,
            ROUTINE_RECORD_M68K_ISA,
            0,
            proc_info::PASCAL_STACK_BASED,
            M68K_ENTRY,
            0,
        );

        enter(&mut cpu, &mut bus, &calls);

        assert_eq!(cpu.read_reg(Register::PC), M68K_ENTRY);
        assert!(calls.is_empty());
    }

    #[test]
    fn native_record_marshals_pascal_arguments_and_result_slot() {
        let (_, mut cpu, mut bus) = setup();
        let calls = SharedGuestCallStack::default();
        let proc_info = stack_proc_info(
            proc_info::PASCAL_STACK_BASED,
            proc_info::SIZE_FOUR,
            &[
                proc_info::SIZE_ONE,
                proc_info::SIZE_TWO,
                proc_info::SIZE_FOUR,
            ],
        );
        write_header(&mut bus, 0);
        write_record(
            &mut bus,
            0,
            ROUTINE_RECORD_POWERPC_ISA,
            ROUTINE_FLAG_USE_NATIVE_ISA,
            proc_info,
            TVECTOR,
            0,
        );
        install_powerpc_target(&mut bus);
        bus.write_long(TEST_SP + 4, 0xcafe_babe);
        bus.write_word(TEST_SP + 8, 0x4321);
        bus.write_word(TEST_SP + 10, 0x0078);

        enter(&mut cpu, &mut bus, &calls);

        let pending = calls.pending_powerpc_from_m68k().unwrap();
        assert_eq!(pending.target.entry, POWERPC_ENTRY);
        assert_eq!(pending.target.rtoc, POWERPC_RTOC);
        assert_eq!(pending.arguments.as_slice(), &[0x78, 0x4321, 0xcafe_babe]);

        let mut ppc = PpcCpu::new();
        calls
            .activate_powerpc_from_m68k(&mut ppc, PPC_RETURN_PC)
            .unwrap();
        ppc.pc = PPC_RETURN_PC;
        ppc.gpr[3] = 0x1234_5678;
        assert!(calls.complete_powerpc_for_m68k(&mut ppc));
        let resume = calls.take_m68k_resume().unwrap();
        assert_eq!(resume.return_pc, RETURN_PC);
        assert_eq!(resume.final_sp, TEST_SP + 12);
        assert_eq!(
            resume.result,
            Some(M68kResultTarget::Memory {
                address: TEST_SP + 12,
                size: 4,
            })
        );
    }

    #[test]
    fn fat_descriptor_honors_use_native_isa_from_a_68k_caller() {
        let (_, mut cpu, mut bus) = setup();
        let calls = SharedGuestCallStack::default();
        write_header(&mut bus, 1);
        write_record(
            &mut bus,
            0,
            ROUTINE_RECORD_M68K_ISA,
            0,
            proc_info::PASCAL_STACK_BASED,
            M68K_ENTRY,
            0,
        );
        write_record(
            &mut bus,
            1,
            ROUTINE_RECORD_POWERPC_ISA,
            ROUTINE_FLAG_USE_NATIVE_ISA,
            proc_info::PASCAL_STACK_BASED,
            TVECTOR,
            0,
        );
        install_powerpc_target(&mut bus);

        enter(&mut cpu, &mut bus, &calls);

        assert_eq!(
            calls.pending_powerpc_from_m68k().unwrap().target.entry,
            POWERPC_ENTRY
        );
        assert_eq!(cpu.read_reg(Register::PC), DESCRIPTOR + 2);
    }

    #[test]
    fn stack_dispatch_pops_selector_and_can_omit_it_from_native_arguments() {
        for (flags, expected_arguments) in [
            (ROUTINE_FLAG_USE_NATIVE_ISA, vec![0x3456, 0x1234_5678]),
            (
                ROUTINE_FLAG_USE_NATIVE_ISA | ROUTINE_FLAG_DONT_PASS_SELECTOR,
                vec![0x1234_5678],
            ),
        ] {
            let (_, mut cpu, mut bus) = setup();
            let calls = SharedGuestCallStack::default();
            let proc_info = stack_proc_info(
                proc_info::STACK_DISPATCHED_PASCAL_STACK_BASED,
                proc_info::SIZE_NONE,
                &[proc_info::SIZE_FOUR],
            ) | (proc_info::SIZE_TWO << proc_info::DISPATCHED_SELECTOR_SIZE_PHASE);
            write_header(&mut bus, 0);
            write_record(
                &mut bus,
                0,
                ROUTINE_RECORD_POWERPC_ISA,
                flags,
                proc_info,
                TVECTOR,
                0x3456,
            );
            install_powerpc_target(&mut bus);
            bus.write_long(TEST_SP + 4, 0x1234_5678);
            bus.write_word(TEST_SP + 8, 0x3456);

            enter(&mut cpu, &mut bus, &calls);

            assert_eq!(
                calls
                    .pending_powerpc_from_m68k()
                    .unwrap()
                    .arguments
                    .as_slice(),
                expected_arguments
            );
            let mut ppc = PpcCpu::new();
            calls
                .activate_powerpc_from_m68k(&mut ppc, PPC_RETURN_PC)
                .unwrap();
            ppc.pc = PPC_RETURN_PC;
            assert!(calls.complete_powerpc_for_m68k(&mut ppc));
            assert_eq!(calls.take_m68k_resume().unwrap().final_sp, TEST_SP + 10);
        }
    }

    #[test]
    fn register_dispatched_conventions_read_d0_and_d1_selectors() {
        for (calling_convention, selector_register, expected_arguments, final_sp) in [
            (
                proc_info::D0_DISPATCHED_C_STACK_BASED,
                Register::D0,
                vec![0x3456, 0x78, 0xcafe_babe],
                TEST_SP + 4,
            ),
            (
                proc_info::D1_DISPATCHED_PASCAL_STACK_BASED,
                Register::D1,
                vec![0x3456, 0x5678, 0xcafe_babe],
                TEST_SP + 10,
            ),
        ] {
            let (_, mut cpu, mut bus) = setup();
            let calls = SharedGuestCallStack::default();
            let sizes = if calling_convention == proc_info::D0_DISPATCHED_C_STACK_BASED {
                [proc_info::SIZE_ONE, proc_info::SIZE_FOUR]
            } else {
                [proc_info::SIZE_TWO, proc_info::SIZE_FOUR]
            };
            let proc_info = stack_proc_info(calling_convention, proc_info::SIZE_NONE, &sizes)
                | (proc_info::SIZE_TWO << proc_info::DISPATCHED_SELECTOR_SIZE_PHASE);
            write_header(&mut bus, 0);
            write_record(
                &mut bus,
                0,
                ROUTINE_RECORD_POWERPC_ISA,
                ROUTINE_FLAG_USE_NATIVE_ISA,
                proc_info,
                TVECTOR,
                0x3456,
            );
            install_powerpc_target(&mut bus);
            cpu.write_reg(selector_register, 0xabcd_3456);
            if calling_convention == proc_info::D0_DISPATCHED_C_STACK_BASED {
                bus.write_word(TEST_SP + 4, 0x0078);
                bus.write_long(TEST_SP + 6, 0xcafe_babe);
            } else {
                bus.write_long(TEST_SP + 4, 0xcafe_babe);
                bus.write_word(TEST_SP + 8, 0x5678);
            }

            enter(&mut cpu, &mut bus, &calls);

            assert_eq!(
                calls
                    .pending_powerpc_from_m68k()
                    .unwrap()
                    .arguments
                    .as_slice(),
                expected_arguments
            );
            let mut ppc = PpcCpu::new();
            calls
                .activate_powerpc_from_m68k(&mut ppc, PPC_RETURN_PC)
                .unwrap();
            ppc.pc = PPC_RETURN_PC;
            assert!(calls.complete_powerpc_for_m68k(&mut ppc));
            assert_eq!(calls.take_m68k_resume().unwrap().final_sp, final_sp);
        }
    }

    #[test]
    fn c_variants_and_register_procinfo_preserve_documented_byte_placement() {
        let (_, _, mut bus) = setup();
        bus.write_word(TEST_SP + 4, 0x7812);
        let sizes = [ValueSize::One];
        assert_eq!(
            read_stack_arguments(&bus, TEST_SP, &sizes, false, false),
            Some((vec![0x12], 2))
        );
        assert_eq!(
            read_stack_arguments(&bus, TEST_SP, &sizes, false, true),
            Some((vec![0x78], 2))
        );

        let (_, mut cpu, mut bus) = setup();
        let calls = SharedGuestCallStack::default();
        let first = proc_info::SIZE_TWO | (1 << proc_info::REGISTER_PARAMETER_WHICH_SHIFT);
        let second = proc_info::SIZE_FOUR | (7 << proc_info::REGISTER_PARAMETER_WHICH_SHIFT);
        let proc_info = proc_info::REGISTER_BASED
            | (proc_info::SIZE_FOUR << proc_info::RESULT_SIZE_PHASE)
            | (4 << proc_info::REGISTER_RESULT_LOCATION_PHASE)
            | (first << proc_info::REGISTER_PARAMETER_PHASE)
            | (second
                << (proc_info::REGISTER_PARAMETER_PHASE + proc_info::REGISTER_PARAMETER_WIDTH));
        write_header(&mut bus, 0);
        write_record(
            &mut bus,
            0,
            ROUTINE_RECORD_POWERPC_ISA,
            ROUTINE_FLAG_USE_NATIVE_ISA,
            proc_info,
            TVECTOR,
            0,
        );
        install_powerpc_target(&mut bus);
        cpu.write_reg(Register::D1, 0x1234_5678);
        cpu.write_reg(Register::A3, 0xcafe_babe);

        enter(&mut cpu, &mut bus, &calls);

        assert_eq!(
            calls
                .pending_powerpc_from_m68k()
                .unwrap()
                .arguments
                .as_slice(),
            &[0x5678, 0xcafe_babe]
        );
        let mut ppc = PpcCpu::new();
        calls
            .activate_powerpc_from_m68k(&mut ppc, PPC_RETURN_PC)
            .unwrap();
        ppc.pc = PPC_RETURN_PC;
        assert!(calls.complete_powerpc_for_m68k(&mut ppc));
        assert_eq!(
            calls.take_m68k_resume().unwrap().result,
            Some(M68kResultTarget::Address { index: 0, size: 4 })
        );
    }

    #[test]
    fn register_procinfo_maps_a_ccr_result_destination() {
        let (_, mut cpu, mut bus) = setup();
        let calls = SharedGuestCallStack::default();
        let proc_info = proc_info::REGISTER_BASED
            | (proc_info::REGISTER_CCR_Z << proc_info::REGISTER_RESULT_LOCATION_PHASE);
        write_header(&mut bus, 0);
        write_record(
            &mut bus,
            0,
            ROUTINE_RECORD_POWERPC_ISA,
            ROUTINE_FLAG_USE_NATIVE_ISA,
            proc_info,
            TVECTOR,
            0,
        );
        install_powerpc_target(&mut bus);

        enter(&mut cpu, &mut bus, &calls);
        let mut ppc = PpcCpu::new();
        calls
            .activate_powerpc_from_m68k(&mut ppc, PPC_RETURN_PC)
            .unwrap();
        ppc.pc = PPC_RETURN_PC;
        ppc.gpr[3] = 1;
        assert!(calls.complete_powerpc_for_m68k(&mut ppc));
        assert_eq!(
            calls.take_m68k_resume().unwrap().result,
            Some(M68kResultTarget::Ccr { mask: 0x04 })
        );
    }
}
