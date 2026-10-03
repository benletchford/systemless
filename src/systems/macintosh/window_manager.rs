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

const WINDOW_VISIBLE_OFFSET: u32 = WINDOW_VISIBLE_FLAG_OFFSET;
const WINDOW_HILITED_OFFSET: u32 = WINDOW_HILITED_FLAG_OFFSET;


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
pub const WINDOW_STRUCTURE_RGN_OFFSET: u32 = 114;
#[allow(dead_code)]
pub const WINDOW_CONTENT_RGN_OFFSET: u32 = 118;
#[allow(dead_code)]
pub const WINDOW_UPDATE_RGN_OFFSET: u32 = 122;
#[allow(dead_code)]
pub const WINDOW_DEF_PROC_HANDLE_OFFSET: u32 = 126;
#[allow(dead_code)]
pub const WINDOW_STATE_DATA_HANDLE_OFFSET: u32 = 130;
#[allow(dead_code)]
pub const WINDOW_TITLE_HANDLE_OFFSET: u32 = 134;
#[allow(dead_code)]
pub const WINDOW_TITLE_WIDTH_OFFSET: u32 = 138;
#[allow(dead_code)]
pub const WINDOW_CONTROL_LIST_OFFSET: u32 = 140;
#[allow(dead_code)]
pub const WINDOW_NEXT_WINDOW_OFFSET: u32 = 144;
#[allow(dead_code)]
pub const WINDOW_PIC_HANDLE_OFFSET: u32 = 148;
#[allow(dead_code)]
pub const WINDOW_REF_CON_OFFSET: u32 = 152;
#[allow(dead_code)]
pub const LM_WINDOW_LIST_ADDR: u32 = 0x09D6;
#[allow(dead_code)]
pub const LM_AUX_WIN_HEAD_ADDR: u32 = 0x0CD0;

/// Canonical Mac OS WindowModality constants.
#[allow(dead_code)]
pub const WINDOW_MODALITY_NONE: u32 = 0;
#[allow(dead_code)]
pub const WINDOW_MODALITY_SYSTEM_MODAL: u32 = 1;
#[allow(dead_code)]
pub const WINDOW_MODALITY_APP_MODAL: u32 = 2;
#[allow(dead_code)]
pub const WINDOW_MODALITY_WINDOW_MODAL: u32 = 3;

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

/// Architecture-neutral evaluation of window hilite flag.
pub fn evaluate_get_window_hilited(window_ptr: u32, flag_byte: Option<u8>) -> bool {
    if window_ptr == 0 {
        return false;
    }
    flag_byte.unwrap_or(0) != 0
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

/// Architecture-neutral evaluation of GetWindowStructureRgn.
pub fn evaluate_get_window_structure_rgn(window_ptr: u32, structure_rgn: Option<u32>) -> u32 {
    if window_ptr == 0 {
        return 0;
    }
    structure_rgn.unwrap_or(0)
}

/// Architecture-neutral evaluation of GetWindowContentRgn.
pub fn evaluate_get_window_content_rgn(window_ptr: u32, content_rgn: Option<u32>) -> u32 {
    if window_ptr == 0 {
        return 0;
    }
    content_rgn.unwrap_or(0)
}

/// Architecture-neutral evaluation of GetWindowUpdateRgn.
pub fn evaluate_get_window_update_rgn(window_ptr: u32, update_rgn: Option<u32>) -> u32 {
    if window_ptr == 0 {
        return 0;
    }
    update_rgn.unwrap_or(0)
}

/// Architecture-neutral evaluation of IsWindowUpdatePending.
pub fn evaluate_is_window_update_pending(
    window_ptr: u32,
    update_rgn_handle: Option<u32>,
    update_rgn_bounds: Option<WindowRect>,
) -> bool {
    if window_ptr == 0 {
        return false;
    }
    let Some(handle) = update_rgn_handle else {
        return false;
    };
    if handle == 0 {
        return false;
    }
    let Some(bounds) = update_rgn_bounds else {
        return false;
    };
    bounds.2 > bounds.0 && bounds.3 > bounds.1
}

/// Architecture-neutral evaluation of GetNextWindow.
pub fn evaluate_get_next_window(window_ptr: u32, next_window: Option<u32>) -> u32 {
    if window_ptr == 0 {
        return 0;
    }
    next_window.unwrap_or(0)
}

/// Architecture-neutral evaluation of GetPreviousWindow.
pub fn evaluate_get_previous_window(window_ptr: u32, window_order: &[u32]) -> u32 {
    if window_ptr == 0 {
        return 0;
    }
    let Some(index) = window_order.iter().position(|&w| w == window_ptr) else {
        return 0;
    };
    if index == 0 {
        return 0;
    }
    window_order[index - 1]
}

/// Architecture-neutral evaluation of IsWindowActive.
pub fn evaluate_is_window_active(window_ptr: u32, is_hilited: bool, is_front: bool) -> bool {
    if window_ptr == 0 {
        return false;
    }
    is_hilited || is_front
}

/// Architecture-neutral evaluation of ActiveNonFloatingWindow.
pub fn evaluate_active_non_floating_window(
    window_order: &[u32],
    mut is_eligible: impl FnMut(u32) -> bool,
) -> u32 {
    window_order
        .iter()
        .copied()
        .find(|&w| w != 0 && is_eligible(w))
        .unwrap_or(0)
}

/// Architecture-neutral parameter validation for GetWindowModality.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GetWindowModalityParameters {
    window_ptr: u32,
    out_modal_kind_ptr: u32,
    out_unavailable_window_ptr: u32,
}

impl GetWindowModalityParameters {
    pub const fn window_ptr(&self) -> u32 {
        self.window_ptr
    }

    pub const fn out_modal_kind_ptr(&self) -> u32 {
        self.out_modal_kind_ptr
    }

    pub const fn out_unavailable_window_ptr(&self) -> u32 {
        self.out_unavailable_window_ptr
    }
}

