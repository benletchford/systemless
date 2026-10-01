//! Architecture-neutral Window Manager ordering operations.

/// The standard QuickDraw 50% gray desktop pattern.
pub(crate) const STANDARD_DESKTOP_PATTERN: [u8; 8] =
    [0xAA, 0x55, 0xAA, 0x55, 0xAA, 0x55, 0xAA, 0x55];

pub(crate) fn standard_desktop_pattern_is_ink(h: i32, v: i32) -> bool {
    let row = STANDARD_DESKTOP_PATTERN[v.rem_euclid(8) as usize];
    row & (0x80 >> h.rem_euclid(8)) != 0
}

pub(crate) type WindowRect = (i16, i16, i16, i16);

/// GrowWindow follows displacement from the mouse-down point, retaining
/// the pointer's offset inside the size box. Toolbox Essentials (1992),
/// pp. 4-99--4-100; confirmed on both Mac OS 8.1 CPU slices.
pub(crate) fn grow_dimensions_from_drag(
    content: WindowRect,
    limits: WindowRect,
    start: (i16, i16),
    mouse: (i16, i16),
) -> (i16, i16) {
    let dimension = |origin: i16, end: i16, down: i16, now: i16, min: i16, max: i16| {
        let min = i32::from(min.max(1));
        let max = i32::from(max).max(min);
        (i32::from(end) - i32::from(origin) + i32::from(now) - i32::from(down)).clamp(min, max)
            as i16
    };
    (
        dimension(content.0, content.2, start.0, mouse.0, limits.0, limits.2),
        dimension(content.1, content.3, start.1, mouse.1, limits.1, limits.3),
    )
}

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
#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct WindowSnapshot {
    pub title: String,
    pub bounds: WindowRect,
    pub structure_bounds: Option<WindowRect>,
    pub visible_region: Option<WindowRect>,
    pub update_region: Option<WindowRect>,
    pub visible: bool,
    pub active: bool,
}

const WINDOW_VISIBLE_OFFSET: u32 = 110;
const WINDOW_HILITED_OFFSET: u32 = 111;
const WINDOW_STRUCTURE_RGN_OFFSET: u32 = 114;
const WINDOW_UPDATE_RGN_OFFSET: u32 = 122;
const WINDOW_TITLE_HANDLE_OFFSET: u32 = 134;

fn snapshot_read_word(read_byte: &mut impl FnMut(u32) -> u8, address: u32) -> u16 {
    u16::from_be_bytes([read_byte(address), read_byte(address.wrapping_add(1))])
}

fn snapshot_read_long(read_byte: &mut impl FnMut(u32) -> u8, address: u32) -> u32 {
    u32::from_be_bytes([
        read_byte(address),
        read_byte(address.wrapping_add(1)),
        read_byte(address.wrapping_add(2)),
        read_byte(address.wrapping_add(3)),
    ])
}

fn snapshot_read_rect(read_byte: &mut impl FnMut(u32) -> u8, address: u32) -> WindowRect {
    (
        snapshot_read_word(read_byte, address) as i16,
        snapshot_read_word(read_byte, address.wrapping_add(2)) as i16,
        snapshot_read_word(read_byte, address.wrapping_add(4)) as i16,
        snapshot_read_word(read_byte, address.wrapping_add(6)) as i16,
    )
}

fn snapshot_region_bounds(
    read_byte: &mut impl FnMut(u32) -> u8,
    handle: u32,
) -> Option<WindowRect> {
    if handle == 0 {
        return None;
    }
    let region = snapshot_read_long(read_byte, handle);
    if region == 0 {
        return None;
    }
    let bounds = snapshot_read_rect(read_byte, region.wrapping_add(2));
    (bounds.2 > bounds.0 && bounds.3 > bounds.1).then_some(bounds)
}

fn snapshot_port_bounds_origin(read_byte: &mut impl FnMut(u32) -> u8, window: u32) -> (i16, i16) {
    let port_version = snapshot_read_word(read_byte, window.wrapping_add(6));
    if port_version & 0xC000 == 0 {
        return (
            snapshot_read_word(read_byte, window.wrapping_add(8)) as i16,
            snapshot_read_word(read_byte, window.wrapping_add(10)) as i16,
        );
    }

    let pixmap_handle = snapshot_read_long(read_byte, window.wrapping_add(2));
    if pixmap_handle == 0 {
        return (0, 0);
    }
    let pixmap = snapshot_read_long(read_byte, pixmap_handle);
    if pixmap == 0 {
        return (0, 0);
    }
    (
        snapshot_read_word(read_byte, pixmap.wrapping_add(6)) as i16,
        snapshot_read_word(read_byte, pixmap.wrapping_add(8)) as i16,
    )
}

