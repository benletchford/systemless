//! Shared rasterization and caching for bundled and guest TrueType outlines.
use super::bundled::bytes;
use super::*;

type Faces = (&'static FontFace, &'static MacRomanFace);
static FACES: LazyLock<Mutex<HashMap<(i16, i16), Faces>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

// Stable glyph descriptors identify their outline source without scanning faces.
#[derive(Clone, Copy)]
struct Source {
    bytes: &'static [u8],
    size: i16,
    id: skrifa::GlyphId,
}
static SOURCES: LazyLock<Mutex<HashMap<usize, Source>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));
static MASKS: LazyLock<Mutex<HashMap<(usize, u32), crate::memory::presentation::OutlineGlyph>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

pub(super) fn face(font_id: i16, size: i16) -> Option<Faces> {
    // Bound permanent cached storage and keep bearings within Glyph's i8
    // representation. Larger requests retain the existing bitmap scaling.
    if !(1..=96).contains(&size) {
        return None;
    }
    let bytes = bytes(font_id)?;
    let mut cache = FACES.lock().expect("URW font cache poisoned");
    if let Some(faces) = cache.get(&(font_id, size)) {
        return Some(*faces);
    }
    let mut faces = rasterize(
        font_id,
        size,
        bytes,
        super::compatibility::bundled_advances(font_id, size),
        super::compatibility::bundled_wid_max(font_id, size),
    )?;
    // Coppet is an optical-size-specific ASCII substitute. Retain the
    // established GetFontInfo metrics and extended Mac Roman fallback.
    // Guest resources and explicit overrides are resolved before this path.
    if size == 9 && matches!(font_id, FONT_APPLICATION | FONT_GENEVA) {
        let (ascii, _) = rasterize(
            font_id,
            size,
            include_bytes!("coppet/Coppet-Regular.ttf"),
            super::compatibility::bundled_advances(font_id, size),
            super::compatibility::bundled_wid_max(font_id, size),
        )?;
        faces.0 = Box::leak(Box::new(FontFace {
            font_id,
            size,
            metrics: faces.0.metrics,
            glyphs: ascii.glyphs,
            data: ascii.data,
        }));
    }
    // Monaco is a compact sans-serif screen face. The serif Courier fallback
    // loses definition at nine points; use the OFL Noto monospace outlines for
    // ASCII at that optical size, retaining the existing logical line metrics.
    if size == 9 && font_id == FONT_MONACO {
        let (ascii, _) = rasterize(
            font_id,
            size,
            include_bytes!("noto/NotoSansMono-Regular.ttf"),
            None,
            super::compatibility::bundled_wid_max(font_id, size),
        )?;
        faces.0 = Box::leak(Box::new(FontFace {
            font_id,
            size,
            metrics: faces.0.metrics,
            glyphs: ascii.glyphs,
            data: ascii.data,
        }));
    }
    cache.insert((font_id, size), faces);
    Some(faces)
}

#[derive(Default)]
struct OutlinePath(Vec<zeno::Command>);

impl skrifa::outline::OutlinePen for OutlinePath {
    fn move_to(&mut self, x: f32, y: f32) {
        self.0.push(zeno::Command::MoveTo((x, y).into()));
    }
    fn line_to(&mut self, x: f32, y: f32) {
        self.0.push(zeno::Command::LineTo((x, y).into()));
    }
    fn quad_to(&mut self, cx: f32, cy: f32, x: f32, y: f32) {
        self.0
            .push(zeno::Command::QuadTo((cx, cy).into(), (x, y).into()));
    }
    fn curve_to(&mut self, cx0: f32, cy0: f32, cx1: f32, cy1: f32, x: f32, y: f32) {
        self.0.push(zeno::Command::CurveTo(
            (cx0, cy0).into(),
            (cx1, cy1).into(),
            (x, y).into(),
        ));
    }
    fn close(&mut self) {
        self.0.push(zeno::Command::Close);
    }
}

/// The guest's one-bit strike of a hinted outline.
pub(super) struct BinaryStrike {
    /// `width * height` bytes, 0 or 255, top row first.
    pub mask: Vec<u8>,
    pub width: usize,
    pub height: usize,
    /// Column of the mask's left edge and row of its top edge, in pixels
    /// from the glyph origin (the top edge counts upwards).
    pub left: i32,
    pub top: i32,
}

