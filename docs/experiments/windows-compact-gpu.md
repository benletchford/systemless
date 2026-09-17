# Windows compact GPU presentation — validation in progress

This opt-in D3D11 backend moves final coverage integration off the Windows window thread while preserving the single-pass image produced by the software renderer. The current branch is based on upstream `edf0fd4` (0.41.9), including #1997. Software remains the default. Final performance/latency checks of the corrected fallback implementation are pending an uninterrupted desktop; this is not yet a production-readiness claim.

## Image and queue design

The transport sends one RGB word per ordinary logical pixel and a scale×scale tile only for cells containing retained outline samples. Indexed frames resolve their palette once during export. A pixel shader applies integer area-coverage weights and rounds once, matching the CPU filter. Opaque software overlays replace the affected logical cells. Unsupported sizes or representations use software presentation.

The app maintains one latest prepared image. A worker waits for the DXGI frame-latency event and wakes the window thread, which uploads and submits that image without advancing the guest or repeating CPU composition. A newer guest image replaces an older pending one. This avoids waiting another guest tick for an available display slot, and avoids spending several milliseconds preparing an image after the slot becomes available. All D3D calls remain on the window thread; the worker only owns the wait operation. Shutdown interrupts the wait and joins the worker before the DXGI handle closes.

The two-buffer flip-discard swap chain has maximum frame latency one. A nonblocking Present that reports `WAS_STILL_DRAWING` retains its permission and schedules a 1 ms retry. Pending input/expose requests survive an asynchronous submission, and a resize rejects a pending image with obsolete dimensions. See Microsoft's [waitable-swap-chain guidance](https://learn.microsoft.com/en-us/windows/uwp/gaming/reduce-latency-with-dxgi-1-3-swap-chains) and [Present contract](https://learn.microsoft.com/en-us/windows/win32/api/dxgi/nf-dxgi-idxgiswapchain-present).

## Software fallback correction

The initial prototype reused the top-level HWND for softbuffer after a GPU failure. Microsoft documents that [a flip-model HWND cannot subsequently use GDI, even after the swap chain is destroyed](https://learn.microsoft.com/en-us/windows/win32/api/dxgi/ne-dxgi-dxgi_swap_effect). A simulated Present failure reproduced this: the visible picture stayed on the Maxis splash while the guest continued executing.

The swap chain now targets a disposable disabled child window. The winit parent retains focus, input and native cursor handling, and remains available for GDI. Dropping the presenter releases its D3D resources and destroys the child before releasing its parent reference. Repeating the same failure probe visibly reached SC2K's registration dialog and displayed newly typed text through software rendering.

This validates recovery from reported errors. It is a diagnostic error injection, not a real driver reset or device-removal test.

## Completed checks

- Native Windows GNU release build on 0.41.9; the earlier 0.41.8-base release executable also completed registration, New City, newspaper, city and physical menu input through the child window.
- The release smoke run's 17 GPU readbacks matched the independent CPU oracle over 8,160,000 output pixels.
- Scripted creation failure, simulated removal after 3,000 accepted presents, and one injected busy Present all recovered and closed normally. The busy case continued GPU rendering; its 32 exact readbacks covered 19,891,620 output pixels, including 800×600, 1104×828, 500×375 and 1100×760. The removal case had 14 exact readbacks before fallback.
- The wait-worker test passed on native Windows, checking one notification per arm and shutdown while waiting. The presentation suite passed 34 tests on the same runtime base before the release-only version bump.
- Fire and the National Guard dialog were verified in the preceding pacing version with 36 exact readbacks. The final child-window version still needs that GUI check repeated.

Some later visible resize captures were partially offscreen, and a run received extra keyboard input. Those captures do not establish a complete visible-window resize pass. The capture helper now rejects offscreen geometry; these checks and final timings must be repeated without concurrent desktop interaction.

## Preliminary pacing/latency results, before the child-window correction

These results belong to V5, not the final child-window build. Same instrumented executable, current-master software path versus GPU; Ryzen 7 5700U / AMD Radeon laptop, 1920×1080 at approximately 60.05 Hz, AC, 98% battery, Balanced, saver off. No builds, tests, readback verification or other Systemless windows overlapped the measured stages. Fresh cities are not instruction-identical replays.

| Active city | Software median main-thread work | GPU median main-thread work |
| --- | ---: | ---: |
| 800×600 | 7.26 ms | 6.76 ms |
| 1104×828 | 20.61 ms | 6.84 ms |

GPU work includes asynchronous readiness retries. Software submitted about 35.8 scaled-city frames/s; GPU's DXGI statistics reported about 58.9 presentations/s. The native GPU city reported 58.5/s, so occasional refresh misses remain. The New City dialog reported 60.05 presentations/s at both sizes. Submission counts and DXGI presentation statistics are distinct metrics; [Microsoft's definitions](https://learn.microsoft.com/en-us/windows/win32/api/dxgi/ns-dxgi-dxgi_frame_statistics) explain why Present call counts alone are insufficient.

Paused-city menu trials used Desktop Duplication to identify the first desktop image containing the dropdown, starting from an injected mouse press. All 20 trials per size/backend succeeded, with no detected pointer interference.

| Output | Software median / p95 | GPU median / p95 |
| --- | ---: | ---: |
| 800×600 | 38.14 / 48.67 ms | 42.15 / 53.01 ms |
| 1104×828 | 78.88 / 88.51 ms | 42.20 / 48.71 ms |

The scaled result is promising; the native result does not demonstrate a latency improvement. These are OS desktop-update observations, not physical input-to-photon measurements. Observer readback return time, injection overhead and coalesced updates were recorded separately. An earlier active-city run had two approximately 420 ms outliers; its subsequent scaled trials were invalidated by the annual budget dialog. Pausing removes that workload from the controlled comparison, so the active-city outliers still need a separate comparison rather than being described as fixed.

Final child-window timings, native-cursor observations and the active-city menu follow-up remain pending. Local raw results, source/binary hashes, fault logs and game-only captures are under `target/windows-validation/gpu-pacing-*`; the earlier experiment remains available in branch `experiment/windows-compact-gpu`.

## Running

Build with `cargo build --release --features gui --target x86_64-pc-windows-gnu`. Set `SYSTEMLESS_D3D11=1` when launching the Windows executable, or leave it unset for software presentation. Native hardware cursors are independent of this choice.

`SYSTEMLESS_GPU_VERIFY=1` enables synchronous GPU readback and a full CPU oracle. Use it only for correctness checks; it materially changes timings. `SYSTEMLESS_GPU_CAPTURE_DIR` can name an existing directory for verified output images.

No font design, guest metrics, guest clock or guest execution optimization changes are part of this backend. Coppet is a separate PR (#2007).
