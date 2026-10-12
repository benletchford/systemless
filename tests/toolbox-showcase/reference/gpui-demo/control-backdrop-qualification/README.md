# Control backdrop qualification checkpoint

Working source based on e786c949. Actual guest Option-F applies Geneva18 bold
through SetControlFontStyle. These captures use the shared Demo compositor and
the separately rebuilt fat build/toolbox-showcase.sit; the default committed
archive is unchanged while the older full regression is running.

Commands use --capture-radio-fonts, with --screen-depth 1 scale0.75 for mono68k,
--screen-depth 8 scale1.5 for colour68k, --prefer-powerpc --screen-depth 8 scale1
for PPC8, and --prefer-powerpc scale2 (default PPC16). Spell each depth option
and value as separate CLI arguments. All four commands exit successfully;
actual display provenance is in their sidecars. The final executable and source
hashes accompany this checkpoint.

PPC16 visual review confirms grey panel preservation without white replacement
rectangles or texture-filtering outlines. A radio crossing the custom panel
border retains guest rendering when its backdrop is uncertain. Original text,
font overrides, bounds and values remain guest-owned. This is one scene and
scale per mode, not full input/inactive/style/native-oracle qualification.

Mono68k visual review reveals black guest panels and invisible original guest
black labels; the guest screenshot confirms that underlying behavior. Do not
invent contrasting host text as a font fidelity fix. PPC8 shows a remaining
shade difference between some control backdrops and the surrounding panel;
its indexed/native presentation path needs further investigation. These modes
are execution checkpoints, not established backdrop fidelity passes.

The 18 current frame/ownership model tests pass, including BGRA colour ordering,
clipping, incomplete backdrop rejection and hard-break caret expectations
reconciled with the existing production fix 90f1175c. The real classic draw
regression passes on the earlier 68k capture implementation, preserving an
original white backdrop across font/style changes and reset; it predates the
stricter partial-repaint model and base-texture changes. Do not treat that
pinned earlier test as final-candidate coverage. Production compilation and the
current capture executable build pass. All production gates remain open.

## Later integration checkpoints

The actual classic control-font drawing regression passes again (one test,
0.15s after 1m43 compilation). A later public library check passes (40.24s).
Both commands began before the font-invalidation change; they establish the
backdrop/gamma implementation checkpoint, not verification of that later change.
A current frontend build and actual guest interaction checks are pending.
