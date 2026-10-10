//! Bounded, read-only UDIF decoding. Publisher archives stay unchanged.
//! Layout: <https://github.com/libyal/libmodi/blob/main/documentation/Mac%20OS%20disk%20image%20types.asciidoc>
use base64::{engine::general_purpose::STANDARD, Engine};
use std::{io::Read, ops::Range};

const MAX_DISK_BYTES: u64 = 256 * 1024 * 1024;
const MAX_XML_BYTES: u64 = 16 * 1024 * 1024;
const MAX_CHUNKS: usize = 65_536;

pub(super) fn recognizes(bytes: &[u8]) -> bool {
    bytes.len() >= 512 && &bytes[bytes.len() - 512..bytes.len() - 508] == b"koly"
}

fn u32_at(b: &[u8], off: usize) -> Result<u32, String> {
    let value = b.get(off..off + 4).ok_or("UDIF truncated integer")?;
    Ok(u32::from_be_bytes(value.try_into().unwrap()))
}
fn u64_at(b: &[u8], off: usize) -> Result<u64, String> {
    let value = b.get(off..off + 8).ok_or("UDIF truncated integer")?;
    Ok(u64::from_be_bytes(value.try_into().unwrap()))
}
fn range(start: u64, len: u64, limit: usize) -> Result<Range<usize>, String> {
    let end = start.checked_add(len).ok_or("UDIF range overflow")?;
    if end > limit as u64 {
        return Err("UDIF range exceeds its containing data".into());
    }
    Ok(start as usize..end as usize)
}
fn sectors(n: u64) -> Result<u64, String> {
    n.checked_mul(512)
        .ok_or_else(|| "UDIF sector overflow".into())
}
fn value<'a, 'input>(
    dict: roxmltree::Node<'a, 'input>,
    key: &str,
) -> Result<roxmltree::Node<'a, 'input>, String> {
    if !dict.has_tag_name("dict") {
        return Err("UDIF plist expected dictionary".into());
    }
    let mut nodes = dict.children().filter(|n| n.is_element());
    let mut found = None;
    while let Some(k) = nodes.next() {
        let v = nodes.next().ok_or("UDIF plist missing dictionary value")?;
        if !k.has_tag_name("key") {
            return Err("UDIF plist expected dictionary key".into());
        }
        if k.text() == Some(key) && found.replace(v).is_some() {
            return Err(format!("UDIF plist duplicate {key}"));
        }
    }
    found.ok_or_else(|| format!("UDIF plist missing {key}"))
}

