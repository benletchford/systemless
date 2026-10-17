# Complete wgpu text rendering

This is an explicit full-rebuild acceptance requirement added by the user on
2026-10-16. It extends the original objective; it does not replace resource-fork,
CODE/PEF, game compatibility, mixed-mode or other rebuild requirements. The goal
must remain active until this requirement and the rest of the rebuild are verified.

All supported guest text must draw through the shared wgpu graphics system:
application text, TextEdit, menus, dialogs, controls and window decorations. CPU
font decoding, measurement, layout and source-glyph preparation may remain; all
destination pixel operations and composition must use wgpu. Native host text APIs
must not become a second production renderer.

## Current evidence and remaining work

`font.rs` parses guest FONT/NFNT data. `text.rs` prepares ordered classic cells;
`text_gpu.rs` now prepares binary masks through Glyphon and composes them through
the shared GPU port service. Menus retain specialized GPU coverage runs.
TextEdit, controls and window titles use shared strike drawing. CPU ab_glyph
source preparation remains. The Glyphon integration gate at 4d3b48902 passed
167 unique tests plus the independent 40-case probe. Full API/style/UI coverage, representative
performance and native/browser acceptance remain open; the goal stays active.

## Evaluate libraries before extending the custom path

Initial upstream evaluation on 2026-10-16 (historical observations; the later
production integration below pins Glyphon 0.12.0):

