# Compiler-generated application probe

This ordinary C application reads a DATA resource, checks its handle size,
allocates cleared memory, disposes it and exits. It uses the existing pinned MPW
toolchain and standard startup libraries, so it exposes requirements absent from
hand-assembled programs. It is a development probe, **not a passing compatibility
fixture**. Generated binaries stay in ignored `future/target/mpw-probe`.

From the repository root, build the compiler image if needed and generate forks:

```sh
docker build -t systemless-toolbox-showcase-mpw -f tests/toolbox-showcase/Dockerfile tests/toolbox-showcase
sh future/fixtures/mpw/build.sh
cargo run --manifest-path future/Cargo.toml --locked -- --code future/target/mpw-probe/probe.68k.rsrc --budget 10000
cargo run --manifest-path future/Cargo.toml --locked -- --pef future/target/mpw-probe/probe.ppc --resources future/target/mpw-probe/probe.ppc.rsrc --budget 10000
cargo run --manifest-path future/Cargo.toml --locked --example trace_application -- code future/target/mpw-probe/probe.68k.rsrc 100
```

Observed on 2026-10-14: compilation and fork generation succeed. The 68K startup
pops the loader's return sentinel into A0 and later reads address 0x8100, faulting
at startup PC 0x01000456. This requires investigation of MPW's launch environment,
not mapping arbitrary bytes over the fault. PPC preparation stops at the first
unsupported required import `StdCLib:_BreakPoint`. Neither application reached
its C main or passed the resource/handle checks. Inspect the startup libraries
and authoritative launch contracts before implementing the missing semantics.
