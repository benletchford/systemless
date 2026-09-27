# Host responsiveness review handoff

This change is delivered as one draft for user review. Do not merge or release
before that review. Native qualification now runs on hosted desktops without
access to a developer’s unlocked Mac. Coverage limits remain explicit and the
desktop backend stays opt-in.

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

Subsequent current-source macOS checks passed continuous drag resizing smaller
and larger, fullscreen entry and AppKit Quit. The remaining window-system gates
are now automated with the public Toolbox Showcase fixture.

### Automated native qualification

[Native window run 36284442451](https://github.com/benletchford/systemless/actions/runs/36284442451)
qualified source `1fcfb52` on hosted macOS and Linux (Xvfb with Openbox). Both inline
and owner-thread modes passed all eleven assertions: guest frames reach the window,
initial focus, resize, fullscreen entry at monitor dimensions, fullscreen exit to
normal bounds, exact resize after exit, focus loss, focus return, subsequent frame
progress, cursor/input verification, and production close with owner teardown.
Each case has a JSON report and log in the workflow artifacts.

AppKit may restore remembered normal/zoom geometry when leaving fullscreen. The
probe checks normal window bounds and then requires a successful exact 700×500
resize; it does not assume that AppKit always restores the last requested size.

Cursor verification is layered. Driver tests execute a guest instruction and
verify owned warp retention and matching acknowledgement. The window probe injects
that owned packet at the host boundary and checks the actual OS pointer position.
Linux provides a real cursor event, which must return through the input path to
the matching guest snapshot. macOS deliberately emits no event for cursor warps;
the probe queries AppKit’s pointer position independently, then verifies a separate
mapped input-command round trip. It does not claim physical input latency or a
hardware-generated macOS input event.

The probe is compiled only with `test-support`; ordinary builds contain no
self-driving controls. To reproduce on a machine with a window server:

```sh
cargo build --locked --profile ci-test --bin systemless --features test-support
python3 scripts/verify-native-window.py
```

The `Native window qualification` workflow provisions the Linux virtual desktop
and runs macOS on a GitHub-hosted desktop. It needs no private game archive and no
access to the developer’s desktop. Failures and timeouts are failures, never skips.
The earlier manual checks remain relevant for live dragging, native menus, saves,
and host responsiveness during a controlled owner stall; headless tests do not
replace those observations.

Cross-display DPI transitions, Windows interactive behavior, physical input/display
latency, and broader hardware coverage remain unverified. The available macOS
session had only one virtual display. These limits constrain future default
promotion; they are not silently counted as passing checks.

A repeated Marathon attract-demo failure was independently reproduced on pre-PR
base `a57b757` and source `9ce37d5`: both failed at frontend tick 9,993 after
1,261,141,621 instructions, invalid PC `$6961EDD4`, SP `$00FFFD5C`, following the
same five demo resource loads. All 184 sampled execution/input/fault records
matched. This is a pre-existing failure and those runs remain failed evidence.

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

Full Linux/macOS/Windows, headless/package, licensing, Clippy, website and catalogue
CI passed on `9ce37d5` after the rebase. Later changes add only test-support window
qualification and its workflow; production defaults remain unchanged. Current
source qualification is linked above; see the PR checks for the final general CI
status. No reference images were regenerated to accept changed output.
