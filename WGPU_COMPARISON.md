# wgpu comparison prototype

This experiment starts from PR #4190 at `b46d7f6c92a6c339044146f6d60adeb055525f89`. It is a separate executable, not a replacement desktop frontend. Issue: [#4721](https://github.com/benletchford/systemless/issues/4721).

**Current conclusion: the evidence does not justify migration.** The narrow renderer gives direct control over texture uploads, geometry, GPU passes, and surface recovery. It also makes the application responsible for text services, accessibility, focus, menus, clipping, and input lifecycle. The prototype has working GPU-composed EV scenes, but native frame delivery and physical interaction remain unqualified. Lower offscreen resource usage, if observed, would not establish a live improvement or equivalent frontend coverage.

## Build and run

```sh
cargo build --locked --profile ci-test --no-default-features --features wgpu-demo,jit --bin wgpu-comparison
cargo tree --locked --no-default-features --features wgpu-demo,jit -e normal --prefix none
# The resolved tree must contain no gpui, gpui-kit, or other GPUI packages.
target/ci-test/wgpu-comparison /path/to/escape-velocity.bin --screen-depth 8
```

The executable refuses compilation with the `gui` feature. Neither `gpui` nor `gpui-kit` is in its resolved dependency graph. Their optional declarations remain in the repository manifest for the existing frontend. GPU rendering uses wgpu 29 on Metal in the tested environment; winit supplies the native window/input loop and AccessKit supplies the accessibility adapter. No software presentation or GPUI rendering fallback is used by this executable.

The shared worker and text-plan qualification are included from `src/bin/desktop/guest_worker.rs` and `guest_text_plans.rs` by both frontends. The existing CPU frame advance, cadence, mailbox, audio clock, semantic activation, clipping, composition, guest key translation, and save-store implementations are reused directly. Files retaining `gpui_demo_*` names contain shared code; names do not imply a dependency. Only their GPUI presentation functions are feature-gated.

## Ownership and scope

- EV gameplay remains the complete guest framebuffer, uploaded as BGRA without a colour-space conversion. The same guest-to-surface transform serves rendering and pointer lookup. Resizing letterboxes the guest; it does not resize the emulated display or reflow guest UI.
- Supported dialog buttons, checkboxes, and single-line editors use custom GPU quads. Guest snapshots supply item bounds, enabled/pressed/value state, selection, caret phase, and text layout. Font Manager strikes and resolved outlines supply glyph ink and advances. The frontend does not shape committed guest text with a host UI font.
- Resource pictures, icons, unknown definitions, unsupported editable layouts, and application-drawn content retain framebuffer pixels. The pilot dialog's explanatory text is a resource picture and remains intact. Arbitrary styled TextEdit presentation is not newly qualified by this demo.
- Supported classic standard menus use live MenuList title hit cells, MenuInfo dimensions, and shared Menu Manager layout functions. Selections carry the menu handle and generation through the same validated worker command as GPUI. Flat, icon-free, non-scrolling standard panes are supported. Hierarchical/custom/icon-bearing/scrolling panes and PowerPC menu projection remain outside this narrow presenter.
- Native physical keys, mouse inputs, semantic activations, and committed text return through the guest worker. The demo never edits EV state directly to produce a scene.

## Reproducible interaction and captures

Set `SYSTEMLESS_COMPARISON_SCRIPT` to an external JSON action array. Both live frontends consume it from the shared comparison driver. Existing PR #4190 EV action arrays are reusable: `after_updates`, mouse/key down/up, `commit_dialog_text`, `select_dialog_text`, `activate_dialog_button`, and bounded readiness checks. Commercial media and EV scripts remain outside tracked source.

```sh
SYSTEMLESS_COMPARISON_SCRIPT=/path/to/pilot-actions.json \
  target/ci-test/wgpu-comparison /path/to/escape-velocity.bin \
  --screen-depth 8 --offscreen --updates 1050 --capture-at 700,820,900 \
  --scale 1.5 --output /tmp/wgpu-pilot
```

Offscreen mode uses the production guest worker and the actual wgpu pipeline. Named PNGs are GPU readbacks; adjacent `.guest.png` files retain the unmodified guest image, and JSON sidecars record guest dialog state. The final report distinguishes CPU composition/submission time, sparse GPU render-pass timestamps, and software event-to-acknowledged-snapshot latency. It is not a native frame-delivery benchmark. `--capture-menu ID` renders a supported menu at the final observation for visual inspection; it does not by itself qualify pointer or keyboard interaction.

