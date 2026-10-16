# Original application reconnaissance

These notes record bounded startup checks and the specific Super Mines
interactions below. They do not establish complete application acceptance. The binaries are external fixtures and are not
redistributed in this repository.

Source archives were read from the local `systemless-games` corpus. Extract with
`unar -k visible`, then unwrap entry type 2 from the emitted AppleDouble `.rsrc`
file to obtain the raw resource fork. The entry table begins at byte 26, has the
big-endian count at byte 24, and contains 12-byte type/offset/length records.
Do not pass the AppleDouble container directly to `--code` or `--resources`.

| Application input | Bytes | SHA-256 |
| --- | ---: | --- |
| Super Mines Demo raw resource fork (`super-mines.sit`) | 518906 | `0a04894b487ed9e42fd6e67f2b8fe17c050a40b0a4f52ee5a768a1ce524b1ec0` |
| Centaurian FAT data fork (`Centaurian_1.2.1.sit`) | 200976 | `ab5e510dce4ea5ecb1fb80a814d658d9f31f7551fd932018299b9df0def3cd65` |
| Centaurian FAT raw resource fork | 1769685 | `e2864ba05272a3d52a8f9f48ef45b8468403f315c4718518c12f5d52f6106d3f` |

Run `cargo run --manifest-path future/Cargo.toml -- --code RAW_RESOURCE_FORK
--budget 100000` for 68K, or `--pef DATA_FORK --resources RAW_RESOURCE_FORK
--budget 100000` for PPC. The `trace_application` example accepts `code`, the raw
fork path and a maximum instruction count, and prints register/opcode context.

## Observations

- Super Mines initially wrote through zero after reading CurStackBase ($0908),
  eventually faulting at $1000. Its CODE 0 declares 6678 bytes below A5. Publishing
  the start of those globals ($003FE5EA) fixes this specific loader defect. The
  original startup now initializes through $00400000, then passes ReleaseResource,
  InitFonts, InitWindows, FlushEvents and InitMenus with graphics enabled. It proceeds
  through InitCursor, TEInit, InitDialogs, SysEnvirons and EventAvail, then enters
  event/mouse polling through GetMouse and PtInRect after
  PBGetCatInfo reports a missing directory, PBDirCreate creates it and
  PBHGetFInfo completes a file lookup and GetNewWindow creates its first
  hidden dBoxProc resource window and DetachResource transfers loaded
  resource ownership, NewControl creates its button-family controls,
  SizeWindow resizes the hidden window, SetWRefCon stores application data,
  MoveWindow positions it, ShowWindow reveals it and InvalRect requests redraw.
  A 100,000-instruction trace completes without an unsupported service.
  A bounded press/release at global (256,171) passes FindWindow, FindControl
  and StillDown, hides/disposes the demonstration splash, restores its saved
  nil port, creates/updates/reads its settings file, loads/installs its menu bar
  draws its menu titles, updates live items, constructs WIND 1000/two controls
  records/draws its polygon indicator and reaches the chooser event loop.
  A recorded-button press then reaches TrackControl ($A968).
  SystemTask preserves the empty periodic service state, PenSize updates live
  GrafPort dimensions and RectInRgn checks content intersections.
  GetPicture loads PICT 202, PortSize sizes its offscreen GrafPort and MovePortTo
  sets the bitmap mapping before drawing. DrawPicture now decodes that v1
  PackBitsRect/srcOr resource and executes through wgpu. CopyBits transfers
  the offscreen images. LineTo now draws through the shared GPU pen path
  and EraseRect clears its requested area before startup reaches a later
  picture containing default-highlight metadata and an 8-bit indexed PixMap.
  PICT 184 now completes table decoding, packed rows, mask and GPU monochrome
  conversion before the application requests a text box.
- Centaurian's 68K path originally faulted writing $010000FC at PC $01000270
  (trace index 92039). The target is a JSR operand containing $00000028; startup
  adds A5 to relocate it to $00400028. Classic CODE now permits writes and these
  relocations complete. Startup now also installs Toolbox replacements for
  LoadSeg ($A9F0), UnloadSeg ($A9F1), and ExitToShell ($A9F4). The next stop is
  unsupported GetGDevice ($AA32) at PC $10007426 (trace index 104863), after
  InitFonts/InitWindows/InitMenus/TEInit/InitCursor/InitDialogs/MaxApplZone
  (graphics enabled), HLock,
  StripAddress and executing resource-loaded code from heap RAM. The prior $A204
  request was Sound Driver Control(-4, 2), setting volume to 3. That state update
  now completes. No window interaction or audio playback is proven.
- Centaurian's PPC load rejects the unresolved strong InterfaceLib import
  `SndDisposeChannel`. TEStyleNew/TENew and TextEdit lifetime, TextFont/TextFace/TextMode/TextSize, NewWindow (plainDBox), InitWindows, GetPattern, ForeColor/BackColor, FrameRoundRect, Random, GetMemFragment and the symbol-query imports now resolve, but no PPC
  application instruction executes because remaining strong imports are still
  validated before launch.

The loader now also publishes CurrentA5 ($0904) and the word CurJTOffset ($0934).
CurStackBase denotes the start of application globals; the current separate,
fixed stack reservation remains an explicit runtime limitation. A generated
instruction regression independently reads these globals and writes through
CurStackBase. This proves the launch contract, not original-app compatibility.

