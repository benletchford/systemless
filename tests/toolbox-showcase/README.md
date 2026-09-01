# Toolbox showcase fixture

This directory contains the source and reproducible build for a classic
Macintosh fat application. The same `showcase.c` is compiled into a 68K
`CODE` slice and a native PowerPC PEF slice. The PEF remains in the data fork;
the 68K code, `cfrg`, menus, windows, dialogs, and other resources share the
resource fork. Both forks are committed in `toolbox-showcase.sit`.

The application deliberately uses ordinary Toolbox APIs rather than a private
test protocol. Its Pages menu selects seven interactive views:

1. Graphics exercises patterns, clipping, indexed color, lines, shapes, and
   text.
2. Controls exercises a push button, checkbox, and scroll bar. Successful
   actions appear as checkmarks in the State menu.
3. Windows creates an auxiliary document window; leaving the page disposes it.
4. Drawing & 3D Bevels exercises polygons, arcs, regions, pictures, icons,
   fonts, styles, and metrics. The PowerPC slice also builds and submits a lit
   QuickDraw 3D TriMesh through a view, camera, renderer, and draw context,
   before both slices paint the same architecture-neutral visible result.
5. Game Preferences presents a game-style configuration panel with audio
   checkboxes, difficulty and renderer radio groups, a volume scroll bar, and
   action buttons. Its settings stay synchronized with hierarchical menus.
6. Dialogs & Alerts exercises resource-backed modal dialogs, controls,
   editable text, and a system alert.
7. Palettes activates a resource-backed mixed-usage palette, draws through
   `PmForeColor` and `PmBackColor`, translates an unrelated indexed PICT
   through a canonical offscreen GWorld into the active screen palette,
   preserves positional indexes copied from a same-identity device ColorTable
   whose RGB entries are transiently black, verifies that `RGBForeColor` uses
   the screen GDevice inverse table when the logical and hardware CLUTs differ,
   and animates explicit CLUT entries without redrawing their indexed pixels.
   Both slices record and replay the PICT through the same visible path.

The menus also cover checkmarks, keyboard equivalents, three levels of
hierarchical game options, and switching menu-bar selections while a menu is
already being tracked.

These calls follow the contracts in *Inside Macintosh: Macintosh Toolbox
Essentials* (1992), Event Manager pp. 2-50–2-71, Menu Manager pp. 3-48–3-65,
Window Manager pp. 4-63–4-93, Control Manager pp. 5-78–5-96, and Dialog
Manager pp. 6-43–6-84. The drawing surface follows *Inside Macintosh: Imaging
With QuickDraw* (1994), pp. 3-38, 3-55–3-95, and 4-68. Palette activation,
usage categories, indexed drawing, and animation follow *Inside Macintosh,
Volume VI* (1991), pp. 20-8–20-22.

## Rebuild and verify

Docker is the only build prerequisite. The image pins the `mps` source commit,
checks the MPW image checksum before installation, and pins `macresources`.
The fixture-local, non-publishable Rust packer pins the same released
`stuffit` crate as the runtime.

```sh
./tests/toolbox-showcase/build.sh
./tests/toolbox-showcase/build.sh --verify
```

`--verify` rebuilds the application and fails unless the resulting StuffIt
archive is byte-for-byte identical to the committed archive. Maintainers use
`--update` only when intentionally changing the fixture source or toolchain.
All intermediates remain under the ignored `build/` directory.

## Systemless interaction contract

The integration test launches the committed archive twice: the default launch
selects the 68K `CODE` slice, and the second launch selects the native PEF with
`SYSTEMLESS_PREFER_POWERPC=1`. Both runs use the same semantic assertions and
exact Systemless framebuffer references while performing the same sequence:

1. Confirm Graphics (Pages menu 129, item 1), the initial menu state, and one
   main window.
2. Choose Controls (item 2), then click the button, checkbox, and right scroll
   arrow. State menu 130 items 1–3 must become checked.
3. Choose Windows (item 3). A second window must appear and State item 4 must
   become checked.
4. Choose Drawing & 3D Bevels (item 4), verify representative QuickDraw
   output, and allow the native PowerPC QuickDraw 3D submission to complete.
5. Choose Game Preferences (item 5), change its controls, verify the matching
   hierarchical-menu checkmarks, then change menu items and verify the panel.
6. Open File → Game Options and capture the nested submenu while it is live.
7. Choose Dialogs & Alerts (item 6), open the resource-backed modal dialog,
   modify it, and confirm it with OK.
8. Invoke the system alert, capturing its live modal state on implementations
   that block for a response, then dismiss it or record its return.
9. Verify the final dialog status after both modal sessions.
10. Activate the Palettes page, verify the indexed PICT → GWorld → screen
    transfer retains distinct colors across unrelated CTables, verify a
    same-device transfer retains its positional indexes through a transient
    black device ColorTable, verify `RGBForeColor` resolves through the
    indexed screen GDevice inverse table when the logical and hardware CLUTs
    differ, and capture the initial tolerant and animated-explicit color
    environment.