pub(super) fn decode(bytes: &[u8]) -> Result<Vec<u8>, String> {
    if !recognizes(bytes) {
        return Err("UDIF footer missing".into());
    }
    let footer_start = bytes.len() - 512;
    let f = &bytes[footer_start..];
    if u32_at(f, 4)? != 4 || u32_at(f, 8)? != 512 {
        return Err("unsupported UDIF footer version or size".into());
    }
    if u64_at(f, 16)? != 0 || u32_at(f, 56)? > 1 || u32_at(f, 60)? > 1 {
        return Err("segmented UDIF images are unsupported".into());
    }
    let disk_len = sectors(u64_at(f, 492)?)?;
    if disk_len == 0 || disk_len > MAX_DISK_BYTES {
        return Err("UDIF disk size exceeds the 256 MiB extraction limit".into());
    }
    let data = range(u64_at(f, 24)?, u64_at(f, 32)?, footer_start)?;
    let xml_len = u64_at(f, 224)?;
    if xml_len == 0 || xml_len > MAX_XML_BYTES {
        return Err("UDIF requires an XML block table no larger than 16 MiB".into());
    }
    let xml_range = range(u64_at(f, 216)?, xml_len, footer_start)?;
    if data.start < xml_range.end && xml_range.start < data.end {
        return Err("UDIF XML overlaps the data fork".into());
    }
    let xml = std::str::from_utf8(&bytes[xml_range]).map_err(|e| format!("UDIF XML: {e}"))?;
    // Apple plists declare a DTD. No external entities are fetched or resolved.
    let doc = roxmltree::Document::parse_with_options(
        xml,
        roxmltree::ParsingOptions {
            allow_dtd: true,
            nodes_limit: 100_000,
            ..Default::default()
        },
    )
    .map_err(|e| format!("UDIF plist: {e}"))?;
    let root = doc.root_element();
    if !root.has_tag_name("plist") {
        return Err("UDIF XML is not a plist".into());
    }
    let dict = root.first_element_child().ok_or("UDIF empty plist")?;
    let blocks = value(value(dict, "resource-fork")?, "blkx")?;
    if !blocks.has_tag_name("array") {
        return Err("UDIF blkx is not an array".into());
    }
    let mut output = vec![0; disk_len as usize];
    let mut covered = Vec::new();
    let mut chunks: usize = 0;
    let mut tables = 0;
    for entry in blocks.children().filter(|n| n.is_element()) {
        tables += 1;
        let encoded = value(entry, "Data")?;
        if !encoded.has_tag_name("data") {
            return Err("UDIF block table is not plist data".into());
        }
        let text: String = encoded
            .text()
            .unwrap_or("")
            .chars()
            .filter(|c| !c.is_ascii_whitespace())
            .collect();
        let table = STANDARD
            .decode(text)
            .map_err(|e| format!("UDIF block table base64: {e}"))?;
        if table.get(..4) != Some(b"mish") || u32_at(&table, 4)? != 1 {
            return Err("unsupported UDIF block table signature or version".into());
        }
        let table_start = sectors(u64_at(&table, 8)?)?;
        let table_len = sectors(u64_at(&table, 16)?)?;
        let table_range = range(table_start, table_len, output.len())?;
        let data_offset = u64_at(&table, 24)?;
        let count = u32_at(&table, 200)? as usize;
        chunks = chunks
            .checked_add(count)
            .ok_or("UDIF chunk count overflow")?;
        if chunks > MAX_CHUNKS || count == 0 || table.len() != 204 + count * 40 {
            return Err("UDIF invalid block count or table length".into());
        }
        let mut terminated = false;
        for c in table[204..].chunks_exact(40) {
            let kind = u32_at(c, 0)?;
            if terminated {
                return Err("UDIF chunk follows table terminator".into());
            }
            if kind == 0xffff_ffff {
                terminated = true;
                continue;
            }
            if kind == 0x7fff_fffe {
                continue;
            }
            let relative = sectors(u64_at(c, 8)?)?;
            let len = sectors(u64_at(c, 16)?)?;
            range(relative, len, table_range.len())?;
            let target = range(
                table_start
                    .checked_add(relative)
                    .ok_or("UDIF sector overflow")?,
                len,
                output.len(),
            )?;
            if target.is_empty() {
                return Err("UDIF empty data chunk".into());
            }
            covered.push(target.clone());
            let stored_len = u64_at(c, 32)?;
            if kind == 0 || kind == 2 {
                if stored_len != 0 {
                    return Err("UDIF sparse chunk contains data".into());
                }
                continue;
            }
            let offset = data_offset
                .checked_add(u64_at(c, 24)?)
                .ok_or("UDIF data offset overflow")?;
            let source = range(offset, stored_len, data.len())?;
            let compressed = &bytes[data.start + source.start..data.start + source.end];
            let dst = &mut output[target];
            match kind {
                1 => {
                    if compressed.len() != dst.len() {
                        return Err("UDIF raw chunk length mismatch".into());
                    }
                    dst.copy_from_slice(compressed);
                }
                0x8000_0005 => {
                    let decoder = unpack(flate2::read::ZlibDecoder::new(compressed), dst)?;
                    if decoder.total_in() != stored_len {
                        return Err("UDIF zlib chunk has trailing data".into());
                    }
                }
                0x8000_0006 => {
                    let decoder = unpack(bzip2::read::BzDecoder::new(compressed), dst)?;
                    if decoder.total_in() != stored_len {
                        return Err("UDIF bzip2 chunk has trailing data".into());
                    }
                }
                _ => return Err(format!("unsupported UDIF chunk compression 0x{kind:08x}")),
            }
        }
        if !terminated {
            return Err("UDIF block table has no terminator".into());
        }
    }
    if tables == 0 {
        return Err("UDIF has no block tables".into());
    }
    covered.sort_unstable_by_key(|r| r.start);
    let mut end = 0;
    for r in covered {
        if r.start != end {
            return Err("UDIF chunks overlap or leave an unmapped disk range".into());
        }
        end = r.end;
    }
    if end != output.len() {
        return Err("UDIF chunks do not cover the disk".into());
    }
    Ok(output)
}