fn snapshot_local_rect_to_global(rect: WindowRect, origin: (i16, i16)) -> WindowRect {
    (
        rect.0.wrapping_sub(origin.0),
        rect.1.wrapping_sub(origin.1),
        rect.2.wrapping_sub(origin.0),
        rect.3.wrapping_sub(origin.1),
    )
}

fn snapshot_title(read_byte: &mut impl FnMut(u32) -> u8, handle: u32) -> String {
    if handle == 0 {
        return String::new();
    }
    let title = snapshot_read_long(read_byte, handle);
    if title == 0 {
        return String::new();
    }
    let length = usize::from(read_byte(title));
    let bytes = (0..length)
        .map(|index| read_byte(title.wrapping_add(1).wrapping_add(index as u32)))
        .collect::<Vec<_>>();
    crate::mac_roman::decode_mac_roman(&bytes)
}

/// Project the process Window Manager list into an owned diagnostic snapshot.
///
/// The byte reader is total: missing bytes contribute zero, and guest-address
/// field arithmetic wraps at 32 bits. Window Manager Boolean fields are true
/// when nonzero. `GhostWindow` is excluded only from the list-derived front
/// candidate; a live nonzero `hilited` field remains independently visible.
/// Inside Macintosh Volume I (1985), pp. I-276--I-287; Macintosh Toolbox
/// Essentials (1992), pp. 4-63--4-65.
pub(crate) fn snapshot_window_stack(
    order: &[u32],
    mut read_byte: impl FnMut(u32) -> u8,
) -> Vec<WindowSnapshot> {
    struct DecodedWindow {
        pointer: u32,
        hilited: bool,
        snapshot: WindowSnapshot,
    }

    let ghost_window =
        snapshot_read_long(&mut read_byte, crate::memory::globals::addr::GHOST_WINDOW);
    let mut windows = order
        .iter()
        .copied()
        .filter(|window| *window != 0)
        .map(|window| {
            let visible = read_byte(window.wrapping_add(WINDOW_VISIBLE_OFFSET)) != 0;
            let hilited = read_byte(window.wrapping_add(WINDOW_HILITED_OFFSET)) != 0;
            let origin = snapshot_port_bounds_origin(&mut read_byte, window);
            let bounds = snapshot_local_rect_to_global(
                snapshot_read_rect(&mut read_byte, window.wrapping_add(16)),
                origin,
            );
            let structure_handle = snapshot_read_long(
                &mut read_byte,
                window.wrapping_add(WINDOW_STRUCTURE_RGN_OFFSET),
            );
            let structure_bounds = snapshot_region_bounds(&mut read_byte, structure_handle);
            let visible_handle = snapshot_read_long(&mut read_byte, window.wrapping_add(24));
            let visible_region = snapshot_region_bounds(&mut read_byte, visible_handle)
                .map(|rect| snapshot_local_rect_to_global(rect, origin));
            let update_handle = snapshot_read_long(
                &mut read_byte,
                window.wrapping_add(WINDOW_UPDATE_RGN_OFFSET),
            );
            let update_region = snapshot_region_bounds(&mut read_byte, update_handle);
            let title_handle = snapshot_read_long(
                &mut read_byte,
                window.wrapping_add(WINDOW_TITLE_HANDLE_OFFSET),
            );
            let title = snapshot_title(&mut read_byte, title_handle);
            DecodedWindow {
                pointer: window,
                hilited,
                snapshot: WindowSnapshot {
                    title,
                    bounds,
                    structure_bounds,
                    visible_region,
                    update_region,
                    visible,
                    active: false,
                },
            }
        })
        .collect::<Vec<_>>();
    let front = windows
        .iter()
        .find(|window| window.pointer != ghost_window && window.snapshot.visible)
        .map(|window| window.pointer);
    for window in &mut windows {
        window.snapshot.active = front == Some(window.pointer) || window.hilited;
    }
    windows.into_iter().map(|window| window.snapshot).collect()
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
    pub(crate) stripe_ink: Vec<WindowRect>,
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

    let mut stripe_ink = Vec::new();
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
                stripe_ink.push((y, stripe_left, y.saturating_add(1), first_end));
            }
            if has_close_box && close_gap_right < stripe_text_left {
                stripe_ink.push((y, close_gap_right, y.saturating_add(1), stripe_text_left));
            }
            if stripe_text_right < stripe_right {
                stripe_ink.push((y, stripe_text_right, y.saturating_add(1), stripe_right));
            }
        }
    }
    ink.extend(stripe_ink.iter().copied());

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
        stripe_ink,
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

