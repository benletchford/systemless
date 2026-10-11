//! Architecture-neutral classic Macintosh Text Utilities evaluation.
//!
//! Provides canonical evaluation for text and handle manipulation operations,
//! primarily the classic `Munger` function (trap `$A9E0` / InterfaceLib `Munger`).
//!
//! References:
//! - Inside Macintosh Volume I (1985), pp. I-468 to I-469
//! - Inside Macintosh: Text (1993), pp. 5-75 to 5-77

#![allow(dead_code)]

use std::sync::OnceLock;

/// Trap word for Munger (`$A9E0`).
pub const MUNGER_TRAP: u16 = 0xA9E0;

/// Toolbox trap offset for Munger (`$A9E0 & 0x03FF = 0x01E0`).
pub const MUNGER_TRAP_OFFSET: u16 = 0x01E0;

/// Phantom trap word for XMunger (`$A819`).
pub const XMUNGER_TRAP: u16 = 0xA819;

/// Toolbox trap offset for XMunger (`$A819 & 0x03FF = 0x0019`).
pub const XMUNGER_TRAP_OFFSET: u16 = 0x0019;

static TRACE_MUNGER: OnceLock<bool> = OnceLock::new();

/// Returns true if Munger diagnostic tracing is enabled via `SYSTEMLESS_TRACE_MUNGER`.
pub fn trace_munger_enabled() -> bool {
    *TRACE_MUNGER.get_or_init(|| std::env::var_os("SYSTEMLESS_TRACE_MUNGER").is_some())
}

/// Identifies the high-level semantic mode of a Munger operation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MungerMode {
    /// Searches for needle without modifying destination bytes (`ptr2 == NIL`, `ptr1 != NIL`).
    SearchOnly,
    /// Inserts replacement bytes at offset without removing existing bytes (`len1 == 0`).
    Insert,
    /// Deletes the target substring (`len2 == 0`, `ptr2 != NIL`).
    Delete,
    /// Replaces the target substring with replacement bytes (`len1 > 0`, `len2 > 0`).
    Replace,
}

/// Result of evaluating a Munger operation against a destination slice.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MungerEvaluation {
    /// Return offset: match offset, offset past replacement/insertion, or -1 on mismatch/error.
    pub return_offset: i32,
    /// Updated destination buffer if modified; `None` if unmodified (search-only or error).
    pub new_data: Option<Vec<u8>>,
    /// Semantic mode resolved for the operation.
    pub mode: MungerMode,
}

impl MungerEvaluation {
    /// Returns true if the operation succeeded (return_offset >= 0).
    #[inline]
    pub fn is_success(&self) -> bool {
        self.return_offset >= 0
    }

    /// Returns true if the destination data was modified.
    #[inline]
    pub fn is_modified(&self) -> bool {
        self.new_data.is_some()
    }
}