/// Scan-convert `path` (pixel units, y up) the way a TrueType rasterizer makes
/// a bitmap: a pixel is on when its centre is inside the outline, and dropout
/// control turns on one pixel wherever a span of ink passes between two
/// adjacent centres, along a row or a column, so a stroke thinner than a
/// pixel never breaks. Stubs, where the stroke ends before the next scanline,
/// are left out. Thresholding area coverage instead loses the thinner of
/// two strokes that share a run of pixels, such as the apex of an "A".
pub(super) fn scan_convert(path: &[zeno::Command]) -> Option<BinaryStrike> {
    // Hinted TrueType outlines are lines and quadratics; cubics come from
    // other outline formats. A fixed tolerance keeps crossings well within a
    // pixel at every strike size.
    const TOLERANCE: f32 = 0.02;
    let mut segments: Vec<[f32; 4]> = Vec::new();
    let (mut start, mut pen) = ((0.0f32, 0.0f32), (0.0f32, 0.0f32));
    let mut line = |from: (f32, f32), to: (f32, f32)| {
        if from != to {
            segments.push([from.0, from.1, to.0, to.1]);
        }
    };
    for command in path {
        match *command {
            zeno::Command::MoveTo(to) => {
                line(pen, start);
                start = (to.x, to.y);
                pen = start;
            }
            zeno::Command::LineTo(to) => {
                line(pen, (to.x, to.y));
                pen = (to.x, to.y);
            }
            zeno::Command::QuadTo(control, to) => {
                let bend = ((pen.0 - 2.0 * control.x + to.x).abs())
                    .max((pen.1 - 2.0 * control.y + to.y).abs());
                let steps = ((bend / (4.0 * TOLERANCE)).sqrt().ceil() as usize).clamp(1, 64);
                let from = pen;
                for step in 1..=steps {
                    let t = step as f32 / steps as f32;
                    let u = 1.0 - t;
                    let next = (
                        u * u * from.0 + 2.0 * u * t * control.x + t * t * to.x,
                        u * u * from.1 + 2.0 * u * t * control.y + t * t * to.y,
                    );
                    line(pen, next);
                    pen = next;
                }
            }
            zeno::Command::CurveTo(first, second, to) => {
                let bend = ((pen.0 - 2.0 * first.x + second.x).abs())
                    .max((pen.1 - 2.0 * first.y + second.y).abs())
                    .max((first.x - 2.0 * second.x + to.x).abs())
                    .max((first.y - 2.0 * second.y + to.y).abs());
                let steps = ((3.0 * bend / (4.0 * TOLERANCE)).sqrt().ceil() as usize).clamp(1, 96);
                let from = pen;
                for step in 1..=steps {
                    let t = step as f32 / steps as f32;
                    let u = 1.0 - t;
                    let next = (
                        u * u * u * from.0
                            + 3.0 * u * u * t * first.x
                            + 3.0 * u * t * t * second.x
                            + t * t * t * to.x,
                        u * u * u * from.1
                            + 3.0 * u * u * t * first.y
                            + 3.0 * u * t * t * second.y
                            + t * t * t * to.y,
                    );
                    line(pen, next);
                    pen = next;
                }
            }
            zeno::Command::Close => {
                line(pen, start);
                pen = start;
            }
        }
    }
    line(pen, start);
    if segments.is_empty() {
        return None;
    }
    let bound = |pick: fn(&[f32; 4]) -> (f32, f32)| {
        segments.iter().fold((f32::MAX, f32::MIN), |(low, high), segment| {
            let (a, b) = pick(segment);
            (low.min(a).min(b), high.max(a).max(b))
        })
    };
    let (x_min, x_max) = bound(|s| (s[0], s[2]));
    let (y_min, y_max) = bound(|s| (s[1], s[3]));
    let (left, right) = (x_min.floor() as i32, x_max.ceil() as i32);
    let (bottom, top) = (y_min.floor() as i32, y_max.ceil() as i32);
    let (width, height) = ((right - left).max(1) as usize, (top - bottom).max(1) as usize);
    let mut mask = vec![0u8; width * height];
    let mut set = |x: i32, y: i32| {
        if x >= left && x < right && y >= bottom && y < top {
            mask[(top - 1 - y) as usize * width + (x - left) as usize] = 255;
        }
    };
    // Spans of ink (non-zero winding) along the line where `along` is fixed:
    // rows when `rows` is set, columns otherwise.
    let spans = |rows: bool, at: f32, out: &mut Vec<(f32, f32)>| {
        let mut crossings: Vec<(f32, i32)> = segments
            .iter()
            .filter_map(|&[x0, y0, x1, y1]| {
                let (a0, b0, a1, b1) = if rows { (y0, x0, y1, x1) } else { (x0, y0, x1, y1) };
                ((a0 <= at) != (a1 <= at))
                    .then(|| (b0 + (at - a0) * (b1 - b0) / (a1 - a0), if a1 > a0 { 1 } else { -1 }))
            })
            .collect();
        crossings.sort_by(|a, b| a.0.total_cmp(&b.0));
        out.clear();
        let (mut winding, mut from) = (0, 0.0);
        for (position, direction) in crossings {
            if winding == 0 {
                from = position;
            }
            winding += direction;
            // Hinting can squeeze a stem to no width at all. Its two edges
            // still bound a span, which dropout control keeps.
            if winding == 0 {
                out.push((from, position));
            }
        }
    };
    // Dropout control leaves out stubs, as scan type 1 asks: a span between two
    // centres that is thinner than half a pixel is kept only where the stroke
    // carries on to the scanlines on both sides. The tip of a serif or
    // terminal that ends between two scanlines would otherwise grow into a
    // full pixel.
    let continues = |lines: &[Vec<(f32, f32)>], index: usize, span: (f32, f32)| {
        if span.1 - span.0 >= 0.5 {
            return true;
        }
        let meets = |line: Option<&Vec<(f32, f32)>>| {
            line.is_some_and(|line| line.iter().any(|&(from, to)| from <= span.1 + 1.0 && to >= span.0 - 1.0))
        };
        meets(index.checked_sub(1).and_then(|index| lines.get(index))) && meets(lines.get(index + 1))
    };
    let mut scratch = Vec::new();
    let mut rows = Vec::with_capacity(height);
    for y in bottom..top {
        spans(true, y as f32 + 0.5, &mut scratch);
        rows.push(scratch.clone());
    }
    let mut columns = Vec::with_capacity(width);
    for x in left..right {
        spans(false, x as f32 + 0.5, &mut scratch);
        columns.push(scratch.clone());
    }
    for (index, y) in (bottom..top).enumerate() {
        for &(from, to) in &rows[index] {
            // Pixels whose centres lie in [from, to).
            let (first, last) = ((from - 0.5).ceil() as i32, (to - 0.5).ceil() as i32 - 1);
            if first <= last {
                (first..=last).for_each(|x| set(x, y));
            } else if continues(&rows, index, (from, to)) {
                set(((from + to) / 2.0).floor() as i32, y);
            }
        }
    }
    for (index, x) in (left..right).enumerate() {
        for &(from, to) in &columns[index] {
            // Centres along a column were already tested by the row pass.
            if (from - 0.5).ceil() as i32 > (to - 0.5).ceil() as i32 - 1
                && continues(&columns, index, (from, to))
            {
                set(x, ((from + to) / 2.0).floor() as i32);
            }
        }
    }
    Some(BinaryStrike { mask, width, height, left, top })
}

