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

`font.rs` parses guest FONT/NFNT data. `text.rs` prepares disjoint glyph ink and
background coverage and sends it through the same GPU port drawing services.
`textedit.rs` uses shared font/graphics behavior. `font_fallback.rs` currently
rasterizes fallback outlines on the CPU with ab_glyph to prepare packed source
strikes. These facts establish an existing GPU destination path, not completion
of all text APIs, styles, fallback behavior or the library evaluation below.

## Evaluate libraries before extending the custom path

Primary upstream sources inspected on 2026-10-16:

- [Glyphon manifest](https://github.com/grovesNL/glyphon/blob/main/Cargo.toml):
  upstream main declares 0.12.0, wgpu 30 and cosmic-text 0.19. This matches the
  rebuild's wgpu major. Its [README](https://github.com/grovesNL/glyphon/blob/main/README.md)
  describes shaping/rasterization through cosmic-text, glyph atlas packing through
  etagere, and rendering through wgpu. Its
  [custom glyph API](https://github.com/grovesNL/glyphon/blob/main/src/custom_glyph.rs)
  accepts single-channel masks and physical-pixel snapping. Evaluate it first for
  bitmap-strike adapters and batching, plus modern fallback text. Dependency/MSRV,
  cache invalidation, exact masks and QuickDraw transfer semantics require adapter
  work. The isolated prototype below now verifies the first API fit; no production
  dependency has been selected or added.
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

These initial observations of upstream main did not select a production backend.
The pinned-release prototype below is separate executable evidence. Pin any
production selection and verify its APIs against current primary documentation.

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

The first bounded Glyphon compatibility prototype is now verified below. Next is
the GPU mask/QuickDraw transfer adapter and API audit, followed by integration and
full pixel acceptance. Work continues alongside Marathon/Escape Velocity compatibility.


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
