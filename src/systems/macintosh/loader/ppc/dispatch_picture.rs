//! Typed Picture Manager dispatch for PowerPC imports.

use super::*;
use crate::trap::pict::PictureCommand;
use crate::trap::picture_callbacks::indexed_picture_bits;
use std::collections::VecDeque;

pub(super) struct PpcPictureDispatchContext<'a> {
    pub(super) binding: &'a PpcImportBinding,
    pub(super) cpu: &'a mut PpcCpu,
    pub(super) memory: &'a mut PpcSectionMem,
    pub(super) process_memory_manager: &'a mut ProcessNativeMemoryManager,
    pub(super) heap_cursor: &'a mut u32,
    pub(super) heap_limit: u32,
    pub(super) last_mem_error: &'a mut i16,
    pub(super) handles: &'a mut Vec<PpcHandleRecord>,
    pub(super) vfs_resources: &'a [PpcVfsResourceRecord],
    pub(super) gworlds: &'a [PpcGWorldRecord],
    pub(super) current_gworld: u32,
    pub(super) screen_clut: &'a [[u16; 3]; 256],
    pub(super) callback_stack: &'a mut Vec<PpcPictureCallbackState>,
    pub(super) color_manager_clut: &'a mut [[u16; 3]; 256],
}

pub(super) fn dispatch_picture_import(
    context: PpcPictureDispatchContext<'_>,
) -> Option<PpcImportAction> {
    let PpcPictureDispatchContext {
        binding,
        cpu,
        memory,
        process_memory_manager,
        heap_cursor,
        heap_limit,
        last_mem_error,
        handles,
        vfs_resources,
        gworlds,
        current_gworld,
        screen_clut,
        color_manager_clut,
        callback_stack,
    } = context;

    match binding.dispatcher_target {
        PpcImportDispatcherTarget::GetPictInfo => Some(PpcImportAction::Return(ppc_i16_result(
            ppc_get_pict_info(cpu, memory, handles),
        ))),
        PpcImportDispatcherTarget::DrawPicture => {
            let resumed = callback_stack
                .last()
                .is_some_and(|frame| frame.import_pc == cpu.pc && cpu.lr == cpu.pc);
            let mut frame = if resumed {
                let mut frame = callback_stack.pop().unwrap();
                if let Some(bits) = frame.bits.as_mut() {
                    // The bitsProc returned; its PixMap and rects are dead.
                    bits.release(process_memory_manager, memory);
                    ppc_apply_process_native_allocator(
                        process_memory_manager,
                        memory,
                        heap_cursor,
                        last_mem_error,
                    );
                } else if let Some(bytes) =
                    ppc_memory_read_bytes(memory, frame.buffer, frame.requested)
                {
                    frame.bytes.extend_from_slice(&bytes);
                } else {
                    frame.failed = true;
                }
                Some(frame)
            } else {
                let procs = memory.read_u32_be(current_gworld + 104).unwrap_or(0);
                let slot = |memory: &mut PpcSectionMem, offset: u32| {
                    if procs == 0 {
                        0
                    } else {
                        memory.read_u32_be(procs + offset).unwrap_or(0)
                    }
                };
                let callback = slot(memory, 44);
                // SetStdCProcs installs StdGetPic, which reads the picture
                // handle exactly as the built-in path does.
                let standard = ppc_logical_trap_address(
                    memory,
                    dispatch_quickdraw::PPC_STD_GET_PIC_TRAP,
                    true,
                )
                .is_some_and(|std_get_pic| std_get_pic == callback);
                let target = (callback != 0 && !standard)
                    .then(|| ppc_resolve_callback_target(memory, callback, cpu.gpr[2], None))
                    .flatten();
                let bits_proc = slot(memory, 32);
                let bits_target = picture_bits_target(memory, bits_proc, cpu.gpr[2]);
                let comment_proc = slot(memory, 36);
                let standard_comments = comment_proc == 0
                    || ppc_logical_trap_address(memory, PPC_STD_COMMENT_TRAP, true)
                        == Some(comment_proc);
                let destination =
                    ppc_memory_read_bytes(memory, cpu.gpr[4], 8).map(|bytes| picture_rect(&bytes));
                let entry = |bytes: Vec<u8>, target: Option<PpcCallbackTarget>, buffer: u32| {
                    PpcPictureCallbackState {
                        import_pc: cpu.pc,
                        final_pc: cpu.lr,
                        rtoc: cpu.gpr[2],
                        picture: cpu.gpr[3],
                        destination: cpu.gpr[4],
                        gworld: current_gworld,
                        target,
                        buffer,
                        requested: 0,
                        bytes,
                        failed: false,
                        bits_target: bits_target.zip(destination),
                        standard_comments,
                        bits: None,
                    }
                };
                if let Some(target) = target {
                    ppc_handle_bytes(memory, handles, cpu.gpr[3]).and_then(|bytes| {
                        let header = bytes.get(..10)?.to_vec();
                        let buffer = process_memory_manager.new_native_ptr(memory, 32767, true);
                        ppc_apply_process_native_allocator(
                            process_memory_manager,
                            memory,
                            heap_cursor,
                            last_mem_error,
                        );
                        (buffer != 0).then(|| entry(header, Some(target), buffer))
                    })
                } else if bits_target.is_some() && destination.is_some() {
                    ppc_handle_bytes(memory, handles, cpu.gpr[3]).map(|bytes| entry(bytes, None, 0))
                } else {
                    None
                }
            };
            if let Some(mut frame) = frame.take() {
                let progress = pict::picture_stream_progress(&frame.bytes);
                if frame.bits.is_none() && !frame.failed {
                    if let (Some(_), pict::PictureStreamProgress::Need(total)) =
                        (frame.target, &progress)
                    {
                        let total = *total;
                        // Bound corrupt streams and honour StdGetPic's signed short count.
                        if total <= 32 * 1024 * 1024 {
                            frame.requested = (total - frame.bytes.len()).min(32767) as u32;
                            if let Some(action) = picture_call_callback(cpu, memory, &frame) {
                                callback_stack.push(frame);
                                return Some(action);
                            }
                        }
                    }
                    if let (Some((target, destination)), pict::PictureStreamProgress::Complete(_)) =
                        (frame.bits_target, &progress)
                    {
                        frame.bits = plan_picture_bits(
                            &frame.bytes,
                            destination,
                            target,
                            frame.standard_comments,
                        );
                        if ppc_hle_trace_enabled() {
                            eprintln!(
                                "[PPC-TRACE] DrawPicture handle=${:08X} bitsProc=${:08X} playback={}",
                                frame.picture,
                                target.entry,
                                frame.bits.as_ref().map_or_else(
                                    || "native".to_owned(),
                                    |bits| format!("{} bitmap(s)", bits.commands.len())
                                ),
                            );
                        }
                    }
                }
                if let Some(bits) = frame.bits.as_mut() {
                    if let Some(action) = picture_call_bits(
                        cpu,
                        memory,
                        process_memory_manager,
                        heap_cursor,
                        last_mem_error,
                        frame.rtoc,
                        bits,
                    ) {
                        callback_stack.push(frame);
                        return Some(action);
                    }
                }
                cpu.lr = frame.final_pc;
                cpu.gpr[2] = frame.rtoc;
                cpu.gpr[3] = frame.picture;
                cpu.gpr[4] = frame.destination;
                if frame.bits.is_none()
                    && !frame.failed
                    && matches!(progress, pict::PictureStreamProgress::Complete(_))
                {
                    let _ = ppc_draw_picture_bytes(
                        cpu,
                        memory,
                        vfs_resources,
                        gworlds,
                        frame.gworld,
                        screen_clut,
                        color_manager_clut,
                        &frame.bytes,
                    );
                }
                if frame.buffer != 0 {
                    process_memory_manager.dispose_native_ptr(frame.buffer);
                    ppc_apply_process_native_allocator(
                        process_memory_manager,
                        memory,
                        heap_cursor,
                        last_mem_error,
                    );
                }
                return Some(PpcImportAction::ReturnPreserve);
            }
            let _ = ppc_draw_picture(
                cpu,
                memory,
                handles,
                vfs_resources,
                gworlds,
                current_gworld,
                screen_clut,
                color_manager_clut,
            );
            Some(PpcImportAction::ReturnPreserve)
        }
        PpcImportDispatcherTarget::KillPicture => {
            let mut allocator = PpcProcessAllocatorView {
                memory_manager: process_memory_manager,
            };
            ppc_kill_picture(
                cpu,
                &mut allocator,
                memory,
                heap_cursor,
                heap_limit,
                last_mem_error,
                handles,
            );
            Some(PpcImportAction::ReturnPreserve)
        }
        _ => None,
    }
}

