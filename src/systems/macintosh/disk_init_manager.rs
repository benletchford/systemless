//! Architecture-neutral evaluation helpers and canonical structures for
//! Macintosh Disk Initialization Manager operations.
//!
//! Inside Macintosh Volume II (1985), Chapter 14 ("The Disk Initialization Package"), pp. II-395--II-402.
//! Inside Macintosh: Files (1992), Chapter 5 ("Disk Initialization Manager"), pp. 5-3--5-28.
//!
//! The Disk Initialization Manager (implemented as Package 2, `_Pack2` $A9E9) provides routines
//! for initializing disks and responding to disk-insert events when an uninitialized or damaged disk
//! is inserted. It also handles formatting and zeroing disks.
//!
//! On 68k, operations are dispatched via trap `_Pack2` ($A9E9) with a 16-bit selector on the stack
//! preceding standard Pascal parameters. On PowerPC, `InterfaceLib` exports `DIBadMount`, `DILoad`,
//! `DIUnload`, `DIFormat`, `DIVerify`, and `DIZero`.

#![allow(dead_code)]

/// `_Pack2` selector for `DIBadMount`: displays the bad-disk dialog box.
/// Inside Macintosh: Files (1992), p. 5-18.
pub const DI_BAD_MOUNT: u16 = 0x0000;

/// `_Pack2` selector for `DILoad`: reads the Disk Initialization Package into memory.
/// Inside Macintosh: Files (1992), p. 5-15.
pub const DI_LOAD: u16 = 0x0002;

/// `_Pack2` selector for `DIUnload`: releases the Disk Initialization Package from memory.
/// Inside Macintosh: Files (1992), p. 5-16.
pub const DI_UNLOAD: u16 = 0x0004;

/// `_Pack2` selector for `DIFormat`: formats the disk in the specified drive.
/// Inside Macintosh: Files (1992), p. 5-19.
pub const DI_FORMAT: u16 = 0x0006;

/// `_Pack2` selector for `DIVerify`: verifies the formatting of the disk in the specified drive.
/// Inside Macintosh: Files (1992), p. 5-20.
pub const DI_VERIFY: u16 = 0x0008;

/// `_Pack2` selector for `DIZero`: writes volume structures (directory, volume information) to disk.
/// Inside Macintosh: Files (1992), p. 5-21.
pub const DI_ZERO: u16 = 0x000A;

/// Standard success result code (`noErr = 0`).
pub const NO_ERR: i16 = 0;

/// Result returned by `DIBadMount` when the user successfully proceeded with initialization or ejection.
pub const DI_USER_PROCEEDED: i16 = 0;

/// Result returned by `DIBadMount` when the user canceled initialization.
pub const DI_USER_CANCEL: i16 = 1;

/// Stack argument bytes consumed by `DIBadMount` on 68k (`where: Point` = 4 bytes, `evtMessage: LongInt` = 4 bytes).
pub const DI_BAD_MOUNT_ARG_BYTES: u32 = 8;

/// Stack argument bytes consumed by `DILoad` on 68k (no arguments).
pub const DI_LOAD_ARG_BYTES: u32 = 0;

/// Stack argument bytes consumed by `DIUnload` on 68k (no arguments).
pub const DI_UNLOAD_ARG_BYTES: u32 = 0;

/// Stack argument bytes consumed by `DIFormat` on 68k (`drvNum: Integer` = 2 bytes).
pub const DI_FORMAT_ARG_BYTES: u32 = 2;

/// Stack argument bytes consumed by `DIVerify` on 68k (`drvNum: Integer` = 2 bytes).
pub const DI_VERIFY_ARG_BYTES: u32 = 2;

/// Stack argument bytes consumed by `DIZero` on 68k (`drvNum: Integer` = 2 bytes, `volName: Str255` = 256 bytes).
pub const DI_ZERO_ARG_BYTES: u32 = 258;

