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
  an 800×600 GPU-private offscreen target on both Metal paths, matching
  production drawable storage. Pixel-oracle calls use a separate CPU-readable
  target; measured calls omit readback. Three hundred samples include validation, upload,
  command encoding and GPU completion, with resources/pipelines already warm.
  Each sample uses a new packet with one changed cell. The backend chooses
  between incremental resident updates and complete shared-memory copies. macOS compares
  the default
  Metal shader/upload path with production wgpu encoding on the same host
  thread, alternating backend order between runs; this does not measure
  drawable acquisition, compositor latency or input responsiveness. Other hosts
  currently report only the wgpu side of this presentation comparison.
- **3D kernels:** the actual software rasterizer and synchronous wgpu renderer
  rasterize the same flat opaque triangle at 64×48, 320×240 and 640×480, with
  thirty samples, plus a sparse 64-triangle mesh and eight large overlapping
  triangles. A separate 1,792-triangle tiled mesh records 100 paired default and
  adaptive-GPU observations with an exact RGB555 oracle; it exceeds the previous
  full-frame scan budget. This synthetic mesh is not the Nanosaur logo. GPU times include upload/readback; software times include the
  helper's target/depth setup and output extraction. `gpu` forces the kernel;
  `gpu-adaptive` applies the production work selector and uses software below
  its crossover, including selector cost. Backend order rotates within each
  repetition so CPU samples do not all precede or follow a forced-GPU batch.
  This isolates raster costs, not full application frame rates.

Neither play-script total duration nor CI software-adapter timings establish
physical GPU/window performance. Pixel comparisons and strict GPU device tests
are separate correctness checks. A forced guest-compute test exercises real
guest drawing, glyphs, masks and 3D through shader execution and routed guest
writeback, independently of adaptive selection.

## Dispatch and resource handling

`--gpu` requests wgpu desktop presentation. Initialization failures and terminal
GPU errors switch presentation to the existing platform renderer without
restarting the guest. Surface/device loss allows at most two presenter rebuilds
per session; a failed rebuild or exhausted budget also switches to the platform
renderer. Error callbacks wake idle windows, including cached unchanged frames.
Failed QuickDraw/3D compute devices retire to CPU rasterization independently
of presentation; healthy compute devices continue adaptive dispatch. Recovery
and fallback reasons are printed to stderr. Guest-visible memory is still
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
QD3D builds conservative 16×16 screen-tile lists after transparency sorting,
so each pixel visits only its tile’s ordered primitives. The selector compares
estimated tile work with the CPU's clipped primitive bounds. Tile storage and
lighting-weighted work remain bounded; excessive overlap still falls back.
Zero-area triangles use the CPU’s double-precision classification, shade at
the perspective-correct centroid and draw all three Bresenham edges, preserving repeated alpha/palette blends and the
software renderer’s viewport/depth bypass. Thin nonzero triangles whose area
collapses in shader precision still use software. Other host renderer implementations
keep their existing selection by default.

Synchronous compute submissions reuse a bounded set of geometrically grown
buffers. Readbacks map only the current result and unmap before reuse. Retained
presentation borrows packed u32 storage directly for uploads. Exact comparisons
find changed ranges, so stable detail and geometry are not uploaded again.
Raw ARGB frames upload directly as BGRA on little-endian hosts.

On supported Metal UMA devices, retained presentation uses three reusable
shared-buffer slots. The CPU writes a slot only after its previous wgpu queue
completion fence has fired, then unmaps the HAL view before submission. This
avoids full-detail comparisons, blit encoders and asynchronous remapping for
ordinary UI updates. wgpu still owns the resources, validates packets/bindings,
and executes the render pass; shader bounds checks remain enabled. A CPU-cached slot is
cleared asynchronously through wgpu before the first HAL write, and remains
unavailable until that initialization fence completes. Exact maximum-reference
validation uses a vectorizable reduction instead of comparing host snapshots. HAL mappings and pointer
lifetimes are confined to one upload, following its documented CPU/GPU ordering
and coherence contract. Slots grow geometrically up to 32 MiB each. Unsupported
backends, larger packets, or three occupied slots use the existing upload path
without waiting for a slot. Reusing the same immutable packet during expose or
resize updates geometry without copying its retained payload again; weak
identity tracking prevents allocation-address reuse from producing a false hit.
Without `--gpu`, no pool is created.