/// Canonical Carbon Window Manager WindowPositionMethod values.
/// Universal Interfaces <MacWindows.h>.
pub const WINDOW_CENTER_ON_MAIN_SCREEN: u16 = 1;
pub const WINDOW_ALERT_POSITION_ON_MAIN_SCREEN: u16 = 2;
pub const WINDOW_STAGGER_ON_MAIN_SCREEN: u16 = 3;
pub const WINDOW_CENTER_ON_PARENT_WINDOW: u16 = 4;
pub const WINDOW_ALERT_POSITION_ON_PARENT_WINDOW: u16 = 5;
pub const WINDOW_STAGGER_ON_PARENT_WINDOW: u16 = 6;
pub const WINDOW_CENTER_ON_PARENT_WINDOW_SCREEN: u16 = 7;
pub const WINDOW_ALERT_POSITION_ON_PARENT_WINDOW_SCREEN: u16 = 8;
pub const WINDOW_STAGGER_ON_PARENT_WINDOW_SCREEN: u16 = 9;

/// Standard staggering offset in pixels for staggered window positioning.
pub const WINDOW_STAGGER_OFFSET: i32 = 20;

/// Architecture-neutral repositioning bounds calculation for RepositionWindow.
///
/// Evaluates the target window content bounds for all 9 canonical WindowPositionMethod
/// methods defined in Apple's Carbon Window Manager specification.
pub(crate) fn evaluate_reposition_window_bounds(
    content_bounds: WindowRect,
    structure_bounds: WindowRect,
    parent_structure_bounds: Option<WindowRect>,
    method: u16,
    screen_width: i32,
    screen_height: i32,
    menu_height: i32,
) -> Option<WindowRect> {
    if !(1..=9).contains(&method) {
        return None;
    }

    let structure_width = i32::from(structure_bounds.3) - i32::from(structure_bounds.1);
    let structure_height = i32::from(structure_bounds.2) - i32::from(structure_bounds.0);
    let content_dx = i32::from(content_bounds.1) - i32::from(structure_bounds.1);
    let content_dy = i32::from(content_bounds.0) - i32::from(structure_bounds.0);
    let content_width = i32::from(content_bounds.3) - i32::from(content_bounds.1);
    let content_height = i32::from(content_bounds.2) - i32::from(content_bounds.0);

    // Usable main screen rectangle (excluding the menu bar).
    let screen_target = (
        menu_height,
        0,
        screen_height,
        screen_width,
    );

    let (target_top, target_left, target_bottom, target_right) = match method {
        WINDOW_CENTER_ON_MAIN_SCREEN
        | WINDOW_ALERT_POSITION_ON_MAIN_SCREEN
        | WINDOW_STAGGER_ON_MAIN_SCREEN
        | WINDOW_CENTER_ON_PARENT_WINDOW_SCREEN
        | WINDOW_ALERT_POSITION_ON_PARENT_WINDOW_SCREEN
        | WINDOW_STAGGER_ON_PARENT_WINDOW_SCREEN => screen_target,
        WINDOW_CENTER_ON_PARENT_WINDOW
        | WINDOW_ALERT_POSITION_ON_PARENT_WINDOW
        | WINDOW_STAGGER_ON_PARENT_WINDOW => {
            if let Some(parent) = parent_structure_bounds {
                (
                    i32::from(parent.0),
                    i32::from(parent.1),
                    i32::from(parent.2),
                    i32::from(parent.3),
                )
            } else {
                screen_target
            }
        }
        _ => return None,
    };

    let target_width = target_right - target_left;
    let target_height = target_bottom - target_top;

    let (mut new_struct_left, mut new_struct_top) = match method {
        WINDOW_CENTER_ON_MAIN_SCREEN
        | WINDOW_CENTER_ON_PARENT_WINDOW
        | WINDOW_CENTER_ON_PARENT_WINDOW_SCREEN => {
            let left = target_left + (target_width - structure_width) / 2;
            let top = target_top + (target_height - structure_height) / 2;
            (left, top)
        }
        WINDOW_ALERT_POSITION_ON_MAIN_SCREEN
        | WINDOW_ALERT_POSITION_ON_PARENT_WINDOW
        | WINDOW_ALERT_POSITION_ON_PARENT_WINDOW_SCREEN => {
            let left = target_left + (target_width - structure_width) / 2;
            let top = target_top + (target_height - structure_height) / 5;
            (left, top)
        }
        WINDOW_STAGGER_ON_MAIN_SCREEN
        | WINDOW_STAGGER_ON_PARENT_WINDOW
        | WINDOW_STAGGER_ON_PARENT_WINDOW_SCREEN => {
            if let Some(parent) = parent_structure_bounds {
                let left = i32::from(parent.1) + WINDOW_STAGGER_OFFSET;
                let top = i32::from(parent.0) + WINDOW_STAGGER_OFFSET;
                (left, top)
            } else {
                let left = target_left + (target_width - structure_width) / 2;
                let top = target_top + (target_height - structure_height) / 2;
                (left, top)
            }
        }
        _ => return None,
    };

    if matches!(
        method,
        WINDOW_STAGGER_ON_MAIN_SCREEN
            | WINDOW_STAGGER_ON_PARENT_WINDOW
            | WINDOW_STAGGER_ON_PARENT_WINDOW_SCREEN
    ) {
        if new_struct_left + structure_width > screen_width {
            new_struct_left = (screen_width - structure_width).max(0);
        }
        if new_struct_top + structure_height > screen_height {
            new_struct_top = (screen_height - structure_height).max(menu_height);
        }
        if new_struct_left < 0 {
            new_struct_left = 0;
        }
        if new_struct_top < menu_height {
            new_struct_top = menu_height;
        }
    }

    let new_content_left = (new_struct_left + content_dx).clamp(i32::from(i16::MIN), i32::from(i16::MAX)) as i16;
    let new_content_top = (new_struct_top + content_dy).clamp(i32::from(i16::MIN), i32::from(i16::MAX)) as i16;
    let new_content_bottom = (i32::from(new_content_top) + content_height).clamp(i32::from(i16::MIN), i32::from(i16::MAX)) as i16;
    let new_content_right = (i32::from(new_content_left) + content_width).clamp(i32::from(i16::MIN), i32::from(i16::MAX)) as i16;

    Some((
        new_content_top,
        new_content_left,
        new_content_bottom,
        new_content_right,
    ))
}