/// Canonical architecture-neutral evaluation of the classic Macintosh `Munger` function.
///
/// Parameters:
/// - `data`: The existing destination bytes in the handle.
/// - `offset`: Zero-based start offset in the destination text.
/// - `ptr1_is_null`: True if `ptr1` was `NIL` (0).
/// - `needle`: Target search bytes (from `ptr1`, of length `len1` if `len1 > 0`).
/// - `len1`: Length of target search string, or special negative/zero flags.
/// - `ptr2_is_null`: True if `ptr2` was `NIL` (0).
/// - `replacement`: Substitution bytes (from `ptr2`, of length `len2` if `len2 > 0`).
/// - `len2`: Length of substitution string.
pub fn evaluate_munger(
    data: &[u8],
    offset: i32,
    ptr1_is_null: bool,
    needle: &[u8],
    len1: i32,
    ptr2_is_null: bool,
    replacement: &[u8],
    len2: i32,
) -> MungerEvaluation {
    if offset < 0 {
        return MungerEvaluation {
            return_offset: -1,
            new_data: None,
            mode: if ptr2_is_null {
                MungerMode::SearchOnly
            } else {
                MungerMode::Replace
            },
        };
    }

    let offset = offset as usize;
    if offset > data.len() {
        return MungerEvaluation {
            return_offset: -1,
            new_data: None,
            mode: if ptr2_is_null {
                MungerMode::SearchOnly
            } else {
                MungerMode::Replace
            },
        };
    }

    let mut replace_offset = offset;
    let mut replace_len = len1.max(0) as usize;

    if !ptr1_is_null && len1 > 0 {
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
            return MungerEvaluation {
                return_offset: -1,
                new_data: None,
                mode: if ptr2_is_null {
                    MungerMode::SearchOnly
                } else {
                    MungerMode::Replace
                },
            };
        };

        replace_offset = found_offset;
        if full_match {
            replace_len = needle.len();
        } else {
            // BasiliskII/System 7.5 ROM does not perform the Apple-documented
            // tail-partial replacement here; it treats the partial tail match
            // as not found and leaves the destination bytes unchanged.
            return MungerEvaluation {
                return_offset: -1,
                new_data: None,
                mode: if ptr2_is_null {
                    MungerMode::SearchOnly
                } else {
                    MungerMode::Replace
                },
            };
        }
    } else if ptr1_is_null && len1 < 0 {
        replace_len = data.len() - offset;
    }

    replace_len = replace_len.min(data.len().saturating_sub(replace_offset));

    if ptr2_is_null && !ptr1_is_null {
        return MungerEvaluation {
            return_offset: replace_offset as i32,
            new_data: None,
            mode: MungerMode::SearchOnly,
        };
    }

    let effective_replacement = if ptr2_is_null || len2 <= 0 {
        &[][..]
    } else {
        let rep_len = (len2 as usize).min(replacement.len());
        &replacement[..rep_len]
    };

    let tail_start = replace_offset + replace_len;
    let mut new_data = Vec::with_capacity(data.len() - replace_len + effective_replacement.len());
    new_data.extend_from_slice(&data[..replace_offset]);
    new_data.extend_from_slice(effective_replacement);
    new_data.extend_from_slice(&data[tail_start..]);

    let return_offset = (replace_offset + effective_replacement.len()) as i32;
    let mode = if replace_len == 0 {
        MungerMode::Insert
    } else if effective_replacement.is_empty() {
        MungerMode::Delete
    } else {
        MungerMode::Replace
    };

    MungerEvaluation {
        return_offset,
        new_data: Some(new_data),
        mode,
    }
}

/// ScriptUtil selector for TruncString (`$8208FFE0`).
pub const TRUNC_STRING_SELECTOR: u32 = 0x8208_FFE0;

/// ScriptUtil selector for TruncText (`$820CFFDE`).
pub const TRUNC_TEXT_SELECTOR: u32 = 0x820C_FFDE;

/// Truncation location: end of string (`smTruncEnd = 0`).
pub const TRUNC_END: u16 = 0;

/// Truncation location: middle of string (`smTruncMiddle = 0x4000`).
pub const TRUNC_MIDDLE: u16 = 0x4000;

/// Truncation result: no truncation required (`smNotTruncated = 0`).
pub const TRUNC_NOT_TRUNCATED: i16 = 0;

/// Truncation result: string was truncated (`smTruncated = 1`).
pub const TRUNC_TRUNCATED: i16 = 1;

/// Truncation result: truncation error (`smTruncErr = -1`).
pub const TRUNC_ERR: i16 = -1;

/// Single-byte ellipsis in Macintosh Roman character set (`$C9`, `…`).
pub const MAC_ROMAN_ELLIPSIS: u8 = 0xC9;

/// Truncation location specifying where the ellipsis should be inserted.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TruncWhere {
    /// Truncate at the end of the string (`smTruncEnd = 0`).
    End,
    /// Truncate in the middle of the string (`smTruncMiddle = 0x4000`).
    Middle,
}

impl TruncWhere {
    /// Parses a classic Macintosh `truncWhere` code.
    pub fn from_code(code: u16) -> Option<Self> {
        match code {
            0 => Some(Self::End),
            0x4000 => Some(Self::Middle),
            _ => None,
        }
    }

    /// Converts to the classic Macintosh `truncWhere` code.
    pub fn to_code(self) -> u16 {
        match self {
            Self::End => TRUNC_END,
            Self::Middle => TRUNC_MIDDLE,
        }
    }
}

/// Result of evaluating a `TruncText` or `TruncString` operation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TruncEvaluation {
    /// Result code: `TRUNC_NOT_TRUNCATED` (0), `TRUNC_TRUNCATED` (1), or `TRUNC_ERR` (-1).
    pub result: i16,
    /// New truncated byte vector if truncation occurred, or `None` if unchanged or error.
    pub new_text: Option<Vec<u8>>,
}

impl TruncEvaluation {
    /// Returns true if the string was truncated.
    #[inline]
    pub fn is_truncated(&self) -> bool {
        self.result == TRUNC_TRUNCATED
    }

