//! Installer VISE 3.x archive parsing and decompression.
//!
//! Classic Macintosh Installer VISE 3.5/3.6 Lite applications keep the
//! complete install tree in an `SVCT` data fork. Each catalog entry carries
//! the destination hierarchy, Finder type/creator, and compressed data and
//! resource fork locations.

use std::io::Read;

use flate2::read::DeflateDecoder;

use crate::trap::types::decode_mac_roman;

const VISE_MAGIC: &[u8; 4] = b"SVCT";
const VISE_CATALOG_MAGIC: &[u8; 4] = b"CVCT";
const VISE_HEADER_LEN: usize = 44;
const VISE_CATALOG_HEADER_LEN: usize = 20;
const VISE_VERSION_35_LITE: u32 = 0x8001_0202;
const VISE_VERSION_36_LITE: u32 = 0x8001_0300;
const VISE_VERSION_EXTENDED_CATALOG: u32 = 0x8001_0307;
const VISE_DIRECTORY_RECORD_LEN: usize = 78;
const VISE_FILE_RECORD_LEN: usize = 120;
const VISE_EXTENDED_CATALOG_PREFIX_LEN: usize = 80;
const VISE_EXTENDED_DIRECTORY_SUFFIX_LEN: usize = 66;
const VISE_EXTENDED_FILE_SUFFIX_LEN: usize = 62;

// Installer VISE 3 archive layout and transform reference:
// ScummVM `common/compression/vise.cpp`, GPL-3.0-or-later, as of
// https://github.com/scummvm/scummvm/blob/209976fdaa3d3081ed22d82bfbae606177f21534/common/compression/vise.cpp
// The table is the inverse byte substitution used by VISE before its raw
// DEFLATE stream is decoded.
const VISE_DEOBFUSCATION_TABLE: [u8; 256] = [
    0x6a, 0xb7, 0x36, 0xec, 0x15, 0xd9, 0xc8, 0x73, 0xe8, 0x38, 0x9a, 0xdf, 0x21, 0x25, 0xd0, 0xcc,
    0xfd, 0xdc, 0x16, 0xd7, 0xe3, 0x43, 0x05, 0xc5, 0x8f, 0x48, 0xda, 0xf2, 0x3f, 0x10, 0x23, 0x6c,
    0x77, 0x7c, 0xf9, 0xa0, 0xa3, 0xe9, 0xed, 0x46, 0x8b, 0xd8, 0xac, 0x54, 0xce, 0x2d, 0x19, 0x5e,
    0x6d, 0x7d, 0x87, 0x5d, 0xfa, 0x5b, 0x9b, 0xe0, 0xc7, 0xee, 0x9f, 0x52, 0xa9, 0xb9, 0x0a, 0xd1,
    0xfe, 0x78, 0x76, 0x4a, 0x3d, 0x44, 0x5a, 0x96, 0x90, 0x1f, 0x26, 0x9d, 0x58, 0x1b, 0x8e, 0x57,
    0x59, 0xc3, 0x0b, 0x6b, 0xfc, 0x1d, 0xe6, 0xa2, 0x7f, 0x92, 0x4f, 0x40, 0xb4, 0x06, 0x72, 0x4d,
    0xf4, 0x34, 0xaa, 0xd2, 0x49, 0xad, 0xef, 0x22, 0x1a, 0xb5, 0xba, 0xbf, 0x29, 0x68, 0x89, 0x93,
    0x3e, 0x32, 0x04, 0xf5, 0xde, 0xe1, 0x6f, 0xfb, 0x67, 0xe4, 0x7e, 0x08, 0xaf, 0xf0, 0xab, 0x41,
    0x82, 0xea, 0x50, 0x0f, 0x2a, 0xc6, 0x35, 0xb3, 0xa8, 0xca, 0xe5, 0x4c, 0x45, 0x8a, 0x97, 0xae,
    0xd6, 0x66, 0x27, 0x53, 0xc9, 0x1c, 0x3c, 0x03, 0x99, 0xc1, 0x09, 0x2e, 0x69, 0x37, 0x8d, 0x2f,
    0x60, 0xc2, 0xa6, 0x18, 0x4e, 0x7a, 0xb8, 0xcf, 0xa7, 0x3a, 0x17, 0xd5, 0x9e, 0xf1, 0x84, 0x51,
    0x0d, 0xa4, 0x64, 0xc4, 0x1e, 0xb1, 0x30, 0x98, 0xbb, 0x79, 0x01, 0xf6, 0x62, 0x0e, 0xb2, 0x63,
    0x91, 0xcb, 0xff, 0x80, 0x71, 0xe7, 0xd4, 0x00, 0xdb, 0x75, 0x2c, 0xbd, 0x39, 0x33, 0x94, 0xbc,
    0x8c, 0x3b, 0xb6, 0x20, 0x85, 0x24, 0x88, 0x2b, 0x70, 0x83, 0x6e, 0x7b, 0x9c, 0xbe, 0x14, 0x47,
    0x65, 0x4b, 0x56, 0x81, 0xf8, 0x12, 0x11, 0x28, 0xeb, 0x55, 0x74, 0xa1, 0x31, 0xf7, 0xb0, 0x13,
    0x86, 0xdd, 0x5f, 0x42, 0xd3, 0x02, 0x61, 0x95, 0x0c, 0x5c, 0xa5, 0xcd, 0xc0, 0x07, 0xe2, 0xf3,
];

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ViseArchive<'a> {
    pub dirs: Vec<String>,
    pub entries: Vec<ViseEntry<'a>>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ViseEntry<'a> {
    pub path: String,
    pub file_type: [u8; 4],
    pub creator: [u8; 4],
    pub data_packed: &'a [u8],
    pub rsrc_packed: &'a [u8],
    pub data_packed_offset: usize,
    pub rsrc_packed_offset: usize,
    pub unpacked_offset: usize,
    pub data_unpacked_len: usize,
    pub rsrc_unpacked_len: usize,
}

