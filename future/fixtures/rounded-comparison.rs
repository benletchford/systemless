//! Standalone comparison harness; never linked into the replacement runtime.
//! Compile with rustc --edition=2021 future/fixtures/rounded-comparison.rs.
mod trap {
    pub mod types {
        pub struct Rect {
            pub top: i16,
            pub left: i16,
            pub bottom: i16,
            pub right: i16,
        }
    }
}
#[path = "../../src/systems/macintosh/quickdraw/raster/spans.rs"]
mod legacy;
#[derive(Debug)]
pub enum Error {
    Graphics(String),
}
pub type Result<T> = std::result::Result<T, Error>;
mod graphics {
    #[derive(Clone, Copy, Debug)]
    pub struct Rect {
        pub left: i32,
        pub top: i32,
        pub right: i32,
        pub bottom: i32,
    }
    impl Rect {
        pub fn new(left: i32, top: i32, right: i32, bottom: i32) -> Self {
            Self {
                left,
                top,
                right,
                bottom,
            }
        }
    }
}
#[path = "../src/rounded.rs"]
mod rounded;
fn main() {
    let mut count = 0;
    for w in 1..=40i16 {
        for h in 1..=40i16 {
            for ow in [-1, 0, 1, 2, 3, 7, 16, 40, 60] {
                for oh in [-1, 0, 1, 2, 3, 7, 16, 40, 60] {
                    let old = legacy::compute_rrect_spans(
                        &trap::types::Rect {
                            top: -7,
                            left: 13,
                            bottom: h - 7,
                            right: w + 13,
                        },
                        ow,
                        oh,
                    );
                    let new = rounded::coverage(
                        graphics::Rect::new(13, -7, i32::from(w) + 13, i32::from(h) - 7),
                        ow,
                        oh,
                        None,
                    )
                    .unwrap();
                    for (y, (l, r)) in old.into_iter().enumerate() {
                        let row = new
                            .iter()
                            .filter(|r| r.top <= y as i32 - 7 && r.bottom > y as i32 - 7)
                            .collect::<Vec<_>>();
                        if l < r {
                            assert_eq!(row.len(), 1);
                            assert_eq!((row[0].left, row[0].right), (i32::from(l), i32::from(r)));
                        } else {
                            assert!(row.is_empty());
                        }
                    }
                    count += 1;
                }
            }
        }
    }
    println!("{count} legacy geometry comparisons passed");
    let mut frames = 0;
    for w in 1..=16i16 {
        for h in 1..=16i16 {
            for (ow, oh) in [(0, 0), (3, 5), (6, 6), (20, 20)] {
                for pw in 0..=3i16 {
                    for ph in 0..=3i16 {
                        let outer = legacy::compute_rrect_spans(
                            &trap::types::Rect {
                                top: 0,
                                left: 0,
                                bottom: h,
                                right: w,
                            },
                            ow,
                            oh,
                        );
                        let inner = legacy::compute_rrect_spans(
                            &trap::types::Rect {
                                top: ph,
                                left: pw,
                                bottom: h - ph,
                                right: w - pw,
                            },
                            ow - 2 * pw,
                            oh - 2 * ph,
                        );
                        let new = rounded::coverage(
                            graphics::Rect::new(0, 0, w.into(), h.into()),
                            ow,
                            oh,
                            Some((pw, ph)),
                        )
                        .unwrap();
                        for y in 0..h {
                            for x in 0..w {
                                let (l, r) = outer[y as usize];
                                let inside_inner = y >= ph
                                    && inner
                                        .get((y - ph) as usize)
                                        .is_some_and(|(l, r)| x >= *l && x < *r);
                                let expected = x >= l && x < r && !inside_inner;
                                let count = new
                                    .iter()
                                    .filter(|r| {
                                        i32::from(x) >= r.left
                                            && i32::from(x) < r.right
                                            && i32::from(y) >= r.top
                                            && i32::from(y) < r.bottom
                                    })
                                    .count();
                                assert_eq!(
                                    count,
                                    usize::from(expected),
                                    "frame {w}x{h} corner {ow}x{oh} pen {pw}x{ph} pixel {x},{y}"
                                );
                            }
                        }
                        frames += 1;
                    }
                }
            }
        }
    }
    println!("{frames} legacy frame comparisons passed");
    for (w, h, ow, oh) in [(8, 8, 6, 6), (8, 8, 3, 5), (8, 4, 4, 4)] {
        println!(
            "{w} {h} {ow} {oh}: {:?}",
            legacy::compute_rrect_spans(
                &trap::types::Rect {
                    top: 0,
                    left: 0,
                    bottom: h,
                    right: w
                },
                ow,
                oh
            )
        );
    }
}