pub fn evaluate_get_window_modality_parameters(
    window_ptr: u32,
    out_modal_kind_ptr: u32,
    out_unavailable_window_ptr: u32,
    can_write_kind: bool,
    can_write_unavail: bool,
) -> Result<GetWindowModalityParameters, i16> {
    if window_ptr == 0 {
        return Err(-50); // PPC_PARAM_ERR
    }
    if out_modal_kind_ptr != 0 && !can_write_kind {
        return Err(-50);
    }
    if out_unavailable_window_ptr != 0 && !can_write_unavail {
        return Err(-50);
    }
    Ok(GetWindowModalityParameters {
        window_ptr,
        out_modal_kind_ptr,
        out_unavailable_window_ptr,
    })
}

/// Architecture-neutral parameter validation for SetWindowModality.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SetWindowModalityParameters {
    window_ptr: u32,
    modal_kind: u32,
    unavailable_window_ptr: u32,
}

impl SetWindowModalityParameters {
    pub const fn window_ptr(&self) -> u32 {
        self.window_ptr
    }

    pub const fn modal_kind(&self) -> u32 {
        self.modal_kind
    }

    pub const fn unavailable_window_ptr(&self) -> u32 {
        self.unavailable_window_ptr
    }
}

pub fn evaluate_set_window_modality_parameters(
    window_ptr: u32,
    modal_kind: u32,
    unavailable_window_ptr: u32,
) -> Result<SetWindowModalityParameters, i16> {
    if window_ptr == 0 || modal_kind > WINDOW_MODALITY_WINDOW_MODAL {
        return Err(-50); // PPC_PARAM_ERR
    }
    Ok(SetWindowModalityParameters {
        window_ptr,
        modal_kind,
        unavailable_window_ptr,
    })
}

/// Canonical Mac OS Window Property error constants.
#[allow(dead_code)]
pub const WINDOW_PROPERTY_NOT_FOUND_ERR: i16 = -5604;

/// Canonical Mac OS Window Property record.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ProcessWindowProperty {
    pub creator: u32,
    pub tag: u32,
    pub attributes: u32,
    pub data: Vec<u8>,
}

#[allow(dead_code)]
pub fn set_window_property(
    properties: &mut Vec<ProcessWindowProperty>,
    creator: u32,
    tag: u32,
    attributes: u32,
    data: Vec<u8>,
) {
    if let Some(prop) = properties
        .iter_mut()
        .find(|p| p.creator == creator && p.tag == tag)
    {
        prop.attributes = attributes;
        prop.data = data;
    } else {
        properties.push(ProcessWindowProperty {
            creator,
            tag,
            attributes,
            data,
        });
    }
}

#[allow(dead_code)]
pub fn get_window_property<'a>(
    properties: &'a [ProcessWindowProperty],
    creator: u32,
    tag: u32,
) -> Option<&'a ProcessWindowProperty> {
    properties
        .iter()
        .find(|p| p.creator == creator && p.tag == tag)
}

#[allow(dead_code)]
pub fn remove_window_property(
    properties: &mut Vec<ProcessWindowProperty>,
    creator: u32,
    tag: u32,
) -> bool {
    let before = properties.len();
    properties.retain(|p| !(p.creator == creator && p.tag == tag));
    properties.len() < before
}

#[allow(dead_code)]
pub fn change_window_property_attributes(
    properties: &mut Vec<ProcessWindowProperty>,
    creator: u32,
    tag: u32,
    set: u32,
    clear: u32,
) -> Result<u32, i16> {
    if let Some(prop) = properties
        .iter_mut()
        .find(|p| p.creator == creator && p.tag == tag)
    {
        prop.attributes = (prop.attributes | set) & !clear;
        Ok(prop.attributes)
    } else {
        Err(WINDOW_PROPERTY_NOT_FOUND_ERR)
    }
}

/// Architecture-neutral parameter validation for SetWindowProperty.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SetWindowPropertyParameters {
    window_ptr: u32,
    creator: u32,
    tag: u32,
    data: Vec<u8>,
}

#[allow(dead_code)]
impl SetWindowPropertyParameters {
    pub const fn window_ptr(&self) -> u32 {
        self.window_ptr
    }

    pub const fn creator(&self) -> u32 {
        self.creator
    }

    pub const fn tag(&self) -> u32 {
        self.tag
    }

    pub fn data(&self) -> &[u8] {
        &self.data
    }

    pub fn into_data(self) -> Vec<u8> {
        self.data
    }
}

pub fn evaluate_set_window_property_parameters(
    window_ptr: u32,
    creator: u32,
    tag: u32,
    data: Vec<u8>,
) -> Result<SetWindowPropertyParameters, i16> {
    if window_ptr == 0 {
        return Err(-50); // PPC_PARAM_ERR
    }
    Ok(SetWindowPropertyParameters {
        window_ptr,
        creator,
        tag,
        data,
    })
}

/// Architecture-neutral parameter validation for GetWindowProperty.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GetWindowPropertyParameters {
    window_ptr: u32,
    creator: u32,
    tag: u32,
    buffer_size: u32,
    out_actual_size_ptr: u32,
    out_data_ptr: u32,
}

impl GetWindowPropertyParameters {
    pub const fn window_ptr(&self) -> u32 {
        self.window_ptr
    }

    pub const fn creator(&self) -> u32 {
        self.creator
    }

    pub const fn tag(&self) -> u32 {
        self.tag
    }

    pub const fn buffer_size(&self) -> u32 {
        self.buffer_size
    }

    pub const fn out_actual_size_ptr(&self) -> u32 {
        self.out_actual_size_ptr
    }

    pub const fn out_data_ptr(&self) -> u32 {
        self.out_data_ptr
    }
}

