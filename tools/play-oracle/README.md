# Optional classic Macintosh capture adapters

These developer tools capture independent screenshots from BasiliskII (68K) and
SheepShaver (PPC). They are excluded from the main Cargo workspace and crates.io
package. They include original launcher sources and patches against the pinned
upstream MacEmu revision, not ROMs, system disks, games, or golden captures.
MacEmu patches retain upstream licensing; Systemless tooling is GPL-3.0-or-later.

## Setup

Use a Unix host with Rust, Docker, `unar`, and `hfsutils`. The archive staging
code supports resource-fork metadata and extfs sidecars; macOS is the primary
host environment. Supply a bootable Mac OS disk compatible with the selected
emulator and your own ROM. A fresh temporary disk copy is used for each run.
The tools compile a small startup launcher and install it into that copy.
Temporary working directories are retained for diagnosis under the OS temporary
directory and can contain media; remove them when finished. Run only one capture
per backend at a time because Docker container names are fixed.

Build from this directory:

```sh
cargo build --locked
support/basiliskii/build_play_image.sh
# For PPC captures:
support/sheepshaver/build_play_image.sh
```

The first capture builds the MPW compiler image if it is absent. Its source/compiler inputs
are pinned like the Toolbox Showcase builder. Building images requires network
access; running an already-built capture does not acquire game or system media.

```sh
export SYSTEMLESS_BASILISK_ROM=/path/to/MacRom.rom
export SYSTEMLESS_BASILISK_DISK=/path/to/System_68K.dsk.gz
cargo run --locked --bin play-basilisk -- /path/to/app.sit /path/to/oracle.json /path/to/new-captures

export SYSTEMLESS_SHEEPSHAVER_ROM=/path/to/PowerMac.rom
export SYSTEMLESS_SHEEPSHAVER_DISK=/path/to/System_PPC.dsk
cargo run --locked --bin play-sheepshaver -- /path/to/app.sit /path/to/oracle.json /path/to/new-captures
```

SheepShaver accepts either a raw disk or gzip file; BasiliskII expects gzip.
Both output directories must be empty. The optional third argument defaults to
`basilisk_output` or `sheepshaver_output` beside the scenario.

## Capture scripts

These adapters use guest ticks and **do not accept the Systemless frontend-tick
script contract**. Use bounded pixel waits to align observable checkpoints:

```json
{
  "version": 1,
  "clock": "guest_ticks",
  "actions": [
    { "type": "run", "ticks": 120 },
    { "type": "screenshot", "path": "before.png" },
    { "type": "key_down", "key": "space" },
    { "type": "run", "ticks": 2 },
    { "type": "key_up", "key": "space" },
    { "type": "run", "ticks": 30 },
    { "type": "screenshot", "path": "after.png" }
  ]
}
```

The supported action subset includes `run` with ticks, `run_until_tick`,
`run_until_pixel` (`x`, `y`, `rgb`, optional `not`, `tolerance`,
`timeout_ticks`), mouse down/up/move (`v`, `h`), key down/up (named `key`),
`screenshot` (`path`), `assert_pixel`, `assert_tick_range`, `log`, and `milestone`.
Input and screenshot actions can specify `at_tick` relative to the launch
baseline. Common key names include `space`, `Return`, `Escape`, `left`, `right`,
`up`, `down`, and single letters.

Use `executable` to select an application, `mac_time_secs` for a fixed startup
clock, and `application_partition_size` when the same memory setting is needed
on both sides. SheepShaver accepts `sheepshaver_screen_width`,
`sheepshaver_screen_height`, and `sheepshaver_color_depth`. Captures must have
matching dimensions and content before using them with `--play-reference`.

Memory/audio assertions, frame recording, and byte patches are rejected before
capture rather than silently treated as successful. In `guest_ticks` mode an unavailable live guest clock is an error. The supplied
SheepShaver setup can lose the launcher's clock stream after the application
starts. For screenshot-only work, explicitly choose `"clock": "wall_time"` in
its scenario: `run.ticks` then requests host seconds at 60.15 ticks/second,
while guest-tick scheduling and assertions are rejected. Capture sidecars mark
this mode and set guest ticks and retired instruction counts to null; wall time
is never reported as an observed guest clock. BasiliskII requires `guest_ticks`. The adapters remain diagnostic capture tools: screenshots
and clock observations do not prove instruction or event trace equivalence.

Run media-free adapter tests with `cargo test --locked`. Complete emulator runs
also require the external prerequisites above.
