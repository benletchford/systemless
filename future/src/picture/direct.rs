//! IM: Imaging With QuickDraw, appendix A: DirectBitsRect/Rgn and PixData.
use super::*;
fn unpack_words(input: &[u8], expected: usize) -> Result<Vec<u8>> {
    let mut reader = Reader {
        bytes: input,
        at: 0,
    };
    let mut out = Vec::with_capacity(expected);
    while reader.at < input.len() {
        let control = reader.byte()? as i8;
        if control == -128 {
            continue;
        }
        let count = if control >= 0 {
            control as usize + 1
        } else {
            (1 - i16::from(control)) as usize
        };
        if out.len() + count * 2 > expected {
            return Err(bad("direct word PackBits overflow"));
        }
        if control >= 0 {
            out.extend(reader.take(count * 2)?);
        } else {
            let word = reader.take(2)?;
            for _ in 0..count {
                out.extend(word);
            }
        }
    }
    if out.len() != expected {
        return Err(bad("direct word PackBits row length"));
    }
    Ok(out)
}
pub(super) fn decode(
    reader: &mut Reader<'_>,
    opcode: u16,
    clips: &Option<Vec<Rect>>,
) -> Result<BitmapCommand> {
    if reader.take(4)? != [0, 0, 0, 0xff] {
        return Err(bad("direct PixMap base placeholder"));
    }
    let row = reader.word()?;
    let stride = usize::from(row & 0x3fff);
    let bounds = reader.rect()?;
    let version = reader.word()?;
    let packed = reader.word()?;
    reader.take(12)?; // packSize and resolutions; scan-line byte counts delimit packed rows.
    let kind = reader.word()?;
    let depth = reader.word()?;
    let components = usize::from(reader.word()?);
    let bits = reader.word()?;
    let plane = reader.take(4)?;
    reader.take(8)?; // No color table follows direct PixMaps; table/reserved are not host pointers.
    if row & 0xc000 != 0x8000
        || stride == 0
        || version != 0
        || kind != 16
        || plane != [0; 4]
        || !positive(bounds)
        || !matches!(
            (depth, components, bits),
            (16, 3, 5) | (32, 3, 8) | (32, 4, 8)
        )
        || (depth == 16 && !matches!(packed, 0 | 1 | 3))
        || (depth == 32 && !matches!(packed, 0 | 1 | 2 | 4))
    {
        return Err(bad("unsupported direct PixMap layout"));
    }
    let width = (bounds.right - bounds.left) as usize;
    let height = (bounds.bottom - bounds.top) as usize;
    let bytes_per_pixel = usize::from(depth / 8);
    if width > 4096
        || height > 4096
        || !stride.is_multiple_of(bytes_per_pixel)
        || width * bytes_per_pixel > stride
        || stride * height > LIMIT
        || width * height * 6 > LIMIT
    {
        return Err(bad("direct bitmap extent"));
    }
    let mut source = reader.rect()?;
    let destination = reader.rect()?;
    let mode = reader.word()?;
    if mode > 7 || !positive(source) || !positive(destination) {
        return Err(bad("direct rectangles or transfer mode"));
    }
    source.left -= bounds.left;
    source.right -= bounds.left;
    source.top -= bounds.top;
    source.bottom -= bounds.top;
    if source.right - source.left > 4096 || source.bottom - source.top > 4096 {
        return Err(bad("direct source rectangle limit"));
    }
    let mask = if opcode == 0x9b {
        Some(reader.region()?)
    } else {
        None
    };
    let clips = match (clips, mask) {
        (Some(a), Some(b)) => {
            if a.len().saturating_mul(b.len()) > 65536 {
                return Err(bad("direct mask intersection limit"));
            }
            Some(
                a.iter()
                    .flat_map(|a| {
                        b.iter().map(|b| {
                            Rect::new(
                                a.left.max(b.left),
                                a.top.max(b.top),
                                a.right.min(b.right),
                                a.bottom.min(b.bottom),
                            )
                        })
                    })
                    .filter(|r| positive(*r))
                    .collect(),
            )
        }
        (Some(a), None) => Some(a.clone()),
        (None, b) => b,
    };
    let pack = if packed == 0 {
        if depth == 16 {
            3
        } else {
            4
        }
    } else {
        packed
    };
    let mut rgb = Vec::with_capacity(width * height);
    for _ in 0..height {
        let raw = stride < 8 || pack == 1;
        let row = if raw {
            reader.take(stride)?.to_vec()
        } else if pack == 2 {
            reader.take(stride / 4 * 3)?.to_vec()
        } else {
            let size = if stride > 250 {
                usize::from(reader.word()?)
            } else {
                usize::from(reader.byte()?)
            };
            let bytes = reader.take(size)?;
            if depth == 16 {
                unpack_words(bytes, stride)?
            } else {
                unpack(bytes, stride / 4 * components)?
            }
        };
        for x in 0..width {
            let color = if depth == 16 {
                let pixel = u16::from_be_bytes([row[2 * x], row[2 * x + 1]]);
                [10, 5, 0].map(|shift| {
                    let value = (pixel >> shift) & 31;
                    // Repeat the five stored bits across the sixteen-bit component.
                    (value << 11) | (value << 6) | (value << 1) | (value >> 4)
                })
            } else if raw {
                [row[4 * x + 1], row[4 * x + 2], row[4 * x + 3]].map(|v| u16::from(v) * 257)
            } else if pack == 2 {
                [row[3 * x], row[3 * x + 1], row[3 * x + 2]].map(|v| u16::from(v) * 257)
            } else {
                let plane = stride / 4;
                let first = components - 3;
                [
                    row[first * plane + x],
                    row[(first + 1) * plane + x],
                    row[(first + 2) * plane + x],
                ]
                .map(|v| u16::from(v) * 257)
            };
            rgb.push(color);
        }
    }
    Ok(BitmapCommand {
        width: width as u32,
        height: height as u32,
        indices: Vec::new(),
        rgb: Some(rgb),
        palette: None,
        source,
        destination,
        clips,
        mode,
        foreground: None,
        background: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn word_packbits_repeats_whole_pixels_and_checks_exact_extent() {
        assert_eq!(
            unpack_words(&[128, 0xfe, 0x12, 0x34], 6).unwrap(),
            [0x12, 0x34, 0x12, 0x34, 0x12, 0x34]
        );
        for (bytes, size) in [
            (&[0xfe, 0x12, 0x34][..], 4),
            (&[0xfe, 0x12][..], 6),
            (&[0, 0x12, 0x34][..], 4),
        ] {
            assert!(unpack_words(bytes, size).is_err());
        }
    }
}
