# Host responsiveness review handoff

This change is delivered as a draft for review. It is not approved for merging,
release or a broader default rollout. Additional native qualification is recorded below; coverage limits remain
explicit and the desktop backend stays opt-in.

## Delivery status

| Area | Implemented and verified | Rollout and limits |
| --- | --- | --- |
| Browser execution | Worker lifecycle, plugin/input/save/audio parity, owned transfer and repeated optimized comparisons | Qualified Chrome worker default; explicit compatibility selection preserved. Safari runtime qualification unavailable. |
| Desktop execution | One owner constructs, uses and destroys the guest; bounded commands and snapshots; asynchronous shutdown and save flush | Opt-in with `SYSTEMLESS_DESKTOP_RUNTIME=thread`. Compatibility remains default. |
| Presentation | Indexed/compact owned packets, direct renderer transport, bounded recycling, same-guest failure recovery, bitmap presentation | Experimental flags remain explicit. Bitmap fixes tested fractional layouts and lowers measured high-DPI host work; not a universal browser/GPU guarantee. |
| Parallel graphics | Persistent native row pool, exact differential tests and ordered tracked commit; representative full-operation gain exceeds 20% | Opt-in native participants; small operations and Wasm remain serial. Broader hardware qualification deferred. |

## Native evidence and deferred checks

The desktop test suite with debugger support reports 128 passed, zero failed and
two intentionally ignored. It exercises owner lifetime, stalled initialization,
ordered inputs, retained snapshots, asynchronous termination state, real guest
ExitToShell, audio failure, host-close save persistence and debugger virtual-frame
control. The ignored offscreen Metal showcase capture was run separately and
passed; its rendered text/dialog image was inspected.

A macOS optimized owner-thread run reached Marathon gameplay, opened and
dismissed the native application menu, zoomed/resized with coherent rendering,
and exited with status 0 through the actual AppKit Quit command. These are
functional observations, not matched timing measurements.

A subsequent diagnostic run blocked only the owner for 38.946 seconds. During
that interval the native window zoomed/resized with a coherent retained image,
and its application menu remained usable. Selecting AppKit Quit returned while
the process remained alive; releasing the owner then produced a clean exit with
status 0. This verifies the deferred termination path under a real owner stall;
it is not a latency-percentile measurement.

A PowerPC Nova pilot was created through the native UI in isolated storage.
Closing the window exited with status 0 and persisted both pilot files, including
forks and metadata. A fresh owner-thread launch restored both files; manually
opening the pilot reproduced its name and ship. Automatic last-pilot selection
was not observed. Fullscreen entry rendered correctly.

Nova's guest File → Quit command did not exit in this check. The same command
also failed to exit on the unchanged inline compatibility backend before entering
fullscreen, so this is not demonstrated to be an owner-thread regression. Both
backends exited with status 0 through the window close control. The fullscreen
exit shortcut attempt was inconclusive and is not counted as a pass.

The following remain **unverified**, rather than silently counted as passes:

- Complete focus, cursor-warp, continuous drag-resize, fullscreen-exit and
  cross-display DPI transition coverage.
- Windows/Linux interactive native behavior and broader macOS workload coverage.

A local diagnostic stall hook is excluded from the public implementation. The
production source and optimized binary were restored after preparing that probe.
Headless tests and browser checks do not substitute for the deferred window-system
checks. They are prerequisites for future native default promotion.

## Measurement and CI interpretation

[Browser measurements](www/RESPONSIVENESS.md) include repeated optimized
comparisons, image correctness, same-guest recovery and sustained lifecycle
checks. The bitmap comparison counts the additional host message callback,
not only animation callbacks. Two high-DPI main-WebGL control runs exceeded
the unchanged pacing threshold and remain recorded as failures; all bitmap runs
passed. Nova startup long-frame failures also remain recorded. Steady gameplay
results do not erase those failures or prove physical input latency.

[Graphics measurements](GRAPHICS_PERFORMANCE.md) report full-operation costs,
threshold selection, exact scalar/parallel comparisons and gameplay regression
checks. Neither kernel speedup nor headless execution proves native UI latency.

Website/catalogue CI run 36266595509 passed on browser implementation `f16b5f5`.
Native CI run 36262864995 passed with the unchanged native implementation,
including Linux, macOS, Windows, headless, package, license and Clippy jobs.
Subsequent documentation-only commits are not described as fresh full CI runs.
No reference images were regenerated to accept changed output.
