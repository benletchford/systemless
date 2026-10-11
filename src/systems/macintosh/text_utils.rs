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

/// RelString ($A050) trap word.
pub const REL_STRING_TRAP: u16 = 0xA050;

/// RelString trap number (OS trap 0x50).
pub const REL_STRING_TRAP_NUM: u16 = 0x50;

/// Register-level _CmpString ($A03C) / EqualString trap word.
pub const CMP_STRING_TRAP: u16 = 0xA03C;

/// _CmpString trap number (OS trap 0x3C).
pub const CMP_STRING_TRAP_NUM: u16 = 0x3C;

/// Result code indicating first string is relationally less than second string.
pub const REL_STRING_LESS: i16 = -1;

/// Result code indicating two strings are relationally equal.
pub const REL_STRING_EQUAL: i16 = 0;

/// Result code indicating first string is relationally greater than second string.
pub const REL_STRING_GREATER: i16 = 1;

/// Strip diacritical marks from a Mac Roman character without case conversion.
/// Per Inside Macintosh IV, IV-235 stripping table.
pub fn mac_roman_strip_diacriticals(ch: u8) -> u8 {
    match ch {
        // Uppercase accented → uppercase base
        0x80 => b'A', // Ä → A
        0x81 => b'A', // Å → A
        0x82 => b'C', // Ç → C
        0x83 => b'E', // É → E
        0x84 => b'N', // Ñ → N
        0x85 => b'O', // Ö → O
        0x86 => b'U', // Ü → U
        0xAE => b'A', // Æ → A
        0xAF => b'O', // Ø → O
        0xCB => b'A', // À → A
        0xCC => b'A', // Ã → A
        0xCD => b'O', // Õ → O
        0xCE => b'O', // Œ → O
        // Lowercase accented → lowercase base
        0x87 => b'a', // á → a
        0x88 => b'a', // à → a
        0x89 => b'a', // â → a
        0x8A => b'a', // ä → a
        0x8B => b'a', // ã → a
        0x8C => b'a', // å → a
        0x8D => b'c', // ç → c
        0x8E => b'e', // é → e
        0x8F => b'e', // è → e
        0x90 => b'e', // ê → e
        0x91 => b'e', // ë → e
        0x92 => b'i', // í → i
        0x93 => b'i', // ì → i
        0x94 => b'i', // î → i
        0x95 => b'i', // ï → i
        0x96 => b'n', // ñ → n
        0x97 => b'o', // ó → o
        0x98 => b'o', // ò → o
        0x99 => b'o', // ô → o
        0x9A => b'o', // ö → o
        0x9B => b'o', // õ → o
        0x9C => b'u', // ú → u
        0x9D => b'u', // ù → u
        0x9E => b'u', // û → u
        0x9F => b'u', // ü → u
        0xBB => b'a', // ª → a
        0xBC => b'o', // º → o
        0xBE => b'a', // æ → a
        0xBF => b'o', // ø → o
        0xCF => b'o', // œ → o
        0xD8 => b'y', // ÿ → y
        _ => ch,
    }
}

/// Convert a Mac Roman character to uppercase per Inside Macintosh IV, IV-235.
/// If `strip_marks` is true, diacritical marks are also stripped.
pub fn mac_roman_to_upper(ch: u8, strip_marks: bool) -> u8 {
    if strip_marks {
        let stripped = mac_roman_strip_diacriticals(ch);
        if stripped.is_ascii_lowercase() {
            return stripped - 32;
        }
        return stripped;
    }
    if ch.is_ascii_lowercase() {
        return ch - 32;
    }
    match ch {
        0x88 => 0xCB, // à → À
        0x8A => 0x80, // ä → Ä
        0x8B => 0xCC, // ã → Ã
        0x8C => 0x81, // å → Å
        0x8D => 0x82, // ç → Ç
        0x8E => 0x83, // é → É
        0x96 => 0x84, // ñ → Ñ
        0x9A => 0x85, // ö → Ö
        0x9B => 0xCD, // õ → Õ
        0x9F => 0x86, // ü → Ü
        0xBE => 0xAE, // æ → Æ
        0xBF => 0xAF, // ø → Ø
        0xCF => 0xCE, // œ → Œ
        _ => ch,
    }
}