/// CPU architecture dispatching the Disk Initialization Manager operation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiskInitArchitecture {
    M68k,
    PowerPc,
}

/// Enumeration of Disk Initialization Manager operations dispatched by `_Pack2` or CFM imports.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiskInitOperation {
    /// FUNCTION DIBadMount(where: Point; evtMessage: LongInt): Integer
    BadMount,
    /// PROCEDURE DILoad
    Load,
    /// PROCEDURE DIUnload
    Unload,
    /// FUNCTION DIFormat(drvNum: Integer): OSErr
    Format,
    /// FUNCTION DIVerify(drvNum: Integer): OSErr
    Verify,
    /// FUNCTION DIZero(drvNum: Integer; volName: Str255): OSErr
    Zero,
}

impl DiskInitOperation {
    /// Decodes a 16-bit selector into a documented `DiskInitOperation`.
    pub fn from_selector(selector: u16) -> Option<Self> {
        match selector {
            DI_BAD_MOUNT => Some(Self::BadMount),
            DI_LOAD => Some(Self::Load),
            DI_UNLOAD => Some(Self::Unload),
            DI_FORMAT => Some(Self::Format),
            DI_VERIFY => Some(Self::Verify),
            DI_ZERO => Some(Self::Zero),
            _ => None,
        }
    }

    /// Returns the canonical 16-bit selector for this operation.
    pub fn selector(self) -> u16 {
        match self {
            Self::BadMount => DI_BAD_MOUNT,
            Self::Load => DI_LOAD,
            Self::Unload => DI_UNLOAD,
            Self::Format => DI_FORMAT,
            Self::Verify => DI_VERIFY,
            Self::Zero => DI_ZERO,
        }
    }

    /// Returns the number of parameter bytes on the 68k Pascal stack for this operation.
    pub fn stack_argument_bytes_68k(self) -> u32 {
        match self {
            Self::BadMount => DI_BAD_MOUNT_ARG_BYTES,
            Self::Load => DI_LOAD_ARG_BYTES,
            Self::Unload => DI_UNLOAD_ARG_BYTES,
            Self::Format => DI_FORMAT_ARG_BYTES,
            Self::Verify => DI_VERIFY_ARG_BYTES,
            Self::Zero => DI_ZERO_ARG_BYTES,
        }
    }

    /// Returns `true` if this operation is a Pascal FUNCTION returning a 16-bit result word.
    pub fn has_result_word_68k(self) -> bool {
        match self {
            Self::BadMount | Self::Format | Self::Verify | Self::Zero => true,
            Self::Load | Self::Unload => false,
        }
    }
}

/// Evaluation result for a 68k `_Pack2` trap invocation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DiskInitEvaluation {
    /// Total bytes to pop from the stack (including the 2-byte selector word and parameter bytes).
    pub pop_bytes_68k: u32,
    /// Result code to write to the caller's Pascal result slot, if the operation returns a result word.
    pub result_word: Option<i16>,
}

/// Evaluates a 68k `_Pack2` selector invocation.
///
/// Returns `Some(DiskInitEvaluation)` if the selector is recognized, or `None` if undocumented.
pub fn evaluate_pack2(selector: u16) -> Option<DiskInitEvaluation> {
    let op = DiskInitOperation::from_selector(selector)?;
    let pop_bytes_68k = 2 + op.stack_argument_bytes_68k();
    let result_word = if op.has_result_word_68k() {
        Some(NO_ERR)
    } else {
        None
    };
    Some(DiskInitEvaluation {
        pop_bytes_68k,
        result_word,
    })
}

/// Evaluates `DIBadMount`.
///
/// Returns 0 (`DI_USER_PROCEEDED`), indicating that no error occurred and the caller should not
/// escalate to system error handling.
pub fn evaluate_di_bad_mount(_where_v: i16, _where_h: i16, _evt_message: u32) -> i16 {
    DI_USER_PROCEEDED
}

