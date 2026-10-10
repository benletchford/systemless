//! Architecture-neutral Printing Manager operations and parameter evaluation.
//!
//! Inside Macintosh Volume II (1985), chapter 5 "The Printing Manager", pp. II-147--II-168;
//! Inside Macintosh Volume V (1986), chapter 22 "The Printing Manager", pp. V-407--V-414;
//! Inside Macintosh: Imaging With QuickDraw (1994), chapter 9 "Printing Manager", pp. 9-3--9-96.
//!
//! The Printing Manager coordinates printer drivers, page imaging, print records (`TPrint`),
//! style and job dialogs, and error reporting across classic Macintosh software.
//!
//! On 68k, Printing Manager routines are dispatched via trap `_PrGlue` ($A8FD), which takes a
//! 32-bit selector on the stack encoding the routine index in bits 31-24 and the parameter byte
//! count in bits 15-8. On PowerPC, InterfaceLib exports direct CFM compatibility symbols
//! (`PrOpen`, `PrClose`, `PrintDefault`, `PrValidate`, `PrJobDialog`, `PrStlDialog`, etc.).

/// Standard success result code (`noErr = 0`).
#[allow(dead_code)]
pub const PR_NO_ERR: i16 = 0;

/// Printing Manager error: operation not implemented by printer driver (`opNotImpl = 2`).
/// Inside Macintosh: Imaging With QuickDraw (1994), p. 9-42.
pub const PR_OP_NOT_IMPL: i16 = 2;

// --- Routine byte identifiers (bits 31-24 of _PrGlue selectors) ---

pub const PR_ROUTINE_OPEN_DOC: u8 = 0x04;
pub const PR_ROUTINE_CLOSE_DOC: u8 = 0x08;
pub const PR_ROUTINE_OPEN_PAGE: u8 = 0x10;
pub const PR_ROUTINE_CLOSE_PAGE: u8 = 0x18;
pub const PR_ROUTINE_PRINT_DEFAULT: u8 = 0x20;
pub const PR_ROUTINE_STL_DIALOG: u8 = 0x2A;
pub const PR_ROUTINE_JOB_DIALOG: u8 = 0x32;
pub const PR_ROUTINE_STL_INIT: u8 = 0x3C;
pub const PR_ROUTINE_JOB_INIT: u8 = 0x44;
pub const PR_ROUTINE_DLG_MAIN: u8 = 0x4A;
pub const PR_ROUTINE_VALIDATE: u8 = 0x52;
pub const PR_ROUTINE_JOB_MERGE: u8 = 0x58;
pub const PR_ROUTINE_PIC_FILE: u8 = 0x60;
pub const PR_ROUTINE_GENERAL: u8 = 0x70;
pub const PR_ROUTINE_DRVR_OPEN: u8 = 0x80;
pub const PR_ROUTINE_DRVR_CLOSE: u8 = 0x88;
pub const PR_ROUTINE_DRVR_DCE: u8 = 0x94;
pub const PR_ROUTINE_DRVR_VERS: u8 = 0x9A;
pub const PR_ROUTINE_CTL_CALL: u8 = 0xA0;
pub const PR_ROUTINE_ERROR: u8 = 0xBA;
pub const PR_ROUTINE_SET_ERROR: u8 = 0xC0;
pub const PR_ROUTINE_OPEN: u8 = 0xC8;
pub const PR_ROUTINE_CLOSE: u8 = 0xD0;

// --- Canonical 32-bit immediate selectors used by MPW and classic shims ---

