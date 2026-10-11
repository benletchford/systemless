# System-drawn pixel and callback inventory

This inventory refines [GPUI_COVERAGE.md](GPUI_COVERAGE.md). “Eligible” means a
standard definition can be identified from guest state; it does not claim that
the GPUI presentation, input, or cross-CPU qualification is complete. The
framebuffer is the fallback. Every GPUI element must use guest identity and
bounds, and its action must re-enter the guest event or Toolbox path.

| Surface | 68K owner and redraw path | PowerPC owner and redraw path | Boundary and classification |
| --- | --- | --- | --- |
| Menu bar and standard MDEF menus | `trap/menu.rs` draws the bar/dropdown, tracks held input, and invokes guest MDEFs. Menu mutation, `DrawMenuBar`, `MenuSelect`, and tracking redraw change pixels. | `loader/ppc/dispatch_menu.rs` and `loader/ppc/menu.rs` draw tracked menus, submenus, flash, and save-under restoration; guest menu definition callbacks can run during tracking. | Recognized standard MBDF/MDEF and live MenuHandles are **eligible** through `GuestMenuSnapshot`. Unknown MBDF/MDEF output stays **guest pixels**. GPUI must not synthesize the selected command or discard live menu validation. |
| Standard window structure and grow area | `trap/window.rs` paints content exposure, frame and drag/grow feedback, recalculates visible regions, and calls custom WDEFs. `ShowWindow`, selection, movement, sizing, zoom, and updates can redraw. | `loader/ppc/dispatch_window.rs` paints frame, grow icon, exposure, geometry transitions, and custom WDEF output. | Recognized WDEF frames with rectangular structure and content regions are **eligible** through ordered `WindowFrameSnapshot`; content belongs to the guest. Unknown WDEFs and windows whose intrinsic regions contain complex scanline data stay **guest pixels**. Exact visible-region rectangles clip retained content overlays; malformed or over-fragmented regions keep guest pixels. Real controls in document gutters must be identified before an overlay covers them. |
| Standard modal and modeless DITL | `trap/dialog.rs` draws the dialog shell/items and routes `DialogSelect`, update events, TextEdit, user-item procedures, and control callbacks. | `loader/ppc/dispatch_dialog.rs` draws standard items and routes `DialogSelect` through native TextEdit/control paths and user callbacks. | Recognized standard item kinds in standard WDEFs are **eligible** through `dialog_snapshot`. User items, application composition, and unknown definitions stay **guest pixels**. Modeless occlusion and edit redraw have two-CPU guest replays; nested modality and callback timing still need qualification. |
| Alerts | The Dialog Manager in `trap/dialog.rs` creates and paints the alert template, then drives its modal loop and filters. | `loader/ppc/dispatch_dialog.rs` handles alert resources and callbacks. | Standard alert items are **eligible** as dialogs, but alert-specific icon, button/default, filter, and nested-modal state need **more extraction and testing**. Custom item drawing stays **guest pixels**. |
| Standard controls, including scrollbars and popup CDEF | `trap/control.rs` owns control drawing/tracking, guest CDEF callbacks, value changes, and redraw on show, hide, move, value, highlight, and `DrawControls`. | `loader/ppc/dispatch_control.rs` draws/tracks the same standard families and invokes native or 68K guest definitions as needed. | Recognized control definitions with live owner, range, value, visibility, and generation are **eligible** through `control_snapshot`. Custom CDEF pixels stay **guest pixels**. Scrollbar arrows, thumb and popup labels must reflect guest values; document gutters alone do not prove a control exists. |
| Lists | `trap/toolbox.rs` implements Pack0/Pack1 list selectors and draws cells through LDEF messages or a standard fallback. `LDraw`, selection, scroll, size, and cell mutation repaint. | `loader/ppc/dispatch_list.rs` owns native list records, scrollbars, standard cell drawing, and LDEF callback paths. | Standard LDEF 0 cells are **eligible** through `list_manager_snapshot`. Unknown LDEFs, app drawing over cells, and unmodeled scroll geometry stay **guest pixels**. Selection and visible range remain guest-owned. |
| TextEdit and DITL edit fields | `trap/dialog.rs` holds TextEdit traps and `DialogSelect` edit handling; `TEKey`, selection, update, and focus redraw guest pixels. | `loader/ppc/textedit.rs`, `dispatch_textedit.rs`, and `dispatch_dialog.rs` mutate TERecs and draw standard edit items. | Unstyled text with verified guest line breaks, metrics, selection, and clipping is **eligible** through `text_edit_snapshot` or `dialog_snapshot`. Supported styled document recipes and qualified wrapped PPC dialog TERecs now use original-font GPUI paint with guest line geometry, selection and caret; unsupported styles, justification, or unqualified overlap retain **guest pixels**. Native text-service staging and subrange corrections exist; physical IME, ranges outside the stage and remaining scrolling/lifecycle behavior still need qualification. |
| Standard File Open/Save | `trap/toolbox.rs` draws Pack3 Open/Save panels and handles list, prompt, selection, keyboard, reply, and guest filter hooks. | `loader/ppc/dispatch_standard_file.rs` draws the native panel, list, buttons, and scrollbar, and handles replies and callbacks. | Recognized standard calls are **eligible** through `standard_file_snapshot`. Custom dialog/filter callbacks and unmodeled icons, directory popup, replacement confirmation, or true scrollbar state require **more extraction**; custom panel pixels remain **guest pixels**. |
| Cursors | `trap/dialog.rs` handles classic cursor traps and shared cursor state; the desktop frontend consumes the guest bitmap, mask, hotspot, and visibility from the runner. | `loader/ppc/dispatch_cursor.rs` updates the same shared state; `dispatch_system.rs` also handles cursor compatibility calls. | The shared worker publishes installed mono/colour pixels, masks, hotspot, visibility/level and guest position. `gpui_demo_cursor.rs` paints supported opaque pixels above GPUI overlays using the shared scene transform and clipping. Actual arrow snapshots have selected-scale cross-CPU captures with exact simulated hidden/inactive/outside restoration (`reference/gpui-demo/cursor-shared-paint`). Inverted pixels decline the entire paint plan with a diagnostic; this is an explicit implementation gap, not faithful replacement. Balanced macOS host hiding is implemented but physically unqualified. Resource-built/application-built variants, pointer capture/warps, transform changes, inversion over system overlays and physical hide/show lifecycle remain open. |
| Notifications | `trap/memory.rs` validates and queues NMInstall/NMRemove in the shared process queue, with once-guarded response and system-sound state. | `loader/ppc/import_targets.rs` binds both install/remove; `dispatch_system.rs` uses the same queue identity and native/Mixed Mode response paths. | Text alerts with no mark/icon and nil/system-alert sound are eligible through an exact worker-owned snapshot. Shared GPUI text, acknowledgment and queued-sound output have selected cross-CPU workflow evidence (`notification-system-sound`, `notification-menu-retention`). Sound-only system tones use the shared queue and wait for their channel to finish before response delivery, without alert ownership or foreground suspension; both CPU runner regressions cover muted/unmuted autoremove. The expanded 20-case actual guest workflow verifies sound-only autoremove, compiled responses, AudioBackend output and subsequent menu interaction across four CPU/display modes (23.73s; `notification-sound-only`). Mark/icon/custom-sound stages remain implementation gaps, not a tested rendering fallback. Complete delivery ordering, callback lifecycle and physical/native qualification remain open. |
| QuickDraw, pictures, color and direct screen drawing | `trap/quickdraw.rs`, dialog/window drawing, and guest blits write the framebuffer. | `loader/ppc/dispatch_quickdraw.rs`, bit transfers, pictures, GWorlds, and direct buffers write guest pixels. | **Guest pixels** for application artwork and custom definitions. Only a verified manager-owned region may be masked for GPUI; blits, palette/depth changes, and dirty regions must invalidate or reconcile overlays. |
| Fullscreen and display transitions | Window visibility and screen mode changes run through the 68K Window/QuickDraw paths. | `loader/ppc/dispatch_drawsprocket.rs`, display, GWorld, and Window Manager paths can change buffers and mode. | **More extraction required.** A mode switch must rederive guest-to-host scale, overlay clips, and menu visibility before resuming GPUI chrome; until qualified, present the guest framebuffer. |

