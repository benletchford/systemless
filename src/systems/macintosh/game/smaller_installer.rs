//! Cyclos Smaller Installer payloads use a Compact Pro directory and fork streams.
//! Recognition comes from the installer resource fork, never an application name.

const MAX_EXPANDED: usize = 256 * 1024 * 1024;

pub(crate) struct File {
    pub name: String,
    pub file_type: [u8; 4],
    pub creator: [u8; 4],
    pub finder_flags: u16,
    pub data: Vec<u8>,
    pub rsrc: Vec<u8>,
}

struct Entry {
    name: String,
    metadata: [u8; 45],
}

fn take<'a>(bytes: &'a [u8], position: &mut usize, count: usize) -> Result<&'a [u8], String> {
    let end = position
        .checked_add(count)
        .ok_or("installer offset overflow")?;
    let value = bytes
        .get(*position..end)
        .ok_or("truncated Smaller Installer")?;
    *position = end;
    Ok(value)
}

fn word(bytes: &[u8]) -> usize {
    u16::from_be_bytes([bytes[0], bytes[1]]) as usize
}

fn long(bytes: &[u8]) -> usize {
    u32::from_be_bytes(bytes[..4].try_into().unwrap()) as usize
}

fn directory(
    bytes: &[u8],
    position: &mut usize,
    count: usize,
    prefix: &str,
    depth: usize,
    entries: &mut Vec<Entry>,
) -> Result<(), String> {
    if depth > 32 {
        return Err("Smaller Installer folder nesting exceeds limit".into());
    }
    let mut remaining = count;
    while remaining > 0 {
        let size = take(bytes, position, 1)?[0];
        let name =
            crate::trap::types::decode_mac_roman(take(bytes, position, (size & 127) as usize)?);
        if name.is_empty() || matches!(name.as_str(), "." | "..") || name.contains(['/', ':', '\0'])
        {
            return Err("invalid Smaller Installer path component".into());
        }
        let path = if prefix.is_empty() {
            name
        } else {
            format!("{prefix}/{name}")
        };
        remaining -= 1;
        if size & 128 != 0 {
            let children = word(take(bytes, position, 2)?);
            if children > remaining {
                return Err("invalid Smaller Installer folder count".into());
            }
            directory(bytes, position, children, &path, depth + 1, entries)?;
            remaining -= children;
        } else {
            entries.push(Entry {
                name: path,
                metadata: take(bytes, position, 45)?.try_into().unwrap(),
            });
        }
    }
    Ok(())
}

pub(crate) fn expand(bytes: &[u8], resources: &[u8]) -> Result<Option<Vec<File>>, String> {
    if !resources
        .windows(b"Smaller Installer".len())
        .any(|window| window == b"Smaller Installer")
    {
        return Ok(None);
    }
    let mut position = 4;
    let count = word(take(bytes, &mut position, 2)?);
    take(bytes, &mut position, 4)?;
    if count == 0 {
        return Err("empty Smaller Installer directory".into());
    }
    let mut entries = Vec::new();
    directory(bytes, &mut position, count, "", 0, &mut entries)?;
    let mut expanded = 0usize;
    let mut files = Vec::new();
    let mut names = std::collections::BTreeSet::new();
    for entry in entries {
        let m = entry.metadata;
        let flags = word(&m[27..]);
        if m[0] != 1 || flags & !6 != 0 {
            return Err("unsupported Smaller Installer volume or flags".into());
        }
        if !names.insert(entry.name.to_lowercase()) {
            return Err("duplicate Smaller Installer path".into());
        }
        let resource_len = long(&m[29..]);
        let data_len = long(&m[33..]);
        expanded = expanded
            .checked_add(resource_len)
            .and_then(|size| size.checked_add(data_len))
            .ok_or("installer expansion overflow")?;
        if expanded > MAX_EXPANDED {
            return Err("Smaller Installer expanded size exceeds limit".into());
        }
        let mut offset = long(&m[1..]);
        if offset < position {
            return Err("Smaller Installer fork overlaps directory".into());
        }
        let rp = take(bytes, &mut offset, long(&m[37..]))?;
        let dp = take(bytes, &mut offset, long(&m[41..]))?;
        let rsrc = decode(rp, resource_len, flags & 2 != 0)?;
        let data = decode(dp, data_len, flags & 4 != 0)?;
        if crc(rsrc.iter().chain(data.iter()).copied()) != long(&m[23..]) as u32 {
            return Err(format!(
                "Smaller Installer checksum mismatch: {}",
                entry.name
            ));
        }
        files.push(File {
            name: entry.name,
            file_type: m[5..9].try_into().unwrap(),
            creator: m[9..13].try_into().unwrap(),
            finder_flags: word(&m[21..]) as u16,
            data,
            rsrc,
        });
    }
    Ok(Some(files))
}

