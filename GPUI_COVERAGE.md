# GPUI Kit system-interface coverage

This is the working inventory for presenting Macintosh system UI through GPUI
Kit on both 68K and PowerPC. A row is complete only when the guest state is
observable on both architectures, GPUI renders it without obscuring
application-owned pixels, input returns through the existing Toolbox/event
path, and deterministic behavior and composed-image checks pass. An existing
Toolbox implementation or a visual demo alone does not satisfy that gate.
The [draw-path inventory](GPUI_DRAW_PATHS.md) records the 68K and PowerPC
pixel owners, redraw triggers, callbacks, and fallback boundary behind each
row.

The guest remains authoritative for state, hit regions, event order, and
callbacks. Unknown or application-defined definitions retain guest pixels.
The opt-in implementation is staged in a draft PR and must not be merged.

| System UI | Existing state and presentation path | Missing GPUI work | Status |
| --- | --- | --- | --- |
| Menu bar and standard menus | `menu_model.rs` supplies `GuestMenuSnapshot`, including live, resource-aware standard MDEF classification on both CPUs, the current menu list's MBDF ID, and each MenuHandle with a process-shared lifetime generation. GPUI menu button identity and queued command validation follow the handle and generation instead of the reusable menu ID; both CPU menu disposal, resource-release, and direct handle-disposal paths invalidate the generation. `gpui_demo.rs` retains a Systemless-owned GPUI Kit popup and reconciles its selected guest item when the menu snapshot changes, renders standard menu buttons and items, and dispatches selected items to the guest. A headless GPUI interaction test confirms live item text, dismissal/reopening, and removal when the guest menu disappears. Translatable Command-key presses and releases enter the guest KeyMap/event path instead of synthesizing a menu click; the showcase responds to Command+P on both CPUs, and a held-key test checks that both guest KeyMap bits remain set through further execution and clear on release. GPUI modifier changes and focus loss clear held keys, and host repeat callbacks do not duplicate guest keyDown events. A custom MDEF or MBDF selects the full guest framebuffer at original coordinates so GPUI chrome cannot cover it. Classic `InitProcMenu` retains the ID and loads its MBDF resource. A synthetic headless composed capture checks placement and pointer translation. Focused guest tests confirm fallback selection during a real 68K MDEF callback in a PowerPC app and during a native PowerPC MDEF invocation; the cross-CPU tracking test confirms live menu pixels and save-under restoration. | Custom MBDF message execution is not implemented: Inside Macintosh V-250 defines Draw, Hit, Calc, Init, Dispose, Hilite, Height, Save, Restore, Rect, SaveAlt, ResetAlt, and MenuRgn messages. Verify a real guest composed capture and standalone 68K custom MDEF tracking; qualify guest autoKey timing during real held shortcuts, submenus, and tracking order. Qualify submenu contents, hover selection, disabled-state transitions, focus, and menu scrolling in composed and interaction tests across real applications. | Demo only |
| Window frames and title bars | `window_manager.rs` supplies ordered `WindowFrameSnapshot` records with a guest WindowPtr and lifetime generation; `gpui_demo_frames.rs` clips standard frame overlays against content and front windows. Headless composed captures of the showcase's three overlapping windows before and after a guest title-bar drag cover 68K and PowerPC, with the front window's identity, generation, and moved bounds checked. GPUI interaction tests verify guest-coordinate press, held movement, and release on the themed title bar; an integrated test drives the real guest through that route and checks the same window's identity and moved bounds on both CPUs. | Verify WDEF variants, nonrectangular regions, host pointer activation and captured drag outside the guest pane, zoom, grow, and fullscreen transitions. | Demo only |
| Document gutters and grow box | `gpui_demo_frames.rs` draws 15-pixel edge strips for selected WDEFs; `control_snapshot` can now identify real scrollbar records and their live value/range. | Distinguish actual guest controls from empty gutters, handle tracking and clipping, and avoid covering custom content. | Unsafe to generalize |
| Dialogs and alerts | `FixtureRunner::dialog_snapshot` exposes ordered items, types, global bounds, live text, enabled state, window lifetime generation, and active edit selection on both CPUs. The first enabled edit item now initializes the 68K DialogRecord and TERec with an insertion point at offset zero; PowerPC reads that selection from its active TERec. Fresh systemless-play runs against BasiliskII and SheepShaver show the same initial caret, and the showcase test verifies insertion and backspace on both CPUs. The demo overlays buttons, static text, value-backed checkboxes/radio buttons, and single-line edit fields in standard `dBoxProc` dialogs. Multiline/tall edit fields retain guest pixels, and all input remains guest-owned. Offscreen composed captures cover the showcase alert and both unchecked and guest-checked modal preferences states on 68K and PowerPC. A host checkbox click queues one guest press/release pair. | Complete TextEdit layout, modality, callback state, custom-item fallback, broader GPUI rendering, and host accessibility. | Standard dialog slice |
| Buttons, checkboxes, radio buttons, popup controls | Both Control Manager dispatchers handle guest controls and their callbacks. `FixtureRunner::control_snapshot` reads live owner, definition, bounds, title, visibility, highlight, value, range, and lifetime generation on both CPUs. Standard document-window buttons, checkboxes, and radio buttons use GPUI Kit. The standard popup CDEF also exposes its live private MENU ID and title width; a GPUI closed control reads its selected label from that guest menu, while open tracking and unknown definitions keep guest pixels. A guest pointer selection changes the live value and GPUI label on monochrome 68K, colour 68K, and PowerPC; composed initial and selected captures cover both CPU slices. Overlay order follows the guest's first-created-frontmost draw order, and a standard control intersecting a custom CDEF keeps guest pixels. | Qualify generation behavior across process replacement, guest-reordered and dynamically redrawn controls, host keyboard/accessibility operation, popup open-state rendering and custom CDEF fallback tests. | Standard controls demo |
| Scrollbars | The same snapshot exposes live scrollbar bounds and value/range. The demo draws arrows and a value-driven thumb for standard document controls, plus a moving outline while the host holds a thumb; the value remains guest-owned until release. Deterministic guest input checks verify click and thumb-drag values on both CPUs, including held-button timing and release outside the tracking area. GPUI interaction tests verify held movement across the guest-pane boundary and one guest release at an off-window coordinate. Eight composed captures cover initial, clicked, held, and released states on both CPUs. | Qualify continuous native pointer movement and release outside the host window; then test hold/page tracking, accessibility, and control/window overlap. Document gutters remain decorative until real controls are identified there. | Standard scrollbar demo |
| Lists | `FixtureRunner::list_manager_snapshot` exposes guest ListHandle, lifetime generation, owner port, local and global view rectangles, LDEF ID, decoded standard cell text, logical cells, selection, visible cell range, drawing/active state, and guest scrollbar visibility bytes. A deterministic showcase test on monochrome 68K, colour 68K, and PowerPC confirms that switching pages hides but retains the same list identity, then restores it. Standard LDEF 0 cells are presented as themed GPUI rows clipped to the owning window and front-window bounds; pointer input still follows guest coordinates. Custom LDEFs and overlapping custom window definitions keep guest pixels. Four offscreen captures cover initial and selected rows on both CPUs. | Add keyboard and accessibility actions, nonrectangular visible-region clipping, broader list definitions and scroll/selection qualification, and overlapping-window composed captures. | Standard list demo |
| TextEdit and editable fields | `FixtureRunner::text_edit_snapshot` reads canonical guest TERecs with stable TEHandle/generation, owner port, local/global view and destination bounds, text bytes, selection, activation, alignment, font fields, guest line starts and height, and private scrap. The showcase checks owner-port geometry, guest-owned typing, Reset selection through `TESetSelect`, and selected-text replacement through `TEKey` on monochrome 68K, colour 68K, and PowerPC. A deterministic 68K guest sequence and BasiliskII both end with a 195-byte, five-line record and selection `[1,1]`; the PowerPC guest sequence reaches the same state. GPUI ASCII keys and arrows enter the existing guest event route; a host event test verifies the translated press/release pair. Dialog items carry edit selection; standard single-line DITL fields use a read-only GPUI presentation. Ordinary unstyled left-aligned document TextEdit uses a clipped GPUI overlay with guest line breaks, scroll origin, selection, and caret. Styled, justified, and custom-overlapping cases retain guest pixels. Seven offscreen captures cover initial, selected, and edited states across 68K and PowerPC. | Qualify font metrics, multiline editing, caret blink, focus and composition state; test guest wrapping, scrolling, selections across lines, Mac Roman input, modifier and clipboard behavior, and keyboard/accessibility actions. | Standard TextEdit slice |
| Standard File panels | The 68K `_Pack3` and PowerPC import paths retain their own modal Open/Save state outside Window Manager records. `FixtureRunner::standard_file_snapshot` normalizes live mode, reply-record identity, per-invocation generation, panel bounds, directory contents, selection, save-name/prompt and keyboard-focus state, and the guest's standard Open/Save item geometry. The opt-in demo overlays modern standard Open and Save panels with GPUI elements on both CPUs; legacy and custom calls retain guest pixels. PowerPC Save lists VFS entries and supports folder selection with Return or a guest-timed double-click, Desktop and parent-directory navigation, guest-owned filename editing, and destination-aware replies. A three-mode showcase test checks Open, cancellation, Save, guest-owned filename editing, and a second cancellation. The PowerPC Save list follows the standard display-list and filename-focus behavior described in Inside Macintosh: Files (1992), pp. 3-5--3-6. | Complete PowerPC Save New Folder, replacement confirmation, directory popup, and keyboard behavior; qualify callback timing, entry icons, true scroll state, nested modality, caret blink and composition, and accessibility actions on both panels. | Standard Open/Save demo |
| Cursors and notifications | Cursor bitmap, mask, hotspot, visibility, and hide/show level are guest state presented by the desktop host. Classic notification records and callbacks are tracked; equivalent PowerPC install and visible-notice coverage is unproven. The draw-path inventory separates these boundaries. | Qualify resource-backed versus application-built cursors, notification imagery, sound, acknowledgment, callback timing, and PowerPC installation before GPUI presentation. | State extraction needed |
| QuickDraw and custom definitions | Framebuffer remains the presentation source. | Mask only verified standard system pixels; keep unknown WDEF, CDEF, MDEF, user items, and application drawing unchanged. | Required fallback |

