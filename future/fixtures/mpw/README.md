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


The GPU drawing probe now creates a TextEdit record, installs its resource glyph,
uses TEUpdate on a partial rectangle, checks A1/41 glyph rows with untouched rows
outside the update, and verifies current port and pen restoration before disposal.
Both architectures then complete the same final pixel image and success marker.

The TextEdit probe also calls TESetSelect for an inactive plain range and an
empty styled record, checking endpoint clamping through both compiled ABIs.
Active selection painting and editing commands remain unsupported.

The GPU drawing probe now selects and deletes its TextEdit glyph, checks empty
text and line/selection fields, exact cleared view pixels with preserved row
padding, and preserved current port/pen fields on both compiled architectures.

The drawing fixture also activates its TextEdit record, verifies an exact XOR
highlight and stable repeat update, moves to a caret, deletes while active,
checks the empty-record caret, toggles both TEIdle phases using live CaretTime,
and deactivates. Both compiled CPU ABIs complete the same final image.

The drawing probe now posts/retrieves a keyDown event and feeds its byte to
TEKey. It checks insertion and active caret pixels, left/right movement,
replacement, backspace and final cleanup on both compiled CPUs. This validates
guest event-to-edit behavior; host keyboard input remains unfinished.

`menus.c` and `menus.r` initialize windows/menus with a compiled mctb 0, check
shared low-memory handles and the desktop's top boundary, assert exact menu-bar
borders/corners and preserved pixels below the bar, copy/dispose GetMenuBar
snapshots, clear/reinitialize without changing manager handle identities, and
verify resource bytes and retained application port state. The optional test runs
both compiled CPUs through a real GPU and compares final screen bytes. Resource-backed title drawing is also exercised; popup rendering, callbacks and
tracking are not exercised.

The menu fixture also initializes/hides/shows the cursor, loads its compiled
CURS -123 resource, sets it while hidden, and balances the hide depth. The paired
acceptance test composites that image through wgpu after exit and checks exact
RGBA cursor pixels while packed guest screen pixels stay unchanged. Windowed
smoke completed guest execution but did not finish presentation/resize on this
host; actual onscreen pointer behavior is still unverified.

`dialogs.c` verifies InitDialogs globals, preserved ANumber, ParamText handle
payloads, NIL/empty replacement, font/sound pointer setters and reset/cleanup
on both CPUs. PPC links Apple's bundled CarbonAccessors.o for GetParamText,
which executes the shared indexed LMGetDAStrings and BlockMoveData routes. That
object is PPC-only; 68K uses a guest loop over the documented low-memory
StringHandles. Callback invocation, alert sound and dialog rendering are not
exercised. The optional test requires the MPW build but no GPU.

`environment.c` checks the shared virtual SysEnvRec, supported/nonpositive/newer
version behavior, actual low-memory application heap bounds, MaxApplZone and
GetApplLimit, then preserves a pointer's bytes across another maximize. Both
compiled CPUs post the success marker without a GPU. The profile's capability
flags describe the current rebuild; no complete System 7 support is implied.

The window fixture also exercises SetRect/OffsetRect/InsetRect, OS versus
Toolbox update events, repeatable EventAvail/GetNextEvent/WaitNextEvent delivery,
BeginUpdate clipping, EqualRgn restoration and invalidation surviving EndUpdate
on both CPUs. These offscreen fixtures do not prove original app or onscreen
window compatibility.

The environment fixture also calls PBGetCatInfoSync for the virtual root, the
indexed application and an out-of-range index. It checks returned catalog IDs,
parent IDs, directory count, name, available fork lengths and ioResult on both
CPUs, including the PPC data-only application.

The environment fixture now creates a parent and nested directory through
PBDirCreateSync, queries their IDs/parent/count/name, rejects duplicate creation
and resolves a colon path on both compiled CPU targets. This modifies only the
process-local virtual catalog.

The environment fixture checks PBHGetFInfoSync's application metadata, flat
PBGetFInfoSync's root lookup, and missing named files inside a created directory.
Both compiled CPU targets use MPW's HParamBlockRec layout.

windows.r supplies a visible noGrowDocProc WIND resource. The paired compiled
window test checks missing resource NIL, resource-driven creation, bounds,
refCon, goAway flag, and disposal in addition to its plain-window/update cases.
The PPC loader receives the same compiled resource fork as the CODE fixture.

The resource/memory probe detaches a mutated DATA handle, checks unchanged
payload identity, reloads original bytes under a new handle, rejects Resource
Manager operations on the detached handle, then disposes it without affecting
the reloaded resource. Both MPW CPU targets exercise this lifetime.

The window fixture now exercises push buttons, checkboxes and radio buttons
using NewControl and the modern MPW header routine names. It checks control
list ordering/record fields, PPC's ninth stack argument, reference/title/value
updates, hidden/inactive hit tests, guest-written value redraw, disposal
unlinking and automatic freeing when the owner window is disposed.

The window fixture also sets/gets WRefCon, resizes its resource-defined window,
hides/reveals it with ShowHide and redraws existing controls. Native 68K trap
words and PPC imports are verified through the real headers/compiler.

