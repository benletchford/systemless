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
