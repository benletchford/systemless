//! Typed Standard C Library dispatch for PowerPC imports.

use super::*;

// Universal Interfaces 3.4 ctype.h defines these C-locale table flags.
pub const PPC_CTYPE_UPP: u8 = 0x01;
pub const PPC_CTYPE_LOW: u8 = 0x02;
pub const PPC_CTYPE_DIG: u8 = 0x04;
pub const PPC_CTYPE_WSP: u8 = 0x08;
pub const PPC_CTYPE_PUN: u8 = 0x10;
pub const PPC_CTYPE_CTL: u8 = 0x20;
pub const PPC_CTYPE_BLA: u8 = 0x40;
pub const PPC_CTYPE_HEX: u8 = 0x80;

pub const fn ppc_ctype_entry(byte: u8) -> u8 {
    match byte {
        b'0'..=b'9' => PPC_CTYPE_DIG | PPC_CTYPE_HEX,
        b'A'..=b'F' => PPC_CTYPE_UPP | PPC_CTYPE_HEX,
        b'G'..=b'Z' => PPC_CTYPE_UPP,
        b'a'..=b'f' => PPC_CTYPE_LOW | PPC_CTYPE_HEX,
        b'g'..=b'z' => PPC_CTYPE_LOW,
        b' ' => PPC_CTYPE_BLA | PPC_CTYPE_WSP,
        b'\t' | b'\n' | 0x0b | 0x0c | b'\r' => PPC_CTYPE_CTL | PPC_CTYPE_WSP,
        0x00..=0x08 | 0x0e..=0x1f | 0x7f => PPC_CTYPE_CTL,
        b'!'..=b'/' | b':'..=b'@' | b'['..=b'`' | b'{'..=b'~' => PPC_CTYPE_PUN,
        _ => 0,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PpcQsortState {
    pub(super) base: u32,
    pub(super) width: u32,
    pub(super) comparator: PpcCallbackTarget,
    pub(super) final_pc: u32,
    pub(super) restore_rtoc: u32,
    pub(super) pass_end: u32,
    pub(super) index: u32,
    pub(super) swapped: bool,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct PpcStdSignalState {
    pub(super) handlers: [u32; 32],
}

pub(super) struct PpcStdCDispatchContext<'a> {
    pub(super) binding: &'a PpcImportBinding,
    pub(super) cpu: &'a mut PpcCpu,
    pub(super) memory: &'a mut PpcSectionMem,
    pub(super) process_memory_manager: &'a mut ProcessNativeMemoryManager,
    pub(super) heap_cursor: &'a mut u32,
    pub(super) last_mem_error: &'a mut i16,
    pub(super) stdc_qsort_stack: &'a mut Vec<PpcQsortState>,
    pub(super) stdc_signal_state: &'a mut PpcStdSignalState,
}

pub(super) fn dispatch_stdc_import(ctx: PpcStdCDispatchContext<'_>) -> Option<PpcImportAction> {
    let PpcStdCDispatchContext {
        binding,
        cpu,
        memory,
        process_memory_manager,
        heap_cursor,
        last_mem_error,
        stdc_qsort_stack,
        stdc_signal_state,
    } = ctx;

    match binding.dispatcher_target {
        PpcImportDispatcherTarget::StdMemset => {
            let destination = cpu.gpr[3];
            let byte = cpu.gpr[4] as u8;
            let count = cpu.gpr[5];
            if ppc_hle_trace_enabled()
                && destination < PPC_MAIN_SCREEN_BASE.saturating_add(ppc_main_screen_buffer_size())
                && destination.saturating_add(count) > PPC_MAIN_SCREEN_BASE
            {
                eprintln!(
                    "[PPC-TRACE] memset screen dst=${destination:08X} byte=${byte:02X} count={count}"
                );
            }
            if ppc_memory_can_write_bytes(memory, destination, count) {
                let bytes = vec![byte; count as usize];
                let _ = memory.write_bytes(destination, &bytes);
            }
            Some(PpcImportAction::Return(destination))
        }
        PpcImportDispatcherTarget::StdMemcmp => Some(PpcImportAction::Return(ppc_std_memcmp(
            memory, cpu.gpr[3], cpu.gpr[4], cpu.gpr[5],
        ) as u32)),
        PpcImportDispatcherTarget::StdMemcpy | PpcImportDispatcherTarget::StdMemmove => {
            let destination = cpu.gpr[3];
            if ppc_hle_trace_enabled()
                && destination < PPC_MAIN_SCREEN_BASE.saturating_add(ppc_main_screen_buffer_size())
                && destination.saturating_add(cpu.gpr[5]) > PPC_MAIN_SCREEN_BASE
            {
                eprintln!(
                    "[PPC-TRACE] {} screen dst=${:08X} src=${:08X} count={}",
                    binding.symbol_name, destination, cpu.gpr[4], cpu.gpr[5]
                );
            }
            ppc_std_memmove(memory, destination, cpu.gpr[4], cpu.gpr[5]);
            Some(PpcImportAction::Return(destination))
        }
        PpcImportDispatcherTarget::StdMalloc => {
            let size = cpu.gpr[3];
            let preserved_mem_error = *last_mem_error;
            let ptr = process_memory_manager.new_native_ptr(memory, size, false);
            process_memory_manager.set_native_mem_error(preserved_mem_error);
            ppc_apply_process_native_allocator(
                process_memory_manager,
                memory,
                heap_cursor,
                last_mem_error,
            );
            Some(PpcImportAction::Return(ptr))
        }
        PpcImportDispatcherTarget::StdFree => {
            let preserved_mem_error = *last_mem_error;
            let _ = process_memory_manager.dispose_native_ptr(cpu.gpr[3]);
            process_memory_manager.set_native_mem_error(preserved_mem_error);
            ppc_apply_process_native_allocator(
                process_memory_manager,
                memory,
                heap_cursor,
                last_mem_error,
            );
            Some(PpcImportAction::ReturnPreserve)
        }
        PpcImportDispatcherTarget::StdCalloc => {
            let count = cpu.gpr[3];
            let ptr = if let Some(size) = count.checked_mul(cpu.gpr[4]).filter(|size| *size != 0) {
                let preserved_mem_error = *last_mem_error;
                let ptr = process_memory_manager.new_native_ptr(memory, size, true);
                process_memory_manager.set_native_mem_error(preserved_mem_error);
                ppc_apply_process_native_allocator(
                    process_memory_manager,
                    memory,
                    heap_cursor,
                    last_mem_error,
                );
                ptr
            } else {
                0
            };
            Some(PpcImportAction::Return(ptr))
        }
        PpcImportDispatcherTarget::StdRealloc => {
            let preserved_mem_error = *last_mem_error;
            let ptr = process_memory_manager.reallocate_native_ptr(memory, cpu.gpr[3], cpu.gpr[4]);
            process_memory_manager.set_native_mem_error(preserved_mem_error);
            ppc_apply_process_native_allocator(
                process_memory_manager,
                memory,
                heap_cursor,
                last_mem_error,
            );
            Some(PpcImportAction::Return(ptr))
        }
        PpcImportDispatcherTarget::StdStrcpy => {
            let destination = cpu.gpr[3];
            let source = cpu.gpr[4];
            let mut offset = 0u32;
            loop {
                let Some(byte) = memory.read_u8(source.wrapping_add(offset)) else {
                    break;
                };
                if memory
                    .write_u8(destination.wrapping_add(offset), byte)
                    .is_none()
                {
                    break;
                }
                offset = offset.wrapping_add(1);
                if byte == 0 {
                    break;
                }
            }
            Some(PpcImportAction::Return(destination))
        }
        PpcImportDispatcherTarget::StdStrncpy => Some(PpcImportAction::Return(ppc_std_strncpy(
            memory, cpu.gpr[3], cpu.gpr[4], cpu.gpr[5],
        ))),
        PpcImportDispatcherTarget::StdStrcat => {
            let destination = cpu.gpr[3];
            ppc_std_strcat(memory, destination, cpu.gpr[4]);
            Some(PpcImportAction::Return(destination))
        }
        PpcImportDispatcherTarget::StdStrncat => Some(PpcImportAction::Return(ppc_std_strncat(
            memory, cpu.gpr[3], cpu.gpr[4], cpu.gpr[5],
        ))),
        PpcImportDispatcherTarget::StdStrcmp => Some(PpcImportAction::Return(ppc_std_strcmp(
            memory, cpu.gpr[3], cpu.gpr[4], None,
        ) as u32)),
        PpcImportDispatcherTarget::StdStrncmp => Some(PpcImportAction::Return(ppc_std_strcmp(
            memory,
            cpu.gpr[3],
            cpu.gpr[4],
            Some(cpu.gpr[5]),
        ) as u32)),
        PpcImportDispatcherTarget::StdStrlen => {
            Some(PpcImportAction::Return(ppc_std_strlen(memory, cpu.gpr[3])))
        }
        PpcImportDispatcherTarget::StdMemchr => Some(PpcImportAction::Return(ppc_std_memchr(
            memory,
            cpu.gpr[3],
            cpu.gpr[4] as u8,
            cpu.gpr[5],
        ))),
        PpcImportDispatcherTarget::StdStrchr => Some(PpcImportAction::Return(ppc_std_strchr(
            memory,
            cpu.gpr[3],
            cpu.gpr[4] as u8,
            false,
        ))),
        PpcImportDispatcherTarget::StdStrrchr => Some(PpcImportAction::Return(ppc_std_strchr(
            memory,
            cpu.gpr[3],
            cpu.gpr[4] as u8,
            true,
        ))),
        PpcImportDispatcherTarget::StdStrspn => Some(PpcImportAction::Return(ppc_std_strspn(
            memory, cpu.gpr[3], cpu.gpr[4], true,
        ))),
        PpcImportDispatcherTarget::StdStrcspn => Some(PpcImportAction::Return(ppc_std_strspn(
            memory, cpu.gpr[3], cpu.gpr[4], false,
        ))),
        PpcImportDispatcherTarget::StdStrpbrk => Some(PpcImportAction::Return(ppc_std_strpbrk(
            memory, cpu.gpr[3], cpu.gpr[4],
        ))),
        PpcImportDispatcherTarget::StdStrstr => Some(PpcImportAction::Return(ppc_std_strstr(
            memory, cpu.gpr[3], cpu.gpr[4],
        ))),
        PpcImportDispatcherTarget::StdAtoi => {
            Some(PpcImportAction::Return(ppc_std_atoi(memory, cpu.gpr[3])))
        }
        PpcImportDispatcherTarget::StdGetenv => Some(PpcImportAction::Return(0)),
        PpcImportDispatcherTarget::StdSprintf => {
            Some(PpcImportAction::Return(ppc_std_sprintf(cpu, memory)))
        }
        PpcImportDispatcherTarget::StdAbs => Some(PpcImportAction::Return(
            (cpu.gpr[3] as i32).wrapping_abs() as u32,
        )),
        PpcImportDispatcherTarget::StdToupper => {
            let value = cpu.gpr[3] as i32;
            let result = if (b'a' as i32..=b'z' as i32).contains(&value) {
                value - i32::from(b'a' - b'A')
            } else {
                value
            };
            Some(PpcImportAction::Return(result as u32))
        }
        PpcImportDispatcherTarget::StdTolower => {
            let value = cpu.gpr[3] as i32;
            let result = if (b'A' as i32..=b'Z' as i32).contains(&value) {
                value + i32::from(b'a' - b'A')
            } else {
                value
            };
            Some(PpcImportAction::Return(result as u32))
        }
        // Universal Interfaces 3.4 ctype.h specifies byte-table masks for the
        // C-locale classifiers and unsigned-int/unsigned-char conversion rules.
        PpcImportDispatcherTarget::StdIsalnum => {
            let byte = cpu.gpr[3] as u8;
            let result = ppc_ctype_entry(byte) & (PPC_CTYPE_UPP | PPC_CTYPE_LOW | PPC_CTYPE_DIG);
            Some(PpcImportAction::Return(u32::from(result)))
        }
        PpcImportDispatcherTarget::StdIsalpha => {
            let byte = cpu.gpr[3] as u8;
            let result = ppc_ctype_entry(byte) & (PPC_CTYPE_UPP | PPC_CTYPE_LOW);
            Some(PpcImportAction::Return(u32::from(result)))
        }
        PpcImportDispatcherTarget::StdIsascii => {
            let result = u32::from(cpu.gpr[3] <= 0x7f);
            Some(PpcImportAction::Return(result))
        }
        PpcImportDispatcherTarget::StdIscntrl => {
            let byte = cpu.gpr[3] as u8;
            let result = ppc_ctype_entry(byte) & PPC_CTYPE_CTL;
            Some(PpcImportAction::Return(u32::from(result)))
        }
        PpcImportDispatcherTarget::StdIsdigit => {
            let byte = cpu.gpr[3] as u8;
            let result = ppc_ctype_entry(byte) & PPC_CTYPE_DIG;
            Some(PpcImportAction::Return(u32::from(result)))
        }
        PpcImportDispatcherTarget::StdIsgraph => {
            let byte = cpu.gpr[3] as u8;
            let result = ppc_ctype_entry(byte)
                & (PPC_CTYPE_UPP | PPC_CTYPE_LOW | PPC_CTYPE_DIG | PPC_CTYPE_PUN);
            Some(PpcImportAction::Return(u32::from(result)))
        }
        PpcImportDispatcherTarget::StdIslower => {
            let byte = cpu.gpr[3] as u8;
            let result = ppc_ctype_entry(byte) & PPC_CTYPE_LOW;
            Some(PpcImportAction::Return(u32::from(result)))
        }
        PpcImportDispatcherTarget::StdIsprint => {
            let byte = cpu.gpr[3] as u8;
            let result = ppc_ctype_entry(byte)
                & (PPC_CTYPE_UPP | PPC_CTYPE_LOW | PPC_CTYPE_DIG | PPC_CTYPE_PUN | PPC_CTYPE_BLA);
            Some(PpcImportAction::Return(u32::from(result)))
        }
        PpcImportDispatcherTarget::StdIspunct => {
            let byte = cpu.gpr[3] as u8;
            let result = ppc_ctype_entry(byte) & PPC_CTYPE_PUN;
            Some(PpcImportAction::Return(u32::from(result)))
        }
        PpcImportDispatcherTarget::StdIsspace => {
            let byte = cpu.gpr[3] as u8;
            let result = ppc_ctype_entry(byte) & PPC_CTYPE_WSP;
            Some(PpcImportAction::Return(u32::from(result)))
        }
        PpcImportDispatcherTarget::StdIsupper => {
            let byte = cpu.gpr[3] as u8;
            let result = ppc_ctype_entry(byte) & PPC_CTYPE_UPP;
            Some(PpcImportAction::Return(u32::from(result)))
        }
        PpcImportDispatcherTarget::StdIsxdigit => {
            let byte = cpu.gpr[3] as u8;
            let result = ppc_ctype_entry(byte) & PPC_CTYPE_HEX;
            Some(PpcImportAction::Return(u32::from(result)))
        }
        PpcImportDispatcherTarget::StdToascii => {
            let byte = cpu.gpr[3] as u8;
            let result = byte & 0x7f;
            Some(PpcImportAction::Return(u32::from(result)))
        }
        PpcImportDispatcherTarget::StdSrand => {
            let seed = if cpu.gpr[3] == 0 { 1 } else { cpu.gpr[3] };
            let _ = memory.write_u32_be(PPC_RAND_SEED_ADDR, seed);
            Some(PpcImportAction::ReturnPreserve)
        }
        PpcImportDispatcherTarget::StdRand => Some(PpcImportAction::Return(u32::from(
            ppc_random(memory) & 0x7fff,
        ))),
        PpcImportDispatcherTarget::P2CStr => {
            ppc_p2cstr(cpu, memory);
            Some(PpcImportAction::Return(cpu.gpr[3]))
        }
        PpcImportDispatcherTarget::C2PStr => {
            ppc_c2pstr(cpu, memory);
            Some(PpcImportAction::Return(cpu.gpr[3]))
        }
        PpcImportDispatcherTarget::UpperText => {
            // UpperText
            // Converts a byte range to localized uppercase in place.
            // PROCEDURE UpperText (textPtr: Ptr; len: Integer);
            // Inside Macintosh Volume VI (1991), 14-63
            let text_ptr = cpu.gpr[3];
            let length = u32::from(cpu.gpr[4] as u16);
            if ppc_memory_can_write_bytes(memory, text_ptr, length) {
                for offset in 0..length {
                    if let Some(byte) = memory.read_u8(text_ptr + offset) {
                        let _ = memory.write_u8(
                            text_ptr + offset,
                            crate::trap::mac_roman_to_upper(byte, false),
                        );
                    }
                }
            }
            Some(PpcImportAction::ReturnPreserve)
        }
        PpcImportDispatcherTarget::StdCCompatibility(operation) => {
            Some(ppc_dispatch_stdc_compatibility(
                operation,
                cpu,
                memory,
                stdc_qsort_stack,
                stdc_signal_state,
            ))
        }
        _ => None,
    }
}

pub(super) fn ppc_dispatch_stdc_signal(
    cpu: &PpcCpu,
    signal_state: &mut PpcStdSignalState,
) -> PpcImportAction {
    // ISO/IEC 9899:1990, 7.3.3.1: signal installs a handler and returns the
    // handler previously associated with the signal. SIG_DFL is represented
    // by a null function pointer and SIG_IGN by the conventional pointer
    // value one. No asynchronous host signal is delivered to guest code, but
    // retaining the guest-visible handler makes install/reset sequences obey
    // the C interface without invoking arbitrary native addresses.
    const SIG_ERR: u32 = u32::MAX;
    const SIG_MIN: i32 = 1;
    const SIG_MAX: i32 = 32;
    let signal = cpu.gpr[3] as i32;
    let handler = cpu.gpr[4];
    if !(SIG_MIN..=SIG_MAX).contains(&signal) || handler == SIG_ERR {
        return PpcImportAction::Return(SIG_ERR);
    }

    let slot = usize::try_from(signal - SIG_MIN).unwrap();
    let previous = signal_state.handlers[slot];
    signal_state.handlers[slot] = handler;
    PpcImportAction::Return(previous)
}

pub(super) fn ppc_dispatch_stdc_compatibility(
    operation: PpcStdCCompatibilityOperation,
    cpu: &mut PpcCpu,
    memory: &mut PpcSectionMem,
    qsort_stack: &mut Vec<PpcQsortState>,
    signal_state: &mut PpcStdSignalState,
) -> PpcImportAction {
    match operation {
        PpcStdCCompatibilityOperation::Signal => ppc_dispatch_stdc_signal(cpu, signal_state),
        PpcStdCCompatibilityOperation::Sscanf => {
            PpcImportAction::Return(ppc_dispatch_stdc_sscanf(cpu, memory))
        }
        PpcStdCCompatibilityOperation::Strftime => {
            PpcImportAction::Return(ppc_dispatch_stdc_strftime(cpu, memory))
        }
        PpcStdCCompatibilityOperation::Qsort => ppc_dispatch_stdc_qsort(cpu, memory, qsort_stack),
        PpcStdCCompatibilityOperation::Vsprintf => {
            PpcImportAction::Return(ppc_std_vsprintf(cpu, memory))
        }
    }
}

pub(super) fn ppc_sscanf_store_integer(
    memory: &mut PpcSectionMem,
    destination: u32,
    value: u64,
    length: PpcPrintfLength,
) -> bool {
    match length {
        PpcPrintfLength::Char => memory.write_u8(destination, value as u8).is_some(),
        PpcPrintfLength::Short => memory.write_u16_be(destination, value as u16).is_some(),
        PpcPrintfLength::LongLong | PpcPrintfLength::LongDouble => {
            memory
                .write_u32_be(destination, (value >> 32) as u32)
                .is_some()
                && memory
                    .write_u32_be(destination.saturating_add(4), value as u32)
                    .is_some()
        }
        PpcPrintfLength::Default | PpcPrintfLength::Long => {
            memory.write_u32_be(destination, value as u32).is_some()
        }
    }
}

pub(super) fn ppc_sscanf_integer(
    input: &[u8],
    start: usize,
    width: usize,
    specifier: u8,
) -> Option<(u64, usize)> {
    let limit = input.len().min(start.saturating_add(width));
    let mut cursor = start;
    let negative = match input.get(cursor) {
        Some(b'-') => {
            cursor += 1;
            true
        }
        Some(b'+') => {
            cursor += 1;
            false
        }
        _ => false,
    };
    let mut radix = match specifier {
        b'o' => 8,
        b'x' | b'X' | b'p' => 16,
        _ => 10,
    };
    if specifier == b'i' {
        if cursor + 2 <= limit
            && input.get(cursor) == Some(&b'0')
            && matches!(input.get(cursor + 1), Some(b'x' | b'X'))
        {
            radix = 16;
            cursor += 2;
        } else if input.get(cursor) == Some(&b'0') {
            radix = 8;
        }
    } else if radix == 16
        && cursor + 2 <= limit
        && input.get(cursor) == Some(&b'0')
        && matches!(input.get(cursor + 1), Some(b'x' | b'X'))
    {
        cursor += 2;
    }
    let digit_start = cursor;
    let mut value = 0u64;
    while cursor < limit {
        let Some(digit) = (input[cursor] as char).to_digit(radix) else {
            break;
        };
        value = value
            .saturating_mul(u64::from(radix))
            .saturating_add(u64::from(digit));
        cursor += 1;
    }
    if cursor == digit_start {
        return None;
    }
    if negative {
        value = 0u64.wrapping_sub(value);
    }
    Some((value, cursor))
}

pub(super) fn ppc_sscanf_length(format: &[u8], cursor: &mut usize) -> PpcPrintfLength {
    match format.get(*cursor).copied() {
        Some(b'h') if format.get(*cursor + 1) == Some(&b'h') => {
            *cursor += 2;
            PpcPrintfLength::Char
        }
        Some(b'h') => {
            *cursor += 1;
            PpcPrintfLength::Short
        }
        Some(b'l') if format.get(*cursor + 1) == Some(&b'l') => {
            *cursor += 2;
            PpcPrintfLength::LongLong
        }
        Some(b'l') => {
            *cursor += 1;
            PpcPrintfLength::Long
        }
        Some(b'L') => {
            *cursor += 1;
            PpcPrintfLength::LongDouble
        }
        Some(b'j') => {
            *cursor += 1;
            PpcPrintfLength::LongLong
        }
        Some(b'z' | b't') => {
            *cursor += 1;
            PpcPrintfLength::Long
        }
        _ => PpcPrintfLength::Default,
    }
}

pub(super) fn ppc_sscanf_scan_set(
    format: &[u8],
    cursor: &mut usize,
) -> Option<([bool; 256], bool)> {
    let inverted = format.get(*cursor) == Some(&b'^');
    if inverted {
        *cursor += 1;
    }
    let mut set = [false; 256];
    let mut previous = None;
    if format.get(*cursor) == Some(&b']') {
        set[usize::from(b']')] = true;
        previous = Some(b']');
        *cursor += 1;
    }
    while let Some(byte) = format.get(*cursor).copied() {
        *cursor += 1;
        if byte == b']' {
            return Some((set, inverted));
        }
        if byte == b'-' {
            if let (Some(start), Some(end)) = (previous, format.get(*cursor).copied()) {
                if end != b']' {
                    *cursor += 1;
                    for value in start.min(end)..=start.max(end) {
                        set[usize::from(value)] = true;
                    }
                    previous = Some(end);
                    continue;
                }
            }
        }
        set[usize::from(byte)] = true;
        previous = Some(byte);
    }
    None
}

pub(super) fn ppc_dispatch_stdc_sscanf(cpu: &PpcCpu, memory: &mut PpcSectionMem) -> u32 {
    let input = ppc_std_c_string(memory, cpu.gpr[3], 65_535);
    let format = ppc_std_c_string(memory, cpu.gpr[4], 4096);
    let mut input_cursor = 0usize;
    let mut format_cursor = 0usize;
    let mut argument = 0usize;
    let mut assignments = 0u32;
    let mut input_failure = false;
    while format_cursor < format.len() {
        if format[format_cursor].is_ascii_whitespace() {
            while format
                .get(format_cursor)
                .is_some_and(u8::is_ascii_whitespace)
            {
                format_cursor += 1;
            }
            while input.get(input_cursor).is_some_and(u8::is_ascii_whitespace) {
                input_cursor += 1;
            }
            continue;
        }
        if format[format_cursor] != b'%' {
            if input.get(input_cursor) != format.get(format_cursor) {
                input_failure = input_cursor >= input.len();
                break;
            }
            input_cursor += 1;
            format_cursor += 1;
            continue;
        }
        format_cursor += 1;
        if format.get(format_cursor) == Some(&b'%') {
            if input.get(input_cursor) != Some(&b'%') {
                input_failure = input_cursor >= input.len();
                break;
            }
            input_cursor += 1;
            format_cursor += 1;
            continue;
        }
        let suppress = format.get(format_cursor) == Some(&b'*');
        if suppress {
            format_cursor += 1;
        }
        let mut width = 0usize;
        while let Some(digit @ b'0'..=b'9') = format.get(format_cursor).copied() {
            width = width
                .saturating_mul(10)
                .saturating_add(usize::from(digit - b'0'));
            format_cursor += 1;
        }
        if width == 0 {
            width = usize::MAX;
        }
        let length = ppc_sscanf_length(&format, &mut format_cursor);
        let Some(specifier) = format.get(format_cursor).copied() else {
            break;
        };
        format_cursor += 1;
        let scan_set = if specifier == b'[' {
            ppc_sscanf_scan_set(&format, &mut format_cursor)
        } else {
            None
        };
        if specifier != b'c' && specifier != b'[' && specifier != b'n' {
            while input.get(input_cursor).is_some_and(u8::is_ascii_whitespace) {
                input_cursor += 1;
            }
        }
        let destination = if suppress || specifier == b'%' {
            0
        } else {
            let destination = ppc_sprintf_argument(cpu, memory, argument);
            argument += 1;
            destination
        };
        let mut counted_assignment = !suppress && specifier != b'n';
        let success = match specifier {
            b'd' | b'i' | b'u' | b'o' | b'x' | b'X' | b'p' => {
                if let Some((value, end)) =
                    ppc_sscanf_integer(&input, input_cursor, width, specifier)
                {
                    input_cursor = end;
                    suppress || ppc_sscanf_store_integer(memory, destination, value, length)
                } else {
                    false
                }
            }
            b'f' | b'F' | b'e' | b'E' | b'g' | b'G' | b'a' | b'A' => {
                let end = input.len().min(input_cursor.saturating_add(width));
                let token_len = input[input_cursor..end]
                    .iter()
                    .take_while(|byte| {
                        byte.is_ascii_digit()
                            || matches!(
                                **byte,
                                b'+' | b'-' | b'.' | b'e' | b'E' | b'p' | b'P' | b'x' | b'X'
                            )
                    })
                    .count();
                let parsed = std::str::from_utf8(&input[input_cursor..input_cursor + token_len])
                    .ok()
                    .and_then(|text| text.parse::<f64>().ok());
                if let Some(value) = parsed {
                    input_cursor += token_len;
                    suppress
                        || if matches!(length, PpcPrintfLength::Long | PpcPrintfLength::LongDouble)
                        {
                            let bits = value.to_bits();
                            memory
                                .write_u32_be(destination, (bits >> 32) as u32)
                                .is_some()
                                && memory
                                    .write_u32_be(destination.saturating_add(4), bits as u32)
                                    .is_some()
                        } else {
                            memory
                                .write_u32_be(destination, (value as f32).to_bits())
                                .is_some()
                        }
                } else {
                    false
                }
            }
            b's' => {
                let end = input.len().min(input_cursor.saturating_add(width));
                let length = input[input_cursor..end]
                    .iter()
                    .take_while(|byte| !byte.is_ascii_whitespace())
                    .count();
                if length == 0 {
                    false
                } else {
                    let bytes = &input[input_cursor..input_cursor + length];
                    input_cursor += length;
                    suppress
                        || (memory.write_bytes(destination, bytes).is_some()
                            && memory
                                .write_u8(destination.saturating_add(length as u32), 0)
                                .is_some())
                }
            }
            b'c' => {
                let length = width.min(input.len().saturating_sub(input_cursor));
                if length == 0 || (width != usize::MAX && length != width) {
                    false
                } else {
                    let length = if width == usize::MAX { 1 } else { length };
                    let bytes = &input[input_cursor..input_cursor + length];
                    input_cursor += length;
                    suppress || memory.write_bytes(destination, bytes).is_some()
                }
            }
            b'[' => {
                let Some((set, inverted)) = scan_set else {
                    break;
                };
                let end = input.len().min(input_cursor.saturating_add(width));
                let length = input[input_cursor..end]
                    .iter()
                    .take_while(|byte| set[usize::from(**byte)] != inverted)
                    .count();
                if length == 0 {
                    false
                } else {
                    let bytes = &input[input_cursor..input_cursor + length];
                    input_cursor += length;
                    suppress
                        || (memory.write_bytes(destination, bytes).is_some()
                            && memory
                                .write_u8(destination.saturating_add(length as u32), 0)
                                .is_some())
                }
            }
            b'n' => {
                counted_assignment = false;
                suppress
                    || ppc_sscanf_store_integer(memory, destination, input_cursor as u64, length)
            }
            _ => false,
        };
        if !success {
            input_failure = input_cursor >= input.len();
            break;
        }
        if counted_assignment {
            assignments = assignments.saturating_add(1);
        }
    }
    if assignments == 0 && input_failure {
        u32::MAX
    } else {
        assignments
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct PpcCTm {
    second: i32,
    minute: i32,
    hour: i32,
    month_day: i32,
    month: i32,
    year: i32,
    week_day: i32,
    year_day: i32,
}

pub(super) fn ppc_read_c_tm(memory: &mut PpcSectionMem, address: u32) -> Option<PpcCTm> {
    let field = |memory: &mut PpcSectionMem, offset: u32| {
        memory
            .read_u32_be(address.checked_add(offset)?)
            .map(|value| value as i32)
    };
    Some(PpcCTm {
        second: field(memory, 0)?,
        minute: field(memory, 4)?,
        hour: field(memory, 8)?,
        month_day: field(memory, 12)?,
        month: field(memory, 16)?,
        year: field(memory, 20)?,
        week_day: field(memory, 24)?,
        year_day: field(memory, 28)?,
    })
}

pub(super) fn ppc_c_tm_is_leap_year(year: i32) -> bool {
    year.rem_euclid(4) == 0 && (year.rem_euclid(100) != 0 || year.rem_euclid(400) == 0)
}

pub(super) fn ppc_c_tm_iso_weeks_in_year(year: i32, january_first_weekday: i32) -> i32 {
    let january_first_iso = if january_first_weekday == 0 {
        7
    } else {
        january_first_weekday
    };
    if january_first_iso == 4 || (january_first_iso == 3 && ppc_c_tm_is_leap_year(year)) {
        53
    } else {
        52
    }
}

pub(super) fn ppc_c_tm_iso_year_and_week(tm: PpcCTm) -> (i32, i32) {
    let year = tm.year.saturating_add(1900);
    let week_day = tm.week_day.rem_euclid(7);
    let iso_week_day = if week_day == 0 { 7 } else { week_day };
    let january_first = (week_day - tm.year_day.rem_euclid(7)).rem_euclid(7);
    let mut week = (tm.year_day + 10 - iso_week_day) / 7;
    if week < 1 {
        let previous_year = year - 1;
        let previous_january_first = (january_first
            - if ppc_c_tm_is_leap_year(previous_year) {
                2
            } else {
                1
            })
        .rem_euclid(7);
        return (
            previous_year,
            ppc_c_tm_iso_weeks_in_year(previous_year, previous_january_first),
        );
    }
    let weeks_in_year = ppc_c_tm_iso_weeks_in_year(year, january_first);
    if week > weeks_in_year {
        return (year + 1, 1);
    }
    week = week.clamp(1, 53);
    (year, week)
}

pub(super) fn ppc_dispatch_stdc_strftime(cpu: &PpcCpu, memory: &mut PpcSectionMem) -> u32 {
    let destination = cpu.gpr[3];
    let maximum = cpu.gpr[4];
    let format = ppc_std_c_string(memory, cpu.gpr[5], 4096);
    let Some(tm) = ppc_read_c_tm(memory, cpu.gpr[6]) else {
        if destination != 0 && maximum != 0 {
            let _ = memory.write_u8(destination, 0);
        }
        return 0;
    };
    let abbreviated_weekdays = [b"Sun", b"Mon", b"Tue", b"Wed", b"Thu", b"Fri", b"Sat"];
    let weekdays: [&[u8]; 7] = [
        b"Sunday",
        b"Monday",
        b"Tuesday",
        b"Wednesday",
        b"Thursday",
        b"Friday",
        b"Saturday",
    ];
    let abbreviated_months = [
        b"Jan", b"Feb", b"Mar", b"Apr", b"May", b"Jun", b"Jul", b"Aug", b"Sep", b"Oct", b"Nov",
        b"Dec",
    ];
    let months: [&[u8]; 12] = [
        b"January",
        b"February",
        b"March",
        b"April",
        b"May",
        b"June",
        b"July",
        b"August",
        b"September",
        b"October",
        b"November",
        b"December",
    ];
    let year = tm.year.saturating_add(1900);
    let week_day = tm.week_day.rem_euclid(7);
    let month = tm.month.clamp(0, 11) as usize;
    let hour_12 = match tm.hour.rem_euclid(24) % 12 {
        0 => 12,
        hour => hour,
    };
    let week_sunday = (tm.year_day + 7 - week_day).div_euclid(7);
    let week_monday = (tm.year_day + 7 - (week_day + 6).rem_euclid(7)).div_euclid(7);
    let (iso_year, iso_week) = ppc_c_tm_iso_year_and_week(tm);
    let mut output = Vec::new();
    let mut cursor = 0usize;
    while cursor < format.len() {
        if format[cursor] != b'%' {
            output.push(format[cursor]);
            cursor += 1;
            continue;
        }
        cursor += 1;
        if matches!(format.get(cursor), Some(b'E' | b'O')) {
            cursor += 1;
        }
        let Some(specifier) = format.get(cursor).copied() else {
            break;
        };
        cursor += 1;
        let field = match specifier {
            b'a' => abbreviated_weekdays[week_day as usize].to_vec(),
            b'A' => weekdays[week_day as usize].to_vec(),
            b'b' | b'h' => abbreviated_months[month].to_vec(),
            b'B' => months[month].to_vec(),
            b'c' => format!(
                "{} {} {:2} {:02}:{:02}:{:02} {year:04}",
                String::from_utf8_lossy(abbreviated_weekdays[week_day as usize]),
                String::from_utf8_lossy(abbreviated_months[month]),
                tm.month_day,
                tm.hour,
                tm.minute,
                tm.second
            )
            .into_bytes(),
            b'C' => format!("{:02}", year.div_euclid(100).rem_euclid(100)).into_bytes(),
            b'd' => format!("{:02}", tm.month_day).into_bytes(),
            b'D' | b'x' => format!(
                "{:02}/{:02}/{:02}",
                tm.month + 1,
                tm.month_day,
                year.rem_euclid(100)
            )
            .into_bytes(),
            b'e' => format!("{:2}", tm.month_day).into_bytes(),
            b'F' => format!("{year:04}-{:02}-{:02}", tm.month + 1, tm.month_day).into_bytes(),
            b'g' => format!("{:02}", iso_year.rem_euclid(100)).into_bytes(),
            b'G' => format!("{iso_year:04}").into_bytes(),
            b'H' => format!("{:02}", tm.hour).into_bytes(),
            b'I' => format!("{hour_12:02}").into_bytes(),
            b'j' => format!("{:03}", tm.year_day + 1).into_bytes(),
            b'm' => format!("{:02}", tm.month + 1).into_bytes(),
            b'M' => format!("{:02}", tm.minute).into_bytes(),
            b'n' => vec![b'\n'],
            b'p' => {
                if tm.hour.rem_euclid(24) < 12 {
                    b"AM".to_vec()
                } else {
                    b"PM".to_vec()
                }
            }
            b'r' => format!(
                "{hour_12:02}:{:02}:{:02} {}",
                tm.minute,
                tm.second,
                if tm.hour.rem_euclid(24) < 12 {
                    "AM"
                } else {
                    "PM"
                }
            )
            .into_bytes(),
            b'R' => format!("{:02}:{:02}", tm.hour, tm.minute).into_bytes(),
            b'S' => format!("{:02}", tm.second).into_bytes(),
            b't' => vec![b'\t'],
            b'T' | b'X' => format!("{:02}:{:02}:{:02}", tm.hour, tm.minute, tm.second).into_bytes(),
            b'u' => (if week_day == 0 { 7 } else { week_day })
                .to_string()
                .into_bytes(),
            b'U' => format!("{week_sunday:02}").into_bytes(),
            b'V' => format!("{iso_week:02}").into_bytes(),
            b'w' => week_day.to_string().into_bytes(),
            b'W' => format!("{week_monday:02}").into_bytes(),
            b'y' => format!("{:02}", year.rem_euclid(100)).into_bytes(),
            b'Y' => format!("{year:04}").into_bytes(),
            b'z' | b'Z' => Vec::new(),
            b'%' => vec![b'%'],
            other => vec![b'%', other],
        };
        output.extend(field);
        if output.len() > 1_048_576 {
            break;
        }
    }

    let Ok(byte_count) = u32::try_from(output.len()) else {
        return 0;
    };
    let success = byte_count < maximum
        && byte_count
            .checked_add(1)
            .is_some_and(|size| ppc_memory_can_write_bytes(memory, destination, size));
    if !success {
        if destination != 0 && maximum != 0 {
            let _ = memory.write_u8(destination, 0);
        }
        return 0;
    }
    let _ = memory.write_bytes(destination, &output);
    let _ = memory.write_u8(destination + byte_count, 0);
    byte_count
}

pub(super) fn ppc_qsort_compare_next(
    cpu: &mut PpcCpu,
    memory: &mut PpcSectionMem,
    state: PpcQsortState,
) -> Option<PpcImportAction> {
    let left = state
        .base
        .checked_add(state.index.checked_mul(state.width)?)?;
    let right = left.checked_add(state.width)?;
    install_powerpc_call_arguments(cpu, memory, &[left, right])?;
    GuestCallEffect::call_guest(
        GuestCallRequest::new(GuestCallTarget {
            isa: GuestIsa::PowerPc,
            entry: state.comparator.entry,
            rtoc: state.comparator.rtoc,
        }),
        GuestCallContinuation::to_powerpc(
            PPC_GUEST_CALL_RETURN_PC,
            cpu.pc,
            state.restore_rtoc,
            PpcNativeReturnGpr3::Preserve,
        ),
    )
    .into_ppc_import_action()
}

pub(super) fn ppc_dispatch_stdc_qsort(
    cpu: &mut PpcCpu,
    memory: &mut PpcSectionMem,
    states: &mut Vec<PpcQsortState>,
) -> PpcImportAction {
    // The PowerPC run-time convention saves GPR3..GPR10 into the caller's
    // parameter area for variable-argument calls and passes ordinary callback
    // arguments in GPR3 onward. See Inside Macintosh: PowerPC System Software
    // (1994), pp. 1-47--1-49. qsort's comparator is an ordinary two-argument
    // native callback, so route it through the same Mixed Mode-aware callback
    // path used by Toolbox UPPs instead of substituting host comparison logic.
    if cpu.lr == cpu.pc {
        let Some(mut state) = states.pop() else {
            return PpcImportAction::ReturnPreserve;
        };
        if cpu.gpr[3] as i32 > 0 {
            let Some(left) = state
                .base
                .checked_add(state.index.saturating_mul(state.width))
            else {
                cpu.lr = state.final_pc;
                return PpcImportAction::ReturnPreserve;
            };
            let Some(right) = left.checked_add(state.width) else {
                cpu.lr = state.final_pc;
                return PpcImportAction::ReturnPreserve;
            };
            let Some(left_bytes) = ppc_memory_read_bytes(memory, left, state.width) else {
                cpu.lr = state.final_pc;
                return PpcImportAction::ReturnPreserve;
            };
            let Some(right_bytes) = ppc_memory_read_bytes(memory, right, state.width) else {
                cpu.lr = state.final_pc;
                return PpcImportAction::ReturnPreserve;
            };
            if memory.write_bytes(left, &right_bytes).is_none()
                || memory.write_bytes(right, &left_bytes).is_none()
            {
                cpu.lr = state.final_pc;
                return PpcImportAction::ReturnPreserve;
            }
            state.swapped = true;
        }

        state.index = state.index.saturating_add(1);
        if state.index >= state.pass_end {
            if !state.swapped || state.pass_end <= 1 {
                cpu.lr = state.final_pc;
                cpu.gpr[2] = state.restore_rtoc;
                return PpcImportAction::ReturnPreserve;
            }
            state.pass_end -= 1;
            state.index = 0;
            state.swapped = false;
        }
        states.push(state);
        return ppc_qsort_compare_next(cpu, memory, state).unwrap_or_else(|| {
            let state = states.pop().unwrap();
            cpu.lr = state.final_pc;
            PpcImportAction::ReturnPreserve
        });
    }

    let (base, count, width, comparator_ptr) = (cpu.gpr[3], cpu.gpr[4], cpu.gpr[5], cpu.gpr[6]);
    if count < 2 {
        return PpcImportAction::ReturnPreserve;
    }
    let Some(byte_count) = count.checked_mul(width) else {
        return PpcImportAction::ReturnPreserve;
    };
    if width == 0
        || !ppc_memory_can_read_bytes(memory, base, byte_count)
        || !ppc_memory_can_write_bytes(memory, base, byte_count)
    {
        return PpcImportAction::ReturnPreserve;
    }
    let Some(comparator) = ppc_resolve_callback_target(memory, comparator_ptr, cpu.gpr[2], None)
    else {
        return PpcImportAction::ReturnPreserve;
    };
    if memory.read_u32_be(comparator.entry).is_none() {
        return PpcImportAction::ReturnPreserve;
    }
    let state = PpcQsortState {
        base,
        width,
        comparator,
        final_pc: cpu.lr,
        restore_rtoc: cpu.gpr[2],
        pass_end: count - 1,
        index: 0,
        swapped: false,
    };
    states.push(state);
    ppc_qsort_compare_next(cpu, memory, state).unwrap_or_else(|| {
        states.pop();
        PpcImportAction::ReturnPreserve
    })
}

pub(super) fn ppc_p2cstr(cpu: &PpcCpu, memory: &mut PpcSectionMem) {
    let ptr = cpu.gpr[3];
    let Some(len) = memory.read_u8(ptr) else {
        return;
    };
    let mut bytes = Vec::new();
    for offset in 0..u32::from(len) {
        let byte = memory.read_u8(ptr + 1 + offset).unwrap_or(0);
        bytes.push(byte);
        let _ = memory.write_u8(ptr + offset, byte);
    }
    let _ = memory.write_u8(ptr + u32::from(len), 0);
    if ppc_hle_trace_enabled() {
        eprintln!(
            "[PPC-TRACE] p2cstr ptr=${:08X} lr=${:08X} len={} text=\"{}\"",
            ptr,
            cpu.lr,
            len,
            decode_mac_roman(&bytes)
        );
    }
}

pub(super) fn ppc_c2pstr(cpu: &PpcCpu, memory: &mut PpcSectionMem) {
    let ptr = cpu.gpr[3];
    let mut len = 0u8;
    while len < u8::MAX {
        match memory.read_u8(ptr + u32::from(len)) {
            Some(0) => break,
            Some(_) => len = len.saturating_add(1),
            None => return,
        }
    }
    let mut bytes = Vec::new();
    for offset in (0..u32::from(len)).rev() {
        let byte = memory.read_u8(ptr + offset).unwrap_or(0);
        bytes.push(byte);
        let _ = memory.write_u8(ptr + 1 + offset, byte);
    }
    let _ = memory.write_u8(ptr, len);
    bytes.reverse();
    if ppc_hle_trace_enabled() {
        eprintln!(
            "[PPC-TRACE] c2pstr ptr=${:08X} lr=${:08X} len={} text=\"{}\"",
            ptr,
            cpu.lr,
            len,
            decode_mac_roman(&bytes)
        );
    }
}

pub(super) fn ppc_std_memcmp(
    memory: &mut PpcSectionMem,
    first: u32,
    second: u32,
    count: u32,
) -> i32 {
    for offset in 0..count {
        let Some(first_addr) = first.checked_add(offset) else {
            break;
        };
        let Some(second_addr) = second.checked_add(offset) else {
            break;
        };
        let (Some(a), Some(b)) = (memory.read_u8(first_addr), memory.read_u8(second_addr)) else {
            break;
        };
        if a != b {
            return i32::from(a) - i32::from(b);
        }
    }
    0
}

pub(super) fn ppc_std_memmove(
    memory: &mut PpcSectionMem,
    destination: u32,
    source: u32,
    count: u32,
) {
    let Some(bytes) = ppc_memory_read_bytes(memory, source, count) else {
        return;
    };
    if ppc_memory_can_write_bytes(memory, destination, count) {
        let _ = memory.write_bytes(destination, &bytes);
    }
}

pub(super) fn ppc_std_strlen(memory: &mut PpcSectionMem, string: u32) -> u32 {
    let mut length = 0u32;
    while let Some(addr) = string.checked_add(length) {
        match memory.read_u8(addr) {
            Some(0) | None => break,
            Some(_) => length = length.saturating_add(1),
        }
    }
    length
}

pub(super) fn ppc_std_memchr(
    memory: &mut PpcSectionMem,
    bytes: u32,
    needle: u8,
    count: u32,
) -> u32 {
    for offset in 0..count {
        let Some(address) = bytes.checked_add(offset) else {
            return 0;
        };
        let Some(value) = memory.read_u8(address) else {
            return 0;
        };
        if value == needle {
            return address;
        }
    }
    0
}

pub(super) fn ppc_std_strchr(
    memory: &mut PpcSectionMem,
    string: u32,
    needle: u8,
    reverse: bool,
) -> u32 {
    let mut offset = 0u32;
    let mut last = 0u32;
    loop {
        let Some(address) = string.checked_add(offset) else {
            return 0;
        };
        let Some(value) = memory.read_u8(address) else {
            return 0;
        };
        if value == needle {
            if !reverse {
                return address;
            }
            last = address;
        }
        if value == 0 {
            return last;
        }
        let Some(next) = offset.checked_add(1) else {
            return 0;
        };
        offset = next;
    }
}

pub(super) fn ppc_std_c_string_contains(
    memory: &mut PpcSectionMem,
    string: u32,
    needle: u8,
) -> bool {
    let mut offset = 0u32;
    loop {
        let Some(address) = string.checked_add(offset) else {
            return false;
        };
        match memory.read_u8(address) {
            Some(0) | None => return false,
            Some(value) if value == needle => return true,
            Some(_) => {
                let Some(next) = offset.checked_add(1) else {
                    return false;
                };
                offset = next;
            }
        }
    }
}

pub(super) fn ppc_std_strspn(
    memory: &mut PpcSectionMem,
    string: u32,
    set: u32,
    accept: bool,
) -> u32 {
    let mut length = 0u32;
    loop {
        let Some(address) = string.checked_add(length) else {
            return length;
        };
        let Some(value) = memory.read_u8(address) else {
            return length;
        };
        if value == 0 || ppc_std_c_string_contains(memory, set, value) != accept {
            return length;
        }
        let Some(next) = length.checked_add(1) else {
            return length;
        };
        length = next;
    }
}

pub(super) fn ppc_std_strpbrk(memory: &mut PpcSectionMem, string: u32, set: u32) -> u32 {
    let mut offset = 0u32;
    loop {
        let Some(address) = string.checked_add(offset) else {
            return 0;
        };
        let Some(value) = memory.read_u8(address) else {
            return 0;
        };
        if value == 0 {
            return 0;
        }
        if ppc_std_c_string_contains(memory, set, value) {
            return address;
        }
        let Some(next) = offset.checked_add(1) else {
            return 0;
        };
        offset = next;
    }
}

pub(super) fn ppc_std_strstr(memory: &mut PpcSectionMem, haystack: u32, needle: u32) -> u32 {
    match memory.read_u8(needle) {
        Some(0) => return haystack,
        Some(_) => {}
        None => return 0,
    }
    let mut candidate_offset = 0u32;
    loop {
        let Some(candidate) = haystack.checked_add(candidate_offset) else {
            return 0;
        };
        match memory.read_u8(candidate) {
            Some(0) | None => return 0,
            Some(_) => {}
        }
        let mut match_offset = 0u32;
        loop {
            let Some(needle_address) = needle.checked_add(match_offset) else {
                return 0;
            };
            let Some(needle_value) = memory.read_u8(needle_address) else {
                return 0;
            };
            if needle_value == 0 {
                return candidate;
            }
            let Some(haystack_address) = candidate.checked_add(match_offset) else {
                return 0;
            };
            if memory.read_u8(haystack_address) != Some(needle_value) {
                break;
            }
            let Some(next) = match_offset.checked_add(1) else {
                return 0;
            };
            match_offset = next;
        }
        let Some(next) = candidate_offset.checked_add(1) else {
            return 0;
        };
        candidate_offset = next;
    }
}

pub(super) fn ppc_std_c_string(memory: &mut PpcSectionMem, string: u32, limit: usize) -> Vec<u8> {
    let mut bytes = Vec::new();
    for offset in 0..limit {
        let Some(address) = string.checked_add(offset as u32) else {
            break;
        };
        let Some(byte) = memory.read_u8(address) else {
            break;
        };
        if byte == 0 {
            break;
        }
        bytes.push(byte);
    }
    bytes
}

pub(super) fn ppc_sprintf_argument(cpu: &PpcCpu, memory: &mut PpcSectionMem, index: usize) -> u32 {
    if index < 6 {
        return cpu.gpr[5 + index];
    }
    cpu.gpr[1]
        .checked_add(32 + index as u32 * 4)
        .and_then(|address| memory.read_u32_be(address))
        .unwrap_or(0)
}

pub(super) fn ppc_sprintf_pad(mut field: Vec<u8>, width: usize, left: bool, zero: bool) -> Vec<u8> {
    if field.len() >= width {
        return field;
    }
    let padding = width - field.len();
    if left {
        field.extend(std::iter::repeat(b' ').take(padding));
        return field;
    }
    let pad = if zero { b'0' } else { b' ' };
    let prefix_len = if zero && matches!(field.first(), Some(b'+' | b'-' | b' ')) {
        1
    } else if zero && matches!(field.get(..2), Some(b"0x" | b"0X")) {
        2
    } else {
        0
    };
    if prefix_len != 0 {
        let prefix = field.drain(..prefix_len).collect::<Vec<_>>();
        let mut padded = Vec::with_capacity(width);
        padded.extend(prefix);
        padded.extend(std::iter::repeat(pad).take(padding));
        padded.extend(field);
        return padded;
    }
    let mut padded = Vec::with_capacity(width);
    padded.extend(std::iter::repeat(pad).take(padding));
    padded.extend(field);
    padded
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum PpcPrintfLength {
    Default,
    Char,
    Short,
    Long,
    LongLong,
    LongDouble,
}

pub(super) fn ppc_printf_unsigned_bytes(value: u64, radix: u32, uppercase: bool) -> Vec<u8> {
    match (radix, uppercase) {
        (8, _) => format!("{value:o}").into_bytes(),
        (16, false) => format!("{value:x}").into_bytes(),
        (16, true) => format!("{value:X}").into_bytes(),
        _ => value.to_string().into_bytes(),
    }
}

pub(super) fn ppc_printf_apply_numeric_precision(
    mut digits: Vec<u8>,
    value_is_zero: bool,
    precision: Option<usize>,
) -> Vec<u8> {
    let Some(precision) = precision else {
        return digits;
    };
    if precision == 0 && value_is_zero {
        return Vec::new();
    }
    if digits.len() < precision {
        let mut padded = Vec::with_capacity(precision);
        padded.extend(std::iter::repeat(b'0').take(precision - digits.len()));
        padded.append(&mut digits);
        return padded;
    }
    digits
}

pub(super) fn ppc_printf_general(value: f64, precision: usize, uppercase: bool) -> Vec<u8> {
    let precision = precision.max(1);
    let absolute = value.abs();
    let exponent = if absolute == 0.0 {
        0
    } else {
        absolute.log10().floor() as i32
    };
    let mut text = if exponent < -4 || exponent >= precision as i32 {
        format!("{:.*e}", precision.saturating_sub(1), value)
    } else {
        format!(
            "{:.*}",
            precision.saturating_sub(1 + exponent.max(0) as usize),
            value
        )
    };
    if let Some(exponent_index) = text.find('e') {
        let exponent = text.split_off(exponent_index);
        while text.ends_with('0') && text.contains('.') {
            text.pop();
        }
        if text.ends_with('.') {
            text.pop();
        }
        text.push_str(&exponent);
    } else {
        while text.ends_with('0') && text.contains('.') {
            text.pop();
        }
        if text.ends_with('.') {
            text.pop();
        }
    }
    if uppercase {
        text.make_ascii_uppercase();
    }
    text.into_bytes()
}

pub(super) fn ppc_std_format<F>(
    memory: &mut PpcSectionMem,
    destination: u32,
    format_ptr: u32,
    trace_name: &str,
    mut next_word: F,
) -> u32
where
    F: FnMut(&mut PpcSectionMem, usize) -> u32,
{
    let format = ppc_std_c_string(memory, format_ptr, 4096);
    let mut output = Vec::new();
    let mut cursor = 0usize;
    let mut argument = 0usize;
    while cursor < format.len() {
        if format[cursor] != b'%' {
            output.push(format[cursor]);
            cursor += 1;
            continue;
        }
        cursor += 1;
        if format.get(cursor) == Some(&b'%') {
            output.push(b'%');
            cursor += 1;
            continue;
        }
        let mut left = false;
        let mut zero = false;
        let mut plus = false;
        let mut space = false;
        let mut alternate = false;
        loop {
            match format.get(cursor).copied() {
                Some(b'-') => left = true,
                Some(b'0') => zero = true,
                Some(b'+') => plus = true,
                Some(b' ') => space = true,
                Some(b'#') => alternate = true,
                _ => break,
            }
            cursor += 1;
        }
        let mut width = 0usize;
        if format.get(cursor) == Some(&b'*') {
            let dynamic_width = next_word(memory, argument) as i32;
            argument += 1;
            if dynamic_width < 0 {
                left = true;
                width = dynamic_width.unsigned_abs() as usize;
            } else {
                width = dynamic_width as usize;
            }
            cursor += 1;
        } else {
            while let Some(digit @ b'0'..=b'9') = format.get(cursor).copied() {
                width = width
                    .saturating_mul(10)
                    .saturating_add(usize::from(digit - b'0'));
                cursor += 1;
            }
        }
        let mut precision = None;
        if format.get(cursor) == Some(&b'.') {
            cursor += 1;
            if format.get(cursor) == Some(&b'*') {
                let dynamic_precision = next_word(memory, argument) as i32;
                argument += 1;
                cursor += 1;
                if dynamic_precision >= 0 {
                    precision = Some(dynamic_precision as usize);
                }
            } else {
                let mut value = 0usize;
                while let Some(digit @ b'0'..=b'9') = format.get(cursor).copied() {
                    value = value
                        .saturating_mul(10)
                        .saturating_add(usize::from(digit - b'0'));
                    cursor += 1;
                }
                precision = Some(value);
            }
        }
        let length = match format.get(cursor).copied() {
            Some(b'h') if format.get(cursor + 1) == Some(&b'h') => {
                cursor += 2;
                PpcPrintfLength::Char
            }
            Some(b'h') => {
                cursor += 1;
                PpcPrintfLength::Short
            }
            Some(b'l') if format.get(cursor + 1) == Some(&b'l') => {
                cursor += 2;
                PpcPrintfLength::LongLong
            }
            Some(b'l') => {
                cursor += 1;
                PpcPrintfLength::Long
            }
            Some(b'L') => {
                cursor += 1;
                PpcPrintfLength::LongDouble
            }
            Some(b'j' | b'z' | b't') => {
                cursor += 1;
                PpcPrintfLength::Long
            }
            _ => PpcPrintfLength::Default,
        };
        let Some(specifier) = format.get(cursor).copied() else {
            break;
        };
        cursor += 1;
        let mut consumes_word = !matches!(specifier, b'%');
        let mut field = match specifier {
            b's' => {
                let value = next_word(memory, argument);
                let mut bytes = ppc_std_c_string(memory, value, 65_535);
                if let Some(precision) = precision {
                    bytes.truncate(precision);
                }
                bytes
            }
            b'P' => {
                let value = next_word(memory, argument);
                let length = memory.read_u8(value).unwrap_or(0) as u32;
                let mut bytes = ppc_memory_read_bytes(memory, value.saturating_add(1), length)
                    .unwrap_or_default();
                if let Some(precision) = precision {
                    bytes.truncate(precision);
                }
                bytes
            }
            b'c' => vec![next_word(memory, argument) as u8],
            b'd' | b'i' => {
                let raw = if length == PpcPrintfLength::LongLong {
                    let high = u64::from(next_word(memory, argument));
                    argument += 1;
                    (high << 32) | u64::from(next_word(memory, argument))
                } else {
                    u64::from(next_word(memory, argument))
                };
                let signed = match length {
                    PpcPrintfLength::Char => i64::from(raw as i8),
                    PpcPrintfLength::Short => i64::from(raw as i16),
                    PpcPrintfLength::LongLong => raw as i64,
                    _ => i64::from(raw as u32 as i32),
                };
                let mut text = ppc_printf_apply_numeric_precision(
                    signed.unsigned_abs().to_string().into_bytes(),
                    signed == 0,
                    precision,
                );
                if signed < 0 {
                    text.insert(0, b'-');
                } else {
                    if plus {
                        text.insert(0, b'+');
                    } else if space {
                        text.insert(0, b' ');
                    }
                }
                text
            }
            b'u' | b'x' | b'X' | b'o' => {
                let value = if length == PpcPrintfLength::LongLong {
                    let high = u64::from(next_word(memory, argument));
                    argument += 1;
                    (high << 32) | u64::from(next_word(memory, argument))
                } else {
                    let value = next_word(memory, argument);
                    match length {
                        PpcPrintfLength::Char => u64::from(value as u8),
                        PpcPrintfLength::Short => u64::from(value as u16),
                        _ => u64::from(value),
                    }
                };
                let radix = match specifier {
                    b'o' => 8,
                    b'x' | b'X' => 16,
                    _ => 10,
                };
                let mut text = ppc_printf_apply_numeric_precision(
                    ppc_printf_unsigned_bytes(value, radix, specifier == b'X'),
                    value == 0,
                    precision,
                );
                if alternate && value != 0 {
                    match specifier {
                        b'o' if text.first() != Some(&b'0') => text.insert(0, b'0'),
                        b'x' => text.splice(0..0, *b"0x").for_each(drop),
                        b'X' => text.splice(0..0, *b"0X").for_each(drop),
                        _ => {}
                    }
                }
                text
            }
            b'p' => format!("{:08x}", next_word(memory, argument)).into_bytes(),
            b'f' | b'F' | b'e' | b'E' | b'g' | b'G' => {
                let high = u64::from(next_word(memory, argument));
                argument += 1;
                let low = u64::from(next_word(memory, argument));
                let value = f64::from_bits((high << 32) | low);
                let precision = precision.unwrap_or(6).min(1024);
                let mut text = match specifier {
                    b'f' | b'F' => format!("{value:.precision$}").into_bytes(),
                    b'e' | b'E' => format!("{value:.precision$e}").into_bytes(),
                    _ => ppc_printf_general(value, precision, matches!(specifier, b'G')),
                };
                if matches!(specifier, b'F' | b'E') {
                    text.make_ascii_uppercase();
                }
                if value.is_sign_positive() {
                    if plus {
                        text.insert(0, b'+');
                    } else if space {
                        text.insert(0, b' ');
                    }
                }
                text
            }
            b'n' => {
                let destination = next_word(memory, argument);
                match length {
                    PpcPrintfLength::Char => {
                        let _ = memory.write_u8(destination, output.len() as u8);
                    }
                    PpcPrintfLength::Short => {
                        let _ = memory.write_u16_be(destination, output.len() as u16);
                    }
                    _ => {
                        let _ = memory.write_u32_be(destination, output.len() as u32);
                    }
                }
                Vec::new()
            }
            b'%' => {
                consumes_word = false;
                vec![b'%']
            }
            other => vec![b'%', other],
        };
        if consumes_word {
            argument += 1;
        }
        let numeric_precision_disables_zero =
            precision.is_some() && matches!(specifier, b'd' | b'i' | b'u' | b'x' | b'X' | b'o');
        field = ppc_sprintf_pad(
            field,
            width.min(1_048_576),
            left,
            zero && !left && !numeric_precision_disables_zero,
        );
        output.extend(field);
        if output.len() > 1_048_576 {
            output.truncate(1_048_576);
            break;
        }
    }
    let byte_count = u32::try_from(output.len()).unwrap_or(u32::MAX);
    if byte_count
        .checked_add(1)
        .is_some_and(|size| ppc_memory_can_write_bytes(memory, destination, size))
    {
        let _ = memory.write_bytes(destination, &output);
        let _ = memory.write_u8(destination + byte_count, 0);
    }
    if ppc_hle_trace_enabled() {
        eprintln!(
            "[PPC-TRACE] {trace_name} dst=${destination:08X} format={:?} -> {:?}",
            String::from_utf8_lossy(&format),
            String::from_utf8_lossy(&output)
        );
    }
    byte_count
}

pub(super) fn ppc_std_sprintf(cpu: &PpcCpu, memory: &mut PpcSectionMem) -> u32 {
    ppc_std_format(
        memory,
        cpu.gpr[3],
        cpu.gpr[4],
        "sprintf",
        |memory, index| ppc_sprintf_argument(cpu, memory, index),
    )
}

pub(super) fn ppc_std_vsprintf(cpu: &PpcCpu, memory: &mut PpcSectionMem) -> u32 {
    let argument_list = cpu.gpr[5];
    ppc_std_format(
        memory,
        cpu.gpr[3],
        cpu.gpr[4],
        "vsprintf",
        |memory, index| {
            argument_list
                .checked_add(u32::try_from(index).unwrap_or(u32::MAX).saturating_mul(4))
                .and_then(|address| memory.read_u32_be(address))
                .unwrap_or(0)
        },
    )
}

pub(super) fn ppc_std_strcmp(
    memory: &mut PpcSectionMem,
    first: u32,
    second: u32,
    limit: Option<u32>,
) -> i32 {
    let max = limit.unwrap_or(u32::MAX);
    for offset in 0..max {
        let a = first
            .checked_add(offset)
            .and_then(|addr| memory.read_u8(addr))
            .unwrap_or(0);
        let b = second
            .checked_add(offset)
            .and_then(|addr| memory.read_u8(addr))
            .unwrap_or(0);
        if a != b {
            return i32::from(a) - i32::from(b);
        }
        if a == 0 {
            break;
        }
    }
    0
}

pub(super) fn ppc_std_strcat(memory: &mut PpcSectionMem, destination: u32, source: u32) {
    let destination_len = ppc_std_strlen(memory, destination);
    let source_len = ppc_std_strlen(memory, source);
    let Some(copy_len) = source_len.checked_add(1) else {
        return;
    };
    let Some(destination_end) = destination.checked_add(destination_len) else {
        return;
    };
    ppc_std_memmove(memory, destination_end, source, copy_len);
}

pub(super) fn ppc_std_strncpy(
    memory: &mut PpcSectionMem,
    destination: u32,
    source: u32,
    count: u32,
) -> u32 {
    if count == 0 {
        return destination;
    }
    // Keep malformed guest counts from forcing an unbounded host allocation.
    // Classic C strings in the supported runtime are much smaller than this;
    // an over-limit request is treated as an invalid guest buffer.
    const MAX_STD_STRING_COPY: u32 = 16 * 1024 * 1024;
    if count > MAX_STD_STRING_COPY {
        return destination;
    }
    let mut bytes = Vec::new();
    for offset in 0..count {
        let byte = memory
            .read_u8(source.checked_add(offset).unwrap_or(u32::MAX))
            .unwrap_or(0);
        bytes.push(byte);
        if byte == 0 {
            bytes.resize(count as usize, 0);
            break;
        }
    }
    if ppc_memory_can_write_bytes(memory, destination, count) {
        let _ = memory.write_bytes(destination, &bytes);
    }
    destination
}

pub(super) fn ppc_std_strncat(
    memory: &mut PpcSectionMem,
    destination: u32,
    source: u32,
    count: u32,
) -> u32 {
    const MAX_STD_STRING_COPY: u32 = 16 * 1024 * 1024;
    if count > MAX_STD_STRING_COPY {
        return destination;
    }
    let destination_end = destination.checked_add(ppc_std_strlen(memory, destination));
    let Some(destination_end) = destination_end else {
        return destination;
    };
    let source_bytes = ppc_std_c_string(memory, source, count as usize);
    let Some(total) = u32::try_from(source_bytes.len())
        .ok()
        .and_then(|length| length.checked_add(1))
    else {
        return destination;
    };
    if ppc_memory_can_write_bytes(memory, destination_end, total) {
        let _ = memory.write_bytes(destination_end, &source_bytes);
        let _ = memory.write_u8(destination_end + total - 1, 0);
    }
    destination
}

pub(super) fn ppc_std_atoi(memory: &mut PpcSectionMem, string: u32) -> u32 {
    let mut offset = 0u32;
    while memory
        .read_u8(string.checked_add(offset).unwrap_or(u32::MAX))
        .is_some_and(|byte| byte.is_ascii_whitespace())
    {
        offset = offset.saturating_add(1);
    }
    let negative = match memory.read_u8(string.checked_add(offset).unwrap_or(u32::MAX)) {
        Some(b'-') => {
            offset = offset.saturating_add(1);
            true
        }
        Some(b'+') => {
            offset = offset.saturating_add(1);
            false
        }
        _ => false,
    };
    let mut value = 0i64;
    let mut saw_digit = false;
    while let Some(byte) = memory.read_u8(string.checked_add(offset).unwrap_or(u32::MAX)) {
        if !byte.is_ascii_digit() {
            break;
        }
        saw_digit = true;
        value = value
            .saturating_mul(10)
            .saturating_add(i64::from(byte - b'0'));
        offset = offset.saturating_add(1);
    }
    if !saw_digit {
        return 0;
    }
    let signed = if negative { -value } else { value };
    signed.clamp(i32::MIN as i64, i32::MAX as i64) as i32 as u32
}