For GPUI capture, use its existing `--capture-application`, `--capture-application-inputs`, `--capture-application-updates`, and `--capture-scale` options. Confirm the output dimensions: on this Mac, GPUI capture scale also multiplies by the host's 2× display density. GPUI `--capture-scale 0.75` and wgpu offscreen `--scale 1.5` both produce 1200×900 pixels from an 800×600 guest. Using the same numeric flag is not an equivalent physical-size comparison.

## Validation recorded so far

Host: Apple M1 integrated GPU, Metal, 8 GiB memory. Builds use `ci-test` and the PR's m68k/ppc dependencies. These are functional-profile results, not production release qualification.

- The shared GPUI-free test target passed 116 tests, including guest text metrics/composition, input/activation, clipping, cadence, mailbox delivery, menu identity, and accessibility lifetime rejection.
- EV pilot workflow: eight scheduled actions over 1050 publications; committed `GPUI Pilot`, selection/caret `[10,10]`, Strict Play value `1`, then Cancel leaves no dialog. GPU captures at physical scales 1, 1.5 and 0.75 were inspected. Initial selection was inspected at 1.5. The scale-1 cancelled scene matches the complete guest RGB image exactly.
- EV gameplay: fourteen scheduled actions over 2900 publications, including held thrust and turning, completed in approximately 50 seconds. The final full scene/HUD was inspected; its GPU readback matches the complete guest RGB image exactly at scale 1.
- An equivalent GPUI edited/checked pilot capture completed all six requested actions with no readiness delay. Matching physical dimensions were checked separately from numeric capture scale.
- Native desktop inspection was blocked by the locked Mac. No native presentation, physical latency, live resize/display-density transition, native IME, or accessibility-client result is claimed.

Early evidence lives in `/tmp/wgpu-comparison-evidence`, with logs in `/tmp/wgpu-*.log` and `/tmp/gpui-*.log`. Those runs preceded the final menu/input-timing additions and must not be relabelled as qualification of those additions. Preserve producer hashes alongside subsequent runs.

## Subsequent menu and input-receipt evidence

The build identified by binary SHA-256 `3736e4319d8b9668151bc80c247f939cc55a42d7424ac3e0e06d8c1216e7cd2c` rendered the File/Quit menu over EV using guest title and pane geometry. This was inspected as an actual GPU readback, not a native interaction test. The same build completed the eight-action pilot workflow at 1600×1200 (2×), including committed text, checked Strict Play, and dismissal. Four raw mouse events yielded acknowledged-snapshot times of 33.041, 33.333, 33.325, and 33.341 ms. Semantic text/button commands are not included in those four receipts. This small startup/dialog sample cannot establish a latency advantage.

Evidence is under `/tmp/wgpu-comparison-evidence/menu` and `final-pilot-2`, with producer metadata in `producer.json`. The final code additionally makes the live `--seconds` deadline independent of redraws, so an occluded/idle window can terminate a measurement. That event-loop-only change was covered by compilation and the 116-test target; native timeout behavior remains unqualified.

The matched 1200×900 GPUI pilot image was visually inspected. It uses the existing modern control skin; the wgpu prototype uses a simple custom skin with guest geometry and text metrics. Both show the edited pilot name, checked Strict Play, resource artwork and Cancel/OK controls. Appearance equivalence is not pixel identity for custom controls. Framebuffer-only gameplay and cancelled-dialog scenes were checked separately for exact RGB equality.

## Headless resize probe

`--offscreen --resize-probe` cycles the same final guest snapshot through 800×600, 1200×900, 1600×1200, 1000×600, 600×1000, zero size, and 800×600 again. It uses the real GPU target allocation/draw/readback path, checks suspension at zero size, and checks inverse pointer mapping at the first, middle and last guest pixel centres after each nonzero resize. The pilot run completed all six actions and all checks; landscape and portrait letterboxed images were inspected. Evidence: `/tmp/wgpu-comparison-evidence/resize-pilot`.

