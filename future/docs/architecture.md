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
before either is written. Asynchronous I/O, directory operations, guest open/read/
write/close and persistence remain pending. This is a virtual volume model, not an
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

## Packed Basic QuickDraw bitmap boundary

`bitmap.rs` reads the 14-byte guest BitMap record and validates its base address,
row stride, signed bounds and mapped image extent. The current GPU extent limit
is 4096 in either dimension; empty/inverted bitmaps and PixMap flags fail
explicitly. Commands use the bitmap's local coordinates. Translation to GPU
coordinates includes pattern phase, so nonzero or negative origins do not shift
an 8x8 pattern incorrectly.

Basic QuickDraw's packed one-bit rows are decoded to temporary index bytes,
processed by the existing wgpu compute commands, then packed back into the
canonical guest bitmap before returning. CPU work is storage conversion only,
not a drawing fallback. Padding bytes and unused bits of partial rows retain
their prior values. Each draw reloads guest bytes; a direct guest write between
operations is therefore visible. A different bitmap extent uses a temporary texture on the same GPU device;
the synchronized guest result outlives this scratch target.
A monochrome presentation uses palette index 0 for white and 1 for black.

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

The current screen policy reserves packed pixels for a 512x342 monochrome
screen. No GPU is acquired during loading or initialization. An embedding host
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

`--code` and `--pef` accept `--window`, selecting a 512x342 monochrome GPU
and the existing nearest/aspect-preserving surface pipeline. Each redraw runs
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


## Environment profile and fixed heap reservation

SysEnvirons describes one virtual Macintosh compatibility world rather than
branching on the calling CPU. Its legacy processor field denotes the 68020
compatibility backend; the native PPC engine retains its own execution state
and semantics. Public capability flags describe implemented Macintosh services,
so internal GPU palette support does not advertise Color QuickDraw. Unknown
machine/keyboard values avoid pretending to emulate a particular hardware model.
Version errors/layout follow Apple's Gestalt Manager reference and OV16.

Loaded CODE and PEF processes map the complete fixed 4 MiB heap during common
preparation and publish bounds that match its allocator. MaxApplZone preserves
allocations while reasserting the full mapped extent. It does not create a
second CPU-specific heap or accept a limit the allocator cannot honor. General
zone resizing and SetApplLimit support are still missing. Standalone Process
unit fixtures retain lazy mapping when they are not created by the loader.

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
Directories and newly created files are in-memory process state: application
bytes remain read-only. File deletion, persistence, working-directory references
and MacRoman folding remain unfinished.

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
