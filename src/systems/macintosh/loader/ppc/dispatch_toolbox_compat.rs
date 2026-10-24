//! Toolbox entry points whose results depend only on guest memory.

use super::*;

/// Inside Macintosh: Devices (1994), p. 1-112: notOpenErr, driver not open.
const PPC_NOT_OPEN_ERR: i16 = -28;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PpcToolboxCompatOperation {
    /// CloseDriver, KillIO, SerReset, SerSetBuf and SerGetBuf on a driver
    /// that OpenDriver never opened.
    UnopenedDriver,
    GetRegionBounds,
    GetPortTextFont,
    GetPortTextSize,
    SetWindowContentColor,
}

pub(crate) fn ppc_dispatch_toolbox_compat(
    operation: PpcToolboxCompatOperation,
    cpu: &PpcCpu,
    memory: &mut PpcSectionMem,
) -> PpcImportAction {
    match operation {
        PpcToolboxCompatOperation::UnopenedDriver => {
            PpcImportAction::Return(ppc_i16_result(PPC_NOT_OPEN_ERR))
        }
        PpcToolboxCompatOperation::GetRegionBounds => {
            // Quickdraw.h: GetRegionBounds(RgnHandle, Rect *) copies rgnBBox,
            // which follows the UInt16 rgnSize, and returns the Rect pointer.
            let (region, out) = (cpu.gpr[3], cpu.gpr[4]);
            let master = memory.read_u32_be(region).unwrap_or(0);
            if master != 0 && ppc_memory_can_write_bytes(memory, out, 8) {
                for offset in (0..8).step_by(2) {
                    let value = memory.read_u16_be(master + 2 + offset).unwrap_or(0);
                    let _ = memory.write_u16_be(out + offset, value);
                }
            }
            PpcImportAction::Return(out)
        }
        PpcToolboxCompatOperation::GetPortTextFont | PpcToolboxCompatOperation::GetPortTextSize => {
            // GrafPort and CGrafPort share txFont at +68 and txSize at +74.
            let offset = if operation == PpcToolboxCompatOperation::GetPortTextFont {
                68
            } else {
                74
            };
            let value = memory
                .read_u16_be(cpu.gpr[3].wrapping_add(offset))
                .unwrap_or(0);
            PpcImportAction::Return(value as i16 as i32 as u32)
        }
        PpcToolboxCompatOperation::SetWindowContentColor => {
            // MacWindows.h: the content colour only affects how the Window
            // Manager erases content; drawing uses the port background.
            PpcImportAction::Return(0)
        }
    }
}
