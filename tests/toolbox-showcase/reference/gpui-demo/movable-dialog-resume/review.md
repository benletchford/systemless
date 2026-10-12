# Movable dialog guest reactivation and editing

Four actual-guest shared Demo captures pass and were reviewed: mono68k/depth1/
scale0.75, colour68k/depth8/scale1, PPC8/scale2 and actual PPC16/scale1.5.
The isolated definition5 fixture and resource patch provenance are unchanged;
its reproduction script lives in ../movable-dialog-title-compositor.

The test-only --capture-modeless-dialog-resumed option requires the existing
modeless capture output and conflicts with the suspended option. Each capture
requests false/true/false/true foreground states through the production session
API. After each owner activation change and completed null event, it verifies
unchanged identity, bounds, text, item values and selection. It then delivers
actual guest z key down/up input, waits for exact zPilot and caret1, and waits
for the completed event/redraw before capturing. Relevant snapshots are refreshed.

Reviewed composed images show smooth active title/dialog text, zPilot in the
field and the PPC caret after z. Application-owned headings remain bitmap ink.
Paired guest frames and actual definition/depth/title/item state manifests are
retained. This is guest/session/shared compositor evidence, not physical host
observer integration, physical IME or independent Macintosh font fidelity.
Drag and broader lifecycle/scale coverage remain open. All release gates remain
open. Reproduce with the separate movable archive and existing modeless capture
command, adding --capture-modeless-dialog-resumed.
