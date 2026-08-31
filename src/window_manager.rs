//! Architecture-neutral Window Manager ordering operations.

/// The standard QuickDraw 50% gray desktop pattern.
pub(crate) const STANDARD_DESKTOP_PATTERN: [u8; 8] =
    [0xAA, 0x55, 0xAA, 0x55, 0xAA, 0x55, 0xAA, 0x55];

pub(crate) fn standard_desktop_pattern_is_ink(h: i32, v: i32) -> bool {
    let row = STANDARD_DESKTOP_PATTERN[v.rem_euclid(8) as usize];
    row & (0x80 >> h.rem_euclid(8)) != 0
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
}
