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
transfer through the host to the renderer in the relay path, where indices,
palette and any cursor patch are uploaded separately. RGBA fallback still expands on the owner and
copies into JavaScript. These paths are not zero-copy. Producer reuse of returned buffers and full
pipeline qualification remain pending. Compact export currently decodes and clones logical ARGB pixels
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


Add `&renderer_direct=1` to the experimental renderer query to test direct
owner-to-renderer `MessagePort` transport. This remains an additional opt-in
capability. The owner sends complete RGBA, indexed or compact packets, with one
active and one newest pending image and at most two returned buffers. Audio,
saves, input and host display demand retain their existing routes. The host
receives image metadata and submission notices rather than image buffers.
Cancelled module imports and presenter failures close the port route and request
fresh RGBA from the same running guest. The existing relay path also now retains
the received RGBA typed array directly, avoiding a redundant JavaScript copy.

Submission notices go directly from the renderer to the host; buffer credits
return separately to the owner. A long guest batch can delay credit processing
without delaying the host's submission notice. Layout and input geometry follow
the submitted image, including output scale, rather than a newer pending image
that could be dropped. `data-render-transport=direct` identifies this route and
`data-render-credit-return-ms` reports owner-clock credit round trips. It does
not represent display latency. Direct-route diagnostics measure host request to
renderer submission acknowledgement entirely on the host clock. They leave
relay-only wait/round-trip and in-flight observations unavailable rather than
reporting them as zero.


Initial release comparisons at matched tick 1801 produced exact full-page image
matches for indexed WebGL, retained-text WebGL at device scale 3, and Canvas2D.
With separate presentation diagnostics enabled, median host request-to-submission
acknowledgement changed from 16.8 to 13.2 ms, 19.6 to 14.8 ms, and 17.3 to 13.7 ms
respectively. All six pacing checks passed. These are single startup/menu pairs
on one Chrome/macOS host; retained-text transition tails varied between runs.
They do not qualify sustained gameplay, end-to-end visible input response or
other platforms. Direct routing remains opt-in pending broader qualification.


A direct-route navigation check covered 12 owner starts, including cancelled
startup and a controlled runtime crash, plus held-key focus release and touch
release coordinates. All 12 owners, 11 created renderer workers and 12 audio
contexts closed after navigation, without browser errors. Collected main
JavaScript heap samples ranged from 7.52 to 9.51 MB and ended at 7.67 MB. This
short lifecycle check does not measure total process, Wasm or GPU memory. A
separate injected renderer crash during compact presentation preserved the same
guest and input canvas and recovered to a nonblank Canvas2D image.


An extended direct-route check completed 50 navigation/restart cycles in about
400 seconds, plus cancelled startup and a controlled runtime crash. All 52
runtime workers, 51 renderer workers and 52 audio contexts closed, with no
browser errors. Median main JavaScript heap in the first and last ten teardown
samples was 7.669 and 7.735 MB. Summed Chrome process-tree RSS ranged from 471 to
1,848 MiB and ended at 714 MiB. RSS can count shared pages more than once and
varies with resident-page pressure; compilation was active during part of this
run. These measurements do not isolate GPU allocations or prove leak absence.

A separate controlled check stalled the owner for 500 ms immediately after
sending a direct image, before its host metadata reply. The renderer acknowledged
that exact packet and a host control painted while the owner was still busy.
The guest resumed without restarting, and worker/audio teardown completed.
This tests independent delivery during a stall, not normal frame-time performance.


## Repeated gameplay and delayed presentation

Twelve additional release runs exercised Marathon's playable world and HUD on
Chrome 151 / Apple M1: the existing main-thread WebGL presenter, the renderer
relay and direct transport, each with one warm-up and three measured repeats in
alternating order. All three kept guest execution on the same worker backend,
with 25 MHz emulation, an 800×600 display at device scale 1, fresh saves and the
same archive and scripted inputs. Initial Macintosh time was fixed at
3,871,497,600 seconds; the monotonic performance clock and pacing stayed live.
All twelve existing pacing checks passed.

The comparison uses common guest ticks 3600–4100, after level loading and before
a held movement key. Median milliseconds per guest tick were 17.641, 17.609 and
17.607 respectively, a difference below 0.2%. Retired instructions per guest tick
differed by less than 0.6%. Median run-level host frame-interval p99 was 18.5,
18.3 and 18.5 ms; callback p99 was 0.1 ms for all three. This case establishes
no material throughput regression and does not show a substantial speedup over
the already worker-backed runtime.

Real-time execution and input receipt can cross a guest-tick boundary. Raw
traces retain actual tick/instruction endpoints. The later key release is
scheduled after completed host frames, so final player positions can differ;
these gameplay screenshots are not claimed as instruction-identical replay or
pixel-parity evidence. Independent scalar/GPU and owned-packet tests establish
conversion correctness. Audio queue observations are separate from the earlier
instrumented underrun checks.

This HUD uses compact retained presentation: its packet is 2,065,792 bytes versus
1,920,000 bytes for RGBA at 1×. Compact transport is therefore not a bandwidth
improvement for this scale. Higher-resolution text benefits and renderer
submission costs must be assessed separately.

The fixture-free GPU probe also exercises the production direct owner/renderer
protocol under an injected 80 ms paint-scheduling delay. Two bursts of 500 packets
cover RGBA/indexed/compact mode changes and palette-only changes. Only sequences
1, 500, 501 and 1000 are submitted; GPU readback matches the corresponding scalar
images exactly. Both bursts drain with no pending image and at most two returned
buffers. This is a queue/correctness test, not a normal-performance measurement.


