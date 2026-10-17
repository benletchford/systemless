# Glyphon compatibility probe

This isolated executable evaluates Glyphon 0.12.0 with the rebuild's wgpu 30.0.1
and classic FONT decoding. The executable remains a compatibility probe. Its
initial evaluation kept production dependencies unchanged; the subsequent shared
runtime integration now pins the same Glyphon release. See the production
integration section in [text-rendering.md](../../docs/text-rendering.md).
The probe retains a local monochrome coverage baseline, since production
`text::draw_strike` now uses Glyphon. Historical benchmark results below describe
the probe, not current production workload performance.

From the repository root, run:

```sh
cargo run --locked --manifest-path future/prototypes/glyphon/Cargo.toml
cargo +1.93.0 check --locked --manifest-path future/prototypes/glyphon/Cargo.toml
cargo +1.99.0 clippy --locked --manifest-path future/prototypes/glyphon/Cargo.toml -- -D warnings
cargo deny --manifest-path future/prototypes/glyphon/Cargo.toml --config future/prototypes/glyphon/deny.toml check licenses
```

A real wgpu adapter is required. Adapter or GPU failures are errors, not skipped
successes. The program prints the adapter and checks exact RGBA readback on a
16-by-12 offscreen target. Its synthetic FONT fixture is decoded by the actual
shared `systemless_future::font::Strike`; Glyphon receives the resulting one-bit
mask through `prepare_with_custom`. The host font database is empty, and
`Buffer::new_empty` avoids fallback shaping. No native font APIs render pixels.

The original six cases verify or characterize:

1. Exact bitmap mask pixels and physical-pixel snapping at a fractional placement.
2. Cache reuse without another rasterization request.
3. Partial rectangular clipping.
4. An edited source FONT byte with the same custom-glyph key retains cached pixels.
5. A new custom-glyph ID refreshes pixels from the edited FONT data.
6. Repeating the stock renderer preserves white pixels rather than implementing
   QuickDraw XOR, which would cancel the two identical draws.

Cases 4 and 6 deliberately characterize missing adapter semantics. They do not
claim guest-edit invalidation or QuickDraw XOR is already implemented by Glyphon.

## Recorded result

On 2026-10-16 all six cases passed on an Apple M1/Metal adapter. Rust 1.93 check,
strict Rust 1.99 Clippy and the scoped license gate passed. Logs:

- `/tmp/systemless-glyphon-probe-acceptance.txt`
- `/tmp/systemless-glyphon-msrv-acceptance.txt`
- `/tmp/systemless-glyphon-lint-acceptance.txt`
- `/tmp/systemless-glyphon-licenses-acceptance.txt`

Earlier exploratory builds exposed the fallible wgpu buffer-view API, strict
slice lint requirements and the default text constructor's fallback-font lookup.
Those were corrected; earlier failed runs are superseded, not counted as passes.

Glyphon's license is MIT OR Apache-2.0 OR Zlib. The pinned direct graph includes
cosmic-text 0.19.0, etagere 0.3.0, lru 0.16.4, rustc-hash 2.1.3 and wgpu 30.0.1.
The separate lockfile makes this experiment reproducible. The license configuration
uses a subset of the repository's permissive allowlist, strict unused entries and
scoped exceptions for the two local GPL packages.

## Next integration gate

The result supports a bitmap-mask adapter prototype. Before production adoption:

- Preserve guest metrics/layout and assign bounded cache identities tied to font
  content/selection/generation. Handle edits, moves, replacement and ID exhaustion.
- Connect actual guest text to the verified GPU mask/QuickDraw transfer bridge
  described below, preserving colors, clipping and packed destinations. Keep
  readback in tests; the production bridge must not copy mask pixels through the
  CPU merely to perform destination composition.
- Verify 68K/PPC guest entry points, synchronization with guest pixel writes,
  opaque spaces, style/spacing APIs and actual UI callers.
- Benchmark complete workloads including uploads, atlas growth, GPU passes and
  readback/synchronization costs; this small probe is not a performance benchmark.
- Verify native presentation and browser support separately.

Primary API references:
[Glyphon TextRenderer 0.12.0](https://docs.rs/glyphon/0.12.0/glyphon/struct.TextRenderer.html),
[COSMIC Text Buffer 0.19.0](https://docs.rs/cosmic-text/0.19.0/cosmic_text/struct.Buffer.html),
and [custom glyph source](https://github.com/grovesNL/glyphon/blob/main/src/custom_glyph.rs).


## GPU QuickDraw transfer bridge (2026-10-16)

The probe now shares the actual runtime Graphics device with Glyphon. Its binary
mask texture is bound directly into the existing QuickDraw Blit kernel through
Ports::copy_gpu_mask. Eight additional cases check packed one-bit outputs against
independent transfer equations with port clipping; two more check double-XOR
cancellation and direct guest edits between GPU draws. All sixteen cases pass on
M1/Metal: /tmp/systemless-glyphon-transfer-final.txt, terminal exit 0. The original
stock renderer still lacks XOR; the shared runtime bridge supplies it. No mask
readback/reupload is used as input to the bridge; readbacks in this executable
serve assertions, while destination readback implements coherent guest memory.

The runtime's indexed-destination fixture separately compares GPU masks to the
existing CopyBits path across 2/4/8-bit targets and all eight modes, including
negative bounds, color palettes, clipping, packed padding and output guards.
Production library selection, source-generation caching, complete text layout/API
integration, native/browser presentation and performance acceptance remain open.
The bridge supports binary Rgba8Unorm red-channel coverage; it does not interpret
antialiased or color glyph output.


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


## Shared production path comparison (2026-10-17)

The runtime now pins Glyphon 0.12.0 for shared strike drawing. The probe retains
its former monochrome coverage implementation locally, using the existing public
GPU coverage primitive; this is separate from production draw_strike. Eight new
checks compare production overlapping strings to this coverage baseline in every
classic transfer mode, including final pen positions. All 40 probe assertions pass
on Apple M1/Metal in /tmp/systemless-text-integration-probe-workload.txt.

The optional benchmark now has coverage, glyphon-probe and glyphon-production
lanes. Both Glyphon lanes warm the current workload before timing, and order is
reversed each alternate round. The production lane does not yet instrument
raster uploads and reports them as unmeasured. All 36 debug samples complete with
pixel equality between all three lanes in the same log. This overlapping debug
run validates the harness only; its times are not production performance evidence.
Representative application workloads, cold uploads, total atlas memory, native
presentation and browser acceptance remain required. Strict final probe lint
passes in /tmp/systemless-text-integration-probe-lint-final.txt.