- [Glyphon manifest](https://github.com/grovesNL/glyphon/blob/main/Cargo.toml):
  upstream main declares 0.12.0, wgpu 30 and cosmic-text 0.19. This matches the
  rebuild's wgpu major. Its [README](https://github.com/grovesNL/glyphon/blob/main/README.md)
  describes shaping/rasterization through cosmic-text, glyph atlas packing through
  etagere, and rendering through wgpu. Its
  [custom glyph API](https://github.com/grovesNL/glyphon/blob/main/src/custom_glyph.rs)
  accepts single-channel masks and physical-pixel snapping. Evaluate it first for
  bitmap-strike adapters and batching, plus modern fallback text. Dependency/MSRV,
  cache invalidation, exact masks and QuickDraw transfer semantics require adapter
  work. The isolated prototype below verified the first API fit before the later
  production dependency and adapter were adopted. The current production status
  is recorded above and in the integration section below.
- [Vello architecture](https://github.com/linebender/vello/blob/main/ARCHITECTURE.md):
  current Vello GPU preprocesses geometry on the CPU and rasterizes/composites on
  the GPU, with text/glyph runs through glifo. The original compute renderer is
  now described as experimental research. Evaluate the current wgpu-backed family
  for outline/fallback glyphs if it fits the compatibility and memory requirements.
  Do not adopt its CPU renderer or WebGL backend as production alternatives.
- Compare both against the current exact mask/coverage path. Reuse a library where
  it improves the result, while keeping classic guest semantics in shared runtime
  services. Record the concrete API fit, versions, licenses, dependency cost,
  performance and any adapter work before choosing or rejecting a library.

These initial observations preceded production selection. The pinned-release
prototype and subsequent production integration below provide separate executable
evidence. Future library changes still require pinned versions and verification
against primary documentation.

## Acceptance

1. Audit every guest text entry point and every UI text caller. Implement remaining
   text APIs/styles/spacing deliberately and route their drawing through wgpu.
2. Preserve original bitmap fonts, MacRoman mapping, missing glyphs, bearings,
   advances, style effects and guest-visible measurements. Modern shaping or
   antialiasing must preserve the behavior required by the selected guest font.
3. Preserve port origin, clipping/regions, foreground/background colors, opaque
   spaces and classic transfer operations, including XOR, on packed indexed
   destinations and the production display formats.
4. Keep guest pixel memory coherent across direct guest writes and GPU text draws.
   Font edits, handle movement/replacement and font selection changes must invalidate
   cached glyphs/layout correctly. Both CPU adapters use one font/text state.
5. Verify equivalent 68K/PPC outcomes with deterministic fixtures and GPU readback,
   including low-bit/packed pixels, clipping, styles, spaces, edited font bytes and
   mixed text/QuickDraw operations. Exercise application UI text in real originals.
6. Verify native presentation and a portable browser design with explicit platform
   evidence. Unavailable GPU/browser checks remain unverified, never passed.
7. Measure representative text-heavy workloads before/after any library migration;
   include uploads, dispatches, cache/memory growth and synchronization costs.

The production strike integration and entry-point audit are recorded below.
Next are remaining API/style/UI cases and representative workloads, alongside
Marathon/Escape Velocity compatibility.


## Baseline GPU verification (2026-10-16)

The existing offscreen text fixture passes on a real adapter, covering glyph
transfer modes, clipping and port preservation. Evidence:
/tmp/systemless-getctable-text-gpu.txt, terminal exit 0. The same run passes the
real device PixMap CopyBits/readback check. This proves the currently supported
GPU text slice; API/UI completeness, library prototype/selection, all formats and
styles, native presentation and browser verification remain open.


## Executable Glyphon evaluation (2026-10-16)

[The isolated probe](../prototypes/glyphon/README.md) pins published Glyphon 0.12.0
and wgpu 30.0.1, with a separate lockfile and no production dependency changes.
It uses the shared classic FONT decoder, an empty host-font database and COSMIC
Text's new_empty buffer. On M1/Metal, six exact-readback cases pass: bitmap mask
and snapping, cache reuse, partial clipping, stale-key behavior after a source FONT
edit, refresh with a new glyph ID, and stock composition's failure to perform XOR.
The last two behaviors establish necessary adapter work, not completed compatibility.
Rust 1.93, strict Rust 1.99 lint and the dependency-license gate pass. Evidence and
repeatable commands are in the probe README. The test is an isolated one-glyph
coverage experiment, not an integrated text API, native presentation, browser,
all-font or performance acceptance result.

Next: tie bounded glyph cache identities to source font generations and feed GPU
coverage into exact GPU QuickDraw transfer/composition for real guest calls. Keep
classic metrics in the shared runtime. Measure complete workloads before selecting
Glyphon for production. Vello remains an alternative under consideration.


## GPU transfer bridge (2026-10-16)

The shared runtime now accepts same-device binary text-mask textures through
Ports::copy_gpu_mask and its existing packed-surface/Blit pipeline. The real
Glyphon executable feeds its GPU texture directly into this path. All eight
monochrome transfers, double-XOR cancellation and guest-write coherence pass;
the combined probe has sixteen cases, including the earlier two library-gap
characterizations. The indexed fixture verifies equivalence to existing CopyBits
across 2/4/8-bit targets, colors, negative bounds, clips and packed storage guards.

The transfer boundary is implemented; full library text integration is not yet
complete. Next: bounded source-generation cache IDs, text cells/metrics/pen
updates and actual 68K/PPC text/UI entry points, followed by workload and platform
acceptance. Current production text calls continue their existing GPU coverage
path until that integration is tested.


## Bounded Glyphon content identities (2026-10-16)

The isolated probe now owns a bounded binary-mask identity cache. Exact source
bytes and dimensions define identity, so font edits receive a fresh ID while
unchanged masks reuse uploads. Entry and byte limits trigger a complete atlas
reset before any ID is reused. Invalid masks are rejected before cache mutation.
Placement and font metrics remain outside the cache. The probe deliberately uses
a two-entry cache to exercise rollover on real M1/Metal GPU output.

Validation: two cache tests pass in /tmp/systemless-glyph-cache-tests.txt and all
21 GPU probe cases pass in /tmp/systemless-glyph-cache-gpu.txt (both terminal exit
0). Five new GPU cases cover original/edited source, content reuse, rollover and
restoring the original. The existing transfer and guest-write checks still pass.
This adapter prepares one cell at a time; batching needs batch-wide identities to
avoid resetting an atlas containing already-prepared cells. Linear lookup is a
bounded prototype choice, not a measured production performance decision.
Actual DrawString/TextEdit/UI integration, ordered overlapping glyph cells,
workload measurements and platform acceptance remain open. No production
library dependency has been added.


## Shared ordered glyph cells (2026-10-16)

The production text coverage renderer and the isolated Glyphon adapter now use
text::plan_cells for source masks, cell bounds and final pen position. Cells
include blank spacing, preserve negative bearings and remain ordered so adapters
can preserve opaque overlap and XOR. The existing coverage planner derives its
row runs from these cells; there is no duplicate library layout algorithm.

All five text integration tests pass, including the real GPU transfer/clip/port
check and a new explicit geometry/pen/overflow fixture:
/tmp/systemless-text-cells.txt, terminal exit 0. All 24 Glyphon GPU probe cases
pass, including two shared cells and restoring the transfer source:
/tmp/systemless-shared-cells-probe.txt, terminal exit 0. Strict all-target Clippy
passes in /tmp/systemless-shared-cells-lint.txt. Sequential formatting and diff
whitespace checks pass. These checks do not establish full text API/UI or platform
acceptance. Next: ordered library cell transfers through actual guest text calls,
followed by representative workload measurements before production selection.


## Ordered library string transfers (2026-10-16)

The Glyphon probe now transfers each shared cell in order into guest memory,
using Ports::copy_gpu_mask_region to select the active cell inside a reusable
GPU render target. All eight transfers of an overlapping three-character string
match the production draw_strike path byte-for-byte, with matching final pen
positions. The string path neither allocates a glyph readback buffer nor reads
its mask back to the CPU; existing destination readback maintains guest coherence.
This verifies the cell ordering/transfer bridge, not native guest ABI dispatch
through Glyphon. Production DrawString/TextEdit/UI still use the coverage backend.

All 32 GPU probe cases pass on M1/Metal in
/tmp/systemless-ordered-text-final.txt; strict all-target lint passes in
/tmp/systemless-ordered-text-lint.txt (both terminal exit 0). Earlier
/tmp/systemless-ordered-text-probe.txt and /tmp/systemless-ordered-text-final.txt
runs before the allocation cleanup are superseded by the final output. No
production dependency changed. Next: compare complete workloads and integrate
selected library rendering through runtime-owned text state, preserving the
existing ABI/service boundary and all indexed formats and clipping behavior.


## Synchronized text microbenchmark (2026-10-16)

Run `cargo run --release --manifest-path future/prototypes/glyphon/Cargo.toml
--locked -- --benchmark` from the repository root. The optional benchmark compares
complete source-cell planning/drawing and synchronous guest destination updates,
with equivalent MoveTo and final pen writes. Each sample draws 16 strings, in
three rounds with alternating backend order, in copy and XOR modes. Workloads
are synthetic FONT-backed AAA (overlapping bearings, 48 cells/sample) and AB?A
(missing-glyph fallback, 64 cells/sample). Pixels match after every sample.

Final optimized M1/Metal measurements, after other builds/runs finished:

| Workload | Mode | Coverage median ms | Glyphon median ms |
|---|---|---:|---:|
| overlap | 0 | 41.695 | 35.623 |
| overlap | 2 | 48.159 | 31.367 |
| missing | 0 | 85.497 | 60.366 |
| missing | 2 | 56.870 | 42.437 |

Times are per 16-string sample, not frame rates. All 24 timed samples finish;
all 32 preceding GPU assertions pass. Warm Glyphon rasterization callbacks remain
zero. The prototype cache retains at most two masks / 1024 source bytes; this is
not an atlas or total host/GPU memory measurement. For these nonempty-ink/paper
cells, coverage invokes two GPU paints per cell; Glyphon invokes one render plus
one GPU transfer. Each destination operation synchronizes guest memory. These
operation counts follow the exercised code, not hardware profiling counters.

Evidence: /tmp/systemless-text-workload-release-final.txt terminal exit 0;
strict all-target lint /tmp/systemless-text-workload-lint-final.txt exit 0.
The debug run and first release run overlapped other work and are preliminary,
not additional acceptance or uncontended performance evidence.

Glyphon is faster in this bounded comparison, supporting integration work. This
is not representative application performance: target is 16x12 monochrome, font
is synthetic, and there is no native/browser presentation. Wider real fonts,
indexed formats, editing/scrolling/UI workloads, atlas memory growth, cold uploads
and dispatch/synchronization profiling remain before production selection.


## Editable dialog integration (2026-10-17)

DITL editText items now bind their canonical text handles to one shared TextEdit
record per dialog. Native/PPC selection, active SetDialogItemText, selection/caret
and DrawDialog use existing wgpu destination operations. Real GPU dialog tests
cover field switching and selection; original EV visibly draws its New Pilot
dialog with the pilot name selected before stopping at missing ModalDialog.
This extends shared UI text coverage; it does not complete modal editing events,
all text APIs/styles, Glyphon production integration or native/browser acceptance.
Full scope and library evaluation above remain required.


## Modal input through shared text (2026-10-17)

Original EV now accepts real host keyboard entry through ModalDialog and TextEdit,
reads back pilot name ben, and accepts the shuttle-name dialog. Text mutations,
selection/caret and control feedback use existing wgpu services. Native/PPC
fixtures also execute real guest modal filters and timer callbacks around a
paused dialog. This extends event-driven GPU text coverage; editable-field mouse
selection, full DialogSelect/edit commands, complete styles/APIs, library
production integration and native/browser acceptance remain unfinished.


## Original EV drawing context and next font boundary (2026-10-17)

BeginUpdate now preserves the caller's selected port. Original EV's picture and
text calls remain in its selected offscreen buffer, then CopyBits presents the
visible main menu with saved pilot/ship details. Before the fix, BeginUpdate
selected the window port and a later black-buffer copy erased the rendered text
and picture. This was shared drawing-context ownership, not a final wgpu upload
failure. Native/PPC real-GPU offscreen fixtures and window/dialog regressions pass.

Real E-key Enter Ship input advances the original game to TextFont2020,size14,
then StringWidth stops at the legacy FONT family*128+size ID overflow guard.
The original application actually contains FOND2020 with one scalable association
(size0,style0,sfntID9295), plus the17000-byte sfnt9295 resource. FOND offsets:
width78,kern0,style598. At that checkpoint the next text slice was correct FOND/outline-resource
resolution and actual measurements, not forcing a small family ID or substituting
an unrelated font to bypass overflow. Verify existing library APIs and reuse a
suitable decoder/rasterizer while retaining shared metrics and wgpu destination
composition. Private original assets remain uncommitted. Full styles, classic
metrics, library production integration and native/browser acceptance stay open.


## Guest FOND and sfnt source preparation (2026-10-17)

Shared selection now resolves FOND associations before legacy FONT IDs, including
high family IDs, arbitrary signed resource IDs, scalable sfnt entries and
exact-size monochrome NFNT/FONT entries. Font initialization uses the same lookup.
Malformed or unsupported selected families return errors instead of substituting
an unrelated font. Intrinsic plain/bold faces precede synthesis from plain.

The existing Apache-2.0 ab_glyph0.2.32 dependency supplies FontRef, em conversion,
MacRoman-to-Unicode glyph lookup and outline coverage. Its pinned implementation
and manifest were inspected locally. No new production dependency or library
version is added. This extends source decoding; it is separate from the unfinished
Glyphon production integration and keeps all destination operations in wgpu.

Family width-table layout is checked against
[Apple's family-width documentation](https://leopard-adc.pepas.com/documentation/mac/Text/Text-274.html)
and [Adobe technical note0091, section5](https://adobe-type-tools.github.io/font-tech-notes/pdfs/0091.Mac_Fond.pdf):
style word, per-character 4.12 widths, missing-character width and unused zero
word. Active tables provide rounded integer advances; absent/disabled tables use
sfnt horizontal metrics. Counts, character ranges, offsets and overlap with the
kerning/style tables are checked. Derived strikes use sfnt vertical metrics and
source bounds. Full FOND global-metric parity and fractional width/style policies
remain open rather than being claimed from integer-width tests.

Prepared outline strikes are cached by exact live FOND+sfnt bytes, size and
intrinsic style, bounded to eight entries/8MiB including keys. Subsequent resource
edits are observed before reuse. Fallback strikes keep their existing editable
guest handles. Glyph/metric/storage limits apply to guest font input. Current
outline source preparation covers printable MacRoman mappings and the missing
glyph, sizes1..96, unhinted binary coverage. Classic hinting, non-Unicode cmap
fonts, bitmap scaling, full styles and native glyph parity remain acceptance work.


Application evidence: original EV1.0.5 now completes TextFont2020,size14,
StringWidth and DrawString for its ship HUD's 'No Target' label, through shared
wgpu composition. Real E-key EnterShip input stops next at FrameOval/A8B7,
CPU8,218,226,PC0101FBB6; flight remains unverified. Scoped regression result:
104 passes (88 core,11 fonts,5 text),0 ignored, real GPU cases enabled, including
embedded-font DrawString pixel equivalence through both CPU adapters. See the
2026-10-17 embedded-font milestone in progress.md for exact private evidence and
unchanged saved-volume verification. This does not complete the library/text
acceptance checklist above.

Cross-map font-suitcase association priority remains unverified; the original EV
font evidence uses FOND and sfnt resources from the same application fork.


The following shared oval milestone advances original EV beyond the embedded-font
HUD stage: two FrameOval calls complete and more HUD labels/bars are visible.
The next stop is SwapMMUMode/A05D,CPU8,572,693,PC00019304. Font semantics and the
full acceptance checklist above remain unchanged and unfinished. See progress.md
for fresh EV/Marathon evidence and the scoped geometry/GPU verification.


The addressing/startup milestone now advances EV past the mode-switch point to
Munger/A9E0,CPU15,136,642,PC0102C7CA. Its snapshot includes planet/station imagery;
font rendering remains the same supported slice. Correctness/playability and the
full text checklist remain unverified. Marathon's initialization port remains nil
in fresh read-only watch evidence. See progress.md for the149 scoped checks and
exact game snapshots; this does not complete the text or full rebuild goal.

### Fixed-width bundled source, 2026-10-17

Marathon's original GetFontInfo requests Monaco4,bold,size9 without embedded font
resources. The existing ab_glyph source converter now accepts a distinct bundled
Nimbus Mono PS source for families4/22, with the same preserved OFL asset/provenance
as the baseline. No new library is added. Source identity and point size key the
bounded32-strike guest-handle cache, so proportional and fixed-width requests
cannot collide; aliases for one source share live guest bytes. Guest FOND/sfnt
and FONT resolution still precede bundled substitutes.

Source decoding, Mac Roman mapping and monochrome mask preparation remain CPU
work. Destination transfer, clipping and composition stay in the existing wgpu
text path. This adds no independent renderer and does not constitute Glyphon
production integration. Nimbus Mono glyph advances/metrics and synthetic bold
are explicit substitutes; classic Monaco hinting, artwork and layout equivalence
remain unverified. The preserved historical widMax override is not imported.


## TextEdit color-port destination (2026-10-17)

Original EV reaches TextBox after13Munger calls, then faults because redraw reads
CGrafPort.portPixMap as a basic BitMap. Redraw now resolves its destination through
the existing shared port_layout::bitmap service. This preserves the actual indexed
depth, palette, bounds and image pointer; drawing still uses shared wgpu coverage
operations. No application-specific geometry or CPU destination renderer is added.

The real GPU regression constructs an eight-bit offscreen CGrafPort with a live
PixMap/ColorTable and attached PixPat handles, then verifies exact TextBox pixels,
untouched row padding, restored port/current-slot state and restored pen pattern.
All17TextEdit tests pass,0ignored,including existing mono drawing, selection,
caret, erasure, complex clipping, fault restoration and real CPU adapter tests.
The new indexed pixel fixture exercises the common service directly; it is not
an independent indexed-pixel replay through both ABI adapters or native parity.
Full text API/style/font/library and native/browser acceptance remain open.

## Direct-colour guest text destinations (2026-10-17)

The shared Surface/port path now carries full RGB555/xRGB8888 pixel values.
The real-GPU direct-destination fixture passes through native DrawChar and PPC
DrawChar imports using guest FONT393. Independently specified ink/paper pixels,
negative bounds, bearing-1, advance4, requested foreground/background RGB,
packed storage, padding and double-XOR restoration are checked for both formats.
Read-only destination rejection preserves port/pixel/GPU state in the same fixture.
The existing source/font planner and GPU coverage renderer remain shared; CPU work
only prepares glyphs and encodes storage.

Evidence: /tmp/systemless-direct-destination-extended.txt, terminal exit0.
This extends supported destination-format acceptance, not all-text completion.
Styles, the full API/UI audit, production Glyphon integration and direct-target
library-mask fixtures, workload measurements and native/browser acceptance remain
open. Keep the whole rebuild goal active.


## Production Glyphon strike integration (2026-10-17)

The shared draw_strike implementation now prepares the existing ordered classic
cells with pinned Glyphon 0.12.0. Application DrawChar/DrawString/DrawText,
TextEdit, control labels and window titles reach this path through their existing
services. Menus retain specialized GPU coverage drawing; no host destination
text renderer is introduced. Guest font decoding and metrics remain shared.

Graphics owns one lazily initialized, same-device renderer, shared by scratch
targets. An empty font database and empty shaping buffer prevent host font
substitution. Content identities include exact binary bytes and cell dimensions;
256 entries / 1,000,000 source bytes trigger a full atlas/renderer reset before ID
reuse. This source bound is not a total GPU-atlas-memory measurement. A reusable
RGBA mask target holds at most1,000,000 texels. Only the active cell rectangle is
transferred, in order, through the existing GPU QuickDraw compositor; masks are
never read back to the CPU. Destination readback still synchronizes guest memory.
Invalid/oversized cell geometry is checked for the complete string before drawing.

The independent prototype retains its former monochrome coverage comparator,
now local to that executable, because production draw_strike uses Glyphon.
The production dependency-license gate passes with future/deny.toml. The declared Rust 1.93 check, final strict scoped lint and license gate pass.
The scoped run passes 167 unique tests. Final expanded all-mode direct-color and
257-mask cache-rollover fixtures pass separately, superseding their earlier
instances without duplicate counts. All 40 independent probe assertions and 36
debug workload samples match pixels. Fresh unchanged originals execute five
Marathon and 36 EV TextDraw calls; screen and saved-volume hashes match prior
rebuild captures. Independent volume checks pass. Neither original is verified
playable. Exact commands/logs and boundaries are in progress.md and
original-applications.md. Historical probe
measurements above do not establish production integration performance. Full
styles/API/UI coverage, representative workload and atlas-memory measurement,
original-application replays, native/browser acceptance and remaining rebuild
requirements remain open; do not mark the goal complete.


### Current entry-point audit

Source audit for this integration (not an exhaustive classic Toolbox inventory):

| Entry/caller | Shared route and current destination backend |
|---|---|
| 68K DrawChar/DrawString/DrawText traps; PPC imports | ABI TextDraw → runtime → text::draw → draw_strike → Glyphon mask → GPU QuickDraw transfer |
| CharWidth/StringWidth/TextWidth/GetFontInfo | Same guest font resolution/metrics; no destination drawing |
| TextFont/TextFace/TextMode/TextSize | Shared port selection; drawing observes live fields |
| TextEdit redraw, TextBox, dialog static/edit text | Shared TextEdit → draw_strike; selection, caret and erasure use GPU geometry |
| Controls and window titles | draw_strike; remaining decoration geometry uses GPU paint |
| Standard File labels | text::draw |
| Menu titles and tracked menu items | Shared cell-to-coverage planner → specialized GPU composition; not Glyphon |

Explicit gaps found in current sources include fractional space extra, non-bold
synthetic styles, custom text/measurement and TextEdit callbacks, mixed-style and
arbitrary colored TextEdit redraw, and picture-recorded text. Neither StdText nor
StdTxMeas, MeasureText or SpaceExtra has a named PPC import mapping in the current
ABI table. Absence of a named mapping is source evidence of a gap, not a native
behavior oracle. StyledLineBreak and TextRatio also need explicit compatibility
decisions. Full API/style acceptance must resolve these deliberately; this audit
does not declare them unnecessary because selected fixtures do not call them.


### Library decision and dependency cost

Glyphon 0.12.0 is selected for the current binary-strike integration because its
same-device custom-mask path has executable pixel evidence, while classic
selection/layout and transfer semantics stay shared. The production lockfile adds
29 package/version pairs and removes none; existing versions are preserved. This
includes COSMIC Text 0.19.0, font parsing and rasterization packages,
etagere 0.3.0 and lru 0.16.4. The pinned published Glyphon manifest declares
MIT OR Apache-2.0 OR Zlib; the whole rebuild license gate passes. Prior optimized
probe timings support integration work, but representative production performance
and total atlas memory remain unverified.

Vello was reconsidered against primary upstream main on 2026-10-17:
[workspace manifest](https://github.com/linebender/vello/blob/main/Cargo.toml),
[GPU manifest](https://github.com/linebender/vello/blob/main/vello_gpu/Cargo.toml),
[Glifo manifest](https://github.com/linebender/vello/blob/main/glifo/Cargo.toml),
[GPU documentation](https://github.com/linebender/vello/blob/main/vello_gpu/README.md).
Main declares vello_gpu 0.3.0, glifo 0.4.0, Rust 1.89, Apache-2.0 OR MIT and wgpu 30;
these are repository declarations, not a verified published dependency selection.
The GPU feature supplies rasterization/composition; text enables Glifo. Its vector
pipeline and experimental glyph caching would need an independently verified
classic-mask/transfer adapter. The documentation lists unsupported mask layers,
resource-lifecycle limitations and some panic paths. These are integration risks,
not proof it cannot preserve classic pixels. No compiled Vello dependency graph,
classic pixel comparison or workload measurement has been performed. It remains
an outline/vector alternative; it is not added alongside the proven Glyphon mask
adapter merely to provide a second renderer. The CPU and WebGL backends are not
selected for production. This decision does not claim Glyphon is universally
faster or that Vello's performance/compatibility was tested.


## Application coordinate arithmetic follow-up (2026-10-17)

A subsequent shared SANE zero-addition correction preserves original EV's actual
nonzero drawing coordinates; it does not change Glyphon, font metrics or source
masks. The original scene now uses nonzero positions, and isolated right/up input
shows centered-ship turning and world scrolling. This is bounded headless game
interaction evidence, not full text/font parity or native/browser/performance
acceptance. Full text API/style/UI and representative workload requirements
remain open. See the exact arithmetic/control milestone in progress.md and
original-applications.md; the whole rebuild goal remains active.


The subsequent native-32-bit entry milestone advances original Marathon to a
textured 3D demo through genuine managed-world services. This does not change the
shared Glyphon text path or complete text/API/style/native/browser acceptance.
See the latest original-applications.md/progress.md evidence; the goal stays active.
