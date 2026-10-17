# Unified replacement runtime

Baseline: `634c0e9120a695224d71c621fbbe33709670d707` in the original
`/Users/ben/.codex/worktrees/b2f1/systemless` checkout. Implementation branch:
`dev/future`, separate managed worktree. Tracking issue: #4801. No merge,
release, or website deployment is authorized. The independent `future/`
workspace is the replacement build; the legacy sources remain reference-only
until replacement compatibility justifies retiring them from this branch.

Current native-32-bit profile: the executable AB03 identity entry is installed,
backed by the implemented direct/managed-world path. The final section records
its additional primary-source rationale and scope; earlier absence notes describe
prior milestones. Complete QuickDraw and full-rebuild acceptance remain open.

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
synchronous PBFlushVol now suspends guest execution until the host checkpoint
acknowledges success or a Macintosh I/O error. Asynchronous I/O and complete
per-service flush/error semantics remain pending.
This is a virtual volume model, not an
HFS disk-image implementation. Source fork bytes remain separate from mutable
resource handles until Resource Manager update/close commits them. PBFlushVol
flushes committed file/catalog state; it does not replace UpdateResFile.

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

`font` parses immutable snapshots of guest FONT/NFNT bytes and exposes metrics,
glyph geometry and one-bit source masks. `font_family` resolves FOND associations
through live Resource Manager handles, preferring intrinsic plain/bold styles and
sfnt outlines, then exact-size NFNT/FONT strikes. Arbitrary association IDs do not
use legacy family*128+size arithmetic. Only families without FOND use that old
lookup (when representable), then the existing explicit substitute policy.

