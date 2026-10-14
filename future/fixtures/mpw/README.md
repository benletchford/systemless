# Compiler-generated application probe

This ordinary C application reads a DATA resource, checks its handle size,
allocates cleared memory, disposes it and exits. It uses the existing pinned MPW
toolchain and standard startup libraries. The 68K build now passes this complete
interaction; PPC still needs StdCLib support. Generated binaries stay in ignored
`future/target/mpw-probe` and are not required by the default test suite.

From the repository root, build the compiler image if needed and generate forks:

```sh
docker build -t systemless-toolbox-showcase-mpw -f tests/toolbox-showcase/Dockerfile tests/toolbox-showcase
sh future/fixtures/mpw/build.sh
cargo run --manifest-path future/Cargo.toml --locked -- --code future/target/mpw-probe/probe.68k.rsrc --budget 10000
cargo run --manifest-path future/Cargo.toml --locked -- --pef future/target/mpw-probe/probe.ppc --resources future/target/mpw-probe/probe.ppc.rsrc --budget 10000
cargo run --manifest-path future/Cargo.toml --locked --example trace_application -- code future/target/mpw-probe/probe.68k.rsrc 100
```

Verified on 2026-10-14: the 68K build completes standard startup and C main in
1308 instruction boundaries, with 18 service calls. It passes return-code
inspection, trap availability checks, CurResFile, PBGetFCBInfo and the optional
static-constructor resource lookup, then checks its DATA resource and cleared
allocation, disposes the handle and exits. The acceptance test requires the
ordered resource/size/allocation/disposal calls; an early CRT return is not success.

```sh
cargo test --manifest-path future/Cargo.toml --locked --test mpw -- --ignored
```

PPC preparation still stops at required import `StdCLib:_BreakPoint`. Other StdCLib
imports are `__setjmp`, `_IntEnv`, `__C_phase`, `__target_for_exit`, `_exit_status`,
`exit` and `__NubAt3`; several are data symbols and need shared guest storage.
The PPC application has not reached C main. No graphics, events, original
commercial application or broader compiler-runtime compatibility is claimed.
