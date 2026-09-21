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
        PpcImportDispatcherTarget::LMGetMenuList => {
            Some(PpcImportAction::Return(current_menu_list))
        }
        PpcImportDispatcherTarget::LMGetMenuFlash => Some(PpcImportAction::Return(ppc_i16_result(
            memory
                .read_u16_be(crate::memory::globals::addr::MENU_FLASH)
                .unwrap_or(crate::memory::globals::DEFAULT_MENU_FLASH_COUNT) as i16,
        ))),
        PpcImportDispatcherTarget::LMGetPaintWhite => Some(PpcImportAction::Return(u32::from(
            memory.read_u16_be(0x09dc).unwrap_or(1) != 0,
        ))),
        PpcImportDispatcherTarget::LMGetSysMap => {
            // LowMem.h: LMGetSysMap returns the signed reference number of
            // the System file's resource map. The HLE resource chain uses
            // refnum 0 as its system/application fallback map.
            Some(PpcImportAction::Return(ppc_i16_result(0)))
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
            let _ = memory.write_u32_be(crate::memory::globals::addr::RESUME_PROC, cpu.gpr[3]);
            Some(PpcImportAction::ReturnPreserve)
        }
        PpcImportDispatcherTarget::LMSetACount => {
            let _ =
                memory.write_u16_be(crate::memory::globals::addr::ALERT_STAGE, cpu.gpr[3] as u16);
            Some(PpcImportAction::ReturnPreserve)
        }
        PpcImportDispatcherTarget::LMSetANumber => {
            let _ = memory.write_u16_be(crate::memory::globals::addr::ANUMBER, cpu.gpr[3] as u16);
            Some(PpcImportAction::ReturnPreserve)
        }
        PpcImportDispatcherTarget::LMSetDlgFont => {
            let _ = memory.write_u16_be(crate::memory::globals::addr::DLG_FONT, cpu.gpr[3] as u16);
            Some(PpcImportAction::ReturnPreserve)
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
