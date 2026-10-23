use super::memory::ppc_memory_read_bytes;
use super::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PpcSystemCompatibilityOperation {
    BuildDdPwds,
    CtbGetCtbVersion,
    CallComponentUpp,
    Char2Pixel,
    CharByte,
    CharType,
    CompareString,
    CompareText,
    DiBadMount,
    DiFormat,
    DiLoad,
    DiUnload,
    DiVerify,
    DiZero,
    DrawJust,
    Debugger,
    Dequeue,
    Enqueue,
    FindNextComponent,
    GetComponentRefcon,
    SetComponentRefcon,
    FindScriptRun,
    FindWord,
    FindWordBreaks,
    IdenticalString,
    IdenticalText,
    IuClearCache,
    IuLangOrder,
    IuMagIdString,
    IuMagString,
    IuMetric,
    IuScriptOrder,
    IuStringOrder,
    IuTextOrder,
    Font2Script,
    FontScript,
    GetNextProcess,
    GetEvQHdr,
    GetFormatOrder,
    GetScript,
    GetScriptManagerVariable,
    GetScriptVariable,
    GetSysBeepVolume,
    GetSysDirection,
    HiliteText,
    IuCompString,
    IuDateString,
    IuDatePString,
    IuLDateString,
    IuTimeString,
    IuTimePString,
    IuLTimeString,
    IuEqualString,
    InitCrm,
    InitCtbUtilities,
    IntlScript,
    KeyTranslate,
    LaunchApplication,
    LmGetCurApName,
    LmGetSysFontFam,
    LmGetSysFontSize,
    MeasureJust,
    MidiAddPort,
    MidiRemovePort,
    MidiSignOut,
    MidiWritePacket,
    Munger,
    NFindWord,
    NmInstall,
    NmRemove,
    ObscureCursor,
    OpenDefaultComponent,
    ParseTable,
    Pixel2Char,
    PortionText,
    ReplaceText,
    ResetAlertStage,
    SetFrontProcess,
    SetIntlResource,
    SetSysDirection,
    StyledLineBreak,
    SystemEdit,
    Transliterate,
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
    let ptr1 = cpu.gpr[5];
    let len1 = cpu.gpr[6] as i32;
    let ptr2 = cpu.gpr[7];
    let len2 = cpu.gpr[8] as i32;

    if handle == 0 || offset < 0 {
        *last_mem_error = PPC_NIL_HANDLE_ERR;
        return u32::MAX;
    }
    let Some(contents) = ppc_handle_bytes(memory, handles, handle) else {
        *last_mem_error = PPC_NIL_HANDLE_ERR;
        return u32::MAX;
    };
    let needle = if ptr1 != 0 && len1 > 0 {
        match ppc_memory_read_bytes(memory, ptr1, len1 as u32) {
            Some(bytes) => bytes,
            None => {
                *last_mem_error = PPC_PARAM_ERR;
                return u32::MAX;
            }
        }
    } else {
        Vec::new()
    };
    let replacement = if ptr2 != 0 && len2 > 0 {
        match ppc_memory_read_bytes(memory, ptr2, len2 as u32) {
            Some(bytes) => bytes,
            None => {
                *last_mem_error = PPC_PARAM_ERR;
                return u32::MAX;
            }
        }
    } else {
        Vec::new()
    };

    let eval = crate::text_utils::evaluate_munger(
        &contents,
        offset,
        ptr1 == 0,
        &needle,
        len1,
        ptr2 == 0,
        &replacement,
        len2,
    );

    if eval.return_offset < 0 {
        return u32::MAX;
    }

    if let Some(new_data) = eval.new_data {
        let result = ppc_allocator_view_resize_handle(
            allocator,
            memory,
            heap_cursor,
            heap_limit,
            last_mem_error,
            handles,
            handle,
            u32::try_from(new_data.len()).unwrap_or(u32::MAX),
        );
        *last_mem_error = result;
        if result != PPC_NO_ERR {
            return u32::MAX;
        }
        let Some(ptr) = memory.read_u32_be(handle) else {
            return u32::MAX;
        };
        if memory.write_bytes(ptr, &new_data).is_none() {
            *last_mem_error = PPC_PARAM_ERR;
            return u32::MAX;
        }
    } else {
        *last_mem_error = PPC_NO_ERR;
    }

    eval.return_offset as u32
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn ppc_replace_text_compatibility(
    allocator: Option<&mut PpcProcessAllocatorView<'_>>,
    memory: &mut PpcSectionMem,
    heap_cursor: &mut u32,
    heap_limit: u32,
    last_mem_error: &mut i16,
    handles: &mut Vec<PpcHandleRecord>,
    base_handle: u32,
    substitution_handle: u32,
    key_ptr: u32,
) -> i16 {
    if base_handle == 0 || substitution_handle == 0 {
        *last_mem_error = crate::text_utils::NIL_HANDLE_ERR;
        return crate::text_utils::NIL_HANDLE_ERR;
    }
    let Some(base_bytes) = ppc_handle_bytes(memory, handles, base_handle) else {
        *last_mem_error = crate::text_utils::MEM_WZ_ERR;
        return crate::text_utils::MEM_WZ_ERR;
    };
    let Some(substitution_bytes) = ppc_handle_bytes(memory, handles, substitution_handle) else {
        *last_mem_error = crate::text_utils::MEM_WZ_ERR;
        return crate::text_utils::MEM_WZ_ERR;
    };
    let key = if key_ptr == 0 {
        Vec::new()
    } else {
        ppc_read_pstring_bytes(memory, key_ptr).unwrap_or_default()
    };
    if key.is_empty() {
        *last_mem_error = PPC_NO_ERR;
        return 0;
    }

    let (substitutions, new_data) = crate::text_utils::evaluate_replace_text(
        &base_bytes,
        &substitution_bytes,
        &key,
        crate::text_utils::SM_ROMAN,
    );
    if substitutions == 0 {
        *last_mem_error = PPC_NO_ERR;
        return 0;
    }

    if let Some(new_data) = new_data {
        let resize_err = ppc_allocator_view_resize_handle(
            allocator,
            memory,
            heap_cursor,
            heap_limit,
            last_mem_error,
            handles,
            base_handle,
            u32::try_from(new_data.len()).unwrap_or(u32::MAX),
        );
        *last_mem_error = resize_err;
        if resize_err != PPC_NO_ERR {
            return crate::text_utils::MEM_FULL_ERR;
        }
        if !new_data.is_empty() {
            if let Some(ptr) = memory.read_u32_be(base_handle) {
                if memory.write_bytes(ptr, &new_data).is_none() {
                    *last_mem_error = PPC_PARAM_ERR;
                    return crate::text_utils::MEM_FULL_ERR;
                }
            }
        }
    }

    *last_mem_error = PPC_NO_ERR;
    substitutions
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn ppc_transliterate_compatibility(
    allocator: Option<&mut PpcProcessAllocatorView<'_>>,
    memory: &mut PpcSectionMem,
    heap_cursor: &mut u32,
    heap_limit: u32,
    last_mem_error: &mut i16,
    handles: &mut Vec<PpcHandleRecord>,
    src_handle: u32,
    dst_handle: u32,
    target: i16,
    src_mask: u32,
) -> i16 {
    if src_handle == 0 || dst_handle == 0 {
        *last_mem_error = crate::text_utils::NIL_HANDLE_ERR;
        return crate::text_utils::NIL_HANDLE_ERR;
    }
    let Some(src_bytes) = ppc_handle_bytes(memory, handles, src_handle) else {
        *last_mem_error = crate::text_utils::MEM_WZ_ERR;
        return crate::text_utils::MEM_WZ_ERR;
    };
    let (trans, _) = crate::text_utils::evaluate_transliterate(&src_bytes, target, src_mask);
    let resize_err = ppc_allocator_view_resize_handle(
        allocator,
        memory,
        heap_cursor,
        heap_limit,
        last_mem_error,
        handles,
        dst_handle,
        u32::try_from(trans.len()).unwrap_or(u32::MAX),
    );
    *last_mem_error = resize_err;
    if resize_err != PPC_NO_ERR {
        return crate::text_utils::MEM_FULL_ERR;
    }
    if !trans.is_empty() {
        if let Some(ptr) = memory.read_u32_be(dst_handle) {
            if memory.write_bytes(ptr, &trans).is_none() {
                *last_mem_error = PPC_PARAM_ERR;
                return crate::text_utils::MEM_FULL_ERR;
            }
        }
    }
    *last_mem_error = PPC_NO_ERR;
    0
}

pub(crate) fn ppc_enqueue_compatibility(
    memory: &mut PpcSectionMem,
    element: u32,
    header: u32,
) -> bool {
    if element == 0
        || header == 0
        || !ppc_memory_can_write_bytes(memory, element, 4)
        || !ppc_memory_can_write_bytes(memory, header, crate::queue_utilities::Q_HDR_SIZE)
    {
        return false;
    }
    let head = memory
        .read_u32_be(header + crate::queue_utilities::Q_HEAD_OFFSET)
        .unwrap_or(0);
    let tail = memory
        .read_u32_be(header + crate::queue_utilities::Q_TAIL_OFFSET)
        .unwrap_or(0);
    let eval = crate::queue_utilities::evaluate_enqueue(element, header, head, tail, |ptr| {
        memory
            .read_u32_be(ptr + crate::queue_utilities::Q_LINK_OFFSET)
            .unwrap_or(0)
    });
    let mut ok = true;
    for (addr, val) in eval.writes {
        if memory.write_u32_be(addr, val).is_none() {
            ok = false;
        }
    }
    ok
}

pub(crate) fn ppc_dequeue_compatibility(
    memory: &mut PpcSectionMem,
    element: u32,
    header: u32,
) -> i16 {
    if element == 0
        || header == 0
        || !ppc_memory_can_write_bytes(memory, header, crate::queue_utilities::Q_HDR_SIZE)
    {
        return PPC_PARAM_ERR;
    }
    let head = memory
        .read_u32_be(header + crate::queue_utilities::Q_HEAD_OFFSET)
        .unwrap_or(0);
    let tail = memory
        .read_u32_be(header + crate::queue_utilities::Q_TAIL_OFFSET)
        .unwrap_or(0);
    let eval = crate::queue_utilities::evaluate_dequeue(element, header, head, tail, |ptr| {
        memory
            .read_u32_be(ptr + crate::queue_utilities::Q_LINK_OFFSET)
            .unwrap_or(0)
    });
    if eval.result == crate::queue_utilities::NO_ERR {
        for (addr, val) in eval.writes {
            let _ = memory.write_u32_be(addr, val);
        }
        let _ = memory.write_u32_be(element + crate::queue_utilities::Q_LINK_OFFSET, 0);
        PPC_NO_ERR
    } else {
        crate::queue_utilities::Q_ERR
    }
}

fn ppc_write_offset_table(
    memory: &mut PpcSectionMem,
    offsets_ptr: u32,
    table: [(i16, i16); 3],
) -> bool {
    if offsets_ptr == 0 {
        return false;
    }
    for (i, (first, second)) in table.iter().enumerate() {
        let base = offsets_ptr + (i as u32 * 4);
        if memory.write_u16_be(base, *first as u16).is_none()
            || memory.write_u16_be(base + 2, *second as u16).is_none()
        {
            return false;
        }
    }
    true
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
    kchr_cache_ptr: &mut u32,
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
        PpcSystemCompatibilityOperation::ReplaceText => {
            let mut allocator = PpcProcessAllocatorView {
                memory_manager: process_memory_manager,
            };
            let result = ppc_replace_text_compatibility(
                Some(&mut allocator),
                memory,
                heap_cursor,
                heap_limit,
                last_mem_error,
                handles,
                cpu.gpr[3],
                cpu.gpr[4],
                cpu.gpr[5],
            );
            PpcImportAction::Return(ppc_i16_result(result))
        }
        PpcSystemCompatibilityOperation::Font2Script => {
            let font_num = cpu.gpr[3] as i16;
            PpcImportAction::Return(crate::text_utils::font_id_to_script(font_num) as u32)
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
            // PROCEDURE UpperString(VAR theString: Str255; diacSens: Boolean);
            // Inside Macintosh: Text (1993), pp. 5-64--5-65.
            if let Some(mut bytes) = ppc_read_pstring_bytes(memory, cpu.gpr[3]) {
                let diac_sensitive = cpu.gpr[4] != 0;
                crate::text_utils::evaluate_upper_string(&mut bytes, diac_sensitive);
                let _ = ppc_write_pstring_bytes(memory, cpu.gpr[3], &bytes);
            }
            PpcImportAction::ReturnPreserve
        }
        PpcSystemCompatibilityOperation::CompareString
        | PpcSystemCompatibilityOperation::IuCompString => {
            let lhs = ppc_read_pstring_bytes(memory, cpu.gpr[3]).unwrap_or_default();
            let rhs = ppc_read_pstring_bytes(memory, cpu.gpr[4]).unwrap_or_default();
            let ordering = crate::text_utils::evaluate_iu_comp_string(&lhs, &rhs);
            PpcImportAction::Return(ppc_i16_result(ordering))
        }
        PpcSystemCompatibilityOperation::IdenticalString
        | PpcSystemCompatibilityOperation::IuEqualString => {
            // IUEqualString / IdenticalString compares Pascal strings using primary ordering:
            // case and diacritic differences do not matter, and it returns
            // zero for equality. Inside Macintosh I (1985), p. I-506.
            // FUNCTION IUEqualString(aStr,bStr: Str255): INTEGER;
            let lhs = ppc_read_pstring_bytes(memory, cpu.gpr[3]).unwrap_or_default();
            let rhs = ppc_read_pstring_bytes(memory, cpu.gpr[4]).unwrap_or_default();
            let equal = crate::text_utils::evaluate_iu_equal_string(&lhs, &rhs);
            PpcImportAction::Return(u32::from(!equal))
        }
        PpcSystemCompatibilityOperation::CompareText
        | PpcSystemCompatibilityOperation::IuMagString => {
            let a_ptr = cpu.gpr[3];
            let b_ptr = cpu.gpr[4];
            let a_len = (cpu.gpr[5] as i16).max(0) as usize;
            let b_len = (cpu.gpr[6] as i16).max(0) as usize;
            let a = if a_ptr != 0 && a_len > 0 {
                ppc_memory_read_bytes(memory, a_ptr, a_len as u32).unwrap_or_default()
            } else {
                Vec::new()
            };
            let b = if b_ptr != 0 && b_len > 0 {
                ppc_memory_read_bytes(memory, b_ptr, b_len as u32).unwrap_or_default()
            } else {
                Vec::new()
            };
            let result = crate::text_utils::evaluate_mag_string(&a, &b);
            PpcImportAction::Return(ppc_i16_result(result))
        }
        PpcSystemCompatibilityOperation::IdenticalText
        | PpcSystemCompatibilityOperation::IuMagIdString => {
            let a_ptr = cpu.gpr[3];
            let b_ptr = cpu.gpr[4];
            let a_len = (cpu.gpr[5] as i16).max(0) as usize;
            let b_len = (cpu.gpr[6] as i16).max(0) as usize;
            let a = if a_ptr != 0 && a_len > 0 {
                ppc_memory_read_bytes(memory, a_ptr, a_len as u32).unwrap_or_default()
            } else {
                Vec::new()
            };
            let b = if b_ptr != 0 && b_len > 0 {
                ppc_memory_read_bytes(memory, b_ptr, b_len as u32).unwrap_or_default()
            } else {
                Vec::new()
            };
            let result = crate::text_utils::evaluate_mag_id_string(&a, &b);
            PpcImportAction::Return(result as u32)
        }
        PpcSystemCompatibilityOperation::IuClearCache => PpcImportAction::ReturnPreserve,
        PpcSystemCompatibilityOperation::IuMetric => {
            PpcImportAction::Return(u32::from(crate::text_utils::evaluate_iu_metric()))
        }
        PpcSystemCompatibilityOperation::IuScriptOrder => {
            let script1 = cpu.gpr[3] as i16;
            let script2 = cpu.gpr[4] as i16;
            let result = crate::text_utils::evaluate_script_order(script1, script2);
            PpcImportAction::Return(ppc_i16_result(result))
        }
        PpcSystemCompatibilityOperation::IuLangOrder => {
            let lang1 = cpu.gpr[3] as i16;
            let lang2 = cpu.gpr[4] as i16;
            let result = crate::text_utils::evaluate_lang_order(lang1, lang2);
            PpcImportAction::Return(ppc_i16_result(result))
        }
        PpcSystemCompatibilityOperation::IuStringOrder => {
            let a = ppc_read_pascal_string(memory, cpu.gpr[3]).unwrap_or_default();
            let b = ppc_read_pascal_string(memory, cpu.gpr[4]).unwrap_or_default();
            let result = crate::text_utils::evaluate_string_order(
                &a,
                &b,
                cpu.gpr[5] as i16,
                cpu.gpr[6] as i16,
                cpu.gpr[7] as i16,
                cpu.gpr[8] as i16,
            );
            PpcImportAction::Return(ppc_i16_result(result))
        }
        PpcSystemCompatibilityOperation::IuTextOrder => {
            let a_ptr = cpu.gpr[3];
            let b_ptr = cpu.gpr[4];
            let a_len = (cpu.gpr[5] as i16).max(0) as usize;
            let b_len = (cpu.gpr[6] as i16).max(0) as usize;
            let a_script = cpu.gpr[7] as i16;
            let b_script = cpu.gpr[8] as i16;
            let a_lang = cpu.gpr[9] as i16;
            let b_lang = cpu.gpr[10] as i16;
            let a = if a_ptr != 0 && a_len > 0 {
                ppc_memory_read_bytes(memory, a_ptr, a_len as u32).unwrap_or_default()
            } else {
                Vec::new()
            };
            let b = if b_ptr != 0 && b_len > 0 {
                ppc_memory_read_bytes(memory, b_ptr, b_len as u32).unwrap_or_default()
            } else {
                Vec::new()
            };
            let result =
                crate::text_utils::evaluate_text_order(&a, &b, a_script, b_script, a_lang, b_lang);
            PpcImportAction::Return(ppc_i16_result(result))
        }
        PpcSystemCompatibilityOperation::TruncText => {
            let width = cpu.gpr[3] as u16 as i16;
            let text = cpu.gpr[4];
            let length_ptr = cpu.gpr[5];
            let trunc_where = cpu.gpr[6] as u16;
            let Some(length) = memory.read_u16_be(length_ptr).map(usize::from) else {
                return PpcImportAction::Return(ppc_i16_result(crate::text_utils::TRUNC_ERR));
            };
            let mut bytes = Vec::with_capacity(length);
            for index in 0..length {
                let Some(b) = memory.read_u8(text + index as u32) else {
                    return PpcImportAction::Return(ppc_i16_result(crate::text_utils::TRUNC_ERR));
                };
                bytes.push(b);
            }
            let eval = crate::text_utils::evaluate_trunc_text(
                &bytes,
                width,
                trunc_where,
                crate::text_utils::MAC_ROMAN_ELLIPSIS,
                crate::text_utils::default_char_width_measure,
            );
            if let Some(new_text) = eval.new_text {
                if ppc_memory_can_write_bytes(memory, text, new_text.len() as u32)
                    && memory.write_bytes(text, &new_text).is_some()
                    && memory
                        .write_u16_be(length_ptr, new_text.len() as u16)
                        .is_some()
                {
                    PpcImportAction::Return(ppc_i16_result(eval.result))
                } else {
                    PpcImportAction::Return(ppc_i16_result(crate::text_utils::TRUNC_ERR))
                }
            } else {
                PpcImportAction::Return(ppc_i16_result(eval.result))
            }
        }
        PpcSystemCompatibilityOperation::StyledLineBreak => {
            let text_ptr = cpu.gpr[3];
            let text_len = cpu.gpr[4];
            let text_start = cpu.gpr[5];
            let text_end = cpu.gpr[6];
            let _flags = cpu.gpr[7];
            let width_ptr = cpu.gpr[8];
            let offset_ptr = cpu.gpr[9];

            let raw_width = if width_ptr != 0 {
                memory.read_u32_be(width_ptr).unwrap_or(0)
            } else {
                0x7FFF
            };
            let input_offset = if offset_ptr != 0 {
                memory.read_u32_be(offset_ptr).unwrap_or(0)
            } else {
                0
            };
            let first_style_run_on_line = input_offset != 0;
            let available_pixels = crate::text_utils::fixed_to_pixels(raw_width);

            let bytes = if text_ptr != 0 && text_len > 0 {
                ppc_memory_read_bytes(memory, text_ptr, text_len).unwrap_or_default()
            } else {
                Vec::new()
            };

            let eval = crate::text_utils::evaluate_styled_line_break(
                &bytes,
                text_start,
                text_end,
                available_pixels,
                first_style_run_on_line,
                crate::text_utils::default_char_advance_measure,
            );

            if offset_ptr != 0 {
                let _ = memory.write_u32_be(offset_ptr, eval.output_offset);
            }
            if width_ptr != 0 {
                let remaining_raw =
                    crate::text_utils::pixels_to_width_like(raw_width, eval.remaining_width);
                let _ = memory.write_u32_be(width_ptr, remaining_raw);
            }
            PpcImportAction::Return(u32::from(eval.code))
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
        PpcSystemCompatibilityOperation::IuDateString
        | PpcSystemCompatibilityOperation::IuDatePString => {
            let intl_handle = if operation == PpcSystemCompatibilityOperation::IuDatePString {
                cpu.gpr[6]
            } else {
                0
            };
            crate::date_format::ppc_write_date_string(
                memory,
                cpu.gpr[3],
                cpu.gpr[4] as i16,
                cpu.gpr[5],
                intl_handle,
            );
            PpcImportAction::ReturnPreserve
        }
        PpcSystemCompatibilityOperation::IuTimeString
        | PpcSystemCompatibilityOperation::IuTimePString => {
            let intl_handle = if operation == PpcSystemCompatibilityOperation::IuTimePString {
                cpu.gpr[6]
            } else {
                0
            };
            crate::date_format::ppc_write_time_string(
                memory,
                cpu.gpr[3],
                cpu.gpr[4] != 0,
                cpu.gpr[5],
                intl_handle,
            );
            PpcImportAction::ReturnPreserve
        }
        PpcSystemCompatibilityOperation::IuLDateString => {
            let intl_handle = cpu.gpr[6];
            crate::date_format::ppc_write_long_date_string(
                memory,
                cpu.gpr[3],
                cpu.gpr[4] as i16,
                cpu.gpr[5],
                intl_handle,
            );
            PpcImportAction::ReturnPreserve
        }
        PpcSystemCompatibilityOperation::IuLTimeString => {
            let intl_handle = cpu.gpr[6];
            crate::date_format::ppc_write_long_time_string(
                memory,
                cpu.gpr[3],
                cpu.gpr[4] != 0,
                cpu.gpr[5],
                intl_handle,
            );
            PpcImportAction::ReturnPreserve
        }
        PpcSystemCompatibilityOperation::SetIntlResource => {
            // PROCEDURE IUSetIntl(refNum: Integer; theID: Integer; intlParam: Handle);
            // Inside Macintosh Volume I, I-487, I-503; Inside Macintosh: Text (1993), p. 6-91.
            // HLE does not persist custom INTL overrides. Return cleanly.
            PpcImportAction::ReturnPreserve
        }
        PpcSystemCompatibilityOperation::SystemEdit => {
            let edit_cmd = cpu.gpr[3] as i16;
            let params = crate::desk_manager::evaluate_system_edit_parameters(edit_cmd);
            let result = if params.handled() { 1 } else { 0 };
            PpcImportAction::Return(result)
        }
        PpcSystemCompatibilityOperation::DiBadMount
        | PpcSystemCompatibilityOperation::DiFormat
        | PpcSystemCompatibilityOperation::DiVerify
        | PpcSystemCompatibilityOperation::DiZero => {
            PpcImportAction::Return(ppc_i16_result(crate::disk_init_manager::NO_ERR))
        }
        PpcSystemCompatibilityOperation::FindNextComponent => {
            if ppc_hle_trace_enabled() {
                let looking = cpu.gpr[4];
                let mut field = |offset| {
                    memory
                        .read_u32_be(looking.wrapping_add(offset))
                        .map(|value| String::from_utf8_lossy(&value.to_be_bytes()).into_owned())
                };
                eprintln!(
                    "[PPC-TRACE] FindNextComponent after=${:08X} type={:?} subtype={:?} manufacturer={:?}",
                    cpu.gpr[3],
                    field(0),
                    field(4),
                    field(8)
                );
            }
            PpcImportAction::Return(0)
        }
        PpcSystemCompatibilityOperation::OpenDefaultComponent => PpcImportAction::Return(0),
        PpcSystemCompatibilityOperation::GetComponentRefcon => {
            // long GetComponentRefcon(Component aComponent). FindNextComponent
            // registers no components in this runtime, so every reference is
            // invalid and reads as a zero refcon (Inside Macintosh: More
            // Macintosh Toolbox, Component Manager).
            PpcImportAction::Return(0)
        }
        PpcSystemCompatibilityOperation::SetComponentRefcon => {
            // void SetComponentRefcon(Component aComponent, long theRefcon):
            // an invalid component reference changes nothing.
            PpcImportAction::ReturnPreserve
        }
        PpcSystemCompatibilityOperation::CtbGetCtbVersion => PpcImportAction::Return(0x0200),
        PpcSystemCompatibilityOperation::IntlScript
        | PpcSystemCompatibilityOperation::FontScript => {
            // Inside Macintosh: Text (1993), pp. 6-22–6-24: IntlScript and
            // FontScript return enabled script codes, defaulting to the
            // Roman script (smRoman = 0).
            PpcImportAction::Return(0)
        }
        PpcSystemCompatibilityOperation::CharByte => {
            // CharByte identifies a byte's place in a multibyte character.
            // FUNCTION CharByte(textBuf: Ptr; textOffset: Integer): Integer;
            // Inside Macintosh Volume V (1986), V-306; Inside Macintosh: Text (1993), pp. 6-78–6-79.
            let text_ptr = cpu.gpr[3];
            let offset = cpu.gpr[4] as i16;
            if text_ptr == 0 || offset < 0 {
                return PpcImportAction::Return(0);
            }
            let len = (offset as usize + 2).min(256);
            let bytes = ppc_memory_read_bytes(memory, text_ptr, len as u32).unwrap_or_default();
            let result = crate::text_utils::evaluate_char_byte(
                &bytes,
                offset as usize,
                crate::text_utils::SM_ROMAN,
            );
            PpcImportAction::Return(ppc_i16_result(result))
        }
        PpcSystemCompatibilityOperation::CharType => {
            // CharType returns information about character type and direction.
            // FUNCTION CharType(textBuf: Ptr; textOffset: Integer): Integer;
            // Inside Macintosh: Text (1993), pp. 6-79–6-80.
            let text_ptr = cpu.gpr[3];
            let offset = cpu.gpr[4] as i16;
            if text_ptr == 0 || offset < 0 {
                return PpcImportAction::Return(0);
            }
            let len = (offset as usize + 1).min(256);
            let bytes = ppc_memory_read_bytes(memory, text_ptr, len as u32).unwrap_or_default();
            let result = crate::text_utils::evaluate_char_type(&bytes, offset as usize);
            PpcImportAction::Return(ppc_i16_result(result))
        }
        PpcSystemCompatibilityOperation::HiliteText => {
            // PROCEDURE HiliteText(textPtr: Ptr; textLength, firstOffset, secondOffset: Integer; VAR offsets: OffsetTable);
            // Inside Macintosh: Text (1993), pp. 5-75–5-77.
            let text_len = (cpu.gpr[4] as i16).max(0) as usize;
            let first_offset = (cpu.gpr[5] as i16).max(0) as usize;
            let second_offset = (cpu.gpr[6] as i16).max(0) as usize;
            let offsets_ptr = cpu.gpr[7];
            if offsets_ptr != 0 {
                let table =
                    crate::text_utils::evaluate_hilite_text(text_len, first_offset, second_offset);
                let _ = ppc_write_offset_table(memory, offsets_ptr, table);
            }
            PpcImportAction::ReturnPreserve
        }
        PpcSystemCompatibilityOperation::FindWord
        | PpcSystemCompatibilityOperation::NFindWord
        | PpcSystemCompatibilityOperation::FindWordBreaks => {
            // PROCEDURE FindWord(textPtr: Ptr; textLength, offset: Integer; leadingEdge: Boolean; breaksPtr: Ptr; VAR offsets: OffsetTable);
            // PROCEDURE NFindWord(textPtr: Ptr; textLength, offset: Integer; leadingEdge: Boolean; nbreaks: NBreakTablePtr; VAR offsets: OffsetTable);
            // PROCEDURE FindWordBreaks(textPtr: Ptr; textLength, offset: Integer; leadingEdge: Boolean; breaks: BreakTablePtr; VAR offsets: OffsetTable; script: ScriptCode);
            // Inside Macintosh: Text (1993), pp. 5-68–5-72.
            let text_ptr = cpu.gpr[3];
            let text_len = (cpu.gpr[4] as i16).max(0) as usize;
            let offset = (cpu.gpr[5] as i16).max(0) as usize;
            let offsets_ptr = cpu.gpr[8];
            if offsets_ptr != 0 {
                let bytes = if text_ptr != 0 && text_len > 0 {
                    ppc_memory_read_bytes(memory, text_ptr, text_len as u32).unwrap_or_default()
                } else {
                    Vec::new()
                };
                let table = crate::text_utils::evaluate_find_word(&bytes, offset);
                let _ = ppc_write_offset_table(memory, offsets_ptr, table);
            }
            PpcImportAction::ReturnPreserve
        }
        PpcSystemCompatibilityOperation::FindScriptRun => {
            // FUNCTION FindScriptRun(textPtr: Ptr; textLength: LongInt; VAR runLen: LongInt): ScriptCode;
            // Inside Macintosh: Text (1993), pp. 5-71–5-73.
            let text_len = (cpu.gpr[4] as i32).max(0) as usize;
            let run_len_ptr = cpu.gpr[5];
            let (script_code, run_len) =
                crate::text_utils::evaluate_find_script_run(text_len, crate::text_utils::SM_ROMAN);
            if run_len_ptr != 0 {
                let _ = memory.write_u32_be(run_len_ptr, run_len as u32);
            }
            PpcImportAction::Return(script_code as u32)
        }
        PpcSystemCompatibilityOperation::ParseTable => {
            // FUNCTION ParseTable(VAR table: CharByteTable): Boolean;
            // Inside Macintosh: Text (1993), pp. 6-77–6-78.
            let table_ptr = cpu.gpr[3];
            let mut table = [0u8; 256];
            let has_2byte =
                crate::text_utils::evaluate_parse_table(&mut table, crate::text_utils::SM_ROMAN);
            if table_ptr != 0 {
                let _ = memory.write_bytes(table_ptr, &table);
            }
            PpcImportAction::Return(if has_2byte { 1 } else { 0 })
        }
        PpcSystemCompatibilityOperation::PortionText => {
            // FUNCTION PortionText(textPtr: Ptr; textLength: LongInt): Fixed;
            // Inside Macintosh: Text (1993), p. 5-73.
            let text_ptr = cpu.gpr[3];
            let text_len = (cpu.gpr[4] as i32).max(0) as usize;
            let bytes = if text_ptr != 0 && text_len > 0 {
                ppc_memory_read_bytes(memory, text_ptr, text_len as u32).unwrap_or_default()
            } else {
                Vec::new()
            };
            let portion = crate::text_utils::evaluate_portion_text(&bytes);
            PpcImportAction::Return(portion as u32)
        }
        PpcSystemCompatibilityOperation::GetFormatOrder => {
            // PROCEDURE GetFormatOrder(ordering: FormatOrderPtr; firstFormat, lastFormat: short;
            //   lineRight: Boolean; rlDirProc: StyleRunDirectionUPP; dirParam: void*);
            // Inside Macintosh: Text (1993), pp. 5-64–5-68.
            let ordering_ptr = cpu.gpr[3];
            let first_format = cpu.gpr[4] as i16;
            let last_format = cpu.gpr[5] as i16;
            let line_right = cpu.gpr[6] != 0;
            let order =
                crate::text_utils::evaluate_get_format_order(first_format, last_format, line_right);
            if ordering_ptr != 0 {
                for (i, &val) in order.iter().enumerate() {
                    let _ = memory.write_u16_be(ordering_ptr + (i * 2) as u32, val as u16);
                }
            }
            PpcImportAction::ReturnPreserve
        }
        PpcSystemCompatibilityOperation::Char2Pixel => {
            // FUNCTION Char2Pixel(textBuf: Ptr; textLen, slop, offset, direction: short): short;
            // Inside Macintosh: Text (1993), pp. 5-54–5-56.
            let text_ptr = cpu.gpr[3];
            let text_len = (cpu.gpr[4] as i16).max(0) as u32;
            let slop = cpu.gpr[5] as i16;
            let offset = cpu.gpr[6] as i16;
            let direction = cpu.gpr[7] as i16;
            let bytes = if text_ptr != 0 && text_len > 0 {
                ppc_memory_read_bytes(memory, text_ptr, text_len).unwrap_or_default()
            } else {
                Vec::new()
            };
            let pixels = crate::text_utils::evaluate_char2pixel(&bytes, slop, offset, direction);
            PpcImportAction::Return(ppc_i16_result(pixels))
        }
        PpcSystemCompatibilityOperation::Pixel2Char => {
            // FUNCTION Pixel2Char(textBuf: Ptr; textLen, slop, pixelWidth: short; VAR leadingEdge: Boolean): short;
            // Inside Macintosh: Text (1993), pp. 5-56–5-60.
            let text_ptr = cpu.gpr[3];
            let text_len = (cpu.gpr[4] as i16).max(0) as u32;
            let slop = cpu.gpr[5] as i16;
            let pixel_width = cpu.gpr[6] as i16;
            let leading_edge_ptr = cpu.gpr[7];
            let bytes = if text_ptr != 0 && text_len > 0 {
                ppc_memory_read_bytes(memory, text_ptr, text_len).unwrap_or_default()
            } else {
                Vec::new()
            };
            let (offset, leading_edge) =
                crate::text_utils::evaluate_pixel2char(&bytes, slop, pixel_width);
            if leading_edge_ptr != 0 {
                let _ = memory.write_u8(leading_edge_ptr, if leading_edge { 1 } else { 0 });
            }
            PpcImportAction::Return(ppc_i16_result(offset))
        }
        PpcSystemCompatibilityOperation::DrawJust => PpcImportAction::ReturnPreserve,
        PpcSystemCompatibilityOperation::MeasureJust => {
            // PROCEDURE MeasureJust(textPtr: Ptr; textLength, slop: short; charLocs: Ptr);
            // Inside Macintosh: Text (1993), pp. 5-54–5-56.
            let text_ptr = cpu.gpr[3];
            let text_len = (cpu.gpr[4] as i16).max(0) as u32;
            let slop = cpu.gpr[5] as i16;
            let char_locs = cpu.gpr[6];
            let bytes = if text_ptr != 0 && text_len > 0 {
                ppc_memory_read_bytes(memory, text_ptr, text_len).unwrap_or_default()
            } else {
                Vec::new()
            };
            let locs = crate::text_utils::evaluate_measure_just(&bytes, slop);
            if char_locs != 0 {
                for (i, &loc) in locs.iter().enumerate() {
                    let _ = memory.write_u16_be(char_locs + (i * 2) as u32, loc as u16);
                }
            }
            PpcImportAction::ReturnPreserve
        }
        PpcSystemCompatibilityOperation::Transliterate => {
            let mut allocator = PpcProcessAllocatorView {
                memory_manager: process_memory_manager,
            };
            let result = ppc_transliterate_compatibility(
                Some(&mut allocator),
                memory,
                heap_cursor,
                heap_limit,
                last_mem_error,
                handles,
                cpu.gpr[3],
                cpu.gpr[4],
                cpu.gpr[5] as i16,
                cpu.gpr[6],
            );
            PpcImportAction::Return(ppc_i16_result(result))
        }
        PpcSystemCompatibilityOperation::GetSysDirection => {
            // GetSysDirection returns the SysDirection global: zero for
            // left-to-right or -1 for right-to-left text.
            // FUNCTION GetSysDirection: Integer;
            // Inside Macintosh: Text (1993), pp. 6-10 and 6-76.
            let raw = memory
                .read_u16_be(crate::text_utils::SYS_DIRECTION)
                .unwrap_or(0);
            PpcImportAction::Return(ppc_i16_result(
                crate::text_utils::evaluate_get_sys_direction(raw),
            ))
        }
        PpcSystemCompatibilityOperation::SetSysDirection => {
            // SetSysDirection sets the SysDirection low-memory global ($0BAC).
            // PROCEDURE SetSysDirection(direction: Integer);
            // Inside Macintosh: Text (1993), pp. 6-10 and 6-76.
            let _ = memory.write_u16_be(crate::text_utils::SYS_DIRECTION, cpu.gpr[3] as u16);
            PpcImportAction::ReturnPreserve
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
            let selector = cpu.gpr[3] as i16;
            if selector == crate::text_utils::SM_KCHR_CACHE && *kchr_cache_ptr == 0 {
                let bytes = crate::trap::dispatch::standard_us_kchr_bytes();
                let ptr = ppc_process_heap_alloc(
                    process_memory_manager,
                    memory,
                    heap_cursor,
                    bytes.len() as u32,
                    false,
                );
                if ptr != 0 && memory.write_bytes(ptr, &bytes).is_some() {
                    *kchr_cache_ptr = ptr;
                } else {
                    *last_mem_error = PPC_MEM_FULL_ERR;
                }
            }
            let result =
                crate::text_utils::evaluate_script_manager_variable(selector, *kchr_cache_ptr);
            PpcImportAction::Return(result)
        }
        PpcSystemCompatibilityOperation::GetScriptVariable
        | PpcSystemCompatibilityOperation::GetScript => {
            let script = cpu.gpr[3] as i16;
            let selector = cpu.gpr[4] as i16;
            let result = crate::text_utils::evaluate_script_variable(script, selector);
            PpcImportAction::Return(result)
        }
        PpcSystemCompatibilityOperation::CallComponentUpp => PpcImportAction::Return(0),
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
        PpcSystemCompatibilityOperation::NmInstall => {
            let nm_rec = cpu.gpr[3];
            let q_type = if nm_rec != 0 {
                memory
                    .read_u16_be(nm_rec + crate::notification_manager::Q_TYPE_OFFSET)
                    .unwrap_or(0)
            } else {
                0
            };
            let result = match crate::notification_manager::validate_nm_rec(nm_rec, q_type) {
                Ok(()) => {
                    let _ =
                        memory.write_u32_be(nm_rec + crate::notification_manager::Q_LINK_OFFSET, 0);
                    crate::notification_manager::NO_ERR
                }
                Err(err) => err,
            };
            PpcImportAction::Return(ppc_i16_result(result))
        }
        PpcSystemCompatibilityOperation::NmRemove => {
            let nm_rec = cpu.gpr[3];
            let q_type = if nm_rec != 0 {
                memory
                    .read_u16_be(nm_rec + crate::notification_manager::Q_TYPE_OFFSET)
                    .unwrap_or(0)
            } else {
                0
            };
            let result = match crate::notification_manager::validate_nm_rec(nm_rec, q_type) {
                Ok(()) => {
                    let _ =
                        memory.write_u32_be(nm_rec + crate::notification_manager::Q_LINK_OFFSET, 0);
                    crate::notification_manager::NO_ERR
                }
                Err(err) => err,
            };
            PpcImportAction::Return(ppc_i16_result(result))
        }
        PpcSystemCompatibilityOperation::DiLoad
        | PpcSystemCompatibilityOperation::DiUnload
        | PpcSystemCompatibilityOperation::Debugger
        | PpcSystemCompatibilityOperation::InitCrm
        | PpcSystemCompatibilityOperation::InitCtbUtilities => PpcImportAction::ReturnPreserve,
    }
}
