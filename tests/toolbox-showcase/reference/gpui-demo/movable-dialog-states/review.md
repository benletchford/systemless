# Shared movable-dialog active/suspended states

Eight actual-guest shared Demo captures succeed and were visually reviewed:
active and guest-suspended pairs for mono68k/depth1/scale0.75,
colour68k/depth8/scale1, PPC8/scale2 and actual PPC16/scale1.5.
The isolated DLOG131 definition5 fixture is reproduced by the script under
`../movable-dialog-title-compositor`; original public guest code is unchanged.

The test-only `--capture-modeless-dialog-suspended` option requires
`--capture-modeless-dialog`. It requests foreground loss through the existing
session API, waits for guest owner deactivation and a null event, and checks
unchanged identity, bounds, item text/value/selection. It refreshes window,
dialog and control snapshots before painting. Detailed dialog-state sidecars
record actual definition/depth, identity, window, title positions/clips and
item contents. Separate active/suspended processes also match identity,
title/structure bounds, title position/clip and all item contents in each mode.

Reviewed images show the title muting and background change on suspend; PPC
caret ink disappears. Font placement, text and field geometry remain stable.
Buttons lose active appearance; application-drawn headings remain native.
These checks do not establish cross-CPU blink-phase equivalence.

Final capture build passes (6.06s), example test compilation passes (7.02s).
Production rendering is unchanged; this extends the capture helper and evidence.
Guest drag, reactivation/lifecycle, every-scale coverage, physical host observer
integration and independent Macintosh font fidelity remain open. All production
gates remain open. To reproduce, use the separate movable archive with the
existing modeless capture command; add the new option only for suspended states.