/// Convert a Mac Roman character to lowercase.
pub fn mac_roman_to_lower(ch: u8) -> u8 {
    if ch.is_ascii_uppercase() {
        return ch + 32;
    }
    match ch {
        0x80 => 0x8A, // Ä → ä
        0x81 => 0x8C, // Å → å
        0x82 => 0x8D, // Ç → ç
        0x83 => 0x8E, // É → é
        0x84 => 0x96, // Ñ → ñ
        0x85 => 0x9A, // Ö → ö
        0x86 => 0x9F, // Ü → ü
        0xAE => 0xBE, // Æ → æ
        0xAF => 0xBF, // Ø → ø
        0xCB => 0x88, // À → à
        0xCC => 0x8B, // Ã → ã
        0xCD => 0x9B, // Õ → õ
        0xCE => 0xCF, // Œ → œ
        _ => ch,
    }
}

/// Compares two character byte slices relationally.
///
/// If `diac_sensitive` is false, diacritical marks are stripped via Mac Roman rules.
/// If `case_sensitive` is false, characters are folded to uppercase.
///
/// Returns:
/// - `-1` (`REL_STRING_LESS`) if `str_a < str_b`
/// - `0` (`REL_STRING_EQUAL`) if `str_a == str_b`
/// - `1` (`REL_STRING_GREATER`) if `str_a > str_b`
pub fn evaluate_rel_string(
    str_a: &[u8],
    str_b: &[u8],
    case_sensitive: bool,
    diac_sensitive: bool,
) -> i16 {
    let min_len = std::cmp::min(str_a.len(), str_b.len());
    for i in 0..min_len {
        let mut a = str_a[i];
        let mut b = str_b[i];
        if !diac_sensitive {
            a = mac_roman_strip_diacriticals(a);
            b = mac_roman_strip_diacriticals(b);
        }
        if !case_sensitive {
            a = mac_roman_to_upper(a, false);
            b = mac_roman_to_upper(b, false);
        }
        if a < b {
            return REL_STRING_LESS;
        } else if a > b {
            return REL_STRING_GREATER;
        }
    }
    match str_a.len().cmp(&str_b.len()) {
        std::cmp::Ordering::Less => REL_STRING_LESS,
        std::cmp::Ordering::Equal => REL_STRING_EQUAL,
        std::cmp::Ordering::Greater => REL_STRING_GREATER,
    }
}

/// Tests equality of two character byte slices with specified sensitivity.
///
/// If lengths differ, returns `false` immediately.
/// Otherwise, compares characters after optional diacritic stripping and case folding.
pub fn evaluate_equal_string(
    str_a: &[u8],
    str_b: &[u8],
    case_sensitive: bool,
    diac_sensitive: bool,
) -> bool {
    if str_a.len() != str_b.len() {
        return false;
    }
    str_a.iter().zip(str_b.iter()).all(|(&a_byte, &b_byte)| {
        let mut a = a_byte;
        let mut b = b_byte;
        if !diac_sensitive {
            a = mac_roman_strip_diacriticals(a);
            b = mac_roman_strip_diacriticals(b);
        }
        if !case_sensitive {
            a = mac_roman_to_upper(a, false);
            b = mac_roman_to_upper(b, false);
        }
        a == b
    })
}

/// Compares two strings using primary Mac Roman ordering (case and diacritics ignored).
///
/// Used by International Utilities `IUEqualString` and `IUCompString` (Inside Macintosh I, p. I-506).
pub fn evaluate_iu_comp_string(str_a: &[u8], str_b: &[u8]) -> i16 {
    let min_len = std::cmp::min(str_a.len(), str_b.len());
    for i in 0..min_len {
        let a = mac_roman_to_upper(str_a[i], true);
        let b = mac_roman_to_upper(str_b[i], true);
        if a < b {
            return REL_STRING_LESS;
        } else if a > b {
            return REL_STRING_GREATER;
        }
    }
    match str_a.len().cmp(&str_b.len()) {
        std::cmp::Ordering::Less => REL_STRING_LESS,
        std::cmp::Ordering::Equal => REL_STRING_EQUAL,
        std::cmp::Ordering::Greater => REL_STRING_GREATER,
    }
}

/// Tests equality of two strings using primary Mac Roman ordering.
///
/// Returns `true` if equal under primary ordering.
pub fn evaluate_iu_equal_string(str_a: &[u8], str_b: &[u8]) -> bool {
    if str_a.len() != str_b.len() {
        return false;
    }
    str_a
        .iter()
        .map(|&byte| mac_roman_to_upper(byte, true))
        .eq(str_b.iter().map(|&byte| mac_roman_to_upper(byte, true)))
}

/// UpperString / UprString ($A054) trap word.
pub const UPR_STRING_TRAP: u16 = 0xA054;

/// UpperString / UprString trap number (OS trap 0x54).
pub const UPR_STRING_TRAP_NUM: u16 = 0x54;

