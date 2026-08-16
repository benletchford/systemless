//! 68k loader data types: CODE 0 header, jump table entries, and the
//! [`LoadedApp`] state record returned by
//! [`FixtureRunner::load_app`](crate::runner::FixtureRunner::load_app).

use std::collections::HashMap;

use crate::loader::ppc::PpcLoadedApp;

pub mod cfrg;
pub mod pef;
pub mod ppc;

/// Application `'SIZE'` resource data used by the Process Manager to
/// choose the app's launch partition. The standard application resource
/// is ID -1 and stores a 16-bit mode flag word followed by preferred
/// and minimum partition sizes. Finder may write an ID 0 resource that
/// overrides the developer-provided ID -1 resource at launch.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ApplicationSizeResource {
    pub flags: u16,
    pub preferred_size: u32,
    pub minimum_size: u32,
}

impl ApplicationSizeResource {
    /// `isHighLevelEventAware` is bit 6 of the 16-bit `'SIZE'` flags field.
    /// Macintosh Toolbox Essentials 1992, pp. 2-30 to 2-32.
    pub const HIGH_LEVEL_EVENT_AWARE: u16 = 0x0040;

    pub fn parse(data: &[u8]) -> Option<Self> {
        if data.len() < 10 {
            return None;
        }
        Some(Self {
            flags: u16::from_be_bytes([data[0], data[1]]),
            preferred_size: u32::from_be_bytes([data[2], data[3], data[4], data[5]]),
            minimum_size: u32::from_be_bytes([data[6], data[7], data[8], data[9]]),
        })
    }

    pub fn preferred_partition_size(self) -> Option<u32> {
        if self.preferred_size >= self.minimum_size && self.preferred_size >= 128 * 1024 {
            Some(self.preferred_size)
        } else {
            None
        }
    }

    pub fn is_high_level_event_aware(self) -> bool {
        self.flags & Self::HIGH_LEVEL_EVENT_AWARE != 0
    }
}

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

/// Full header used by MPW far-model `CODE` resources.
///
/// Mac OS Runtime Architectures describes the far header as carrying
/// relocation stream offsets for A5-relative and PC-relative longwords.
/// The streams encode deltas in words; the Segment Manager applies them
/// after loading the segment.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MpwFarSegmentHeader {
    pub near_entry_start_a5_offset: u32,
    pub near_entry_count: u32,
    pub far_entry_start_a5_offset: u32,
    pub far_entry_count: u32,
    pub a5_relocation_data_offset: u32,
    pub current_a5: u32,
    pub pc_relocation_data_offset: u32,
    pub load_address: u32,
}

impl MpwFarSegmentHeader {
    pub const SIZE: usize = 40;
    pub const CURRENT_A5_OFFSET: u32 = 24;
    pub const LOAD_ADDRESS_OFFSET: u32 = 32;

    pub fn parse(data: &[u8]) -> Option<Self> {
        if data.len() < Self::SIZE || u16::from_be_bytes([data[0], data[1]]) != 0xFFFF {
            return None;
        }

        Some(Self {
            near_entry_start_a5_offset: read_be_u32(data, 4)?,
            near_entry_count: read_be_u32(data, 8)?,
            far_entry_start_a5_offset: read_be_u32(data, 12)?,
            far_entry_count: read_be_u32(data, 16)?,
            a5_relocation_data_offset: read_be_u32(data, 20)?,
            current_a5: read_be_u32(data, 24)?,
            pc_relocation_data_offset: read_be_u32(data, 28)?,
            load_address: read_be_u32(data, 32)?,
        })
    }

    pub fn a5_relocation_offsets(self, data: &[u8]) -> Option<Vec<u32>> {
        let Some((start, end)) = relocation_stream_bounds(
            self.a5_relocation_data_offset,
            Some(self.pc_relocation_data_offset),
            data.len(),
        )?
        else {
            return Some(Vec::new());
        };
        decode_relocation_offsets(data, start, end)
    }

    pub fn pc_relocation_offsets(self, data: &[u8]) -> Option<Vec<u32>> {
        let Some((start, end)) =
            relocation_stream_bounds(self.pc_relocation_data_offset, None, data.len())?
        else {
            return Some(Vec::new());
        };
        decode_relocation_offsets(data, start, end)
    }
}

