# Guest editing after movable dialog drag

Use the separate movable-dialog fixture described in ../movable-dialog-drag.
Add --capture-modeless-dialog-dragged-edited to its drag capture command.
The helper first asserts unchanged text/selection and translated window/item
bounds, then clicks after the displayed first glyph using original-font guest
advances. Real guest mouse events select offset1. Real key events insert z,
producing exact Pzilot and caret2, then wait for completed guest painting.

Four reviewed composed captures pass mono68k/0.75, colour68k/1, PPC8/2 and
PPC16/1.5. Carets are visible at offset2 and no background rectangle crosses
the moved dialog. Build and test compilation pass. This extends the selected
session workflow; it does not prove physical mouse delivery through the GPUI
window or independent Macintosh font fidelity. No release gate closes here.
