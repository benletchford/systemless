//! StuffIt InstallerMaker container helpers.
//!
//! InstallerMaker 4.x self-extracting applications store the install payload
//! in the application data fork.  Gridz 1.2 uses an `ST46` data fork with
//! classic StuffIt-style 112-byte file headers followed by compressed
//! resource and data fork streams.

use crate::trap::types::decode_mac_roman;

const ST46_MAGIC: &[u8; 4] = b"ST46";
const ST46_ENTRY_COUNT_OFFSET: usize = 0x44;
const ST46_FIRST_ENTRY_OFFSET_OFFSET: usize = 0x54;
const ST46_ENTRY_HEADER_LEN: usize = 112;
const ST46_RSRC_METHOD_OFFSET: usize = 0;
const ST46_DATA_METHOD_OFFSET: usize = 1;
const ST46_NAME_LEN_OFFSET: usize = 2;
const ST46_NAME_OFFSET: usize = 3;
const ST46_NAME_FIELD_LEN: usize = 32;
const ST46_FILE_TYPE_OFFSET: usize = 66;
const ST46_CREATOR_OFFSET: usize = 70;
const ST46_FINDER_FLAGS_OFFSET: usize = 74;
const ST46_RSRC_UNPACKED_LEN_OFFSET: usize = 84;
const ST46_DATA_UNPACKED_LEN_OFFSET: usize = 88;
const ST46_RSRC_PACKED_LEN_OFFSET: usize = 92;
const ST46_DATA_PACKED_LEN_OFFSET: usize = 96;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InstallerMakerContainer<'a> {
    pub entries: Vec<InstallerMakerEntry<'a>>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InstallerMakerEntry<'a> {
    pub name: String,
    pub header_offset: usize,
    pub file_type: [u8; 4],
    pub creator: [u8; 4],
    pub finder_flags: u16,
    pub rsrc_method: u8,
    pub data_method: u8,
    pub rsrc_packed_offset: usize,
    pub data_packed_offset: usize,
    pub rsrc_packed_len: usize,
    pub data_packed_len: usize,
    pub rsrc_unpacked_len: usize,
    pub data_unpacked_len: usize,
    pub rsrc_packed: &'a [u8],
    pub data_packed: &'a [u8],
}

pub fn parse_installer_maker_st46(data: &[u8]) -> Option<InstallerMakerContainer<'_>> {
    parse_installer_maker_st46_result(data).ok()
}

pub fn parse_installer_maker_st46_result(
    data: &[u8],
) -> Result<InstallerMakerContainer<'_>, String> {
    if !data.starts_with(ST46_MAGIC) {
        return Err("missing ST46 signature".to_string());
    }

    let entry_count = read_u32_be(data, ST46_ENTRY_COUNT_OFFSET, "entry count")? as usize;
    let mut header_offset =
        read_u32_be(data, ST46_FIRST_ENTRY_OFFSET_OFFSET, "first entry offset")? as usize;
    let mut entries = Vec::with_capacity(entry_count);

    for index in 0..entry_count {
        let header = get_range(
            data,
            header_offset,
            ST46_ENTRY_HEADER_LEN,
            &format!("entry {index} header"),
        )?;
        let name_len = usize::from(header[ST46_NAME_LEN_OFFSET]).min(ST46_NAME_FIELD_LEN);
        let name = decode_mac_roman(get_range(
            header,
            ST46_NAME_OFFSET,
            name_len,
            &format!("entry {index} name"),
        )?);

        let rsrc_packed_len = read_u32_be(
            header,
            ST46_RSRC_PACKED_LEN_OFFSET,
            "resource packed length",
        )? as usize;
        let data_packed_len =
            read_u32_be(header, ST46_DATA_PACKED_LEN_OFFSET, "data packed length")? as usize;
        let rsrc_unpacked_len = read_u32_be(
            header,
            ST46_RSRC_UNPACKED_LEN_OFFSET,
            "resource unpacked length",
        )? as usize;
        let data_unpacked_len = read_u32_be(
            header,
            ST46_DATA_UNPACKED_LEN_OFFSET,
            "data unpacked length",
        )? as usize;
        let rsrc_packed_offset = header_offset
            .checked_add(ST46_ENTRY_HEADER_LEN)
            .ok_or_else(|| format!("entry {index} resource offset overflow"))?;
        let data_packed_offset = rsrc_packed_offset
            .checked_add(rsrc_packed_len)
            .ok_or_else(|| format!("entry {index} data offset overflow"))?;
        let next_header_offset = data_packed_offset
            .checked_add(data_packed_len)
            .ok_or_else(|| format!("entry {index} next header offset overflow"))?;

        let rsrc_packed = get_range(
            data,
            rsrc_packed_offset,
            rsrc_packed_len,
            &format!("entry {index} resource stream"),
        )?;
        let data_packed = get_range(
            data,
            data_packed_offset,
            data_packed_len,
            &format!("entry {index} data stream"),
        )?;

        let mut file_type = [0u8; 4];
        file_type.copy_from_slice(get_range(
            header,
            ST46_FILE_TYPE_OFFSET,
            4,
            &format!("entry {index} file type"),
        )?);
        let mut creator = [0u8; 4];
        creator.copy_from_slice(get_range(
            header,
            ST46_CREATOR_OFFSET,
            4,
            &format!("entry {index} creator"),
        )?);

        entries.push(InstallerMakerEntry {
            name,
            header_offset,
            file_type,
            creator,
            finder_flags: read_u16_be(header, ST46_FINDER_FLAGS_OFFSET, "finder flags")?,
            rsrc_method: header[ST46_RSRC_METHOD_OFFSET],
            data_method: header[ST46_DATA_METHOD_OFFSET],
            rsrc_packed_offset,
            data_packed_offset,
            rsrc_packed_len,
            data_packed_len,
            rsrc_unpacked_len,
            data_unpacked_len,
            rsrc_packed,
            data_packed,
        });

        header_offset = next_header_offset;
    }

    Ok(InstallerMakerContainer { entries })
}

