//! Native ICM window alignment, completed through the normal MoveWindow path.
use super::*;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct AlignWindowFrame {
    import_pc: u32,
    caller_lr: u32,
    caller_rtoc: u32,
    arguments: [u32; 8],
    window: u32,
    front: bool,
    offset_h: i32,
    offset_v: i32,
    rect: u32,
}

fn move_arguments(
    cpu: &mut PpcCpu,
    window: u32,
    front: bool,
    offset_h: i32,
    offset_v: i32,
    rect: (i16, i16, i16, i16),
) -> Result<(), ()> {
    let left = i16::try_from(i32::from(rect.1) - offset_h).map_err(|_| ())?;
    let top = i16::try_from(i32::from(rect.0) - offset_v).map_err(|_| ())?;
    cpu.gpr[3] = window;
    cpu.gpr[4] = u32::from(left as u16);
    cpu.gpr[5] = u32::from(top as u16);
    cpu.gpr[6] = u32::from(front);
    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub(super) fn prepare_align_window(
    cpu: &mut PpcCpu,
    memory: &mut PpcSectionMem,
    manager: &mut ProcessNativeMemoryManager,
    heap_cursor: &mut u32,
    last_mem_error: &mut i16,
    gworlds: &[PpcGWorldRecord],
    windows: &SharedProcessWindowList,
    startup: &mut PpcToolboxStartupState,
) -> Result<Option<PpcImportAction>, ()> {
    // Native guest calls return to this same import; nested calls have a normal LR.
    if startup
        .align_window_frames
        .last()
        .is_some_and(|f| f.import_pc == cpu.pc && cpu.lr == cpu.pc)
    {
        let frame = startup.align_window_frames.pop().ok_or(())?;
        let rect = ppc_read_rect(memory, frame.rect);
        manager.dispose_native_ptr(frame.rect);
        ppc_apply_process_native_allocator(manager, memory, heap_cursor, last_mem_error);
        cpu.gpr[3..11].copy_from_slice(&frame.arguments);
        cpu.gpr[2] = frame.caller_rtoc;
        cpu.lr = frame.caller_lr;
        if !windows.windows().contains(&frame.window) {
            return Err(());
        }
        move_arguments(
            cpu,
            frame.window,
            frame.front,
            frame.offset_h,
            frame.offset_v,
            rect.ok_or(())?,
        )?;
        return Ok(None);
    }
    let window = cpu.gpr[3];
    if window == 0 {
        return Ok(Some(PpcImportAction::ReturnPreserve));
    }
    if !windows.windows().contains(&window) {
        return Err(());
    }
    let local = ppc_read_rect(
        memory,
        window.checked_add(PPC_CWINDOW_PORT_RECT_OFFSET).ok_or(())?,
    )
    .ok_or(())?;
    let global = ppc_window_global_content_bounds(memory, gworlds, window).ok_or(())?;
    let selected = if cpu.gpr[5] == 0 {
        local
    } else {
        ppc_read_rect(memory, cpu.gpr[5]).ok_or(())?
    };
    let offset_h = i32::from(selected.1) - i32::from(local.1);
    let offset_v = i32::from(selected.0) - i32::from(local.0);
    let global_rect = (
        i16::try_from(i32::from(global.0) + offset_v).map_err(|_| ())?,
        i16::try_from(i32::from(global.1) + offset_h).map_err(|_| ())?,
        i16::try_from(i32::from(global.0) + i32::from(selected.2) - i32::from(local.0))
            .map_err(|_| ())?,
        i16::try_from(i32::from(global.1) + i32::from(selected.3) - i32::from(local.1))
            .map_err(|_| ())?,
    );
    // Resolve and validate custom code before allocating or changing the window.
    let callback = if cpu.gpr[6] == 0 {
        None
    } else {
        let record = cpu.gpr[6];
        let procedure = memory.read_u32_be(record).ok_or(())?;
        let refcon = memory
            .read_u32_be(record.checked_add(4).ok_or(())?)
            .ok_or(())?;
        let target = ppc_resolve_callback_target(memory, procedure, cpu.gpr[2], None).ok_or(())?;
        if target.entry == 0 || target.entry & 3 != 0 || memory.read_u32_be(target.entry).is_none()
        {
            return Err(());
        }
        Some((target, refcon))
    };
    let aligned = dispatch_quicktime::standard_screen_alignment(memory, global_rect).ok_or(())?;
    let front = cpu.gpr[4] as u8 != 0;
    let Some((target, refcon)) = callback else {
        move_arguments(cpu, window, front, offset_h, offset_v, aligned)?;
        return Ok(None);
    };
    let rect = manager.new_native_ptr(memory, 8, true);
    ppc_apply_process_native_allocator(manager, memory, heap_cursor, last_mem_error);
    if rect == 0 {
        return Err(());
    }
    let frame = AlignWindowFrame {
        import_pc: cpu.pc,
        caller_lr: cpu.lr,
        caller_rtoc: cpu.gpr[2],
        arguments: cpu.gpr[3..11].try_into().unwrap(),
        window,
        front,
        offset_h,
        offset_v,
        rect,
    };
    if ppc_write_rect(memory, rect, aligned.0, aligned.1, aligned.2, aligned.3).is_none()
        || install_powerpc_call_arguments(cpu, memory, &[rect, refcon]).is_none()
    {
        manager.dispose_native_ptr(rect);
        ppc_apply_process_native_allocator(manager, memory, heap_cursor, last_mem_error);
        cpu.gpr[3..11].copy_from_slice(&frame.arguments);
        return Err(());
    }
    startup.align_window_frames.push(frame.clone());
    let action = GuestCallEffect::call_guest(
        GuestCallRequest::new(GuestCallTarget {
            isa: GuestIsa::PowerPc,
            entry: target.entry,
            rtoc: target.rtoc,
        }),
        GuestCallContinuation::to_powerpc(
            PPC_GUEST_CALL_RETURN_PC,
            frame.import_pc,
            frame.caller_rtoc,
            PpcNativeReturnGpr3::Set(window),
        ),
    )
    .into_ppc_import_action()
    .ok_or(())?;
    Ok(Some(action))
}