#[derive(Debug, Clone)]
pub(crate) struct PpcPictureCallbackState {
    import_pc: u32,
    final_pc: u32,
    rtoc: u32,
    picture: u32,
    destination: u32,
    gworld: u32,
    /// Replacement getPicProc streaming the picture; `None` when the picture
    /// is read from its handle (StdGetPic).
    target: Option<PpcCallbackTarget>,
    buffer: u32,
    requested: u32,
    bytes: Vec<u8>,
    failed: bool,
    /// Replacement bitsProc and the DrawPicture dstRect, captured at entry.
    bits_target: Option<(PpcCallbackTarget, [i16; 4])>,
    standard_comments: bool,
    bits: Option<PpcPictureBitsPlayback>,
}

/// StdComment, the commentProc default that ignores picture comments.
const PPC_STD_COMMENT_TRAP: u16 = 0xA8F1;
/// StdBits, the bitsProc default.
const PPC_STD_BITS_TRAP: u16 = 0xA8EB;

fn picture_rect(bytes: &[u8]) -> [i16; 4] {
    std::array::from_fn(|index| i16::from_be_bytes([bytes[index * 2], bytes[index * 2 + 1]]))
}

/// A replaced PowerPC bitsProc (CQDProcs +32). The standard StdBits entry
/// that SetStdCProcs installs, NIL and 68K routines keep the native renderer.
fn picture_bits_target(
    memory: &mut PpcSectionMem,
    bits_proc: u32,
    rtoc: u32,
) -> Option<PpcCallbackTarget> {
    if bits_proc == 0
        || ppc_logical_trap_address(memory, PPC_STD_BITS_TRAP, true) == Some(bits_proc)
    {
        return None;
    }
    ppc_resolve_callback_target(memory, bits_proc, rtoc, None)
}

