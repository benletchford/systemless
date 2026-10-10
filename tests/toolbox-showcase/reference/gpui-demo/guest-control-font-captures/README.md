# Guest-owned control font captures

These four shared Demo captures use actual guest Option-F keyboard input and
SetControlFontStyle calls in the rebuilt fat Toolbox Showcase fixture. The
capture asserts flags7, family3 (Geneva), size18 and bold face on the two
Controls-page targets or three difficulty radio targets. Visible control
identities, bounds and values remain unchanged. Other preferences controls
intentionally retain their original fonts.

Modes: mono68k scale0.75, colour68k scale1.5, PPC8 scale1 and PPC16 scale2.
Actual display provenance is in each capture sidecar. All four commands exited
successfully. Colour68k and PPC16 images were visually inspected. This is a
selected raster/state checkpoint, not a complete scale, inactive or interaction
qualification, and not an independent Macintosh font oracle.

The archive used is currently the ignored build/toolbox-showcase.sit, built
from the recorded source hashes with public MPW tooling and AppearanceLib.
The committed default archive is deliberately unchanged while an older pinned
full regression run is still using it. Rebuild using tests/toolbox-showcase/build.sh;
pass the resulting build archive to the example with --capture-control-fonts
or --capture-radio-fonts. PPC16 is the capture default with --prefer-powerpc;
PPC8 requires --screen-depth 8 (as two separate arguments).

PPC radio labels clip at the original guest bounds. White control backgrounds
against the custom grey preferences panel remain a fidelity question requiring
separate investigation. These captures do not qualify that paint choice.

## Styled title input checkpoint

A subsequent PPC16 scale1.5 capture sends real guest mouse down/up within the
styled checkbox title (30 guest pixels from the left edge). The guest toggles
value0 to1 while preserving generation, bounds and font record. The command
exits successfully and the selected result is composed by Demo. This is guest
input evidence, not physical host pointer delivery or a GPUI hit-test oracle.
All five captures were regenerated successfully from one final executable;
source/executable hashes are in source-sha256.json.

Comparing the actual PPC16 guest framebuffer with its composed radio image
confirms a defect: GPUI paints white control rectangles over the guest grey
preferences panel. The source is the unconditional theme.background in the
standard-control overlay. A fix must retain the guest-owned background without
leaving the old bitmap title underneath the smoothed glyphs; merely making the
overlay transparent would double-paint text. This remains open.
