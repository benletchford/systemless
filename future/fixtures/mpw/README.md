# Compiler-generated application probe

This ordinary C application reads a DATA resource, checks its handle size,
allocates cleared memory, disposes it and exits. It uses the existing pinned MPW
toolchain and standard startup libraries. Both the 68K and PPC builds pass this complete
interaction. A second PPC program checks two nonlocal jumps and returns 37
from main through the standard startup library. Generated binaries stay in ignored
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

The PPC resource/handle build reaches C main and exits successfully after 102
instruction boundaries and 6 service calls. The `jump.c` program additionally
checks the packed standalone argument environment, longjmp results 7 and 0
(the latter must become 1), and normal return from main with status 37.
Tests require all observed jumps and the final CExit(37), not just termination.

StdCLib support is a bounded standalone application subset. Shared guest data
symbols, non-AltiVec jump buffers and C exit are implemented. C exit records
phase/status and invokes the shared CFM termination path. Stdio, atexit,
MPW shell cleanup hooks and interactive debugger support remain unavailable;
unknown required imports and explicit breakpoint calls fail. No graphics,
Toolbox events, original commercial application or broader compiler-runtime
compatibility is claimed.

The `events.c` program passes on both architectures. It checks the default
key-up posting mask, enables all low-level events, posts multiple types, peeks
twice without removing, selectively removes an event, flushes with and without
a stop mask, and directly reads the guest queue header and event elements.
The test requires a final posted success marker before ExitToShell, so returning
an error from 68K main cannot pass. This exercises the Operating System event
queue; it does not claim window activation/update events or WaitNextEvent.

The `wait.c` acceptance program passes on both architectures. It consumes an
already queued event immediately, waits three virtual ticks for a null event,
then suspends for input injected by the Rust acceptance host. Repeated polling
while suspended must not execute guest instructions or dispatch the call again.
The test requires the success marker after all three waits. Running this fixture
with the file CLI alone cannot satisfy the injected-input step; use the MPW test
command above for acceptance.

The `draw.c` program now passes on both CPUs with an explicitly supplied GPU.
It initializes application-owned QDGlobals, checks the nil current port, opens
and switches ports, changes the destination bitmap and clip, loads a resource,
selects gray/black pen patterns, paints/XORs, modifies guest bitmap bytes directly,
and consumes an event before its success marker. The test compares both GPU
RGBA outputs against independently specified bytes, as well as C-side checks of
packed pixels and untouched row padding. This verifies an 8x4 offscreen drawing
slice, not windows, arbitrary applications or complete QuickDraw compatibility.

The build script treats MPW compiler/linker diagnostics as failure even when the
container runner exits zero. Both MPW targets define their own QDGlobals storage;
this toolchain does not export a `qd` data symbol from InterfaceLib.
