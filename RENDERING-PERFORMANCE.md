# GPU rendering performance

Run on an idle host with a physical GPU:

```sh
python3 .github/scripts/benchmark-renderers.py --output /tmp/render-benchmark
```

The opt-in harness builds the production `release` profile, runs serially,
requires a working GPU, and writes logs, captures and `summary.json`. Its
ordinary test-suite entries are ignored. There are no machine-dependent CI
timing thresholds. `--samples` controls guest redraw repetitions and `--runs`
controls independent runs. Default values are 15 and 3. Use `--profile fast`
for shorter optimized iteration builds or `--profile ci-test` for checks-enabled
diagnostics; their timings can differ from production LTO. Device creation and
warm-up work is excluded from measured samples (three guest/kernel iterations,
120 presentation frames).

## Workloads and timing boundaries

- **UI, drawing, text, sprites, 3D:** native PowerPC Toolbox Showcase controls,
  graphics, styled text, CopyMask/CopyDeepMask sprites and QuickDraw 3D pages.
  Each sample switches from another page through the menu and runs the same
  24 simulated frontend frames. Reset-page work, initial loading, screenshots,
  host window presentation and vsync are excluded. These are complete guest
  redraw transactions, not individual display-frame times. CPU/GPU captures
  must match exactly, allowing eight RGB levels for 3D float quantization.
- **UI/text presentation:** 640×480 retained packets at 4× detail resolved into
  an 800×600 offscreen target. Three hundred samples include validation, upload,
  command encoding and GPU completion, with resources/pipelines already warm.
  Each sample uses a new packet with one changed cell. Unchanged retained
  detail may stay resident, as it does during ordinary UI updates. macOS compares
  the default
  Metal shader/upload path with production wgpu encoding on the same host
  thread, alternating backend order between runs; this does not measure
  drawable acquisition, compositor latency or input responsiveness. Other hosts
  currently report only the wgpu side of this presentation comparison.
- **3D kernels:** the actual software rasterizer and synchronous wgpu renderer
  rasterize the same flat opaque triangle at 64×48, 320×240 and 640×480, with
  thirty samples, plus a sparse 64-triangle mesh and eight large overlapping
  triangles. GPU times include upload/readback; software times include the
  helper's target/depth setup and output extraction. `gpu` forces the kernel;
  `gpu-adaptive` applies the production work selector and uses software below
  its crossover. This isolates raster costs, not full application frame rates.

Neither play-script total duration nor CI software-adapter timings establish
physical GPU/window performance. Pixel comparisons and strict GPU device tests
are separate correctness checks. A forced guest-compute test exercises real
guest drawing, glyphs, masks and 3D through shader execution and routed guest
writeback, independently of adaptive selection.

## Dispatch and resource handling

`--gpu` always selects wgpu desktop presentation. Guest-visible memory is still
CPU-addressable, so cheap guest writes stay on the CPU rather than paying a
synchronous GPU round trip. CPU-generated coverage is not repackaged into GPU
atlases merely to discover that the operation should run on the CPU.

Compute dispatch currently selects classic polygon work at 131,072 pixel-edge
products, expensive bitmap work at 16,384 destination cells, and 3D estimated
work at the larger of 200,000 samples and one quarter of the framebuffer area.
The 3D estimate includes clipped primitive bounds, viewport clearing and lighting;
framebuffer area matters
because guest readback has a cost even when a primitive covers few pixels.
The minimum 3D work avoids the measured small-frame p95 readback regression:
a faster GPU median alone does not justify worse synchronous guest latency.
These are conservative dispatch heuristics, not universal hardware guarantees.
The selector also compares the compute kernel's full-frame primitive scan with
the CPU's clipped primitive bounds. Sparse meshes and frames beyond the GPU
work limit stay on the CPU before GPU staging. Other host renderer
implementations keep their existing selection by default.

Synchronous compute submissions reuse a bounded set of geometrically grown
buffers. Readbacks map only the current result and unmap before reuse. Retained
presentation borrows packed u32 storage directly for uploads. Exact comparisons
find changed ranges, so stable detail and geometry are not uploaded again.
Raw ARGB frames upload directly as BGRA on little-endian hosts.
Large retained updates can use one aligned shared allocation on supported UMA
adapters;
other updates use persistent buffers and a reusable staging belt. Queue ordering
protects earlier frames when a buffer receives a later incremental update.

Resolve pipelines specialize the four supported sample scales and a flat-area
shortcut. The shortcut returns a color only when every guest cell in the
footprint has that same uniform color. Detailed samples preserve the original
integer weighting and rounding, with shader runtime bounds checks enabled.
Pipelines compile lazily. Unchanged immutable desktop packets skip presentation
until host expose/resize, geometry changes or raster overlays invalidate the
cached frame. Without `--gpu`, existing platform rendering is unchanged.

## Measurements

Apple M1 / Metal, 2026-10-18. Preliminary diagnosis used `ci-test` optimization, not
production LTO; compare production runs using the default `release` profile. Preliminary measurements found 18–22× slower guest redraws when
all small shapes and glyphs were forced through synchronous GPU submissions.
Short-warm-up retained presentation medians were 2.01 ms for UI and 6.47 ms for
text, versus native Metal's 1.22 ms and 2.42 ms. A 64×48 triangle cost 0.635 ms
on the GPU versus 0.151 ms on the CPU; 320×240 and 640×480 already favored the
GPU (0.823/1.753 ms versus 2.178/8.197 ms).

Use the JSON report and individual run logs for current measurements. Reports
include source and fixture hashes, revision, local changes, sample counts,
median/p95 costs, and separate presentation submission costs. Presentation
completion costs include the host API wait and are not display/input latency.
