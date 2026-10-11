//! Rectangular guest-frame overlays. Content pixels and input remain guest-owned.

use systemless::runner::WindowFrameSnapshot;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Rect {
    pub top: i32,
    pub left: i32,
    pub bottom: i32,
    pub right: i32,
}

impl From<(i16, i16, i16, i16)> for Rect {
    fn from(r: (i16, i16, i16, i16)) -> Self {
        Self {
            top: r.0.into(),
            left: r.1.into(),
            bottom: r.2.into(),
            right: r.3.into(),
        }
    }
}

impl Rect {
    pub fn width(self) -> i32 {
        self.right - self.left
    }
    pub fn height(self) -> i32 {
        self.bottom - self.top
    }
    fn valid(self) -> bool {
        self.width() > 0 && self.height() > 0
    }
    pub fn intersection(self, other: Self) -> Option<Self> {
        let r = Self {
            top: self.top.max(other.top),
            left: self.left.max(other.left),
            bottom: self.bottom.min(other.bottom),
            right: self.right.min(other.right),
        };
        r.valid().then_some(r)
    }
    fn subtract(self, cover: Self) -> Vec<Self> {
        let Some(cut) = self.intersection(cover) else {
            return vec![self];
        };
        [
            Self {
                bottom: cut.top,
                ..self
            },
            Self {
                top: cut.bottom,
                ..self
            },
            Self {
                top: cut.top,
                bottom: cut.bottom,
                right: cut.left,
                ..self
            },
            Self {
                top: cut.top,
                bottom: cut.bottom,
                left: cut.right,
                ..self
            },
        ]
        .into_iter()
        .filter(|r| r.valid())
        .collect()
    }
}

