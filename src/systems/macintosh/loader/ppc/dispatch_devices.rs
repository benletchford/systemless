//! Typed Device Manager dispatch for PowerPC imports.

use super::*;

pub(super) struct PpcDeviceDispatchContext<'a> {
    pub(super) binding: &'a PpcImportBinding,
    pub(super) cpu: &'a mut PpcCpu,
    pub(super) memory: &'a mut PpcSectionMem,
    pub(super) current_gdevice: u32,
    pub(super) screen_clut: &'a mut [[u16; 3]; 256],
    pub(super) display_gamma: &'a SharedProcessDisplayGamma,
    pub(super) toolbox_startup: &'a mut PpcToolboxStartupState,
    pub(super) sound: &'a mut PpcSoundState,
}

pub(super) fn dispatch_device_import(
    context: PpcDeviceDispatchContext<'_>,
) -> Option<PpcImportAction> {
    let PpcDeviceDispatchContext {
        binding,
        cpu,
        memory,
        current_gdevice,
        screen_clut,
        display_gamma,
        toolbox_startup,
        sound,
    } = context;

    match binding.dispatcher_target {
        PpcImportDispatcherTarget::AutoSleepControl => {
            // AutoSleepControl(Boolean enableSleep): false increments the
            // disable level, and true decrements it. A classic guest cannot
            // put the host to sleep, but this preserves the guest's Power
            // Manager state and its paired query.
            // Inside Macintosh: Devices (1994), pp. 6-43–6-44.
            if cpu.gpr[3] as u8 == 0 {
                toolbox_startup.auto_sleep_disable_level =
                    toolbox_startup.auto_sleep_disable_level.saturating_add(1);
            } else {
                toolbox_startup.auto_sleep_disable_level =
                    toolbox_startup.auto_sleep_disable_level.saturating_sub(1);
            }
            Some(PpcImportAction::ReturnPreserve)
        }
        PpcImportDispatcherTarget::IsAutoSlpControlDisabled => {
            // Boolean IsAutoSlpControlDisabled(); Devices (1994), p. 6-44.
            Some(PpcImportAction::Return(u32::from(
                toolbox_startup.auto_sleep_disable_level != 0,
            )))
        }
        PpcImportDispatcherTarget::GetDCtlEntry => {
            Some(PpcImportAction::Return(if cpu.gpr[3] as u16 as i16 == 0 {
                PPC_MAIN_DCE_HANDLE
            } else {
                0
            }))
        }
        PpcImportDispatcherTarget::GetADBInfo => {
            // Inside Macintosh: Devices (1994), pp. 5-37 and 5-43--5-44:
            // the standard ADB table exposes the keyboard at address 2 and
            // mouse at address 3 through a packed ten-byte ADBDataBlock.
            let info_ptr = cpu.gpr[3];
            let address = cpu.gpr[4] as u8;
            let device = match address {
                2 => Some((2u8, 2u8)),
                3 => Some((1u8, 3u8)),
                _ => None,
            };
            let result = if let Some((handler_id, original_address)) = device {
                if info_ptr != 0 && ppc_memory_can_write_bytes(memory, info_ptr, 10) {
                    let _ = memory.write_u8(info_ptr, handler_id);
                    let _ = memory.write_u8(info_ptr + 1, original_address);
                    let _ = memory.write_u32_be(info_ptr + 2, 0);
                    let _ = memory.write_u32_be(info_ptr + 6, 0);
                }
                PPC_NO_ERR
            } else {
                -1
            };
            Some(PpcImportAction::Return(ppc_i16_result(result)))
        }
        PpcImportDispatcherTarget::OpenDriver => {
            let name = ppc_read_pstring_bytes(memory, cpu.gpr[3])
                .map(|name| decode_mac_roman(&name))
                .unwrap_or_default();
            if cpu.gpr[4] != 0 && ppc_memory_can_write_bytes(memory, cpu.gpr[4], 2) {
                let _ = memory.write_u16_be(cpu.gpr[4], 0);
            }
            if ppc_hle_trace_enabled() {
                eprintln!("[PPC-TRACE] OpenDriver name={name:?} -> openErr");
            }
            // Inside Macintosh: Devices (1994), pp. 1-13 and 1-40--1-41:
            // OpenDriver searches installed units and DRVR resources. PPC HLE
            // exposes neither legacy serial nor AppleTalk device drivers, so
            // the documented openErr response disables those optional paths.
            Some(PpcImportAction::Return(ppc_i16_result(PPC_OPEN_ERR)))
        }
        PpcImportDispatcherTarget::Status => {
            // No legacy device drivers are installed. A status request for a
            // driver that failed OpenDriver must report the absent unit.
            // Inside Macintosh: Devices (1994), pp. 1-76--1-79.
            Some(PpcImportAction::Return(ppc_i16_result(PPC_OPEN_ERR)))
        }
        PpcImportDispatcherTarget::Control => {
            if cpu.gpr[3] as u16 as i16 == -4 {
                let result = legacy_sound_control(memory, sound, cpu.gpr[4] as u16, cpu.gpr[5]);
                return Some(PpcImportAction::Return(ppc_i16_result(result)));
            }
            Some(PpcImportAction::Return(ppc_i16_result(ppc_control(
                cpu,
                memory,
                current_gdevice,
                screen_clut,
                display_gamma,
                toolbox_startup,
            ))))
        }
        PpcImportDispatcherTarget::PBControl => {
            let pb = cpu.gpr[3];
            if pb.checked_add(24).and_then(|p| memory.read_u16_be(p)) == Some((-4i16) as u16) {
                let result = pb
                    .checked_add(28)
                    .and_then(|param| {
                        let code = memory.read_u16_be(pb + 26)?;
                        Some(legacy_sound_control(memory, sound, code, param))
                    })
                    .unwrap_or(PPC_PARAM_ERR);
                let _ = memory.write_u16_be(pb + 16, result as u16);
                return Some(PpcImportAction::Return(ppc_i16_result(result)));
            }
            Some(PpcImportAction::Return(ppc_i16_result(ppc_pb_control(
                cpu,
                memory,
                current_gdevice,
                screen_clut,
                display_gamma,
                toolbox_startup,
            ))))
        }
        PpcImportDispatcherTarget::PBStatus => Some(PpcImportAction::Return(ppc_i16_result(
            ppc_pb_status(cpu, memory),
        ))),
        _ => None,
    }
}

fn legacy_sound_control(
    memory: &mut PpcSectionMem,
    sound: &mut PpcSoundState,
    code: u16,
    param: u32,
) -> i16 {
    if !matches!(code, 2 | 3) {
        return -17; // controlErr
    }
    let Some(value) = memory.read_u16_be(param) else {
        return PPC_PARAM_ERR;
    };
    if code == 3 {
        // Square-wave amplitude, reflected in SoundLevel; see the 68k path.
        let _ = memory.write_u8(
            crate::memory::globals::addr::SOUND_LEVEL,
            value.min(255) as u8,
        );
    } else {
        super::dispatch_sound::set_legacy_sound_volume(memory, sound, value.min(7) as u8);
    }
    PPC_NO_ERR
}
