//! Bounded, read-only decoding of NDIF (Disk Copy 6) images. The `bcem`
//! resource maps disk sectors to chunks stored in the data fork.
//! Layout: a 128-byte header with the disk's sector count as a big-endian
//! u32 at offset 68, then 12-byte entries of big-endian u32s: the start
//! sector in the high 24 bits and the chunk type in the low 8 bits, the
//! chunk's data fork offset, and its stored length. Each chunk ends where
//! the next entry starts; a 0xFF entry at the sector count ends the table.

const MAX_DISK_BYTES: u64 = 1024 * 1024 * 1024;
const SECTOR_BYTES: u64 = 512;
const HEADER_BYTES: usize = 128;
const ENTRY_BYTES: usize = 12;
const SECTOR_COUNT_OFFSET: usize = 68;

const CHUNK_ZERO: u8 = 0x00;
const CHUNK_RAW: u8 = 0x02;
const CHUNK_ADC: u8 = 0x83;
const CHUNK_END: u8 = 0xff;

fn u32_at(b: &[u8], off: usize) -> u32 {
    u32::from_be_bytes(b[off..off + 4].try_into().unwrap())
}

pub(super) fn decode(data: &[u8], bcem: &[u8]) -> Result<Vec<u8>, String> {
    if bcem.len() < HEADER_BYTES || !(bcem.len() - HEADER_BYTES).is_multiple_of(ENTRY_BYTES) {
        return Err("NDIF block table has an invalid length".into());
    }
    let sectors = u64::from(u32_at(bcem, SECTOR_COUNT_OFFSET));
    let disk_len = sectors * SECTOR_BYTES;
    if disk_len == 0 || disk_len > MAX_DISK_BYTES {
        return Err("NDIF disk size exceeds the 1 GiB extraction limit".into());
    }
    let entries: Vec<&[u8]> = bcem[HEADER_BYTES..].chunks_exact(ENTRY_BYTES).collect();
    let sector_of = |e: &[u8]| u64::from(u32_at(e, 0) >> 8);
    let mut chunks = Vec::with_capacity(entries.len());
    let mut expected = 0;
    for pair in entries.windows(2) {
        let (entry, next) = (pair[0], pair[1]);
        let (start, end) = (sector_of(entry), sector_of(next));
        if start != expected || end <= start || end > sectors {
            return Err("NDIF chunks overlap or leave an unmapped disk range".into());
        }
        expected = end;
        let kind = entry[3];
        let len = ((end - start) * SECTOR_BYTES) as usize;
        let source = match kind {
            CHUNK_END => return Err("NDIF chunk follows table terminator".into()),
            CHUNK_ZERO => &[][..],
            CHUNK_RAW | CHUNK_ADC => {
                let offset = u64::from(u32_at(entry, 4));
                let stored = offset + u64::from(u32_at(entry, 8));
                if stored > data.len() as u64 {
                    return Err("NDIF chunk exceeds the data fork".into());
                }
                let source = &data[offset as usize..stored as usize];
                if kind == CHUNK_RAW && source.len() != len {
                    return Err("NDIF raw chunk length mismatch".into());
                }
                source
            }
            _ => return Err(format!("unsupported NDIF chunk type 0x{kind:02x}")),
        };
        chunks.push((kind, len, source));
    }
    let last = entries.last().ok_or("NDIF block table has no chunks")?;
    if last[3] != CHUNK_END || sector_of(last) != sectors || expected != sectors {
        return Err("NDIF block table does not cover the disk".into());
    }
    // The whole table is validated first; the output then grows per chunk.
    let mut output = Vec::new();
    for (kind, len, source) in chunks {
        let start = output.len();
        output.resize(start + len, 0);
        let dst = &mut output[start..];
        match kind {
            CHUNK_RAW => dst.copy_from_slice(source),
            CHUNK_ADC => expand_adc(source, dst)?,
            _ => {}
        }
    }
    Ok(output)
}

