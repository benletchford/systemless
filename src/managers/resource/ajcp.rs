//! Broderbund/Br0derbund-style `ajcp` resource decompression.
//!
//! The on-disk resource starts with:
//! - `ajcp` magic
//! - 32-bit big-endian decompressed length
//! - a dynamic-Huffman LZ stream with an 8 KiB sliding window

const MAGIC: &[u8; 4] = b"ajcp";
const MAX_OUTPUT_LEN: usize = 64 * 1024 * 1024;

const DIC_SIZE: usize = 0x2000;
const NC: usize = 0x1FE;
const NP: usize = 14;
const NT: usize = 19;
const CBIT: u8 = 9;
const PBIT: u8 = 4;
const TBIT: u8 = 5;
const C_TABLE_BITS: usize = 12;
const P_TABLE_BITS: usize = 8;
const C_TABLE_SIZE: usize = 1 << C_TABLE_BITS;
const P_TABLE_SIZE: usize = 1 << P_TABLE_BITS;
const TREE_SIZE: usize = NC * 2;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum AjcpDecompressError {
    TruncatedHeader,
    OutputTooLarge(usize),
    BadHuffmanTable,
    HuffmanTableOverflow,
    HuffmanSymbolOutOfRange(u16),
    BadBlockSize,
    LengthRunOverflow,
}

pub(crate) fn decompress_if_needed(data: &[u8]) -> Result<Option<Vec<u8>>, AjcpDecompressError> {
    if data.len() < MAGIC.len() || &data[..4] != MAGIC {
        return Ok(None);
    }
    if data.len() < 8 {
        return Err(AjcpDecompressError::TruncatedHeader);
    }

    let output_len = u32::from_be_bytes([data[4], data[5], data[6], data[7]]) as usize;
    if output_len > MAX_OUTPUT_LEN {
        return Err(AjcpDecompressError::OutputTooLarge(output_len));
    }

    let mut decoder = AjcpDecoder::new(&data[8..]);
    decoder.decode(output_len).map(Some)
}

struct AjcpDecoder<'a> {
    input: &'a [u8],
    bit_pos: i64,
    bitbuf: u32,
    block_size: u32,
    c_len: [u8; NC],
    pt_len: [u8; NT],
    c_table: [u16; C_TABLE_SIZE],
    pt_table: [u16; P_TABLE_SIZE],
    left: [u16; TREE_SIZE],
    right: [u16; TREE_SIZE],
}

impl<'a> AjcpDecoder<'a> {
    fn new(input: &'a [u8]) -> Self {
        let mut decoder = Self {
            input,
            bit_pos: -32,
            bitbuf: 0,
            block_size: 0,
            c_len: [0; NC],
            pt_len: [0; NT],
            c_table: [0; C_TABLE_SIZE],
            pt_table: [0; P_TABLE_SIZE],
            left: [0; TREE_SIZE],
            right: [0; TREE_SIZE],
        };
        decoder.fillbuf(32);
        decoder
    }

    fn decode(&mut self, output_len: usize) -> Result<Vec<u8>, AjcpDecompressError> {
        let mut out = Vec::with_capacity(output_len);
        let mut ring = [0u8; DIC_SIZE];
        let mut ring_pos = 0usize;

        while out.len() < output_len {
            let code = self.decode_c()?;
            if code <= 0xFF {
                let byte = code as u8;
                out.push(byte);
                ring[ring_pos] = byte;
                ring_pos = (ring_pos + 1) & (DIC_SIZE - 1);
            } else {
                let length = (code as usize).saturating_sub(0xFD);
                let distance = self.decode_p()? as usize;
                let mut src = ring_pos.wrapping_sub(distance + 1) & (DIC_SIZE - 1);
                for _ in 0..length {
                    let byte = ring[src];
                    src = (src + 1) & (DIC_SIZE - 1);
                    out.push(byte);
                    ring[ring_pos] = byte;
                    ring_pos = (ring_pos + 1) & (DIC_SIZE - 1);
                    if out.len() == output_len {
                        break;
                    }
                }
            }
        }

        Ok(out)
    }

