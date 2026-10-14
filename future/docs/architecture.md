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
referenced segments and patches the A5 jump table. PEF materializes sections,
pattern data and BSS, applies bounded relocation streams using existing addends,
resolves registered InterfaceLib imports, and enters the relocated main transition
vector. Unknown strong imports fail; unresolved weak imports remain zero.
Initializer/main/terminator functions are sequenced by runtime lifecycle state.
This custom focused parser has no dependency on the legacy loader.

`abi` decodes 68K Pascal stack and register conventions or PPC CFM registers into
typed service calls, then applies results through architecture-specific replies.
Loaded applications cannot access private fixture gateways. The implemented
services are TickCount, ExitToShell, NewHandle/NewHandleClear, DisposeHandle,
GetHandleSize, GetResource and error queries. Unsupported calls terminate clearly.

`process` owns a bounded first-fit heap, guest master pointers, resource identity
and operation errors. Disposal coalesces free blocks. Both CPUs see the same
resource handles and guest-modified contents; cleared allocations zero reused
blocks. This is not yet a relocatable or purgeable Macintosh heap. The guest
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
the host. This implements one application fragment; shared-library closure
ordering, dynamic CloseConnection, and CFM APIs remain future work. The mapped
container and initialization block remain valid for the Runtime lifetime.

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
lookup invokes the same address resolver. This does not yet implement patching,
daisy chains, or PPC calls through returned 68K pointers (Mixed Mode is pending).

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

Currently only rectangular regions and monochrome colors are supported.
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

Real adapters currently expose InitGraf, OpenPort, SetPort, GetPort,
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
real host input translation is still pending. Tests verify scheduler budgets,
waiting and terminal behavior without a GPU. Bounded surface smoke must still
prove actual presentation on an available desktop; guest completion alone does
not count as a successful window smoke.