fn crc(bytes: impl Iterator<Item = u8>) -> u32 {
    let mut value = u32::MAX;
    for byte in bytes {
        value ^= byte as u32;
        for _ in 0..8 {
            value = (value >> 1) ^ (0xedb88320 & 0u32.wrapping_sub(value & 1));
        }
    }
    value
}

struct Bits<'a> {
    bytes: &'a [u8],
    position: usize,
}
impl Bits<'_> {
    fn read(&mut self, count: usize) -> Result<usize, String> {
        let mut value = 0;
        for _ in 0..count {
            let byte = self
                .bytes
                .get(self.position / 8)
                .ok_or("truncated Compact Pro bitstream")?;
            value = (value << 1) | ((byte >> (7 - self.position % 8)) & 1) as usize;
            self.position += 1;
        }
        Ok(value)
    }
}

#[derive(Default)]
struct Code {
    symbols: Vec<u8>,
    counts: [usize; 16],
    first_codes: [usize; 16],
    first_symbols: [usize; 16],
}
impl Code {
    fn read(bits: &mut Bits<'_>, count: usize) -> Result<Self, String> {
        let packed_count = bits.read(8)?;
        if packed_count * 2 > count {
            return Err("oversized Compact Pro prefix table".into());
        }
        let mut lengths = Vec::new();
        for pair in 0..packed_count {
            let byte = bits.read(8)?;
            for (symbol, length) in [(pair * 2, byte >> 4), (pair * 2 + 1, byte & 15)] {
                if length != 0 {
                    lengths.push((length, symbol));
                }
            }
        }
        lengths.sort_unstable();
        let mut code = 0;
        let mut previous = 0;
        let mut table = Self::default();
        for (length, symbol) in lengths {
            code <<= length - previous;
            if code >= 1 << length {
                return Err("oversubscribed Compact Pro prefix table".into());
            }
            if table.counts[length] == 0 {
                table.first_codes[length] = code;
                table.first_symbols[length] = table.symbols.len();
            }
            table.counts[length] += 1;
            table.symbols.push(symbol as u8);
            code += 1;
            previous = length;
        }
        Ok(table)
    }
    fn symbol(&self, bits: &mut Bits<'_>) -> Result<u8, String> {
        let mut code = 0;
        for length in 1..=15 {
            code = (code << 1) | bits.read(1)?;
            if let Some(index) = code.checked_sub(self.first_codes[length]) {
                if index < self.counts[length] {
                    return Ok(self.symbols[self.first_symbols[length] + index]);
                }
            }
        }
        Err("invalid Compact Pro prefix code".into())
    }
}

