# GPUI Kit menu, window-frame, and alert demo

This prototype presents a running Macintosh application's menu bar and standard window frames using
[GPUI Kit 0.7.1](https://gpui-kit.com/). It is a separate experimental runner;
the usual `systemless` runner remains independent of GPUI. The shared PowerPC
scrollbar hit region was corrected to match its existing painted thumb.

Frames use GPUI elements and GPUI Kit theme colours over the guest's existing
frame rectangles. Standard document windows also receive matching content-edge
scrollbar gutters and a resize grip. They do not use the Kit's host `TitleBar`,
which moves the actual OS window. Window content, geometry and behaviour remain
guest-owned.

## Try it on macOS

From a checkout of [systemless](https://github.com/benletchford/systemless):

```sh
cargo run --no-default-features --features gpui-demo --example gpui-menu-demo -- tests/toolbox-showcase/toolbox-showcase.sit
```

Add `--prefer-powerpc` before the archive path to run the PPC slice, or
`--screen-depth 1` to try monochrome 68k presentation. Other supported game
archives can replace the showcase path. Use `--release` for faster guest
execution after the initial build.

Open **Pages → Controls** and reopen Pages to see the guest update its checkmark.
Use **Options → Difficulty** to try nested menus. Open the Apple menu's About
item to see a standard alert's text and button restyled through GPUI Kit;
Return or a click on the button still dismisses it through the guest.
Mouse input is forwarded to the guest below the menu bar.

Choose **Pages → Windows** to try overlapping windows. Drag a title bar,
click an exposed rear window to activate it, and use the front window's **×**
to close it. The demo follows the guest's title, active state, frame bounds
and front-to-back order. Supported zoom windows show **□** in their original
zoom hit region; resizing still uses the guest's original lower-right hit region.

## What the demo exercises

- Real `GuestMenuSnapshot` titles, items, checkmarks, enabled states and separators.
- GPUI Kit buttons and a Systemless-owned themed popup, including nested and
  scrollable menus. Selected guest items survive live label and checkmark updates;
  keyboard navigation scrolls long menus to keep the selected row visible.
- Menu selection through `FixtureRunner::select_guest_menu_item`, which validates
  against current guest state and returns through the normal guest event loop.
- Command-key equivalents while the game surface has focus.
- ASCII typing and arrow-key events translated to Macintosh key codes and
  delivered to the guest TextEdit/event path.
- Original guest framebuffer content and unsupported dialogs below the new menu bar.
- Standard `dBoxProc` dialogs use GPUI Kit buttons, checkboxes, and radio
  buttons when their live guest values are known. Static text is themed; edit
  fields retain guest pixels and guest input remains unchanged.
- Standard document-window buttons, checkboxes, and radio buttons use GPUI Kit
  controls over live Control Manager rectangles. A Systemless GPUI scrollbar
  draws its arrows and thumb from the guest value and range.
- Standard popup CDEFs show GPUI closed controls using the guest's current
  selection and live menu label. Their open dropdown and tracking stay guest-drawn.
- Standard LDEF 0 lists use themed GPUI rows with guest-owned text, selection,
  and pointer events; custom list definitions retain their guest pixels.
- Modern standard Open and Save panels use themed GPUI panels, lists, and
  buttons positioned over the guest's live item rectangles. Save also themes
  the filename field. The guest owns directory contents, selection, filename
  editing, focus, clicks, and the returned reply.
- GPUI-drawn standard title bars, frame edges, close and zoom glyphs, with
  active/inactive colours and close-button press feedback.
- The Systemless logo in the menu bar, and GPUI-drawn document gutters and
  resize corners over the classic `DrawGrowIcon` marks.
- Clipped frame overlays that preserve content and respect overlapping windows.
- Guest-owned dragging, activation and closing through normal mouse events.
- Presentation-only removal of the old menu rows, preserving guest coordinates.
- A guest worker thread and a single latest-frame slot, keeping emulation off
  the GPUI event loop.

## Prototype limits

This macOS-only example is an in-window GPUI menu bar, not a replacement for the macOS system menu
bar. Root menus open on click; cross-title hover switching is not implemented.
Open standard menus follow live guest checkmarks, labels, and enabled states;
the guest still validates selections against current Toolbox state.
Custom guest MDEF drawing is not reproduced.

Use `--capture-standard-menu` to capture the open Pages menu with Systemless's
headless GPUI renderer. Reviewed composed captures use the same showcase on
[68K](tests/toolbox-showcase/reference/gpui-demo/20-standard-menu-68k.png) and
[PowerPC](tests/toolbox-showcase/reference/gpui-demo/20-standard-menu-ppc.png).

Frame overlays preserve the original compact guest geometry; they do not enlarge
title bars or detach windows. Unknown/custom
WDEFs retain guest rendering. Occlusion by a custom nonrectangular window uses
its bounding box conservatively, leaving original pixels in that area. Standard
fullscreen takeover suppresses the frame overlays. Window movement keeps the
guest's original drag outline rather than introducing live GPUI window movement.
Use `--capture-windows` and `--capture-windows-moved` for headless composed
captures of the showcase's three overlapping windows before and after a guest
title-bar drag. The capture checks that the front window retains its identity
and moves by the requested guest-coordinate delta. Reviewed captures are
[68K initial](tests/toolbox-showcase/reference/gpui-demo/21-windows-68k.png),
[68K moved](tests/toolbox-showcase/reference/gpui-demo/21-windows-moved-68k.png),
[PowerPC initial](tests/toolbox-showcase/reference/gpui-demo/21-windows-ppc.png),
and [PowerPC moved](tests/toolbox-showcase/reference/gpui-demo/21-windows-moved-ppc.png).
The scripted capture drag enters the guest directly. Separate GPUI interaction
tests check title-bar pointer translation and held-button routing, then drive
the live showcase through the host drag path and verify the guest window moves
on both CPUs. They also click an exposed rear content region and verify that
the guest promotes and activates its owner window. Use
`--capture-windows-activated` to review the composed front-window change on
[68K](tests/toolbox-showcase/reference/gpui-demo/22-windows-activated-68k.png)
and [PowerPC](tests/toolbox-showcase/reference/gpui-demo/22-windows-activated-ppc.png).
`--capture-windows-grown` checks the guest's 25×25 growth and records the
resized frame on [68K](tests/toolbox-showcase/reference/gpui-demo/23-windows-grown-68k.png)
and [PowerPC](tests/toolbox-showcase/reference/gpui-demo/23-windows-grown-ppc.png).
`--capture-windows-zoomed` grows and zooms the auxiliary window, recording
the reachable themed title bar on
[68K](tests/toolbox-showcase/reference/gpui-demo/26-windows-zoomed-68k.png)
and [PowerPC](tests/toolbox-showcase/reference/gpui-demo/26-windows-zoomed-ppc.png).
`--capture-windows-zoom-restored` clicks the guest zoom box again and checks
that the grown bounds and window identity return. The composed
[68K](tests/toolbox-showcase/reference/gpui-demo/27-windows-zoom-restored-68k.png)
and [PowerPC](tests/toolbox-showcase/reference/gpui-demo/27-windows-zoom-restored-ppc.png)
captures show the re-exposed window stack.
The restored capture also asserts that a point newly exposed in the main
window has its guest-painted white background rather than stale auxiliary
pixels.
The guest showcase gives its stacked inspector an application-defined
`WStateData.stdState` rectangle. `--capture-windows-custom-zoomed` checks that
the guest chooses its 600×420 bounds and records the composed
[68K](tests/toolbox-showcase/reference/gpui-demo/28-windows-custom-zoomed-68k.png)
and [PowerPC](tests/toolbox-showcase/reference/gpui-demo/28-windows-custom-zoomed-ppc.png)
frames. `--capture-windows-custom-zoom-restored` checks its original bounds
and shows the re-exposed stack on
[68K](tests/toolbox-showcase/reference/gpui-demo/29-windows-custom-zoom-restored-68k.png)
and [PowerPC](tests/toolbox-showcase/reference/gpui-demo/29-windows-custom-zoom-restored-ppc.png).
`--capture-windows-promoted` closes the inspector, leaving the auxiliary window
frontmost on [68K](tests/toolbox-showcase/reference/gpui-demo/24-windows-promoted-68k.png)
and [PowerPC](tests/toolbox-showcase/reference/gpui-demo/24-windows-promoted-ppc.png).
`--capture-windows-main-promoted` closes both modeless windows and shows the
main frame on [68K](tests/toolbox-showcase/reference/gpui-demo/25-windows-main-promoted-68k.png)
and [PowerPC](tests/toolbox-showcase/reference/gpui-demo/25-windows-main-promoted-ppc.png).
The capture sends guest input directly; off-pane drag capture remains to be
qualified in composed images. The GPUI interaction test also drags the newly
activated title bar beyond the guest pane inside the host window, checks one
release at the clamped guest edge, and verifies movement on both CPUs. It also
releases outside the outer host window and verifies a single guest release and
completed move. Physical desktop pointer capture and focus-loss behavior still
need qualification.
The gutter overlays are presentation only: scrollbar tracks do not invent a
thumb or scroll state, and input in those areas still reaches the guest. Real
standard scrollbar controls in document windows now show a guest-value thumb;
guest thumb dragging commits on release on both CPUs. Custom CDEFs and controls
in unsupported windows retain guest pixels. Host keyboard and accessibility
activation, drag feedback and pointer capture, and broader overlap/layout
qualification remain unfinished.

Choose **Pages → Popup & Dropdown Lists** to see resource-backed and
programmatic standard popups. `--capture-popup-controls` saves the composed
closed state, reviewed on [68K](tests/toolbox-showcase/reference/gpui-demo/18-popup-68k.png)
and [PowerPC](tests/toolbox-showcase/reference/gpui-demo/18-popup-ppc.png).
The guest owns popup selection, disabled items, menu tracking, and return values;
the open dropdown retains its guest pixels. Host keyboard operation, disabled
item cancellation, and composed open-state captures still need qualification
through the GPUI frontend.
`--capture-popup-controls-selected` makes a guest pointer selection of the
long resource-menu item and captures the repainted GPUI closed control on
[68K](tests/toolbox-showcase/reference/gpui-demo/18-popup-selected-68k.png)
and [PowerPC](tests/toolbox-showcase/reference/gpui-demo/18-popup-selected-ppc.png).
The focused test also checks the selected ControlRecord value and live menu
label on monochrome 68K, colour 68K, and PowerPC.

The dialog overlay is limited to standard `dBoxProc` windows with button,
static-text, checkbox, radio, and edit-text items. Editable fields retain
guest pixels for multiline or tall layouts; ordinary single-line fields show
GPUI text, border, selection, and caret from the guest dialog snapshot.
Dialogs with custom items or unknown control values retain all guest pixels.
The themed alert button is not yet qualified for keyboard or
accessibility operation through the host control; guest keyboard input and
click handling remain the operative path.

Use `--capture-modal-dialog` to review the real showcase preferences dialog
through the macOS headless GPUI renderer. Composed captures are available for
[68K](tests/toolbox-showcase/reference/gpui-demo/07-modal-68k.png) and
[PowerPC](tests/toolbox-showcase/reference/gpui-demo/07-modal-ppc.png).
Use `--capture-modal-dialog-checked` to inspect the guest-updated checkbox:
[68K checked](tests/toolbox-showcase/reference/gpui-demo/07-modal-checked-68k.png)
and [PowerPC checked](tests/toolbox-showcase/reference/gpui-demo/07-modal-checked-ppc.png).
On both CPUs the composed image differs from its unchecked capture only
inside the checkbox's 28-by-28-pixel rendered area.

Standard unstyled document TextEdit now uses a read-only GPUI overlay with
guest-defined line breaks, scroll origin, selection, and insertion point.
Styled and justified records, dialog TextEdit with multiline layouts, and
records overlapping custom controls retain guest pixels. The runner forwards
ordinary ASCII keys and arrows to the guest; the guest owns typing, focus,
and selection. Use `--capture-text-edit` to inspect the composed showcase on
[68K colour](tests/toolbox-showcase/reference/gpui-demo/16-text-edit-68k.png),
[PowerPC](tests/toolbox-showcase/reference/gpui-demo/16-text-edit-ppc.png), or
[68K monochrome](tests/toolbox-showcase/reference/gpui-demo/16-text-edit-mono-68k.png).
The showcase Reset button selects fourteen guest bytes through `TESetSelect`;
`--capture-text-edit-selected` shows the [68K](tests/toolbox-showcase/reference/gpui-demo/16-text-edit-selected-68k.png)
and [PowerPC](tests/toolbox-showcase/reference/gpui-demo/16-text-edit-selected-ppc.png)
selection. `--capture-text-edit-edited` replaces that selection through the
guest key path and shows the resulting caret on [68K](tests/toolbox-showcase/reference/gpui-demo/16-text-edit-edited-68k.png)
and [PowerPC](tests/toolbox-showcase/reference/gpui-demo/16-text-edit-edited-ppc.png).
The same Reset-and-replace guest sequence reaches a 195-byte, five-line
record with selection `[1,1]` in Systemless and BasiliskII. The GPUI editor,
single-line dialog fields, and Save filename now share the kit's dedicated
text-selection color.
Font metrics, caret blink, Mac Roman non-ASCII input, composition, host
modifier state, and broader TextEdit layouts need separate qualification.

The Open and Save overlays are limited to modern standard entry points on both
CPUs. Legacy and custom panels retain guest pixels. The Save filename field now
shows the guest's selection span and caret; caret blink, text composition,
replacement confirmation, New Folder, and full keyboard navigation need more
work before this can replace the ordinary frontend.

The framebuffer is shown at 1:1 with no automatic scaling. Enlarge the window
if necessary. Keyboard forwarding covers Return, Escape, Space, Tab and
Backspace; this is not a full game-input frontend. Audio is serviced but muted,
saves are not persisted, and host-native window integration is not included.
The prototype has not been ported to the browser. GPUI dependencies are optional
and compile only with `gpui-demo`, but add a substantial first-build cost.

## Check the guest bridge

```sh
cargo test --no-default-features --features gpui-demo --example gpui-menu-demo
cargo test --no-default-features --features gpui-demo-test --example gpui-menu-demo
```

The focused tests load the showcase in monochrome 68k, colour 68k and PPC modes,
selects a page and a nested difficulty item, verifies guest-updated checkmarks,
rejects an invalid item, and checks that a nonempty guest image is exported.
It also verifies frame metadata, title-bar dragging, close-box handling and
promotion of the rear window in all three modes. Separate tests exhaustively
check frame clipping for overlapping rectangles, hidden/custom fallback, and a
rapid close press/release through the demo worker queue. The alert test checks
the shared item geometry and default button on all three guest modes, confirms
that the standard alert is eligible for the GPUI overlay, then dismisses it
through guest mouse input. The additional GPUI Kit host-control test clicks the
themed alert button and verifies exactly one guest press and release is queued.
Another host-control test clicks the themed standard Open and Save actions and
checks that each press/release pair lands inside its guest button rectangle.

For a composed-pixel review of the live About alert, run the opt-in capture
mode on macOS:

```sh
cargo run --no-default-features --features gpui-demo-test --example gpui-menu-demo -- tests/toolbox-showcase/toolbox-showcase.sit --capture-about-alert /tmp/systemless-gpui-about-alert.png
```

This uses GPUI's Metal headless renderer on the macOS main thread. The capture
contains the guest framebuffer, GPUI Kit menu and alert components, and
Systemless window chrome together. It needs GPU access but no display-backed
window. The alert's guest carriage returns are displayed as separate lines,
and the default button's guest outline is covered along with its item rectangle.
Committed captures from the 68K and PowerPC slices are available at
[68K alert](tests/toolbox-showcase/reference/gpui-demo/08-alert-68k.png) and
[PowerPC alert](tests/toolbox-showcase/reference/gpui-demo/08-alert-ppc.png).

Use `--capture-controls` or `--capture-controls-changed` in place of
`--capture-about-alert` to compare the standard control presentation before
and after guest checkbox and scrollbar input. The four composed references
cover [68K initial](tests/toolbox-showcase/reference/gpui-demo/02-controls-68k.png),
[68K changed](tests/toolbox-showcase/reference/gpui-demo/02-controls-changed-68k.png),
[PowerPC initial](tests/toolbox-showcase/reference/gpui-demo/02-controls-ppc.png), and
[PowerPC changed](tests/toolbox-showcase/reference/gpui-demo/02-controls-changed-ppc.png).
Use `--capture-controls-dragged` to capture the guest thumb after a full drag
and release: [68K](tests/toolbox-showcase/reference/gpui-demo/02-controls-dragged-68k.png)
and [PowerPC](tests/toolbox-showcase/reference/gpui-demo/02-controls-dragged-ppc.png).
Use `--capture-controls-held` to capture the composed surface before release.
The guest value is asserted unchanged while the Systemless GPUI thumb outline
tracks the held pointer: [68K](tests/toolbox-showcase/reference/gpui-demo/02-controls-held-68k.png)
and [PowerPC](tests/toolbox-showcase/reference/gpui-demo/02-controls-held-ppc.png).

Use `--capture-lists` and `--capture-lists-selected` for standard list rows
before and after a guest selection: [68K initial](tests/toolbox-showcase/reference/gpui-demo/15-lists-68k.png),
[68K selected](tests/toolbox-showcase/reference/gpui-demo/15-lists-selected-68k.png),
[PowerPC initial](tests/toolbox-showcase/reference/gpui-demo/15-lists-ppc.png), and
[PowerPC selected](tests/toolbox-showcase/reference/gpui-demo/15-lists-selected-ppc.png).

For the Standard File Save panel, capture either its guest framebuffer or the
composed GPUI surface:

```sh
cargo run --no-default-features --features gpui-demo-test --example gpui-menu-demo -- tests/toolbox-showcase/toolbox-showcase.sit --prefer-powerpc --capture-standard-file-save /tmp/systemless-save-ppc.png
cargo run --no-default-features --features gpui-demo-test --example gpui-menu-demo -- tests/toolbox-showcase/toolbox-showcase.sit --prefer-powerpc --capture-standard-file-save-composed /tmp/systemless-save-ppc-composed.png
```

The first command checks guest pixels without native graphics services; the
second uses the Metal headless renderer to include the themed overlay. The
reviewed guest captures are
[68K Save](tests/toolbox-showcase/reference/gpui-demo/17-standard-file-save-68k-guest.png)
and [PowerPC Save](tests/toolbox-showcase/reference/gpui-demo/17-standard-file-save-ppc-guest.png).
The composed captures are [68K Save](tests/toolbox-showcase/reference/gpui-demo/17-standard-file-save-68k.png)
and [PowerPC Save](tests/toolbox-showcase/reference/gpui-demo/17-standard-file-save-ppc.png).
Use `--capture-standard-file-save-edited-composed` to capture the same field
after the guest replaces its initial selection with `S`: [68K edited](tests/toolbox-showcase/reference/gpui-demo/17-standard-file-save-edited-68k.png)
and [PowerPC edited](tests/toolbox-showcase/reference/gpui-demo/17-standard-file-save-edited-ppc.png).
Use `--capture-standard-file-open-composed` for the corresponding standard
Open panel: [68K Open](tests/toolbox-showcase/reference/gpui-demo/17-standard-file-open-68k.png)
and [PowerPC Open](tests/toolbox-showcase/reference/gpui-demo/17-standard-file-open-ppc.png).
