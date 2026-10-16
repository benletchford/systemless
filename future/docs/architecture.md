# Unified replacement runtime

Baseline: `634c0e9120a695224d71c621fbbe33709670d707` in the original
`/Users/ben/.codex/worktrees/b2f1/systemless` checkout. Implementation branch:
`dev/future`, separate managed worktree. Tracking issue: #4801. No merge,
release, or website deployment is authorized. The independent `future/`
workspace is the replacement build; the legacy sources remain reference-only
until replacement compatibility justifies retiring them from this branch.

## Evidence and boundaries

The baseline README, loader layout, and Toolbox Showcase sources describe CODE
resources, PEF/CFM, resource forks, archive intake, indexed QuickDraw, events,
files, handles, and nested callbacks. Showcase contains reproducible 68K/PPC
slices and interaction scripts. Its `reference/systemless-*` images are legacy
comparisons, not native authorities. Native emulator recordings are distinct.
Neither the showcase nor commercial applications work in this replacement yet.

Dependency evaluation (2026-10-13): CPU engines `m68k 0.14.4` and `ppc 0.7.2`
provide single-instruction execution with a caller-supplied memory bus. Their
published source/rustdoc in the Cargo registry was inspected, including fault,
trap and context interfaces. No runtime/loader implementation is reused.
wgpu 30.0.1 provides compute, integer buffers, storage textures, offscreen
readback and surface presentation; winit 0.30.13 provides ApplicationHandler.
Pin these evaluated versions; commit the independent lockfile.
Primary API references: https://docs.rs/wgpu/30.0.1/wgpu/,
https://docs.rs/winit/0.30.13/winit/application/trait.ApplicationHandler.html,
https://docs.rs/m68k/0.14.4/m68k/, https://docs.rs/ppc/0.7.2/ppc/.

## Dependency direction and ownership

`host -> runtime -> {cpu, services, graphics, memory}`. `cpu -> memory` and
ABI call values only; services never see CPU registers. `graphics -> memory`
for synchronized pixel access and wgpu for exact integer compute/presentation.
`memory` knows neither CPU nor host. One Runtime owns one address space,
service state, deterministic instruction clock and trace, graphics device, and
an execution continuation stack. Process-owned resources and handles live here; future files, windows, menus
and event queues also belong here, never in CPU backends.
Avoid a generic CPU register interface: an enum owns architecture-specific
engines; ABI adapters decode calls and apply results. Explicit outcomes are
continue, service, callback, return, yield/budget, fault, and exit.

## Memory contract

GuestPtr is u32, never a host pointer. Mappings are non-overlapping checked
ranges; access arithmetic may not wrap. Multi-byte values are big-endian.
Unaligned data accesses are allowed by storage. PPC uses the engine's
`EmulateData` policy for Macintosh packed structures, as required by MPW
startup's two-byte-aligned environment fields. These accesses still traverse
the checked memory bus; instruction fetch requires 2/4-byte alignment. Permissions are
read/write/execute. Whole scalar writes validate before mutation. No fastmem
or CPU instruction caches bypass tracking. Unmapped/protected accesses stop
execution with a structured fault. The 68K bus records the first fault even
if its engine subsequently attempts an exception frame; later accesses in
that instruction are suppressed. Faults are terminal for this first slice;
restartable guest exception delivery requires a later explicit contract.

## Services, ABI and execution

First executable fixtures are raw code with an explicitly private test ABI,
not CODE/PEF compatibility claims. 68K uses A-line gateways with a big-endian
stack argument; PPC uses registered import addresses with r3 arguments and LR
return. Both decode into the same service enum and dispatcher. Loaded applications select the separate Macintosh ABI described below.
Unknown calls fault, never silently succeed. Instruction budgets include
service gateways and returns, making guest loops bounded. Time is deterministic
instruction count; host wall clock cannot change fixture results.

Mixed-mode execution has a home in the runtime continuation stack. A callback
suspends the entire caller engine (including private CPU state), creates a
callee with a separate guest stack, and shares the same memory/services/GPU.
Return restores the caller engine and encodes only the declared result.
Nested transitions therefore require no state reconciliation. The initial
internal routine value is NOT an Apple RoutineDescriptor/ProcInfo parser;
real descriptor validation, calling conventions, stack argument signatures,
TOC preservation and reentrant Toolbox continuations must land with fixtures.

## Graphics ownership and synchronization

Guest indexed pixel bytes are CPU-authoritative at execution boundaries.
A graphics command uploads the current indexed image into u32 GPU storage,
executes an integer WGSL operation, reads the resulting indices back, then
updates guest bytes before the next instruction. This deliberately pays a
synchronous readback per draw to make direct stores, direct loads, and
Toolbox operations coherent from the first stage. No old renderer is used.
Later dirty ranges and deferred synchronization must preserve these tests.

GPU commands own clipping, 8x8 pattern selection, bitwise source transfer
modes and overlap-safe copies (a separate immutable source snapshot).
Palette expansion runs on the GPU into RGBA8Unorm. No blending participates
in guest drawing. Offscreen readback uses padded rows; presentation scales
nearest-neighbor independently of guest resolution. Future irregular regions
are explicit masks, not host scissor approximations. Packed 1/4-bit pixels,
QuickDraw port semantics, color matching and text need further slices.
Exact CPU algorithms are permitted for operations needing synchronous access
or semantics unsuitable for GPU execution, with shared tests and documented
barriers; none is silently substituted when the GPU is unavailable.

The host owns window/surface/input only. Resize changes presentation extent,
not guest memory. Zero-sized surfaces suspend presentation. Lost surfaces are recreated, outdated
surfaces reconfigure; timeouts/occlusion use paced retries; device/validation
errors are surfaced.
Offscreen operation uses the same device/commands without a window. Browser
integration will require asynchronous readback/execution scheduling instead
of native blocking waits, while keeping these semantic modules unchanged.

## Loading and staged acceptance

1. Foundation: independent build, permission/boundary/endianness tests,
   wgpu offscreen command/readback, resizable windowed presentation.
2. Dual CPU: real 68K and PPC instructions, distinct ABI marshalling, shared
   observable service state, unsupported instruction and budget tests.
3. Graphics: both fixtures directly store guest pixels then issue a shared
   drawing service; require identical indices and RGBA readback, plus clipping,
   indexed palette, bitwise transfer and overlapping-copy tests.
4. Lifecycle: checked resource-fork reader and CODE/PEF section loaders,
   relocation/import resolution, shared heap/handles, files and event state.
   Start with a reproducible minimal ordinary Toolbox app per ISA, then
   Toolbox Showcase Graphics/Controls. Missing services remain explicit errors.
5. Mixed mode: nested cross-ISA callbacks and state preservation first with
   internal descriptors, then actual RoutineDescriptor/ProcInfo guest fixtures.
6. Compatibility: complete interaction slices from Toolbox Showcase and one
   explicitly selected original app per ISA. Launch/first frame is insufficient.

Tests distinguish structural fixture contracts, documented Macintosh semantics,
native-oracle behavior and legacy comparisons. GPU tests are opt-in ignored
Rust tests: run explicitly on a real adapter, with failure if unavailable;
default suites report them ignored, never passed. Record adapter and exact
commands in progress.md. No stage is complete before executable acceptance.

## Implemented loader and process slice (2026-10-14)