#[derive(Clone, Debug)]
struct ViseDirectory {
    path: String,
}

pub fn parse_vise(data: &[u8]) -> Option<Result<ViseArchive<'_>, String>> {
    data.starts_with(VISE_MAGIC)
        .then(|| parse_vise_result(data))
}

fn parse_vise_result(data: &[u8]) -> Result<ViseArchive<'_>, String> {
    let header = range(data, 0, VISE_HEADER_LEN, "header")?;
    let version = read_u32(header, 16, "archive version")?;
    if !matches!(
        version,
        VISE_VERSION_35_LITE | VISE_VERSION_36_LITE | VISE_VERSION_EXTENDED_CATALOG
    ) {
        return Err(format!("unsupported archive version 0x{version:08X}"));
    }

    let catalog_offset = read_u32(header, 36, "catalog offset")? as usize;
    let catalog = range(
        data,
        catalog_offset,
        VISE_CATALOG_HEADER_LEN,
        "catalog header",
    )?;
    if !catalog.starts_with(VISE_CATALOG_MAGIC) {
        return Err(format!(
            "missing CVCT catalog signature at 0x{catalog_offset:X}"
        ));
    }
    let entry_count = read_u16(catalog, 16, "catalog entry count")? as usize;
    let mut cursor = catalog_offset + VISE_CATALOG_HEADER_LEN;
    if version == VISE_VERSION_EXTENDED_CATALOG {
        let prefix = range(
            data,
            cursor,
            VISE_EXTENDED_CATALOG_PREFIX_LEN,
            "extended catalog prefix",
        )?;
        if !prefix.starts_with(b"PACK") {
            return Err("extended catalog is missing PACK prefix".to_string());
        }
        cursor += VISE_EXTENDED_CATALOG_PREFIX_LEN;
    }
    let mut dirs = Vec::<ViseDirectory>::new();
    let mut entries = Vec::<ViseEntry<'_>>::new();

    for index in 0..entry_count {
        let magic = range(data, cursor, 4, &format!("catalog entry {index} magic"))?;
        cursor += 4;
        if &magic[1..4] != b"VCT" {
            return Err(format!(
                "catalog entry {index} has invalid magic {:?}",
                magic
            ));
        }

        match magic[0] {
            b'D' => {
                let record = range(
                    data,
                    cursor,
                    VISE_DIRECTORY_RECORD_LEN,
                    &format!("directory {index} record"),
                )?;
                cursor += VISE_DIRECTORY_RECORD_LEN;
                let parent = read_u16(record, 68, "directory parent")? as usize;
                let name_len = record[76] as usize;
                if version == VISE_VERSION_36_LITE {
                    range(data, cursor, 6, "VISE 3.6 directory extension")?;
                    cursor += 6;
                } else if version == VISE_VERSION_EXTENDED_CATALOG {
                    range(
                        data,
                        cursor,
                        VISE_EXTENDED_DIRECTORY_SUFFIX_LEN,
                        "extended directory suffix",
                    )?;
                    cursor += VISE_EXTENDED_DIRECTORY_SUFFIX_LEN;
                }
                let name = decode_catalog_name(data, &mut cursor, name_len, "directory name")?;
                let path = child_path(&dirs, parent, &name, "directory")?;
                dirs.push(ViseDirectory { path });
            }
            b'F' => {
                let record = range(
                    data,
                    cursor,
                    VISE_FILE_RECORD_LEN,
                    &format!("file {index} record"),
                )?;
                cursor += VISE_FILE_RECORD_LEN;
                let parent = read_u16(record, 92, "file parent")? as usize;
                let packed_offset = read_u32(record, 96, "file payload offset")? as usize;
                let declared_data_packed_len = read_u32(record, 64, "packed data length")? as usize;
                let data_unpacked_len = read_u32(record, 68, "data length")? as usize;
                let declared_rsrc_packed_len =
                    read_u32(record, 72, "packed resource length")? as usize;
                let rsrc_unpacked_len = read_u32(record, 76, "resource length")? as usize;
                let extended_unpacked_offset = if version == VISE_VERSION_EXTENDED_CATALOG {
                    read_u32(record, 100, "unpacked payload offset")? as usize
                } else {
                    0
                };
                let name_len = record[118] as usize;
                let mut file_type = [0; 4];
                file_type.copy_from_slice(&record[40..44]);
                let mut creator = [0; 4];
                creator.copy_from_slice(&record[44..48]);
                if version == VISE_VERSION_EXTENDED_CATALOG {
                    range(
                        data,
                        cursor,
                        VISE_EXTENDED_FILE_SUFFIX_LEN,
                        "extended file suffix",
                    )?;
                    cursor += VISE_EXTENDED_FILE_SUFFIX_LEN;
                }
                let name = decode_catalog_name(data, &mut cursor, name_len, "file name")?;
                let path = child_path(&dirs, parent, &name, "file")?;
                // In classic VISE resource-only records, the resource stream
                // starts at the file payload offset and uses the first packed
                // length. Gridz uses this layout for controls and graphics.
                let resource_only_classic = version != VISE_VERSION_EXTENDED_CATALOG
                    && data_unpacked_len == 0
                    && rsrc_unpacked_len != 0
                    && declared_data_packed_len != 0;
                let (data_packed_len, rsrc_packed_len) = if resource_only_classic {
                    (0, declared_data_packed_len)
                } else {
                    (declared_data_packed_len, declared_rsrc_packed_len)
                };
                let unpacked_offset = if resource_only_classic {
                    read_u32(record, 104, "grouped resource offset")? as usize
                } else {
                    extended_unpacked_offset
                };
                let data_packed = range(
                    data,
                    packed_offset,
                    data_packed_len,
                    &format!("{path} data stream"),
                )?;
                let rsrc_offset = packed_offset
                    .checked_add(data_packed_len)
                    .ok_or_else(|| format!("{path} resource offset overflow"))?;
                let rsrc_packed = range(
                    data,
                    rsrc_offset,
                    rsrc_packed_len,
                    &format!("{path} resource stream"),
                )?;
                entries.push(ViseEntry {
                    path,
                    file_type,
                    creator,
                    data_packed,
                    rsrc_packed,
                    data_packed_offset: packed_offset,
                    rsrc_packed_offset: rsrc_offset,
                    unpacked_offset: if version == VISE_VERSION_EXTENDED_CATALOG
                        && unpacked_offset >= data_unpacked_len
                    {
                        0
                    } else {
                        unpacked_offset
                    },
                    data_unpacked_len,
                    rsrc_unpacked_len,
                });
            }
            kind => {
                return Err(format!(
                    "catalog entry {index} has unsupported kind 0x{kind:02X}"
                ));
            }
        }
    }

    Ok(ViseArchive {
        dirs: dirs.into_iter().map(|dir| dir.path).collect(),
        entries,
    })
}

