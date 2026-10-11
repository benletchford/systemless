# Movable dialog guest drag

The test-only capture drives real guest mouse down/move/up from the displayed
movable-dialog title, then waits for a completed null event. Assertions preserve
window/dialog lifetime, text, values and selection and translate every item and
window bound by +12 vertical/+16 horizontal. The separate resource fixture is
reproducible using the script in ../movable-dialog-title-compositor; the original
committed fixture is unchanged.

`drag` retains the original failed PPC result. `drag-fixed` retains the partial
pixel-transfer fix, which still failed visually. `drag-clipped` contains the
reviewed final captures: mono68k scale0.75, colour68k scale1, PPC8 scale2 and
PPC16 scale1.5. The final PPC captures show neither duplicate original dialog
ink nor the background application's rectangle crossing the dialog. A rejected
explicit depth16 CLI invocation was corrected by using the PPC capture's
native depth16 default; manifests establish actual depths.

The PPC MoveWindow path snapshots visible content before translating its port,
then transfers pixels and drawing detail without copying occluded source or
painting over front windows. FrameRect now respects both clipRgn and visRgn,
preventing background update drawing from crossing a moved dialog.

PPC Window Manager regressions passed104 before the separate FrameRect change.
Final QuickDraw regressions pass118, including an all-depth disjoint clipping
and visibility regression. The example build passes. These results qualify the
selected guest workflow, not physical input, arbitrary custom WDEFs, performance,
independent Macintosh font fidelity or the complete release candidate.
