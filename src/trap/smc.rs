//! Apple Graphics (QuickTime `smc `) 8-bit image decoder.
//!
//! SMC divides an indexed image into 4×4 blocks. Each opcode either
//! preserves/repeats earlier blocks or encodes a block with 1, 2, 4, 8, or
//! 16 palette indices. The color-pair/quad/octet dictionaries persist across
//! frames, as required for temporally compressed QuickTime sequences.

#[derive(Clone, Debug)]
pub(crate) struct SmcDecoder {
    width: usize,
    height: usize,
    frame: Vec<u8>,
    color_pairs: [[u8; 2]; 256],
    color_quads: [[u8; 4]; 256],
    color_octets: [[u8; 8]; 256],
}

impl Default for SmcDecoder {
    fn default() -> Self {
        Self {
            width: 0,
            height: 0,
            frame: Vec::new(),
            color_pairs: [[0; 2]; 256],
            color_quads: [[0; 4]; 256],
            color_octets: [[0; 8]; 256],
        }
    }
}

impl SmcDecoder {
    pub(crate) fn new(width: usize, height: usize) -> Self {
        Self {
            width,
            height,
            frame: vec![0; width.saturating_mul(height)],
            ..Self::default()
        }
    }

    pub(crate) fn decode(&mut self, data: &[u8]) -> Result<&[u8], &'static str> {
        if self.width == 0 || self.height == 0 || data.len() < 4 {
            return Err("invalid SMC dimensions or frame header");
        }

        let block_columns = self.width.div_ceil(4);
        let block_rows = self.height.div_ceil(4);
        let total_blocks = block_columns.saturating_mul(block_rows);
        let mut block = 0usize;
        let mut pos = 4usize; // flags byte + 24-bit frame size
        let mut pair_index = 0usize;
        let mut quad_index = 0usize;
        let mut octet_index = 0usize;

        while block < total_blocks {
            let opcode = read_u8(data, &mut pos)?;
            let family = opcode & 0xF0;
            match family {
                0x00 | 0x10 => {
                    let count = block_run_count(opcode, data, &mut pos)?;
                    block = block.saturating_add(count).min(total_blocks);
                }
                0x20 | 0x30 => {
                    let count = block_run_count(opcode, data, &mut pos)?;
                    if block == 0 {
                        return Err("SMC repeat-before-first-block");
                    }
                    let source = block - 1;
                    for _ in 0..count {
                        if block >= total_blocks {
                            break;
                        }
                        self.copy_block(source, block, block_columns);
                        block += 1;
                    }
                }
                0x40 | 0x50 => {
                    let count = block_run_count(opcode, data, &mut pos)?.saturating_mul(2);
                    if block < 2 {
                        return Err("SMC pair-repeat-before-two-blocks");
                    }
                    let sources = [block - 2, block - 1];
                    for index in 0..count {
                        if block >= total_blocks {
                            break;
                        }
                        self.copy_block(sources[index & 1], block, block_columns);
                        block += 1;
                    }
                }
                0x60 | 0x70 => {
                    let count = block_run_count(opcode, data, &mut pos)?;
                    let color = read_u8(data, &mut pos)?;
                    for _ in 0..count {
                        if block >= total_blocks {
                            break;
                        }
                        self.fill_block(block, block_columns, [color; 16]);
                        block += 1;
                    }
                }
                0x80 | 0x90 => {
                    let count = usize::from(opcode & 0x0F) + 1;
                    let colors = if family == 0x80 {
                        let colors = [read_u8(data, &mut pos)?, read_u8(data, &mut pos)?];
                        self.color_pairs[pair_index] = colors;
                        pair_index = (pair_index + 1) & 0xFF;
                        colors
                    } else {
                        self.color_pairs[usize::from(read_u8(data, &mut pos)?)]
                    };
                    for _ in 0..count {
                        let flags = read_be_u16(data, &mut pos)?;
                        let mut pixels = [0u8; 16];
                        for (index, pixel) in pixels.iter_mut().enumerate() {
                            *pixel = colors[usize::from((flags >> (15 - index)) & 1)];
                        }
                        if block < total_blocks {
                            self.fill_block(block, block_columns, pixels);
                            block += 1;
                        }
                    }
                }
                0xA0 | 0xB0 => {
                    let count = usize::from(opcode & 0x0F) + 1;
                    let colors = if family == 0xA0 {
                        let colors = [
                            read_u8(data, &mut pos)?,
                            read_u8(data, &mut pos)?,
                            read_u8(data, &mut pos)?,
                            read_u8(data, &mut pos)?,
                        ];
                        self.color_quads[quad_index] = colors;
                        quad_index = (quad_index + 1) & 0xFF;
                        colors
                    } else {
                        self.color_quads[usize::from(read_u8(data, &mut pos)?)]
                    };
                    for _ in 0..count {
                        let flags = read_be_u32(data, &mut pos)?;
                        let mut pixels = [0u8; 16];
                        for (index, pixel) in pixels.iter_mut().enumerate() {
                            let shift = 30 - index * 2;
                            *pixel = colors[((flags >> shift) & 3) as usize];
                        }
                        if block < total_blocks {
                            self.fill_block(block, block_columns, pixels);
                            block += 1;
                        }
                    }
                }
                0xC0 | 0xD0 => {
                    let count = usize::from(opcode & 0x0F) + 1;
                    let colors = if family == 0xC0 {
                        let mut colors = [0u8; 8];
                        for color in &mut colors {
                            *color = read_u8(data, &mut pos)?;
                        }
                        self.color_octets[octet_index] = colors;
                        octet_index = (octet_index + 1) & 0xFF;
                        colors
                    } else {
                        self.color_octets[usize::from(read_u8(data, &mut pos)?)]
                    };
                    for _ in 0..count {
                        let first = read_be_u16(data, &mut pos)?;
                        let second = read_be_u16(data, &mut pos)?;
                        let third = read_be_u16(data, &mut pos)?;
                        let upper = (u32::from(first & 0xFFF0) << 8) | u32::from(second >> 4);
                        let lower = (u32::from(third & 0xFFF0) << 8)
                            | (u32::from(first & 0x000F) << 8)
                            | (u32::from(second & 0x000F) << 4)
                            | u32::from(third & 0x000F);
                        let mut pixels = [0u8; 16];
                        for index in 0..8 {
                            pixels[index] = colors[((upper >> (21 - index * 3)) & 7) as usize];
                            pixels[index + 8] = colors[((lower >> (21 - index * 3)) & 7) as usize];
                        }
                        if block < total_blocks {
                            self.fill_block(block, block_columns, pixels);
                            block += 1;
                        }
                    }
                }
                0xE0 | 0xF0 => {
                    let count = usize::from(opcode & 0x0F) + 1;
                    for _ in 0..count {
                        let mut pixels = [0u8; 16];
                        for pixel in &mut pixels {
                            *pixel = read_u8(data, &mut pos)?;
                        }
                        if block < total_blocks {
                            self.fill_block(block, block_columns, pixels);
                            block += 1;
                        }
                    }
                }
                _ => unreachable!(),
            }
        }