struct Lzh<'a> {
    bits: Bits<'a>,
    codes: [Code; 3],
    block_start: usize,
    charge: usize,
    initialized: bool,
    window: [u8; 8192],
    cursor: usize,
    distance: usize,
    remaining: usize,
}
impl<'a> Lzh<'a> {
    fn new(bytes: &'a [u8]) -> Self {
        Self {
            bits: Bits { bytes, position: 0 },
            codes: Default::default(),
            block_start: 0,
            charge: 0,
            initialized: false,
            window: [0; 8192],
            cursor: 0,
            distance: 0,
            remaining: 0,
        }
    }
    fn next(&mut self) -> Result<u8, String> {
        if self.remaining == 0 {
            if !self.initialized || self.charge >= 0x1fff0 {
                if self.initialized {
                    self.bits.position = self.bits.position.div_ceil(8) * 8;
                    let padding = if (self.bits.position / 8 - self.block_start) & 1 == 0 {
                        2
                    } else {
                        3
                    };
                    self.bits.read(padding * 8)?;
                }
                self.codes = [
                    Code::read(&mut self.bits, 256)?,
                    Code::read(&mut self.bits, 64)?,
                    Code::read(&mut self.bits, 128)?,
                ];
                self.block_start = self.bits.position / 8;
                self.charge = 0;
                self.initialized = true;
            }
            if self.bits.read(1)? != 0 {
                self.charge += 2;
                let byte = self.codes[0].symbol(&mut self.bits)?;
                return Ok(self.record(byte));
            }
            self.charge += 3;
            self.remaining = self.codes[1].symbol(&mut self.bits)? as usize;
            if self.remaining == 0 {
                return Err("zero-length Compact Pro match".into());
            }
            self.distance =
                ((self.codes[2].symbol(&mut self.bits)? as usize) << 6) | self.bits.read(6)?;
        }
        self.remaining -= 1;
        let byte = self.window[self.cursor.wrapping_sub(self.distance) & 8191];
        Ok(self.record(byte))
    }
    fn record(&mut self, byte: u8) -> u8 {
        self.window[self.cursor & 8191] = byte;
        self.cursor = self.cursor.wrapping_add(1);
        byte
    }
}

