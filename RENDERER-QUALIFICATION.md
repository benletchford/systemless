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
GPU allocation accounting. Review growth after warm-up across repeated cycles.
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
longer runs (up to 3600 seconds per renderer/runtime). Virtual/software adapters
qualify correctness only. Windows correctness builds run separately; real Windows
window lifecycle qualification still needs a desktop run of this tool.

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