    fn decode_c(&mut self) -> Result<u16, AjcpDecompressError> {
        if self.block_size == 0 {
            self.block_size = self.get_bits(16);
            if self.block_size == 0 {
                return Err(AjcpDecompressError::BadBlockSize);
            }
            self.read_pt_len(NT, TBIT, 3)?;
            self.read_c_len()?;
            self.read_pt_len(NP, PBIT, -1)?;
        }
        self.block_size -= 1;

        let mut symbol = self.c_table[(self.bitbuf >> (32 - C_TABLE_BITS)) as usize];
        if symbol as usize >= NC {
            let mut mask = 1u32 << (32 - 1 - C_TABLE_BITS);
            while symbol as usize >= NC {
                if mask == 0 {
                    return Err(AjcpDecompressError::HuffmanSymbolOutOfRange(symbol));
                }
                let idx = symbol as usize;
                if idx >= TREE_SIZE {
                    return Err(AjcpDecompressError::HuffmanSymbolOutOfRange(symbol));
                }
                symbol = if (self.bitbuf & mask) != 0 {
                    self.right[idx]
                } else {
                    self.left[idx]
                };
                mask >>= 1;
            }
        }

        self.fillbuf(self.c_len[symbol as usize] as u32);
        Ok(symbol)
    }

    fn decode_p(&mut self) -> Result<u16, AjcpDecompressError> {
        let mut symbol = self.pt_table[(self.bitbuf >> (32 - P_TABLE_BITS)) as usize];
        if symbol as usize >= NP {
            let mut mask = 1u32 << (32 - 1 - P_TABLE_BITS);
            while symbol as usize >= NP {
                if mask == 0 {
                    return Err(AjcpDecompressError::HuffmanSymbolOutOfRange(symbol));
                }
                let idx = symbol as usize;
                if idx >= TREE_SIZE {
                    return Err(AjcpDecompressError::HuffmanSymbolOutOfRange(symbol));
                }
                symbol = if (self.bitbuf & mask) != 0 {
                    self.right[idx]
                } else {
                    self.left[idx]
                };
                mask >>= 1;
            }
        }

        self.fillbuf(self.pt_len[symbol as usize] as u32);
        if symbol != 0 {
            let extra_bits = symbol - 1;
            symbol = (1u16 << extra_bits) + self.get_bits(extra_bits as u8) as u16;
        }
        Ok(symbol)
    }

    fn read_pt_len(
        &mut self,
        nn: usize,
        nbit: u8,
        i_special: isize,
    ) -> Result<(), AjcpDecompressError> {
        let n = self.get_bits(nbit) as usize;
        if n == 0 {
            let c = self.get_bits(nbit) as u16;
            self.pt_len[..nn].fill(0);
            self.pt_table.fill(c);
            return Ok(());
        }

        let mut i = 0usize;
        while i < n {
            let mut c = (self.bitbuf >> (32 - 3)) as u8;
            if c == 7 {
                let mut mask = 1u32 << (32 - 1 - 3);
                while (self.bitbuf & mask) != 0 {
                    mask >>= 1;
                    c = c.saturating_add(1);
                }
            }
            self.fillbuf(if c < 7 { 3 } else { u32::from(c - 3) });
            if i >= nn {
                return Err(AjcpDecompressError::LengthRunOverflow);
            }
            self.pt_len[i] = c;
            i += 1;

            if i as isize == i_special {
                let zero_count = self.get_bits(2) as usize;
                if i + zero_count > nn {
                    return Err(AjcpDecompressError::LengthRunOverflow);
                }
                self.pt_len[i..i + zero_count].fill(0);
                i += zero_count;
            }
        }
        if i < nn {
            self.pt_len[i..nn].fill(0);
        }

        Self::make_table(
            nn,
            &self.pt_len,
            P_TABLE_BITS,
            &mut self.pt_table,
            &mut self.left,
            &mut self.right,
        )
    }

