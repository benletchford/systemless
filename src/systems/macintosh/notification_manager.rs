//! Architecture-neutral evaluation helpers and canonical structures for
//! Macintosh Notification Manager operations.
//!
//! Inside Macintosh Volume VI (1991), Chapter 24 ("The Notification Manager"), pp. 24-1--24-11.
//! Inside Macintosh: Processes (1994), Chapter 5 ("Notification Manager"), pp. 5-3--5-28.
//!
//! The Notification Manager allows background applications, system software extensions,
//! and device drivers to alert the user about completed tasks or conditions requiring
//! attention. Notifications can post an Apple menu mark, cycle an icon in the menu bar,
//! play a sound, present an alert box, and execute a response procedure.
//!
//! On 68k, the Notification Manager is invoked via OS traps `NMInstall` ($A05E) and
//! `NMRemove` ($A05F) with pointer `nmReqPtr` in register `A0`. On PowerPC, `InterfaceLib`
//! exports `NMInstall` and `NMRemove`.

#![allow(dead_code)]

/// Expected queue element type for Notification Manager requests: `ORD(nmType) = 8`.
/// Inside Macintosh Volume VI (1991), p. 24-10.
pub const NM_TYPE: u16 = 8;
pub const ORD_NM_TYPE: u16 = 8;

/// Standard size in bytes of a Notification Manager record (`NMRec`).
/// Inside Macintosh Volume VI (1991), p. 24-6.
pub const NM_REC_SIZE: usize = 36;

/// Offset of `qLink` in `NMRec` (longword, next queue element pointer).
pub const Q_LINK_OFFSET: u32 = 0;

/// Offset of `qType` in `NMRec` (word, queue type, must be `ORD(nmType) = 8`).
pub const Q_TYPE_OFFSET: u32 = 4;

/// Offset of `nmFlags` in `NMRec` (word, reserved).
pub const NM_FLAGS_OFFSET: u32 = 6;

/// Offset of `nmPrivate` in `NMRec` (longword, reserved for Notification Manager).
pub const NM_PRIVATE_OFFSET: u32 = 8;

/// Offset of `nmReserved` in `NMRec` (word, reserved).
pub const NM_RESERVED_OFFSET: u32 = 12;

/// Offset of `nmMark` in `NMRec` (word, item to mark in Apple menu or mark character).
pub const NM_MARK_OFFSET: u32 = 14;

/// Offset of `nmIcon` in `NMRec` (longword, handle to small icon 'SICN').
pub const NM_ICON_OFFSET: u32 = 16;

/// Offset of `nmSound` in `NMRec` (longword, handle to sound 'snd ' resource).
pub const NM_SOUND_OFFSET: u32 = 20;

/// Offset of `nmStr` in `NMRec` (longword, pointer to Pascal string for notification alert).
pub const NM_STR_OFFSET: u32 = 24;

/// Offset of `nmResp` in `NMRec` (longword, pointer to response procedure, or -1 / 0).
pub const NM_RESP_OFFSET: u32 = 28;

/// Offset of `nmRefCon` in `NMRec` (longword, reference constant for application use).
pub const NM_REF_CON_OFFSET: u32 = 32;

/// Predefined response procedure value (`-1` as a 32-bit pointer) indicating that the
/// Notification Manager should remove the notification request immediately after posting it.
/// Inside Macintosh Volume VI (1991), p. 24-8.
pub const NM_RESP_REMOVE: u32 = u32::MAX;

/// Notification Manager result code: no error (`noErr = 0`).
pub const NO_ERR: i16 = 0;

/// Notification Manager result code: queue element not found (`qErr = -1`).
/// Inside Macintosh Volume VI (1991), p. 24-11.
pub const Q_ERR: i16 = -1;

/// Notification Manager result code: invalid queue element type (`nmTypErr = -299`).
/// Inside Macintosh Volume VI (1991), p. 24-10.
pub const NM_TYP_ERR: i16 = -299;

/// Size of the 68k response procedure trampoline in bytes.
pub const NOTIFICATION_TRAMPOLINE_SIZE: usize = 28;

/// CPU architecture dispatching the Notification Manager operation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NotificationArchitecture {
    M68k,
    PowerPc,
}

/// Canonical structured representation of a Notification Manager record (`NMRec`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NotificationRecord {
    pub q_link: u32,
    pub q_type: u16,
    pub nm_flags: u16,
    pub nm_private: u32,
    pub nm_reserved: u16,
    pub nm_mark: i16,
    pub nm_icon: u32,
    pub nm_sound: u32,
    pub nm_str: u32,
    pub nm_resp: u32,
    pub nm_ref_con: u32,
}