For live menu validation, GPUI Kit 0.7.1's `PopupMenu::rebuild` clears its
private selected row. Systemless now owns a retained standard-menu popup built
with GPUI Kit's `Popover`, theme, focus, and accessibility primitives. It keys
selection to the guest MenuHandle generation and item number, reconciles that
selection after each update, and clears it if the selected item is disabled or
disposed. The popup preserves keyboard and hover selection when labels or
checkmarks change, skips disabled items, and routes leaf commands through the
guest queue. Disabled rows omit GPUI click handlers. A keyboard test covers
hierarchical navigation with Right and Enter; a live-update test covers item
changes and an unrelated menu appearing while the popup is open. Offscreen
composed captures of the real showcase menu
cover both 68K and PowerPC. An interaction test confirms a 40-item menu scrolls
its last keyboard-selected item into view. The interaction test also verifies
that removing the guest menu unmounts the open popup; replacement with a reused
MenuHandle still needs qualification. Selections still pass through
`FixtureRunner::select_guest_menu_item` for guest-side validation and Toolbox
event ordering. Inside Macintosh Volume I, I-352 and I-356–I-358, defines
menu lifetime, item state, and `MenuSelect` tracking; a visual refresh must
not synthesize an application command.

The showcase's Command+P input now has a deterministic Event Manager timing
check on both CPUs: while the shortcut is held, no `autoKey` is queued before
guest tick 16, one is queued at tick 16, and the next arrives four ticks later.
After key release, four further guest ticks add no repeat. This checks the
Systemless input and guest clock path; actual host key-hold timing and menu
tracking during repeated shortcuts still need a GPUI interaction check.
GPUI focus loss also releases a held guest mouse button through the input
queue and clears local scrollbar/popup tracking, with a test for exactly one
release. Scroll-wheel translation still needs a defined guest event route;
the frontend does not synthesize direct scrollbar value changes.

