//! 68k loader data types: CODE 0 header, jump table entries, and the
//! [`LoadedApp`] state record returned by
//! [`FixtureRunner::load_app`](crate::runner::FixtureRunner::load_app).

use std::collections::HashMap;

/// CODE 0 resource header — 16 bytes parsed from the start of every
/// 68k application's `CODE` resource ID 0. Defines the A5-world layout
/// (above + below sizes) and where the jump table lives within it.
/// Inside Macintosh: Memory 1992, 7-31 ("CODE Resource Format").
#[derive(Debug, Clone, Default)]
pub struct Code0Header {
    /// Bytes of A5-world space above A5 (application globals, not
    /// counting the jump table itself).
    pub above_a5: u32,
    /// Bytes of A5-world space below A5 (parameter area + initial SP).
    pub below_a5: u32,
    /// Total size in bytes of the jump table region (8 bytes per entry).
    pub jump_table_size: u32,
    /// Byte offset from A5 to the jump table base (typically 32).
    pub jump_table_offset: u32,
}

impl Code0Header {
    /// Parse a 16-byte CODE 0 header from `data` (4 big-endian
    /// `u32` fields). Returns `None` if `data` is shorter than 16
    /// bytes; otherwise infallible.
    pub fn parse(data: &[u8]) -> Option<Self> {
        if data.len() < 16 {
            return None;
        }
        Some(Self {
            above_a5: u32::from_be_bytes([data[0], data[1], data[2], data[3]]),
            below_a5: u32::from_be_bytes([data[4], data[5], data[6], data[7]]),
            jump_table_size: u32::from_be_bytes([data[8], data[9], data[10], data[11]]),
            jump_table_offset: u32::from_be_bytes([data[12], data[13], data[14], data[15]]),
        })
    }

    /// Number of jump-table entries (each entry is 8 bytes).
    pub fn num_entries(&self) -> usize {
        (self.jump_table_size / 8) as usize
    }
}

/// One slot in the application's jump table. The Mac OS Segment Loader
/// patches each slot's `loaded` + `address` lazily as `LoadSeg` faults
/// pull CODE segments into memory.
#[derive(Debug, Clone)]
pub struct JumpTableEntry {
    /// Byte offset within the target segment of the call destination.
    pub offset: u16,
    /// CODE resource ID containing the call destination.
    pub segment: i16,
    /// True once the segment has been loaded and the slot patched.
    pub loaded: bool,
    /// Resolved guest address of the call destination (valid when
    /// `loaded == true`).
    pub address: u32,
}

/// Header stored at the front of each nonzero `CODE` resource.
///
/// MPW-style near segments use the documented `tabOff, nEntries`
/// format. Symantec/THINK far CODE uses the same four bytes differently:
/// word 0 stores the first jump-table entry index plus the relocation
/// flag, and word 1 has bit `$4000` set plus the entry count.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CodeSegmentHeader {
    /// MPW far-model segment with a 40-byte header (`$FFFF` marker).
    MpwFar,
    /// Standard near-model segment: byte offset from the current jump
    /// table base, plus number of entries owned by the segment.
    Near { table_offset: u16, entry_count: u16 },
    /// Symantec/THINK far CODE segment.
    ThinkFar {
        has_relocations: bool,
        first_entry_index: u16,
        entry_count: u16,
    },
}

impl CodeSegmentHeader {
    const THINK_RELOC_FLAG: u16 = 0x8000;
    const THINK_FAR_FLAG: u16 = 0x4000;

    pub fn parse(data: &[u8]) -> Option<Self> {
        if data.len() < 4 {
            return None;
        }

        let first = u16::from_be_bytes([data[0], data[1]]);
        let second = u16::from_be_bytes([data[2], data[3]]);
        Some(Self::from_words(first, second))
    }

