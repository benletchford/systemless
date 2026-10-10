# GPUI Kit system-interface coverage

The [typography policy](GPUI_TYPOGRAPHY.md) records font-resource precedence,
bitmap and outline fidelity, display-resolution synthetic styles, guest layout
authority and the evidence required for text qualification.

Qualification correction (2026-10-10): the styled ink, selection and caret
capture helper initialized PPC at 8bpp when no explicit depth was supplied.
Earlier compositor captures labelled PPC16 actually establish PPC8 raster
evidence. The manifests now record the actual depth; the full ink/selection
matrix verifiers refuse qualification until true PPC16 recaptures replace the
missing coverage. The two-case caret colour counterexample remains valid as
colour 68k versus PPC8. Existing CPU-specific native unit tests are unaffected.
The helper now explicitly sets PPC16 by default and emits actual depth/state
sidecars. The corrected 192-case isolated styled compositor matrix is archived
in `styled-text-qualified`; it establishes true PPC16 fixture raster evidence,
but does not qualify the live Demo. The shared Demo multiline matrix is archived
in `styled-text-multiline-shared`. The separate 192-case shared Demo single-line
capture run is archived in `styled-text-single-line-shared`, pinned to f565b695.
All 192 fixture raster/state cases pass; this earlier bitmap evidence does not
qualify the newer smooth typography or production readiness.

### Standard File filename abbreviation

The GPUI Open and Save row painters now use the guest's CPU-specific display
policy: 68k abbreviates names longer than 36 characters with three periods;
PPC retains the full name. Snapshot entry names and accessibility labels remain
complete, so display abbreviation does not change selection or file operations.
The text continues through the guest glyph canvas rather than host typography.
The character-boundary test covers accented Mac Roman characters and exact-limit
names. The modal snapshot regression verifies the policy, text origins and
directory markers in monochrome 68k, colour 68k and PPC, with both semantic and
pointer routes (8.99 seconds). The GPUI Open/Save click routing test passes
(0.70 seconds). These checks do not establish long-name visual parity, selected
or scrolled row parity, or the remaining directory/volume label typography.

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
The implementation is staged in a draft PR and must not be merged or released
until explicitly authorized.

GPUI is now the default and sole desktop presentation frontend on this branch.
Release readiness is the completion gate, not a feature flag. Superseded winit,
software-buffer, Metal, D3D and native-menu presentation paths are removed.
Headless execution and guest Toolbox/CPU services remain authoritative. Native
guest identity and Dock icons remain host services connected to GPUI. Other
host platforms currently retain headless execution only; GPUI presentation is
macOS-only and cross-platform production readiness is not claimed.

The text gate covers every recognized standard menu, window title, button,
dialog, list, Standard File panel and TextEdit field in monochrome 68k, colour
68k and PPC. GPUI must preserve guest text, font/style intent, bounds, wrapping,
alignment, clipping, selection, caret, scrolling and editing. Displayed glyphs
and guest hit testing must agree across scales and active/inactive states in
the shared live/headless compositor. Extend Systemless components when GPUI Kit
cannot express these requirements. Modern host typography is not evidence of
classic Macintosh font fidelity: changed metrics require an explicit policy
and layout/interaction qualification. Application-drawn text retains guest
rendering until its ownership and faithful replacement are established.

Current typography does not yet pass the full gate. Standard menus, window
titles, controls, recognized dialog text, Standard File rows, supported plain
TextEdit, qualified styled TextEdit, and qualified standard list cells now use
shared guest glyph recipes, with native fallback where faithful ownership is
unproven. The later surface-specific records describe their implementation and
qualification limits. Styled fields and list cells require current native paint
evidence; partial fixture coverage does not qualify arbitrary guest fonts,
custom backgrounds, definitions, clipping or lifecycle behavior. Broader layout,
interaction, accessibility and production qualification remain unfinished.
The user has requested crisp GPUI typography. The current binary glyph canvases
remain an unfinished visual state. The smooth path must use display-resolution
outline rendering where the resolved guest font has outlines, preserving guest
advances, line breaks, baseline, clipping, selection and caret geometry. A host
font substitution requires an explicit policy and qualification; it cannot
silently change layout. Bitmap-only fonts require a separately stated fidelity
policy. Existing binary-pixel comparisons do not qualify smooth rendering.

The plain-document text component uses smooth resolved outlines through GPUI
where available, retaining binary glyph spans for unsupported sources. Both
paths use the same resolved guest FONT/NFNT/sfnt, explicit override, or bundled
fallback as QuickDraw. Its selection and caret share the glyph advances and
guest fontAscent. This establishes consistency with Systemless guest drawing;
it does not assert that bundled outlines reproduce original Apple bitmaps.
Authentic guest resources/explicit overrides retain precedence. Substituted
strikes requiring rescaling and unsupported justified records retain guest
pixels pending faithful support. Supported styled records now use a separately
qualified whole-field painter; unsupported records retain guest pixels. These
paths establish consistency with current guest paint, not universal font fidelity.

Standard window titles now use the shared Font Manager system strike (font 0,
12 points), WDEF horizontal origin, baseline and title clip. GPUI paints its
border separately so layout does not inset the glyphs. The PPC guest WDEF draw
now honors the same title clip; paired images exposed a previously unclipped
descender below it. Unknown definitions and substituted strikes retain guest
pixels. Host ellipsis and theme-font shaping no longer replace these titles.
The all-mode menu/window lifecycle regression and GPUI title-drag regression
pass. The focused PPC descender-clip regression also passes (0.13s), checking
that title ink stays visible inside the clip and cannot escape below it.
Paired active/inactive stacked-window captures across the three modes
are recorded in
[`text-classic-window-titles-review.json`](tests/toolbox-showcase/reference/gpui-demo/text-classic-window-titles-review.json).
The reviewed colour 68k/PPC guest glyph and composed-title regions are identical
after clipping. This does not qualify all WDEFs, scales, physical host input,
accessibility or live performance.

The actual guest geometry/typing regression passes in all three modes (36.57s),
including a click at a shared-strike insertion boundary. The GPUI platform event
regression passes first/second-line clicks at 0.75x, 1x, 1.5x and 2x scene scales
in all three modes (119.31s). The clipping/custom-definition fallback test also
passes, including an unqualified rescaled-strike size. Six selected/inactive
composed captures and their unmodified guest frames are recorded in
[`text-classic-strike-review.json`](tests/toolbox-showcase/reference/gpui-demo/text-classic-strike-review.json).
The selection-only difference occupies the same 160x30 physical-pixel region
in all modes, and the colour 68k/PPC document regions are byte-identical.
This qualifies the showcase plain Geneva 9 slice, not all fonts/styles,
physical host input, original Macintosh font fidelity or live performance.

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
| Lists | `FixtureRunner::list_manager_snapshot` retains ListHandle/generation, owner, geometry, native LDEF classification, logical cells, selection, visibility and active state. Resource-zero painters on both CPUs additionally retain actual draw-time bytes, font/size, baseline/inset, clipping, spacing representation and backing raster. The shared worker qualifies every unchanged owned physical pixel against the native cell recipe; GPUI paints that recipe with device-snapped canvas edges and clips against cell/window visibility and exact native paint regions. Application modifications and custom LDEFs remain guest-rendered. Accessibility labels remain available when paint declines; guest LClick owns selection. The shared Demo source-masked archives cover 16 selected and 48 scrolling/inactive/reactivated cases across mono/colour 68k, PPC8/PPC16 and four scales, with native-depth/state/hash checks. | Complete integrated GPUI pointer, keyboard/accessibility actions, scrolling and lifecycle qualification, complex visible regions, overlapping windows, callbacks, mutation/disposal/reuse and real-application performance. The loaded-guest centered-scene click regression passes two rows at all four scales and CPU/depth modes; physical host input and broader gestures remain unqualified. | Native-qualified standard paint; broader qualification open |
| TextEdit and editable fields | `FixtureRunner::text_edit_snapshot` reads canonical guest TERecs with stable TEHandle/generation, owner port, local/global view and destination bounds, text bytes, selection, activation, alignment, font fields, guest line starts and height, and private scrap. The showcase checks owner-port geometry, guest-owned typing, Reset selection through `TESetSelect`, and selected-text replacement through `TEKey` on monochrome 68K, colour 68K, and PowerPC. A deterministic 68K guest sequence and BasiliskII both end with a 195-byte, five-line record and selection `[1,1]`; the PowerPC guest sequence reaches the same state. GPUI ASCII keys and arrows enter the existing guest event route; a host event test verifies the translated press/release pair. Dialog items carry edit selection; standard single-line DITL fields use a read-only GPUI presentation. Drawing evidence is independent of optional sharp-text rendering, includes the guest clip/visible regions, and compares against the actual presented framebuffer. Allocated but unpainted records and records overwritten by another page remain hidden. Ordinary unstyled left-aligned document TextEdit uses a clipped GPUI overlay with guest line breaks, scroll origin, selection, and caret. Supported styled document fields now use whole-field native-qualified GPUI paint with guest style runs, selection ordering and caret recipes; unsupported paint, justification and custom overlap retain guest pixels. The shared Demo source-masked selected multiline matrix passes all four CPU/display modes and scales. Centered-scene styled click/drag tests pass across those combinations. Earlier isolated captures retain their original scope; shared-renderer inactive/caret/lifecycle qualification remains outstanding. | Qualify font metrics, multiline editing, caret blink, focus and composition state; test guest wrapping, scrolling, selections across lines, Mac Roman input, modifier and clipboard behavior, and keyboard/accessibility actions. | Standard TextEdit slice |
| Standard File panels | The 68K `_Pack3` and PowerPC import paths retain their own modal Open/Save state outside Window Manager records. `FixtureRunner::standard_file_snapshot` normalizes live mode, reply-record identity, per-invocation generation, panel bounds, directory contents, selection, save-name/prompt and keyboard-focus state, and the guest's standard Open/Save item geometry. GPUI overlays standard Open and Save panels with GPUI elements on both CPUs; legacy and custom calls retain guest pixels. PowerPC Save lists VFS entries and supports folder selection with Return or a guest-timed double-click, Desktop and parent-directory navigation, guest-owned filename editing, and destination-aware replies. A three-mode showcase test checks Open, cancellation, Save, guest-owned filename editing, and a second cancellation. The PowerPC Save list follows the standard display-list and filename-focus behavior described in Inside Macintosh: Files (1992), pp. 3-5--3-6. | New Folder now has shared guest state, GPUI rendering and semantic creation/cancel/duplicate-retry coverage on all three CPU modes, with reviewed initial composed captures. Finish native error alerts, pointer text editing, read-only UI states and durable persistence; complete directory popup and keyboard behavior, callback timing, entry icons, true scroll state, nested modality, caret blink/composition and native accessibility. Replacement uses the oracle-verified Cancel default; broader file-content replacement remains unqualified. | Standard Open/Save demo |
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
existing guest route. Closing the last host menu restores scene focus. Invalidating its guest menu
lifetime also restores scene focus when replacement unmounts the popup without
a normal dismissal callback; the live-menu replacement regression covers this. The
hierarchical menu test checks an unbound printable key while open, guest typing
after selection, and typing after Escape cancellation (after delivery of the
GPUI dismissal event). This does not qualify full control traversal or returning
to an independently focused host text editor.


Document and dialog choices now omit host focus and semantic activation while
their guest owner is inactive. Guest pointer forwarding remains available for
window activation. The document-checkbox interaction regression covers an
inactive owner, reactivation, disabling, and re-enabling without changing the
control identity. It verifies focus availability and a single forwarded press;
this is not a native accessibility or complete focus-traversal qualification.


### Choice accessibility disabled state

Document and dialog checkboxes and radio buttons now use a shared
`AccessibleComponent` adapter to add the disabled flag missing from Kit's
rendered accessibility node. It preserves the component's role, value, label,
actions and layout delegation. Inactive guest owners also disable semantic
interaction without suppressing guest pointer forwarding for activation.
The rendered-node regression verifies checkbox state and both selected and
unselected radio states across enabled, disabled and re-enabled transitions.
All seven `themed_` interaction regressions pass, including dialog pointer
forwarding, window dragging, lists and standard file actions. These tests do
not qualify native screen-reader dispatch or complete keyboard navigation.


### Button input follow-up

Document and dialog buttons use a shared Systemless presentation built on Kit's
base Button. Semantic focus is available only for enabled controls in active
owners. Pointer events remain available in every state so the guest can activate
windows and apply its own disabled-control rules. Kit's styled Button disabled
property could not provide that separation: an initial regression exposed its
pointer suppression, and that approach was removed.

Keyboard click callbacks and explicit accessibility Click actions queue the
existing identity-validated guest activation commands. Document pressed state
follows the guest's inButton highlight; dialog pressed and default state follow
the live dialog snapshot. The shared accessibility wrapper supplies disabled
state while preserving button semantics. Full keyboard traversal, native
screen-reader dispatch and broader visual qualification remain open.

Document and dialog interaction tests cover inactive owners, disabled controls,
and reactivation, requiring exactly one guest press and release and no duplicate
semantic command for each pointer click.


The button follow-up passes eight themed interaction tests and the production
GPUI example check. Fresh composed modal-dialog and document-controls captures
were reviewed in monochrome 68K, colour 68K and PPC at the default capture scale:
button labels fit and default/non-default presentation remains distinct. This
is six initial-state captures, not qualification of all scales, themes or held
states. The monochrome modal capture contains a black guest background region
behind the dialog. The paired raw guest framebuffer contains the same black
region, so it is not introduced by GPUI button composition. This is the same
custom-panel drawing already matched by the one-bit BasiliskII qualification
above and covered by `one_bit_color_port_light_gray_fill_matches_native_pattern_fallback`.
It must be preserved rather than recoloured by the frontend. The broader
monochrome qualification remains open.

The document-button regression also verifies the rendered accessibility node's
Button role, label and disabled flag through inactive, disabled and re-enabled
states. This checks node metadata, not native screen-reader dispatch.


### PPC SetDialogItem lifetime qualification

The PPC dialog import regression now checks frontend snapshots after actual
SetDialogItem dispatch: an unregistered handle has no control identity; two
successive live replacement controls expose different identities; reinstalling
the same control with changed geometry preserves its identity; and DisposeControl
removes that identity while the DITL still references the disposed handle.
This focused regression passes. The existing 68K text/control record replacement
regression also passes, but does not yet exercise the frontend lifetime snapshot.
Neither result closes queued GPUI action rejection across replacement, DITL
shortening/reappend, handle reuse, or process replacement qualification.


The semantic-dialog integration regression now changes a live DITL checkbox
slot to another registered control on monochrome and colour 68K. The snapshot
must expose the replacement identity, and an action carrying the original
identity must be rejected without moving the guest pointer. Restoring the slot
allows the existing guest tracking/value-change checks to complete. This test
passes across its three-mode run; PPC retains the normal activation checks.
The 68K replacement is injected directly into DITL memory, not performed by a
guest SetDialogItem call, so end-to-end replacement qualification remains open.

The full 70-test GPUI suite passed after the shared button changes (189.36s).
The subsequent live-DITL stale-action assertions passed in their focused
three-mode semantic-dialog regression. These results do not close the remaining
readiness gaps listed above.


### Standard File list accessibility structure

Open and Save lists now expose a named ListBox containing ListBoxOption rows,
with selection supplied only by the guest snapshot. The focused host regression
checks Open selection, an empty Save list, populated Save selection changes,
and no generated guest input when presenting those changes. Existing Open/Save
button press/release assertions still pass. This qualifies metadata and input
non-interference, not native screen-reader navigation or activation.


Open/Save action buttons now share the guest button presentation with document
and dialog controls. Disabled Open suppresses host focus but still forwards one
guest press/release so the guest retains event ownership; the focused Standard
File regression passes. Four composed captures (Open/Save on colour 68K and PPC)
were reviewed: button labels fit their guest bounds. Panel borders and Save
layout still differ by backend; these remain visual parity gaps. The initially
clipped 68K Open directory label now uses a width-constrained text child, and a
fresh composed capture confirms proper ellipsis within the guest bounds. Keyboard and explicit accessibility action
routing for Standard File buttons is covered by the follow-up below.


### Standard File semantic button actions

Open/Save, Cancel and Desktop now have explicit keyboard and accessibility Click
handlers routed through serialized guest mouse tracking. The worker resolves the
current panel identity and geometry, rejects stale identities and unavailable
Open selections, then delivers a press, guest execution, release and pointer
restoration. Physical pointer clicks continue through the ordinary guest input
path; the host regression rejects duplicate semantic activation.

The guest-session regression exercises Cancel alongside the existing Escape
path in monochrome 68K, colour 68K and PPC. It checks stale generations, actions
from a dismissed Open panel while Save is active, actions after dismissal, and
pointer restoration. This does not qualify native screen-reader dispatch,
keyboard focus navigation, Accept/Desktop outcomes, nested confirmation dialogs,
New Folder, or complete file-panel parity.

Both focused Standard File regressions pass on this revision (7.15s total).


### Standard File acceptance qualification

The Save acceptance regression reads the returned StandardFileReply through the
existing observational debugger API, using the active CPU address space. It
checks guest-edited filename bytes, sfGood, sfReplacing for a new filename,
panel dismissal and pointer restoration in monochrome 68K, colour 68K and PPC.
The GPUI test feature enables the debugger for this observation; the production
GPUI feature remains unchanged.

Source inspection before the follow-up below confirmed that **both** Save
backends calculated sfReplacing and returned immediately for existing filenames.
Neither performed the subsidiary confirmation required by Inside Macintosh:
Files (1992), p. 3-7. The follow-up addresses that shared gap; complete file-panel
qualification still requires New Folder and the remaining workflow evidence.

The focused three-mode Save acceptance regression passes (3.24s). This checks
the returned record, not complete file persistence or replacement behavior.


### Shared Save replacement confirmation

Both retained Save backends now present a subsidiary confirmation for an existing
filename. Cancel returns to Save with its filename preserved; Replace alone
returns the accepted replacement reply. Shared geometry and event interpretation
serve native drawing and the GPUI AlertDialog. Entering or leaving confirmation
changes the panel generation, preventing queued actions from crossing modal
states. Parent Save semantic actions are rejected while confirmation is active.

The three-mode guest regression covers new-file acceptance, name-conflict
confirmation, cancellation back to Save, reopening confirmation, confirmed
replacement and returned sfGood/sfReplacing/name bytes. The snapshot/lifecycle
and acceptance regressions pass alongside the existing pointer regression.
The GPUI host regression additionally checks AlertDialog metadata and one guest
press/release within Replace, without duplicated semantic input. Visual capture
review, native oracle comparison, keyboard confirmation/cancellation, full modal
focus and screen-reader operation still require qualification. New Folder remains
unimplemented.


Four Standard File library regressions and all eight existing Save regressions
pass. The 68K directory-navigation test now explicitly confirms Replace before
checking the reply. Three composed replacement captures were reviewed across
monochrome 68K, colour 68K and PPC: confirmation labels/buttons fit, and Save
stays GPUI-rendered underneath, preventing document overlays from showing through.
Existing backend differences in the parent Save geometry remain. Review also
identified the parent filename caret as incorrectly active; snapshots now clear
filename focus during confirmation and restore it on cancellation, with guest
integration assertions for both transitions.

The final focused Standard File run passes all three tests (12.55s), including
filename focus clearing and restoration across replacement confirmation.

Fresh final composed captures in all three modes confirm the parent filename
caret is absent while replacement confirmation is open.


### Replacement modal background semantics

While confirmation is open, the Save panel remains painted but its accessibility
subtree is hidden using AccessKit's subtree exclusion flag. Its buttons lose host
focus handles and semantic handlers, and regain them when confirmation closes.
The GPUI regression checks background focus suppression and restoration alongside
normal pointer delivery. A node-level regression checks hidden/unhidden state
without losing the parent role, label or identity. This qualifies the Save panel
boundary only; native screen-reader operation and modal isolation of unrelated
application/window controls remain open.


Replacement confirmation's guest integration test now includes Escape cancellation
and Return acceptance on monochrome 68K, colour 68K and PPC, alongside semantic
pointer activation. Ordinary typing while confirmation is open must leave the
parent filename unchanged. All nine Save scenarios pass (20.54s). Command-period,
keypad Enter, visible keyboard button feedback and native-oracle equivalence
remain unqualified.

The complete 72-test GPUI suite passes after replacement confirmation and its
modal-background/keyboard follow-ups (215.28s). Native replacement oracle replay
is being added separately; this suite result does not establish native parity.

The Save regression also passes Command-period cancellation and keypad Enter
acceptance on all three modes (12 scenarios, 61.99s under concurrent build load).
This extends keyboard coverage; visible keyboard feedback and native equivalence
remain separate requirements.


### Native replacement default-button correction

The Mac OS 8.1 BasiliskII and SheepShaver replays both show **Cancel** as the
replacement dialog default. Return cancels the subsidiary dialog and reselects
the entire filename in Save. This contradicts the initial implementation and its
self-consistent Return/Enter acceptance tests above; those earlier passing tests
are not evidence of native parity. Both backends and the GPUI default styling
now use Cancel, and cancellation restores filename selection. The guest
regression is updated to require default-button cancellation and an explicit
Replace click before accepting the replacement.

The native harness exposes the launched executable as `_PlayTarget`; the replay
uses that existing name. An initial replay using the application title created a
new filename and did not exercise replacement, so its captures are excluded from
replacement evidence. Native confirmation and return-to-Save checkpoints are now
visually verified on both oracles. Explicit Replace was then replayed successfully on both oracles.


The final native replay retains nine checkpoints per oracle and 42 hashed capture
files; all recorded capture hashes and the scenario hash were verified. Return
returns to Save with the whole filename selected; explicit Replace dismisses it.
The showcase subsequently reports its own creation error because FSpCreate is
called on the existing target, consistent on both native CPUs. This is not a
confirmation failure and does not qualify replacing file contents or persistence.

The corrected 12-scenario guest regression passes (35.02s), as do all eight Save
library tests (0.71s). Fresh composed captures on monochrome 68K, colour 68K and
PPC show Cancel as the default, matching the native observation. Broader layout,
callback timing, accessibility and full file-workflow qualification remain open.

### Native New Folder workflow baseline

The `tests/toolbox-showcase/oracle/standard-file-new-folder.json` replay now
captures New Folder on Mac OS 8.1 in BasiliskII and SheepShaver. All 34 recorded
capture-file hashes and both recorded scenario hashes were verified. Reviewed
checkpoints on both CPUs establish these implementation requirements:

- New opens a subsidiary dialog labelled "Name of new folder:" with the entire
  initial name "untitled folder" selected. Create is the default button; Cancel
  is secondary. The parent Save filename loses its visible selection while the
  subsidiary dialog is active.
- Escape returns to Save without changing its directory or filename and restores
  full selection of the original filename, "Untitled".
- Typing "gpui folder" replaces the selected initial name. Return creates the
  directory and navigates Save into it, with an empty contents list and the
  unchanged Save filename fully selected again.
- A subsequent Escape dismisses Save and the showcase reports "Modern Save:
  cancelled" on both native CPUs.

Inside Macintosh: Files (1992), pp. 3-6–3-7, documents the subsidiary dialog and
Command-N keyboard equivalent. The observed default name, default button and
selection restoration above come from the native captures. The replay uses a
live guest clock on BasiliskII and 60.15 Hz wall-clock pacing on SheepShaver;
audio is disabled. These captures establish a behavioral baseline, not a passing
Systemless implementation. New Folder remains unimplemented; duplicate/invalid
names, read-only destinations, persistence after cancellation, keyboard
equivalents, and custom callbacks still require qualification.

PPC `DirCreate` and `FSpDirCreate` now share a directory-creation operation that
can also serve retained Standard File tracking. Existing name normalization,
duplicate-directory replies, parent lookup and allocation behavior are preserved.
All five focused directory-creation tests pass, including a regression through
both imported APIs that verifies an invalid reply pointer leaves no directory or
consumed ID and a valid retry succeeds. This extraction does not implement the
New Folder dialog or qualify filename validation, file/directory collisions or
read-only volume errors.

The shared New Folder edit state now uses the existing byte-based TextEdit
buffer. Two focused unit tests pass for native initial selection, Create/Cancel
keys, Mac Roman insertion, caret/backspace, clipboard selection replacement and
the 31-byte HFS name limit. This state is not yet connected to either retained
panel event loop, process scrap or GPUI rendering, so these are component tests
only. A slash remains a legal name character at this layer; filesystem path
normalization must not turn it into unintended directory traversal when creation
is connected. Pointer selection and native clipboard parity also remain unqualified. Classic
TextEdit does not support Shift/Option-arrow selection (Text, 1993, “Caret
Position and Movement”); do not assume modern modifier-arrow semantics.

The 68K FSpDirCreate handler now delegates to a reusable child-directory
operation, preserving its empty-name, missing-parent, read-only and duplicate
file/resource-fork checks. Both existing FSpDirCreate tests pass after extraction.
Together with the PPC extraction, this prepares the guest filesystem operations
for retained New Folder tracking; it does not close the remaining workflow.

Inspection confirmed that 68K already encodes literal slashes in HFS name
components. PPC directory creation now reuses that encoding rather than treating
the slash as a VFS separator. Standard File directory rows and the current-folder
label decode it back to the guest name. Five literal-slash regressions pass,
including both PPC creation imports and a creation-to-Standard-File-label round
trip for `Audio/Video`. This closes that specific creation/display mismatch;
other PPC File Manager lookup, rename and deletion paths still need qualification
for encoded names. New Folder event-loop and GPUI integration remain open.

New Folder now has shared guest-coordinate layout and modal event interpretation.
The child lies within the existing retained parent's saved region; pointer Create
and keyboard Create both reject an empty name, Cancel remains available, and
mouse-up/background clicks do not accept the dialog. All three New Folder
component tests pass, including translated geometry and pointer/key agreement.
Neither backend invokes this state yet; native placement, pointer text selection,
guest reply/persistence effects and composed rendering remain unqualified.

