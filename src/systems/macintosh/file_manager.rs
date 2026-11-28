//! Architecture-neutral evaluation helpers for classic Macintosh File Manager
//! parameter-block reads, shared by the 68K `_Read` trap and the PowerPC
//! `PBRead` import.
//!
//! Inside Macintosh: Files (1992), pp. 2-84, 2-121--2-122, 2-238.

/// `ioPosMode` bit requesting a newline-mode read; the newline character is
/// held in the high byte of `ioPosMode`.
pub(crate) const NEWLINE_MODE: u16 = 0x0080;

/// How many bytes a `PBRead` transfers and whether it completes with `eofErr`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ReadExtent {
    pub(crate) len: usize,
    pub(crate) eof: bool,
}

/// Evaluate a read of `requested` bytes from `remaining`, the file's bytes
/// from the starting mark to its logical end of file.
///
/// In newline mode the read ends after the first newline character within
/// the transfer and completes without error; otherwise it transfers up to
/// `requested` bytes and reports `eofErr` when the file ends first.
pub(crate) fn read_extent(remaining: &[u8], requested: usize, pos_mode: u16) -> ReadExtent {
    let available = requested.min(remaining.len());
    if pos_mode & NEWLINE_MODE != 0 {
        let newline = (pos_mode >> 8) as u8;
        if let Some(offset) = remaining[..available]
            .iter()
            .position(|&byte| byte == newline)
        {
            return ReadExtent {
                len: offset + 1,
                eof: false,
            };
        }
    }
    ReadExtent {
        len: available,
        eof: available < requested,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const RETURN_NEWLINE_MODE: u16 = 0x0D80;

    #[test]
    fn newline_mode_ends_after_the_first_newline() {
        assert_eq!(
            read_extent(b"[Section]\rKEY=VALUE\r\r", 500, RETURN_NEWLINE_MODE),
            ReadExtent {
                len: 10,
                eof: false
            }
        );
    }

    #[test]
    fn newline_as_the_last_byte_of_the_file_is_not_end_of_file() {
        assert_eq!(
            read_extent(b"last\r", 500, RETURN_NEWLINE_MODE),
            ReadExtent { len: 5, eof: false }
        );
    }

    #[test]
    fn newline_mode_without_a_newline_reads_to_end_of_file() {
        assert_eq!(
            read_extent(b"tail", 16, RETURN_NEWLINE_MODE),
            ReadExtent { len: 4, eof: true }
        );
    }

    #[test]
    fn newline_beyond_the_request_is_not_reached() {
        assert_eq!(
            read_extent(b"abcdef\r", 3, RETURN_NEWLINE_MODE),
            ReadExtent { len: 3, eof: false }
        );
    }

    #[test]
    fn newline_character_is_taken_from_the_high_byte() {
        assert_eq!(
            read_extent(b"a\rb\nc", 16, 0x0A80),
            ReadExtent { len: 4, eof: false }
        );
    }

    #[test]
    fn without_newline_mode_a_newline_does_not_end_the_read() {
        assert_eq!(
            read_extent(b"a\rb", 3, 0x0D00),
            ReadExtent { len: 3, eof: false }
        );
        assert_eq!(
            read_extent(b"a\rb", 8, 0x0D00),
            ReadExtent { len: 3, eof: true }
        );
    }
}

/// fsCurPerm: whatever permission is allowed.
pub(crate) const FS_CUR_PERM: u8 = 0;

/// Whether an open's `ioPermssn` explicitly asks to write: fsWrPerm (2),
/// fsRdWrPerm (3) or fsRdWrShPerm (4). The File Manager has no write-only
/// access, so fsWrPerm is read/write too. Such a request on a locked volume
/// fails rather than falling back to read-only. Inside Macintosh: Files
/// (1992), pp. 2-7--2-8.
pub(crate) fn permission_requests_write(permission: i16) -> bool {
    matches!(permission, 2..=4)
}

/// Whether an access path opened with `permission` can write, given whether
/// the file can be written at all. fsCurPerm requests "whatever permission is
/// currently allowed": read/write when available, otherwise read-only
/// (Files, p. 2-8).
pub(crate) fn permission_grants_write(permission: i16, file_writable: bool) -> bool {
    file_writable && (permission == i16::from(FS_CUR_PERM) || permission_requests_write(permission))
}

#[cfg(test)]
mod permission_tests {
    use super::*;

    #[test]
    fn current_permission_writes_when_it_can_and_read_permission_never_does() {
        for permission in 0..=4i16 {
            assert_eq!(permission_requests_write(permission), permission >= 2);
            assert!(!permission_grants_write(permission, false));
        }
        assert!(permission_grants_write(i16::from(FS_CUR_PERM), true));
        assert!(!permission_grants_write(1, true)); // fsRdPerm
        assert!(permission_grants_write(4, true)); // fsRdWrShPerm

        // A byte that isn't a permission, such as a vRefNum's low byte.
        assert!(!permission_grants_write(0xFF, true));
        assert!(!permission_grants_write(-1, true));
    }
}
