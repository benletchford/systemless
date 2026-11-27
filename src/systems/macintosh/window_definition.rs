//! The window definition function (WDEF) protocol, shared by the 68K and
//! PowerPC Window Managers.
//!
//! A window's definition ID selects a 'WDEF' resource in its upper 12 bits and
//! a variation code in its lower four. Standard definitions are built in;
//! IDs from 128 up are application resources that the Window Manager calls:
//!
//! ```text
//! FUNCTION MyWindow (varCode: Integer; theWindow: WindowPtr;
//!                    message: Integer; param: LongInt): LongInt;
//! ```
//!
//! Macintosh Toolbox Essentials (1992), pp. 4-120 through 4-124.

use crate::guest_procedure::GuestProcedureMemory;

/// wDraw: draw the window frame in the Window Manager port (Toolbox
/// Essentials, p. 4-121).
pub(crate) const WDRAW: i16 = 0;
/// wCalcRgns: calculate the structure and content regions (p. 4-123).
pub(crate) const WCALC_RGNS: i16 = 2;
/// wNew: perform any additional initialization (p. 4-123).
pub(crate) const WNEW: i16 = 3;

/// The first window definition resource ID an application owns.
pub(crate) const FIRST_APPLICATION_RESOURCE_ID: i16 = 128;

/// `uppWindowDefProcInfo`: Pascal, four-byte result, then varCode (two
/// bytes), theWindow (four), message (two) and param (four) (Universal
/// Interfaces 3.x MacWindows.h).
pub(crate) const PROC_INFO: u32 = 0x3BB0;

/// The 'WDEF' resource ID a window definition ID selects.
pub(crate) fn resource_id(proc_id: i16) -> i16 {
    proc_id >> 4
}

/// The variation code a window definition receives as `varCode`.
pub(crate) fn variation(proc_id: i16) -> i16 {
    proc_id & 0xF
}

/// Whether the window definition is an application 'WDEF' resource, called
/// through the guest, rather than one of the built-in standard definitions.
pub(crate) fn is_application_definition(proc_id: i16) -> bool {
    resource_id(proc_id) >= FIRST_APPLICATION_RESOURCE_ID
}

/// When the Window Manager calls a window's definition function.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum WindowDefinitionEvent {
    /// NewWindow and its relatives: wNew, then, for a visible window whose
    /// frame is drawn now, wCalcRgns and wDraw.
    Create { draws_frame: bool },
    /// ShowWindow or ShowHide revealing a hidden window: its frame is drawn
    /// through its definition (boot3 WindowMgr.a ShowHide, DrawNew).
    Reveal,
}

/// The messages, with their `param` values, the Window Manager sends to a
/// window's definition function for `event`, in order.
pub(crate) fn messages(event: WindowDefinitionEvent) -> &'static [(i16, u32)] {
    match event {
        WindowDefinitionEvent::Create { draws_frame: true } => {
            &[(WNEW, 0), (WCALC_RGNS, 0), (WDRAW, 0)]
        }
        WindowDefinitionEvent::Create { draws_frame: false } => &[(WNEW, 0)],
        WindowDefinitionEvent::Reveal => &[(WCALC_RGNS, 0), (WDRAW, 0)],
    }
}

/// Classic GrafPort fields a color window lends to its definition function.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct LegacyPortBounds {
    /// The two longs at +8 and +12 while the definition runs: the port
    /// pixel map's `bounds`.
    pub(crate) lent: [u32; 2],
    /// The CGrafPort's own `grafVars` and `chExtra`/`pnLocHFrac`, which
    /// must be written back when the definition returns.
    pub(crate) saved: [u32; 2],
}