The 68K retained Save loop now opens New Folder through New or Command-N,
routes child keys through the shared state and process TextEdit scrap, and uses
the extracted File Manager operation to create the directory. Success navigates
into it; cancellation returns to Save; both restore the filename selection and
advance the modal generation. The nine focused StandardPutFile tests pass,
including a new monochrome/colour 68K sequence that cancels the child, reopens it,
types a name, creates the directory, and cancels Save while verifying that the
directory remains and sfGood stays false. This establishes retained VFS state,
not persistence across process restart.

While this child is open, the 68K presentation snapshot temporarily declines the
GPUI Save overlay so the guest-rendered child stays visible. This is an interim
implementation, not a completed GPUI component or custom-definition fallback.
PPC tracking, GPUI child rendering, composed visual review, text-pointer selection
and native error-alert presentation remain open. Creation errors currently keep
the child open with a short descriptive message.

PPC now also retains New Folder, opens it from New/Command-N, processes its
shared edit state and TextEdit scrap, creates the directory and restores Save
focus/selection with a new modal generation. All ten focused StandardPutFile
tests pass, including the PPC cancel/reopen/create/cancel sequence. The PPC
reply remains caller-owned while the dialog runs; the regression checks its
sentinel before completion and sfGood=false after cancellation. Both CPU paths
currently show the child through the guest renderer, declining the GPUI Save
overlay during this transition. GPUI child rendering and visual qualification
remain open, as do PPC read-only volume and file-versus-directory collision
handling, error alerts, pointer selection and durable persistence.

PPC New Folder now checks directory/data-fork/resource-fork name collisions and
the ancestor volume's hardware/software lock bits before the creation operation.
Six New Folder tests pass, including both CPU workflows and a focused destination
check covering encoded slash names, case-insensitive collisions and both lock
bits. These checks are wired into the retained child; native error-alert visuals,
disabled-button behavior and full guest-driven failure/retry qualification remain
open. This does not establish equivalent validation for every PPC File Manager
creation entry point.

Both CPU snapshots now expose New Folder's live name, byte selection, error and
shared geometry. The GPUI Save panel includes New, and the subsidiary GPUI dialog
renders that state with Create/Cancel controls. Semantic actions validate the
current reply identity and generation and use guest clicks; parent actions are
rejected while the child is active. Save's accessibility subtree is hidden and
its buttons lose focus handles during the child. The interim guest-renderer-only
presentation described above is removed.

The focused GPUI file-panel interaction test passes with the child present: it
checks the Dialog role, background button focus suppression and one correctly
positioned guest press/release pair for Create, without a duplicate semantic
request. This is a synthetic presentation test, not composed visual review or
end-to-end semantic creation qualification. Those checks, native accessibility,
pointer editing and error-alert parity remain open.

The `--capture-standard-file-new-folder-composed` route now opens Save in the
showcase, invokes New through the identity-checked semantic action and captures
the shared live compositor. Fresh captures were reviewed for monochrome 68K,
colour 68K and PPC. All show the full initial folder-name selection, Create as
default, a readable child panel, and no parent filename caret. The route asserts
the guest child snapshot before capture and retains the paired guest frame.
Parent Save geometry still differs across CPUs. The monochrome fixture also
shows a black region in its application content; this remains a visual issue to
investigate rather than evidence of complete presentation parity. Creation,
cancel/error captures and end-to-end semantic-action qualification remain open.

The paired monochrome guest frame confirms that the black region already exists
before GPUI composition. It also exposes a guest-renderer selection defect: the
parent Save name remains highlighted while the child name lacks its selection.
The GPUI snapshot renders the intended selection correctly, but the ordinary
renderer must be corrected as part of the shared-behavior requirement.

The ordinary 68K renderer now receives the child's byte selection, and both
backends suppress parent filename highlighting during subsidiary dialogs. PPC
draws the selected child range using system-font byte widths and the applicable
selection theme. Six focused New Folder tests passed before the PPC paint change;
the GPUI capture build passes afterward. Fresh PPC and monochrome 68K guest
frames were visually reviewed and show child selection with no parent selection.
Highlight extent still differs between the two ordinary renderers (68K fills
the field while PPC measures the selected text), and the monochrome application's
black content region remains. These are still qualification gaps.

An end-to-end semantic New Folder regression now passes on monochrome 68K,
colour 68K and PPC (7.76s). It boots the showcase, opens Save, invokes New via
the activation bridge, cancels and reopens the child, types a folder name,
invokes Create, and cancels Save. It verifies directory navigation, preserved
Save name and restored focus, pointer restoration, rejection of parent actions
under the child, and rejection of stale parent/child generations. This closes
the basic semantic creation/cancellation sequence; error/retry, native
accessibility dispatch, pointer text editing and restart persistence remain open.

The semantic workflow now also exercises duplicate file and duplicate directory
names on all three modes. Each failure keeps New Folder open with dupFNErr,
preserves the parent directory/list/Save filename, and permits deletion of the
failed name followed by a successful retry. Empty-name semantic Create is
rejected. All six CPU/collision scenarios pass. This verifies the current
failure-and-retry state transitions, not native subsidiary error-alert parity.

The complete opt-in GPUI interaction suite passes after the New Folder changes:
73 tests, zero failures, 211.05 seconds. This includes the existing popup,
menu, window, control, TextEdit and file-panel regressions as well as the new
semantic workflow. It is regression evidence for the exercised paths; it does
not close the remaining visual, accessibility, persistence or real-application
performance requirements.

The monochrome Standard File fixture's black region originates in application
drawing: `DrawStandardFilePage` calls `DrawBeveledBox`, which sets RGBForeColor
to light gray (0xeeee in each channel) and paints the panel before drawing black
text. The paired guest frame already contains the region. Native monochrome
Color QuickDraw comparison is required to determine whether this is incorrect
runtime mapping or expected fixture behavior; it is not established as a GPUI
compositor defect.

### Native New Folder duplicate-name qualification

The `standard-file-new-folder-error` oracle replay completed on Mac OS 8.1 in
BasiliskII and SheepShaver. Both native captures show that attempting to create
an existing directory (including a case-only name difference) closes the name
entry dialog and presents a separate modal alert: “That name is already taken;
please use another name.” OK is the default action. Return dismisses the alert
and restores the Save panel with its original filename fully selected; it does
not return to the New Folder editor. The audit found that shared New Folder state and GPUI presentation retained an
inline error and permitted editing/retrying in that child. The implementation now
presents a single-action alert in both ordinary renderers and GPUI, blocks name
editing while the alert is active, advances the interaction generation on failure,
and dismisses back to Save. GPUI exposes the error as an AlertDialog and accepts
only its OK semantic action. The updated six-scenario workflow regression passed across monochrome 68k,
colour 68k and PPC, including rejection of the pre-error interaction generation.
The shared error input-isolation test and GPUI presentation test passed; the latter
checks AlertDialog semantics, background button focus suppression and physical OK
click forwarding. Composed captures from all three modes were reviewed: each shows
a complete, wrapped message and one default OK button, without a name editor or
Cancel button. Ordinary colour-68k and PPC guest captures were also reviewed.
The reusable capture flag is `--capture-standard-file-new-folder-error-composed`.
Read-only and other error messages remain unqualified against native oracles;
full application-wide modal isolation remains a separate readiness requirement. Files (1992), pp. 3-6–3-7 describes subsidiary Standard
File dialogs; the duplicate-error transition is established by the native replay.
Capture identities were verified against the completed manifest; the native runs
used a live guest clock (68k) and 60.15 Hz wall-clock pacing (PPC), with audio off.

Standard File modal boundaries now suppress semantic activation of background
Control Manager controls and Dialog Manager items even when their guest owning
window remains active. The worker explicitly rejects queued document/dialog
activations while a Standard File snapshot exists. GPUI removes background
button/choice focus eligibility and accessibility click actions, and suppresses
background dialog edit focus presentation. The presentation regression supplies
an otherwise-active document button, verifies loss of focus eligibility during
Standard File, and verifies restoration after dismissal. That test and the
six-scenario New Folder workflow across all three guest modes pass. This does
not yet qualify menu accessibility, standalone TextEdit/list semantics, native
VoiceOver navigation or every nested custom-dialog combination.
All nine existing dialog regressions also pass after the modal-boundary change,
including modeless lifecycle, nested modality, semantic checkbox activation,
and physical dialog-item input. Menu handling must retain guest-permitted
commands: Macintosh Toolbox Essentials (1992), “Menus in Dialog Boxes” explains
that System 7 modal dialogs may permit selected Edit and Help commands rather
than disabling the entire menu bar.

New Folder now interprets Up/Down as beginning/end of its single-line name,
following Text (1993), “Caret Position and Movement.” The shared-state test
passes for boundary movement and insertion at both ends, including unchanged
classic behaviour with Shift/Option held. The cross-mode workflow now asserts
caret positions after actual guest Up/Down events; all six scenarios pass
across monochrome 68k, colour 68k and PPC (30.26 seconds).
Pointer selection and multiline TextEdit navigation are separate unfinished
paths, not covered by this single-line correction.


The `standard-file-monochrome` native replay now resolves the black-panel
question above. BasiliskII/Mac OS 8.1 at saved `displaycolordepth 1` paints the
same solid black application panel as Systemless. The reviewed screenshot has
black-on-black checkpoint text and visible controls below it. All five retained
capture-file identities and the replay identity match the manifest. This is
native one-bit fixture behaviour, not a GPUI compositor defect; do not change
RGBForeColor/painting semantics to make this fixture look like its colour mode.
The existing `one_bit_color_port_light_gray_fill_matches_native_pattern_fallback`
regression covers the RGBForeColor(0xeeee)/PaintRect operation. The native replay
is deliberately BasiliskII-only so a colour PPC run cannot masquerade as
monochrome evidence. Other monochrome UI qualification remains necessary.

### New Folder pointer selection

The `standard-file-new-folder-pointer` native replay establishes matching Mac OS
8.1 behaviour on BasiliskII and SheepShaver: click beyond the name to place the
caret at its end, append `x`, drag back to the leading edge to select the name,
and type `a` to replace it. All six selection checkpoint images were reviewed;
34 retained capture files and both replay identities match their manifests.
Text (1993), TEClick, p. 2-85 specifies retained mouse ownership and selection
extension.

The shared New Folder state now retains an anchor, clamps Mac Roman byte
offsets, extends selection during held dragging and preserves the final range
on release. CPU adapters use guest font measurement, poll their existing mouse
state and consume the gesture's release. PPC discards old releases preceding a
new accepted event; the initial regression exposed a stale mouse-up that changed
an end caret to a partial selection. Stationary polling avoids redundant redraws.
Proportional glyph hit testing uses exact midpoint comparisons, including
odd-width glyphs. The 68k child scopes drawing to Roman system font 0/12 and
restores the caller's font, size and face; its hit testing uses the same metrics.

Validation:

- Ten focused New Folder tests passed before adapter integration; the additional
  proportional/Mac Roman glyph-midpoint test also passes.
- The six-scenario guest workflow passes on monochrome 68k, colour 68k and PPC,
  including assertions at mouse-down and release, appending text, dragging beyond
  the field, replacement, cancellation, duplicate-name error and successful
  creation. The latest run after the system-font correction passed in 33.17s.
- `--capture-standard-file-new-folder-selected-composed` performs a real partial
  drag through the shared live compositor. All three modes were reviewed; a fresh
  colour-68k capture after the font correction matches the PPC selected prefix
  for this checkpoint.

At the initial pointer milestone, pointer-to-GPUI-displayed-glyph alignment
remained incomplete because the host and guest fonts differed (addressed below
for the exercised New Folder paths). Host focus-loss handling,
double-click word selection, and full accessibility text editing also remain
unqualified. These results establish guest pointer semantics for the exercised
paths, not complete GPUI text-editing readiness. The full GPUI example regression
suite passed: 73 tests, zero failures, in 215.61s.

The initial alignment defect was confirmed in the live path: `pointer` applies
only the aspect-fit transform, while the New Folder field renders proportional
host text as separate prefix, selection and suffix elements with host padding.
The CPU adapters interpret that unadjusted position using guest font widths and
CPU-specific insets. A fix must use the rendered text geometry to resolve the
Mac Roman offset, preserve that mapping throughout captured dragging, and route
the result through guest interaction semantics. Verification must click actual
displayed glyph boundaries at multiple scene scales, including accented Roman
characters, rather than only exercising guest-coordinate endpoints.

The next alignment change exposes one global guest insertion x position per
Mac Roman byte offset (including EOF) in the New Folder snapshot. Both CPU
adapters obtain these positions from their existing system-font measurement
paths and preserve their field insets. This is read-only presentation metadata;
it does not change guest selection. The six-scenario New Folder workflow still
passes (34.67s). The New Folder frontend now renders one shaped text element with selection
highlighting and an independently painted caret. After paint, its actual glyph
positions map to the corresponding guest insertion positions. Mouse-down starts
translation inside the field; movement and release retain it outside the field.
Snapshot identity and text checks reject stale maps, and host input release
clears capture. No guest records are changed by this presentation mapping.

The host midpoint test passes at 0.75, 1, 1.5 and 2 scales. The live GPUI test
checks all 16 painted insertion positions in the default name and dispatches
mouse-down, outside-field movement and mouse-up, asserting translated guest
input coordinates. Both tests pass. Composed partial-selection captures were reviewed for colour
68k, monochrome 68k and PPC; the New Folder text and selected prefix agree. The
painted-layout test also passes for an accented Mac Roman name and an empty
field (0.15s), exercising their insertion positions and caret rendering. Remaining
qualification includes long-name horizontal scrolling, double-click selection,
composition and accessibility editing.
The mapping currently covers New Folder only, not other TextEdit surfaces.

The guest-side round-trip test also passes across all six New Folder scenarios
(62.91s): clicks at exported offsets 0, 1, 7 and 15 return those exact caret
offsets on monochrome 68k, colour 68k and PPC. This complements the painted host coordinate test.

The combined host-to-running-guest test now passes (12.23s). It opens New Folder
through guest Standard File, renders it at 0.75, 1 and 1.5 scene scales on all
three guest modes, reads the actual painted glyph positions, dispatches host
mouse-down/move/up events and verifies guest caret offsets and retained drag
selection after each event. The full GPUI suite passed before this additional
test (74 tests, zero failures, 227.25s); the added test passed separately. The
ordinary library Mac Roman geometry test also passed. These results qualify
single-click and drag alignment for the exercised short New Folder names, not
all TextEdit surfaces or complete GPUI readiness.

### Long New Folder names: confirmed remaining defect

A rendered regression using a 31-byte name (27 `w` characters followed by
`abcd`) fails the caret-visibility requirement: at the exercised scene scale,
the caret is at host x=491.02 while the field spans x=211.5..416.25. The new
local assertion intentionally exposes this unresolved overflow; it is not a
passing qualification. Text (1993), TEAutoView/TESelView and the automatic
scrolling discussion specify selection visibility and held-drag scrolling.
The dedicated native replay completed on Mac OS 8.1 in BasiliskII and
SheepShaver. All six long-name checkpoints were reviewed: both CPUs scroll to
show the trailing `abcd`, show the leading text after Up, and restore the tail
after Down. All 34 retained capture files and both scenario identities match
the capture manifests. BasiliskII used its live guest clock; SheepShaver used
60.15 Hz wall-clock pacing; audio was disabled for this UI replay.

This confirms that a fix must coordinate the guest field's scroll origin,
visible selection/caret, guest hit testing and themed text positioning. Moving
only the host text is insufficient: exported guest insertion positions could
otherwise lie outside the editable field and fail to begin a click gesture.
The scrolling implementation remains pending; no long-name fix is claimed.

The local scrolling implementation now retains a guest horizontal offset in
shared New Folder state. Both CPU adapters update it after editing and pointer
tracking, include it in hit testing, and subtract it from exported insertion
positions. Guest drawing uses the same offset and clips the name to its field.
The focused state test passes (retained visible caret, start/end movement,
scrolled hit testing and shortening the name); the existing six-scenario guest
workflow passes in 38.94s. GPUI still needs its corresponding host-font scroll
layout, so the rendered long-name regression remains unresolved. Native-versus-
Systemless long-name captures and broader drawing regression checks remain due.

The GPUI single-line renderer now shapes the name once, retains its host-font
scroll origin while the active offset remains visible, and derives painting,
selection, caret and pointer mapping from that layout. The previously failing
31-byte caret-visibility regression passes (0.11s). The live host-to-guest test
now types both short and long names through guest keyboard events and verifies
scrolled tail clicks and dragging across all three CPU/display modes and three
scene scales (32.14s). Each viewport restores the tail through Down before
clicking it; the first test revision incorrectly attempted to click a hidden
tail after leaving a different selection endpoint visible.

A new long-name composed capture uses real guest typing. Its colour-68k guest
and GPUI frames were reviewed: both clip the leading text and expose the final
`abcd` and caret within the field. PPC/monochrome capture review, start/end
navigation qualification, held out-of-field auto-scroll and broader drawing
regressions remain outstanding before staging this change.

PPC and monochrome 68k long-name guest/composed captures were also reviewed.
All GPUI modes expose the tail and caret inside the field. Both guest renderers
clip and scroll the tail; the PPC guest capture still lacks visible caret
feedback, unlike the 68k guest frame. Caret rendering/blink equivalence remains
an explicit qualification gap rather than being inferred from themed output.
The live integration test additionally sends Up/Down through GPUI and clicks
the newly revealed first/last glyph boundaries; it passes in 72.74s.

The full GPUI regression suite passes with the scrolling implementation: 75
tests, zero failures, 258.12s. After that run, the PPC guest field gained the
missing empty-selection caret; the example rebuild passes and its fresh guest
capture was reviewed with the caret at the scrolled tail. This final drawing
change does not yet establish blink timing equivalence. Held outside-field
auto-scroll, double-click selection, composition and accessibility editing
remain open, as do text surfaces outside New Folder. The broader readiness
goal remains incomplete.

The live integration test now holds a long-name selection outside each field
edge, advances the guest without further mouse motion, and verifies complete
selection plus release preservation across all three modes and three scales.
It passes in 73.78s. A dedicated native autoscroll replay is being qualified:
its initial PPC rightward checkpoints show retained selection, but the leftward
gesture started on the field border and did not select text. The corrected
replay moves that mouse-down inside the field; native parity for both directions
is not yet claimed from the initial capture.

The corrected native autoscroll replay completed on both CPUs. All eight held
and released checkpoints were reviewed: leftward dragging reveals the leading
text, rightward dragging reveals the trailing `abcd`, and release preserves the
selection and view. All 50 capture files and both scenario identities match
the manifests. These observations agree with the exercised Systemless held-
selection paths. Scroll velocity, double-click selection, blink timing,
composition and accessibility editing remain separate qualification items.

### New Folder caret timing

The local caret change follows Text (1993), TEIdle, p. 2-84 and the existing
CPU TextEdit paths: empty active selections blink at the default 32 guest-tick
interval. Both CPU adapters now advance shared caret state and redraw on a
visibility transition; the GPUI renderer consumes that snapshot state rather
than using a host timer. Editing and pointer release restart the visible phase;
nonempty selections, held tracking and error alerts suppress blinking.

The focused timing test passes, covering the 31/32-tick boundary, repeat idle
calls, visible-phase reset, tick wraparound and selection/error suppression.
The real guest workflow passes visible/hidden/visible checkpoints at 31, 32
and 64 ticks in monochrome 68k, colour 68k and PPC, including creation,
cancellation and duplicate-file/directory retry. Checkpoints advance through
guest tick boundaries: a GUI deadline is a cap, and PPC deliberately returns
at each VBL boundary rather than jumping to that deadline in one call.

All six visible/hidden composed captures and their paired guest frames have
been reviewed. The long name remains clipped at its scrolled origin, with
the caret appearing and disappearing at the same insertion point; neither
panel nor scene moves. Reproduce with `--capture-standard-file-new-folder-long-composed`
and `--capture-standard-file-new-folder-caret-hidden-composed`, each followed
by an output PNG path, using the `gpui-menu-demo` example with `gpui-demo-test`
and the showcase archive. Run each with `--screen-depth 1`, `--screen-depth 8`
and `--prefer-powerpc`. The hidden capture asserts the 32-tick transition and
unchanged selection/scroll endpoint. The non-test `gpui-demo` feature build
also passes without exposing test-only capture arguments.
The default-rate change passed the full 75-test GPUI interaction suite. The
subsequent adjustable-rate change has the focused validation described below.

Caret timing now follows the live guest `CaretTime` long at `$02F4`, seeded
to 32 ticks at launch. Toolbox Essentials (1992), p. 2-113 documents
GetCaretTime and its global; the Event Manager C summary supplies the address.
Both CPU TextEdit paths and New Folder idle processing read that value rather
than retaining a fixed interval. The PPC GetCaretTime import returns the same
live value. Fifteen focused caret tests, the PPC TextEdit blink/import test,
import classification and launch-default checks pass. They cover changes
from 32 to 64 to 5 ticks without reactivation, preserving the previous blink
timestamp, and shared-state tick wraparound. The expanded real New Folder
workflow passes all interval checkpoints in monochrome 68k, colour 68k and
PPC, with both duplicate-file and duplicate-directory retry paths.
Host-focus behaviour remains open.

### Host activation audit

Host focus is not yet a qualified Macintosh application lifecycle. The GPUI
`on_focus_out` and `on_focus_lost` callbacks call `release_host_input`, which
releases keys and a held mouse button and clears pointer/wheel tracking. The
session input API carries mouse and keyboard events only; it has no host
activation transition. Releasing input therefore does not prove guest window
deactivation, inactive text selection, stopped caret blinking, or resume
behaviour. Widget focus moving into a GPUI menu must also be distinguished
from the host application actually moving into the background.

The platform-window deactivation regression activates a GPUI test window,
holds a guest key and mouse button, then deactivates the window through the
GPUI test platform. It verifies one mouse-up and one key-up even after repeated
deactivation. It passes using the existing focus callbacks, without a separate
window-activation observer. This qualifies input cleanup only, not Macintosh
suspend/resume; a future lifecycle bridge must observe actual window activation
rather than interpreting every widget focus change as a foreground switch. Shared SIZE-policy
queries also have a passing regression for all four combinations of accepting
suspend/resume and owning activation: the ownership bit alone must not suppress
Window Manager activation events. These queries are not yet connected to host
transition scheduling.

Launch SIZE is now retained in process-owned state shared by the classic and
native adapters. Both GetProcessInformation paths report those flags instead
of a constant zero. Four focused tests cover the same flag combinations through
both guest ABIs, adapter attachment and detached snapshots, and successive
68k/PPC/68k launches ending with an application without SIZE. This establishes
the shared policy source; it does not implement suspend/resume event delivery
or background scheduling.

The shared Event Manager state now owns a foreground-transition sequencer.
Its model tests cover suspend handling before yielding, delayed activation
availability, peeking without consumption, policy combinations, modality,
background-only applications, rapid requests, and clipboard conversion flags.
Queue tests cover snapshot isolation, merge conflicts, preservation during
event replacement, and launch reset. Both Toolbox Event Manager gateways can
now select a prepared suspend/resume record with normal event priority and
masking. Paired tests verify stable posting ticks across repeated EventAvail
peeks, exclusion from GetOSEvent, survival of FlushEvents, key-event precedence,
and delivery before high-level events. Switch scheduling, the following Window
Manager activation, and the host activation callback still need connecting;
these tests do not qualify a complete guest suspend/resume lifecycle.

The existing Window Manager adapters already own activation delivery through
`CurActivate`/`CurDeactive` and coalesced activation records. A new host bridge
must use those lifecycle paths, rather than directly modifying TERec.active
or only suppressing a painted GPUI caret. Toolbox Essentials (1992),
pp. 2-51 and 2-59--2-61, and its SIZE resource description on pp. 2-115--2-119,
require application-specific scheduling *capabilities*, not application-name
special cases:

- Applications accepting suspend/resume events receive `osEvt` with high
  message byte `$01`; bit 0 distinguishes suspend from resume. Clipboard
  conversion on resume is a separate bit, justified by actual scrap changes.
- Applications also declaring `doesActivateOnFGSwitch` perform their own
  activation in response. Applications needing activation events must receive
  them through the Window Manager's existing delivery path.
- `canBackground` governs background null-event processing. Processes (1994),
  “About Processes” and “Process Scheduling,” place the actual suspension
  after the application receives
  suspend and next calls WaitNextEvent or EventAvail; freezing immediately on
  the host callback would prevent the guest from handling its own transition.
- Ordinary modal dialogs generally prevent a major switch, whereas movable
  modal dialogs permit one (Processes, “Process Scheduling”). Host blur must
  not indiscriminately inject a guest major switch into either modal loop;
  the embedding's deferral and input policy needs explicit qualification.
- `getFrontClicks` controls whether the click that resumes an application is
  subsequently delivered. Regaining host focus must not unconditionally
  place a caret, toggle a control, or select a menu item.

The fixture now handles window activation for TextEdit and modeless dialogs,
and implements the suspend/resume and private-scrap responsibilities declared
by its SIZE flags. Both executable slices have been rebuilt. The new
`oracle/modeless-text-activation.json` sequence retains a document selection
while opening a modeless dialog and restores it on document activation.
Fresh native runs on both CPUs show that selection lifecycle; this is window
activation evidence, not host suspension or clipboard-conversion qualification.
The TextEdit and window native reference sets have been refreshed against this
fixture (18 and 14 checkpoints respectively), with changed images reviewed.
Drawing, popup and list reference records have also been refreshed from new
native runs: drawing images are byte-identical, popup selection is preserved,
and reviewed list checkpoints retain selection, mutation, scrolling, resizing
and inactive-state behaviour.
The overview reference set is refreshed after its existing verifier passed
59 checkpoints and 85 reviewed regions per CPU, state relationships, and
isolated SysBeep output returning to silence. Fresh native sampled-audio
extractions at full, 75% and 50% volume are byte-identical to all three retained
259,733-sample waveforms on both CPUs. Reviewed status images and raw sample
intervals confirm continued playback after Flush, silence after Quiet without
normal completion, later normal completion, and disposal. The audio manifest
now records the new PCM hashes, extraction offsets and cancelled lengths.
The remaining native batch completed all 20 requested runs and all 380 file
hashes were verified. Reviewed Standard File checkpoints on both CPUs confirm
default replacement cancellation with filename reselection, creation entering
the new directory, duplicate-name errors, pointer selection replacement, and
held rightward selection scrolling. The final replacement fixture still reports
its documented FSpCreate error on an existing file; this does not establish
replacement-content persistence. Intermediate and remaining scrolling/error
recovery checkpoints still require review before claiming full replay qualification.
Eight reviewed native modeless activation checkpoints are retained in
`oracle/modeless-text-activation-capture.json`. Other affected native provenance
still needs refreshing before the rebuilt fixture can be treated as fully qualified.