#[allow(dead_code)]
pub const PR_OPEN_DOC_SELECTOR: u32 = 0x0400_0C00;
#[allow(dead_code)]
pub const PR_CLOSE_DOC_SELECTOR: u32 = 0x0800_0484;
#[allow(dead_code)]
pub const PR_OPEN_PAGE_SELECTOR: u32 = 0x1000_0808;
#[allow(dead_code)]
pub const PR_CLOSE_PAGE_SELECTOR: u32 = 0x1800_040C;
#[allow(dead_code)]
pub const PR_PRINT_DEFAULT_SELECTOR: u32 = 0x2004_0480;
#[allow(dead_code)]
pub const PR_STL_DIALOG_SELECTOR: u32 = 0x2A04_0484;
#[allow(dead_code)]
pub const PR_JOB_DIALOG_SELECTOR: u32 = 0x3204_0488;
#[allow(dead_code)]
pub const PR_STL_INIT_SELECTOR: u32 = 0x3C04_040C;
#[allow(dead_code)]
pub const PR_JOB_INIT_SELECTOR: u32 = 0x4404_0410;
#[allow(dead_code)]
pub const PR_DLG_MAIN_SELECTOR: u32 = 0x4A04_0894;
#[allow(dead_code)]
pub const PR_VALIDATE_SELECTOR: u32 = 0x5204_0498;
#[allow(dead_code)]
pub const PR_JOB_MERGE_SELECTOR: u32 = 0x5804_089C;
#[allow(dead_code)]
pub const PR_PIC_FILE_SELECTOR: u32 = 0x6005_1480;
#[allow(dead_code)]
pub const PR_GENERAL_SELECTOR: u32 = 0x7007_0480;
#[allow(dead_code)]
pub const PR_DRVR_OPEN_SELECTOR: u32 = 0x8000_0000;
#[allow(dead_code)]
pub const PR_DRVR_CLOSE_SELECTOR: u32 = 0x8800_0000;
#[allow(dead_code)]
pub const PR_DRVR_DCE_SELECTOR: u32 = 0x9400_0000;
#[allow(dead_code)]
pub const PR_DRVR_VERS_SELECTOR: u32 = 0x9A00_0000;
#[allow(dead_code)]
pub const PR_CTL_CALL_SELECTOR: u32 = 0xA000_0E00;
#[allow(dead_code)]
pub const PR_ERROR_SELECTOR: u32 = 0xBA00_0000;
#[allow(dead_code)]
pub const PR_SET_ERROR_SELECTOR: u32 = 0xC000_0200;
#[allow(dead_code)]
pub const PR_OPEN_SELECTOR: u32 = 0xC800_0000;
#[allow(dead_code)]
pub const PR_CLOSE_SELECTOR: u32 = 0xD000_0000;

/// Execution architecture for Printing Manager evaluation.
#[allow(dead_code)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PrintingArchitecture {
    M68k,
    PowerPc,
}

/// Strongly-typed Printing Manager operation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PrintingOperation {
    OpenDoc,
    CloseDoc,
    OpenPage,
    ClosePage,
    PrintDefault,
    StlDialog,
    JobDialog,
    StlInit,
    JobInit,
    DlgMain,
    Validate,
    JobMerge,
    PicFile,
    General,
    DrvrOpen,
    DrvrClose,
    DrvrDce,
    DrvrVers,
    CtlCall,
    Error,
    SetError,
    Open,
    Close,
}

impl PrintingOperation {
    /// Maps a routine byte (bits 31-24 of `_PrGlue` selector) to a known operation.
    #[inline]
    pub const fn from_routine_byte(routine: u8) -> Option<Self> {
        match routine {
            PR_ROUTINE_OPEN_DOC => Some(Self::OpenDoc),
            PR_ROUTINE_CLOSE_DOC => Some(Self::CloseDoc),
            PR_ROUTINE_OPEN_PAGE => Some(Self::OpenPage),
            PR_ROUTINE_CLOSE_PAGE => Some(Self::ClosePage),
            PR_ROUTINE_PRINT_DEFAULT => Some(Self::PrintDefault),
            PR_ROUTINE_STL_DIALOG => Some(Self::StlDialog),
            PR_ROUTINE_JOB_DIALOG => Some(Self::JobDialog),
            PR_ROUTINE_STL_INIT => Some(Self::StlInit),
            PR_ROUTINE_JOB_INIT => Some(Self::JobInit),
            PR_ROUTINE_DLG_MAIN => Some(Self::DlgMain),
            PR_ROUTINE_VALIDATE => Some(Self::Validate),
            PR_ROUTINE_JOB_MERGE => Some(Self::JobMerge),
            PR_ROUTINE_PIC_FILE => Some(Self::PicFile),
            PR_ROUTINE_GENERAL => Some(Self::General),
            PR_ROUTINE_DRVR_OPEN => Some(Self::DrvrOpen),
            PR_ROUTINE_DRVR_CLOSE => Some(Self::DrvrClose),
            PR_ROUTINE_DRVR_DCE => Some(Self::DrvrDce),
            PR_ROUTINE_DRVR_VERS => Some(Self::DrvrVers),
            PR_ROUTINE_CTL_CALL => Some(Self::CtlCall),
            PR_ROUTINE_ERROR => Some(Self::Error),
            PR_ROUTINE_SET_ERROR => Some(Self::SetError),
            PR_ROUTINE_OPEN => Some(Self::Open),
            PR_ROUTINE_CLOSE => Some(Self::Close),
            _ => None,
        }
    }

