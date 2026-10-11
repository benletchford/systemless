//! Systemless notification-alert geometry in guest coordinates.
//!
//! This explicit system-owned recipe has no application-owned bounds. It is
//! not independent evidence of a particular Macintosh System version's alert.
use super::{frames::Rect, text::ClassicLine};
use systemless::runner::NotificationSnapshot;

#[derive(Clone, Debug)]
pub(crate) struct AlertPlan {
    pub notice: NotificationSnapshot,
    pub bounds: Rect,
    pub message: Rect,
    pub button: Rect,
    pub lines: Vec<std::ops::Range<usize>>,
}

impl AlertPlan {
    pub fn build(notice: &NotificationSnapshot, viewport: Rect) -> Option<Self> {
        if notice.response_started || notice.mark != 0 || notice.icon_handle != 0
            || notice.sound_handle != 0 { return None; }
        let bytes = notice.text.as_ref()?;
        let width = viewport.width().checked_sub(32)?.min(400);
        if width < 160 { return None; }
        let text_width = width - 40;
        let advances = ClassicLine::plain(bytes, 0, 12).positions;
        let lines = systemless::quickdraw::text::wrap_classic_text(bytes, text_width as i16,
            |index, _| (advances[index + 1] - advances[index]) as i16)
            .into_iter().map(|line| line.start..line.visible_end).collect::<Vec<_>>();
        let text_height = i32::try_from(lines.len().max(1)).ok()?.checked_mul(16)?;
        let height = text_height.checked_add(76)?.max(112);
        if height > viewport.height().checked_sub(32)? { return None; }
        let left = viewport.left + (viewport.width() - width) / 2;
        let top = viewport.top + (viewport.height() - height) / 3;
        let bounds = Rect { top, left, bottom: top + height, right: left + width };
        let message = Rect { top: top + 20, left: left + 20,
            bottom: top + 20 + text_height, right: bounds.right - 20 };
        let button = Rect { top: bounds.bottom - 36, bottom: bounds.bottom - 16,
            left: bounds.right - 88, right: bounds.right - 20 };
        Some(Self { notice: notice.clone(), bounds, message, button, lines })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn notification_alert_preserves_raw_text_and_declines_unreadable_layouts() {
        let notice = NotificationSnapshot { guest_id: 1, instance_id: 2,
            response_started: false, mark: 0, icon_handle: 0, sound_handle: 0,
            text: Some(b"First\rCaf\x8e and original system font metrics".to_vec()),
            response: 0, ref_con: 0 };
        let viewport = Rect::from((0, 0, 342, 512));
        let plan = AlertPlan::build(&notice, viewport).unwrap();
        assert_eq!(plan.notice, notice);
        assert_eq!(&notice.text.as_ref().unwrap()[plan.lines[0].clone()], b"First");
        assert!(plan.message.bottom < plan.button.top);
        assert_eq!(plan.bounds.intersection(viewport), Some(plan.bounds));
        assert!(AlertPlan::build(&notice, Rect::from((0, 0, 64, 512))).is_none());
        let mut mixed = notice.clone(); mixed.sound_handle = u32::MAX;
        assert!(AlertPlan::build(&mixed, viewport).is_none());
        let mut completed = notice; completed.response_started = true;
        assert!(AlertPlan::build(&completed, viewport).is_none());
    }
}