pub fn decode_vise_fork(packed: &[u8], expected_len: usize) -> Result<Vec<u8>, String> {
    if expected_len == 0 {
        return Ok(Vec::new());
    }
    if packed.is_empty() {
        return Err(format!(
            "compressed stream is empty but declares {expected_len} output bytes"
        ));
    }

    let mut transformed = packed.to_vec();
    for pair in transformed.chunks_exact_mut(2) {
        pair.swap(0, 1);
    }
    for byte in &mut transformed {
        *byte = VISE_DEOBFUSCATION_TABLE[*byte as usize];
    }

    // VISE's `Dcmp` resource emits a headerless DEFLATE stream. ScummVM notes
    // that stored blocks are word-aligned by VISE; compressible application
    // payloads use ordinary blocks accepted by the standard decoder.
    let mut decoder = DeflateDecoder::new(transformed.as_slice());
    let mut output = Vec::with_capacity(expected_len);
    // VISE catalog sizes are authoritative. Its original decompressor, and
    // ScummVM's compatible implementation, stop after filling exactly this
    // output buffer even if the encoded bitstream can yield more bytes.
    let standard_result = decoder.read_to_end(&mut output);
    if standard_result.is_ok() && output.len() >= expected_len {
        output.truncate(expected_len);
        return Ok(output);
    }

    let concatenated_result = decode_concatenated_deflate(&transformed, expected_len);
    if let Ok(mut concatenated) = concatenated_result {
        concatenated.truncate(expected_len);
        return Ok(concatenated);
    }
    let concatenated_error = concatenated_result.unwrap_err();

    // Installer VISE's decompressor consumes 16-bit words. Unlike RFC 1951,
    // its stored blocks skip to the next word boundary rather than merely the
    // next byte boundary. Standard DEFLATE handles virtually every fork; this
    // fallback covers grouped streams that contain those word-aligned blocks.
    decode_vise_word_aligned_deflate(&transformed, expected_len).map_err(|error| {
        let standard = standard_result.map_or_else(
            |decode_error| format!("standard DEFLATE failed: {decode_error}"),
            |_| format!("standard DEFLATE produced only {} bytes", output.len()),
        );
        format!(
            "{standard}; concatenated DEFLATE failed: {concatenated_error}; VISE DEFLATE failed: {error}"
        )
    })
}

