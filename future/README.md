# Systemless future

An independent replacement runtime on `dev/future`. One guest address space and
service dispatcher serve real 68K and PowerPC engines. Indexed drawing, palette
expansion and presentation use wgpu. This build does not link the legacy runtime.

**Current acceptance: generated and MPW-compiled guests run across 68K/PPC,
including shared resource/heap lifetime, CFM loading, events/waits, drawing,
TextEdit, window updates, resource-defined windows and button-family controls.
Offscreen GPU output is checked. Super Mines has bounded splash, board, menu,
save and open interactions; full original application acceptance and native
presentation remain unfinished.**

Resource-fork/CODE and PowerPC PEF loaders share one Macintosh world. Resources,
allocation, virtual directories/file metadata, OS/update events, Basic QuickDraw,
fonts, plain TextEdit, cursor state, window records and controls use common
services. GetPicture shares PICT loading with the Resource Manager; PortSize
and MovePortTo support offscreen port setup. DrawPicture supports bounded
monochrome bitmap commands in PICT v1/v2 and 8-bit indexed PICT v2 source
PixMaps with GPU palette conversion into monochrome or indexed destinations.
Vector/text commands, other PICT source depths and picture recording remain
pending. CopyBits shares GPU scaling, transfer modes and complex masks
for monochrome BitMaps and checked live 1/2/4/8-bit indexed PixMap sources,
including overlapping storage. Device queries expose owned GDevice/PixMap/table
records for the actual configured main screen; Index2Color reads its live
palette. Color-port lifetime and packed 1/2/4/8-bit drawing use attached PixMap/PixPat
records, including text and temporary pattern restoration. Selected offscreen
devices can initialize color ports; main-screen depth changes, palette lifecycle
and general Color QuickDraw remain pending. NewCWindow shares
standard one-bit window creation, updates, geometry, painting and disposal with
NewWindow, including hidden empty windows that are resized before display.
LineTo/Line share GPU rectangular-pen coverage, patterns,
transfer modes and live pen advancement. Rectangle frame/erase/invert/fill use
shared clipping and operation-specific patterns; FillRect updates fillPat.
Recording/custom procedures remain gaps.
Menus and dialogs implement bounded standard monochrome subsets. General
Mixed Mode, custom WDEF/CDEF callbacks and complete application compatibility
remain unfinished. Private fixture gateways are disabled for loaded applications.
See [architecture](docs/architecture.md) and [progress](docs/progress.md).

From the repository root:

