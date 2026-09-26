# Browser worker responsiveness

The worker owns Macintosh execution; the page owns DOM, input capture, audio
output and presentation. Ordered commands carry a runtime generation and command
sequence. Frame sequence is independent of Macintosh TickCount. Movement only
coalesces with adjacent pending movement. The bridge bounds transport to eight
commands in flight and 256 pending, with one reserved shutdown request. Save
operations have a separate bound of 32 pending acknowledgements.

Normal navigation releases page resources, asks the owner to flush IndexedDB
transactions, then terminates it. A replacement launch of the same game waits for
that flush. Startup failures can use retained archive/plugin data for observable
compatibility fallback. Failures after startup stop visibly without rebooting.

## Measured startup and menu workloads

Measurements used release Wasm, macOS 26.5.2, Apple M1 with 8 GiB RAM, and
headless Chrome 151.0.0.0. Each pair used a fresh browser profile and empty saves,
identical catalogue archives/settings, no guest input and no debug overlay.
Three repetitions covered GPU-enabled WebGL and GPU-disabled Canvas2D. Analysis
uses the common guest-time interval near ticks 900–1800; reports retain exact
observed tick/instruction endpoints because frame sampling can overshoot.
These are startup/menu cases, not claims about full gameplay performance.

| Game / presentation | Main-thread callback p99, before → after | ms per guest tick, before → after | Cold startup median, before → after |
| --- | --- | --- | --- |
| Marathon / WebGL | 13.0 → 0.5 ms | 16.632 → 16.689 | 906.0 → 1004.6 ms |
| Marathon / Canvas2D | 12.6 → 0.5 ms | 16.629 → 16.708 | 890.5 → 1044.0 ms |
| Glider PRO / WebGL | 1.3 → 0.3 ms | 16.648 → 16.646 | 664.6 → 696.7 ms |
| Glider PRO / Canvas2D | 1.3 → 0.2 ms | 16.648 → 16.647 | 446.1 → 542.4 ms |
| EV Nova / WebGL | 0.5 → 0.5 ms | 16.752 → 16.752 | 8165.7 → 7941.6 ms |
| EV Nova / Canvas2D | 0.4 → 0.4 ms | 16.744 → 16.729 | 7863.2 → 7918.2 ms |

All 18 before/after screenshot pairs were byte-identical. Nova already used
workers in the baseline. Its roughly 113 ms tail round trips remain in both
versions and fail the existing 50 ms gate. Worker execution does not impose a
hard deadline on long guest operations. The baseline Marathon loading-gap gate
also failed; thresholds were not relaxed.

Newly worker-backed games pay additional cold-start overhead. A sampled Marathon
startup spent about 38 ms between posting boot and beginning archive loading,
including worker setup and binding/Wasm initialization. The archive-loading
phase itself remained about 542 ms. Total startup also includes variable page
and audio initialization. This tradeoff is separate from steady-state host
responsiveness and is not presented as a guest-execution speedup.

A controlled 500 ms owner stall left the page's game-information control usable:
its changed layout was visible after 17.6 ms, while the owner was still stalled.
Guest rendering resumed afterward. This measures a host control, not a guest's
response to input.

Separate audio diagnostics runs covered Marathon and Nova, three repetitions
per baseline/candidate pair. All twelve runs reported zero underrun blocks and
samples over the measured interval. A diagnostics-only audio worklet was used
on both versions; these runs are separate from the latency table.

Twelve repeated starts closed all twelve workers and audio contexts without
browser errors. Main JavaScript heap observations after explicit collection
showed no sustained growth and ended around 1.74 MB. This does not measure total
process, Wasm or GPU memory. A separate test with real browser clicks and no
Chrome autoplay override verified running audio on launch, plugin enable and
plugin disable, followed by closure of all three contexts.

## Reproduction

Use the catalogue archive URL and a legally obtained matching local archive with
`www/scripts/verify-runtime-pacing-cdp.mjs`. Set `SYSTEMLESS_RUNTIME_TARGET_TICK`
to 1800, `SYSTEMLESS_RUNTIME_SAMPLE_MS` to 90000 as a timeout,
`SYSTEMLESS_RUNTIME_WARMUP_MS` to 3000, `SYSTEMLESS_RUNTIME_DEBUG` to 0, and
`SYSTEMLESS_RUNTIME_GPU` to 0 or 1. Keep raw traces for common-interval analysis;
compare actual endpoints and images rather than wall-time throughput alone.
`SYSTEMLESS_RUNTIME_AUDIO_DIAGNOSTICS=1` enables separate bounded audio and
worker-startup traces. Diagnostics runs should remain separate from primary
latency measurements.

