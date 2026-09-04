//! Architecture-neutral Window Manager ordering operations.

/// The standard QuickDraw 50% gray desktop pattern.
pub(crate) const STANDARD_DESKTOP_PATTERN: [u8; 8] =
    [0xAA, 0x55, 0xAA, 0x55, 0xAA, 0x55, 0xAA, 0x55];

pub(crate) fn standard_desktop_pattern_is_ink(h: i32, v: i32) -> bool {
    let row = STANDARD_DESKTOP_PATTERN[v.rem_euclid(8) as usize];
    row & (0x80 >> h.rem_euclid(8)) != 0
}

pub(crate) type WindowRect = (i16, i16, i16, i16);

/// Architecture-neutral inspection of one live Window Manager record.
///
/// This is intentionally a semantic seam for deterministic fixtures and
/// diagnostics.  The WindowPtr is not exposed: callers should identify a
/// window by its title and assert the returned vector's front-to-back order,
/// geometry, activation, visibility, and pending update region.  Regions are
/// represented by their QuickDraw bounding boxes because the public fixture
/// contract only needs to know which screen area is dirty/visible; the guest
/// region records remain private implementation details.
#[doc(hidden)]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WindowSnapshot {
    pub title: String,
    pub bounds: WindowRect,
    pub structure_bounds: Option<WindowRect>,
    pub visible_region: Option<WindowRect>,
    pub update_region: Option<WindowRect>,
    pub visible: bool,
    pub active: bool,
}