This sequence exposed two runtime gaps: 68k ShowWindow omitted deactivation of
the previous visible front window, and PPC SelectWindow changed window chrome
without delivering activation events. Focused regressions now exercise those
transitions. GPUI text rendering also now respects the guest TextEdit active
state when painting selection highlighting. The reproducible capture flags
`--capture-text-edit-inactive` and `--capture-text-edit-reactivated` exercise
guest-owned selection and window changes and retain paired guest frames.
The monochrome and colour 68k pairs have been visually reviewed: inactive text is unhighlighted,
and activating the document restores its retained selection without moving it.
PPC visual review exposed stale guest selection pixels after TEDeactivate and
stale dialog pixels after selecting the covered document. These were invisible
to the state assertions: the earlier 76-test GPUI suite passed. PPC activation
now repaints TextEdit immediately, and SelectWindow requests a document update.
A pixel regression verifies selection removal without TEUpdate/TEIdle, and the
window regression verifies the update event. Fresh PPC composed and raw guest
captures have been reviewed: inactive highlighting is removed and reactivation
repaints the exposed document with its selection preserved. All 76 GPUI tests
passed after the PPC repaint changes. Investigation of the modeless
close-control difference found that 68k GetNewDialog discarded DLOG goAway
and refCon values; PPC retained them correctly. The 68k creation path now
passes both parsed fields through, with a resource-to-window regression;
all 21 matching GetNewDialog tests pass across both adapters.
After that change, all three GPUI modeless lifecycle/selection tests pass.
The coverage verifier includes the new activation scenario and passes its
16-page, eight-scenario inventory and capture identities; this does not qualify
host activation or the other implementation gaps below.
Refreshed monochrome and colour 68k composed/guest pairs now show the close
control requested by the resource, matching PPC. Reviewed activation captures
and their hashes are retained in `tests/toolbox-showcase/reference/gpui-demo/`
under `16-text-edit-{inactive,reactivated}-{mono,colour,ppc}` and
`text-activation-review.json`. The cross-mode
guest-state regression passes in all three modes; the PPC import regression
also verifies event order and that reselecting the active window posts no
additional activation event.

Required qualification is a matching native/Systemless sequence on 68k and
PPC: activate an insertion point, switch away, observe suspend and any required
activation event, verify inactive selection/caret and permitted background
progress, resume without editing, and confirm restored input. Repeat with a
modeless dialog, retained modal dialog, open menu, held key, and held pointer;
include both SIZE activation policies, repeated focus notifications, and
front-click suppression. Existing key/button-release tests cover only input
cleanup and must not be counted as this lifecycle qualification.


The shared process-activation state now retains launch SIZE policy across CPU
adapters and schedules requested transitions through both Event Managers.
Prepared suspend/resume records preserve timestamps across EventAvail peeks,
respect event masks and priority, and remain separate from GetOSEvent and
FlushEvents. Matching WaitNextEvent regressions cover all four combinations
of acceptSuspendResume and doesActivateOnFGSwitch, including the subsequent
yield that commits suspension and the absence of duplicate notifications.
The GPUI window-activation observer now sends foreground requests through the
session to this shared state. Its platform test verifies one suspend request
for repeated deactivation, held-input cleanup, and one resume request on
reactivation. Input/widget focus loss alone must not request a process switch.
These are runtime boundary and platform-adapter checks; background scheduling,
clipboard conversion and the native lifecycle qualification above remain open.

Foreground requests also wake a parked 68k WaitNextEvent through its existing
Event Manager return path. A regression uses a 3,600-tick sleep and verifies
immediate suspend delivery, event-mask filtering, interrupt-callback deferral,
and foreground ownership until the application's next yield. The 27 runner
event tests and 87 combined CPU Event Manager tests pass.

PPC WaitNextEvent now retains its native import frame and yields bounded
execution slices until a matching event or the original sleep deadline.
It no longer charges an entire sleep on immediate return. The 34-test PPC
Event Manager suite includes input wake-up, suspend wake-up, timeout expiry,
unmodified arguments and EventRecord while waiting, and slice-budget bounds.
All 18 WaitNextEvent regressions also pass. These checks do not establish
background CPU eligibility, activation-click policy or complete native
lifecycle equivalence.


The `showcase_text_selection_survives_host_suspend_resume` session regression
now passes on monochrome 68k, colour 68k and PPC. It selects guest TextEdit
content, requests two suspend/resume cycles through the frontend session API
(including duplicate requests), observes the guest osEvt messages and resulting
TextEdit activation, preserves the text and selection throughout, then verifies
that a fresh key press replaces exactly that selection after resume. The
showcase handles its own suspend/resume activation; the test does not mutate
TextEdit records. This is end-to-end Systemless session evidence, separate from
the GPUI platform focus observer test. Native oracle comparison and rendered
host-switch captures remain required, as do background scheduling and modal,
menu, clipboard and front-click lifecycle scenarios.

### Host activation capture evidence

The opt-in capture flags `--capture-text-edit-host-suspended` and
`--capture-text-edit-host-resumed` request process transitions through the
frontend session API. They wait for the expected TextEdit activation and
selection state, then a subsequent null event so guest painting has completed.
The six composed captures and paired raw frames are recorded in
`tests/toolbox-showcase/reference/gpui-demo/text-host-activation-review.json`.
Visual review confirms selection suppression/restoration and stable geometry
on monochrome 68k, colour 68k and PPC. The menu uses the existing transparent
SVG logo instead of the app icon's black tile.

These captures do not qualify native host activation observer integration or
native-oracle process switching. Monochrome custom guest panels remain black, consistent with the native
one-bit evidence and PaintRect regression recorded above. Background
scheduling, modal/menu transitions and clipboard conversion remain open.

### Clipboard bridge continuation

`MacintoshSession::import_clipboard_text` and the runner forwarding API now
replace the shared global scrap with Macintosh Roman TEXT bytes and mark
clipboard conversion for the next resume. Callers must supply CR line endings
and import before requesting resume. Private TextEdit scrap remains guest-owned.
The focused `external_clipboard_import_updates_both_gateways_and_resume_conversion`
regression passes: both attached CPU adapters see the replacement, old formats
are cleared, and conversion is requested on the next resume only. This is
shared-state evidence, not end-to-end native clipboard qualification.

The GPUI text-import bridge now reads actual string clipboard entries on initial
active presentation and host-window resume. It uses the shared Macintosh Roman
encoder, normalizes LF/CRLF to CR, and queues import before the foreground
request. Unchanged host text leaves newer guest scrap intact. Unrepresentable
Unicode rejects the entire import; non-text contents leave guest scrap intact.
Empty string entries import empty TEXT. File paths are not synthesized as text.

The two conversion/change-detection policy tests and real showcase
resume/private-scrap conversion regression pass (three tests, 30.08s). The
showcase test runs monochrome 68k, colour 68k and PPC, observes changed and
unchanged resume message bits, and preserves document text/selection. The
platform-observer ordering regression also passes within the full installed
binary suite (113 tests, zero failures, 368.42s).

Guest-to-host TEXT export now waits for the guest suspend handler and its next
eligible event yield. The worker retains export candidates across coalesced
updates; activation generations reject stale candidates. The macOS pasteboard
change count protects newer host copies even when their text is identical.
Unknown/mixed formats block export, and successful writes are remembered to
avoid importing the bridge's own output. Macintosh Roman is decoded exactly
and CR line endings become LF for host text.

The focused clipboard suite passes seven tests (29.89s), including GPUI test
platform read/write behavior and real guest Copy/private-scrap conversion in
monochrome 68k, colour 68k and PPC. The separate guest scheduling boundary test
also passes. These are mocked host-platform and actual guest evidence; physical
macOS clipboard behavior has not been qualified. Initial active import and
rapid suspend/resume coalescing still need guest conversion qualification.
Native clipboard integration, non-text format transport and native
oracle clipboard lifecycle qualification remain open; this does not qualify
the complete clipboard bridge or overall GPUI readiness.

### GPUI-only default migration

The installed `systemless` binary now launches the same GPUI frontend used by
the example/capture harness. CLI address mode, executable preference, guest
theme, arrow-to-keypad input, explicit fullscreen, initial physical display
scale, preference reset and save storage remain connected. Guest identity and
Dock artwork are retained independently of presentation. The removed legacy
window/menu/presenter and alternate-owner modules are not fallback choices.
A bounded CPU-frame helper now serves live GPUI execution and the existing
headless modal-scheduling regression. This is not complete live/headless
scheduler equivalence or game-performance qualification.

The default binary type-check, macOS no-default-features library check and
WebAssembly no-default-features library check pass. All 113 installed-binary
tests pass (368.42s), including platform activation/clipboard ordering and the
headless modal-scheduling regression. Offline packaging and verification of
the default packaged binary pass; its transparent logo is now a packaged
runtime asset, byte-identical to the website SVG. Post-migration physical
host/window, composed captures and real-game qualification remain pending. Windows/Linux desktop presentation remains unsupported; headless
buildability alone is not cross-platform production readiness.

### Classic glyphs in the New Folder editor

The shared GPUI New Folder editor now paints the guest Roman system font
(family 0, size 12), including its binary glyph ink and Mac Roman byte advances.
Selection, caret, horizontal scrolling and pointer mapping use those same
advances; modern host shaping no longer changes this editor's text width. The
CPU adapters explicitly choose this font independently of the caller port.
The existing themed field vertical layout is retained; exact per-CPU baseline,
inset and selection-height qualification still needs a richer snapshot.

The painted pointer regression passes on monochrome 68k, colour 68k and PPC at
0.75×, 1× and 1.5× with short and horizontally scrolled names, held selection
and release. It now also checks every glyph advance against guest insertion
positions (81.17 seconds). The default desktop type-check passes. Three actual
shared headless-compositor selected-prefix captures and paired guest frames
were reviewed; commands, hashes and limitations are recorded in
`tests/toolbox-showcase/reference/gpui-demo/text-classic-new-folder-review.json`.
This is not authentic Macintosh font, physical input, 2×, inactive-field or
non-ASCII-name qualification. Surrounding Standard File labels still need guest
font painting, and Appearance control font overrides need snapshot support.

### Appearance control font intent

Control snapshots now retain the process-owned `ControlFontStyleRec` on both
CPU adapters, including flag bits, font family, size, face, mode, justification
and foreground/background RGB words. The regular CDEF compositor retains guest
pixels for an override until GPUI can paint its style faithfully; intersecting
standard controls also retain guest pixels so the override is not erased. The
overlap regression passes. This is an intermediate fidelity boundary, not
completion of GPUI-rendered control text. Dialog-item overlays still need the
same font/style metadata and ownership treatment.
The shared reader regression preserves every style field while validating
record identity and owner-font mutation (0.01 seconds). Default desktop and
no-default-features type-checks pass. These are structural and compositor
ownership checks; no new native font/style visual qualification is claimed.

### Guest glyphs in standard button labels

The shared GPUI button wrapper now paints font-family 0, size-12 guest glyphs,
with the guest integer centering rule applied before presentation scaling.
CDEF, dialog and Standard File buttons use this wrapper. The border is a
decorative child so it cannot inset the guest label coordinate space. Labels
retain their accessibility text and guest event routing. Regular CDEF font
overrides retain guest pixels; dialog override metadata remains unfinished.

The document and dialog button input regressions pass (0.08 and 0.14 seconds),
preserving one guest press/release pair and existing activation semantics.
Three actual shared-compositor captures and paired guest frames were reviewed;
`text-classic-buttons-review.json` records the commands, hashes and limits.
Open TEXT and Standard File/New Folder actions show centered, clipped guest
labels with primary and secondary styling. This one viewport does not qualify
all scales, active/inactive, pressed/disabled states or authentic Apple fonts.
The remaining text surfaces and production gates are still incomplete.

### Guest glyphs in choice labels

Checkbox and radio wrappers now share left-aligned guest system-font glyph
painting, using the control height for the integer guest baseline before
scaling. Host shaping and wrapping no longer determine these labels. The
existing indicator, tracking, accessibility and guest activation routes remain.
Checkbox and dialog pointer regressions pass (0.07 and 0.15 seconds). Three
actual shared-compositor Controls captures show guest checkbox label ink;
`text-classic-choices-review.json` records the images and limits. The page
contains no radio, so radio visuals remain unqualified. Small-height indicator
geometry, font overrides and the scale/activation/state matrix remain open.

### Guest wrapping in Standard File prompts

Save and New Folder prompts, including subsidiary error messages, now paint
guest system-font glyphs through the shared GPUI canvas. They use the same
`wrap_classic_text` primitive as both guest drawing paths, preserving word
breaks, hard line endings and hidden trailing whitespace with guest advances.
Standard File's baseline 12 and line step 16 remain explicit; parent bounds
clip the canvas. Host typography no longer determines prompt wrapping.

The create/cancel/stale-action regression passes across all three guest modes
(137.64 seconds). The desktop type-check passed and the shared capture harness
builds. Three final duplicate-name error captures and paired guest frames were
reviewed: the complete message wraps into two guest-width lines, while the Save
prompt remains occluded by the child where their guest bounds overlap.
`text-classic-prompts-review.json` records commands, hashes and limitations.
One viewport and one error do not qualify all scales, states or errors. General
dialog fonts/styles, menus, list labels and Save-field editing remain open.

### Guest glyphs in Standard File lists

Open/Save row labels now use guest system-font Unicode glyph resolution.
Snapshots retain CPU-specific row text origins and directory markers: 68k
uses inset 4/baseline 11 relative to the row and the triangle, PPC uses
inset 3/baseline 13 and `>`. The list border is decorative so it cannot
inset row coordinates. Unicode and raw Mac Roman glyph parity is tested.
The Standard File GPUI action regression passes (2.94 seconds).

Three Save composed captures and paired guest frames were reviewed and saved
in `text-classic-file-list-review.json`. Rows are clipped inside the list;
PPC's marker is visible. The current resolved 68k strike supplies no triangle
ink, so the painter retains the guest missing-glyph behavior rather than
substituting a host font. Authentic Apple font fidelity, exact panel pixel
geometry, Open captures, selected/scrolled rows and scale/state qualification
remain open, along with the other unfinished text surfaces.


### Open volume text in the guest glyph canvas

The Open popup no longer invents a host-font four-character abbreviation. Its
snapshot carries the text and origin produced by the actual CPU painter. The
68k adapter reuses the popup's guest-font width truncation and centered baseline
with a 15-pixel inset; PPC preserves the literal `Maci...` at origin (0,12). GPUI
paints font 0/12 guest glyphs through the shared canvas. The decorative border
does not inset text coordinates. Adjacent directory labels remain unfinished.

The modal snapshot regression passes in monochrome 68k, colour 68k and PPC with
pointer and semantic routes (7.50 seconds), including volume text and width
checks. Default binary type-check and capture build pass. All three actual Open
compositor captures are reviewed and retained with paired guest frames and
hashes in `text-classic-open-volume-review.json`. This is one viewport/scale
and modal state; exact geometry, other scales and activation states, Save
volume UI and authentic Macintosh font qualification remain open.


### Standard File directory typography

Open/Save directory labels now use guest glyphs and wrapping rather than host
typography, centering and ellipsis. The 68k snapshot exposes QuickDraw font, raw
size and face, font ascent/line spacing and the one-pixel statText inset; PPC
retains system text at baseline 12 with spacing 16. 68k ParamText expansion and
lossy Mac Roman conversion follow the guest painter. Styled faces and scaled
substitute strikes retain the guest panel pending faithful implementation.

Six actual shared-compositor Open/Save captures are reviewed and retained with
paired guest frames in `text-classic-directory-review.json`. The 68k fixture's
smaller guest font and PPC system font remain distinct. The three-mode modal
snapshot regression checks font/layout metadata (9.90 seconds). GPUI pointer
routing and render-level fallback checks for bold, italic and a 97-point scaled
strike pass (0.62 seconds). Exact generated 13-point strikes remain eligible.
These checks cover one viewport/scale and modal state. Long-directory wrapping,
font mutation versus paint-time state, activation, exact geometry and authentic
Macintosh font fidelity remain unqualified; the Save filename editor still uses
host typography.


### Save filename Mac Roman editing prerequisite

The 68k retained Save buffer now treats selection endpoints as Mac Roman byte
offsets, matching TextEdit and the snapshot. Initial selection and Command-A
measure encoded bytes; insertion and backspace edit the encoded buffer before
decoding its display string. Both CPU handlers now accept high Mac Roman
characters while retaining filename separator and control-character guards.
The 63-byte 68k filename limit is unchanged.

The guest Save/replacement regression passes across monochrome 68k, colour 68k
and PPC (34.02 seconds). New-file cases type `é£S`, delete twice to `é`, check
selection (1,1), reinsert the suffix and inspect the returned FSSpec's encoded
name bytes. Existing replacement cancellation/confirmation paths remain covered.
This is guest editing evidence; the Save editor still uses host typography and
needs glyph-aligned pointer selection, caret, scrolling and interaction work.


### Save filename guest click placement

Both retained Save handlers now place an insertion point when the name field
receives a guest mouseDown. The shared nearest-boundary calculation uses Mac
Roman prefix advances; 68k measures its active QuickDraw font with the editText
one-pixel inset, and PPC measures system text from the field's left edge. The
same calculation serves New Folder without changing its scrolling policy.

The three-mode Save/replacement regression passes (41.65 seconds). It clicks
every insertion boundary in `é£S`, asserts guest selection offsets, and checks
the final encoded FSSpec reply. This does not qualify held or Shift selection,
caret blinking, horizontal scrolling, GPUI glyph pointer mapping or field
painting; the Save field still uses host typography.


### Save filename held and Shift selection

Both CPU handlers retain a guest selection anchor after name-field mouseDown,
track the moving Mac Roman insertion boundary while the button is held, and
consume release before returning to the panel event loop. Shift extends from
the existing selection boundary. Tracking continues outside the field and ends
on mouseUp or loss of the held-button state. Stationary tracking redraws only
on selection changes/release (or disturbed 68k guest pixels).

The final three-mode Save/replacement regression passes (37.87 seconds), covering
Shift extension, held movement, movement outside the field, release stopping
further selection changes, accented editing and encoded FSSpec replies. This
is guest-event evidence, not qualification of native pointer capture or GPUI
glyph mapping. Save field painting, caret blinking, horizontal scrolling,
keyboard navigation/clipboard and the scale/activation matrix remain unfinished.


### PPC Save partial-selection geometry

PPC Save no longer highlights the whole filename field for every nonempty
selection. Its painter computes the selected Mac Roman prefix bounds, clips
them to the field and uses themed selection or classic QuickDraw inversion
after drawing the text. List focus and subsidiary dialogs suppress selection.

The focused maximum-length/name-selection regression passes (0.02 seconds),
including an accented partial range and suppression for list, replacement and
New Folder focus. The headless type-check passes. This is geometry and build
evidence; new pixel captures, native parity, GPUI field glyph painting, caret
blink and horizontal scrolling remain pending.


### Save filename GPUI guest glyph painting

Save snapshots now expose the filename font, baseline/inset, selection height,
edge-extension rule and wrapping rule. GPUI paints the same resolved Font
Manager binary glyph ink and Mac Roman advances used by the guest. 68k retains
its inherited font and selection to the field edge; PPC retains system-font
geometry and the selected glyph range. Decorative borders do not inset text.
Styled or non-exact resolved strikes retain the guest panel rather than silently
changing metrics. Pointer/edit handling continues through existing guest paths.

The shared compositor Standard File action test passes; its extension checks
filename style/scaled-strike fallback. Six initial/edited composed captures and
paired guest images are recorded in `save-name-glyph-review.json`. These are
Systemless guest/compositor evidence, not native Macintosh font qualification.
The static frontend insertion feedback remains interim: caret blinking and
exact caret height, horizontal scrolling, paint-time font mutation, native
pointer capture, clipboard/keyboard navigation and the scale/activation matrix
remain unfinished. Other earlier sections describe evidence at their respective
implementation stages; this entry supersedes their host-typography limitation
for supported plain Save filename strikes only.


### Save insertion caret driven by guest CaretTime

Both retained Pack3 Save gateways own a shared guest-tick/CaretTime blink
state. Ordinary field input resets visibility; idle toggles only an empty
selection without a held selection gesture or subsidiary modal UI. PPC list
focus also suppresses it. Snapshots supply visibility to GPUI and both guest
painters use the same state. Carets clip at the field boundary and retain the
guest top+2 through bottom-1 geometry, including 68k fields with smaller fonts.

The final three-mode Save/edit/replacement regression passes (116.90s), covering
on/off transitions preserving accented text and byte selection, held/Shift
selection suppression, editing and encoded replies. The wrapping-clock timing
unit test passes (0.00s). Headless/JIT checking and the capture build pass.
Six phase captures and paired guest frames are recorded with hashes and whole
frame difference bounds in `save-caret-review.json`: every pair differs only
in a 1×17 guest caret or its 2×39 composed raster footprint. No text or scene
geometry changes occur between phases.

Reproduce the visible phase with `--capture-standard-file-save-edited-composed`
and the hidden phase with `--capture-standard-file-save-caret-hidden-composed`
after building the example with `gpui-demo-test`. Use `--screen-depth 1`,
`--screen-depth 8` or `--prefer-powerpc` for the three modes. Both capture cases
wait for the requested guest visibility without changing the insertion point.

Changed CaretTime preference integration, scale/activation coverage,
theme-provider caret/focus fidelity and native-oracle qualification remain
pending. Horizontal scrolling and broader Save keyboard/clipboard coverage
remain unfinished. This supersedes the interim static insertion-feedback
limitation in the preceding Save glyph entry for these supported fields.


### Plain dialog statText guest glyph presentation

Dialog item snapshots carry a resolved static-text font, inset, baseline,
line spacing and bottom-inclusion rule. GPUI uses shared guest Mac Roman glyph
ink and wrapping for supported plain exact strikes. 68k retains inherited
metrics, the one-pixel inset and short-field baseline clamp; PPC retains its
current guest painter's system font, inset, baseline 12 and line spacing 16.
Size zero is the system-size sentinel and resolves to the 12-point strike.
68k snapshots expand ParamText as the guest statText painter does.

Styled, colour-table or non-exact 68k strikes currently retain guest item
pixels. The overlay partition excludes items without supported layout, while
other standard items continue through their existing ownership/clipping path.
The three-mode dialog geometry/identity test passes (4.81s), checking statText
metadata, and the inactive/occlusion partition test passes (0.01s), including
missing-layout fallback. The capture build and headless/JIT check pass. Three
About-alert composed captures and paired guest frames are recorded in
`dialog-static-glyph-review.json` and were inspected for the three explicit
text lines. These establish this fixture presentation, not broad wrapping,
bottom clipping, styled text, ParamText integration, font mutation,
scale/activation coverage or native Macintosh font fidelity. Those requirements
remain open, along with general dialog editing and menu typography.

Styled-glyph groundwork now shares QuickDraw's binary base mask with the guest
framebuffer painter and exposes a Macintosh Roman styled-ink/advance helper for
GPUI. It resolves intrinsic italics before synthetic shear and preserves guest
bold, outline, shadow, condensed and extended advances. Underlines remain a
line-level operation. A focused GPUI test compares plain helper ink and advances
against the existing binary span painter for Chicago and Geneva strikes and Mac
Roman bytes (passes, 0.07 seconds); default and JIT/headless checks also pass.
Styled field replacement, complete style-mask parity, scale behavior and CPU
capture evidence are still unfinished. This groundwork does not enable styled
text replacement or establish native Macintosh font fidelity.

The styled-ink follow-up now uses the same complete binary mask synthesis in
QuickDraw and GPUI statText, including intrinsic/synthetic italics, bold, hollow
outline/shadow, condensed/extended advances and full-line underlining. Exact
68k statText strikes with face values can be replaced; item colour overrides and
missing exact strikes keep guest pixels. PPC statText still reports its guest's
fixed plain face. Wrapping preserves the guest's separate current-port advance
adjustment rather than inferring measurement from the item's drawing face.
The shared helper matches framebuffer pixels and advances for all 128 face
combinations over Chicago 12 and Geneva 9/12, including Mac Roman (1.66 seconds).
GPUI span/underline coverage passes all faces (0.17 seconds), and guest clipping
passes (0.19 seconds). These tests establish current guest-renderer parity;
style-specific composed captures, paint-time font mutation, CPU/scale/activation
coverage, styled TextEdit, colour styles and native font fidelity remain open.
Default and JIT/headless checks pass, as does the existing monochrome 68k,
colour 68k and PPC dialog identity/layout regression (4.50 seconds).

Styled TextEdit snapshots now include canonical STElement font, face, point
size, RGB colour, height/ascent and byte-indexed style runs, plus each guest
LHElement line height/ascent. Both CPU paths read these tables through the
same snapshot decoder. Missing/inconsistent tables, non-monotonic runs,
out-of-range style indices, excessive counts and overflowed table pointers
produce absent style metadata while retaining the text snapshot and guest
rendering. The mixed-run GPUI replacement guard remains closed until its
renderer, selections/caret and hit geometry use these values faithfully.
The synthetic decoder test passes (0.01 seconds). The real Styled Text & Fonts
fixture verifies bold blue Geneva 12, italic green Monaco 14 and underline
runs and line metrics in monochrome 68k, colour 68k and PPC (2.69 seconds).
This proves state extraction, not mixed-run GPUI rendering or native font
fidelity. Default and JIT/headless checks pass.

