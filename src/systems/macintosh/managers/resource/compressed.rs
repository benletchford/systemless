//! Classic Mac OS compressed resource decompression.
//!
//! System 7 resource maps can mark individual resources as compressed. The
//! on-disk data then starts with a common compressed-resource header followed
//! by bytes for a `dcmp` decompressor. The Resource Manager expands these
//! transparently before clients see resource data, so Systemless needs to do
//! the same before CODE resources reach the Segment Loader.

pub(crate) const COMPRESSED_RESOURCE_ATTR: u8 = 0x01;
const COMPRESSED_MAGIC: &[u8; 4] = b"\xA8\x9Fer";
const COMPRESSED_TYPE_8: u16 = 0x0801;
const COMPRESSED_TYPE_9: u16 = 0x0901;
const HEADER_LEN: usize = 0x12;

const DCMP0_TABLE: [u8; 358] = [
    0x00, 0x00, 0x4E, 0xBA, 0x00, 0x08, 0x4E, 0x75, 0x00, 0x0C, 0x4E, 0xAD, 0x20, 0x53, 0x2F, 0x0B,
    0x61, 0x00, 0x00, 0x10, 0x70, 0x00, 0x2F, 0x00, 0x48, 0x6E, 0x20, 0x50, 0x20, 0x6E, 0x2F, 0x2E,
    0xFF, 0xFC, 0x48, 0xE7, 0x3F, 0x3C, 0x00, 0x04, 0xFF, 0xF8, 0x2F, 0x0C, 0x20, 0x06, 0x4E, 0xED,
    0x4E, 0x56, 0x20, 0x68, 0x4E, 0x5E, 0x00, 0x01, 0x58, 0x8F, 0x4F, 0xEF, 0x00, 0x02, 0x00, 0x18,
    0x60, 0x00, 0xFF, 0xFF, 0x50, 0x8F, 0x4E, 0x90, 0x00, 0x06, 0x26, 0x6E, 0x00, 0x14, 0xFF, 0xF4,
    0x4C, 0xEE, 0x00, 0x0A, 0x00, 0x0E, 0x41, 0xEE, 0x4C, 0xDF, 0x48, 0xC0, 0xFF, 0xF0, 0x2D, 0x40,
    0x00, 0x12, 0x30, 0x2E, 0x70, 0x01, 0x2F, 0x28, 0x20, 0x54, 0x67, 0x00, 0x00, 0x20, 0x00, 0x1C,
    0x20, 0x5F, 0x18, 0x00, 0x26, 0x6F, 0x48, 0x78, 0x00, 0x16, 0x41, 0xFA, 0x30, 0x3C, 0x28, 0x40,
    0x72, 0x00, 0x28, 0x6E, 0x20, 0x0C, 0x66, 0x00, 0x20, 0x6B, 0x2F, 0x07, 0x55, 0x8F, 0x00, 0x28,
    0xFF, 0xFE, 0xFF, 0xEC, 0x22, 0xD8, 0x20, 0x0B, 0x00, 0x0F, 0x59, 0x8F, 0x2F, 0x3C, 0xFF, 0x00,
    0x01, 0x18, 0x81, 0xE1, 0x4A, 0x00, 0x4E, 0xB0, 0xFF, 0xE8, 0x48, 0xC7, 0x00, 0x03, 0x00, 0x22,
    0x00, 0x07, 0x00, 0x1A, 0x67, 0x06, 0x67, 0x08, 0x4E, 0xF9, 0x00, 0x24, 0x20, 0x78, 0x08, 0x00,
    0x66, 0x04, 0x00, 0x2A, 0x4E, 0xD0, 0x30, 0x28, 0x26, 0x5F, 0x67, 0x04, 0x00, 0x30, 0x43, 0xEE,
    0x3F, 0x00, 0x20, 0x1F, 0x00, 0x1E, 0xFF, 0xF6, 0x20, 0x2E, 0x42, 0xA7, 0x20, 0x07, 0xFF, 0xFA,
    0x60, 0x02, 0x3D, 0x40, 0x0C, 0x40, 0x66, 0x06, 0x00, 0x26, 0x2D, 0x48, 0x2F, 0x01, 0x70, 0xFF,
    0x60, 0x04, 0x18, 0x80, 0x4A, 0x40, 0x00, 0x40, 0x00, 0x2C, 0x2F, 0x08, 0x00, 0x11, 0xFF, 0xE4,
    0x21, 0x40, 0x26, 0x40, 0xFF, 0xF2, 0x42, 0x6E, 0x4E, 0xB9, 0x3D, 0x7C, 0x00, 0x38, 0x00, 0x0D,
    0x60, 0x06, 0x42, 0x2E, 0x20, 0x3C, 0x67, 0x0C, 0x2D, 0x68, 0x66, 0x08, 0x4A, 0x2E, 0x4A, 0xAE,
    0x00, 0x2E, 0x48, 0x40, 0x22, 0x5F, 0x22, 0x00, 0x67, 0x0A, 0x30, 0x07, 0x42, 0x67, 0x00, 0x32,
    0x20, 0x28, 0x00, 0x09, 0x48, 0x7A, 0x02, 0x00, 0x2F, 0x2B, 0x00, 0x05, 0x22, 0x6E, 0x66, 0x02,
    0xE5, 0x80, 0x67, 0x0E, 0x66, 0x0A, 0x00, 0x50, 0x3E, 0x00, 0x66, 0x0C, 0x2E, 0x00, 0xFF, 0xEE,
    0x20, 0x6D, 0x20, 0x40, 0xFF, 0xE0, 0x53, 0x40, 0x60, 0x08, 0x04, 0x80, 0x00, 0x68, 0x0B, 0x7C,
    0x44, 0x00, 0x41, 0xE8, 0x48, 0x41,
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum DecompressError {
    Truncated,
    InvalidHeader,
    UnsupportedHeaderType(u16),
    UnsupportedDecompressor(i16),
    BadChunk(u8),
    BadBackReference(usize),
    LengthMismatch { expected: usize, actual: usize },
    ExtraData,
    ValueOutOfRange,
}

pub(crate) fn decompress_if_needed(
    attrs: u8,
    data: &[u8],
) -> Result<Option<Vec<u8>>, DecompressError> {
    if attrs & COMPRESSED_RESOURCE_ATTR == 0 || !data.starts_with(COMPRESSED_MAGIC) {
        return Ok(None);
    }

    decompress_resource(data).map(Some)
}

fn decompress_resource(data: &[u8]) -> Result<Vec<u8>, DecompressError> {
    if data.len() < HEADER_LEN {
        return Err(DecompressError::Truncated);
    }
    if &data[0..4] != COMPRESSED_MAGIC {
        return Err(DecompressError::InvalidHeader);
    }

    let header_len = read_u16(data, 4) as usize;
    if header_len != HEADER_LEN || data.len() < header_len {
        return Err(DecompressError::InvalidHeader);
    }

    let header_type = read_u16(data, 6);
    let decompressed_len = read_u32(data, 8) as usize;
    match header_type {
        COMPRESSED_TYPE_8 => {
            let dcmp_id = read_i16(data, 14);
            let reserved = read_u16(data, 16);
            if reserved != 0 {
                return Err(DecompressError::InvalidHeader);
            }
            if dcmp_id != 0 {
                return Err(DecompressError::UnsupportedDecompressor(dcmp_id));
            }
            decompress_dcmp0(&data[header_len..], decompressed_len)
        }
        COMPRESSED_TYPE_9 => {
            let dcmp_id = read_i16(data, 12);
            if dcmp_id != 3 {
                return Err(DecompressError::UnsupportedDecompressor(dcmp_id));
            }
            decompress_dcmp3(&data[header_len..], decompressed_len)
        }
        other => Err(DecompressError::UnsupportedHeaderType(other)),
    }
}

// System 7's dcmp 3 stores literal runs and overlapping backward copies in an
// MSB-first bitstream. The dynamic offset code depends on bytes already output.
// Format reference: ResourceDASM's System3 decompressor (MIT license).
fn decompress_dcmp3(data: &[u8], decompressed_len: usize) -> Result<Vec<u8>, DecompressError> {
    struct Bits<'a> {
        data: &'a [u8],
        pos: usize,
    }
    impl Bits<'_> {
        fn read(&mut self, count: usize) -> Result<usize, DecompressError> {
            let end = self
                .pos
                .checked_add(count)
                .ok_or(DecompressError::Truncated)?;
            if end > self.data.len() * 8 {
                return Err(DecompressError::Truncated);
            }
            let mut value = 0;
            while self.pos < end {
                value =
                    (value << 1) | ((self.data[self.pos / 8] >> (7 - self.pos % 8)) & 1) as usize;
                self.pos += 1;
            }
            Ok(value)
        }
        fn run_length(&mut self) -> Result<usize, DecompressError> {
            let mut prefix = 0;
            while prefix < 10 && self.read(1)? == 1 {
                prefix += 1;
            }
            let (base, bits) = match prefix {
                0 => (0, 1),
                1 => {
                    return Ok(if self.read(1)? == 0 {
                        2
                    } else {
                        self.read(1)? + 3
                    })
                }
                2 => {
                    return Ok(if self.read(1)? == 0 {
                        self.read(1)? + 5
                    } else {
                        self.read(2)? + 7
                    })
                }
                3 => (11, 3),
                4 => (19, 3),
                5 => (27, 5),
                6 => (59, 6),
                7 => (123, 7),
                8 => (251, 8),
                9 => (507, 9),
                _ => (1019, 10),
            };
            Ok(base + self.read(bits)?)
        }
        fn literal_length(&mut self) -> Result<usize, DecompressError> {
            if self.read(1)? == 0 {
                return Ok(1);
            }
            match self.read(2)? {
                0 => Ok(2),
                1 => Ok(3),
                2 => Ok(self.read(2)? + 4),
                _ => {
                    let code = self.read(4)?;
                    if code < 8 {
                        Ok(code + 8)
                    } else if code < 12 {
                        Ok(((code - 8) << 2) + self.read(2)? + 16)
                    } else {
                        Ok(((code - 12) << 3) + self.read(3)? + 32)
                    }
                }
            }
        }
        fn offset(&mut self, available: usize) -> Result<usize, DecompressError> {
            if available == 0 {
                return Err(DecompressError::BadBackReference(0));
            }
            let bucket = [
                10, 20, 40, 80, 160, 672, 1000, 2688, 5376, 10752, 21504, 43008, 70000, 172032,
            ];
            let k = bucket
                .iter()
                .position(|&limit| available <= limit)
                .unwrap_or(14);
            if self.read(1)? == 0 {
                return Ok(self.read(k)? + 1);
            }
            if self.read(1)? == 0 {
                return Ok(self.read(k + 2)? + (1 << k) + 1);
            }
            let base = 5 * (1 << k) + 1;
            let range = available.saturating_sub(base) + 1;
            let mut bits = (usize::BITS - (range - 1).leading_zeros()) as usize;
            // Preserve the original dcmp 3 decoder's two irregular thresholds.
            if k == 7 && (0x285..=0x288).contains(&available) {
                bits = 4;
            }
            if k == 7 && (0x66D..=0x680).contains(&available) {
                bits = 11;
            }
            if k == 14 && (0x200D..=0x14080).contains(&available) {
                bits = 8;
            }
            Ok(base + self.read(bits)?)
        }
    }

    let mut bits = Bits { data, pos: 0 };
    let mut out = Vec::with_capacity(decompressed_len);
    let mut literal_allowed = true;
    while out.len() < decompressed_len {
        let mut count = bits.run_length()?;
        if count == 0 && literal_allowed {
            count = bits.literal_length()?;
            literal_allowed = count == 63;
            if out.len() + count > decompressed_len {
                return Err(DecompressError::LengthMismatch {
                    expected: decompressed_len,
                    actual: out.len() + count,
                });
            }
            for _ in 0..count {
                out.push(bits.read(8)? as u8);
            }
        } else {
            count += if literal_allowed { 2 } else { 3 };
            literal_allowed = true;
            let offset = bits.offset(out.len())?;
            if offset == 0 || offset > out.len() {
                return Err(DecompressError::BadBackReference(offset));
            }
            if out.len() + count > decompressed_len {
                return Err(DecompressError::LengthMismatch {
                    expected: decompressed_len,
                    actual: out.len() + count,
                });
            }
            for _ in 0..count {
                let value = out[out.len() - offset];
                out.push(value);
            }
        }
    }
    Ok(out)
}

