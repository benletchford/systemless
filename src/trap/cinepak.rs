//! QuickTime Cinepak (`cvid`) video decoder, pure Rust.
//!
//! Cinepak divides each frame into horizontal strips. A strip owns two
//! codebooks: `v1` (one YUV vector expanded over a 4×4 block) and `v4` (four
//! YUV vectors, one per 2×2 quadrant of a 4×4 block). Codebook entries and the
//! reconstructed frame persist across frames so temporally-compressed
//! (inter) frames can reuse unchanged blocks — the decoder therefore keeps its
//! state between `decode` calls, exactly like a real QuickTime image sequence.
//!
//! Behavioural reference: FFmpeg `cinepak.c`
//! (<https://www.ffmpeg.org/doxygen/8.0/cinepak_8c_source.html>). The algorithm
//! here is reimplemented from the format description, not copied.

/// A single Cinepak codebook vector: four luma samples plus a shared signed
/// chroma pair. For a `v1` vector the four luma values cover the four 2×2
/// quadrants of a 4×4 block; for a `v4` vector they cover the four pixels of a
/// single 2×2 quadrant.
#[derive(Clone, Copy, Default, Debug)]
struct Codebook {
    y: [u8; 4],
    u: i8,
    v: i8,
}

/// Persistent Cinepak decoder. Holds the reconstructed RGB frame plus the two
/// codebooks so inter-coded frames can be applied on top of prior output.
#[derive(Clone, Debug)]
pub(crate) struct CinepakDecoder {
    width: usize,
    height: usize,
    /// Reconstructed frame, 3 bytes (R,G,B) per pixel.
    rgb: Vec<u8>,
    v1: [Codebook; 256],
    v4: [Codebook; 256],
    decoded_any: bool,
}

impl CinepakDecoder {
    pub(crate) fn new(width: usize, height: usize) -> Self {
        Self {
            width,
            height,
            rgb: vec![0; width.saturating_mul(height).saturating_mul(3)],
            v1: [Codebook::default(); 256],
            v4: [Codebook::default(); 256],
            decoded_any: false,
        }
    }

    pub(crate) fn width(&self) -> usize {
        self.width
    }

    pub(crate) fn height(&self) -> usize {
        self.height
    }

    /// Decode one Cinepak sample and return the reconstructed RGB frame
    /// (`width * height * 3` bytes). Inter frames are composited onto the
    /// previously decoded frame.
    pub(crate) fn decode(&mut self, data: &[u8]) -> Result<&[u8], &'static str> {
        if data.len() < 10 {
            return Err("cinepak: frame header too short");
        }
        // frame header: flags(1) length(3) width(2) height(2) strips(2)
        let width = ((data[4] as usize) << 8) | data[5] as usize;
        let height = ((data[6] as usize) << 8) | data[7] as usize;
        let num_strips = ((data[8] as usize) << 8) | data[9] as usize;

        if width == 0 || height == 0 {
            return Err("cinepak: zero frame dimensions");
        }
        // A frame may legitimately declare its own dimensions; adopt them if
        // the decoder was created with a placeholder size.
        if width != self.width || height != self.height {
            self.width = width;
            self.height = height;
            self.rgb.resize(width * height * 3, 0);
        }

        let mut pos = 10usize;
        let mut y_top = 0usize;

        for _ in 0..num_strips {
            if pos + 12 > data.len() {
                return Err("cinepak: truncated strip header");
            }
            // strip header: id(2) size(2) top(2) left(2) bottom(2) right(2)
            let strip_size = ((data[pos + 2] as usize) << 8) | data[pos + 3] as usize;
            let top = ((data[pos + 4] as usize) << 8) | data[pos + 5] as usize;
            let left = ((data[pos + 6] as usize) << 8) | data[pos + 7] as usize;
            let bottom = ((data[pos + 8] as usize) << 8) | data[pos + 9] as usize;
            let right = ((data[pos + 10] as usize) << 8) | data[pos + 11] as usize;

            let strip_end = (pos + strip_size).min(data.len());
            // Strip vertical extent. `top`/`bottom` are relative to the strip;
            // strips are laid out top-to-bottom, so track a running origin.
            let s_top = y_top;
            let s_bottom = (y_top + bottom.saturating_sub(top)).min(self.height);
            let left = left.min(self.width);
            let right = right.min(self.width);

            self.decode_strip(data, pos + 12, strip_end, s_top, s_bottom, left, right)?;

            y_top = s_bottom;
            pos = strip_end;
        }