Plain GPUI TextEdit now applies the shared 68k/PPC one-pixel destRect inset to
its glyphs and selection end. A selection beginning at line offset zero still
includes the left inset; the caret starts at the inset and backs up one pixel
after nonzero byte offsets, matching both guest draw paths. WDEF title geometry
is separate and retains its own pen origin. The existing three-mode TextEdit
regression now compares corrected GPUI binary spans and caret against actual
guest first-line pixels before checking clicks at the displayed insertion
boundary, typing, selection replacement and Mac Roman edits (139.02 seconds,
passes). The default build passes. This covers the tested plain line/caret;
composed scale/state recaptures, newline/soft-wrap caret ownership, theme caret
width and mixed-run TextEdit rendering remain open. Earlier composed text
captures precede this inset correction and do not qualify the current matrix.

Plain TextEdit caret ownership now follows canonical byte spans independently
of visible ink: both guest draw paths choose the first matching line at an
inclusive wrap boundary. 68k selects among visible lines and retains its last
visible-line fallback; PPC clamps caret and selection offsets to the trimmed
visible end. GPUI paints no CR/LF/trailing-space glyphs, but retains their byte
advances where the guest measures them. Caret offset is independent of the
highlight range and has one owner per record. Focused coverage includes CR,
empty lines, scrolling and inactive state (0.05 seconds). The three-mode
fixture reaches the first wrap boundary through actual guest Right-arrow
TEKey events and compares caret/line pixels, then exercises typing, selection
replacement and Mac Roman editing (157.40 seconds, passes). Default and
JIT/headless checks pass. Broader CR/scroll pixel checks, composed scale/state
recaptures, theme caret width, mixed-run presentation and native qualification
remain unfinished.

The corrected plain TextEdit compositor now has a refreshed 36-capture matrix:
monochrome 68k, colour 68k and PPC at 0.75x, 1x, 1.5x and 2x, with active
selection, window inactivity beneath a front dialog, and unobscured guest
session suspension. All field crops were reviewed in the three
[text-scale contact sheets](tests/toolbox-showcase/reference/gpui-demo/text-scale-mono-contact.png)
([colour](tests/toolbox-showcase/reference/gpui-demo/text-scale-colour-contact.png),
[PPC](tests/toolbox-showcase/reference/gpui-demo/text-scale-ppc-contact.png)).
Wrapping/inset remain stable, selection tracks the displayed glyphs, front
occlusion clips the field, and inactive/suspended selection is hidden. Numeric
checks confirm requested image dimensions and selection bounds within one guest
pixel across scales. The [review manifest](tests/toolbox-showcase/reference/gpui-demo/text-scale-review.json)
records fixture/renderer hashes, paired guest frames, pixel-preserving PNG
encoding, scope and gaps. A fresh helper run matches all 72 original composed
and guest PNG hashes; this is determinism evidence, not a native oracle.

The hidden test-only `--capture-scale` option sizes fixture-scene captures in
guest coordinates, using the same `Demo::render` path as live presentation.
Reproduce with a new empty output directory:

```sh
cargo build --locked --example gpui-menu-demo --features gpui-demo-test
python3 tests/toolbox-showcase/capture-gpui-text-scales.py /tmp/new-empty-output
```

The ordinary default check, example/test build and helper execution pass. This matrix does not qualify
native host observers, native Macintosh font fidelity, scaled pointer/editing
interactions, all caret phases, mixed-run TextEdit, or general dialog editing.
These captures precede the dialog editText guest-glyph replacement below and
do not qualify its new field rendering.

Scaled plain TextEdit insertion clicks are now checked at the painted glyph
boundary, including the one-pixel destRect inset. The GPUI test
`classic_document_glyph_clicks_reach_guest_at_scene_scales` dispatches real
frontend mouse-down/up events through the existing guest input path and asserts
the resulting guest insertion offset on two lines at 0.75x, 1x, 1.5x and 2x,
across monochrome 68k, colour 68k and PPC (24 checks; passed in 254.37 seconds).
The same test now also holds the mouse button while moving from byte offset 3
to 6, then releases it and asserts the guest selection `(3, 6)` at each scale
and in each mode. All 36 checks (24 clicks and 12 drags) pass in 344.11 seconds.
Each frontend event is delivered through the existing session input path;
selection is read from the guest TERec, without a host editing model. This
qualifies the tested insertion and forward-drag mapping; reverse/cross-line
dragging, Shift-selection, scaled editing, scrolling and native host input
remain open.

Ordinary dialog editText fields now use GPUI canvas guest bitmap glyphs instead
of host typography. `DialogEditTextLayout` records the guest font, baseline,
line height and painter geometry: 68k uses current port metrics and extends an
end-of-text selection to the field edge; PPC active fields use validated TERec
geometry, trimmed selection offsets and the caret backstep, while inactive
fields use the fixed wrapped Dialog Manager painter. Field chrome remains
outside the guest display rectangle. Pointer/key delivery stays on the existing
guest event path. Unsupported or absent layout metadata retains guest pixels;
there is no host-font substitution for those fields.

The modeless lifecycle/editing test passes in all three modes (4.94 seconds),
including guest geometry assertions. GPUI field/checkbox click forwarding passes
(0.12 seconds), and explicit absent-layout clipping/fallback coverage passes.
Default, example and JIT/headless builds pass. Six actual shared-compositor
captures (modeless and modal visible-caret scenes across the three modes), with
paired guest frames, were reviewed in the
[dialog field contact sheet](tests/toolbox-showcase/reference/gpui-demo/dialog-edit-font-contact.png).
The [review manifest](tests/toolbox-showcase/reference/gpui-demo/dialog-edit-font-review.json)
records source/fixture hashes, commands, scope and pixel-preserving archive
encoding. Tested glyph forms, baseline and caret placement are consistent with
the paired guest frames. This does not establish native Macintosh font fidelity,
the dialog field scale/state matrix, font mutation, multiline/styled/scrolled
fields, or exhaustive selection/editing behavior; those remain unfinished.

DialogSelect editText mouse handling now shares the retained TEClick selection
machinery on both CPU paths. 68k previously activated the field without placing
or dragging its insertion point; PPC placed it once without retaining mouse
ownership. Both now track held movement and Shift extension until release, then
return to guest editing. The classic retained trap preserves its Pascal stack
until release and synchronizes the DITL selection; it avoids resetting the
shared TERec or repainting an unchanged field on every tracking poll.

The shared GPUI dialog-field geometry now distinguishes a collapsed guest
selection from a range collapsed only by PPC trailing-space trimming. A selected
trailing space cannot acquire a spurious caret. The focused regression passes
(0.02 seconds). The three-mode fixture uses the GUI scheduler and actual guest
mouse/modifier events to verify full, partial, reverse and Shift-extended ranges,
compares the renderer's glyph/highlight geometry with every guest field pixel,
and verifies typing after release (12 selection checks; 20.65 seconds, passes).
Five existing classic TEClick tests pass (0.03 seconds), 12 DialogSelect tests
pass (0.04 seconds), and the PPC public-record editing test passes (0.04 seconds).
All six focused text tests pass (0.25 seconds), as does the three-mode modeless
lifecycle regression (6.38 seconds). Default and JIT/headless checks pass.
This qualifies the tested modeless field's guest behavior and geometry. It does
not establish composed selected-field scale/state coverage, switching among
multiple edit fields, ClikLoop callbacks, native Macintosh fidelity or the
remaining styled/multiline/scrolled field requirements.

Work in progress: classic DialogSelect field activation now moves the shared
TERec destination, view, selection rectangle and owner port to the selected
DITL item before calculating text and interpreting clicks. A collapsed caret
is repainted after the Dialog Manager item redraw. The two-field mouse/null
regression checks the second field's rectangles and owner and now expects the
clicked insertion point `(3, 3)`, rather than the previous stored selection.
It passes under both themes (0.08 seconds). All 12 `dialog_select` tests pass
(0.04 seconds), including the caret blink regression corrected to read the
dialog bitmap's actual row stride. On switching fields, classic TextEdit also
deactivates and erases its previous highlight/caret before moving the record.
PPC DialogSelect now uses the existing SelectDialogItemText field-switch helper,
which detaches the borrowed DITL text handle before replacing the TERec. A native
import regression switches second/first/second and checks active field, borrowed
handle, text, destination and view rectangles (passes, 0.08 seconds). The
three-mode real-fixture selection/pixel/editing regression passes (21.09 seconds)
with the geometry and caret changes; it exercises one field, so real-fixture
multi-field coverage and composed field-switch captures remain unfinished.

The new real-fixture modal two-field regression exposed an additional classic
ModalDialog gap: returning to a field restored its stored insertion point
instead of resolving the new click. The fix uses TERec guest metrics
to place and persist the clicked insertion point. The regression switches
nickname/name/nickname, asserts insertion at the displayed left edge, checks
renderable active-field layout and types in each field without changing the
other field across all three modes. It also Shift-clicks at glyph offset 3
from insertion offset 1 and asserts selection `(1, 3)` in each mode (passes,
14.00 seconds). Held modal dragging and composed multi-field captures still
need verification.

All three classic modal theme-parity regressions pass (0.10 seconds), including
the corrected clicked insertion `(3, 3)` expectation; default/cancel and update
event behavior remain covered.

Work in progress: the real modal regression now includes a held glyph drag
from offset 1 to 4. It initially failed on monochrome 68k with `(1, 1)`.
Classic ModalDialog now retains its call while shared TEClick tracking owns
the mouse, updates the active DITL selection and writes it back on release.
That intermediate regression passed the drag assertion in monochrome
and colour 68k, then failed on PPC with `(1, 1)` instead of `(1, 4)` (14.43
seconds); the PPC failure is resolved below.

PPC ModalDialog now resumes the shared native TEClick tracker before consuming
another filtered dialog event. The complete three-mode regression passes
(11.81 seconds): switching, independent text, Shift extension, held glyph drag
`(1, 4)`, then typing to replace the released range while preserving the other
field. All 13 `modaldialog` regressions pass (0.05 seconds), including an
explicit held-stack/no-item-result assertion followed by mouse release. The
classic release path now saves the current painted selection pixels rather
than reusing pre-drag rendered pixels. Classic click selection is applied
once through the shared TEClick helper; the final real-fixture check passes
(11.25 seconds). Reverse Shift-click now extends `(1, 3)` back to `(0, 3)`
in all three modes; the expanded fixture regression passes (12.26 seconds).
All 13 modal regressions also pass on the final implementation (0.04 seconds).
Composed selection snapshots and broader callback/lifecycle qualification
remain open.

The existing nested-modal focus/editing regression also passes on both CPU
paths (3.46 seconds), including dismissal and resumed modeless editing.

Work in progress: hidden `--capture-modal-dialog-selection` drives a real
nickname drag from offsets 3 to 6, asserts the guest range and saves shared
GPUI composition plus its paired guest frame. The example builds successfully.
The first monochrome capture was reviewed: the GPUI highlight covers the same
selected nickname glyph span as the guest. The guest frame also shows a stale
caret in the previously active name field, which needs investigation before
claiming complete focus/selection presentation. Colour/PPC, scale/state captures
and archived review evidence remain pending.

The stale caret came from retained modal chrome restoring an older painted
snapshot during field activation/tracking. Classic tracking now snapshots the
newly active field and refreshes pixels when the dragged range changes, leaving
unchanged polls alone. A fresh monochrome paired capture was reviewed: the old
name-field caret is absent and nickname offsets 3--6 remain highlighted. The
example build passes; modal regressions are running for this revision. The
remaining CPU/scale/state capture matrix and archived evidence remain pending.

All 13 modal regressions pass on the snapshot fix (0.05 seconds). Final
monochrome, colour 68k and PPC compositions and paired guest frames were
reviewed and archived with lossless pixel-preserving encoding. The
[review manifest](tests/toolbox-showcase/reference/gpui-demo/dialog-modal-selection-review.json)
records source/fixture and file/decoded-pixel hashes, commands and scope.
The selected nickname glyph span is consistent with each paired guest frame,
and the previous name-field caret is absent. This qualifies the captured
active selection at the historical viewport; the explicit scale/inactive
matrix and native-oracle font/input qualification remain open.


The reproducible scale harness now accepts `--surface modal`, preserving the
existing document capture default. All 12 active modal selection captures
complete across monochrome 68k, colour 68k and PPC at 0.75, 1, 1.5 and 2.
The harness verifies scene dimensions, aspect ratio and consistent host density;
each capture asserts guest nickname offsets 3--6. All twelve field crops were
visually reviewed: the selected glyph span and absence of the previous name
caret are consistent. Paired frames, a contact sheet and the scoped
`dialog-modal-selection-scale-review.json` are archived with pixel/file hashes.
Only mono 0.75 received full-screen visual review in this matrix. Inactive/modal
host transitions, full-scene review at remaining scales, native pointer mapping
and native Macintosh font fidelity remain open.


Work in progress: `--capture-modal-dialog-selection-inactive` selects nickname
offsets 3--6, then switches focus to the name field through guest pointer
events. The example builds. Monochrome and PPC paired captures pass and were
visually reviewed: the nickname highlight disappears and the name field owns
the caret. PPC snapshots intentionally expose selection only for the active
DialogRecord TERec; classic snapshots retain the inactive item range. The
assertions reflect these guest snapshot contracts without synthesizing a PPC
inactive selection. The scale harness now includes this inactive case; colour,
scale captures and archived review evidence remain pending for this revision.


All 12 inactive modal field captures now complete across the three CPU/display
modes and four explicit scales. Geometry/density checks pass. Every composed
field crop was reviewed: nickname highlight/caret is absent, name caret is at
offset zero and text remains unchanged. The paired frames and contact sheet
are losslessly archived with scoped source/file/pixel hashes in
`dialog-modal-selection-scale-inactive-review.json`. The harness supports
`--state inactive` for a focused reproduction. This verifies a field focus
change, not host suspension of the modal application; native font/input, wider
scene and paired guest pixel comparison at every scale remain open.


Menu typography work in progress: shared `GuestMenuItem` snapshots now carry
the live QuickDraw style byte. The common menu-list projection regression
verifies style preservation for regular and hierarchical entries (0.01 s).
GPUI item-name labels now paint through shared styled QuickDraw glyph spans
with ink-overhang bounds rather than host text shaping. The example check and
build pass. A fresh monochrome open-menu composition was captured for review.
Shortcut/mark/submenu symbols, menu-bar titles, scaled layout, guest-equivalent
row geometry and style/interaction/native font qualification remain open; this
work does not qualify complete menus.


Menu item-name verification: the live-update keyboard-selection test passes
(0.18 s), and the three-mode guest bridge page/nested-check regression passes
(6.05 s). All three composed open Pages menus were visually reviewed and
losslessly archived in `menu-guest-label-review.json` with source/fixture and
file/pixel hashes. Item names fit this menu; styled rows, disabled/selected
presentation, scaled menus and native font fidelity are not qualified by these
plain-item captures. The remaining title/symbol/geometry gaps remain open.


Menu-bar titles now use the shared guest glyph canvas, with GPUI Kit custom
button children and explicit accessibility labels preserving menu identities.
Visible-bar labels use the scene scale; hidden-menu reveal uses its existing
host bar scale. The example builds, live-update keyboard selection passes
(0.28 s), and the open-menu live snapshot regression passes (4.11 s). Three
title-bar crops were reviewed and archived in `menu-guest-title-review.json`:
titles fit at the historical viewport and Pages opens beneath its control.
Explicit scales/states, full-scene review of this revision, symbols, guest row
geometry and native Macintosh font/input qualification remain open.


Menu symbols work in progress: snapshots retain the raw Menu Manager mark
byte, including hierarchical menu IDs; projection assertions now cover the
standard checkmark and submenu marks. GPUI marks and Command shortcuts use the
same Unicode-to-guest-glyph resolver as the guest painters. The hierarchy
triangle uses an exported presentation wrapper around the existing shared
Menu Manager raster rather than a host chevron. No final build/test or capture
result is established for this revision yet; verification is running. Guest
row/column geometry, command display case, scale/state and native font/input
qualification remain open.


Symbol revision verification: shared projection passes (0.01 s), the example
check and build pass, and live-update keyboard selection passes (0.36 s). All
three captures completed; reviewed checkmark/name crops show the guest glyph
beside Graphics. Losslessly archived compositions/crops and source/file/pixel
hashes are in `menu-guest-symbol-review.json`. These captures contain neither
Command shortcuts nor submenu triangles; their visual evidence, arbitrary
marks, exact columns, command case, scale/state and native fidelity remain open.

The hierarchical keyboard-selection interaction regression also passes (0.36 s);
it establishes event routing, not native triangle pixel parity.


Work in progress: hidden `--capture-standard-menu-id` selects a real visible
guest menu (default remains Pages/129). File/131 captures complete on all three
modes; symbol crops were reviewed and show Command shortcuts and the shared
triangle. They exposed inline-shortcut column misalignment. Rows now reserve
a shared name width measured from styled guest advances/ink, aligning shortcut
and hierarchy indicators. The example builds and keyboard live-update test
passes (0.19 s). A fresh monochrome full composition was reviewed after the fix.
Colour/PPC recaptures and archived final evidence remain pending; exact guest
menu rectangle/row geometry, command case and native fidelity remain open.


Final colour 68k and PPC File-menu recaptures complete and were reviewed,
matching the corrected monochrome composition: Command shortcuts and hierarchy
indicator use the shared label column. All three final compositions are archived
losslessly with source/fixture/file/pixel hashes in `menu-file-column-review.json`.
The reproduction chooses menu ID 131. The prior build and keyboard-selection
regression cover this unchanged source. Exact guest geometry, command display
case, arbitrary marks/styled rows and scale/state/native fidelity remain open.

### Menu command character fidelity

The shared menu projection preserves the guest command character's case and
Mac Roman identity. GPUI menu rows and their accessibility labels no longer
uppercase the projected character. Shortcut matching continues through the
existing guest Menu Manager and event paths; presentation does not normalize
the command. The exhaustive projection regression covers all 256 bytes in
regular and hierarchical menu partitions (0.01 seconds). The GPUI text
regression compares the ink and advance of every displayable command byte
against the raw guest strike (0.01 seconds). Live guest-memory mutation and
inserted-menu snapshot regressions pass (0.02 seconds each), and GPUI live
keyboard selection remains intact (0.20 seconds). These checks establish
shared metadata and glyph fidelity, not native font-oracle parity or composed
visual qualification of unusual command characters. Exact menu geometry,
scales, states and remaining system text surfaces still require qualification.

### Guest-tracked popup row typography

Standard popup row names now use binary guest glyph canvases with the owner
GrafPort font/size and live QuickDraw style. Marks and Command characters use
the same guest font, with hierarchy pixels from the shared Menu Manager raster.
The snapshot exposes text/baseline/mark/command anchors from the same standard
MDEF layout used by both CPU painters. Rows retain guest bounds, height, scrolling
origin and clipping; text is no longer shaped or ellipsized by host typography.
Input continues to bubble to the existing guest tracker. The updated example
type-check passes. Visual captures, icon-bearing row context, colour/disabled
pattern fidelity and scroll-arrow glyph replacement remain unfinished; this
change does not qualify popup parity or native font fidelity.

The eight popup regressions pass (214.15 seconds), covering guest-linked menu
state, cancellation, scrolling in both directions, release and actual GPUI
pointer routing across monochrome 68k, colour 68k and PPC. All three full
open-popup compositions were reviewed and archived losslessly in
`popup-guest-strike-review.json` with source, fixture, file and pixel hashes.
The selected long label is readable and the disabled row label remains present.
This fixture does not qualify icons, marks/commands, alternate owner fonts,
scroll arrows, further scales or native colour/disabled-pattern parity.

### Popup scrolling indicator raster

GPUI popup arrow slots now paint the shared Menu Manager up/down pixel rasters
instead of host-font triangle characters. Each raster pixel uses the scene scale;
centering follows integer guest coordinates within the unchanged 16-pixel slot.
The updated example/test build and guest-content-origin indicator regression
pass. The existing shared-raster/gray-phase regression also passes (0.02 seconds).
The prior popup interaction matrix remains evidence for unchanged tracking,
not composed verification of this new raster. Scrolled compositions across
CPUs/scales and native colour/pattern fidelity remain unqualified.

The updated binary build and three scrolled-popup captures complete. Full
monochrome 68k, colour 68k and PPC compositions were reviewed and archived
losslessly with hashes in `popup-guest-raster-scrolled-review.json`. Capture
assertions verify Geneva 9 owner font, 12-pixel rows, 140-pixel width and final
item visibility after held down-arrow tracking. The compositions show the
up-arrow raster and selected final row. The partially clipped top label needs
closer comparison; native parity and additional scales/states remain open.

Follow-up clipping inspection: both guest painters reserve a 16-pixel upper
scroll slot and preserve original baselines when clipping partially exposed
rows. The PPC guest-frame crop also contains the partial first label. Retain
this clipping rather than shifting its baseline. This internal renderer
comparison supports the observed clipping policy; it is not native-oracle
evidence or complete pixel parity. All archived composition hashes revalidate.

### Closed popup label guest font migration

Closed popup control titles and selected-item labels now use binary guest
glyph canvases with the snapshot owner font/size. Host shaping and ellipsis
are removed. The canvas baseline uses that font’s ascent/descent rather than
fixed Chicago metrics. The example test build and Unicode/raw guest strike
resolver regression pass (0.09 seconds). Control padding, exact CDEF baseline
and border/arrow geometry, composed captures, alternate font/style cases and
CPU/scale/state interaction qualification remain unfinished.

The capture binary build and all three selected closed-popup captures pass.
Full compositions were reviewed and archived losslessly with hashes in
`popup-closed-guest-strike-review.json`: 12-point Loadout and 9-point Theme
labels use guest glyphs, and the long selected item clips in its allocated
control. The live-selected-item ownership regression passes. Exact CDEF
geometry and native/scale/state fidelity remain open.

### Closed popup title alignment

Both guest CDEF paths place the title right-aligned six guest pixels before
the selection box. GPUI now derives that origin from the painted guest advance
and reserved title width, clamping the origin at the left edge for long labels.
The updated example type-check passes. Corrected composed captures and exact
CDEF geometry/long-title display policy remain to be verified; the prior
closed-popup archive predates this alignment correction.

The corrected binary builds and all three recaptures complete. Full
compositions are reviewed and archived losslessly with hashes in
`popup-title-aligned-review.json`. Titles align before the selection boxes
while the long selected label retains clipping. These captures do not
qualify long-title policy, exact CDEF geometry, scales/states or native parity.

### Shared popup selected-label truncation work

The classic and PPC CDEFs now call one shared truncation loop with their
existing guest measurement adapters. GPUI selected labels use that loop with
painted guest advances and three periods, replacing hard clipping of the full
label. Host padding is removed and the guest 15-pixel text inset is used inside
the allocated content pane. Type-check and capture binary build pass. Boundary
and guest regressions plus three composed recaptures are still running; exact
CDEF border/content geometry, scale/state and native fidelity remain open.

All three recaptures complete and full compositions are reviewed: the long
selected label ends in three guest periods and title alignment is retained.
Lossless captures with source/fixture/file/pixel hashes are archived in
`popup-guest-ellipsis-review.json`. The regression build remains live; these
images do not qualify exact geometry, other scales/states or native parity.

The shared boundary regression passes (0.02 seconds), covering suffix-only,
insufficient-space, exact-fit and accented character boundaries. PPC
fixed-width truncation passes (0.02 seconds), as does the classic CDEF
text/arrow clipping regression (0.08 seconds). Capture/source hashes
revalidate. This closes the running-build checkpoint; exact control geometry,
other scales/states and native parity remain unqualified.

### CPU-specific popup selected text inset

Guest CDEF inspection establishes distinct selected-label origins: classic
68k uses box-left plus 15 pixels; PPC uses plus five. Control snapshots now
retain this inset and GPUI uses it for both glyph placement and truncation
budget. The updated example type-check passes. The existing three-mode popup
metadata regression now asserts each inset; it is running. Corrected PPC
compositions, exact box/baseline geometry and scale/state qualification remain
open. The preceding ellipsis captures predate this PPC correction.

The three-mode popup metadata/selection regression passes (59.50 seconds),
including each CPU-specific inset. The corrected capture binary builds; its
full PPC selected-control composition is reviewed and losslessly archived
with hashes in `popup-ppc-text-inset-review.json`. The label uses the smaller
PPC inset and retains guest ellipsis. Exact geometry, scales/states and native
parity remain unqualified.

### Closed popup text coordinates independent of host borders

The closed popup title and selected-label canvases now use absolute guest
coordinates rather than the host border's flex content box. The selected text
area ends 19 guest pixels before the control right edge, and its baseline is
one guest pixel above the centered title baseline, following both CPU CDEF
paths. The existing CPU-specific 15/5-pixel text origins remain intact.
The GPUI example passes its locked build check and the diff whitespace check.
The rebuilt PPC composed capture passed and was visually reviewed and archived
with source hashes in `popup-coordinate-ppc-review.json`. Initial sandboxed
captures failed at host service access; the successful compositor run used
macOS host access. Both 68k compositions also passed and were visually reviewed and archived.
Scale/state interaction verification remains pending; the host arrow and themed border still need faithful replacement.

### Popup scale qualification in progress

The actual GPUI pointer-to-guest popup regression now exercises 0.75, 1, 1.5
and 2 times scene scales on all three CPU/display modes. Explicit mock window
bounds and a scene image force real scaling; each event asserts the rendered
scale before checking guest press, held movement, release, highlighted row and
final control value. The regression passes all twelve CPU/scale combinations (235.83 seconds).
This verifies the guest event path at those scene scales, not native pointer
capture or exact guest text/chrome pixel parity.

The reusable capture script accepts `--surface popup` for closed selected
controls at these scales. Its command help and whitespace checks pass. The
four monochrome compositions passed and were visually reviewed: title
and selected labels fit and truncation stays before the arrow. The colour/PPC
matrix is running; inactive popup states and native oracle parity remain open.
Future runs also record and verify the capture binary hash to reject a binary
change mid-matrix. The already-running matrix predates that manifest addition.