    fn read_c_len(&mut self) -> Result<(), AjcpDecompressError> {
        let n = self.get_bits(CBIT) as usize;
        if n == 0 {
            let c = self.get_bits(CBIT) as u16;
            self.c_len.fill(0);
            self.c_table.fill(c);
            return Ok(());
        }

        let mut i = 0usize;
        while i < n {
            let mut c = self.pt_table[(self.bitbuf >> (32 - P_TABLE_BITS)) as usize];
            if c as usize >= NT {
                let mut mask = 1u32 << (32 - 1 - P_TABLE_BITS);
                while c as usize >= NT {
                    if mask == 0 {
                        return Err(AjcpDecompressError::HuffmanSymbolOutOfRange(c));
                    }
                    let idx = c as usize;
                    if idx >= TREE_SIZE {
                        return Err(AjcpDecompressError::HuffmanSymbolOutOfRange(c));
                    }
                    c = if (self.bitbuf & mask) != 0 {
                        self.right[idx]
                    } else {
                        self.left[idx]
                    };
                    mask >>= 1;
                }
            }

            self.fillbuf(self.pt_len[c as usize] as u32);
            if c <= 2 {
                let zero_count = match c {
                    0 => 1,
                    1 => self.get_bits(4) as usize + 3,
                    _ => self.get_bits(CBIT) as usize + 20,
                };
                if i + zero_count > NC {
                    return Err(AjcpDecompressError::LengthRunOverflow);
                }
                self.c_len[i..i + zero_count].fill(0);
                i += zero_count;
            } else {
                if i >= NC {
                    return Err(AjcpDecompressError::LengthRunOverflow);
                }
                self.c_len[i] = (c - 2) as u8;
                i += 1;
            }
        }
        if i < NC {
            self.c_len[i..].fill(0);
        }

        Self::make_table(
            NC,
            &self.c_len,
            C_TABLE_BITS,
            &mut self.c_table,
            &mut self.left,
            &mut self.right,
        )
    }

    fn fillbuf(&mut self, bits: u32) {
        self.bit_pos += i64::from(bits);
        self.bitbuf = self.extract_bits(self.bit_pos, 32);
    }

    fn get_bits(&mut self, bits: u8) -> u32 {
        let value = if bits == 0 {
            0
        } else {
            self.bitbuf >> (32 - bits)
        };
        self.fillbuf(u32::from(bits));
        value
    }

    fn extract_bits(&self, bit_pos: i64, width: usize) -> u32 {
        let mut value = 0u32;
        for offset in 0..width {
            let pos = bit_pos + offset as i64;
            let bit = if pos < 0 {
                0
            } else {
                let byte_index = (pos as usize) / 8;
                let bit_index = (pos as usize) % 8;
                self.input
                    .get(byte_index)
                    .map(|byte| (byte >> (7 - bit_index)) & 1)
                    .unwrap_or(0)
            };
            value = (value << 1) | u32::from(bit);
        }
        value
    }