fn decompress_dcmp0(data: &[u8], decompressed_len: usize) -> Result<Vec<u8>, DecompressError> {
    let mut r = Reader::new(data);
    let mut out = Vec::with_capacity(decompressed_len);
    let mut stored_literals: Vec<Vec<u8>> = Vec::new();

    loop {
        let tag = r.read_u8()?;
        match tag {
            0x00..=0x1F => {
                let count_div2 = if tag == 0x00 || tag == 0x10 {
                    r.read_u8()? as usize
                } else {
                    (tag & 0x0F) as usize
                };
                let count = count_div2
                    .checked_mul(2)
                    .ok_or(DecompressError::ValueOutOfRange)?;
                let literal = r.read_exact(count)?.to_vec();
                if tag >= 0x10 {
                    stored_literals.push(literal.clone());
                }
                append_chunk(&mut out, &literal, decompressed_len);
            }
            0x20 | 0x21 => {
                let lo = r.read_u8()? as usize;
                let index = 0x28 + (((tag - 0x20) as usize) << 8) + lo;
                let literal = stored_literals
                    .get(index)
                    .ok_or(DecompressError::BadBackReference(index))?;
                append_chunk(&mut out, literal, decompressed_len);
            }
            0x22 => {
                let index = 0x28 + r.read_u16()? as usize;
                let literal = stored_literals
                    .get(index)
                    .ok_or(DecompressError::BadBackReference(index))?;
                append_chunk(&mut out, literal, decompressed_len);
            }
            0x23..=0x4A => {
                let index = (tag - 0x23) as usize;
                let literal = stored_literals
                    .get(index)
                    .ok_or(DecompressError::BadBackReference(index))?;
                append_chunk(&mut out, literal, decompressed_len);
            }
            0x4B..=0xFD => {
                let index = (tag - 0x4B) as usize * 2;
                append_chunk(&mut out, &DCMP0_TABLE[index..index + 2], decompressed_len);
            }
            0xFE => decode_extended_dcmp0(&mut r, &mut out, decompressed_len)?,
            0xFF => {
                if !r.is_eof() {
                    return Err(DecompressError::ExtraData);
                }
                if out.len() != decompressed_len {
                    return Err(DecompressError::LengthMismatch {
                        expected: decompressed_len,
                        actual: out.len(),
                    });
                }
                return Ok(out);
            }
        }
    }
}

