//! Toolbox entry points whose results depend only on guest memory.

use super::*;
use std::collections::BTreeMap;

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
    SetPortBounds,
    SetPortGrafProcs,
    SetPortPenMode,
    IsPortColor,
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
        PpcToolboxCompatOperation::SetPortBounds => {
            // Quickdraw.h: SetPortBounds(CGrafPtr, const Rect *) replaces
            // portRect at +16, which both port formats share.
            let (port, rect) = (cpu.gpr[3], cpu.gpr[4]);
            if let Some((top, left, bottom, right)) = ppc_read_rect(memory, rect) {
                if port != 0 && ppc_memory_can_write_bytes(memory, port.wrapping_add(16), 8) {
                    let _ = ppc_write_rect(memory, port.wrapping_add(16), top, left, bottom, right);
                }
            }
            PpcImportAction::ReturnPreserve
        }
        PpcToolboxCompatOperation::SetPortGrafProcs => {
            // SetPortGrafProcs(CGrafPtr, CQDProcsPtr) stores grafProcs at
            // +104, the field GetPortGrafProcs reads.
            let port = cpu.gpr[3];
            if port != 0 && ppc_memory_can_write_bytes(memory, port.wrapping_add(104), 4) {
                let _ = memory.write_u32_be(port.wrapping_add(104), cpu.gpr[4]);
            }
            PpcImportAction::ReturnPreserve
        }
        PpcToolboxCompatOperation::SetPortPenMode => {
            // SetPortPenMode(CGrafPtr, SInt32) stores pnMode at +56, as
            // PenMode does for the current port.
            let port = cpu.gpr[3];
            if port != 0 && ppc_memory_can_write_bytes(memory, port.wrapping_add(56), 2) {
                let _ = memory.write_u16_be(port.wrapping_add(56), cpu.gpr[4] as u16);
            }
            PpcImportAction::ReturnPreserve
        }
        PpcToolboxCompatOperation::IsPortColor => {
            // IsPortColor(CGrafPtr) is true for a CGrafPort, whose
            // portVersion at +6 has its two high bits set.
            let color = cpu.gpr[3] != 0
                && memory
                    .read_u16_be(cpu.gpr[3].wrapping_add(6))
                    .is_some_and(|version| version & 0xc000 == 0xc000);
            PpcImportAction::Return(u32::from(color))
        }
    }
}

/// The default content colour of a window whose colour table is the
/// standard one: white.
const PPC_DEFAULT_WINDOW_CONTENT_COLOR: [u16; 3] = [0xffff; 3];

/// SetWindowContentColor(WindowRef, const RGBColor *) and
/// GetWindowContentColor(WindowRef, RGBColor *) keep the colour the Window
/// Manager erases the content region with. The runtime erases with the port
/// background, so the colour is only recorded and reported.
pub(crate) fn ppc_window_content_color(
    set: bool,
    cpu: &PpcCpu,
    memory: &mut PpcSectionMem,
    colors: &mut BTreeMap<u32, [u16; 3]>,
) -> PpcImportAction {
    let (window, color) = (cpu.gpr[3], cpu.gpr[4]);
    if window == 0 || color == 0 {
        return PpcImportAction::Return(ppc_i16_result(PPC_PARAM_ERR));
    }
    let result = if set {
        match ppc_read_rgb_color(memory, color) {
            Some(rgb) => {
                colors.insert(window, [rgb.red, rgb.green, rgb.blue]);
                PPC_NO_ERR
            }
            None => PPC_PARAM_ERR,
        }
    } else {
        let components = colors
            .get(&window)
            .copied()
            .unwrap_or(PPC_DEFAULT_WINDOW_CONTENT_COLOR);
        if ppc_memory_can_write_bytes(memory, color, 6) {
            for (index, component) in components.into_iter().enumerate() {
                let _ = memory.write_u16_be(color + 2 * index as u32, component);
            }
            PPC_NO_ERR
        } else {
            PPC_PARAM_ERR
        }
    };
    PpcImportAction::Return(ppc_i16_result(result))
}