The first representative fixture is the Toolbox Showcase fat application: it
exercises the same resources and scripted actions as 68K and PowerPC code,
including standard and custom definitions. The game qualification set is still
open: select at least one windowed and one fullscreen title per CPU, capture
their guest checkpoints and composed surface, and compare Toolbox outcomes
against a reference Macintosh environment before promoting a row.

Sources for the contract: *Macintosh Toolbox Essentials* (1992), chapters 3–6
and `PaintOne` on pp. 4-118 onward; *Inside Macintosh: Text* (1993), chapter 2;
*Inside Macintosh: More Macintosh Toolbox* (1993), List Manager and Standard
File chapters. Systemless's source paths above identify the current owners;
none of the classifications grants the frontend authority to edit guest state.

### Historical notification source audit (2026-10-11; superseded by the current inventory row)

Current source confirms a missing implementation, not merely missing captures.
Classic `trap/memory.rs::install_notification_request` validates qType8, links
requests, treats response -1 as automatic removal and arms guest response
procedures. `remove_notification_request` unlinks and returns qErr for absent
requests. The request list is local to the classic dispatcher. No shared
notification model or GPUI notification presenter is established by this path.

PPC `import_targets.rs` binds NMRemove to SystemCompatibility::NmRemove;
`dispatch_system.rs` treats it as ReturnPreserve, leaving the guest return
register unchanged. There is no InterfaceLib NMInstall binding. Thus PPC
notification installation, queue lifetime, error returns, callbacks and visible
notice behavior are incomplete. A host-font toast is not an equivalent fix.