Plugin parity uses a generated two-fork fixture with Finder metadata and a mount
path. The save probe creates a pilot through the game UI, downloads it, deletes
it, imports the identical forks and verifies deletion across immediate restart.
A stale worker protocol also passes that round trip through visible fallback.

Safari and Windows/Linux browser execution have not been qualified on this host.
Compatibility mode remains available through explicit `runtime.worker: false`.

## Experimental renderer worker

On supported browsers, `?renderer=worker` selects an experimental OffscreenCanvas
2D presenter alongside the existing emulation worker. It is opt-in and has not
passed the full presentation qualification gate. Safari retains its existing
Canvas2D path. This capability class accepts complete RGBA images; it does not
coalesce incremental QD3D submissions or enable external QD3D GPU capture.

Adding `&renderer_gpu=1` selects the experimental OffscreenCanvas WebGL
presenter. Its helper loads with the runtime asset identity and reports supported
packet kinds before accepting images. The GPU kernel and bounded transport accept
complete 8-bit indices plus a full palette. Eligible gameplay frames now use
that representation, including a small cursor patch composed by the existing
scalar cursor renderer on the owner. Retained-text frames reuse the native
`CompactPresentation` cells/detail format at integer output scales 1–4. Cursor
composition remains on the owner and replaces only logical pixels changed by
the existing cursor renderer, preserving detail beneath unchanged cursor pixels.
Debug overlays and unsupported layouts retain RGBA export. Backend recovery
requests fresh RGBA from the same owner and invalidates cached RGBA pixels after
an indexed or compact snapshot. Shader-load failure,
context loss and renderer failure use the same presenter-only recovery path.

The logical canvas retains input listeners and focus. A separate display canvas
transfers to the renderer before context creation. One submitted image and one
newest pending image bound the sender queue; at most two returned buffers are
retained. Renderer failure removes the display canvas and initializes Canvas2D
on the logical canvas, using retained pixels or requesting a fresh snapshot from
the same guest. Snapshot recovery also works after guest execution stops.

`data-render-backend` and `data-render-fallback` expose backend selection and
failure. `data-render-sequence` identifies submitted images independently of guest
TickCount. `data-render-roundtrip-ms` measures submission acknowledgement on the
host clock; `data-render-submit-ms` measures the renderer's own submission call.
Neither measures physical display completion. `data-render-packet-kind` and `data-render-packet-bytes` report the last submitted
representation and payload size, including palette and cursor data. Indexed
snapshots copy packed guest pixels into a reusable owned allocation, then copy
the indices, palette and cursor patch into JavaScript buffers. Those buffers
transfer through the host to the renderer, where indices, palette and any cursor
patch are uploaded separately. RGBA fallback still expands on the owner and
copies into JavaScript. These paths are not zero-copy. Direct owner-to-renderer
transport, producer reuse of returned buffers and full pipeline measurements
remain pending. Compact export currently decodes and clones logical ARGB pixels
when composing a cursor, then copies the native cells and detail into JavaScript
buffers. The renderer uploads those two arrays and resolves high-resolution
pixels on the GPU. This avoids owner-side high-resolution expansion but does not
eliminate owner-side snapshot work.

Release browser checks of a retained-text menu produced identical full-page
images at 1×, 2× and 3× device scale, with the same logical 800×600 display and
matched guest progress. The compact payload stayed at 1,962,112 bytes, compared
with expanded RGBA sizes of 1,920,000, 7,680,000 and 17,280,000 bytes respectively.
It is slightly larger at 1×; the reduction applies to higher display scales.
These are single startup/menu pairs, not repeated gameplay qualification. The
3× compatibility run exceeded the existing 50 ms frame gate (75.6 ms maximum);
the compact run passed with a 35.5 ms maximum. No threshold was relaxed, and
these samples do not establish a sustained performance gain. GPU differential
checks also compare all 16 combinations of native detail/output scales 1–4,
including integer area rounding, against native scalar output.


Set `SYSTEMLESS_RUNTIME_PRESENTATION_DIAGNOSTICS=1` when running the runtime
probe to collect separate owner execution, snapshot/conversion and Wasm-to-JS
packet costs. This mode is opt-in and should be run separately from primary
timing comparisons. Owner snapshot time includes presentation bookkeeping;
packet-copy time includes JavaScript packet construction. Audio, saves and
incremental QD3D packet construction are outside these phase measurements.

For renderer-worker packets, the probe correlates transferred buffer identities
with submissions. Host receipt-to-send, renderer round-trip and request-to-ack
intervals all use the same host clock. The worker reports only its local upload
and submission duration. These measurements expose host scheduling/queue waits
but do not establish GPU completion or physical display latency. Raw traces are
bounded and include packet kind, payload bytes and matched guest progress.