fn decode_extended_dcmp0(
    r: &mut Reader<'_>,
    out: &mut Vec<u8>,
    decompressed_len: usize,
) -> Result<(), DecompressError> {
    let kind = r.read_u8()?;
    match kind {
        0x00 => {
            let segment = r.read_var_i32()?;
            if !(0..=0xFFFF).contains(&segment) {
                return Err(DecompressError::ValueOutOfRange);
            }
            let segment = segment as u16;
            let entry_tail = [0x3F, 0x3C, (segment >> 8) as u8, segment as u8, 0xA9, 0xF0];
            append_chunk(out, &entry_tail, decompressed_len);

            let count = r.read_var_i32()?;
            if count <= 0 {
                return Err(DecompressError::ValueOutOfRange);
            }

            let mut current = r.read_var_i32()?;
            if !(0..=0xFFFF).contains(&current) {
                return Err(DecompressError::ValueOutOfRange);
            }
            append_jump_table_entry(out, current as u16, &entry_tail, decompressed_len);

            for _ in 1..count {
                let diff = r.read_var_i32()? - 6;
                current = (current + diff) & 0xFFFF;
                append_jump_table_entry(out, current as u16, &entry_tail, decompressed_len);
            }
        }
        0x01 => {
            // dcmp 0 can reconstruct a CODE jump table from its target and
            // A5 offsets. Entries are BSR.W target; JMP d16(A5).
            let mut target = r.read_var_i32()? as u16;
            let delta = r.read_var_i32()? as u16;
            let count = (r.read_var_i32()? as u32 & 0xFFFF) as usize + 1;
            let mut a5_offset = r.read_var_i32()? as u16;
            for index in 0..count {
                if index != 0 {
                    target = target.wrapping_sub(8);
                    a5_offset = if delta == 0 {
                        r.read_var_i32()? as u16
                    } else {
                        a5_offset.wrapping_add(delta)
                    };
                }
                let mut entry = [0u8; 8];
                entry[0..2].copy_from_slice(&0x6100u16.to_be_bytes());
                entry[2..4].copy_from_slice(&target.to_be_bytes());
                entry[4..6].copy_from_slice(&0x4EEDu16.to_be_bytes());
                entry[6..8].copy_from_slice(&a5_offset.to_be_bytes());
                append_chunk(out, &entry, decompressed_len);
            }
        }
        0x02 | 0x03 => {
            let byte_count = if kind == 0x02 { 1 } else { 2 };
            let value = r.read_var_i32()?;
            if value < 0 || value > if byte_count == 1 { 0xFF } else { 0xFFFF } {
                return Err(DecompressError::ValueOutOfRange);
            }
            let count = r.read_var_i32()? + 1;
            if count <= 0 {
                return Err(DecompressError::ValueOutOfRange);
            }
            let mut chunk = Vec::with_capacity(byte_count * count as usize);
            for _ in 0..count {
                if byte_count == 1 {
                    chunk.push(value as u8);
                } else {
                    chunk.extend_from_slice(&(value as u16).to_be_bytes());
                }
            }
            append_chunk(out, &chunk, decompressed_len);
        }
        0x04 => {
            let initial = r.read_var_i32()?;
            if !(-0x8000..=0x7FFF).contains(&initial) {
                return Err(DecompressError::ValueOutOfRange);
            }
            let mut current = (initial as i16) as u16;
            append_chunk(out, &current.to_be_bytes(), decompressed_len);

            let count = r.read_var_i32()?;
            if count < 0 {
                return Err(DecompressError::ValueOutOfRange);
            }
            for _ in 0..count {
                let diff = r.read_i8()? as i32;
                current = ((current as i32 + diff) & 0xFFFF) as u16;
                append_chunk(out, &current.to_be_bytes(), decompressed_len);
            }
        }
        0x05 => {
            let mut current = r.read_var_i32()? as u16;
            let count = (r.read_var_i32()? as u32 & 0xFFFF) as usize + 1;
            for index in 0..count {
                if index != 0 {
                    current = current.wrapping_add(r.read_var_i32()? as u16);
                }
                append_chunk(out, &current.to_be_bytes(), decompressed_len);
            }
        }
        0x06 => {
            let initial = r.read_var_i32()?;
            let mut current = initial as u32;
            append_chunk(out, &current.to_be_bytes(), decompressed_len);

            let count = r.read_var_i32()?;
            if count < 0 {
                return Err(DecompressError::ValueOutOfRange);
            }
            for _ in 0..count {
                let diff = r.read_var_i32()?;
                current = ((current as i64 + diff as i64) & 0xFFFF_FFFF) as u32;
                append_chunk(out, &current.to_be_bytes(), decompressed_len);
            }
        }
        other => return Err(DecompressError::BadChunk(other)),
    }
    Ok(())
}

