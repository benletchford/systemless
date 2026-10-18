# Renderer qualification

Wgpu remains opt-in with `--gpu` or `--renderer=wgpu`. `auto` and `platform`
select the existing renderer. Passing automated checks does not change that
policy; promotion requires a separate reviewed change.

## Reproduce automated evidence

On an unlocked desktop, with a working physical GPU:

```sh
python3 .github/scripts/qualify-renderers.py \
  --output /tmp/renderer-qualification --soak-seconds 300 --benchmarks
```

The runner requires GPU-backed correctness tests, builds the test-support native
probe, exercises both runtime modes with default/explicit renderer selection and
five failure scenarios, repeats baseline lifecycle checks for the requested soak
duration (at least two cycles), then runs production release benchmarks serially.
Each lifecycle cycle checks guest progress, resize, fullscreen entry/exit,
focus loss/return, cursor input and clean owner teardown. Error scenarios cover
startup failure, successful loss recovery, terminal failure, failed recreation,
and repeated loss exhausting the two-rebuild session budget. Injection uses the
production error notification/recovery paths; it does not simulate actual driver
resets or hardware removal.

Reports preserve commands, logs, revision, dirty state, fixture hash, adapter
messages, per-case display size/scale/refresh, recovery/fallback counts and guest
progress. `memory.json` records one-second process RSS samples on Unix; it is not
GPU allocation accounting. Test-support builds additionally enable wgpu HAL
counters and record presenter-device resource snapshots after the first frame and
each lifecycle cycle. Reports include backend-dependent object counts, positive
buffer/texture byte counters, optional allocator allocated/reserved totals, and
requested shared-upload pool capacity on Metal. Metal also samples `MTLDevice.currentAllocatedSize` on the renderer's own native
device when available. That total may include other owners sharing the native
device and does not measure residency. Zero HAL memory counters and missing
allocator reports are recorded as unknown (`null`), not evidence of zero usage.
Compare matching renderer/rebuild epochs after warm-up: deferred GPU destruction
and allocator reservation can legitimately outlive a frame. These observations
exclude guest compute devices and complete driver/process residency; unsupported
accounting, compute-resource stress and physical memory/power review remain
pending. Counter instrumentation is absent from ordinary and release benchmark
builds. Resource observations are streamed during the run, and repeated check labels are
retained once, keeping observer storage bounded during steady-state RSS sampling.
The compatible resource array is materialized only for final/error reporting;
account for that shutdown serialization spike. Review growth after warm-up across
repeated cycles.
Sampling applies to functional windows, not timed offscreen benchmarks.

Use `--skip-windows` on a locked session to collect correctness/benchmark evidence;
window qualification remains explicitly pending. Omitting `--benchmarks` also
leaves production benchmarks pending. The report always sets
`default_eligible: false`: this tool collects evidence and cannot approve rollout.
A failed command records failure and returns nonzero. Use a fresh output directory
for each run. Full benchmark timing boundaries are in
[RENDERING-PERFORMANCE.md](RENDERING-PERFORMANCE.md).

The native-window workflow runs Linux virtual-desktop and macOS desktop checks,
including a short repeated lifecycle soak. Dispatch it with `soak-seconds` for
longer runs. The default four-case matrix permits up to 3600 seconds per owner.
For a genuine multi-hour session, select a single runtime and presenter:

```sh
python3 .github/scripts/verify-native-window.py --soak-only --soak-seconds 7200 \
  --runtime inline --presenter wgpu --output /tmp/native-two-hour
```

`--runtime`/`--presenter` only filter soak-only runs; ordinary qualification always
keeps the complete matrix. Durations above one hour require both selectors.
Local runs allow up to six hours; hosted jobs allow up to five hours so build and
matrix checks fit their six-hour job limit. Dispatch the native workflow with
`runtime` and `presenter` selections for those long cases. The main qualifier
accepts `--soak-runtime`/`--soak-presenter` too. A dry run lists selected cases
without opening a desktop (`--dry-run`). Virtual/software adapters
qualify correctness only. Windows correctness builds run separately; real Windows
window lifecycle qualification still needs a desktop run of this tool.

## Sustained guest compute resources

The window soak observes the presenter. To exercise the separate QuickDraw/QD3D
compute devices in a continuous session, run:

```sh
python3 .github/scripts/soak-gpu-compute.py --seconds 7200 --output /tmp/compute-qualification
```

This test-support-only workload forces actual GPU writeback for UI, drawing,
styled glyphs, sprite/deep masks and 3D, including small work the production
adaptive selector intentionally keeps on CPU. Independent guests compare pixels
throughout; every sweep must make progress in all four GPU request categories.
Any GPU error/fallback, exhausted guest work budget or pixel mismatch fails.
It records both compute devices' counters, requested scratch capacities, native
Metal allocation observations when available, and process RSS on Unix. Windows
collects test-process working set, private commit and their peaks through
`GetProcessMemoryInfo` (`PROCESS_MEMORY_COUNTERS_EX`); `rss_kib` is working-set
bytes divided by 1024. Private commit is process-wide commitment, not GPU
residency. These values must not be added to HAL counters. Shared
native Metal totals must not be summed across owners. This is resource/correctness
stress, not a production performance or power benchmark.