11. Click Animate Palette and capture the same indexed pixels recolored by
    `AnimateEntry` without repainting the swatches.
12. Open File, drag across the menu bar to Pages, and capture Graphics
    highlighted before releasing.
13. Confirm the release selected Graphics, restored the default color
    environment, and disposed the auxiliary window.

For a manual launch from the public repository:

```sh
cargo run --release -- tests/toolbox-showcase/toolbox-showcase.sit
SYSTEMLESS_PREFER_POWERPC=1 cargo run --release -- tests/toolbox-showcase/toolbox-showcase.sit
```

## Classic-Mac oracle runs

Expand the same `toolbox-showcase.sit` on a shared HFS volume. Launch **Toolbox
Showcase** in BasiliskII for the 68K slice and in SheepShaver for the native
PowerPC slice, then follow the thirteen interaction steps above. Use an 800×600,
8-bit display for captures matching this gallery. The Pages, State, and nested
menu checkmarks, window count, control values, modal sessions, visible drawing,
and final page provide the comparison points between runs.

## Reference screenshots

These full-frame 800×600 captures all come from the same committed archive.
The Systemless images are exact RGB baselines checked by the integration test;
the classic-Mac images are human-review oracles because system fonts, desktop
patterns, and window chrome can vary between compatible OS installations.
The paired frames are therefore functional comparisons rather than pixel-identical
targets. Cursor placement and preference values can also differ when a manual
oracle capture is taken at a different point in the interaction sequence. The
shared Systemless baseline is required for both CPU slices. The PowerPC run
also submits native QuickDraw 3D geometry before the fixture paints the same
architecture-neutral visible result; classic operating-system presentation
remains environment-dependent.

SheepShaver's oracle display is direct color, so its animation checkpoint is
intentionally unchanged: *Inside Macintosh, Volume VI* (1991), p. 20-11 notes
that color-table animation is unavailable on direct devices. Its same-device
transfer band likewise uses an RGB fallback because positional CLUT indexes
exist only on indexed devices. The Systemless 8-bit captures for both CPU
slices and BasiliskII's 8-bit capture exercise the actual indexed paths.

### 68K oracle

| Checkpoint | Systemless | BasiliskII |
| --- | --- | --- |
| 1. Graphics | <img src="reference/systemless/01-graphics.png" alt="Shared Graphics baseline in Systemless" width="360"> | <img src="reference/basiliskii-68k/01-graphics.png" alt="Graphics page in BasiliskII running the 68K slice" width="360"> |
| 2. Controls and State menu | <img src="reference/systemless/02-controls.png" alt="Shared Controls baseline in Systemless" width="360"> | <img src="reference/basiliskii-68k/02-controls.png" alt="Interacted Controls page and State menu in BasiliskII" width="360"> |
| 3. Windows | <img src="reference/systemless/03-windows.png" alt="Shared Windows baseline in Systemless" width="360"> | <img src="reference/basiliskii-68k/03-windows.png" alt="Windows page and auxiliary window in BasiliskII" width="360"> |
| 4. Drawing and 3D fallback | <img src="reference/systemless/04-drawing.png" alt="Shared Drawing baseline in Systemless" width="360"> | <img src="reference/basiliskii-68k/04-drawing.png" alt="QuickDraw drawing and 68K bevel fallback in BasiliskII" width="360"> |
| 5. Game preferences | <img src="reference/systemless/05-preferences.png" alt="Shared Game Preferences baseline in Systemless" width="360"> | <img src="reference/basiliskii-68k/05-preferences.png" alt="Changed game preferences in BasiliskII" width="360"> |
| 6. Nested menus | <img src="reference/systemless/06-nested-menus.png" alt="Shared nested menus baseline in Systemless" width="360"> | <img src="reference/basiliskii-68k/06-nested-menus.png" alt="File and nested Game Options menus in BasiliskII" width="360"> |
| 7. Modal dialog | <img src="reference/systemless/07-modal-dialog.png" alt="Shared modal dialog baseline in Systemless" width="360"> | <img src="reference/basiliskii-68k/07-modal-dialog.png" alt="Resource-backed game configuration dialog in BasiliskII" width="360"> |
| 8. Alert | <img src="reference/systemless/08-alert.png" alt="Shared alert baseline in Systemless" width="360"> | <img src="reference/basiliskii-68k/08-alert.png" alt="System alert in BasiliskII" width="360"> |
| 9. Dialog result | <img src="reference/systemless/09-dialogs.png" alt="Shared dialog result baseline in Systemless" width="360"> | <img src="reference/basiliskii-68k/09-dialogs.png" alt="Dialogs page after modal interactions in BasiliskII" width="360"> |
| 10. Palette activation | <img src="reference/systemless/10-palette.png" alt="Shared palette baseline in Systemless" width="360"> | <img src="reference/basiliskii-68k/10-palette.png" alt="Initial mixed-usage palette in BasiliskII" width="360"> |
| 11. Palette animation | <img src="reference/systemless/11-palette-animated.png" alt="Shared palette animation baseline in Systemless" width="360"> | <img src="reference/basiliskii-68k/11-palette-animated.png" alt="Animated explicit CLUT entries in BasiliskII" width="360"> |
| 12. Menu-bar hover | <img src="reference/systemless/12-menu-hover.png" alt="Shared menu-bar hover baseline in Systemless" width="360"> | <img src="reference/basiliskii-68k/12-menu-hover.png" alt="Pages menu selected while dragging from File in BasiliskII" width="360"> |
| 13. Palette restoration | <img src="reference/systemless/13-graphics-return.png" alt="Shared palette restoration baseline in Systemless" width="360"> | <img src="reference/basiliskii-68k/13-graphics-return.png" alt="Returned Graphics page with the default palette restored in BasiliskII" width="360"> |