fn append_jump_table_entry(
    out: &mut Vec<u8>,
    address: u16,
    entry_tail: &[u8; 6],
    decompressed_len: usize,
) {
    let mut entry = [0u8; 8];
    entry[0..2].copy_from_slice(&address.to_be_bytes());
    entry[2..].copy_from_slice(entry_tail);
    append_chunk(out, &entry, decompressed_len);
}

fn append_chunk(out: &mut Vec<u8>, chunk: &[u8], decompressed_len: usize) {
    if decompressed_len % 2 != 0 && out.len() + chunk.len() == decompressed_len + 1 {
        out.extend_from_slice(&chunk[..chunk.len() - 1]);
    } else {
        out.extend_from_slice(chunk);
    }
}

struct Reader<'a> {
    data: &'a [u8],
    pos: usize,
}

impl<'a> Reader<'a> {
    fn new(data: &'a [u8]) -> Self {
        Self { data, pos: 0 }
    }

    fn is_eof(&self) -> bool {
        self.pos == self.data.len()
    }

    fn read_exact(&mut self, len: usize) -> Result<&'a [u8], DecompressError> {
        let end = self
            .pos
            .checked_add(len)
            .ok_or(DecompressError::Truncated)?;
        if end > self.data.len() {
            return Err(DecompressError::Truncated);
        }
        let out = &self.data[self.pos..end];
        self.pos = end;
        Ok(out)
    }

    fn read_u8(&mut self) -> Result<u8, DecompressError> {
        Ok(self.read_exact(1)?[0])
    }

    fn read_i8(&mut self) -> Result<i8, DecompressError> {
        Ok(i8::from_be_bytes([self.read_u8()?]))
    }

    fn read_u16(&mut self) -> Result<u16, DecompressError> {
        let bytes = self.read_exact(2)?;
        Ok(u16::from_be_bytes([bytes[0], bytes[1]]))
    }

    fn read_i32(&mut self) -> Result<i32, DecompressError> {
        let bytes = self.read_exact(4)?;
        Ok(i32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
    }

    fn read_var_i32(&mut self) -> Result<i32, DecompressError> {
        let head = self.read_u8()?;
        if head == 0xFF {
            self.read_i32()
        } else if head >= 0x80 {
            let next = self.read_u8()?;
            let high = head.wrapping_sub(0xC0);
            Ok(i16::from_be_bytes([high, next]) as i32)
        } else {
            Ok(head as i8 as i32)
        }
    }
}

