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
| Window frames and title bars | `window_manager.rs` supplies ordered `WindowFrameSnapshot` records with a guest WindowPtr and lifetime generation; `gpui_demo_frames.rs` clips standard frame overlays against content and front windows. The presentation snapshot retains the actual WDEF ID but rejects rectangular overlays when either guest structure or content region has complex scanline data; those windows keep their guest pixels. Headless composed captures cover the initial three-window stack, guest title-bar drag, rear activation, grow-box resize, zoom out and back, and both close promotions on 68K and PowerPC, with identity, bounds, active state, order, and clipping checks. GPUI interaction tests verify guest-coordinate press, held movement, and release on the themed title bar; an integrated test drives the real guest through that route, checks moved bounds, activates an exposed rear window, then drags its title bar beyond the guest pane and outer host window, verifying one edge-clamped release and guest-owned movement on both CPUs. The same integrated test drags the GPUI grow corner through the guest event route, verifies one press/release pair, and checks the guest-owned +25/+25 bounds change without changing window identity on both CPUs. Host zoom-box clicks enter the guest standard state and restore the moved/resized user state on both CPUs without changing window identity; the default standard rectangle leaves the title bar below the menu bar. Focused 68K and PowerPC Toolbox tests honor application-written `WStateData.stdState` rectangles and restore `userState`; the PowerPC test checks both FindWindow zoom part codes. The rebuilt showcase now writes a 600×420 standard rectangle for its stacked inspector; headless composed captures show that guest-owned zoom and restoration on both CPUs. | Verify WDEF variants, real complex visible-region behavior and fragmentation fallback, physical desktop pointer capture and focus loss during tracking, and fullscreen transitions. | Demo only |
| Document gutters and grow box | `gpui_demo_frames.rs` themes only the guest-painted one-pixel delimiters and 15×15 grow corner after an actual `DrawGrowIcon` call; the rest of each content edge retains guest pixels. The overlay also clips against every visible guest control bound, including custom CDEFs, so a recorded control in the grow area retains its guest or control presentation. Both CPU gateways record the grow call with window lifetime and painted bounds; geometry mutations clear the signal, and the presentation snapshot rejects a reused WindowPtr or changed bounds. `control_snapshot` separately identifies real scrollbar records and their live value/range. Inside Macintosh assigns scrollbars to application-created controls in the content region and the size box to `DrawGrowIcon` (Macintosh Toolbox Essentials, pp. 4-4--4-5, 4-12, 4-111--4-112). | Verify custom CDEF painting beyond recorded bounds, guest redraw or blit after the grow call, nonrectangular clipping, and active/inactive transitions in real applications. | Confirmed grow pixels and recorded controls |
| Dialogs and alerts | `FixtureRunner::dialog_snapshot` exposes ordered items, types, global bounds, live text, enabled state, window lifetime generation, and active edit selection on both CPUs. The first enabled edit item now initializes the 68K DialogRecord and TERec with an insertion point at offset zero; PowerPC reads that selection from its active TERec. Fresh systemless-play runs against BasiliskII and SheepShaver show the same initial caret, and the showcase test verifies insertion and backspace on both CPUs. The demo overlays buttons, static text, value-backed checkboxes/radio buttons, and single-line edit fields in standard `dBoxProc` dialogs. Multiline/tall edit fields retain guest pixels, and all input remains guest-owned. Offscreen composed captures cover the showcase alert and both unchecked and guest-checked modal preferences states on 68K and PowerPC. A host checkbox click queues one guest press/release pair. | Complete TextEdit layout, modality, callback state, custom-item fallback, broader GPUI rendering, and host accessibility. | Standard dialog slice |
| Buttons, checkboxes, radio buttons, popup controls | Both Control Manager dispatchers handle guest controls and their callbacks. `FixtureRunner::control_snapshot` reads live owner, definition, bounds, title, visibility, highlight, value, range, and lifetime generation on both CPUs. Standard document-window buttons, checkboxes, and radio buttons use GPUI Kit. Checkboxes and radio buttons use 12-point titles and a four-pixel indicator gap in guest coordinates, preserving compact guest bounds without clipping the showcase alignment labels. The standard popup CDEF also exposes its live private MENU ID and title width; a GPUI closed control reads its selected label from that guest menu, while open tracking and unknown definitions keep guest pixels. A guest pointer selection changes the live value and GPUI label on monochrome 68K, colour 68K, and PowerPC; composed initial and selected captures cover both CPU slices. Overlay order follows the guest's first-created-frontmost draw order, and a standard control intersecting a custom CDEF keeps guest pixels. | Qualify generation behavior across process replacement, guest-reordered and dynamically redrawn controls, host keyboard/accessibility operation, popup open-state rendering and custom CDEF fallback tests. | Standard controls demo |
| Scrollbars | The same snapshot exposes live scrollbar bounds and value/range. The demo draws arrows and a value-driven thumb for standard document controls, plus a moving outline while the host holds a thumb; the value remains guest-owned until release. Deterministic guest input checks verify click and thumb-drag values on both CPUs, including held-button timing and release outside the tracking area. GPUI interaction tests verify held movement across the guest-pane boundary and one guest release at an off-window coordinate. Eight composed captures cover initial, clicked, held, and released states on both CPUs. The grow-icon overlay no longer creates a visual scrollbar where the guest has no control. | Qualify continuous native pointer movement and release outside the host window; then test hold/page tracking, accessibility, and control/window overlap. | Standard scrollbar demo |
| Mouse wheel and trackpad scrolling | GPUI wheel events accumulate fractional pixel/line deltas and target an unambiguous standard scrollbar in the active window. The worker revalidates lifetime, visibility, clipping and limits before posting ordinary arrow clicks; guest execution separates press, release and pointer restoration. Consecutive requests coalesce with a bounded step count. Loaded monochrome 68K, colour 68K and PPC tests verify guest-owned value changes; a live-worker test exercises both CPUs. | Qualify vertical document scrolling, real trackpads, rapid direction changes, focus-loss cancellation, nested modal controls, list/Standard File internal scrolling, callback timing under load and audible/frame latency. The shared session API still has no general wheel event; custom and ambiguous targets are not translated. | Standard scrollbar wheel path; broader qualification open |
| Lists | `FixtureRunner::list_manager_snapshot` exposes guest ListHandle, lifetime generation, owner port, local and global view rectangles, LDEF ID, decoded standard cell text, logical cells, selection, visible cell range, drawing/active state, and guest scrollbar visibility bytes. A deterministic showcase test on monochrome 68K, colour 68K, and PowerPC confirms that switching pages hides but retains the same list identity, then restores it. Standard LDEF 0 cells are presented as themed GPUI rows clipped to the owning window and front-window bounds; pointer input still follows guest coordinates. Custom LDEFs and overlapping custom window definitions keep guest pixels. Four offscreen captures cover initial and selected rows on both CPUs. | Add keyboard and accessibility actions, broader list definitions and scroll/selection qualification, and composed captures with complex visible regions and overlapping windows. | Standard list demo |
| TextEdit and editable fields | `FixtureRunner::text_edit_snapshot` reads canonical guest TERecs with stable TEHandle/generation, owner port, local/global view and destination bounds, text bytes, selection, activation, alignment, font fields, guest line starts and height, and private scrap. The showcase checks owner-port geometry, guest-owned typing, Reset selection through `TESetSelect`, and selected-text replacement through `TEKey` on monochrome 68K, colour 68K, and PowerPC. A deterministic 68K guest sequence and BasiliskII both end with a 195-byte, five-line record and selection `[1,1]`; the PowerPC guest sequence reaches the same state. GPUI ASCII keys and arrows enter the existing guest event route; a host event test verifies the translated press/release pair. Dialog items carry edit selection; standard single-line DITL fields use a read-only GPUI presentation. Drawing evidence is independent of optional sharp-text rendering, includes the guest clip/visible regions, and compares against the actual presented framebuffer. Allocated but unpainted records and records overwritten by another page remain hidden. Ordinary unstyled left-aligned document TextEdit uses a clipped GPUI overlay with guest line breaks, scroll origin, selection, and caret. Styled, justified, and custom-overlapping cases retain guest pixels. Seven offscreen captures cover initial, selected, and edited states across 68K and PowerPC. | Qualify font metrics, multiline editing, caret blink, focus and composition state; test guest wrapping, scrolling, selections across lines, Mac Roman input, modifier and clipboard behavior, and keyboard/accessibility actions. | Standard TextEdit slice |
| Standard File panels | The 68K `_Pack3` and PowerPC import paths retain their own modal Open/Save state outside Window Manager records. `FixtureRunner::standard_file_snapshot` normalizes live mode, reply-record identity, per-invocation generation, panel bounds, directory contents, selection, save-name/prompt and keyboard-focus state, and the guest's standard Open/Save item geometry. The opt-in demo overlays modern standard Open and Save panels with GPUI elements on both CPUs; legacy and custom calls retain guest pixels. PowerPC Save lists VFS entries and supports folder selection with Return or a guest-timed double-click, Desktop and parent-directory navigation, guest-owned filename editing, and destination-aware replies. A three-mode showcase test checks Open, cancellation, Save, guest-owned filename editing, and a second cancellation. The PowerPC Save list follows the standard display-list and filename-focus behavior described in Inside Macintosh: Files (1992), pp. 3-5--3-6. | Complete PowerPC Save New Folder, replacement confirmation, directory popup, and keyboard behavior; qualify callback timing, entry icons, true scroll state, nested modality, caret blink and composition, and accessibility actions on both panels. | Standard Open/Save demo |
| Cursors and notifications | Cursor bitmap, mask, hotspot, visibility, and hide/show level are guest state presented by the desktop host. Classic notification records and callbacks are tracked; equivalent PowerPC install and visible-notice coverage is unproven. The draw-path inventory separates these boundaries. | Qualify resource-backed versus application-built cursors, notification imagery, sound, acknowledgment, callback timing, and PowerPC installation before GPUI presentation. | State extraction needed |
| QuickDraw and custom definitions | Framebuffer remains the presentation source. | Mask only verified standard system pixels; keep unknown WDEF, CDEF, MDEF, user items, and application drawing unchanged. | Required fallback |