    fn make_table(
        nchar: usize,
        bitlen: &[u8],
        table_bits: usize,
        table: &mut [u16],
        left: &mut [u16; TREE_SIZE],
        right: &mut [u16; TREE_SIZE],
    ) -> Result<(), AjcpDecompressError> {
        #[derive(Clone, Copy)]
        enum Slot {
            Table(usize),
            Left(usize),
            Right(usize),
        }

        fn slot_value(slot: Slot, table: &[u16], left: &[u16], right: &[u16]) -> u16 {
            match slot {
                Slot::Table(i) => table[i],
                Slot::Left(i) => left[i],
                Slot::Right(i) => right[i],
            }
        }

        fn set_slot(
            slot: Slot,
            value: u16,
            table: &mut [u16],
            left: &mut [u16],
            right: &mut [u16],
        ) {
            match slot {
                Slot::Table(i) => table[i] = value,
                Slot::Left(i) => left[i] = value,
                Slot::Right(i) => right[i] = value,
            }
        }

        let mut count = [0u32; 17];
        for &len in bitlen.iter().take(nchar) {
            if len as usize > 16 {
                return Err(AjcpDecompressError::BadHuffmanTable);
            }
            count[len as usize] += 1;
        }

        let mut start = [0u32; 18];
        for i in 1..=16 {
            start[i + 1] = start[i] + (count[i] << (16 - i));
        }
        if start[17] != 0x1_0000 {
            return Err(AjcpDecompressError::BadHuffmanTable);
        }

        let jut_bits = 16 - table_bits;
        for value in start.iter_mut().take(table_bits + 1).skip(1) {
            *value >>= jut_bits;
        }

        let mut weight = [0u32; 17];
        for (i, value) in weight.iter_mut().enumerate().take(table_bits + 1).skip(1) {
            *value = 1 << (table_bits - i);
        }
        for (i, value) in weight.iter_mut().enumerate().skip(table_bits + 1) {
            *value = 1 << (16 - i);
        }

        let fill_from = (start[table_bits + 1] >> jut_bits) as usize;
        if fill_from < table.len() {
            table[fill_from..].fill(0);
        }

        let mut avail = nchar;
        let mask = 1u32 << (15 - table_bits);
        for ch in 0..nchar {
            let len = bitlen[ch] as usize;
            if len == 0 {
                continue;
            }

            let next_code = start[len] + weight[len];
            if len <= table_bits {
                let start_idx = start[len] as usize;
                let end_idx = next_code as usize;
                if end_idx > table.len() {
                    return Err(AjcpDecompressError::BadHuffmanTable);
                }
                table[start_idx..end_idx].fill(ch as u16);
            } else {
                let mut code = start[len];
                let table_idx = (code >> jut_bits) as usize;
                if table_idx >= table.len() {
                    return Err(AjcpDecompressError::BadHuffmanTable);
                }
                let mut slot = Slot::Table(table_idx);

                for _ in 0..(len - table_bits) {
                    let mut node = slot_value(slot, table, left, right);
                    if node == 0 {
                        if avail >= TREE_SIZE {
                            return Err(AjcpDecompressError::HuffmanTableOverflow);
                        }
                        left[avail] = 0;
                        right[avail] = 0;
                        node = avail as u16;
                        set_slot(slot, node, table, left, right);
                        avail += 1;
                    }

                    slot = if (code & mask) != 0 {
                        Slot::Right(node as usize)
                    } else {
                        Slot::Left(node as usize)
                    };
                    code <<= 1;
                }

                set_slot(slot, ch as u16, table, left, right);
            }

            start[len] = next_code;
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::decompress_if_needed;

    fn pack_bits(bits: &str) -> Vec<u8> {
        let mut out = Vec::new();
        let mut byte = 0u8;
        for (idx, bit) in bits.bytes().enumerate() {
            byte <<= 1;
            if bit == b'1' {
                byte |= 1;
            }
            if idx % 8 == 7 {
                out.push(byte);
                byte = 0;
            }
        }
        let rem = bits.len() % 8;
        if rem != 0 {
            byte <<= 8 - rem;
            out.push(byte);
        }
        out
    }

    #[test]
    fn literal_only_decompresses() {
        let mut bits = String::new();
        bits.push_str("0000000000000001"); // block size: 1 symbol
        bits.push_str("00000"); // empty PT length table
        bits.push_str("00000"); // PT table resolves to 0
        bits.push_str("000000000"); // empty C length table
        bits.push_str("001000001"); // C table resolves to literal 'A'
        bits.push_str("0000"); // empty position length table
        bits.push_str("0000"); // position table resolves to 0

        let mut data = Vec::new();
        data.extend_from_slice(b"ajcp");
        data.extend_from_slice(&1u32.to_be_bytes());
        data.extend_from_slice(&pack_bits(&bits));

        let decoded = decompress_if_needed(&data).unwrap().unwrap();
        assert_eq!(decoded, b"A");
    }

    #[test]
    fn non_ajcp_data_is_ignored() {
        assert_eq!(decompress_if_needed(b"plain").unwrap(), None);
    }
}