/// DrawPicture passes each picture PixMap to the port's bitsProc (IM:V
/// V-89..V-91; develop 20 pp. 63-64 replaces bitsProc to take over picture
/// PixMaps). Applications use this to copy indexed pixels into their own
/// offscreen palettes, so bypassing the callback colour-matches the pixels
/// into the wrong indices.
#[derive(Debug, Clone)]
struct PpcPictureBitsPlayback {
    target: PpcCallbackTarget,
    commands: VecDeque<PictureCommand>,
    source: [i16; 4],
    destination: [i16; 4],
    pointers: Vec<u32>,
    color_table: u32,
}

impl PpcPictureBitsPlayback {
    fn release(&mut self, manager: &mut ProcessNativeMemoryManager, memory: &mut PpcSectionMem) {
        for pointer in self.pointers.drain(..) {
            manager.dispose_native_ptr(pointer);
        }
        if self.color_table != 0 {
            manager.dispose_native_handle(memory, self.color_table);
            self.color_table = 0;
        }
    }
}

/// Replay a picture through its bitsProc only when every operation is either
/// a bitmap the callback receives or picture state with no drawing effect
/// here: header, hilite colour, comments ignored by StdComment, and a
/// rectangular clip that covers the source frame. Other streams keep the
/// native renderer rather than being partially executed.
fn plan_picture_bits(
    bytes: &[u8],
    destination: [i16; 4],
    target: PpcCallbackTarget,
    standard_comments: bool,
) -> Option<PpcPictureBitsPlayback> {
    let commands = pict::picture_commands(bytes)?;
    let mut source = picture_rect(bytes.get(2..10)?);
    let mut bitmaps = VecDeque::new();
    for command in commands {
        match command.opcode {
            0x00 | 0x11 | 0x1e | 0xff => {}
            0x0c00 => {
                if command.data.get(..2) == Some(&[0xff, 0xfe][..]) {
                    source = picture_rect(command.data.get(12..20)?);
                }
            }
            0x01 => {
                let clip = picture_rect(command.data.get(2..10)?);
                if command.data.len() != 10
                    || clip[0] > source[0]
                    || clip[1] > source[1]
                    || clip[2] < source[2]
                    || clip[3] < source[3]
                {
                    return None;
                }
            }
            0xa0 | 0xa1 if standard_comments => {}
            0x98 => {
                indexed_picture_bits(&command)?;
                bitmaps.push_back(command);
            }
            _ => return None,
        }
    }
    let empty = |rect: [i16; 4]| rect[2] <= rect[0] || rect[3] <= rect[1];
    if bitmaps.is_empty() || empty(source) || empty(destination) {
        return None;
    }
    Some(PpcPictureBitsPlayback {
        target,
        commands: bitmaps,
        source,
        destination,
        pointers: Vec::new(),
        color_table: 0,
    })
}