fn decode(bytes: &[u8], size: usize, compressed: bool) -> Result<Vec<u8>, String> {
    if size > MAX_EXPANDED {
        return Err("Compact Pro fork exceeds limit".into());
    }
    if size == 0 {
        return if bytes.is_empty() {
            Ok(Vec::new())
        } else {
            Err("nonempty packed zero-length fork".into())
        };
    }
    let mut lzh = compressed.then(|| Lzh::new(bytes));
    let mut position = 0;
    let mut next = || -> Result<u8, String> {
        if let Some(stream) = &mut lzh {
            stream.next()
        } else {
            Ok(take(bytes, &mut position, 1)?[0])
        }
    };
    let mut output = Vec::with_capacity(size);
    let mut previous = 0;
    let mut escaped = false;
    while output.len() < size {
        let byte = if escaped { 0x81 } else { next()? };
        escaped = false;
        if byte != 0x81 {
            output.push(byte);
            previous = byte;
            continue;
        }
        match next()? {
            0x82 => match next()? {
                0 => {
                    output.extend_from_slice(&[0x81, 0x82]);
                    previous = 0x82;
                }
                count @ 2..=255 => {
                    let run = count as usize - 1;
                    if run > size - output.len() {
                        return Err("Compact Pro run exceeds fork length".into());
                    }
                    output.resize(output.len() + run, previous);
                }
                _ => return Err("invalid Compact Pro repeat count".into()),
            },
            0x81 => {
                output.push(0x81);
                previous = 0x81;
                escaped = true;
            }
            byte => {
                output.extend_from_slice(&[0x81, byte]);
                previous = byte;
            }
        }
        if output.len() > size {
            return Err("Compact Pro escape exceeds fork length".into());
        }
    }
    Ok(output)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn simple_tables() -> Vec<u8> {
        // Only literal A, length 3 and offset high part 0 have a one-bit code.
        let mut bytes = vec![33];
        bytes.extend_from_slice(&[0; 32]);
        bytes.extend_from_slice(&[1, 2, 0, 1, 1, 0x10]);
        bytes
    }
    #[test]
    fn lzh_copies_overlapping_window_matches() {
        let mut bytes = simple_tables();
        // Literal A, then a three-byte match one byte back.
        bytes.extend_from_slice(&[0x80, 0x20]);
        assert_eq!(decode(&bytes, 4, true).unwrap(), b"AAAA");
        assert!(decode(&bytes[..bytes.len() - 1], 4, true).is_err());
        assert!(decode(&[2, 0x11, 0x10], 1, true).is_err());
    }
    #[test]
    fn lzh_refreshes_tables_at_block_boundary() {
        let mut bytes = simple_tables();
        // 65,528 literals exhaust a 0x1fff0 token budget, then aligned padding.
        bytes.extend_from_slice(&[0xaa; 16_382]);
        bytes.extend_from_slice(&[0; 2]);
        bytes.extend_from_slice(&simple_tables());
        bytes.push(0x80);
        assert_eq!(decode(&bytes, 65_529, true).unwrap(), vec![b'A'; 65_529]);
    }
    #[test]
    fn rle_handles_runs_and_escape_pairs() {
        assert_eq!(decode(&[b'A', 0x81, 0x82, 4], 4, false).unwrap(), b"AAAA");
        assert_eq!(decode(&[0x81, 0x82, 0], 2, false).unwrap(), [0x81, 0x82]);
        assert_eq!(decode(&[0x81, b'B'], 2, false).unwrap(), [0x81, b'B']);
        assert_eq!(
            decode(&[0x81, 0x81, b'B'], 3, false).unwrap(),
            [0x81, 0x81, b'B']
        );
    }
    #[test]
    fn malformed_streams_fail_without_partial_forks() {
        for stream in [
            &[0x81][..],
            &[0x81, 0x82][..],
            &[0x81, 0x82, 1][..],
            &[b'A', 0x81, 0x82, 255][..],
        ] {
            assert!(decode(stream, 4, false).is_err());
        }
        assert!(decode(&[0x81, b'B'], 1, false).is_err());
        assert!(decode(&[129], 10, true).is_err());
        assert!(decode(&[1], 0, false).is_err());
    }
    fn fixture() -> Vec<u8> {
        let mut bytes = vec![0; 10];
        bytes[4..6].copy_from_slice(&1u16.to_be_bytes());
        bytes.extend_from_slice(&[4, b'T', b'e', b's', b't']);
        let mut m = [0; 45];
        m[0] = 1;
        m[1..5].copy_from_slice(&60u32.to_be_bytes());
        m[5..9].copy_from_slice(b"TEXT");
        m[9..13].copy_from_slice(b"ttxt");
        m[23..27].copy_from_slice(&crc([b'A'].into_iter()).to_be_bytes());
        m[33..37].copy_from_slice(&1u32.to_be_bytes());
        m[41..45].copy_from_slice(&1u32.to_be_bytes());
        bytes.extend_from_slice(&m);
        bytes.push(b'A');
        bytes
    }
    #[test]
    fn installer_checks_metadata_and_crc() {
        let mut bytes = fixture();
        assert!(expand(&bytes, b"unrelated application").unwrap().is_none());
        let files = expand(&bytes, b"Smaller Installer").unwrap().unwrap();
        assert_eq!(files[0].data, b"A");
        assert_eq!(files[0].file_type, *b"TEXT");
        bytes[60] = b'B';
        assert!(expand(&bytes, b"Smaller Installer").is_err());
        bytes = fixture();
        bytes[42] = 1;
        assert!(expand(&bytes, b"Smaller Installer").is_err());
        bytes = fixture();
        bytes[16..20].copy_from_slice(&0u32.to_be_bytes());
        assert!(expand(&bytes, b"Smaller Installer").is_err());
        bytes = fixture();
        bytes[48..52].copy_from_slice(&u32::MAX.to_be_bytes());
        assert!(expand(&bytes, b"Smaller Installer").is_err());
    }
}