pub fn evaluate_get_window_property_parameters(
    window_ptr: u32,
    creator: u32,
    tag: u32,
    buffer_size: u32,
    out_actual_size_ptr: u32,
    out_data_ptr: u32,
    can_write_actual_size: bool,
    can_write_data: bool,
) -> Result<GetWindowPropertyParameters, i16> {
    if window_ptr == 0 {
        return Err(-50); // PPC_PARAM_ERR
    }
    if out_actual_size_ptr != 0 && !can_write_actual_size {
        return Err(-50);
    }
    if out_data_ptr != 0 && buffer_size > 0 && !can_write_data {
        return Err(-50);
    }
    Ok(GetWindowPropertyParameters {
        window_ptr,
        creator,
        tag,
        buffer_size,
        out_actual_size_ptr,
        out_data_ptr,
    })
}

/// Architecture-neutral parameter validation for GetWindowPropertySize.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GetWindowPropertySizeParameters {
    window_ptr: u32,
    creator: u32,
    tag: u32,
    out_actual_size_ptr: u32,
}

impl GetWindowPropertySizeParameters {
    pub const fn window_ptr(&self) -> u32 {
        self.window_ptr
    }

    pub const fn creator(&self) -> u32 {
        self.creator
    }

    pub const fn tag(&self) -> u32 {
        self.tag
    }

    pub const fn out_actual_size_ptr(&self) -> u32 {
        self.out_actual_size_ptr
    }
}

pub fn evaluate_get_window_property_size_parameters(
    window_ptr: u32,
    creator: u32,
    tag: u32,
    out_actual_size_ptr: u32,
    can_write_actual_size: bool,
) -> Result<GetWindowPropertySizeParameters, i16> {
    if window_ptr == 0 {
        return Err(-50); // PPC_PARAM_ERR
    }
    if out_actual_size_ptr != 0 && !can_write_actual_size {
        return Err(-50);
    }
    Ok(GetWindowPropertySizeParameters {
        window_ptr,
        creator,
        tag,
        out_actual_size_ptr,
    })
}

/// Architecture-neutral parameter validation for RemoveWindowProperty.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RemoveWindowPropertyParameters {
    window_ptr: u32,
    creator: u32,
    tag: u32,
}

impl RemoveWindowPropertyParameters {
    pub const fn window_ptr(&self) -> u32 {
        self.window_ptr
    }

    pub const fn creator(&self) -> u32 {
        self.creator
    }

    pub const fn tag(&self) -> u32 {
        self.tag
    }
}

pub fn evaluate_remove_window_property_parameters(
    window_ptr: u32,
    creator: u32,
    tag: u32,
) -> Result<RemoveWindowPropertyParameters, i16> {
    if window_ptr == 0 {
        return Err(-50); // PPC_PARAM_ERR
    }
    Ok(RemoveWindowPropertyParameters {
        window_ptr,
        creator,
        tag,
    })
}

/// Architecture-neutral parameter validation for GetWindowPropertyAttributes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GetWindowPropertyAttributesParameters {
    window_ptr: u32,
    creator: u32,
    tag: u32,
    out_attributes_ptr: u32,
}

impl GetWindowPropertyAttributesParameters {
    pub const fn window_ptr(&self) -> u32 {
        self.window_ptr
    }

    pub const fn creator(&self) -> u32 {
        self.creator
    }

    pub const fn tag(&self) -> u32 {
        self.tag
    }

    pub const fn out_attributes_ptr(&self) -> u32 {
        self.out_attributes_ptr
    }
}

pub fn evaluate_get_window_property_attributes_parameters(
    window_ptr: u32,
    creator: u32,
    tag: u32,
    out_attributes_ptr: u32,
    can_write_attributes: bool,
) -> Result<GetWindowPropertyAttributesParameters, i16> {
    if window_ptr == 0 {
        return Err(-50); // PPC_PARAM_ERR
    }
    if out_attributes_ptr != 0 && !can_write_attributes {
        return Err(-50);
    }
    Ok(GetWindowPropertyAttributesParameters {
        window_ptr,
        creator,
        tag,
        out_attributes_ptr,
    })
}

/// Architecture-neutral parameter validation for ChangeWindowPropertyAttributes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ChangeWindowPropertyAttributesParameters {
    window_ptr: u32,
    creator: u32,
    tag: u32,
    to_set: u32,
    to_clear: u32,
}

impl ChangeWindowPropertyAttributesParameters {
    pub const fn window_ptr(&self) -> u32 {
        self.window_ptr
    }

    pub const fn creator(&self) -> u32 {
        self.creator
    }

    pub const fn tag(&self) -> u32 {
        self.tag
    }

    pub const fn to_set(&self) -> u32 {
        self.to_set
    }

    pub const fn to_clear(&self) -> u32 {
        self.to_clear
    }
}

pub fn evaluate_change_window_property_attributes_parameters(
    window_ptr: u32,
    creator: u32,
    tag: u32,
    to_set: u32,
    to_clear: u32,
) -> Result<ChangeWindowPropertyAttributesParameters, i16> {
    if window_ptr == 0 {
        return Err(-50); // PPC_PARAM_ERR
    }
    Ok(ChangeWindowPropertyAttributesParameters {
        window_ptr,
        creator,
        tag,
        to_set,
        to_clear,
    })
}

/// Architecture-neutral parameter validation for SetWindowPic.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SetWindowPicParameters {
    window_ptr: u32,
    pic_handle: u32,
}

impl SetWindowPicParameters {
    pub const fn window_ptr(&self) -> u32 {
        self.window_ptr
    }

    pub const fn pic_handle(&self) -> u32 {
        self.pic_handle
    }
}

#[allow(dead_code)]
pub fn evaluate_get_window_pic(window_ptr: u32, pic_handle: Option<u32>) -> u32 {
    if window_ptr == 0 {
        0
    } else {
        pic_handle.unwrap_or(0)
    }
}

#[allow(dead_code)]
pub fn evaluate_set_window_pic_parameters(
    window_ptr: u32,
    pic_handle: u32,
    can_write: bool,
) -> Result<SetWindowPicParameters, i16> {
    if window_ptr == 0 || !can_write {
        return Err(-50); // PPC_PARAM_ERR
    }
    Ok(SetWindowPicParameters {
        window_ptr,
        pic_handle,
    })
}