/// Pre-Color QuickDraw definitions receive a WindowPeek and read the
/// classic inline `portBits.bounds` at +8 through +15 to turn the port
/// rectangle into global coordinates. A CWindowRecord keeps `grafVars`,
/// `chExtra` and `pnLocHFrac` there instead, so a color window lends its
/// pixel map's bounds for the duration of the call, as System 7 does.
/// Returns `None` for a classic window, or a color window without a pixel
/// map, which need nothing lent.
pub(crate) fn legacy_port_bounds(
    memory: &mut impl GuestProcedureMemory,
    window: u32,
) -> Option<LegacyPortBounds> {
    if memory.procedure_read_u16(window.checked_add(6)?)? & 0xC000 == 0 {
        return None;
    }
    let pixmap_handle = memory.procedure_read_u32(window.checked_add(2)?)?;
    let pixmap = memory.procedure_read_u32(pixmap_handle)?;
    if pixmap == 0 {
        return None;
    }
    Some(LegacyPortBounds {
        lent: [
            memory.procedure_read_u32(pixmap.checked_add(6)?)?,
            memory.procedure_read_u32(pixmap.checked_add(10)?)?,
        ],
        saved: [
            memory.procedure_read_u32(window.checked_add(8)?)?,
            memory.procedure_read_u32(window.checked_add(12)?)?,
        ],
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn application_definitions_start_at_resource_128() {
        assert!(!is_application_definition(0));
        assert!(!is_application_definition(1999)); // floating-window WDEF 124
        assert!(!is_application_definition(2047));
        assert!(is_application_definition(2048)); // WDEF 128, variant 0
        assert!(is_application_definition(2061));
        assert!(!is_application_definition(-1));
        assert_eq!(resource_id(2061), 128);
        assert_eq!(variation(2061), 13);
    }

    #[test]
    fn creation_and_reveal_send_the_window_manager_messages_in_order() {
        assert_eq!(
            messages(WindowDefinitionEvent::Create { draws_frame: true }),
            &[(WNEW, 0), (WCALC_RGNS, 0), (WDRAW, 0)]
        );
        assert_eq!(
            messages(WindowDefinitionEvent::Create { draws_frame: false }),
            &[(WNEW, 0)]
        );
        assert_eq!(
            messages(WindowDefinitionEvent::Reveal),
            &[(WCALC_RGNS, 0), (WDRAW, 0)]
        );
    }

    struct Bytes(Vec<u8>);

    impl GuestProcedureMemory for Bytes {
        fn procedure_read_u8(&mut self, address: u32) -> Option<u8> {
            self.0.get(address as usize).copied()
        }
        fn procedure_read_u16(&mut self, address: u32) -> Option<u16> {
            let a = address as usize;
            Some(u16::from_be_bytes(self.0.get(a..a + 2)?.try_into().ok()?))
        }
        fn procedure_read_u32(&mut self, address: u32) -> Option<u32> {
            let a = address as usize;
            Some(u32::from_be_bytes(self.0.get(a..a + 4)?.try_into().ok()?))
        }
    }

    fn put(bytes: &mut [u8], at: usize, value: u32) {
        bytes[at..at + 4].copy_from_slice(&value.to_be_bytes());
    }

    #[test]
    fn color_windows_lend_their_pixel_map_bounds() {
        let mut bytes = vec![0; 0x100];
        let window = 0x10;
        put(&mut bytes, window + 2, 0x80); // portPixMap handle
        bytes[window + 6] = 0xC0; // portVersion
        put(&mut bytes, window + 8, 0x1111_2222); // grafVars
        put(&mut bytes, window + 12, 0x3333_4444); // chExtra, pnLocHFrac
        put(&mut bytes, 0x80, 0x90); // master pointer
        put(&mut bytes, 0x90 + 6, 0xFFEC_FFF6); // bounds.top, left
        put(&mut bytes, 0x90 + 10, 0x0258_0320); // bounds.bottom, right
        assert_eq!(
            legacy_port_bounds(&mut Bytes(bytes.clone()), window as u32),
            Some(LegacyPortBounds {
                lent: [0xFFEC_FFF6, 0x0258_0320],
                saved: [0x1111_2222, 0x3333_4444],
            })
        );
        bytes[window + 6] = 0; // a classic GrafPort already holds portBits
        assert_eq!(legacy_port_bounds(&mut Bytes(bytes), window as u32), None);
    }
}
