//! Typed Time and Vertical Retrace (VBL) Manager dispatch for PowerPC imports.

use super::*;

pub(super) struct PpcTimeDispatchContext<'a> {
    pub(super) binding: &'a PpcImportBinding,
    pub(super) cpu: &'a mut PpcCpu,
    pub(super) memory: &'a mut PpcSectionMem,
    pub(super) timer_tasks: &'a SharedProcessTimerTasks,
    pub(super) vbl_tasks: &'a SharedProcessVblTasks,
    pub(super) callback_scheduling: &'a SharedProcessCallbackScheduling,
    pub(super) tick_count: u32,
}

pub(super) fn dispatch_time_import(context: PpcTimeDispatchContext<'_>) -> Option<PpcImportAction> {
    let PpcTimeDispatchContext {
        binding,
        cpu,
        memory,
        timer_tasks,
        vbl_tasks,
        callback_scheduling,
        tick_count,
    } = context;

    match binding.dispatcher_target {
        PpcImportDispatcherTarget::InsTime | PpcImportDispatcherTarget::InsXTime => {
            timer_tasks.with_mut(|timer_tasks| {
                ppc_install_time_task(
                    memory,
                    timer_tasks,
                    callback_scheduling,
                    cpu.gpr[3],
                    binding.dispatcher_target == PpcImportDispatcherTarget::InsXTime,
                );
            });
            Some(PpcImportAction::ReturnPreserve)
        }
        PpcImportDispatcherTarget::PrimeTime => {
            timer_tasks.with_mut(|timer_tasks| {
                ppc_prime_time_task(
                    memory,
                    timer_tasks,
                    callback_scheduling,
                    cpu.gpr[3],
                    cpu.gpr[4] as i32,
                    tick_count,
                );
            });
            Some(PpcImportAction::ReturnPreserve)
        }
        PpcImportDispatcherTarget::RmvTime => {
            callback_scheduling
                .advance_current_subtick_min(u64::from(tick_count) * 1_000_000);
            timer_tasks.with_mut(|timer_tasks| {
                ppc_remove_time_task(memory, timer_tasks, callback_scheduling, cpu.gpr[3]);
            });
            Some(PpcImportAction::ReturnPreserve)
        }
        PpcImportDispatcherTarget::VInstall
        | PpcImportDispatcherTarget::VRemove
        | PpcImportDispatcherTarget::SlotVInstall
        | PpcImportDispatcherTarget::SlotVRemove => {
            let task_ptr = cpu.gpr[3];
            if ppc_hle_trace_enabled() {
                let q_type = memory.read_u16_be(task_ptr + 4).unwrap_or(0);
                let vbl_addr = memory.read_u32_be(task_ptr + 6).unwrap_or(0);
                let vbl_count = memory.read_u16_be(task_ptr + 10).unwrap_or(0);
                let vbl_phase = memory.read_u16_be(task_ptr + 12).unwrap_or(0);
                let callback_target =
                    ppc_resolve_callback_target(memory, vbl_addr, cpu.gpr[2], None);
                let callback_text = callback_target
                    .map(|target| {
                        format!(
                            " entry=${:08X} rtoc=${:08X} procInfo=${:08X} flags=${:04X}",
                            target.entry, target.rtoc, target.proc_info, target.routine_flags
                        )
                    })
                    .unwrap_or_default();
                eprintln!(
                    "[PPC-TRACE] {} task=${:08X} qType={} addr=${:08X} count={} phase={}{}",
                    binding.symbol_name,
                    task_ptr,
                    q_type,
                    vbl_addr,
                    vbl_count,
                    vbl_phase,
                    callback_text
                );
            }
            // Inside Macintosh: Processes (1994), pp. 4-22–4-24:
            // SlotVInstall and SlotVRemove use the same VBLTask layout and
            // scheduling rules as their system-based counterparts. Systemless
            // exposes one active display queue, so both feed that queue.
            let installing = matches!(
                binding.dispatcher_target,
                PpcImportDispatcherTarget::VInstall | PpcImportDispatcherTarget::SlotVInstall
            );
            let result = vbl_tasks.with_mut(|vbl_tasks| {
                if installing {
                    let slot = matches!(
                        binding.dispatcher_target,
                        PpcImportDispatcherTarget::SlotVInstall
                    )
                    .then_some(cpu.gpr[4] as i16);
                    ppc_install_vbl_task(memory, vbl_tasks, task_ptr, slot)
                } else {
                    ppc_remove_vbl_task(memory, vbl_tasks, task_ptr)
                }
            });
            Some(PpcImportAction::Return(ppc_i16_result(result)))
        }
        _ => None,
    }
}