/// Architecture-neutral evaluation of CollapseWindow.
pub(crate) fn evaluate_collapse_window(valid_window: bool, collapse: bool) -> i16 {
    if !valid_window {
        -50 // PPC_PARAM_ERR
    } else if !collapse {
        0 // PPC_NO_ERR
    } else {
        -4 // unimpErr
    }
}

/// Architecture-neutral evaluation of IsWindowCollapsed.
pub(crate) fn evaluate_is_window_collapsed(_valid_window: bool) -> bool {
    false
}

/// Canonical offsets into Mac OS WindowRecord structure.
#[allow(dead_code)]
pub const WINDOW_KIND_OFFSET: u32 = 108;
#[allow(dead_code)]
pub const WINDOW_VISIBLE_FLAG_OFFSET: u32 = 110;
#[allow(dead_code)]
pub const WINDOW_HILITED_FLAG_OFFSET: u32 = 111;
#[allow(dead_code)]
pub const WINDOW_GO_AWAY_FLAG_OFFSET: u32 = 112;
#[allow(dead_code)]
pub const WINDOW_SPARE_FLAG_OFFSET: u32 = 113;
#[allow(dead_code)]
pub const WINDOW_DEF_PROC_HANDLE_OFFSET: u32 = 126;
#[allow(dead_code)]
pub const WINDOW_STATE_DATA_HANDLE_OFFSET: u32 = 130;

/// Architecture-neutral parameter validation for GetWindowUserState.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GetWindowUserStateParameters {
    window_ptr: u32,
    out_rect_ptr: u32,
}

impl GetWindowUserStateParameters {
    pub const fn window_ptr(&self) -> u32 {
        self.window_ptr
    }

    pub const fn out_rect_ptr(&self) -> u32 {
        self.out_rect_ptr
    }
}

pub fn evaluate_get_window_user_state_parameters(
    window_ptr: u32,
    out_rect_ptr: u32,
    can_write: bool,
) -> Result<GetWindowUserStateParameters, i16> {
    if window_ptr == 0 || out_rect_ptr == 0 || !can_write {
        return Err(-50); // PPC_PARAM_ERR
    }
    Ok(GetWindowUserStateParameters {
        window_ptr,
        out_rect_ptr,
    })
}

/// Architecture-neutral parameter validation for SetWindowUserState.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SetWindowUserStateParameters {
    window_ptr: u32,
    in_rect_ptr: u32,
}

impl SetWindowUserStateParameters {
    pub const fn window_ptr(&self) -> u32 {
        self.window_ptr
    }

    pub const fn in_rect_ptr(&self) -> u32 {
        self.in_rect_ptr
    }
}

pub fn evaluate_set_window_user_state_parameters(
    window_ptr: u32,
    in_rect_ptr: u32,
    can_read: bool,
) -> Result<SetWindowUserStateParameters, i16> {
    if window_ptr == 0 || in_rect_ptr == 0 || !can_read {
        return Err(-50); // PPC_PARAM_ERR
    }
    Ok(SetWindowUserStateParameters {
        window_ptr,
        in_rect_ptr,
    })
}

/// Architecture-neutral evaluation of GetWindowGoAwayFlag.
pub fn evaluate_get_window_go_away_flag(window_ptr: u32, flag_byte: Option<u8>) -> bool {
    if window_ptr == 0 {
        return false;
    }
    flag_byte.unwrap_or(0) != 0
}

/// Architecture-neutral evaluation of GetWindowSpareFlag.
pub fn evaluate_get_window_spare_flag(window_ptr: u32, flag_byte: Option<u8>) -> bool {
    if window_ptr == 0 {
        return false;
    }
    flag_byte.unwrap_or(0) != 0
}

