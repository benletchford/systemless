//! Direct Apple events addressed to the current process.
use super::*;

#[allow(clippy::too_many_arguments)]
pub(super) fn dispatch(
    cpu: &mut PpcCpu,
    manager: &mut ProcessNativeMemoryManager,
    memory: &mut PpcSectionMem,
    heap_cursor: &mut u32,
    heap_limit: u32,
    last_mem_error: &mut i16,
    handles: &mut Vec<PpcHandleRecord>,
    state: &mut PpcAppleEventState,
    toolbox: &mut PpcToolboxStartupState,
    event_queue: &mut crate::event_queue::EventQueue,
    tick_count: u32,
    event_modifiers: u16,
    input: PpcInputSnapshot,
    application_signature: u32,
) -> PpcImportAction {
    let fail = |error| PpcImportAction::Return(ppc_i16_result(error));
    let wait_key = (
        toolbox.execution.calls().current_task().thread_id(),
        cpu.pc,
        toolbox.execution.calls().depth(),
    );
    if let Some(wait) = state.waiting_sends.get(&wait_key).copied() {
        if wait.filter_dispatch {
            state
                .waiting_sends
                .get_mut(&wait_key)
                .unwrap()
                .filter_dispatch = false;
            cpu.lr = wait.return_lr;
        }
        if wait.filter_scratch != 0 {
            let accepted = cpu.gpr[3] & 0xff != 0;
            let _ = manager.dispose_native_handle(memory, wait.filter_handle);
            handles.retain(|record| record.handle != wait.filter_handle);
            state.descriptors.with_mut(|descriptors| {
                descriptors.backing.remove(&wait.filter_handle);
                descriptors.descriptors.remove(&(wait.filter_scratch + 16));
            });
            let _ = manager.dispose_native_ptr(wait.filter_scratch);
            let waiting = state.waiting_sends.get_mut(&wait_key).unwrap();
            waiting.filter_scratch = 0;
            waiting.filter_handle = 0;
            state
                .filter_seen
                .get_mut(&wait.id)
                .unwrap()
                .insert(wait.filter_id);
            cpu.lr = wait.return_lr;
            state.prune_queued_self_events(event_queue);
            if accepted {
                let position = event_queue
                    .iter()
                    .position(|event| event.delivery_id == Some(wait.filter_id));
                let payload = state
                    .queued_self_events
                    .iter()
                    .position(|(event, _, _)| event.delivery_id == Some(wait.filter_id));
                if let Some(position) = position {
                    let retained = payload.is_some();
                    let event = if let Some(payload) = payload {
                        let (_, event, route) = state.queued_self_events.remove(payload).unwrap();
                        (event, route)
                    } else {
                        let mut data = Vec::new();
                        if data.try_reserve_exact(8).is_err()
                            || self_queue_reservation_fails(state, 23)
                        {
                            remove_wait(state, wait_key);
                            abandon_wait(state, wait.id);
                            return fail(PPC_MEM_FULL_ERR);
                        }
                        let record = event_queue[position];
                        data.extend_from_slice(&record.message.to_be_bytes());
                        data.extend_from_slice(&(record.where_v as u16).to_be_bytes());
                        data.extend_from_slice(&(record.where_h as u16).to_be_bytes());
                        let mut descriptor = ProcessAeDescriptor {
                            desc_type: PPC_CORE_EVENT_CLASS,
                            data,
                            ..Default::default()
                        };
                        if state.is_launch_event_id(wait.filter_id)
                            && ppc_add_finder_sender(&mut descriptor).is_err()
                        {
                            remove_wait(state, wait_key);
                            abandon_wait(state, wait.id);
                            return fail(PPC_MEM_FULL_ERR);
                        }
                        (descriptor, PpcSelfReplyRoute::None)
                    };
                    let record = event_queue.remove(position).unwrap();
                    let (event, route) = event;
                    let depth = toolbox.execution.calls().depth();
                    state
                        .waiting_sends
                        .get_mut(&wait_key)
                        .unwrap()
                        .filter_dispatch = true;
                    cpu.lr = cpu.pc;
                    let action = ppc_dispatch_owned_apple_event(
                        cpu,
                        manager,
                        memory,
                        heap_cursor,
                        heap_limit,
                        last_mem_error,
                        handles,
                        state,
                        toolbox,
                        depth,
                        event,
                        retained.then_some(u32::from(route != PpcSelfReplyRoute::None)),
                        true,
                        route,
                    );
                    if !retained
                        && matches!(action, PpcImportAction::Return(result) if result == ppc_i16_result(PPC_MEM_FULL_ERR))
                    {
                        event_queue.insert(position, record);
                        remove_wait(state, wait_key);
                        abandon_wait(state, wait.id);
                        cpu.lr = wait.return_lr;
                    }
                    return action;
                }
            }
            return yield_waiting_send(cpu, toolbox);
        }
        if wait.idle_scratch != 0 {
            let canceled = cpu.gpr[3] & 0xff != 0;
            let sleep = memory.read_u32_be(wait.idle_scratch + 16).unwrap_or(0);
            let mouse_region = memory.read_u32_be(wait.idle_scratch + 20).unwrap_or(0);
            let _ = manager.dispose_native_ptr(wait.idle_scratch);
            let waiting = state.waiting_sends.get_mut(&wait_key).unwrap();
            waiting.idle_scratch = 0;
            waiting.idle_initialized = true;
            waiting.idle_sleep = sleep;
            waiting.idle_mouse_region = mouse_region;
            waiting.idle_started = tick_count;
            cpu.lr = wait.return_lr;
            if canceled {
                remove_wait(state, wait_key);
                abandon_wait(state, wait.id);
                return fail(-1711); // errAEWaitCanceled (MacErrors.h)
            }
            return yield_waiting_send(cpu, toolbox);
        }
        if let Some(reply) = state.take_wait_reply(wait.id) {
            remove_wait(state, wait_key);
            return fail(complete_wait_reply(
                reply,
                wait.output,
                manager,
                memory,
                heap_cursor,
                last_mem_error,
                handles,
                state,
            ));
        }
        if wait
            .timeout
            .is_some_and(|timeout| tick_count.wrapping_sub(wait.started) >= timeout)
        {
            remove_wait(state, wait_key);
            abandon_wait(state, wait.id);
            return fail(-1712); // errAETimeout (MacErrors.h)
        }
        if wait.filter_proc != 0 {
            ppc_enqueue_open_application_event_if_needed(
                state,
                event_queue,
                PPC_HIGH_LEVEL_EVENT_MASK,
                tick_count,
            );
            let position = event_queue.iter().position(|event| {
                event.what == PPC_HIGH_LEVEL_EVENT
                    && event
                        .delivery_id
                        .is_none_or(|id| !state.filter_seen[&wait.id].contains(&id))
            });
            if let Some(position) = position {
                let candidate = if let Some(id) = event_queue[position].delivery_id {
                    id
                } else {
                    let Some(id) = state.next_self_event_id.checked_add(1) else {
                        remove_wait(state, wait_key);
                        abandon_wait(state, wait.id);
                        return fail(PPC_MEM_FULL_ERR);
                    };
                    state.next_self_event_id = id;
                    event_queue[position].delivery_id = Some(id);
                    id
                };
                return start_wait_filter(
                    cpu,
                    manager,
                    memory,
                    heap_cursor,
                    heap_limit,
                    last_mem_error,
                    handles,
                    state,
                    toolbox,
                    wait_key,
                    event_queue,
                    candidate,
                );
            }
            state.filter_seen.get_mut(&wait.id).unwrap().clear();
        }
        if wait.idle_proc != 0 {
            let mouse_moved =
                wait.idle_initialized && idle_mouse_moved(memory, wait.idle_mouse_region, input);
            let ready_event = mouse_moved
                || event_queue
                    .iter()
                    .any(|event| matches!(event.what, 6 | 8 | 15));
            if wait.idle_initialized
                && !ready_event
                && tick_count.wrapping_sub(wait.idle_started) < wait.idle_sleep
            {
                return yield_waiting_send(cpu, toolbox);
            }
            return start_wait_idle(
                cpu,
                manager,
                memory,
                state,
                event_queue,
                wait_key,
                tick_count,
                input,
                event_modifiers,
                heap_cursor,
                heap_limit,
                toolbox,
            );
        }
        return yield_waiting_send(cpu, toolbox);
    }
    let event = match ppc_try_ae_descriptor(memory, &state.descriptors, cpu.gpr[3]) {
        Ok(event) if !self_queue_reservation_fails(state, 12) => event,
        _ => return fail(PPC_MEM_FULL_ERR),
    };
    let output = cpu.gpr[4];
    if output != 0 && !ppc_write_ae_desc(memory, output, PPC_TYPE_NULL, 0) {
        return fail(PPC_PARAM_ERR);
    }
    if output != 0 {
        state.descriptors.with_mut(|descriptors| {
            descriptors.events.remove(&output);
            descriptors.descriptors.remove(&output);
        });
    }
    let mode = cpu.gpr[5];
    let reply_mode = mode & 3;
    // AEDataModel.h: three reply modes and mutually exclusive recording flags.
    if reply_mode == 0 || mode & !0xb2f3 != 0 || mode & 0x3000 == 0x3000 {
        return fail(-1710); // errAEUnknownSendMode
    }
    if reply_mode != 1 && output == 0 {
        return fail(PPC_PARAM_ERR);
    }
    let Some(mut event) =
        event.filter(|event| event.desc_type == PPC_CORE_EVENT_CLASS && event.data.len() >= 8)
    else {
        return fail(-1707); // errAENotAppleEvent
    };
    let Some(address) = event.attributes.get(&u32::from_be_bytes(*b"addr")) else {
        return fail(-1716); // errAEUnknownAddressType
    };
    let direct = match (address.desc_type.to_be_bytes(), address.data.as_slice()) {
        ([b'p', b's', b'n', b' '], [0, 0, 0, 0, 0, 0, 0, 2]) => true,
        ([b'p', b's', b'n', b' '], bytes)
            if bytes.len() == 8
                && bytes[..4]
                    == crate::process_manager::ProcessSerialNumber::ACTUAL
                        .high
                        .to_be_bytes()
                && bytes[4..]
                    == crate::process_manager::ProcessSerialNumber::ACTUAL
                        .low
                        .to_be_bytes() =>
        {
            false
        }
        ([b's', b'i', b'g', b'n'], bytes)
            if application_signature != u32::from_be_bytes(*b"????")
                && bytes == application_signature.to_be_bytes() =>
        {
            false
        }
        ([b'p', b's', b'n', b' '], _) | ([b's', b'i', b'g', b'n'], _) => {
            return fail(PPC_PROC_NOT_FOUND_ERR);
        }
        _ => return fail(-1716),
    };
    if mode & 0x2000 != 0 {
        return fail(PPC_NO_ERR); // kAEDontExecute: recording only.
    }
    if !direct && reply_mode == 3 && cpu.gpr[8] != 0 {
        let Some(procedure) = resolve_guest_procedure(
            memory,
            cpu.gpr[8],
            cpu.gpr[2],
            None,
            GuestIsa::PowerPc,
            GuestIsa::PowerPc,
        ) else {
            return fail(PPC_PARAM_ERR);
        };
        let mapped = match procedure.isa {
            GuestIsa::PowerPc => memory.read_u32_be(procedure.entry).is_some(),
            GuestIsa::M68k => memory.read_u16_be(procedure.entry).is_some(),
        };
        if !mapped {
            return fail(PPC_PARAM_ERR);
        }
    }
    // Inside Macintosh: timeouts apply only to kAEWaitReply.
    if reply_mode != 3 {
        event.attributes.remove(&u32::from_be_bytes(*b"timo"));
    }
    let mut source_data = Vec::new();
    if event.attributes.try_reserve(1).is_err()
        || self_queue_reservation_fails(state, 9)
        || source_data.try_reserve_exact(2).is_err()
        || self_queue_reservation_fails(state, 10)
    {
        return fail(PPC_MEM_FULL_ERR);
    }
    source_data.extend_from_slice(&(if direct { 1i16 } else { 2i16 }).to_be_bytes());
    event.attributes.insert(
        u32::from_be_bytes(*b"esrc"),
        ProcessAeDescriptor {
            desc_type: u32::from_be_bytes(*b"shor"),
            data: source_data,
            ..Default::default()
        },
    );
    if !direct {
        let event_class = u32::from_be_bytes(event.data[..4].try_into().unwrap());
        let event_id = u32::from_be_bytes(event.data[4..8].try_into().unwrap());
        state.prune_queued_self_events(event_queue);
        ppc_queue_completed_apple_event_replies(state, event_queue, tick_count, event_modifiers);
        let Some(delivery_id) = state.next_self_event_id.checked_add(1) else {
            return fail(PPC_MEM_FULL_ERR);
        };
        if reply_mode == 2 {
            // Each accepted request may later complete while another handler
            // is active or while discarded events are being pruned.
            let outstanding = state
                .queued_self_events
                .iter()
                .filter(|(_, _, route)| *route == PpcSelfReplyRoute::Queued)
                .count()
                .saturating_add(usize::from(
                    state
                        .current_self_event
                        .as_ref()
                        .is_some_and(|event| event.2 == PpcSelfReplyRoute::Queued),
                ))
                .saturating_add(
                    state
                        .pending_dispatches
                        .iter()
                        .chain(&state.suspended_dispatches)
                        .filter(|dispatch| dispatch.reply_route == PpcSelfReplyRoute::Queued)
                        .count(),
                )
                .saturating_add(1);
            if state
                .completed_self_replies
                .try_reserve(outstanding)
                .is_err()
                || self_queue_reservation_fails(state, 1)
            {
                return fail(PPC_MEM_FULL_ERR);
            }
        }
        if reply_mode == 3 {
            let outstanding = state.waiting_sends.len().saturating_add(1);
            if state.waiting_sends.try_reserve(1).is_err()
                || state.filter_seen.try_reserve(1).is_err()
                || self_queue_reservation_fails(state, 21)
                || state
                    .completed_wait_replies
                    .try_reserve(outstanding)
                    .is_err()
            {
                return fail(PPC_MEM_FULL_ERR);
            }
        }
        let record = PpcQueuedEvent {
            delivery_id: Some(delivery_id),
            what: PPC_HIGH_LEVEL_EVENT,
            message: event_class,
            when: tick_count,
            where_v: (event_id >> 16) as u16 as i16,
            where_h: event_id as u16 as i16,
            modifiers: event_modifiers,
        };
        // Reserve both queues before publishing either copy, so an allocation
        // failure cannot leave an event without its retained descriptor.
        if state.queued_self_events.try_reserve(1).is_err()
            || self_queue_reservation_fails(state, 2)
            || event_queue.try_reserve(1).is_err()
            || self_queue_reservation_fails(state, 3)
        {
            return fail(PPC_MEM_FULL_ERR);
        }
        state.next_self_event_id = delivery_id;
        state.queued_self_events.push_back((
            record,
            event,
            match reply_mode {
                2 => PpcSelfReplyRoute::Queued,
                3 => PpcSelfReplyRoute::Waiting(delivery_id),
                _ => PpcSelfReplyRoute::None,
            },
        ));
        if cpu.gpr[6] == 1 {
            event_queue.push_front(record);
        } else {
            event_queue.push_back(record);
        }
        if reply_mode == 3 {
            let timeout = match cpu.gpr[7] as i32 {
                -2 => None,       // kNoTimeOut
                -1 => Some(3600), // kAEDefaultTimeout
                value if value >= 0 => Some(value as u32),
                _ => Some(0),
            };
            state.filter_seen.insert(delivery_id, HashSet::new());
            state.waiting_sends.insert(
                wait_key,
                PpcWaitingSend {
                    id: delivery_id,
                    output,
                    started: tick_count,
                    timeout,
                    idle_proc: cpu.gpr[8],
                    idle_scratch: 0,
                    return_lr: cpu.lr,
                    idle_initialized: false,
                    idle_sleep: 0,
                    idle_mouse_region: 0,
                    idle_started: tick_count,
                    filter_proc: cpu.gpr[9],
                    filter_scratch: 0,
                    filter_handle: 0,
                    filter_id: 0,
                    filter_dispatch: false,
                },
            );
            return yield_waiting_send(cpu, toolbox);
        }
        return fail(PPC_NO_ERR);
    }
    // Apple Technical Note IC01: self-addressed PSN events bypass the event
    // queue. Both QueueReply and WaitReply return their reply directly.
    let depth = toolbox.execution.calls().depth();
    ppc_dispatch_owned_apple_event(
        cpu,
        manager,
        memory,
        heap_cursor,
        heap_limit,
        last_mem_error,
        handles,
        state,
        toolbox,
        depth,
        event,
        Some(if reply_mode == 1 { 0 } else { output }),
        false,
        PpcSelfReplyRoute::None,
    )
}