// Inside Macintosh: Text (1993), pp. 4-7–4-9 and 4-18–4-19:
// outline fonts generate a strike at the requested point size. At the logical
// 72-dpi screen, one point is one em pixel. Keep QuickDraw's binary masks and
// boolean transfer modes; hint before thresholding to retain small stems.
pub(super) fn rasterize(
    font_id: i16,
    size: i16,
    bytes: &'static [u8],
    compatibility_advances: Option<&'static [u8; 95]>,
    compatibility_wid_max: Option<i16>,
) -> Option<Faces> {
    use skrifa::{
        instance::{LocationRef, Size},
        outline::{DrawSettings, HintingInstance, Target},
        FontRef, MetadataProvider,
    };
    let font = FontRef::new(bytes).ok()?;
    let ppem = Size::new(f32::from(size));
    let location = LocationRef::default();
    let metrics = font.metrics(ppem, location);
    let advances = font.glyph_metrics(ppem, location);
    let charmap = font.charmap();
    let parsed = ttf_parser::Face::parse(bytes, 0).ok()?;
    let macintosh_cmap = parsed.tables().cmap.and_then(|cmap| {
        cmap.subtables
            .into_iter()
            .find(|table| table.platform_id == ttf_parser::PlatformId::Macintosh)
    });
    let outlines = font.outline_glyphs();
    // The output is a one-bit QuickDraw mask. LCD hinting preserves fractional
    // stem positions and therefore loses strokes when thresholded. Mono fits
    // both axes to the pixel grid and returns matching adjusted advances.
    let hinter = HintingInstance::new(&outlines, ppem, location, Target::Mono).ok()?;
    let mut data = Vec::new();
    let mut ids = Vec::new();
    let mut glyph = |ch: char| {
        let id = charmap
            .map(ch)
            .or_else(|| {
                let code = crate::mac_roman::encode_mac_roman_char(ch)?;
                let mapped = macintosh_cmap?.glyph_index(u32::from(code))?;
                Some(skrifa::GlyphId::new(u32::from(mapped.0)))
            })
            .unwrap_or_default();
        ids.push(id);
        let mut path = OutlinePath::default();
        let adjusted = outlines.get(id).and_then(|outline| {
            outline
                .draw(DrawSettings::hinted(&hinter, false), &mut path)
                .ok()
        });
        let advance = adjusted
            .and_then(|metrics| metrics.advance_width)
            .or_else(|| advances.advance_width(id))
            .unwrap_or(0.0)
            .round()
            .clamp(0.0, 255.0) as u8;
        let mut result = Glyph {
            width: 0,
            height: 0,
            advance,
            origin_x: 0,
            origin_y: 0,
            data_offset: data.len(),
        };
        if let Some(strike) = scan_convert(path.0.as_slice()) {
            result.width = u8::try_from(strike.width).ok()?;
            result.height = u8::try_from(strike.height).ok()?;
            result.origin_x = i8::try_from(strike.left).ok()?;
            result.origin_y = i8::try_from(-strike.top).ok()?;
            data.extend(strike.mask);
        }
        Some(result)
    };
    let mut ascii = (super::compatibility::FIRST_ASCII_CODE
        ..=super::compatibility::LAST_ASCII_CODE)
        .map(|code| glyph(char::from(code)))
        .collect::<Option<Vec<_>>>()?;
    if let Some(compatibility_advances) = compatibility_advances {
        for (glyph, advance) in ascii.iter_mut().zip(compatibility_advances) {
            glyph.advance = *advance;
        }
    }
    let extended = (0x80u8..=0xff)
        .map(|code| {
            Some(MacRomanGlyph {
                mac_code: code,
                glyph: glyph(
                    crate::mac_roman::decode_mac_roman(&[code])
                        .chars()
                        .next()
                        .unwrap(),
                )?,
            })
        })
        .collect::<Option<Vec<_>>>()?;
    let all = || {
        ascii
            .iter()
            .chain(extended.iter().map(|entry| &entry.glyph))
    };
    let ascent = (metrics.ascent.ceil() as i16)
        .max(all().map(|g| -i16::from(g.origin_y)).max().unwrap_or(0));
    let descent = ((-metrics.descent).ceil() as i16).max(
        all()
            .map(|g| i16::from(g.origin_y) + i16::from(g.height))
            .max()
            .unwrap_or(0),
    );
    let mapped_wid_max = all().map(|g| i16::from(g.advance)).max().unwrap_or(0);
    // Inside Macintosh: Text (1993), pp. 3-66 and 3-74: GetFontInfo.widMax
    // is the integer width of the largest glyph in the selected font.
    let selected_wid_max = if let Some(wid_max) = compatibility_wid_max {
        wid_max
    } else if compatibility_advances.is_some() {
        mapped_wid_max
    } else {
        metrics
            .max_width
            .map(|width| width.round().clamp(0.0, f32::from(i16::MAX)) as i16)
            .unwrap_or(mapped_wid_max)
    };
    let wid_max = selected_wid_max.max(mapped_wid_max);
    let data = Box::leak(data.into_boxed_slice());
    let face = Box::leak(Box::new(FontFace {
        font_id,
        size,
        metrics: FontMetrics {
            ascent,
            descent,
            wid_max,
            leading: metrics.leading.round().max(0.0) as i16,
        },
        glyphs: Box::leak(ascii.into_boxed_slice()),
        data,
    }));
    let extended = Box::leak(Box::new(MacRomanFace {
        font_id,
        size,
        glyphs: Box::leak(extended.into_boxed_slice()),
        data,
    }));
    let mut sources = SOURCES.lock().ok()?;
    for (glyph, id) in face
        .glyphs
        .iter()
        .chain(extended.glyphs.iter().map(|g| &g.glyph))
        .zip(ids)
    {
        sources.insert(glyph as *const Glyph as usize, Source { bytes, size, id });
    }
    Some((face, extended))
}

