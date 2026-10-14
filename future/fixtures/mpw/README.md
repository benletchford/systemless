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

The drawing fixture also closes and reopens ports. Its acceptance test requires
three ClosePort calls on each CPU before the final success marker, and checks
that closing an inactive port preserves the current selection.

The fixture also resets a modified port with InitPort and checks that both
region handles retain identity while the current port, bitmap and pen defaults
are restored.

The drawing program also grows and shrinks a guest handle, checking retained
contents, GetHandleSize and MemError through each architecture's real ABI.

The drawing fixture saves and restores its clip using NewRgn/GetClip/RectRgn/
SetClip/CopyRgn/DisposeRgn before pixel verification on both architectures.

EmptyRgn and PtInRgn are checked against empty and rectangular regions, including
right/bottom exclusion, through both native calling conventions.

A complex region built with DiffRgn clips two XOR draws. The program
checks intermediate packed bytes and the restored final image, so a skipped
draw cannot count as success. The fixture also exercises SectRgn, UnionRgn and XorRgn with aliased destinations; native-system comparison remains outstanding.

The PPC probe explicitly exports `main` with PPCLink's `-export` option. The
acceptance test requires its compiler-generated export record, transition-vector
class, and executable target before running the usual resource/handle checks.
This validates a real linker-produced PEF export table; it does not exercise
guest FindSymbol or dynamic CFM connections.

The PPC probe now uses FutureInit as its PEF initializer. It captures the real
connection ID and calls CountSymbols before main. Main checks FindSymbol,
zero-based GetIndSymbol, optional output pointers, case-sensitive lookup and the
one-byte class result with a neighboring canary. Both compiled architectures
verify the invalid-connection error without modifying the count output. This
extends CFM ABI acceptance; it is not a native CFM semantic comparison or a
dynamic fragment load.

The `dynamic.c` PPC application loads `plugin.c` from a `plug` resource. The plugin
has a custom initializer/terminator, exports PluginMain and uses the same
InterfaceLib services as its caller. The application locks the resource, calls
GetMemFragment, counts and finds exports, invokes the returned transition vector,
and verifies the terminator modifies its own stack-local counter. It then closes
the connection, unlocks/releases the resource and posts a required success
marker. The ignored MPW acceptance test checks completion and mapping removal.
This fixture has no 68K dynamic counterpart until actual Mixed Mode is supported.


The drawing fixture also calls Random before selecting a port, checking its
signed sequence, guest QDGlobals.randSeed updates, the -32768-to-zero case and
zero seed behavior on both architectures. Zero remains zero following published
Apple QuickDraw source; legacy Systemless instead reseeds zero to one. Existing
Systemless comparisons are authorized in place of native System 7 checks.


The drawing fixture now checks FrameRoundRect and PaintRoundRect on both CPUs,
including XOR cancellation, asymmetric pen size and unchanged row padding. Its
rounded-rectangle checks include two centered four-pixel rows. Earlier rectangular
and region results remain checked by the guest before the rounded calls.

For an independent legacy geometry comparison (not native Mac validation):

```sh
rustc --edition=2021 future/fixtures/rounded-comparison.rs -o /tmp/rounded-comparison
/tmp/rounded-comparison
```

The standalone harness includes only the legacy span generator for comparison
and is never linked into the replacement runtime. It compares 129,600 filled
shapes and 16,384 frames, without compiling the legacy emulator.


GetPattern now supplies the gray drawing pattern from a signed-ID `PAT ` resource.
The fixture compares GetPattern/GetResource handle identity, checks a missing
ID, draws the resource bytes, and releases the handle. ForeColor/BackColor then
reverse the selected port's black/white values without changing another port.
The final pixel image is the inverse gray pattern: 55 AA 55 AA, with unchanged
row-padding bytes. Earlier drawing results are checked before color reversal.


The resource/memory probe now also allocates a cleared nonrelocatable pointer,
checks its logical size, blocks growth with an adjacent allocation, verifies
failed growth preserves bytes, frees the blocker, grows and shrinks in place,
and disposes both pointers. These checks run through MPW-generated 68K and PPC
calling conventions and inspect MemError after each mutation.


`windows.c` exercises InitWindows and plainDBox NewWindow through both compilers.
It inspects the WindowRecord and local GrafPort, creates two overlapping windows,
checks visRgn membership, draws into the covered window, closes/disposes runtime
storage, reuses static caller storage for hidden windows, and posts a success
marker. The Rust acceptance test checks that both architectures finish with no
linked windows and exactly the same desktop pattern bytes. This does not verify
title-bar WDEFs, update-event delivery or original application interaction.


The drawing probe also selects TextFont, TextSize, TextFace and TextMode on two
ports and checks their independent guest fields. TextFace uses the Universal
Interfaces word-sized StyleParameter argument and byte-sized port StyleField.
These checks do not exercise font loading or glyph drawing.


The drawing probe selects family 3 at size 9 and calls GetFontInfo on an original
synthetic FONT resource (ID 393). It checks ascent 2, descent 1, maximum width 4
and leading -1 on both CPUs. The resource contains tiny test glyphs, not copied
font artwork. Glyph drawing and system-font fallback remain pending.


The drawing fixture includes Fonts.h, runs InitFonts twice, and queries positive
system metrics before selecting its explicit FONT resource. This tests the default
strike and idempotent initialization through both real compilers. The fallback is
an OFL-licensed Nimbus Sans substitute; glyph pixel equivalence is not asserted.


Text acceptance now checks CharWidth/StringWidth, MoveTo, DrawChar and DrawString.
The resource glyph has a negative bearing and exact three-row bits; copy produces
E0 A0 40, XOR cancels it, and drawing preserves pen mode/padding while advancing
pnLoc. A blank fallback space clears its advance cell under srcCopy. Both MPW
architectures complete these checks before the final color/pattern image.


DrawText/TextWidth now use a substring of an ordinary byte buffer in the drawing
probe. Both compiled architectures measure xAB?z at offset 1/count 3 as 10 and
draw/cancel xAz at offset 1/count 1 with the same exact resource-glyph pixels.


`textedit.c` initializes TextEdit, creates plain and styled records, checks
captured port attributes/metrics, style sentinels, nested tables and empty text
handles, queries style/text handles, disposes and creates another record, then
posts its success marker. The acceptance test runs both MPW architectures without
requiring a GPU; it exercises allocation/lifetime rather than editing or redraw.


The TextEdit probe now replaces plain/styled text, checks insertion-point placement,
word-wrapped line starts, recalculates after changing destination width, verifies
styled line-table size/run sentinel, handles explicit CRLF bytes, and empties the
record before disposal. These checks run through both real calling conventions.
Rendering and interactive editing remain the next acceptance slice.
