//! Translate host wheel intent into ordinary guest scrollbar clicks.

use super::frames::{control_pieces, scrollbar_geometry, Rect};
use systemless::{
    menu_model::GuestMenuSnapshot,
    runner::{ControlSnapshot, WindowFrameSnapshot},
    systems::macintosh::session::{MacintoshInput, MacintoshSession},
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct ScrollTarget {
    pub id: u32,
    pub generation: u64,
    pub vertical: bool,
}

fn contains(rect: Rect, point: (i16, i16)) -> bool {
    i32::from(point.0) >= rect.top
        && i32::from(point.0) < rect.bottom
        && i32::from(point.1) >= rect.left
        && i32::from(point.1) < rect.right
}

/// Retained Standard File lists own their wheel input while their modal panel
/// is visible. Nested confirmation/name dialogs must not scroll the parent.
pub(crate) fn file_target(panel: &systemless::runner::StandardFileSnapshot,
    point: (i16, i16), vertical: bool) -> Option<ScrollTarget> {
    if !vertical || !panel.standard_entry_point || panel.confirming_replace || panel.new_folder.is_some() { return None; }
    let (list, scroll, visible) = match panel.kind {
        systemless::runner::StandardFileKind::Get => {
            let layout = panel.get_layout.as_ref()?; (layout.list, layout.scroll, layout.visible_rows)
        }
        systemless::runner::StandardFileKind::Put => {
            let layout = panel.put_layout.as_ref()?; (layout.list, layout.scroll, layout.visible_rows)
        }
    };
    if panel.entries.as_ref()?.len() <= visible
        || !(contains(Rect::from(list), point) || contains(Rect::from(scroll), point)) { return None; }
    Some(ScrollTarget { id: panel.guest_id, generation: panel.generation, vertical: true })
}

/// Only an unambiguous standard scrollbar in the active, visible window owns
/// a wheel gesture. Custom controls and overlapping windows retain ownership.
pub(crate) fn target(
    controls: &[ControlSnapshot],
    menus: &GuestMenuSnapshot,
    windows: &[WindowFrameSnapshot],
    viewport: Rect,
    point: (i16, i16),
    vertical: bool,
) -> Option<ScrollTarget> {
    let owner = windows.iter().find(|frame| {
        frame.window.visible
            && frame
                .window
                .structure_bounds
                .is_some_and(|bounds| contains(Rect::from(bounds), point))
    })?;
    if !owner.window.active
        || !contains(Rect::from(owner.window.bounds), point)
        || !owner
            .visible_content_rects
            .as_ref()
            .is_some_and(|rects| rects.iter().any(|r| contains(Rect::from(*r), point)))
    {
        return None;
    }
    let pieces = control_pieces(controls, menus, windows, viewport);
    let mut candidates = Vec::new();
    for piece in &pieces {
        let control = &controls[piece.control];
        if control.owner_id != owner.guest_id
            || control.proc_id != 16
            || !control.enabled
            || control.minimum >= control.maximum
            || scrollbar_geometry(control).vertical != vertical
        {
            continue;
        }
        let candidate = ScrollTarget {
            id: control.guest_id,
            generation: control.generation,
            vertical,
        };
        if contains(piece.clip, point) {
            return Some(candidate);
        }
        if !candidates.contains(&candidate) {
            candidates.push(candidate);
        }
    }
    // Do not redirect a gesture over another control into a document scrollbar.
    if controls.iter().any(|control| {
        control.owner_id == owner.guest_id
            && control.visible
            && contains(Rect::from(control.bounds), point)
    }) {
        return None;
    }
    (candidates.len() == 1).then(|| candidates[0])
}

#[derive(Default)]
pub(crate) struct WheelAccumulator {
    target: Option<ScrollTarget>,
    remainder: f32,
}

impl WheelAccumulator {
    pub(crate) fn reset(&mut self) {
        *self = Self::default();
    }

    pub(crate) fn push(&mut self, target: ScrollTarget, lines: f32) -> i8 {
        if !lines.is_finite() {
            self.reset();
            return 0;
        }
        if self.target != Some(target) || self.remainder.signum() != lines.signum() {
            self.remainder = 0.;
        }
        self.target = Some(target);
        self.remainder = (self.remainder + lines).clamp(-4., 4.);
        let steps = self.remainder.trunc() as i8;
        self.remainder -= f32::from(steps);
        steps
    }
}

#[derive(Clone, Copy)]
pub(crate) struct WheelRequest {
    pub target: ScrollTarget,
    pub origin: (i16, i16),
    pub steps: i8,
}

/// Re-resolve the target from live guest snapshots immediately before each
/// click, including lifetime, visible region and the exposed arrow hit point.
pub(crate) fn arrow(session: &mut MacintoshSession, request: WheelRequest) -> Option<(i16, i16)> {
    if request.steps == 0 || session.runner().guest_menu_tracking_active() {
        return None;
    }
    let controls = session.runner_mut().control_snapshot();
    let windows = session.runner_mut().window_frame_snapshot();
    let menus = session.runner_mut().guest_menu_snapshot();
    let mode = session.runner().dispatcher().screen_mode;
    let viewport = Rect {
        top: 0,
        left: 0,
        bottom: i32::from(mode.3),
        right: i32::from(mode.2),
    };
    if target(
        &controls,
        &menus,
        &windows,
        viewport,
        request.origin,
        request.target.vertical,
    ) != Some(request.target)
    {
        return None;
    }
    let control = controls
        .iter()
        .find(|c| c.guest_id == request.target.id && c.generation == request.target.generation)?;
    if request.steps > 0 && control.value >= control.maximum
        || request.steps < 0 && control.value <= control.minimum
    {
        return None;
    }
    let geometry = scrollbar_geometry(control);
    let (top, left, bottom, right) = control.bounds;
    if geometry.arrow_extent <= 0 {
        return None;
    }
    let half_arrow = geometry.arrow_extent / 2;
    let point = if geometry.vertical {
        (
            (if request.steps > 0 {
                i32::from(bottom) - half_arrow - 1
            } else {
                i32::from(top) + half_arrow
            }) as i16,
            ((i32::from(left) + i32::from(right)) / 2) as i16,
        )
    } else {
        (
            ((i32::from(top) + i32::from(bottom)) / 2) as i16,
            (if request.steps > 0 {
                i32::from(right) - half_arrow - 1
            } else {
                i32::from(left) + half_arrow
            }) as i16,
        )
    };
    if controls.iter().any(|other| {
        other.guest_id != control.guest_id
            && other.owner_id == control.owner_id
            && other.visible
            && contains(Rect::from(other.bounds), point)
    }) {
        return None;
    }
    control_pieces(&controls, &menus, &windows, viewport)
        .iter()
        .any(|piece| {
            controls[piece.control].guest_id == control.guest_id && contains(piece.clip, point)
        })
        .then_some(point)
}

/// A down and up each receive a guest execution slice before pointer restore.
/// Applications retain TrackControl and action-procedure ownership (Macintosh
/// Toolbox Essentials, pp. 5-57--5-59); this never changes a ControlRecord.
pub(crate) struct WheelClick {
    point: (i16, i16),
    request: WheelRequest,
    released: bool,
}

impl WheelClick {
    pub(crate) fn begin(session: &mut MacintoshSession, request: WheelRequest) -> Option<Self> {
        let point = arrow(session, request)?;
        session.deliver_input(MacintoshInput::MouseDown {
            vertical: point.0,
            horizontal: point.1,
        });
        Some(Self {
            point,
            request,
            released: false,
        })
    }

    pub(crate) fn stop_repeating(&mut self) {
        self.request.steps = self.request.steps.signum();
    }

    pub(crate) fn advance(mut self, session: &mut MacintoshSession) -> Option<Self> {
        if !self.released {
            session.deliver_input(MacintoshInput::MouseUp {
                vertical: self.point.0,
                horizontal: self.point.1,
            });
            self.released = true;
            return Some(self);
        }
        session.deliver_input(MacintoshInput::MouseMove {
            vertical: self.request.origin.0,
            horizontal: self.request.origin.1,
        });
        self.request.steps -= self.request.steps.signum();
        Self::begin(session, self.request)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fractional_wheel_input_resets_on_direction_target_and_focus_changes() {
        let target = ScrollTarget {
            id: 1,
            generation: 2,
            vertical: true,
        };
        let mut wheel = WheelAccumulator::default();
        assert_eq!(wheel.push(target, 0.75), 0);
        assert_eq!(wheel.push(target, 0.5), 1);
        assert_eq!(wheel.push(target, -0.75), 0);
        assert_eq!(wheel.push(target, -0.5), -1);
        assert_eq!(
            wheel.push(
                ScrollTarget {
                    generation: 3,
                    ..target
                },
                0.75
            ),
            0
        );
        wheel.reset();
        assert_eq!(wheel.push(target, 0.5), 0);
        assert_eq!(wheel.push(target, f32::NAN), 0);
        assert_eq!(wheel.push(target, 0.75), 0);
        assert_eq!(wheel.push(target, 1000.), 4);
    }
}