/// Resolve only glyphs actually supplied by this fallback, preserving resource fonts.
pub(crate) fn presentation_glyph(
    glyph: &Glyph,
    data: &[u8],
    scale: u32,
) -> Option<crate::memory::presentation::OutlineGlyph> {
    let key = (glyph as *const Glyph as usize, scale);
    let _ = data;
    let mut masks = MASKS.lock().ok()?;
    if let Some(mask) = masks.get(&key) {
        return Some(mask.clone());
    }
    let source = *SOURCES.lock().ok()?.get(&key.0)?;
    use skrifa::{
        instance::{LocationRef, Size},
        outline::{DrawSettings, HintingInstance, SmoothMode, Target},
        FontRef, MetadataProvider,
    };
    let font = FontRef::new(source.bytes).ok()?;
    let outlines = font.outline_glyphs();
    let hint = HintingInstance::new(
        &outlines,
        Size::new(source.size as f32 * scale as f32),
        LocationRef::default(),
        Target::from(SmoothMode::Normal),
    )
    .ok()?;
    let mut path = OutlinePath::default();
    outlines
        .get(source.id)?
        .draw(DrawSettings::hinted(&hint, false), &mut path)
        .ok()?;
    let mut pixels = Vec::new();
    let placement = zeno::Mask::new(path.0.as_slice())
        .origin(zeno::Origin::BottomLeft)
        .inspect(|format, w, h| pixels.resize(format.buffer_size(w, h), 0))
        .render_into(&mut pixels, None);
    let mask = crate::memory::presentation::OutlineGlyph {
        pixels: pixels.into(),
        width: placement.width as i32,
        height: placement.height as i32,
        left: placement.left,
        top: -placement.top,
    };
    masks.insert(key, mask.clone());
    Some(mask)
}

type UnicodeGlyph = (&'static Glyph, &'static [u8]);
static UNICODE_GLYPHS: LazyLock<Mutex<HashMap<(i16, i16, char), UnicodeGlyph>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

/// UI symbols not present in the Roman face use the bundled Noto symbol font.
pub(crate) fn unicode_glyph(font_id: i16, size: i16, ch: char) -> Option<UnicodeGlyph> {
    use skrifa::{
        instance::{LocationRef, Size},
        outline::{DrawSettings, HintingInstance, Target},
        FontRef, MetadataProvider,
    };
    // txSize 0 selects the default size, 12 (IM:I I-153).
    let size = if size == 0 { 12 } else { size }.clamp(1, 96);
    let mut cache = UNICODE_GLYPHS.lock().ok()?;
    if let Some(glyph) = cache.get(&(font_id, size, ch)) {
        return Some(*glyph);
    }
    let primary = bytes(font_id).unwrap_or(bytes(FONT_APPLICATION)?);
    let primary_font = FontRef::new(primary).ok()?;
    let source_bytes: &'static [u8] = if primary_font.charmap().map(ch).is_some() {
        primary
    } else {
        include_bytes!("noto/NotoSansSymbols2-Regular.ttf")
    };
    let font = FontRef::new(source_bytes).ok()?;
    let id = font.charmap().map(ch)?;
    let outlines = font.outline_glyphs();
    let size_px = Size::new(size as f32);
    let hint =
        HintingInstance::new(&outlines, size_px, LocationRef::default(), Target::Mono).ok()?;
    let mut path = OutlinePath::default();
    let adjusted = outlines
        .get(id)?
        .draw(DrawSettings::hinted(&hint, false), &mut path)
        .ok()?;
    let advance = adjusted
        .advance_width
        .or_else(|| {
            font.glyph_metrics(size_px, LocationRef::default())
                .advance_width(id)
        })
        .unwrap_or(0.0)
        .round()
        .clamp(0.0, 255.0) as u8;
    let strike = scan_convert(path.0.as_slice())?;
    let glyph: &'static Glyph = Box::leak(Box::new(Glyph {
        width: strike.width.try_into().ok()?,
        height: strike.height.try_into().ok()?,
        origin_x: strike.left.try_into().ok()?,
        origin_y: (-strike.top).try_into().ok()?,
        advance,
        data_offset: 0,
    }));
    let data: &'static [u8] = Box::leak(strike.mask.into_boxed_slice());
    SOURCES.lock().ok()?.insert(
        glyph as *const Glyph as usize,
        Source {
            bytes: source_bytes,
            size,
            id,
        },
    );
    cache.insert((font_id, size, ch), (glyph, data));
    Some((glyph, data))
}

#[cfg(test)]
mod tests {
    use super::*;