The four reviewed monochrome scale compositions are archived with image hashes
in `popup-selected-scale-mono-review.json`. Inspection of the PPC CDEF reveals
that its selected box additionally insets top by 1, bottom by 2 and right by 1
before resolving text geometry. The current snapshot does not retain that box;
exact PPC baseline and clip parity therefore remain incomplete despite passing
input routing. Carry the resolved CDEF box into the presentation snapshot.

### Resolved popup CDEF box snapshot

Control snapshots now retain the selected box separately from contrlRect.
68k calls the existing popup_control_box_rect geometry path, including live
menu-based auto width and screen-edge clamping; PPC supplies its actual
1/2/1 top/bottom/right insets and title reservation. The GPUI selected chrome,
text canvas, truncation area and arrow container consume that box. This also
corrects the earlier assumption that the vertical insets were PPC-only. The
locked GPUI example check passes; the three-mode resolved-box regression passes (94.45 seconds). The completed scale matrix predates this correction and cannot prove
its visual parity. Fresh compositions and exact chrome/state parity remain open.

The resolved-box implementation also passes the locked GPUI example build.
A fresh twelve-capture matrix is running with binary-hash verification in a
new output directory. Earlier scale images are retained as historical evidence
only; they do not qualify this geometry revision.

The first fresh resolved-box monochrome composition at 0.75 scale is reviewed
and archived with source/image hashes in `popup-resolved-box-review.json`.
Selected text fits the inset box and truncates before the arrow. A post-change
run of actual GPUI pointer tracking at all twelve CPU/scale combinations has
been requested; its result is still pending.

All four fresh monochrome resolved-box compositions have now been visually
reviewed and archived with hashes in the same review manifest. Text fits the
resolved box at each scale. The post-change pointer test started successfully
inside the sandbox after automatic approval review timed out before launching
the elevated attempt. Colour/PPC captures and post-change input results remain
pending; no full popup or production-readiness claim is made.

All four fresh colour 68k resolved-box captures are also reviewed and archived
with hashes in the resolved-box review manifest. Labels remain bounded and the
long selected title truncates before the arrow at all four scales. PPC capture
and post-change input results remain pending.

The fresh resolved-box matrix completed successfully, including constant binary
hash and scene scale/aspect checks. All twelve full compositions have been
visually reviewed and archived in `popup-resolved-box-review.json`, retaining
commands, dimensions, source/binary/fixture/image hashes. PPC uses its smaller
text inset and all three modes retain bounded labels at each scale. Exact
chrome and disabled/inactive parity remain open. Post-change input is pending.

The ordinary crate also passes `cargo check --locked --no-default-features`
(18.73 seconds). All twelve archived image hashes and the recorded source
hashes match the current files, with twelve distinct CPU/scale pairs. These
checks do not establish inactive-state, native font or production readiness.

The post-change actual GPUI pointer regression passes all twelve CPU/scale
combinations (281.96 seconds), including press, held movement, release, exact
guest coordinates, highlighted row, committed control value and popup closure.
No native host pointer-capture or inactive/disabled/chrome qualification follows
from this result. The full production migration remains incomplete.

### Popup guest suspend capture route

A new host-suspended popup capture selects the long label through retained
guest tracking, then requests foreground suspension through MacintoshSession.
It waits for the actual visible owner window to become inactive and for a null
event after suspend handling, checks selected value 4 is preserved and confirms
no open popup remains. This uses guest activation state rather than changing
GPUI snapshot booleans. The reusable scale script accepts this state for popup
controls. Capture execution and visual review remain pending; this route does
not prove native host observer integration or native application switching.

The first monochrome 0.75 capture failed its inactive-owner assertion before
composition (`/tmp/gpui-popup-host-suspended-scales`). Investigation found that
the diagnostic stack marks the front window active regardless of its guest
`hilited` byte. The presentation snapshot now additionally requires that byte
to be set, preserving ordering while permitting guest HiliteWindow suspension.
The three-mode TextEdit suspend/resume regression now asserts presentation
activation after each guest-handled transition. The failed capture is not visual evidence.

The updated three-mode `showcase_text_selection_survives_host_suspend_resume`
regression passes (38.20 s), including presentation activation, preserved text
and selection, repeated suspension/resumption, and subsequent typing. The
modeless-window activation regression also passes (7.84 s; both 68K display
depths). The GPUI capture build passes. A fresh monochrome 0.75 suspended
popup capture passes its owner-state and retained-value assertions; visual
review confirms an inactive title and retained/clipped selected guest text.
The complete twelve-capture suspended matrix is still pending. These checks
do not establish native host observer or native Macintosh parity.

### Suspended popup fractional-scale review failure

The running `4cd6b1fc` capture matrix in
`/tmp/gpui-popup-host-suspended-fixed-scales` completed the four monochrome
scales, which were reviewed with preserved inactive title and popup labels.
Colour 68K 0.75 composition has a blank title bar, while its raw guest frame
contains “Toolbox Showcase”; colour 1.0 composition contains the title. This
is a failed presentation result, not a qualified matrix. Investigate title
snapshot/layout and fractional-scale clipping before accepting these captures.
The rest of the matrix and its final binary/hash check are still pending.

Disabled popup text is a separate unresolved fidelity defect: classic CDEF
`draw_control_label_text` uses a screen-coordinate checkerboard except on
8-bit devices, where it resolves dimmed ink through the live screen CLUT.
PPC selected text instead uses its theme frame ink; Appearance deactivation
changes that palette independently of contrlHilite. The GPUI closed labels
currently substitute host muted foreground for hilite-disabled text. A faithful
replacement needs resolved guest ink/pattern metadata, including global pattern
phase, instead of CPU or font heuristics. These source observations are not
native-oracle verification.

The twelve-capture matrix completed with a stable binary hash, correct scene
scale/aspect checks, and verified artifact hashes. All compositions were
reviewed. Both colour 68K and PPC 0.75 omit the inactive window title; the other
reviewed scales show it. Original compositions and the two failed guest frames
are archived as `popup-suspended-*`, with source-commit hashes and explicitly
failed review status in `popup-suspended-review.json`. No inactive-title
qualification follows from this matrix. Capture-only title snapshot/layout
diagnostics have been added for the next reproduction; their build is pending.

The diagnostic build passes (10.53 s). Its first PPC 0.75 reproduction logs
title “Toolbox Showcase”, inactive owner bounds (50,40,420,600), title origin
(266,45), clip (31,39,47,601), width 107 and ascent/descent 12/3. This time
composition visibly contains the title. No semantic rendering change was
made; the original failure remains unresolved and may be intermittent. Three
further identical diagnostic captures are running to investigate that result.

All three further PPC 0.75 diagnostic captures omit the title despite identical
correct owner/title/layout logs; their original captures and logs remain in
`/tmp/gpui-popup-suspended-ppc-title-repeats`. Added test-feature-only
`SYSTEMLESS_GPUI_TEXT_TRACE` logging for title canvas paint bounds, scale and
ink-run count to distinguish missing paint from clipping. Its build and traced
reproduction remain pending.

The paint-trace build passes (6.59 s). PPC 0.75 tracing reproduces the blank
title while logging two title paint calls with 180 ink runs, scale 0.75 and
bounds origin (199,24.5), size (80,11). The title snapshot is correct. This
narrows the unresolved failure to GPUI paint clipping/composition rather than
missing guest text or skipped title painting. The traced original PNG, raw
guest frame and log remain at `/tmp/gpui-popup-suspended-ppc-title-paint-trace.*`.

Expanded tracing confirms a valid mask at (29,23), size (421.5,12), and
opaque grey foreground (lightness 0.451, alpha 1), while the PPC 0.75 title
remains blank. An explicit title paint-layer experiment built successfully
(9.45 s) but the first repeated capture still omitted the title; the experiment
was removed. No rendering fix is established. Remaining investigation should
isolate the title in a minimal shared-compositor scene and inspect primitive
ordering/rendering without changing guest font metrics or hit coordinates.

### Title-only compositor isolation

A capture-only switch removes controls/dialogs/lists/TextEdit overlays while
retaining the same guest frame and standard window compositor. Its PPC 0.75
composition still omits the title, excluding later control/text overlays as
necessary triggers. The build passes (10.29 s). A red marker inside that canvas
and the title appeared together in one diagnostic capture (build 5.92 s), but
an opaque white canvas-background experiment still omitted the title in its
first repeat (build 13.51 s). Neither experiment establishes a fix; marker
painting was removed.

A test-only `SYSTEMLESS_GPUI_TITLE_PATH` branch batches the same guest glyph
ink runs as one GPUI fill path, with unchanged advance/baseline geometry. Its
build passes (18.82 s), and the first capture is running. It is diagnostic only:
path antialiasing/device snapping would need fidelity validation before any
production adoption. The normal draw path remains unchanged.

The first path diagnostic capture completed and visibly preserves the title
at PPC 0.75 (`/tmp/gpui-popup-title-path.png`), with unchanged guest title layout.
Three repeat captures are running to check consistency. This does not qualify
the path backend or establish the quad backend's root cause.

All three unsnapped-path PPC 0.75 repeats display the title, but pixel review
finds five grey/white levels versus the existing bitmap's two. Device-edge
snapping using GPUI's nearest-pixel, half-toward-zero rule removes this
antialiasing difference: the snapped PPC 0.75 title region (398,49)-(558,71)
is pixel-identical to the previously working monochrome quad title, including
719 grey ink pixels and 2801 white pixels. This checks internal bitmap raster
preservation, not native Macintosh font parity.

The shared title geometry now uses snapped GPUI paths as a candidate fix;
TextEdit's selection/caret path retains its existing drawing. Removed the
title-only capture switch and test-only path selector. The candidate capture
build passes (7.11 s). A fresh twelve-capture suspended matrix, production
binary check and GPUI title-drag interaction regression are running. Broader
active/inactive title validation and performance remain pending.

The production `systemless` binary check passes (10.31 s); the GPUI title-drag
regression passes (0.06 s). The first three fresh monochrome captures (0.75,
1.0,1.5) are reviewed and their title region is pixel-identical to the earlier
working quad captures at each corresponding scale. The remaining matrix and
full title/backend qualification are still pending.

The fresh suspended matrix is complete: all twelve compositions are reviewed,
and each title region is pixel-identical to the corresponding original working
monochrome bitmap capture. Source, binary, fixture and artifact hashes were
independently checked before lossless archival as `popup-snapped-title-*` and
`popup-snapped-title-review.json`. The earlier failed `popup-suspended-*`
evidence is retained. This supports the snapped-path fix for this standard
fixture title across three CPU/display modes and four scales, without changing
font metrics, guest event routing or application-drawn text. It does not prove
native font parity or explain the underlying GPUI quad failure. A matching
active twelve-capture matrix is running; general title resources, native host
interaction and performance qualification remain open.

The matching active matrix is complete and reviewed: all twelve titles are
visible and each title region is pixel-identical to its original active quad
capture. Stable binary/source/fixture hashes and all original artifact hashes
were verified before lossless archival as `popup-snapped-title-active-*` and
`popup-snapped-title-active-review.json`. Together, these two matrices cover the
fixture's active and suspended title at 0.75, 1.0, 1.5 and 2.0 on monochrome
68K, colour 68K and PPC. The title-drag regression continues to cover forwarding
guest coordinates. Arbitrary title resources, native host input/activation,
native font parity and performance remain unqualified; disabled popup ink and
arrow shape still require faithful guest-resolved presentation.

### Guest-resolved popup text ink candidate

A new `ControlTextInk` snapshot candidate replaces host muted foreground for
closed standard popup text. Classic disabled eight-bit ink resolves the CDEF's
live device-CLUT entry through display gamma; monochrome dimming retains global
guest checker phase. PPC uses the same active/deactivated control-palette helper
as its CDEF painter, preserving the distinction between contrlHilite and
Appearance DeactivateControl. Solid ink keeps bitmap-run batching. Font metrics,
selected-label bounds and guest input paths are unchanged. The GPUI example
check passed before extracting the shared classic ink helper; the existing
live-palette guest-pixel regression is extended to compare snapshot ink, and its
run plus a capture build are pending. Disabled composed captures, global phase
under clipping/scales, custom palettes and interaction qualification remain
required before claiming fidelity.

The live-palette guest-pixel regression passes (0.04 s after compilation),
including the new comparison between disabled snapshot ink and the actual
palette entry written by the CDEF. The public showcase now accepts `d`/`e`
on its popup page to disable/re-enable both controls through guest
HiliteControl; the MPW 68K/PPC rebuild succeeded. A new disabled capture case
waits for both guest records and completed painting, then uses the existing
shared compositor. The three-mode popup regression now checks disabled
mouse-down rejection, unchanged selected value, re-enabling and subsequent
guest tracking. Its run is queued behind the capture build, which is still
compiling. No disabled composed rendering qualification is established yet.

The first disabled capture used a build that did not execute the disable action;
its visibly enabled guest/composed text is retained at
`/tmp/gpui-popup-disabled-mono-075*` and is not qualification evidence. The
three-mode test also failed on the second selection because its helper assumed
an initial value of 1, whereas re-enabling correctly retained value 4. The
helper now retains the pre-tracking value and derives the target row from live
popup geometry. Capture state now asserts every visible popup is disabled and
logs guest hilite/ink. The fresh build passes (11.77 s); a new capture and the
corrected guest interaction regression are running.

The rebuilt mono 0.75 disabled capture completes with explicit guest hilite 255
and Checker ink for both controls. Guest/composed frames are reviewed at
`/tmp/gpui-popup-disabled-mono-075-v2*`: GPUI now shows patterned disabled glyphs.
An independent black-pixel comparison against device-edge-scaled guest selected
text is not exact (loadout: 71 missing/160 extra physical pixels; theme: 22/24
in the inspected regions), so scale/phase fidelity remains unqualified. The
production binary check passes (1m13s). A fresh twelve-capture disabled matrix
is running against a fixed binary with source fingerprints recorded before it.
The corrected interaction regression fails at completed-popup disappearance
when reselecting an already-selected value: its readiness predicate accepts
value 4 before tracking closes. Require both the retained value and closed
tracking before retrying; do not treat this failure as interaction parity.

The helper readiness predicate now requires the selected value and closed guest
tracking together; its three-mode retry is running. The unchanged capture
binary remains fixed during the matrix; its source copy and original hash are
preserved separately before the helper edit. Mono 1.0 selected Theme black ink
matches the device-edge-scaled guest exactly in the inspected text region. The
Loadout region's only difference is a guest arrow pixel at (423,158), outside
the selected-text boundary; exclude arrow ink from subsequent text comparisons.
The remaining mono fractional-scale mismatch still needs layout/device-edge
investigation rather than a font substitution or regenerated baseline.

The corrected popup interaction regression passes on monochrome 68K, colour
68K and PPC (224.96 s): disabled controls reject tracking, preserve selection,
and track again after guest re-enabling. The original disabled twelve-capture
matrix completed and was visually reviewed; its fractional-scale mismatch
remains failed fidelity evidence. The GPUI bitmap text now derives its origin,
width and height directly from guest CDEF bounds and the shared scene transform,
avoiding fractional host layout rounding. The example check and capture build
pass. A fresh mono 0.75 capture is reviewed at
`/tmp/gpui-popup-disabled-canonical-mono-075.png`; independent device-edge-scaled
black-pixel comparisons exactly match both selected labels (Loadout region
305,153–419,169; Theme 295,189–370,205). Arrow ink is excluded. This single
capture does not qualify other modes, scales, clipping or live letterboxing.
A fresh fixed-binary twelve-capture matrix is running at
`/tmp/gpui-popup-disabled-canonical-scales`, with source hashes recorded before
capture in `/tmp/gpui-popup-disabled-canonical-source-sha.json`.

All 24 original disabled-matrix PNGs are losslessly archived as
`tests/toolbox-showcase/reference/gpui-demo/popup-disabled-layout-*`; the review
manifest records the failed fractional-scale comparison, original source
fingerprints, and binary provenance limits. The corrected matrix's mono 0.75
and 1.0 selected-text regions both match the scaled guest black mask exactly
and their composed images are reviewed. The public no-default-features check
passes (15.41 s). The remaining ten corrected captures and their review are
pending; this checkpoint does not claim complete popup or production fidelity.

The corrected disabled matrix completes across monochrome 68K, colour 68K and
PPC at 0.75, 1.0, 1.5 and 2.0. All twelve full compositions are reviewed, with
visible titles and contained selected labels. All 24 selected-label regions
exactly match independently device-edge-scaled guest ink: black checker ink in
mono, RGB 150/150/150 in colour and active CDEF black in PPC. Each comparison
excludes arrows and borders; its coordinates and pixel counts are recorded in
`popup-disabled-canonical-review.json`. All 24 original PNG hashes/dimensions,
the fixed binary, fixture and eight source hashes were verified before lossless
archival as `popup-disabled-canonical-*`. This qualifies the inspected fixture
regions in the shared compositor, not native typography, arbitrary palettes,
clipping/letterboxing, popup arrow/chrome or the broader release gate.

### Shared classic popup indicator candidate

The GPUI closed popup still used a host “▾” glyph for both CPU CDEFs. A new
shared geometry helper now retains the classic 68K downward triangle and dotted
disabled pattern, and PPC paired up/down marks. Both guest painters and popup
snapshots use that helper; GPUI paints the resolved runs with canonical global
guest coordinates and solid guest ink. Classic colour ink resolves the same
logical black ColorTable index as the guest framebuffer path. Themed indicator
projection remains unfinished and still uses the previous host glyph. Build,
guest raster preservation, compositor captures and interaction checks are
pending; no fidelity qualification is claimed for this candidate.

The initial candidate's example check passes (1m09s), capture build passes
(3m05s), and public no-default-features check passes (22.09s). Its reviewed
mono 0.75 disabled composition at `/tmp/gpui-popup-indicator-mono-075.png`
exactly matches both guest indicator ink masks after independent device-edge
scaling. The raw guest pixels are unchanged from the archived canonical disabled
capture, so the shared geometry refactor preserves this guest raster. PPC
snapshot selection was subsequently moved to a helper reading the live main
GWorld theme, matching the CDEF even when a one-bit world forces classic
presentation. That metadata correction is rebuilding; the three-mode guest
interaction regression is running against the initial candidate. Further CPU,
scale, active-state and themed-indicator qualification remain pending.

The live-theme PPC metadata correction's capture build passes (27.72s). A
fresh fixed-binary 36-capture matrix is running at
`/tmp/gpui-popup-indicator-scales`: active, host-suspended and disabled states
across all three CPU/display modes and four scales. Twelve source/fixture
fingerprints are recorded before capture in
`/tmp/gpui-popup-indicator-v2-source-sha.json`. Review and guest/composed pixel
comparison remain pending; the prior single-capture proof is still scoped to
monochrome disabled indicator geometry.

The initial candidate's real guest popup interaction regression passes across
all three CPU/display modes (149.90s), including disabled rejection, retained
selection and tracking after re-enabling. The shared shape helper and guest
painters are unchanged by the subsequent PPC live-theme metadata correction.
This is interaction evidence; the full composed state/scale matrix remains
running and is not yet qualified.

### CPU-owned TextEdit line geometry candidate

Styled TextEdit still retains guest drawing while its GPUI run ink, colour,
selection and editing proof remain unfinished. A new shared snapshot geometry
API preserves the existing CPU line-layout rules: classic 68K stacks canonical
LHElement heights cumulatively; PPC recomputes mixed-run height/ascent and uses
the current line's height times its index. The CPU adapter explicitly records
that policy instead of relying on a host paragraph engine. Geometry also uses
the shared guest justification helper and supports styled caret ownership at
wrap/scroll boundaries. PPC run metrics are resolved by intersecting run byte
ranges, avoiding repeated per-byte style searches in the frontend.

The ordinary GPUI document component now consumes this API for its eligible
plain lines, retaining its guest-font ink, selection inset and Toolbox input
routes. Styled/justified eligibility remains guarded until faithful run
painting and interactions are established. The extended snapshot test covers
different line heights, native metrics independent of LHTable, justification
and styled caret boundaries. Real three-mode snapshot/TEClick tests now derive
the clicked glyph boundary from the shared geometry. Checks and tests are
running; no rendering or editing qualification is claimed for this candidate.
The simultaneous popup matrix remains on the fixed `d46a9f65` capture binary;
its source fingerprints refer to that committed source, before these edits.

### Completed classic indicator matrix and line-geometry checks

The fixed `d46a9f65` popup matrix completed all 36 captures: monochrome
68K, colour 68K and PPC; active, host-suspended and disabled; scales 0.75,
1, 1.5 and 2 at host density 2. All 72 original guest/composed artifacts
are archived as `popup-classic-indicator-*`, with capture commands, hashes,
dimensions and twelve source fingerprints in
[`popup-classic-indicator-review.json`](tests/toolbox-showcase/reference/gpui-demo/popup-classic-indicator-review.json).
The source fingerprints were checked against that committed source, independently
of the newer TextEdit edits. Binary and fixture hashes were unchanged.

All 144 selected-label and classic-indicator regions exactly match nonempty
raw guest ink masks after independently snapping guest pixel edges to device
pixels. The twelve disabled raw guest images remain pixel-identical to the
previous canonical disabled matrix. All 36 popup-region crops were visually
reviewed in three contact sheets; the full compositions were not reviewed for
this matrix. This establishes these selected text and classic indicator regions
in the shared compositor, not native Macintosh font parity, arbitrary palettes,
full chrome, clipping/letterboxing, themed indicators or release readiness.

The CPU-owned TextEdit geometry unit regression passes, including mixed-run
metrics, canonical 68K LHTable placement, PPC run placement, justification,
visible styled caret ownership and deep negative scrolling origins. Deep-scroll
placement deliberately preserves the current CPU arithmetic difference rather
than normalizing it to host typography. Both actual guest snapshot/TEClick
tests pass (232.29s together), and the existing owner/front-window clipping and
custom fallback test passes. The latest example check (8.37s) and public
no-default-features check (14.47s) pass. Styled presentation remains guest-owned;
these results do not establish styled GPUI painting or editing qualification.
The production command check passes (41.38s). The actual GPUI document
click/drag scale regression is running against the latest geometry source.

Reproduce the scoped popup proof without regenerating references:

```sh
python3 tests/toolbox-showcase/verify-gpui-popup-indicators.py \
  tests/toolbox-showcase/reference/gpui-demo/popup-classic-indicator-review.json
```

The verifier passes all 72 artifact hashes/dimensions, committed source
fingerprints, 144 nonempty exact ink regions and twelve unchanged disabled
guest rasters. Binary/fixture fingerprints were independently verified at
matrix completion; the archived verifier does not require that old executable.

### Styled run projection and editable fixture candidate

The actual GPUI plain document click/drag regression for `e56522b8` passes
across monochrome 68K, colour 68K and PPC at 0.75/1/1.5/2 scales (226.49s).

`TextEditSnapshot::visible_style_runs` now intersects canonical style records
with each guest-wrapped visible line, retaining absolute Mac Roman byte ranges
and the original font/face/size/RGB16 attributes. Both guest drawing paths trim
trailing spaces and line breaks for ink; line metrics still use the full range.
The projection rejects missing, overlapping or out-of-bounds style ownership.
Unit and real three-mode snapshot checks are running; no styled GPUI ink,
palette, selection or editing qualification is claimed from this API.

The showcase styled field gains click-to-focus `TEActivate`/`TEClick`, `TEKey`,
`TEIdle` and activation-event routes. Initial sample presentation stays unfocused;
leaving the page clears focus. This provides public guest-driven editing
coverage instead of writing TERec fields directly from the host. Its new
three-mode regression checks insertion, shifted run attributes, suspend/resume
and deletion restoring the original text and styles. The final fat fixture
build and this regression remain pending. Styled frontend ownership remains
guarded until faithful GPUI rendering and interactions are established.

The final MPW fat fixture build passes and its copied `showcase.c` input matches
current source. Archive SHA256 is
`17e3cd4dd1a2510627b55487db166773759319f93454bb9141ad8a80bc40d4b8`.
The extended real three-mode style projection test passes (4.68s); the shared
unit regression and public no-default-features check also pass (16.55s for the
latter). The archived popup verifier still passes against its pinned old source.

The new editing regression fails in colour 68K: insertion changes the text but
observed style run starts remain at their original offsets. Waiting for a null
event after insertion/redraw reproduces the failure (29.48s), so the first
failure was not qualified away as partial painting. Monochrome passes before
that failing mode; PPC editing is not reached. Conditional TextEdit tracing
now records the edited span, original/edited run starts and write result. A
fresh traced reproduction is compiling. This guest editing failure must be
resolved before styled GPUI replacement; the failed test is retained.

The completed trace corrects the earlier mode attribution: **PPC** is the
failing CPU. Both 68K modes write, draw and restore shifted style runs correctly;
PPC initialization does not produce the same `[INIT]` log prefix. The PPC edit
commit path called `ppc_te_set_text` without restyling the changed span. The
candidate now moves styles before recalculating layout, using shared byte-span
and run-movement semantics with 68K. It retains null-style insertion intent,
font/face/size/RGB16/metrics, and the native text-length limit. Text allocation
growth is reserved before changing style ownership; resize errors propagate.
The existing PPC style-table serializer is shared with `TESetStyle`.

The initial fix passes both real three-mode styled snapshot/editing tests
(21.07s together), including guest clicks, insertion, suspend/resume and deletion
restoring original attributes. A subsequent allocation-growth safeguard and
inside-blue-bold-run insertion case are undergoing fresh checks. Temporary
style diagnostics were removed. This is guest editing support and the model
needed for faithful replacement; styled GPUI painting remains unfinished.

The final styled regression passes (38.07s for both tests), including insertion
at the first byte and inside the blue bold run, inherited insertion style,
unchanged surrounding attributes, suspend/resume and backspace restoring the
original runs in all three modes. Both helper regressions pass: identical-byte
insertion remains anchored to the guest caret and classic TEDelete/TEInsert
retain their established style semantics. The latest example check passes;
production command check (13.78s) and public no-default-features check (7.24s)
pass. This completes the identified PPC edit bug, not styled GPUI rendering.

