# GPU production readiness: 68k and PowerPC

Audit baseline: `01c531150` (release 0.99.0, [PR #4813](https://github.com/benletchford/systemless/pull/4813)). This is a desktop
wgpu plan; browser WebGPU presentation and compute are recorded at the end.
Production readiness means correct guest-visible results, bounded failure and
resource behavior, and measured performance on supported hardware. It does not
mean forcing every small guest write through synchronous GPU readback.

## Agreed qualification scope

Only the Apple M1 Mac is available. Per the user's direction, rely on wgpu's
backend portability and driver abstraction rather than requiring Systemless to
repeat a physical vendor/OS qualification matrix. Release acceptance focuses on
Systemless-owned shaders, 68k/PPC dispatch, guest-memory coherence, resource
lifetimes, recovery policy and teardown. Validate these on the available Metal
host and retain portable CI coverage. Other physical hosts are follow-up evidence,
not a prerequisite for this implementation. Do not describe untested hardware as
locally validated or extrapolate M1 performance to it.

The historical Windows destructor localization remains follow-up work: the later
bounded-cache two-hour run completed all teardown stages and exited normally.
A new reproducible integration failure would still require a fix. Source review
of the unsafe Metal upload integration is required; a separate external reviewer
is not an additional prerequisite imposed by this plan.

## Current acceptance status

The identified implementation gaps are fixed. Final-source validation passes:
7,060 library tests; 207 GPU-required desktop tests; six headless application
replays; both two-hour architecture-specific GPU soaks with normal teardown; and
three release benchmark runs per architecture with 100 paired guest samples.
All use source hash
`36cd83c9bdf29b9af4b672eb93aee93568bf4db6063a4833885fde83ac6e2836`.

The unlocked-Mac native window/recovery matrix now passes all 38 cases,
including focus, resize/fullscreen, cursor round trips, renderer selection,
device-loss recovery, platform/software fallback and clean owner teardown.
The additional six-case 300-second lifecycle repetition was stopped after
reviewing its diminishing value beyond the passing 38-case matrix and two-hour
compute runs. It supplies no passing evidence and is not an outstanding gate. Additional
physical GPU coverage, real driver-reset/display-hardware scenarios, power and
input-latency measurements, and historical Windows destructor localization are
follow-up qualification under the agreed scope, not blockers to this
implementation. Wgpu remains explicitly selectable; changing the default
renderer is a separate rollout decision.

## Existing implementation

Both CPU architectures share QuickDraw command types and synchronous writeback.
The kernels support packed/indexed/direct-color formats, clips, pattern transfers
and coverage masks; individual classic/native routes require separate coverage.
Classic trap and native PPC entry points build commands independently. Native
PPC additionally supplies QD3D geometry, textures, lighting, fog and depth.
Retained desktop presentation is independent of guest architecture. Adaptive
selection preserves CPU execution for work below the measured crossover.

Device retirement, bounded readback waits, bounded scratch/bind-group reuse,
presenter recovery and platform/software fallback are already implemented.
Keep these paths and test failure atomicity: rejected GPU work must leave guest
memory unchanged before CPU execution, and successful work must notify the
normal guest memory/presentation/code-cache observers.

## Implementation and acceptance sequence

1. **Establish the baseline.** Run the GPU-required desktop suite serially on
   this host. Record adapter identity, revision, commands and failures. Inspect
   shared command validation, classic/native routing and CI requirements.
2. **Close architecture evidence gaps.** Add a forced real-GPU 68k guest test
   alongside the PPC test, asserting architecture and successful requests as
   well as CPU/GPU pixel equivalence. Cover UI, drawing, styled text and sprite
   masks. QD3D is a native PPC workload and must not be represented as 68k
   coverage. Add explicit architecture selection and evidence to benchmarks
   and sustained compute qualification; reject mismatched or absent evidence.
3. **Complete application comparisons.** Exercise classic and native 2D pages
   and native 3D through headless play. Compare every GPU capture with its CPU
   counterpart (exact integer output; explicit 3D component tolerance). Ensure
   the top-level qualifier runs these checks rather than only kernel tests.
4. **Fix exposed implementation defects.** For each failing operation, trace
   the classic/native call through command preparation, shader execution and
   routed writeback. Add a focused regression using an independent CPU result,
   then fix the responsible path. Preserve packed neighbors, padding, overlap,
   clipping, same-value paint notifications and retained glyph detail. Do not
   broaden acceleration merely to make request counters nonzero.
5. **Validate resource and failure behavior.** Run architecture-specific
   compute smoke checks, then sustained sessions. Require advancing GPU work,
   pixel equality, valid process/device observations, every teardown stage and
   normal exit. Run presenter recovery/lifecycle checks in both runtime modes.
   Diagnose stalls without making successful workload completion conceal a
   failed destructor. [Windows #4890](https://github.com/benletchford/systemless/issues/4890) remains a causal investigation: bounded
   bind-group reuse fixed observed growth and a later two-hour run exited, but
   did not identify the original blocking destructor.
6. **Measure production performance.** Collect three independent release runs
   per architecture on the idle M1 host, alternating backend order. Preserve
   per-sample dispatch counts, median/p95 and p99 only with enough observations.
   Attribute CPU fallback versus GPU work. Investigate individual regressions;
   do not average away a failing workload. Measure actual window pacing and
   input-to-display latency separately from offscreen completion.
7. **Qualify and roll out.** Complete Systemless integration acceptance below,
   review unsafe Metal upload ownership/fences and supported adapter policy, then make a
   separate evidence-backed default-selection change. Keep explicit renderer
   rollback and automatic runtime fallback. Automated success alone must not
   set default eligibility.

## Systemless integration acceptance

| Dimension | Required coverage | Acceptance |
| --- | --- | --- |
| Guest | 68k and PPC 2D; PPC QD3D | CPU/GPU pixels, actual requests, correct architecture |
| Formats | 1/2/4/8-bit indexed, RGB555, 32-bit | Palette/transfer fidelity and packed-neighbor preservation |
| Operations | Shapes, regions, polygons, text, CopyBits/Mask/DeepMask, scaled/direct-color transfers | CPU equivalence, failure-atomic routed writes |
| Host | Available Apple M1/Metal plus portable CI | Named local adapter evidence; rely on wgpu for backend portability |
| Recovery | Startup failure, loss/recreation, exhausted recovery, platform/software fallback | Guest continues; bounded retries; actionable errors |
| Lifecycle | Both runtime modes; resize/fullscreen/focus; sleep/wake, minimize, hot-plug, mixed DPI/refresh | No stale output, hangs, crashes or lost input |
| Resources | Multi-hour active and idle sessions, teardown, host/GPU memory and power | Bounded post-warmup growth and normal exit |
| Performance | UI, drawing, text, sprites, 3D, retained presentation | Investigated M1 median/tail behavior; other hosts follow up |

Additional physical hardware coverage is explicitly unavailable and deferred,
not a release gate. Hosted software adapters establish correctness, not physical
GPU performance or power. Systemless-owned window lifecycle and recovery checks
on the available Mac remain necessary; wgpu does not implement that application
policy or validate guest-visible pixels. Detailed existing procedures are in
[RENDERER-QUALIFICATION.md](RENDERER-QUALIFICATION.md) and
[RENDERING-PERFORMANCE.md](RENDERING-PERFORMANCE.md).

## Execution record

- Baseline audit found PPC-only forced guest writeback, benchmarks and compute
  soaks; the classic play test checks adaptive installation, not actual dispatch.
- Added classic deep-mask GPU dispatch with bounded staging, complete RAM
  preflight, overlap rejection and atomic fallback. Preserved classic RGB555
  replication and full-precision palette matching in the shared kernel.
- Routed previously unsupported classic CopyMask direct-color/deep-mask work
  through the complete CopyDeepMask CPU/GPU transfer implementation.
- Added forced 68k guest writeback coverage; the first M1/Metal run passed.
- Expanded the GPU deep-mask oracle across classic and native color policies,
  including duplicate palette endpoints. Final instrumented desktop suite:
  207 passed, seven intentionally ignored, GPU required on Apple M1/Metal.
- Added architecture selection/identity validation to benchmark and compute
  reports (schema 5), both-architecture compute CI, and both-architecture runs
  in the top-level qualifier. Parser tests: 15 benchmark and 13 compute passed;
  one Windows-only API test skipped on macOS.
- Added the missing full CPU/GPU native 3D capture comparison, reject QD3D
  software fallback logs, and include headless application comparisons in
  top-level qualification.
- Final classic/QuickDraw library checks passed: 190 classic-filtered tests and
  975 QuickDraw-filtered tests, including transfer and fallback regressions.
  A final compatibility guard preserves existing 2/4-bit-to-8-bit CopyMask
  palette translation; all 12 focused CopyMask tests passed after that change.
- Both 60-second M1/Metal compute smoke runs passed with pixel agreement,
  all eight teardown events and normal exit. 68k: 27 sweeps, 37,207 QuickDraw
  requests, 28 deep-mask requests, 64 valid memory samples. PPC: 11 sweeps,
  153,158 QuickDraw requests, 12 deep-mask and 12 QD3D requests, 69 valid memory
  samples. Neither run had sampling errors; these are not multi-hour evidence.
- Final headless play validation passed all six CPU/GPU runs (68k UI,
  PPC 3D and PPC sprites), with full screenshot comparisons and visual review.
- Full native lifecycle qualification is blocked on this desktop: the platform
  baseline delivered 1,823 guest frames but timed out at initial native focus
  after 90 seconds. The remaining focus-dependent matrix was stopped; this is
  failed/pending lifecycle evidence, not a GPU pass. Recovery-only startup
  fallback passed, but the next wgpu case advanced 2,205 guest frames without
  recording a presentation; loss injection was never reached. The remaining
  window matrix was stopped. A read-only IORegistry query subsequently reported
  `CGSSessionScreenIsLocked: true`, consistent with these failures. Rerun the
  full matrix on an unlocked desktop; offscreen success does not replace it.
  Three independent release batches of 100 paired guest observations per
  architecture passed all pixel checks. Both architectures' timed showcase
  redraws submitted zero adaptive compute requests; these are CPU-execution
  observations, not GPU-compute speedups. Separate offscreen presentation
  medians were 0.928–1.042 ms (wgpu UI) versus 2.018–2.544 ms (native Metal UI),
  and 2.828–2.833 ms (wgpu text) versus 4.431–4.452 ms (native Metal text).
  Per-run p95/p99 and chronological observations are retained in the reports.
  Benchmark source hash: `d3175eecae0e897836f3fd7e215b60d87a81ae5abd22b1645de11d78b9611997`;
  this precedes the final extreme-coordinate arithmetic hardening. Rerun final
  release qualification before approving rollout.
- Bounds review found that valid signed QuickDraw rectangles can overflow the
  shared 32-bit scaling product. The mapping now uses 64-bit intermediates;
  a regression exercises the far end of a 65,535-pixel bitmap and checks its
  neighboring bytes. All eight deep-mask and 55 scaling-related tests passed.
  The no-default-features library build passed. Both refreshed final-source
  GPU smoke checks passed, followed by 207 GPU-required desktop tests (seven
  intentionally ignored). Final source hash:
  `36cd83c9bdf29b9af4b672eb93aee93568bf4db6063a4833885fde83ac6e2836`.
  The final application rebuild and all six capture replays passed, including
  full CPU/GPU screenshot comparisons (exact integer scenes; tolerance eight
  for 3D). Broader hardware/performance/power qualification
  and the original Windows teardown causal investigation remain pending.

- Extended qualification is running sequentially for 7,200 seconds per
  architecture, using the final source hash above. Reports are collected at
  `/tmp/systemless-compute-long-68k` and `/tmp/systemless-compute-long-ppc`.
  Initial 68k telemetry confirms Apple M1/Metal and advancing actual GPU
  requests. These runs are pending, not passing evidence, until their final
  reports include complete teardown and normal process exit.

- Qualification-driver review found a four-case outer timeout around a
  six-case lifecycle soak. Timeout budgets now derive from the verifier
  dry-run matrix for both lifecycle and recovery, including selected subsets.
  Ten native-matrix tests pass without opening windows. Guest/rendering source
  is unchanged; the ongoing compute runs retain their source identity.

- Source review checked the shared Metal uploader against the pinned
  wgpu-hal 30.0.1 mapping contract: mappable usage, nonzero bounded mapping,
  initialization/submission completion before CPU writes, flush for noncoherent
  mappings, and pointer lifetime ending at unmap. No mismatch was found in
  those paths; this is not independent sign-off or physical fault testing.
- Three final-source release benchmark batches of 100 paired samples per
  architecture are queued after both extended compute runs pass. Output paths:
  `/tmp/systemless-benchmark-final-68k` and
  `/tmp/systemless-benchmark-final-ppc`. No benchmark runs concurrently with
  the active compute workload.

- Final-source full library validation passed: 7,060 tests, zero failures, six
  ignored (`cargo test --locked --profile ci-test --lib -- --test-threads=1`).
  Log: `/tmp/systemless-final-library-suite.log`. Its 38-second serial test
  phase overlapped the 68k stress run; that run is resource/correctness evidence,
  not idle-host performance evidence. Release benchmarks remain serialized
  after the compute runs.

- Final-source 68k extended compute qualification passed: 7,202.04 active
  seconds, 3,106 CPU/GPU-checked sweeps and 4,049,144 QuickDraw requests
  (including 3,747,680 masked-raster and 3,107 deep-mask requests; zero native
  QD3D requests). All eight teardown markers completed and process exit was
  zero. Across the run, requested scratch capacity stayed at 6,079,668 bytes
  and native Metal allocated size at 6,979,584 bytes. All 7,125 process-memory
  observations were valid, with zero collection errors. RSS ranged from
  9,328 to 105,328 KiB including startup, ending at 52,768 KiB. These counters
  do not establish complete driver residency or power use. Evidence:
  `/tmp/systemless-compute-long-68k/summary.json`. The PPC two-hour run has
  started; release benchmarks remain queued behind it.

- Final-source PPC extended compute qualification also passed: 7,201.85 active
  seconds, 1,209 CPU/GPU-checked sweeps and 15,355,778 QuickDraw requests
  (15,183,913 masked-raster and 1,210 deep-mask requests), plus 1,210 QD3D
  requests. All eight teardown markers completed and exit was zero. Scratch
  capacities stayed at 6,109,364 bytes (QuickDraw) and 4,343,944 bytes (QD3D).
  Both devices reported a constant native Metal total of 11,878,400 bytes;
  these shared-native-device observations must not be summed. All 7,135 memory
  samples were valid with zero errors. RSS ranged from 1,728 to 285,824 KiB
  including startup and ended at 95,344 KiB. Evidence:
  `/tmp/systemless-compute-long-ppc/summary.json`. Both architecture soaks are
  now complete; final-source release benchmarks have started.

- Final-source release benchmarks passed all six batches (three per guest
  architecture), including pixel oracles and 100 paired guest observations per
  workload. Source identity matches both extended soaks. Guest adaptive requests
  were zero for these small scenes; their median CPU-path deltas ranged from
  -0.34% to +0.17%. Tail differences varied across runs (including PPC text p99
  +8.9% in one run and -5.35% in another), without a repeatable GPU-dispatch
  regression in these zero-dispatch scenes. Raw samples and per-run tails remain
  in the reports; this is not proof of hardware-independent latency.
  Across all six batches, offscreen UI presentation medians were 0.900–1.069 ms
  with wgpu versus 1.991–2.677 ms with native Metal; text medians were
  2.716–2.832 ms versus 3.939–4.434 ms. The synthetic tiled 3D mesh medians
  were 14.609–15.038 ms adaptive GPU versus 133.758–134.684 ms software.
  These measure completed offscreen operations, not window pacing or game FPS.
  Reports: `/tmp/systemless-benchmark-final-68k/summary.json` and
  `/tmp/systemless-benchmark-final-ppc/summary.json`.

- After the user unlocked the Mac, the full native window matrix passed all
  38 cases, with 38 successful reports and 38 production-close teardown checks.
  This includes inline/thread runtime modes, default/explicit platform/wgpu/
  software selection, startup failure, loss/recreation, repeated loss,
  platform failure, software fallback, and software-layer recreation.
  Evidence: `/tmp/systemless-native-unlocked-final` and its adjacent `.log`.
  A subsequent repeated lifecycle run was started at
  `/tmp/systemless-lifecycle-unlocked-final` and stopped as recorded below.

- The extra repeated-window run was stopped deliberately (SIGTERM, exit 143),
  and its owned test window/process was terminated. Do not count it as passing
  qualification. The completed 38-case matrix remains the window evidence.
- Web scope was extended at the user's request. The browser now has an opt-in
  `?renderer=wgpu` Rust/WASM presenter, using wgpu 30's WebGPU backend in the
  existing bounded renderer worker. Both execution modes retain CPU-authoritative
  guest memory and complete RGBA snapshots. GPU completion gates transport credit;
  device loss, startup failure and submission errors use existing Canvas2D recovery.
  Default browser backend selection is unchanged.
- Web validation: WASM check and Trunk build passed; 141 JavaScript tests and 93 frontend Rust tests passed.
  The real Chrome 151 probe on this M1 required `offscreen-wgpu`, compared four
  resized synthetic images and both 68k/PPC Toolbox showcase framebuffers with
  zero differing bytes, and observed injected device loss. Both guests advanced
  and produced nonblank 256-color images. Evidence:
  `/tmp/systemless-webgpu-browser.log`, `/tmp/systemless-webgpu-build.log`, and
  `/tmp/systemless-web-tests.log`. The probe keeps separate reference copies
  because transferred frame buffers become detached.
- Browser compute (follow-up request): guest-visible QuickDraw and QD3D now run
  on WebGPU for both 68k and PowerPC, using the same kernels as the desktop.
  - Shared kernels: `src/gpu_compute/` (`wgpu-compute` feature) holds the
    QuickDraw and QD3D compute state, scratch buffers and shaders, moved from
    `src/bin/desktop`. Readback and error scopes are async; the desktop wraps
    each call in `pollster::block_on` with its existing bounded Metal-aware
    poll, so desktop behavior and tests are unchanged. Dispatch heuristics are
    in an ungated `routing` module shared by every host.
  - Synchronization: the trait contract stays synchronous. A dedicated compute
    worker owns the WebGPU device; the emulation worker sends a full input
    snapshot (`www/src/gpu_wire.rs`), blocks in `Atomics.wait`, and copies the
    reply out of a `SharedArrayBuffer` before committing pixels. Requests are
    serialized because kernels reuse scratch buffers. Guest memory is never
    shared with the compute worker.
  - Failure handling: declined operations fall back per operation; device
    loss, decode errors, worker failure or a reply timeout (5 s default) are
    sticky and return all later work to the CPU. A timed-out link is closed
    and the compute worker terminated, so late replies cannot be misattributed.
  - Opt-in: `?renderer=wgpu` (adaptive) or `gpu_compute=adaptive|force|off`,
    worker execution only. `www/public/_headers` adds COOP `same-origin` and
    COEP `credentialless` for cross-origin isolation; unsupported browsers stay
    unisolated and keep CPU rendering.
- Browser compute evidence (Chrome 151 headless, Apple M1, `fast` profile
  bundle, `node www/scripts/verify-gpu-compute-cdp.mjs`): pages served with the
  production `_headers` were cross-origin isolated. The production emulation
  worker replayed the desktop play scenarios on the CPU and with forced GPU
  dispatch:
  - 68k UI: 802 GPU raster requests, 0 failures; `graphics` and `controls`
    captures identical to the CPU.
  - PPC sprites: 2,785 raster and 3 deep-mask blit requests, 0 failures;
    `sprite-deep-mask` identical.
  - PPC QD3D: 1 QD3D frame and 2,708 raster requests, 0 failures;
    `native-qd3d` identical (0 differing pixels; tolerance was 8).
  - Adaptive dispatch on the QD3D scenario ran cleanly and, like the desktop
    heuristics, kept these small operations on the CPU (0 requests).
  - Injected device loss after 300 ms on 68k: 9 requests completed, the next
    reported the fatal loss, all later work ran on the CPU, and both captures
    stayed identical.
  - Guest-observed round trip, including transfer and readback: mean
    0.56–0.64 ms per request, maximum 13–20 ms (first-use pipeline creation).
- Browser compute validation: desktop GPU kernel tests against the shared
  module, 101 frontend Rust tests (wire round trips, malformed input, proxy
  routing/failure handling, page options), and the JavaScript suite including
  real `worker_threads` tests of blocking replies, buffer growth, timeouts and
  undeliverable replies.
- Browser compute limits: evidence covers Chrome on this Mac only. Safari lacks
  COEP `credentialless` and stays on the CPU. The compute worker loads a second
  copy of the WASM bundle when enabled. These scenarios demonstrate correctness
  and integration, not a frame-rate improvement: adaptive dispatch made no GPU
  requests here, and forced dispatch is a verification mode.