    const GENEVA9_ADVANCES: &[u8; 95] = include_bytes!("compatibility/geneva9-advances.bin");
    const EXPECTED_GENEVA9_ADVANCES: [u8; 95] = [
        3, 3, 5, 7, 6, 9, 8, 3, 4, 4, 7, 6, 4, 5, 3, 6, 6, 6, 6, 6, 6, 6, 6, 6, 6, 6, 4, 4, 5, 6,
        5, 6, 8, 7, 6, 6, 6, 5, 5, 6, 6, 3, 6, 6, 5, 8, 6, 6, 6, 6, 6, 6, 6, 6, 6, 8, 6, 6, 5, 4,
        6, 4, 4, 6, 3, 5, 5, 5, 5, 5, 4, 5, 5, 3, 4, 5, 3, 8, 5, 5, 5, 5, 5, 5, 4, 5, 6, 8, 6, 6,
        5, 4, 2, 4, 6,
    ];

    fn ascii_width(face: &FontFace, text: &[u8]) -> i16 {
        text.iter()
            .map(|byte| i16::from(face.glyphs[usize::from(*byte - b' ')].advance))
            .sum()
    }

    fn assert_same_glyph_except_advance(left: &Glyph, right: &Glyph) {
        assert_eq!(left.width, right.width);
        assert_eq!(left.height, right.height);
        assert_eq!(left.origin_x, right.origin_x);
        assert_eq!(left.origin_y, right.origin_y);
        assert_eq!(left.data_offset, right.data_offset);
    }

    fn closed(points: &[(f32, f32)]) -> Vec<zeno::Command> {
        let mut path = vec![zeno::Command::MoveTo(points[0].into())];
        path.extend(points[1..].iter().map(|&point| zeno::Command::LineTo(point.into())));
        path.push(zeno::Command::Close);
        path
    }

    fn rows(strike: &BinaryStrike) -> Vec<String> {
        strike
            .mask
            .chunks(strike.width)
            .map(|row| row.iter().map(|&p| if p != 0 { '#' } else { '.' }).collect())
            .collect()
    }

    #[test]
    fn binary_strikes_light_pixels_whose_centres_are_inside_the_outline() {
        // A 3 x 2 rectangle from (0.4, 0.4): centres at 0.5..2.5 by 0.5..1.5
        // are inside; the column at 3.5 and the row at 2.5 are not.
        let strike =
            scan_convert(&closed(&[(0.4, 0.4), (3.4, 0.4), (3.4, 2.4), (0.4, 2.4)])).unwrap();
        assert_eq!((strike.left, strike.top), (0, 3));
        assert_eq!(rows(&strike), ["....", "###.", "###."]);
        assert!(scan_convert(&[]).is_none());
    }

    #[test]
    fn binary_strikes_treat_a_centre_on_the_right_edge_as_outside() {
        // Ink from 0.5 to 2.5 holds the centres at 0.5 and 1.5; the centre
        // at 2.5 sits on the far edge and belongs to the next span over.
        let strike =
            scan_convert(&closed(&[(0.5, 0.0), (2.5, 0.0), (2.5, 1.0), (0.5, 1.0)])).unwrap();
        assert_eq!(rows(&strike), ["##."]);
    }

    #[test]
    fn binary_strikes_fill_where_contours_overlap() {
        // Two boxes wound the same way share the column from 2 to 3, as the
        // overlapping strokes of a glyph drawn in several contours do.
        // Non-zero winding keeps the shared part inked.
        let mut path = closed(&[(0.0, 0.0), (3.0, 0.0), (3.0, 2.0), (0.0, 2.0)]);
        path.extend(closed(&[(2.0, 0.0), (5.0, 0.0), (5.0, 2.0), (2.0, 2.0)]));
        let strike = scan_convert(&path).unwrap();
        assert_eq!(rows(&strike), ["#####", "#####"]);
    }

    #[test]
    fn binary_strikes_retain_strokes_that_pass_between_pixel_centres() {
        // A vertical stem 0.6 pixels wide covers no pixel centre, yet every
        // row keeps one pixel of it, in the column that holds the stem.
        let stem = scan_convert(&closed(&[(1.55, 0.0), (2.15, 0.0), (2.15, 4.0), (1.55, 4.0)])).unwrap();
        assert_eq!(rows(&stem), ["#.", "#.", "#.", "#."]);
        assert_eq!(stem.left, 1);
        // Transposed: a thin crossbar keeps one pixel in every column.
        let bar = scan_convert(&closed(&[(0.0, 2.55), (4.0, 2.55), (4.0, 3.15), (0.0, 3.15)])).unwrap();
        assert_eq!(rows(&bar), ["....", "####"]);
        assert_eq!(bar.top, 4);
    }

    #[test]
    fn binary_strikes_leave_out_the_stub_ends_of_hairlines() {
        // A stroke thinner than half a pixel keeps a pixel on each scanline
        // it runs through, but not on its first and last, where it ends
        // before reaching the next scanline: a serif tip must not grow.
        let hairline = scan_convert(&closed(&[(1.6, 0.0), (1.9, 0.0), (1.9, 4.0), (1.6, 4.0)])).unwrap();
        assert_eq!(rows(&hairline), [".", "#", "#", "."]);
    }

    #[test]
    fn binary_strikes_keep_the_thinner_of_two_strokes_in_one_run() {
        // Two strokes meet at an apex, like an "A": a thick one whose pixels
        // pass the centre test and, touching it, a hairline that passes
        // between centres. Thresholding area coverage lit only the thick one.
        let thick = closed(&[(2.0, 0.0), (3.2, 0.0), (3.2, 4.0), (2.0, 4.0)]);
        let hairline = closed(&[(1.6, 0.0), (1.9, 0.0), (1.9, 4.0), (1.6, 4.0)]);
        let strike = scan_convert(&[thick, hairline].concat()).unwrap();
        // Columns 1 to 3: the hairline (without its stub ends), the thick
        // stroke, and the column whose centre (3.5) the thick stroke does not
        // reach.
        assert_eq!(rows(&strike), [".#.", "##.", "##.", ".#."]);
    }

