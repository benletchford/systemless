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
}
