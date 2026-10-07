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
- GPUI Kit buttons and popup menus, including nested and scrollable menus.
- Menu selection through `FixtureRunner::select_guest_menu_item`, which validates
  against current guest state and returns through the normal guest event loop.
- Command-key equivalents while the game surface has focus.
- ASCII typing and arrow-key events translated to Macintosh key codes and
  delivered to the guest TextEdit/event path.
- Original guest framebuffer content and unsupported dialogs below the new menu bar.
- A standard `dBoxProc` dialog containing only DITL buttons and static text uses
  GPUI Kit components over the guest item rectangles, with guest input unchanged.
- Standard document-window buttons, checkboxes, and radio buttons use GPUI Kit
  controls over live Control Manager rectangles. A Systemless GPUI scrollbar
  draws its arrows and thumb from the guest value and range.
- Standard LDEF 0 lists use themed GPUI rows with guest-owned text, selection,
  and pointer events; custom list definitions retain their guest pixels.
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
An open popup holds its opening snapshot; the guest still validates selections
against live state. Reopening a menu refreshes its checkmarks and enabled states.
Custom guest MDEF drawing is not reproduced.

Frame overlays preserve the original compact guest geometry; they do not enlarge
title bars or detach windows. Unknown/custom
WDEFs retain guest rendering. Occlusion by a custom nonrectangular window uses
its bounding box conservatively, leaving original pixels in that area. Standard
fullscreen takeover suppresses the frame overlays. Window movement keeps the
guest's original drag outline rather than introducing live GPUI window movement.
The gutter overlays are presentation only: scrollbar tracks do not invent a
thumb or scroll state, and input in those areas still reaches the guest. Real
standard scrollbar controls in document windows now show a guest-value thumb;
guest thumb dragging commits on release on both CPUs. Custom CDEFs and controls
in unsupported windows retain guest pixels. Host keyboard and accessibility
activation, drag feedback and pointer capture, and broader overlap/layout
qualification remain unfinished.

The dialog overlay is limited to standard `dBoxProc` windows with button/static-text
items. Dialogs with custom items, editable text, or other definitions retain
guest pixels. The themed alert button is not yet qualified for keyboard or
accessibility operation through the host control; guest keyboard input and
click handling remain the operative path.

TextEdit fields still use guest pixels. The GPUI runner now forwards ordinary
ASCII keys and arrows to the guest, preserving the guest TERec's text and
selection changes. Mac Roman non-ASCII input, composition, and host modifier
state need separate qualification before themed editable fields can replace
those pixels.

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

For a composed-pixel review of the live About alert, run the opt-in capture
mode on macOS:

```sh
cargo run --no-default-features --features gpui-demo-test --example gpui-menu-demo -- tests/toolbox-showcase/toolbox-showcase.sit --capture-about-alert /tmp/systemless-gpui-about-alert.png
```

This uses GPUI's offscreen renderer on the macOS main thread. The capture
contains the guest framebuffer, GPUI Kit menu and alert components, and
Systemless window chrome together. It needs access to native macOS graphics
services even though it does not open a visible demo window. The alert's
guest carriage returns are displayed as separate lines, and the default
button's guest outline is covered along with its item rectangle.
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
