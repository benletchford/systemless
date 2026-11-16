//! Matte compositing for the PICT `UncompressedQuickTime` opcode ($8201).
//!
//! The opcode carries a version, a MatrixRecord, a matte size and matte
//! rectangle, and then the matte itself: an ImageDescription followed by
//! the matte's (usually compressed) pixels. The image it applies to is the
//! ordinary QuickDraw bits opcode that follows. Inside Macintosh: QuickTime
//! (1993), Table 3-1.
//!
//! A matte is a grayscale alpha map with CopyDeepMask semantics: black
//! takes the source pixel, white keeps the destination, and grays blend
//! between them (Inside Macintosh: Imaging With QuickDraw (1994),
//! CopyDeepMask).

use crate::memory::{MacMemoryBus, MemoryBus};

use super::{closest_clut_index, write_pixel, DstClip, PictureMapping};

/// Decoded matte waiting for the next bits opcode in the picture.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct PictureMatte {
    /// matteRect in picture coordinates: (top, left, bottom, right).
    rect: (i16, i16, i16, i16),
    width: usize,
    height: usize,
    /// Per-pixel source weight: 255 takes the image, 0 keeps the destination.
    alpha: Vec<u8>,
}

impl PictureMatte {
    fn alpha_at_picture(&self, pic_y: i32, pic_x: i32) -> Option<u8> {
        let (top, left, bottom, right) = (
            i32::from(self.rect.0),
            i32::from(self.rect.1),
            i32::from(self.rect.2),
            i32::from(self.rect.3),
        );
        if pic_y < top || pic_y >= bottom || pic_x < left || pic_x >= right {
            return None;
        }
        // matteRect may be scaled relative to the matte's own pixel size.
        let my = (pic_y - top) as usize * self.height / (bottom - top) as usize;
        let mx = (pic_x - left) as usize * self.width / (right - left) as usize;
        self.alpha.get(my * self.width + mx).copied()
    }
}

/// Parse the matte of an `UncompressedQuickTime` opcode whose data length
/// long starts at `pos`. Returns `None` for an absent or unsupported matte;
/// the following image then draws unmatted, as before.
pub(super) fn parse_uncompressed_quicktime_matte(
    bus: &MacMemoryBus,
    pos: u32,
) -> Option<PictureMatte> {
    let payload_size = bus.read_long(pos);
    let end = pos.checked_add(4)?.checked_add(payload_size)?;
    // version (2) + MatrixRecord (36)
    let mut cursor = pos + 4 + 2 + 36;
    if cursor.checked_add(12)? > end {
        return None;
    }
    let matte_size = bus.read_long(cursor);
    cursor += 4;
    let rect = (
        bus.read_word(cursor) as i16,
        bus.read_word(cursor + 2) as i16,
        bus.read_word(cursor + 4) as i16,
        bus.read_word(cursor + 6) as i16,
    );
    cursor += 8;
    if matte_size < 86 || rect.2 <= rect.0 || rect.3 <= rect.1 {
        return None;
    }
    let matte_end = cursor.checked_add(matte_size)?;
    if matte_end > end {
        return None;
    }
    let description_size = bus.read_long(cursor);
    let codec = bus.read_long(cursor + 4);
    let width = usize::from(bus.read_word(cursor + 32));
    let height = usize::from(bus.read_word(cursor + 34));
    let data_size = bus.read_long(cursor + 44);
    let depth = bus.read_word(cursor + 82);
    let data_start = cursor.checked_add(description_size)?;
    if description_size < 86
        || width == 0
        || height == 0
        || data_start.checked_add(data_size)? > matte_end
    {
        return None;
    }
    // Mattes are 8-bit grayscale (depth 40). The Animation codec is the
    // one QuickTime picture tools write for them.
    if codec != u32::from_be_bytes(*b"rle ") || depth != 40 {
        return None;
    }
    let compressed = bus.read_bytes(data_start, data_size as usize);
    let mut decoder = super::super::qtrle::QtRleDecoder::new(width, height);
    // Depth-40 grayscale indices run from white (0) to black (255), so the
    // index is the source weight directly.
    let alpha = decoder.decode(&compressed).ok()?.to_vec();
    Some(PictureMatte {
        rect,
        width,
        height,
        alpha,
    })
}

