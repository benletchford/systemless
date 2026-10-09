//! Packed Font Manager records used by FMSwapFont.
//! Inside Macintosh: Text (1993), pp. 4-36--4-37 and 4-60.
//! Apple Technical Note Text 21 / #26 (May 1992), p. 6, specifies
//! output scaling fractions normalized to a denominator of 256.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct FontSwapInput {
    pub family: i16,
    pub size: i16,
    pub face: u8,
    pub need_bits: bool,
    pub device: i16,
    pub numer: [i16; 2],
    pub denom: [i16; 2],
}

impl FontSwapInput {
    pub(super) fn decode(bytes: &[u8]) -> Option<Self> {
        let bytes: &[u8; 16] = bytes.try_into().ok()?;
        let word = |offset| i16::from_be_bytes([bytes[offset], bytes[offset + 1]]);
        Some(Self {
            family: word(0),
            size: word(2),
            face: bytes[4],
            need_bits: bytes[5] != 0,
            device: word(6),
            numer: [word(8), word(10)],
            denom: [word(12), word(14)],
        })
    }

    /// Compose point-size substitution with the client's independent
    /// vertical/horizontal fractions. The chosen font's unscaled metrics
    /// remain separate from these output fractions.
    pub(super) fn output_numerators(
        self,
        requested_size: i16,
        selected_size: i16,
    ) -> Option<[i16; 2]> {
        if requested_size <= 0 || selected_size <= 0 || self.denom.contains(&0) {
            return None;
        }
        let mut result = [0; 2];
        for (axis, value) in result.iter_mut().enumerate() {
            let numerator = i64::from(self.numer[axis]) * i64::from(requested_size) * 256;
            let denominator = i64::from(self.denom[axis]) * i64::from(selected_size);
            *value = i16::try_from(numerator / denominator).ok()?;
        }
        Some(result)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct FontSwapOutput {
    pub font_handle: u32,
    pub bold: u8,
    pub italic: u8,
    pub underline_offset: u8,
    pub underline_shadow: u8,
    pub underline_thickness: u8,
    pub shadow: u8,
    pub extra: i8,
    pub ascent: u8,
    pub descent: u8,
    pub widest: u8,
    pub leading: i8,
    pub actual_style: u8,
    pub numer: [i16; 2],
}

impl FontSwapOutput {
    pub(super) fn encode(self) -> [u8; 26] {
        let mut bytes = [0; 26];
        // errNum is reserved for the Font Manager and remains zero.
        bytes[2..6].copy_from_slice(&self.font_handle.to_be_bytes());
        bytes[6..18].copy_from_slice(&[
            self.bold,
            self.italic,
            self.underline_offset,
            self.underline_shadow,
            self.underline_thickness,
            self.shadow,
            self.extra as u8,
            self.ascent,
            self.descent,
            self.widest,
            self.leading as u8,
            self.actual_style,
        ]);
        bytes[18..20].copy_from_slice(&self.numer[0].to_be_bytes());
        bytes[20..22].copy_from_slice(&self.numer[1].to_be_bytes());
        bytes[22..24].copy_from_slice(&256i16.to_be_bytes());
        bytes[24..26].copy_from_slice(&256i16.to_be_bytes());
        bytes
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodes_the_verified_civilization_font_request() {
        let request =
            FontSwapInput::decode(&[0, 20, 0, 16, 1, 0, 0, 0, 0, 1, 0, 1, 0, 1, 0, 1]).unwrap();
        assert_eq!(request.family, 20);
        assert_eq!(request.size, 16);
        assert_eq!(request.face, 1);
        assert!(!request.need_bits);
        assert_eq!(request.device, 0);
        assert_eq!(request.output_numerators(16, 16), Some([256, 256]));
        assert!(FontSwapInput::decode(&[0; 15]).is_none());
        assert!(FontSwapInput::decode(&[0; 17]).is_none());
    }

    #[test]
    fn composes_strike_substitution_with_independent_axis_scaling() {
        let mut request =
            FontSwapInput::decode(&[0, 20, 0, 18, 0, 1, 0, 0, 0, 1, 0, 1, 0, 1, 0, 1]).unwrap();
        // Apple's Text 21 example: requested 18pt, available 12pt -> 384/256.
        assert_eq!(request.output_numerators(18, 12), Some([384, 384]));
        request.numer = [1, 2];
        request.denom = [3, 1];
        assert_eq!(request.output_numerators(18, 12), Some([128, 768]));
        request.denom[0] = 0;
        assert_eq!(request.output_numerators(18, 12), None);
        request.denom[0] = 1;
        assert_eq!(request.output_numerators(18, 0), None);
    }

    #[test]
    fn output_uses_the_packed_resource_handle_and_signed_metric_layout() {
        let output = FontSwapOutput {
            font_handle: 0x11223344,
            bold: 1,
            italic: 8,
            underline_offset: 1,
            underline_shadow: 0,
            underline_thickness: 1,
            shadow: 2,
            extra: -1,
            ascent: 12,
            descent: 3,
            widest: 14,
            leading: -2,
            actual_style: 0,
            numer: [384, 256],
        }
        .encode();
        assert_eq!(
            output,
            [
                0, 0, 0x11, 0x22, 0x33, 0x44, 1, 8, 1, 0, 1, 2, 0xff, 12, 3, 14, 0xfe, 0, 1, 0x80,
                1, 0, 1, 0, 1, 0,
            ]
        );
    }
}

use super::*;
use crate::quickdraw::fonts::FontMetrics;

const OUTPUT_ADDRESS: u32 = 0x998;

fn bitmap_metrics(bytes: &[u8]) -> Option<FontMetrics> {
    let word = |offset| {
        bytes
            .get(offset..offset + 2)
            .map(|s| u16::from_be_bytes([s[0], s[1]]))
    };
    let first = usize::from(word(2)?);
    let last = usize::from(word(4)?);
    let height = usize::from(word(14)?);
    let row_words = usize::from(word(24)?);
    if first > last || last > 255 || height == 0 || row_words == 0 {
        return None;
    }
    let locations = 26usize.checked_add(row_words.checked_mul(2)?.checked_mul(height)?)?;
    let glyphs = last - first + 2;
    let locations_end = locations.checked_add((glyphs + 1).checked_mul(2)?)?;
    let widths = 16usize.checked_add(usize::from(word(16)?).checked_mul(2)?)?;
    if widths < locations_end || widths.checked_add(glyphs.checked_mul(2)?)? > bytes.len() {
        return None;
    }
    let mut previous = 0;
    for index in 0..=glyphs {
        let location = usize::from(word(locations + index * 2)?);
        if location < previous || location > row_words * 16 {
            return None;
        }
        previous = location;
    }
    Some(FontMetrics {
        ascent: word(18)? as i16,
        descent: word(20)? as i16,
        wid_max: word(6)? as i16,
        leading: word(22)? as i16,
    })
}

fn outline_metrics(bytes: &[u8], size: i16) -> Option<FontMetrics> {
    use skrifa::{
        instance::{LocationRef, Size},
        FontRef, MetadataProvider,
    };
    let font = FontRef::new(bytes).ok()?;
    let metrics = font.metrics(Size::new(f32::from(size)), LocationRef::default());
    Some(FontMetrics {
        ascent: metrics.ascent.ceil().clamp(0.0, f32::from(i16::MAX)) as i16,
        descent: (-metrics.descent).ceil().clamp(0.0, f32::from(i16::MAX)) as i16,
        wid_max: metrics.max_width?.round().clamp(0.0, f32::from(i16::MAX)) as i16,
        leading: metrics
            .leading
            .round()
            .clamp(f32::from(i16::MIN), f32::from(i16::MAX)) as i16,
    })
}

#[derive(Clone, Copy)]
struct SelectedFont {
    index: usize,
    size: i16,
    style: u8,
    metrics: FontMetrics,
}

// Apple Text 21, p. 7, documents the relative weights of italic (8),
// bold (4) and outline (3). Other synthesized variations break ties.
fn font_style_weight(style: u8) -> u8 {
    u8::from(style & 2 != 0) * 8 + u8::from(style & 1 != 0) * 4 + u8::from(style & 8 != 0) * 3
}

fn select_resource_font(
    resources: &[PpcVfsResourceRecord],
    family: i16,
    size: i16,
    face: u8,
    current: i16,
) -> Option<SelectedFont> {
    let mut choices = Vec::new();
    for fond in resources.iter().filter(|r| {
        r.ref_num != PPC_CLOSED_RESOURCE_REF_NUM
            && r.res_type == u32::from_be_bytes(*b"FOND")
            && r.res_id == family
    }) {
        let Some(associations) =
            crate::quickdraw::fonts::parse_fond_associations(family, &fond.data)
        else {
            continue;
        };
        for association in associations {
            let Some((index, font)) = resources.iter().enumerate().find(|(_,r)|
                r.ref_num == fond.ref_num && r.path == fond.path && r.res_id == association.font_resource_id
                && (if association.size == 0 { r.res_type == u32::from_be_bytes(*b"sfnt") }
                    else { matches!(r.res_type, x if x == u32::from_be_bytes(*b"FONT") || x == u32::from_be_bytes(*b"NFNT")) })) else { continue; };
            let metrics = if association.size == 0 {
                outline_metrics(&font.data, size)
            } else {
                bitmap_metrics(&font.data)
            };
            if let Some(metrics) = metrics {
                choices.push((
                    font.ref_num,
                    SelectedFont {
                        index,
                        size: if association.size == 0 {
                            size
                        } else {
                            association.size
                        },
                        style: association.style as u8,
                        metrics,
                    },
                    association.size == 0,
                ));
            }
        }
    }
    // Original FONT resources encode their family and point size in the ID.
    for (index, font) in resources.iter().enumerate().filter(|(_, r)| {
        r.ref_num != PPC_CLOSED_RESOURCE_REF_NUM
            && r.res_type == u32::from_be_bytes(*b"FONT")
            && r.res_id >= 0
            && r.res_id / 128 == family
    }) {
        if let Some(metrics) = bitmap_metrics(&font.data) {
            let strike_size = font.res_id % 128;
            if strike_size > 0 {
                choices.push((
                    font.ref_num,
                    SelectedFont {
                        index,
                        size: strike_size,
                        style: 0,
                        metrics,
                    },
                    false,
                ));
            }
        }
    }
    choices
        .into_iter()
        .min_by_key(|(refnum, font, outline)| {
            let strike = i32::from(font.size);
            let request = i32::from(size);
            let size_rank = if !outline && strike == request {
                (0, 0)
            } else if *outline {
                (1, 0)
            } else if strike == request * 2 {
                (2, 0)
            } else if strike * 2 == request {
                (3, 0)
            } else if strike > request {
                (4, strike - request)
            } else {
                (5, request - strike)
            };
            (
                u8::from(*refnum != current),
                size_rank,
                font_style_weight(font.style).abs_diff(font_style_weight(face)),
                ((font.style ^ face) & 0x74).count_ones(),
            )
        })
        .map(|(_, font, _)| font)
}

#[allow(clippy::too_many_arguments)]
pub(super) fn swap_font(
    input_ptr: u32,
    memory: &mut PpcSectionMem,
    manager: &mut ProcessNativeMemoryManager,
    heap_cursor: &mut u32,
    heap_limit: u32,
    last_mem_error: &mut i16,
    handles: &mut Vec<PpcHandleRecord>,
    resources: &mut Vec<PpcVfsResourceRecord>,
    current: i16,
    last_resource_error: &mut i16,
) -> u32 {
    let mut bytes = [0; 16];
    if input_ptr == 0
        || memory.read_bytes_into(input_ptr, &mut bytes).is_none()
        || !ppc_memory_can_write_bytes(memory, OUTPUT_ADDRESS, 26)
    {
        return 0;
    }
    let Some(input) = FontSwapInput::decode(&bytes) else {
        return 0;
    };
    // Systemless has a screen text device. Do not fabricate printer driver characterization.
    if input.device as u16 & 0xff00 != 0 || input.denom.contains(&0) {
        return 0;
    }
    let family = if input.family == FONT_APPLICATION {
        3
    } else {
        input.family
    };
    let size = if input.size == 0 { 12 } else { input.size };
    if size <= 0 {
        return 0;
    }
    // TrueType font lookup uses the vertically scaled point size (Text 21,
    // p. 6). Looking up the unscaled size can incorrectly prefer a bitmap.
    let scaled = i64::from(size) * i64::from(input.numer[0]);
    let denominator = i64::from(input.denom[0]);
    if scaled <= 0 || denominator <= 0 {
        return 0;
    }
    let Ok(effective_size) = i16::try_from((scaled + denominator / 2) / denominator) else {
        return 0;
    };
    if effective_size <= 0 {
        return 0;
    }
    let selected = select_resource_font(resources, family, effective_size, input.face, current);
    let selected = if let Some(font) = selected {
        font
    } else {
        let fallback_family = if crate::quickdraw::fonts::bundled_font_bytes(family).is_some() {
            family
        } else {
            3
        };
        let Some(font_bytes) = crate::quickdraw::fonts::bundled_font_bytes(fallback_family) else {
            return 0;
        };
        let Some(metrics) = outline_metrics(font_bytes, effective_size) else {
            return 0;
        };
        let path = "__system__/FontManager";
        let index = if let Some(index) = resources.iter().position(|r| {
            r.path == path
                && r.res_id == fallback_family
                && r.res_type == u32::from_be_bytes(*b"sfnt")
        }) {
            index
        } else {
            resources.push(PpcVfsResourceRecord {
                ref_num: 0,
                path: path.to_string(),
                res_type: u32::from_be_bytes(*b"sfnt"),
                res_id: fallback_family,
                name: font_name_for_id(fallback_family)
                    .unwrap_or("System Font")
                    .as_bytes()
                    .to_vec(),
                data: font_bytes.to_vec(),
                raw_data: None,
                raw_attrs: None,
                attrs: 0,
                handle: 0,
            });
            resources.len() - 1
        };
        SelectedFont {
            index,
            size: effective_size,
            style: 0,
            metrics,
        }
    };
    let Some(numer) = input.output_numerators(size, selected.size) else {
        return 0;
    };
    let handle = ppc_materialize_vfs_resource_handle(
        manager,
        memory,
        heap_cursor,
        heap_limit,
        last_mem_error,
        handles,
        resources,
        selected.index,
        true,
        last_resource_error,
    );
    if handle == 0 {
        return 0;
    }
    let synthesized = input.face & !selected.style;
    let bold = u8::from(synthesized & 1 != 0);
    let italic = if synthesized & 2 != 0 { 8 } else { 0 };
    let underline = u8::from(synthesized & 4 != 0);
    let shadow = if synthesized & 0x10 != 0 { 2 } else { 0 };
    let extra = i8::from(bold != 0)
        + i8::from(synthesized & 8 != 0) * 2
        + i8::from(shadow != 0)
        + i8::from(synthesized & 0x40 != 0)
        - i8::from(synthesized & 0x20 != 0);
    let metric = |value: i16| value.clamp(0, 255) as u8;
    let output = FontSwapOutput {
        font_handle: handle,
        bold,
        italic,
        underline_offset: underline,
        underline_shadow: u8::from(underline != 0 && shadow != 0),
        underline_thickness: underline,
        shadow,
        extra,
        ascent: metric(selected.metrics.ascent),
        descent: metric(selected.metrics.descent),
        widest: metric(selected.metrics.wid_max),
        leading: selected.metrics.leading.clamp(-128, 127) as i8,
        actual_style: selected.style,
        numer,
    }
    .encode();
    if memory.write_bytes(OUTPUT_ADDRESS, &output).is_none() {
        return 0;
    }
    OUTPUT_ADDRESS
}