fn decode_concatenated_deflate(data: &[u8], required_len: usize) -> Result<Vec<u8>, String> {
    let mut output = Vec::with_capacity(required_len);
    let mut offset = 0usize;
    while offset < data.len() && output.len() < required_len {
        let mut decoder = DeflateDecoder::new(&data[offset..]);
        let before = output.len();
        decoder
            .read_to_end(&mut output)
            .map_err(|error| format!("member at 0x{offset:X}: {error}"))?;
        let consumed = decoder.total_in() as usize;
        if consumed == 0 {
            return Err(format!("member at 0x{offset:X} consumed no input"));
        }
        offset = offset
            .checked_add(consumed)
            .ok_or_else(|| "concatenated DEFLATE offset overflow".to_string())?;
        // Each independently-finalized stream starts on the next 16-bit word,
        // matching the VISE decompressor's word-at-a-time input contract.
        offset = offset
            .checked_add(1)
            .ok_or_else(|| "concatenated DEFLATE alignment overflow".to_string())?
            & !1;
        if output.len() == before && output.len() < required_len {
            return Err(format!("member at 0x{offset:X} produced no output"));
        }
    }
    if output.len() < required_len {
        return Err(format!(
            "concatenated streams produced {} bytes, expected at least {required_len}",
            output.len()
        ));
    }
    Ok(output)
}

fn decode_vise_word_aligned_deflate(data: &[u8], required_len: usize) -> Result<Vec<u8>, String> {
    let mut bits = DeflateBitReader::new(data);
    let mut output = Vec::with_capacity(required_len);

    loop {
        let is_final = bits.read_bits(1)? != 0;
        let block_type = bits.read_bits(2)?;
        match block_type {
            0 => {
                bits.align_to_word()?;
                let len = bits.read_bits(16)? as u16;
                let inverse_len = bits.read_bits(16)? as u16;
                if len != !inverse_len {
                    return Err(format!(
                        "stored block length check failed ({len} != !{inverse_len})"
                    ));
                }
                for _ in 0..len {
                    output.push(bits.read_bits(8)? as u8);
                    if output.len() == required_len {
                        return Ok(output);
                    }
                }
            }
            1 => {
                let (literal_lengths, distance_lengths) = fixed_huffman_lengths();
                decode_huffman_block(
                    &mut bits,
                    &mut output,
                    required_len,
                    &HuffmanTree::new(&literal_lengths)?,
                    &HuffmanTree::new(&distance_lengths)?,
                )?;
            }
            2 => {
                let (literal_lengths, distance_lengths) = dynamic_huffman_lengths(&mut bits)?;
                decode_huffman_block(
                    &mut bits,
                    &mut output,
                    required_len,
                    &HuffmanTree::new(&literal_lengths)?,
                    &HuffmanTree::new(&distance_lengths)?,
                )?;
            }
            _ => return Err("reserved DEFLATE block type".to_string()),
        }
        if output.len() >= required_len {
            output.truncate(required_len);
            return Ok(output);
        }
        if is_final {
            return Err(format!(
                "stream ended after {} bytes, expected at least {required_len}",
                output.len()
            ));
        }
    }
}

struct DeflateBitReader<'a> {
    data: &'a [u8],
    bit_pos: usize,
}

impl<'a> DeflateBitReader<'a> {
    fn new(data: &'a [u8]) -> Self {
        Self { data, bit_pos: 0 }
    }

    fn read_bits(&mut self, count: u8) -> Result<u32, String> {
        let end = self
            .bit_pos
            .checked_add(count as usize)
            .ok_or_else(|| "DEFLATE bit offset overflow".to_string())?;
        if end > self.data.len() * 8 {
            return Err("DEFLATE input truncated".to_string());
        }
        let mut value = 0u32;
        for bit_index in 0..count {
            let byte = self.data[self.bit_pos / 8];
            let bit = (byte >> (self.bit_pos % 8)) & 1;
            value |= u32::from(bit) << bit_index;
            self.bit_pos += 1;
        }
        Ok(value)
    }