/// Apple Data Compression: a literal run, or a copy from earlier output whose
/// source may overlap its destination.
fn expand_adc(mut source: &[u8], dst: &mut [u8]) -> Result<(), String> {
    const TRUNCATED: &str = "NDIF ADC chunk is truncated";
    let mut written = 0;
    while let Some((&code, rest)) = source.split_first() {
        source = rest;
        if code & 0x80 != 0 {
            let len = usize::from(code & 0x7f) + 1;
            let literal = source.get(..len).ok_or(TRUNCATED)?;
            dst.get_mut(written..written + len)
                .ok_or("NDIF ADC chunk exceeds its declared size")?
                .copy_from_slice(literal);
            source = &source[len..];
            written += len;
            continue;
        }
        let (len, distance) = if code & 0x40 != 0 {
            let offset = source.get(..2).ok_or(TRUNCATED)?;
            let distance = usize::from(u16::from_be_bytes([offset[0], offset[1]]));
            source = &source[2..];
            (usize::from(code & 0x3f) + 4, distance)
        } else {
            let (&low, rest) = source.split_first().ok_or(TRUNCATED)?;
            source = rest;
            (
                usize::from((code >> 2) & 0x0f) + 3,
                (usize::from(code & 0x03) << 8) | usize::from(low),
            )
        };
        let Some(from) = written.checked_sub(distance + 1) else {
            return Err("NDIF ADC copy precedes the chunk".into());
        };
        if written + len > dst.len() {
            return Err("NDIF ADC chunk exceeds its declared size".into());
        }
        for index in 0..len {
            dst[written + index] = dst[from + index];
        }
        written += len;
    }
    if written != dst.len() {
        return Err("NDIF ADC chunk is shorter than its declared size".into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn table(sectors: u32, chunks: &[(u32, u8, u32, u32)]) -> Vec<u8> {
        let mut bcem = vec![0; HEADER_BYTES];
        bcem[SECTOR_COUNT_OFFSET..SECTOR_COUNT_OFFSET + 4].copy_from_slice(&sectors.to_be_bytes());
        for &(sector, kind, offset, len) in chunks {
            bcem.extend_from_slice(&((sector << 8) | u32::from(kind)).to_be_bytes());
            bcem.extend_from_slice(&offset.to_be_bytes());
            bcem.extend_from_slice(&len.to_be_bytes());
        }
        bcem
    }

    #[test]
    fn expands_literals_and_both_copy_forms() {
        let mut dst = [0; 16];
        // Literal "abc", 2-byte copy of "abc", overlapping 3-byte copy of "c",
        // literal "xyz", overlapping 2-byte copy of "z".
        let source = [
            0x82, b'a', b'b', b'c', 0x00, 0x02, 0x40, 0x00, 0x00, 0x82, b'x', b'y', b'z', 0x00,
            0x00,
        ];
        expand_adc(&source, &mut dst).unwrap();
        assert_eq!(&dst, b"abcabcccccxyzzzz");
    }

    #[test]
    fn adc_rejects_truncated_and_out_of_range_input() {
        let mut dst = [0; 4];
        assert!(expand_adc(&[0x83, b'a'], &mut dst).is_err());
        assert!(expand_adc(&[0x00, 0x00, 0x83, 0, 0, 0, 0], &mut dst).is_err());
        assert!(expand_adc(&[0x87, 0, 0, 0, 0, 0, 0, 0, 0], &mut dst).is_err());
        assert!(expand_adc(&[0x80, b'a'], &mut dst).is_err());
    }

    #[test]
    fn reconstructs_raw_zero_and_adc_chunks() {
        let raw = vec![7; 512];
        // One literal byte followed by a 511-byte run built from copies.
        let mut adc = vec![0x80, 9];
        let mut remaining = 511;
        while remaining > 0 {
            let len = remaining.clamp(4, 67);
            adc.extend_from_slice(&[0x40 | (len - 4) as u8, 0, 0]);
            remaining -= len;
        }
        let mut data = raw.clone();
        data.extend_from_slice(&adc);
        let bcem = table(
            3,
            &[
                (0, CHUNK_RAW, 0, 512),
                (1, CHUNK_ADC, 512, adc.len() as u32),
                (2, CHUNK_ZERO, 0, 0),
                (3, CHUNK_END, data.len() as u32, 0),
            ],
        );
        let disk = decode(&data, &bcem).unwrap();
        assert_eq!(disk.len(), 1536);
        assert_eq!(&disk[..512], &raw[..]);
        assert!(disk[512..1024].iter().all(|&b| b == 9));
        assert!(disk[1024..].iter().all(|&b| b == 0));
    }

    #[test]
    fn rejects_malformed_tables() {
        let data = [0; 512];
        let end = (1, CHUNK_END, 0, 0);
        assert!(decode(&data, &[0; 10]).is_err());
        assert!(decode(&data, &table(0, &[(0, CHUNK_END, 0, 0)])).is_err());
        assert!(decode(&data, &table(u32::MAX, &[(0, CHUNK_END, 0, 0)]))
            .unwrap_err()
            .contains("limit"));
        assert!(decode(&data, &table(1, &[(0, 0x80, 0, 512), end]))
            .unwrap_err()
            .contains("unsupported"));
        assert!(decode(&data, &table(1, &[(0, CHUNK_RAW, 1, 512), end]))
            .unwrap_err()
            .contains("data fork"));
        assert!(
            decode(&data, &table(1, &[(0, CHUNK_RAW, u32::MAX, u32::MAX), end]))
                .unwrap_err()
                .contains("data fork")
        );
        assert!(decode(
            &data,
            &table(2, &[(1, CHUNK_ZERO, 0, 0), (2, CHUNK_END, 0, 0)])
        )
        .unwrap_err()
        .contains("unmapped"));
        assert!(decode(&data, &table(2, &[(0, CHUNK_ZERO, 0, 0), end]))
            .unwrap_err()
            .contains("cover"));
    }
}