pub(crate) fn ppc_install_vbl_task(
    memory: &mut PpcSectionMem,
    vbl_tasks: &mut Vec<PpcVblTaskRecord>,
    task_ptr: u32,
    slot: Option<i16>,
) -> i16 {
    if task_ptr == 0 || memory.read_u16_be(task_ptr + 4).unwrap_or(0) != 1 {
        return -2;
    }
    let vbl_count = memory.read_u16_be(task_ptr + 10).unwrap_or(0);
    let vbl_phase = memory.read_u16_be(task_ptr + 12).unwrap_or(0);
    let _ = memory.write_u16_be(task_ptr + 10, vbl_count.wrapping_add(vbl_phase));

    vbl_tasks.retain(|task| task.task_ptr != task_ptr);
    vbl_tasks.push(PpcVblTaskRecord {
        task_ptr,
        architecture: CallbackTaskArchitecture::PowerPc,
        slot,
        pending: false,
    });
    ppc_sync_vbl_task_links(memory, vbl_tasks);
    PPC_NO_ERR
}

pub(crate) fn ppc_install_time_task(
    memory: &mut PpcSectionMem,
    timer_tasks: &mut Vec<PpcTimerTaskRecord>,
    scheduling: &SharedProcessCallbackScheduling,
    task_ptr: u32,
    extended: bool,
) {
    if task_ptr == 0 || !ppc_memory_can_write_bytes(memory, task_ptr, 14) {
        return;
    }
    // Inside Macintosh: Processes (1994), pp. 3-17--3-19: InsTime inserts
    // the record inactive and clears the active high bit in qType.
    let q_type = memory.read_u16_be(task_ptr + 4).unwrap_or(0) & 0x7fff;
    let _ = memory.write_u32_be(task_ptr, 0);
    let _ = memory.write_u16_be(task_ptr + 4, q_type);
    if !extended || memory.read_u32_be(task_ptr + 14).unwrap_or(0) == 0 {
        scheduling.remove_extended_wakeup(task_ptr);
    }
    timer_tasks.retain(|task| task.task_ptr != task_ptr);
    timer_tasks.push(PpcTimerTaskRecord {
        task_ptr,
        architecture: CallbackTaskArchitecture::PowerPc,
        extended,
        callback: memory.read_u32_be(task_ptr + 6).unwrap_or(0),
        active: false,
        fire_at_tick: 0,
        fire_at_subtick: 0,
        last_fired_tick: None,
    });
    ppc_sync_time_task_links(memory, timer_tasks);
    if ppc_timer_trace_enabled() {
        eprintln!(
            "[TIMER-PPC] install task=${task_ptr:08X} callback=${:08X}",
            memory.read_u32_be(task_ptr + 6).unwrap_or(0)
        );
    }
}

pub(crate) fn ppc_prime_time_task(
    memory: &mut PpcSectionMem,
    timer_tasks: &mut [PpcTimerTaskRecord],
    scheduling: &SharedProcessCallbackScheduling,
    task_ptr: u32,
    count: i32,
    current_tick: u32,
) {
    if task_ptr == 0 || !ppc_memory_can_write_bytes(memory, task_ptr, 14) {
        return;
    }
    // Inside Macintosh: Processes (1994), pp. 3-19--3-20: PrimeTime stores
    // the requested delay and marks the installed task active.
    let q_type = memory.read_u16_be(task_ptr + 4).unwrap_or(0) | 0x8000;
    let _ = memory.write_u16_be(task_ptr + 4, q_type);
    let _ = memory.write_u32_be(task_ptr + 10, count as u32);
    const SUBTICKS_PER_TICK: u64 = 1_000_000;
    let requested_delay_subticks = if count == 0 {
        0
    } else if count > 0 {
        u64::from(count as u32) * 60_000
    } else {
        (u64::from(count.unsigned_abs()) * 60).max(1)
    };
    let current_subtick = scheduling
        .advance_current_subtick_min(u64::from(current_tick) * SUBTICKS_PER_TICK);
    if let Some(task) = timer_tasks
        .iter_mut()
        .find(|task| task.task_ptr == task_ptr)
    {
        task.callback = memory.read_u32_be(task_ptr + 6).unwrap_or(0);
        task.active = true;
        task.fire_at_subtick = if task.extended {
            let prior_wakeup = if memory.read_u32_be(task_ptr + 14).unwrap_or(0) == 0 {
                None
            } else {
                scheduling.extended_wakeup(task_ptr)
            };
            let intended_wakeup = prior_wakeup
                .unwrap_or(current_subtick)
                .saturating_add(requested_delay_subticks);
            scheduling.set_extended_wakeup(task_ptr, intended_wakeup);
            let opaque_wakeup = ((intended_wakeup / 60) as u32).max(1);
            let _ = memory.write_u32_be(task_ptr + 14, opaque_wakeup);
            intended_wakeup.max(current_subtick)
        } else {
            let delay_subticks = if count == 0 {
                SUBTICKS_PER_TICK
            } else {
                requested_delay_subticks
            };
            current_subtick.saturating_add(delay_subticks)
        };
        task.fire_at_tick = task.fire_at_subtick.div_ceil(SUBTICKS_PER_TICK) as u32;
        if ppc_timer_trace_enabled() {
            eprintln!(
                "[TIMER-PPC] prime task=${task_ptr:08X} count={count} now={current_tick} fire_at={} callback=${:08X}",
                task.fire_at_tick,
                task.callback,
            );
        }
    } else if ppc_timer_trace_enabled() {
        eprintln!(
            "[TIMER-PPC] prime ignored uninstalled task=${task_ptr:08X} count={count} now={current_tick}"
        );
    }
}