/// Architecture-neutral evaluation of GetWindowKind.
pub fn evaluate_get_window_kind(window_ptr: u32, kind: Option<i16>) -> i16 {
    if window_ptr == 0 {
        return 0;
    }
    kind.unwrap_or(0)
}

/// Architecture-neutral parameters for SetWindowKind.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SetWindowKindParameters {
    window_ptr: u32,
    kind: i16,
}

impl SetWindowKindParameters {
    pub const fn window_ptr(&self) -> u32 {
        self.window_ptr
    }

    pub const fn kind(&self) -> i16 {
        self.kind
    }
}

pub fn evaluate_set_window_kind_parameters(
    window_ptr: u32,
    kind: i16,
) -> SetWindowKindParameters {
    SetWindowKindParameters { window_ptr, kind }
}

/// Architecture-neutral evaluation of GetWindowDefProc.
pub fn evaluate_get_window_def_proc(window_ptr: u32, def_proc: Option<u32>) -> u32 {
    if window_ptr == 0 {
        return 0;
    }
    def_proc.unwrap_or(0)
}

#[cfg(test)]
mod tests {
    #[test]
    fn reposition_window_bounds_evaluation_all_methods() {
        let content = (40, 50, 240, 350);
        let structure = (21, 49, 242, 352); // width: 303, height: 221, dx: 1, dy: 19
        let screen_w = 640;
        let screen_h = 480;
        let menu_h = 20;

        // Method 1: kWindowCenterOnMainScreen
        let res1 = evaluate_reposition_window_bounds(content, structure, None, WINDOW_CENTER_ON_MAIN_SCREEN, screen_w, screen_h, menu_h).unwrap();
        assert_eq!(res1.0, 158);
        assert_eq!(res1.1, 169);
        assert_eq!(res1.2, 158 + 200);
        assert_eq!(res1.3, 169 + 300);

        // Method 2: kWindowAlertPositionOnMainScreen
        let res2 = evaluate_reposition_window_bounds(content, structure, None, WINDOW_ALERT_POSITION_ON_MAIN_SCREEN, screen_w, screen_h, menu_h).unwrap();
        assert_eq!(res2.0, 86);
        assert_eq!(res2.1, 169);

        // Method 3: kWindowStaggerOnMainScreen (without parent)
        let res3 = evaluate_reposition_window_bounds(content, structure, None, WINDOW_STAGGER_ON_MAIN_SCREEN, screen_w, screen_h, menu_h).unwrap();
        assert_eq!(res3, res1);

        // Method 4: kWindowCenterOnParentWindow (with parent)
        let parent = (100, 150, 400, 550); // width: 400, height: 300
        let res4 = evaluate_reposition_window_bounds(content, structure, Some(parent), WINDOW_CENTER_ON_PARENT_WINDOW, screen_w, screen_h, menu_h).unwrap();
        assert_eq!(res4.0, 158);
        assert_eq!(res4.1, 199);

        // Method 5: kWindowAlertPositionOnParentWindow (with parent)
        let res5 = evaluate_reposition_window_bounds(content, structure, Some(parent), WINDOW_ALERT_POSITION_ON_PARENT_WINDOW, screen_w, screen_h, menu_h).unwrap();
        assert_eq!(res5.0, 134);
        assert_eq!(res5.1, 199);

        // Method 6: kWindowStaggerOnParentWindow (with parent)
        let res6 = evaluate_reposition_window_bounds(content, structure, Some(parent), WINDOW_STAGGER_ON_PARENT_WINDOW, screen_w, screen_h, menu_h).unwrap();
        assert_eq!(res6.0, 139);
        assert_eq!(res6.1, 171);

        // Method 7: kWindowCenterOnParentWindowScreen
        let res7 = evaluate_reposition_window_bounds(content, structure, Some(parent), WINDOW_CENTER_ON_PARENT_WINDOW_SCREEN, screen_w, screen_h, menu_h).unwrap();
        assert_eq!(res7, res1);

        // Method 8: kWindowAlertPositionOnParentWindowScreen
        let res8 = evaluate_reposition_window_bounds(content, structure, Some(parent), WINDOW_ALERT_POSITION_ON_PARENT_WINDOW_SCREEN, screen_w, screen_h, menu_h).unwrap();
        assert_eq!(res8, res2);

        // Method 9: kWindowStaggerOnParentWindowScreen (with parent)
        let res9 = evaluate_reposition_window_bounds(content, structure, Some(parent), WINDOW_STAGGER_ON_PARENT_WINDOW_SCREEN, screen_w, screen_h, menu_h).unwrap();
        assert_eq!(res9, res6);

        // Invalid method returns None
        assert_eq!(evaluate_reposition_window_bounds(content, structure, None, 0, screen_w, screen_h, menu_h), None);
        assert_eq!(evaluate_reposition_window_bounds(content, structure, None, 10, screen_w, screen_h, menu_h), None);
    }

