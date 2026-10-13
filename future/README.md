# Systemless future

An independent replacement runtime on `dev/future`. One guest address space and
service dispatcher serve real 68K and PowerPC engines. Indexed drawing, palette
expansion and presentation use wgpu. This build does not link the legacy runtime.

**Current scope: executable synthetic fixtures, not original Macintosh apps.**
The private fixture gateways are not Toolbox trap/CFM ABI implementations.
See [architecture](docs/architecture.md) and [progress](docs/progress.md).

From the repository root:

```sh
cargo test --manifest-path future/Cargo.toml --locked
# Explicit GPU acceptance; requires a real adapter and fails if unavailable.
cargo test --manifest-path future/Cargo.toml --locked -- --include-ignored --nocapture
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

The default test command reports GPU tests **ignored**. They are not counted
as passed until explicitly run. Native blocking GPU synchronization is the
first correctness boundary; browser integration and optimization are pending.
