//! Validated guest Notification Manager records for shared presentation.

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NotificationSnapshot {
    pub guest_id: u32,
    /// Changes on removal/reinstallation, including reuse after a guest launch.
    pub instance_id: u64,
    pub response_started: bool,
    pub mark: i16,
    pub icon_handle: u32,
    pub sound_handle: u32,
    /// Original Pascal-string payload, in Macintosh Roman. None means no alert.
    pub text: Option<Vec<u8>>,
    pub response: u32,
    pub ref_con: u32,
}

pub(super) fn snapshot_request(
    guest_id: u32,
    instance_id: u64,
    response_started: bool,
    mut read: impl FnMut(u32) -> Option<u8>,
) -> Option<NotificationSnapshot> {
    if guest_id == 0 { return None; }
    let mut record = [0u8; 36];
    for (offset, byte) in record.iter_mut().enumerate() {
        *byte = read(guest_id.checked_add(offset as u32)?)?;
    }
    if u16::from_be_bytes([record[4], record[5]]) != 8 { return None; }
    let long = |offset: usize| u32::from_be_bytes(record[offset..offset + 4].try_into().unwrap());
    let text_ptr = long(24);
    let text = if text_ptr == 0 { None } else {
        let length = read(text_ptr)?;
        let mut bytes = Vec::with_capacity(length as usize);
        for offset in 1..=u32::from(length) {
            bytes.push(read(text_ptr.checked_add(offset)?)?);
        }
        Some(bytes)
    };
    Some(NotificationSnapshot {
        guest_id, instance_id, response_started,
        mark: i16::from_be_bytes([record[14], record[15]]),
        icon_handle: long(16), sound_handle: long(20), text,
        response: long(28), ref_con: long(32),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn notification_snapshot_preserves_guest_bytes_and_rejects_unmapped_text() {
        let mut memory = vec![0; 96];
        memory[8 + 5] = 8;
        memory[8 + 14..8 + 16].copy_from_slice(&(-1i16).to_be_bytes());
        memory[8 + 24..8 + 28].copy_from_slice(&64u32.to_be_bytes());
        memory[64..68].copy_from_slice(&[3, b'A', 0x8e, b'\r']);
        let snapshot = snapshot_request(8, 42, false, |address| memory.get(address as usize).copied()).unwrap();
        assert_eq!(snapshot.mark, -1);
        assert_eq!(snapshot.instance_id, 42);
        assert_eq!(snapshot.text, Some(vec![b'A', 0x8e, b'\r']));
        assert!(snapshot_request(8, 42, false, |address| (address < 67).then(|| memory[address as usize])).is_none());
        memory[8 + 5] = 7;
        assert!(snapshot_request(8, 42, false, |address| memory.get(address as usize).copied()).is_none());
        assert!(snapshot_request(u32::MAX, 42, false, |_| Some(0)).is_none());
    }
}
