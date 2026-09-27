//! BinHex 4.0 decoding for classic Mac single-file containers.
//!
//! BinHex wraps a Macintosh filename, Finder metadata, data fork, and resource
//! fork in a colon-delimited ASCII stream. Systemless uses the decoded forks as
//! another input to the existing StuffIt, MacBinary, disk-image, and APPL
//! loaders.

const BINHEX_MARKER: &[u8] = b"with BinHex";
const BINHEX_ALPHABET: &[u8; 64] =
    b"!\"#$%&'()*+,-012345689@ABCDEFGHIJKLMNPQRSTUVXYZ[`abcdefhijklmpqr";
const RLE_MARKER: u8 = 0x90;
const FINDER_FLAGS_CLEAR_ON_DECODE: u16 = 0x4101;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BinHexFile {
    pub name: String,
    pub file_type: [u8; 4],
    pub creator: [u8; 4],
    pub finder_flags: u16,
    pub data: Vec<u8>,
    pub rsrc: Vec<u8>,
}

pub fn looks_like_binhex(bytes: &[u8]) -> bool {
    find_payload_start(bytes).is_some()
}

pub fn decode(bytes: &[u8]) -> Result<Option<BinHexFile>, String> {
    let Some(payload_start) = find_payload_start(bytes) else {
        return Ok(None);
    };

    let encoded = collect_encoded_payload(bytes, payload_start)?;
    parse_encoded_payload(&encoded)
        .or_else(|first_err| {
            if encoded.last() == Some(&0) {
                parse_encoded_payload(&encoded[..encoded.len() - 1]).map_err(|_| first_err)
            } else {
                Err(first_err)
            }
        })
        .map(Some)
}

fn find_payload_start(bytes: &[u8]) -> Option<usize> {
    let marker_pos = bytes
        .windows(BINHEX_MARKER.len())
        .position(|window| window == BINHEX_MARKER)?;
    let mut offset = marker_pos + BINHEX_MARKER.len();
    while offset < bytes.len() && !matches!(bytes[offset], b'\r' | b'\n') {
        offset += 1;
    }
    while bytes
        .get(offset)
        .is_some_and(|byte| is_binhex_return(*byte))
    {
        offset += 1;
    }
    (bytes.get(offset) == Some(&b':')).then_some(offset + 1)
}

fn collect_encoded_payload(bytes: &[u8], mut offset: usize) -> Result<Vec<u8>, String> {
    let mut encoded = Vec::new();
    while let Some(&byte) = bytes.get(offset) {
        offset += 1;
        if byte == b':' {
            return Ok(encoded);
        }
        if is_binhex_return(byte) {
            continue;
        }
        let Some(value) = decode_char(byte) else {
            return Err(format!("invalid BinHex character 0x{byte:02X}"));
        };
        encoded.push(value);
    }
    Err("BinHex payload missing terminating colon".to_string())
}

fn parse_encoded_payload(encoded: &[u8]) -> Result<BinHexFile, String> {
    let bytes = decode_6bit(encoded);
    let decoded = decode_rle(&bytes)?;
    parse_binhex_file(&decoded)
}

fn decode_6bit(encoded: &[u8]) -> Vec<u8> {
    let mut decoded = Vec::with_capacity(encoded.len() * 3 / 4);
    let mut bit_buffer = 0u32;
    let mut bit_count = 0u8;

    for &value in encoded {
        bit_buffer = (bit_buffer << 6) | u32::from(value);
        bit_count += 6;
        while bit_count >= 8 {
            bit_count -= 8;
            decoded.push(((bit_buffer >> bit_count) & 0xFF) as u8);
        }
    }

    decoded
}

fn decode_rle(bytes: &[u8]) -> Result<Vec<u8>, String> {
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut previous = None;
    let mut offset = 0usize;

    while offset < bytes.len() {
        let byte = bytes[offset];
        offset += 1;
        if byte != RLE_MARKER {
            decoded.push(byte);
            previous = Some(byte);
            continue;
        }

        let Some(&count) = bytes.get(offset) else {
            return Err("BinHex RLE marker missing repeat count".to_string());
        };
        offset += 1;
        if count == 0 {
            decoded.push(RLE_MARKER);
            previous = Some(RLE_MARKER);
            continue;
        }

        let byte = previous.ok_or_else(|| "BinHex RLE run before first byte".to_string())?;
        decoded.extend(std::iter::repeat_n(byte, count.saturating_sub(1) as usize));
    }

    Ok(decoded)
}

