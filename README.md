# systemless

Systemless is a high-level runtime for 68k classic Macintosh applications and
games, written in Rust.

It executes guest 68k code with the [`m68k`](https://crates.io/crates/m68k)
crate and handles Mac OS A-line traps in native Rust. Native builds explicitly
enable m68k's Cranelift JIT for eligible hot traces; WebAssembly uses its
portable trace executor. That lets packaged Mac applications run without a Mac
ROM image, a full System install, or hardware emulation.

See it running in the browser: **[Marathon](https://systemless.org/marathon)**
and **[Escape Velocity](https://systemless.org/escape-velocity)**. More demos
are available at <https://systemless.org/>.

## Status

Systemless is focused on real 68k applications that use the classic Mac Toolbox.
The HLE now covers the major runtime surfaces needed by interactive software:

- Memory Manager handles, pointers, zones, low-memory globals, and common
  exception paths.
- Resource Manager, Segment Loader, File Manager calls, and an in-memory
  HFS-like VFS with data and resource forks.
- QuickDraw ports, regions, text, shapes, PICT, CopyBits, color tables,
  offscreen GWorlds, cursors, and 1bpp/4bpp/8bpp framebuffers.
- Event, Menu, Window, Control, Dialog, TextEdit, Cursor, Process, Sound,
  SANE, and common Toolbox utility traps.
- Cooperative Thread Manager contexts, yielding, current-thread queries,
  critical sections, and thread-entry result delivery.
- Sound Manager playback, channel state, command queues, callbacks, file
  playback, and host audio mixing.

It is not a bit-perfect Mac hardware emulator. Hardware-specific services such
as slot interrupts, device queues, removable-media behavior, and multi-process
system integration are modeled only where guest-visible behavior matters.

## Quick Start

Install with Homebrew on macOS:

```sh
brew install benletchford/tap/systemless
systemless path/to/app-or-game.sit
```

Or install from crates.io:

```sh
cargo install systemless
systemless path/to/app-or-game.sit
```

The installed `systemless` command opens a window, renders the guest framebuffer,
maps keyboard and mouse input, and enables audio when a host backend is
available.

Common runner options:

```sh
systemless --headless --max-instructions 5000000 path/to/app.sit
systemless --arrows-as-numpad path/to/game.sit
systemless --show-menu-bar path/to/app.sit
systemless --cpu-mhz 25 path/to/app.sit
```

Systemless accepts StuffIt and ZIP archives, MacBinary files, and raw/macOS resource forks.
Archives may contain multiple files; Systemless populates the in-memory VFS and
selects an executable resource fork from the archive.

Desktop saves are stored next to the launched archive under
`.systemless/saves/<archive-name>/`. For example, launching
`/Games/EV Override 1.0.1.sit` restores and persists saves under
`/Games/.systemless/saves/EV Override 1.0.1/`. The store preserves Mac data and
resource forks and is kept separate from the original archive.

Systemless does not ship applications, games, Mac ROMs, or Apple system software.
Use legally obtained application archives.

For a local checkout, use `cargo run --release -- path/to/app-or-game.sit`.

## Library Use

Programmatic loading goes through `FixtureRunner`:

```rust
use systemless::runner::{FixtureRunner, FixtureRunnerConfig};

let bytes = std::fs::read("game.sit").expect("read game");
let mut runner = FixtureRunner::new(32 * 1024 * 1024, FixtureRunnerConfig::default());

systemless::game::load_game(&mut runner, &bytes).expect("load game");
let (_steps, _still_running) = runner.run_steps(100_000, None);
runner.composite_frame();
```

Use `systemless::display` to render the current framebuffer for custom frontends.

## Save Persistence

Systemless keeps the guest filesystem in the runner's in-memory VFS. Persistence
is a frontend responsibility: the engine exposes snapshots of VFS files, and a
frontend decides where to store them.

Use the `FixtureRunner` VFS snapshot API for save files:

- `vfs_file_summaries()` lists VFS files with fork sizes, hashes, and metadata.
- `vfs_file_snapshot(path)` exports one file's data fork, resource fork, and
  Finder metadata.
- `import_vfs_file(snapshot)` restores a previously exported file into the VFS.
- `remove_vfs_file(path)` removes a file from the VFS.

The expected frontend sequence is:

```text
create runner
load archive into runner
record archive VFS summaries/fingerprints
load stored save snapshots
import_vfs_file(...) for each stored save
init_game(...)
periodically scan vfs_file_summaries()
persist changed user-save snapshots from vfs_file_snapshot(...)
flush one final scan on shutdown
```

Record the archive fingerprints before importing stored saves. That lets the
frontend avoid copying packaged game files into the save store and persist only
new or changed user-save files. Save-file filtering is frontend policy; common
filters exclude System Folder preferences, temporary items, Trash, and desktop
database files.

The built-in desktop runner uses this API and stores snapshots next to the
launched archive under `.systemless/saves/<archive-name>/`.

## Crate Map

| Module | Role |
| ------ | ---- |
| `game` | Shared app/archive loading, VFS population, and runner initialization. |
| `runner` | Main execution API: CPU stepping, input events, timing, audio, and frame composition. |
| `trap` | Toolbox and OS trap handlers grouped by manager. |
| `memory` | Guest RAM, low-memory globals, heap zones, handles, and pointer operations. |
| `quickdraw` | Public QuickDraw data helpers and font routing. |
| `display` | Host framebuffer and cursor rendering helpers. |
| `sound` | Sound Manager state and PCM mixing engine. |
| `loader` | 68k CODE resource loading and jump-table setup. |
| `trace` | Runtime trace hook (event/snapshot types + `TraceSink`) for cross-runtime parity comparison. |

## Build And Test

```sh
cargo build --release
cargo test --lib
cargo test --lib --features test-support   # also covers scripted_traces
cargo check --no-default-features
cargo package
```

The off-by-default `test-support` feature exposes `scripted_traces`, the
deterministic trap-replay test scaffolding. It is kept out of the published
public API; enable it only when running tests.

The default `gui` feature enables the desktop runner dependencies: `winit`,
`softbuffer`, and `cpal`. Disable default features for headless library builds.

On Linux, the default GUI/audio build also needs ALSA development files for
`cpal`'s ALSA backend. Install `pkg-config` plus your distribution's ALSA dev
package before running `cargo build --release`; for example:

```sh
sudo apt install pkg-config libasound2-dev      # Debian/Ubuntu
sudo dnf install pkgconf-pkg-config alsa-lib-devel  # Fedora/RHEL
sudo pacman -S pkgconf alsa-lib                # Arch
```

## Font Data

Systemless ships its own original bitmap fonts. Every glyph is authored for this
project — hand-drawn as ASCII art in `src/quickdraw/fonts/pixel_font/` and
lowered to static glyph tables by `const fn` at compile time; there is no
external font file, no offline baker, and no third-party font data in the crate.

The faces are named after Australian native plants. The classic Mac font names
survive **only as internal compatibility identifiers** so that classic
applications requesting a family by name or ID still resolve to a sensible face
— this is nominative use, not branding.

| systemless face | Kind                   | Stands in for (compat family, font ID) |
|-----------------|------------------------|----------------------------------------|
| **Jarrah**      | Heavy system / UI sans | Chicago (0) |
| **Kurrajong**   | Humanist body sans     | Geneva (3), Application (1), Helvetica (21); Venice (5), London (6), Cairo (11) |
| **Mallee**    | Monospace              | Monaco (4), Courier (22) |
| **Ironbark**    | Serif                  | New York (2), Times (20) |

Sizes: Jarrah 9/12; Kurrajong 9/10/12/14/18/24 (+ Application 12, Helvetica 12);
Mallee 9/10/12; Ironbark 12/14/18.

Every face is hand-drawn glyph by glyph in a consistent house style, with
advances, side-bearings and x-height / cap height conformed to the original Mac
strike so classic text lays out identically. Kurrajong 24 and Ironbark 18 are
heavy display cuts matching their originals' bold weight. Venice (5), London (6)
and Cairo (11) render as Kurrajong — the reference System has no strike for
those families and substitutes the application font, which Systemless mirrors.

Render every face on white and black backgrounds for review with
`cargo run --bin font_specimen` (output in `target/font_specimens/`).

If `SYSTEMLESS_ORIGINAL_FONTS_DIR` is set, Systemless can also load locally
generated bitmap override blobs ahead of the built-in catalogue.

### Trademark / non-affiliation

Systemless is not affiliated with, authorized by, or endorsed by Apple Inc.
Macintosh, Mac OS, QuickDraw, and the classic font family names (Chicago,
Geneva, Monaco, New York, Venice, London, Cairo, etc.) are trademarks of Apple
Inc. "Times" / "Helvetica" / "Courier" are trademarks of their respective
owners. These names appear here solely as compatibility identifiers to
interoperate with classic Macintosh software; the systemless faces themselves
are original works, distributed under their own botanical names.

### Font license

The systemless bitmap faces are original artwork and are licensed separately
from the crate's GPL code. The glyph sources in
`src/quickdraw/fonts/pixel_font/` are additionally available under the **SIL
Open Font License 1.1** (see [OFL.txt](./OFL.txt)), with **"Systemless"** as
the Reserved Font Name. This lets the faces be reused outside this project —
including in software that is not GPL — while the emulator code itself stays
GPL-3.0-or-later. Under the OFL, a modified font must not use the reserved
name.

## Useful Environment Variables

| Variable | Effect |
| -------- | ------ |
| `SYSTEMLESS_LOAD_EXECUTABLE` | Selects an executable from a multi-app archive by substring. |
| `SYSTEMLESS_ORIGINAL_FONTS_DIR` | Loads optional runtime font override blobs. |
| `SYSTEMLESS_SHOW_MENU_BAR` | Shows classic Mac menu chrome by default. |
| `SYSTEMLESS_TRACE_LOAD` | Logs archive, VFS, resource, and startup loading diagnostics. |
| `SYSTEMLESS_TRACE_LOADSEG` | Logs Segment Loader jump-table patching. |
| `SYSTEMLESS_TRACE_TRAP_COUNTS` | Prints trap dispatch frequency summaries. |

## References & Documentation Conventions

Systemless reimplements guest-visible Toolbox / OS behavior, favoring what an
application observes over cycle- or hardware-level fidelity. That behavior is a
contract, so non-obvious decisions are documented **at the code that implements
them** and cite the source that justifies them — a reader should be able to
check the reasoning without leaving the file.

**When to cite.** Add a citation whenever the "why" is not obvious from the
code: trap semantics and edge cases, magic constants and error codes, on-disk or
in-heap struct layouts, and any deliberate deviation from the books. Put it in
the `///` doc comment of the trap/function, or an inline `//` comment on the
exact line it explains.

**Inside Macintosh** is the primary source. Cite the volume, year, and page,
using `p.` for a page and `pp.` for a range:

- Old series — roman-numeral volumes; the page carries the volume prefix:
  `Inside Macintosh Volume I (1985), p. I-115`
- New series — named volumes; the page is chapter-page:
  `Inside Macintosh: Devices (1994), pp. 2-70`

A short form without the year is fine for a repeated reference in the same area
(`Inside Macintosh Volume I, I-189`). Multiple sources can back one line:
`Inside Macintosh: Files (1992), p. 2-236; Technical Note #108`.

**Other sources**, cited the same way (inline, next to the code):

- **BasiliskII** / **Executor** — when the books are silent or ambiguous, cite
  the observed behavior of an existing emulator that a matching guest relies on;
  name the file/function where it helps (e.g. `BasiliskII's fpu_ieee.cpp`).
- **Apple Technical Notes** — by number, e.g. `Technical Note #108`.

Cite only the source, never the test that checks it: comments should not name
tests, fixtures, or tooling that live outside this crate.

Prefer the narrowest source that settles the question, and always note when
Systemless intentionally diverges from it, and why.

## License

The emulator code is GPL-3.0-or-later. See [LICENSE](./LICENSE).

The systemless bitmap fonts are original artwork authored for this project and
are additionally available under the SIL Open Font License 1.1 (see
[OFL.txt](./OFL.txt)), Reserved Font Name "Systemless" — see
[Font license](#font-license). No third-party font data is bundled.
