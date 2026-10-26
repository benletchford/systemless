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
) -> PpcImportAction {
    let fail = |error| PpcImportAction::Return(ppc_i16_result(error));
    let event = ppc_ae_descriptor(memory, &state.descriptors, cpu.gpr[3]);
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
    match (address.desc_type.to_be_bytes(), address.data.as_slice()) {
        ([b'p', b's', b'n', b' '], [0, 0, 0, 0, 0, 0, 0, 2]) => {}
        ([b'p', b's', b'n', b' '], _) | ([b's', b'i', b'g', b'n'], _) => {
            return fail(PPC_PROC_NOT_FOUND_ERR);
        }
        _ => return fail(-1716),
    }
    if mode & 0x2000 != 0 {
        return fail(PPC_NO_ERR); // kAEDontExecute: recording only.
    }
    // Apple Technical Note IC01: self-addressed PSN events bypass the event
    // queue. Both QueueReply and WaitReply return their reply directly.
    event.attributes.insert(
        u32::from_be_bytes(*b"esrc"),
        ProcessAeDescriptor {
            desc_type: u32::from_be_bytes(*b"shor"),
            data: 1i16.to_be_bytes().to_vec(), // kAEDirectCall
            ..Default::default()
        },
    );
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
    )
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
