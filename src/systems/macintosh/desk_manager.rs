//! Architecture-neutral Desk Manager operations and parameter evaluation.
//!
//! Inside Macintosh Volume I (1985), chapter 14 "The Desk Manager", pp. I-435--I-448;
//! Inside Macintosh: Devices (1994), chapter 1 "Device Manager", pp. 1-65--1-69;
//! Macintosh Toolbox Essentials (1992), chapter 6 "Desk Manager", pp. 2-94--2-95.
//!
//! Classic Mac OS Desk Accessories ran in system windows (`windowKind < 0`, equal to
//! `-refNum`) sharing the application address space.
//! In Systemless HLE, desk accessories are not modeled (`windowKind >= 0` always),
//! and periodic driver tasks are advanced by the runner rather than guest-side DRVR
//! queue traversal. The canonical evaluation functions below formalize the ABI contracts
//! across 68k traps ($A9B2, $A9B3, $A9B4, $A9B5, $A9B6, $A9B7, $A9C2) and PowerPC CFM imports.

/// Periodic action evaluation result for `SystemTask`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SystemTaskAction {
    /// No periodic desk accessory or driver work performed (HLE quiescent).
    Preserve,
}

/// Evaluates a `SystemTask` invocation.
///
/// Per IM:I I-442, `SystemTask` causes open desk accessories to perform periodic actions
/// defined for them. In Systemless HLE, there are no open accessories or periodic device
/// driver chains.
#[inline]
pub fn evaluate_system_task() -> SystemTaskAction {
    SystemTaskAction::Preserve
}

/// Evaluates parameters for `SystemClick`.
///
/// Per IM:I I-441: `PROCEDURE SystemClick(theEvent: EventRecord; theWindow: WindowPtr);`
/// When `FindWindow` reports `inSysWindow`, the application forwards mouse-down events
/// to the Desk Manager via `SystemClick`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SystemClickParameters {
    pub event_ptr: u32,
    pub window_ptr: u32,
}

impl SystemClickParameters {
    /// In Systemless HLE, all windows are application windows, so no desk accessory window
    /// can receive a click.
    #[allow(dead_code)]
    #[inline]
    pub const fn is_desk_accessory_window(&self) -> bool {
        false
    }
}

/// Evaluates canonical parameters for `SystemClick`.
#[inline]
pub fn evaluate_system_click_parameters(event_ptr: u32, window_ptr: u32) -> SystemClickParameters {
    SystemClickParameters {
        event_ptr,
        window_ptr,
    }
}

/// Evaluates parameters for `OpenDeskAcc`.
///
/// Per IM:I I-440: `FUNCTION OpenDeskAcc(theAcc: Str255): INTEGER;`
/// Opens the desk accessory having the given Pascal name.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OpenDeskAccParameters {
    pub name_ptr: u32,
}

impl OpenDeskAccParameters {
    /// Result driver reference number. Returns 0 indicating no accessory opened.
    ///
    /// Per IM:I I-440, if the desk accessory cannot be opened, the function result is undefined.
    /// Systemless uses 0 as the defensive sentinel.
    #[inline]
    pub const fn ref_num(&self) -> i16 {
        0
    }
}

/// Evaluates canonical parameters for `OpenDeskAcc`.
#[inline]
pub fn evaluate_open_desk_acc_parameters(name_ptr: u32) -> OpenDeskAccParameters {
    OpenDeskAccParameters { name_ptr }
}

/// Evaluates parameters for `CloseDeskAcc`.
///
/// Per IM:I I-440: `PROCEDURE CloseDeskAcc(refNum: INTEGER);`
/// Closes the desk accessory specified by its driver reference number.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CloseDeskAccParameters {
    pub ref_num: i16,
}

impl CloseDeskAccParameters {
    /// Desk accessory reference numbers are negative driver reference numbers (`refNum < 0`).
    #[allow(dead_code)]
    #[inline]
    pub const fn is_valid_desk_acc(&self) -> bool {
        self.ref_num < 0
    }
}

/// Evaluates canonical parameters for `CloseDeskAcc`.
#[inline]
pub fn evaluate_close_desk_acc_parameters(ref_num: i16) -> CloseDeskAccParameters {
    CloseDeskAccParameters { ref_num }
}

/// Standard editing command selectors for `SystemEdit`.
///
/// Per IM:I I-441 table of standard edit commands.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SystemEditCommand {
    Undo,
    Cut,
    Copy,
    Paste,
    Clear,
    Unknown(i16),
}

