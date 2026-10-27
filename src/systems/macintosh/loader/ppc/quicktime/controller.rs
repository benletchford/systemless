//! Movie-controller action filters. Movies.h defines the WithRefCon filter as
//! Boolean (MovieController, short action, void *params, long refCon).
//! QuickTime Components: true consumes the action; false continues processing.

use super::super::*;
use super::*;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct FilterFrame {
    import_pc: u32,
    caller_lr: u32,
    caller_rtoc: u32,
    arguments: [u32; 8],
    event_record: u32,
    reports_handled: bool,
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn filter_controller_action(
    operation: PpcQuickTimeCompatibilityOperation,
    cpu: &mut PpcCpu,
    memory: &mut PpcSectionMem,
    manager: &mut ProcessNativeMemoryManager,
    heap_cursor: &mut u32,
    last_mem_error: &mut i16,
    quicktime: &mut PpcQuickTimeState,
) -> Option<PpcImportAction> {
    use super::callbacks::ContinuationFrame;
    if quicktime.callback_frames.last().is_some_and(|frame| {
        matches!(frame,
        ContinuationFrame::ControllerFilter(frame)
            if frame.import_pc == cpu.pc && cpu.lr == cpu.pc)
    }) {
        let Some(ContinuationFrame::ControllerFilter(frame)) = quicktime.callback_frames.pop()
        else {
            unreachable!("checked controller filter continuation");
        };
        let handled = cpu.gpr[3] as u8 != 0;
        cpu.gpr[3..11].copy_from_slice(&frame.arguments);
        cpu.gpr[2] = frame.caller_rtoc;
        cpu.lr = frame.caller_lr;
        if frame.event_record != 0 {
            if !handled {
                // The filter receives the event itself, so edits to its
                // parameters must reach the controller's default handler.
                let event = frame.event_record;
                cpu.gpr[4] = memory.read_u32_be(event + 2).unwrap_or(cpu.gpr[4]);
                cpu.gpr[5] = memory.read_u32_be(event + 10).unwrap_or(cpu.gpr[5]);
                cpu.gpr[6] = memory.read_u32_be(event + 6).unwrap_or(cpu.gpr[6]);
                cpu.gpr[7] = memory
                    .read_u16_be(event + 14)
                    .map(u32::from)
                    .unwrap_or(cpu.gpr[7]);
            }
            manager.dispose_native_ptr(frame.event_record);
            ppc_apply_process_native_allocator(manager, memory, heap_cursor, last_mem_error);
        }
        return handled.then_some(PpcImportAction::Return(u32::from(frame.reports_handled)));
    }
    use PpcQuickTimeCompatibilityOperation as Op;
    let (action, mut parameter, reports_handled) = match operation {
        Op::MCDoAction => (cpu.gpr[4] as u16 as i16 as i32 as u32, cpu.gpr[5], false),
        Op::MCIdle => (1, 0, false),
        Op::MCDraw => (2, cpu.gpr[4], true),
        Op::MCClick => (5, 0, true),
        _ => return None,
    };
    let controller = *quicktime
        .movie_controllers
        .iter()
        .find(|record| record.handle == cpu.gpr[3])?;
    if controller.action_filter == 0 {
        return None;
    }
    let Some(target) = ppc_resolve_callback_target(
        memory,
        controller.action_filter,
        controller.action_filter_rtoc,
        None,
    )
    .filter(|target| {
        target.entry != 0 && target.entry & 3 == 0 && memory.read_u32_be(target.entry).is_some()
    }) else {
        return Some(PpcImportAction::Return(ppc_i16_result(PPC_PARAM_ERR)));
    };
    let mut frame = FilterFrame {
        import_pc: cpu.pc,
        caller_lr: cpu.lr,
        caller_rtoc: cpu.gpr[2],
        arguments: cpu.gpr[3..11].try_into().unwrap(),
        event_record: 0,
        reports_handled,
    };
    if operation == Op::MCClick {
        let event = manager.new_native_ptr(memory, 16, true);
        ppc_apply_process_native_allocator(manager, memory, heap_cursor, last_mem_error);
        if event == 0 {
            return Some(PpcImportAction::Return(ppc_i16_result(PPC_MEM_FULL_ERR)));
        }
        let _ = memory.write_u16_be(event, 1); // mouseDown
        let _ = memory.write_u32_be(event + 2, cpu.gpr[4]); // window
        let _ = memory.write_u32_be(event + 6, cpu.gpr[6]); // when
        let _ = memory.write_u32_be(event + 10, cpu.gpr[5]); // global Point
        let _ = memory.write_u16_be(event + 14, cpu.gpr[7] as u16);
        frame.event_record = event;
        parameter = event;
    }
    if install_powerpc_call_arguments(
        cpu,
        memory,
        &[
            controller.handle,
            action,
            parameter,
            controller.action_ref_con,
        ],
    )
    .is_none()
    {
        cpu.gpr[3..11].copy_from_slice(&frame.arguments);
        if frame.event_record != 0 {
            manager.dispose_native_ptr(frame.event_record);
            ppc_apply_process_native_allocator(manager, memory, heap_cursor, last_mem_error);
        }
        return Some(PpcImportAction::Return(ppc_i16_result(PPC_PARAM_ERR)));
    }
    quicktime
        .callback_frames
        .push(ContinuationFrame::ControllerFilter(frame.clone()));
    GuestCallEffect::call_guest(
        GuestCallRequest::new(GuestCallTarget {
            isa: GuestIsa::PowerPc,
            entry: target.entry,
            rtoc: target.rtoc,
        }),
        GuestCallContinuation::to_powerpc(
            PPC_GUEST_CALL_RETURN_PC,
            frame.import_pc,
            frame.caller_rtoc,
            PpcNativeReturnGpr3::Preserve,
        ),
    )
    .into_ppc_import_action()
}
