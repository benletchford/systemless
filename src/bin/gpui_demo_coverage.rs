//! Lossless row spans for already-composited guest text coverage.
//! Colours include the resolved background and indexed selection inversion.

use std::collections::BTreeMap;

#[derive(Debug, PartialEq)]
pub(super) struct CoverageSpan<Color = [u8; 3]> {
    pub left: i32,
    pub right: i64,
    pub y: i32,
    pub color: Color,
}

pub(super) fn row_spans<Color: Copy + PartialEq>(
    pixels: BTreeMap<(i32, i32), Color>,
) -> Vec<CoverageSpan<Color>> {
    let mut pixels: Vec<_> = pixels.into_iter().collect();
    pixels.sort_unstable_by_key(|((x, y), _)| (*y, *x));
    let mut spans: Vec<CoverageSpan<Color>> = Vec::new();
    for ((x, y), color) in pixels {
        if let Some(last) = spans.last_mut() {
            if last.y == y && last.right == i64::from(x) && last.color == color {
                last.right += 1;
                continue;
            }
        }
        spans.push(CoverageSpan { left: x, right: i64::from(x) + 1, y, color });
    }
    spans
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spans_preserve_holes_colours_negative_origins_and_rows() {
        let pixels = BTreeMap::from([
            ((-2, -1), [20, 40, 60]), ((-1, -1), [20, 40, 60]),
            ((0, -1), [21, 40, 60]), ((2, -1), [21, 40, 60]),
            ((-2, 0), [20, 40, 60]), ((-1, 0), [20, 40, 60]),
        ]);
        let spans = row_spans(pixels.clone());
        assert_eq!(spans.len(), 4);
        let restored: BTreeMap<_, _> = spans.iter().flat_map(|s|
            (i64::from(s.left)..s.right).map(move |x| ((x as i32, s.y), s.color))).collect();
        assert_eq!(restored, pixels);
    }

    #[test]
    fn solid_selection_rectangle_uses_one_span_per_row() {
        let pixels: BTreeMap<_, _> = (0..24).flat_map(|y|
            (0..800).map(move |x| ((x, y), [0; 3]))).collect();
        let spans = row_spans(pixels);
        assert_eq!(spans.len(), 24);
        assert!(spans.iter().all(|s| s.left == 0 && s.right == 800));
    }

    #[test]
    fn spans_preserve_device_pixels_across_scales_densities_and_origins() {
        let pixels: BTreeMap<_, _> = (-4..8).flat_map(|y| (-40..40)
            .filter(move |x| (x + y) % 11 != 0)
            .map(move |x| ((x, y), if x % 13 < 4 { [0, 0, 0] } else { [55, 127, 201] })))
            .collect();
        let spans = row_spans(pixels.clone());
        for scale in [0.5_f32, 0.75, 1., 1.25, 1.5, 2.] {
            for density in [1_f32, 1.5, 2., 3.] {
                let raster = (scale * density).ceil().max(1.);
                let unit = scale / raster;
                for origin in [-32.5_f32, 0., 17.25] {
                    let snap = |v: f32| ((v * density).abs() - 0.5)
                        .ceil().copysign(v * density) as i32;
                    let paint = |rects: Vec<(i32, i64, i32, [u8; 3])>| {
                        let mut device = BTreeMap::new();
                        for (left, right, y, color) in rects {
                            let left = snap(origin + left as f32 * unit);
                            let right = snap(origin + right as f32 * unit);
                            let top = snap(origin + y as f32 * unit);
                            let bottom = snap(origin + (i64::from(y) + 1) as f32 * unit);
                            for dy in top..bottom { for dx in left..right {
                                device.insert((dx, dy), color);
                            } }
                        }
                        device
                    };
                    let individual = paint(pixels.iter().map(|(&(x, y), &color)|
                        (x, i64::from(x) + 1, y, color)).collect());
                    let merged = paint(spans.iter().map(|s|
                        (s.left, s.right, s.y, s.color)).collect());
                    assert_eq!(merged, individual,
                        "scale={scale}, density={density}, origin={origin}");
                }
            }
        }
    }

    #[test]
    fn alpha_spans_preserve_fractional_coverage_and_holes() {
        let pixels: BTreeMap<_, _> = (1..=255_u8).flat_map(|alpha|
            (0..8).filter(|x| *x != 3).map(move |x| ((x, i32::from(alpha)), alpha)))
            .collect();
        let spans = row_spans(pixels.clone());
        assert_eq!(spans.len(), 510);
        let restored: BTreeMap<_, _> = spans.iter().flat_map(|s|
            (i64::from(s.left)..s.right).map(move |x| ((x as i32, s.y), s.color))).collect();
        assert_eq!(restored, pixels);
    }

    #[test]
    fn empty_and_extreme_coordinates_do_not_overflow() {
        assert!(row_spans::<[u8; 3]>(BTreeMap::new()).is_empty());
        let spans = row_spans(BTreeMap::from([((i32::MAX, i32::MIN), [255; 3])]));
        assert_eq!(spans[0].right, i64::from(i32::MAX) + 1);
    }
}