Content overlays now use a bounded snapshot of the guest window’s exact
visible region. Rectangular regions contribute one clip; complex QuickDraw
scanlines are decoded into coalesced rectangles, so controls, dialog items,
lists, TextEdit, and the grow icon do not cross holes in the guest’s visible
area. Malformed or excessively fragmented regions keep guest pixels. A
synthetic hole test checks the composed clip pieces, while fresh normal
68K and PowerPC composed captures remain pixel-identical to their
references. Complex-region behavior in real applications still needs
qualification.

The existing `tests/toolbox-showcase/oracle/windows.json` replays 64 guest
actions across seven window checkpoints. Current PowerPC and 68K replays both
completed the sequence; visual review against the committed SheepShaver and
BasiliskII references confirms the same stacking, activation, movement, growth,
hit-test, and close-promotion transitions. The reference emulators' desktop,
palette, and window metrics differ, so whole-screen pixel equality is not an
acceptance test here. GPUI composed captures now cover the initial stack, drag,
activation, growth, zoom out and restoration, and both close-promotion states
on each architecture. The
GPUI grow-corner drag and zoom-box clicks also reach the guest on both
architectures. The zoomed standard rectangle leaves its title bar visible,
and clicking it again restores the bounds reached after host move and resize.
Visual review caught stale 68K pixels in the windows exposed by zoom-back;
ZoomWindow now recalculates visibility and invalidates those windows, and the
reviewed restored capture shows their guest repaint. Custom application
standard-state rectangles now have focused Toolbox checks and composed
showcase zoom-and-restore captures on both CPUs. Stale-overlay behavior in broader applications remains to
be qualified.

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
still needs qualification. Real nonrectangular visible regions and arbitrary
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

