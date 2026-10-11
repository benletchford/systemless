# GPUI Kit menu and window-frame demo

This prototype presents a running Macintosh application's menu bar and standard window frames using
[GPUI Kit 0.7.1](https://gpui-kit.com/). It is a separate experimental runner;
the usual `systemless` runner and Toolbox drawing code are unchanged.

Frames use GPUI elements and GPUI Kit theme colours over the guest's existing
frame rectangles. They do not use the Kit's host `TitleBar`, which moves the
actual OS window. Window content, geometry and behaviour remain guest-owned.

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
item to show the original guest dialog; Return dismisses a default button.
Mouse input is forwarded to the guest below the menu bar.

Choose **Pages → Windows** to try overlapping windows. Drag a title bar,
click an exposed rear window to activate it, and use the front window's **×**
to close it. The demo follows the guest's title, active state, frame bounds
and front-to-back order. Supported zoom windows show **□** in their original
zoom hit region; resizing still uses the guest's original content-area grip.

## What the demo exercises

- Real `GuestMenuSnapshot` titles, items, checkmarks, enabled states and separators.
- GPUI Kit buttons and popup menus, including nested and scrollable menus.
- Menu selection through `FixtureRunner::select_guest_menu_item`, which validates
  against current guest state and returns through the normal guest event loop.
- Command-key equivalents while the game surface has focus.
- Original guest framebuffer content and dialogs below the new menu bar.
- GPUI-drawn standard title bars, frame edges, close and zoom glyphs, with
  active/inactive colours and close-button press feedback.
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
title bars, detach windows or restyle controls inside windows. Unknown/custom
WDEFs retain guest rendering. Occlusion by a custom nonrectangular window uses
its bounding box conservatively, leaving original pixels in that area. Standard
fullscreen takeover suppresses the frame overlays. Window movement keeps the
guest's original drag outline rather than introducing live GPUI window movement.

The framebuffer is shown at 1:1 with no automatic scaling. Enlarge the window
if necessary. Keyboard forwarding covers Return, Escape, Space, Tab and
Backspace; this is not a full game-input frontend. Audio is serviced but muted,
saves are not persisted, and host-native window integration is not included.
The prototype has not been ported to the browser. GPUI dependencies are optional
and compile only with `gpui-demo`, but add a substantial first-build cost.

## Check the guest bridge

```sh
cargo test --no-default-features --features gpui-demo --example gpui-menu-demo
```

The focused test loads the showcase in monochrome 68k, colour 68k and PPC modes,
selects a page and a nested difficulty item, verifies guest-updated checkmarks,
rejects an invalid item, and checks that a nonempty guest image is exported.
It also verifies frame metadata, title-bar dragging, close-box handling and
promotion of the rear window in all three modes. Separate tests exhaustively
check frame clipping for overlapping rectangles, hidden/custom fallback, and a
rapid close press/release through the demo worker queue.