    #[test]
    fn binary_strikes_keep_a_stem_hinted_to_no_width() {
        // Both edges of the stem lie on x = 1.7: a span of zero width that
        // still marks where the stroke is.
        let strike =
            scan_convert(&closed(&[(1.7, 0.0), (1.7, 3.0), (1.7, 0.0), (3.0, 0.0), (3.0, -1.0), (1.7, -1.0)]))
                .unwrap();
        // The stem keeps column 1 below its stub end; the foot below it
        // covers only the centre of column 2.
        assert_eq!(rows(&strike), ["..", "#.", "#.", ".#"]);
    }

    #[test]
    fn binary_strikes_keep_counters_open() {
        // A ring: the inner contour runs the other way, so the middle is out.
        let mut path = closed(&[(0.0, 0.0), (5.0, 0.0), (5.0, 5.0), (0.0, 5.0)]);
        path.extend(closed(&[(1.2, 1.2), (1.2, 3.8), (3.8, 3.8), (3.8, 1.2)]));
        let strike = scan_convert(&path).unwrap();
        assert_eq!(rows(&strike), ["#####", "#...#", "#...#", "#...#", "#####"]);
    }

    #[test]
    fn geneva9_compatibility_component_has_exact_ofl_advance_bytes() {
        assert_eq!(*GENEVA9_ADVANCES, EXPECTED_GENEVA9_ADVANCES);
        assert_eq!(GENEVA9_ADVANCES.len(), usize::from(0x7eu8 - 0x20 + 1));
    }

    #[test]
    fn bundled_geneva9_uses_compatibility_advances() {
        let (face, _) = super::face(FONT_GENEVA, 9).expect("bundled Geneva 9");
        assert_eq!(
            face.glyphs
                .iter()
                .map(|glyph| glyph.advance)
                .collect::<Vec<_>>(),
            GENEVA9_ADVANCES.as_slice()
        );
    }

    #[test]
    fn bundled_application9_uses_geneva9_compatibility_advances() {
        let (face, _) = super::face(FONT_APPLICATION, 9).expect("bundled Application 9");
        assert_eq!(
            face.glyphs
                .iter()
                .map(|glyph| glyph.advance)
                .collect::<Vec<_>>(),
            GENEVA9_ADVANCES.as_slice()
        );
    }

    #[test]
    fn coppet9_keeps_guest_ink_and_retained_outline_sources() {
        for family in [FONT_GENEVA, FONT_APPLICATION] {
            let (face, _) = super::face(family, 9).unwrap();
            for (index, glyph) in face.glyphs.iter().enumerate().skip(1) {
                let count = usize::from(glyph.width) * usize::from(glyph.height);
                assert!(
                    face.data[glyph.data_offset..glyph.data_offset + count]
                        .iter()
                        .any(|&p| p == 255),
                    "character {} lost its guest ink",
                    index + 32
                );
                let high = presentation_glyph(glyph, face.data, 4)
                    .expect("Coppet retains its own outline source");
                assert!(high.pixels.iter().any(|&p| p > 0));
            }
            let i = &face.glyphs[usize::from(b'i' - b' ')];
            let w = usize::from(i.width);
            let pixels = &face.data[i.data_offset..i.data_offset + w * usize::from(i.height)];
            let rows = pixels.chunks_exact(w).collect::<Vec<_>>();
            assert!(rows[0].contains(&255), "i dot must survive");
            assert!(rows[1].iter().all(|&p| p == 0), "i dot stays separate");
            assert!(
                rows[2].iter().filter(|&&p| p == 255).count() >= 2,
                "i entry stroke must survive"
            );
        }
    }

    #[test]
    fn geneva9_apeiron_phrase_spans_match_recorded_origins() {
        const FIRST: &[u8] = b"This is your shooter.  There are many like it, ";
        const SECOND: &[u8] = b"but this one is yours.  Mind it well, cuz it is ";
        let (face, _) = super::face(FONT_GENEVA, 9).expect("bundled Geneva 9");

        assert_eq!((FIRST.len(), SECOND.len()), (47, 48));
        assert_eq!(ascii_width(face, FIRST), 207);
        assert_eq!(ascii_width(face, SECOND), 200);
        let bold = crate::quickdraw::text::QuickDrawTextStyle::from_bits(
            crate::quickdraw::text::QuickDrawTextStyle::BOLD_BIT,
        );
        assert_eq!(
            FIRST
                .iter()
                .map(|byte| bold
                    .glyph_advance(i32::from(face.glyphs[usize::from(*byte - b' ')].advance)))
                .sum::<i32>(),
            254
        );
        assert_eq!(
            SECOND
                .iter()
                .map(|byte| bold
                    .glyph_advance(i32::from(face.glyphs[usize::from(*byte - b' ')].advance)))
                .sum::<i32>(),
            248
        );
    }