    /// Returns the standard routine byte corresponding to this operation.
    #[allow(dead_code)]
    #[inline]
    pub const fn routine_byte(self) -> u8 {
        match self {
            Self::OpenDoc => PR_ROUTINE_OPEN_DOC,
            Self::CloseDoc => PR_ROUTINE_CLOSE_DOC,
            Self::OpenPage => PR_ROUTINE_OPEN_PAGE,
            Self::ClosePage => PR_ROUTINE_CLOSE_PAGE,
            Self::PrintDefault => PR_ROUTINE_PRINT_DEFAULT,
            Self::StlDialog => PR_ROUTINE_STL_DIALOG,
            Self::JobDialog => PR_ROUTINE_JOB_DIALOG,
            Self::StlInit => PR_ROUTINE_STL_INIT,
            Self::JobInit => PR_ROUTINE_JOB_INIT,
            Self::DlgMain => PR_ROUTINE_DLG_MAIN,
            Self::Validate => PR_ROUTINE_VALIDATE,
            Self::JobMerge => PR_ROUTINE_JOB_MERGE,
            Self::PicFile => PR_ROUTINE_PIC_FILE,
            Self::General => PR_ROUTINE_GENERAL,
            Self::DrvrOpen => PR_ROUTINE_DRVR_OPEN,
            Self::DrvrClose => PR_ROUTINE_DRVR_CLOSE,
            Self::DrvrDce => PR_ROUTINE_DRVR_DCE,
            Self::DrvrVers => PR_ROUTINE_DRVR_VERS,
            Self::CtlCall => PR_ROUTINE_CTL_CALL,
            Self::Error => PR_ROUTINE_ERROR,
            Self::SetError => PR_ROUTINE_SET_ERROR,
            Self::Open => PR_ROUTINE_OPEN,
            Self::Close => PR_ROUTINE_CLOSE,
        }
    }

    /// Returns the canonical routine name string.
    #[allow(dead_code)]
    #[inline]
    pub const fn name(self) -> &'static str {
        match self {
            Self::OpenDoc => "PrOpenDoc",
            Self::CloseDoc => "PrCloseDoc",
            Self::OpenPage => "PrOpenPage",
            Self::ClosePage => "PrClosePage",
            Self::PrintDefault => "PrintDefault",
            Self::StlDialog => "PrStlDialog",
            Self::JobDialog => "PrJobDialog",
            Self::StlInit => "PrStlInit",
            Self::JobInit => "PrJobInit",
            Self::DlgMain => "PrDlgMain",
            Self::Validate => "PrValidate",
            Self::JobMerge => "PrJobMerge",
            Self::PicFile => "PrPicFile",
            Self::General => "PrGeneral",
            Self::DrvrOpen => "PrDrvrOpen",
            Self::DrvrClose => "PrDrvrClose",
            Self::DrvrDce => "PrDrvrDCE",
            Self::DrvrVers => "PrDrvrVers",
            Self::CtlCall => "PrCtlCall",
            Self::Error => "PrError",
            Self::SetError => "PrSetError",
            Self::Open => "PrOpen",
            Self::Close => "PrClose",
        }
    }
}