/// Destination pixels under a matte, saved before its image draws.
pub(super) struct MatteBackdrop {
    matte: PictureMatte,
    top: i32,
    left: i32,
    bottom: i32,
    right: i32,
    /// Saved destination bytes for rows `top..bottom`, `row_bytes` each.
    saved: Vec<u8>,
}

fn read_pixel(bytes: &[u8], row_offset: usize, x: i32, pixel_size: u16) -> u32 {
    let x = x as usize;
    match pixel_size {
        32 => u32::from_be_bytes(
            bytes[row_offset + x * 4..row_offset + x * 4 + 4]
                .try_into()
                .unwrap_or([0; 4]),
        ),
        16 => u32::from(u16::from_be_bytes([
            bytes[row_offset + x * 2],
            bytes[row_offset + x * 2 + 1],
        ])),
        8 => u32::from(bytes[row_offset + x]),
        1 | 2 | 4 => {
            let bit = x * usize::from(pixel_size);
            let byte = bytes[row_offset + bit / 8];
            let shift = 8 - usize::from(pixel_size) - bit % 8;
            u32::from(byte >> shift) & ((1 << pixel_size) - 1)
        }
        _ => 0,
    }
}

fn pixel_rgb8(pixel: u32, pixel_size: u16, clut: &[[u16; 3]; 256]) -> [u32; 3] {
    match pixel_size {
        32 => [(pixel >> 16) & 0xff, (pixel >> 8) & 0xff, pixel & 0xff],
        16 => [10, 5, 0].map(|shift| ((pixel >> shift) & 31) * 255 / 31),
        _ => clut[pixel as usize & 0xff].map(|c| u32::from(c >> 8)),
    }
}

impl MatteBackdrop {
    /// Save the destination under `matte`, mapped through the picture's
    /// frame-to-destination transform and clipped to the screen.
    pub(super) fn save(
        bus: &MacMemoryBus,
        matte: PictureMatte,
        mapping: &PictureMapping,
        screen_mode: (u32, u32, u16, u16, u16),
    ) -> Option<Self> {
        let (base, row_bytes, screen_w, screen_h, pixel_size) = screen_mode;
        if !matches!(pixel_size, 1 | 2 | 4 | 8 | 16 | 32) {
            return None;
        }
        let (top, left, bottom, right) = matte.rect;
        let (y0, x0) = mapping.to_destination(i32::from(top), i32::from(left));
        let (y1, x1) = mapping.to_destination(i32::from(bottom), i32::from(right));
        let top = y0.min(y1).max(0);
        let bottom = y0.max(y1).min(i32::from(screen_h));
        let left = x0.min(x1).max(0);
        let right = x0.max(x1).min(i32::from(screen_w));
        if top >= bottom || left >= right {
            return None;
        }
        let start = base.checked_add(top as u32 * row_bytes)?;
        let saved = bus.read_bytes(start, ((bottom - top) as u32 * row_bytes) as usize);
        Some(Self {
            matte,
            top,
            left,
            bottom,
            right,
            saved,
        })
    }

