//! Guest FONT, NFNT, FOND and sfnt resource decoding.
use super::*;

/// Bitmap strikes supplied by the currently loaded Classic Mac application.
/// The decoded storage is leaked deliberately: font resources are tiny and
/// glyph references are handed through the renderer as `'static` slices.
/// A Systemless process hosts one guest application, while replacement lets a
/// later resource fork with the same family/size take precedence.
pub(super) static RESOURCE_FACES: LazyLock<Mutex<HashMap<(i16, i16), &'static FontFace>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));
pub(super) static RESOURCE_MACROMAN_FACES: LazyLock<
    Mutex<HashMap<(i16, i16), &'static MacRomanFace>>,
> = LazyLock::new(|| Mutex::new(HashMap::new()));
pub(super) static RESOURCE_OUTLINE_FONTS: LazyLock<Mutex<HashMap<i16, &'static [u8]>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

fn resource_word(data: &[u8], offset: usize) -> Option<u16> {
    Some(u16::from_be_bytes([
        *data.get(offset)?,
        *data.get(offset + 1)?,
    ]))
}

/// One entry from a font family (`FOND`) resource's association table.
///
/// `font_resource_id` identifies an `NFNT`, `FONT`, or `sfnt` resource;
/// unlike old-style `FONT` IDs, an `NFNT` ID does not encode its family or
/// point size. *Inside Macintosh: Text* (1993), pp. 4-47–4-48 and 4-95–4-96.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct FondAssociation {
    pub family_id: i16,
    pub size: i16,
    pub style: u16,
    pub font_resource_id: i16,
}

/// Decode the mandatory association table that immediately follows a
/// `FOND` resource's 52-byte `FamRec` header.
pub(crate) fn parse_fond_associations(
    fond_resource_id: i16,
    bytes: &[u8],
) -> Option<Vec<FondAssociation>> {
    const FAM_REC_LEN: usize = 52;
    const ASSOCIATION_LEN: usize = 6;
    let count_minus_one = resource_word(bytes, FAM_REC_LEN)? as i16;
    if count_minus_one < -1 {
        return None;
    }
    let count = usize::try_from(i32::from(count_minus_one) + 1).ok()?;
    let table_len = count.checked_mul(ASSOCIATION_LEN)?;
    let table_end = FAM_REC_LEN.checked_add(2)?.checked_add(table_len)?;
    if table_end > bytes.len() {
        return None;
    }

    let mut associations = Vec::with_capacity(count);
    for index in 0..count {
        let offset = FAM_REC_LEN + 2 + index * ASSOCIATION_LEN;
        associations.push(FondAssociation {
            // The family number used by Font Manager clients is the FOND
            // resource ID. FamRec.ffFamID redundantly records that value.
            family_id: fond_resource_id,
            size: resource_word(bytes, offset)? as i16,
            style: resource_word(bytes, offset + 2)?,
            font_resource_id: resource_word(bytes, offset + 4)? as i16,
        });
    }
    Some(associations)
}

/// Decode a classic `FONT` bitmap strike whose resource ID uses the original
/// `family * 128 + pointSize` encoding. `NFNT` resources normally use the
/// arbitrary IDs recorded by a `FOND` association table and should call
/// [`register_resource_font_strike_for_family`] instead.
pub(crate) fn register_resource_font_strike(resource_id: i16, bytes: &[u8]) -> bool {
    if resource_id <= 0 {
        return false;
    }
    let family_id = resource_id / 128;
    let size = resource_id % 128;
    if size <= 0 {
        return false;
    }
    register_resource_font_strike_for_family(family_id, size, bytes)
}

