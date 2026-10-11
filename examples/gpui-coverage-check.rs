#[path = "../src/bin/gpui_demo_coverage.rs"]
mod coverage;

mod compositor {
    use crate::coverage;
    use gpui_kit::{prelude::*, *};
    use std::collections::BTreeMap;
    use std::sync::Arc;

    struct CoverageView {
        merged: bool,
        alpha: bool,
        scale: f32,
        origin: f32,
    }

    impl Render for CoverageView {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            let (merged, scale, origin, alpha) = (self.merged, self.scale, self.origin, self.alpha);
            canvas(|bounds, _, _| bounds, move |_, _, window, _| {
                let density = window.scale_factor();
                let raster = (scale * density).ceil().max(1.);
                let unit = scale / raster;
                let snap = |v: f32| px(((v * density).abs() - 0.5)
                    .ceil().copysign(v * density) / density);
                let pixels: BTreeMap<_, _> = (0..32).flat_map(|y| (0..160)
                    .filter(move |x| (x + y) % 17 != 0)
                    .map(move |x| ((x, y), if y < 12 && x < 100 { [0, 0, 0, 255] }
                        else { [(x % 16 * 16) as u8, (y % 16 * 16) as u8, 127,
                            if alpha { ((x + y) % 254 + 1) as u8 } else { 255 }] })))
                    .collect();
                let spans = if merged { coverage::row_spans(pixels) } else {
                    pixels.into_iter().map(|((x, y), color)| coverage::CoverageSpan {
                        left: x, right: i64::from(x) + 1, y, color,
                    }).collect()
                };
                let mut paths = BTreeMap::new();
                for span in spans {
                    let left = snap(origin + span.left as f32 * unit);
                    let right = snap(origin + span.right as f32 * unit);
                    let top = snap(origin + span.y as f32 * unit);
                    let bottom = snap(origin + (i64::from(span.y) + 1) as f32 * unit);
                    if right <= left || bottom <= top { continue; }
                    let path = paths.entry(span.color).or_insert_with(PathBuilder::fill);
                    path.move_to(point(left, top)); path.line_to(point(right, top));
                    path.line_to(point(right, bottom)); path.line_to(point(left, bottom)); path.close();
                }
                for ([r, g, b, a], path) in paths {
                    let mut ink: Hsla = rgb((u32::from(r) << 16) | (u32::from(g) << 8) | u32::from(b)).into();
                    ink.a = f32::from(a) / 255.;
                    window.paint_path(path.build().unwrap(), ink);
                }
            }).size_full()
        }
    }

    pub(super) fn run() {
        let mut app = HeadlessAppContext::with_platform(
            platform::current_platform(true).text_system(),
            Arc::new(gpui_kit::assets::Assets), platform::current_headless_renderer);
        app.update(gpui_kit::init);
        let output = std::env::args().nth(1).map(std::path::PathBuf::from);
        if let Some(path) = &output { std::fs::create_dir_all(path).unwrap(); }
        let mut opaque = BTreeMap::new();
        for alpha in [false, true] {
        for scale in [0.75, 1., 1.5, 2.] {
            for origin in [-3.25, 0., 17.25] {
                let mut screenshots = Vec::new();
                for merged in [false, true] {
                    let window = app.open_window(size(px(240.), px(100.)), |_, cx|
                        cx.new(|_| CoverageView { merged, scale, origin, alpha })).unwrap();
                    app.run_until_parked();
                    let screenshot = app.capture_screenshot(window.into()).unwrap();
                    assert!(screenshot.pixels().any(|p| p.0 == [0, 0, 0, 255]), "black coverage must be present");
                    assert!(screenshot.pixels().any(|p| p.0[0] != p.0[1] && p.0[3] == 255), "coloured coverage must be present");
                    if let Some(path) = &output {
                        screenshot.save(path.join(format!("alpha-{alpha}-scale-{scale}-origin-{origin}-merged-{merged}.png"))).unwrap();
                    }
                    screenshots.push(screenshot);
                    app.update_window(window.into(), |_, window, _| window.remove_window()).unwrap();
                }
                assert!(screenshots[0] == screenshots[1], "scale={scale}, origin={origin}, alpha={alpha}");
                let key = format!("{scale}/{origin}");
                if alpha {
                    assert!(screenshots[0] != opaque[&key], "fractional alpha must affect the image");
                } else { opaque.insert(key, screenshots.remove(0)); }
            }
        }
        }
    }
}

fn main() {
    compositor::run();
    println!("All 24 GPUI path comparisons match exactly");
}