fn remove_wait(state: &mut PpcAppleEventState, key: (u32, u32, usize)) {
    if let Some(wait) = state.waiting_sends.remove(&key) {
        state.filter_seen.remove(&wait.id);
    }
}

fn abandon_wait(state: &mut PpcAppleEventState, id: u64) {
    for (_, _, route) in &mut state.queued_self_events {
        if *route == PpcSelfReplyRoute::Waiting(id) {
            *route = PpcSelfReplyRoute::Abandoned;
        }
    }
    if let Some((_, _, route)) = &mut state.current_self_event {
        if *route == PpcSelfReplyRoute::Waiting(id) {
            *route = PpcSelfReplyRoute::Abandoned;
        }
    }
    for dispatch in state
        .pending_dispatches
        .iter_mut()
        .chain(&mut state.suspended_dispatches)
    {
        if dispatch.reply_route == PpcSelfReplyRoute::Waiting(id) {
            dispatch.reply_route = PpcSelfReplyRoute::Abandoned;
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn start_wait_filter(
    cpu: &mut PpcCpu,
    manager: &mut ProcessNativeMemoryManager,
    memory: &mut PpcSectionMem,
    heap_cursor: &mut u32,
    heap_limit: u32,
    last_mem_error: &mut i16,
    handles: &mut Vec<PpcHandleRecord>,
    state: &mut PpcAppleEventState,
    toolbox: &mut PpcToolboxStartupState,
    key: (u32, u32, usize),
    queue: &crate::event_queue::EventQueue,
    id: u64,
) -> PpcImportAction {
    let wait = state.waiting_sends[&key];
    if state
        .filter_seen
        .get_mut(&wait.id)
        .unwrap()
        .try_reserve(1)
        .is_err()
        || self_queue_reservation_fails(state, 22)
    {
        remove_wait(state, key);
        abandon_wait(state, wait.id);
        return PpcImportAction::Return(ppc_i16_result(PPC_MEM_FULL_ERR));
    }
    let Some(procedure) = resolve_guest_procedure(
        memory,
        wait.filter_proc,
        cpu.gpr[2],
        None,
        GuestIsa::PowerPc,
        GuestIsa::PowerPc,
    ) else {
        remove_wait(state, key);
        abandon_wait(state, wait.id);
        return PpcImportAction::Return(ppc_i16_result(PPC_PARAM_ERR));
    };
    let Some(record) = queue
        .iter()
        .find(|record| record.delivery_id == Some(id))
        .copied()
    else {
        return yield_waiting_send(cpu, toolbox);
    };
    let event = state
        .queued_self_events
        .iter()
        .find(|(record, _, _)| record.delivery_id == Some(id))
        .map(|(_, event, _)| event);
    let return_id = event
        .and_then(|event| event.attributes.get(&u32::from_be_bytes(*b"rtid")))
        .and_then(|value| value.data.as_slice().try_into().ok())
        .map(i16::from_be_bytes)
        .unwrap_or(0) as i32 as u32;
    let transaction_id = event
        .and_then(|event| event.attributes.get(&u32::from_be_bytes(*b"tran")))
        .and_then(|value| value.data.as_slice().try_into().ok())
        .map(u32::from_be_bytes)
        .unwrap_or(0);
    let scratch = manager.new_native_ptr(memory, 24, true);
    if scratch == 0 {
        remove_wait(state, key);
        abandon_wait(state, wait.id);
        return PpcImportAction::Return(ppc_i16_result(PPC_MEM_FULL_ERR));
    }
    let psn = crate::process_manager::ProcessSerialNumber::ACTUAL;
    let mut sender = [0u8; 8];
    sender[..4].copy_from_slice(&psn.high.to_be_bytes());
    sender[4..].copy_from_slice(&psn.low.to_be_bytes());
    if event.is_some() || state.is_launch_event_id(id) {
        let (sender_type, sender_data): (u32, &[u8]) = if event.is_some() {
            (u32::from_be_bytes(*b"psn "), &sender)
        } else {
            (u32::from_be_bytes(*b"sign"), b"MACS")
        };
        let result = ppc_create_process_owned_ae_desc(
            manager,
            memory,
            heap_cursor,
            heap_limit,
            last_mem_error,
            handles,
            Some(&state.descriptors),
            scratch + 16,
            sender_type,
            sender_data,
        );
        if result != PPC_NO_ERR {
            let _ = manager.dispose_native_ptr(scratch);
            remove_wait(state, key);
            abandon_wait(state, wait.id);
            return PpcImportAction::Return(ppc_i16_result(result));
        }
    }
    let handle = memory.read_u32_be(scratch + 20).unwrap_or(0);
    let _ = memory.write_u16_be(scratch, record.what);
    let _ = memory.write_u32_be(scratch + 2, record.message);
    let _ = memory.write_u32_be(scratch + 6, record.when);
    let _ = memory.write_u16_be(scratch + 10, record.where_v as u16);
    let _ = memory.write_u16_be(scratch + 12, record.where_h as u16);
    let _ = memory.write_u16_be(scratch + 14, record.modifiers);
    let args = [scratch, return_id, transaction_id, scratch + 16];
    let return_pc = cpu.pc;
    let rtoc = cpu.gpr[2];
    let action = match procedure.isa {
        GuestIsa::PowerPc if memory.read_u32_be(procedure.entry).is_some() => {
            if install_powerpc_call_arguments(cpu, memory, &args).is_some() {
                GuestCallEffect::call_guest(
                    GuestCallRequest::new(GuestCallTarget {
                        isa: GuestIsa::PowerPc,
                        entry: procedure.entry,
                        rtoc: procedure.rtoc,
                    }),
                    GuestCallContinuation::to_powerpc(
                        PPC_GUEST_CALL_RETURN_PC,
                        return_pc,
                        rtoc,
                        PpcNativeReturnGpr3::Preserve,
                    ),
                )
                .into_ppc_import_action()
            } else {
                None
            }
        }
        GuestIsa::M68k if memory.read_u16_be(procedure.entry).is_some() => {
            let saved = toolbox.mixed_mode_m68k.snapshot();
            let action = ppc_begin_m68k_universal_proc(
                cpu,
                Some(manager),
                memory,
                heap_cursor,
                heap_limit,
                toolbox,
                procedure,
                0x0000_3fd0,
                None,
                args.to_vec(),
                return_pc,
                PpcNativeReturnGpr3::Preserve,
            );
            if action.is_none() {
                toolbox.mixed_mode_m68k.restore_snapshot(saved);
            }
            action
        }
        _ => None,
    };
    let Some(action) = action else {
        let _ = manager.dispose_native_handle(memory, handle);
        handles.retain(|record| record.handle != handle);
        state.descriptors.with_mut(|descriptors| {
            descriptors.backing.remove(&handle);
            descriptors.descriptors.remove(&(scratch + 16));
        });
        let _ = manager.dispose_native_ptr(scratch);
        remove_wait(state, key);
        abandon_wait(state, wait.id);
        return PpcImportAction::Return(ppc_i16_result(PPC_PARAM_ERR));
    };
    let waiting = state.waiting_sends.get_mut(&key).unwrap();
    waiting.filter_scratch = scratch;
    waiting.filter_handle = handle;
    waiting.filter_id = id;
    action
}

fn idle_mouse_moved(memory: &mut PpcSectionMem, region: u32, input: PpcInputSnapshot) -> bool {
    region != 0
        && !ppc_empty_rgn(memory, region)
        && !ppc_point_in_region(memory, region, input.mouse_v, input.mouse_h)
}

#[allow(clippy::too_many_arguments)]
fn start_wait_idle(
    cpu: &mut PpcCpu,
    manager: &mut ProcessNativeMemoryManager,
    memory: &mut PpcSectionMem,
    state: &mut PpcAppleEventState,
    queue: &mut crate::event_queue::EventQueue,
    key: (u32, u32, usize),
    tick_count: u32,
    input: PpcInputSnapshot,
    event_modifiers: u16,
    heap_cursor: &mut u32,
    heap_limit: u32,
    toolbox: &mut PpcToolboxStartupState,
) -> PpcImportAction {
    let wait = state.waiting_sends[&key];
    let Some(procedure) = resolve_guest_procedure(
        memory,
        wait.idle_proc,
        cpu.gpr[2],
        None,
        GuestIsa::PowerPc,
        GuestIsa::PowerPc,
    ) else {
        remove_wait(state, key);
        abandon_wait(state, wait.id);
        return PpcImportAction::Return(ppc_i16_result(PPC_PARAM_ERR));
    };
    let scratch = manager.new_native_ptr(memory, 24, true);
    if scratch == 0 {
        remove_wait(state, key);
        abandon_wait(state, wait.id);
        return PpcImportAction::Return(ppc_i16_result(PPC_MEM_FULL_ERR));
    }
    let return_pc = cpu.pc;
    let rtoc = cpu.gpr[2];
    let action = match procedure.isa {
        GuestIsa::PowerPc => {
            if install_powerpc_call_arguments(cpu, memory, &[scratch, scratch + 16, scratch + 20])
                .is_none()
            {
                None
            } else {
                GuestCallEffect::call_guest(
                    GuestCallRequest::new(GuestCallTarget {
                        isa: GuestIsa::PowerPc,
                        entry: procedure.entry,
                        rtoc: procedure.rtoc,
                    }),
                    GuestCallContinuation::to_powerpc(
                        PPC_GUEST_CALL_RETURN_PC,
                        return_pc,
                        rtoc,
                        PpcNativeReturnGpr3::Preserve,
                    ),
                )
                .into_ppc_import_action()
            }
        }
        GuestIsa::M68k => {
            let saved = toolbox.mixed_mode_m68k.snapshot();
            // AEInteraction.h: uppAEIdleProcInfo is Pascal Boolean with three pointers.
            let action = ppc_begin_m68k_universal_proc(
                cpu,
                Some(manager),
                memory,
                heap_cursor,
                heap_limit,
                toolbox,
                procedure,
                0x0000_0fd0,
                None,
                vec![scratch, scratch + 16, scratch + 20],
                return_pc,
                PpcNativeReturnGpr3::Preserve,
            );
            if action.is_none() {
                toolbox.mixed_mode_m68k.restore_snapshot(saved);
            }
            action
        }
    };
    let Some(action) = action else {
        let _ = manager.dispose_native_ptr(scratch);
        remove_wait(state, key);
        abandon_wait(state, wait.id);
        return PpcImportAction::Return(ppc_i16_result(PPC_MEM_FULL_ERR));
    };
    // Inside Macintosh IAC, p. 5-23: the first idle event is null;
    // subsequent callbacks receive only update, activate, OS and null events.
    let event = wait
        .idle_initialized
        .then(|| {
            queue
                .iter()
                .position(|event| matches!(event.what, 6 | 8 | 15))
                .and_then(|index| queue.remove(index))
        })
        .flatten();
    let _ = memory.write_u32_be(scratch + 16, wait.idle_sleep);
    let _ = memory.write_u32_be(scratch + 20, wait.idle_mouse_region);
    let event = event.or_else(|| {
        (wait.idle_initialized && idle_mouse_moved(memory, wait.idle_mouse_region, input))
            .then_some(PpcQueuedEvent {
                delivery_id: None,
                what: 15,
                message: 0xfa00_0000,
                when: tick_count,
                where_v: input.mouse_v,
                where_h: input.mouse_h,
                modifiers: event_modifiers,
            })
    });
    if let Some(event) = event {
        let _ = memory.write_u16_be(scratch, event.what);
        let _ = memory.write_u32_be(scratch + 2, event.message);
        let _ = memory.write_u32_be(scratch + 6, event.when);
        let _ = memory.write_u16_be(scratch + 10, event.where_v as u16);
        let _ = memory.write_u16_be(scratch + 12, event.where_h as u16);
        let _ = memory.write_u16_be(scratch + 14, event.modifiers);
    } else {
        let _ = memory.write_u32_be(scratch + 6, tick_count);
    }
    state.waiting_sends.get_mut(&key).unwrap().idle_scratch = scratch;
    action
}

fn yield_waiting_send(cpu: &mut PpcCpu, toolbox: &mut PpcToolboxStartupState) -> PpcImportAction {
    match toolbox.execution.calls().yield_native_pending_import(cpu) {
        Ok(true) => PpcImportAction::Yield(1),
        Ok(false) | Err(_) => PpcImportAction::Yield(u64::MAX),
    }
}

pub(super) fn self_queue_reservation_fails(state: &PpcAppleEventState, stage: u8) -> bool {
    #[cfg(test)]
    {
        state.fail_self_queue_reservation == stage
    }
    #[cfg(not(test))]
    {
        let _ = (state, stage);
        false
    }
}

#[allow(clippy::too_many_arguments)]
pub(super) fn complete_reply(
    handler_result: i16,
    output: u32,
    source: u32,
    manager: &mut ProcessNativeMemoryManager,
    memory: &mut PpcSectionMem,
    heap_cursor: &mut u32,
    last_mem_error: &mut i16,
    handles: &mut Vec<PpcHandleRecord>,
    state: &mut PpcAppleEventState,
) -> i16 {
    if output == 0 {
        return PPC_NO_ERR;
    }
    let Some(mut reply) = ppc_ae_descriptor(memory, &state.descriptors, source) else {
        return -1707;
    };
    add_handler_error(&mut reply, handler_result);
    let result = ppc_create_process_owned_ae_desc(
        manager,
        memory,
        heap_cursor,
        0,
        last_mem_error,
        handles,
        Some(&state.descriptors),
        output,
        reply.desc_type,
        &reply.data,
    );
    if result == PPC_NO_ERR {
        ppc_store_ae_descriptor_semantics(memory, &state.descriptors, output, reply);
    }
    result
}

/// Materialize a reply retained for a synchronous sender in its process heap.
#[allow(clippy::too_many_arguments, dead_code)]
pub(in crate::systems::macintosh::loader::ppc) fn complete_wait_reply(
    pending: PpcPendingSelfReply,
    output: u32,
    manager: &mut ProcessNativeMemoryManager,
    memory: &mut PpcSectionMem,
    heap_cursor: &mut u32,
    last_mem_error: &mut i16,
    handles: &mut Vec<PpcHandleRecord>,
    state: &mut PpcAppleEventState,
) -> i16 {
    let (mut reply, error) = match pending {
        PpcPendingSelfReply::Completed(reply, error) => (reply, error),
        PpcPendingSelfReply::Error(event, error) => (
            ProcessAeDescriptor {
                desc_type: PPC_CORE_EVENT_CLASS,
                data: b"aevtansr".to_vec(),
                attributes: event.attributes,
                ..Default::default()
            },
            error,
        ),
    };
    add_handler_error(&mut reply, error);
    let result = ppc_create_process_owned_ae_desc(
        manager,
        memory,
        heap_cursor,
        0,
        last_mem_error,
        handles,
        Some(&state.descriptors),
        output,
        reply.desc_type,
        &reply.data,
    );
    if result == PPC_NO_ERR {
        ppc_store_ae_descriptor_semantics(memory, &state.descriptors, output, reply);
    }
    result
}

/// Preserve explicit keyErrorNumber values supplied by an event handler.
pub(super) fn add_handler_error(reply: &mut ProcessAeDescriptor, handler_result: i16) {
    // Inside Macintosh, "Replying to an Apple Event": the handler's error
    // belongs in keyErrorNumber; preserve an explicit error supplied by it.
    if handler_result != PPC_NO_ERR {
        let keyword = u32::from_be_bytes(*b"errn");
        if !reply.fields.contains_key(&keyword) {
            let error = ProcessAeDescriptor {
                desc_type: u32::from_be_bytes(*b"long"),
                data: i32::from(handler_result).to_be_bytes().to_vec(),
                ..Default::default()
            };
            reply.fields.insert(keyword, error.clone());
            reply.items.push((keyword, error));
        }
    }
}
