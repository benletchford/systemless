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

On macOS with a working GPUI graphics context, reproduce the popup capture
matrix with `sh .github/scripts/capture-gpui-popups.sh output/gpui-popups`. It runs
scrolled and selected checkpoints in monochrome 68k, colour 68k and PPC,
retaining paired guest/composed PNGs, per-checkpoint logs and a manifest with
the fixture hash, image hashes and dimensions. These are review artifacts,
not an automatic visual-parity verdict.

Hosted macOS qualification on revision `07723508b4cdd1738346b98f930b61587ce0c45a`
passed all 64 GPUI interaction tests and the six-checkpoint popup capture matrix
([CI run 37843689878](https://github.com/benletchford/systemless/actions/runs/37843689878)).
The downloaded `gpui-popup-captures` artifact contains all twelve guest/composed
images; every manifest hash was verified and every image is byte-identical to
the corresponding locally reviewed capture. This qualifies the popup matrix,
not the unfinished components below or other host platforms. The same CI run
still failed native capture provenance validation against an older showcase
archive, and its scripted checkbox trace required a release-event correction;
those failures must be cleared separately.

The native overview replay now targets the expanded preferences dialog's
actual OK button and asserts that the dialog has disappeared before continuing.
Fresh BasiliskII and SheepShaver runs both pass that assertion; their accepted
screenshots show the application's confirmed-with-OK status. This repairs a
replay error that otherwise left later page captures inside the modal dialog.
The refreshed overview references now pass all 59 checkpoints, 85 reviewed
outcome regions, state-transition relations and the isolated SysBeep checks on
both emulators. Their manifest records the current fixture, scenario, emulator
image, runner, ROM, system disk, preferences and PCM identities. Eighty regions
matched the previous references; the remaining five were reviewed against the
added modeless-menu entry and expanded preferences dialog. This qualifies the
overview scenario only. The sound references have also been refreshed after
verifying full-, 75%- and 50%-volume waveforms exactly against the existing
native PCM references, flush retaining playback, quiet cancelling playback,
and the displayed callback/status readouts on both CPUs. Sound-image changes
were confined to pointer positions. Drawing and popup references are also
refreshed: both drawing captures and all 22 popup checkpoints match their
previous outcomes, with only reviewed pointer-position differences. All 16
list checkpoints likewise retain their reviewed outcomes and now have current
capture identities. The 14 window and 18 TextEdit checkpoints are refreshed as
well, with differences confined to pointer positions and the blinking edit
caret. The local coverage gate now passes all seven native scenario manifests
across 16 fixture pages. Hosted headless/package validation passes on revision
`ae2129aaca769ab13801acc27359f8996d61eb9f`
([CI run 37848619909](https://github.com/benletchford/systemless/actions/runs/37848619909)),
clearing the stale-reference gate. This evidence repair does not qualify
unfinished GPUI components.

| System UI | Existing state and presentation path | Missing GPUI work | Status |
| --- | --- | --- | --- |
| Menu bar and standard menus | `menu_model.rs` supplies `GuestMenuSnapshot`, including live, resource-aware standard MDEF classification on both CPUs, the current menu list's MBDF ID, and each MenuHandle with a process-shared lifetime generation. GPUI menu button identity and queued command validation follow the handle and generation instead of the reusable menu ID; both CPU menu disposal, resource-release, and direct handle-disposal paths invalidate the generation. `gpui_demo.rs` retains a Systemless-owned GPUI Kit popup and reconciles its selected guest item when the menu snapshot changes, renders standard menu buttons and items, and dispatches selected items to the guest. A headless GPUI interaction test confirms live item text, dismissal/reopening, and removal when the guest menu disappears. Translatable Command-key presses and releases enter the guest KeyMap/event path instead of synthesizing a menu click; the showcase responds to Command+P on both CPUs, and a held-key test checks that both guest KeyMap bits remain set through further execution and clear on release. GPUI modifier changes and focus loss clear held keys, and host repeat callbacks do not duplicate guest keyDown events. A custom MDEF or MBDF selects the full guest framebuffer at original coordinates so GPUI chrome cannot cover it. Classic `InitProcMenu` retains the ID and loads its MBDF resource. A synthetic headless composed capture checks placement and pointer translation. Focused guest tests confirm fallback selection during a real 68K MDEF callback in a PowerPC app and during a native PowerPC MDEF invocation; the cross-CPU tracking test confirms live menu pixels and save-under restoration. | Custom MBDF message execution is not implemented: Inside Macintosh V-250 defines Draw, Hit, Calc, Init, Dispose, Hilite, Height, Save, Restore, Rect, SaveAlt, ResetAlt, and MenuRgn messages. Verify a real guest composed capture and standalone 68K custom MDEF tracking; qualify guest autoKey timing during real held shortcuts, submenus, and tracking order. Qualify submenu contents, hover selection, disabled-state transitions, focus, and menu scrolling in composed and interaction tests across real applications. | Demo only |
| Window frames and title bars | `window_manager.rs` supplies ordered `WindowFrameSnapshot` records with a guest WindowPtr and lifetime generation; `gpui_demo_frames.rs` clips standard frame overlays against content and front windows. The presentation snapshot retains the actual WDEF ID but rejects rectangular overlays when either guest structure or content region has complex scanline data; those windows keep their guest pixels. Headless composed captures cover the initial three-window stack, guest title-bar drag, rear activation, grow-box resize, zoom out and back, and both close promotions on 68K and PowerPC, with identity, bounds, active state, order, and clipping checks. GPUI interaction tests verify guest-coordinate press, held movement, and release on the themed title bar; an integrated test drives the real guest through that route, checks moved bounds, activates an exposed rear window, then drags its title bar beyond the guest pane and outer host window, verifying one edge-clamped release and guest-owned movement on both CPUs. The same integrated test drags the GPUI grow corner through the guest event route, verifies one press/release pair, and checks the guest-owned +25/+25 bounds change without changing window identity on both CPUs. Host zoom-box clicks enter the guest standard state and restore the moved/resized user state on both CPUs without changing window identity; the default standard rectangle leaves the title bar below the menu bar. Focused 68K and PowerPC Toolbox tests honor application-written `WStateData.stdState` rectangles and restore `userState`; the PowerPC test checks both FindWindow zoom part codes. The rebuilt showcase now writes a 600×420 standard rectangle for its stacked inspector; headless composed captures show that guest-owned zoom and restoration on both CPUs. | Verify WDEF variants, real complex visible-region behavior and fragmentation fallback, physical desktop pointer capture and focus loss during tracking, and fullscreen transitions. | Demo only |
| Document gutters and grow box | `gpui_demo_frames.rs` themes only the guest-painted one-pixel delimiters and 15×15 grow corner after an actual `DrawGrowIcon` call; the rest of each content edge retains guest pixels. The overlay also clips against every visible guest control bound, including custom CDEFs, so a recorded control in the grow area retains its guest or control presentation. Both CPU gateways record the grow call with window lifetime and painted bounds; geometry mutations clear the signal, and the presentation snapshot rejects a reused WindowPtr or changed bounds. `control_snapshot` separately identifies real scrollbar records and their live value/range. Inside Macintosh assigns scrollbars to application-created controls in the content region and the size box to `DrawGrowIcon` (Macintosh Toolbox Essentials, pp. 4-4--4-5, 4-12, 4-111--4-112). | Verify custom CDEF painting beyond recorded bounds, guest redraw or blit after the grow call, nonrectangular clipping, and active/inactive transitions in real applications. | Confirmed grow pixels and recorded controls |
| Dialogs and alerts | `FixtureRunner::dialog_snapshot` exposes ordered items, types, global bounds, live text, enabled state, window lifetime generation, and active edit selection on both CPUs. The first enabled edit item now initializes the 68K DialogRecord and TERec with an insertion point at offset zero; PowerPC reads that selection from its active TERec. Fresh systemless-play runs against BasiliskII and SheepShaver show the same initial caret, and the showcase test verifies insertion and backspace on both CPUs. The demo overlays buttons, static text, value-backed checkboxes/radio buttons, and single-line edit fields in standard `dBoxProc` dialogs. Multiline/tall edit fields retain guest pixels, and all input remains guest-owned. Offscreen composed captures cover the showcase alert and both unchecked and guest-checked modal preferences states on 68K and PowerPC. A host checkbox click queues one guest press/release pair. | Complete TextEdit layout, modality, callback state, custom-item fallback, broader GPUI rendering, keyboard focus navigation and native accessibility qualification. Dialog checkbox/radio semantic callbacks now use fresh dialog identity, enabled state and visible clipping to queue ordinary guest presses; button keyboard callbacks use the same route. Guest checkbox tracking and value updates are tested across monochrome 68K, colour 68K and PPC. Explicit button accessibility dispatch and item replacement within a surviving dialog remain open. | Standard dialog slice |
| Buttons, checkboxes, radio buttons, popup controls | Both Control Manager dispatchers handle guest controls and their callbacks. `FixtureRunner::control_snapshot` reads live owner, definition, bounds, title, visibility, highlight, value, range, and lifetime generation on both CPUs. Standard document-window buttons, checkboxes, and radio buttons use GPUI Kit. Checkboxes and radio buttons use 12-point titles and a four-pixel indicator gap in guest coordinates, preserving compact guest bounds without clipping the showcase alignment labels. The standard popup CDEF also exposes its live private MENU ID and title width; a GPUI closed control reads its selected label from that guest menu, while open tracking and unknown definitions keep guest pixels. A guest pointer selection changes the live value and GPUI label on monochrome 68K, colour 68K, and PowerPC; composed initial and selected captures cover both CPU slices. Overlay order follows the guest's first-created-frontmost draw order, and a standard control intersecting a custom CDEF keeps guest pixels. | Complete keyboard focus navigation, dialog/button semantic activation and screen-reader qualification. Document checkbox/radio callbacks now queue identity-validated guest clicks, with explicit accessibility Click handlers; tab traversal remains disabled. The shared worker path is tested for radio tracking, guest-owned group updates and pointer restoration on monochrome 68K, colour 68K and PPC. Host pointer forwarding is checked for duplicate semantic requests; native accessibility dispatch and keyboard focus routing remain unqualified. Qualify generation behavior across process replacement, guest-reordered and dynamically redrawn controls, popup open-state rendering and custom CDEF fallback tests. | Standard controls demo |
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
guest queue. Disabled rows omit GPUI click handlers. Rendered menu tests cover
native accessibility state as well: disabled rows expose a disabled flag,
checked items expose a checked menu-item role, and submenu rows report their
expanded state. Systemless supplies the missing disabled-state setter through
a reusable element wrapper that preserves layout, input and accessibility
metadata. Rendered interaction tests check live disabled/checkmark changes
and submenu expansion without bypassing guest command validation. All nine
menu-related tests pass. VoiceOver navigation and activation still require
host-level qualification. A keyboard test covers hierarchical navigation
with Right and Enter; a live-update test covers item
changes and an unrelated menu appearing while the popup is open. Offscreen
composed captures of the real showcase menu
cover both 68K and PowerPC. An interaction test confirms a 40-item menu scrolls
its last keyboard-selected item into view. The interaction test also verifies
that removing the guest menu unmounts the open popup. A replacement at the same
MenuHandle with a new generation now also has a GPUI interaction regression:
the old popup disappears, stale open-menu state no longer pins the revealed
bar, disposal sends no menu command, and the replacement dispatches its current
generation. All 64 GPUI interaction tests pass with this cleanup. This checks
the frontend snapshot lifecycle; a real application's
disposal/reallocation sequence still needs end-to-end qualification. Selections still pass through
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
release. Standard-scrollbar wheel translation now posts guest arrow clicks,
with execution between press and release; it does not change control values
directly. General wheel routing and the broader qualification listed in the
inventory remain open.

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

A retained-press regression found that PPC ModalDialog returned standard
button hits on mouseDown while both 68K modes waited for release. PPC now
retains that event while the synchronous import tracks the button, returns
an item only for release inside the original item, and consumes the release
without returning a hit when it occurs outside. Queued mouseUp coordinates
take precedence over later host pointer state. The guest controls still own
the action and resulting value changes; no frontend record mutation is used.
The extended three-mode preferences regression holds Cancel, moves outside
and releases without dismissing, then clicks and releases inside to dismiss.
All seven GPUI dialog tests pass. This qualifies release semantics for standard
DITL buttons; pressed-state snapshot/rendering, custom filters and resource
control definitions remain separate open work.
All 633 library dialog tests also pass. The optional-button StandardAlert
regression now sends a real press/release sequence, checks that the alert and
output remain unchanged while held, and verifies the queued release selects
the correct button even after the current pointer has moved elsewhere.

Dialog item snapshots now carry the guest-owned tracking highlight. Classic
snapshots read retained Dialog Manager button tracking; PPC updates the live
ControlRecord highlight while its modal import is held, repainting only when
that state changes. GPUI push buttons follow this snapshot, gated by active
and enabled state, rather than inferring a press from host pointer position.
The three-mode regression checks highlighted-inside and unhighlighted-outside
states before release; all seven GPUI dialog tests pass.
`--capture-modal-dialog-button-held <png>` and
`--capture-modal-dialog-button-outside <png>` exercise these states through the
existing composed capture path with real guest pointer events. Keyboard flash,
checkbox/radio pressed presentation and custom filter tracking remain open.
Six composed captures were generated and reviewed: the held Cancel button is
shaded and returns to normal when the pointer leaves, while the dialog remains
open, on monochrome 68K, colour 68K and PPC. The library dialog suite passed
633 tests; after correcting PPC highlight storage to the documented control
part code, the focused StandardAlert test also verifies `inButton` (10) while
held and completes the release normally. The final seven GPUI dialog tests pass.

Held-checkbox qualification found another CPU mismatch: classic ModalDialog
returned the checkbox hit on mouseDown, allowing the application to toggle
its value before release. Classic checkbox/radio items now use retained
standard-control tracking, update the ControlRecord highlight without changing
the value, and return the item only after an inside release. An outside release
clears the highlight and leaves the dialog and value unchanged. Push-button
flash handling remains separate from checkbox/radio completion.
The preferences regression now checks unchanged value and exposed tracking
state while held, cancellation outside, and one successful toggle on an inside
release on monochrome 68K, colour 68K and PPC. All seven GPUI dialog tests pass.
GPUI checkbox/radio pressed presentation remains open: the Kit's `selected`
property means checked value and must not be used as a transient press flag.
Dedicated radio-button interaction and capture qualification also remain open.
All 633 library dialog tests pass after this tracking change, including the
existing push-button provider feedback test.

Checkbox pressed presentation now uses a shared Systemless component built on
GPUI Kit's semantic Checkbox and CheckboxIndicator bases. Both document controls
and dialog items supply guest value and tracking state independently. Holding an
unchecked box adds a foreground outline and secondary fill without a checkmark;
holding a checked box uses the active primary fill while retaining its checkmark.
Disabled controls suppress pressed feedback. Indicator, label and gap dimensions
follow the scene scale. The semantic checkbox role, accessible label and toggled
value are retained; accessibility action routing remains unfinished.

The existing composed capture path now accepts
`--capture-modal-dialog-checkbox-held`,
`--capture-modal-dialog-checkbox-outside`, and
`--capture-modal-dialog-checkbox-checked-held`. These drive guest input and assert
the exposed tracking state, without synthesizing host hover or press state.
Reviewed captures cover those three states and the existing
`--capture-modal-dialog-checked` state on monochrome 68K, colour 68K and PPC
(12 captures total). All seven GPUI dialog regression tests pass. This qualifies
the modal showcase at the existing capture scale and light theme; document
checkbox tracking, other scales/themes, disabled-state captures, radio pressed
presentation and operable accessibility still require qualification.

Document radio tracking exposed two additional runtime gaps. Classic
`TrackControl` stored a boolean highlight byte rather than its hit part; it now
publishes 10 for a push button and 11 for a checkbox/radio. Its classic and themed
control drawing accepts these part codes while retaining existing explicit
highlight-1 rendering. The pressed-button provider regression checks the new
part code and still verifies painted feedback.

PPC `TrackControl` previously returned a nil-action standard button/checkbox/radio
hit on mouse-down. It now retains the import frame until release, updates
`contrlHilite` as the pointer crosses the control boundary, restores the prior
highlight on release, and returns zero for an outside release. The application
still owns value and radio-group changes. Retained tracking checks the control
pointer, lifetime generation, active record and calling frame before resuming.
Queued mouse-up coordinates take precedence over subsequent pointer movement.

The new showcase regression verifies unchanged radio-group values during a held
press, outside cancellation, highlight restoration and mutually exclusive values
after an inside release in monochrome 68K, colour 68K and PPC. The existing document
checkbox regression also asserts unchanged value and part-11 highlighting while
held. All 242 library tests matching `control` pass. Radio GPUI pressed visuals,
custom action-procedure repetition, classic PPC pressed-pixel rendering and
retained-tracking disposal/identity-reuse qualification remain open.
The full 58-test GPUI suite passes after these changes, including the new radio
tracking regression and strengthened checkbox assertion. The ordinary desktop
frontend also passes its build check.

Radio pressed presentation now shares the Systemless choice-control visual
implementation with checkboxes, using separate GPUI Kit Checkbox and Radio
semantic bases. Document and dialog radios read guest tracking independently
from their selected value. The shared visual layer uses checkbox checkmarks and centred dots for selected
radio buttons, and scales the indicator and label in guest coordinates. The dot
follows Macintosh Toolbox Essentials (1992), Radio Buttons, p. 5-6. A fresh
colour 68K selected-radio capture from the live compositor was reviewed after
this correction; the earlier nine captures below predate the dot correction.
No host callback changes guest values.

The composed capture path adds `--capture-radio-held`, `--capture-radio-outside`
and `--capture-radio-selected`, driving the preferences page through guest menu
selection and mouse events and checking the resulting value and highlight.
Nine reviewed captures cover these states in monochrome 68K, colour 68K and PPC.
The radio tracking regression and all seven dialog regressions pass. A fresh
held-checkbox capture is pixel-identical to the prior reviewed colour 68K
capture after extracting the shared presentation module.

This verifies the document-radio state transition at the existing light-theme
capture scale, not complete scene readiness. The preferences captures show
white overlay rectangles against guest-drawn grey panels. Their native provenance
was unresolved at that review; the follow-up oracle check below resolves it.
The monochrome black panel interiors already have native evidence recorded above;
they are not a newly discovered GPUI defect. Dialog-radio fixtures, already-selected held
radios, disabled/inactive states, other scales/themes, and accessibility actions
still require qualification.

A fresh colour 68K native-oracle comparison resolves the preferences backplate
question. The same deterministic sequence ran through the existing headless
Systemless runner and BasiliskII with Mac OS 8.1: start the showcase, open Pages,
choose Game Preferences, settle, move the pointer away, and capture. Both runs
completed. The native capture has white rectangles behind checkbox and radio
labels within the guest-painted grey panels, matching the GPUI composition's
backplate treatment. The ordinary HLE framebuffer instead leaves grey behind
those labels. Removing GPUI's white backplates would therefore move this case
away from the native reference. This is a background-provenance finding, not
whole-image equivalence: native font weight, window chrome and scrollbar position
differ and are not qualified by this comparison.

For reproduction, use the existing play/oracle runners with an 8-bit 68K script:
run 180 ticks; mouse-down on Pages at (v=10,h=64); run 8 ticks; move to the fifth
item at (v=91,h=150); run 15 ticks; release there; run 120 ticks; move to
(v=550,h=760); capture. Compare with the opt-in frontend's `--capture-radio-outside`
preferences scene, considering only the control backplates. The earlier
monochrome oracle evidence and focused one-bit PaintRect regression remain the
authority for black custom panel interiors. Neither appearance should be replaced
by guessed colours sampled from neighbouring pixels.

Retained PPC standard-control tracking now has direct import-level lifetime and
release-timing coverage. Tests exercise buttons, checkboxes and radios with a
queued release inside/outside while the current pointer has already moved to the
opposite location. They verify returned part codes, mouse-up consumption,
highlight restoration and unchanged control values. Separate cases dispose the
control or replace its registered generation/handle pointer during a held press;
resuming returns zero, clears tracking and does not write into the stale record.
The generation/pointer cases simulate identity replacement and do not prove all
allocator-driven handle reuse paths.

An additional test found that a mouse-up queued before initial TrackControl entry
was left pending and the original hit part returned immediately. Standard PPC
nil-action controls now consume this early release and return its actual
inside/outside result before starting retained tracking. All 245 library tests
matching `control` pass, including the three new regressions. Action-procedure
repetition, nested callback frames and broader cross-CPU lifetime parity remain
open.
The seven GPUI tests matching `control` and the radio tracking regression also
pass against this change.

The equivalent 68K nil-action standard-control path now consumes queued mouse-up
positions both before initial entry and while tracking. A regression matrix
covers buttons, checkboxes and radios, inside/outside releases, and early/held
entry, with the current pointer deliberately opposite the release location.
It verifies part codes, stack completion, cleared tracking/highlights, event
consumption and unchanged application-owned values. The test reproduced an
outside release incorrectly returning a successful hit before the fix. Immediate
checkbox/radio completion also now returns part 11 instead of the button part 10.
All 246 library tests matching `control` pass. This establishes release-position
parity for these standard nil-action controls; it does not qualify callback
repetition, custom definitions or all control lifetime cases.
The seven GPUI control tests and the three-mode radio tracking regression also
pass with the 68K release-position change.

Retained 68K simple-control tracking now snapshots the registered control lifetime
and validates it and the handle pointer before further drawing or completion.
A direct trap regression disposes a held radio control, replaces its registered
lifetime at the same address, or changes its handle pointer. Every case must
return zero, clear tracking, finish the original stack frame and leave the old
record bytes untouched. Before the fix, disposal still returned part 11. All 247
library tests matching `control` pass. As with the PPC lifetime tests, simulated
address reuse is not proof of every allocator-driven reuse path; scrollbar,
popup and nested callback lifetime qualification remains separate work.
The seven GPUI control tests and radio tracking regression pass with the lifetime
validation enabled.

Open standard popup controls now have a shared read-only geometry snapshot and
GPUI presentation, in addition to the closed-control presentation. The snapshot
contains the live menu identity, guest pane rectangle, scrolling origin, exact
row heights and tracked highlight. The PPC adapter borrows retained tracking
state without cloning its saved framebuffer pixels. The live worker and composed
capture path feed the same popup component; pointer events still bubble into
the scene and the guest tracker owns cancellation and value changes.

The three-mode popup regression verifies that opening leaves value 1 unchanged,
tracking exposes highlight 4 with stable menu identity and bounds, release
commits value 4 through the guest, and completed tracking removes the snapshot.
The test passes on monochrome 68K, colour 68K and PPC. Reviewed captures from
`--capture-popup-controls-open` show consistent text, checkmarks, separator,
disabled row and fourth-row highlight in all three modes. The build also passes.

This is not complete popup qualification. Visual review found residual guest
border/shadow pixels at the right edge, especially on PPC. Scrolling indicators,
icons, styled rows, hierarchical chains, accessibility actions and broader live
input/cancellation coverage remain open. Custom definitions and PPC open submenu
chains retain guest rendering; icon-bearing snapshots and styled classic rows
are conservatively excluded. Styled PPC rows still need explicit presentation
metadata. These standard-component gaps must be completed, not reclassified as
permanent custom-definition fallback.

Popup border qualification now covers the full standard-MDEF ownership area:
the pane plus its two one-pixel shadow strips, preserving the unpainted corner
pixels. These regions are masked in the source texture before filtering, and
GPUI draws its border above the rows so selection backgrounds cannot erase it.
Classic popup-control chrome now uses the PopUp shadow plan rather than the
hierarchical-menu plan, eliminating a stray top-right pixel. Fresh reviewed
monochrome 68K, colour 68K and PPC captures show the black edge remnants removed.
The geometry regression and both existing popup tests pass (three tests), and
the opt-in example builds.

The broader library menu run passed 431 tests and failed
`hle_import_runner_builds_and_draws_mbar_resources`: at the MenuList +4 assertion,
the result is 0 rather than 1000. An isolated rerun with the classic renderer
change removed reproduces the identical failure. This pre-existing readiness
failure remains unresolved; the menu suite is not recorded as fully passing.

The menu-resource test failure above is resolved as a stale assertion, with no
runtime change. DynamicMenuList offset +4 is `mbResID`, the MBDF identifier and
variant, not the MBAR resource ID (Macintosh Toolbox Essentials, pp. 3-97--3-98,
3-104, 3-111). Loading MBAR 1000 with the standard definition must leave this
field zero. The corrected import-level test also asserts that the frontend
snapshot does not classify this standard bar as custom. All 432 library tests
matching `menu` now pass. This closes that specific test failure, not the broader
menu interaction, accessibility or popup presentation qualification gaps.

Popup cancellation now has a loaded-application regression across monochrome
68K, colour 68K and PPC. Each mode reopens the resource popup and releases outside,
on its disabled row, and on its separator. The held snapshot must report no
highlight while retaining menu identity; release must remove the popup snapshot,
leave the control value at 1 and preserve only the first item's checkmark.
Repeated reopening is part of the same sequence. All nine cases pass in
`popup_cancellation_preserves_guest_value_across_cpu_modes` (84.54 seconds).
This exercises the guest input/tracking and frontend snapshot boundary. It does
not establish host pointer routing, keyboard or accessibility cancellation,
rapid queued-release ordering, scrolling, or custom-menu behaviour.

A rendered GPUI event test now covers popup pointer routing on monochrome 68K,
colour 68K and PPC. Platform mouse-down, held movement and mouse-up events enter
the Demo window at positions derived from its display origin and scale. The test
checks the emitted guest coordinates, delivers those commands to the loaded
application, refreshes the live popup snapshot between phases, and verifies
highlight 4, committed value 4 and removed tracking after release. It passes in
50.24 seconds. This establishes event propagation through the rendered dropdown
and scene transform; asynchronous worker scheduling, outside-window capture,
keyboard and accessibility routes remain separate qualification work.

GPUI popup presentation now reserves the standard 16-pixel arrow slots when
content extends above or below the guest pane, and omits partial rows between
those slots as the guest renderer does. Arrow visibility uses the shared
MenuRows hidden-content calculation through GuestPopupSnapshot, rather than a
host scrolling state. A focused regression covers top, middle, bottom and
fully-fitting content (including a shorter separator row). The rendered popup
host-pointer regression still passes across all three modes (50.60 seconds).
Long-popup captures, sustained auto-scrolling and release near arrow boundaries
remain unqualified; this implementation does not close those inventory items.

The existing 55-item programmatic Theme popup now supplies a reproducible long
popup check: `popup_scrolling_reaches_last_item_across_cpu_modes` and
`--capture-popup-controls-scrolled`. Held input reaches the bottom, exposes only
the up indicator, highlights item 55 without changing the control value, then
commits 55 on release on monochrome 68K, colour 68K and PPC (50.54 seconds).
Reviewed composed captures exposed the host menu bar obscuring the up arrow.
The bar now sits below the tracked popup within the same scene input layer,
with its original host text metrics preserved. Fresh three-mode captures show
the arrow, complete rows and last-item highlight; a final colour capture also
verifies unchanged menu-bar text scale. The rendered host pointer regression
passes (50.26 seconds), as does the fullscreen menu-reveal regression.

This does not establish complete long-popup parity: the guest-computed Theme
popup width differs between classic and PPC, and reverse scrolling, arrow-boundary
release, font metrics and asynchronous input timing still need qualification.
Existing native reference captures remain available; this run did not generate
a new BasiliskII or SheepShaver oracle comparison.


Popup geometry follow-up: both control adapters now anchor their dropdown at
control-left plus the retained popup title width. The classic adapter sizes the
open menu from its menu items rather than the entire closed control rectangle;
`popupFixedWidth` still governs the closed box. This follows the title/box split
in Macintosh Toolbox Essentials (1992), pp. 5-25--5-27, and the existing native
Theme popup references. Focused classic and PPC tests assert the title offset;
the classic test also rejects expansion to the closed control width. The
showcase held-selection regression passes across monochrome 68k, colour 68k
and PPC (45.50 seconds), asserting the Loadout dropdown starts after its
60-pixel title and still commits item 4 only on release. This does not qualify
window-font metrics, reverse scrolling, arrow-boundary release or asynchronous
input timing.

Fresh composed long-popup captures for colour 68k and PPC were reviewed after
this correction: the Theme dropdowns now have matching placement and width,
leave the title visible, and retain the up arrow and last-item highlight.
All 46 focused library popup tests pass. The remaining native row-height/font
mismatch is not resolved by this geometry correction.


Long-popup reverse scrolling and cancellation are now exercised by
`popup_reverse_scrolling_and_arrow_release_preserve_value_across_cpu_modes`.
The same showcase sequence runs on monochrome 68k, colour 68k and PPC: scroll
to item 55, reverse to the beginning, verify stable menu identity and bounds,
release on the down-arrow area, verify tracking closes with value 1 unchanged,
then reopen and successfully select item 55. The test passes in 105.02 seconds.
This qualifies the guest tracking/snapshot path for those sequences; it does
not establish native timing parity, host asynchronous pointer routing, every
arrow boundary pixel, or window-font metrics. No runtime behavior changed in
this follow-up.


### Remaining popup typography contract

The window-font discrepancy is a confirmed implementation gap, not a GPUI theme
choice. Inside Macintosh VI (1991), “Running in System Software Version 7.0”,
p. 3-18 explicitly requires `popupUseWFont` to use the owning GrafPort's font
and size for the **active popup menu**, as well as its title. The shorter
Toolbox Essentials description only mentions the title and is insufficient
for implementing this variation. The showcase sets its owner port to
`applFont`, size 9, and creates Theme with `popupUseWFont`; its fixture comment
explicitly identifies Geneva 9. The existing BasiliskII scrolled reference
shows the corresponding smaller rows (Archive 09 through 55), whereas the
current composed GPUI capture shows Archive 21 through 55.

The traced gaps are:

- Classic `menu_rows` / `standard_menu_width` and dropdown painting in
  `trap/menu.rs` use system-font row metrics and Chicago 12 text. Popup-control
  tracking delegates to these helpers without a window-font context.
- PPC `ppc_calc_menu_size_with_resources`, `ppc_menu_item_appearances` and
  `ppc_draw_tracked_menu` likewise use system metrics. The control adapter
  delegates to `PopUpMenuSelect` without supplying the owner font.
- Retained menu appearances contain height and icon information, but no font
  context. `GuestPopupSnapshot` carries row geometry but no typography, and
  `gpui_demo_popup.rs` paints every row at 12 guest pixels.

Close this with a shared resolved menu typography context, derived from the
owning port only for `popupUseWFont`, retained through tracking, and consumed by
measurement, guest painting and the GPUI snapshot. Keep default menus on their
system font and preserve the owner port's font state; do not globally alter
menu metrics or infer a font from row height. Resolve font identity, size-zero
semantics, fallback/scaling and style/icon interactions using the existing
QuickDraw font machinery. Apply the same context to closed popup label/value
layout, including truncation, rather than fixing only the open menu.

Required evidence: flagged and unflagged controls sharing a menu; owner-font
changes between openings; matching metrics, hit-testing and selection on
monochrome 68k, colour 68k and PPC; small/large fonts, separators and icon/style
rows; scrolling and cancellation with the changed row heights; unchanged
ordinary menu metrics and owner port state; and reviewed composed captures
against the existing native references. This audit establishes the root cause
and implementation scope, not completion of typography support.


Popup typography implementation has begun with `GuestMenuFont`, an explicit
family/size descriptor carried by `GuestPopupSnapshot`. GPUI row text now uses
that descriptor's point size instead of its own literal 12. Standard menu text
measurement delegates to the descriptor, using the existing font resolver and
QuickDraw scaling. Tests cover size-zero normalization, different sizes, empty
text, saturated widths, and agreement with native QuickDraw measurement for
five font families, five sizes, and ASCII/Mac Roman/empty strings. All 434
library menu tests pass.

This is shared infrastructure, not a claim that `popupUseWFont` works yet:
both snapshot adapters still provide the system default. The next required
change is to retain the owner font when control tracking starts and thread it
through row/width layout, guest painting, and snapshots on both CPUs. Closed
control typography and the native comparison gates above remain unfinished.

The existing held-popup selection test also passes on monochrome 68k, colour
68k and PPC after this plumbing change (48.42 seconds).


The open control-popup path now retains `popupUseWFont` from the owner port on
both CPUs. Classic control tracking stores the descriptor; native tracking
stores it with each item appearance. Width measurement, row geometry, guest
painting and `GuestPopupSnapshot` consume that font. Ordinary menu entry
points continue to provide the system default. Font metrics are resolved once
per classic row-layout pass rather than once per item. No owner port fields
are changed to present the menu.

The existing long-popup showcase test now asserts the requested 9-point font
and native 12-pixel row height before scrolling to and selecting item 55 on
monochrome 68k, colour 68k and PPC; it passes (107.79 seconds). All 434 library
menu tests pass. Fresh composed colour 68k and PPC captures were reviewed and
show matching small text, smaller rows, the up arrow and final-item highlight.

This is not complete native popup parity. The new font-aware width is narrower
than the existing native Theme reference; the native reference also displays
a partial row below its scroll indicator while the current compositor skips
partial rows. Resolve native popup minimum-width/closed-box relationships and
partial-row clipping with oracle evidence. Closed-control title/value fonts,
hierarchical inheritance, styled/icon rows, font mutation/lifetime cases,
exact GPUI font-family mapping and performance remain unqualified.

The reverse-scroll / down-arrow release / reopen-and-select regression also
passes across all three modes with the changed font metrics (104.05 seconds).


The native popup scenario was rerun with the current showcase archive
(SHA-256 `b1218d2ce2273950fe30911c26cd1b0c0037f86c703cf7dce9906a79bcf4f381`)
through the existing BasiliskII play runner. The stored capture manifest had
named an older archive, so it was not treated as proof of current geometry.
The fresh 45-action run completed and its open/scrolled screenshots were
reviewed: Theme still spans approximately x=282..422 and exposes part of
Archive 09 below its up arrow. The width difference is therefore still real,
not just stale-reference evidence. A targeted native width/closed-control
probe is needed before choosing a minimum-width rule.

GPUI now renders rows intersecting the visible content area, letting the
scroll-arrow overlays and pane clip hide the covered portions. Previously it
omitted any partly covered row, despite the guest's existing hit test allowing
selection in the exposed portion. A fresh composed colour-68k capture was
reviewed after the change and now exposes partial Archive 09, with the up arrow
and last-item highlight intact. The example build passes. This is a shared
presentation change; runtime pixel painting still needs equivalent clipping
qualification, and no new PPC native capture was produced in this follow-up.


### Fixed popup width resolved against both native CPUs

A temporary copy of the existing dual-CPU showcase was built with its normal
MPW pipeline. The only geometry change was Theme's `SetRect(&r, 190, 136,
400, 160)` becoming `SetRect(&r, 190, 136, 500, 160)`; diagnostic text reported
`MenuInfo.menuWidth` before/after `DrawControls` and `StringWidth` at Geneva 9
and Chicago 12. The existing popup scenario was replayed in both BasiliskII
and SheepShaver. Both open-menu captures moved the right edge from 422 to 522
while preserving left=282: the fixed dropdown grew from 140 to 240 pixels.
The menu record reported width 143 after drawing, separately from the open
fixed rectangle. This rejects the hypothesis that the open fixed width is
solely the measured text width or a fixed minimum of 140.

Both CPU control adapters now use a shared `fixed_popup_menu_width` rule:
control width minus title width minus the 18-pixel arrow/end-cap area, then
the existing screen clipping. Automatic-width popup calls continue through
their existing menu-size path. The shared regression records the two native
samples and verifies unflagged controls do not receive this fixed override.
The long-menu showcase check additionally requires width 140, point size 9
and row height 12 on monochrome 68k, colour 68k and PPC. All 47 focused library
popup tests pass. This supersedes the earlier fixed-width text-measurement
assumption; broader popup typography and lifecycle qualification remain open.

The three-mode scrolling/selection test passes with all three geometry
assertions (51.59 seconds). The example build passes, and fresh colour-68k
and PPC composed captures were reviewed: both retain the partial top row,
up arrow and last-item highlight at the corrected fixed width.


### Closed popup owner-font rendering

Closed standard popup controls now retain their live owner-port font in the
shared control snapshot when `popupUseWFont` is set, following Inside Macintosh
VI, p. 3-18. GPUI uses the resolved point size for both the title and selected
value. The 68k control/dialog painters and PPC control painter also use the
resolved font for selected-text measurement, truncation and drawing. Baselines
use the shared saturating control-label positioning helper. Unflagged popups
retain the system font.

All 47 focused popup library tests and 248 control library tests pass. The
existing live-menu linkage test passes across monochrome 68k, colour 68k and
PPC and now checks Theme at 9 points versus Loadout at 12 points. The example
build passes. Composed captures after selection were reviewed in all three
modes: Theme title/value are smaller, while Loadout retains its larger text
and selected-value truncation. These captures qualify the shared GPUI output,
not exact native font-family or pixel parity.

Remaining typography work includes title justification/style flags, automatic
control geometry, arbitrary font scaling and fallback, font mutation and
lifetime cases, Appearance font-style precedence, exact GPUI family mapping,
and native comparisons of guest pixel rendering. This does not close the
broader popup or frontend readiness requirements.


### Popup font mutation and automatic geometry audit

The shared snapshot reader now has a passing regression for successive owner
font changes, zero-size resolution, owner replacement, unflagged system-font
controls, non-popup controls and disposed/repointed handles. Both CPU adapters
call this same reader. This is memory-model coverage, not proof of guest API
callback timing, redraw after mutation or native rendering equivalence.

Automatic closed-popup geometry remains a confirmed implementation gap. The
68k `popup_control_box_rect` calls `popup_menu_max_item_width`, which measures
Chicago 12 regardless of `popupUseWFont`; it then applies a minimum width of
80 and 40 pixels of padding. The PPC popup painter instead uses the supplied
control rectangle minus the title area and edge insets. These paths therefore
do not share an automatic-sizing rule. Inside Macintosh VI, pp. 3-18--3-19,
requires recalculation on redraw and describes sizing from the longest menu
item, title, arrow and whitespace. Before replacing these rules, compare
non-fixed controls at multiple owner fonts and title widths in both native
oracles, including menu mutation after creation. The earlier fixed-width
probe does not establish automatic-width behavior.


### Native automatic popup baseline on both CPUs

A temporary copy of the dual-CPU showcase was rebuilt through its existing
MPW pipeline with Theme's control rectangle `(190, 136, 500, 160)`, title
width 52 and `popupUseWFont`, removing only `popupFixedWidth` from the earlier
width probe. The existing popup action sequence completed through both
BasiliskII and SheepShaver. Closed and open captures were inspected on both
CPUs: Theme's closed box spans approximately x=282..400 and its open dropdown
x=282..398. Both are substantially narrower than the fixed-width baseline
(open x=282..522). The owner uses Geneva 9; the diagnostic text reports
`StringWidth("Deep Field Archive")` as 84 and the stored menu width as 143
after drawing. Stored menu width therefore must not be used directly as
the automatic owner-font popup width.

This provides a matching native automatic-width sample for both CPUs. It
does not yet establish the rule across fonts, narrow bounding rectangles,
title widths or live item mutation. The added control-record diagnostic was
partly clipped by the window bottom and is not evidence for record mutation.
The automatic-sizing implementation gap remains open pending those probes
and a shared geometry implementation.


### Native owner-font and item-mutation sizing

The same automatic-width probe was rebuilt with `TextFont(applFont)` and
`TextSize(12)` immediately before `DrawControls`. After the existing Loadout
selection sets value 4, its page redraw calls `SetMenuItemText` on Theme item
36, changing `Deep Field Archive` to `Deep Field Archive Extended`. This uses
the documented Menu Manager mutation path (Macintosh Toolbox Essentials,
Changing Menu Items), without disposing or recreating Theme. Both native
play runners completed; before/after closed captures and the final open
dropdown were reviewed on both CPUs.

Both show the closed Theme box growing from approximately x=282..442 to
x=282..501 after mutation, versus x=282..400 in the Geneva-9 baseline. The
final open dropdown reaches approximately x=500. The now-visible diagnostic
reports `contrlRect.left/right = 190/500` before and after mutation, while
`MenuInfo.menuWidth` changes from 143 to 205. Automatic presentation geometry
therefore changes without rewriting the guest's supplied control rectangle.
A shared implementation must preserve that separation and recalculate from
live menu content and the owner font.

The separate longest-text diagnostic overlapped existing page text, so these
captures do not establish its numeric value. Do not infer an exact padding
formula from them. Narrow-rectangle clamping, exact font measurement, title
placement and hit testing outside the displayed box remain to be qualified.


### Native narrow-boundary popup probe

The font/mutation fixture was rebuilt with only Theme's supplied right edge
changed from 500 to 350, and its text-width diagnostic moved clear of other
text. Both native runners completed. Closed and open screenshots agree on
68k and PPC: the closed box stays approximately x=282..390 before and after
mutation, while the final open dropdown spans approximately x=282..486.
The readable diagnostics report Geneva-12 text widths 116 before mutation
and 175 after mutation; the control record remains 190/350 and the menu
width changes from 143 to 205.

This proves closed-box clamping and open-dropdown geometry must remain
separate. It also prevents a premature formula: the preceding wide-boundary
probe's final dropdown reached approximately x=500, whereas this otherwise
equivalent narrow probe reaches x=486. Reconcile that native difference
before treating measured text plus constant padding as the full open-menu
rule. No guessed sizing rule has been added to the runtime.


The narrow-boundary probe was extended to measure the item immediately before
and after `DrawControls`, record `txFace`, and obtain `GetFontInfo.widMax`.
Both native runners completed and their diagnostics agree: the mutated text
measures 175 on both sides of drawing, the face remains 0 (plain), and widMax
is 15. The 68k initial capture similarly reports 116 before/after drawing.
This rules out a changed font face or a post-draw text-measurement artifact as
the cause of the wide/narrow dropdown difference. Native open-menu layout
and its relationship to the closed box still need resolution; these readings
are evidence, not a justified constant-padding implementation.


### PPC partial-row guest painting

The PPC standard-menu painter no longer discards an entire row when a scroll
slot covers part of it. It preserves the original baseline and icon origin,
clips text/marks/commands through the existing QuickDraw clipping path, and
bounds icons, separators, hierarchy indicators, selection and dimming to the
visible item area. Attached-menu first-row icon pixels remain intact. This
brings guest painting closer to the already-corrected GPUI partial-row
compositor and the reviewed native scrolling captures.

A pixel regression mutates a partially exposed row from blank text to ink and
requires nonempty changes exclusively inside its visible strip, protecting
the scroll slot and surrounding pixels. All five tracked-menu tests pass,
including icon precedence/variable rows at 1/2/4/8/16-bit depths; all 436 menu
library tests pass. The initial extra top inset failed the icon test and was
removed before this validation. No new composed capture is claimed here.
The 68k framebuffer painter still omits partial rows and requires equivalent
clipping; cross-CPU partial-row visual qualification remains open.


### 68k partial-row guest painting

The 68k standard-menu painter now follows the PPC partial-row behavior: it
keeps original baselines and icon origins while clipping text, marks, command
equivalents, resource icons, separators, hierarchy indicators, selection and
dimming to the visible item area. Styled framebuffer text accepts an optional
clip, including outline/shadow pixels and underlines. Existing unrestricted
callers retain their behavior, and plain glyphs entirely inside the clip keep
the 8-bit row-painting fast path.

The monochrome menu regression changes a partly exposed row from blank text
to ink and verifies every changed pixel lies inside the exposed strip. The
styled-text regression compares clipped output against unrestricted pixels
inside two clip rectangles for all eight standard styles, and verifies the
surrounding framebuffer is untouched. Both CPU partial-row tests pass, all
437 menu library tests pass, and all 103 framebuffer tests pass. This closes
the known whole-row omission in both guest painters. Fresh composed captures,
scrolling interactions with partial icon rows and broad presentation-scale
qualification remain outstanding; no complete popup-readiness claim follows
from these focused tests.


### Paired guest/composed popup verification

The shared fixture capture path now saves `<output-stem>.guest.png` alongside
composed captures, from the same RGBA frame before GPUI overlays. No second
guest run or alternate layout is involved. The existing guest-only Standard
File capture keeps its existing output behavior. The example builds, and
fresh scrolled-popup capture pairs completed in monochrome 68k, colour 68k
and PPC. Composed captures show the partial Archive 09 row, upper arrow and
Archive 55 highlight consistently; both 68k guest captures show those pixels
as well.

The PPC guest companion exposes a defect concealed by the GPUI overlay:
menu pixels outside the application window disappear during host composition.
The upper scroll slot also previously had a black fill behind a black arrow.
That fill now uses the menu background; the five focused tracked-menu tests
pass with a new background assertion, and a fresh PPC pair visually confirms
the arrow correction. The missing outer menu area persists.

Investigate `retained_host_overlay_rects` and the subsequent host chrome pass:
the current PPC retained-overlay list includes Standard File panels but no
menu panes. This is a suspected compositing cause, not yet a verified fix.
PPC raw-frame parity remains incomplete despite the correct themed overlay.


### Native PPC menus survive host composition

PPC retained host-overlay bounds now include the active root menu and every
submenu, using their saved extents to protect the shadow strips as well. The
host desktop pass therefore leaves native menu pixels outside WindowList
untouched. When a retained native overlay overlaps the menu bar, the classic
bar repaint is deferred: the native adapter has already painted that area.
Normal repaint resumes when the overlay is removed.

The native submenu regression verifies both pane extents and removal of all
protection after tracking is cleared. A framebuffer regression verifies that
a native pane crossing the bar survives composition and that the same pixels
repaint after disposal. All 438 menu library tests and 104 framebuffer tests
pass, and the GPUI capture example builds.

Fresh paired PPC captures were reviewed. The raw scrolling frame now retains
the full menu above and below the application window, the partial Archive 09
row, upper arrow, bottom highlight on Archive 55 and shadow. A separate
post-selection capture shows the Loadout value updated to 4, no residual menu
and restored desktop/window pixels. This resolves the specific clipping by
host composition exposed by the preceding paired captures. Broad nested
custom-definition visual qualification and automatic popup sizing remain
open.


### Reproducible popup matrix and current integration checks

All eight GPUI popup integration tests pass on the current implementation
(151.66 seconds), including pointer routing, live control/menu linkage,
scrolling to the final item, reverse scrolling, release on an arrow, reopening
and cancellation over outside/disabled/separator targets. The existing tests
exercise monochrome 68k, colour 68k and PPC guest paths.

The new capture script completed locally with six checkpoints and twelve PNGs.
All manifest hashes were independently rechecked; composed frames are
1800×1480 and guest frames 800×600. The scrolling pairs in all modes and the
PPC selected pair exactly match their reviewed predecessors; monochrome and
colour selected composed frames also match the earlier reviewed captures.
The macOS CI job now runs this matrix after its interaction tests and uploads
captures, logs and the manifest, retaining evidence even on a failed run.
Hosted-runner execution remains unverified until that workflow runs; local
success does not prove hosted graphics availability or complete UI coverage.


### Dialog item identity follow-up

Queued semantic dialog actions currently identify a dialog lifetime and item
number. This rejects replaced windows, disabled or obscured items, but does not
prove that the same item still occupies that slot. `SetDialogItem` explicitly
replaces an item's type, handle and rectangle without replacing its dialog
(Macintosh Toolbox Essentials, pp. 6-122--6-123).

`DialogItemSnapshot::control_identity` now exposes the backing ControlHandle
and its registered lifetime on both backends. Both backends resolve the live DITL handle against the control registry and
verify its current master pointer and owning dialog. The 68K path does not rely
on the cached dialog-control mapping for this identity. A guest
checkbox test verifies nonzero identity and stability across a value change on
monochrome 68K, colour 68K and PPC. Queued dialog commands carry this identity and revalidate it before delivering
input. Resolver tests reject different handles and changed lifetimes, and the
three-mode guest test rejects missing/stale identities while preserving normal
checkbox activation. Actual guest replacement and DITL lifecycle qualification
is still incomplete.

Before qualifying asynchronous activation, cover SetDialogItem replacement, shortened
and re-appended DITLs, handle reuse, and process replacement with matching 68K
and PPC guest tests. Changes to a surviving item's label, value or geometry must
not be mistaken for disposal. Comparing display text or cached rectangles is
not a substitute for lifetime identity. This remains an open readiness gap.

The dialog-control identity change passes the public crate check with default
features disabled and the ordinary desktop binary check. Focused GPUI tests
cover semantic dialog activation across all three guest modes and unchanged
single-press/single-release pointer forwarding for dialog buttons, checkboxes
and edit fields. These checks do not establish native screen-reader operation,
keyboard traversal, or full dialog replacement qualification.


### Host menu keyboard focus

Ordinary key events reach the guest only while the guest scene owns keyboard
focus. Host-focused menus retain their own keys; Command shortcuts retain the
existing guest route. Closing the last host menu restores scene focus. The
hierarchical menu test checks an unbound printable key while open, guest typing
after selection, and typing after Escape cancellation (after delivery of the
GPUI dismissal event). This does not qualify full control traversal or returning
to an independently focused host text editor.