        self.decoded_any = true;
        Ok(&self.rgb)
    }

    #[allow(clippy::too_many_arguments)]
    fn decode_strip(
        &mut self,
        data: &[u8],
        mut pos: usize,
        strip_end: usize,
        s_top: usize,
        s_bottom: usize,
        left: usize,
        right: usize,
    ) -> Result<(), &'static str> {
        // Current 4×4 block cursor.
        let mut x = left;
        let mut y = s_top;

        while pos + 4 <= strip_end {
            let chunk_id = ((data[pos] as usize) << 8) | data[pos + 1] as usize;
            let chunk_size = ((data[pos + 2] as usize) << 8) | data[pos + 3] as usize;
            let chunk_end = (pos + chunk_size).max(pos + 4).min(strip_end);
            let body = pos + 4;
            let ctype = chunk_id >> 8;

            match ctype {
                // V4 codebook: 0x20 full, 0x21 partial, 0x24/0x25 grayscale.
                0x20 | 0x21 | 0x24 | 0x25 => {
                    let grayscale = matches!(ctype, 0x24 | 0x25);
                    let partial = matches!(ctype, 0x21 | 0x25);
                    Self::load_codebook(&mut self.v4, data, body, chunk_end, grayscale, partial);
                }
                // V1 codebook: 0x22 full, 0x23 partial, 0x26/0x27 grayscale.
                0x22 | 0x23 | 0x26 | 0x27 => {
                    let grayscale = matches!(ctype, 0x26 | 0x27);
                    let partial = matches!(ctype, 0x23 | 0x27);
                    Self::load_codebook(&mut self.v1, data, body, chunk_end, grayscale, partial);
                }
                // Intra vectors, V1 only: one index per 4×4 block, no flags.
                0x32 => {
                    x = left;
                    y = s_top;
                    let mut r = body;
                    while r < chunk_end && y < s_bottom {
                        let cb = self.v1[data[r] as usize];
                        Self::put_v1(&mut self.rgb, self.width, x, y, s_bottom, &cb);
                        r += 1;
                        x += 4;
                        if x >= right {
                            x = left;
                            y += 4;
                        }
                    }
                }
                // Vectors, intra (0x30) or inter (0x31). Both consume a
                // continuous MSB-first bitstream of 32-bit words. Intra: one
                // bit per block selects V4 (set) or V1 (clear). Inter: one bit
                // per block selects coded (set) or skipped (clear); each coded
                // block then reads another bit selecting V4 (set) or V1 (clear).
                0x30 | 0x31 => {
                    let inter = ctype == 0x31;
                    let mut bits = BitReader::new(data, body, chunk_end);
                    x = left;
                    y = s_top;
                    while y < s_bottom {
                        if inter {
                            match bits.bit() {
                                Some(true) => {} // coded — fall through
                                Some(false) => {
                                    // Skipped: keep the block from the prior frame.
                                    x += 4;
                                    if x >= right {
                                        x = left;
                                        y += 4;
                                    }
                                    continue;
                                }
                                None => break,
                            }
                        }
                        let use_v4 = match bits.bit() {
                            Some(b) => b,
                            None => break,
                        };
                        if use_v4 {
                            let (Some(a), Some(b2), Some(c), Some(d2)) =
                                (bits.byte(), bits.byte(), bits.byte(), bits.byte())
                            else {
                                break;
                            };
                            let q = [
                                self.v4[a as usize],
                                self.v4[b2 as usize],
                                self.v4[c as usize],
                                self.v4[d2 as usize],
                            ];
                            Self::put_v4(&mut self.rgb, self.width, x, y, s_bottom, &q);
                        } else {
                            let Some(idx) = bits.byte() else { break };
                            let cb = self.v1[idx as usize];
                            Self::put_v1(&mut self.rgb, self.width, x, y, s_bottom, &cb);
                        }
                        x += 4;
                        if x >= right {
                            x = left;
                            y += 4;
                        }
                    }
                }
                _ => {
                    // Unknown chunk: skip its body.
                }
            }

            if chunk_size < 4 {
                break;
            }
            pos = chunk_end;
        }
        Ok(())
    }

    /// Load a codebook chunk. `partial` chunks carry 32-bit flag words that
    /// select which of every 32 entries are updated; `grayscale` chunks omit
    /// the shared chroma pair (4 bytes/entry instead of 6).
    fn load_codebook(
        book: &mut [Codebook; 256],
        data: &[u8],
        body: usize,
        end: usize,
        grayscale: bool,
        partial: bool,
    ) {
        let entry_len = if grayscale { 4 } else { 6 };
        let read_entry = |r: usize| -> Codebook {
            let mut cb = Codebook {
                y: [data[r], data[r + 1], data[r + 2], data[r + 3]],
                u: 0,
                v: 0,
            };
            if !grayscale {
                cb.u = data[r + 4] as i8;
                cb.v = data[r + 5] as i8;
            }
            cb
        };

        if partial {
            let mut r = body;
            let mut idx = 0usize;
            while r + 4 <= end && idx < 256 {
                let mut flags =
                    u32::from_be_bytes([data[r], data[r + 1], data[r + 2], data[r + 3]]);
                r += 4;
                for _ in 0..32 {
                    if idx >= 256 {
                        break;
                    }
                    if (flags & 0x8000_0000) != 0 {
                        if r + entry_len > end {
                            return;
                        }
                        book[idx] = read_entry(r);
                        r += entry_len;
                    }
                    flags <<= 1;
                    idx += 1;
                }
            }
        } else {
            let mut r = body;
            let mut idx = 0usize;
            while r + entry_len <= end && idx < 256 {
                book[idx] = read_entry(r);
                r += entry_len;
                idx += 1;
            }
        }
    }

    #[inline]
    fn set_pixel(rgb: &mut [u8], width: usize, x: usize, y: usize, luma: u8, u: i8, v: i8) {
        let (r, g, b) = yuv_to_rgb(luma, u, v);
        let o = (y * width + x) * 3;
        if o + 2 < rgb.len() {
            rgb[o] = r;
            rgb[o + 1] = g;
            rgb[o + 2] = b;
        }
    }

    /// Expand a `v1` vector over a 4×4 block: each luma sample fills one 2×2
    /// quadrant, the chroma pair is shared across the whole block.
    fn put_v1(rgb: &mut [u8], width: usize, x: usize, y: usize, y_limit: usize, cb: &Codebook) {
        // Quadrant luma: y0 top-left, y1 top-right, y2 bottom-left, y3 bottom-right.
        let quads = [
            (0usize, 0usize, cb.y[0]),
            (2, 0, cb.y[1]),
            (0, 2, cb.y[2]),
            (2, 2, cb.y[3]),
        ];
        for (qx, qy, luma) in quads {
            for dy in 0..2 {
                for dx in 0..2 {
                    let py = y + qy + dy;
                    if py >= y_limit {
                        continue;
                    }
                    if x + qx + dx < width {
                        Self::set_pixel(rgb, width, x + qx + dx, py, luma, cb.u, cb.v);
                    }
                }
            }
        }
    }

    /// Expand a `v4` block: four vectors, one per 2×2 quadrant, each supplying
    /// its own four luma samples and chroma pair.
    fn put_v4(rgb: &mut [u8], width: usize, x: usize, y: usize, y_limit: usize, q: &[Codebook; 4]) {
        let origins = [(0usize, 0usize), (2, 0), (0, 2), (2, 2)];
        for (qi, (ox, oy)) in origins.into_iter().enumerate() {
            let cb = &q[qi];
            // Within the 2×2 quadrant: y0 TL, y1 TR, y2 BL, y3 BR.
            let px = [
                (0usize, 0usize, cb.y[0]),
                (1, 0, cb.y[1]),
                (0, 1, cb.y[2]),
                (1, 1, cb.y[3]),
            ];
            for (dx, dy, luma) in px {
                let py = y + oy + dy;
                if py >= y_limit {
                    continue;
                }
                if x + ox + dx < width {
                    Self::set_pixel(rgb, width, x + ox + dx, py, luma, cb.u, cb.v);
                }
            }
        }
    }
}

