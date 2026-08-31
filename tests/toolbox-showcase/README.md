# Toolbox showcase fixture

This directory contains the source and reproducible build for one classic
Macintosh fat application. The same `showcase.c` is compiled into a 68K
`CODE` slice and a native PowerPC PEF slice. The PEF remains in the data fork;
the 68K code, `cfrg`, menus, window, and other resources share the resource
fork. Both forks are committed in `toolbox-showcase.sit`.

The application deliberately uses ordinary Toolbox APIs rather than a private
test protocol. Its Pages menu selects three interactive views:

1. Graphics exercises shapes, patterns, clipping, indexed color, lines, and
   text.
2. Controls exercises a push button, checkbox, and scroll bar. Successful
   actions appear as checkmarks in the State menu.
3. Windows creates an auxiliary document window; leaving the page disposes it.

These calls follow the contracts in *Inside Macintosh: Macintosh Toolbox
Essentials* (1992), Event Manager pp. 2-50–2-71, Menu Manager pp. 3-48–3-65,
Window Manager pp. 4-63–4-93, and Control Manager pp. 5-78–5-96. The drawing
surface follows *Inside Macintosh: Imaging With QuickDraw* (1994), pp. 3-38,
3-55–3-95, and 4-68.

## Rebuild and verify

Docker is the only build prerequisite. The image pins the `mps` source commit,
checks the MPW image checksum before installation, and pins `macresources`.
The packaged Rust example used as the archive packer shares the runtime's
locked `stuffit` dependency.

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
`SYSTEMLESS_PREFER_POWERPC=1`. In both cases it performs the same sequence:

1. Confirm Graphics (Pages menu 129, item 1) and one main window.
2. Choose Controls (item 2), then click the button, checkbox, and right scroll
   arrow. State menu 130 items 1–3 must become checked.
3. Choose Windows (item 3). A second window must appear and State item 4 must
   become checked.
4. Return to Graphics. The auxiliary window and its State checkmark must go
   away.

For a manual launch from this repository:

```sh
cargo run --release -- tests/toolbox-showcase/toolbox-showcase.sit
SYSTEMLESS_PREFER_POWERPC=1 cargo run --release -- tests/toolbox-showcase/toolbox-showcase.sit
```

## Classic-Mac oracle runs

Expand the same `toolbox-showcase.sit` on a shared HFS volume. Launch **Toolbox
Showcase** in BasiliskII for the 68K slice and in SheepShaver for the native
PowerPC slice, then follow the four interaction steps above. Keep each emulator
at an 8-bit display; 640×480 is sufficient for the interaction coordinates.
The Pages and State checkmarks, window count, control values, and visible
drawing should agree between the two runs.

## Reference screenshots

These full-frame 800×600 captures all come from the same committed archive.
The Systemless images are exact RGB baselines checked by the integration test;
the classic-Mac images are human-review oracles because system fonts, desktop
patterns, and window chrome can vary between compatible OS installations.

### 68K

| Page | Systemless | BasiliskII |
| --- | --- | --- |
| Graphics | <img src="reference/systemless-68k/01-graphics.png" alt="Graphics page in Systemless running the 68K slice" width="360"> | <img src="reference/basiliskii-68k/01-graphics.png" alt="Graphics page in BasiliskII running the 68K slice" width="360"> |
| Controls after interaction | <img src="reference/systemless-68k/02-controls.png" alt="Interacted Controls page and State menu in Systemless running the 68K slice" width="360"> | <img src="reference/basiliskii-68k/02-controls.png" alt="Interacted Controls page and State menu in BasiliskII" width="360"> |
| Windows | <img src="reference/systemless-68k/03-windows.png" alt="Windows page and auxiliary window in Systemless running the 68K slice" width="360"> | <img src="reference/basiliskii-68k/03-windows.png" alt="Windows page and auxiliary window in BasiliskII" width="360"> |
| Graphics after window disposal | <img src="reference/systemless-68k/04-graphics-return.png" alt="Returned Graphics page in Systemless after disposing the 68K auxiliary window" width="360"> | Same visual contract as the initial Graphics page |

### PowerPC

| Page | Systemless | SheepShaver |
| --- | --- | --- |
| Graphics | <img src="reference/systemless-ppc/01-graphics.png" alt="Graphics page in Systemless running the PowerPC slice" width="360"> | <img src="reference/sheepshaver-ppc/01-graphics.png" alt="Graphics page in SheepShaver running the PowerPC slice" width="360"> |
| Controls after interaction | <img src="reference/systemless-ppc/02-controls.png" alt="Interacted Controls page and State menu in Systemless running the PowerPC slice" width="360"> | <img src="reference/sheepshaver-ppc/02-controls.png" alt="Interacted Controls page and State menu in SheepShaver" width="360"> |
| Windows | <img src="reference/systemless-ppc/03-windows.png" alt="Windows page in Systemless running the PowerPC slice" width="360"> | <img src="reference/sheepshaver-ppc/03-windows.png" alt="Windows page and auxiliary window in SheepShaver" width="360"> |
| Graphics after window disposal | <img src="reference/systemless-ppc/04-graphics-return.png" alt="Returned Graphics page in Systemless after disposing the PowerPC auxiliary window" width="360"> | Same visual contract as the initial Graphics page |

The test loads the `.sit` once per CPU slice, waits on semantic menu and window
state rather than fixed delays, and compares all four rendered frames. To
review and accept an intentional rendering change, regenerate the Systemless
sources and inspect the resulting PNG diff before committing it:

```sh
SYSTEMLESS_UPDATE_TOOLBOX_REFERENCES=1 cargo test --locked --test toolbox_showcase
SYSTEMLESS_PREFER_POWERPC=1 SYSTEMLESS_UPDATE_TOOLBOX_REFERENCES=1 cargo test --locked --test toolbox_showcase
```