    #[test]
    fn collapse_window_evaluation() {
        assert_eq!(evaluate_collapse_window(false, false), -50);
        assert_eq!(evaluate_collapse_window(false, true), -50);
        assert_eq!(evaluate_collapse_window(true, false), 0);
        assert_eq!(evaluate_collapse_window(true, true), -4);
        assert!(!evaluate_is_window_collapsed(true));
        assert!(!evaluate_is_window_collapsed(false));
    }

    #[test]
    fn window_user_state_flags_and_kind_evaluation() {
        assert_eq!(
            evaluate_get_window_user_state_parameters(0, 0x2000, true),
            Err(-50)
        );
        assert_eq!(
            evaluate_get_window_user_state_parameters(0x1000, 0, true),
            Err(-50)
        );
        assert_eq!(
            evaluate_get_window_user_state_parameters(0x1000, 0x2000, false),
            Err(-50)
        );
        let get_params = evaluate_get_window_user_state_parameters(0x1000, 0x2000, true).unwrap();
        assert_eq!(get_params.window_ptr(), 0x1000);
        assert_eq!(get_params.out_rect_ptr(), 0x2000);

        assert_eq!(
            evaluate_set_window_user_state_parameters(0, 0x2000, true),
            Err(-50)
        );
        assert_eq!(
            evaluate_set_window_user_state_parameters(0x1000, 0, true),
            Err(-50)
        );
        assert_eq!(
            evaluate_set_window_user_state_parameters(0x1000, 0x2000, false),
            Err(-50)
        );
        let set_params = evaluate_set_window_user_state_parameters(0x1000, 0x2000, true).unwrap();
        assert_eq!(set_params.window_ptr(), 0x1000);
        assert_eq!(set_params.in_rect_ptr(), 0x2000);

        assert!(!evaluate_get_window_go_away_flag(0, Some(1)));
        assert!(!evaluate_get_window_go_away_flag(0x1000, None));
        assert!(!evaluate_get_window_go_away_flag(0x1000, Some(0)));
        assert!(evaluate_get_window_go_away_flag(0x1000, Some(1)));

        assert!(!evaluate_get_window_spare_flag(0, Some(1)));
        assert!(!evaluate_get_window_spare_flag(0x1000, None));
        assert!(!evaluate_get_window_spare_flag(0x1000, Some(0)));
        assert!(evaluate_get_window_spare_flag(0x1000, Some(1)));

        assert_eq!(evaluate_get_window_kind(0, Some(8)), 0);
        assert_eq!(evaluate_get_window_kind(0x1000, None), 0);
        assert_eq!(evaluate_get_window_kind(0x1000, Some(8)), 8);
        assert_eq!(evaluate_get_window_kind(0x1000, Some(2)), 2);

        let kind_params = evaluate_set_window_kind_parameters(0x1000, 8);
        assert_eq!(kind_params.window_ptr(), 0x1000);
        assert_eq!(kind_params.kind(), 8);

        assert_eq!(evaluate_get_window_def_proc(0, Some(0x3000)), 0);
        assert_eq!(evaluate_get_window_def_proc(0x1000, None), 0);
        assert_eq!(evaluate_get_window_def_proc(0x1000, Some(0x3000)), 0x3000);
    }
    #[test]
    fn grow_retains_the_pointer_offset_inside_the_size_box() {
        let content = (185, 215, 430, 535);
        let limits = (64, 64, 600, 800);
        assert_eq!(
            super::grow_dimensions_from_drag(content, limits, (425, 525), (450, 550)),
            (270, 345)
        );
        assert_eq!(
            super::grow_dimensions_from_drag(content, limits, (425, 525), (425, 525)),
            (245, 320)
        );
        assert_eq!(
            super::grow_dimensions_from_drag(content, limits, (425, 525), (-100, -100)),
            (64, 64)
        );
    }
    use super::*;
    use std::collections::BTreeMap;

    #[derive(Clone, Debug, Default, Eq, PartialEq)]
    struct SnapshotMemory {
        bytes: BTreeMap<u32, u8>,
    }

    impl SnapshotMemory {
        fn read_byte(&self, address: u32) -> u8 {
            self.bytes.get(&address).copied().unwrap_or(0)
        }

        fn write_byte(&mut self, address: u32, value: u8) {
            self.bytes.insert(address, value);
        }

        fn write_word(&mut self, address: u32, value: u16) {
            for (index, byte) in value.to_be_bytes().into_iter().enumerate() {
                self.write_byte(address.wrapping_add(index as u32), byte);
            }
        }

        fn write_long(&mut self, address: u32, value: u32) {
            for (index, byte) in value.to_be_bytes().into_iter().enumerate() {
                self.write_byte(address.wrapping_add(index as u32), byte);
            }
        }