/// Reads a Cinepak vector chunk. Flag bits (skip / V1-vs-V4 selectors) and the
/// codebook index bytes are interleaved in a single stream sharing one
/// position: 32-bit flag words are pulled inline as bits run out, consumed
/// MSB-first, while index bytes are read from the current position.
struct BitReader<'a> {
    data: &'a [u8],
    pos: usize,
    end: usize,
    flag: u32,
    mask: u32,
}

impl<'a> BitReader<'a> {
    fn new(data: &'a [u8], pos: usize, end: usize) -> Self {
        Self {
            data,
            pos,
            end: end.min(data.len()),
            flag: 0,
            mask: 0,
        }
    }

    /// Next flag bit, refilling a 32-bit word from the current position when
    /// the mask is exhausted. Returns None if the stream is depleted.
    fn bit(&mut self) -> Option<bool> {
        if self.mask == 0 {
            if self.pos + 4 > self.end {
                return None;
            }
            self.flag = u32::from_be_bytes([
                self.data[self.pos],
                self.data[self.pos + 1],
                self.data[self.pos + 2],
                self.data[self.pos + 3],
            ]);
            self.pos += 4;
            self.mask = 0x8000_0000;
        }
        let b = (self.flag & self.mask) != 0;
        self.mask >>= 1;
        Some(b)
    }

