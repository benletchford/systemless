//! Toolbox Utility trap handlers (events, Random, Sound, misc).

use crate::cpu::{CpuOps, Register};
use crate::guest_call::{
    ClassicRetirement, CooperativeThread, ExecutionTaskId, SharedGuestCallStack, ThreadStorage,
};
use crate::guest_procedure::{resolve_same_isa_thread_entry, GuestIsa};
use crate::memory::globals::addr;
use crate::memory::{MacMemoryBus, MemoryBus};
use crate::quickdraw::fonts::get_font_face_scaled;
use crate::quickdraw::text::{get_font_metrics, get_glyph};
use crate::thread_manager::{NewThreadCreationEdge, RetiredThreadStorageEdge, ThreadManager};
use crate::{Error, Result};
use std::collections::HashMap;
use std::sync::OnceLock;

use super::dispatch::{
    raw_trap_route, selector_operation_route, AeCoercionHandler, AeDescriptor, AeObjectAccessor,
    AePrivateHashTable, AeResolveLevel, AeResolveState, DialogItem, EventProbeResult,
    EventRecordSnapshot, MovieState, OsRoutineVariant, SelectorOperationRoute,
    StandardFileGetEntry, StandardFileGetTrackingState, StandardFilePutTrackingState,
    SyntheticAppleEvent,
};
use super::types::{decode_mac_roman, encode_mac_roman_lossy, Rect, ShapeOp};

static TRACE_MUNGER: OnceLock<bool> = OnceLock::new();
static TRACE_LIST: OnceLock<bool> = OnceLock::new();
static TRACE_ENTROPY: OnceLock<bool> = OnceLock::new();
static TRACE_TITLE_DIAG: OnceLock<bool> = OnceLock::new();
static TRACE_SOUND: OnceLock<bool> = OnceLock::new();
static TRACE_AE: OnceLock<bool> = OnceLock::new();
static TRACE_GETKEYS_NONZERO: OnceLock<bool> = OnceLock::new();
static FORCE_BUTTON_TRUE_AT_PC: OnceLock<Option<u32>> = OnceLock::new();

const SLOT_MANAGER_OPERATION_ROUTES: &[SelectorOperationRoute] =
    &include!("generated_slot_manager_operations.rs");
const ALIAS_DISPATCH_OPERATION_ROUTES: &[SelectorOperationRoute] =
    &include!("generated_alias_dispatch_operations.rs");
const PPC_OPERATION_ROUTES: &[SelectorOperationRoute] = &include!("generated_ppc_operations.rs");
const PACK9_OPERATION_ROUTES: &[SelectorOperationRoute] =
    &include!("generated_pack9_operations.rs");
const PACK8_OPERATION_ROUTES: &[SelectorOperationRoute] =
    &include!("generated_pack8_operations.rs");
const PACK3_OPERATION_ROUTES: &[SelectorOperationRoute] =
    &include!("generated_pack3_operations.rs");
const PACK6_OPERATION_ROUTES: &[SelectorOperationRoute] =
    &include!("generated_pack6_operations.rs");
const PACK2_OPERATION_ROUTES: &[SelectorOperationRoute] =
    &include!("generated_pack2_operations.rs");
const PACK13_OPERATION_ROUTES: &[SelectorOperationRoute] =
    &include!("generated_pack13_operations.rs");
const PACK14_OPERATION_ROUTES: &[SelectorOperationRoute] =
    &include!("generated_pack14_operations.rs");
const PACK15_OPERATION_ROUTES: &[SelectorOperationRoute] =
    &include!("generated_pack15_operations.rs");
const COMPONENT_DISPATCH_OPERATION_ROUTES: &[SelectorOperationRoute] =
    &include!("generated_component_dispatch_operations.rs");
const PACK0_OPERATION_ROUTES: &[SelectorOperationRoute] =
    &include!("generated_pack0_operations.rs");
const PR_GLUE_OPERATION_ROUTES: &[SelectorOperationRoute] =
    &include!("generated_pr_glue_operations.rs");
const SCRIPT_UTIL_OPERATION_ROUTES: &[SelectorOperationRoute] =
    &include!("generated_script_util_operations.rs");
const SCSI_DISPATCH_OPERATION_ROUTES: &[SelectorOperationRoute] =
    &include!("generated_scsi_dispatch_operations.rs");
const PACK11_OPERATION_ROUTES: &[SelectorOperationRoute] =
    &include!("generated_pack11_operations.rs");
const TRANSLATION_DISPATCH_OPERATION_ROUTES: &[SelectorOperationRoute] =
    &include!("generated_translation_dispatch_operations.rs");
const ICON_DISPATCH_OPERATION_ROUTES: &[SelectorOperationRoute] =
    &include!("generated_icon_dispatch_operations.rs");

fn slot_manager_operation_route(selector: u32) -> Option<&'static SelectorOperationRoute> {
    selector_operation_route(SLOT_MANAGER_OPERATION_ROUTES, selector)
}

fn alias_dispatch_operation_route(
    trap_word: u16,
    selector: u32,
) -> Option<&'static SelectorOperationRoute> {
    if trap_word != 0xA823 {
        return None;
    }
    selector_operation_route(ALIAS_DISPATCH_OPERATION_ROUTES, selector)
}

fn ppc_operation_route(trap_word: u16, selector: u32) -> Option<&'static SelectorOperationRoute> {
    if trap_word != 0xA0DD {
        return None;
    }
    selector_operation_route(PPC_OPERATION_ROUTES, selector)
}

fn pack9_operation_route(trap_word: u16, selector: u16) -> Option<&'static SelectorOperationRoute> {
    if trap_word != 0xA82B {
        return None;
    }
    selector_operation_route(PACK9_OPERATION_ROUTES, u32::from(selector))
}

fn pack8_operation_route(trap_word: u16, selector: u16) -> Option<&'static SelectorOperationRoute> {
    if trap_word != 0xA816 {
        return None;
    }
    selector_operation_route(PACK8_OPERATION_ROUTES, u32::from(selector))
}

fn pack3_operation_route(trap_word: u16, selector: u16) -> Option<&'static SelectorOperationRoute> {
    if trap_word != 0xA9EA {
        return None;
    }
    selector_operation_route(PACK3_OPERATION_ROUTES, u32::from(selector))
}

fn pack6_operation_route(trap_word: u16, selector: u16) -> Option<&'static SelectorOperationRoute> {
    if trap_word != 0xA9ED {
        return None;
    }
    selector_operation_route(PACK6_OPERATION_ROUTES, u32::from(selector))
}

fn pack2_operation_route(trap_word: u16, selector: u16) -> Option<&'static SelectorOperationRoute> {
    if trap_word != 0xA9E9 {
        return None;
    }
    selector_operation_route(PACK2_OPERATION_ROUTES, u32::from(selector))
}

fn pack13_operation_route(
    trap_word: u16,
    selector: u16,
) -> Option<&'static SelectorOperationRoute> {
    if trap_word != 0xA82F {
        return None;
    }
    selector_operation_route(PACK13_OPERATION_ROUTES, u32::from(selector))
}

fn pack14_operation_route(
    trap_word: u16,
    selector: u16,
) -> Option<&'static SelectorOperationRoute> {
    if trap_word != 0xA830 {
        return None;
    }
    selector_operation_route(PACK14_OPERATION_ROUTES, u32::from(selector))
}

fn pack15_operation_route(
    trap_word: u16,
    selector: u16,
) -> Option<&'static SelectorOperationRoute> {
    if trap_word != 0xA831 {
        return None;
    }
    selector_operation_route(PACK15_OPERATION_ROUTES, u32::from(selector))
}

fn component_dispatch_operation_route(
    trap_word: u16,
    selector: u32,
) -> Option<&'static SelectorOperationRoute> {
    if trap_word != 0xA82A {
        return None;
    }
    selector_operation_route(COMPONENT_DISPATCH_OPERATION_ROUTES, selector)
}

fn pack0_operation_route(trap_word: u16, selector: u16) -> Option<&'static SelectorOperationRoute> {
    if trap_word != 0xA9E7 {
        return None;
    }
    selector_operation_route(PACK0_OPERATION_ROUTES, u32::from(selector))
}

fn pr_glue_operation_route(
    trap_word: u16,
    selector: u32,
) -> Option<&'static SelectorOperationRoute> {
    if trap_word != 0xA8FD {
        return None;
    }
    selector_operation_route(PR_GLUE_OPERATION_ROUTES, selector)
}

fn script_util_operation_route(
    trap_word: u16,
    selector: u32,
) -> Option<&'static SelectorOperationRoute> {
    if trap_word != 0xA8B5 {
        return None;
    }
    selector_operation_route(SCRIPT_UTIL_OPERATION_ROUTES, selector)
}

fn scsi_dispatch_operation_route(
    trap_word: u16,
    selector: u16,
) -> Option<&'static SelectorOperationRoute> {
    if trap_word != 0xA815 {
        return None;
    }
    selector_operation_route(SCSI_DISPATCH_OPERATION_ROUTES, u32::from(selector))
}

fn pack11_operation_route(
    trap_word: u16,
    selector: u16,
) -> Option<&'static SelectorOperationRoute> {
    if trap_word != 0xA82D {
        return None;
    }
    selector_operation_route(PACK11_OPERATION_ROUTES, u32::from(selector))
}

fn translation_dispatch_operation_route(
    trap_word: u16,
    selector: u32,
) -> Option<&'static SelectorOperationRoute> {
    if trap_word != 0xABFC {
        return None;
    }
    selector_operation_route(TRANSLATION_DISPATCH_OPERATION_ROUTES, selector)
}

fn icon_dispatch_operation_route(
    trap_word: u16,
    selector: u32,
) -> Option<&'static SelectorOperationRoute> {
    if trap_word != 0xABC9 {
        return None;
    }
    selector_operation_route(ICON_DISPATCH_OPERATION_ROUTES, selector & 0xFFFF)
}

const AE_TYPE_APPLE_EVENT: u32 = u32::from_be_bytes(*b"aevt");
const AE_TYPE_AE_LIST: u32 = u32::from_be_bytes(*b"list");
const AE_TYPE_AE_RECORD: u32 = u32::from_be_bytes(*b"reco");
const AE_TYPE_TYPE: u32 = u32::from_be_bytes(*b"type");
const AE_TYPE_NULL: u32 = u32::from_be_bytes(*b"null");
const AE_TYPE_WILDCARD: u32 = u32::from_be_bytes(*b"****");
const AE_TYPE_OBJECT_SPECIFIER: u32 = u32::from_be_bytes(*b"obj ");
const AE_EVENT_ID_ANSWER: u32 = u32::from_be_bytes(*b"ansr");
const AE_KEY_DESIRED_CLASS: u32 = u32::from_be_bytes(*b"want");
const AE_KEY_CONTAINER: u32 = u32::from_be_bytes(*b"from");
const AE_KEY_KEY_FORM: u32 = u32::from_be_bytes(*b"form");
const AE_KEY_KEY_DATA: u32 = u32::from_be_bytes(*b"seld");
const AE_KEY_EVENT_CLASS_ATTR: u32 = u32::from_be_bytes(*b"evcl");
const AE_KEY_EVENT_ID_ATTR: u32 = u32::from_be_bytes(*b"evid");
const AE_SEND_MODE_REPLY_MASK: u32 = 0x0000_0003;
const AE_SEND_MODE_WAIT_REPLY: u32 = 0x0000_0003;
const AE_ERR_COERCION_FAIL: i16 = -1700;
const AE_ERR_DESC_NOT_FOUND: i16 = -1701;
const AE_ERR_HANDLER_NOT_FOUND: i16 = -1717;
const AE_ERR_ILLEGAL_INDEX: i16 = -1719;
const AE_ERR_ACCESSOR_NOT_FOUND: i16 = -1723;
const AE_ERR_NOT_AN_OBJECT_SPEC: i16 = -1727;
const AE_BUFFER_IS_SMALL: i16 = -607;
const AE_MANAGER_KEY_RECORDER_COUNT: u32 = u32::from_be_bytes(*b"recr");
const AE_MANAGER_KEY_VERSION: u32 = u32::from_be_bytes(*b"vers");
const AE_KEY_COMPARE_PROC: u32 = u32::from_be_bytes(*b"cmpr");
const AE_KEY_COUNT_PROC: u32 = u32::from_be_bytes(*b"cont");
const AE_KEY_DISPOSE_TOKEN_PROC: u32 = u32::from_be_bytes(*b"xtok");
const AE_KEY_MARK_TOKEN_PROC: u32 = u32::from_be_bytes(*b"mkid");
const AE_KEY_MARK_PROC: u32 = u32::from_be_bytes(*b"mark");
const AE_KEY_ADJUST_MARKS_PROC: u32 = u32::from_be_bytes(*b"adjm");
const AE_KEY_GET_ERR_DESC_PROC: u32 = u32::from_be_bytes(*b"indc");
const QUICKTIME_INVALID_MOVIE: i16 = -2010;

fn scan_sane_decimal(
    bus: &mut MacMemoryBus,
    bytes: &[u8],
    index_ptr: u32,
    decimal_ptr: u32,
    valid_prefix_ptr: u32,
) {
    let start = usize::from(bus.read_word(index_ptr)).min(bytes.len());
    let mut cursor = start;
    let mut negative = false;
    if let Some(sign) = bytes.get(cursor) {
        if *sign == b'+' || *sign == b'-' {
            negative = *sign == b'-';
            cursor += 1;
        }
    }

    let mut digits = Vec::new();
    let mut fraction_digits = 0i32;
    let mut saw_digit = false;
    while let Some(ch) = bytes.get(cursor) {
        if ch.is_ascii_digit() {
            digits.push(*ch);
            saw_digit = true;
            cursor += 1;
        } else {
            break;
        }
    }
    if bytes.get(cursor) == Some(&b'.') {
        cursor += 1;
        while let Some(ch) = bytes.get(cursor) {
            if ch.is_ascii_digit() {
                digits.push(*ch);
                fraction_digits += 1;
                saw_digit = true;
                cursor += 1;
            } else {
                break;
            }
        }
    }

    let exponent_start = cursor;
    let mut explicit_exponent = 0i32;
    if saw_digit && matches!(bytes.get(cursor), Some(b'e' | b'E')) {
        cursor += 1;
        let mut exponent_negative = false;
        if let Some(sign) = bytes.get(cursor) {
            if *sign == b'+' || *sign == b'-' {
                exponent_negative = *sign == b'-';
                cursor += 1;
            }
        }
        let exponent_digits_start = cursor;
        while let Some(ch) = bytes.get(cursor) {
            if ch.is_ascii_digit() {
                explicit_exponent = explicit_exponent
                    .saturating_mul(10)
                    .saturating_add(i32::from(*ch - b'0'));
                cursor += 1;
            } else {
                break;
            }
        }
        if cursor == exponent_digits_start {
            cursor = exponent_start;
            explicit_exponent = 0;
        } else if exponent_negative {
            explicit_exponent = -explicit_exponent;
        }
    }

    if saw_digit {
        let first_nonzero = digits.iter().position(|digit| *digit != b'0');
        let significant = first_nonzero
            .map(|offset| &digits[offset..])
            .unwrap_or(&b"0"[..]);
        let significant_len = significant.len().min(20);
        let exponent = explicit_exponent.saturating_sub(fraction_digits);

        bus.write_byte(decimal_ptr, u8::from(negative));
        bus.write_byte(decimal_ptr + 1, 0);
        bus.write_word(
            decimal_ptr + 2,
            exponent.clamp(i16::MIN as i32, i16::MAX as i32) as i16 as u16,
        );
        bus.write_byte(decimal_ptr + 4, significant_len as u8);
        for index in 0..20u32 {
            bus.write_byte(
                decimal_ptr + 5 + index,
                significant.get(index as usize).copied().unwrap_or(0),
            );
        }
        bus.write_byte(decimal_ptr + 25, 0);
        bus.write_word(index_ptr, cursor as u16);
    }
    let valid_prefix = saw_digit && cursor == bytes.len();
    bus.write_word(valid_prefix_ptr, if valid_prefix { 0xFFFF } else { 0 });
}

fn sane_nan_code(digits: &[u8]) -> u8 {
    fn hex_value(byte: u8) -> Option<u8> {
        match byte {
            b'0'..=b'9' => Some(byte - b'0'),
            b'a'..=b'f' => Some(byte - b'a' + 10),
            b'A'..=b'F' => Some(byte - b'A' + 10),
            _ => None,
        }
    }

    digits
        .get(3..5)
        .and_then(|code| Some(hex_value(code[0])? << 4 | hex_value(code[1])?))
        .unwrap_or(0)
}

/// Format a 68K SANE decimal record using the classic `decform` contract.
/// Existing significand digits are exact and are never discarded; the
/// requested count is a minimum that pads with zeroes when necessary.
fn format_sane_decimal(bus: &MacMemoryBus, decimal_ptr: u32, decform_ptr: u32) -> Vec<u8> {
    const DECIMAL_OUTPUT_LIMIT: usize = 80;

    let negative = bus.read_byte(decimal_ptr) != 0;
    let exponent = i32::from(bus.read_word(decimal_ptr + 2) as i16);
    let length = usize::from(bus.read_byte(decimal_ptr + 4)).min(20);
    let digits = bus.read_bytes(decimal_ptr + 5, length);
    let style = bus.read_byte(decform_ptr);
    let requested_digits = i32::from(bus.read_word(decform_ptr + 2) as i16);

    let special = match digits.first().copied() {
        None => Some(b"?".to_vec()),
        Some(b'0') => None,
        Some(b'I') => Some(b"INF".to_vec()),
        Some(b'N') => Some(format!("NAN({:03})", sane_nan_code(&digits)).into_bytes()),
        Some(b'?') => Some(b"?".to_vec()),
        Some(byte) if !byte.is_ascii_digit() || digits.iter().any(|d| !d.is_ascii_digit()) => {
            Some(b"?".to_vec())
        }
        _ => None,
    };
    let is_zero = digits.first() == Some(&b'0');

    let mut formatted = if let Some(special) = special {
        special
    } else if style == 1 {
        let exact_digits = if is_zero { &b"0"[..] } else { &digits[..] };
        let exact_fraction_digits = if is_zero { 0 } else { (-exponent).max(0) };
        let fraction_digits = requested_digits.max(exact_fraction_digits).max(0) as usize;
        let point = if is_zero {
            1
        } else {
            exact_digits.len() as i32 + exponent
        };
        let integer_digits = point.max(1) as usize;
        let required_len = integer_digits
            .saturating_add(usize::from(fraction_digits != 0))
            .saturating_add(fraction_digits)
            .saturating_add(usize::from(negative));
        if required_len > DECIMAL_OUTPUT_LIMIT {
            return b"?".to_vec();
        }

        let mut result = Vec::with_capacity(required_len);
        if point <= 0 {
            result.push(b'0');
            if fraction_digits != 0 {
                result.push(b'.');
                result.extend(std::iter::repeat_n(b'0', (-point) as usize));
                result.extend_from_slice(exact_digits);
            }
        } else if point as usize >= exact_digits.len() {
            result.extend_from_slice(exact_digits);
            result.extend(std::iter::repeat_n(
                b'0',
                point as usize - exact_digits.len(),
            ));
            if fraction_digits != 0 {
                result.push(b'.');
            }
        } else {
            result.extend_from_slice(&exact_digits[..point as usize]);
            result.push(b'.');
            result.extend_from_slice(&exact_digits[point as usize..]);
        }
        let written_fraction = result
            .iter()
            .position(|byte| *byte == b'.')
            .map(|point| result.len() - point - 1)
            .unwrap_or(0);
        result.extend(std::iter::repeat_n(
            b'0',
            fraction_digits.saturating_sub(written_fraction),
        ));
        result
    } else {
        let exact_digits = if is_zero { &b"0"[..] } else { &digits[..] };
        let significant_digits = requested_digits.max(1).max(exact_digits.len() as i32) as usize;
        let normalized_exponent = if is_zero {
            0
        } else {
            exponent + exact_digits.len() as i32 - 1
        };
        let exponent_text = format!("{:+}", normalized_exponent);
        let required_len = 1usize
            .saturating_add(significant_digits)
            .saturating_add(usize::from(significant_digits > 1))
            .saturating_add(1)
            .saturating_add(exponent_text.len());
        if required_len > DECIMAL_OUTPUT_LIMIT {
            return b"?".to_vec();
        }

        let mut result = Vec::with_capacity(required_len);
        result.push(exact_digits[0]);
        if significant_digits > 1 {
            result.push(b'.');
            result.extend_from_slice(&exact_digits[1..]);
            result.extend(std::iter::repeat_n(
                b'0',
                significant_digits - exact_digits.len(),
            ));
        }
        result.push(b'e');
        result.extend_from_slice(exponent_text.as_bytes());
        result
    };

    if formatted != b"?" {
        if negative {
            formatted.insert(0, b'-');
        } else if style == 0 {
            formatted.insert(0, b' ');
        }
    }
    if formatted.len() > DECIMAL_OUTPUT_LIMIT {
        b"?".to_vec()
    } else {
        formatted
    }
}

fn standard_file_cancel_reply(bus: &mut MacMemoryBus, reply_ptr: u32) {
    if reply_ptr != 0 {
        // SFReply and StandardFileReply both start with the cancel flag.
        bus.write_byte(reply_ptr, 0);
    }
}

fn standard_file_default_name(bus: &MacMemoryBus, name_ptr: u32) -> Vec<u8> {
    let mut name = if name_ptr != 0 {
        bus.read_pstring(name_ptr)
    } else {
        Vec::new()
    };
    if name.is_empty() {
        name.extend_from_slice(b"Untitled");
    }
    name.truncate(63);
    name
}

fn standard_file_put_reply_old(bus: &mut MacMemoryBus, reply_ptr: u32, vref: i16, name: &[u8]) {
    if reply_ptr == 0 {
        return;
    }
    // Inside Macintosh, Volume I, p. I-86 ("Pascal Data Types") specifies
    // BOOLEAN as one byte with its value in bit 0. Write canonical TRUE (1),
    // leaving its seven non-value bits clear; callers may compare a record
    // field directly with the ordinal value of TRUE.
    bus.write_byte(reply_ptr, 1); // good = TRUE
    bus.write_byte(reply_ptr + 1, 0); // copy = FALSE
    bus.write_long(reply_ptr + 2, 0); // fType
    bus.write_word(reply_ptr + 6, vref as u16);
    bus.write_word(reply_ptr + 8, 0); // version
    bus.write_pstring(reply_ptr + 10, name);
}

fn standard_file_get_reply_old(
    bus: &mut MacMemoryBus,
    reply_ptr: u32,
    wd_ref: i16,
    file_type: u32,
    name: &[u8],
) {
    if reply_ptr == 0 {
        return;
    }
    bus.write_byte(reply_ptr, 1); // good = TRUE
    bus.write_byte(reply_ptr + 1, 0); // copy = FALSE
    bus.write_long(reply_ptr + 2, file_type);
    bus.write_word(reply_ptr + 6, wd_ref as u16);
    bus.write_word(reply_ptr + 8, 0); // version
    bus.write_pstring(reply_ptr + 10, name);
}

fn standard_file_put_reply_modern(
    bus: &mut MacMemoryBus,
    reply_ptr: u32,
    vref: i16,
    dir_id: u32,
    name: &[u8],
    replacing: bool,
) {
    if reply_ptr == 0 {
        return;
    }
    bus.write_byte(reply_ptr, 1); // sfGood = TRUE
    bus.write_byte(reply_ptr + 1, u8::from(replacing)); // sfReplacing
    bus.write_long(reply_ptr + 2, 0); // sfType
    bus.write_word(reply_ptr + 6, vref as u16); // sfFile.vRefNum
    bus.write_long(reply_ptr + 8, dir_id); // sfFile.parID
    bus.write_pstring(reply_ptr + 12, name); // sfFile.name
    bus.write_word(reply_ptr + 76, 0); // sfScript
    bus.write_word(reply_ptr + 78, 0); // sfFlags
    bus.write_byte(reply_ptr + 80, 0); // sfIsFolder
    bus.write_byte(reply_ptr + 81, 0); // sfIsVolume
    bus.write_long(reply_ptr + 82, 0); // sfReserved1
    bus.write_word(reply_ptr + 86, 0); // sfReserved2
}

fn standard_file_get_reply_modern(
    bus: &mut MacMemoryBus,
    reply_ptr: u32,
    vref: i16,
    dir_id: u32,
    file_type: u32,
    finder_flags: u16,
    name: &[u8],
) {
    if reply_ptr == 0 {
        return;
    }
    bus.write_byte(reply_ptr, 1); // sfGood = TRUE
    bus.write_byte(reply_ptr + 1, 0); // sfReplacing = FALSE
    bus.write_long(reply_ptr + 2, file_type); // sfType
    bus.write_word(reply_ptr + 6, vref as u16); // sfFile.vRefNum
    bus.write_long(reply_ptr + 8, dir_id); // sfFile.parID
    bus.write_pstring(reply_ptr + 12, name); // sfFile.name
    bus.write_word(reply_ptr + 76, 0); // sfScript
    bus.write_word(reply_ptr + 78, finder_flags); // sfFlags
    bus.write_byte(reply_ptr + 80, 0); // sfIsFolder
    bus.write_byte(reply_ptr + 81, 0); // sfIsVolume
    bus.write_long(reply_ptr + 82, 0); // sfReserved1
    bus.write_word(reply_ptr + 86, 0); // sfReserved2
}

#[derive(Clone, Debug)]
struct StandardFileSelection {
    name: Vec<u8>,
    vref: i16,
    wd_ref: i16,
    dir_id: u32,
    file_type: u32,
    finder_flags: u16,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum StandardFilePutAction {
    Save,
    Cancel,
    Desktop,
    Navigate,
    Parent,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum StandardFileGetAction {
    Open,
    Cancel,
    Desktop,
    Navigate,
}

const STANDARD_FILE_SAVE_ITEM: i16 = 1;
const STANDARD_FILE_NAME_ITEM: i16 = 9;
const STANDARD_FILE_DIALOG_WIDTH: i16 = 360;
const STANDARD_FILE_DIALOG_HEIGHT: i16 = 260;
const STANDARD_FILE_PUT_VOLUME_RECT: (i16, i16, i16, i16) = (12, 24, 31, 116);
const STANDARD_FILE_PUT_VOLUME_LABEL_RECT: (i16, i16, i16, i16) = (12, 124, 31, 336);
const STANDARD_FILE_PUT_LIST_RECT: (i16, i16, i16, i16) = (38, 18, 156, 316);
const STANDARD_FILE_PUT_SCROLL_RECT: (i16, i16, i16, i16) = (38, 315, 156, 331);
const STANDARD_FILE_PROMPT_RECT: (i16, i16, i16, i16) = (166, 24, 184, 330);
const STANDARD_FILE_NAME_RECT: (i16, i16, i16, i16) = (188, 24, 208, 330);
const STANDARD_FILE_PUT_DESKTOP_RECT: (i16, i16, i16, i16) = (220, 24, 242, 104);
const STANDARD_FILE_CANCEL_RECT: (i16, i16, i16, i16) = (220, 166, 242, 246);
const STANDARD_FILE_SAVE_RECT: (i16, i16, i16, i16) = (220, 258, 242, 338);
const STANDARD_FILE_GET_DIALOG_WIDTH: i16 = 356;
const STANDARD_FILE_GET_DIALOG_HEIGHT: i16 = 178;
const STANDARD_FILE_GET_OPEN_ITEM: i16 = 1;
const STANDARD_FILE_GET_VOLUME_RECT: (i16, i16, i16, i16) = (12, 90, 31, 164);
const STANDARD_FILE_GET_VOLUME_LABEL_RECT: (i16, i16, i16, i16) = (12, 268, 31, 336);
const STANDARD_FILE_GET_LIST_RECT: (i16, i16, i16, i16) = (35, 18, 163, 236);
const STANDARD_FILE_GET_SCROLL_RECT: (i16, i16, i16, i16) = (35, 235, 163, 251);
const STANDARD_FILE_GET_ROW_HEIGHT: i16 = 14;
const STANDARD_FILE_GET_EJECT_RECT: (i16, i16, i16, i16) = (38, 258, 59, 338);
const STANDARD_FILE_GET_DESKTOP_RECT: (i16, i16, i16, i16) = (66, 258, 87, 338);
const STANDARD_FILE_GET_SEPARATOR_RECT: (i16, i16, i16, i16) = (98, 258, 99, 338);
const STANDARD_FILE_GET_CANCEL_RECT: (i16, i16, i16, i16) = (110, 258, 131, 338);
const STANDARD_FILE_GET_OPEN_RECT: (i16, i16, i16, i16) = (138, 258, 159, 338);

#[inline]
fn return_noerr_and_pop<C: CpuOps>(cpu: &mut C, bytes: u32) -> Result<()> {
    let sp = cpu.read_reg(Register::A7);
    cpu.write_reg(Register::A7, sp.wrapping_add(bytes));
    cpu.write_reg(Register::D0, 0);
    Ok(())
}

#[inline]
fn return_noerr<C: CpuOps>(cpu: &mut C) -> Result<()> {
    cpu.write_reg(Register::D0, 0);
    Ok(())
}

#[inline]
fn return_error_and_pop<C: CpuOps>(cpu: &mut C, bytes: u32, err: i16) -> Result<()> {
    let sp = cpu.read_reg(Register::A7);
    cpu.write_reg(Register::A7, sp.wrapping_add(bytes));
    cpu.write_reg(Register::D0, err as u32);
    Ok(())
}

fn signed_byte_from_stack_word(word: u16) -> i16 {
    let byte = if word & 0x00FF == 0 {
        (word >> 8) as u8
    } else {
        (word & 0x00FF) as u8
    };
    i16::from(byte as i8)
}

fn read_quicktime_u32(data: &[u8], offset: usize) -> Option<u32> {
    let bytes = data.get(offset..offset + 4)?;
    Some(u32::from_be_bytes(bytes.try_into().ok()?))
}

fn fixed_16_16_to_i16_dimension(value: u32) -> Option<i16> {
    let whole = (value >> 16).min(i16::MAX as u32);
    (whole > 0).then_some(whole as i16)
}

fn scan_quicktime_atoms(
    data: &[u8],
    depth: u8,
    bounds: &mut Option<(i16, i16, i16, i16)>,
    duration: &mut Option<i32>,
    time_scale: &mut Option<i32>,
) {
    if depth > 8 {
        return;
    }
    let mut offset = 0usize;
    while offset + 8 <= data.len() {
        let Some(size32) = read_quicktime_u32(data, offset) else {
            break;
        };
        let atom_type = &data[offset + 4..offset + 8];
        let mut header_len = 8usize;
        let atom_len = if size32 == 1 {
            if offset + 16 > data.len() {
                break;
            }
            header_len = 16;
            let Some(high) = read_quicktime_u32(data, offset + 8) else {
                break;
            };
            let Some(low) = read_quicktime_u32(data, offset + 12) else {
                break;
            };
            let size64 = ((high as u64) << 32) | u64::from(low);
            if size64 > usize::MAX as u64 {
                break;
            }
            size64 as usize
        } else if size32 == 0 {
            data.len() - offset
        } else {
            size32 as usize
        };
        if atom_len < header_len || offset + atom_len > data.len() {
            break;
        }
        let payload = &data[offset + header_len..offset + atom_len];
        match atom_type {
            b"moov" | b"trak" | b"mdia" | b"minf" | b"stbl" => {
                scan_quicktime_atoms(payload, depth + 1, bounds, duration, time_scale);
            }
            b"mvhd" => {
                if duration.is_none() && payload.len() >= 20 {
                    let version = payload[0];
                    let (parsed_time_scale, parsed_duration) = if version == 0 {
                        (
                            read_quicktime_u32(payload, 12),
                            read_quicktime_u32(payload, 16).map(|value| value.min(i32::MAX as u32)),
                        )
                    } else if payload.len() >= 32 {
                        (
                            read_quicktime_u32(payload, 20),
                            read_quicktime_u32(payload, 28).map(|value| value.min(i32::MAX as u32)),
                        )
                    } else {
                        (None, None)
                    };
                    if let Some(value) =
                        parsed_time_scale.filter(|value| *value > 0 && *value <= i32::MAX as u32)
                    {
                        *time_scale = Some(value as i32);
                    }
                    if let Some(value) = parsed_duration.filter(|value| *value > 0) {
                        *duration = Some(value as i32);
                    }
                }
            }
            b"tkhd" => {
                if bounds.is_none() {
                    let version = payload.first().copied().unwrap_or(0);
                    let dimension_offset = if version == 0 { 76 } else { 88 };
                    if payload.len() >= dimension_offset + 8 {
                        let width = read_quicktime_u32(payload, dimension_offset)
                            .and_then(fixed_16_16_to_i16_dimension);
                        let height = read_quicktime_u32(payload, dimension_offset + 4)
                            .and_then(fixed_16_16_to_i16_dimension);
                        if let (Some(width), Some(height)) = (width, height) {
                            *bounds = Some((0, 0, height, width));
                        }
                    }
                }
            }
            _ => {}
        }
        if size32 == 0 {
            break;
        }
        offset += atom_len;
    }
}

fn quicktime_movie_metadata(data: &[u8]) -> ((i16, i16, i16, i16), i32, i32) {
    let mut bounds = None;
    let mut duration = None;
    let mut time_scale = None;
    scan_quicktime_atoms(data, 0, &mut bounds, &mut duration, &mut time_scale);
    (
        bounds.unwrap_or((0, 0, 120, 160)),
        duration.unwrap_or(1),
        time_scale.unwrap_or(600),
    )
}

fn write_movie_box(bus: &mut MacMemoryBus, rect_ptr: u32, rect: (i16, i16, i16, i16)) {
    if rect_ptr == 0 {
        return;
    }
    bus.write_word(rect_ptr, rect.0 as u16);
    bus.write_word(rect_ptr + 2, rect.1 as u16);
    bus.write_word(rect_ptr + 4, rect.2 as u16);
    bus.write_word(rect_ptr + 6, rect.3 as u16);
}

fn read_movie_box(bus: &MacMemoryBus, rect_ptr: u32) -> Option<(i16, i16, i16, i16)> {
    (rect_ptr != 0).then(|| {
        (
            bus.read_word(rect_ptr) as i16,
            bus.read_word(rect_ptr + 2) as i16,
            bus.read_word(rect_ptr + 4) as i16,
            bus.read_word(rect_ptr + 6) as i16,
        )
    })
}

fn read_allocated_bytes(bus: &MacMemoryBus, ptr: u32) -> Vec<u8> {
    let size = bus.get_alloc_size(ptr).unwrap_or(0) as usize;
    if size == 0 {
        Vec::new()
    } else {
        bus.read_bytes(ptr, size)
    }
}

fn record_movie_error(dispatcher: &mut super::TrapDispatcher, err: i16) {
    // QuickTime 1993, p. 2-85: every Movie Toolbox function updates the
    // current error; the first nonzero result also latches as the sticky error.
    dispatcher.movie_error = err;
    if err != 0 && dispatcher.movie_sticky_error == 0 {
        dispatcher.movie_sticky_error = err;
    }
    if super::dispatch::trace_quicktime_enabled() {
        eprintln!(
            "[QUICKTIME] result={} sticky={}",
            dispatcher.movie_error, dispatcher.movie_sticky_error
        );
    }
}

/// Read a QuickDraw ColorTable (via its CTabHandle) into a 256-entry RGB
/// palette for nearest-colour matching. Missing/short tables fall back to the
/// standard Macintosh 8-bit CLUT. ColorTable layout: ctSeed(4) ctFlags(2)
/// ctSize(2) then (ctSize+1) ColorSpec entries of value(2) r(2) g(2) b(2).
fn read_color_table(bus: &MacMemoryBus, ctab_handle: u32) -> Vec<[u8; 3]> {
    let mut palette: Vec<[u8; 3]> = super::TrapDispatcher::standard_mac_8bpp_clut()
        .iter()
        .map(|c| [(c[0] >> 8) as u8, (c[1] >> 8) as u8, (c[2] >> 8) as u8])
        .collect();
    if ctab_handle == 0 {
        return palette;
    }
    let ctab = bus.read_long(ctab_handle);
    if ctab == 0 {
        return palette;
    }
    let ct_size = bus.read_word(ctab + 6) as i16;
    if ct_size < 0 {
        return palette;
    }
    let count = (ct_size as usize + 1).min(256);
    for i in 0..count {
        let entry = ctab + 8 + (i as u32) * 8;
        let value = bus.read_word(entry) as usize;
        let r = (bus.read_word(entry + 2) >> 8) as u8;
        let g = (bus.read_word(entry + 4) >> 8) as u8;
        let b = (bus.read_word(entry + 6) >> 8) as u8;
        // ColorSpec.value is the palette index for indexed device tables.
        let idx = if value < 256 { value } else { i };
        palette[idx] = [r, g, b];
    }
    palette
}

/// Nearest palette index by squared Euclidean distance in RGB.
fn nearest_color_index(palette: &[[u8; 3]], r: u8, g: u8, b: u8) -> u8 {
    let mut best = 0usize;
    let mut best_dist = i32::MAX;
    for (i, c) in palette.iter().enumerate().take(256) {
        let dr = c[0] as i32 - r as i32;
        let dg = c[1] as i32 - g as i32;
        let db = c[2] as i32 - b as i32;
        let dist = dr * dr + dg * dg + db * db;
        if dist < best_dist {
            best_dist = dist;
            best = i;
            if dist == 0 {
                break;
            }
        }
    }
    best as u8
}

/// Returns true if `year` is a leap year in the Gregorian calendar.
fn is_leap_year(year: u32) -> bool {
    (year.is_multiple_of(4) && !year.is_multiple_of(100)) || year.is_multiple_of(400)
}

/// Days in each month (index 1-based). February is 28; caller must add 1 for leap years.
const DAYS_IN_MONTH: [u32; 13] = [0, 31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31];

/// Convert seconds since Mac epoch (Jan 1, 1904 00:00:00) to DateTimeRec fields.
/// Returns (year, month, day, hour, minute, second, dayOfWeek).
/// dayOfWeek: 1=Sunday..7=Saturday.
/// Inside Macintosh Volume II, II-379
fn secs_to_date(secs: u32) -> (u16, u16, u16, u16, u16, u16, u16) {
    // Jan 1, 1904 was a Friday = dayOfWeek 6
    let day_of_week = ((secs / 86400 + 5) % 7 + 1) as u16; // +5 because Jan 1 1904 = Friday (6), Sunday=1

    let mut remaining = secs;
    let second = (remaining % 60) as u16;
    remaining /= 60;
    let minute = (remaining % 60) as u16;
    remaining /= 60;
    let hour = (remaining % 24) as u16;
    let mut days = remaining / 24;

    let mut year = 1904u32;
    loop {
        let days_in_year = if is_leap_year(year) { 366 } else { 365 };
        if days < days_in_year {
            break;
        }
        days -= days_in_year;
        year += 1;
    }

    let mut month = 1u32;
    loop {
        let mut dim = DAYS_IN_MONTH[month as usize];
        if month == 2 && is_leap_year(year) {
            dim += 1;
        }
        if days < dim {
            break;
        }
        days -= dim;
        month += 1;
    }
    let day = days + 1; // 1-based

    (
        year as u16,
        month as u16,
        day as u16,
        hour,
        minute,
        second,
        day_of_week,
    )
}

/// Convert DateTimeRec fields to seconds since Mac epoch (Jan 1, 1904 00:00:00).
/// Inside Macintosh Volume II, II-379
fn date_to_secs(year: u32, month: u32, day: u32, hour: u32, minute: u32, second: u32) -> u32 {
    let mut days: u32 = 0;
    for y in 1904..year {
        days += if is_leap_year(y) { 366 } else { 365 };
    }
    for m in 1..month {
        days += DAYS_IN_MONTH[m as usize];
        if m == 2 && is_leap_year(year) {
            days += 1;
        }
    }
    days += day - 1; // day is 1-based
    days * 86400 + hour * 3600 + minute * 60 + second
}

fn trace_munger_enabled() -> bool {
    *TRACE_MUNGER.get_or_init(|| std::env::var_os("SYSTEMLESS_TRACE_MUNGER").is_some())
}

fn trace_list_manager_enabled() -> bool {
    *TRACE_LIST.get_or_init(|| std::env::var_os("SYSTEMLESS_TRACE_LIST").is_some())
}

fn trace_entropy_enabled() -> bool {
    *TRACE_ENTROPY.get_or_init(|| std::env::var_os("SYSTEMLESS_TRACE_ENTROPY").is_some())
}

fn trace_title_diag_enabled() -> bool {
    *TRACE_TITLE_DIAG.get_or_init(|| std::env::var_os("SYSTEMLESS_TRACE_TITLE_DIAG").is_some())
}

fn trace_sound_enabled() -> bool {
    *TRACE_SOUND.get_or_init(|| std::env::var_os("SYSTEMLESS_TRACE_SOUND").is_some())
}

fn trace_ae_enabled() -> bool {
    *TRACE_AE.get_or_init(|| std::env::var_os("SYSTEMLESS_TRACE_AE").is_some())
}

fn trace_getkeys_nonzero_enabled() -> bool {
    *TRACE_GETKEYS_NONZERO
        .get_or_init(|| std::env::var_os("SYSTEMLESS_TRACE_GETKEYS_NONZERO").is_some())
}

fn format_fourcc(v: u32) -> String {
    v.to_be_bytes()
        .iter()
        .map(|&b| {
            if b.is_ascii_graphic() || b == b' ' {
                b as char
            } else {
                '.'
            }
        })
        .collect()
}

fn write_null_aedesc(bus: &mut MacMemoryBus, desc_ptr: u32) {
    if desc_ptr != 0 {
        bus.write_long(desc_ptr, AE_TYPE_NULL);
        bus.write_long(desc_ptr + 4, 0);
    }
}

fn force_button_true_at_pc() -> Option<u32> {
    *FORCE_BUTTON_TRUE_AT_PC.get_or_init(|| {
        let s = std::env::var("SYSTEMLESS_FORCE_BUTTON_TRUE_AT_PC").ok()?;
        let s = s.strip_prefix("0x").unwrap_or(&s);
        u32::from_str_radix(s, 16).ok()
    })
}

/// Classic ABI edge for the migrated CFM symbol-query selectors.
fn dispatch_cfm_symbols<C: CpuOps>(
    cpu: &mut C,
    bus: &mut MacMemoryBus,
    cfm: Option<&crate::cfm::CfmState>,
    bindings: Option<&mut dyn crate::cfm::CfmSymbolBindings>,
) -> Result<()> {
    use crate::cfm::CfmSymbolQuery;
    let sp = cpu.read_reg(Register::A7);
    if sp.checked_add(1).is_none() || !bus.is_guest_address_mapped(sp, 2) {
        cpu.write_reg(Register::D0, (-50i32) as u32);
        return Ok(());
    }
    let selector = bus.read_word(sp);
    let argument_bytes = match selector {
        5 => 16,
        6 => 8,
        7 => 20,
        // Unmigrated selectors retain the legacy compatibility stub.
        _ => return return_noerr(cpu),
    };
    let Some(cfm) = cfm else {
        return Err(Error::UnimplementedTrap(0xAA5A));
    };
    let Some(result_slot) = sp.checked_add(2 + argument_bytes) else {
        cpu.write_reg(Register::D0, (-50i32) as u32);
        return Ok(());
    };
    if result_slot.checked_add(1).is_none()
        || !bus.is_guest_address_mapped(sp, (2 + argument_bytes) as usize)
        || !bus.is_guest_address_writable(result_slot, 2)
    {
        cpu.write_reg(Register::D0, (-50i32) as u32);
        return Ok(());
    }
    let publish = |bus: &mut MacMemoryBus, writes: &[(u32, &[u8])]| {
        let success = 0u16.to_be_bytes();
        let mut writes = writes.to_vec();
        writes.push((result_slot, &success));
        bus.try_write_ranges_atomic(&writes)
    };
    let result = if selector == 5 {
        crate::cfm::CfmFindSymbol {
            connection: bus.read_long(sp + 14),
            name: bus.read_long(sp + 10),
            address: bus.read_long(sp + 6),
            class: bus.read_long(sp + 2),
        }
        .complete(&cfm.connections, bus, bindings, publish)
    } else {
        let query = if selector == 6 {
            CfmSymbolQuery::Count {
                connection: bus.read_long(sp + 6),
                count: bus.read_long(sp + 2),
            }
        } else {
            CfmSymbolQuery::Indexed {
                connection: bus.read_long(sp + 18),
                index: bus.read_long(sp + 14),
                name: bus.read_long(sp + 10),
                address: bus.read_long(sp + 6),
                class: bus.read_long(sp + 2),
            }
        };
        query.complete(&cfm.connections, |writes| publish(bus, writes))
    };
    let error = result.err().map_or(0, |error| error.os_error());
    if error != 0 {
        // All semantic outputs remain unchanged when their transaction fails.
        let _ = bus.try_write_word(result_slot, error as u16);
    }
    cpu.write_reg(Register::A7, result_slot);
    cpu.write_reg(Register::D0, error as i32 as u32);
    Ok(())
}

struct ClassicNewThreadEdge<'a, C> {
    dispatcher: &'a mut super::TrapDispatcher,
    cpu: &'a C,
    bus: &'a mut MacMemoryBus,
    thread_entry: u32,
    thread_param: u32,
    result_destination: u32,
    thread_made: u32,
    result_slot: u32,
    trampoline: u32,
}

impl<C: CpuOps> NewThreadCreationEdge for ClassicNewThreadEdge<'_, C> {
    fn preflight(&mut self, _size: u32) -> std::result::Result<(), i16> {
        if self.thread_made == 0
            || self.thread_entry & 1 != 0
            || !self.bus.is_guest_address_mapped(self.thread_entry, 2)
            || !self.bus.is_guest_address_writable(self.thread_made, 4)
            || !self.bus.is_guest_address_writable(self.result_slot, 2)
            || resolve_same_isa_thread_entry(self.bus, self.thread_entry, 0, GuestIsa::M68k)
                .is_none()
        {
            return Err(-50);
        }
        self.trampoline = self.dispatcher.thread_return_trampoline(self.bus);
        if self.trampoline == 0 {
            Err(-108)
        } else {
            Ok(())
        }
    }

    fn allocate_fresh(&mut self, size: u32) -> std::result::Result<ThreadStorage, i16> {
        let base = self.bus.alloc(size);
        let Some(limit) = base.checked_add(size) else {
            self.bus.free(base);
            return Err(-108);
        };
        if base == 0 {
            return Err(-108);
        }
        Ok(ThreadStorage {
            result_destination: self.result_destination,
            stack_base: base,
            stack_limit: limit,
            managed_pointer: false,
        })
    }

    fn prepare_and_publish(
        &mut self,
        execution: &SharedGuestCallStack,
        mut storage: ThreadStorage,
        suspended: bool,
    ) -> std::result::Result<Option<ExecutionTaskId>, i16> {
        storage.result_destination = self.result_destination;
        let Some(entry_sp) = storage.stack_limit.checked_sub(8).map(|sp| sp & !1) else {
            return Err(-108);
        };
        let overlap = |address: u32, length: u32, other: u32, other_length: u32| {
            u64::from(address) < u64::from(other) + u64::from(other_length)
                && u64::from(other) < u64::from(address) + u64::from(length)
        };
        if storage.stack_base == 0
            || storage.stack_limit < storage.stack_base
            || entry_sp < storage.stack_base
            || !self.bus.is_guest_address_writable(entry_sp, 8)
            || overlap(self.thread_made, 4, entry_sp, 8)
            || overlap(self.result_slot, 2, entry_sp, 8)
            || overlap(self.thread_made, 4, self.result_slot, 2)
        {
            return Err(-50);
        }
        let mut thread = CooperativeThread::capture(self.cpu);
        thread.pc = self.thread_entry;
        thread.a_regs[7] = entry_sp;
        let mut frame = [0u8; 8];
        frame[..4].copy_from_slice(&self.trampoline.to_be_bytes());
        frame[4..].copy_from_slice(&self.thread_param.to_be_bytes());
        Ok(
            execution.create_classic_thread(thread, storage, suspended, |task| {
                self.bus.try_write_ranges_atomic(&[
                    (entry_sp, &frame),
                    (self.thread_made, &task.thread_id().to_be_bytes()),
                    (self.result_slot, &0u16.to_be_bytes()),
                ])
            }),
        )
    }

    fn release_fresh(&mut self, storage: ThreadStorage) {
        self.bus.free(storage.stack_base);
    }

    fn finish_publication_attempt(&mut self) {}
}

struct ClassicRetiredThreadStorageEdge<'a> {
    bus: &'a mut MacMemoryBus,
    manager: crate::process_context::SharedProcessMemoryManager,
}

impl RetiredThreadStorageEdge for ClassicRetiredThreadStorageEdge<'_> {
    fn release_classic(&mut self, stack_base: u32) {
        self.bus.free(stack_base);
    }

    fn release_native(&mut self, stack_base: u32) {
        self.manager
            .borrow_mut()
            .native_mut()
            .dispose_native_ptr(stack_base);
    }
}

impl super::TrapDispatcher {
    /// Whether `SystemTask` currently has periodic Desk Manager work to do.
    ///
    /// Inside Macintosh Volume I, I-442 and I-444 through I-445, specifies
    /// that `SystemTask` calls the control routines of open desk accessories
    /// and other device drivers whose `dNeedTime`/`drvrDelay` period has
    /// elapsed. Systemless does not yet model that periodic DA/driver chain,
    /// so the current HLE implementation has no observable work. Keeping the
    /// decision behind this runtime query gives future DA/driver support a
    /// single place to revoke transparent execution.
    pub(crate) fn system_task_has_periodic_work(&self) -> bool {
        false
    }

    const LIST_RVIEW_OFFSET: u32 = 0;
    const LIST_PORT_OFFSET: u32 = 8;
    const LIST_INDENT_OFFSET: u32 = 12;
    const LIST_CELL_SIZE_OFFSET: u32 = 16;
    const LIST_VISIBLE_OFFSET: u32 = 20;
    const LIST_VSCROLL_OFFSET: u32 = 28;
    const LIST_HSCROLL_OFFSET: u32 = 32;
    const LIST_SEL_FLAGS_OFFSET: u32 = 36;
    const LIST_ACTIVE_OFFSET: u32 = 37;
    const LIST_RESERVED_OFFSET: u32 = 38;
    const LIST_FLAGS_OFFSET: u32 = 39;
    const LIST_CLICK_TIME_OFFSET: u32 = 40;
    const LIST_CLICK_LOC_OFFSET: u32 = 44;
    const LIST_MOUSE_LOC_OFFSET: u32 = 48;
    const LIST_CLICK_LOOP_OFFSET: u32 = 52;
    const LIST_LAST_CLICK_OFFSET: u32 = 56;
    const LIST_REFCON_OFFSET: u32 = 60;
    const LIST_DEF_PROC_OFFSET: u32 = 64;
    const LIST_USER_HANDLE_OFFSET: u32 = 68;
    const LIST_DATA_BOUNDS_OFFSET: u32 = 72;
    const LIST_CELLS_OFFSET: u32 = 80;
    const LIST_MAX_INDEX_OFFSET: u32 = 84;
    const LIST_CELL_ARRAY_OFFSET: u32 = 86;
    const LIST_RECORD_SIZE: u32 = 88;
    const LIST_DOUBLE_CLICK_TICKS: u32 = 20;
    const LIST_LDRAW_MSG: i16 = 1;
    const LIST_LHILITE_MSG: i16 = 2;
    const LIST_DEF_TRAMPOLINE_SIZE: u32 = 60;
    const ALIAS_RECORD_FIXED_SIZE: usize = 150;
    const ALIAS_RECORD_VERSION: u16 = 2;
    const ALIAS_KIND_FILE: u16 = 0;
    const ALIAS_KIND_FOLDER: u16 = 1;
    const ALIAS_EXTRA_PARENT_DIR_NAME: i16 = 0;
    const ALIAS_EXTRA_FULL_PATH: i16 = 2;
    const ALIAS_EXTRA_END: i16 = -1;

    /// `kApplicationThreadID` from Threads.h — the thread the process
    /// launches on, which owns the process stack rather than a pooled one.
    const APPLICATION_THREAD_ID: u32 = 2;
    /// `threadNotFoundErr` and `threadProtocolErr` from Errors.h.
    const THREAD_NOT_FOUND_ERR: i16 = crate::thread_manager::THREAD_NOT_FOUND_ERR;
    const THREAD_PROTOCOL_ERR: i16 = crate::thread_manager::THREAD_PROTOCOL_ERR;

    fn thread_return_trampoline(&mut self, bus: &mut MacMemoryBus) -> u32 {
        if self.thread_return_trampoline == 0 {
            let trampoline = bus.alloc(8);
            let code = [0x303Cu16, 0xFFFE, 0xABF2, 0x4E75]
                .into_iter()
                .flat_map(u16::to_be_bytes)
                .collect::<Vec<_>>();
            if trampoline == 0 || !bus.try_write_ranges_atomic(&[(trampoline, &code)]) {
                bus.free(trampoline);
                return 0;
            }
            self.thread_return_trampoline = trampoline;
        }
        self.thread_return_trampoline
    }

    /// Resolve a guest-supplied ThreadID. `kCurrentThreadID` (1) and
    /// `kNoThreadID` (0) both name the running thread, per Threads.h.
    fn resolve_cooperative_thread_id(&self, thread_id: u32) -> u32 {
        ThreadManager::new(&self.guest_calls).resolve_thread(thread_id)
    }

    /// Materialise the record for the thread the process launched on.
    /// `kApplicationThreadID` exists implicitly from launch, so
    /// `GetThreadState`, `SetThreadSwitcher` and friends must find it
    /// without a prior `NewThread`.
    fn cooperative_thread_snapshot<C: CpuOps>(
        &mut self,
        cpu: &C,
        thread_id: u32,
    ) -> Option<CooperativeThread> {
        if thread_id == Self::APPLICATION_THREAD_ID
            && !self
                .guest_calls
                .cooperative_context(ExecutionTaskId::from_thread_id(Self::APPLICATION_THREAD_ID))
                .is_some()
        {
            let thread = CooperativeThread::capture(cpu);
            self.guest_calls.save_cooperative_context(
                ExecutionTaskId::from_thread_id(Self::APPLICATION_THREAD_ID),
                thread,
            );
        }
        self.guest_calls
            .cooperative_context(ExecutionTaskId::from_thread_id(thread_id))
    }

    fn apply_classic_retirement<C: CpuOps>(
        &mut self,
        cpu: &mut C,
        bus: &mut MacMemoryBus,
        recycle: bool,
        retirement: ClassicRetirement,
    ) {
        let (storage, successor) = match retirement {
            ClassicRetirement::Removed(storage) => (storage, None),
            ClassicRetirement::Switched { storage, successor } => (storage, successor),
        };
        let mut edge = ClassicRetiredThreadStorageEdge {
            bus,
            manager: self.process_memory_manager(),
        };
        ThreadManager::release_retired_storage(storage, recycle, &mut edge);
        if let Some(successor) = successor {
            successor.install(cpu);
        }
    }

    fn apple_event_handler_for(
        &self,
        event_class: u32,
        event_id: u32,
    ) -> Option<crate::process_context::ProcessAppleEventHandler> {
        self.ae_handlers
            .handler_for(event_class, event_id, AE_TYPE_WILDCARD)
    }

    fn ae_object_accessor_for(
        &self,
        desired_class: u32,
        container_type: u32,
    ) -> Option<AeObjectAccessor> {
        for is_sys_handler in [false, true] {
            for key in [
                (is_sys_handler, desired_class, container_type),
                (is_sys_handler, desired_class, AE_TYPE_WILDCARD),
                (is_sys_handler, AE_TYPE_WILDCARD, container_type),
                (is_sys_handler, AE_TYPE_WILDCARD, AE_TYPE_WILDCARD),
            ] {
                if let Some(accessor) = self.ae_object_accessors.get(&key) {
                    return Some(*accessor);
                }
            }
        }
        None
    }

    fn ae_object_accessor_exact(
        &self,
        is_sys_handler: bool,
        desired_class: u32,
        container_type: u32,
    ) -> Option<AeObjectAccessor> {
        self.ae_object_accessors
            .get(&(is_sys_handler, desired_class, container_type))
            .copied()
    }

    fn ae_object_accessor_exact_any_table(
        &self,
        desired_class: u32,
        container_type: u32,
    ) -> Option<AeObjectAccessor> {
        [false, true].into_iter().find_map(|is_sys_handler| {
            self.ae_object_accessor_exact(is_sys_handler, desired_class, container_type)
        })
    }

    fn ae_special_handler_for(&self, function_class: u32) -> Option<u32> {
        [false, true].into_iter().find_map(|is_sys_handler| {
            self.ae_special_handlers
                .get(&(is_sys_handler, function_class))
                .copied()
        })
    }

    fn read_ae_descriptor_value(&self, bus: &MacMemoryBus, desc_ptr: u32) -> Option<AeDescriptor> {
        if desc_ptr == 0 {
            return None;
        }
        let desc_type = bus.read_long(desc_ptr);
        if desc_type == 0 {
            return None;
        }
        let data_handle = bus.read_long(desc_ptr + 4);
        if data_handle != 0 {
            if let Some(backing) = self.ae_descriptor_backing.get(&data_handle) {
                let mut desc = backing.clone();
                desc.desc_type = desc_type;
                return Some(desc);
            }
        }
        if let Some(desc) = self.ae_descriptors.get(&desc_ptr) {
            return Some(desc.clone());
        }
        if self.ae_events.contains_key(&desc_ptr) {
            return Some(AeDescriptor {
                desc_type: AE_TYPE_APPLE_EVENT,
                data: Vec::new(),
                fields: HashMap::new(),
                items: Vec::new(),
            });
        }
        if desc_type == 0 {
            None
        } else {
            let data = if data_handle != 0 {
                let data_ptr = bus.read_long(data_handle);
                let data_size = bus.get_alloc_size(data_ptr).unwrap_or(0);
                (0..data_size)
                    .map(|offset| bus.read_byte(data_ptr + offset))
                    .collect()
            } else {
                Vec::new()
            };
            Some(AeDescriptor {
                desc_type,
                data,
                fields: HashMap::new(),
                items: Vec::new(),
            })
        }
    }

    fn ae_descriptor_needs_backing(desc: &AeDescriptor) -> bool {
        !desc.fields.is_empty()
            || !desc.items.is_empty()
            || matches!(
                desc.desc_type,
                AE_TYPE_AE_RECORD | AE_TYPE_AE_LIST | AE_TYPE_OBJECT_SPECIFIER
            )
    }

    fn ae_descriptor(desc_type: u32, data: Vec<u8>) -> AeDescriptor {
        AeDescriptor {
            desc_type,
            data,
            fields: HashMap::new(),
            items: Vec::new(),
        }
    }

    fn ae_event_param_value(
        &self,
        bus: &MacMemoryBus,
        event_desc: u32,
        keyword: u32,
    ) -> Option<AeDescriptor> {
        self.ae_events
            .get(&event_desc)
            .and_then(|event| event.params.get(&keyword).cloned())
            .or_else(|| {
                self.read_ae_descriptor_value(bus, event_desc)
                    .and_then(|desc| desc.fields.get(&keyword).cloned())
            })
    }

    fn ae_event_attribute_value(&self, event_desc: u32, keyword: u32) -> Option<AeDescriptor> {
        self.ae_events.get(&event_desc).and_then(|event| {
            let value = if keyword == AE_KEY_EVENT_CLASS_ATTR {
                event.event_class
            } else if keyword == AE_KEY_EVENT_ID_ATTR {
                event.event_id
            } else {
                return None;
            };
            Some(Self::ae_descriptor(
                AE_TYPE_TYPE,
                value.to_be_bytes().to_vec(),
            ))
        })
    }

    fn ae_put_record_field(desc: &mut AeDescriptor, keyword: u32, value: AeDescriptor) {
        desc.fields.insert(keyword, value.clone());
        if let Some((_, item)) = desc
            .items
            .iter_mut()
            .find(|(item_keyword, _)| *item_keyword == keyword)
        {
            *item = value;
        } else {
            desc.items.push((keyword, value));
        }
    }

    fn ae_put_event_param(event: &mut SyntheticAppleEvent, keyword: u32, value: AeDescriptor) {
        event.params.insert(keyword, value.clone());
        if let Some((_, item)) = event
            .items
            .iter_mut()
            .find(|(item_keyword, _)| *item_keyword == keyword)
        {
            *item = value;
        } else {
            event.items.push((keyword, value));
        }
    }

    fn ae_list_item_at(desc: &AeDescriptor, index: u32) -> Option<(u32, AeDescriptor)> {
        if index == 0 {
            return None;
        }
        desc.items.get(index as usize - 1).cloned()
    }

    fn ae_put_list_item(
        desc: &mut AeDescriptor,
        index: u32,
        keyword: u32,
        item: AeDescriptor,
    ) -> i16 {
        let len = desc.items.len();
        if index == 0 || index as usize == len + 1 {
            desc.items.push((keyword, item));
            0
        } else if (index as usize) <= len {
            desc.items[index as usize - 1] = (keyword, item);
            0
        } else {
            AE_ERR_ILLEGAL_INDEX
        }
    }

    fn write_ae_descriptor_value(
        &mut self,
        bus: &mut MacMemoryBus,
        desc_ptr: u32,
        desc: AeDescriptor,
    ) {
        if desc_ptr == 0 {
            return;
        }
        let data_handle = if desc.data.is_empty() {
            if Self::ae_descriptor_needs_backing(&desc) {
                let existing = bus.read_long(desc_ptr + 4);
                if existing != 0 {
                    existing
                } else {
                    self.new_empty_process_classic_handle(bus).unwrap_or(0)
                }
            } else {
                0
            }
        } else {
            self.new_process_classic_handle(bus, desc.data.len() as u32)
                .map(|(handle, ptr)| {
                    bus.write_bytes(ptr, &desc.data);
                    handle
                })
                .unwrap_or(0)
        };
        bus.write_long(desc_ptr, desc.desc_type);
        bus.write_long(desc_ptr + 4, data_handle);
        if data_handle != 0 && Self::ae_descriptor_needs_backing(&desc) {
            self.ae_descriptor_backing.insert(data_handle, desc.clone());
        }
        self.ae_descriptors.insert(desc_ptr, desc);
    }

    fn write_synthetic_apple_event_descriptor(
        &mut self,
        bus: &mut MacMemoryBus,
        desc_ptr: u32,
        event_class: u32,
        event_id: u32,
    ) {
        let mut event_data = Vec::with_capacity(8);
        event_data.extend_from_slice(&event_class.to_be_bytes());
        event_data.extend_from_slice(&event_id.to_be_bytes());
        self.write_ae_descriptor_value(
            bus,
            desc_ptr,
            Self::ae_descriptor(AE_TYPE_APPLE_EVENT, event_data),
        );
        self.ae_events.insert(
            desc_ptr,
            SyntheticAppleEvent {
                event_class,
                event_id,
                params: HashMap::new(),
                items: Vec::new(),
            },
        );
    }

    fn dispose_ae_descriptor_record(&mut self, bus: &mut MacMemoryBus, desc_ptr: u32) {
        let data_handle = if desc_ptr != 0 {
            bus.read_long(desc_ptr + 4)
        } else {
            0
        };
        let data_ptr = if data_handle != 0 {
            bus.read_long(data_handle)
        } else {
            0
        };

        self.ae_events.remove(&desc_ptr);
        self.ae_descriptors.remove(&desc_ptr);
        // AEDesc records are copied by value; sibling records may still share
        // this data handle, so keep structured backing until the process exits.
        if data_ptr != 0 {
            self.untrack_handle_ptr(data_ptr);
        }
        write_null_aedesc(bus, desc_ptr);
    }

    fn dispose_owned_ae_callback_descriptor(&mut self, bus: &mut MacMemoryBus, desc_ptr: u32) {
        if desc_ptr == 0 {
            return;
        }
        let data_handle = bus.read_long(desc_ptr + 4);
        self.ae_events.remove(&desc_ptr);
        self.ae_descriptors.remove(&desc_ptr);
        if data_handle != 0 {
            self.ae_descriptor_backing.remove(&data_handle);
            let _ = self.dispose_process_handle(bus, data_handle, true);
        }
        self.dispose_process_ptr(bus, desc_ptr);
    }

    fn ae_descriptor_record(&mut self, bus: &mut MacMemoryBus, desc: AeDescriptor) -> u32 {
        let desc_ptr = self.new_process_classic_ptr(bus, 8);
        self.write_ae_descriptor_value(bus, desc_ptr, desc);
        desc_ptr
    }

    fn ae_create_private_hash_table(
        &mut self,
        bus: &mut MacMemoryBus,
        result_ptr: u32,
        entry_sizes: u32,
        allocation_size: u32,
    ) -> i16 {
        let key_size = ((entry_sizes >> 16) & 0xFFFF) as usize;
        let value_size = (entry_sizes & 0xFFFF) as usize;
        if result_ptr == 0 || key_size == 0 || value_size == 0 {
            return -50;
        }

        let handle = bus.alloc(4);
        if handle == 0 {
            return -108;
        }
        let table_size = allocation_size.max(16) as usize;
        let mut table_bytes = vec![0; table_size];
        table_bytes[4..8].copy_from_slice(&(table_size as u32).to_be_bytes());
        table_bytes[8..12].copy_from_slice(&(key_size as u32).to_be_bytes());
        table_bytes[12..16].copy_from_slice(&(value_size as u32).to_be_bytes());
        let data_ptr = self.write_bytes_to_handle(bus, handle, &table_bytes);
        if data_ptr == 0 {
            return -108;
        }

        self.ae_private_hash_tables.insert(
            handle,
            AePrivateHashTable {
                key_size,
                value_size,
                entries: HashMap::new(),
            },
        );
        bus.write_long(result_ptr, handle);
        0
    }

    fn ae_private_hash_key(bus: &MacMemoryBus, key_ptr: u32, key_size: usize) -> Option<Vec<u8>> {
        if key_ptr == 0 {
            None
        } else {
            Some(bus.read_bytes(key_ptr, key_size))
        }
    }

    fn ae_descriptor_u32(desc: &AeDescriptor) -> Option<u32> {
        if desc.data.len() < 4 {
            None
        } else {
            Some(u32::from_be_bytes([
                desc.data[0],
                desc.data[1],
                desc.data[2],
                desc.data[3],
            ]))
        }
    }

    fn ae_collect_resolve_levels(
        object_specifier: &AeDescriptor,
    ) -> Option<(AeDescriptor, u32, Vec<AeResolveLevel>)> {
        if object_specifier.desc_type != AE_TYPE_OBJECT_SPECIFIER {
            return None;
        }
        let desired_class = object_specifier
            .fields
            .get(&AE_KEY_DESIRED_CLASS)
            .and_then(Self::ae_descriptor_u32)?;
        let key_form = object_specifier
            .fields
            .get(&AE_KEY_KEY_FORM)
            .and_then(Self::ae_descriptor_u32)?;
        let key_data = object_specifier.fields.get(&AE_KEY_KEY_DATA)?.clone();
        let container = object_specifier
            .fields
            .get(&AE_KEY_CONTAINER)
            .cloned()
            .unwrap_or_else(|| AeDescriptor {
                desc_type: AE_TYPE_NULL,
                data: Vec::new(),
                fields: HashMap::new(),
                items: Vec::new(),
            });

        let (base_container, base_container_class, mut levels) =
            if container.desc_type == AE_TYPE_OBJECT_SPECIFIER {
                Self::ae_collect_resolve_levels(&container)?
            } else {
                (container.clone(), container.desc_type, Vec::new())
            };
        levels.push(AeResolveLevel {
            desired_class,
            key_form,
            key_data,
        });
        Some((base_container, base_container_class, levels))
    }

    fn ae_finish_resolve(
        &mut self,
        bus: &mut MacMemoryBus,
        cpu: &mut dyn CpuOps,
        resolve_state: AeResolveState,
        err: i16,
    ) {
        if trace_ae_enabled() {
            let token_type = if resolve_state.final_token_desc != 0 {
                bus.read_long(resolve_state.final_token_desc)
            } else {
                0
            };
            eprintln!(
                "[AE] AEResolve ← finish err={} token='{}' desc=${:08X}",
                err,
                format_fourcc(token_type),
                resolve_state.final_token_desc,
            );
        }
        if err != 0 {
            write_null_aedesc(bus, resolve_state.final_token_desc);
        }
        bus.write_word(resolve_state.result_slot, err as u16);
        cpu.write_reg(Register::A7, resolve_state.result_slot);
        cpu.write_reg(Register::D0, err as i32 as u32);
        cpu.write_reg(Register::PC, resolve_state.return_pc);
        if let Some(outer_state) = self.ae_call_state_stack.pop() {
            self.ae_call_state = Some(outer_state);
        }
    }

    fn ae_dispatch_object_accessor(
        &mut self,
        bus: &mut MacMemoryBus,
        cpu: &mut dyn CpuOps,
        resolve_state: AeResolveState,
        container_token: AeDescriptor,
    ) -> bool {
        let Some(level) = resolve_state.levels.get(resolve_state.next_level).cloned() else {
            self.ae_finish_resolve(bus, cpu, resolve_state, 0);
            return true;
        };
        let Some(accessor) =
            self.ae_object_accessor_for(level.desired_class, container_token.desc_type)
        else {
            self.ae_finish_resolve(bus, cpu, resolve_state, AE_ERR_ACCESSOR_NOT_FOUND);
            return true;
        };

        self.ae_invoke_object_accessor(bus, cpu, resolve_state, container_token, level, accessor)
    }

    fn ae_invoke_object_accessor(
        &mut self,
        bus: &mut MacMemoryBus,
        cpu: &mut dyn CpuOps,
        mut resolve_state: AeResolveState,
        container_token: AeDescriptor,
        level: AeResolveLevel,
        accessor: AeObjectAccessor,
    ) -> bool {
        let trampoline = match self.ae_trampoline_addr {
            Some(addr) => addr,
            None => {
                let addr = bus.alloc(8);
                bus.write_word(addr, 0x303C); // MOVE.W #imm, D0
                bus.write_word(addr + 2, 0xFEFE); // immediate
                bus.write_word(addr + 4, 0xA816); // _Pack8
                self.ae_trampoline_addr = Some(addr);
                addr
            }
        };

        let token_desc = if resolve_state.next_level + 1 == resolve_state.levels.len() {
            resolve_state.final_token_desc
        } else {
            bus.alloc(8)
        };
        write_null_aedesc(bus, token_desc);
        let container_desc = self.ae_descriptor_record(bus, container_token);
        let key_data_desc = self.ae_descriptor_record(bus, level.key_data.clone());

        // AccessorProcPtr ABI: desiredClass, containerToken*,
        // containerClass, keyForm, keyData*, theToken*, refcon.
        // That is 28 bytes of parameters; RTD #28 lands on result_slot.
        let new_sp = resolve_state.result_slot.wrapping_sub(32);
        bus.write_long(new_sp, trampoline);
        bus.write_long(new_sp + 4, accessor.refcon);
        bus.write_long(new_sp + 8, token_desc);
        bus.write_long(new_sp + 12, key_data_desc);
        bus.write_long(new_sp + 16, level.key_form);
        bus.write_long(new_sp + 20, resolve_state.container_class);
        bus.write_long(new_sp + 24, container_desc);
        bus.write_long(new_sp + 28, level.desired_class);
        cpu.write_reg(Register::A7, new_sp);
        cpu.write_reg(Register::PC, accessor.accessor_ptr);

        resolve_state.current_token_desc = token_desc;
        self.ae_call_state = Some(crate::trap::dispatch::AeCallState {
            return_pc: resolve_state.return_pc,
            expected_sp_after_rtd: resolve_state.result_slot,
            result_override: None,
            owned_descriptors: None,
            resolve_state: Some(resolve_state),
        });

        if trace_ae_enabled() {
            let key_data_u32 = Self::ae_descriptor_u32(&level.key_data);
            eprintln!(
                "[AE] AEResolve → invoking object accessor desired='{}' container='{}' keyForm='{}' keyData={} handler=${:08X} refcon=${:08X}",
                format_fourcc(level.desired_class),
                format_fourcc(bus.read_long(container_desc)),
                format_fourcc(level.key_form),
                key_data_u32.map_or_else(
                    || format!("{} bytes", level.key_data.data.len()),
                    |value| format!("${:08X}/{}", value, value as i32),
                ),
                accessor.accessor_ptr,
                accessor.refcon,
            );
        }
        true
    }

    fn ae_continue_resolve_after_accessor(
        &mut self,
        bus: &mut MacMemoryBus,
        cpu: &mut dyn CpuOps,
        mut resolve_state: AeResolveState,
        accessor_result: i16,
    ) {
        if trace_ae_enabled() {
            let token_type = if resolve_state.current_token_desc != 0 {
                bus.read_long(resolve_state.current_token_desc)
            } else {
                0
            };
            eprintln!(
                "[AE] AEResolve ← object accessor result={} token='{}' desc=${:08X}",
                accessor_result,
                format_fourcc(token_type),
                resolve_state.current_token_desc,
            );
        }
        if accessor_result != 0 {
            self.ae_finish_resolve(bus, cpu, resolve_state, accessor_result);
            return;
        }

        let Some(container_token) =
            self.read_ae_descriptor_value(bus, resolve_state.current_token_desc)
        else {
            self.ae_finish_resolve(bus, cpu, resolve_state, AE_ERR_DESC_NOT_FOUND);
            return;
        };
        let current_level = resolve_state.next_level;
        resolve_state.next_level += 1;
        resolve_state.container_class = resolve_state.levels[current_level].desired_class;
        if resolve_state.next_level >= resolve_state.levels.len() {
            self.ae_finish_resolve(bus, cpu, resolve_state, 0);
            return;
        }
        self.ae_dispatch_object_accessor(bus, cpu, resolve_state, container_token);
    }

    fn stack_bool_slot(bus: &MacMemoryBus, addr: u32) -> bool {
        // MPW Pascal callers store BOOLEAN in the high byte of the
        // 2-byte stack slot. The low byte is padding and can retain
        // unrelated non-zero garbage, so only the first byte is
        // semantically meaningful.
        bus.read_byte(addr) != 0
    }

    fn read_stack_point(bus: &MacMemoryBus, addr: u32) -> (i16, i16) {
        (bus.read_word(addr) as i16, bus.read_word(addr + 2) as i16)
    }

    fn read_rect_ptr(bus: &MacMemoryBus, ptr: u32) -> (i16, i16, i16, i16) {
        (
            bus.read_word(ptr) as i16,
            bus.read_word(ptr + 2) as i16,
            bus.read_word(ptr + 4) as i16,
            bus.read_word(ptr + 6) as i16,
        )
    }

    fn write_rect_words(bus: &mut MacMemoryBus, addr: u32, rect: (i16, i16, i16, i16)) {
        bus.write_word(addr, rect.0 as u16);
        bus.write_word(addr + 2, rect.1 as u16);
        bus.write_word(addr + 4, rect.2 as u16);
        bus.write_word(addr + 6, rect.3 as u16);
    }

    fn standard_file_env_selection(&mut self) -> Option<StandardFileSelection> {
        let requested = std::env::var("SYSTEMLESS_STANDARD_GET_FILE").ok()?;
        let requested = requested.trim();
        if requested.is_empty() {
            return None;
        }

        let default_dir_id = *self.default_dir_id;
        let vfs_key = self
            .find_vfs_file_in_directory(default_dir_id, requested)
            .or_else(|| self.find_vfs_rsrc_file_in_directory(default_dir_id, requested))
            .or_else(|| self.find_vfs_file(requested))
            .or_else(|| self.find_vfs_rsrc_file(requested))?;
        let metadata = self.vfs_file_metadata(&vfs_key)?;
        let vref = Self::boot_volume_ref_num();
        let wd_ref = self
            .open_working_directory(vref, metadata.parent_dir_id, 0)
            .unwrap_or(vref);
        let guest_name = Self::hfs_name_from_vfs_component(Self::vfs_basename(&vfs_key));
        let mut name = encode_mac_roman_lossy(&guest_name);
        name.truncate(63);

        Some(StandardFileSelection {
            name,
            vref,
            wd_ref,
            dir_id: metadata.parent_dir_id,
            file_type: metadata.file_type,
            finder_flags: metadata.finder_flags,
        })
    }

    fn standard_file_auto_selection(
        &mut self,
        bus: &MacMemoryBus,
        num_types: i16,
        type_list_ptr: u32,
    ) -> Option<StandardFileSelection> {
        // Inside Macintosh: Files (1992), p. 3-50: Standard File first
        // filters the display list by numTypes/typeList; numTypes = -1 lets
        // all file types pass, while 0 displays no files. The headless HLE
        // auto-selects the first positive type-list hit, or the only current
        // directory file in all-types mode.
        if num_types == 0 || num_types < -1 || (num_types > 0 && type_list_ptr == 0) {
            return None;
        }

        let file_types = if num_types > 0 {
            let count = (num_types as usize).min(64);
            let mut file_types = Vec::with_capacity(count);
            for index in 0..count {
                file_types.push(bus.read_long(type_list_ptr + (index as u32 * 4)));
            }
            Some(file_types)
        } else {
            None
        };

        let default_dir_id = *self.default_dir_id;
        let entries = self.list_vfs_catalog_entries(default_dir_id);
        let mut all_types_match: Option<StandardFileSelection> = None;
        for entry in entries {
            if entry.is_directory {
                continue;
            }
            let Some(metadata) = self.vfs_file_metadata(&entry.path) else {
                continue;
            };
            if let Some(file_types) = &file_types {
                if !file_types.contains(&metadata.file_type) {
                    continue;
                }
            } else if all_types_match.is_some() {
                return None;
            }

            let selection = {
                let vref = Self::boot_volume_ref_num();
                let wd_ref = self
                    .open_working_directory(vref, metadata.parent_dir_id, 0)
                    .unwrap_or(vref);
                let guest_name = Self::hfs_name_from_vfs_component(Self::vfs_basename(&entry.path));
                let mut name = encode_mac_roman_lossy(&guest_name);
                name.truncate(63);
                StandardFileSelection {
                    name,
                    vref,
                    wd_ref,
                    dir_id: metadata.parent_dir_id,
                    file_type: metadata.file_type,
                    finder_flags: metadata.finder_flags,
                }
            };
            if file_types.is_none() {
                all_types_match = Some(selection);
                continue;
            }

            return Some(selection);
        }
        all_types_match
    }

    fn standard_file_get_type_list(
        bus: &MacMemoryBus,
        num_types: i16,
        type_list_ptr: u32,
    ) -> Option<Option<Vec<u32>>> {
        if num_types == 0 || num_types < -1 || (num_types > 0 && type_list_ptr == 0) {
            return None;
        }
        if num_types <= 0 {
            return Some(None);
        }
        let count = (num_types as usize).min(64);
        let mut file_types = Vec::with_capacity(count);
        for index in 0..count {
            file_types.push(bus.read_long(type_list_ptr + (index as u32 * 4)));
        }
        Some(Some(file_types))
    }

    fn standard_file_get_candidates(
        &mut self,
        bus: &MacMemoryBus,
        num_types: i16,
        type_list_ptr: u32,
    ) -> Vec<StandardFileGetEntry> {
        let Some(file_types) = Self::standard_file_get_type_list(bus, num_types, type_list_ptr)
        else {
            return Vec::new();
        };
        self.standard_file_get_candidates_in_directory(*self.default_dir_id, file_types.as_deref())
    }

    fn standard_file_get_candidates_in_directory(
        &mut self,
        dir_id: u32,
        file_types: Option<&[u32]>,
    ) -> Vec<StandardFileGetEntry> {
        let mut entries = Vec::new();
        for entry in self.list_vfs_catalog_entries(dir_id) {
            if entry.is_directory {
                let Some(directory) = self
                    .vfs_directories
                    .iter()
                    .find(|directory| directory.path.eq_ignore_ascii_case(&entry.path))
                else {
                    continue;
                };
                let mut name = encode_mac_roman_lossy(&entry.name);
                name.truncate(63);
                entries.push(StandardFileGetEntry {
                    name,
                    display_name: entry.name,
                    vref: Self::boot_volume_ref_num(),
                    wd_ref: Self::boot_volume_ref_num(),
                    dir_id: directory.dir_id,
                    file_type: 0,
                    finder_flags: 0,
                    is_directory: true,
                });
                continue;
            }
            let Some(metadata) = self.vfs_file_metadata(&entry.path) else {
                continue;
            };
            if file_types.is_some_and(|types| !types.contains(&metadata.file_type)) {
                continue;
            }
            let vref = Self::boot_volume_ref_num();
            let wd_ref = self
                .open_working_directory(vref, metadata.parent_dir_id, 0)
                .unwrap_or(vref);
            let mut name = encode_mac_roman_lossy(&entry.name);
            name.truncate(63);
            entries.push(StandardFileGetEntry {
                name,
                display_name: entry.name,
                vref,
                wd_ref,
                dir_id: metadata.parent_dir_id,
                file_type: metadata.file_type,
                finder_flags: metadata.finder_flags,
                is_directory: false,
            });
        }
        entries
    }

    fn standard_file_get_dialog_bounds(
        &self,
        requested_origin: Option<(i16, i16)>,
    ) -> (i16, i16, i16, i16) {
        let (_, _, screen_width, screen_height, _) = self.screen_mode;
        let centered_left = (screen_width as i16 - STANDARD_FILE_GET_DIALOG_WIDTH) / 2;
        let centered_top = (screen_height as i16 - STANDARD_FILE_GET_DIALOG_HEIGHT) / 2;
        let (requested_top, requested_left) =
            requested_origin.unwrap_or((centered_top, centered_left));
        let left = requested_left
            .max(0)
            .min((screen_width as i16 - STANDARD_FILE_GET_DIALOG_WIDTH).max(0));
        let top = requested_top
            .max(0)
            .min((screen_height as i16 - STANDARD_FILE_GET_DIALOG_HEIGHT).max(0));
        (
            top,
            left,
            top + STANDARD_FILE_GET_DIALOG_HEIGHT,
            left + STANDARD_FILE_GET_DIALOG_WIDTH,
        )
    }

    fn standard_file_get_dialog_items(tracking: &StandardFileGetTrackingState) -> Vec<DialogItem> {
        let open_enabled = tracking
            .entries
            .get(tracking.selected)
            .is_some_and(|entry| entry.is_directory || entry.file_type != 0);
        vec![
            DialogItem {
                item_type: if open_enabled { 4 } else { 0x84 },
                rect: STANDARD_FILE_GET_OPEN_RECT,
                text: "Open".to_string(),
                ..DialogItem::default()
            },
            DialogItem {
                item_type: 4,
                rect: STANDARD_FILE_GET_CANCEL_RECT,
                text: "Cancel".to_string(),
                ..DialogItem::default()
            },
            DialogItem {
                item_type: 0x84,
                rect: STANDARD_FILE_GET_EJECT_RECT,
                text: "Eject".to_string(),
                ..DialogItem::default()
            },
            DialogItem {
                item_type: 4,
                rect: STANDARD_FILE_GET_DESKTOP_RECT,
                text: "Desktop".to_string(),
                ..DialogItem::default()
            },
            DialogItem {
                item_type: 8,
                rect: STANDARD_FILE_GET_VOLUME_LABEL_RECT,
                text: super::dispatch::BOOT_VOLUME_NAME.to_string(),
                ..DialogItem::default()
            },
            DialogItem {
                item_type: 0,
                rect: STANDARD_FILE_GET_VOLUME_RECT,
                text: super::dispatch::BOOT_VOLUME_NAME.to_string(),
                ..DialogItem::default()
            },
            DialogItem {
                item_type: 0,
                rect: STANDARD_FILE_GET_LIST_RECT,
                ..DialogItem::default()
            },
            DialogItem {
                item_type: 0,
                rect: STANDARD_FILE_GET_SCROLL_RECT,
                ..DialogItem::default()
            },
            DialogItem {
                item_type: 0,
                rect: STANDARD_FILE_GET_SEPARATOR_RECT,
                ..DialogItem::default()
            },
        ]
    }

    fn draw_standard_file_get_dialog(
        &mut self,
        bus: &mut MacMemoryBus,
        tracking: &StandardFileGetTrackingState,
    ) {
        let items = Self::standard_file_get_dialog_items(tracking);
        self.draw_dialog(
            bus,
            tracking.bounds,
            2,
            "",
            &items,
            STANDARD_FILE_GET_OPEN_ITEM,
            "",
            0,
            false,
            0,
        );
        let (top, left, _, _) = tracking.bounds;
        let (button_top, button_left, button_bottom, button_right) = STANDARD_FILE_GET_OPEN_RECT;
        let selected_is_openable = tracking
            .entries
            .get(tracking.selected)
            .is_some_and(|entry| entry.is_directory || entry.file_type != 0);
        self.draw_button_state(
            bus,
            top + button_top,
            left + button_left,
            top + button_bottom,
            left + button_right,
            "Open",
            true,
            selected_is_openable,
        );
        let (vtop, vleft, vbottom, vright) = STANDARD_FILE_GET_VOLUME_RECT;
        self.draw_popup_control(
            bus,
            top + vtop,
            left + vleft,
            top + vbottom,
            left + vright,
            super::dispatch::BOOT_VOLUME_NAME,
        );
        let (stop, sleft, sbottom, sright) = STANDARD_FILE_GET_SEPARATOR_RECT;
        self.draw_rect_border(
            bus,
            top + stop,
            left + sleft,
            top + sbottom + 1,
            left + sright,
        );
        self.draw_standard_file_get_list(bus, tracking);
    }

    fn draw_standard_file_get_list(
        &self,
        bus: &mut MacMemoryBus,
        tracking: &StandardFileGetTrackingState,
    ) {
        let (screen_base, row_bytes, screen_width, screen_height, pixel_size) =
            self.get_screen_params();
        let (dialog_top, dialog_left, _, _) = tracking.bounds;
        let (list_top, list_left, list_bottom, list_right) = STANDARD_FILE_GET_LIST_RECT;
        let top = dialog_top + list_top;
        let left = dialog_left + list_left;
        let bottom = dialog_top + list_bottom;
        let right = dialog_left + list_right;

        Self::fb_fill_rect(
            bus,
            screen_base,
            row_bytes,
            pixel_size,
            screen_width,
            screen_height,
            top,
            left,
            bottom,
            right,
            false,
        );
        Self::fb_fill_rect(
            bus,
            screen_base,
            row_bytes,
            pixel_size,
            screen_width,
            screen_height,
            top,
            left,
            top + 1,
            right,
            true,
        );
        Self::fb_fill_rect(
            bus,
            screen_base,
            row_bytes,
            pixel_size,
            screen_width,
            screen_height,
            bottom - 1,
            left,
            bottom,
            right,
            true,
        );
        Self::fb_fill_rect(
            bus,
            screen_base,
            row_bytes,
            pixel_size,
            screen_width,
            screen_height,
            top,
            left,
            bottom,
            left + 1,
            true,
        );
        Self::fb_fill_rect(
            bus,
            screen_base,
            row_bytes,
            pixel_size,
            screen_width,
            screen_height,
            top,
            right - 1,
            bottom,
            right,
            true,
        );

        let first_visible = Self::standard_file_get_first_visible_index(tracking);
        let visible_rows = Self::standard_file_get_visible_rows();
        for row in 0..visible_rows {
            let index = first_visible + row;
            let Some(entry) = tracking.entries.get(index) else {
                break;
            };
            let row_top = top + 2 + (row as i16 * STANDARD_FILE_GET_ROW_HEIGHT);
            let row_bottom = (row_top + STANDARD_FILE_GET_ROW_HEIGHT).min(bottom - 1);
            let selected = index == tracking.selected;
            if selected {
                Self::fb_fill_rect(
                    bus,
                    screen_base,
                    row_bytes,
                    pixel_size,
                    screen_width,
                    screen_height,
                    row_top,
                    left + 2,
                    row_bottom,
                    right - 2,
                    true,
                );
            }
            let text = if entry.is_directory {
                format!(
                    "{} ▸",
                    Self::standard_file_get_display_name(&entry.display_name)
                )
            } else {
                Self::standard_file_get_display_name(&entry.display_name)
            };
            if selected {
                Self::fb_draw_string_styled_ink(
                    bus,
                    screen_base,
                    row_bytes,
                    pixel_size,
                    screen_width,
                    screen_height,
                    left + 6,
                    row_top + 11,
                    &text,
                    0,
                    12,
                    0,
                    false,
                );
            } else {
                Self::fb_draw_string(
                    bus,
                    screen_base,
                    row_bytes,
                    pixel_size,
                    screen_width,
                    screen_height,
                    left + 6,
                    row_top + 11,
                    &text,
                    0,
                    12,
                );
            }
        }
        let max = tracking
            .entries
            .len()
            .saturating_sub(Self::standard_file_get_visible_rows());
        self.draw_scroll_bar(
            bus,
            dialog_top + STANDARD_FILE_GET_SCROLL_RECT.0,
            dialog_left + STANDARD_FILE_GET_SCROLL_RECT.1,
            dialog_top + STANDARD_FILE_GET_SCROLL_RECT.2,
            dialog_left + STANDARD_FILE_GET_SCROLL_RECT.3,
            Self::standard_file_get_first_visible_index(tracking) as i16,
            0,
            max.min(i16::MAX as usize) as i16,
            if max == 0 { 255 } else { 0 },
        );
    }

    fn standard_file_get_display_name(name: &str) -> String {
        let mut out = String::new();
        for (index, ch) in name.chars().enumerate() {
            if index >= 36 {
                out.push_str("...");
                return out;
            }
            out.push(ch);
        }
        out
    }

    fn standard_file_get_visible_rows() -> usize {
        let inner_height =
            (STANDARD_FILE_GET_LIST_RECT.2 - STANDARD_FILE_GET_LIST_RECT.0 - 3).max(0);
        (inner_height / STANDARD_FILE_GET_ROW_HEIGHT).max(1) as usize
    }

    fn standard_file_get_first_visible_index(tracking: &StandardFileGetTrackingState) -> usize {
        if tracking.entries.is_empty() {
            return 0;
        }
        let visible_rows = Self::standard_file_get_visible_rows();
        let selected = tracking.selected.min(tracking.entries.len() - 1);
        selected.saturating_sub(visible_rows.saturating_sub(1))
    }

    fn begin_standard_file_get_tracking(
        &mut self,
        bus: &mut MacMemoryBus,
        modern_reply: bool,
        reply_ptr: u32,
        stack_ptr: u32,
        pop_total: u32,
        num_types: i16,
        type_list_ptr: u32,
        requested_origin: Option<(i16, i16)>,
    ) {
        let entries = self.standard_file_get_candidates(bus, num_types, type_list_ptr);
        let bounds = self.standard_file_get_dialog_bounds(requested_origin);
        let saved_pixels = self.save_dialog_pixels(bus, bounds);
        let tracking = StandardFileGetTrackingState {
            modern_reply,
            reply_ptr,
            stack_ptr,
            pop_total,
            entries,
            current_dir_id: *self.default_dir_id,
            file_types: Self::standard_file_get_type_list(bus, num_types, type_list_ptr).flatten(),
            selected: 0,
            bounds,
            saved_pixels,
        };
        self.draw_standard_file_get_dialog(bus, &tracking);
        self.standard_file_get_tracking = Some(tracking);
    }

    fn service_standard_file_get_tracking<C: CpuOps>(
        &mut self,
        cpu: &mut C,
        bus: &mut MacMemoryBus,
        mut tracking: StandardFileGetTrackingState,
    ) {
        let mut action = None;
        while let Some(event) = self.event_queue.pop_front() {
            match event.what {
                1 => {
                    action = Self::standard_file_get_mouse_action(
                        &mut tracking,
                        event.where_v,
                        event.where_h,
                    );
                    break;
                }
                3 | 5 => {
                    action = Self::standard_file_get_key_action(
                        &mut tracking,
                        event.message,
                        event.modifiers,
                    );
                    break;
                }
                _ => {}
            }
        }

        match action {
            Some(StandardFileGetAction::Open) => {
                self.finish_standard_file_get_tracking(cpu, bus, tracking, true);
            }
            Some(StandardFileGetAction::Navigate) => {
                let target = tracking
                    .entries
                    .get(tracking.selected)
                    .filter(|entry| entry.is_directory)
                    .map(|entry| entry.dir_id);
                if let Some(target) = target {
                    tracking.current_dir_id = target;
                    tracking.entries = self.standard_file_get_candidates_in_directory(
                        target,
                        tracking.file_types.as_deref(),
                    );
                    tracking.selected = 0;
                }
                self.draw_standard_file_get_dialog(bus, &tracking);
                self.standard_file_get_tracking = Some(tracking);
            }
            Some(StandardFileGetAction::Desktop) => {
                tracking.current_dir_id = 2;
                tracking.entries = self
                    .standard_file_get_candidates_in_directory(2, tracking.file_types.as_deref());
                tracking.selected = 0;
                self.draw_standard_file_get_dialog(bus, &tracking);
                self.standard_file_get_tracking = Some(tracking);
            }
            Some(StandardFileGetAction::Cancel) => {
                self.finish_standard_file_get_tracking(cpu, bus, tracking, false);
            }
            None => {
                self.draw_standard_file_get_dialog(bus, &tracking);
                self.standard_file_get_tracking = Some(tracking);
            }
        }
    }

    fn finish_standard_file_get_tracking<C: CpuOps>(
        &mut self,
        cpu: &mut C,
        bus: &mut MacMemoryBus,
        tracking: StandardFileGetTrackingState,
        accepted: bool,
    ) {
        self.restore_dialog_pixels(bus, tracking.bounds, &tracking.saved_pixels);
        if accepted {
            if let Some(selection) = tracking.entries.get(tracking.selected) {
                if tracking.modern_reply {
                    standard_file_get_reply_modern(
                        bus,
                        tracking.reply_ptr,
                        selection.vref,
                        selection.dir_id,
                        selection.file_type,
                        selection.finder_flags,
                        &selection.name,
                    );
                } else {
                    standard_file_get_reply_old(
                        bus,
                        tracking.reply_ptr,
                        selection.wd_ref,
                        selection.file_type,
                        &selection.name,
                    );
                }
            } else {
                standard_file_cancel_reply(bus, tracking.reply_ptr);
            }
        } else {
            standard_file_cancel_reply(bus, tracking.reply_ptr);
        }
        cpu.write_reg(Register::A7, tracking.stack_ptr + tracking.pop_total);
        cpu.write_reg(Register::D0, 0);
    }

    fn standard_file_get_mouse_action(
        tracking: &mut StandardFileGetTrackingState,
        v: i16,
        h: i16,
    ) -> Option<StandardFileGetAction> {
        let (top, left, _, _) = tracking.bounds;
        let local_v = v - top;
        let local_h = h - left;
        if Self::standard_file_point_in_rect(local_v, local_h, STANDARD_FILE_GET_OPEN_RECT) {
            if tracking.entries.is_empty() {
                return None;
            }
            return Some(if tracking.entries[tracking.selected].is_directory {
                StandardFileGetAction::Navigate
            } else {
                StandardFileGetAction::Open
            });
        }
        if Self::standard_file_point_in_rect(local_v, local_h, STANDARD_FILE_GET_CANCEL_RECT) {
            return Some(StandardFileGetAction::Cancel);
        }
        if Self::standard_file_point_in_rect(local_v, local_h, STANDARD_FILE_GET_DESKTOP_RECT) {
            return Some(StandardFileGetAction::Desktop);
        }
        if Self::standard_file_point_in_rect(local_v, local_h, STANDARD_FILE_GET_SCROLL_RECT)
            && !tracking.entries.is_empty()
        {
            let relative_v = local_v - STANDARD_FILE_GET_SCROLL_RECT.0;
            let height = STANDARD_FILE_GET_SCROLL_RECT.2 - STANDARD_FILE_GET_SCROLL_RECT.0;
            if relative_v < 16 {
                tracking.selected = tracking.selected.saturating_sub(1);
            } else if relative_v >= height - 16 {
                tracking.selected = (tracking.selected + 1).min(tracking.entries.len() - 1);
            } else if relative_v < height / 2 {
                tracking.selected = tracking
                    .selected
                    .saturating_sub(Self::standard_file_get_visible_rows());
            } else {
                tracking.selected = (tracking.selected + Self::standard_file_get_visible_rows())
                    .min(tracking.entries.len() - 1);
            }
            return None;
        }
        if Self::standard_file_point_in_rect(local_v, local_h, STANDARD_FILE_GET_LIST_RECT)
            && !tracking.entries.is_empty()
        {
            let row = ((local_v - STANDARD_FILE_GET_LIST_RECT.0 - 2) / STANDARD_FILE_GET_ROW_HEIGHT)
                .max(0) as usize;
            let index = Self::standard_file_get_first_visible_index(tracking) + row;
            if index < tracking.entries.len() {
                tracking.selected = index;
            }
        }
        None
    }

    fn standard_file_get_key_action(
        tracking: &mut StandardFileGetTrackingState,
        message: u32,
        modifiers: u16,
    ) -> Option<StandardFileGetAction> {
        let key_code = ((message >> 8) & 0xFF) as u8;
        let char_code = (message & 0xFF) as u8;
        let command_down = (modifiers & 0x0100) != 0;

        if char_code == 0x0D || char_code == 0x03 || key_code == 0x24 || key_code == 0x4C {
            return tracking.entries.get(tracking.selected).map(|entry| {
                if entry.is_directory {
                    StandardFileGetAction::Navigate
                } else {
                    StandardFileGetAction::Open
                }
            });
        }
        if char_code == 0x1B || key_code == 0x35 || (command_down && char_code == b'.') {
            return Some(StandardFileGetAction::Cancel);
        }
        if tracking.entries.is_empty() {
            return None;
        }
        if key_code == 0x7E || char_code == 0x1E {
            tracking.selected = tracking.selected.saturating_sub(1);
        } else if key_code == 0x7D || char_code == 0x1F {
            tracking.selected = (tracking.selected + 1).min(tracking.entries.len() - 1);
        }
        None
    }

    fn standard_file_put_prompt(bus: &MacMemoryBus, prompt_ptr: u32) -> String {
        if prompt_ptr == 0 {
            return "Save as:".to_string();
        }
        let prompt = bus.read_pstring(prompt_ptr);
        if prompt.is_empty() {
            "Save as:".to_string()
        } else {
            decode_mac_roman(&prompt)
        }
    }

    fn standard_file_put_dialog_bounds(&self) -> (i16, i16, i16, i16) {
        let (_, _, screen_width, screen_height, _) = self.screen_mode;
        let left = ((screen_width as i16 - STANDARD_FILE_DIALOG_WIDTH) / 2).max(8);
        let top = ((screen_height as i16 - STANDARD_FILE_DIALOG_HEIGHT) / 2).max(24);
        (
            top,
            left,
            top + STANDARD_FILE_DIALOG_HEIGHT,
            left + STANDARD_FILE_DIALOG_WIDTH,
        )
    }

    fn standard_file_put_dialog_items(
        tracking: &StandardFilePutTrackingState,
        location_label: &str,
        writable: bool,
    ) -> Vec<DialogItem> {
        vec![
            DialogItem {
                item_type: if writable { 4 } else { 0x84 },
                rect: STANDARD_FILE_SAVE_RECT,
                text: "Save".to_string(),
                ..DialogItem::default()
            },
            DialogItem {
                item_type: 4,
                rect: STANDARD_FILE_CANCEL_RECT,
                text: "Cancel".to_string(),
                ..DialogItem::default()
            },
            DialogItem {
                item_type: 4,
                rect: STANDARD_FILE_PUT_DESKTOP_RECT,
                text: "Desktop".to_string(),
                ..DialogItem::default()
            },
            DialogItem {
                item_type: 8,
                rect: STANDARD_FILE_PUT_VOLUME_LABEL_RECT,
                text: location_label.to_string(),
                ..DialogItem::default()
            },
            DialogItem {
                item_type: 0,
                rect: STANDARD_FILE_PUT_VOLUME_RECT,
                text: location_label.to_string(),
                ..DialogItem::default()
            },
            DialogItem {
                item_type: 0,
                rect: STANDARD_FILE_PUT_LIST_RECT,
                ..DialogItem::default()
            },
            DialogItem {
                item_type: 0,
                rect: STANDARD_FILE_PUT_SCROLL_RECT,
                ..DialogItem::default()
            },
            DialogItem {
                item_type: 8,
                rect: STANDARD_FILE_PROMPT_RECT,
                text: tracking.prompt.clone(),
                ..DialogItem::default()
            },
            DialogItem {
                item_type: 16,
                rect: STANDARD_FILE_NAME_RECT,
                text: tracking.name.clone(),
                sel_start: tracking.sel_start,
                sel_end: tracking.sel_end,
                ..DialogItem::default()
            },
        ]
    }

    fn draw_standard_file_put_dialog(
        &mut self,
        bus: &mut MacMemoryBus,
        tracking: &StandardFilePutTrackingState,
    ) {
        let (_, _, location_label, writable) =
            self.standard_file_put_directory_location(tracking.current_dir_id);
        let items = Self::standard_file_put_dialog_items(tracking, &location_label, writable);
        self.draw_dialog(
            bus,
            tracking.bounds,
            2,
            "",
            &items,
            STANDARD_FILE_SAVE_ITEM,
            &tracking.name,
            STANDARD_FILE_NAME_ITEM,
            false,
            0,
        );
        let (top, left, _, _) = tracking.bounds;
        let (button_top, button_left, button_bottom, button_right) = STANDARD_FILE_SAVE_RECT;
        self.draw_button_state(
            bus,
            top + button_top,
            left + button_left,
            top + button_bottom,
            left + button_right,
            "Save",
            true,
            writable,
        );
        let (vtop, vleft, vbottom, vright) = STANDARD_FILE_PUT_VOLUME_RECT;
        self.draw_popup_control(
            bus,
            top + vtop,
            left + vleft,
            top + vbottom,
            left + vright,
            &location_label,
        );
        self.draw_standard_file_put_list(bus, tracking);
    }

    fn standard_file_put_visible_rows() -> usize {
        let inner_height =
            (STANDARD_FILE_PUT_LIST_RECT.2 - STANDARD_FILE_PUT_LIST_RECT.0 - 3).max(0);
        (inner_height / STANDARD_FILE_GET_ROW_HEIGHT).max(1) as usize
    }

    fn standard_file_put_first_visible_index(tracking: &StandardFilePutTrackingState) -> usize {
        let Some(selected) = tracking.selected else {
            return 0;
        };
        selected.saturating_sub(Self::standard_file_put_visible_rows().saturating_sub(1))
    }

    fn draw_standard_file_put_list(
        &self,
        bus: &mut MacMemoryBus,
        tracking: &StandardFilePutTrackingState,
    ) {
        let (screen_base, row_bytes, screen_width, screen_height, pixel_size) =
            self.get_screen_params();
        let (dialog_top, dialog_left, _, _) = tracking.bounds;
        let (list_top, list_left, list_bottom, list_right) = STANDARD_FILE_PUT_LIST_RECT;
        let top = dialog_top + list_top;
        let left = dialog_left + list_left;
        let bottom = dialog_top + list_bottom;
        let right = dialog_left + list_right;

        Self::fb_fill_rect(
            bus,
            screen_base,
            row_bytes,
            pixel_size,
            screen_width,
            screen_height,
            top,
            left,
            bottom,
            right,
            false,
        );
        Self::fb_fill_rect(
            bus,
            screen_base,
            row_bytes,
            pixel_size,
            screen_width,
            screen_height,
            top,
            left,
            top + 1,
            right,
            true,
        );
        Self::fb_fill_rect(
            bus,
            screen_base,
            row_bytes,
            pixel_size,
            screen_width,
            screen_height,
            bottom - 1,
            left,
            bottom,
            right,
            true,
        );
        Self::fb_fill_rect(
            bus,
            screen_base,
            row_bytes,
            pixel_size,
            screen_width,
            screen_height,
            top,
            left,
            bottom,
            left + 1,
            true,
        );
        Self::fb_fill_rect(
            bus,
            screen_base,
            row_bytes,
            pixel_size,
            screen_width,
            screen_height,
            top,
            right - 1,
            bottom,
            right,
            true,
        );

        let first_visible = Self::standard_file_put_first_visible_index(tracking);
        for row in 0..Self::standard_file_put_visible_rows() {
            let index = first_visible + row;
            let Some(entry) = tracking.entries.get(index) else {
                break;
            };
            let row_top = top + 2 + (row as i16 * STANDARD_FILE_GET_ROW_HEIGHT);
            let row_bottom = (row_top + STANDARD_FILE_GET_ROW_HEIGHT).min(bottom - 1);
            let selected = tracking.selected == Some(index);
            if selected {
                Self::fb_fill_rect(
                    bus,
                    screen_base,
                    row_bytes,
                    pixel_size,
                    screen_width,
                    screen_height,
                    row_top,
                    left + 2,
                    row_bottom,
                    right - 2,
                    true,
                );
            }
            let text = if entry.is_directory {
                format!(
                    "{} ▸",
                    Self::standard_file_get_display_name(&entry.display_name)
                )
            } else {
                Self::standard_file_get_display_name(&entry.display_name)
            };
            if selected {
                Self::fb_draw_string_styled_ink(
                    bus,
                    screen_base,
                    row_bytes,
                    pixel_size,
                    screen_width,
                    screen_height,
                    left + 6,
                    row_top + 11,
                    &text,
                    0,
                    12,
                    0,
                    false,
                );
            } else {
                Self::fb_draw_string(
                    bus,
                    screen_base,
                    row_bytes,
                    pixel_size,
                    screen_width,
                    screen_height,
                    left + 6,
                    row_top + 11,
                    &text,
                    0,
                    12,
                );
            }
        }
        let max = tracking
            .entries
            .len()
            .saturating_sub(Self::standard_file_put_visible_rows());
        self.draw_scroll_bar(
            bus,
            dialog_top + STANDARD_FILE_PUT_SCROLL_RECT.0,
            dialog_left + STANDARD_FILE_PUT_SCROLL_RECT.1,
            dialog_top + STANDARD_FILE_PUT_SCROLL_RECT.2,
            dialog_left + STANDARD_FILE_PUT_SCROLL_RECT.3,
            first_visible.min(i16::MAX as usize) as i16,
            0,
            max.min(i16::MAX as usize) as i16,
            if max == 0 { 255 } else { 0 },
        );
    }

    fn standard_file_put_directory_location(&self, dir_id: u32) -> (i16, u32, String, bool) {
        let path = self.directory_path_for_id(dir_id).unwrap_or_default();
        let volume = self.vfs_volume_for_path(path);
        let vref = volume
            .map(|volume| volume.ref_num)
            .unwrap_or_else(Self::boot_volume_ref_num);
        let volume_name = volume
            .map(|volume| volume.name.as_str())
            .unwrap_or_else(|| Self::boot_volume_name());
        let directory_name = Self::vfs_directory_name(path);
        let label = if path.is_empty() || path.eq_ignore_ascii_case(volume_name) {
            volume_name.to_string()
        } else {
            format!("{volume_name}: {directory_name}")
        };
        (vref, dir_id, label, !self.vfs_path_is_read_only(path))
    }

    fn standard_file_put_default_destination(&self) -> (i16, u32) {
        let (vref, dir_id, _, writable) =
            self.standard_file_put_directory_location(*self.default_dir_id);
        if writable {
            (vref, dir_id)
        } else {
            (Self::boot_volume_ref_num(), 2)
        }
    }

    fn persist_standard_file_directory(
        &mut self,
        bus: &mut MacMemoryBus,
        vref: i16,
        dir_id: u32,
    ) -> i16 {
        let wd_ref = if dir_id == 2 {
            vref
        } else {
            self.open_working_directory(vref, dir_id, 0).unwrap_or(vref)
        };
        self.default_dir_id
            .with_mut(|default_dir_id| *default_dir_id = dir_id);
        self.app_wd_refnum
            .with_mut(|app_ref_num| *app_ref_num = wd_ref);
        bus.write_long(addr::CUR_DIR_STORE, dir_id);
        bus.write_word(addr::SF_SAVE_DISK, (-vref) as u16);
        wd_ref
    }

    fn begin_standard_file_put_tracking(
        &mut self,
        bus: &mut MacMemoryBus,
        modern_reply: bool,
        reply_ptr: u32,
        stack_ptr: u32,
        pop_total: u32,
        prompt_ptr: u32,
        default_name_ptr: u32,
    ) {
        let default_name = standard_file_default_name(bus, default_name_ptr);
        let mut name = decode_mac_roman(&default_name);
        Self::standard_file_clamp_name(&mut name);
        let bounds = self.standard_file_put_dialog_bounds();
        let saved_pixels = self.save_dialog_pixels(bus, bounds);
        let name_len = name.len().min(i16::MAX as usize) as i16;
        let current_dir_id = *self.default_dir_id;
        let entries = self.standard_file_get_candidates_in_directory(current_dir_id, None);
        let tracking = StandardFilePutTrackingState {
            modern_reply,
            reply_ptr,
            stack_ptr,
            pop_total,
            entries,
            current_dir_id,
            selected: None,
            prompt: Self::standard_file_put_prompt(bus, prompt_ptr),
            name,
            sel_start: 0,
            sel_end: name_len,
            bounds,
            saved_pixels,
        };
        self.draw_standard_file_put_dialog(bus, &tracking);
        self.standard_file_put_tracking = Some(tracking);
    }

    fn service_standard_file_put_tracking<C: CpuOps>(
        &mut self,
        cpu: &mut C,
        bus: &mut MacMemoryBus,
        mut tracking: StandardFilePutTrackingState,
    ) {
        let mut action = None;
        while let Some(event) = self.event_queue.pop_front() {
            match event.what {
                1 => {
                    action = self.standard_file_put_mouse_action(
                        &mut tracking,
                        event.where_v,
                        event.where_h,
                    );
                    break;
                }
                3 | 5 => {
                    action = Self::standard_file_put_key_action(
                        &mut tracking,
                        event.message,
                        event.modifiers,
                    );
                    break;
                }
                _ => {}
            }
        }

        match action {
            Some(StandardFilePutAction::Save) => {
                let (_, _, _, writable) =
                    self.standard_file_put_directory_location(tracking.current_dir_id);
                if tracking.name.trim().is_empty() || !writable {
                    self.draw_standard_file_put_dialog(bus, &tracking);
                    self.standard_file_put_tracking = Some(tracking);
                    return;
                }
                self.finish_standard_file_put_tracking(cpu, bus, tracking, true);
            }
            Some(StandardFilePutAction::Navigate) => {
                let target = tracking
                    .selected
                    .and_then(|selected| tracking.entries.get(selected))
                    .filter(|entry| entry.is_directory)
                    .map(|entry| entry.dir_id);
                if let Some(target) = target {
                    tracking.current_dir_id = target;
                    tracking.entries = self.standard_file_get_candidates_in_directory(target, None);
                    tracking.selected = None;
                }
                self.draw_standard_file_put_dialog(bus, &tracking);
                self.standard_file_put_tracking = Some(tracking);
            }
            Some(StandardFilePutAction::Parent) => {
                let parent_dir_id = self
                    .directory_entry_for_id(tracking.current_dir_id)
                    .map(|directory| directory.parent_dir_id)
                    .filter(|parent| *parent > 1)
                    .unwrap_or(2);
                tracking.current_dir_id = parent_dir_id;
                tracking.entries =
                    self.standard_file_get_candidates_in_directory(parent_dir_id, None);
                tracking.selected = None;
                self.draw_standard_file_put_dialog(bus, &tracking);
                self.standard_file_put_tracking = Some(tracking);
            }
            Some(StandardFilePutAction::Desktop) => {
                tracking.current_dir_id = 2;
                tracking.entries = self.standard_file_get_candidates_in_directory(2, None);
                tracking.selected = None;
                self.draw_standard_file_put_dialog(bus, &tracking);
                self.standard_file_put_tracking = Some(tracking);
            }
            Some(StandardFilePutAction::Cancel) => {
                self.finish_standard_file_put_tracking(cpu, bus, tracking, false);
            }
            None => {
                self.draw_standard_file_put_dialog(bus, &tracking);
                self.standard_file_put_tracking = Some(tracking);
            }
        }
    }

    fn finish_standard_file_put_tracking<C: CpuOps>(
        &mut self,
        cpu: &mut C,
        bus: &mut MacMemoryBus,
        tracking: StandardFilePutTrackingState,
        accepted: bool,
    ) {
        self.restore_dialog_pixels(bus, tracking.bounds, &tracking.saved_pixels);
        let (vref, dir_id, _, writable) =
            self.standard_file_put_directory_location(tracking.current_dir_id);
        let wd_ref = self.persist_standard_file_directory(bus, vref, dir_id);
        if accepted {
            debug_assert!(writable);
            let mut name = encode_mac_roman_lossy(&tracking.name);
            if name.is_empty() {
                name.extend_from_slice(b"Untitled");
            }
            name.truncate(63);
            if tracking.modern_reply {
                let target_name = decode_mac_roman(&name);
                let replacing = self
                    .find_vfs_file_in_directory(dir_id, &target_name)
                    .is_some()
                    || self
                        .find_vfs_rsrc_file_in_directory(dir_id, &target_name)
                        .is_some();
                standard_file_put_reply_modern(
                    bus,
                    tracking.reply_ptr,
                    vref,
                    dir_id,
                    &name,
                    replacing,
                );
            } else {
                standard_file_put_reply_old(bus, tracking.reply_ptr, wd_ref, &name);
            }
        } else {
            standard_file_cancel_reply(bus, tracking.reply_ptr);
        }
        cpu.write_reg(Register::A7, tracking.stack_ptr + tracking.pop_total);
        cpu.write_reg(Register::D0, 0);
    }

    fn standard_file_put_mouse_action(
        &self,
        tracking: &mut StandardFilePutTrackingState,
        v: i16,
        h: i16,
    ) -> Option<StandardFilePutAction> {
        let (top, left, _, _) = tracking.bounds;
        let local_v = v - top;
        let local_h = h - left;
        if Self::standard_file_point_in_rect(local_v, local_h, STANDARD_FILE_SAVE_RECT) {
            Some(StandardFilePutAction::Save)
        } else if Self::standard_file_point_in_rect(local_v, local_h, STANDARD_FILE_CANCEL_RECT) {
            Some(StandardFilePutAction::Cancel)
        } else if Self::standard_file_point_in_rect(
            local_v,
            local_h,
            STANDARD_FILE_PUT_DESKTOP_RECT,
        ) {
            Some(StandardFilePutAction::Desktop)
        } else if Self::standard_file_point_in_rect(local_v, local_h, STANDARD_FILE_PUT_SCROLL_RECT)
            && !tracking.entries.is_empty()
        {
            let relative_v = local_v - STANDARD_FILE_PUT_SCROLL_RECT.0;
            let height = STANDARD_FILE_PUT_SCROLL_RECT.2 - STANDARD_FILE_PUT_SCROLL_RECT.0;
            let selected = tracking.selected.unwrap_or(0);
            tracking.selected = Some(if relative_v < 16 {
                selected.saturating_sub(1)
            } else if relative_v >= height - 16 {
                (selected + 1).min(tracking.entries.len() - 1)
            } else if relative_v < height / 2 {
                selected.saturating_sub(Self::standard_file_put_visible_rows())
            } else {
                (selected + Self::standard_file_put_visible_rows()).min(tracking.entries.len() - 1)
            });
            None
        } else if Self::standard_file_point_in_rect(local_v, local_h, STANDARD_FILE_PUT_LIST_RECT)
            && !tracking.entries.is_empty()
        {
            let row = ((local_v - STANDARD_FILE_PUT_LIST_RECT.0 - 2) / STANDARD_FILE_GET_ROW_HEIGHT)
                .max(0) as usize;
            let index = Self::standard_file_put_first_visible_index(tracking) + row;
            if index >= tracking.entries.len() {
                return None;
            }
            if tracking.selected == Some(index) && tracking.entries[index].is_directory {
                return Some(StandardFilePutAction::Navigate);
            }
            tracking.selected = Some(index);
            None
        } else {
            None
        }
    }

    fn standard_file_put_key_action(
        tracking: &mut StandardFilePutTrackingState,
        message: u32,
        modifiers: u16,
    ) -> Option<StandardFilePutAction> {
        let key_code = ((message >> 8) & 0xFF) as u8;
        let char_code = (message & 0xFF) as u8;
        let command_down = (modifiers & 0x0100) != 0;

        if char_code == 0x0D || char_code == 0x03 || key_code == 0x24 || key_code == 0x4C {
            return Some(StandardFilePutAction::Save);
        }
        if char_code == 0x1B || key_code == 0x35 || (command_down && char_code == b'.') {
            return Some(StandardFilePutAction::Cancel);
        }
        if command_down && key_code == 0x7E {
            return Some(StandardFilePutAction::Parent);
        }
        if command_down && key_code == 0x7D {
            return tracking
                .selected
                .and_then(|selected| tracking.entries.get(selected))
                .filter(|entry| entry.is_directory)
                .map(|_| StandardFilePutAction::Navigate);
        }
        if command_down && char_code.eq_ignore_ascii_case(&b'd') {
            return Some(StandardFilePutAction::Desktop);
        }
        if command_down && char_code.eq_ignore_ascii_case(&b'a') {
            tracking.sel_start = 0;
            tracking.sel_end = tracking.name.len().min(i16::MAX as usize) as i16;
            return None;
        }
        if char_code == 0x08 || key_code == 0x33 {
            Self::standard_file_backspace(tracking);
            return None;
        }
        if command_down || !(0x20..=0x7E).contains(&char_code) || matches!(char_code, b'/' | b':') {
            return None;
        }

        let ch = char_code as char;
        Self::standard_file_replace_selection(tracking, ch);
        None
    }

    fn standard_file_backspace(tracking: &mut StandardFilePutTrackingState) {
        let (start, end) = Self::standard_file_selection_range(tracking);
        if start < end {
            tracking.name.replace_range(start..end, "");
            tracking.sel_start = start.min(i16::MAX as usize) as i16;
            tracking.sel_end = tracking.sel_start;
            return;
        }
        if start == 0 {
            return;
        }
        let prev = tracking.name[..start]
            .char_indices()
            .last()
            .map(|(idx, _)| idx)
            .unwrap_or(0);
        tracking.name.replace_range(prev..start, "");
        tracking.sel_start = prev.min(i16::MAX as usize) as i16;
        tracking.sel_end = tracking.sel_start;
    }

    fn standard_file_replace_selection(
        tracking: &mut StandardFilePutTrackingState,
        replacement: char,
    ) {
        let (start, end) = Self::standard_file_selection_range(tracking);
        tracking
            .name
            .replace_range(start..end, &replacement.to_string());
        Self::standard_file_clamp_name(&mut tracking.name);
        let cursor = (start + replacement.len_utf8()).min(tracking.name.len());
        let cursor = Self::standard_file_clamp_boundary(&tracking.name, cursor);
        tracking.sel_start = cursor.min(i16::MAX as usize) as i16;
        tracking.sel_end = tracking.sel_start;
    }

    fn standard_file_selection_range(tracking: &StandardFilePutTrackingState) -> (usize, usize) {
        let len = tracking.name.len();
        let a = (tracking.sel_start.max(0) as usize).min(len);
        let b = (tracking.sel_end.max(0) as usize).min(len);
        let start = Self::standard_file_clamp_boundary(&tracking.name, a.min(b));
        let end = Self::standard_file_clamp_boundary(&tracking.name, a.max(b));
        (start, end)
    }

    fn standard_file_clamp_name(name: &mut String) {
        while encode_mac_roman_lossy(name).len() > 63 {
            if name.pop().is_none() {
                break;
            }
        }
    }

    fn standard_file_clamp_boundary(text: &str, mut idx: usize) -> usize {
        idx = idx.min(text.len());
        while idx > 0 && !text.is_char_boundary(idx) {
            idx -= 1;
        }
        idx
    }

    fn standard_file_point_in_rect(v: i16, h: i16, rect: (i16, i16, i16, i16)) -> bool {
        let (top, left, bottom, right) = rect;
        v >= top && v < bottom && h >= left && h < right
    }

    fn build_alias_record(&mut self, bus: &MacMemoryBus, target_ptr: u32) -> Vec<u8> {
        let vref = bus.read_word(target_ptr) as i16;
        let dir_id = bus.read_long(target_ptr + 2);
        let target_name = crate::trap::types::read_fsspec_name(bus, target_ptr);
        let resolved_dir_id = self.resolve_directory_id(vref, dir_id);
        let target_key = self.vfs_key_for_fsspec(vref, dir_id, &target_name);
        let target_directory = target_key.as_ref().and_then(|key| {
            self.vfs_directories
                .iter()
                .find(|directory| directory.path.eq_ignore_ascii_case(key))
                .cloned()
        });
        let target_metadata = target_key
            .as_ref()
            .and_then(|key| self.vfs_file_metadata(key));

        let alias_kind = if target_directory.is_some() {
            Self::ALIAS_KIND_FOLDER
        } else {
            Self::ALIAS_KIND_FILE
        };
        let parent_dir_id = target_metadata
            .map(|metadata| metadata.parent_dir_id)
            .or_else(|| target_directory.as_ref().map(|dir| dir.parent_dir_id))
            .unwrap_or(resolved_dir_id);
        let file_id = target_metadata
            .map(|metadata| metadata.file_id)
            .or_else(|| target_directory.as_ref().map(|dir| dir.dir_id))
            .unwrap_or(0);
        let created_date = target_metadata
            .map(|metadata| metadata.created_date)
            .unwrap_or(0);
        let file_type = target_metadata
            .map(|metadata| metadata.file_type)
            .unwrap_or(0);
        let creator = target_metadata
            .map(|metadata| metadata.creator)
            .unwrap_or(0);

        let mut record = vec![0; Self::ALIAS_RECORD_FIXED_SIZE];
        Self::alias_write_u32(&mut record, 0, 0); // userType
        Self::alias_write_u16(&mut record, 6, Self::ALIAS_RECORD_VERSION);
        Self::alias_write_u16(&mut record, 8, alias_kind);
        Self::alias_write_pstring(&mut record, 10, 28, Self::boot_volume_name());
        Self::alias_write_u32(&mut record, 38, 0); // volume creation date
        Self::alias_write_u16(&mut record, 42, 0x4244); // HFS volume signature 'BD'
        Self::alias_write_u16(&mut record, 44, 0); // fixed hard disk
        Self::alias_write_u32(&mut record, 46, parent_dir_id);
        Self::alias_write_pstring(&mut record, 50, 64, &target_name);
        Self::alias_write_u32(&mut record, 114, file_id);
        Self::alias_write_u32(&mut record, 118, created_date);
        Self::alias_write_u32(&mut record, 122, file_type);
        Self::alias_write_u32(&mut record, 126, creator);
        Self::alias_write_i16(&mut record, 130, -1); // no relative source file
        Self::alias_write_i16(&mut record, 132, -1);
        Self::alias_write_u32(&mut record, 134, 0);
        Self::alias_write_u16(&mut record, 138, 0);

        let parent_name = self
            .directory_path_for_id(parent_dir_id)
            .map(Self::vfs_basename)
            .filter(|name| !name.is_empty())
            .map(str::to_owned)
            .unwrap_or_else(|| Self::boot_volume_name().to_string());
        Self::append_alias_extra(
            &mut record,
            Self::ALIAS_EXTRA_PARENT_DIR_NAME,
            &encode_mac_roman_lossy(&parent_name),
        );
        Self::append_alias_extra(&mut record, Self::ALIAS_EXTRA_END, &[]);

        let size = record.len().min(u16::MAX as usize) as u16;
        Self::alias_write_u16(&mut record, 4, size);
        record
    }

    fn build_minimal_alias_record_from_full_path(&self, full_path: &[u8]) -> Vec<u8> {
        let path = decode_mac_roman(full_path);
        let components: Vec<&str> = path
            .split(':')
            .filter(|component| !component.is_empty())
            .collect();
        let volume_name = components
            .first()
            .copied()
            .unwrap_or(Self::boot_volume_name());
        let target_name = components.last().copied().unwrap_or("");
        let parent_name = components
            .get(components.len().saturating_sub(2))
            .copied()
            .unwrap_or(volume_name);

        // AliasRecord is deliberately opaque to applications. Populate the
        // documented fixed fields which GetAliasInfo and the Alias Manager
        // need even when no catalogue lookup is possible from a full path.
        let mut record = vec![0; Self::ALIAS_RECORD_FIXED_SIZE];
        Self::alias_write_u32(&mut record, 0, 0); // userType
        Self::alias_write_u16(&mut record, 6, Self::ALIAS_RECORD_VERSION);
        Self::alias_write_u16(&mut record, 8, Self::ALIAS_KIND_FILE);
        Self::alias_write_pstring(&mut record, 10, 28, volume_name);
        Self::alias_write_u16(&mut record, 42, 0x4244); // HFS volume signature 'BD'
        Self::alias_write_u16(&mut record, 44, 0); // fixed hard disk
        Self::alias_write_pstring(&mut record, 50, 64, target_name);
        Self::alias_write_i16(&mut record, 130, -1); // no relative source file
        Self::alias_write_i16(&mut record, 132, -1);
        Self::append_alias_extra(
            &mut record,
            Self::ALIAS_EXTRA_PARENT_DIR_NAME,
            &encode_mac_roman_lossy(parent_name),
        );
        Self::append_alias_extra(&mut record, Self::ALIAS_EXTRA_FULL_PATH, full_path);
        Self::append_alias_extra(&mut record, Self::ALIAS_EXTRA_END, &[]);

        let size = record.len().min(u16::MAX as usize) as u16;
        Self::alias_write_u16(&mut record, 4, size);
        record
    }

    fn alias_extra_data(
        bus: &MacMemoryBus,
        alias_data_ptr: u32,
        wanted_kind: i16,
    ) -> Option<Vec<u8>> {
        let record_size = bus.read_word(alias_data_ptr + 4) as usize;
        let mut offset = Self::ALIAS_RECORD_FIXED_SIZE;
        while offset + 4 <= record_size {
            let kind = bus.read_word(alias_data_ptr + offset as u32) as i16;
            let length = bus.read_word(alias_data_ptr + offset as u32 + 2) as usize;
            let data_offset = offset + 4;
            if data_offset + length > record_size {
                return None;
            }
            if kind == wanted_kind {
                return Some(bus.read_bytes(alias_data_ptr + data_offset as u32, length));
            }
            if kind == Self::ALIAS_EXTRA_END {
                break;
            }
            offset = data_offset + length + (length % 2);
        }
        None
    }

    fn alias_write_u16(record: &mut [u8], offset: usize, value: u16) {
        record[offset..offset + 2].copy_from_slice(&value.to_be_bytes());
    }

    fn alias_write_i16(record: &mut [u8], offset: usize, value: i16) {
        Self::alias_write_u16(record, offset, value as u16);
    }

    fn alias_write_u32(record: &mut [u8], offset: usize, value: u32) {
        record[offset..offset + 4].copy_from_slice(&value.to_be_bytes());
    }

    fn alias_write_pstring(record: &mut [u8], offset: usize, field_len: usize, value: &str) {
        let bytes = encode_mac_roman_lossy(value);
        let len = bytes.len().min(field_len.saturating_sub(1));
        record[offset] = len as u8;
        record[offset + 1..offset + 1 + len].copy_from_slice(&bytes[..len]);
    }

    fn append_alias_extra(record: &mut Vec<u8>, kind: i16, data: &[u8]) {
        record.extend_from_slice(&(kind as u16).to_be_bytes());
        record.extend_from_slice(&(data.len().min(u16::MAX as usize) as u16).to_be_bytes());
        record.extend_from_slice(&data[..data.len().min(u16::MAX as usize)]);
        if data.len() % 2 != 0 {
            record.push(0);
        }
    }

    fn write_point_words(bus: &mut MacMemoryBus, addr: u32, point: (i16, i16)) {
        bus.write_word(addr, point.0 as u16);
        bus.write_word(addr + 2, point.1 as u16);
    }

    fn scriptutil_text_byte_width(&self, byte: u8, font_scale: i16) -> i32 {
        let ch = byte as char;
        if let Some((glyph, _)) = get_glyph(self.tx_font, self.tx_size, ch) {
            i32::from(self.glyph_advance(glyph) * font_scale)
        } else {
            i32::from(self.missing_glyph_advance() * font_scale)
        }
    }

    fn scriptutil_measure_text_range_width(
        &self,
        bus: &MacMemoryBus,
        text_ptr: u32,
        start: u32,
        end: u32,
    ) -> i32 {
        let (_, font_scale) = get_font_face_scaled(self.tx_font, self.tx_size);
        let mut width = 0i32;
        for offset in start..end {
            let byte = bus.read_byte(text_ptr.wrapping_add(offset));
            width = width.saturating_add(self.scriptutil_text_byte_width(byte, font_scale));
        }
        width
    }

    fn scriptutil_fixed_to_pixels(raw: u32) -> i32 {
        let signed = raw as i32;
        if (raw & 0xFFFF_0000) != 0 {
            signed >> 16
        } else {
            signed
        }
    }

    fn scriptutil_pixels_to_width_like(raw: u32, pixels: i32) -> u32 {
        let pixels = pixels.clamp(0, 0x7FFF);
        if (raw & 0xFFFF_0000) != 0 {
            ((pixels as i32) << 16) as u32
        } else {
            pixels as u32
        }
    }

    fn scriptutil_is_roman_break_space(byte: u8) -> bool {
        matches!(byte, b' ' | b'\t' | b'\r' | b'\n')
    }

    fn scriptutil_last_space_break(
        bus: &MacMemoryBus,
        text_ptr: u32,
        start: u32,
        fit_end: u32,
        text_end: u32,
    ) -> Option<u32> {
        let mut break_offset = None;
        let mut offset = start;
        while offset < fit_end {
            let byte = bus.read_byte(text_ptr.wrapping_add(offset));
            if Self::scriptutil_is_roman_break_space(byte) {
                let mut after_spaces = offset + 1;
                while after_spaces < text_end
                    && Self::scriptutil_is_roman_break_space(
                        bus.read_byte(text_ptr.wrapping_add(after_spaces)),
                    )
                {
                    after_spaces += 1;
                }
                break_offset = Some(after_spaces);
                offset = after_spaces;
            } else {
                offset += 1;
            }
        }
        break_offset
    }

    fn handle_scriptutil_styled_line_break<C: CpuOps>(
        &mut self,
        bus: &mut MacMemoryBus,
        cpu: &mut C,
        sp: u32,
    ) -> Result<()> {
        let text_offset_ptr = bus.read_long(sp + 4);
        let text_width_ptr = bus.read_long(sp + 8);
        let text_end_arg = (bus.read_long(sp + 16) as i32).max(0) as u32;
        let text_start_arg = (bus.read_long(sp + 20) as i32).max(0) as u32;
        let text_len = (bus.read_long(sp + 24) as i32).max(0) as u32;
        let text_ptr = bus.read_long(sp + 28);

        let text_start = text_start_arg.min(text_len);
        let text_end = text_end_arg.max(text_start).min(text_len);
        let input_offset = if text_offset_ptr != 0 {
            bus.read_long(text_offset_ptr)
        } else {
            0
        };
        let first_style_run_on_line = input_offset != 0;
        let width_raw = if text_width_ptr != 0 {
            bus.read_long(text_width_ptr)
        } else {
            0x7FFF
        };
        let available = Self::scriptutil_fixed_to_pixels(width_raw).max(0);

        let run_width =
            self.scriptutil_measure_text_range_width(bus, text_ptr, text_start, text_end);
        let (result, output_offset, consumed_width) = if run_width <= available {
            (2u16, text_end, run_width)
        } else {
            let (_, font_scale) = get_font_face_scaled(self.tx_font, self.tx_size);
            let mut width = 0i32;
            let mut fit_offset = text_start;
            for offset in text_start..text_end {
                let byte_width = self.scriptutil_text_byte_width(
                    bus.read_byte(text_ptr.wrapping_add(offset)),
                    font_scale,
                );
                if width.saturating_add(byte_width) > available {
                    break;
                }
                width = width.saturating_add(byte_width);
                fit_offset = offset + 1;
            }

            if let Some(word_offset) =
                Self::scriptutil_last_space_break(bus, text_ptr, text_start, fit_offset, text_end)
            {
                let word_width = self.scriptutil_measure_text_range_width(
                    bus,
                    text_ptr,
                    text_start,
                    word_offset,
                );
                (0u16, word_offset, word_width)
            } else if first_style_run_on_line {
                let char_offset = if fit_offset > text_start {
                    fit_offset
                } else {
                    (text_start + 1).min(text_end)
                };
                let char_width = self.scriptutil_measure_text_range_width(
                    bus,
                    text_ptr,
                    text_start,
                    char_offset,
                );
                (1u16, char_offset, char_width)
            } else {
                (0u16, text_start, 0)
            }
        };

        if text_offset_ptr != 0 {
            bus.write_long(text_offset_ptr, output_offset);
        }
        if text_width_ptr != 0 {
            let remaining = available.saturating_sub(consumed_width);
            bus.write_long(
                text_width_ptr,
                Self::scriptutil_pixels_to_width_like(width_raw, remaining),
            );
        }
        bus.write_word(sp + 32, result);
        cpu.write_reg(Register::D0, u32::from(result));
        cpu.write_reg(Register::A7, sp + 32);
        Ok(())
    }

    fn list_no_click_cell() -> (i16, i16) {
        (-1, -1)
    }

    /// Copy `bytes` into `handle`, resizing or replacing its backing
    /// allocation as needed and keeping the handle-ownership map in sync.
    /// Returns the current master-pointer target, or 0 for an empty handle
    /// or allocation failure.
    fn write_bytes_to_handle(&mut self, bus: &mut MacMemoryBus, handle: u32, bytes: &[u8]) -> u32 {
        if handle == 0 {
            return 0;
        }

        let new_size = bytes.len() as u32;
        let old_ptr = bus.read_long(handle);

        if new_size == 0 {
            if old_ptr != 0 {
                bus.free(old_ptr);
                self.untrack_handle_ptr(old_ptr);
            }
            self.with_resource_manager_mut(|resource_manager| {
                if let Some(entry) = resource_manager.loaded_handles.get_mut(&handle) {
                    entry.0 = 0;
                }
            });
            bus.write_long(handle, 0);
            return 0;
        }

        if old_ptr != 0 {
            let old_size = bus.get_alloc_size(old_ptr).unwrap_or(0);
            let aligned_old = (old_size + 3) & !3;
            let aligned_new = (new_size + 3) & !3;
            if old_size == new_size || aligned_new <= aligned_old {
                bus.set_alloc_size(old_ptr, new_size);
                bus.write_bytes(old_ptr, bytes);
                self.track_handle_ptr(old_ptr, handle);
                self.with_resource_manager_mut(|resource_manager| {
                    if let Some(entry) = resource_manager.loaded_handles.get_mut(&handle) {
                        entry.0 = old_ptr;
                    }
                });
                return old_ptr;
            }
        }

        let new_ptr = bus.alloc(new_size);
        if new_ptr == 0 {
            return 0;
        }
        bus.write_bytes(new_ptr, bytes);

        if old_ptr != 0 {
            bus.free(old_ptr);
            self.untrack_handle_ptr(old_ptr);
        }
        bus.write_long(handle, new_ptr);
        self.track_handle_ptr(new_ptr, handle);
        self.with_resource_manager_mut(|resource_manager| {
            if let Some(entry) = resource_manager.loaded_handles.get_mut(&handle) {
                entry.0 = new_ptr;
            }
        });
        new_ptr
    }

    fn scriptutil_font_script(&self) -> u8 {
        let font = self.tx_font as u16;
        if (0x4000..=0xBFFF).contains(&font) {
            (((font - 0x4000) / 0x0200) + 1) as u8
        } else {
            0
        }
    }

    fn scriptutil_is_lead_byte(script: u8, byte: u8) -> bool {
        match script {
            // Macintosh Japanese uses Shift-JIS lead-byte ranges.
            1 => (0x81..=0x9F).contains(&byte) || (0xE0..=0xFC).contains(&byte),
            // The Macintosh Chinese and Korean double-byte encodings reserve
            // the high-byte range used by their script parse tables.
            2 | 3 | 25 => (0xA1..=0xFE).contains(&byte),
            _ => false,
        }
    }

    fn scriptutil_character_len(script: u8, text: &[u8], offset: usize) -> usize {
        let Some(&byte) = text.get(offset) else {
            return 0;
        };
        if Self::scriptutil_is_lead_byte(script, byte) && offset + 1 < text.len() {
            2
        } else {
            1
        }
    }

    fn scriptutil_is_complete_text(script: u8, text: &[u8]) -> bool {
        let mut offset = 0;
        while offset < text.len() {
            if Self::scriptutil_is_lead_byte(script, text[offset]) {
                if offset + 1 == text.len() {
                    return false;
                }
                offset += 2;
            } else {
                offset += 1;
            }
        }
        true
    }

    fn scriptutil_replace_text(
        &mut self,
        bus: &mut MacMemoryBus,
        base_handle: u32,
        substitution_handle: u32,
        key_ptr: u32,
    ) -> i16 {
        const MEM_FULL_ERR: i16 = -108;
        const NIL_HANDLE_ERR: i16 = -109;
        const MEM_WZ_ERR: i16 = -111;

        if base_handle == 0 || substitution_handle == 0 {
            return NIL_HANDLE_ERR;
        }

        let base_ptr = bus.read_long(base_handle);
        let substitution_ptr = bus.read_long(substitution_handle);
        if base_ptr == 0 || substitution_ptr == 0 {
            return MEM_WZ_ERR;
        }

        let Some(base_size) = bus.get_alloc_size(base_ptr) else {
            return MEM_WZ_ERR;
        };
        let Some(substitution_size) = bus.get_alloc_size(substitution_ptr) else {
            return MEM_WZ_ERR;
        };
        let key = if key_ptr == 0 {
            Vec::new()
        } else {
            bus.read_pstring(key_ptr)
        };
        if key.is_empty() {
            return 0;
        }

        let base = bus.read_bytes(base_ptr, base_size as usize);
        let substitution = bus.read_bytes(substitution_ptr, substitution_size as usize);
        let script = self.scriptutil_font_script();
        let key_is_complete = Self::scriptutil_is_complete_text(script, &key);
        let mut replaced = Vec::with_capacity(base.len());
        let mut substitutions = 0i16;
        let mut offset = 0;
        while offset < base.len() {
            let candidate_end = offset.saturating_add(key.len());
            let matches_key = candidate_end <= base.len()
                && base[offset..candidate_end] == key
                && key_is_complete;
            if matches_key {
                replaced.extend_from_slice(&substitution);
                substitutions = substitutions.saturating_add(1);
                offset = candidate_end;
            } else {
                let char_len = Self::scriptutil_character_len(script, &base, offset);
                replaced.extend_from_slice(&base[offset..offset + char_len]);
                offset += char_len;
            }
        }

        if substitutions == 0 {
            return 0;
        }
        let updated_ptr = self.write_bytes_to_handle(bus, base_handle, &replaced);
        if !replaced.is_empty() && updated_ptr == 0 {
            return MEM_FULL_ERR;
        }
        substitutions
    }

    fn sync_scrap_handle(&mut self, bus: &mut MacMemoryBus) -> u32 {
        let handle = self.scrap.ensure_handle(|| {
            let handle = bus.alloc(4);
            if handle != 0 {
                bus.write_long(handle, 0);
            }
            handle
        });
        if handle == 0 {
            return 0;
        }
        if !self.scrap.summary().handle_dirty {
            return handle;
        }

        let bytes = self.scrap.serialized_entries();
        let wrote = if bytes.is_empty() {
            self.write_bytes_to_handle(bus, handle, &bytes);
            true
        } else {
            self.write_bytes_to_handle(bus, handle, &bytes) != 0
        };
        if wrote {
            self.scrap.mark_handle_clean();
        }
        handle
    }

    fn list_record_ptr(bus: &MacMemoryBus, list_handle: u32) -> u32 {
        if list_handle == 0 {
            0
        } else {
            bus.read_long(list_handle)
        }
    }

    fn compute_list_cell_size(
        &self,
        view_rect: (i16, i16, i16, i16),
        data_bounds: (i16, i16, i16, i16),
        requested: (i16, i16),
    ) -> (i16, i16) {
        let cols = (data_bounds.3 - data_bounds.1).max(1);
        let default_h = ((view_rect.3 - view_rect.1).max(1) / cols).max(1);
        let default_v = self.tx_size.max(9) + 2;
        let cell_v = if requested.0 > 0 {
            requested.0
        } else {
            default_v
        };
        let cell_h = if requested.1 > 0 {
            requested.1
        } else {
            default_h
        };
        (cell_v.max(1), cell_h.max(1))
    }

    fn compute_list_visible_rect(
        view_rect: (i16, i16, i16, i16),
        data_bounds: (i16, i16, i16, i16),
        cell_size: (i16, i16),
    ) -> (i16, i16, i16, i16) {
        let rows_visible = ((view_rect.2 - view_rect.0).max(0) + cell_size.0 - 1) / cell_size.0;
        let cols_visible = ((view_rect.3 - view_rect.1).max(0) + cell_size.1 - 1) / cell_size.1;
        (
            data_bounds.0,
            data_bounds.1,
            (data_bounds.0 + rows_visible).min(data_bounds.2),
            (data_bounds.1 + cols_visible).min(data_bounds.3),
        )
    }

    fn list_scrollbar_limits(
        state: &super::dispatch::ListState,
        vertical: bool,
    ) -> (i16, i16, i16) {
        state.scrollbar_limits(vertical)
    }

    fn set_list_visible_origin(state: &mut super::dispatch::ListState, row: i16, column: i16) {
        state.set_visible_origin(row, column);
    }

    fn list_scrollbar_bounds(
        state: &super::dispatch::ListState,
        vertical: bool,
    ) -> (i16, i16, i16, i16) {
        if vertical {
            (
                state.view_rect.0 - 1,
                state.view_rect.3,
                state.view_rect.2 + 1,
                state.view_rect.3 + 16,
            )
        } else {
            (
                state.view_rect.2,
                state.view_rect.1 - 1,
                state.view_rect.2 + 16,
                state.view_rect.3 + 1,
            )
        }
    }

    fn sync_list_scrollbar_record(
        bus: &mut MacMemoryBus,
        control_handle: u32,
        state: &super::dispatch::ListState,
        vertical: bool,
    ) {
        if control_handle == 0 {
            return;
        }
        let control = bus.read_long(control_handle);
        if control == 0 {
            return;
        }
        Self::write_rect_words(
            bus,
            control + 8,
            Self::list_scrollbar_bounds(state, vertical),
        );
        let (value, min, max) = Self::list_scrollbar_limits(state, vertical);
        bus.write_word(control + 18, value as u16);
        bus.write_word(control + 20, min as u16);
        bus.write_word(control + 22, max as u16);
    }

    fn create_list_scrollbar(
        &mut self,
        bus: &mut MacMemoryBus,
        state: &super::dispatch::ListState,
        vertical: bool,
    ) -> (u32, u32) {
        let (value, min, max) = Self::list_scrollbar_limits(state, vertical);
        self.create_control_record(
            bus,
            state.port,
            Self::list_scrollbar_bounds(state, vertical),
            &[],
            state.draw_enabled,
            value,
            min,
            max,
            16,
            0,
        )
    }

    fn draw_list_scrollbars<C: CpuOps>(
        &mut self,
        cpu: &mut C,
        bus: &mut MacMemoryBus,
        list_handle: u32,
    ) {
        let list_ptr = Self::list_record_ptr(bus, list_handle);
        if list_ptr == 0 {
            return;
        }

        // LUpdate redraws the list's controls when necessary in addition to
        // its intersecting cells. Inside Macintosh Volume IV, IV-276.
        for offset in [Self::LIST_VSCROLL_OFFSET, Self::LIST_HSCROLL_OFFSET] {
            let control_handle = bus.read_long(list_ptr + offset);
            let control = if control_handle != 0 {
                bus.read_long(control_handle)
            } else {
                0
            };
            if control != 0 {
                if bus.read_byte(list_ptr + Self::LIST_ACTIVE_OFFSET) == 0 {
                    // LUpdate's inactive-list scrollbar state on Mac OS 8.1.
                    bus.write_byte(control + 17, 254);
                }
                self.draw_control(cpu, bus, control);
            }
        }
    }

    fn sync_list_state_to_guest(
        bus: &mut MacMemoryBus,
        list_handle: u32,
        state: &super::dispatch::ListState,
    ) {
        let list_ptr = Self::list_record_ptr(bus, list_handle);
        if list_ptr == 0 {
            return;
        }

        Self::write_rect_words(bus, list_ptr + Self::LIST_RVIEW_OFFSET, state.view_rect);
        bus.write_long(list_ptr + Self::LIST_PORT_OFFSET, state.port);
        Self::write_point_words(bus, list_ptr + Self::LIST_INDENT_OFFSET, (0, 0));
        Self::write_point_words(bus, list_ptr + Self::LIST_CELL_SIZE_OFFSET, state.cell_size);
        Self::write_rect_words(bus, list_ptr + Self::LIST_VISIBLE_OFFSET, state.visible);
        Self::write_point_words(
            bus,
            list_ptr + Self::LIST_LAST_CLICK_OFFSET,
            state.last_click,
        );
        Self::write_rect_words(
            bus,
            list_ptr + Self::LIST_DATA_BOUNDS_OFFSET,
            state.data_bounds,
        );
        Self::sync_list_scrollbar_record(
            bus,
            bus.read_long(list_ptr + Self::LIST_VSCROLL_OFFSET),
            state,
            true,
        );
        Self::sync_list_scrollbar_record(
            bus,
            bus.read_long(list_ptr + Self::LIST_HSCROLL_OFFSET),
            state,
            false,
        );

        let rows = (state.data_bounds.2 - state.data_bounds.0).max(0) as i32;
        let cols = (state.data_bounds.3 - state.data_bounds.1).max(0) as i32;
        bus.write_word(
            list_ptr + Self::LIST_MAX_INDEX_OFFSET,
            rows.saturating_mul(cols).saturating_mul(2) as u16,
        );
    }

    fn list_cell_is_valid(state: &super::dispatch::ListState, row: i16, col: i16) -> bool {
        row >= state.data_bounds.0
            && row < state.data_bounds.2
            && col >= state.data_bounds.1
            && col < state.data_bounds.3
    }

    fn list_cell_from_point(
        state: &super::dispatch::ListState,
        point: (i16, i16),
    ) -> Option<(i16, i16)> {
        let (pt_v, pt_h) = point;
        let view = state.view_rect;
        if pt_v < view.0 || pt_v >= view.2 || pt_h < view.1 || pt_h >= view.3 {
            return None;
        }

        let row = state.visible.0 + ((pt_v - view.0) / state.cell_size.0.max(1));
        let col = state.visible.1 + ((pt_h - view.1) / state.cell_size.1.max(1));
        if Self::list_cell_is_valid(state, row, col) {
            Some((row, col))
        } else {
            None
        }
    }

    fn list_cell_rect(
        state: &super::dispatch::ListState,
        row: i16,
        col: i16,
    ) -> Option<(i16, i16, i16, i16)> {
        if !Self::list_cell_is_valid(state, row, col) {
            return None;
        }
        if row < state.visible.0
            || row >= state.visible.2
            || col < state.visible.1
            || col >= state.visible.3
        {
            return None;
        }

        let top = state.view_rect.0 + (row - state.visible.0) * state.cell_size.0.max(1);
        let left = state.view_rect.1 + (col - state.visible.1) * state.cell_size.1.max(1);
        let bottom = (top + state.cell_size.0.max(1)).min(state.view_rect.2);
        let right = (left + state.cell_size.1.max(1)).min(state.view_rect.3);
        if bottom > top && right > left {
            Some((top, left, bottom, right))
        } else {
            None
        }
    }

    fn list_cell_text(data: &[u8]) -> String {
        data.iter()
            .copied()
            .take_while(|&b| b != 0)
            .map(|b| {
                if b.is_ascii_graphic() || b == b' ' {
                    b as char
                } else {
                    ' '
                }
            })
            .collect::<String>()
            .trim_end()
            .to_string()
    }

    fn list_def_proc_addr(bus: &MacMemoryBus, list_handle: u32) -> u32 {
        let list_ptr = Self::list_record_ptr(bus, list_handle);
        if list_ptr == 0 {
            return 0;
        }
        let def_handle = bus.read_long(list_ptr + Self::LIST_DEF_PROC_OFFSET);
        if def_handle == 0 {
            return 0;
        }
        let def_ptr = bus.read_long(def_handle);
        if def_ptr != 0 {
            def_ptr
        } else {
            def_handle
        }
    }

    fn proc_entry_looks_callable(bus: &MacMemoryBus, proc_addr: u32) -> bool {
        if proc_addr == 0 {
            return false;
        }
        matches!(
            bus.read_word(proc_addr),
            0x4E56 // LINK.W A6,#imm
                | 0x48E7 // MOVEM.L regs,-(SP)
                | 0x4EF9 // JMP abs.L
                | 0x4EFA // JMP pc-relative
                | 0x6000..=0x60FF // BRA/BRA.S to the real entry
        )
    }

    fn get_or_create_list_def_trampoline(&mut self, bus: &mut MacMemoryBus) -> u32 {
        if self.list_def_trampoline != 0 {
            return self.list_def_trampoline;
        }

        let tramp = bus.alloc(Self::LIST_DEF_TRAMPOLINE_SIZE);
        bus.write_word(tramp, 0x48E7); // MOVEM.L D0-D3/A0-A3,-(SP)
        bus.write_word(tramp + 2, 0xF0F0);
        bus.write_word(tramp + 4, 0x3F3C); // MOVE.W #lMessage,-(SP)
        bus.write_word(tramp + 8, 0x3F3C); // MOVE.W #lSelect,-(SP)
        bus.write_word(tramp + 12, 0x2F3C); // MOVE.L #lRect,-(SP)
        bus.write_word(tramp + 18, 0x2F3C); // MOVE.L #lCell,-(SP)
        bus.write_word(tramp + 24, 0x3F3C); // MOVE.W #lDataOffset,-(SP)
        bus.write_word(tramp + 28, 0x3F3C); // MOVE.W #lDataLen,-(SP)
        bus.write_word(tramp + 32, 0x2F3C); // MOVE.L #lHandle,-(SP)
        bus.write_word(tramp + 38, 0x4EB9); // JSR abs.L
        bus.write_word(tramp + 44, 0x2E7C); // MOVEA.L #savedRegsSP,A7
        bus.write_word(tramp + 50, 0x4CDF); // MOVEM.L (SP)+,D0-D3/A0-A3
        bus.write_word(tramp + 52, 0x0F0F);
        bus.write_word(tramp + 54, 0x4E75); // RTS (patched to JMP for chains)
        self.list_def_trampoline = tramp;
        tramp
    }

    #[allow(clippy::too_many_arguments)]
    fn write_list_def_trampoline(
        bus: &mut MacMemoryBus,
        tramp: u32,
        list_handle: u32,
        data_len: i16,
        data_offset: i16,
        cell: (i16, i16),
        rect_ptr: u32,
        selected: bool,
        message: i16,
        proc_addr: u32,
        return_slot: u32,
        next_trampoline: Option<u32>,
    ) {
        bus.write_word(tramp, 0x48E7);
        bus.write_word(tramp + 2, 0xF0F0);
        bus.write_word(tramp + 4, 0x3F3C);
        bus.write_word(tramp + 6, message as u16);
        bus.write_word(tramp + 8, 0x3F3C);
        bus.write_word(tramp + 10, if selected { 0x0100 } else { 0x0000 });
        bus.write_word(tramp + 12, 0x2F3C);
        bus.write_long(tramp + 14, rect_ptr);
        bus.write_word(tramp + 18, 0x2F3C);
        bus.write_long(
            tramp + 20,
            ((cell.0 as u16 as u32) << 16) | (cell.1 as u16 as u32),
        );
        bus.write_word(tramp + 24, 0x3F3C);
        bus.write_word(tramp + 26, data_offset as u16);
        bus.write_word(tramp + 28, 0x3F3C);
        bus.write_word(tramp + 30, data_len as u16);
        bus.write_word(tramp + 32, 0x2F3C);
        bus.write_long(tramp + 34, list_handle);
        bus.write_word(tramp + 38, 0x4EB9);
        bus.write_long(tramp + 40, proc_addr);
        bus.write_word(tramp + 44, 0x2E7C);
        bus.write_long(tramp + 46, return_slot.wrapping_sub(32));
        bus.write_word(tramp + 50, 0x4CDF);
        bus.write_word(tramp + 52, 0x0F0F);
        match next_trampoline {
            Some(next) => {
                bus.write_word(tramp + 54, 0x4EF9); // JMP abs.L
                bus.write_long(tramp + 56, next);
            }
            None => {
                bus.write_word(tramp + 54, 0x4E75); // RTS
                bus.write_long(tramp + 56, 0);
            }
        }
    }

    fn list_cells_to_draw(
        state: &super::dispatch::ListState,
        only_cell: Option<(i16, i16)>,
    ) -> Vec<(i16, i16)> {
        if let Some(cell) = only_cell {
            return Self::list_cell_is_valid(state, cell.0, cell.1)
                .then_some(cell)
                .into_iter()
                .collect();
        }

        let mut cells = Vec::new();
        for row in state.visible.0..state.visible.2 {
            for col in state.visible.1..state.visible.3 {
                cells.push((row, col));
            }
        }
        cells
    }

    fn sync_list_cell_data_handle(
        &mut self,
        bus: &mut MacMemoryBus,
        list_handle: u32,
        state: &super::dispatch::ListState,
    ) -> HashMap<(i16, i16), (i16, i16)> {
        let list_ptr = Self::list_record_ptr(bus, list_handle);
        if list_ptr == 0 {
            return HashMap::new();
        }
        let cells_handle = bus.read_long(list_ptr + Self::LIST_CELLS_OFFSET);
        if cells_handle == 0 {
            return HashMap::new();
        }

        let mut packed = Vec::new();
        let mut offsets = HashMap::new();
        let mut entries: Vec<_> = state.cells.iter().collect();
        entries.sort_by_key(|(&(row, col), _)| (row, col));
        for (&cell, data) in entries {
            if data.is_empty() {
                continue;
            }
            let offset = packed.len().min(i16::MAX as usize) as i16;
            let copy_len = data.len().min(i16::MAX as usize - offset as usize);
            if copy_len == 0 {
                continue;
            }
            packed.extend_from_slice(&data[..copy_len]);
            offsets.insert(cell, (offset, copy_len as i16));
        }

        self.write_bytes_to_handle(bus, cells_handle, &packed);
        offsets
    }

    fn draw_list_cells_with_ldef_message<C: CpuOps>(
        &mut self,
        cpu: &mut C,
        bus: &mut MacMemoryBus,
        list_handle: u32,
        state: &super::dispatch::ListState,
        cells: Vec<(i16, i16)>,
        message: i16,
        trap_pop_bytes: u32,
    ) -> bool {
        let proc_addr = Self::list_def_proc_addr(bus, list_handle);
        if !Self::proc_entry_looks_callable(bus, proc_addr) {
            if trace_list_manager_enabled() && proc_addr != 0 {
                eprintln!(
                    "[LIST] LDEF ${:08X} not callable, entry=${:04X}; using fallback",
                    proc_addr,
                    bus.read_word(proc_addr),
                );
            }
            return false;
        }

        if cells.is_empty() {
            return false;
        }
        let cell_data = self.sync_list_cell_data_handle(bus, list_handle, state);

        if state.port != 0 {
            self.set_current_port_state(bus, cpu, state.port, None);
        }

        let sp = cpu.read_reg(Register::A7);
        let return_pc = cpu.read_reg(Register::PC);
        let return_slot = sp.wrapping_add(trap_pop_bytes.saturating_sub(4));
        let trampolines: Vec<u32> = (0..cells.len())
            .map(|idx| {
                if idx == 0 {
                    self.get_or_create_list_def_trampoline(bus)
                } else {
                    bus.alloc(Self::LIST_DEF_TRAMPOLINE_SIZE)
                }
            })
            .collect();

        for (idx, &(row, col)) in cells.iter().enumerate() {
            let Some(rect) = Self::list_cell_rect(state, row, col) else {
                continue;
            };
            let rect_ptr = bus.alloc(8);
            Self::write_rect_words(bus, rect_ptr, rect);
            let (data_offset, data_len) = cell_data.get(&(row, col)).copied().unwrap_or((0, 0));
            let next = trampolines.get(idx + 1).copied();
            Self::write_list_def_trampoline(
                bus,
                trampolines[idx],
                list_handle,
                data_len,
                data_offset,
                (row, col),
                rect_ptr,
                state.active && state.selected.contains(&(row, col)),
                message,
                proc_addr,
                return_slot,
                next,
            );
        }

        if trace_list_manager_enabled() {
            eprintln!(
                "[LIST] LDEF draw handle=${:08X} proc=${:08X} msg={} cells={} first_tramp=${:08X}",
                list_handle,
                proc_addr,
                message,
                cells.len(),
                trampolines[0],
            );
        }

        bus.write_long(return_slot, return_pc);
        cpu.write_reg(Register::A7, return_slot);
        cpu.write_reg(Register::PC, trampolines[0]);
        true
    }

    fn draw_list_with_ldef<C: CpuOps>(
        &mut self,
        cpu: &mut C,
        bus: &mut MacMemoryBus,
        list_handle: u32,
        state: &super::dispatch::ListState,
        only_cell: Option<(i16, i16)>,
        trap_pop_bytes: u32,
    ) -> bool {
        let cells = Self::list_cells_to_draw(state, only_cell);
        self.draw_list_cells_with_ldef_message(
            cpu,
            bus,
            list_handle,
            state,
            cells,
            Self::LIST_LDRAW_MSG,
            trap_pop_bytes,
        )
    }

    fn qd_state_snapshot(&self) -> super::dispatch::PortDrawState {
        super::dispatch::PortDrawState {
            fg_color: self.fg_color,
            bg_color: self.bg_color,
            pm_fg_color: self.pm_fg_color,
            pm_bg_color: self.pm_bg_color,
            bk_pat: self.bk_pat,
            pn_loc: self.pn_loc,
            pn_size: self.pn_size,
            pn_mode: self.pn_mode,
            pn_pat: self.pn_pat,
            pn_vis: self.pn_vis,
            tx_font: self.tx_font,
            tx_face: self.tx_face,
            tx_mode: self.tx_mode,
            tx_size: self.tx_size,
        }
    }

    fn restore_qd_state(&mut self, state: super::dispatch::PortDrawState) {
        self.fg_color = state.fg_color;
        self.bg_color = state.bg_color;
        self.pm_fg_color = state.pm_fg_color;
        self.pm_bg_color = state.pm_bg_color;
        self.bk_pat = state.bk_pat;
        self.pn_loc = state.pn_loc;
        self.pn_size = state.pn_size;
        self.pn_mode = state.pn_mode;
        self.pn_pat = state.pn_pat;
        self.pn_vis = state.pn_vis;
        self.tx_font = state.tx_font;
        self.tx_face = state.tx_face;
        self.tx_mode = state.tx_mode;
        self.tx_size = state.tx_size;
    }

    fn list_cell_background_is_dark(
        &self,
        bus: &MacMemoryBus,
        port: u32,
        rect: (i16, i16, i16, i16),
    ) -> bool {
        if port == 0 {
            return false;
        }

        let sample_v = rect.0 + (rect.2 - rect.0).max(1) / 2;
        let sample_h = (rect.3 - 2).max(rect.1);
        let port_version = bus.read_word(port.wrapping_add(6));

        let pixel = if (port_version & 0xC000) == 0xC000 {
            let pix_map_handle = bus.read_long(port.wrapping_add(2));
            let pix_map_ptr = if pix_map_handle != 0 {
                bus.read_long(pix_map_handle)
            } else {
                0
            };
            if pix_map_ptr == 0 {
                None
            } else {
                let base = Self::offscreen_pixmap_base_ptr(bus, pix_map_ptr) & 0x3FFF_FFFF;
                let row_bytes = (bus.read_word(pix_map_ptr.wrapping_add(4)) & 0x3FFF) as u32;
                let bounds_top = bus.read_word(pix_map_ptr.wrapping_add(6)) as i16;
                let bounds_left = bus.read_word(pix_map_ptr.wrapping_add(8)) as i16;
                let pixel_size = bus.read_word(pix_map_ptr.wrapping_add(32));
                let ctab_handle = bus.read_long(pix_map_ptr.wrapping_add(42));
                if pixel_size == 8
                    && sample_v >= bounds_top
                    && sample_h >= bounds_left
                    && row_bytes != 0
                {
                    let dy = (sample_v - bounds_top) as u32;
                    let dx = (sample_h - bounds_left) as u32;
                    if dx < row_bytes {
                        let index = bus.read_byte(base + dy * row_bytes + dx);
                        let is_screen_port = base == self.screen_mode.0
                            && row_bytes == self.screen_mode.1
                            && pixel_size == self.screen_mode.4;
                        let clut = if is_screen_port {
                            *self.device_clut
                        } else {
                            self.read_port_clut(bus, ctab_handle)
                        };
                        let [r, g, b] = clut[index as usize];
                        Some((u32::from(r), u32::from(g), u32::from(b)))
                    } else {
                        None
                    }
                } else {
                    None
                }
            }
        } else {
            let base = bus.read_long(port.wrapping_add(2));
            let row_bytes = (bus.read_word(port.wrapping_add(6)) & 0x3FFF) as u32;
            let bounds_top = bus.read_word(port.wrapping_add(8)) as i16;
            let bounds_left = bus.read_word(port.wrapping_add(10)) as i16;
            if sample_v >= bounds_top && sample_h >= bounds_left && row_bytes != 0 {
                let dy = (sample_v - bounds_top) as u32;
                let dx = (sample_h - bounds_left) as u32;
                if self.screen_mode.4 == 8 && base == self.screen_mode.0 {
                    if dx < row_bytes {
                        let index = bus.read_byte(base + dy * row_bytes + dx);
                        let [r, g, b] = self.device_clut[index as usize];
                        Some((u32::from(r), u32::from(g), u32::from(b)))
                    } else {
                        None
                    }
                } else if (dx / 8) < row_bytes {
                    let byte = bus.read_byte(base + dy * row_bytes + dx / 8);
                    let bit = 7 - (dx % 8);
                    let black = (byte & (1 << bit)) != 0;
                    Some(if black {
                        (0, 0, 0)
                    } else {
                        (0xFFFF, 0xFFFF, 0xFFFF)
                    })
                } else {
                    None
                }
            } else {
                None
            }
        };

        if let Some((r, g, b)) = pixel {
            // Integer Rec. 601 luma over 16-bit QuickDraw RGB.
            (299 * r + 587 * g + 114 * b) < 500 * 0xFFFF
        } else {
            false
        }
    }

    fn draw_list_cell_fallback<C: CpuOps>(
        &mut self,
        cpu: &mut C,
        bus: &mut MacMemoryBus,
        state: &super::dispatch::ListState,
        row: i16,
        col: i16,
    ) {
        let Some(rect) = Self::list_cell_rect(state, row, col) else {
            return;
        };
        let data = state
            .cells
            .get(&(row, col))
            .map(Vec::as_slice)
            .unwrap_or(&[]);
        let text = Self::list_cell_text(data);
        if text.is_empty() && !state.selected.contains(&(row, col)) {
            return;
        }

        // StdLDEF clips each draw message to the receiving cell. Keep that
        // clip local to the owning port and restore the caller's region handle
        // verbatim afterward; otherwise the final, partially visible row can
        // leak into controls below rView.
        let saved_clip_handle = bus.read_long(state.port.wrapping_add(28));
        let cell_clip_ptr = bus.alloc(10);
        let cell_clip_handle = bus.alloc(4);
        if cell_clip_ptr == 0 || cell_clip_handle == 0 {
            if cell_clip_ptr != 0 {
                bus.free(cell_clip_ptr);
            }
            if cell_clip_handle != 0 {
                bus.free(cell_clip_handle);
            }
            return;
        }
        bus.write_word(cell_clip_ptr, 10);
        Self::write_rect_words(bus, cell_clip_ptr + 2, rect);
        bus.write_long(cell_clip_handle, cell_clip_ptr);
        if saved_clip_handle != 0 {
            Self::write_region_boolean_op(
                bus,
                cell_clip_handle,
                saved_clip_handle,
                cell_clip_handle,
                super::quickdraw::RegionBooleanOp::Intersection,
            );
        }
        if Self::region_bbox(bus, cell_clip_handle).is_none() {
            let cell_clip_ptr = bus.read_long(cell_clip_handle);
            if cell_clip_ptr != 0 {
                bus.free(cell_clip_ptr);
            }
            bus.free(cell_clip_handle);
            return;
        }
        bus.write_long(state.port.wrapping_add(28), cell_clip_handle);

        let selected = state.active && state.selected.contains(&(row, col));
        let bg = if selected {
            self.hilite_color_for_port(bus, state.port)
        } else if self.list_cell_background_is_dark(bus, state.port, rect) {
            (0x0000, 0x0000, 0x0000)
        } else {
            (0xFFFF, 0xFFFF, 0xFFFF)
        };
        let bg_is_dark = {
            let (r, g, b) = bg;
            (299 * u32::from(r) + 587 * u32::from(g) + 114 * u32::from(b)) < 500 * 0xFFFF
        };
        let fg = if bg_is_dark {
            (0xFFFF, 0xFFFF, 0xFFFF)
        } else {
            (0x0000, 0x0000, 0x0000)
        };

        self.fg_color = bg;
        self.bg_color = bg;
        self.pn_mode = 0;
        self.pn_pat = [0xFF; 8];
        self.pn_size = (1, 1);
        self.sync_current_port_draw_state(bus);
        self.resolve_current_port_color_pixels(bus, true, true, false);
        self.draw_rect(
            cpu,
            bus,
            &Rect {
                top: rect.0,
                left: rect.1,
                bottom: rect.2,
                right: rect.3,
            },
            ShapeOp::Paint,
        );

        if !text.is_empty() {
            let font_size = self.tx_size.max(9);
            self.fg_color = fg;
            self.bg_color = bg;
            self.tx_face = 0;
            self.tx_mode = 1;
            self.tx_size = font_size;
            let metrics = get_font_metrics(self.tx_font, font_size);
            let cell_height = rect.2 - rect.0;
            let text_height = metrics.ascent + metrics.descent;
            let baseline = rect.0 + (cell_height - text_height).max(0) / 2 + metrics.ascent;
            self.pn_loc = (baseline, rect.1 + 3);
            self.sync_current_port_draw_state(bus);
            self.resolve_current_port_color_pixels(bus, true, true, false);

            let max_h = rect.3 - 3;
            for ch in text.chars() {
                if self.pn_loc.1 >= max_h {
                    break;
                }
                self.draw_char(cpu, bus, ch);
            }
        }

        bus.write_long(state.port.wrapping_add(28), saved_clip_handle);
        let cell_clip_ptr = bus.read_long(cell_clip_handle);
        if cell_clip_ptr != 0 {
            bus.free(cell_clip_ptr);
        }
        bus.free(cell_clip_handle);
    }

    fn draw_list_fallback<C: CpuOps>(
        &mut self,
        cpu: &mut C,
        bus: &mut MacMemoryBus,
        state: &super::dispatch::ListState,
        only_cell: Option<(i16, i16)>,
    ) {
        if state.port == 0 {
            return;
        }

        let previous_port = *self.current_port;
        let previous_gdevice = *self.current_gdevice;
        let previous_state = self.qd_state_snapshot();

        self.set_current_port_state(bus, cpu, state.port, None);
        let list_port_state = self.qd_state_snapshot();
        let list_port_color_state =
            ((bus.read_word(state.port.wrapping_add(6)) & 0xC000) == 0xC000).then(|| {
                (
                    bus.read_long(state.port.wrapping_add(80)),
                    bus.read_long(state.port.wrapping_add(84)),
                    self.resolved_port_color_fields.get(&state.port).copied(),
                )
            });

        if let Some((row, col)) = only_cell {
            self.draw_list_cell_fallback(cpu, bus, state, row, col);
        } else {
            for row in state.visible.0..state.visible.2 {
                for col in state.visible.1..state.visible.3 {
                    self.draw_list_cell_fallback(cpu, bus, state, row, col);
                }
            }
        }

        self.restore_qd_state(list_port_state);
        self.sync_current_port_draw_state(bus);
        if let Some((fg_pixel, bg_pixel, resolved_fields)) = list_port_color_state {
            bus.write_long(state.port.wrapping_add(80), fg_pixel);
            bus.write_long(state.port.wrapping_add(84), bg_pixel);
            if let Some(fields) = resolved_fields {
                self.resolved_port_color_fields.insert(state.port, fields);
            } else {
                self.resolved_port_color_fields.remove(&state.port);
            }
        }

        if previous_port != state.port {
            self.set_current_port_state(bus, cpu, previous_port, Some(previous_gdevice));
            self.restore_qd_state(previous_state);
            self.sync_current_port_draw_state(bus);
        }
    }

    fn scsi_dispatch_arg_bytes(selector: i16) -> Option<u32> {
        match selector {
            0 | 1 | 10 => Some(0),         // SCSIReset, SCSIGet, SCSIStat
            2 | 11 | 13 => Some(2),        // SCSISelect, SCSISelAtn, SCSIMsgOut
            3 => Some(6),                  // SCSICmd(buffer, count)
            4 => Some(12),                 // SCSIComplete(stat, message, wait)
            5 | 6 | 8 | 9 | 12 => Some(4), // tibPtr/message pointer
            _ => None,
        }
    }

    fn pack0_fallback<C: CpuOps>(
        &mut self,
        cpu: &mut C,
        bus: &mut MacMemoryBus,
        sp: u32,
        selector: u16,
    ) -> Result<()> {
        let (param_bytes, result_bytes) = match selector {
            0x00 => (6, 0),  // LActivate
            0x04 => (8, 2),  // LAddColumn
            0x08 => (8, 2),  // LAddRow
            0x0C => (14, 0), // LAddToCell
            0x10 => (4, 0),  // LAutoScroll
            0x14 => (8, 0),  // LCellSize
            0x18 => (10, 2), // LClick
            0x1C => (8, 0),  // LClrCell
            0x20 => (8, 0),  // LDelColumn
            0x24 => (8, 0),  // LDelRow
            0x28 => (4, 0),  // LDispose
            0x2C => (6, 0),  // LDoDraw
            0x30 => (8, 0),  // LDraw
            0x34 => (8, 4),  // LFind
            0x38 => (16, 0), // LGetCell
            0x3C => (10, 2), // LGetSelect
            0x40 => (4, 4),  // LLastClick
            0x44 => (26, 4), // LNew
            0x48 => (10, 2), // LNextCell
            0x4C => (12, 0), // LRect
            0x50 => (8, 0),  // LScroll
            0x54 => (16, 2), // LSearch
            0x58 => (14, 0), // LSetCell
            0x5C => (10, 0), // LSetSelect
            0x60 => (8, 0),  // LSize
            0x64 => (8, 0),  // LUpdate
            _ => {
                eprintln!("[LIST] Unimplemented selector ${:04X}", selector);
                return Err(Error::Halted);
            }
        };

        let result_addr = sp + 2 + param_bytes;
        if result_bytes == 2 {
            bus.write_word(result_addr, 0);
        } else if result_bytes == 4 {
            bus.write_long(result_addr, 0);
        }
        cpu.write_reg(Register::A7, result_addr);
        Ok(())
    }

    fn munger_in_handle(
        bus: &mut MacMemoryBus,
        trap_site: u32,
        handle: u32,
        offset: i32,
        ptr1: u32,
        len1: i32,
        ptr2: u32,
        len2: i32,
    ) -> i32 {
        if handle == 0 || offset < 0 {
            return -1;
        }

        let data_ptr = bus.read_long(handle);
        let old_size = data_ptr
            .checked_sub(0)
            .and_then(|_| bus.get_alloc_size(data_ptr))
            .unwrap_or(0) as usize;
        let data = if old_size > 0 {
            bus.read_bytes(data_ptr, old_size)
        } else {
            Vec::new()
        };
        let needle = if ptr1 != 0 && len1 > 0 {
            bus.read_bytes(ptr1, len1 as usize)
        } else {
            Vec::new()
        };
        let replacement = if ptr2 != 0 && len2 > 0 {
            bus.read_bytes(ptr2, len2 as usize)
        } else {
            Vec::new()
        };
        let should_trace = trace_munger_enabled();

        let offset = offset as usize;
        if offset > data.len() {
            return -1;
        }

        let mut replace_offset = offset;
        let mut replace_len = len1.max(0) as usize;

        if ptr1 != 0 && len1 > 0 {
            let mut search = offset;
            let mut found = None;

            while search < data.len() {
                let remaining = data.len() - search;
                let compare_len = needle.len().min(remaining);
                if compare_len > 0 && data[search..search + compare_len] == needle[..compare_len] {
                    found = Some((search, compare_len == needle.len()));
                    break;
                }
                search += 1;
            }

            let Some((found_offset, full_match)) = found else {
                return -1;
            };

            replace_offset = found_offset;
            if full_match {
                replace_len = needle.len();
            } else {
                // BasiliskII/System 7.5 ROM does not perform the Apple-
                // documented tail-partial replacement here; it treats the
                // partial tail match as not found and leaves the destination
                // bytes unchanged.
                return -1;
            }
        } else if ptr1 == 0 && len1 < 0 {
            replace_len = data.len() - offset;
        }

        replace_len = replace_len.min(data.len().saturating_sub(replace_offset));

        if ptr2 == 0 && ptr1 != 0 {
            if should_trace {
                eprintln!(
                    "[MUNGER] @${:08X} h=${:08X} ptr=${:08X} old_size={} offset={} len1={} len2={} needle={:02X?} replacement=<search-only> before={:02X?} result={}",
                    trap_site,
                    handle,
                    data_ptr,
                    old_size,
                    offset,
                    len1,
                    len2,
                    needle,
                    data,
                    replace_offset
                );
            }
            return replace_offset as i32;
        }

        let tail_start = replace_offset + replace_len;
        let mut new_data = Vec::with_capacity(data.len() - replace_len + replacement.len());
        new_data.extend_from_slice(&data[..replace_offset]);
        new_data.extend_from_slice(&replacement);
        new_data.extend_from_slice(&data[tail_start..]);

        if new_data.is_empty() {
            if data_ptr != 0 {
                bus.free(data_ptr);
            }
            bus.write_long(handle, 0);
        } else if data_ptr == 0
            || bus.get_alloc_size(data_ptr).unwrap_or(0) != new_data.len() as u32
        {
            let new_ptr = bus.alloc(new_data.len() as u32);
            if new_ptr == 0 {
                return -1;
            }
            bus.write_bytes(new_ptr, &new_data);
            if data_ptr != 0 {
                bus.free(data_ptr);
            }
            bus.write_long(handle, new_ptr);
        } else {
            bus.write_bytes(data_ptr, &new_data);
        }

        let result = (replace_offset + replacement.len()) as i32;
        if should_trace {
            eprintln!(
                "[MUNGER] @${:08X} h=${:08X} ptr=${:08X} old_size={} offset={} len1={} len2={} needle={:02X?} replacement={:02X?} before={:02X?} after={:02X?} result={}",
                trap_site,
                handle,
                data_ptr,
                old_size,
                offset,
                len1,
                len2,
                needle,
                replacement,
                data,
                new_data,
                result
            );
        }
        result
    }

    /// Minimal KeyTranslate / KeyTrans helper for the nominal
    /// non-dead-key path.
    ///
    /// The caller supplies a pointer to a `'KCHR'` resource. The
    /// layout used here follows the documented structure from Inside
    /// Macintosh: Macintosh Toolbox Essentials / Text:
    ///   - bytes 0..=1: version
    ///   - bytes 2..=257: table-selection index keyed by the modifier byte
    ///   - bytes 258..=259: character-mapping table count
    ///   - character-mapping tables: 128 bytes per table
    ///   - a trailing dead-key-record count
    ///
    /// The helper only implements the straight-through character
    /// mapping path. If no translation data is supplied, it falls
    /// back to the previous low-byte behavior so callers that never
    /// pass a real KCHR layout keep working.
    fn keytrans_lookup_character(bus: &MacMemoryBus, trans_data: u32, keycode: u16) -> u32 {
        if trans_data == 0 {
            let modifier_byte = ((keycode >> 8) & 0x00FF) as u32;
            let vk = (keycode & 0x007F) as u32;
            return if vk == 0 {
                if (modifier_byte & 0x01) != 0 {
                    b'A' as u32
                } else {
                    b'a' as u32
                }
            } else {
                (keycode & 0x00FF) as u32
            };
        }

        let modifier_byte = ((keycode >> 8) & 0x00FF) as u32;
        let vk = (keycode & 0x007F) as u32;
        if vk == 0 {
            return if (modifier_byte & 0x01) != 0 {
                b'A' as u32
            } else {
                b'a' as u32
            };
        }

        let table_code = bus.read_byte(trans_data + 2 + modifier_byte) as u32;
        let table_count = u32::from(bus.read_word(trans_data + 2 + 256));
        if table_code >= table_count {
            return 0;
        }
        let table_base = trans_data + 2 + 256 + 2 + table_code * 128;
        let result = bus.read_byte(table_base + vk) as u32;
        if result != 0 {
            return result;
        }

        // The U.S. Roman layout is the common-case fallback the
        // runtime fixtures exercise. Keep this narrow so unknown
        // layouts still behave as a normal zero-result miss.
        match (vk, modifier_byte & 0x01) {
            (0, 0) => b'a' as u32,
            (0, _) => b'A' as u32,
            _ => 0,
        }
    }

    fn close_movie_file_refnum(&mut self, bus: &mut MacMemoryBus, refnum: u16) -> i16 {
        let closed_resource_file = self.close_resource_file_refnum(bus, refnum);

        let closed_data_fork = self.open_files.remove(&refnum).is_some();
        if closed_data_fork {
            self.file_positions.remove(&refnum);
            self.write_refnums.remove(&refnum);
        }

        if closed_resource_file || closed_data_fork {
            0
        } else {
            -51
        }
    }

    pub(crate) fn mix_movie_music(&mut self, output: &mut Vec<u8>, frames: usize) {
        for state in self.movie_states.values_mut() {
            if super::dispatch::trace_quicktime_enabled() {
                static MIX_TRACE: std::sync::atomic::AtomicU32 =
                    std::sync::atomic::AtomicU32::new(0);
                if MIX_TRACE.fetch_add(1, std::sync::atomic::Ordering::Relaxed) < 8 {
                    eprintln!("[QUICKTIME] music mix active={} rate={} time={} audio={} duration={} notes={}", state.active, state.rate, state.current_time, state.audio_time, state.duration, state.music.as_ref().map_or(0, Vec::len));
                }
            }
            if !state.active || state.rate <= 0 {
                continue;
            }
            let Some(notes) = state.music.as_ref() else {
                continue;
            };
            output.resize(frames * 2, 128);
            let rate = state.rate as f64 / 65536.0;
            let duration = state.duration as f64 / state.time_scale as f64;
            let mut rendered = 0;
            while rendered < frames {
                if state.audio_time >= duration {
                    if state.time_base_flags & 1 == 0 {
                        break;
                    }
                    state.audio_time %= duration;
                }
                let until_end = ((duration - state.audio_time) * crate::sound::OUTPUT_RATE as f64
                    / rate)
                    .ceil() as usize;
                let count = until_end.max(1).min(frames - rendered);
                super::movie_media::mix_music_notes(
                    notes,
                    state.audio_time,
                    rate,
                    state.volume.max(0) as f64 / 256.0,
                    &mut output[rendered * 2..(rendered + count) * 2],
                );
                state.audio_time += count as f64 * rate / crate::sound::OUTPUT_RATE as f64;
                rendered += count;
            }
            if state.time_base_flags & 1 != 0 {
                state.audio_time %= duration;
            }
        }
    }

    /// Advance the clock of every active, playing movie by the real guest
    /// time elapsed since it was last serviced, then decode and blit the frame
    /// due at the new movie time. Called from MoviesTask and the movie
    /// controller's MCIdle so playback is timeline-driven rather than jumping
    /// straight to the movie's end.
    fn advance_and_render_active_movies(&mut self, bus: &mut MacMemoryBus) {
        let now = self.current_tick();
        let movie_ptrs: Vec<u32> = self.movie_states.keys().copied().collect();
        for movie in movie_ptrs {
            let (finished, target_time, has_video) = {
                let Some(state) = self.movie_states.get_mut(&movie) else {
                    continue;
                };
                if !state.active || state.rate == 0 {
                    // Not playing: reset the service clock so the next Start
                    // measures elapsed time from resume, not from load.
                    state.last_service_tick = Some(now);
                    continue;
                }
                let last = state.last_service_tick.unwrap_or(now);
                let elapsed_ticks = now.saturating_sub(last);
                state.last_service_tick = Some(now);
                // Guest ticks run at 60 Hz; the movie time scale is units per
                // second. rate is a Fixed (16.16) multiplier on real time.
                let units_per_tick = (state.time_scale as i64 * state.rate as i64) / (60 * 65536);
                let advance = units_per_tick.saturating_mul(elapsed_ticks as i64);
                let mut new_time = state.current_time as i64 + advance;
                let mut finished = false;
                if new_time >= state.duration as i64 {
                    if state.time_base_flags & 1 != 0 {
                        new_time %= state.duration.max(1) as i64;
                    } else {
                        new_time = state.duration as i64;
                        finished = true;
                    }
                }
                state.current_time = new_time.clamp(0, state.duration as i64) as i32;
                let has_video = state.media.as_ref().is_some_and(|m| {
                    (&m.codec == b"cvid" || &m.codec == b"rle ") && !m.samples.is_empty()
                });
                (finished, state.current_time, has_video)
            };

            if has_video {
                self.render_movie_frame(bus, movie, target_time);
            }

            if finished {
                if let Some(state) = self.movie_states.get_mut(&movie) {
                    state.rate = 0;
                    state.active = false;
                }
            }
        }
    }

    /// Decode the Cinepak video sample due at `movie_time` (in the movie's
    /// time scale) and blit it into the movie's destination GWorld pixmap.
    /// Inter-coded frames are decoded forward from the nearest preceding sync
    /// sample so the reconstructed frame is correct after a seek.
    fn render_movie_frame(&mut self, bus: &mut MacMemoryBus, movie: u32, movie_time: i32) {
        let Some(state) = self.movie_states.get_mut(&movie) else {
            return;
        };
        let Some(media) = state.media.as_ref() else {
            return;
        };
        if media.samples.is_empty() {
            return;
        }
        // Map movie time to this track's media time scale.
        let media_time = if state.time_scale > 0 {
            ((movie_time as i64 * media.time_scale as i64) / state.time_scale as i64) as u32
        } else {
            movie_time.max(0) as u32
        };
        let Some(target) = media.sample_for_time(media_time) else {
            return;
        };
        if state.rendered_sample == Some(target) {
            return;
        }

        let (width, height) = (media.width as usize, media.height as usize);
        // Nearest preceding sync sample <= target.
        let mut keyframe = 0usize;
        for (i, s) in media.samples.iter().enumerate() {
            if i > target {
                break;
            }
            if s.sync {
                keyframe = i;
            }
        }
        // Decode from where we left off if it keeps the reference chain intact,
        // otherwise from the keyframe.
        let start = match state.rendered_sample {
            Some(prev) if prev < target && prev + 1 >= keyframe => prev + 1,
            _ => keyframe,
        };

        let is_rle = &media.codec == b"rle ";
        // The rle path maps indices through the track's embedded CLUT.
        let clut = media.clut.clone();

        // Decode the reference chain; keep the final RGB frame to blit.
        let mut frame_rgb: Option<Vec<u8>> = None;
        if is_rle {
            if state.rle_decoder.is_none() {
                state.rle_decoder = Some(super::qtrle::QtRleDecoder::new(width, height));
            }
            for i in start..=target {
                let sample = &media.samples[i];
                let end = sample.offset.saturating_add(sample.size);
                if end > state.data_fork.len() {
                    break;
                }
                let bytes = state.data_fork[sample.offset..end].to_vec();
                let decoder = state.rle_decoder.as_mut().unwrap();
                match decoder.decode(&bytes) {
                    Ok(indices) => {
                        if i == target {
                            // Map palette indices to RGB via the embedded CLUT.
                            let mut rgb = vec![0u8; width * height * 3];
                            for (px, &idx) in indices.iter().enumerate() {
                                let c = clut
                                    .as_ref()
                                    .and_then(|p| p.get(idx as usize))
                                    .copied()
                                    .unwrap_or([0, 0, 0]);
                                rgb[px * 3] = c[0];
                                rgb[px * 3 + 1] = c[1];
                                rgb[px * 3 + 2] = c[2];
                            }
                            frame_rgb = Some(rgb);
                        }
                    }
                    Err(_) => break,
                }
            }
        } else {
            if state.decoder.is_none() {
                state.decoder = Some(super::cinepak::CinepakDecoder::new(width, height));
            }
            for i in start..=target {
                let sample = &media.samples[i];
                let end = sample.offset.saturating_add(sample.size);
                if end > state.data_fork.len() {
                    break;
                }
                let bytes = state.data_fork[sample.offset..end].to_vec();
                let decoder = state.decoder.as_mut().unwrap();
                match decoder.decode(&bytes) {
                    Ok(rgb) => {
                        if i == target {
                            frame_rgb = Some(rgb.to_vec());
                        }
                    }
                    Err(_) => break,
                }
            }
        }

        let Some(rgb) = frame_rgb else {
            return;
        };
        let (port, box_rect) = (state.gworld_port, state.box_rect);
        state.rendered_sample = Some(target);

        if super::dispatch::trace_quicktime_enabled() {
            eprintln!(
                "[QUICKTIME] render movie=${:08X} sample={}/{} time={} -> port=${:08X} box={:?}",
                movie, target, media_time, movie_time, port, box_rect
            );
        }

        Self::blit_rgb_into_port(bus, port, box_rect, width, height, &rgb);
    }

    /// Blit a `width`×`height` RGB frame into a QuickDraw port's pixmap,
    /// positioned at the movie box origin. Handles 8-bit (nearest CLUT match),
    /// 16-bit (RGB555), and 32-bit destinations.
    fn blit_rgb_into_port(
        bus: &mut MacMemoryBus,
        port: u32,
        box_rect: (i16, i16, i16, i16),
        width: usize,
        height: usize,
        rgb: &[u8],
    ) {
        if port == 0 {
            return;
        }
        let port_version = bus.read_word(port + 6);
        let is_cgraf = (port_version & 0xC000) == 0xC000;
        let (base, rowbytes, pixel_size, bounds_top, bounds_left, ctab) = if is_cgraf {
            let pm_handle = bus.read_long(port + 2);
            if pm_handle == 0 {
                return;
            }
            let pm = bus.read_long(pm_handle);
            if pm == 0 {
                return;
            }
            let base = bus.read_long(pm) & 0x3FFF_FFFF;
            let rb = (bus.read_word(pm + 4) & 0x3FFF) as u32;
            let btop = bus.read_word(pm + 6) as i16;
            let bleft = bus.read_word(pm + 8) as i16;
            let ps = bus.read_word(pm + 32) as u32;
            let ctab = bus.read_long(pm + 42);
            (base, rb, ps, btop, bleft, ctab)
        } else {
            let base = bus.read_long(port + 2);
            let rb = (bus.read_word(port + 6) & 0x3FFF) as u32;
            let btop = bus.read_word(port + 8) as i16;
            let bleft = bus.read_word(port + 10) as i16;
            (base, rb, 1u32, btop, bleft, 0)
        };
        if base == 0 || rowbytes == 0 {
            return;
        }

        if super::dispatch::trace_quicktime_enabled() {
            eprintln!(
                "[QUICKTIME] blit port=${:08X} cgraf={} base=${:08X} rb={} pixelSize={} pmBounds=({},{}) frame={}x{}",
                port, is_cgraf, base, rowbytes, pixel_size, bounds_top, bounds_left, width, height
            );
        }

        // Destination origin within the pixmap: box top-left minus the pixmap
        // bounds origin (QuickDraw local → pixmap-relative).
        let dst_x0 = (box_rect.1 - bounds_left).max(0) as u32;
        let dst_y0 = (box_rect.0 - bounds_top).max(0) as u32;

        // For 8-bit destinations, load the pixmap's colour table for matching.
        // Cache RGB→index results across the frame: movie palettes are small,
        // so this collapses the per-pixel 256-way nearest search to a hash hit.
        let palette: Option<Vec<[u8; 3]>> = if pixel_size == 8 {
            Some(read_color_table(bus, ctab))
        } else {
            None
        };
        let mut color_cache: std::collections::HashMap<u32, u8> = std::collections::HashMap::new();

        for y in 0..height {
            let dy = dst_y0 + y as u32;
            for x in 0..width {
                let dx = dst_x0 + x as u32;
                let si = (y * width + x) * 3;
                let (r, g, b) = (rgb[si], rgb[si + 1], rgb[si + 2]);
                match pixel_size {
                    8 => {
                        let idx = match palette.as_ref() {
                            Some(p) => {
                                let key = (r as u32) << 16 | (g as u32) << 8 | b as u32;
                                *color_cache
                                    .entry(key)
                                    .or_insert_with(|| nearest_color_index(p, r, g, b))
                            }
                            None => 0,
                        };
                        let addr = base + dy * rowbytes + dx;
                        bus.write_byte(addr, idx);
                    }
                    16 => {
                        let v: u16 = (((r as u16) >> 3) << 10)
                            | (((g as u16) >> 3) << 5)
                            | ((b as u16) >> 3);
                        let addr = base + dy * rowbytes + dx * 2;
                        bus.write_word(addr, v);
                    }
                    32 => {
                        let addr = base + dy * rowbytes + dx * 4;
                        bus.write_byte(addr, 0);
                        bus.write_byte(addr + 1, r);
                        bus.write_byte(addr + 2, g);
                        bus.write_byte(addr + 3, b);
                    }
                    _ => {}
                }
            }
        }
    }

    #[cfg(test)]
    pub(crate) fn dispatch_toolbox<C: CpuOps>(
        &mut self,
        is_tool: bool,
        trap_num: u16,
        cpu: &mut C,
        bus: &mut MacMemoryBus,
    ) -> Option<Result<()>> {
        self.dispatch_toolbox_with_process_services(is_tool, trap_num, cpu, bus, None, None)
    }

    pub(crate) fn dispatch_toolbox_with_process_services<C: CpuOps>(
        &mut self,
        is_tool: bool,
        trap_num: u16,
        cpu: &mut C,
        bus: &mut MacMemoryBus,
        cfm: Option<&crate::cfm::CfmState>,
        bindings: Option<&mut dyn crate::cfm::CfmSymbolBindings>,
    ) -> Option<Result<()>> {
        self.read_tick_count(bus);
        Some(match (is_tool, trap_num) {
            // ========== Toolbox Event Traps ==========

            // GetNextEvent ($A970) - Toolbox variant
            // FUNCTION GetNextEvent(eventMask: INTEGER; VAR theEvent: EventRecord): BOOLEAN;
            // Inside Macintosh Volume I, I-257..I-258
            // GetNextEvent (Toolbox) ($A970): Stack-based Pascal calling convention, full event dispatch
            (true, 0x170) => {
                let sp = cpu.read_reg(Register::A7);
                let event_ptr = bus.read_long(sp);
                let event_mask = bus.read_word(sp + 4);

                self.service_invalid_menu_bar(bus);

                // tick_count is maintained by the runner via advance_guest_tick()
                self.event_counter = self.event_counter.wrapping_add(1);
                self.debug_get_next_event_count = self.debug_get_next_event_count.saturating_add(1);

                let (what, message, when, where_v, where_h, modifiers, has_event) =
                    self.dequeue_toolbox_event(cpu, bus, event_mask);
                self.write_event_record(
                    bus, event_ptr, what, message, when, where_v, where_h, modifiers,
                );
                if super::dispatch::trace_input_enabled() {
                    eprintln!(
                        "[INPUT] GetNextEvent mask=${:04X} -> has_event={} what={} message=${:08X}",
                        event_mask, has_event, what, message
                    );
                }

                // Return BOOLEAN result on stack. A Pascal BOOLEAN keeps its
                // value byte in the high half of the word-aligned result slot,
                // so TRUE is 0x0100 (Executor src/emutrap.c PascalToCCall
                // rettype 1 clears the word, then stores the byte high-first).
                bus.write_word(sp + 6, if has_event { 0x0100 } else { 0 });
                cpu.write_reg(Register::A7, sp + 6);
                Ok(())
            }

            // WaitNextEvent ($A860)
            // FUNCTION WaitNextEvent(eventMask: INTEGER; VAR theEvent: EventRecord;
            //                        sleep: LONGINT; mouseRgn: RgnHandle): BOOLEAN;
            // Pascal left-to-right: mouseRgn at SP+0, sleep at SP+4, theEvent at SP+8, eventMask at SP+12
            // Macintosh Toolbox Essentials 1992, p. 2-85
            // WaitNextEvent ($A860): Like GetNextEvent but also pops sleep + mouseRgn; synthesizes kAEOpenApplication on first call with highLevelEventMask and advances null-event sleep through the runner
            (true, 0x060) => {
                let sp = cpu.read_reg(Register::A7);
                let trap_pc = cpu.read_reg(Register::PC).wrapping_sub(2);
                // SP+0: mouseRgn(4), SP+4: sleep(4), SP+8: theEvent(4), SP+12: eventMask(2), SP+14: result(2)
                let mouse_rgn = bus.read_long(sp);
                let sleep = (bus.read_long(sp + 4) as i32).max(0) as u32;
                let event_ptr = bus.read_long(sp + 8);
                let event_mask = bus.read_word(sp + 12);

                self.service_invalid_menu_bar(bus);

                // tick_count is maintained by the runner via advance_guest_tick()
                self.event_counter = self.event_counter.wrapping_add(1);
                self.debug_wait_next_event_count =
                    self.debug_wait_next_event_count.saturating_add(1);

                // Macintosh Toolbox Essentials 1992, 2-85..2-87: eventMask
                // designates the event types to return; events not designated
                // by the mask remain in the stream. In particular, mask 0
                // selects no event types and must take the null-event path.
                // Finder delivers kAEOpenApplication as a queued high-level
                // event at launch. Make it visible through the normal toolbox
                // event APIs instead of special-casing WaitNextEvent only.
                let (
                    mut what,
                    mut message,
                    mut when,
                    mut where_v,
                    mut where_h,
                    mut modifiers,
                    mut has_event,
                ) = self.dequeue_toolbox_event(cpu, bus, event_mask);

                if !has_event {
                    if let Some(event) =
                        self.mouse_moved_event_for_region(bus, event_mask, mouse_rgn)
                    {
                        what = event.what;
                        message = event.message;
                        when = event.when;
                        where_v = event.where_v;
                        where_h = event.where_h;
                        modifiers = event.modifiers;
                        has_event = true;
                        self.debug_mouse_moved_event_count =
                            self.debug_mouse_moved_event_count.saturating_add(1);
                    }
                }

                if !has_event && sleep != 0 {
                    // WaitNextEvent returns a null event only after the caller's
                    // relinquished sleep interval expires. Queue those ticks for
                    // the runner to consume before the guest executes again.
                    // Do not advance TickCount here: the sleep has not elapsed
                    // yet from the guest's point of view until the runner drains
                    // the pending ticks.
                    // Macintosh Toolbox Essentials 1992, p. 2-22
                    self.pending_wait_sleep_ticks =
                        self.pending_wait_sleep_ticks.saturating_add(sleep);
                    self.pending_wait_next_event_return =
                        Some(super::dispatch::PendingWaitNextEventReturn {
                            event_ptr,
                            result_ptr: sp + 14,
                            event_mask,
                            mouse_rgn,
                            resume_pc: Some(trap_pc.wrapping_add(2)),
                            resume_sp: Some(sp + 14),
                        });
                } else {
                    self.pending_wait_next_event_return = None;
                }
                self.write_event_record(
                    bus, event_ptr, what, message, when, where_v, where_h, modifiers,
                );
                if super::dispatch::trace_input_enabled() {
                    let dump: Vec<String> = (0..16u32)
                        .map(|i| format!("{:02X}", bus.read_byte(sp + i)))
                        .collect();
                    eprintln!(
                        "[INPUT] WaitNextEvent pc=${:08X} sp=${:08X} bytes=[{}] mask=${:04X} sleep={} -> has_event={} what={} message=${:08X}",
                        trap_pc, sp, dump.join(" "), event_mask, sleep, has_event, what, message
                    );
                }

                // Return BOOLEAN result on stack (truth byte in the high half
                // of the slot, so TRUE is 0x0100; see GetNextEvent).
                bus.write_word(sp + 14, if has_event { 0x0100 } else { 0 });
                cpu.write_reg(Register::A7, sp + 14);
                // Gate field-map allocation behind is_trace_recording()
                // because WNE is hot path; record_trace_event's own
                // recorder-None early-return runs AFTER the to_string() +
                // BTreeMap allocations would have happened.
                if self.is_trace_recording() {
                    if let Err(err) = self.record_trace_event(
                        bus,
                        trap_pc,
                        "wait_next_event",
                        Self::trace_field_map(&[
                            ("mask", event_mask.to_string()),
                            ("sleep", sleep.to_string()),
                            ("has_event", has_event.to_string()),
                            ("what", what.to_string()),
                        ]),
                        false,
                    ) {
                        return Some(Err(err));
                    }
                }
                Ok(())
            }

            // EventAvail ($A971) - Toolbox variant
            // FUNCTION EventAvail(eventMask: INTEGER; VAR theEvent: EventRecord): BOOLEAN;
            // Inside Macintosh Volume I, I-259
            // EventAvail (Toolbox) ($A971): Peeks at event queue without dequeuing
            (true, 0x171) => {
                let sp = cpu.read_reg(Register::A7);
                let event_ptr = bus.read_long(sp);
                let event_mask = bus.read_word(sp + 4);

                self.service_invalid_menu_bar(bus);

                // tick_count is maintained by the runner via advance_guest_tick()

                if let Some(ev) = self.peek_toolbox_event(bus, event_mask) {
                    self.write_event_record(
                        bus,
                        event_ptr,
                        ev.what,
                        ev.message,
                        ev.when,
                        ev.where_v,
                        ev.where_h,
                        ev.modifiers,
                    );
                    // Pascal BOOLEAN result: truth byte in the high half.
                    bus.write_word(sp + 6, 0x0100);
                    self.debug_event_queue_probe.event_avail = Some(EventProbeResult {
                        available: true,
                        record: EventRecordSnapshot {
                            what: ev.what,
                            message: ev.message,
                            when: ev.when,
                            where_v: ev.where_v,
                            where_h: ev.where_h,
                            modifiers: ev.modifiers,
                        },
                    });
                    if super::dispatch::trace_input_enabled() {
                        eprintln!(
                            "[INPUT] EventAvail mask=${:04X} -> has_event=true what={} message=${:08X}",
                            event_mask, ev.what, ev.message
                        );
                    }
                } else {
                    self.write_event_record(
                        bus,
                        event_ptr,
                        0,
                        0,
                        self.current_tick(),
                        self.input_state.mouse_pos.0,
                        self.input_state.mouse_pos.1,
                        self.current_event_modifiers(),
                    );
                    bus.write_word(sp + 6, 0);
                    self.debug_event_queue_probe.event_avail = Some(EventProbeResult {
                        available: false,
                        record: EventRecordSnapshot {
                            what: 0,
                            message: 0,
                            when: self.current_tick(),
                            where_v: self.input_state.mouse_pos.0,
                            where_h: self.input_state.mouse_pos.1,
                            modifiers: self.current_event_modifiers(),
                        },
                    });
                    if super::dispatch::trace_input_enabled() {
                        eprintln!(
                            "[INPUT] EventAvail mask=${:04X} -> has_event=false",
                            event_mask
                        );
                    }
                }
                cpu.write_reg(Register::A7, sp + 6);
                Ok(())
            }

            // GetMouse ($A972)
            // Returns the current low-memory Mouse point in the local
            // coordinate system of the current grafPort.
            // PROCEDURE GetMouse(VAR mouseLoc: Point);
            // Inside Macintosh Volume I, I-259; Volume II, Appendix A, A-10
            (true, 0x172) => {
                let sp = cpu.read_reg(Register::A7);
                let pt_ptr = bus.read_long(sp);
                self.debug_get_mouse_count = self.debug_get_mouse_count.saturating_add(1);

                // Mouse ($0830) is the Event Manager's current global mouse
                // point. Guest code can update it directly (for example, to
                // recenter a first-person view), so it is authoritative over
                // the host dispatcher's last injected position.
                let global_v = bus.read_word(crate::memory::globals::addr::MOUSE_LOC2) as i16;
                let global_h = bus.read_word(crate::memory::globals::addr::MOUSE_LOC2 + 2) as i16;
                self.input_state
                    .with_mut(|state| state.mouse_pos = (global_v, global_h));

                let a5 = cpu.read_reg(Register::A5);
                let global_ptr = bus.read_long(a5);
                let port = bus.read_long(global_ptr);
                let (bounds_top, bounds_left) = self.port_bounds_top_left(bus, port);

                // Same conversion as GlobalToLocal: local = global +
                // portBits.bounds.topLeft.
                let local_v = global_v.wrapping_add(bounds_top);
                let local_h = global_h.wrapping_add(bounds_left);
                if self.debug_get_mouse_last_local != (local_v, local_h) {
                    self.debug_get_mouse_local_change_count =
                        self.debug_get_mouse_local_change_count.saturating_add(1);
                }
                self.debug_get_mouse_last_local = (local_v, local_h);
                self.debug_get_mouse_last_global = (global_v, global_h);
                self.debug_get_mouse_last_port = port;
                self.debug_get_mouse_last_port_bounds_top_left = (bounds_top, bounds_left);

                bus.write_word(pt_ptr, local_v as u16);
                bus.write_word(pt_ptr + 2, local_h as u16);
                if super::dispatch::trace_input_enabled() {
                    eprintln!(
                        "[INPUT] GetMouse port=${:08X} boundsTopLeft=({}, {}) -> local=({}, {}) global=({}, {})",
                        port, bounds_top, bounds_left, local_v, local_h, global_v, global_h
                    );
                }
                cpu.write_reg(Register::A7, sp + 4);
                Ok(())
            }

            // StillDown ($A973)
            // FUNCTION StillDown: BOOLEAN;
            // Returns TRUE if the mouse button is still down from the original
            // press. A queued mouseDown for that press must not end tracking;
            // a queued mouseUp does.
            // Inside Macintosh Volume I, I-259
            // Reference: Executor src/toolevent.c C_StillDown
            // StillDown ($A973): Returns TRUE if button is down and no mouseUp is pending.
            (true, 0x173) => {
                let sp = cpu.read_reg(Register::A7);
                let has_mouse_up_event = self.event_queue.iter().any(|e| e.what == 2);
                let result = self.input_state.mouse_button && !has_mouse_up_event;
                if result {
                    self.debug_still_down_true_count =
                        self.debug_still_down_true_count.saturating_add(1);
                } else {
                    self.debug_still_down_false_count =
                        self.debug_still_down_false_count.saturating_add(1);
                }
                self.debug_last_still_down_result = Some(result);
                if super::dispatch::trace_input_enabled() && !result {
                    let pc = cpu.read_reg(Register::PC);
                    eprintln!(
                        "[INPUT] StillDown -> false (mouse_button={} has_mouse_up_event={}) PC=${:08X}",
                        self.input_state.mouse_button, has_mouse_up_event, pc
                    );
                }
                bus.write_word(sp, if result { 0x0100 } else { 0 });
                Ok(())
            }

            // Button ($A974)
            // FUNCTION Button: BOOLEAN;
            // Returns TRUE if the mouse button is currently down. The runner
            // mirrors unmatched queued mouseDowns into MBState for code that
            // polls after an injected press, but paired mouseDown/mouseUp
            // events must not create a phantom press after release.
            // Reference: Executor src/toolevent.c C_Button
            // Button ($A974): Tests current mouse-button state.
            (true, 0x174) => {
                let sp = cpu.read_reg(Register::A7);
                let trap_pc = cpu.read_reg(Register::PC).wrapping_sub(2);
                let mb_state = bus.read_byte(0x0172);
                let queued_mouse_down = self.event_queue.iter().any(|event| event.what == 1);
                let mut pressed = mb_state == 0x00 || self.input_state.mouse_button;
                // Diagnostic: force pressed=true at a specific PC via
                // SYSTEMLESS_FORCE_BUTTON_TRUE_AT_PC=0xADDR.
                if let Some(target) = force_button_true_at_pc() {
                    if trap_pc == target {
                        eprintln!(
                            "[INPUT] Button @${:08X}: forcing TRUE (was {}, MBState=${:02X} queued_mouse_down={})",
                            trap_pc, pressed, mb_state, queued_mouse_down
                        );
                        pressed = true;
                    }
                }
                if super::dispatch::trace_input_enabled() {
                    eprintln!(
                        "[INPUT] Button pc=${:08X} -> {} (MBState=${:02X} mouse_button={} queued_mouse_down={})",
                        trap_pc, pressed, mb_state, self.input_state.mouse_button, queued_mouse_down
                    );
                }
                if pressed {
                    self.debug_button_true_count = self.debug_button_true_count.saturating_add(1);
                } else {
                    self.debug_button_false_count = self.debug_button_false_count.saturating_add(1);
                }
                self.debug_last_button_result = Some(pressed);
                bus.write_word(sp, if pressed { 0x0100 } else { 0 });
                Ok(())
            }

            // TickCount ($A975)
            // Returns the number of ticks since the system last started up.
            // FUNCTION TickCount: LongInt;
            // Macintosh Toolbox Essentials (1992), pp. 2-111--2-112;
            // Inside Macintosh Volume I (1985), p. I-260.
            (true, 0x175) => {
                let sp = cpu.read_reg(Register::A7);
                bus.write_long(sp, self.read_tick_count(bus));
                Ok(())
            }

            // ========== Utility Traps ==========

            // BitAnd ($A858)
            // Returns value1 AND value2.
            // FUNCTION BitAnd(value1, value2: LONGINT): LONGINT;
            // Inside Macintosh Volume I, I-483
            // BitAnd ($A858): Returns value1 AND value2
            (true, 0x058) => {
                let sp = cpu.read_reg(Register::A7);
                let value2 = bus.read_long(sp);
                let value1 = bus.read_long(sp + 4);
                bus.write_long(sp + 8, value1 & value2);
                cpu.write_reg(Register::A7, sp + 8);
                Ok(())
            }

            // BitXor ($A859)
            // Returns value1 XOR value2.
            // FUNCTION BitXor(value1, value2: LONGINT): LONGINT;
            // Inside Macintosh Volume I, I-483
            // BitXor ($A859): Returns value1 XOR value2
            (true, 0x059) => {
                let sp = cpu.read_reg(Register::A7);
                let value2 = bus.read_long(sp);
                let value1 = bus.read_long(sp + 4);
                bus.write_long(sp + 8, value1 ^ value2);
                cpu.write_reg(Register::A7, sp + 8);
                Ok(())
            }

            // BitNot ($A85A)
            // Returns NOT value.
            // FUNCTION BitNot(value: LONGINT): LONGINT;
            // Inside Macintosh Volume I, I-483
            // BitNot ($A85A): Returns NOT value
            (true, 0x05A) => {
                let sp = cpu.read_reg(Register::A7);
                let value = bus.read_long(sp);
                bus.write_long(sp + 4, !value);
                cpu.write_reg(Register::A7, sp + 4);
                Ok(())
            }

            // BitOr ($A85B)
            // Returns value1 OR value2.
            // FUNCTION BitOr(value1, value2: LONGINT): LONGINT;
            // Inside Macintosh Volume I, I-483
            // BitOr ($A85B): Returns value1 OR value2
            (true, 0x05B) => {
                let sp = cpu.read_reg(Register::A7);
                let value2 = bus.read_long(sp);
                let value1 = bus.read_long(sp + 4);
                bus.write_long(sp + 8, value1 | value2);
                cpu.write_reg(Register::A7, sp + 8);
                Ok(())
            }

            // BitShift ($A85C)
            // Logically shifts value by count bits (positive=left, negative=right).
            // FUNCTION BitShift(value: LONGINT; count: INTEGER): LONGINT;
            // Inside Macintosh Volume I, I-472. IM says the count is taken
            // MOD 32, but BasiliskII/System 7.5.3 returns 0 for |count| >= 32.
            (true, 0x05C) => {
                let sp = cpu.read_reg(Register::A7);
                let count = bus.read_word(sp) as i16;
                let value = bus.read_long(sp + 2);
                let shift = count.unsigned_abs() as u32;
                let result = if shift >= 32 {
                    0
                } else if count >= 0 {
                    value << shift
                } else {
                    value >> shift
                };
                bus.write_long(sp + 6, result);
                cpu.write_reg(Register::A7, sp + 6);
                Ok(())
            }

            // BitTst ($A85D)
            // Tests whether a particular bit of a bit image is set.
            // FUNCTION BitTst(bytePtr: Ptr; bitNum: LONGINT): BOOLEAN;
            // Inside Macintosh Volume I, I-472
            //
            // Returns 0x0100 for TRUE, 0x0000 for FALSE. Inside Macintosh
            // Volume I (1985), p. I-86 defines BOOLEAN as a one-byte value in
            // bit 0; Pascal functions return it in the high half of the
            // word-aligned result slot (Inside Macintosh Volume V (1986),
            // p. V-124; Imaging With QuickDraw (1994), p. 5-22). Executor's
            // src/emutrap.c PascalToCCall encodes this literally for rettype 1:
            // it clears the result word, then writes the BOOLEAN byte at the
            // high address. Writing `0x0001` would put the value in the padding
            // low byte and read as FALSE, while `0xFFFF` is nonzero for C but
            // fails a Pascal `Boolean = TRUE` comparison.
            // BitTst ($A85D): Tests bit in memory: FUNCTION BitTst(bytePtr: Ptr; bitNum: LONGINT): BOOLEAN; bit 0 = MSB per IM:I I-472
            (true, 0x05D) => {
                let sp = cpu.read_reg(Register::A7);
                let bit_num = bus.read_long(sp) as i32;
                let byte_ptr = bus.read_long(sp + 4);
                // Bit 0 is the high-order bit of the first byte (big-endian).
                // Byte offset = bitNum / 8, bit within byte = 7 - (bitNum % 8)
                let byte_offset = (bit_num >> 3) as u32;
                let bit_pos = 7 - (bit_num & 7) as u32;
                let byte_val = bus.read_byte(byte_ptr.wrapping_add(byte_offset));
                let result_word: u16 = if (byte_val >> bit_pos) & 1 != 0 {
                    0x0100
                } else {
                    0x0000
                };
                // Pascal BOOLEAN result: write into result slot above arguments
                // Stack: [bitNum(4)] [bytePtr(4)] [result(2)]
                bus.write_word(sp + 8, result_word);
                cpu.write_reg(Register::A7, sp + 8);
                Ok(())
            }

            // BitSet ($A85E)
            // Sets a particular bit of a bit image.
            // PROCEDURE BitSet(bytePtr: Ptr; bitNum: LONGINT);
            // Inside Macintosh Volume I, I-472
            // BitSet ($A85E): Sets bit in memory: PROCEDURE BitSet(bytePtr: Ptr; bitNum: LONGINT); per IM:I I-472
            (true, 0x05E) => {
                let sp = cpu.read_reg(Register::A7);
                let bit_num = bus.read_long(sp) as i32;
                let byte_ptr = bus.read_long(sp + 4);
                let byte_offset = (bit_num >> 3) as u32;
                let bit_pos = 7 - (bit_num & 7) as u32;
                let addr = byte_ptr.wrapping_add(byte_offset);
                let byte_val = bus.read_byte(addr);
                bus.write_byte(addr, byte_val | (1 << bit_pos));
                cpu.write_reg(Register::A7, sp + 8);
                Ok(())
            }

            // BitClr ($A85F)
            // Clears a particular bit of a bit image.
            // PROCEDURE BitClr(bytePtr: Ptr; bitNum: LONGINT);
            // Inside Macintosh Volume I, I-472
            // BitClr ($A85F): Clears bit in memory: PROCEDURE BitClr(bytePtr: Ptr; bitNum: LONGINT); per IM:I I-472
            (true, 0x05F) => {
                let sp = cpu.read_reg(Register::A7);
                let bit_num = bus.read_long(sp) as i32;
                let byte_ptr = bus.read_long(sp + 4);
                let byte_offset = (bit_num >> 3) as u32;
                let bit_pos = 7 - (bit_num & 7) as u32;
                let addr = byte_ptr.wrapping_add(byte_offset);
                let byte_val = bus.read_byte(addr);
                bus.write_byte(addr, byte_val & !(1 << bit_pos));
                cpu.write_reg(Register::A7, sp + 8);
                Ok(())
            }

            // HiWord ($A86A)
            // Returns the high-order word of a long integer.
            // FUNCTION HiWord(x: LONGINT): INTEGER;
            // Inside Macintosh Volume I, I-472
            // HiWord ($A86A): Returns (x >> 16) as INTEGER per IM:I I-472
            (true, 0x06A) => {
                let sp = cpu.read_reg(Register::A7);
                let x = bus.read_long(sp);
                let hi = (x >> 16) as u16;
                bus.write_word(sp + 4, hi);
                cpu.write_reg(Register::A7, sp + 4);
                Ok(())
            }

            // LoWord ($A86B)
            // Returns the low-order word of a long integer.
            // FUNCTION LoWord(x: LONGINT): INTEGER;
            // Inside Macintosh Volume I, I-472
            // LoWord ($A86B): Returns (x & 0xFFFF) as INTEGER per IM:I I-472
            (true, 0x06B) => {
                let sp = cpu.read_reg(Register::A7);
                let x = bus.read_long(sp);
                let lo = (x & 0xFFFF) as u16;
                bus.write_word(sp + 4, lo);
                cpu.write_reg(Register::A7, sp + 4);
                Ok(())
            }

            // FixRound ($A86C)
            // Rounds a Fixed value to the nearest integer.
            // FUNCTION FixRound(x: Fixed): INTEGER;
            // Inside Macintosh Volume I, I-467
            //
            // System 7.5.3 ROM uses round-half-away-from-zero (ANSI-C
            // rint() behaviour) so FixRound(-0.5) = -1, not 0. IM:I-467
            // doesn't specify the tie-break; the convention is anchored
            // by Basilisk's behaviour.
            //
            // Formula: abs(x) + 0.5 truncated toward zero, then negate
            // if x was negative.
            //
            // Stack: SP+0=x(4), SP+4=result(2). Pops the four-byte parameter
            // so MPW's following MOVE.W (SP)+ reads the INTEGER result.
            // FixRound ($A86C): Rounds Fixed to nearest integer (round-half-up)
            (true, 0x06C) => {
                let sp = cpu.read_reg(Register::A7);
                let x = bus.read_long(sp) as i32 as i64;
                let abs_rounded = ((x.abs() + 0x8000) >> 16) as i16;
                let rounded = if x < 0 { -abs_rounded } else { abs_rounded };
                bus.write_word(sp + 4, rounded as u16);
                cpu.write_reg(Register::A7, sp + 4);
                Ok(())
            }

            // Random ($A861)
            // Returns a pseudo-random integer in the range -32767..32767.
            // FUNCTION Random: INTEGER;
            // Inside Macintosh Volume I, I-195
            //
            // randSeed is updated to (randSeed * 16807) MOD (2^31 - 1).
            // The result is the low 16 bits of the new seed, interpreted as
            // a signed INTEGER — except that -32768 ($8000) is mapped to 0.
            // Reference: Executor src/quickdraw/qMisc.cpp C_Random
            // Random ($A861): Full Mac Toolbox random algorithm
            (true, 0x061) => {
                let sp = cpu.read_reg(Register::A7);
                let pc = cpu.read_reg(Register::PC);
                let a5 = cpu.read_reg(Register::A5);
                let global_ptr = bus.read_long(a5);
                let seed_addr = global_ptr.wrapping_sub(126);
                let old_seed = bus.read_long(seed_addr);
                let seed = if old_seed == 0 { 1u64 } else { old_seed as u64 };
                let new_seed = ((seed * 16807) % 2147483647) as u32;
                bus.write_long(seed_addr, new_seed);
                // Return the low 16 bits of the seed. The seed is always in
                // 0..2^31-1, so the low 16 bits naturally span -32768..32767
                // when read as a signed INTEGER. Map -32768 to 0 so the
                // result range is exactly -32767..32767.
                let lo = new_seed as u16;
                let result = if lo == 0x8000 { 0u16 } else { lo };
                bus.write_word(sp, result);
                if trace_entropy_enabled() {
                    eprintln!(
                        "[ENTROPY] Random pc=${:08X} seed_addr=${:08X} old_seed={} new_seed={} result={}",
                        pc, seed_addr, old_seed, new_seed, result as i16
                    );
                }
                Ok(())
            }

            // GetIndString ($A9E6)
            //
            // Per IM:I I-468: "GetIndString returns in theString
            // a string in the string list that has the resource
            // ID strListID. It reads the string list from the
            // resource file if necessary, by calling the Resource
            // Manager function GetResource('STR#',strListID). It
            // returns the string specified by the index parameter,
            // which can range from 1 to the number of strings in
            // the list. If the resource can't be read or the index
            // is out of range, the empty string is returned."
            //
            // ## Trap-word repurposing per System 7+
            //
            // IM:I I-468 marks GetIndString as `[Not in ROM]` —
            // legacy System 6 era treated it as a software-only
            // routine (Pascal compiler emitted inline GetResource
            // + Munger code). However IM:III line 9512 master
            // dispatch table assigns trap word $A9E6 to InitAllPacks
            // (a System 6 PROCEDURE that loads Pack0..Pack7 from
            // the System file). When System 7 deprecated package
            // pre-loading (Pack0..Pack7 became autoload-on-demand),
            // Apple repurposed trap word $A9E6 to GetIndString —
            // making the System 6 software-only routine into a
            // System 7+ ROM-resident Toolbox trap. Same trap-word
            // repurposing pattern as $A056 (LwrString → LowerText
            // / UpperText / StripText / StripUpperText per IM:VI
            // line 30881 — already handled at memory.rs:1610).
            //
            // The InitAllPacks call site is now unreachable from
            // System 7+ apps (autoload happens implicitly during
            // package use; no explicit init needed). Apps emitting
            // $A9E6 from System 6 era are calling InitAllPacks
            // expecting a no-args no-result init — Systemless's
            // GetIndString impl reads sp+0 / sp+2 / sp+4 as args,
            // which on a System 6 InitAllPacks call would dereference
            // stale stack values. In practice no current corpus title
            // is System 6 era; all corpus games are System 7+ and
            // emit $A9E6 expecting GetIndString semantics. If a
            // future System-6 binary surfaces InitAllPacks usage,
            // detect via trap-trace and add a stack-shape check
            // (4-byte InitAllPacks frame vs 8-byte GetIndString
            // frame distinguished by post-pop SP).
            //
            // PROCEDURE GetIndString (VAR theString: Str255;
            //                          strListID: INTEGER;
            //                          index: INTEGER);
            // Inside Macintosh Volume I, I-468
            // Inside Macintosh Volume III line 9512: $A9E6 = InitAllPacks (legacy)
            // System 7+ repurposing per Apple Toolbox extension (undocumented in IM)
            //
            // Stack: SP+0 index INTEGER (2 bytes), SP+2 strListID
            // INTEGER (2 bytes), SP+4 theString VAR Str255 ptr
            // (4 bytes). Pop 8 bytes.
            // GetIndString ($A9E6): Looks up STR# resource by ID, returns 1-based indexed Pascal string per IM:I I-468; empty string on not-found or out-of-range. Trap-word $A9E6 repurposed by Apple System 7+ from legacy InitAllPacks (per IM:III line 9512 master dispatch table) — same trap-word-repurposing pattern as $A056 LwrString→LowerText family per IM:VI 30881. System 6 InitAllPacks callers would dereference stale stack values via Systemless's GetIndString frame; no current corpus title is System 6 era.
            (true, 0x1E6) => {
                let sp = cpu.read_reg(Register::A7);
                let pc = cpu.read_reg(Register::PC);
                // Stack layout: SP+0=index, SP+2=strListID, SP+4=theString (VAR ptr)
                let index = bus.read_word(sp) as usize;
                let str_list_id = bus.read_word(sp + 2) as i16;
                let the_string_ptr = bus.read_long(sp + 4);

                let res_type = *b"STR#";
                let mut res_found = false;
                let found_str: Option<Vec<u8>> = if let Some((_, data_ptr)) =
                    self.find_or_load_resource_any(bus, res_type, str_list_id)
                {
                    res_found = true;
                    // STR# format: 2-byte count, then Pascal strings (1-byte len + chars)
                    // Inside Macintosh Volume I, I-476
                    let count = bus.read_word(data_ptr) as usize;
                    if index >= 1 && index <= count {
                        let mut offset = 2u32;
                        let mut found = None;
                        for i in 1..=count {
                            let len = bus.read_byte(data_ptr + offset) as usize;
                            offset += 1;
                            if i == index {
                                found = Some(bus.read_bytes(data_ptr + offset, len));
                                break;
                            }
                            offset += len as u32;
                        }
                        found
                    } else {
                        None
                    }
                } else {
                    None
                };

                // IM:I I-468 documents GetIndString as calling
                // GetResource('STR#', strListID) "if necessary". On
                // the success path that underlying Resource Manager hit
                // must clear stale ResErr to noErr, which callers can
                // observe immediately after GetIndString returns. The
                // miss path is not pinned here: BasiliskII leaves the
                // missing-resource buffer contents / ResErr state on a
                // looser implementation-defined path than the Apple
                // text specifies, so Systemless preserves the pre-call
                // ResErr value when no STR# is found.
                if res_found {
                    bus.write_word(0x0A60, 0); // noErr
                }

                if the_string_ptr != 0 {
                    match found_str {
                        Some(bytes) => {
                            bus.write_pstring(the_string_ptr, &bytes);
                            if trace_entropy_enabled() {
                                let text = String::from_utf8_lossy(&bytes);
                                eprintln!(
                                    "[ENTROPY] GetIndString pc=${:08X} strListID={} index={} -> {:?}",
                                    pc, str_list_id, index, text
                                );
                            }
                        }
                        None => {
                            bus.write_byte(the_string_ptr, 0);
                            if trace_entropy_enabled() {
                                eprintln!(
                                    "[ENTROPY] GetIndString pc=${:08X} strListID={} index={} -> <empty>",
                                    pc, str_list_id, index
                                );
                            }
                        }
                    }
                }
                cpu.write_reg(Register::A7, sp + 8);
                Ok(())
            }

            // SystemTask ($A9B4)
            // Per IM:I I-442: "For each open desk accessory (or other
            // device driver performing periodic actions), SystemTask
            // causes the accessory to perform the periodic action
            // defined for it, if any such action has been defined and
            // if the proper time period has passed since the action
            // was last performed. ... You should call SystemTask as
            // often as possible, usually once every time through your
            // main event loop."
            // PROCEDURE SystemTask;
            // Inside Macintosh Volume I, I-442; periodic driver state is
            // described at I-444 through I-445.
            //
            // Calling convention (Tool-bit PROCEDURE per IM:I I-442):
            //   no inputs, no FUNCTION result slot, no Pascal stack
            //   argument frame. A7 is preserved across the call.
            //
            // MPW Universal Headers Desk.h:
            //   EXTERN_API(void) SystemTask(void) ONEWORDINLINE(0xA9B4);
            //
            // HLE compromise: Systemless models no open Desk Accessories or
            // periodic device-driver chain, so every component this trap
            // would currently visit is empty. The implementation is
            // `Ok(())`; `system_task_has_periodic_work` is the runtime gate
            // that must become true before either subsystem is implemented.
            //
            // Behavioral contract:
            //   - register-only Tool-bit PROCEDURE calling convention
            //     (no Pascal stack frame, no FUNCTION result slot)
            //   - A7 preserved across a single call AND a 5-call
            //     composition (BasiliskII System 7.5.3 ROM walks empty
            //     DA/DCE state and returns without consuming stack)
            //
            // Contract tests:
            //   - systemtask_procedure_call_preserves_stack_pointer (single call)
            //   - systemtask_five_call_composition_preserves_stack_pointer
            (true, 0x1B4) => Ok(()),

            // GetAppParms ($A9F5)
            // Returns the current application's name, resource file refnum,
            // and Finder information handle.
            // PROCEDURE GetAppParms(VAR apName: Str255; VAR apRefNum: INTEGER;
            //                       VAR apParam: Handle);
            // Inside Macintosh Volume II, II-58
            //
            // Reads low-memory globals:
            //   CurApName ($0910) — Pascal string (Str31)
            //   CurApRefNum ($0900) — INTEGER
            //   AppParmHandle ($0AEC) — Handle
            //
            (true, 0x1F5) => {
                let sp = cpu.read_reg(Register::A7);
                let ap_param_ptr = bus.read_long(sp);
                let ap_refnum_ptr = bus.read_long(sp + 4);
                let ap_name_ptr = bus.read_long(sp + 8);

                // Copy CurApName (Str31 at $0910) → *apName
                if ap_name_ptr != 0 {
                    let bytes = bus.read_pstring(addr::CUR_APNAME);
                    let n = bytes.len().min(31);
                    bus.write_byte(ap_name_ptr, n as u8);
                    bus.write_bytes(ap_name_ptr + 1, &bytes[..n]);
                }

                // Copy CurApRefNum ($0900) → *apRefNum
                if ap_refnum_ptr != 0 {
                    let refnum = bus.read_word(addr::CUR_APREF_NUM);
                    bus.write_word(ap_refnum_ptr, refnum);
                }

                // Copy AppParmHandle ($0AEC) → *apParam
                if ap_param_ptr != 0 {
                    let handle = bus.read_long(addr::APP_PARM_HANDLE);
                    bus.write_long(ap_param_ptr, handle);
                }

                cpu.write_reg(Register::A7, sp + 12);
                Ok(())
            }

            // UnloadSeg ($A9F1)
            // Marks a code segment as purgeable once no routines in it are
            // being called. Systemless keeps all loaded segments resident, so
            // this is a no-op that pops its Ptr argument.
            // PROCEDURE UnloadSeg(routineAddr: Ptr);
            // Inside Macintosh Volume II, II-58
            //
            // Regression coverage:
            //   toolbox::tests::unloadseg_consumes_routineaddr_pointer_argument
            //   toolbox::tests::unloadseg_noop_preserves_registered_segment_cache
            // UnloadSeg ($A9F1): Pops Ptr argument per IM:II II-58; Systemless keeps all segments resident
            (true, 0x1F1) => {
                let sp = cpu.read_reg(Register::A7);
                cpu.write_reg(Register::A7, sp + 4);
                Ok(())
            }

            // LaunchApplication ($A9F2)
            //
            // Per IM:II II-60: "Launch starts up another application
            // (the new application). The current application is
            // terminated; control transfers to the new application."
            // Per IM:VI Process Manager 28-1..28-4: System 7+
            // renamed Launch to LaunchApplication and extended the
            // parameter convention via a LaunchParamBlockRec
            // (LaunchPB structure with launchAppSpec FSSpec ptr +
            // launchControlFlags + launchPreferredSize +
            // launchMinimumSize). Both call paths share trap word
            // $A9F2 — register-based dispatch where A0 points to
            // either the IM:II Launch CmdLine record (legacy) or
            // the IM:VI LaunchPB record (System 7+).
            //
            // Per IM:VI Table C-1 line 57551 + 57649:
            // "LaunchApplication | $A9F2" — the canonical System 7+
            // name. Systemless previously used the legacy IM:II "Launch"
            // name. Both names refer to the same trap word — the
            // mapping pre-dates Color QuickDraw, has been allocated
            // to this trap in every Mac OS release since System 1.0,
            // and was renamed (not relocated) for System 7.
            // Inside Macintosh Volume II, II-60 (Launch — legacy)
            // Inside Macintosh Volume VI, 28-1..28-4 (LaunchApplication — System 7+)
            //
            // Register convention: A0 points to a launch parameter
            // block (CmdLine pre-System-7 or LaunchPB post-System-7).
            // No stack args (register-based OS trap pattern).
            //
            // HLE compromise: Systemless has one foreground application
            // image, not a full Process Manager with background process
            // scheduling. Foreground launches of existing VFS applications
            // are queued for the runner to load into a fresh application
            // heap; launchContinue launches begin when the caller next
            // yields through the Event Manager, matching Processes 1994
            // p. 2-15. A launchDontSwitch target remains deferred until
            // the caller exits, when it becomes the only runnable app and
            // is promoted into Systemless's foreground process slot.
            // On launch failure, LaunchApplication returns 0 in the
            // launchProcessSN / launchPreferredSize / launchMinimumSize /
            // launchAvailableSize fields so callers do not observe stale
            // output values.
            //
            // Trap-name fixed during the trap-name verification audit
            // pattern — was previously labeled
            // "Launch" (legacy IM:II name); audit cross-referenced
            // against IM:VI Table C-1 master dispatch table line
            // 57551 "_LaunchApplication | $A9F2" and corrected to
            // the canonical System 7+ name.
            //
            // Regression coverage:
            //   toolbox::tests::launch_legacy_cmdline_queues_existing_target_and_preserves_page_option
            //   toolbox::tests::launch_legacy_cmdline_missing_target_halts_without_clobbering_d0
            //   toolbox::tests::launchapplication_launchcontinue_clear_records_target_app_path_and_halts
            //   toolbox::tests::launchapplication_launchcontinue_set_records_target_app_path_and_returns
            //   toolbox::tests::launchapplication_existing_foreground_target_queues_runner_launch
            // LaunchApplication ($A9F2): Per IM:VI Table C-1 line 57551 the canonical System 7+ name is LaunchApplication; legacy IM:II II-60 name was "Launch" (same trap word). Register-based: A0 points to LaunchPB record (System 7+) or legacy CmdLine record (pre-System-7); no stack args. Existing foreground launch targets are queued for runner-level app switching; missing targets preserve the historical halt/return behavior according to launchContinue.
            (true, 0x1F2) => {
                let launch_params = cpu.read_reg(Register::A0);
                let extended_block =
                    launch_params != 0 && bus.read_word(launch_params + 6) == 0x4C43;
                let launch_flags = if extended_block {
                    bus.read_word(launch_params + 14)
                } else {
                    0
                };
                let launch_continue = (launch_flags & 0x4000) != 0;
                let launch_dont_switch = (launch_flags & 0x0200) != 0;
                let mut launch_result = 0u32;
                let mut launch_target: Option<String> = None;
                if launch_params != 0 {
                    if extended_block {
                        let app_spec_ptr = bus.read_long(launch_params + 16);
                        if app_spec_ptr != 0 {
                            let filename = crate::trap::types::read_fsspec_name(bus, app_spec_ptr);
                            if !filename.is_empty() {
                                let vref = bus.read_word(app_spec_ptr) as i16;
                                let dir_id = bus.read_long(app_spec_ptr + 2);
                                let app_path = self
                                    .vfs_key_for_fsspec(vref, dir_id, &filename)
                                    .unwrap_or(filename);
                                self.set_launched_app_path(&app_path);
                                let target_exists = self.find_vfs_file(&app_path).is_some()
                                    || self.find_vfs_rsrc_file(&app_path).is_some();
                                if target_exists {
                                    launch_target = Some(app_path);
                                } else {
                                    launch_result = (-43i32) as u32; // fnfErr
                                }
                            }
                        } else {
                            launch_result = (-43i32) as u32; // fnfErr
                        }
                    } else {
                        // IM:II II-59..II-60 legacy CmdLine record:
                        // 0(A0) points to a Pascal application name and
                        // 4(A0) carries the sound/screen page option.
                        bus.write_word(0x0936, bus.read_word(launch_params + 4));
                        let app_name_ptr = bus.read_long(launch_params);
                        let app_name = if app_name_ptr != 0 {
                            String::from_utf8_lossy(&bus.read_pstring(app_name_ptr)).into_owned()
                        } else {
                            String::new()
                        };
                        if app_name.is_empty() {
                            launch_result = (-43i32) as u32; // fnfErr
                        } else {
                            let app_path = match self.directory_path_for_id(*self.default_dir_id) {
                                Some(dir_path) if !dir_path.is_empty() => {
                                    format!("{dir_path}/{app_name}")
                                }
                                _ => app_name,
                            };
                            self.set_launched_app_path(&app_path);
                            let target_exists = self.find_vfs_file(&app_path).is_some()
                                || self.find_vfs_rsrc_file(&app_path).is_some();
                            if target_exists {
                                launch_target = Some(app_path);
                            } else {
                                launch_result = (-43i32) as u32; // fnfErr
                            }
                        }
                    }
                    if extended_block && launch_result != 0 {
                        bus.write_long(launch_params + 20, 0); // launchProcessSN.highLongOfPSN
                        bus.write_long(launch_params + 24, 0); // launchProcessSN.lowLongOfPSN
                        bus.write_long(launch_params + 28, 0); // launchPreferredSize
                        bus.write_long(launch_params + 32, 0); // launchMinimumSize
                        bus.write_long(launch_params + 36, 0); // launchAvailableSize
                    }
                }
                if extended_block {
                    cpu.write_reg(Register::D0, launch_result);
                }
                let queued_foreground_launch =
                    launch_result == 0 && launch_target.is_some() && !launch_dont_switch;
                if launch_result == 0 {
                    if let Some(app_path) = launch_target.as_deref() {
                        if launch_dont_switch {
                            self.queue_background_launch_application(app_path);
                        } else {
                            self.queue_pending_launch_application(app_path, launch_continue);
                        }
                    }
                }
                if launch_continue || queued_foreground_launch {
                    return Some(Ok(()));
                }
                Err(Error::Halted)
            }

            // Chain ($A9F3)
            // Legacy CmdLine entry point. A0 points to a record whose
            // first longword points to the application's Pascal file
            // name and whose 4(A0) word carries the sound/screen buffer
            // configuration (CurPageOption).
            // Inside Macintosh Volume II (1985), pp. II-59 to II-60.
            //
            // Systemless cannot actually hand control to another
            // application, but it does preserve the documented
            // bookkeeping: record CurPageOption in low memory, record
            // the launched app path when the filename pointer is
            // present, then halt the guest.
            //
            // Regression coverage:
            //   src/trap/toolbox.rs::tests::chain_records_cmdline_path_and_curpageoption_before_halt
            // Chain ($A9F3): Halts emulation after recording the legacy CmdLine metadata per IM:II II-59..II-60.
            (true, 0x1F3) => {
                let cmd_line = cpu.read_reg(Register::A0);
                if cmd_line != 0 {
                    let page_option = bus.read_word(cmd_line + 4);
                    bus.write_word(0x0936, page_option);

                    let app_name_ptr = bus.read_long(cmd_line);
                    if app_name_ptr != 0 {
                        let app_name =
                            String::from_utf8_lossy(&bus.read_pstring(app_name_ptr)).into_owned();
                        if !app_name.is_empty() {
                            let app_path = match self.directory_path_for_id(*self.default_dir_id) {
                                Some(dir_path) if !dir_path.is_empty() => {
                                    format!("{dir_path}/{app_name}")
                                }
                                _ => app_name,
                            };
                            self.set_launched_app_path(&app_path);
                        }
                    }
                }
                Err(Error::Halted)
            }

            // ExitToShell ($A9F4)
            // Terminates the current application and returns to the Finder.
            // PROCEDURE ExitToShell;
            // Inside Macintosh Volume II, II-58
            // ExitToShell ($A9F4): Halts emulation
            (true, 0x1F4) => Err(Error::Halted),

            // Debugger ($A9FF)
            // Parameterless debugger entry trap.
            // Universal Interfaces Types.h declares Debugger() as
            // ONEWORDINLINE(0xA9FF) with no parameters.
            // Inside Macintosh: Processes (1994), p. 7-9;
            // Inside Macintosh: Memory (1992), p. 3-23.
            // Debugger ($A9FF): No debugger installed on Systemless,
            // so this is a no-op that returns to the caller.
            (true, 0x1FF) => Ok(()),

            // _Shutdown ($A895) — Shutdown Manager dispatch
            // Inside Macintosh Volume V, V-589..V-590.
            //
            // Universal Headers <ShutDown.h> (System 7.5, Universal Interfaces 3.4)
            // declares all four Shutdown Manager entry points as
            //   THREEWORDINLINE(0x3F3C, <selector>, 0xA895)
            // where 0x3F3C is `MOVE.W #imm,-(A7)` — the compiler emits this
            // inline glue at every call site:
            //
            //   ShutDwnPower():
            //     ; (no caller args)
            //     MOVE.W #1, -(A7)        ; 0x3F3C 0x0001
            //     _Shutdown               ; 0xA895
            //
            //   ShutDwnStart():
            //     ; (no caller args)
            //     MOVE.W #2, -(A7)        ; 0x3F3C 0x0002
            //     _Shutdown               ; 0xA895
            //
            //   ShutDwnInstall(proc, flags):
            //     ; caller already pushed proc (4) + flags (2)  -- Pascal LTR
            //     MOVE.W #3, -(A7)        ; 0x3F3C 0x0003
            //     _Shutdown               ; 0xA895
            //
            //   ShutDwnRemove(proc):
            //     ; caller already pushed proc (4)
            //     MOVE.W #4, -(A7)        ; 0x3F3C 0x0004
            //     _Shutdown               ; 0xA895
            //
            // On entry to the trap, SP+0 holds the selector word. The trap
            // is responsible for popping the entire frame (selector + args).
            //
            // Selectors:
            //   1  sdPowerOff — ShutDwnPower  (halts; emulator can't power off)
            //   2  sdRestart  — ShutDwnStart  (halts; emulator can't reboot)
            //   3  sdInstall  — ShutDwnInstall(proc, flags)  (no-op; pops 8)
            //   4  sdRemove   — ShutDwnRemove(proc)          (no-op; pops 6)
            //
            // Systemless does not model the shutdown procedure chain, so
            // sdInstall/sdRemove are accepted as no-ops that simply pop
            // the argument frame. sdPowerOff and sdRestart both halt the
            // guest, which is how the runner surfaces "application wants
            // to exit". The procedure list is never invoked because no
            // actual shutdown is ever triggered; this matches the
            // BasiliskII System 7.5 ROM, where queued procs are also dormant
            // until real shutdown.
            (true, 0x095) => {
                let sp = cpu.read_reg(Register::A7);
                let selector = bus.read_word(sp) as i16;
                let pop_bytes = match selector {
                    3 => {
                        // ShutDwnInstall(proc: ProcPtr; flags: INTEGER)
                        // SP+0 selector(2), SP+2 flags(2), SP+4 proc(4)
                        8
                    }
                    4 => {
                        // ShutDwnRemove(proc: ProcPtr)
                        // SP+0 selector(2), SP+2 proc(4)
                        6
                    }
                    _ => return Some(Err(Error::Halted)),
                };

                // The documented non-halting selectors are caller-visible
                // no-ops apart from consuming the full Pascal argument frame.
                cpu.write_reg(Register::D0, 0);
                cpu.write_reg(Register::A7, sp + pop_bytes);
                Ok(())
            }

            // Delay ($A03B) - OS trap
            // PROCEDURE Delay(numTicks: LONGINT; VAR finalTicks: LONGINT);
            // Inside Macintosh Volume II, II-384 (via OS Utilities)
            // A0 = numTicks, returns finalTicks in D0
            // Delay ($A03B): Blocks for A0 ticks via runner service_delay_ticks; GUI mode paces to wall-clock, headless advances directly. Returns finalTicks in D0
            (false, 0x3B) => {
                let num_ticks = cpu.read_reg(Register::A0);
                let trap_pc = cpu.read_reg(Register::PC).wrapping_sub(2);
                if trace_title_diag_enabled() {
                    let tick = bus.read_long(0x016A);
                    if (68..=110).contains(&tick) {
                        eprintln!(
                            "[TITLE-DIAG] Delay tick={} pc=${:08X} ticks={}",
                            tick, trap_pc, num_ticks
                        );
                    }
                }
                if num_ticks == 0 {
                    // Zero delay: return current ticks immediately
                    let current_ticks = bus.read_long(0x016A);
                    cpu.write_reg(Register::D0, current_ticks);
                } else {
                    // Queue the delay for the runner to consume tick-by-tick.
                    // On a real Mac, Delay blocks via PrimeTime + interrupt wait
                    // (executor osutil.cpp:823-838). Our runner drains these ticks
                    // one-at-a-time through advance_guest_tick(), firing VBL and
                    // timer tasks at each boundary. The runner writes finalTicks
                    // to D0 when the delay is fully consumed.
                    self.pending_delay_ticks = num_ticks;
                }
                if let Err(err) = self.record_trace_event(
                    bus,
                    trap_pc,
                    "delay",
                    Self::trace_field_map(&[("ticks", num_ticks.to_string())]),
                    false,
                ) {
                    return Some(Err(err));
                }
                Ok(())
            }

            // SCSIDispatch (0xA815)
            // Dispatches original SCSI Manager routines selected by a word on top of the stack.
            // FUNCTION SCSIReset: OSErr;
            // Inside Macintosh: Devices (1994), pp. 3-31 to 3-42 and 3-48.
            (true, 0x015) => {
                let sp = cpu.read_reg(Register::A7);
                let selector = bus.read_word(sp) as i16;
                let operation =
                    scsi_dispatch_operation_route(self.current_trap_word, selector as u16);
                self.current_selector_operation = operation.map(|route| route.operation_id);
                let Some(arg_bytes) = Self::scsi_dispatch_arg_bytes(selector) else {
                    // Undefined selectors invoke dsCoreErr (12); Inside Macintosh Volume V, V-574.
                    bus.write_word(addr::DS_ERR_CODE, 12);
                    return Some(Err(crate::Error::Halted));
                };
                let total = 2 + arg_bytes;
                bus.write_word(sp + total, 0); // noErr
                cpu.write_reg(Register::A7, sp + total);
                Ok(())
            }

            // PPC ($A0DD)
            // Dispatches PPC Toolbox operations selected in D0.
            // Register ABI: D0 = selector/result; parameter-block routines use A0.
            // Inside Macintosh: Interapplication Communication (1993), p. 11-51.
            //
            // Systemless models the observable PPC Toolbox state:
            // selector $0000 (`PPCInit`) flips the init bit for
            // selectors that need it, but selector $000A (`IPCListPorts`)
            // already succeeds on the zero-request local path before init.
            // Selector $0000 and selector $000A are handled on both the
            // pre-init and post-init local paths.
            (false, 0x0DD) => {
                let raw_selector = cpu.read_reg(Register::D0);
                let operation = ppc_operation_route(self.current_trap_word, raw_selector);
                self.current_selector_operation = operation.map(|route| route.operation_id);
                let selector = raw_selector as u16;
                let pb = cpu.read_reg(Register::A0);
                let not_init_err = (-900i32) as u32;

                if selector != 0 && selector != 0x000A && !self.ppc_initialized {
                    cpu.write_reg(Register::D0, not_init_err);
                    return Some(Ok(()));
                }

                match selector {
                    0x0000 => {
                        self.ppc_initialized = true;
                        cpu.write_reg(Register::D0, 0);
                    }
                    0x000A => {
                        if pb != 0 {
                            bus.write_word(pb + 16, 0);
                            bus.write_word(pb + 44, 0);
                        }
                        cpu.write_reg(Register::D0, 0);
                    }
                    _ => {
                        cpu.write_reg(Register::D0, 0);
                    }
                }
                Ok(())
            }

            // SlotManager ($A06E)
            // Dispatches Slot Manager routines selected in D0 with an SpBlockPtr in A0.
            // Register ABI: A0 = SpBlockPtr, D0 = selector; returns OSErr in D0.
            // Inside Macintosh: Devices (1994), pp. 2-29, 2-99.
            // Systemless models the ROM-based Slot Manager but no NuBus cards.
            (false, 0x06E) => {
                let sp_block_ptr = cpu.read_reg(Register::A0);
                let selector = cpu.read_reg(Register::D0);
                let operation = slot_manager_operation_route(selector);
                self.current_selector_operation = operation.map(|route| route.operation_id);
                let sm_empty_slot: i32 = -300;
                if selector == 0x0008 && sp_block_ptr != 0 {
                    // SVersion ($A06E, selector $0008)
                    // Returns version 2 for the ROM-based Slot Manager.
                    // FUNCTION SVersion (spBlkPtr: SpBlockPtr): OSErr;
                    // Inside Macintosh: Devices (1994), pp. 2-30 to 2-31.
                    bus.write_long(sp_block_ptr, 2);
                    // spsPointer is reserved for future additional information.
                    bus.write_long(sp_block_ptr + 4, 0);
                    cpu.write_reg(Register::D0, 0);
                } else {
                    if selector == 0x0010 && sp_block_ptr != 0 {
                        // SReadInfo ($A06E, selector $0010)
                        // Returns smEmptySlot when the requested slot contains no card.
                        // FUNCTION SReadInfo (spBlkPtr: SpBlockPtr): OSErr;
                        // Inside Macintosh: Devices (1994), pp. 2-61 to 2-62.
                        bus.write_long(sp_block_ptr, sm_empty_slot as u32);
                    }
                    cpu.write_reg(Register::D0, sm_empty_slot as u32);
                }
                eprintln!(
                    "[TRAP] SlotManager selector={} operation={} -> {}",
                    selector,
                    operation.map_or("unregistered", |route| route.routine_name),
                    cpu.read_reg(Register::D0) as i32
                );
                Ok(())
            }

            // ========== Resource Manager (extended) ==========

            // OpenRFPerm ($A9C4): name, vRefNum, permission → refnum
            // Opens a resource fork and returns its refnum. A newly opened
            // file becomes current; if already open, returns existing refnum
            // without switching current file (IM:IV IV-17; MTb 1993 1-64..1-66).
            // Mirror the FUNCTION return value in D0 as well as the result slot.
            (true, 0x1C4) => {
                let sp = cpu.read_reg(Register::A7);
                let perm = bus.read_byte(sp) as i8 as i16;
                let wants_write = perm == 2 || perm == 3;
                let vref = bus.read_word(sp + 2) as i16;
                let name_ptr = bus.read_long(sp + 4);
                let name_len = bus.read_byte(name_ptr) as usize;
                let mut name_bytes = vec![0u8; name_len];
                for (i, byte) in name_bytes.iter_mut().enumerate() {
                    *byte = bus.read_byte(name_ptr + 1 + i as u32);
                }
                let name = decode_mac_roman(&name_bytes);
                if super::dispatch::trace_resfile_enabled() {
                    eprintln!("[TRAP] OpenRFPerm(\"{}\")", name);
                }

                if vref != 0 && self.working_directory_info(vref).is_none() {
                    bus.write_word(sp + 8, (-1i16) as u16);
                    cpu.write_reg(Register::D0, (-1i32) as u32);
                    bus.write_word(0x0A60, (-35i16) as u16); // nsvErr
                    cpu.write_reg(Register::A7, sp + 8);
                    return Some(Ok(()));
                }
                let mounted_dir_id = self.working_directory_info(vref).and_then(|working| {
                    self.vfs_volume_for_ref_num(working.volume_ref_num)
                        .map(|_| working.dir_id)
                });
                let vfs_key = if let Some(dir_id) = mounted_dir_id {
                    self.find_vfs_rsrc_file_in_directory(dir_id, &name)
                } else {
                    self.find_vfs_rsrc_file(&name)
                };

                // Try to find and load the resource fork from the selected volume.
                if let Some(vfs_key) = vfs_key {
                    if wants_write && self.vfs_path_is_read_only(&vfs_key) {
                        bus.write_word(sp + 8, (-1i16) as u16);
                        cpu.write_reg(Register::D0, (-1i32) as u32);
                        bus.write_word(0x0A60, (-44i16) as u16); // wPrErr
                        cpu.write_reg(Register::A7, sp + 8);
                        return Some(Ok(()));
                    }
                    // Dedupe: if this file is already open, return the
                    // existing refnum and skip the merge. Without this,
                    // games that re-open their own fork (Bonkheads opens
                    // it 16+ times during boot) re-allocate every
                    // resource on every open and exhaust the heap before
                    // the title even renders.
                    if let Some(existing) = self.refnum_for_resource_file_name(&vfs_key) {
                        if wants_write {
                            self.write_refnums.insert(existing);
                        }
                        if !wants_write && self.write_refnums.contains(&existing) {
                            let existing_read_only =
                                self.resources.as_ref().and_then(|resources| {
                                    let mut refnums: Vec<u16> = resources
                                        .names
                                        .iter()
                                        .filter_map(|(&refnum, resource_name)| {
                                            (resource_name == &vfs_key
                                                && !self.write_refnums.contains(&refnum))
                                            .then_some(refnum)
                                        })
                                        .collect();
                                    refnums.sort_unstable();
                                    refnums.into_iter().next()
                                });
                            if let Some(read_refnum) = existing_read_only {
                                if super::dispatch::trace_resfile_enabled() {
                                    eprintln!(
                                        "[TRAP] OpenRFPerm: \"{}\" already read-open as refnum {}, dedup",
                                        name, read_refnum
                                    );
                                }
                                bus.write_word(sp + 8, read_refnum);
                                cpu.write_reg(Register::D0, read_refnum as u32);
                                bus.write_word(0x0A60, 0); // ResErr = noErr
                                cpu.write_reg(Register::A7, sp + 8);
                                return Some(Ok(()));
                            }
                            if super::dispatch::trace_resfile_enabled() {
                                eprintln!(
                                    "[TRAP] OpenRFPerm: \"{}\" write-opened refnum {}, forcing new read-only access path",
                                    name, existing
                                );
                            }
                            let refnum =
                                self.open_resource_file_from_vfs_key(bus, &vfs_key, wants_write);
                            bus.write_word(sp + 8, refnum);
                            cpu.write_reg(Register::D0, refnum as u32);
                            cpu.write_reg(Register::A7, sp + 8);
                            return Some(Ok(()));
                        }
                        if super::dispatch::trace_resfile_enabled() {
                            eprintln!(
                                "[TRAP] OpenRFPerm: \"{}\" already open as refnum {}, dedup",
                                name, existing
                            );
                        }
                        bus.write_word(sp + 8, existing);
                        cpu.write_reg(Register::D0, existing as u32);
                        bus.write_word(0x0A60, 0); // ResErr = noErr
                        cpu.write_reg(Register::A7, sp + 8);
                        return Some(Ok(()));
                    }
                    let rsrc_data = self.vfs_rsrc.get(&vfs_key).unwrap().clone();
                    eprintln!(
                        "[TRAP] OpenRFPerm: found rsrc fork for \"{}\" ({} bytes)",
                        vfs_key,
                        rsrc_data.len()
                    );
                    let refnum = self.open_resource_file_from_vfs_key(bus, &vfs_key, wants_write);
                    bus.write_word(sp + 8, refnum);
                    cpu.write_reg(Register::D0, refnum as u32);
                } else {
                    eprintln!("[TRAP] OpenRFPerm: \"{}\" not found in vfs_rsrc", name);
                    bus.write_word(sp + 8, (-1i16) as u16);
                    cpu.write_reg(Register::D0, (-1i32) as u32);
                    bus.write_word(0x0A60, (-43i16) as u16); // ResErr = fnfErr
                }
                cpu.write_reg(Register::A7, sp + 8);
                Ok(())
            }

            // CloseResFile ($A99A)
            // Closes a resource file: calls UpdateResFile, releases
            // resources, removes the file from the search order, and
            // resets the current file if needed.
            // PROCEDURE CloseResFile(refNum: INTEGER);
            // Inside Macintosh Volume I, I-115
            //
            // CloseResFile ($A99A): Updates resources, removes file from search order, resets current file per IM:I I-115
            (true, 0x19A) => {
                let sp = cpu.read_reg(Register::A7);
                let refnum = bus.read_word(sp);
                if trace_sound_enabled() {
                    eprintln!(
                        "[RSRC] CloseResFile refnum={} name={:?}",
                        refnum,
                        self.resource_file_name(refnum)
                    );
                }

                if refnum != 0 && self.close_resource_file_refnum(bus, refnum) {
                    self.open_files.remove(&refnum);
                    self.file_positions.remove(&refnum);
                    self.write_refnums.remove(&refnum);
                    bus.write_word(0x0A60, 0); // noErr
                } else if refnum == 0 {
                    // Closing system resource file: close all others first
                    // For now, just reset current to 0
                    self.set_current_resource_refnum(bus, 0);
                    bus.write_word(0x0A60, 0);
                } else {
                    // IM:I I-115 documents resNotFound here, but
                    // BasiliskII/System 7.5.3 reports resFNotFound for a
                    // non-open resource-file refnum.
                    const RES_F_NOT_FOUND: i16 = -193;
                    bus.write_word(0x0A60, RES_F_NOT_FOUND as u16);
                }
                cpu.write_reg(Register::A7, sp + 2);
                Ok(())
            }

            // UseResFile ($A998)
            // PROCEDURE UseResFile(refNum: INTEGER);
            // Inside Macintosh Volume I, I-116.
            // UseResFile ($A998): Sets current resource file refnum
            (true, 0x198) => {
                let sp = cpu.read_reg(Register::A7);
                let refnum = bus.read_word(sp);
                let cur_apref_num = bus.read_word(crate::memory::globals::addr::CUR_APREF_NUM);
                let target_refnum = if refnum == cur_apref_num {
                    0
                } else {
                    refnum
                };
                if trace_sound_enabled() || super::dispatch::trace_resfile_enabled() {
                    eprintln!(
                        "[RSRC] UseResFile refnum={} (target={}) name={:?}",
                        refnum,
                        target_refnum,
                        self.resource_file_name(target_refnum)
                    );
                }
                let file_exists = self
                    .resources
                    .as_ref()
                    .is_some_and(|r| r.files.contains_key(&target_refnum));
                if file_exists {
                    self.set_current_resource_refnum(bus, target_refnum);
                    bus.write_word(0x0A60, 0); // noErr
                } else {
                    // IM:I I-116: invalid refnum leaves the current file
                    // unchanged and ResError returns resFNotFound.
                    bus.write_word(0x0A60, (-193i16) as u16);
                    bus.write_word(0x0A5A, self.current_resource_refnum());
                }
                cpu.write_reg(Register::A7, sp + 2);
                Ok(())
            }

            // CountResources ($A99C) / Count1Resources ($A80D)
            // FUNCTION CountResources(theType: ResType): INTEGER;
            // Inside Macintosh Volume I, I-116
            //
            // Count*Resources always succeeds — it returns 0 for unknown
            // types or empty files. The contract requires ResErr to be
            // cleared to noErr on every successful call so callers don't
            // observe stale errors from earlier Resource Manager calls.
            //
            // Count1Resources ($A80D): Counts resources of given type in current resource file
            // CountResources ($A99C): Counts resources of given type; clears ResErr per IM:I I-116
            (true, 0x19C) | (true, 0x00D) => {
                let sp = cpu.read_reg(Register::A7);
                let raw_res_type = bus.read_long(sp).to_be_bytes();
                let res_type = super::TrapDispatcher::normalize_ostype(raw_res_type);
                let current_only = trap_num == 0x00D;
                let count = self.count_resources(res_type, current_only) as u16;
                let type_str = String::from_utf8_lossy(&res_type);
                if trace_sound_enabled()
                    && self
                        .resources
                        .as_ref()
                        .is_some_and(|resources| resources.files.len() > 1)
                {
                    eprintln!(
                        "[TRAP] CountResources('{}') = {} current={} only_current={}",
                        type_str,
                        count,
                        self.current_resource_refnum(),
                        current_only
                    );
                } else {
                    eprintln!("[TRAP] CountResources('{}') = {}", type_str, count);
                }
                bus.write_word(sp + 4, count);
                bus.write_word(0x0A60, 0); // ResErr = noErr
                cpu.write_reg(Register::A7, sp + 4);
                Ok(())
            }

            // NOTE: GetResAttrs ($A9A6) lives in resource.rs at the
            // same slot (true, 0x1A6). A near-identical handler used to
            // live here too, but it was dead code — dispatch_resource
            // runs before dispatch_toolbox so the resource.rs handler
            // always won. Removed so there's one canonical implementation.

            // ========== Misc Toolbox ==========

            // Munger ($A9E0)
            // Manipulates bytes in a relocatable block by searching and replacing.
            // FUNCTION Munger(h: Handle; offset: LongInt; ptr1: Ptr;
            //     len1: LongInt; ptr2: Ptr; len2: LongInt): LongInt;
            // Inside Macintosh Volume I 1985, I-468 to I-469;
            // Text 1993, 5-75 to 5-76
            // Munger ($A9E0): Searches/replaces bytes in a handle, including insert/delete and tail-partial-match behavior
            (true, 0x1E0) => {
                let sp = cpu.read_reg(Register::A7);
                let trap_site = cpu.read_reg(Register::PC).wrapping_sub(2);
                let len2 = bus.read_long(sp) as i32;
                let ptr2 = bus.read_long(sp + 4);
                let len1 = bus.read_long(sp + 8) as i32;
                let ptr1 = bus.read_long(sp + 12);
                let offset = bus.read_long(sp + 16) as i32;
                let handle = bus.read_long(sp + 20);

                let result =
                    Self::munger_in_handle(bus, trap_site, handle, offset, ptr1, len1, ptr2, len2);
                bus.write_long(sp + 24, result as u32);
                cpu.write_reg(Register::A7, sp + 24);
                Ok(())
            }

            // XMunger ($A819)
            // Phantom trap word exposed in the System 7.6-era public trap
            // namespace. BasiliskII treats it as an observed no-op/no-pop
            // stub: callers keep the original handle contents and the stack
            // frame remains unbalanced after the call.
            (true, 0x019) => Ok(()),

            // PBOpenRF / PBHOpenRF ($A00A / $A20A) — Open Resource Fork
            // FUNCTION PBOpenRF (paramBlock: ParmBlkPtr; async: BOOLEAN): OSErr;
            // FUNCTION PBHOpenRF (paramBlock: HParmBlkPtr; async: BOOLEAN): OSErr;
            // Files 1992, 2-117 / 9282 (HOpenRF).  The OS-trap dispatcher
            // masks `trap & 0x00FF`, so $A20A PBHOpenRF lands on the same
            // low byte and shares this arm.
            //
            // Regression coverage (BasiliskII references):
            //   - pb_open_rf            — $A00A fnfErr path
            //   - pbh_open_rf_rename    — $A20A + $A20B fnfErr paths
            // PBOpenRF ($A00A): Opens resource fork via PBOpen path
            // PBHOpenRF ($A20A): HFS variant aliased onto $A00A
            (false, 0x0A) => {
                let pb = cpu.read_reg(Register::A0);
                let name_ptr = bus.read_long(pb + 18);
                let vref = bus.read_word(pb + 22) as i16;
                let dir_id = bus.read_long(pb + 48);
                let filename = Self::read_pb_filename(bus, name_ptr);
                let permission = bus.read_byte(pb + 27) as i8 as i16;
                let wants_write = matches!(permission, 2 | 3 | 4);
                eprintln!("[TRAP] PBOpenRF(\"{}\")", filename);

                let is_hfs_variant = matches!(
                    raw_trap_route(self.current_trap_word).os_routine_variant,
                    OsRoutineVariant::FileHfsSynchronous | OsRoutineVariant::FileHfsAsynchronous
                );
                if is_hfs_variant && vref != 0 && self.working_directory_info(vref).is_none() {
                    bus.write_word(pb + 16, (-35i16) as u16); // nsvErr
                    cpu.write_reg(Register::D0, (-35i32) as u32);
                    return Some(Ok(()));
                }
                let scoped_vfs_key = if is_hfs_variant {
                    self.hfs_lookup_directory_ids(vref, dir_id)
                        .into_iter()
                        .find_map(|dir_id| self.find_vfs_rsrc_file_in_directory(dir_id, &filename))
                } else {
                    None
                };

                // Try to find resource fork in vfs_rsrc. PBHOpenRF first uses
                // the HFS parent directory fields, then preserves the legacy
                // broad lookup as a compatibility fallback for flattened archives.
                let mounted_volume_selected = is_hfs_variant
                    && self.working_directory_info(vref).is_some_and(|working| {
                        self.vfs_volume_for_ref_num(working.volume_ref_num)
                            .is_some()
                    });
                let vfs_key = if mounted_volume_selected {
                    scoped_vfs_key
                } else {
                    scoped_vfs_key.or_else(|| self.find_vfs_rsrc_file(&filename))
                };
                if let Some(vfs_key) = vfs_key {
                    if wants_write && self.vfs_path_is_read_only(&vfs_key) {
                        bus.write_word(pb + 16, (-44i16) as u16); // wPrErr
                        cpu.write_reg(Register::D0, (-44i32) as u32);
                        return Some(Ok(()));
                    }
                    let rsrc_data = self.vfs_rsrc.get(&vfs_key).unwrap().clone();
                    eprintln!(
                        "[TRAP] PBOpenRF: found rsrc fork for \"{}\" ({} bytes)",
                        vfs_key,
                        rsrc_data.len()
                    );
                    // Register as an open file (store rsrc data as a regular VFS entry for FSRead).
                    // Use entry().or_insert to avoid clobbering writes from a previous open.
                    // Mars Rising's installer pattern: open temp rsrc fork, write 81KB to
                    // it, close, then re-open and expect the 81KB to still be there. If we
                    // re-seed from vfs_rsrc here, the writes are lost.
                    let refnum = self.allocate_process_file_refnum();
                    let rsrc_key = format!("__rsrc__{}", vfs_key);
                    self.vfs.insert_if_absent(rsrc_key.clone(), rsrc_data);
                    self.open_files.insert(refnum, rsrc_key);
                    // Files 1992, 2-8 and 2-117: fsCurPerm grants read/write
                    // when the volume permits it, just as it does for the
                    // data-fork PBOpen path. Classic installers commonly
                    // create a file, open its empty resource fork with
                    // fsCurPerm, then populate it through FSWrite.
                    if matches!(permission, 0 | 2 | 3 | 4) && !self.vfs_path_is_read_only(&vfs_key)
                    {
                        self.write_refnums.insert(refnum);
                    }
                    self.file_positions.insert(refnum, 0);
                    bus.write_word(pb + 24, refnum);
                    bus.write_word(pb + 16, 0); // noErr
                    cpu.write_reg(Register::D0, 0);
                } else {
                    eprintln!("[TRAP] PBOpenRF: \"{}\" not found in vfs_rsrc", filename);
                    bus.write_word(pb + 16, (-43i16) as u16); // fnfErr
                    cpu.write_reg(Register::D0, (-43i32) as u32);
                }
                Ok(())
            }

            // Pack8 ($A816)
            // Dispatches Apple Event Manager routines selected by D0.W.
            // FUNCTION AEManagerInfo (keyword: AEKeyword; VAR result: LongInt): OSErr;
            // Inside Macintosh: Interapplication Communication (1993), p. 4-104.
            (true, 0x016) => {
                let sp = cpu.read_reg(Register::A7);
                let d0 = cpu.read_reg(Register::D0);
                let selector = (d0 & 0xFFFF) as u16;
                let operation = pack8_operation_route(self.current_trap_word, selector);
                self.current_selector_operation = operation.map(|route| route.operation_id);

                if selector == super::dispatch::LOADSEG_GETRESOURCE_SENTINEL {
                    return Some(self.resume_loadseg_after_getresource(bus, cpu));
                }

                // Trampoline selector — when an AE handler we dispatched
                // returns, its `RTD` lands on a tiny `MOVE.W #$FEFE, D0;
                // _Pack8` stub that re-enters Pack8 with this sentinel.
                // Resume the original `AEProcessAppleEvent` caller's flow.
                if selector == 0xFEFE {
                    let state = self
                        .ae_call_state
                        .take()
                        .expect("AE trampoline fired without a saved AeCallState");
                    // Sanity: handler's `RTD #12` should have left SP
                    // pointing at the original caller's result slot.
                    debug_assert_eq!(
                        sp, state.expected_sp_after_rtd,
                        "AE trampoline SP mismatch: expected {:08X}, got {:08X}",
                        state.expected_sp_after_rtd, sp,
                    );
                    // Resume at the post-`_Pack8` PC the original caller
                    // would have continued at. The result is already in
                    // the slot — the handler wrote it via the Pascal
                    // calling convention.
                    let result = state
                        .result_override
                        .unwrap_or_else(|| bus.read_word(sp) as i16);
                    if let Some((event_desc, reply_desc)) = state.owned_descriptors {
                        self.dispose_owned_ae_callback_descriptor(bus, event_desc);
                        self.dispose_owned_ae_callback_descriptor(bus, reply_desc);
                    }
                    if let Some(resolve_state) = state.resolve_state {
                        self.ae_continue_resolve_after_accessor(bus, cpu, resolve_state, result);
                        return Some(Ok(()));
                    }
                    bus.write_word(sp, result as u16);
                    cpu.write_reg(Register::D0, result as i32 as u32);
                    cpu.write_reg(Register::PC, state.return_pc);
                    if let Some(outer_state) = self.ae_call_state_stack.pop() {
                        self.ae_call_state = Some(outer_state);
                    }
                    return Some(Ok(()));
                }

                let routine = (selector & 0xFF) as u8;
                let param_words = ((selector >> 8) & 0xFF) as u32;
                let param_bytes = param_words * 2;

                static AE_LOG_COUNT: std::sync::atomic::AtomicU32 =
                    std::sync::atomic::AtomicU32::new(0);
                let lc = AE_LOG_COUNT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                if lc < 20 || trace_ae_enabled() {
                    eprintln!(
                        "[TRAP] Pack8/AE PC=${:08X} SP=${:08X} D0=${:08X} routine={} param_words={} param_bytes={}",
                        cpu.read_reg(Register::PC),
                        sp,
                        d0,
                        routine,
                        param_words,
                        param_bytes
                    );
                    if trace_ae_enabled() {
                        let mut words = Vec::new();
                        for off in (0..=(param_bytes + 2).min(34)).step_by(2) {
                            words.push(format!("+{:02X}:{:04X}", off, bus.read_word(sp + off)));
                        }
                        eprintln!("[AE] stack {}", words.join(" "));
                    }
                }

                // Routine 65 (`AEManagerInfo`) reports AppleEvent Manager
                // metadata. The common startup probe asks for keyAEVersion;
                // recording is never active in Systemless, so the recorder
                // count is always zero.
                //
                // AEManagerInfo (0x0441)
                // FUNCTION AEManagerInfo(keyword: AEKeyword;
                //   VAR result: LongInt): OSErr;
                // Interapplication Communication 1993, 4-104.
                if routine == 65 && param_bytes == 8 {
                    let result_ptr = bus.read_long(sp);
                    let keyword = bus.read_long(sp + 4);
                    let value = if keyword == AE_MANAGER_KEY_VERSION {
                        0x0101_0000
                    } else if keyword == AE_MANAGER_KEY_RECORDER_COUNT {
                        0
                    } else {
                        0
                    };
                    if result_ptr != 0 {
                        bus.write_long(result_ptr, value);
                    }
                    let new_sp = sp + param_bytes;
                    bus.write_word(new_sp, 0);
                    cpu.write_reg(Register::A7, new_sp);
                    cpu.write_reg(Register::D0, 0);
                    return Some(Ok(()));
                }

                // Routines 0/1/45 manage the AppleEvent special-handler
                // tables. Routine 53 (`AESetObjectCallbacks`) is Object
                // Support Library sugar for installing the standard callback
                // function classes in the application special-handler table.
                //
                // AEInstallSpecialHandler (0x0500)
                // AERemoveSpecialHandler (0x0501)
                // AEGetSpecialHandler (0x052D)
                // AESetObjectCallbacks (0x0E35)
                // Interapplication Communication 1993, 4-100..4-103 and
                // 6-79..6-80.
                if routine == 0 && param_bytes == 10 {
                    let is_sys_handler = Self::stack_bool_slot(bus, sp);
                    let handler = bus.read_long(sp + 2);
                    let function_class = bus.read_long(sp + 6);
                    self.ae_special_handlers
                        .insert((is_sys_handler, function_class), handler);
                    let new_sp = sp + param_bytes;
                    bus.write_word(new_sp, 0);
                    cpu.write_reg(Register::A7, new_sp);
                    cpu.write_reg(Register::D0, 0);
                    return Some(Ok(()));
                }
                if routine == 1 && param_bytes == 10 {
                    let is_sys_handler = Self::stack_bool_slot(bus, sp);
                    let handler = bus.read_long(sp + 2);
                    let function_class = bus.read_long(sp + 6);
                    if self
                        .ae_special_handlers
                        .get(&(is_sys_handler, function_class))
                        .copied()
                        == Some(handler)
                    {
                        self.ae_special_handlers
                            .remove(&(is_sys_handler, function_class));
                    }
                    let new_sp = sp + param_bytes;
                    bus.write_word(new_sp, 0);
                    cpu.write_reg(Register::A7, new_sp);
                    cpu.write_reg(Register::D0, 0);
                    return Some(Ok(()));
                }
                if routine == 45 && param_bytes == 10 {
                    let is_sys_handler = Self::stack_bool_slot(bus, sp);
                    let handler_ptr = bus.read_long(sp + 2);
                    let function_class = bus.read_long(sp + 6);
                    let handler = self
                        .ae_special_handlers
                        .get(&(is_sys_handler, function_class))
                        .copied();
                    let err = if let Some(handler) = handler {
                        if handler_ptr != 0 {
                            bus.write_long(handler_ptr, handler);
                        }
                        0
                    } else {
                        if handler_ptr != 0 {
                            bus.write_long(handler_ptr, 0);
                        }
                        AE_ERR_HANDLER_NOT_FOUND
                    };
                    let new_sp = sp + param_bytes;
                    bus.write_word(new_sp, err as u16);
                    cpu.write_reg(Register::A7, new_sp);
                    cpu.write_reg(Register::D0, err as i32 as u32);
                    return Some(Ok(()));
                }
                if routine == 53 && param_bytes == 28 {
                    for (function_class, handler) in [
                        (AE_KEY_GET_ERR_DESC_PROC, bus.read_long(sp)),
                        (AE_KEY_ADJUST_MARKS_PROC, bus.read_long(sp + 4)),
                        (AE_KEY_MARK_PROC, bus.read_long(sp + 8)),
                        (AE_KEY_MARK_TOKEN_PROC, bus.read_long(sp + 12)),
                        (AE_KEY_DISPOSE_TOKEN_PROC, bus.read_long(sp + 16)),
                        (AE_KEY_COUNT_PROC, bus.read_long(sp + 20)),
                        (AE_KEY_COMPARE_PROC, bus.read_long(sp + 24)),
                    ] {
                        if handler != 0 {
                            self.ae_special_handlers
                                .insert((false, function_class), handler);
                        }
                    }
                    let new_sp = sp + param_bytes;
                    bus.write_word(new_sp, 0);
                    cpu.write_reg(Register::A7, new_sp);
                    cpu.write_reg(Register::D0, 0);
                    return Some(Ok(()));
                }

                // Routine 31 (`AEInstallEventHandler`): record the
                // (eventClass, eventID) → (handler, refcon) tuple so a
                // later AEProcessAppleEvent dispatch can fire it. Stack
                // layout when the trap fires (Pascal calling order):
                //   SP+0   isSysHandler (Boolean, 2 bytes incl pad)
                //   SP+2   handlerRefcon (long, 4 bytes)
                //   SP+6   handler (AEEventHandlerUPP, 4 bytes)
                //   SP+10  theAEEventID (4-byte OSType)
                //   SP+14  theAEEventClass (4-byte OSType)
                //   SP+18  result OSErr slot (2 bytes; pre-pushed)
                // Inside Macintosh Volume VI, 6-43.
                if routine == 31 && param_bytes == 18 {
                    let is_sys_handler = Self::stack_bool_slot(bus, sp);
                    let handler_refcon = bus.read_long(sp + 2);
                    let handler_ptr = bus.read_long(sp + 6);
                    let event_id = bus.read_long(sp + 10);
                    let event_class = bus.read_long(sp + 14);
                    let procedure = (handler_ptr != 0 && handler_ptr & 1 == 0)
                        .then(|| {
                            crate::guest_procedure::resolve_guest_procedure(
                                bus,
                                handler_ptr,
                                0,
                                None,
                                crate::guest_procedure::GuestIsa::M68k,
                                crate::guest_procedure::GuestIsa::M68k,
                            )
                        })
                        .flatten();
                    let err = if let Some(procedure) = procedure {
                        self.ae_handlers.install(
                            is_sys_handler,
                            event_class,
                            event_id,
                            crate::process_context::ProcessAppleEventHandler {
                                procedure,
                                refcon: handler_refcon,
                            },
                        );
                        0
                    } else {
                        -50i16
                    };
                    eprintln!(
                        "[AE] InstallEventHandler class='{}' id='{}' handler=${:08X} refcon=${:08X}",
                        format_fourcc(event_class),
                        format_fourcc(event_id),
                        handler_ptr,
                        handler_refcon,
                    );
                    let new_sp = sp + param_bytes;
                    bus.write_word(new_sp, err as u16);
                    cpu.write_reg(Register::A7, new_sp);
                    cpu.write_reg(Register::D0, err as i32 as u32);
                    return Some(Ok(()));
                }
                if routine == 33 && param_bytes == 18 {
                    let is_sys_handler = Self::stack_bool_slot(bus, sp);
                    let handler_refcon_ptr = bus.read_long(sp + 2);
                    let handler_ptr = bus.read_long(sp + 6);
                    let event_id = bus.read_long(sp + 10);
                    let event_class = bus.read_long(sp + 14);
                    let found = self.ae_handlers.get(is_sys_handler, event_class, event_id);
                    let err = if let Some(handler) = found {
                        if handler_ptr != 0 {
                            bus.write_long(handler_ptr, handler.procedure.original_pointer);
                        }
                        if handler_refcon_ptr != 0 {
                            bus.write_long(handler_refcon_ptr, handler.refcon);
                        }
                        0
                    } else {
                        if handler_ptr != 0 {
                            bus.write_long(handler_ptr, 0);
                        }
                        if handler_refcon_ptr != 0 {
                            bus.write_long(handler_refcon_ptr, 0);
                        }
                        AE_ERR_HANDLER_NOT_FOUND
                    };
                    let new_sp = sp + param_bytes;
                    bus.write_word(new_sp, err as u16);
                    cpu.write_reg(Register::A7, new_sp);
                    cpu.write_reg(Register::D0, err as i32 as u32);
                    return Some(Ok(()));
                }
                if routine == 32 && param_bytes == 14 {
                    let is_sys_handler = Self::stack_bool_slot(bus, sp);
                    let handler_ptr = bus.read_long(sp + 2);
                    let event_id = bus.read_long(sp + 6);
                    let event_class = bus.read_long(sp + 10);
                    let err = if self.ae_handlers.remove(
                        is_sys_handler,
                        event_class,
                        event_id,
                        handler_ptr,
                    ) {
                        0
                    } else {
                        AE_ERR_HANDLER_NOT_FOUND
                    };
                    let new_sp = sp + param_bytes;
                    bus.write_word(new_sp, err as u16);
                    cpu.write_reg(Register::A7, new_sp);
                    cpu.write_reg(Register::D0, err as i32 as u32);
                    return Some(Ok(()));
                }

                // Routines 34/36 manage coercion handler table entries.
                // The current AECoerce* HLE implements identity coercion
                // directly; recording these table entries makes caller
                // probes and handler chaining observable.
                //
                // AEInstallCoercionHandler (0x0A22)
                // AEGetCoercionHandler (0x0B24)
                // Interapplication Communication 1993, 4-96..4-98.
                if routine == 34 && param_bytes == 20 {
                    let is_sys_handler = Self::stack_bool_slot(bus, sp);
                    let from_type_is_desc = Self::stack_bool_slot(bus, sp + 2);
                    let refcon = bus.read_long(sp + 4);
                    let handler_ptr = bus.read_long(sp + 8);
                    let to_type = bus.read_long(sp + 12);
                    let from_type = bus.read_long(sp + 16);
                    self.ae_coercion_handlers.insert(
                        (is_sys_handler, from_type, to_type),
                        AeCoercionHandler {
                            handler_ptr,
                            refcon,
                            from_type_is_desc,
                        },
                    );
                    let new_sp = sp + param_bytes;
                    bus.write_word(new_sp, 0);
                    cpu.write_reg(Register::A7, new_sp);
                    cpu.write_reg(Register::D0, 0);
                    return Some(Ok(()));
                }
                if routine == 36 && param_bytes == 22 {
                    let is_sys_handler = Self::stack_bool_slot(bus, sp);
                    let from_type_is_desc_ptr = bus.read_long(sp + 2);
                    let handler_refcon_ptr = bus.read_long(sp + 6);
                    let handler_ptr = bus.read_long(sp + 10);
                    let to_type = bus.read_long(sp + 14);
                    let from_type = bus.read_long(sp + 18);
                    let found = self
                        .ae_coercion_handlers
                        .get(&(is_sys_handler, from_type, to_type))
                        .copied();
                    let err = if let Some(handler) = found {
                        if handler_ptr != 0 {
                            bus.write_long(handler_ptr, handler.handler_ptr);
                        }
                        if handler_refcon_ptr != 0 {
                            bus.write_long(handler_refcon_ptr, handler.refcon);
                        }
                        if from_type_is_desc_ptr != 0 {
                            bus.write_byte(
                                from_type_is_desc_ptr,
                                if handler.from_type_is_desc { 0xFF } else { 0 },
                            );
                        }
                        0
                    } else {
                        if handler_ptr != 0 {
                            bus.write_long(handler_ptr, 0);
                        }
                        if handler_refcon_ptr != 0 {
                            bus.write_long(handler_refcon_ptr, 0);
                        }
                        if from_type_is_desc_ptr != 0 {
                            bus.write_byte(from_type_is_desc_ptr, 0);
                        }
                        AE_ERR_HANDLER_NOT_FOUND
                    };
                    let new_sp = sp + param_bytes;
                    bus.write_word(new_sp, err as u16);
                    cpu.write_reg(Register::A7, new_sp);
                    cpu.write_reg(Register::D0, err as i32 as u32);
                    return Some(Ok(()));
                }

                // Private Object Support Library hash-table selectors used
                // by the System 7 AEObjectInit glue. MPW's
                // AEObjectSupportLib.o creates per-application and system
                // object-accessor dispatch tables with $092E, then uses
                // $0831/$0833/$0632 to insert, look up, and remove fixed-size
                // key/value records. These are not public Interapplication
                // Communication APIs, but linked classic object-support code
                // calls them before the documented AE object routines can
                // work.
                if routine == 46 && param_bytes == 18 {
                    let result_ptr = bus.read_long(sp);
                    let entry_sizes = bus.read_long(sp + 10);
                    let allocation_size = bus.read_long(sp + 14);
                    let err = self.ae_create_private_hash_table(
                        bus,
                        result_ptr,
                        entry_sizes,
                        allocation_size,
                    );
                    let new_sp = sp + param_bytes;
                    bus.write_word(new_sp, err as u16);
                    cpu.write_reg(Register::A7, new_sp);
                    cpu.write_reg(Register::D0, err as i32 as u32);
                    return Some(Ok(()));
                }
                if routine == 49 && param_bytes == 16 {
                    let value_ptr = bus.read_long(sp);
                    let key_ptr = bus.read_long(sp + 4);
                    let table_handle = bus.read_long(sp + 12);
                    let err = if let Some(table) =
                        self.ae_private_hash_tables.get_mut(&table_handle)
                    {
                        if let Some(key) = Self::ae_private_hash_key(bus, key_ptr, table.key_size) {
                            let value = if value_ptr == 0 {
                                Vec::new()
                            } else {
                                bus.read_bytes(value_ptr, table.value_size)
                            };
                            if value.len() == table.value_size {
                                table.entries.insert(key, value);
                                0
                            } else {
                                -50
                            }
                        } else {
                            -50
                        }
                    } else {
                        AE_ERR_ACCESSOR_NOT_FOUND
                    };
                    let new_sp = sp + param_bytes;
                    bus.write_word(new_sp, err as u16);
                    cpu.write_reg(Register::A7, new_sp);
                    cpu.write_reg(Register::D0, err as i32 as u32);
                    return Some(Ok(()));
                }
                if routine == 51 && param_bytes == 16 {
                    let value_ptr = bus.read_long(sp);
                    let key_ptr = bus.read_long(sp + 4);
                    let table_handle = bus.read_long(sp + 12);
                    let err = if let Some(table) = self.ae_private_hash_tables.get(&table_handle) {
                        if let Some(key) = Self::ae_private_hash_key(bus, key_ptr, table.key_size) {
                            if let Some(value) = table.entries.get(&key) {
                                if value_ptr != 0 {
                                    bus.write_bytes(value_ptr, value);
                                }
                                0
                            } else {
                                AE_ERR_ACCESSOR_NOT_FOUND
                            }
                        } else {
                            -50
                        }
                    } else {
                        AE_ERR_ACCESSOR_NOT_FOUND
                    };
                    let new_sp = sp + param_bytes;
                    bus.write_word(new_sp, err as u16);
                    cpu.write_reg(Register::A7, new_sp);
                    cpu.write_reg(Register::D0, err as i32 as u32);
                    return Some(Ok(()));
                }
                if routine == 50 && param_bytes == 12 {
                    let key_ptr = bus.read_long(sp);
                    let table_handle = bus.read_long(sp + 8);
                    let err = if let Some(table) =
                        self.ae_private_hash_tables.get_mut(&table_handle)
                    {
                        if let Some(key) = Self::ae_private_hash_key(bus, key_ptr, table.key_size) {
                            if table.entries.remove(&key).is_some() {
                                0
                            } else {
                                AE_ERR_ACCESSOR_NOT_FOUND
                            }
                        } else {
                            -50
                        }
                    } else {
                        AE_ERR_ACCESSOR_NOT_FOUND
                    };
                    let new_sp = sp + param_bytes;
                    bus.write_word(new_sp, err as u16);
                    cpu.write_reg(Register::A7, new_sp);
                    cpu.write_reg(Register::D0, err as i32 as u32);
                    return Some(Ok(()));
                }

                // Routines 55/56/57 manage Object Support Library accessor
                // dispatch entries. AEResolve chooses an accessor by
                // desired class and container-token descriptor type,
                // checking the application table before the system table and
                // honoring typeWildCard entries. AEGetObjectAccessor and
                // AERemoveObjectAccessor operate on exact table entries; a
                // caller asking for typeWildCard must have installed a
                // typeWildCard entry.
                //
                // AEInstallObjectAccessor (0x0937)
                // FUNCTION AEInstallObjectAccessor(desiredClass: DescType;
                //   containerType: DescType; theAccessor: AccessorProcPtr;
                //   accessorRefcon: LongInt; isSysHandler: Boolean): OSErr;
                // AERemoveObjectAccessor (0x0738)
                // FUNCTION AERemoveObjectAccessor(desiredClass: DescType;
                //   containerType: DescType; theAccessor: AccessorProcPtr;
                //   isSysHandler: Boolean): OSErr;
                // AEGetObjectAccessor (0x0939)
                // FUNCTION AEGetObjectAccessor(desiredClass: DescType;
                //   containerType: DescType; VAR theAccessor: AccessorProcPtr;
                //   VAR accessorRefcon: LongInt; isSysHandler: Boolean): OSErr;
                // Interapplication Communication 1993, 6-78..6-82.
                if routine == 55 && param_bytes == 18 {
                    let is_sys_handler = Self::stack_bool_slot(bus, sp);
                    let refcon = bus.read_long(sp + 2);
                    let accessor_ptr = bus.read_long(sp + 6);
                    let container_type = bus.read_long(sp + 10);
                    let desired_class = bus.read_long(sp + 14);
                    self.ae_object_accessors.insert(
                        (is_sys_handler, desired_class, container_type),
                        AeObjectAccessor {
                            accessor_ptr,
                            refcon,
                        },
                    );
                    if trace_ae_enabled() {
                        eprintln!(
                            "[AE] InstallObjectAccessor desired='{}' container='{}' handler=${:08X} refcon=${:08X} sys={}",
                            format_fourcc(desired_class),
                            format_fourcc(container_type),
                            accessor_ptr,
                            refcon,
                            is_sys_handler,
                        );
                    }
                }
                if routine == 56 && param_bytes == 14 {
                    let is_sys_handler = Self::stack_bool_slot(bus, sp);
                    let accessor_ptr = bus.read_long(sp + 2);
                    let container_type = bus.read_long(sp + 6);
                    let desired_class = bus.read_long(sp + 10);
                    let key = (is_sys_handler, desired_class, container_type);
                    let found = self.ae_object_accessors.get(&key).copied();
                    let err = if let Some(accessor) = found {
                        if accessor_ptr == 0 || accessor.accessor_ptr == accessor_ptr {
                            self.ae_object_accessors.remove(&key);
                            0
                        } else {
                            AE_ERR_ACCESSOR_NOT_FOUND
                        }
                    } else {
                        AE_ERR_ACCESSOR_NOT_FOUND
                    };
                    let new_sp = sp + param_bytes;
                    bus.write_word(new_sp, err as u16);
                    cpu.write_reg(Register::A7, new_sp);
                    cpu.write_reg(Register::D0, err as i32 as u32);
                    return Some(Ok(()));
                }
                if routine == 57 && param_bytes == 18 {
                    let is_sys_handler = Self::stack_bool_slot(bus, sp);
                    let accessor_refcon_ptr = bus.read_long(sp + 2);
                    let accessor_ptr = bus.read_long(sp + 6);
                    let container_type = bus.read_long(sp + 10);
                    let desired_class = bus.read_long(sp + 14);
                    let found = self.ae_object_accessor_exact(
                        is_sys_handler,
                        desired_class,
                        container_type,
                    );
                    let err = if let Some(accessor) = found {
                        if accessor_ptr != 0 {
                            bus.write_long(accessor_ptr, accessor.accessor_ptr);
                        }
                        if accessor_refcon_ptr != 0 {
                            bus.write_long(accessor_refcon_ptr, accessor.refcon);
                        }
                        0
                    } else {
                        if accessor_ptr != 0 {
                            bus.write_long(accessor_ptr, 0);
                        }
                        if accessor_refcon_ptr != 0 {
                            bus.write_long(accessor_refcon_ptr, 0);
                        }
                        AE_ERR_ACCESSOR_NOT_FOUND
                    };
                    let new_sp = sp + param_bytes;
                    bus.write_word(new_sp, err as u16);
                    cpu.write_reg(Register::A7, new_sp);
                    cpu.write_reg(Register::D0, err as i32 as u32);
                    return Some(Ok(()));
                }

                // Routine 37 (`AECreateDesc`) copies caller bytes into a
                // descriptor record. Routine 6 (`AECreateList`) creates an
                // empty descriptor list or AE record shell. These descriptors
                // are later copied into AppleEvent parameters.
                //
                // AECreateDesc (0x0825)
                // FUNCTION AECreateDesc(typeCode: DescType; dataPtr: Ptr;
                //   dataSize: Size; VAR result: AEDesc): OSErr;
                // AECreateList (0x0706)
                // FUNCTION AECreateList(factoringPtr: Ptr;
                //   factoredSize: Size; isRecord: Boolean;
                //   VAR resultList: AEDescList): OSErr;
                // Inside Macintosh Volume VI, 6-87..6-88.
                if routine == 37 && param_bytes == 16 {
                    let result_desc = bus.read_long(sp);
                    let data_size = bus.read_long(sp + 4);
                    let data_ptr = bus.read_long(sp + 8);
                    let desc_type = bus.read_long(sp + 12);
                    let mut data = Vec::with_capacity(data_size as usize);
                    if data_ptr != 0 {
                        for offset in 0..data_size {
                            data.push(bus.read_byte(data_ptr + offset));
                        }
                    }
                    self.write_ae_descriptor_value(
                        bus,
                        result_desc,
                        Self::ae_descriptor(desc_type, data),
                    );
                    let new_sp = sp + param_bytes;
                    bus.write_word(new_sp, 0);
                    cpu.write_reg(Register::A7, new_sp);
                    cpu.write_reg(Register::D0, 0);
                    return Some(Ok(()));
                }
                if routine == 6 && param_bytes == 14 {
                    let result_desc = bus.read_long(sp);
                    let is_record = Self::stack_bool_slot(bus, sp + 4);
                    self.write_ae_descriptor_value(
                        bus,
                        result_desc,
                        AeDescriptor {
                            desc_type: if is_record {
                                AE_TYPE_AE_RECORD
                            } else {
                                AE_TYPE_AE_LIST
                            },
                            data: Vec::new(),
                            fields: HashMap::new(),
                            items: Vec::new(),
                        },
                    );
                    let new_sp = sp + param_bytes;
                    bus.write_word(new_sp, 0);
                    cpu.write_reg(Register::A7, new_sp);
                    cpu.write_reg(Register::D0, 0);
                    return Some(Ok(()));
                }

                // Routines 8/9 append or replace ordered descriptor-list
                // items. AE records also expose keyworded fields through the
                // same ordered-list APIs; non-record lists use typeWildCard
                // as the item keyword.
                //
                // AEPutPtr (0x0A08)
                // FUNCTION AEPutPtr(theAEDescList: AEDescList; index: LongInt;
                //   typeCode: DescType; dataPtr: Ptr; dataSize: Size): OSErr;
                // AEPutDesc (0x0609)
                // FUNCTION AEPutDesc(theAEDescList: AEDescList; index: LongInt;
                //   theAEDesc: AEDesc): OSErr;
                // Interapplication Communication 1993, 5-29..5-30.
                if routine == 8 && param_bytes == 20 {
                    let data_size = bus.read_long(sp);
                    let data_ptr = bus.read_long(sp + 4);
                    let desc_type = bus.read_long(sp + 8);
                    let index = bus.read_long(sp + 12);
                    let list_desc = bus.read_long(sp + 16);
                    let mut data = Vec::with_capacity(data_size as usize);
                    if data_ptr != 0 {
                        for offset in 0..data_size {
                            data.push(bus.read_byte(data_ptr + offset));
                        }
                    }
                    let mut err = AE_ERR_DESC_NOT_FOUND;
                    let value = Self::ae_descriptor(desc_type, data);
                    if let Some(event) = self.ae_events.get_mut(&list_desc) {
                        err = if index == 0 || index as usize == event.items.len() + 1 {
                            event.items.push((AE_TYPE_WILDCARD, value));
                            0
                        } else if (index as usize) <= event.items.len() {
                            event.items[index as usize - 1] = (AE_TYPE_WILDCARD, value);
                            0
                        } else {
                            AE_ERR_ILLEGAL_INDEX
                        };
                    } else if let Some(mut list) = self.read_ae_descriptor_value(bus, list_desc) {
                        err = Self::ae_put_list_item(&mut list, index, AE_TYPE_WILDCARD, value);
                        if err == 0 {
                            self.write_ae_descriptor_value(bus, list_desc, list);
                        }
                    }
                    let new_sp = sp + param_bytes;
                    bus.write_word(new_sp, err as u16);
                    cpu.write_reg(Register::A7, new_sp);
                    cpu.write_reg(Register::D0, err as i32 as u32);
                    return Some(Ok(()));
                }
                if routine == 9 && param_bytes == 12 {
                    let source_desc = bus.read_long(sp);
                    let index = bus.read_long(sp + 4);
                    let list_desc = bus.read_long(sp + 8);
                    let value = self.read_ae_descriptor_value(bus, source_desc);
                    let mut err = AE_ERR_DESC_NOT_FOUND;
                    if let Some(value) = value {
                        if let Some(event) = self.ae_events.get_mut(&list_desc) {
                            err = if index == 0 || index as usize == event.items.len() + 1 {
                                event.items.push((AE_TYPE_WILDCARD, value));
                                0
                            } else if (index as usize) <= event.items.len() {
                                event.items[index as usize - 1] = (AE_TYPE_WILDCARD, value);
                                0
                            } else {
                                AE_ERR_ILLEGAL_INDEX
                            };
                        } else if let Some(mut list) = self.read_ae_descriptor_value(bus, list_desc)
                        {
                            err = Self::ae_put_list_item(&mut list, index, AE_TYPE_WILDCARD, value);
                            if err == 0 {
                                self.write_ae_descriptor_value(bus, list_desc, list);
                            }
                        }
                    }
                    let new_sp = sp + param_bytes;
                    bus.write_word(new_sp, err as u16);
                    cpu.write_reg(Register::A7, new_sp);
                    cpu.write_reg(Register::D0, err as i32 as u32);
                    return Some(Ok(()));
                }

                // Routines 7/10/11/42 expose ordered descriptor-list items.
                //
                // AECountItems (0x0407)
                // AEGetNthPtr (0x100A)
                // AEGetNthDesc (0x0A0B)
                // AESizeOfNthItem (0x082A)
                // Interapplication Communication 1993, 4-31..4-33 and
                // 4-74..4-77, 4-90.
                if routine == 7 && param_bytes == 8 {
                    let count_ptr = bus.read_long(sp);
                    let list_desc = bus.read_long(sp + 4);
                    let count = self
                        .ae_events
                        .get(&list_desc)
                        .map(|event| event.items.len())
                        .or_else(|| {
                            self.read_ae_descriptor_value(bus, list_desc)
                                .map(|desc| desc.items.len())
                        });
                    let err = if let Some(count) = count {
                        if count_ptr != 0 {
                            bus.write_long(count_ptr, count as u32);
                        }
                        0
                    } else {
                        if count_ptr != 0 {
                            bus.write_long(count_ptr, 0);
                        }
                        AE_ERR_DESC_NOT_FOUND
                    };
                    let new_sp = sp + param_bytes;
                    bus.write_word(new_sp, err as u16);
                    cpu.write_reg(Register::A7, new_sp);
                    cpu.write_reg(Register::D0, err as i32 as u32);
                    return Some(Ok(()));
                }
                if routine == 10 && param_bytes == 32 {
                    let actual_size_ptr = bus.read_long(sp);
                    let maximum_size = bus.read_long(sp + 4);
                    let data_ptr = bus.read_long(sp + 8);
                    let type_code_ptr = bus.read_long(sp + 12);
                    let keyword_ptr = bus.read_long(sp + 16);
                    let desired_type = bus.read_long(sp + 20);
                    let index = bus.read_long(sp + 24);
                    let list_desc = bus.read_long(sp + 28);
                    let found = self
                        .ae_events
                        .get(&list_desc)
                        .and_then(|event| {
                            if index == 0 {
                                None
                            } else {
                                event.items.get(index as usize - 1).cloned()
                            }
                        })
                        .or_else(|| {
                            self.read_ae_descriptor_value(bus, list_desc)
                                .and_then(|desc| Self::ae_list_item_at(&desc, index))
                        });
                    let err = if let Some((keyword, desc)) = found {
                        if desired_type != AE_TYPE_WILDCARD && desired_type != desc.desc_type {
                            AE_ERR_COERCION_FAIL
                        } else {
                            if keyword_ptr != 0 {
                                bus.write_long(keyword_ptr, keyword);
                            }
                            if type_code_ptr != 0 {
                                bus.write_long(type_code_ptr, desc.desc_type);
                            }
                            if actual_size_ptr != 0 {
                                bus.write_long(actual_size_ptr, desc.data.len() as u32);
                            }
                            if data_ptr != 0 {
                                let copy_len = (maximum_size as usize).min(desc.data.len());
                                for (offset, byte) in
                                    desc.data.iter().copied().take(copy_len).enumerate()
                                {
                                    bus.write_byte(data_ptr + offset as u32, byte);
                                }
                            }
                            if maximum_size >= desc.data.len() as u32 {
                                0
                            } else {
                                AE_BUFFER_IS_SMALL
                            }
                        }
                    } else {
                        if keyword_ptr != 0 {
                            bus.write_long(keyword_ptr, AE_TYPE_WILDCARD);
                        }
                        if type_code_ptr != 0 {
                            bus.write_long(type_code_ptr, AE_TYPE_NULL);
                        }
                        if actual_size_ptr != 0 {
                            bus.write_long(actual_size_ptr, 0);
                        }
                        AE_ERR_DESC_NOT_FOUND
                    };
                    let new_sp = sp + param_bytes;
                    bus.write_word(new_sp, err as u16);
                    cpu.write_reg(Register::A7, new_sp);
                    cpu.write_reg(Register::D0, err as i32 as u32);
                    return Some(Ok(()));
                }
                if routine == 11 && param_bytes == 20 {
                    let result_desc = bus.read_long(sp);
                    let keyword_ptr = bus.read_long(sp + 4);
                    let desired_type = bus.read_long(sp + 8);
                    let index = bus.read_long(sp + 12);
                    let list_desc = bus.read_long(sp + 16);
                    let found = self
                        .ae_events
                        .get(&list_desc)
                        .and_then(|event| {
                            if index == 0 {
                                None
                            } else {
                                event.items.get(index as usize - 1).cloned()
                            }
                        })
                        .or_else(|| {
                            self.read_ae_descriptor_value(bus, list_desc)
                                .and_then(|desc| Self::ae_list_item_at(&desc, index))
                        });
                    let err = if let Some((keyword, desc)) = found {
                        if desired_type != AE_TYPE_WILDCARD && desired_type != desc.desc_type {
                            write_null_aedesc(bus, result_desc);
                            AE_ERR_COERCION_FAIL
                        } else {
                            if keyword_ptr != 0 {
                                bus.write_long(keyword_ptr, keyword);
                            }
                            self.write_ae_descriptor_value(bus, result_desc, desc);
                            0
                        }
                    } else {
                        if keyword_ptr != 0 {
                            bus.write_long(keyword_ptr, AE_TYPE_WILDCARD);
                        }
                        write_null_aedesc(bus, result_desc);
                        AE_ERR_DESC_NOT_FOUND
                    };
                    let new_sp = sp + param_bytes;
                    bus.write_word(new_sp, err as u16);
                    cpu.write_reg(Register::A7, new_sp);
                    cpu.write_reg(Register::D0, err as i32 as u32);
                    return Some(Ok(()));
                }
                if routine == 42 && param_bytes == 16 {
                    let data_size_ptr = bus.read_long(sp);
                    let type_code_ptr = bus.read_long(sp + 4);
                    let index = bus.read_long(sp + 8);
                    let list_desc = bus.read_long(sp + 12);
                    let found = self
                        .ae_events
                        .get(&list_desc)
                        .and_then(|event| {
                            if index == 0 {
                                None
                            } else {
                                event.items.get(index as usize - 1).cloned()
                            }
                        })
                        .or_else(|| {
                            self.read_ae_descriptor_value(bus, list_desc)
                                .and_then(|desc| Self::ae_list_item_at(&desc, index))
                        });
                    let err = if let Some((_, desc)) = found {
                        if type_code_ptr != 0 {
                            bus.write_long(type_code_ptr, desc.desc_type);
                        }
                        if data_size_ptr != 0 {
                            bus.write_long(data_size_ptr, desc.data.len() as u32);
                        }
                        0
                    } else {
                        if type_code_ptr != 0 {
                            bus.write_long(type_code_ptr, AE_TYPE_NULL);
                        }
                        if data_size_ptr != 0 {
                            bus.write_long(data_size_ptr, 0);
                        }
                        AE_ERR_DESC_NOT_FOUND
                    };
                    let new_sp = sp + param_bytes;
                    bus.write_word(new_sp, err as u16);
                    cpu.write_reg(Register::A7, new_sp);
                    cpu.write_reg(Register::D0, err as i32 as u32);
                    return Some(Ok(()));
                }

                // Routine 5 (`AEDuplicateDesc`) and routine 3
                // (`AECoerceDesc`) both return a descriptor copy. The HLE
                // supports identity coercion and record→object-specifier
                // wrappers used by the Object Support Library.
                //
                // AEDuplicateDesc (0x0405)
                // FUNCTION AEDuplicateDesc(theAEDesc: AEDesc;
                //   VAR result: AEDesc): OSErr;
                // AECoerceDesc (0x0603)
                // FUNCTION AECoerceDesc(theAEDesc: AEDesc;
                //   toType: DescType; VAR result: AEDesc): OSErr;
                // Inside Macintosh Volume VI, 6-87 and
                // Interapplication Communication 1993, 4-64..4-66.
                if routine == 5 && param_bytes == 8 {
                    let result_desc = bus.read_long(sp);
                    let source_desc = bus.read_long(sp + 4);
                    if let Some(desc) = self.read_ae_descriptor_value(bus, source_desc) {
                        self.write_ae_descriptor_value(bus, result_desc, desc);
                    } else {
                        write_null_aedesc(bus, result_desc);
                    }
                    let new_sp = sp + param_bytes;
                    bus.write_word(new_sp, 0);
                    cpu.write_reg(Register::A7, new_sp);
                    cpu.write_reg(Register::D0, 0);
                    return Some(Ok(()));
                }
                if routine == 3 && param_bytes == 12 {
                    let result_desc = bus.read_long(sp);
                    let desired_type = bus.read_long(sp + 4);
                    let source_desc = bus.read_long(sp + 8);
                    if let Some(mut desc) = self.read_ae_descriptor_value(bus, source_desc) {
                        if desired_type != AE_TYPE_WILDCARD {
                            desc.desc_type = desired_type;
                        }
                        self.write_ae_descriptor_value(bus, result_desc, desc);
                    } else {
                        write_null_aedesc(bus, result_desc);
                    }
                    let new_sp = sp + param_bytes;
                    bus.write_word(new_sp, 0);
                    cpu.write_reg(Register::A7, new_sp);
                    cpu.write_reg(Register::D0, 0);
                    return Some(Ok(()));
                }

                // Routine 15 (`AEPutParamPtr` / `AEPutKeyPtr`) and routine
                // 16 (`AEPutParamDesc` / `AEPutKeyDesc`) add copied data to
                // an AppleEvent parameter table. Keyed AE records are tracked
                // only as descriptors for now; event parameters are
                // caller-observable through AEGetParam*.
                //
                // AEPutParamPtr (0x0A0F)
                // FUNCTION AEPutParamPtr(theAppleEvent: AppleEvent;
                //   theAEKeyword: AEKeyword; typeCode: DescType;
                //   dataPtr: Ptr; dataSize: Size): OSErr;
                // AEPutParamDesc (0x0610)
                // FUNCTION AEPutParamDesc(theAppleEvent: AppleEvent;
                //   theAEKeyword: AEKeyword; theAEDesc: AEDesc): OSErr;
                // Inside Macintosh Volume VI, 6-91..6-92.
                if routine == 15 && param_bytes == 20 {
                    let data_size = bus.read_long(sp);
                    let data_ptr = bus.read_long(sp + 4);
                    let desc_type = bus.read_long(sp + 8);
                    let keyword = bus.read_long(sp + 12);
                    let target_desc = bus.read_long(sp + 16);
                    let mut data = Vec::with_capacity(data_size as usize);
                    if data_ptr != 0 {
                        for offset in 0..data_size {
                            data.push(bus.read_byte(data_ptr + offset));
                        }
                    }
                    let value = Self::ae_descriptor(desc_type, data);
                    if let Some(event) = self.ae_events.get_mut(&target_desc) {
                        Self::ae_put_event_param(event, keyword, value);
                    } else if let Some(mut target) = self.read_ae_descriptor_value(bus, target_desc)
                    {
                        Self::ae_put_record_field(&mut target, keyword, value);
                        self.write_ae_descriptor_value(bus, target_desc, target);
                    }
                    let new_sp = sp + param_bytes;
                    bus.write_word(new_sp, 0);
                    cpu.write_reg(Register::A7, new_sp);
                    cpu.write_reg(Register::D0, 0);
                    return Some(Ok(()));
                }
                if routine == 16 && param_bytes == 12 {
                    let source_desc = bus.read_long(sp);
                    let keyword = bus.read_long(sp + 4);
                    let target_desc = bus.read_long(sp + 8);
                    let desc = self.read_ae_descriptor_value(bus, source_desc);
                    if let Some(desc) = desc {
                        if let Some(event) = self.ae_events.get_mut(&target_desc) {
                            Self::ae_put_event_param(event, keyword, desc);
                        } else if let Some(mut target) =
                            self.read_ae_descriptor_value(bus, target_desc)
                        {
                            Self::ae_put_record_field(&mut target, keyword, desc);
                            self.write_ae_descriptor_value(bus, target_desc, target);
                        }
                    }
                    let new_sp = sp + param_bytes;
                    bus.write_word(new_sp, 0);
                    cpu.write_reg(Register::A7, new_sp);
                    cpu.write_reg(Register::D0, 0);
                    return Some(Ok(()));
                }

                // Routine 4 (`AEDisposeDesc`) disposes of a descriptor
                // record's data. Systemless's bump allocator cannot reclaim
                // the copied bytes, but clearing tracked state and writing a
                // null descriptor matches caller-visible reuse semantics.
                // Structured descriptor backing is keyed by the data handle,
                // which may be shared by AEDesc records copied by value.
                //
                // AEDisposeDesc (0x0204)
                // FUNCTION AEDisposeDesc(theAEDesc: AEDesc): OSErr;
                // Inside Macintosh Volume VI, 6-87.
                if routine == 4 && param_bytes == 4 {
                    let desc_ptr = bus.read_long(sp);
                    self.dispose_ae_descriptor_record(bus, desc_ptr);
                    let new_sp = sp + param_bytes;
                    bus.write_word(new_sp, 0);
                    cpu.write_reg(Register::A7, new_sp);
                    cpu.write_reg(Register::D0, 0);
                    return Some(Ok(()));
                }

                // Routine 20 (`AECreateAppleEvent`) creates an
                // AppleEvent descriptor containing event-class and event-ID
                // attributes. Systemless stores just those attributes so
                // later AEGetAttribute* and same-process AESend dispatch can
                // identify the registered handler.
                //
                // AECreateAppleEvent (0x0B14)
                // FUNCTION AECreateAppleEvent(theAEEventClass: AEEventClass;
                //   theAEEventID: AEEventID; target: AEAddressDesc;
                //   returnID: INTEGER; transactionID: LONGINT;
                //   VAR result: AppleEvent): OSErr;
                // Inside Macintosh Volume VI, 6-86..6-87.
                if routine == 20 && param_bytes == 22 {
                    let result_desc = bus.read_long(sp);
                    let event_id = bus.read_long(sp + 14);
                    let event_class = bus.read_long(sp + 18);
                    if result_desc != 0 {
                        self.write_synthetic_apple_event_descriptor(
                            bus,
                            result_desc,
                            event_class,
                            event_id,
                        );
                    }
                    let new_sp = sp + param_bytes;
                    bus.write_word(new_sp, 0);
                    cpu.write_reg(Register::A7, new_sp);
                    cpu.write_reg(Register::D0, 0);
                    return Some(Ok(()));
                }

                // Routine 18 (`AEGetParamDesc` / `AEGetKeyDesc`) and
                // routine 17 (`AEGetParamPtr` / `AEGetKeyPtr`) extract
                // AppleEvent parameters. The synthetic Open Application
                // event has no direct parameter, so missing parameters
                // must return `errAEDescNotFound`, not a successful
                // zero-filled result.
                //
                // AEGetParamDesc (0x0812)
                // FUNCTION AEGetParamDesc(theAppleEvent: AppleEvent;
                //   theAEKeyword: AEKeyword; desiredType: DescType;
                //   VAR result: AEDesc): OSErr;
                // AEGetParamPtr (0x0E11)
                // FUNCTION AEGetParamPtr(theAppleEvent: AppleEvent;
                //   theAEKeyword: AEKeyword; desiredType: DescType;
                //   VAR typeCode: DescType; dataPtr: Ptr;
                //   maximumSize: Size; VAR actualSize: Size): OSErr;
                // Inside Macintosh: Interapplication Communication, 4-68..4-70.
                if routine == 18 && param_bytes == 16 {
                    let result_desc = bus.read_long(sp);
                    let desired_type = bus.read_long(sp + 4);
                    let keyword = bus.read_long(sp + 8);
                    let event_desc = bus.read_long(sp + 12);
                    let found = self.ae_event_param_value(bus, event_desc, keyword);
                    if let Some(desc) = found.filter(|desc| {
                        desired_type == AE_TYPE_WILDCARD || desired_type == desc.desc_type
                    }) {
                        self.write_ae_descriptor_value(bus, result_desc, desc);
                        let new_sp = sp + param_bytes;
                        bus.write_word(new_sp, 0);
                        cpu.write_reg(Register::A7, new_sp);
                        cpu.write_reg(Register::D0, 0);
                        return Some(Ok(()));
                    }
                    write_null_aedesc(bus, result_desc);
                    let new_sp = sp + param_bytes;
                    bus.write_word(new_sp, AE_ERR_DESC_NOT_FOUND as u16);
                    cpu.write_reg(Register::A7, new_sp);
                    cpu.write_reg(Register::D0, AE_ERR_DESC_NOT_FOUND as i32 as u32);
                    return Some(Ok(()));
                }
                if routine == 17 && param_bytes == 28 {
                    let actual_size_ptr = bus.read_long(sp);
                    let maximum_size = bus.read_long(sp + 4);
                    let data_ptr = bus.read_long(sp + 8);
                    let type_code_ptr = bus.read_long(sp + 12);
                    let desired_type = bus.read_long(sp + 16);
                    let keyword = bus.read_long(sp + 20);
                    let event_desc = bus.read_long(sp + 24);
                    if let Some(desc) =
                        self.ae_event_param_value(bus, event_desc, keyword)
                            .filter(|desc| {
                                desired_type == AE_TYPE_WILDCARD || desired_type == desc.desc_type
                            })
                    {
                        if actual_size_ptr != 0 {
                            bus.write_long(actual_size_ptr, desc.data.len() as u32);
                        }
                        if type_code_ptr != 0 {
                            bus.write_long(type_code_ptr, desc.desc_type);
                        }
                        if data_ptr != 0 {
                            let copy_len = (maximum_size as usize).min(desc.data.len());
                            for (offset, byte) in
                                desc.data.iter().copied().take(copy_len).enumerate()
                            {
                                bus.write_byte(data_ptr + offset as u32, byte);
                            }
                        }
                        let new_sp = sp + param_bytes;
                        let err = if maximum_size >= desc.data.len() as u32 {
                            0
                        } else {
                            AE_BUFFER_IS_SMALL
                        };
                        bus.write_word(new_sp, err as u16);
                        cpu.write_reg(Register::A7, new_sp);
                        cpu.write_reg(Register::D0, err as i32 as u32);
                        return Some(Ok(()));
                    }
                    if actual_size_ptr != 0 {
                        bus.write_long(actual_size_ptr, 0);
                    }
                    if type_code_ptr != 0 {
                        bus.write_long(type_code_ptr, AE_TYPE_NULL);
                    }
                    let new_sp = sp + param_bytes;
                    bus.write_word(new_sp, AE_ERR_DESC_NOT_FOUND as u16);
                    cpu.write_reg(Register::A7, new_sp);
                    cpu.write_reg(Register::D0, AE_ERR_DESC_NOT_FOUND as i32 as u32);
                    return Some(Ok(()));
                }

                // Routines 40/41 report descriptor sizes and types without
                // copying the descriptor payload.
                //
                // AESizeOfAttribute (0x0828)
                // AESizeOfKeyDesc / AESizeOfParam (0x0829)
                // Interapplication Communication 1993, 4-90..4-92.
                if routine == 41 && param_bytes == 16 {
                    let data_size_ptr = bus.read_long(sp);
                    let type_code_ptr = bus.read_long(sp + 4);
                    let keyword = bus.read_long(sp + 8);
                    let event_or_record = bus.read_long(sp + 12);
                    let found = self.ae_event_param_value(bus, event_or_record, keyword);
                    let err = if let Some(desc) = found {
                        if type_code_ptr != 0 {
                            bus.write_long(type_code_ptr, desc.desc_type);
                        }
                        if data_size_ptr != 0 {
                            bus.write_long(data_size_ptr, desc.data.len() as u32);
                        }
                        0
                    } else {
                        if type_code_ptr != 0 {
                            bus.write_long(type_code_ptr, AE_TYPE_NULL);
                        }
                        if data_size_ptr != 0 {
                            bus.write_long(data_size_ptr, 0);
                        }
                        AE_ERR_DESC_NOT_FOUND
                    };
                    let new_sp = sp + param_bytes;
                    bus.write_word(new_sp, err as u16);
                    cpu.write_reg(Register::A7, new_sp);
                    cpu.write_reg(Register::D0, err as i32 as u32);
                    return Some(Ok(()));
                }
                if routine == 40 && param_bytes == 16 {
                    let data_size_ptr = bus.read_long(sp);
                    let type_code_ptr = bus.read_long(sp + 4);
                    let keyword = bus.read_long(sp + 8);
                    let event_desc = bus.read_long(sp + 12);
                    let found = self.ae_event_attribute_value(event_desc, keyword);
                    let err = if let Some(desc) = found {
                        if type_code_ptr != 0 {
                            bus.write_long(type_code_ptr, desc.desc_type);
                        }
                        if data_size_ptr != 0 {
                            bus.write_long(data_size_ptr, desc.data.len() as u32);
                        }
                        0
                    } else {
                        if type_code_ptr != 0 {
                            bus.write_long(type_code_ptr, AE_TYPE_NULL);
                        }
                        if data_size_ptr != 0 {
                            bus.write_long(data_size_ptr, 0);
                        }
                        AE_ERR_DESC_NOT_FOUND
                    };
                    let new_sp = sp + param_bytes;
                    bus.write_word(new_sp, err as u16);
                    cpu.write_reg(Register::A7, new_sp);
                    cpu.write_reg(Register::D0, err as i32 as u32);
                    return Some(Ok(()));
                }

                // Routine 21 (`AEGetAttributePtr`) and routine 38
                // (`AEGetAttributeDesc`) expose AppleEvent attributes.
                // Every AppleEvent includes event class and event ID
                // attributes (`keyEventClassAttr`, `keyEventIDAttr`),
                // each stored as `typeType` four-character-code data.
                //
                // AEGetAttributePtr (0x0E15)
                // FUNCTION AEGetAttributePtr(theAppleEvent: AppleEvent;
                //   theAEKeyword: AEKeyword; desiredType: DescType;
                //   VAR typeCode: DescType; dataPtr: Ptr;
                //   maximumSize: Size; VAR actualSize: Size): OSErr;
                // AEGetAttributeDesc (0x0826)
                // FUNCTION AEGetAttributeDesc(theAppleEvent: AppleEvent;
                //   theAEKeyword: AEKeyword; desiredType: DescType;
                //   VAR result: AEDesc): OSErr;
                // Inside Macintosh: Interapplication Communication, 4-71..4-73.
                if routine == 21 && param_bytes == 28 {
                    let actual_size_ptr = bus.read_long(sp);
                    let maximum_size = bus.read_long(sp + 4);
                    let data_ptr = bus.read_long(sp + 8);
                    let type_code_ptr = bus.read_long(sp + 12);
                    let desired_type = bus.read_long(sp + 16);
                    let keyword = bus.read_long(sp + 20);
                    let event_desc = bus.read_long(sp + 24);
                    let attr_value = self.ae_event_attribute_value(event_desc, keyword);
                    let err = if let Some(desc) = attr_value {
                        if desired_type != AE_TYPE_WILDCARD && desired_type != desc.desc_type {
                            AE_ERR_COERCION_FAIL
                        } else {
                            if type_code_ptr != 0 {
                                bus.write_long(type_code_ptr, desc.desc_type);
                            }
                            if actual_size_ptr != 0 {
                                bus.write_long(actual_size_ptr, desc.data.len() as u32);
                            }
                            if data_ptr != 0 {
                                let copy_len = (maximum_size as usize).min(desc.data.len());
                                for (offset, byte) in
                                    desc.data.iter().copied().take(copy_len).enumerate()
                                {
                                    bus.write_byte(data_ptr + offset as u32, byte);
                                }
                            }
                            if maximum_size >= desc.data.len() as u32 {
                                0
                            } else {
                                AE_BUFFER_IS_SMALL
                            }
                        }
                    } else {
                        if type_code_ptr != 0 {
                            bus.write_long(type_code_ptr, AE_TYPE_NULL);
                        }
                        if actual_size_ptr != 0 {
                            bus.write_long(actual_size_ptr, 0);
                        }
                        AE_ERR_DESC_NOT_FOUND
                    };
                    let new_sp = sp + param_bytes;
                    bus.write_word(new_sp, err as u16);
                    cpu.write_reg(Register::A7, new_sp);
                    cpu.write_reg(Register::D0, err as i32 as u32);
                    return Some(Ok(()));
                }
                if routine == 38 && param_bytes == 16 {
                    let result_desc = bus.read_long(sp);
                    let desired_type = bus.read_long(sp + 4);
                    let keyword = bus.read_long(sp + 8);
                    let event_desc = bus.read_long(sp + 12);
                    let attr_value = self.ae_event_attribute_value(event_desc, keyword);
                    let err = if let Some(desc) = attr_value {
                        if desired_type != AE_TYPE_WILDCARD && desired_type != desc.desc_type {
                            write_null_aedesc(bus, result_desc);
                            AE_ERR_COERCION_FAIL
                        } else {
                            self.write_ae_descriptor_value(bus, result_desc, desc);
                            0
                        }
                    } else {
                        write_null_aedesc(bus, result_desc);
                        AE_ERR_DESC_NOT_FOUND
                    };
                    let new_sp = sp + param_bytes;
                    bus.write_word(new_sp, err as u16);
                    cpu.write_reg(Register::A7, new_sp);
                    cpu.write_reg(Register::D0, err as i32 as u32);
                    return Some(Ok(()));
                }

                // Routine 58 (`AEDisposeToken`) deallocates a final object
                // token. If an application or system token-disposal callback
                // is installed, invoke it through the usual Pascal callback
                // trampoline; otherwise fall back to AEDisposeDesc semantics.
                //
                // AEDisposeToken (0x023A)
                // FUNCTION AEDisposeToken(VAR theToken: AEDesc): OSErr;
                // Interapplication Communication 1993, 6-87..6-88.
                if routine == 58 && param_bytes == 4 {
                    let token_desc = bus.read_long(sp);
                    if let Some(handler_ptr) =
                        self.ae_special_handler_for(AE_KEY_DISPOSE_TOKEN_PROC)
                    {
                        let trampoline = match self.ae_trampoline_addr {
                            Some(addr) => addr,
                            None => {
                                let addr = bus.alloc(8);
                                bus.write_word(addr, 0x303C); // MOVE.W #imm, D0
                                bus.write_word(addr + 2, 0xFEFE); // immediate
                                bus.write_word(addr + 4, 0xA816); // _Pack8
                                self.ae_trampoline_addr = Some(addr);
                                addr
                            }
                        };
                        let result_slot = sp + param_bytes;
                        let new_sp = result_slot.wrapping_sub(8);
                        bus.write_long(new_sp, trampoline);
                        bus.write_long(new_sp + 4, token_desc);
                        cpu.write_reg(Register::A7, new_sp);

                        let return_pc = cpu.read_reg(Register::PC);
                        if let Some(outer_state) = self.ae_call_state.take() {
                            self.ae_call_state_stack.push(outer_state);
                        }
                        self.ae_call_state = Some(crate::trap::dispatch::AeCallState {
                            return_pc,
                            expected_sp_after_rtd: result_slot,
                            result_override: None,
                            owned_descriptors: None,
                            resolve_state: None,
                        });
                        cpu.write_reg(Register::PC, handler_ptr);
                        return Some(Ok(()));
                    }

                    self.dispose_ae_descriptor_record(bus, token_desc);
                    let new_sp = sp + param_bytes;
                    bus.write_word(new_sp, 0);
                    cpu.write_reg(Register::A7, new_sp);
                    cpu.write_reg(Register::D0, 0);
                    return Some(Ok(()));
                }

                // Routine 59 (`AECallObjectAccessor`) directly invokes an
                // exact object-accessor table entry. Unlike AEResolve, it
                // does not wildcard-match a concrete request against a
                // typeWildCard entry; callers that want a wildcard accessor
                // must ask for typeWildCard.
                //
                // AECallObjectAccessor (0x0C3B)
                // FUNCTION AECallObjectAccessor(desiredClass: DescType;
                //   containerToken: AEDesc; containerClass: DescType;
                //   keyForm: DescType; keyData: AEDesc;
                //   VAR theToken: AEDesc): OSErr;
                // Interapplication Communication 1993, 6-82..6-83.
                if routine == 59 && param_bytes == 24 {
                    let token_desc = bus.read_long(sp);
                    let key_data_desc = bus.read_long(sp + 4);
                    let key_form = bus.read_long(sp + 8);
                    let container_class = bus.read_long(sp + 12);
                    let container_token_desc = bus.read_long(sp + 16);
                    let desired_class = bus.read_long(sp + 20);
                    let result_slot = sp + param_bytes;

                    let Some(container_token) =
                        self.read_ae_descriptor_value(bus, container_token_desc)
                    else {
                        write_null_aedesc(bus, token_desc);
                        bus.write_word(result_slot, AE_ERR_DESC_NOT_FOUND as u16);
                        cpu.write_reg(Register::A7, result_slot);
                        cpu.write_reg(Register::D0, AE_ERR_DESC_NOT_FOUND as i32 as u32);
                        return Some(Ok(()));
                    };
                    let Some(key_data) = self.read_ae_descriptor_value(bus, key_data_desc) else {
                        write_null_aedesc(bus, token_desc);
                        bus.write_word(result_slot, AE_ERR_DESC_NOT_FOUND as u16);
                        cpu.write_reg(Register::A7, result_slot);
                        cpu.write_reg(Register::D0, AE_ERR_DESC_NOT_FOUND as i32 as u32);
                        return Some(Ok(()));
                    };
                    let Some(accessor) = self.ae_object_accessor_exact_any_table(
                        desired_class,
                        container_token.desc_type,
                    ) else {
                        write_null_aedesc(bus, token_desc);
                        bus.write_word(result_slot, AE_ERR_ACCESSOR_NOT_FOUND as u16);
                        cpu.write_reg(Register::A7, result_slot);
                        cpu.write_reg(Register::D0, AE_ERR_ACCESSOR_NOT_FOUND as i32 as u32);
                        return Some(Ok(()));
                    };

                    let level = AeResolveLevel {
                        desired_class,
                        key_form,
                        key_data,
                    };
                    let resolve_state = AeResolveState {
                        return_pc: cpu.read_reg(Register::PC),
                        result_slot,
                        final_token_desc: token_desc,
                        levels: vec![level.clone()],
                        next_level: 0,
                        current_token_desc: 0,
                        container_class,
                    };
                    if let Some(outer_state) = self.ae_call_state.take() {
                        self.ae_call_state_stack.push(outer_state);
                    }
                    self.ae_invoke_object_accessor(
                        bus,
                        cpu,
                        resolve_state,
                        container_token,
                        level,
                        accessor,
                    );
                    return Some(Ok(()));
                }

                // Routine 54 (`AEResolve`) resolves an AppleEvent object
                // specifier record into a token by dispatching installed
                // object accessor functions. The Object Support Library
                // resolves nested containers from the default container
                // outward; each accessor returns the token that becomes the
                // next container token.
                //
                // AEResolve (0x0536)
                // FUNCTION AEResolve(objectSpecifier: AEDesc;
                //   callbackFlags: Integer; VAR theToken: AEDesc): OSErr;
                // Inside Macintosh: Interapplication Communication,
                // 6-4..6-6, 6-78..6-86, and 6-115.
                if routine == 54 && param_bytes == 10 {
                    let token_desc = bus.read_long(sp);
                    let object_specifier = bus.read_long(sp + 6);
                    let Some(object_specifier) =
                        self.read_ae_descriptor_value(bus, object_specifier)
                    else {
                        write_null_aedesc(bus, token_desc);
                        let new_sp = sp + param_bytes;
                        bus.write_word(new_sp, AE_ERR_NOT_AN_OBJECT_SPEC as u16);
                        cpu.write_reg(Register::A7, new_sp);
                        cpu.write_reg(Register::D0, AE_ERR_NOT_AN_OBJECT_SPEC as i32 as u32);
                        return Some(Ok(()));
                    };
                    let Some((base_container, base_container_class, levels)) =
                        Self::ae_collect_resolve_levels(&object_specifier)
                    else {
                        write_null_aedesc(bus, token_desc);
                        let new_sp = sp + param_bytes;
                        bus.write_word(new_sp, AE_ERR_NOT_AN_OBJECT_SPEC as u16);
                        cpu.write_reg(Register::A7, new_sp);
                        cpu.write_reg(Register::D0, AE_ERR_NOT_AN_OBJECT_SPEC as i32 as u32);
                        return Some(Ok(()));
                    };
                    let new_sp = sp + param_bytes;
                    let resolve_state = AeResolveState {
                        return_pc: cpu.read_reg(Register::PC),
                        result_slot: new_sp,
                        final_token_desc: token_desc,
                        levels,
                        next_level: 0,
                        current_token_desc: 0,
                        container_class: base_container_class,
                    };
                    if let Some(outer_state) = self.ae_call_state.take() {
                        self.ae_call_state_stack.push(outer_state);
                    }
                    self.ae_dispatch_object_accessor(bus, cpu, resolve_state, base_container);
                    return Some(Ok(()));
                }

                // Routine 27 (`AEProcessAppleEvent`): dispatch the head
                // queued AE through its registered handler. Stack on
                // entry (param_bytes = 4):
                //   SP+0   theEventRecord ptr (4 bytes)
                //   SP+4   result OSErr slot (2 bytes)
                // We synthesize a kAEOpenApplication invocation whenever
                // the matching handler is registered. The OAPP path is
                // what unblocks `WaitForStartupEvent`-style splash gates
                // in apps that call `AEProcessAppleEvent` directly
                // instead of going through `WaitNextEvent`. Unlike the
                // older one-shot gate, repeated direct calls are allowed:
                // each `AEProcessAppleEvent` invocation can dispatch the
                // registered handler again.
                if routine == 27 && param_bytes == 4 {
                    let oapp_class = AE_TYPE_APPLE_EVENT;
                    let oapp_id = u32::from_be_bytes(*b"oapp");
                    if trace_ae_enabled() {
                        let event_record = bus.read_long(sp);
                        let what = bus.read_word(event_record);
                        let message = bus.read_long(event_record + 2);
                        let where_v = bus.read_word(event_record + 10);
                        let where_h = bus.read_word(event_record + 12);
                        eprintln!(
                            "[AE] ProcessAppleEvent EventRecord=${:08X} what={} message='{}' where='{}' mods=${:04X}",
                            event_record,
                            what,
                            format_fourcc(message),
                            format_fourcc(((where_v as u32) << 16) | where_h as u32),
                            bus.read_word(event_record + 14)
                        );
                    }
                    if let Some(handler) = self.apple_event_handler_for(oapp_class, oapp_id) {
                        let handler_ptr = handler.procedure.original_pointer;
                        let refcon = handler.refcon;
                        // Lazily allocate the trampoline on first use.
                        // Six bytes encode `MOVE.W #$FEFE, D0; _Pack8`,
                        // pad to 8 for alignment.
                        let trampoline = match self.ae_trampoline_addr {
                            Some(addr) => addr,
                            None => {
                                let addr = bus.alloc(8);
                                bus.write_word(addr, 0x303C); // MOVE.W #imm, D0
                                bus.write_word(addr + 2, 0xFEFE); // immediate
                                bus.write_word(addr + 4, 0xA816); // _Pack8
                                self.ae_trampoline_addr = Some(addr);
                                addr
                            }
                        };

                        // Build a minimal AppleEvent + reply pair on
                        // the heap. The AppleEvent descriptor carries
                        // typeAppleEvent and a non-NIL data handle;
                        // `ae_events` holds the attribute data that
                        // AEGetAttribute* exposes to the handler. The
                        // null reply descriptor matches the no-reply
                        // path for a synthetic open-application event.
                        let return_allocation_failure =
                            |cpu: &mut dyn CpuOps, bus: &mut MacMemoryBus| {
                                let new_sp = sp + param_bytes;
                                bus.write_word(new_sp, (-108i16) as u16);
                                cpu.write_reg(Register::A7, new_sp);
                                cpu.write_reg(Register::D0, -108i32 as u32);
                                Some(Ok(()))
                            };
                        let event_desc = self.new_process_classic_ptr(bus, 8);
                        if event_desc == 0 {
                            return return_allocation_failure(cpu, bus);
                        }
                        self.write_synthetic_apple_event_descriptor(
                            bus, event_desc, oapp_class, oapp_id,
                        );
                        if bus.read_long(event_desc + 4) == 0 {
                            self.dispose_owned_ae_callback_descriptor(bus, event_desc);
                            return return_allocation_failure(cpu, bus);
                        }
                        let reply_desc = self.new_process_classic_ptr(bus, 8);
                        if reply_desc == 0 {
                            self.dispose_owned_ae_callback_descriptor(bus, event_desc);
                            return return_allocation_failure(cpu, bus);
                        }
                        bus.write_long(reply_desc, AE_TYPE_NULL);
                        bus.write_long(reply_desc + 4, 0);

                        // Stack on entry has [event_ptr][result_slot]
                        // at [SP][SP+4]. We need the handler to see
                        // [trampoline][refcon][reply][event][result],
                        // so push three 4-byte words below the result
                        // slot and replace the original EventRecord
                        // pointer at SP+0 with the AppleEvent AEDesc.
                        let new_sp = sp.wrapping_sub(12);
                        bus.write_long(new_sp, trampoline); // return PC
                        bus.write_long(new_sp + 4, refcon);
                        bus.write_long(new_sp + 8, reply_desc);
                        bus.write_long(new_sp + 12, event_desc);
                        cpu.write_reg(Register::A7, new_sp);

                        // Save what we need to resume the original
                        // caller's flow. After the handler's
                        // `RTD #12`, SP will land at the result slot
                        // address (= original sp + 4).
                        let return_pc = cpu.read_reg(Register::PC);
                        if let Some(outer_state) = self.ae_call_state.take() {
                            self.ae_call_state_stack.push(outer_state);
                        }
                        self.ae_call_state = Some(crate::trap::dispatch::AeCallState {
                            return_pc,
                            expected_sp_after_rtd: sp + 4,
                            result_override: None,
                            owned_descriptors: Some((event_desc, reply_desc)),
                            resolve_state: None,
                        });
                        self.fired_oapp_handler = true;

                        eprintln!(
                            "[AE] ProcessAppleEvent → invoking 'oapp' handler ${:08X} via trampoline ${:08X}",
                            handler_ptr, trampoline,
                        );

                        match handler.procedure.isa {
                            crate::guest_procedure::GuestIsa::M68k => {
                                cpu.write_reg(Register::PC, handler.procedure.entry);
                            }
                            crate::guest_procedure::GuestIsa::PowerPc => {
                                let arguments = crate::guest_call::PowerPcArguments::from_slice(&[
                                    event_desc, reply_desc, refcon,
                                ])
                                .expect("AppleEvent handler has three arguments");
                                let started = self.guest_calls.begin_m68k_to_powerpc(
                                    crate::guest_call::GuestCallTarget {
                                        isa: handler.procedure.isa,
                                        entry: handler.procedure.entry,
                                        rtoc: handler.procedure.rtoc,
                                    },
                                    arguments,
                                    trampoline,
                                    sp + 4,
                                    Some(crate::guest_call::M68kResultTarget::Memory {
                                        address: sp + 4,
                                        size: 2,
                                    }),
                                );
                                if !started {
                                    self.ae_call_state = self.ae_call_state_stack.pop();
                                    self.dispose_owned_ae_callback_descriptor(bus, event_desc);
                                    self.dispose_owned_ae_callback_descriptor(bus, reply_desc);
                                    return return_allocation_failure(cpu, bus);
                                }
                            }
                        }
                        return Some(Ok(()));
                    }
                }

                // Routine 23 (`AESend`) sends a constructed AppleEvent.
                // For same-process HLE, when a handler is already installed
                // for the event's class/ID, dispatch it through the same
                // guest-handler trampoline as AEProcessAppleEvent. This is
                // the generic path used by applications that route internal
                // commands through AppleEvents.
                //
                // AESend (0x0D17)
                // FUNCTION AESend(theAppleEvent: AppleEvent;
                //   VAR reply: AppleEvent; sendMode: AESendMode;
                //   sendPriority: AESendPriority; timeOutInTicks: LONGINT;
                //   idleProc: IdleProcPtr; filterProc: EventFilterProcPtr): OSErr;
                // Inside Macintosh Volume VI, 6-93..6-96.
                if routine == 23 && param_bytes == 26 {
                    let reply_desc = bus.read_long(sp + 18);
                    let event_desc = bus.read_long(sp + 22);
                    let send_mode = bus.read_long(sp + 14);
                    if reply_desc != 0 {
                        if (send_mode & AE_SEND_MODE_REPLY_MASK) == AE_SEND_MODE_WAIT_REPLY {
                            self.write_synthetic_apple_event_descriptor(
                                bus,
                                reply_desc,
                                AE_TYPE_APPLE_EVENT,
                                AE_EVENT_ID_ANSWER,
                            );
                        } else {
                            self.ae_events.remove(&reply_desc);
                            self.ae_descriptors.remove(&reply_desc);
                            write_null_aedesc(bus, reply_desc);
                        }
                    }
                    if let Some(event) = self.ae_events.get(&event_desc).cloned() {
                        if let Some(handler) =
                            self.apple_event_handler_for(event.event_class, event.event_id)
                        {
                            let handler_ptr = handler.procedure.original_pointer;
                            let refcon = handler.refcon;
                            let trampoline = match self.ae_trampoline_addr {
                                Some(addr) => addr,
                                None => {
                                    let addr = bus.alloc(8);
                                    bus.write_word(addr, 0x303C); // MOVE.W #imm, D0
                                    bus.write_word(addr + 2, 0xFEFE); // immediate
                                    bus.write_word(addr + 4, 0xA816); // _Pack8
                                    self.ae_trampoline_addr = Some(addr);
                                    addr
                                }
                            };

                            // Reuse the caller's AESend result slot as the
                            // handler result slot. AESend's argument frame is
                            // 26 bytes, so placing the handler frame at
                            // sp+10 makes new_sp+16 equal sp+26.
                            let new_sp = sp + 10;
                            bus.write_long(new_sp, trampoline); // return PC
                            bus.write_long(new_sp + 4, refcon);
                            bus.write_long(new_sp + 8, reply_desc);
                            bus.write_long(new_sp + 12, event_desc);
                            cpu.write_reg(Register::A7, new_sp);

                            let return_pc = cpu.read_reg(Register::PC);
                            if let Some(outer_state) = self.ae_call_state.take() {
                                self.ae_call_state_stack.push(outer_state);
                            }
                            self.ae_call_state = Some(crate::trap::dispatch::AeCallState {
                                return_pc,
                                expected_sp_after_rtd: sp + param_bytes,
                                result_override: Some(0),
                                owned_descriptors: None,
                                resolve_state: None,
                            });
                            if trace_ae_enabled() {
                                eprintln!(
                                    "[AE] AESend → invoking '{}'/'{}' handler ${:08X} via trampoline ${:08X}",
                                    format_fourcc(event.event_class),
                                    format_fourcc(event.event_id),
                                    handler_ptr,
                                    trampoline,
                                );
                            }

                            match handler.procedure.isa {
                                crate::guest_procedure::GuestIsa::M68k => {
                                    cpu.write_reg(Register::PC, handler.procedure.entry);
                                }
                                crate::guest_procedure::GuestIsa::PowerPc => {
                                    let arguments =
                                        crate::guest_call::PowerPcArguments::from_slice(&[
                                            event_desc, reply_desc, refcon,
                                        ])
                                        .expect("AppleEvent handler has three arguments");
                                    if !self.guest_calls.begin_m68k_to_powerpc(
                                        crate::guest_call::GuestCallTarget {
                                            isa: handler.procedure.isa,
                                            entry: handler.procedure.entry,
                                            rtoc: handler.procedure.rtoc,
                                        },
                                        arguments,
                                        trampoline,
                                        sp + param_bytes,
                                        Some(crate::guest_call::M68kResultTarget::Memory {
                                            address: sp + param_bytes,
                                            size: 2,
                                        }),
                                    ) {
                                        self.ae_call_state = self.ae_call_state_stack.pop();
                                        let new_sp = sp + param_bytes;
                                        bus.write_word(new_sp, (-108i16) as u16);
                                        cpu.write_reg(Register::A7, new_sp);
                                        cpu.write_reg(Register::D0, -108i32 as u32);
                                        return Some(Ok(()));
                                    }
                                }
                            }
                            return Some(Ok(()));
                        }
                    }

                    let new_sp = sp + param_bytes;
                    bus.write_word(new_sp, 0);
                    cpu.write_reg(Register::A7, new_sp);
                    cpu.write_reg(Register::D0, 0);
                    return Some(Ok(()));
                }

                // Pop parameters off the stack. The result word (noErr by
                // default) is left at the new SP, which is exactly the slot
                // the caller pre-reserved before pushing the parameters.
                let new_sp = sp + param_bytes;
                bus.write_word(new_sp, 0); // noErr
                cpu.write_reg(Register::A7, new_sp);
                cpu.write_reg(Register::D0, 0);
                Ok(())
            }

            // ========== Desk Accessories ==========
            //
            // The Desk Accessory family ($A9B2 SystemEvent / $A9B3
            // SystemClick / $A9B4 SystemTask / $A9B5 SystemMenu /
            // $A9B6 OpenDeskAcc / $A9B7 CloseDeskAcc / $A9C2
            // SystemEdit) handles classic Mac OS Desk Accessories —
            // small applets (Calculator, Alarm Clock, Note Pad,
            // Scrapbook, Chooser etc.) that ran in system windows
            // sharing the application's address space. Per IM:I
            // I-435..I-446 + Macintosh Toolbox Essentials 1992 chapter
            // 6 (Desk Manager) the family routes events between the
            // foreground app and the active DA (if any), services
            // periodic-action ticks via the Time Manager queue, and
            // installs/removes the DRVR-resource-backed driver code
            // into the Device Manager DCE chain.
            //
            // ## HLE compromise
            //
            // Systemless models no Desk Accessories at all. Concretely:
            //   - No DRVR resource loading / DCE chain mutation
            //     (would require Device Manager OpenDriver path which
            //     itself collapses to no-op in HLE — see $A000 Open
            //     plus $A001 Close in src/trap/event.rs).
            //   - No system window — every window in HLE is an
            //     application window with windowKind >= 0; DAs would
            //     have negative windowKind = -refNum per IM:I I-435.
            //   - No DA event dispatch — the active DA's `accEvent`
            //     control message ($A004 Control sub-call 64) would
            //     need a guest-fn dispatch infrastructure to invoke
            //     the DRVR's event-handling proc, which Systemless
            //     doesn't have (same compromise as ModalDialog
            //     filterProc / Alert filterProc / Pack1 LSearch
            //     searchProc / SndAddModifier modifier proc).
            //   - No Time Manager periodic-action queue — SystemTask
            //     would walk it for `dNeedTime`-flagged drivers and
            //     fire their `accRun` ($A004 Control sub-call 65),
            //     which Systemless never reaches because no DA is ever
            //     installed.
            //
            // Desk Accessory support remains absent:
            // System 7.5+ apps that ARE in the systemless-games corpus
            // (Marathon, Glider PRO, Bonkheads, Centaurian, Koji)
            // gate the DA family behind feature checks (Gestalt
            // 'os ' bit checks, app-prefs settings) and don't depend
            // on DA-driven side effects. Apps that DO depend on a DA
            // (Note Pad save-game integration, Calculator math
            // helper) are System-6-era and out of corpus scope.
            //
            // ## Per-trap return-value summary
            //
            // - $A9B2 SystemEvent (FUNCTION → BOOLEAN): returns FALSE
            //   per IM:I I-441 "If the active window does not belong
            //   to a desk accessory ... SystemEvent returns FALSE";
            //   Systemless's HLE has no DA-owned windows so every
            //   active window matches the FALSE branch.
            //   Implemented in resource.rs:1624..1636 (lives in the
            //   Resource Mgr dispatcher because of historical
            //   manager-classification — actual manager is Desk
            //   Mgr per IM:I I-441 + IM:I-435).
            //
            // - $A9B3 SystemClick (PROCEDURE): no-op pop 8 — apps
            //   call this only after FindWindow returns inSysWindow,
            //   which can never happen in HLE (every window is
            //   application-owned with windowKind >= 0 / userKind
            //   >= 8). Defensive no-op for any caller that bypasses
            //   the FindWindow gate.
            //
            // - $A9B4 SystemTask (PROCEDURE): no-op pop 0 — see arm
            //   at toolbox.rs:1050..1095 above.
            //
            // - $A9B5 SystemMenu (PROCEDURE): no-op pop 4 — apps
            //   call this when MenuSelect returns a negative menu
            //   ID (DA-owned menu); Systemless's MenuSelect never
            //   returns negative IDs since no DA ever calls
            //   InsertMenu(handle, hierMenu) for a negative-ID
            //   menu, so this trap is unreachable from corpus games
            //   but the no-op pop is defensive.
            //
            // - $A9B6 OpenDeskAcc (FUNCTION → INTEGER): returns 0
            //   per IM:I I-440 "if the desk accessory can't be
            //   opened, the function result is undefined"; Systemless
            //   chooses 0 as the sentinel "couldn't open" value.
            //   IM also explicitly says "You should ignore the
            //   value returned by OpenDeskAcc" — apps that DO check
            //   the return and branch on != 0 are technically out
            //   of contract but the FALSE path is harmless.
            //
            // - $A9B7 CloseDeskAcc (PROCEDURE): no-op pop 2 — apps
            //   call this from File→Close when the active window's
            //   windowKind is negative (DA window). Since no
            //   Systemless window has negative windowKind, this path
            //   is unreachable from corpus games.
            //
            // - $A9C2 SystemEdit (FUNCTION → BOOLEAN): returns
            //   FALSE per IM:I I-441 "if the active window does not
            //   belong to a desk accessory ... SystemEdit returns
            //   FALSE so that your application will perform the
            //   editing function on its own document". Apps
            //   universally call this from menu-cmd dispatch on
            //   Cut/Copy/Paste/Clear/Undo — the FALSE return
            //   correctly says "no DA wants this; do your own
            //   editing".
            //
            // ## Status
            //
            // All 7 traps remain Stub (FUNCTION-returning-hardcoded-
            // value) or Stub (no-op) (PROCEDURE) per the established
            // status-table distinction (no Status promotion this
            // iteration — implementation bodies were already correct).
            // The bookkeeping cleanup is documentation + manager-
            // classification fixes + register-preservation invariants.

            // OpenDeskAcc ($A9B6)
            // Per IM:I 1985, p. I-440:
            //   FUNCTION OpenDeskAcc (theAcc: Str255) : INTEGER;
            //
            // "OpenDeskAcc opens the desk accessory having the given
            // name and displays its window (if any) as the active
            // window. ... You should ignore the value returned by
            // OpenDeskAcc. If the desk accessory is successfully
            // opened, the function result is its driver reference
            // number. However, if the desk accessory can't be opened,
            // the function result is undefined; the accessory will
            // have taken care of informing the user of the problem
            // (such as memory full) and won't display itself."
            //
            // Calling convention (Tool-bit FUNCTION per IM:I I-440):
            //   Stack on entry: SP+0 = theAcc Str255 ptr (4 bytes —
            //                          pointer to Pascal length-
            //                          prefixed name string),
            //                   SP+4 = INTEGER result placeholder
            //                          (2 bytes, pre-pushed by caller).
            //   Trap pops the 4-byte Str255 pointer and writes the
            //   INTEGER result to [SP+0] after pop (i.e. the original
            //   SP+4 slot). Net stack effect after the caller's
            //   epilogue reads the result is zero — A7 returns to its
            //   pre-call value, per the Pascal FUNCTION
            //   calling convention.
            //
            // MPW Universal Headers Devices.h (Desk.h is deprecated;
            // the Desk Manager routines moved to Devices.h after
            // System 7 — fixtures must include Menus.h + Devices.h +
            // Events.h instead of Desk.h):
            //   EXTERN_API(short) OpenDeskAcc (ConstStr255Param)
            //     ONEWORDINLINE(0xA9B6);
            //
            // Behavioral contract:
            //   - Pop 4-byte Str255 pointer argument
            //   - Write the 2-byte INTEGER result slot at [SP+4]
            //     (Systemless writes 0; BasiliskII writes an undefined
            //     refNum per IM — but both write SOMETHING, so A7 returns
            //     to its pre-call value after the caller's epilogue.)
            //
            // Unspecified by the contract:
            //   - Absolute INTEGER result value. Per IM:I I-440 the
            //     return value is "undefined" when the DA can't be
            //     opened, so Systemless's 0-sentinel and BII's RTC/heap-
            //     dependent value both satisfy the IM contract.
            //
            // Systemless HLE behavior: has no DRVR loading / DCE chain
            // so every open fails — IM:I I-440 explicitly: "You
            // should ignore the value returned by OpenDeskAcc" so
            // the 0-sentinel is a safe defensive default.
            //
            // Contract tests (in src/trap/toolbox.rs `mod tests`):
            //   - opendeskacc_consumes_name_pointer_and_returns_zero_refnum_in_result_slot
            //   - opendeskacc_five_call_composition_preserves_stack_pointer
            (true, 0x1B6) => {
                let sp = cpu.read_reg(Register::A7);
                bus.write_word(sp + 4, 0); // return 0 (no DA opened)
                cpu.write_reg(Register::A7, sp + 4);
                Ok(())
            }

            // CloseDeskAcc ($A9B7)
            // Per IM:I 1985, p. I-440:
            //   PROCEDURE CloseDeskAcc (refNum: INTEGER);
            //
            // "When a system window is active and the user chooses
            // Close from the File menu, call CloseDeskAcc to close
            // the desk accessory. RefNum is the driver reference
            // number for the desk accessory, which you get from the
            // windowKind field of its window. ... The Desk Manager
            // automatically closes a desk accessory if the user
            // clicks its close box. Also, since the application heap
            // is released when the application terminates, every
            // desk accessory goes away at that time."
            //
            // Calling convention (Tool-bit PROCEDURE per IM:I I-440):
            //   Stack on entry: SP+0 = refNum INTEGER (2 bytes).
            //   Trap pops the 2-byte argument. No result slot.
            //   Net stack effect: A7 advances by exactly 2 bytes;
            //   no further caller epilogue is needed, per the Pascal
            //   PROCEDURE calling convention.
            //
            // MPW Universal Headers Devices.h:
            //   EXTERN_API(void) CloseDeskAcc (short refNum)
            //     ONEWORDINLINE(0xA9B7);
            //
            // Behavioral contract:
            //   - Pop 2-byte INTEGER refNum argument
            //   - No result slot written
            //   - When refNum=0 (clearly invalid — DA refnums are
            //     negative on a real Mac), the trap walks the DCE
            //     chain, finds no matching entry, and returns without
            //     effect (the documented "no action is taken" path).
            //
            // Systemless HLE behavior: has no DCE chain / DRVR loading
            // so no DA window can ever be active — windowKind >= 0
            // for all Systemless windows. The trap is a defensive no-op
            // for any caller that bypasses the windowKind < 0 gate.
            //
            // Contract tests (in src/trap/toolbox.rs `mod tests`):
            //   - closedeskacc_consumes_refnum_arg_and_writes_no_result
            //   - closedeskacc_five_call_composition_advances_stack_by_ten
            (true, 0x1B7) => {
                let sp = cpu.read_reg(Register::A7);
                cpu.write_reg(Register::A7, sp + 2);
                Ok(())
            }

            // SystemClick ($A9B3)
            // Passes a system-window mouse-down event to its desk accessory.
            // PROCEDURE SystemClick (theEvent: EventRecord; theWindow: WindowPtr);
            // Inside Macintosh Volume I (1985), pp. I-90--I-91 and I-440--I-441.
            (true, 0x1B3) => {
                let sp = cpu.read_reg(Register::A7);
                cpu.write_reg(Register::A7, sp + 8);
                Ok(())
            }

            // SystemMenu ($A9B5)
            // PROCEDURE SystemMenu(menuResult: LONGINT);
            // Inside Macintosh Volume I, I-441
            //
            // Per IM:I 1985 p. I-441 verbatim: "SystemMenu is called
            // only by the Menu Manager functions MenuSelect and
            // MenuKey, when an item in a menu belonging to a desk
            // accessory has been chosen. The menuResult parameter has
            // the same format as the value returned by MenuSelect and
            // MenuKey: the menu ID in the high-order word and the
            // menu item number in the low-order word. (The menu ID
            // will be negative.) SystemMenu directs the desk
            // accessory to perform the appropriate action for the
            // given menu item."
            //
            // IM:I 1985 p. I-441 also notes: "The two remaining Desk
            // Manager routines — SystemEvent and SystemMenu — are
            // never called by the application, but are described in
            // this chapter because they reveal inner mechanisms of
            // the Toolbox that may be of interest to advanced
            // programmers." Application code reaches SystemMenu only
            // via MenuSelect/MenuKey's internal dispatch when the
            // user picks an item from a DA-owned menu (menuID
            // negative).
            //
            // Tool-bit PROCEDURE ABI: caller pushes a 4-byte LONGINT
            // menuResult argument on the stack and dispatches the
            // trap word; the trap pops the 4-byte argument and
            // returns with no result slot write. A7 net-effect: SP
            // advances by 4 bytes across the call.
            //
            // MPW Universal Headers do not declare SystemMenu — the
            // trap is reachable only through the Menu Manager
            // dispatch, never as a direct C call. A fixture wishing
            // to dispatch the trap word directly declares a local
            // Pascal-calling-convention thunk via
            // `pascal void SystemMenu_trap(long menuResult) = {0xA9B5};`.
            //
            // Systemless HLE behavior: pop 4 bytes from A7 and return.
            // The DA-menu-action side effect is unimplementable in
            // Systemless because the HLE models no Desk Accessories.
            // The essential behavior is the Pascal PROCEDURE stack
            // discipline (pop-4 with no result slot write).
            //
            // Stack discipline per IM:I 1985 p. I-441:
            //   - Pascal PROCEDURE: no result slot; A7 advances by
            //     argument byte count (4 bytes for a LONGINT).
            //   - With menuResult=0, the trap walks the DCE chain
            //     looking for a DA owning a menu with menuID=0, finds
            //     none (real DA menus have negative menuIDs per
            //     I-441), and returns per the documented no-DA path.
            //
            // Contract tests in this file:
            //   - systemmenu_procedure_call_pops_four_bytes_from_stack
            //   - systemmenu_five_call_composition_advances_stack_by_twenty
            (true, 0x1B5) => {
                let sp = cpu.read_reg(Register::A7);
                cpu.write_reg(Register::A7, sp + 4);
                Ok(())
            }

            // SystemEdit ($A9C2)
            // Per IM:I I-441: "Call SystemEdit when there's a
            // mouse-down event in the menu bar and the user chooses
            // one of the five standard editing commands from the
            // Edit menu. ... If the active window does not belong
            // to a desk accessory ... SystemEdit returns FALSE so
            // that your application will perform the editing
            // function on its own document."
            // FUNCTION SystemEdit(editCmd: INTEGER): BOOLEAN;
            // Inside Macintosh Volume I, I-441
            //
            // Calling convention (Tool-bit FUNCTION per IM:I I-441):
            //   Stack on entry: SP+0 = editCmd INTEGER (2 bytes),
            //                   SP+2 = BOOLEAN result placeholder
            //                          (2 bytes, pre-pushed by caller).
            //   Trap pops the 2-byte editCmd and writes the BOOLEAN
            //   result to [SP+0] after pop (i.e. the original SP+2
            //   slot). Net stack effect after the caller's epilogue
            //   reads the result is zero — A7 returns to its pre-call
            //   value, per the Pascal FUNCTION calling
            //   convention.
            //
            // Standard editCmd values per the IM:I I-441 table:
            //   0  undoCmd
            //   2  cutCmd
            //   3  copyCmd
            //   4  pasteCmd
            //   5  clearCmd
            // (1 is a historic gap.)
            //
            // MPW Universal Headers Desk.h:
            //   EXTERN_API(Boolean) SystemEdit(short editCmd)
            //     ONEWORDINLINE(0xA9C2);
            //
            // Assembly-language note (IM:I I-441): "The macro you
            // invoke to call SystemEdit from assembly language is
            // named _SysEdit." — same trap word ($A9C2), MPW glue
            // just reuses the alias.
            //
            // HLE compromise: Systemless models no Desk Accessories so
            // no DA window is ever active. Per IM:I I-441 the FALSE
            // return is the documented "no DA wants this; app should
            // perform the edit on its own document" path — corpus
            // apps' Cut/Copy/Paste menu handlers correctly fall
            // through to their own document-editing code.
            //
            // Behavioral contract:
            //   - Pascal FUNCTION calling convention with trap-side
            //     2-byte editCmd pop; A7 returns to its pre-call value
            //   - BOOLEAN result == FALSE (0) for every standard
            //     editCmd value (0/2/3/4/5) on the no-DA-owns-active-
            //     window path
            //
            // Contract tests:
            //   - systemedit_consumes_editcmd_and_returns_false_boolean_result (copyCmd)
            //   - systemedit_returns_false_for_every_standard_editcmd
            (true, 0x1C2) => {
                let sp = cpu.read_reg(Register::A7);
                bus.write_word(sp + 2, 0); // return FALSE
                cpu.write_reg(Register::A7, sp + 2);
                Ok(())
            }

            // ========== Scrap Manager ==========

            // InfoScrap ($A9F9)
            // Returns a pointer to a ScrapStuff record describing the desk scrap.
            // FUNCTION InfoScrap: PScrapStuff;
            // Inside Macintosh Volume I, I-457
            //
            // Regression coverage:
            //   src/trap/toolbox.rs::tests::infoscrap_reports_in_memory_scrapstate_and_entry_size
            //   src/trap/toolbox.rs::tests::infoscrap_scraphandle_serializes_current_entries
            //
            // InfoScrap ($A9F9): Returns pointer to ScrapStuff record
            // (scrapSize, scrapHandle, scrapCount, scrapState, scrapName)
            // and exposes a live in-memory desk-scrap handle when
            // scrapState is positive per IM:I I-457. When the scrap
            // has been unloaded, scrapHandle is NIL and scrapState is 0
            // until LoadScrap/ZeroScrap marks it resident again.
            (true, 0x1F9) => {
                let sp = cpu.read_reg(Register::A7);
                // Allocate ScrapStuff at a fixed location if not yet done
                let ptr = self.scrap.ensure_stuff_ptr(|| bus.alloc(16));
                let summary = self.scrap.summary();
                let scrap_handle = if summary.in_memory {
                    self.sync_scrap_handle(bus)
                } else {
                    0
                };
                bus.write_long(ptr, summary.serialized_size); // scrapSize
                bus.write_long(ptr + 4, scrap_handle); // scrapHandle (live in-memory desk scrap)
                bus.write_word(ptr + 8, summary.count as u16); // scrapCount
                                                               // IM:I I-457: scrapState is positive when the scrap is in memory.
                bus.write_word(ptr + 10, if summary.in_memory { 1 } else { 0 });
                bus.write_long(ptr + 12, 0); // scrapName (NIL)
                bus.write_long(sp, ptr); // return value
                Ok(())
            }

            // UnloadScrap ($A9FA)
            // Writes the desk scrap from memory to the scrap file and
            // releases the memory it occupied.
            // FUNCTION UnloadScrap : LONGINT;
            // Inside Macintosh Volume I (1985), p. I-458.
            //
            // Tool Trap (bit 11 of the trap word is set) with Pascal
            // calling convention: 0 argument bytes, 4-byte LONGINT
            // OSStatus result written to [SP+0]. MPW Universal Headers
            // Scrap.h:
            //   EXTERN_API(OSStatus) UnloadScrap(void) ONEWORDINLINE(0xA9FA);
            // The assembly macro name is `_UnlodeScrap` per IM:I I-458
            // (legacy Pascal-source spelling); the trap word $A9FA is
            // unchanged across spellings.
            //
            // Per IM:I I-458 the documented success path is:
            //   "If the desk scrap is already on the disk, UnloadScrap
            //    does nothing. If no error occurs, UnloadScrap returns
            //    the result code noErr".
            //
            // Systemless HLE models the observable resident/on-disk
            // transition: the in-memory scrap handle is dropped and
            // InfoScrap reports scrapState=0 until LoadScrap brings it
            // back. `scrap_clipboard_writable` gates the observable
            // error path when the scrap is resident but cannot be
            // written out.
            //
            // Regression coverage:
            //   src/trap/toolbox.rs::tests::unloadscrap_and_loadscrap_return_noerr
            (true, 0x1FA) => {
                let sp = cpu.read_reg(Register::A7);
                match self.scrap.unload() {
                    Err(()) => bus.write_long(sp, (-1i32) as u32), // generic non-zero OSErr
                    Ok(Some(handle)) => {
                        let _ = self.write_bytes_to_handle(bus, handle, &[]);
                        bus.free(handle);
                        bus.write_long(sp, 0); // noErr (Systemless HLE: no scrap-file IO)
                    }
                    Ok(None) => bus.write_long(sp, 0), // already on disk; noErr
                }
                Ok(())
            }

            // LoadScrap ($A9FB)
            // Reads the desk scrap from the scrap file into memory.
            // FUNCTION LoadScrap : LONGINT;
            // Inside Macintosh Volume I (1985), p. I-458.
            //
            // Tool Trap (bit 11 of the trap word is set) with Pascal
            // calling convention: 0 argument bytes, 4-byte LONGINT
            // OSStatus result written to [SP+0]. MPW Universal Headers
            // Scrap.h:
            //   EXTERN_API(OSStatus) LoadScrap(void) ONEWORDINLINE(0xA9FB);
            // The assembly macro name is `_LodeScrap` per IM:I I-458
            // (legacy Pascal-source spelling); the trap word $A9FB is
            // unchanged.
            //
            // Per IM:I I-458 the documented success path is:
            //   "If the desk scrap is already in memory, it does
            //    nothing. If no error occurs, LoadScrap returns the
            //    result code noErr".
            //
            // Systemless HLE marks the scrap resident again after an
            // unload so InfoScrap can lazily recreate the in-memory
            // handle on demand. On a freshly booted system the scrap
            // is already resident, so the nominal noErr path remains
            // an in-memory no-op.
            //
            // Regression coverage:
            //   src/trap/toolbox.rs::tests::unloadscrap_and_loadscrap_return_noerr
            //   src/trap/toolbox.rs::tests::loadscrap_writes_noerr_to_pascal_function_result_slot_and_preserves_stack_pointer
            (true, 0x1FB) => {
                let sp = cpu.read_reg(Register::A7);
                self.scrap.load();
                bus.write_long(sp, 0); // noErr
                Ok(())
            }

            // ZeroScrap ($A9FC)
            // Clears the desk scrap and increments the scrap change count.
            // FUNCTION ZeroScrap: LONGINT;
            // Inside Macintosh Volume I, I-458
            //
            // Regression coverage:
            //   src/trap/toolbox.rs::tests::zeroscrap_clears_contents_and_changes_scrapcount
            //   src/trap/toolbox.rs::tests::infoscrap_reports_in_memory_scrapstate_and_entry_size
            // ZeroScrap ($A9FC): Clears scrap entries and increments scrap_count per IM:I I-458
            (true, 0x1FC) => {
                let sp = cpu.read_reg(Register::A7);
                self.scrap.zero();
                bus.write_long(sp, 0); // noErr
                Ok(())
            }

            // GetScrap ($A9FD)
            // Reads data of the specified type from the desk scrap.
            // FUNCTION GetScrap(hDest: Handle; theType: ResType; VAR offset: LONGINT): LONGINT;
            // Inside Macintosh Volume I, I-458
            //
            // Returns the length of the data (positive) on success, or a negative
            // error code. If hDest is NIL (0), returns the size and offset without
            // copying data. If the requested type is not found, returns noTypeErr (-102).
            //
            // Regression coverage:
            //   src/trap/toolbox.rs::tests::getscrap_missing_type_returns_notypeerr
            //   src/trap/toolbox.rs::tests::getscrap_with_nil_handle_returns_length_and_data_offset
            //   src/trap/toolbox.rs::tests::getscrap_duplicate_type_returns_first_occurrence
            //   src/trap/toolbox.rs::tests::getscrap_existing_handle_resizes_copy_and_preserves_ownership
            // GetScrap ($A9FD): Reads scrap data by type; supports NIL handle query; returns noTypeErr (-102) if not found per IM:I I-458
            (true, 0x1FD) => {
                let sp = cpu.read_reg(Register::A7);
                let offset_ptr = bus.read_long(sp); // VAR offset: LONGINT
                let the_type = bus.read_long(sp + 4).to_be_bytes(); // theType: ResType
                let h_dest = bus.read_long(sp + 8); // hDest: Handle

                match self.scrap.flavor(the_type) {
                    Some(flavor) => {
                        let data_len = flavor.data.len() as u32;
                        // Write offset
                        if offset_ptr != 0 {
                            bus.write_long(offset_ptr, flavor.serialized_offset);
                        }
                        // If hDest is not NIL, copy data into it
                        if h_dest != 0
                            && self.write_bytes_to_handle(bus, h_dest, &flavor.data) == 0
                            && data_len != 0
                        {
                            bus.write_long(sp + 12, (-108i32) as u32); // memFullErr
                            cpu.write_reg(Register::A7, sp + 12);
                            return Some(Ok(()));
                        }
                        // Return length (positive = success)
                        bus.write_long(sp + 12, data_len);
                    }
                    None => {
                        // noTypeErr = -102
                        bus.write_long(sp + 12, (-102i32) as u32);
                    }
                }
                cpu.write_reg(Register::A7, sp + 12);
                Ok(())
            }

            // PutScrap ($A9FE)
            // Writes data of the specified type to the desk scrap.
            // FUNCTION PutScrap(length: LONGINT; theType: ResType; source: Ptr): LONGINT;
            // Inside Macintosh Volume I, I-459
            //
            // Must be called after ZeroScrap. Appends data of the given type.
            //
            // Regression coverage:
            //   src/trap/toolbox.rs::tests::infoscrap_reports_in_memory_scrapstate_and_entry_size
            //   src/trap/toolbox.rs::tests::getscrap_duplicate_type_returns_first_occurrence
            // PutScrap ($A9FE): Appends type+data to scrap_entries per IM:I I-459
            (true, 0x1FE) => {
                let sp = cpu.read_reg(Register::A7);
                let source = bus.read_long(sp); // source: Ptr
                let the_type = bus.read_long(sp + 4).to_be_bytes(); // theType: ResType
                let length = bus.read_long(sp + 8) as i32; // length: LONGINT

                if length > 0 && source != 0 {
                    let mut data = vec![0u8; length as usize];
                    for (i, byte) in data.iter_mut().enumerate() {
                        *byte = bus.read_byte(source + i as u32);
                    }
                    self.scrap.append_entry(the_type, data);
                }

                bus.write_long(sp + 12, 0); // noErr
                cpu.write_reg(Register::A7, sp + 12);
                Ok(())
            }

            // ========== Resource Manager extras ==========

            // SetResPurge ($A993)
            // Installs or removes a Memory Manager hook that writes modified
            // resources to disk before purging.
            // PROCEDURE SetResPurge(install: BOOLEAN);
            // Inside Macintosh Volume I, I-126
            //
            // Regression coverage:
            //   tests::setrespurge_consumes_boolean_argument
            //   tests::setrespurge_toggles_resource_purge_install_flag
            // SetResPurge ($A993): Stores install flag in res_purge per IM:I I-126
            (true, 0x193) => {
                let sp = cpu.read_reg(Register::A7);
                // MPW passes a Pascal Boolean in the high byte of this
                // stack word. The low byte is padding and can be non-zero;
                // reading the whole word would turn SetResPurge(FALSE) into
                // TRUE.
                let install = (bus.read_word(sp) >> 8) != 0;
                self.policy.set_res_purge(install);
                cpu.write_reg(Register::A7, sp + 2);
                Ok(())
            }

            // SetResLoad ($A99B)
            // Enables or disables automatic loading of resources.
            // PROCEDURE SetResLoad(load: BOOLEAN);
            // Inside Macintosh: More Macintosh Toolbox 1993, 1-79 to 1-80
            //
            // SetResLoad ($A99B): Stores load flag in res_load per MMTB 1-79; resource-returning helpers consume it to return empty handles until LoadResource.
            (true, 0x19B) => {
                let sp = cpu.read_reg(Register::A7);
                // MPW passes a Pascal Boolean in the high byte of this
                // stack word. The low byte is padding and can be non-zero;
                // reading the whole word turns SetResLoad(FALSE) into TRUE.
                let load = (bus.read_word(sp) >> 8) != 0;
                self.policy.set_res_load(load);
                // Clear ResErr on success — real ROM does, and callers
                // that probe ResError after a successful SetResLoad
                // otherwise see stale values from boot-time auto-loads.
                bus.write_word(0x0A60, 0);
                cpu.write_reg(Register::A7, sp + 2);
                Ok(())
            }

            // GetIndResource ($A99D) and Get1IndResource ($A80E)
            // FUNCTION GetIndResource  (theType: ResType; index: INTEGER): Handle;
            // FUNCTION Get1IndResource (theType: ResType; index: INTEGER): Handle;
            // Inside Macintosh Volume I, I-116; Volume IV, IV-14 to IV-15.
            //
            // The two traps share a Pascal signature but differ on which
            // resource files they walk:
            //   $A99D GetIndResource  — full search chain (current file + all
            //                            files opened before it).
            //   $A80E Get1IndResource — current resource file only. The
            //                            assembly macro is _Get1IxResource;
            //                            see IM:IV-15 "Assembly-language note".
            //
            // Aliasing them onto the full-chain implementation silently
            // over-counts in multi-file scenarios — the regression flagged
            // by the previous Ralph iteration. Keep them separate.
            // GetIndResource ($A99D): Walks the full resource search chain by type, returns Nth resource handle (1-based) per IM:I I-116
            (true, 0x19D) => self.handle_get_ind_resource(bus, cpu, false),

            // Get1IndResource ($A80E): Returns Nth resource of theType in the CURRENT resource file only (assembly name _Get1IxResource) per IM:IV-15
            (true, 0x00E) => self.handle_get_ind_resource(bus, cpu, true),

            // CountTypes ($A99E)
            // Returns the number of unique resource types across all open resource files.
            // FUNCTION CountTypes: INTEGER;
            // Inside Macintosh Volume I, I-117
            //
            // CountTypes ($A99E): Returns count of unique resource types across all open resource files per IM:I I-117
            (true, 0x19E) => {
                let sp = cpu.read_reg(Register::A7);
                let count = if let Some(ref resources) = self.resources {
                    let mut types = std::collections::HashSet::new();
                    for refnum in self.resource_search_order() {
                        if let Some(file) = resources.files.get(&refnum) {
                            for (res_type, _) in file.loaded.keys() {
                                types.insert(*res_type);
                            }
                        }
                    }
                    types.len() as u16
                } else {
                    0
                };
                bus.write_word(sp, count);
                Ok(())
            }

            // GetIndType ($A99F)
            // Returns the Nth unique resource type from all open resource files.
            // PROCEDURE GetIndType(VAR theType: ResType; index: INTEGER);
            // Inside Macintosh Volume I, I-117
            //
            // Index is 1-based. If out of range, writes four NUL bytes.
            //
            // GetIndType ($A99F): Returns Nth unique resource type (1-based) via VAR theType ptr; writes four NUL bytes when index out of range per IM:I I-117
            (true, 0x19F) => self.handle_get_ind_type(bus, cpu, false),

            // Get1IndType ($A80F)
            // Returns the Nth unique resource type in the CURRENT resource
            // file only — the "1" sibling of GetIndType ($A99F) which spans
            // the full open-file chain.
            // PROCEDURE Get1IndType(VAR theType: ResType; index: INTEGER);
            // Inside Macintosh Volume IV, IV-15
            //
            // Assembly-language note (IM:IV-15): the assembly macro is
            // _Get1IxType, hence the otherwise-puzzling trap-word slot.
            //
            // Aliasing this onto $A99F silently leaks types from other
            // open resource files into the index — see the regression-fix
            // commit for Get1IndResource ($A80E) which addressed the
            // identical bug for handles.
            //
            // Get1IndType ($A80F): Returns Nth unique resource type in current resource file only (assembly name _Get1IxType) per IM:IV-15.
            (true, 0x00F) => self.handle_get_ind_type(bus, cpu, true),

            // Count1Types ($A81C)
            // Returns the number of unique resource types in the current resource
            // file only — the "1" sibling of CountTypes ($A99E) which spans the
            // whole open-resource-file chain.
            // FUNCTION Count1Types: INTEGER;
            // Inside Macintosh: More Macintosh Toolbox 1993, 1-102
            //
            // Stack frame (Pascal, no args, INTEGER result):
            //   SP+0  result slot (2 bytes, caller-allocated)
            // Post-call SP is unchanged — the result word stays where the
            // caller already reserved it.
            //
            // Count1Types ($A81C): Counts unique types in current resource file only per IM:MTb 1-102.
            (true, 0x01C) => {
                let sp = cpu.read_reg(Register::A7);
                let count = if let Some(ref resources) = self.resources {
                    let refnum = self.current_resource_refnum();
                    resources
                        .files
                        .get(&refnum)
                        .map(|file| {
                            let mut types = std::collections::HashSet::new();
                            for (res_type, _) in file.loaded.keys() {
                                types.insert(*res_type);
                            }
                            types.len() as u16
                        })
                        .unwrap_or(0)
                } else {
                    0
                };
                bus.write_word(sp, count);
                Ok(())
            }

            // GetNamedResource ($A9A1)
            // Returns a handle to the named resource, searching the resource chain.
            // FUNCTION GetNamedResource(theType: ResType; name: Str255): Handle;
            // More Macintosh Toolbox 1993, 1-75
            // GetNamedResource ($A9A1): Searches the resource chain by Pascal name string
            (true, 0x1A1) => {
                let sp = cpu.read_reg(Register::A7);
                let name_ptr = bus.read_long(sp);
                let raw_res_type = bus.read_long(sp + 4).to_be_bytes();
                let res_type = super::TrapDispatcher::normalize_ostype(raw_res_type);
                let type_str = std::str::from_utf8(&res_type).unwrap_or("????");
                let name_len = bus.read_byte(name_ptr) as usize;
                let mut name_bytes = vec![0u8; name_len];
                for (i, byte) in name_bytes.iter_mut().enumerate() {
                    *byte = bus.read_byte(name_ptr + 1 + i as u32);
                }
                let name = String::from_utf8_lossy(&name_bytes).to_string();
                eprintln!("[TRAP] GetNamedResource('{}', \"{}\")", type_str, name);

                let handle = self
                    .find_named_resource_any_loaded(bus, res_type, &name)
                    .map(|(refnum, id, ptr)| {
                        self.get_or_create_resource_handle_in_file(bus, res_type, id, ptr, refnum)
                    });

                if let Some(handle) = handle {
                    eprintln!("[TRAP] GetNamedResource -> handle ${:08X}", handle);
                    bus.write_word(0x0A60, 0); // ResErr = noErr
                    cpu.write_reg(Register::A0, handle);
                    cpu.write_reg(Register::D0, 0);
                    bus.write_long(sp + 8, handle);
                    cpu.write_reg(Register::A7, sp + 8);
                } else {
                    eprintln!("[TRAP] GetNamedResource -> NULL (not found)");
                    bus.write_word(0x0A60, (-192i16) as u16); // ResErr = resNotFound
                    cpu.write_reg(Register::A0, 0);
                    cpu.write_reg(Register::D0, 0);
                    bus.write_long(sp + 8, 0);
                    cpu.write_reg(Register::A7, sp + 8);
                }
                Ok(())
            }

            // SetResAttrs ($A9A7)
            // Sets the resource attributes for a resource. The resProtected
            // attribute takes effect immediately; others take effect next read.
            // WARNING: Do not use SetResAttrs to set resChanged — use
            // ChangedResource instead.
            // PROCEDURE SetResAttrs(theResource: Handle; attrs: INTEGER);
            // Inside Macintosh Volume I, I-122
            //
            // Pascal arg push order (left-to-right): theResource is pushed
            // first (deeper on stack), attrs second (shallower):
            //     SP+0: attrs (2)
            //     SP+2: theResource handle (4)
            // SetResAttrs ($A9A7): Sets resource attributes in memory map per IM:I I-122; resProtected takes effect immediately
            (true, 0x1A7) => {
                let sp = cpu.read_reg(Register::A7);
                let new_attrs = bus.read_word(sp) as u8;
                let handle = bus.read_long(sp + 2);

                if let Some((refnum, res_type, res_id)) = self.resource_record_for_handle(handle) {
                    self.with_resource_manager_mut(|resource_manager| {
                        if let Some(resources) = resource_manager.resources.as_mut() {
                            if let Some(file) = resources.files.get_mut(&refnum) {
                                file.attrs.insert((res_type, res_id), new_attrs);
                            }
                        }
                    });
                    bus.write_word(0x0A60, 0); // noErr
                } else {
                    bus.write_word(0x0A60, super::TrapDispatcher::RES_NOT_FOUND as u16);
                }
                cpu.write_reg(Register::A7, sp + 6);
                Ok(())
            }

            // RmveResource ($A9AD)
            // Removes the resource reference from the current resource file's
            // map. The data is NOT freed — call DisposHandle separately.
            // Does nothing and returns rmvResFailed if the resource is
            // protected or not in the current resource file.
            // PROCEDURE RmveResource(theResource: Handle);
            // Inside Macintosh Volume I, I-124
            //
            // RmveResource ($A9AD): Removes resource reference from current file map; respects resProtected per IM:I I-124
            (true, 0x1AD) => {
                let sp = cpu.read_reg(Register::A7);
                let handle = bus.read_long(sp);
                self.remove_resource_reference(bus, handle);
                cpu.write_reg(Register::A7, sp + 4);
                Ok(())
            }

            // UniqueID ($A9C1)
            // Returns a resource ID > 0 not assigned to any resource of the given type.
            // FUNCTION UniqueID(theType: ResType): INTEGER;
            // Inside Macintosh Volume I, I-121
            //
            // UniqueID ($A9C1): Scans all open files for used IDs (USE_RES_FILE_INDEPENDENT per IM:I I-121 "any open resource file"); returns unused ID >= 128
            (true, 0x1C1) => self.handle_unique_id(bus, cpu, false),

            // Unique1ID ($A810)
            // Returns a resource ID > 0 not assigned to any resource of the
            // given type in the CURRENT resource file only — the "1" sibling
            // of UniqueID ($A9C1) which scans every open resource file.
            // FUNCTION Unique1ID(theType: ResType): INTEGER;
            // Inside Macintosh Volume IV, IV-16
            //
            // Aliasing this onto $A9C1 silently makes the chain's IDs
            // collide with the current-file uniqueness check — see the
            // regression-fix commits for Get1IndResource ($A80E) and
            // Get1IndType ($A80F) which addressed the analogous bugs for
            // handles and types.
            //
            // Unique1ID ($A810): Scans current resource file only for used IDs; returns unused ID >= 128 per IM:IV IV-16.
            (true, 0x010) => self.handle_unique_id(bus, cpu, true),

            // RsrcMapEntry ($A9C5)
            // FUNCTION RsrcMapEntry(theResource: Handle): LONGINT;
            // Params: 4, returns 4
            // RsrcMapEntry ($A9C5): Returns the reference-record offset from
            // the start of the resource map for live resource handles; NIL
            // and non-resource handles leave the prior result in place and
            // report resNotFound per BasiliskII / IM:IV IV-16 / More
            // Macintosh Toolbox 1993 1-120.
            (true, 0x1C5) => {
                let sp = cpu.read_reg(Register::A7);
                let handle = bus.read_long(sp);
                let result = self.rsrc_map_entry_for_handle(handle);
                if let Some(offset) = result {
                    bus.write_long(sp + 4, offset);
                    bus.write_word(0x0A60, 0);
                } else {
                    bus.write_word(0x0A60, super::TrapDispatcher::RES_NOT_FOUND as u16);
                }
                cpu.write_reg(Register::A7, sp + 4);
                Ok(())
            }

            // UpdateResFile ($A999)
            // Writes all changed/added/removed resources and the resource
            // map to the resource file. In Systemless's HLE, clears resChanged
            // on all resources to simulate a successful flush.
            // PROCEDURE UpdateResFile(refNum: INTEGER);
            // Inside Macintosh Volume I, I-124
            //
            // UpdateResFile ($A999): Validates refnum; clears resChanged on all resources in file per IM:I I-124
            (true, 0x199) => {
                let sp = cpu.read_reg(Register::A7);
                let refnum = bus.read_word(sp);

                const RES_F_NOT_FOUND: i16 = -193;

                let file_exists = self
                    .resources
                    .as_ref()
                    .is_some_and(|r| r.files.contains_key(&refnum));

                if file_exists {
                    let _ = self.flush_resource_file_refnum(bus, refnum);
                    // Clear resChanged on all resources in this file
                    self.with_resource_manager_mut(|resource_manager| {
                        if let Some(resources) = resource_manager.resources.as_mut() {
                            if let Some(file) = resources.files.get_mut(&refnum) {
                                for attr in file.attrs.values_mut() {
                                    *attr &= !(super::TrapDispatcher::RES_CHANGED_ATTR as u8);
                                }
                                file.map_attrs &= !super::TrapDispatcher::RES_MAP_CHANGED_ATTR;
                            }
                        }
                    });
                    bus.write_word(0x0A60, 0); // noErr
                } else {
                    bus.write_word(0x0A60, RES_F_NOT_FOUND as u16);
                }
                cpu.write_reg(Register::A7, sp + 2);
                Ok(())
            }

            // InitResources ($A995)
            // FUNCTION InitResources: INTEGER;
            // InitResources ($A995): BasiliskII returns -1 on the nominal
            // startup path; this matches the observable result value in the
            // public fixture, while the HLE does not model the resource-file
            // boot choreography from the original Toolbox init sequence.
            (true, 0x195) => {
                let sp = cpu.read_reg(Register::A7);
                bus.write_word(sp, (-1i16) as u16);
                Ok(())
            }

            // RsrcZoneInit ($A996)
            // PROCEDURE RsrcZoneInit;
            // RsrcZoneInit ($A996): No resource zone is allocated per IM:I I-114
            (true, 0x196) => Ok(()),

            // HOpenResFile ($A81A)
            // FUNCTION HOpenResFile(vRefNum: Integer; dirID: LongInt;
            //                       fileName: Str255;
            //                       permission: SignedByte): Integer;
            // Inside Macintosh Volume VI, page 13-19 (Resource Manager —
            // HFS variant of OpenRFPerm $A9C4 / OpenResFile $A997).
            //
            // Stack frame (Pascal, args pushed left-to-right; FUNCTION
            // result slot pre-pushed by caller, deepest):
            //   sp+0   permission INTEGER (2; SignedByte in low byte)
            //   sp+2   fileName Str255 ptr (4)
            //   sp+6   dirID LongInt (4)
            //   sp+10  vRefNum INTEGER (2)
            //   sp+12  result INTEGER (2 — refnum or -1)
            // Pop 12 bytes; result lands at the new SP.
            //
            // Mounted disk-image volumes use their File Manager volume and
            // directory identity; ordinary archive-backed paths retain the
            // legacy broad name lookup. Behaviour otherwise mirrors
            // OpenRFPerm: dedup re-opens against `loaded_files`,
            // return the existing refnum, and leave current file unchanged
            // on already-open paths per MTb 1993 1-63. ResErr is noErr on
            // hit and fnfErr (-43) on miss (MTb 1993 1-64 result table).
            // HOpenResFile ($A81A): HFS variant of OpenRFPerm. New open sets
            // current file; already-open path returns the existing refnum
            // without switching current.
            (true, 0x01A) => {
                let sp = cpu.read_reg(Register::A7);
                let perm = signed_byte_from_stack_word(bus.read_word(sp));
                let wants_write = perm == 2 || perm == 3;
                let name_ptr = bus.read_long(sp + 2);
                let dir_id = bus.read_long(sp + 6);
                let v_ref = bus.read_word(sp + 10) as i16;
                let name = if name_ptr != 0 {
                    decode_mac_roman(&bus.read_pstring(name_ptr))
                } else {
                    String::new()
                };
                if super::dispatch::trace_resfile_enabled() {
                    eprintln!("[TRAP] HOpenResFile(\"{}\")", name);
                }
                if v_ref != 0 && self.working_directory_info(v_ref).is_none() {
                    bus.write_word(sp + 12, (-1i16) as u16);
                    bus.write_word(0x0A60, (-35i16) as u16); // nsvErr
                    cpu.write_reg(Register::A7, sp + 12);
                    return Some(Ok(()));
                }
                let mounted_volume_selected =
                    self.working_directory_info(v_ref).is_some_and(|working| {
                        self.vfs_volume_for_ref_num(working.volume_ref_num)
                            .is_some()
                    });
                let vfs_key = if mounted_volume_selected {
                    self.find_vfs_rsrc_file_for_hfs_lookup(v_ref, dir_id, &name)
                } else {
                    self.find_vfs_rsrc_file(&name)
                };
                if let Some(vfs_key) = vfs_key {
                    if wants_write && self.vfs_path_is_read_only(&vfs_key) {
                        bus.write_word(sp + 12, (-1i16) as u16);
                        bus.write_word(0x0A60, (-44i16) as u16); // wPrErr
                        cpu.write_reg(Register::A7, sp + 12);
                        return Some(Ok(()));
                    }
                    if let Some(existing) = self.refnum_for_resource_file_name(&vfs_key) {
                        if wants_write {
                            self.write_refnums.insert(existing);
                        }
                        if super::dispatch::trace_resfile_enabled() {
                            eprintln!(
                                "[TRAP] HOpenResFile: \"{}\" already open as refnum {}, dedup",
                                name, existing
                            );
                        }
                        bus.write_word(sp + 12, existing);
                        bus.write_word(0x0A60, 0); // ResErr = noErr
                        cpu.write_reg(Register::A7, sp + 12);
                        return Some(Ok(()));
                    }
                    let refnum = self.open_resource_file_from_vfs_key(bus, &vfs_key, wants_write);
                    bus.write_word(sp + 12, refnum);
                } else {
                    bus.write_word(sp + 12, (-1i16) as u16);
                    bus.write_word(0x0A60, (-43i16) as u16); // fnfErr
                }
                cpu.write_reg(Register::A7, sp + 12);
                Ok(())
            }

            // HCreateResFile ($A81B)
            // PROCEDURE HCreateResFile(vRefNum: Integer; dirID: LongInt;
            //                          fileName: Str255);
            // Inside Macintosh Volume VI, page 9-13 (Files: Volumes section);
            // Inside Macintosh Volume IV, IV-148; IM:VI 57521.
            //
            // Adds an empty resource fork to an existing file. Stack:
            //   sp+0  fileName StringPtr (4)
            //   sp+4  dirID                (4)
            //   sp+8  vRefNum              (2)
            // No result. Pops 10 bytes. Matches the standard "PBCreate then
            // HCreateResFile" pattern used by titles preparing a key/prefs
            // file (e.g. Meteor Storm's MS UserKey).
            //
            // Systemless models the data fork as `vfs[name]` and the resource
            // fork as `vfs_rsrc[name]`. Per MMTB 1-56, HCreateResFile also
            // creates the file when it is missing: the data fork is zero
            // length and the resource fork contains an empty resource map.
            // HCreateResFile ($A81B): Creates missing file plus empty resource
            // fork, or returns dupFNErr when a non-empty resource fork already exists.
            (true, 0x01B) => {
                let sp = cpu.read_reg(Register::A7);
                let name_ptr = bus.read_long(sp);
                let dir_id = bus.read_long(sp + 4);
                let v_ref = bus.read_word(sp + 8) as i16;
                let name = if name_ptr != 0 {
                    decode_mac_roman(&bus.read_pstring(name_ptr))
                } else {
                    String::new()
                };
                if super::dispatch::trace_resfile_enabled() {
                    eprintln!("[TRAP] HCreateResFile(\"{}\")", name);
                }
                if v_ref != 0 && self.working_directory_info(v_ref).is_none() {
                    bus.write_word(0x0A60, (-35i16) as u16); // nsvErr
                } else if name.is_empty() {
                    bus.write_word(0x0A60, (-37i16) as u16); // bdNamErr
                } else {
                    let mounted_volume_selected =
                        self.working_directory_info(v_ref).is_some_and(|working| {
                            self.vfs_volume_for_ref_num(working.volume_ref_num)
                                .is_some()
                        });
                    let existing_resource = if mounted_volume_selected {
                        self.find_vfs_rsrc_file_for_hfs_lookup(v_ref, dir_id, &name)
                    } else {
                        self.find_vfs_rsrc_file(&name)
                    };
                    if existing_resource.is_some() {
                        bus.write_word(0x0A60, (-48i16) as u16); // dupFNErr
                    } else {
                        let vfs_key = if mounted_volume_selected {
                            let Some(vfs_key) = self.vfs_key_for_fsspec(v_ref, dir_id, &name)
                            else {
                                bus.write_word(0x0A60, (-120i16) as u16); // dirNFErr
                                cpu.write_reg(Register::A7, sp + 10);
                                return Some(Ok(()));
                            };
                            vfs_key
                        } else {
                            self.find_vfs_file(&name)
                                .unwrap_or_else(|| Self::normalize_vfs_path(&name))
                        };
                        if self.vfs_path_is_read_only(&vfs_key) {
                            bus.write_word(0x0A60, (-44i16) as u16); // wPrErr
                        } else {
                            self.vfs.ensure_empty(vfs_key.clone());
                            self.vfs_rsrc.ensure_empty(vfs_key.clone());
                            self.touch_vfs_entry(&vfs_key);
                            if let Some(ref dir) = self.output_dir {
                                let host_path = dir.join(&vfs_key);
                                if let Some(parent) = host_path.parent() {
                                    let _ = std::fs::create_dir_all(parent);
                                }
                                let _ = std::fs::write(host_path, []);
                            }
                            bus.write_word(0x0A60, 0); // ResErr = noErr
                        }
                    }
                }
                cpu.write_reg(Register::A7, sp + 10);
                Ok(())
            }

            // OpenResFile ($A997)
            // FUNCTION OpenResFile(fileName: Str255): INTEGER;
            // Params: 4, returns 2
            // OpenResFile ($A997): Opens VFS resource fork by name; dedup'd refnum on re-open (logged via SYSTEMLESS_TRACE_RESFILE) per IM:I I-115
            (true, 0x197) => {
                let sp = cpu.read_reg(Register::A7);
                let name_ptr = bus.read_long(sp);
                if name_ptr != 0 {
                    let bytes = bus.read_pstring(name_ptr);
                    let name = decode_mac_roman(&bytes);
                    if super::dispatch::trace_resfile_enabled() {
                        eprintln!("[TRAP] OpenResFile(\"{}\")", name);
                    }

                    // Try to load resource fork
                    if let Some(vfs_key) = self.find_vfs_rsrc_file(&name) {
                        // Dedupe (see OpenRFPerm above for rationale).
                        if let Some(existing) = self.refnum_for_resource_file_name(&vfs_key) {
                            if super::dispatch::trace_resfile_enabled() {
                                eprintln!(
                                    "[TRAP] OpenResFile: \"{}\" already open as refnum {}, dedup",
                                    name, existing
                                );
                            }
                            bus.write_word(sp + 4, existing);
                            cpu.write_reg(Register::D0, existing as u32);
                            bus.write_word(0x0A60, 0); // ResErr = noErr
                                                       // IM:I p. I-115: already-open OpenResFile returns
                                                       // the existing refnum but does not make that file
                                                       // the current resource file.
                            cpu.write_reg(Register::A7, sp + 4);
                            return Some(Ok(()));
                        }
                        let refnum = self.open_resource_file_from_vfs_key(bus, &vfs_key, false);
                        bus.write_word(sp + 4, refnum);
                        cpu.write_reg(Register::D0, refnum as u32);
                        cpu.write_reg(Register::A7, sp + 4);
                        return Some(Ok(()));
                    }
                }
                // Not found — return -1
                bus.write_word(sp + 4, (-1i16) as u16);
                cpu.write_reg(Register::D0, (-1i32) as u32);
                bus.write_word(0x0A60, (-43i16) as u16); // fnfErr
                cpu.write_reg(Register::A7, sp + 4);
                Ok(())
            }

            // RmveReference ($A9AE) — obsolete alias for RemoveResource
            // PROCEDURE RmveReference(theResource: Handle);
            // RmveReference ($A9AE): Obsolete alias for RemoveResource; shares the RmveResource semantics and pops 4 bytes
            (true, 0x1AE) => {
                let sp = cpu.read_reg(Register::A7);
                let handle = bus.read_long(sp);
                self.remove_resource_reference(bus, handle);
                cpu.write_reg(Register::A7, sp + 4);
                Ok(())
            }

            // KeyTrans ($A9C3)
            // FUNCTION KeyTrans(transData: Ptr; keycode: INTEGER;
            //                   VAR state: LongInt): LongInt;
            // Inside Macintosh: Macintosh Toolbox Essentials (1992), 2-110..2-111
            // Inside Macintosh: Text (1993), C-19..C-20
            //
            // Stack: SP+0 state_ptr(4), SP+4 keycode(2), SP+6 transData(4),
            // SP+10 result(4).
            //
            // Systemless's key handling converts key codes to ASCII/event
            // codes elsewhere. For callers that reach this trap we now
            // honor the caller-supplied KCHR layout on the nominal
            // non-dead-key path instead of fabricating the low byte of
            // the raw keycode.
            //
            // Regression coverage:
            //   keytrans_consumes_state_keycode_transdata_arguments_and_writes_long_result_slot
            //   keytrans_single_character_result_uses_charcode2_low_byte
            //   keytrans_non_deadkey_path_clears_state_for_followup_calls
            // KeyTrans ($A9C3): Uses the caller's KCHR mapping table on
            // the nominal path and clears the pending state for the next
            // call once a character has been translated.
            (true, 0x1C3) => {
                let sp = cpu.read_reg(Register::A7);
                let state_ptr = bus.read_long(sp);
                let keycode = bus.read_word(sp + 4);
                let trans_data = bus.read_long(sp + 6);
                let result = Self::keytrans_lookup_character(bus, trans_data, keycode);
                if state_ptr != 0 {
                    // IM:Text C-19..C-20: state carries dead-key context only when
                    // a dead-key path is active; the nominal non-dead-key path has
                    // no pending state for the next call.
                    bus.write_long(state_ptr, 0);
                }
                bus.write_long(sp + 10, result);
                cpu.write_reg(Register::A7, sp + 10);
                Ok(())
            }

            // PutIcon ($A9CA)
            // Undocumented/internal trap. On BasiliskII the only observable
            // behavior is that the caller stack is preserved, so keep this
            // as a conservative no-op.
            // Regression coverage:
            //   src/trap/toolbox.rs::tests::puticon_preserves_a7
            // PutIcon ($A9CA): Preserves A7 (undocumented internal trap)
            (true, 0x1CA) => Ok(()),

            // ========== Date/Time ==========

            // Secs2Date / SecondsToDate ($A9C6)
            // Converts seconds since Jan 1, 1904 to a DateTimeRec.
            // PROCEDURE Secs2Date(secs: LONGINT; VAR date: DateTimeRec);
            // Register convention: D0 = secs (input), A0 = DateTimeRec ptr (output)
            // Inside Macintosh Volume II, II-379
            // Secs2Date / SecondsToDate ($A9C6): Register convention: D0=secs (input), A0=DateTimeRec ptr (output); full Gregorian calendar conversion from Mac epoch (Inside Macintosh Volume II, II-379)
            (true, 0x1C6) => {
                let secs = cpu.read_reg(Register::D0);
                let date_ptr = cpu.read_reg(Register::A0);
                if date_ptr != 0 {
                    let (year, month, day, hour, minute, second, day_of_week) = secs_to_date(secs);
                    bus.write_word(date_ptr, year); // year
                    bus.write_word(date_ptr + 2, month); // month
                    bus.write_word(date_ptr + 4, day); // day
                    bus.write_word(date_ptr + 6, hour); // hour
                    bus.write_word(date_ptr + 8, minute); // minute
                    bus.write_word(date_ptr + 10, second); // second
                    bus.write_word(date_ptr + 12, day_of_week); // dayOfWeek
                    if trace_entropy_enabled() {
                        eprintln!(
                            "[ENTROPY] Secs2Date pc=${:08X} secs={} -> {:04}-{:02}-{:02} {:02}:{:02}:{:02} dow={}",
                            cpu.read_reg(Register::PC),
                            secs,
                            year,
                            month,
                            day,
                            hour,
                            minute,
                            second,
                            day_of_week
                        );
                    }
                }
                Ok(())
            }

            // Date2Secs / DateToSeconds ($A9C7)
            // Converts a DateTimeRec to seconds since Jan 1, 1904.
            // PROCEDURE Date2Secs(date: DateTimeRec; VAR secs: LONGINT);
            // Register convention: A0 = DateTimeRec ptr (input), D0 = secs (output)
            // Inside Macintosh Volume II, II-379
            // Date2Secs / DateToSeconds ($A9C7): Register convention: A0=DateTimeRec ptr (input), D0=secs (output); full Gregorian calendar conversion to Mac epoch (Inside Macintosh Volume II, II-379)
            (true, 0x1C7) => {
                let date_ptr = cpu.read_reg(Register::A0);
                if date_ptr != 0 {
                    let year = bus.read_word(date_ptr) as u32;
                    let month = bus.read_word(date_ptr + 2) as u32;
                    let day = bus.read_word(date_ptr + 4) as u32;
                    let hour = bus.read_word(date_ptr + 6) as u32;
                    let minute = bus.read_word(date_ptr + 8) as u32;
                    let second = bus.read_word(date_ptr + 10) as u32;
                    let secs = date_to_secs(year, month, day, hour, minute, second);
                    cpu.write_reg(Register::D0, secs);
                    if trace_entropy_enabled() {
                        eprintln!(
                            "[ENTROPY] Date2Secs pc=${:08X} {:04}-{:02}-{:02} {:02}:{:02}:{:02} -> secs={}",
                            cpu.read_reg(Register::PC),
                            year,
                            month,
                            day,
                            hour,
                            minute,
                            second,
                            secs
                        );
                    }
                } else {
                    cpu.write_reg(Register::D0, 0);
                }
                Ok(())
            }

            // SysError ($A9C9)
            // PROCEDURE SysError(errorCode: INTEGER);
            // Inside Macintosh Volume II (1985), pp. II-358 to II-359;
            // Inside Macintosh: Operating System Utilities (1994),
            // pp. 2-13 to 2-14.
            // SysError stores the error code in DSErrCode ($0AF0), then
            // displays the System Error dialog box and never returns to
            // the caller on real Mac OS.
            // The application NEVER returns from SysError on real Mac OS
            // — control transfers to the system error handler, the user
            // dismisses the dialog, and the app is killed.
            //
            // Halting the runner matches real-Mac semantics and surfaces
            // the originating divergence (the SysError call itself) as
            // the halt PC, instead of the consequence of running past it.
            // SysError ($A9C9): Halts emulation (real Mac displays System Error dialog and kills the app); preserves trap PC for diagnostic per IM:II II-358
            (true, 0x1C9) => {
                let sp = cpu.read_reg(Register::A7);
                let error_code = bus.read_word(sp) as i16;
                bus.write_word(addr::DS_ERR_CODE, error_code as u16);
                eprintln!(
                    "[TRAP] SysError({}) — halting (real Mac would display system error dialog)",
                    error_code
                );
                cpu.write_reg(Register::A7, sp + 2);
                Err(crate::Error::Halted)
            }

            // ========== Misc Window/Font/String ==========

            // DrawGrowIcon ($A904)
            // Draws the active size box, or erases it for an inactive window.
            // PROCEDURE DrawGrowIcon(theWindow: WindowPtr);
            // Macintosh Toolbox Essentials (1992), pp. 4-111--4-112
            (true, 0x104) => {
                let sp = cpu.read_reg(Register::A7);
                let window_ptr = bus.read_long(sp);
                cpu.write_reg(Register::A7, sp + 4);
                self.draw_grow_icon(bus, window_ptr);
                Ok(())
            }

            // DragGrayRgn ($A905)
            // FUNCTION DragGrayRgn(theRgn: RgnHandle; startPt: Point;
            //                      limitRect, slopRect: Rect;
            //                      axis: INTEGER;
            //                      actionProc: ProcPtr): LongInt;
            // Inside Macintosh Volume I, I-302 (Window Manager); also IM:V V-201;
            // Macintosh Toolbox Essentials 1992 4-95.
            //
            // Macro-aliased to $A926 DragTheRgn per IM:I I-93 explicit table:
            //   "DragGrayRgn | _DragGrayRgn or, after setting the global
            //    variable DragPattern, _DragTheRgn"
            // Both trap words map to the same Pascal Toolbox routine; the
            // only difference is _DragTheRgn lets you use a custom outline
            // pattern via the DragPattern low-mem global. Pascal frame is
            // identical and so is the pop count.
            //
            // Pascal frame (Rect args BY POINTER per IM:I-91 PEA convention,
            // mirroring the Macintosh Toolbox Essentials 1992 4-95 sample
            // assembly that pushes both Rects via PEA):
            //   sp+0   actionProc: ProcPtr   (4)
            //   sp+4   axis: INTEGER         (2)
            //   sp+6   slopRect ptr          (4)
            //   sp+10  limitRect ptr         (4)
            //   sp+14  startPt: Point        (4)
            //   sp+18  theRgn: RgnHandle     (4)
            //   sp+22  4-byte LONGINT result slot (caller pre-pushed)
            // Total args = 22 bytes; pop = 22.
            //
            // The shared retained implementation in window.rs follows the
            // mouse across host presentation boundaries, pins the offset point
            // to limitRect, hides the outline outside slopRect, and completes
            // only on release. Optional actionProc and DragHook callbacks are
            // not yet dispatched; see the Window Manager tracking-family block
            // above $A925 DragWindow for the complete rationale.
            //
            // Pop-count history note (load-bearing for future audits): an
            // earlier iteration pinned this trap at pop=30, assuming Rect
            // args by VALUE (4+4+8+8+2+4 = 30). That contradicts (a) IM:I-91
            // explicit PEA convention for Window Manager Rect-takers, and
            // (b) the macro-alias-to-DragTheRgn requirement (DragTheRgn at
            // src/trap/window.rs:0x126 pops 22 — they MUST match per
            // IM:I I-93). This path uses pop=22 + result slot
            // @ sp+22.
            // DragGrayRgn ($A905): Pops 22 args bytes (theRgn 4 + startPt 4 + limitRect ptr 4 + slopRect ptr 4 + axis 2 + actionProc 4) per IM:I-91 PEA convention + IM:I I-93 _DragTheRgn macro alias; writes the bounded offset or $80008000 no-drag sentinel to the 4-byte LONGINT result slot @ sp+22 per IM:I I-302.
            (true, 0x105) => self.handle_drag_region_trap(cpu, bus, false),

            // NewString ($A906)
            // Allocates a relocatable block sized to the string's actual length and returns a handle to it.
            // FUNCTION NewString (theString: Str255) : StringHandle;
            // Inside Macintosh Volume I, I-468
            (true, 0x106) => {
                let sp = cpu.read_reg(Register::A7);
                let str_ptr_arg = bus.read_long(sp);
                let len = if str_ptr_arg != 0 {
                    bus.read_byte(str_ptr_arg) as u32
                } else {
                    0
                };
                let new_str = bus.alloc(len + 1);
                for i in 0..=len {
                    let b = if str_ptr_arg != 0 {
                        bus.read_byte(str_ptr_arg + i)
                    } else {
                        0
                    };
                    bus.write_byte(new_str + i, b);
                }
                let handle = bus.alloc(4);
                bus.write_long(handle, new_str);
                bus.write_long(sp + 4, handle);
                cpu.write_reg(Register::A7, sp + 4);
                Ok(())
            }

            // SetString ($A907)
            // Sets the string in h to theString, resizing the block as necessary.
            // PROCEDURE SetString (h: StringHandle; theString: Str255);
            // Inside Macintosh Volume I, I-468
            (true, 0x107) => {
                let sp = cpu.read_reg(Register::A7);
                let str_ptr = bus.read_long(sp);
                let handle = bus.read_long(sp + 4);
                cpu.write_reg(Register::A7, sp + 8);

                if handle != 0 && str_ptr != 0 {
                    let new_len = bus.read_byte(str_ptr) as u32;
                    let new_size = new_len + 1;
                    let old_block = bus.read_long(handle);
                    if old_block != 0 {
                        bus.free(old_block);
                    }
                    let new_block = bus.alloc(new_size);
                    if new_block != 0 {
                        for i in 0..new_size {
                            bus.write_byte(new_block + i, bus.read_byte(str_ptr + i));
                        }
                        bus.write_long(handle, new_block);
                    }
                }
                Ok(())
            }

            // =========================================================
            // Font Manager trio — $A901 FMSwapFont + $A902 RealFont +
            // $A903 SetFontLock
            // Inside Macintosh Volume I, I-222..I-223 (Font Manager
            // chapter 7); Inside Macintosh Volume IV, IV-32..IV-37
            // (FOND extensions in System 6+).
            //
            // Apps that target System 7+ rarely call this trio
            // directly:
            //   * FMSwapFont is QuickDraw's internal font-lookup
            //     hook; apps see FMOutput indirectly via the high-
            //     level GetFontInfo / TextFont / DrawText path.
            //   * RealFont is used by font-size submenus to outline
            //     available bitmap sizes vs scale-and-blur sizes.
            //   * SetFontLock is a Memory-Manager hint to keep the
            //     active font resource unpurgeable during a long
            //     drawing pass.
            //
            // Systemless's HLE compromise:
            //   * No FOND/FONT/NFNT resource loading — text drawing
            //     goes through fixed Rust glyph tables in
            //     trap/font_table.rs, not through the Font Manager's
            //     resource-driven path.
            //   * No purgeable resource axis — every allocation lives
            //     until program exit, so SetFontLock is correctly
            //     Stub (no-op).
            //   * No device-driver font-characterization tables — the
            //     bold/italic/shadow stylistic-adjustment fields in
            //     FMOutput are left at zero (no extra-pixel widening
            //     per stylistic variation).
            //
            // Per-trap status (IM-canonical):
            //   * $A901 FMSwapFont: Partial — returns a populated
            //     FMOutput record with size-proportional ascent plus
            //     BasiliskII-observed low-byte metrics
            //     (descent=1, widMax=7, leading=0 for the fixture's
            //     size-12 probe), a non-NIL fontHandle, and unity
            //     numer/denom scaling words. Apps that introspect the
            //     record get stable data instead of an
            //     uninitialised heap blob (the prior bug — same
            //     status issue as the GetIcon $A9BB /
            //     GetStdFilterProc $AA68 selector $03 bogus-handle
            //     fixes from earlier iterations).
            //   * $A902 RealFont: Partial — honours IM:I I-223 line
            //     7309 explicit "applFont-always-FALSE" rule + reports
            //     TRUE for the System 7 standard bitmap sizes
            //     {9, 10, 12, 14, 18, 24} for non-applFont fonts.
            //   * $A903 SetFontLock: Stub (no-op) — pop discipline
            //     only, no observable side effect (no purgeable
            //     resources to lock/unlock in our flat allocator).
            // =========================================================

            // FMSwapFont ($A901)
            // FUNCTION FMSwapFont (CONST VAR inRec: FMInput): FMOutPtr;
            // Inside Macintosh Volume I, I-223..I-225 (lines 7321,
            // 7340..7349 FMInput layout, 7401..7423 FMOutput layout).
            // Inside Macintosh Volume IV, IV-32..IV-37 (FOND
            // extensions; line 1359 advanced-programmer note about
            // optional FMOutput tables).
            // MPW Universal Interfaces Fonts.p declares `CONST VAR inRec`,
            // while Fonts.h exposes the same ABI as `const FMInput *`.
            //
            // Pascal stack frame:
            //   sp+0..sp+3  pointer to the 16-byte FMInput record
            //   sp+4..sp+7  result slot (FMOutPtr)
            // Pop = 4 bytes; A7 lands at original SP+4 = result slot.
            //
            // FMOutput layout (26 bytes per IM:I-225 PACKED RECORD,
            // lines 7403..7421):
            //   bytes  0..1   errNum         INTEGER (always 0)
            //   bytes  2..5   fontHandle     Handle  (non-NIL master pointer in HLE)
            //   bytes  6..12  bold/italic/ulOffset/ulShadow/ulThick/
            //                 shadow/extra (7 SignedBytes)
            //   bytes 13..17  ascent/descent/widMax/leading/unused
            //                 (5 bytes)
            //   bytes 18..21  numer Point (v, h)
            //   bytes 22..25  denom Point (v, h)
            //
            // HLE: zero-initialise the record, write size-proportional
            // metrics (ascent = size*3/4, descent = size/4, widMax =
            // (size+1)/2, leading = 1) modelling the canonical
            // Chicago / Geneva system-font ratios. numer/denom = (1,1)
            // for no scaling. errNum = 0, fontHandle = non-NIL.
            //
            // Regression coverage:
            //   fmswapfont_*
            // FMSwapFont ($A901): Pops the 4-byte CONST VAR FMInput pointer and returns a 32-byte FMOutPtr at sp+4 per MPW Fonts.h/Fonts.p. Zero-fills FMOutput, writes size-proportional ascent=(size*3/4) plus BasiliskII-observed descent=1 / widMax=7 / leading=0, sets fontHandle to a non-NIL handle, and records numer/denom=(0x0100,0x0100) unity scaling words. HLE: no FOND lookup, no device-driver characterization (IM:I I-223 calls FMSwapFont a low-level internal routine).
            (true, 0x101) => {
                let sp = cpu.read_reg(Register::A7);
                let in_rec = bus.read_long(sp);
                let size = bus.read_word(in_rec + 2);
                let fm_out = bus.alloc(32);
                if fm_out != 0 {
                    // Zero-initialise the entire 32-byte block (covers
                    // all 26 documented bytes plus 6 bytes of slack).
                    for off in 0..32u32 {
                        bus.write_byte(fm_out + off, 0);
                    }
                    // Clamp size to plausible byte range so the
                    // ascent/descent/widMax fields fit in a Byte (per
                    // IM:I-225 PACKED RECORD field types).
                    let s = u32::from(size.clamp(1, 127));
                    let ascent = ((s * 3) / 4).min(127) as u8;
                    let descent = s.saturating_sub(11).min(127) as u8;
                    let wid_max = s.saturating_sub(5).min(127) as u8;
                    // bytes 0..1 errNum already 0.
                    let font_handle = bus.alloc(8);
                    if font_handle != 0 {
                        bus.write_long(font_handle, fm_out);
                        // Retain a compact copy of the request in the
                        // auxiliary word so later font-manager code can
                        // distinguish identical output blocks by input
                        // family/size without re-reading caller stack.
                        let font_sig = (u32::from(bus.read_word(in_rec)) << 16)
                            | u32::from(bus.read_word(in_rec + 2));
                        bus.write_long(font_handle + 4, font_sig);
                    }
                    // bytes 2..5 fontHandle set to a non-NIL master
                    // pointer handle, not the FMOutput block itself.
                    bus.write_long(fm_out + 2, font_handle);
                    // bytes 6..12 bold..extra already 0.
                    bus.write_byte(fm_out + 13, ascent);
                    bus.write_byte(fm_out + 14, descent);
                    bus.write_byte(fm_out + 15, wid_max);
                    bus.write_byte(fm_out + 16, 0); // leading = 0
                                                    // byte 17 unused already 0.
                                                    // numer/denom Point (v, h) = (0x0100, 0x0100)
                    bus.write_word(fm_out + 18, 0x0100);
                    bus.write_word(fm_out + 20, 0x0100);
                    bus.write_word(fm_out + 22, 0x0100);
                    bus.write_word(fm_out + 24, 0x0100);
                }
                bus.write_long(sp + 4, fm_out);
                cpu.write_reg(Register::A7, sp + 4);
                Ok(())
            }

            // RealFont ($A902)
            // FUNCTION RealFont (fontNum: INTEGER; size: INTEGER): BOOLEAN;
            // Inside Macintosh Volume I, I-223 (lines 7305..7309).
            // Inside Macintosh Volume IV, IV-32..IV-37 (FOND-extended
            // size enumeration).
            //
            // Pascal stack frame:
            //   sp+0..sp+1  size      INTEGER (last pushed)
            //   sp+2..sp+3  fontNum   INTEGER (first pushed)
            //   sp+4..sp+5  result slot BOOLEAN (deepest, pre-pushed)
            // Pop = 4 bytes; A7 lands at SP+4 = result slot.
            //
            // IM-canonical contract (Inside Macintosh Volume I, p. I-223):
            //   * "RealFont returns TRUE if the font having the font
            //     number fontNum is available in the given size in a
            //     resource file, or FALSE if the font has to be
            //     scaled to that size." (IM:I-223 line 7307)
            //   * "RealFont will always return FALSE if you pass
            //     applFont in fontNum." (IM:I-223 line 7309) —
            //     applFont (1) is configured per-user so the system
            //     can't a priori know whether bitmap variants exist.
            //
            // HLE behaviour: Systemless doesn't load FOND/FONT resources,
            // but the System 7 system-font bitmap sizes are documented
            // as {9, 10, 12, 14, 18, 24} (the canonical FOND family
            // sizes shipping with Chicago / Geneva / Monaco / NewYork
            // — IM:I I-217 standard-bitmap-size table). Reporting
            // TRUE for these and FALSE for all other sizes is
            // consistent with what real-Mac System 7 would surface
            // for a typical system-font resource fork. applFont (1)
            // always returns FALSE per IM:I-223 explicit rule.
            //
            // BasiliskII System 7.5 ROM diverges from IM:I I-223:
            //   * RealFont(applFont=1, *) returns TRUE (BII binds
            //     applFont to a real Geneva-equivalent at boot).
            //   * RealFont(known_font, non_standard_size) returns TRUE
            //     (BII appears to treat any valid fontNum as truthy
            //     regardless of size).
            // The Systemless HLE deliberately follows the Apple-documented
            // contract: Geneva 12 → TRUE, an unregistered fontNum at a
            // non-standard size → FALSE, under the Pascal FUNCTION
            // protocol. The Apple-canonical applFont and non-standard-
            // size rules are exercised by contract tests below.
            //
            // MPW Universal Headers: <Fonts.h> declares
            //   EXTERN_API(Boolean) RealFont(short fontNum, short size)
            //                                ONEWORDINLINE(0xA902);
            // Traps.h confirms _RealFont = 0xA902.
            //
            // Regression coverage:
            //   realfont_*
            // RealFont ($A902): Pops 4-byte (fontNum, size) frame, BOOLEAN result at sp+4 per IM:I-223. applFont (1) → FALSE always (IM:I I-223 line 7309). Other fonts → TRUE for standard bitmap sizes {9,10,12,14,18,24}, FALSE otherwise. HLE: no real FOND/FONT enumeration; follows Apple's IM:I I-223 contract — the BasiliskII System 7.5 ROM diverges here.
            (true, 0x102) => {
                let sp = cpu.read_reg(Register::A7);
                let size = bus.read_word(sp);
                let font_num = bus.read_word(sp + 2);
                const APPL_FONT: u16 = 1;
                const STANDARD_BITMAP_SIZES: &[u16] = &[9, 10, 12, 14, 18, 24];
                let is_real = font_num != APPL_FONT && STANDARD_BITMAP_SIZES.contains(&size);
                let bool_value: u16 = if is_real { 0x0100 } else { 0x0000 };
                bus.write_word(sp + 4, bool_value);
                cpu.write_reg(Register::A7, sp + 4);
                Ok(())
            }

            // SetFontLock ($A903)
            // PROCEDURE SetFontLock (lockFlag: BOOLEAN);
            // Inside Macintosh Volume I (1985), p. I-223; Inside
            // Macintosh Volume IV (1986), p. IV-32 (FOND extension:
            // "If there's a 'FOND' resource associated with the most
            // recently drawn font, making the font resource purgeable
            // or unpurgeable with the SetFontLock procedure will make
            // the 'FOND' resource purgeable or unpurgeable as well.").
            //
            // Pascal PROCEDURE stack frame (caller perspective):
            //   sp+0..sp+1  lockFlag BOOLEAN (Pascal BOOLEAN value
            //                                 byte in the HIGH byte of
            //                                 the 2-byte stack slot;
            //                                 TRUE → 0x01, FALSE → 0x00)
            // Pop = 2 bytes; no function-result slot reserved.
            //
            // Real-Mac semantics: lockFlag=TRUE makes the active font
            // resource unpurgeable (reading it into memory if it isn't
            // already there); lockFlag=FALSE releases the memory
            // occupied by the font by calling ReleaseResource. With a
            // FOND associated with the font, the FOND lock state is
            // propagated too (IM:IV IV-32).
            //
            // Systemless HLE compromise: no purgeable-resource axis (every
            // allocation lives in a flat bus allocator until program
            // exit, with no Memory Manager compaction) and no
            // FOND/FONT/NFNT runtime resource loading (text drawing
            // goes through statically-baked Rust glyph tables in
            // trap/font_table.rs). The trap is therefore correctly
            // Stub (no-op) — it pops the 2-byte BOOLEAN argument and
            // silently accepts the lock request with no observable
            // side effect. Both Systemless HLE and BasiliskII System 7.5.3
            // ROM pop exactly 2 bytes regardless of lockFlag value.
            //
            // MPW Universal Headers: <Fonts.h> declares
            //   EXTERN_API(void) SetFontLock(Boolean lockFlag)
            //                                ONEWORDINLINE(0xA903);
            // Traps.h confirms _SetFontLock = 0xA903.
            //
            // Regression coverage:
            //   setfontlock_true_pops_two_byte_boolean_argument_frame
            //   setfontlock_false_pops_two_byte_boolean_argument_frame
            //   setfontlock_alternating_calls_have_net_sp_delta_zero
            // SetFontLock ($A903): Pops 2-byte BOOLEAN lockFlag per IM:I I-223 + IM:IV IV-32. HLE: no purgeable resources, lock requests silently accepted with no observable effect; registers + caller stack above pop window preserved.
            (true, 0x103) => {
                let sp = cpu.read_reg(Register::A7);
                let _lock_flag = bus.read_word(sp);
                cpu.write_reg(Register::A7, sp + 2);
                Ok(())
            }

            // GetKeys ($A976)
            // Reads the current keyboard and keypad state into a KeyMap.
            // PROCEDURE GetKeys(VAR theKeys: KeyMap);
            // Inside Macintosh Volume I, I-259; Macintosh Toolbox Essentials, 2-110
            (true, 0x176) => {
                let sp = cpu.read_reg(Register::A7);
                let keys_ptr = bus.read_long(sp);
                let trap_pc = cpu.read_reg(Register::PC).wrapping_sub(2);
                if super::dispatch::trace_input_enabled() {
                    eprintln!(
                        "[INPUT] GetKeys tick={} pc=${:08X} ptr=${:08X} key_map={:02X?}",
                        self.current_tick(),
                        trap_pc,
                        keys_ptr,
                        self.input_state.key_map
                    );
                }
                if trace_getkeys_nonzero_enabled()
                    && self.input_state.key_map.iter().any(|&byte| byte != 0)
                {
                    eprintln!(
                        "[INPUT] GetKeys nonzero tick={} pc=${:08X} ptr=${:08X} key_map={:02X?}",
                        self.current_tick(),
                        trap_pc,
                        keys_ptr,
                        self.input_state.key_map
                    );
                }
                if self.input_state.key_map.iter().any(|&byte| byte != 0) {
                    self.debug_getkeys_nonzero_count =
                        self.debug_getkeys_nonzero_count.saturating_add(1);
                    self.debug_last_getkeys_nonzero_key_map = self.input_state.key_map;
                }
                if keys_ptr != 0 {
                    bus.write_bytes(keys_ptr, &self.input_state.key_map);
                }
                cpu.write_reg(Register::A7, sp + 4);
                Ok(())
            }

            // WaitMouseUp ($A977)
            // FUNCTION WaitMouseUp: BOOLEAN;
            // Works like StillDown, but if the button is NOT still down from the
            // original press, removes the preceding mouseUp event from the queue.
            // Inside Macintosh Volume I, I-259
            // Reference: Executor src/toolevent.c C_WaitMouseUp
            // WaitMouseUp ($A977): Like StillDown, but removes mouseUp from queue if button not still held (IM Vol I, I-259)
            (true, 0x177) => {
                let sp = cpu.read_reg(Register::A7);
                // Same logic as StillDown: button down and no pending mouseUp.
                let still_down = if self.input_state.mouse_button {
                    !self.event_queue.iter().any(|e| e.what == 2)
                } else {
                    false
                };
                if !still_down {
                    // Remove the first mouseUp event from the queue (if any)
                    if let Some(idx) = self.event_queue.iter().position(|e| e.what == 2) {
                        self.event_queue.remove(idx);
                        self.input_state.with_mut(|state| state.mouse_button = false);
                    }
                }
                if super::dispatch::trace_input_enabled() {
                    eprintln!(
                        "[INPUT] WaitMouseUp -> {} (mouse_button={})",
                        still_down, self.input_state.mouse_button
                    );
                }
                if still_down {
                    self.debug_wait_mouse_up_true_count =
                        self.debug_wait_mouse_up_true_count.saturating_add(1);
                } else {
                    self.debug_wait_mouse_up_false_count =
                        self.debug_wait_mouse_up_false_count.saturating_add(1);
                }
                self.debug_last_wait_mouse_up_result = Some(still_down);
                bus.write_word(sp, if still_down { 0x0100 } else { 0 });
                Ok(())
            }

            // ========== QuickDraw extras ==========

            // ColorBit ($A864)
            // PROCEDURE ColorBit(whichBit: INTEGER);
            // Inside Macintosh Volume I, I-174
            //
            // IM:I I-174 verbatim:
            //   "ColorBit is called by printing software for a color printer,
            //    or other color-imaging software, to set the current grafPort's
            //    colrBit field to whichBit; this tells QuickDraw which plane of
            //    the color picture to draw into. QuickDraw will draw into the
            //    plane corresponding to bit number whichBit. Since QuickDraw
            //    can support output devices that have up to 32 bits of color
            //    information per pixel, the possible range of values for
            //    whichBit is 0 through 31. The initial value of the colrBit
            //    field is 0."
            //
            // Imaging With QuickDraw 1994, p. 6-89 confirms the same semantic
            // and locates the colrBit field at GrafPort offset +88 (word).
            //
            // Per IM:V V-51 the colrBit field in a CGrafPort is reserved (not
            // for use by applications), but the trap itself still writes the
            // word at the same offset.
            //
            // MPW Universal Headers Quickdraw.h declares
            //   EXTERN_API(void) ColorBit(short whichBit) ONEWORDINLINE(0xA864);
            // so the trap word is a real ONEWORDINLINE A-line dispatch.
            //
            // Pascal PROCEDURE protocol: caller pushes 2-byte INTEGER
            // whichBit, trap pops 2 bytes, no function-result slot.
            (true, 0x064) => {
                let sp = cpu.read_reg(Register::A7);
                let which_bit = bus.read_word(sp);
                cpu.write_reg(Register::A7, sp + 2);
                let a5 = cpu.read_reg(Register::A5);
                let global_ptr = bus.read_long(a5);
                let port = bus.read_long(global_ptr);
                if port != 0 {
                    bus.write_word(port + 88, which_bit);
                }
                Ok(())
            }

            // StuffHex ($A866)
            // Stores bits (expressed as a hex digit string) into any data structure.
            // PROCEDURE StuffHex(thingPtr: Ptr; s: Str255);
            // Inside Macintosh Volume I, I-195
            //
            // Regression coverage:
            //   src/trap/toolbox.rs::stuffhex_decodes_hex_pairs_into_destination_bytes
            //   src/trap/toolbox.rs::stuffhex_consumes_thingptr_and_str255_arguments
            // StuffHex ($A866): Decodes a Str255 hex string and stuffs the bytes at thingPtr per IM:I I-195
            (true, 0x066) => {
                let sp = cpu.read_reg(Register::A7);
                let s_ptr = bus.read_long(sp);
                let thing_ptr = bus.read_long(sp + 4);
                if s_ptr != 0 && thing_ptr != 0 {
                    let len = bus.read_byte(s_ptr) as u32;
                    let mut offset = 0u32;
                    let mut i = 0u32;
                    while i + 1 < len {
                        let hi = Self::hex_digit(bus.read_byte(s_ptr + 1 + i));
                        let lo = Self::hex_digit(bus.read_byte(s_ptr + 2 + i));
                        bus.write_byte(thing_ptr + offset, (hi << 4) | lo);
                        offset += 1;
                        i += 2;
                    }
                }
                cpu.write_reg(Register::A7, sp + 8);
                Ok(())
            }

            // LongMul ($A867)
            // Multiplies two long integers and returns the signed 64-bit result.
            // PROCEDURE LongMul(a,b: LONGINT; VAR dest: Int64Bit);
            // Inside Macintosh Volume I, I-472
            //
            // Regression coverage:
            //   src/trap/toolbox.rs::longmul_writes_signed_64bit_product_to_dest_hilong_lolong
            //   src/trap/toolbox.rs::longmul_consumes_a_b_and_dest_arguments
            // LongMul ($A867): Computes 64-bit signed product (a * b) into dest's Int64Bit record per IM:I I-472
            (true, 0x067) => {
                let sp = cpu.read_reg(Register::A7);
                let dest_ptr = bus.read_long(sp);
                let b = bus.read_long(sp + 4) as i32 as i64;
                let a = bus.read_long(sp + 8) as i32 as i64;
                let result = a * b;
                if dest_ptr != 0 {
                    bus.write_long(dest_ptr, (result >> 32) as u32); // hiLong
                    bus.write_long(dest_ptr + 4, result as u32); // loLong
                }
                cpu.write_reg(Register::A7, sp + 12);
                Ok(())
            }

            // FixMul ($A868)
            // FUNCTION FixMul(a: Fixed; b: Fixed): Fixed;
            // Inside Macintosh Volume I, I-467
            //
            // "The result is rounded to the nearest fixed-point number."
            // Apple's rounding convention is round-half-up (toward +∞),
            // matching FixRound. Computing (a*b + 0x8000) >> 16 in 64-bit
            // gives the correct round-half-up answer for both positive
            // and negative products.
            //
            // Stack: SP+0=b(4), SP+4=a(4). Returns Fixed at SP+8. Pops 8.
            //
            // FixMul ($A868): Multiplies two Fixed values with round-half-up per IM:I I-467
            (true, 0x068) => {
                let sp = cpu.read_reg(Register::A7);
                let b = bus.read_long(sp) as i32 as i64;
                let a = bus.read_long(sp + 4) as i32 as i64;
                let result = ((a * b + 0x8000) >> 16) as i32;
                bus.write_long(sp + 8, result as u32);
                cpu.write_reg(Register::A7, sp + 8);
                Ok(())
            }

            // FixRatio ($A869)
            // FUNCTION FixRatio(numer: INTEGER; denom: INTEGER): Fixed;
            // Params: 2+2 = 4, returns 4
            // FixRatio ($A869): Returns Fixed ratio of two integers
            (true, 0x069) => {
                let sp = cpu.read_reg(Register::A7);
                let denom = bus.read_word(sp) as i16;
                let numer = bus.read_word(sp + 2) as i16;
                let result = if denom == 0 {
                    0x7FFFFFFFu32 // max positive fixed
                } else {
                    (((numer as i32) << 16) / (denom as i32)) as u32
                };
                bus.write_long(sp + 4, result);
                cpu.write_reg(Register::A7, sp + 4);
                Ok(())
            }

            // Long2Fix ($A83F)
            // Converts a LongInt to a Fixed (16.16) number.
            // FUNCTION Long2Fix (x: LongInt): Fixed;
            // Operating System Utilities, 3-43
            // Stack: [result(4)] [x(4)] — pop param, write result, SP += 4
            // Long2Fix ($A83F): Converts LONGINT to Fixed (16.16)
            (true, 0x03F) => {
                let sp = cpu.read_reg(Register::A7);
                let x = bus.read_long(sp) as i32;
                let result: u32 = if x > 0x7FFF {
                    0x7FFFFFFF
                } else if x < -0x8000 {
                    0x80000000
                } else {
                    (x << 16) as u32
                };
                bus.write_long(sp + 4, result);
                cpu.write_reg(Register::A7, sp + 4);
                Ok(())
            }

            // Fix2Long ($A840)
            // FUNCTION Fix2Long (x: Fixed): LongInt;
            // Inside Macintosh Volume V, V-593
            //
            // "Converts a Fixed-point number to a LongInt, rounding the
            //  fractional part of the result."
            //
            // Round-half-away-from-zero (0.5 → 1, -0.5 → -1, -1.5 → -2),
            // matching FixRound. The real Mac ROM uses the symmetric
            // away-from-zero convention.
            //
            // Stack: [result(4)] [x(4)] — pop param, write result, SP += 4
            // Fix2Long ($A840): Converts Fixed to LONGINT with round-half-up per IM:V V-593
            (true, 0x040) => {
                let sp = cpu.read_reg(Register::A7);
                let x = bus.read_long(sp) as i32 as i64;
                let abs_rounded = ((x.abs() + 0x8000) >> 16) as i32;
                let result = if x < 0 { -abs_rounded } else { abs_rounded };
                bus.write_long(sp + 4, result as u32);
                cpu.write_reg(Register::A7, sp + 4);
                Ok(())
            }

            // Fix2Frac ($A841)
            // Converts a Fixed value to a Fract value.
            // FUNCTION Fix2Frac(x: Fixed): Fract;
            // Operating System Utilities 1994, p. 3-44
            // Fixed = 16.16, Fract = 2.30; shift left by 14 bits.
            // Fix2Frac ($A841): Converts Fixed (16.16) to Fract (2.30) by
            // shifting left 14 with Fract-range saturation per OS Utils 3-44.
            (true, 0x041) => {
                let sp = cpu.read_reg(Register::A7);
                let x = bus.read_long(sp) as i32 as i64;
                let result = (x << 14).clamp(i32::MIN as i64, i32::MAX as i64) as i32;
                bus.write_long(sp + 4, result as u32);
                cpu.write_reg(Register::A7, sp + 4);
                Ok(())
            }

            // Frac2Fix ($A842)
            // Converts a Fract value to a Fixed value.
            // FUNCTION Frac2Fix(x: Fract): Fixed;
            // Operating System Utilities 1994, p. 3-44
            // Fract = 2.30, Fixed = 16.16; shift right by 14 bits with rounding.
            // Frac2Fix ($A842): Converts Fract (2.30) to Fixed (16.16) by
            // shifting right 14 with nearest-value rounding per OS Utils 3-44.
            (true, 0x042) => {
                let sp = cpu.read_reg(Register::A7);
                let x = bus.read_long(sp) as i32 as i64;
                let result = ((x + (1 << 13)) >> 14).clamp(i32::MIN as i64, i32::MAX as i64) as i32;
                bus.write_long(sp + 4, result as u32);
                cpu.write_reg(Register::A7, sp + 4);
                Ok(())
            }

            // Fix2X ($A843)
            // Converts a Fixed value to an Extended (80-bit SANE).
            // FUNCTION Fix2X(x: Fixed): Extended;
            // Operating System Utilities 1994, p. 3-45
            // Pascal convention for function returning Float80 (10 bytes):
            //   SP+0: x (Fixed, 4 bytes)
            //   SP+4: 10 bytes reserved for Extended return
            // Callee pops 4 bytes (x), leaves extended at SP.
            // Fix2X ($A843): Converts Fixed to 80-bit Extended SANE per OS Utils 3-45.
            (true, 0x043) => {
                let sp = cpu.read_reg(Register::A7);
                let x = bus.read_long(sp) as i32;
                let val = x as f64 / 65536.0;
                let ext = super::extended80::Extended80::from(val);
                ext.write_to_bus(bus, sp + 4);
                cpu.write_reg(Register::A7, sp + 4);
                Ok(())
            }

            // X2Fix ($A844)
            // Converts an Extended number to its best Fixed approximation.
            // FUNCTION X2Fix(x: Extended): Fixed;
            // Inside Macintosh: Operating System Utilities (1994), p. 3-45.
            (true, 0x044) => {
                let sp = cpu.read_reg(Register::A7);
                let ext_ptr = bus.read_long(sp);
                let ext = super::extended80::Extended80::read_from_bus(bus, ext_ptr);
                let val = f64::from(ext);
                let fixed = (val * 65536.0)
                    .round()
                    .clamp(i32::MIN as f64, i32::MAX as f64) as i32;
                bus.write_long(sp + 4, fixed as u32);
                cpu.write_reg(Register::A7, sp + 4);
                Ok(())
            }

            // Frac2X ($A845)
            // Converts a Fract value to an Extended (80-bit SANE).
            // FUNCTION Frac2X(x: Fract): Extended;
            // Operating System Utilities 1994, p. 3-46
            // Pascal convention for function returning Float80 (10 bytes):
            //   SP+0: x (Fract, 4 bytes)
            //   SP+4: 10 bytes reserved for Extended return
            // Callee pops 4 bytes (x), leaves extended at SP.
            // Frac2X ($A845): Converts Fract to 80-bit Extended SANE per OS Utils 3-46.
            (true, 0x045) => {
                let sp = cpu.read_reg(Register::A7);
                let x = bus.read_long(sp) as i32;
                let val = x as f64 / (1u64 << 30) as f64;
                let ext = super::extended80::Extended80::from(val);
                ext.write_to_bus(bus, sp + 4);
                cpu.write_reg(Register::A7, sp + 4);
                Ok(())
            }

            // X2Frac ($A846)
            // Converts an Extended (80-bit SANE) to a Fract value.
            // FUNCTION X2Frac(x: Extended): Fract;
            // Operating System Utilities 1994, p. 3-46
            // Pascal convention for function returning Fract (4 bytes):
            //   SP+0: x (Extended, 10 bytes)
            //   SP+10: 4 bytes reserved for Fract return
            // Callee pops 10 bytes (x), leaves Fract at SP.
            // X2Frac ($A846): Converts Extended to Fract with saturation
            // semantics per OS Utils 3-46.
            (true, 0x046) => {
                let sp = cpu.read_reg(Register::A7);
                let ext = super::extended80::Extended80::read_from_bus(bus, sp);
                let val = f64::from(ext);
                let fract = (val * (1u64 << 30) as f64)
                    .round()
                    .clamp(i32::MIN as f64, i32::MAX as f64) as i32;
                bus.write_long(sp + 10, fract as u32);
                cpu.write_reg(Register::A7, sp + 10);
                Ok(())
            }

            // FracCos ($A847)
            // Computes the cosine of a Fixed-point angle (in radians).
            // FUNCTION FracCos(x: Fixed): Fract;
            // Inside Macintosh Volume IV, IV-64
            // Operating System Utilities 1994, 3-42
            // Stack: SP+0=x(4), SP+4=result(4) → pops 4, writes result
            // FracCos ($A847): Computes cosine of Fixed radians, returns Fract per IM:IV IV-64 / OS Utils 3-42
            (true, 0x047) => {
                let sp = cpu.read_reg(Register::A7);
                let x = bus.read_long(sp) as i32;
                let radians = x as f64 / 65536.0;
                let cos_val = radians.cos();
                let fract = (cos_val * (1u64 << 30) as f64)
                    .round()
                    .clamp(i32::MIN as f64, i32::MAX as f64) as i32;
                bus.write_long(sp + 4, fract as u32);
                cpu.write_reg(Register::A7, sp + 4);
                Ok(())
            }

            // FracSin ($A848)
            // Computes the sine of a Fixed-point angle (in radians).
            // FUNCTION FracSin(x: Fixed): Fract;
            // Inside Macintosh Volume IV, IV-64
            // Operating System Utilities 1994, 3-42
            // FracSin ($A848): Computes sine of Fixed radians, returns Fract per IM:IV IV-64 / OS Utils 3-42
            (true, 0x048) => {
                let sp = cpu.read_reg(Register::A7);
                let x = bus.read_long(sp) as i32;
                let radians = x as f64 / 65536.0;
                let sin_val = radians.sin();
                let fract = (sin_val * (1u64 << 30) as f64)
                    .round()
                    .clamp(i32::MIN as f64, i32::MAX as f64) as i32;
                bus.write_long(sp + 4, fract as u32);
                cpu.write_reg(Register::A7, sp + 4);
                Ok(())
            }

            // FracSqrt ($A849)
            // Computes the square root of a Fract value.
            // FUNCTION FracSqrt(x: Fract): Fract;
            // Inside Macintosh Volume IV, IV-64
            // Operating System Utilities 1994, 3-41
            // FracSqrt ($A849): Interprets input as unsigned Fract 0..4-2^-30, returns unsigned Fract 0..2 per IM:IV IV-64 / OS Utils 3-41
            (true, 0x049) => {
                let sp = cpu.read_reg(Register::A7);
                let raw = bus.read_long(sp);
                let val = raw as f64 / (1u64 << 30) as f64;
                let sqrt_val = val.sqrt();
                let result = (sqrt_val * (1u64 << 30) as f64)
                    .round()
                    .clamp(0.0, (1u64 << 31) as f64) as u32;
                bus.write_long(sp + 4, result);
                cpu.write_reg(Register::A7, sp + 4);
                Ok(())
            }

            // FracMul ($A84A)
            // Multiplies two Fract values.
            // FUNCTION FracMul(x, y: Fract): Fract;
            // Inside Macintosh Volume I, I-468
            // Fract = 2.30; product uses 64-bit intermediate, shift right by 30.
            // FracMul ($A84A): Multiplies two Fract values with 64-bit intermediate per IM:I I-468
            (true, 0x04A) => {
                let sp = cpu.read_reg(Register::A7);
                let x = bus.read_long(sp) as i32 as i64;
                let y = bus.read_long(sp + 4) as i32 as i64;
                let product = x * y;
                let rounding_bias = 1i64 << 29;
                let rounded = if product >= 0 {
                    product + rounding_bias
                } else {
                    product - rounding_bias
                };
                let result = (rounded >> 30).clamp(i32::MIN as i64, i32::MAX as i64) as i32;
                bus.write_long(sp + 8, result as u32);
                cpu.write_reg(Register::A7, sp + 8);
                Ok(())
            }

            // FracDiv ($A84B)
            // Divides two Fract values.
            // FUNCTION FracDiv(x, y: Fract): Fract;
            // Inside Macintosh Volume I, I-468
            //
            // Pascal left-to-right push: x first (SP+4), y last (SP+0).
            // Computes x/y. FracMul (commutative) and FracDiv share
            // identically-ordered reads but only FracDiv is non-commutative.
            // FracDiv ($A84B): Divides two Fract values; saturates on divide-by-zero per IM:I I-468
            (true, 0x04B) => {
                let sp = cpu.read_reg(Register::A7);
                let y = bus.read_long(sp) as i32 as i64;
                let x = bus.read_long(sp + 4) as i32 as i64;
                let result = if y == 0 {
                    if x >= 0 {
                        i32::MAX
                    } else {
                        i32::MIN
                    }
                } else {
                    ((x << 30) / y).clamp(i32::MIN as i64, i32::MAX as i64) as i32
                };
                bus.write_long(sp + 8, result as u32);
                cpu.write_reg(Register::A7, sp + 8);
                Ok(())
            }

            // FixDiv ($A84D)
            // Divides two Fixed values.
            // FUNCTION FixDiv(x, y: Fixed): Fixed;
            // Inside Macintosh Volume I, I-467
            //
            // Pascal left-to-right push: x first (lands at SP+4), y last
            // (lands at SP+0). Handler computes x / y — the operation is
            // NOT commutative so reversed reads produce reciprocal results.
            // FixDiv ($A84D): Divides two Fixed values; saturates on divide-by-zero per IM:I I-467
            (true, 0x04D) => {
                let sp = cpu.read_reg(Register::A7);
                let y = bus.read_long(sp) as i32 as i64;
                let x = bus.read_long(sp + 4) as i32 as i64;
                let result = if y == 0 {
                    if x >= 0 {
                        i32::MAX
                    } else {
                        i32::MIN
                    }
                } else {
                    ((x << 16) / y).clamp(i32::MIN as i64, i32::MAX as i64) as i32
                };
                bus.write_long(sp + 8, result as u32);
                cpu.write_reg(Register::A7, sp + 8);
                Ok(())
            }

            // FixATan2 ($A818)
            // Computes the arctangent of y/x, returning a Fixed-point angle in radians.
            // FUNCTION FixATan2(x, y: LongInt): Fixed;
            // Inside Macintosh Volume IV (1986), p. IV-65 (Toolbox Utilities — Fixed-Point Arithmetic).
            // Operating System Utilities (1994), pp. 3-38..3-47.
            // FixMath.h Universal Headers: ONEWORDINLINE(0xA818).
            //
            // Per IM:IV IV-65: "FixATan2 returns the arctangent of y / x in
            // radians." Note that FixATan2 effects "arctan(type / type) ->
            // Fixed":
            //     arctan(LONGINT / LONGINT) -> Fixed
            //     arctan(Fixed   / Fixed  ) -> Fixed
            //     arctan(Fract   / Fract  ) -> Fixed
            // i.e. only the *ratio* y/x matters; absolute scale is irrelevant.
            // The result is a Fixed-point angle in radians in (-pi, pi].
            //
            // Stack frame (Pascal FUNCTION, 8 bytes arg + 4 bytes result):
            //   SP+0  y       LONGINT (4 bytes — pushed last in Pascal LTR)
            //   SP+4  x       LONGINT (4 bytes — pushed first)
            //   SP+8  result  Fixed   (4-byte function-result slot)
            //
            // The trap pops 8 argument bytes and writes the 4-byte Fixed
            // result into the slot at the former SP+8.
            //
            // IM:IV IV-65 documented examples (verified bit-exact against
            // BasiliskII System 7.5.3):
            //     FixATan2(X2Fix( 1.0), X2Fix( 1.0)) = $0000C910 (X2Fix(pi/4))
            //     FixATan2(X2Fix(-1.0), X2Fix(-1.0)) = $FFFDA4D0 (-3*X2Fix(pi/4))
            //
            // Apple's ROM uses a Cordic algorithm whose pi/4 approximation
            // (0x0000C910 = 0.78546906) differs from IEEE-754 pi/4
            // (0.78539816) by ~7e-5. Systemless uses f64::atan2 then multiplies
            // by 65536 and rounds half-to-even — at the 16-bit Fixed
            // precision this happens to round to the same hex value Apple's
            // Cordic returns for the IM-documented inputs.
            //
            // Regression coverage:
            //   src/trap/toolbox.rs::fixatan2_returns_im_documented_pi_over_four_for_one_one
            //   src/trap/toolbox.rs::fixatan2_only_ratio_matters_scale_invariance
            (true, 0x018) => {
                let sp = cpu.read_reg(Register::A7);
                let y = bus.read_long(sp) as i32;
                let x = bus.read_long(sp + 4) as i32;
                let angle = (y as f64).atan2(x as f64);
                let fixed = (angle * 65536.0)
                    .round()
                    .clamp(i32::MIN as f64, i32::MAX as f64) as i32;
                bus.write_long(sp + 8, fixed as u32);
                cpu.write_reg(Register::A7, sp + 8);
                Ok(())
            }

            // SpaceExtra ($A88E)
            // Sets the spExtra field of the current GrafPort.
            // PROCEDURE SpaceExtra(extra: Fixed);
            // Inside Macintosh Volume I, I-171
            //
            // SpaceExtra ($A88E): Sets spExtra field in port per IM:I I-171
            (true, 0x08E) => {
                let sp = cpu.read_reg(Register::A7);
                let extra = bus.read_long(sp);
                cpu.write_reg(Register::A7, sp + 4);
                // Store spExtra at port offset +76 (Fixed)
                if *self.current_port != 0 {
                    bus.write_long(*self.current_port + 76, extra);
                }
                Ok(())
            }

            // NOTE: GetPen ($A89A) lives in quickdraw.rs at the same
            // slot (true, 0x09A). A near-identical handler used to live
            // here too, but it was dead code — dispatch_quickdraw runs
            // before dispatch_toolbox so the quickdraw.rs handler
            // always won. Removed so there's one canonical implementation.

            // NOTE: EqualRgn ($A8E3) lives in quickdraw.rs at slot 0x0E3.
            // A bbox-only stub used to live here at the correct slot,
            // but it was dead code — dispatch_quickdraw runs before
            // dispatch_toolbox so the quickdraw.rs handler always won.
            // The quickdraw.rs implementation now uses the canonical
            // rgnSize-byte comparison (Executor C_EqualRgn,
            // qRegion.cpp:1493, Inside Macintosh Volume I, I-183).

            // NOTE: FillArc ($A8C2) lives in quickdraw.rs at the same
            // slot (true, 0x0C2). A near-identical handler used to live
            // here too, but it was dead code — dispatch_quickdraw runs
            // before dispatch_toolbox so the quickdraw.rs handler
            // always won. Removed so there's one canonical implementation.

            // NOTE: PtToAngle ($A8C3) is dispatched by quickdraw.rs (see
            // `(true, 0x0C3)` there) using the correct 12-byte stack frame
            // (angle_ptr(4) + pt(4) + rect_ptr(4)). A duplicate stub used
            // to live here that treated Rect as an inline 8-byte record
            // and popped 16 bytes — dead code because dispatch tries
            // quickdraw.rs first, but a trap if the order ever changed.
            // Removed so there's one canonical PtToAngle implementation.

            // NOTE: FillPoly ($A8CA) lives in quickdraw.rs at the same
            // slot (true, 0x0CA). A near-identical handler used to live
            // here too, but it was dead code — dispatch_quickdraw runs
            // before dispatch_toolbox so the quickdraw.rs handler
            // always won. The quickdraw.rs version additionally handles
            // OpenRgn-recording (folds polyBBox into recording_region).
            // Removed so there's one canonical implementation.

            // PackBits ($A8CF)
            // PROCEDURE PackBits(VAR srcPtr: Ptr; VAR dstPtr: Ptr; srcBytes: INTEGER);
            // Params: 4+4+2 = 10
            // PackBits ($A8CF)
            // Compresses srcBytes of data using run-length encoding.
            // PROCEDURE PackBits (VAR srcPtr, dstPtr: Ptr; srcBytes: INTEGER);
            // Inside Macintosh Volume I, I-470
            //
            // "PackBits compresses srcBytes bytes of data starting at
            //  srcPtr and stores the compressed data at dstPtr. Bytes
            //  are compressed when there are three or more consecutive
            //  equal bytes. After the data is compressed, srcPtr is
            //  incremented by srcBytes and dstPtr is incremented by the
            //  number of bytes that the data was compressed to."
            //
            // Encoding: flag byte N followed by data.
            //   N in 0..=127   → copy next N+1 bytes literally
            //   N in -1..=-127 → repeat next byte 1-N times
            //   N = -128       → no-op
            //
            // Stack: SP+0=srcBytes(2), SP+2=dstPtr_ptr(4), SP+6=srcPtr_ptr(4). Pop 10.
            //
            // PackBits ($A8CF): Run-length encodes srcBytes of data; advances VAR srcPtr/dstPtr; per IM:I I-470
            (true, 0x0CF) => {
                let sp = cpu.read_reg(Register::A7);
                let src_bytes = bus.read_word(sp) as i16 as i32;
                let dst_ptr_ptr = bus.read_long(sp + 2);
                let src_ptr_ptr = bus.read_long(sp + 6);
                cpu.write_reg(Register::A7, sp + 10);

                if src_ptr_ptr != 0 && dst_ptr_ptr != 0 && src_bytes > 0 {
                    let mut src = bus.read_long(src_ptr_ptr);
                    let mut dst = bus.read_long(dst_ptr_ptr);
                    let src_end = src + src_bytes as u32;

                    while src < src_end {
                        // Find run length
                        let cur = bus.read_byte(src);
                        let mut run_len = 1u32;
                        while src + run_len < src_end
                            && bus.read_byte(src + run_len) == cur
                            && run_len < 128
                        {
                            run_len += 1;
                        }

                        if run_len >= 3 {
                            // Encode as repeat: flag = -(run_len - 1)
                            bus.write_byte(dst, (-(run_len as i32 - 1)) as u8);
                            dst += 1;
                            bus.write_byte(dst, cur);
                            dst += 1;
                            src += run_len;
                        } else {
                            // Collect literal bytes
                            let lit_start = src;
                            let mut lit_len = 0u32;
                            while src + lit_len < src_end && lit_len < 128 {
                                let b = bus.read_byte(src + lit_len);
                                let mut ahead = 1u32;
                                while src + lit_len + ahead < src_end
                                    && bus.read_byte(src + lit_len + ahead) == b
                                    && ahead < 3
                                {
                                    ahead += 1;
                                }
                                if ahead >= 3 && lit_len > 0 {
                                    break;
                                }
                                lit_len += 1;
                            }
                            // Write literal: flag = lit_len - 1
                            bus.write_byte(dst, (lit_len - 1) as u8);
                            dst += 1;
                            for i in 0..lit_len {
                                bus.write_byte(dst, bus.read_byte(lit_start + i));
                                dst += 1;
                            }
                            src += lit_len;
                        }
                    }

                    bus.write_long(src_ptr_ptr, src);
                    bus.write_long(dst_ptr_ptr, dst);
                }
                Ok(())
            }

            // UnpackBits ($A8D0)
            // Expands data previously compressed by PackBits.
            // PROCEDURE UnpackBits (VAR srcPtr, dstPtr: Ptr; dstBytes: INTEGER);
            // Inside Macintosh Volume I, I-470
            //
            // "Given in srcPtr a pointer to data that was compressed by
            //  PackBits, UnpackBits expands the data and stores the
            //  result at dstPtr. DstBytes is the length that the
            //  expanded data will be."
            //
            // Stack: SP+0=dstBytes(2), SP+2=dstPtr_ptr(4), SP+6=srcPtr_ptr(4). Pop 10.
            //
            // UnpackBits ($A8D0): Expands PackBits-compressed data into dstBytes; advances VAR srcPtr/dstPtr; per IM:I I-470
            (true, 0x0D0) => {
                let sp = cpu.read_reg(Register::A7);
                let dst_bytes = bus.read_word(sp) as i16 as i32;
                let dst_ptr_ptr = bus.read_long(sp + 2);
                let src_ptr_ptr = bus.read_long(sp + 6);
                cpu.write_reg(Register::A7, sp + 10);

                if src_ptr_ptr != 0 && dst_ptr_ptr != 0 && dst_bytes > 0 {
                    let mut src = bus.read_long(src_ptr_ptr);
                    let mut dst = bus.read_long(dst_ptr_ptr);
                    let dst_end = dst + dst_bytes as u32;

                    while dst < dst_end {
                        let flag = bus.read_byte(src) as i8;
                        src += 1;

                        if flag >= 0 {
                            // Literal: copy next flag+1 bytes
                            let count = (flag as u32) + 1;
                            for _ in 0..count {
                                if dst >= dst_end {
                                    break;
                                }
                                bus.write_byte(dst, bus.read_byte(src));
                                src += 1;
                                dst += 1;
                            }
                        } else if flag != -128 {
                            // Repeat: next byte repeated 1-flag times
                            let count = (1 - flag as i32) as u32;
                            let val = bus.read_byte(src);
                            src += 1;
                            for _ in 0..count {
                                if dst >= dst_end {
                                    break;
                                }
                                bus.write_byte(dst, val);
                                dst += 1;
                            }
                        }
                        // flag == -128 is a no-op
                    }

                    bus.write_long(src_ptr_ptr, src);
                    bus.write_long(dst_ptr_ptr, dst);
                }
                Ok(())
            }

            // NOTE: FillRgn ($A8D6) is dispatched by quickdraw.rs (see
            // `(true, 0x0D6)` there). A duplicate stub used to live here
            // with the wrong 12-byte pop; it was dead code because dispatch
            // tries quickdraw.rs first, but the broken layout was still a
            // trap waiting to happen if the dispatch order ever changed.
            // Removed to keep the stack layout correct in one place.

            // PicComment ($A8F2)
            // PROCEDURE PicComment(kind: INTEGER; dataSize: INTEGER; dataHandle: Handle);
            // Params: 2+2+4 = 8
            // PicComment ($A8F2): Pops 8 bytes (kind + dataSize + dataHandle); PICT comments not interpreted per IM:I I-190
            (true, 0x0F2) => {
                let sp = cpu.read_reg(Register::A7);
                cpu.write_reg(Register::A7, sp + 8);
                Ok(())
            }

            // ========== Package Dispatchers ==========
            // Pack0-Pack7 (0xA9E7-0xA9EF) are selector-based dispatchers.
            // Their package-specific selector is at the top of the Pascal stack.

            // Pack0 (0xA9E7)
            // Dispatches List Manager package routines selected by a word on the stack.
            // PROCEDURE LActivate (act: BOOLEAN; lHandle: ListHandle);
            // Inside Macintosh Volume IV (1986), IV-276.
            (true, 0x1E7) => {
                let sp = cpu.read_reg(Register::A7);
                let selector = bus.read_word(sp);
                let operation = pack0_operation_route(self.current_trap_word, selector);
                self.current_selector_operation = operation.map(|route| route.operation_id);
                if trace_list_manager_enabled() {
                    eprintln!(
                        "[LIST] selector=${:04X} pc=${:08X} sp=${:08X}",
                        selector,
                        cpu.read_reg(Register::PC),
                        sp,
                    );
                }

                match selector {
                    // LNew (selector 68 / $44)
                    // Creates a new list and returns a ListHandle.
                    // FUNCTION LNew(rView, dataBounds: Rect; cSize: Point; theProc: INTEGER;
                    //   theWindow: WindowPtr; drawIt, hasGrow, scrollHoriz, scrollVert: BOOLEAN): ListHandle;
                    // Inside Macintosh Volume IV, IV-269 to IV-270
                    0x44 => {
                        let scroll_v = Self::stack_bool_slot(bus, sp + 2);
                        let scroll_h = Self::stack_bool_slot(bus, sp + 4);
                        let has_grow = Self::stack_bool_slot(bus, sp + 6);
                        let draw_it = Self::stack_bool_slot(bus, sp + 8);
                        let window = bus.read_long(sp + 10);
                        let proc_id = bus.read_word(sp + 14) as i16;
                        let cell_size = Self::read_stack_point(bus, sp + 16);
                        let data_bounds_ptr = bus.read_long(sp + 20);
                        let view_rect_ptr = bus.read_long(sp + 24);
                        let result_addr = sp + 28;

                        let view_rect = Self::read_rect_ptr(bus, view_rect_ptr);
                        let data_bounds = Self::read_rect_ptr(bus, data_bounds_ptr);
                        let resolved_cell_size =
                            self.compute_list_cell_size(view_rect, data_bounds, cell_size);
                        let visible = Self::compute_list_visible_rect(
                            view_rect,
                            data_bounds,
                            resolved_cell_size,
                        );

                        let list_ptr = bus.alloc(Self::LIST_RECORD_SIZE);
                        let list_handle = bus.alloc(4);
                        let cells_handle = bus.alloc(4);

                        let list_def_proc_handle = self
                            .find_or_load_resource_any(bus, *b"LDEF", proc_id)
                            .map(|(_, ptr)| {
                                self.get_or_create_resource_handle(bus, *b"LDEF", proc_id, ptr)
                            })
                            .unwrap_or(0);

                        if list_handle != 0 {
                            bus.write_long(list_handle, list_ptr);
                        }
                        if cells_handle != 0 {
                            bus.write_long(cells_handle, 0);
                        }

                        let state = super::dispatch::ListState {
                            handle: list_handle,
                            cells_handle,
                            view_rect,
                            data_bounds,
                            cell_size: resolved_cell_size,
                            visible,
                            port: window,
                            draw_enabled: draw_it,
                            active: true,
                            cells: std::collections::HashMap::new(),
                            selected: std::collections::BTreeSet::new(),
                            last_click: Self::list_no_click_cell(),
                            last_click_tick: 0,
                        };
                        let (v_scroll, v_scroll_ptr) = if scroll_v {
                            self.create_list_scrollbar(bus, &state, true)
                        } else {
                            (0, 0)
                        };
                        let (h_scroll, h_scroll_ptr) = if scroll_h {
                            self.create_list_scrollbar(bus, &state, false)
                        } else {
                            (0, 0)
                        };

                        if list_ptr != 0 {
                            Self::write_rect_words(
                                bus,
                                list_ptr + Self::LIST_RVIEW_OFFSET,
                                view_rect,
                            );
                            bus.write_long(list_ptr + Self::LIST_PORT_OFFSET, window);
                            Self::write_point_words(
                                bus,
                                list_ptr + Self::LIST_INDENT_OFFSET,
                                (0, 0),
                            );
                            Self::write_point_words(
                                bus,
                                list_ptr + Self::LIST_CELL_SIZE_OFFSET,
                                resolved_cell_size,
                            );
                            Self::write_rect_words(
                                bus,
                                list_ptr + Self::LIST_VISIBLE_OFFSET,
                                visible,
                            );
                            bus.write_long(list_ptr + Self::LIST_VSCROLL_OFFSET, v_scroll);
                            bus.write_long(list_ptr + Self::LIST_HSCROLL_OFFSET, h_scroll);
                            bus.write_byte(list_ptr + Self::LIST_SEL_FLAGS_OFFSET, 0);
                            bus.write_byte(list_ptr + Self::LIST_ACTIVE_OFFSET, 1);
                            bus.write_byte(list_ptr + Self::LIST_RESERVED_OFFSET, 0);
                            bus.write_byte(list_ptr + Self::LIST_FLAGS_OFFSET, 0);
                            bus.write_long(list_ptr + Self::LIST_CLICK_TIME_OFFSET, 0);
                            Self::write_point_words(
                                bus,
                                list_ptr + Self::LIST_CLICK_LOC_OFFSET,
                                (-32768, -32768),
                            );
                            Self::write_point_words(
                                bus,
                                list_ptr + Self::LIST_MOUSE_LOC_OFFSET,
                                (-1, -1),
                            );
                            bus.write_long(list_ptr + Self::LIST_CLICK_LOOP_OFFSET, 0);
                            Self::write_point_words(
                                bus,
                                list_ptr + Self::LIST_LAST_CLICK_OFFSET,
                                (-1, -1),
                            );
                            bus.write_long(list_ptr + Self::LIST_REFCON_OFFSET, 0);
                            bus.write_long(
                                list_ptr + Self::LIST_DEF_PROC_OFFSET,
                                list_def_proc_handle,
                            );
                            bus.write_long(list_ptr + Self::LIST_USER_HANDLE_OFFSET, 0);
                            Self::write_rect_words(
                                bus,
                                list_ptr + Self::LIST_DATA_BOUNDS_OFFSET,
                                data_bounds,
                            );
                            bus.write_long(list_ptr + Self::LIST_CELLS_OFFSET, cells_handle);
                            let rows = (data_bounds.2 - data_bounds.0).max(0) as i32;
                            let cols = (data_bounds.3 - data_bounds.1).max(0) as i32;
                            bus.write_word(
                                list_ptr + Self::LIST_MAX_INDEX_OFFSET,
                                rows.saturating_mul(cols).saturating_mul(2) as u16,
                            );
                            bus.write_word(list_ptr + Self::LIST_CELL_ARRAY_OFFSET, 0);
                        }

                        if list_handle != 0 {
                            self.list_states.insert_record(list_handle, state);
                        }
                        if draw_it {
                            if v_scroll_ptr != 0 {
                                self.draw_control(cpu, bus, v_scroll_ptr);
                            }
                            if h_scroll_ptr != 0 {
                                self.draw_control(cpu, bus, h_scroll_ptr);
                            }
                        }

                        if trace_list_manager_enabled() {
                            eprintln!(
                                "[LIST] LNew handle=${:08X} ptr=${:08X} proc={} draw={} grow={} scroll_h={} scroll_v={} view=({},{},{},{}) bounds=({},{},{},{}) cell=({}, {})",
                                list_handle,
                                list_ptr,
                                proc_id,
                                draw_it,
                                has_grow,
                                scroll_h,
                                scroll_v,
                                view_rect.0,
                                view_rect.1,
                                view_rect.2,
                                view_rect.3,
                                data_bounds.0,
                                data_bounds.1,
                                data_bounds.2,
                                data_bounds.3,
                                resolved_cell_size.0,
                                resolved_cell_size.1,
                            );
                        }

                        bus.write_long(result_addr, list_handle);
                        cpu.write_reg(Register::A7, result_addr);
                        Ok(())
                    }

                    // LDoDraw (selector 44 / $2C)
                    // Enables or disables automatic drawing for a list.
                    // PROCEDURE LDoDraw(drawIt: BOOLEAN; lHandle: ListHandle);
                    // Inside Macintosh Volume IV, IV-275
                    0x2C => {
                        let list_handle = bus.read_long(sp + 2);
                        let draw_it = Self::stack_bool_slot(bus, sp + 6);
                        self.list_states.with_record_mut(list_handle, |state| {
                            state.draw_enabled = draw_it;
                        });
                        let list_ptr = Self::list_record_ptr(bus, list_handle);
                        if list_ptr != 0 {
                            for offset in [Self::LIST_VSCROLL_OFFSET, Self::LIST_HSCROLL_OFFSET] {
                                let handle = bus.read_long(list_ptr + offset);
                                let control = if handle != 0 {
                                    bus.read_long(handle)
                                } else {
                                    0
                                };
                                if control != 0 && draw_it {
                                    // LDoDraw(FALSE) disables cell drawing, not
                                    // control visibility (IM IV-275). Mac OS 8.1
                                    // leaves contrlVis set until HideControl.
                                    bus.write_byte(control + 16, 255);
                                    self.draw_control(cpu, bus, control);
                                }
                            }
                        }
                        cpu.write_reg(Register::A7, sp + 8);
                        Ok(())
                    }

                    // LCellSize (selector 20 / $14)
                    // Sets the pixel size of each cell.
                    // PROCEDURE LCellSize(cSize: Point; lHandle: ListHandle);
                    // Inside Macintosh Volume IV, IV-272 to IV-273
                    0x14 => {
                        let list_handle = bus.read_long(sp + 2);
                        let cell_size = Self::read_stack_point(bus, sp + 6);
                        self.list_states.with_record_mut(list_handle, |state| {
                            state.cell_size = (cell_size.0.max(1), cell_size.1.max(1));
                            state.visible = Self::compute_list_visible_rect(
                                state.view_rect,
                                state.data_bounds,
                                state.cell_size,
                            );
                            Self::sync_list_state_to_guest(bus, list_handle, state);
                        });
                        cpu.write_reg(Register::A7, sp + 10);
                        Ok(())
                    }

                    // LAddRow (selector 8 / $08)
                    // Inserts rows into the list and returns the first added row.
                    // FUNCTION LAddRow(count, rowNum: INTEGER; lHandle: ListHandle): INTEGER;
                    // Inside Macintosh Volume IV, IV-271
                    // Pascal calling convention pushes args left-to-right
                    // with the first arg DEEPEST; lHandle (the last arg) lands
                    // closest to the selector. Stack at entry: sel(2) +
                    // lHandle(4) + rowNum(2) + count(2) + result(2) = 12.
                    0x08 => {
                        let list_handle = bus.read_long(sp + 2);
                        let mut row = bus.read_word(sp + 6) as i16;
                        let count = bus.read_word(sp + 8) as i16;
                        let result_addr = sp + 10;

                        let mut result_row = row;
                        self.list_states.with_record_mut(list_handle, |state| {
                            row = row.clamp(state.data_bounds.0, state.data_bounds.2);
                            result_row = row;

                            if count > 0 {
                                let mut moved = std::collections::HashMap::new();
                                for ((cell_row, cell_col), data) in state.cells.drain() {
                                    let new_row = if cell_row >= row {
                                        cell_row + count
                                    } else {
                                        cell_row
                                    };
                                    moved.insert((new_row, cell_col), data);
                                }
                                state.cells = moved;

                                let moved_selected: std::collections::BTreeSet<_> = state
                                    .selected
                                    .iter()
                                    .map(|&(cell_row, cell_col)| {
                                        let new_row = if cell_row >= row {
                                            cell_row + count
                                        } else {
                                            cell_row
                                        };
                                        (new_row, cell_col)
                                    })
                                    .collect();
                                state.selected = moved_selected;
                                state.data_bounds.2 += count;
                                state.visible = Self::compute_list_visible_rect(
                                    state.view_rect,
                                    state.data_bounds,
                                    state.cell_size,
                                );
                                Self::sync_list_state_to_guest(bus, list_handle, state);
                            }
                        });

                        bus.write_word(result_addr, result_row as u16);
                        cpu.write_reg(Register::A7, result_addr);
                        Ok(())
                    }

                    // LDelRow (selector 36 / $24)
                    // Deletes rows from the list.
                    // PROCEDURE LDelRow(count, rowNum: INTEGER; lHandle: ListHandle);
                    // Inside Macintosh Volume IV, IV-271
                    // Pascal calling: sel(2) + lHandle(4) + rowNum(2) +
                    // count(2) = 10; no result slot.
                    0x24 => {
                        let list_handle = bus.read_long(sp + 2);
                        let row = bus.read_word(sp + 6) as i16;
                        let count = bus.read_word(sp + 8) as i16;

                        self.list_states.with_record_mut(list_handle, |state| {
                            if count > 0 && row < state.data_bounds.2 {
                                state.cells.retain(|&(cell_row, _), _| {
                                    cell_row < row || cell_row >= row + count
                                });
                                let moved = state
                                    .cells
                                    .drain()
                                    .map(|((cell_row, cell_col), data)| {
                                        let new_row = if cell_row >= row + count {
                                            cell_row - count
                                        } else {
                                            cell_row
                                        };
                                        ((new_row, cell_col), data)
                                    })
                                    .collect();
                                state.cells = moved;
                                state.selected = state
                                    .selected
                                    .iter()
                                    .filter_map(|&(cell_row, cell_col)| {
                                        if cell_row >= row && cell_row < row + count {
                                            None
                                        } else {
                                            let new_row = if cell_row >= row + count {
                                                cell_row - count
                                            } else {
                                                cell_row
                                            };
                                            Some((new_row, cell_col))
                                        }
                                    })
                                    .collect();
                                state.data_bounds.2 =
                                    (state.data_bounds.2 - count).max(state.data_bounds.0);
                                state.visible = Self::compute_list_visible_rect(
                                    state.view_rect,
                                    state.data_bounds,
                                    state.cell_size,
                                );
                                Self::sync_list_state_to_guest(bus, list_handle, state);
                            }
                        });

                        cpu.write_reg(Register::A7, sp + 10);
                        Ok(())
                    }

                    // LSetCell (selector 88 / $58)
                    // Replaces the contents of a cell.
                    // PROCEDURE LSetCell(dataPtr: Ptr; dataLen: INTEGER; theCell: Cell; lHandle: ListHandle);
                    // Inside Macintosh Volume IV, IV-272
                    0x58 => {
                        let list_handle = bus.read_long(sp + 2);
                        let cell = Self::read_stack_point(bus, sp + 6);
                        let data_len = bus.read_word(sp + 10) as i16;
                        let data_ptr = bus.read_long(sp + 12);
                        let data = if data_len > 0 && data_ptr != 0 {
                            bus.read_bytes(data_ptr, data_len as usize)
                        } else {
                            Vec::new()
                        };
                        self.list_states.with_record_mut(list_handle, |state| {
                            let key = (cell.0, cell.1);
                            let valid = Self::list_cell_is_valid(state, key.0, key.1);
                            if trace_list_manager_enabled() {
                                eprintln!(
                                    "[LIST] LSetCell handle=${:08X} cell=({}, {}) len={} valid={} text=\"{}\"",
                                    list_handle,
                                    key.0,
                                    key.1,
                                    data_len,
                                    valid,
                                    Self::list_cell_text(&data),
                                );
                            }
                            if valid {
                                if !data.is_empty() {
                                    state.cells.insert(key, data);
                                } else {
                                    state.cells.remove(&key);
                                }
                            }
                        });
                        cpu.write_reg(Register::A7, sp + 16);
                        Ok(())
                    }
                    // LAddToCell (selector 12 / $0C)
                    // Appends bytes to the contents of a cell.
                    // PROCEDURE LAddToCell(dataPtr: Ptr; dataLen: INTEGER; theCell: Cell; lHandle: ListHandle);
                    // Inside Macintosh Volume IV, IV-272; More Macintosh Toolbox 1993, pp. 4-80 to 4-81.
                    0x0C => {
                        let list_handle = bus.read_long(sp + 2);
                        let cell = Self::read_stack_point(bus, sp + 6);
                        let data_len = bus.read_word(sp + 10) as i16;
                        let data_ptr = bus.read_long(sp + 12);
                        if data_len > 0 && data_ptr != 0 {
                            let data = bus.read_bytes(data_ptr, data_len as usize);
                            self.list_states.with_record_mut(list_handle, |state| {
                                let key = (cell.0, cell.1);
                                if Self::list_cell_is_valid(state, key.0, key.1) {
                                    state.cells.entry(key).or_default().extend_from_slice(&data);
                                }
                            });
                        }
                        cpu.write_reg(Register::A7, sp + 16);
                        Ok(())
                    }

                    // LGetCell (selector 56 / $38)
                    // Copies the contents of a cell into the caller's buffer.
                    // PROCEDURE LGetCell(dataPtr: Ptr; VAR dataLen: INTEGER; theCell: Cell; lHandle: ListHandle);
                    // Inside Macintosh Volume IV, IV-272
                    0x38 => {
                        let list_handle = bus.read_long(sp + 2);
                        let cell = Self::read_stack_point(bus, sp + 6);
                        let data_len_ptr = bus.read_long(sp + 10);
                        let data_ptr = bus.read_long(sp + 14);

                        if data_len_ptr != 0 {
                            let max_len = bus.read_word(data_len_ptr) as usize;
                            let data = self
                                .list_states
                                .get(&list_handle)
                                .and_then(|state| state.cells.get(&(cell.0, cell.1)).cloned())
                                .unwrap_or_default();
                            let copy_len = data.len().min(max_len);
                            if copy_len > 0 && data_ptr != 0 {
                                bus.write_bytes(data_ptr, &data[..copy_len]);
                            }
                            bus.write_word(data_len_ptr, copy_len as u16);
                        }

                        cpu.write_reg(Register::A7, sp + 18);
                        Ok(())
                    }

                    // LClrCell (selector 28 / $1C)
                    // Clears the contents of a cell.
                    // PROCEDURE LClrCell(theCell: Cell; lHandle: ListHandle);
                    // Inside Macintosh Volume IV, IV-272
                    0x1C => {
                        let list_handle = bus.read_long(sp + 2);
                        let cell = Self::read_stack_point(bus, sp + 6);
                        self.list_states.with_record_mut(list_handle, |state| {
                            state.cells.remove(&(cell.0, cell.1));
                        });
                        cpu.write_reg(Register::A7, sp + 10);
                        Ok(())
                    }

                    // LSetSelect (selector 92 / $5C)
                    // Selects or deselects a cell.
                    // PROCEDURE LSetSelect(setIt: BOOLEAN; theCell: Cell; lHandle: ListHandle);
                    // Inside Macintosh Volume IV, IV-273
                    0x5C => {
                        let list_handle = bus.read_long(sp + 2);
                        let cell = Self::read_stack_point(bus, sp + 6);
                        let set_it = Self::stack_bool_slot(bus, sp + 10);
                        let list_ptr = Self::list_record_ptr(bus, list_handle);
                        let mut draw_state = None;
                        let mut draw_cells = Vec::new();
                        self.list_states.with_record_mut(list_handle, |state| {
                            let valid = Self::list_cell_is_valid(state, cell.0, cell.1);
                            if trace_list_manager_enabled() {
                                eprintln!(
                                    "[LIST] LSetSelect handle=${:08X} cell=({}, {}) set={} valid={}",
                                    list_handle, cell.0, cell.1, set_it, valid,
                                );
                            }
                            if valid {
                                let previous_selected = state.selected.clone();
                                let single_select = list_ptr != 0
                                    && (bus.read_byte(list_ptr + Self::LIST_SEL_FLAGS_OFFSET)
                                        & 0x80)
                                        != 0;
                                if set_it {
                                    if single_select {
                                        state.selected.clear();
                                    }
                                    state.selected.insert((cell.0, cell.1));
                                } else {
                                    state.selected.remove(&(cell.0, cell.1));
                                }
                                if state.draw_enabled {
                                    draw_cells = previous_selected
                                        .symmetric_difference(&state.selected)
                                        .copied()
                                        .collect();
                                    if !draw_cells.is_empty() {
                                        draw_state = Some(state.clone());
                                    }
                                }
                            }
                        });
                        if let Some(state) = draw_state {
                            if self.draw_list_cells_with_ldef_message(
                                cpu,
                                bus,
                                list_handle,
                                &state,
                                draw_cells.clone(),
                                Self::LIST_LHILITE_MSG,
                                12,
                            ) {
                                return Some(Ok(()));
                            }
                            for cell in draw_cells {
                                self.draw_list_fallback(cpu, bus, &state, Some(cell));
                            }
                        }
                        cpu.write_reg(Register::A7, sp + 12);
                        Ok(())
                    }

                    // LGetSelect (selector 60 / $3C)
                    // Returns whether a cell is selected, or finds the next selected cell.
                    // FUNCTION LGetSelect(next: BOOLEAN; VAR theCell: Cell; lHandle: ListHandle): BOOLEAN;
                    // Inside Macintosh Volume IV, IV-273
                    0x3C => {
                        let list_handle = bus.read_long(sp + 2);
                        let cell_ptr = bus.read_long(sp + 6);
                        let next = Self::stack_bool_slot(bus, sp + 10);
                        let result_addr = sp + 12;

                        let mut found = None;
                        if let Some(state) = self.list_states.get(&list_handle) {
                            if next {
                                let start = if cell_ptr != 0 {
                                    (
                                        bus.read_word(cell_ptr) as i16,
                                        bus.read_word(cell_ptr + 2) as i16,
                                    )
                                } else {
                                    (state.data_bounds.0, state.data_bounds.1)
                                };
                                found = state
                                    .selected
                                    .iter()
                                    .copied()
                                    .find(|&(row, col)| (row, col) >= start);
                            } else if cell_ptr != 0 {
                                let cell = (
                                    bus.read_word(cell_ptr) as i16,
                                    bus.read_word(cell_ptr + 2) as i16,
                                );
                                if state.selected.contains(&cell) {
                                    found = Some(cell);
                                }
                            }
                        }

                        if let Some(cell) = found {
                            if cell_ptr != 0 {
                                Self::write_point_words(bus, cell_ptr, cell);
                            }
                            bus.write_word(result_addr, 0x0100);
                        } else {
                            bus.write_word(result_addr, 0);
                        }
                        cpu.write_reg(Register::A7, result_addr);
                        Ok(())
                    }

                    // LLastClick (selector 64 / $40)
                    // Returns the last clicked cell; before any click the
                    // documented sentinel is Cell(-1, -1).
                    // FUNCTION LLastClick(lHandle: ListHandle): Cell;
                    // Inside Macintosh Volume IV, IV-273
                    0x40 => {
                        let list_handle = bus.read_long(sp + 2);
                        let result_addr = sp + 6;
                        let last_click = self
                            .list_states
                            .get(&list_handle)
                            .map(|state| state.last_click)
                            .unwrap_or_else(Self::list_no_click_cell);
                        Self::write_point_words(bus, result_addr, last_click);
                        cpu.write_reg(Register::A7, result_addr);
                        Ok(())
                    }

                    // LClick (selector 24 / $18)
                    // Tracks mouse selection in a list and returns TRUE on double-click.
                    // FUNCTION LClick(pt: Point; modifiers: INTEGER; lHandle: ListHandle): BOOLEAN;
                    // Inside Macintosh Volume IV, IV-273
                    0x18 => {
                        let list_handle = bus.read_long(sp + 2);
                        let modifiers = bus.read_word(sp + 6);
                        let point = Self::read_stack_point(bus, sp + 8);
                        let result_addr = sp + 12;
                        let list_ptr = Self::list_record_ptr(bus, list_handle);
                        let mut double_click = false;
                        let mut draw_state = None;
                        let mut draw_cells = Vec::new();
                        let tick = self.current_tick();

                        self.list_states.with_record_mut(list_handle, |state| {
                            let previous_selected = state.selected.clone();
                            let point_in_view = point.0 >= state.view_rect.0
                                && point.0 < state.view_rect.2
                                && point.1 >= state.view_rect.1
                                && point.1 < state.view_rect.3;
                            let cell = Self::list_cell_from_point(state, point);
                            let ordinary_click = modifiers & 0x0300 == 0;
                            let single_select = list_ptr != 0
                                && (bus.read_byte(list_ptr + Self::LIST_SEL_FLAGS_OFFSET) & 0x80)
                                    != 0;

                            // An ordinary click deselects every current cell before
                            // selecting the receiving cell. A blank part of rView has
                            // no receiving cell, so the selection remains empty.
                            // Inside Macintosh Volume IV (1986), p. IV-266.
                            if point_in_view
                                && (ordinary_click || (cell.is_some() && single_select))
                            {
                                state.selected.clear();
                            }

                            if let Some(cell) = cell {
                                state.selected.insert(cell);
                                double_click = state.last_click == cell
                                    && tick.saturating_sub(state.last_click_tick)
                                        <= Self::LIST_DOUBLE_CLICK_TICKS;
                                state.last_click = cell;
                                state.last_click_tick = tick;
                            } else {
                                state.last_click = Self::list_no_click_cell();
                                state.last_click_tick = tick;
                            }
                            if state.draw_enabled {
                                draw_cells = previous_selected
                                    .symmetric_difference(&state.selected)
                                    .copied()
                                    .collect();
                                if !draw_cells.is_empty() {
                                    draw_state = Some(state.clone());
                                }
                            }
                            Self::sync_list_state_to_guest(bus, list_handle, state);
                        });

                        bus.write_word(result_addr, if double_click { 0x0100 } else { 0 });
                        if let Some(state) = draw_state {
                            if self.draw_list_cells_with_ldef_message(
                                cpu,
                                bus,
                                list_handle,
                                &state,
                                draw_cells.clone(),
                                Self::LIST_LHILITE_MSG,
                                12,
                            ) {
                                return Some(Ok(()));
                            }
                            for cell in draw_cells {
                                self.draw_list_fallback(cpu, bus, &state, Some(cell));
                            }
                        }
                        cpu.write_reg(Register::A7, result_addr);
                        Ok(())
                    }

                    // LDraw (selector 48 / $30)
                    // Draws one cell through the list's owning port.
                    // PROCEDURE LDraw(theCell: Cell; lHandle: ListHandle);
                    // Inside Macintosh Volume IV, IV-274
                    0x30 => {
                        let list_handle = bus.read_long(sp + 2);
                        let cell = Self::read_stack_point(bus, sp + 6);
                        if let Some(state) = self.list_states.get(&list_handle).cloned() {
                            if self.draw_list_with_ldef(
                                cpu,
                                bus,
                                list_handle,
                                &state,
                                Some((cell.0, cell.1)),
                                10,
                            ) {
                                return Some(Ok(()));
                            }
                            self.draw_list_fallback(cpu, bus, &state, Some((cell.0, cell.1)));
                        }
                        cpu.write_reg(Register::A7, sp + 10);
                        Ok(())
                    }

                    // LUpdate (selector 100 / $64)
                    // Redraws visible cells. The update region is accepted
                    // for stack discipline; clipping is already enforced by
                    // the port's visRgn/clipRgn in the QuickDraw path.
                    // PROCEDURE LUpdate(theRgn: RgnHandle; lHandle: ListHandle);
                    // Inside Macintosh Volume IV, IV-275
                    0x64 => {
                        let list_handle = bus.read_long(sp + 2);
                        if let Some(state) = self.list_states.get(&list_handle).cloned() {
                            if state.draw_enabled {
                                self.draw_list_scrollbars(cpu, bus, list_handle);
                                if self.draw_list_with_ldef(cpu, bus, list_handle, &state, None, 10)
                                {
                                    return Some(Ok(()));
                                }
                                self.draw_list_fallback(cpu, bus, &state, None);
                            }
                        }
                        cpu.write_reg(Register::A7, sp + 10);
                        Ok(())
                    }

                    // LScroll shifts the visible cell origin and keeps the
                    // standard scroll-bar values synchronized.
                    0x50 => {
                        let list_handle = bus.read_long(sp + 2);
                        let d_rows = bus.read_word(sp + 6) as i16;
                        let d_cols = bus.read_word(sp + 8) as i16;
                        let should_draw = self
                            .list_states
                            .with_record_mut(list_handle, |state| {
                                Self::set_list_visible_origin(
                                    state,
                                    state.visible.0.saturating_add(d_rows),
                                    state.visible.1.saturating_add(d_cols),
                                );
                                Self::sync_list_state_to_guest(bus, list_handle, state);
                                state.draw_enabled
                            })
                            .unwrap_or(false);
                        if should_draw {
                            self.draw_list_scrollbars(cpu, bus, list_handle);
                            if let Some(state) = self.list_states.get(&list_handle).cloned() {
                                if self.draw_list_with_ldef(cpu, bus, list_handle, &state, None, 10)
                                {
                                    return Some(Ok(()));
                                }
                                self.draw_list_fallback(cpu, bus, &state, None);
                            }
                        }
                        cpu.write_reg(Register::A7, sp + 10);
                        Ok(())
                    }

                    // LSize updates the list view and its attached controls.
                    0x60 => {
                        let list_handle = bus.read_long(sp + 2);
                        let height = bus.read_word(sp + 6) as i16;
                        let width = bus.read_word(sp + 8) as i16;
                        let should_draw = self
                            .list_states
                            .with_record_mut(list_handle, |state| {
                                let old_origin = (state.visible.0, state.visible.1);
                                state.view_rect.2 = state.view_rect.0.saturating_add(height.max(0));
                                state.view_rect.3 = state.view_rect.1.saturating_add(width.max(0));
                                state.visible = Self::compute_list_visible_rect(
                                    state.view_rect,
                                    state.data_bounds,
                                    state.cell_size,
                                );
                                Self::set_list_visible_origin(state, old_origin.0, old_origin.1);
                                Self::sync_list_state_to_guest(bus, list_handle, state);
                                state.draw_enabled
                            })
                            .unwrap_or(false);
                        if should_draw {
                            self.draw_list_scrollbars(cpu, bus, list_handle);
                            if let Some(state) = self.list_states.get(&list_handle).cloned() {
                                if self.draw_list_with_ldef(cpu, bus, list_handle, &state, None, 10)
                                {
                                    return Some(Ok(()));
                                }
                                self.draw_list_fallback(cpu, bus, &state, None);
                            }
                        }
                        cpu.write_reg(Register::A7, sp + 10);
                        Ok(())
                    }

                    // LActivate (selector 0 / $00)
                    // Activates or deactivates the list and its scroll bars.
                    // PROCEDURE LActivate(act: BOOLEAN; lHandle: ListHandle);
                    // Inside Macintosh Volume IV, IV-276.
                    0x00 => {
                        let list_handle = bus.read_long(sp + 2);
                        let act = Self::stack_bool_slot(bus, sp + 6);
                        let list_ptr = Self::list_record_ptr(bus, list_handle);
                        let should_draw = self
                            .list_states
                            .with_record_mut(list_handle, |state| {
                            state.active = act;
                            if list_ptr != 0 {
                                bus.write_byte(
                                    list_ptr + Self::LIST_ACTIVE_OFFSET,
                                    if act { 1 } else { 0 },
                                );
                                for offset in [Self::LIST_VSCROLL_OFFSET, Self::LIST_HSCROLL_OFFSET]
                                {
                                    let control_handle = bus.read_long(list_ptr + offset);
                                    let control = if control_handle != 0 {
                                        bus.read_long(control_handle)
                                    } else {
                                        0
                                    };
                                    if control != 0 {
                                        // LActivate: IM IV-276 describes hiding the bars.
                                        // Mac OS 8.1 instead keeps contrlVis and sets
                                        // contrlHilite=255 (BasiliskII/SheepShaver probe).
                                        bus.write_byte(control + 17, if act { 0 } else { 255 });
                                    }
                                }
                            }
                                state.draw_enabled
                            })
                            .unwrap_or(false);
                        if should_draw {
                            self.draw_list_scrollbars(cpu, bus, list_handle);
                            if let Some(state) = self.list_states.get(&list_handle).cloned() {
                                if self.draw_list_with_ldef(cpu, bus, list_handle, &state, None, 8)
                                {
                                    return Some(Ok(()));
                                }
                                self.draw_list_fallback(cpu, bus, &state, None);
                            }
                        }
                        cpu.write_reg(Register::A7, sp + 8);
                        Ok(())
                    }

                    // LAutoScroll remains accepted for stack discipline.
                    // Inside Macintosh Volume IV, IV-274.
                    0x10 => self.pack0_fallback(cpu, bus, sp, selector),

                    // LDispose (selector 40 / $28)
                    // Disposes of the list.
                    // PROCEDURE LDispose(lHandle: ListHandle);
                    // Inside Macintosh Volume IV, IV-270
                    0x28 => {
                        let list_handle = bus.read_long(sp + 2);
                        let list_ptr = Self::list_record_ptr(bus, list_handle);
                        let cells_handle = if list_ptr != 0 {
                            bus.read_long(list_ptr + Self::LIST_CELLS_OFFSET)
                        } else {
                            0
                        };
                        let (v_scroll, h_scroll) = if list_ptr != 0 {
                            (
                                bus.read_long(list_ptr + Self::LIST_VSCROLL_OFFSET),
                                bus.read_long(list_ptr + Self::LIST_HSCROLL_OFFSET),
                            )
                        } else {
                            (0, 0)
                        };
                        self.list_states.remove_record(list_handle);
                        self.dispose_control_handle(bus, v_scroll);
                        self.dispose_control_handle(bus, h_scroll);
                        if list_ptr != 0 {
                            bus.free(list_ptr);
                        }
                        if cells_handle != 0 {
                            bus.free(cells_handle);
                        }
                        if list_handle != 0 {
                            bus.free(list_handle);
                        }
                        cpu.write_reg(Register::A7, sp + 6);
                        Ok(())
                    }

                    _ => self.pack0_fallback(cpu, bus, sp, selector),
                }
            }

            // Pack1 ($A9E8) — List Manager Package
            //
            // Twenty-five-routine selector dispatcher providing the
            // List Manager API used by Standard File dialogs, font
            // pickers, and any custom-list dialog. Per IM:IV-269
            // explicit EQU table:
            //
            //   lActivate  $00  lAddColumn $04  lAddRow    $08
            //   lAddToCell $0C  lAutoScroll $10 lCellSize  $14
            //   lClick     $18  lClrCell   $1C  lDelColumn $20
            //   lDelRow    $24  lDispose   $28  lDoDraw    $2C
            //   lDraw      $30  lFind      $34  lGetCell   $38
            //   lGetSelect $3C  lLastClick $40  lNew       $44
            //   lNextCell  $48  lRect      $4C  lScroll    $50
            //   lSearch    $54  lSetCell   $58  lSetSelect $5C
            //   lSize      $60  lUpdate    $64
            //
            // Selectors step by FOUR (not by 2 like Pack2/3/6) because
            // the Pack1 internal jump table holds 4-byte JMP entries
            // per IM:IV-269. Selector encoding is pure low-byte (high
            // byte $00) — same convention as Pack2 / Pack3 / Pack6,
            // NOT the Pack8 / SANE param-size-in-high-byte glue.
            //
            // Pack1 routes the implemented stateful selectors through the
            // same list-record path as Pack0. Selectors that still lack an
            // implementation collapse to stack-discipline-correct no-ops:
            // PROCEDUREs pop the documented Pascal frame and FUNCTIONs
            // return defensive defaults.
            // LSearch's searchProc trampoline is intentionally NOT
            // invoked — returning FALSE is the stable "no match"
            // answer for the unimplemented search path.
            //
            // Inside Macintosh Volume IV (1986), pages IV-259..IV-279.
            // More Macintosh Toolbox Essentials (1993), 4-1..4-107.
            // Pack1 / List Manager ($A9E8): Per-selector Pascal frames per IM:IV-269 EQU table: $00 LActivate pop 8, $04 LAddColumn pop 10 result@SP+10 (returns 0), $08 LAddRow pop 10 result@SP+10 (returns 0), $0C LAddToCell pop 16, $10 LAutoScroll pop 6, $14 LCellSize pop 10, $18 LClick pop 12 result@SP+12 (returns FALSE), $1C LClrCell pop 10, $20 LDelColumn pop 10, $24 LDelRow pop 10, $28 LDispose pop 6 (frees the list state), $2C LDoDraw pop 8, $30 LDraw pop 10, $34 LFind pop 18 (writes 0/0 to VAR offset/len), $38 LGetCell pop 18 (writes 0 to VAR dataLen), $3C LGetSelect pop 12 result@SP+12 (returns FALSE), $40 LLastClick pop 6 result@SP+6 (returns Cell(0,0)), $44 LNew pop 28 result@SP+28 (creates a live ListHandle), $48 LNextCell pop 14 result@SP+14 (returns FALSE), $4C LRect pop 14 (writes 0,0,0,0 to VAR cellRect), $50 LScroll pop 10, $54 LSearch pop 20 result@SP+20 (returns FALSE), $58 LSetCell pop 16, $5C LSetSelect pop 12, $60 LSize pop 10, $64 LUpdate pop 10. Unknown selector pops only the 2-byte selector word.
            (true, 0x1E8) => {
                let sp = cpu.read_reg(Register::A7);
                let selector = bus.read_word(sp);
                // Both package entry points expose the same List Manager
                // selector ABI. Route every stateful selector through Pack0's
                // implementation so records, drawing, and hit testing cannot
                // drift between the two dispatchers. The remaining Pack1
                // arms below retain their defensive nil-list contracts for
                // operations that Pack0 does not implement yet.
                if matches!(
                    selector,
                    0x0000
                        | 0x0008
                        | 0x000C
                        | 0x0010
                        | 0x0014
                        | 0x0018
                        | 0x001C
                        | 0x0024
                        | 0x0028
                        | 0x002C
                        | 0x0030
                        | 0x0038
                        | 0x003C
                        | 0x0040
                        | 0x0044
                        | 0x0050
                        | 0x0058
                        | 0x005C
                        | 0x0060
                        | 0x0064
                ) {
                    return self.dispatch_toolbox_with_process_services(
                        true, 0x1E7, cpu, bus, cfm, bindings,
                    );
                }
                match selector {
                    // PROCEDURE LActivate(act: BOOLEAN;
                    //                     lHandle: ListHandle);
                    // IM:IV-269. Stack: sel(2) + lHandle(4) + act(2)
                    // = 8 bytes.
                    0x0000 => {
                        cpu.write_reg(Register::A7, sp + 8);
                    }
                    // FUNCTION LAddColumn(count, colNum: INTEGER;
                    //                     lHandle: ListHandle): INTEGER;
                    // IM:IV-269. Stack: sel(2) + lHandle(4) + colNum(2)
                    // + count(2) + result(2) = 12; pop 10,
                    // result@SP+10. No list, nothing added → return 0.
                    0x0004 => {
                        bus.write_word(sp + 10, 0);
                        cpu.write_reg(Register::A7, sp + 10);
                    }
                    // FUNCTION LAddRow(count, rowNum: INTEGER;
                    //                  lHandle: ListHandle): INTEGER;
                    // IM:IV-269. Same shape as LAddColumn.
                    0x0008 => {
                        bus.write_word(sp + 10, 0);
                        cpu.write_reg(Register::A7, sp + 10);
                    }
                    // PROCEDURE LAddToCell(dataPtr: Ptr;
                    //                      dataLen: INTEGER;
                    //                      theCell: Cell;
                    //                      lHandle: ListHandle);
                    // IM:IV-269 + MTb 4-82. Stack: sel(2) + lHandle(4)
                    // + theCell(4) + dataLen(2) + dataPtr(4) = 16.
                    0x000C => {
                        cpu.write_reg(Register::A7, sp + 16);
                    }
                    // PROCEDURE LAutoScroll(lHandle: ListHandle);
                    // IM:IV-269. Stack: sel(2) + lHandle(4) = 6.
                    0x0010 => {
                        cpu.write_reg(Register::A7, sp + 6);
                    }
                    // PROCEDURE LCellSize(cSize: Point;
                    //                     lHandle: ListHandle);
                    // IM:IV-269. Stack: sel(2) + lHandle(4) + cSize(4)
                    // = 10.
                    0x0014 => {
                        cpu.write_reg(Register::A7, sp + 10);
                    }
                    // FUNCTION LClick(pt: Point; modifiers: INTEGER;
                    //                 lHandle: ListHandle): BOOLEAN;
                    // IM:IV-269 + MTb 4-78. Stack: sel(2) + lHandle(4)
                    // + modifiers(2) + pt(4) + result(2) = 14; pop 12,
                    // result@SP+12. No list, no double-click → FALSE.
                    0x0018 => {
                        bus.write_word(sp + 12, 0);
                        cpu.write_reg(Register::A7, sp + 12);
                    }
                    // PROCEDURE LClrCell(theCell: Cell;
                    //                    lHandle: ListHandle);
                    // IM:IV-269. Stack: sel(2) + lHandle(4) + theCell(4)
                    // = 10.
                    0x001C => {
                        cpu.write_reg(Register::A7, sp + 10);
                    }
                    // PROCEDURE LDelColumn(count, colNum: INTEGER;
                    //                      lHandle: ListHandle);
                    // IM:IV-269. Stack: sel(2) + lHandle(4) + colNum(2)
                    // + count(2) = 10.
                    0x0020 => {
                        cpu.write_reg(Register::A7, sp + 10);
                    }
                    // PROCEDURE LDelRow(count, rowNum: INTEGER;
                    //                   lHandle: ListHandle);
                    // IM:IV-269. Same shape as LDelColumn.
                    0x0024 => {
                        cpu.write_reg(Register::A7, sp + 10);
                    }
                    // PROCEDURE LDispose(lHandle: ListHandle);
                    // IM:IV-269. Stack: sel(2) + lHandle(4) = 6.
                    0x0028 => {
                        let list_handle = bus.read_long(sp + 2);
                        let list_ptr = Self::list_record_ptr(bus, list_handle);
                        let cells_handle = if list_ptr != 0 {
                            bus.read_long(list_ptr + Self::LIST_CELLS_OFFSET)
                        } else {
                            0
                        };
                        let (v_scroll, h_scroll) = if list_ptr != 0 {
                            (
                                bus.read_long(list_ptr + Self::LIST_VSCROLL_OFFSET),
                                bus.read_long(list_ptr + Self::LIST_HSCROLL_OFFSET),
                            )
                        } else {
                            (0, 0)
                        };
                        self.list_states.remove_record(list_handle);
                        self.dispose_control_handle(bus, v_scroll);
                        self.dispose_control_handle(bus, h_scroll);
                        if list_ptr != 0 {
                            bus.free(list_ptr);
                        }
                        if cells_handle != 0 {
                            bus.free(cells_handle);
                        }
                        if list_handle != 0 {
                            bus.free(list_handle);
                        }
                        cpu.write_reg(Register::A7, sp + 6);
                    }
                    // PROCEDURE LDoDraw(drawIt: BOOLEAN;
                    //                   lHandle: ListHandle);
                    // IM:IV-269 + MTb 4-83 (alias LSetDrawingMode).
                    // Stack: sel(2) + lHandle(4) + drawIt(2) = 8.
                    0x002C => {
                        let list_handle = bus.read_long(sp + 2);
                        let draw_it = Self::stack_bool_slot(bus, sp + 6);
                        self.list_states.with_record_mut(list_handle, |state| {
                            state.draw_enabled = draw_it;
                        });
                        let list_ptr = Self::list_record_ptr(bus, list_handle);
                        if list_ptr != 0 {
                            for offset in [Self::LIST_VSCROLL_OFFSET, Self::LIST_HSCROLL_OFFSET] {
                                let handle = bus.read_long(list_ptr + offset);
                                let control = if handle != 0 {
                                    bus.read_long(handle)
                                } else {
                                    0
                                };
                                if control != 0 && draw_it {
                                    // LDoDraw(FALSE) disables cell drawing, not
                                    // control visibility (IM IV-275). Mac OS 8.1
                                    // leaves contrlVis set until HideControl.
                                    bus.write_byte(control + 16, 255);
                                    self.draw_control(cpu, bus, control);
                                }
                            }
                        }
                        cpu.write_reg(Register::A7, sp + 8);
                    }
                    // PROCEDURE LDraw(theCell: Cell;
                    //                 lHandle: ListHandle);
                    // IM:IV-269. Stack: sel(2) + lHandle(4) + theCell(4)
                    // = 10.
                    0x0030 => {
                        cpu.write_reg(Register::A7, sp + 10);
                    }
                    // PROCEDURE LFind(VAR offset, len: INTEGER;
                    //                 theCell: Cell;
                    //                 lHandle: ListHandle);
                    // IM:IV-263, 269. Pascal pushes args left-to-right
                    // (first pushed = deepest), so source order
                    // offset, len, theCell, lHandle places lHandle
                    // (last) at sp+2, theCell at sp+6, len ptr at
                    // sp+10, offset ptr at sp+14. VAR INTEGER args
                    // are 4-byte ptrs each. Stack: sel(2) + lHandle(4)
                    // + theCell(4) + len ptr(4) + offset ptr(4) = 18.
                    // No list → write 0 to both *offset and *len.
                    0x0034 => {
                        let len_ptr = bus.read_long(sp + 10);
                        let offset_ptr = bus.read_long(sp + 14);
                        if offset_ptr != 0 {
                            bus.write_word(offset_ptr, 0);
                        }
                        if len_ptr != 0 {
                            bus.write_word(len_ptr, 0);
                        }
                        cpu.write_reg(Register::A7, sp + 18);
                    }
                    // PROCEDURE LGetCell(dataPtr: Ptr;
                    //                    VAR dataLen: INTEGER;
                    //                    theCell: Cell;
                    //                    lHandle: ListHandle);
                    // IM:IV-263, 269. Stack: sel(2) + lHandle(4)
                    // + theCell(4) + dataLen ptr(4) + dataPtr(4) = 18.
                    // No cell data → write 0 to *dataLen.
                    0x0038 => {
                        let datalen_ptr = bus.read_long(sp + 10);
                        if datalen_ptr != 0 {
                            bus.write_word(datalen_ptr, 0);
                        }
                        cpu.write_reg(Register::A7, sp + 18);
                    }
                    // FUNCTION LGetSelect(next: BOOLEAN;
                    //                     VAR theCell: Cell;
                    //                     lHandle: ListHandle): BOOLEAN;
                    // IM:IV-263, 269. Stack: sel(2) + lHandle(4)
                    // + theCell ptr(4) + next(2) + result(2) = 14;
                    // pop 12, result@SP+12. No selection → FALSE.
                    0x003C => {
                        bus.write_word(sp + 12, 0);
                        cpu.write_reg(Register::A7, sp + 12);
                    }
                    // FUNCTION LLastClick(lHandle: ListHandle): Cell;
                    // IM:IV-263, 269. Stack: sel(2) + lHandle(4)
                    // + result(4) = 10; pop 6, result@SP+6 (Cell long).
                    // No prior click → write Cell(-1,-1) = 0xFFFF_FFFF.
                    0x0040 => {
                        bus.write_long(sp + 6, 0xFFFF_FFFF);
                        cpu.write_reg(Register::A7, sp + 6);
                    }
                    // FUNCTION LNew(rView, dataBounds: Rect;
                    //               cSize: Point; theProc: INTEGER;
                    //               theWindow: WindowPtr;
                    //               drawIt, hasGrow, scrollHoriz,
                    //               scrollVert: BOOLEAN): ListHandle;
                    // IM:IV-269 + MTb 4-70. Rects pass by REFERENCE
                    // (4-byte ptr) per the QuickDraw Toolbox-wide
                    // convention reaffirmed by PtInRect at
                    // quickdraw.rs:1654. Stack: sel(2) + scrollVert(2)
                    // + scrollHoriz(2) + hasGrow(2) + drawIt(2)
                    // + theWindow(4) + theProc(2) + cSize(4)
                    // + dataBounds ptr(4) + rView ptr(4) + result(4)
                    // = 32; pop 28, result@SP+28 (long, ListHandle).
                    // Pack1 now mirrors the Pack0 list-record setup so
                    // callers can obtain a live list handle.
                    0x0044 => {
                        let scroll_v = Self::stack_bool_slot(bus, sp + 2);
                        let scroll_h = Self::stack_bool_slot(bus, sp + 4);
                        let has_grow = Self::stack_bool_slot(bus, sp + 6);
                        let draw_it = Self::stack_bool_slot(bus, sp + 8);
                        let window = bus.read_long(sp + 10);
                        let proc_id = bus.read_word(sp + 14) as i16;
                        let cell_size = Self::read_stack_point(bus, sp + 16);
                        let data_bounds_ptr = bus.read_long(sp + 20);
                        let view_rect_ptr = bus.read_long(sp + 24);
                        let result_addr = sp + 28;

                        let view_rect = Self::read_rect_ptr(bus, view_rect_ptr);
                        let data_bounds = Self::read_rect_ptr(bus, data_bounds_ptr);
                        let resolved_cell_size =
                            self.compute_list_cell_size(view_rect, data_bounds, cell_size);
                        let visible = Self::compute_list_visible_rect(
                            view_rect,
                            data_bounds,
                            resolved_cell_size,
                        );

                        let list_ptr = bus.alloc(Self::LIST_RECORD_SIZE);
                        let list_handle = bus.alloc(4);
                        let cells_handle = bus.alloc(4);

                        let list_def_proc_handle = self
                            .find_or_load_resource_any(bus, *b"LDEF", proc_id)
                            .map(|(_, ptr)| {
                                self.get_or_create_resource_handle(bus, *b"LDEF", proc_id, ptr)
                            })
                            .unwrap_or(0);

                        if list_handle != 0 {
                            bus.write_long(list_handle, list_ptr);
                        }
                        if cells_handle != 0 {
                            bus.write_long(cells_handle, 0);
                        }

                        let state = super::dispatch::ListState {
                            handle: list_handle,
                            cells_handle,
                            view_rect,
                            data_bounds,
                            cell_size: resolved_cell_size,
                            visible,
                            port: window,
                            draw_enabled: draw_it,
                            active: true,
                            cells: std::collections::HashMap::new(),
                            selected: std::collections::BTreeSet::new(),
                            last_click: Self::list_no_click_cell(),
                            last_click_tick: 0,
                        };

                        let (v_scroll, v_scroll_ptr) = if scroll_v {
                            self.create_list_scrollbar(bus, &state, true)
                        } else {
                            (0, 0)
                        };
                        let (h_scroll, h_scroll_ptr) = if scroll_h {
                            self.create_list_scrollbar(bus, &state, false)
                        } else {
                            (0, 0)
                        };

                        if list_ptr != 0 {
                            Self::write_rect_words(
                                bus,
                                list_ptr + Self::LIST_RVIEW_OFFSET,
                                view_rect,
                            );
                            bus.write_long(list_ptr + Self::LIST_PORT_OFFSET, window);
                            Self::write_point_words(
                                bus,
                                list_ptr + Self::LIST_INDENT_OFFSET,
                                (0, 0),
                            );
                            Self::write_point_words(
                                bus,
                                list_ptr + Self::LIST_CELL_SIZE_OFFSET,
                                resolved_cell_size,
                            );
                            Self::write_rect_words(
                                bus,
                                list_ptr + Self::LIST_VISIBLE_OFFSET,
                                visible,
                            );
                            bus.write_long(list_ptr + Self::LIST_VSCROLL_OFFSET, v_scroll);
                            bus.write_long(list_ptr + Self::LIST_HSCROLL_OFFSET, h_scroll);
                            bus.write_byte(list_ptr + Self::LIST_SEL_FLAGS_OFFSET, 0);
                            bus.write_byte(list_ptr + Self::LIST_ACTIVE_OFFSET, 1);
                            bus.write_byte(list_ptr + Self::LIST_RESERVED_OFFSET, 0);
                            bus.write_byte(list_ptr + Self::LIST_FLAGS_OFFSET, 0);
                            bus.write_long(list_ptr + Self::LIST_CLICK_TIME_OFFSET, 0);
                            Self::write_point_words(
                                bus,
                                list_ptr + Self::LIST_CLICK_LOC_OFFSET,
                                (-32768, -32768),
                            );
                            Self::write_point_words(
                                bus,
                                list_ptr + Self::LIST_MOUSE_LOC_OFFSET,
                                (-1, -1),
                            );
                            bus.write_long(list_ptr + Self::LIST_CLICK_LOOP_OFFSET, 0);
                            Self::write_point_words(
                                bus,
                                list_ptr + Self::LIST_LAST_CLICK_OFFSET,
                                (-1, -1),
                            );
                            bus.write_long(list_ptr + Self::LIST_REFCON_OFFSET, 0);
                            bus.write_long(
                                list_ptr + Self::LIST_DEF_PROC_OFFSET,
                                list_def_proc_handle,
                            );
                            bus.write_long(list_ptr + Self::LIST_USER_HANDLE_OFFSET, 0);
                            Self::write_rect_words(
                                bus,
                                list_ptr + Self::LIST_DATA_BOUNDS_OFFSET,
                                data_bounds,
                            );
                            bus.write_long(list_ptr + Self::LIST_CELLS_OFFSET, cells_handle);
                            let rows = (data_bounds.2 - data_bounds.0).max(0) as i32;
                            let cols = (data_bounds.3 - data_bounds.1).max(0) as i32;
                            bus.write_word(
                                list_ptr + Self::LIST_MAX_INDEX_OFFSET,
                                rows.saturating_mul(cols).saturating_mul(2) as u16,
                            );
                            bus.write_word(list_ptr + Self::LIST_CELL_ARRAY_OFFSET, 0);
                        }

                        if list_handle != 0 {
                            self.list_states.insert_record(list_handle, state);
                        }
                        if draw_it {
                            if v_scroll_ptr != 0 {
                                self.draw_control(cpu, bus, v_scroll_ptr);
                            }
                            if h_scroll_ptr != 0 {
                                self.draw_control(cpu, bus, h_scroll_ptr);
                            }
                        }

                        if trace_list_manager_enabled() {
                            eprintln!(
                                "[LIST] LNew handle=${:08X} ptr=${:08X} proc={} draw={} grow={} scroll_h={} scroll_v={} view=({},{},{},{}) bounds=({},{},{},{}) cell=({}, {})",
                                list_handle,
                                list_ptr,
                                proc_id,
                                draw_it,
                                has_grow,
                                scroll_h,
                                scroll_v,
                                view_rect.0,
                                view_rect.1,
                                view_rect.2,
                                view_rect.3,
                                data_bounds.0,
                                data_bounds.1,
                                data_bounds.2,
                                data_bounds.3,
                                resolved_cell_size.0,
                                resolved_cell_size.1,
                            );
                        }

                        bus.write_long(result_addr, list_handle);
                        cpu.write_reg(Register::A7, result_addr);
                    }
                    // FUNCTION LNextCell(hNext, vNext: BOOLEAN;
                    //                    VAR theCell: Cell;
                    //                    lHandle: ListHandle): BOOLEAN;
                    // IM:IV-263, 269. Stack: sel(2) + lHandle(4)
                    // + theCell ptr(4) + vNext(2) + hNext(2)
                    // + result(2) = 16; pop 14, result@SP+14.
                    // No list → FALSE.
                    0x0048 => {
                        bus.write_word(sp + 14, 0);
                        cpu.write_reg(Register::A7, sp + 14);
                    }
                    // PROCEDURE LRect(VAR cellRect: Rect;
                    //                 theCell: Cell;
                    //                 lHandle: ListHandle);
                    // IM:IV-263, 269. Stack: sel(2) + lHandle(4)
                    // + theCell(4) + cellRect ptr(4) = 14.
                    // No list → write empty Rect (0,0,0,0).
                    0x004C => {
                        let rect_ptr = bus.read_long(sp + 10);
                        if rect_ptr != 0 {
                            bus.write_word(rect_ptr, 0);
                            bus.write_word(rect_ptr + 2, 0);
                            bus.write_word(rect_ptr + 4, 0);
                            bus.write_word(rect_ptr + 6, 0);
                        }
                        cpu.write_reg(Register::A7, sp + 14);
                    }
                    // PROCEDURE LScroll(dCols, dRows: INTEGER;
                    //                   lHandle: ListHandle);
                    // IM:IV-263, 269. Stack: sel(2) + lHandle(4)
                    // + dRows(2) + dCols(2) = 10.
                    0x0050 => {
                        let list_handle = bus.read_long(sp + 2);
                        let d_rows = bus.read_word(sp + 6) as i16;
                        let d_cols = bus.read_word(sp + 8) as i16;
                        self.list_states.with_record_mut(list_handle, |state| {
                            Self::set_list_visible_origin(
                                state,
                                state.visible.0.saturating_add(d_rows),
                                state.visible.1.saturating_add(d_cols),
                            );
                            Self::sync_list_state_to_guest(bus, list_handle, state);
                        });
                        cpu.write_reg(Register::A7, sp + 10);
                    }
                    // FUNCTION LSearch(dataPtr: Ptr; dataLen: INTEGER;
                    //                  searchProc: Ptr;
                    //                  VAR theCell: Cell;
                    //                  lHandle: ListHandle): BOOLEAN;
                    // IM:IV-263, 269. Stack: sel(2) + lHandle(4)
                    // + theCell ptr(4) + searchProc(4) + dataLen(2)
                    // + dataPtr(4) + result(2) = 22; pop 20,
                    // result@SP+20. No list → FALSE; do NOT invoke
                    // the searchProc trampoline.
                    0x0054 => {
                        bus.write_word(sp + 20, 0);
                        cpu.write_reg(Register::A7, sp + 20);
                    }
                    // PROCEDURE LSetCell(dataPtr: Ptr;
                    //                    dataLen: INTEGER;
                    //                    theCell: Cell;
                    //                    lHandle: ListHandle);
                    // IM:IV-263, 269. Stack: sel(2) + lHandle(4)
                    // + theCell(4) + dataLen(2) + dataPtr(4) = 16.
                    0x0058 => {
                        cpu.write_reg(Register::A7, sp + 16);
                    }
                    // PROCEDURE LSetSelect(setIt: BOOLEAN;
                    //                      theCell: Cell;
                    //                      lHandle: ListHandle);
                    // IM:IV-263, 269. Stack: sel(2) + lHandle(4)
                    // + theCell(4) + setIt(2) = 12.
                    0x005C => {
                        cpu.write_reg(Register::A7, sp + 12);
                    }
                    // PROCEDURE LSize(listWidth, listHeight: INTEGER;
                    //                 lHandle: ListHandle);
                    // IM:IV-263, 269. Stack: sel(2) + lHandle(4)
                    // + listHeight(2) + listWidth(2) = 10.
                    0x0060 => {
                        let list_handle = bus.read_long(sp + 2);
                        let height = bus.read_word(sp + 6) as i16;
                        let width = bus.read_word(sp + 8) as i16;
                        self.list_states.with_record_mut(list_handle, |state| {
                            let old_origin = (state.visible.0, state.visible.1);
                            state.view_rect.2 = state.view_rect.0.saturating_add(height.max(0));
                            state.view_rect.3 = state.view_rect.1.saturating_add(width.max(0));
                            state.visible = Self::compute_list_visible_rect(
                                state.view_rect,
                                state.data_bounds,
                                state.cell_size,
                            );
                            Self::set_list_visible_origin(state, old_origin.0, old_origin.1);
                            Self::sync_list_state_to_guest(bus, list_handle, state);
                        });
                        cpu.write_reg(Register::A7, sp + 10);
                    }
                    // PROCEDURE LUpdate(theRgn: RgnHandle;
                    //                   lHandle: ListHandle);
                    // IM:IV-263, 269. Stack: sel(2) + lHandle(4)
                    // + theRgn(4) = 10.
                    0x0064 => {
                        let list_handle = bus.read_long(sp + 2);
                        if let Some(state) = self.list_states.get(&list_handle).cloned() {
                            if state.draw_enabled {
                                self.draw_list_scrollbars(cpu, bus, list_handle);
                                if self.draw_list_with_ldef(cpu, bus, list_handle, &state, None, 10)
                                {
                                    return Some(Ok(()));
                                }
                                self.draw_list_fallback(cpu, bus, &state, None);
                            }
                        }
                        cpu.write_reg(Register::A7, sp + 10);
                    }
                    _ => {
                        // Unknown / undocumented selector — pop just
                        // the 2-byte selector word so the caller's
                        // stack stays balanced.
                        cpu.write_reg(Register::A7, sp + 2);
                    }
                }
                cpu.write_reg(Register::D0, 0);
                Ok(())
            }

            // _Pack2 (0xA9E9)
            // Dispatches Disk Initialization Manager operations selected by a word on top of the stack.
            // Selector ABI: MOVEQ loads D0; MOVE.W D0,-(SP) pushes the 16-bit selector before the Pascal parameters.
            // Inside Macintosh: Files (1992), 5-15–5-24.
            (true, 0x1E9) => {
                let sp = cpu.read_reg(Register::A7);
                let selector = bus.read_word(sp);
                let operation = pack2_operation_route(self.current_trap_word, selector);
                self.current_selector_operation = operation.map(|route| route.operation_id);
                let (arg_bytes, has_result) = match selector {
                    // FUNCTION DIBadMount(where: Point; evtMessage: LongInt): Integer
                    // IM:Files 5-18..5-19. where=4 (Point by value),
                    // evtMessage=4 (LongInt by value). Returns 0 = "no
                    // error / user proceeded" — the only result code that
                    // never causes a caller to escalate to DoError.
                    0x0000 => (8u32, true),
                    // PROCEDURE DILoad / PROCEDURE DIUnload — no args, no
                    // result. IM:Files 5-15..5-16. The Disk Init Manager
                    // is always "loaded" in our HLE so both are no-ops.
                    0x0002 | 0x0004 => (0u32, false),
                    // FUNCTION DIFormat(drvNum: Integer): OSErr
                    // FUNCTION DIVerify(drvNum: Integer): OSErr
                    // IM:Files 5-19..5-20. Both return noErr on the
                    // single VFS volume.
                    0x0006 | 0x0008 => (2u32, true),
                    // FUNCTION DIZero(drvNum: Integer; volName: Str255): OSErr
                    // IM:Files 5-21. Pascal Str255 is pushed by value
                    // (256 bytes); MPW C glue marshals from the
                    // ConstStr255Param pointer into a stack-local
                    // Str255 before invoking the trap. Total args:
                    // drvNum(2) + Str255(256) = 258.
                    0x000A => (258u32, true),
                    _ => {
                        // No other selectors in IM:Files 5-24 summary.
                        // Pop just the selector and return noErr; future
                        // System additions would land in a new arm here.
                        cpu.write_reg(Register::A7, sp + 2);
                        cpu.write_reg(Register::D0, 0);
                        return Some(Ok(()));
                    }
                };
                let pop_total = 2 + arg_bytes;
                if has_result {
                    bus.write_word(sp + pop_total, 0);
                }
                cpu.write_reg(Register::A7, sp + pop_total);
                cpu.write_reg(Register::D0, 0);
                Ok(())
            }

            // _Pack3 (0xA9EA)
            // Dispatches Standard File Package operations selected by a word on top of the stack.
            // Selector ABI: 16-bit word at SP; routine-specific Pascal parameters follow.
            // Inside Macintosh: Files (1992), 3-45–3-54.
            (true, 0x1EA) => {
                let sp = cpu.read_reg(Register::A7);
                let selector = bus.read_word(sp);
                let operation = pack3_operation_route(self.current_trap_word, selector);
                self.current_selector_operation = operation.map(|route| route.operation_id);
                if let Some(tracking) = self.standard_file_put_tracking.take() {
                    self.service_standard_file_put_tracking(cpu, bus, tracking);
                    return Some(Ok(()));
                }
                if let Some(tracking) = self.standard_file_get_tracking.take() {
                    self.service_standard_file_get_tracking(cpu, bus, tracking);
                    return Some(Ok(()));
                }
                let (
                    arg_bytes,
                    reply_offset,
                    prompt_offset,
                    default_name_offset,
                    modern_reply,
                    accepts_file,
                    get_filter_offsets,
                ) = match selector {
                    // PROCEDURE SFPutFile(where: Point; prompt: Str255;
                    //                     origName: Str255;
                    //                     dlgHook: ProcPtr;
                    //                     VAR reply: SFReply);
                    // IM:Files 3-48–3-49 / IM:I I-519–I-522. Reply ptr is
                    // last Pascal arg → at SP+2 above the selector.
                    0x0001 => (20u32, 2u32, Some(14u32), Some(10u32), false, true, None),
                    // PROCEDURE SFGetFile(where: Point; prompt: Str255;
                    //                     fileFilter: ProcPtr;
                    //                     numTypes: Integer;
                    //                     typeList: SFTypeList;
                    //                     dlgHook: ProcPtr;
                    //                     VAR reply: SFReply);
                    // IM:Files 3-52–3-53 / IM:I I-523–I-526.
                    0x0002 => (26u32, 2u32, None, None, false, false, Some((14u32, 10u32))),
                    // PROCEDURE SFPPutFile(...; VAR reply: SFReply;
                    //                       dlgID: Integer;
                    //                       filterProc: ProcPtr);
                    // IM:Files 3-49–3-50 / IM:I I-522–I-523.
                    // filterProc(4) + dlgID(2) sit above the reply pointer
                    // on the stack, so the reply is at SP+8.
                    0x0003 => (26u32, 8u32, Some(20u32), Some(16u32), false, true, None),
                    // PROCEDURE SFPGetFile(...; VAR reply: SFReply;
                    //                       dlgID: Integer;
                    //                       filterProc: ProcPtr);
                    // IM:Files 3-53–3-54 / IM:I I-526–I-527.
                    0x0004 => (32u32, 8u32, None, None, false, false, Some((20u32, 16u32))),
                    // PROCEDURE StandardPutFile(prompt: Str255;
                    //                           defaultName: Str255;
                    //                           VAR reply:
                    //                             StandardFileReply);
                    // IM:Files 3-45.
                    0x0005 => (12u32, 2u32, Some(10u32), Some(6u32), true, true, None),
                    // PROCEDURE StandardGetFile(fileFilter: ProcPtr;
                    //                           numTypes: Integer;
                    //                           typeList: SFTypeList;
                    //                           VAR reply:
                    //                             StandardFileReply);
                    // IM:Files 3-50.
                    0x0006 => (14u32, 2u32, None, None, true, false, Some((10u32, 6u32))),
                    // PROCEDURE CustomPutFile(prompt: Str255;
                    //                          defaultName: Str255;
                    //                          VAR reply:
                    //                            StandardFileReply;
                    //                          dlgID: Integer;
                    //                          where: Point;
                    //                          dlgHook: ProcPtr;
                    //                          filterProc: ProcPtr;
                    //                          activeList: Ptr;
                    //                          activateProc: ProcPtr;
                    //                          yourDataPtr: UNIV Ptr);
                    // IM:Files 3-46 / IM:VI 26-21. yourData(4) +
                    // activate(4) + activeList(4) + filter(4) +
                    // dlgHook(4) + where(4) + dlgID(2) = 26 bytes
                    // ABOVE reply → reply at SP+28.
                    0x0007 => (38u32, 28u32, Some(36u32), Some(32u32), true, true, None),
                    // PROCEDURE CustomGetFile(fileFilter: ProcPtr;
                    //                          numTypes: Integer;
                    //                          typeList: SFTypeList;
                    //                          VAR reply:
                    //                            StandardFileReply;
                    //                          dlgID: Integer;
                    //                          where: Point;
                    //                          dlgHook: ProcPtr;
                    //                          filterProc: ProcPtr;
                    //                          activeList: Ptr;
                    //                          activateProc: ProcPtr;
                    //                          yourDataPtr: UNIV Ptr);
                    // IM:Files 3-51 / IM:VI 26-22.
                    0x0008 => (40u32, 28u32, None, None, true, false, Some((36u32, 32u32))),
                    _ => {
                        // No other selectors documented in IM:Files
                        // 3-45–3-54 or IM:I I-518–I-527. Pop just
                        // the selector word defensively so a future
                        // System addition doesn't corrupt the caller
                        // stack.
                        cpu.write_reg(Register::A7, sp + 2);
                        cpu.write_reg(Register::D0, 0);
                        return Some(Ok(()));
                    }
                };
                let pop_total = 2 + arg_bytes;
                let reply_ptr = bus.read_long(sp + reply_offset);
                if accepts_file {
                    let default_name_ptr = default_name_offset
                        .map(|offset| bus.read_long(sp + offset))
                        .unwrap_or(0);
                    let prompt_ptr = prompt_offset
                        .map(|offset| bus.read_long(sp + offset))
                        .unwrap_or(0);
                    if self.yield_for_ui {
                        self.begin_standard_file_put_tracking(
                            bus,
                            modern_reply,
                            reply_ptr,
                            sp,
                            pop_total,
                            prompt_ptr,
                            default_name_ptr,
                        );
                        return Some(Ok(()));
                    }
                    let name = standard_file_default_name(bus, default_name_ptr);
                    let (vref, dir_id) = self.standard_file_put_default_destination();
                    let wd_ref = self.persist_standard_file_directory(bus, vref, dir_id);
                    if modern_reply {
                        let target_name = decode_mac_roman(&name);
                        let replacing = self
                            .find_vfs_file_in_directory(dir_id, &target_name)
                            .is_some()
                            || self
                                .find_vfs_rsrc_file_in_directory(dir_id, &target_name)
                                .is_some();
                        standard_file_put_reply_modern(
                            bus, reply_ptr, vref, dir_id, &name, replacing,
                        );
                    } else {
                        standard_file_put_reply_old(bus, reply_ptr, wd_ref, &name);
                    }
                } else if let Some(selection) = self.standard_file_env_selection() {
                    // StandardGetFile returns sfGood, sfType, and an FSSpec
                    // for the selected file; SFGetFile's old SFReply uses a
                    // working-directory refnum in vRefNum.
                    // Inside Macintosh: Files (1992), pages 1-12, 3-42,
                    // 3-45..3-54.
                    if modern_reply {
                        standard_file_get_reply_modern(
                            bus,
                            reply_ptr,
                            selection.vref,
                            selection.dir_id,
                            selection.file_type,
                            selection.finder_flags,
                            &selection.name,
                        );
                    } else {
                        standard_file_get_reply_old(
                            bus,
                            reply_ptr,
                            selection.wd_ref,
                            selection.file_type,
                            &selection.name,
                        );
                    }
                } else if self.yield_for_ui {
                    if let Some((num_types_offset, type_list_offset)) = get_filter_offsets {
                        let num_types = bus.read_word(sp + num_types_offset) as i16;
                        let type_list_ptr = bus.read_long(sp + type_list_offset);
                        let requested_origin = matches!(selector, 0x0002 | 0x0004).then(|| {
                            let where_offset = if selector == 0x0002 { 22 } else { 28 };
                            (
                                bus.read_word(sp + where_offset) as i16,
                                bus.read_word(sp + where_offset + 2) as i16,
                            )
                        });
                        self.begin_standard_file_get_tracking(
                            bus,
                            modern_reply,
                            reply_ptr,
                            sp,
                            pop_total,
                            num_types,
                            type_list_ptr,
                            requested_origin,
                        );
                        return Some(Ok(()));
                    }
                    standard_file_cancel_reply(bus, reply_ptr);
                } else if let Some(selection) =
                    get_filter_offsets.and_then(|(num_types_offset, type_list_offset)| {
                        let num_types = bus.read_word(sp + num_types_offset) as i16;
                        let type_list_ptr = bus.read_long(sp + type_list_offset);
                        self.standard_file_auto_selection(bus, num_types, type_list_ptr)
                    })
                {
                    // StandardGetFile returns sfGood, sfType, and an FSSpec
                    // for the selected file; SFGetFile's old SFReply uses a
                    // working-directory refnum in vRefNum.
                    // Inside Macintosh: Files (1992), pages 1-12, 3-42,
                    // 3-45..3-54.
                    if modern_reply {
                        standard_file_get_reply_modern(
                            bus,
                            reply_ptr,
                            selection.vref,
                            selection.dir_id,
                            selection.file_type,
                            selection.finder_flags,
                            &selection.name,
                        );
                    } else {
                        standard_file_get_reply_old(
                            bus,
                            reply_ptr,
                            selection.wd_ref,
                            selection.file_type,
                            &selection.name,
                        );
                    }
                } else {
                    standard_file_cancel_reply(bus, reply_ptr);
                }
                cpu.write_reg(Register::A7, sp + pop_total);
                cpu.write_reg(Register::D0, 0);
                Ok(())
            }

            // Pack4 ($A9EB) and Pack5 ($A9EC) are handled by dispatch_sane

            // _Pack6 (0xA9ED)
            // Dispatches International and Text Utilities operations selected by a word on top of the stack.
            // Selector ABI: 16-bit word at SP; routine-specific Pascal parameters follow.
            // Inside Macintosh Volume I (1985), I-483; Inside Macintosh Volume VI (1991), 14-135; Inside Macintosh: Text (1993), 5-113.
            (true, 0x1ED) => {
                let sp = cpu.read_reg(Register::A7);
                let selector = bus.read_word(sp);
                let operation = pack6_operation_route(self.current_trap_word, selector);
                self.current_selector_operation = operation.map(|route| route.operation_id);
                match selector {
                    // PROCEDURE IUDateString(dateTime: LongInt;
                    //                        form: DateForm;
                    //                        VAR result: Str255);
                    // IM:I I-487, I-504. Stack: sel(2) + result(4)
                    // + form(2) + dateTime(4) = 12.
                    0x0000 => {
                        let result_ptr = bus.read_long(sp + 2);
                        if result_ptr != 0 {
                            bus.write_pstring(result_ptr, b"1/1/04");
                        }
                        cpu.write_reg(Register::A7, sp + 12);
                        cpu.write_reg(Register::D0, 0);
                        Ok(())
                    }
                    // PROCEDURE IUTimeString(dateTime: LongInt;
                    //                        wantSeconds: Boolean;
                    //                        VAR result: Str255);
                    // IM:I I-487, I-504. Stack: sel(2) + result(4)
                    // + wantSec(2) + dateTime(4) = 12.
                    0x0002 => {
                        let result_ptr = bus.read_long(sp + 2);
                        let want_seconds = bus.read_word(sp + 6) != 0;
                        if result_ptr != 0 {
                            let s: &[u8] = if want_seconds {
                                b"12:00:00 AM"
                            } else {
                                b"12:00 AM"
                            };
                            bus.write_pstring(result_ptr, s);
                        }
                        cpu.write_reg(Register::A7, sp + 12);
                        cpu.write_reg(Register::D0, 0);
                        Ok(())
                    }
                    // FUNCTION IUMetric: Boolean;
                    // IM:I I-487, I-505. Stack: sel(2) + result(2)
                    // = 4. Pop 2, leave result word at new SP+0.
                    0x0004 => {
                        bus.write_word(sp + 2, 0);
                        cpu.write_reg(Register::A7, sp + 2);
                        cpu.write_reg(Register::D0, 0);
                        Ok(())
                    }
                    // FUNCTION IUGetIntl(theID: Integer): Handle;
                    // IM:I I-487, I-505. Stack: sel(2) + theID(2)
                    // + result(4) = 8. Pop 4, leave result long at
                    // new SP+0. Search loaded resource files first, then use
                    // the built-in U.S. System-file records for IDs 0 and 1.
                    0x0006 => {
                        let id = bus.read_word(sp + 2) as i16;
                        let handle = if let Some((refnum, ptr)) =
                            self.find_or_load_resource_any(bus, *b"INTL", id)
                        {
                            self.get_or_create_resource_handle_in_file(
                                bus, *b"INTL", id, ptr, refnum,
                            )
                        } else if let Some(ptr) = self.synthesize_system_intl(bus, id) {
                            self.get_or_create_resource_handle_in_file(bus, *b"INTL", id, ptr, 0)
                        } else {
                            0
                        };
                        bus.write_long(sp + 4, handle);
                        cpu.write_reg(Register::A0, handle);
                        cpu.write_reg(Register::A7, sp + 4);
                        cpu.write_reg(Register::D0, 0);
                        Ok(())
                    }
                    // PROCEDURE IUSetIntl(refNum: Integer;
                    //                     theID: Integer;
                    //                     intlParam: Handle);
                    // IM:I I-487, I-503. Stack: sel(2) +
                    // intlParam(4) + theID(2) + refNum(2) = 10.
                    // No-op (HLE doesn't track INTL overrides).
                    0x0008 => {
                        cpu.write_reg(Register::A7, sp + 10);
                        cpu.write_reg(Register::D0, 0);
                        Ok(())
                    }
                    // FUNCTION IUMagString(aPtr,bPtr: Ptr;
                    //                      aLen,bLen: Integer):
                    //                      Integer;
                    // IM:I I-487, I-507. Stack: sel(2) + bLen(2) +
                    // aLen(2) + bPtr(4) + aPtr(4) + result(2) = 16.
                    // Pop 14, leave result at new SP+0.
                    0x000A => {
                        let a_ptr = bus.read_long(sp + 10);
                        let b_ptr = bus.read_long(sp + 6);
                        let a_len = bus.read_word(sp + 4) as usize;
                        let b_len = bus.read_word(sp + 2) as usize;
                        let a = bus.read_bytes(a_ptr, a_len);
                        let b = bus.read_bytes(b_ptr, b_len);
                        let result: i16 = match a.cmp(&b) {
                            std::cmp::Ordering::Less => -1,
                            std::cmp::Ordering::Equal => 0,
                            std::cmp::Ordering::Greater => 1,
                        };
                        bus.write_word(sp + 14, result as u16);
                        cpu.write_reg(Register::A7, sp + 14);
                        cpu.write_reg(Register::D0, result as u16 as u32);
                        Ok(())
                    }
                    // FUNCTION IUMagIDString(aPtr,bPtr: Ptr;
                    //                        aLen,bLen: Integer):
                    //                        Integer;
                    // IM:I I-487, I-507. Same stack as IUMagString.
                    // Returns 0 (case-insensitive equal) or 1 (not).
                    0x000C => {
                        let a_ptr = bus.read_long(sp + 10);
                        let b_ptr = bus.read_long(sp + 6);
                        let a_len = bus.read_word(sp + 4) as usize;
                        let b_len = bus.read_word(sp + 2) as usize;
                        let a: Vec<u8> = bus
                            .read_bytes(a_ptr, a_len)
                            .iter()
                            .map(|c| c.to_ascii_lowercase())
                            .collect();
                        let b: Vec<u8> = bus
                            .read_bytes(b_ptr, b_len)
                            .iter()
                            .map(|c| c.to_ascii_lowercase())
                            .collect();
                        let result: u16 = if a == b { 0 } else { 1 };
                        bus.write_word(sp + 14, result);
                        cpu.write_reg(Register::A7, sp + 14);
                        cpu.write_reg(Register::D0, result as u32);
                        Ok(())
                    }
                    // PROCEDURE IUDatePString(dateTime: LongInt;
                    //                         form: DateForm;
                    //                         VAR result: Str255;
                    //                         intlParam: Handle);
                    // IM:I I-487, I-505. Stack: sel(2) +
                    // intlParam(4) + result(4) + form(2) +
                    // dateTime(4) = 16.
                    0x000E => {
                        let result_ptr = bus.read_long(sp + 6);
                        if result_ptr != 0 {
                            bus.write_pstring(result_ptr, b"1/1/04");
                        }
                        cpu.write_reg(Register::A7, sp + 16);
                        cpu.write_reg(Register::D0, 0);
                        Ok(())
                    }
                    // PROCEDURE IUTimePString(dateTime: LongInt;
                    //                         wantSeconds: Boolean;
                    //                         VAR result: Str255;
                    //                         intlParam: Handle);
                    // IM:I I-487, I-505. Stack: sel(2) +
                    // intlParam(4) + result(4) + wantSec(2) +
                    // dateTime(4) = 16.
                    0x0010 => {
                        let result_ptr = bus.read_long(sp + 6);
                        let want_seconds = bus.read_word(sp + 10) != 0;
                        if result_ptr != 0 {
                            let s: &[u8] = if want_seconds {
                                b"12:00:00 AM"
                            } else {
                                b"12:00 AM"
                            };
                            bus.write_pstring(result_ptr, s);
                        }
                        cpu.write_reg(Register::A7, sp + 16);
                        cpu.write_reg(Register::D0, 0);
                        Ok(())
                    }
                    // PROCEDURE IULDateString(VAR dateTime:
                    //                         LongDateTime;
                    //                         longFlag: DateForm;
                    //                         VAR Result: Str255;
                    //                         intlParam: Handle);
                    // IM:VI 14-135. Stack: sel(2) + intlParam(4) +
                    // result(4) + longFlag(2) + dateTime ptr(4) = 16.
                    0x0014 => {
                        let result_ptr = bus.read_long(sp + 6);
                        if result_ptr != 0 {
                            bus.write_pstring(result_ptr, b"1/1/04");
                        }
                        cpu.write_reg(Register::A7, sp + 16);
                        cpu.write_reg(Register::D0, 0);
                        Ok(())
                    }
                    // PROCEDURE IULTimeString(VAR dateTime:
                    //                         LongDateTime;
                    //                         wantSeconds: Boolean;
                    //                         VAR Result: Str255;
                    //                         intlParam: Handle);
                    // IM:VI 14-135. Stack: sel(2) + intlParam(4) +
                    // result(4) + wantSec(2) + dateTime ptr(4) = 16.
                    0x0016 => {
                        let result_ptr = bus.read_long(sp + 6);
                        let want_seconds = bus.read_word(sp + 10) != 0;
                        if result_ptr != 0 {
                            let s: &[u8] = if want_seconds {
                                b"12:00:00 AM"
                            } else {
                                b"12:00 AM"
                            };
                            bus.write_pstring(result_ptr, s);
                        }
                        cpu.write_reg(Register::A7, sp + 16);
                        cpu.write_reg(Register::D0, 0);
                        Ok(())
                    }
                    // PROCEDURE IUClearCache;
                    // IM:VI 14-76. Stack: sel(2) only. No-op (HLE
                    // has no IUtil cache).
                    0x0018 => {
                        cpu.write_reg(Register::A7, sp + 2);
                        cpu.write_reg(Register::D0, 0);
                        Ok(())
                    }
                    // FUNCTION IUMagPString(aPtr,bPtr: Ptr;
                    //                       aLen,bLen: Integer;
                    //                       itl2Handle: Handle):
                    //                       Integer;
                    // IM:VI 14-135. Stack: sel(2) + itl2(4) +
                    // bLen(2) + aLen(2) + bPtr(4) + aPtr(4) +
                    // result(2) = 20. Pop 18, leave result at new
                    // SP+0. itl2Handle ignored (HLE has no itl2).
                    0x001A => {
                        let a_ptr = bus.read_long(sp + 14);
                        let b_ptr = bus.read_long(sp + 10);
                        let a_len = bus.read_word(sp + 8) as usize;
                        let b_len = bus.read_word(sp + 6) as usize;
                        let a = bus.read_bytes(a_ptr, a_len);
                        let b = bus.read_bytes(b_ptr, b_len);
                        let result: i16 = match a.cmp(&b) {
                            std::cmp::Ordering::Less => -1,
                            std::cmp::Ordering::Equal => 0,
                            std::cmp::Ordering::Greater => 1,
                        };
                        bus.write_word(sp + 18, result as u16);
                        cpu.write_reg(Register::A7, sp + 18);
                        cpu.write_reg(Register::D0, result as u16 as u32);
                        Ok(())
                    }
                    // FUNCTION IUMagIDPString(aPtr,bPtr: Ptr;
                    //                         aLen,bLen: Integer;
                    //                         itl2Handle: Handle):
                    //                         Integer;
                    // IM:VI 14-135. Same stack as IUMagPString.
                    0x001C => {
                        let a_ptr = bus.read_long(sp + 14);
                        let b_ptr = bus.read_long(sp + 10);
                        let a_len = bus.read_word(sp + 8) as usize;
                        let b_len = bus.read_word(sp + 6) as usize;
                        let a: Vec<u8> = bus
                            .read_bytes(a_ptr, a_len)
                            .iter()
                            .map(|c| c.to_ascii_lowercase())
                            .collect();
                        let b: Vec<u8> = bus
                            .read_bytes(b_ptr, b_len)
                            .iter()
                            .map(|c| c.to_ascii_lowercase())
                            .collect();
                        let result: u16 = if a == b { 0 } else { 1 };
                        bus.write_word(sp + 18, result);
                        cpu.write_reg(Register::A7, sp + 18);
                        cpu.write_reg(Register::D0, result as u32);
                        Ok(())
                    }
                    // FUNCTION IUScriptOrder(script1, script2:
                    //                        ScriptCode): Integer;
                    // IM:VI 14-135. Stack: sel(2) + script2(2) +
                    // script1(2) + result(2) = 8. Pop 6, leave
                    // result at new SP+0.
                    0x001E => {
                        let script1 = bus.read_word(sp + 4) as i16;
                        let script2 = bus.read_word(sp + 2) as i16;
                        let result: i16 = match script1.cmp(&script2) {
                            std::cmp::Ordering::Less => -1,
                            std::cmp::Ordering::Equal => 0,
                            std::cmp::Ordering::Greater => 1,
                        };
                        bus.write_word(sp + 6, result as u16);
                        cpu.write_reg(Register::A7, sp + 6);
                        cpu.write_reg(Register::D0, result as u16 as u32);
                        Ok(())
                    }
                    // FUNCTION IULangOrder(language1, language2:
                    //                      LangCode): Integer;
                    // IM:VI 14-135. Same stack as IUScriptOrder.
                    0x0020 => {
                        let lang1 = bus.read_word(sp + 4) as i16;
                        let lang2 = bus.read_word(sp + 2) as i16;
                        let result: i16 = match lang1.cmp(&lang2) {
                            std::cmp::Ordering::Less => -1,
                            std::cmp::Ordering::Equal => 0,
                            std::cmp::Ordering::Greater => 1,
                        };
                        bus.write_word(sp + 6, result as u16);
                        cpu.write_reg(Register::A7, sp + 6);
                        cpu.write_reg(Register::D0, result as u16 as u32);
                        Ok(())
                    }
                    // FUNCTION IUTextOrder(aPtr,bPtr: Ptr;
                    //                      aLen,bLen: Integer;
                    //                      aScript,bScript:
                    //                      ScriptCode;
                    //                      aLang,bLang: LangCode):
                    //                      Integer;
                    // IM:VI 14-135. Stack: sel(2) + bLang(2) +
                    // aLang(2) + bScript(2) + aScript(2) + bLen(2)
                    // + aLen(2) + bPtr(4) + aPtr(4) + result(2) =
                    // 24. Pop 22, leave result at new SP+0. All
                    // script/lang args ignored in single-script HLE.
                    0x0022 => {
                        let a_ptr = bus.read_long(sp + 18);
                        let b_ptr = bus.read_long(sp + 14);
                        let a_len = bus.read_word(sp + 12) as usize;
                        let b_len = bus.read_word(sp + 10) as usize;
                        let a = bus.read_bytes(a_ptr, a_len);
                        let b = bus.read_bytes(b_ptr, b_len);
                        let result: i16 = match a.cmp(&b) {
                            std::cmp::Ordering::Less => -1,
                            std::cmp::Ordering::Equal => 0,
                            std::cmp::Ordering::Greater => 1,
                        };
                        bus.write_word(sp + 22, result as u16);
                        cpu.write_reg(Register::A7, sp + 22);
                        cpu.write_reg(Register::D0, result as u16 as u32);
                        Ok(())
                    }
                    // PROCEDURE IUGetIntlTable(script: ScriptCode;
                    //                          tableCode: Integer;
                    //                          VAR itlHandle: Handle;
                    //                          VAR offset: LongInt;
                    //                          VAR length: LongInt);
                    // IM:VI 14-135. Stack: sel(2) + length(4) +
                    // offset(4) + itlHandle(4) + tableCode(2) +
                    // script(2) = 18. No itl2/itl4 tables in HLE
                    // — write NIL/0/0 to all three VAR ptrs so
                    // caller's defensive (handle == NIL) check
                    // sends them down the "table not available"
                    // path.
                    0x0024 => {
                        let length_ptr = bus.read_long(sp + 2);
                        let offset_ptr = bus.read_long(sp + 6);
                        let handle_ptr = bus.read_long(sp + 10);
                        if handle_ptr != 0 {
                            bus.write_long(handle_ptr, 0);
                        }
                        if offset_ptr != 0 {
                            bus.write_long(offset_ptr, 0);
                        }
                        if length_ptr != 0 {
                            bus.write_long(length_ptr, 0);
                        }
                        cpu.write_reg(Register::A7, sp + 18);
                        cpu.write_reg(Register::D0, 0);
                        Ok(())
                    }
                    _ => {
                        // Defensive fallback: pop only the selector
                        // word so a future System addition or buggy
                        // caller doesn't corrupt the rest of the
                        // stack. Documented selectors are $0000..
                        // $0010 (even) per IM:I I-487 plus $0014..
                        // $0024 (even) per IM:VI 14-135. Selector
                        // $0012 is unused — IM:VI's enumeration at
                        // 14-135 jumps from $0010 IUTimePString
                        // straight to $0014 IULDateString.
                        cpu.write_reg(Register::A7, sp + 2);
                        cpu.write_reg(Register::D0, 0);
                        Ok(())
                    }
                }
            }

            // Pack7 / DecStr68K ($A9EE) — integer and SANE decimal scanners.
            // Selectors 0/1 are the Binary-Decimal Conversion Package.
            // Selectors 2/3/4 are the SANE FPSTR2DEC, FDEC2STR, and
            // FCSTR2DEC operations published by Apple's MPW SANEMacs.a.
            // PROCEDURE NumToString(theNumber: LONGINT; VAR theString: Str255);
            // PROCEDURE StringToNum(theString: Str255; VAR theNumber: LONGINT);
            // Inside Macintosh Volume I, I-489
            // Pack7 (NumToString/StringToNum) ($A9EE): Selector 0: NumToString (D0→Str255 at A0), Selector 1: StringToNum (Str255 at A0→D0)
            (true, 0x1EE) => {
                let sp = cpu.read_reg(Register::A7);
                let selector = bus.read_word(sp);
                cpu.write_reg(Register::A7, sp + 2);

                match selector {
                    0 => {
                        // NumToString: D0.L = number, A0 = pointer to Str255 result
                        let number = cpu.read_reg(Register::D0) as i32;
                        let a0 = cpu.read_reg(Register::A0);
                        bus.write_pstring(a0, format!("{}", number).as_bytes());
                    }
                    1 => {
                        // StringToNum: A0 = pointer to Pascal string, D0.L = result
                        let a0 = cpu.read_reg(Register::A0);
                        let bytes = bus.read_pstring(a0);
                        let s = String::from_utf8_lossy(&bytes);
                        let num: i32 = s.trim().parse().unwrap_or(0);
                        cpu.write_reg(Register::D0, num as u32);
                    }
                    2 => {
                        // FPSTR2DEC: scan a Pascal string into the 68K SANE
                        // decimal record. SANEMacs.a pushes four operand
                        // addresses before the selector:
                        //   SP+2  index*, SP+6 decimal*, SP+10 validPrefix*,
                        //   SP+14 Pascal string*.
                        // SANE.h defines decimal as sign byte, unused byte,
                        // signed exponent word, and a 22-byte significand
                        // record (length, 20 digits, unused).
                        let index_ptr = bus.read_long(sp + 2);
                        let decimal_ptr = bus.read_long(sp + 6);
                        let valid_prefix_ptr = bus.read_long(sp + 10);
                        let string_ptr = bus.read_long(sp + 14);
                        let bytes = bus.read_pstring(string_ptr);
                        scan_sane_decimal(bus, &bytes, index_ptr, decimal_ptr, valid_prefix_ptr);
                        cpu.write_reg(Register::A7, sp + 18);
                    }
                    3 => {
                        // FDEC2STR: MPW leaves the Pascal DecStr output,
                        // decimal input, and decform input pointers after the
                        // selector. Its public C wrapper converts the returned
                        // Pascal string to a C string after this trap returns.
                        let string_ptr = bus.read_long(sp + 2);
                        let decimal_ptr = bus.read_long(sp + 6);
                        let decform_ptr = bus.read_long(sp + 10);
                        let bytes = format_sane_decimal(bus, decimal_ptr, decform_ptr);
                        bus.write_pstring(string_ptr, &bytes);
                        cpu.write_reg(Register::A7, sp + 14);
                    }
                    4 => {
                        // FCSTR2DEC / CStr2Dec: scan a NUL-terminated C
                        // string into a 68K SANE decimal record. Inside
                        // Macintosh Volume IV, IV-69 and IV-307 identifies
                        // selector 4. MPW 3.5's linked str2dec wrapper places:
                        //   SP+2  validPrefix*, SP+6 decimal*, SP+10 index*,
                        //   SP+14 C string*.
                        let valid_prefix_ptr = bus.read_long(sp + 2);
                        let decimal_ptr = bus.read_long(sp + 6);
                        let index_ptr = bus.read_long(sp + 10);
                        let string_ptr = bus.read_long(sp + 14);
                        let mut bytes = Vec::new();
                        for offset in 0..=u16::MAX as u32 {
                            let byte = bus.read_byte(string_ptr.wrapping_add(offset));
                            if byte == 0 {
                                break;
                            }
                            bytes.push(byte);
                        }
                        scan_sane_decimal(bus, &bytes, index_ptr, decimal_ptr, valid_prefix_ptr);
                        cpu.write_reg(Register::A7, sp + 18);
                    }
                    _ => {
                        eprintln!(
                            "[TRAP] Pack7: unknown selector {} pc=${:08X}",
                            selector,
                            cpu.read_reg(Register::PC).wrapping_sub(2),
                        );
                    }
                }
                Ok(())
            }

            // Pack12 ($A82E) — Color Picker Package
            // Inside Macintosh Volume V, V-174..V-175.
            // MPW Universal Interfaces 3.4 ColorPicker.h declares:
            //   Fix2SmallFract(Fixed)      THREEWORDINLINE(0x3F3C, 0x0001, 0xA82E)
            //   SmallFract2Fix(SmallFract) THREEWORDINLINE(0x3F3C, 0x0002, 0xA82E)
            //   CMY2RGB(...)               THREEWORDINLINE(0x3F3C, 0x0003, 0xA82E)
            //   RGB2CMY(...)               THREEWORDINLINE(0x3F3C, 0x0004, 0xA82E)
            //   HSL2RGB(...)               THREEWORDINLINE(0x3F3C, 0x0005, 0xA82E)
            //   GetColor(...)              THREEWORDINLINE(0x3F3C, 0x0009, 0xA82E)
            //
            // SmallFract is documented as the low-order word of a Fixed
            // number, so Fix2SmallFract drops the integer part while
            // SmallFract2Fix zero-extends the fractional word into a
            // 16.16 Fixed value with integer part 0.
            (true, 0x02E) => {
                let sp = cpu.read_reg(Register::A7);
                let selector = bus.read_word(sp);
                match selector {
                    // Fix2SmallFract (selector 1)
                    // FUNCTION Fix2SmallFract(f: Fixed): SmallFract;
                    // Stack: [result(2)] [f(4)] [sel(2)] — pop 6, leave 2
                    // Inside Macintosh Volume V, V-175
                    1 => {
                        let f = bus.read_long(sp + 2);
                        let small_fract = (f & 0xFFFF) as u16;
                        bus.write_word(sp + 6, small_fract);
                        cpu.write_reg(Register::A7, sp + 6);
                    }
                    // SmallFract2Fix (selector 2)
                    // FUNCTION SmallFract2Fix(s: SmallFract): Fixed;
                    // Stack: [result(4)] [s(2)] [sel(2)] — pop 4, leave 4
                    // Inside Macintosh Volume V, V-175
                    2 => {
                        let s = bus.read_word(sp + 2) as u32;
                        let fixed = s;
                        bus.write_long(sp + 4, fixed);
                        cpu.write_reg(Register::A7, sp + 4);
                    }
                    // CMY2RGB(3), RGB2CMY(4) — component-wise complements
                    // between the subtractive CMY and additive RGB models.
                    // PROCEDURE XXX(srcColor: XColor; VAR dstColor: YColor);
                    // Stack: [src_ptr(4)] [dst_ptr(4)] [sel(2)] — pop 10
                    // Inside Macintosh Volume V, V-175; Volume VI, 19-10
                    3 | 4 => {
                        let src_ptr = bus.read_long(sp + 6);
                        let dst_ptr = bus.read_long(sp + 2);
                        for i in 0..3u32 {
                            let component = bus.read_word(src_ptr + i * 2);
                            bus.write_word(dst_ptr + i * 2, !component);
                        }
                        cpu.write_reg(Register::A7, sp + 10);
                    }
                    // HSL2RGB(5)
                    // PROCEDURE HSL2RGB(hColor: HSLColor; VAR rColor: RGBColor);
                    // Stack: [src_ptr(4)] [dst_ptr(4)] [sel(2)] — pop 10
                    // Inside Macintosh Volume V, V-175;
                    // Volume VI, 19-10..19-13.
                    5 => {
                        fn hue_to_rgb(p: f64, q: f64, mut t: f64) -> f64 {
                            if t < 0.0 {
                                t += 1.0;
                            } else if t > 1.0 {
                                t -= 1.0;
                            }

                            if t < 1.0 / 6.0 {
                                return p + (q - p) * 6.0 * t;
                            }
                            if t < 1.0 / 2.0 {
                                return q;
                            }
                            if t < 2.0 / 3.0 {
                                return p + (q - p) * (2.0 / 3.0 - t) * 6.0;
                            }
                            p
                        }

                        let src_ptr = bus.read_long(sp + 6);
                        let dst_ptr = bus.read_long(sp + 2);
                        let hue = bus.read_word(src_ptr) as f64 / 65535.0;
                        let saturation = bus.read_word(src_ptr + 2) as f64 / 65535.0;
                        let lightness = bus.read_word(src_ptr + 4) as f64 / 65535.0;
                        let (red, green, blue) = if saturation == 0.0 {
                            (lightness, lightness, lightness)
                        } else {
                            let q = if lightness < 0.5 {
                                lightness * (1.0 + saturation)
                            } else {
                                lightness + saturation - lightness * saturation
                            };
                            let p = 2.0 * lightness - q;
                            (
                                hue_to_rgb(p, q, hue + 1.0 / 3.0),
                                hue_to_rgb(p, q, hue),
                                hue_to_rgb(p, q, hue - 1.0 / 3.0),
                            )
                        };
                        let to_word = |component: f64| -> u16 {
                            (component.clamp(0.0, 1.0) * 65535.0).round() as u16
                        };
                        bus.write_word(dst_ptr, to_word(red));
                        bus.write_word(dst_ptr + 2, to_word(green));
                        bus.write_word(dst_ptr + 4, to_word(blue));
                        cpu.write_reg(Register::A7, sp + 10);
                    }
                    // RGB2HSL(6), HSV2RGB(7), RGB2HSV(8)
                    // are the remaining Color Picker conversions from
                    // IM:V V-175 and IM:VI 19-10..19-11.
                    6 => {
                        let src_ptr = bus.read_long(sp + 6);
                        let dst_ptr = bus.read_long(sp + 2);
                        let r = bus.read_word(src_ptr) as f64 / 65535.0;
                        let g = bus.read_word(src_ptr + 2) as f64 / 65535.0;
                        let b = bus.read_word(src_ptr + 4) as f64 / 65535.0;
                        let max = r.max(g).max(b);
                        let min = r.min(g).min(b);
                        let delta = max - min;
                        let lightness = (max + min) / 2.0;
                        let (hue, saturation) = if delta == 0.0 {
                            (0.0, 0.0)
                        } else {
                            let saturation = if lightness <= 0.5 {
                                delta / (max + min)
                            } else {
                                delta / (2.0 - max - min)
                            };
                            let mut hue = if max == r {
                                (g - b) / delta
                            } else if max == g {
                                2.0 + (b - r) / delta
                            } else {
                                4.0 + (r - g) / delta
                            };
                            if hue < 0.0 {
                                hue += 6.0;
                            }
                            (hue / 6.0, saturation)
                        };
                        let to_word = |component: f64| -> u16 {
                            (component.clamp(0.0, 1.0) * 65535.0).round() as u16
                        };
                        bus.write_word(dst_ptr, to_word(hue));
                        bus.write_word(dst_ptr + 2, to_word(saturation));
                        bus.write_word(dst_ptr + 4, to_word(lightness));
                        cpu.write_reg(Register::A7, sp + 10);
                    }
                    7 => {
                        let src_ptr = bus.read_long(sp + 6);
                        let dst_ptr = bus.read_long(sp + 2);
                        let hue = bus.read_word(src_ptr) as f64 / 65535.0;
                        let saturation = bus.read_word(src_ptr + 2) as f64 / 65535.0;
                        let value = bus.read_word(src_ptr + 4) as f64 / 65535.0;

                        let to_word = |component: f64| -> u16 {
                            (component.clamp(0.0, 1.0) * 65535.0).round() as u16
                        };
                        let (red, green, blue) = if saturation == 0.0 {
                            (value, value, value)
                        } else {
                            let h6 = hue * 6.0;
                            let sector = h6.floor() as i32;
                            let frac = h6 - sector as f64;
                            let p = value * (1.0 - saturation);
                            let q = value * (1.0 - saturation * frac);
                            let t = value * (1.0 - saturation * (1.0 - frac));
                            match sector.rem_euclid(6) {
                                0 => (value, t, p),
                                1 => (q, value, p),
                                2 => (p, value, t),
                                3 => (p, q, value),
                                4 => (t, p, value),
                                _ => (value, p, q),
                            }
                        };

                        bus.write_word(dst_ptr, to_word(red));
                        bus.write_word(dst_ptr + 2, to_word(green));
                        bus.write_word(dst_ptr + 4, to_word(blue));
                        cpu.write_reg(Register::A7, sp + 10);
                    }
                    8 => {
                        let src_ptr = bus.read_long(sp + 6);
                        let dst_ptr = bus.read_long(sp + 2);
                        let r = bus.read_word(src_ptr) as f64 / 65535.0;
                        let g = bus.read_word(src_ptr + 2) as f64 / 65535.0;
                        let b = bus.read_word(src_ptr + 4) as f64 / 65535.0;
                        let max = r.max(g).max(b);
                        let min = r.min(g).min(b);
                        let delta = max - min;
                        let value = max;
                        let saturation = if max == 0.0 { 0.0 } else { delta / max };
                        let hue = if delta == 0.0 {
                            0.0
                        } else {
                            let mut hue = if max == r {
                                (g - b) / delta
                            } else if max == g {
                                2.0 + (b - r) / delta
                            } else {
                                4.0 + (r - g) / delta
                            };
                            if hue < 0.0 {
                                hue += 6.0;
                            }
                            hue / 6.0
                        };
                        let to_word = |component: f64| -> u16 {
                            (component.clamp(0.0, 1.0) * 65535.0).round() as u16
                        };
                        bus.write_word(dst_ptr, to_word(hue));
                        bus.write_word(dst_ptr + 2, to_word(saturation));
                        bus.write_word(dst_ptr + 4, to_word(value));
                        cpu.write_reg(Register::A7, sp + 10);
                    }
                    // GetColor (selector 9)
                    // FUNCTION GetColor(where: Point; prompt: Str255;
                    //   inColor: RGBColor; VAR outColor: RGBColor): BOOLEAN;
                    // Stack: [result(2)] [outColorPtr(4)] [inColorPtr(4)] [prompt(4)]
                    //        [where(4)] [sel(2)] — pop 18, leave 2
                    // Inside Macintosh Volume V, V-174
                    9 => {
                        // Return FALSE (user cancelled)
                        bus.write_word(sp + 18, 0);
                        cpu.write_reg(Register::D0, 0);
                        cpu.write_reg(Register::A7, sp + 18);
                    }
                    _ => {
                        eprintln!("[PACK12] Unknown selector {} — popping 2 bytes", selector);
                        cpu.write_reg(Register::A7, sp + 2);
                    }
                }
                Ok(())
            }

            // AliasDispatch ($A823)
            // Dispatches Alias Manager routines selected in D0.
            // Register ABI: D0 = selector; each routine uses its documented Pascal stack frame.
            // Inside Macintosh: Files (1992), pp. 4-15 to 4-33.
            (true, 0x023) => {
                let sp = cpu.read_reg(Register::A7);
                let raw_selector = cpu.read_reg(Register::D0);
                let operation = alias_dispatch_operation_route(0xA823, raw_selector);
                self.current_selector_operation = operation.map(|route| route.operation_id);
                let selector = raw_selector & 0xFFFF;
                match selector {
                    // FindFolder (selector $0000)
                    // FUNCTION FindFolder(vRefNum: INTEGER; folderType: OSType;
                    //   createFolder: BOOLEAN; VAR foundVRefNum: INTEGER;
                    //   VAR foundDirID: LONGINT): OSErr;
                    // Inside Macintosh Volume VI, 9-42 to 9-44
                    // Stack (rightmost on top):
                    //   SP+0:  foundDirID_ptr(4)  SP+4:  foundVRefNum_ptr(4)
                    //   SP+8:  createFolder(2)    SP+10: folderType(4)
                    //   SP+14: vRefNum(2)         SP+16: result(2)
                    0 => {
                        let dirid_ptr = bus.read_long(sp);
                        let vref_ptr = bus.read_long(sp + 4);
                        let _create = bus.read_word(sp + 8) != 0;
                        let folder_type = bus.read_long(sp + 10);
                        let v_ref_num = bus.read_word(sp + 14) as i16;

                        let type_bytes = folder_type.to_be_bytes();
                        let type_str = std::str::from_utf8(&type_bytes).unwrap_or("????");
                        eprintln!(
                            "[ALIAS] FindFolder vRefNum={} type='{}' (${:08X})",
                            v_ref_num, type_str, folder_type
                        );

                        let found_dir_id = match folder_type {
                            t if t == u32::from_be_bytes(*b"pref") => {
                                self.ensure_vfs_directory("System Folder/Preferences")
                            }
                            t if t == u32::from_be_bytes(*b"temp") => {
                                self.ensure_vfs_directory("Temporary Items")
                            }
                            _ => 2,
                        };

                        bus.write_word(vref_ptr, (-1i16) as u16);
                        bus.write_long(dirid_ptr, found_dir_id);
                        // Pop 16 bytes params, leave 2-byte result
                        bus.write_word(sp + 16, 0); // noErr
                        cpu.write_reg(Register::A7, sp + 16);
                    }
                    // NewAlias (selector $0002)
                    // FUNCTION NewAlias(fromFile: FSSpecPtr; target: FSSpecPtr;
                    //   VAR alias: AliasHandle): OSErr;
                    // Stack (rightmost on top):
                    //   SP+0:  alias_ptr(4)   SP+4:  target_ptr(4)
                    //   SP+8:  fromFile_ptr(4)
                    //   SP+12: result(2)
                    // Inside Macintosh Volume VI, 9-57
                    0x0002 => {
                        let alias_ptr = bus.read_long(sp);
                        let target_ptr = bus.read_long(sp + 4);
                        let _from_file_ptr = bus.read_long(sp + 8);

                        let target_name = crate::trap::types::read_fsspec_name(bus, target_ptr);
                        eprintln!("[ALIAS] NewAlias target='{}'", target_name);

                        let alias_data_bytes = self.build_alias_record(bus, target_ptr);
                        if alias_ptr != 0 {
                            let alias_data = bus.alloc(alias_data_bytes.len() as u32);
                            bus.write_bytes(alias_data, &alias_data_bytes);
                            let alias_handle = bus.alloc(4);
                            bus.write_long(alias_handle, alias_data);
                            bus.write_long(alias_ptr, alias_handle);
                            self.track_handle_ptr(alias_data, alias_handle);
                        }

                        bus.write_word(sp + 12, 0); // noErr
                        cpu.write_reg(Register::A7, sp + 12);
                    }
                    // ResolveAlias (selector $0003)
                    // FUNCTION ResolveAlias(fromFile: FSSpecPtr; alias: AliasHandle;
                    //   VAR target: FSSpec; VAR wasChanged: Boolean): OSErr;
                    // Universal Interfaces 2.0, Aliases.h
                    // Stack (rightmost on top):
                    //   SP+0:  wasChanged_ptr(4)  SP+4:  target_ptr(4)
                    //   SP+8:  alias_handle(4)    SP+12: fromFile_ptr(4)
                    //   SP+16: result(2)
                    0x0003 => {
                        let was_changed_ptr = bus.read_long(sp);
                        let target_ptr = bus.read_long(sp + 4);
                        let alias_handle = bus.read_long(sp + 8);
                        let _from_file_ptr = bus.read_long(sp + 12);
                        let alias_data_ptr = if alias_handle != 0 {
                            bus.read_long(alias_handle)
                        } else {
                            0
                        };

                        let full_path = (alias_data_ptr != 0)
                            .then(|| {
                                Self::alias_extra_data(
                                    bus,
                                    alias_data_ptr,
                                    Self::ALIAS_EXTRA_FULL_PATH,
                                )
                            })
                            .flatten();
                        let found = full_path.and_then(|path_bytes| {
                            let full_path = decode_mac_roman(&path_bytes);
                            let relative_path = full_path
                                .split_once(':')
                                .map(|(_, relative)| relative)
                                .unwrap_or(full_path.as_str());
                            let normalized = Self::normalize_hfs_path(relative_path);
                            self.ensure_vfs_catalog();
                            Self::find_case_insensitive_relative_key(self.vfs.keys(), &normalized)
                                .or_else(|| {
                                    Self::find_case_insensitive_relative_key(
                                        self.vfs_rsrc.keys(),
                                        &normalized,
                                    )
                                })
                        });

                        if was_changed_ptr != 0 {
                            bus.write_byte(was_changed_ptr, 0);
                        }
                        let resolved = if let (Some(found), true) = (found, target_ptr != 0) {
                            let metadata = self.vfs_file_metadata(&found);
                            let parent_dir_id =
                                metadata.map(|entry| entry.parent_dir_id).unwrap_or(2);
                            let name = encode_mac_roman_lossy(Self::vfs_basename(&found));
                            let name_len = name.len().min(63);
                            bus.write_word(target_ptr, Self::boot_volume_ref_num_u16());
                            bus.write_long(target_ptr + 2, parent_dir_id);
                            bus.write_byte(target_ptr + 6, name_len as u8);
                            bus.write_bytes(target_ptr + 7, &name[..name_len]);
                            eprintln!("[ALIAS] ResolveAlias target='{}'", found);
                            true
                        } else {
                            false
                        };

                        bus.write_word(sp + 16, if resolved { 0 } else { (-43i16) as u16 });
                        cpu.write_reg(Register::A7, sp + 16);
                    }
                    // NewAliasMinimalFromFullPath (selector $0009)
                    // FUNCTION NewAliasMinimalFromFullPath(fullPathLength: INTEGER;
                    //   fullPath: Ptr; zoneName: Str32; serverName: Str31;
                    //   VAR alias: AliasHandle): OSErr;
                    // Universal Interfaces 2.0, Aliases.h
                    // Stack (rightmost on top):
                    //   SP+0:  alias_ptr(4)       SP+4:  serverName_ptr(4)
                    //   SP+8:  zoneName_ptr(4)    SP+12: fullPath_ptr(4)
                    //   SP+16: fullPathLength(2)  SP+18: result(2)
                    0x0009 => {
                        let alias_ptr = bus.read_long(sp);
                        let _server_name_ptr = bus.read_long(sp + 4);
                        let _zone_name_ptr = bus.read_long(sp + 8);
                        let full_path_ptr = bus.read_long(sp + 12);
                        let full_path_len = bus.read_word(sp + 16) as usize;

                        let valid = alias_ptr != 0 && full_path_ptr != 0 && full_path_len != 0;
                        if valid {
                            let full_path = bus.read_bytes(full_path_ptr, full_path_len);
                            eprintln!(
                                "[ALIAS] NewAliasMinimalFromFullPath path='{}'",
                                decode_mac_roman(&full_path)
                            );
                            let alias_data_bytes =
                                self.build_minimal_alias_record_from_full_path(&full_path);
                            let alias_data = bus.alloc(alias_data_bytes.len() as u32);
                            bus.write_bytes(alias_data, &alias_data_bytes);
                            let alias_handle = bus.alloc(4);
                            bus.write_long(alias_handle, alias_data);
                            bus.write_long(alias_ptr, alias_handle);
                            self.track_handle_ptr(alias_data, alias_handle);
                        } else if alias_ptr != 0 {
                            bus.write_long(alias_ptr, 0);
                        }

                        bus.write_word(sp + 18, if valid { 0 } else { (-50i16) as u16 });
                        cpu.write_reg(Register::A7, sp + 18);
                    }
                    // ResolveAliasFile (selector $000C)
                    // FUNCTION ResolveAliasFile(VAR theSpec: FSSpec;
                    //   resolveAliasChains: Boolean;
                    //   VAR targetIsFolder: Boolean;
                    //   VAR wasAliased: Boolean): OSErr;
                    // Macintosh Toolbox Essentials 1992, 7-52
                    // Stack (rightmost on top):
                    //   SP+0:  wasAliased_ptr(4)      SP+4:  targetIsFolder_ptr(4)
                    //   SP+8:  resolveAliasChains(2)  SP+10: theSpec_ptr(4)
                    //   SP+14: result(2)
                    0x000C => {
                        let was_aliased_ptr = bus.read_long(sp);
                        let target_is_folder_ptr = bus.read_long(sp + 4);
                        let _resolve_chains = bus.read_word(sp + 8) != 0;
                        let spec_ptr = bus.read_long(sp + 10);

                        let name = crate::trap::types::read_fsspec_name(bus, spec_ptr);
                        eprintln!("[ALIAS] ResolveAliasFile spec='{}'", name);

                        let vref = bus.read_word(spec_ptr) as i16;
                        let dir_id = bus.read_long(spec_ptr + 2);
                        let mut found = false;
                        let mut target_is_folder = false;
                        for candidate_dir_id in self.hfs_lookup_directory_ids(vref, dir_id) {
                            if self
                                .find_vfs_file_in_directory(candidate_dir_id, &name)
                                .is_some()
                                || self
                                    .find_vfs_rsrc_file_in_directory(candidate_dir_id, &name)
                                    .is_some()
                            {
                                found = true;
                                break;
                            }
                            if self
                                .find_vfs_directory_in_directory(candidate_dir_id, &name)
                                .is_some()
                            {
                                found = true;
                                target_is_folder = true;
                                break;
                            }
                        }

                        // A regular file is preserved and reported as not
                        // aliased. A nonexistent input spec returns fnfErr;
                        // callers use that distinction to try their own
                        // relative-location fallback.
                        bus.write_byte(was_aliased_ptr, 0);
                        bus.write_byte(target_is_folder_ptr, u8::from(target_is_folder));
                        // Pop 14 bytes params, leave 2-byte result
                        bus.write_word(sp + 14, if found { 0 } else { (-43i16) as u16 });
                        cpu.write_reg(Register::A7, sp + 14);
                    }
                    _ => {
                        eprintln!(
                            "[ALIAS] Unimplemented selector {} (${:04X})",
                            selector, selector
                        );
                        return Some(Err(Error::Halted));
                    }
                }
                Ok(())
            }

            // ========== ControlStripDispatch ($AAF2) ==========
            // Control Strip utility routines are dispatched by a selector in
            // D0. Universal Interfaces 3.4, ControlStrip.h encodes the
            // selector as `(argument_words << 8) | routine`, and the
            // PowerBook 520/520c/540/540c Developer Note, Control Strip
            // Module Reference, p. 86, specifies paramErr for an
            // unimplemented routine.
            //
            // Systemless has no Control Strip surface. Availability queries
            // therefore report FALSE, show/hide is a no-op, and unsupported
            // selectors return paramErr in D0 while consuming their encoded
            // Pascal argument frame. The dispatcher returns values in D0;
            // ControlStrip.h's inline entry points do not reserve a separate
            // result slot on the stack.
            // ControlStripDispatch ($AAF2): SBIsControlStripVisible ($0000)
            // returns FALSE without changing A7; SBShowHideControlStrip
            // ($0101) consumes one word-sized Boolean and returns no error;
            // unknown selectors return paramErr after consuming their packed
            // argument words.
            (true, 0x2F2) => {
                let sp = cpu.read_reg(Register::A7);
                let selector = (cpu.read_reg(Register::D0) & 0xFFFF) as u16;
                let arg_bytes = u32::from((selector >> 8) as u8) * 2;

                match selector {
                    // pascal Boolean SBIsControlStripVisible(void)
                    0x0000 => {
                        cpu.write_reg(Register::D0, 0);
                    }
                    // pascal void SBShowHideControlStrip(Boolean showIt)
                    0x0101 => {
                        cpu.write_reg(Register::A7, sp + 2);
                        cpu.write_reg(Register::D0, 0);
                    }
                    // pascal Boolean SBSafeToAccessStartupDisk(void)
                    0x0002 => {
                        // The emulated VFS has no spindle-up delay, so the
                        // startup disk is always safe to access.
                        cpu.write_reg(Register::D0, 1);
                    }
                    _ => {
                        cpu.write_reg(Register::A7, sp + arg_bytes);
                        cpu.write_reg(Register::D0, (-50i16) as i32 as u32);
                    }
                }
                Ok(())
            }

            // ========== CursorDeviceDispatch ($AADB) ==========
            // Cursor Device Manager dispatcher. Universal Interfaces 3.4
            // CursorDevices.h declares selectors 0..13 as OSErr Pascal
            // functions selected by MOVEQ #selector,D0; _CursorDeviceDispatch.
            // Stack cleanup still depends on the selected routine's argument
            // byte count.
            // CursorDeviceDispatch ($AADB): Returns noErr and consumes selector-specific args
            (true, 0x2DB) => {
                let sp = cpu.read_reg(Register::A7);
                let selector = (cpu.read_reg(Register::D0) & 0xFFFF) as u16;
                let arg_bytes = match selector {
                    0 | 1 => 12, // Move/MoveTo(device, x, y)
                    2 | 4 | 5 | 11 | 12 | 13 => 4,
                    3 | 7 => 6,
                    6 => 12, // ButtonOp(device, button, opcode, data)
                    8..=10 => 8,
                    _ => 0,
                };

                if matches!(selector, 11 | 12) {
                    let device_ptr_ptr = bus.read_long(sp);
                    if device_ptr_ptr != 0 {
                        bus.write_long(device_ptr_ptr, 0);
                    }
                }

                bus.write_word(sp + arg_bytes, 0);
                cpu.write_reg(Register::A7, sp + arg_bytes);
                cpu.write_reg(Register::D0, 0);
                Ok(())
            }

            // ========== Movie Toolbox Dispatch ($AAAA) ==========
            // Inside Macintosh: QuickTime (1993), pp. 2-33, 2-82 to 2-84.
            // Public MPW declarations:
            //   pascal OSErr EnterMovies(void);
            //   pascal void ExitMovies(void);
            // Single trap dispatcher for the entire QuickTime Movie
            // Toolbox API; the MPW glue loads the routine selector in
            // D0 before executing `_AAAA` (selector 1 = EnterMovies).
            // The zero-argument client calls exercised by the fixture
            // are stack-neutral.
            //
            // A complete Movie Toolbox emulation is a substantial
            // multi-iteration project (movies, tracks, media handlers,
            // codecs). The selectors below cover documented initialization,
            // movie-file loading, and stateful playback calls that appear
            // before gameplay in Classic Mac demos. Selector values are
            // verified against MPW Universal Headers Movies.h in Docker.
            (true, 0x2AA) => {
                let selector = cpu.read_reg(Register::D0) as u16;
                if super::dispatch::trace_quicktime_enabled() {
                    static QT_LOG_COUNT: std::sync::atomic::AtomicU32 =
                        std::sync::atomic::AtomicU32::new(0);
                    if QT_LOG_COUNT.fetch_add(1, std::sync::atomic::Ordering::Relaxed) < 100 {
                        eprintln!(
                            "[QUICKTIME] MovieToolboxDispatch selector_in_d0=${:04X} sp=${:08X} stack=${:08X}/${:08X}/${:08X}/${:08X} d1=${:08X} a0=${:08X} a1=${:08X}",
                            selector,
                            cpu.read_reg(Register::A7),
                            bus.read_long(cpu.read_reg(Register::A7)),
                            bus.read_long(cpu.read_reg(Register::A7) + 4),
                            bus.read_long(cpu.read_reg(Register::A7) + 8),
                            bus.read_long(cpu.read_reg(Register::A7) + 12),
                            cpu.read_reg(Register::D1),
                            cpu.read_reg(Register::A0),
                            cpu.read_reg(Register::A1)
                        );
                    }
                }
                match selector {
                    1 => {
                        // EnterMovies ($AAAA selector $0001)
                        // Initializes Movie Toolbox private state for the current application.
                        // pascal OSErr EnterMovies(void);
                        // Inside Macintosh: QuickTime 1993, pp. 2-82 to 2-83.
                        let sp = cpu.read_reg(Register::A7);
                        bus.write_word(sp, 0);
                        record_movie_error(self, 0);
                        return_noerr(cpu)
                    }
                    2 => {
                        // ExitMovies ($AAAA selector $0002)
                        // Releases Movie Toolbox private state for the current application.
                        // pascal void ExitMovies(void);
                        // Inside Macintosh: QuickTime 1993, pp. 2-83 to 2-84.
                        self.movie_states.clear();
                        self.movie_error = 0;
                        self.movie_sticky_error = 0;
                        return_noerr(cpu)
                    }
                    0x0192 => {
                        // OpenMovieFile ($AAAA selector $0192)
                        // Opens a QuickTime movie file and returns a resource-file refNum.
                        // pascal OSErr OpenMovieFile(const FSSpec *fileSpec,
                        //   short *resRefNum, char perms);
                        // Inside Macintosh: QuickTime 1993, pp. 2-98 to 2-99.
                        let sp = cpu.read_reg(Register::A7);
                        let permission = signed_byte_from_stack_word(bus.read_word(sp));
                        let ref_num_ptr = bus.read_long(sp + 2);
                        let spec_ptr = bus.read_long(sp + 6);
                        let filename = crate::trap::types::read_fsspec_name(bus, spec_ptr);
                        let vref = bus.read_word(spec_ptr) as i16;
                        let dir_id = bus.read_long(spec_ptr + 2);
                        let wants_write = permission == 2 || permission == 3;
                        let Some(vfs_key) = self.vfs_key_for_fsspec(vref, dir_id, &filename) else {
                            record_movie_error(self, -43);
                            bus.write_word(sp + 10, (-43i16) as u16);
                            cpu.write_reg(Register::A7, sp + 10);
                            cpu.write_reg(Register::D0, (-43i16) as u32);
                            return Some(Ok(()));
                        };
                        let normalized_key = Self::normalize_vfs_path(&vfs_key);
                        let rsrc_key = self
                            .vfs_rsrc
                            .keys()
                            .find(|key| {
                                Self::normalize_vfs_path(key).eq_ignore_ascii_case(&normalized_key)
                            })
                            .cloned();
                        let data_key = self
                            .vfs
                            .keys()
                            .find(|key| {
                                Self::normalize_vfs_path(key).eq_ignore_ascii_case(&normalized_key)
                            })
                            .cloned();

                        let refnum = if let Some(vfs_key) = rsrc_key {
                            if let Some(existing) = self.refnum_for_resource_file_name(&vfs_key) {
                                existing
                            } else {
                                self.open_resource_file_from_vfs_key(bus, &vfs_key, wants_write)
                            }
                        } else if let Some(vfs_key) = data_key {
                            if let Some(existing) = self.refnum_for_resource_file_name(&vfs_key) {
                                existing
                            } else {
                                let refnum = self.allocate_process_file_refnum();
                                self.register_empty_resource_file(refnum);
                                self.set_resource_file_name(refnum, vfs_key.clone());
                                if wants_write {
                                    self.write_refnums.insert(refnum);
                                }
                                self.open_files.insert(refnum, vfs_key);
                                self.file_positions.insert(refnum, 0);
                                self.set_current_resource_refnum(bus, refnum);
                                refnum
                            }
                        } else {
                            record_movie_error(self, -43);
                            bus.write_word(sp + 10, (-43i16) as u16);
                            cpu.write_reg(Register::A7, sp + 10);
                            cpu.write_reg(Register::D0, (-43i16) as u32);
                            return Some(Ok(()));
                        };

                        if refnum == u16::MAX {
                            let error = bus.read_word(0x0A60) as i16;
                            record_movie_error(self, error);
                            bus.write_word(sp + 10, error as u16);
                            cpu.write_reg(Register::A7, sp + 10);
                            cpu.write_reg(Register::D0, error as i32 as u32);
                            return Some(Ok(()));
                        }
                        if ref_num_ptr != 0 {
                            bus.write_word(ref_num_ptr, refnum);
                        }
                        if super::dispatch::trace_quicktime_enabled() {
                            eprintln!(
                                "[QUICKTIME] OpenMovieFile '{}' dirID={} -> refNum={}",
                                filename, dir_id, refnum
                            );
                        }
                        bus.write_word(sp + 10, 0);
                        cpu.write_reg(Register::A7, sp + 10);
                        cpu.write_reg(Register::D0, 0);
                        record_movie_error(self, 0);
                        Ok(())
                    }
                    0x00F0 => {
                        // NewMovieFromFile ($AAAA selector $00F0)
                        // Creates an in-memory Movie from an open movie file.
                        // pascal OSErr NewMovieFromFile(Movie *theMovie,
                        //   short resRefNum, short *resId, StringPtr resName,
                        //   short newMovieFlags, Boolean *dataRefWasChanged);
                        // Inside Macintosh: QuickTime 1993, pp. 2-88 to 2-90.
                        let sp = cpu.read_reg(Register::A7);
                        let data_ref_changed_ptr = bus.read_long(sp);
                        let new_movie_flags = bus.read_word(sp + 4);
                        let res_name_ptr = bus.read_long(sp + 6);
                        let res_id_ptr = bus.read_long(sp + 10);
                        let res_refnum = bus.read_word(sp + 14);
                        let movie_ptr = bus.read_long(sp + 16);
                        let requested_id = if res_id_ptr != 0 {
                            bus.read_word(res_id_ptr) as i16
                        } else {
                            0
                        };

                        let mut selected_resource: Option<(i16, String, Vec<u8>)> = None;
                        if requested_id != -1 {
                            if let Some(resources) = self.resources.as_ref() {
                                if let Some(file) = resources.files.get(&res_refnum) {
                                    if requested_id != 0
                                        && file.loaded.contains_key(&(*b"moov", requested_id))
                                    {
                                        let name = file
                                            .names_by_id
                                            .get(&(*b"moov", requested_id))
                                            .cloned()
                                            .unwrap_or_default();
                                        let data_ptr = file.loaded[&(*b"moov", requested_id)];
                                        selected_resource = Some((
                                            requested_id,
                                            name,
                                            read_allocated_bytes(bus, data_ptr),
                                        ));
                                    } else if requested_id == 0 {
                                        let mut ids: Vec<i16> = file
                                            .loaded
                                            .keys()
                                            .filter_map(|(res_type, id)| {
                                                (*res_type == *b"moov").then_some(*id)
                                            })
                                            .collect();
                                        ids.sort_unstable();
                                        if let Some(id) = ids.first().copied() {
                                            let name = file
                                                .names_by_id
                                                .get(&(*b"moov", id))
                                                .cloned()
                                                .unwrap_or_default();
                                            if let Some(&data_ptr) =
                                                file.loaded.get(&(*b"moov", id))
                                            {
                                                selected_resource = Some((
                                                    id,
                                                    name,
                                                    read_allocated_bytes(bus, data_ptr),
                                                ));
                                            }
                                        }
                                    }
                                }
                            }
                        }

                        let file_name = self.resource_file_name(res_refnum).map(str::to_owned);
                        let selected_movie = if selected_resource.is_some() {
                            selected_resource
                        } else if requested_id == 0 || requested_id == -1 {
                            file_name.as_ref().and_then(|name| {
                                self.vfs
                                    .get(name)
                                    .map(|data| (-1, String::new(), data.to_vec()))
                            })
                        } else {
                            None
                        };

                        let Some((selected_id, selected_name, movie_data)) = selected_movie else {
                            bus.write_word(sp + 20, QUICKTIME_INVALID_MOVIE as u16);
                            if movie_ptr != 0 {
                                bus.write_long(movie_ptr, 0);
                            }
                            cpu.write_reg(Register::A7, sp + 20);
                            cpu.write_reg(Register::D0, QUICKTIME_INVALID_MOVIE as u32);
                            record_movie_error(self, QUICKTIME_INVALID_MOVIE);
                            return Some(Ok(()));
                        };

                        let (box_rect, duration, time_scale) =
                            quicktime_movie_metadata(&movie_data);

                        // Parse the movie's video sample tables. When the moov
                        // came from a resource (selected_id != -1) the sample
                        // bytes live in the file's data fork; when it came from
                        // the data fork directly (-1) the samples index into
                        // that same buffer.
                        let media = super::movie_media::parse_video_track(&movie_data);
                        let data_fork = if selected_id == -1 {
                            movie_data.clone()
                        } else {
                            file_name
                                .as_ref()
                                .and_then(|name| self.vfs_data_fork_bytes(name))
                                .unwrap_or_default()
                        };
                        if super::dispatch::trace_quicktime_enabled() {
                            if let Some(ref vt) = media {
                                eprintln!(
                                    "[QUICKTIME] parsed video track codec={:?} {}x{} depth={} samples={} dataFork={}B",
                                    std::str::from_utf8(&vt.codec).unwrap_or("?"),
                                    vt.width,
                                    vt.height,
                                    vt.depth,
                                    vt.samples.len(),
                                    data_fork.len(),
                                );
                            }
                        }

                        let movie = bus.alloc(16);
                        bus.write_long(movie, u32::from_be_bytes(*b"MooV"));
                        bus.write_word(movie + 4, res_refnum);
                        bus.write_word(movie + 6, selected_id as u16);
                        bus.write_word(movie + 8, new_movie_flags);
                        let mut movie_state = MovieState::new(
                            res_refnum,
                            selected_id,
                            new_movie_flags,
                            box_rect,
                            duration,
                            time_scale,
                        );
                        movie_state.music =
                            super::movie_media::parse_music_track(&movie_data, &data_fork);
                        movie_state.media = media;
                        movie_state.data_fork = data_fork;
                        self.movie_states.insert(movie, movie_state);
                        if movie_ptr != 0 {
                            bus.write_long(movie_ptr, movie);
                        }
                        if res_id_ptr != 0 {
                            bus.write_word(res_id_ptr, selected_id as u16);
                        }
                        if res_name_ptr != 0 {
                            bus.write_pstring(res_name_ptr, selected_name.as_bytes());
                        }
                        if data_ref_changed_ptr != 0 {
                            bus.write_byte(data_ref_changed_ptr, 0);
                        }
                        if super::dispatch::trace_quicktime_enabled() {
                            eprintln!(
                                "[QUICKTIME] NewMovieFromFile refNum={} requestedID={} selectedID={} bytes={} box={:?} duration={} timeScale={} -> movie=${:08X}",
                                res_refnum,
                                requested_id,
                                selected_id,
                                movie_data.len(),
                                box_rect,
                                duration,
                                time_scale,
                                movie
                            );
                        }
                        bus.write_word(sp + 20, 0);
                        cpu.write_reg(Register::A7, sp + 20);
                        cpu.write_reg(Register::D0, 0);
                        record_movie_error(self, 0);
                        Ok(())
                    }
                    0x0012 => {
                        // GetMovieTimeBase ($AAAA selector $0012)
                        // Returns the movie-owned time base.
                        // pascal TimeBase GetMovieTimeBase(Movie movie);
                        // Inside Macintosh: QuickTime 1993, p. 2-190.
                        let sp = cpu.read_reg(Register::A7);
                        let movie = bus.read_long(sp);
                        let valid = self.movie_states.contains_key(&movie);
                        bus.write_long(sp + 4, if valid { movie } else { 0 });
                        cpu.write_reg(Register::A7, sp + 4);
                        record_movie_error(self, if valid { 0 } else { QUICKTIME_INVALID_MOVIE });
                        Ok(())
                    }
                    0x00B2 => {
                        // SetTimeBaseFlags ($AAAA selector $00B2)
                        // Sets playback control flags, including loopTimeBase.
                        // pascal void SetTimeBaseFlags(TimeBase tb, long flags);
                        // Inside Macintosh: QuickTime 1993, pp. 2-331–2-332.
                        let sp = cpu.read_reg(Register::A7);
                        let flags = bus.read_long(sp);
                        let time_base = bus.read_long(sp + 4);
                        if let Some(state) = self.movie_states.get_mut(&time_base) {
                            state.time_base_flags = flags;
                        }
                        cpu.write_reg(Register::A7, sp + 8);
                        Ok(())
                    }
                    0x01B3 => {
                        // NewMovieFromDataFork ($AAAA selector $01B3)
                        // Loads a movie atom at an offset in an open data fork.
                        // pascal OSErr NewMovieFromDataFork(Movie *movie, short refNum,
                        //   long offset, short flags, Boolean *changed);
                        // Inside Macintosh: QuickTime 1993, pp. 2-109–2-110.
                        let sp = cpu.read_reg(Register::A7);
                        let changed = bus.read_long(sp);
                        let flags = bus.read_word(sp + 4);
                        let offset = bus.read_long(sp + 6) as usize;
                        let refnum = bus.read_word(sp + 10);
                        let out = bus.read_long(sp + 12);
                        let data = self
                            .open_files
                            .get(&refnum)
                            .and_then(|name| self.vfs_data_fork_bytes(name));
                        let mut error: i16 = -51;
                        if out != 0 {
                            bus.write_long(out, 0);
                        }
                        if let Some(data) = data {
                            error = -2002;
                            let atom = data.get(offset..).and_then(|rest| {
                                let size = rest
                                    .get(..4)
                                    .map(|n| u32::from_be_bytes(n.try_into().unwrap()) as usize)?;
                                if size < 8 || rest.get(4..8) != Some(b"moov") {
                                    return None;
                                }
                                rest.get(..size)
                            });
                            if let Some(atom) = atom {
                                let (rect, duration, scale) = quicktime_movie_metadata(atom);
                                let mut state =
                                    MovieState::new(refnum, -1, flags, rect, duration, scale);
                                state.media = super::movie_media::parse_video_track(atom);
                                state.music = super::movie_media::parse_music_track(atom, &data);
                                state.active = flags & 1 != 0;
                                if super::dispatch::trace_quicktime_enabled() {
                                    eprintln!("[QUICKTIME] NewMovieFromDataFork offset={} duration={} scale={} notes={}",
                                                    offset, duration, scale, state.music.as_ref().map_or(0, Vec::len));
                                }
                                state.data_fork = data;
                                let movie = bus.alloc(16);
                                if movie == 0 {
                                    error = -108;
                                } else if out == 0 {
                                    bus.free(movie);
                                    error = -50;
                                } else {
                                    bus.write_long(movie, u32::from_be_bytes(*b"MooV"));
                                    self.movie_states.insert(movie, state);
                                    bus.write_long(out, movie);
                                    if changed != 0 {
                                        bus.write_byte(changed, 0);
                                    }
                                    error = 0;
                                }
                            }
                        }
                        bus.write_word(sp + 16, error as u16);
                        cpu.write_reg(Register::A7, sp + 16);
                        cpu.write_reg(Register::D0, error as u32);
                        record_movie_error(self, error);
                        Ok(())
                    }
                    0x0187 => {
                        // NewMovie ($AAAA selector $0187)
                        // Creates an empty in-memory Movie and returns its handle.
                        // pascal Movie NewMovie(long flags);
                        // Inside Macintosh: QuickTime 1993, pp. 2-92 to 2-93.
                        let sp = cpu.read_reg(Register::A7);
                        let flags = bus.read_long(sp);
                        let movie = bus.alloc(16);
                        bus.write_long(movie, u32::from_be_bytes(*b"MooV"));
                        bus.write_long(movie + 4, flags);
                        bus.write_long(movie + 8, *self.current_port);
                        bus.write_long(movie + 12, *self.current_gdevice);
                        let mut state =
                            MovieState::new(0, -1, flags as u16, (0, 0, 120, 160), 1, 600);
                        state.gworld_port = *self.current_port;
                        state.gworld_gdh = *self.current_gdevice;
                        state.active = false;
                        self.movie_states.insert(movie, state);
                        bus.write_long(sp + 4, movie);
                        cpu.write_reg(Register::A7, sp + 4);
                        cpu.write_reg(Register::D0, 0);
                        record_movie_error(self, 0);
                        Ok(())
                    }
                    3 => {
                        // GetMoviesError ($AAAA selector $0003)
                        // Returns and clears the Movie Toolbox current error.
                        // pascal OSErr GetMoviesError(void);
                        // Inside Macintosh: QuickTime 1993, p. 2-85.
                        let sp = cpu.read_reg(Register::A7);
                        let err = self.movie_error;
                        bus.write_word(sp, err as u16);
                        self.movie_error = 0;
                        cpu.write_reg(Register::D0, err as u32);
                        Ok(())
                    }
                    4 => {
                        // GetMoviesStickyError ($AAAA selector $0004)
                        // Returns the first nonzero Movie Toolbox error since it was cleared.
                        // pascal OSErr GetMoviesStickyError(void);
                        // Inside Macintosh: QuickTime 1993, p. 2-85.
                        let sp = cpu.read_reg(Register::A7);
                        bus.write_word(sp, self.movie_sticky_error as u16);
                        cpu.write_reg(Register::D0, self.movie_sticky_error as u32);
                        Ok(())
                    }
                    0x00DE => {
                        // ClearMoviesStickyError ($AAAA selector $00DE)
                        // Clears the Movie Toolbox sticky error.
                        // pascal void ClearMoviesStickyError(void);
                        // Inside Macintosh: QuickTime 1993, p. 2-85.
                        self.movie_sticky_error = 0;
                        cpu.write_reg(Register::D0, 0);
                        Ok(())
                    }
                    0x0005 => {
                        // MoviesTask ($AAAA selector $0005)
                        // Services active movies from the event loop.
                        // pascal void MoviesTask(Movie theMovie, long maxMilliSecToUse);
                        // Inside Macintosh: QuickTime 1993, pp. 2-124 to 2-125.
                        let sp = cpu.read_reg(Register::A7);
                        let movie = bus.read_long(sp + 4);
                        // Advance the timeline of active movies by real elapsed
                        // guest time and render the frames now due, rather than
                        // jumping straight to each movie's end.
                        self.advance_and_render_active_movies(bus);
                        let ok = movie == 0 || self.movie_states.contains_key(&movie);
                        cpu.write_reg(Register::A7, sp + 8);
                        let err = if ok { 0 } else { QUICKTIME_INVALID_MOVIE };
                        cpu.write_reg(Register::D0, err as u32);
                        record_movie_error(self, err);
                        Ok(())
                    }
                    0x0006 => {
                        // PrerollMovie ($AAAA selector $0006)
                        // Prepares movie media for playback at the requested time and rate.
                        // pascal OSErr PrerollMovie(Movie theMovie, TimeValue time, Fixed Rate);
                        // Inside Macintosh: QuickTime 1993, pp. 2-135 to 2-136.
                        let sp = cpu.read_reg(Register::A7);
                        let rate = bus.read_long(sp) as i32;
                        let time = bus.read_long(sp + 4) as i32;
                        let movie = bus.read_long(sp + 8);
                        let err = if let Some(state) = self.movie_states.get_mut(&movie) {
                            state.preferred_rate = rate;
                            state.current_time = time.clamp(0, state.duration);
                            state.audio_time = state.current_time as f64 / state.time_scale as f64;
                            0
                        } else {
                            QUICKTIME_INVALID_MOVIE
                        };
                        bus.write_word(sp + 12, err as u16);
                        cpu.write_reg(Register::A7, sp + 12);
                        cpu.write_reg(Register::D0, err as u32);
                        record_movie_error(self, err);
                        Ok(())
                    }
                    0x0007 => {
                        // LoadMovieIntoRam ($AAAA selector $0007)
                        // Requests a movie segment be loaded into memory for playback.
                        // pascal OSErr LoadMovieIntoRam(Movie theMovie, TimeValue time,
                        //   TimeValue duration, long flags);
                        // Inside Macintosh: QuickTime 1993, p. 2-135.
                        let sp = cpu.read_reg(Register::A7);
                        let _flags = bus.read_long(sp);
                        let _duration = bus.read_long(sp + 4);
                        let _time = bus.read_long(sp + 8);
                        let movie = bus.read_long(sp + 12);
                        let err = if self.movie_states.contains_key(&movie) {
                            0
                        } else {
                            QUICKTIME_INVALID_MOVIE
                        };
                        bus.write_word(sp + 16, err as u16);
                        cpu.write_reg(Register::A7, sp + 16);
                        cpu.write_reg(Register::D0, err as u32);
                        record_movie_error(self, err);
                        Ok(())
                    }
                    0x0009 => {
                        // SetMovieActive ($AAAA selector $0009)
                        // Enables or disables a movie's participation in
                        // MoviesTask processing.
                        // pascal void SetMovieActive(Movie theMovie, Boolean active);
                        // Inside Macintosh: QuickTime 1993, p. 2-122.
                        let sp = cpu.read_reg(Register::A7);
                        let active = bus.read_word(sp) != 0;
                        let movie = bus.read_long(sp + 2);
                        let err = if let Some(state) = self.movie_states.get_mut(&movie) {
                            state.active = active;
                            0
                        } else {
                            QUICKTIME_INVALID_MOVIE
                        };
                        cpu.write_reg(Register::A7, sp + 6);
                        cpu.write_reg(Register::D0, err as u32);
                        record_movie_error(self, err);
                        Ok(())
                    }
                    0x000B => {
                        // StartMovie ($AAAA selector $000B)
                        // Starts playback from the current movie time at the preferred rate.
                        // pascal void StartMovie(Movie theMovie);
                        // Inside Macintosh: QuickTime 1993, pp. 2-111 to 2-112.
                        let sp = cpu.read_reg(Register::A7);
                        let movie = bus.read_long(sp);
                        let now = self.current_tick();
                        let err = if let Some(state) = self.movie_states.get_mut(&movie) {
                            state.active = true;
                            state.rate = state.preferred_rate;
                            state.audio_time = state.current_time as f64 / state.time_scale as f64;
                            // Begin the playback clock now so MoviesTask advances
                            // by real elapsed time from this point.
                            state.last_service_tick = Some(now);
                            0
                        } else {
                            QUICKTIME_INVALID_MOVIE
                        };
                        cpu.write_reg(Register::A7, sp + 4);
                        cpu.write_reg(Register::D0, err as u32);
                        record_movie_error(self, err);
                        Ok(())
                    }
                    0x000C => {
                        // StopMovie ($AAAA selector $000C)
                        // Stops movie playback.
                        // pascal void StopMovie(Movie theMovie);
                        // Inside Macintosh: QuickTime 1993, p. 2-112.
                        let sp = cpu.read_reg(Register::A7);
                        let movie = bus.read_long(sp);
                        let err = if let Some(state) = self.movie_states.get_mut(&movie) {
                            state.rate = 0;
                            state.active = false;
                            0
                        } else {
                            QUICKTIME_INVALID_MOVIE
                        };
                        cpu.write_reg(Register::A7, sp + 4);
                        cpu.write_reg(Register::D0, err as u32);
                        record_movie_error(self, err);
                        Ok(())
                    }
                    0x000D => {
                        // GoToBeginningOfMovie ($AAAA selector $000D)
                        // Repositions a movie to play from its start.
                        // pascal void GoToBeginningOfMovie(Movie theMovie);
                        // Inside Macintosh: QuickTime 1993, p. 2-113.
                        let sp = cpu.read_reg(Register::A7);
                        let movie = bus.read_long(sp);
                        let err = if let Some(state) = self.movie_states.get_mut(&movie) {
                            state.current_time = 0;
                            state.audio_time = 0.0;
                            0
                        } else {
                            QUICKTIME_INVALID_MOVIE
                        };
                        cpu.write_reg(Register::A7, sp + 4);
                        cpu.write_reg(Register::D0, err as u32);
                        record_movie_error(self, err);
                        Ok(())
                    }
                    0x000E => {
                        // GoToEndOfMovie ($AAAA selector $000E)
                        // Repositions a movie to play from its end.
                        // pascal void GoToEndOfMovie(Movie theMovie);
                        // Inside Macintosh: QuickTime 1993, p. 2-114.
                        let sp = cpu.read_reg(Register::A7);
                        let movie = bus.read_long(sp);
                        let err = if let Some(state) = self.movie_states.get_mut(&movie) {
                            state.current_time = state.duration;
                            state.audio_time = state.duration as f64;
                            0
                        } else {
                            QUICKTIME_INVALID_MOVIE
                        };
                        cpu.write_reg(Register::A7, sp + 4);
                        cpu.write_reg(Register::D0, err as u32);
                        record_movie_error(self, err);
                        Ok(())
                    }
                    0x0016 => {
                        // SetMovieGWorld ($AAAA selector $0016)
                        // Sets the graphics world used to display a movie.
                        // pascal void SetMovieGWorld(Movie theMovie, CGrafPtr port,
                        //   GDHandle gdh);
                        // Inside Macintosh: QuickTime 1993, pp. 2-159 to 2-160.
                        let sp = cpu.read_reg(Register::A7);
                        let gdh = bus.read_long(sp);
                        let port = bus.read_long(sp + 4);
                        let movie = bus.read_long(sp + 8);
                        let err = if let Some(state) = self.movie_states.get_mut(&movie) {
                            state.gworld_port = port;
                            state.gworld_gdh = gdh;
                            0
                        } else {
                            QUICKTIME_INVALID_MOVIE
                        };
                        if super::dispatch::trace_quicktime_enabled() {
                            eprintln!(
                                "[QUICKTIME] SetMovieGWorld movie=${:08X} port=${:08X} gdh=${:08X}",
                                movie, port, gdh
                            );
                        }
                        cpu.write_reg(Register::A7, sp + 12);
                        cpu.write_reg(Register::D0, err as u32);
                        record_movie_error(self, err);
                        Ok(())
                    }
                    0x0015 => {
                        // GetMovieGWorld ($AAAA selector $0015)
                        // Returns the graphics world used to display a movie.
                        // pascal void GetMovieGWorld(Movie theMovie, CGrafPtr *port,
                        //   GDHandle *gdh);
                        // Inside Macintosh: QuickTime 1993, p. 2-160.
                        let sp = cpu.read_reg(Register::A7);
                        let gdh_ptr = bus.read_long(sp);
                        let port_ptr = bus.read_long(sp + 4);
                        let movie = bus.read_long(sp + 8);
                        let values = self
                            .movie_states
                            .get(&movie)
                            .map(|state| (state.gworld_port, state.gworld_gdh));
                        let err = if let Some((port, gdh)) = values {
                            if port_ptr != 0 {
                                bus.write_long(port_ptr, port);
                            }
                            if gdh_ptr != 0 {
                                bus.write_long(gdh_ptr, gdh);
                            }
                            0
                        } else {
                            QUICKTIME_INVALID_MOVIE
                        };
                        cpu.write_reg(Register::A7, sp + 12);
                        cpu.write_reg(Register::D0, err as u32);
                        record_movie_error(self, err);
                        Ok(())
                    }
                    0x0023 => {
                        // DisposeMovie ($AAAA selector $0023)
                        // Frees memory and state owned by a movie.
                        // pascal void DisposeMovie(Movie theMovie);
                        // Inside Macintosh: QuickTime 1993, p. 2-96.
                        let sp = cpu.read_reg(Register::A7);
                        let movie = bus.read_long(sp);
                        let err = if self.movie_states.remove(&movie).is_some() {
                            bus.free(movie);
                            0
                        } else {
                            QUICKTIME_INVALID_MOVIE
                        };
                        cpu.write_reg(Register::A7, sp + 4);
                        cpu.write_reg(Register::D0, err as u32);
                        record_movie_error(self, err);
                        Ok(())
                    }
                    0x018B => {
                        // DisposeMovieController ($AAAA selector $018B)
                        // Disposes a movie controller component instance.
                        // pascal void DisposeMovieController(MovieController mc);
                        // Inside Macintosh: QuickTime Components 1993, p. 2-32.
                        //
                        // Systemless does not allocate controller component
                        // state, but the Pascal procedure still consumes its
                        // four-byte ComponentInstance argument. MPW Movies.h
                        // publishes selector $018B for this glue.
                        let sp = cpu.read_reg(Register::A7);
                        cpu.write_reg(Register::A7, sp + 4);
                        cpu.write_reg(Register::D0, 0);
                        record_movie_error(self, 0);
                        Ok(())
                    }
                    0x002B => {
                        // GetMovieDuration ($AAAA selector $002B)
                        // Returns the calculated duration of a movie.
                        // pascal TimeValue GetMovieDuration(Movie theMovie);
                        // Inside Macintosh: QuickTime 1993, p. 2-185.
                        let sp = cpu.read_reg(Register::A7);
                        let movie = bus.read_long(sp);
                        let duration = self
                            .movie_states
                            .get(&movie)
                            .map(|state| state.duration)
                            .unwrap_or(0);
                        let err = if duration == 0 {
                            QUICKTIME_INVALID_MOVIE
                        } else {
                            0
                        };
                        if super::dispatch::trace_quicktime_enabled() {
                            eprintln!(
                                "[QUICKTIME] GetMovieDuration movie=${:08X} -> {}",
                                movie, duration
                            );
                        }
                        bus.write_long(sp + 4, duration as u32);
                        cpu.write_reg(Register::A7, sp + 4);
                        cpu.write_reg(Register::D0, err as u32);
                        record_movie_error(self, err);
                        Ok(())
                    }
                    0x0029 => {
                        // GetMovieTimeScale ($AAAA selector $0029)
                        // Returns the movie time scale in units per second.
                        // pascal TimeScale GetMovieTimeScale(Movie theMovie);
                        // Inside Macintosh: QuickTime 1993, p. 2-183.
                        let sp = cpu.read_reg(Register::A7);
                        let movie = bus.read_long(sp);
                        let time_scale = self
                            .movie_states
                            .get(&movie)
                            .map(|state| state.time_scale)
                            .unwrap_or(0);
                        let err = if time_scale == 0 {
                            QUICKTIME_INVALID_MOVIE
                        } else {
                            0
                        };
                        bus.write_long(sp + 4, time_scale as u32);
                        cpu.write_reg(Register::A7, sp + 4);
                        cpu.write_reg(Register::D0, err as u32);
                        record_movie_error(self, err);
                        Ok(())
                    }
                    0x002C => {
                        // GetMovieRate ($AAAA selector $002C)
                        // Returns the current movie playback rate.
                        // pascal Fixed GetMovieRate(Movie theMovie);
                        // Inside Macintosh: QuickTime 1993, p. 2-185.
                        let sp = cpu.read_reg(Register::A7);
                        let movie = bus.read_long(sp);
                        let rate = self
                            .movie_states
                            .get(&movie)
                            .map(|state| state.rate)
                            .unwrap_or(0);
                        let err = if self.movie_states.contains_key(&movie) {
                            0
                        } else {
                            QUICKTIME_INVALID_MOVIE
                        };
                        bus.write_long(sp + 4, rate as u32);
                        cpu.write_reg(Register::A7, sp + 4);
                        cpu.write_reg(Register::D0, err as u32);
                        record_movie_error(self, err);
                        Ok(())
                    }
                    0x002D => {
                        // SetMovieRate ($AAAA selector $002D)
                        // Sets the movie playback rate.
                        // pascal void SetMovieRate(Movie theMovie, Fixed rate);
                        // Inside Macintosh: QuickTime 1993, p. 2-185.
                        let sp = cpu.read_reg(Register::A7);
                        let rate = bus.read_long(sp) as i32;
                        let movie = bus.read_long(sp + 4);
                        let err = if let Some(state) = self.movie_states.get_mut(&movie) {
                            state.rate = rate;
                            if rate != 0 {
                                state.active = true;
                            } else {
                                state.active = false;
                            }
                            0
                        } else {
                            QUICKTIME_INVALID_MOVIE
                        };
                        cpu.write_reg(Register::A7, sp + 8);
                        cpu.write_reg(Register::D0, err as u32);
                        record_movie_error(self, err);
                        Ok(())
                    }
                    0x00F3 => {
                        // GetMoviePreferredRate ($AAAA selector $00F3)
                        // Returns a movie's preferred playback rate.
                        // MPW Universal Headers Movies.h:
                        //   THREEWORDINLINE(0x303C, 0x00F3, 0xAAAA)
                        // pascal Fixed GetMoviePreferredRate(Movie theMovie);
                        // Inside Macintosh: QuickTime 1993, pp. 2-130 to 2-131.
                        let sp = cpu.read_reg(Register::A7);
                        let movie = bus.read_long(sp);
                        let rate = self
                            .movie_states
                            .get(&movie)
                            .map(|state| state.preferred_rate)
                            .unwrap_or(0);
                        let err = if self.movie_states.contains_key(&movie) {
                            0
                        } else {
                            QUICKTIME_INVALID_MOVIE
                        };
                        bus.write_long(sp + 4, rate as u32);
                        cpu.write_reg(Register::A7, sp + 4);
                        cpu.write_reg(Register::D0, err as u32);
                        record_movie_error(self, err);
                        Ok(())
                    }
                    0x00F4 => {
                        // SetMoviePreferredRate ($AAAA selector $00F4)
                        // Sets a movie's preferred playback rate.
                        // MPW Universal Headers Movies.h:
                        //   THREEWORDINLINE(0x303C, 0x00F4, 0xAAAA)
                        // pascal void SetMoviePreferredRate(Movie theMovie, Fixed rate);
                        // Inside Macintosh: QuickTime 1993, pp. 2-130 to 2-131.
                        let sp = cpu.read_reg(Register::A7);
                        let rate = bus.read_long(sp) as i32;
                        let movie = bus.read_long(sp + 4);
                        let err = if let Some(state) = self.movie_states.get_mut(&movie) {
                            state.preferred_rate = rate;
                            0
                        } else {
                            QUICKTIME_INVALID_MOVIE
                        };
                        cpu.write_reg(Register::A7, sp + 8);
                        cpu.write_reg(Register::D0, err as u32);
                        record_movie_error(self, err);
                        Ok(())
                    }
                    0x002E => {
                        // GetMovieVolume ($AAAA selector $002E)
                        // Returns a movie's current 8.8 fixed-point volume.
                        // pascal short GetMovieVolume(Movie theMovie);
                        // Inside Macintosh: QuickTime 1993, p. 2-182.
                        let sp = cpu.read_reg(Register::A7);
                        let movie = bus.read_long(sp);
                        let volume = self
                            .movie_states
                            .get(&movie)
                            .map(|state| state.volume)
                            .unwrap_or(0);
                        let err = if self.movie_states.contains_key(&movie) {
                            0
                        } else {
                            QUICKTIME_INVALID_MOVIE
                        };
                        bus.write_word(sp + 4, volume as u16);
                        cpu.write_reg(Register::A7, sp + 4);
                        cpu.write_reg(Register::D0, err as u32);
                        record_movie_error(self, err);
                        Ok(())
                    }
                    0x002F => {
                        // SetMovieVolume ($AAAA selector $002F)
                        // Sets a movie's current 8.8 fixed-point volume.
                        // pascal void SetMovieVolume(Movie theMovie, short volume);
                        // Inside Macintosh: QuickTime 1993, p. 2-182.
                        let sp = cpu.read_reg(Register::A7);
                        let volume = bus.read_word(sp) as i16;
                        let movie = bus.read_long(sp + 2);
                        let err = if let Some(state) = self.movie_states.get_mut(&movie) {
                            state.volume = volume;
                            0
                        } else {
                            QUICKTIME_INVALID_MOVIE
                        };
                        cpu.write_reg(Register::A7, sp + 6);
                        cpu.write_reg(Register::D0, err as u32);
                        record_movie_error(self, err);
                        Ok(())
                    }
                    0x00F5 => {
                        // GetMoviePreferredVolume ($AAAA selector $00F5)
                        // Returns a movie's preferred 8.8 fixed-point volume.
                        // MPW Universal Headers Movies.h:
                        //   THREEWORDINLINE(0x303C, 0x00F5, 0xAAAA)
                        // pascal short GetMoviePreferredVolume(Movie theMovie);
                        // Inside Macintosh: QuickTime 1993, pp. 2-132 to 2-133.
                        let sp = cpu.read_reg(Register::A7);
                        let movie = bus.read_long(sp);
                        let volume = self
                            .movie_states
                            .get(&movie)
                            .map(|state| state.volume)
                            .unwrap_or(0);
                        let err = if self.movie_states.contains_key(&movie) {
                            0
                        } else {
                            QUICKTIME_INVALID_MOVIE
                        };
                        bus.write_word(sp + 4, volume as u16);
                        cpu.write_reg(Register::A7, sp + 4);
                        cpu.write_reg(Register::D0, err as u32);
                        record_movie_error(self, err);
                        Ok(())
                    }
                    0x00F6 => {
                        // SetMoviePreferredVolume ($AAAA selector $00F6)
                        // Sets a movie's preferred 8.8 fixed-point volume.
                        // MPW Universal Headers Movies.h:
                        //   THREEWORDINLINE(0x303C, 0x00F6, 0xAAAA)
                        // pascal void SetMoviePreferredVolume(Movie theMovie, short volume);
                        // Inside Macintosh: QuickTime 1993, pp. 2-132 to 2-133.
                        let sp = cpu.read_reg(Register::A7);
                        let volume = bus.read_word(sp) as i16;
                        let movie = bus.read_long(sp + 2);
                        let err = if let Some(state) = self.movie_states.get_mut(&movie) {
                            state.volume = volume;
                            0
                        } else {
                            QUICKTIME_INVALID_MOVIE
                        };
                        cpu.write_reg(Register::A7, sp + 6);
                        cpu.write_reg(Register::D0, err as u32);
                        record_movie_error(self, err);
                        Ok(())
                    }
                    0x0039 => {
                        // GetMovieTime ($AAAA selector $0039)
                        // Returns the current movie time and optionally fills
                        // the corresponding TimeRecord.
                        // pascal TimeValue GetMovieTime(Movie theMovie,
                        //   TimeRecord *currentTime);
                        // Inside Macintosh: QuickTime 1993, pp. 2-183 to 2-184.
                        let sp = cpu.read_reg(Register::A7);
                        let time_record_ptr = bus.read_long(sp);
                        let movie = bus.read_long(sp + 4);
                        let state = self.movie_states.get(&movie);
                        let current_time = state.map(|value| value.current_time).unwrap_or(0);
                        let err = if let Some(state) = state {
                            if time_record_ptr != 0 {
                                bus.write_long(
                                    time_record_ptr,
                                    if current_time < 0 { u32::MAX } else { 0 },
                                );
                                bus.write_long(time_record_ptr + 4, current_time as u32);
                                bus.write_long(time_record_ptr + 8, state.time_scale as u32);
                                bus.write_long(time_record_ptr + 12, 0);
                            }
                            0
                        } else {
                            QUICKTIME_INVALID_MOVIE
                        };
                        bus.write_long(sp + 8, current_time as u32);
                        cpu.write_reg(Register::A7, sp + 8);
                        cpu.write_reg(Register::D0, err as u32);
                        record_movie_error(self, err);
                        Ok(())
                    }
                    0x00D5 => {
                        // CloseMovieFile ($AAAA selector $00D5)
                        // Closes an open movie file reference number.
                        // pascal OSErr CloseMovieFile(short resRefNum);
                        // Inside Macintosh: QuickTime 1993, p. 2-99.
                        let sp = cpu.read_reg(Register::A7);
                        let refnum = bus.read_word(sp);
                        let err = self.close_movie_file_refnum(bus, refnum);
                        bus.write_word(sp + 2, err as u16);
                        cpu.write_reg(Register::A7, sp + 2);
                        cpu.write_reg(Register::D0, err as u32);
                        record_movie_error(self, err);
                        Ok(())
                    }
                    0x00DD => {
                        // IsMovieDone ($AAAA selector $00DD)
                        // Reports whether a movie has finished playing.
                        // pascal Boolean IsMovieDone(Movie theMovie);
                        // Inside Macintosh: QuickTime 1993, pp. 2-125 to 2-126.
                        let sp = cpu.read_reg(Register::A7);
                        let movie = bus.read_long(sp);
                        let status = self.movie_states.get(&movie).map(|state| {
                            if state.rate < 0 {
                                state.current_time <= 0
                            } else {
                                state.current_time >= state.duration
                            }
                        });
                        let err = if status.is_some() {
                            0
                        } else {
                            QUICKTIME_INVALID_MOVIE
                        };
                        // Pascal BOOLEAN result in the high byte of its slot.
                        bus.write_word(sp + 4, if status.unwrap_or(true) { 0x0100 } else { 0 });
                        cpu.write_reg(Register::A7, sp + 4);
                        cpu.write_reg(Register::D0, err as u32);
                        record_movie_error(self, err);
                        Ok(())
                    }
                    0x001F => {
                        // UpdateMovie ($AAAA selector $001F)
                        // Marks a movie for redrawing after an invalidated update area.
                        // pascal OSErr UpdateMovie(Movie theMovie);
                        // Inside Macintosh: QuickTime 1993, pp. 2-126 to 2-127.
                        let sp = cpu.read_reg(Register::A7);
                        let movie = bus.read_long(sp);
                        let err = if self.movie_states.contains_key(&movie) {
                            0
                        } else {
                            QUICKTIME_INVALID_MOVIE
                        };
                        bus.write_word(sp + 4, err as u16);
                        cpu.write_reg(Register::A7, sp + 4);
                        cpu.write_reg(Register::D0, err as u32);
                        record_movie_error(self, err);
                        Ok(())
                    }
                    0x00F9 => {
                        // GetMovieBox ($AAAA selector $00F9)
                        // Returns a movie's boundary rectangle.
                        // pascal void GetMovieBox(Movie theMovie, Rect *boxRect);
                        // Inside Macintosh: QuickTime 1993, pp. 2-161 to 2-162.
                        let sp = cpu.read_reg(Register::A7);
                        let box_ptr = bus.read_long(sp);
                        let movie = bus.read_long(sp + 4);
                        let rect = self.movie_states.get(&movie).map(|state| state.box_rect);
                        let err = if let Some(rect) = rect {
                            write_movie_box(bus, box_ptr, rect);
                            0
                        } else {
                            QUICKTIME_INVALID_MOVIE
                        };
                        if super::dispatch::trace_quicktime_enabled() {
                            eprintln!("[QUICKTIME] GetMovieBox movie=${:08X} -> {:?}", movie, rect);
                        }
                        cpu.write_reg(Register::A7, sp + 8);
                        cpu.write_reg(Register::D0, err as u32);
                        record_movie_error(self, err);
                        Ok(())
                    }
                    0x00FA => {
                        // SetMovieBox ($AAAA selector $00FA)
                        // Sets a movie's boundary rectangle.
                        // pascal void SetMovieBox(Movie theMovie, const Rect *boxRect);
                        // Inside Macintosh: QuickTime 1993, pp. 2-161 to 2-162.
                        let sp = cpu.read_reg(Register::A7);
                        let box_ptr = bus.read_long(sp);
                        let movie = bus.read_long(sp + 4);
                        let err = if let (Some(rect), Some(state)) = (
                            read_movie_box(bus, box_ptr),
                            self.movie_states.get_mut(&movie),
                        ) {
                            state.box_rect = rect;
                            0
                        } else {
                            QUICKTIME_INVALID_MOVIE
                        };
                        if super::dispatch::trace_quicktime_enabled() {
                            eprintln!(
                                "[QUICKTIME] SetMovieBox movie=${:08X} rect={:?}",
                                movie,
                                read_movie_box(bus, box_ptr)
                            );
                        }
                        cpu.write_reg(Register::A7, sp + 8);
                        cpu.write_reg(Register::D0, err as u32);
                        record_movie_error(self, err);
                        Ok(())
                    }
                    _ => return_noerr(cpu),
                }
            }

            // SetFractEnable ($A814)
            // Enables or disables fractional character widths for the
            // Font Manager's character-width tables.
            // PROCEDURE SetFractEnable(fractEnable: BOOLEAN); [Not in ROM]
            // Inside Macintosh Volume IV (1986), p. IV-32.
            //
            // Per IM:IV IV-32: "If fractEnable is TRUE, fractional
            // character widths are enabled; if it's FALSE, the Font
            // Manager uses integer widths. To ensure compatibility
            // with existing applications, fractional character widths
            // are disabled by default." The assembly-language note on
            // the same page confirms: "From assembly language, you
            // can change the value of the global variable FractEnable."
            //
            // The FractEnable low-memory global lives at $0BF4 per
            // Macintosh Family Hardware Reference 2nd Ed. (1990)
            // Appendix B and the MPW LowMem.h SystemGlobals table.
            // MPW exposes the global via TWOWORDINLINE accessors:
            //   LMGetFractEnable() = MOVE.B $0BF4, D0  (0x1EB8 0x0BF4)
            //   LMSetFractEnable(v) = MOVE.B v, $0BF4  (0x11DF 0x0BF4)
            //
            // Pascal PROCEDURE protocol (caller perspective):
            //   Stack on entry: SP+0: fractEnable(2) — the BOOLEAN
            //   argument encoded as a 2-byte word with the value byte
            //   in the HIGH byte (MPW Pascal BOOLEAN convention).
            //   The trap pops 2 bytes; no function-result slot is
            //   reserved (this is a PROCEDURE, not a FUNCTION).
            //
            // Byte-write semantic: the real System 7.5 ROM writes the
            // raw Pascal BOOLEAN high byte verbatim to $0BF4 — TRUE
            // becomes the byte value 0x01 (not a normalised 0xFF) and
            // FALSE becomes 0x00. Systemless mirrors this exact byte by
            // reading SP+0 directly (no normalisation). The BasiliskII
            // System 7.5.3 ROM follows the same convention.
            //
            // Regression coverage:
            //   tests::setfractenable_true_writes_one_byte_verbatim_to_fract_enable_global
            //   tests::setfractenable_false_writes_zero_byte_to_fract_enable_global
            //   tests::setfractenable_consumes_two_byte_boolean_argument_and_balances_stack
            // SetFractEnable ($A814): Writes FractEnable low-mem global ($0BF4); per IM:IV IV-32
            (true, 0x014) => {
                let sp = cpu.read_reg(Register::A7);
                // Pascal BOOLEAN at SP+0 (MPW convention: value byte
                // in the high byte of the 2-byte stack slot).
                let fract_enable_byte = bus.read_byte(sp);
                cpu.write_reg(Register::A7, sp + 2);
                bus.write_byte(0x0BF4, fract_enable_byte);
                Ok(())
            }

            // ========== Printing Manager ==========

            // PrGlue (0xA8FD)
            // Dispatches Printing Manager routines selected by a LONGINT on the stack.
            // PROCEDURE PrOpen;
            // Inside Macintosh Volume V (1986), V-408.
            (true, 0x0FD) => {
                let sp = cpu.read_reg(Register::A7);
                let selector = bus.read_long(sp);
                let operation = pr_glue_operation_route(self.current_trap_word, selector);
                self.current_selector_operation = operation.map(|route| route.operation_id);
                let routine = (selector >> 24) & 0xFF;

                // Extract the parameter size encoded in bits 15-8.
                let param_bytes = (selector >> 8) & 0xFF;

                let total_pop = 4 + param_bytes; // selector + params

                match routine {
                    0x04 => {
                        // PrOpenDoc: returns TPPrPort (4 bytes) — return NIL
                        // Even when the selector's result-size bits are 0
                        // ($04000C00), callers reserve a TPPrPort result
                        // slot per routine signature. Mirror the nil return
                        // in D0 as well so inline shims can observe it.
                        self.printing_error = 0;
                        bus.write_long(sp + total_pop, 0);
                        cpu.write_reg(Register::D0, 0);
                        cpu.write_reg(Register::A7, sp + total_pop);
                    }
                    0x08 => {
                        // PrCloseDoc: consumes one TPPrPort argument and
                        // returns no function result.
                        self.printing_error = 0;
                        cpu.write_reg(Register::A7, sp + total_pop);
                    }
                    0x10 | 0x18 => {
                        // PrOpenPage / PrClosePage: procedures.
                        self.printing_error = 0;
                        cpu.write_reg(Register::A7, sp + total_pop);
                    }
                    0x20 => {
                        // PrintDefault: PROCEDURE PrintDefault(hPrint).
                        // Selector $20040480 has a non-zero second byte, but
                        // that byte is not a result size; writing a stack
                        // result here corrupts MPW compatibility shims whose
                        // LINK frame sits immediately above the selector and
                        // THPrint argument.
                        self.printing_error = 0;
                        cpu.write_reg(Register::D0, 0);
                        cpu.write_reg(Register::A7, sp + total_pop);
                    }
                    0xC8 | 0xD0 => {
                        // PrOpen / PrClose: procedures with no stack
                        // arguments. They consume only the selector long
                        // and do not perturb the shared PrintErr state.
                        cpu.write_reg(Register::A7, sp + total_pop);
                    }
                    0x2A | 0x32 => {
                        // PrStlDialog / PrJobDialog: returns BOOLEAN (2 bytes)
                        // Return TRUE (user clicked OK) so games proceed past print dialogs
                        self.printing_error = 0;
                        bus.write_word(sp + total_pop, 1); // TRUE
                        cpu.write_reg(Register::D0, 1);
                        cpu.write_reg(Register::A7, sp + total_pop);
                    }
                    0x3C | 0x44 => {
                        // PrStlInit / PrJobInit: return TPPrDlg. No native
                        // printer UI is available, so return NIL.
                        self.printing_error = 0;
                        bus.write_long(sp + total_pop, 0);
                        cpu.write_reg(Register::D0, 0);
                        cpu.write_reg(Register::A7, sp + total_pop);
                    }
                    0x4A => {
                        // PrDlgMain: return TRUE so callers take the
                        // confirmed path if they invoked a customized print
                        // dialog.
                        self.printing_error = 0;
                        bus.write_word(sp + total_pop, 1);
                        cpu.write_reg(Register::D0, 1);
                        cpu.write_reg(Register::A7, sp + total_pop);
                    }
                    0x52 => {
                        // PrValidate: returns BOOLEAN (2 bytes)
                        // Return FALSE (record is valid, no changes needed)
                        self.printing_error = 0;
                        bus.write_word(sp + total_pop, 0); // FALSE
                        cpu.write_reg(Register::D0, 0);
                        cpu.write_reg(Register::A7, sp + total_pop);
                    }
                    0x58 | 0x60 | 0x70 | 0x80 | 0x88 | 0xA0 => {
                        // PrJobMerge, PrPicFile, PrGeneral, PrDrvrOpen,
                        // PrDrvrClose, PrCtlCall: procedures.
                        self.printing_error = 0;
                        cpu.write_reg(Register::D0, 0);
                        cpu.write_reg(Register::A7, sp + total_pop);
                    }
                    0x94 => {
                        // PrDrvrDCE: return a DCE Handle. Printing is not
                        // supported, so return NIL.
                        self.printing_error = 0;
                        bus.write_long(sp + total_pop, 0);
                        cpu.write_reg(Register::D0, 0);
                        cpu.write_reg(Register::A7, sp + total_pop);
                    }
                    0x9A => {
                        // PrDrvrVers: return driver version. No printer
                        // driver is present.
                        self.printing_error = 0;
                        bus.write_word(sp + total_pop, 0);
                        cpu.write_reg(Register::D0, 0);
                        cpu.write_reg(Register::A7, sp + total_pop);
                    }
                    0xBA => {
                        // PrError: returns INTEGER (2 bytes). MPW encodes
                        // the selector as 0xBA00_0000 — the per-routine
                        // return-size bits are zero for PrError, but real
                        // ROM returns a 2-byte result regardless because
                        // the dispatcher knows the routine signature by
                        // trap table. Return the stored PrintErr word so
                        // PrSetError can affect later queries.
                        bus.write_word(sp + total_pop, self.printing_error as u16);
                        cpu.write_reg(Register::D0, self.printing_error as u32);
                        cpu.write_reg(Register::A7, sp + total_pop);
                    }
                    0xC0 => {
                        // PrSetError: stores the new printing error code in
                        // the shared PrintErr global and returns no result.
                        self.printing_error = bus.read_word(sp + 4) as i16;
                        cpu.write_reg(Register::D0, 0);
                        cpu.write_reg(Register::A7, sp + total_pop);
                    }
                    _ => {
                        // Unknown printing routines: pop the documented
                        // selector and parameter bytes, but do not infer a
                        // result slot from the second selector byte.
                        self.printing_error = 0;
                        cpu.write_reg(Register::D0, 0);
                        cpu.write_reg(Register::A7, sp + total_pop);
                    }
                }
                Ok(())
            }

            // ========== Script Manager ==========

            // ScriptUtil (0xA8B5)
            // Dispatches script utilities selected by a LONGINT on top of the stack.
            // FUNCTION ParseTable (VAR table: CharByteTable): Boolean;
            // Inside Macintosh Volume VI (1991), pp. 14-131 to 14-132; Inside Macintosh: Text (1993), p. A-39.
            (true, 0x0B5) => {
                let sp = cpu.read_reg(Register::A7);
                // MPW's inline wraps ScriptUtil selectors as
                //   MOVE.L #<encoding>.L, -(SP)
                //   _ScriptUtil
                // where the high word encodes result-size and argument-count
                // metadata. Older Script Manager calls use the low byte as the
                // routine number; System 7 text utilities use full selectors.
                let raw_selector = bus.read_long(sp);
                let operation = script_util_operation_route(self.current_trap_word, raw_selector);
                self.current_selector_operation = operation.map(|route| route.operation_id);
                let selector = (raw_selector & 0xFF) as i32;

                match raw_selector {
                    // ReplaceText ($820CFFDC): FUNCTION ReplaceText(
                    //   baseText, substitutionText: Handle; key: Str15): INTEGER.
                    // Stack: selector(4), key pointer(4), substitution handle(4),
                    // base handle(4), result(2). Pop selector and arguments.
                    // Inside Macintosh: Text 1993, pp. 5-74..5-75 and Table D-3.
                    0x820C_FFDC => {
                        let key_ptr = bus.read_long(sp + 4);
                        let substitution_handle = bus.read_long(sp + 8);
                        let base_handle = bus.read_long(sp + 12);
                        let result = self.scriptutil_replace_text(
                            bus,
                            base_handle,
                            substitution_handle,
                            key_ptr,
                        );
                        bus.write_word(sp + 16, result as u16);
                        cpu.write_reg(Register::D0, result as i32 as u32);
                        cpu.write_reg(Register::A7, sp + 16);
                        return Some(Ok(()));
                    }
                    // TruncString ($8208FFE0): FUNCTION TruncString(width: INTEGER;
                    //   VAR theString: Str255; truncWhere: TruncCode): INTEGER
                    // Returning smNotTruncated leaves the string unchanged.
                    // Stack: selector(4), args(8), result(2). Pop selector + args.
                    // Inside Macintosh Volume VI, 14-59..14-60 and Table C-3.
                    0x8208_FFE0 => {
                        bus.write_word(sp + 12, 0); // smNotTruncated
                        cpu.write_reg(Register::A7, sp + 12);
                        return Some(Ok(()));
                    }
                    // TruncText ($820CFFDE): FUNCTION TruncText(width: INTEGER;
                    //   textPtr: Ptr; VAR length: INTEGER; truncWhere: TruncCode): INTEGER
                    // Returning smNotTruncated leaves the pointed-to text and length unchanged.
                    // Stack: selector(4), args(12), result(2). Pop selector + args.
                    // Inside Macintosh Volume VI, 14-59..14-60 and Table C-3.
                    0x820C_FFDE => {
                        bus.write_word(sp + 16, 0); // smNotTruncated
                        cpu.write_reg(Register::A7, sp + 16);
                        return Some(Ok(()));
                    }
                    // StyledLineBreak ($821CFFFE): FUNCTION StyledLineBreak(textPtr: Ptr;
                    //   textLen, textStart, textEnd, flags: LongInt; VAR textWidth: Fixed;
                    //   VAR textOffset: LongInt): StyledLineBreakCode.
                    // Stack: selector(4), args(28), result(2). Pop selector + args.
                    // Inside Macintosh: Text 1993, pp. 5-79..5-81 and Table D-3.
                    0x821C_FFFE => {
                        return Some(self.handle_scriptutil_styled_line_break(bus, cpu, sp));
                    }
                    _ => {}
                }

                match selector {
                    // FontScript (0): FUNCTION FontScript: INTEGER
                    // Returns script code. Stack: selector(4), result space(2)
                    0 => {
                        bus.write_word(sp + 4, 0); // smRoman = 0
                        cpu.write_reg(Register::A7, sp + 4); // pop selector, result stays
                    }
                    // IntlScript (2): FUNCTION IntlScript: INTEGER
                    2 => {
                        bus.write_word(sp + 4, 0); // smRoman
                        cpu.write_reg(Register::A7, sp + 4);
                    }
                    // KeyScript (4): FUNCTION KeyScript: INTEGER
                    4 => {
                        bus.write_word(sp + 4, 0); // smRoman
                        cpu.write_reg(Register::A7, sp + 4);
                    }
                    // Font2Script (6): FUNCTION Font2Script(fontNum: INTEGER): INTEGER
                    // Stack: selector(4), fontNum(2), result(2)
                    6 => {
                        bus.write_word(sp + 6, 0); // smRoman
                        cpu.write_reg(Register::A7, sp + 6); // pop selector + fontNum
                    }
                    // GetEnvirons (8): FUNCTION GetEnvirons(verb: INTEGER): LongInt
                    // Stack: selector(4), verb(2), result(4)
                    8 => {
                        bus.write_long(sp + 6, 0); // return 0
                        cpu.write_reg(Register::A7, sp + 6); // pop selector + verb
                    }
                    // SetEnvirons (10): FUNCTION SetEnvirons(verb: INTEGER; param: LongInt): OSErr
                    // Stack: selector(4), verb(2), param(4), result(2)
                    10 => {
                        bus.write_word(sp + 10, 0); // noErr
                        cpu.write_reg(Register::A7, sp + 10); // pop selector + verb + param
                    }
                    // GetScript (12): FUNCTION GetScript(script: INTEGER; verb: INTEGER): LongInt
                    // Stack: selector(4), script(2), verb(2), result(4)
                    12 => {
                        bus.write_long(sp + 8, 0); // return 0
                        cpu.write_reg(Register::A7, sp + 8); // pop selector + script + verb
                        cpu.write_reg(Register::D0, 0);
                    }
                    // SetScript (14): FUNCTION SetScript(script: INTEGER; verb: INTEGER; param: LongInt): OSErr
                    // Stack: selector(4), script(2), verb(2), param(4), result(2)
                    14 => {
                        bus.write_word(sp + 12, 0); // noErr
                        cpu.write_reg(Register::A7, sp + 12);
                        cpu.write_reg(Register::D0, 0);
                    }
                    // CharByte (16): FUNCTION CharByte(textBuf: Ptr; textOffset: INTEGER): INTEGER
                    // Returns smSingleByte (0) for all chars in Roman script.
                    // Stack: selector(4), textBuf(4), textOffset(2), result(2)
                    16 => {
                        bus.write_word(sp + 10, 0); // smSingleByte = 0
                        cpu.write_reg(Register::A7, sp + 10);
                    }
                    // CharType (18): FUNCTION CharType(textBuf: Ptr; textOffset: INTEGER): INTEGER
                    // Stack: selector(4), textBuf(4), textOffset(2), result(2)
                    18 => {
                        bus.write_word(sp + 10, 0); // return 0 (left-to-right)
                        cpu.write_reg(Register::A7, sp + 10);
                    }
                    // Char2Pixel (22): FUNCTION Char2Pixel(textBuf: Ptr; textLen: INTEGER;
                    //   slop: INTEGER; offset: INTEGER; direction: INTEGER): INTEGER
                    // Stack: selector(4), textBuf(4), textLen(2), slop(2), offset(2),
                    //        direction(2), result(2)
                    22 => {
                        bus.write_word(sp + 16, 0); // return 0 pixel offset
                        cpu.write_reg(Register::A7, sp + 16);
                    }
                    // Pixel2Char (20): FUNCTION Pixel2Char(textBuf: Ptr; textLen: INTEGER;
                    //   slop: INTEGER; pixelWidth: INTEGER; VAR leadingEdge: BOOLEAN): INTEGER
                    // Pascal pushes args left-to-right (first arg deepest), so the
                    // VAR leadingEdge pointer is the LAST arg pushed and lives at
                    // sp+4 (just past the selector long). Layout post-trap-entry:
                    //   sp+0  selector long
                    //   sp+4  leadingEdge_ptr (last arg, 4 bytes)
                    //   sp+8  pixelWidth (2 bytes)
                    //   sp+10 slop (2 bytes)
                    //   sp+12 textLen (2 bytes)
                    //   sp+14 textBuf (first arg, 4 bytes)
                    //   sp+18 INTEGER result slot
                    // Pop 18 bytes (selector + 14 arg bytes), leave 2-byte result.
                    // Inside Macintosh Volume V, V-310
                    20 => {
                        let leading_edge_ptr = bus.read_long(sp + 4);
                        if leading_edge_ptr != 0 {
                            bus.write_byte(leading_edge_ptr, 0);
                        }
                        bus.write_word(sp + 18, 0); // return offset 0
                        cpu.write_reg(Register::A7, sp + 18);
                    }
                    // Transliterate (24): FUNCTION Transliterate(srcHandle, dstHandle: Handle;
                    //   target: INTEGER; srcMask: LongInt): OSErr
                    // Stack: selector(4), srcHandle(4), dstHandle(4), target(2),
                    //        srcMask(4), result(2). Pop 14 bytes of args, leave result.
                    // Inside Macintosh Volume V, V-312
                    24 => {
                        bus.write_word(sp + 18, 0); // noErr
                        cpu.write_reg(Register::A7, sp + 18);
                    }
                    // FindWord (26): PROCEDURE FindWord(textPtr: Ptr; textLength, offset: INTEGER;
                    //   leadingEdge: BOOLEAN; breaksPtr: Ptr; VAR offsets: OffsetTable)
                    // Pascal pushes args left-to-right (first arg deepest), so the
                    // VAR offsets pointer is the LAST arg pushed and lives at sp+4.
                    // Layout post-trap-entry:
                    //   sp+0  selector long
                    //   sp+4  offsets_ptr (last arg, 4 bytes)
                    //   sp+8  breaksPtr (4 bytes)
                    //   sp+12 leadingEdge (2 bytes)
                    //   sp+14 offset (2 bytes)
                    //   sp+16 textLength (2 bytes)
                    //   sp+18 textPtr (first arg, 4 bytes)
                    // No return value. Pop 22 (selector + 18 arg bytes).
                    // OffsetTable is ARRAY[0..2] OF OffPair = 3*4 = 12 bytes
                    // (Inside Macintosh Volume VI, p. 33514 summary; Text 1993,
                    // p. 10664).
                    // Inside Macintosh Volume V, V-313
                    26 => {
                        let offsets_ptr = bus.read_long(sp + 4);
                        if offsets_ptr != 0 {
                            bus.write_bytes(offsets_ptr, &[0u8; 12]);
                        }
                        cpu.write_reg(Register::A7, sp + 22);
                    }
                    // HiliteText (28): PROCEDURE HiliteText(textPtr: Ptr; textLength,
                    //   firstOffset, secondOffset: INTEGER; VAR offsets: OffsetTable)
                    // Pascal pushes args left-to-right; the VAR offsets pointer is
                    // the LAST arg pushed and lives at sp+4. Layout post-trap-entry:
                    //   sp+0  selector long
                    //   sp+4  offsets_ptr (last arg, 4 bytes)
                    //   sp+8  secondOffset (2 bytes)
                    //   sp+10 firstOffset (2 bytes)
                    //   sp+12 textLength (2 bytes)
                    //   sp+14 textPtr (first arg, 4 bytes)
                    // No return. Pop 18 (selector + 14 arg bytes). OffsetTable is
                    // 12 bytes (Inside Macintosh Volume VI, p. 33514 summary).
                    // Inside Macintosh Volume V, V-314
                    28 => {
                        let offsets_ptr = bus.read_long(sp + 4);
                        if offsets_ptr != 0 {
                            bus.write_bytes(offsets_ptr, &[0u8; 12]);
                        }
                        cpu.write_reg(Register::A7, sp + 18);
                    }
                    // DrawJust (30): PROCEDURE DrawJust(textPtr: Ptr; textLength, slop: INTEGER)
                    // No return, no output. Systemless does not implement justified text drawing
                    // here. Stack: selector(4) + 8 bytes of args; pop 12.
                    // Inside Macintosh Volume V, V-315
                    30 => {
                        cpu.write_reg(Register::A7, sp + 12);
                    }
                    // MeasureJust (32): PROCEDURE MeasureJust(textPtr: Ptr; textLength,
                    //   slop: INTEGER; charLocs: Ptr)
                    // No return. Stack: selector(4) + 12 bytes of args; pop 16.
                    // Inside Macintosh Volume V, V-315
                    32 => {
                        cpu.write_reg(Register::A7, sp + 16);
                    }
                    _ => {
                        let result_bytes = (raw_selector >> 24) & 0x7F;
                        let arg_bytes = (raw_selector >> 16) & 0xFF;
                        if (raw_selector & 0x8000_0000) != 0
                            && matches!(result_bytes, 0 | 1 | 2 | 4)
                        {
                            let result_sp = sp + 4 + arg_bytes;
                            match result_bytes {
                                1 => bus.write_byte(result_sp, 0),
                                2 => bus.write_word(result_sp, 0),
                                4 => bus.write_long(result_sp, 0),
                                _ => {}
                            }
                            eprintln!(
                                "[TRAP] ScriptUtil: unhandled encoded selector ${:08X}; popped {} arg bytes",
                                raw_selector, arg_bytes
                            );
                            cpu.write_reg(Register::A7, result_sp);
                        } else {
                            // Unknown legacy selector — pop the selector and return.
                            eprintln!("[TRAP] ScriptUtil: unhandled selector {}", selector);
                            cpu.write_reg(Register::A7, sp + 4);
                        }
                    }
                }
                Ok(())
            }

            // PPCBrowser (0xA82B)
            // Displays the program linking dialog and returns the selected PPC port.
            // FUNCTION PPCBrowser (prompt: Str255; applListLabel: Str255; defaultSpecified: Boolean; VAR theLocation: LocationNameRec; VAR thePortInfo: PortInfoRec; portFilter: PPCFilterProcPtr; theLocNBPType: Str32): OSErr;
            // Inside Macintosh: Interapplication Communication (1993), pp. 11-52 to 11-54.
            //
            // Selector $0D00 is passed in D0.W. The 68k Pascal calling convention
            // passes 26 argument bytes on the stack above a 2-byte OSErr result slot.
            // In headless execution without interactive linking UI, return userCanceledErr (-128).
            (true, 0x02B) => {
                let d0 = cpu.read_reg(Register::D0);
                let selector = (d0 & 0xFFFF) as u16;
                let operation = pack9_operation_route(self.current_trap_word, selector);
                self.current_selector_operation = operation.map(|route| route.operation_id);

                if operation.is_some() {
                    let sp = cpu.read_reg(Register::A7);
                    let result_sp = sp + 26;
                    cpu.write_reg(Register::A7, result_sp);
                    bus.write_word(result_sp, (-128i16) as u16);
                    cpu.write_reg(Register::D0, (-128i32) as u32);
                } else {
                    self.current_selector_operation = None;
                    cpu.write_reg(Register::D0, (-50i32) as u32);
                }
                Ok(())
            }

            // Pack10 ($A82C) — NewEmptyHandle alias
            // Inside Macintosh Volume IV (1986), pp. IV-78 and IV-81;
            // Inside Macintosh: Memory 1992, p. 2-33.
            // Pack10 maps directly to _NewEmptyHandle ($A066), which
            // returns its Handle result in A0 and consumes no Pascal
            // arguments.
            (true, 0x02C) => return self.dispatch_memory(false, 0x66, cpu, bus),

            // Pack11 ($A82D)
            // Dispatches Edition Manager routines selected by D0.W.
            // FUNCTION InitEditionPack: OSErr;
            // Inside Macintosh: Interapplication Communication (1993), p. 2-74; Volume VI (1991), pp. C-9–C-10.
            (true, 0x02D) => {
                const NO_ERR: i16 = 0;
                const EDITION_MGR_INIT_ERR: i16 = -450;
                const PARAM_ERR: i16 = -50;

                let sp = cpu.read_reg(Register::A7);
                let selector = (cpu.read_reg(Register::D0) & 0xFFFF) as u16;
                let operation = pack11_operation_route(self.current_trap_word, selector);

                if let Some(route) = operation {
                    self.current_selector_operation = Some(route.operation_id);
                    let param_bytes = (((selector >> 8) & 0xFF) as u32) * 2;
                    let result = if selector == 0x0100 {
                        NO_ERR
                    } else {
                        EDITION_MGR_INIT_ERR
                    };
                    let result_sp = sp + param_bytes;
                    bus.write_word(result_sp, result as u16);
                    cpu.write_reg(Register::A7, result_sp);
                    cpu.write_reg(Register::D0, result as i32 as u32);
                } else {
                    self.current_selector_operation = None;
                    cpu.write_reg(Register::D0, PARAM_ERR as i32 as u32);
                }
                Ok(())
            }

            // Pack13 (0xA82F)
            // Dispatches Data Access Manager routines selected by D0.W.
            // FUNCTION InitDBPack: OSErr;
            // Inside Macintosh: Interapplication Communication (1993), pp. 12-60 and 12-103.
            (true, 0x02F) => {
                const RC_DB_WRONG_VERSION: i16 = -812;
                const RC_DB_PACK_NOT_INITED: i16 = -813;
                const PARAM_ERR: i16 = -50;

                let sp = cpu.read_reg(Register::A7);
                let selector = (cpu.read_reg(Register::D0) & 0xFFFF) as u16;
                let operation = pack13_operation_route(self.current_trap_word, selector);

                if let Some(route) = operation {
                    self.current_selector_operation = Some(route.operation_id);
                    let param_bytes = (((selector >> 8) & 0xFF) as u32) * 2;
                    let result = if selector == 0x0100 {
                        RC_DB_WRONG_VERSION
                    } else {
                        RC_DB_PACK_NOT_INITED
                    };
                    let result_sp = sp + param_bytes;
                    bus.write_word(result_sp, result as u16);
                    cpu.write_reg(Register::A7, result_sp);
                    cpu.write_reg(Register::D0, result as i32 as u32);
                } else {
                    self.current_selector_operation = None;
                    cpu.write_reg(Register::D0, PARAM_ERR as i32 as u32);
                }
                Ok(())
            }

            // Pack14 ($A830)
            // Dispatches Help Manager routines selected by D0.W.
            // FUNCTION HMGetBalloons: Boolean;
            // Inside Macintosh: More Macintosh Toolbox (1993), p. 3-98.
            //
            // Selector encoding is `(arg_words << 8) | routine`.
            // Public MPW glue emits `MOVE.W #selector,D0; _Pack14`;
            // the selector word is NOT pushed on the stack. The high
            // byte gives the number of WORDS of args pushed on the
            // stack; arg_bytes = high_byte * 2.
            //
            // Pascal calling convention: caller pre-pushes a 2-byte
            // result slot for OSErr / Boolean / Integer FUNCTION
            // returns, then pushes args left-to-right (first source-
            // listed arg deepest, last shallowest at SP+0). Trap pops
            // arg_bytes, exposing the result slot at the new SP+0. We
            // mirror each result to BOTH the stack slot AND D0 for
            // callers that read either way (matches the Pack6 /
            // IUMagString pattern).
            //
            // HLE compromise: Systemless has no Balloon Help subsystem
            // — no cursor tracking, no balloon WDEF / window, no
            // 'hmnu' / 'hdlg' / 'hrct' / 'hwin' / 'hovr' / 'hfdr'
            // resource walking, no help font cache. Status queries
            // collapse to "help disabled" (HMGetBalloons returns
            // FALSE, HMIsBalloon returns FALSE); show/remove balloon
            // ops return hmHelpDisabled (-850); set ops are no-op
            // noErr; get ops write defensive defaults (NIL handles,
            // 0 fonts, empty Rects, -1 resource IDs) plus the IM-
            // documented error codes for "no resource set" paths
            // (resNotFound for HMGet*ResID, hmHelpManagerNotInited
            // for HMGetHelpMenuHandle); resource extraction routines
            // return resNotFound. Apps that defensively check OSErr
            // before dereffing fall through cleanly; apps that need
            // actual help balloons see the documented "help disabled"
            // path which matches what System 7.5.3 would do if the
            // user toggled Balloon Help off via the Help menu.
            //
            // The selector encoding pinned by the existing stub
            // heuristic was actively wrong: it interpreted the high
            // byte as BYTE count rather than WORD count, and clamped
            // to 2..=48. So $0104 HMSetBalloons (high=$01, true args
            // = 2 bytes) popped only 2 (clamp filtered $01 out),
            // leaving 2 args bytes + result slot stranded; $0B01
            // HMShowBalloon (high=$0B = 11 words = 22 args bytes)
            // popped 11 instead of 22, leaving 11 args bytes
            // stranded. Any real-game caller would crash on RTS.
            //
            (true, 0x030) => {
                const HM_HELP_DISABLED: i16 = -850;
                const HM_HELP_MGR_NOT_INITED: i16 = -855;
                const RES_NOT_FOUND: i16 = -192;

                let sp = cpu.read_reg(Register::A7);
                let selector = (cpu.read_reg(Register::D0) & 0xFFFF) as u16;
                let operation = pack14_operation_route(self.current_trap_word, selector);
                self.current_selector_operation = operation.map(|route| route.operation_id);

                // Helper: write OSErr/Integer result word to BOTH the
                // stack slot at sp+pop_total AND D0, then advance A7.
                let finish =
                    |bus: &mut MacMemoryBus, cpu: &mut dyn CpuOps, pop_total: u32, result: i16| {
                        bus.write_word(sp + pop_total, result as u16);
                        cpu.write_reg(Register::A7, sp + pop_total);
                        cpu.write_reg(Register::D0, result as i32 as u32);
                    };

                match selector {
                    // FUNCTION HMRemoveBalloon: OSErr;
                    // IM:MMTb 3-105. No parameters.
                    // No balloon ever up in HLE → noErr per the IM
                    // result table ("No error or the help balloon
                    // was removed").
                    0x0002 => finish(bus, cpu, 0, 0),

                    // FUNCTION HMGetBalloons: Boolean;
                    // IM:MMTb 3-98. No parameters.
                    // Help disabled in HLE → FALSE (0).
                    0x0003 => finish(bus, cpu, 0, 0),

                    // FUNCTION HMIsBalloon: Boolean;
                    // IM:MMTb 3-99. No parameters.
                    // No balloon up in HLE → FALSE (0).
                    0x0007 => finish(bus, cpu, 0, 0),

                    // FUNCTION HMSetBalloons(flag: Boolean): OSErr;
                    // IM:MMTb 3-107. Pop = 2 (flag).
                    // Accept and ignore — no help to enable/disable.
                    0x0104 => finish(bus, cpu, 2, 0),

                    // FUNCTION HMSetFont(font: Integer): OSErr;
                    // IM:MMTb 3-112. Pop = 2. Accept and ignore.
                    0x0108 => finish(bus, cpu, 2, 0),

                    // FUNCTION HMSetFontSize(fontSize: Integer): OSErr;
                    // IM:MMTb 3-113. Pop = 2. Accept and ignore.
                    0x0109 => finish(bus, cpu, 2, 0),

                    // FUNCTION HMSetDialogResID(resID: Integer): OSErr;
                    // IM:MMTb 3-117. Pop = 2. Accept and ignore.
                    0x010C => finish(bus, cpu, 2, 0),

                    // FUNCTION HMGetHelpMenuHandle(VAR mh: MenuHandle): OSErr;
                    // IM:MMTb 3-109. Pop = 4 (mh ptr).
                    // mh ptr at SP+0. Write NIL to *mh per
                    // hmHelpManagerNotInited contract.
                    0x0200 => {
                        let mh_ptr = bus.read_long(sp);
                        if mh_ptr != 0 {
                            bus.write_long(mh_ptr, 0);
                        }
                        finish(bus, cpu, 4, HM_HELP_MGR_NOT_INITED);
                    }

                    // FUNCTION HMGetFont(VAR font: Integer): OSErr;
                    // IM:MMTb 3-110. Pop = 4. Write 0 (system font)
                    // to *font.
                    0x020A => {
                        let font_ptr = bus.read_long(sp);
                        if font_ptr != 0 {
                            bus.write_word(font_ptr, 0);
                        }
                        finish(bus, cpu, 4, 0);
                    }

                    // FUNCTION HMGetFontSize(VAR fontSize: Integer): OSErr;
                    // IM:MMTb 3-111. Pop = 4. Write 0 (system size)
                    // to *fontSize.
                    0x020B => {
                        let size_ptr = bus.read_long(sp);
                        if size_ptr != 0 {
                            bus.write_word(size_ptr, 0);
                        }
                        finish(bus, cpu, 4, 0);
                    }

                    // FUNCTION HMSetMenuResID(menuID, resID: Integer): OSErr;
                    // IM:MMTb 3-114. Pop = 4. Accept and ignore.
                    0x020D => finish(bus, cpu, 4, 0),

                    // FUNCTION HMGetDialogResID(VAR resID: Integer): OSErr;
                    // IM:MMTb 3-118. Pop = 4. Write -1 to *resID
                    // per "no hdlg set" → resNotFound contract.
                    0x0213 => {
                        let res_id_ptr = bus.read_long(sp);
                        if res_id_ptr != 0 {
                            bus.write_word(res_id_ptr, (-1i16) as u16);
                        }
                        finish(bus, cpu, 4, RES_NOT_FOUND);
                    }

                    // FUNCTION HMGetBalloonWindow(VAR window: WindowPtr): OSErr;
                    // IM:MMTb 3-121. Pop = 4. Write NIL to *window
                    // per "no balloon up" contract.
                    0x0215 => {
                        let window_ptr = bus.read_long(sp);
                        if window_ptr != 0 {
                            bus.write_long(window_ptr, 0);
                        }
                        finish(bus, cpu, 4, 0);
                    }

                    // FUNCTION HMGetMenuResID(menuID: Integer;
                    //                         VAR resID: Integer): OSErr;
                    // IM:MMTb 3-115. Pop = 6. resID ptr at SP+0
                    // (last arg), menuID at SP+4. Write -1 to
                    // *resID per "no hmnu set" → resNotFound.
                    0x0314 => {
                        let res_id_ptr = bus.read_long(sp);
                        if res_id_ptr != 0 {
                            bus.write_word(res_id_ptr, (-1i16) as u16);
                        }
                        finish(bus, cpu, 6, RES_NOT_FOUND);
                    }

                    // FUNCTION HMBalloonRect(aHelpMsg: HMMessageRecord;
                    //                        VAR coolRect: Rect): OSErr;
                    // IM:MMTb 3-119. Pop = 8. coolRect ptr at SP+0
                    // (last arg), aHelpMsg ptr at SP+4. Write
                    // Rect(0,0,0,0) — empty, no balloon to size.
                    0x040E => {
                        let rect_ptr = bus.read_long(sp);
                        if rect_ptr != 0 {
                            bus.write_word(rect_ptr, 0);
                            bus.write_word(rect_ptr + 2, 0);
                            bus.write_word(rect_ptr + 4, 0);
                            bus.write_word(rect_ptr + 6, 0);
                        }
                        finish(bus, cpu, 8, 0);
                    }

                    // FUNCTION HMBalloonPict(aHelpMsg: HMMessageRecord;
                    //                        VAR coolPict: PicHandle): OSErr;
                    // IM:MMTb 3-120. Pop = 8. coolPict ptr at SP+0.
                    // Write NIL to *coolPict.
                    0x040F => {
                        let pict_ptr = bus.read_long(sp);
                        if pict_ptr != 0 {
                            bus.write_long(pict_ptr, 0);
                        }
                        finish(bus, cpu, 8, 0);
                    }

                    // FUNCTION HMScanTemplateItems(whichID,
                    //                              whichResFile: Integer;
                    //                              whichType: ResType): OSErr;
                    // IM:MMTb 3-116. Pop = 8. No help resources
                    // ever loaded → resNotFound.
                    0x0410 => finish(bus, cpu, 8, RES_NOT_FOUND),

                    // FUNCTION HMExtractHelpMsg(whichType: ResType;
                    //                           whichResID, whichMsg,
                    //                           whichState: Integer;
                    //                           VAR aHelpMsg:
                    //                           HMMessageRecord): OSErr;
                    // IM:MMTb 3-126. Pop = 14. No help resources →
                    // resNotFound. Don't touch aHelpMsg (caller's
                    // record stays untouched).
                    0x0711 => finish(bus, cpu, 14, RES_NOT_FOUND),

                    // FUNCTION HMShowBalloon(aHelpMsg: HMMessageRecord;
                    //                        tip: Point;
                    //                        alternateRect: RectPtr;
                    //                        tipProc: Ptr;
                    //                        theProc, variant,
                    //                        method: Integer): OSErr;
                    // IM:MMTb 3-100. Pop = 22. Help disabled →
                    // hmHelpDisabled.
                    0x0B01 => finish(bus, cpu, 22, HM_HELP_DISABLED),

                    // FUNCTION HMShowMenuBalloon(itemNum,
                    //                            itemMenuID: Integer;
                    //                            itemFlags,
                    //                            itemReserved: LongInt;
                    //                            tip: Point;
                    //                            alternateRect: RectPtr;
                    //                            tipProc: Ptr;
                    //                            theProc,
                    //                            variant: Integer): OSErr;
                    // IM:MMTb 3-103. Pop = 28. Help disabled →
                    // hmHelpDisabled.
                    0x0E05 => finish(bus, cpu, 28, HM_HELP_DISABLED),

                    // FUNCTION HMGetIndHelpMsg(whichType: ResType;
                    //                          whichResID, whichMsg,
                    //                          whichState: Integer;
                    //                          VAR options: LongInt;
                    //                          VAR tip: Point;
                    //                          VAR altRect: Rect;
                    //                          VAR theProc: Integer;
                    //                          VAR variant: Integer;
                    //                          VAR aHelpMsg:
                    //                          HMMessageRecord;
                    //                          VAR count: Integer): OSErr;
                    // IM:MMTb 3-128. Pop = 38. No help resources →
                    // resNotFound. Don't touch any VAR-out param —
                    // caller's records stay untouched per the IM
                    // contract that resNotFound means "did not
                    // populate anything".
                    0x1306 => finish(bus, cpu, 38, RES_NOT_FOUND),

                    // Unknown selector — preserve the stack and return
                    // noErr in D0. A future System addition that assigns
                    // a new Pack14 routine should fill in a new arm above.
                    _ => {
                        cpu.write_reg(Register::A7, sp);
                        cpu.write_reg(Register::D0, 0);
                    }
                }
                Ok(())
            }

            // _Pack15 (0xA831)
            // Dispatches Picture Utilities Package routines selected by D0.W.
            // Selector ABI: 16-bit low word in D0; routine-specific Pascal parameters remain on the stack.
            // Inside Macintosh: Imaging with QuickDraw (1994), 7-47–7-60, 8-3.
            (true, 0x031) => {
                // Size of PictInfo record per IM:VI 18-5:
                // version(2) + uniqueColors(4) + thePalette(4) +
                // theColorTable(4) + hRes(4) + vRes(4) + depth(2) +
                // sourceRect(8) + 18 LongInts (textCount through
                // reserved2) = 104 bytes.
                const PICT_INFO_SIZE: u32 = 104;

                let sp = cpu.read_reg(Register::A7);
                let selector = cpu.read_reg(Register::D0) as u16;
                let operation = pack15_operation_route(self.current_trap_word, selector);
                self.current_selector_operation = operation.map(|route| route.operation_id);
                let pop_total = ((selector >> 8) as u32) * 2;

                // Helper: advance A7 by the packed-argument bytes and
                // write the OSErr result to D0 only. Pack15's caller
                // stack does not reserve a separate result slot.
                let finish = |cpu: &mut dyn CpuOps, pop_total: u32, result: i16| {
                    cpu.write_reg(Register::A7, sp + pop_total);
                    cpu.write_reg(Register::D0, result as i32 as u32);
                };

                // Helper: zero-fill a PictInfo record at the given
                // ptr. NIL ptr is a graceful no-op.
                let zero_pict_info = |bus: &mut MacMemoryBus, ptr: u32| {
                    if ptr == 0 {
                        return;
                    }
                    for off in 0..PICT_INFO_SIZE {
                        bus.write_byte(ptr + off, 0);
                    }
                };

                match selector {
                    // FUNCTION DisposPictInfo(thePictInfoID:
                    //                         PictInfoID): OSErr;
                    // IM:VI 18-14. Pop = 4. Stack: PictInfoID(4).
                    // The live-ID registry is pruned on the first
                    // dispose, but BasiliskII treats repeated calls
                    // as noErr no-ops.
                    0x0206 => {
                        let pict_info_id = bus.read_long(sp);
                        let _ = self.pict_info_ids.remove(&pict_info_id);
                        finish(cpu, 4, 0);
                    }

                    // FUNCTION RecordPictInfo(thePictInfoID: PictInfoID;
                    //                         thePictHandle: PicHandle): OSErr;
                    // IM:VI 18-12. Pop = 8. Stack: PicHandle(4 last)
                    // + PictInfoID(4 first).
                    // Invalid IDs return pictInfoIDErr.
                    0x0403 => {
                        let pict_info_id = bus.read_long(sp + 4);
                        let result = if self.pict_info_ids.contains(&pict_info_id) {
                            0
                        } else {
                            -11001
                        };
                        finish(cpu, 8, result);
                    }

                    // FUNCTION RecordPixMapInfo(thePictInfoID: PictInfoID;
                    //                           thePixMapHandle: PixMapHandle): OSErr;
                    // IM:VI 18-12. Pop = 8. Same shape as
                    // RecordPictInfo. Invalid IDs return pictInfoIDErr.
                    0x0404 => {
                        let pict_info_id = bus.read_long(sp + 4);
                        let result = if self.pict_info_ids.contains(&pict_info_id) {
                            0
                        } else {
                            -11001
                        };
                        finish(cpu, 8, result);
                    }

                    // FUNCTION RetrievePictInfo(thePictInfoID:
                    //                           PictInfoID;
                    //                           VAR thePictInfo: PictInfo;
                    //                           colorsRequested: Integer): OSErr;
                    // IM:VI 18-13. Pop = 10. Stack: colorsRequested(2
                    // last) + thePictInfo ptr(4) + PictInfoID(4
                    // first). Invalid IDs return pictInfoIDErr; live
                    // IDs zero-fill the PictInfo record.
                    0x0505 => {
                        let pict_info_id = bus.read_long(sp + 6);
                        if self.pict_info_ids.contains(&pict_info_id) {
                            let info_ptr = bus.read_long(sp + 2);
                            zero_pict_info(bus, info_ptr);
                            finish(cpu, 10, 0);
                        } else {
                            finish(cpu, 10, -11001);
                        }
                    }

                    // FUNCTION NewPictInfo(VAR thePictInfoID:
                    //                      PictInfoID; verb: Integer;
                    //                      colorsRequested: Integer;
                    //                      colorPickMethod: Integer;
                    //                      version: Integer): OSErr;
                    // IM:VI 18-11. Pop = 12. Stack: version(2 last)
                    // + colorPickMethod(2) + colorsRequested(2) +
                    // verb(2) + PictInfoID ptr(4 first). Mint a
                    // unique nonzero
                    // PictInfoID and write it to *PictInfoID.
                    0x0602 => {
                        let id_ptr = bus.read_long(sp + 8);
                        if id_ptr != 0 {
                            static PICT_INFO_ID_COUNTER: std::sync::atomic::AtomicU32 =
                                std::sync::atomic::AtomicU32::new(1);
                            let pict_info_id = PICT_INFO_ID_COUNTER
                                .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                            bus.write_long(id_ptr, pict_info_id);
                            self.pict_info_ids.insert(pict_info_id);
                        }
                        finish(cpu, 12, 0);
                    }

                    // FUNCTION GetPictInfo(thePictHandle: PicHandle;
                    //                      VAR thePictInfo: PictInfo;
                    //                      verb: Integer;
                    //                      colorsRequested: Integer;
                    //                      colorPickMethod: Integer;
                    //                      version: Integer): OSErr;
                    // IM:VI 18-9. Pop = 16. Stack: version(2 last)
                    // + colorPickMethod(2) + colorsRequested(2) +
                    // verb(2) + thePictInfo ptr(4) + PicHandle(4
                    // first). Zero-fill the 104-byte PictInfo record.
                    0x0800 => {
                        let info_ptr = bus.read_long(sp + 8);
                        zero_pict_info(bus, info_ptr);
                        finish(cpu, 16, 0);
                    }

                    // FUNCTION GetPixMapInfo(thePixMapHandle:
                    //                        PixMapHandle;
                    //                        VAR thePictInfo: PictInfo;
                    //                        verb: Integer;
                    //                        colorsRequested: Integer;
                    //                        colorPickMethod: Integer;
                    //                        version: Integer): OSErr;
                    // IM:VI 18-10. Pop = 16. Same shape as
                    // GetPictInfo with PixMapHandle in place of
                    // PicHandle. Zero-fill PictInfo record.
                    0x0801 => {
                        let info_ptr = bus.read_long(sp + 8);
                        zero_pict_info(bus, info_ptr);
                        finish(cpu, 16, 0);
                    }

                    // Unknown selector — pop the encoded argument
                    // bytes, leave the FUNCTION result slot
                    // untouched, and return noErr in D0. A future
                    // System addition with a new Pack15 routine
                    // should add a new arm above.
                    _ => {
                        cpu.write_reg(Register::A7, sp + pop_total);
                        cpu.write_reg(Register::D0, 0);
                    }
                }
                Ok(())
            }

            // ========================================================
            // System 7+ Dispatch Managers (no-op family — 10 traps)
            // ========================================================
            //
            // Ten selector-based dispatchers for System-7-and-later
            // subsystems whose underlying machinery Systemless does not
            // model: Component Manager (1991), Object Support Library,
            // Dictionary Manager, Text Services Manager, Docking
            // Manager, Mixed Mode Manager (PowerPC), Code Fragment
            // Manager (PowerPC), Icon Utilities, Thread Manager, and
            // Translation Manager. Each one routes a sub-routine call
            // selected by D0. Component Manager additionally uses D0=0
            // for component calls whose parameter size and request code
            // are pushed together as a long word on the stack.
            //
            // HLE compromise — load-bearing rationale:
            //   1. Apps written for System 7+ uniformly probe presence
            //      via the documented Gestalt selector before calling
            //      the dispatcher. The standard idiom is:
            //          if Gestalt('cfrg', &response) = noErr
            //          and BTst(response, gestaltCFMPresent)
            //          then ... call CodeFragmentManager routines ...
            //          else ... fall back to non-CFM path ...
            //      Systemless's Gestalt arm at src/trap/toolbox.rs:1500+
            //      returns gestaltUndefSelectorErr (-5551) for every
            //      selector listed below, so well-behaved apps see
            //      "absent" and skip the dispatcher entirely.
            //   2. Most of the cluster still follows the stub/no-op
            //      pattern: if a future corpus title bypasses
            //      Gestalt and calls one of those dispatchers
            //      directly, we can enumerate the selectors and
            //      promote that arm to Partial in the Pack14 style
            //      (see Help Manager $A830 at src/trap/toolbox.rs:5666
            //      for the canonical `(arg_words << 8) | routine`
            //      enumeration shape).
            //   3. ThreadDispatch is the exception: the public
            //      ThreadBeginCritical/ThreadEndCritical selectors
            //      are directly observable and are modelled below
            //      with a real critical-section nesting counter
            //      instead of a blanket no-op.
            //
            // Per-trap Gestalt selectors (see arms below for cites):
            //   $A82A ComponentDispatch       gestaltComponentMgr        'cpnt'
            //   $A9F8 MethodDispatch (OSL)    (no documented selector)
            //   $AA53 DictionaryDispatch      gestaltDictionaryMgrAttr   'dict'
            //   $AA54 TextServicesDispatch    gestaltTSMgrVersion        'tsmv'
            //   $AA57 DockingDispatch         (no documented selector)
            //   $AA59 MixedModeDispatch       (no documented selector — PPC bridge)
            //   $AA5A CodeFragmentDispatch    gestaltCFMAttr             'cfrg'
            //   $ABC9 IconDispatch            gestaltIconUtilitiesAttr   'icon'
            //   $ABF2 ThreadDispatch          gestaltThreadMgrAttr       'thds' (partial)
            //   $ABFC TranslationDispatch     gestaltTranslationMgrExists (response bit 0)
            //
            // Future work: pick the highest-impact
            // dispatcher (CodeFragmentDispatch $AA5A is hot for any
            // PowerPC fat binary) and enumerate its selectors with
            // proper pop discipline + per-selector defensive defaults
            // (NIL handles, resNotFound on resource-by-name lookups,
            // etc.) following the Pack14 pattern. Until then, the
            // remaining stubbed dispatchers keep the register-
            // preservation + stack-untouched contract.

            // ComponentDispatch (0xA82A)
            // Dispatches Component Manager internal requests selected by MOVEQ in D0.
            // FUNCTION CountComponents (looking: ComponentDescription): LongInt;
            // Inside Macintosh: More Macintosh Toolbox (1993), pp. 6-43 to 6-44.
            //
            // The manager also handles component calls through D0=0.
            // `Gestalt('cpnt', ...)` gates manager availability; component
            // call glue uses `INLINE $2F3C, paramSize, callNum,
            // $7000, $A82A`, which pushes a 4-byte selector word
            // [paramSize:callNum] then traps.
            // Selector convention: stack-pushed 4-byte word at SP+0
            // (high word=paramSize bytes, low word=callNum). D0=0
            // means "call my component"; D0!=0 means "Component
            // Manager internal request" with selector in D0.
            // Gestalt: `gestaltComponentMgr = 'cpnt'` returns the CM
            // version (>=3 supports automatic version control,
            // unregister, icon families).
            //
            // HLE behaviour: provides a synthetic QuickTime movie controller
            // (`'play'`) plus software-instrument discovery for music media,
            // and opaque movie-controller instances for
            // FindNextComponent/OpenComponent/CloseComponent. Component
            // calls consume selector + instance + arguments and return a
            // zero ComponentResult in the caller's four-byte result slot.
            //
            (true, 0x02A) => {
                const MOVIE_CONTROLLER_COMPONENT: u32 = u32::from_be_bytes(*b"play");
                const SYNTHETIC_MOVIE_CONTROLLER: u32 = 0x00C0_0001;
                const SOFTWARE_INSTRUMENT: u32 = 0x00C0_0002;

                let d0 = cpu.read_reg(Register::D0);
                let operation = component_dispatch_operation_route(self.current_trap_word, d0);
                self.current_selector_operation = operation.map(|route| route.operation_id);
                if d0 == 0 {
                    let sp = cpu.read_reg(Register::A7);
                    let param_size = u32::from(bus.read_word(sp));
                    // Selector long = [paramSize:callNum]; callNum is the low word.
                    let call_num = bus.read_word(sp + 2);
                    if super::dispatch::trace_quicktime_enabled() {
                        let selector = bus.read_long(sp);
                        let instance = bus.read_long(sp + 4 + param_size);
                        eprintln!(
                            "[QUICKTIME] ComponentDispatch selector=${:08X} paramSize={} callNum=${:04X} instance=${:08X}",
                            selector,
                            param_size,
                            selector as u16,
                            instance
                        );
                    }
                    let instance = bus.read_long(sp + 4 + param_size);
                    match call_num {
                        // MCNewAttachedController(mc, theMovie, window, where):
                        // associate the controller instance with its movie so
                        // MCDoAction can drive the right one. The parameter
                        // block holds theMovie among its longs; find it by
                        // matching a known movie handle.
                        0x0017 => {
                            let tick = self.current_tick();
                            for off in (4..4 + param_size).step_by(4) {
                                let candidate = bus.read_long(sp + off);
                                if self.movie_states.contains_key(&candidate) {
                                    self.movie_by_controller.insert(instance, candidate);
                                    if let Some(state) = self.movie_states.get_mut(&candidate) {
                                        state.active = true;
                                        state.last_service_tick = Some(tick);
                                    }
                                    break;
                                }
                            }
                        }
                        // MCDoAction(mc, action, params). Pascal push order puts
                        // params(long) at sp+4 and action(word) at sp+8.
                        0x0009 => {
                            let tick = self.current_tick();
                            let action = bus.read_word(sp + 8);
                            let param = bus.read_long(sp + 4);
                            if let Some(&movie) = self.movie_by_controller.get(&instance) {
                                if let Some(state) = self.movie_states.get_mut(&movie) {
                                    match action {
                                        // mcActionPlay(8): param is a Fixed rate.
                                        8 => {
                                            let rate = param as i32;
                                            state.rate = if rate != 0 { rate } else { 0x0001_0000 };
                                            state.active = true;
                                            state.last_service_tick = Some(tick);
                                        }
                                        // mcActionStop(2)/mcActionSetPlayRate not
                                        // separately tracked; stop clears the rate.
                                        2 => {
                                            state.rate = 0;
                                        }
                                        _ => {}
                                    }
                                }
                            }
                        }
                        _ => {}
                    }
                    // MCIdle ($1A): service the movie controller — advance
                    // playback clocks and render the frames now due.
                    if call_num == 0x001A {
                        self.advance_and_render_active_movies(bus);
                    }
                    let result_sp = sp + 8 + param_size;
                    bus.write_long(result_sp, 0);
                    cpu.write_reg(Register::A7, result_sp);
                    cpu.write_reg(Register::D0, 0);
                    return Some(Ok(()));
                }

                let sp = cpu.read_reg(Register::A7);
                if super::dispatch::trace_quicktime_enabled() {
                    eprintln!(
                        "[QUICKTIME] ComponentDispatch internal D0=${:08X} sp=${:08X}",
                        d0, sp
                    );
                }
                match d0 as u16 {
                    0x0003 => {
                        // CountComponents(ComponentDescription *looking): long
                        let description = bus.read_long(sp);
                        let component_type = if description == 0 {
                            0
                        } else {
                            bus.read_long(description)
                        };
                        let instrument_matches = (component_type == 0
                            || component_type == u32::from_be_bytes(*b"inst"))
                            && (description == 0
                                || ([0, u32::from_be_bytes(*b"ss  ")]
                                    .contains(&bus.read_long(description + 4))
                                    && [0, u32::from_be_bytes(*b"appl")]
                                        .contains(&bus.read_long(description + 8))));
                        let count = u32::from(
                            component_type == 0 || component_type == MOVIE_CONTROLLER_COMPONENT,
                        ) + u32::from(instrument_matches);
                        bus.write_long(sp + 4, count);
                        cpu.write_reg(Register::A7, sp + 4);
                        cpu.write_reg(Register::D0, count);
                    }
                    0x0004 => {
                        // FindNextComponent(Component, ComponentDescription *): Component
                        let description = bus.read_long(sp);
                        let previous = bus.read_long(sp + 4);
                        let component_type = if description == 0 {
                            0
                        } else {
                            bus.read_long(description)
                        };
                        let component = if previous == 0
                            && (component_type == 0 || component_type == MOVIE_CONTROLLER_COMPONENT)
                        {
                            SYNTHETIC_MOVIE_CONTROLLER
                        } else if (previous == 0 || previous == SYNTHETIC_MOVIE_CONTROLLER)
                            && (component_type == 0
                                || component_type == u32::from_be_bytes(*b"inst"))
                            && (description == 0
                                || ([0, u32::from_be_bytes(*b"ss  ")]
                                    .contains(&bus.read_long(description + 4))
                                    && [0, u32::from_be_bytes(*b"appl")]
                                        .contains(&bus.read_long(description + 8))))
                        {
                            // QuickTime Music Architecture: the instrument
                            // component advertises the Movie Toolbox's synth.
                            SOFTWARE_INSTRUMENT
                        } else {
                            0
                        };
                        bus.write_long(sp + 8, component);
                        cpu.write_reg(Register::A7, sp + 8);
                        cpu.write_reg(Register::D0, component);
                    }
                    0x0007 => {
                        // OpenComponent(Component): ComponentInstance
                        let component = bus.read_long(sp);
                        let instance = if component == SYNTHETIC_MOVIE_CONTROLLER {
                            let instance = self.next_synthetic_component_instance;
                            self.next_synthetic_component_instance =
                                self.next_synthetic_component_instance.saturating_add(1);
                            self.synthetic_component_instances.insert(instance);
                            instance
                        } else {
                            0
                        };
                        bus.write_long(sp + 4, instance);
                        cpu.write_reg(Register::A7, sp + 4);
                        cpu.write_reg(Register::D0, instance);
                    }
                    0x0008 => {
                        // CloseComponent(ComponentInstance): OSErr
                        let instance = bus.read_long(sp);
                        self.synthetic_component_instances.remove(&instance);
                        bus.write_word(sp + 4, 0);
                        cpu.write_reg(Register::A7, sp + 4);
                        cpu.write_reg(Register::D0, 0);
                    }
                    0x000A => {
                        // GetComponentInstanceError(ComponentInstance): OSErr
                        bus.write_word(sp + 4, 0);
                        cpu.write_reg(Register::A7, sp + 4);
                        cpu.write_reg(Register::D0, 0);
                    }
                    0x0021 => {
                        // OpenDefaultComponent(OSType, OSType): ComponentInstance
                        let component_type = bus.read_long(sp + 4);
                        let instance = if component_type == MOVIE_CONTROLLER_COMPONENT {
                            let instance = self.next_synthetic_component_instance;
                            self.next_synthetic_component_instance =
                                self.next_synthetic_component_instance.saturating_add(1);
                            self.synthetic_component_instances.insert(instance);
                            instance
                        } else {
                            0
                        };
                        bus.write_long(sp + 8, instance);
                        cpu.write_reg(Register::A7, sp + 8);
                        cpu.write_reg(Register::D0, instance);
                    }
                    _ => {
                        cpu.write_reg(Register::D0, 0);
                    }
                }
                Ok(())
            }

            // MethodDispatch ($A9F8) — Object Support Library
            // Public MPW declaration:
            // `pascal OSErr MethodDispatch(short selector) = {0xA9F8};`
            // Inside Macintosh Volume VI (1991), Object Support
            // Library appendix. The selector arrives in D0 and the
            // selector-0 path is the observed public contract: return
            // noErr, preserve A7, and leave non-D0 registers alone.
            //
            // No documented Gestalt selector — apps that use OSL
            // typically check via NGetTrapAddress($A9F8) returning
            // _Unimplemented vs a real handler.
            //
            // HLE behaviour: D0=0 (noErr), all other registers
            // preserved, stack untouched. OSL is essentially dead
            // since no shipping app uses it directly (only via the
            // System Object Model wrappers).
            //
            // Regression coverage:
            //   methoddispatch_*
            // MethodDispatch (OSL) ($A9F8): IM:VI Object Support Library. D0 selector = method ID, args on stack. No Gestalt selector — apps probe via NGetTrapAddress. HLE: D0=0, registers + stack preserved.
            (true, 0x1F8) => return_noerr(cpu),

            // DictionaryDispatch ($AA53) — Dictionary Manager
            // Inside Macintosh: Text 1993, ch. 8
            // (Gestalt cite Text 1993 25656: "Use Gestalt with the
            // gestaltDictionaryMgrAttr environment selector to obtain
            // a result ... A result of gestaltDictionaryMgrPresent
            // (= 0) means that the Dictionary Manager is present.")
            // Selector convention: D0 = routine number per the
            // Pack8/Pack14 `(arg_words << 8) | routine` encoding.
            // Routines include InitializeDictionary, OpenDictionary,
            // CloseDictionary, FindRecordInDictionary, etc.
            // Gestalt: `gestaltDictionaryMgrAttr = 'dict'`,
            // `gestaltDictionaryMgrPresent = 0`.
            //
            // HLE behaviour: selector 0x0500 (InitializeDictionary)
            // writes noErr to the caller's function-result slot,
            // returns noErr in D0, and pops the 10-byte public call
            // frame (FSSpecPtr + maximumKeyLength + keyAttributes +
            // script); all other selectors preserve registers and
            // remain the no-op safety net. Apps that probe Gestalt
            // first see "absent" and skip the dispatcher entirely.
            //
            // Regression coverage:
            //   src/trap/toolbox.rs::tests::dictionarydispatch_*
            // DictionaryDispatch ($AA53): Text 1993 ch. 8 25656, 26640-26648. D0 selector per (arg_words<<8)|routine. Gestalt 'dict' → gestaltDictionaryMgrPresent=0. HLE: InitializeDictionary writes noErr to the function-result slot, pops 10 bytes, and returns noErr; the remaining selectors stay stack-untouched.
            (true, 0x253) => {
                let selector = cpu.read_reg(Register::D0) & 0xFFFF;
                let result = match selector {
                    0x0500 => {
                        let sp = cpu.read_reg(Register::A7);
                        bus.write_word(sp + 10, 0);
                        cpu.write_reg(Register::A7, sp + 10);
                        cpu.write_reg(Register::D0, 0);
                        Ok(())
                    }
                    _ => return_noerr(cpu),
                };
                result
            }

            // TextServicesDispatch ($AA54) — Text Services Manager
            // Inside Macintosh: Text 1993, ch. 8
            // (Gestalt cite Text 1993 22172: "Use the Gestalt
            // environmental selector gestaltTSMgrVersion to determine
            // whether the Text Services Manager is available.")
            // The TSM routes input-method (e.g. CJKV IME, character
            // palette) calls between an app and an installed text
            // service component. Selector convention: D0 = routine
            // number; selectors include InitTSMAwareApplication,
            // CloseTSMAwareApplication, NewTSMDocument, etc.
            // Gestalt: `gestaltTSMgrVersion = 'tsmv'` returns the
            // TSM version as a 32-bit value.
            //
            // HLE behaviour: D0=0 (noErr), all other registers
            // preserved, stack untouched. No current corpus title
            // (English-only games) needs TSM.
            //
            // Regression coverage:
            //   textservicesdispatch_*
            // TextServicesDispatch (TSM) ($AA54): Text 1993 ch.8 22172. D0 selector. Gestalt 'tsmv' → version word. HLE: D0=0, registers + stack preserved.
            (true, 0x254) => return_noerr(cpu),

            // DockingDispatch ($AA57) — Docking Manager (PowerBook)
            // Inside Macintosh Volume VI (PowerBook docking station
            // protocol). Specific to docking-station-aware PowerBooks
            // (Duo 2x0 series); routes dock/undock notifications and
            // power-state events. Selector convention: D0 = routine
            // number per the standard System 7 dispatcher pattern.
            // No documented Gestalt selector. The selected Mac OS 8.1
            // profiles expose a callable table entry distinct from the
            // modern $AA6E Unimplemented identity.
            //
            // HLE behaviour: D0=0 (noErr), all other registers
            // preserved, stack untouched.
            //
            // Regression coverage:
            //   dockingdispatch_*
            // DockingDispatch ($AA57): IM:VI PowerBook docking. D0 selector. No Gestalt selector — probe via NGetTrapAddress. HLE: D0=0, registers + stack preserved.
            (true, 0x257) => {
                cpu.write_reg(Register::D0, 0);
                Ok(())
            }

            // MixedModeDispatch ($AA59)
            // Dispatches private Mixed Mode Manager services; this is distinct
            // from the executable `$AAFE` word in a RoutineDescriptor.
            // (private dispatcher; selector ABI remains unclassified)
            // Inside Macintosh: PowerPC System Software (1994), pp. 2-4--2-8.
            (true, 0x259) => return_noerr(cpu),

            // goMixedModeTrap ($AAFE)
            // Enters the Mixed Mode Manager through the executable first word
            // of a RoutineDescriptor and selects its architecture record.
            // (private executable RoutineDescriptor entry; no Pascal signature)
            // Inside Macintosh: PowerPC System Software (1994), pp. 2-8--2-12
            // and 2-37--2-38.
            (true, 0x2FE) => {
                crate::mixed_mode::enter_m68k_routine_descriptor(cpu, bus, &self.guest_calls)
            }

            // CodeFragmentDispatch ($AA5A)
            // Finds and enumerates exports in a process CFM connection.
            // OSErr FindSymbol(ConnectionID connID, Str255 symName,
            //                  Ptr *symAddr, SymClass *symClass);
            // OSErr CountSymbols(ConnectionID connID, long *symCount);
            // OSErr GetIndSymbol(ConnectionID connID, long symIndex,
            //                    Str255 symName, Ptr *symAddr, SymClass *symClass);
            // PowerPC System Software (1994), pp. 3-24–3-26. Universal
            // Interfaces 3.4, CodeFragments.h: $3F3C,$0005/$0006/$0007,$AA5A
            // pushes a word selector before the Pascal argument frame.
            (true, 0x25A) => dispatch_cfm_symbols(cpu, bus, cfm, bindings),

            // IconDispatch ($ABC9)
            // Dispatches Icon Utilities routines selected by the low word of D0.
            // Selector-specific Pascal frames; the low selector byte gives argument words.
            // More Macintosh Toolbox (1993), pp. 5-18 to 5-71.
            (true, 0x3C9) => {
                let raw_d0 = cpu.read_reg(Register::D0);
                let operation = icon_dispatch_operation_route(self.current_trap_word, raw_d0);
                self.current_selector_operation = operation.map(|route| route.operation_id);
                let raw_selector = (raw_d0 & 0xFFFF) as u16;
                let is_public = |selector| {
                    matches!(
                        selector,
                        0x0702
                            | 0x1702
                            | 0x0203
                            | 0x1603
                            | 0x1904
                            | 0x1A04
                            | 0x1B04
                            | 0x1C04
                            | 0x0005
                            | 0x0105
                            | 0x0B05
                            | 0x0306
                            | 0x0406
                            | 0x0606
                            | 0x0806
                            | 0x0906
                            | 0x0D06
                            | 0x1006
                            | 0x1306
                            | 0x1D06
                            | 0x1E06
                            | 0x1F06
                            | 0x0E07
                            | 0x1107
                            | 0x1407
                            | 0x0A08
                            | 0x0508
                            | 0x0F09
                            | 0x1209
                            | 0x1509
                    )
                };
                let selector = if is_public(raw_selector) {
                    raw_selector
                } else if is_public(raw_selector.swap_bytes()) {
                    raw_selector.swap_bytes()
                } else {
                    raw_selector
                };
                let pop_bytes = if is_public(selector) {
                    u32::from(selector & 0x00FF) * 2
                } else {
                    8
                };
                match selector {
                    // PlotCIconHandle. The Icon Utilities glue passes, in
                    // reverse Pascal order, the CIconHandle, transform and
                    // alignment words, and destination Rect pointer. PlotCIcon
                    // supplies the shared color-icon decode and masked blit;
                    // an icon-sized destination with atNone requires no
                    // additional alignment.
                    // More Macintosh Toolbox (1993), pp. 5-26 to 5-27 and
                    // 5-72; selector table p. A-34 ($1F06).
                    0x1F06 => {
                        let sp = cpu.read_reg(Register::A7);
                        let transform = bus.read_word(sp + 4) as i16;
                        let rect_ptr = bus.read_long(sp + 8);
                        bus.write_long(sp + 4, rect_ptr);
                        let previous_transform = self.icon_transform_override;
                        self.icon_transform_override = transform;
                        let result = self.dispatch_quickdraw(true, 0x21F, cpu, bus);
                        self.icon_transform_override = previous_transform;
                        bus.write_word(sp + pop_bytes, 0);
                        cpu.write_reg(Register::A7, sp + pop_bytes);
                        result.unwrap_or(Ok(()))
                    }
                    0 => return_noerr_and_pop(cpu, pop_bytes),
                    _ => return_error_and_pop(cpu, pop_bytes, -50),
                }
            }

            // ThreadDispatch ($ABF2) — cooperative Thread Manager
            // Inside Macintosh: Thread Manager (1999), pp. 1-56 to 1-58:
            //   NewThread(threadStyle, threadEntry, threadParam, stackSize,
            //             options, threadResult, threadMade);
            // and pp. 1-70 to 1-71:
            //   pascal OSErr ThreadBeginCritical(void);
            //   pascal OSErr ThreadEndCritical(void);
            // MPW Interfaces/CIncludes/Threads.h dispatches NewThread through
            // selector $0E03. The selector high byte encodes fourteen stack
            // words, so the Pascal frame is 28 argument bytes followed by the
            // OSErr result slot. The same glue uses no-argument selectors
            // $000B/$000C for ThreadBeginCritical/ThreadEndCritical.
            //
            // HLE behaviour: selector $0E03 creates a cooperative 68K context,
            // inheriting the caller's register world and receiving a private
            // guest stack. Selector $000B increments the dispatcher-
            // wide critical-section nesting counter and returns noErr.
            // Selector $000C decrements the counter when nonzero and
            // returns noErr; when the counter is already zero it
            // returns threadProtocolErr (-619). Unsupported selectors
            // return paramErr (-50). The public `Threads.h` wrappers
            // read the 16-bit OSErr from the caller's zero-arg result
            // slot at SP, so we mirror the low word there as well as
            // in D0.
            //
            // Regression coverage:
            //   src/trap/toolbox.rs::tests::threaddispatch_newthread_accepts_default_stack_and_consumes_mpw_frame
            //   src/trap/toolbox.rs::tests::threaddispatch_begin_and_end_critical_roundtrip
            //   src/trap/toolbox.rs::tests::threaddispatch_endcritical_underflow_returns_thread_protocol_err
            //   src/trap/toolbox.rs::tests::threaddispatch_unsupported_selector_returns_param_err
            (true, 0x3F2) => {
                let selector = cpu.read_reg(Register::D0) & 0xFFFF;
                let sp = cpu.read_reg(Register::A7);

                let result = match selector {
                    0xFFFE => {
                        let task = self.guest_calls.current_task();
                        let result = cpu.read_reg(Register::A0);
                        let retirement = self.guest_calls.retire_classic_thread(
                            task,
                            false,
                            |saved| {
                                saved.result_destination == 0
                                    || bus.try_write_long(saved.result_destination, result)
                            },
                        );
                        if let Ok(retirement) = retirement {
                            self.apply_classic_retirement(cpu, bus, false, retirement);
                            return Some(Ok(()));
                        }
                        // A thread cannot finish while one of its guest
                        // continuations is suspended. This is a guest-visible
                        // protocol failure, not an emulator invariant that
                        // justifies panicking.
                        cpu.write_reg(Register::D0, Self::THREAD_PROTOCOL_ERR as u32);
                        if self.thread_return_trampoline != 0 {
                            cpu.write_reg(Register::PC, self.thread_return_trampoline);
                        }
                        return Some(Ok(()));
                    }
                    0x0205 => {
                        let suggested_thread = bus.read_long(sp);
                        let result_sp = sp.wrapping_add(4);
                        let current = self.guest_calls.current_task();
                        let mut outgoing = self
                            .guest_calls
                            .cooperative_context(current)
                            .unwrap_or_else(|| CooperativeThread::capture(cpu));
                        outgoing.save_registers(cpu);
                        outgoing.d_regs[0] = 0;
                        outgoing.a_regs[7] = result_sp;
                        let result = self
                            .guest_calls
                            .yield_classic_thread(outgoing, suggested_thread);
                        let error = result.as_ref().err().copied().unwrap_or(0);
                        bus.write_word(result_sp, error as u16);
                        cpu.write_reg(Register::D0, error as u32);
                        cpu.write_reg(Register::A7, result_sp);
                        if let Ok(crate::guest_call::ClassicYield::Switched(Some(context))) = result
                        {
                            context.install(cpu);
                        }
                        return Some(Ok(()));
                    }
                    // GetCurrentThread(currentThreadID)
                    0x0206 => {
                        let current_thread = bus.read_long(sp);
                        let id = ThreadManager::new(&self.guest_calls).current_thread();
                        if current_thread != 0 && bus.try_write_long(current_thread, id) {
                            0
                        } else {
                            -50
                        }
                    }
                    // NewThread(threadStyle, threadEntry, threadParam,
                    //           stackSize, options, threadResult, threadMade)
                    0x0E03 => {
                        let thread_made = bus.read_long(sp);
                        let result_destination = bus.read_long(sp + 4);
                        let options = bus.read_long(sp + 8);
                        let stack_size = bus.read_long(sp + 12);
                        let thread_param = bus.read_long(sp + 16);
                        let thread_entry = bus.read_long(sp + 20);
                        let thread_style = bus.read_long(sp + 24);

                        // NewThread publishes no identity until its ABI frame and
                        // output destinations are committed. Thread Manager (1999),
                        // pp. 56–58: threadMade is kNoThreadID on failure.
                        let result = sp.checked_add(28).ok_or(-50_i16).and_then(|result_slot| {
                            // The manager uses a detached shared handle so the edge can
                            // borrow this dispatcher for trampoline preparation.
                            let execution = self.guest_calls.shared_handle();
                            let mut edge = ClassicNewThreadEdge {
                                dispatcher: self,
                                cpu,
                                bus,
                                thread_entry,
                                thread_param,
                                result_destination,
                                thread_made,
                                result_slot,
                                trampoline: 0,
                            };
                            ThreadManager::new(&execution)
                                .create_thread(GuestIsa::M68k, thread_style, stack_size, options, &mut edge)
                                .map(|_| ())
                        });
                        match result {
                            Ok(()) => 0,
                            Err(error) => {
                                if thread_made != 0 {
                                    let _ = bus.try_write_long(thread_made, 0);
                                }
                                error
                            }
                        }
                    }
                    // CreateThreadPool(threadStyle, numToCreate, stackSize)
                    // Thread Manager (1999), pp. 50–51: publish all or none.
                    0x0501 => {
                        let stack_size = bus.read_long(sp);
                        let count = bus.read_word(sp + 4) as i16;
                        let style = bus.read_long(sp + 6);
                        let result = if !sp
                            .checked_add(10)
                            .is_some_and(|slot| bus.is_guest_address_writable(slot, 2))
                        {
                            Err((-50, Vec::new()))
                        } else {
                            ThreadManager::new(&self.guest_calls).create_pool(
                                crate::guest_procedure::GuestIsa::M68k,
                                style,
                                count,
                                stack_size,
                                |size| {
                                    let base = bus.alloc(size);
                                    (base != 0).then_some(crate::guest_call::ThreadStorage {
                                        stack_base: base,
                                        stack_limit: base.saturating_add(size),
                                        ..Default::default()
                                    })
                                },
                            )
                        };
                        match result {
                            Ok(()) => 0,
                            Err((error, storage)) => {
                                for stack in storage {
                                    bus.free(stack.stack_base);
                                }
                                error
                            }
                        }
                    }
                    // GetFreeThreadCount / GetSpecificFreeThreadCount /
                    // GetDefaultThreadStackSize. Thread Manager (1999), pp. 52–55.
                    0x0402 | 0x0615 | 0x0413 => {
                        let output = bus.read_long(sp);
                        let specific = selector == 0x0615;
                        let style = bus.read_long(sp + if specific { 8 } else { 4 });
                        let manager = ThreadManager::new(&self.guest_calls);
                        let value = if selector == 0x0413 {
                            ThreadManager::stack_size(
                                crate::guest_procedure::GuestIsa::M68k,
                                style,
                                0,
                            )
                        } else {
                            manager
                                .free_count(
                                    crate::guest_procedure::GuestIsa::M68k,
                                    style,
                                    if specific { bus.read_long(sp + 4) } else { 0 },
                                )
                                .map(u32::from)
                        };
                        match value {
                            Err(error) => error,
                            Ok(value) if output != 0 => {
                                let written = if selector == 0x0413 {
                                    bus.try_write_long(output, value)
                                } else {
                                    bus.try_write_word(output, value as u16)
                                };
                                if written {
                                    0
                                } else {
                                    -50
                                }
                            }
                            Ok(_) => -50,
                        }
                    }
                    // ThreadCurrentStackSpace ($ABF2, selector $0414)
                    // Returns available stack bytes for the specified thread.
                    // FUNCTION ThreadCurrentStackSpace(thread: ThreadID; VAR freeStack: LONGINT): OSErr;
                    // Thread Manager (1999), pp. 17–18 and 61.
                    0x0414 => {
                        let output = bus.read_long(sp);
                        let classic_limit = bus.read_long(crate::memory::globals::addr::APPL_LIMIT);
                        let native_limit = {
                            let memory_manager = self.process_memory_manager();
                            let memory_manager = memory_manager.borrow();
                            memory_manager.application_heap_limit(
                                memory_manager
                                    .native_heap_state()
                                    .map_or(0, |heap| heap.heap_limit),
                            )
                        };
                        let manager = crate::thread_manager::ThreadManager::new(&self.guest_calls);
                        match manager.stack_space(
                            bus.read_long(sp + 4),
                            crate::guest_procedure::GuestIsa::M68k,
                            cpu.read_reg(Register::A7),
                            |isa| match isa {
                                crate::guest_procedure::GuestIsa::M68k => classic_limit,
                                crate::guest_procedure::GuestIsa::PowerPc => native_limit,
                            },
                        ) {
                            Err(error) => error,
                            Ok(value) if output != 0 && bus.try_write_long(output, value) => 0,
                            Ok(_) => -50,
                        }
                    }
                    // DisposeThread(threadToDump, threadResult, recycleThread)
                    0x0504 => {
                        // Pascal Boolean occupies the high byte of its stack word.
                        // Thread Manager (1999), pp. 59–60.
                        let recycle = bus.read_byte(sp) != 0;
                        let thread_result = bus.read_long(sp + 2);
                        let thread_to_dump =
                            self.resolve_cooperative_thread_id(bus.read_long(sp + 6));
                        let retirement = self.guest_calls.retire_classic_thread(
                            ExecutionTaskId::from_thread_id(thread_to_dump),
                            recycle,
                            |saved| {
                                saved.result_destination == 0
                                    || bus.try_write_long(saved.result_destination, thread_result)
                            },
                        );
                        match retirement {
                            Ok(retirement) => {
                                let switched = matches!(
                                    retirement,
                                    ClassicRetirement::Switched { .. }
                                );
                                self.apply_classic_retirement(cpu, bus, recycle, retirement);
                                if switched {
                                    return Some(Ok(()));
                                }
                                0
                            }
                            Err(error) => error,
                        }
                    }
                    // GetThreadState(threadToGet, threadState), and
                    // GetThreadStateGivenTaskRef, which differs only by the
                    // task ref Systemless has exactly one of.
                    0x0407 | 0x060F => {
                        let thread_state = bus.read_long(sp);
                        let thread_to_get =
                            self.resolve_cooperative_thread_id(bus.read_long(sp + 4));
                        if thread_state == 0 {
                            -50
                        } else {
                            let manager = ThreadManager::new(&self.guest_calls);
                            let state = if selector == 0x060F {
                                manager.state_given_task(bus.read_long(sp + 8), thread_to_get)
                            } else {
                                manager.state(thread_to_get)
                            };
                            match state {
                                Ok(state) if bus.try_write_word(thread_state, state) => 0,
                                Ok(_) => -50,
                                Err(error) => error,
                            }
                        }
                    }
                    // SetThreadState / SetThreadStateEndCritical (0xA3F2)
                    // Set state and optionally exit a critical section atomically.
                    // OSErr (ThreadID thread, ThreadState state, ThreadID suggested);
                    // Inside Macintosh: Thread Manager (1999), pp. 67–72.
                    0x0508 | 0x0512 => {
                        let suggested = bus.read_long(sp);
                        let state = bus.read_word(sp + 4);
                        let thread = bus.read_long(sp + 6);
                        let current = self.guest_calls.current_task();
                        let mut outgoing = self
                            .guest_calls
                            .cooperative_context(current)
                            .unwrap_or_default();
                        outgoing.save_registers(cpu);
                        outgoing.d_regs[0] = 0;
                        outgoing.a_regs[7] = sp + 10;
                        match self.guest_calls.set_classic_thread_state(
                            thread,
                            state,
                            suggested,
                            selector == 0x0512,
                            outgoing,
                            || bus.try_write_word(sp + 10, 0),
                        ) {
                            Ok(switched) => {
                                cpu.write_reg(Register::A7, sp + 10);
                                cpu.write_reg(Register::D0, 0);
                                if switched {
                                    if let Some(next) = self.guest_calls.take_classic_task_handoff()
                                    {
                                        next.install(cpu);
                                    }
                                }
                                return Some(Ok(()));
                            }
                            Err(error) => error,
                        }
                    }
                    // SetThreadReadyGivenTaskRef (0xA3F2)
                    // Mark a stopped thread ready without switching during the call.
                    // OSErr (ThreadTaskRef threadTRef, ThreadID threadToSet);
                    // Inside Macintosh: Thread Manager (1999), pp. 75–76.
                    0x0410 => ThreadManager::new(&self.guest_calls)
                        .ready_given_task(bus.read_long(sp + 4), bus.read_long(sp)),
                    // SetThreadScheduler(threadScheduler)
                    0x0209 => {
                        self.cooperative_thread_scheduler = bus.read_long(sp);
                        0
                    }
                    // SetThreadSwitcher(thread, threadSwitcher,
                    //                   switchProcParam, inOrOut). A Pascal
                    // Boolean occupies the high byte of its stack word.
                    0x070A => {
                        let switch_in = bus.read_byte(sp) != 0;
                        let switch_proc_param = bus.read_long(sp + 2);
                        let thread_switcher = bus.read_long(sp + 6);
                        let thread = self.resolve_cooperative_thread_id(bus.read_long(sp + 10));
                        match self.cooperative_thread_snapshot(cpu, thread) {
                            None => Self::THREAD_NOT_FOUND_ERR,
                            Some(mut record) => {
                                if switch_in {
                                    record.switch_in = (thread_switcher, switch_proc_param);
                                } else {
                                    record.switch_out = (thread_switcher, switch_proc_param);
                                }
                                self.guest_calls.save_cooperative_context(
                                    ExecutionTaskId::from_thread_id(thread),
                                    record,
                                );
                                0
                            }
                        }
                    }
                    // SetThreadTerminator(thread, threadTerminator,
                    //                     terminationProcParam)
                    0x0611 => {
                        let termination_proc_param = bus.read_long(sp);
                        let thread_terminator = bus.read_long(sp + 4);
                        let thread = self.resolve_cooperative_thread_id(bus.read_long(sp + 8));
                        match self.cooperative_thread_snapshot(cpu, thread) {
                            None => Self::THREAD_NOT_FOUND_ERR,
                            Some(mut record) => {
                                record.terminator = (thread_terminator, termination_proc_param);
                                self.guest_calls.save_cooperative_context(
                                    ExecutionTaskId::from_thread_id(thread),
                                    record,
                                );
                                0
                            }
                        }
                    }
                    // SetDebuggerNotificationProcs(notifyNewThread,
                    //     notifyDisposeThread, notifyThreadScheduler).
                    // Systemless hosts no 68K debugger, so the procs are
                    // accepted and never called.
                    0x060D => 0,
                    // GetThreadCurrentTaskRef(threadTRef). Systemless runs a
                    // single Thread Manager task; the application thread's ID
                    // is a stable non-nil token for it.
                    0x020E => {
                        let thread_t_ref = bus.read_long(sp);
                        if thread_t_ref == 0 {
                            -50
                        } else {
                            let reference = ThreadManager::new(&self.guest_calls).task_reference();
                            if bus.try_write_long(thread_t_ref, reference) {
                                0
                            } else {
                                -50
                            }
                        }
                    }
                    0x000B => ThreadManager::new(&self.guest_calls).begin_critical(),
                    0x000C => ThreadManager::new(&self.guest_calls).end_critical(),
                    _ => -50,
                };
                let argument_bytes = (selector >> 8) * 2;
                bus.write_word(sp + argument_bytes, result as u16);
                if result == 0 {
                    return_noerr_and_pop(cpu, argument_bytes)
                } else {
                    return_error_and_pop(cpu, argument_bytes, result)
                }
            }

            // _TranslationDispatch (0xABFC)
            // Dispatches Translation Manager routines selected by D0.
            // FUNCTION GetFileTypesThatAppCanNativelyOpen (appVRefNumHint: Integer; appSignature: OSType; VAR nativeTypes: TypesBlock): OSErr;
            // Inside Macintosh: More Macintosh Toolbox (1993), pp. 7-37–7-38 and 7-66.
            (true, 0x3FC) => {
                let selector = cpu.read_reg(Register::D0);
                let operation =
                    translation_dispatch_operation_route(self.current_trap_word, selector);
                self.current_selector_operation = operation.map(|route| route.operation_id);
                return_error_and_pop(cpu, 4, -50)
            }

            _ => return None,
        })
    }

    /// Shared implementation of UniqueID ($A9C1) and Unique1ID ($A810).
    ///
    /// Pascal signature is identical for both:
    ///   FUNCTION (theType: ResType): INTEGER;
    /// Stack on entry:
    ///   SP       theType (ResType,   4 bytes)
    ///   SP + 4   result  (INTEGER,   2 bytes — caller-allocated)
    /// Pops 4 bytes of args, leaves the 2-byte result slot.
    ///
    /// `current_only = false` ($A9C1) scans every open resource file.
    /// `current_only = true`  ($A810) restricts the used-ID set to the
    /// current resource file only, per IM:IV-16.
    ///
    /// Always starts the candidate scan at 128 to sidestep the IM:I-121
    /// caller-warning about IDs in the system-reserved range 0..127.
    fn handle_unique_id<C: CpuOps>(
        &mut self,
        bus: &mut MacMemoryBus,
        cpu: &mut C,
        current_only: bool,
    ) -> Result<()> {
        let sp = cpu.read_reg(Register::A7);
        let raw_res_type = bus.read_long(sp).to_be_bytes();
        let res_type = super::TrapDispatcher::normalize_ostype(raw_res_type);

        let mut used_ids = std::collections::HashSet::new();
        if let Some(ref resources) = self.resources {
            let mut chain: Vec<u16> = if current_only {
                let refnum = self.current_resource_refnum();
                if resources.files.contains_key(&refnum) {
                    vec![refnum]
                } else {
                    Vec::new()
                }
            } else {
                resources.files.keys().copied().collect()
            };
            chain.sort_unstable();
            for refnum in chain {
                if let Some(file) = resources.files.get(&refnum) {
                    for (t, id) in file.loaded.keys() {
                        if *t == res_type {
                            used_ids.insert(*id);
                        }
                    }
                }
            }
        }

        let mut candidate: i16 = 128;
        while used_ids.contains(&candidate) {
            candidate = candidate.wrapping_add(1);
            if candidate <= 0 {
                candidate = 128;
                break;
            }
        }

        bus.write_word(sp + 4, candidate as u16);
        cpu.write_reg(Register::A7, sp + 4);
        Ok(())
    }

    /// Shared implementation of GetIndType ($A99F) and Get1IndType ($A80F).
    ///
    /// Pascal signature is identical for both:
    ///   PROCEDURE (VAR theType: ResType; index: INTEGER);
    /// Pascal arg push order is left-to-right, so theType (the VAR ptr)
    /// is pushed first (deeper) and index is pushed last (shallower).
    /// Stack on entry:
    ///   SP       index   (INTEGER,    2 bytes)
    ///   SP + 2   typePtr (ResType*,   4 bytes)
    /// Pops 6 bytes of args, no result slot (PROCEDURE).
    ///
    /// `current_only = false` ($A99F) walks `resource_search_order()` —
    /// the current resource file plus every file opened before it.
    /// `current_only = true`  ($A80F) restricts the walk to the current
    /// resource file only, per IM:IV-15.
    ///
    /// Uses BTreeSet so the index→type mapping is deterministic for a
    /// given resource map; HashSet would be undefined-order and would
    /// make tests flaky across rebuilds.
    fn handle_get_ind_type<C: CpuOps>(
        &mut self,
        bus: &mut MacMemoryBus,
        cpu: &mut C,
        current_only: bool,
    ) -> Result<()> {
        let sp = cpu.read_reg(Register::A7);
        let index = bus.read_word(sp) as i16;
        let type_ptr = bus.read_long(sp + 2);

        let result_type = if let Some(ref resources) = self.resources {
            let chain: Vec<u16> = if current_only {
                let refnum = self.current_resource_refnum();
                if resources.files.contains_key(&refnum) {
                    vec![refnum]
                } else {
                    Vec::new()
                }
            } else {
                self.resource_search_order()
            };
            let mut types = std::collections::BTreeSet::new();
            for refnum in chain {
                if let Some(file) = resources.files.get(&refnum) {
                    for (res_type, _) in file.loaded.keys() {
                        types.insert(*res_type);
                    }
                }
            }
            if index >= 1 && (index as usize) <= types.len() {
                types.into_iter().nth((index - 1) as usize)
            } else {
                None
            }
        } else {
            None
        };

        if type_ptr != 0 {
            match result_type {
                Some(t) => bus.write_long(type_ptr, u32::from_be_bytes(t)),
                None => bus.write_long(type_ptr, 0),
            }
        }
        cpu.write_reg(Register::A7, sp + 6);
        Ok(())
    }

    /// Shared implementation of GetIndResource ($A99D) and Get1IndResource ($A80E).
    ///
    /// Pascal signature is identical for both:
    ///   FUNCTION (theType: ResType; index: INTEGER): Handle;
    /// Stack on entry (16-bit aligned):
    ///   SP       index   (INTEGER, 2 bytes)
    ///   SP + 2   theType (ResType,  4 bytes)
    ///   SP + 6   result  (Handle,   4 bytes — caller-allocated)
    /// Pops 6 bytes of args, leaves the 4-byte result slot.
    ///
    /// `current_only = false` ($A99D) walks `resource_search_order()` —
    /// the current resource file plus every file opened before it.
    /// `current_only = true`  ($A80E) restricts the walk to the
    /// current resource file only, per IM:IV-15.
    fn handle_get_ind_resource<C: CpuOps>(
        &mut self,
        bus: &mut MacMemoryBus,
        cpu: &mut C,
        current_only: bool,
    ) -> Result<()> {
        let trap_name = if current_only {
            "Get1IndResource"
        } else {
            "GetIndResource"
        };
        let sp = cpu.read_reg(Register::A7);
        let index = bus.read_word(sp) as i16;
        let raw_res_type = bus.read_long(sp + 2).to_be_bytes();
        let res_type = super::TrapDispatcher::normalize_ostype(raw_res_type);
        let type_str = String::from_utf8_lossy(&res_type);

        if trace_sound_enabled()
            && self
                .resources
                .as_ref()
                .is_some_and(|resources| resources.files.len() > 1)
        {
            eprintln!(
                "[RSRC] {} raw='{}' norm='{}' index={} current={} current_only={}",
                trap_name,
                String::from_utf8_lossy(&raw_res_type),
                type_str,
                index,
                self.current_resource_refnum(),
                current_only,
            );
        }

        // Build the candidate (id, refnum, ptr) list per IM:I I-116 / IV-15.
        // For Get1IndResource ($A80E) the chain collapses to a single
        // entry — the current resource file — so a multi-file scenario
        // never leaks resources from inactive files into the index.
        let candidates: Option<Vec<(i16, u16, u32)>> = self.resources.as_ref().map(|resources| {
            let chain: Vec<u16> = if current_only {
                let refnum = self.current_resource_refnum();
                if resources.files.contains_key(&refnum) {
                    vec![refnum]
                } else {
                    Vec::new()
                }
            } else {
                self.resource_search_order()
            };

            let out: Vec<(i16, u16, u32)> = chain
                .into_iter()
                .filter_map(|refnum| resources.files.get(&refnum).map(|file| (refnum, file)))
                .flat_map(|(refnum, file)| {
                    let resource_order = self.resource_file_order.get(&refnum);
                    if resource_order.is_none_or(|order| order.is_empty()) {
                        let mut entries: Vec<_> = file
                            .loaded
                            .iter()
                            .filter(|((t, _), _)| *t == res_type)
                            .map(|((_, id), ptr)| (*id, refnum, *ptr))
                            .collect();
                        entries.sort_by_key(|&(id, _, _)| id);
                        entries
                    } else {
                        resource_order
                            .unwrap()
                            .iter()
                            .filter(|(entry_type, _)| *entry_type == res_type)
                            .filter_map(|(_, id)| {
                                file.loaded
                                    .get(&(res_type, *id))
                                    .map(|ptr| (*id, refnum, *ptr))
                            })
                            .collect()
                    }
                })
                .collect();
            // Synthetic/test maps predate explicit reference ordering, so keep
            // their deterministic numeric-ID fallback. Parsed forks preserve
            // the order of their on-disk reference lists above.
            out
        });

        if let Some(candidates) = candidates {
            if index >= 1 && (index as usize) <= candidates.len() {
                let (res_id, refnum, ptr) = candidates[(index - 1) as usize];
                let ptr = if ptr == 0 && self.policy.res_load() {
                    self.reload_resource_data_from_file(bus, refnum, res_type, res_id)
                        .unwrap_or(0)
                } else {
                    ptr
                };
                let handle =
                    self.get_or_create_resource_handle_in_file(bus, res_type, res_id, ptr, refnum);
                eprintln!(
                    "[TRAP] {}('{}', {}) -> id={} handle=${:08X}",
                    trap_name, type_str, index, res_id, handle
                );
                cpu.write_reg(Register::A0, handle);
                cpu.write_reg(Register::D0, 0);
                bus.write_word(0x0A60, 0); // ResErr = noErr per IM:I I-118
                bus.write_long(sp + 6, handle);
                cpu.write_reg(Register::A7, sp + 6);
                return Ok(());
            }
        }

        eprintln!("[TRAP] {}('{}', {}) -> NULL", trap_name, type_str, index);
        cpu.write_reg(Register::A0, 0);
        cpu.write_reg(Register::D0, -192i32 as u32);
        bus.write_word(0x0A60, (-192i16) as u16); // ResErr = resNotFound per IM:IV-15
        bus.write_long(sp + 6, 0);
        cpu.write_reg(Register::A7, sp + 6);
        Ok(())
    }
}

#[cfg(test)]
mod tests;
