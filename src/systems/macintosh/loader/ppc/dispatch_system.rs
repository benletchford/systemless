use super::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PpcSystemCompatibilityOperation {
    BuildDdPwds,
    CtbGetCtbVersion,
    CallComponentUpp,
    CharByte,
    DiBadMount,
    DiLoad,
    DiUnload,
    Debugger,
    Dequeue,
    Enqueue,
    FindNextComponent,
    GetNextProcess,
    GetEvQHdr,
    GetScript,
    GetScriptManagerVariable,
    GetScriptVariable,
    GetSysBeepVolume,
    GetSysDirection,
    IuCompString,
    IuDateString,
    IuEqualString,
    InitCrm,
    InitCtbUtilities,
    IntlScript,
    KeyTranslate,
    LaunchApplication,
    LmGetCurApName,
    LmGetSysFontFam,
    LmGetSysFontSize,
    MidiAddPort,
    MidiRemovePort,
    MidiSignOut,
    MidiWritePacket,
    Munger,
    NmInstall,
    NmRemove,
    ObscureCursor,
    OpenDefaultComponent,
    ResetAlertStage,
    SetFrontProcess,
    StyledLineBreak,
    SystemEdit,
    TruncText,
    UpperString,
}

fn ppc_key_translate(memory: &mut PpcSectionMem, trans_data: u32, keycode: u16, state: u32) -> u32 {
    // KeyTranslate / KeyTrans uses the caller's KCHR modifier index and
    // 128-byte key tables. Ordinary (non-dead-key) translations return the
    // character in the low byte and leave no pending state.
    // FUNCTION KeyTranslate(transData: Ptr; keycode: Integer;
    //                       VAR state: LongInt): LongInt;
    // Macintosh Toolbox Essentials (1992), pp. 2-110--2-111.
    let result = (|| {
        let modifier_index = u32::from(keycode >> 8);
        let table = u32::from(memory.read_u8(trans_data.checked_add(2 + modifier_index)?)?);
        let table_count = u32::from(memory.read_u16_be(trans_data.checked_add(258)?)?);
        if table >= table_count {
            return None;
        }
        let table_offset = 260u32.checked_add(table.checked_mul(128)?)?;
        let offset = table_offset.checked_add(u32::from(keycode & 0x007f))?;
        memory.read_u8(trans_data.checked_add(offset)?)
    })()
    .map_or(0, u32::from);
    if state != 0 {
        let _ = memory.write_u32_be(state, 0);
    }
    if crate::trap::dispatch::trace_input_enabled() {
        eprintln!(
            "[INPUT] PPC KeyTranslate data=${trans_data:08X} key=${keycode:04X} state=${state:08X} -> ${result:08X}"
        );
    }
    result
}

pub(crate) fn ppc_munger_compatibility(
    cpu: &PpcCpu,
    allocator: Option<&mut PpcProcessAllocatorView<'_>>,
    memory: &mut PpcSectionMem,
    heap_cursor: &mut u32,
    heap_limit: u32,
    last_mem_error: &mut i16,
    handles: &mut Vec<PpcHandleRecord>,
) -> u32 {
    let handle = cpu.gpr[3];
    let offset = cpu.gpr[4] as i32;
    let search_len = cpu.gpr[6] as i32;
    let replacement_len = cpu.gpr[8] as i32;
    if offset < 0 || search_len < 0 || replacement_len < 0 {
        return u32::MAX;
    }
    let Some(mut contents) = ppc_handle_bytes(memory, handles, handle) else {
        *last_mem_error = PPC_NIL_HANDLE_ERR;
        return u32::MAX;
    };
    let start = offset as usize;
    if start > contents.len() {
        return u32::MAX;
    }
    let Some(needle) = ppc_memory_read_bytes(memory, cpu.gpr[5], search_len as u32)
        .or_else(|| (search_len == 0).then(Vec::new))
    else {
        return u32::MAX;
    };
    let position = if needle.is_empty() {
        Some(start)
    } else {
        contents[start..]
            .windows(needle.len())
            .position(|window| window == needle)
            .map(|relative| start + relative)
    };
    let Some(position) = position else {
        return u32::MAX;
    };
    if cpu.gpr[7] == 0 && replacement_len == 0 {
        return u32::try_from(position).unwrap_or(u32::MAX);
    }
    let Some(replacement) = ppc_memory_read_bytes(memory, cpu.gpr[7], replacement_len as u32)
        .or_else(|| (replacement_len == 0).then(Vec::new))
    else {
        return u32::MAX;
    };
    contents.splice(position..position + needle.len(), replacement);
    let result = ppc_allocator_view_resize_handle(
        allocator,
        memory,
        heap_cursor,
        heap_limit,
        last_mem_error,
        handles,
        handle,
        u32::try_from(contents.len()).unwrap_or(u32::MAX),
    );
    *last_mem_error = result;
    if result != PPC_NO_ERR {
        return u32::MAX;
    }
    let Some(ptr) = memory.read_u32_be(handle) else {
        return u32::MAX;
    };
    if memory.write_bytes(ptr, &contents).is_none() {
        *last_mem_error = PPC_PARAM_ERR;
        return u32::MAX;
    }
    u32::try_from(position).unwrap_or(u32::MAX)
}