Styled TextEdit run placement now has a shared `styled_line_geometry` projection:
it returns canonical Mac Roman byte ranges and every insertion boundary for
visible runs, aligned together using the existing CPU-specific line geometry.
The caller supplies the owning guest painter's byte advances; this API does not
resolve fonts through host typography or convert canonical RGB16 styles into
host colours. Tests cover mixed advances, a Mac Roman high byte, trimmed styled
whitespace, right/centre alignment, shared style boundaries, saturation and
invalid overlapping ownership under both line policies. This is compositor
plumbing, not evidence of faithful styled GPUI painting. The next integration
must supply CPU-specific strike/ratio/spacing and palette-resolved ink, including
68k per-character versus PPC per-run underline/outline synthesis, before removing
the styled-field guest-rendering guard.

The snapshot's `guest_styled_line_geometry` now supplies advances from the actual
TextEdit measurement policies, shared with both guest adapters. Classic 68k adds
style spacing after integer strike scaling; PPC applies the Font Manager ratio
to the styled advance and rounds as QuickDraw does. This distinction remains
explicit instead of choosing a host font with similar appearance. The PPC
comparison exercises all 256 bytes and 128 style combinations across five font
families and eight sizes against the existing QuickDraw width function. Styled
ink still needs palette resolution, port spacing and CPU-specific synthesis;
measurement equivalence alone does not establish GPUI rendering ownership.
The final shared-advance comparison and snapshot regression pass together (6s),
covering 1,310,720 PPC byte/style/font/size combinations. Both native styled
TextEdit allocation/measurement tests pass (0.47s), as does the classic styled
TEDelete/TEInsert regression (0.04s). No new styled visual qualification is claimed.

PPC styled run ink now has a shared source-strike visitor extracted from the
native QuickDraw painter, including hollow outline/shadow synthesis and its
special run underline interaction. The native painter and pure GPUI recipe also
share rational pixel footprints: floor negative bearings and retain at least
one destination pixel for a shrinking source pixel. `ClassicLine::ppc_styled_run`
accepts the guest's measured insertion positions independently of mask extents,
then builds the same binary horizontal ink spans used by the compositor. It is
explicitly a zero-CharExtra recipe; caller palette, transfer mode, clipping and
ownership policy still apply.

A native-memory drawing comparison passes for 1,280 cases: Geneva/Monaco,
9/10/12/14/24 points, every low-seven-bit face combination, and `A i` plus Mac
Roman byte 0x8e. The test clears and draws the guest framebuffer, then compares
all pixels in its asserted containing region with the pure run recipe (13.06s).
This is guest-implementation equivalence, not an external Macintosh oracle.
The GPUI component regression passes 640 size/style cases (4.66s), preserving
independent insertion positions and every binary ink pixel after span packing.
Styled field replacement remains guarded: classic 68k synthesis, resolved
palette ink/port spacing, selection/caret and scaled live/headless compositions
still need integration and qualification before claiming faithful replacement.
The final footprint saturation/bounds regression and 1,280-case guest ink
comparison pass together (10.84s). The pure recipe is a binary union for
srcCopy/srcOr presentation; overlapping hits in other transfer modes can have
different semantics and remain guest-owned. Nonzero CharExtra stays in the
native painter; its existing nonspace-spacing regression passes (0.10s).
The final GPUI component regression passes (5.77s), and the production binary
and no-default-features checks pass (19.07s and 13.04s respectively). The new
run component is not yet wired into styled-field ownership, so these results
must not be interpreted as a rendered styled-field scene qualification.

Classic 68k TextEdit now has a shared per-character glyph coverage function
extracted from `draw_char`. Native QuickDraw continues to own erasure, clipping,
palette resolution, transfer modes and pen/spacing updates. The shared function
retains integer strike scaling, intrinsic/synthetic italic, bold, descender
breaks in per-character underlines, and outline/shadow interactions. Continuous
DrawString underline bounds/breaks are also passed through; this state is not
invented for TextEdit. The pure `classic_textedit_glyph_ink` recipe refuses
unrepresentable raster bounds, preserving the option to retain guest pixels.

`ClassicLine::classic_textedit_run` assembles this ink without replacing the
caller's guest insertion positions. It is separate from the existing line-wide
label recipe and from PPC ratio/run synthesis, and currently requires zero
CharExtra/SpaceExtra with srcOr. A real guest-memory comparison passes all 5,120
cases across monochrome/8bpp ports, Geneva/Monaco, 9/10/12/14/24 points, all 128
low-seven-bit face combinations, descender `g` and Mac Roman byte 0x8e (4.12s).
The GPUI run/span regression passes 640 size/style cases with descenders,
interior space and the high byte (1.20s). These are guest implementation
comparisons, not new Macintosh-oracle or rendered scene qualification.
Styled TextEdit ownership remains guarded pending resolved palette/spacing,
selection/caret integration, and live/headless CPU/scale/state verification.
The final shared-coverage tests pass together (4.46s), including explicit
continuous-underline breaks. The classic styled TEDelete/TEInsert regression
also passes (0.02s). Production and no-default-features checks pass (14.83s and
6.90s). The new GPUI run constructors remain unwired pending the field's resolved
paint data and interaction/scene qualification; no release readiness is claimed.

Styled TextEdit snapshots now carry read-only resolved paint inputs separately
from canonical RGB16 style intent: destination depth, transfer mode, CPU-specific
CharExtra, SpaceExtra, and each run's pixel/display RGB/inverted display RGB.
Classic 68k resolves screen run ink through the native screen CLUT policy;
PPC uses its destination colour table at 8bpp and native RGB555 at its default
16bpp. The RGB555 decoder is shared with the existing display compositor.
Indexed colours retain physical display gamma. Offscreen/unsupported surfaces
remain unresolved rather than assuming host colours or host font metrics.

The real Toolbox Showcase styled-field snapshot regression passes monochrome
68k, colour 68k, default 16bpp PPC, and explicit 8bpp PPC (8.74s), requiring a
settled null event and intact drawing and finding every resolved run colour in
the guest field raster. This checks colour resolution, not full glyph masks,
inversion/selection painting, altered palettes, fades, or rendered GPUI scenes.
The CPU spacing eligibility regression passes (0.01s), including negative
fractional classic Fixed values and PPC's ignored SpaceExtra.

Both GPUI styled-run components now expose native paint advance separately
from their supplied guest insertion positions. This retains PPC's distinction
between whole-run drawing advances and per-character rounded TEClick widths;
subsequent runs must use the former while caret/selection use the latter.
The classic/PPC 640-case component regressions pass (1.85s/8.04s), checking
native ink, paint advance, and unchanged supplied positions. No host shaping
or modern font substitution is introduced. Styled-field ownership remains
guarded pending background/transfer/selection integration, glyph-aligned
interaction mapping and shared live/headless CPU/scale/activation evidence.
Production binary and no-default-features checks pass after these changes
(1m30s/11.22s), with dead-code warnings for the guarded run constructors.

Styled TextEdit now has an ordered GPUI ink plan that keeps absolute measured
byte boundaries separate from each run's actual native paint origin. Run masks
use the shared classic per-character or PPC whole-run recipes, CPU baseline
and justification, and resolved ink colours. Overlapping masks resolve in
native draw order. Saturated/unrepresentable paint coordinates are refused;
background and selection remain separate operations. A new GPUI canvas paints
this resolved bitmap ink with device-snapped paths and caller-owned clipping.

The Showcase styled snapshot regression now compares every field pixel to the
ordered ink plan over the fixture's white erased background across monochrome
68k, colour 68k, 8bpp PPC and default 16bpp PPC (final 3.29s). It covers mixed
Geneva/Monaco sizes/faces/colours and actual paint/measurement distinctions,
not arbitrary styles, font overrides, transfer modes or selections.

`--capture-styled-text-edit-ink` uses the public fixture and shared GPUI/headless
renderer to remove the inactive field's guest text pixels and repaint only its
new canvas. It saves the original `.guest.png` alongside the composed image.
The white fixture background is painted by GPUI rather than relying on a
filtered guest texture: the first capture exposed grey edge bleed, corrected
before the final matrix. All 16 field captures match the device-snapped native
raster exactly across the four CPU/display cases at 0.75/1/1.5/2 scales and
device density 2. The production binary check passes (3.78s), and the capture
example builds (5.79s). Styled document scene ownership is still guarded:
these captures qualify inactive ink on this fixture, not live input, general
backgrounds, active selection/caret, or lifecycle replacement.
The 16 composed captures and 16 untouched-pixel guest baselines are archived in
`tests/toolbox-showcase/reference/gpui-demo/styled-text-ink/review.json`, pinned
to source commit `9c43d1f1`, fixture/source hashes, original capture byte hashes,
CPU/depth/scale commands and final PNG hashes. PNG archives were losslessly
recompressed with RGBA equality checked. Recheck the exact field matrix with:

```sh
python3 tests/toolbox-showcase/verify-gpui-styled-text-ink.py \
  tests/toolbox-showcase/reference/gpui-demo/styled-text-ink/review.json
```

The verifier requires all four modes and four scales once each, device density
2, file hashes and zero mismatched field pixels. This is implementation-to-
guest comparison; it does not add new Macintosh-oracle evidence.

Styled selection now has a read-only guest geometry projection. It measures
canonical byte ranges with the shared CPU advance policy, normalizes reversed
ranges, retains the left inset rule and view clipping, and distinguishes
classic trailing space/return selection from PPC's trimmed visible-line range.
PPC selection top is derived from its saturated drawing baseline; the GPUI ink
plan also retains the native baseline addition order. The snapshot regression
passes clipping, inactive/no-range cases and the signed-coordinate saturation
boundary (0.09s). Existing four-mode inactive ink comparison passes (3.43s).

The styled canvas can apply classic selection by inverting resolved pixel
colours after ordered run painting. It uses each run's physical inverted ink
and a caller-supplied qualified erased background, retaining RGB16 style
intent rather than substituting a host accent colour. The public fixture
capture drives a real guest drag from byte 0 to 26 across style changes and
can request suspend/resume through the frontend session. Each transition
checks unchanged guest text, selection and style runs and waits for null-event
repaint completion before comparing every native field pixel and rendering.

At this checkpoint, initial selected PPC16 at scale 1 and all four monochrome
selected/suspended scales have exact compositor field comparisons. The larger
selected/suspended/resumed CPU/scale capture matrix is not yet archived as
qualification. The verifier still passes the existing 16 archived inactive
captures, now checking pinned Git source and fixture hashes as well as PNG
hashes, dimensions and complete CPU/scale/state matrices. Production and
no-default-features checks pass (13.24s/7.14s). General backgrounds, modern
theme selection, caret rendering, live hit-testing and styled production scene
ownership remain unfinished; capture session activation does not independently
prove the native host-window observer.

Styled TextEdit caret geometry can now project the visible caret fragment in
port coordinates while preserving the existing guest caret owner and blink
phase. The helper uses canonical CPU-specific range measurement, the native
one-pixel offset adjustment, the supplied classic theme width versus PPC's
fixed one-pixel width, and view clipping. Snapshot tests pass wrap ownership,
PPC trimmed CR measurement, classic partially scrolled lines, width, inactive
and hidden-blink states, and suppression during a nonempty selection (0.08s).
Caret ink, pen patterns, theme painting and guest pixels outside the owned view
remain separate; this is not rendered caret or production ownership evidence.
Production and no-default-features checks pass (40.41s/12.23s).

The completed styled selection matrix is now archived in
`tests/toolbox-showcase/reference/gpui-demo/styled-text-selection/review.json`:
48 composed captures and 48 original guest baselines cover monochrome 68k,
colour 68k, PPC8 and PPC16 at 0.75/1/1.5/2 scales, device density 2, in selected,
suspended and resumed states. All 48 have zero mismatched field pixels. The
manifest pins source commit `6112e194`, the capture executable, fixture and
source hashes, commands, original capture hashes and lossless archive hashes.
RGBA equality was checked during recompression; the archive occupies about
4 MB. Recheck with:

```sh
python3 tests/toolbox-showcase/verify-gpui-styled-text-ink.py \
  tests/toolbox-showcase/reference/gpui-demo/styled-text-selection/review.json
```

This qualifies the fixture's mixed-style field, real guest drag and session
activation assertions with classic inversion, zero-spacing srcOr and a white
erased background. It does not qualify rendered caret, arbitrary backgrounds
or themes, production styled scene ownership, GPUI pointer mapping or native
host-window activation observers. The later caret geometry helper is outside
the pinned capture source.

A shared GPUI styled solid-caret canvas can now paint caller-qualified native
ink over ordered glyph ink, preserving overlapping text outside the caret and
rejecting simultaneous selection or malformed rectangles. The focused canvas
plan regression passes (0.02s), and the production binary checks (6.53s).
Native source inspection establishes that PPC classic carets use the style
colour at the insertion offset, whereas classic 68k uses the final drawing
pen state or the theme painter. The canvas deliberately requires resolved ink
and clipped geometry rather than inferring either CPU's policy from host text
styles. It is not yet wired into capture or production ownership; patterned
classic pens, themed caps and rendered caret compositor qualification remain
unfinished.

The public fixture now has `--capture-styled-text-edit-caret OUTPUT` for the
default classic-theme PPC path. It clicks through existing guest mouse events
at measured byte offset 26, requires active collapsed selection and intact
visible caret drawing after a null event, then resolves the insertion run's
physical ink and clips the caret fragment to the owned view. Both the native
field check and shared headless GPUI canvas include the caret; original guest
pixels are retained separately. Initial PPC16 scale-1 and PPC8 scale-0.75
captures have zero differing field pixels at device density 2 and were visually
inspected. Example build passes (4.17s), production check passes (2.73s), and
the four-mode styled snapshot regression passes (2.75s).

```sh
target/debug/examples/gpui-menu-demo \
  tests/toolbox-showcase/toolbox-showcase.sit --prefer-powerpc \
  --capture-scale 1 --capture-styled-text-edit-caret /tmp/ppc-caret.png
```

Default PPC depth is 16; use `--screen-depth 8` for PPC8. The new command rejects
classic 68k pending pen/pattern metadata rather than substituting insertion
style ink for classic's final painting pen. These two temporary captures are
initial evidence, not an archived CPU/scale/blink/activation matrix or native
Macintosh oracle evidence. Production styled scene ownership is still guarded.

Classic 68k caret qualification now retains the actual painted fragment at
TextEdit's native drawing boundary. A completed drawing stores solid-caret
metadata only when every pixel in the clipped fragment is identical; mixed
patterns, empty/out-of-view rectangles and unsupported depths are refused.
The snapshot resolves that physical pixel through the current screen palette
and gamma only while the original drawing remains intact. GPUI checks the
current active/blink/collapsed selection and owner geometry against the retained
fragment before composing it. This avoids inferring classic caret colour from
the insertion style or the restored caller foreground. Disposal, reuse and a
fresh non-caret drawing clear the retained metadata.

The same `--capture-styled-text-edit-caret` command now accepts classic 68k.
Initial monochrome scale-1 and colour scale-0.75 captures both match every field
pixel in the shared compositor at device density 2 and were visually inspected.
These join the earlier initial PPC captures, not a complete archived matrix.
All six drawing-evidence regressions pass (0.01s), including packed view edges,
uniform/mixed fragments, clipping and caret metadata disposal/replacement.
The capture example builds (59.15s); production check passes (53.83s), and the
no-default-features check passes (8.87s). Themed caps and nonuniform caret rasters
still need faithful GPUI painting, and production styled scene ownership,
multiple insertion positions, blink/activation and the complete scale matrix
remain unfinished. No new native Macintosh oracle qualification is claimed.

Caret fixture captures accept `--capture-styled-caret-offset BYTE_OFFSET`
(default 26). Pointer interaction also asserts unchanged text, canonical style
runs and owner generation. At offset 0, colour 68k's actual caret is purple
(RGB 179/84/179), whereas PPC16's is black: the native final drawing pen and
insertion style differ even at the same guest byte boundary. Both initial
scale-1 captures match every composed field pixel at device density 2 and were
visually inspected. This is a targeted policy counterexample, not a complete
caret matrix. Example build passes (15.11s), production check passes (14.75s).
The two start-position colour counterexample captures and their original guest
baselines are archived under
`tests/toolbox-showcase/reference/gpui-demo/styled-caret-colour/review.json`,
pinned to `1ed0293b`, source/fixture/executable and original/archive image hashes.
Lossless recompression was checked for RGBA equality. The targeted verifier
checks both complete field rasters and their differing uniform caret columns:

```sh
python3 tests/toolbox-showcase/verify-gpui-styled-caret-colour.py \
  tests/toolbox-showcase/reference/gpui-demo/styled-caret-colour/review.json
```

The verifier passes both cases and explicitly reports that this is not full
matrix qualification. CPU/scale/blink/activation coverage and styled production
ownership remain required.

Styled caret captures accept `--capture-styled-caret-state` with `visible`,
`blink-off`, `suspended` or `resumed`. Blink-off advances the guest clock and
waits for the application's TEIdle, intact drawing and a null event; it never
writes caretState or introduces host blinking. Activation still uses session
requests. Captures retain actual depth, phase, active state, insertion range,
generation, tick and view geometry in a JSON sidecar after successful rendering.
Text, canonical styles, selection and owner generation must survive each phase.

This exposed PPC TEIdle toggling the flag without updating caret pixels. The
native dispatcher now erases and redraws the guest view only when TEIdle changes
phase, using the existing port background and native TextEdit painter. The
four-mode fixture framebuffer regression passes visible/hidden/visible pixels
and preserved text/style/selection (23.14s); the existing PPC guest timing/phase
regression passes (0.07s). Four true PPC16 scale-1 captures at insertion 0 pass
all field pixels through the shared compositor and confirm depth 16 in sidecars.
Example build passes (85s), production check passes (98s). These smoke captures
are temporary; the corrected complete CPU/scale/insertion/activation matrix
and production styled ownership remain unfinished.

A reusable macOS capture runner now checks the corrected styled matrix:

```sh
cargo build --locked --example gpui-menu-demo --features gpui-demo-test
python3 tests/toolbox-showcase/capture-gpui-styled-text-matrix.py \
  /tmp/systemless-styled-text-matrix-new
```

It requires a fresh output directory and a capture executable built from the
current clean source. Its 192 cases cover four CPU/depth modes and four scales:
16 inactive fields, 48 selected/suspended/resumed fields, and 128 caret fields
at insertion offsets 0 and 26 in visible/blink-off/suspended/resumed states.
Every case checks actual snapshot depth and state, preserved selection, intact
native drawing and exact composed field pixels at device density 2. Each PNG,
original guest PNG and state sidecar has a recorded hash, with source revision,
fixture and unchanged executable hashes retained in progress.json. A failed
case stops the run and retains its error; completed cases are not silently
regenerated. This is field-raster qualification infrastructure, not proof of
production ownership or arbitrary theme/background/custom-font policies.

The corrected run is in progress against `d7c3e82f`; results must be archived
and reviewed before replacing the invalidated full-matrix qualification.

The corrected matrix has dedicated verification and archive tools:

```sh
# Observe completed cases without asserting full qualification:
python3 tests/toolbox-showcase/verify-gpui-styled-text-matrix.py \
  --partial /tmp/systemless-styled-text-matrix-new/progress.json
# After the existing capture job reports completion:
python3 tests/toolbox-showcase/archive-gpui-styled-text-matrix.py \
  /tmp/systemless-styled-text-matrix-new/progress.json \
  tests/toolbox-showcase/reference/gpui-demo/styled-text-qualified
python3 tests/toolbox-showcase/verify-gpui-styled-text-matrix.py \
  tests/toolbox-showcase/reference/gpui-demo/styled-text-qualified/review.json
```

The full verifier requires all 192 distinct CPU/depth/scale/state/insertion
combinations, source hashes and a completed job. It checks actual depth and
phase sidecars, active state, selection, generation, view geometry and all
field pixels. Negative checks reject wrong depth and duplicate cases. Partial
checking explicitly reports incomplete qualification; 43 completed cases have
been independently rechecked. The archive tool refuses a live incomplete job,
verifies originals, retains original byte hashes, losslessly recompresses PNGs
with RGBA equality and verifies the archive again. The original corrected job
continues on its unchanged executable; the final archive is not yet produced.

### Whole-field styled paint qualification

`StyledTextEditPaintPlan::qualify` constructs a clipped field recipe from guest
strikes, run paint and measured geometry, then checks every field pixel against
intact native drawing. The caller supplies resolved background and caret paint;
this does not infer host typography or theme colours. Classic paints visible
line ink before highlights; PPC highlights after each line, allowing later ink
to overwrite earlier selection in overlapping mixed-height lines. Repeated
highlights invert the current physical colour pair in native order.

The four-mode snapshot regression checks the inactive field and rejects modified
field pixels or missing drawing evidence while tolerating unrelated drawing
outside its bounds. The multiline regression inserts Return and selects both
lines through guest events; PPC additionally requires overlapping highlight
boxes. These checks qualify a native field recipe, not production ownership,
GPUI multiline compositor output, live pointer alignment, arbitrary backgrounds
or themes. Styled production ownership remains guarded pending those checks.

The whole-field GPUI canvas now paints resolved background and recipe ink with
the same device snapping at fractional presentation scales. The plan retains
its port-local view bounds; the caller supplies the port-to-scene transform
and clipping. Invalid nonpositive/nonfinite scales and nonfinite origins
decline. The production build check passes, and the four-mode snapshot test
constructs the canvas and checks transform rejection. This is not yet rendered
whole-field compositor evidence or enabled production styled ownership.

The styled capture preview now consumes the qualified whole-field plan rather
than merging independent line selection recipes. Qualification occurs against
the original guest frame before its field is erased; the canvas paints the
resolved background and CPU-ordered ink together. The existing `d7c3e82f`
matrix job remains on its original unchanged executable and does not qualify
this newer canvas. New rendered captures are required after that job finishes.

`--capture-styled-text-edit-multiline OUTPUT` exercises the whole-field preview
with Return inserted at guest byte 26 and selection dragged across both lines.
It requires two intact guest lines and overlapping PPC highlight boxes, then
qualifies the native field before composition. Its sidecar marks `multiline`.
The setup follows the passing four-mode native multiline regression. This new
command has not yet been rendered or archived; the running older matrix is
independent and must finish before rebuilding its pinned example executable.

### Remaining standard list typography gap

The current GPUI list row path still uses host text layout (`text_size(13)`,
host centering and padding). `ListManagerSnapshot` includes cells and geometry
but lacks resolved guest font/size, native baseline/inset, physical foreground
and background, and intact per-cell drawing evidence. Standard LDEF identity
alone does not establish faithful replacement. This remains a release gap.

The native implementations cannot be treated as one shared metric recipe:
classic `draw_list_cell_fallback` uses the current guest font, size at least 9,
face zero, a 3-pixel horizontal inset and a guest ascent/descent-centered
baseline. It resolves selected highlight colour and foreground contrast, with
cell/port clipping. PPC `ppc_list_draw` uses the owning port font/size, a 1-pixel
inset, baseline at cell top plus guest ascent, and black/white selected paint,
clipped to the cell and view. Guest LDEF callbacks also need independent
ownership evidence. The next list implementation must capture these CPU-owned
recipes and physical colours at drawing time, qualify pixels, then feed bitmap
glyphs to the shared compositor without changing guest cell hit-testing.

The original corrected styled matrix has independently verified 142 completed
cases; this remains incomplete qualification and does not prove list rendering
or the newer whole-field canvas.

`ClassicListCellLayout` now accepts explicit native font/size, baseline, inset,
clip and optional pre-glyph stopping boundary. Its bitmap recipe rejects
nonzero character spacing and malformed clips. It does not infer host metrics
or establish ownership. A crop/partial-cell regression checks translated guest
ink and the classic stopping boundary. Native frame and compositor evidence
remain required before wiring this into list presentation.

The native byte policy is another release gap: classic fallback
`list_cell_text` stops at NUL, replaces non-ASCII/non-graphic bytes with spaces
and trims trailing spaces; PPC passes the original bytes to its glyph resolver.
The recipe therefore takes the owning painter's actual bytes, rather than the
snapshot's independently decoded label. Preserving accented Mac Roman text
on the classic fallback requires a separately tested native correction, with
font mapping, pen advance and callback-owned cells accounted for.

The classic standard-list fallback now retains high Mac Roman character codes
instead of replacing them with spaces. `draw_char` consumes guest byte-valued
characters through `get_glyph`; decoding those codes to Unicode before this
call would select different glyphs. ASCII control-to-space conversion, NUL
termination and trailing-space trimming retain their prior behavior. The
focused regression compares accented guest glyphs and advances against the
Unicode-to-guest resolver and checks the unchanged ASCII policy. Full native
list frame/callback qualification and GPUI paint ownership remain unfinished.

`ClassicListCellPaintPlan::qualify` checks a complete translated cell against
its original native RGBA buffer, using explicit foreground/background colours
and guest layout. It requires intact drawing evidence and exact view geometry;
any cell pixel mismatch declines. A qualified cell uses the same device-snapped
background/ink canvas as styled TextEdit. Bounds, truncated-buffer, absent
evidence and changed-cell guard tests use a synthetic empty cell; they are not
proof of native list rendering. Actual per-cell paint metadata, lifecycle
evidence, real native captures and compositor checks remain required before
production ownership.

The real-fixture `standard_list_first_row_qualification_preserves_native_cpu_pixels`
regression passes in all four modes. Colour 68k and PPC8/PPC16 first-row
interiors match the explicit application-font-9 native bitmap recipes, using
their distinct insets/baselines. The fixture draws its own `FrameRect` over
the list view: the regression excludes that application-owned top/side border
without moving glyph anchors. Monochrome 68k instead asserts native black
interior pixels and refused replacement, preserving the established one-bit
custom-panel behavior. This is one unselected public-fixture row, not general
list ownership, selected-state/lifecycle qualification or GPUI rendered output.