impl SystemEditCommand {
    pub const fn from_raw(raw: i16) -> Self {
        match raw {
            0 => Self::Undo,
            2 => Self::Cut,
            3 => Self::Copy,
            4 => Self::Paste,
            5 => Self::Clear,
            other => Self::Unknown(other),
        }
    }

    #[allow(dead_code)]
    pub const fn raw(&self) -> i16 {
        match *self {
            Self::Undo => 0,
            Self::Cut => 2,
            Self::Copy => 3,
            Self::Paste => 4,
            Self::Clear => 5,
            Self::Unknown(other) => other,
        }
    }
}

/// Evaluates parameters for `SystemEdit`.
///
/// Per IM:I I-441: `FUNCTION SystemEdit(editCmd: INTEGER): BOOLEAN;`
/// Called when the user chooses an editing command from the Edit menu.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SystemEditParameters {
    pub command: SystemEditCommand,
    pub raw_command: i16,
}

impl SystemEditParameters {
    /// Returns `true` if a desk accessory handled the edit command.
    ///
    /// Per IM:I I-441: "If the active window does not belong to a desk accessory ...
    /// SystemEdit returns FALSE so that your application will perform the editing function
    /// on its own document." In Systemless HLE, always returns `false`.
    #[inline]
    pub const fn handled(&self) -> bool {
        false
    }
}

/// Evaluates canonical parameters for `SystemEdit`.
#[inline]
pub fn evaluate_system_edit_parameters(edit_cmd: i16) -> SystemEditParameters {
    SystemEditParameters {
        command: SystemEditCommand::from_raw(edit_cmd),
        raw_command: edit_cmd,
    }
}

/// Evaluates parameters for `SystemEvent`.
///
/// Per IM:I I-442: `FUNCTION SystemEvent(theEvent: EventRecord): BOOLEAN;`
/// Lets the Desk Manager handle an event directed to a desk accessory.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SystemEventParameters {
    pub event_ptr: u32,
}

impl SystemEventParameters {
    /// Returns `true` if a desk accessory handled the event.
    ///
    /// Per IM:I I-441: "If the active window does not belong to a desk accessory ...
    /// SystemEvent returns FALSE". In Systemless HLE, always returns `false`.
    #[inline]
    pub const fn handled(&self) -> bool {
        false
    }
}

/// Evaluates canonical parameters for `SystemEvent`.
#[inline]
pub fn evaluate_system_event_parameters(event_ptr: u32) -> SystemEventParameters {
    SystemEventParameters { event_ptr }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn desk_manager_evaluation() {
        // SystemTask
        assert_eq!(evaluate_system_task(), SystemTaskAction::Preserve);

        // SystemClick
        let click = evaluate_system_click_parameters(0x2000, 0x3000);
        assert_eq!(click.event_ptr, 0x2000);
        assert_eq!(click.window_ptr, 0x3000);
        assert!(!click.is_desk_accessory_window());

        // OpenDeskAcc
        let open = evaluate_open_desk_acc_parameters(0x4000);
        assert_eq!(open.name_ptr, 0x4000);
        assert_eq!(open.ref_num(), 0);

        // CloseDeskAcc
        let close_invalid = evaluate_close_desk_acc_parameters(0);
        assert_eq!(close_invalid.ref_num, 0);
        assert!(!close_invalid.is_valid_desk_acc());

        let close_valid = evaluate_close_desk_acc_parameters(-12);
        assert_eq!(close_valid.ref_num, -12);
        assert!(close_valid.is_valid_desk_acc());

        // SystemEdit
        for (raw, expected_cmd) in [
            (0, SystemEditCommand::Undo),
            (2, SystemEditCommand::Cut),
            (3, SystemEditCommand::Copy),
            (4, SystemEditCommand::Paste),
            (5, SystemEditCommand::Clear),
            (1, SystemEditCommand::Unknown(1)),
            (99, SystemEditCommand::Unknown(99)),
        ] {
            let edit = evaluate_system_edit_parameters(raw);
            assert_eq!(edit.raw_command, raw);
            assert_eq!(edit.command, expected_cmd);
            assert_eq!(edit.command.raw(), raw);
            assert!(!edit.handled());
        }

        // SystemEvent
        let evt = evaluate_system_event_parameters(0x5000);
        assert_eq!(evt.event_ptr, 0x5000);
        assert!(!evt.handled());
    }
}