pub(crate) fn ppc_remove_time_task(
    memory: &mut PpcSectionMem,
    timer_tasks: &mut Vec<PpcTimerTaskRecord>,
    scheduling: &SharedProcessCallbackScheduling,
    task_ptr: u32,
) {
    if task_ptr == 0 || !ppc_memory_can_write_bytes(memory, task_ptr, 14) {
        return;
    }
    let current_subtick = scheduling.current_subtick();
    let remaining_subticks = timer_tasks
        .iter()
        .find(|task| task.task_ptr == task_ptr && task.active)
        .map(|task| task.fire_at_subtick.saturating_sub(current_subtick))
        .unwrap_or(0);
    let remaining_count = if remaining_subticks == 0 {
        0
    } else {
        let remaining_us = remaining_subticks.div_ceil(60);
        if remaining_us <= i32::MAX as u64 {
            -(remaining_us as i32)
        } else {
            remaining_us.div_ceil(1_000).min(i32::MAX as u64) as i32
        }
    };
    let q_type = memory.read_u16_be(task_ptr + 4).unwrap_or(0) & 0x7fff;
    let _ = memory.write_u32_be(task_ptr, 0);
    let _ = memory.write_u16_be(task_ptr + 4, q_type);
    let _ = memory.write_u32_be(task_ptr + 10, remaining_count as u32);
    timer_tasks.retain(|task| task.task_ptr != task_ptr);
    ppc_sync_time_task_links(memory, timer_tasks);
    if ppc_timer_trace_enabled() {
        eprintln!("[TIMER-PPC] remove task=${task_ptr:08X}");
    }
}

fn ppc_sync_time_task_links(memory: &mut PpcSectionMem, timer_tasks: &[PpcTimerTaskRecord]) {
    for (index, task) in timer_tasks.iter().enumerate() {
        let next = timer_tasks
            .get(index.saturating_add(1))
            .map(|next| next.task_ptr)
            .unwrap_or(0);
        let _ = memory.write_u32_be(task.task_ptr, next);
    }
}

pub(crate) fn ppc_remove_vbl_task(
    memory: &mut PpcSectionMem,
    vbl_tasks: &mut Vec<PpcVblTaskRecord>,
    task_ptr: u32,
) -> i16 {
    if task_ptr == 0 || memory.read_u16_be(task_ptr + 4).unwrap_or(0) != 1 {
        return -2;
    }
    let before = vbl_tasks.len();
    vbl_tasks.retain(|task| task.task_ptr != task_ptr);
    if vbl_tasks.len() == before {
        return -1;
    }
    let _ = memory.write_u32_be(task_ptr, 0);
    ppc_sync_vbl_task_links(memory, vbl_tasks);
    PPC_NO_ERR
}

pub(super) fn ppc_sync_vbl_task_links(memory: &mut PpcSectionMem, vbl_tasks: &[PpcVblTaskRecord]) {
    for task in vbl_tasks {
        let next = vbl_tasks
            .iter()
            .skip_while(|candidate| candidate.task_ptr != task.task_ptr)
            .skip(1)
            .find(|candidate| candidate.slot == task.slot)
            .map(|candidate| candidate.task_ptr)
            .unwrap_or(0);
        let _ = memory.write_u32_be(task.task_ptr, next);
    }
}