The real first-row regression also passes after guest mouse-down/up selection
in all four modes, preserving cell bytes and list generation. It samples the
known empty inset for the physical native highlight and uses native contrast
policy, then compares every interior pixel. Monochrome is state-dependent:
the initial black panel declines, but selection repaints a native row whose
text recipe matches. This does not establish ownership from equality alone;
production still needs retained drawing provenance and current paint metadata.
The result qualifies selected native geometry/ink for this fixture, not live
GPUI pointer transforms, arbitrary highlights or full list lifecycle.

### Corrected styled matrix archived

The unchanged `d7c3e82f` capture job completed all 192 cases. Full verification
passed for the original captures and again for the lossless archive at
`tests/toolbox-showcase/reference/gpui-demo/styled-text-qualified/review.json`:
actual mono/colour/PPC8/PPC16 depths, four scales, inactive/selected activation
states and two caret insertion offsets with visible/off/suspended/resumed
phases. Original and archived byte hashes, source hashes and state sidecars are
retained; recompression preserves every RGBA pixel. Four representative visuals
have a separate scoped review record. The invalidated older archives remain
marked invalid; this corrected archive replaces their full-matrix claim.

This qualifies that revision's one-line public fixture raster and guest states
through the shared macOS Metal headless compositor. It does not establish
production ownership, live GPUI pointer mapping, arbitrary fonts/backgrounds
or themes, other platforms, or the newer whole-field canvas. A separate
whole-field multiline run against `716378c4` reached 16/16 terminal passes; its
manifest and images passed independent verification and lossless repository
archival at `reference/gpui-demo/styled-text-multiline/review.json` (under
`tests/toolbox-showcase`). Original and archived byte hashes, sidecars, pinned
source hashes and RGBA equality are retained. Three additional representative
visuals were reviewed; the archive remains isolated-canvas evidence.
The first true PPC16 capture matches every composed field pixel and has been
visually reviewed.

Styled TextEdit visibility candidates now reuse the plain-field window-owner,
standard-definition, control-overlap, guest-visible region, painted-region
and front-window occlusion rules. Plain eligibility retains its existing font
and layout guards. A candidate is not an ownership grant: the whole-field
native paint plan must qualify separately before drawing it. Regression checks
cover partial painted regions, missing drawing evidence, custom windows and
front-window clipping. The live worker now qualifies whole-field styled recipes against the same
RGBA frame that it submits for display, before converting that frame to BGRA.
The live renderer and ordinary composed-capture renderer use the shared
whole-field canvas, standard-owner visibility candidates, and scene scale and
origin. Plans are replaced with each snapshot, so a mismatch or lost drawing
evidence restores guest presentation. The explicit classic white-background
candidate and PPC insertion-style caret candidate must match every native
field pixel; changed themes, unsupported paint and application modifications
remain guest-rendered. Guest event routing and byte offsets are unchanged.
Live pointer, centered-scene, clipping and lifecycle capture qualification for
this integration is still required; prior isolated canvas captures do not
prove those paths.


Styled capture commands now instantiate the same `Demo` renderer as the live
frontend, with standard-window visibility and the native-qualified styled
plan. They erase the native field from the source texture before composing it,
so retained guest pixels cannot hide a missing GPUI field. Sidecars identify
`shared Demo renderer`. A true PPC16 selected multiline field at 1x and a
colour-68k selected field at 0.75x matched every device-snapped native field
pixel; the PPC16 composed image was visually reviewed. These two smoke checks
are not a complete CPU/state/scale matrix. The older isolated-canvas evidence
retains its original scope.

The centered-scene GPUI interaction regression completed all mono/colour/PPC8/
PPC16 and 0.75/1/1.5/2x combinations: two styled insertion clicks and a drag
across style boundaries per combination reach the guest event path and produce
the expected byte selections. A stronger revision additionally asserts visible
standard ownership and refreshes snapshots during held gestures; that revision
passed separately (424.20 seconds). New capture matrices support `--multiline` and
require `shared Demo renderer` sidecars. The verifier retains legacy matrix
scope and rejects missing, duplicate, wrong-depth, wrong-state or wrong-pixel
cases. Neither these checks nor native-pixel equality establish arbitrary guest
font replacement, native host activation observation or release readiness.

The `7c2e238f` shared Demo multiline matrix completed 16/16 cases and passed
independent verification before and after lossless archival at
`tests/toolbox-showcase/reference/gpui-demo/styled-text-multiline-shared/review.json`.
All four display/CPU modes and scales have actual-depth sidecars, original and
archived hashes, and complete field pixel equality. Two representative composed
visuals were reviewed. Native field pixels were erased before rendering. This
establishes the selected two-line fixture in the production renderer, while
inactive/caret/lifecycle matrices in that renderer remain outstanding.

List Manager snapshots now retain actual standard-painter cell inputs on both
CPU paths: draw-time font/size, baseline/inset/clipping/stopping policy, original
and painter-decoded bytes, spacing representation, selection and raster evidence.
Only the built-in resource-zero painter records evidence. Snapshot eligibility
checks the current cell data, geometry, selection, lifetime and backing raster;
new allocations start empty and old visible-cell evidence is pruned on redraw.
The four-mode native row test verifies retained font and baseline metadata before
and after guest selection. Production list rows now use the exact guest cell canvas after retained raster
and complete physical-pixel qualification over every retained region. Application
modifications remain native at their exact pixel locations; unchanged regions
are coalesced vertically, with complex fragmented custom drawing declining.
Uniform ink/background candidates
come from the native standard draw and must match every retained pixel; host theme
or text sizing no longer substitutes list typography. Unsupported spacing,
patterns, missing evidence, modified cells and custom LDEFs retain guest pixels.
Standard cell accessibility labels and selection semantics remain exposed even
when paint declines; guest LClick still owns interaction. Canvas clips combine
list visibility, cell bounds and the native painted regions. Full live/headless
list capture, scrolling, activation, lifecycle and scale matrices remain required.

The production list qualification regression now passes all four modes after
native guest selection and requires at least one retained cell plan. The exact
unchanged-region primitive preserves every unaffected pixel at 1/2/4/8/16/32-bit
depths and excludes the modified pixel. List evidence guards pass changes to
raster, bytes, selection, scroll anchor, custom definition and reused lifetime.
The existing GPUI row click test passes with native fallback and retained
accessibility semantics. These tests do not replace composed list image or
performance qualification.

The shared Demo selected-list matrix now passes all 16 mono/colour/PPC8/PPC16
and 0.75/1/1.5/2x combinations, with actual retained backing-paint depth in
each sidecar. The generic capture path explicitly applies requested PPC depth;
PPC16 uses the CLI architecture default because its indexed-depth argument
accepts only 1/2/4/8. Qualified visible regions are erased to magenta before
composition, so original guest glyphs cannot conceal absent GPUI list paint.
The first fractional-scale capture exposed row-boundary scissor seams; list
clips now share the canvas's global device-edge rounding, including centered
origins. Every erased owned device pixel matches native guest paint. Magenta
sampling at unowned custom borders is a capture-mask artifact outside that
comparison, not qualified replacement of those borders.

The lossless archive is `reference/gpui-demo/list-text-shared/review.json` under
`tests/toolbox-showcase`; original and archived hashes and dirty source hashes
are retained. All archived cases passed `verify-gpui-list-text.py` again; a
changed owned pixel and wrong actual-depth record were independently rejected.
The PPC16 0.75x image was visually reviewed. The production check and example
build pass. This establishes selected standard-list fixture paint only;
inactive, scrolling, lifecycle, interaction matrices and release qualification
remain outstanding.

The shared Demo list transition matrix completed 48/48 combinations: all four
CPU/display modes and scales, with guest-button scrolling, LActivate(false)
and LActivate(true) after deactivation. Captures assert retained row-7 selection,
first visible row 4 after scrolling, actual backing paint depth, active state
and stable list lifetime metadata. Every erased owned device pixel matches the
native frame. This is guest-button/session fixture state and composed paint
evidence; it does not independently prove GPUI click targets, native host
activation observation or general list lifecycle behavior.

Reusable list capture, matrix verification and lossless archival scripts are
documented in `tests/toolbox-showcase/README.md`. The matrix verifier rejects
missing/duplicate combinations, changed file hashes and incorrect depth; the
pixel verifier rejects changed active, selection and scroll metadata. The
archive tool passed a complete selected-matrix round trip. Monochrome scrolled
custom black panels were reviewed against their native source and retained.
Typography remains the canonical guest glyph canvas pending the user's explicit
smooth-versus-bitmap presentation preference; pixel equality is evidence for
this bitmap path, not a qualification of future host typography.

The loaded-guest GPUI list click regression passed in 78.36 seconds. It renders
the shared Demo with fresh native frames, list snapshots and qualified row plans
before mouse down, dispatches GPUI mouse down/up at guest glyph coordinates
through the centered scene transform, and delivers exactly one ordinary guest
input for each event. Both row 0 and row 7 become the sole guest selection at
0.75/1/1.5/2x across mono/colour 68k and PPC8/PPC16. The same ListHandle and
generation survive each click. This establishes mock-GPUI pointer translation
for the loaded standard-list fixture; it does not establish physical host
capture, complex clipping, inactive clicks, scrolling gestures or full lifecycle
behavior. Pixel rendering is separately supported by the source-masked matrices.

List lifecycle capture work in progress: `--capture-list-transition mutated|resized` now routes through the fixture guest buttons and records cell bytes and view bounds. `capture-gpui-list-matrix.py --lifecycle` requests 32 CPU/depth/scale cases. Compilation and Python syntax are checked; these new cases have not yet been captured or qualified. The ongoing styled-field matrix remains pinned to f565b695 and its original binary. Disposal and identity reuse remain separate unfinished lifecycle requirements.

Guest list disposal snapshot qualification: `cargo test --locked --example gpui-menu-demo --features gpui-demo-test guest_quit_removes_list_presentation_on_all_display_modes -- --nocapture` passes for mono 68k, colour 68k, PPC8 and PPC16. It opens Lists, requires retained standard paint and the visible list scrollbar, dispatches File → Quit through the guest menu path, and asserts removal of list and that scrollbar identity. Hidden controls from other pages are outside this check. This verifies frontend snapshot cleanup, not a rendered post-disposal frame, in-process handle reuse, or custom LDEF disposal callbacks. The five List Manager unit tests also pass, including rejection of old generation paint evidence.

Guest list mutation/resize paint qualification: `guest_list_mutation_and_resize_refresh_qualified_paint` passes in mono 68k, colour 68k, PPC8 and PPC16 (8.68 s). It selects row zero through guest mouse events, clicks Update Selected Row, shrinks and restores via Resize List, checks exact appended bytes, retained handle/generation/owner and selection, and requires the updated row to pass production native-frame paint qualification after each change. This does not replace the pending 32 composed lifecycle captures or qualify arbitrary custom LDEFs, clipping, host clicks, or in-process handle reuse.

Standard File replacement message migration: the Replace existing filename prompt now uses the shared wrapped guest prompt painter (system font 0/12, baseline 12, line advance 16) inside the existing message rectangle, replacing inherited host text shaping. This matches the PPC dialog text recipe; guest Cancel/Replace routing is unchanged. Example check and `themed_standard_file_actions_forward_guest_clicks` pass (0.79 s). CPU/scale replacement-message raster captures and long-name wrapping parity remain pending.

Smooth typography candidate: the shared plain label, wrapped prompt and Standard File row canvases now request antialiased coverage from the already resolved outline source at scene/device resolution. They retain guest advances, baselines, wrapping and clipping; missing/scaled/bitmap sources and unsupported styles retain the binary path. This candidate is not visually or performance qualified. The pinned f565b695 capture matrix still checks the previous bitmap binary. Exact binary guest-pixel verifiers do not qualify these changed outline edges; smooth rendering requires separate visual and interaction evidence. The depth accessor regression passed all four display modes (13.29 s); replacement capture tooling remains unqualified.

Smooth candidate validation: the example check passes and all 13 `text::tests` pass (8.17 s), including outline fractional coverage and unchanged guest advances for font 0/12, font 1/9 and font 3/12 at raster scales 1–4. These tests do not establish composed smooth glyph appearance, all styles, application-specific resource fonts, pointer parity or acceptable quad cost. Window titles, editable fields and retained whole-field/list paint still need smooth-path integration.

Smooth window-title candidate now preserves the WDEF baseline and title clipping while using resolved outline coverage where available. Coverage spans of equal alpha share GPUI paths, addressing the previous small-quad risk without snapping back to a guest-pixel bitmap. The example check passes; title drag regression and visual/performance qualification remain pending. Plain and styled editable fields still require their own smooth selection/caret integration.

Smooth plain editable text candidate: plain document lines and Save filenames now use the resolved outline painter while retaining guest selection, caret, baseline, wrapping and input geometry. Source pens must match the current guest insertion positions or the binary path remains active. The title-drag GPUI regression passes (0.18 s); the four-scale document click regression is running against this candidate. Composed active/inactive selection/caret appearance, other fields, styles and performance remain unqualified.

Smooth editable-field candidate now also covers the New Folder name and recognized single-line dialog fields, preserving selection/caret paint order and existing scroll/input geometry. The example check passes. Their interaction and composed visual qualification remain pending; the plain document click test is still running. Bitmap-only sources and unsupported styled/list recipes retain the guest-compatible binary fallback.

New Folder smooth interaction qualification now explicitly runs mono 68k, colour 68k, PPC8 and PPC16 at scales 0.75, 1, 1.5 and 2, with short and horizontally scrolled names, clicks, held drags and caret visibility changes. This expanded test is running; no passing result is yet established. The candidate retains the existing guest insertion-position checks.

Expanded New Folder test correction: its initial run failed at the requested-scale assertion because centered mock-display bounds clamped the new 2x viewport. The test now uses explicit bounds, matching the document test, and has been restarted after that terminal failure. No guest selection failure was observed in that run; the corrected run remains pending. Styled whole-field paint still uses binary native run recipes with ordered selection inversion; migrating it requires preserving synthetic styles, CPU-specific paint advances and inverted selection colors.

Plain document interaction matrix correction: source inspection found its existing PPC case used an implicit depth. The test now specifies PPC8 and PPC16 independently, alongside mono/colour 68k and all four scales. The currently running invocation predates this expansion and cannot establish the expanded matrix; rerun after it terminates.

Smooth plain document click regression passed its original three-mode/four-scale invocation (331.96 s), using actual GPUI events and guest selection assertions. The expanded explicit PPC8/PPC16 invocation is now running; it is separate pending evidence. No composed smooth appearance or performance claim follows from this interaction pass.

Smooth Mac Roman regression passes (0.18 s): font 0/12, font 1/9 and font 3/12, raster scales 1–4, now include accented and symbol bytes. Decoded Unicode and original guest bytes retain identical insertion positions and binary reference ink, while resolved masks contain fractional edge coverage. This checks character mapping and metrics, not composed appearance or arbitrary resource-font fidelity.

Composed fixture capture provenance: generic shared Demo captures now also write `.capture.json` with actual guest depth, requested and rendered scene scales, scene origin, guest/composed dimensions, capture case and the current smooth/binary policy. Existing specialized `.json` evidence remains intact. Example check passes; producing and reviewing smooth composed captures is pending the pinned older-binary run. Metadata alone establishes no visual, font-fidelity or performance qualification.

Expanded interaction depth correction: both document and New Folder runs terminated at PPC16 construction because the classic constructor accepts only indexed depths. The tests now explicitly configure native PPC depth through `set_powerpc_screen_depth`, use the indexed-compatible constructor, and assert actual presented depth after guest settling. These failed runs do not qualify PPC16; corrected runs are pending.

Smooth candidate visual review: colour 68k Save edited at scale 1 and PPC16 New Folder selected at scale 0.75 were rendered and inspected through the shared Demo compositor. Plain system labels, titles, buttons and editable text appear smooth; colour Standard File bitmap-source rows retain binary rendering. PPC16 metadata confirms actual depth16 and rendered scale0.75. These two samples do not qualify the full CPU/scale/state matrix, exact selection/clipping, font fidelity or performance.

Reviewed smooth samples are archived with native frames, geometry/depth sidecars, source commit, executable hash and unchanged RGBA pixels in `smooth-plain-text-samples/review.json`. The third sample covers mono 68k New Folder selected at scale2. These three samples remain partial visual evidence; the binary bitmap-source rows and styled/list recipes are explicit remaining work.

Corrected smooth document interaction matrix passes (370.47 s): explicit actual-depth assertions cover mono 68k, colour 68k, PPC8 and PPC16, with GPUI clicks/drags at scales0.75,1,1.5,2 and guest selection assertions. New Folder remains running. Open popup menu rows now also attempt the resolved outline painter for their label, checkmark and command text, retaining binary glyph fallback and the existing guest hierarchy indicator/row geometry. Popup smooth appearance and interaction qualification remain pending.

Popup smoothing checks: all 13 text tests pass (5.57 s), and the hierarchical menu keyboard selection/focus restoration regression passes (0.77 s). These checks do not qualify the guest-tracked popup pointer path or composed smooth popup appearance; its capture is next.

Smooth open guest popup PPC8/scale0.75 capture reviewed and archived in `smooth-popup-sample/review.json`: selected long label, normal labels and disabled row appear smooth. The pointer test now explicitly configures/ asserts all four framebuffer modes and is running; its passing result remains pending. Closed bitmap-source labels still retain binary fallback; one visual sample is not full qualification.

Corrected smooth New Folder interaction matrix passes (522.73 s): mono/colour68k, PPC8/PPC16, scales0.75/1/1.5/2, explicit actual framebuffer assertions, short and horizontally scrolled names, GPUI clicks/held drags and guest selection/caret scrolling behavior. This interaction pass does not establish full smooth clipping/visual parity, arbitrary fonts/styles, lifecycle or performance.

Smooth standard-list candidate: qualified cell plans now retain a resolved glyph line, native left/baseline and physical foreground color. The renderer uses outline coverage after painting the qualified background, inside existing owned-region scissors; stop-before preserves the final glyph that starts inside the boundary. Unsupported sources keep the binary canvas. Native ownership qualification remains unchanged. Text and CPU cell qualification tests are running; composed smooth list selection/clipping, lifecycle and performance remain pending.

Smooth list candidate checks: all13 text regressions pass (5.47 s); the native first-row qualification regression passes in mono/colour68k/PPC8/PPC16 (11.47 s). These checks preserve ownership/reference recipe evidence but do not qualify changed smooth composed pixels.

Smooth list composed review (PPC16/scale0.75): normal and selected row text appears smooth, but thin magenta ownership-mask seams remain at right/lower boundaries. This is an unresolved composed clipping defect, not qualified visual parity. The source/binary hashes and reviewed image are recorded in the temporary smooth list review; centered-scene pointer regression is running.

Smooth list mask-edge analysis correction: the PPC16/0.75 diagnostic image contains 1010 magenta-tinted output pixels, all outside the snapped owned-region projection. None of the 167319 owned device pixels contains magenta tint. The visible seam is diagnostic erased-source texture sampling outside the owner scissor, not demonstrated uncovered owned paint. This does not establish full smooth visual parity; a retained-native-source capture is needed to inspect normal presentation without painting outside guest ownership.

Smooth popup pointer matrix passes (286.39 s), explicitly configured mono/colour68k/PPC8/PPC16 with presented-depth assertions and scales0.75/1/1.5/2. Retained-source list capture tooling builds: `--capture-lists-retain-native-source` with selected capture leaves native source pixels intact and explicitly marks appearance-only evidence; the existing erased-source verifier rejects it. First normal-source selected list capture is running; no visual result is yet established.

Smooth standard-list interaction matrix passes (185.44 s): actual GPUI clicks at centered scene scales0.75/1/1.5/2 select rows0/7 through guest events in mono68k, colour68k, PPC8 and PPC16. The final example check passes (4.16 s). Diagnostic and retained-native-source PPC16/0.75 captures are reviewed and archived in `smooth-list-samples/review.json`: normal and selected labels appear smooth; retained source shows no diagnostic magenta seams. Retained source is appearance-only evidence and can conceal missing replacement paint. The diagnostic image has zero magenta pixels inside 167319 projected owned device pixels. These two samples do not establish a full CPU/scale/state visual matrix, lifecycle or performance qualification.

Smooth selected-list shared matrix: all16 retained-source captures completed at source4f6da9ee, covering mono68k/colour68k/PPC8/PPC16 at scales0.75/1/1.5/2. `verify-gpui-smooth-list-provenance.py` checks hashes, actual framebuffer/paint depths, composed dimensions, scene geometry, native owner regions and guest row7 selection; all16 pass. Altered depth, scale and image hash are rejected. Captures are archived losslessly in `smooth-list-selected-shared` (decoded RGBA unchanged). Five images were visually reviewed, explicitly listed in `review.json`; they show smooth normal/selected text and existing native lower view clipping. Retained source can conceal missing replacement paint. This establishes provenance/geometry and sampled appearance, not full font fidelity, all-state visual parity, custom definitions, lifecycle, host events or performance.

Smooth list state capture extension: retained-source mode now covers scrolled/inactive/reactivated/mutated/resized through the existing guest-button fixture path. Shared renderer metadata preserves the explicit source-retained case; native transition assertions and owner qualification are unchanged. The example build and the archived selected16 provenance regression pass. Transition/lifecycle captures and visual review are next; this tooling does not establish their smooth appearance or production qualification.

Smooth transition provenance correction: scrolled mono captures have visible range rows4..13 but only stored cells0..11. Ownership must cover the intersection of stored cells and the visible rectangle, rather than invent row12. The checker now validates that intersection; completed transition cases and the archived selected16 matrix pass. Negative checks reject a missing stored visible owner and an owner for nonexistent row12. The48-case transition capture remains live; no complete matrix or inactive appearance result is yet established. Only verifier/documentation changed during capture, leaving its pinned renderer/binary/fixture hashes intact.

Smooth list archive tooling: the reusable archive script validates a complete matrix before and after lossless PNG compression and preserves explicit visual review provenance. A complete archived selected16 matrix was rearchived to a fresh temporary directory: all images retained decoded RGBA and both provenance checks passed. The separate48-state capture remains live with the renderer/binary unchanged; no complete transition matrix claim follows.

Smooth list transition matrix completed at pinned renderer sourcec9ce4559: all48 scrolled/inactive/reactivated captures cover mono68k, colour68k, PPC8/PPC16 at scales0.75/1/1.5/2. The corrected provenance checker verifies actual depths, scene dimensions/scale, retained-source policy, active state, row7 selection and stored visible ownership in all48 cases. Lossless archive `smooth-list-transitions-shared` passes before/after checks and preserves decoded RGBA. Twelve explicit images were visually reviewed across all four modes and three states; their source/archive hashes and findings are in `review.json`. A colour68k0.75 active/inactive comparison preserves exact RGBA in all unchanged visible cells. This is sampled smooth appearance and fixture state/geometry evidence; retained source can conceal replacement gaps. It does not establish all glyph/style fidelity, host events, custom definitions, mutation/disposal/identity reuse, real-game performance or release readiness.

Smooth list mutation/resize matrix completed at source060af1aa: all32 cases cover mono68k/colour68k/PPC8/PPC16 and scales0.75/1/1.5/2. Native guest-button assertions retain identity, selection and updated bytes or reduced bounds. Smooth provenance checks pass before/after lossless archive in `smooth-list-lifecycle-shared`, preserving decoded RGBA. Eight images were visually reviewed (one of each state/mode), explicitly recorded in `review.json`. Colour68k0.75 resize preserves exact pixels in full rows0..5, but partial row6 differs: the classic native painter centers its baseline using the clipped cell rectangle (`draw_list_cell_fallback`), whereas PPC uses top+ascent. GPUI retains the captured native baseline; no cross-state equality is claimed for that partial row. Retained source can conceal missing replacement. This fixture evidence does not establish arbitrary custom definitions, disposal/identity reuse/callbacks, all fonts/styles, physical host events, production performance or release readiness. Styled TextEdit smoothing remains unfinished.

Smooth closed popup-control label candidate: solid physical-ink labels and titles now attempt the shared resolved-outline painter at the existing guest CDEF baseline/origin, preserving truncation, title alignment and bounds. Unsupported bitmap sources and disabled checker-pattern ink retain the original recipe. All13 text regressions pass (4.54 s). The explicit four-framebuffer/four-scale actual GPUI popup pointer regression is running; no new passing result or composed closed-control visual qualification is yet established. Styled TextEdit smoothing remains unfinished.

Smooth closed-popup samples archived at sourcec89b3391 in `smooth-closed-popup-samples`: selected PPC16/scale0.75, disabled PPC16/scale0.75, selected colour68k/scale1 and mono68k/scale2. All four were visually reviewed; resolved labels/titles appear smooth with guest truncation/alignment retained. Actual depths/scales are checked from shared compositor metadata, and PNG recompression preserves decoded RGBA. Disabled PPC uses native Solid palette frame_dark ink (`ppc_popup_text_ink`); Checker is monochrome68k disabled policy. Similar PPC selected/disabled appearance is not by itself evidence of a new GPUI defect. No disabled mono capture or full CPU/scale/state/fidelity/performance qualification follows. The actual GPUI popup pointer matrix remains running.

Closed-popup pointer qualification passes after label smoothing: actual GPUI tracker matrix across mono/colour68k/PPC8/PPC16 and four scales passed in277.21 s. Styled TextEdit source integration now retains exact resolved glyph sources for native plain runs at unscaled strikes, using native paint pens separately from measured insertion positions. Nonplain/synthetic or ratio-scaled runs still decline these sources. All13 text tests pass (4.44 s), including every128 style mask binary recipe/insertion-position regression and source identity/paint-pen assertions. This is a necessary source stage only: whole-field styled presentation remains binary pending smooth style coverage and ordered selection/caret composition; no styled smoothing appearance or production qualification is claimed.

Styled TextEdit outline preflight stage: qualified whole-field plans retain native ordered run, selection inversion and caret operations. PPC per-line inversion and classic post-line inversion remain distinct. Resolution collects every run before any future smooth field painting, preserving native glyph paint pens independently of guest insertion positions; unsupported sources decline the whole field. All14 text regressions pass (4.44 s), including mixed supported/unsupported atomic rejection, raster limits and operation/painter-origin preservation. Existing field presentation remains binary: antialiased physical ink-pair composition, synthetic style coverage, full shared visual/interaction qualification and performance remain unfinished. No new styled smooth rendering claim follows.