    /// Returns true if the operation completed successfully without error.
    #[inline]
    pub fn is_success(&self) -> bool {
        self.result >= 0
    }
}

/// Standard monospace 6-pixel glyph advance measuring fallback.
#[inline]
pub fn default_char_width_measure(bytes: &[u8]) -> i16 {
    (bytes.len() as i16).saturating_mul(6)
}

/// Evaluates text truncation (`TruncText`).
///
/// References:
/// - Inside Macintosh: Text (1993), Chapter 5 "Text Utilities", pp. 5-77..5-78
///
/// Parameters:
/// - `text`: Byte slice of the text to evaluate.
/// - `width`: Maximum allowable pixel width (or measured units).
/// - `trunc_where`: Truncation location code (`0` for end, `0x4000` for middle).
/// - `ellipsis`: Character byte to use for the ellipsis (e.g. `MAC_ROMAN_ELLIPSIS = 0xC9`).
/// - `measure`: Closure that returns the measured width of a byte slice.
pub fn evaluate_trunc_text(
    text: &[u8],
    width: i16,
    trunc_where: u16,
    ellipsis: u8,
    mut measure: impl FnMut(&[u8]) -> i16,
) -> TruncEvaluation {
    let Some(where_) = TruncWhere::from_code(trunc_where) else {
        return TruncEvaluation {
            result: TRUNC_ERR,
            new_text: None,
        };
    };

    if width < 0 {
        return TruncEvaluation {
            result: TRUNC_ERR,
            new_text: None,
        };
    }

    if measure(text) <= width {
        return TruncEvaluation {
            result: TRUNC_NOT_TRUNCATED,
            new_text: None,
        };
    }

    if measure(&[ellipsis]) > width {
        return TruncEvaluation {
            result: TRUNC_ERR,
            new_text: None,
        };
    }

    for retained in (0..text.len()).rev() {
        let left = match where_ {
            TruncWhere::Middle => retained.div_ceil(2),
            TruncWhere::End => retained,
        };
        let right = retained - left;

        let mut candidate = Vec::with_capacity(retained + 1);
        candidate.extend_from_slice(&text[..left]);
        candidate.push(ellipsis);
        candidate.extend_from_slice(&text[text.len() - right..]);

        if measure(&candidate) <= width {
            return TruncEvaluation {
                result: TRUNC_TRUNCATED,
                new_text: Some(candidate),
            };
        }
    }

    TruncEvaluation {
        result: TRUNC_ERR,
        new_text: None,
    }
}

