# wgpu comparison on master

This implementation starts from master at `6673f218`. PR #4190 is a reference and the separately built GPUI comparison frontend, not the base or an implementation dependency.

The earlier experiment on `dev/wgpu-comparison` and draft #4723 used the wrong base. Its screenshots and measurements qualify only that earlier experiment. They do not qualify this implementation.

## Port boundary

Reuse master’s MacintoshSession/FixtureRunner execution, input delivery, guest timing and framebuffer APIs. Add the narrow, frontend-neutral snapshot/command support needed for EV controls without importing the GPUI branch’s runtime or unrelated changes. Preserve master’s existing desktop frontend.

The standalone wgpu renderer and sparse GPU probe depend only on wgpu/winit and image readback. The demo now includes master’s production runtime owner, driver, mailbox and frame snapshot modules directly. Runtime policy helpers and keyboard translation are extracted into shared files used by both master’s desktop frontend and the demo. A narrow read-only modal-dialog projection supplies custom controls; unsupported items retain framebuffer pixels.

Neither GPUI nor gpui-kit may be in the resolved demo dependency graph. Compare against #4190 in its own checkout with recorded source/binary identities, equivalent scenes and physical viewports. Account for guest-runtime differences when interpreting results; those differences prevent attributing all observed cost changes solely to the renderer.

## Remaining qualification

Headless gameplay, pilot interactions, standard menus and renderer resize/scaling probes have passed as described below. Remaining work is an equivalent paired comparison and native frame delivery, latency, CPU/GPU and memory measurements, plus native IME/accessibility probes. Native desktop access was previously blocked by the locked Mac; headless results do not replace those checks.

## Master-based evidence

Build with `cargo build --locked --profile ci-test --no-default-features --features wgpu-demo,jit --bin wgpu-comparison`. The resolved normal dependency graph contains no GPUI packages. The demo target passed 39 tests covering the shared production owner, command mailbox, mouse-transition sequencing, save-store and frame-cache behavior.

`wgpu-comparison GAME --offscreen --output DIR --script INPUTS.json --updates 920` exercises the same owner thread without a native surface. The initial EV image and corrected pilot image under `/tmp/wgpu-master-evidence` were inspected. Twenty-four ordinary mouse/key transitions opened New Pilot, typed `WGPU Pilot`, and checked Strict Play. The guest snapshot confirms text `WGPU Pilot`, selection `[10,10]`, visible caret and checkbox value `1`. A prior script swapped the first click’s horizontal/vertical coordinates and reached Preferences; that run is explicitly not a passing pilot test. Media and saves are isolated under `/tmp/wgpu-master-media`.

This is functional evidence only. Native IME/accessibility, complete live instrumentation and equivalent comparison runs remain unfinished. The initial live window/input path has compiled but has not been qualified on an unlocked desktop.

The follow-up run `/tmp/wgpu-master-evidence/pilot-cancel-menu` captured the initial selected name (`[0,12]`), the edited/checked dialog, and a successful raw Cancel click leaving no dialog. A raw File-title click did not open a menu; menu interaction is therefore not qualified and needs an explicit presenter/command path. Producer hashes are recorded in `/tmp/wgpu-master-evidence/producer.json`. Master’s original `systemless` binary also passes `cargo check` after the shared-helper extraction.

## Qualified standard-menu path

The new presenter projects flat, unstyled, icon-free standard menu panes from guest MenuList title cells and MenuInfo dimensions, using the same Menu Manager row/anchor helpers as the guest painter. Unsupported definitions and rows remain on the framebuffer path. Both live and scripted inputs pass through the same presenter; commands carry the full expected snapshot back to the owner, which checks it against current guest state before calling master’s existing `select_guest_menu_item` route.

Three added tests cover selection routing, changed-menu cancellation and Escape/key-up consumption. `/tmp/wgpu-master-evidence/menu-quit` shows the actual GPU menu and highlighted Quit row. Five scripted mouse actions selected File → Quit; EV executed trap `$A9F4` (`QuitApplication`) and the production owner stopped normally after observation 771. This supersedes the earlier unsuccessful raw-title probe. It is still headless interaction evidence, not native display latency.

## Master-based gameplay

`/tmp/wgpu-master-evidence/flight` completed sixteen raw mouse/key transitions over 2900 publications in 48.44 seconds: New Pilot, Return acceptance of pilot/ship dialogs, Enter Ship, departure, held thrust and turning. Final spaceflight and HUD were visually inspected. The actual GPU readback is byte-for-byte RGB identical to the separately exported guest image at scale 1. The run used master’s production audio/save/runtime path with isolated media and saves. This confirms framebuffer fidelity for the sampled gameplay scene, not native frame delivery or a performance advantage.

## Resize and timing instrumentation

The same publication-driven script now drives native and offscreen modes, preserving mouse hold intervals when readiness delays an action. `--resize-probe` exercises 800×600, 1200×900, 1600×1200, 1000×600, 600×1000, zero-size suspension and restoration. The GPU run in `/tmp/wgpu-master-evidence/resize` passed draw/suspension and inverse pointer-mapping assertions. Landscape letterboxing and fractional-scale pilot captures were inspected. These are renderer checks, not OS resize or display-change notification checks.

`--seconds N` bounds native runs and writes `live-report.json` with compose/submit CPU time, present-call gaps, guest sequence, surface size and scale factor. Native reports are implemented but have not been exercised on an unlocked desktop. Present-call gaps cannot establish compositor delivery.

Measured commands retain an enqueue timestamp through the existing mailbox, preserving adjacent mouse-motion coalescing. The next published snapshot carries the latest dequeued command receipt. This measures enqueue-to-snapshot consumption; it does not prove that the guest applied that command or that its effect was displayed. Intermediate receipts can be coalesced. Tests cover receipt ordering and motion coalescing; the original desktop target still passes `cargo check`.

A fresh isolated pilot run (`/tmp/wgpu-master-evidence/receipts`) completed 920 publications and all 24 actions in 16.64 seconds. The guest snapshot confirms `WGPU Pilot`, selection `[10,10]`, caret visible and Strict Play checked. All 24 receipts were observed: median 20.12 ms, p95 27.36 ms. Offscreen composition/submit time was median 0.867 ms, p95 1.285 ms, excluding framebuffer conversion and readback. Thirty sparse GPU render-pass samples were median 0.174 ms, p95 0.290 ms; these exclude upload, compositor and display. This single run is diagnostic evidence, not a controlled frontend speed comparison.

## Platform integration and provisional decision

Source inspection finds no winit IME event handling, composition state, candidate-window positioning or accessibility semantic tree in this master-based demo. Ordinary keyboard editing is therefore not evidence of IME or screen-reader support. A proper integration needs guest-owned selection/text mutation, composition cancellation and focus handling, candidate positioning through the viewport transform, and semantic nodes/actions whose state comes from guest snapshots. The locked Mac prevents a native behavioral probe; none is claimed here.

Do not migrate on the present evidence. The prototype demonstrates direct control of rendering, guest geometry and framebuffer fallback with no GPUI dependency. It also exposes platform-service work that wgpu itself does not provide. Equivalent native measurements and platform integration validation remain required to decide whether that additional ownership yields enough simplicity or measurable improvement. The earlier PR-based experiment is not a substitute for this master-based comparison.