pub(crate) fn standard_window_structure_bounds(content: WindowRect) -> WindowRect {
    (
        content.0.saturating_sub(19),
        content.1.saturating_sub(1),
        content.2.saturating_add(2),
        content.3.saturating_add(2),
    )
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct StandardWindowChrome {
    pub(crate) background: WindowRect,
    pub(crate) ink: Vec<WindowRect>,
    pub(crate) zoom_ink: Vec<WindowRect>,
    pub(crate) title_h: i16,
    pub(crate) title_baseline: i16,
    pub(crate) title_clip: WindowRect,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct StandardGrowIcon {
    pub(crate) background: WindowRect,
    pub(crate) ink: Vec<WindowRect>,
}

/// Build the standard document WDEF size-box presentation geometry.
///
/// `DrawGrowIcon` owns the 15-by-15 lower-right area of the content region.
/// It always draws the scroll-bar delimiters, erases the size box, and adds
/// the diagonal grow image only while the window is active.
/// Inside Macintosh: Macintosh Toolbox Essentials (1992), pp. 4-111--4-112.
pub(crate) fn standard_grow_icon(content: WindowRect, active: bool) -> StandardGrowIcon {
    let (_, left, bottom, right) = content;
    let separator_y = bottom.saturating_sub(15);
    let separator_x = right.saturating_sub(15);
    let mut ink = vec![
        (
            content.0,
            separator_x,
            bottom,
            separator_x.saturating_add(1),
        ),
        (
            separator_y,
            left.saturating_sub(1),
            separator_y.saturating_add(1),
            right.saturating_add(2),
        ),
    ];

    if active {
        // Three parallel diagonals form the classic lower-right size grip.
        // Express each pixel as a one-pixel rectangle so both CPU adapters
        // render exactly the same WDEF geometry.
        for length in [12i16, 8, 4] {
            for step in 0..length {
                let y = bottom.saturating_sub(2).saturating_sub(step);
                let x = right
                    .saturating_sub(2)
                    .saturating_sub(length.saturating_sub(1).saturating_sub(step));
                ink.push((y, x, y.saturating_add(1), x.saturating_add(1)));
            }
        }
    }

    StandardGrowIcon {
        background: (
            separator_y.saturating_add(1),
            separator_x.saturating_add(1),
            bottom,
            right,
        ),
        ink,
    }
}

/// Build the standard document/movable-dialog WDEF presentation geometry.
/// Rectangles use QuickDraw's exclusive bottom/right convention.
pub(crate) fn standard_window_chrome(
    content: WindowRect,
    menu_bar_height: i16,
    title_width: i16,
    title_ascent: i16,
    title_descent: i16,
    has_title: bool,
    active: bool,
    document_proc: bool,
    go_away: bool,
    zoom_box: bool,
) -> StandardWindowChrome {
    let (top, left, bottom, right) = content;
    let tb_top = top.saturating_sub(19).max(menu_bar_height);
    let tb_bottom = top.saturating_sub(1);
    let tb_left = left.saturating_sub(1);
    let tb_right = right.saturating_add(1);
    let title_height = title_ascent.saturating_add(title_descent);
    let title_interior_height = tb_bottom.saturating_sub(tb_top).saturating_sub(1);
    let title_baseline = tb_top
        .saturating_add(1)
        .saturating_add(title_interior_height.saturating_sub(title_height) / 2)
        .saturating_add(title_ascent);
    let title_h =
        tb_left.saturating_add(tb_right.saturating_sub(tb_left).saturating_sub(title_width) / 2);
    let (title_clear_left, title_clear_right) = if has_title {
        (
            title_h.saturating_sub(8),
            title_h.saturating_add(title_width).saturating_add(8),
        )
    } else {
        (tb_right, tb_right)
    };
    // The standard Window Manager frame comprises the title bar and the
    // window outline. Keep the title bar enclosed on all four sides.
    // Macintosh Toolbox Essentials (1992), Figure 4-2, pp. 4-5--4-6;
    // Macintosh Human Interface Guidelines (1992), Figures 5-2--5-4.
    let mut ink = vec![(tb_top, tb_left, tb_top.saturating_add(1), tb_right)];
    ink.extend([
        (tb_bottom, tb_left, tb_bottom.saturating_add(1), tb_right),
        (
            tb_top,
            tb_left,
            tb_bottom.saturating_add(1),
            tb_left.saturating_add(1),
        ),
        (
            tb_top,
            tb_right.saturating_sub(1),
            tb_bottom.saturating_add(1),
            tb_right,
        ),
    ]);

    let has_close_box = active && document_proc && go_away;
    if has_close_box {
        let close_top = top.saturating_sub(15);
        let close_left = left.saturating_add(8);
        ink.extend([
            (
                close_top,
                close_left,
                close_top.saturating_add(1),
                close_left.saturating_add(11),
            ),
            (
                close_top,
                close_left,
                close_top.saturating_add(11),
                close_left.saturating_add(1),
            ),
            (
                close_top.saturating_add(2),
                close_left.saturating_add(9),
                close_top.saturating_add(10),
                close_left.saturating_add(10),
            ),
            (
                close_top.saturating_add(9),
                close_left.saturating_add(2),
                close_top.saturating_add(10),
                close_left.saturating_add(10),
            ),
        ]);
    }

    let has_zoom_box = active && document_proc && zoom_box;
    let mut zoom_ink = Vec::new();
    if has_zoom_box {
        // The visible zoom control is an 11-by-11 outer box with the bottom
        // and right edges of its smaller state box inset by four pixels. It is
        // centered over the same rightmost 15-pixel control column as the
        // vertical scroll bar and grow box. Macintosh Human Interface
        // Guidelines (1992), Figure 5-38, p. 168; Macintosh Toolbox
        // Essentials (1992), Figure 4-2 and Listing 5-17.
        let box_top = top.saturating_sub(15);
        let box_left = right.saturating_sub(13);
        let box_bottom = box_top.saturating_add(11);
        let box_right = box_left.saturating_add(11);
        let small_bottom = box_top.saturating_add(7);
        let small_right = box_left.saturating_add(7);
        zoom_ink.extend([
            (box_top, box_left, box_top.saturating_add(1), box_right),
            (box_top, box_left, box_bottom, box_left.saturating_add(1)),
            (
                box_bottom.saturating_sub(1),
                box_left,
                box_bottom,
                box_right,
            ),
            (box_top, box_right.saturating_sub(1), box_bottom, box_right),
            (
                box_top,
                small_right.saturating_sub(1),
                small_bottom,
                small_right,
            ),
            (
                small_bottom.saturating_sub(1),
                box_left,
                small_bottom,
                small_right,
            ),
        ]);
        ink.extend(zoom_ink.iter().copied());
    }

    if active {
        let stripe_left = tb_left.saturating_add(2);
        let stripe_right = if has_zoom_box {
            right.saturating_sub(15)
        } else {
            tb_right.saturating_sub(2)
        };
        let stripe_text_left = title_clear_left.saturating_add(2);
        let stripe_text_right = title_clear_right.saturating_sub(2);
        let (close_gap_left, close_gap_right) = if has_close_box {
            (left.saturating_add(7), left.saturating_add(20))
        } else {
            (stripe_right, stripe_right)
        };
        for y in tb_top.saturating_add(2)..=tb_bottom.saturating_sub(3) {
            if (y - tb_top) % 2 != 0 {
                continue;
            }
            let first_end = if has_close_box {
                close_gap_left
            } else {
                stripe_text_left
            };
            if stripe_left < first_end {
                ink.push((y, stripe_left, y.saturating_add(1), first_end));
            }
            if has_close_box && close_gap_right < stripe_text_left {
                ink.push((y, close_gap_right, y.saturating_add(1), stripe_text_left));
            }
            if stripe_text_right < stripe_right {
                ink.push((y, stripe_text_right, y.saturating_add(1), stripe_right));
            }
        }
    }

    ink.extend([
        (top, left.saturating_sub(1), bottom, left),
        (top, right, bottom, right.saturating_add(1)),
        (
            bottom,
            left.saturating_sub(1),
            bottom.saturating_add(1),
            right.saturating_add(1),
        ),
        (
            tb_top,
            right.saturating_add(1),
            bottom.saturating_add(2),
            right.saturating_add(2),
        ),
        (
            bottom.saturating_add(1),
            left,
            bottom.saturating_add(2),
            right.saturating_add(2),
        ),
    ]);

    StandardWindowChrome {
        background: (tb_top, tb_left, tb_bottom.saturating_add(1), tb_right),
        ink,
        zoom_ink,
        title_h,
        title_baseline,
        title_clip: (tb_top, tb_left, tb_bottom.saturating_sub(2), tb_right),
    }
}

/// Return the eligible windows in front of `target`, frontmost first.
///
/// CPU adapters supply live visibility and special-window filtering while the
/// shared Window Manager owns the z-order rule. The caller subtracts each
/// returned structure region from the target's content region. Inside
/// Macintosh Volume I (1985), p. I-297.
pub(crate) fn window_occluders<Window>(
    front_to_back: impl IntoIterator<Item = Window>,
    target: Window,
    mut eligible: impl FnMut(Window) -> bool,
) -> Vec<Window>
where
    Window: Copy + Eq,
{
    front_to_back
        .into_iter()
        .take_while(|window| *window != target)
        .filter(|window| eligible(*window))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn occluders_are_only_eligible_windows_in_front_of_the_target() {
        assert_eq!(
            window_occluders([4u32, 3, 2, 1], 1, |window| window != 3),
            [4, 2]
        );
    }

    #[test]
    fn unknown_target_uses_all_eligible_windows() {
        assert_eq!(window_occluders([3u32, 2, 1], 9, |_| true), [3, 2, 1]);
    }

    #[test]
    fn standard_desktop_pattern_alternates_in_both_axes() {
        assert!(standard_desktop_pattern_is_ink(0, 0));
        assert!(!standard_desktop_pattern_is_ink(1, 0));
        assert!(!standard_desktop_pattern_is_ink(0, 1));
        assert!(standard_desktop_pattern_is_ink(1, 1));
    }

    #[test]
    fn standard_document_chrome_includes_all_seven_pinstripes_and_shadow() {
        let chrome = standard_window_chrome(
            (49, 40, 420, 600),
            20,
            120,
            12,
            3,
            true,
            true,
            true,
            true,
            true,
        );

        let stripe_rows = chrome
            .ink
            .iter()
            .filter(|(top, left, bottom, _)| {
                *left == 41 && *bottom == top.saturating_add(1) && (32..=44).contains(top)
            })
            .map(|(top, _, _, _)| *top)
            .collect::<Vec<_>>();
        assert_eq!(stripe_rows, [32, 34, 36, 38, 40, 42, 44]);
        assert!(chrome.ink.contains(&(30, 39, 31, 601)));
        assert!(chrome
            .ink
            .iter()
            .any(|&(top, _, bottom, right)| top == 34 && bottom == 35 && right == 585));
        assert!(chrome.ink.contains(&(30, 601, 422, 602)));
        assert!(chrome.ink.contains(&(421, 40, 422, 602)));
        assert_eq!(
            standard_window_structure_bounds((49, 40, 420, 600)),
            (30, 39, 422, 602)
        );
        assert_eq!(chrome.title_baseline, 44);
        assert_eq!(
            chrome.zoom_ink,
            [
                (34, 587, 35, 598),
                (34, 587, 45, 588),
                (44, 587, 45, 598),
                (34, 597, 45, 598),
                (34, 593, 41, 594),
                (40, 587, 41, 594),
            ],
            "the zoom control should nest its smaller state box inside an 11-pixel frame"
        );
    }

    #[test]
    fn standard_grow_icon_erases_the_size_box_and_only_grips_when_active() {
        let inactive = standard_grow_icon((155, 180, 400, 500), false);
        assert_eq!(inactive.background, (386, 486, 400, 500));
        assert_eq!(inactive.ink.len(), 2);

        let active = standard_grow_icon((155, 180, 400, 500), true);
        assert_eq!(active.background, inactive.background);
        assert!(active.ink.len() > inactive.ink.len());
        assert!(active.ink.contains(&(398, 487, 399, 488)));
    }
}