pub fn decode_installer_method14(data: &[u8], expected_len: usize) -> Result<Vec<u8>, String> {
    let mut decoder = Installer14Decoder::new(data, expected_len);
    decoder.decode()
}

fn get_range<'a>(
    data: &'a [u8],
    offset: usize,
    len: usize,
    label: &str,
) -> Result<&'a [u8], String> {
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

fn read_u16_be(data: &[u8], offset: usize, label: &str) -> Result<u16, String> {
    Ok(u16::from_be_bytes(
        get_range(data, offset, 2, label)?.try_into().unwrap(),
    ))
}

fn read_u32_be(data: &[u8], offset: usize, label: &str) -> Result<u32, String> {
    Ok(u32::from_be_bytes(
        get_range(data, offset, 4, label)?.try_into().unwrap(),
    ))
}

struct BitReader<'a> {
    data: &'a [u8],
    pos: usize,
    bit_buf: u32,
    bit_count: u8,
}

impl<'a> BitReader<'a> {
    fn new(data: &'a [u8]) -> Self {
        Self {
            data,
            pos: 0,
            bit_buf: 0,
            bit_count: 0,
        }
    }

    fn get_bits_low(&mut self, bits: u8) -> Result<u32, String> {
        while self.bit_count < bits {
            let Some(&byte) = self.data.get(self.pos) else {
                return Err("method14 input truncated".to_string());
            };
            self.pos += 1;
            self.bit_buf |= u32::from(byte) << self.bit_count;
            self.bit_count += 8;
        }
        let mask = if bits == 32 {
            u32::MAX
        } else {
            (1u32 << bits) - 1
        };
        let value = self.bit_buf & mask;
        self.bit_buf >>= bits;
        self.bit_count -= bits;
        Ok(value)
    }

    fn byte_boundary(&mut self) {
        self.bit_buf = 0;
        self.bit_count = 0;
    }
}

struct Installer14Decoder<'a> {
    input: BitReader<'a>,
    output: Vec<u8>,
    expected_len: usize,
    code: [u8; 308],
    codecopy: [u8; 308],
    freq: [u16; 616],
    buff: [u32; 308],
    var1: [u8; 52],
    var2: [u16; 52],
    var3: [u16; 150],
    var4: [u8; 76],
    var5: [u32; 75],
    var6: [u8; 1024],
    var7: [u16; 616],
    var8: [u8; 0x4000],
    window: Vec<u8>,
    window_pos: usize,
}

impl<'a> Installer14Decoder<'a> {
    fn new(data: &'a [u8], expected_len: usize) -> Self {
        Self {
            input: BitReader::new(data),
            output: Vec::with_capacity(expected_len),
            expected_len,
            code: [0; 308],
            codecopy: [0; 308],
            freq: [0; 616],
            buff: [0; 308],
            var1: [0; 52],
            var2: [0; 52],
            var3: [0; 150],
            var4: [0; 76],
            var5: [0; 75],
            var6: [0; 1024],
            var7: [0; 616],
            var8: [0; 0x4000],
            window: vec![0; 0x40000],
            window_pos: 0,
        }
    }