/// Decode and register a bitmap strike under the family and point size from
/// its `FOND` association entry. The resource bytes stay in the user's
/// application; Systemless expands their 1-bit glyph bitmap into the same
/// runtime coverage representation used by its built-in faces.
pub(crate) fn register_resource_font_strike_for_family(
    family_id: i16,
    size: i16,
    bytes: &[u8],
) -> bool {
    const HEADER_LEN: usize = 26;
    if family_id < 0 || size <= 0 || bytes.len() < HEADER_LEN {
        return false;
    }

    let first_char = resource_word(bytes, 2)
        .map(usize::from)
        .unwrap_or(usize::MAX);
    let last_char = resource_word(bytes, 4).map(usize::from).unwrap_or(0);
    let wid_max = resource_word(bytes, 6).map(|v| v as i16).unwrap_or(0);
    let kern_max = resource_word(bytes, 8).map(|v| v as i16).unwrap_or(0);
    let f_rect_height = resource_word(bytes, 14).map(usize::from).unwrap_or(0);
    let ow_t_loc = resource_word(bytes, 16).map(usize::from).unwrap_or(0);
    let ascent = resource_word(bytes, 18).map(|v| v as i16).unwrap_or(0);
    let descent = resource_word(bytes, 20).map(|v| v as i16).unwrap_or(0);
    let leading = resource_word(bytes, 22).map(|v| v as i16).unwrap_or(0);
    let row_words = resource_word(bytes, 24).map(usize::from).unwrap_or(0);
    if first_char > last_char
        || last_char > 255
        || f_rect_height == 0
        || f_rect_height > u8::MAX as usize
        || row_words == 0
    {
        return false;
    }

    let bitmap_len = match row_words
        .checked_mul(2)
        .and_then(|row_bytes| row_bytes.checked_mul(f_rect_height))
    {
        Some(len) => len,
        None => return false,
    };
    let location_offset = HEADER_LEN + bitmap_len;
    // One glyph per encoded character plus the missing-character glyph; the
    // location table has one additional terminal entry.
    let glyph_count = last_char - first_char + 2;
    let location_count = glyph_count + 1;
    let location_end = match location_offset.checked_add(location_count * 2) {
        Some(end) => end,
        None => return false,
    };
    // FontRec.owTLoc is measured in words from the owTLoc field itself
    // (byte offset 16), not from the start of the resource.
    let ow_offset = match 16usize.checked_add(ow_t_loc * 2) {
        Some(offset) => offset,
        None => return false,
    };
    if HEADER_LEN + bitmap_len > bytes.len()
        || location_end > bytes.len()
        || ow_offset < location_end
        || ow_offset.saturating_add(glyph_count * 2) > bytes.len()
    {
        return false;
    }
    let bitmap_width = row_words * 16;
    let missing_index = last_char - first_char + 1;
    let mut coverage = Vec::new();

    let mut decode_index = |mut index: usize| -> Option<Glyph> {
        if index >= glyph_count {
            index = missing_index;
        }
        let mut ow = resource_word(bytes, ow_offset + index * 2)?;
        if ow == 0xFFFF && index != missing_index {
            index = missing_index;
            ow = resource_word(bytes, ow_offset + index * 2)?;
        }
        if ow == 0xFFFF {
            return None;
        }
        let start = resource_word(bytes, location_offset + index * 2)? as usize;
        let end = resource_word(bytes, location_offset + (index + 1) * 2)? as usize;
        if end < start || end > bitmap_width || end - start > u8::MAX as usize {
            return None;
        }
        let width = end - start;
        let data_offset = coverage.len();
        coverage.reserve(width.saturating_mul(f_rect_height));
        let row_bytes = row_words * 2;
        for row in 0..f_rect_height {
            let row_start = HEADER_LEN + row * row_bytes;
            for column in start..end {
                let byte = *bytes.get(row_start + column / 8)?;
                let mask = 0x80 >> (column & 7);
                coverage.push(if byte & mask != 0 { 255 } else { 0 });
            }
        }
        // The offset byte is unsigned; adding the (normally negative)
        // kernMax converts it to the signed displacement from the pen.
        let origin_x = kern_max.saturating_add(i16::from((ow >> 8) as u8));
        Some(Glyph {
            width: width as u8,
            height: f_rect_height as u8,
            advance: (ow & 0x00FF) as u8,
            origin_x: origin_x.clamp(i8::MIN as i16, i8::MAX as i16) as i8,
            origin_y: (-ascent).clamp(i8::MIN as i16, i8::MAX as i16) as i8,
            data_offset,
        })
    };

    let mut ascii = Vec::with_capacity(95);
    for code in 0x20usize..=0x7E {
        let index = code
            .checked_sub(first_char)
            .filter(|_| code <= last_char)
            .unwrap_or(missing_index);
        ascii.push(decode_index(index).unwrap_or(Glyph {
            width: 0,
            height: 0,
            advance: wid_max.clamp(0, u8::MAX as i16) as u8,
            origin_x: 0,
            origin_y: 0,
            data_offset: 0,
        }));
    }
    let mut macroman = Vec::new();
    for code in 0x80usize..=0xFF {
        if code < first_char || code > last_char {
            continue;
        }
        if let Some(glyph) = decode_index(code - first_char) {
            macroman.push(MacRomanGlyph {
                mac_code: code as u8,
                glyph,
            });
        }
    }

    let coverage: &'static [u8] = Box::leak(coverage.into_boxed_slice());
    let ascii: &'static [Glyph] = Box::leak(ascii.into_boxed_slice());
    let face = Box::leak(Box::new(FontFace {
        font_id: family_id,
        size,
        metrics: FontMetrics {
            ascent,
            descent,
            wid_max,
            leading,
        },
        glyphs: ascii,
        data: coverage,
    }));
    RESOURCE_FACES
        .lock()
        .expect("resource font cache poisoned")
        .insert((family_id, size), face);
    if !macroman.is_empty() {
        let glyphs: &'static [MacRomanGlyph] = Box::leak(macroman.into_boxed_slice());
        let face = Box::leak(Box::new(MacRomanFace {
            font_id: family_id,
            size,
            glyphs,
            data: coverage,
        }));
        RESOURCE_MACROMAN_FACES
            .lock()
            .expect("resource Mac Roman font cache poisoned")
            .insert((family_id, size), face);
    }
    true
}

/// Register a scalable `sfnt` resource for lazy rasterization at the point
/// size requested by QuickDraw. Inside Macintosh: Text (1993), pp. 4-47–4-48
/// and 4-97–4-98, describes `sfnt` resources as the outline data associated
/// with a FOND and specifies that the Font Manager scales outline fonts.
pub(crate) fn register_resource_outline_font(family_id: i16, bytes: &[u8]) -> bool {
    if family_id < 0 || skrifa::FontRef::new(bytes).is_err() {
        return false;
    }
    let bytes = Box::leak(bytes.to_vec().into_boxed_slice());
    RESOURCE_OUTLINE_FONTS
        .lock()
        .expect("resource outline font cache poisoned")
        .insert(family_id, bytes);
    true
}

pub(super) fn rasterize_resource_outline_face(
    font_id: i16,
    size: i16,
) -> Option<&'static FontFace> {
    if !(1..=96).contains(&size) {
        return None;
    }
    let bytes = RESOURCE_OUTLINE_FONTS
        .lock()
        .expect("resource outline font cache poisoned")
        .get(&font_id)
        .copied()?;
    let (face, extended) = outline::rasterize(font_id, size, bytes)?;
    RESOURCE_FACES
        .lock()
        .expect("resource font cache poisoned")
        .insert((font_id, size), face);
    RESOURCE_MACROMAN_FACES
        .lock()
        .expect("resource Mac Roman font cache poisoned")
        .insert((font_id, size), extended);
    Some(face)
}