        Ok(&self.frame)
    }

    fn copy_block(&mut self, source: usize, destination: usize, block_columns: usize) {
        let mut pixels = [0u8; 16];
        self.read_block(source, block_columns, &mut pixels);
        self.fill_block(destination, block_columns, pixels);
    }

    fn read_block(&self, block: usize, block_columns: usize, pixels: &mut [u8; 16]) {
        let block_x = (block % block_columns) * 4;
        let block_y = (block / block_columns) * 4;
        for y in 0..4 {
            for x in 0..4 {
                if block_x + x < self.width && block_y + y < self.height {
                    pixels[y * 4 + x] = self.frame[(block_y + y) * self.width + block_x + x];
                }
            }
        }
    }

    fn fill_block(&mut self, block: usize, block_columns: usize, pixels: [u8; 16]) {
        let block_x = (block % block_columns) * 4;
        let block_y = (block / block_columns) * 4;
        for y in 0..4 {
            for x in 0..4 {
                if block_x + x < self.width && block_y + y < self.height {
                    self.frame[(block_y + y) * self.width + block_x + x] = pixels[y * 4 + x];
                }
            }
        }
    }
}

fn block_run_count(opcode: u8, data: &[u8], pos: &mut usize) -> Result<usize, &'static str> {
    if opcode & 0x10 != 0 {
        Ok(usize::from(read_u8(data, pos)?) + 1)
    } else {
        Ok(usize::from(opcode & 0x0F) + 1)
    }
}

fn read_u8(data: &[u8], pos: &mut usize) -> Result<u8, &'static str> {
    let value = data.get(*pos).copied().ok_or("truncated SMC frame")?;
    *pos += 1;
    Ok(value)
}

fn read_be_u16(data: &[u8], pos: &mut usize) -> Result<u16, &'static str> {
    let high = read_u8(data, pos)?;
    let low = read_u8(data, pos)?;
    Ok(u16::from_be_bytes([high, low]))
}

fn read_be_u32(data: &[u8], pos: &mut usize) -> Result<u32, &'static str> {
    let a = read_u8(data, pos)?;
    let b = read_u8(data, pos)?;
    let c = read_u8(data, pos)?;
    let d = read_u8(data, pos)?;
    Ok(u32::from_be_bytes([a, b, c, d]))
}

#[cfg(test)]
mod tests {
    use super::SmcDecoder;

    #[test]
    fn decodes_solid_and_literal_blocks() {
        let mut decoder = SmcDecoder::new(8, 4);
        let data = [
            0, 0, 0, 22, // SMC frame header
            0x60, 7,    // one solid block
            0xE0, // one literal block
            0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15,
        ];
        let frame = decoder.decode(&data).expect("decode");
        assert_eq!(&frame[0..4], &[7, 7, 7, 7]);
        assert_eq!(&frame[4..8], &[0, 1, 2, 3]);
        assert_eq!(&frame[12..16], &[4, 5, 6, 7]);
        assert_eq!(&frame[20..24], &[8, 9, 10, 11]);
        assert_eq!(&frame[28..32], &[12, 13, 14, 15]);
    }

    #[test]
    fn skip_blocks_preserves_previous_frame_pixels() {
        let mut decoder = SmcDecoder::new(4, 4);
        let first = [0, 0, 0, 6, 0x60, 42];
        decoder.decode(&first).expect("first frame");
        let skipped = [0, 0, 0, 5, 0x00];
        assert!(decoder
            .decode(&skipped)
            .expect("delta frame")
            .iter()
            .all(|pixel| *pixel == 42));
    }
}