/// Evaluates `DIFormat`.
///
/// In Systemless HLE, the single VFS volume is always formatted and returns `noErr` (0).
pub fn evaluate_di_format(_drv_num: i16) -> i16 {
    NO_ERR
}

/// Evaluates `DIVerify`.
///
/// In Systemless HLE, verification always succeeds and returns `noErr` (0).
pub fn evaluate_di_verify(_drv_num: i16) -> i16 {
    NO_ERR
}

/// Evaluates `DIZero`.
///
/// In Systemless HLE, zeroing/re-initialization returns `noErr` (0).
pub fn evaluate_di_zero(_drv_num: i16, _vol_name: &[u8]) -> i16 {
    NO_ERR
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn selector_decoding_and_roundtrip() {
        let all_ops = [
            (DI_BAD_MOUNT, DiskInitOperation::BadMount, 8, true),
            (DI_LOAD, DiskInitOperation::Load, 0, false),
            (DI_UNLOAD, DiskInitOperation::Unload, 0, false),
            (DI_FORMAT, DiskInitOperation::Format, 2, true),
            (DI_VERIFY, DiskInitOperation::Verify, 2, true),
            (DI_ZERO, DiskInitOperation::Zero, 258, true),
        ];

        for (sel, op, arg_bytes, has_result) in all_ops {
            assert_eq!(DiskInitOperation::from_selector(sel), Some(op));
            assert_eq!(op.selector(), sel);
            assert_eq!(op.stack_argument_bytes_68k(), arg_bytes);
            assert_eq!(op.has_result_word_68k(), has_result);
        }

        assert_eq!(DiskInitOperation::from_selector(0x000C), None);
        assert_eq!(DiskInitOperation::from_selector(0xFFFF), None);
    }

    #[test]
    fn pack2_evaluation() {
        // DIBadMount: 2 selector + 8 args = 10 pop, result Some(0)
        let eval = evaluate_pack2(DI_BAD_MOUNT).unwrap();
        assert_eq!(eval.pop_bytes_68k, 10);
        assert_eq!(eval.result_word, Some(0));

        // DILoad: 2 selector + 0 args = 2 pop, result None
        let eval = evaluate_pack2(DI_LOAD).unwrap();
        assert_eq!(eval.pop_bytes_68k, 2);
        assert_eq!(eval.result_word, None);

        // DIUnload: 2 selector + 0 args = 2 pop, result None
        let eval = evaluate_pack2(DI_UNLOAD).unwrap();
        assert_eq!(eval.pop_bytes_68k, 2);
        assert_eq!(eval.result_word, None);

        // DIFormat: 2 selector + 2 args = 4 pop, result Some(0)
        let eval = evaluate_pack2(DI_FORMAT).unwrap();
        assert_eq!(eval.pop_bytes_68k, 4);
        assert_eq!(eval.result_word, Some(0));

        // DIVerify: 2 selector + 2 args = 4 pop, result Some(0)
        let eval = evaluate_pack2(DI_VERIFY).unwrap();
        assert_eq!(eval.pop_bytes_68k, 4);
        assert_eq!(eval.result_word, Some(0));

        // DIZero: 2 selector + 258 args = 260 pop, result Some(0)
        let eval = evaluate_pack2(DI_ZERO).unwrap();
        assert_eq!(eval.pop_bytes_68k, 260);
        assert_eq!(eval.result_word, Some(0));

        // Undocumented selector returns None
        assert_eq!(evaluate_pack2(0x1234), None);
    }

    #[test]
    fn disk_init_evaluation_helpers() {
        assert_eq!(evaluate_di_bad_mount(10, 20, 0x1234), 0);
        assert_eq!(evaluate_di_format(1), 0);
        assert_eq!(evaluate_di_verify(1), 0);
        assert_eq!(evaluate_di_zero(1, b"DiskName"), 0);
    }
}