The checked FOND reader validates family IDs, character ranges, association
ordering/counts and optional table offsets. It reads family width subtables using
Apple Inside Macintosh: Text and
[Adobe technical note 0091](https://adobe-type-tools.github.io/font-tech-notes/pdfs/0091.Mac_Fond.pdf).
The width table includes each character, a missing-character width and an unused
zero word. Active family widths are scaled from 4.12 fixed point and rounded to
integer advances; disabled/absent tables use outline horizontal metrics. Existing
fractional-spacing restrictions remain explicit.

## Font initialization and source ownership

Font Manager initialization and text selection share family resolution. The pinned
ab_glyph 0.2.32 library decodes guest sfnt outlines as well as the licensed bundled
Nimbus Sans substitute. It prepares binary source masks at 72 dpi; wgpu performs
destination drawing. The current converter covers MacRoman printable characters
through Unicode mappings and a real missing glyph, sizes1..96, with bounded
metrics/glyph extents/strike storage. Outline vertical metrics come from the sfnt
and enclosing glyph bounds. Classic TrueType hinting, FOND global-metric parity,
non-Unicode cmap fonts, bitmap scaling, color/auxiliary strikes and full styles
remain compatibility work; no native pixel-parity claim is made.

Fallback strikes remain editable guest handles. Prepared outline strikes are
bounded host source data: at most eight entries and8MiB including keys/payloads.
Each key contains exact live FOND and sfnt bytes, size and intrinsic style. Thus
resource edits, purges/reloads and changed resource owners cannot reuse stale
coverage merely because a family/resource/handle ID is unchanged. There is no
second architecture-specific font state. Default family/size globals are read on
each selection. Bold synthesis only applies when an intrinsic bold association
was unavailable, and does not modify source resources.

Cross-map font-suitcase association priority remains unverified; the original EV
font evidence uses FOND and sfnt resources from the same application fork.


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
without clearing new invalidation. Neither operation selects a graphics port or
graphics device: the caller may keep an offscreen port selected while bracketing
updates to a window. Current-port storage is not an output of these operations. Host metadata retains allocation identities
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


### Synchronous volume flush handshake

PBFlushVol (A013), PBFlushVolSync and synchronous PBFlushVol imports share one
volume-selection/preflight service (IM Files p. 2-143). A valid request clears
ioCompletion and publishes an internal pending ioResult, saves its typed reply,
and yields Stop::FlushVolume. Repeated advance calls return the same stop without
executing instructions or dispatching callbacks until complete_volume_flush is
called. Invalid volumes return their catalog error without a host request; invalid
guest pointers/output permissions fault before mutation.

The host volume adapter checkpoints canonical files using its synchronized atomic
replacement, then publishes noErr or ioErr (-36) to ioResult and the original CPU
ABI. Windowed and headless launch paths use this adapter. Failed checkpoints leave
canonical changed data available for retry. Automatic checkpoint errors outside a
guest flush retain their host-error behavior. No host paths enter guest services.
Without a backing volume, hosts explicitly acknowledge the canonical in-memory
volume barrier; this does not claim disk durability. The diagnostic tracer prints
that policy and its optional final snapshot remains a separate export operation.

This boundary flushes committed catalog/fork bytes, including open access paths,
without changing timestamps, marks, reference identities or dirty Resource Manager
handles. Resource Manager maps still require UpdateResFile/CloseResFile before
those changes are file contents. Asynchronous completion, additional volumes,
physical-drive details and full per-service File Manager durability remain open.


### Region outline recording

OpenRgn/CloseRgn are shared process services with void native/PPC ABIs. One active
recording owns a temporary guest handle and its originating port; rgnSave exposes
that handle using a private checked edge-buffer layout. Open hides the pen and
close balances visibility, replaces an existing owned destination region and
releases the temporary handle. SetPort naturally suspends collection when drawing
into another port. Saving rgnSave, setting it to NIL and restoring it also suspends
and resumes collection. Nested region/polygon/picture recording and custom
procedures fail explicitly rather than combining incompatible state.

Lines contribute mathematical endpoints; framed rectangles, round rectangles and
polygons contribute boundaries independent of pen size/pattern and clip/visible
regions. Integer-coordinate even-odd scan conversion preserves disjoint areas,
holes and negative coordinates, using wide rational intersections. Vertical spans
coalesce before the shared inversion-region encoder runs. This is source geometry
preparation: visible pen drawing still uses wgpu and guest pixel synchronization.

The temporary buffer is 32,760 bytes with at most 4,093 nonhorizontal edges;
recording and scan-conversion work limits fail explicitly. The current shared
region codec retains its 32-KB limit even for color ports. Full 64-KB color-region
support, regionTooBigError/QDError publication, arbitrary custom procedures and
remaining oval/arc/region APIs are not claimed. FrameArc is excluded from region
outlines by the Macintosh contract. Close rejects foreign or resized recording handles;
failed destination resize keeps the active recording for retry. Actual source
recording bytes are private runtime metadata, not a ROM-layout parity claim.

References: [OpenRgn](https://dev.os9.ca/techpubs/mac/QuickDraw/QuickDraw-133.html)
and [QuickDraw Drawing, CloseRgn p. 3-89](https://woofle.net/impdf/QD-03-Drawing.pdf).


### Selective Window Manager visibility calculations

CalcVis A909 and CalcVisBehind A90A / matching PPC imports use one typed
WindowVisibility request and the same geometry planner as internal window
recalculation. The planner walks the live WindowList front to back, subtracts
visible structure coverage from content intersected with the desktop, and converts
the result to each port's local coordinates. Single-window calculation stops at
its target. Behind calculation starts at the specified list entry and selects
windows whose actual structure region intersects the supplied global damage
region; holes in damage are respected. NIL start denotes an empty suffix, and
unknown non-NIL windows or corrupt region/list storage fail explicitly.

All input geometry and owned output handles are validated and staged before one
transactional replace_handles commit. A later malformed record or allocation
failure leaves earlier visibility outputs unchanged. Nonselected windows, pixels,
current port, update regions and window ordering are not rewritten. Geometry-only
calls do not require a GPU. Both CPU adapters decode void ABIs into this planner.

During BeginUpdate, recalculation intersects the narrowed active visibility with
new full visibility and updates the saved full region used by EndUpdate. It does
not broaden the active update drawing area. This is the rebuild's explicit update
preservation policy; it is not a claim of every undocumented System 7 interaction.
Custom WDEF/layering behavior, broad Window Manager expansion and native/browser
interaction acceptance remain incomplete.

Primary references: [CalcVis](https://dev.os9.ca/techpubs/mac/Toolbox/Toolbox-287.html)
and [CalcVisBehind](https://dev.os9.ca/techpubs/mac/Toolbox/Toolbox-288.html).


### Exposed window and desktop repainting

PaintOne A90C / PaintBehind A90D and matching PPC imports use one typed Repaint
request. The manager snapshots global damage, clips it to the desktop, subtracts
visible structures above each selected window, and prepares exposed content and
frame geometry. PaintOne selects one window; PaintBehind selects a list suffix.
NIL denotes desktop repaint. Desktop drawing subtracts all visible window
structures. All clipping uses inversion-region geometry, including holes.

PaintOne uses the word controls SaveUpdate ($09DA) and PaintWhite ($09DC): exposed
content is added to updateRgn only when SaveUpdate is set, and background erasure
occurs only when PaintWhite is set. PaintBehind suppresses content erasure and
sets PaintWhite after its drawing operation. It still repaints frames, schedules
updates when enabled and paints exposed desktop. InitWindows initializes both
controls and the eight-byte DeskPattern at $0A3C. Desktop paints read that live
pattern rather than a separate renderer constant.

Update-region growth is staged for every selected window before destination
pixel operations. Invalid geometry, unsupported custom WDEFs and allocation
failure are reported before drawing. The existing wgpu frame/chrome/background
renderer performs the pixels; content erasure is now an explicit option shared
with ordinary window creation/geometry changes. The current port and manager-port
record are restored by the existing drawing boundary. GPU/runtime faults are
terminal and are not claimed to roll back pixels or previously published updates.

This supports the rebuild's managed standard window definitions and indexed
surface path. Custom WDEF callbacks, window pictures, desktop hooks, broader
layering/theme behavior and full native/browser UI acceptance remain unfinished.
Primary [PaintOne](https://dev.os9.ca/techpubs/mac/Toolbox/Toolbox-285.html) and
[PaintBehind](https://dev.os9.ca/techpubs/mac/Toolbox/Toolbox-286.html) describe the
separate erase/update controls and clipped frame/content behavior.


### CopyBits color-port bitmap references

CopyBits accepts either direct BitMap/PixMap records or the documented
GrafPtr(colorPort)^.portBits reference. In the latter form, both high bits of
the apparent rowBytes word are set: it is actually portVersion, while the apparent
baseAddr is a PixMapHandle. A shared checked descriptor resolver follows that
handle once, requires mapped non-NIL handle/data and an actual 50-byte PixMap,
then invokes the existing strict indexed decoder. Low portVersion bits are not
mistaken for stride. Source and destination adapters preserve their input bytes.

Normalization is specific to CopyBits reference entry points. Typed display,
GDevice and port PixMap readers retain direct-record validation and reject aliases.
No recursive alias chain is accepted. Underlying depth, packing, component,
stride/bounds, palette definition/matching, storage permissions and size checks
remain unchanged. Each call rereads the live master pointer; metadata is not cached
across handle relocation or guest edits. Raw bitmap references remain supported.

The same normalized surfaces reach the production wgpu transfer pipeline from
both CPU ABIs. Masking, overlapping-copy snapshots, color transfer semantics and
CPU/GPU synchronization use the existing shared implementation. Direct-color
formats, dithering and broader CopyMask/CopyDeepMask expansion are not completed
by this reference-form change. Diagnostic tracing now shows failing CopyBits
headers and possible handle targets without changing guest execution or storage.

Primary: [CopyBits](https://dev.os9.ca/techpubs/mac/QuickDraw/QuickDraw-166.html),
which describes the two high portVersion bits and PixMapHandle indirection.

### FSSpec Finder information (2026-10-16)

FSpGetFInfo (HighLevelHFSDispatch selector7 and PPC import) uses the shared
catalog projection also used by parameter-block file/catalog information. It
returns the original 16-byte FInfo/DInfo, not extended Finder fields, without
opening either fork. Canonical FSSpec lookup is shared with deletion. Native
arguments and the Pascal result slot are checked before stack adjustment;
destination ranges are checked before lookup/writes. Lookup errors preserve
output bytes. Files return their stored Finder record; directories/root currently
have default zero DInfo because nondefault folder metadata import/mutation has
not been implemented. That limitation remains explicit rather than returning a
fabricated file type/creator for folders. See Inside Macintosh: Files p.2-160.

### Packed indexed picture input (2026-10-16)

PICT BitsRect/BitsRgn and PackBitsRect/PackBitsRgn decode indexed1/2/4/8-bit
sources through one bounded parser. Single components must match pixel depth;
row capacity uses bits, high-order pixels are extracted first, unused row bits
are excluded from image indices and referenced palette entries must be defined.
CPU work decodes source storage; existing shared GPU palette conversion,
clipping, scaling and transfer operations still own destination drawing. This
adds no direct-color picture path or unsupported-opcode fallback.

### Resource-backed SndStartFilePlay request (2026-10-16)

Native selector0D000008 and the PPC import marshal all eight arguments into
sound::StartFilePlay. The supported resource-backed path resolves snd resources
through the shared Resource Manager and uses the existing command FIFO, mixer,
completion accounting and synchronous runtime wait. It preserves caller MemErr
and ResErr; nil-channel asynchronous calls return badChannel and missing resources
return resNotFound. This is not a second CPU-specific sound implementation.

AIFF/AIFF-C disk streaming, caller-provided input buffers, audio selections and
file-play completion routines fail explicitly until implemented. Resource source
playback currently shares SndPlay buffering/lifetime behavior; file-specific
status, pause/stop, buffering and callbacks remain unfinished. Full file-play
compatibility and original EV audio acceptance must not be inferred from the
resource fixture or the game advancing past a missing resource.

### Owned-zone MaxMem and purged resource identities (2026-10-16)

The current application zone is eagerly grown to its owned reservation. MaxMem
stages unlocked purgeable payload release and address-ordered handle compaction
inside fixed-allocation intervals, then returns the largest actual free extent
with0 remaining growth. Pointer blocks, master cells and locked payloads stay
fixed. Every live master/range and all source snapshots/placements are checked
before publishing heap metadata or writing relocated bytes. Custom/modified zone
bounds fail explicitly; multiple/system zones and grow/purge hooks remain open.

Purging preserves master identity and flags, clears payload pointer/size and
releases only payload storage. GetResource reloads an empty resource into that
same handle from its source map. Failed allocation preserves empty identity for
retry. Automatic writeback on purge and SetResLoad(false)/explicit LoadResource
are unfinished; unpersisted edits to purgeable source data may be lost.

Both CPU adapters use this same service. Native A01D preserves A0; A11D returns
current growth0 there, with the documented TST.W condition codes. Trap Manager
p8-13 specifies this bit8 distinction. PPC validates its nonnil4-byte grow output
before any purge/relocation; runtime preflights MemErr publication and clears it
on success, preserving ResErr. This is owned application-zone behavior, not an
assertion that all Memory Manager zone/compaction/purge APIs are complete.

### Resource file-play pause state (2026-10-16)

Successful asynchronous SndStartFilePlay resource requests retain their own
completion identity and pause flag on the shared channel. SndPauseFilePlay toggles
only active file-play work, returns channelNotBusy(-211) for valid idle or ordinary
SndPlay channels, and badChannel(-205) for invalid channels. Both ABIs share this
state; native frames preflight the result before popping. Paused channels preserve
sample phase, queue order and pending callbacks across rendered and silent time.
Status reports the actual pause bit. Completion/quiet/flush makes the old identity
inactive, so later ordinary channel work is not misclassified as file play.

This remains resource-backed file-play support: AIFF streaming/position times,
file completion routines, caller buffers/selections and stop semantics are open.
Resource source lifetime still follows the existing live SndPlay path; automatic
file-play source pinning/owned buffering across compaction/purge must be finished
before complete file-play compatibility or original music acceptance is claimed.

### Declared application partition requests (2026-10-16)

Application loaders share SIZE selection: ID0 overrides ID-1; the selected
10-byte flags/preferred/minimum record is validated and retained in LoadInfo.
Compressed/invalid/reversed or above64MiB requests fail before guest execution.
The current bounded launcher grants preferred size aligned to16bytes; it has no
minimum-size fallback/consent dialog yet. Scheduling/addressing flags are retained
metadata, not implemented Process Manager behavior.

Declared CODE partitions place the heap at10000, A5 world and64KiB stack at the
top of the actual reservation. Eager executable sections start at16MiB or beyond
the relocated stack, whichever is later. Checked arithmetic/mappings reject
impossible worlds and overlaps; CurrentA5, CurStackBase, HeapEnd, ApplLimit and SP
follow the layout. PEF uses the same request to reserve a real preferred-minus-
stack application heap at its separate heap base, retaining its existing stack
address. LoadInfo exposes the actual stack range and selected resource metadata.

Unspecified-SIZE host defaults remain the existing runtime policy, not System7's
512KiB default. Full partition accounting for the current eager executable images
outside the heap/stack reservation, native absent-SIZE launch policy, user minimum
fallback and all SIZE operating flags remain unfinished. No free-memory query is
inflated to compensate for a loader layout; it reads actual allocator extents.

### Software-SANE integer exceptional results (2026-10-16)

Native FP68K extended-to-integer storage conversions now use the software-SANE
profile described by Apple Numerics Manual pp22/197: NaN, Infinity and rounded
out-of-range values deliver the destination's most negative integer and set
invalid. Exact negative minima remain valid. Newly generated intermediate inexact
is suppressed on invalid conversion while prior sticky state remains; integer
range failure does not raise floating overflow. Source/destination preflight,
alias snapshots, native stack/register preservation and supported nearest-even
rounding remain shared. Alternate rounding/precision and halt handlers are still
unfinished; this does not add native PPC integer/FPU conversion ABI support.

The original EV input was traced to a valid NewPtr(clearFALSE) allocation reusing
old PICT bytes. Allocation semantics were preserved; the fix implements numeric
behavior instead of clearing that memory. Opt-in trace_application operand and
single-address watch diagnostics expose guest frames/bytes/environment/code
without changing production services. Unnormalized extended operands are a
separate remaining compatibility boundary requiring their own format evidence.


### Pixel image ownership

The process owns independent per-PixMap pins for movable pixel-image handles.
LockPixels exposes a stable payload pointer; UnlockPixels restores the image
handle. Compaction, purge, MoveHHi and resizing respect these pins independently
of HLock/HPurge flags. Fixed owned NewPtr images and main-screen storage require
no relocation pin. GetPixBaseAddr reads current ownership without acquiring a pin
and returns nil for purged images. Both CPU adapters use these same services;
existing wgpu operations continue using canonical guest pixels. Full GWorld
lifecycle, pixel-state/purge APIs and external/interior image ownership remain open.


### Full RGB picture sources

Decoded direct-color PICT commands retain sixteen-bit RGB components per pixel.
The GPU source buffer carries three components per pixel and uses a distinct
source-kind flag; existing mono/indexed source and glyph-mask paths keep their
representations. GPU palette matching and transfer modes operate on RGB before
packing the destination's guest indices. No256-color source approximation or CPU
destination renderer is introduced. This supports direct-color sources into the
current mono/indexed destinations; direct-color guest destinations and dithering
remain separate requirements.


### Dithered source transfer

Dithered PICT/CopyBits sources are resampled and error-diffused on the same wgpu
device. Two GPU scratch rows carry deterministic Floyd–Steinberg errors; the
resulting RGB storage buffer feeds the existing transfer pass without CPU readback.
That pass applies guest clipping and transfer/inversion. A full-rectangle phase
precedes region clipping. The first implementation serializes diffusion ordering;
large-image throughput and native pattern parity remain open. Existing geometry/
color-work budgets and device storage-binding limits are checked before dispatch.
The preserved Systemless GWorld algorithm is a comparison reference, not a second
production backend or proof of native QuickDraw equivalence.


### Typed pixel-pattern resource ownership

GetPixPat resolves through the shared live Resource Manager, then constructs an
independent handle graph from checked serialized offsets. The graph owns its
record, PixMap, pixels, color table and empty invalidated caches; it is independent
of resource-file lifetime and survives payload movement through stable masters.
DisposePixPat preflights the complete graph before freeing it. Partial allocation
failure rolls back new graph members. Both CPU adapters marshal into one manager.
This is typed resource-object support; installation and GPU color-pattern painting
remain separate requirements, alongside other PixPat constructors/copy operations.

### Owned color-icon records

The shared `cicon` manager reconstructs compiled `cicn` resources for native
GetCIcon ($AA1E) and PPC GetCIcon. DisposeCIcon ($AA25), including the PPC
DisposCIcon spelling, releases the owned graph. Both adapters marshal into one
service; source lookup remains the normal Resource Manager search over live cached
resource bytes. Each successful Get allocates an independent, nonresource 82-byte
CIcon handle, a pixel-image handle, a ColorTable handle and fixed storage for mask
and optional monochrome image. File placeholders become live pointers/handles;
returning the resource handle itself would expose an invalid graph. Cached resource
pointer placeholders may already contain guest addresses; these values are
ignored and replaced, never dereferenced. Inline mask/table/pixel bytes remain the
source of the fresh graph. EV directly populates iconMask.baseAddr before GetCIcon.

The source format and field meanings follow Apple's [Color Icon Record](https://dev.os9.ca/techpubs/mac/MoreToolbox/MoreToolbox-277.html)
and [compiled color-icon resource](https://dev.os9.ca/techpubs/mac/QuickDraw/QuickDraw-268.html).
This slice accepts bounded, uncompressed indexed 1/2/4/8-bit images, matching mask
bounds and an optional nil monochrome bitmap. It checks all ranges, palette
indices and exact payload length before allocating typed children. Unsupported
packing, direct color and relative tables fail explicitly.

Live PixMap.baseAddr contains a pointer. An internal ownership pin prevents the
pixel image from moving or purging while its CIcon lives; this pin is independent
of guest HLock/HUnlock and does not alter the public handle flags. Masks use fixed
storage and remain stable when the root record or color table moves. This is an
explicit lifetime policy, not verified native allocation-layout or lock-state
parity. In-place guest resizing follows existing handle semantics; structural
mutations outside the supported owned graph are not silently repaired. Disposal
preflights all owned members and expected pointer/handle links before freeing any.
Raw DisposeHandle invalidates root metadata but is not a deep destructor; proper
callers must use DisposeCIcon. PlotCIcon rendering remains a separate open service.

### Shared wgpu PlotCIcon

PlotCIcon ($AA1F/PPC import) uses one service to prepare a live CIcon or a compiled
resource and submit a masked indexed blit to the existing wgpu transfer pipeline.
Source/mask bytes and palette metadata are decoded on the CPU before any target
write; scaling, binary mask rejection, palette conversion, recoloring and every
destination pixel operation run on the GPU. No CPU renderer is added.

The primary [PlotCIcon reference](https://dev.os9.ca/techpubs/mac/MoreToolbox/MoreToolbox-284.html)
was retrieved directly after the web-reader cache failed. It specifies local
coordinates, mask-one coverage, scaling from iconPMap bounds, matching source/mask
sizes, and iconBMap selection for 1/2-bit screens unless its rowBytes is zero.
Selection reads the current GDevice PixMap depth, falling back to the configured
main display before a device exists; it does not confuse a packed offscreen
format with the selected screen depth. A missing monochrome strike uses the color
image. Differing bitmap origins are supported when their dimensions match.

[Apple TN QD18](https://leopard-adc.pepas.com/technotes/qd/qd_18.html) explicitly
states that PlotCIcon honors foreground/background colors, unlike PlotCIconHandle.
This later contract supersedes the contrary older SpInside text for this slice;
shared GPU CopyMask/CopyBits recoloring implements it. Native parity is unproven.
The icon target intersects portRect, visRgn, clipRgn and destination bounds even
for offscreen ports. Per the primary reference, PlotCIcon bypasses bottleneck
procedures and is not recorded in pictures; its target route ignores recording
and custom-procedure pointers without patching port state or invoking them.

Live icons resolve pixels through iconData, with a scoped PixMap base override
that never modifies the guest descriptor. Compiled resources use their inline
mask, optional monochrome image, ColorTable and pixel image; placeholder guest
pointers are ignored. Input is snapshotted before uploads and writes, allowing
shared backing storage without incremental source corruption. Drawing preserves
MemErr/ResErr and does not allocate guest objects. Nil arguments and empty
rectangles are inert; malformed supported inputs fail before pixel writes.
Direct-color source/destination formats, full picture recording elsewhere,
PlotCIconHandle transforms and broader Icon Utilities remain open.

### Button polling and host MBState delivery

Native Button ($A974) and the PPC import share a read of the high bit of guest
MBState ($0172): zero means pressed, one means released. The event subsystem
initializes that byte to $80. Host pointer transitions preflight the state byte,
post the corresponding event when permitted, and update its high bit while
preserving the other seven bits. Masked events still change physical state;
focus loss releases a held button. Polling neither traverses nor consumes queue
entries, so a queued old mouseDown cannot fabricate a currently held button.
Guest driver writes to MBState remain observable through either CPU adapter.

There is conflicting Apple prose. The [Toolbox Button page](https://dev.os9.ca/techpubs/mac/Toolbox/Toolbox-73.html)
says it searches the event queue, but the classic [Apple device-driver note DV520](https://leopard-adc.pepas.com/technotes/dv/dv_520.html)
explicitly requires device drivers to update MBState for Button and defines its
high-bit polarity. The [later Event Manager reference](https://leopard-adc.pepas.com/documentation/Carbon/Reference/EventManagerLegacyRef/event_mgr_legacy_ref.pdf)
also describes current-state polling. This slice follows the explicit classic
driver contract. The earlier experimental queue-only implementation was replaced
before publication. The first original EV replay passed Button but exhausted its budget in the old
queue-only StillDown implementation despite released MBState. StillDown now
follows the preserved Systemless comparison contract: physical press plus no
pending mouseUp. A queued original mouseDown does not end tracking; a pending
release still ends it after a subsequent press. Neither query consumes events.
This distinguishes a verified legacy comparison from native Macintosh parity;
full mouse-driver and event-timing compatibility remain open.

### Shared keyboard map, GetKeys and host delivery

Both CPU adapters dispatch GetKeys ($A976/PPC import) to one sixteen-byte snapshot
of low-memory KeyMap ($0174). Events initialization clears this guest-owned map.
The native procedure consumes one checked pointer argument; PPC preserves its
procedure argument/result registers. Nil, short, unreadable-source and protected
outputs fail explicitly; the complete result is preflighted before writes and a
snapshot handles aliases without incremental corruption. Polling does not touch
event queue entries, errors, allocation state or host-held state.

The [GetKeys reference](https://dev.os9.ca/techpubs/mac/Toolbox/Toolbox-77.html)
describes 128 key states but its prose says to subtract one from the virtual key
code. This rebuild uses the preserved Systemless byte contract instead:
byte=code/8, mask=1<<(code%8), with code0 representing A and code38 J. This is also
consistent with Apple's [keyboard-layout note TN2056](https://leopard-adc.pepas.com/technotes/tn2002/tn2056.html)
defining A as virtual key0. Exact bytes are shared across 68K/PPC without treating
the map as four host-endian words. Comparison evidence is the preserved
process_context.rs key_is_down/set_key_down implementation and PPC GetKeys byte
copy, not a new native Macintosh oracle run.

The thin Winit adapter maps common Apple/ADB physical assignments, independent of
character translation, and passes transitions to one host keyboard driver. It
updates the canonical map even when events are masked. Representable character
transitions post keyDown3, keyUp4 or host-supplied autoKey5 with virtual code in
message bits8..15, character in bits0..7, current modifiers, location and TickCount.
Queue dequeue/PostEvent never fabricates physical state. Map writability and
queue failures are preflighted before committing driver-held state, allowing a
failed delivery to be retried. Duplicate presses do not generate spurious events.

Left/right modifiers maintain separate physical bits and aggregate the five
classic modifier flags. Winit modifier-only notifications provide a fallback
until side-specific events arrive; lock state is separate. Focus loss releases
nonlocking keys without synthesizing text events; the Caps Lock latch remains.
Unsupported host keys do not alias A. Navigation and function keys use classic
character codes, including Return13 and keypad Enter3, while single host text
characters use the same MacRoman mapping as fallback source fonts. The font table
was moved unchanged into mac_roman.rs with a full256-byte round-trip test. This
adds no CPU destination renderer; existing text drawing remains wgpu-backed.

This is a supported host-input slice, not complete keyboard-driver emulation.
The printable fallback is US; platform text wins when provided. Unsupported or
multi-character/IME text and zero character results generate no fabricated text
events while physical state remains visible. Guest KCHR/KeyTranslate, Script
Manager/dead-key composition and emulated repeat timing remain open. Winit0.30.13's
macOS flags_changed path forwards only Shift/Control/Option/Command, so actual
macOS Caps Lock delivery/resynchronization needs additional integration; the
latch is currently verified through the shared driver fixtures. Native window
input, browser behavior and original-game interaction remain unverified.


### Editable dialog text and shared TextEdit

Resource-backed DITL editText16 creates an independent raw text handle per item
and one unstyled TERec per dialog. DialogRecord.textH points to that shared TERec;
editField is the zero-based active item index. The TERec borrows the selected
item's canonical hText, so TEKey and Get/SetDialogItemText observe the same bytes.
Switching fields rebinds that one TERec; it never copies edited text into a
separate host string. TextEdit tracks borrowed ownership, releases its initial
private empty handle when first bound, and leaves borrowed item text alive on TE
disposal. Dialog Manager frees item text once during close/disposal.

SelectDialogItemText/SelIText share native$A97E and PPC import routes, signed
selection inputs clamped at zero and by TextEdit's live text length, per-field
selection and GPU highlights/caret. DITL's disabled bit suppresses item reports;
it does not prohibit editing. Hidden inactive selection uses shared layout without GPU
drawing; switching a previously active hidden field retains the available GPU
for TextEdit cleanup. Custom TE layout/selection callbacks remain explicit errors. Binding
failure restores the old TERec and handle identity, re-resolving movable storage.
Copied DITL data is likewise re-resolved after allocations.

DrawDialog uses TextEdit GPU update for the active editable field and shared GPU
text-box drawing for other fields; editable text is literal while static text
retains ParamText substitutions. Active SetDialogItemText relayouts canonical
TextEdit storage and erases/redraws through wgpu. Destination text pixels,
selection and caret are GPU operations; CPU font decoding/layout remain allowed.
The reserved editOpen word is implementation state, not claimed native parity.
ModalDialog/DialogSelect event handling, click/drag/tab navigation, dialog edit
commands, noneditable selection targets and complete text/UI coverage remain open.

Primary contracts: [DialogRecord](https://dev.os9.ca/techpubs/mac/Toolbox/Toolbox-388.html),
[SelectDialogItemText](https://dev.os9.ca/techpubs/mac/Toolbox/Toolbox-420.html),
[text access](https://dev.os9.ca/techpubs/mac/Toolbox/Toolbox-416.html), and
[disabled item meaning](https://dev.os9.ca/techpubs/mac/Toolbox/Toolbox-381.html).


### Cooperative ModalDialog slice

Native$A991 and PPC ModalDialog decode the same filter/itemHit request and
preflight the complete nonnil two-byte output before entering the shared service.
The visible current dialog must be manager-owned. A bounded host wait retains
the pending ABI reply and a private guest EventRecord/output scratch allocation;
it never returns a fabricated item when no event is available. The OS queue and
Window Manager remain authoritative, with disk-insertion events excluded by
mask$FF7F. Input can wake the wait without advancing guest CPU instructions.

The nil standard filter accepts Return/Enter through the live default button.
Text entry, deletion/arrows supported by TextEdit, Tab/Shift-Tab, selection/caret
and button tracking use the existing shared managers and wgpu. Enabled editable
items report their number after editing; disabled editable items still edit but
do not return an item report. Button/radio/checkbox tracking waits for an actual
release and cancels releases outside; caller code remains responsible for changing
control values. Updates use BeginUpdate/DrawDialog/EndUpdate and activation uses
TextEdit. Outside-content mouse clicks are rejected before guest filtering; NIL
DABeeper retains the existing explicit silent boundary.

Real guest modal filters receive DialogPtr,EventRecord*,itemHit* through an
18-byte 68K Pascal frame or prepared same-ISA PPC RoutineDescriptor (procInfo$FD0).
The whole caller CPU is retained. TRUE returns the filter's signed item value;
FALSE dispatches the possibly modified guest EventRecord. No host filter lambda
replaces guest execution. Modal waits join the suspension graph used by timer,
VBL and sound continuations. Mixed-ISA descriptors remain explicitly unsupported.

This is a keyboard/default-button/filter/control-tracking slice, not complete
Dialog Manager acceptance. Editable-field mouse click/drag/double-click selection
currently fails explicitly; external DialogSelect, custom alert sounds, edit-menu
commands and broader modal/modeless behavior remain open. Full native/browser,
audio and original-gameplay acceptance remain separate.

Primary contract: [ModalDialog](https://dev.os9.ca/techpubs/mac/Toolbox/Toolbox-426.html).

The diagnostic tracer's --key-at-poll uses bounded host advance-call counts, so
real host key delivery can occur while the guest instruction clock is paused.
--key-at retains its original guest-instruction clock. Both call the same host
Keyboard driver; neither changes PCs, return values or text handles directly.
Read-only GetDialogItemText logging records actual guest-visible name outputs.


## Shared oval drawing

FrameOval, PaintOval, EraseOval, InvertOval and FillOval marshal into one DrawOval
call and shared GrafPort implementation on both CPUs. Oval coverage reuses the
checked fixed-point ellipse recurrence; destination pattern operations, palette
handling, visibility/clip intersection and guest-memory synchronization use wgpu.
Frame uses the current rectangular pen dimensions and leaves the interior alone.
Paint uses pen pattern/mode, erase uses the background pattern with copy, invert
uses bit inversion, and fill uses the supplied pattern with copy. Fill publishes
the same live fill-pattern state as the rectangle path, including empty shapes.
Pen location, pen pattern/mode, current port and unrelated fields stay unchanged.
Hidden framing is suppressed before pen geometry; region recording first records
the un-clipped mathematical outer boundary, independent of pen size or visibility.
Hidden recording and empty nonpixel operations require no GPU destination access.

Oval extents are i32 differences of checked guest endpoints: a valid rectangle can
span65535 pixels even though each endpoint is i16. The public rounded-rectangle
API retains its signed i16 corner diameters. Wide recurrence intermediates and
nonoverlapping/coalesced spans avoid overflow and repeated XOR destination writes.
Basic and color port patterns reuse existing validation; expanded PixPats, custom
bottleneck procedures and picture/polygon recording through these calls remain
explicit unsupported boundaries. No legacy module is linked by the runtime.

Apple's [QuickDraw drawing description](https://leopard-adc.pepas.com/documentation/mac/QuickDraw/QuickDraw-59.html)
provides the drawing semantics. A separate comparison harness links the new
library to preserved Systemless's pure span function only for reference evidence;
it is not a production dependency or native System7 verification.


## Address-mode startup and service contract

The shared CODE/PEF launch path initializes MMU32Bit ($0CB2) to true32b (1),
matching the existing flat 32 address space and startup-dependent StripAddress
identity behavior. Previously the byte was zero even though instruction/data
accesses already used full 32-bit addresses. Gestalt 'addr' exposes verified
32-bit startup/capability bits (value 5); a distinct system-zone compatibility bit
is not advertised. This remains one compatibility profile for both CPU backends.

The shared addressing service reads and checks the live flag against the actual
supported memory model. GetMMUMode is a PPC import; native 68K code can read the
low-memory byte inline. SwapMMUMode receives/returns the low D0 byte on 68K and
uses a checked writable byte reference on PPC, returning the previous mode. The
current profile supports same-mode 32 requests. Requests for 24-bit translation,
invalid modes and a flag inconsistent with the backing model fail explicitly
before caller output changes. Real 24-bit translation, code/device aliases and
related architecture-specific behavior remain unfinished; setting a flag alone
would not implement them.

[Inside Macintosh: Memory, pp.4-26--4-27](https://dev.os9.ca/techpubs/mac/pdf/Memory/Memory_Mgmt_Utilities.pdf)
defines the parameter and register conventions. Supported query/same-mode calls
perform no allocation or blocking and are permitted by the VBL service guard.
The OS trap discovery table now exposes a readable executable A05D/RTS veneer;
the five oval verbs also have their implemented canonical discovery entries.
Entries are appended so existing built-in addresses retain their identity.

### Initial Window Manager port ownership

InitGraf leaves thePort NIL, as specified in Imaging With QuickDraw Table2-1.
InitWindows opens the real Window Manager-owned port and retains that selection
on first initialization when no caller port existed. An existing caller port and
the current graphics device are preserved. Repeated InitWindows is idempotent,
including after an explicit SetPort(NIL). No text operation repairs a nil port;
BeginUpdate continues to preserve the caller selection. This startup ownership
policy follows the documented creation/OpenPort operations; exact native
InitWindows selection behavior has not been independently measured. The preserved
implementation's synthetic InitGraf port is not copied.

Primary references: [QuickDraw initial values, Table2-1](https://www.vintageapple.org/inside_r/pdf/Imaging_With_QuickDraw_1994.pdf)
and [InitWindows creates the Window Manager port](https://dev.os9.ca/techpubs/mac/Toolbox/Toolbox-222.html).

### Bundled source identities for fixed-width text

The fallback source selector maps system/Geneva/Helvetica families0/3/21 to
Nimbus Sans and Monaco/Courier4/22 to Nimbus Mono PS. Existing guest FOND/sfnt
and legacy FONT resources resolve first. The bounded per-process cache uses
(source identity,point size), retaining one editable guest handle for aliases of
the same source and distinct handles across proportional/fixed-width sources.
Family mappings and source preparation do not add a rendering backend: packed
strikes, measurement, bold preparation and wgpu destination services stay shared.
The fixed-width substitute retains its own metrics; no historical Monaco metric
override or Apple glyph artwork is introduced. Full classic font fidelity remains
open. Asset provenance is in assets/fonts/README.md.

### Standalone PixMap descriptor ownership

NewPixMap and DisposePixMap/DisposPixMap use the shared device manager on both
architectures. NewPixMap snapshots the current device's live50-byte descriptor,
normalizes both resolutions to72dpi, and creates separate descriptor/table
handles. Pixel storage is referenced, not allocated or copied. The new table
has deterministic empty storage (8-byte header,ctSize=-1), not initialized
colors; the caller must supply its palette before using indexed pixels.

Device bootstrap precedes the independent two-handle allocation transaction.
Failure returns NIL with memFullErr and reclaims any partial handle. Disposal
validates descriptor/table ownership, protected device storage, live masters,
write access and image pinning before freeing either handle. It frees the
descriptor and its table, retaining the pixel image and current device. Existing
DisposeCTable supports caller replacement of the initially empty table.

Both ABI adapters reach this implementation; the68K no-argument long result is
validated before dispatch and the void disposal consumes one handle. PPC void
registers are preserved. CanonicalAA03/AA04 discovery veneers are appended, leaving
existing veneer identities stable. These operations remain forbidden at VBL
interrupt time by the existing no-allocation service guard. CopyPixMap and
SetPortPix remain explicit gaps rather than aliases of these operations.

Primary reference: [Imaging With QuickDraw,4-85–4-87](https://www.vintageapple.org/inside_r/pdf/Imaging_With_QuickDraw_1994.pdf).

### Live menu keyboard-equivalent queries

GetItemCmd reads the same checked live MENU item used by SetItemCmd, including
raw special markers1B–1E and extended bytes. It returns a zero-extended16-bit
CharParameter through guest memory; it does not format/render the item or
interpret special markers. Both ABI adapters validate the required two-byte
output before dispatch, and the68K adapter preserves its stack on output faults.
The shared service independently validates direct callers and snapshots the
record before writing, permitting output aliases. It preserves MemErr and makes
no guest allocation or menu installation. Relocated handles and direct guest
edits are observed; invalid items, nil outputs and malformed records fail
explicitly without publishing a value. Native behavior for invalid inputs is
not claimed. PPC void registers and68K tail bytes are preserved.

The canonicalA84E discovery veneer is appended without changing previous entry
identities. References: [GetItemCmd](https://dev.os9.ca/techpubs/mac/Toolbox/Toolbox-161.html),
[Menu Manager C summary](https://dev.os9.ca/techpubs/mac/Toolbox/Toolbox-187.html)
and the observed Apple10.4u SDK Menus.h/MacTypes.h declarations.

### Shared live menu-list insertion and deletion

InsertMenu/DeleteMenu mutate the existing DynamicMenuList and MenuCInfo handles
through one shared service. Both regular and submenu partitions retain handle
identity and opaque trailing entry values. Insertion uses-1 for submenus, appends
for0/missing IDs, and matches other signed before IDs in the regular partition.
Already-installed handles and full target partitions are no-ops. The checked
partition bound is1024entries, not a measured native capacity. Automatic ordering
of system Help/Keyboard/Application menus remains outside this supported slice.

Deletion searches submenus before regular entries, removes the first matching
ID and all color entries for that ID, and retains every menu record. Missing IDs
are no-ops, including orphan color entries. Existing title-width/spacing policy
reflows regular entries; inserted records use the existing shared menu sizing
path and guest font resources. Font source preparation can use its normal
bounded caches; list/record/color publication is staged through replace_handles.
Locked-growth failure leaves all staged target payloads unchanged. Saved tracking
contexts are rejected before font preparation; that interaction remains open.

No implicit bar drawing, image mutation or menu-record disposal occurs. Resource
bar IDs, manager handles and unrelated color records are preserved. Both ABIs
marshal void calls into this service: native Insert consumes2+4bytes and Delete
consumes2; PPC truncates signed word IDs and preserves its void result register.
A935/A936 discovery entries are appended without changing prior identities.
Primary references: [InsertMenu](https://dev.os9.ca/techpubs/mac/Toolbox/Toolbox-120.html)
and [DeleteMenu](https://dev.os9.ca/techpubs/mac/Toolbox/Toolbox-121.html). Existing
geometry remains reference-policy evidence, not new native pixel-parity proof.

### Shared Munger handle editing

Munger takes six32-bit arguments and edits one managed relocatable handle. Both
CPU adapters invoke Process::munger; source/needle/replacement snapshots survive
relocation and overlapping inputs. Search-only returns a match offset, edits
return the first byte after the substitution, and a miss/failure returns-1.
Direct NIL-needle overwrite, negative-length tail replacement, insertion, deletion
and raw binary/extended bytes use the same path. No text decoding is performed.

Compatibility policy deliberately follows the preserved Systemless evaluator: a
partial tail is not a match, and both-NIL pointers permit direct deletion. Apple's
general [Munger reference](https://leopard-adc.pepas.com/documentation/Carbon/Reference/Text_Utilities/Reference/reference.html)
describes partial-tail replacement and a broader NIL-second-pointer search-only
rule. The old source cites a System7.5 ROM observation, but no retained raw oracle
was located for this slice. This is authorized reference evidence, not new native
parity proof; that discrepancy remains explicit. Other negative search/replacement
lengths fail explicitly. Invalid offsets return-1 without editing.

The service uses checked shared handle sizing/preflight and staged replacement,
preserving handle identity/flags and resource associations. Growth inside current
allocated padding is legal even when locked; relocation of locked/pinned storage
fails with memFullErr and leaves bytes/allocation unchanged. Nil/purged destination
masters fail with nilHandleErr. Needed input ranges are checked before mutation;
misses do not read unused replacement data. No result-buffer or heap allocation
is attempted after an invalid native output frame. VBL's no-allocation guard
continues to exclude this operation.

Native Pascal decoding reverses six longs, validates a four-byte result before
popping24bytes, and returns the offset on stack with the Memory Manager error in
D0. PPC uses six registers and returns the offset in R3. The separate native D0
error channel follows [Apple PT14's glue convention](https://leopard-adc.pepas.com/technotes/pt/pt_14.html);
its native values are not independently remeasured. The canonicalA9E0 discovery
entry is appended without changing existing veneer addresses.


### Color-table change notification

CTabChanged uses shared Color Manager state through InterfaceLib and native
QDExtensions selector00040007. It validates a live managed ColorTable, including
its declared entry extent (empty tables allowed), preflights the seed write, then
assigns a fresh seed from the same monotonic namespace as GetCTSeed and entry
mutations. The table handle, size, flags and color entries survive unchanged.
Errors preserve the seed namespace and payload; Memory/Resource errors restore
on all outcomes. Native consumes one long and preserves registers; PPC is void.
Canonical QDExtensions discovery uses an appended veneer; unsupported selectors
still fail explicitly.

The [primary reference](https://www.vintageapple.org/inside_r/pdf/Imaging_With_QuickDraw_1994.pdf)
pp4-97–4-98 requires a new seed and invalidation notification. Live rendering
reads guest color tables each operation, so no hidden GPU palette cache survives
this call. Existing inverse-table seeds remain stale until MakeITable rebuilds
from current colors; their seed mismatch is the invalidation signal. This does
not claim ROM-exact inverse matching, native invalid-input behavior or interrupt
safety. The runtime's VBL allocation-safety guard continues to exclude Color
Manager operations, as the documented call is not interrupt-safe.


### FP68K single-to-extended conversion

FS2X selector100E snapshots exactly four big-endian source bytes, preflights a
ten-byte destination and invokes the existing shared single expansion. Finite
values, signed zeros, infinities and NaN sign/payload follow the same numeric
state as single-format arithmetic. Signaling NaNs quiet and accrue invalid;
conversion failure leaves flags and destination unchanged. Overlap is supported.
The adapter validates input/output and checked ten-byte frame removal before
changing SP; this void native call preserves registers. No fabricated InterfaceLib
single-conversion symbol or duplicate numeric engine is added. PPC numeric ABI
and full SANE exception/halt/native-profile coverage remain separate work.

Primary source: Apple Numerics Manual,2ndedition,chapter21,p156 and appendixE
binary-conversion table (FS2X100E), available in the
[Apple manual mirror](https://manuals.plus/m/1f06cc272b1e59ae58f6379edca5b7829815540f233f0679c0f5f26cb9dbcbde).
The service reuses the already implemented exact integer-bit expansion rather
than routing through host float arithmetic. New fixed bit-vector tests and
existing numeric regressions are evidence for the supported profile, not a new
native SANE parity claim.


The immediately following Marathon call is FX2S1010, documented in the same
manual,p157/appendixE. This now shares one integer-based narrowing implementation
with FX2D: format precision/exponent widths select IEEE single or double storage.
The original double method remains as a wrapper, preserving its supported profile.
Direct extended→single rounding retains all64significand bits until its one
nearest-even decision, avoiding an incorrect intermediate host-double tie.
Subnormal/normal carry, overflow, signed zero, NaN quieting/payload and accrued
exception flags use the common implementation. Single writes exactly4bytes,
double8; both snapshot ten source bytes and preflight before changing flags.
Rounding modes/precision/halts outside the existing supported SANE environment
remain explicit gaps rather than implied native parity.


### Shared SANE general exponentiation

Elems68K8012 (FXPWRY) decodes two extended pointers, validates both operands and
the destination before popping its ten-byte frame, and calls shared numeric state.
Both operands are snapshotted; numeric flags publish only after successful output.
No PPC symbol is fabricated. Source masks/canonical values retain extended low
bits instead of converting through host f64.

[Apple Numerics Manual](https://manuals.plus/m/1f06cc272b1e59ae58f6379edca5b7829815540f233f0679c0f5f26cb9dbcbde)
chapter8,p64 and table5-1 require special zero/infinity/domain rules and NANPOWER
code25hex. Those cases, NaN quieting/payloads, negative-base integral parity and
signed-zero poles precede dependency evaluation. Finite general powers use pinned
astro-float-num0.3.7, whose API and directed rounding implementation were inspected
locally. Its general pow path stalled on exact cases, so evaluation composes
directed ln/mul/exp bounds instead. Integer magnitudes up to65535 use powi and
outward reciprocal bounds; dyadic exponents additionally check up to16exact square
roots by squaring them back with full precision. This resolves algebraic midpoints
without guessing which endpoint to select. Down/up results use the shared exact
extended rounder; a value is accepted only when both bounds round to identical
bits.
Requested precision escalates128→4096bits before an explicit unresolved-rounding
error. Dependency evaluation remains adaptive internally; guest instruction
budgets do not prove wall-time bounds. No new production dependency is introduced.

The exact wide-result packing formerly inside addition is shared, with an early
extended-range overflow check before narrowing the exponent. Gradual underflow,
nearest-even rounding and range flags remain common. Mathematical inexactness can
be conservatively accrued by elementary evaluation as allowed by the manual;
ROM-specific last-bit/flag parity and alternate environments remain unverified.


### Virtual driver control and physical palettes

One process-owned driver manager serves both CPU adapters. The resident virtual
main display has reference-128 (unit127); its owned main GDevice publishes that
same reference. Sound retains-4. These are virtual identities, not ROM drivers or
a populated Macintosh unit table. Driver open/close/status, other video controls,
multiple physical displays and driver-backed NewGDevice initialization remain
unimplemented. Synchronous/immediate Control and PBControlSync share dispatch.

Video control3 reads VDSetEntryRecord(csTable pointer,csStart,csCount). Sequential
mode ignores ColorSpec.value; start-1 uses its logical indices. Count is entries
minus one; -1 is an explicit zero-entry no-op. Duplicates apply in order. Out-of-
range/nil required parameters return paramErr-50 rather than clamping to an entire
table. Unknown references return badUnitErr-21; unsupported resident controls
return controlErr-17. These invalid-input policies are not newly measured native
parity. ioResult is preflighted before driver-specific reads and published with
signed D0/R3. Snapshots cover source/result aliases; invalid later entries leave
the hardware palette unchanged. Existing Sound volume semantics are retained.

The physical palette is initialized from host display policy and owns256u16RGB
slots separately from guest QuickDraw matching tables. Direct video writes alter
only this palette and ioResult: no ctSeed,ColorSpec metadata,selected GDevice,port
or guest pixel bytes change. High-level SetEntries stages its existing matching-
table/seed changes and patches the same physical entries only for the registered
main device. Offscreen tables retain their matching role; low-level video control
still targets the physical display when an offscreen device is selected. Binding
is to the registered main owner, not an arbitrary copied PixMap/GDevice; live
changes to driver associations in fabricated records remain outside this slice.

Presentation uses configured physical framebuffer geometry and the hardware
palette, independently of guest matching metadata. Fixed monochrome explicitly
uses white/black even when the transient GPU drawing palette differs; its driver
does not advertise a programmable CLUT. Ordinary mono drawing keeps its bitwise
transfer path. Both depths present and compose cursors through wgpu, with no CPU
destination renderer. Metadata copies and packed-pixel staging retain the existing
synchronization boundary; the host refresh selects the physical screen, not the
last offscreen drawing texture.

Primary references: [Designing Cards and Drivers,p9-15](https://www.vintageapple.org/inside_o/pdf/Designing_Cards_and_Drivers_for_the_Macintiosh_II_and_SE_1987.pdf),
[Device Manager parameters](https://dev.os9.ca/techpubs/mac/Devices/Devices-19.html),
[PBControl result contract](https://dev.os9.ca/techpubs/mac/pdf/Devices/Device_Manager.pdf),
and [Color Manager device CLUTs](https://dev.os9.ca/techpubs/mac/ACI/ACI-107.html).
The preserved implementation additionally confirms that low-level video palette
animation must not overwrite QuickDraw matching metadata; that is reference
evidence, not new native System7 measurement.

### Shared offscreen-world ownership (in progress, 2026-10-17)

`gworld::Manager`, owned by Process, owns offscreen port pointers, pixel-image
handles and associated graphics devices/tables. Native QDExtensions selectors
and PPC imports marshal into one `Call::GWorld` and the same manager. It composes
existing color-port, device, Color Manager and pixel-lock services rather than
creating a second drawing runtime. Destination drawing remains the shared wgpu
path; direct pixel writes use LockPixels/GetPixBaseAddr and existing synchronization.

The first implementation covers NewGWorld, DisposeGWorld, GetGWorld, SetGWorld,
GetGWorldPixMap and GetGWorldDevice for indexed worlds. Depth0 and noNewDevice
borrow the selected screen/provided device; explicit indexed depth owns a detached
device. Palettes are copied. Each port retains a separate owned PixMap, and each
world owns image storage sized from its aligned stride and bounds. Creation
preserves the caller's current port/device. Selection preflights both writes;
an offscreen world's associated device overrides the caller's device argument.
Disposal validates port attachments and detached-device releases before freeing
anything. Current-world disposal, locked disposal, direct-color worlds and
temporary allocation remain explicit unsupported operations. Purgeable creation and
pixel-state management are implemented by the later slice below.

NewGWorld/UpdateGWorld flags and signatures are checked against Apple's Universal
Interfaces3.4 QDOffscreen.h and Inside Macintosh: Imaging With QuickDraw chapter6.
[NewGWorld](https://leopard-adc.pepas.com/documentation/mac/QuickDraw/QuickDraw-311.html)
and [SetGWorld](https://leopard-adc.pepas.com/documentation/mac/QuickDraw/QuickDraw-319.html)
provide device borrowing/selection contracts. UpdateGWorld and wider flags remain
unfinished. AB03 stays uninstalled: supporting individual offscreen APIs does
not establish complete 32-bit QuickDraw capability or fix original Marathon yet.
Test/build results are recorded separately after terminal verification.

### Indexed GWorld updates and GPU image conversion (2026-10-17)

UpdateGWorld uses the shared native/PPC dispatch and stages image, table, inverse
map, both PixMaps, GDevice and visible-region payloads before publishing a compound
handle replacement. Port geometry and foreground/background index fields publish
only after that replacement succeeds. CPU work validates/snapshots metadata and
packs storage; clipping, stretching, palette/depth conversion and dithering use
the shared wgpu Surface/Graphics path, with final synchronized readback to guest
storage. No guest routine is replaced or skipped.

Image conversion needs a neutral source-colour transfer, independent of a port's
foreground/background weighting. Surface::copy_image selects that explicit GPU
policy; ordinary QuickDraw and text keep their previous transfer policy. A bit
in the existing indexed-target command tail carries the policy without changing
its size. Shader matching and dithering still execute on the GPU. Source palettes
without exact black/white must work; mapping to the nearest destination black/
white before applying tint is not equivalent to image conversion.

The [Apple UpdateGWorld contract](https://leopard-adc.pepas.com/documentation/mac/QuickDraw/QuickDraw-314.html)
defines the selector, flags, device precedence, unchanged world on failure and
QDError reporting. Depth0 updates copy device state rather than aliasing screen
storage; an explicitly supplied device determines format over depth/table args.
World pointers and owned master identities survive ordinary resize/compaction.
Purged images are reallocated and reported lost, including unchanged bounds;
there is no attempt to read source pixels from a missing image.

Unpreserved size changes with flags0 report reallocPix and require reconstruction;
clipPix/stretchPix select preserved pixels. Same-size origin changes preserve
pixels through wgpu. Locked-image replacement is explicitly rejected with a
failure flag/error. Direct-colour worlds, wider creation flags, exact native
alignment/reallocation-flag conformance and remaining offscreen/pixel-state APIs
are unfinished. AB03 remains unavailable; this indexed slice does not establish
complete32-bit QuickDraw or original Marathon compatibility. Validation evidence
is recorded in progress.md, separately from native/browser/game acceptance.


### Offscreen pixel-state lifecycle (2026-10-17)

GetPixelsState, SetPixelsState, AllowPurgePixels and NoPurgePixels marshal through
both CPU adapters into Process::pixels_state. Flags project from the existing
image handle and public PixMap pins; no second cached flag authority is added.
Set restores movement/purge policy and baseAddr representation while retaining
unrelated handle attributes. NewGWorld's pixPurge option now marks its owned image
purgeable. Purging, compaction, failed LockPixels and UpdateGWorld reconstruction
therefore follow the same allocator and image ownership model.

HGetState includes explicit pixel locks in its guest-visible lock bit. Private
CIcon stability leases remain invisible as before; they cannot be managed through
the new offscreen-state calls. A PixMap cannot release another owner's image pin.
Mutation preflights any baseAddr write before changing handle policy. No state
call allocates guest storage or changes MemErr/ResErr, port selection or geometry.
The GPU drawing path is unchanged and reads the canonical current image.

Apple's [AllowPurgePixels](https://leopard-adc.pepas.com/documentation/mac/QuickDraw/QuickDraw-325.html),
[NoPurgePixels](https://leopard-adc.pepas.com/documentation/mac/QuickDraw/QuickDraw-326.html),
[GetPixelsState](https://leopard-adc.pepas.com/documentation/mac/QuickDraw/QuickDraw-327.html)
and [SetPixelsState](https://leopard-adc.pepas.com/documentation/mac/QuickDraw/QuickDraw-328.html)
define the public calls; UI3.4 QDOffscreen.h confirms selectors/frame sizes.
AllowPurgePixels requires an unlocked image. Unsupported flag bits, attempts to
restore a lock on purged storage, fixed-address images and private icon backing
storage are explicit diagnostics in this profile. Native edge/error/alias parity,
temporary/hardware images, direct colour and remaining offscreen contracts are
unfinished. AB03 remains unavailable and the full rebuild remains active.

### Direct-colour GPU storage foundation

The shared integer drawing pipeline accepts RGB555 and xRGB8888 destinations.
`Graphics::execute_direct` decodes big-endian guest storage to canonical u32
colour words, runs the existing WGSL commands, and repacks after successful GPU
readback. CPU work encodes storage; destination pixel operations remain on wgpu.
RGB555 uses bits 10/5/0 for five-bit R/G/B; xRGB8888 uses RGB bytes after the
unused leading byte. Packing clears the unused bit/byte on pixels written by QuickDraw, while
preserving untouched pixels and row padding. Apple's PixMap reference explicitly
requires zero unused high bits. Exact native edge and quantization behavior
still need conformance evidence.

Draw coordinates at this backend boundary are bitmap-local. The existing shader
handles patterns, clipping, source-snapshot overlap, transfer modes and direct
RGB quantization without an indexed palette search. Draw foreground/background
values are now u32; indexed callers explicitly widen their validated indices.
All entry paths reject scalar values outside the destination storage format.
The direct wrapper publishes guest bytes only after successful validation and GPU
readback, preserving writable-memory checks and synchronous guest visibility.

Live direct PixMap sources are now parsed as described below. Direct destination
`Surface` records are now implemented below; direct screen profiles and managed
NewGWorld is implemented below; direct UpdateGWorld remains unfinished.
Those shared service changes,
both ABI adapters, display presentation and original-game acceptance are next.
Do not advertise AB03/complete 32-bit QuickDraw from this backend foundation.
It also does not complete the separate text-library integration requirement.


### Live direct PixMap sources

Shared CopyBits source parsing accepts RGBDirect (pixelType16), pixelSize16/32,
three components of size5/8, pmVersion0/4, raw packType0/1 with packSize0,
zero planeBytes/reserved fields and checked aligned row storage. CPU preparation
extracts RGB555/xRGB8888 data, ignores unused high bits and pmTable, and snapshots
all source colours before destination drawing. Serialized PICT compressed rows
remain a separate input decoder. Unsupported packed/planar/component formats
reject explicitly instead of being interpreted as indexed data.

Both native and PPC CopyBits route those RGB samples through existing shared GPU
RGB transfer and dithering services. Indexed destination policy, port colours,
clipping and guest-memory readback stay unchanged. Shared image conversion also
accepts RGB samples without introducing a second drawing implementation. A
borrowed RGB view avoids reallocation when dithering direct sources. RGB555 CPU
expansion is shared with PICT decoding; the shader repeats the same five bits.
This establishes one expansion contract, not verified native rounding parity.
Direct destination PixMaps now use the shared surface path described below.
Direct CIcon source formats and full graphics-profile capability remain unfinished;
do not advertise AB03 from source/destination storage support alone.

Primary format/high-bit authority:
[Apple QuickDraw Reference, PixMap](https://leopard-adc.pepas.com/documentation/Carbon/Reference/QuickDraw_Ref/QuickDraw_Ref.pdf),
Data Types, page130; and
[Apple QD13](https://leopard-adc.pepas.com/technotes/qd/qd_13.html).
The preceding backend milestone's preservation of high bits on written pixels
was an explicit provisional contract; it is now corrected to the documented zero
behavior. Only CPU storage encoding/decoding is used; destination rendering stays
on the shared wgpu pipeline.


### Direct destination surfaces and full-width port colours

Live RGBDirect PixMap destinations share the same checked layout parser as source
images. Surface colours/indices are u32 values: indexed formats retain their
bounded palette indexes; RGB555/xRGB8888 formats use packed colour components.
Matching and RGB extraction select the direct format before any palette/index
arithmetic, avoiding truncation and depth32 shifts. Port foreground/background
fields, copy context, picture colours, icon targets and window colour preservation
carry the same full-width values. RGBFore/BackColor retains requested RGB values
and the existing checked current-device/destination format agreement.

Surface staging uses the existing one-byte indexed representation or big-endian
u32 direct words as appropriate. DirectFormat owns packing/unpacking for both
the standalone GPU API and shared guest surfaces. Readback preserves row padding
and untouched pixels; shader writes clear unused high bits. Regions, negative
bounds, pattern alignment, source RGB conversion, transfers and glyph coverage
use the existing GPU pipeline. Direct foreground/background defaults require
metadata only and never dereference pmTable or require attached pixel storage;
actual source/destination drawing still requires valid backing memory.

Read-only destinations reject before GPU/pixel mutation. Direct destination
Floyd-Steinberg dithering is explicitly unsupported before any drawing; indexed
dithering continues its existing shared service. Do not call the unfinished
direct-dither path or advertise it through offscreen/profile flags. Direct screen
mode creation, managed16/32-bit GWorld updates and complete native edge
conformance remain open. Source storage support and a checked drawable manual
PixMap do not establish a complete 32-bit QuickDraw capability.

Before declaring a complete direct-device profile, audit device colour queries
as well as drawing. Index2Color currently implements indexed-table semantics.
The published Apple reference (page257) specifies a table index with a zero high
word; it does not establish a packed24-bit conversion contract. The earlier plan
to route it directly through pixel unpacking was an unverified inference. Verify
32-Bit QuickDraw's actual API rules before changing it. Color2Index/native inverse
matching conformance also remains open. Drawable-surface fixtures do not establish
these adjacent APIs or complete capability.


### Managed direct-colour world creation

NewGWorld now creates RGB555/xRGB8888 worlds through the existing shared manager.
The image, port, separate port/device PixMaps, dummy colour table, inverse handle
and ownership/disposal remain one graph for both CPU adapters. Direct table input
is ignored; the owned16-byte dummy table has seed15/24, zero flags/size/specification.
Direct devices use gdType2 and an empty six-byte inverse header, without generating
an indexed matcher. Both indexed/direct owned offscreen devices now publish
noDriver, the colour-device bit where appropriate, and mode-1. Row storage is
four-byte aligned and initially clear. Creation preserves the selected port/device;
noNewDevice inherits the supplied format and keeps its device/records borrowed.

Device colour matching now resolves metadata separately from image access.
An unlocked GWorld's device PixMap may contain an image handle; matching must not
interpret that handle as pixel bytes. Actual drawing/source access still validates
image storage. RGB colour selection and initialization use live format/table
metadata; this does not add a second colour cache or weaken drawing validation.

New allocations roll back on failure. A device owner cannot be disposed while
another managed world borrows its device, preventing dangling child graphs. Current
worlds/pinned images retain their existing disposal restrictions. Direct worlds now use the atomic update path below. Unsupported direct destination
dithering still rejects before mutation. Temporary/hardware flags, direct
destination dithering,
complete device-query semantics and actual profile acceptance remain unfinished.

Primary creation/type authority:
[Apple QD13](https://leopard-adc.pepas.com/technotes/qd/qd_13.html),
[NewGWorld](https://leopard-adc.pepas.com/documentation/mac/QuickDraw/QuickDraw-311.html)
and [QuickDraw Reference](https://leopard-adc.pepas.com/documentation/Carbon/Reference/QuickDraw_Ref/QuickDraw_Ref.pdf)
(device types, page151). Native allocation/error/edge parity is not established by
the synthetic service fixtures. AB03 remains unavailable.


### Atomic direct-world updates

UpdateGWorld now stages indexed/direct conversions through the same manager and
wgpu image-copy path. New/Update share dummy direct-table and PixMap component
initialization. The image, table, inverse header/table, port PixMap, device PixMap,
GDevice and visibility region are preflighted and replaced together before port
geometry/colour publication. RGB requests remain unchanged while their packed
foreground/background values are rematched for the new format. Device type,
colour/noDriver attributes and mode stay coherent with the destination. Direct
formats have empty inverse headers; indexed formats generate their existing table.

Clipping/stretching, origin/size changes, direct/indexed conversion and purge
recovery retain the shared update policies. CPU work decodes/encodes storage and
stages unchanged or cleared bytes; pixel preservation/conversion uses wgpu.
Image-copy policy remains independent of the caller's port tint. Existing indexed
Floyd-Steinberg output accepts direct RGB input; dithering to direct destinations
remains explicitly unsupported. The scoped dither-to-indexed fixture uses palette
exact swatches; it is not a new general/native diffusion oracle.

A borrowed-device world detaches to its own complete graph on update, preserving
the source device and releasing the parent dependency only after publication.
Owners with borrowers reject changes to depth/colour metadata before mutation.
Allocation failure after GPU conversion leaves every guest payload unchanged.
Purged direct images can be reallocated with complete matching component records.
Flags and native edge/rounding parity still require broader conformance evidence;
this is not complete graphics-profile, screen-mode or original-game acceptance.


### Shared library text masks

The text service retains guest font selection, classic cell geometry, bearings,
advances and pen ownership. Glyphon 0.12.0 now prepares these binary cells on the
same wgpu device; no host font database or modern layout substitutes for guest
metrics. A Graphics-owned mutex serializes cached mask preparation and its GPU
QuickDraw transfer. Scratch Graphics targets share this device-bound cache.
Content/dimension keys observe live guest font edits without retaining guest
pointers. Atlas/renderer reset precedes identity reuse at the source-cache bound.
The reusable RGBA target is bounded to 1,000,000 texels, and only the active cell
rectangle is composed. Masks remain on the GPU; destination synchronization
continues through the shared Surface path. Specialized menu text still uses
GPU coverage runs. Integration acceptance and remaining full-text requirements
are tracked in text-rendering.md; this is not native/browser acceptance.


### Native-32-bit internal QuickDraw address entry

The legacy internal AB03 entry is now installed with a real shared native-mode
contract: validate the existing 32-bit address model and preserve registers,
condition codes, stack arguments and guest memory. Callable veneers retain the
ordinary JSR return behavior. Both CPU adapters observe the same trap table.
No PPC private import or 24-bit translation ABI is invented.

Additional primary evidence revises the earlier conservative absence decision:
[Apple's Jackson Pollock product plan, scanned PDF page 38](https://bitsavers.trailing-edge.com/pdf/apple/mac/blue/Apple_Blue_Book_vol1_1989.pdf#page=38)
identifies AB03 internally and specifies return-only behavior for native-32-bit
machines. We implement that documented case as a compatibility policy for this
fixed native-32-bit runtime; this is not an alias inferred from the later public
A091 interface or a claim about every private ROM revision. The downloaded PDF
SHA256 is a02c03a6cedc774466db1fe962ece328492d9bd9692932c140e3acf4e966c694.

The declaration is backed by implemented 8/16/32-bit surfaces and managed-world
creation, coherent resize/locking, device ownership, GPU composition and tests.
The previous isolated offscreen-policy assertion is superseded by executable
entry tests and the existing real Marathon-sized resize test linked to this
profile. This does not establish complete QuickDraw conformance: direct-target
dithering, hardware/temporary allocations, remaining queries, native/display
profiles and full text/native/browser acceptance remain open. Corrupting the
mode flag to an unsupported 24-bit state still errors without translation.

Unchanged original Marathon now selects its System 7 managed-world branch and
renders a textured 3D demo beyond the prior framebuffer/light-array fault. The
shared manager updates both port/device PixMaps with the resized image; no pixel
allocation padding, aliasing, guest patch or shade-index clamp is introduced.
Escape Velocity's corrected idle frame remains byte-identical. Exact acceptance
and remaining interaction limits are in progress.md and original-applications.md.


### Timed diagnostic screen captures

The file-application tracer accepts up to 64 repeatable
`--save-screen-at INSTRUCTION:NEW_PPM` options. It orders them by retired guest
instruction count and captures at the first poll that reaches each threshold,
before delivering a host key scheduled at that same boundary. Each capture logs
both the requested threshold and actual retired count/poll; callback and waiting
polls mean these clocks must not be conflated. A threshold below the poll budget
is not a promise that the guest will retire that many instructions before stop.

Captures use the same canonical wgpu readback as the final `--save-screen`,
require `--graphics`, and create a new output file without replacing an existing
one. They do not checkpoint the input volume or inject guest events. These are
headless diagnostic observations, not native presentation or performance evidence.
This makes loading/menu/demo transitions observable before a final frame changes
again, without guessing their contents from a terminal budget stop.


### Picture-local initial colors

PICT bitmap playback starts from local black foreground/white background instead
of borrowing the caller port's current colors. Decoded FgColor/BkColor opcodes
still override that local state in stream order. The caller's port and color
fields are not modified. Clipping, mapping, source palette decoding, explicit
transfer modes and GPU destination operations keep their existing shared routes;
this is not a neutral-copy exception for all indexed images.

Apple Technical Note21, QuickDraw's Internal Picture Definition, p.4, specifies
that OpenPicture/DrawPicture establish default drawing state. The note explicitly
covers version1; its scope must not be presented as a new version2/native oracle.
The preserved implementation independently initializes picture colors (reference
evidence); focused v1/v2 fixtures exercise this runtime's default/recorded-state
contract on both CPU adapters. Apple's Color QuickDraw Table4-1, pp.4-32–4-33,
continues to govern the shared foreground/background-weighted transfer operations.
Sources: [TN21 scanned note](https://show.docjava.com/posterous/file/2012/07/9614411-DOC01000.PDF),
[Color QuickDraw](https://developer.apple.com/library/archive/documentation/mac/pdf/Imaging_With_QuickDraw/Color_QuickDraw.pdf).
The TN21 page was rendered and visually inspected; PDF SHA-256
`f2854e06001e91c438b8e029fd168bd05debcab2078bcf660e318874221a39f5`.
Full PICT graphics/text opcodes and authoritative native color-picture parity
remain outside this completed slice.


### Shared synchronous virtual-volume information

Files owns PBGetVInfo/PBHGetVInfo behavior. Native A007/A207 and the PPC
PBGetVInfoSync/PBHGetVInfoSync imports marshal one pointer and a layout selector
into Call::VolumeInfo; neither adapter computes catalog or capacity information.
The basic output is64bytes, the packed HFS output122bytes. Preserve the common
header, name pointer, reserved filler and enumeration index; publish normalized
vRefNum-1 and synchronous completion/result fields using existing File Manager
error conventions. Errors preserve other outputs. All successful PB/name outputs
are preflighted before publication. Input/name aliases are snapshotted; as with
existing file-info queries, PB fields take precedence on overlapping output.

The one mounted Application Volume selects index1; later indices return nsvErr.
Index0 ignores input name contents and uses vRefNum only. Negative indices use
full volume-name prefixes or fall back to the default/WD reference. Explicit
absent names yield nsvErr; an absent default yields paramErr. Name output is
optional and canonical. WD/default-subdirectory queries return the real directory
valence. Positive enumeration selects root. File/directory counts and next CNID
come from the shared catalog; write count is the process's logical catalog revision.

Capacity describes the existing64MiB writable fork/reservation pool, not host
filesystem space. Immutable loaded application forks remain outside that existing
write quota. The reported allocation unit is2048bytes, total32768unsigned blocks;
free blocks floor (quota minus space_used)/2048. Pending Resource Manager growth
is charged by the same space_used method as raw fork expansion, so a query cannot
advertise bytes already reserved. A partial allocation unit is conservatively
unavailable. This does not change the byte-level write quota or SLVOLUME format.

This virtual profile reports HFS signatureBD and stable online drive1/reference-1,
not physical disk extents or a callable ROM disk driver. Extent/bitmap fields,
Finder/backup data and volume-level timestamps are zero; timestamp zero denotes
the1904epoch and volume timestamp mutation/persistence is not implemented in this
slice. Do not infer native HFS disk layout or metadata parity from these profile
values. General GetVInfo wrappers, async calls, PBHSetVInfo, additional mounts and
physical driver operations remain unsupported. The canonical OS table entry is
A007; direct A207 dispatch retains its HFS modifier. Cached OS-entry modifier
calling conventions are not newly verified.

Primary contract: [Apple File Manager, pp.2-144–2-146](https://developer.apple.com/library/archive/documentation/mac/pdf/Files/File_Manager.pdf).
Packed field offsets are derived from the typed HVolumeParam declaration (free
blocks62, allocation-block size48); the printed assembly-summary offsets are
inconsistent for some fields. Original Marathon's free-space routine independently
reads these exact offsets. This is direct application evidence, not native parity.


## Shared Folder Manager and persistent System Folder identity

FindFolder is a shared catalog service reached by native 68K A823 selector zero
and the PowerPC FindFolder import. Both adapters preflight output pointers before
publishing any directory creation. Classic system-subfolder types resolve below
one blessed directory, while desktop, temporary and trash folders resolve below
the application volume root. Gestalt fold advertises only FindFolder bit zero.

File-application host initialization provisions a virtual System Folder after
catalog restoration and before guest execution. This is deliberately separate
from FindFolder: that API never creates the System Folder, even when its create
argument is true (Inside Macintosh, Toolbox, FindFolder). No System file or ROM
is installed. SysEnvirons reports the blessed folder's actual mounted volume
reference; PBHGetVInfo returns its ID in ioVFndrInfo[0]. The blessed directory
cannot be deleted through the shared catalog service.

Catalog version three persists the System Folder ID immediately after the
32-byte header and otherwise uses version-two directory/file records, including
extended Finder metadata. CRC, length, node and parent checks precede publication;
a blessed ID must name an actual directory. Versions one and two still load and
remain the export format until a System Folder has been provisioned. Restore
precedes host initialization so a reopened volume retains its directory IDs.

This virtual profile uses classic English folder names and one local user.
Extended Folder Manager calls, localized fld# descriptors, network folders and
startup temporary-file cleanup are outside the implemented slice. Original-game
acceptance must verify actual successful folder and volume queries rather than
accepting a fabricated volume reference after failed discovery.

Primary contract: https://dev.os9.ca/techpubs/mac/Toolbox/Toolbox-472.html


## Shared uncompressed sound double buffers

SndPlayDoubleBuffer dispatches native SoundDispatch selector 00200008 (including
its four-word argument-count modifier) and the PPC import into the same Manager.
The manager validates both live guest buffers before publishing ownership, uses
the existing PCM mixer for mono/stereo 8-bit offset and big-endian 16-bit samples,
and carries fractional phase and division remainder across buffer boundaries.
The uncompressed packet-size field is unused. Compressed/custom-codec headers
remain unsupported; they do not claim playback. Gestalt snd bit 10 now identifies
this implemented double-buffer path alongside existing stereo/multichannel/16-bit
output capabilities. Sound input/recording and codecs remain unadvertised.

Exhaustion clears only dbBufferReady and preserves other flags/user words. A
bounded pending queue retains the actual exhausted-buffer pointers; buffers cannot
be replayed before the pending guest callback has returned. dbLastBuffer ends the
stream after consumption. An unready next buffer stalls with silence and retains
ownership until guest refill or cancellation. The current mixer remains the only
sample consumer for audible and silent host time. Guest callbacks execute only
through budgeted Runtime continuations, never on the native audio callback.

The two-pointer Pascal callback adapter serves both sound command and doubleback
procedures (same 3C0 procedure info). 68K receives the exhausted pointer at 4(sp)
and channel at 8(sp); PPC receives channel in r3 and buffer in r4 through a prepared
same-ISA UPP. Caller CPU/waits are suspended whole. Channel generation plus a
separate submission generation prevents late callback completion from affecting
replacement playback. Quiet/disposal/process termination cancel owned state.

Primary contracts: Inside Macintosh Sound, pp. 2-69, 2-111–113, 2-148, 2-153–154:
https://dev.os9.ca/techpubs/mac/pdf/Sound/Sound_Manager.pdf
Selector/count and UPP signatures also checked against Apple's Sound.h interface:
https://www.cs.vu.nl/~eliens/research/media/lib-of-vs-libs-QTDevWin-CIncludes-Sound.h.html
These are supported uncompressed semantics, not full Sound Manager acceptance.