The runner resolves the exact test executable from Cargo JSON, verifies the
ignored test exists, and preserves binary/source/harness/fixture hashes and logs.
Changed source/revision or incomplete device/progress evidence cannot pass.
Schema 2 also records explicit destruction stages for the guest runners, QD3D,
QuickDraw and report writer. A final resource sample marks workload completion;
qualification still requires every destruction stage and successful process exit.
Timeout reports retain the last entered stage, so a teardown stall cannot be
mistaken for a completed session. The compute workflow's optional OS selector
supports targeted diagnostic reruns; PR smoke checks retain all three OSes.
Ordinary tests skip the explicit-duration soak; release benchmarks exclude the
instrumentation. `--seconds 1` is a smoke check, not long-run qualification.
The compute workflow smoke-tests all three OSes and can be dispatched for up to
five hours per OS; hosted software devices still do not qualify physical GPUs.
The main qualification runner accepts `--compute-soak-seconds` (0 leaves this gate
pending). A one-hour lifecycle case is not a multi-hour session, even when four
cases run sequentially.

## Gates before enabling wgpu by default

| Gate | Required evidence | Current disposition |
| --- | --- | --- |
| Recovery | Both runtime modes survive all injected failures and continue submitting guest frames; no panic/restart or unbounded recreation | Automated tests; real driver-reset qualification still required |
| Pixels | UI, drawing, glyphs, indexed colors, clips, masks and 3D match CPU/native oracles | GPU-required suite and benchmark capture checks |
| Physical hardware | Apple Silicon and Intel Mac Metal; Windows Intel/AMD/Nvidia DX12; Linux Intel/AMD/Nvidia Vulkan, with OS/driver/adapter identity | Broader physical matrix pending; hosted CI does not fill these rows |
| Performance | At least three independent production runs on idle hosts, order alternation, UI/drawing/text/3D/sprites median/p95 and p99 where sampled sufficiently | Apple M1 evidence exists; rerun after material changes and collect other hardware |
| Real window responsiveness | Drawable acquisition and presentation pacing, dropped frames and input-to-display latency at common refresh rates and scales | Pending; offscreen completion timings do not measure this |
| Lifecycle | Sleep/wake, minimize/restore, monitor hot-plug, mixed DPI/refresh migration and resize/fullscreen/focus transitions | Automated subset; remaining physical transitions pending |
| Long-running resources | Multi-hour idle and active workloads, bounded host/GPU memory, no progressive stalls; compare power/battery with platform | Pending; short CI soak is a regression check |
| Metal upload safety | Review HAL mapping lifetime, ownership, coherent CPU/GPU ordering, fence reuse and bounded slots; validate supported/unsupported adapter paths | Existing tests and bounded implementation; independent review pending |
| Rollout | Review all evidence, choose supported hardware policy, preserve explicit platform rollback and automatic failure fallback | Pending; auto remains platform |

Investigate a reproducible median or tail regression before approving a hardware
row. Do not average away one failing workload or use shared CI timing as physical
performance evidence. Keep raw captures/logs and identify the exact revision and
fixture. A manual hardware failure blocks that hardware row even if every
software-adapter test passes.

Compute report schema 3 requires process-memory observations on both Unix and
Windows. `memory.jsonl` flushes each one-second observation, including errors,
so failures retain partial evidence; `memory.json` retains the final array.
The summary's `memory` field counts valid samples and live-process sampling
errors. Missing, malformed or failed live-process evidence prevents a pass.
An error observed after the child exits is retained as an exit race rather
than a failed live-process sample. No memory value or short smoke approves
bounded residency or default eligibility automatically.

Windows collection uses a read-only process-query handle, closed after each
observation. API definitions and accounting scope follow Microsoft's
[GetProcessMemoryInfo](https://learn.microsoft.com/en-us/windows/win32/api/psapi/nf-psapi-getprocessmemoryinfo)
and [PROCESS_MEMORY_COUNTERS_EX](https://learn.microsoft.com/en-us/windows/win32/api/psapi/ns-psapi-process_memory_counters_ex)
documentation. The hosted Windows parser test samples an actual child process
with a known allocation before the GPU smoke; it also exercises invalid-PID
handling. This instrumentation diagnoses the allocation abort recorded in
#4895; it does not fix that abort or the separate teardown stall in #4890.

The compute runner and manual workflow accept `--profile release` / the
`profile` input to compare sustained work with production build settings.
Default and PR runs retain `ci-test`, including debug assertions and native
validation defaults. The summary records the selected profile and the exact
build command. Keep profile results separate and compare request counts as
well as elapsed time; an optimized build can complete more sweeps. A release
pass does not resolve growth or failures seen with validation enabled, and
neither profile alone qualifies a physical hardware/default-readiness row.

For allocation diagnosis only, `--workload cpu-baseline` (manual workflow
`workload: cpu-baseline`) runs the same two guests, page sweeps and pixel
comparisons without installing compute renderers. It requires zero GPU
requests and null compute-device observations. Schema 4 labels the workload,
sets `gpu_qualification: false`, and reports success as `diagnostic_passed`.
Default/PR runs remain `gpu`; their parser rejects CPU baseline observations.
Keep CPU baseline results separate from GPU qualification. Compare growth per
completed sweep as well as time, since removing readback changes throughput.
All workload, memory, timeout, source and teardown checks remain in place.