/// Decoded components of a `_PrGlue` selector long.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PrGlueSelectorDecode {
    pub routine: u8,
    pub param_bytes: u32,
    pub total_pop: u32,
    pub operation: Option<PrintingOperation>,
}

/// Decodes a 32-bit `_PrGlue` selector into routine ID, parameter bytes, total pop count, and operation.
#[inline]
pub fn decode_pr_glue_selector(selector: u32) -> PrGlueSelectorDecode {
    let routine = ((selector >> 24) & 0xFF) as u8;
    let param_bytes = (selector >> 8) & 0xFF;
    let total_pop = 4 + param_bytes;
    let operation = PrintingOperation::from_routine_byte(routine);
    PrGlueSelectorDecode {
        routine,
        param_bytes,
        total_pop,
        operation,
    }
}

/// Stack result return specification for `_PrGlue`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PrGlueStackResult {
    /// Routine does not push a function result onto the stack.
    None,
    /// Routine writes a 16-bit word result at `SP + total_pop`.
    Word(u16),
    /// Routine writes a 32-bit long result at `SP + total_pop`.
    Long(u32),
}

/// Register D0 update specification for `_PrGlue`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PrGlueRegisterResult {
    /// Preserves existing D0 contents.
    Preserve,
    /// Writes specified value to register D0.
    Set(u32),
}

/// Mutation action on shared Printing Manager error state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PrGlueErrorAction {
    /// Leaves existing printing error unchanged.
    Preserve,
    /// Resets printing error to `PR_NO_ERR` (0).
    Reset,
    /// Updates printing error to the specified value.
    Set(i16),
}

/// Evaluated outcome of a 68k `_PrGlue` dispatch invocation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PrGlueEvaluation {
    pub routine: u8,
    pub param_bytes: u32,
    pub total_pop: u32,
    pub operation: Option<PrintingOperation>,
    pub stack_result: PrGlueStackResult,
    pub register_result: PrGlueRegisterResult,
    pub error_action: PrGlueErrorAction,
}

/// Evaluates a 68k `_PrGlue` invocation for the given selector, current error, and optional set-error argument.
#[inline]
pub fn evaluate_pr_glue(
    selector: u32,
    current_printing_error: i16,
    set_error_arg: i16,
) -> PrGlueEvaluation {
    let decoded = decode_pr_glue_selector(selector);

    let (stack_result, register_result, error_action) = match decoded.routine {
        PR_ROUTINE_OPEN_DOC => (
            PrGlueStackResult::Long(0),
            PrGlueRegisterResult::Set(0),
            PrGlueErrorAction::Reset,
        ),
        PR_ROUTINE_CLOSE_DOC | PR_ROUTINE_OPEN_PAGE | PR_ROUTINE_CLOSE_PAGE => (
            PrGlueStackResult::None,
            PrGlueRegisterResult::Preserve,
            PrGlueErrorAction::Reset,
        ),
        PR_ROUTINE_PRINT_DEFAULT => (
            PrGlueStackResult::None,
            PrGlueRegisterResult::Set(0),
            PrGlueErrorAction::Reset,
        ),
        PR_ROUTINE_OPEN | PR_ROUTINE_CLOSE => (
            PrGlueStackResult::None,
            PrGlueRegisterResult::Preserve,
            PrGlueErrorAction::Preserve,
        ),
        PR_ROUTINE_STL_DIALOG | PR_ROUTINE_JOB_DIALOG => (
            PrGlueStackResult::Word(1),
            PrGlueRegisterResult::Set(1),
            PrGlueErrorAction::Reset,
        ),
        PR_ROUTINE_STL_INIT | PR_ROUTINE_JOB_INIT => (
            PrGlueStackResult::Long(0),
            PrGlueRegisterResult::Set(0),
            PrGlueErrorAction::Reset,
        ),
        PR_ROUTINE_DLG_MAIN => (
            PrGlueStackResult::Word(1),
            PrGlueRegisterResult::Set(1),
            PrGlueErrorAction::Reset,
        ),
        PR_ROUTINE_VALIDATE => (
            PrGlueStackResult::Word(0),
            PrGlueRegisterResult::Set(0),
            PrGlueErrorAction::Reset,
        ),
        PR_ROUTINE_JOB_MERGE
        | PR_ROUTINE_PIC_FILE
        | PR_ROUTINE_GENERAL
        | PR_ROUTINE_DRVR_OPEN
        | PR_ROUTINE_DRVR_CLOSE
        | PR_ROUTINE_CTL_CALL => (
            PrGlueStackResult::None,
            PrGlueRegisterResult::Set(0),
            PrGlueErrorAction::Reset,
        ),
        PR_ROUTINE_DRVR_DCE => (
            PrGlueStackResult::Long(0),
            PrGlueRegisterResult::Set(0),
            PrGlueErrorAction::Reset,
        ),
        PR_ROUTINE_DRVR_VERS => (
            PrGlueStackResult::Word(0),
            PrGlueRegisterResult::Set(0),
            PrGlueErrorAction::Reset,
        ),
        PR_ROUTINE_ERROR => (
            PrGlueStackResult::Word(current_printing_error as u16),
            PrGlueRegisterResult::Set(current_printing_error as u32),
            PrGlueErrorAction::Preserve,
        ),
        PR_ROUTINE_SET_ERROR => (
            PrGlueStackResult::None,
            PrGlueRegisterResult::Set(0),
            PrGlueErrorAction::Set(set_error_arg),
        ),
        _ => (
            PrGlueStackResult::None,
            PrGlueRegisterResult::Set(0),
            PrGlueErrorAction::Reset,
        ),
    };

    PrGlueEvaluation {
        routine: decoded.routine,
        param_bytes: decoded.param_bytes,
        total_pop: decoded.total_pop,
        operation: decoded.operation,
        stack_result,
        register_result,
        error_action,
    }
}