fn parse_binhex_file(bytes: &[u8]) -> Result<BinHexFile, String> {
    let mut offset = 0usize;
    let name_len = read_u8(bytes, &mut offset)? as usize;
    let name_bytes = read_exact(bytes, &mut offset, name_len)?;
    let terminator = read_u8(bytes, &mut offset)?;
    if terminator != 0 {
        return Err("BinHex header filename is not nul-terminated".to_string());
    }

    let file_type = read_fourcc(bytes, &mut offset)?;
    let creator = read_fourcc(bytes, &mut offset)?;
    let finder_flags = read_u16_be(bytes, &mut offset)? & !FINDER_FLAGS_CLEAR_ON_DECODE;
    let data_len = read_u32_be(bytes, &mut offset)? as usize;
    let rsrc_len = read_u32_be(bytes, &mut offset)? as usize;
    verify_crc("header", &bytes[..offset], read_u16_be(bytes, &mut offset)?)?;

    let data = read_exact(bytes, &mut offset, data_len)?.to_vec();
    verify_crc("data fork", &data, read_u16_be(bytes, &mut offset)?)?;

    let rsrc = read_exact(bytes, &mut offset, rsrc_len)?.to_vec();
    verify_crc("resource fork", &rsrc, read_u16_be(bytes, &mut offset)?)?;

    let name = std::str::from_utf8(name_bytes)
        .unwrap_or("BinHex File")
        .to_string();

    Ok(BinHexFile {
        name,
        file_type,
        creator,
        finder_flags,
        data,
        rsrc,
    })
}

fn verify_crc(section: &str, bytes: &[u8], expected: u16) -> Result<(), String> {
    let actual = binhex_crc(bytes);
    if actual == expected {
        Ok(())
    } else {
        Err(format!(
            "BinHex {section} CRC mismatch: expected 0x{expected:04X}, got 0x{actual:04X}"
        ))
    }
}

fn binhex_crc(bytes: &[u8]) -> u16 {
    let mut crc = 0u16;
    for &byte in bytes {
        calc_crc_byte(&mut crc, byte);
    }
    calc_crc_byte(&mut crc, 0);
    calc_crc_byte(&mut crc, 0);
    crc
}

fn calc_crc_byte(crc: &mut u16, mut byte: u8) {
    for _ in 0..8 {
        let high_bit = (*crc & 0x8000) != 0;
        *crc = (*crc << 1) | u16::from(byte >> 7);
        if high_bit {
            *crc ^= 0x1021;
        }
        byte <<= 1;
    }
}

fn read_exact<'a>(bytes: &'a [u8], offset: &mut usize, len: usize) -> Result<&'a [u8], String> {
    let end = offset
        .checked_add(len)
        .ok_or_else(|| "BinHex offset overflow".to_string())?;
    let slice = bytes
        .get(*offset..end)
        .ok_or_else(|| "BinHex payload truncated".to_string())?;
    *offset = end;
    Ok(slice)
}

fn read_u8(bytes: &[u8], offset: &mut usize) -> Result<u8, String> {
    let byte = *bytes
        .get(*offset)
        .ok_or_else(|| "BinHex payload truncated".to_string())?;
    *offset += 1;
    Ok(byte)
}

fn read_u16_be(bytes: &[u8], offset: &mut usize) -> Result<u16, String> {
    let bytes = read_exact(bytes, offset, 2)?;
    Ok(u16::from_be_bytes([bytes[0], bytes[1]]))
}