        fn write_rect(&mut self, address: u32, rect: WindowRect) {
            for (index, value) in [rect.0, rect.1, rect.2, rect.3].into_iter().enumerate() {
                self.write_word(address.wrapping_add(index as u32 * 2), value as u16);
            }
        }

        fn write_region(&mut self, handle: u32, region: u32, size: u16, bounds: WindowRect) {
            self.write_long(handle, region);
            self.write_word(region, size);
            self.write_rect(region.wrapping_add(2), bounds);
        }

        fn write_title(&mut self, handle: u32, title: u32, bytes: &[u8]) {
            self.write_long(handle, title);
            self.write_byte(title, bytes.len() as u8);
            for (index, byte) in bytes.iter().copied().enumerate() {
                self.write_byte(title.wrapping_add(1).wrapping_add(index as u32), byte);
            }
        }

        fn write_window_flags(&mut self, window: u32, visible: u8, hilited: u8) {
            self.write_byte(window.wrapping_add(WINDOW_VISIBLE_OFFSET), visible);
            self.write_byte(window.wrapping_add(WINDOW_HILITED_OFFSET), hilited);
        }

        fn write_window_title(&mut self, window: u32, handle: u32, title: u32, bytes: &[u8]) {
            self.write_long(window.wrapping_add(WINDOW_TITLE_HANDLE_OFFSET), handle);
            self.write_title(handle, title, bytes);
        }
    }

    fn snapshots(memory: &SnapshotMemory, order: &[u32]) -> Vec<WindowSnapshot> {
        snapshot_window_stack(order, |address| memory.read_byte(address))
    }

    #[test]
    fn window_snapshot_applies_ghost_window_to_the_list_derived_active_candidate() {
        const UTILITY: u32 = 0x1000;
        const DOCUMENT: u32 = 0x1200;
        let mut memory = SnapshotMemory::default();
        memory.write_window_flags(UTILITY, 0xFF, 0);
        memory.write_window_flags(DOCUMENT, 0xFF, 0xFF);
        memory.write_window_title(UTILITY, 0x2000, 0x2100, b"Utility");
        memory.write_window_title(DOCUMENT, 0x2200, 0x2300, b"Document");
        memory.write_long(crate::memory::globals::addr::GHOST_WINDOW, UTILITY);

        let ghosted = snapshots(&memory, &[UTILITY, 0, DOCUMENT]);
        assert_eq!(
            ghosted
                .iter()
                .map(|window| window.title.as_str())
                .collect::<Vec<_>>(),
            ["Utility", "Document"]
        );
        assert!(!ghosted[0].active);
        assert!(ghosted[1].active);

        memory.write_long(crate::memory::globals::addr::GHOST_WINDOW, 0);
        let ordinary = snapshots(&memory, &[UTILITY, DOCUMENT]);
        assert!(ordinary[0].active);
        assert!(ordinary[1].active, "direct hilite remains visible");
    }

    #[test]
    fn window_snapshot_accepts_documented_ff_booleans() {
        let mut memory = SnapshotMemory::default();
        for (window, visible, hilited) in [(0x1000, 0xFF, 0), (0x1200, 1, 0), (0x1400, 2, 2)] {
            memory.write_window_flags(window, visible, hilited);
        }

        let result = snapshots(&memory, &[0x1000, 0x1200, 0x1400]);
        assert!(result.iter().all(|window| window.visible));
        assert!(result[0].active, "first visible list entry is front");
        assert!(!result[1].active);
        assert!(result[2].active, "every nonzero hilite value is true");
    }

    fn write_complete_window(memory: &mut SnapshotMemory, window: u32, color: bool) {
        const PIXMAP_HANDLE: u32 = 0x3000;
        const PIXMAP: u32 = 0x3100;
        const STRUCTURE_HANDLE: u32 = 0x3200;
        const STRUCTURE: u32 = 0x3300;
        const VISIBLE_HANDLE: u32 = 0x3400;
        const VISIBLE: u32 = 0x3500;
        const UPDATE_HANDLE: u32 = 0x3600;
        const UPDATE: u32 = 0x3700;
        const TITLE_HANDLE: u32 = 0x3800;
        const TITLE: u32 = 0x3900;

        memory.write_window_flags(window, 0xFF, 0xFF);
        memory.write_rect(window.wrapping_add(16), (10, 20, 30, 40));
        if color {
            memory.write_word(window.wrapping_add(6), 0xC000);
            memory.write_long(window.wrapping_add(2), PIXMAP_HANDLE);
            memory.write_long(PIXMAP_HANDLE, PIXMAP);
            memory.write_word(PIXMAP.wrapping_add(6), (-100i16) as u16);
            memory.write_word(PIXMAP.wrapping_add(8), (-200i16) as u16);
        } else {
            memory.write_word(window.wrapping_add(6), 0);
            memory.write_word(window.wrapping_add(8), (-100i16) as u16);
            memory.write_word(window.wrapping_add(10), (-200i16) as u16);
        }
        memory.write_long(
            window.wrapping_add(WINDOW_STRUCTURE_RGN_OFFSET),
            STRUCTURE_HANDLE,
        );
        memory.write_region(STRUCTURE_HANDLE, STRUCTURE, 10, (100, 200, 140, 250));
        memory.write_long(window.wrapping_add(24), VISIBLE_HANDLE);
        memory.write_region(VISIBLE_HANDLE, VISIBLE, 10, (5, 7, 15, 17));
        memory.write_long(window.wrapping_add(WINDOW_UPDATE_RGN_OFFSET), UPDATE_HANDLE);
        memory.write_region(UPDATE_HANDLE, UPDATE, 10, (101, 202, 111, 212));
        memory.write_window_title(window, TITLE_HANDLE, TITLE, b"Window");
    }