    #[test]
    fn geneva9_coppet_preserves_extended_glyphs_and_font_metrics() {
        let bytes = super::bytes(FONT_GENEVA).expect("bundled Geneva bytes");
        let (raw, raw_extended) = super::rasterize(FONT_GENEVA, 9, bytes, None, None).unwrap();
        let (bundled, bundled_extended) = super::face(FONT_GENEVA, 9).unwrap();

        assert_ne!(raw.data, bundled.data, "ASCII artwork uses Coppet");
        assert_eq!(raw_extended.data, bundled_extended.data);
        assert_eq!(raw.metrics.ascent, bundled.metrics.ascent);
        assert_eq!(raw.metrics.descent, bundled.metrics.descent);
        assert_eq!(raw.metrics.leading, bundled.metrics.leading);
        for (raw_glyph, bundled_glyph) in raw_extended.glyphs.iter().zip(bundled_extended.glyphs) {
            assert_eq!(raw_glyph.mac_code, bundled_glyph.mac_code);
            assert_same_glyph_except_advance(&raw_glyph.glyph, &bundled_glyph.glyph);
            assert_eq!(raw_glyph.glyph.advance, bundled_glyph.glyph.advance);
        }
        let expected_wid_max = GENEVA9_ADVANCES
            .iter()
            .copied()
            .chain(raw_extended.glyphs.iter().map(|entry| entry.glyph.advance))
            .max()
            .unwrap();
        assert_eq!(bundled.metrics.wid_max, i16::from(expected_wid_max));
    }

    #[test]
    fn monaco9_retains_layout_and_extended_art_while_preserving_ascii_stems() {
        let (raw, raw_extended) = super::rasterize(
            FONT_MONACO,
            9,
            super::bytes(FONT_MONACO).unwrap(),
            None,
            super::super::compatibility::bundled_wid_max(FONT_MONACO, 9),
        )
        .unwrap();
        let (face, extended) = super::face(FONT_MONACO, 9).unwrap();
        assert_eq!(face.metrics.ascent, raw.metrics.ascent);
        assert_eq!(face.metrics.descent, raw.metrics.descent);
        assert_eq!(face.metrics.leading, raw.metrics.leading);
        assert_eq!(face.metrics.wid_max, raw.metrics.wid_max);
        for (new, old) in face.glyphs.iter().zip(raw.glyphs) {
            assert_eq!(new.advance, old.advance);
        }
        assert_eq!(extended.data, raw_extended.data);
        let one = &face.glyphs[usize::from(b'1' - b' ')];
        let ink = &face.data
            [one.data_offset..one.data_offset + usize::from(one.width) * usize::from(one.height)];
        assert!(one.height >= 7);
        assert!(
            ink.chunks(usize::from(one.width))
                .all(|row| row.contains(&255)),
            "The digit's vertical stem must survive between its top and foot"
        );
        let high = presentation_glyph(one, face.data, 4).unwrap();
        assert!(high.pixels.iter().any(|&p| p != 0));
    }

    #[test]
    fn monaco_compatibility_changes_only_the_selected_maximum_advance() {
        let bytes = super::bytes(FONT_MONACO).expect("bundled Monaco bytes");
        for size in [14, 23] {
            let (raw, raw_extended) =
                super::rasterize(FONT_MONACO, size, bytes, None, None).unwrap();
            let (bundled, bundled_extended) = super::face(FONT_MONACO, size).unwrap();

            assert_eq!(raw.data, bundled.data);
            assert_eq!(raw.glyphs.len(), bundled.glyphs.len());
            for (raw_glyph, bundled_glyph) in raw.glyphs.iter().zip(bundled.glyphs) {
                assert_same_glyph_except_advance(raw_glyph, bundled_glyph);
                assert_eq!(raw_glyph.advance, bundled_glyph.advance);
            }
            assert_eq!(raw_extended.data, bundled_extended.data);
            assert_eq!(raw_extended.glyphs.len(), bundled_extended.glyphs.len());
            for (raw_glyph, bundled_glyph) in
                raw_extended.glyphs.iter().zip(bundled_extended.glyphs)
            {
                assert_eq!(raw_glyph.mac_code, bundled_glyph.mac_code);
                assert_same_glyph_except_advance(&raw_glyph.glyph, &bundled_glyph.glyph);
                assert_eq!(raw_glyph.glyph.advance, bundled_glyph.glyph.advance);
            }
            assert_eq!(
                bundled.metrics.wid_max,
                super::super::compatibility::bundled_wid_max(FONT_MONACO, size)
                    .expect("classic scalable Monaco maximum advance")
            );
            assert_ne!(raw.metrics.wid_max, bundled.metrics.wid_max);
        }
    }

    #[test]
    fn compatibility_does_not_change_other_bundled_faces_or_guest_sfnt_rasterization() {
        for (font_id, size) in [
            (FONT_GENEVA, 8),
            (FONT_GENEVA, 10),
            (FONT_APPLICATION, 10),
            (FONT_HELVETICA, 9),
            (FONT_COURIER, 23),
        ] {
            let bytes = super::bytes(font_id).unwrap();
            let (raw, _) = super::rasterize(font_id, size, bytes, None, None).unwrap();
            let (bundled, _) = super::face(font_id, size).unwrap();
            assert_eq!(raw.data, bundled.data);
            assert_eq!(
                raw.glyphs
                    .iter()
                    .map(|glyph| glyph.advance)
                    .collect::<Vec<_>>(),
                bundled
                    .glyphs
                    .iter()
                    .map(|glyph| glyph.advance)
                    .collect::<Vec<_>>()
            );
            assert_eq!(raw.metrics.wid_max, bundled.metrics.wid_max);
        }

        let guest_bytes = include_bytes!("urw/NimbusMonoPS-Regular.ttf");
        let (guest, _) = super::rasterize(FONT_GENEVA, 9, guest_bytes, None, None).unwrap();
        assert!(
            guest
                .glyphs
                .windows(2)
                .all(|pair| pair[0].advance == pair[1].advance),
            "a guest sfnt registered as Geneva 9 must retain its monospaced metrics"
        );
    }