    fn align_to_word(&mut self) -> Result<(), String> {
        self.bit_pos = self
            .bit_pos
            .checked_add(15)
            .ok_or_else(|| "DEFLATE word alignment overflow".to_string())?
            & !15;
        if self.bit_pos > self.data.len() * 8 {
            return Err("DEFLATE input truncated at word boundary".to_string());
        }
        Ok(())
    }
}

struct HuffmanTree {
    codes_by_len: Vec<Vec<(u16, u16)>>,
    max_len: u8,
}

impl HuffmanTree {
    fn new(lengths: &[u8]) -> Result<Self, String> {
        let max_len = lengths.iter().copied().max().unwrap_or(0);
        if max_len == 0 || max_len > 15 {
            return Err(format!("invalid Huffman maximum code length {max_len}"));
        }
        let mut counts = vec![0u16; max_len as usize + 1];
        for &len in lengths {
            if len != 0 {
                counts[len as usize] += 1;
            }
        }
        let mut next_code = vec![0u16; max_len as usize + 1];
        let mut code = 0u16;
        for len in 1..=max_len as usize {
            code = (code + counts[len - 1]) << 1;
            next_code[len] = code;
        }
        let mut codes_by_len = vec![Vec::new(); max_len as usize + 1];
        for (symbol, &len) in lengths.iter().enumerate() {
            if len == 0 {
                continue;
            }
            let canonical = next_code[len as usize];
            next_code[len as usize] += 1;
            codes_by_len[len as usize].push((reverse_low_bits(canonical, len), symbol as u16));
        }
        Ok(Self {
            codes_by_len,
            max_len,
        })
    }

    fn decode(&self, bits: &mut DeflateBitReader<'_>) -> Result<u16, String> {
        let mut code = 0u16;
        for len in 1..=self.max_len {
            code |= (bits.read_bits(1)? as u16) << (len - 1);
            if let Some((_, symbol)) = self.codes_by_len[len as usize]
                .iter()
                .find(|(candidate, _)| *candidate == code)
            {
                return Ok(*symbol);
            }
        }
        Err("invalid Huffman code".to_string())
    }
}

fn reverse_low_bits(mut code: u16, len: u8) -> u16 {
    let mut reversed = 0u16;
    for _ in 0..len {
        reversed = (reversed << 1) | (code & 1);
        code >>= 1;
    }
    reversed
}

fn fixed_huffman_lengths() -> (Vec<u8>, Vec<u8>) {
    let mut literal_lengths = vec![0u8; 288];
    literal_lengths[0..144].fill(8);
    literal_lengths[144..256].fill(9);
    literal_lengths[256..280].fill(7);
    literal_lengths[280..288].fill(8);
    (literal_lengths, vec![5; 32])
}

fn dynamic_huffman_lengths(bits: &mut DeflateBitReader<'_>) -> Result<(Vec<u8>, Vec<u8>), String> {
    const CODE_LENGTH_ORDER: [usize; 19] = [
        16, 17, 18, 0, 8, 7, 9, 6, 10, 5, 11, 4, 12, 3, 13, 2, 14, 1, 15,
    ];
    let literal_count = bits.read_bits(5)? as usize + 257;
    let distance_count = bits.read_bits(5)? as usize + 1;
    let code_length_count = bits.read_bits(4)? as usize + 4;
    let mut code_lengths = vec![0u8; 19];
    for &index in &CODE_LENGTH_ORDER[..code_length_count] {
        code_lengths[index] = bits.read_bits(3)? as u8;
    }
    let code_length_tree = HuffmanTree::new(&code_lengths)?;
    let total = literal_count + distance_count;
    let mut lengths = Vec::with_capacity(total);
    while lengths.len() < total {
        match code_length_tree.decode(bits)? {
            symbol @ 0..=15 => lengths.push(symbol as u8),
            16 => {
                let previous = *lengths
                    .last()
                    .ok_or_else(|| "repeat code 16 has no previous length".to_string())?;
                let repeat = bits.read_bits(2)? as usize + 3;
                lengths.extend(std::iter::repeat_n(previous, repeat));
            }
            17 => {
                let repeat = bits.read_bits(3)? as usize + 3;
                lengths.extend(std::iter::repeat_n(0, repeat));
            }
            18 => {
                let repeat = bits.read_bits(7)? as usize + 11;
                lengths.extend(std::iter::repeat_n(0, repeat));
            }
            symbol => return Err(format!("invalid code-length symbol {symbol}")),
        }
        if lengths.len() > total {
            return Err("dynamic Huffman lengths exceed declared count".to_string());
        }
    }
    Ok((
        lengths[..literal_count].to_vec(),
        lengths[literal_count..].to_vec(),
    ))
}

