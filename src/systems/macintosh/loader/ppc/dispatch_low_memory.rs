//! Typed low-memory accessor dispatch for PowerPC imports.

use super::*;

pub(super) struct PpcLowMemoryDispatchContext<'a> {
    pub(super) target: &'a PpcImportDispatcherTarget,
    pub(super) cpu: &'a PpcCpu,
    pub(super) memory: &'a mut PpcSectionMem,
    pub(super) current_menu_list: u32,
    pub(super) default_dir_id: u32,
}

pub(super) fn dispatch_low_memory_import(
    context: PpcLowMemoryDispatchContext<'_>,
) -> Option<PpcImportAction> {
    let PpcLowMemoryDispatchContext {
        target,
        cpu,
        memory,
        current_menu_list,
        default_dir_id,
    } = context;
    match target {
        PpcImportDispatcherTarget::LMGetGhostWindow => Some(PpcImportAction::Return(
            memory.read_u32_be(crate::memory::globals::addr::GHOST_WINDOW).unwrap_or(0),
        )),
        PpcImportDispatcherTarget::LMGetMenuList => {
            Some(PpcImportAction::Return(current_menu_list))
        }
        PpcImportDispatcherTarget::LMSetMenuHook => {
            // Inside Macintosh Volume III (1985), low-memory globals:
            // MenuHook at $A30 is the callback address used by MenuSelect.
            let _ = memory.write_u32_be(0x0a30, cpu.gpr[3]);
            Some(PpcImportAction::ReturnPreserve)
        }
        PpcImportDispatcherTarget::LMSetCurActivate => {
            // CurActivate is the pointer to the window awaiting an activate
            // event. Inside Macintosh Volume I (1985), I-280; Volume III,
            // low-memory globals, $A64.
            let _ = memory.write_u32_be(crate::memory::globals::addr::CUR_ACTIVATE, cpu.gpr[3]);
            Some(PpcImportAction::ReturnPreserve)
        }
        PpcImportDispatcherTarget::LMGetCurDeactive => Some(PpcImportAction::Return(
            memory
                .read_u32_be(crate::memory::globals::addr::CUR_DEACTIVE)
                .unwrap_or(0),
        )),
        PpcImportDispatcherTarget::LMSetCurDeactive => {
            let _ = memory.write_u32_be(crate::memory::globals::addr::CUR_DEACTIVE, cpu.gpr[3]);
            Some(PpcImportAction::ReturnPreserve)
        }
        PpcImportDispatcherTarget::LMGetMenuFlash => Some(PpcImportAction::Return(ppc_i16_result(
            memory
                .read_u16_be(crate::memory::globals::addr::MENU_FLASH)
                .unwrap_or(crate::memory::globals::DEFAULT_MENU_FLASH_COUNT) as i16,
        ))),
        PpcImportDispatcherTarget::LMGetPaintWhite => Some(PpcImportAction::Return(u32::from(
            memory.read_u16_be(0x09dc).unwrap_or(1) != 0,
        ))),
        PpcImportDispatcherTarget::LMGetHWCfgFlags => Some(PpcImportAction::Return(
            // Apple Technical Note PT09 identifies HwCfgFlgs as a signed
            // low-memory word at $0B22.
            ppc_i16_result(memory.read_u16_be(0x0b22).unwrap_or(0) as i16),
        )),
        PpcImportDispatcherTarget::LMSetROMMapInsert => {
            // RomMapInsert is a byte global at $0B9E. The Resource Manager
            // consumes it on the next resource lookup.
            let _ = memory.write_u8(0x0b9e, cpu.gpr[3] as u8);
            Some(PpcImportAction::ReturnPreserve)
        }
        PpcImportDispatcherTarget::LMGetSysMap => {
            // LowMem.h: LMGetSysMap returns the signed reference number of
            // the System file's resource map. The HLE resource chain uses
            // refnum 0 as its system/application fallback map.
            Some(PpcImportAction::Return(ppc_i16_result(0)))
        }
        PpcImportDispatcherTarget::LMGetCurApRefNum => {
            // Inside Macintosh Volume II (1985), II-58: CurApRefNum is the
            // reference number of the current application's resource file.
            Some(PpcImportAction::Return(ppc_i16_result(
                memory
                    .read_u16_be(crate::memory::globals::addr::CUR_APREF_NUM)
                    .unwrap_or(0) as i16,
            )))
        }
        PpcImportDispatcherTarget::GetVCBQHdr => {
            // Inside Macintosh Volume II (1985), II-126: GetVCBQHdr returns
            // a pointer to the volume-control-block queue header.
            Some(PpcImportAction::Return(
                crate::memory::globals::addr::VCB_Q_HDR,
            ))
        }
        PpcImportDispatcherTarget::GetDrvQHdr => {
            // Inside Macintosh: Files (1992), 2-236: GetDrvQHdr returns
            // the address of the drive queue header in low memory.
            Some(PpcImportAction::Return(
                crate::memory::globals::addr::DRV_Q_HDR,
            ))
        }
        PpcImportDispatcherTarget::LMGetSysEvtMask => Some(PpcImportAction::Return(
            u32::from(memory.read_u16_be(crate::memory::globals::addr::SYS_EVT_MASK)
                .unwrap_or(crate::memory::globals::DEFAULT_SYS_EVT_MASK)),
        )),
        PpcImportDispatcherTarget::LMSetSysEvtMask => {
            let _ = memory.write_u16_be(
                crate::memory::globals::addr::SYS_EVT_MASK, cpu.gpr[3] as u16,
            );
            Some(PpcImportAction::ReturnPreserve)
        }
        PpcImportDispatcherTarget::LMGetDefltStack => Some(PpcImportAction::Return(
            // Inside Macintosh Volume II (1985), II-17 and II-51:
            // DefltStack is the long default stack-space allotment.
            memory
                .read_u32_be(crate::memory::globals::addr::DEFLT_STACK)
                .unwrap_or(crate::memory::globals::DEFAULT_DEFLT_STACK_SIZE),
        )),
        PpcImportDispatcherTarget::LMGetCurStackBase => Some(PpcImportAction::Return(
            // Inside Macintosh Volume II (1985), II-51: CurStackBase is the
            // address of the stack base and start of application globals.
            memory
                .read_u32_be(crate::memory::globals::addr::CUR_STACK_BASE)
                .unwrap_or(PPC_STACK_BASE),
        )),
        PpcImportDispatcherTarget::LMSetPaintWhite => {
            let _ = memory.write_u16_be(0x09dc, u16::from(cpu.gpr[3] != 0));
            Some(PpcImportAction::ReturnPreserve)
        }
        PpcImportDispatcherTarget::LMSetResumeProc => {
            let resume_proc = crate::dialog_manager::evaluate_set_resume_proc(cpu.gpr[3]);
            let _ = memory.write_u32_be(crate::memory::globals::addr::RESUME_PROC, resume_proc);
            Some(PpcImportAction::ReturnPreserve)
        }
        PpcImportDispatcherTarget::LMGetResumeProc => {
            let raw_proc = memory
                .read_u32_be(crate::memory::globals::addr::RESUME_PROC)
                .unwrap_or(0);
            let resume_proc = crate::dialog_manager::evaluate_get_resume_proc(raw_proc);
            Some(PpcImportAction::Return(resume_proc))
        }
        PpcImportDispatcherTarget::LMSetACount => {
            let stage = crate::dialog_manager::evaluate_set_alert_stage(cpu.gpr[3] as u16 as i16);
            let _ =
                memory.write_u16_be(crate::memory::globals::addr::ALERT_STAGE, stage);
            Some(PpcImportAction::ReturnPreserve)
        }
        PpcImportDispatcherTarget::LMGetACount => {
            let raw_stage = memory
                .read_u16_be(crate::memory::globals::addr::ALERT_STAGE)
                .unwrap_or(0);
            let stage = crate::dialog_manager::evaluate_get_alert_stage(raw_stage);
            Some(PpcImportAction::Return(ppc_i16_result(stage)))
        }
        PpcImportDispatcherTarget::LMSetANumber => {
            let anumber = crate::dialog_manager::evaluate_set_anumber(cpu.gpr[3] as u16 as i16);
            let _ = memory.write_u16_be(crate::memory::globals::addr::ANUMBER, anumber);
            Some(PpcImportAction::ReturnPreserve)
        }
        PpcImportDispatcherTarget::LMGetANumber => {
            let raw_anumber = memory
                .read_u16_be(crate::memory::globals::addr::ANUMBER)
                .unwrap_or(0);
            let anumber = crate::dialog_manager::evaluate_get_anumber(raw_anumber);
            Some(PpcImportAction::Return(ppc_i16_result(anumber)))
        }
        PpcImportDispatcherTarget::LMSetDABeeper => {
            let sound_proc = crate::dialog_manager::evaluate_set_da_beeper(cpu.gpr[3]);
            let _ = memory.write_u32_be(crate::memory::globals::addr::DA_BEEPER, sound_proc);
            Some(PpcImportAction::ReturnPreserve)
        }
        PpcImportDispatcherTarget::LMGetDABeeper => {
            let raw_beeper = memory
                .read_u32_be(crate::memory::globals::addr::DA_BEEPER)
                .unwrap_or(0);
            let sound_proc = crate::dialog_manager::evaluate_get_da_beeper(raw_beeper);
            Some(PpcImportAction::Return(sound_proc))
        }
        PpcImportDispatcherTarget::LMGetDAStrings => {
            let addr = crate::dialog_manager::evaluate_get_da_strings_addr();
            Some(PpcImportAction::Return(addr))
        }
        PpcImportDispatcherTarget::LMSetDlgFont => {
            let params = crate::dialog_manager::evaluate_set_dialog_font_parameters(
                cpu.gpr[3] as u16 as i16,
            );
            let _ = memory.write_u16_be(
                crate::memory::globals::addr::DLG_FONT,
                params.font_num() as u16,
            );
            Some(PpcImportAction::ReturnPreserve)
        }
        PpcImportDispatcherTarget::LMGetDlgFont => {
            let raw_font = memory
                .read_u16_be(crate::memory::globals::addr::DLG_FONT)
                .unwrap_or(0);
            let font = crate::dialog_manager::evaluate_dialog_font(raw_font);
            Some(PpcImportAction::Return(ppc_i16_result(font)))
        }
        PpcImportDispatcherTarget::SetMenuFlash => {
            // Macintosh Toolbox Essentials (1992), p. 3-142: SetMenuFlash
            // stores the selected-menu blink count in the MenuFlash global.
            let _ =
                memory.write_u16_be(crate::memory::globals::addr::MENU_FLASH, cpu.gpr[3] as u16);
            Some(PpcImportAction::ReturnPreserve)
        }
        PpcImportDispatcherTarget::GetGrayRgn => Some(PpcImportAction::Return(
            memory
                .read_u32_be(PPC_GRAY_RGN_ADDR)
                .unwrap_or(PPC_GRAY_RGN_HANDLE),
        )),
        PpcImportDispatcherTarget::LMSetGrayRgn => {
            // Macintosh Toolbox Essentials (1992), p. 4-113: GrayRgn is the
            // low-memory handle to the current desktop region.
            let _ = memory.write_u32_be(PPC_GRAY_RGN_ADDR, cpu.gpr[3]);
            Some(PpcImportAction::ReturnPreserve)
        }
        PpcImportDispatcherTarget::LMGetUTableBase => {
            // Inside Macintosh: Devices (1994), pp. 1-8--1-9: UTableBase
            // points to an array of DCE handles, with the Sound Driver at
            // unit 3 (reference number -4).
            Some(PpcImportAction::Return(PPC_UNIT_TABLE))
        }
        PpcImportDispatcherTarget::LMGetCurDirStore => {
            // Inside Macintosh: PowerPC System Software (1994), p. 1-57:
            // LMGetCurDirStore returns the CurDirStore directory ID at $0398.
            Some(PpcImportAction::Return(
                memory
                    .read_u32_be(crate::memory::globals::addr::CUR_DIR_STORE)
                    .unwrap_or(default_dir_id),
            ))
        }
        PpcImportDispatcherTarget::LMSetCurDirStore => {
            // Inside Macintosh: PowerPC System Software (1994), p. 1-57:
            // LMSetCurDirStore sets the CurDirStore directory ID at $0398.
            let _ = memory.write_u32_be(crate::memory::globals::addr::CUR_DIR_STORE, cpu.gpr[3]);
            Some(PpcImportAction::ReturnPreserve)
        }
        PpcImportDispatcherTarget::LMGetSFSaveDisk => {
            // Inside Macintosh: Files (1992), p. 3-65: SFSaveDisk is the
            // signed word at $0214 containing the negative volume refnum.
            Some(PpcImportAction::Return(ppc_i16_result(
                memory
                    .read_u16_be(crate::memory::globals::addr::SF_SAVE_DISK)
                    .unwrap_or_default() as i16,
            )))
        }
        PpcImportDispatcherTarget::LMSetSFSaveDisk => {
            // Inside Macintosh: Files (1992), p. 3-65: SFSaveDisk is the
            // signed word at $0214 containing the negative volume refnum.
            let _ = memory.write_u16_be(
                crate::memory::globals::addr::SF_SAVE_DISK,
                cpu.gpr[3] as u16,
            );
            Some(PpcImportAction::ReturnPreserve)
        }
        PpcImportDispatcherTarget::LMGetRndSeed => Some(PpcImportAction::Return(
            memory.read_u32_be(PPC_RAND_SEED_ADDR).unwrap_or(1),
        )),
        PpcImportDispatcherTarget::LMGetCrsrBusy => {
            // CrsrBusy is true only while the system is changing the cursor.
            // No asynchronous cursor operation is active in this HLE.
            // Inside Macintosh: Processes (1994), 4-18.
            Some(PpcImportAction::Return(0))
        }
        PpcImportDispatcherTarget::LMSetRndSeed => {
            // Inside Macintosh: Memory (1992), pp. 2-6--2-8: RndSeed is the
            // 32-bit random-number seed low-memory global at $0156.
            let _ = memory.write_u32_be(PPC_RAND_SEED_ADDR, cpu.gpr[3]);
            Some(PpcImportAction::ReturnPreserve)
        }
        PpcImportDispatcherTarget::SetCurrentA5
        | PpcImportDispatcherTarget::SetA5
        | PpcImportDispatcherTarget::LMGetCurrentA5 => {
            // Native PowerPC code has no architectural A5 register. These
            // compatibility routes expose the synthetic mini-A5 world used
            // when native glue brackets a legacy callback.
            Some(PpcImportAction::Return(PPC_DATA_BASE))
        }
        _ => None,
    }
}