/// Text conversion utility family base trap word ($A056).
pub const TEXT_CONVERT_TRAP: u16 = 0xA056;

/// Text conversion utility family trap number (OS trap 0x56).
pub const TEXT_CONVERT_TRAP_NUM: u16 = 0x56;

/// LowerText ($A056) trap word.
pub const LOWER_TEXT_TRAP: u16 = 0xA056;

/// StripText ($A256) trap word.
pub const STRIP_TEXT_TRAP: u16 = 0xA256;

/// UpperText ($A456) trap word.
pub const UPPER_TEXT_TRAP: u16 = 0xA456;

/// StripUpperText ($A656) trap word.
pub const STRIP_UPPER_TEXT_TRAP: u16 = 0xA656;

/// Text transformation operations supported by classic Macintosh Text Utilities.
///
/// References:
/// - Inside Macintosh Volume VI (1991), 14-62 to 14-63
/// - Inside Macintosh: Text (1993), pp. 5-64 to 5-66
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextTransformOperation {
    /// Convert Roman characters to lowercase (`LowerText`).
    Lower,
    /// Strip diacritical marks from Roman characters without case change (`StripText`).
    StripDiacritics,
    /// Convert Roman characters to uppercase preserving diacritical marks (`UpperText`).
    Upper,
    /// Convert Roman characters to uppercase and strip diacritical marks (`StripUpperText`).
    StripUpper,
}

/// Transforms a single Mac Roman character according to the specified operation.
#[inline]
pub fn evaluate_transform_char(ch: u8, op: TextTransformOperation) -> u8 {
    match op {
        TextTransformOperation::Lower => mac_roman_to_lower(ch),
        TextTransformOperation::StripDiacritics => mac_roman_strip_diacriticals(ch),
        TextTransformOperation::Upper => mac_roman_to_upper(ch, false),
        TextTransformOperation::StripUpper => mac_roman_to_upper(ch, true),
    }
}

/// Transforms a slice of Mac Roman characters in-place.
pub fn evaluate_transform_text(bytes: &mut [u8], op: TextTransformOperation) {
    for b in bytes.iter_mut() {
        *b = evaluate_transform_char(*b, op);
    }
}

/// Converts a Pascal or raw string to uppercase in-place (`UpperString` / `UprString`).
///
/// If `diac_sensitive` is `true`, diacritical marks are retained (`strip_diacritics = false`).
/// If `diac_sensitive` is `false`, diacritical marks are stripped (`strip_diacritics = true`).
///
/// References:
/// - Inside Macintosh: Text (1993), pp. 5-64 to 5-65.
pub fn evaluate_upper_string(bytes: &mut [u8], diac_sensitive: bool) {
    let strip_diacritics = !diac_sensitive;
    for b in bytes.iter_mut() {
        *b = mac_roman_to_upper(*b, strip_diacritics);
    }
}

/// Binary-Decimal Conversion Package (Pack7) trap word ($A9EE).
pub const PACK7_TRAP: u16 = 0xA9EE;

/// Toolbox trap offset for Pack7 ($A9EE & 0x03FF = 0x01EE).
pub const PACK7_TRAP_OFFSET: u16 = 0x01EE;

/// Package selector for NumToString (0).
pub const NUM_TO_STRING_SELECTOR: u16 = 0;

/// Package selector for StringToNum (1).
pub const STRING_TO_NUM_SELECTOR: u16 = 1;

/// Converts a 32-bit signed integer into decimal ASCII bytes (`NumToString`).
///
/// References:
/// - Inside Macintosh Volume I (1985), p. I-489
/// - Inside Macintosh: Text (1993), p. 5-73
#[inline]
pub fn evaluate_num_to_string(number: i32) -> Vec<u8> {
    format!("{}", number).into_bytes()
}