fn decode_huffman_block(
    bits: &mut DeflateBitReader<'_>,
    output: &mut Vec<u8>,
    required_len: usize,
    literal_tree: &HuffmanTree,
    distance_tree: &HuffmanTree,
) -> Result<(), String> {
    const LENGTH_BASE: [usize; 29] = [
        3, 4, 5, 6, 7, 8, 9, 10, 11, 13, 15, 17, 19, 23, 27, 31, 35, 43, 51, 59, 67, 83, 99, 115,
        131, 163, 195, 227, 258,
    ];
    const LENGTH_EXTRA: [u8; 29] = [
        0, 0, 0, 0, 0, 0, 0, 0, 1, 1, 1, 1, 2, 2, 2, 2, 3, 3, 3, 3, 4, 4, 4, 4, 5, 5, 5, 5, 0,
    ];
    const DISTANCE_BASE: [usize; 30] = [
        1, 2, 3, 4, 5, 7, 9, 13, 17, 25, 33, 49, 65, 97, 129, 193, 257, 385, 513, 769, 1025, 1537,
        2049, 3073, 4097, 6145, 8193, 12289, 16385, 24577,
    ];
    const DISTANCE_EXTRA: [u8; 30] = [
        0, 0, 0, 0, 1, 1, 2, 2, 3, 3, 4, 4, 5, 5, 6, 6, 7, 7, 8, 8, 9, 9, 10, 10, 11, 11, 12, 12,
        13, 13,
    ];

    loop {
        match literal_tree.decode(bits)? {
            literal @ 0..=255 => {
                output.push(literal as u8);
                if output.len() == required_len {
                    return Ok(());
                }
            }
            256 => return Ok(()),
            length_symbol @ 257..=285 => {
                let length_index = (length_symbol - 257) as usize;
                let length = LENGTH_BASE[length_index]
                    + bits.read_bits(LENGTH_EXTRA[length_index])? as usize;
                let distance_symbol = distance_tree.decode(bits)? as usize;
                if distance_symbol >= DISTANCE_BASE.len() {
                    return Err(format!("invalid distance symbol {distance_symbol}"));
                }
                let distance = DISTANCE_BASE[distance_symbol]
                    + bits.read_bits(DISTANCE_EXTRA[distance_symbol])? as usize;
                if distance == 0 || distance > output.len() {
                    return Err(format!(
                        "invalid back-reference distance {distance} at output {}",
                        output.len()
                    ));
                }
                for _ in 0..length {
                    let byte = output[output.len() - distance];
                    output.push(byte);
                    if output.len() == required_len {
                        return Ok(());
                    }
                }
            }
            symbol => return Err(format!("invalid literal/length symbol {symbol}")),
        }
    }
}

fn child_path(
    dirs: &[ViseDirectory],
    parent: usize,
    name: &str,
    kind: &str,
) -> Result<String, String> {
    validate_component(name, kind)?;
    if parent == 0 {
        return Ok(name.to_string());
    }
    let parent_dir = dirs
        .get(parent - 1)
        .ok_or_else(|| format!("{kind} {name:?} has invalid parent directory {parent}"))?;
    Ok(format!("{}/{}", parent_dir.path, name))
}

fn validate_component(name: &str, kind: &str) -> Result<(), String> {
    if name.is_empty()
        || name == "."
        || name == ".."
        || name.contains('/')
        || name.contains(':')
        || name.contains('\0')
    {
        return Err(format!("unsafe VISE {kind} component {name:?}"));
    }
    Ok(())
}

fn decode_catalog_name(
    data: &[u8],
    cursor: &mut usize,
    len: usize,
    label: &str,
) -> Result<String, String> {
    let bytes = range(data, *cursor, len, label)?;
    *cursor = cursor
        .checked_add(len)
        .ok_or_else(|| format!("{label} offset overflow"))?;
    Ok(decode_mac_roman(bytes))
}

fn range<'a>(data: &'a [u8], offset: usize, len: usize, label: &str) -> Result<&'a [u8], String> {
    let end = offset
        .checked_add(len)
        .ok_or_else(|| format!("{label} range overflow"))?;
    data.get(offset..end).ok_or_else(|| {
        format!(
            "{label} range 0x{offset:X}..0x{end:X} exceeds len {}",
            data.len()
        )
    })
}

fn read_u16(data: &[u8], offset: usize, label: &str) -> Result<u16, String> {
    Ok(u16::from_be_bytes(
        range(data, offset, 2, label)?.try_into().unwrap(),
    ))
}

