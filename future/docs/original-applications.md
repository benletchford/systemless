# Original application reconnaissance

These are bounded startup checks, not application acceptance. No window or game
interaction has been exercised. The binaries are external fixtures and are not
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
