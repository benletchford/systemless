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
    application_signature: u32,
) -> PpcImportAction {
    let fail = |error| PpcImportAction::Return(ppc_i16_result(error));
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
    if !direct && reply_mode == 3 {
        return fail(PPC_PROC_NOT_FOUND_ERR);
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
            if reply_mode == 2 {
                PpcSelfReplyRoute::Queued
            } else {
                PpcSelfReplyRoute::None
            },
        ));
        if cpu.gpr[6] == 1 {
            event_queue.push_front(record);
        } else {
            event_queue.push_back(record);
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
