<p align="center">
  <a href="https://systemless.org/">
    <img src=".github/assets/systemless-logo.svg" alt="Systemless mascot" width="192" height="192">
  </a>
</p>

<h1 align="center">systemless</h1>

<p align="center">
  <strong>A high-level runtime for classic Macintosh applications and games.</strong><br>
  Run original Mac software without a ROM image, System installation, or hardware emulation.
</p>

<p align="center">
  <a href="https://github.com/benletchford/systemless/actions/workflows/ci.yml"><img src="https://github.com/benletchford/systemless/actions/workflows/ci.yml/badge.svg" alt="CI"></a>
  <a href="https://crates.io/crates/systemless"><img src="https://img.shields.io/crates/v/systemless.svg" alt="crates.io"></a>
  <a href="https://docs.rs/systemless"><img src="https://docs.rs/systemless/badge.svg" alt="Documentation"></a>
  <a href="LICENSE"><img src="https://img.shields.io/crates/l/systemless.svg" alt="License"></a>
</p>

<p align="center">
  <img src=".github/assets/systemless-launch-macos.gif" alt="Launching Escape Velocity from Finder with native macOS menu and application icon integration">
</p>

Systemless reimplements the classic Mac Toolbox and operating-system APIs in
Rust, allowing original 68K and PowerPC Macintosh software to run without a ROM
image, a System installation, or hardware emulation. On macOS, classic
applications keep their own identity: guest menus appear in the native menu bar,
while the guest application name and icon integrate with the Dock.

## Quick Start

Install with Homebrew on macOS:

```sh
brew install benletchford/tap/systemless
systemless path/to/app-or-game.sit
```