fn read_u16(data: &[u8], offset: usize) -> u16 {
    u16::from_be_bytes([data[offset], data[offset + 1]])
}

fn read_i16(data: &[u8], offset: usize) -> i16 {
    i16::from_be_bytes([data[offset], data[offset + 1]])
}

fn read_u32(data: &[u8], offset: usize) -> u32 {
    u32::from_be_bytes([
        data[offset],
        data[offset + 1],
        data[offset + 2],
        data[offset + 3],
    ])
}

#[cfg(test)]
mod tests {
    use super::*;

    fn compressed_resource(decompressed_len: u32, body: &[u8]) -> Vec<u8> {
        let mut out = Vec::new();
        out.extend_from_slice(COMPRESSED_MAGIC);
        out.extend_from_slice(&(HEADER_LEN as u16).to_be_bytes());
        out.extend_from_slice(&COMPRESSED_TYPE_8.to_be_bytes());
        out.extend_from_slice(&decompressed_len.to_be_bytes());
        out.extend_from_slice(&[0x80, 0x03]);
        out.extend_from_slice(&0i16.to_be_bytes());
        out.extend_from_slice(&0u16.to_be_bytes());
        out.extend_from_slice(body);
        out
    }

    #[test]
    fn dcmp0_decodes_literal_resource() {
        let data = compressed_resource(4, &[0x02, b'A', b'B', b'C', b'D', 0xFF]);

        let decompressed = decompress_if_needed(0x01, &data).unwrap().unwrap();

        assert_eq!(decompressed, b"ABCD");
    }

