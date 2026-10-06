use super::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PpcSystemCompatibilityOperation {
    BuildDdPwds,
    CtbGetCtbVersion,
    CallComponentUpp,
    DiBadMount,
    DiLoad,
    DiUnload,
    Debugger,
    Dequeue,
    Enqueue,
    FindNextComponent,
    GetNextProcess,
    GetScript,
    GetScriptManagerVariable,
    GetScriptVariable,
    GetSysBeepVolume,
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
) -> PpcImportAction {
    match operation {
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
        PpcSystemCompatibilityOperation::SystemEdit
        | PpcSystemCompatibilityOperation::DiBadMount => PpcImportAction::Return(0),
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
        PpcSystemCompatibilityOperation::GetScriptManagerVariable
        | PpcSystemCompatibilityOperation::GetScriptVariable
        | PpcSystemCompatibilityOperation::GetScript
        | PpcSystemCompatibilityOperation::KeyTranslate
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
        | PpcSystemCompatibilityOperation::InitCtbUtilities
        | PpcSystemCompatibilityOperation::NmRemove => PpcImportAction::ReturnPreserve,
    }
}
