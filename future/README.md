# Systemless future

An independent replacement runtime on `dev/future`. One guest address space and
service dispatcher serve real 68K and PowerPC engines. Indexed drawing, palette
expansion and presentation use wgpu. This build does not link the legacy runtime.

**Current acceptance: generated and MPW-compiled guests run across 68K/PPC,
including shared resource/heap lifetime, CFM loading, events/waits, drawing,
TextEdit, window updates, resource-defined windows and button-family controls.
Offscreen GPU output is checked. Original applications have bounded startup
traces; original UI interaction and native presentation remain unverified.**

Resource-fork/CODE and PowerPC PEF loaders share one Macintosh world. Resources,
allocation, virtual directories/file metadata, OS/update events, Basic QuickDraw,
fonts, plain TextEdit, cursor state, window records and controls use common
services. GetPicture shares PICT loading with the Resource Manager; PortSize
and MovePortTo support offscreen port setup. DrawPicture supports bounded
monochrome bitmap commands in PICT v1/v2 and 8-bit indexed PICT v2 source
PixMaps with GPU palette conversion to the monochrome destination. Vector/text
commands, other PixMap depths, color destinations and picture recording remain pending. CopyBits shares GPU scaling,
source transfer modes and complex masks for monochrome BitMaps, including
overlapping storage. LineTo/Line share GPU rectangular-pen coverage, patterns,
transfer modes and live pen advancement. Rectangle frame/erase/invert/fill use
shared clipping and operation-specific patterns; FillRect updates fillPat.
Recording/custom procedures remain gaps.
Menus and dialogs currently provide startup/parameter subsets. Real
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
imports fail explicitly. Guest file I/O, Toolbox window/high-level event synthesis, mouse regions, the remaining
QuickDraw operations, handle locking/purging and general heap compaction and original-app compatibility
remain pending. The rebuild goal is incomplete.

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
close/reopen on both CPUs. Loaded application forks stay read-only; persistence
and full filesystem compatibility remain pending.

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
Opening additional resource maps, resource updates and persistence remain pending.
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
Random itself is unchanged. The original draws a populated board, then stops at
SetEmptyRgn ($A8DD) during update handling. Gameplay and control callbacks remain
unfinished. Next: empty-region mutation and original board interaction. The full
rebuild stays active.
