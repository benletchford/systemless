//! Typed Toolbox Utilities dispatch for PowerPC imports.

use super::*;
use crate::machine_profile::KEYBOARD_ENVIRON_TYPE;

pub(super) struct PpcToolboxDispatchContext<'a> {
    pub(super) binding: &'a PpcImportBinding,
    pub(super) cpu: &'a mut PpcCpu,
    pub(super) memory: &'a mut PpcSectionMem,
    pub(super) init_graf_global_ptr: u32,
}

pub(super) fn dispatch_toolbox_import(
    context: PpcToolboxDispatchContext<'_>,
) -> Option<PpcImportAction> {
    let PpcToolboxDispatchContext {
        binding,
        cpu,
        memory,
        init_graf_global_ptr,
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
        PpcImportDispatcherTarget::RelString => {
            let left = ppc_read_pstring_bytes(memory, cpu.gpr[3])?;
            let right = ppc_read_pstring_bytes(memory, cpu.gpr[4])?;
            let case_sensitive = cpu.gpr[5] != 0;
            let diac_sensitive = cpu.gpr[6] != 0;
            let result = crate::text_utils::evaluate_rel_string(
                &left,
                &right,
                case_sensitive,
                diac_sensitive,
            );
            Some(PpcImportAction::Return(ppc_i16_result(result)))
        }
        PpcImportDispatcherTarget::InitEditionPackVersion => {
            // As with the classic _Pack11 InitEditionPack path, initialization
            // succeeds for the process-owned Edition Manager stub.
            Some(PpcImportAction::Return(ppc_i16_result(PPC_NO_ERR)))
        }
        PpcImportDispatcherTarget::StuffHex => {
            // Inside Macintosh I, I-195: write each pair of Pascal-string
            // hexadecimal digits to the destination, as the 68K trap does.
            let destination = cpu.gpr[3];
            let digits = ppc_read_pstring_bytes(memory, cpu.gpr[4])?;
            if destination != 0 {
                for (offset, pair) in digits.chunks_exact(2).enumerate() {
                    let nibble = |digit: u8| match digit {
                        b'0'..=b'9' => digit - b'0',
                        b'a'..=b'f' => digit - b'a' + 10,
                        b'A'..=b'F' => digit - b'A' + 10,
                        _ => 0,
                    };
                    let _ = memory.write_u8(
                        destination.wrapping_add(offset as u32),
                        (nibble(pair[0]) << 4) | nibble(pair[1]),
                    );
                }
            }
            Some(PpcImportAction::ReturnPreserve)
        }
        PpcImportDispatcherTarget::IUEqualPString => {
            // IdenticalString / IUEqualPString uses the script's primary ordering.
            // Inside Macintosh: Text (1993), 5-17. itl2Handle at gpr[5] is ignored.
            let left = ppc_read_pstring_bytes(memory, cpu.gpr[3])?;
            let right = ppc_read_pstring_bytes(memory, cpu.gpr[4])?;
            let equal = crate::text_utils::evaluate_iu_equal_string(&left, &right);
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
                let bytes = crate::text_utils::evaluate_num_to_string(number);
                let _ = ppc_write_pstring_bytes(memory, string_ptr, &bytes);
            }
            Some(PpcImportAction::ReturnPreserve)
        }
        PpcImportDispatcherTarget::StringToNum => {
            ppc_string_to_num(cpu, memory);
            Some(PpcImportAction::ReturnPreserve)
        }
        PpcImportDispatcherTarget::GetQDGlobalsRandomSeed => {
            // Quickdraw.h: long GetQDGlobalsRandomSeed(void). A query must
            // preserve all seed bits and must not advance Random's state.
            let seed_addr = qd_random_seed_address(init_graf_global_ptr);
            Some(PpcImportAction::Return(memory.read_u32_be(seed_addr).unwrap_or(0)))
        }
        PpcImportDispatcherTarget::SetQDGlobalsRandomSeed => {
            // Quickdraw.h: void SetQDGlobalsRandomSeed(long randomSeed).
            // Share the live QDGlobals.randSeed location with Random, including
            // its existing pre-InitGraf storage. Do not overwrite LM RandSeed
            // once the application has installed its own QuickDraw globals.
            let seed_addr = qd_random_seed_address(init_graf_global_ptr);
            if ppc_memory_can_write_bytes(memory, seed_addr, 4) {
                let _ = memory.write_u32_be(seed_addr, cpu.gpr[3]);
            }
            Some(PpcImportAction::ReturnPreserve)
        }
        PpcImportDispatcherTarget::Random => {
            // QuickDraw's randSeed lives 126 bytes before thePort, not at
            // the unrelated low-memory RandSeed global (Inside Macintosh I-195).
            let seed_addr = qd_random_seed_address(init_graf_global_ptr);
            // IM:I I-194: Random returns a signed INTEGER. Preserve its sign
            // in r3 so native callers can take its magnitude before division.
            Some(PpcImportAction::Return(ppc_i16_result(
                ppc_random_at(memory, seed_addr) as i16,
            )))
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
    let value = crate::text_utils::evaluate_string_to_num(&bytes);
    let _ = memory.write_u32_be(number_ptr, value as u32);
}

pub(crate) fn ppc_sys_environs(memory: &mut PpcSectionMem, rec_ptr: u32) -> i16 {
    if rec_ptr < 0x100 || !ppc_memory_can_write_bytes(memory, rec_ptr, 16) {
        return PPC_PARAM_ERR;
    }
    let _ = memory.write_u16_be(rec_ptr, 2);
    let _ = memory.write_u16_be(
        rec_ptr + 2,
        crate::machine_profile::POWERPC_GESTALT_MACHINE_TYPE,
    );
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
    let diac_sensitive = (cpu.gpr[6] & 0xff) != 0;
    let Some(a_bytes) = ppc_read_pstring_bytes(memory, a_ptr) else {
        return 0;
    };
    let Some(b_bytes) = ppc_read_pstring_bytes(memory, b_ptr) else {
        return 0;
    };
    let equal = crate::text_utils::evaluate_equal_string(
        &a_bytes,
        &b_bytes,
        case_sensitive,
        diac_sensitive,
    );

    if ppc_hle_trace_enabled() {
        eprintln!(
            "[PPC-TRACE] EqualString a={:?} b={:?} case_sensitive={} diac_sensitive={} -> {}",
            decode_mac_roman(&a_bytes),
            decode_mac_roman(&b_bytes),
            case_sensitive,
            diac_sensitive,
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

fn qd_random_seed_address(init_graf_global_ptr: u32) -> u32 {
    init_graf_global_ptr.checked_sub(126).unwrap_or(PPC_RAND_SEED_ADDR)
}
