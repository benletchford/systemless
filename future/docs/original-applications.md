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