impl NotificationRecord {
    /// Deserializes an `NMRec` from 36 big-endian bytes.
    pub fn from_bytes(bytes: &[u8; NM_REC_SIZE]) -> Self {
        Self {
            q_link: u32::from_be_bytes(bytes[0..4].try_into().unwrap()),
            q_type: u16::from_be_bytes(bytes[4..6].try_into().unwrap()),
            nm_flags: u16::from_be_bytes(bytes[6..8].try_into().unwrap()),
            nm_private: u32::from_be_bytes(bytes[8..12].try_into().unwrap()),
            nm_reserved: u16::from_be_bytes(bytes[12..14].try_into().unwrap()),
            nm_mark: i16::from_be_bytes(bytes[14..16].try_into().unwrap()),
            nm_icon: u32::from_be_bytes(bytes[16..20].try_into().unwrap()),
            nm_sound: u32::from_be_bytes(bytes[20..24].try_into().unwrap()),
            nm_str: u32::from_be_bytes(bytes[24..28].try_into().unwrap()),
            nm_resp: u32::from_be_bytes(bytes[28..32].try_into().unwrap()),
            nm_ref_con: u32::from_be_bytes(bytes[32..36].try_into().unwrap()),
        }
    }

    /// Serializes an `NMRec` to 36 big-endian bytes.
    pub fn to_bytes(&self) -> [u8; NM_REC_SIZE] {
        let mut bytes = [0u8; NM_REC_SIZE];
        bytes[0..4].copy_from_slice(&self.q_link.to_be_bytes());
        bytes[4..6].copy_from_slice(&self.q_type.to_be_bytes());
        bytes[6..8].copy_from_slice(&self.nm_flags.to_be_bytes());
        bytes[8..12].copy_from_slice(&self.nm_private.to_be_bytes());
        bytes[12..14].copy_from_slice(&self.nm_reserved.to_be_bytes());
        bytes[14..16].copy_from_slice(&self.nm_mark.to_be_bytes());
        bytes[16..20].copy_from_slice(&self.nm_icon.to_be_bytes());
        bytes[20..24].copy_from_slice(&self.nm_sound.to_be_bytes());
        bytes[24..28].copy_from_slice(&self.nm_str.to_be_bytes());
        bytes[28..32].copy_from_slice(&self.nm_resp.to_be_bytes());
        bytes[32..36].copy_from_slice(&self.nm_ref_con.to_be_bytes());
        bytes
    }

    /// Returns `true` if `q_type` matches `ORD(nmType) = 8`.
    pub fn is_valid_type(&self) -> bool {
        self.q_type == NM_TYPE
    }

    /// Returns `true` if `nm_resp` is `-1`, requesting immediate automatic removal.
    pub fn is_auto_remove(&self) -> bool {
        self.nm_resp == NM_RESP_REMOVE
    }
}

/// Validates that `nm_rec` is a non-null pointer and `q_type` equals `NM_TYPE` (8).
pub fn validate_nm_rec(nm_rec: u32, q_type: u16) -> Result<(), i16> {
    if nm_rec == 0 || q_type != NM_TYPE {
        Err(NM_TYP_ERR)
    } else {
        Ok(())
    }
}

/// Action to apply when installing a notification request.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NmInstallAction {
    /// The request is already present in the queue; installation is a no-op returning `noErr`.
    AlreadyPresent,
    /// The request is added to the tail of the queue.
    Enqueued {
        /// Previous tail pointer in the queue that must link to `nm_rec`, if any.
        previous_tail: Option<u32>,
        /// Optional procedure pointer to arm as the notification response callback.
        arm_response: Option<u32>,
    },
    /// The request specified `nmResp = -1` (immediate automatic removal).
    /// The active queue elements remain unchanged, and `nm_rec.qLink` is cleared to 0.
    AutoRemoved,
}

/// Evaluates installation of a notification request into the active queue.
pub fn evaluate_nm_install(
    nm_rec: u32,
    q_type: u16,
    queue: &[u32],
    nm_resp: u32,
) -> Result<NmInstallAction, i16> {
    validate_nm_rec(nm_rec, q_type)?;
    if queue.contains(&nm_rec) {
        return Ok(NmInstallAction::AlreadyPresent);
    }
    if nm_resp == NM_RESP_REMOVE {
        return Ok(NmInstallAction::AutoRemoved);
    }
    let previous_tail = queue.last().copied();
    let arm_response = if nm_resp != 0 { Some(nm_resp) } else { None };
    Ok(NmInstallAction::Enqueued {
        previous_tail,
        arm_response,
    })
}