    fn decode(&mut self) -> Result<Vec<u8>, String> {
        self.init_tables();
        let mut blocks = self.input.get_bits_low(16)? as usize;
        while blocks > 0 && self.output.len() < self.expected_len {
            blocks -= 1;
            let _packed_low = self.input.get_bits_low(16)?;
            let _packed_high = self.input.get_bits_low(16)?;
            let mut block_len = self.input.get_bits_low(16)? as usize;
            block_len |= (self.input.get_bits_low(16)? as usize) << 16;

            let mut literal_tree = [0u16; 616];
            self.read_tree(308, &mut literal_tree)?;
            self.var7.copy_from_slice(&literal_tree);
            let mut distance_tree = [0u16; 150];
            self.read_tree(75, &mut distance_tree)?;
            self.var3.copy_from_slice(&distance_tree);

            while block_len > 0 && self.output.len() < self.expected_len {
                let symbol = self.read_symbol_308()?;
                if symbol < 0x100 {
                    self.put_byte(symbol as u8)?;
                    block_len -= 1;
                } else {
                    let length_symbol = symbol - 0x100;
                    let length_index = usize::from(length_symbol);
                    let mut copy_len = usize::from(self.var2[length_index]) + 4;
                    let extra_bits = self.var1[length_index];
                    if extra_bits != 0 {
                        copy_len += self.input.get_bits_low(extra_bits)? as usize;
                    }

                    let distance_symbol = self.read_symbol_75()?;
                    let distance_index = usize::from(distance_symbol);
                    let mut distance = self.var5[distance_index] as usize;
                    let extra_bits = self.var4[distance_index];
                    if extra_bits != 0 {
                        distance += self.input.get_bits_low(extra_bits)? as usize;
                    }

                    if copy_len > block_len {
                        return Err(format!(
                            "method14 copy length {copy_len} exceeds remaining block {block_len}"
                        ));
                    }
                    block_len -= copy_len;
                    let mut source = self.window_pos + 0x40000 - distance;
                    for _ in 0..copy_len {
                        source &= 0x3ffff;
                        let byte = self.window[source];
                        source += 1;
                        self.put_byte(byte)?;
                    }
                }
            }
            self.input.byte_boundary();
        }

        if self.output.len() != self.expected_len {
            return Err(format!(
                "method14 decoded {} bytes, expected {}",
                self.output.len(),
                self.expected_len
            ));
        }
        Ok(std::mem::take(&mut self.output))
    }

    fn init_tables(&mut self) {
        let mut k = 0u16;
        for i in 0..52 {
            self.var2[i] = k;
            self.var1[i] = if i >= 4 { ((i - 4) >> 2) as u8 } else { 0 };
            k = k.wrapping_add(1u16 << self.var1[i]);
        }

        for i in 0..4 {
            self.var8[i] = i as u8;
        }
        let mut i = 4usize;
        let mut m = 1usize;
        let mut l = 4u8;
        while i < 0x4000 {
            let end = l + 4;
            while l < end {
                for _ in 0..m {
                    self.var8[i] = l;
                    i += 1;
                }
                l += 1;
            }
            m <<= 1;
        }

        let mut k = 1u32;
        for i in 0..75 {
            self.var5[i] = k;
            self.var4[i] = if i >= 3 { ((i - 3) >> 2) as u8 } else { 0 };
            k = k.wrapping_add(1u32 << self.var4[i]);
        }

        self.var6[0] = 0xff;
        self.var6[1] = 0;
        self.var6[2] = 1;
        self.var6[3] = 2;
        let mut i = 4usize;
        let mut m = 1usize;
        let mut l = 3u8;
        while i < 0x400 {
            let end = l + 4;
            while l < end {
                for _ in 0..m {
                    self.var6[i] = l;
                    i += 1;
                }
                l += 1;
            }
            m <<= 1;
        }
    }