The window acceptance guests also call SystemTask with no periodic registrations,
set/reset PenSize and inspect the live GrafPort dimensions, and use RectInRgn
on the visible region. Both compiler outputs exercise the shared services.

The window probes load PICT 322 through GetPicture and GetResource, compare
handle identity/size, release it, and verify PortSize/MovePortTo fields in a
temporary GrafPort before restoring the window. This verifies setup and loading;
the current probes additionally draw a generated bitmap PICT and inspect
canonical guest screen bytes. Both CPU builds must pass those pixel assertions.

The window guests copy part of the generated PICT drawing within the window's
BitMap using CopyBits and inspect the destination screen bit. This adds real
compiler ABI/guest-pixel coverage on both CPUs.

The window probes draw a thick patXor line, verify a changed screen bit, draw it
again to restore the bitmap, and check relative Line pnLoc advancement. Pen
size/mode/location are reset before continuing the other window checks.

The same window guests also erase/invert/frame/fill rectangles, inspect changed
and restored screen bits, and verify that FillRect stores its pattern in the
live fillPat field.

PICT 323 is an indexed version-2 generated resource with an explicit sparse
ColorTable, default-highlight metadata and PackBitsRgn mask. The window probes
inspect black and white screen bits after drawing it; source index 255 is white
and index 7 is black, requiring palette interpretation rather than index-value
thresholding.

The TextEdit fixture also calls TETextBox with bold centered text and an empty
right-aligned erase on both compiler targets, checking preserved pen location,
text face and MemError. Its acceptance now requires a real wgpu adapter and both
TextBox service calls before the final success marker.

The same TextEdit compiler guest now changes the live Mouse point, calls GetMouse,
and checks its local coordinates against portBits.bounds. It also verifies
PtInRect's inclusive upper-left and exclusive bottom/right edges. Rust acceptance
requires one GetMouse and three PtInRect calls on each architecture.

The window fixture now checks FindWindow content/menu/frame results and exact
output pointers, FindControl button/radio/miss parts, non-consuming StillDown
behavior before/after a queued mouse-up and SetPort(nil)/GetPort restoration.
Both compiler guests must execute all new queries before their success marker;
no visual/native compatibility is inferred from those fixture checks alone.

The environment fixture also creates a file under Prefs, opens data/resource forks,
writes and reads actual bytes, checks EOF/marks, seeks/truncates/flushes, closes and
reopens, and verifies Finder metadata and retained fork lengths. The acceptance
host requires every operation before the final success marker on both real CPUs.
Devices.h is required for the compiler's register-based PBReadSync/PBWriteSync/
PBCloseSync declarations; Files.h alone permits erroneous implicit declarations.
Root-relative catalog calls explicitly reset ioDirID after receiving a file ID.

The menu fixture also builds regular/hierarchical list partitions with guest-owned
menu handles, verifies duplicate-ID precedence, relocation/live ID changes, nil
misses and clearing without disposing caller-owned menus. The environment guest
checks Gestalt system/processor values against SysEnvirons and the undefined
communications selector. Native Gestalt uses the SDK's A0 result plus A1 store;
PPC uses a result pointer. Acceptance requires eight menu lookups and three capability
queries on each compiler target. The SDK spelling is GetMenuHandle.

The menu fixture now compiles MENU, MBAR and a tiny FONT strike. Both CPUs load
and measure menus, preserve GetResource identity, copy/install/dispose a bar,
check title pixels and handle a missing child resource without replacing the live
bar. The original application comparison baseline remains legacy Systemless.

The menu guest also counts live items, appends named TEST resources with default
attributes and checks repeated-append deduplication. It sets/checks/unchecks a mark,
sets a command byte and grows/shrinks item text, verifying the raw record after
each change. It uses SDK-declared AppendResMenu and SetMenuItemText C names.

The menu guest also exercises NewMenu/AppendMenu and verifies PenNormal's size,
mode, pattern and preserved location. The environment guest creates ResourceStore,
checks duplicate ResErr and unchanged current resource reference, then uses File
Manager calls to read the real empty-fork header. Pattern values use SDK structure
addresses. This is separate from writable Resource Manager map/open acceptance.

The menu guest now requires live GetMenuItemText bytes and a full polygon cycle:
OpenPoly/LineTo/ClosePoly, visible port state, bounding box, paint/invert/fill/frame/
erase, offset, balanced pen visibility and disposal. Both compiled CPUs execute
these through the common manager and GPU services before emitting success.

The tracking fixture compiles for both CPUs with ControlDefinitions.h constants.
It leaves the caller port nil, tracks a standard button while the host holds,
drags out/reenters and releases, verifies the returned part and unchanged value,
then disposes its control/window and exits. The menu fixture uses DisposeMenu for
its NewMenu allocation.

The environment guest also requires five NumToString/StringToNum calls per CPU,
checking the signed minimum, zero, low-nibble CAT parsing and negative overflow.
The public SDK wrappers exercise native Pack7 register results and PPC output
pointers through the same shared service.

The drawing guest calls CopyMask with independent source/mask BitMaps and checks
masked result bytes on every row plus unchanged destination row padding. The
same compiled call must execute through both CPU adapters before fixture success.