    #[test]
    fn menu_symbols_have_outline_coverage_at_both_resolutions() {
        for ch in ['\u{2318}', '\u{2713}', '\u{2122}'] {
            let (glyph, data) = unicode_glyph(FONT_CHICAGO, 12, ch).expect("bundled menu symbol");
            assert!(glyph.advance > 0 && data.iter().any(|p| *p == 255));
            let high = presentation_glyph(glyph, data, 4).expect("symbol outline source");
            assert!(high.pixels.iter().any(|p| *p > 0 && *p < 255));
        }
    }

    #[test]
    fn outlines_supply_exact_sizes_and_extended_mac_roman() {
        for family in [
            FONT_CHICAGO,
            FONT_GENEVA,
            FONT_MONACO,
            FONT_NEWYORK,
            FONT_PALATINO,
            FONT_VENICE,
            FONT_LONDON,
            FONT_CAIRO,
        ] {
            for size in [9, 12, 17, 24, 40, 96] {
                let (face, extended) = super::face(family, size).unwrap();
                assert_eq!(face.size, size);
                assert_eq!(face.glyphs.len(), 95);
                assert_eq!(extended.glyphs.len(), 128);
                assert!(face.data.iter().all(|&value| value == 0 || value == 255));
                for (glyph, data) in face.glyphs.iter().map(|g| (g, face.data)).chain(
                    extended
                        .glyphs
                        .iter()
                        .map(|entry| (&entry.glyph, extended.data)),
                ) {
                    assert!(
                        glyph.data_offset + usize::from(glyph.width) * usize::from(glyph.height)
                            <= data.len()
                    );
                }
                assert!(std::ptr::eq(face, super::face(family, size).unwrap().0));
            }
        }
        let (face, numerator, denominator) = get_font_face_scale_ratio(FONT_GENEVA, 17);
        assert_eq!((face.size, numerator, denominator), (17, 17, 17));
        let (accent, data) = get_macroman_glyph(FONT_GENEVA, 17, 0x8e).unwrap(); // é
        assert!(data[accent.data_offset
            ..accent.data_offset + usize::from(accent.width) * usize::from(accent.height)]
            .iter()
            .any(|&v| v > 0));
    }

    #[test]
    fn monochrome_small_text_keeps_stems_counters_and_baselines() {
        for family in [FONT_GENEVA, FONT_MONACO] {
            for size in 9..=12 {
                let (face, _) = super::face(family, size).unwrap();
                let h = &face.glyphs[(b'H' - b' ') as usize];
                let w = usize::from(h.width);
                let pixels = &face.data[h.data_offset..h.data_offset + w * usize::from(h.height)];
                let rows = pixels
                    .chunks_exact(w)
                    .filter(|row| row.iter().any(|&v| v != 0))
                    .collect::<Vec<_>>();
                let stems = (0..w)
                    .filter(|&x| rows.iter().all(|row| row[x] == 255))
                    .count();
                assert!(
                    stems >= 2,
                    "{family}/{size}: H must have two unbroken stems"
                );
                assert!(h.origin_y < 0);
                assert!(
                    i16::from(h.origin_y) + i16::from(h.height) <= 1,
                    "H must sit above the baseline"
                );

                let o = &face.glyphs[(b'o' - b' ') as usize];
                let w = usize::from(o.width);
                let h = usize::from(o.height);
                let pixels = &face.data[o.data_offset..o.data_offset + w * h];
                let mut outside = vec![false; pixels.len()];
                let mut queue = (0..pixels.len())
                    .filter(|&i| i < w || i >= w * (h - 1) || i % w == 0 || i % w == w - 1)
                    .collect::<Vec<_>>();
                while let Some(i) = queue.pop() {
                    if outside[i] || pixels[i] != 0 {
                        continue;
                    }
                    outside[i] = true;
                    if i >= w {
                        queue.push(i - w);
                    }
                    if i + w < pixels.len() {
                        queue.push(i + w);
                    }
                    if i % w != 0 {
                        queue.push(i - 1);
                    }
                    if i % w + 1 < w {
                        queue.push(i + 1);
                    }
                }
                assert!(
                    pixels
                        .iter()
                        .zip(outside)
                        .any(|(&ink, outside)| ink == 0 && !outside),
                    "{family}/{size}: o must retain its enclosed counter"
                );
            }
        }
    }

    #[test]
    fn monospaced_advances_and_em_scale_are_preserved() {
        let (face, _) = super::face(FONT_COURIER, 20).unwrap();
        assert!(face.glyphs.iter().all(|glyph| glyph.advance == 12)); // 600/1000 em
        assert!(super::face(FONT_GENEVA, 97).is_none());
        assert!(super::face(30000, 12).is_none());
    }

    #[test]
    fn guest_outline_wins_for_an_extended_character_drawn_first() {
        assert!(register_resource_outline_font(
            30002,
            include_bytes!("urw/NimbusMonoPS-Regular.ttf")
        ));
        let (accent, _) = get_macroman_glyph(30002, 23, 0x8e).unwrap();
        let face = get_font_face(30002, 23).unwrap();
        assert_eq!(accent.advance, face.glyphs[0].advance);
        assert!(face
            .glyphs
            .iter()
            .all(|glyph| glyph.advance == accent.advance));
        // Registration is process-global, so use a unique compatibility family
        // for this test instead of changing a family shared by other tests.
    }

    #[test]
    fn guest_bitmap_strike_wins_even_after_urw_is_cached() {
        super::face(FONT_COURIER, 19).unwrap();
        // Keep this family/size unique so parallel tests do not share a strike.
        assert!(register_resource_font_strike_for_family(
            FONT_COURIER,
            19,
            &super::super::tests::minimal_nfnt()
        ));
        let face = get_font_face(FONT_COURIER, 19).unwrap();
        assert_eq!(face.metrics.ascent, 1);
        assert_eq!(face.glyphs[0].advance, 1);
    }
}