`load` constructs a fresh checked Memory and only publishes a Runtime on success.
Resource maps preserve raw name bytes and signed IDs, reject overlapping tables
and payloads, and enforce input/count limits. Near CODE loading eagerly maps
referenced segments and patches the A5 jump table. Classic RAM CODE mappings are
read/write/execute: startup may relocate embedded operands or patch instructions.
Writes and instruction fetches use the same canonical bytes; no simulated cache
requires a flush. Mapping boundaries remain enforced. PEF section permissions
remain determined separately by section kind. Apple documents classic
[self-modifying CODE usage and its environment limitations](https://dev.os9.ca/techpubs/mac/pdf/Processes/Time_Manager.pdf)
(Time Manager, page 3-13). PEF materializes sections,
pattern data and BSS, applies bounded relocation streams using existing addends,
resolves registered InterfaceLib imports, and enters the relocated main transition
vector. Unknown strong imports fail; unresolved weak imports remain zero.
Initializer/main/terminator functions are sequenced by runtime lifecycle state.
This custom focused parser has no dependency on the legacy loader.

`abi` decodes 68K Pascal stack and register conventions or PPC CFM registers into
typed service calls, then applies results through architecture-specific replies.
Loaded applications cannot access private fixture gateways. The implemented
services are TickCount, ExitToShell, NewHandle/NewHandleClear, DisposeHandle,
GetHandleSize/SetHandleSize, GetResource and error queries. Unsupported calls terminate clearly.

`process` owns a bounded first-fit heap, guest master pointers, resource identity
and operation errors. Disposal coalesces free blocks. Both CPUs see the same
resource handles and guest-modified contents; cleared allocations zero reused
blocks. Explicit SetHandleSize may relocate data while preserving the master
pointer address. General heap compaction, handle locking and purging remain
unimplemented. The guest
low-memory globals hold MemErr, ResErr and Ticks; deterministic scheduling advances
Ticks every 1000 instructions, with no claim of hardware timing fidelity.

Format evidence: Apple's *Mac OS Runtime Architectures* (1997), chapters 8 and 10,
[PEF sections](https://dev.os9.ca/techpubs/mac/runtimehtml/RTArch-92.html),
[relocations](https://dev.os9.ca/techpubs/mac/runtimehtml/RTArch-98.html),
[CODE layout](https://dev.os9.ca/techpubs/mac/runtimehtml/RTArch-118.html);
*Inside Macintosh: More Macintosh Toolbox* (1993),
[resource layout](https://dev.os9.ca/techpubs/mac/MoreToolbox/MoreToolbox-99.html) and
[GetResource](https://dev.os9.ca/techpubs/mac/MoreToolbox/MoreToolbox-50.html).
Hand-assembled files exercise these contracts; compiler-generated and original
applications remain separate acceptance work.

### PEF lifecycle

A prepared application runs its initializer before main and invokes its terminator
once on main return or ExitToShell. The initializer receives a guest System 7
CFragInitBlock with nonzero opaque IDs, a memory-container locator, the original
PEF bytes, an empty Pascal fragment name and zero reserved fields. The byte-slice
API represents a memory container; filesystem locators require the future VFS.
The relocated vectors select each routine's entry and TOC. Main's prepared CPU
state is retained across initialization; cleanup uses the process entry stack
and returns the original exit/return result.

All phases share the normal instruction budget and service dispatcher. Nonzero
OSErr initialization results fault the preparation before main; CPU faults do not
run cleanup. ExitToShell from an initializer or terminator reports an explicit
error. A looping initializer or terminator exhausts budgets rather than blocking
the host. This state machine owns the application fragment. Dynamic fragments
use the process connections and continuations described below. Shared-library
dependency closure ordering remains unfinished. The application container and
initialization block remain valid for the Runtime lifetime.

Primary references: [CFM special symbols](https://dev.os9.ca/techpubs/mac/runtimehtml/RTArch-26.html),
[PEF loader header](https://dev.os9.ca/techpubs/mac/runtimehtml/RTArch-96.html),
and Apple's [CFM reference](https://leopard-adc.pepas.com/documentation/Carbon/Reference/Code_Fragment_Manager/index.html)
for the System 7 initialization-block layout (680x0 alignment).

### Trap identities and callable entries

Installed OS and Toolbox routines have stable readable executable entry addresses.
Lookup masks the trap number according to table type; every unsupported routine
resolves to the same Unimplemented entry, which faults with dsCoreErr 12 when
called. Native 68K JSR entries remove the caller return address before ABI decoding,
then return after the shared service applies its result. OS entries accept the
original trap word in D1 so modifier variants retain their semantics. PPC CFM
lookup reads the same process table. Toolbox entries live in canonical guest
memory at $71100000. SetToolboxTrapAddress ($A647 or the PPC InterfaceLib import)
replaces the selected ten-bit entry. 68K A-line dispatch enters replacement code
with its return PC above Pascal arguments; saved built-in veneers remain callable
for chaining. Auto-pop glue returns to its caller. Patch execution consumes the
normal instruction budget and permits nested service calls. Invalid targets fault
when executed, rather than at assignment.

OS trap replacement, come-from patches, legacy low-memory table aliases and
NSetTrapAddress are not implemented. PPC imports still invoke registered services;
native PPC patch execution and PPC calls through returned 68K pointers require
real Mixed Mode. Shared table access does not claim those transitions. Primary
contracts: [SetToolboxTrapAddress](https://dev.os9.ca/techpubs/mac/OSUtilities/OSUtilities-180.html)
and [Toolbox dispatch/auto-pop](https://dev.os9.ca/techpubs/mac/pdf/Operating_System_Utilities/Trap_Manager.pdf),
pages 8-15–8-20.

CurResFile reads the process's installed application-map reference. The loader
currently creates one map; opening and switching additional maps must be tied to
the forthcoming VFS. Primary contracts: [trap lookup](https://dev.os9.ca/techpubs/mac/OSUtilities/OSUtilities-177.html),
[OS trap D1 parameter](https://dev.os9.ca/techpubs/mac/PPCSoftware/PPCSoftware-16.html),
and [application resource opening](https://dev.os9.ca/techpubs/mac/MoreToolbox/MoreToolbox-20.html).

### Virtual application file and named resources

The process retains the original loaded data/resource fork bytes in one read-only
virtual application file (default guest name `Application`, configurable with a raw
Macintosh Str31). The virtual volume has reference -1, drive 1, root directory ID
2 and application file ID 16, with 4096-byte allocation units. File opens have
distinct references and independent marks; resource-map identity is the reference
of the resource fork opened at launch. No resource fork supplied means no open
resource reference, not a fabricated empty open file. Additional guest-created
files retain mutable, independent forks within the process. Host paths are not exposed.

PBGetFCBInfoSync and HFSDispatch selector 8 report actual fork length, allocation
extent, read-only/resource flags, mark, name, volume and parent. Indexed and direct
reference queries share the same table. Parameter/name destinations are validated
before either is written. Directory operations and guest open/read/write/close
now share the same catalog. Optional host backing persists writable files;
asynchronous I/O and per-service host flush/error semantics remain pending.
This is a virtual volume model, not an
HFS disk-image implementation. Source fork bytes remain separate from mutable
resource handles until an explicit save/flush model exists.

GetNamedResource/Get1NamedResource compare exact raw names, retain absent-type vs
absent-name error behavior, and resolve to the same cached handle as ID lookup.
Both search modes visit the current single application map; additional maps and
search chains are future work. Primary references: IM: Files (1992), pp. 2-237–238,
[FCB layout](https://dev.os9.ca/techpubs/mac/Files/Files-122.html), and
[GetNamedResource](https://dev.os9.ca/techpubs/mac/MoreToolbox/MoreToolbox-52.html).


## Operating System event queue

`events.rs` operates directly on the shared guest address space. The low-memory
system event mask (0x144), queue header (0x14a), and twenty 22-byte event queue
elements in a reserved mapped arena are authoritative. Elements use 24-byte
slots, preserving the packed Macintosh field offsets. Link traversal is bounded
and validates arena membership, loops, and head/tail consistency. Imported
`GetEvQHdr` and the 68K inline implementation expose that same header. Direct
guest changes to valid event records are observed on retrieval.

PostEvent snapshots the process input coordinates/modifiers and deterministic
guest ticks. Unsupported event types or disabled mask bits return evtNotEnb.
A full queue evicts its oldest element. GetOSEvent and OSEventAvail select the
first matching event; only GetOSEvent removes it. Null events carry current
input/time. FlushEvents removes matching elements until the first stop-mask
match. Output memory is checked before consuming an event. ABI adapters convert
the logical result to the raw 68K 0/-1 convention or PPC Boolean 1/0.

Host keyboard/mouse ingestion, autorepeat generation, Toolbox event synthesis,
mouse-region wakeups, and arbitrary guest-inserted queue allocations remain
pending. The process input snapshot is an explicit host boundary, not a second
copy of the queue. Real Mixed Mode callbacks remain separate unfinished work.

Primary contracts: Apple's [PostEvent](https://dev.os9.ca/techpubs/mac/Toolbox/Toolbox-67.html),
[GetOSEvent](https://dev.os9.ca/techpubs/mac/Toolbox/Toolbox-60.html),
[OSEventAvail](https://dev.os9.ca/techpubs/mac/Toolbox/Toolbox-61.html),
[FlushEvents](https://dev.os9.ca/techpubs/mac/Toolbox/Toolbox-56.html), and
[Introduction to Events](https://dev.os9.ca/techpubs/mac/Toolbox/Toolbox-29.html),
plus MPW Events.h/OSUtils.h for inline traps, record layout and evType=4.


## Cooperative event waiting

WaitNextEvent's adapters marshal a packed 68K Pascal frame/byte result or PPC
register arguments. Runtime retains the typed request and deferred reply while
returning `Stop::Waiting { ticks }`; the CPU is not stepped during idle polling.
The host may add an event to the shared queue or advance deterministic time
through `elapse_ticks`. A matching event, pointer outside the supplied mouse
region, or expiration completes the request. Unmatched events remain queued. A zero sleep never yields. The event
record and deferred result are validated before consuming an event.

Wait remaining time is a saturating duration; guest TickCount remains a wrapping
32-bit value. Time advances explicitly while idle and through the existing
instruction clock while executing. No host wall clock or sleep is hidden inside
the runtime. The file CLI advances directly to wakeup times, retaining a single
instruction budget across all yields. Interactive host pacing remains pending.

This includes queued events, synthetic window activation/updates, null-event
timeout and checked global mouse-region delivery in a single foreground process.
High-level events, system interception, FKEY/alarm and complete process switching
remain gaps. Supporting a wait is not complete Toolbox Event Manager compatibility.

Contract: Apple's [WaitNextEvent reference](https://dev.os9.ca/techpubs/mac/Toolbox/Toolbox-51.html)
and MPW Events.h; compiled MPW callers verify both ABI paths.

## Packed QuickDraw surface boundary

`bitmap.rs` reads the 14-byte guest BitMap record and validates its base address,
row stride, signed bounds and mapped image extent. The current GPU extent limit
is 4096 in either dimension; empty/inverted bitmaps and PixMap flags fail
explicitly. Commands use the bitmap's local coordinates. Translation to GPU
coordinates includes pattern phase, so nonzero or negative origins do not shift
an 8x8 pattern incorrectly.

`surface.rs` shares packed 1/2/4/8-bit storage conversion and drawing dispatch
between basic BitMaps and indexed PixMaps. Metadata, live palette entries and
pixel indices are validated before rendering. CPU work unpacks/repacks storage;
geometry and pixel transfers execute in wgpu. Each operation reloads guest bytes
and synchronizes the packed result before returning, preserving row padding and
unused tail bits. A different extent uses a temporary texture on the same GPU
device. Palette changes are read on the next operation.

Indexed destinations use actual pixel indices, live 16-bit RGB entries and exact
integer nearest-color matching, with lowest pixel index breaking ties. GPU
matching preserves full distance precision using carry arithmetic. Matching work
is bounded. Same-palette source copies preserve duplicate-color index identity
and snapshot overlapping sources. Pattern transfers use actual foreground and
background colors; color transfers operate in RGB space before palette matching.
This deterministic policy does not establish native ROM tie/propagation behavior
or conformance for colored Boolean combinations Apple leaves undefined.

RGBForeColor/RGBBackColor share requested RGB and device-matched index state on
both CPUs. Initialization copies the selected indexed device's format/table;
attachments remain owned and the color table borrowed. Custom device search or
comparison procedures, palette-relative tables, direct color, arithmetic/dither
modes, expanded pixel patterns and indexed cursor presentation remain explicit
unsupported boundaries. One-bit drawing retains white/black palette semantics.
The default display remains one-bit 512×342. The optional indexed main-display
policy below uses real screen storage and a live main-device palette.

This bitmap/backend boundary also serves loaded applications through the
GrafPort services and real QuickDraw ABI adapters described below.
Reference: Apple's [BitMap record](https://dev.os9.ca/techpubs/mac/QuickDraw/QuickDraw-26.html)
and MPW Quickdraw.h.

## Guest GrafPort services

`quickdraw::Ports` binds guest addresses for the current-port slot and screen
bitmap descriptor; it does not cache either record. OpenPort creates the
108-byte basic port and two rectangular region handles using the shared process
heap. SetPort, SetPortBits and ClipRect mutate those guest records. PaintRect
reads live port bounds, region handles, pattern, colors and transfer mode, then
uses the packed-bitmap wgpu boundary. Pattern phase follows the port rectangle.
Screen visibility clips screen drawing but does not clip offscreen bitmaps.

Monochrome colors and bounded rectangular/complex clipping regions are supported.
Picture/region/polygon recording and custom QDProcs fail explicitly. The real 68K/PPC adapters now call this service layer; InitGraf binds
application-owned globals as described below.

Defaults and layouts follow Apple's [OpenPort reference](https://dev.os9.ca/techpubs/mac/QuickDraw/QuickDraw-32.html)
and [GrafPort reference](https://dev.os9.ca/techpubs/mac/QuickDraw/QuickDraw-27.html),
with packed offsets checked against MPW Quickdraw.h.


## InitGraf and real drawing ABI

InitGraf receives the address of thePort, validates its 206-byte QDGlobals
storage, initializes patterns/cursor data, seed 1 and screenBits, and leaves
thePort nil. The 68K adapter also supplies the A5 anchor address; initialization
writes the globals pointer through it after validating both destinations. PPC
uses the same guest storage contract without the A5 anchor. Both MPW fixture
builds own their QDGlobals; no invented InterfaceLib data symbol is exported.

The default screen policy reserves packed pixels for a 512x342 monochrome
screen. Before execution, `LoadedApplication::with_display` can select a bounded
8-bit main display instead, installing its real storage, 256-entry host RGB332
table and matching owned GDevice/PixMap. It consumes application preparation,
so failed selection cannot publish a half-configured runtime. This is an
explicit host palette policy, not a ROM/default Macintosh CLUT claim. Display
selection after instructions, ports, a device or GPU exists is rejected.
No GPU is acquired during loading or initialization. An embedding host
must supply a Graphics target before PaintRect. The file CLI does so when
`--graphics` is selected, preserving GPU-free execution for CPU-only programs.
Different bitmap sizes share the device, queue, pipeline and error state but
use a temporary texture. This correctness-first path recreates that texture
on each differently sized draw. Missing GPU setup produces an explicit error.
The graphics texture is a drawing result, not yet a composed display: drawing
to an equal-sized offscreen bitmap can replace its contents. `Runtime::refresh_screen` explicitly refreshes from fixed screen memory and
is independent of the current port and application-modifiable screenBits copy.
It accepts an immutable guest address space and only updates the GPU texture.
File-mode graphics runs call it after execution. The shared winit host also
calls it before presenting a loaded application.

Real adapters currently expose InitGraf, OpenPort, InitPort, ClosePort, SetPort, GetPort,
SetPortBits, ClipRect, PaintRect, PenMode and PenPat. 68K Pascal parameters are
popped at the trap boundary; PPC parameters come through CFM registers. The
shared services own globals, heap regions, clipping and GPU drawing. A compiled
application exercises resources, port switching, patterns, direct bitmap writes
and event consumption on both architectures, with exact expected RGBA readback.

Initialization reference: Apple's [InitGraf](https://dev.os9.ca/techpubs/mac/QuickDraw/QuickDraw-30.html).
Cursor appearance and broader native QuickDraw behavior have not been compared
against a native Macintosh oracle; the current acceptance is the documented
record/service subset and explicit test pixels.


## Loaded applications in the window host

`--code` and `--pef` accept `--window`, using the configured guest display and
the existing nearest/aspect-preserving surface pipeline. `--display WIDTHxHEIGHTx8`
selects actual indexed storage before execution; without it the display remains
512x342 monochrome. GPU extent and input mapping follow guest display dimensions,
independently of host window size. Each redraw runs
at most 10,000 CPU instruction boundaries under one total application budget.
WaitNextEvent returns to the host without spinning; elapsed host time adds
60 Hz ticks before the next slice, alongside the runtime's existing deterministic
instruction-derived ticks. This is a host pacing policy, not calibrated Macintosh
CPU timing. Zero-sized windows suspend drawing/execution until restored.

After guest exit the host retains the final screen until close. Surface retries
and application execution share the existing paced redraw path. The synthetic
fixture's palette-click demonstration is disabled for loaded applications;
host left-button pointer input now feeds the shared OS queue. Keyboard
events and Caps Lock remain pending. Tests verify scheduler budgets,
waiting and terminal behavior without a GPU. Bounded surface smoke must still
prove actual presentation on an available desktop; guest completion alone does
not count as a successful window smoke.


## Host mouse input

The host maps physical window coordinates through the same centered aspect fit
as presentation. Coordinates are clamped to the guest screen; presses in the
letterbox or outside the window are ignored. Releases after a guest press are
retained outside the viewport so the guest button cannot remain stuck. Focus
loss releases a held button and clears modifier state. Resizing invalidates
press eligibility until another motion event supplies current coordinates.

Left-button transitions post mouseDown/mouseUp through `events::post`, carrying
the guest clock, global guest coordinates and modifiers. Duplicate transitions
do not post twice; system-mask suppression still updates physical button state.
Command, Shift, Option and Control map from host modifiers. Caps Lock, keyboard
translation and complete Window Manager input dispatch are not implemented.
Monochrome cursor imagery is now a shared process value with GPU presentation
composition; actual onscreen pointer behavior remains unverified.
These records follow Apple's [Event Record](https://dev.os9.ca/techpubs/mac/Toolbox/Toolbox-45.html)
and [low-level event handling](https://dev.os9.ca/techpubs/mac/Toolbox/Toolbox-40.html).


ClosePort releases the visible and clipping region handles through the shared
process heap. It preserves the caller-owned port record, bitmap and current-port
slot. Both handles are validated before either is disposed; malformed, aliased
or unowned handles fault explicitly. Color ports remain unsupported. The 68K
A87D Pascal trap and InterfaceLib ClosePort import share this implementation.
The contract follows Apple's *Imaging With QuickDraw*, Basic QuickDraw, page
2-40 ([manual](https://vintageapple.org/inside_r/pdf/Imaging_With_QuickDraw_1994.pdf));
the trap word is checked against MPW Quickdraw.h. Freed-field contents and other
undefined use-after-close behavior are not claimed to reproduce native systems.


InitPort (68K A86D / InterfaceLib) resets an existing basic port and makes it
current, sharing OpenPort's defaults implementation without allocating new
regions. Existing visible/clip handles retain identity and their rectangular
contents are reset. Port, globals slot and both region destinations are checked
before mutation. The current subset requires owned ten-byte regions; resetting
complex regions awaits handle resizing. Color-port initialization is unsupported.
This follows *Imaging With QuickDraw*, pages 2-39–2-40 and MPW Quickdraw.h.


SetHandleSize shares one process implementation for 68K A024 (A0 handle, D0
size/result) and PPC InterfaceLib (r3/r4, void result plus MemErr). Shrinking
releases the unused aligned extent; growth first consumes adjacent free space,
then tries a new block while retaining the old allocation until success. The
preserved prefix is copied and the guest master pointer is updated before old
storage is reclaimed. Newly exposed bytes are unspecified. Failure preserves
size, master pointer and data; nil/disposed handles report Memory Manager errors.
Zero logical size retains a valid handle. No compaction or grow-zone callbacks
are attempted. See Apple's [Memory Manager, pages 2-40–2-41](https://dev.os9.ca/techpubs/mac/pdf/Memory/Memory_Manager.pdf).


## Shared region storage

The regions module implements NewRgn, DisposeRgn, RectRgn, CopyRgn, GetClip and
SetClip through shared process handles. Empty regions are ten-byte records with
zero bounds. RectRgn normalizes empty/inverted rectangles to that form. Copying
snapshots the complete declared payload before resizing its destination, which
preserves handle identity and supports self-copy. Source sizes must be even,
10–32766 bytes and fit the owned allocation. Payload geometry is opaque here;
these copying services do not imply that complex-region rasterization works.

GetClip and SetClip copy region bytes rather than exchange port handles. The
68K Pascal stack and PPC register adapters use the same implementation. Drawing now decodes nonrectangular records for GPU clipping. These storage
contracts follow Apple's [region routines](https://dev.os9.ca/techpubs/mac/QuickDraw/QuickDraw-131.html)
and MPW Quickdraw.h. Region Boolean construction is implemented as described below.


## Bounded region geometry

A separate decoder expands inversion-point rows into nonoverlapping half-open
rectangles. Horizontal endpoints toggle the active spans at each vertical change;
32767 terminates each row and the whole stream. Ten-byte records use the bounding
rectangle directly. Decoding checks lengths, ordered coordinates, even endpoint
counts, closure, exact bounding boxes and a maximum of 65,536 output rectangles.
The expansion cap can reject unusually intricate otherwise valid input; it is
an explicit runtime bound, not a native QuickDraw compatibility claim.

PtInRgn and EmptyRgn use this decoder through both real ABIs. Signed points and
half-open right/bottom boundaries are retained. GPU clipping uses the same decoder. Apple's public Region documentation describes the payload as
proprietary; inversion-point interpretation follows the first-hand published
analysis [ConvertPict2Rgn](https://preserve.mactech.com/articles/mactech/Vol.05/05.06/ConvertPict2Rgn/index.html).
Hand-built donut/disjoint-region tests verify geometry independently; they are
not a native Macintosh comparison. MPW programs verify rectangular hit-testing
and empty-region results through actual trap/import calls.


## Complex regions in the GPU drawing path

PaintRect intersects decoded clip and visible geometry with the port rectangle;
offscreen bitmaps retain the existing rule that visibility does not clip them.
Geometry intersection is CPU work, with at most one million rectangle pairs.
The bitmap boundary translates geometry into bitmap-local coordinates alongside
pattern phase. The compute shader tests coverage and performs the transfer once
per pixel in a single dispatch, so overlapping coverage cannot apply XOR twice.
No CPU pixel drawing path is introduced.

Clipping accepts at most 65,536 rectangles and at most 32 million pixel/rectangle
checks per draw. Exceeding either limit fails before guest pixels change. These
explicit limits bound GPU work while the initial linear geometry scan is in use.
Malformed geometry also fails before pixel publication. Guest memory remains
canonical, with readback/repacking after each draw as before. Native-system
pixel comparison is still outstanding; acceptance uses independent expected
pixel maps and compiled programs constructing complex regions through QuickDraw Boolean APIs.


## Region Boolean construction

SectRgn, UnionRgn, DiffRgn and XorRgn snapshot both input payloads before writing
the destination, allowing it to alias either input. Decoded geometry is swept
in vertical bands; horizontal endpoint transitions select the requested Boolean
set. Encoding cancels shared inversion boundaries, emits zero-bounds empty
regions, and reduces rectangular results to ten-byte records. Resizing and
master-pointer updates use the shared process heap.

Work is bounded to two million band/rectangle visits and 65,536 output spans;
encoded results must fit 32,766 bytes. Complex results requiring the reserved
32767 coordinate fail explicitly. No destination is mutated until geometry and
encoding succeed. These are deliberate implementation bounds, not claims about
native system limits. Contracts follow Apple's [DiffRgn](https://dev.os9.ca/techpubs/mac/QuickDraw/QuickDraw-144.html)
and [UnionRgn](https://dev.os9.ca/techpubs/mac/QuickDraw/QuickDraw-143.html) references
and MPW trap declarations. CPU work constructs geometry; pixel drawing remains
in the GPU pipeline.


### Handle locks and executable heap RAM

HLock/HUnlock share owned-handle lock metadata across the register-based 68K and
PPC CFM adapters. Locking is idempotent. SetHandleSize preserves a locked block's
address, permits in-place resize and returns memFullErr when relocation would be
needed. Explicit unlock permits relocation again. Disposal remains allowed and
resource loading applies resLocked. Purging, compaction, empty handles and
HGetState/HSetState remain pending. Contracts: Apple's
[Memory Manager](https://dev.os9.ca/techpubs/mac/pdf/Memory/Memory_Manager.pdf),
pages 2-40–2-46.

The shared application heap is read/write/execute: classic applications execute
resource-loaded routines and generated code from RAM. Both engines fetch the
canonical bytes, including rewrites; heap mapping bounds still apply. PEF section
permissions remain independent. StripAddress implements the documented 32-bit
startup-mode behavior, preserving all address bits without dereferencing the
pointer. 24-bit startup/address translation is unsupported. See
[Memory Management Utilities](https://dev.os9.ca/techpubs/mac/Memory/Memory-181.html)
for heap-code execution and StripAddress semantics.


### Sound Driver volume state

Legacy volume is the low three bits of the canonical guest SdVolume byte ($0260),
identified by Apple's MPW LowMem.h LMGetSdVolume inline instruction. SetSoundVol
and GetSoundVol PPC imports share that state with the 68K immediate/synchronous
Control(-4, 2) path, whose csParam contains an inline volume word. Changes preserve
unrelated high bits; getters observe direct guest writes. Control validates its
result destination before changing volume and writes noErr to ioResult.

This slice supports levels 0–7 and faults explicitly for other levels, drivers,
control selectors and asynchronous requests. It implements observable volume
state only: synthesizers, sample playback, channels, completion callbacks and host
audio output remain unfinished. Startup initializes low memory to zero, including
volume. No host device settings are modified. Contracts: Apple’s Inside Macintosh
II, Sound Driver, pp. 232–236 (the GetSoundVol/SdVolume relationship), and the
[Device Manager parameter layout](https://dev.os9.ca/techpubs/mac/Devices/Devices-52.html).
The original Centaurian glue independently establishes the -4/2/inline-word call.


### Basic port coordinate conversion

LocalToGlobal/GlobalToLocal use the current guest GrafPort bitmap bounds, not
portRect or cached host window coordinates. Local-to-global subtracts the bounds'
top/left; the reverse adds them. Points are copied before mutation and written as
one checked four-byte value. Coordinates use signed-word wrapping arithmetic;
native overflow comparison remains pending. Color GrafPorts are explicitly
unsupported. SetOrigin, MovePortTo and the implemented Window Manager placement
operations share this guest coordinate model.

The 68K Pascal pointer argument and PPC InterfaceLib argument both invoke Ports
conversion. MPW programs verify a nonzero origin, the inverse operation and a
port switch before continuing their exact drawing checks. Geometry tests cover
negative origins, signed-word edges, direct guest origin changes and invalid
outputs without partial writes. Source: Apple's
[Basic QuickDraw coordinate model](https://dev.os9.ca/techpubs/mac/QuickDraw/QuickDraw-19.html)
and [coordinate conversion usage](https://dev.os9.ca/techpubs/mac/QuickDraw/QuickDraw-20.html).


### Process-owned CFM symbol queries

`Process::fragments` owns connection identity and immutable export records. PEF
startup installs the application connection before its initializer executes;
the System 7 initialization block carries that actual ID. Load metadata and
connection state share the same immutable export allocation through Arc.

FindSymbol, CountSymbols and GetIndSymbol decode into typed requests in `abi`
and execute in `cfm`, independently of the current CPU. PPC imports and classic
68K $AA5A selectors 5–7 use the same table. Outputs retain 32-bit guest addresses,
a one-byte symbol class, and Pascal byte names. All selected outputs are checked
before writing; optional pointers follow Apple's MPW CodeFragments.h. Unknown
connections/symbols return -2801/-2802 without changing outputs. Unsupported
selectors and a requested Str255 conversion of a longer PEF name fault explicitly.

GetIndSymbol uses zero-based indexing, following the later Apple Code Fragment
Manager reference. The 1994 Inside Macintosh manual instead documents one-based
indexing; native comparison remains outstanding. Compiler-generated MPW calls
verify ABI behavior, not that historical semantic discrepancy. Dynamic PPC
connections extend this table as described below; real Mixed Mode remains unfinished.

References: [CFM API reference](https://leopard-adc.pepas.com/documentation/Carbon/Reference/Code_Fragment_Manager/DeprecationAppendix/AppendixADeprecatedAPI.html),
[older CFM usage](https://leopard-adc.pepas.com/documentation/mac/PPCSoftware/PPCSoftware-42.html),
and the pinned MPW toolchain's `Interfaces/CIncludes/CodeFragments.h`.


### Dynamic memory fragments in the existing runtime

GetMemFragment prepares PEF sections/imports/exports in staging memory, then adopts
the mappings into the existing guest address space. It does not create a runtime
or replace resources, files, heap, events or graphics. Main is retained as a
symbol address: a library can expose data or omit main, while application launch
still validates a callable transition vector. First-use StdCLib storage is
process-owned and survives fragment unload; later imports never reset it.

Connections track source pointer/length identity, monotonic IDs, references,
initializing/ready/closing state, and owned mappings/gates. Reference, find and
private-copy options (1/2/5) are supported. Fourteen explicit reservations start
at $80000000 with $08000000 spacing; each admits up to 64 MiB of section storage.
Per-slot gates/vectors/init blocks occupy 64 KiB at $75000000 + slot*$10000.
Reservations check live memory and intercepted import gates. Exhaustion returns
-2810; closing frees a slot without recycling its connection ID. Other options,
including in-place loading, fault explicitly. Guest library dependency binding,
GetSharedLibrary and disk-based CFM loading remain unsupported.

PPC init/term calls suspend the complete caller CPU and use a separate 64-byte
linkage/argument frame below its stack pointer. They execute through the normal
dispatcher and instruction budget, including nested loads. The initializer sees
its live connection ID, closure ID, Pascal name and original memory locator.
Success publishes connection/main outputs only after initialization returns.
Nonzero initializer results return -2821 and remove the failed fragment's
mappings, gates and connection, without running its terminator. Arbitrary guest
side effects (including explicitly nested loads) are not transactional. CPU
faults retain the existing terminal-fault policy, rather than executing cleanup.

CloseConnection decrements references, clears its ID output on success, and
executes the final terminator before removing owned mappings and gates. After
application lifecycle completion, remaining dynamic roots terminate in reverse
creation order under the same instruction budget, regardless of reference count.
Dependency-aware ordering is not yet implemented. Exit during dynamic init/term
is an explicit fault. Callers retain responsibility for locking resource
containers until close. Loading/unloading PPC fragments while executing 68K
faults explicitly until real Mixed Mode transitions are implemented; symbol
queries continue to work through both adapters.

The MPW dynamic fixture loads a locked resource plugin, queries and calls its
exported function, observes a terminator's write to application storage, closes
and releases the resource, then posts its success marker. This proves the
resource-to-CFM path in one runtime, not broad original-application acceptance.


## Rounded-rectangle geometry

`rounded` prepares bounded, nonoverlapping scanline coverage for framed and
painted rounded rectangles. `quickdraw` intersects it with the same live guest
port/region state used by PaintRect, and `bitmap` submits a patterned fill and
coverage to wgpu. Vertical runs are coalesced to reduce GPU clip work. Frames
subtract the inset rounded shape, reading pnSize and pnVis from guest memory;
pen patterns, modes and colors remain shared. Geometry uses wide fixed-point
integer intermediates and accepts classic signed-word coordinates. The runtime
has no dependency on the legacy comparison harness in `fixtures`.


## Stable pointers and window ownership

Nonrelocatable pointers share the process heap's free extents with handles.
A separate ownership table records logical pointer sizes, so DisposePtr and
SetPtrSize cannot operate on handle masters or handle data. Resizing uses only
adjacent free space and never relocates the block. Zero-sized blocks retain a
minimum aligned extent and a unique live address. Disposal returns the extent
to the same coalescing allocator. System-heap variants remain unsupported.

This is the storage prerequisite for Window Manager records: caller-provided
wStorage must remain caller-owned, while nil wStorage requires a stable runtime
allocation whose disposal is tied to the window lifecycle. Window creation must
also initialize its embedded GrafPort, resource/title ownership, ordered window
list and global structure/content/update regions before publishing the window.
Visibility must update drawing clips and exposure events across all windows.
The legacy NewWindow trap documents an eight-argument ABI (26 Pascal bytes,
then a long result; PPC r3–r10), including Boolean values in the high byte of
68K stack words. These are continuation requirements, not implemented services.


## Shared basic Window Manager

`windows` owns only allocation/lifecycle metadata. Guest WindowList links,
WindowRecord fields and region handles supply ordering, coordinates, visibility
and drawing state. The first implemented definition is plainDBox. Both ABIs
marshal into typed requests and invoke the same manager. Runtime temporarily
moves the manager out of Process to borrow shared heap/ports, then restores it
even when an operation returns an error. Window storage is nonrelocatable;
regions and titles are handles. CloseWindow releases internal handles, and
DisposeWindow also releases a live NewPtr record. Static caller storage uses
CloseWindow and can be initialized again.

The manager's screen port provides temporary drawing context for global frame,
background and desktop coverage. Window content ports refer to the same guest
screen pixels, with bitmap bounds shifted to local coordinates. CPU region
Boolean operations produce visibility/exposure coverage; wgpu performs all
pixel operations. Closing queues exposure geometrically in updateRgn; Toolbox
update-event delivery and BeginUpdate/EndUpdate remain the next integration.
Builtin plain windows currently have nil windowDefProc; real guest WDEF calls
belong to the Mixed Mode/callback work, and other styles fail explicitly.


## Font strike boundary

`font` parses a borrowed immutable snapshot of guest FONT/NFNT bytes and exposes
metrics, glyph geometry and one-bit glyph masks. `text` resolves the selected
old-style FONT through the same resource handles used by both CPUs, so guest
writes affect subsequent metrics. No mutable global font registry or duplicated
architecture font state exists. Mask extraction only decodes source storage;
wgpu glyph composition remains the next drawing boundary. Default font
initialization, NFNT/FOND association selection, optional tables, color fonts,
styles/scaling and outline fallbacks are explicit outstanding work.


## Font initialization and fallback ownership

The shared Font Manager initializes a guest system strike from an application
FONT resource or a licensed outline substitute. Fallback generation on the CPU
produces canonical packed source-font data; it does not render destination pixels.
The process stores only fallback handle identities and initialization metadata.
Default family/size selection reads guest low-memory globals on every metrics call,
and the current strike is reparsed from live handle bytes. Cached generated strikes
are bounded by count and supported point sizes. Resource priority, fallback
approximation, thresholding, licenses and rasterization costs are explicit.
Application text composition through wgpu remains the next shared drawing service.


## Shared text drawing

Text measurement and drawing snapshot the selected guest strike and input bytes.
CPU work decodes source masks and constructs disjoint ink/background coverage;
wgpu applies all destination pixel operations. Coverage includes glyph bearings
and the advance cell, enabling opaque blank-space erasure and source transfer
modes. Temporary pen pattern/mode changes reuse the same GrafPort clipping/color
pipeline and are restored even on failure. Measurement preserves pnLoc, while
successful drawing publishes the advanced horizontal word. Geometry preflight is
bounded; GPU errors remain terminal and can retain earlier completed glyph pixels.
Bold face uses checked one-pixel rightward dilation of a source FONT strike and
one-pixel advance growth, shared by measurement and drawing. Original font bytes,
missing-character mapping and vertical metrics are preserved; no CPU destination
renderer is introduced. Other styles, fractional spacing and DrawText/StdText
remain explicit gaps. Native styled-font pixel parity is unverified.


## TextEdit record ownership

`textedit` creates and owns guest TEHandles and their text/style graphs through
the shared heap. Current records and style fields are read from guest memory,
including after relocation. Host metadata records allocation ownership and the
constructor's record kind, preventing corrupt or foreign links from freeing
unrelated blocks. Constructors stage allocations and roll back on exhaustion.
Disposal validates the entire graph before mutation. Fonts supply actual initial
metrics; buffer drawing is available for the next layout/redraw integration.
Native 68K/PPC adapters marshal the same typed operations. Editing, style/handle
adoption and callback defaults remain explicit gaps in this first lifecycle slice.


## TextEdit grouped updates

Replacement/layout read captured guest attributes and prepare immutable payloads
for the text, record and style/line handles. The shared heap stages every resize
against cloned allocator metadata, respecting locked blocks and existing master
identities. It validates writes and commits only after all allocations succeed.
This prevents partial TextEdit updates on heap exhaustion or locked-record growth.
Word wrapping and line-table construction use decoded shared strike advances;
redraw will consume the same guest tables rather than a cached host layout.
Only one style run is currently supported, with explicit limits/callback failures.


## TextEdit redraw boundary

Redraw reads guest line/style tables and freezes text/font source bytes for the
update. It intersects update/view/caller clips with drawable port/bitmap bounds
before selecting visible lines, then bounds total visible glyph work. Temporary
port fields and clip ownership are restored even on failure. The shared glyph
renderer accepts a decoded source strike so drawing cannot observe its own pixel
writes through aliased font/text sources. wgpu performs destination transfers;
CPU work constructs geometry and snapshots source storage. Inactive plain and
single-style records are the current scope; selection/caret and dispatch callbacks
remain explicit outstanding integration.


Inactive deletion uses the grouped layout transaction to include selection
collapse with text and line/style changes. After publication it clears the view
with the port background pattern and redraws through the existing clipped GPU
route. Unsupported active editing/layout callbacks fail before publication;
rendering faults after publication are terminal and retain the committed edit.
Transient port state is restored on rendering failure. Scrap remains independent
of TEDelete. Active selection/caret interaction is still a separate missing path.


Active TextEdit redraw clears the clipped update area, draws the source glyphs,
and applies monochrome XOR selection/caret overlays using wgpu. This guarantees
repeatable active updates without maintaining a host-side overlay pixel cache.
Geometry derives from live guest line/style tables and font advances; port state
and temporary clip handles follow the existing restoration contract. Selection
and activation publish guest state before redraw, so GPU faults remain terminal
with that state retained. TEIdle uses the shared guest tick count and live
CaretTime low-memory value; it does not sample host wall time. Direct selRect/
selPoint synchronization and application callback hooks remain pending.


TEKey snapshots live text and normalizes selection only at the edit boundary.
Insertion/deletion publishes text, caret and line/style changes in the same
grouped heap transaction; failed allocation does not draw or partially edit.
Left/right movement uses the shared selection route. Successful changes redraw
through wgpu, with rendering faults remaining terminal after publication.
Guest event-to-TEKey acceptance uses the same queue and services on both CPUs.
Host keyboard routing and vertical/click navigation are still incomplete.


## Menu state and screen composition

The shared Menu Manager retains allocation identities, with DynamicMenuList and
MCEntry payloads canonical in guest memory. Color resources are read through
the same Resource Manager handles visible to the application. List/color and
desktop-region replacement uses the grouped heap transaction before publishing
low-memory globals. Menu snapshots belong to the caller and share menu references.
The empty bar draws through the GPU screen-bitmap boundary independently of the
application's current port/clip; its pixels never use a legacy CPU renderer.
Populated drawing, custom definitions and color modes outside the current
monochrome screen policy are explicit missing routes. No menu callback shim or
silent success placeholder is installed. Menu resource expansion and tracking
remain future shared services, with callbacks belonging to the Mixed Mode work.


## Cursor composition boundary

SetCursor transfers a 68-byte guest record by value into process state. Both
CPU adapters use that same image, hide depth and obscuring state; no cursor
register logic enters shared services. InitCursor reads the guest QuickDraw
arrow prototype, while GetCursor uses canonical Resource Manager handles.
Presentation snapshots the packed screen into the existing index upload boundary
and applies cursor mask/data only to the WGSL texture result. Indexed storage
and guest bitmap memory retain their underlying pixels. Cursor coordinates are
signed guest coordinates minus signed hotspots, bounded before upload.
Host canvas ownership controls native pointer visibility without changing the
guest's requested image or hide level. Direct cursor low-memory compatibility,
color/animated cursors and shielding remain missing paths. The windowed smoke
could not complete presentation/resize; offscreen tests do not prove onscreen
behavior.


## Dialog startup and parameter ownership

The shared Dialog Manager retains only owned parameter-handle identities.
DAStrings and their Pascal payloads are authoritative guest memory, read by
both service accessors and compiled SDK glue. ParamText freezes all source
inputs before grouped handle replacement; GetParamText freezes source values
and validates all outputs before writing. Reset validates globals/ownership,
clears the public slots, and releases its heap graph. Font, alert-stage, resume
and sound globals have one guest representation. Procedure values are stored
without a callback shim; invocation remains part of the real Mixed Mode work.
NIL DABeeper records the explicit legacy no-sound limit, not an implemented
standard alert sound. Dialog/window/control construction remains incomplete.

BlockMove/BlockMoveData snapshots source bytes before writing overlapping
ranges, checks mappings/permissions before mutation, and observes the image
execution bound. Both CPUs execute from canonical writable code storage; there
is no private instruction-byte cache to invalidate. Bitmap memory writes use
the existing GPU synchronization boundary on subsequent drawing/presentation.


## Environment profile and application heap reservations

SysEnvirons describes one virtual Macintosh compatibility world rather than
branching on the calling CPU. Its legacy processor field denotes the 68020
compatibility backend; the native PPC engine retains its own execution state
and semantics. Public capability flags describe implemented Macintosh services,
so the default monochrome profile does not advertise Color QuickDraw. The real
indexed main-display profile sets SysEnvirons.hasColorQD after shared screen,
window, palette, menu-restoration and cursor acceptance; unsupported operations
still fail explicitly. This is a bounded implementation, not full Color QuickDraw
compatibility or a 32-Bit QuickDraw Gestalt version claim. Unknown
machine/keyboard values avoid pretending to emulate a particular hardware model.
Version errors/layout follow Apple's Gestalt Manager reference and OV16.

Loaded PEF processes reserve the default 4 MiB heap at $10000000. CODE
preparation configures the same Process allocator from $00010000 to the start
of its A5 world, before any guest instruction or owned allocation exists. The
unused high mapping is removed; live pointers are never relocated. Allocation,
resize preflight and staged handle replacement validate the configured mapping
and capacity. This is a loader-selected application layout, not separate CPU
memory-manager implementations; either CPU can operate on a Process's heap.

SetApplLimit stores a checked absolute address in ApplLimit ($130), with native
D0/MemErr errors and a void PPC setter. GetApplLimit reads the guest global and
rejects addresses outside the reservation. The current allocator eagerly grows
the zone to the reservation. Per IM: Memory pp. 2-84–2-85, lowering a growth limit
does not shrink an already-grown zone, invalidate blocks, or reclaim stack
space. MaxApplZone preserves the existing extent and the requested limit. Lazy
zone growth, guest zone headers, multiple zones, alternate stack placement and
partition sizing remain unfinished. Standalone Process fixtures retain lazy
mapping of the default reservation.

Window update notifications derive from guest WindowList, visible-region and
update-region storage. The shared runtime prioritizes eligible OS queue events
then synthesizes updates for Toolbox event receivers, including WaitNextEvent.
Delivery does not dequeue dirty geometry. BeginUpdate saves the old visible
region in an owned guest handle, narrows it to dirty local coordinates and clears
updateRgn atomically through grouped handle replacement. EndUpdate restores it
without clearing new invalidation. Host metadata retains allocation identities
only. Nested brackets and window creation/close/dispose while updating are
explicit faults. Activation events and menu-bar invalidation servicing remain
unimplemented; this is a partial Toolbox event contract.

Rectangle mutations are shared signed 16-bit guest-memory services. Offset and
inset wrap arithmetic; inset inversion follows legacy Systemless evidence, not
a verified native System 7 contract. EqualRgn compares geometry rather than raw
region storage.

The File Manager catalog projects the same loaded application and fork bytes as
FCB queries. Virtual volume -1 (drive 1/default 0) contains root node 2 and
application node 16. Synchronous PBGetCatInfo decodes named/indexed/directory-ID
queries without host paths; metadata includes fork lengths, 4 KiB allocation
blocks, open-fork flags and parent identities. Error results and ioCompletion
are written through checked guest memory. Successful parameter/name outputs are
validated together before either changes. Finder metadata/dates currently have
deterministic zero defaults; imported metadata, MacRoman name folding, general
hierarchical paths, writable catalogs and asynchronous completion are unfinished.

PBDirCreateSync/native HFSDispatch selector 6 adds process-local directories to
the same File Manager catalog as the application. Stable node IDs start at 17;
the root remains 2 and application 16. Created directories retain name/parent/
creation date and are visible to named and directory-ID catalog queries. Root
indexed enumeration includes its direct children; child counts derive from the
live tree. Colon paths support the virtual volume name, relative prefixes,
trailing directory separators and parent traversal without escaping the root.
The combined 1024-created-node limit returns dirFulErr; duplicate names return dupFNErr.
Guest output is validated before inserting or allocating an ID. Dates come from
the shared low-memory Time value ($20C), initialized to the loader/host calendar
epoch and advanced by the shared clock. Parent modification dates are not yet
maintained.
Directories and newly created files use shared process state with optional host
volume backing; application bytes remain read-only. File deletion and working
directory references now share that catalog. MacRoman folding and complete host
durability/error integration remain unfinished.

PBGetFInfoSync and PBHGetFInfoSync share the catalog's application metadata
projection, with their own 80-byte guest parameter layout. Hierarchical lookup
uses ioDirID for parent/indexed queries; flat lookup selects the root. Queries
return first open reference, open-fork/locked attributes, logical/physical sizes
and file ID. Named closed-file queries preserve caller pathnames; indexed/open
queries return the leaf name. Parameter and optional name output are validated
together. These now query loaded and guest-created files. New files retain Finder
metadata and dates; imported application metadata still has deterministic defaults.

GetNewWindow decodes the live guest-loaded WIND handle through the shared
Resource Manager and calls the same creator as NewWindow. Bounds, definition
ID, visibility, close flag, refCon and Pascal title remain guest data. Missing
or truncated templates return NIL; nonzero optional positioning specifications
and unsupported/custom definition IDs are explicit faults. Caller storage and
allocated storage retain the existing WindowRecord lifetime rules.

Built-in definitions now cover plainDBox (2), dBoxProc (1) and noGrowDocProc (4).
The immutable built-in definition identity is retained with window registration;
geometry, title, visible/highlight flags and update state remain guest records
and handles. Modal double borders and document title/close/stripe/shadow chrome
use region union and GPU QuickDraw transfers. Titles reuse the shared font
strike/text path; font fallback is not claimed pixel-identical to native Chicago.
Manager port and clip state are restored after chrome drawing. Visible document
chrome is repainted when creation/closure changes highlighting, respecting
z-order occlusion. Activation events, mouse tracking, dragging, resizing, zoom,
custom WDEF execution, AuxWin records and resource positioning remain unfinished.

DetachResource severs the Resource Manager cache association after validating
the owned master pointer. It preserves master/data addresses, payload, size,
lock and allocator ownership; the caller now owns an ordinary handle. Later
lookup allocates an independent handle from original fork bytes. No clone or
allocation is required because resource payloads were never shared between
masters. resChanged blocks detachment with resAttrErr; missing/ordinary/already
detached handles return resNotFound. MemErr is preserved and ResErr is projected
through low-memory state on both CPU service paths. Resource-file close,
AddResource, mutable resource maps and purge state APIs remain unfinished.

The shared Control Manager registers owned ControlRecord handles with immutable
owner/standard definition IDs. Fields, title, bounds, value/range, visibility,
highlight and next links live in guest RAM, starting at WindowRecord+140. Native
68K and checked PPC import adapters marshal NewControl, getters/setters, drawing,
visibility, hit testing and disposal into this service. PPC import decoding can
now fail before returning to LR; NewControl's ninth argument comes from r1+56.

Button (0), checkbox (1) and radio (2) definitions draw through GPU QuickDraw,
with bounded rounded/ellipse geometry and shared font strikes. Drawing restores
the owner's port/clip and the previous current port. Titles support multiple
lines, initial width truncation and clipping; inactive ink is dithered. Region
visibility prevents drawing into covered/invisible owners. DisposeControl
unlinks/erases/frees; window close validates the full control list before freeing
controls, preventing cycles, foreign handles and orphan registration from
destroying the owner. Built-in definition identity is currently host dispatch
metadata: contrlDefProc is NIL, not a callable CDEF handle, and AuxCtlRec is
unimplemented. Custom definitions/data, tracking/action callbacks, scroll bars,
CNTL loading and limit setters remain gaps.

SizeWindow and ShowHide use a shared staged geometry/exposure operation. All
affected visibility/update regions plus resized content/structure handles are
validated and replaced together before record fields change. Retained global
visible content keeps its guest bitmap pixels; frame and newly exposed damage
are repainted through wgpu. SizeWindow adds new content geometry to updateRgn
only when fUpdate is true, intersects prior invalidation with the resized content
and preserves clipRgn/local origin/current port. Other exposed windows receive
update regions independently of fUpdate.

ShowHide updates visibility and queues newly exposed content while preserving
WindowList order and highlighting. It has no activation semantics. ShowWindow, HideWindow and SelectWindow layer
selection and activation onto this primitive. Controls retain their records
and local coordinates; application-driven DrawControls observes the changed
visibility/port bounds. Geometry changes during BeginUpdate and nonpositive
nonzero dimensions are explicit faults; a zero/zero size request is a no-op.
Window reference getters/setters access live refCon memory without graphics or
MemErr changes.

MoveWindow preserves local port coordinates, shifts bitmap bounds and global
content/structure/update regions, and stages all affected region replacements.
Visible retained pixels come from an immutable snapshot of canonical guest
bitmap memory: CPU unpacking supplies indices, while wgpu performs the clipped
copy after exposure repair. Overlapping moves cannot read already overwritten
destination pixels. Uncopiable newly visible content receives update invalidation.

SelectWindow and fronting moves publish checked list links and hilite fields.
CurDeactive ($A68) and CurActivate ($A64) are guest-owned, coalescing pending
activation slots. Toolbox event delivery peeks or consumes deactivation before
activation; OS-only retrieval leaves these slots alone. Closing clears slots
that name the removed window. Hidden front creation preserves existing activation,
matching the legacy Systemless comparison; no new native System 7 test was run.
InvalRect/ValidRect translate current-window local rectangles to global content
and update the shared update region, including during BeginUpdate. Custom window
definitions, dragging/growing/zooming and native presentation remain unfinished.

SystemTask ($A9B4 / InterfaceLib) is an explicit shared service boundary. The
current runtime has no registered desk accessories or periodic device driver
chains, so the dispatch has no work and preserves CPU results, event queues and
guest memory. It does not introduce an extra clock step: instruction-boundary
and host tick progression retain ownership of time. Loading such drivers or
accessories will require real periodic callback dispatch, not this empty case.
Legacy Systemless provides the comparison contract; native System 7 was not run.

PenSize writes the signed vertical/horizontal words together at GrafPort +52;
68K decodes height then width on its Pascal stack, PPC decodes width/height in
r3/r4. Existing wgpu frame coverage reads these live dimensions. Either zero pen dimension
suppresses frame coverage; negative dimensions remain rejected by drawing.
RectInRgn uses positive-area overlap against decoded region spans, including
holes/disjoint rows, rather than region bounding boxes. Both adapters share it;
the 68K Boolean writes one byte and preserves the caller's padding. Output
permissions are validated before consuming the argument stack.

GetPicture ($A9BC / InterfaceLib) decodes only the resource ID and invokes the
existing shared GetResource service with type PICT. It preserves cache identity,
resource ownership, release/reload behavior and ResErr. Loading exposes the
original bytes without treating them as a validated or drawable picture.

PortSize changes only the current portRect bottom/right, preserving its origin,
bitmap mapping, clip and visibility handles. MovePortTo shifts BitMap bounds so
portRect.topLeft minus bounds.topLeft equals the requested global position,
preserving bitmap dimensions and all local port/region coordinates. These are
GrafPort operations, not Window Manager movement/composition. Both adapters
invoke the same shared operations; all output bytes are checked as one write.
Word-coordinate arithmetic wraps explicitly. Color MovePortTo remains rejected.

PICT 202 in Super Mines is a v1 PackBitsRect bitmap with srcOr. Its loading,
offscreen port setup and bitmap picture drawing now execute. The decoder and
GPU transfer contract are described below. Unknown drawing opcodes fault
explicitly rather than silently disappear.

The picture module now decodes a bounded bitmap subset of PICT v1/v2 before
issuing any drawing. It supports BitsRect/BitsRgn and PackBitsRect/PackBitsRgn
for one-bit BitMaps, clip regions/masks, monochrome foreground/background colors,
comments and v2 header metadata. The declared picSize is advisory; owned handle
size bounds all reads. Input and expanded command storage are capped at 16 MiB,
bitmap axes/source spans at 4096, and stream traversal at 4096 opcodes. Missing
EndPic, malformed PackBits, unsupported PixMap layouts and unknown drawing/state opcodes fail
explicitly. Text/vector picture commands and picture recording remain gaps.

CPU decoding converts compressed/packed storage to immutable source indices.
Graphics::Draw::Blit supplies independent source dimensions/rectangle, source
colors and a transfer operation to wgpu. The compute shader enlarges using integer nearest sampling and reduces
monochrome footprints with OR. It excludes source coordinates outside the bitmap and performs
copy/or/xor/bic plus inverted-source variants. No CPU picture renderer or CPU
scaling path exists. The destination BitMap wrapper retains packed row padding
and synchronizes canonical guest bytes before/after each command.

QuickDraw drawing_target supplies the same portRect, bitmap, visRgn and clipRgn
intersection to pattern drawing and pictures, including offscreen visibility
rules. Picture frame/clip/mask coordinates map to the caller's destination;
port state and region handles remain unchanged. Decode failures occur before
pixel writes; GPU/runtime failures are still per-command errors, not whole-
picture transactions. Generated pixels and legacy source contracts are evidence,
not a claim of native System 7 scaling parity.

CopyBits ($A8EC / InterfaceLib) decodes into one shared Macintosh request
with source/destination BitMaps/rectangles, mode and optional mask handle. 68K
pops 22 Pascal argument bytes; PPC reads six registers. Both procedures preserve
return registers. NIL source/destination parameters are inert, following the
legacy comparison contract. Other malformed descriptors/unsupported modes fault.

For ordinary one-bit BitMaps, source storage is decoded to an immutable snapshot
before destination mutation. Draw::Blit then performs scaled copies and all eight
source transfer modes on the GPU, using live current-port monochrome colors. Monochrome reductions OR their
source footprint on the GPU, following the legacy Basic QuickDraw contract.
Packed destination padding remains intact. When destination storage matches the
current port, port clips and visibility are translated into the destination's
local coordinates; this CopyBits path honors visRgn even for offscreen ports,
unlike the painting offscreen rule. Unrelated destination storage uses its own
bounds and the explicit mask. Complex masks use decoded spans with holes.

Alias layouts with a different row stride from the current port are rejected.
Color PixMaps, tagged/handle descriptors, transparent/extended modes, recording
and custom bitsProc callbacks remain unsupported. Existing bitmap axes/source
span and GPU clip-work limits apply. Legacy source comparison and deterministic
GPU/compiled pixel checks do not establish native System 7 parity.

The shared source-mode/color and destination clipping contract is also described
in Apple's [CopyBits reference](https://dev.os9.ca/techpubs/mac/QuickDraw/QuickDraw-166.html).
The legacy one-bit reduction path references Inside Macintosh V, V-65 for OR
compression; reduction checks distinguish it from nearest sampling.

LineTo ($A891) and relative Line ($A892) use the same live GrafPort pen state
on both CPUs. Signed-word locations, pen dimensions, visibility, pattern and
mode follow the [documented QuickDraw pen model](https://dev.os9.ca/techpubs/mac/QuickDraw/QuickDraw-59.html).
Relative endpoints saturate at signed-word bounds, matching the legacy adapter.
Hidden/zero-dimension pens advance pnLoc without requiring graphics; negative
dimensions and recording/custom procedures are explicit faults. The pnLoc output
is validated before drawing and published after successful GPU completion.

The lines module computes bounded integer endpoint geometry, not pixel coverage.
Draw::Line sends that geometry plus the usual pattern/colors/transfer mode to
wgpu. For each destination pixel, the shader finds the interval of pen positions
that could cover it. Minor coordinates are monotone and step at most one pixel,
so interval endpoints determine coverage without stamping overlapping brushes
repeatedly. Thick patXor lines apply the transfer once per covered pixel.
Unsigned products support the full signed-word endpoint span without GPU int64
or floating point. Clipping and packed bitmap synchronization reuse the existing
QuickDraw target; region work scales with clip complexity, not line length.

Integer nearest placement preserves both endpoints. This replaces the legacy
fixed-point bias/overflow behavior rather than claiming identical diagonal
rounding. The independent test oracle uses floating-point point placement and
brush union only in tests. Axis behavior, pen state and union/transfer contracts
are compared with legacy source/documentation; native diagonal pixel parity
remains unverified. Polygon/region recording and lineProc callbacks remain gaps.

Rectangle drawing adds FrameRect, EraseRect, InvertRect and FillRect to the same
QuickDraw target as PaintRect. FrameRect uses pen pattern/mode and inside-border
geometry; hidden pens or either zero dimension suppress the frame. The same
zero-dimension correction applies to framed rounded rectangles.

Erase uses live bkPat with patCopy; Fill uses the caller's pattern with patCopy;
Invert uses all-one XOR independently of pnPat, pnMode and port colors. These
attributes are selected for the drawing command without temporarily editing pen
state. FillRect additionally writes the documented fillPat field after successful
drawing, with its output permissions validated before any pixels change. Source
patterns and rectangle words are checked guest reads. Other pen/location fields
remain unchanged. [Apple's drawing contracts](https://dev.os9.ca/techpubs/mac/QuickDraw/QuickDraw-59.html)
define the distinct fill/erase/invert semantics. Recording/custom procedures and
color ports remain unsupported; ordinary clipping and packed-row preservation
continue through shared wgpu/BitMap synchronization.

PICT v2 now accepts byte-indexed 8-bit PixMaps with one 8-bit component and
packType 0/1. Inline ColorTables follow explicit ColorSpec indices or sequential
indices when ctFlags bit 15 is set. Entries are bounded to 256, duplicates/out-
of-range values fail, and logical pixels must reference defined entries. Row
padding is storage, not additional pixels; byte/word PackBits lengths and raw
rows are checked before drawing. Serialized pointers are never dereferenced.

BitmapCommand retains original indices and an immutable 16-bit RGB palette.
The GPU receives that palette with the command and matches each source color
to the current monochrome white/black destination by RGB distance, before
transfer/reduction. No CPU color renderer or source-index threshold substitutes
for palette lookup. Packed destination padding and port state remain unchanged,
and edits to guest resource table bytes affect the next draw. This is a bounded
monochrome destination policy, not Color Manager ITable/dithering or color-port
compatibility. Other indexed depths and direct/component packing remain gaps.

Version-2 HiliteColor/DefHilite metadata is retained as picture-local state.
Default is an explicit marker, not a fabricated guest procedure or changed
HiliteRGB global. HiliteMode and unsupported highlight transfer modes still
fault before drawing; supported source modes do not consume highlight state.
Original Super Mines PICT 184 now completes its metadata, 256-entry table,
PackBitsRgn and mask through this path.

## Transient TextBox drawing

TETextBox/TextBox share one signed-length/alignment request across the 14-byte
68K Pascal and four-register PPC adapters. The manager snapshots raw source bytes,
creates a temporary plain TERec/text-handle pair, applies justification, runs the
existing word-wrap layout and redraws with background erasure through wgpu.
Caller clipping and port/pen state survive the draw; empty text erases the view.
The temporary graph is disposed after success or failure, preserving allocation
errors. Input length is bounded to 32767 bytes; unsupported alignment values fail
before drawing. Allocation failure does not erase the existing display. GPU
failure can retain completed pixels, as with other synchronous drawing services.

## Mouse coordinates and rectangle hit testing

GetMouse uses the same signed-word conversion as GlobalToLocal, reading the
current guest port's bitmap origin and the live low-memory Mouse point ($0830).
The 68K Pascal pointer and PPC r3 pointer reach one runtime implementation, with
no GPU requirement. Source/origin bytes are frozen before the result write, and
the host point cache is updated only after a successful write. Host pointer motion
publishes the letterboxed/clamped guest point to Mouse before updating its cache;
nonfinite/zero-size input remains inert. Color-port conversion remains explicit.

PtInRect shares a read-only signed-edge predicate with top/left inclusion and
bottom/right exclusion, including empty/inverted rectangles. The 68K adapter
checks the Boolean result byte before popping eight argument bytes; PPC unpacks
the by-value Point from r3 and reads its Rect pointer from r4. No current port or
GPU is needed for this geometric query. Low-memory raw mouse mirrors, mouse
acceleration and custom window tracking remain outside this slice.

## Window/control hit queries and splash tracking

FindWindow decodes one global Point and output pointer on both CPU adapters.
The manager validates output storage, preserves MemErr, clips to screen/menu height and walks
the live WindowList front to back, skipping hidden windows. Checked structure
and content regions determine occlusion, including complex holes. Content,
negative windowKind system windows, document title bands and active close boxes
produce their documented part codes; inert frames return desktop/nil and do not
expose a lower window. Plain/modal definitions have no synthetic drag target.
The close rectangle is shared with document-chrome geometry. No selection,
activation, drawing or GPU is required for the query. Current definitions remain
1/2/4; custom WDEF handles and unsupported definitions fail explicitly.

FindControl validates the live owner and entire registered control list, then
reuses TestControl for each candidate in guest list order. It returns the first
nonzero button-family part and handle, or zero/nil, without drawing or clipping
local control bounds through a window's current visRgn. Invalid lists and output
permissions are checked before result publication; custom CDEFs remain errors.

StillDown performs a non-consuming check for either pending mouse-down or
mouse-up in the shared OS queue, returning true only when neither is pending.
This follows Apple Toolbox Essentials, Event Manager p. 2-109; legacy Systemless
also consulted physical state and only queued mouse-up, so that comparison is
not treated as authority for this case. Button/WaitMouseUp are separate contracts
and remain unimplemented. Nil SetPort restores a saved empty current-port slot;
operations requiring a drawable port still fail until a real port is selected.

## Process-local fork I/O

File Manager operations share one catalog, access-path table and fork storage on
68K and PPC. File creation adds an empty data/resource pair to an existing parent
with a stable node ID; catalog/file/FCB queries project those same bytes, dates,
Finder information and open flags. Named/indexed catalog queries honor ioDirID
as their parent; callers must not reuse an earlier returned file ID as a directory.
This corrects the previous root-only lookup assumption and its fixtures.

Synchronous PB[H]Open/OpenRF, PBHOpenDF, create, read/write, position, EOF, flush,
Finder-info updates and close have checked ABI routes. fsCurPerm falls back to
reading when write access is unavailable; fsWrPerm aliases exclusive read/write.
Shared and exclusive write paths are distinct, fork-specific access decisions.
Loaded application forks remain locked; newly created forks are writable. Marks
belong to access paths, while bytes/EOF belong to a file fork. Reads report actual
counts, partial EOF and newline termination; writes snapshot guest source bytes,
zero-fill extension and preserve the other fork. All parameter/buffer outputs are
validated before publishing state; memory faults do not consume references, move
marks or modify bytes. Flush commits no extra work because writes already update
the canonical process storage.

The model bounds created nodes to 1024, simultaneous paths to 256 and total mutable
logical bytes to 64 MiB. Shrinking releases excess fork storage. Parameter errors,
missing/locked files, permission/share conflicts, bad references and bounded-volume
exhaustion are guest errors. Unsupported asynchronous completion remains an explicit
fault; the generic PPC read/write/close entry points accept only synchronous calls.
Devices.h supplies MPW's register-based I/O declarations. Resource-file byte I/O
does not attach a new Resource Manager map. Persistence, deletion/rename, imported
Finder data, raw allocation/range locks, working directories, read-verify flags and
asynchronous callbacks remain unfinished. Virtual 4 KiB extents are metadata
projections, not a physical HFS allocation implementation.

## Live menu lookup and Gestalt bootstrap

GetMHandle/GetMenuHandle reads the current DynamicMenuList, searches hierarchical
entries before regular entries and reads signed IDs from live handle payloads.
It preserves handle identity across relocation and direct guest ID/list edits;
missing IDs return nil. List geometry, handle storage and ID-word bounds are
checked, with MemErr preserved. Lookup needs no GPU, font or color-table state.
It does not load menu resources or insert entries. The 68K Pascal word argument
and long result slot differ from PPC's low-word r3 argument and full r3 handle.

Gestalt shares the compatibility profile's system-version and 68K processor
answers with SysEnvirons. Only implemented selectors sysv/proc are registered;
other selectors return gestaltUndefSelectorErr (-5551). In particular, ppc-space
asks about the PPC communications Toolbox, not the PowerPC CPU backend. That
communications manager and evnt's enhanced Event Manager capabilities are not
advertised. No dynamic selector registration or callback dispatch is implied.

The native A0AD/A1AD query adapter returns a successful value in A0 and OSErr in
D0; the SDK's following MOVE.L A0,(A1) performs the caller output store. PPC writes
the supplied r4 result pointer and returns signed OSErr in r3. Failed queries
preserve the native A0 and PPC output; native caller glue can still store its
undefined old A0 on error. Output errors preserve PPC return registers. This
architecture distinction is tested instead of replacing both ABIs with pointer
writes. NewGestalt/ReplaceGestalt registration paths remain unsupported.


## Resource-backed menu construction

Native GetMenu/GetNewMBar Pascal adapters and PPC imports marshal into the same
Menus operations. Checked MENU parsing preserves signed logical IDs independently
of Resource Manager keys; MBAR describes ordered resource IDs. The Resource Manager
owns cached MENU handles while the caller owns a newly allocated DynamicMenuList.
Preparation computes standard dimensions and color merges before publishing a
single group of handle replacements. Missing child resources cannot install a
partial list. SetMenuBar copies that list into the manager-owned stable handle.
Shared source FONT geometry plans title ink; destination coverage and clipping run
through wgpu on the canonical screen bitmap without changing the guest port.
Hosted standard definitions use a zero definition field; this is not a substitute
for a callable guest MDEF or a completed Mixed Mode callback implementation.
Popup rendering, tracking and guest definitions remain executable milestones.
Resource layouts follow Apple's [MENU](https://dev.os9.ca/techpubs/mac/Toolbox/Toolbox-183.html)
and [MBAR](https://dev.os9.ca/techpubs/mac/Toolbox/Toolbox-184.html) descriptions;
layout/sizing evidence is separately identified as legacy Systemless comparison.


## Resource-fork creation and encoding

The checked ResourceFork format layer now serializes supported maps without a
legacy-runtime dependency. File Manager catalog nodes remain the only owners of
mutable fork bytes. CreateResFile preflights capacity, initializes an empty map
and leaves existing raw access paths coherent; it neither installs a process map
nor changes the current resource reference. Native and PPC adapters reach this
same operation, with errors published through ResErr and no function result.
Additional open resource maps, cached-resource ownership across those maps and
changed-resource publication remain required future operations; raw file writes
still do not automatically attach Resource Manager maps.


## Polygon recording and GPU coverage

The process polygon manager stores only active handle/port identity. Vertex bytes,
size and bounds remain in the shared guest heap; line calls read/update those live
bytes. Open/Close balance pen visibility and publish/clear polySave. Suspension,
foreign state, heap failures and unsupported callback/recording combinations have
explicit boundaries. CPU adapters marshal the same lifecycle and drawing operations.

Polygon source vertices are borrowed during synchronous command submission and
copied into the GPU parameter buffer. The compute shader performs bounded even-odd
fill tests or a union of the shared line-brush predicate. Transfer executes once per
covered pixel, retaining XOR corners, clipping, patterns and direct-memory coherence.
No second framebuffer renderer or CPU polygon scan conversion is introduced.
Raster sampling follows the permitted legacy reference, with integer arithmetic;
native pixel parity and broader recording integration remain unverified. The first
GPU validation error is retained so shader faults are not hidden by pipeline fallout.

OpenPoly visibility semantics follow Apple's [OpenPoly reference](https://leopard-adc.pepas.com/documentation/mac/QuickDraw/QuickDraw-121.html).
Legacy fill sampling is separately evidenced by `trap/shapes.rs::draw_poly` and
`loader/ppc/regions.rs::ppc_fill_polygon_interior` in the preserved original runtime.


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


Tracking is a runtime continuation containing the shared control identity and the
architecture-specific Reply. Polling precedes further CPU execution and returns
Waiting { ticks: 1 } while held. Event removal is preflighted before drawing and
committed only on a successful release. This is distinct from guest callbacks;
real Mixed Mode action procedures remain unimplemented.


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

The shared service takes typed Format/Parse operations with explicit guest
pointers. Parse returns a scalar and optionally writes a guest output; the adapter
alone chooses D0 versus PPC output-pointer delivery. The runtime does not inspect
CPU registers. Complete output validation precedes writes, including overlapping
source/output. Guest-memory faults propagate without partial publication. Real
MPW compiler wrappers on both CPUs and generated storage/ABI tests validate this
boundary; original-app evidence is recorded separately.


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

The shared Trap Manager table advertises CopyMask with a readable built-in entry
veneer, so availability checks and indirect native calls resolve the implemented
service. Its availability test validates the same table entry and opcode.


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


IsDialogEvent shares the WindowList traversal used by FrontWindow, and reads the
live front visible windowKind rather than maintaining a second dialog-focus
cache. The documented System 7 predicate is independent of event type/message;
EventRecord storage must still be readable. The shared query has no graphics,
allocation or event-consumption dependency. DialogSelect and dialog construction
remain separate unsupported services. See Apple's [IsDialogEvent contract](https://dev.os9.ca/techpubs/mac/Toolbox/Toolbox-427.html);
legacy Systemless filters foreign updates/outside mouse clicks differently.


EmptyRect is an allocation-free shared rectangle query over the live eight-byte
Rect. Signed endpoint comparisons avoid guest-word subtraction overflow, so a
rectangle spanning -32768..32767 remains nonempty. It preserves guest error
globals and source bytes, accepts readable immutable storage, and faults on
truncated/unmapped input. Native $A8AE pops a four-byte pointer and writes only
the canonical Boolean result byte; PPC returns 0/1 in r3. EmptyRect and
IsDialogEvent preflight the native result byte before changing the stack pointer.
The predicate follows Apple's [rectangle definition](https://dev.os9.ca/techpubs/mac/QuickDraw/QuickDraw-19.html).


EnableItem/DisableItem modify only the live classic MENU enableFlags word. Bit
zero gates the whole menu while retaining all individual bits; supported existing
items 1..31 have independent bits. No host enable-state cache, guest allocation,
implicit redraw or graphics dependency is introduced. Checked handle/record
reads precede the in-place write, and both CPU routes preserve caller error
globals and void return values. The existing DrawMenuBar reads the live bit zero
for title dimming. Standard MenuSelect tracking remains an explicit missing
service rather than returning a fake no-selection result.


MenuSelect is a shared cooperative service. A MenuWait retains only its checked
return ABI and shared Menu Manager Tracking state while either real CPU is
suspended. Polls read live menu records, enable bits, the screen bitmap, global
Mouse and pending release coordinates; they do not execute guest instructions
or dispatch the service again. Keyboard and unrelated OS events remain queued.
Masked mouse-up posting still permits completion from physical input state.

The tracker plans standard monochrome title/dropdown/text/mark/shortcut drawing
through the existing shared strike geometry and wgpu bitmap operations. A host
owned immutable snapshot of canonical screen indices provides the restoration
source. Switching/redrawing/restoring uses bounded GPU blits over the damaged
area; pixels outside it and bitmap padding retain live guest state. Presentation
continues to use canonical guest screen bytes. A selected title stays highlighted
through DrawMenuBar until HiliteMenu clears it; MenuChoice reads live MenuDisable.
Both CPU boundaries retain their own Pascal/PPC result layouts. Error globals
and caller port state are preserved.

This is the standard non-scrolling monochrome application-menu slice. Custom
MDEFs, hooks, desk-accessory/zero menu IDs, icons, hierarchical submenus, per-item
color overrides, oversized/scrolling menus, unsupported text faces and nonzero
MenuFlash counts remain explicit errors. Native menu pixel parity is unverified.
Full hierarchy, callback, flash and scrolling work remains required by the
rebuild, and is not replaced with fake no-selection results.

HiWord/LoWord use a shared signed 16-bit extraction helper with native word
results and signed PPC register results; adapters only decode arguments.
Contracts: Apple's [MenuSelect](https://dev.os9.ca/techpubs/mac/Toolbox/Toolbox-133.html),
[MenuChoice](https://dev.os9.ca/techpubs/mac/Toolbox/Toolbox-135.html), and
[mathematical utilities](https://dev.os9.ca/techpubs/mac/pdf/Operating_System_Utilities/MLU.pdf).


Control geometry shares the Window Manager's staged local-to-global content
invalidation. Move/size validates registered ownership, bounds, supported owner
drawing state, clip geometry and text positions before changing pixels. A cloned
allocator/handle map simulates the exact update/clip-resize sequence without
publishing guest state; successful rendering uses the existing shared control
renderer, followed by the rectangle publication. Old visible areas use bkPat and
become dirty, while hidden controls only update geometry. A hidden owner has no
destination pixels and needs no GPU. Both adapters preserve void results and
caller errors/port state. Coordinate wrapping/nonpositive sizes, custom controls
and recording ports remain explicit gaps; device faults are not rolled back.


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
fork permissions and catalog ownership remain shared. The default-volume selection
extension below adds real selected-directory state; multiple mounted volumes
remain unfinished.


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
requirements are checked explicitly. Nonzero dialog hooks, custom Pack3 selectors,
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
nodes; host paths and actual disk I/O remain outside this File Manager model.


### Classic open dialog and production file-filter callbacks

SFGetFile (Pack3 selector 2 / InterfaceLib) uses the same registered modal window,
controls, catalog, input queue and wgpu renderer as SFPutFile. Its 28-byte native
selector/argument frame and seven PPC arguments have checked ABI boundaries;
the ignored prompt pointer is never dereferenced. The type list is snapshotted
before allocation: -1 admits every file type, 0 admits none, and 1–4 compare real
Finder types. Folders remain navigable without invoking the file filter. Success
returns the selected file's Finder type, name and actual WD; Cancel modifies only
the good byte. It creates no files and performs no host I/O.

A retained dialog prepares the shared File Manager's catalog parameter block,
including the classic FileParam prefix, Finder info, fork lengths, dates and
parent directory. Runtime suspends the entire caller CPU in a continuation and
executes one guest filter using the ordinary bounded instruction loop. A true
Boolean suppresses the file; false admits it. Callback Toolbox calls use the
same dispatcher and shared process. Returning resumes filtering before accepting
input. Idle waits execute no guest instructions. Structural catalog changes or
folder navigation invalidate the filtered listing.

The native callback inherits A5/register context and receives a real Pascal
pointer argument and word-sized Boolean result slot, with a checked guest return
gate. PPC receives r3 and a checked 64-byte aligned caller linkage frame, loading
entry/TOC from its transition vector. The whole suspended CPU restores private
and architectural caller state on completion. The supported UPP form is raw
68K code or a version-7, single-record, prepared absolute same-ISA routine
descriptor with procInfo $D0. Apple BUILD_ROUTINE_DESCRIPTOR output is exercised
by the compiled PPC fixture. Other descriptor versions/flags, unprepared or
relative fragments, fat descriptors and cross-ISA routines explicitly fail.
This is production same-ISA callback support, not general Mixed Mode or
CallUniversalProc/NewRoutineDescriptor support. Native pixel parity, hooks,
modern/custom Standard File APIs, remembered directories, multiple volumes,
clipboard/focus behavior and complete host flush/error semantics remain unfinished.


### Writable catalog persistence at the host boundary

Files now encodes/transactionally restores one bounded portable writable catalog,
including IDs, directories, Finder metadata/dates and both forks. Application
forks, live file/WD references and guest execution state remain process-owned.
The same File Manager mutation points update a host revision counter for both
CPUs. The optional host Volume owns paths, locking, bounded reads and synchronized
atomic image replacement; guest services do not acquire host path knowledge.
Headless/native launch paths accept --volume and checkpoint at execution/close
boundaries. No-volume behavior remains process-local. The codec, version-1 wire
format, validation rules, platform evidence and incomplete per-service flush/error
semantics are detailed in [volumes](volumes.md). This is persistence of Macintosh
file state, not a snapshot of CPU/UI state or an HFS disk image.


### Shared Sound Manager channel lifetime

Process owns one Sound Manager channel registry for both CPU adapters. SndNewChannel
($A807 / InterfaceLib) validates its pointer slot, records synthesizer/initialization
configuration and creates an owned guest queue. A nil channel receives an owned
1,060-byte SndChannel record and the default 128-command queue. A caller-supplied
record keeps its userInfo, allocation and queue-size choice; positive signed
16-bit qLength values determine the separately allocated internal storage and
checked inline record extent. Commands use the actual inline queue at offset 36. Guest fields are
big-endian, nextChan links reflect the same process registry, and the reserved
firstMod field holds the runtime's queue allocation identity. Callback addresses
are retained as guest values and resolved by the callback adapter when called.
Host diagnostics can query configuration.

SndDisposeChannel ($A801 / InterfaceLib) validates registration, allocation
ownership and idle queue fields before unlinking. It repairs a predecessor link,
releases the manager queue and releases the channel record only if the manager
allocated it. Caller storage, callback/userInfo and referenced handles remain
owned by the caller. Unknown/nil/duplicate/corrupt channels return badChannel;
allocation rollback and process Memory/Resource error preservation are checked.
Native OSErr results occupy the Pascal word slot, while PPC returns a sign-extended
OSErr in r3. Output bounds are checked before native stack consumption.

This slice supports idle channels configured for unspecified or sampled sound,
with known uncompressed initialization flags. Other synth/configuration requests
return explicit errors. There is a documented 64-channel runtime bound; it does
not describe native hardware capacity. Both quietNow values complete immediately
for an idle channel. Busy/private-field corruption is rejected, never discarded
as if played. The sampled-immediate extension below adds actual rendering and
native transport. The FIFO/callback extension below adds real command ordering.
Channel-status APIs, other synthesizers and wait-for-playback disposal remain
unfinished. Channel
creation/disposal alone does not establish original-game playback compatibility.


## Shared display-device records and live indexed sources

The process now owns an initial display-device manager. The first device query
prepares owned GDevice (62-byte), PixMap (50-byte) and ColorTable handles, then
publishes MainDevice ($08A4), DeviceList ($08A8) and TheGDevice ($0CC8). Those
addresses and public layouts were verified against the pinned Macintosh SDK.
Initialization validates global/output storage first and rolls back partial
handle allocations. It is lazy and uses the current process heap; it is not yet
an eager system-heap boot device or a complete Graphics Devices Manager.

The initial device truthfully describes the existing 512x342, one-bit screen.
Its PixMap references the exact packed guest pixel memory used by basic
QuickDraw, with the current two-entry white/black palette. Flags report main,
screen, active and no driver; gdDevType remains clear. SysEnvirons/Gestalt do not
newly advertise Color QuickDraw. Multiple devices, mode/depth changes, device
creation/disposal, SetGDevice and general device attribute setters remain
unsupported. Unknown selections/links, disposed records and unsupported table
layouts fail explicitly rather than inventing handles or colors.

GetGDevice/GetMainDevice/GetDeviceList/GetNextDevice/TestDeviceAttribute and
Index2Color are shared services with architecture-specific marshalling. Queries
read live registered guest globals/records; Index2Color follows the current
record's PixMap/table handles and returns full 16-bit RGB components. Process
and guest MemErr/ResErr are preserved. Native result slots, Pascal argument
widths and PPC void/Boolean/pointer behavior have real-CPU coverage.

CopyBits now accepts checked live indexed PixMap sources at 1/2/4/8 bits, in
addition to ordinary BitMaps. The source decoder honors row stride, packed bit
order, signed bounds, component/depth fields and device-versus-value-indexed
color tables. It rejects direct/compressed/planar layouts, invalid or duplicate
palette entries, undefined pixel indices and out-of-bounds storage. Metadata,
rows and palettes may be read-only. Both row storage and decoded pixel counts
are bounded to the existing 64 MiB image limit. A complete source snapshot is
prepared before destination writes, retaining overlap safety.

The existing wgpu palette path converts indexed RGB through the already-tested
monochrome destination policy used for PICT. This is not full native color
quantization parity or a color destination implementation. CGrafPort drawing,
NewCWindow, indexed destination memory/drawing, configurable screen modes and
palette-to-presentation synchronization remain required next steps. Tests copy
both the actual device PixMap and a read-only eight-bit RGB source through both
CPU adapters and check destination bytes, padding and real GPU readback.


## CGrafPort lifecycle before color-window integration

One process-owned color-port registry now implements OpenCPort, InitCPort and
CloseCPort through both CPU adapters. The SDK-verified 108-byte CGrafPort owns
seven attached handles: a PixMap, visibility/clip regions, three old-style
PixPats, and the 26-byte GrafVars record. The copied PixMap borrows the current
device's color table and pixel storage; those are not freed on close. Caller
port storage remains caller-owned. Defaults include portVersion $C000,
pnLocHFrac $8000, black/white requested RGB, old-style white/black patterns,
zero opColor, and the live HiliteRGB ($0DA0) copied to GrafVars. Fore/background
indices are selected against the current two-entry device table.

Opening validates caller storage/current-slot output first and rolls back a
partial attached-handle allocation. Allocation failure returns through MemErr
without publishing a port or replacing the current port. Shared device creation
is a separate prerequisite and its lazy device objects persist independently.
InitCPort reuses the existing attached handles without guest allocation; a basic
GrafPort is an inert case that preserves its bytes, current selection and guest
MemErr. Close validates the complete owned graph before disposal and never
frees a borrowed/replaced table or the screen. Caller record bytes/current port
selection survive close, matching the existing ClosePort contract; callers must
select another usable port before further drawing.

Unknown/closed ports, replaced owned fields and unsupported expanded/color
pattern graphs fail explicitly. The supported patterns are the initialized
old-style PixPat form; general PixPat cache/resource ownership is unfinished.
Reset/close require the owned handle identities to remain attached, while a
custom pmTable remains caller-owned. Nil/short/read-only native output storage
is rejected before popping the caller frame.

Color-port drawing now resolves the live portPixMap and old-style background,
pen and fill PixPat handles through a shared checked layout module. Geometry,
text, TextEdit and control painting use the common fields and restore attached
pen pattern bytes separately from the port record. SetOrigin/MovePortTo update
PixMap bounds without overwriting grafVars/chExtra. Coordinate conversion uses
those same bounds. PenNormal and PenPat preserve attached handle identities.
Black/white ForeColor/BackColor update requested RGB and pixel index fields.
CopyBits/CopyMask accept one-bit PixMap destinations with the canonical palette.

This is bounded one-bit drawing. The destination must be depth one with white
index zero and black index one; other depths/palettes and expanded/color PixPat
caches fail explicitly. NewCWindow now uses the same Window Manager described
below; display modes and full Color QuickDraw remain unfinished. No new Color
QuickDraw capability is advertised.


## Shared color-window lifecycle and startup utilities

NewCWindow ($AA45 / InterfaceLib) decodes into a distinct typed call and joins
NewWindow in one Window Manager. Standard supported definitions (1, 2 and 4),
ordering, visibility, drawing, updates, movement, sizing and close/dispose share
the existing implementation. CWindowRecord is 156 bytes: its CGrafPort owns
seven attached records and its standard window tail owns four handles. The
Window Manager preflights the entire graph, master writes and cross-graph alias
checks before unlinking a color window or removing its controls. Close releases
all eleven handles once while preserving caller storage and borrowed device
palette/pixels. Dispose also releases heap-owned window storage.

Failure while allocating window-tail/port attachments rolls back those
allocations, returns nil with MemErr=-108 and preserves caller storage and the
current port/window list. Subsequent visibility-region growth and painting keep
the existing Window Manager fault behavior; full rollback after publication is
not yet guaranteed. Lazy
device bootstrap remains an independent prerequisite. Hidden empty rectangles
are valid setup windows; port dimensions retain degeneracy while region storage
uses canonical empty rectangles. Resizing an empty window reconstructs its
global origin from port/PixMap bounds rather than the normalized empty region.
Visible empty or inverted rectangles remain unsupported. Native Boolean slots
use the high byte only; nonzero padding does not make EV's hidden window visible.

The manager reads/writes PixMap bounds through shared accessors, preserving
CGrafPort grafVars/chExtra. Content background painting resolves bkPixPat and
converts the one-bit foreground/background indices for the monochrome manager
port. Display depth, destination palette and PixPat limitations still apply;
GetNewCWindow, general WDEFs, multiple devices and display modes are unfinished.

SetGDevice selects an already registered device by writing the live current
device global, preserving the current port and errors. The initial single-device
profile permits restoring that registered selection after a guest global edit;
nil/foreign handles fail without creating a device or changing selection.
BitAnd/BitOr/BitXor/BitNot use exact shared 32-bit logical operations. FreeMem
reports the sum of actual free allocator extents, including fragmentation,
master-pointer storage, heap-specific CODE reservations and reclaimed blocks.
These utilities preserve guest error globals. FreeMem neither compacts/purges
nor reports an invented compatibility floor; system zones and memory compaction
remain unfinished. Both native register/stack and PPC import ABIs are tested.


## Shared sampled sound and thin native output

SndDoImmediate ($A804 / InterfaceLib) reaches one process-owned command
implementation. Supported commands are null, quiet, amplitude/getAmplitude,
stereo volume/getVolume and bufferCmd. The API reads a const eight-byte command;
getters write only their requested guest outputs. Format/unsupported init errors
return OSErrs, while unimplemented command families fail explicitly. A busy
buffer submission is rejected rather than replacing an active sound. Quiet
cancels the voice. Quiet channel disposal releases active owned playback;
nonquiet disposal of an active channel currently returns -209 instead of
waiting. The FIFO/callback extension below adds command execution and
completion callbacks; quiet cancels the current voice while queued work continues.

Standard SoundHeader fields are checked before publication: inline/external
sample pointers, bounded nonzero length, unsigned 16.16 rates (1 Hz through
64 kHz) and encoding. Buffer playback does not interpret note-pitch/loop fields. Only unsigned eight-bit
mono standard headers are accepted. Buffer playback does not use loop points
for note-duration synthesis. Extended/compressed headers, installed voices and
frequency/duration commands remain unfinished. Mono pan and no-interpolation
initialization flags are applied. Legacy sample-rate attribute bits are retained
while the host chooses the sink clock; the header rate controls source timing.
Unsupported no-drop/stereo-source configurations still return errors.

The shared renderer pulls at most 65,536 stereo frames at an output rate of
8–192 kHz. It validates every channel, sample span and completion write before
advancing any voice. Live guest bytes are read on each pull. Unsigned 8-bit input
is centered at 128, converted to signed 16-bit PCM, mixed with channel gains and
saturated. Linear interpolation is the default; initNoInterp uses held samples.
Fixed-point phase keeps a rational remainder to prevent an extra sample at
nonintegral rate ratios. Changing output-clock denominators can lose less than
one 32-bit fractional phase unit; full native resampler/filter parity is not
claimed. Master SdVolume is read live; the existing initial value is retained. The
compiler fixture explicitly sets volume seven before producing its samples. Error globals are preserved by command/render operations.

Native and headless hosts share these semantics but select their clock policy.
Native Output owns only bounded mixed PCM frames, never guest memory or CPU
state. CPAL 0.18.2 opens the actual default output, converts stereo PCM into a
supported device format and consumes it on the audio callback. The callback
never waits on the producer mutex; missing frames become silence. The native
host lazily opens output on first playback, pumps up to 50 ms ahead, keeps
pumping while presentation is minimized and drops the stream on guest exit.
This queued transport adds latency; end-to-end timing/continuous playback under
load is not yet accepted. Device/configuration errors remain visible; CPAL Xrun
and realtime-promotion advisories are counted separately from fatal errors.
The Linux build requires libasound2-dev/pkg-config; the future CI installs them.
Web audio transport remains unfinished; the common command/PCM model is portable.

Headless CLI waits advance sound in bounded work per registered channel, using
wide rational arithmetic rather than rendering enormous discarded buffers.
Explicit render_sound provides sample capture. Both policies clear active
command bytes on completion. Exit and CPU faults cancel owned voices at the
Runtime boundary and prohibit later sound rendering; final guest bytes remain
available as an inspection snapshot. No version/capability bypass was added.

Primary references: [Apple SoundHeader](https://dev.os9.ca/techpubs/mac/Sound/Sound-74.html),
[Apple SndDoImmediate](https://dev.os9.ca/techpubs/mac/Sound/Sound-101.html),
[Apple command definitions](https://leopard-adc.pepas.com/documentation/mac/Sound/Sound-47.html)
and [CPAL API](https://docs.rs/cpal/0.18.2/cpal/), plus pinned public SDK Sound.h.


## Real handle flags and bounded high relocation

HGetState/HSetState, HPurge/HNoPurge, HSetRBit/HClrRBit and HLock/HUnlock share
one allocator-owned flags model. The lock bit directly controls whether payloads
can relocate. Purgeable/resource bits and opaque reserved bits survive normal
and grouped resizes. The flags are out of line, so a 32-bit master pointer always
contains the complete guest data address. Resource-map resLocked/resPurgeable
attributes initialize lock/purge state; resource-backed handles also receive the
resource bit. DetachResource clears that bit while retaining the other live
properties. The Resource Manager's identity map remains distinct from raw flag
changes: HClrRBit does not itself detach a map entry, and HSetRBit does not forge
resource metadata.

HGetState returns a signed byte, including the low error byte on invalid/nil
handles. Native adaptation writes only D0's low byte, preserving A0/stack and
upper D0 bytes; PPC sign-extends the result into r3. Setter procedures preserve
PPC r3 and report errors in MemErr; native register procedures return the result
code in D0. Resource errors remain unchanged. Nil master pointers now report
nilHandleErr for flag/lock operations; nonnil foreign master edits still fault
before properties change. Full empty/purged-handle allocation semantics remain
unfinished; marking purgeability does not claim actual compaction or eviction.

MoveHHi preserves flags, handle identity, payload and total free memory. An
unlocked block can slide upward into its adjacent free interval, including an
overlapping move. Snapshot bytes and a cloned allocator stage the operation;
master/destination output checks precede publication. Other allocations are
barriers, including nonrelocatable masters/Ptrs and other handle payloads. This
does not compact other unlocked handles or jump across an immovable block. An
already blocked/high block stays in place successfully; a locked block reports
memLockedErr (-117). Changed resource blocks fail explicitly until WriteResource
can perform the required update, rather than silently discarding persistence.

Sources: [Apple flags](https://leopard-adc.pepas.com/documentation/mac/Memory/Memory-86.html),
[HSetState](https://dev.os9.ca/techpubs/mac/Memory/Memory-87.html),
[HPurge](https://dev.os9.ca/techpubs/mac/Memory/Memory-90.html),
[MoveHHi](https://dev.os9.ca/techpubs/mac/Memory/Memory-100.html) and pinned SDK MacMemory.h.


## Sound command FIFO and CPU-bound completion callbacks

SndDoCommand ($A803 / InterfaceLib) copies eight command bytes into the channel's
inline ring at offset 36 and advances checked qHead/qTail indices. Positive signed
qLength controls capacity for caller storage, including records larger than the
standard 128 slots. Pending ring bytes and registry state must agree; arbitrary
guest edits to internal queue storage are diagnosed rather than silently ignored.
The firstMod allocation remains an opaque owned internal block, not the ring.
The shared executor supports null, quiet, flush, amplitude/getter, stereo
volume/getter, standard bufferCmd and queued callBackCmd. Unknown command families
still fault explicitly; rejected headers/configurations return actual errors.
Queued buffer pointers and samples remain live, while the submitted command record
itself may be reused immediately. Normal quiet stops the current voice and then
processes queued work; flush clears pending work without stopping the voice.
Quiet inside an active callback leaves that callback's command frame intact.

Full queues return queueFull (-203) when noWait is true. Otherwise Runtime owns
a copied pending command and reply, returns a cooperative one-tick wait, and retries
without executing guest instructions until space is available. Queue state and
result remain unchanged while full. This wait can itself be interrupted by sound
callbacks. A saved generation also prevents a blocked submission from targeting
a replacement channel at the same address; it returns badChannel instead. Nonquiet active channel disposal remains -209 until its separate
waiting semantics are implemented.

The renderer crosses buffer/control-command boundaries within one output quantum,
applying controls only after preceding buffers finish. It checks channel/ring
ownership, queued header spans and output locations before phase advancement.
The same native renderer's PCM contract applies; command-induced guest memory
self-corruption is not a transaction with full memory rollback. Silent queued
advancement matches a 48 kHz sink using analytic frame consumption, visits each
pending command/voice at most once and allocates no discarded PCM even for a
u32::MAX-tick wait. It stops at a callback boundary until guest execution resumes;
callback latency is therefore host-quantum/cooperative scheduling, not hardware
interrupt emulation.

callBackCmd blocks further commands in that channel and schedules a guest call.
Runtime dispatches it at a budget boundary, never on the native audio thread.
The adapter marshals Pascal void(SndChannelPtr, SndCommandPtr) with procInfo $3C0:
raw 68K routines or prepared absolute version-7 same-ISA RoutineDescriptors;
PPC uses the routine's transition vector/TOC. Runtime retains the whole caller
CPU and suspended event/control/menu/file/sound waits. Event deadlines continue
through host time advancement while suspended. Instruction-derived TickCount
increments now use that same clock path. A compiled long callback verifies expiry
of a suspended EventWait without host time advancement; this remains a virtual
clock policy, not native hardware timing parity. The running callback holds a mutable borrow of its command pointer; it can
reuse those bytes for another sound command without failing the channel's private
state checks. Pending callbacks still validate the original command, and return
clears the borrowed frame before pumping more work. Callback services use the same
Process; returning restores the interrupted CPU/wait and continues the FIFO.
Nested callbacks remain bounded to depth 32. This cooperative implementation
permits allocation/disposal in a callback as a runtime extension; Apple's actual
interrupt-time contract forbids Memory Manager calls. The disposal/reallocation
fixture stress-tests continuation ownership and is not a native-conformance
example. Synthetic register clobbers likewise stress restoration; conforming
native callbacks must preserve the registers required by the callback ABI. A monotonically assigned channel
generation prevents an old callback return from clearing a replacement allocated
at the same guest address. Exit during a callback fails explicitly. General
routine-descriptor allocation, cross-ISA Mixed Mode and nonlocal callback exits
remain unfinished.

Both actual CPUs and public-header compiler fixtures verify copied commands/live
samples, PCM boundaries and gains, full-queue failure/wait, callback budget/state
restoration, nested Toolbox calls, event-wait resumption and disposal/address
reuse inside a callback. CLI fixtures finish without host audio. The resource extension below adds SndPlay. This does not
establish pause/wait/sync commands, Sound Manager version APIs,
other formats or original EV/Marathon gameplay. Sources: Apple's
[Sound Manager summary](https://leopard-adc.pepas.com/documentation/mac/Sound/Sound-136.html),
[SndDoImmediate](https://dev.os9.ca/techpubs/mac/Sound/Sound-101.html),
[callback contract](https://leopard-adc.pepas.com/documentation/mac/Sound/Sound-131.html)
and pinned SDK Sound.h.


## Loaded resource sound and scoped synchronous completion

SndPlay ($A805 / InterfaceLib) resolves an owned, resident handle, preserving
MemErr/ResErr and returning resProblem for nil/freed/empty handles. Resource
Manager identity is not required, so application-created sound blocks work.
Format 1 reads its six-byte data-type entries before the command count. Format 2
ignores its application-owned reference-count word. Command counts are bounded
by the format's u16 and the actual handle length. Resource-relative bufferCmd
pointers are checked after the command table, against the complete 22-byte header
and inline sample extent. Absolute headers and external sample pointers use
normal checked guest-memory spans. Source command bytes are copied/resolved;
the resource is not rewritten and sample memory remains live. Unsupported
commands/formats fail explicitly before normal publication.

A supplied channel retains its initialization; a nil channel is allocated from
the same Process allocator, using zero/default or one sampledSynth initializer.
Multiple/other internal synth chains return an error. Legacy rate attribute
bits ($30, including original resources' $A0 configuration) are preserved;
portable output remains host-clock selected with source timing from SoundHeader.
This does not emulate a fixed classic hardware output clock or prove native
filter parity. Buffer commands do not consume baseFrequency or loop points,
so these unused fields no longer reject one-shot buffers. Note synthesis will
need its own validation and remains unimplemented. Encoding, source rate,
length and actual read spans are still validated; no private asset is normalized.

Every queued resource command owns an optional reference to a per-call completion
counter. Pending entries and active buffers/callbacks retain it; normal execution,
quiet, flush, cancellation and disposal release exactly their own references.
Zero-time commands finish immediately. These counters contain no guest CPU or
host audio transport state. They let a synchronous call finish when its own
commands finish, even if a callback has queued unrelated later audio. Neither
whole-channel-idle polling nor an early success substitutes for that boundary.

Runtime retains resource submission/completion and reply state while returning a
cooperative one-tick wait. An async call can wait for queue space to copy its
commands, then returns before playback finishes. A nil channel always plays
synchronously regardless of the async argument. Its temporary pointer slot is
released after creation, and its owned channel/internal storage is disposed after
completion or an ordinary submission error. Allocation rollback is checked.
Generation checks prevent a suspended call from targeting a replacement channel
at the same address. A sync call inside its own running callback returns busy;
an async reentrant call that cannot fit also returns busy, avoiding self-deadlock.
Resource waits are suspended/restored by the same callback continuation as event,
menu, control and file waits. Both host and instruction-derived virtual ticks now
age suspended event counters. A host diagnostic counts successful sound callback
returns without introducing guest-visible state.

Source residency remains the caller's responsibility for asynchronous playback,
including the documented lock requirement. SndPlay does not change source handle
flags. The current allocator does not automatically move/purge blocks, so ordinary
unlocked synchronous sources stay resident while their caller is suspended.
Automatic compaction/purge will require an internal source-residency lease before
it can be enabled; that ownership work is not established by these tests.
Fault/exit retains heap snapshots but cancels sound as at the existing Runtime
boundary. Resource command self-modification is not a fully transactional operation.

Public SC/MrC fixtures prove both resource formats, unchanged source flags/commands,
true nil-channel sync behavior, async return, scoped callback completion, malformed
bounds, cleanup and guest-driven event deadlines. Both compiled applications also
finish through the headless CLI without a device. The external probe uses synthetic
68K/PPC callers against original sound forks; it renders only 32-frame prefixes,
then silently advances remaining playback. Matching prefixes and cleanup are not
full-waveform/native audio or original-application execution acceptance.
References: [SndPlay](https://leopard-adc.pepas.com/documentation/mac/Sound/Sound-35.html),
[resource formats](https://leopard-adc.pepas.com/documentation/mac/Sound/Sound-60.html)
and [sound headers](https://dev.os9.ca/techpubs/mac/Sound/Sound-74.html).


## Live sound status, version ABI and extended PCM

The process sound registry serves SndChannelStatus and SndManagerStatus across
both CPUs. SCStatus is 24 bytes, SMStatus 6; shorter/nil outputs return paramErr,
unknown channels return badChannel, and no partial status is published. The
channel busy bit reflects active samples, queued work and an executing callback.
Attributes retain the channel's actual initialization flags. Manager channel count
includes idle registered channels. Disk times, unsupported pause state and obsolete
CPU-load fields remain zero. Larger outputs retain caller bytes beyond the known
structure. Queries preserve MemErr and ResErr.

SndSoundManagerVersion targets the NumVersion 3.0 final ABI. The 68K dispatcher
writes its four-byte Pascal stack result; the PowerPC ABI receives a hidden
return buffer in r3 because NumVersion is a structure. Real MPW-generated calls
verify this distinction. Gestalt snd reports only stereo output, multichannel
mixing and software 16-bit samples (0x1801). It does not advertise sound input,
recording, hardware 16-bit I/O, codecs, disk or double-buffer playback. This is
an implementation target, not a claim of complete Sound Manager 3.0 behavior.

The shared sample layout parser accepts standard 22-byte unsigned 8-bit mono
headers and 64-byte extended headers for one/two channels and 8/16-bit PCM. It
checks frame count times channels times sample width against bounded input and
resource extents before submission. Interleaved 16-bit samples are signed big
endian; 8-bit samples are unsigned binary offset. A voice retains frame count
and byte extent separately; silent advancement counts frames. Interpolation
operates independently per side; mono channel output averages a stereo source,
while explicit stereo initialization preserves left/right. This software policy
has exact deterministic tests, not native filter/dither conformance evidence.

Primary sources: [Sound Manager](https://dev.os9.ca/techpubs/mac/pdf/Sound/Sound_Manager.pdf),
[ExtSoundHeader](https://dev.os9.ca/techpubs/mac/Sound/Sound-75.html),
[capabilities](https://dev.os9.ca/techpubs/mac/Sound/Sound-64.html), and signed
16-bit representation in [Sound Components](https://dev.os9.ca/techpubs/mac/pdf/Sound/Sound_Components.pdf).

## Handle payload duplication

HandToHand snapshots an owned live handle's payload and allocates a distinct
handle through the same process allocator. It copies no resource identity or
handle attributes; the new block starts unlocked, unpurgeable and non-resource.
The original payload and metadata survive all failures. Empty nil masters return
nilHandleErr; freed/unknown handles return memWZErr; insufficient heap returns
memFullErr without leaking an allocation. Zero-byte allocated blocks remain
valid. Both adapters update MemErr; ResErr is unchanged. The 68K call consumes
A0 and returns new A0 plus D0 error, whereas PPC dereferences the caller's handle
variable, stores the new handle and returns the error in r3. Output storage is
validated before allocation. The current process owns one application heap, so
multiple source heap-zone selection remains a broader allocator requirement.
Source: [HandToHand](https://dev.os9.ca/techpubs/mac/Memory/Memory-106.html).


### Offscreen GDevice attribute ownership

SetDeviceAttribute and TestDeviceAttribute share owned-record validation and
read/write only the 16-bit gdFlags field at offset 20 of a live 62-byte GDevice
handle. Application-created offscreen records need not belong to the initial
screen registry. Attribute access does not initialize devices, append to the
screen list or select the record. All other bytes and process error globals are
preserved. Invalid bits/records fault explicitly; writes validate atomically.
The Pascal Boolean occupies the high byte of its two-byte stack slot; PPC uses
the low argument byte. Public-header compiled guests verify on/off updates on
application records before InitGraf, then unchanged screen/device behavior.
Source: [Apple SetDeviceAttribute](https://leopard-adc.pepas.com/documentation/mac/QuickDraw/QuickDraw-281.html).
This extends state access; it does not implement inverse tables, arbitrary device
selection, indexed drawing destinations or advertise full Color QuickDraw.


### Generated inverse-table data and Color Manager errors

Process owns one Color Manager, including the latest MakeITable result queried
through QDError. The 68K ABI uses reversed 10-byte Pascal arguments for MakeITable
and a two-byte no-pop result for QDError; PPC uses its normal arguments and signed
return. Internal allocation errors do not replace MemErr/ResErr. Invalid resolution
reports cResErr (-156); a failed staged resize retains output bytes and identity.

The builder reads bounded owned ColorTable records, snapshots their entries and
writes the public six-byte ITab header plus 2^(3*resolution) byte indices. RGB
coordinates concatenate red, green and blue. Device tables use ordinals; ordinary
tables use unique explicit byte-sized pixel values. A bounded integer nearest
match at each cell center generates this software table. Source seed/bounds and
3/4/5-bit resolution are checked, and default parameters read current-device
state. Missing default ITab handles are allocated and published after success.
No legacy runtime dependency or ROM table bytes are introduced. Table creation
is state preparation on the CPU, not a graphics rendering fallback.

This does not yet provide hidden-color data/search, reserved client entries,
ROM tie/propagation equivalence or native temporary-memory accounting. Those
remain explicit Color Manager requirements. The supplied C fixture tests public
bytes while allowing private extra allocation. Sources:
[Apple inverse tables](https://dev.os9.ca/techpubs/mac/ACI/ACI-103.html) and
[QuickDraw Reference, MakeITable/QDError](https://leopard-adc.pepas.com/documentation/Carbon/Reference/QuickDraw_Ref/QuickDraw_Ref.pdf).


### Selected offscreen device and empty-geometry contracts

The current GDevice is guest-visible state separate from the initial main/list
device. SetGDevice validates an owned record and publishes only CURRENT;
GetGDevice reads that same field across CPUs. Lazy main-display initialization
preserves an existing selection. Index2Color follows selected device -> owned
PixMap -> owned device ColorTable, so live edits and default MakeITable calls
observe one coherent palette. GetNextDevice validates the requested record/link;
it does not implicitly initialize the display. Automatic physical display lists,
arbitrary device creation/disposal and full indexed drawing remain unfinished.

An empty paint rectangle has no pixel coverage. The shared geometry path validates
the current port/custom-procedure boundary, then returns before decoding an empty
bitmap. Nonempty paints retain all existing bitmap/format checks. GPU regressions
verify unchanged guest and GPU state through actual callers, plus continued
failure for a nonempty request targeting the same empty bitmap. This corrects
initialization behavior; it does not implement 8-bit destination rendering.


### Current-map ID lookup

Get1Resource checks the installed application-map entry before sharing the normal
resource loader/cache. A cached fallback CURS handle is not a member of that map.
A missing ID in an existing type reports -192; an absent type reports noErr with
a nil handle. Native $A81F and PPC Get1Resource decode to one current-map request;
$A820 remains the named variant. Both full/current ID/name native adapters check
result-slot write permission before stack mutation or allocation. Dynamic resource
map chains and UseResFile remain a later extension of the same process model.
[Apple Get1Resource contract](https://dev.os9.ca/techpubs/mac/MoreToolbox/MoreToolbox-51.html).


## Configured main color display

Process owns the host display policy; the main GDevice's live PixMap/table describe
its canonical guest pixels. InitGraf screenBits and basic screen port descriptors
carry the actual stride. Shared surface resolution recognizes full main-screen
basic views (including translated bounds) using the main device rather than the
currently selected offscreen device. Color ports borrow the same table and
screen storage. Offscreen bitmaps retain their own packing. Basic operation ink
maps white/black into actual device indices. CopyBits resolves basic screen
sources/destinations to their real depth and table; window moves snapshot/copy
actual indices. Menu restoration uses index copies so duplicate colors survive.
No monochrome shadow framebuffer is created.

Refresh uses the configured main display independently of the current port or
selected offscreen device. Palette edits affect the next refresh. The GPU cursor
is a presentation overlay: white/black masked pixels and RGB inversion with
palette matching leave canonical screen bytes unchanged. Main-screen depth
switches during execution, palette-manager lifecycle, sub-base basic screen
views, direct color, grayscale attributes and general device operations remain
unfinished. Color matching retains the documented deterministic policy and
limits of the indexed destination milestone.

Color-window background composition resolves live window foreground/background
indices through the window's own palette, then maps their RGB colors into the
main screen through the same clipped wgpu paint service. It does not reinterpret
arbitrary indices as legacy black/white constants. NewCWindow initializes its
screen PixMap from the main device and restores an application's selected
offscreen device even when color-port construction fails. Basic Window Manager
state, clipping, patterns and caller-port restoration remain shared.


## Shared default volume and directory selection

PBSetVolSync/PBHSetVolSync and native $A015/$A215 update one process-local Files
selection after validating caller storage and resolving a real directory. The
selection retains physical directory identity, a hierarchical reference/owner,
and the distinct classic-query reference. Full hierarchical paths override an
invalid volume input; a WD input overrides a stale directory ID. Classic volume
names select the volume root. Failed selection preserves the previous default.

Default relative file operations and Standard File initial browsing follow the
selected folder. Classic explicit physical-volume references retain root semantics;
WD references retain their own folders. Hierarchical zero-directory lookup follows
the selected directory on the virtual volume, while explicit-volume catalog
queries with directory ID zero address the root. Nonzero directory IDs remain
explicit. GetVol and HGetVol share selection but retain their documented different
reference/name results after hierarchical selection. Closing a selected WD drops
that reference while retaining physical directory identity; deletion protects the
selected directory. Selection does not change catalog revision or serialized
volume state, and a fresh process starts at the root. No host path enters a service.

This remains one virtual Application Volume. Asynchronous selection, high-level
wrappers, multiple volumes and broader search-path fallback remain unfinished.
Primary contract: [Apple File Manager, default-volume routines, pp. 2-150–2-154](https://leopard-adc.pepas.com/documentation/mac/pdf/Files/File_Manager.pdf).


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


## Vertical retrace queues and interrupt continuations

`retrace` owns separate system and virtual-main-display VBL queues. Guest QHdr
and VBLTask linkage is real checked memory; VInstall/VRemove and SlotVInstall /
SlotVRemove share this state through both CPU ABI adapters. The one backed host
screen is slot 0 at a fixed virtual 60 Hz. Other slots return slotNumErr. The
system header is at $0160; ScrnVBLPtr points to the owned main-display header.
Queue records cannot alias one another, the headers/pointer, or managed timer
records through Runtime's dispatch. Guest linkage/type corruption faults explicitly.

Runtime advances one checked monotonic counter, currently held by `timers::Timers`,
and supplies its units to both scheduler models; retrace has no second clock.
One refresh consumes 50,000 units. A zero count remains dormant; an initial phase
is added once using wrapping 16-bit word arithmetic. Foreground countdown edits are observed from live memory. The
refresh scan decrements and calls tasks in queue order, allowing an earlier
callback to change or disable a later task before its turn. Backlog processing
skips intervals without callbacks and remains bounded by the guest instruction
budget. A returned task must rearm its count or it is unlinked automatically.

The existing task-callback adapter now marshals VBL A0 on 68K and a prepared
same-ISA PPC VBLUPP ($9802) into r3, while timers retain A1 / TimerUPP ($B802).
Runtime holds the whole interrupted CPU and suspended waits. Queue inVBL flags
remain set through the callback; completion restores caller state and waits.
Timer/VBL callbacks are serialized against one another, and 68K interrupt masks
defer delivery. VBL entry models a level-one mask and a conservative service
allowlist rejects allocation/blocking calls. Process exit/fault cancels ownership.

This remains deterministic virtual scheduling. Full hardware exception/supervisor
stack semantics, interrupt overrun/coalescing equivalence, PPC MSR interrupt
masking, additional display rates/slots, direct foreign queue mutation,
AttachVBL/DoVBLTask, general interrupt-safe Toolbox coverage and production Mixed
Mode remain unfinished. No full Vertical Retrace Manager Gestalt capability is
advertised. Only implemented trap entries/imports are exposed.


### Display selection by global rectangle

GetMaxDevice ($AA27 and the PPC import) uses one shared device operation. It
reads the global Rect before lazy allocation, returns the registered host screen
for positive-area intersection and nil for disjoint/empty/inverted rectangles.
The current host supports one screen, so it is necessarily the deepest intersecting
screen. Application-owned offscreen selection does not participate or change.
An unregistered list head/next link fails explicitly; multiple displays remain
unfinished. This follows Inside Macintosh's greatest pixel-depth rule and QD13's
one-pixel intersection/nil rule, not a greatest-area heuristic:
https://leopard-adc.pepas.com/documentation/mac/QuickDraw/QuickDraw-288.html
https://leopard-adc.pepas.com/technotes/qd/qd_13.html


### Detached device ownership

NewGDevice(-1 mode) allocates independent GDevice, PixMap, empty ColorTable and
inverse-table handles in the shared allocator. The caller initializes the fields;
no screen pixels/palette are cloned and the display list/current selection stay
unchanged. Failed partial allocation returns nil, retains MemError and reclaims
all handles. DisposeGDevice/DisposGDevice release the owned graph, preflight
writable masters, and reject main/foreign/current devices or replaced supporting
handles. General supporting-handle replacement/ownership transfer is unfinished.
Driver-backed modes fail explicitly. The rebuild still has one application heap;
the documented system-heap distinction and driver initialization remain open.
Sources: Apple NewGDevice (QuickDraw-279), InitGDevice (QuickDraw-280),
DisposeGDevice (QuickDraw-283), mirrored at leopard-adc.pepas.com/documentation/mac/QuickDraw/.


DisposeCTable now shares checked disposal through both CPU adapters. It validates
the live ColorTable extent and protects registered screen/structural storage.
Detached ownership forgets the disposed table before handle reuse; PixMap fields
remain caller-owned. Device disposal can adopt an independent replacement or nil
and rejects cross-device replacement sharing. Borrowed/replaced PixMap/inverse
lifecycles remain unfinished. References: Apple QuickDraw Reference,
https://leopard-adc.pepas.com/documentation/Carbon/Reference/QuickDraw_Ref/QuickDraw_Ref.pdf
and QD13's offscreen table replacement example:
https://leopard-adc.pepas.com/technotes/qd/qd_13.html


### PixMap metadata before storage attachment

Indexed PixMap parsing validates dimensions, row capacity, format and the live
palette without reading pixels. Color-port default color selection uses that
metadata view, so a caller can initialize a port before attaching its offscreen
pixel buffer. Source/destination drawing still unpacks checked guest storage and
fails on an inaccessible buffer. Both CPU adapters exercise this initialization
order; a unit regression proves nil storage remains invalid for actual drawing.


### Shared software numerics

Process owns a CPU-independent Numerics service. The native Elems68K adapter
recognizes FATANX selector $001E, reads the destination by address, validates ten
writable bytes before popping its six-byte frame and invokes shared extended
arctangent. No CPU registers are part of numeric evaluation. Unsupported selectors
fault explicitly. PPC math imports are not yet connected to this service.

The initial operation uses the pinned MIT astro-float-num 0.3.7 library with
optional random/serde features disabled (four added lock entries, no upgrades).
It accepts the 10-byte sign/exponent plus explicit 64-bit significand directly,
computes nearest-even at 64-bit precision, and retains low bits that f64 would
lose. The dependency is pure Rust; no host C math or legacy runtime is used.
Signed zeros, infinities, quiet/signaling NaN payloads and subnormals have explicit
handling. Tiny inputs round back to themselves; noncanonical unnormal and
pseudo-denormal operands fault. Process status retains inexact/underflow/invalid,
but public SANE environment get/set, alternate rounding/precision, exception halt
handlers, FP68K arithmetic/conversions and other elementary functions remain
unfinished. This does not advertise complete SANE conformance or PPC math parity.
The 32-bit word adapter is source-defined but a wasm target build is unverified.

Primary interface: pinned MPW SANEMacs.a FATANX / FOATANX and Apple Numerics Manual,
2nd edition, chapter 24, one-address elementary functions. Library API evaluation:
https://docs.rs/astro-float-num/0.3.7/astro_float_num/struct.BigFloat.html
https://github.com/stencillogic/astro-float


### Exact extended multiplication

FP68K FMULX ($A9EB/$0004) now invokes the same process numeric service. The
native adapter preflights both ten-byte operands and writable destination before
popping its ten-byte selector/address frame. The service snapshots both operands,
including aliases, multiplies the complete 64-bit significands into u128 and
rounds once to the extended result. Subnormal results round directly to the
minimum quantum, avoiding double rounding. Sticky underflow follows inexact
tininess after rounding; overflow returns signed infinity in default nearest mode.
Signed zeros, infinities and NaNs are explicit; invalid zero-times-infinity uses
SANE NANMUL code 8. With two NaNs, destination preference chooses one input as
permitted by the manual, quieting signaling NaNs and retaining invalid status.
Public SANE environment/alternate rounding/precision/halt controls, other FP68K
selectors, noncanonical encodings and PPC math imports remain unfinished.
Primary specification: Apple Numerics Manual, 2nd edition, chapters 5, 7, 19–20;
pinned SANEMacs.a FMULX macro. Manual mirror:
https://manuals.plus/m/1f06cc272b1e59ae58f6379edca5b7829815540f233f0679c0f5f26cb9dbcbde


FP68K also marshals FX2D ($0810) and FD2X ($080E) through this shared service.
Extended-to-double conversion rounds the original significand directly to binary64,
including gradual underflow, overflow and NaN payloads. Double-to-extended expansion
is exact for all canonical finite binary64 values, including the smallest
subnormal. Source snapshots permit in-place use; the adapters validate actual
8/10-byte extents and only write the destination's format size. No host float
arithmetic computes the conversions. Current rounding remains nearest-even;
public environment controls and PPC math imports remain unfinished.


### Extended division and integer expansion

FDIVX ($A9EB/$0006) shares process numerics with other SANE slices. Canonical
significands normalize using integer shifts; a u128 numerator yields an exact
quotient/remainder for nearest-even rounding. Tiny results round directly at the
extended minimum quantum. Signed zeros, infinities and propagated NaNs are
explicit; 0/0 and infinity/infinity produce NAN division code 4, while finite
nonzero/zero sets a distinct sticky divide-by-zero status. Overflow and inexact
after-rounding tininess retain the existing policy. Independent astro-float
normal-range division vectors verify the implementation without a host f64 path.

Integer-to-extended format selectors $200E/$280E expand signed 16/32-bit integers exactly
(the pinned macros combine FOZ2X with FFINT/FFLNG). Adapters read exactly two/four
bytes, validate ten output bytes and retain the common ten-byte frame. Public
SANE environment/alternate rounding/precision/halt APIs, other source formats and
PPC math imports remain unfinished. No complete SANE compatibility is advertised.


### Extended trig and copy conversion

Elems68K FSINX/FCOSX/FTANX ($0018/$001A/$001C) now share process numerics and
astro-float's 64-bit nearest-even evaluation with FATANX. Large-argument reduction
retains the original extended input, and result sign comes from the computation
rather than the input (cosine and tangent can change sign). Signed zeros and tiny
inputs have explicit handling; cosine of zero is exactly one. Infinite trig
arguments produce SANE NANTRIG code $21 and invalid status; NaN payloads quiet as
required. The minimum tiny-input branches retain subnormals without narrowing.
Independent Python Decimal Machin/Taylor calculations generate checked vectors
at two working precisions, including 2^64, 2^1000 and one-bit changes near one.
They are mathematical references, not proof of ROM-identical approximations.

FP68K FX2X ($000E) snapshots and copies ten bytes while detecting and quieting a
signaling NaN; it is not an unchecked BlockMove alias. Actual 10-byte bounds and
frame/register preservation are exercised through the native adapter. Public SANE
environment/alternate modes/halt APIs, noncanonical encodings, other operations
and PPC math imports remain unfinished; no complete SANE capability is advertised.


### Native SANE classification

FP68K FCLASSX ($001C) reads the canonical ten-byte operand and writes a signed
16-bit result: magnitude 1 signaling NaN, 2 quiet NaN, 3 infinity, 4 zero,
5 normal, 6 subnormal; the source sign determines the result sign, including -4
for negative zero. These are MC68000 trap codes, not SANE.h's high-level NumClass
enum 0–5. The inquiry neither quiets a signaling NaN nor raises exceptions.
Both pointers are checked before the native ten-byte frame is popped. Other
source formats and noncanonical encodings remain unsupported explicitly.
Primary reference: Apple Numerics Manual, chapter 20, p. 153/table 20-2;
pinned SANEMacs.a FCLASSX and C SANE.h (the latter for the enum distinction).


### SANE environment state, default arithmetic mode

FGETENV/FSETENV/FPROCENTRY/FPROCEXIT ($0003/$0001/$0017/$0019) now share process
numeric state. All take one address of a 16-bit word on MC68000 (six-byte native
selector/address frame). Get serializes real sticky flags into bits $0100–$1000;
Set replaces them; Entry writes the actual saved state before clearing flags;
Exit restores the saved flags and unions exceptions raised by the procedure.
An unwritable save destination cannot clear live state. Tests include real
zero-divisor arithmetic between Entry and Exit and exact native frame cleanup.

Only the implemented default nearest/extended/no-halt controls can currently be
installed. Alternate rounding/precision, halt enables and reserved bits are
rejected before changing state; they are not accepted as ignored controls. Low-
memory environment storage, exception-test/set APIs, halt vectors, full mode
support and PPC numeric imports remain unfinished. Primary specification:
Apple Numerics Manual, chapter 22, pp. 162–165, including the MC68000 address
operand convention (different from the 65C816 by-value exit convention).


FCMPX/FCPXX ($0008/$000A) compare canonical extended operands in shared numerics,
returning a logical Less/Equal/Greater/Unordered relation. The native reply adapter
owns XNZVC encoding: $19/$04/$00/$02 respectively, retaining upper SR bits and
integer registers. Both operand addresses may be readonly; signed zeros compare
equal and negative magnitude order reverses. Quiet comparisons raise invalid only
for signaling NaNs; signaling comparisons raise it for any unordered NaN pair.
Neither input is quieted or rewritten. Primary: Apple Numerics Manual, chapter 20,
pp. 151–152/table 20-1. Other operand-format variants remain unfinished.


### Exact extended addition and subtraction

FADDX/FSUBX ($0000/$0002) snapshot both canonical operands and use astro-float's
full-precision alignment/sum/difference before one explicit nearest-even rounding
step. The result is not first rounded to 64 bits and then rounded again for a
subnormal destination. Raw-word quantization retains guard/sticky information;
valid extended exponent bounds cap alignment at 33,024 bits. Zero cancellation,
signed zero combinations, infinities, propagated NaNs, NANADD code 2 and overflow
are explicit. Tests cover a one-bit subnormal cancellation, halfway ties and the
maximum exponent separation. Native operand order, registers and ten-byte frame
are checked. Other source formats and alternate environment controls remain open.


FSQRTX ($0012) uses the shared software numeric engine at 64-bit significand
precision. It accepts the full extended input range, so even the smallest
subnormal has a normal nonzero square root. Perfect squares do not introduce
inexact; rounded roots do. Negative zero is preserved, positive infinity is
exact, NaN payloads quiet as required, and negative finite/infinite input produces
SANE NANSQRT code 1 with invalid status. The native adapter checks ten writable
operand bytes before popping its six-byte selector/address frame. Alternate
rounding/precision and noncanonical representations remain unsupported explicitly.


### Typed SANE arithmetic sources

The native FP68K adapter now decodes operation and source format separately for
add/subtract/multiply/divide: canonical extended, binary64, binary32, signed 16-bit
integer and signed 32-bit integer. A typed shared NumericBinary call snapshots the
actual source extent and extended destination, expands the source exactly into
canonical extended bytes, and invokes the existing arithmetic engines. It does not
perform a host floating-point operation or round to the source format first.
Source operands remain borrowed/unchanged. Status is staged and committed only
with successful output, including signaling-NaN quieting during source expansion.
Comp and 96-bit formats, mixed-format comparisons, alternate environment controls
and PPC numeric imports remain explicitly unfinished. Legacy public extended-only
entry points retain the same underlying arithmetic helpers; redundant native
arithmetic decoding was removed.


FRTIX/FTTIX ($0014/$0016) round the canonical extended operand to an integral
extended value. Nearest-even is the implemented default for FRTIX; FTTIX always
truncates toward zero. Discarded fractions set inexact without introducing
underflow for a zero result. Signed zero, large exact integers, infinities and
NaN quieting are explicit. Both use checked ten-byte operands and six-byte native
frames. Alternate environment rounding remains rejected rather than ignored.


### Finite signed integer storage conversion (2026-10-16)

FX2I/FX2L decode two-address native frames into a shared typed service, which
snapshots the ten-byte source and preflights only the two/four-byte destination.
Default nearest-even integral rounding precedes a signed range check; conversion
uses the original extended significand, without a binary64 intermediate. Signed
zero and tiny values store zero. Status changes commit with output only.

Apple Numerics Manual, second edition, chapter 3 (p. 22) specifies rounding before
integral range checking; chapter 21 (pp. 156–157) specifies address operands and
integer widths. Source: https://manuals.plus/m/1f06cc272b1e59ae58f6379edca5b7829815540f233f0679c0f5f26cb9dbcbde

Exceptional integer conversion encodings remain unverified for software MC68000
SANE. NaN, infinity and rounded out-of-range inputs fail explicitly without
changing numeric status or output; they do not claim compatible invalid-result
semantics. Alternate rounding controls and PPC numeric imports remain open.


### Basic data-fork opens and typed comparisons (2026-10-16)

PBOpenDFSync ($A060, selector $001A) uses the same shared data-fork open service
as PBOpenSync; PBHOpenDFSync ($A260/$001A) retains its hierarchical form. hfsBit
selects the parameter-block contract: the basic call reads 28 bytes and follows
the classic volume/working directory; the hierarchical call reads 52 bytes and
uses ioDirID. The PPC PBOpenDFSync import resolves to the basic shared operation.
Apple's Files.h in the pinned MPW image confirms both inline trap sequences;
evidence: /tmp/systemless-opendf-sdk.txt. Async variants remain unsupported.

Native FCMP/FCPX source format decoding now shares arithmetic's typed expansion
for Extended/Double/Single/Integer/Long. A shared NumericCompare call returns a
logical relation; only the ABI applies MC68000 condition codes. Both guest inputs
can be readonly, with exact 10/8/4/2/4-byte source extents and no writeback. Native
frames consume ten bytes and preserve unrelated registers and upper SR. Canonical
extended precision survives mixed comparison, including the bit above 1.0 that a
binary64 intermediate would discard. Quiet/signaling NaN rules and sticky status
are staged until the comparison succeeds. Typed source expansion is now one
helper shared by arithmetic and comparisons. Comp, 96-bit formats, alternate
controls and PPC math imports remain explicit gaps. SDK macros for FCMPD/FCPXD
are captured at /tmp/systemless-typed-compare-sdk.txt; Apple Numerics Manual,
second edition, chapter 20 specifies relations and signaling behavior.


### Explicit wgpu text completion requirement (2026-10-16)

The user added complete text rendering through wgpu to full-rebuild acceptance.
The supported text path already composes destination pixels through wgpu, while
font decoding/layout/source coverage remain CPU work. Complete API/UI coverage
and an evidence-based library evaluation are now required before goal completion.
See [text-rendering.md](text-rendering.md) for scope, Glyphon/Vello primary-source
findings, implementation milestones and concrete pixel/CPU/platform acceptance.


### Resource-backed GetCTable (2026-10-16)

GetCTable ($AA18 and the PPC import) calls the shared device manager without
requiring display initialization. It resolves clut IDs through the current
Resource Manager search chain, validates the live owned handle and declared
ColorSpec extent, then copies the payload through the shared allocator. Each
result has independent master/data storage, cleared resource/lock/purge flags,
and the source's seed, flags and ColorSpec values. Live guest edits to a loaded
resource are visible to subsequent copies. Disposal uses existing DisposeCTable
ownership checks; the resource and other copies remain independent.

Apple's [Color QuickDraw reference](https://developer.apple.com/library/archive/documentation/mac/pdf/Imaging_With_QuickDraw/Color_QuickDraw.pdf),
pp. 4-92–4-93 and 4-104, defines resource lookup/NIL, disposal and resource fields.
Resource seeds are retained, unlike the legacy reference's unconditional fresh
seed assignment. No native-System conformance claim is made. Malformed extents
fault explicitly. Missing custom resources return NIL/ResError; allocation failure
returns NIL/MemError. Error-global destinations and native output cells are checked
before service allocation. Unbacked standard system tables remain explicitly
unsupported, including enhanced/default tables; they are not reported as ordinary
missing custom resources. A failed resource load due to allocation is preserved as
an allocation failure even for an otherwise standard ID.


### Executable Glyphon compatibility seam (2026-10-16)

The isolated future/prototypes/glyphon executable pins Glyphon 0.12.0, wgpu 30.0.1
and its own dependency graph. It decodes a synthetic classic FONT with the shared
font service, supplies one-bit masks to Glyphon custom glyphs, and verifies exact
GPU output without installed fonts or modern shaping. Production text APIs and
dependencies are unchanged. An edited FONT with a reused custom key retains cached
pixels; assigning a new ID refreshes them. Stock repeated composition is not
QuickDraw XOR. A production integration therefore needs bounded source-generation
cache identity and a GPU mask-to-QuickDraw transfer seam, not direct substitution
of RGBA text painting for indexed guest semantics. Pixel/layout/platform/performance
acceptance remains open. See text-rendering.md and the prototype README.


### GPU-resident text mask transfer boundary (2026-10-16)

Ports::copy_gpu_mask accepts a same-device binary red-channel Rgba8Unorm texture
and applies the eight supported source transfer modes through the shared packed
Surface and Graphics pipeline. The new CopySource variant selects GPU texture
loads inside the existing Blit kernel; RGB matching, inversion, region clipping
and destination semantics remain shared with CPU bitmap sources. An unused
one-pixel source texture keeps the ordinary pipeline's bindings valid. Scratch
targets share its device/pipeline and validate the actual source/output texture
identity. Format, usage, sample/layer shape, size and output-alias errors are
rejected before dispatch. Masks are binary coverage; antialiased/color glyphs
remain outside this interface's contract.

Guest bitmap origins, portRect, clip/visibility, colors and packed 1/2/4/8-bit
storage use the existing drawing-target path. CPU work only validates and packs
storage; transfer/composition runs on wgpu. Intermediate mask pixels are never
read back or reuploaded by the bridge. Destination readback remains the existing
synchronous guest-memory boundary, observing direct guest writes between calls.

The real Glyphon probe now renders on the Graphics device and feeds that texture
directly into the transfer boundary. It demonstrates all eight monochrome modes,
XOR cancellation and intervening guest writes. Production DrawString/TextEdit/UI
calls still use the prior GPU coverage path. Choosing the library-backed path
requires a bounded source-generation cache, cell/layout integration, actual ABI
acceptance and workload measurement. No second graphics backend is introduced.


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
