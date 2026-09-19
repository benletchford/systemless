//! Typed Toolbox Utilities dispatch for PowerPC imports.

use super::*;

pub(super) struct PpcToolboxDispatchContext<'a> {
    pub(super) binding: &'a PpcImportBinding,
    pub(super) cpu: &'a mut PpcCpu,
    pub(super) memory: &'a mut PpcSectionMem,
}

pub(super) fn dispatch_toolbox_import(
    context: PpcToolboxDispatchContext<'_>,
) -> Option<PpcImportAction> {
    let PpcToolboxDispatchContext {
        binding,
        cpu,
        memory,
    } = context;

    match binding.dispatcher_target {
        PpcImportDispatcherTarget::SysEnvirons => Some(PpcImportAction::Return(ppc_i16_result(
            ppc_sys_environs(memory, cpu.gpr[4]),
        ))),
        PpcImportDispatcherTarget::SVersion => {
            // Inside Macintosh: Devices (1994), 2-30 through 2-31: version 2
            // denotes the ROM-based Slot Manager; spsPointer is reserved.
            let block = cpu.gpr[3];
            let result = if block != 0
                && memory.write_u32_be(block, 2).is_some()
                && memory.write_u32_be(block + 4, 0).is_some()
            {
                PPC_NO_ERR
            } else {
                PPC_PARAM_ERR
            };
            Some(PpcImportAction::Return(ppc_i16_result(result)))
        }
        PpcImportDispatcherTarget::EqualString => {
            Some(PpcImportAction::Return(ppc_equal_string(cpu, memory)))
        }
        PpcImportDispatcherTarget::NumToString => {
            let number = cpu.gpr[3] as i32;
            let string_ptr = cpu.gpr[4];
            if string_ptr != 0 {
                let _ = ppc_write_pstring_bytes(memory, string_ptr, number.to_string().as_bytes());
            }
            Some(PpcImportAction::ReturnPreserve)
        }
        PpcImportDispatcherTarget::StringToNum => {
            ppc_string_to_num(cpu, memory);
            Some(PpcImportAction::ReturnPreserve)
        }
        PpcImportDispatcherTarget::Random => {
            Some(PpcImportAction::Return(u32::from(ppc_random(memory))))
        }
        PpcImportDispatcherTarget::BitAnd => Some(PpcImportAction::Return(cpu.gpr[3] & cpu.gpr[4])),
        PpcImportDispatcherTarget::BitOr => Some(PpcImportAction::Return(cpu.gpr[3] | cpu.gpr[4])),
        PpcImportDispatcherTarget::BitTst => {
            // Inside Macintosh: Operating System Utilities (1992), 3-13:
            // bit zero is the high-order bit of the first addressed byte.
            let bit = cpu.gpr[4];
            let byte_ptr = cpu.gpr[3].wrapping_add(bit / 8);
            let mask = 0x80u8 >> (bit & 7);
            let byte = memory.read_u8(byte_ptr).unwrap_or(0);
            let result = byte & mask != 0;
            Some(PpcImportAction::Return(u32::from(result)))
        }
        _ => None,
    }
}
