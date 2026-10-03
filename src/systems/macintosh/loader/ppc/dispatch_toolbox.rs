//! Typed Toolbox Utilities dispatch for PowerPC imports.

use super::*;
use crate::machine_profile::KEYBOARD_ENVIRON_TYPE;

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
        PpcImportDispatcherTarget::IUEqualPString => {
            // IdenticalString uses the current script's primary ordering when
            // no explicit 'itl2' resource is supplied (Inside Macintosh: Text, 5-17).
            if cpu.gpr[5] != 0 {
                return None;
            }
            let left = ppc_read_pstring_bytes(memory, cpu.gpr[3])?;
            let right = ppc_read_pstring_bytes(memory, cpu.gpr[4])?;
            let primary = |byte| crate::trap::mac_roman_to_upper(byte, true);
            let equal = left
                .into_iter()
                .map(primary)
                .eq(right.into_iter().map(primary));
            Some(PpcImportAction::Return(u32::from(!equal)))
        }
        PpcImportDispatcherTarget::GetIntlResourceTable => {
            // Inside Macintosh: Text, GetIntlResourceTable. The current HLE has
            // no itl2/itl4 system resources, so there is no table to return.
            // Initialize all VAR outputs as the 68K IUGetIntlTable path does.
            for pointer in [cpu.gpr[5], cpu.gpr[6], cpu.gpr[7]] {
                if pointer != 0 {
                    let _ = memory.write_u32_be(pointer, 0);
                }
            }
            Some(PpcImportAction::ReturnPreserve)
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

pub(crate) fn ppc_string_to_num(cpu: &PpcCpu, memory: &mut PpcSectionMem) {
    let string_ptr = cpu.gpr[3];
    let number_ptr = cpu.gpr[4];
    if string_ptr == 0 || number_ptr == 0 || !ppc_memory_can_write_bytes(memory, number_ptr, 4) {
        return;
    }
    let Some(bytes) = ppc_read_pstring_bytes(memory, string_ptr) else {
        return;
    };
    let mut index = 0usize;
    while bytes
        .get(index)
        .is_some_and(|byte| byte.is_ascii_whitespace())
    {
        index += 1;
    }
    let mut sign = 1i64;
    if let Some(byte) = bytes.get(index) {
        if *byte == b'-' {
            sign = -1;
            index += 1;
        } else if *byte == b'+' {
            index += 1;
        }
    }
    let mut value = 0i64;
    while let Some(byte) = bytes.get(index) {
        if !byte.is_ascii_digit() {
            break;
        }
        value = value
            .saturating_mul(10)
            .saturating_add(i64::from(byte - b'0'));
        index += 1;
    }
    let signed = value
        .saturating_mul(sign)
        .clamp(i32::MIN as i64, i32::MAX as i64) as i32;
    let _ = memory.write_u32_be(number_ptr, signed as u32);
}

pub(crate) fn ppc_sys_environs(memory: &mut PpcSectionMem, rec_ptr: u32) -> i16 {
    if rec_ptr < 0x100 || !ppc_memory_can_write_bytes(memory, rec_ptr, 16) {
        return PPC_PARAM_ERR;
    }
    let _ = memory.write_u16_be(rec_ptr, 2);
    let _ = memory.write_u16_be(rec_ptr + 2, REFERENCE_MACHINE_PROFILE.gestalt_machine_type);
    let _ = memory.write_u16_be(rec_ptr + 4, POWERPC_SYSTEM_VERSION_BCD);
    let _ = memory.write_u16_be(
        rec_ptr + 6,
        REFERENCE_MACHINE_PROFILE.gestalt_processor_type as u16,
    );
    let _ = memory.write_u8(rec_ptr + 8, u8::from(REFERENCE_MACHINE_PROFILE.has_fpu()));
    let _ = memory.write_u8(rec_ptr + 9, 1);
    let _ = memory.write_u16_be(rec_ptr + 10, KEYBOARD_ENVIRON_TYPE);
    let _ = memory.write_u16_be(rec_ptr + 12, 0);
    let _ = memory.write_u16_be(rec_ptr + 14, 0);
    PPC_NO_ERR
}

pub(crate) fn ppc_read_pstring(memory: &mut PpcSectionMem, addr: u32) -> Option<String> {
    Some(decode_mac_roman(&ppc_read_pstring_bytes(memory, addr)?))
}

pub(crate) fn ppc_equal_string(cpu: &PpcCpu, memory: &mut PpcSectionMem) -> u32 {
    let a_ptr = cpu.gpr[3];
    let b_ptr = cpu.gpr[4];
    let case_sensitive = (cpu.gpr[5] & 0xff) != 0;
    let _diac_sensitive = (cpu.gpr[6] & 0xff) != 0;
    let Some(a_bytes) = ppc_read_pstring_bytes(memory, a_ptr) else {
        return 0;
    };
    let Some(b_bytes) = ppc_read_pstring_bytes(memory, b_ptr) else {
        return 0;
    };
    let equal = if case_sensitive {
        a_bytes == b_bytes
    } else {
        a_bytes.eq_ignore_ascii_case(&b_bytes)
    };

    if ppc_hle_trace_enabled() {
        eprintln!(
            "[PPC-TRACE] EqualString a={:?} b={:?} case_sensitive={} -> {}",
            decode_mac_roman(&a_bytes),
            decode_mac_roman(&b_bytes),
            case_sensitive,
            equal
        );
    }

    u32::from(equal)
}

pub(crate) fn ppc_write_pstring_bytes(memory: &mut PpcSectionMem, addr: u32, bytes: &[u8]) -> bool {
    let len = bytes.len().min(255);
    if memory.write_u8(addr, len as u8).is_none() {
        return false;
    }
    for (offset, byte) in bytes.iter().copied().take(len).enumerate() {
        let Some(byte_addr) = addr.checked_add(1 + offset as u32) else {
            return false;
        };
        if memory.write_u8(byte_addr, byte).is_none() {
            return false;
        }
    }
    true
}

pub(crate) fn ppc_read_pstring_bytes(memory: &mut PpcSectionMem, addr: u32) -> Option<Vec<u8>> {
    let len = memory.read_u8(addr)? as usize;
    let mut bytes = Vec::with_capacity(len);
    for offset in 0..len {
        bytes.push(memory.read_u8(addr.checked_add(1 + offset as u32)?)?);
    }
    Some(bytes)
}