Install a prebuilt release with [cargo-binstall](https://github.com/cargo-bins/cargo-binstall):

```sh
cargo binstall systemless
```

Release archives and SHA-256 checksums are available on
[GitHub Releases](https://github.com/benletchford/systemless/releases) for macOS,
Linux (GNU), and Windows (MSVC), each on x86-64 and ARM64. Linux binaries require
glibc 2.35 or newer and the ALSA runtime library (`libasound2` on Ubuntu 22.04).

Or build and install from crates.io:

```sh
cargo install systemless
systemless path/to/app-or-game.sit
```

Systemless accepts StuffIt archives, MacBinary files, and raw/macOS resource forks.
Archives may contain multiple files; Systemless populates the in-memory VFS and
selects an executable resource fork from the archive.

Systemless does not ship applications, games, Mac ROMs, or Apple system software.
Use legally obtained application archives.

For a local checkout, use `cargo run --release -- path/to/app-or-game.sit`.

## Try it in your browser

| [Marathon](https://systemless.org/marathon) | [Escape Velocity](https://systemless.org/escape-velocity) |
| :---: | :---: |
| [![Marathon running in Systemless](.github/assets/marathon-gameplay.png)](https://systemless.org/marathon) | [![Escape Velocity running in Systemless](.github/assets/escape-velocity-gameplay.png)](https://systemless.org/escape-velocity) |

Play these and more classic Macintosh games in your browser at
[systemless.org](https://systemless.org/).

## How it works

Systemless executes classic 68K code with the
[`m68k`](https://crates.io/crates/m68k) crate and native 32-bit PowerPC code
with the [`ppc`](https://crates.io/crates/ppc) crate. Native builds enable
m68k's Cranelift JIT for eligible hot traces, while WebAssembly uses its
portable trace executor.

68K and PowerPC are execution formats, not separate Macintosh platforms. Both
participate in one coherent Macintosh world containing the guest memory map,
system services, processes, tasks, and Toolbox state. Architecture-specific
gateways preserve observable 68K trap and PowerPC CFM behavior before
converging on canonical Macintosh service implementations in Rust.

The runtime is converging on one logical Macintosh process: both CPU adapters
already use one address-routing authority, explicitly identified task-owned
Mixed Mode continuations, and shared authorities for migrated services such as
the process clock, ordinary handle allocation, and Trap Manager. Other Toolbox
managers still contain explicitly tracked compatibility projections while their
two ABI paths are moved onto one semantic operation at a time. The
Trap Manager additionally requires registered system-memory provenance before
either ABI may mutate a protected permanent patch chain; matching bytes in
application memory never grant that capability. The
fat-application Toolbox showcase enforces cross-architecture behavior by
running the same interaction sequence through both slices and requiring
identical semantic state and rendered checkpoints. Mixed Mode transitions must
not copy or reconcile process-visible state, just as original software expects.

```text
                         APPLICATION
                              │
                ┌─────────────┴─────────────┐
                │                           │
             68K CODE                  PowerPC PEF
                │                           │
                ▼                           ▼
          68K ABI gateway             PPC ABI gateway
                │                           │
                └─────────────┬─────────────┘
                              ▼
                       Execution kernel
                  UPP / ProcInfo / continuations
                              │
                              ▼
                    ┌───────────────────┐
                    │ Macintosh world   │
                    │ guest memory      │
                    │ system services   │
                    │ processes/tasks   │
                    │ Toolbox services  │
                    └─────────┬─────────┘
                              │
                              ▼
                       Host presentation
                    macOS / Web / other hosts
```

### Architecture contract

The runtime is converging on these principles:

- Every guest-visible fact has one semantic authority. Guest structures such
  as menu records, windows, PixMaps, handles, low-memory globals, and trap-table
  entries remain authoritative in guest memory when original software can
  inspect or modify them directly.
- Machine-, process-, task-, CPU-engine-, and host-scoped state are modeled at
  their proper lifetimes rather than collected into one monolithic process
  object.
- Both CPU engines observe one authoritative guest memory map. Different
  memory interfaces and optimized views are allowed, but writes never require
  a cross-architecture synchronization pass.
- CPU gateways decode arguments, preserve guest-visible trap or import routing,
  and encode results. Architecture-independent Toolbox semantics live in one
  Macintosh service implementation.
- Host menus, framebuffers, audio devices, and persistence are derived
  presentation or policy layers. They do not become the source of guest truth.

State is deliberately scoped:

| Scope | Examples |
| --- | --- |
| Macintosh world / machine | Guest memory, system mappings, volumes, clock, devices, display, input, and audio environment. |
| Process | Application heap and resources, open sessions, UI objects, and process-specific trap state. |
| Task | Event delivery, Thread Manager state, callbacks, suspended calls, and continuation stacks. |
| CPU engine | Registers, ABI conventions, execution caches, CODE/PEF metadata, and PowerPC TOC state. |
| Host | Native menus and windows, textures, audio output, browser input, and save-storage policy. |

### Mixed Mode and callbacks

Universal Procedure Pointers, RoutineDescriptors, and ProcInfo allow either
architecture to call the other without the caller knowing which ISA implements
the destination. Mixed Mode transitions are task continuations: the execution
kernel suspends one engine, marshals the original Macintosh ABI, runs the target
engine, and resumes the caller with its expected result layout.

Toolbox services may themselves invoke guest procedures. The target service
contract models menu definition procedures, window and control definitions,
event handlers, timers, sound callbacks, and asynchronous completions as
resumable operations. A service releases its runtime state before guest code
runs and continues when the task returns, allowing callbacks to alternate
architectures without making either CPU the permanent host.

The fat-application Toolbox showcase runs the same interaction sequence through
both executable slices and requires matching semantic state and rendered
checkpoints. Focused Mixed Mode tests additionally exercise shared memory,
nested cross-ISA calls, trap patches, and callbacks within one live Macintosh
environment.

## Status

Systemless is focused on real classic Macintosh applications that use the Mac
Toolbox, whether they contain 68K CODE resources or native PowerPC PEF/CFM
fragments. The HLE covers the major runtime surfaces needed by interactive
software:

- Memory Manager handles, pointers, zones, low-memory globals, and common
  exception paths.
- Resource Manager, Segment Loader, File Manager calls, and an in-memory
  HFS-like VFS with data and resource forks.
- QuickDraw ports, regions, text, shapes, PICT, CopyBits, color tables,
  offscreen GWorlds, cursors, and 1bpp/4bpp/8bpp framebuffers.
- Event, Menu, Window, Control, Dialog, TextEdit, Cursor, Process, Sound,
  Standard File, SANE, and common Toolbox utility traps.
- Cooperative Thread Manager contexts, yielding, current-thread queries,
  critical sections, and thread-entry result delivery.
- Sound Manager playback, channel state, command queues, callbacks, file
  playback, and host audio mixing.

It is not a bit-perfect Mac hardware emulator. Hardware-specific services such
as slot interrupts, device queues, removable-media behavior, and multi-process
system integration are modeled only where guest-visible behavior matters.

## Desktop Runner

The installed `systemless` command opens a window, renders the guest framebuffer,
maps keyboard and mouse input, and enables audio when a host backend is
available.

Common runner options:

```sh
systemless --headless --max-instructions 5000000 path/to/app.sit
systemless --arrows-as-numpad path/to/game.sit
systemless --display-scale 2 path/to/game.sit
systemless --ui-theme classic-system7 path/to/game.sit
systemless --fullscreen path/to/game.sit
```

Desktop windows automatically open at a comfortable size for the display,
including Retina and other high-DPI screens. Games keep their aspect ratio,
and the initial window fits within the monitor. Use `--display-scale` with an
integer from 1 through 8 to override automatic sizing with an exact physical
guest-to-host pixel ratio (`1` selects 1:1). `--fullscreen` starts the guest in a borderless fullscreen space.
On systems where macOS selects direct scan-out for the fullscreen surface this
measurably reduced pointer-to-screen latency in testing (see issue #1050); the
benefit depends on the machine and compositor state and is not guaranteed.

The default `classic-system7` guest chrome uses classic Macintosh presentation,
control geometry, and metrics. The optional Systemless theme remains available
with `--ui-theme systemless-default`.

The desktop runner uses the canonical machine profile automatically. On macOS,
guest menus are mirrored into the native menu bar and the guest's application
name and icon are integrated with the Dock. Other platforms render the classic
menu bar according to the guest application's own visibility state.

Desktop saves are stored next to the launched archive under
`.systemless/saves/<archive-name>/`. For example, launching
`/Games/EV Override 1.0.1.sit` restores and persists saves under
`/Games/.systemless/saves/EV Override 1.0.1/`. The store preserves Mac data and
resource forks and is kept separate from the original archive.

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
| `loader` | 68K CODE resource and PowerPC PEF/CFM loading, relocation, and launch setup. |
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

By default, Systemless renders its own original bitmap fonts. Every default
glyph is authored for this project — hand-drawn as ASCII art in `src/quickdraw/fonts/pixel_font/` and
lowered to static glyph tables by `const fn` at compile time; there is no
external font file or offline baker for the default renderer. The crate also
contains optional URW Core 35 TrueType assets for the experiment below.

The faces are named after Australian native plants. The classic Mac font names
survive **only as internal compatibility identifiers** so that classic
applications requesting a family by name or ID still resolve to a sensible face
— this is nominative use, not branding.

| systemless face | Kind                   | Stands in for (compat family, font ID) |
|-----------------|------------------------|----------------------------------------|
| **Jarrah**      | Heavy system / UI sans | Chicago (0) |
| **Kurrajong**   | Humanist body sans     | Geneva (3), Application (1), Helvetica (21); Venice (5), London (6), Cairo (11) |
| **Mallee**    | Monospace              | Monaco (4), Courier (22) |
| **Ironbark**    | Serif                  | New York (2), Palatino (16), Times (20) |

Sizes: Jarrah 9/12; Kurrajong 9/10/12/14/18/24 (+ Application 12, Helvetica 12);
Mallee 9/10/12; Ironbark 12/14/18 (with 2× scaling for 24).

Every face is hand-drawn glyph by glyph in a consistent house style, with
advances, side-bearings and x-height / cap height conformed to the original Mac
strike so classic text lays out identically. Kurrajong 24 and Ironbark 18 are
heavy display cuts matching their originals' bold weight. Venice (5), London (6)
and Cairo (11) render as Kurrajong — the reference System has no strike for
those families and substitutes the application font, which Systemless mirrors.

If `SYSTEMLESS_ORIGINAL_FONTS_DIR` is set, Systemless can also load locally
generated bitmap override blobs ahead of the built-in catalogue.

### Experimental URW TrueType fallback

The opt-in `experimental-urw-fonts` feature substitutes bundled URW Core 35
TrueType faces at 1–96 points. Guest font resources and local overrides retain
precedence. Skrifa/Zeno rasterizes the outlines without installed host fonts.

The substitutes change logical widths and wrapping. A separate experimental
8-bit presentation API renders supported text at 2×–4× while preserving guest
pixels; desktop and browser frontends do not enable it automatically. See the
[font source and licences](src/quickdraw/fonts/urw/README.md) and
[Toolbox Showcase gallery](tests/toolbox-showcase/urw-experiment/README.md)
for reproduction, supported paths, and review limitations. Tracks
[#673](https://github.com/benletchford/systemless/issues/673).

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
| `SYSTEMLESS_TRACE_LOAD` | Logs archive, VFS, resource, and startup loading diagnostics. |
| `SYSTEMLESS_TRACE_LOADSEG` | Logs Segment Loader jump-table patching. |
| `SYSTEMLESS_TRACE_TRAP_COUNTS` | Prints trap dispatch frequency summaries. |
| `SYSTEMLESS_SOFTWARE_CURSOR` | Restores the composited guest cursor overlay instead of the hardware pointer (macOS). |

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

The open-source Systemless emulator/runtime is licensed under
GPL-3.0-or-later.

Some components have additional component-specific licensing, including the
original Systemless bitmap fonts under the SIL Open Font License 1.1.

See [LICENSING.md](./LICENSING.md), [LICENSE](./LICENSE), and
[OFL.txt](./OFL.txt) for details.