pub(crate) fn ppc_enqueue_compatibility(
    memory: &mut PpcSectionMem,
    element: u32,
    header: u32,
) -> bool {
    if element == 0
        || header == 0
        || !ppc_memory_can_write_bytes(memory, element, 4)
        || !ppc_memory_can_write_bytes(memory, header, 10)
    {
        return false;
    }
    let tail = memory.read_u32_be(header + 6).unwrap_or(0);
    let _ = memory.write_u32_be(element, 0);
    if tail == 0 {
        let _ = memory.write_u32_be(header + 2, element);
    } else {
        let _ = memory.write_u32_be(tail, element);
    }
    memory.write_u32_be(header + 6, element).is_some()
}

pub(crate) fn ppc_dequeue_compatibility(
    memory: &mut PpcSectionMem,
    element: u32,
    header: u32,
) -> i16 {
    if element == 0 || header == 0 || !ppc_memory_can_write_bytes(memory, header, 10) {
        return PPC_PARAM_ERR;
    }
    let mut previous = 0;
    let mut current = memory.read_u32_be(header + 2).unwrap_or(0);
    while current != 0 {
        let next = memory.read_u32_be(current).unwrap_or(0);
        if current == element {
            if previous == 0 {
                let _ = memory.write_u32_be(header + 2, next);
            } else {
                let _ = memory.write_u32_be(previous, next);
            }
            if memory.read_u32_be(header + 6) == Some(element) {
                let _ = memory.write_u32_be(header + 6, previous);
            }
            let _ = memory.write_u32_be(element, 0);
            return PPC_NO_ERR;
        }
        previous = current;
        current = next;
    }
    -1
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn ppc_dispatch_system_compatibility(
    operation: PpcSystemCompatibilityOperation,
    cpu: &mut PpcCpu,
    process_memory_manager: &mut ProcessNativeMemoryManager,
    memory: &mut PpcSectionMem,
    heap_cursor: &mut u32,
    heap_limit: u32,
    last_mem_error: &mut i16,
    handles: &mut Vec<PpcHandleRecord>,
    launched_app_path: Option<&str>,
    toolbox_startup: &mut PpcToolboxStartupState,
) -> PpcImportAction {
    match operation {
        PpcSystemCompatibilityOperation::NmInstall | PpcSystemCompatibilityOperation::NmRemove => {
            let request = cpu.gpr[3];
            let result = if request == 0 || memory.read_u16_be(request.wrapping_add(4)) != Some(8)
                || !ppc_memory_can_write_bytes(memory, request, 32) { -299 }
            else if operation == PpcSystemCompatibilityOperation::NmRemove {
                ppc_remove_notification(memory, &mut toolbox_startup.notification_requests, request)
            } else if toolbox_startup.notification_requests.contains(&request) { 0 }
            else {
                if let Some(&tail) = toolbox_startup.notification_requests.last() { let _ = memory.write_u32_be(tail, request); }
                let _ = memory.write_u32_be(request, 0);
                toolbox_startup.notification_requests.push(request);
                // Response is the final notification stage (Inside Macintosh:
                // Processes, Notification Manager, p.5-9). Alert/sound requests
                // must remain pending until actual delivery/acknowledgement.
                if memory.read_u16_be(request + 14).unwrap_or(0) != 0
                    || memory.read_u32_be(request + 16).unwrap_or(0) != 0
                    || memory.read_u32_be(request + 20).unwrap_or(0) != 0
                    || memory.read_u32_be(request + 24).unwrap_or(0) != 0 {
                    return PpcImportAction::Return(0);
                }
                let instance = toolbox_startup.notification_requests.instance_id(request).unwrap();
                if !toolbox_startup.notification_requests.begin_response(request, instance) {
                    return PpcImportAction::Return(0);
                }
                let response = memory.read_u32_be(request + 28).unwrap_or(0);
                if response == u32::MAX {
                    ppc_remove_notification(memory, &mut toolbox_startup.notification_requests, request);
                } else if response != 0 {
                    if let Some(target) = resolve_guest_procedure(memory, response, cpu.gpr[2], None,
                        GuestIsa::PowerPc, GuestIsa::PowerPc) {
                        if target.isa == GuestIsa::M68k && target.proc_info != 0 {
                            let saved = toolbox_startup.mixed_mode_m68k.snapshot();
                            if let Some(action) = ppc_begin_m68k_universal_proc(cpu,
                                Some(process_memory_manager), memory, heap_cursor, heap_limit,
                                toolbox_startup, target, target.proc_info, None, vec![request],
                                cpu.lr, PpcNativeReturnGpr3::Set(0)) {
                                return action;
                            }
                            toolbox_startup.mixed_mode_m68k.restore_snapshot(saved);
                        }
                        if target.isa != GuestIsa::PowerPc {
                            return PpcImportAction::Return(0);
                        }
                        let restore_rtoc = cpu.gpr[2];
                        let final_pc = cpu.lr;
                        if install_powerpc_call_arguments(cpu, memory, &[request]).is_some() {
                            return GuestCallEffect::call_guest(
                                GuestCallRequest::new(GuestCallTarget { isa: GuestIsa::PowerPc,
                                    entry: target.entry, rtoc: target.rtoc }),
                                GuestCallContinuation::to_powerpc(PPC_GUEST_CALL_RETURN_PC,
                                    final_pc, restore_rtoc, PpcNativeReturnGpr3::Set(0)),
                            ).into_ppc_import_action().unwrap();
                        }
                    }
                }
                0
            };
            PpcImportAction::Return(ppc_i16_result(result))
        }
        PpcSystemCompatibilityOperation::Munger => {
            let mut allocator = PpcProcessAllocatorView {
                memory_manager: process_memory_manager,
            };
            PpcImportAction::Return(ppc_munger_compatibility(
                cpu,
                Some(&mut allocator),
                memory,
                heap_cursor,
                heap_limit,
                last_mem_error,
                handles,
            ))
        }
        PpcSystemCompatibilityOperation::Enqueue => {
            let _ = ppc_enqueue_compatibility(memory, cpu.gpr[3], cpu.gpr[4]);
            PpcImportAction::ReturnPreserve
        }
        PpcSystemCompatibilityOperation::Dequeue => PpcImportAction::Return(ppc_i16_result(
            ppc_dequeue_compatibility(memory, cpu.gpr[3], cpu.gpr[4]),
        )),
        // ObscureCursor has no effect on the cursor level. It hides the cursor
        // only until the user moves the mouse, which the host injects each frame.
        // PROCEDURE ObscureCursor;
        // Inside Macintosh Volume I, I-168.
        PpcSystemCompatibilityOperation::ObscureCursor => PpcImportAction::ReturnPreserve,
        PpcSystemCompatibilityOperation::UpperString => {
            if let Some(bytes) = ppc_read_pstring_bytes(memory, cpu.gpr[3]) {
                let upper = bytes
                    .into_iter()
                    .map(|byte| byte.to_ascii_uppercase())
                    .collect::<Vec<_>>();
                let _ = ppc_write_pstring_bytes(memory, cpu.gpr[3], &upper);
            }
            PpcImportAction::ReturnPreserve
        }
        PpcSystemCompatibilityOperation::IuCompString => {
            let lhs = ppc_read_pstring_bytes(memory, cpu.gpr[3]).unwrap_or_default();
            let rhs = ppc_read_pstring_bytes(memory, cpu.gpr[4]).unwrap_or_default();
            let lhs = lhs
                .into_iter()
                .map(|byte| byte.to_ascii_uppercase())
                .collect::<Vec<_>>();
            let rhs = rhs
                .into_iter()
                .map(|byte| byte.to_ascii_uppercase())
                .collect::<Vec<_>>();
            let ordering = match lhs.cmp(&rhs) {
                std::cmp::Ordering::Less => -1i16,
                std::cmp::Ordering::Equal => 0,
                std::cmp::Ordering::Greater => 1,
            };
            PpcImportAction::Return(ppc_i16_result(ordering))
        }
        PpcSystemCompatibilityOperation::IuEqualString => {
            // IUEqualString compares Pascal strings using primary ordering:
            // case and diacritic differences do not matter, and it returns
            // zero for equality. Inside Macintosh I (1985), p. I-506.
            // FUNCTION IUEqualString(aStr,bStr: Str255): INTEGER;
            let lhs = ppc_read_pstring_bytes(memory, cpu.gpr[3]).unwrap_or_default();
            let rhs = ppc_read_pstring_bytes(memory, cpu.gpr[4]).unwrap_or_default();
            let primary = |byte| crate::trap::mac_roman_to_upper(byte, true);
            let equal = lhs
                .into_iter()
                .map(primary)
                .eq(rhs.into_iter().map(primary));
            PpcImportAction::Return(u32::from(!equal))
        }
        PpcSystemCompatibilityOperation::TruncText => {
            let width = usize::from(cpu.gpr[3] as u16);
            let text = cpu.gpr[4];
            let length_ptr = cpu.gpr[5];
            let Some(length) = memory.read_u16_be(length_ptr).map(usize::from) else {
                return PpcImportAction::Return(ppc_i16_result(-1));
            };
            let capacity = width / 6;
            if length <= capacity {
                return PpcImportAction::Return(0);
            }
            if capacity < 3 || !ppc_memory_can_write_bytes(memory, text, length as u32) {
                return PpcImportAction::Return(ppc_i16_result(-1));
            }
            let retained = capacity - 3;
            if cpu.gpr[6] & 0x4000 != 0 {
                let left = retained / 2;
                let right = retained - left;
                let tail = (0..right)
                    .filter_map(|index| memory.read_u8(text + (length - right + index) as u32))
                    .collect::<Vec<_>>();
                let _ = memory.write_bytes(text + left as u32, b"...");
                let _ = memory.write_bytes(text + left as u32 + 3, &tail);
            } else {
                let _ = memory.write_bytes(text + retained as u32, b"...");
            }
            let _ = memory.write_u16_be(length_ptr, capacity as u16);
            PpcImportAction::Return(1)
        }
        PpcSystemCompatibilityOperation::StyledLineBreak => {
            let text_len = cpu.gpr[4];
            let text_end = cpu.gpr[6].min(text_len);
            let width_ptr = cpu.gpr[8];
            let offset_ptr = cpu.gpr[9];
            let width = memory.read_u32_be(width_ptr).unwrap_or(0) as i32;
            let available_chars = (width.max(0) as u32 / (6 << 16)).max(1);
            if text_end <= available_chars {
                let _ = memory.write_u32_be(offset_ptr, text_end);
                let used = text_end.saturating_mul(6 << 16);
                let _ = memory.write_u32_be(width_ptr, (width as u32).saturating_sub(used));
                PpcImportAction::Return(2)
            } else {
                let break_at = available_chars.min(text_end);
                let _ = memory.write_u32_be(offset_ptr, break_at);
                let _ = memory.write_u32_be(width_ptr, 0);
                PpcImportAction::Return(1)
            }
        }
        PpcSystemCompatibilityOperation::GetNextProcess => {
            let psn = cpu.gpr[3];
            let current = ProcessSerialNumber::new(
                memory.read_u32_be(psn).unwrap_or(u32::MAX),
                memory.read_u32_be(psn + 4).unwrap_or(u32::MAX),
            );
            match current.next_single_process() {
                SingleProcessEnumeration::Current(current) => {
                    let _ = memory.write_u32_be(psn, current.high);
                    let _ = memory.write_u32_be(psn + 4, current.low);
                    PpcImportAction::Return(0)
                }
                SingleProcessEnumeration::End | SingleProcessEnumeration::Invalid => {
                    PpcImportAction::Return(ppc_i16_result(PPC_PROC_NOT_FOUND_ERR))
                }
            }
        }
        PpcSystemCompatibilityOperation::SetFrontProcess => PpcImportAction::Return(0),
        PpcSystemCompatibilityOperation::LmGetCurApName => {
            let name = launched_app_path
                .and_then(|path| path.rsplit('/').next())
                .unwrap_or("Systemless");
            let encoded = encode_mac_roman_lossy(name);
            let _ = ppc_write_pstring_bytes(memory, PPC_IMPORT_CUR_AP_NAME, &encoded);
            PpcImportAction::Return(PPC_IMPORT_CUR_AP_NAME)
        }
        PpcSystemCompatibilityOperation::LmGetSysFontFam => PpcImportAction::Return(0),
        PpcSystemCompatibilityOperation::LmGetSysFontSize => PpcImportAction::Return(12),
        PpcSystemCompatibilityOperation::GetSysBeepVolume => {
            let result = if memory.write_u32_be(cpu.gpr[3], 0x0100_0100).is_some() {
                PPC_NO_ERR
            } else {
                PPC_PARAM_ERR
            };
            PpcImportAction::Return(ppc_i16_result(result))
        }
        PpcSystemCompatibilityOperation::IuDateString => {
            let _ = ppc_write_pstring_bytes(memory, cpu.gpr[5], b"");
            PpcImportAction::ReturnPreserve
        }
        PpcSystemCompatibilityOperation::SystemEdit => {
            let edit_cmd = cpu.gpr[3] as i16;
            let params = crate::desk_manager::evaluate_system_edit_parameters(edit_cmd);
            let result = if params.handled() { 1 } else { 0 };
            PpcImportAction::Return(result)
        }
        PpcSystemCompatibilityOperation::DiBadMount => PpcImportAction::Return(0),
        PpcSystemCompatibilityOperation::FindNextComponent
        | PpcSystemCompatibilityOperation::OpenDefaultComponent => PpcImportAction::Return(0),
        PpcSystemCompatibilityOperation::CtbGetCtbVersion => PpcImportAction::Return(0x0200),
        PpcSystemCompatibilityOperation::IntlScript => {
            // Inside Macintosh: Text (1993), pp. 6-22–6-24: IntlScript
            // returns an enabled script code, defaulting to the system script
            // when the font script is unavailable. This guest installs only
            // the Roman script (smRoman = 0).
            PpcImportAction::Return(0)
        }
        PpcSystemCompatibilityOperation::CharByte => {
            // CharByte identifies a byte's place in a multibyte character.
            // The installed Roman script uses only single-byte characters.
            // FUNCTION CharByte(textBuf: Ptr; textOffset: Integer): Integer;
            // Inside Macintosh Volume V (1986), V-306.
            PpcImportAction::Return(0)
        }
        PpcSystemCompatibilityOperation::GetSysDirection => {
            // GetSysDirection returns the SysDirection global: zero for
            // left-to-right or -1 for right-to-left text.
            // FUNCTION GetSysDirection: Integer;
            // Inside Macintosh: Text (1993), pp. 6-10 and 6-76.
            PpcImportAction::Return(ppc_i16_result(
                memory.read_u16_be(0x0BAC).unwrap_or(0) as i16,
            ))
        }
        PpcSystemCompatibilityOperation::GetEvQHdr => {
            // GetEvQHdr returns the address of the EventQueue low-memory QHdr.
            // FUNCTION GetEvQHdr: QHdrPtr;
            // Inside Macintosh Volume II (1985), II-71; Volume III, low-memory globals.
            PpcImportAction::Return(0x014A)
        }
        PpcSystemCompatibilityOperation::KeyTranslate => PpcImportAction::Return(
            ppc_key_translate(memory, cpu.gpr[3], cpu.gpr[4] as u16, cpu.gpr[5]),
        ),
        PpcSystemCompatibilityOperation::GetScriptManagerVariable => {
            // smKCHRCache (38) is the current KCHR data pointer, used by
            // KeyTranslate to map virtual keys through the keyboard layout.
            // Inside Macintosh: Text (1993), pp. 6-61 and C-18--C-20.
            let result = if cpu.gpr[3] as u16 == 38 {
                if toolbox_startup.kchr_cache_ptr == 0 {
                    let bytes = crate::trap::dispatch::standard_us_kchr_bytes();
                    let ptr = ppc_process_heap_alloc(
                        process_memory_manager,
                        memory,
                        heap_cursor,
                        bytes.len() as u32,
                        false,
                    );
                    if ptr != 0 && memory.write_bytes(ptr, &bytes).is_some() {
                        toolbox_startup.kchr_cache_ptr = ptr;
                    } else {
                        *last_mem_error = PPC_MEM_FULL_ERR;
                    }
                }
                toolbox_startup.kchr_cache_ptr
            } else {
                0
            };
            PpcImportAction::Return(result)
        }
        PpcSystemCompatibilityOperation::GetScriptVariable
        | PpcSystemCompatibilityOperation::GetScript
        | PpcSystemCompatibilityOperation::CallComponentUpp => PpcImportAction::Return(0),
        PpcSystemCompatibilityOperation::LaunchApplication => {
            PpcImportAction::Return(ppc_i16_result(PPC_PROC_NOT_FOUND_ERR))
        }
        PpcSystemCompatibilityOperation::BuildDdPwds => PpcImportAction::ReturnPreserve,
        PpcSystemCompatibilityOperation::MidiAddPort
        | PpcSystemCompatibilityOperation::MidiRemovePort
        | PpcSystemCompatibilityOperation::MidiSignOut
        | PpcSystemCompatibilityOperation::MidiWritePacket => {
            PpcImportAction::Return(ppc_i16_result(PPC_NOT_ENOUGH_HARDWARE_ERR))
        }
        PpcSystemCompatibilityOperation::ResetAlertStage => {
            let stage = crate::dialog_manager::evaluate_reset_alert_stage();
            let _ = memory.write_u16_be(crate::memory::globals::addr::ALERT_STAGE, stage);
            PpcImportAction::ReturnPreserve
        }
        PpcSystemCompatibilityOperation::DiLoad
        | PpcSystemCompatibilityOperation::DiUnload
        | PpcSystemCompatibilityOperation::Debugger
        | PpcSystemCompatibilityOperation::InitCrm
        | PpcSystemCompatibilityOperation::InitCtbUtilities => PpcImportAction::ReturnPreserve,
    }
}

fn ppc_remove_notification(memory: &mut PpcSectionMem, requests: &mut crate::process_context::SharedProcessNotificationQueue, request: u32) -> i16 {
    let Some(index) = requests.iter().position(|&entry| entry == request) else { return -1; };
    requests.remove(index);
    let _ = memory.write_u32_be(request, 0);
    if index > 0 { let _ = memory.write_u32_be(requests[index - 1], requests.get(index).copied().unwrap_or(0)); }
    0
}
