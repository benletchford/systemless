//! QuickTime Animation (`rle `) 8-bit decoder, pure Rust.
//!
//! Each frame updates a contiguous band of scanlines (`startLine`..`+numLines`)
//! of a persistent indexed frame; skipped lines and skipped pixels keep their
//! previous values, so the decoder retains its frame buffer between calls.
//!
//! QuickTime RLE processes 8-bit images in units of **four** pixels: skip
//! counts and run/literal lengths are all multiplied by four. Per line, an
//! initial skip byte is followed by opcodes until a `-1` terminator: `0` =
//! another skip, negative = a 4-pixel group repeated `-code` times, positive =
//! `code` literal 4-pixel groups.
//!
//! Behavioural reference: FFmpeg `qtrle.c` (reimplemented from the format).

/// Persistent QuickTime Animation decoder producing 8-bit palette indices.
#[derive(Clone, Debug)]
pub(crate) struct QtRleDecoder {
    width: usize,
    height: usize,
    /// Reconstructed frame as palette indices, one byte per pixel.
    indices: Vec<u8>,
}

impl QtRleDecoder {
    pub(crate) fn new(width: usize, height: usize) -> Self {
        Self {
            width,
            height,
            indices: vec![0; width.saturating_mul(height)],
        }
    }

    /// Decode one sample and return the reconstructed indexed frame
    /// (`width * height` bytes). Inter frames composite onto the prior frame.
    pub(crate) fn decode(&mut self, data: &[u8]) -> Result<&[u8], &'static str> {
        if self.width == 0 || self.height == 0 {
            return Err("qtrle: zero dimensions");
        }
        if data.len() < 8 {
            return Err("qtrle: frame too short");
        }
        let header = u16::from_be_bytes([data[4], data[5]]);
        let (start_line, num_lines, mut p) = if header & 0x0008 != 0 {
            if data.len() < 14 {
                return Err("qtrle: truncated header");
            }
            let start = u16::from_be_bytes([data[6], data[7]]) as usize;
            let lines = u16::from_be_bytes([data[10], data[11]]) as usize;
            (start, lines, 14usize)
        } else {
            (0usize, self.height, 6usize)
        };

        let w = self.width;
        let end = data.len();
        let get = |p: &mut usize| -> Option<u8> {
            if *p >= end {
                None
            } else {
                let b = data[*p];
                *p += 1;
                Some(b)
            }
        };

        let mut row = start_line;
        for _ in 0..num_lines {
            if row >= self.height {
                break;
            }
            let row_base = row * w;
            let mut x: isize = 0;

            // Initial per-line skip.
            let Some(skip) = get(&mut p) else { break };
            x += (skip as isize - 1) * 4;

            loop {
                let Some(code) = get(&mut p) else { break };
                if code == 0xFF {
                    break; // -1: end of line
                }
                let code = code as i8;
                if code == 0 {
                    // Additional skip.
                    let Some(skip) = get(&mut p) else { break };
                    x += (skip as isize - 1) * 4;
                } else if code < 0 {
                    // A single 4-pixel group repeated (-code) times.
                    let count = (-(code as isize)) as usize;
                    let (Some(a), Some(b), Some(c), Some(d)) =
                        (get(&mut p), get(&mut p), get(&mut p), get(&mut p))
                    else {
                        break;
                    };
                    let group = [a, b, c, d];
                    for _ in 0..count {
                        for &v in &group {
                            if x >= 0 && (x as usize) < w {
                                self.indices[row_base + x as usize] = v;
                            }
                            x += 1;
                        }
                    }
                } else {
                    // `code` literal 4-pixel groups.
                    for _ in 0..code as usize {
                        for _ in 0..4 {
                            let Some(v) = get(&mut p) else { break };
                            if x >= 0 && (x as usize) < w {
                                self.indices[row_base + x as usize] = v;
                            }
                            x += 1;
                        }
                    }
                }
            }
            row += 1;
        }

        Ok(&self.indices)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Build an rle sample with a full-frame (flag 8) header and a raw line
    /// body. `body` is the per-line opcode stream.
    fn sample(start_line: u16, num_lines: u16, body: &[u8]) -> Vec<u8> {
        let mut v = Vec::new();
        let size = 14 + body.len();
        v.extend_from_slice(&(size as u32).to_be_bytes());
        v.extend_from_slice(&0x0008u16.to_be_bytes()); // flag: lines present
        v.extend_from_slice(&start_line.to_be_bytes());
        v.extend_from_slice(&0u16.to_be_bytes());
        v.extend_from_slice(&num_lines.to_be_bytes());
        v.extend_from_slice(&0u16.to_be_bytes());
        v.extend_from_slice(body);
        v
    }

    #[test]
    fn literal_groups_fill_four_pixels_each() {
        // 8×1 image. One line: skip=1 (no skip), code=2 (two literal groups =
        // 8 pixels), then indices 10..17, then -1 terminator.
        let mut body = vec![1u8, 2u8];
        body.extend_from_slice(&[10, 11, 12, 13, 14, 15, 16, 17]);
        body.push(0xFF);
        let s = sample(0, 1, &body);
        let mut dec = QtRleDecoder::new(8, 1);
        let out = dec.decode(&s).expect("decode");
        assert_eq!(out, &[10, 11, 12, 13, 14, 15, 16, 17]);
    }

    #[test]
    fn negative_code_repeats_group() {
        // 8×1. skip=1, code=-2 => one 4-pixel group [1,2,3,4] repeated twice.
        let body = vec![1u8, (-2i8) as u8, 1, 2, 3, 4, 0xFF];
        let s = sample(0, 1, &body);
        let mut dec = QtRleDecoder::new(8, 1);
        let out = dec.decode(&s).expect("decode");
        assert_eq!(out, &[1, 2, 3, 4, 1, 2, 3, 4]);
    }

    #[test]
    fn initial_skip_offsets_start() {
        // 8×1. skip=2 => (2-1)*4 = 4 pixels skipped, then one literal group.
        let mut body = vec![2u8, 1u8];
        body.extend_from_slice(&[20, 21, 22, 23]);
        body.push(0xFF);
        let s = sample(0, 1, &body);
        let mut dec = QtRleDecoder::new(8, 1);
        let out = dec.decode(&s).expect("decode");
        // First four pixels untouched (0), next four are the literal group.
        assert_eq!(out, &[0, 0, 0, 0, 20, 21, 22, 23]);
    }

    #[test]
    fn inter_frame_keeps_unwritten_lines() {
        // Frame 0: two lines, both filled with a literal group.
        let mut body0 = Vec::new();
        for _ in 0..2 {
            body0.push(1u8); // skip
            body0.push(1u8); // one literal group
            body0.extend_from_slice(&[5, 5, 5, 5]);
            body0.push(0xFF);
        }
        let f0 = sample(0, 2, &body0);
        let mut dec = QtRleDecoder::new(4, 2);
        dec.decode(&f0).expect("f0");
        assert_eq!(dec.indices, vec![5, 5, 5, 5, 5, 5, 5, 5]);

        // Frame 1: update only line 1 (start_line=1, num_lines=1) to 9s.
        let mut body1 = vec![1u8, 1u8];
        body1.extend_from_slice(&[9, 9, 9, 9]);
        body1.push(0xFF);
        let f1 = sample(1, 1, &body1);
        dec.decode(&f1).expect("f1");
        // Line 0 preserved, line 1 updated.
        assert_eq!(dec.indices, vec![5, 5, 5, 5, 9, 9, 9, 9]);
    }
}