Palette restoration now selects the active indexed depth and color/grayscale personality on both CPUs instead of installing an eight-bit palette on a monochrome screen. Tests cover default-window activation and explicit device restoration across 1-, 2-, 4-, and 8-bit modes. A one-bit BasiliskII run, after correcting the oracle's host pixel conversion, reproduces the showcase's solid-black custom panel interiors and hidden captions. The attempted nearest-colour shape change was therefore withdrawn: a more readable image was not faithful evidence. The colour-resolution semantics still need a focused guest probe. The refreshed monochrome GPUI reference has been reviewed and preserves this native custom drawing. A focused regression covers the observed light-grey PaintRect fallback and preserves neighbouring bits; all 758 existing QuickDraw tests also pass. The broader monochrome mode remains unqualified.

Interactive GPUI startup now attaches the existing native stereo audio backend
on the guest worker, shared with the ordinary desktop frontend through the
`native-audio` feature. Headless captures do not open an output device. Nine
buffer/resampling tests pass; an opt-in hardware smoke test confirms that the
macOS output callback consumes queued silent stereo frames and that stopping
clears the queue. Both GPUI and ordinary desktop builds pass. These checks do
not establish audible fidelity, sustained game playback, or latency under UI
load; those remain qualification requirements.

Interactive GPUI now reuses the ordinary desktop save store and its archive-based
save location. Saved files are imported before initialization; periodic scans
and final flushes cover guest exit, command-channel disconnect, and explicit
host shutdown. The app quit hook stops and joins the worker before returning.
Host services remain disabled in guest-worker tests and headless captures.
All 52 existing GPUI/save-store tests pass, plus a temporary-directory roundtrip
covering loaded monochrome 68K, colour 68K and PowerPC sessions, both forks and
metadata. Real-game save/relaunch, write-failure recovery, and abnormal process
termination remain unqualified.

