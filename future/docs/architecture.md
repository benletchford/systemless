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
Unaligned data accesses are allowed by storage; each CPU enforces its own ISA
alignment, and instruction fetch requires 2/4-byte alignment. Permissions are
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