/// Architecture-neutral parameter validation for SetWRefCon.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SetWRefConParameters {
    window_ptr: u32,
    ref_con: u32,
}

impl SetWRefConParameters {
    pub const fn window_ptr(&self) -> u32 {
        self.window_ptr
    }

    pub const fn ref_con(&self) -> u32 {
        self.ref_con
    }
}

#[allow(dead_code)]
pub fn evaluate_get_w_ref_con(window_ptr: u32, ref_con: Option<u32>) -> u32 {
    if window_ptr == 0 {
        0
    } else {
        ref_con.unwrap_or(0)
    }
}

#[allow(dead_code)]
pub fn evaluate_set_w_ref_con_parameters(
    window_ptr: u32,
    ref_con: u32,
    can_write: bool,
) -> Result<SetWRefConParameters, i16> {
    if window_ptr == 0 || !can_write {
        return Err(-50); // PPC_PARAM_ERR
    }
    Ok(SetWRefConParameters {
        window_ptr,
        ref_con,
    })
}

#[allow(dead_code)]
pub fn evaluate_get_w_variant(window_ptr: u32, proc_id: Option<i16>) -> i16 {
    if window_ptr == 0 {
        0
    } else {
        proc_id.map(|id| id & 0x0F).unwrap_or(0)
    }
}

#[allow(dead_code)]
pub fn evaluate_get_window_port(window_ptr: u32) -> u32 {
    window_ptr
}

#[allow(dead_code)]
pub fn evaluate_set_port_window_port(window_ptr: u32) -> u32 {
    window_ptr
}

/// Architecture-neutral parameter validation for GetAuxWin.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GetAuxWinParameters {
    window_ptr: u32,
    out_aux_ctable_ptr: u32,
}

impl GetAuxWinParameters {
    pub const fn window_ptr(&self) -> u32 {
        self.window_ptr
    }

    pub const fn out_aux_ctable_ptr(&self) -> u32 {
        self.out_aux_ctable_ptr
    }
}

#[allow(dead_code)]
pub fn evaluate_get_aux_win_parameters(
    window_ptr: u32,
    out_aux_ctable_ptr: u32,
    can_write: bool,
) -> Result<GetAuxWinParameters, i16> {
    if window_ptr == 0 {
        return Err(-50);
    }
    if out_aux_ctable_ptr != 0 && !can_write {
        return Err(-50);
    }
    Ok(GetAuxWinParameters {
        window_ptr,
        out_aux_ctable_ptr,
    })
}

#[allow(dead_code)]
pub fn evaluate_get_aux_win(color_table: Option<u32>, is_tracked: bool) -> (u32, bool) {
    let ctab = color_table.unwrap_or(0);
    let success = ctab != 0 || is_tracked;
    (ctab, success)
}

#[allow(dead_code)]
pub fn evaluate_lm_get_window_list(first_window: Option<u32>, low_mem_window: Option<u32>) -> u32 {
    first_window.filter(|&w| w != 0).or(low_mem_window).unwrap_or(0)
}

/// Architecture-neutral parameter validation for LMSetWindowList.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LmSetWindowListParameters {
    window_ptr: u32,
}

impl LmSetWindowListParameters {
    pub const fn window_ptr(&self) -> u32 {
        self.window_ptr
    }
}

#[allow(dead_code)]
pub fn evaluate_lm_set_window_list_parameters(
    window_ptr: u32,
    can_write_low_mem: bool,
) -> Result<LmSetWindowListParameters, i16> {
    if !can_write_low_mem {
        return Err(-50);
    }
    Ok(LmSetWindowListParameters { window_ptr })
}

#[allow(dead_code)]
pub fn evaluate_lm_get_aux_win_head(aux_head: Option<u32>) -> u32 {
    aux_head.unwrap_or(0)
}

/// Architecture-neutral parameter validation for LMSetAuxWinHead.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LmSetAuxWinHeadParameters {
    aux_head: u32,
}

impl LmSetAuxWinHeadParameters {
    pub const fn aux_head(&self) -> u32 {
        self.aux_head
    }
}

#[allow(dead_code)]
pub fn evaluate_lm_set_aux_win_head_parameters(
    aux_head: u32,
    can_write_low_mem: bool,
) -> Result<LmSetAuxWinHeadParameters, i16> {
    if !can_write_low_mem {
        return Err(-50);
    }
    Ok(LmSetAuxWinHeadParameters { aux_head })
}

/// Standard window feature attributes returned by GetWindowFeatures.
/// Carbon/MacWindows.h:
/// kWindowCanCollapseAttribute = (1 << 0)
/// kWindowCanGetWindowInfoAttribute = (1 << 1)
/// kWindowHasProxyIconAttribute = (1 << 2)
pub const WINDOW_FEATURES_STANDARD: u32 = 0x0000_0007;

/// Standard default window structure frame margins (top, left, bottom, right).
pub const WINDOW_STRUCTURE_WIDTHS_STANDARD: (i16, i16, i16, i16) = (20, 1, 1, 1);

#[allow(dead_code)]
pub fn evaluate_is_window_modified(window_ptr: u32, is_modified: bool) -> bool {
    window_ptr != 0 && is_modified
}

/// Architecture-neutral parameter validation for SetWindowModified.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SetWindowModifiedParameters {
    window_ptr: u32,
    modified: bool,
}

impl SetWindowModifiedParameters {
    pub const fn window_ptr(&self) -> u32 {
        self.window_ptr
    }

    pub const fn modified(&self) -> bool {
        self.modified
    }
}

#[allow(dead_code)]
pub fn evaluate_set_window_modified_parameters(
    window_ptr: u32,
    modified: bool,
) -> Result<SetWindowModifiedParameters, i16> {
    if window_ptr == 0 {
        return Err(-50); // PPC_PARAM_ERR
    }
    Ok(SetWindowModifiedParameters {
        window_ptr,
        modified,
    })
}

/// Architecture-neutral parameter validation for GetWindowProxyIcon.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GetWindowProxyIconParameters {
    window_ptr: u32,
    out_icon_ptr: u32,
}