That single instrumented run took 15.99 s wall time, 4.87 s user CPU and 1.10 s system CPU, with maximum RSS 133,087,232 bytes and peak footprint 154,846,096 bytes. These figures include startup, PNG readbacks and resize allocations. They are neither a steady-state budget nor a comparison with GPUI. This probe does not create a native surface or test compositor/DPI notifications.

## Measurement contract and remaining live protocol

Use the same build profile, archive hash, guest architecture/depth, scene actions, physical viewport, display refresh rate, audio setting and foreground state. Warm up before steady-flight sampling. Run sequential alternating GPUI/wgpu repetitions with builds and other heavy work stopped. Record host load and the exact binary/source hashes. Keep tracing runs separate from uninstrumented CPU/memory runs.

For wgpu, `--seconds N --report /tmp/report.json` bounds a live sample. `SYSTEMLESS_COMPARISON_METRICS=1` enables optional physical-input receipts in GPUI; the shared scripted driver already uses receipts. A receipt is attached only when the worker delivers an input and travels in its next snapshot. It measures software transport and submission, not proof of a visible guest effect and not input-to-photon latency. Validate semantic response with the guest state or displayed pixel transition. Mouse-motion receipts may coalesce; report their sampling boundary rather than treating them as every input.

| Dimension | Available instrumentation | Qualification still required |
| --- | --- | --- |
| Live frame delivery | wgpu present-call intervals; common worker/publication profiling | Native presented-drawable/compositor trace for both frontends; dropped/repeated frames and p50/p95/p99/max intervals |
| Input latency | Common event-to-worker-snapshot receipts; wgpu receipt-to-submit endpoint | Native event delivery plus visible response, keyboard and mouse separately; physical input-to-photon needs external observation |
| CPU | Composition/submission samples; `/usr/bin/time -l`; Time Profiler | Matched live steady-flight and dialog runs, including idle and menu-open phases |
| GPU | Sparse asynchronous wgpu timestamp queries every 30 renders | Metal System Trace for both frontends, including uploads, copies, compositor and scheduling costs |
| Memory | Process peak RSS and footprint from `/usr/bin/time -l` | Matched live plateau, resize cycles and repeated dialog/menu open/close; GPU allocations from native trace |
| Resize/scaling | Shared aspect-preserving render/input transform; offscreen scale captures | Drag-resize, minimize/restore, monitor/DPI transition, fractional scale, pointer/caret/AX bounds in physical coordinates |
| IME | winit preedit/commit events, shared guarded composition, guest-caret candidate positioning | Native candidate-window placement, composition/cancel/commit and focus transitions |
| Accessibility | AccessKit roles, bounds, checkbox state, button press and guarded field-value replacement | Native AX client/VoiceOver delivery, focus navigation, selection and text-range APIs |

Xcode provides `Metal System Trace`, `Time Profiler`, `Animation Hitches`, and `Allocations` templates on this host. For example, use `xcrun xctrace record --template 'Metal System Trace' --time-limit 60s --output /tmp/wgpu.trace --launch -- /absolute/path/to/wgpu-comparison GAME --seconds 55 --report /tmp/wgpu-live.json`, then repeat against the GPUI executable with equivalent scene inputs. Inspect the trace before claiming actual presented-frame or GPU-cost results. The template's presence does not establish that capture permissions or counters work.

## IME and accessibility assessment

wgpu itself supplies neither text services nor accessibility. This prototype explicitly integrates winit and AccessKit. Representable preedit is displayed using guest font geometry without mutating guest text; commits use the existing Mac Roman validation and lifetime guards. Unrepresentable preedit relies on the OS candidate UI and is not painted with substitute question marks. Unsupported commits are rejected rather than silently changing text. This is incomplete multilingual host editing, not IME parity with GPUI.

The accessibility prototype exposes the active pilot dialog, physical bounds, button actions, checkbox values, and replacement of the active text value. Actions against closed/recycled dialogs are rejected in tests and again by the worker. It does not yet provide a complete text-range/selection provider, menu accessibility, or the full application/window/document semantics already developed in PR #4190. Native delivery remains untested while the desktop is locked.

The experiment demonstrates a smaller rendering mechanism, not an equivalent mature frontend. A migration decision needs matched native evidence and an explicit cost for rebuilding these platform services. Retain GPUI until those gaps are qualified and a material benefit survives controlled live comparison.