fn read_u32_be(bytes: &[u8], offset: &mut usize) -> Result<u32, String> {
    let bytes = read_exact(bytes, offset, 4)?;
    Ok(u32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
}

fn read_fourcc(bytes: &[u8], offset: &mut usize) -> Result<[u8; 4], String> {
    let bytes = read_exact(bytes, offset, 4)?;
    Ok([bytes[0], bytes[1], bytes[2], bytes[3]])
}

fn decode_char(byte: u8) -> Option<u8> {
    BINHEX_ALPHABET
        .iter()
        .position(|&candidate| candidate == byte)
        .map(|value| value as u8)
}

fn is_binhex_return(byte: u8) -> bool {
    matches!(byte, b'\r' | b'\n' | b'\t' | b' ')
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_binhex(
        name: &str,
        file_type: [u8; 4],
        creator: [u8; 4],
        finder_flags: u16,
        data: &[u8],
        rsrc: &[u8],
    ) -> Vec<u8> {
        let mut header = Vec::new();
        header.push(name.len() as u8);
        header.extend_from_slice(name.as_bytes());
        header.push(0);
        header.extend_from_slice(&file_type);
        header.extend_from_slice(&creator);
        header.extend_from_slice(&finder_flags.to_be_bytes());
        header.extend_from_slice(&(data.len() as u32).to_be_bytes());
        header.extend_from_slice(&(rsrc.len() as u32).to_be_bytes());

        let mut decoded = header.clone();
        decoded.extend_from_slice(&binhex_crc(&header).to_be_bytes());
        decoded.extend_from_slice(data);
        decoded.extend_from_slice(&binhex_crc(data).to_be_bytes());
        decoded.extend_from_slice(rsrc);
        decoded.extend_from_slice(&binhex_crc(rsrc).to_be_bytes());

        wrap_decoded(&decoded)
    }

    fn wrap_decoded(decoded: &[u8]) -> Vec<u8> {
        let rle = encode_rle_literals_only(decoded);
        let encoded = encode_6bit(&rle);
        let mut hqx = b"(This file must be converted with BinHex 4.0)\r:".to_vec();
        hqx.extend_from_slice(&encoded);
        hqx.extend_from_slice(b":\r");
        hqx
    }

    fn encode_rle_literals_only(bytes: &[u8]) -> Vec<u8> {
        let mut out = Vec::new();
        for &byte in bytes {
            if byte == RLE_MARKER {
                out.extend_from_slice(&[RLE_MARKER, 0]);
            } else {
                out.push(byte);
            }
        }
        out
    }

    fn encode_6bit(bytes: &[u8]) -> Vec<u8> {
        let mut out = Vec::new();
        let mut bit_buffer = 0u32;
        let mut bit_count = 0u8;
        for &byte in bytes {
            bit_buffer = (bit_buffer << 8) | u32::from(byte);
            bit_count += 8;
            while bit_count >= 6 {
                bit_count -= 6;
                out.push(BINHEX_ALPHABET[((bit_buffer >> bit_count) & 0x3F) as usize]);
            }
        }
        if bit_count > 0 {
            out.push(BINHEX_ALPHABET[((bit_buffer << (6 - bit_count)) & 0x3F) as usize]);
        }
        out
    }

    #[test]
    fn decodes_binhex_forks_and_metadata() {
        let hqx = make_binhex(
            "Sample.img",
            *b"dImg",
            *b"ddsk",
            0xC101,
            b"data fork",
            b"resource fork",
        );

        let file = decode(&hqx)
            .expect("BinHex should parse")
            .expect("payload should be detected");

        assert_eq!(file.name, "Sample.img");
        assert_eq!(file.file_type, *b"dImg");
        assert_eq!(file.creator, *b"ddsk");
        assert_eq!(file.finder_flags, 0x8000);
        assert_eq!(file.data, b"data fork");
        assert_eq!(file.rsrc, b"resource fork");
    }

    #[test]
    fn decodes_literal_marker_then_marker_run() {
        let decoded = decode_rle(&[0x2B, RLE_MARKER, 0, RLE_MARKER, 5]).unwrap();

        assert_eq!(
            decoded,
            [0x2B, RLE_MARKER, RLE_MARKER, RLE_MARKER, RLE_MARKER, RLE_MARKER]
        );
    }

    #[test]
    fn rejects_bad_crc() {
        let data = b"data";
        let mut header = Vec::new();
        header.push(6);
        header.extend_from_slice(b"Sample");
        header.push(0);
        header.extend_from_slice(b"TEXT");
        header.extend_from_slice(b"ttxt");
        header.extend_from_slice(&0u16.to_be_bytes());
        header.extend_from_slice(&(data.len() as u32).to_be_bytes());
        header.extend_from_slice(&0u32.to_be_bytes());

        let mut decoded = header.clone();
        decoded.extend_from_slice(&binhex_crc(&header).to_be_bytes());
        decoded.extend_from_slice(data);
        decoded.extend_from_slice(&(binhex_crc(data) ^ 1).to_be_bytes());
        decoded.extend_from_slice(&binhex_crc(&[]).to_be_bytes());
        let hqx = wrap_decoded(&decoded);

        let err = decode(&hqx).expect_err("modified BinHex should fail CRC");

        assert!(err.contains("CRC mismatch"));
    }
}
