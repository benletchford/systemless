# Ordinary guest root Tab routing

GPUI Base Root resolves Tab bindings before raw key capture. The window-scoped interceptor now forwards Tab and Shift-Tab when the guest root has focus, as well as retaining existing owned-volume-popup and notification boundaries. Host menu/control focus remains outside the ordinary root route. The expanded Standard File pointer/keyboard regression passes3.30s, checking two ordinary-root Tab deliveries, root focus retention, no semantic file activation, existing popup delivery and inactive exclusion.

This is synthetic GPUI test-window evidence for the shared root route. Actual guest editor tab behavior, broader host focus transitions and physical keyboard/accessibility qualification remain open. No production gate closes.

The additional actual-session editor test passes6.73s across mono68k, colour68k, PPC8 and PPC16. It renders GPUI with an active plain guest TextEdit snapshot, presses Tab, verifies exactly one Macintosh key-down and root focus retention, then delivers the resulting input to the guest. TextEdit replaces the current selection with one tab byte and advances selection/caret once. This establishes that selected guest editing workflow; physical keyboard integration and broader focus transitions remain open.

All five notification regressions pass29.50s after the route change. The unchanged-feature guard on the older geometry test was restored after the editor test insertion; production routing is unchanged between the editor pass and this rerun.
