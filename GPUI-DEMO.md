# GPUI Kit menu demo

This prototype presents a running Macintosh application's menu bar using
[GPUI Kit 0.7.1](https://gpui-kit.com/). It is a separate experimental runner;
the usual `systemless` runner and Toolbox drawing code are unchanged.

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

## What the demo exercises

- Real `GuestMenuSnapshot` titles, items, checkmarks, enabled states and separators.
- GPUI Kit buttons and popup menus, including nested and scrollable menus.
- Menu selection through `FixtureRunner::select_guest_menu_item`, which validates
  against current guest state and returns through the normal guest event loop.
- Command-key equivalents while the game surface has focus.
- Original guest framebuffer content and dialogs below the new menu bar.
- Presentation-only removal of the old menu rows, preserving guest coordinates.
- A guest worker thread and a single latest-frame slot, keeping emulation off
  the GPUI event loop.

## Prototype limits

This macOS-only example is an in-window GPUI menu bar, not a replacement for the macOS system menu
bar. Root menus open on click; cross-title hover switching is not implemented.
An open popup holds its opening snapshot; the guest still validates selections
against live state. Reopening a menu refreshes its checkmarks and enabled states.
Custom guest MDEF drawing is not reproduced.

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
