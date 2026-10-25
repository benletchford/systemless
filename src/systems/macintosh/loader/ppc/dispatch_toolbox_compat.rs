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
    GetPortTextFace,
    GetPortTextMode,
    GetPortPenMode,
    GetPortPenSize,
    GetPortGrafProcs,
    GetPortForeColor,
    GetPortBackColor,
    GetPixBounds,
    SetWindowContentColor,
    /// Communications Toolbox calls with no connection tools installed:
    /// CMNew and CRMSearch find nothing and return nil.
    CommToolboxNil,
    /// The other calls report cmGenericError (-1); CMGetProcID's -1 means
    /// no such tool, and CMIdle and CMDispose ignore the value.
    CommToolboxError,
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
        PpcToolboxCompatOperation::GetPortTextFace => {
            // txFace is a Style byte at +70 in both port formats.
            let value = memory.read_u8(cpu.gpr[3].wrapping_add(70)).unwrap_or(0);
            PpcImportAction::Return(u32::from(value))
        }
        PpcToolboxCompatOperation::GetPortTextMode => {
            let value = memory.read_u16_be(cpu.gpr[3].wrapping_add(72)).unwrap_or(0);
            PpcImportAction::Return(value as i16 as i32 as u32)
        }
        PpcToolboxCompatOperation::GetPortPenMode => {
            let value = memory.read_u16_be(cpu.gpr[3].wrapping_add(56)).unwrap_or(0);
            PpcImportAction::Return(value as i16 as i32 as u32)
        }
        PpcToolboxCompatOperation::GetPortPenSize => {
            // Quickdraw.h: GetPortPenSize(CGrafPtr, Point *) copies pnSize
            // at +52 and returns the Point pointer.
            let (port, out) = (cpu.gpr[3], cpu.gpr[4]);
            let size = memory.read_u32_be(port.wrapping_add(52)).unwrap_or(0);
            if ppc_memory_can_write_bytes(memory, out, 4) {
                let _ = memory.write_u32_be(out, size);
            }
            PpcImportAction::Return(out)
        }
        PpcToolboxCompatOperation::GetPortGrafProcs => {
            let procs = memory
                .read_u32_be(cpu.gpr[3].wrapping_add(104))
                .unwrap_or(0);
            PpcImportAction::Return(procs)
        }
        PpcToolboxCompatOperation::GetPortForeColor
        | PpcToolboxCompatOperation::GetPortBackColor => {
            // A CGrafPort (portVersion high bits set) keeps rgbFgColor at +36
            // and rgbBkColor at +42. A basic GrafPort has only the old
            // eight-colour model, which defaults to black on white.
            let (port, out) = (cpu.gpr[3], cpu.gpr[4]);
            let fore = operation == PpcToolboxCompatOperation::GetPortForeColor;
            let color_port = memory
                .read_u16_be(port.wrapping_add(6))
                .is_some_and(|version| version & 0xc000 == 0xc000);
            let components = if color_port {
                let base = port.wrapping_add(if fore { 36 } else { 42 });
                [0, 2, 4].map(|offset| memory.read_u16_be(base + offset).unwrap_or(0))
            } else if fore {
                [0; 3]
            } else {
                [0xffff; 3]
            };
            if ppc_memory_can_write_bytes(memory, out, 6) {
                for (index, component) in components.into_iter().enumerate() {
                    let _ = memory.write_u16_be(out + 2 * index as u32, component);
                }
            }
            PpcImportAction::Return(out)
        }
        PpcToolboxCompatOperation::GetPixBounds => {
            // Quickdraw.h: GetPixBounds(PixMapHandle, Rect *) copies the
            // PixMap's bounds at +6 and returns the Rect pointer.
            let (pixmap, out) = (cpu.gpr[3], cpu.gpr[4]);
            let master = memory.read_u32_be(pixmap).unwrap_or(0);
            if master != 0 && ppc_memory_can_write_bytes(memory, out, 8) {
                for offset in (0..8).step_by(2) {
                    let value = memory.read_u16_be(master + 6 + offset).unwrap_or(0);
                    let _ = memory.write_u16_be(out + offset, value);
                }
            }
            PpcImportAction::Return(out)
        }
        PpcToolboxCompatOperation::CommToolboxNil => PpcImportAction::Return(0),
        PpcToolboxCompatOperation::CommToolboxError => PpcImportAction::Return(ppc_i16_result(-1)),
        PpcToolboxCompatOperation::SetWindowContentColor => {
            // MacWindows.h: the content colour only affects how the Window
            // Manager erases content; drawing uses the port background.
            PpcImportAction::Return(0)
        }
    }
}
