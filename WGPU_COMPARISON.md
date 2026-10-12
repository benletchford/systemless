# wgpu comparison on master

This implementation starts from master at `6673f218`. PR #4190 is a reference and the separately built GPUI comparison frontend, not the base or an implementation dependency.

The earlier experiment on `dev/wgpu-comparison` and draft #4723 used the wrong base. Its screenshots and measurements qualify only that earlier experiment. They do not qualify this implementation.

## Port boundary

Reuse master’s MacintoshSession/FixtureRunner execution, input delivery, guest timing and framebuffer APIs. Add the narrow, frontend-neutral snapshot/command support needed for EV controls without importing the GPUI branch’s runtime or unrelated changes. Preserve master’s existing desktop frontend.

The standalone wgpu renderer and sparse GPU probe depend only on wgpu/winit and image readback. The demo now includes master’s production runtime owner, driver, mailbox and frame snapshot modules directly. Runtime policy helpers and keyboard translation are extracted into shared files used by both master’s desktop frontend and the demo. A narrow read-only modal-dialog projection supplies custom controls; unsupported items retain framebuffer pixels.

Neither GPUI nor gpui-kit may be in the resolved demo dependency graph. Compare against #4190 in its own checkout with recorded source/binary identities, equivalent scenes and physical viewports. Account for guest-runtime differences when interpreting results; those differences prevent attributing all observed cost changes solely to the renderer.

## Remaining qualification

Build the master-based demo; validate EV gameplay, pilot text/selection/caret, checkbox/buttons and menus with guest-owned metrics and framebuffer fallback. Re-run headless GPU captures, resizing/scaling and interaction assertions. Then measure both native frontends’ frame delivery, latency, CPU/GPU and memory and probe native IME/accessibility. Native desktop access was previously blocked by the locked Mac; headless results do not replace those checks.

## Master-based evidence

Build with `cargo build --locked --profile ci-test --no-default-features --features wgpu-demo,jit --bin wgpu-comparison`. The resolved normal dependency graph contains no GPUI packages. The demo target passed 37 tests covering the shared production owner, command mailbox, mouse-transition sequencing, save-store and frame-cache behavior.

`wgpu-comparison GAME --offscreen --output DIR --script INPUTS.json --updates 920` exercises the same owner thread without a native surface. The initial EV image and corrected pilot image under `/tmp/wgpu-master-evidence` were inspected. Twenty-four ordinary mouse/key transitions opened New Pilot, typed `WGPU Pilot`, and checked Strict Play. The guest snapshot confirms text `WGPU Pilot`, selection `[10,10]`, visible caret and checkbox value `1`. A prior script swapped the first click’s horizontal/vertical coordinates and reached Preferences; that run is explicitly not a passing pilot test. Media and saves are isolated under `/tmp/wgpu-master-media`.

This is functional evidence only. Native IME/accessibility, complete live instrumentation and equivalent comparison runs remain unfinished. The initial live window/input path has compiled but has not been qualified on an unlocked desktop.

The follow-up run `/tmp/wgpu-master-evidence/pilot-cancel-menu` captured the initial selected name (`[0,12]`), the edited/checked dialog, and a successful raw Cancel click leaving no dialog. A raw File-title click did not open a menu; menu interaction is therefore not qualified and needs an explicit presenter/command path. Producer hashes are recorded in `/tmp/wgpu-master-evidence/producer.json`. Master’s original `systemless` binary also passes `cargo check` after the shared-helper extraction.

## Qualified standard-menu path

The new presenter projects flat, unstyled, icon-free standard menu panes from guest MenuList title cells and MenuInfo dimensions, using the same Menu Manager row/anchor helpers as the guest painter. Unsupported definitions and rows remain on the framebuffer path. Both live and scripted inputs pass through the same presenter; commands carry the full expected snapshot back to the owner, which checks it against current guest state before calling master’s existing `select_guest_menu_item` route.

Three added tests cover selection routing, changed-menu cancellation and Escape/key-up consumption. `/tmp/wgpu-master-evidence/menu-quit` shows the actual GPU menu and highlighted Quit row. Five scripted mouse actions selected File → Quit; EV executed trap `$A9F4` (`QuitApplication`) and the production owner stopped normally after observation 771. This supersedes the earlier unsuccessful raw-title probe. It is still headless interaction evidence, not native display latency.

## Master-based gameplay

`/tmp/wgpu-master-evidence/flight` completed sixteen raw mouse/key transitions over 2900 publications in 48.44 seconds: New Pilot, Return acceptance of pilot/ship dialogs, Enter Ship, departure, held thrust and turning. Final spaceflight and HUD were visually inspected. The actual GPU readback is byte-for-byte RGB identical to the separately exported guest image at scale 1. The run used master’s production audio/save/runtime path with isolated media and saves. This confirms framebuffer fidelity for the sampled gameplay scene, not native frame delivery or a performance advantage.