    /// Next codebook index byte from the current position.
    fn byte(&mut self) -> Option<u8> {
        if self.pos >= self.end {
            return None;
        }
        let b = self.data[self.pos];
        self.pos += 1;
        Some(b)
    }
}

/// Cinepak's simplified YUV→RGB reconstruction (signed chroma, centered at 0).
#[inline]
fn yuv_to_rgb(y: u8, u: i8, v: i8) -> (u8, u8, u8) {
    let y = y as i32;
    let u = u as i32;
    let v = v as i32;
    let r = y + 2 * v;
    let g = y - (u / 2) - v;
    let b = y + 2 * u;
    (
        r.clamp(0, 255) as u8,
        g.clamp(0, 255) as u8,
        b.clamp(0, 255) as u8,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn be16(v: u16) -> [u8; 2] {
        v.to_be_bytes()
    }

    /// Build a one-strip Cinepak frame from raw chunk bytes.
    fn frame(width: u16, height: u16, chunks: &[u8]) -> Vec<u8> {
        let mut strip = Vec::new();
        strip.extend_from_slice(&be16(0x1000)); // strip id
        let strip_size = 12 + chunks.len();
        strip.extend_from_slice(&be16(strip_size as u16));
        strip.extend_from_slice(&be16(0)); // top
        strip.extend_from_slice(&be16(0)); // left
        strip.extend_from_slice(&be16(height)); // bottom
        strip.extend_from_slice(&be16(width)); // right
        strip.extend_from_slice(chunks);

        let mut f = Vec::new();
        f.push(0x00); // flags: intra
        let total = 10 + strip.len();
        f.push((total >> 16) as u8);
        f.push((total >> 8) as u8);
        f.push(total as u8);
        f.extend_from_slice(&be16(width));
        f.extend_from_slice(&be16(height));
        f.extend_from_slice(&be16(1)); // num strips
        f.extend_from_slice(&strip);
        f
    }

    fn chunk(id: u16, body: &[u8]) -> Vec<u8> {
        let mut c = Vec::new();
        c.extend_from_slice(&be16(id));
        c.extend_from_slice(&be16((4 + body.len()) as u16));
        c.extend_from_slice(body);
        c
    }

    #[test]
    fn v1_grayscale_single_block_fills_4x4() {
        // One V1 grayscale codebook entry [10,20,30,40], then a 0x3200 chunk
        // referencing index 0 for the single 4×4 block.
        let cb = chunk(0x2600, &[10, 20, 30, 40]);
        let vec = chunk(0x3200, &[0u8]);
        let mut body = cb;
        body.extend_from_slice(&vec);
        let f = frame(4, 4, &body);

        let mut dec = CinepakDecoder::new(4, 4);
        let rgb = dec.decode(&f).expect("decode");
        assert_eq!(rgb.len(), 4 * 4 * 3);

        // Grayscale => R=G=B=luma. Quadrant layout: y0 TL, y1 TR, y2 BL, y3 BR.
        let px = |x: usize, y: usize| rgb[(y * 4 + x) * 3];
        assert_eq!(px(0, 0), 10); // TL quadrant
        assert_eq!(px(1, 1), 10);
        assert_eq!(px(2, 0), 20); // TR quadrant
        assert_eq!(px(3, 1), 20);
        assert_eq!(px(0, 2), 30); // BL quadrant
        assert_eq!(px(1, 3), 30);
        assert_eq!(px(2, 2), 40); // BR quadrant
        assert_eq!(px(3, 3), 40);
        // Grayscale is achromatic.
        assert_eq!(rgb[0], rgb[1]);
        assert_eq!(rgb[1], rgb[2]);
    }

    #[test]
    fn v4_block_uses_four_quadrant_vectors() {
        // Four V4 grayscale entries, each a flat luma so we can read quadrants.
        let mut cbbody = Vec::new();
        cbbody.extend_from_slice(&[100, 100, 100, 100]); // idx0
        cbbody.extend_from_slice(&[110, 110, 110, 110]); // idx1
        cbbody.extend_from_slice(&[120, 120, 120, 120]); // idx2
        cbbody.extend_from_slice(&[130, 130, 130, 130]); // idx3
        let cb = chunk(0x2400, &cbbody);

        // 0x3000 vectors: one flag word (top bit set => V4), then 4 indices.
        let mut vecbody = Vec::new();
        vecbody.extend_from_slice(&0x8000_0000u32.to_be_bytes());
        vecbody.extend_from_slice(&[0, 1, 2, 3]);
        let vec = chunk(0x3000, &vecbody);

        let mut body = cb;
        body.extend_from_slice(&vec);
        let f = frame(4, 4, &body);

        let mut dec = CinepakDecoder::new(4, 4);
        let rgb = dec.decode(&f).expect("decode");
        let px = |x: usize, y: usize| rgb[(y * 4 + x) * 3];
        assert_eq!(px(0, 0), 100); // TL quadrant -> v4[0]
        assert_eq!(px(2, 0), 110); // TR quadrant -> v4[1]
        assert_eq!(px(0, 2), 120); // BL quadrant -> v4[2]
        assert_eq!(px(2, 2), 130); // BR quadrant -> v4[3]
    }

    #[test]
    fn color_vector_reconstructs_via_yuv() {
        // Single color V1 entry: luma 128, u=+20, v=-10.
        let entry = [128u8, 128, 128, 128, 20u8, (-10i8) as u8];
        let cb = chunk(0x2200, &entry);
        let vec = chunk(0x3200, &[0u8]);
        let mut body = cb;
        body.extend_from_slice(&vec);
        let f = frame(4, 4, &body);

        let mut dec = CinepakDecoder::new(4, 4);
        let rgb = dec.decode(&f).expect("decode");
        let (r, g, b) = yuv_to_rgb(128, 20, -10);
        assert_eq!((rgb[0], rgb[1], rgb[2]), (r, g, b));
        // b = 128 + 2*20 = 168, r = 128 + 2*(-10) = 108.
        assert_eq!(b, 168);
        assert_eq!(r, 108);
    }

    #[test]
    fn inter_frame_preserves_unchanged_blocks() {
        // Intra frame: 8×4 (two blocks wide), both blocks luma 50 via V1-only.
        let cb0 = chunk(0x2600, &[50, 50, 50, 50]);
        let intra_vec = chunk(0x3200, &[0u8, 0u8]);
        let mut b0 = cb0;
        b0.extend_from_slice(&intra_vec);
        let f0 = frame(8, 4, &b0);

        let mut dec = CinepakDecoder::new(8, 4);
        dec.decode(&f0).expect("intra");
        assert_eq!(dec.rgb[0], 50);

        // Inter frame (0x31 vectors): redefine v1[0]=200, then code block0 and
        // skip block1. Inter bitstream per block: a "coded?" bit, and for coded
        // blocks a "V4?" selector bit. Flag word (MSB-first):
        //   bit31=1 block0 coded, bit30=0 block0 uses V1, bit29=0 block1 skip.
        // => 0x8000_0000. The V1 index byte (0) follows the flag word.
        let cb1 = chunk(0x2600, &[200, 200, 200, 200]);
        let mut inter_body = Vec::new();
        inter_body.extend_from_slice(&0x8000_0000u32.to_be_bytes());
        inter_body.push(0u8); // block0 -> v1[0]=200
        let inter_vec = chunk(0x3100, &inter_body);
        let mut b1 = cb1;
        b1.extend_from_slice(&inter_vec);
        let f1 = frame(8, 4, &b1);

        dec.decode(&f1).expect("inter");
        // Left block updated to 200.
        assert_eq!(dec.rgb[0], 200);
        // Right block (x=4) skipped => preserved from the first frame at 50.
        assert_eq!(dec.rgb[4 * 3], 50);
    }

    #[test]
    fn inter_frame_v4_coded_block_reads_four_indices() {
        // Intra frame: single 4×4 block luma 10 (V1-only).
        let cb0 = chunk(0x2600, &[10, 10, 10, 10]);
        let intra = chunk(0x3200, &[0u8]);
        let mut b0 = cb0;
        b0.extend_from_slice(&intra);
        let f0 = frame(4, 4, &b0);
        let mut dec = CinepakDecoder::new(4, 4);
        dec.decode(&f0).expect("intra");

        // Inter frame: define four V4 grayscale entries, then code the block
        // as V4. Bitstream: bit31=1 (coded), bit30=1 (V4) => 0xC000_0000,
        // followed by four V4 indices [0,1,2,3].
        let mut v4body = Vec::new();
        for luma in [60u8, 70, 80, 90] {
            v4body.extend_from_slice(&[luma, luma, luma, luma]);
        }
        let v4cb = chunk(0x2400, &v4body);
        let mut inter_body = Vec::new();
        inter_body.extend_from_slice(&0xC000_0000u32.to_be_bytes());
        inter_body.extend_from_slice(&[0, 1, 2, 3]);
        let inter = chunk(0x3100, &inter_body);
        let mut b1 = v4cb;
        b1.extend_from_slice(&inter);
        let f1 = frame(4, 4, &b1);

        dec.decode(&f1).expect("inter v4");
        let px = |x: usize, y: usize| dec.rgb[(y * 4 + x) * 3];
        assert_eq!(px(0, 0), 60); // TL quadrant -> v4[0]
        assert_eq!(px(2, 0), 70); // TR quadrant -> v4[1]
        assert_eq!(px(0, 2), 80); // BL quadrant -> v4[2]
        assert_eq!(px(2, 2), 90); // BR quadrant -> v4[3]
    }
}