pub struct FramePiece {
    pub window: usize,
    /// Full strip geometry, retained so clipping never recentres a title.
    pub source: Rect,
    pub clip: Rect,
    pub title: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum GutterKind {
    Vertical,
    Horizontal,
    GrowBox,
}

pub struct GutterPiece {
    pub window: usize,
    pub source: Rect,
    pub clip: Rect,
    pub kind: GutterKind,
}

/// Restyle the content-edge areas reserved for standard document scrollbars
/// and DrawGrowIcon without painting over another window or a custom WDEF.
pub fn gutter_pieces(windows: &[WindowFrameSnapshot], viewport: Rect) -> Vec<GutterPiece> {
    let mut result = Vec::new();
    let mut covers = Vec::new();
    for (index, frame) in windows.iter().enumerate() {
        let window = &frame.window;
        if !window.visible {
            continue;
        }
        let Some(structure) = window.structure_bounds.map(Rect::from) else {
            continue;
        };
        let content = Rect::from(window.bounds);
        if matches!(frame.definition_id, Some(0 | 4 | 8 | 12))
            && content.width() > 30
            && content.height() > 30
            && structure.intersection(content) == Some(content)
        {
            let right = content.right - 15;
            let bottom = content.bottom - 15;
            for (kind, source) in [
                (
                    GutterKind::Vertical,
                    Rect {
                        left: right,
                        bottom,
                        ..content
                    },
                ),
                (
                    GutterKind::Horizontal,
                    Rect {
                        top: bottom,
                        right,
                        ..content
                    },
                ),
                (
                    GutterKind::GrowBox,
                    Rect {
                        top: bottom,
                        left: right,
                        ..content
                    },
                ),
            ] {
                let mut clips: Vec<_> = source.intersection(viewport).into_iter().collect();
                for cover in &covers {
                    clips = clips
                        .into_iter()
                        .flat_map(|clip| clip.subtract(*cover))
                        .collect();
                }
                result.extend(clips.into_iter().map(|clip| GutterPiece {
                    window: index,
                    source,
                    clip,
                    kind,
                }));
            }
        }
        covers.push(structure);
    }
    result
}

/// Preserve the guest's frame/content boundary and front-to-back ordering.
/// Standard window variants: Inside Macintosh I, I-275; structure/content
/// regions and FindWindow: I-276--I-288. Custom WDEFs retain their pixels.
pub fn frame_pieces(windows: &[WindowFrameSnapshot], viewport: Rect) -> Vec<FramePiece> {
    let mut result = Vec::new();
    let mut covers = Vec::new();
    for (index, frame) in windows.iter().enumerate() {
        let window = &frame.window;
        if !window.visible {
            continue;
        }
        let Some(structure) = window.structure_bounds.map(Rect::from) else {
            continue;
        };
        let content = Rect::from(window.bounds);
        let supported = matches!(
            frame.definition_id,
            Some(0 | 1 | 2 | 3 | 4 | 5 | 8 | 12 | 16)
        );
        if supported && structure.intersection(content) == Some(content) {
            let has_title = matches!(frame.definition_id, Some(0 | 4 | 5 | 8 | 12 | 16));
            for source in structure.subtract(content) {
                let title = has_title && source.bottom == content.top;
                let mut clips: Vec<_> = source.intersection(viewport).into_iter().collect();
                for cover in &covers {
                    clips = clips
                        .into_iter()
                        .flat_map(|clip| clip.subtract(*cover))
                        .collect();
                }
                result.extend(clips.into_iter().map(|clip| FramePiece {
                    window: index,
                    source,
                    clip,
                    title,
                }));
            }
        }
        // Bounding-box occlusion is conservative for custom, nonrectangular WDEFs:
        // leave the original guest pixels there rather than paint over them.
        covers.push(structure);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::{frame_pieces, gutter_pieces, GutterKind, Rect};
    use systemless::runner::{WindowFrameSnapshot, WindowSnapshot};

    fn window(
        bounds: (i16, i16, i16, i16),
        visible: bool,
        definition_id: i16,
    ) -> WindowFrameSnapshot {
        WindowFrameSnapshot {
            guest_id: 1,
            window: WindowSnapshot {
                title: "Test".into(),
                bounds,
                structure_bounds: Some((bounds.0 - 19, bounds.1 - 1, bounds.2 + 2, bounds.3 + 2)),
                visible_region: None,
                update_region: None,
                visible,
                active: true,
            },
            definition_id: Some(definition_id),
            close_box: true,
        }
    }

    #[test]
    fn clipped_chrome_never_paints_guest_content_or_front_windows() {
        let windows = [
            window((40, 60, 90, 140), true, 0),
            window((65, 10, 150, 170), true, 0),
        ];
        let viewport = Rect::from((20, 0, 160, 180));
        let pieces = frame_pieces(&windows, viewport);
        assert!(pieces.iter().any(|p| p.window == 1 && p.title));
        for piece in &pieces {
            assert_eq!(piece.clip.intersection(viewport), Some(piece.clip));
            assert!(piece
                .clip
                .intersection(windows[piece.window].window.bounds.into())
                .is_none());
            for front in &windows[..piece.window] {
                assert!(piece
                    .clip
                    .intersection(front.window.structure_bounds.unwrap().into())
                    .is_none());
            }
        }
        // Every visible standard frame pixel is painted exactly once.
        for y in 20..160 {
            for x in 0..180 {
                let point = Rect {
                    top: y,
                    left: x,
                    bottom: y + 1,
                    right: x + 1,
                };
                let expected = windows
                    .iter()
                    .find(|w| {
                        Rect::from(w.window.structure_bounds.unwrap())
                            .intersection(point)
                            .is_some()
                    })
                    .is_some_and(|w| Rect::from(w.window.bounds).intersection(point).is_none());
                assert_eq!(
                    pieces
                        .iter()
                        .filter(|p| p.clip.intersection(point).is_some())
                        .count(),
                    usize::from(expected)
                );
            }
        }
    }

    #[test]
    fn gutters_stay_on_standard_document_edges_and_below_front_windows() {
        let windows = [
            window((60, 60, 120, 120), true, 8),
            window((40, 40, 140, 140), true, 8),
            window((20, 20, 160, 160), true, 99),
        ];
        let pieces = gutter_pieces(&windows, Rect::from((0, 0, 180, 180)));
        assert!(pieces
            .iter()
            .any(|piece| { piece.window == 0 && piece.kind == GutterKind::GrowBox }));
        assert!(pieces.iter().all(|piece| piece.window != 2));
        let front = Rect::from(windows[0].window.structure_bounds.unwrap());
        assert!(pieces
            .iter()
            .filter(|piece| piece.window == 1)
            .all(|piece| piece.clip.intersection(front).is_none()));
    }

    #[test]
    fn hidden_and_custom_frames_keep_guest_presentation() {
        let windows = [
            window((30, 10, 90, 100), false, 0),
            window((60, 20, 120, 130), true, 128),
        ];
        assert!(frame_pieces(&windows, Rect::from((20, 0, 200, 200))).is_empty());
    }
}