/// Action to apply when removing a notification request from the active queue.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NmRemoveAction {
    /// Index in the active queue vector to remove.
    pub queue_index: usize,
    /// Pointer to the request whose `qLink` must be cleared to 0.
    pub clear_link_ptr: u32,
    /// If there was a preceding queue element, update its `qLink` to the subsequent element (or 0).
    pub update_link: Option<(u32, u32)>,
}

/// Evaluates removal of a notification request from the active queue.
pub fn evaluate_nm_remove(
    nm_rec: u32,
    q_type: u16,
    queue: &[u32],
) -> Result<NmRemoveAction, i16> {
    validate_nm_rec(nm_rec, q_type)?;
    let Some(queue_index) = queue.iter().position(|&req| req == nm_rec) else {
        return Err(Q_ERR);
    };
    let update_link = if queue_index > 0 {
        let previous = queue[queue_index - 1];
        let next = queue.get(queue_index + 1).copied().unwrap_or(0);
        Some((previous, next))
    } else {
        None
    };
    Ok(NmRemoveAction {
        queue_index,
        clear_link_ptr: nm_rec,
        update_link,
    })
}

/// Builds the 28-byte 68k trampoline for calling a Pascal response procedure `MyResponse(nmReqPtr: NMRecPtr)`.
///
/// Trampoline instructions:
/// ```text
/// 48E7 F0F0    MOVEM.L D0-D3/A0-A3,-(SP)
/// 2F3C xxxxxxxx MOVE.L  #nmReqPtr,-(SP)
/// 4EB9 xxxxxxxx JSR     response
/// 2E7C xxxxxxxx MOVEA.L #savedRegsSP,A7
/// 4CDF 0F0F    MOVEM.L (SP)+,D0-D3/A0-A3
/// 4E75         RTS
/// ```
pub fn build_68k_notification_response_trampoline(
    nm_rec: u32,
    response: u32,
    saved_regs_sp: u32,
) -> [u8; NOTIFICATION_TRAMPOLINE_SIZE] {
    let mut bytes = [0u8; NOTIFICATION_TRAMPOLINE_SIZE];
    // MOVEM.L D0-D3/A0-A3,-(SP)
    bytes[0..2].copy_from_slice(&0x48E7u16.to_be_bytes());
    bytes[2..4].copy_from_slice(&0xF0F0u16.to_be_bytes());
    // MOVE.L #nmReqPtr,-(SP)
    bytes[4..6].copy_from_slice(&0x2F3Cu16.to_be_bytes());
    bytes[6..10].copy_from_slice(&nm_rec.to_be_bytes());
    // JSR abs.L
    bytes[10..12].copy_from_slice(&0x4EB9u16.to_be_bytes());
    bytes[12..16].copy_from_slice(&response.to_be_bytes());
    // MOVEA.L #savedRegsSP,A7
    bytes[16..18].copy_from_slice(&0x2E7Cu16.to_be_bytes());
    bytes[18..22].copy_from_slice(&saved_regs_sp.to_be_bytes());
    // MOVEM.L (SP)+,D0-D3/A0-A3
    bytes[22..24].copy_from_slice(&0x4CDFu16.to_be_bytes());
    bytes[24..26].copy_from_slice(&0x0F0Fu16.to_be_bytes());
    // RTS
    bytes[26..28].copy_from_slice(&0x4E75u16.to_be_bytes());
    bytes
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validate_nm_rec_checks_null_and_type() {
        assert_eq!(validate_nm_rec(0, NM_TYPE), Err(NM_TYP_ERR));
        assert_eq!(validate_nm_rec(0x1000, 0), Err(NM_TYP_ERR));
        assert_eq!(validate_nm_rec(0x1000, 7), Err(NM_TYP_ERR));
        assert_eq!(validate_nm_rec(0x1000, 9), Err(NM_TYP_ERR));
        assert_eq!(validate_nm_rec(0x1000, NM_TYPE), Ok(()));
    }

    #[test]
    fn notification_record_round_trips_bytes() {
        let record = NotificationRecord {
            q_link: 0x1122_3344,
            q_type: NM_TYPE,
            nm_flags: 0,
            nm_private: 0xAA55_AA55,
            nm_reserved: 0,
            nm_mark: 1,
            nm_icon: 0x1000,
            nm_sound: 0x2000,
            nm_str: 0x3000,
            nm_resp: 0x4000,
            nm_ref_con: 0x55AA_55AA,
        };
        let bytes = record.to_bytes();
        let parsed = NotificationRecord::from_bytes(&bytes);
        assert_eq!(parsed, record);
        assert!(parsed.is_valid_type());
        assert!(!parsed.is_auto_remove());
    }

    #[test]
    fn notification_record_auto_remove_flag() {
        let record = NotificationRecord {
            q_link: 0,
            q_type: NM_TYPE,
            nm_flags: 0,
            nm_private: 0,
            nm_reserved: 0,
            nm_mark: 0,
            nm_icon: 0,
            nm_sound: 0,
            nm_str: 0,
            nm_resp: NM_RESP_REMOVE,
            nm_ref_con: 0,
        };
        assert!(record.is_auto_remove());
    }

    #[test]
    fn evaluate_nm_install_enqueues_and_links() {
        let mut queue = Vec::new();
        let first = 0x2000;
        let action = evaluate_nm_install(first, NM_TYPE, &queue, 0).unwrap();
        assert_eq!(
            action,
            NmInstallAction::Enqueued {
                previous_tail: None,
                arm_response: None,
            }
        );
        queue.push(first);

        let second = 0x3000;
        let action = evaluate_nm_install(second, NM_TYPE, &queue, 0x4000).unwrap();
        assert_eq!(
            action,
            NmInstallAction::Enqueued {
                previous_tail: Some(first),
                arm_response: Some(0x4000),
            }
        );
        queue.push(second);

        // Idempotent / already present
        let action = evaluate_nm_install(first, NM_TYPE, &queue, 0).unwrap();
        assert_eq!(action, NmInstallAction::AlreadyPresent);

        // Predefined response -1
        let third = 0x5000;
        let action = evaluate_nm_install(third, NM_TYPE, &queue, NM_RESP_REMOVE).unwrap();
        assert_eq!(action, NmInstallAction::AutoRemoved);
    }

    #[test]
    fn evaluate_nm_remove_unlinks_correctly() {
        let queue = vec![0x1000, 0x2000, 0x3000];

        // Removing head (0x1000)
        let action = evaluate_nm_remove(0x1000, NM_TYPE, &queue).unwrap();
        assert_eq!(
            action,
            NmRemoveAction {
                queue_index: 0,
                clear_link_ptr: 0x1000,
                update_link: None,
            }
        );

        // Removing middle (0x2000)
        let action = evaluate_nm_remove(0x2000, NM_TYPE, &queue).unwrap();
        assert_eq!(
            action,
            NmRemoveAction {
                queue_index: 1,
                clear_link_ptr: 0x2000,
                update_link: Some((0x1000, 0x3000)),
            }
        );

        // Removing tail (0x3000)
        let action = evaluate_nm_remove(0x3000, NM_TYPE, &queue).unwrap();
        assert_eq!(
            action,
            NmRemoveAction {
                queue_index: 2,
                clear_link_ptr: 0x3000,
                update_link: Some((0x2000, 0)),
            }
        );

        // Missing from queue returns qErr (-1)
        assert_eq!(evaluate_nm_remove(0x4000, NM_TYPE, &queue), Err(Q_ERR));
        // Invalid type returns nmTypErr (-299)
        assert_eq!(evaluate_nm_remove(0x1000, 5, &queue), Err(NM_TYP_ERR));
    }

    #[test]
    fn trampoline_construction_contains_correct_opcodes() {
        let nm_rec = 0x0012_3456;
        let response = 0x0065_4321;
        let saved_regs_sp = 0x00FF_F000;

        let tramp = build_68k_notification_response_trampoline(nm_rec, response, saved_regs_sp);
        assert_eq!(tramp.len(), NOTIFICATION_TRAMPOLINE_SIZE);

        // Check MOVEM.L D0-D3/A0-A3,-(SP)
        assert_eq!(&tramp[0..4], &[0x48, 0xE7, 0xF0, 0xF0]);
        // Check MOVE.L #nmReqPtr,-(SP)
        assert_eq!(&tramp[4..6], &[0x2F, 0x3C]);
        assert_eq!(&tramp[6..10], &nm_rec.to_be_bytes());
        // Check JSR response
        assert_eq!(&tramp[10..12], &[0x4E, 0xB9]);
        assert_eq!(&tramp[12..16], &response.to_be_bytes());
        // Check MOVEA.L #savedRegsSP,A7
        assert_eq!(&tramp[16..18], &[0x2E, 0x7C]);
        assert_eq!(&tramp[18..22], &saved_regs_sp.to_be_bytes());
        // Check MOVEM.L (SP)+,D0-D3/A0-A3
        assert_eq!(&tramp[22..26], &[0x4C, 0xDF, 0x0F, 0x0F]);
        // Check RTS
        assert_eq!(&tramp[26..28], &[0x4E, 0x75]);
    }
}