/// Evaluates Pascal string truncation (`TruncString`).
///
/// References:
/// - Inside Macintosh: Text (1993), Chapter 5 "Text Utilities", pp. 5-77..5-78
///
/// Parameters:
/// - `string_bytes`: Byte slice of the Pascal string content (excluding length byte).
/// - `width`: Maximum allowable pixel width.
/// - `trunc_where`: Truncation location code (`0` for end, `0x4000` for middle).
/// - `ellipsis`: Character byte to use for the ellipsis (e.g. `MAC_ROMAN_ELLIPSIS = 0xC9`).
/// - `measure`: Closure that returns the measured width of a byte slice.
pub fn evaluate_trunc_string(
    string_bytes: &[u8],
    width: i16,
    trunc_where: u16,
    ellipsis: u8,
    measure: impl FnMut(&[u8]) -> i16,
) -> TruncEvaluation {
    let clamped_len = string_bytes.len().min(255);
    evaluate_trunc_text(
        &string_bytes[..clamped_len],
        width,
        trunc_where,
        ellipsis,
        measure,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_munger_replaces_first_occurrence() {
        let data = b"Hello, ^0 world";
        let needle = b"^0";
        let replacement = b"Ace";
        let eval = evaluate_munger(
            data,
            0,
            false,
            needle,
            needle.len() as i32,
            false,
            replacement,
            replacement.len() as i32,
        );

        assert_eq!(eval.return_offset, 10);
        assert_eq!(eval.mode, MungerMode::Replace);
        assert!(eval.is_success());
        assert!(eval.is_modified());
        assert_eq!(
            eval.new_data.as_deref(),
            Some(b"Hello, Ace world".as_slice())
        );
    }

    #[test]
    fn test_munger_search_only_mode() {
        let data = b"there's the apple";
        let needle = b"the";
        let eval = evaluate_munger(
            data,
            4,
            false,
            needle,
            needle.len() as i32,
            true, // ptr2 is null
            &[],
            0,
        );

        assert_eq!(eval.return_offset, 8);
        assert_eq!(eval.mode, MungerMode::SearchOnly);
        assert!(eval.is_success());
        assert!(!eval.is_modified());
        assert_eq!(eval.new_data, None);
    }

    #[test]
    fn test_munger_len1_zero_inserts_at_offset() {
        let data = b"apple";
        let replacement = b"X";
        let eval = evaluate_munger(
            data,
            2,
            false,
            &[],
            0, // len1 == 0 -> insertion
            false,
            replacement,
            replacement.len() as i32,
        );

        assert_eq!(eval.return_offset, 3);
        assert_eq!(eval.mode, MungerMode::Insert);
        assert_eq!(eval.new_data.as_deref(), Some(b"apXple".as_slice()));
    }

    #[test]
    fn test_munger_len2_zero_deletes_target() {
        let data = b"abc123def";
        let needle = b"123";
        let eval = evaluate_munger(
            data,
            0,
            false,
            needle,
            needle.len() as i32,
            false, // ptr2 is non-null
            &[],
            0, // len2 == 0 -> deletion
        );

        assert_eq!(eval.return_offset, 3);
        assert_eq!(eval.mode, MungerMode::Delete);
        assert_eq!(eval.new_data.as_deref(), Some(b"abcdef".as_slice()));
    }

    #[test]
    fn test_munger_nil_ptr1_negative_len1_replaces_tail() {
        let data = b"abcdef";
        let replacement = b"XYZ";
        let eval = evaluate_munger(
            data,
            3,
            true, // ptr1 is null
            &[],
            -1, // len1 < 0 -> replace to end
            false,
            replacement,
            replacement.len() as i32,
        );

        assert_eq!(eval.return_offset, 6);
        assert_eq!(eval.mode, MungerMode::Replace);
        assert_eq!(eval.new_data.as_deref(), Some(b"abcXYZ".as_slice()));
    }

    #[test]
    fn test_munger_nil_ptr1_positive_len1_overwrites_bytes() {
        let data = b"abcdef";
        let replacement = b"99";
        let eval = evaluate_munger(
            data,
            2,
            true, // ptr1 is null
            &[],
            2, // len1 = 2 -> replace 2 bytes at offset 2
            false,
            replacement,
            replacement.len() as i32,
        );

        assert_eq!(eval.return_offset, 4);
        assert_eq!(eval.mode, MungerMode::Replace);
        assert_eq!(eval.new_data.as_deref(), Some(b"ab99ef".as_slice()));
    }

    #[test]
    fn test_munger_target_not_found_returns_negative() {
        let data = b"hello";
        let needle = b"zz";
        let replacement = b"A";
        let eval = evaluate_munger(
            data,
            0,
            false,
            needle,
            needle.len() as i32,
            false,
            replacement,
            replacement.len() as i32,
        );

        assert_eq!(eval.return_offset, -1);
        assert_eq!(eval.new_data, None);
        assert!(!eval.is_success());
    }

    #[test]
    fn test_munger_partial_tail_match_returns_negative() {
        let data = b"ab";
        let needle = b"abc";
        let replacement = b"Z";
        let eval = evaluate_munger(
            data,
            0,
            false,
            needle,
            needle.len() as i32,
            false,
            replacement,
            replacement.len() as i32,
        );

        assert_eq!(eval.return_offset, -1);
        assert_eq!(eval.new_data, None);
    }

    #[test]
    fn test_munger_offset_out_of_bounds() {
        let data = b"abc";
        let eval_negative = evaluate_munger(data, -1, false, b"a", 1, true, &[], 0);
        assert_eq!(eval_negative.return_offset, -1);
        assert_eq!(eval_negative.new_data, None);

        let eval_past_end = evaluate_munger(data, 4, false, b"a", 1, true, &[], 0);
        assert_eq!(eval_past_end.return_offset, -1);
        assert_eq!(eval_past_end.new_data, None);
    }

    #[test]
    fn test_trap_constants() {
        assert_eq!(MUNGER_TRAP, 0xA9E0);
        assert_eq!(XMUNGER_TRAP, 0xA819);
        assert_eq!(MUNGER_TRAP_OFFSET, 0x01E0);
        assert_eq!(XMUNGER_TRAP_OFFSET, 0x0019);
        assert_eq!(0xA800 | MUNGER_TRAP_OFFSET, MUNGER_TRAP);
        assert_eq!(0xA800 | XMUNGER_TRAP_OFFSET, XMUNGER_TRAP);
    }

    #[test]
    fn test_trunc_constants() {
        assert_eq!(TRUNC_STRING_SELECTOR, 0x8208_FFE0);
        assert_eq!(TRUNC_TEXT_SELECTOR, 0x820C_FFDE);
        assert_eq!(TRUNC_END, 0);
        assert_eq!(TRUNC_MIDDLE, 0x4000);
        assert_eq!(TRUNC_NOT_TRUNCATED, 0);
        assert_eq!(TRUNC_TRUNCATED, 1);
        assert_eq!(TRUNC_ERR, -1);
        assert_eq!(MAC_ROMAN_ELLIPSIS, 0xC9);
    }

    #[test]
    fn test_trunc_where_conversions() {
        assert_eq!(TruncWhere::from_code(0), Some(TruncWhere::End));
        assert_eq!(TruncWhere::from_code(0x4000), Some(TruncWhere::Middle));
        assert_eq!(TruncWhere::from_code(1), None);
        assert_eq!(TruncWhere::End.to_code(), 0);
        assert_eq!(TruncWhere::Middle.to_code(), 0x4000);
    }

    #[test]
    fn test_evaluate_trunc_text_fits_untruncated() {
        let text = b"Hello";
        let eval = evaluate_trunc_text(
            text,
            40,
            TRUNC_END,
            MAC_ROMAN_ELLIPSIS,
            default_char_width_measure,
        );
        assert_eq!(eval.result, TRUNC_NOT_TRUNCATED);
        assert_eq!(eval.new_text, None);
        assert!(!eval.is_truncated());
        assert!(eval.is_success());
    }

    #[test]
    fn test_evaluate_trunc_text_truncates_end() {
        let text = b"Hello World";
        let eval = evaluate_trunc_text(
            text,
            36,
            TRUNC_END,
            MAC_ROMAN_ELLIPSIS,
            default_char_width_measure,
        );
        assert_eq!(eval.result, TRUNC_TRUNCATED);
        assert!(eval.is_truncated());
        assert!(eval.is_success());
        let expected = [b'H', b'e', b'l', b'l', b'o', MAC_ROMAN_ELLIPSIS];
        assert_eq!(eval.new_text.as_deref(), Some(&expected[..]));
    }

    #[test]
    fn test_evaluate_trunc_text_truncates_middle() {
        let text = b"Hello World";
        let eval = evaluate_trunc_text(
            text,
            36,
            TRUNC_MIDDLE,
            MAC_ROMAN_ELLIPSIS,
            default_char_width_measure,
        );
        assert_eq!(eval.result, TRUNC_TRUNCATED);
        assert!(eval.is_truncated());
        let expected = [b'H', b'e', b'l', MAC_ROMAN_ELLIPSIS, b'l', b'd'];
        assert_eq!(eval.new_text.as_deref(), Some(&expected[..]));
    }

    #[test]
    fn test_evaluate_trunc_text_too_narrow_returns_err() {
        let text = b"Hello";
        let eval = evaluate_trunc_text(
            text,
            4,
            TRUNC_END,
            MAC_ROMAN_ELLIPSIS,
            default_char_width_measure,
        );
        assert_eq!(eval.result, TRUNC_ERR);
        assert_eq!(eval.new_text, None);
        assert!(!eval.is_success());

        let eval_neg = evaluate_trunc_text(
            text,
            -1,
            TRUNC_END,
            MAC_ROMAN_ELLIPSIS,
            default_char_width_measure,
        );
        assert_eq!(eval_neg.result, TRUNC_ERR);
    }

    #[test]
    fn test_evaluate_trunc_text_invalid_where_code() {
        let text = b"Hello World";
        let eval = evaluate_trunc_text(
            text,
            36,
            0x1234,
            MAC_ROMAN_ELLIPSIS,
            default_char_width_measure,
        );
        assert_eq!(eval.result, TRUNC_ERR);
        assert_eq!(eval.new_text, None);
    }

    #[test]
    fn test_evaluate_trunc_string_truncates_pascal_content() {
        let pstring = b"Application Name";
        let eval = evaluate_trunc_string(
            pstring,
            48,
            TRUNC_END,
            MAC_ROMAN_ELLIPSIS,
            default_char_width_measure,
        );
        assert_eq!(eval.result, TRUNC_TRUNCATED);
        let text = eval.new_text.unwrap();
        assert_eq!(text.len(), 8);
        assert_eq!(text.last(), Some(&MAC_ROMAN_ELLIPSIS));
    }
}