Separate phase diagnostics in that same pre-movement interval observed only eight
complete images per path; the world view was mostly stationary. Median owner
snapshot/export time was 1.9 ms for RGBA and 0.5 ms for compact presentation.
JavaScript packet construction took 0.2 and 0.3 ms respectively. The direct
renderer submitted compact images in a median 2.8 ms. These small samples show
work moving off the owner; they do not demonstrate lower total presentation
cost or sustained animated-frame performance at 1×.

Returned JavaScript buffers remain bounded but producer reuse is deferred.
Reusing them could remove allocation, not the required Wasm-to-JavaScript copy.
Packet construction was not the dominant measured phase in these cases, and the
sustained teardown check did not show continuing main-heap growth. Animated
workloads and allocation-specific profiling remain necessary before broadening
that API. Native snapshot storage already reuses its owned allocations.


Clipped CSS scaling remains unqualified. On the current Chrome build, an 800×600
image displayed at 798×598 differs from the existing presenter at 155 pixels on
two rows. Canvas bounds and owned RGBA/expanded-indexed bytes agree; integer-size
images agree exactly. A clipped-container synthetic probe reproduces boundary
sampling differences, while the corresponding unclipped probe matches. Changing
layer promotion, paint containment or WebGL context attributes did not resolve
it. The experimental backend remains opt-in, and this result is not counted as
full fractional-scale image parity.


## Animated gameplay and visible response

A separate eight-run release comparison held the camera-turn key during Marathon
world rendering: one warm-up and three measured repeats each for the main WebGL
presenter and direct compact renderer, alternating order. The same owner-worker
runtime, fresh saves, fixed startup date, 25 MHz setting and device scale 1 were
used. Keydown and keyup followed guest-tick milestones 3500 and 4500. All eight
pacing checks passed. Common ticks 3600–4100 contained hundreds of changed images,
so this exercises continuous drawing rather than the earlier stationary view.

| Median of three run-level measurements | Main WebGL | Direct compact |
| --- | ---: | ---: |
| Milliseconds per guest tick | 16.763 | 16.672 |
| Retired instructions per guest tick | 215,095 | 214,679 |
| Host callback p99 | 0.6 ms | 0.2 ms |
| Host frame-interval p99 | 18.4 ms | 18.4 ms |
| Owner request/reply median | 9.7 ms | 8.6 ms |
| Owner request/reply p99 | 18.1 ms | 16.6 ms |

This case shows no material guest-throughput regression, with less host callback
work. It does not establish a substantial gameplay speedup or reduced physical
input latency. The compact HUD still carries more bytes than RGBA at 1×.
Actual instruction and input endpoints are retained; real-time runs are not
instruction-identical replay. These probes did not overlap other browser probes
or builds launched for this comparison, but background host load was not isolated.

Repeating that comparison at device scale 3 (2400×1800 backing images displayed
at 800×600 CSS pixels) also passed all eight pacing checks. Each presenter had
one warm-up and three measured repeats, again alternating order and using the
common guest interval with continuous camera rotation.

| Median of three run-level measurements, scale 3 | Main WebGL | Direct compact |
| --- | ---: | ---: |
| Milliseconds per guest tick | 16.714 | 16.667 |
| Retired instructions per guest tick | 216,030 | 214,679 |
| Host callback p95 / p99 | 6.2 / 6.9 ms | 0.1 / 0.2 ms |
| Host frame-interval p99 | 18.4 ms | 18.5 ms |
| Owner request/reply median / p99 | 15.3 / 24.3 ms | 8.6 / 17.8 ms |
| Complete-image payload | 17,280,000 bytes | 2,065,792 bytes |

This high-resolution case reduces host callback work and owner roundtrip time,
with guest time per tick differing by less than 0.3% and retired work per tick
within 0.7%. It demonstrates a benefit for this capability class without a
material throughput regression. Frame-interval p99 remains similar. Audio queue
observations do not replace instrumented underrun checks, and the unresolved
clipped fractional-layout difference still prevents a general default rollout.

A separate settled-scene input diagnostic sampled composited screenshots of a
world-view region, with 42 main-presenter and 41 direct-renderer captures unchanged
before a camera-turn input. Both showed actual camera movement afterward. Owner
application completed 0.7/0.5 ms after host dispatch, normalized using each clock's
time origin and checked against host acknowledgement. This measures the runner's
input enqueue, not the guest's eventual event consumption. The first changed
capture started/completed 46.3/112.9 ms after dispatch for main WebGL and
61.6/109.2 ms for direct compact rendering. Capture overhead is substantial:
these are single-event conservative visible-response upper bounds, not physical
monitor latency, latency percentiles or evidence of a speedup. Inputs were sent
at completed-frame boundaries, not worst-case random phases. An earlier attempt
whose scene changed before input was retained as inconclusive and excluded.

Separate 3× display-scale diagnostics sampled 261/267 changed images over the
same common guest interval. Main RGBA versus direct compact export took a median
6.9/0.5 ms on the owner, with Wasm-to-JavaScript packet construction at 1.5/0.2 ms.
Payloads were 17,280,000/2,065,792 bytes. Compact renderer submission took a median
2.8 ms (p99 3.0 ms), and host request-to-submission acknowledgement was 11.6 ms
(p99 21.4 ms). These are separate phase measurements on local clocks, not summed
cross-thread timestamps or GPU-completion measurements. This diagnostic pair
demonstrates reduced owner conversion/copy work; it is not a repeated primary
timing comparison.

The initial direct diagnostic report exceeded 4 MiB and lost its CDP connection.
A fixture-free large-response check reproduced the failure. Retrieving the
complete report in bounded chunks succeeded, preserving all samples and the
existing pacing gates; the runtime probe now uses that transport and rejects
closed or timed-out connections explicitly. The failed attempts remain excluded
from timing evidence.