/// Parses a 32-bit signed integer from ASCII / Pascal string bytes (`StringToNum`).
///
/// Skips leading whitespace, parses an optional '+' or '-' sign, and converts
/// consecutive decimal digits until the first non-digit character.
/// Overflow behavior wraps using standard 32-bit signed two's complement arithmetic.
///
/// References:
/// - Inside Macintosh Volume I (1985), p. I-490
/// - Inside Macintosh: Text (1993), p. 5-73
pub fn evaluate_string_to_num(bytes: &[u8]) -> i32 {
    let mut index = 0usize;
    while bytes.get(index).is_some_and(|byte| byte.is_ascii_whitespace()) {
        index += 1;
    }
    let mut sign = 1i64;
    if let Some(&byte) = bytes.get(index) {
        if byte == b'-' {
            sign = -1;
            index += 1;
        } else if byte == b'+' {
            index += 1;
        }
    }
    let mut value = 0i64;
    while let Some(&byte) = bytes.get(index) {
        if !byte.is_ascii_digit() {
            break;
        }
        value = value.wrapping_mul(10).wrapping_add(i64::from(byte - b'0'));
        index += 1;
    }
    value.wrapping_mul(sign) as i32
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

    #[test]
    fn test_comparison_constants() {
        assert_eq!(REL_STRING_TRAP, 0xA050);
        assert_eq!(REL_STRING_TRAP_NUM, 0x50);
        assert_eq!(CMP_STRING_TRAP, 0xA03C);
        assert_eq!(CMP_STRING_TRAP_NUM, 0x3C);
        assert_eq!(REL_STRING_LESS, -1);
        assert_eq!(REL_STRING_EQUAL, 0);
        assert_eq!(REL_STRING_GREATER, 1);
    }

    #[test]
    fn test_mac_roman_casing_and_diacritics() {
        assert_eq!(mac_roman_strip_diacriticals(0x8E), b'e');
        assert_eq!(mac_roman_to_upper(b'e', false), b'E');
        assert_eq!(mac_roman_to_upper(0x8E, false), 0x83); // é → É
        assert_eq!(mac_roman_to_upper(0x8E, true), b'E');
        assert_eq!(mac_roman_to_lower(b'E'), b'e');
        assert_eq!(mac_roman_to_lower(0x83), 0x8E); // É → é
    }

    #[test]
    fn test_evaluate_rel_string_matrix() {
        let accented_lower = [0x8E]; // é
        let plain_lower = [b'e'];
        let plain_upper = [b'E'];

        // bare: case=false, diac=false
        assert_eq!(evaluate_rel_string(&accented_lower, &plain_upper, false, false), 0);
        assert_eq!(evaluate_rel_string(&plain_lower, &plain_upper, false, false), 0);

        // marks only: case=false, diac=true
        assert_eq!(evaluate_rel_string(&accented_lower, &plain_upper, false, true), 1);
        assert_eq!(evaluate_rel_string(&plain_lower, &plain_upper, false, true), 0);

        // case only: case=true, diac=false
        assert_eq!(evaluate_rel_string(&accented_lower, &plain_upper, true, false), 1);
        assert_eq!(evaluate_rel_string(&plain_lower, &plain_upper, true, false), 1);

        // both: case=true, diac=true
        assert_eq!(evaluate_rel_string(&accented_lower, &plain_upper, true, true), 1);
        assert_eq!(evaluate_rel_string(&plain_lower, &plain_upper, true, true), 1);

        // differing lengths
        assert_eq!(evaluate_rel_string(b"foo", b"foobar", false, false), -1);
        assert_eq!(evaluate_rel_string(b"foobar", b"foo", false, false), 1);
    }

    #[test]
    fn test_evaluate_equal_string_matrix() {
        let accented_lower = [0x8E]; // é
        let plain_lower = [b'e'];
        let plain_upper = [b'E'];

        assert!(evaluate_equal_string(&accented_lower, &plain_upper, false, false));
        assert!(evaluate_equal_string(&plain_lower, &plain_upper, false, false));

        assert!(!evaluate_equal_string(&accented_lower, &plain_upper, false, true));
        assert!(evaluate_equal_string(&plain_lower, &plain_upper, false, true));

        assert!(!evaluate_equal_string(&accented_lower, &plain_upper, true, false));
        assert!(!evaluate_equal_string(&plain_lower, &plain_upper, true, false));

        assert!(!evaluate_equal_string(&accented_lower, &plain_upper, true, true));
        assert!(!evaluate_equal_string(&plain_lower, &plain_upper, true, true));

        assert!(!evaluate_equal_string(b"short", b"longer", false, false));
    }

    #[test]
    fn test_evaluate_iu_comp_and_equal() {
        assert_eq!(evaluate_iu_comp_string(b"Rose", b"ros\x8E"), 0);
        assert!(evaluate_iu_equal_string(b"Rose", b"ros\x8E"));

        assert_eq!(evaluate_iu_comp_string(b"Rose", b"Rope"), 1);
        assert!(!evaluate_iu_equal_string(b"Rose", b"Rope"));

        assert_eq!(evaluate_iu_comp_string(b"Rope", b"Rose"), -1);
    }

    #[test]
    fn test_text_transform_constants() {
        assert_eq!(UPR_STRING_TRAP, 0xA054);
        assert_eq!(UPR_STRING_TRAP_NUM, 0x54);
        assert_eq!(TEXT_CONVERT_TRAP, 0xA056);
        assert_eq!(TEXT_CONVERT_TRAP_NUM, 0x56);
        assert_eq!(LOWER_TEXT_TRAP, 0xA056);
        assert_eq!(STRIP_TEXT_TRAP, 0xA256);
        assert_eq!(UPPER_TEXT_TRAP, 0xA456);
        assert_eq!(STRIP_UPPER_TEXT_TRAP, 0xA656);
    }

    #[test]
    fn test_evaluate_transform_text_operations() {
        // Source text: "Café 123" with Mac Roman 'é' = 0x8E
        let text = vec![b'C', b'a', b'f', 0x8E, b' ', b'1', b'2', b'3'];

        // LowerText
        let mut lower = text.clone();
        evaluate_transform_text(&mut lower, TextTransformOperation::Lower);
        assert_eq!(lower, vec![b'c', b'a', b'f', 0x8E, b' ', b'1', b'2', b'3']);

        // StripText (strips diacritics without case folding)
        let mut strip = text.clone();
        evaluate_transform_text(&mut strip, TextTransformOperation::StripDiacritics);
        assert_eq!(strip, vec![b'C', b'a', b'f', b'e', b' ', b'1', b'2', b'3']);

        // UpperText (uppercases while preserving diacritics: 0x8E 'é' -> 0x83 'É')
        let mut upper = text.clone();
        evaluate_transform_text(&mut upper, TextTransformOperation::Upper);
        assert_eq!(upper, vec![b'C', b'A', b'F', 0x83, b' ', b'1', b'2', b'3']);

        // StripUpperText (uppercases and strips diacritics: 0x8E 'é' -> 'E')
        let mut strip_upper = text.clone();
        evaluate_transform_text(&mut strip_upper, TextTransformOperation::StripUpper);
        assert_eq!(strip_upper, vec![b'C', b'A', b'F', b'E', b' ', b'1', b'2', b'3']);
    }

    #[test]
    fn test_evaluate_upper_string() {
        // String with accented character 'é' = 0x8E
        let mut str1 = vec![b'c', b'a', b'f', 0x8E];
        // diac_sensitive = true (preserves marks -> 0x83 'É')
        evaluate_upper_string(&mut str1, true);
        assert_eq!(str1, vec![b'C', b'A', b'F', 0x83]);

        let mut str2 = vec![b'c', b'a', b'f', 0x8E];
        // diac_sensitive = false (strips marks -> 'E')
        evaluate_upper_string(&mut str2, false);
        assert_eq!(str2, vec![b'C', b'A', b'F', b'E']);
    }

    #[test]
    fn test_binary_decimal_constants() {
        assert_eq!(PACK7_TRAP, 0xA9EE);
        assert_eq!(PACK7_TRAP_OFFSET, 0x01EE);
        assert_eq!(NUM_TO_STRING_SELECTOR, 0);
        assert_eq!(STRING_TO_NUM_SELECTOR, 1);
    }

    #[test]
    fn test_evaluate_num_to_string() {
        assert_eq!(evaluate_num_to_string(0), b"0");
        assert_eq!(evaluate_num_to_string(42), b"42");
        assert_eq!(evaluate_num_to_string(-1234), b"-1234");
        assert_eq!(evaluate_num_to_string(i32::MAX), b"2147483647");
        assert_eq!(evaluate_num_to_string(i32::MIN), b"-2147483648");
    }

    #[test]
    fn test_evaluate_string_to_num() {
        assert_eq!(evaluate_string_to_num(b"0"), 0);
        assert_eq!(evaluate_string_to_num(b"42"), 42);
        assert_eq!(evaluate_string_to_num(b"-1234"), -1234);
        assert_eq!(evaluate_string_to_num(b"+567"), 567);
        assert_eq!(evaluate_string_to_num(b"   123"), 123);
        assert_eq!(evaluate_string_to_num(b"  -456"), -456);
        assert_eq!(evaluate_string_to_num(b"  +789"), 789);
        assert_eq!(evaluate_string_to_num(b"123abc"), 123);
        assert_eq!(evaluate_string_to_num(b"  -99xyz"), -99);
        assert_eq!(evaluate_string_to_num(b"not a number"), 0);
        assert_eq!(evaluate_string_to_num(b""), 0);
        assert_eq!(evaluate_string_to_num(b"2147483647"), i32::MAX);
    }
}
