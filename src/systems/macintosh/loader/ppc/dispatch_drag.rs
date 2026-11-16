//! Classic DragLib PowerPC ABI adapters (Apple Drag.h).
use super::*;
use crate::drag_manager::{
    DragFlavor, DragHandler, DragHandlerKind, BAD_DRAG_ITEM_ERR, CANT_GET_FLAVOR_ERR,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PpcDragOperation {
    Track,
    SetImage,
    ShowHilite,
    HideHilite,
    WaitMouseMoved,
    ZoomRects,
    New,
    Dispose,
    AddFlavor,
    CountItems,
    GetFlavorSize,
    GetFlavorFlags,
    GetFlavorData,
    GetAttributes,
    GetMouse,
    GetDropLocation,
    GetItemReference,
    GetItemBounds,
    SetItemBounds,
    InstallTrackingHandler,
    InstallReceiveHandler,
    RemoveTrackingHandler,
    RemoveReceiveHandler,
}

pub(super) fn dispatch_drag_import(
    binding: &PpcImportBinding,
    cpu: &mut PpcCpu,
    memory: &mut PpcSectionMem,
    manager: &SharedProcessDragManager,
) -> Option<PpcImportAction> {
    let PpcImportDispatcherTarget::Drag(operation) = binding.dispatcher_target else {
        return None;
    };
    let arguments = cpu.gpr;
    let result: Result<(), i16> = (|| {
        match operation {
            PpcDragOperation::Track
            | PpcDragOperation::SetImage
            | PpcDragOperation::WaitMouseMoved
            | PpcDragOperation::ZoomRects
            | PpcDragOperation::ShowHilite
            | PpcDragOperation::HideHilite => {
                unreachable!("TrackDrag uses the retained controller")
            }
            PpcDragOperation::New => {
                let output = arguments[3];
                if output == 0 || !ppc_memory_can_write_bytes(memory, output, 4) {
                    return Err(PPC_PARAM_ERR);
                }
                let reference = manager.with_mut(|state| state.new_drag())?;
                memory
                    .write_u32_be(output, reference)
                    .ok_or(PPC_PARAM_ERR)?;
            }
            PpcDragOperation::Dispose => {
                manager.with_mut(|state| state.dispose(arguments[3]))?;
            }
            PpcDragOperation::AddFlavor => {
                let size = arguments[7] as i32;
                if arguments[6] != 0 && size < 0 {
                    return Err(PPC_PARAM_ERR);
                }
                let data = if arguments[6] == 0 {
                    None
                } else {
                    Some(
                        ppc_memory_read_bytes(memory, arguments[6], size as u32)
                            .ok_or(PPC_PARAM_ERR)?,
                    )
                };
                let flavor = DragFlavor {
                    flavor_type: arguments[5],
                    flags: arguments[8],
                    data,
                };
                manager.with_mut(|state| {
                    state
                        .get_mut(arguments[3])?
                        .add_flavor(arguments[4], flavor)
                })?;
            }
            PpcDragOperation::CountItems => {
                let output = arguments[4];
                if output == 0 || !ppc_memory_can_write_bytes(memory, output, 2) {
                    return Err(PPC_PARAM_ERR);
                }
                let count = manager
                    .with_ref(|state| state.get(arguments[3]).map(|record| record.items.len()))?;
                let count = u16::try_from(count).map_err(|_| PPC_PARAM_ERR)?;
                memory.write_u16_be(output, count).ok_or(PPC_PARAM_ERR)?;
            }
            PpcDragOperation::GetFlavorSize | PpcDragOperation::GetFlavorFlags => {
                let output = arguments[6];
                if output == 0 || !ppc_memory_can_write_bytes(memory, output, 4) {
                    return Err(PPC_PARAM_ERR);
                }
                let value = manager.with_ref(|state| {
                    let flavor = state
                        .get(arguments[3])?
                        .flavor(arguments[4], arguments[5])?;
                    if operation == PpcDragOperation::GetFlavorFlags {
                        Ok(flavor.flags)
                    } else {
                        let data = flavor.data.as_ref().ok_or(CANT_GET_FLAVOR_ERR)?;
                        u32::try_from(data.len()).map_err(|_| PPC_PARAM_ERR)
                    }
                })?;
                memory.write_u32_be(output, value).ok_or(PPC_PARAM_ERR)?;
            }
            PpcDragOperation::GetItemReference => {
                let output = arguments[5];
                if output == 0 || !ppc_memory_can_write_bytes(memory, output, 4) {
                    return Err(PPC_PARAM_ERR);
                }
                let index = arguments[4] as u16;
                let reference = manager.with_ref(|state| {
                    let record = state.get(arguments[3])?;
                    index
                        .checked_sub(1)
                        .and_then(|i| record.items.get(usize::from(i)))
                        .map(|item| item.reference)
                        .ok_or(BAD_DRAG_ITEM_ERR)
                })?;
                memory
                    .write_u32_be(output, reference)
                    .ok_or(PPC_PARAM_ERR)?;
            }
            PpcDragOperation::GetItemBounds => {
                let output = arguments[5];
                if output == 0 || !ppc_memory_can_write_bytes(memory, output, 8) {
                    return Err(PPC_PARAM_ERR);
                }
                let bounds =
                    manager.with_ref(|state| state.get(arguments[3])?.item_bounds(arguments[4]))?;
                ppc_write_rect(memory, output, bounds.0, bounds.1, bounds.2, bounds.3)
                    .ok_or(PPC_PARAM_ERR)?;
            }
            PpcDragOperation::SetItemBounds => {
                if arguments[5] == 0 {
                    return Err(PPC_PARAM_ERR);
                }
                let bounds = ppc_read_rect(memory, arguments[5]).ok_or(PPC_PARAM_ERR)?;
                manager.with_mut(|state| {
                    state.get_mut(arguments[3])?.item_mut(arguments[4])?.bounds = Some(bounds);
                    Ok::<(), i16>(())
                })?;
            }
            PpcDragOperation::InstallTrackingHandler | PpcDragOperation::InstallReceiveHandler => {
                let procedure = arguments[3];
                let target = ppc_resolve_callback_target(memory, procedure, arguments[2], None)
                    .ok_or(PPC_PARAM_ERR)?;
                let handler = DragHandler {
                    kind: if operation == PpcDragOperation::InstallTrackingHandler {
                        DragHandlerKind::Tracking
                    } else {
                        DragHandlerKind::Receive
                    },
                    window: arguments[4],
                    procedure,
                    refcon: arguments[5],
                    rtoc: target.rtoc,
                };
                manager.with_mut(|state| state.install_handler(handler))?;
            }
            PpcDragOperation::RemoveTrackingHandler | PpcDragOperation::RemoveReceiveHandler => {
                let kind = if operation == PpcDragOperation::RemoveTrackingHandler {
                    DragHandlerKind::Tracking
                } else {
                    DragHandlerKind::Receive
                };
                manager.with_mut(|state| state.remove_handler(kind, arguments[4], arguments[3]))?;
            }
            PpcDragOperation::GetAttributes => {
                let output = arguments[4];
                if output == 0 || !ppc_memory_can_write_bytes(memory, output, 4) {
                    return Err(PPC_PARAM_ERR);
                }
                let value = manager
                    .with_ref(|state| state.get(arguments[3]).map(|record| record.attributes))?;
                memory.write_u32_be(output, value).ok_or(PPC_PARAM_ERR)?;
            }
            PpcDragOperation::GetMouse => {
                for output in [arguments[4], arguments[5]] {
                    if output != 0 && !ppc_memory_can_write_bytes(memory, output, 4) {
                        return Err(PPC_PARAM_ERR);
                    }
                }
                let point =
                    manager.with_ref(|state| state.get(arguments[3]).map(|record| record.mouse))?;
                for output in [arguments[4], arguments[5]] {
                    if output != 0 {
                        memory
                            .write_u16_be(output, point.0 as u16)
                            .ok_or(PPC_PARAM_ERR)?;
                        memory
                            .write_u16_be(output + 2, point.1 as u16)
                            .ok_or(PPC_PARAM_ERR)?;
                    }
                }
            }
            PpcDragOperation::GetDropLocation => {
                let output = arguments[4];
                if output == 0 || !ppc_memory_can_write_bytes(memory, output, 8) {
                    return Err(PPC_PARAM_ERR);
                }
                manager.with_ref(|state| state.get(arguments[3]).map(|_| ()))?;
                // No receiver has set a location: Apple documents typeNull.
                memory
                    .write_u32_be(output, u32::from_be_bytes(*b"null"))
                    .ok_or(PPC_PARAM_ERR)?;
                memory.write_u32_be(output + 4, 0).ok_or(PPC_PARAM_ERR)?;
            }
            PpcDragOperation::GetFlavorData => {
                let size_ptr = arguments[7];
                if size_ptr == 0 || !ppc_memory_can_write_bytes(memory, size_ptr, 4) {
                    return Err(PPC_PARAM_ERR);
                }
                let capacity = memory.read_u32_be(size_ptr).ok_or(PPC_PARAM_ERR)? as i32;
                if capacity < 0 {
                    return Err(PPC_PARAM_ERR);
                }
                let bytes = manager.with_ref(|state| {
                    state
                        .get(arguments[3])?
                        .flavor_data(
                            arguments[4],
                            arguments[5],
                            arguments[8] as usize,
                            capacity as usize,
                        )
                        .map(<[u8]>::to_vec)
                })?;
                let length = bytes.len() as u32;
                if length != 0
                    && (arguments[6] == 0
                        || !ppc_memory_can_write_bytes(memory, arguments[6], length))
                {
                    return Err(PPC_PARAM_ERR);
                }
                if length != 0 {
                    memory
                        .write_bytes(arguments[6], &bytes)
                        .ok_or(PPC_PARAM_ERR)?;
                }
                memory.write_u32_be(size_ptr, length).ok_or(PPC_PARAM_ERR)?;
            }
        }
        Ok(())
    })();
    Some(PpcImportAction::Return(ppc_i16_result(
        result.err().unwrap_or(0),
    )))
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct PpcZoomRectState {
    import_pc: u32,
    entry_sp: u32,
    path: crate::drag_manager::ZoomRectPath,
    surface: PpcPixMapBits,
    saved: Vec<(i32, i32, u32)>,
    step: u16,
    tick: u32,
    started_tick: u32,
}

fn zoom_rect_mask(depth: u32) -> Option<u32> {
    match depth {
        32 => Some(0x00ff_ffff),
        16 => Some(0x7fff),
        1 | 2 | 4 | 8 => Some((1 << depth) - 1),
        _ => None,
    }
}

fn zoom_rect_erase(memory: &mut PpcSectionMem, state: &mut PpcZoomRectState) {
    let Some(mask) = zoom_rect_mask(state.surface.depth) else {
        return;
    };
    for (x, y, pixel) in state.saved.drain(..) {
        if ppc_read_pixmap_raw_pixel(memory, state.surface, x, y) == Some(pixel ^ mask) {
            let _ = ppc_write_pixmap_raw_pixel(memory, state.surface, x, y, pixel);
        }
    }
}

#[cfg(test)]
thread_local! {
    static TEST_ZOOM_RESERVATION_FAILURE: std::cell::Cell<u8> = const { std::cell::Cell::new(0) };
}

#[cfg(test)]
pub(super) fn fail_zoom_reservation_for_test(stage: u8) {
    TEST_ZOOM_RESERVATION_FAILURE.with(|failure| failure.set(stage));
}

fn zoom_reservation_fails(stage: u8) -> bool {
    #[cfg(test)]
    {
        return TEST_ZOOM_RESERVATION_FAILURE.with(|failure| {
            if failure.get() == stage {
                failure.set(0);
                true
            } else {
                false
            }
        });
    }
    #[cfg(not(test))]
    {
        let _ = stage;
        false
    }
}

fn zoom_rect_draw(memory: &mut PpcSectionMem, state: &mut PpcZoomRectState) -> Result<(), i16> {
    let [top, left, bottom, right] = state.path.frame(state.step).map(i32::from);
    if top >= bottom || left >= right {
        return Ok(());
    }
    let bits = state.surface;
    let Some(mask) = zoom_rect_mask(bits.depth) else {
        return Ok(());
    };
    let left_clip = left.max(i32::from(bits.left));
    let right_clip = right.min(i32::from(bits.right));
    let top_clip = top.max(i32::from(bits.top));
    let bottom_clip = bottom.min(i32::from(bits.bottom));
    if left_clip >= right_clip || top_clip >= bottom_clip {
        return Ok(());
    }
    let max_pixels = 2 * ((right_clip - left_clip) as usize + (bottom_clip - top_clip) as usize);
    if zoom_reservation_fails(2) || state.saved.try_reserve(max_pixels).is_err() {
        return Err(PPC_MEM_FULL_ERR);
    }
    let mut draw = |x, y| {
        if let Some(pixel) = ppc_read_pixmap_raw_pixel(memory, bits, x, y) {
            if ppc_write_pixmap_raw_pixel(memory, bits, x, y, pixel ^ mask).is_some() {
                state.saved.push((x, y, pixel));
            }
        }
    };
    for x in left_clip..right_clip {
        if top >= top_clip && top < bottom_clip {
            draw(x, top);
        }
        if bottom - 1 != top && bottom - 1 >= top_clip && bottom - 1 < bottom_clip {
            draw(x, bottom - 1);
        }
    }
    for y in (top + 1).max(top_clip)..(bottom - 1).min(bottom_clip) {
        if left >= left_clip && left < right_clip {
            draw(left, y);
        }
        if right - 1 != left && right - 1 >= left_clip && right - 1 < right_clip {
            draw(right - 1, y);
        }
    }
    Ok(())
}

pub(super) fn zoom_rects(
    cpu: &PpcCpu,
    memory: &mut PpcSectionMem,
    gworlds: &[PpcGWorldRecord],
    startup: &mut PpcToolboxStartupState,
    tick: u32,
    cycles_per_tick: u32,
) -> PpcImportAction {
    if startup
        .zoom_rect
        .last()
        .is_none_or(|state| state.import_pc != cpu.pc || state.entry_sp != cpu.gpr[1])
    {
        if cpu.gpr[3] == 0 || cpu.gpr[4] == 0 {
            return PpcImportAction::Return(ppc_i16_result(PPC_PARAM_ERR));
        }
        let Some(from) = ppc_read_rect(memory, cpu.gpr[3]) else {
            return PpcImportAction::Return(ppc_i16_result(PPC_PARAM_ERR));
        };
        let Some(to) = ppc_read_rect(memory, cpu.gpr[4]) else {
            return PpcImportAction::Return(ppc_i16_result(PPC_PARAM_ERR));
        };
        let Some(path) = crate::drag_manager::ZoomRectPath::new(
            [from.0, from.1, from.2, from.3],
            [to.0, to.1, to.2, to.3],
            cpu.gpr[5] as i16,
            cpu.gpr[6] as i16,
        ) else {
            return PpcImportAction::Return(ppc_i16_result(PPC_PARAM_ERR));
        };
        let Some(front) = ppc_live_front_buffer_for_gworld(memory, gworlds, PPC_MAIN_GWORLD) else {
            return PpcImportAction::Return(ppc_i16_result(PPC_PARAM_ERR));
        };
        let surface = PpcPixMapBits {
            base_addr: front.base_addr,
            row_bytes: front.row_bytes,
            top: 0,
            left: 0,
            bottom: front.height.min(i16::MAX as u32) as i16,
            right: front.width.min(i16::MAX as u32) as i16,
            width: front.width,
            height: front.height,
            depth: front.depth,
        };
        if zoom_reservation_fails(1) || startup.zoom_rect.try_reserve(1).is_err() {
            return PpcImportAction::Return(ppc_i16_result(PPC_MEM_FULL_ERR));
        }
        let mut state = PpcZoomRectState {
            import_pc: cpu.pc,
            entry_sp: cpu.gpr[1],
            path,
            surface,
            saved: Vec::new(),
            step: 0,
            tick,
            started_tick: tick,
        };
        if let Err(error) = zoom_rect_draw(memory, &mut state) {
            return PpcImportAction::Return(ppc_i16_result(error));
        }
        startup.zoom_rect.push(state);
        return PpcImportAction::Yield(u64::from(cycles_per_tick.max(1)));
    }
    let state = startup.zoom_rect.last_mut().unwrap();
    let elapsed = tick.wrapping_sub(state.tick);
    if elapsed == 0 || elapsed > i32::MAX as u32 {
        return PpcImportAction::Yield(u64::from(cycles_per_tick.max(1)));
    }
    zoom_rect_erase(memory, state);
    let next = u32::from(state.step) + elapsed.min(u32::from(state.path.steps()) + 1);
    if next > u32::from(state.path.steps()) {
        let duration = tick.wrapping_sub(state.started_tick);
        startup.zoom_rect.pop();
        if let Some(parent) = startup.zoom_rect.last_mut() {
            parent.tick = parent.tick.wrapping_add(duration);
        }
        return PpcImportAction::Return(ppc_i16_result(0));
    }
    state.step = next as u16;
    state.tick = tick;
    let Some(front) = ppc_live_front_buffer_for_gworld(memory, gworlds, PPC_MAIN_GWORLD) else {
        let duration = tick.wrapping_sub(state.started_tick);
        startup.zoom_rect.pop();
        if let Some(parent) = startup.zoom_rect.last_mut() {
            parent.tick = parent.tick.wrapping_add(duration);
        }
        return PpcImportAction::Return(ppc_i16_result(PPC_PARAM_ERR));
    };
    state.surface = PpcPixMapBits {
        base_addr: front.base_addr,
        row_bytes: front.row_bytes,
        top: 0,
        left: 0,
        bottom: front.height.min(i16::MAX as u32) as i16,
        right: front.width.min(i16::MAX as u32) as i16,
        width: front.width,
        height: front.height,
        depth: front.depth,
    };
    if let Err(error) = zoom_rect_draw(memory, state) {
        let duration = tick.wrapping_sub(state.started_tick);
        startup.zoom_rect.pop();
        if let Some(parent) = startup.zoom_rect.last_mut() {
            parent.tick = parent.tick.wrapping_add(duration);
        }
        return PpcImportAction::Return(ppc_i16_result(error));
    }
    PpcImportAction::Yield(u64::from(cycles_per_tick.max(1)))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum PpcDragCallback {
    Tracking(crate::drag_manager::DragTrackingMessage),
    Receive { handler: DragHandler, window: u32 },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum PpcDragPhase {
    Tracking,
    Receiving,
    Leaving,
}

#[derive(Clone, Debug)]
pub(crate) struct PpcDragTrackingState {
    pub(crate) import_pc: u32,
    pub(crate) entry_sp: u32,
    pub(crate) final_pc: u32,
    pub(crate) restore_rtoc: u32,
    pub(crate) reference: u32,
    pub(crate) sender_window: u32,
    pub(crate) window: Option<u32>,
    pub(crate) handlers: Vec<DragHandler>,
    pub(crate) callbacks: std::collections::VecDeque<PpcDragCallback>,
    pub(crate) awaiting_callback: Option<PpcDragCallback>,
    pub(crate) phase: PpcDragPhase,
    pub(crate) snapshot_pending: bool,
    pub(crate) result: i16,
    outline: Option<PpcDragOutline>,
    image_feedback: Option<PpcDragImageFeedback>,
    image_origin: (i16, i16),
}

impl PpcDragTrackingState {
    pub(crate) fn is_presentation_wait(&self, pc: u32) -> bool {
        pc == self.import_pc
            && self.phase == PpcDragPhase::Tracking
            && !self.snapshot_pending
            && self.awaiting_callback.is_none()
            && self.callbacks.is_empty()
    }
}

#[derive(Clone, Debug)]
struct PpcDragOutline {
    region: Vec<u8>,
    origin: (i16, i16),
    front: PpcFrontBuffer,
    saved_pixels: Vec<(i32, i32, u16)>,
}

fn erase_drag_outline(memory: &mut PpcSectionMem, outline: &mut PpcDragOutline) {
    for (x, y, pixel) in outline.saved_pixels.drain(..) {
        ppc_quickdraw_write_raw_pixel(memory, outline.front, (x, y), pixel);
    }
}

fn refresh_drag_outline(
    memory: &mut PpcSectionMem,
    state: &mut PpcDragTrackingState,
    point: (i16, i16),
    clut: &[[u16; 3]; 256],
) {
    let Some(outline) = state.outline.as_mut() else {
        return;
    };
    erase_drag_outline(memory, outline);
    let Some((top, left, bottom, right)) = ppc_region_storage_bbox(&outline.region) else {
        return;
    };
    let dv = i32::from(point.0) - i32::from(outline.origin.0);
    let dh = i32::from(point.1) - i32::from(outline.origin.1);
    let (Some(black), Some(white)) = (
        ppc_physical_screen_color_pixel(outline.front, PPC_RGB_BLACK, clut),
        ppc_physical_screen_color_pixel(outline.front, PPC_RGB_WHITE, clut),
    ) else {
        return;
    };
    let contains = |v: i32, h: i32| match (i16::try_from(v), i16::try_from(h)) {
        (Ok(v), Ok(h)) => ppc_point_in_region_storage(&outline.region, v, h),
        _ => false,
    };
    for y in (i32::from(top) + dv).max(0)..(i32::from(bottom) + dv).min(outline.front.height as i32)
    {
        for x in
            (i32::from(left) + dh).max(0)..(i32::from(right) + dh).min(outline.front.width as i32)
        {
            let v = y - dv;
            let h = x - dh;
            if contains(v, h)
                && [(v - 1, h), (v + 1, h), (v, h - 1), (v, h + 1)]
                    .into_iter()
                    .any(|(v, h)| !contains(v, h))
            {
                if let Some(pixel) = ppc_quickdraw_read_pixel(memory, outline.front, (x, y)) {
                    outline.saved_pixels.push((x, y, pixel));
                    ppc_quickdraw_write_raw_pixel(
                        memory,
                        outline.front,
                        (x, y),
                        if (x + y).rem_euclid(2) == 0 {
                            black
                        } else {
                            white
                        },
                    );
                }
            }
        }
    }
}

pub(super) fn call_drag_callback(
    cpu: &mut PpcCpu,
    memory: &mut PpcSectionMem,
    state: &mut PpcDragTrackingState,
    callback: PpcDragCallback,
) -> Option<PpcImportAction> {
    let (handler, arguments) = match callback {
        PpcDragCallback::Tracking(message) => (
            message.handler,
            vec![
                u32::from(message.message),
                message.window,
                message.handler.refcon,
                state.reference,
            ],
        ),
        PpcDragCallback::Receive { handler, window } => {
            (handler, vec![window, handler.refcon, state.reference])
        }
    };
    let target = ppc_resolve_callback_target(memory, handler.procedure, handler.rtoc, None)?;
    install_powerpc_call_arguments(cpu, memory, &arguments)?;
    let action = GuestCallEffect::call_guest(
        GuestCallRequest::new(GuestCallTarget {
            isa: GuestIsa::PowerPc,
            entry: target.entry,
            rtoc: target.rtoc,
        }),
        GuestCallContinuation::to_powerpc(
            PPC_GUEST_CALL_RETURN_PC,
            state.import_pc,
            state.restore_rtoc,
            PpcNativeReturnGpr3::Preserve,
        ),
    )
    .into_ppc_import_action()?;
    state.awaiting_callback = Some(callback);
    Some(action)
}

fn drag_window_at_mouse(
    memory: &mut PpcSectionMem,
    gworlds: &[PpcGWorldRecord],
    windows: &SharedProcessWindowList,
    point: (i16, i16),
) -> Option<u32> {
    let height = memory
        .read_u16_be(crate::memory::globals::addr::MBAR_HEIGHT)
        .unwrap_or(0) as i16;
    let (_, window) = dispatch_window::ppc_find_window_at_point(
        memory, gworlds, windows, point.0, point.1, height,
    );
    (window != 0).then_some(window)
}

fn finish_drag(
    memory: &mut PpcSectionMem,
    cpu: &mut PpcCpu,
    stack: &mut Vec<PpcDragTrackingState>,
    error: i16,
) -> PpcImportAction {
    let mut state = stack.pop().unwrap();
    erase_drag_feedback(memory, &mut state);
    cpu.lr = state.final_pc;
    cpu.gpr[2] = state.restore_rtoc;
    PpcImportAction::Return(ppc_i16_result(error))
}

pub(super) fn track_drag(
    cpu: &mut PpcCpu,
    memory: &mut PpcSectionMem,
    manager: &SharedProcessDragManager,
    stack: &mut Vec<PpcDragTrackingState>,
    gworlds: &[PpcGWorldRecord],
    windows: &SharedProcessWindowList,
    input: PpcInputSnapshot,
    events: &mut EventQueue,
    clut: &[[u16; 3]; 256],
) -> PpcImportAction {
    let same_call = stack
        .last()
        .is_some_and(|state| state.import_pc == cpu.pc && state.entry_sp == cpu.gpr[1]);
    if !same_call {
        let reference = cpu.gpr[3];
        if let Err(error) = manager.with_ref(|state| state.get(reference).map(|_| ())) {
            return PpcImportAction::Return(ppc_i16_result(error));
        }
        let event = cpu.gpr[4];
        let Some(bytes) = (event != 0)
            .then(|| ppc_memory_read_bytes(memory, event, 16))
            .flatten()
        else {
            return PpcImportAction::Return(ppc_i16_result(PPC_PARAM_ERR));
        };
        let origin = (
            i16::from_be_bytes([bytes[10], bytes[11]]),
            i16::from_be_bytes([bytes[12], bytes[13]]),
        );
        let outline = if cpu.gpr[5] == 0 {
            None
        } else {
            let Some(region) = ppc_region_storage(memory, cpu.gpr[5]) else {
                return PpcImportAction::Return(ppc_i16_result(PPC_PARAM_ERR));
            };
            let Some(front) = ppc_live_front_buffer_for_gworld(memory, gworlds, PPC_MAIN_GWORLD)
            else {
                return PpcImportAction::Return(ppc_i16_result(PPC_PARAM_ERR));
            };
            Some(PpcDragOutline {
                region,
                origin,
                front,
                saved_pixels: Vec::new(),
            })
        };
        let sender_window = drag_window_at_mouse(memory, gworlds, windows, origin).unwrap_or(0);
        manager.with_mut(|state| {
            let record = state.get_mut(reference).unwrap();
            record.origin = origin;
            record.mouse = origin;
            record.attributes = 0;
        });
        stack.push(PpcDragTrackingState {
            import_pc: cpu.pc,
            entry_sp: cpu.gpr[1],
            final_pc: cpu.lr,
            restore_rtoc: cpu.gpr[2],
            reference,
            sender_window,
            window: None,
            handlers: Vec::new(),
            callbacks: std::collections::VecDeque::new(),
            awaiting_callback: None,
            phase: PpcDragPhase::Tracking,
            snapshot_pending: false,
            result: -128,
            outline,
            image_feedback: None,
            image_origin: origin,
        });
    }
    erase_drag_feedback(memory, stack.last_mut().unwrap());
    let reference = stack.last().unwrap().reference;
    if let Err(error) = manager.with_ref(|state| state.get(reference).map(|_| ())) {
        return finish_drag(memory, cpu, stack, error);
    }
    if stack.last().unwrap().awaiting_callback.is_some() {
        if cpu.lr != cpu.pc {
            return PpcImportAction::Halt;
        }
        let state = stack.last_mut().unwrap();
        if let Some(PpcDragCallback::Receive { .. }) = state.awaiting_callback.take() {
            let error = cpu.gpr[3] as u16 as i16;
            if error == 0 {
                state.result = 0;
                state.callbacks.clear();
            }
        } else {
            state.awaiting_callback = None;
        }
    }
    loop {
        let state = stack.last_mut().unwrap();
        if let Some(callback) = state.callbacks.pop_front() {
            let (handler, window) = match callback {
                PpcDragCallback::Tracking(message) => (message.handler, message.window),
                PpcDragCallback::Receive { handler, window } => (handler, window),
            };
            let live = windows.windows().contains(&window)
                && manager.with_ref(|manager| {
                    manager
                        .handlers_for_window(handler.kind, window)
                        .contains(&handler)
                });
            if !live {
                continue;
            }
            if let Some(action) = call_drag_callback(cpu, memory, state, callback) {
                return action;
            }
            return finish_drag(memory, cpu, stack, PPC_PARAM_ERR);
        }
        match state.phase {
            PpcDragPhase::Leaving => {
                let result = state.result;
                if let Some(index) = events.iter().position(|event| event.what == 2) {
                    events.remove(index);
                }
                return finish_drag(memory, cpu, stack, result);
            }
            PpcDragPhase::Receiving => {
                state.callbacks.extend(
                    crate::drag_manager::tracking_messages(
                        state.window,
                        &state.handlers,
                        None,
                        &[],
                    )
                    .into_iter()
                    .map(PpcDragCallback::Tracking),
                );
                state.phase = PpcDragPhase::Leaving;
                continue;
            }
            PpcDragPhase::Tracking if state.snapshot_pending => {
                state.snapshot_pending = false;
                let image = manager.with_ref(|manager| {
                    manager
                        .get(reference)
                        .ok()
                        .and_then(|record| record.image.clone())
                });
                if image.as_ref().is_none_or(|image| image.flags & 0x10 != 0) {
                    refresh_drag_outline(memory, state, (input.mouse_v, input.mouse_h), clut);
                }
                refresh_drag_image(
                    memory,
                    gworlds,
                    state,
                    image.as_ref(),
                    (input.mouse_v, input.mouse_h),
                    clut,
                );
                return PpcImportAction::Yield(u64::MAX);
            }
            PpcDragPhase::Tracking => {}
        }
        let point = (input.mouse_v, input.mouse_h);
        let window = drag_window_at_mouse(memory, gworlds, windows, point);
        let handlers = window
            .map(|window| {
                manager.with_ref(|manager| {
                    manager.handlers_for_window(DragHandlerKind::Tracking, window)
                })
            })
            .unwrap_or_default();
        manager.with_mut(|manager| {
            let record = manager.get_mut(reference).unwrap();
            record.mouse = point;
            let left = record.attributes & 1 != 0 || window != Some(state.sender_window);
            record.attributes = u32::from(left)
                | if window.is_some() { 2 } else { 0 }
                | if window == Some(state.sender_window) {
                    4
                } else {
                    0
                };
        });
        state.callbacks.extend(
            crate::drag_manager::tracking_messages(
                state.window,
                &state.handlers,
                window,
                &handlers,
            )
            .into_iter()
            .map(PpcDragCallback::Tracking),
        );
        state.window = window;
        state.handlers = handlers;
        state.snapshot_pending = true;
        if input.key_down(53) {
            state.callbacks.clear();
            state.result = -128;
            state.phase = PpcDragPhase::Receiving;
        } else if !input.mouse_button {
            state.phase = PpcDragPhase::Receiving;
            if let Some(window) = window {
                let handlers = manager.with_ref(|manager| {
                    manager.handlers_for_window(DragHandlerKind::Receive, window)
                });
                state.callbacks.extend(
                    handlers
                        .into_iter()
                        .map(|handler| PpcDragCallback::Receive { handler, window }),
                );
            }
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct PpcDragHilite {
    pub(crate) reference: u32,
    pub(crate) port: u32,
    surface: PpcQuickDrawSurface,
    pixels: Vec<(i32, i32, u16)>,
}

pub(super) fn hide_drag_hilite(memory: &mut PpcSectionMem, startup: &mut PpcToolboxStartupState) {
    let Some(hilite) = startup.drag_hilite.take() else {
        return;
    };
    let clips = drag_port_clips(memory, hilite.port);
    for (x, y, pixel) in hilite.pixels {
        let v = (y + i32::from(hilite.surface.top)) as i16;
        let h = (x + i32::from(hilite.surface.left)) as i16;
        if clips
            .iter()
            .all(|storage| ppc_point_in_region_storage(storage, v, h))
        {
            ppc_quickdraw_write_raw_pixel(memory, hilite.surface.front_buffer, (x, y), pixel);
        }
    }
}

fn drag_port_clips(memory: &mut PpcSectionMem, port: u32) -> Vec<Vec<u8>> {
    let mut clips = Vec::new();
    for offset in [
        PPC_CGRAF_PORT_VIS_RGN_OFFSET,
        PPC_CGRAF_PORT_CLIP_RGN_OFFSET,
    ] {
        if let Some(handle) = memory.read_u32_be(port.wrapping_add(offset)) {
            if let Some(storage) = ppc_region_storage(memory, handle) {
                clips.push(storage);
            }
        }
    }
    clips
}

pub(super) fn show_drag_hilite(
    memory: &mut PpcSectionMem,
    gworlds: &[PpcGWorldRecord],
    port: u32,
    colors: &SharedProcessQuickDrawHiliteColors,
    startup: &mut PpcToolboxStartupState,
    reference: u32,
    region: u32,
    inside: bool,
) -> Result<(), i16> {
    let storage = ppc_region_storage(memory, region).ok_or(PPC_PARAM_ERR)?;
    let surface = ppc_live_quickdraw_surface(memory, gworlds, port).ok_or(PPC_PARAM_ERR)?;
    let bytes = surface
        .front_buffer
        .row_bytes
        .checked_mul(surface.front_buffer.height)
        .ok_or(PPC_PARAM_ERR)?;
    if !ppc_memory_can_write_bytes(memory, surface.front_buffer.base_addr, bytes) {
        return Err(PPC_PARAM_ERR);
    }
    let color = ppc_current_hilite_color(memory, port, colors);
    let pixel = ppc_quickdraw_surface_color_pixel(memory, surface, color).ok_or(PPC_PARAM_ERR)?;
    let clips = drag_port_clips(memory, port);
    let mut points = Vec::new();
    if let Some((top, left, bottom, right)) = ppc_region_storage_bbox(&storage) {
        let min_v = (i32::from(top) - 3)
            .max(i32::from(surface.top))
            .max(i32::from(i16::MIN));
        let min_h = (i32::from(left) - 3)
            .max(i32::from(surface.left))
            .max(i32::from(i16::MIN));
        let max_v = (i32::from(bottom) + 3)
            .min(i32::from(surface.top) + surface.front_buffer.height as i32)
            .min(i32::from(i16::MAX) + 1);
        let max_h = (i32::from(right) + 3)
            .min(i32::from(surface.left) + surface.front_buffer.width as i32)
            .min(i32::from(i16::MAX) + 1);
        for v in min_v..max_v {
            for h in min_h..max_h {
                let point = (v as i16, h as i16);
                if clips
                    .iter()
                    .all(|clip| ppc_point_in_region_storage(clip, point.0, point.1))
                    && crate::drag_manager::hilite_pixel(point, inside, |v, h| {
                        ppc_point_in_region_storage(&storage, v, h)
                    })
                {
                    points.push((h - i32::from(surface.left), v - i32::from(surface.top)));
                }
            }
        }
    }
    hide_drag_hilite(memory, startup);
    let mut pixels = Vec::new();
    for (x, y) in points {
        let old =
            ppc_quickdraw_read_pixel(memory, surface.front_buffer, (x, y)).ok_or(PPC_PARAM_ERR)?;
        pixels.push((x, y, old));
    }
    for &(x, y, _) in &pixels {
        ppc_quickdraw_write_raw_pixel(memory, surface.front_buffer, (x, y), pixel);
    }
    startup.drag_hilite = Some(PpcDragHilite {
        reference,
        port,
        surface,
        pixels,
    });
    Ok(())
}

#[derive(Clone, Debug)]
struct PpcDragImageFeedback {
    bits: PpcPixMapBits,
    saved_pixels: Vec<(i32, i32, u32)>,
}

fn erase_drag_feedback(memory: &mut PpcSectionMem, state: &mut PpcDragTrackingState) {
    if let Some(feedback) = state.image_feedback.take() {
        for (x, y, pixel) in feedback.saved_pixels {
            let _ = ppc_write_pixmap_raw_pixel(memory, feedback.bits, x, y, pixel);
        }
    }
    if let Some(outline) = state.outline.as_mut() {
        erase_drag_outline(memory, outline);
    }
}

pub(super) fn set_drag_image(
    cpu: &PpcCpu,
    memory: &mut PpcSectionMem,
    manager: &SharedProcessDragManager,
    gworlds: &[PpcGWorldRecord],
    clut: &[[u16; 3]; 256],
) -> Result<(), i16> {
    let reference = cpu.gpr[3];
    manager.with_ref(|manager| manager.get(reference).map(|_| ()))?;
    let flags = cpu.gpr[7];
    if flags & !0x33 != 0 {
        return Err(PPC_PARAM_ERR);
    }
    let pixmap = memory
        .read_u32_be(cpu.gpr[4])
        .filter(|pointer| *pointer != 0)
        .ok_or(PPC_PARAM_ERR)?;
    let bits = ppc_resolve_pixmap_bits(memory, gworlds, pixmap).ok_or(PPC_PARAM_ERR)?;
    if !matches!(bits.depth, 1 | 2 | 4 | 8 | 16 | 32) {
        return Err(PPC_PARAM_ERR);
    }
    let mask = if cpu.gpr[5] == 0 {
        None
    } else {
        Some(ppc_region_storage(memory, cpu.gpr[5]).ok_or(PPC_PARAM_ERR)?)
    };
    let source_table = ppc_resolve_pixmap_ctable_handle_with_provenance(memory, gworlds, pixmap)
        .handle
        .unwrap_or(0);
    let source_clut = if ppc_indexed_depth_entry_count(bits.depth).is_some() {
        Some(ppc_copy_bits_clut(memory, source_table, clut))
    } else {
        None
    };
    let mut pixels = Vec::new();
    for y in bits.top..bits.bottom {
        for x in bits.left..bits.right {
            if mask
                .as_ref()
                .is_some_and(|mask| !ppc_point_in_region_storage(mask, y, x))
            {
                continue;
            }
            let raw = ppc_read_pixmap_raw_pixel(memory, bits, i32::from(x), i32::from(y))
                .ok_or(PPC_PARAM_ERR)?;
            let color = if bits.depth == 1 && source_table != 0 {
                source_clut
                    .as_ref()
                    .and_then(|clut| clut.get(raw as usize))
                    .copied()
            } else {
                ppc_pixmap_pixel_rgb(bits, raw, source_clut.as_ref())
            }
            .ok_or(PPC_PARAM_ERR)?;
            pixels.try_reserve(1).map_err(|_| PPC_MEM_FULL_ERR)?;
            pixels.push((y, x, color));
        }
    }
    let image = crate::drag_manager::DragImage {
        offset: ((cpu.gpr[6] >> 16) as u16 as i16, cpu.gpr[6] as u16 as i16),
        flags,
        pixels,
    };
    manager.with_mut(|manager| {
        manager.get_mut(reference)?.image = Some(image);
        Ok(())
    })
}

fn refresh_drag_image(
    memory: &mut PpcSectionMem,
    gworlds: &[PpcGWorldRecord],
    state: &mut PpcDragTrackingState,
    image: Option<&crate::drag_manager::DragImage>,
    point: (i16, i16),
    clut: &[[u16; 3]; 256],
) {
    let Some(image) = image else {
        return;
    };
    let Some(front) = ppc_live_front_buffer_for_gworld(memory, gworlds, PPC_MAIN_GWORLD) else {
        return;
    };
    let bits = PpcPixMapBits {
        base_addr: front.base_addr,
        row_bytes: front.row_bytes,
        top: 0,
        left: 0,
        bottom: front.height.min(i16::MAX as u32) as i16,
        right: front.width.min(i16::MAX as u32) as i16,
        width: front.width,
        height: front.height,
        depth: front.depth,
    };
    let origin = state.image_origin;
    let dv = i32::from(point.0) - i32::from(origin.0) + i32::from(image.offset.0);
    let dh = i32::from(point.1) - i32::from(origin.1) + i32::from(image.offset.1);
    let opacity = [35u32, 50, 75, 100][(image.flags & 3) as usize];
    let mut feedback = PpcDragImageFeedback {
        bits,
        saved_pixels: Vec::new(),
    };
    for &(y, x, source) in &image.pixels {
        let y = i32::from(y) + dv;
        let x = i32::from(x) + dh;
        if x < 0 || y < 0 || x >= i32::from(bits.right) || y >= i32::from(bits.bottom) {
            continue;
        }
        let Some(raw) = ppc_read_pixmap_raw_pixel(memory, bits, x, y) else {
            continue;
        };
        let Some(back) = ppc_pixmap_pixel_rgb(bits, raw, Some(clut)) else {
            continue;
        };
        let color = std::array::from_fn(|channel| {
            ((u32::from(source[channel]) * opacity + u32::from(back[channel]) * (100 - opacity))
                / 100) as u16
        });
        let Some(pixel) = ppc_rgb_to_pixmap_pixel(bits, color, Some(clut)) else {
            continue;
        };
        if ppc_write_pixmap_raw_pixel(memory, bits, x, y, pixel).is_some() {
            feedback.saved_pixels.push((x, y, raw));
        }
    }
    state.image_feedback = Some(feedback);
}
