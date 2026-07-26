# Classic Mac menu-bar UX

Status: design proposal, 2026-07-25

## What already exists

Systemless already has a substantial high-level implementation of the
Classic Menu Manager in `src/trap/menu.rs`. It maintains guest-visible menu
records and a host-side menu model, handles the standard creation and mutation
traps, draws classic menu bars and pull-downs into the guest framebuffer, and
models mouse tracking, item flashing, keyboard equivalents, hierarchical
menus, popup menus, marks, styles, icons, and menu-color tables.

The runner currently starts in kiosk mode. `--show-menu-bar`,
`SYSTEMLESS_SHOW_MENU_BAR`, or `FixtureRunner::set_menu_bar_visible(true)`
enables the framebuffer-rendered menu bar. This path is useful as a
compatibility and visual-reference mode, but it is not a good default frontend
experience for a cropped game:

- a cached game viewport can exclude guest screen row 0, hiding the menu;
- including row 0 can expose the otherwise unused desktop between the menu
  bar and a centered game viewport;
- adding the guest menu rectangle to a cropped presentation can resize or
  downscale the game when the menu appears;
- a global top-of-screen Classic Mac menu does not naturally belong inside a
  modern resizable host window.

The missing layer is therefore not a new Menu Manager implementation. It is a
frontend adapter from Systemless's existing guest-owned menu state to a native
or portable host menu surface.

## Lemmings

The Lemmings 1.5.2 resource fork contains four ordinary `MENU` resources:

- Apple: About Lemmings…
- File: Start Game, End Game, New Level…, End Level, Quit
- Edit: Undo, Cut, Copy, Paste, Clear
- Game: Pause, Sound, Music, Faster

The resource records include command-key equivalents and enable flags. These
are standard menus, so Lemmings does not require a custom menu-definition
procedure for its primary menu UX. File > End Game provides the missing way
back from gameplay; File > New Level exposes the access-code dialog.

## Recommended user experience

### macOS

Publish the active guest's standard menus in the real macOS menu bar. Do not
put menu pixels inside the cropped game viewport and do not resize, shift, or
rescale the game when a menu opens.

The application menu should be titled for the guest application. Its first
items should mirror the guest Apple menu (for Lemmings, About Lemmings…).
Host-owned commands such as About Systemless, preferences, Hide, and Quit
Systemless remain clearly separated below them. Guest File, Edit, Game, and
later menus follow as ordinary `NSMenu` instances.

The native menu reflects live guest state:

- disabled menus and items are disabled;
- `-` items become separators;
- check marks and supported menu marks are shown;
- command equivalents are displayed and activated;
- hierarchical menus become submenus;
- guest mutations such as `SetItem`, `EnableItem`, and `CheckItem` update the
  open application's menus without restarting it.

Opening a native guest menu freezes foreground guest time in the same way as
the existing `MenuSelect` tracking implementation. Sound and asynchronous
system services may continue, matching the fact that a real menu tracking
loop blocks the application rather than stopping the machine. Switching host
focus changes the global macOS menu bar to the newly active guest.

### Windows and Linux

Use the same platform-neutral menu snapshot and command bridge, rendered in a
host-owned strip above the guest viewport. The window's content area grows by
the menu strip height; the guest image keeps the same integer scale and screen
position within its own viewport. Opening a menu overlays host UI and never
changes the guest framebuffer dimensions.

### Visibility policy

Use three explicit policies:

1. **Native/automatic (default):** expose standard guest menus through the
   host frontend when the guest installs them. This is the Wine-like behavior
   and keeps Lemmings navigable even though its pixels are cropped.
2. **Classic framebuffer:** retain the current `--show-menu-bar` behavior for
   visual fidelity, custom MDEF testing, screenshots, and applications whose
   menus cannot be represented natively.
3. **Kiosk/hidden:** suppress guest menus for dedicated installations.

Guest `MBarHeight` still controls the classic framebuffer mode. In native
mode, it should not make important commands undiscoverable: a full-screen
host may auto-hide its own menu chrome, but the normal platform gesture at the
top edge reveals it. This is an intentional frontend convenience; guest menu
enable state and command semantics remain authoritative.

## Event and command semantics

The guest application must still believe it followed the normal Event
Manager/Menu Manager path. Calling an application routine directly from a
native menu callback would bypass application code that expects a menu-bar
mouse event followed by `MenuSelect`.

Recommended bridge:

1. Menu traps update a platform-neutral `MenuSnapshot` and increment its
   revision.
2. The frontend applies the newest snapshot on its UI thread.
3. A native selection queues a stable `GuestMenuSelection { menu_id, item }`.
4. The runner delivers the equivalent menu-bar event to the guest.
5. When the application calls `MenuSelect`, the HLE consumes the queued
   selection and returns the normal packed LongInt (`menuID` in the high word,
   item number in the low word). The application's existing switch statement
   then performs the command.
6. Cancellation returns zero and restores tracking/highlight state.

Keyboard commands should follow the same application-visible route. A native
shortcut can synthesize a Command-key event and let the application call
`MenuKey`, or queue the equivalent semantic selection when the native toolkit
has already resolved it. Only one route may fire for a given keystroke.

Snapshots need stable identities and generations so a delayed host callback
cannot select item 3 from a menu that the guest has since deleted and rebuilt.
The runner, not the frontend, validates the menu ID, item index, generation,
and current enabled state before returning a selection.

## Compatibility boundary

Native menus cover standard `MDEF` behavior. Applications can install custom
menu-bar or menu-definition procedures and can draw arbitrary content in a
menu. A native toolkit cannot reproduce those semantics faithfully.

If Systemless observes a custom `MBDF`/`MDEF`, an unsupported dynamic menu
feature, or a guest callback whose behavior depends on live pixel tracking,
that application/menu should fall back to the existing classic framebuffer
renderer. The guest model remains the source of truth in every mode; native
menus are a view and input adapter, not a second Menu Manager.

## Implementation stages

1. Add an immutable public menu snapshot API and revision counter to
   `FixtureRunner`; cover all existing mutation traps with snapshot tests.
2. Add validated queued-selection injection and scripted tests proving the
   guest receives the same packed result and application event sequence as a
   framebuffer click.
3. Implement the macOS `NSMenu` adapter on the main thread, including enabled
   state, separators, marks, shortcuts, and submenus.
4. Make native/automatic, classic framebuffer, and kiosk explicit CLI/library
   policies; keep `--show-menu-bar` as a compatibility alias initially.
5. Add the portable in-window menu frontend for Windows/Linux using the same
   snapshots and selection bridge.
6. Test Lemmings transitions through File > End Game and File > New Level,
   Game-menu toggles, command keys, pause-time behavior, audio continuity,
   crop stability, integer scaling, and repeated menu open/close cycles.

The first shippable slice should be stages 1–3 plus the Lemmings tests. It
delivers the user-visible benefit on macOS without disturbing the already
working classic renderer or the cropped Metal presentation path.