Standard DITL items are now selected by their owning window identity and
generation rather than by the active-dialog flag. The supported standard
window definitions include `noGrowDocProc` (4), the usual modeless dialog
frame (Macintosh Toolbox Essentials, pp. 4-10--4-11); custom WDEFs retain
guest pixels. Visible inactive modeless dialogs can therefore use the same
GPUI item presentation, clipped beneath front window structure bounds. A
focused clipping test checks an inactive WDEF 4 dialog, custom item/WDEF
fallbacks, and a stale window generation. A 68K/PowerPC guest replay now
checks that a modal dialog opened above a modeless one receives key input,
that the underlying edit field stays unchanged, and that focus and editing
return to the modeless dialog on dismissal. The read-only frame presentation
now gives focus to only the frontmost active window, even if a covered guest
WindowRecord retains its hilite flag. A closed custom MDEF no longer disables
window and dialog overlays; active custom-menu tracking still preserves the
whole guest framebuffer so its dropdown can cover windows. The
`--capture-nested-modal-dialog` headless case asserts that tracking has ended
and records actual GPUI composition on both CPUs. The captures show matching
modal controls and edit caret above a clipped, inactive modeless dialog.
The showcase now uses GPUI's standard menu bar on both CPUs. Previously,
GetNewMBar copied its MBAR resource ID into the new list's MBDF field and
incorrectly classified that standard bar as custom. The true custom-MBDF
guest fallback remains synthetic-capture verified; a real custom definition
still needs qualification. Nonrectangular visible regions and arbitrary
nested-modal layouts remain unqualified.
The `--capture-modeless-dialog-layout` headless case composes an inactive standard
dialog behind a front window and checks that its covered item keeps guest
pixels while the exposed part uses the GPUI overlay. The showcase also opens
a real resource-backed modeless dialog through its Options menu. A focused
68K/PowerPC replay checks its WDEF 4 identity, checkbox state, deactivation,
reactivation, and close lifecycle; `--capture-modeless-dialog` records the
composed guest and GPUI surface on both CPUs. PowerPC newly shown windows
paint the default white content background when they have no WCTab, avoiding
stale artwork beneath the dialog items. Deterministic guest-input replays on
both CPUs also cover checkbox tracking and reactivation after full occlusion;
the 68K standard-dialog redraw clears newly exposed content while retaining
application-painted custom dialog content. DialogSelect key input now repaints
the guest-owned edit field immediately on both CPUs; the replays assert a new
glyph before any window switch, alongside the semantic text snapshot checks.
The PowerPC key redraw is restricted to the edited DITL item, with a test that
guest drawing over a different item survives.

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