    #[test]
    fn window_snapshot_decodes_grafport_and_cgrafport_equivalently() {
        let mut graf = SnapshotMemory::default();
        let mut color = SnapshotMemory::default();
        write_complete_window(&mut graf, 0x1000, false);
        write_complete_window(&mut color, 0x1000, true);

        let graf = snapshots(&graf, &[0x1000]);
        let color = snapshots(&color, &[0x1000]);
        assert_eq!(graf, color);
        assert_eq!(graf[0].bounds, (110, 220, 130, 240));
        assert_eq!(graf[0].visible_region, Some((105, 207, 115, 217)));
        assert_eq!(graf[0].structure_bounds, Some((100, 200, 140, 250)));
        assert_eq!(graf[0].update_region, Some((101, 202, 111, 212)));
    }

    #[test]
    fn window_snapshot_has_one_explicit_malformed_projection() {
        const WINDOW: u32 = u32::MAX - 32;
        const BROKEN_PIXMAP_HANDLE: u32 = 0x900;
        const STRUCTURE_HANDLE: u32 = 0xA00;
        const STRUCTURE: u32 = 0xA20;
        const TITLE_HANDLE: u32 = 0xB00;
        const TITLE: u32 = 0xB20;
        let mut memory = SnapshotMemory::default();
        memory.write_word(WINDOW.wrapping_add(6), 0xC000);
        memory.write_long(WINDOW.wrapping_add(2), BROKEN_PIXMAP_HANDLE);
        memory.write_rect(WINDOW.wrapping_add(16), (10, 20, 30, 40));
        memory.write_long(
            WINDOW.wrapping_add(WINDOW_STRUCTURE_RGN_OFFSET),
            STRUCTURE_HANDLE,
        );
        memory.write_region(STRUCTURE_HANDLE, STRUCTURE, 2, (1, 2, 5, 7));
        memory.write_long(
            WINDOW.wrapping_add(WINDOW_TITLE_HANDLE_OFFSET),
            TITLE_HANDLE,
        );
        memory.write_long(TITLE_HANDLE, TITLE);
        memory.write_byte(TITLE, 3);
        memory.write_byte(TITLE.wrapping_add(1), b'A');
        memory.write_byte(TITLE.wrapping_add(3), b'C');

        let result = snapshots(&memory, &[WINDOW]);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].bounds, (10, 20, 30, 40));
        assert_eq!(result[0].structure_bounds, Some((1, 2, 5, 7)));
        assert_eq!(result[0].visible_region, None);
        assert_eq!(result[0].update_region, None);
        assert_eq!(result[0].title.as_bytes(), b"A\0C");
        assert!(!result[0].visible);
        assert!(!result[0].active);
    }

    #[test]
    fn window_snapshot_returns_owned_data_without_mutating_input() {
        let mut memory = SnapshotMemory::default();
        memory.write_window_flags(0x1000, 0xFF, 0xFF);
        memory.write_window_title(0x1000, 0x2000, 0x2100, b"Before");
        let before = memory.clone();
        let result = snapshots(&memory, &[0x1000]);
        assert_eq!(memory, before);

        memory.write_window_title(0x1000, 0x2000, 0x2100, b"After");
        assert_eq!(result[0].title, "Before");
        assert_eq!(snapshots(&memory, &[0x1000])[0].title, "After");
    }
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
            .stripe_ink
            .iter()
            .filter(|(top, left, bottom, _)| {
                *left == 41 && *bottom == top.saturating_add(1) && (32..=44).contains(top)
            })
            .map(|(top, _, _, _)| *top)
            .collect::<Vec<_>>();
        assert_eq!(stripe_rows, [32, 34, 36, 38, 40, 42, 44]);
        assert!(
            chrome.stripe_ink.contains(&(32, 41, 33, 47)),
            "the short stripe segment left of the close box must remain identifiable"
        );
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