    /// Blend the image just drawn with the saved destination by matte alpha.
    pub(super) fn composite(
        self,
        bus: &mut MacMemoryBus,
        mapping: &PictureMapping,
        screen_mode: (u32, u32, u16, u16, u16),
        device_clut: &[[u16; 3]; 256],
        dst_clip: Option<&DstClip>,
    ) {
        let (base, row_bytes, screen_w, screen_h, pixel_size) = screen_mode;
        let start = base + self.top as u32 * row_bytes;
        let drawn = bus.read_bytes(start, self.saved.len());
        for y in self.top..self.bottom {
            let row_offset = (y - self.top) as usize * row_bytes as usize;
            for x in self.left..self.right {
                if dst_clip.is_some_and(|clip| !clip.contains(x, y)) {
                    continue;
                }
                let (pic_y, pic_x) = mapping.to_picture(y, x);
                // Outside the matte the image keeps its own coverage.
                let Some(alpha) = self.matte.alpha_at_picture(pic_y, pic_x) else {
                    continue;
                };
                if alpha == 255 {
                    continue;
                }
                let old = read_pixel(&self.saved, row_offset, x, pixel_size);
                let new = read_pixel(&drawn, row_offset, x, pixel_size);
                if old == new {
                    continue;
                }
                let pixel = if alpha == 0 {
                    old
                } else if pixel_size < 8 {
                    if alpha >= 128 {
                        new
                    } else {
                        old
                    }
                } else {
                    let a = u32::from(alpha);
                    let old_rgb = pixel_rgb8(old, pixel_size, device_clut);
                    let new_rgb = pixel_rgb8(new, pixel_size, device_clut);
                    let rgb =
                        [0, 1, 2].map(|i| (new_rgb[i] * a + old_rgb[i] * (255 - a) + 127) / 255);
                    match pixel_size {
                        32 => (rgb[0] << 16) | (rgb[1] << 8) | rgb[2],
                        16 => rgb.iter().fold(0, |acc, c| (acc << 5) | (c * 31 / 255)),
                        _ => u32::from(closest_clut_index(
                            (rgb[0] * 257) as u16,
                            (rgb[1] * 257) as u16,
                            (rgb[2] * 257) as u16,
                            device_clut,
                        )),
                    }
                };
                let addr = base + y as u32 * row_bytes;
                match pixel_size {
                    32 => bus.write_long(addr + x as u32 * 4, pixel),
                    16 => bus.write_word(addr + x as u32 * 2, pixel as u16),
                    _ => write_pixel(
                        bus,
                        base,
                        row_bytes,
                        x,
                        y,
                        pixel as u8,
                        i32::from(screen_w),
                        i32::from(screen_h),
                        pixel_size,
                    ),
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::draw_picture;
    use crate::memory::{MacMemoryBus, MemoryBus};

    fn push_u16(out: &mut Vec<u8>, value: u16) {
        out.extend_from_slice(&value.to_be_bytes());
    }

    fn push_u32(out: &mut Vec<u8>, value: u32) {
        out.extend_from_slice(&value.to_be_bytes());
    }

    fn push_rect(out: &mut Vec<u8>, rect: (i16, i16, i16, i16)) {
        for value in [rect.0, rect.1, rect.2, rect.3] {
            push_u16(out, value as u16);
        }
    }

    /// One-row, four-pixel picture: an optional UncompressedQuickTime matte
    /// with the given depth-40 `rle ` weights, then an all-black 16-bit
    /// DirectBitsRect.
    fn matted_picture(matte: Option<[u8; 4]>) -> Vec<u8> {
        let rect = (0, 0, 1, 4);
        let mut ops = Vec::new();
        push_u16(&mut ops, 0x0011);
        push_u16(&mut ops, 0x02FF);
        push_u16(&mut ops, 0x0C00);
        ops.extend_from_slice(&[0xFF, 0xFE, 0, 0]);
        push_u32(&mut ops, 0x0048_0000);
        push_u32(&mut ops, 0x0048_0000);
        push_rect(&mut ops, rect);
        push_u32(&mut ops, 0);
        if let Some(weights) = matte {
            // Animation frame: chunk size, header 0 (all lines), then one
            // line of skip 1, one literal group of four pixels, end of line.
            let mut frame = Vec::new();
            push_u32(&mut frame, 0);
            push_u16(&mut frame, 0);
            frame.extend_from_slice(&[1, 1]);
            frame.extend_from_slice(&weights);
            frame.push(0xFF);
            let mut description = vec![0u8; 86];
            description[0..4].copy_from_slice(&86u32.to_be_bytes());
            description[4..8].copy_from_slice(b"rle ");
            description[32..34].copy_from_slice(&4u16.to_be_bytes());
            description[34..36].copy_from_slice(&1u16.to_be_bytes());
            description[44..48].copy_from_slice(&(frame.len() as u32).to_be_bytes());
            description[82..84].copy_from_slice(&40u16.to_be_bytes());
            description[84..86].copy_from_slice(&(-1i16 as u16).to_be_bytes());
            let matte_size = (description.len() + frame.len()) as u32;
            push_u16(&mut ops, 0x8201);
            push_u32(&mut ops, 2 + 36 + 4 + 8 + matte_size);
            push_u16(&mut ops, 0);
            for value in [0x0001_0000u32, 0, 0, 0, 0x0001_0000, 0, 0, 0, 0x4000_0000] {
                push_u32(&mut ops, value);
            }
            push_u32(&mut ops, matte_size);
            push_rect(&mut ops, rect);
            ops.extend_from_slice(&description);
            ops.extend_from_slice(&frame);
            if ops.len() % 2 != 0 {
                ops.push(0);
            }
        }
        push_u16(&mut ops, 0x009A);
        push_u32(&mut ops, 0x0000_00FF);
        push_u16(&mut ops, 0x8000 | 8);
        push_rect(&mut ops, rect);
        push_u16(&mut ops, 0); // pmVersion
        push_u16(&mut ops, 1); // packType: unpacked
        push_u32(&mut ops, 0);
        push_u32(&mut ops, 0x0048_0000);
        push_u32(&mut ops, 0x0048_0000);
        push_u16(&mut ops, 16); // pixelType: RGBDirect
        push_u16(&mut ops, 16);
        push_u16(&mut ops, 3);
        push_u16(&mut ops, 5);
        push_u32(&mut ops, 0);
        push_u32(&mut ops, 0);
        push_u32(&mut ops, 0);
        push_rect(&mut ops, rect);
        push_rect(&mut ops, rect);
        push_u16(&mut ops, 0); // srcCopy
        ops.extend_from_slice(&[0; 8]);
        push_u16(&mut ops, 0x00FF);

        let mut picture = Vec::new();
        push_u16(&mut picture, 0);
        push_rect(&mut picture, rect);
        picture.extend_from_slice(&ops);
        picture
    }

    /// Draw onto a white 32-bit row and return the four pixels.
    fn draw_on_white(picture: &[u8]) -> [u32; 4] {
        let mut bus = MacMemoryBus::new(0x10000);
        let pic = 0x1000;
        let screen = 0x8000;
        bus.write_bytes(pic, picture);
        for x in 0..4 {
            bus.write_long(screen + x * 4, 0x00FF_FFFF);
        }
        let clut = [[0u16; 3]; 256];
        let (drawn, _) = draw_picture(
            &mut bus,
            pic,
            0,
            0,
            1,
            4,
            (screen, 16, 4, 1, 32),
            &clut,
            0,
            None,
        );
        assert!(drawn);
        [0, 1, 2, 3].map(|x| bus.read_long(screen + x * 4) & 0x00FF_FFFF)
    }

    #[test]
    fn uncompressed_quicktime_matte_blends_following_bits_with_destination() {
        // Index 255 (black) takes the image, 0 (white) keeps the
        // destination, and grays blend. Candy Crisis draws its shareware
        // notice text as black DirectBits through such a matte.
        assert_eq!(
            draw_on_white(&matted_picture(Some([0, 255, 128, 64]))),
            [0x00FF_FFFF, 0, 0x007F_7F7F, 0x00BF_BFBF]
        );
    }

    #[test]
    fn bits_without_a_matte_still_draw_opaquely() {
        assert_eq!(draw_on_white(&matted_picture(None)), [0; 4]);
    }

    #[test]
    fn matte_applies_only_to_the_next_bits_opcode() {
        let mut picture = matted_picture(Some([0; 4]));
        // Repeat the DirectBitsRect: the second copy must draw unmatted.
        let bits_start = picture
            .windows(2)
            .rposition(|pair| pair == [0x00, 0x9A])
            .unwrap();
        let bits = picture[bits_start..picture.len() - 2].to_vec();
        let end = picture.len() - 2;
        picture.splice(end..end, bits);
        assert_eq!(draw_on_white(&picture), [0; 4]);
    }
}