Semantics are documented in Apple's [Memory Manager summary](https://dev.os9.ca/techpubs/mac/pdf/Memory/Memory_Manager.pdf)
(page 2-104) and [Inside Macintosh volumes I–III](https://puyoman.net/documentation/Inside_Macintosh.pdf)
(global variables, III-227). ReleaseResource now invalidates loaded resource identity and permits reloads;
its error behavior and changed-resource retention follow Apple’s
[ReleaseResource contract](https://dev.os9.ca/techpubs/mac/MoreToolbox/MoreToolbox-85.html).
InitFonts ($A8FE) now loads a guest system font or allocates the licensed fallback.
InitMenus ($A930) owns and draws an empty bar through shared guest state/wgpu.
InitCursor ($A850) now initializes shared cursor state for the GPU presentation
overlay. InitDialogs ($A97B) now initializes shared dialog globals and parameter
StringHandles. SysEnvirons now reports the shared compatibility profile; common
loading maps/publishes the full fixed heap and MaxApplZone preserves its extent.
Fresh traces pass EventAvail/SetRect, bold TETextBox, GetMouse and PtInRect in Super Mines; a press/release dismisses the splash and reaches PBHOpen. Centaurian reaches GetGDevice. Actual
onscreen cursor behavior, heap-zone resizing, menu/dialog construction, window
custom definitions and original application interaction remain unfinished.
Dynamic memory loading is verified with the MPW resource-plugin fixture;
Centaurian has not yet executed its own GetMemFragment calls. Full acceptance
still requires original application interactions on both architectures.

Super Mines TextBox calls now complete with the selected family-3, size-9 bold
strike. Layout and clipping use the shared TextEdit/GPU route, with a synthesized
one-pixel bold source. A bounded press/release now dismisses that original splash;
gameplay and complete lifecycle acceptance remain pending. Native styled-text
pixel comparison has not passed. Legacy Systemless
comparison alone is permitted by the user; native System 7 execution is optional.

A reproducible bounded click probe captures PPM images before and after dispatch:

```sh
cargo run --manifest-path future/Cargo.toml --example probe_mouse -- CODE_RESOURCE_FORK /tmp/mines-probe 256 171
```

This diagnostic loads a 68K CODE fork, runs 30,000 startup boundaries, injects
mouse-down, then permits up to 30,000 more boundaries before mouse-up and another
30,000. It propagates service faults; screenshots are evidence, not automatic
acceptance. No original application/resource bytes are added to the repository.

The current probe at (256,171) dismisses the splash, then opens
`Super Mines Demo Settings` with read/write permission in its created preferences
directory. The missing-file path creates both forks, sets Finder metadata, reopens
the data fork and executes EOF/seek/write/read operations. It then queries GetMHandle(4), closes the file and probes Gestalt selectors
ppc-space and evnt, both correctly unavailable. It loads and installs MBAR 1000,
counts menu 1's items, calls AddResMenu('DRVR') and draws the original menu bar.
It changes menu 4's mark, text and command key, reads settings, constructs WIND
1000/two controls, creates menu 5010 and appends four game names. It shows the
window and draws the game/difficulty chooser, including its polygon indicator.
The splash press/release finishes its bounded execution without a service fault.
A second click at (145,150) also completes; this is input delivery and drawing,
not accepted chooser selection. The estimated point (330,230) misses the actual
control rectangle. A press at global (402,231), chosen from live bounds/status,
reaches TrackControl ($A968), post-trap PC $0100EC06, before mouse release.
Confirmation, popup interaction, gameplay and a complete lifecycle remain unverified.
Files remain process-local. Next: actual held-mouse control tracking/callbacks and
resume this chooser interaction, preserving the real CPU reply and input lifecycle.

An earlier record mislabeled decimal 43313 as $A9B1/CreateResFile. It is actually
$A931/NewMenu; resource-file creation is accepted separately through generated and
MPW-compiled guests. Original-app bytes remain outside the repository.
The full rebuild goal remains active.

The diagnostic also accepts additional coordinate pairs. To reproduce the current
tracking boundary, append `145 150 402 231` after the initial `256 171` click.
Every press/release has a separate 30,000-boundary budget. A failed press is not
followed by a release or later click. Multiple-click runs save per-step screens;
error output includes the hexadecimal trap and native stack bytes for continuation.


## 2026-10-15: cooperative standard-control tracking

TrackControl now suspends either real CPU with its ABI reply pending while the
host button is held. Shared control state redraws push buttons, checkboxes and
radio buttons on drag out/reentry and returns the released part or zero outside.
Coordinates use the control owner even when the caller port is nil; checkbox and
radio values remain caller-owned. A queued release uses its saved position and
removes only that event; unrelated and later events retain order. Physical release
also works when mouse-up events are masked. No guest instructions spin while held.
Nil action procedures and the stored-action sentinel resolving to nil are accepted;
nonzero action/custom definition callbacks remain explicit errors. Draw failures
restore hilite and preserve MemErr. DisposeMenu releases caller-owned menu handles
without implicitly deleting menu-list entries or replacing resource ownership rules.

Generated tests cover both ABI frames, delayed result guards, held/drag/release
pixels, disabled/missed controls, cancellation, event ordering, nil caller ports,
callback rejection and drawing rollback. An MPW-compiled fixture on both CPUs
waits for host input, drags out and back, releases, then exits successfully.

The original Super Mines sequence (256,171), (145,150), (402,231) now waits on the
third press and completes its release. It hides/disposes the chooser, disposes its
created menu and opens WIND 1001 titled Super Mines. The partial game window then
stops at Pack7 ($A9EE), post-trap PC $0101264E. Selector word zero at SP $0080FA02,
D0=5 and A0=$0080FA2E identify NumToString. Gameplay, popup interaction and complete
lifecycle acceptance remain unverified. Next: shared signed integer decimal
conversion with the Pack7/PPC ABI boundaries, then continue real game drawing.
Native presentation, original PPC compatibility, production Mixed Mode and the
other documented gaps remain required. The full rebuild goal stays active.


## 2026-10-15: shared integer decimal conversion

NumToString/StringToNum now use one memory-only decimal service. Native Pack7
$A9EE selectors zero/one pop only their selector word, use D0/A0 and preserve the
void formatting result; parsing returns D0. PPC imports receive r3/r4, write the
Pascal string or big-endian long output, and preserve void return registers.
Unsupported SANE selectors remain explicit errors before selector-stack mutation.
Signed formatting handles the complete i32 range and writes only the Pascal
length/content. Parsing snapshots input before overlapping output and wraps
multiply/add/negation at 32 bits. Neither operation allocates guest memory or
changes MemErr/ResErr.

The Roman parser consumes every character after an optional initial sign using
its low four bits; spaces contribute zero, CAT yields 314 and 2: yields 30. This
follows Apple's [Text Utilities reference](https://leopard-adc.pepas.com/documentation/Carbon/Reference/Text_Utilities/DeprecationAppendix/AppendixADeprecatedAPI.html#//apple_ref/c/func/StringToNum).
Legacy Systemless instead skips whitespace and stops at nondigits; that reference
behavior is not copied where it contradicts the documented contract. Script
Manager patching/international conversion and SANE decimal scanners remain gaps.

Four new default tests cover signed limits, byte guards, atomic output faults,
truncated/maximal Pascal input, overlapping storage, unusual digits/wrapping,
unsupported/short selector frames and both real CPU routes/register preservation.
Rebuilt MPW environment guests call both functions five times on each CPU and
verify signed minimum, zero, CAT and negative overflow before successful exit.

The same original Super Mines click sequence (256,171), (145,150), (402,231)
passes two NumToString calls for 5 and 0, rendering those numbers in the partially
drawn game window. It next stops at CopyMask ($A817), post-trap PC $0100B298,
after loading cicn 130. The stack at $0080F8EA begins with six pointers, from
last to first: dstRect=$0080F9DC, maskRect=$1001098C, srcRect=$1001099A,
dstBits=$1000224E, maskBits=$10010986, srcBits=$10010994. Next: shared GPU
masked bitmap copying with checked snapshots, scaling and ABI coverage, then
continue the original icon/board drawing. Popup interaction, gameplay and full
application lifecycle acceptance remain unverified. The full rebuild stays active.


## 2026-10-15: GPU one-bit masked bitmap copying

CopyMask now marshals six checked pointers from the native 24-byte Pascal frame
or PPC registers into one shared Ports service. Source, mask and destination are
basic monochrome BitMaps with independent signed origins/strides. Equal-sized
source/mask rectangles can scale to a different destination rectangle. Both input
bitmaps are unpacked into immutable snapshots before destination publication,
including aliased storage. The existing GPU blit accepts a borrowed one-bit mask;
its bounded mask payload is serialized after clip/palette parameters. Pixel
coverage and source selection run in WGSL, not a CPU framebuffer loop. Packing
retains destination padding bits and row padding.

CopyBits and CopyMask share live port colors, destination-alias stride checks,
origin translation and visible/clip region coverage; offscreen targets use their
own bounds. Masked scaling samples the source/mask pair with the legacy
Systemless nearest-neighbor rule. Ordinary Basic CopyBits keeps its tested OR
reduction. This reference comparison is not native System 7 pixel acceptance.
One-bit copying follows Apple's [CopyMask contract](https://dev.os9.ca/techpubs/mac/QuickDraw/QuickDraw-167.html).
Indexed/direct source/destination PixMaps, deep/color masks, printing and active
recording/custom-procedure contexts remain unsupported and fail explicitly.
Bitmap/GPU extents remain bounded to 4096 per axis; clip work stays bounded.
Malformed extents, rectangle-size mismatch, unsupported flags, excessive normalized
coordinates and protected destinations fail before pixels are published.


One default ABI test and three GPU tests exercise both real CPUs, exact argument
order/void results, short frames, pixel masks, enlargement/reduction, differing
origins, complex clip holes, source/mask/destination overlap, foreign targets,
live colors, row padding, MemErr and rejected malformed/protected storage.
Existing CopyBits pixel tests pass after sharing their target/clip setup. Rebuilt
MPW drawing guests execute a masked copy and inspect all four result rows and
padding bytes on both CPUs before their existing lifecycle success.

The 30,000-boundary Super Mines sequence (256,171), (145,150), (402,231) completes
its third release without a service fault, including one real CopyMask call.
The captured frame has numeric labels and a status icon but a blank board. A
longer 300,000-boundary-per-step run reaches a different fault: GetResource('CURS',4)
returns nil, followed by SetCursor(nil), post-trap PC $010026B8, SP $0080FB0C.
This is the missing standard watch cursor resource, not a CopyMask fault or
accepted gameplay. Next: shared standard cursor-resource fallback ownership and
application-resource precedence, then continue original new-game initialization.
SetCursor(nil) must not be hidden behind a silent success. The original PPC app,
production Mixed Mode, native presentation and other documented gaps remain.
The full rebuild goal stays active.

The mouse diagnostic now accepts SYSTEMLESS_PROBE_BUDGET (1..=10000000) to choose
an explicit per-step boundary budget; the default remains 30000. It reports that
budget, preserves separate press/release slices and still propagates faults.


## 2026-10-15: standard cursor resource fallback provider

The shared Resource Manager now searches a ROM-free fallback provider for unnamed
CURS IDs 1..4 after the application map. Those IDs represent I-beam, crosshair,
plus and watch cursors, following Apple's [Cursor Utilities contract](https://dev.os9.ca/techpubs/mac/QuickDraw/QuickDraw-371.html).
Their static 68-byte records use the legacy Systemless reference artwork and
hotspots; they are not claimed as native System-file pixel parity. No System file,
ROM, legacy runtime dependency or original-app bytes are bundled.

GetCursor and GetResource('CURS',id) already marshal to the same resource service
on both CPUs; they now obtain one cached, heap-owned handle. Release/dispose clear
its identity, detach retains caller-owned storage and a later lookup reloads the
fallback bytes independently. SetCursor still copies its image, so releasing the
resource does not invalidate presentation. Heap exhaustion returns nil/ResErr
without publishing a cache entry. Application cursor records take precedence,
including short or compressed records; unsupported application compression fails
rather than silently selecting a fallback. Standard fallbacks do not modify the
application resource map or current resource file reference.

The fallback CURS type participates in absent-ID and full-chain absent-name
ResErr reporting. Fallback cursors are unnamed; Get1NamedResource continues to
search only the application map. This provider does not establish support for
multiple open resource maps, general system resources, enumeration of a virtual
System resource file or writable resource maps.


Three new default tests verify all four cursor layouts/hotspots, cached identity,
release/detach/reload behavior, full heap reclamation/exhaustion recovery,
application precedence, unsupported compression, missing-ID/type/name results,
both real CPU adapters and copied cursor state after release. Existing resource
lifetime and cursor rendering tests pass. Rebuilt MPW tracking guests obtain the
watch through GetCursor and GetResource, check its 68-byte handle, set/release it
and reload it before their existing held-button interaction on both CPUs.

The original Super Mines 300000-boundary-per-step sequence (256,171), (145,150),
(402,231) now passes GetResource('CURS',4) and SetCursor with a valid pointer.
It continues new-game initialization, invalidates the board and loads TILE 1101,
then stops at GetResInfo ($A9A8), post-trap PC $01005840. Native SP $0080FAA2 holds
name=$0080FB1E, type=$0080FACA, ID=$0080FACE, resource handle=$10002544. Next:
shared resource metadata/ownership queries and both ABI boundaries, then continue
original tile loading. The board is still blank; gameplay and complete lifecycle
acceptance remain unverified. Original PPC compatibility, production Mixed Mode,
native presentation and the other recorded gaps remain required. The full rebuild
goal stays active.


## 2026-10-15: shared resource metadata queries

GetResInfo now marshals one ResourceInfo request from the native 16-byte Pascal
frame or PPC r3..r6 into the same Process service. The live resource-handle
identity supplies the signed ID, raw four-byte type and Pascal name from the map;
mutable resource payloads and handle relocation do not change that metadata.
Standard cursor fallback handles report their CURS ID and an empty name. Apple’s
[Resource Manager reference](https://leopard-adc.pepas.com/documentation/Carbon/Reference/Resource_Manager/Reference/reference.html#//apple_ref/c/func/GetResInfo)
defines this query and its invalid-resource behavior.

Detached, released, ordinary and unknown handles leave output buffers untouched
and set resNotFound. Valid owned master pointers are checked without changing
MemErr. All nonnil outputs are preflighted before the ID/type/name writes, and
metadata is snapshotted before overlapping outputs can modify storage. Nil output
pointers are skipped, matching the permitted legacy reference. Successful queries
set ResErr to zero without allocating guest memory. Runtime preflight protects the
ResErr publication slot. Both ABI replies are void; the Trap Manager advertises
a readable built-in GetResInfo entry. Writable resource metadata/maps, additional
resource files and general system-resource enumeration remain unimplemented.

Four new default tests cover maximum/raw Macintosh names, absent names, signed
IDs, relocation/payload mutation, heap exhaustion without query allocation, locked
resources, untouched guards, overlapping outputs, malformed masters, protected/
truncated outputs, ownership loss, cursor fallback metadata, native frame bounds,
void register/stack preservation, shared real CPU state and trap availability.
Rebuilt MPW tracking guests verify GetResInfo's watch-cursor ID/type/empty name
and ResError on both real CPUs before their existing host interaction acceptance.


The original Super Mines 300000-boundary-per-step sequence now passes GetResInfo
for TILE 1101 and continues guest tile-byte decoding without a service fault.
With default 30000-boundary click slices plus a 2000000-boundary tail, it reaches
repeated Random calls instead of another missing service. The tail ends Budget
at PC $01006696 (opcode $56C0, D0/D1 zero), and the board remains blank. A second
run adding one explicit host tick per slice also ends at that PC with clock 2188
and QuickDraw randSeed zero. Advancing the clock does not resolve this loop;
there is no established root cause yet. Do not change Random's documented zero
seed behavior merely to bypass the original application's initialization.

Next: trace the first zero-seed write and the random caller's guest state, check
initialization/ABI assumptions against the legacy reference and compiler fixtures,
then continue tile/board initialization. Resource metadata is accepted separately
from gameplay, which remains unverified. The full rebuild goal stays active.

The bounded diagnostic supports SYSTEMLESS_PROBE_TAIL_BUDGET (0..=10000000,
default zero) after the last successful input slice, and
SYSTEMLESS_PROBE_TICKS_PER_STEP (u32, default zero) through the existing host clock
API. It reports the ending clock/QuickDraw seed, skips continuation after a
terminal result and propagates service faults. Guest seed storage is never patched.
Per-step screenshots precede the tail; the final screenshot includes it.


## 2026-10-15: initialized shared calendar clock

The random-loop trace identified a guest instruction at $01005AFC (MOVE.L $020C,
(A0)) copying low-memory Time into qd.randSeed at $003FFF7E. InitGraf had correctly
initialized that seed to one, and earlier tile-setup Random calls produced valid
nonzero values. The application then replaced it with the uninitialized zero
calendar word. Advancing TickCount could not fix a separate calendar word that
never advanced. Legacy Systemless normalizes zero seeds differently, but changing
Random would mask this clock initialization defect; its rebuild implementation
and existing sequence/zero-edge tests remain unchanged.

One Calendar Clock now owns the subsecond phase for both CPU backends. Time at
$020C is an unsigned wrapping count of seconds since 1904-01-01; Ticks at $016A
retains its existing wrapping behavior. Both the existing execution quantum (one
tick per 1000 boundaries) and explicit host elapse_ticks update the same clock.
Sixty modeled ticks advance Time by one second, preserving the remainder across
execution, idle time and TickCount wrapping. Reads and write permission checks
for both words precede publication; a fault leaves words and fractional phase
unchanged. Direct guest Time edits remain authoritative for future advancement.
The host set_date_time API resets the fractional phase without changing Ticks
and rejects stopped processes.

Core CODE/PEF loads start at the reproducible epoch 2000-01-01 00:00:00
(3029529600 Macintosh seconds). The native application launcher replaces that
value once with current host UTC time. Core execution does not sample wall time;
fixtures/probes retain the reproducible default unless the host explicitly sets
another epoch. This is a UTC profile with a 60-tick modeled second, not exact
hardware timing, local time-zone/DST conversion or a battery-backed clock chip.
Calendar conversions, guest SetDateTime and long-date utilities remain gaps.

PPC GetDateTime (void) and ReadDateTime (OSErr), and native ReadDateTime $A039
(A0 output, D0 result, no stack pop), share the same checked Time copy. The native
trap has a shared Trap Manager entry. Direct assembly reads of $020C see the same
word. These contracts follow Apple's [date/time startup model](https://leopard-adc.pepas.com/documentation/mac/OSUtilities/OSUtilities-105.html)
and [ReadDateTime reference](https://leopard-adc.pepas.com/documentation/mac/OSUtilities/OSUtilities-106.html).
Created file/directory dates now derive from initialized, advancing Time; parent
modification-date maintenance remains a separate gap.


Four new default tests cover fractional/mixed execution/idle advancement, seconds
and tick wrapping, huge elapses, reset behavior, atomic permission faults and
phase rollback, unsigned dates, both real CPU date-read routes, output guards,
MemErr/ResErr preservation and the exact observed direct-calendar-to-seed guest
instruction sequence before Random. Rebuilt MPW environment guests compare
GetDateTime and ReadDateTime with the live Time word on both compiler targets.

The original Super Mines three-click sequence with a 2000000-boundary tail now
leaves the random loop and draws a populated board: cells, number glyphs and the
flag count are visible. Its final clock is 1132 and QuickDraw seed 1609244819.
It next stops at SetEmptyRgn ($A8DD), post-trap PC $0100C986, with native
SP $0080FEBE naming region handle $10000EC0. This is rendered board initialization,
not accepted click-to-play/gameplay or completed update/lifecycle handling.
Next: shared SetEmptyRgn mutation/ABI ownership and finish this update path, then
exercise original board interactions. The full rebuild goal stays active.

SYSTEMLESS_PROBE_TRACE_SEED=1 enables bounded instruction-by-instruction seed
observation. It records up to 64 ordinary transitions, every transition to zero
and the first four Random calls with PC/code/register context. It changes no
guest bytes and preserves the same execution budgets/yield/fault outcomes. The
original private code and trace files remain outside the repository.


## 2026-10-15: region transforms and mouse-aware cooperative waits

SetEmptyRgn ($A8DD/InterfaceLib) now replaces an owned region with the canonical
10-byte empty record, discarding the old description without parsing it. Shrinking
locked storage succeeds even with a full heap and reclaims the tail. SetOrigin
($A878) shifts the current basic port's bitmap bounds, portRect and visRgn while
leaving clipRgn, pen, patterns and all other port fields unchanged. OffsetRgn
($A8E0) shares the same checked inversion-storage translation without changing
handle identity or storage size. All three preserve caller MemErr/ResErr. Nil
empty/offset requests and zero displacement are inert per the legacy reference.
The adapters preserve void results and exact four/eight-byte native frames.

Translation validates full region geometry before altering bytes, preserves row
sentinels, and checks all port/region destinations before publication. Unsupported
signed-coordinate overflow or sentinel collisions fail explicitly; full native
wrapping behavior is not claimed. Color ports and visible/clip handle aliasing
remain unsupported. Origin changes reject port/region storage overlap. No drawing,
GPU or guest allocation is needed for region/coordinate changes. GPU drawing
subsequently uses the live shifted visibility and unchanged clipping.

WaitNextEvent now retains a caller-owned mouse-region handle while suspended.
Polling reads its live checked geometry and global Mouse coordinates. A pointer
outside a nonempty region can return osEvt with message $FA000000 when osMask is
requested; nil/empty regions disable this delivery. Existing matching queue,
activation and update events retain priority. Unmatched events stay queued and
mouse-moved delivery does not occupy an OS queue slot. Timeout/null behavior and
non-spinning waits remain unchanged. Output/result validation precedes publication;
invalid live region state faults without consuming a queued event. This is the
single foreground-process model, not complete suspend/resume, desk accessory,
FKEY/alarm or high-level event processing.

Contracts: Apple's [region management reference](https://dev.os9.ca/techpubs/mac/QuickDraw/QuickDraw-131.html),
[origin/clipping discussion](https://dev.os9.ca/techpubs/mac/Toolbox/Toolbox-312.html)
and [WaitNextEvent reference](https://dev.os9.ca/techpubs/mac/Toolbox/Toolbox-51.html).


Nine new default tests and one optional GPU test cover owned/locked/exhausted
storage, canonical empties, malformed masters, complex visibility and region
translation, untouched clip/pen bytes, signed ABI order/guards, transactional
coordinate/permission failures, native/PPC state parity, exact GPU clipping after
origin changes, region holes, masked/empty mouse regions, queue priority, global
mouse authority, live handle edits and deferred waiting without CPU execution.
The previous unsupported-mouse-region test now retains the invalid-handle fault
case. Rebuilt MPW windows check emptying, origin/pen/visibility changes, inverse
region offsets and their existing lifecycle. Rebuilt MPW wait guests sleep inside
a region, then require a real host move to (20,20) and the correct mouse-moved
record before success on both CPUs.

The temporary original-app fork was restored from the local systemless-games
archive and verified against its recorded SHA-256 before use. The original Super
Mines sequence now passes emptying, origin shifts and cursor-region offsets, and
WaitNextEvent returns a window update (what=6, message=$1000224C, ticks=1149,
position v=231/h=402). It next stops at IsDialogEvent ($A97F), post-trap
PC $0100C7D2, SP $0080FEAE, event pointer $0080FEB8. The update needs proper
classification against dialog/window state; returning false unconditionally
would not implement that service. Next: shared dialog-event classification and
complete this main-loop update route, then deliver and verify a board interaction.
The board is rendered, but gameplay and complete lifecycle acceptance remain
unverified. The full rebuild goal stays active.

SYSTEMLESS_PROBE_SETTLE_AFTER_CLICK selects a click after which the diagnostic
runs one additional bounded tail slice, retaining the normal final tail. This
allows initialization to finish before later input. The attempted board click at
(30,65) after click three was correctly skipped because the settle slice faulted
at IsDialogEvent. No failed-slice release or later input is silently injected.


### Live dialog event classification (2026-10-15)

IsDialogEvent ($A97F / PPC import) now routes through shared Window Manager
state. It validates the 16-byte EventRecord, walks the checked live WindowList,
skips hidden windows and compares the first visible window's signed windowKind
with dialogKind (2). No windows or a front document yields false; a front dialog
yields true for every event, including null events and updates naming another
window. This follows Apple's [System 7 contract](https://dev.os9.ca/techpubs/mac/Toolbox/Toolbox-427.html):
the caller must filter unsupported events before DialogSelect. Legacy Systemless
instead filters foreign updates and mouse clicks outside the dialog; that is a
reference difference, not a parity claim. No native System 7 comparison is required.

Both CPU routes return canonical Boolean 0/1, preserve error globals and do not
consume events, allocate, draw or change window state. The native adapter pops
four bytes and writes only the result byte; PPC returns in r3. The installed
Toolbox trap is discoverable. General dialog creation, item management and
DialogSelect remain unimplemented; this classification does not pretend to
implement them. Tests change live window kinds without claiming a constructed
DialogRecord or interactive dialog acceptance.

Two default geometry/state tests cover all event classes, visibility/order,
signed kinds, empty lists, corrupted lists and truncated records. A
third exercises both real CPU routes and error/register/result-byte guards. The
existing GPU window acceptance now checks a positive dialog result on both CPUs.
Rebuilt MPW window guests require four public IsDialogEvent calls and canonical
true/false results after live windowKind changes before their success marker.

The bounded Super Mines sequence (256,171), (145,150), (402,231), then a
2,000,000-instruction settle slice passes IsDialogEvent on the game-window update
and continues picture/region/control update work. It next faults at EmptyRect
($A8AE), post-trap PC $01012076, SP $0080FE4C, rectangle pointer $0080FE6C.
The later requested click (30,65) is correctly skipped, so no board interaction
or gameplay acceptance is claimed. Next: shared rectangle emptiness through both
ABIs, rerun the main-loop update and verify an actual board interaction. The
full rebuild goal remains active.


### EmptyRect continuation and first board interaction (2026-10-15)

With shared EmptyRect, the original now finishes the two-million-boundary settle
after chooser confirmation and returns Waiting(10). Delivering a real
press/release at global (30,65), then another bounded two-million tail, returns
Waiting(10) again without a service fault. The app routes FindWindow, converts
the input to local (24,22), checks controls and redraws the board. Visually
inspected 512x342 readbacks show previously covered upper-left cells become
visible cells/numbers. The complete image changes 4680 pixels including cursor
movement, with half-open bounds [22,55,411,236). This is one basic board
interaction, not full gameplay or lifecycle acceptance.

Reproduction uses the recorded private raw fork and:

```sh
SYSTEMLESS_PROBE_TAIL_BUDGET=2000000 SYSTEMLESS_PROBE_SETTLE_AFTER_CLICK=3 \
cargo run --manifest-path future/Cargo.toml --example probe_mouse -- \
  /path/to/Super-Mines.raw-rsrc /tmp/systemless-mines-emptyrect \
  256 171 145 150 402 231 30 65
```

No native System 7 comparison is required under the user's authorization. The
private executable and screenshots remain outside the repository. Full gameplay,
menu-driven lifecycle, original PPC compatibility and real Mixed Mode remain
unfinished; the rebuild goal stays active.

A follow-up run appends a File-menu press/release at global (35,10) after that
board click. The game routes it through FindWindow and looks up menu ID 2, then
faults at EnableItem ($A939), post-trap PC $01005200, SP $0080FE6E. The native
frame requests item 4 on handle $100020BC. This is the next executable milestone:
shared live menu enable/disable flags, then normal menu tracking/selection and
application shutdown. The File menu has not yet opened or selected a command.


### Shared menu enable state (2026-10-15)

EnableItem ($A939) and DisableItem ($A93A), plus their classic PPC imports, now
mutate the checked live MENU enableFlags field in place. Item zero changes only
the menu-wide bit, preserving individually disabled items. Existing items 1..31
have individual bits; negative, absent or >31 item requests are inert, matching
the supported legacy Systemless contract. The whole-menu bit still controls
items beyond 31. This classic API does not advertise newer extended-menu APIs.

The query/mutation needs no GPU, guest allocation or handle resize. Locked
handles work and keep their data identity. Unknown handles, corrupt masters and
malformed standard records fail before changing flags. Process MemErr is restored
on every path, and this operation bypasses stale error projection so caller
MemErr/ResErr words stay unchanged. Both adapters preserve void results; native
frames contain a signed word item followed by a long handle and pop six bytes.
DrawMenuBar already reads the live whole-menu bit for its existing dimmed-title
pattern; changing flags does not itself redraw the bar.

Two new default tests cover bit 0/1/31 boundaries, more than 31 items, live guest
edits, invalid/nonexistent indices, locked identity, corrupt masters, both real
CPU routes, short frames, registers, padding and distinct process/guest errors.
Rebuilt MPW menu guests require six public enable/disable calls, inspect the
actual mask and preserve individually disabled state through whole-menu toggles
before their existing rendering/lifecycle success marker.

The bounded original sequence through chooser confirmation, one board click and
a File-menu click at (35,10) now passes menu preparation. It next faults at
MenuSelect ($A93D), post-trap PC $0100CD2C, SP $0080FE84, with the start Point
v=10/h=35. The board interaction remains verified, but the menu has not yet been
opened or selected. Next: cooperative shared standard MenuSelect tracking with
real host motion/release, GPU dropdown restoration and exact packed return values,
then rerun the original menu lifecycle. Full gameplay, original PPC, native
presentation acceptance and real Mixed Mode remain incomplete. The full rebuild
goal remains active.

Primary contracts: Apple's [EnableItem](https://dev.os9.ca/techpubs/mac/Toolbox/Toolbox-150.html)
and [DisableItem](https://dev.os9.ca/techpubs/mac/Toolbox/Toolbox-151.html).


### Cooperative menu selection and command decoding (2026-10-15)

Shared MenuSelect ($A93D / PPC import) now yields while the mouse is held, tracks
real global motion across titles and rows, and returns packed menu/item words
on release. Both CPU adapters retain their exact deferred result ABI. Live
flags/text/geometry are revalidated on each poll. Enabled rows highlight; disabled
items and separators do not select. MenuChoice ($AA66 / PPC) reads live
MenuDisable, including attempts to select disabled rows. HiliteMenu ($A938 / PPC)
controls the retained title; DrawMenuBar preserves that highlight. Current port,
caller error globals, unrelated queued input and padding remain unchanged.

Dropdowns, text/marks/shortcuts, highlights and restoration use shared wgpu
operations. An immutable host snapshot of canonical guest screen indices is the
restoration source; only the damaged area is restored. Idle polls do not execute
guest instructions, redispatch calls or redraw unchanged menu state. A queued
release uses its event coordinates even if the pointer subsequently moves; when
mouseUp posting is masked, the physical release still completes tracking. Bad
live storage faults before consuming a release or publishing a result.

The supported slice is standard monochrome, non-scrolling application menus.
Hooks, custom definitions, desk-accessory/zero menu IDs, hierarchy, icons,
per-item color overrides, scrolling/oversized menus, unsupported text faces and
nonzero flashing counts remain explicit unsupported cases. Native pixel parity
is unverified. These remain rebuild requirements, not silently accepted stubs.

Four new default tests cover menu ABIs, live MenuChoice, signed HiWord/LoWord
extraction, guards, result widths and faults. Three GPU tests cover both real
CPUs, waiting/no-spin, real motion/release, retained highlight, exact restoration
of nonuniform pixels, outside edits, switching, disabled/separator/outside
release, masked release, live edits, corrupt masters and unsupported callbacks.
A new rebuilt MPW fixture requires the real host-driven selection on both CPUs
before its success marker and exit.

The private original Super Mines fork retains its recorded SHA-256. The sequence
(256,171), (145,150), (402,231), settle, (30,65), settle, then a held File-menu
press at (35,10) opens the actual dropdown. Moving to (60,35) highlights New Game;
release returns $00020001. A separate run drags to (60,121) and returns $00020008
for Quit. Visually inspected readbacks show the opened/highlighted menu, and
both runs pass shared HiWord ($A86A) and LoWord ($A86B) command decoding.

Both commands next begin constructing WIND 2000 and its four controls, then
stop at MoveControl ($A959), post-trap PC $0100E118. The native frame requests
h=236/v=36 on control $10014EC8 (New Game SP $0080FC78, Quit SP $0080FC86).
The confirmation has not yet been displayed or accepted; Quit has not exited.
Next: shared control repositioning with correct old/new-area drawing, complete
this confirmation and verify the original New Game/Quit lifecycle. Full gameplay,
original PPC compatibility, native presentation acceptance, hierarchy/callbacks
and real Mixed Mode remain unfinished. The full rebuild goal stays active.

The diagnostic now accepts multiple comma-separated settle clicks and a bounded
host drag via SYSTEMLESS_PROBE_DRAG=CLICK:X:Y. It moves while still held, takes
a drag readback and only releases/continues if the slice succeeds. No guest
state is patched. Reproduction:

```sh
SYSTEMLESS_PROBE_TAIL_BUDGET=2000000 \
SYSTEMLESS_PROBE_SETTLE_AFTER_CLICK=3,4 SYSTEMLESS_PROBE_DRAG=5:60:35 \
cargo run --manifest-path future/Cargo.toml --example probe_mouse -- \
  /path/to/Super-Mines.raw-rsrc /tmp/systemless-mines-menu \
  256 171 145 150 402 231 30 65 35 10
```

Use drag target 5:60:121 for the separate Quit route. Private binaries/readbacks
remain outside the repository. System 7 native comparison is not required under
the user's authorization; this is not a native pixel parity claim.

Final interaction coverage additionally uses a 511-pixel logical bitmap in
64-byte rows to prove padding-bit preservation. Traversing the menu bar's bottom
divider keeps the dropdown open; the complete bar is the hit area while its
bottom divider is excluded from title inversion. The redraw cache also includes
the live menu-bar height so a geometry change cannot retain stale pixels.


### Shared control repositioning and resizing (2026-10-15)

MoveControl ($A959) and SizeControl ($A95C), plus their PPC imports, now use one
shared geometry operation for registered standard button/checkbox/radio controls.
Move retains dimensions; size retains the upper-left corner. Hidden controls
change only the live rectangle, without drawing or dirtying their owner. Visible
controls erase the old rectangle with the owner's background pattern, union that
old area into the owner's update region, then redraw at the new rectangle through
the existing wgpu control renderer. Owner visibility/clipping still applies; a
visible control in a hidden owner window changes geometry/dirty state without
requiring a GPU. Current port (including nil), owner port fields, clip contents,
control identity/attributes, visibility and caller error globals are preserved.

The Window Manager exposes a staged invalidation payload so both ordinary window
invalidation and control geometry use the same global content/update model. A
read-only allocator simulation checks the exact update/clip-resize sequence before
publishing changes or erasing pixels. Drawing targets, region geometry, supported
port state and text positions are also checked first. Locked controls remain in
place; moves needing no region growth work even with the guest heap exhausted.
A disjoint dirty-region expansion that cannot allocate fails before geometry,
region bytes or pixels change. GPU/device failures are still explicit faults,
not a promise of transactional device recovery.

Native frames contain v/height, h/width, then the long handle, pop eight bytes and
return no result. PPC low signed words feed the same service and preserve r3.
Nil requests are inert under the legacy reference. Signed coordinate overflow,
nonpositive sizes, private update/clip/visibility aliases, custom/recording port
contexts and unsupported control definitions remain errors. Full classic wrapping
and zero/inverted-control behavior are not claimed. Standalone HideControl's
complete update-region contract remains a separate compatibility gap; this
geometry slice explicitly supplies the documented erasure invalidation.

Two new default tests cover exact ABIs/nil requests and sequential allocator
preflight invariants. Three GPU tests cover both CPUs and all supported control
families, patterned erasure, an independent erase/draw pixel reference, dirty
region geometry, restored nil ports/state/errors, hidden controls/owners without
a GPU, locked identity, exhausted-heap success and failure, invalid geometry and
unsupported port state. Rebuilt MPW window guests require four public move/size
calls, inspect exact rectangles/visibility and preserve a nil current port before
completing their existing lifecycle/pixel acceptance on both targets.

The original Quit sequence now passes two MoveControl requests, including hiding
a placeholder at (-1000,-1000), and continues constructing WIND 2000. It creates
a plain TERecord and sets the 26-byte confirmation prompt, then stops at
TEDispatch $A83D selector $0009 (TEGetHeight), post-trap PC $010090A0,
SP $0080FC86. The prompt has not yet been displayed or accepted, and Quit has
not exited. Unknown TextEdit selector diagnostics now include the exact selector.
The application consumes a long result at that PC (MOVE.L (SP)+,D7). The actual
MPW header at /root/MPW/Interfaces/CIncludes/TextEdit.h in the pinned toolchain
also declares TEGetHeight with long arguments/result and selector 9; this is more
precise ABI evidence than the older PDF's INTEGER return label.

Next: shared TEGetHeight measurement from live monostyled/styled line metrics and
line starts, with documented trailing-blank-line behavior and checked ranges,
then complete the confirmation and verify New Game/Quit lifecycle. The original
call also uses zero/large line bounds, so validate those range semantics carefully.
Full gameplay, original PPC compatibility, native presentation acceptance, real
Mixed Mode/callbacks and remaining menu/Toolbox contracts are unfinished. The
full rebuild goal stays active.

Primary contracts: Apple's [MoveControl](https://dev.os9.ca/techpubs/mac/Toolbox/Toolbox-340.html),
[SizeControl](https://dev.os9.ca/techpubs/mac/Toolbox/Toolbox-341.html),
[HideControl](https://dev.os9.ca/techpubs/mac/Toolbox/Toolbox-339.html), and
[TextEdit reference](https://dev.os9.ca/techpubs/mac/pdf/Text/TextEdit.pdf).


### Shared live TextEdit line-height measurement

TEGetHeight now reads the registered record's live line starts and plain
lineHeight or styled LHTable metrics. It measures inclusive one-based ranges,
omits trailing blank lines, validates table bounds/ownership and positive heights,
and preserves guest memory and both error globals. It requires neither guest
allocation nor a current port/GPU. Both CPUs call this same implementation.
The 68K TEDispatch selector 9 consumes a 14-byte selector/argument frame and
returns a long; PPC uses three long arguments and a long register result.
The public MPW TextEdit.h declaration and original caller verify the long ABI;
Apple's TextEdit PDF incorrectly prints INTEGER for the result.

For out-of-contract endpoints, values below one denote line one, reversed
endpoints are sorted, and the range intersects actual nonblank lines. This is
an explicit finite extension, not a claim of native out-of-range parity. Legacy
Systemless/Executor can invent heights or read beyond line storage for such
ranges; that behavior is not copied. Corrupt live tables fail explicitly.

Focused tests cover live plain/styled metrics, extreme/reversed/outside ranges,
trailing blanks, exhausted guest allocation, corrupt records, exact CPU frames,
long results above 32767, a nil current port and unchanged error globals. The
public MPW fixtures make four height queries on each architecture.

The original Super Mines Quit route passes TEGetHeight with start=32767/end=0,
continues prompt layout and control relocation, then stops at GetIcon ($A9BB),
icon ID 0, post-trap PC $0100E9A2, SP $0080FBF2. Evidence is the private bounded
run /tmp/systemless-mines-teheight-probe.txt; original binaries remain outside
Git. The confirmation has not displayed or been accepted and shutdown has not
completed. Next: resource-backed icon lookup/drawing, then continue the real
confirmation interaction. Full rebuild and mixed-mode work remain active.

Contract: Apple's [TextEdit reference](https://dev.os9.ca/techpubs/mac/pdf/Text/TextEdit.pdf),
TEGetHeight, p. 2-92; pinned MPW Interfaces/CIncludes/TextEdit.h for the ABI.

Legacy inspection for the next slice: GetIcon delegates to resource lookup and
its built-in icon fallback supports ID 1 only, not the requested ID 0. Preserve
resource-miss semantics; establish whether the application supplies ICON 0
before introducing fallback artwork.


### Shared resource-backed icon lookup

GetIcon now routes 68K trap $A9BB and the InterfaceLib PPC import to the same
GetResource('ICON', signed ID) service. The native adapter validates its long
result before popping the two-byte ID; PPC takes the low signed word in r3.
The trap is installed for guest availability queries. Handles share Resource
Manager identity, mutation, release/reload and errors; missing resources return
nil through the actual lookup. No substitute icon is created for a missing ID.
PlotIcon, colored icon loading, icon suites and standard system-icon fallback
artwork remain separate unfinished work.

The shared real-CPU resource test now runs for both PICT and ICON, including
negative IDs, 128-byte icon contents, GetResource identity, guest mutation,
release/reload, missing ID zero, result guards and readonly native outputs.

Inspection of the original Super Mines resource map finds ICON 128 and 129,
128 bytes each, but no ICON 0. The original Quit route now passes the real
missing-resource lookup and displays its prompt and Yes/No/Cancel controls.
Private screenshot /tmp/systemless-mines-geticon-after.png verifies the visible
"Save game before quitting?" prompt; the bounded guest continues its custom
event loop without a service fault. No native System 7 comparison was required.

Contract: Apple's [Icon Utilities](https://dev.os9.ca/techpubs/mac/MoreToolbox/MoreToolbox-287.html)
and [resource disposal](https://dev.os9.ca/techpubs/mac/MoreToolbox/MoreToolbox-290.html).

The follow-up Quit run presses No at global (270,86), after the identical splash,
chooser, board and File-menu sequence. It disposes the confirmation TextEdit
record, all four controls, its window and the game window, then stops at
GetWMgrPort ($A910), post-trap PC $0101161A, SP $0080FD1C. The confirmation
acceptance/cleanup is verified; ExitToShell has not been reached. The save and
Cancel branches and New Game confirmation were not exercised in this slice.
Evidence: /tmp/systemless-mines-quit-no-probe.txt. Next: shared Window Manager
port identity/access and continuation of original shutdown. Full rebuild remains
active, with PPC originals and real Mixed Mode still outstanding.


### Shared Window Manager port and original no-save shutdown

GetWMgrPort ($A910 and InterfaceLib) now returns the monochrome port already
owned by InitWindows, through one Windows::manager_port query. It never creates
a replacement port, changes the current port, draws or allocates guest storage.
A query before InitWindows is an explicit error. The native adapter validates
the four-byte output before popping its four-byte pointer argument; PPC takes
the pointer in r3 and preserves its void result. Both use the same port identity,
including after ordinary window teardown. Color Window Manager ports remain
unfinished; this does not implement GetCWMgrPort.

Tests check both real CPU routes, repeated identity, an unchanged live 108-byte
port, nil caller port, differing process/guest error values, output guards,
readonly native output before stack movement and failure before initialization.
The initialized query is tested with the GPU detached. Public-header MPW window
guests issue two GetWMgrPort calls, verify stable identity and full-screen port
bounds, then complete the existing window/control lifecycle and pixel checks.

The original Super Mines sequence dismisses the splash, confirms its chooser,
changes board cells, selects File > Quit with a real press/drag/release, then
presses No at global (270,86). It now passes GetWMgrPort, sets the returned port,
continues final menu/cursor/pointer/region and file cleanup, and reaches
ExitToShell with code 0 on click 6 release. This is bounded original 68K execution
with no guest patching. Evidence: /tmp/systemless-mines-wmgr-quit-probe.txt;
post-exit PC $01000030. Original files and screenshots remain outside Git.
Save/Cancel, New Game, wider gameplay, original PPC acceptance and production
Mixed Mode remain independent incomplete requirements. The full rebuild goal
remains active.

Contract: Apple's [GetWMgrPort](https://leopard-adc.pepas.com/documentation/mac/Toolbox/Toolbox-276.html),
Macintosh Toolbox Essentials, p. 4-114. Legacy Systemless is comparison evidence;
native System 7 comparison is optional under the user's authorization.

A separate original New Game run uses drag target (60,35), declines saving at
(270,86), and visibly returns to the game/difficulty chooser without a service
fault. Evidence: /tmp/systemless-mines-newgame-no-probe.txt and its private
after.png readback. This establishes chooser return, not yet a fresh board or
save support. The Quit run's No branch reaches Exit(0); keep those distinct.

The follow-up New Game run confirms the chooser at (402,231). It draws a new
board with a visibly different layout, returns Waiting { ticks: 10 }, and stays
at that normal wait through the final bounded advance. Evidence:
/tmp/systemless-mines-newgame-restart-probe.txt and its private after.png. This
verifies no-save New Game restart; it does not establish complete gameplay or
saving. Both original lifecycle runs use actual host input, not guest patches.

The separate Quit/Cancel run presses Cancel at (367,86) and returns to the
existing game's Waiting { ticks: 10 } state without Exit or a service fault.
Private evidence: /tmp/systemless-mines-quit-cancel-probe.txt, step-4 and after
PPM/PNG readbacks. Comparison of x=20..496, y=58..319 finds 123 changed pixels;
53 are pointer movement and 70 form a former confirmation edge (x=409..422 at
y=58 and x=422 through y=114). Board/counter pixels outside those areas remain
identical. Thus Cancel lifecycle works, but exact damage restoration is not
accepted. Next: reproduce/fix that dialog-edge exposure in a focused window
fixture, then verify save-path dialogs/files and broader original gameplay.
Do not mistake no-save Quit/New Game success for full application compatibility.


### Cancel damage investigation and original save entry

The prior 70-pixel Cancel difference is inside the game board border, not the
modal window's outer edge. Read-only instruction tracing of (422,60)/(410,58)
shows their initial black value comes from the application's PaintRect at
PC $01006EF2. Showing the confirmation makes them white; the later update
redraw does not restore their black value. The redraw executes with a normal
one-pixel black pen and a visible/update region covering these points. Initial
and redraw WindowRecords retain the same bitmap origin. This narrows the next
investigation to the app's initial/background and later object/bitmap redraw
paths; it does not yet prove a Window Manager defect or an application defect.
No speculative clipping, port movement or snapshot-restoration change is made.

The existing bounded probe now accepts SYSTEMLESS_PROBE_TRACE_PIXELS=422:60,410:58
(or 1..16 X:Y points inside its 512x342 display). It samples canonical guest
screen bits, excluding the presentation cursor, and reports the exact instruction
PC and services when values change. It caps emitted changes at 256, uses the
existing execution budgets, and never patches guest memory/registers. Pixel and
seed traces are independently enabled. Invalid/outside coordinates fail before
reading an application. Private traces remain outside Git.

Legacy comparison uses the preserved baseline 634c0e9120a695224d71c621fbbe33709670d707,
compiled with the ci-test profile, original archive, classic 68K preference and
512x342/1-bit display. The first bounded scripts completed actions but never
reached the confirmation (zero assertions); they are not compatibility passes.
A longer settling run panicked in legacy quickdraw/raster/spans.rs:73 with an
arithmetic overflow. Thus this comparison supplies no reliable confirmation
pixel oracle. Evidence: /tmp/systemless-legacy-cancel-run{,2,3} and matching logs;
/tmp/systemless-legacy-cancel3.json records the failed longer scenario. No native
System 7 comparison is required, and the baseline source is unchanged.

The original Yes-save branch is now independently exercised: after splash,
chooser, changed board and File > Quit, press Yes at (175,86). The confirmation
is disposed, then the guest stops at Pack3 $A9EA selector 1 (classic SFPutFile),
post-trap PC $0100878A, SP $0080FBB8. The frame has reply pointer $0080FC3A,
nil dialog hook, default-name pointer $003FE77C, prompt pointer $003FE7C8 and
where=(100,100). Evidence: /tmp/systemless-mines-quit-save-probe.txt. No file
selection, successful save or saved-game reload is claimed.

Next executable milestones: shared cooperative Standard File save UI, checked
classic reply/file identity and the actual save/reload path, while continuing
focused board-redraw investigation. Neither a silent cancellation stub nor an
unverified framebuffer workaround satisfies those requirements. The complete
rebuild, original PPC compatibility and production Mixed Mode remain active.


### Working-directory identity for the Standard File save path

The original SFPutFile reply must carry a working-directory reference, not just
volume -1. The shared File Manager now owns a bounded process-local WD table.
References identify an existing catalog folder and user ID; repeated opens for
the same pair reuse identity, while distinct owners get distinct references.
Closing invalidates the directory reference without closing access paths already
opened through it. Volume references close harmlessly. The model allocates up
to 128 references in the classic negative WD range, returning tmwdoErr (-121)
on exhaustion. This is an explicit runtime bound, not a native capacity claim.

PBOpenWDSync/PBCloseWDSync/PBGetWDInfoSync and native HFSDispatch selectors
1/2/7 reach the same Files implementation. Parameter blocks are big-endian,
outputs are checked before table mutation, and replies publish both ioResult
and the correct signed CPU error result. The info query reports volume -1,
actual directory and owner, optional Pascal volume name and indexed entries
from the one virtual volume. Invalid indexes/references, unknown volumes and
missing folders report errors. Process and guest Memory/Resource errors are
preserved; asynchronous completion and the high-level PPC WD wrappers are not
implemented by aliases or stubs.

Classic create/open/fork and file-info calls now follow a WD's folder. HFS calls
use that folder when directory ID is zero, and an explicit nonzero directory
ID overrides it. Directory creation, catalog lookup/enumeration and FCB volume
queries resolve the same references. Existing relative/absolute path behavior,
fork permissions and catalog ownership remain shared. The default volume still
means the existing application-volume root; GetVol/SetVol/default-directory
selection and multiple volumes remain separate work.

Three default tests exercise both real CPUs, shared identity, path-based opens,
owner separation, actual file creation in a selected folder, classic/HFS lookup,
explicit root override, catalog enumeration, names/errors, table exhaustion and
reuse, access-path survival after CloseWD, and output faults without publication.
The public-header working.c fixture creates Saves, opens its WD twice, creates
and opens Game there, verifies the root does not contain Game, queries the WD,
closes it and verifies its stale reference fails. Both compiled architectures
reach the explicit success marker and Exit(0).

This is the required file-identity foundation for the next cooperative Standard
File save dialog. SFPutFile UI/selection/cancellation and successful original
save/reload are still unfinished; the original currently still stops at Pack3
selector 1. Preserve the full save-dialog milestone, including visible controls,
filename editing, folder/catalog navigation, replacement confirmation, checked
SFReply publication and real file operations. A silent cancellation or root-only
volume substitute does not meet that milestone. The full rebuild remains active.

Contracts: Apple's [original Standard File reply](https://leopard-adc.pepas.com/documentation/mac/Files/Files-316.html),
[File Manager reference](https://leopard-adc.pepas.com/documentation/mac/pdf/Files/File_Manager.pdf),
pp. 2-201–2-204, and [Standard File Q&A](https://leopard-adc.pepas.com/technotes/fl/fl_525.html).

The original no-save Quit sequence was rerun after WD-aware file resolution
changed. It still completes all final file/resource cleanup and Exit(0) on the
No button's release. Evidence: /tmp/systemless-mines-wd-quit-probe.txt. This is
a regression check, not successful Standard File save-dialog acceptance.


### Cooperative classic save dialog and original game-file writes

SFPutFile now shares one cooperative dialog implementation between native Pack3
selector 1 and the PPC InterfaceLib import. Runtime retains its ABI reply while
both CPU engines are suspended. The UI owns an ordinary registered monochrome
modal window, five shared controls and a shared TextEdit filename record, with
Window Manager, font/QuickDraw and wgpu drawing throughout. Input consumes the
existing checked OS event queue, at most 20 events per poll. Empty polls execute
no guest instructions; host ticks drive the existing caret behavior. No host
filesystem chooser, ROM/System resources or legacy runtime dependency is used.

The picker shows the process-local catalog, supports folder entry/Up, paging,
file-name selection, typed edits, caret positioning, arrow/backspace and Cmd-A,
horizontal filename scrolling, Save/Cancel and explicit replacement confirmation.
It limits HFS leaf names to 31 bytes and disables Save for empty/invalid names,
folder collisions and the protected application file. Exhausting WD capacity
keeps the dialog interactive with cancellation available. Catalog snapshots are
read-only views of the same live file/directory tree; acceptance returns an
actual working-directory reference. The picker itself neither creates nor
replaces a file. Caller port/error state is restored and owned UI allocations
are disposed when it completes. GPU faults remain terminal and do not promise
transactional pixel rollback.

The native adapter consumes the exact 22-byte selector/argument frame and has a
void result; PPC decodes the packed Point plus prompt/name/hook/reply arguments
and preserves its void register result. SFReply is 74 bytes: successful replies
publish good, the WD and Pascal name with checked bounds; cancellation changes
only good, preserving all other bytes. The output is revalidated while waiting.
Pascal inputs are snapshotted before guest allocation, and font/window/display
requirements are checked explicitly. Nonzero hooks, other Pack3 selectors,
modern/custom put-file APIs, multiple volumes, remembered Standard File folder
globals, clipboard/drag-selection/focus-cycle parity and native pixel parity
remain unfinished. The supported slice starts at the virtual volume root and
lets the user choose real subfolders; it is not a root-only return shortcut.

The original game next required PBHDelete before creating its selected file.
PBDeleteSync/PBHDeleteSync and native $A009/$A209 now use the shared catalog and
WD/path resolution. Closed mutable files lose both forks; open files, nonempty
folders and folders with open WDs return fBsyErr. The loaded application is
protected. Tests cover the signed CPU/parameter-block errors, closed-file
removal and busy/protected directory cases. Deletion only affects process-local
nodes; host paths and persistence remain outside this File Manager model.

Real-CPU tests verify exact ABI frames, bad outputs/hooks/selectors, no-spin
waits, matching UI pixels across both architectures, cancellation and guards,
folder selection/editing, replacement/back-out, disabled names, paging/Up,
long names, WD exhaustion and caller state restoration. The public-header
standard_file.c program suspends at the dialog on both compiler targets. Save
returns through WD lookup to real Delete/Create/Open/Write/Close/GetFInfo calls,
and the host verifies the resulting bytes are exactly ABC. Cancel returns its
own explicit success marker and creates no file.

The original Super Mines Yes-save route is now exercised through the visible
SFPutFile dialog. From the changed board, select File > Quit, press Yes at
(175,86), then Save at (407,299). The guest passes PBHDelete, creates/writes/closes
Super Mines Game and reaches ExitToShell/Exit(0), post-exit PC $01000030. The
process-local catalog contains its 1,972-byte data fork (no resource fork), with
prefix 00 03 00 00 00 00 00 00 04 B0 00 00 0C DF 00 00. Evidence:
/tmp/systemless-mines-save-catalog-probe.txt; the step-6 PNG confirms the visible
filename, catalog and Save/Cancel controls. Private files/screenshots stay out
of Git. SYSTEMLESS_PROBE_CATALOG=1 optionally prints catalog names, fork sizes
and 16-byte prefixes for bounded original-app diagnostics.

A separate original run presses Cancel at (407,269). It returns to the game's
Waiting { ticks: 10 } state, and catalog readback confirms no Super Mines Game
file. Evidence: /tmp/systemless-mines-save-cancel-probe.txt and private after.png.
This verifies cancellation and save-to-process-local-bytes, not persistence or
successful reload. The earlier board-border redraw difference remains unresolved.

The next original reload slice is concrete: File > Open Game (drag to (60,52)),
then decline saving at (270,86). It stops at SFGetFile, Pack3 selector 2, PC
$010086EC, SP $0080FBCE. Its frame carries reply $0080FC56, nil dialog hook,
type-list pointer $0080FCBE, one type, file-filter callback $004002F2, prompt
$003FFD76 and where=(100,100). Evidence: /tmp/systemless-mines-open-next-probe.txt.
Implement the cooperative open picker AND real guest filter invocation; ignoring
that callback or returning false cancellation would not satisfy the slice.
Persistent file backing, original PPC application acceptance, production Mixed
Mode and the rest of the rebuild remain active.

Contracts: Apple's [Standard File Package](https://dev.os9.ca/techpubs/mac/pdf/Files/StdFilePkg.pdf),
[SFReply](https://leopard-adc.pepas.com/documentation/mac/Files/Files-316.html),
and [PBHDelete](https://leopard-adc.pepas.com/documentation/mac/Files/Files-236.html).


## Same-process Super Mines save and reload

The cooperative classic SFGetFile dialog now executes the original app's
nonzero file filter ($004002F2) through its real Pascal guest entry, after
matching its one Finder type. The picker displays Preferences and the actual
1,972-byte Super Mines Game file, and acceptance returns its name/type/WD. The
guest opens, reads and closes that file, redraws its board and resumes
WaitNextEvent with sleep=10 at PC $0100C798. No unsupported-service fault occurs.

Reproduce with the private raw resource fork and a real wgpu adapter:

```sh
SYSTEMLESS_PROBE_CATALOG=1 \
SYSTEMLESS_PROBE_TAIL_BUDGET=2000000 \
SYSTEMLESS_PROBE_SETTLE_AFTER_CLICK=3,4,5,6,7,8,9 \
SYSTEMLESS_PROBE_DRAG=5:60:71,7:60:52 \
cargo run --manifest-path future/Cargo.toml --example probe_mouse -- \
  RAW_RESOURCE_FORK /tmp/systemless-mines-reload \
  256 171 145 150 402 231 30 65 35 10 407 299 35 10 180 169 407 299
```

Steps dismiss the splash, choose/start a board, reveal cells, drag File > Save
Game, accept Save, drag File > Open Game, select the file and accept Open.
The diagnostic now supports comma-separated held drags at distinct click steps;
existing single-drag syntax remains valid. Evidence from the development run is
/tmp/systemless-mines-reload-probe.txt and its step/after PPMs. The returned board
and flag-number area were inspected. The run ends at Waiting{ticks:10}; its
catalog retains the same saved bytes. This demonstrates reload within one
process, not persistence across launches, every game state or native pixel
parity. The earlier Quit-Cancel board-border difference remains unresolved.

## Super Mines reload after process restart

The optional persistent backing now proves the same original saved game across
separate host processes. Start with a new volume path; an existing game may
require the normal replacement confirmation. The development run used
`/tmp/systemless-mines-persistent.slv` and these two independent invocations:

```sh
SYSTEMLESS_PROBE_VOLUME=/tmp/systemless-mines-persistent.slv \
SYSTEMLESS_PROBE_CATALOG=1 SYSTEMLESS_PROBE_TAIL_BUDGET=2000000 \
SYSTEMLESS_PROBE_SETTLE_AFTER_CLICK=3,4,5,6,7 SYSTEMLESS_PROBE_DRAG=5:60:121 \
cargo run --manifest-path future/Cargo.toml --example probe_mouse -- \
  RAW_RESOURCE_FORK /tmp/systemless-mines-persist-save \
  256 171 145 150 402 231 30 65 35 10 175 86 407 299

SYSTEMLESS_PROBE_VOLUME=/tmp/systemless-mines-persistent.slv \
SYSTEMLESS_PROBE_CATALOG=1 SYSTEMLESS_PROBE_TAIL_BUDGET=2000000 \
SYSTEMLESS_PROBE_SETTLE_AFTER_CLICK=3,4,5,6,7,8 SYSTEMLESS_PROBE_DRAG=5:60:52 \
cargo run --manifest-path future/Cargo.toml --example probe_mouse -- \
  RAW_RESOURCE_FORK /tmp/systemless-mines-persist-open \
  256 171 145 150 402 231 30 65 35 10 270 86 180 169 407 299
```

The first run changes cells, chooses Quit, confirms saving, accepts Save and
reaches Exit(0). The second starts a fresh CPU/heap/catalog, restores the writable
volume, changes its new board, chooses Open, declines saving that board, selects
Super Mines Game and opens it. The real guest filter and file reads run again;
the saved board redraws and returns to Waiting{ticks:10} at PC $0100C798. Both
logs show node 19, 1,972 game data bytes and no resource bytes, with matching
16-byte prefixes. The preferences folder/settings file also survive. The picker
and final board screenshots were visually inspected. Evidence is
/tmp/systemless-mines-persist-{save,open}-probe.txt and associated step/after PPMs.

This proves one bounded original game state across process launches using the
new runtime's volume format. It does not establish HFS disk-image compatibility,
native-window interaction, every game state, original PPC acceptance, general
Mixed Mode or exact native pixel parity. Host checkpoint and failure boundaries
are specified in [volumes](volumes.md); the full rebuild remains active.


Centaurian's PPC load was rerun during the persistence milestone with the same
recorded data/resource SHA-256 values. It still fails before execution at the
strong InterfaceLib SndDisposeChannel import. Evidence:
/tmp/systemless-volume-centaurian-next.txt. This remains an actual loader gap;
no original PPC application launch or sound-channel acceptance is claimed.


## Sound-channel lifetime and refreshed Centaurian gaps

SndNewChannel/SndDisposeChannel now resolve to real shared channel/queue lifetime
services rather than a success alias. Centaurian's original PPC PEF loader moves
past the previous SndDisposeChannel import and now rejects the strong InterfaceLib
Index2Color import. It still does not reach guest execution or gameplay. Evidence:
/tmp/systemless-channels-centaurian-next.txt. Its 68K path was separately rerun
with a 200,000-boundary budget and offscreen graphics and still stops at GetGDevice
($AA32, decimal 43570): /tmp/systemless-channels-centaurian-68k-next.txt. The private
input hashes match the recorded originals. Shared device/Color QuickDraw state
is the next concrete foundation needed by both paths; other sound commands,
status, callback execution and actual audio remain unfinished.

Super Mines retains the specific automated/offscreen gameplay and persistence
coverage above. An interactive native-window check was attempted, but the desktop
was locked and UI inspection/input was unavailable. The disposable test process
was closed without attaching a persistent volume. This is not a native gameplay
acceptance result. A full live desktop session and win/loss gameplay remain
unverified; no broader playable-game claim follows from channel-lifetime tests.


## Original Escape Velocity and Marathon priorities

The next original-game targets are Escape Velocity 1.0.5 and Marathon 1 v1.2.
These are the original Macintosh applications, not EV Override/Nova, Marathon 2,
Infinity, or modern engine replacements. Neither has reached gameplay in the
rebuild. Existing legacy Systemless results remain separate comparison evidence;
a native System 7 comparison is optional under the user's revised scope.

Private source archives are `Escape_Velocity_1.0.5.sit` and `marathon.sit` in the
local original-game corpus. Extraction used unar's visible AppleDouble forks;
entry 2 was copied verbatim into a raw resource fork. No original input is
checked into Git. The observed raw resource SHA-256 values are:

- Escape Velocity: `d739c1d5bd0d5c7efddc55f34bfadd8f66a25c819a00bbed9523c7c56e3f716b`
  (500,871 bytes; application data fork 514,331 bytes).
- Marathon v1.2: `afa4e77d8b53c42062fe4b730472cfbb396aacf715c2181aeb5dd12ea60491a1`
  (625,479 bytes; application data fork 322,352 bytes).

Initial bounded diagnostics against rebuild commit 66bb4612:

| Original | 68K CODE path | PowerPC PEF path |
| --- | --- | --- |
| Escape Velocity 1.0.5 | 200,000 instructions is insufficient for startup initialization; a 5,000,000 budget reaches unsupported trap $A02D (SetApplLimit) | Strong InterfaceLib SndSoundManagerVersion import unresolved before execution |
| Marathon 1 v1.2 | Unsupported trap $A036 (MoreMasters) within a 200,000 budget | Strong InterfaceLib NewRoutineDescriptor import unresolved before execution |

68K commands use `--code RAW --graphics --budget LIMIT`; PPC uses
`--pef DATA --resources RAW --budget 200000`. A real Metal adapter was available
outside the restricted process. The restricted adapter lookup failed and is not
counted as a guest failure. Logs are `/tmp/systemless-ev-startup-long.txt`,
`/tmp/systemless-marathon-startup.txt`, `/tmp/systemless-ev-ppc-startup.txt`, and
`/tmp/systemless-marathon-ppc-startup.txt`. Short traces confirm the originals
execute startup initialization; budget exhaustion alone is not evidence of a hang.

Implement the shared application heap limit/master-pointer semantics first,
with real allocation and failure behavior and ABI coverage for both CPUs.
Then rerun originals to choose the next required service. Do not bypass startup
checks with success aliases. The companion-file model also needs actual mounted
assets: EV Data/Graphics/Titles/Sounds/Music and its plug-in/pilot folders;
Marathon Map/Shapes/Sounds/Music. Merely loading application forks does not mount
those files. Preserve data and resource forks and their Finder metadata.

Acceptance must advance from startup to title/menu, then real gameplay and
save/reload across a fresh process. EV should create a pilot, enter flight,
respond to steering/thrust, and travel or land; Marathon should start a level,
respond to movement/turning/firing, and save/reload. Verify color graphics and
required sound behavior explicitly. Maintain separate 68K/PPC results and native
window/offscreen evidence. These targets guide compatibility work; they do not
replace the full rebuild objective or authorize marking that objective complete.


### Master-pointer reservation implementation

The reservation contract follows Apple's [Inside Macintosh: Memory,
MoreMasters](https://dev.os9.ca/techpubs/mac/Memory/Memory-65.html), pp. 1-51–1-52.
MoreMasters now has one shared implementation reached by the 68K $A036 trap and
PowerPC InterfaceLib import. Each explicit call reserves a real nonrelocatable
256-byte heap block containing 64 master slots. NewHandle consumes available
reserved slots before its existing individual-master fallback; DisposeHandle
zeros and returns pooled slots while preserving the reservation. Allocation
failure reports memFullErr without adding a partial pool. This does not yet
implement guest heap-zone records, configurable moreMast, system-heap selection,
or automatic 64-slot pool replenishment for every NewHandle call.

The focused tests consume all other heap space, allocate and dispose a real
handle through the reservation, verify reuse and pointer-ownership rejection,
and check failed further reservations preserve the live handle. Actual 68K/PPC
execution reserves two blocks in one Process; tests check stack/void-register
behavior and guest MemErr publication on exhaustion.

The original Marathon 68K rerun, using a 500,000-boundary graphics-enabled budget,
now passes MoreMasters and stops at unsupported Alert ($A985):
/tmp/systemless-marathon-moremasters.txt. This is a new observed startup boundary,
not a title/menu or gameplay result. The development tracer now prints bounded
Pascal-string bytes passed to ParamText. A follow-up trace decoded parameter 0
as the original request to put Map, Shapes and Sounds beside the application,
and parameter 1 as -35 (file not found):
/tmp/systemless-marathon-alert-parameters.txt. This identifies companion-file
mounting as the next concrete Marathon requirement; the archive contains those
files, but the current application loader does not mount them.
Escape Velocity's SetApplLimit and both recorded PPC import blockers remain.


A second original Marathon run populated a disposable existing-format SLVOLUME
image with Map/Shapes/Sounds/Music at root directory 2 (nodes 17–20). Data forks
were copied verbatim, AppleDouble entry 2 supplied resource forks, and the first
16 bytes of entry 9 supplied Finder metadata. The application forks remained
loader-owned. Observed fork lengths were Map 3,863,478/319, Shapes 0/6,481,428,
Sounds 0/1,021,569 and Music 228,430/361 (data/resource bytes). Thus resource-only
companion files must be retained even when no host data file exists.

With `--volume /tmp/systemless-marathon-assets.slv --graphics --budget 1000000`,
Marathon passes the previous missing-companion-file alert and stops at $A214,
the hierarchical GetVol parameter-block trap. Evidence:
/tmp/systemless-marathon-with-assets.txt. This uses the existing writable volume
codec as a diagnostic import route, not a new general host-folder importer or
HFS disk image. No title/menu, rendering, sound or gameplay acceptance follows.
The next observed Marathon service requirement is real current volume/directory
query behavior; a reusable original-folder import boundary is still needed.


### Current-volume queries and Marathon sound-version boundary

PBGetVolSync/PBHGetVolSync and the 68K $A014/$A214 traps now use the shared File
Manager working-directory service. They report the catalog's actual single
Application Volume (-1), initial root directory 2 and owner 0; optional volume
names are written to the supplied Pascal-string buffer. Classic calls touch only
the 24-byte prefix; hierarchical calls additionally publish owner, real volume
and directory fields in the 52-byte WD record. Nil name pointers are accepted.
Missing catalogs return -35. Invalid PB/name buffers fault before partial output.
SetVol/HSetVol and selectable default directories remain unsupported; opening a
working-directory reference does not itself change the default directory.
Contracts: [Inside Macintosh: Files](https://vintageapple.org/inside_r/pdf/Files_1992.pdf),
pp. 2-150–2-153. No native parity claim follows from this initial single-volume
query slice.

The unchanged original Marathon v1.2, with its recorded companion volume, now
passes $A214 and reaches SoundDispatch $A800 with D0=$000C0008, the [sound-manager version query](https://dev.os9.ca/techpubs/mac/pdf/Sound/Sound_Manager.pdf),
Sound Manager Reference p. 2-133. This is the same service family as EV's unresolved PPC
SndSoundManagerVersion import. Evidence: /tmp/systemless-marathon-getvol.txt and
/tmp/systemless-marathon-sounddispatch.txt. This is not a gameplay result; do not
invent sound support or a success alias simply to pass a version check.

The development tracer now accepts `--volume IMAGE` to restore a bounded,
read-only snapshot of an existing SLVOLUME image before execution. It never
checkpoints guest changes back to the supplied image. Reproduction:

```sh
cargo run --manifest-path future/Cargo.toml --locked --example trace_application -- \
  code RAW_RESOURCE_FORK 1000000 --graphics --volume COMPANION_VOLUME_IMAGE
```

This mode does not mount host folders or import HFS disk images. New real-CPU
query tests verify both forms with and without name buffers, exact output fields,
unchanged caller state, no implicit default-directory selection, missing-volume
errors and memory-boundary failures.


### Escape Velocity application-partition heap and SetApplLimit

The actual EV 1.0.5 failure has A0=$003DDAD8 at $A02D, requesting a limit below
its A5 world. Returning success against the old unrelated $10000000 heap would
not model that address. CODE preparation now configures the shared allocator
from $00010000 to the A5-world base before guest execution/allocations, removes
the unused high mapping, and publishes the corresponding bounds. PEF retains
the default 4 MiB reservation; both use the same Process allocation/resize logic.

SetApplLimit now sets a checked absolute ApplLimit ($130), shared with
GetApplLimit and both real CPU adapters. Invalid out-of-reservation addresses
return memFullErr without changing the limit. The allocator is still eagerly
grown: lowering its growth limit does not shrink the existing zone or destroy
live blocks, following [Inside Macintosh: Memory](https://dev.os9.ca/techpubs/mac/pdf/Memory/Memory_Manager.pdf),
pp. 2-84–2-85. This does not establish general lazy heap growth, guest heap-zone
headers, alternate stack placement or arbitrary application partition sizes.

Original EV 1.0.5 now passes its actual SetApplLimit call and stops at GetGDevice
($AA32) with a 2,000,000-boundary graphics-enabled budget:
/tmp/systemless-ev-appllimit-next.txt. Marathon v1.2 with its companion volume
still passes its recorded startup path and stops at SoundDispatch $000C0008:
/tmp/systemless-marathon-partition-next.txt. Neither reaches gameplay.

New tests verify CODE pointers/handles reside below globals, relocation preserves
bytes, full reservation exhaustion/disposal, native absolute limits and errors,
retention of already-grown high-address blocks, and native/PPC sharing of limit
state with correct void-register behavior. The cross-application loader test now
checks each real handle identity before comparing normalized service contracts;
different application partitions no longer require identical numeric pointers.


Super Mines' previously recorded automated Save/Open sequence was rerun with
the CODE partition layout and unchanged original resource fork. It created and
opened Super Mines Game (node 19, 1,972 data bytes; no resource bytes), retained
its settings file and returned to Waiting{ticks:10} at PC $0100C798. The final
board image was visually inspected. Evidence:
/tmp/systemless-mines-partition-reload.txt and
/tmp/systemless-mines-partition-reload-after.png. This preserves the prior
bounded offscreen interaction result, not live desktop or win/loss acceptance.


The public-header MPW environment fixture was also rebuilt for both CPUs. It now
uses GetApplLimit rather than a fixed $10400000 address and exercises lowering,
MaxApplZone preservation and restoration through actual SetApplLimit calls.
Both rebuilt applications reach Exit(0) and the existing success marker. Build
and focused execution evidence: /tmp/systemless-appllimit-mpw-build.txt and
/tmp/systemless-appllimit-mpw-tests.txt. The initial broad run exposed this old
fixture's fixed-address expectation; it was corrected before the final gate.


### Shared device state advances EV to its color-window call

GetGDevice now returns a real owned display record with a PixMap/color table,
referencing the current monochrome screen storage. Device identities and globals
are shared by both CPUs; registered record fields and palette data are read live.
The initial one-bit profile remains honest: this does not turn on Color QuickDraw
capability flags, create color windows or claim a 256-color screen.

The unchanged original EV 1.0.5 now passes GetGDevice and reaches unsupported
NewCWindow ($AA45), using the recorded resource fork, graphics and a 2,000,000
boundary budget: /tmp/systemless-ev-device-next.txt. This is a color-port/window
foundation requirement, not a title/menu or gameplay result.

Centaurian's original PPC load separately passes its former strong Index2Color
import and now fails at InterfaceLib BackPixPat:
/tmp/systemless-centaurian-device-next.txt. It still does not execute guest code.
Marathon's previously verified sound-version requirement is unchanged by these
services; neither original target game has reached gameplay.

The public-header devices.c fixture checks GDevice/PixMap sizes, current/main/list
identity, next-device termination, flags, low-memory globals, actual screen-memory
aliasing, and live RGB lookup on both real CPU engines. Separate GPU acceptance
copies the actual device PixMap and a read-only 8-bit source through both CopyBits
ABIs and checks byte-exact output/padding plus GPU readback. Exact device/parser
limits are in architecture.md; no full native color parity is implied.


Final device-enabled refresh retains Marathon's 68K $A800 sound-version boundary
and the PPC strong-import boundaries SndSoundManagerVersion (EV) and
NewRoutineDescriptor (Marathon). Evidence:
/tmp/systemless-marathon-device-refresh.txt,
/tmp/systemless-ev-device-ppc-refresh.txt and
/tmp/systemless-marathon-device-ppc-refresh.txt.

Super Mines' original Save/Open sequence was rerun after device trap publication
and indexed-source decoding. It reopened node 19's 1,972-byte data fork and
returned to Waiting{ticks:10} at PC $0100C798; its final board image was visually
inspected. Evidence: /tmp/systemless-mines-device-reload.txt and
/tmp/systemless-mines-device-reload-after.png. This retains the prior bounded
offscreen interaction result; live desktop and win/loss acceptance remain gaps.


### Owned color-port foundation for EV's window path

OpenCPort/InitCPort/CloseCPort now create, reset and dispose actual shared
CGrafPort attached records, with borrowed device palette/screen lifetime and
caller-owned port storage. Both real CPUs pass a public-header fixture that
inspects sizes/defaults, resets patterns, opens independent ports and checks
that closing one preserves the other port and both device/custom color tables.
The fixture posts marker $C907 and exits successfully. Evidence:
/tmp/systemless-color-ports-mpw-build-final.txt and
/tmp/systemless-color-ports-mpw-final.txt.

This is a prerequisite for NewCWindow, not an implementation of that window
call or color-port drawing. EV's next recorded blocker remains NewCWindow;
Marathon's sound-version and PPC routine-descriptor requirements remain. Legacy
operations that would misinterpret color-port fields now fail before mutating
them. The monochrome display/profile is unchanged, and neither original target
is playable. Full port/drawing limitations are in architecture.md.


A final original-game refresh confirms EV still stops at NewCWindow ($AA45),
and Marathon with its companion volume still stops at SoundDispatch ($A800):
/tmp/systemless-ev-color-ports-next.txt and
/tmp/systemless-marathon-color-ports-next.txt. This stage adds the required
port owner/lifecycle; it does not advance either original beyond that boundary.
Super Mines Save/Open completed after publication of the new port traps, reopened
its 1,972-byte game and returned to Waiting{ticks:10} at PC $0100C798. Its final
PPM is byte-identical to the preceding device milestone's inspected board.
Evidence: /tmp/systemless-mines-color-ports-reload.txt and associated after.ppm.


A bounded final development trace captures EV's actual $AA45 frame at PC
$00015B7C/SP $0080FAC4: nil caller storage, bounds pointer $0080FB4C, title
pointer $003FEFEE, visible=false, plainDBoxProc=2, behind=$FFFFFFFF,
goAway=false and refCon=0. The Boolean's padding byte is nonzero ($94) and must
not be treated as part of the visible value. Evidence:
/tmp/systemless-ev-color-window-frame.txt. The tracer now reports bounded stack
bytes on a 68K service fault, without changing guest state. The next window slice
must support this standard hidden dialog and use color-port bitmap/pattern
accessors rather than interpreting CGrafPort offsets 8/32 as inline bitmap data.


### Color-port rendering prerequisite

The rebuild draws through actual CGrafPort PixMap and old-style PixPat records
on its existing one-bit display. Both CPU-created ports pass exact GPU pixel and
handle-preservation checks. Text and control paths use the same checked layout.
This removes the inline-layout hazard before NewCWindow integration; it does not
yet advance EV past that trap or Marathon past sound startup.

The original binaries were refreshed after the drawing changes. EV still faults
at service 43589 ($AA45), and Marathon with its companion volume at 43008
($A800). Evidence: /tmp/systemless-ev-layout-next.txt and
/tmp/systemless-marathon-layout-next.txt. Super Mines Save/Open reopened its
1,972-byte game, returned to Waiting{ticks:10} at $0100C798, and produced a final
PPM byte-identical to the preceding inspected color-port milestone. Evidence:
/tmp/systemless-mines-layout-reload.txt and associated after.ppm.


### EV passes color-window creation and reaches sound startup

The original EV 1.0.5 CODE application successfully creates a real hidden color
window from bounds (0,0,0,0), procID 2, behind -1 and Boolean padding $94. Shared
window drawing/geometry/lifetime support is verified on both CPUs and with a
public-header CWindowRecord fixture. EV then passes BitAnd ($A858), SetGDevice
($AA31) and FreeMem ($A01C), using actual registered display identity and actual
allocator free space. The final bounded trace stops at SoundDispatch $A800,
D0=$000C0008, PC=$0001436C, SP=$0080FABE: SndSoundManagerVersion. Evidence:
/tmp/systemless-ev-free-memory-next.txt and
/tmp/systemless-ev-newcwindow-sound-trace.txt. No sound version is fabricated.
This advances EV beyond its previous NewCWindow blocker; it is not gameplay.
Marathon's observed sound boundary and both original PPC blockers remain.

Super Mines' original Save/Open sequence was rebuilt and rerun after the window
integration. It reopened its 1,972-byte game, returned to Waiting{ticks:10} at
$0100C798 and produced a final PPM byte-identical to the preceding inspected
milestone. Evidence: /tmp/systemless-mines-cwindow-reload.txt and associated
after.ppm. Marathon with its companion volume still stops at $A800:
/tmp/systemless-marathon-cwindow-next.txt. These remain bounded offscreen checks;
original-game live desktop and gameplay acceptance are incomplete.


### Shared sound foundation before the version boundary

SndDoImmediate can now render actual standard sampled buffers through both CPU
adapters and a shared mixer. Public-header compiler fixtures verify exact PCM,
completion/disposal and headless CLI progress. Both compiled guests also produced
matching samples consumed by the real native output callback. This does not yet
implement the SndSoundManagerVersion request reached by the original games;
queued/resource playback, sound callbacks and additional formats remain required.
No version value or capability was fabricated to get past startup. Neither
original EV nor Marathon has verified gameplay.

Final source refreshes preserve the original games' actual $A800 startup fault:
/tmp/systemless-ev-sampled-next.txt and /tmp/systemless-marathon-sampled-next.txt.
A separate Super Mines volume-seven diagnostic reaches missing HGetState $A069;
sound-enabled Mines is not accepted. The existing initial-volume profile remains
unchanged, and the compiled sampled-sound fixture explicitly sets volume seven
for actual PCM/native-output acceptance.

The retained-profile Super Mines Save/Open rerun completes, reopens its
1,972-byte game and returns to Waiting{ticks:10} at $0100C798. Its final PPM is
byte-identical to the preceding inspected milestone. Evidence:
/tmp/systemless-mines-sampled-final-reload.txt and associated after.ppm. This
preserves the earlier muted-profile result, not sound-enabled acceptance.


### Sound-enabled Mines reaches resource playback

With explicit diagnostic volume seven, the original now passes its prior
HGetState ($A069) and MoveHHi ($A064) boundaries, with real shared metadata and
checked heap movement. It loads snd 12244, creates a callback-bearing sound
channel, preserves residency flags and next calls SndPlay ($A805). The resource
is format 1, sampledSynth with init $A0, and one offset bufferCmd pointing to a
standard 8-bit mono header at rate $56EE8BA3. Evidence:
/tmp/systemless-mines-movehi-sound-full.txt. Sound-enabled gameplay remains
unaccepted until resource/queued playback and completion callbacks execute.

After the handle changes, the retained-profile Save/Open regression still
reopens the 1,972-byte game and returns to its event wait at $0100C798. Final
pixels match the preceding milestone byte for byte:
/tmp/systemless-mines-handle-profile-reload.txt and associated after.ppm.
This does not change the separate sound-enabled SndPlay limitation above.


### FIFO/callback runtime regression

The command queue and same-ISA sound callback milestone does not yet implement
SndPlay or provide a sound version result. Original EV/Marathon startup and
sound-enabled Mines acceptance therefore remain at their recorded boundaries.
The retained-profile Mines Save/Open rerun still reopens the 1,972-byte game and
returns to Waiting{ticks:10} at $0100C798. The final PPM is byte-identical to the
handle-state milestone. Evidence: /tmp/systemless-mines-queue-profile-reload.txt
and associated after.ppm. Native desktop gameplay and sound-enabled resource
playback remain separate, unfinished checks.


### Original sound companion metadata for resource playback

Bounded static inspection of the actual companion resource forks gives concrete
next fixtures; it does not execute their commands or prove playback. Private
bytes remain outside Git. Raw resource-fork identities:

| Companion | Bytes | SHA-256 |
| --- | ---: | --- |
| EV Sounds | 902115 | `9adf841b8bcbed925bd856f04f651a7941f7efb1baeb4a6eb0b0ee781cc7bf69` |
| EV Music | 976932 | `23e9e0992133dc5388354f034a7a7e4766beb510a6b51cf600facb72e1e426ac` |
| Marathon Sounds | 1021569 | `d8c7a8139c56a367d8b3081ea2fefc9558fa34d0feec4c826c16ff195e17abd3` |

EV Sounds has nine format-1 and fifty format-2 snd resources; EV Music adds four
format-2 resources. Marathon Sounds has 104 format-1 resources. Every scanned
sound uses one offset bufferCmd ($8051), a standard encoding-zero header and
inline samples within its resource bounds. Format-1 initializers are sampledSynth
with $A0. Rates are approximately 11.127 or 22.255 kHz. No snd resources occur in
the scanned application forks or Marathon Music resource fork; this does not
exclude streamed/generated sound or other formats in actual game execution.

Fifteen EV Sounds headers have loop metadata that fails the current strict
loop bounds check (some equal endpoints, others beyond the sample length), and
one has baseFrequency zero. The current header validator would reject these;
do not silently normalize assets. The next playback slice must resolve which
fields bufferCmd actually uses and add command-specific compatibility evidence.
Apple's header contract specifies baseFrequency 1..127 for pitch calculations;
its role in buffer playback needs to stay distinct from note synthesis. Evidence:
/tmp/systemless-original-game-sound-metadata.txt and
https://dev.os9.ca/techpubs/mac/Sound/Sound-74.html.


### Resource-sound execution evidence

The new sound-resource probe accepted 59 EV Sounds, four EV Music and 104 Marathon
Sounds entries through synthetic 68K and PPC callers. All 167 matching 32-frame
PCM prefixes passed the cross-adapter assertion; each call then advanced silently
to completion and released its internal channel. This includes EV's unused loop/
pitch metadata cases. Resource formats/bytes are not changed. The probe does not
execute original application startup, render complete waveforms, open an audio
device or establish original EV/Marathon gameplay. Evidence:
/tmp/systemless-ev-sounds-final-playback.txt,
/tmp/systemless-ev-music-final-playback.txt and
/tmp/systemless-marathon-sounds-final-playback.txt.

Sound-enabled original Super Mines completes the bounded Save/Open script with
volume seven and 120 virtual ticks per input step. Sound time now advances with
that explicit probe clock. Its actual callback returns once, restores the source
handle's saved state ($60) and quietly disposes its channel. Later channel reuse
also completes; the 1,972-byte game is reopened and the program reaches its normal
Waiting{ticks:10} at $0100C798. Its final board screenshot was visually inspected.
Evidence: /tmp/systemless-mines-resource-sound-clocked.txt and after.ppm/png.
This replaces the previous sound-enabled SndPlay stop for this script, not native
audio/desktop/win-loss acceptance. The retained muted/zero-tick profile separately
completes Save/Open with final pixels identical to the queue checkpoint:
/tmp/systemless-mines-resource-muted-profile.txt and after.ppm. Do not compare
random-board/timer pixels across the two different virtual-time profiles.


### 2026-10-16 sound status, extended PCM and handle-copy refresh

Original EV 1.0.5 68K passes SndSoundManagerVersion and HandToHand, then stops
at SetDeviceAttribute (AA2D), PC $00014942. Its offscreen graphics setup creates
its own GDevice handle; this is not evidence of rendered gameplay. Trace:
/tmp/systemless-ev-hand-copy-next.txt. Original Marathon v1.2 68K passes the sound
version call but explicitly reports "Sorry, Marathon requires Color QuickDraw."
The subsequent error dialog stops at A985, PC $010108B2. Trace:
/tmp/systemless-marathon-sound-info-next.txt. The virtual profile still correctly
does not claim complete Color QuickDraw; do not flip a flag to bypass the check.

Current PPC loader diagnostics, with actual application resource forks: EV
requires missing strong PBHGetVolParmsSync; Marathon still requires
NewRoutineDescriptor. Logs /tmp/systemless-ev-ppc-sound-info-next.txt and
/tmp/systemless-marathon-ppc-sound-info-next.txt. Neither original is playable.

The extended PCM renderer refresh again accepts all 167 original sound resources
through synthetic 68K/PPC callers. The 32-frame prefix hashes and completion/
cleanup results match the prior checkpoint exactly. Logs:
/tmp/systemless-ev-sounds-stereo-check.txt,
/tmp/systemless-ev-music-stereo-check.txt and
/tmp/systemless-marathon-sounds-stereo-check.txt. This does not establish complete
waveform, host audio, or original-game playback acceptance.


### Offscreen device attribute refresh

EV 1.0.5 68K now passes SetDeviceAttribute on its application-owned offscreen
GDevice. It next stops at MakeITable, AA39, PC $00014964. Trace:
/tmp/systemless-ev-device-attributes-next.txt. This is further setup execution,
not a rendered title screen or gameplay. Marathon's prior color-capability and
PPC routine-descriptor requirements remain unresolved.


### Inverse-table refresh

EV 1.0.5 68K now completes four MakeITable requests using its actual color-table
and inverse-table handles. It next selects its application-created offscreen
GDevice (handle 79676); selection is still limited to the initial display, so it
stops at PC $00017694. /tmp/systemless-ev-inverse-detail.txt prints the failed
service argument explicitly. No title/menu/gameplay is established. Original
Marathon's color capability and PPC routine-descriptor requirements remain open.


### Selected offscreen devices and empty PaintRect refresh

EV now selects its owned offscreen devices, completes two empty PaintRect calls
and restores the main display. The verified empty request is Rect{0,0,0,0}, not
nonempty drawing that may be discarded. The next stop is Get1Resource, A81F,
PC $00016168, native arguments resource type 'RBlt', ID 1. Evidence:
/tmp/systemless-ev-empty-paint-next.txt and the preceding
/tmp/systemless-ev-offscreen-paint-detail.txt. Neither title/menu nor gameplay is
established. Full color rendering and Marathon's outstanding requirements remain.

The original Super Mines sound-enabled Save/Open refresh still completes one
callback, reopens the 1,972-byte game and returns to its normal event wait.
/tmp/systemless-mines-offscreen.txt and after.ppm/png; the inspected board pixels
match /tmp/systemless-mines-status-stereo-after.ppm byte for byte. Native and
win/loss acceptance remain separate.


### Current-map lookup refresh

EV 1.0.5 68K executes twelve Get1Resource calls for RBlt/MBlt IDs and reaches
GetNewDialog (A97C), PC $0002577C. The unconsumed frame specifies dialog 3000,
nil storage and front-window placement. /tmp/systemless-ev-get1-next.txt. The
resource lookup barrier is removed; no dialog/title/gameplay has been verified.


### Resource-backed dialog refresh

EV 1.0.5 68K now creates DLOG/DITL 3000 through GetNewDialog, receives a
real 170-byte DialogRecord and sets its port. It next stops at DialogDispatch
($AA68), D0=$0304 (SetDialogDefaultItem), PC $000257A8. Evidence:
/tmp/systemless-ev-dialog-next.txt. This establishes record/control construction,
not complete dialog rendering, modal interaction, title/menu or gameplay.
The new shared implementation copies DITL, retains source resources, owns control
and raw static-text handles, and loads actual picture/icon resource handles.
Controls are drawn; non-control item update drawing remains unfinished. Neither
original game is playable. Marathon's Color QuickDraw and PPC routine-descriptor
requirements remain unchanged.


### Default-button refresh

EV 1.0.5 68K passes the exact $AA68/$0304 default-item call, ShowWindow and
SelectWindow. It next requests DrawDialog ($A981), PC $000257D2, for the real
dialog 3000. /tmp/systemless-ev-default-next.txt. Shared default-button selection
and outline drawing are tested separately through both CPUs, including GPU
readback, hidden/show and outline replacement. Full item drawing, modal events,
title/menu and gameplay remain unverified. Marathon's requirements are unchanged.


### Dialog contents refresh

EV 1.0.5 68K passes DrawDialog ($A981), retrieves its button/static-text items and
uses native QuickDraw to frame the button. It next requests SetDialogItemText
($A98F), PC $00025816. The guest string reports an error opening the Monitor Tool;
this dialog is an error path, not title/menu/gameplay. The cause is not yet
verified. /tmp/systemless-ev-draw-dialog-next.txt. The shared rendering extension
is separately tested with public text/parameter, ICON and scaled PICT fixtures
through both actual CPUs and full GPU/bitmap comparison. Next replace/redraw item
text and investigate the Monitor Tool failure without fabricating capability or
operation success. Marathon's requirements remain unchanged.


### Item text and Monitor Tool diagnostics

EV 1.0.5 68K now updates/draws its error text, flushes events and reaches
ModalDialog ($A991), PC $0002582C. The complete guest message reports Monitor Tool
error ID -4. Current-device PixMap depth is 1; of requested RBlt/MBlt resource IDs
1,2,4,8,16,32, only ID 8 exists. /tmp/systemless-ev-itemtext-next.txt. The mismatch
is evidence to investigate rather than proof of the precise error cause. The
bounded instruction trace is /tmp/systemless-ev-monitor-instructions.txt.
Actual indexed-color destinations/displays are a priority for EV and Marathon;
do not relabel one-bit storage or synthesize operation success. The modal error
UI and full gameplay remain unfinished.


### Indexed offscreen foundation

Packed 2/4/8-bit destination drawing, shared RGB setters and initialization from
selected indexed devices now pass real 68K/PPC tests and a public-header compiled
fixture. This does not yet change the main one-bit display or advertise full
Color QuickDraw. EV still reports Monitor Tool ID -4 and reaches ModalDialog
($A991), PC $0002582C; Marathon still reports that Color QuickDraw is required and
reaches Alert ($A985), PC $010108B2. Evidence:
/tmp/systemless-ev-indexed-foundation.txt and
/tmp/systemless-marathon-indexed-foundation.txt. Neither game is playable.

Original Super Mines again completes the sound-enabled Save/Open sequence,
reopens its 1,972-byte game, completes one actual sound callback and returns to
WaitNextEvent at PC $0100C798. Its final screenshot was inspected and is byte-for-byte
identical to /tmp/systemless-mines-offscreen-after.ppm. Evidence:
/tmp/systemless-mines-indexed-foundation.txt and matching -after.ppm. Native
window/audio and win/loss acceptance are still separate unfinished work.


### Real color main display

The optional `--display 640x480x8` profile now has actual indexed storage, a live
host RGB332 table, matching main device and coherent screen ports. SysEnvirons
reports color only for this configured profile. Original Marathon v1.2 68K
passes its initial Color QuickDraw rejection and continues through file/working-
directory queries. It next requests PBHSetVol ($A215), PC $01001ECE. Trace:
/tmp/systemless-marathon-color-display.txt, with the existing asset-volume
snapshot. This is startup progress, not title/menu or movement/combat acceptance.

Original EV 1.0.5 still reports Monitor Tool ID -4 with genuine depth-8 main
pixels, then reaches ModalDialog ($A991), PC $000260B4. Trace:
/tmp/systemless-ev-color-display.txt. Depth alone therefore does not explain that
error. No success result or trap availability was fabricated. The first color
trace before changing environment reporting is separately preserved at
/tmp/systemless-ev-color-screen-first.txt. PPC loader blockers remain unchanged.

Final-source original refresh uses /tmp/systemless-marathon-color-display-complete.txt
(PBHSetVol $A215, PC $01001ECE), /tmp/systemless-ev-color-display-complete.txt
(Monitor Tool ID -4, ModalDialog $A991, PC $000260B4), and
/tmp/systemless-mines-color-display-complete.txt (restored game, one sound
callback, normal wait). The final Super Mines -after.ppm was inspected and is
byte-identical to the preceding indexed-foundation checkpoint. These refreshes
use the completed visible-color-window composition code.


### Default-directory selection

Original Marathon v1.2 68K now passes real PBHSetVol ($A215) after its directory
queries. It proceeds to OpenRFPerm ($A9C4), PC $010023C6, on the actual color
profile with the existing asset-volume snapshot. Initial evidence:
/tmp/systemless-marathon-setvol-first.txt; final-source refresh:
/tmp/systemless-marathon-setvol-final.txt. Resource-file opening must install a
real parsed resource map and preserve file/handle ownership; a returned file
reference alone will not satisfy the next milestone. No title/menu or gameplay
acceptance is claimed. EV's known Monitor Tool error and PPC gaps remain separate.

Final-source refresh: /tmp/systemless-marathon-setvol-complete.txt confirms
OpenRFPerm ($A9C4), PC $010023C6; /tmp/systemless-ev-setvol-complete.txt retains
Monitor Tool ID -4/ModalDialog ($A991), PC $000260B4. Super Mines again reopens its
1,972-byte game, completes one actual sound callback and returns to normal wait
at $0100C798. Its final screenshot was inspected and its -after.ppm is byte-for-byte
identical to the preceding color-display checkpoint. Evidence:
/tmp/systemless-mines-setvol-complete.txt. Native-window presentation remains
unverified; no new native smoke pass is claimed.


Resource-file maps now share real catalog resource forks and File Manager access
paths through OpenRFPerm/OpenResFile, UseResFile, CloseResFile and HomeResFile on
both CPU adapters. Lookup starts at the selected map and follows older maps;
handles are scoped by owning file as well as type/ID. Named lookups and shared
font/menu/dialog consumers follow the same selection. Reopening an existing map
returns its reference without selecting it. Preload failure rolls back attached
allocations and the access path; closing releases only attached resources from
that file, retaining detached handles. Managed access paths reject raw close and
resource-fork writes that would invalidate parsed maps. CurMap ($0A5A) reflects
API selection. SysMap ($0A58) is not fabricated or aliased to the application.

Limits remain explicit: at most 64 external maps and 64 MiB aggregate external
fork bytes; compressed preload decoding, changed-map writeback, guest map-header
handles/TopMapHndl, direct low-memory map-chain manipulation, general System
resources and full Resource Manager compatibility remain unfinished. The built-in
cursor fallback at reference zero does not represent an installed System file.

Original Marathon now reaches sound-channel creation and next requests InsTime
($A058), PC $010122DA. The instruction trace confirms ResError = 0 after OpenRFPerm; reaching this
point is not gameplay.
Escape Velocity retains its monitor setup boundary. Neither original game nor
the whole rebuild is complete.


Resource-map checkpoint validation: four new final-source tests pass, zero
failures/skips, exit 0 (/tmp/systemless-resource-maps-four-tests.txt). These cover
actual 68K/PPC opening/selection/closing/home calls, duplicate IDs with distinct
names, fallback search, managed raw-close protection, detached lifetimes, errors,
native result boundaries and preload rollback. The preceding 68 selected
regressions passed before the final selection/builtin/preflight refinements;
that earlier run is not claimed as final-source full acceptance. Final-source
strict all-target lint exits 0 (/tmp/systemless-resource-maps-lint.txt), sequential
changed-file formatting and git diff --check pass.

Original traces: /tmp/systemless-{marathon,ev}-resource-maps.txt. Marathon's
instruction trace (/tmp/systemless-marathon-resource-maps-instructions.txt) shows
ResError = 0 after OpenRFPerm, then three sound channels and missing InsTime
($A058). EV retains Monitor Tool -4 and missing ModalDialog ($A991). The refreshed
Super Mines regression (/tmp/systemless-mines-resource-maps.txt) completes one
sound callback, reopens 1,972 bytes and waits at $0100C798; its inspected screenshot
is byte-identical to /tmp/systemless-mines-setvol-complete-after.ppm. No native
window/audio or original-game gameplay pass is claimed.

Full all-target acceptance remains RUNNING, not passed, in command session 98583
(/tmp/systemless-resource-maps-all-tests.txt). Resume that same job rather than
restarting it. It has completed compilation and its first library/CPU/graphics
modules without a failure so far. Record its final exit and totals before
claiming broad acceptance. Next executable implementation: shared Time Manager
task ownership, scheduling and state-preserving same-ISA callbacks for Marathon;
continue EV's genuine SysMap/FCB/Monitor Tool diagnosis. Keep the rebuild active.


## Shared Time Manager execution

InsTime, PrimeTime and RmvTime now operate on one process-owned task queue through
both CPU adapters. Task records retain real qLink order, qType active bits and
live callback addresses. Deadlines use checked monotonic units of 1/3 microsecond:
50 units per virtual instruction, 50,000 per elapsed host tick. This preserves the
existing deterministic 1,000-instructions-per-tick policy without counting the
same instruction tick twice. Positive delays are milliseconds; negative values
are negated microseconds. Nil callbacks expire normally. RmvTime returns unused
time in negated microseconds where representable, otherwise positive milliseconds.
Guest calendar/TickCount writes do not rewrite monotonic timer deadlines.

Runtime dispatches due tasks at instruction boundaries, in deadline/installation
order, clears active state before entry, and retains installed tasks for rearming.
Callbacks receive A1 on 68K or r3 through a validated same-ISA PPC TimerUPP
(procInfo $B802). The entire interrupted CPU is suspended and restored, including
engine-private/FPU state; waits are suspended and restored through the same shared
continuation model as sound callbacks. Timer callbacks are serialized and may
rearm themselves. Host event waits are shortened to the next timer tick; waiting
time elapses even while callbacks run. 68K interrupt masking defers delivery.

This is deterministic guest scheduling, not a native hard real-time guarantee.
The queue is explicitly bounded at 256 records. Invalid/overlapping/duplicate
records, changed managed linkage, unsupported descriptor transitions, invalid
callback frames and unsafe record access fail explicitly. InsXTime/fixed-frequency
extended timing, Microseconds, general hardware interrupts/PPC interrupt masking,
interrupt-time service restrictions and production Mixed Mode remain unfinished.
No extended Time Manager Gestalt capability is advertised.

With the actual 640x480x8 profile and asset volume, original Marathon now installs
and primes a real 33 ms task, then reaches PaletteDispatch HasDepth (selector
$0A14, trap $AAA2), PC $010116CE. Evidence:
/tmp/systemless-marathon-timers-first.txt. This does not establish title/menu,
original timer callback execution, movement or combat acceptance. EV's monitor
error and PPC loader gaps remain separate; neither original game is playable.
The full rebuild goal remains active.


Timer checkpoint verification: seven final integration tests and two internal
ownership/clock tests pass with zero failures/skips. The focused calendar,
sound-queue and waiting regressions pass 16 further tests; the earlier six-timer
run is superseded by the final seven-case run. The compiled timer fixture passes
on both SC68K and MrC/PPC, executing three real callbacks per guest, cancellation,
rearming, idle resumption and ExitToShell. SC callback entry uses the Pascal A1
reader from IM: Processes listing 3-3; a register-parameter declaration alone
incorrectly generated a stack parameter and was corrected in the fixture. The
runtime continues to supply the documented register ABI. The diagnostic trace
now supports --elapse-waits for bounded virtual idle-time investigations.

Evidence: /tmp/systemless-timers-seven-tests.txt,
/tmp/systemless-timers-final-ownership.txt,
/tmp/systemless-timers-focused-final.txt,
/tmp/systemless-timers-sdk-final.txt and
/tmp/systemless-timers-sdk-verified.txt. All original compiler artifacts rebuilt;
final timer artifacts were regenerated from the corrected volatile-counter source.
Strict all-target lint passes /tmp/systemless-timers-lint-final.txt (exit 0), and
the final tracing/test diagnostics pass /tmp/systemless-timers-diagnostics-lint.txt
(exit 0). Sequential changed-file formatting and whitespace checks pass.

The older resource-map broad suite was deliberately stopped as SUPERSEDED because
source and selected binaries changed during the timer milestone. Its exit is 143,
not a pass; it cannot establish a single final-source checkpoint. Replacement
complete acceptance covers all 79 integration targets in two normal Cargo batches,
40/39 targets, with the library, binary and all five examples in batch 1 and all
ignored tests enabled in both. Coverage is recorded in
/tmp/systemless-timers-full-coverage.json. Both jobs remain running:
command sessions 21627 and 76629; logs /tmp/systemless-timers-full-{1,2}.txt.
Continue these exact jobs. Keep runtime/test/compiler source fixed until both
finish; source fingerprint /tmp/systemless-timers-full-source.sha256 is
c188ed76478efa9531be7c04002816f832488e4268061507aff6b9d68f289e43.
Do not claim a full pass until both exits, totals, failures and skips are inspected.
Documentation edits do not affect that code fingerprint.

Final original traces /tmp/systemless-{marathon,ev}-timers-final.txt confirm
Marathon's next HasDepth boundary and EV's unchanged Monitor Tool -4/ModalDialog
boundary. Refresh original Super Mines after the current probe_mouse build
(command session 37344, /tmp/systemless-timers-mouse-probe-build.txt), before
claiming the shared callback/wait refactor preserves that interaction regression.
Next implementation after full acceptance: actual display mode/capability
semantics for HasDepth/SetDepth, with real device records and pixels, then further
Marathon startup. EV's System-resource FCB/Monitor Tool path remains a separate
actual missing-capability diagnosis. No original-gameplay or full-rebuild claim.


Final timer-source Super Mines refresh passes, exit 0:
/tmp/systemless-mines-timers-final.txt. It completes one sound callback, reopens
its 1,972-byte game and returns to event waiting at $0100C798. The final screenshot
was inspected and is byte-identical to the resource-map checkpoint. Evidence:
/tmp/systemless-mines-timers-final-after.ppm and -after.png. This closes the
interaction-refresh step above; native presentation/audio and win/loss acceptance
remain open. Both exhaustive acceptance batches remain running and must be resumed
before changing runtime/test/compiler source. The full rebuild stays active.


Fresh EV diagnosis on the unchanged timer checkpoint now proves propagation of
the earlier SysMap/FCB lead. /tmp/systemless-ev-timers-current-instructions.txt
runs the original EV with actual 640x480x8 pixels; concise checked evidence is
/tmp/systemless-ev-current-error-propagation.txt. Original CODE 12 bytes at $72E0
are 31 78 0A 58 00 18 70 08 A2 60 66 24: copy SysMap ($0A58) into the real FCB
parameter block, then invoke FCBInfo selector 8. Instruction 798545 at $0001B2EC
calls it; 798546 receives -51. The helper returns a nonzero error, and instruction
798557 at $00019974 assigns -4 to D3. The function returns that value through D0,
the caller retains its word, and instructions 799413–799421 pass it to NumToString.
The trace records Decimal(Format { number: -4, ... }) and the resulting Monitor
Tool ID = -4 text, then the known missing ModalDialog boundary at $000260B4.
This is now verified value propagation, not merely the first observed -4.

This fallback exposes a missing ROM-free owned System resource-file /
boot-directory capability, but later diagnosis below identifies an earlier
trap-discovery error that avoids this fallback. SysMap is the
System resource-file reference, not CurApRefNum; do not alias it to the app FCB
or force success. Real map/search/handle ownership, file metadata and folder
identity must agree. Resolving this first actual failure does not prove that the
rest of Monitor Tool initialization or gameplay will work. Neither original game
nor the full rebuild is complete.


### Completed acceptance and Escape Velocity trap discovery

Both frozen timer-checkpoint acceptance batches completed with exit 0: 311 and
194 tests, 505 total, zero failures and zero ignored. All 79 integration targets
and the library, binary and five examples were covered with ignored cases enabled.
The source fingerprint was verified unchanged before subsequent implementation.
Earlier references to sessions 21627/76629 as running are now superseded.

The Rust 1.99 compatibility patch was validated separately with strict all-target
Clippy, 114 affected existing tests (zero failed/ignored), and the Rust 1.93 minimum
compiler check. All 12 promoted files match those validated sources exactly.
These mechanical slice-array changes preserve the previous remainder handling.

EV's earlier fallback diagnosis is superseded as the immediate startup blocker:
the runtime already implements SysEnvirons, but its trap table omitted $A090.
The original tests GetOSTrapAddress($90) against Unimplemented, then falls back to
the SysMap lookup. Registering the real existing service avoids that fallback;
the isolated original trace now invokes SysEnvirons and advances to missing
trap $AA3D at $0001ACFE, after selecting the current graphics device. Evidence:
/tmp/systemless-ev-sysenv-experiment.txt. No System file was fabricated or aliased
to application resources. Other unadvertised traps require individual review.
A new actual-CPU regression checks PPC/68K lookup identity and calls the returned
68K veneer, verifying the shared profile, register results and restored stack.
Neither original EV nor Marathon has reached verified gameplay. Marathon remains
at HasDepth ($AAA2 selector $0A14). The full rebuild remains active.


Final-source confirmation: /tmp/systemless-ev-trap-main-original.txt again reaches
ProtectEntry $AA3D at $0001ACFE after invoking discovered SysEnvirons. Diagnostic
exit 1 is the explicit remaining unsupported service, not a playable-game pass.
The discovery/call regression and related environment, loading and trap-patching
checks pass 43 cases, zero failed/ignored: /tmp/systemless-ev-trap-focused.txt.
Final Rust 1.99 strict all-target lint passes:
/tmp/systemless-ev-trap-final-clippy.txt. Full acceptance above belongs to the
preceding frozen timer checkpoint; it is not relabeled as a full final-source run.


### Marathon advances through backed display-depth discovery

The new shared HasDepth/SetDepth calls expose the host-selected display's actual
mode (gdMode 128), with real mono or indexed8 storage. Only the configured depth
and color format are supported; SetDepth accepts that mode ID or raw depth and
returns cDepthErr (-157) for other configurations without changing records or
pixels. Runtime depth switching, grayscale/direct color and more video devices
remain unfinished. The native ABI uses D0 selectors $0A14/$0A13 with ten argument
bytes and a word result, not a stacked selector.

Original Marathon, with unchanged raw-fork hash
`afa4e77d8b53c42062fe4b730472cfbb396aacf715c2181aeb5dd12ea60491a1`,
now invokes HasDepth(device=$00010884, depth=8, whichFlags=1, flags=1), passes
that startup check, allocates further regions/handles, and reaches missing
GetNewCWindow ($AA46), PC $01007F10. Trace:
/tmp/systemless-marathon-display-modes-first.txt. Reproduction:

    future/target/debug/examples/trace_application code /tmp/systemless-original-marathon/application.raw-rsrc 1000000 --graphics --display 640x480x8 --volume /tmp/systemless-marathon-assets.slv --services-only

Original EV's raw hash remains
`d739c1d5bd0d5c7efddc55f34bfadd8f66a25c819a00bbed9523c7c56e3f716b`;
its final-source regression still reaches ProtectEntry ($AA3D), PC $0001ACFE:
/tmp/systemless-ev-display-modes-regression.txt. Both diagnostic exits are 1 for
explicit missing services, not passes for gameplay. Neither original game nor
the complete rebuild is finished. Native presentation/audio remain unverified.

Both real Apple-header compiler guests pass five display calls and exit 0:
/tmp/systemless-display-modes-sdk-tests.txt. Compiler fixture generation succeeds
in /tmp/systemless-display-modes-sdk-build.txt. It checks HasDepth/current gdMode,
unsupported color depth, SetDepth by mode/raw depth and failure preservation.


Final display-mode acceptance: 18 distinct device/display/mode integration tests
plus the compiler-fixture test pass (19 checks), zero failures/ignored. Actual
GPU cases were explicitly enabled. The final mode-target rerun after applying the
project's explicit-GPU annotation passes all five cases with --include-ignored,
including readback; none skipped. Evidence:
/tmp/systemless-display-modes-tests.txt,
/tmp/systemless-display-modes-final-tests.txt,
/tmp/systemless-display-modes-sdk-tests.txt.
Strict final-source all-target Rust 1.99 Clippy passes, exit 0:
/tmp/systemless-display-modes-clippy-final-acceptance.txt.
Sequential changed-file formatting and git diff --check pass. The earlier
505-test full acceptance remains evidence for its frozen timer checkpoint; no
new full-suite pass is claimed for this mode milestone. Native presentation and
full runtime depth switching are still unverified/unfinished, respectively.


### Marathon reaches display-refresh callback installation

Shared resource-backed color windows now load live WIND/wctb and create actual
CGrafPort ownership graphs. Marathon's own wctb 128/129 each specify five black
window parts; these are window RGB settings, not display CLUT entries. It creates
and shows both color windows, resizes the first to 640x480, then queries HasDepth
and accepts its real mode ID through SetDepth. The next missing call is
SlotVInstall ($A06F), PC $01009364, task $0001821C, slot 0, callback $010093BE.
It still does not reach a verified title/menu or gameplay.

Evidence: /tmp/systemless-marathon-color-window-first.txt, diagnostic exit 1.
Reproduce with the previous Marathon trace command and the same original files.
Retrace.h verifies this is display-retrace scheduling, not SlotManager:
/tmp/systemless-marathon-slot-vbl-sdk.txt. EV preserves ProtectEntry ($AA3D),
PC $0001ACFE: /tmp/systemless-ev-color-window-regression.txt, exit 1.

Synthetic color-window tests inspect actual GPU content/frame colors, both CPU
ABIs, copied live resource settings, caller/heap record ownership, attached handle
teardown and malformed resources. Arbitrary document-title themes/custom WDEFs
and public auxiliary-window color APIs remain unsupported and explicit. The full
rebuild, native presentation/audio and original-game acceptance remain unfinished.


Final resource-color-window validation passes 21 focused checks, no failures or
ignored cases: /tmp/systemless-color-window-final-tests.txt (20 integration tests,
including actual GPU readback) and /tmp/systemless-color-window-sdk-test.txt
(one compiler-fixture test, both engines). Strict all-target Rust 1.99 Clippy:
/tmp/systemless-color-window-acceptance-lint.txt, exit 0. Sequential changed-file
formatting and whitespace checks pass. The preliminary run was superseded with
exit 130 after correcting classic color-aware GetNewWindow; it is not a pass.
Neither original game nor the full rebuild is complete.


## VBL checkpoint (2026-10-16)

Original Marathon 1 v1.2 now installs its slot-0 vertical retrace task and reaches
GetMaxDevice ($AA27), PC $01034B98, rectangle $0080FEC8. The task at $0001821C
has count/phase zero and no completed callback at this boundary. Installation
is proven; original-game callback execution and gameplay remain unverified.
Evidence: /tmp/systemless-marathon-vbl-word-acceptance.txt, explicit exit 1.
Original Escape Velocity 1.0.5 remains at ProtectEntry ($AA3D), PC $0001ACFE:
/tmp/systemless-ev-vbl-word-acceptance.txt, explicit exit 1. Neither is playable.

Both CPU compiler fixtures execute/rearm three real VBL callbacks and cancel a
phased system task. Final focused acceptance is 108 passed, zero failed/ignored;
see progress.md for terminal logs and limitations. Super Mines save/open replay
exits 0, reopens 1,972 bytes and completes one sound callback; its final image is
byte-identical to the timer checkpoint. Evidence:
/tmp/systemless-mines-vbl-word-acceptance.txt and corresponding -after.ppm.
Native presentation/audio and win/loss acceptance remain open. Next executable
work is shared GetMaxDevice, followed by further Marathon startup and real
per-device palette protection for EV. The full rebuild remains active.


## GetMaxDevice checkpoint (2026-10-16)

Shared GetMaxDevice ($AA27/PPC import) now returns the one registered host display
when its bounds intersect the global rectangle by positive area, otherwise nil.
It validates input before lazy allocation, preserves selected offscreen device
state and rejects an unregistered display list. Multiple host displays and depth
ranking among them remain unfinished. Apple's QuickDraw-288 and QD13 specify
greatest depth, not intersection area; sources are in architecture.md.

Nine device integration checks pass with zero failed/ignored, including both CPU
ABIs, empty/inverted/disjoint/one-pixel rectangles, offscreen selection retention,
and the real GPU CopyBits/readback regression. Evidence:
/tmp/systemless-maxdevice-tests.txt, terminal exit 0. Strict Rust 1.99 all-target
Clippy passes: /tmp/systemless-maxdevice-lint.txt, exit 0. Changed-file sequential
formatting and whitespace checks pass. No full-suite rerun is claimed.

Original Marathon passes GetMaxDevice, allocates 121,856-byte drawing storage and
a 122-byte port, then stops at NewGDevice ($AA2F), PC $01034C08. Evidence:
/tmp/systemless-marathon-maxdevice.txt, diagnostic exit 1. Its VBL task remains
dormant and no callbacks complete before this boundary. EV's previously verified
ProtectEntry boundary is unchanged by scope; it was not replayed this checkpoint.
Neither original game is playable. Next: shared owned NewGDevice/DisposeGDevice
lifecycle with correct detached offscreen records and public ABI fixtures, then
continue Marathon startup. Keep the full rebuild active.


## Detached graphics-device checkpoint (2026-10-16)

Both CPU adapters now share real no-driver NewGDevice(mode -1) and owned
DisposeGDevice/DisposGDevice. Four independent handles retain a detached GDevice,
PixMap, empty ColorTable and inverse table. Allocation rollback returns nil and
MemError -108 without leaking storage; disposal frees the complete original graph.
Main/foreign/current-device disposal and replaced supporting handles fail
explicitly. Driver-backed modes, handle replacement/transfer and separate system
heap allocation remain unfinished; no screen clone or false driver support is used.

Final acceptance: 11 device integration checks (including real GPU readback) plus
the updated pinned-SDK compiler fixture on both CPU engines pass: 12 distinct
checks, zero failed/ignored. Logs: /tmp/systemless-newdevice-tests.txt and
/tmp/systemless-newdevice-sdk-test.txt. Pinned compilation:
/tmp/systemless-newdevice-sdk-build.txt. Strict final all-target Rust 1.99 lint
passes (/tmp/systemless-newdevice-final-lint.txt); sequential changed-file format
and whitespace checks pass. No full-suite or new minimum-version run is claimed.

Original Marathon passes NewGDevice(0,-1), fills its offscreen records, then
requests DisposeCTable ($AA24), PC $01034C96, argument $00010C30. Diagnostic exit 1:
/tmp/systemless-marathon-newdevice.txt. Trap identity verified against the pinned
SDK: /tmp/systemless-marathon-aa24-sdk.txt. No completed sound/timer/VBL callbacks
precede this boundary. EV was not replayed; its previous ProtectEntry boundary is
still the latest evidence. Neither original game is playable. Next executable
slice: real ColorTable disposal/replacement ownership and subsequent Marathon
setup, preserving main palettes and detached device cleanup. Full rebuild stays
active; draft PR and no merge/release/deployment restrictions remain in force.


## ColorTable replacement checkpoint (2026-10-16)

DisposeCTable ($AA24/PPC import) now frees a checked owned ColorTable and forgets
its detached-device ownership before the master pointer can be reused. It leaves
the caller's PixMap field unchanged; the caller installs its replacement.
DisposeGDevice can then free an independent live replacement, or a nil table,
while rejecting main display storage, structural handles and replacement tables
shared with another detached device. Replacement of PixMap/inverse handles and
general borrowed-table lifecycle remain unfinished. Main palettes stay protected.

Final acceptance: 13 device tests plus one pinned-SDK compiler-fixture test running
both CPUs pass, zero failed/ignored. GPU CopyBits/readback executed. Logs:
/tmp/systemless-disposetable-final-tests.txt and
/tmp/systemless-disposetable-final-sdk-test.txt (its filtered device target runs
zero tests; only the one MPW result is counted). The earlier 12-test run predates
final guards and is preliminary. Compiler build:
/tmp/systemless-disposetable-sdk-build.txt, no compiler/linker errors.
Strict Rust 1.99 all-target lint passes:
/tmp/systemless-disposetable-acceptance-lint.txt. Sequential changed-file formatting
and whitespace checks pass. No full-suite or minimum-version rerun is claimed.

Original Marathon now disposes the initial table, duplicates its chosen table,
builds a 3-bit inverse table and selects the offscreen device. It then faults in
ColorPort Open at PC $01034CEE with Memory read address 0. This is a runtime fault,
not another missing trap or a gameplay pass. Evidence:
/tmp/systemless-marathon-disposetable.txt, exit 1. The current PixMap has a zero
baseAddr during setup; investigate color-port initialization's use of pixel-storage
validation for default color metadata. No callbacks complete before this boundary.
EV was not replayed and remains at its previously observed ProtectEntry boundary.
Neither original is playable. Full rebuild stays active; next work is the actual
color-port fault and subsequent Marathon startup.


## Color-port metadata checkpoint (2026-10-16)

Fixed the original Marathon ColorPort Open read-at-zero fault. Default color
selection only needs PixMap geometry/format and palette metadata; indexed parsing
no longer reads its pixel buffer. Actual source/destination drawing still unpacks
checked storage and rejects a missing buffer. New tests exercise initialization
before offscreen buffer attachment through both CPUs and explicitly assert drawing
still fails with nil storage.

Final acceptance: 63 checks pass, zero failed/ignored: 38 library, five color-port,
13 device and seven indexed-destination tests. GPU cases were enabled and executed.
Logs: /tmp/systemless-port-metadata-tests.txt and
/tmp/systemless-port-metadata-drawing-tests.txt, both terminal exit 0. Strict
Rust 1.99 all-target lint passes (/tmp/systemless-port-metadata-lint.txt).
Sequential changed-file formatting and whitespace checks pass. This is focused
acceptance, not a full-suite or new minimum-version result.

Original Marathon passes color-port setup and completes its first real Timer
Manager callback (completed timer=1, sound=0, VBL=0). It then requests Elems68K /
Pack5 ($A9EC), PC $0103CE22, selector $001E with extended operand at $0080FDBA.
The pinned SDK Traps.h confirms the package identity:
/tmp/systemless-marathon-a9ec-sdk.txt. Evidence:
/tmp/systemless-marathon-port-metadata.txt, explicit missing-service exit 1.
No title/menu/gameplay or audible-output acceptance is inferred. EV was not
replayed and remains at its previously verified ProtectEntry boundary. Neither
original game is playable. Next: document and implement the actual SANE elementary
operation, numeric representation/exception behavior and ABI, then continue
Marathon. Keep the full rebuild active and PR in draft.


## Extended arctangent checkpoint (2026-10-16)

Implemented shared default-nearest SANE FATANX ($A9EC, selector $001E) with the
pinned astro-float-num 0.3.7 pure-Rust engine. Native ABI marshals a ten-byte
extended destination, preflights memory and pops six bytes. Numeric evaluation
preserves all 64 significand bits; signed zero, infinity, NaN payload quieting and
subnormal inputs are explicit. Process sticky status is retained, but public SANE
environment/alternate rounding/precision/halt handlers, other selectors, FP68K
and PPC math imports remain unfinished. Noncanonical operands fault explicitly.
No complete SANE capability or CPU math parity is advertised.

Final acceptance: 52 CPU/service checks pass, zero failed/ignored (39 library,
two numeric integration, seven timer and four trap patch tests).
/tmp/systemless-sane-acceptance.txt, terminal exit 0. The new numeric check proves
atan(1) and atan(1 + 2^-63) differ in the correct lowest output bit. Signed and
special cases, native register/frame preservation, bad destinations and unsupported
selectors are tested. Strict Rust 1.99 all-target lint passes:
/tmp/systemless-sane-lint-final.txt. Rust 1.93 library check passes:
/tmp/systemless-sane-msrv.txt. Sequential changed-file formatting and whitespace
checks pass. Initial compilation/lint errors were corrected and superseded; they
are not acceptance results. No full-suite, GPU suite or wasm build is claimed.

Original Marathon consumes the extended pi/4 arctangent and reaches FP68K /
Pack4 ($A9EB), PC $0101DC00, selector $0004 (FMULX), destination $0080FE10,
source $0080FE1A. Pinned SANEMacs.a confirms FATANX and FMULX:
/tmp/systemless-sane-sdk-atan.txt and /tmp/systemless-sane-sdk-multiply.txt.
Evidence: /tmp/systemless-marathon-sane-atan.txt, missing-service exit 1;
completed timer=1, sound=0, VBL=0. Native title/menu/gameplay and audible output
remain unverified. EV was not replayed and its last boundary is ProtectEntry.
Neither original is playable. Next: shared FP68K extended arithmetic and conversion
with correct rounding/range/exception state, then continue Marathon startup.
Full rebuild remains active; PR stays draft.


## Extended multiply and binary64 conversion checkpoint (2026-10-16)

Shared numerics now handles FP68K FMULX ($0004), FX2D ($0810) and FD2X ($080E).
Multiplication retains the full u128 product and rounds once, including direct
subnormal rounding, overflow and SANE NANMUL code 8. Conversions round extended
straight to binary64 and expand binary64 exactly, including its smallest subnormal.
Operands are snapshotted for aliases and native adapters preflight actual 8/10-byte
extents before popping ten-byte frames. Process exception status remains sticky;
nearest-even and after-rounding tininess are the current modeled policy. The
manual permits either tininess convention. Environment APIs/alternate modes,
halts, other FP68K selectors, noncanonical values and PPC math imports remain open.

Final acceptance: 59 CPU/service checks pass, zero failed/ignored: 42 library,
six numeric integration, seven timer and four trap-patch tests. Evidence:
/tmp/systemless-sane-arithmetic-pair-acceptance.txt, terminal exit 0. Tests cover
low bits, halfway ties, signed zero/infinities/NaNs, exact/inexact subnormals,
rounding up to minimum normal, overflow, conversion extents and alias snapshots,
register/frame preservation and bad-source atomicity. Earlier multiply-only and
one-way conversion runs are preliminary and not added to the 59 count. Final
strict Rust 1.99 all-target lint and Rust 1.93 library checks pass:
/tmp/systemless-sane-arithmetic-pair-final-lint.txt and
/tmp/systemless-sane-arithmetic-pair-msrv.txt. Sequential changed-file formatting
and whitespace checks pass. No new full-suite/GPU/wasm result is claimed.

Original Marathon passes extended multiplication and both conversion directions.
It next requests FDIVX ($A9EB selector $0006), PC $0101DC2A, destination $0080FE1A,
source $0101DFC4. Evidence: /tmp/systemless-marathon-sane-arithmetic-pair.txt,
explicit unsupported-selector exit 1. Completed timer=1, sound=0, VBL=0. The
original title/menu/gameplay and native audio remain unverified. EV was not
replayed and retains its previously recorded ProtectEntry boundary. Neither game
is playable. Next: exact shared extended division, then continue Marathon startup.
Keep the full rebuild active and PR in draft.


## Extended division and integer checkpoint (2026-10-16)

Shared numerics now handles FDIVX ($0006), signed 16-bit-to-extended ($200E) and
signed 32-bit-to-extended ($280E). Division uses normalized integer significands
and an exact u128 quotient/remainder, with one nearest-even rounding step and
direct subnormal rounding. Distinct sticky divide-by-zero/invalid/overflow/
inexact-underflow behavior includes SANE NaN division code 4. Integer expansion
is exact, including both minimum signed values. Native adapters read only the
actual two/four/ten source bytes and preflight output before popping ten bytes.
Public SANE environment/alternate precision/rounding/halt APIs, other selectors,
noncanonical values and PPC math imports remain unfinished.

Final acceptance: 65 checks passed, zero failed/ignored: 46 library, eight numeric
integration, seven timer and four trap patch checks. The independent arbitrary-
precision oracle matches 256 normal-range division vectors. Known vectors cover
one-third, subnormal operands/results, exact versus inexact underflow, overflow,
signed zeros, infinities and invalid division; ABI tests cover operand order,
register/frame preservation and exact source widths. Evidence:
/tmp/systemless-sane-division-integer-acceptance.txt, terminal exit 0. Earlier
division-only passes are preliminary, not added to this count. Strict Rust 1.99
all-target lint and Rust 1.93 library checks pass:
/tmp/systemless-sane-division-integer-lint.txt and
/tmp/systemless-sane-division-integer-msrv.txt. Sequential formatting and whitespace
checks pass. No new full-suite/GPU/wasm result is claimed.

Original Marathon now executes division, integer expansion and additional multiply/
divide/binary64 conversions before requesting FATANX's sibling FSINX: Elems68K
$A9EC selector $0018, PC $0103CDFA, destination $0080FDB0. Evidence:
/tmp/systemless-marathon-sane-division-integer.txt, unsupported-selector exit 1.
Completed timer=1, sound=0, VBL=0; no title/menu/gameplay acceptance. EV was not
replayed and retains its previously recorded ProtectEntry boundary. Neither game
is playable. Next: shared extended sine with proper range reduction and special
cases, then continue original Marathon startup. Full rebuild stays active and
PR remains draft.


## Extended trig and copy checkpoint (2026-10-16)

Shared numerics now handles FSINX/FCOSX/FTANX ($A9EC selectors $18/$1A/$1C), with
64-bit significands and high-precision argument reduction. Result signs come from
the evaluation; signed zeros/tiny values/NaNs/infinities have explicit status and
SANE NANTRIG code $21 handling. FX2X ($A9EB/$000E) is a checked ten-byte conversion
that quiets signaling NaNs and retains invalid status rather than a blind copy.
Alternate environment/rounding/precision/halt APIs, other operations and PPC math
imports remain unfinished; no complete SANE compatibility is claimed.

Final acceptance: 69 checks passed, zero failed/ignored: 48 library, ten numeric
integration, seven timer and four trap patch tests. Evidence:
/tmp/systemless-sane-trig-copy-acceptance.txt, terminal exit 0. Fifteen independent
Decimal Machin/Taylor reference vectors match exactly, including 2^64, 2^1000,
negative inputs and a one-bit input difference; generator results agree at two
working precisions. Reproduce with tests/reference/trig_decimal.py. Tiny and
special values, native frame/register behavior and signaling-NaN copies are tested.
Strict Rust 1.99 all-target lint and Rust 1.93 library checks pass:
/tmp/systemless-sane-trig-copy-lint.txt and /tmp/systemless-sane-trig-copy-msrv.txt.
Sequential changed-file formatting and whitespace checks pass. Earlier trig-only
runs are preliminary, not added to this count. These are mathematical reference
checks, not proof of ROM-identical approximations; no new full/GPU/wasm run.

Original Marathon passes sine, additional arithmetic and extended copy, then
requests FP68K Classify ($A9EB selector $001C), PC $0103CE36. Stack records pointers
$0080FD94 and $0080FD9A; confirm the classification output ABI before implementing.
Evidence: /tmp/systemless-marathon-sane-trig-copy.txt, explicit unsupported-selector
exit 1. Pinned SANEMacs.a identifies FOCLASS:
/tmp/systemless-sane-sdk-001c.txt. Completed timer=1, sound=0, VBL=0. Original
cosine/tangent execution, title/menu/gameplay and native audio remain unverified.
EV was not replayed and retains its recorded ProtectEntry boundary. Neither game
is playable. Next: shared number classification and its real native result ABI,
then continue Marathon startup. Full rebuild stays active and PR remains draft.


## Native classification checkpoint (2026-10-16)

FCLASSX ($A9EB/$001C) now invokes shared numeric inquiry and returns signed
16-bit MC68000 class codes 1–6, rather than the C enum's 0–5. Negative zero
returns -4. Signaling NaNs remain unchanged and do not raise invalid or inexact.
The adapter validates the ten-byte source and two-byte destination before popping
ten frame bytes. Other source formats and noncanonical encodings remain explicit
unsupported cases; public SANE environment/modes/halts and PPC math access are open.

Final acceptance: 70 checks pass, zero failed/ignored: 48 library, eleven numeric
integration, seven timer and four trap patch tests. New native inquiry coverage
exercises all six classes with both signs, exact two-byte output guards, readonly
unaltered source (including signaling NaNs), exception status and stack cleanup.
Evidence: /tmp/systemless-sane-classify-acceptance.txt, terminal exit 0.
Strict Rust 1.99 all-target lint and Rust 1.93 library checks pass:
/tmp/systemless-sane-classify-lint.txt and /tmp/systemless-sane-classify-msrv.txt.
Sequential changed-file formatting and whitespace checks pass. No new full/GPU/
wasm suite or native-system conformance run is claimed. Primary ABI evidence:
Apple Numerics Manual, chapter 20, p. 153/table 20-2 and
/tmp/systemless-sane-classify-sdk.txt.

Original Marathon consumes the correct class and reaches FP68K ProcEntry
($A9EB/$0017), PC $01002DFC, environment destination $0080FDA4. Evidence:
/tmp/systemless-marathon-sane-classify.txt, explicit unsupported-selector exit 1.
Completed timer=1, sound=0, VBL=0. Next executable slice is real SANE environment
save/reset/restore and exception-state serialization, not a hardcoded success.
Title/menu/gameplay and native audio remain unverified. EV was not replayed and
retains its previously recorded ProtectEntry boundary. Neither game is playable.
Full rebuild stays active; PR remains draft.


## Environment and comparison checkpoint (2026-10-16)

FGETENV/FSETENV/FPROCENTRY/FPROCEXIT now serialize actual shared exception flags,
replace them, save/reset them and restore/merge procedure exceptions respectively.
All use the MC68000 address operand convention and six-byte native frame. Only
implemented default nearest/extended/no-halt controls are accepted; other controls
and reserved bits fault before state changes. Low-memory environment storage,
exception test/set, alternate modes/halts and PPC math imports remain unfinished.

FCMPX/FCPXX compare canonical extended operands in shared numerics; the native
reply adapter marshals logical relation to XNZVC ($19 less, $04 equal, $00 greater,
$02 unordered), preserving upper SR and integer registers. Both operands can be
readonly; signed zeros compare equal. Ordinary comparison signals invalid for
signaling NaNs; the signaling variant does so for any NaN. Inputs are not quieted.

Final acceptance: 73 checks passed, zero failed/ignored: 48 library, fourteen
numeric integration, seven timer and four trap patch cases. Evidence:
/tmp/systemless-sane-env-compare-final-acceptance.txt, terminal exit 0. Real
zero-divisor arithmetic inside a saved procedure verifies exception merging;
failed/readonly saves preserve status, and unsupported controls preserve state.
Comparison tests cover native condition bits, operand order, readonly unchanged
NaNs, signed zero, negative ordering and variant exception differences. An initial
comparison fixture switched supervisor stacks; the fixture was corrected to retain
its stack while testing SR bits. That failed run and earlier environment-only
passes are superseded, not added to the final count. Strict Rust 1.99 all-target
lint and Rust 1.93 library checks pass:
/tmp/systemless-sane-env-compare-final-lint.txt and
/tmp/systemless-sane-env-compare-msrv.txt. Sequential formatting and whitespace
checks pass; no new full/GPU/wasm suite is claimed. Primary references: Apple
Numerics Manual chapters 20/22 and /tmp/systemless-sane-env-sdk.txt.

Original Marathon saves its real $1000 inexact environment and executes two
comparisons. Its next observed requirement is FSUBX ($A9EB/$0002), PC $01002E32,
destination $0080FDA6, source $0080FDB0. Evidence:
/tmp/systemless-marathon-sane-env-compare.txt, explicit unsupported-selector exit 1.
Completed timer=1, sound=0, VBL=0; original ProcExit and title/menu/gameplay remain
unverified. EV was not replayed and retains its previously recorded ProtectEntry
boundary. Neither game is playable. Next: shared extended subtraction/addition
with exact cancellation and correct rounding, then continue Marathon startup.
Full rebuild stays active and PR remains draft.


## Exact sums and square-root checkpoint (2026-10-16)

FADDX/FSUBX now retain full-precision alignment and exact cancellation before a
single nearest-even packing step. Subnormal destinations round directly from the
exact result. Extended exponent bounds cap alignment storage at 33,024 bits.
Signed zero, infinities, propagated NaNs, SANE NANADD code 2 and overflow are
explicit. FSQRTX uses software 64-bit extended precision across the full canonical
input range; exact roots do not add inexact, negative zero is preserved, and
negative finite/infinite input produces NANSQRT code 1 with invalid status.
Alternate modes, other source formats and noncanonical encodings remain open.

Final acceptance: 77 checks passed, zero failed/ignored: 50 library, sixteen numeric
integration, seven timer and four trap patch tests. Evidence:
/tmp/systemless-sane-add-sqrt-acceptance.txt, terminal exit 0. Tests cover deep
cancellation, a one-bit subnormal difference, halfway ties, widest exponent gaps,
overflow, signed zero, exact/inexact roots, smallest-subnormal square root, native
operand order, output bytes and frame/register preservation. The earlier 75-check
addition-only run is preliminary and not added to this count. Strict Rust 1.99
all-target lint and Rust 1.93 library checks pass:
/tmp/systemless-sane-add-sqrt-lint.txt and /tmp/systemless-sane-add-sqrt-msrv.txt.
Sequential formatting and whitespace checks pass. No new full/GPU/wasm suite or
native-system conformance run is claimed.

Original Marathon passes multiple sums/differences, square root, further arithmetic,
and its real environment Get/Set/Exit sequence. Original ProcExit execution is now
verified (earlier checkpoints only proved it with fixtures). It next requests
FMULD ($A9EB/$0804), PC $0101DD5C: extended destination $0080FE1A, binary64 source
$0080FDE6. Evidence: /tmp/systemless-marathon-sane-add-sqrt.txt, explicit unsupported-
selector exit 1. Completed timer=1, sound=0, VBL=0. Title/menu/gameplay and native
audio remain unverified. EV was not replayed and retains its prior ProtectEntry
boundary. Neither game is playable. Next: typed mixed-source SANE arithmetic,
starting with binary64 multiplication through the existing shared engine, then
continue Marathon startup. Full rebuild stays active and PR remains draft.


## Typed arithmetic and integral rounding checkpoint (2026-10-16)

Native arithmetic now decodes typed Extended/Double/Single/Integer/Long sources
and one Add/Subtract/Multiply/Divide operation. Exact source expansion feeds the
existing shared extended engines; redundant native arithmetic branches were
removed. Source extents remain 10/8/4/2/4 bytes and operands are snapshotted.
Exception changes commit only with successful output, including source NaN
quieting. Comp/96-bit formats, mixed comparisons, alternate controls and PPC math
imports remain unsupported explicitly. FRTIX/FTTIX now produce integral extended
values using default nearest-even or explicit truncation, preserving signed zero
and recording discarded fractions without spurious underflow.

Final acceptance: 81 checks pass, zero failed/ignored: 51 library, nineteen numeric
integration, seven timer and four trap patch tests. Evidence:
/tmp/systemless-sane-formats-round-acceptance.txt, terminal exit 0. The native matrix
runs all twenty source-format/operation combinations with readonly exact-size
source mappings and output guards. Additional tests cover binary32/64 subnormals,
signaling-NaN payload/status, rollback on failed operations, rounding ties,
negative truncation, tiny-to-zero behavior and native frame/register preservation.
Earlier format-only runs and the focused NaN replay are preliminary/overlapping,
not added to this count. Strict Rust 1.99 all-target lint and Rust 1.93 library
checks pass: /tmp/systemless-sane-formats-round-lint.txt and
/tmp/systemless-sane-formats-round-msrv.txt. Sequential formatting and whitespace
checks pass. No new full/GPU/wasm suite is claimed.

Original Marathon passes FMULD, FDIVD, further arithmetic and FTTIX. It next
requests FX2I ($A9EB/$2010), PC $0103741A, signed 16-bit destination $0080FDB8,
extended source $0080FE06. Evidence:
/tmp/systemless-marathon-sane-formats-round.txt, unsupported-selector exit 1.
Completed timer=1, sound=0, VBL=0. Title/menu/gameplay and native audio remain
unverified. EV was not replayed and retains its prior ProtectEntry boundary.
Neither game is playable. Next: real extended-to-integer conversion with proper
rounding/range/invalid behavior, then continue Marathon startup. Full rebuild
stays active and PR remains draft.


## Finite integer storage conversion checkpoint (2026-10-16)

Native FX2I/FX2L now call one shared finite extended-to-signed-16/32 conversion.
Nearest-even rounding precedes signed range checks without a binary64 intermediate.
Source snapshots, exact destination sizes and transactional status preserve aliases,
readonly sources and failure behavior. Signed zero, tiny values, halfway ties and
both integer limits are covered. NaNs, infinities and rounded out-of-range values
remain explicit unsupported errors, preserving output/status; compatible exceptional
integer encodings and invalid status are not claimed. Other environment modes and
PPC math imports remain open.

Final frozen-source acceptance: 83 checks pass, zero failed/ignored: 51 library,
21 numeric integration, seven timer and four trap patch tests. Evidence:
/tmp/systemless-sane-integer-storage-acceptance.txt, terminal exit 0. The earlier
83-check run is preliminary/overlapping and not added to this count. Strict Rust
1.99 all-target lint and Rust 1.93 library checks pass:
/tmp/systemless-sane-integer-storage-lint.txt and
/tmp/systemless-sane-integer-storage-msrv.txt. Sequential formatting and whitespace
checks pass. No new full/GPU/wasm suite or native-system conformance is claimed.

Original Marathon executes four FX2I conversions and continues into file startup.
The next boundary is unregistered FSDispatch $A060 with D0 selector $001A,
PC $01001D2E, A0 parameter block $0080FE6C. Evidence:
/tmp/systemless-marathon-sane-integer-storage.txt, explicit Service(41056) exit 1.
Completed timer=1, sound=0, VBL=0. The A260 variant already routes selector $001A
to hierarchical data-fork open; verify the A060 form and parameter-block contract
before adding its route. Title/menu/gameplay and native audio remain unverified.
EV was not replayed and retains its prior ProtectEntry boundary. Neither game is
playable. Next: verify and implement Marathon's actual FSDispatch request, then
continue original startup; complete exceptional integer conversions with primary
or reference evidence. The full rebuild stays active and the PR remains draft.


## Basic fork opens and typed comparison checkpoint (2026-10-16)

PBOpenDFSync now resolves through both native $A060/$001A and PPC imports into
the shared data-fork service. The hierarchical $A260 variant retains ioDirID;
the basic form reads only 28 bytes and ignores offset 48. Both use real access
paths, permissions and error results. Apple SDK trap evidence:
/tmp/systemless-opendf-sdk.txt. Async dispatch remains unsupported.

FCMP/FCPX now accept Extended/Double/Single/Integer/Long source formats through
one typed shared NumericCompare service. Arithmetic and comparison share exact
source expansion. Readonly operands, native ten-byte frames, extended low-bit
relations, CCR/upper SR preservation and quiet/signaling NaN status are verified.
Status changes roll back on rejected noncanonical operands. Comp, 96-bit source
formats, alternate controls and PPC math imports remain explicit gaps.

Final acceptance: 113 checks pass, zero failed/ignored: 51 library, 23 numeric,
seven file I/O, nine file metadata, six resource-file, six working-directory,
seven timer and four trap patch checks. Evidence:
/tmp/systemless-opendf-compare-acceptance.txt, terminal exit 0. Native file tests
exercise both basic/hierarchical routes on both CPUs, exact 28/52-byte mappings,
shared fork lengths and ignored/used directory fields. The comparison matrix
executes both operations across five formats with readonly exact-size sources;
extra tests cover binary32/64 NaNs and transactional status. The initial file-only
run exposed an undersized test PEF import area, corrected by allocating enough
data storage. That failed run and the later preliminary run are superseded and
not added to the final count. Strict Rust 1.99 all-target lint and Rust 1.93 library
checks pass: /tmp/systemless-opendf-compare-lint.txt and
/tmp/systemless-opendf-compare-msrv.txt. Sequential formatting and whitespace
checks pass. No new full/GPU/wasm or native-system conformance run is claimed.

Original Marathon now executes the basic open, repeated double comparisons and
further resource loading, with 62 completed timer callbacks. Next is GetCTable
$AA18, ID 130 ($0082), PC $01012E70. Evidence:
/tmp/systemless-marathon-opendf-compare.txt, explicit Service(43544) exit 1.
The original resource fork contains clut 130: 160 bytes, ctSize 18 (19 entries).
The preceding open-only replay stopped at FCPXD $080A with 53 timer callbacks;
that diagnostic is superseded by the final replay. Completed sound=0, VBL=0;
the installed zero-count VBL remains dormant. Title/menu/gameplay and native
audio remain unverified. EV was not replayed and retains its prior ProtectEntry
boundary. Neither game is playable. Next: GetCTable resource lookup and independent
table allocation/seed/lifetime semantics, then continue Marathon; finish EV's
per-device palette protection and reservation. The full rebuild stays active and
PR #4802 remains draft.


## Resource-backed color tables checkpoint (2026-10-16)

GetCTable now reaches one shared resource/allocator implementation from native
$AA18 and the PPC import. It returns independent copies of live clut resources,
preserving their seed, flags and ColorSpec values while clearing handle resource,
lock and purge attributes. Existing DisposeCTable semantics dispose copies without
freeing the resource or another copy. Missing custom IDs return NIL/ResError;
malformed tables fault explicitly and copy allocation failure returns NIL with
MemError -108. Unbacked standard system color tables remain explicitly unsupported.
Seed retention follows Apple's primary resource documentation rather than the
legacy implementation's unconditional reseeding.

Final acceptance: 104 checks pass, zero failed, one GPU-marked device test ignored
in the normal run: 51 library, three new color-table, twelve device, 23 numeric,
four resource-map, seven timer and four trap patch checks. Evidence:
/tmp/systemless-getctable-acceptance.txt, terminal exit 0. New tests cover both
CPU adapters, signed IDs, independent allocations, seed/flags/content retention,
live resource edits, release/disposal lifetimes, empty/malformed/missing tables
and real heap exhaustion. The ignored device test was then explicitly executed
on a real adapter, together with the existing GPU text test: both pass, zero
failed/ignored, /tmp/systemless-getctable-text-gpu.txt, terminal exit 0. These are
two distinct offscreen GPU checks, not a full GPU/native presentation suite. The
text check verifies current glyph transfer modes, clipping and port preservation;
it does not complete the new all-text/library requirement.

Strict Rust 1.99 all-target lint and Rust 1.93 library checks pass:
/tmp/systemless-getctable-lint.txt and /tmp/systemless-getctable-msrv.txt.
Sequential formatting and whitespace checks pass. Initial commands naming absent
color_manager/color test targets did not run tests and are not counted. No new
full-suite, browser or native-system conformance claim is made.

Original Marathon passes GetCTable(130), looks up finf 128, and next calls TextFont
with family 4, PC $01010E86. It faults because the current QuickDraw port is NIL.
Evidence: /tmp/systemless-marathon-getctable.txt, explicit graphics-error exit 1.
Earlier GetPort/SetPort save/restore calls restore NIL; investigate Window Manager
initial/current-port lifecycle against primary/reference behavior before changing
it. Completed timer=62, sound=0, VBL=0. Title/menu/gameplay and native audio remain
unverified. EV was not replayed and retains its prior ProtectEntry boundary.
Neither game is playable. Next: resolve Marathon's current-port/text context and
continue its startup, and prototype the library-backed GPU text path described in
[text-rendering.md](text-rendering.md). The full rebuild, including the user-added
complete wgpu text requirement, stays active and PR #4802 remains draft.


## GPU-resident text mask transfers checkpoint (2026-10-16)

The shared Ports/Surface/Graphics path now consumes a same-device binary red-channel
Rgba8Unorm texture. Glyphon coverage can feed the existing Blit kernel directly;
all eight source transfers, inversion, RGB matching, clipping and packed storage
remain shared with CopyBits. Ordinary CPU sources retain the same behavior via
an unused valid texture binding. Invalid texture format/usage/sample/layer/size
and source/output aliases are rejected before dispatch. No mask readback/reupload
is needed by the bridge; destination readback remains the established synchronous
guest-memory boundary. Binary coverage is the contract; antialiasing/color-glyph
interpretation remains open.

Final frozen-source acceptance: 83 checks pass, zero failed/ignored with GPU cases
explicitly included: 51 library, four bitmap, five color-port, eight indexed,
four text, seven timer and four trap patch checks. Evidence:
/tmp/systemless-gpu-mask-final-acceptance.txt, terminal exit 0. The new indexed
matrix compares packed bytes and RGBA against existing CopyBits for 2/4/8-bit
targets and all eight modes, with colored foreground/background, negative bounds,
partial clipping, padding and output guards. Invalid mode/format/output aliases
preserve guest bytes and do not poison GPU error state. Existing independent RGB
mode tests and both-CPU drawing fixtures also pass in this run. The earlier
83-check run is preliminary/overlapping and not added to this count.

The real Glyphon probe now shares the runtime Graphics device and passes sixteen
cases on M1/Metal: its original six cases (including two library-gap
characterizations), all eight monochrome transfer equations, double-XOR
cancellation and direct guest-write coherence. Evidence:
/tmp/systemless-glyphon-transfer-final.txt, terminal exit 0. Strict Rust 1.99 lint
passes for runtime and probe: /tmp/systemless-gpu-mask-final-lint.txt and
/tmp/systemless-gpu-mask-probe-lint.txt. Rust 1.93 checks pass for the production
locked graph and probe: /tmp/systemless-gpu-mask-runtime-msrv.txt and
/tmp/systemless-gpu-mask-final-msrv.txt. Sequential formatting and whitespace
checks pass. Production dependencies are unchanged. This selected GPU suite does
not claim full graphics, native-window, browser or performance acceptance.

Original Marathon retains the previous TextFont family 4/no-current-port boundary,
PC $01010E86, with timer=62, sound=0, VBL=0. Evidence:
/tmp/systemless-marathon-gpu-mask.txt, explicit graphics-error exit 1. The shared
shader change causes no earlier observed startup failure. EV was not replayed
and retains its previous ProtectEntry boundary. Neither game is playable.
Production text calls still use the established GPU coverage path. Next: bounded
font-generation cache identities, glyph/cell/metrics/pen integration and actual
68K/PPC text/UI entry points through the library bridge; continue Marathon's port
lifecycle investigation. The complete text and full rebuild goals remain active.


## Color client identity prerequisite (2026-10-16)

SetClientID now reaches one shared device service from the 68K AA3C Pascal
procedure and PPC InterfaceLib import. It writes the selected live GDevice's
16-bit gdID field (+2), including signed/high-bit values and zero, without
changing another device's identity. This is required groundwork for client-aware
palette protection/reservation and matching; ProtectEntry itself remains
unsupported, so Escape Velocity's last stop is unchanged and neither priority
game is claimed playable.

The [Apple Color Manager summary](https://dev.os9.ca/techpubs/mac/ACI/ACI-108.html)
defines the client procedure; its
[device CLUT discussion](https://dev.os9.ca/techpubs/mac/ACI/ACI-107.html)
distinguishes protection against palette changes from reservation against color
matching. The legacy implementation is comparison evidence, not authoritative
proof of full protection behavior. Follow-up must implement live table metadata,
client ownership, QDError, invalidation and effects on SetEntries/color matching.

Validation: /tmp/systemless-client-id-tests-final.txt terminal exit 0, 13 passed,
1 GPU-only test ignored. The new fixture invokes both real CPU adapters and
checks main/detached device isolation and replacing/clearing IDs. Strict runtime
and probe lint passes in /tmp/systemless-client-id-lint.txt exit 0. Formatting and
whitespace checks pass. The earlier test run failed because its fixture requested
unsupported driver mode 0; final uses the documented detached mode -1. This is
not an original-game replay or full color-manager acceptance.


## Live palette protection (2026-10-16)

ProtectEntry is shared by the 68K AA3D Pascal procedure and PPC import. It
protects/unprotects an entry in the current device's live owned ColorTable,
reports cProtectErr (-154) on duplicate protection and cRangeErr (-153) on an
invalid index, preserves RGB/client bytes, and increments ctSeed after successful
updates. Write preflights precede both metadata/seed writes. The high-byte
protection bit is the rebuild's private manager representation, not a claim of
byte-for-byte ROM-private metadata compatibility. Device-table matching ignores
this protection bit because protection does not reserve/hide a color; unknown
reservation tags remain explicitly unsupported.

Primary references:
[Apple Color Manager chapter, mirrored SpInside Macintosh](https://spinsidemacintosh.neocities.org/im202),
Color Table Management and inverse-table invalidation;
[Apple MacErrors.h](https://github.com/apple-oss-distributions/CarbonHeaders/blob/main/MacErrors.h),
cRangeErr/cProtectErr constants. SetEntries/reservation/ownership matching are
still missing, so palette modification and animation acceptance remain open.
No successful SetEntries operation can yet bypass protection because that API
remains unsupported rather than silently implemented.

Validation: five color-manager/inverse-table tests pass on both real CPU adapters
in /tmp/systemless-protect-verified.txt, terminal exit 0; strict test lint passes
in /tmp/systemless-protect-verified-lint.txt. The fixture covers noncanonical
Boolean true values, padding/stack preservation, cross-CPU duplicate errors,
seed changes, inverse rebuilding, unchanged RGB/client data, invalid indices,
and isolation from a selected detached device. Earlier extended-fixture runs
failed first on an uninitialized detached palette and then use of a private
helper; these were corrected using public guest handle resizing/writes. Those
runs are superseded, not additional acceptance counts.


Final-source original Escape Velocity 1.0.5 replay:
/tmp/systemless-ev-protect-final.txt, terminal exit 1 at explicit unsupported
ReserveEntry AA3E, PC 0001AD04, after successful ProtectEntry(index 0, false).
This supersedes the previous AA3D / 0001ACFE boundary. Replay used one million
instructions and the original resource fork with graphics at 640x480x8; no
companion volume was mounted in this bounded startup run. No callbacks or
playable title/menu/gameplay were observed. Marathon was not replayed and its
last TextFont/no-current-port boundary remains unchanged.


## Reservation-aware indexed matching (2026-10-16)

ReserveEntry (68K AA3E / PPC import) now shares the same live device-entry
service as protection. Reservation preserves the high byte, copies gdID's low
byte into the entry, changes ctSeed, rejects duplicate reservation with -154,
and permits any client to clear reservation while preserving protection/client
metadata. Bit 14 is the rebuild's private reservation marker; no ROM-private
byte-layout parity is claimed.

Pixel validity and color-match eligibility are separate. Reserved physical
indices remain usable for reading, packing and display; owner clients can match
them, while other clients' RGB selection and GPU nearest-color operations skip
them. Generated inverse tables omit reserved entries for all clients. With no
eligible inverse entries, MakeITable reports cMatchErr (-150) before changing
its destination. CPU nearest matching now reports no-match errors instead of
unwrapping; GPU operations requiring matching reject empty eligible sets before
submission. Presentation retains the full RGB table. Foreign-client reservation
in monochrome drawing remains explicitly unsupported; custom search procedures,
Color2Index/RealColor, SetEntries and palette animation remain incomplete.

Semantics reference: [Apple's mirrored Color Manager chapter](https://spinsidemacintosh.neocities.org/im202),
ReserveEntry, MakeITable and Color Table Management. The inverse matcher remains
the rebuild's bounded exact-distance policy, not proven ROM inverse/hidden-color
behavior. The wgpu compute path consumes a separate eligibility mask without
removing valid pixel indices or adding a CPU destination renderer.


Reservation validation: /tmp/systemless-reserve-acceptance.txt terminal exit 0,
71 passed / 0 failed / 0 ignored: core 51, indexed destinations 9, inverse/color
manager 6, text 5. Real GPU cases include 2/4/8-bit owner-versus-foreign RGB
selection and XOR nearest matching, preexisting reserved pixels, packed guards,
all existing indexed transfer modes and text/port preservation. Inverse tests
exercise both actual CPU adapters, protection/reservation independence, copied
client byte, duplicate errors, unrestricted dereservation and atomic no-match
failure. Strict runtime/probe lint passes in
/tmp/systemless-reserve-runtime-lint.txt; final fixture lint is recorded separately.
Sequential changed-file formatting and whitespace checks pass.

Original Escape Velocity 1.0.5 now passes reservation and stops at
GetCTable(8), PC 010257AE: the standard system 8-bit color table is explicitly
unimplemented. Evidence: /tmp/systemless-ev-reserve.txt, terminal exit 1 at this
reported graphics error, final tracer build exit 0. Startup replay used the same
original resource fork (SHA-256 d739c1d5bd0d5c7efddc55f34bfadd8f66a25c819a00bbed9523c7c56e3f716b),
one million instructions, 640x480x8 graphics, and no companion volume. This
supersedes the prior ReserveEntry AA3E / 0001AD04 boundary. No gameplay, audio,
or visible title/menu acceptance follows. Marathon was not replayed.

Next executable game boundary: documented standard system color tables through
shared GetCTable, without substituting the host RGB332 display policy for the
classic Macintosh palette. Resource ownership, copies and mutations must retain
the existing GetCTable contracts. Text library integration and the full rebuild
remain ongoing requirements; the goal is not complete.

Final strict fixture lint: /tmp/systemless-reserve-test-lint.txt, terminal exit 0.


## Standard color resources and shared seeds (2026-10-16)

The shared resource provider now supplies built-in clut IDs 1/2/4/8 and grayscale
IDs 34/36/40 without a ROM/System file. Application/resource-chain overrides keep
precedence. Built-in source handles have stable resource identity; GetCTable
continues to validate and copy live source bytes into independently disposable,
ordinary handles. Edits, release, allocation failure and recovery follow the same
resource/handle path as existing cursors. This does not establish a complete
system resource catalog or enumeration behavior. Highlight-adjusted IDs, 33 and
127 remain explicitly unfinished when not resource-backed.

The 4/8-bit tables match full captured Systemless reference bytes, whose pinned
baseline, source functions and SHA-256 values are recorded in
[reference provenance](../tests/reference/standard-cluts.md). These are legacy
comparisons authorized by the user, not new native System 7/ROM conformance.
Apple's [default-table documentation](https://dev.os9.ca/techpubs/mac/ACI/ACI-20.html)
establishes the resource IDs and grayscale convention. The host RGB332 display
policy remains separate from these standard Macintosh source palettes.

Both CPU adapters now dispatch GetCTSeed to one shared sequence above minSeed
1023. Palette protection/reservation invalidation also uses that sequence rather
than independently incrementing each table's seed into possible collisions.
The sequence advances above the previous table seed, preserves QDError for
GetCTSeed, allocates no guest memory, and reports exhaustion before wrapping.
The semantic reference is Apple's Color Manager chapter, Color Table Management.

Final validation: /tmp/systemless-standard-color-acceptance.txt, terminal exit 0:
16 passed / 0 failed / 0 ignored (color tables 6, inverse/color services 7,
standard cursors 3). Cases include full reference bytes, both CPU adapters,
resource ownership/live edits/overrides, source-allocation failure and recovery,
unique interleaved seeds, preserved QDError and cursor-provider regressions.
Strict runtime/probe lint passes in /tmp/systemless-standard-clut-seed-lint.txt;
final fixture lint is recorded separately. Formatting/whitespace checks pass.
Earlier narrower color-table runs are preliminary, not additional acceptance.

Original Escape Velocity passes GetCTable(8) and GetCTSeed, now stopping at
SetEntries AA3F, PC 0001A07C. Evidence:
/tmp/systemless-ev-standard-clut-seed.txt, terminal exit 1 at that explicit missing
service; tracer build exit 0. Replay uses the original resource fork, one million
instructions, 640x480x8 graphics and no companion volume. Sound/timer/VBL callback
counts remain zero; title/menu/gameplay/audio acceptance is not established.
Marathon was not replayed. Next: atomic shared SetEntries updates honoring
protection/client reservations, palette seed changes and live wgpu presentation.
The full rebuild, including complete text rendering, remains incomplete.

Final strict fixture lint: /tmp/systemless-standard-color-test-lint.txt, terminal exit 0.


## Atomic SetEntries and live presentation (2026-10-16)

68K AA3F and the PPC SetEntries import now use one shared Color Manager
transaction. Sequential mode ignores input ColorSpec.value fields; index mode
uses them to select entries. Counts are zero-based. Inputs are snapshotted before
device lookup/writes, including input overlapping the destination table. Every
requested entry is validated before a single table write: protected entries,
foreign reservations and invalid target indices reject the entire batch with
cProtectErr. Owner updates retain reservation metadata and record the current
client byte. A successful transaction allocates a new shared seed; rejected
updates preserve the palette and seed. No guest allocation or pixel writes are
needed. Unsupported device types/depths and malformed tables remain explicit.

Live surface reconstruction and the existing wgpu presentation path pick up the
new palette. A new real-GPU main-display test uses native sequential and PPC
indexed updates on a 7x3 8-bit screen, refreshes presentation, verifies exact
red/green then yellow/blue pixels, and checks guest bytes, padding and read-only
input preservation. This is offscreen presentation acceptance, not a native
window/browser check. Existing indexed tests continue to cover 2/4/8-bit drawing,
reservation-aware matching, transfer modes, clipping and guest-memory coherence.

Validation: 10 indexed-destination tests (real GPU enabled) pass in
/tmp/systemless-setentries-acceptance.txt, terminal exit 0. The final strengthened
atomic fixture run has 8 color/inverse tests passing in
/tmp/systemless-setentries-atomic-final.txt, exit 0. These are 18 distinct tests,
not 26: the first log also contains an overlapping inverse suite. Atomic cases
run both CPU adapters, change RGB requests before rejection, exercise reserved
owner/foreign updates and an aliased index-mode swap, and preserve error globals,
stack padding and guest allocation state. Strict runtime/probe lint passes in
/tmp/systemless-setentries-runtime-lint-final.txt. Final fixture lint is recorded
separately. Sequential formatting/whitespace checks pass.

Original Escape Velocity 1.0.5 now passes its 256-entry SetEntries update and
stops at Pack12 A82E, selector 6, PC 0001A5B4. Evidence:
/tmp/systemless-ev-setentries.txt, terminal exit 1 at the explicit unsupported
service; tracer build exit 0. The same original resource fork ran with a one
million instruction bound, 640x480x8 graphics and no companion volume. No
sound/timer/VBL callback or playable title/menu/gameplay acceptance was observed.
Marathon was not replayed. Next: verify/implement the Color Picker conversion
selector through shared services, preserving fractional formats and both ABIs.
Full rebuild and text-library integration remain incomplete.

Final strict fixture lint: /tmp/systemless-setentries-test-lint.txt, terminal exit 0.


## Shared RGB2HSL conversion (2026-10-16)

Pack12 A82E selector 6 and the PPC RGB2HSL import now marshal source/destination
pointers to one shared color-record conversion. The service snapshots all three
unsigned sixteen-bit RGB components before validating/writing the six-byte HSL
record, so in-place and partially overlapping records work. Native frames pop
the selector plus two pointers (10 bytes); record preflight precedes stack
changes. PPC uses R3/R4 and a void reply. Color Picker conversions preserve
QDError, resource/memory errors, palette seeds and guest allocation state.

The integer algorithm uses bounded rational intermediates and nearest rounding,
with halves upward. Saturation/lightness retain sixteen-bit component precision;
red/green/blue hue anchors are 0/21845/43690. This follows the documented model and
the legacy implementation's rounding policy; it is not a new native last-bit
parity or exhaustive legacy-corpus claim. Other Pack12 selectors remain explicit
unsupported operations. No destination-pixel renderer or dependency was added.

Primary reference: [Apple's mirrored Color Picker chapter](https://spinsidemacintosh.neocities.org/im202),
Conversion Facilities / package summary, specifies selector 6, HSL field order,
unsigned SmallFract components, and the hue anchors. The published
[HSL structure](https://dev.os9.ca/techpubs/mac/ACIReference/ACIReference-85.html)
also distinguishes lightness from HSV brightness.

Validation: /tmp/systemless-rgb-hsl-tests.txt, terminal exit 0: three tests pass,
covering both actual CPU adapters, primary/complementary colors, grayscale,
near-gray/near-white and low-value rounding, in-place/partial overlap, unchanged
surrounding bytes, preserved QDError/error state/allocation, and invalid/readonly
record preflight with unchanged native stack. Strict runtime/probe lint passes
in /tmp/systemless-rgb-hsl-lint.txt; fixture lint is recorded separately.
Formatting and whitespace checks pass.

Original Escape Velocity now passes RGB2HSL and stops at FixRatio A869,
PC 0001A5EA: /tmp/systemless-ev-rgb-hsl.txt, terminal exit 1 at the explicit missing
service; tracer build exit 0. Same original resource fork, one million instruction
bound, 640x480x8 graphics, no companion volume. Sound/timer/VBL callbacks remain
zero; title/menu/gameplay/audio acceptance is not established. Marathon was not
replayed. Next: verify signed integer-to-Fixed ratio semantics and overflow/error
behavior, implement the shared arithmetic service and both ABI adapters, then
replay. Full rebuild and text-library integration remain incomplete.

Final strict fixture lint: /tmp/systemless-rgb-hsl-test-lint.txt, terminal exit 0.


## Shared modern FixRatio (2026-10-16)

68K A869 and the PPC FixRatio import now decode signed sixteen-bit numerator
and denominator into one stateless integer service. A widened calculation
produces Fixed 16.16, rounds to nearest, saturates signed overflow and returns
signed endpoints for division by zero (including positive saturation for 0/0).
This deliberately follows the later Macintosh rules rather than the legacy
Systemless truncation/unconditional-positive-zero-divisor path or 64K ROM quirks.
The native adapter preflights two word arguments and the full four-byte result
before popping four bytes; PPC ignores argument-register upper halves and
returns the full signed result. A869 has a readable installed trap veneer. The
bounded, nonallocating service is permitted in the existing VBL-safe whitelist;
nested VBL invocation is not a new acceptance result here.

Primary reference: [Apple Mathematical and Logical Utilities](https://dev.os9.ca/techpubs/mac/pdf/Operating_System_Utilities/MLU.pdf),
pp. 3-12 and 3-46–47. The old Toolbox chapter's truncated and negative-zero-divisor
80000001 description belongs to earlier behavior; the later boundary rules are
used for the rebuild's System 7/PPC target. No native System 7 comparison was run.

Validation: /tmp/systemless-fixratio-acceptance.txt terminal exit 0 for all three
suites: 7 passed / 0 failed / 0 ignored (fixed ratio 3, logical 2, words 2).
The nearest-result invariant covers every signed-word numerator for 17 selected
nonzero denominators: 1,114,112 cases, with the positive saturation edge tested
explicitly. Both real CPU adapters cover signed/garbage-upper-word inputs,
negative/zero/overflow cases, stack/return guards, unchanged allocation/error
state and installed veneer bytes. Invalid argument/result frames preserve SP and
memory. This is not an exhaustive four-billion-pair or native rounding oracle.
Strict runtime/probe lint passes in /tmp/systemless-fixratio-runtime-lint.txt;
strict fixture lint passes in /tmp/systemless-fixratio-test-lint.txt, terminal
exit 0. Sequential formatting and whitespace checks pass.

Original Escape Velocity now passes FixRatio(-1,-32768), producing Fixed value
2, and stops at Long2Fix A83F, PC 0001A5F8. Evidence:
/tmp/systemless-ev-fixratio.txt, terminal exit 1 at the explicit unsupported
service; tracer build exit 0. Same original resource fork, one million instruction
bound, 640x480x8 graphics, no companion volume. Sound/timer/VBL callbacks remain
zero; playable title/menu/gameplay/audio acceptance remains open. Marathon was
not replayed. Next: shared signed LongInt-to-Fixed conversion with documented
saturation and both ABI adapters, then replay. Full rebuild and complete text
rendering remain active requirements.


## Shared LongInt-to-Fixed conversion (2026-10-16)

Long2Fix now uses one stateless signed 32-bit-to-Fixed conversion from both the
68K A83F trap and PPC import. The calculation widens before scaling and clamps
to signed endpoints, preserving the complete -32768..32767 exact integer range
without wrapping values outside it. Native Pascal frames retain their argument,
pop four bytes and write a full four-byte result only after preflight. PPC retains
all input bits rather than accidentally narrowing to a signed word. The native
trap has a readable installed veneer; the bounded nonallocating call joins the
existing interrupt-safe scalar whitelist. Nested interrupt invocation was not
newly exercised here.

Primary reference: [Apple Mathematical and Logical Utilities](https://dev.os9.ca/techpubs/mac/pdf/Operating_System_Utilities/MLU.pdf),
Long2Fix, p. 3-43. No native System 7 conformance run was performed.

Validation: /tmp/systemless-longfixed-acceptance.txt reports 9 passed, no failures
or skips: conversion 2, fixed ratio 3, logical 2, word extraction 2. The new
conversion test checks all 65,536 exactly representable integer inputs and
explicit 32-bit extremes/overflow boundaries. Real CPU fixtures verify full-width
signed values, native argument/result guards, PPC return/unrelated registers,
unchanged allocation/error state and veneer bytes. Frame-fault preflight now
covers both Long2Fix and FixRatio. Strict runtime/probe lint passes in
/tmp/systemless-longfixed-runtime-lint.txt; strict fixture lint passes in
/tmp/systemless-longfixed-test-lint.txt, terminal exit 0. Sequential formatting
and whitespace checks pass.


Final-source original Escape Velocity replay passes Long2Fix(65535), returning
7FFFFFFF, and stops at FixMul A868, PC 0001A604. Evidence:
/tmp/systemless-ev-longfixed.txt, terminal exit 1 at the explicit unsupported
service; tracer build exit 0. Same original resource fork, one million instruction
bound, 640x480x8 graphics, no companion volume. Sound/timer/VBL callbacks remain
zero; playable title/menu/gameplay/audio acceptance remains open. Marathon was
not replayed. Next: shared Fixed multiplication with documented rounding and
saturation, both ABI adapters and a bounded replay. Full rebuild and complete
text rendering remain active requirements.


## Shared Fixed multiplication (2026-10-16)

FixMul now routes native A868 and the PPC import through one stateless signed
16.16 multiplication service. A widened product rounds to nearest, with half
magnitudes away from zero, and saturates at signed endpoints. The native adapter
preflights both long arguments and the full result before popping eight bytes;
PPC preserves full-width operands. The installed trap veneer is readable and the
bounded call joins the existing scalar interrupt whitelist (nested interrupt
invocation was not newly tested).

Primary reference: [Apple Mathematical and Logical Utilities](https://dev.os9.ca/techpubs/mac/pdf/Operating_System_Utilities/MLU.pdf),
pp. 3-12 and 3-38–39. No native System 7 conformance run was performed.

Validation: /tmp/systemless-fixmul-tests.txt, terminal exit 0: 8 passed, no
failures or skips (multiplication 3, conversion 2, ratio 3). Explicit cases cover
signed half rounding, full-width operands and extreme overflow. The independent
nearest-result error bound covers every signed-word left operand against nine
selected right operands (589,824 nonsaturating pairs); this is not exhaustive
32-bit-pair coverage. Actual 68K/PPC execution verifies results, stack guards,
unchanged registers, allocation/error state and veneer bytes. Short arguments,
read-only results and truncated results preserve SP and memory on fault.
Strict library/fixture lint passes in /tmp/systemless-fixmul-lint.txt; tracer
build passes in /tmp/systemless-fixmul-build.txt. Sequential formatting and
whitespace checks pass. No fresh full-suite, native or browser acceptance.

Original EV now executes repeated FixMul calls, including (2, 7FFFFFFF), and
advances to unsupported FP68K selector 000D at PC 0001F3C8. Evidence:
/tmp/systemless-ev-fixmul.txt, expected explicit unsupported-selector exit 1.
The original resource fork, one-million instruction limit, 640x480x8 graphics
and no-companion-volume setup are unchanged. Sound/timer/VBL callbacks remain
zero. Neither priority game is playable; Marathon was not replayed. Next:
identify selector 000D from primary SANE documentation and implement its shared
numeric operation with ABI and boundary tests, then replay EV. The full rebuild
and complete wgpu text requirement remain active.


## Shared nonarithmetic SANE sign operations (2026-10-16)

FP68K selectors 000D (FNEGX) and 000F (FABSX) now invoke one shared numeric
service. They modify only the sign bit of the ten-byte guest operand, preserving
NaN payloads and all exception flags. Full operand read/write preflight precedes
native stack mutation; the one-pointer frame pops six bytes. No host floating
conversion is involved. No new PPC numeric import or 96-bit ABI is claimed.

Primary: Apple Numerics Manual, second edition, ch. 20 p. 150 and appendix E
p. 270 ([scanned manual transcription](https://manuals.plus/m/1f06cc272b1e59ae58f6379edca5b7829815540f233f0679c0f5f26cb9dbcbde)).
The manual specifies exception-free sign manipulation even for signaling NaNs.

Validation: /tmp/systemless-sign-tests.txt, terminal exit 0: 26 tests pass,
none fail or skip (new sign 3, existing numerics 23). Sixty selected raw extended
patterns cover both signs, zeros, normals/subnormals, infinity, quiet/signaling
NaNs and noncanonical payloads. Shared tests verify sign laws, preserved payload
bits, exception flags and adjacent guards; real 68K tests verify registers,
unchanged frame bytes, six-byte pop and allocation state. Read-only and truncated
operands fail without stack/memory/flag mutation. Strict library/fixture lint
passes in /tmp/systemless-sign-lint.txt; tracer build passes in
/tmp/systemless-sign-build.txt. Sequential formatting and whitespace checks pass.
These are focused checks, not a new full-suite, native or browser acceptance.

Original Escape Velocity passes negation and next faults in NumericCompare at
PC 0001F3D6 with `unsupported unnormal SANE operand`. Evidence:
/tmp/systemless-ev-sign.txt, terminal exit 1. The observed negative input has
exponent FFFF and zero significand; the positive form has exponent 7FFF and zero
significand. Apple Numerics Manual appendix E table E-5 permits either integer
bit for infinity, so the current strict integer-bit check needs a shared format
correction. Next: accept both documented extended infinity encodings throughout
numeric consumers, verify equivalent arithmetic/comparison/classification and
conversion behavior, then replay EV. Same original fork, one-million instruction
budget, 640x480x8, no companion volume; callbacks remain zero. No gameplay/audio
acceptance; Marathon not rerun. Full rebuild and complete wgpu text remain open.


## Documented extended special encodings (2026-10-16)

All currently implemented extended numeric consumers now canonicalize their local
infinity/NaN operand integer bit before interpretation. Apple Numerics Manual,
appendix E table E-5 permits either integer bit when the exponent is 7FFF;
previous normal-form checks incorrectly rejected the zero-significand infinity
constants used by original EV. Normalizing local operands also ensures the two
infinity representations compare equally and are never mistaken for zero.
Arithmetic, comparison, rounding/truncation, square root, elementary functions,
classification, copy and double conversion share this interpretation. Integer
conversion still explicitly rejects exceptional results. Sign-only operations
retain raw payload bits. This does not extend finite unnormal-format support or
add PPC numeric imports/96-bit ABI support.

Primary: [Apple Numerics Manual transcription](https://manuals.plus/m/1f06cc272b1e59ae58f6379edca5b7829815540f233f0679c0f5f26cb9dbcbde),
appendix E table E-5, p. 266. This is documented-format evidence, not native
System 7 conformance.

Validation: /tmp/systemless-special-tests.txt terminal exit 0: 79 tests pass,
none failed/skipped (library 53, numeric sign 3, numeric integration 23).
New tests exercise both integer bits, both signs, infinity and multiple quiet/
signaling NaN payloads against zero, finite values and each other, in both
operand orders. Results and exception state match across arithmetic, comparison
and unary consumers. Explicit expected infinity ordering/equality/double bits,
classification, copy payload/quieting and unchanged sources are checked. Existing
native ABI and raw sign tests pass. Strict library/fixture lint succeeds in
/tmp/systemless-special-lint.txt; tracer build succeeds in
/tmp/systemless-special-build.txt. Sequential formatting/whitespace pass. This
is not a fresh all-integration, native or browser acceptance run.

Original EV passes infinity comparisons and reaches further trigonometric calls
within the original one-million-instruction bound, then exhausts that budget
(/tmp/systemless-ev-special.txt, exit 1). A separate bounded ten-million run
reaches folder setup and explicitly stops at AliasDispatch A823, PC 0002FB7A,
D0=0000000C (/tmp/systemless-ev-special-10m.txt, exit 1). Sound/timer/VBL
callbacks remain zero. Original fork, 640x480x8 graphics and no companion volume
are unchanged. No title/menu/gameplay/audio acceptance; Marathon not replayed.
Next: identify the AliasDispatch selector/stack contract from primary file-system
references, implement the shared file service and both relevant adapters, then
replay with the appropriate application-volume fixture. Full rebuild and complete
wgpu text rendering remain open.


## Catalog-backed ResolveAliasFile (2026-10-16)

AliasDispatch A823 selector 000C and the PPC ResolveAliasFile import now reach
one shared Files service. It looks up the live volume/directory/name catalog,
leaves ordinary-target FSSpec bytes unchanged, and writes exact one-byte folder
and aliased Booleans. Existing regular files, directories and the volume root
are distinguished; missing volumes, parents and targets return catalog errors
instead of success. Full FSSpec/output preflight precedes native stack mutation;
Pascal arguments pop 14 bytes and leave a two-byte OSErr result. Both adapters
use the same directory/working-reference model and filesystem state.

Actual Finder alias files (kIsAlias flag) stop explicitly without changes; alias
record decoding, target search, chains, mounts and UI are not implemented. This
is the documented ordinary-target slice, not full Alias Manager compatibility.
Primary: [ResolveAliasFile, Inside Macintosh Toolbox Essentials](https://dev.os9.ca/techpubs/mac/Toolbox/Toolbox-470.html),
Finder Interface p. 7-49. No native System 7 conformance run.

Validation: /tmp/systemless-alias-tests.txt terminal exit 0: 62 pass, no failures
or skips (library 54, alias adapters 2, working directories 6). New tests cover
nested ordinary targets, removal, real alias rejection, volume root, missing
volume/parent/target, both Boolean chain arguments and PPC upper-byte garbage,
full native/PPC result conventions, spec/stack guards, unrelated registers,
allocation/error state and installed veneer. Read-only/truncated mappings fail
without native SP or output mutation. Strict library/fixture lint passes in
/tmp/systemless-alias-lint.txt; tracer build passes in
/tmp/systemless-alias-build.txt; sequential formatting/whitespace pass. No new
full-integration, native, GPU-pixel or browser acceptance.

Both ten-million-instruction EV replays pass ResolveAliasFile and reach
SecondsToDate A9C6 at PC 0002C5C0:
/tmp/systemless-ev-alias-no-assets.txt and /tmp/systemless-ev-alias-assets.txt,
both expected unsupported-service exit 1. Sound/timer/VBL callbacks remain zero.
The second mounts a disposable diagnostic volume at
/tmp/systemless-ev-assets-alias.slv (7,238,921 bytes; initial SHA-256
12fa5917d2a7c22dd21bdf5a5f8068800ca99d44a42ee95c0ac3c3ea9e0cc2f6).
Directories Pilots and EV Plug-Ins are nodes 17/18; original EV Data, Graphics,
Sounds, Titles and EV Tuner are nodes 19–23. Data forks are empty; resource
lengths are 249132/2320085/902115/3766861/400, copied verbatim from AppleDouble
entry 2; Finder metadata comes from entry 9. Application forks remain loader-owned.
The assets do not change this stopping point, and this does not establish that
all companion resources were consumed. Private originals/images remain outside
Git. Original resource fork and 640x480x8 graphics are unchanged. Neither game
is playable; Marathon not replayed. Next: shared Gregorian SecondsToDate with
both ABIs, epoch/leap-year/range validation, then asset-backed EV replay. The
full rebuild and complete wgpu text requirement remain active.


## Shared Gregorian SecondsToDate (2026-10-16)

Native A9C6 (D0 seconds, A0 record) and PPC SecondsToDate/Secs2Date imports now
use one shared calendar conversion. All unsigned 32-bit timestamps are supported:
1904-01-01 00:00:00 through 2040-02-06 06:28:15. Gregorian leap rules include
the year 2000; weekday is Sunday=1. The service publishes exactly seven
big-endian words, preserves register/stack inputs and has no clock, timezone,
allocation or error-global side effects. Nil, read-only and truncated records
fail explicitly before writes. The bounded nonallocating operation joins the
existing interrupt-safe scalar whitelist; nested interrupt invocation was not
newly exercised. DateToSeconds and long-calendar/localization APIs remain open.

Primary: [SecondsToDate](https://dev.os9.ca/techpubs/mac/OSUtilities/OSUtilities-113.html)
and [DateTimeRec](https://dev.os9.ca/techpubs/mac/OSUtilities/OSUtilities-98.html),
Inside Macintosh: Operating System Utilities. No native System 7 conformance run.

Validation: /tmp/systemless-date-tests-final.txt terminal exit 0: 8 passed,
none failed/skipped (conversion 4, existing calendar 4). Fourteen independent
Python datetime anchors cover epoch, first leap day, Unix epoch, signed 32-bit
boundary, 2000 leap day, the original EV timestamp and unsigned maximum.
Day starts/ends across all 49,711 representable days (99,422 selected timestamps)
are checked against a separate closed-form Gregorian ordinal and time/weekday
invariants; this is not exhaustive testing of all 2^32 seconds. Actual 68K/PPC
execution checks exact bytes/guards, both PPC names, native registers and stack,
PPC void returns, unchanged clock/allocation/error state and installed veneer.
Strict lint passes in /tmp/systemless-date-lint-final.txt. Initial lint required
replacing manual divisibility expressions; final tests and tracer were rebuilt
from that corrected source. /tmp/systemless-date-build-final.txt exits 0;
sequential formatting and whitespace pass. No fresh full-suite/native/browser
or GPU-pixel acceptance.

Original EV with the companion volume passes SecondsToDate(3029529619), producing
2000-01-01 00:00:19, Saturday=7, then stops at HighLevelFSDispatch AA52 selector
1 (FSMakeFSSpec), PC 0002C5DA. Evidence: /tmp/systemless-ev-date.txt, expected
unsupported-service exit 1. Same original resource fork, disposable original
asset volume, ten-million instruction bound and 640x480x8 graphics; completed
sound/timer/VBL callbacks remain zero. No title/menu/gameplay/audio acceptance;
Marathon not replayed. Next: shared FSMakeFSSpec catalog/path semantics with
missing-leaf hints and both adapters, then asset-backed EV replay. Full rebuild
and complete wgpu text rendering remain active.


## Shared FSMakeFSSpec catalog/path construction (2026-10-16)

HighLevelFSDispatch AA52 selector 1 and the PPC FSMakeFSSpec import now share
one Files implementation. Existing file/directory targets return canonical
catalog names and parent IDs with the real volume reference. Empty names identify
the specified directory (including a canonical volume-root record), full paths
override supplied references, and partial paths follow the current/working
directory with explicit-directory overrides. A missing final leaf returns -43
with a valid creation hint; other catalog errors clear all 70 output bytes.
This reuses the current single-volume HFS path model, 31-byte leaf limit and
ASCII-only case folding; broader HFS collation and multiple volumes remain open.
Native frames preflight input name, complete output and stack result before
popping 14 bytes; PPC narrows only the volume reference to its signed word.

Primary: [FSMakeFSSpec, Inside Macintosh: Files](https://dev.os9.ca/techpubs/mac/Files/Files-59.html),
Introduction to File Management p. 1-54. Directory canonicalization is also
consistent with Apple's parent-walking example in [Supporting Window Proxy Icons](https://dev.os9.ca/techpubs/new/ProgWMacOS8.5WindowMgr/WindowMgr.13.html).
No native System 7 conformance run.

Validation: /tmp/systemless-fsspec-tests-final.txt terminal exit 0: 65 pass,
none failed/skipped (library 55, file-spec adapters 2, alias adapters 2, working
directories 6). New shared cases cover nested/case-insensitive paths, parent
traversal, working-directory overrides, full-path reference override, root and
empty-name directory identity, long/bad names, missing parents/volumes and
missing-leaf hints. A returned hint creates a file that then resolves as existing.
Both real CPUs verify complete records, signed/garbage-upper-word volume inputs,
result/stack guards, preserved unrelated registers, allocation/error state and
installed AA52 veneer. Frame/name/destination faults preserve SP and output.
Initial fixture compilation needed an explicit u32 directory-ID annotation; the
corrected fixtures pass. Strict library/fixture lint passes in
/tmp/systemless-fsspec-lint.txt; tracer build passes in
/tmp/systemless-fsspec-build.txt. Sequential formatting/whitespace pass. No fresh
full-integration, native, GPU-pixel or browser acceptance.

Asset-backed original EV executes FSMakeFSSpec twice, returning a missing-file
hint for Escape Velocity Prefs, and stops at AA52 selector 000E
(FSpCreateResFile), PC 0002C8DC. Evidence: /tmp/systemless-ev-fsspec.txt,
expected explicit unsupported-selector exit 1. The fault stack contains volume
FFFF, parent ID 2 and the original 21-byte preferences name. Same original
resource fork, disposable original companion volume, ten-million instruction
bound and 640x480x8 graphics. Completed sound/timer/VBL callbacks remain zero;
no title/menu/gameplay/audio acceptance. Marathon not replayed. Next: implement
FSpCreateResFile using shared catalog/resource-fork ownership, Finder metadata,
ResError and both ABI adapters, then replay EV. Full rebuild and complete wgpu
text rendering remain active requirements.


## Shared FSpCreateResFile (2026-10-16)

AA52 selector 000E and PPC FSpCreateResFile now create an actual encoded empty
resource map through one shared Files service. New files retain zero-length data
forks and receive current creation/modification dates plus raw four-byte creator
and type metadata. Existing zero-resource files retain their data, node identity,
creation date and other Finder bytes while receiving the resource map and new
type/creator. Existing nonempty resource forks and directories are not replaced.
The operation does not open a fork or change current resource-file state.
Errors publish shared ResError and the low-memory word; void native/PPC results
preserve registers and native Pascal parameters pop 14 bytes.

This stage supports Roman (0) and system-Roman (-1) filename scripts; other
scripts stop explicitly before file changes. Extended Finder/script metadata,
other script systems, multiple volumes and full HFS collation remain open.
Primary: [Creating and Opening a Resource Fork](https://dev.os9.ca/techpubs/mac/MoreToolbox/MoreToolbox-20.html),
Inside Macintosh: More Macintosh Toolbox. No native System 7 conformance run.

Validation: 65 distinct tests pass, none failed/skipped: library 57 and existing
resource files 6 in /tmp/systemless-create-spec-tests.txt (terminal exit 0), plus
both final adapter tests in /tmp/systemless-create-spec-adapters-final.txt
(terminal exit 0). The first log also ran an earlier copy of the successful ABI
test; that duplicate is not counted again. Tests parse actual resource maps,
check empty data, metadata and current dates, preserve existing data/identity/
Finder fields, verify unchanged duplicate/error images, and save/restore the
volume with both forks and metadata intact. Real CPU cases verify void results,
PPC script narrowing, input/stack guards, unchanged MemError/current resource
file and mirrored ResError. Native short frames/specs, nil spec and read-only
ResError fail before SP/global mutation. The unsupported-script slice is checked
for no catalog changes. Strict final library/fixture lint passes in
/tmp/systemless-create-spec-lint-final.txt; tracer build succeeds in
/tmp/systemless-create-spec-build.txt. Sequential formatting/whitespace pass.
No fresh full-suite, native, GPU-pixel or browser acceptance.

Original EV passes FSpCreateResFile with creator 4D917263, type 4D70A8C4 and
script 0, then stops at AA52 selector 000D (FSpOpenResFile), PC 0002C8EA.
Evidence: /tmp/systemless-ev-create-spec.txt, expected unsupported-selector exit
1. Same original resource fork, companion volume, ten-million instruction bound
and 640x480x8 graphics; sound/timer/VBL callbacks remain zero. The disposable
volume image remains unchanged on this faulting replay (same initial SHA-256
12fa5917d2a7c22dd21bdf5a5f8068800ca99d44a42ee95c0ac3c3ea9e0cc2f6);
game preference persistence is not yet verified. No title/menu/gameplay/audio
acceptance; Marathon not replayed. Next: FSpOpenResFile through shared resource
map ownership, access permissions and search-chain state, then repeat EV startup.
Full rebuild and complete wgpu text rendering remain active requirements.


## Shared FSpOpenResFile and resource reopen semantics (2026-10-16)

AA52 selector 000D and PPC FSpOpenResFile now resolve the FSSpec's volume,
parent and leaf through the shared Files catalog and enter the same resource
opening routine as the existing filename API. Map parsing, quota checks, access
paths, preload rollback, map ownership, search order, current-file state and
resource-handle identity are shared. Results return a signed reference/-1 and
publish ResError; current resource-file publication remains shared. Native
preflight validates the six-byte argument frame, full readable FSSpec, result
word and error/current-file globals before popping arguments.

The shared routine now distinguishes independent read-only reopens from reuse
of the process's existing writable reference. Reopens preserve current-file
selection. The legacy filename-route reuse fixture now requests fsCurPerm,
matching that behavior; a separate new case explicitly verifies a distinct
read-only reference and handle for the same on-disk resource. No separate
architecture-specific resource manager or map state was added.

Primary: [FSpOpenResFile](https://dev.os9.ca/techpubs/mac/MoreToolbox/MoreToolbox-41.html),
Inside Macintosh: More Macintosh Toolbox, including separate read maps and
unchanged selection on reopening. Selector 000D is corroborated by the [File
Manager summary](https://dev.os9.ca/techpubs/mac/Files/Files-301A.html) and original
trap; the older resource-reference page's selector table contains a typo.
No native System 7 conformance run. Guest-visible resource-map exposure, broader
writeback, multiple processes/volumes and other existing limits remain open.

Validation: /tmp/systemless-open-spec-tests.txt terminal exit 0: 72 tests pass,
none failed/skipped (library 57, creation 2, lifetime 7, resource maps 6).
Actual 68K/PPC tests verify signed-byte permission marshalling, native result/
argument guards, unrelated registers, mirrored ResError/current file, shared
selection, writable reference reuse, distinct read maps/handles, payloads,
missing targets, invalid permissions and close lifecycle. Native short frames,
short FSSpecs and read-only globals preserve SP/global bytes. Existing malformed
map, preload rollback and handle-lifetime tests exercise the common opening
routine. Strict library/fixture lint passes in /tmp/systemless-open-spec-lint.txt;
tracer build passes in /tmp/systemless-open-spec-build.txt; sequential formatting
and whitespace pass. No fresh full-suite/native/GPU-pixel/browser acceptance.

Original asset-backed EV opens its preferences resource file with permission 3,
allocates a 98-byte preferences handle and selects resource file 2, then stops at
AddResource A9AB, PC 0002CA80. Evidence: /tmp/systemless-ev-open-spec.txt,
expected unsupported-service exit 1. Original fork, companion volume, ten-million
instruction bound and 640x480x8 graphics are unchanged; sound/timer/VBL callbacks
remain zero. No game preference save/reload, title/menu/gameplay/audio acceptance;
Marathon not replayed. Next: AddResource with handle ownership, duplicate IDs,
changed-resource state, permissions and both adapters, followed by real writeback
and EV replay. Full rebuild and complete wgpu text rendering remain active.


## Shared AddResource attachment and pending-growth reservation (2026-10-16)

A9AB and PPC AddResource now attach an existing owned handle to the current
writable resource map without copying or allocating a replacement guest handle.
The entry has resChanged=2 and other resource attributes zero; the map gains
mapChanged=32, and the original handle gains resource identity while retaining
its memory lock/state. Names and four-byte types preserve raw Macintosh bytes,
IDs are signed words, and zero-length data is supported. Nil/unknown/already
attached handles return addResFailed (-194); read-only maps reject additions.
GetResource/HomeResFile and existing changed-resource release/detach/close rules
share this same ownership. Disk bytes remain unchanged until writeback.

A candidate map is validated/encoded before publication. Initial pending fork
growth reserves space in the shared file quota; ordinary fork resizing and both
resource-file creation APIs now account for that reservation. Failed quota checks
leave handle ownership, flags and map entries unchanged. Reservations are runtime
state, not serialized disk content. Live payload edits/resizing and actual resource
writeback remain the next slice; this is not complete persistence support.

Primary: Apple More Macintosh Toolbox (1993), AddResource pp. 1-90–91
([manual scan](https://vintageapple.org/inside_r/pdf/More_Macintosh_TB_1993.pdf));
[Resource Manager reference](https://leopard-adc.pepas.com/documentation/Carbon/Reference/Resource_Manager/Reference/reference.html),
AddResource and mapChanged. Older Apple documentation says AddResource does not
check duplicate type/ID pairs. The current unique-key map cannot represent those;
such collisions stop explicitly without overwriting rather than inventing a native
error result. Duplicate-key map support remains open, as do unowned/purged handle
forms, guest map exposure and application/System-file writes. No native System 7
conformance run.

Validation: 81 distinct tests pass, no failures/skips. Initial
/tmp/systemless-add-resource-tests.txt terminal exit 0 covers library 57,
attachment 2 and resource-file/lifetime/map regressions 6/7/6. Final-source
/tmp/systemless-add-resource-final-tests.txt terminal exit 0 covers library 58
and attachment/fault/quota tests 4. Repeated tests are counted once. Actual CPU
cases verify original-handle lookup, raw named metadata, negative IDs/PPC upper
bits, void ABI, frame/register guards, mirrored ResError and unchanged guest
allocation/MemError. Changed entries remain resident and resist detach/unsafe
close. Tests cover zero-length/nil/attached handles, explicit duplicate rejection,
read-only maps, bad native frames/name/global mappings, disk-full nonattachment,
and reservations blocking other file growth/creation. Strict final library/fixture
lint succeeds in /tmp/systemless-add-resource-final-lint.txt; tracer build succeeds
in /tmp/systemless-add-resource-build.txt. Sequential formatting/whitespace pass.
No fresh full-integration/native/GPU-pixel/browser acceptance.

Asset-backed original EV attaches its 98-byte preferences handle as type 4D70A8C4,
ID 128, then restores application resource selection and changes handle flags.
It stops at UpdateResFile A999, PC 0002CA9E. Evidence:
/tmp/systemless-ev-add-resource.txt, expected unsupported-service exit 1. Same
original resource fork, companion volume, ten-million instruction bound and
640x480x8 graphics; sound/timer/VBL callbacks remain zero. No preferences
save/reload, title/menu/gameplay/audio acceptance; Marathon not replayed. Next:
shared UpdateResFile using current handle payloads, atomic fork replacement,
reservation accounting/changed flags and both adapters, then EV replay and
preference save/reload verification. Full rebuild and complete wgpu text remain
active requirements.


## Shared UpdateResFile, dirty close and original preference round-trip (2026-10-16)

A999 and PPC UpdateResFile now target the supplied resource reference independently
of current search selection. Changed unprotected entries snapshot live owned
handle contents and current sizes, not AddResource-time copies. A candidate fork
is encoded and quota-checked before one catalog-fork replacement; only successful
replacement publishes clean map/resource state and releases pending-growth
reservations. Corrupt handles or quota failure retain the old fork and dirty
state for retry. Data forks, catalog identity and Finder metadata remain intact;
modification time uses the shared Macintosh clock. A retained EOF is padded when
shrinking without mapCompact; explicit mapCompact alone triggers compaction.
CloseResFile now updates dirty maps before releasing handles/access paths instead
of rejecting all changed maps. This replaces the prior intentional close gap.

Primary: [Resource Manager reference](https://leopard-adc.pepas.com/documentation/Carbon/Reference/Resource_Manager/Reference/reference.html),
UpdateResFile, CloseResFile and changed-resource rules, consistent with Apple
More Macintosh Toolbox pp. 1-92–93 and 1-110. No native System 7 conformance run.
Single-resource WriteResource/ChangedResource, broader removal/attribute APIs,
purged/unowned dirty handles, duplicate-key maps, guest map exposure and
application/System-file write support remain incomplete. Serialization compacts
supported logical maps and retains EOF as documented; raw layout parity is not
claimed.

The diagnostic tracer gains an explicit --save-volume NEW_IMAGE option. It saves
only committed Files state at guest stop/fault/budget completion, creates a new
output file exclusively and never overwrites its input or an existing output.
It does not flush dirty resource maps or bypass missing guest services.

Validation: 85 distinct tests pass, no failures/skips across focused runs.
/tmp/systemless-update-resource-tests.txt (exit 0) covers initial library 58,
attachment 4, existing resource files/lifetime/maps 6/7/6 and initial update 2.
/tmp/systemless-update-resource-final-tests.txt (exit 0) covers final library 59,
attachment 4 and update 3; /tmp/systemless-update-resource-adapters-final.txt
(exit 0) reruns the latest update cases. Repeats are excluded from 85. Final cases
verify both actual CPU void ABIs, signed reference marshalling, frames/registers,
ResError, update of a noncurrent file, live resized data, clean flags, automatic
close, exported-volume restore and actual resource reload. Corrupt-master and
full-disk cases preserve old fork/dirty state and recover on retry. A unit fixture
verifies retained EOF and explicit compaction. Strict final library/fixture/tracer
lint passes in /tmp/systemless-update-resource-final-lint.txt; tracer build passes
in /tmp/systemless-update-resource-build.txt. Sequential formatting/whitespace
pass. These are focused checks, not fresh full-suite/native/browser/pixel gates.

Original EV now updates and closes its preferences resource file, then stops at
PBFlushVol A013, PC 0001B644. /tmp/systemless-ev-update-resource.txt exits 1
at that unsupported service and saves /tmp/systemless-ev-update-saved.slv:
7,239,400 bytes, SHA-256
1e321b69965601116735786e4dbd1df07ac5cedb5a3eeb081d792ada039bb9b5.
Independent CRC/catalog/resource parsing finds node 24, parent 2, zero data fork,
a 417-byte resource fork, type 4D70A8C4, ID 128, 98-byte data and clean map/
resource flags. Payload SHA-256:
01f3125e175f85049d0030520405f6688a17c5e67171016811c337dd40af7094.

A fresh original EV replay using that saved volume opens the preferences map,
gets type 4D70A8C4 ID 128 as nonzero handle 81756, locks/uses/unlocks it and closes
the map. It then enters an existing-file rewrite path and stops at AA52 selector
6 (FSpDelete), PC 0002C906. Evidence: /tmp/systemless-ev-update-reload.txt,
exit 1. Its separate output /tmp/systemless-ev-update-reloaded.slv is byte-identical
to the first saved volume. The original companion input remains 7,238,921 bytes
with its original hash. This proves bounded preference-resource write/readback,
not UI settings acceptance or a completed rewrite/game session. Both replays use
10 million instructions and 640x480x8; sound/timer/VBL callbacks remain zero.
No title/menu/gameplay/audio acceptance; Marathon not replayed. Next: shared
FSpDelete and PBFlushVol semantics, then complete the existing-preferences rewrite
path and continue original EV startup. Full rebuild and complete wgpu text remain
active requirements.


## EV preference deletion/recreation (2026-10-16)


Original EV saved-preferences replay now passes FSpDelete, creates a replacement
preferences resource file, opens ref 3, attaches the existing live preferences
handle, updates and closes it. It stops at PBFlushVol A013, PC 0001B644, with
sound/timer/VBL callbacks zero. Evidence: /tmp/systemless-ev-delete.txt, terminal
exit 1 (expected unsupported service), following successful tracer build in
/tmp/systemless-delete-build.txt. The run uses 10 million instructions and
640x480x8. Output /tmp/systemless-ev-delete-saved.slv is 7,239,400 bytes, SHA256
5f48f2c6f79516349bc38561596539017aa1831f8041d8901ee3851ea0400dab.
The saved input hash remains
1e321b69965601116735786e4dbd1df07ac5cedb5a3eeb081d792ada039bb9b5.
Independent CRC/catalog parsing confirms old prefs node 24 replaced by node 25,
next-node 25 -> 26, empty data fork and identical 417-byte resource fork (SHA256
9b69b19ff3501c0f35f25b25d5a6d567e4c9b798957822d18a93968ee860b057).
Evidence: /tmp/systemless-delete-volume-verification.txt. This is a bounded
preferences read/delete/recreate/save cycle, not completed volume flush, settings
UI or gameplay acceptance. Strict scoped lint passes in
/tmp/systemless-delete-lint.txt; sequential formatting and diff whitespace pass.
No new native/browser/GPU-pixel or full-suite acceptance. Marathon/PPC originals
not rerun. Next: PBFlushVol with explicit host checkpoint/error semantics, then
continue EV startup. Full rebuild and complete wgpu text remain active.


## EV synchronous volume flush and later UI setup (2026-10-16)


Original EV now passes the volume flush in two distinct host policies. The
one-instruction tracer explicitly acknowledges a memory-only barrier, then reaches
OpenRgn A8DA at PC 01026B6A after palette work, CODE 4 loading, menu-bar installation,
WMgr port selection and NewRgn. Evidence: /tmp/systemless-ev-flush.txt, terminal
exit 1 (expected unsupported OpenRgn). The normal headless executable uses a real
writable Volume copy and reaches the same unsupported service A8DA; evidence
/tmp/systemless-ev-flush-host.txt, terminal exit 1. Both use 10 million instructions
and 640x480x8. No title/menu interaction, gameplay, sound or native-window acceptance
is claimed; tracer sound/timer/VBL callbacks remain zero. Marathon/PPC not rerun.

Input /tmp/systemless-ev-delete-saved.slv remains 7,239,400 bytes, SHA256
5f48f2c6f79516349bc38561596539017aa1831f8041d8901ee3851ea0400dab.
Tracer output /tmp/systemless-ev-flush-saved.slv has SHA256
a34a4f72439d271464812ac5d768454b0ce767f132282ac0ff64bf5236d8eb7d;
writable host /tmp/systemless-ev-flush-host.slv has SHA256
f956ff4c359fe18d78dc2abd210e7b4dab61279a6197e6d899e6550639ca9800.
Independent CRC/catalog parsing verifies both outputs contain preferences node 26
(replacing 25), next node 27, empty data fork and identical 417-byte resource fork,
SHA256 9b69b19ff3501c0f35f25b25d5a6d567e4c9b798957822d18a93968ee860b057.
Host versus tracer whole-volume differences include dates: the normal launcher
seeds wall time while the tracer retains its deterministic epoch. Evidence:
/tmp/systemless-flush-volume-verification.txt. Both outputs are separate from the
input; no private original/snapshot files enter Git.

Strict all-target lint passes in /tmp/systemless-flush-lint-acceptance.txt;
tracer and normal host builds pass in /tmp/systemless-flush-build.txt and
/tmp/systemless-flush-host-build.txt. Earlier lint attempts exposed an invalid
read-only test constant (corrected) and an existing Resource Manager test initializer
warning (rewritten equivalently); they are superseded by acceptance output.
No fresh complete test-suite, browser, native presentation, GPU-pixel or GitHub
CI acceptance. Next: shared OpenRgn/CloseRgn recording with exact region geometry
and QuickDraw drawing semantics, then continue original EV UI startup. Complete
wgpu text and the full rebuild remain active requirements.


## EV region recording and later visibility setup (2026-10-16)


Final acceptance: 71 distinct focused tests pass with no skipped/ignored cases in
acceptance runs: core 62 (/tmp/systemless-region-record-acceptance.txt), polygon 4
and region 5 (/tmp/systemless-region-record-final-gpu.txt). The two real-wgpu
cases pass, including hidden recording with unchanged destination pixels, exact
rectangular holes despite zero pen size, visible ShowPen frame output, and existing
polygon fill/frame/XOR regression coverage. The new cross-port test rejects nested
region/polygon recording in both directions. Core geometry also checks a tall
full-signed-range rectangle coalesces to a compact region. Earlier default runs
ignored GPU tests and are not additional GPU acceptance.

Strict all-target lint and final tracer build pass in
/tmp/systemless-region-record-final-lint.txt and
/tmp/systemless-region-record-final-build.txt. Sequential formatting and diff
whitespace pass. Earlier compile/format issues (cast parentheses and a borrowed
line-input snapshot) were corrected and superseded by final output.

The final original EV run passes OpenRgn, FrameRect and CloseRgn, combines the
result with WMgr regions, and stops at CalcVisBehind A90A, PC 01026A90. Evidence:
/tmp/systemless-ev-region-final.txt, terminal exit 1 (unsupported next service),
10 million instructions, 640x480x8. The tracer explicitly acknowledges its
memory-only volume flush. Sound/timer/VBL callbacks are zero. Native-window,
normal writable-backed host, PPC originals and Marathon were not replayed in
this milestone. No UI interaction, playable game, audio or browser acceptance.

Independent CRC/catalog verification in /tmp/systemless-region-volume-final.txt
confirms the separate output /tmp/systemless-ev-region-final-saved.slv remains
7,239,400 bytes, SHA256
ad05777204b2f60ec2c352b1f0c3a62e91567fad1311ff788b0a1b542eabf540,
with preferences node 26 -> 27, unchanged metadata/both fork bytes and every
companion entry unchanged. Input /tmp/systemless-ev-flush-saved.slv remains SHA256
a34a4f72439d271464812ac5d768454b0ce767f132282ac0ff64bf5236d8eb7d.
Initial and final output images are byte-identical. No originals enter Git.

Next: shared CalcVisBehind/CalcVis through existing Window Manager visibility
geometry, respecting the start window, clobbered region, front-to-back coverage,
port coordinates and BeginUpdate state; exercise both ABIs and actual overlapping
windows before replaying EV. The legacy helper that recalculates every tracked
window is reference evidence, not sufficient implementation of that contract.
Primary: https://dev.os9.ca/techpubs/mac/Toolbox/Toolbox-288.html. Full rebuild and
complete wgpu text rendering remain active requirements.


## EV visibility and refreshed Marathon boundary (2026-10-16)


Final strict all-target lint passes in /tmp/systemless-calcvis-final-lint.txt;
tracer build passes in /tmp/systemless-calcvis-build.txt. Sequential formatting
and whitespace checks pass. Core/selected-window runs are focused acceptance,
not a fresh full integration suite or GitHub CI acceptance.

Original EV passes WindowVisibility(Behind { start: 0, clobbered: 81784 }) with
its empty window list and reaches PaintBehind A90D, PC 01026A9C. Evidence:
/tmp/systemless-ev-calcvis.txt, terminal exit 1 (unsupported next service), using
10 million instructions, 640x480x8 and the tracer's explicit memory-only flush
policy. Sound/timer/VBL callbacks remain zero. This empty-list application call
is supplemented by real overlapping-window/both-CPU fixtures; it alone does not
prove nonempty-list visibility behavior or playable gameplay.

Independent CRC/catalog comparison in /tmp/systemless-calcvis-volume-check.txt
verifies output /tmp/systemless-ev-calcvis-saved.slv is 7,239,400 bytes, SHA256
ff1d7b960c7787f0016992809979eee85257cba2daf2488e00134c1a11d168f7,
with preferences node 27 -> 28, unchanged metadata/fork bytes and all companions
preserved. Input /tmp/systemless-ev-region-final-saved.slv remains SHA256
ad05777204b2f60ec2c352b1f0c3a62e91567fad1311ff788b0a1b542eabf540.
An initial verification script used an older baseline and correctly failed its
node-counter assertion; the generalized verifier compares the actual input above.

Marathon 1 v1.2 was replayed on the same build: TextFont family 4 still faults at
PC 01010E86 with no current QuickDraw port, after 62 timer callbacks and zero
sound/VBL callbacks. Evidence: /tmp/systemless-marathon-calcvis.txt, terminal
exit 1, 10 million instructions, 640x480x8 and the existing companion volume.
No default port was fabricated to bypass it. Neither priority game is playable.
PPC originals, native-window and writable-backed game runs were not repeated;
no new audio, browser or native-interaction acceptance is claimed.

Next: shared PaintBehind/PaintOne drawing, exact clobbered-region clipping and
PaintWhite/update semantics through wgpu, before continuing EV startup. Primary:
https://dev.os9.ca/techpubs/mac/Toolbox/Toolbox-286.html. The baseline implementation
invalidates bounding boxes and is reference evidence, not proof of the documented
PaintOne drawing contract. Keep Marathon's port initialization issue open. Full
rebuild, complete wgpu text and library integration remain active requirements.


## EV desktop repaint and offscreen color-copy boundary (2026-10-16)


Final strict all-target lint passes in /tmp/systemless-repaint-final-lint.txt;
tracer build passes in /tmp/systemless-repaint-build.txt. Scoped sequential
formatting and diff whitespace pass. This is focused core/window acceptance,
not a fresh complete integration suite or GitHub CI result.

Original EV now passes WindowRepaint with a NIL desktop target, creates a hidden
color window and several offscreen color ports, loads 8-bit blitter resources and
reaches CopyBits. It stops at PC 000182F6 with "unsupported or invalid indexed
PixMap source". The failing request uses source 219526, destination 212054,
source/destination rectangle 8453824, mode 0 and NIL mask. The source is exactly
CGrafPort 219524 + 2 (a preceding ColorPort Open), so inspect color-port bitmap
alias/handle normalization before changing indexed palette validation. Destination
is likewise current color port 212052 + 2. The current PixMap source decoder
rejects row flag forms other than actual 0x8000 indexed PixMaps; this is a next
investigation boundary, not proof that every alias field is valid.

Evidence: /tmp/systemless-ev-repaint.txt, terminal exit 1 (unsupported source
form), 10 million instructions and 640x480x8, with explicit memory-only volume
flush. Sound/timer/VBL callbacks remain zero. No title/menu interaction, gameplay,
audio, native-window or browser acceptance; Marathon/PPC originals not rerun.
The hidden window/offscreen work is startup evidence, not a displayed game frame.

Independent verification in /tmp/systemless-repaint-volume-check.txt confirms
output /tmp/systemless-ev-repaint-saved.slv is 7,239,400 bytes, SHA256
077a48e820e947cbfdb9346f105c737770dbca6288d690d47ecac0d3d597028e,
with preferences node 28 -> 29 and unchanged metadata/fork bytes and all companion
entries. Input /tmp/systemless-ev-calcvis-saved.slv remains SHA256
ff1d7b960c7787f0016992809979eee85257cba2daf2488e00134c1a11d168f7.
No original assets or snapshot files enter Git.

Next: inspect live source/destination descriptors and implement shared validated
CGrafPort/bitmap/PixMap reference normalization for CopyBits if confirmed, retaining
all strict indexed formats, bounds, palettes and coherent GPU transfer semantics.
Use both ABI fixtures and actual offscreen color-port copies, then replay EV.
Marathon's font selection without a current port remains open. Full rebuild and
complete wgpu text rendering/library integration remain active requirements.


## EV color-port bitmap reference and plug-in inspection (2026-10-16)


Final pixel-activity acceptance passes in /tmp/systemless-alias-pixels-final.txt:
the mode-0 cases demonstrably change destination pixels, all modes preserve every
pixel outside the region mask, and normalized references match direct PixMaps for
both real CPU adapters (32 mode/overlap/CPU combinations). This repeat is counted
once in the 83 distinct tests. Strict all-target lint passes in
/tmp/systemless-alias-final-lint.txt; the tracer build passes in
/tmp/systemless-alias-build.txt. A final scoped lint checks the late-added pixel
assertions. Formatting and diff whitespace pass. No fresh full integration-suite
or GitHub CI acceptance is claimed.

Original EV passes the previously failing offscreen-to-screen CopyBits after
showing its color window, then advances into plug-in file enumeration, catalog
queries and ordinary-target ResolveAliasFile calls. It stops at FSpGetFInfo,
HighLevelFSDispatch AA52 selector 7, PC 0001CF68, while inspecting EV Tuner.
Evidence: /tmp/systemless-ev-alias.txt, terminal exit 1 (unsupported next selector),
10 million instructions, 640x480x8, explicit memory-only volume flush. Its
sound/timer/VBL callbacks remain zero. This is bounded service and pixel-path
progress; no original-frame screenshot, UI interaction, gameplay, native-window,
audio or browser acceptance. Marathon/PPC originals were not rerun.

Independent CRC/catalog verification in /tmp/systemless-alias-volume-check.txt
confirms /tmp/systemless-ev-alias-saved.slv is 7,239,400 bytes, SHA256
b465c326dbf011df7512f1f4a6d0b397d1d6ce2c71f443998410afcd1416a389.
Preferences node 29 -> 30, with identical metadata and both fork bytes; every
companion entry is unchanged. Input /tmp/systemless-ev-repaint-saved.slv remains
SHA256 077a48e820e947cbfdb9346f105c737770dbca6288d690d47ecac0d3d597028e.
No private original assets or saved volumes enter Git.

Next: shared FSpGetFInfo using the live FSSpec/catalog/Finder metadata model,
including pointer/error/ABI checks and both CPU fixtures, then replay EV plug-in
inspection. Keep actual Finder alias records distinct from these color-port
bitmap references. Marathon's port initialization, actual gameplay/audio, complete
text/library integration and the full rebuild remain active and unverified.

### EV Finder metadata replay (2026-10-16)

The original 68K EV1.0.5 passes FSpGetFInfo on its plug-in catalog entry and
continues to drawing PICT128 from EV Titles. It stops at PC00010EAC with
`PICT: unsupported indexed PixMap layout`: that resource is a packed 4-bit indexed
picture, beyond the decoder's current 8-bit indexed slice. Trace:
/tmp/systemless-ev-finfo.txt, expected exit1, sound/timer/VBL callbacks0. No
interactive gameplay or original-frame acceptance is established.

The exported committed volume /tmp/systemless-ev-finfo-saved.slv is 7,239,400
bytes, SHA256 e1bc97cb6844fdf93d5bf412d2546104216d9f37fcd66bbc4a1f9d9a27f17236.
Independent CRC/catalog/fork verification against alias-saved passes: only the
preferences catalog identity increments30->31; metadata and both fork bytes,
all companions and directories remain identical. The tracer still acknowledges
PBFlushVol as a memory-only barrier, not a writable host checkpoint. Next replay
may use this snapshot; use a fresh save filename.

### EV packed picture replay (2026-10-16)

Original EV1.0.5 now passes drawing PICT128 from EV Titles and advances into
sound initialization. It stops at PC00112CB4 on unsupported SoundDispatch
selector0D000008, following SndNewChannel(synth5,init128). Trace
/tmp/systemless-ev-packed.txt exits1, sound/timer/VBL completed callbacks0.
This establishes startup service progress, without screenshot, interaction,
gameplay or audio acceptance. The tracer's flush acknowledgment remains an
explicit memory-only barrier.

Input finfo-saved is unchanged. Output /tmp/systemless-ev-packed-saved.slv is
7,239,400bytes SHA256 a947f1f336634d43d2f456379750d76e02eab03588810f9bab71c6ec97de2ec0.
Independent CRC/catalog/fork verification passes: preferences node31->32, same
metadata and fork bytes, all companion entries/directories identical. Use this
snapshot and a fresh output filename for the next replay.

### EV resource file-play request (2026-10-16)

Original EV's SoundDispatch0D000008 is SndStartFilePlay with fileRef0,
resource30000, asyncTRUE, channel00113340, bufferSize65534 and nil caller buffer,
selection and completion. The shared resource-backed service now handles this
request shape. Resource-map scans of the supplied application and companion
forks find no snd30000, so this replay follows the missing-resource result path;
it does not establish original music playback. Exact synthetic samples are
verified separately through both CPUs.

/tmp/systemless-ev-fileplay.txt advances to another DrawPicture and then stops
at unsupported trapA11D (MaxMem), PC00011356. Completed sound/timer/VBL callbacks
remain0. No gameplay/UI/audio acceptance is claimed. Snapshot
/tmp/systemless-ev-fileplay-saved.slv is7,239,400bytes,
SHA25677d11063a93cdc53e4b2dc4f4c5733241cca51f47c9908d69a97cca127b532c3.
Independent CRC/catalog/fork checks pass: preferences identity32->33, all other
metadata, forks, companions and directories unchanged. Tracer flushes remain
explicitly memory-only acknowledgments.

### EV MaxMem replay (2026-10-16)

/tmp/systemless-ev-maxmem.txt passes two MaxMem operations and continues to
allocation/picture/cleanup activity. It stops at unsupported SndPauseFilePlay,
SoundDispatch02040008, PC00112CDC (expected exit1). Original audio, native frame,
UI and gameplay remain unverified; earlier snd30000 is absent from the supplied
forks. Completed sound/timer/VBL callbacks remain0.

Output /tmp/systemless-ev-maxmem-saved.slv is7,239,400bytes,
SHA256a2ee46609579718aa78954219082c96a8f0d3f82ccfeed14c7bf1cb359cbd6d4.
Independent CRC/catalog/fork verification against fileplay-saved passes:
preferences identity33->34, all metadata/forks, companions and directories
unchanged. Input is unchanged, and tracer flush acknowledgments remain explicitly
memory-only. Use this snapshot with a fresh save filename for further replay.

### EV pause replay and declared memory partition (2026-10-16)

/tmp/systemless-ev-pause.txt passes SndPauseFilePlay after its missing music
resource and enters an insufficient-memory alert. Guest text says EV needs at
least another1500K; execution stops at unsupported ModalDialog(A991), PC001B4600,
expected exit1. This is an error path, not successful startup, gameplay or audio.
Original SIZE(-1) payload58800081b00000659000 declares preferred8,499,200 and
minimum6,656,000bytes. Current CODE A5_BASE4MiB/STACK_BASE8MiB ignores the requested
partition. Next loader work should honor actual SIZE-driven heap/A5/stack bounds;
do not fake larger free-memory results or merely raise A5 across the old stack.

Snapshot /tmp/systemless-ev-pause-saved.slv is7,239,400bytes,
SHA256d995f8ce1631d1b110e7a7e55dd40b0584fc6eb978360045374393373b59b9d2.
Independent CRC/catalog/fork checks pass: preferences identity34->35, all metadata,
forks, companions and directories unchanged. Input remains unchanged. Tracer
flushes are still explicit memory-only acknowledgments; no host checkpoint,
original frame/UI, native audio or gameplay acceptance is established.

### EV declared SIZE replay (2026-10-16)

The loader now uses EV's SIZE(-1) preferred8,499,200/minimum6,656,000bytes, moving
its actual heap/A5/stack reservations together. /tmp/systemless-ev-size.txt gets
beyond the prior insufficient-memory/ModalDialog path and stops later at
ExtendedToInteger(bits32), PC00012076: "unsupported exceptional SANE integer
conversion" (expected exit1). Numeric source/environment/ABI evidence must be
inspected before implementing that case. No original frame/UI/audio/gameplay
acceptance is established; sound/timer/VBL completed callbacks remain0.

Snapshot /tmp/systemless-ev-size-saved.slv is7,239,400bytes,
SHA256443473dfc972c08b900f877ce61a80c68103fda20bbd2bbf307cb20155ae2faf.
Independent CRC/catalog/fork checks pass against pause-saved: preferences identity
35->36, all metadata/forks, companions and directories unchanged. Input is
unchanged; tracer flushes remain memory-only acknowledgments. Use this snapshot
with a fresh output filename for the next original replay.

### EV software-SANE integer result and Marathon refresh (2026-10-16)

/tmp/systemless-ev-integer.txt passes the previously failing ExtendedToInteger
using the software-SANE most-negative result plus invalid status. Its source
came from NewPtr(clearFALSE) reusing PICT8100 storage, confirmed by raw operand
and global/allocation watches. No allocator clearing/reuse semantics were changed.
The next stop is subtracting the next10bytes of that allocation: unsupported
unnormal SANE operand, source001153CA, PC0001209C, expected exit1. Numeric format
normalization remains separate work; no original frame/UI/audio/gameplay
acceptance is established. Callback counts remain0.

Snapshot /tmp/systemless-ev-integer-saved.slv is7,239,400bytes,
SHA256cb30e2b9a0cc89a0dbd27859765ac5e1640071857aaf8b7a3e56dd06553860a3.
Independent CRC/catalog/fork verification passes: preferences36->37, identical
metadata/forks, companions and directories. Input size-saved is unchanged; tracer
flushes remain memory-only acknowledgments.

Fresh original Marathon1v1.2 replay /tmp/systemless-marathon-integer.txt still
stops at TextFont family4, PC01010E86, with no current QuickDraw port. It runs
with its declared SIZE layout and completes62timer,0sound/0VBL callbacks. Dormant
VBL record remains0001821C. This verifies the current boundary after loader/math
changes; it does not establish interactive play or justify inventing a port.


### EV: finite software SANE representation checkpoint

The documented software extended format accepts EV's finite denormal source
1ff9100649b8b82a1010. Shared exact normalization and exponent-zero boundary fixes
let the original proceed to CountResources ($A99C), PC0002A28A, which is the next
unsupported service. Replay /tmp/systemless-ev-extended.txt terminates exit1;
zero sound/timer/VBL callbacks, no gameplay/frame/UI/audio acceptance.
Input /tmp/systemless-ev-integer-saved.slv remains unchanged. Saved output
/tmp/systemless-ev-extended-saved.slv (7,239,400bytes) hashes to
8a9584ae108f9baa55d47a58aa94718e86b4dcdfa06bb925f8fc7e2986cce859.
Independent CRC/catalog/fork validation proves preferences37->38 is the only
identity change, with metadata and fork bytes retained across all files.


### EV: resource counting checkpoint

Shared CountResources ($A99C) passes both observed calls (raw types73709a62 and
73d87374); both are absent, so count zero. The original proceeds to more
shared device/color/rectangle drawing and stops at missing Delay ($A03B),
PC001125AE. /tmp/systemless-ev-count.txt terminates exit1 with zero callbacks.
This does not prove correct application enumeration or visible/gameplay/audio UI.
Saved volume /tmp/systemless-ev-count-saved.slv (7,239,400bytes),
SHA256 c641fc677f625767a163d645d11a9c25e6838bf9a911615781de467fef9c17ec,
passes independent CRC/catalog/fork validation against ev-extended-saved:
preferences38->39 only identity change, all metadata/forks and companions intact.


### EV: resumable Delay checkpoint

Original EV completes five Delay(2) calls with five advances of two virtual ticks,
then proceeds through more picture/device/color drawing. It stops at unsupported
QDExtensions $AB1D selector00040001 (LockPixels), PC0002FA34. Trace
/tmp/systemless-ev-delay.txt exits1; no callbacks, visible UI/audio/gameplay claim.
Input ev-count-saved remains intact. Output /tmp/systemless-ev-delay-saved.slv,
7,239,400bytes, SHA256
7cac08cecd3714a4d628cb3cb895ad4b594db28dfe1908ffba8ca876f9500e6e,
passes independent CRC/catalog/fork verification: preferences39->40 is the only
identity change, all metadata/fork bytes and companions remain identical.


### EV: shared pixel access checkpoint

A read-only fault inspection established an owned 80x75/depth8/version0 PixMap
with a 6000-byte NewPtr pixel image. Shared LockPixels now accepts fixed owned
images and pins movable handle images correctly; GetPixBaseAddr resolves both.
The final original passes both calls and stops during DrawPicture, PC0002F748:
unsupported bitmap stride, flags or extent. Inspect the destination before
changing validators. No visible frame/UI/audio/gameplay acceptance; callbacks0.
/tmp/systemless-ev-pixel-base.txt exits1. Input ev-pixels-saved remains intact.
Output /tmp/systemless-ev-pixel-base-saved.slv (7,239,400bytes), SHA256
da98ddbeca4a66372b1e9cf2681f182801cfddf22ae027c083b543e0beede452,
passes independent CRC/catalog/fork validation: preferences41->42 only identity
change, metadata/fork bytes and all companions identical.


### EV: inherited-bounds basic offscreen drawing

Read-only inspection shows the application retains640x480 bitmap bounds while
replacing base/stride with an owned750-byte image, row10, and portRect80x75.
This is outside the strict standalone bitmap contract. A checked basic offscreen
port-prefix compatibility path preserves standalone/screen validation and guest
metadata. The original passes that failure and later stops decoding direct-pixel
PICT$009A (DirectBitsRect) in a48x48 destination, PC0002F748. Next work is RGB PICT
decoding and GPU transfer. No callback/frame/UI/audio/gameplay claim.
/tmp/systemless-ev-offscreen.txt exits1. Saved volume
/tmp/systemless-ev-offscreen-saved.slv (7,239,400bytes), SHA256
10900d727e914c63915ad77956a45c10423c6673d29409ffce025a667622290b,
passes independent CRC/catalog/fork checks: preferences42->43 only identity
change, metadata/forks and companions identical to ev-pixel-base-saved.


### EV: direct RGB PICT source checkpoint

Direct16/32-bit PICT decoding and lossless RGB GPU source transfer are implemented
and verified with synthetic fixtures through both CPUs. The exact original EV
application fork was restored from the existing local archive after temporary
folder cleanup; its established SHA256 matches. Durable ignored fixture:
future/target/private-originals/ev/application.raw-rsrc. Restored original replay
/tmp/systemless-ev-direct-restored.txt exits1 on PICT2027's mode$40 (ditherCopy),
with valid48x48 rectangles. The direct opcode is recognized, but rendering that
original picture remains unverified until real dithering is implemented. No flag
stripping or unsupported-success substitution was used. Private assets stay ignored.
Saved /tmp/systemless-ev-direct-saved.slv (7,239,400bytes), SHA256
989a2de094265c4b9fd1239e4af57c9b5c493dd5145287bffd3c746be2b02f2e,
passes independent CRC/catalog/fork checks: preferences43->44 only identity
change, metadata/fork bytes and companions unchanged.

Marathon's matching original fork is also restored in ignored
future/target/private-originals/marathon/application.raw-rsrc. Fresh indexed
640x480x8 replay /tmp/systemless-marathon-direct-indexed.txt reaches its known
TextFont/no-current-port error, PC01010E86,62timer callbacks. Default monochrome
instead takes an error dialog and stops at missing Alert, PC010108B2,
/tmp/systemless-marathon-direct.txt. Neither run proves gameplay or visible/audio UI.


### EV: real on-device ditherCopy

The original now passes its direct-color PICT2027/ditherCopy boundary using real
GPU error diffusion, continues through additional offscreen/picture loads and
stops at missing GetPixPat ($AA0C), resource128, PC0002F256. Exact-source replay
/tmp/systemless-ev-dither-final.txt exits1, callbacks0. No visible frame/UI/audio/
gameplay acceptance follows. The diffusion algorithm is compared with preserved
Systemless GWorld behavior, not claimed native QuickDraw pattern parity.
Input ev-dither-saved remains unchanged. Final saved volume
/tmp/systemless-ev-dither-final-saved.slv (7,239,400bytes), SHA256
2ba13890c96592ee8112132a416ad7d6f83392996f3be37741e3827d9ee9b9dd,
passes independent CRC/catalog/fork checks: preferences45->46 only identity
change, metadata/fork bytes and companions identical. Continue using the ignored
future/target/private-originals/ev/application.raw-rsrc fixture. Next work is
GetPixPat resource loading and owned pixel-pattern state, with real drawing
semantics rather than an empty success stub. Both original games remain unplayable.


### EV: typed pixel-pattern loading

The original now passes GetPixPat requests128–137 and200; all11resources exist
in EV Graphics. The rebuild converts offsets into independent owned PixPat,
PixMap, pixel, color-table and cache handles rather than returning raw resource
bytes. Color-pattern installation/painting remains open. The next missing service
is GetCIcon$AA1E/id10000, PC00017352. /tmp/systemless-ev-pixpat-final.txt exits1,
callbacks0. No visible frame/UI/audio/gameplay acceptance is claimed.
Saved /tmp/systemless-ev-pixpat-final-saved.slv (7,239,400bytes), SHA256
74930f4351f5fba929ece70350ab5b2a98564a970e668627f21b0e2e70785160,
passes independent CRC/catalog/fork checks against ev-pixpat-saved: preferences
47->48 only identity change, metadata/fork bytes and companions unchanged. Use
this checkpoint with the ignored original application fork for the color-icon slice.

## Original Escape Velocity: typed color-icon loading (2026-10-16)

Implemented GetCIcon/DisposeCIcon through one manager for native and PPC callers.
The original requests cicn10000 from EV Graphics:202bytes,16x16,1-bit indexed,
with mask and monochrome images. EV first edits the cached mask baseAddr; a
read-only watch proves that write at PC00017D8A. GetCIcon now replaces resource
pointer placeholders with independent owned addresses rather than requiring zero
or following guest addresses. Seven new integration cases cover both CPU frames,
live source edits, independent graphs, source release, compaction, all indexed
depths, malformed input, allocation rollback and full disposal. Total scoped
validation:105distinct passes,4existing GPU cases ignored; strict scoped lint and
exact-source tracer build pass. No fresh GPU draw acceptance for this resource
loading slice.

Final replay:

```sh
future/target/debug/examples/trace_application code \
  future/target/private-originals/ev/application.raw-rsrc 10000000 \
  --graphics --display 640x480x8 \
  --volume /tmp/systemless-ev-pixpat-final-saved.slv \
  --save-volume /tmp/systemless-ev-cicon-final-saved.slv \
  --services-only --elapse-waits
```

The saved-volume destination must be fresh when repeating. Log:
/tmp/systemless-ev-cicon-final.txt. It passes GetCIcon and ReleaseResource, then
stops at missing PlotCIcon ($AA1F), PC00017424, for a16x16 indexed offscreen target.
Sound/timer/VBL callbacks all0. The final checkpoint is7,239,400bytes,SHA256
b2d0862f9286b6f61aff477109cf822962995538e10889a4a9bd3057dd53ab9c.
The independent verifier passes CRC/catalog/forks, preferences48->49 with unchanged
metadata/fork bytes and companions. App and input hashes are in progress.md.
Tracer flushes remain memory-only. PlotCIcon drawing is the next executable slice;
original frame/UI/audio/gameplay acceptance remains unverified. Marathon retains
its last verified indexed TextFont/no-current-port stop and was not replayed here.
Neither priority game is playable. Full rebuild and complete wgpu text rendering
remain active.

## Original Escape Velocity: wgpu PlotCIcon (2026-10-16)

The shared PlotCIcon service completes27original icon calls and proceeds to
DrawPicture. Its next missing service is Button ($A974), PC000330E2; the trace
still names DrawPicture as the latest *decoded* service because Button is not yet
implemented. No CIcon/PICT failure is inferred from that diagnostic.

Replay command (the output volume filename must be fresh):

```sh
future/target/debug/examples/trace_application code \
  future/target/private-originals/ev/application.raw-rsrc 10000000 \
  --graphics --display 640x480x8 \
  --volume /tmp/systemless-ev-cicon-final-saved.slv \
  --save-volume /tmp/systemless-ev-plotcicon-saved.slv \
  --services-only --elapse-waits
```

Terminal exit1, log /tmp/systemless-ev-plotcicon.txt. Sound/timer/VBL callbacks all0.
The independent volume verifier passes CRC/catalog/forks, preferences49->50 with
identical metadata/fork bytes and companions. Output7,239,400bytes,SHA256
07c0ea3d74c3cfcc18ed79ed07ec7c4e1b05aeb5fa18fe751811f5db6eded5e1.
The unchanged app/input hashes and detailed GPU/CPU evidence are in progress.md.
Fresh real-adapter tests verify both CPU adapters, compiled/live icons, packed
indexed formats, shallow-screen fallback, masks, scaling, exact region clipping,
recoloring, source edits and malformed-input atomicity. They do not prove native
Macintosh pixel parity or a real2-bit host display. Original icons ran through wgpu,
but no original screen export, interaction, audio or gameplay is accepted yet.
Tracer flushes remain memory-only. Next: mouse-button service and bounded input
interaction. Marathon retains its last verified indexed TextFont/no-current-port
boundary; it was not replayed here. Neither priority game is playable. Full rebuild
and complete wgpu text rendering remain active.

## Original games after Button/StillDown (2026-10-16)

Both CPU adapters now share current button polling via MBState and tracking via
physical press plus no pending mouseUp. Host delivery keeps the guest state byte
and events coherent, including masked presses and focus loss. Primary-source
conflicts and the preserved-Systemless tracking comparison are in architecture.md.

Original EV first advanced past Button but exhausted its budget in the previous
queue-only StillDown implementation. The corrected final replay, with no injected
input and a read-only MBState watch, completes27PlotCIcon,72Button and1StillDown
calls, then stops at GetKeys($A976),PC01022152, after menu drawing. MBState stays$80
(released). Latest decoded service Menu(Draw) is not the failing operation.

```sh
future/target/debug/examples/trace_application code \
  future/target/private-originals/ev/application.raw-rsrc 10000000 \
  --graphics --display 640x480x8 \
  --volume /tmp/systemless-ev-plotcicon-saved.slv \
  --save-volume /tmp/systemless-ev-button-tracking-saved.slv \
  --services-only --elapse-waits --watch-address 172
```

Output filename must be fresh. Log /tmp/systemless-ev-button-tracking.txt,exit1;
callbacks all0. Independent volume verification passes, preferences50->51 while
metadata/fork bytes and companions remain unchanged. Snapshot SHA256
aa7b09d1b3ab4660f83616315fe9ddfaf0f07b2d232441cfca75fa215ccba32c,
7,239,400bytes. Input/app hashes and the initial diagnostic budget exhaustion are
in progress.md. Flushes are memory-only, not host persistence acceptance.

Fresh Marathon replay uses a reconstructed Git-ignored companion catalog because
the old temporary volume disappeared. Map/Shapes/Sounds/Music contain original
forks and Finder metadata; this is a new catalog, not a byte-identical recovery of
the old image. Volume:future/target/private-originals/marathon/companions.slv,
11,595,805bytes,SHA256
3eb20478ac49df19a8a6015e2ab196366475042edbcb3e25f94474a75154f1d7.

```sh
future/target/debug/examples/trace_application code \
  future/target/private-originals/marathon/application.raw-rsrc 1000000 \
  --graphics --display 640x480x8 \
  --volume future/target/private-originals/marathon/companions.slv \
  --services-only --elapse-waits
```

Log /tmp/systemless-marathon-button-indexed.txt,exit1:TextFont family4,
PC01010E86,no current QuickDraw port,62timer callbacks,0sound/VBL. App hash unchanged.
93scoped tests and strict lint pass;3existing core GPU tests are skipped, no fresh
GPU rendering acceptance here. Next: GetKeys/keyboard state and bounded interaction,
and Marathon's missing port initialization. Neither game is playable; no original
visible frame, real menu/keyboard interaction, audio or gameplay acceptance.
Full rebuild and complete wgpu text rendering remain active requirements.


## GetKeys and bounded keyboard diagnostics (2026-10-16)

Shared native/PPC GetKeys now reads the canonical guest KeyMap. Host keyboard
delivery updates that map and posts classic key events through one shared driver.
102scoped tests pass;3existing GPU tests remain ignored. Driver Caps latch tests
do not establish native macOS Caps delivery (the pinned Winit backend omits it).
IME/KCHR/dead keys, non-US fallback and software repeat remain open.

EV with the same original app and prior button-tracking volume now completes
17,046GetKeys calls through a10,000,000-boundary trace budget. No unsupported-call
fault occurs, but the terminal outcome is budget exhaustion, not application exit
or gameplay acceptance. Log:/tmp/systemless-ev-getkeys.txt. Volume integrity passes
with preferences51->52 and unchanged metadata/fork bytes/companions; output SHA256
d1f1d500e1a44112766eb4e6fcecaef9d79c18001c1eca5fed8a209129db572b.

The tracer now supports --save-screen NEW_PPM (real GPU readback after canonical
display refresh) and --key-at INSTRUCTION:CODE:down|up[:TEXT]. Scheduled input uses
the same Keyboard driver as the native window; it never forces a PC or GetKeys
result. Outputs must be fresh; all original bytes and captures remain outside Git.

A fresh no-input capture (/tmp/systemless-ev-keys-baseline-1.txt,exit1 for budget)
ends at instruction10,000,000,PC00032FBC. The640x480GPU capture shows a white menu
bar with File/Edit above a black game area. This is actual visual evidence of the
menu bar, not a complete game screen. Neither priority game is verified playable.
Marathon's latest boundary remains TextFont4/no current port atPC01010E86, as
recorded above; it has not been replayed in this keyboard-only milestone.

Bounded Return capture: /tmp/systemless-ev-keys-return-1.txt,exit1 for budget.
The shared driver delivers code36 down at instruction8,000,000 and up at8,005,000.
The subsequent trace includes WaitEvent then FindWindow/CursorInit/MenuSelect;
this changed service path does not establish correct keyboard handling. Final
PC000314A4 versus baseline00032FBC. The final GPU capture is byte-identical to
baseline: PPM SHA2562993b33f4929130f24c427580c648d004d92c814fff74e16b64ef16b7bf04529.
No visible interaction success is claimed. Next: verify event decoding/handling
and diagnose the black game area, alongside Marathon's current-port boundary.


## Verified N shortcut reaches new-pilot dialog (2026-10-16)

Replay uses the unchanged original EV application and the same button-tracking
input volume. Repeat with a fresh screen-output filename:

```sh
future/target/debug/examples/trace_application code \
  future/target/private-originals/ev/application.raw-rsrc 8100000 \
  --graphics --display 640x480x8 \
  --volume /tmp/systemless-ev-button-tracking-saved.slv \
  --key-at 8000000:45:down --key-at 8005000:45:up \
  --save-screen /tmp/systemless-ev-newpilot-1.ppm \
  --services-only --elapse-waits
```

Log:/tmp/systemless-ev-newpilot-1.txt. The press is delivered; the application faults
before the scheduled release, at instruction8,000,733,PC0003297E, while loading
Dialog(Resource{id:3100,storage:0,behind:-1}). Error:unsupported dialog item type
or data. No PC/result override and no synthetic Toolbox result.

Actual GPU output now shows the green New Pilot button over the black game area
under File/Edit: a visible shortcut response, not a completed pilot dialog or
gameplay acceptance. Capture:/tmp/systemless-ev-newpilot-1.ppm, SHA256
d3ee58de9012d7009d8df8c147c694377c022d3ac82bfdf41c9abe697e034cf3.
Read-only original-resource inspection confirms DLOG3100 uses DITL3100 with seven
items:button,checkbox,disabled static text,disabled picture,editText16,button,
disabled picture. Item5 is the unsupported editText, bounds(top33,left60,bottom49,
right260),nine initial bytes. No original payload bytes enter Git. Next executable
slice: shared dialog edit-text ownership/TextEdit integration and GPU drawing,
then actual pilot-name interaction and modal event handling; do not treat editText
as static text or suppress this failure. Marathon's port issue remains separate.
Full rebuild/complete wgpu text integration stay active. Neither game is playable.


## New Pilot dialog editText and selection (2026-10-17)

Both CPU adapters now share DITL editText ownership and SelectDialogItemText/
SelIText. TextEdit's active hText is the actual item handle, not a host-side copy.
Shared TEKey fixture edits and Get/SetDialogItemText observe the same bytes; field
switching, GPU selection and cleanup are covered by the scoped checks in progress.md.
Dialog event dispatch and actual original typing/confirmation remain incomplete.

```sh
future/target/debug/examples/trace_application code \
  future/target/private-originals/ev/application.raw-rsrc 8100000 \
  --graphics --display 640x480x8 \
  --volume /tmp/systemless-ev-button-tracking-saved.slv \
  --key-at 8000000:45:down --key-at 8005000:45:up \
  --save-screen /tmp/systemless-ev-edit-dialog-verified.ppm \
  --save-volume /tmp/systemless-ev-edit-dialog-verified.slv \
  --services-only --elapse-waits
```

Use fresh output names. Log:/tmp/systemless-ev-edit-dialog-verified.txt,terminal
exit1, missing ModalDialog$A991 at instruction8,036,025,PC00032A44. N down and up
are both delivered. The game loads DLOG3100, draws it, sets item5 text and calls
SelectDialogItemText(5,0,254), followed by FlushEvents before the missing trap.
Latest decoded FlushEvents is not the failing service. The actual GPU capture
shows the pilot-name dialog with the name selected, explanatory text, checkbox
and Cancel/OK buttons. This verifies drawing and programmatic selection in the
original, not modal editing, pilot creation, audio or gameplay. The surrounding
game area remains largely black except for the previously activated New Pilot
button. No forced PCs or fake results.

The first replay (/tmp/systemless-ev-edit-dialog-1.txt) and the repeat capture are
byte-identical:PPM SHA256
9c9ea9bf3239fdf262f78668a08d635646183d6ca161d40c565afddee2771e56.
Input volume SHA256
aa7b09d1b3ab4660f83616315fe9ddfaf0f07b2d232441cfca75fa215ccba32c.
Independent output verification passes CRC/catalog/companion forks,preferences
51->52 with identical metadata/fork bytes. Output7,239,400bytes,SHA256
d1f1d500e1a44112766eb4e6fcecaef9d79c18001c1eca5fed8a209129db572b.
Original application hash remains unchanged. Flushes remain memory-only.

Next:shared ModalDialog polling, real guest filter callbacks and TextEdit/control
event routing, then actual pilot-name entry/confirmation. Marathon has not been
replayed here; its TextFont4/no-current-port boundary remains. Neither game is
playable. The full rebuild and complete wgpu text/library integration stay active.

The verified capture and90-line trace tail are also preserved privately under
future/target/private-originals/ev/evidence/2026-10-17-new-pilot.{png,ppm}
and2026-10-17-new-pilot-trace-tail.txt, all Git-ignored.


## Real modal typing and resource-backed pilot creation (2026-10-17)

ModalDialog now consumes actual shared host input, with guest filter execution
and cooperative waits instead of fabricated results. The original reads back
ben after b/e/n and Return, then opens DLOG3001, accepts its default shuttle name
Journeyman on a second Return, and writes a pilot resource file. Reproduce with
the same private CODE fork and durable copy of the original input volume:

```sh
future/target/debug/examples/trace_application code \
  future/target/private-originals/ev/application.raw-rsrc 10000000 \
  --graphics --display 640x480x8 \
  --volume future/target/private-originals/ev/evidence/2026-10-17-modal-input.slv \
  --key-at 8000000:45:down --key-at 8005000:45:up \
  --key-at-poll 8044000:11:down --key-at-poll 8044001:11:up \
  --key-at-poll 8044500:14:down --key-at-poll 8044501:14:up \
  --key-at-poll 8045000:45:down --key-at-poll 8045001:45:up \
  --key-at-poll 8045500:36:down --key-at-poll 8045501:36:up \
  --key-at-poll 8050000:36:down --key-at-poll 8050001:36:up \
  --trace-range 8060000:8060064 \
  --save-screen /tmp/systemless-ev-modal-pilot-extended.ppm \
  --save-volume /tmp/systemless-ev-modal-pilot-extended.slv \
  --services-only --elapse-waits
```

Use fresh output names. The original run used the byte-identical temporary input
/tmp/systemless-ev-button-tracking-saved.slv. Guest-instruction --key-at and host
advance-call --key-at-poll clocks are distinct; the latter permits keys while a
modal procedure blocks CPU execution. Every action uses the shared host Keyboard
driver. No direct text-handle edits, forced PCs or fake return values occur.
Actual GetDialogItemText readbacks are ben twice and Journeyman twice.

Final log:/tmp/systemless-ev-modal-pilot-extended.txt,exit1. Stops at missing
[NewAlias](https://dev.os9.ca/techpubs/mac/Files/Files-355.html),AliasDispatch
selector$0002,PC0002E7B2,instruction9,157,968,while preparing the Last Pilot
reference. Latest decoded DeleteSpec is not the failing call. Completed original
sound/timer/VBL callbacks0. A preliminary8.1Mrun reached shuttle naming but did
not confirm it; an8.06Mrun confirmed it and exhausted its budget in later guest
arithmetic. The extended run passes that bounded initialization and writes the
pilot. Those budget stops are not unsupported-call faults or gameplay evidence.

Independent verification:/tmp/systemless-modal-volume-verification.txt. CRC,
volume framing, directories and all original companion metadata/forks pass.
Preferences51->52 is byte-identical apart from catalog identity. Only added file:
Pilots/ben,parent17,node53,empty data fork,18,952-byte resource fork. Finder bytes
4d70954c4d9172630000000000000000; type4d70954c resources128=9,646bytes,129=8,958
bytes,each independently checked within the fork's data bounds. Resource SHA256
c75ab0d0e07173942ef9a5aa0fd5d44f55e05c2967650838a5d3ddf001730f4c.
Output volume7,258,396bytes,SHA256
0ba0173bc1a866864f8006e5266222a893df677817949e52bb6d9a143bc2f698.
Input SHA256 remains
aa7b09d1b3ab4660f83616315fe9ddfaf0f07b2d232441cfca75fa215ccba32c.
Earlier two volumes remain the unchanged preferences-only snapshot SHA256
d1f1d500e1a44112766eb4e6fcecaef9d79c18001c1eca5fed8a209129db572b.

Private durable evidence under future/target/private-originals/ev/evidence/:
2026-10-17-modal-input.slv,2026-10-17-modal-pilot.slv,
2026-10-17-ship-name.png,2026-10-17-modal-final.{png,ppm},
2026-10-17-modal-trace-tail.txt and2026-10-17-modal-volume-verify.py. All are
Git-ignored. The verifier takes the durable input and pilot-volume paths.
Shuttle dialog capture PPM SHA256
c48fc64e0d46a0067639baf2f0b55de74028042d4d8f23e1053eca878a8bddb9.
Final fault capture PPM SHA256
0c514d92e883ae6cb8e2f7682834938a01456c61988a4d565dcd991431283094;
it shows a cleared rectangle over the mostly black game area, not a flight frame.

141 distinct scoped cases pass, including25actual GPU cases and both CPU/filter/
timer-wait paths; exact logs/counts are in progress.md. Modal editable-field
mouse selection and external DialogSelect still fail explicitly/remain missing.
Next:real NewAlias records and Last Pilot reference handling, followed by actual
pilot re-open and flight interaction. The snapshot proves resource-backed file
creation, not semantic pilot validity, re-open, host persistence, audio or gameplay.
Flush acknowledgments are memory-only. Marathon has not been replayed here.
Neither priority game is playable; full rebuild and complete GPU text remain active.