```sh
cargo test --manifest-path future/Cargo.toml --locked
# Explicit GPU acceptance; requires a real adapter and fails if unavailable.
cargo test --manifest-path future/Cargo.toml --locked --test contracts -- --include-ignored --nocapture
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

The default test command reports GPU and MPW compiler tests **ignored**.
Build and run the compiler fixture using [its instructions](fixtures/mpw/README.md). They are not counted
as passed until explicitly run. Native blocking GPU synchronization is the
first correctness boundary; browser integration and optimization are pending.

Generate and run the real-format resource/handle fixtures:

```sh
cargo run --manifest-path future/Cargo.toml --locked --example make_load_fixtures -- /tmp/systemless-load
cargo run --manifest-path future/Cargo.toml --locked -- --code /tmp/systemless-load/resources.rsrc --budget 1000
cargo run --manifest-path future/Cargo.toml --locked -- --pef /tmp/systemless-load/application.pef --resources /tmp/systemless-load/resources.rsrc --budget 1000
```

The generator also writes `lifecycle.pef`; run it with `--pef` to exercise
initializer, main and cleanup, returning 7 after 17 instruction boundaries.

The resource fixtures both load a DATA resource, inspect its handle, allocate cleared memory and exit.
Supported loading is near-model CODE and PowerPC PEF, including pattern data and
relocations. PEF initialization and termination run with bounded execution and a guest
initialization block. Compressed resources, far-model CODE and unknown required
imports fail explicitly. Persistent file I/O, high-level event synthesis, remaining
QuickDraw operations, handle purging/general heap compaction and complete original-app
compatibility remain pending. The rebuild goal is incomplete.

Loaded applications can yield `Stop::Waiting { ticks }` from WaitNextEvent.
Embedding hosts inject events and call `elapse_ticks` before resuming; waiting
polls do not execute instructions. The file CLI advances virtual time to each
wake deadline while retaining one total instruction budget. The bounded trace
example reports a wait and returns control to its caller.

Use `--graphics` with `--code` or `--pef` to acquire a headless GPU for loaded
applications. Without that option, CPU-only programs need no GPU and drawing
fails explicitly. Basic QuickDraw describes a fixed 512x342 monochrome screen;
other bitmap sizes use temporary targets on the same device. The compiled
drawing acceptance test also runs both programs through this CLI path. Color ports,
recording, custom QDProcs, windows and full graphics compatibility remain pending.

For embedding hosts, `Runtime::refresh_screen` updates the display texture from
canonical screen memory, including direct guest writes. The graphics CLI calls
it after execution. Loaded applications can also use `--window` (which supplies graphics), with
`--smoke` for bounded presentation/resize verification. Their CPU execution is
sliced across redraws, waits use elapsed host ticks, and the final screen remains
until the window closes. Left-button mouse input and Command/Shift/Option/Control modifiers now feed
the shared event queue. Keyboard events, Caps Lock and live interaction
verification remain pending. Window integration
is implemented but presentation is still unverified on this host.

Region storage supports NewRgn, DisposeRgn, RectRgn, CopyRgn, GetClip and SetClip.
Variable-sized region payloads can be copied and decoded for hit testing and
GPU clipping, including holes and disjoint spans. SectRgn, UnionRgn, DiffRgn and XorRgn construct regions through the shared heap.
Native Macintosh comparison and broader region drawing/transform APIs remain pending.

Window lifecycle now includes MoveWindow, SelectWindow, ShowWindow/HideWindow,
activation-event delivery and InvalRect/ValidRect. Movement uses wgpu copies from
a preserved guest-memory snapshot and queues exposed content for application
redraw. Legacy Systemless is the permitted comparison baseline; native System 7
verification and complete original application interaction remain pending.

TETextBox/TextBox now share transient TextEdit wrapping, alignment and clipped
GPU erasure/drawing on both CPU adapters. Bold source strikes are synthesized
without changing guest font resources. Super Mines startup passes its text boxes
and enters event/mouse polling. A bounded press/release dismisses its splash,
creates/reads settings, loads, installs and draws its resource menu bar, updates menu items and reaches
the chooser event loop;
gameplay and complete application lifecycle acceptance remain pending.

GetMouse shares checked current-port coordinates and live guest Mouse globals;
PtInRect shares signed, half-open rectangle containment. A bounded 68K mouse probe
captures offscreen screens and propagates faults for original-app diagnostics.

FindWindow and FindControl query live guest window/control lists through shared
services. StillDown examines pending mouse events without consuming them.
SetPort(nil) supports saved-port restoration while drawing without a port fails.

Shared process-local file I/O now creates mutable data/resource forks, enforces
access permissions and supports reading/writing, positions, EOF, metadata and
close/reopen on both CPUs. Working-directory references now carry real folder
identity through classic file calls and explicit HFS directory overrides. Both
CPU routes open/query/close those references against the same catalog. Loaded
application forks stay read-only. Optional --volume backing now persists the
writable catalog; full filesystem compatibility and per-service host durability
remain pending.

GetMHandle/GetMenuHandle now searches live regular/hierarchical lists without
loading resources. Gestalt shares the known virtual profile and reports missing
capabilities explicitly. GetMenu/GetNewMBar load live resource-backed menus,
SetMenuBar copies their list, and DrawMenuBar renders standard bitmap titles
through wgpu. Insertion, popup tracking and guest definition callbacks remain
unfinished.

CountMItems, resource-name append, item marks/checks, command keys and item text
now operate on the same live menu records through both CPU adapters. Super Mines
passes menu setup and starts another resource window; writable Resource Manager
files, menu interaction and full application acceptance remain unfinished.

NewMenu and AppendMenu now construct live standard records and parse item
specifications on both CPUs. PenNormal resets pen defaults without changing other
port state. Super Mines reaches its partially drawn game/difficulty chooser;
selection and gameplay remain unfinished.

CreateResFile writes a valid empty map through the same File Manager fork store,
and checked resource-fork serialization preserves supported resource metadata.
Opening additional resource maps and Resource Manager updates remain pending;
initialized fork bytes can persist through explicit volume backing.
An earlier continuation note incorrectly called the original NewMenu trap
CreateResFile; resource-file creation is proven by fixtures, not this original run.

GetMenuItemText now reads live menu records through both adapters. Polygon recording,
visibility, offsets and disposal share guest handles; polygon fills and edge unions
run on wgpu with shared clipping/pattern rules. The bounded original splash now
reaches the chooser event loop; a live-control press waits in TrackControl and
its release opens the game window. Shared NumToString/StringToNum now handle
signed integers through Pack7 and PPC imports. The original renders its numeric
labels and completes a shared GPU CopyMask call. Standard cursor resource
fallbacks now supply CURS 1..4 with normal shared handle lifetimes and application
precedence. Shared GetResInfo now reports live resource ID/type/name on both CPUs.
The original passes TILE 1101 metadata and decoding. A shared initialized calendar
clock now supplies its date-based seed and advances with execution/host ticks;
Random itself is unchanged. The original draws a populated board, then returns to
a normal event wait after routing the update through shared IsDialogEvent and
EmptyRect. A real press/release at (30,65) changes upper-left board cells and
returns to the event wait without a service fault. This verifies one basic board
interaction; broader gameplay, menu-driven lifecycle and callbacks remain
unfinished. Region/coordinate changes, mouse-region waits and dialog event
classification share live guest state across both CPUs. A follow-up File-menu
click now opens/tracks the File dropdown through shared MenuSelect. Separate
New Game and Quit drags return their real packed commands and decode through
shared HiWord/LoWord. Shared MoveControl/SizeControl now handle geometry,
owner-background erasure and dirty regions. The original Quit route passes
control movement and shared TEGetHeight while laying out its confirmation
prompt. Shared GetIcon returns the real missing-resource result for ICON 0;
the Quit confirmation displays and pressing No disposes its controls/TextEdit
and windows. Shared GetWMgrPort now returns the existing monochrome manager
port; the original No-save Quit path completes file/resource cleanup and reaches
ExitToShell with code 0. No-save New Game also returns through its chooser to a
fresh board and the normal event wait. Cancel resumes the existing game with a
remaining board-border redraw difference. The Yes-save branch now uses a shared cooperative SFPutFile dialog, writes a
1,972-byte game file in the process-local catalog and reaches Exit(0). Cancelling
that dialog returns to the game without creating the file. A same-process
Save Game > Open Game sequence now selects the saved file, executes the guest
filter, reads it back and resumes the board/event wait. Per-service host durability/error semantics,
exact board-redraw parity and broader gameplay remain separate acceptance work.
SFGetFile now shares the cooperative picker, type filtering and real
budgeted guest file-filter callbacks between both CPU backends. The full rebuild
stays active.


Writable files can now survive launches with the explicit `--volume PATH`
application option. CODE and PEF share a validated catalog image containing
mutable directories, metadata and both forks; executable application bytes and
transient references remain separate. Host checkpoints use exclusive locking
and atomic replacement. See [volume format and durability boundaries](docs/volumes.md).

The original Super Mines save-and-Quit route now writes an explicit volume, and
an independent fresh process reopens that game through its real filter and
returns to the restored board/event wait. This is a bounded cross-launch check;
full application and rebuild acceptance remain active.


Sound Manager channel creation/disposal and SndDoImmediate share ownership and
standard sampled playback between 68K and PPC. The shared renderer reads live
standard 8-bit mono and extended 8/16-bit mono/stereo buffers, resamples/mixes stereo PCM and applies gain/pan/volume. Native
output uses a bounded CPAL transport; headless virtual waits advance completion
without an audio device. Compiler-generated guests verify exact samples and
completion. SndDoCommand now executes an inline guest-visible FIFO, waits on full
queues without spinning, and runs same-ISA callbacks through the guest CPU.
SndPlay now parses loaded format-1/format-2 resources into this same queue,
with real synchronous waits and resource-scoped completion. Live channel/manager
status and a virtual Sound Manager 3.0 ABI target now share both adapters. The
capabilities report stereo, multichannel mixing and software 16-bit samples;
recording, codecs, disk/double-buffer playback remain unfinished. The PowerPC
NumVersion structure uses the actual caller-provided return buffer. Both original
68K games pass the earlier sound-version call; EV passes offscreen graphics-device
attribute updates and inverse-table construction, and selects its offscreen
devices. It now passes current-map resource lookup and next requests GetNewDialog. Marathon reports
that Color QuickDraw is required. Super Mines' recorded interaction
is automated/offscreen; live desktop and win/loss acceptance remain incomplete.


Handle properties now share real allocator metadata across both CPUs:
HGetState/HSetState, HPurge/HNoPurge and resource-bit operations preserve lock,
purgeable and resource flags without changing 32-bit master addresses. Resource
loads initialize those flags; detaching clears the resource bit. MoveHHi can
relocate into the immediately adjacent free interval without crossing other
allocations. HandToHand duplicates the payload into a new independently owned,
unlocked, unpurgeable non-resource handle; both adapters publish real allocation
errors. The current allocator has one application heap; multiple heap zones and
general compaction/purging remain unfinished. The sound-enabled
Super Mines now completes its resource-sound callback and Save/Open route in
a bounded offscreen run with explicit volume and virtual time.

Sound callbacks preserve interrupted CPU and cooperative wait state. A callback
can call shared Toolbox services and quietly dispose/recreate its own channel;
generation checks protect the replacement voice. Queued buffers render across
command boundaries in one PCM pull. A silent 48 kHz clock advances queued work
in time bounded by command count, including very large guest waits. Production
Mixed Mode, pause/wait/sync commands and nonquiet active-disposal
waiting remain unfinished. Native audio quality/timing acceptance remains separate.

Resource sound accepts owned loaded handles without requiring Resource Manager
identity. Relative headers/inline samples are checked against handle bounds;
absolute pointers retain normal guest-memory checks. Commands are copied, sample
bytes stay live, and source flags are retained. A nil channel forces synchronous
playback and its internal allocations are released after completion. Ordinary
synchronous calls wait only for their own resource commands. Buffer playback
ignores unused note-pitch/loop metadata, covering the original EV asset cases.
Synthetic guest callers accepted 167 scanned EV/Marathon sounds through both
adapters, with matching 32-frame PCM prefixes and silent completion/cleanup.
This is asset-level evidence; original EV/Marathon gameplay remains unfinished.


MakeITable now generates the public inverse lookup portion in real owned handles
for resolutions 3/4/5, with default-device parameters and shared QDError state.
ROM tie/hidden-color matching, reserved clients and full color drawing remain
unfinished. No ROM-derived lookup tables are included.

Selected offscreen GDevice state now drives GetGDevice, Index2Color and default
inverse-table construction, while main/list display identity remains separate.
Empty paint geometry returns without pixel access; nonempty requests retain
bitmap checks. General indexed destinations and full color acceptance remain open.

Get1Resource now uses the installed application map and shares real resource
identity/loading with GetResource while excluding cached fallback cursors. Dynamic
resource-map chains and UseResFile remain unfinished.


GetNewDialog now loads checked DLOG/DITL resources into real 170-byte records,
independent copied item lists and shared window/control ownership. GetDialogItem,
CloseDialog and DisposeDialog use the same records through native traps and PPC
imports. Standard buttons, checkboxes, radio controls and static text are owned;
icon/picture handles refer to actual resources. Controls draw on creation;
non-control update drawing, modal event handling, edit-text items, color dialogs,
custom definitions and additional positioning modes remain unfinished. Original
EV passes dialog creation and next requests SetDialogDefaultItem; no dialog or
original-game gameplay acceptance is claimed.


SetDialogDefaultItem now stores shared default-button state and draws a real
outline from live control geometry. Changing/removing the default restores its
old border; hidden dialogs preserve the selection and ShowWindow redraws their
controls and outline. Native selector/result and PPC import/error behavior are
covered by real CPU tests. General dialog update drawing and modal keyboard
activation remain unfinished. Original EV next reaches DrawDialog ($A981).


DrawDialog now draws live static text with parameter substitution through
TextEdit, real ICON/PICT resource handles through shared wgpu graphics, and shared
controls/default outlines. Rendering respects effective clipping and hidden
dialogs, preserves drawing state and frees transient allocations. Visible
application-defined item callbacks, edit-text construction, updates and modal
events remain unfinished. Original EV passes drawing, then reaches item-text
replacement in a dialog reporting a Monitor Tool opening error. This establishes
an error-reporting path, not title/menu or gameplay.


Dialog item text access now converts live raw handle bytes to/from Pascal
strings, preserves handle identity/flags, validates output extents and stages
alias-safe updates. Known static items redraw only their text rectangle, including
shortening/empty erasure; hidden changes need no GPU. Original EV now reaches
ModalDialog after displaying Monitor Tool error ID -4. Its current display depth
is one bit while only 8-bit blitter resources are present; actual indexed-color
rendering remains a priority, and the exact error cause is still under study.


Indexed offscreen destinations now support packed 2/4/8-bit pixels through the
same wgpu drawing boundary as monochrome BitMaps. Both CPUs share real RGB color
setters, live palette matching, clipping, pattern/copy modes and synchronized
guest bytes. Selected indexed devices can initialize color ports. The main
512×342 display remains one-bit; configurable color display, window composition,
palette/device lifecycle and broader Color QuickDraw remain unfinished. Original
EV and Marathon still stop at their existing error paths and are not playable.


A real color main display can now be selected before execution:

```sh
cargo run --manifest-path future/Cargo.toml --locked -- --code APPLICATION.rsrc --display 640x480x8 --graphics --budget 1000000
cargo run --manifest-path future/Cargo.toml --locked -- --pef APPLICATION --resources APPLICATION.rsrc --display 640x480x8 --window
```

The default remains 512×342 monochrome. The explicit color profile owns actual
8-bit pixels, a live 256-entry host RGB332 palette and matching device/port
records. Basic/color drawing, window moves, menu restoration and cursor overlay
share the screen storage; direct guest writes and palette edits are visible on
refresh. Host resizing preserves guest resolution. Runtime depth switching,
palette-manager lifecycle, direct color and full Color QuickDraw remain gaps.
Marathon now passes its initial Color QuickDraw rejection, then requests
PBHSetVol ($A215) during directory setup. EV still reports Monitor Tool ID -4 on
the real color screen. Neither original game has reached gameplay.


Default-volume/directory selection now uses real shared catalog state through
PBSetVol/PBHSetVol on both CPUs. Relative file access and Standard File browsing
follow the selected folder; classic and hierarchical queries preserve their
reference distinctions, failed selections leave state intact, and process
selection stays outside persistent volume images. Marathon now passes PBHSetVol
and next requests OpenRFPerm ($A9C4) to open a resource file. Neither original
game nor the full rebuild is complete.