/// Evaluates `PrSetError`: updates the Printing Manager error state.
///
/// `PROCEDURE PrSetError(iErr: Integer);`
/// Inside Macintosh: Imaging With QuickDraw (1994), p. 9-78.
#[inline]
pub fn evaluate_pr_set_error(input_err: i16) -> i16 {
    input_err
}

/// Evaluates `PrError`: returns the active Printing Manager error code.
///
/// `FUNCTION PrError: Integer;`
/// Inside Macintosh: Imaging With QuickDraw (1994), p. 9-75.
#[inline]
pub fn evaluate_pr_error(current_err: i16) -> i16 {
    current_err
}

/// Evaluates `PrValidate`: returns `false` (record is valid, no driver-specific adjustments needed).
///
/// `FUNCTION PrValidate(hPrint: THPrint): Boolean;`
/// Inside Macintosh: Imaging With QuickDraw (1994), p. 9-60.
#[inline]
pub fn evaluate_pr_validate() -> bool {
    false
}

/// Evaluates `PrintDefault`: initializes print record defaults (returns 0 / noErr).
///
/// `PROCEDURE PrintDefault(hPrint: THPrint);`
/// Inside Macintosh: Imaging With QuickDraw (1994), p. 9-59.
#[inline]
pub const fn evaluate_print_default() -> u32 {
    0
}

/// Evaluates `PrOpenDoc`: returns NIL (0) port pointer since native printing is unattached.
///
/// `FUNCTION PrOpenDoc(hPrint: THPrint; pPrPort: TPPrPort; pIOBuf: Ptr): TPPrPort;`
/// Inside Macintosh: Imaging With QuickDraw (1994), p. 9-65.
#[inline]
pub const fn evaluate_pr_open_doc() -> u32 {
    0
}

/// Evaluates `PrStlDialog` or `PrJobDialog` confirmation based on architecture profile.
///
/// On 68k, games often require `true` (user confirmed OK) to proceed past print screens.
/// On PowerPC CFM compatibility, standard shims report `false` (0) indicating no printer driver dialog was shown.
#[inline]
pub fn evaluate_pr_dialog(arch: PrintingArchitecture) -> bool {
    match arch {
        PrintingArchitecture::M68k => true,
        PrintingArchitecture::PowerPc => false,
    }
}