Implementation must establish validated guest record ownership and queue
lifecycle on both CPU paths, then use the existing guest callback machinery for
response procedures. Presentation needs guest text/resource recipes, icon/sound
and acknowledgement state without consuming application-owned drawing. Tests
must cover invalid qType, duplicate install, head/middle/tail removal, absent
request, response -1/null/procedure, callback-triggered removal/reinstall and
CPU parity. Visible notices then require shared-compositor interaction evidence.
This audit closes no release gate and does not claim Macintosh oracle parity.

Apple's original Notification Manager reference (Processes, p.5-9):
https://developer.apple.com/library/archive/documentation/mac/pdf/Processes/Notification_Manager.pdf
requires response execution after delivery, including alert OK acknowledgement
and sound completion. The existing classic immediate-response behavior does not
establish faithful visible-notice timing. Requests with nmStr/nmSound now remain
pending in the local work until a real delivery/completion path is implemented.
This is an unfinished path, not notification qualification.

### Shared Notification Manager queue checkpoint (2026-10-11)

PPC NMInstall/NMRemove now implement record validation, queue links, duplicate
installation, error returns and response callbacks. Classic and PPC adapters
attach to one process-owned request queue. Native PPC and Mixed Mode 68k
responses execute through existing guest call machinery. Alert/sound requests
remain queued without prematurely executing responses or automatic removal.
Eleven targeted tests pass (0.08s; build58.66s), including cross-CPU install/remove.
Evidence is under `reference/gpui-demo/notification-queue` with source hashes.
Visible alert delivery, menu/icon notices, sound completion, acknowledgment and
post-delivery callbacks remain unfinished; this is not GPUI notification
qualification and closes no production gate.

### Notification presentation boundary audit (2026-10-11)

At `ba5efecb`, validated snapshots travel through the worker into shared Demo
state, but no paint, input, accessibility or composition path consumes the
notification list. Snapshot text is raw Macintosh Roman and carries no alert
window, font resource, bounds or layout. Existing standard dialog presentation
requires guest Dialog/Window state, so feeding notification bytes to a host-font
toast would bypass the required system-alert geometry and ownership model.
The next presentation implementation must establish the system-owned alert
recipe, delivery ordering and modality before wiring acknowledgment to the
public runner completion entry. Queue/callback tests do not prove visible
notification delivery. Menu marks, icon flashing and sound completion remain
separate unfinished delivery stages.

### Standard File volume-selector source audit (2026-10-11)

The current Open overlay covers the full guest volume selector and paints only
its retained label. The classic guest path calls `draw_popup_control`, including
CPU-resolved indicator spans (or the configured theme provider). PPC draws a
PopupButton through `ppc_draw_retained_control_rect` and then its retained label.
The four reviewed Open/Save captures in `standard-file-current-typography` show
that the GPUI Open replacement loses that indicator. A shared presentation fix
must carry the guest-resolved indicator/theme geometry rather than pick a host
font symbol, preserve label bounds and paint order, and retain guest event
routing. The selector drawing alone does not establish volume-selection
tracking: that behavior needs a separate source and interaction audit. This is
an identified release gap, not an accepted fallback or completed control.