    fn read_tree(&mut self, code_size: usize, result: &mut [u16]) -> Result<(), String> {
        let marker_bit = self.input.get_bits_low(1)? != 0;
        let j = self.input.get_bits_low(2)? as usize + 2;
        let o = self.input.get_bits_low(3)? as u8 + 1;
        let size = 1usize << j;
        let m = size - 1;
        let k = if marker_bit { Some(m - 1) } else { None };

        if (self.input.get_bits_low(2)? & 1) != 0 {
            let mut nested = vec![0u16; size * 2];
            self.read_tree(size, &mut nested)?;
            let mut i = 0usize;
            while i < code_size {
                let mut l = self.decode_tree_value(&nested, size)?;
                if k != Some(l) {
                    if l == m {
                        l = self.decode_tree_value(&nested, size)?;
                        let repeats = l + 3;
                        if i == 0 {
                            return Err("method14 repeat before first code".to_string());
                        }
                        for _ in 0..repeats {
                            if i >= code_size {
                                return Err("method14 tree repeat exceeds code size".to_string());
                            }
                            self.code[i] = self.code[i - 1];
                            i += 1;
                        }
                    } else {
                        self.code[i] = l as u8 + o;
                        i += 1;
                    }
                } else {
                    self.code[i] = 0;
                    i += 1;
                }
            }
        } else {
            let mut i = 0usize;
            while i < code_size {
                let l = self.input.get_bits_low(j as u8)? as usize;
                if k != Some(l) {
                    if l == m {
                        let repeats = self.input.get_bits_low(j as u8)? as usize + 3;
                        if i == 0 {
                            return Err("method14 repeat before first code".to_string());
                        }
                        for _ in 0..repeats {
                            if i >= code_size {
                                return Err("method14 tree repeat exceeds code size".to_string());
                            }
                            self.code[i] = self.code[i - 1];
                            i += 1;
                        }
                    } else {
                        self.code[i] = l as u8 + o;
                        i += 1;
                    }
                } else {
                    self.code[i] = 0;
                    i += 1;
                }
            }
        }

        for i in 0..code_size {
            self.codecopy[i] = self.code[i];
            self.freq[i] = i as u16;
        }
        sit14_update(0, code_size, &mut self.codecopy, &mut self.freq);

        let mut i = 0usize;
        while i < code_size && self.codecopy[i] == 0 {
            i += 1;
        }
        let mut j_value = 0u32;
        while i < code_size {
            if i != 0 {
                let shift = self.codecopy[i].saturating_sub(self.codecopy[i - 1]);
                j_value <<= shift;
            }

            let mut k_bits = self.codecopy[i];
            let mut m_bits = 0u32;
            let mut l_bits = j_value;
            while k_bits != 0 {
                m_bits = (m_bits << 1) | (l_bits & 1);
                l_bits >>= 1;
                k_bits -= 1;
            }
            let freq_index = usize::from(self.freq[i]);
            self.buff[freq_index] = m_bits;

            i += 1;
            j_value += 1;
        }

        result.fill(0);
        let mut next_node = 2u16;
        for i in 0..code_size {
            let mut node = 0usize;
            let mut bits = self.buff[i];
            for bit_index in 0..self.code[i] {
                node += (bits & 1) as usize;
                if usize::from(bit_index) + 1 >= usize::from(self.code[i]) {
                    result[node] = (code_size * 2 + i) as u16;
                } else {
                    if result[node] == 0 {
                        if usize::from(next_node) + 1 >= result.len() {
                            return Err("method14 tree node overflow".to_string());
                        }
                        result[node] = next_node;
                        next_node += 2;
                    }
                    node = usize::from(result[node]);
                }
                bits >>= 1;
            }
        }
        self.input.byte_boundary();
        Ok(())
    }

    fn decode_tree_value(&mut self, tree: &[u16], code_size: usize) -> Result<usize, String> {
        let mut node = 0usize;
        loop {
            let bit = self.input.get_bits_low(1)? as usize;
            node = usize::from(*tree.get(node + bit).ok_or_else(|| {
                format!(
                    "method14 tree branch {} out of range {}",
                    node + bit,
                    tree.len()
                )
            })?);
            if node >= code_size * 2 {
                return Ok(node - code_size * 2);
            }
        }
    }

    fn read_symbol_308(&mut self) -> Result<u16, String> {
        let tree = self.var7;
        self.decode_symbol(&tree, 308)
    }

    fn read_symbol_75(&mut self) -> Result<u16, String> {
        let tree = self.var3;
        self.decode_symbol(&tree, 75)
    }

    fn decode_symbol(&mut self, tree: &[u16], code_size: usize) -> Result<u16, String> {
        let mut node = 0usize;
        loop {
            let bit = self.input.get_bits_low(1)? as usize;
            node = usize::from(*tree.get(node + bit).ok_or_else(|| {
                format!(
                    "method14 symbol branch {} out of range {}",
                    node + bit,
                    tree.len()
                )
            })?);
            if node >= code_size * 2 {
                return Ok((node - code_size * 2) as u16);
            }
        }
    }