/// Evaluates `PrGeneral` operation: reports `opNotImpl` (2) if caller data buffer is writable.
///
/// `PROCEDURE PrGeneral(pData: Ptr);`
/// Inside Macintosh: Imaging With QuickDraw (1994), pp. 9-72--9-73.
#[inline]
pub fn evaluate_pr_general(memory_writable: bool) -> Option<i16> {
    if memory_writable {
        Some(PR_OP_NOT_IMPL)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pr_glue_selector_decoding() {
        let open_doc = decode_pr_glue_selector(PR_OPEN_DOC_SELECTOR);
        assert_eq!(open_doc.routine, PR_ROUTINE_OPEN_DOC);
        assert_eq!(open_doc.param_bytes, 12);
        assert_eq!(open_doc.total_pop, 16);
        assert_eq!(open_doc.operation, Some(PrintingOperation::OpenDoc));

        let print_default = decode_pr_glue_selector(PR_PRINT_DEFAULT_SELECTOR);
        assert_eq!(print_default.routine, PR_ROUTINE_PRINT_DEFAULT);
        assert_eq!(print_default.param_bytes, 4);
        assert_eq!(print_default.total_pop, 8);
        assert_eq!(
            print_default.operation,
            Some(PrintingOperation::PrintDefault)
        );

        let open = decode_pr_glue_selector(PR_OPEN_SELECTOR);
        assert_eq!(open.routine, PR_ROUTINE_OPEN);
        assert_eq!(open.param_bytes, 0);
        assert_eq!(open.total_pop, 4);
        assert_eq!(open.operation, Some(PrintingOperation::Open));

        let set_error = decode_pr_glue_selector(PR_SET_ERROR_SELECTOR);
        assert_eq!(set_error.routine, PR_ROUTINE_SET_ERROR);
        assert_eq!(set_error.param_bytes, 2);
        assert_eq!(set_error.total_pop, 6);
        assert_eq!(set_error.operation, Some(PrintingOperation::SetError));

        let custom = decode_pr_glue_selector(0xF100_0600);
        assert_eq!(custom.routine, 0xF1);
        assert_eq!(custom.param_bytes, 6);
        assert_eq!(custom.total_pop, 10);
        assert_eq!(custom.operation, None);
    }

    #[test]
    fn pr_glue_evaluation_stack_and_register_contracts() {
        // PrOpenDoc returns Long(0) in result slot and D0, resets error
        let eval_open_doc = evaluate_pr_glue(PR_OPEN_DOC_SELECTOR, -128, 0);
        assert_eq!(eval_open_doc.stack_result, PrGlueStackResult::Long(0));
        assert_eq!(eval_open_doc.register_result, PrGlueRegisterResult::Set(0));
        assert_eq!(eval_open_doc.error_action, PrGlueErrorAction::Reset);

        // PrCloseDoc returns None on stack, preserves D0, resets error
        let eval_close_doc = evaluate_pr_glue(PR_CLOSE_DOC_SELECTOR, -128, 0);
        assert_eq!(eval_close_doc.stack_result, PrGlueStackResult::None);
        assert_eq!(
            eval_close_doc.register_result,
            PrGlueRegisterResult::Preserve
        );
        assert_eq!(eval_close_doc.error_action, PrGlueErrorAction::Reset);

        // PrOpen and PrClose preserve error state and D0
        let eval_open = evaluate_pr_glue(PR_OPEN_SELECTOR, 42, 0);
        assert_eq!(eval_open.stack_result, PrGlueStackResult::None);
        assert_eq!(eval_open.register_result, PrGlueRegisterResult::Preserve);
        assert_eq!(eval_open.error_action, PrGlueErrorAction::Preserve);

        let eval_close = evaluate_pr_glue(PR_CLOSE_SELECTOR, 42, 0);
        assert_eq!(eval_close.stack_result, PrGlueStackResult::None);
        assert_eq!(eval_close.register_result, PrGlueRegisterResult::Preserve);
        assert_eq!(eval_close.error_action, PrGlueErrorAction::Preserve);

        // PrStlDialog and PrJobDialog return Word(1) and D0=1
        let eval_stl_dlg = evaluate_pr_glue(PR_STL_DIALOG_SELECTOR, 0, 0);
        assert_eq!(eval_stl_dlg.stack_result, PrGlueStackResult::Word(1));
        assert_eq!(eval_stl_dlg.register_result, PrGlueRegisterResult::Set(1));
        assert_eq!(eval_stl_dlg.error_action, PrGlueErrorAction::Reset);

        let eval_job_dlg = evaluate_pr_glue(PR_JOB_DIALOG_SELECTOR, 0, 0);
        assert_eq!(eval_job_dlg.stack_result, PrGlueStackResult::Word(1));
        assert_eq!(eval_job_dlg.register_result, PrGlueRegisterResult::Set(1));
        assert_eq!(eval_job_dlg.error_action, PrGlueErrorAction::Reset);

        // PrValidate returns Word(0) and D0=0
        let eval_validate = evaluate_pr_glue(PR_VALIDATE_SELECTOR, 0, 0);
        assert_eq!(eval_validate.stack_result, PrGlueStackResult::Word(0));
        assert_eq!(eval_validate.register_result, PrGlueRegisterResult::Set(0));
        assert_eq!(eval_validate.error_action, PrGlueErrorAction::Reset);

        // PrError returns Word(error) and D0=error, preserves error state
        let eval_error = evaluate_pr_glue(PR_ERROR_SELECTOR, -128, 0);
        assert_eq!(
            eval_error.stack_result,
            PrGlueStackResult::Word((-128i16) as u16)
        );
        assert_eq!(
            eval_error.register_result,
            PrGlueRegisterResult::Set((-128i16) as u32)
        );
        assert_eq!(eval_error.error_action, PrGlueErrorAction::Preserve);

        // PrSetError consumes argument and updates error
        let eval_set_error = evaluate_pr_glue(PR_SET_ERROR_SELECTOR, 0, 0x1357);
        assert_eq!(eval_set_error.stack_result, PrGlueStackResult::None);
        assert_eq!(eval_set_error.register_result, PrGlueRegisterResult::Set(0));
        assert_eq!(eval_set_error.error_action, PrGlueErrorAction::Set(0x1357));
    }

    #[test]
    fn printing_operation_enum_mapping_roundtrip() {
        for op in [
            PrintingOperation::OpenDoc,
            PrintingOperation::CloseDoc,
            PrintingOperation::OpenPage,
            PrintingOperation::ClosePage,
            PrintingOperation::PrintDefault,
            PrintingOperation::StlDialog,
            PrintingOperation::JobDialog,
            PrintingOperation::StlInit,
            PrintingOperation::JobInit,
            PrintingOperation::DlgMain,
            PrintingOperation::Validate,
            PrintingOperation::JobMerge,
            PrintingOperation::PicFile,
            PrintingOperation::General,
            PrintingOperation::DrvrOpen,
            PrintingOperation::DrvrClose,
            PrintingOperation::DrvrDce,
            PrintingOperation::DrvrVers,
            PrintingOperation::CtlCall,
            PrintingOperation::Error,
            PrintingOperation::SetError,
            PrintingOperation::Open,
            PrintingOperation::Close,
        ] {
            let byte = op.routine_byte();
            let mapped = PrintingOperation::from_routine_byte(byte);
            assert_eq!(mapped, Some(op));
            assert!(!op.name().is_empty());
        }
    }

    #[test]
    fn printing_manager_evaluation_helpers() {
        assert_eq!(evaluate_pr_error(42), 42);
        assert_eq!(evaluate_pr_set_error(-128), -128);
        assert!(!evaluate_pr_validate());
        assert_eq!(evaluate_print_default(), 0);
        assert_eq!(evaluate_pr_open_doc(), 0);

        assert!(evaluate_pr_dialog(PrintingArchitecture::M68k));
        assert!(!evaluate_pr_dialog(PrintingArchitecture::PowerPc));

        assert_eq!(evaluate_pr_general(true), Some(PR_OP_NOT_IMPL));
        assert_eq!(evaluate_pr_general(false), None);
    }
}