### PowerPC oracle

| Checkpoint | Shared Systemless baseline | SheepShaver |
| --- | --- | --- |
| 1. Graphics | <img src="reference/systemless/01-graphics.png" alt="Shared Graphics baseline in Systemless" width="360"> | <img src="reference/sheepshaver-ppc/01-graphics.png" alt="Graphics page in SheepShaver running the PowerPC slice" width="360"> |
| 2. Controls and State menu | <img src="reference/systemless/02-controls.png" alt="Shared Controls baseline in Systemless" width="360"> | <img src="reference/sheepshaver-ppc/02-controls.png" alt="Interacted Controls page and State menu in SheepShaver" width="360"> |
| 3. Windows | <img src="reference/systemless/03-windows.png" alt="Shared Windows baseline in Systemless" width="360"> | <img src="reference/sheepshaver-ppc/03-windows.png" alt="Windows page and auxiliary window in SheepShaver" width="360"> |
| 4. Drawing and QuickDraw 3D | <img src="reference/systemless/04-drawing.png" alt="Shared Drawing baseline in Systemless" width="360"> | <img src="reference/sheepshaver-ppc/04-drawing.png" alt="QuickDraw drawing and native QuickDraw 3D TriMesh in SheepShaver" width="360"> |
| 5. Game preferences | <img src="reference/systemless/05-preferences.png" alt="Shared Game Preferences baseline in Systemless" width="360"> | <img src="reference/sheepshaver-ppc/05-preferences.png" alt="Changed game preferences in SheepShaver" width="360"> |
| 6. Nested menus | <img src="reference/systemless/06-nested-menus.png" alt="Shared nested menus baseline in Systemless" width="360"> | <img src="reference/sheepshaver-ppc/06-nested-menus.png" alt="File and nested Game Options menus in SheepShaver" width="360"> |
| 7. Modal dialog | <img src="reference/systemless/07-modal-dialog.png" alt="Shared modal dialog baseline in Systemless" width="360"> | <img src="reference/sheepshaver-ppc/07-modal-dialog.png" alt="Resource-backed game configuration dialog in SheepShaver" width="360"> |
| 8. Alert | <img src="reference/systemless/08-alert.png" alt="Shared alert baseline in Systemless" width="360"> | <img src="reference/sheepshaver-ppc/08-alert.png" alt="Live system alert in SheepShaver" width="360"> |
| 9. Dialog result | <img src="reference/systemless/09-dialogs.png" alt="Shared dialog result baseline in Systemless" width="360"> | <img src="reference/sheepshaver-ppc/09-dialogs.png" alt="Dialogs page after modal interactions in SheepShaver" width="360"> |
| 10. Palette activation | <img src="reference/systemless/10-palette.png" alt="Shared palette baseline in Systemless" width="360"> | <img src="reference/sheepshaver-ppc/10-palette.png" alt="Initial mixed-usage palette in SheepShaver" width="360"> |
| 11. Palette animation | <img src="reference/systemless/11-palette-animated.png" alt="Shared palette animation baseline in Systemless" width="360"> | <img src="reference/sheepshaver-ppc/11-palette-animated.png" alt="Animated explicit CLUT entries in SheepShaver" width="360"> |
| 12. Menu-bar hover | <img src="reference/systemless/12-menu-hover.png" alt="Shared menu-bar hover baseline in Systemless" width="360"> | <img src="reference/sheepshaver-ppc/12-menu-hover.png" alt="Pages menu selected while dragging from File in SheepShaver" width="360"> |
| 13. Palette restoration | <img src="reference/systemless/13-graphics-return.png" alt="Shared palette restoration baseline in Systemless" width="360"> | <img src="reference/sheepshaver-ppc/13-graphics-return.png" alt="Returned Graphics page with the default palette restored in SheepShaver" width="360"> |

The test loads the `.sit` once per CPU slice, waits on semantic menu and window
state rather than relying on fixed delays, and compares all thirteen rendered
frames. To review and accept an intentional rendering change, regenerate the
Systemless sources and inspect the resulting PNG diff before committing it:

```sh
SYSTEMLESS_UPDATE_TOOLBOX_REFERENCES=1 cargo test --locked --test toolbox_showcase
SYSTEMLESS_PREFER_POWERPC=1 SYSTEMLESS_UPDATE_TOOLBOX_REFERENCES=1 cargo test --locked --test toolbox_showcase
```