Smooth styled TextEdit composition candidate: the shared whole-field canvas now resolves all runs before selecting smooth paint, composes antialiased glyph coverage with native normal/inverted physical ink pairs, clips in guest-coordinate subpixels, preserves PPC per-line versus classic post-line inversion order, and paints solid carets last. Native measured insertion coordinates and paint pens are unchanged. Any unsupported source/style/raster keeps the complete qualified binary field recipe. All15 text tests pass (4.64 s); new coverage checks non-complementary indexed colours, before/after and repeated inversion, crop equality, fractional coverage and caret overwrite at raster1..4. The example build passes (5.81 s). Synthetic faces and ratio-scaled strikes still lack smooth sources, so existing mixed-style showcase fields continue to fall back. No new shared smooth-field capture, actual interaction or performance qualification is established; those and full style/font coverage remain unfinished.

Smooth TextEdit bold candidate: classic and PPC face1 runs retain their original resolved font sources and native paint pens; outline coverage uses max(original, one guest pixel rightward), matching both native bold recipes without a host bold-face substitution. Guest insertion coordinates, binary oracle ink and paint advances remain unchanged. All16 text tests pass (5.84 s), including every128 style mask native binary recipe regressions and per-subpixel bold coverage/advance assertions for both CPU recipes at raster1..4. The example build passes (11.76 s). Other synthetic styles, scaled strikes and bitmap-only sources still decline whole-field smoothing. The actual shared pointer matrix launched against source6c188e57 remains live at the last authoritative session poll; its result is not yet established and it does not qualify this later bold candidate. Smooth composed captures and performance remain unfinished.

Smooth TextEdit italic candidate: classic/PPC face2 and face3 sources now retain the native upright outline and apply the shared baseline/descent row shear before optional one-pixel bold synthesis. The resolver currently supplies no separate italic strikes; if one becomes available, this candidate declines that glyph rather than replacing its native face. Native binary ink, insertion coordinates and paint pens remain unchanged. All17 text regressions pass (5.31 s), including per-subpixel shear/combined-bold coverage for both CPU recipes at raster1..4 and all128 native style recipes; the example build passes (5.79 s). Underline/outline/shadow, ratio-scaled strikes, bitmap sources and shared visual/performance qualification remain unfinished. The earlier source6c188e57 actual pointer test remains live at the authoritative session poll; no passing result or later italic interaction qualification is claimed.

Smooth TextEdit underline candidate: basic faces0..7 now resolve outline glyphs plus separate solid native underline strokes. Classic per-character strokes share the existing native descender predicate and retain italic extension; PPC keeps its continuous run ribbon. Explicit solid coverage avoids making glyph/underline overlaps translucent. All18 text tests pass (5.62 s), including normal/bold/italic underline native-ink union, insertion/advance preservation and subpixel solid-stroke equality on both CPU recipes at raster1..4. The native classic coverage library regression is still compiling at the last authoritative session poll; no passing result for that command or new composed underline capture is claimed. Outline/shadow, scaled strikes, bitmap-only sources, smooth visual/interaction and performance qualification remain unfinished.

The earlier actual shared `styled_document_glyph_clicks_reach_guest_at_centered_scene_scales` matrix completed successfully (394.47 s) against source6c188e57, exercising mono68k/colour68k/PPC8/PPC16 and scales0.75/1/1.5/2 with guest pointer assertions. It does not qualify later bold/italic/underline candidates or prove smooth replacement in mixed-style fields that still use fallback.

Smooth TextEdit outline/shadow candidate: non-underlined basic outline/shadow combinations now apply native (-1,-1)..radius smear/exclusion to the original resolved outline after italic/bold synthesis. Both native CPU recipes match binary-input silhouettes and guest advances for W/g at font3/12 across faces8..11,16..19,24..27; antialiased outline inputs retain fractional boundaries at raster1..4. All19 text tests pass (4.79 s), including all128 native binary recipes and native PPC source-paint advance checks; the example build passes (10.63 s). An initial regression caught missing new PPC effect spacing, corrected before this passing result. The prior native classic continuous-underline library test also completed successfully (0.02 s). Underline/halo interaction, condensed/extended styles, scaled strikes, bitmap-only fonts, composed smooth-field review, later interaction tests and performance remain unfinished. This candidate does not establish full font fidelity or production readiness.

Smooth styled inactive samples: the existing shared whole-field capture now records native style runs and outline preflight support for raster1..8. The showcase actually uses faces0/1/2/4 at Geneva10/12 and Monaco10/14, and all four sampled CPU/depth modes resolve all its runs. Four inactive single-line captures at scene scale0.75 are visually reviewed and archived in `smooth-styled-inactive-samples` with native guest images, renderer/binary/fixture hashes and explicit review limits. Full native source-view erasure follows successful native whole-field qualification in the existing capture path; composed mixed text appears smooth while application-drawn panel labels retain guest pixels. Mono custom black panels remain native. Depths1/8/8/16 and1200x900 composed/800x600 guest dimensions are checked; all eight PNGs preserve decoded RGBA through lossless archival. Device density2 is inferred from dimensions, not a host observer measurement. The explicit PPC16 depth CLI attempt was rejected (classic-depth validation); only the missing case was retried using the existing implicit PPC16 default. All19 text regressions pass (7.27 s), and the example build passes (9.80 s). This proves sampled inactive appearance and preflight diagnostics; active selection/caret, multiline, full scales/styles/font fidelity, host interaction, lifecycle, performance and the overall release goal remain unfinished.

Smooth styled matrix capture extension: `capture-gpui-styled-text-matrix.py --smooth-review` uses the existing shared 192-case single-line or16-case multiline guest state paths, records actual depths, preflight coverage, source/binary/fixture hashes, and checks scene dimensions, view/selection/caret/activation state. Binary-ink oracle verification remains the default; smooth mode explicitly skips it and requires composed visual review. Renderer/font/capture inputs are pinned before and after each case. Python syntax and CLI discovery pass; full smooth matrices have not yet completed and no all-state visual or interaction qualification is claimed by this tooling.

### Smooth styled multiline capture evidence (2026-10-10)

`reference/gpui-demo/smooth-styled-multiline-shared` archives all 16 selected, two-line mixed TextEdit fixture captures from source `f69b47ef`: monochrome 68k, colour 68k, PPC8 and PPC16 at scene scales 0.75, 1, 1.5 and 2 through the shared Demo compositor. Every composed image was visually reviewed; larger previews were resized during inspection, as recorded in `review.json`. The owned field has smooth original-font outlines with native selection colours, wrapping, underline and view bounds; application-drawn labels remain guest pixels.

The independent styled provenance verifier checks the complete case set, source/image hashes, native depth, selection/activation state, geometry and smooth raster preflight. Ten altered-evidence cases are rejected, including incorrect Boolean types. The archiver validates before and after lossless PNG recompression and checks identical decoded RGBA pixels and dimensions. These checks do not constitute a glyph appearance oracle.

This evidence covers the selected multiline fixture only. The 192-case single-line activation, selection and caret matrix is still running against the same pinned source. All fonts/styles, multiline editing interactions, physical host integration and performance remain unqualified by this capture matrix.

Smooth styled pointer regression on source `b4d1f7c5` passed (1 test, 583.93 s): `styled_document_glyph_clicks_reach_guest_at_centered_scene_scales` drives actual shared GPUI window click offsets 3/20 and held selection drag 9..26 through guest events in mono68k, colour68k, PPC8/PPC16 at scales 0.75/1/1.5/2. The test requires a visible owned-field candidate and asserts native guest selection offsets. Its log and source/test-binary/fixture hashes are archived in `tests/toolbox-showcase/reference/gpui-demo/smooth-styled-pointer`. It does not independently assert smooth replacement on every event, establish image appearance, multiline editing, host integration, lifecycle or performance. The state capture matrix ran concurrently; duration is not a benchmark.

Smooth styled state matrix completed at pinned source `f69b47ef`: all192 cases cover mono68k, colour68k, PPC8/PPC16 at scene scales0.75/1/1.5/2, inactive fields, selected/suspended/resumed selections, and insertion offsets0/26 with visible/blink-off/suspended/resumed carets. Independent verification checks complete case membership, original-font/style intent, actual depth, activation, selection, caret state, geometry, image/source/binary/fixture hashes and smooth preflight at raster1..8. The fixture uses Geneva10/12 and Monaco10/14 with faces0/1/2/4. Native whole-field qualification precedes erasure of the original source view, so retained native field pixels cannot conceal replacement gaps.

The lossless archive `tests/toolbox-showcase/reference/gpui-demo/smooth-styled-states-shared` passes verification before and after archival; all384 PNGs preserve decoded RGBA and dimensions, sidecars remain unchanged, and original capture hashes are retained separately. Nineteen altered-evidence cases are rejected by the current verifier, including font identity/size/style/boundaries, Boolean types and caret phase/insertion. Thirty-two explicitly listed composed images were visually reviewed; the remaining160 were not. Larger previews were resized during inspection. Device density2 is inferred from dimensions. This completes fixture provenance/state/geometry coverage and sampled appearance, not a glyph appearance oracle, all-font/style fidelity, multiline editing, physical host integration, lifecycle, performance or the overall release goal.

The strengthened multiline selection regression passes (63.67 s) in mono68k, colour68k and PPC8/PPC16. It requires original-font smooth resolution at raster1..8 for the selected two-line field, then replaces the complete selection with X and deletes X through guest key events. Both edits must settle with the expected bytes and insertion offsets and retain native whole-field paint qualification and smooth resolution. Source/test-binary/fixture/log hashes are archived in `smooth-styled-multiline-edit`; the source was dirty with the recorded test additions. This covers guest editing and resolution, not GPUI pointer dispatch, all editing/scrolling/style inheritance, a glyph appearance oracle, host integration or performance.

The stricter actual GPUI pointer matrix passes (370.21 s): every click/held-drag phase now requires the interacted field's native paint qualification and original-outline resolution at raster1..8, in all four CPU/depth modes at scales0.75/1/1.5/2. Guest selection assertions remain authoritative. `smooth-styled-pointer-required` records the log and reconstructed compilation-source hash; the running binary was replaced by later test compilation before hashing, so binary provenance is explicitly unavailable. This establishes fixture input and resolution, not composed appearance, physical host events or performance; concurrent tests make duration unsuitable as a benchmark.

Basic styled menu/label smoothing now retains original resolved glyph sources for faces0..7, applies native one-pixel bold and row-shear italic, and paints the existing continuous underline as solid strokes. Native binary ink and insertion/paint advances remain unchanged; menu bearing normalization shifts glyph sources and underline strokes together. Unsupported faces8..127 or unavailable sources retain atomic bitmap fallback. All20 text regressions pass (5.90 s), including source-pen and continuous-underline checks at raster1..8 for system/Geneva/Monaco fonts, existing native all-style ink/layout checks, and unsupported-run atomic rejection. The latter fixture now uses outline rather than newly supported bold. This is source/style integration evidence; styled menu composed visual/state/interaction and performance qualification remain pending.

Standard menu capture now honors `--capture-scale`, explicitly selects PPC8 or PPC16, and writes actual framebuffer depth, item text/style, scene scale/origin and viewport metadata. The example build passes (6.96 s). Four plain Pages-menu samples are archived in `smooth-menu-capture-samples`: mono68k0.75, colour68k1, PPC8 at2 and PPC16 at0.75. Actual depth, scale, zero origin, viewport, composed dimensions and all16 plain item styles are checked; lossless archival preserves decoded RGBA. All four images were visually reviewed, with the PPC8/2 preview resized to1824x1368. Menu text appears smooth and application-drawn labels retain guest pixels. Source/binary/fixture and original/archive image hashes are recorded. This fixture has no styled menu items, so these samples do not qualify the new basic styled faces, a full state/scale matrix, physical host events or performance. Density2 is inferred from composed dimensions.

Guest-styled menu fixture derivation now uses the existing packer dependency:
`cargo run --locked --manifest-path tests/toolbox-showcase/packer/Cargo.toml --bin styled-menu-fixture -- tests/toolbox-showcase/toolbox-showcase.sit /tmp/styled-menu.sit`.
Only fourteen style bytes change in MENU129, producing faces0..7 twice; executable forks, labels, item numbers/actions, other resource bytes and archive metadata are preserved and checked. Round-trip fork/metadata assertions pass and repeat derivation is byte-identical. The original packer remains the default binary.

Four actual guest-loaded styled menu samples are archived losslessly in `guest-styled-menu-samples`: mono68k0.75, colour68k1.5, PPC8/2 and PPC16/1. Their actual depth, scene geometry, face sequence, image/source/binary/base/derived-fixture hashes and original-font resolution at raster1..8 are checked. All four were visually reviewed, with larger previews resized. Bold, italic and continuous underline intent remain visible, but native integer row-shear synthesis leaves coarse italic steps; full crisp styled appearance remains unfinished. Source preflight alone is not a paint oracle. Removing the painter's equality check between translated paint pens and insertion positions does not change these sample pixels, so it is not claimed to fix their appearance. All20 text regressions pass (5.85 s); the diagnostics example build passes (6.66 s). Style mutation, selection/disabled states, callbacks/lifecycle, all CPU/scale combinations, native font fidelity, physical host events and performance remain unqualified by these samples.

Continuous display-resolution italic synthesis now replaces the higher-raster stair-step shear with the same native half-pixel slope and baseline/descent pivot, clamped to the existing native envelope. Half-device-pixel translations distribute coverage between adjacent samples. Raster1 retains exact native row placement; guest bitmap rendering, source font, insertion/paint advances, baseline, height and underline strokes remain unchanged. This is an explicit smooth presentation policy, not substitution of a host italic font or a claim of identical Apple bitmap appearance at higher resolution.

All21 text regressions pass (6.24 s). The new opaque-bar test at raster2..8 requires fractional edges, unchanged native envelope/advance, conserved row coverage and no multi-pixel row jumps; CPU-specific styled tests retain exact raster1 masks and validate layout/coverage at higher resolution. The example build passes (7.54 s). Four reviewed captures in `continuous-italic-menu-samples` show smoother italic edges across mono68k0.75, colour68k1.5, PPC8/2 and PPC16/1. Lossless archival preserves RGBA, and metadata exactly matches the earlier samples. In PPC16/1, all8529 changed pixels lie inside the eight explicitly reviewed italic item bands; every other pixel is unchanged. Larger previews were resized. Source/binary/fixture/log/image hashes are recorded. Broader font/style fidelity, TextEdit composed state recaptures, interaction, lifecycle and performance qualification remain unfinished; earlier smooth-field captures document the previous renderer.

The multiline guest editing regression passes against source9fee4404 (99.83 s), rechecking two-line selection, replacement and deletion, native paint qualification and original-font resolution at raster1..8 in mono68k, colour68k and PPC8/PPC16. Source/test-binary/fixture hashes were recorded before execution; the completed log and result are archived in `continuous-italic-multiline-edit`. Concurrent pointer/capture work makes duration unsuitable as a performance result. The current-source16-case multiline capture and actual GPUI pointer matrix remain running; their complete results are not yet established. The consolidated [typography policy](GPUI_TYPOGRAPHY.md) explicitly describes authentic-font precedence, bitmap-only fidelity and the continuous display-resolution italic policy.

Current continuous-italic qualification completed at pinned source9fee4404: all16 selected multiline captures cover mono68k, colour68k, PPC8/PPC16 at scene scales0.75/1/1.5/2. All16 composed images were visually reviewed; larger previews were resized. Original-font italic, second-line underline, selection colours and view bounds remain visible. The archive `continuous-italic-multiline-shared` passes independent provenance/state/geometry verification before and after lossless archival; all32 PNGs retain decoded RGBA and dimensions. Eighteen altered-evidence cases are rejected, now including requested CPU, depth and scale command arguments. Actual framebuffer depth is independently checked; these existing sidecars do not independently record runtime CPU identity. This covers this selected fixture, not all activation/caret states, a native glyph oracle, all fonts/styles, physical host integration or performance.

The stricter actual GPUI pointer matrix also passes against source9fee4404 (490.99 s): click and held-drag phases require native field qualification and original-font resolution at raster1..8, with authoritative guest selection offsets across all four CPU/depth modes and four scales. Pre-execution source/test-binary/fixture hashes and the completed log are archived in `continuous-italic-pointer-required`. Concurrent captures make duration unsuitable as a benchmark. Physical host events, broader lifecycle and production performance remain unfinished.

Styled capture evidence now asserts the active application's actual runner CPU against the requested CPU and records `runtime_powerpc` in each sidecar. Fresh matrix manifests require this runtime evidence; the independent verifier rejects changed, missing and non-Boolean runtime flags or invalid evidence policy. Older archives retain their explicitly weaker provenance. The example build passes (12.82 s). The current continuous-italic192-state matrix is running from pinned source39fbd0cf; partial state/provenance checks and26 altered-evidence rejection checks pass, including caret cases. Five explicitly listed PPC16 scale1 images have been reviewed for selection selected/suspended/resumed and caret0 visible/blink-off; no complete matrix or full appearance result is claimed yet.

The existing nested-modal keyboard regression passes against source39fbd0cf (3.79 s). Colour68k and PPC default depth preserve the covered modeless field during modal typing, remove the modal dialog on guest Cancel, restore modeless focus and accept subsequent typing. Source/fixture/test-binary/log hashes and their recording limits are archived in `current-nested-modal`. This is guest event routing and snapshot evidence; it does not qualify mono68k, PPC8, GPUI semantic callbacks, physical host focus, composition or native accessibility.

Runtime CPU evidence policy and sidecar presence must now agree, preventing a new capture from silently dropping its policy while retaining runtime flags. The partial current-state matrix passes27 altered-evidence rejection checks, including missing policy; old16-case multiline evidence still verifies under its original scope. Reports are archived in `runtime-cpu-policy-checks`. The current public library check `cargo check --locked --lib --no-default-features` passes (45.40 s), with its log preserved there. This confirms the current macOS library configuration, not cross-platform or release qualification. The current state matrix remains running; fourteen explicitly listed PPC16/PPC8 composed images have been visually reviewed, without a complete matrix appearance claim.

A reusable activation pixel verifier now compares exact decoded RGBA within the guest field bounds for selected versus resumed captures, in both native guest and shared-compositor images. All16 pairs in the earlier sourcef69b47ef192-case archive match; eight currently completed PPC8/PPC16 pairs in the source39fbd0cf run also match. Four one-pixel mutations (guest/composed, selected/resumed) are rejected even after recomputing image hashes and passing provenance checks. Partial current-pair and rejection reports are archived in `activation-pixel-restoration`. This establishes restoration consistency, not original Macintosh glyph fidelity, physical host activation or performance. Current full-state capture and further appearance review remain unfinished.

Coverage-span preparation: a separate helper merges adjacent equal-colour pixels within each row without joining gaps or different colours. `cargo test --locked --no-default-features --test gpui_coverage` passes all4 tests (0.11 s after compilation), including exact snapped-device reconstruction across6 scales,4 densities and3 origins. A standalone actual-source harness also reconstructs three selected field crops (mono68k, PPC8, PPC16 at scale1) exactly, reducing full opaque74,024-pixel crops to2018/2019/2020 spans. These counts include blank background pixels and are not current compositor primitive counts. Logs, source/test-binary hashes and recording limits are archived in `coverage-span-preparation`. The helper is not yet linked into the renderer while the current state matrix source/binary remain pinned; actual integration, GPUI pixel equivalence and live performance remain unfinished.

Coverage-span compositor qualification: `cargo run --locked --example gpui-coverage-check --features gpui-demo-test -- OUTPUT_DIRECTORY` executes on macOS's main thread. All12 synthetic resolved-colour path comparisons match exact full screenshots at scales0.75/1/1.5/2 and origins-3.25/0/17.25. Both drawing methods must produce opaque black and coloured pixels, preventing blank equality from passing. All24 screenshots and source/executable/image hashes are archived in `coverage-span-compositor`; one explicitly listed image was visually reviewed. Density2 is inferred from dimensions. Headless test support is required, and the example source is explicitly included in the public package. This confirms row-span versus individual-path raster equivalence for this synthetic coverage, not original Macintosh font appearance, guest interaction, production integration or live speedup. The ordinary test-runner attempt was unable to create macOS's platform off the main thread; the executable is the reproducible comparison route. Current renderer integration awaits completion of the pinned state matrix.

Activation pixel restoration now covers insertion carets at offsets0/26 as well as selected text. Exact guest and composed field RGBA matches all48 before/resumed pairs in the older sourcef69b47ef archive and32 currently completed pairs in the source39fbd0cf run. A reusable rejection checker changes one pixel in selected/visible/resumed guest or composed images, recomputes hashes, verifies provenance still passes and requires restoration comparison to fail; all12 checks pass. Reports are archived in `selection-caret-pixel-restoration`. This verifies restored coverage consistency, not a native glyph appearance oracle, physical host activation, complete current matrix appearance or live performance. The pinned current matrix is still running.

The ordinary integration test `cargo test --locked --no-default-features --test macintosh_modal_text -- --test-threads=1 --nocapture` passes (10.58 s) across mono68k, colour68k, PPC8 and PPC16, asserting actual runtime CPU and display depth. Modal typing leaves the covered modeless field untouched; guest Cancel restores its focus and permits subsequent typing. Reopening/dismissing the modal dialog preserves the modeless generation/text and gives the new modal lifetime a new generation. Every mode actually reuses the disposed modal's guest pointer, so this tests pointer reuse rather than merely a different allocation. Logs, source/fixture/test-binary hashes and execution recording limits are archived in `all-mode-modal-text-lifecycle`. This establishes guest event and snapshot lifecycle behavior; GPUI stale semantic callbacks, physical host focus, composition and native accessibility remain unqualified.

Coverage spans now preserve generic coverage values, allowing both composed RGB pixels and original glyph alpha masks to use the helper during future integration. All5 ordinary unit tests pass (0.09 s), including all255 nonzero alpha values with holes. The headless executable passes24 exact screenshot comparisons for opaque/fractional coverage at four scales and three origins, requires nonblank black/coloured drawing and requires fractional alpha to visibly change the image. Every earlier opaque screenshot remains exact RGBA-identical. All48 images and source/executable hashes are archived in `coverage-span-alpha-compositor`; one explicitly listed fractional-coverage image was visually reviewed. This qualifies helper/path equivalence for synthetic coverage; no font oracle, production integration or live speedup is claimed. Current pinned state captures have completed mono68k and PPC8/PPC16 and are continuing colour68k.

The reusable `compare-gpui-smooth-styled-pixels.py` checks complete matrix provenance/state/geometry on both inputs, matching case membership and fixture identity, then requires exact decoded RGBA for every guest and composed image. Its self-check passes all32 images in the current continuous-italic multiline archive. Separate single-pixel guest/composed mutations with updated hashes pass provenance but fail pixel comparison; reports are in `pixel-comparison-checks`. This validates the comparison tool for future renderer changes; no new renderer equivalence or original Macintosh font oracle is claimed.

The current continuous-italic shared Demo single-line matrix is complete and archived in `continuous-italic-states-shared`, pinned to clean source39fbd0cf with the capture executable/fixture/renderer/font-source hashes checked before and after every case. All192 CPU/depth/state/geometry cases pass across mono68k, colour68k, PPC8 and actual runtime PPC16 at0.75/1/1.5/2 scales. Every384 guest/composed PNG retains exact decoded RGBA through archival. Thirty-five explicitly listed composed samples were visually reviewed; other157 were not. All48 selection/caret before-resume field pairs match exact guest and composed RGBA. All27 metadata rejection and12 changed-restoration-pixel rejection checks pass. This qualifies fixture consistency and sampled smooth appearance; original Macintosh glyph appearance, physical host input/focus, all fonts/styles and production performance remain unqualified.

The shared GPUI styled-field canvas now uses coverage row spans after the native ordered RGB composition has resolved glyphs, clipping, selection inversion and solid caret paint. It coalesces equal adjacent pixels only, preserving guest advances and device snapping; label painting already used alpha row spans. Capture provenance now also pins `gpui_demo_coverage.rs`. All21 text tests pass (6.69 s) and the example builds (7.79 s); logs/source/executable hashes are in `coverage-span-integration`. The24 earlier synthetic opaque/alpha GPUI comparisons remain helper evidence; actual integrated guest-field equivalence and live performance are still being qualified.

Initial integrated source019cae5d pixel comparisons pass all16 images from8 PPC8/PPC16 multiline cases and34 images from17 PPC16 activation cases against complete pre-integration archives. The comparator can explicitly snapshot a partial after-matrix while a job appends cases; it still requires a complete verified baseline and matching fixture/case membership. All4 updated-hash guest/composed mutation checks fail as intended in full/partial modes. Reports in `coverage-span-initial-pixel-equivalence` describe only those initial cases. Both matrix jobs remain running; full integration equivalence, reviewed appearance and live performance remain unfinished.

The synthetic compositor pattern now groups RGB and fractional alpha into tiles with holes and explicitly requires a translucent multi-pixel span. This closes a test weakness: the previous fractional pattern mostly emitted individual translucent pixels. All24 opaque/fractional per-pixel versus span screenshots still match exactly at four scales and three origins; nonblank drawing and a visible alpha effect remain required. All48 losslessly archived images, executable/source hashes and successful log are in `fractional-multipixel-span-compositor`. No font oracle, live speedup or additional actual-guest qualification follows from this synthetic check.

Integrated coverage spans pass the complete16-case shared Demo multiline matrix from clean source019cae5d. All32 full guest/composed images match exact decoded RGBA against the pre-integration continuous-italic multiline archive, across mono68k/colour68k/PPC8/PPC16 and all four scales. All32 images are losslessly archived in `integrated-span-multiline-shared`, with complete source/executable/fixture hashes and one explicitly reviewed current PPC16 sample. The prior16 composed baseline images were reviewed previously. A separately marked partial activation report compares all142 images from71 available cases exactly; the192-case integrated activation matrix remains running. This verifies multiline compositor equivalence while preserving guest font/style/selection/layout; full activation equivalence, pointer regression and live performance remain unfinished.