GPUI forwards Command, Shift, Option and Control transitions through guest key
input. Duplicate host modifier notifications do not duplicate key presses;
focus cleanup releases each held modifier once. A GPUI event test covers host
notifications and cleanup, and loaded 68K/PPC tests confirm held modifier bits
survive guest execution until release. Caps Lock and exact Mac Roman input
qualification are described below; composition and comprehensive modified-text
behaviour remain open.

Recognized GPUI key identities now carry single Unicode characters through the
shared exact Mac Roman encoder instead of rejecting all non-ASCII input.
Unrepresentable or multi-character text is still rejected by this key-event
path. Key release depends only on key identity and uses the character retained
at key-down, preventing changed/unrepresentable release text from leaving a
key held. Three focused GPUI tests pass, including é/£/π translation and release
after incompatible text. IME composition and non-US key identities still need
a proper text-input integration and guest TextEdit qualification.

Further input qualification confirms exact é/£/π bytes and caret movement in
loaded guest TextEdit on monochrome 68K, colour 68K and PPC. Caps Lock now tracks
host latch changes (including the window's state at key-down) through guest
press/release pairs; duplicate notifications and focus cleanup do not retoggle
it. Host-event and loaded 68K/PPC latch tests pass. These checks do not cover
IME composition or non-US physical key identities.

Keyboard translation, held-key identity, modifier transitions, Caps Lock and
focus cleanup now live in `src/bin/gpui_demo_input.rs`. The view forwards the
returned guest events through the existing session command route. This module
has no window, rendering or worker-channel ownership; live host events and the
existing interaction tests use the same implementation. Pointer lifecycle still
needs extraction. Standard scrollbar wheel translation now lives in `src/bin/gpui_demo_scroll.rs`; broader scrolling remains open.

The standard scrollbar wheel path also has a real GPUI event test: fractional
horizontal deltas combine into one guest request at the scaled scene position,
and a held mouse button suppresses wheel translation. Focus loss clears pending
wheel work; a cancelled touch gesture takes the same cancellation route. These
checks do not establish complete modal, list or Standard File wheel support.

List scrollbar qualification found that both LClick implementations ignored
scrollbar clicks. Standard default-list arrow tracking now retains the call
through release, repeats using the shared Control Manager cadence, pauses
outside the original arrow and resumes on re-entry. It preserves selection,
synchronizes the visible cell origin and scrollbar value, redraws changed
state, and validates control lifetime and geometry. GPUI retained-list loops
advance guest ticks so held scrolling continues in the live frontend.
Loaded monochrome 68K, colour 68K and PPC tests cover the gesture lifecycle;
the live-worker test checks wheel and held-arrow scrolling on both CPUs.
The 45 List Manager tests, four existing LClick tests and 56 GPUI tests pass.

The deterministic native Mac OS 8.1 replay scrolls down, back to the first row,
and to the final page while held. Systemless now reaches those qualitative
states and preserves empty selection. Exact native equivalence is not yet
established: the short native press moves two rows versus one in Systemless,
and the native scrollbar thumb position differs in the captured replay.
Timed guest-state probes are still needed to explain those differences.
Thumb tracking, custom definitions and click-loop callbacks remain unfinished.
Page-click progress and its remaining native qualification are described below. More Macintosh Toolbox (1993), pp. 4-84--4-85.

Standard List Manager page clicks now use the same retained tracking state as
arrow clicks on both CPUs. The shared hit test distinguishes the two page
regions from the thumb, checks the current thumb position before repeating,
and pages by visible capacity less one cell. Resizing and a clipped final row
are included in that calculation. A loaded monochrome 68K, colour 68K and PPC
regression resizes the list, pages up/down/up, checks the retained call and
preserves selection. A shared test covers page overlap, thumb crossing and
wide signed coordinate/value ranges without arithmetic overflow.

The native compact-list replay reaches row six on page-down, matching the HLE
result. Its thumb remains near the top, so the same intended page-up coordinate
is still below the native thumb and does not scroll back. This remains an
unresolved native-state/rendering discrepancy, not evidence of complete page
tracking equivalence. The page-click implementation does not close thumb,
custom-CDEF/LDEF or click-loop qualification.

A native Mac OS 8.1 diagnostic readout narrows the compact-list discrepancy:
page-down reports `visible.top = 6`, `visible.bottom = 13`, and scrollbar
value/minimum/maximum `6/0/6`, despite the thumb remaining near the top.
The shared List Manager now preserves that visible viewport extent instead of
clamping it to the twelve-row data bounds. Rendering and custom-definition
cell enumeration still exclude nonexistent cells. The loaded three-mode GPUI
regression checks the seven-row visible extent after every compact page click.
The native thumb rendering/hit-region discrepancy remains unresolved; matching
logical scrollbar values alone does not qualify complete native equivalence.

A separate native ordinary-scrollbar diagnostic also reaches value/minimum/
maximum `10/0/10` after four page clicks while leaving its thumb at the start.
The same replay reaches `10/0/10` in Systemless with the thumb at the end.
Explicitly changing and restoring the native list control value after LClick
also leaves its thumb at the start. Thus the observed native thumb anomaly is
not specific to List Manager, nor explained by an omitted value refresh.
Its cause remains unqualified; do not infer a required HLE thumb-position change
from these native captures alone. Logical paging and visual thumb positioning
must continue to be evaluated separately.

The native Mac OS 8.1 PowerPC cross-check resolves the expected thumb direction:
ordinary paging reaches `10/0/10` with the thumb at the end, matching Systemless.
Compact list paging likewise matches Systemless's values, visible extents and
thumb positions: down is `6..13` / `6/0/6`, up is `0..7` / `0/0/6`.
The anomalous fixed thumb is confined to the tested 68K oracle environment;
its cause remains open, and these PPC results do not qualify that environment.

A deterministic native PPC list-thumb replay now demonstrates down/up dragging
and cancellation outside the drag allowance. The list stays stationary while
a separate thumb outline follows the pointer, then scrolls on release. A
cancelled drag preserves the original origin. The shared LClick implementation
now retains thumb tracking, leaves content stationary while held, commits on
release, and cancels outside the drag allowance. Loaded monochrome 68K,
colour 68K and PPC tests cover down/up/cancel and unchanged selection. Native
comparison replays now reach row six on release and row zero on cancellation
in both Systemless CPU adapters. Shared geometry tests cover horizontal motion,
changed list limits and signed ranges wider than an i16 delta.

Standard list-thumb tracking now also paints and restores a moving raster
outline on both CPU paths. Pixel snapshots retain presentation detail and are
restored before moving, cancelling, releasing or redrawing the control; a
framebuffer with changed address, dimensions, row stride or depth is not
restored from an old snapshot. The loaded three-mode
regression checks that an outline appears, cancellation restores the track
exactly, and held feedback leaves the list content and application border
unchanged. PPC highlight-only updates now redraw the control rather than
repainting list cells. Reviewed headless 68K/PPC captures cover held and cancelled
states. Full thumb readiness still requires composed GPUI list-drag captures,
overlap/lifecycle qualification, direct-colour classic surfaces beyond the
indexed save/restore helper, and custom CDEF/LDEF and click-loop behaviour.

Validation for retained list-thumb release: all 47 list tests and all 56 GPUI
tests pass, the latter with one test thread. An earlier concurrent run during
additional compilation timed out in the live worker test; that test passed
in isolation and in the complete sequential rerun. This is not performance
qualification under load.

GPUI scrollbar preview and retained list-thumb release now share the same
architecture-neutral drag-position calculation. Native PPC confirms that a
compact vertical bar dragged from `(150,522)` to `(300,522)` cancels rather than
committing the end value. GPUI previously kept its outline at the end in that
case; it now cancels on either axis using the guest's thirty-pixel allowance.
Boundary tests cover both orientations and large signed control ranges without
intermediate multiplication overflow. The loaded three-mode interaction test
also covers cancellation beyond the end of the bar. This aligns preview
geometry; the raster feedback qualification above covers ordinary standard lists.

List disposal now restores saved thumb-outline pixels before removing tracking
and freeing the list/control records. Regression checks exercise both classic
Pack0/Pack1 disposal entry points and the native PPC LDispose dispatcher, and
verify pixel restoration plus cleared tracking. All 48 list tests pass.
This closes the retained-outline disposal leak. Switching a retained standard
list to a custom definition now also restores the outline before cancelling
the matching tracking call on both CPU paths. Regression checks verify restored
pixels, cleared tracking, a false LClick result and unchanged selection.
Custom-definition callback execution, mixed-mode callback disposal and broader
lifecycle/overlap behaviour still require qualification.

Composed list-thumb capture modes now cover held and cancelled guest drags.
Monochrome 68K, colour 68K and PPC captures assert unchanged origin and selection, and show
stationary rows during the drag and a removed outline after cancellation.
Visual review exposed a clipped right edge on the held GPUI outline in all
three modes. The scrollbar container border was insetting its full-width
children; painting the frame separately now preserves guest-coordinate layout.
Reviewed held captures show all four outline edges in all three modes and on
a horizontal PPC control. The focused drag geometry test also passes. This
fix does not establish full thumb readiness or overlap/scaling qualification.

The document TextEdit overlay now follows the guest TERec caret blink phase
instead of painting every active insertion point continuously. Both CPU
adapters expose the same canonical caretState field, with guest TEIdle timing
remaining authoritative. The painted caret has zero layout width so toggling
it does not shift adjacent text. Focused classic and PPC checks exercise the
32-tick blink boundary; dialog and Standard File caret presentation, physical
focus transitions and composed blink captures remain separate qualification.

Single-line dialog edit overlays now also consume the active DialogRecord
TextEdit caret phase on both CPU paths. Non-active items have no phase, and
unknown phases do not synthesize a caret. The caret has zero layout width.
Seven focused dialog tests pass; the preferences interaction test additionally
checks phase availability on monochrome 68K, colour 68K and PPC. This validates
the snapshot connection, not full visual blink timing: composed visible/hidden
pairs, focus transitions and Standard File caret behaviour remain open.

The stronger modal-dialog blink regression exposed a stationary caret after
65 explicit guest ticks. The no-filter, empty-event ModalDialog wait now services
the active editor on both CPUs. Classic captures updated dialog pixels after
a phase change so later saved-pixel restoration retains that change; PPC uses
the same idle helper as TEIdle and redraws the editor only when its phase changes.
PPC text painting also now honors the stored phase. The three-mode preferences
regression observes a phase change with unchanged text and selection, then
continues its existing editing and checkbox interaction checks. Filter-driven
idle, callback ordering, composed blink pairs and saved-pixel visual review
remain unqualified; this is not full dialog readiness.

Pixel-level modal blink regression identified the presentation overwrite:
after restoring saved pixels, classic dialog chrome redrew the active field
as select-all until modified, ignoring the TERec selection and blink phase.
That redraw now reads the active TERec and positions its caret at the guest
byte offset. The three-mode regression requires changed pixels within the
active field, no changed pixels outside it, and exact restoration after a full
blink cycle. Broader custom filters, nested callbacks, font metrics and composed
GPUI visible/hidden image review remain qualification work.
The full-cycle assertion now passes on monochrome 68K, colour 68K and PPC.
PPC initial drawing now uses the active TextEdit record, and modal idle
repaints only that edit item, clearing the previous caret before drawing the
current phase. This removes the discrepancy between initial and idle painting.
The regression also completes its typing, deletion and checkbox interactions.

The mixed-content redraw gate honors recorded application ownership when the
dialog contains custom items, without requiring the entire dialog to classify
as fully custom. Standard controls still repaint, while the application-composed
background is retained. Standard-only dialogs keep their existing exposed-background
repaint behaviour. All 633 library dialog tests passed with this distinction;
both focused edit-field provider tests also passed.
All seven GPUI dialog regressions pass after the PPC painter consolidation.

The existing composed capture path now supports
`--capture-modal-dialog-caret-visible <png>` and
`--capture-modal-dialog-caret-hidden <png>`. Both open the showcase preferences
through guest input and wait for the requested live TERec phase; neither
changes the guest selection or forces the caret state. Run the example with
`tests/toolbox-showcase/toolbox-showcase.sit`, adding `--screen-depth 1` for
monochrome 68K or `--prefer-powerpc` for PPC.

All six captures were generated and visually reviewed. Each 1800×1480 pair
differs in exactly 62 pixels, confined to the same two-pixel-wide,
31-pixel-high caret rectangle; text and surrounding layout remain stationary.
This qualifies the composed initial insertion-point blink for this standard
dialog, not other caret positions, fonts, modal filters or nested dialogs.
The same visual review exposes thin residual outlines around standard buttons,
including the default button; their paint ownership and overlay bounds still
need investigation. The monochrome custom page background also differs from
the colour page and needs separate guest/oracle comparison.

Button-outline diagnosis: the current GPUI image fragment shader uses linear
minification and magnification. Standard guest button borders are inside their
recorded rectangles, but their sampled colour reaches outside the covering GPUI
rectangle at fractional output scales. A diagnostic composed modal capture
cleared only the existing clipped button source rectangles before image upload,
without expanding any overlay. The two dialog button outlines disappeared;
878 output pixels changed within their combined output bounds. The document
buttons, which were deliberately untouched, retained their outlines. This
isolates the source to the sampled framebuffer underlay, not GPUI button chrome.
The diagnostic-only edit was removed. A production solution must share texture
preparation between live and capture paths, use the current presentation theme,
and preserve custom, occluded and fallback content without enlarging hit regions.

The shared live/capture render path now prepares the framebuffer beneath
standard document and dialog buttons using the exact existing clipped overlay
rectangles and current GPUI background colour. It does not expand guest bounds
or hit regions. Custom-control overlap and window visibility use the same
piece selection as the visible overlays; custom-menu tracking leaves the raw
framebuffer in place. Prepared textures are cached by source image, clipping
rectangles and theme colour, and old textures are released on replacement.
A focused pixel test checks negative/out-of-screen clipping and unchanged bytes
outside owned rectangles, including a gap between them. All 57 GPUI tests pass.
This fixes button underlay preparation; other component edges, dark-theme
captures and material performance impact still require qualification.
Fresh composed captures on monochrome 68K, colour 68K and PPC show clean dialog
and document buttons. Each mode changes the same 2,621 outline pixels compared
with its previous capture. The visible/hidden caret pair still differs only in
62 pixels at the existing caret rectangle, with no text or layout movement.

Standard dialog buttons now use GPUI primary styling when the live guest
snapshot identifies them as the enabled default action of the active dialog.
This restores a clear default-action cue after replacing the guest outline;
it does not infer defaults from labels or intercept Return. HIG (1992),
pp. 205–206 describes the default-action distinction and guest keyboard behavior.
All seven GPUI dialog tests pass. Composed preferences captures on monochrome
68K, colour 68K and PPC show the designated OK action emphasized and Cancel
unchanged. Dynamic default changes, disabled defaults and nested inactive
state still need dedicated visual qualification.