    #[test]
    fn dcmp3_decodes_literal_and_overlapping_copy() {
        let mut data = compressed_resource(9, &[0x2A, 0x0A, 0x12, 0x1D, 0x48]);
        data[6..8].copy_from_slice(&COMPRESSED_TYPE_9.to_be_bytes());
        data[12..14].copy_from_slice(&3i16.to_be_bytes());
        assert_eq!(
            decompress_if_needed(0x01, &data).unwrap().unwrap(),
            b"ABCABCABC"
        );
    }

    #[test]
    fn dcmp0_decodes_jump_table_extension() {
        let data = compressed_resource(16, &[0xFE, 0x01, 0x12, 0x08, 0x01, 0x20, 0xFF]);
        assert_eq!(
            decompress_if_needed(0x01, &data).unwrap().unwrap(),
            [
                0x61, 0x00, 0x00, 0x12, 0x4E, 0xED, 0x00, 0x20, 0x61, 0x00, 0x00, 0x0A, 0x4E, 0xED,
                0x00, 0x28
            ]
        );
    }

    #[test]
    fn dcmp0_decodes_stored_literal_backreference_and_odd_trim() {
        let data = compressed_resource(7, &[0x12, b'A', b'B', b'C', b'D', 0x23, 0xFF]);

        let decompressed = decompress_if_needed(0x01, &data).unwrap().unwrap();

        assert_eq!(decompressed, b"ABCDABC");
    }

    #[test]
    fn dcmp0_decodes_extended_jump_table_chunk() {
        let data = compressed_resource(
            24,
            &[0x01, 0x12, 0x34, 0xFE, 0x00, 0x05, 0x02, 0x20, 0x08, 0xFF],
        );

        let decompressed = decompress_if_needed(0x01, &data).unwrap().unwrap();

        assert_eq!(
            decompressed,
            [
                0x12, 0x34, 0x3F, 0x3C, 0x00, 0x05, 0xA9, 0xF0, 0x00, 0x20, 0x3F, 0x3C, 0x00, 0x05,
                0xA9, 0xF0, 0x00, 0x22, 0x3F, 0x3C, 0x00, 0x05, 0xA9, 0xF0,
            ]
        );
    }

    #[test]
    fn uncompressed_resource_is_ignored() {
        assert_eq!(decompress_if_needed(0x00, b"\xA8\x9Fer").unwrap(), None);
        assert_eq!(decompress_if_needed(0x01, b"plain").unwrap(), None);
    }
}
