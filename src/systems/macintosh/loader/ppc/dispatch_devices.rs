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
    } = context;

    match binding.dispatcher_target {
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
        PpcImportDispatcherTarget::Control => {
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
