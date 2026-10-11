# GPUI Kit system-interface coverage

This is the working inventory for presenting Macintosh system UI through GPUI
Kit on both 68K and PowerPC. A row is complete only when the guest state is
observable on both architectures, GPUI renders it without obscuring
application-owned pixels, input returns through the existing Toolbox/event
path, and deterministic behavior and composed-image checks pass. An existing
Toolbox implementation or a visual demo alone does not satisfy that gate.

The guest remains authoritative for state, hit regions, event order, and
callbacks. Unknown or application-defined definitions retain guest pixels.
The opt-in implementation is staged in a draft PR and must not be merged.

| System UI | Existing state and presentation path | Missing GPUI work | Status |
| --- | --- | --- | --- |
| Menu bar and standard menus | `menu_model.rs` supplies `GuestMenuSnapshot`; `gpui_demo.rs` renders menu buttons and popups and dispatches selected items to the guest. | Verify live validation, keyboard equivalents, submenus, tracking order, custom MDEF fallback, and composed captures on both CPUs. | Demo only |
| Window frames and title bars | `window_manager.rs` supplies ordered `WindowFrameSnapshot` records with a guest WindowPtr identity; `gpui_demo_frames.rs` clips standard frame overlays against content and front windows. | Add a generation to distinguish disposed-pointer reuse; verify WDEF variants, nonrectangular regions, activation, drag, zoom, grow, and fullscreen transitions. | Demo only |
| Document gutters and grow box | `gpui_demo_frames.rs` draws 15-pixel edge strips for selected WDEFs; `control_snapshot` can now identify real scrollbar records and their live value/range. | Distinguish actual guest controls from empty gutters, handle tracking and clipping, and avoid covering custom content. | Unsafe to generalize |
| Dialogs and alerts | `FixtureRunner::dialog_snapshot` exposes ordered items, types, global bounds, live text, enabled state, and known default/focus data on both CPUs. The demo overlays standard `dBoxProc` button/static-text dialogs, while the guest handles clicks and dismissal. Offscreen composed captures cover the showcase alert on 68K and PowerPC. | Complete live control values and PPC text selection, generation identity, modality, callback state, custom-item fallback, broader GPUI rendering, and host accessibility. | Standard dialog slice |
| Buttons, checkboxes, radio buttons, popup controls | Both Control Manager dispatchers handle guest controls and their callbacks. `FixtureRunner::control_snapshot` reads live owner, definition, bounds, title, visibility, highlight, value, range, and lifetime generation on both CPUs. Standard document-window buttons, checkboxes, and radio buttons use GPUI Kit; other definitions keep guest pixels. Overlay order now follows the guest's first-created-frontmost draw order, and a standard control intersecting a custom CDEF keeps guest pixels. | Qualify generation behavior across process replacement, guest-reordered and dynamically redrawn controls, host keyboard/accessibility operation, popups, and custom CDEF fallback tests. | Standard controls demo |
| Scrollbars | The same snapshot exposes live scrollbar bounds and value/range. The demo draws arrows and a value-driven thumb for standard document controls, and guest input changes its value on both CPUs. Four composed captures cover initial and changed states. | Qualify thumb drag capture, hold/page tracking, accessibility, and control/window overlap. Document gutters remain decorative until real controls are identified there. | Standard scrollbar demo |
| Lists | `FixtureRunner::list_manager_snapshot` exposes logical cells and guest scrollbar visibility bytes for inspection. | Add presentation identity, selection and scroll state, clipping, input, and custom-cell fallback. | Guest-rendered |
| TextEdit and editable fields | `FixtureRunner::text_edit_snapshot` inspects guest records; dialog items also carry edit selection. | Unify text, selection, caret, composition, focus, clipboard, and guest event semantics. | Guest-rendered |
| Standard File panels | Dialog and file-package paths are guest-rendered. | Identify standard panel items and file state; preserve callbacks and modal timing. | Guest-rendered |
| Cursors and notifications | Guest presentation and event paths remain in the runtime. | Inventory standard versus application-owned imagery and define host/guest ownership. | Unclassified |
| QuickDraw and custom definitions | Framebuffer remains the presentation source. | Mask only verified standard system pixels; keep unknown WDEF, CDEF, MDEF, user items, and application drawing unchanged. | Required fallback |

The dependency order is menus, frames, dialogs, controls, lists/TextEdit, then
Standard File. The first end-to-end gate is one standard modal dialog on each
CPU alongside menus and windows. For every row, record at least one 68K and
one PowerPC case, guest-event trace, Toolbox outcome, and offscreen composed
image before marking it complete. Compare behavior with a reference Macintosh
environment where available, including nested modality and overlapping
windows. Qualify macOS first; treat other desktop platforms and the browser as
separate gates. Keep headless guest execution independent of GPUI.

Reference behavior: *Macintosh Toolbox Essentials* (1992), chapters 3–6
(Menu, Window, Control, and Dialog Managers); *Text* (1993), chapter 2
(TextEdit). In particular, Toolbox Essentials 4-19 describes window coordinate
systems, 4-40 describes update regions, 5-7 through 5-10 describe scrollbar
parts and values, and 6-1 onward covers dialog items and event handling.
