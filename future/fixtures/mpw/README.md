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

Observed on 2026-10-14: compilation and fork generation succeed. MPW 68K startup
inspects its caller's instruction bytes. Loaded CODE applications now receive a
readable/executable ExitToShell return trampoline instead of an unmapped private
sentinel, and a regression checks both inspection and return execution. Startup
now advances through global initialization to instruction boundary 759, where
it requests GetOSTrapAddress (A346), querying Gestalt (A1AD). The Trap Manager
lookup service and its callable entries remain to be implemented.

PPC preparation stops at required import `StdCLib:_BreakPoint`. The other StdCLib
imports are `__setjmp`, `_IntEnv`, `__C_phase`, `__target_for_exit`, `_exit_status`,
`exit` and `__NubAt3`; several are data symbols and need shared guest storage.
Neither application has reached its C main or passed the resource/handle checks.
Inspect the startup libraries and authoritative trap/StdCLib contracts before
implementing the missing semantics; do not bypass startup or add silent stubs.