    pub fn from_words(first: u16, second: u16) -> Self {
        if first == 0xFFFF {
            Self::MpwFar
        } else if (second & Self::THINK_FAR_FLAG) != 0 {
            Self::ThinkFar {
                has_relocations: (first & Self::THINK_RELOC_FLAG) != 0,
                first_entry_index: first & !Self::THINK_RELOC_FLAG,
                entry_count: second & 0x3FFF,
            }
        } else {
            Self::Near {
                table_offset: first,
                entry_count: second,
            }
        }
    }

    pub fn code_header_size(self) -> u32 {
        match self {
            Self::MpwFar => 40,
            Self::Near { .. } | Self::ThinkFar { .. } => 4,
        }
    }

    pub fn jump_table_start_offset(self) -> Option<u32> {
        match self {
            Self::MpwFar => None,
            Self::Near { table_offset, .. } => Some(table_offset as u32),
            Self::ThinkFar {
                first_entry_index, ..
            } => Some(first_entry_index as u32 * 8),
        }
    }

    pub fn jump_table_entry_count(self) -> Option<u32> {
        match self {
            Self::MpwFar => None,
            Self::Near { entry_count, .. } | Self::ThinkFar { entry_count, .. } => {
                Some(entry_count as u32)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::CodeSegmentHeader;

    #[test]
    fn parses_think_far_header_entry_index_and_count_flags() {
        let header = CodeSegmentHeader::from_words(0x8051, 0x4085);

        assert_eq!(
            header,
            CodeSegmentHeader::ThinkFar {
                has_relocations: true,
                first_entry_index: 0x0051,
                entry_count: 0x0085,
            }
        );
        assert_eq!(header.code_header_size(), 4);
        assert_eq!(header.jump_table_start_offset(), Some(0x0051 * 8));
        assert_eq!(header.jump_table_entry_count(), Some(0x0085));
    }

    #[test]
    fn parses_near_and_mpw_far_segment_headers() {
        let near = CodeSegmentHeader::from_words(0x0018, 0x0002);
        assert_eq!(
            near,
            CodeSegmentHeader::Near {
                table_offset: 0x0018,
                entry_count: 2,
            }
        );
        assert_eq!(near.code_header_size(), 4);
        assert_eq!(near.jump_table_start_offset(), Some(0x0018));
        assert_eq!(near.jump_table_entry_count(), Some(2));

        let mpw_far = CodeSegmentHeader::from_words(0xFFFF, 0x0000);
        assert_eq!(mpw_far, CodeSegmentHeader::MpwFar);
        assert_eq!(mpw_far.code_header_size(), 40);
        assert_eq!(mpw_far.jump_table_start_offset(), None);
        assert_eq!(mpw_far.jump_table_entry_count(), None);
    }
}

/// State produced by loading a 68k application: parsed CODE 0 header,
/// resolved A5 placement, jump-table slot vector, per-segment load
/// addresses, and the initial stack pointer the runner will seed.
///
/// Returned by
/// [`FixtureRunner::load_app`](crate::runner::FixtureRunner::load_app)
/// and consumed by
/// [`FixtureRunner::init_app`](crate::runner::FixtureRunner::init_app).
#[derive(Default)]
pub struct LoadedApp {
    /// Parsed CODE 0 header bytes (above_a5 / below_a5 / jt_size / jt_offset).
    pub code0_header: Code0Header,
    /// Guest address chosen for A5; A5-relative globals + jump table
    /// are placed relative to this base.
    pub a5_base: u32,
    /// Materialised jump-table slot vector; one entry per CODE call site.
    pub jump_table: Vec<JumpTableEntry>,
    /// Map from CODE resource ID to the guest address where each
    /// segment was loaded.
    pub segment_bases: HashMap<i16, u32>,
    /// Initial stack pointer (top of below-A5 region) the runner
    /// seeds A7 with before the first instruction.
    pub initial_sp: u32,
}

impl LoadedApp {
    pub fn entry_point(&self, a5_base: u32) -> u32 {
        a5_base + self.code0_header.jump_table_offset + 2
    }
}