Large retained updates can use one aligned shared allocation on supported UMA
adapters;
other updates use persistent buffers and a reusable staging belt. Queue ordering
protects earlier frames when a buffer receives a later incremental update.

Resolve pipelines specialize the four supported sample scales and a flat-area
shortcut. The shortcut returns a color only when every guest cell in the
footprint has that same uniform color. Detailed samples preserve the original
integer weighting and rounding, with shader runtime bounds checks enabled.
Coverage ratios cancel each axis's greatest common divisor on the CPU. The
shader uses the smaller equivalent integer weights, and an exact shift when
the final divisor is a power of two; other divisors retain integer division.
Cropped source addressing and nearest-sample clamping use the original source
extent. Pixel checks include odd divisors and independent up/downscaling axes.

On Metal, synchronous guest readback uses sleeping completion waits with a
50 µs timeout for up to 4 ms before reverting to a longer sleeping wait. The complete readback wait has a
five-second budget on physical GPU adapters; software (`Cpu`) adapters receive a
finite 60-second budget because their shaders run on shared host CPUs. The
adapter policy is cached at device creation, outside timed frame work. A timeout retires that compute device to
CPU fallback for the session, including when the driver reports no device loss.
Retired compute renderers decline GPU selection on subsequent work, avoiding
repeated bitmap packet construction and 3D background gathering before CPU fallback.
Failed compute state is retired immediately on a detached cleanup worker: wgpu
queue destruction can itself wait for idle. If worker creation fails, that one
failed state is retained until process exit rather than blocking the guest owner. This allows wgpu to
observe completed command-buffer status even when the driver's separately
scheduled completion notification is delayed. Mapping callbacks and coherence
still run through wgpu's normal maintenance path. Other backends retain their
blocking wait. The offscreen completion benchmark uses this same policy;
window presentation remains asynchronous.

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
median/p95 costs, and separate presentation preparation/encoding/submission
costs. Set `SYSTEMLESS_BENCH_GPU_TIMESTAMPS=1` for supplementary wgpu render-pass
GPU timestamps when the adapter supports queries. Query resolution/readback runs
after all timed samples; ordinary runs do not enable timestamp queries.
Presentation
completion costs include the host API wait and are not display/input latency.

To isolate a guest workload, add `--guest-case ui`, `drawing`, `text`, `3d` or
`sprites`. This keeps the same samples and pixel oracle, and still measures
presentation and 3D kernels. `--reverse-first` reverses the first presentation
backend order when collecting independent batches. Reports record both options;
without `--guest-case`, every guest workload is measured as before.

## Tail evidence

Guest samples now run in independent default/GPU guests, paired by redraw
iteration with alternating backend order. `--reverse-first` also reverses the
first guest order; earlier reports only reversed offscreen presentation.
Reset-page work and screenshots remain outside timing. Both final captures
must still match the pixel oracle. Compare new paired guest measurements with
other paired runs rather than pooling them with the older sequential batches.

Schema 2 reports preserve chronological `samples_us` for every record. Values
at the same index in the two guest records are paired observations. Backend
completeness, expected counts, finite nonnegative durations and agreement with
logged quantiles are required; missing evidence fails the run. p99 is reported
only for records with at least 100 observations, and remains null below that
count. A displayed p99 is not confidence that a hardware row is qualified; use
longer runs and inspect distributions/outliers and backend-order effects.
No machine-dependent timing threshold approves the default automatically.

Schema 3 additionally requires chronological `gpu_requests_per_sample` for
GPU guest records. Each four-element row counts successful QuickDraw requests,
glyph-mask requests, deep-mask requests and 3D requests during the corresponding
timed redraw. Mask requests are subsets of QuickDraw, not additional submissions.
Reset and warmup requests are excluded. All-zero rows mean adaptive selection
submitted no GPU request; they remain valid CPU-execution timing observations.
Other records use null. `gpu_request_columns` names the four columns. Missing,
misaligned or malformed dispatch evidence fails parsing; a GPU request error
fails the benchmark instead of silently qualifying CPU fallback.

The benchmark wrappers delegate all production dispatch preferences and add
single-threaded counters after successful calls. Counter updates are included
in elapsed time; reading and logging them is outside timing. These are completed
renderer requests, not hardware timestamp queries or input-to-display latency.
Use this evidence to attribute tails before changing dispatch thresholds, and
keep schema 2 results separate when comparing instrumentation-sensitive timings.