    fn put_byte(&mut self, byte: u8) -> Result<(), String> {
        if self.output.len() >= self.expected_len {
            return Err("method14 output exceeded expected length".to_string());
        }
        self.window[self.window_pos] = byte;
        self.window_pos = (self.window_pos + 1) & 0x3ffff;
        self.output.push(byte);
        Ok(())
    }
}

fn sit14_update(first: usize, last: usize, code: &mut [u8; 308], freq: &mut [u16; 616]) {
    let mut first = first;
    let mut last = last;
    while last - first > 1 {
        let mut i = first;
        let mut j = last;
        loop {
            while {
                i += 1;
                i < last && code[first] > code[i]
            } {}
            while {
                j -= 1;
                j > first && code[first] < code[j]
            } {}
            if j <= i {
                break;
            }
            code.swap(i, j);
            freq.swap(i, j);
        }

        if first != j {
            code.swap(first, j);
            freq.swap(first, j);
            i = j + 1;
            if last - i <= j - first {
                sit14_update(i, last, code, freq);
                last = j;
            } else {
                sit14_update(first, j, code, freq);
                first = i;
            }
        } else {
            first += 1;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_st46_entry_table_and_fork_ranges() {
        let mut data = vec![0u8; 0x86];
        data[0..4].copy_from_slice(b"ST46");
        data[ST46_ENTRY_COUNT_OFFSET..ST46_ENTRY_COUNT_OFFSET + 4]
            .copy_from_slice(&1u32.to_be_bytes());
        data[ST46_FIRST_ENTRY_OFFSET_OFFSET..ST46_FIRST_ENTRY_OFFSET_OFFSET + 4]
            .copy_from_slice(&0x86u32.to_be_bytes());

        let mut header = [0u8; ST46_ENTRY_HEADER_LEN];
        header[ST46_RSRC_METHOD_OFFSET] = 14;
        header[ST46_DATA_METHOD_OFFSET] = 14;
        header[ST46_NAME_LEN_OFFSET] = 8;
        header[ST46_NAME_OFFSET..ST46_NAME_OFFSET + 8].copy_from_slice(b"Test App");
        header[ST46_FILE_TYPE_OFFSET..ST46_FILE_TYPE_OFFSET + 4].copy_from_slice(b"APPL");
        header[ST46_CREATOR_OFFSET..ST46_CREATOR_OFFSET + 4].copy_from_slice(b"TEST");
        header[ST46_FINDER_FLAGS_OFFSET..ST46_FINDER_FLAGS_OFFSET + 2]
            .copy_from_slice(&0x2540u16.to_be_bytes());
        header[ST46_RSRC_UNPACKED_LEN_OFFSET..ST46_RSRC_UNPACKED_LEN_OFFSET + 4]
            .copy_from_slice(&5u32.to_be_bytes());
        header[ST46_DATA_UNPACKED_LEN_OFFSET..ST46_DATA_UNPACKED_LEN_OFFSET + 4]
            .copy_from_slice(&6u32.to_be_bytes());
        header[ST46_RSRC_PACKED_LEN_OFFSET..ST46_RSRC_PACKED_LEN_OFFSET + 4]
            .copy_from_slice(&2u32.to_be_bytes());
        header[ST46_DATA_PACKED_LEN_OFFSET..ST46_DATA_PACKED_LEN_OFFSET + 4]
            .copy_from_slice(&3u32.to_be_bytes());
        data.extend_from_slice(&header);
        data.extend_from_slice(b"rs");
        data.extend_from_slice(b"dat");

        let parsed = parse_installer_maker_st46(&data).expect("ST46 should parse");

        assert_eq!(parsed.entries.len(), 1);
        let entry = &parsed.entries[0];
        assert_eq!(entry.name, "Test App");
        assert_eq!(entry.header_offset, 0x86);
        assert_eq!(entry.file_type, *b"APPL");
        assert_eq!(entry.creator, *b"TEST");
        assert_eq!(entry.finder_flags, 0x2540);
        assert_eq!(entry.rsrc_method, 14);
        assert_eq!(entry.data_method, 14);
        assert_eq!(entry.rsrc_packed_offset, 0x86 + ST46_ENTRY_HEADER_LEN);
        assert_eq!(entry.data_packed_offset, 0x86 + ST46_ENTRY_HEADER_LEN + 2);
        assert_eq!(entry.rsrc_packed, b"rs");
        assert_eq!(entry.data_packed, b"dat");
        assert_eq!(entry.rsrc_unpacked_len, 5);
        assert_eq!(entry.data_unpacked_len, 6);
    }
}