fn unpack<R: Read>(mut decoder: R, dst: &mut [u8]) -> Result<R, String> {
    decoder
        .read_exact(dst)
        .map_err(|e| format!("UDIF compressed chunk: {e}"))?;
    let mut extra = [0];
    if decoder
        .read(&mut extra)
        .map_err(|e| format!("UDIF compressed chunk: {e}"))?
        != 0
    {
        return Err("UDIF decompressed chunk exceeds its declared size".into());
    }
    Ok(decoder)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn put32(b: &mut [u8], at: usize, n: u32) {
        b[at..at + 4].copy_from_slice(&n.to_be_bytes());
    }
    fn put64(b: &mut [u8], at: usize, n: u64) {
        b[at..at + 8].copy_from_slice(&n.to_be_bytes());
    }
    fn image(kind: u32, stored: &[u8], sectors: u64) -> Vec<u8> {
        let mut table = vec![0; 284];
        table[..4].copy_from_slice(b"mish");
        put32(&mut table, 4, 1);
        put64(&mut table, 16, sectors);
        put32(&mut table, 200, 2);
        put32(&mut table, 204, kind);
        put64(&mut table, 220, sectors);
        put64(&mut table, 236, stored.len() as u64);
        put32(&mut table, 244, 0xffff_ffff);
        container(&table, stored, sectors)
    }
    fn container(table: &[u8], stored: &[u8], sectors: u64) -> Vec<u8> {
        let xml = format!("<?xml version=\"1.0\"?><!DOCTYPE plist PUBLIC \"-//Apple//DTD PLIST 1.0//EN\" \"http://www.apple.com/DTDs/PropertyList-1.0.dtd\"><plist><dict><key>resource-fork</key><dict><key>blkx</key><array><dict><key>Data</key><data>{}</data></dict></array></dict></dict></plist>", STANDARD.encode(table));
        let mut b = stored.to_vec();
        b.extend_from_slice(xml.as_bytes());
        let mut footer = vec![0; 512];
        footer[..4].copy_from_slice(b"koly");
        put32(&mut footer, 4, 4);
        put32(&mut footer, 8, 512);
        put64(&mut footer, 32, stored.len() as u64);
        put64(&mut footer, 216, stored.len() as u64);
        put64(&mut footer, 224, xml.len() as u64);
        put64(&mut footer, 492, sectors);
        b.extend_from_slice(&footer);
        b
    }
    #[test]
    fn original_chunk_encodings_reconstruct_identical_sectors() {
        let disk: Vec<u8> = (0..1024).map(|n| (n % 251) as u8).collect();
        assert_eq!(decode(&image(1, &disk, 2)).unwrap(), disk);
        let mut z = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::default());
        z.write_all(&disk).unwrap();
        assert_eq!(
            decode(&image(0x8000_0005, &z.finish().unwrap(), 2)).unwrap(),
            disk
        );
        let mut bz = bzip2::write::BzEncoder::new(Vec::new(), bzip2::Compression::default());
        bz.write_all(&disk).unwrap();
        assert_eq!(
            decode(&image(0x8000_0006, &bz.finish().unwrap(), 2)).unwrap(),
            disk
        );
        assert_eq!(decode(&image(2, &[], 2)).unwrap(), vec![0; 1024]);
    }
    #[test]
    fn rejects_malformed_ranges_and_unsupported_compression() {
        assert!(decode(&image(1, &[0; 511], 1))
            .unwrap_err()
            .contains("length mismatch"));
        assert!(decode(&image(0x8000_0007, &[0; 512], 1))
            .unwrap_err()
            .contains("unsupported UDIF chunk compression"));
        let mut b = image(1, &[0; 512], 1);
        let f = b.len() - 512;
        put64(&mut b, f + 216, u64::MAX);
        assert!(decode(&b).unwrap_err().contains("overflow"));
        let mut b = image(1, &[0; 512], 1);
        let f = b.len() - 512;
        put64(&mut b, f + 492, u64::MAX);
        assert!(decode(&b).unwrap_err().contains("overflow"));
    }
    #[test]
    fn rejects_overlaps_gaps_and_unbounded_chunk_counts() {
        let b = image(2, &[], 1);
        let footer = &b[b.len() - 512..];
        let off = u64_at(footer, 216).unwrap() as usize;
        let len = u64_at(footer, 224).unwrap() as usize;
        let doc = roxmltree::Document::parse_with_options(
            std::str::from_utf8(&b[off..off + len]).unwrap(),
            roxmltree::ParsingOptions {
                allow_dtd: true,
                ..Default::default()
            },
        )
        .unwrap();
        let data = doc
            .descendants()
            .find(|n| n.has_tag_name("data"))
            .unwrap()
            .text()
            .unwrap();
        let table = STANDARD.decode(data).unwrap();
        let mut overlap = table.clone();
        let chunk = overlap[204..244].to_vec();
        overlap.splice(244..244, chunk);
        put32(&mut overlap, 200, 3);
        assert!(decode(&container(&overlap, &[], 1))
            .unwrap_err()
            .contains("overlap"));
        let mut gap = table.clone();
        put64(&mut gap, 16, 2);
        assert!(decode(&container(&gap, &[], 2))
            .unwrap_err()
            .contains("cover"));
        let mut huge = table.clone();
        put32(&mut huge, 200, u32::MAX);
        assert!(decode(&container(&huge, &[], 1))
            .unwrap_err()
            .contains("block count"));
        let mut no_end = table;
        put32(&mut no_end, 244, 0x7fff_fffe);
        assert!(decode(&container(&no_end, &[], 1))
            .unwrap_err()
            .contains("terminator"));
    }
    #[test]
    fn refuses_short_and_overlong_decompression() {
        for len in [511, 513] {
            let mut z = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::default());
            z.write_all(&vec![0; len]).unwrap();
            assert!(decode(&image(0x8000_0005, &z.finish().unwrap(), 1)).is_err());
        }
    }
}
