---
name: systemless-headless
description: Drive classic Macintosh applications headlessly in Systemless to reproduce bugs, inspect screenshots, and assert behavior using structured play scripts. Use for automated guest interaction and optional emulator comparisons, not ordinary Rust unit tests or interactive desktop performance measurements.
---

# Headless Systemless

Run commands from the Systemless repository root. Read [the play guide](references/headless-play.md) for the JSON action schema, output contract, and oracle setup.

## Start with a known fixture

```sh
cargo build --locked --profile ci-test --bin systemless
run_dir=$(mktemp -d)
target/ci-test/systemless tests/toolbox-showcase/toolbox-showcase.sit \
  --headless --prefer-classic-68k --play-script tests/play/toolbox-showcase.json --play-output "$run_dir"
```

The GUI feature is needed to compile this executable, but headless execution opens no window. Reuse the built executable for subsequent runs. `ci-test` optimizes guest execution without production LTO costs.

Inspect `report.json` and the named PNGs, including `failure.png` on execution failures. A zero exit status and `status: passed` mean the requested actions/assertions completed; they do not establish application compatibility. Inspect the actual images before describing visible behavior.

## Reproduce an application issue

- Use an archive the user supplies. Keep the archive, scratch scenarios, captures, and extracted firmware outside tracked source. Do not add a game scenario collection to this repository.
- Use `--play-script /absolute/path/repro.json --play-output /new/empty/directory`. A scenario can live anywhere; its archive is always the positional CLI argument. Each run starts from the archive's own state and neither reads nor writes adjacent saved files.
- PPC is the default for fat applications; use `--prefer-classic-68k` for the 68K slice or `--prefer-powerpc` explicitly for PPC. Use `SYSTEMLESS_LOAD_EXECUTABLE` to select an executable within a multi-application archive. Record those choices with your findings.
- Use `run_until_pixel` with a finite timeout to synchronize with visible readiness. Use short separate mouse/key down, run, and up actions where the application needs held input. Mouse coordinates are **v, h**; pixel coordinates are **x, y**.
- Put named screenshots before and after the smallest interaction that reproduces the issue. Once verified, add a pixel, memory, or audio assertion. Never guess expected pixel values: inspect a capture or an independently verified reference first.
- `max_ticks` bounds total simulated frontend work. A timeout is a failed run; increasing it is appropriate only when evidence shows legitimate progress. Guest halt, assertion failure, malformed input, and missing media must not be reported as success.

## Clock and evidence boundaries

The JSON contract requires `version: 1` and `clock: frontend_ticks`. At roughly 60.15 Hz, the frontend clock advances even while a retained menu freezes guest TickCount. Old guest-tick or instruction-budget scripts require explicit timing conversion and new verification. Do not compare instruction-mode CPU figures with real gameplay performance.

`--play-reference DIR` compares each named screenshot with exactly `DIR/NAME.png`; missing images, size differences, or differing pixels fail. Tolerance is explicit in the scenario and defaults to zero. A second Systemless run is a determinism comparison, not independent oracle evidence. Never regenerate expected images merely to make a failure pass.

For BasiliskII/SheepShaver captures, read the optional oracle section of the guide first. Those tools use a separate **guest-tick** format and require user-provided system media. SheepShaver also offers explicit `wall_time` capture when its guest clock is unavailable; its sidecars then contain null timing evidence. Their screenshots can supply references after aligning scene, dimensions, fonts, and display depth; their timing does not prove trace parity.