impl GetWindowProxyIconParameters {
    pub const fn window_ptr(&self) -> u32 {
        self.window_ptr
    }

    pub const fn out_icon_ptr(&self) -> u32 {
        self.out_icon_ptr
    }
}

#[allow(dead_code)]
pub fn evaluate_get_window_proxy_icon_parameters(
    window_ptr: u32,
    out_icon_ptr: u32,
    can_write: bool,
) -> Result<GetWindowProxyIconParameters, i16> {
    if window_ptr == 0 || out_icon_ptr == 0 || !can_write {
        return Err(-50); // PPC_PARAM_ERR
    }
    Ok(GetWindowProxyIconParameters {
        window_ptr,
        out_icon_ptr,
    })
}

/// Architecture-neutral parameter validation for SetWindowProxyIcon.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SetWindowProxyIconParameters {
    window_ptr: u32,
    icon: u32,
}

impl SetWindowProxyIconParameters {
    pub const fn window_ptr(&self) -> u32 {
        self.window_ptr
    }

    pub const fn icon(&self) -> u32 {
        self.icon
    }
}

#[allow(dead_code)]
pub fn evaluate_set_window_proxy_icon_parameters(
    window_ptr: u32,
    icon: u32,
) -> Result<SetWindowProxyIconParameters, i16> {
    if window_ptr == 0 {
        return Err(-50); // PPC_PARAM_ERR
    }
    Ok(SetWindowProxyIconParameters {
        window_ptr,
        icon,
    })
}

/// Architecture-neutral parameter validation for RemoveWindowProxy.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RemoveWindowProxyParameters {
    window_ptr: u32,
}

impl RemoveWindowProxyParameters {
    pub const fn window_ptr(&self) -> u32 {
        self.window_ptr
    }
}

#[allow(dead_code)]
pub fn evaluate_remove_window_proxy_parameters(
    window_ptr: u32,
) -> Result<RemoveWindowProxyParameters, i16> {
    if window_ptr == 0 {
        return Err(-50); // PPC_PARAM_ERR
    }
    Ok(RemoveWindowProxyParameters { window_ptr })
}

#[allow(dead_code)]
pub fn evaluate_is_window_path_select_click(
    window_ptr: u32,
    modifiers: u16,
    in_title_bar: bool,
) -> bool {
    const CMD_KEY_MASK: u16 = 0x0100;
    window_ptr != 0 && in_title_bar && (modifiers & CMD_KEY_MASK != 0)
}

/// Architecture-neutral parameter validation for GetWindowFeatures.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GetWindowFeaturesParameters {
    window_ptr: u32,
    out_features_ptr: u32,
}

impl GetWindowFeaturesParameters {
    pub const fn window_ptr(&self) -> u32 {
        self.window_ptr
    }

    pub const fn out_features_ptr(&self) -> u32 {
        self.out_features_ptr
    }
}

#[allow(dead_code)]
pub fn evaluate_get_window_features_parameters(
    window_ptr: u32,
    out_features_ptr: u32,
    can_write: bool,
) -> Result<GetWindowFeaturesParameters, i16> {
    if window_ptr == 0 || out_features_ptr == 0 || !can_write {
        return Err(-50); // PPC_PARAM_ERR
    }
    Ok(GetWindowFeaturesParameters {
        window_ptr,
        out_features_ptr,
    })
}

#[allow(dead_code)]
pub fn evaluate_get_window_features(window_ptr: u32) -> u32 {
    if window_ptr != 0 {
        WINDOW_FEATURES_STANDARD
    } else {
        0
    }
}

/// Architecture-neutral parameter validation for GetWindowStructureWidths.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GetWindowStructureWidthsParameters {
    window_ptr: u32,
    out_rect_ptr: u32,
}

impl GetWindowStructureWidthsParameters {
    pub const fn window_ptr(&self) -> u32 {
        self.window_ptr
    }

    pub const fn out_rect_ptr(&self) -> u32 {
        self.out_rect_ptr
    }
}

#[allow(dead_code)]
pub fn evaluate_get_window_structure_widths_parameters(
    window_ptr: u32,
    out_rect_ptr: u32,
    can_write: bool,
) -> Result<GetWindowStructureWidthsParameters, i16> {
    if window_ptr == 0 || out_rect_ptr == 0 || !can_write {
        return Err(-50); // PPC_PARAM_ERR
    }
    Ok(GetWindowStructureWidthsParameters {
        window_ptr,
        out_rect_ptr,
    })
}

