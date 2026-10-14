# Systemless future

An independent replacement runtime on `dev/future`. One guest address space and
service dispatcher serve real 68K and PowerPC engines. Indexed drawing, palette
expansion and presentation use wgpu. This build does not link the legacy runtime.

**Current scope: generated fixtures, an MPW-compiled resource/handle app on both
68K and PPC, event-queue and cooperative-wait programs on both architectures, and a compiled
PPC nonlocal-jump/main-return probe. A compiled resource/drawing/event app also
passes on both CPUs with exact GPU pixel parity. No original
application verified.**
Resource-fork/CODE and PowerPC PEF loaders use a small real Macintosh ABI slice
for resources, handles, OS event queues, bounded waiting, Basic QuickDraw port
initialization/clipped rectangle painting, timing and exit. The graphics fixtures still use private
gateways, which are disabled for loaded applications.
See [architecture](docs/architecture.md) and [progress](docs/progress.md).

From the repository root:

```sh
cargo test --manifest-path future/Cargo.toml --locked
# Explicit GPU acceptance; requires a real adapter and fails if unavailable.
cargo test --manifest-path future/Cargo.toml --locked --test contracts -- --include-ignored --nocapture
cargo run --manifest-path future/Cargo.toml --locked
cargo run --manifest-path future/Cargo.toml --locked -- --ppc
cargo run --manifest-path future/Cargo.toml --locked -- --window
cargo run --manifest-path future/Cargo.toml --locked -- --window --ppc --smoke
cargo clippy --manifest-path future/Cargo.toml --locked --all-targets -- -D warnings
```

The window shows an intentionally tiny 8×8 indexed fixture scaled with nearest
sampling. A guest store writes index 5, then a shared GPU XOR operation applies
index 3 to a 4×4 rectangle. The first pixel becomes index 6. Clicking swaps two
palette entries as a host presentation demonstration. Resizing changes only
the host viewport. `--smoke` verifies presentation, physical resize to 640×360,
and the zero-size suspension branch, then exits.

The default test command reports GPU and MPW compiler tests **ignored**.
Build and run the compiler fixture using [its instructions](fixtures/mpw/README.md). They are not counted
as passed until explicitly run. Native blocking GPU synchronization is the
first correctness boundary; browser integration and optimization are pending.

Generate and run the real-format resource/handle fixtures:

```sh
cargo run --manifest-path future/Cargo.toml --locked --example make_load_fixtures -- /tmp/systemless-load
cargo run --manifest-path future/Cargo.toml --locked -- --code /tmp/systemless-load/resources.rsrc --budget 1000
cargo run --manifest-path future/Cargo.toml --locked -- --pef /tmp/systemless-load/application.pef --resources /tmp/systemless-load/resources.rsrc --budget 1000
```

The generator also writes `lifecycle.pef`; run it with `--pef` to exercise
initializer, main and cleanup, returning 7 after 17 instruction boundaries.

The resource fixtures both load a DATA resource, inspect its handle, allocate cleared memory and exit.
Supported loading is near-model CODE and PowerPC PEF, including pattern data and
relocations. PEF initialization and termination run with bounded execution and a guest
initialization block. Compressed resources, far-model CODE and unknown required
imports fail explicitly. Guest file I/O, Toolbox window/high-level event synthesis, mouse regions, the remaining
QuickDraw operations, movable/purgeable handles and original-app compatibility
remain pending. The rebuild goal is incomplete.

Loaded applications can yield `Stop::Waiting { ticks }` from WaitNextEvent.
Embedding hosts inject events and call `elapse_ticks` before resuming; waiting
polls do not execute instructions. The file CLI advances virtual time to each
wake deadline while retaining one total instruction budget. The bounded trace
example reports a wait and returns control to its caller.

The compiled drawing acceptance harness explicitly supplies an 8x4 GPU target;
the file CLI does not yet prepare a graphics device for loaded apps. Basic
QuickDraw currently describes a fixed 512x342 monochrome screen and supports
matching-size offscreen GPU bitmaps. Nonrectangular clipping, color ports,
recording, custom QDProcs, windows and full graphics compatibility remain pending.
