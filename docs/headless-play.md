# Structured headless play

Use a JSON scenario to reproduce guest interactions, capture named screenshots,
and check behavior without opening a window. The command uses the same simulated
frontend scheduler as `--headless --max-ticks`, including retained Toolbox waits
and audio callbacks. It is part of the ordinary `systemless` executable.

From a source checkout:

```sh
cargo build --locked --profile ci-test --bin systemless
run_dir=$(mktemp -d)
target/ci-test/systemless tests/toolbox-showcase/toolbox-showcase.sit \
  --headless --prefer-classic-68k \
  --play-script tests/play/toolbox-showcase.json --play-output "$run_dir"
```

The Toolbox Showcase archive is an original source-built fixture already in the
repository. No commercial application, ROM, or system disk is needed for this
example. A normal installed executable uses the same flags. The executable
requires the default `gui` build feature, even when running headlessly.

For another application, supply its archive as the positional argument and your
own scenario with `--play-script`. Both can live outside the checkout. Use a new
or empty output directory for every run; the default is `play-output` in the
working directory. The runner does not import or persist adjacent `.systemless`
saves. State packaged inside the archive is still present.

PowerPC is the default for fat applications. Select `--prefer-classic-68k` to
exercise their 68K slice, or `--prefer-powerpc` explicitly for PPC. For an archive
with several executables, set `SYSTEMLESS_LOAD_EXECUTABLE` to the exact name.
`--screen-depth` and `--ui-theme` select the same display settings as ordinary
headless execution. Record these settings and the archive hash with a bug report.

## Script contract

```json
{
  "version": 1,
  "clock": "frontend_ticks",
  "max_ticks": 600,
  "reference_tolerance": 0,
  "actions": [
    { "type": "run", "ticks": 120 },
    { "type": "screenshot", "name": "before" },
    { "type": "mouse_down", "v": 150, "h": 200 },
    { "type": "run", "ticks": 2 },
    { "type": "mouse_up", "v": 150, "h": 200 },
    { "type": "run", "ticks": 30 },
    { "type": "screenshot", "name": "after" }
  ]
}
```

Actions execute in order. `max_ticks` bounds the total number of simulated
frontend ticks, including waits and audio assertions. A frontend tick is about
1/60.15 second; it keeps moving when menu tracking freezes guest TickCount.
The Mac startup time is fixed; use `--headless-start-time SECONDS` to override
Mac-epoch seconds (since 1904). Host time is not an input pacing clock.

The `clock` field is required. Old guest-tick/instruction-count scenarios are
not accepted implicitly. Re-author their timing and verify checkpoints after
conversion. Unknown fields are errors, preventing silently ignored assertions
or configuration. Do not combine this mode with `--max-ticks`, either plain
text input-script flag, instruction-budget mode, debugger sockets, or preference
resetting.

| Action | Fields and behavior |
| --- | --- |
| `run` | `ticks`: advance this many frontend ticks. |
| `mouse_move`, `mouse_down`, `mouse_up` | `v`, `h`: vertical and horizontal guest coordinates. |
| `key_down`, `key_up` | `key`, `ch`: numeric Macintosh virtual key code and character byte, each 0–255. For Return use 36 and 13; Space uses 49 and 32. |
| `screenshot` | `name`: unique letters/digits/underscore/hyphen identifier; writes `NAME.png`. `failure` is reserved. |
| `run_until_pixel` | `x`, `y`, `rgb: [r,g,b]`, positive `timeout_ticks`; optional `not` and per-channel `tolerance`, default false/0. Polls each frontend frame. |
| `assert_pixel` | `x`, `y`, `rgb`; optional `not` and `tolerance`. Tests the current rendered pixel without advancing. |
| `assert_memory_word` | Aligned numeric `address`, expected 16-bit `value`; reads a guest big-endian word. |
| `assert_audio_during` | Positive `ticks`, `min_non_silent`: advances while collecting audio and checks samples differing from unsigned-PCM silence (128). |
| `log` | `message`: diagnostic text. |
| `milestone` | `name`: diagnostic label with the current guest tick. |

Mouse input is **v, h**; pixels are **x, y**. For held keys or buttons, put a
`run` between down and up. Merely queuing input does not prove that a guest
processed it; advance and assert the resulting state.

## Results and comparisons

`report.json` contains `status`, an error and zero-based `failed_action` on
failure, frontend/guest clocks, assertion count, checkpoint names, reference
pixel differences, and instruction-budget exhaustion counts. Failed execution
also attempts `failure.png`. Malformed scripts and archive load errors produce
a failed report when the output directory can be created. Nonempty output
folders are rejected without modifying their contents.

Exit status is nonzero for parse/load errors, exceeded budgets, halted guests,
wait timeouts, failed assertions, or failed reference comparisons. A passed
scenario with no assertions only proves its actions completed. Budget exhaustion
counts warrant investigation even if a scenario passes.

To compare with an independently captured set:

```sh
systemless /path/to/application.sit --headless \
  --play-script /path/to/scenario.json --play-output /path/to/new-run \
  --play-reference /path/to/reference
```

Every named screenshot requires exactly `reference/NAME.png` with the same
dimensions. Every pixel must match within `reference_tolerance` (default 0).
Missing files fail. No alternate-frame search, auto-updating baselines, or
synthetic oracle traces are used. A repeated Systemless run can check determinism,
but is not independent oracle evidence. Keep captures as local artifacts unless
a focused test change explicitly needs a reviewed redistributable reference.

## Optional BasiliskII and SheepShaver capture

The source-only adapters in [tools/play-oracle](../tools/play-oracle/README.md)
run independently of the main Cargo workspace. They require Docker, host archive
utilities, and system media supplied by the user. Ordinary play runs do not need
these dependencies. Their scripts use `clock: guest_ticks` (or explicit SheepShaver `wall_time`), named
key strings, and screenshot `path` fields. Align observable scenes before
comparing their PNGs with frontend-tick captures; identical numbers of ticks do
not establish identical execution. No trace-parity claim follows from matching
screenshots.

The repository agent skill is
[systemless-headless](../.agents/skills/systemless-headless/SKILL.md).
