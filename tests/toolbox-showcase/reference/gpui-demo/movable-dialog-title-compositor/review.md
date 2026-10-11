# Shared movable-dialog title compositor

Production source 285cc101; capture build passes (25.03s). Four actual-guest
shared Demo captures succeed and were reviewed: mono68k depth1 scale0.75,
colour68k depth8 scale1, PPC depth8 scale2, PPC depth16 scale1.5. Each manifest
records the actual presented depth. Movable title, static text, checkbox/button
labels and editable text render smoothly with guest placement and clipping.
The inactive background document title is muted. Original application headings
and custom drawing stay guest rendered. No close/zoom control is introduced.
The PPC caret happens to be visible; blink-phase equivalence is not claimed.

The isolated fixture changes only compiled DLOG131 procID from4 to5. Its title
still says Modeless Settings because text is deliberately unchanged. All guest
code, other resources and file metadata use the public fixture's existing
packer. GetNewDialog/ShowWindow/DrawDialog and the existing Options menu open it
through guest Toolbox/event paths. The original committed fixture is unchanged.
Resource and executable hashes plus the exact patch location are recorded.

These active captures establish four selected CPU/depth/scale checkpoints, not
all scales/states. Suspended movable-dialog captures, guest title drag/activation,
lifecycle, physical host interaction and independent Macintosh fidelity remain
open. The full production goal and all release gates remain open.

Reproduction uses the ordinary public fixture build (`tests/toolbox-showcase/build.sh`),
then runs `make-variant.py` with `build/showcase.rsrc` and a separate output fork.
Pass `build/showcase`, that fork and a separate `.sit` output to
`cargo run --locked --manifest-path tests/toolbox-showcase/packer/Cargo.toml -- DATA RESOURCE ARCHIVE`.
The retained patch script reproduced the captured resource fork byte for byte.
Run the archive through the example with `--capture-modeless-dialog OUTPUT`
and `--capture-scale SCALE`: `--screen-depth 1` for mono68k,
`--screen-depth 8` for colour68k, `--prefer-powerpc --screen-depth 8` for PPC8,
and `--prefer-powerpc` without a depth override for PPC16. Inspect the resulting
actual-depth manifest rather than inferring depth from the requested arguments.