/// Call the bitsProc for the next picture bitmap with
/// `(srcBits, srcRect, dstRect, mode, maskRgn)`: a PixMap whose baseAddr
/// holds the unpacked source indices and whose pmTable is the picture's own
/// ColorTable, the source rect, and the dstRect mapped from the picture frame
/// into DrawPicture's dstRect. Returns `None` once every bitmap is done.
fn picture_call_bits(
    cpu: &mut PpcCpu,
    memory: &mut PpcSectionMem,
    manager: &mut ProcessNativeMemoryManager,
    heap_cursor: &mut u32,
    last_mem_error: &mut i16,
    rtoc: u32,
    bits: &mut PpcPictureBitsPlayback,
) -> Option<PpcImportAction> {
    while let Some(command) = bits.commands.pop_front() {
        let bitmap = indexed_picture_bits(&command)?;
        let pixels = manager.new_native_ptr(memory, bitmap.pixels.len() as u32, false);
        let pixmap = manager.new_native_ptr(memory, 50, true);
        let rects = manager.new_native_ptr(memory, 16, true);
        let color_table = manager.copy_bytes_to_new_native_handle(memory, &bitmap.color_table);
        ppc_apply_process_native_allocator(manager, memory, heap_cursor, last_mem_error);
        bits.pointers
            .extend([pixels, pixmap, rects].into_iter().filter(|ptr| *ptr != 0));
        bits.color_table = color_table;
        let table = memory.read_u32_be(color_table).unwrap_or(0);
        let mut written = pixels != 0 && pixmap != 0 && rects != 0 && table != 0;
        if written {
            written &= memory.write_bytes(pixels, &bitmap.pixels).is_some();
            written &= memory.write_u32_be(pixmap, pixels).is_some();
            written &= memory
                .write_bytes(pixmap + 4, &bitmap.pixmap_header)
                .is_some();
            written &= memory.write_u32_be(pixmap + 42, color_table).is_some();
            for index in 0..4 {
                let axis = index % 2;
                let from = i64::from(bits.source[axis]);
                let extent = i64::from(bits.source[axis + 2]) - from;
                let target = i64::from(bits.destination[axis]);
                let target_extent = i64::from(bits.destination[axis + 2]) - target;
                let mapped =
                    target + (i64::from(bitmap.destination[index]) - from) * target_extent / extent;
                let mapped = mapped.clamp(i64::from(i16::MIN), i64::from(i16::MAX)) as i16;
                written &= memory
                    .write_u16_be(rects + index as u32 * 2, bitmap.source[index] as u16)
                    .is_some();
                written &= memory
                    .write_u16_be(rects + 8 + index as u32 * 2, mapped as u16)
                    .is_some();
            }
        }
        if !written
            || install_powerpc_call_arguments(
                cpu,
                memory,
                &[pixmap, rects, rects + 8, u32::from(bitmap.mode), 0],
            )
            .is_none()
        {
            // Like QuickDraw without memory for the PixMap, skip the bitmap.
            bits.release(manager, memory);
            ppc_apply_process_native_allocator(manager, memory, heap_cursor, last_mem_error);
            continue;
        }
        return GuestCallEffect::call_guest(
            GuestCallRequest::new(GuestCallTarget {
                isa: GuestIsa::PowerPc,
                entry: bits.target.entry,
                rtoc: bits.target.rtoc,
            }),
            GuestCallContinuation::to_powerpc(
                PPC_GUEST_CALL_RETURN_PC,
                cpu.pc,
                rtoc,
                PpcNativeReturnGpr3::Preserve,
            ),
        )
        .into_ppc_import_action();
    }
    None
}

fn picture_call_callback(
    cpu: &mut PpcCpu,
    memory: &mut PpcSectionMem,
    frame: &PpcPictureCallbackState,
) -> Option<PpcImportAction> {
    install_powerpc_call_arguments(cpu, memory, &[frame.buffer, frame.requested])?;
    GuestCallEffect::call_guest(
        GuestCallRequest::new(GuestCallTarget {
            isa: GuestIsa::PowerPc,
            entry: frame.target?.entry,
            rtoc: frame.target?.rtoc,
        }),
        GuestCallContinuation::to_powerpc(
            PPC_GUEST_CALL_RETURN_PC,
            cpu.pc,
            frame.rtoc,
            PpcNativeReturnGpr3::Preserve,
        ),
    )
    .into_ppc_import_action()
}