#[allow(dead_code)]
pub fn evaluate_get_window_structure_widths(window_ptr: u32) -> (i16, i16, i16, i16) {
    if window_ptr != 0 {
        WINDOW_STRUCTURE_WIDTHS_STANDARD
    } else {
        (0, 0, 0, 0)
    }
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
    fn window_modality_activation_and_chain_traversal_evaluation() {
        // Regions
        assert_eq!(evaluate_get_window_structure_rgn(0, Some(0x2000)), 0);
        assert_eq!(evaluate_get_window_structure_rgn(0x1000, None), 0);
        assert_eq!(evaluate_get_window_structure_rgn(0x1000, Some(0x2000)), 0x2000);

        assert_eq!(evaluate_get_window_content_rgn(0, Some(0x2000)), 0);
        assert_eq!(evaluate_get_window_content_rgn(0x1000, None), 0);
        assert_eq!(evaluate_get_window_content_rgn(0x1000, Some(0x2000)), 0x2000);

        assert_eq!(evaluate_get_window_update_rgn(0, Some(0x2000)), 0);
        assert_eq!(evaluate_get_window_update_rgn(0x1000, None), 0);
        assert_eq!(evaluate_get_window_update_rgn(0x1000, Some(0x2000)), 0x2000);

        // Update pending
        assert!(!evaluate_is_window_update_pending(0, Some(0x2000), Some((10, 10, 50, 50))));
        assert!(!evaluate_is_window_update_pending(0x1000, None, Some((10, 10, 50, 50))));
        assert!(!evaluate_is_window_update_pending(0x1000, Some(0), Some((10, 10, 50, 50))));
        assert!(!evaluate_is_window_update_pending(0x1000, Some(0x2000), None));
        assert!(!evaluate_is_window_update_pending(0x1000, Some(0x2000), Some((10, 10, 10, 50))));
        assert!(!evaluate_is_window_update_pending(0x1000, Some(0x2000), Some((10, 10, 50, 10))));
        assert!(evaluate_is_window_update_pending(0x1000, Some(0x2000), Some((10, 10, 50, 50))));

        // Chain traversal
        assert_eq!(evaluate_get_next_window(0, Some(0x2000)), 0);
        assert_eq!(evaluate_get_next_window(0x1000, None), 0);
        assert_eq!(evaluate_get_next_window(0x1000, Some(0x2000)), 0x2000);

        let order = [0x1000, 0x2000, 0x3000];
        assert_eq!(evaluate_get_previous_window(0, &order), 0);
        assert_eq!(evaluate_get_previous_window(0x9999, &order), 0);
        assert_eq!(evaluate_get_previous_window(0x1000, &order), 0);
        assert_eq!(evaluate_get_previous_window(0x2000, &order), 0x1000);
        assert_eq!(evaluate_get_previous_window(0x3000, &order), 0x2000);

        // Active
        assert!(!evaluate_is_window_active(0, true, true));
        assert!(evaluate_is_window_active(0x1000, true, false));
        assert!(evaluate_is_window_active(0x1000, false, true));
        assert!(!evaluate_is_window_active(0x1000, false, false));

        // Active non-floating
        assert_eq!(evaluate_active_non_floating_window(&[], |_| true), 0);
        assert_eq!(evaluate_active_non_floating_window(&order, |_| false), 0);
        assert_eq!(evaluate_active_non_floating_window(&order, |w| w == 0x2000), 0x2000);
        assert_eq!(evaluate_active_non_floating_window(&order, |w| w != 0x1000), 0x2000);

        // Modality parameters
        assert_eq!(
            evaluate_get_window_modality_parameters(0, 0x4000, 0x5000, true, true),
            Err(-50)
        );
        assert_eq!(
            evaluate_get_window_modality_parameters(0x1000, 0x4000, 0x5000, false, true),
            Err(-50)
        );
        assert_eq!(
            evaluate_get_window_modality_parameters(0x1000, 0x4000, 0x5000, true, false),
            Err(-50)
        );
        let get_params = evaluate_get_window_modality_parameters(0x1000, 0x4000, 0x5000, true, true).unwrap();
        assert_eq!(get_params.window_ptr(), 0x1000);
        assert_eq!(get_params.out_modal_kind_ptr(), 0x4000);
        assert_eq!(get_params.out_unavailable_window_ptr(), 0x5000);

        // Null out pointers are allowed
        let get_null_params = evaluate_get_window_modality_parameters(0x1000, 0, 0, false, false).unwrap();
        assert_eq!(get_null_params.out_modal_kind_ptr(), 0);
        assert_eq!(get_null_params.out_unavailable_window_ptr(), 0);

        assert_eq!(evaluate_set_window_modality_parameters(0, WINDOW_MODALITY_APP_MODAL, 0), Err(-50));
        assert_eq!(evaluate_set_window_modality_parameters(0x1000, 4, 0), Err(-50));
        let set_params = evaluate_set_window_modality_parameters(0x1000, WINDOW_MODALITY_WINDOW_MODAL, 0x2000).unwrap();
        assert_eq!(set_params.window_ptr(), 0x1000);
        assert_eq!(set_params.modal_kind(), WINDOW_MODALITY_WINDOW_MODAL);
        assert_eq!(set_params.unavailable_window_ptr(), 0x2000);
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

    #[test]
    fn window_property_management_evaluation() {
        use super::*;

        let mut properties = Vec::new();
        let creator = 0x54455354; // 'TEST'
        let tag = 0x54414731; // 'TAG1'
        let data = vec![1, 2, 3, 4, 5, 6, 7, 8];

        // 1. Initial state: property not found
        assert!(get_window_property(&properties, creator, tag).is_none());
        assert!(!remove_window_property(&mut properties, creator, tag));
        assert_eq!(
            change_window_property_attributes(&mut properties, creator, tag, 1, 0),
            Err(WINDOW_PROPERTY_NOT_FOUND_ERR)
        );

        // 2. Set property
        set_window_property(&mut properties, creator, tag, 0x10, data.clone());
        let prop = get_window_property(&properties, creator, tag).unwrap();
        assert_eq!(prop.creator, creator);
        assert_eq!(prop.tag, tag);
        assert_eq!(prop.attributes, 0x10);
        assert_eq!(prop.data, data);

        // 3. Update property data and attributes
        let new_data = vec![10, 20];
        set_window_property(&mut properties, creator, tag, 0x20, new_data.clone());
        let prop = get_window_property(&properties, creator, tag).unwrap();
        assert_eq!(prop.attributes, 0x20);
        assert_eq!(prop.data, new_data);

        // 4. Change attributes: set 0x05, clear 0x20
        let new_attrs = change_window_property_attributes(&mut properties, creator, tag, 0x05, 0x20).unwrap();
        assert_eq!(new_attrs, 0x05);
        let prop = get_window_property(&properties, creator, tag).unwrap();
        assert_eq!(prop.attributes, 0x05);

        // 5. Remove property
        assert!(remove_window_property(&mut properties, creator, tag));
        assert!(get_window_property(&properties, creator, tag).is_none());
        assert!(!remove_window_property(&mut properties, creator, tag));

        // 6. Parameter validations
        // SetWindowPropertyParameters
        assert_eq!(
            evaluate_set_window_property_parameters(0, creator, tag, vec![]),
            Err(-50)
        );
        let set_params = evaluate_set_window_property_parameters(0x1000, creator, tag, vec![9, 8]).unwrap();
        assert_eq!(set_params.window_ptr(), 0x1000);
        assert_eq!(set_params.creator(), creator);
        assert_eq!(set_params.tag(), tag);
        assert_eq!(set_params.data(), &[9, 8]);
        assert_eq!(set_params.into_data(), vec![9, 8]);

        // GetWindowPropertyParameters
        assert_eq!(
            evaluate_get_window_property_parameters(0, creator, tag, 16, 0x2000, 0x3000, true, true),
            Err(-50)
        );
        assert_eq!(
            evaluate_get_window_property_parameters(0x1000, creator, tag, 16, 0x2000, 0x3000, false, true),
            Err(-50)
        );
        assert_eq!(
            evaluate_get_window_property_parameters(0x1000, creator, tag, 16, 0x2000, 0x3000, true, false),
            Err(-50)
        );
        let get_params = evaluate_get_window_property_parameters(0x1000, creator, tag, 16, 0x2000, 0x3000, true, true).unwrap();
        assert_eq!(get_params.window_ptr(), 0x1000);
        assert_eq!(get_params.creator(), creator);
        assert_eq!(get_params.tag(), tag);
        assert_eq!(get_params.buffer_size(), 16);
        assert_eq!(get_params.out_actual_size_ptr(), 0x2000);
        assert_eq!(get_params.out_data_ptr(), 0x3000);

        // GetWindowPropertySizeParameters
        assert_eq!(
            evaluate_get_window_property_size_parameters(0, creator, tag, 0x2000, true),
            Err(-50)
        );
        assert_eq!(
            evaluate_get_window_property_size_parameters(0x1000, creator, tag, 0x2000, false),
            Err(-50)
        );
        let size_params = evaluate_get_window_property_size_parameters(0x1000, creator, tag, 0x2000, true).unwrap();
        assert_eq!(size_params.window_ptr(), 0x1000);
        assert_eq!(size_params.out_actual_size_ptr(), 0x2000);

        // RemoveWindowPropertyParameters
        assert_eq!(
            evaluate_remove_window_property_parameters(0, creator, tag),
            Err(-50)
        );
        let remove_params = evaluate_remove_window_property_parameters(0x1000, creator, tag).unwrap();
        assert_eq!(remove_params.window_ptr(), 0x1000);

        // GetWindowPropertyAttributesParameters
        assert_eq!(
            evaluate_get_window_property_attributes_parameters(0, creator, tag, 0x2000, true),
            Err(-50)
        );
        assert_eq!(
            evaluate_get_window_property_attributes_parameters(0x1000, creator, tag, 0x2000, false),
            Err(-50)
        );
        let get_attr_params = evaluate_get_window_property_attributes_parameters(0x1000, creator, tag, 0x2000, true).unwrap();
        assert_eq!(get_attr_params.window_ptr(), 0x1000);
        assert_eq!(get_attr_params.out_attributes_ptr(), 0x2000);

        // ChangeWindowPropertyAttributesParameters
        assert_eq!(
            evaluate_change_window_property_attributes_parameters(0, creator, tag, 1, 0),
            Err(-50)
        );
        let change_params = evaluate_change_window_property_attributes_parameters(0x1000, creator, tag, 0x10, 0x01).unwrap();
        assert_eq!(change_params.window_ptr(), 0x1000);
        assert_eq!(change_params.to_set(), 0x10);
        assert_eq!(change_params.to_clear(), 0x01);
    }

    #[test]
    fn window_pic_refcon_variant_port_and_low_memory_evaluation() {
        // Window Pic evaluation
        assert_eq!(evaluate_get_window_pic(0, Some(0x2000)), 0);
        assert_eq!(evaluate_get_window_pic(0x1000, None), 0);
        assert_eq!(evaluate_get_window_pic(0x1000, Some(0x2000)), 0x2000);

        assert_eq!(evaluate_set_window_pic_parameters(0, 0x2000, true), Err(-50));
        assert_eq!(evaluate_set_window_pic_parameters(0x1000, 0x2000, false), Err(-50));
        let pic_params = evaluate_set_window_pic_parameters(0x1000, 0x2000, true).unwrap();
        assert_eq!(pic_params.window_ptr(), 0x1000);
        assert_eq!(pic_params.pic_handle(), 0x2000);

        // Window RefCon evaluation
        assert_eq!(evaluate_get_w_ref_con(0, Some(0x3000)), 0);
        assert_eq!(evaluate_get_w_ref_con(0x1000, None), 0);
        assert_eq!(evaluate_get_w_ref_con(0x1000, Some(0x3000)), 0x3000);

        assert_eq!(evaluate_set_w_ref_con_parameters(0, 0x3000, true), Err(-50));
        assert_eq!(evaluate_set_w_ref_con_parameters(0x1000, 0x3000, false), Err(-50));
        let ref_params = evaluate_set_w_ref_con_parameters(0x1000, 0x3000, true).unwrap();
        assert_eq!(ref_params.window_ptr(), 0x1000);
        assert_eq!(ref_params.ref_con(), 0x3000);

        // Window Variant evaluation
        assert_eq!(evaluate_get_w_variant(0, Some(0x12)), 0);
        assert_eq!(evaluate_get_w_variant(0x1000, None), 0);
        assert_eq!(evaluate_get_w_variant(0x1000, Some(0x12)), 0x02);
        assert_eq!(evaluate_get_w_variant(0x1000, Some(0x10)), 0x00);
        assert_eq!(evaluate_get_w_variant(0x1000, Some(0x0F)), 0x0F);

        // Window Port and SetPortWindowPort evaluation
        assert_eq!(evaluate_get_window_port(0x1000), 0x1000);
        assert_eq!(evaluate_set_port_window_port(0x1000), 0x1000);

        // AuxWin evaluation
        assert_eq!(evaluate_get_aux_win_parameters(0, 0x4000, true), Err(-50));
        assert_eq!(evaluate_get_aux_win_parameters(0x1000, 0x4000, false), Err(-50));
        let aux_params = evaluate_get_aux_win_parameters(0x1000, 0x4000, true).unwrap();
        assert_eq!(aux_params.window_ptr(), 0x1000);
        assert_eq!(aux_params.out_aux_ctable_ptr(), 0x4000);
        let aux_nil_params = evaluate_get_aux_win_parameters(0x1000, 0, false).unwrap();
        assert_eq!(aux_nil_params.out_aux_ctable_ptr(), 0);

        assert_eq!(evaluate_get_aux_win(None, false), (0, false));
        assert_eq!(evaluate_get_aux_win(None, true), (0, true));
        assert_eq!(evaluate_get_aux_win(Some(0x5000), false), (0x5000, true));
        assert_eq!(evaluate_get_aux_win(Some(0x5000), true), (0x5000, true));

        // Low memory WindowList evaluation
        assert_eq!(evaluate_lm_get_window_list(None, None), 0);
        assert_eq!(evaluate_lm_get_window_list(None, Some(0x1000)), 0x1000);
        assert_eq!(evaluate_lm_get_window_list(Some(0x2000), Some(0x1000)), 0x2000);
        assert_eq!(evaluate_lm_get_window_list(Some(0), Some(0x1000)), 0x1000);

        assert_eq!(evaluate_lm_set_window_list_parameters(0x1000, false), Err(-50));
        let lm_win_params = evaluate_lm_set_window_list_parameters(0x1000, true).unwrap();
        assert_eq!(lm_win_params.window_ptr(), 0x1000);

        // Low memory AuxWinHead evaluation
        assert_eq!(evaluate_lm_get_aux_win_head(None), 0);
        assert_eq!(evaluate_lm_get_aux_win_head(Some(0x6000)), 0x6000);

        assert_eq!(evaluate_lm_set_aux_win_head_parameters(0x6000, false), Err(-50));
        let lm_aux_params = evaluate_lm_set_aux_win_head_parameters(0x6000, true).unwrap();
        assert_eq!(lm_aux_params.aux_head(), 0x6000);
    }

    #[test]
    fn window_proxy_icon_modified_features_and_structure_widths_evaluation() {
        // Window modified
        assert!(!evaluate_is_window_modified(0, true));
        assert!(!evaluate_is_window_modified(0x1000, false));
        assert!(evaluate_is_window_modified(0x1000, true));

        assert_eq!(evaluate_set_window_modified_parameters(0, true), Err(-50));
        let mod_params = evaluate_set_window_modified_parameters(0x1000, true).unwrap();
        assert_eq!(mod_params.window_ptr(), 0x1000);
        assert!(mod_params.modified());

        // Window proxy icon
        assert_eq!(
            evaluate_get_window_proxy_icon_parameters(0, 0x2000, true),
            Err(-50)
        );
        assert_eq!(
            evaluate_get_window_proxy_icon_parameters(0x1000, 0, true),
            Err(-50)
        );
        assert_eq!(
            evaluate_get_window_proxy_icon_parameters(0x1000, 0x2000, false),
            Err(-50)
        );
        let get_icon_params =
            evaluate_get_window_proxy_icon_parameters(0x1000, 0x2000, true).unwrap();
        assert_eq!(get_icon_params.window_ptr(), 0x1000);
        assert_eq!(get_icon_params.out_icon_ptr(), 0x2000);

        assert_eq!(
            evaluate_set_window_proxy_icon_parameters(0, 0x3000),
            Err(-50)
        );
        let set_icon_params =
            evaluate_set_window_proxy_icon_parameters(0x1000, 0x3000).unwrap();
        assert_eq!(set_icon_params.window_ptr(), 0x1000);
        assert_eq!(set_icon_params.icon(), 0x3000);

        assert_eq!(evaluate_remove_window_proxy_parameters(0), Err(-50));
        let rem_icon_params = evaluate_remove_window_proxy_parameters(0x1000).unwrap();
        assert_eq!(rem_icon_params.window_ptr(), 0x1000);

        // Path select click
        assert!(!evaluate_is_window_path_select_click(0, 0x0100, true));
        assert!(!evaluate_is_window_path_select_click(0x1000, 0, true));
        assert!(!evaluate_is_window_path_select_click(
            0x1000, 0x0100, false
        ));
        assert!(evaluate_is_window_path_select_click(0x1000, 0x0100, true));
        assert!(evaluate_is_window_path_select_click(0x1000, 0x0300, true));

        // Window features
        assert_eq!(
            evaluate_get_window_features_parameters(0, 0x4000, true),
            Err(-50)
        );
        assert_eq!(
            evaluate_get_window_features_parameters(0x1000, 0, true),
            Err(-50)
        );
        assert_eq!(
            evaluate_get_window_features_parameters(0x1000, 0x4000, false),
            Err(-50)
        );
        let feat_params =
            evaluate_get_window_features_parameters(0x1000, 0x4000, true).unwrap();
        assert_eq!(feat_params.window_ptr(), 0x1000);
        assert_eq!(feat_params.out_features_ptr(), 0x4000);
        assert_eq!(evaluate_get_window_features(0), 0);
        assert_eq!(
            evaluate_get_window_features(0x1000),
            WINDOW_FEATURES_STANDARD
        );

        // Window structure widths
        assert_eq!(
            evaluate_get_window_structure_widths_parameters(0, 0x5000, true),
            Err(-50)
        );
        assert_eq!(
            evaluate_get_window_structure_widths_parameters(0x1000, 0, true),
            Err(-50)
        );
        assert_eq!(
            evaluate_get_window_structure_widths_parameters(0x1000, 0x5000, false),
            Err(-50)
        );
        let widths_params =
            evaluate_get_window_structure_widths_parameters(0x1000, 0x5000, true).unwrap();
        assert_eq!(widths_params.window_ptr(), 0x1000);
        assert_eq!(widths_params.out_rect_ptr(), 0x5000);
        assert_eq!(evaluate_get_window_structure_widths(0), (0, 0, 0, 0));
        assert_eq!(
            evaluate_get_window_structure_widths(0x1000),
            WINDOW_STRUCTURE_WIDTHS_STANDARD
        );
    }
}


