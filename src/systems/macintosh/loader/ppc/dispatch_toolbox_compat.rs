//! Toolbox entry points whose results depend only on guest memory.

use super::*;
use std::collections::BTreeMap;

/// Inside Macintosh: Devices (1994), p. 1-112: notOpenErr, driver not open.
const PPC_NOT_OPEN_ERR: i16 = -28;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PpcToolboxCompatOperation {
    PtToAngle,
    IsCmdChar,
    GetColor,
    FindServiceWindow,
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
    /// CMGetToolName(procID, Str255 name): no tool has that procID, so the
    /// name is empty.
    CommToolboxToolName,
    /// GetPageState(const void *address)
    GetPageState,
    /// LMGetWMgrPort()
    WMgrPort,
    /// OpenSlot(ParmBlkPtr, Boolean async)
    OpenSlot,
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
        PpcToolboxCompatOperation::PtToAngle => {
            // Inside Macintosh: Imaging With QuickDraw (PtToAngle): the angle
            // from the rectangle's centre to the point, clockwise from 12
            // o'clock and scaled to the rectangle, so that 45 degrees always
            // passes through the top-right corner.
            let (rect_ptr, point, out) = (cpu.gpr[3], cpu.gpr[4], cpu.gpr[5]);
            if let Some((top, left, bottom, right)) = ppc_read_rect(memory, rect_ptr) {
                let angle = ppc_pt_to_angle(
                    (top, left, bottom, right),
                    ((point >> 16) as u16 as i16, point as u16 as i16),
                );
                if ppc_memory_can_write_bytes(memory, out, 2) {
                    let _ = memory.write_u16_be(out, angle as u16);
                }
            }
            PpcImportAction::ReturnPreserve
        }
        PpcToolboxCompatOperation::IsCmdChar => {
            // Inside Macintosh: Text (IsCmdChar): true when a key-down or
            // auto-key event has the Command key down and its character,
            // translated without the Command modifier, is `test`. The
            // event's message already holds that translation.
            let event = cpu.gpr[3];
            let test = cpu.gpr[4] as u8;
            let what = memory.read_u16_be(event).unwrap_or(0);
            let message = memory.read_u32_be(event + 2).unwrap_or(0);
            let modifiers = memory.read_u16_be(event + 14).unwrap_or(0);
            let matches = matches!(what, 3 | 5) && modifiers & 0x0100 != 0 && message as u8 == test;
            PpcImportAction::Return(u32::from(matches))
        }
        PpcToolboxCompatOperation::GetColor => {
            // The runtime has no Color Picker dialog, so GetColor behaves as
            // a dismissed picker: it returns false and leaves the original
            // colour as the result. Inside Macintosh: Advanced Color
            // Imaging (GetColor).
            let (input, output) = (cpu.gpr[5], cpu.gpr[6]);
            if let Some(color) = ppc_memory_read_bytes(memory, input, 6) {
                if ppc_memory_can_write_bytes(memory, output, 6) {
                    let _ = memory.write_bytes(output, &color);
                }
            }
            PpcImportAction::Return(0)
        }
        PpcToolboxCompatOperation::FindServiceWindow => {
            // Inside Macintosh: Text (FindServiceWindow): no input method
            // has opened a floating service window, so no point is inside
            // one; the result is inDesk with a nil window.
            let out = cpu.gpr[4];
            if ppc_memory_can_write_bytes(memory, out, 4) {
                let _ = memory.write_u32_be(out, 0);
            }
            PpcImportAction::Return(0)
        }
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
        PpcToolboxCompatOperation::CommToolboxToolName => {
            let name = cpu.gpr[4];
            if ppc_memory_can_write_bytes(memory, name, 1) {
                let _ = memory.write_u8(name, 0);
            }
            PpcImportAction::ReturnPreserve
        }
        PpcToolboxCompatOperation::GetPageState => {
            // Inside Macintosh: Memory (GetPageState): with virtual memory
            // off no page is managed by the pager, so every address reports
            // kNotPaged.
            const K_NOT_PAGED: u32 = 2;
            PpcImportAction::Return(K_NOT_PAGED)
        }
        PpcToolboxCompatOperation::WMgrPort => {
            // WMgrPort is the Window Manager port, the same port
            // GetWMgrPort returns.
            PpcImportAction::Return(PPC_MAIN_GWORLD)
        }
        PpcToolboxCompatOperation::OpenSlot => {
            // Inside Macintosh: Devices (OpenSlot): opens a slot device's
            // driver. The runtime has no slot devices or their drivers, so
            // the open fails with openErr in the result and ioResult, as
            // OpenDriver does.
            let pb = cpu.gpr[3];
            if ppc_memory_can_write_bytes(memory, pb, 26) {
                let _ = memory.write_u16_be(pb + 24, 0);
                let _ = memory.write_u16_be(pb + 16, PPC_OPEN_ERR as u16);
            }
            PpcImportAction::Return(ppc_i16_result(PPC_OPEN_ERR))
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

/// PtToAngle's angle in whole degrees, 0 through 359.
pub(crate) fn ppc_pt_to_angle(rect: (i16, i16, i16, i16), point: (i16, i16)) -> i16 {
    let (top, left, bottom, right) = rect;
    let height = f64::from(bottom) - f64::from(top);
    let width = f64::from(right) - f64::from(left);
    let dx = f64::from(point.1) - (f64::from(left) + f64::from(right)) / 2.0;
    let dy = f64::from(point.0) - (f64::from(top) + f64::from(bottom)) / 2.0;
    let (dx, dy) = if width > 0.0 && height > 0.0 {
        (dx / width, dy / height)
    } else {
        (dx, dy)
    };
    if dx == 0.0 && dy == 0.0 {
        return 0;
    }
    let degrees = dx.atan2(-dy).to_degrees().rem_euclid(360.0).round();
    (degrees as i16) % 360
}