fn read_u32(data: &[u8], offset: usize, label: &str) -> Result<u32, String> {
    Ok(u32::from_be_bytes(
        range(data, offset, 4, label)?.try_into().unwrap(),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use flate2::{write::DeflateEncoder, Compression};
    use std::io::Write;

    fn encode_vise_fork(bytes: &[u8]) -> Vec<u8> {
        let mut encoder = DeflateEncoder::new(Vec::new(), Compression::default());
        encoder.write_all(bytes).unwrap();
        let mut encoded = encoder.finish().unwrap();
        let mut inverse = [0u8; 256];
        for (index, decoded) in VISE_DEOBFUSCATION_TABLE.iter().copied().enumerate() {
            inverse[decoded as usize] = index as u8;
        }
        for byte in &mut encoded {
            *byte = inverse[*byte as usize];
        }
        for pair in encoded.chunks_exact_mut(2) {
            pair.swap(0, 1);
        }
        encoded
    }

    #[test]
    fn parses_catalog_paths_and_decodes_both_forks() {
        let data_fork = b"installed application data";
        let resource_fork = b"installed application resources";
        let packed_data = encode_vise_fork(data_fork);
        let packed_rsrc = encode_vise_fork(resource_fork);
        let payload_offset = VISE_HEADER_LEN;
        let catalog_offset = payload_offset + packed_data.len() + packed_rsrc.len();
        let mut archive = vec![0u8; VISE_HEADER_LEN];
        archive[0..4].copy_from_slice(VISE_MAGIC);
        archive[16..20].copy_from_slice(&VISE_VERSION_35_LITE.to_be_bytes());
        archive[36..40].copy_from_slice(&(catalog_offset as u32).to_be_bytes());
        archive.extend_from_slice(&packed_data);
        archive.extend_from_slice(&packed_rsrc);
        let mut catalog = [0u8; VISE_CATALOG_HEADER_LEN];
        catalog[0..4].copy_from_slice(VISE_CATALOG_MAGIC);
        catalog[16..18].copy_from_slice(&2u16.to_be_bytes());
        archive.extend_from_slice(&catalog);
        archive.extend_from_slice(b"DVCT");
        let mut directory = [0u8; VISE_DIRECTORY_RECORD_LEN];
        directory[76] = 4;
        archive.extend_from_slice(&directory);
        archive.extend_from_slice(b"Game");
        archive.extend_from_slice(b"FVCT");
        let mut file = [0u8; VISE_FILE_RECORD_LEN];
        file[40..44].copy_from_slice(b"APPL");
        file[44..48].copy_from_slice(b"TEST");
        file[64..68].copy_from_slice(&(packed_data.len() as u32).to_be_bytes());
        file[68..72].copy_from_slice(&(data_fork.len() as u32).to_be_bytes());
        file[72..76].copy_from_slice(&(packed_rsrc.len() as u32).to_be_bytes());
        file[76..80].copy_from_slice(&(resource_fork.len() as u32).to_be_bytes());
        file[92..94].copy_from_slice(&1u16.to_be_bytes());
        file[96..100].copy_from_slice(&(payload_offset as u32).to_be_bytes());
        file[100..104].copy_from_slice(&346u32.to_be_bytes());
        file[118] = 7;
        archive.extend_from_slice(&file);
        archive.extend_from_slice(b"Runtime");

        let parsed = parse_vise(&archive).unwrap().unwrap();
        assert_eq!(parsed.dirs, ["Game"]);
        assert_eq!(parsed.entries.len(), 1);
        let entry = &parsed.entries[0];
        assert_eq!(entry.path, "Game/Runtime");
        assert_eq!(entry.file_type, *b"APPL");
        assert_eq!(entry.creator, *b"TEST");
        assert_eq!(entry.unpacked_offset, 0);
        assert_eq!(
            decode_vise_fork(entry.data_packed, entry.data_unpacked_len).unwrap(),
            data_fork
        );
        assert_eq!(
            decode_vise_fork(entry.rsrc_packed, entry.rsrc_unpacked_len).unwrap(),
            resource_fork
        );
    }

    #[test]
    fn parses_classic_resource_only_stream_at_the_file_payload_offset() {
        let prefix = b"earlier grouped resource";
        let resource_fork = b"resource-only VISE payload";
        let grouped_resources = [prefix.as_slice(), resource_fork.as_slice()].concat();
        let packed_rsrc = encode_vise_fork(&grouped_resources);
        let payload_offset = VISE_HEADER_LEN;
        let catalog_offset = payload_offset + packed_rsrc.len();
        let mut archive = vec![0u8; VISE_HEADER_LEN];
        archive[0..4].copy_from_slice(VISE_MAGIC);
        archive[16..20].copy_from_slice(&VISE_VERSION_35_LITE.to_be_bytes());
        archive[36..40].copy_from_slice(&(catalog_offset as u32).to_be_bytes());
        archive.extend_from_slice(&packed_rsrc);

        let mut catalog = [0u8; VISE_CATALOG_HEADER_LEN];
        catalog[0..4].copy_from_slice(VISE_CATALOG_MAGIC);
        catalog[16..18].copy_from_slice(&1u16.to_be_bytes());
        archive.extend_from_slice(&catalog);
        archive.extend_from_slice(b"FVCT");
        let mut file = [0u8; VISE_FILE_RECORD_LEN];
        file[40..44].copy_from_slice(b"RSRC");
        file[44..48].copy_from_slice(b"TEST");
        file[64..68].copy_from_slice(&(packed_rsrc.len() as u32).to_be_bytes());
        file[72..76].copy_from_slice(&(grouped_resources.len() as u32).to_be_bytes());
        file[76..80].copy_from_slice(&(resource_fork.len() as u32).to_be_bytes());
        file[96..100].copy_from_slice(&(payload_offset as u32).to_be_bytes());
        file[104..108].copy_from_slice(&(prefix.len() as u32).to_be_bytes());
        file[118] = 7;
        archive.extend_from_slice(&file);
        archive.extend_from_slice(b"Control");

        let parsed = parse_vise(&archive).unwrap().unwrap();
        let entry = &parsed.entries[0];
        assert!(entry.data_packed.is_empty());
        assert_eq!(entry.rsrc_packed, packed_rsrc);
        assert_eq!(entry.unpacked_offset, prefix.len());
        let decoded = decode_vise_fork(
            entry.rsrc_packed,
            entry.unpacked_offset + entry.rsrc_unpacked_len,
        )
        .unwrap();
        assert_eq!(
            &decoded[entry.unpacked_offset..entry.unpacked_offset + entry.rsrc_unpacked_len],
            resource_fork
        );
    }

    #[test]
    fn parses_extended_catalog_records() {
        let data_fork = b"extended catalog payload";
        let packed_data = encode_vise_fork(data_fork);
        let payload_offset = VISE_HEADER_LEN;
        let catalog_offset = payload_offset + packed_data.len();
        let mut archive = vec![0u8; VISE_HEADER_LEN];
        archive[0..4].copy_from_slice(VISE_MAGIC);
        archive[16..20].copy_from_slice(&VISE_VERSION_EXTENDED_CATALOG.to_be_bytes());
        archive[36..40].copy_from_slice(&(catalog_offset as u32).to_be_bytes());
        archive.extend_from_slice(&packed_data);

        let mut catalog = [0u8; VISE_CATALOG_HEADER_LEN];
        catalog[0..4].copy_from_slice(VISE_CATALOG_MAGIC);
        catalog[16..18].copy_from_slice(&2u16.to_be_bytes());
        archive.extend_from_slice(&catalog);
        let mut prefix = [0u8; VISE_EXTENDED_CATALOG_PREFIX_LEN];
        prefix[0..4].copy_from_slice(b"PACK");
        archive.extend_from_slice(&prefix);

        archive.extend_from_slice(b"DVCT");
        let mut directory = [0u8; VISE_DIRECTORY_RECORD_LEN];
        directory[76] = 4;
        archive.extend_from_slice(&directory);
        archive.extend_from_slice(&[0u8; VISE_EXTENDED_DIRECTORY_SUFFIX_LEN]);
        archive.extend_from_slice(b"Game");

        archive.extend_from_slice(b"FVCT");
        let mut file = [0u8; VISE_FILE_RECORD_LEN];
        file[40..44].copy_from_slice(b"APPL");
        file[44..48].copy_from_slice(b"TEST");
        file[64..68].copy_from_slice(&(packed_data.len() as u32).to_be_bytes());
        file[68..72].copy_from_slice(&(data_fork.len() as u32).to_be_bytes());
        file[92..94].copy_from_slice(&1u16.to_be_bytes());
        file[96..100].copy_from_slice(&(payload_offset as u32).to_be_bytes());
        file[100..104].copy_from_slice(&0xfffc_0000u32.to_be_bytes());
        file[118] = 7;
        archive.extend_from_slice(&file);
        archive.extend_from_slice(&[0u8; VISE_EXTENDED_FILE_SUFFIX_LEN]);
        archive.extend_from_slice(b"Runtime");

        let parsed = parse_vise(&archive).unwrap().unwrap();
        assert_eq!(parsed.dirs, ["Game"]);
        assert_eq!(parsed.entries.len(), 1);
        let entry = &parsed.entries[0];
        assert_eq!(entry.path, "Game/Runtime");
        assert_eq!(entry.unpacked_offset, 0);
        assert_eq!(
            decode_vise_fork(entry.data_packed, entry.data_unpacked_len).unwrap(),
            data_fork
        );
    }

    #[test]
    fn rejects_traversal_in_catalog_components() {
        assert_eq!(
            validate_component("..", "file").unwrap_err(),
            "unsafe VISE file component \"..\""
        );
        assert!(validate_component("Folder/Game", "directory").is_err());
        assert!(validate_component("Volume:Game", "file").is_err());
    }
}