fn read_be_u32(data: &[u8], offset: usize) -> Option<u32> {
    let bytes = data.get(offset..offset + 4)?;
    Some(u32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
}

fn relocation_stream_bounds(
    start: u32,
    following_start: Option<u32>,
    data_len: usize,
) -> Option<Option<(usize, usize)>> {
    if start == 0 {
        return Some(None);
    }

    let start = start as usize;
    if start >= data_len {
        return None;
    }

    let end = following_start
        .filter(|&next| next != 0)
        .map(|next| next as usize)
        .filter(|&next| next > start && next <= data_len)
        .unwrap_or(data_len);

    Some(Some((start, end)))
}

fn decode_relocation_offsets(data: &[u8], start: usize, end: usize) -> Option<Vec<u32>> {
    if start > end || end > data.len() {
        return None;
    }

    let mut offsets = Vec::new();
    let mut cursor = start;
    let mut offset = 0u32;
    let data_len = data.len() as u32;

    while cursor < end {
        let first = *data.get(cursor)?;
        cursor += 1;

        let units = if first == 0 {
            let second = *data.get(cursor)?;
            cursor += 1;
            if second == 0 {
                break;
            }
            if (second & 0x80) == 0 {
                return None;
            }
            if cursor + 3 > end {
                return None;
            }
            let third = *data.get(cursor)?;
            let fourth = *data.get(cursor + 1)?;
            let fifth = *data.get(cursor + 2)?;
            cursor += 3;
            (((second & 0x7F) as u32) << 24)
                | ((third as u32) << 16)
                | ((fourth as u32) << 8)
                | fifth as u32
        } else if (first & 0x80) != 0 {
            let second = *data.get(cursor)?;
            cursor += 1;
            (((first & 0x7F) as u32) << 8) | second as u32
        } else {
            first as u32
        };

        let delta = units.checked_mul(2)?;
        offset = offset.checked_add(delta)?;
        if offset.checked_add(4)? > data_len {
            return None;
        }
        offsets.push(offset);
    }

    Some(offsets)
}

#[cfg(test)]
mod tests {
    use super::{ApplicationSizeResource, CodeSegmentHeader, MpwFarSegmentHeader};

    #[test]
    fn parses_application_size_resource_flags_and_partition_sizes() {
        let bytes = [0x51, 0x80, 0x00, 0x30, 0x00, 0x00, 0x00, 0x20, 0x00, 0x00];
        let size = ApplicationSizeResource::parse(&bytes).expect("SIZE resource should parse");

        assert_eq!(size.flags, 0x5180);
        assert_eq!(size.preferred_size, 0x0030_0000);
        assert_eq!(size.minimum_size, 0x0020_0000);
        assert_eq!(size.preferred_partition_size(), Some(0x0030_0000));
        assert!(!size.is_high_level_event_aware());

        let aware = ApplicationSizeResource {
            flags: size.flags | ApplicationSizeResource::HIGH_LEVEL_EVENT_AWARE,
            ..size
        };
        assert!(aware.is_high_level_event_aware());
    }

    #[test]
    fn rejects_truncated_or_inverted_application_size_resources() {
        assert!(ApplicationSizeResource::parse(&[0; 9]).is_none());

        let inverted = ApplicationSizeResource {
            flags: 0,
            preferred_size: 0x0010_0000,
            minimum_size: 0x0020_0000,
        };
        assert_eq!(inverted.preferred_partition_size(), None);
    }

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

    #[test]
    fn decodes_mpw_far_relocation_stream_offsets() {
        let mut data = vec![0u8; 0x1450];
        data[0..2].copy_from_slice(&0xFFFFu16.to_be_bytes());
        data[20..24].copy_from_slice(&0x40u32.to_be_bytes());
        data[28..32].copy_from_slice(&0x44u32.to_be_bytes());

        data[0x40..0x44].copy_from_slice(&[
            0x8A, 0x14, // two-byte delta: 0x0A14 words => byte offset 0x1428
            0x00, 0x00, // end
        ]);
        data[0x44..0x47].copy_from_slice(&[
            0x15, // one-byte delta: 0x15 words => byte offset 0x2A
            0x00, 0x00,
        ]);

        let header = MpwFarSegmentHeader::parse(&data).expect("parse MPW far header");

        assert_eq!(header.a5_relocation_offsets(&data), Some(vec![0x1428]));
        assert_eq!(header.pc_relocation_offsets(&data), Some(vec![0x2A]));
    }
}

/// State produced by loading a 68k application: parsed CODE 0 header,
/// resolved A5 placement, jump-table slot vector, per-segment load
/// addresses, the end of the direct-loaded image, and the initial stack
/// pointer the runner will seed.
///
/// Returned by
/// [`FixtureRunner::load_app`](crate::runner::FixtureRunner::load_app)
/// and consumed by
/// [`FixtureRunner::init_app`](crate::runner::FixtureRunner::init_app).
#[derive(Default)]
pub struct LoadedApp {
    /// Loaded PowerPC PEF application state, when the selected executable
    /// is a native CFM/PEF app instead of a classic 68k CODE app.
    pub ppc: Option<PpcLoadedApp>,
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
    /// First byte after direct loader-owned memory (A5 world, CODE 0,
    /// jump table, and preloaded CODE segments). Heap allocations must
    /// start at or above this boundary.
    pub loaded_image_end: u32,
    /// Initial stack pointer (top of below-A5 region) the runner
    /// seeds A7 with before the first instruction.
    pub initial_sp: u32,
    /// Parsed application `'SIZE'` launch settings, preferring a valid ID 0
    /// resource over the developer-provided ID -1 resource.
    pub size_resource: Option<ApplicationSizeResource>,
}

impl LoadedApp {
    pub fn from_ppc(ppc: PpcLoadedApp) -> Self {
        Self {
            ppc: Some(ppc),
            ..Self::default()
        }
    }

    pub fn is_powerpc(&self) -> bool {
        self.ppc.is_some()
    }

    pub fn entry_point(&self, a5_base: u32) -> u32 {
        a5_base + self.code0_header.jump_table_offset + 2
    }
}
