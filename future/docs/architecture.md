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
resource reference, not a fabricated empty open file. Host paths are not exposed.

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
through `elapse_ticks`. Only a matching event or expiration completes the
request. Unmatched events remain queued. A zero sleep never yields. The event
record and deferred result are validated before consuming an event.

Wait remaining time is a saturating duration; guest TickCount remains a wrapping
32-bit value. Time advances explicitly while idle and through the existing
instruction clock while executing. No host wall clock or sleep is hidden inside
the runtime. The file CLI advances directly to wakeup times, retaining a single
instruction budget across all yields. Interactive host pacing remains pending.

This implements low-level event delivery and null-event timeout in the current
windowless process. Mouse-region handles currently fault explicitly; region
containment and mouse-moved events must be added with QuickDraw regions.
Window update/activation, high-level events, system event interception and
GetNextEvent/EventAvail are still pending. Supporting a wait is not a claim of
complete Toolbox Event Manager compatibility.

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
translation, cursor imagery and Window Manager dispatch are not implemented.
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
unsupported. SetOrigin, MovePortTo and Window Manager placement are still pending.

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
Synthetic styles, fractional spacing and DrawText/StdText remain explicit gaps.


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
