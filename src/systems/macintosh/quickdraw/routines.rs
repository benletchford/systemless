//! QuickDraw routines whose behaviour does not depend on the calling ISA.
//!
//! A routine is reached by a 68K A-line trap or a PowerPC InterfaceLib
//! import. Each dispatcher decodes the routine's arguments from its own ABI
//! and implements [`QuickDrawRoutineContext`] over its own state; the
//! routine's sequence (picture recording, the hidden pen, drawing) is written
//! once here, so the two cannot drift apart.

use crate::trap::types::Rect;

/// The verbs of QuickDraw's shape-drawing routines.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ShapeVerb {
    Frame,
    Paint,
    Erase,
    Invert,
    /// FillRect and friends paint this pattern.
    Fill([u8; 8]),
}

impl ShapeVerb {
    /// The picture opcode that records this verb applied to a rectangle:
    /// frameRect $0030 through fillRect $0034. Imaging With QuickDraw (1994),
    /// p. A-8.
    pub(crate) fn rect_opcode(self) -> u16 {
        match self {
            Self::Frame => 0x0030,
            Self::Paint => 0x0031,
            Self::Erase => 0x0032,
            Self::Invert => 0x0033,
            Self::Fill(_) => 0x0034,
        }
    }
}

/// What a QuickDraw routine needs from the dispatcher that called it, over the
/// current graphics port.
pub(crate) trait QuickDrawRoutineContext {
    /// The rectangle at `ptr` in guest memory, or `None` when it cannot be read.
    fn read_rect(&mut self, ptr: u32) -> Option<Rect>;
    /// Records `opcode` with `rect` when a picture is open in the current
    /// port, returning whether one was. A recorded call draws nothing.
    fn record_picture_rect(&mut self, opcode: u16, rect: Rect) -> bool;
    /// Whether the current port's pen is hidden (pnVis below 0), which
    /// suppresses drawing. Imaging With QuickDraw (1994), p. 3-41.
    fn pen_hidden(&mut self) -> bool;
    /// Fills `rect` with the current port's background pattern in patCopy
    /// mode, clipped as the port clips drawing.
    fn fill_rect_with_background(&mut self, rect: Rect);
}

/// EraseRect: fills `r` with the current port's background pattern, in patCopy
/// mode. Imaging With QuickDraw (1994), p. 3-61. It is not a framed shape, so
/// an open region does not record it (p. 3-87).
pub(crate) fn erase_rect<C: QuickDrawRoutineContext + ?Sized>(context: &mut C, rect_ptr: u32) {
    let Some(rect) = context.read_rect(rect_ptr) else {
        return;
    };
    if context.record_picture_rect(ShapeVerb::Erase.rect_opcode(), rect) {
        return;
    }
    if context.pen_hidden() {
        return;
    }
    context.fill_rect_with_background(rect);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Default)]
    struct Recorder {
        picture_open: bool,
        pen_hidden: bool,
        events: Vec<String>,
    }

    impl QuickDrawRoutineContext for Recorder {
        fn read_rect(&mut self, ptr: u32) -> Option<Rect> {
            (ptr != 0).then_some(Rect {
                top: 1,
                left: 2,
                bottom: 3,
                right: 4,
            })
        }
        fn record_picture_rect(&mut self, opcode: u16, rect: Rect) -> bool {
            if self.picture_open {
                self.events
                    .push(format!("record {opcode:#06x} {}", rect.top));
            }
            self.picture_open
        }
        fn pen_hidden(&mut self) -> bool {
            self.pen_hidden
        }
        fn fill_rect_with_background(&mut self, rect: Rect) {
            self.events.push(format!("erase {}", rect.left));
        }
    }

    #[test]
    fn erase_rect_records_into_an_open_picture_and_draws_only_with_a_visible_pen() {
        let mut open = Recorder {
            picture_open: true,
            ..Recorder::default()
        };
        erase_rect(&mut open, 8);
        assert_eq!(open.events, ["record 0x0032 1"]);

        let mut hidden = Recorder {
            pen_hidden: true,
            ..Recorder::default()
        };
        erase_rect(&mut hidden, 8);
        assert!(hidden.events.is_empty());

        let mut visible = Recorder::default();
        erase_rect(&mut visible, 8);
        assert_eq!(visible.events, ["erase 2"]);

        let mut unreadable = Recorder::default();
        erase_rect(&mut unreadable, 0);
        assert!(unreadable.events.is_empty());
    }
}
