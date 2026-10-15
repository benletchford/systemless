# Progress and continuation

## Isolation and evidence

- Baseline: `634c0e9120a695224d71c621fbbe33709670d707`.
- Original checkout: `/Users/ben/.codex/worktrees/b2f1/systemless`, preserved.
- Rebuild: `/Users/ben/.codex/worktrees/future/systemless`, `dev/future`.
- Public tracking issue: https://github.com/benletchford/systemless/issues/4801.
- No existing runtime/website files changed. `future` is its own Cargo workspace
  with a pinned lockfile and no dependency on the old `systemless` package.

## 2026-10-13: executable foundation

Implemented:

- Checked u32 guest pointers; mapping overlap, permissions, endian access,
  bounded scalar writes and explicit instruction alignment/fetch permissions.
- Real m68k 68020 and PPC engines borrowing one memory object, distinct private
  fixture ABI adapters, a single typed service dispatcher and observable state.
- Deterministic bounded stepping, terminal faults/exits, unknown-call errors.
- WGSL integer indexed fill/pattern/clip/Copy/Or/Xor/Bic and source-snapshot
  overlap copies; GPU palette expansion; padded RGBA readback. Guest bytes are
  synchronized before execution resumes. No legacy renderer or CPU fallback.
- winit host window, nearest/aspect-preserving scaling, resize, zero-size skip,
  lost/outdated surface handling, explicit GPU error reporting, bounded smoke.
- Internal callback descriptors and continuation stack holding whole caller
  engines; nested 68K -> PPC -> 68K and PPC caller preservation tests.

Acceptance status:

1. Foundation: CPU and offscreen GPU executable and tested on native
   macOS/Metal. Windowed implementation compiles; actual presentation remains
   unverified because the Mac was locked. Browser integration, device recovery
   and non-Metal hosts remain unverified.
2. Both architectures: synthetic machine-code fixtures pass distinct ABI paths;
   an additional test switches CPUs within one live runtime and checks that the
   second call sees the first call's accumulator state.
3. First graphics slice: synthetic dual-CPU direct-write/XOR parity passes
   exact expected indices and RGBA comparison. These are graphics primitive
   contracts, not a claim of complete QuickDraw/Toolbox ABI compatibility.
4. Application lifecycle: not implemented. No original application was run.
5. Mixed-mode: internal transition mechanism tested, but Apple descriptors,
   ProcInfo, real Mixed Mode ABI and Toolbox callback continuations are pending.
6. Compatibility expansion: pending ordinary-application loading.

Verification on Apple M1, Metal adapter (2026-10-13):

- `cargo test --manifest-path future/Cargo.toml --locked --offline -- --include-ignored`:
  11 integration tests including 2 real-GPU tests; all passed.
- Default suite: 9 non-GPU tests, 2 intentionally ignored GPU tests.
- Window smoke: **unavailable**, not passed. The initial smoke counted skipped
  frames; this was corrected before commit. The strict smoke receives Metal
  `Occluded` for every acquisition and exits with an error after 600 paced
  attempts. Computer-use inspection confirmed the Mac is locked. Rerun
  `cargo run --manifest-path future/Cargo.toml --locked --offline -- --window --smoke`
  and the same command with `--ppc` once unlocked. Actual presentation, resize
  and zero-size acceptance are pending; the code must never count an occluded
  frame as a successful presentation.
- Strict all-target clippy; changed-file rustfmt only. No legacy build/format
  gate was run: the replacement has no legacy source dependencies.
- No screenshot/native Macintosh oracle comparisons yet. GPU expectations are
  independently specified synthetic values. Forced surface/device-loss paths
  and actual OS minimization were not exercised.

## Next executable milestone

First rerun strict window acceptance once the Mac is unlocked; do not mark
foundation complete until it passes. Continue independent loader work meanwhile.

Keep working in this worktree/branch. Read architecture.md and run the above
contracts before broad changes. Do not port either old runtime wholesale.

1. Resource/CODE/PEF loading and a shared resource/handle slice are implemented
   below. Extend through executable application behavior, not isolated stubs.
2. PEF application lifecycle is implemented below; extend toward real compiler
   startup and shared-library closure support through application-driven slices.
3. Add shared VFS forks and a deterministic event queue, preserving host-policy
   separation from ABI handlers.
4. Build one minimal reproducible ordinary Toolbox application in both ISAs
   that allocates memory, reads a resource, draws, consumes an event, and exits.
   Add actual 68K stack/register and PPC CFM adapters for precisely those calls.
   Retain private fixtures as architecture tests, clearly labelled.
5. Verify shared semantics and pixels; then select Toolbox Showcase Graphics
   and Controls interactions. Keep native-authoritative and legacy evidence
   separate. Do not call an app compatible from launch or first-frame output.
6. Parse real RoutineDescriptor/ProcInfo and implement declared signatures,
   return storage, TOC and reentrant service continuations. The current internal
   descriptor tests establish control flow only, not Mixed Mode compatibility.

Known technical debt: full-image upload and synchronous readback on each draw,
new GPU buffers per operation, native blocking scheduler, only 8-bit indexed
surfaces and rectangular clips, no QuickDraw port state, packed pixels, text,
region masks, archive intake, persistence, audio or guest UI. The fixture CLI
is deliberately small. CPU faults terminate rather than deliver restartable
Macintosh exceptions. Callback stack slots are private runtime reservations;
a real allocator must reserve them before application address-space loading.

## 2026-10-14: resource/CODE/PEF and shared process slice

Implemented checked resource maps, near CODE/A5 jump tables, PEF sections, all
pattern-data operations and documented relocation opcode families, bounded
import resolution and main transition vectors. Both architectures now invoke
actual Macintosh ABI adapters for resource lookup, handles, timing and exit.
A single process owns the heap/resource cache; a test switches CPU architecture
inside one loaded Runtime and verifies preserved handle identity and mutated data.

Verification on Apple M1/Metal:

- Full explicit-GPU suite: 29 tests passed (6 unit, 11 contracts, 12 loading).
- Strict local all-target clippy passed. The foundation CI used a newer compiler
  and found constant `chunks_exact` calls; these are replaced with `as_chunks`.
  Both replacement-runtime CI runs passed for commit `86b2ae02`.
- Generated CODE application: Exit(0), 23 instructions, 5 service calls.
- Generated PEF application with resource fork: Exit(0), 46 instructions,
  5 service calls. Reproduction commands are in the README.
- Malformed/truncated inputs, relocation addends/repeats/budgets, weak/strong
  imports, allocation reuse/exhaustion, cleared memory, resource identity and
  low-memory error/tick visibility have focused tests.

No original application has been run. Window acceptance remains pending as
recorded above. Far-model CODE, compressed resources, full CFM lifecycle, files,
events, QuickDraw ABI, real Mixed Mode descriptors and complete application
interactions remain unfinished. **The rebuild goal remains active and incomplete.**

## 2026-10-14: bounded PEF application lifecycle

Added initializer/main/terminator sequencing, relocated entry/TOC validation and
a guest initialization block describing the memory container. Tests cover
phase ordering across budgets, initialization failure, preserved main results,
ExitToShell cleanup with/without initialization, one-time cleanup, fault paths
and nonreturning routines. All 34 tests, including 2 explicit GPU tests, and
strict all-target clippy pass locally on Apple M1/Metal.

This extends the single application fragment loader. Dynamic shared libraries,
closure dependency ordering and guest CFM connection APIs remain unfinished.
Original application interactions, events/files/QuickDraw ABI and window
acceptance are still the next major executable gaps; the full goal stays active.

## Compiler-generated continuation probe

Both lifecycle CI runs passed for `9d293bac`. The existing local MPW Docker image
also compiled `future/fixtures/mpw/probe.c` for both architectures, with standard
startup libraries and generated DATA resources. Its reproducible build and exact
run commands are documented beside the source. `trace_application` provides a
bounded instruction/register/service trace without changing guest execution.

The compiled probe does not run yet: 68K startup consumes the initial stack word
as launch data and faults reading 0x8100 at PC 0x01000456; PPC preparation rejects
required `StdCLib:_BreakPoint`. These are the next concrete startup requirements
to investigate, before events and QuickDraw. Do not bypass startup or resolve
imports with silent stubs to make the probe appear compatible. Generated binaries
remain ignored; the original checkout is still clean and unchanged.

## 2026-10-14: compiler-observed CODE startup return contract

The MPW trace established that startup reads and compares the caller's code
bytes; it was not consuming a Finder parameter structure. CODE launch now maps a
readable, immutable, executable ExitToShell trampoline and puts that address on
the initial stack. A guest fixture reads its instruction bytes and returns
through it, verifying actual service dispatch rather than private-sentinel exit.

The compiled 68K probe advances through global initialization to GetOSTrapAddress
(A346) at boundary 759, querying Gestalt (A1AD). Trap address lookup needs coherent
Unimplemented identity and callable supported entries; a constant success or
arbitrary dummy address would violate the documented contract. Primary evidence:
https://dev.os9.ca/techpubs/mac/OSUtilities/OSUtilities-175.html and
https://dev.os9.ca/techpubs/mac/OSUtilities/OSUtilities-177.html.

The compiled PPC import inventory is recorded in the probe README, including
StdCLib data symbols. The next executable milestone is still both ordinary
compiler-generated probes reaching and completing their resource/handle checks,
then extending them through events and QuickDraw. The rebuild remains incomplete.

Validation for the return-trampoline fix: all 35 tests pass, including both
explicit GPU tests, and strict all-target clippy passes on Apple M1/Metal.

## 2026-10-14: callable trap lookup and current resource map

Implemented GetOSTrapAddress/GetToolboxTrapAddress/NGetTrapAddress, shared
Unimplemented identity with explicit dsCoreErr failure, and callable 68K OS and
Toolbox entries. Tests cover number masks, PPC/68K identity, preservation of
unrelated error state, D1 modifier words, Pascal arguments/results and shared heap
operations through retrieved addresses. CurResFile now reflects the process's
installed application resource map through both ABIs.

All 41 tests pass (including both real-GPU tests), as does strict all-target clippy.
The compiled 68K MPW probe now reaches boundary 902: HFSDispatch A260 selector 8,
PBGetFCBInfo, with its application's resource-file reference. The next slice needs
an actual virtual file/fork record supplying this information, then the following
startup requests. Do not supply arbitrary successful file metadata. Relevant
primary layouts: https://dev.os9.ca/techpubs/mac/Files/Files-122.html and
https://dev.os9.ca/techpubs/mac/Files/Files-301A.html. PPC StdCLib support and both
compiler-generated applications reaching their C main remain incomplete.

## 2026-10-14: virtual file metadata and compiled 68K acceptance

Application loading now retains original fork bytes in a process-owned read-only
virtual file. PBGetFCBInfo reports actual fork metadata through both CPU ABIs,
with indexed/direct-reference lookup, error results, big-endian parameter blocks,
68K condition codes and validation of all output ranges before mutation. Named
resource lookup uses exact names and the same handle cache as ID lookup.

The unmodified MPW-generated 68K probe now completes startup and its C main:
1308 boundaries, 18 service calls, Exit(0). Its C checks verify resource presence
and length, cleared allocation contents and disposal. The explicit MPW acceptance
test checks the ordered service trace and is ignored unless its generated binary
exists; see fixtures/mpw/README.md to build and run it. This is one controlled
compiler-generated app, not original-app or general Toolbox compatibility.

Validation: all 47 tests passed with explicit GPU and MPW tests on Apple M1/Metal;
strict all-target clippy passed. Default suites have 44 passing/3 ignored tests.
New tests cover both FCB ABIs, real fork bytes/lengths, reference and volume errors,
atomic output faults, missing resource forks, named-resource case/raw bytes,
truncated Pascal names, cached identity and both named-resource ABIs.

Next: implement the PPC probe's StdCLib functions/data contracts, preserving the
real startup path. Then extend both compiled apps through deterministic events
and actual QuickDraw drawing. Virtual file read/open/close and additional resource
maps, async callbacks, window acceptance and real Mixed Mode are still incomplete.
The full rebuild goal remains active.

### PPC StdCLib jump ABI (in progress)

Implemented `StdCLib:__setjmp` and `StdCLib:longjmp` import routing and the
non-AltiVec MPW 256-byte guest jump buffer. Architecture-specific register
marshalling stays in `jump.rs`; the shared runtime dispatches typed calls.
Buffers retain reserved bytes, can be copied by the guest, and restore LR,
SP, TOC, nonvolatile integer/floating registers and FPSCR. Longjmp resumes
at the saved LR and converts a zero result to one, with MPW's CR0 behavior.
Evidence: MPW `setjmp.h` and the corresponding Apple StdCLib instruction
sequence, inspected using MPW DumpPEF. No Apple library implementation is
included in the runtime.

Validation: all 51 tests passed, including both offscreen GPU checks and
the compiled 68K MPW probe; strict all-target Clippy passed. New unit checks
cover copied buffers, register restoration, reserved bytes, and atomic
memory-fault behavior. A loaded generated PEF additionally exercises real PPC
instructions and CFM vectors, returning through the saved continuation.

Typed data imports for `_IntEnv`, `__C_phase`, `__target_for_exit`,
`_exit_status`, and `__NubAt3` now share writable process guest storage.
The standalone environment provides the SH signature, zero argc and guest
null-terminated argv/envp arrays; shell environment services remain absent.
Tests check import identity, byte layout and rejection of data/function
class mismatches, including weak imports.

End-to-end compiled PPC startup remains unverified: breakpoint and C exit
contracts still block loading. Continue with these contracts and a compiled
setjmp/longjmp probe; do not count generated fixtures as compiled PPC
compatibility. Apple StdCLib exit delegates to _RTExit, which also handles
shell context and registered cleanup. Inspect those semantics before
implementing application exit; do not silently discard cleanup work.


### Compiled PPC startup, packed structures and C exit

Both MPW builds of `probe.c` now pass their complete resource/size/cleared
allocation/disposal/exit interaction. PPC: 102 instruction boundaries, 6
service calls. The additional compiled `jump.c` validates empty application
arguments, two real nonlocal jumps (results 7 and zero converted to 1), and a
return of 37 from main through the standard C startup and C exit import.

PPC uses the engine's `EmulateData` policy: MPW's packed environment is read
at two-byte offsets. Checked memory bounds, atomic scalar stores and aligned
instruction fetch remain enforced and tested. No address-specific workaround
or change to guest structure offsets was used.

Standalone C exit records phase 5 and the status, then uses the existing shared
fragment lifecycle, including exactly-once CFM termination. This runtime does
not export stdio or atexit yet, so it cannot accept registrations and silently
lose them. Nonzero MPW shell exit-hook records fault explicitly. `_BreakPoint`
is an explicit terminal debugger-request diagnostic, not a silent no-op.
Normal MPW startup does not call it with the debugger flag clear.

Validation: all 57 tests passed, including two Metal GPU checks and three
MPW-generated application checks. Strict all-target Clippy passed. Generated
compiler outputs remain ignored and the five optional checks remain ignored
in the default suite (52 passing). No original-app compatibility is claimed.

Next: add deterministic event service ABI calls and a compiled event-driven
slice on both architectures, then connect real QuickDraw port/drawing calls
to the existing wgpu graphics model. Keep C runtime cleanup expansion and
real Mixed Mode descriptors explicit remaining work. Window presentation
acceptance and the complete rebuild remain unfinished.


### Shared OS event queue and compiled event program

Added PostEvent, GetOSEvent, OSEventAvail, FlushEvents, SetEventMask and
GetEvQHdr through the architecture adapters. The guest-linked 20-entry queue,
packed records and system mask are shared authorities. Event input/time
snapshots are deterministic; queue overflow evicts the oldest entry, invalid
links fault, and invalid output storage cannot consume an event.

The same MPW `events.c` program passes on 68K and PPC, exercising masks,
posting, peeking, selective removal, flush stop masks and direct queue reads.
A separate test posts from real 68K instructions and consumes from PPC within
one loaded runtime, proving live shared state rather than comparing two queues.
Boundary tests cover twenty-entry overflow, guest mutation, record bytes,
null events, invalid output and cyclic links.

All 61 tests passed, including two GPU checks and four MPW acceptance tests
(the new compiled event test exercises both architectures). Strict all-target
Clippy passed. The default suite leaves six optional GPU/compiler tests ignored.
Generated artifacts remain under ignored target/mpw-probe. These are compiler
fixtures, not original-app compatibility evidence.

Next executable work: connect Toolbox event waiting/yielding to deterministic
scheduling, then QuickDraw port state and real drawing calls to wgpu. OS queue
support alone does not implement GetNextEvent/WaitNextEvent, window update or
activation, host input, or autorepeat. The full rebuild remains active.


### Cooperative WaitNextEvent and explicit runtime yield

WaitNextEvent now suspends through `Stop::Waiting { ticks }` and preserves its
pending request/reply. Repeated polling executes no instructions. Hosts can
inject matching events or call `elapse_ticks` to expire the wait; unmatched
events remain queued. Zero sleep completes immediately. Guest time wraps at
32 bits while remaining duration saturates. Mouse-region handles fail explicitly.

The new MPW `wait.c` passes on both architectures: immediate queued delivery,
three-tick timeout checked through TickCount, then host-injected input during
a suspended call. Unit/integration checks cover unchanged idle output, zero
instruction budgets, clock wrap, mask filtering, stopped-clock rejection and
unsupported regions. A CLI subprocess test verifies it resumes after a wait
instead of reporting the yield as application completion. Headless CLI policy
advances virtual time and keeps one total instruction budget across yields.

All 66 tests passed including two GPU checks and five MPW tests. Strict
all-target Clippy passed. Seven optional checks remain ignored by default.
This is low-level delivery/null timeout in a windowless process; system event
interception, mouse regions, window/high-level synthesis, GetNextEvent/EventAvail
and interactive host pacing remain unfinished.

Next: implement QuickDraw guest port/bitmap/region state and real drawing ABI
calls backed by the existing wgpu commands. Exercise a compiled app that combines
resources, drawing, and event consumption on each architecture; keep pixel
readback authoritative. Full rebuild completion remains unproven and active.

### Packed bitmap storage through the GPU backend

Implemented checked Basic QuickDraw BitMap decoding and synchronous packed-bit
conversion around the existing wgpu operations. Guest memory remains canonical;
row padding and bits outside logical width are preserved. Local coordinates and
pattern phase are translated for nonzero/negative bitmap origins.

A real GPU test verifies patterned XOR, a direct guest write between draws,
packed-byte and RGBA readback, and overlapping copies with preserved padding.
A CPU test rejects bad strides, PixMap flags, invalid bounds and insufficient
mapped storage. All 68 tests passed (three GPU tests and five MPW acceptance
tests included), with strict all-target Clippy passing.

This is preparation for the real QuickDraw ABI, not a compiled drawing-app
milestone. Next implement GrafPort/current-port state, rectangular regions and
InitGraf/OpenPort/SetPort/SetPortBits/PaintRect-related adapters. The bitmap
bridge requires matching GPU extents and supports nonempty monochrome bitmaps
only, with the backend's 4096-pixel dimension limit. Keep these restrictions
explicit while expanding; do not claim original-application drawing yet.

### Guest GrafPort service layer

Added a guest-authoritative port binding, OpenPort initialization with two
shared-heap region handles, current-port selection, bitmap replacement,
rectangular clipping, and PaintRect command preparation. GPU tests read port
changes directly and verify clipped pixels, patXor, port-origin pattern phase,
and offscreen drawing without screen visibility clipping. Custom procedures
and recording state fail explicitly.

All 70 tests passed with four GPU checks and five MPW acceptance checks enabled;
strict all-target Clippy passed. After refining offscreen visibility behavior,
the affected port tests and strict Clippy passed again. These new port tests
exercise service records directly, not CPU QuickDraw calls.

Next: implement InitGraf global binding and screen policy, expose port/drawing
services through real 68K/PPC ABIs, and add the compiled resource/drawing/event
acceptance app. MPW QDGlobals has 76 private bytes, randSeed, screenBits, a 68-byte
Cursor, five 8-byte patterns, and thePort at offset 202; InitGraf receives the
address of thePort. GrafPort is 108 bytes. Verify initialization semantics before
publishing the ABI; do not add a no-op InitGraf. Loaded apps still cannot draw
through QuickDraw, and the full rebuild remains active.


### Real QuickDraw ABI and compiled resource/drawing/event parity

InitGraf now binds application-owned QDGlobals, leaves thePort nil, seeds the
public globals, and writes the 68K A5 anchor. The loader reserves a fixed
512x342 packed monochrome screen without acquiring a GPU. OpenPort, SetPort,
GetPort, SetPortBits, ClipRect, PaintRect, PenMode and PenPat now reach the shared
port/heap/GPU services through real 68K traps and PPC imports.

The MPW `draw.c` application passes on both CPUs: nil-port validation, two-port
switching, offscreen bitmap selection, clipping, resource lookup, gray/black
patterns and XOR, direct bitmap writes, padding checks and event consumption.
The harness requires the final success marker and compares each GPU RGBA image
with explicit expected bytes, not just with the other architecture. It supplies
an 8x4 GPU target; file CLI graphics preparation is not implemented yet.

Compilation established that both targets must define QDGlobals storage for this
toolchain. The speculative InterfaceLib qd export was removed before commit.
The build wrapper now detects MPW compiler/linker diagnostics despite zero host
exit codes. InitGraf validation is tested to avoid partial writes on a bad A5
anchor. All 72 tests and strict all-target Clippy passed, including four standalone
GPU checks and six MPW tests (one additionally requires the GPU). Ten tests are
optional and ignored by default.

Next: support graphics-device preparation/resizing in the ordinary file host,
then expand through ClosePort, regions and window/event interactions. Current
limitations include fixed monochrome screen policy, matching bitmap/GPU extents,
rectangular clipping, no custom QDProcs/recording, no color ports or windows,
and no original-app compatibility evidence. Window presentation and real Mixed
Mode remain unverified/incomplete. Keep the full rebuild goal active.


### File-mode graphics and differently sized bitmap targets

The file CLI now accepts `--graphics` for CODE and PEF runs and creates one
512x342 monochrome GPU backend. CPU-only runs retain their GPU-free path.
Packed bitmap drawing can use a different extent through a temporary texture
on the same device, queue, compute pipeline and shared error state. Guest bytes
remain authoritative and are synchronized before the scratch texture is dropped.

Both compiled drawing applications completed through the actual CLI: 68K used
1701 instruction boundaries and 33 services; PPC used 341 and 21. The acceptance
test compares CLI completion and execution counts with the run that verifies
success-marker ordering and exact pixels. A separate GPU test draws a padded
10x2 bitmap through a 512x342 backend and checks packed bytes and padding.
All 73 tests and strict all-target Clippy pass. Eleven tests require optional
GPU/MPW dependencies; the default suite retains 62 tests.

This does not verify window presentation or original application compatibility.
Scratch targets are recreated per differently sized draw. Equal-sized offscreen
drawing can replace the backend texture, so future presentation must refresh
screen memory explicitly. Next: screen presentation from canonical guest memory,
port disposal and the region/window services required by application slices.


### Canonical screen refresh

Added a read-only packed-bitmap presentation path and `Runtime::refresh_screen`.
The display comes from the fixed guest screen buffer, independently of the
current GrafPort and the most recently drawn offscreen bitmap. File graphics
runs refresh the screen after execution. Presentation decodes packed storage
and uses the existing wgpu pipeline without writing back to guest memory.

A GPU test first uploads a black, read-only offscreen bitmap with the same size
as the screen, then refreshes a mostly white screen containing a direct guest
write. Exact readback proves the offscreen image cannot select the screen,
subsequent direct writes appear, and both buffers remain unchanged by refresh.
All 74 tests and strict all-target Clippy pass, including both compiled CLI
applications. Twelve optional GPU/MPW tests remain excluded by default.

Window presentation is still unverified and loaded apps are not yet integrated
with the window event loop. Next: connect this refresh boundary to loaded-app
window presentation and implement port disposal/region services. The complete
rebuild, real Mixed Mode and original-application acceptance remain unfinished.


### Loaded applications in the shared window event loop

CODE/PEF file runs now accept `--window` and optional `--smoke`. Both use the
same surface/presentation path as the graphics fixture, with a fixed 512x342
screen refreshed from canonical guest bytes. CPU work is sliced at 10,000
boundaries per redraw under one total budget; event waits return control to
the host and use elapsed ticks. Guest exit preserves the final display until
window close. Fixture-only mouse palette changes are disabled for loaded apps.

All 77 tests and strict Clippy pass. Three new GPU-independent tests cover
cross-slice budget exhaustion, waiting without spinning followed by timeout,
and terminal execution on both CPUs. Default tests number 65; twelve optional
GPU/MPW tests were also enabled and passed locally.

Both compiled drawing applications completed inside the actual window loop:
68K at 1701 boundaries and PPC at 341. The fixture, 68K and PPC bounded presentation
smokes still failed to acquire/present/resize within 600 attempts. Window output
is therefore unverified; guest completion does not count as visible rendering.
Real host input delivery, window-manager services, real Mixed Mode and original
application acceptance remain unfinished. Next: host input mapping and the
port/region/window services needed by representative application interactions.


### Host mouse delivery into the shared event queue

Added a host pointer adapter and wired loaded-window pointer motion, left-button
transitions, modifier changes and focus loss. Physical coordinates account for
aspect-fit letterboxing; outside presses are ignored, while an outstanding
release remains deliverable outside the viewport. Masked events still update
physical button state. Both CPUs continue consuming the same guest queue.

Three new tests verify coordinates/invalid inputs, a real loaded PPC
WaitNextEvent waking on host mouse input with exact timestamp/position/modifiers,
and masked button changes plus focus-loss cleanup. All 80 tests and strict
Clippy pass, including twelve explicitly enabled optional GPU/MPW tests.
The default suite now contains 68 tests.

Live desktop interaction and presentation remain unverified. Keyboard events,
Caps Lock, cursor drawing and Window Manager dispatch remain unfinished. Next:
keyboard delivery and complete application-driven port/region/window services;
original application acceptance and real Mixed Mode are still outstanding.


### Basic port disposal through both ABIs

Implemented ClosePort using the shared process heap, with 68K A87D Pascal and
PPC InterfaceLib adapters. Both region handles are preflighted before freeing
either. Caller-owned bitmap/port storage and the current-port selection remain
unchanged. Color ports and malformed handles produce explicit errors.

Unit tests verify region reclamation, reuse on reopen, preserved caller storage,
and invalid/aliased second handles without partial disposal. Rebuilt the MPW
drawing application on both architectures; it now closes an inactive port,
checks the current port, reopens, switches, then closes both ports. Acceptance
requires three ClosePort calls, final success marking and exact pixel parity,
including CLI execution. All 82 tests and strict Clippy pass; the six MPW tests
were rerun after rebuilding and strengthening the acceptance check.

The default suite contains 70 tests; twelve optional GPU/MPW tests were enabled
locally. Presentation remains unverified; keyboard input, complex regions,
Window Manager services, real Mixed Mode and original-app acceptance remain
unfinished. Next: continue the complete port/region/window application slice.


### Existing-port initialization

Implemented InitPort through both real ABIs. OpenPort and InitPort now share one
defaults implementation; initialization retains both region handles, resets
their bounds, restores drawing fields/screen bitmap, and selects the target port.
All destination validation precedes mutation. Complex-region resizing and color
ports remain unsupported with explicit errors.

Rebuilt both MPW drawing applications with checks for region-handle identity,
current-port selection, restored bitmap and pen mode. Both retain exact GPU
pixel parity and complete through the file CLI. A new unit test compares the
entire reset port and both region records, and verifies malformed input leaves
port/region bytes unchanged. All 83 tests and strict Clippy pass (71 default,
twelve optional GPU/MPW tests explicitly enabled).

This completes the currently supported basic rectangular-port lifecycle, not
full QuickDraw compatibility. Next work remains region storage/operations and
Window Manager behavior driven by representative applications. Presentation,
keyboard input, real Mixed Mode and original-application acceptance are still
unfinished; the rebuild goal remains active.


### Resizable shared guest handles

Implemented SetHandleSize on the shared heap through 68K A024 and PPC imports.
Handles retain identity while data can move; shrinking frees the aligned tail,
adjacent growth remains in place, and relocation preserves the original prefix.
Allocation failure leaves the original block intact. Zero-size, nil, disposed
and guest-corrupted master pointers have explicit tested behavior.

Three new heap tests cover relocation/in-place resize, complete coalescing after
disposal, OOM atomicity, zero size and invalid handles. Rebuilt both MPW drawing
programs with grow/shrink, contents, GetHandleSize and MemError checks. All 86
tests and strict Clippy pass (74 default plus twelve optional GPU/MPW tests).

This provides storage needed for complex QuickDraw regions. Region operations
still need to adopt it; handle locking, purging, grow-zone callbacks and general
heap compaction remain unimplemented. The complete rebuild goal remains active,
with original-application acceptance and real Mixed Mode still outstanding.


### Region storage and clip save/restore

Added NewRgn, DisposeRgn, RectRgn, CopyRgn, GetClip and SetClip with shared 68K/PPC
services. Region payloads use resizable guest handles. Copies preserve complete
bounded payloads, support self-copy, and retain destination-handle identity.
RectRgn normalizes empty rectangles. Clip get/set copy contents into existing
handles rather than exchanging pointers.

Rebuilt both MPW drawing programs with region allocation, saving the current
clip, setting a temporary clip, copying/restoring the original and disposal.
Exact expected drawing pixels and final success markers pass on both CPUs and
through the CLI. Two new tests cover empty/rectangular records, disposal,
variable-length payload relocation, self-copy and malformed source bounds.
All 88 tests and strict Clippy pass (76 default, twelve optional GPU/MPW checks).

Complex region contents can be stored/copied but are not yet interpreted for
drawing; PaintRect still rejects them explicitly. Next: bounded region geometry
and Boolean operations, then GPU clipping and Window Manager application slices.
The full rebuild and original-application/mixed-mode acceptance remain unfinished.


### Bounded complex-region decoding and hit testing

Added inversion-point decoding with ordered rows/endpoints, closure/bounds
validation and a 65,536-rectangle expansion ceiling. PtInRgn and EmptyRgn share
this geometry across the 68K/PPC ABI adapters. Three tests verify donut holes,
disjoint negative-coordinate spans, exact half-open edges, malformed/truncated
records and a small encoded input that would expand excessively.

Both MPW drawing applications were rebuilt with EmptyRgn/PtInRgn checks; they
pass with unchanged expected drawing pixels and CLI execution. All 91 tests and
strict Clippy pass (79 default plus twelve optional GPU/MPW tests). Previous
handle-resizing CI was confirmed successful; the newer region-storage CI was
still running when checked.

Complex decoding is validated with independent synthetic geometry, not a native
Macintosh oracle. GPU clipping still rejects complex regions. Next: feed the
validated geometry into the wgpu clipping path, then add Boolean region APIs
and complete window/application interactions. The full rebuild remains active.


### Complex-region GPU clipping

Connected region decoding and clip/visible intersection to the existing integer
WGSL drawing pipeline. Geometry is translated at the bitmap boundary and tested
inside one compute pass; each covered pixel receives its transfer once. CPU
work remains storage conversion and geometry, with no pixel drawing fallback.
Intersection and shader-work ceilings reject excessive complexity explicitly.

A GPU test checks a hole, exact XOR pixels, padding, direct writes between draws,
complex visible/clip intersection and malformed input without pixel changes.
Both MPW programs were rebuilt with an encoded complex clip and independently
checked intermediate/final XOR bytes. All 92 tests and strict Clippy pass; the
extended visible-region test was rerun after adding that coverage. There are
79 default tests and thirteen optional GPU/MPW checks, all enabled locally.

Native comparison, arbitrary region construction/Boolean operations and Window
Manager application behavior remain outstanding. Window presentation is still
unverified. Next: region Boolean APIs and representative window interactions;
the full rebuild and real Mixed Mode acceptance remain unfinished.


### Boolean regions constructed by guest applications

Implemented SectRgn, UnionRgn, DiffRgn and XorRgn through the shared region/heap
layer with real 68K/PPC ABIs. The bounded band sweep constructs result geometry,
then encodes it before resizing the destination. Empty/rectangular results use
canonical ten-byte records. Input snapshots support aliased destinations.

An independent point-set test covers all four operations across overlapping,
negative, empty and holed regions. A service test verifies source/destination
aliasing and malformed-input preservation. Both MPW programs now build their
holed clip with DiffRgn rather than a raw byte fixture, exercise all four APIs,
and check intermediate/final GPU pixels. All 94 tests and strict Clippy pass
(81 default, thirteen optional GPU/MPW checks enabled locally).

Native comparison remains outstanding. General region recording, shape drawing,
transforms and Window Manager interactions are unfinished. Next: use the region
model in complete window/application slices. Presentation, keyboard delivery,
real Mixed Mode and original application acceptance still prevent rebuild completion.


### Original-application launch metadata

External Super Mines Demo and Centaurian FAT binaries now drive bounded startup
checks; hashes and exact limits are in [original-applications.md](original-applications.md).
Super Mines exposed a missing CurStackBase, causing startup writes through zero.
CODE launches now publish CurrentA5, CurStackBase and word-sized CurJTOffset from
the validated A5 layout. Oversized jump-table offsets are rejected. The trace
example includes all 68K registers and the current opcode.

A generated regression reads these globals and writes through CurStackBase.
Super Mines now initializes its globals and reaches unsupported trap $A9A3;
Centaurian 68K reaches the same trap. Centaurian PPC rejects unresolved SetSoundVol.
All 95 tests pass, including thirteen optional GPU/MPW checks; strict Clippy passes.
Next: resource lifetime behavior needed by original startup, then bounded reruns.
No original application acceptance is claimed. The full rebuild remains active.


### Resource release through shared process state

ReleaseResource now resolves through the 68K Pascal trap and PPC InterfaceLib
adapter to the same process service. Successful release frees resource data and
its master allocation, clears cached identity and permits reload of fork bytes.
Ordinary/nil/unknown handles report resNotFound without freeing unrelated memory;
resources carrying resChanged remain resident with noErr. Resource editing and
attribute mutation APIs remain unimplemented.

Independent tests verify reclamation, invalid identity, restored bytes and changed
resource retention. Rebuilt MPW programs on both CPUs reject an ordinary handle,
modify and release a resource, reload its original bytes, and release it again.
All 97 tests pass (84 default and thirteen optional GPU/MPW checks enabled);
strict all-target Clippy passes.

Both original 68K programs now pass ReleaseResource. Super Mines next stops at
$A8FE, PC $0100D004, after 13511 instruction boundaries. Centaurian reaches a
write fault at $010000FC inside a CODE segment, PC $01000270, after 92039 boundaries.
Next: investigate startup relocation and CODE permissions, then the Toolbox trap.
No application interaction acceptance is claimed; full rebuild remains active.
GitHub publication remains pending the user's response to the approval request.


### Writable classic CODE and startup relocation

Centaurian's blocked write was verified as startup relocation of an embedded JSR
operand: adding A5 changes $00000028 into $00400028. Classic CODE segments now
map read/write/execute, consistent with classic RAM code semantics; all accesses
still obey mapped bounds. PEF permissions remain independently selected by kind.
A generated guest test relocates a longword, patches a MOVEQ instruction and
executes the updated instruction through the real 68K engine, then verifies that
writing beyond the segment is rejected.

All 98 tests pass, including thirteen optional GPU/MPW checks, and strict Clippy
passes. Centaurian now completes these relocations and reaches unsupported $A647
at PC $010000C0 after 96660 boundaries. D0 selects LoadSeg and A0 points into the
application, identifying trap replacement as the next loader-related work.
Super Mines remains at $A8FE. Original application interactions, presentation and
real Mixed Mode remain unfinished. The rebuild goal remains active.


### Shared Toolbox replacement table and patch execution

SetToolboxTrapAddress now updates a guest-resident table used by both ABI lookup
paths and 68K A-line dispatch. Guest patches run with normal budgets, can call
other Pascal services and chain to saved built-in veneers. Auto-pop glue and
restoring original entries preserve stack behavior. Invalid pointers fault on
execution. A test installs a pointer through PPC and executes that 68K routine
against the same memory. This is shared state, not a Mixed Mode transition.

All 101 tests passed with optional GPU/MPW checks enabled; an additional nested
Pascal-service regression then passed with the four-test patch suite. Total: 102
passing tests. Strict all-target Clippy passes. An initial address collision with
StdCLib was corrected before verification; the table is now at $71100000.
Centaurian passes replacement of LoadSeg, UnloadSeg and ExitToShell, then stops at
HLock ($A029), PC $01000384, after 96700 instruction boundaries. Next: handle locks
and continued original-app execution. OS replacement, come-from patches, native
PPC patch dispatch and real Mixed Mode remain unfinished. Rebuild remains active.
The user approved pushing the accumulated commits to dev/future.


### Locked handles, address stripping and resource-loaded execution

Implemented HLock/HUnlock through one owned-handle model. Locked resize preserves
the pointer, permits in-place shrink/growth, and reports memFullErr if moving is
necessary; unlocking allows movement. resLocked resources load locked. Tests
cover repeated locking, invalid handles, failure preservation, disposal and reload
attributes. Rebuilt MPW programs verify lock/resize behavior on both CPUs.

Centaurian then required StripAddress and execution of CODE copied into heap RAM.
StripAddress preserves all bits in the runtime's 32-bit startup mode. The MPW PPC
probe did not emit that service call, so an explicit PEF import fixture verifies
its adapter alongside 68K. Heap mappings now permit code execution; both engines
are tested against rewritten instructions and mapping boundaries.

All 107 tests passed with optional GPU/MPW checks enabled, and strict Clippy passed.
The heap coherence test was then strengthened to reuse each CPU across writes and
rerun. Centaurian now reaches $A204 at PC $10002B32 after 103852 boundaries. Next:
inspect that I/O request and proceed with original application-driven slices.
Purging, compaction, 24-bit mode, real Mixed Mode, presentation and application
interaction acceptance remain unfinished. The full rebuild remains active.


### Sound Driver volume shared across original startup paths

The diagnostic trace now prints a bounded A0 memory window on 68K faults.
Centaurian's $A204 parameter block identified Sound Driver reference -4, selector
2 and inline volume 3. That control path and PPC SetSoundVol/GetSoundVol now share
SdVolume's low three bits at $0260, preserving other bits and observing direct
guest writes. PBControlSync uses the same implementation. Unsupported drivers,
selectors, asynchronous requests and out-of-range levels remain explicit faults.

Three tests cover both ABIs, parameter/output preservation, read-only and invalid
memory, direct writes and unsupported requests. All 110 tests pass with optional
GPU/MPW checks enabled; strict Clippy passes. This implements observable volume
state only; playback and host audio are not implemented or claimed.

Centaurian 68K advances to $A8FE at PC $10004462 after 103860 boundaries, the same
trap blocking Super Mines. Centaurian PPC now rejects LocalToGlobal instead of
SetSoundVol during strong-import validation; no application instructions execute.
Next: the common $A8FE startup requirement and QuickDraw coordinate transforms.
Original application interactions, presentation, audio playback and real Mixed
Mode still prevent rebuild completion. The goal remains active.


### Local/global QuickDraw coordinates through both ABIs

Implemented LocalToGlobal and GlobalToLocal against the current guest GrafPort's
bitmap origin. Whole-point writes preserve failure atomicity; color ports fail
explicitly. Geometry tests check negative/nonzero origins, changing guest records,
current-port changes, word edges and invalid/read-only outputs. MPW programs on
both architectures verify conversion and its inverse and switch ports before
continuing exact GPU pixel checks.

All 112 tests pass, including thirteen optional GPU/MPW checks. Strict all-target
Clippy passes. Centaurian's PPC loader now resolves LocalToGlobal and next rejects
GetMemFragment. This exposes the outstanding dynamic CFM loading requirement;
no original PPC application instruction has executed. The 68K programs remain
at $A8FE. Next: dynamic fragment loading and the common 68K startup requirement.
Color ports, native overflow comparison, real Mixed Mode, presentation and full
original application interactions remain unfinished. Rebuild goal stays active.


### Separate PEF preparation from process startup

PEF parsing, relocation, mappings and import adapters now produce a prepared
image before application startup installs process resources, low memory, stacks
and CPU lifecycle state. This is a behavior-preserving boundary for CFM work;
GetMemFragment remains unresolved and original application stopping points are
unchanged. All 112 tests pass with optional GPU/MPW tests enabled; strict
all-target Clippy passes.

Next: parameterize fragment address reservations and shared StdCLib storage,
retain validated exports and optional main routines, atomically adopt mappings
into the existing runtime, track connections, and execute initializers through
a dedicated guest continuation before publishing load results. Failure cleanup
and eventual CloseConnection termination must preserve existing process state.
Fixed addresses and the application-only lifecycle are not dynamic CFM support.
The full rebuild goal remains active.


### Relocatable prepared fragments and atomic memory adoption

PEF preparation accepts explicit section, import-gate, transition-vector,
container and initialization-block reservations. Reservations reject overlap,
misalignment, overflow and section exhaustion. Main routines are optional during
preparation (Apple PEF loader header, mainSection = -1); application launch still
requires one. All present routine entry points and TOCs are validated before
adoption. StdCLib storage is installed by application startup, not preparation.

Memory can adopt a prepared image without cloning live storage; any mapping
collision leaves the destination unchanged. Tests run relocated imports through
the real PPC backend, place two images in one memory space while preserving the
first image's changed data, inspect relocated initializer/container pointers,
and reject invalid reservations and malformed lifecycle vectors. Boundary tests
cover partial collision rollback, permissions and storage ending at 2^32.

All 117 tests pass, including thirteen optional GPU/MPW checks; strict all-target
Clippy passes. This is loader machinery, not GetMemFragment support. There is no
active connection allocator yet; future CFM allocation must track every live
reservation, including intercepted gates which do not have memory mappings.
Next: retained/validated exports, connection identity and allocation, initializer
continuations in the running process, and failure/unload cleanup. Initializer
IDs remain application defaults until connection tracking lands. Original
application stopping points are unchanged; the full rebuild remains active.

Specification: https://leopard-adc.pepas.com/documentation/mac/runtimehtml/RTArch-96.html


### Resolve and retain PEF exports

Prepared images and application load metadata now retain exports in PEF table
order, with original byte names, annotation classes and guest addresses. Section
offsets, absolute values and re-exported import indices resolve explicitly.
Hash words and bucket ownership are checked, including overlapping/orphaned
chains, duplicate names, reserved classes, invalid indices and out-of-section
offsets. Total copied/hashed name bytes are bounded by the image limit.
End-of-section symbols are retained as metadata; guest access remains checked.

Three deterministic tests cover all classes/address forms, MacRoman and case
distinctions, signed hash overflow, malformed table rejection and execution
through an exported transition vector on the real PPC backend. The PPC MPW probe
now exports main using PPCLink -export; its acceptance test requires that real
linker-generated record and validates its executable transition-vector target.
The rebuilt compiler probes all pass. All 120 tests pass including thirteen
optional GPU/MPW checks; strict all-target Clippy passes.

This does not yet implement FindSymbol or GetMemFragment. Next: connection
identity/reservation tracking, guest symbol-query ABIs, and dynamic loads with
initializer continuations and rollback in the existing runtime. Original
application stopping points remain unchanged; rebuild goal remains active.

Source: https://leopard-adc.pepas.com/documentation/mac/runtimehtml/RTArch-100.html


### CFM connection identity and symbol queries through both ABIs

Application startup now registers its immutable exports in process-owned CFM
state before running initialization. The initializer's connection ID identifies
that table. FindSymbol, CountSymbols and GetIndSymbol use shared services through
PPC imports or 68K $AA5A selectors 5–7, with Pascal stack cleanup, byte class
outputs, optional pointers and -2801/-2802 errors. Output validation precedes
mutation. Load metadata shares immutable exports instead of copying names.

Four tests exercise both real CPU adapters against one process, exact names,
errors, output atomicity, and queries during initialization/main across single
instruction budgets. Rebuilt MPW probes validate compiler-generated CFM calls;
the PPC initializer and main query their own connection successfully. All 124
tests pass including thirteen optional GPU/MPW checks; strict all-target Clippy
passes. Zero-based GetIndSymbol follows the later Apple reference despite the
older manual's conflicting one-based example; native comparison is pending.

GetMemFragment, connection allocation/reference counts, guest initializer
continuations and CloseConnection remain unfinished. No new original-application
acceptance or real Mixed Mode is claimed. Next: use these process-owned
connections for dynamic fragment loads with rollback and termination. The full
rebuild remains active.


### Resource-based PPC fragments load, run and unload in one runtime

GetMemFragment now adopts prepared images into the current memory/process,
registers their exports and runs initializers with preserved caller CPU/stack
state. Nested calls are budgeted guest execution. Reference/find/private-copy
options, monotonic IDs and bounded address reservations are implemented. Main
is a symbol address (possibly data or absent), not an automatically dereferenced
code pointer. CloseConnection runs the final terminator and removes mappings
and gates; process exit also closes remaining dynamic roots. Failed initializers
return -2821 with fragment diagnostics and do not publish load outputs or run
termination. Shared StdCLib globals are installed once and survive unloading.

Eleven deterministic tests cover shared memory, full caller-state preservation,
source/name/ID init metadata, queries, failed/corrupt/unresolved loads, reference
lifetimes, independent private data, nested loading and termination order,
budgets, data/absent main symbols, invalid outputs, address exhaustion/reuse and
shared StdCLib storage. The new MPW-built resource plugin completes actual C
GetResource/HLock/GetMemFragment/FindSymbol/function-call/CloseConnection/
HUnlock/ReleaseResource flow and posts a success marker. All 136 tests pass
(122 default plus fourteen optional GPU/MPW checks); strict all-target Clippy
passes. Full-suite output precedes two additional focused dynamic tests, which
also pass on the same production code.

Centaurian PPC now stops at unresolved InterfaceLib:Random rather than
GetMemFragment; no original PPC application instruction has executed. 68K
originals remain blocked at $A8FE. Next: Random and the shared 68K startup
requirement. Dynamic loading is a bounded PPC memory-fragment subset: fourteen
slots, built-in imports only, no in-place/disk/shared-library loading or
dependency closure ordering. 68K-to-PPC load/unload calls fault explicitly;
real Mixed Mode, presentation and original application interaction acceptance
remain unfinished. The full rebuild goal remains active.


### QuickDraw Random and comparison scope

Random now uses one shared service for the 68K $A861 trap and PPC InterfaceLib
import. The unsigned seed lives at QDGlobals.thePort minus 126, initialized by
InitGraf; it is never duplicated in host state. Direct guest writes take effect
on the next call. A selected port and a GPU are unnecessary. The 68K result is
a reserved stack word; PPC receives a sign-extended short. Invalid storage
faults explicitly, and the 68K result slot is validated before advancing seed.

The recurrence is `(seed * 16807) % 2147483647`; the low signed word is returned,
with -32768 mapped to zero. Comparison against the existing Systemless 68K
handler (`src/systems/macintosh/trap/toolbox.rs`) and PPC `ppc_random_at`
(`src/systems/macintosh/loader/ppc/loaded_app_execution.rs`) found one intentional
difference: legacy normalizes zero to one, whereas this implementation leaves
zero at zero as in the published original Apple QuickDraw
[Random source](https://raw.githubusercontent.com/jrk/QuickDraw/master/Util.a).
This is source comparison, not an execution comparison against a native Mac.

The user authorized comparison against existing Systemless instead of native
System 7. Native comparison is therefore optional, not a completion gate for
these slices. Historical notes identifying missing native evidence still describe
the evidence available; legacy agreement must not be labeled native validation.

Two focused tests alternate real CPU adapters on one guest seed and cover direct
seed writes, unsigned edges, the exceptional output, neighboring memory and
invalid storage. The MPW drawing fixture additionally checks sequence and seed
writes through both compiled calling conventions. Centaurian PPC now rejects
FrameRoundRect at load time; no original PPC application code has run. Next:
shared $A8FE startup support and rounded-rectangle drawing, then continued
original application interaction. Real Mixed Mode and full rebuild acceptance
remain outstanding.

Validation: all 138 tests passed, including 14 optional GPU/MPW tests; the MPW
fixtures rebuilt successfully and strict all-target Clippy passed.


### Rounded-rectangle drawing through the shared GPU path

FrameRoundRect ($A8B0) and PaintRoundRect ($A8B1) now marshal reverse Pascal
arguments or PPC r3–r5 into one shared service. Framing reads the live guest pen
size and visibility; painting uses the live pen pattern/mode/colors. Rectangles,
bitmap bounds, port bounds and complex clip/visible regions share the existing
synchronous packed-bitmap/wgpu boundary. Repeated XOR frames apply each pixel
once. Negative pen dimensions, recording and custom QDProcs fail explicitly.

A bounded CPU geometry module uses integer fixed-point ellipse spans and
subtracts an inset rounded rectangle for frames. Wide intermediates handle
extreme guest dimensions without the legacy narrow-coordinate arithmetic.
This is geometry preparation, not a CPU pixel renderer: wgpu performs pattern
application, transfer modes and indexed pixel writes. Existing geometry/GPU
work limits still apply, so exceptionally complex draws can fail explicitly.

Comparison evidence: the standalone `future/fixtures/rounded-comparison.rs`
harness executes the legacy span generator and new coverage generator on
129,600 filled shapes and 16,384 frames, including odd corners, clamping,
translation and unequal/zero pen dimensions. All agree in the tested domain.
This is legacy comparison, not a claim of native Macintosh pixel conformance.
The runtime does not depend on that harness or legacy code. Fixed pixel tests,
extreme coordinate tests and a GPU clipping/pattern/direct-write test cover
additional boundaries. The MPW C fixture runs copy/XOR frames and painting on
both real architectures and checks every output byte and row padding.

The shared 68K $A8FE stop is InitFonts. The legacy no-op relies on built-in font
tables; those do not exist in the rebuild, so no unsupported success stub was
added. Centaurian PPC now resolves FrameRoundRect and stops at GetPattern before
application execution. Next: pattern-resource access and a substantive shared
Font Manager initialization/text slice, followed by original app interaction.

Validation: all 141 tests pass, including 15 optional GPU/MPW checks. The MPW
fixtures rebuilt successfully and strict all-target Clippy passes.


### Pattern resources and port color setters

GetPattern ($A9B8 / InterfaceLib) is now an ABI adapter for the shared
GetResource('PAT ', signed ID) service. It retains resource handle identity,
loads actual fork bytes, exposes guest mutations, and uses the same resource
errors and lifetime as direct GetResource calls. The 68K adapter validates the
reserved long result before lookup. No default pattern is invented on a miss.
Legacy comparison: `trap/dialog.rs` GetPattern and PPC `dispatch_resources.rs`
use the same typed-resource lookup contract. The rebuild still has only its
application resource map; system resource-chain search remains unfinished.

ForeColor ($A862) and BackColor ($A863) now write the selected basic GrafPort's
32-bit fgColor/bkColor fields through one service. Rendering reads those same
guest fields. Setters preserve the full value; drawing currently accepts only
black/white and faults on unsupported colors. Color GrafPort setters fail
explicitly. This supports the basic-port slice, not legacy Systemless's broader
RGB/color-port behavior. No host color cache was introduced.

A real-CPU test alternates GetPattern between PPC and 68K, checking one handle,
signed IDs, guest mutations and misses. The MPW fixture retrieves a signed-ID
PAT resource, compares its handle with GetResource, draws through its bytes,
releases it, reverses foreground/background, and checks the exact inverse gray
pattern on both architectures. Port tests cover selection and read-only or
color-port rejection. All 143 tests pass (128 default, 15 optional GPU/MPW),
fixtures rebuild, and strict all-target Clippy passes.

Centaurian PPC now stops on NewWindow before application execution. Next:
shared window records, ownership and drawing/visibility integration, alongside
substantive Font Manager startup for the 68K InitFonts stop. Original app
interaction and real Mixed Mode remain required for full rebuild acceptance.


### Stable pointer allocation for Window Manager storage

Inspection of NewWindow showed that nil wStorage needs a stable nonrelocatable
allocation. Added NewPtr/NewPtrClear, GetPtrSize, SetPtrSize and DisposePtr to
both ABI adapters and the shared process heap. Pointers and handles consume the
same coalescing allocator, with separate ownership tables. Resizing never moves
a pointer; it can shrink or grow into adjacent free space and otherwise reports
memFullErr without changing its contents or logical size. Zero-sized pointers
retain unique live addresses. The low-memory MemErr result remains shared.

Invalid, interior, disposed or handle-owned addresses report memWZErr (-111)
without freeing or resizing any allocation; invalid GetPtrSize returns that
signed error. This is an explicit defensive policy, not native validation of
undefined stale-pointer uses. The application-only 4 MiB heap, no compaction or
grow-zone callbacks, and unsupported system-heap variants remain limitations.
Legacy comparison: memory.rs SetPtrSize documents the same stable-address
contract; the new allocator can additionally grow into reclaimed adjacent space.
Apple's [nonrelocatable block reference](https://dev.os9.ca/techpubs/mac/Memory/Memory-74.html)
and [Memory Manager reference](https://dev.os9.ca/techpubs/mac/pdf/Memory/Memory_Manager.pdf)
provide the allocation and resizing contracts (pp. 2-36–2-43).

Three tests cover coalescing/reuse, clearing, zero size, exhaustion, blocked
resize, pointer/handle ownership isolation, and real PPC/68K operations on one
allocation. The MPW probe also checks successful/failed resize and retained
bytes through both compiled ABIs. NewWindow remains unresolved; this implements
its memory prerequisite, not window creation. Next: window ownership/records,
visibility and update integration, plus Font Manager startup. Full rebuild
acceptance remains outstanding.

Validation: all 146 tests pass, including 15 optional GPU/MPW checks; MPW
fixtures rebuild successfully and strict all-target Clippy passes.


### First shared Window Manager lifecycle

InitWindows, NewWindow, FrontWindow, CloseWindow and DisposeWindow now share
Window Manager state across both CPU adapters. The implemented standard WDEF
variant is plainDBox (procID 2). Other definitions fault explicitly while font,
title-bar chrome and custom callback support are rebuilt. NewWindow marshals
eight PPC register arguments or 26 reverse Pascal argument bytes; Boolean values
come from the high byte of their 68K stack slots, and the reserved long result
is validated before popping arguments.

WindowRecords use the documented 156-byte basic layout and contain their own
GrafPort. Nil wStorage allocates a stable pointer; caller storage remains reusable
after CloseWindow, and DisposeWindow requires a live NewPtr allocation. Title,
clip, visible, structure, content and update regions use shared heap handles.
WindowList ($09D6) and nextWindow are the guest z-order authority. WMgrPort
($09DE) and GrayRgn ($09EE) are stable guest objects. Bitmap bounds translate
global screen pixels into each window's local coordinates. Structure/content/
update regions remain global; visRgn is local and subtracts visible structures
above it. The front visible record has hilited set. No duplicate host geometry
or separate PPC window model was introduced.

Creation erases exposed content and draws the one-pixel border through wgpu.
Closing subtracts covering windows, erases newly exposed content with the live
window background pattern/colors, accumulates updateRgn, and repaints exposed
desktop pixels. Existing application pixels outside the exposure are preserved.
The manager temporarily changes its drawing fields and restores the current
port afterward. All drawing uses the canonical shared screen bitmap. The
64-window and existing bounded region/GPU work limits are explicit. Invalid
input and initial record/handle allocation failure do not publish a new window;
region-growth failure during visibility recomputation is still a terminal fault.

Legacy comparison: trap/window.rs confirms record offsets, Pascal layout,
behind ordering and plainDBox's one-pixel structure outset. Tests cover local
visibility, occlusion, XOR-safe shared drawing infrastructure, preserving
unexposed pixels, update-region accumulation, hidden windows, owned/static/
caller-allocated storage, record reuse, exhausted heap and invalid parameters.
A real MPW application creates overlapping visible windows and hidden caller
windows, draws, closes and disposes through both ABIs, then returns exactly the
same gray desktop bytes. All 149 tests pass: 132 default and 17 optional GPU/MPW.
Strict all-target Clippy and the MPW rebuild pass.

Limitations: no activation/update event delivery, BeginUpdate/EndUpdate, show/
hide/move/select operations, GetNewWindow/WIND resources, menus/controls, title
styles or custom WDEF callbacks yet. The builtin definition uses internal shared
services; windowDefProc is nil and titleWidth is not measured for plain windows.
Original Centaurian PPC now stops at TextFont during strong-import validation;
none of its own PPC code has run. The original 68K InitFonts stop is unchanged.
Next: Font Manager initialization, shared text state/metrics/rendering, then
window event/update handling and original application interaction. Real Mixed
Mode and full rebuild acceptance remain unfinished; the goal stays active.


### Shared GrafPort text selection

TextFont, TextFace, TextMode and TextSize now marshal into one service that
writes the selected basic GrafPort's txFont (68), txFace (70), txMode (72) and
txSize (74). Port memory is authoritative and selecting another port preserves
each port's values. txFace is one byte; the adjacent filler byte is preserved.
Color ports and picture recording fail explicitly before changing text fields.
This selects text state; font loading, metrics and glyph drawing remain pending.

The real MPW fixture caught an ABI distinction: Universal Interfaces declares
TextFace(StyleParameter), where StyleParameter is short, whereas StyleField in
the port is unsigned char. The 68K trap therefore consumes a word argument and
the service stores its low byte. The other setters also consume words. Header
evidence is MPW QuickdrawText.h TextFace and MacTypes.h StyleParameter/StyleField.
Legacy quickdraw.rs has a heuristic accepting either byte of the argument word;
this slice follows the compiled MPW interface rather than guessing a style from
padding. The fixture verifies font, size, face and mode on two separate ports
through both architectures. Focused tests verify field widths, padding, argument
marshalling and invalid/read-only port atomicity.

Centaurian PPC now stops at TEStyleNew during strong-import validation. No
original PPC application code has executed; 68K applications still require
InitFonts. Next: Font Manager resource decoding and a licensed system-font
fallback, metrics and wgpu glyph drawing, followed by TextEdit and title-bearing
windows. The existing runtime has FONT/NFNT/FOND/sfnt decoders and bundled
URW/Coppet/Noto fonts as reference evidence; their implementation must remain
independent from the new runtime, and asset licenses must be preserved.

Validation: all 151 tests pass (134 default and 17 optional GPU/MPW); MPW
fixtures rebuild successfully and strict all-target Clippy passes.


### Bitmap-font decoding and resource-backed metrics

Added an independent checked FONT/NFNT strike decoder. It validates the 26-byte
header, character range, dimensions, bitmap extent, location monotonicity,
width-table offset, missing glyph and terminal word before exposing glyphs.
Glyph bearings remain wide signed values (unsigned offset byte plus kernMax),
advances remain distinct from bitmap widths, and masks retain exact packed-bit
pixels. Missing characters and -1 width entries resolve to the missing glyph.
Positive nDescent contributes the high word of the width-table offset; the
calculation uses wide checked arithmetic, including for 32-bit hosts. Resources
and per-glyph masks have explicit 16 MiB limits. Color/depth and auxiliary-table
formats fail explicitly while the first monochrome strike path is implemented.

GetFontInfo ($A88B / InterfaceLib) now resolves an explicitly selected plain
old-style FONT strike by family*128+pointSize, using the shared Resource Manager.
It reads live guest handle bytes on each call, preserves signed metric values,
and validates the eight-byte output before lookup/publication. Both ABIs write
the same FontInfo layout. Default/system/application font substitution, point
size zero, styled/scaled metrics, FOND associations and NFNT ID selection are
still unsupported and fault explicitly. This does not initialize the Font
Manager or render glyphs yet.

Evidence: Apple's [bitmap font resource](https://leopard-adc.pepas.com/documentation/mac/Text/Text-250.html),
[font-type bits](https://leopard-adc.pepas.com/documentation/mac/Text/Text-251.html)
and [extended offset](https://leopard-adc.pepas.com/documentation/mac/Text/Text-252.html)
references define the parsed layout. Legacy fonts/resources.rs provides a
separate comparison for packed bitmap locations, missing-glyph lookup and
bearing arithmetic. The rebuild does not retain its global font cache or clamp
bearings to i8; it also handles the documented extended offset, which the
legacy parser does not currently combine with nDescent.

Four focused tests cover exact masks/metrics/advances, signed bearings,
missing glyphs, every truncated prefix, corrupt table offsets/locations/types,
extended offsets, and actual PPC/68K GetFontInfo calls sharing mutated guest
resource data. A small original synthetic FONT resource in the MPW fixture
also supplies exact metrics through both compiled architectures. No Apple
font artwork or licensed fallback asset was copied. Centaurian's PPC strong
import stop remains TEStyleNew and the original 68K InitFonts stop remains.
Next: licensed system-font fallback and InitFonts, shared measurement and
wgpu glyph drawing, then TextEdit and title-bearing windows.

Validation: all 155 tests pass (138 default and 17 optional GPU/MPW); the MPW
fixtures rebuild successfully and strict all-target Clippy passes.


### Font Manager initialization and licensed fallback

InitFonts ($A8FE / InterfaceLib) now loads and validates an available guest
system FONT resource, otherwise generates a real monochrome source strike from
an unmodified OFL-licensed Nimbus Sans asset. Initialization publishes the default
SysFontSize ($BA8, 12 when unset) and ApFontID ($984, 3 when unset). SysFontFam
($BA6) is honored. The addresses are confirmed by MPW LowMem.h inline getters.
Repeated initialization retains live guest storage and does not allocate again.
FONT resources override the substitute. GetFontInfo now follows system/application
selectors and default point size through these guest globals; changes take effect
on subsequent calls. Fallback handles and initialization belong to the shared
Process, with no duplicated architecture font manager or cached host metrics.

`ab_glyph` 0.2.32 was already pinned in the dependency graph; it is now an explicit
source-font dependency. Its APIs were checked against primary documentation.
The generator converts screen points at 72 dpi to pixels per em, maps Mac Roman,
rounds logical advances and thresholds outline coverage at 0.5 into packed source
bits. CPU outline rasterization generates font source data; it does not modify
application pixel memory. Glyph drawing through wgpu remains outstanding. Sizes
1–96 and 32 cached guest fallback strikes are bounded. Families 0, 3 and 21 share
this initial Sans substitute. Other missing families, synthetic styles and
advanced font selection continue to fail explicitly.

The asset and unchanged OFL license/provenance are in future/assets/fonts. SHA-256
matches the repository's source asset. No Apple artwork was introduced. The
legacy font pipeline uses a different rasterizer and compatibility advances,
including Coppet for Geneva 9; this first substitute does not claim equal glyph
pixels or layout. Tests verify generated strikes at small/default/extended-offset
sizes, Mac Roman missing-glyph handling, real PPC/68K initialization and default
metrics, idempotence, guest-global changes and resource precedence/mutation. The
MPW drawing fixture initializes twice and queries system and resource metrics on
both architectures.

Original 68K progress, with the graphics option enabled: Super Mines completes
InitFonts, InitWindows and FlushEvents, then stops at unsupported InitMenus
($A930), PC $0100D012, trace index 13516. Centaurian completes InitFonts and
InitWindows, then stops at InitMenus, PC $10004466, index 103862. These are bounded
startup traces, not original application interaction acceptance. The trace example
now accepts an optional --graphics flag for this evidence. PPC Centaurian still
stops at strong import TEStyleNew before application execution.

Validation: the full suite passed 157 tests, then the added resource-precedence
check passed independently on the same production implementation, for 158 verified
tests (141 default, 17 optional GPU/MPW). MPW rebuild and strict all-target Clippy
pass. Next: shared text measurement and wgpu glyph drawing, then TextEdit, title
WDEFs and substantive menu initialization. Real Mixed Mode and full rebuild
acceptance remain outstanding; the goal stays active.


### Shared text measurement and wgpu glyph drawing

CharWidth/StringWidth, DrawChar/DrawString and MoveTo now share the selected guest
FONT or fallback strike across 68K traps and PPC imports. Measurement returns a
signed INTEGER through the correct ABI and leaves pnLoc unchanged. MoveTo writes
both guest point words. Drawing reads live text mode/colors, snapshots source
string/font data, prepares nonoverlapping ink/background coverage on the CPU,
and applies source transfer operations through the existing wgpu path. Pen mode
and pattern are restored, while the horizontal pen advances by glyph advances.
Opaque copy covers the advance cell as well as glyph bearing/overhang, so spaces
with blank images erase their background area. All eight source-mode variants
are implemented, including inverted source, using the shared integer GPU transfer
semantics. CPU source decoding and coverage geometry never modify destination
pixels. Normal port/visible/clip regions and packed-bitmap row padding are retained.

Text preflight validates input, mode, font, port and coordinates and caps coverage
work at one million cells. Unsupported style, fractional SpaceExtra, color ports,
recording/custom procedures and out-of-word drawing coordinates fail explicitly.
GPU/bitmap failure restores port fields; a terminal failure after earlier glyphs
may retain those completed pixels. DrawText/StdText, kerning/shaping, scalable or
styled metrics, fractional spacing and broader language/script behavior remain
unfinished. Default glyph pixels/layout are still the documented licensed
substitute, not a native Macintosh oracle comparison.

Legacy trap/quickdraw.rs confirms Pascal char/string argument sizes, signed word
width returns and pen movement. A real-CPU test verifies long signed width results
across PPC and 68K, stack canaries and unchanged pen location. The GPU test checks
all eight source modes against independent bit expectations, clipping, row padding,
port restoration, invalid mode/coordinates and read-only bitmap rejection. The MPW
fixture draws a synthetic resource glyph with signed bearing, verifies pen advance
and unchanged pen mode, XORs the same glyph away, checks CharWidth/StringWidth,
and erases a blank fallback space opaquely on both architectures. Its final image
continues to verify the earlier shared pattern/color operations.

All 160 tests pass (142 default, 18 optional GPU/MPW), the MPW rebuild passes,
and strict all-target Clippy passes. Original stopping points are unchanged:
68K applications require InitMenus; PPC Centaurian requires TEStyleNew at strong
import validation. Next: TextEdit/window text integration and title WDEFs, then
menu initialization and original application interactions. Full rebuild acceptance
and real Mixed Mode remain outstanding; keep the goal active.


### Shared text-buffer ranges

DrawText ($A885) and TextWidth ($A886) now decode a pointer, signed first-byte
offset and signed byte count into the existing shared text input. PPC r3–r5 and
reverse Pascal count/offset/pointer slots reach the same measurement/glyph path.
The 68K width result is a reserved word after eight argument bytes, validated
before stack mutation; PPC returns the sign-extended INTEGER. Signed offsets use
checked guest address arithmetic and may select bytes before an interior pointer.
Nonpositive counts select empty text without source reads or font resolution.
Positive ranges are fully validated and snapshotted before measurement/drawing,
including input buffers that could otherwise be affected by drawing. Existing
coordinate/glyph work limits, style/spacing gaps and GPU terminal-fault policy
remain unchanged. StdText and TextEdit integration remain outstanding.

Legacy quickdraw.rs confirms the argument ordering and char-buffer traversal;
checked boundaries replace its unchecked wrapping pointer arithmetic. Tests
cover signed offsets, empty counts with invalid pointers, overflow and unmapped
ranges, reversed 68K arguments, stack canaries and read-only result rejection.
The MPW drawing fixture measures a substring and draws/XOR-cancels its synthetic
resource glyph from a byte range on both architectures. Original startup stops
remain InitMenus on 68K and TEStyleNew at PPC strong-import validation. Next:
shared TextEdit records, style storage and lifecycle using these buffer APIs;
menu/window integration and full rebuild acceptance remain unfinished.

Validation: all 161 tests pass (143 default and 18 optional GPU/MPW); the MPW
rebuild and strict all-target Clippy pass.


### Shared TextEdit construction and lifetime

TEInit, TENew, TEStyleNew, TEGetText, TEGetStyleHandle and TEDispose now share
one TextEdit manager in Process. Constructors read destination/view rectangle
pointers through both native adapters (MPW TextEdit.h), capture current basic-port
text attributes/metrics, and allocate a movable TEHandle plus empty text handle.
Styled records install the documented -1 sentinels and TEStyleHandle overlay,
with real style, line-height, null-style and null-scrap handles. One initial style
and run are initialized from the current port. Public fields and style data live
in guest memory; host state retains allocation ownership only. The initial
foreground color supports black/white and initial font style must currently be
plain because synthetic metrics remain unfinished.

TEInit allocates a real empty TextEdit scrap handle and publishes TEScrpHandle
($AB4) and the word TEScrpLength ($AB0), preserving the adjacent reserved word.
LowMem.h confirmed the word/long widths. Repeated initialization preserves the
allocation. Constructor allocation failure frees its partial owned graph and
returns nil with memFullErr. Disposal validates all current master pointers and
style/text links before freeing anything. Replacement with foreign auxiliary
handles fails explicitly until adoption APIs are implemented. Record relocation
through the shared Memory Manager is supported. GetText returns the actual guest
CharsHandle; GetStyleHandle uses the style record overlay. The 68K styled selector
4 is decoded explicitly; other selectors remain unsupported.

Evidence: legacy loader/ppc/textedit.rs provides record/table offsets and initial
run/null-style layout; MPW TextEdit.h verifies the compiled ABI and fields. Three
focused tests cover record contents, style graphs, reserved bytes, damaged-link
atomicity, relocated TEHandles, complete disposal/reuse, partial-allocation
rollback and one record created/query/disposed across real PPC/68K adapters.
A real MPW application inspects plain/styled records, style-table metrics and
sentinels, getters and disposal on both architectures. Creation does not claim
working editing: text changes, selection, layout, redraw, clipboard operations,
style assignment and callbacks remain unfinished. Default callback/dispatch
fields are zero pending the callback/Mixed Mode implementation.

Centaurian PPC now resolves TEStyleNew and stops at SndDisposeChannel during
strong-import validation; no original PPC code has executed. 68K startup stops
remain InitMenus. Next: text replacement, line layout and redraw through the
shared buffer/font/wgpu services, followed by selection/editing and menu/window
integration. Full rebuild acceptance and real Mixed Mode remain outstanding.

Validation: all 165 tests pass (146 default, 19 optional GPU/MPW); the MPW
rebuild and strict all-target Clippy pass.


### TextEdit replacement and line layout

TESetText and TECalText now share the live record/text/style graph across both
adapters. Replacement snapshots caller bytes (including aliases of hText), retains
CharsHandle identity, moves selection endpoints to the new end, and updates the
record length/line starts. Recalculation reads live text and captured record/style
attributes rather than substituting the current port's font. Plain records update
line metrics; single-style records update style metrics, the per-line/caret-height
table and terminal run marker. CR/LF pairs, whitespace word wrapping, hard wrapping
of long words, empty text and the crOnly no-wrap flag are supported. Long whitespace
runs are scanned linearly. Text is bounded at 32767 bytes; invalid ranges and larger
lengths fault explicitly rather than silently truncating data. Mixed-style runs,
custom layout callbacks and synthetic style metrics remain unsupported.

The heap now stages grouped handle payload/size updates against a cloned allocator
map. All master identities remain stable, locked blocks cannot move, and no live
payload/master/size metadata is published unless every resize succeeds. This costs
a copy of allocator metadata per update, not a copy of the heap's guest bytes. All
writes are validated before commitment. Text, record and line-table allocation
failures therefore preserve the prior logical state even after earlier staged
allocations. Original TextEdit ownership metadata continues to track the same
handles after relocation.

Legacy PPC TextEdit and wrap_classic_text provide the argument/layout reference;
transactional resize and checked text limits strengthen the existing sequential
update behavior. Tests exercise text-handle identity, input aliasing, word/CRLF
breaks, captured fonts, no-wrap mode, style/caret metrics, 32767-byte whitespace,
recalculation, real 68K replacement/PPC recalculation on one record, and allocation/
lock failures after other changes have been staged. The MPW fixture checks plain
and styled replacement, selection endpoints, line starts, recalculation after
changing the destination width, CRLF, empty replacement and disposal on both CPUs.
Its CRLF data uses explicit bytes 13/10 rather than compiler newline escapes.

All 167 tests passed in the full suite; the final expanded long-whitespace case
also passed independently with the same production code. MPW rebuild and strict
all-target Clippy pass. The count is 148 default and 19 optional GPU/MPW checks.
Redraw, caret/selection geometry, editing commands, mixed styles, clipboard and
callbacks are still unfinished. Next: TEUpdate drawing the laid-out lines through
shared font/wgpu services, then selection/editing and window/menu integration.
Original stops remain InitMenus on 68K and SndDisposeChannel at PPC strong-import
validation. Full rebuild acceptance and real Mixed Mode remain outstanding.


### TextEdit redraw through shared wgpu glyph services

TEUpdate ($A9D3 / InterfaceLib) now consumes live line starts, captured attributes
and plain or single-style line metrics to redraw inactive records. It snapshots
text/font source data for the whole update and draws via the same shared glyph
renderer used by DrawText. Left/forced-left, centered and right alignment are
supported. Single-style black/white colors are read from the guest style table;
color fonts/mixed style runs remain unsupported.

Redraw intersects the caller update rectangle, view rectangle, existing complex
clip, port rectangle, bitmap bounds and applicable screen visibility. It does
not erase the whole view: source transfer modes control background writes, as in
legacy TEUpdate. The legacy PPC and 68K paths provide the line/ABI reference but
currently ignore the caller's update rectangle; this implementation honors it.
A temporary owned clip is disposed afterward; current port, original clip handle
and bytes, font/color state and pen fields are restored on success or failure.
Visible glyph-cell work is bounded at one million cells, and bitmap/port bounds
are applied before walking offscreen lines. Unused click/selection hooks do not
block inactive redraw; active selection and custom dispatch fail explicitly until
those drawing paths/callbacks are implemented.

The GPU test verifies partial updates, complex clipping, multiple lines, saved
ports/clip data, right alignment, styled white text and read-only destination
failure restoration. The MPW drawing fixture creates/replaces a TextEdit record,
redraws a partial rectangle, checks exact pixels and preserved current port/pen,
then disposes it on both CPUs. All 168 tests pass (148 default, 20 optional
GPU/MPW), the MPW rebuild passes and strict all-target Clippy passes.

This establishes text redraw, not interactive editing acceptance. Caret and
selection geometry/drawing, activation, editing commands, mixed styles, callbacks,
clipboard and menu/window integration remain unfinished. Original application
stops are unchanged. Next: selection/caret state and editing through the same
layout/redraw machinery, then substantive menu initialization and original app
interaction. Full rebuild acceptance and real Mixed Mode remain outstanding.

### Inactive TextEdit selection endpoints

TESetSelect ($A9D1 / InterfaceLib) now writes the live selection endpoints through
one shared service. The 68K adapter consumes two long offsets and a TEHandle;
PPC reads r3/r4/r5 and preserves its void result. Both endpoints independently
clamp to the live text length, following legacy Systemless's PPC implementation.
Reversed endpoints are preserved as in that reference; this is legacy comparison
evidence, not a native System 7 compatibility claim. Plain and styled records
are supported. Active records fail explicitly before changing selection because
highlight/caret painting remains unfinished.

The MPW fixture calls TESetSelect on both kinds of record and executes on both
CPUs. Unit checks cover clamping, reversed ranges and unchanged active records
on failure. All 169 tests pass (149 default, 20 optional GPU/MPW), freshly rebuilt
MPW TextEdit fixtures pass and strict all-target Clippy passes. Editing, active
selection geometry/drawing and original-application acceptance remain unfinished;
continue with those executable slices. The full rebuild goal remains active.

### Inactive TextEdit deletion with GPU erasure

TEDelete ($A9D7 / InterfaceLib) now deletes a nonempty selection through the
shared manager. It clamps and normalizes live endpoints as the legacy
TextEditBuffer does, keeps the text handle identity, and publishes text, collapsed
selection, line starts and single-style tables in one grouped heap transaction.
A collapsed selection is a no-op. TextEdit scrap bytes and length are preserved.

Deletion clears the clipped view with the captured port's background pattern,
then redraws remaining text using shared wgpu glyph services. The update/view,
caller clip, drawable bounds and screen visibility rules are shared with
TEUpdate. Temporary pen/text/clip fields and current port are restored. This
full clipped view redraw deliberately removes stale glyphs after shortening
text. The legacy buffer is the storage/selection reference; erasure is verified
with explicit GPU pixel assertions rather than claiming native System 7 parity.

Tests cover plain and styled deletion, reversed ranges, stable handles, wrapping,
empty records, insertion-point collapse, preserved scrap, no-op selections,
callback rejection before mutation, patterned clipped erasure, row padding and
port restoration. A destination fault after layout publication leaves the edit
committed and restores port state; runtime GPU faults remain terminal. The MPW
drawing probe deletes its TextEdit glyph and checks cleared pixels, record/handle
lengths and preserved port fields on both CPUs. All 170 tests pass (149 default,
21 optional GPU/MPW), MPW fixtures rebuild and strict all-target Clippy passes.

Active records, mixed style runs, key/click handling, insertion, clipboard and
selection/caret geometry/drawing remain unfinished. Continue with activation and
selection rendering, then interactive editing and menu/window integration.
Original app blockers and real Mixed Mode remain outstanding. The rebuild goal
is still active; this is an executable editing milestone, not rebuild completion.

### Active TextEdit selection, caret and deterministic blinking

TEActivate/TEDeactivate ($A9D8/$A9D9) and TEIdle ($A9DA), plus their
InterfaceLib imports, now use the shared TextEdit manager and GPU renderer.
Activation updates the live active/caret phase/time fields and immediately
redraws the clipped view; deactivation removes overlays. TESetSelect now redraws
active records, while inactive selection remains usable without a GPU. TEDelete
also supports active records and transactionally resets the caret timestamp when
collapsing the selection. Custom dispatch, highlight and caret hooks explicitly
fail before these state changes.

Highlight rectangles derive from guest line starts, font advances, line metrics
and alignment. They include selected trailing spaces and extend selected CR/LF
segments to the view edge. Carets support wrapped lines, empty text, and the
empty line after a trailing newline. wgpu performs monochrome XOR overlays.
Active updates clear only the clipped update area with bkPat before redrawing
text/overlays, making repeated updates stable rather than toggling the highlight.
Port state and original clip ownership are restored on completion or failure.
Inactive TEUpdate retains its existing source-mode behavior.

TEIdle blinks only active insertion points after the live low-memory CaretTime
interval ($02F4), initialized to 32 ticks in the common application loader.
Interval zero toggles once per call. Blink phase/time are live guest fields;
wrapping tick subtraction is deterministic. TextEdit now uses the runtime's
canonical tick count including host-driven elapsed guest time, correcting the
previous use of retired instruction count alone. No host wall time is involved.

The legacy PPC implementation provides record/ABI/selection geometry references.
It defers activation drawing and trims selection at visible text; immediate
activation, newline extension and stable active redraw are independently tested
here rather than claimed as native System 7 parity. Selection rectangle/point
field synchronization, click/key handling, insertion, mixed style runs, clipboard,
custom callbacks and broader window/menu integration remain outstanding.

All 171 tests pass (149 default, 22 optional GPU/MPW), MPW fixtures rebuild and
strict all-target Clippy passes. The GPU test covers plain/styled activation,
repeated updates, active selection/deletion, caret phase/timing, wraparound,
wrapping/newline geometry, partial updates, no-GPU rejection and hook rejection.
The real PPC constructor test proves elapsed guest ticks reach caretTime. The
MPW drawing fixture exercises activation/deactivation, selection, active deletion
and two idle blink phases on both CPUs, checking exact pixels and retained port
fields. This is not original-application interaction acceptance. Continue with
TEKey/click interaction and substantive Menu Manager initialization to advance
the original-app stops. Real Mixed Mode and full rebuild acceptance remain
outstanding; the goal remains active.

### Shared TextEdit key editing from compiled event loops

TEKey ($A9DC / InterfaceLib) now edits through the shared manager and grouped
layout transaction. Ordinary MacRoman bytes replace the normalized live
selection or insert at its caret; bytes 8 and 127 delete the selection or
preceding character, matching legacy Systemless's TextEditBuffer. Left/right
bytes 28/29 move or collapse the selection without inserting control bytes.
Active changes reset caret timing using the canonical guest clock and redraw
with the existing clipped GPU background/glyph/overlay path. Text handle
identity and TextEdit scrap remain unchanged.

The 68K adapter reads the low byte of the word CharParameter after the TEHandle
and consumes six bytes. PPC reads the low byte of r3 and TEHandle in r4, with a
void result that preserves r3. Up/down bytes 30/31 explicitly fail until vertical
layout navigation is implemented; they are not silently inserted as text.

Tests cover plain/styled insertion, active/inactive pixels, reversed selection
replacement, backspace at an empty caret, MacRoman bytes, CR insertion and
left/right movement, retained handle identity/scrap/port state, the 32767-byte
limit, and a locked four-byte text block failing growth without changing record,
text or pixels. The allocator permits growth within its four-byte alignment;
the failure test deliberately crosses that boundary. ABI checks cover high
parameter bits, void results and truncated 68K frames before stack mutation.

The MPW drawing fixture posts a keyDown event, retrieves it with WaitNextEvent,
feeds it to TEKey, and checks exact active text/caret pixels, arrow movement,
selection replacement and backspace on both CPUs. It requires a GPU and is a
compiled synthetic application acceptance slice, not an original app interaction
claim or host keyboard acceptance. All 173 tests pass (150 default, 23 optional
GPU/MPW), MPW fixtures rebuild and strict all-target Clippy passes.

Next: substantive Menu Manager initialization/loading to advance the original
68K applications past InitMenus. TextEdit vertical navigation, click/drag,
selRect/selPoint synchronization, mixed styles, clipboard and callbacks remain
unfinished, as do host keyboard input and real Mixed Mode. The full rebuild
acceptance remains unproven and the goal stays active.

### Shared Menu Manager initialization and empty-bar lifecycle

InitMenus ($A930 / InterfaceLib) now owns stable guest DynamicMenuList and
MenuCInfo handles in the shared process. It resets the current list/selection,
sets the standard 20-pixel menu height, and merges bounded, checked 30-byte
MCEntry records from optional mctb 0. Resource Manager handles remain canonical: an
already loaded mctb modified by the guest is read on reinitialization. Color table
identity and unrelated entries persist. Missing color resources preserve ResErr;
resource lookup and allocation results publish their shared error globals.

Initialization stages heap updates before publishing MenuList ($A1C), MenuCInfo
($D50), MBarHeight ($BAA) and TheMenu ($A26). Partial new-handle allocation rolls
back. If Window Manager's GrayRgn exists, its desktop geometry is intersected
with the area below the menu bar in the same heap transaction. Reserving the bar
for existing windows when the height changes remains an explicit unsupported
path rather than leaving their visibility inconsistent.

The empty bar draws through wgpu directly into the guest screen bitmap, honoring
live black/white RGB4 background information, its bottom border and the legacy
standard corner mask. Application port/clip/pen fields remain untouched.
DrawMenuBar ($A937), GetMenuBar ($A93B) and ClearMenuBar ($A934) share this state
through both adapters. Snapshots copy validated regular/hierarchical list
records into caller-owned handles; referenced menus are not disposed. Clearing
removes list references and TheMenu without drawing. Populated menu drawing,
custom definitions and nonmonochrome backgrounds explicitly fail. Menu creation,
MENU/MBAR resource expansion, title/item rendering, tracking and callbacks are
not implemented yet. GPU faults after state publication remain terminal.

All 176 tests pass (151 default, 25 optional GPU/MPW), MPW fixtures rebuild and
strict all-target Clippy passes. Checks cover cross-CPU shared state, caller-owned
snapshots, canonical loaded-resource edits, malformed color payload rejection,
color identity/retention, pixel-exact borders/corners, preserved desktop pixels,
port state, GrayRgn clipping, allocation rollback and ABI result validation. The
compiled fixture initializes windows/menus, loads mctb, snapshots/clears/reinitializes
the bar, and checks exact pixels and preserved application port fields on both
CPUs. Its initial below-bar assertion was corrected to preserve InitWindows'
existing gray desktop, rather than assuming untouched pixels were zero.

Fresh original 68K traces pass InitMenus. Super Mines now stops at InitCursor
($A850), PC $0100D014, trace index 13517. Centaurian passes InitMenus and TEInit
then stops at InitCursor, PC $1000446A, index 103864. The original PPC Centaurian
load still rejects strong InterfaceLib:SndDisposeChannel before executing its
instructions. These are startup observations, not application interaction
acceptance. Next: shared cursor initialization/state and GPU/host presentation,
then further menu construction/resource expansion and application startup. Real
Mixed Mode, controls, interactive originals and full rebuild acceptance remain
outstanding. The goal remains active.

### Shared monochrome cursor state and GPU presentation

InitCursor/SetCursor/HideCursor/ShowCursor/ObscureCursor ($A850/$A851/$A852/
$A853/$A856) and their InterfaceLib imports now use one process-owned cursor
value. InitCursor copies the live QuickDraw arrow prototype, resets hide depth
and obscuring, and makes the cursor eligible for presentation. SetCursor copies
a checked 68-byte data/mask/hotspot record by value; nil/truncated records fail
without replacing the previous image. It preserves hide depth. Hide/show nest
with a bounded signed level; excess show calls do not create positive depth.
Obscuring clears only when the shared guest pointer position changes. GetCursor
($A9B9 / InterfaceLib) reads CURS through the common Resource Manager, preserving
resource handle identity, ordinary miss/error behavior and both native ABIs.

The cursor is a presentation overlay: WGSL applies (index AND NOT mask) XOR data
for its 16x16 footprint, including transparent, white, black and inverse pixels.
It leaves the indexed storage and packed guest screen unchanged. Presentation
accepts read-only guest pixels, restores the underlying image automatically on
motion/hide, clips signed hotspots at display edges, and rejects nonmonochrome
indices or drawing-command misuse. No CPU destination compositor is added.
Runtime refresh observes shared input and composes the cursor through the same
GPU texture used by the window frontend. The host hides its native pointer while
an initialized guest owns the canvas; leaving, losing focus, resizing or ending
guest execution clears that ownership until eligible motion. Headless display
refresh can still compose the final guest cursor after process exit.

The legacy cursor ABI/value/hide-depth behavior is reference evidence. The
legacy ObscureCursor compatibility route is a no-op; this implementation supports
actual position-change obscuring. This is not a native System 7 parity claim.
Color cursors, shielding, animated cursor managers, cursor-related low-memory
globals and full mouse-driver compatibility remain outstanding.

All 178 tests pass (152 default, 26 optional GPU/MPW), MPW fixtures rebuild and
strict all-target Clippy passes. A real shared-world CPU test interleaves PPC and
68K cursor calls, verifies copied data, depth, reset, obscuring and bad-source
rollback. GPU assertions cover all mask/data cases, clipping, motion/hide and
read-only guest memory. The compiled menu fixture now loads a CURS resource,
sets it while hidden, balances show calls and verifies actual RGBA composition
on both CPUs while the original screen byte remains F8.

Window smoke reached the compiled guest's successful exit (1621 instructions),
but failed to complete acquire/present/resize within 600 paced attempts. Actual
onscreen cursor appearance and pointer hiding remain unverified on this host.
The scoped smoke process exited; it is not a passing window test.

Fresh original 68K startup traces pass InitCursor. Super Mines then passes
TEInit and stops at InitDialogs ($A97B), PC $0100D01A, index 13520. Centaurian
stops at InitDialogs, PC $10004470, index 103866. Original application interaction
and original PPC execution remain unproven; the PPC loader's SndDisposeChannel
strong-import gap is unchanged. Next: substantive Dialog Manager initialization
and resource/window/control integration, then menu construction and further
original startup. Real Mixed Mode and full rebuild acceptance remain incomplete.
The rebuild goal stays active.

### Shared Dialog Manager startup and parameter StringHandles

InitDialogs ($A97B / InterfaceLib) now publishes ResumeProc ($A8C), resets
AlertStage ($A9A) and DlgFont ($AFA), and clears DAStrings ($AA0..AAF) while
releasing its owned parameter handles. ANumber ($A98) stays unchanged. Resume
and ErrorSound procedure values are stored for future callback routes, not
invoked during initialization. DABeeper ($A9C) resets to NIL, the explicit legacy
Systemless no-sound boundary; default alert sound/menu flashing are still absent.
No fake beeper or resume callback shim is installed.

ParamText ($A98B / InterfaceLib) snapshots non-NIL Pascal inputs before resizing
and publishes their payloads through one grouped heap transaction. NIL arguments
preserve existing slots; non-NIL empty strings replace their value. DAStrings
contain checked, live Pascal StringHandles, with stable existing identities.
GetParamText reads those same guest payloads and validates all non-NIL outputs
before writing, including aliasing and read-only failures. SetDialogFont/SetDAFont
and ErrorSound ($A98C) write the canonical font/sound globals. Replaced slot
pointers or malformed owned strings explicitly fail rather than being hidden
by a host text cache. Startup validates its complete write set before mutation.

The SDK's PPC CarbonAccessors.o implements GetParamText using the indexed
InterfaceLib LMGetDAStrings(short) accessor and BlockMoveData. The shared indexed
accessor returns the actual guest StringHandle, as declared by LowMem.h; it is
not the legacy helper's no-argument array-address interpretation. BlockMove and
BlockMoveData now share checked, overlap-safe guest-memory copying through both
ABIs. Nonpositive counts are no-ops; positive copies are capped at the 64 MiB
image execution limit and validate source/destination before mutation. There is
no separate simulated instruction cache; copying into writable executable
storage is visible to the next guest instruction. Pixel-memory copies remain
raw guest writes, synchronized by the existing bitmap drawing/presentation
boundaries. They are not a CPU graphics renderer.

All 183 tests pass (156 default, 27 optional GPU/MPW), MPW fixtures rebuild and
strict all-target Clippy passes. Checks cover shared cross-CPU globals/strings,
MacRoman bytes, source/output aliasing, stable handles, failed locked growth,
partial allocation cleanup, whole-heap reclamation, read-only output/startup
atomicity, overlap in both directions, bounded counts and execution of copied
code. The compiled fixture stores/reads dialog globals, replaces parameters,
checks empty/reset values and posts its success marker on both CPUs. PPC links
and executes Apple's CarbonAccessors.o; the bundled object is PPC-only, so the
68K fixture uses a guest loop over documented DAStrings. The shared PPC
GetParamText service is independently exercised by a real CPU test. The fixture
uses the SDK's SoundUPP type for ErrorSound; callback invocation is not proven.

Fresh original 68K startup traces pass InitDialogs. Super Mines now stops at
SysEnvirons ($A090), PC $0100C484, index 13776. Centaurian stops at MaxApplZone
($A063), PC $10004472, index 103867. These are startup observations, not original
application interactions. Dialog/alert construction and rendering, controls,
modal dispatch, substitution rendering and callback/sound execution remain
unfinished. Next: truthful system-environment and application-heap startup
contracts to advance those stops, then further resource/menu/dialog application
slices. Original PPC strong sound imports and real Mixed Mode remain outstanding;
the full rebuild goal stays active.

### Shared system environment and maximized heap startup

SysEnvirons ($A090 / InterfaceLib) now reports one 16-byte virtual Macintosh
record independent of caller ISA: version 2, unknown virtual machine/keyboard,
System 7 compatibility target, and the 68020 compatibility processor. FPU and
Color QuickDraw are false because those 68K hardware/public Color QuickDraw
capabilities are not implemented. Internal GPU palettes do not imply public
Color QuickDraw support. AppleTalk and installed-System-file references are zero.
This intentionally differs from the legacy reference's hardcoded machine/FPU/
Color QuickDraw claims. It does not claim complete System 7 compatibility or
physical PowerPC hardware identity.

Versions 1/2 receive the available version-2 record; a newer request receives
the same record with envVersTooBig (-5502). Nonpositive versions return
envBadVers (-5501) without modifying output; NIL records return paramErr.
Record writes use checked guest memory and preserve neighboring bytes. Version
and layout contracts follow Apple's [Gestalt Manager reference](https://dev.os9.ca/techpubs/mac/pdf/Operating_System_Utilities/Gestalt_Manager.pdf)
and [technical note OV16](https://leopard-adc.pepas.com/technotes/ov/ov_16.html),
with MPW OSUtils.h/MacErrors.h providing primary declarations/constants.

The common CODE/PEF loader now reserves the entire fixed 4 MiB application heap
before guest execution, publishing HeapEnd ($114) and ApplLimit ($130) as
$10400000. Both reflect the actual mapped allocator extent rather than zero or
an unbacked projection. MaxApplZone ($A063 / InterfaceLib) validates the fixed
application limit and restores HeapEnd to that full extent, preserving all
existing handles/pointers and free space. GetApplLimit reads the same validated
bound. Heap limit changes/zone restructuring are still unsupported; foreign
limit values fail before mutation rather than pretending to grow the allocator.

All 187 tests pass (159 default, 28 optional GPU/MPW), MPW fixtures rebuild and
strict all-target Clippy passes. Checks cover cross-CPU identical environment
records, version errors, read-only output, full-heap allocations, preserved
allocated bytes, repeatable maximize calls and inconsistent-limit rejection.
The compiled fixture calls SysEnvirons for supported/bad/newer versions, reads
low-memory heap limits, maximizes, queries GetApplLimit and uses a live pointer
across another maximize on both CPUs. Existing loader, CFM, resource, graphics
and application fixtures still pass with eager heap mapping.

Fresh original startup traces now pass the preceding stops. Super Mines reaches
EventAvail ($A971), PC $0100C458, index 13824. Centaurian passes MaxApplZone and
reaches SetRect ($A8A7), PC $0100061C, index 104601. No original application
interaction acceptance is claimed. Next: shared rectangle mutation and Toolbox
event availability with proper window/event integration, then further application
resource/menu/dialog slices. Native windows/cursor presentation, original PPC
strong sound imports and real Mixed Mode remain unfinished. The full rebuild
goal stays active.

### Shared rectangles and window update delivery

SetRect, OffsetRect and InsetRect now use one checked signed 16-bit rectangle
implementation for native 68K traps and PPC imports. Set preserves edge order;
offset/inset wrap coordinates, preserve neighboring bytes, and reject read-only
output before mutation. Inset preserves inverted edges, following the legacy
Systemless comparison; native System 7 behavior has not been verified.

EventAvail and GetNextEvent now peek/consume eligible shared OS events and
synthesize update events from visible windows' live guest update regions.
WaitNextEvent uses the same availability/delivery path, so pending updates wake
it immediately. OS-only calls still exclude window updates. Update delivery
leaves the region pending until BeginUpdate; BeginUpdate intersects the local
visible region with global dirty geometry, saves visibility in an owned guest
handle, clears the dirty region and selects the window port. EndUpdate restores
visibility and preserves newly accumulated invalidation. EqualRgn compares
decoded geometry, including differently encoded equivalent regions.

All 190 tests pass (162 default, 28 optional GPU/MPW); strict all-target Clippy
passes and MPW fixtures rebuild. The compiled window fixture checks both CPU
ABIs, repeatable update delivery, OS/Toolbox separation, immediate WaitNextEvent,
clipped visibility, restored visibility and invalidation retained across the
update bracket. Default tests cover rectangle boundary faults/wrapping and
shared CPU event peek/consume with native Boolean stack padding.

Fresh original traces advance to HFSDispatch ($A260), selector 9 in Super Mines
(PC $010124BC, index 14265), and GetGDevice ($AA32) in Centaurian's 68K code
(PC $10007426, index 104863). These are bounded startup traces, not accepted
original application interactions. Activation events, menu invalidation, nested
update brackets, window lifecycle changes during updates, public Color QuickDraw,
original PPC sound imports and real Mixed Mode remain unfinished. Native
onscreen presentation also remains unverified. Next: application-driven file
metadata/device contracts and subsequent menu/dialog slices. Full rebuild goal
remains active.

### Shared application catalog queries

PBGetCatInfoSync and native HFSDispatch ($A260), selector 9 now query the
existing shared read-only application file and virtual root. Root ID 2, parent
ID 1, application ID 16 and volume -1 agree with the FCB projection. Named
application queries, indexed root enumeration and negative directory-ID queries
return names, parent identities, fork sizes/allocation blocks, file locking and
open-fork flags. Nonnegative calls ignore input ioDirID as the documented
contract requires. Synchronous completion is cleared and ioResult matches the
ABI error return. Output faults leave parameter/name destinations unchanged;
lookup errors update only completion/result. Unknown volume/directory and
missing entries have distinct errors.

The contract/layout follows MPW Files.h and Apple's [File Manager reference](https://leopard-adc.pepas.com/documentation/mac/pdf/Files/File_Manager.pdf),
pp. 2-100–2-104 and 2-190–2-192. Dates and Finder metadata currently default to
zero; general hierarchy/path parsing, MacRoman case folding, writable catalogs
and asynchronous completion remain gaps. No installed System or host directory
is exposed by this service. Legacy catalog code served as comparison material;
native System 7 testing is optional under the user's permission and was not run.

Validation: 192 tests (164 default, 28 optional GPU/MPW), rebuilt MPW fixtures
and strict all-target Clippy. New tests check native CPU ABI metadata, loaded
fork lengths/flags, ignored stale file ID, named queries, directory/error
contracts and output atomicity. Compiled 68K/PPC guests query root/application
and end-of-enumeration, including PPC without a resource fork.

Super Mines now completes its PBGetCatInfo call, receives fnfErr for the absent
entry and reaches PBDirCreate (HFSDispatch selector 6), PC $0101247A, trace
index 14285. This advances startup without claiming original interaction
acceptance. Centaurian 68K remains at GetGDevice; original PPC strong sound
imports, native presentation and real Mixed Mode remain outstanding. Next:
expand the coherent virtual file catalog with application-driven directory/file
operations, and implement device contracts needed by the graphics slice.
The full rebuild goal stays active.

### Process-local virtual directory creation

PBDirCreateSync and native HFSDispatch selector 6 now create real directory
nodes in the shared File Manager catalog. Returned IDs are stable and distinct
from the application; parent identity, name, child count and dates can be read
through PBGetCatInfo. Relative/absolute colon paths and parent traversal resolve
against that same tree. Named lookup and root enumeration see created nodes,
duplicate siblings are rejected, and invalid parent/volume/name errors remain
distinct. Guest output faults preserve both catalog contents and ID allocation.
The tree is bounded to 1024 created directories with dirFulErr at capacity.

Layout and creation contracts follow MPW Files.h and Apple's [File Manager reference](https://leopard-adc.pepas.com/documentation/mac/pdf/Files/File_Manager.pdf),
pp. 2-188. Directory dates use the guest low-memory Time value ($20C); it starts
at zero and an advancing calendar clock remains unfinished. Parent modification
dates, working-directory references, MacRoman folding, file creation/write/delete
and persistence remain gaps. This is process-local storage; the loaded
application's forks remain read-only and no host file changes are made.

Validation: 194 tests (166 default, 28 optional GPU/MPW), MPW rebuild and strict
all-target Clippy. The generated CPU fixtures create/query the same directory
state and check date projection. Tests cover nested/absolute/relative/parent
paths, root enumeration, child counts, duplicate/error replies and atomic output
faults. Compiled 68K/PPC guests create a parent and child, query the tree and
reject a duplicate.

A fresh bounded Super Mines trace creates its missing directory and advances to
PBHGetFInfo ($A20C), PC $0101252A, trace index 14329. Original application
interaction acceptance is still absent. Centaurian's 68K GetGDevice stop and PPC
strong sound imports remain outstanding, along with native presentation and real
Mixed Mode. Next: shared file metadata/access and mutable fork operations, then
further application-driven menu/dialog/graphics slices. Full rebuild goal stays
active.

### Shared file-information queries

PBGetFInfoSync and PBHGetFInfoSync/native $A00C and $A20C now use one File
Manager service and the same application fork metadata projection as catalog
queries. The 80-byte parameter block returns first open reference, file ID,
locked/open-fork attributes, logical/physical fork lengths and allocation
blocks. Hierarchical indexed queries honor the parent directory; flat calls
select the root. Named missing files and invalid volume/parent errors are
reported via ioResult and the CPU ABI. Synchronous completion is cleared.

Named queries preserve the caller pathname when the file is closed, accepting
a read-only input string. Indexed/open-file queries return the leaf name.
Both guest destinations are checked before mutation, and bytes after the
80-byte record remain untouched. Contracts follow MPW Files.h and Apple's
[File Manager reference](https://leopard-adc.pepas.com/documentation/mac/pdf/Files/File_Manager.pdf),
pp. 2-194–2-195. Finder metadata and dates remain zero defaults; additional
virtual files, mutable forks and asynchronous requests remain unfinished.

All 196 tests pass (168 default, 28 optional GPU/MPW), strict all-target Clippy
passes and MPW fixtures rebuild. Focused reruns also pass with the rebuilt
compiled fixture and closed-file pathname assertion. Generated real-CPU guests
exercise both flat/hierarchical imports/traps with exact fork values; boundary
tests cover error outputs, directory separation, atomic read-only destination
failure and unchanged neighboring bytes. Both compiled CPU guests query
application metadata and missing files in a created directory.

Super Mines now reaches resource-defined GetNewWindow ($A9BD), PC $01010D04,
trace index 14410. This is bounded startup evidence, not original application
interaction acceptance. Centaurian 68K still needs GetGDevice; original PPC
sound imports, native presentation and real Mixed Mode remain unfinished. Next:
load WIND resource definitions into real shared window creation/graphics, then
continue application-driven file, menu/dialog and graphics slices. Full rebuild
goal remains active.

### WIND loading and modal/document window definitions

GetNewWindow ($A9BD / InterfaceLib) now reads the live loaded WIND handle and
uses shared NewWindow creation, including caller-supplied WindowRecord storage.
Checked templates provide bounds, procID, visible/goAway flags, refCon and
Pascal title; missing/truncated resources return NIL. Optional zero positioning
is accepted; nonzero positioning and unsupported/custom definitions fault
explicitly. This follows Apple's [GetNewWindow contract](https://dev.os9.ca/techpubs/mac/Toolbox/Toolbox-226.html)
and [WIND layout](https://dev.os9.ca/techpubs/mac/Toolbox/Toolbox-293.html).

The common creator now supports dBoxProc (1) modal double-border geometry and
noGrowDocProc (4) document title/close/stripe/shadow chrome, alongside plainDBox
(2). All guest pixel writes use wgpu-backed drawing services. Rectangle ink is
unioned before inversion-point encoding so overlapping strokes do not cancel.
Titles use the shared font/text service and the existing licensed fallback when
a native font resource is absent. Legacy Systemless geometry provided comparison
evidence; native System 7 pixel parity and Chicago font parity are not claimed.
Chrome observes live guest title/highlight/goAway state, restores manager port/
clip state, clips against higher windows, and refreshes when front-window changes
require active/inactive rendering. Immutable built-in definition identity remains
registration metadata; no executable WDEF or AuxWin support is implied.

All 197 tests pass (168 default, 29 optional GPU/MPW), strict all-target Clippy
passes and MPW fixtures rebuild. New GPU tests compare full screen bytes across
CPU adapters; check native result padding, region geometry, title/close/shadow
pixels, modal border gaps, active/inactive chrome, live WIND edits and caller
storage ownership. Compiled 68K/PPC guests read the WIND template, check record
fields and dispose the created document window. Existing update/clip/lifetime
fixtures continue to pass.

Fresh Super Mines startup now creates its first hidden modal WIND window and
reaches DetachResource ($A992), PC $0100D9F8, index 14460. It has not completed
interactive acceptance. Resource positioning, activation events, window mouse
tracking, remaining standard/custom WDEFs, AuxWin records, original PPC sound
imports and real Mixed Mode remain unfinished; native presentation is still
unverified. Centaurian 68K remains at GetGDevice. Next: resource detachment and
application-driven initialization/lifecycle, then further dialog/menu/graphics
slices. The full rebuild goal stays active.

### Resource detachment and application handle ownership

DetachResource ($A992 / InterfaceLib) now removes the resource-cache association
while retaining the same owned master pointer, payload address/data, size and
lock state. Later GetResource calls allocate a separate handle and reload the
original fork bytes. Detached handles can be resized/unlocked/disposed normally;
ReleaseResource and repeated DetachResource reject them with resNotFound.
Detach needs no allocation even when the application heap is full, and preserves
MemErr. Corrupted guest master pointers fault before identity changes.

Resources marked resChanged remain attached and report resAttrErr (-198),
following the System 7-era [Resource Manager contract](https://vintageapple.org/inside_r/pdf/More_Macintosh_TB_1993.pdf)
(p. 1-108) and Apple's [Resource Manager reference](https://leopard-adc.pepas.com/documentation/Carbon/Reference/Resource_Manager/Reference/reference.html#//apple_ref/c/func/DetachResource).
This differs from legacy Systemless's zero-error changed-resource handling.
Its clone-on-detach approach is also unnecessary here: rebuild resource handles
already own independent allocations. Resource-file close, mutable maps,
AddResource, dynamic attribute changes and purge-state APIs remain unfinished.

All 201 tests pass (172 default, 29 optional GPU/MPW), strict all-target Clippy
passes and MPW fixtures rebuild. New checks cover retained locks/addresses/data,
independent reload/disposal, changed/invalid/repeated detach, corruption rejection,
full-heap detachment, both native CPU ABIs, void-result preservation and low-memory
ResErr. Compiled 68K/PPC probes mutate/detach/reload and dispose independent
handles through real startup/toolbox code.

Super Mines advances through detached resource processing and reaches NewControl
($A954), PC $0100DBD8, trace index 14973. Its original UI interaction acceptance
is still absent. Centaurian 68K remains at GetGDevice, original PPC at strong
sound imports, with native presentation and real Mixed Mode unfinished. Next:
shared control records, CDEF drawing and dialog/window control lifecycle, then
continue original application-driven slices. The full rebuild goal stays active.

### Shared button-family control records, drawing and disposal

NewControl ($A954 / InterfaceLib) now creates real owned 296-byte ControlRecords
and prepends their handles to the owner's live controlList. Push buttons,
checkboxes and radio buttons share one service for record access, title/value/
reference changes, visibility/highlighting, drawing, basic hit testing and
disposal. Values clamp to the live min/max range; getters read guest fields.
Native result writes preserve neighboring stack bytes, and PPC's ninth refCon
argument is checked at r1+56. PPC import decoding is now fallible and leaves the
return PC untouched on argument faults. Contracts follow Apple's [ControlRecord](https://dev.os9.ca/techpubs/mac/Toolbox/Toolbox-317.html),
[NewControl](https://dev.os9.ca/techpubs/mac/Toolbox/Toolbox-324.html) and
[SetControlValue](https://dev.os9.ca/techpubs/mac/Toolbox/Toolbox-335.html) references.

All pixel changes use GPU QuickDraw; geometry follows existing Systemless
button-family comparisons and titles use the shared font strike path. Multi-line
titles, initial title truncation, pressed ink, inactive dithering, and owner
visibility/clip state are included. Owner port, clip and current-port state are
restored after drawing. Hide/dispose erases visible pixels using the owner's
background pattern/colors. Native font/CDEF pixel parity is not claimed.

Window close/dispose frees owned controls after validating the entire linked
list, including cycle/foreign-handle/orphan checks. DisposeControl unlinks and
frees one control; KillControls handles the whole live list. This follows
Apple's [removal contract](https://dev.os9.ca/techpubs/mac/Toolbox/Toolbox-356.html).
Built-in definition identity remains registration metadata and contrlDefProc is
currently NIL; callable CDEF handles and AuxCtlRec are not implemented. Custom
definitions/data fault explicitly. Tracking/action callbacks, scroll bars, CNTL
loading, move/size and limit setters remain unfinished.

All 203 tests pass (173 default, 30 optional GPU/MPW), strict all-target Clippy
passes and MPW fixtures rebuild. Focused ABI/compiled reruns verify final result
slot decoding. New tests cover ninth PPC stack arguments/faults, write-only and
read-only native result slots, full-screen cross-CPU control parity, checkbox/
radio pixels, guest value changes, owner port restoration, visibility/hit state,
corrupt control-list rejection and automatic window-owned cleanup. Both compiled
CPU guests check list order, record fields, references, titles, clamped values,
highlight/visibility/hit behavior, redraws and disposal.

Super Mines now creates controls and reaches SizeWindow ($A91D), PC $01010D42,
trace index 15298. Original application UI interaction is still unverified.
Centaurian 68K still needs GetGDevice; original PPC sound imports, native
presentation and real Mixed Mode remain outstanding. Next: shared window sizing/
visibility and resulting region/update/control redraw behavior, then continue
application-driven UI slices. Full rebuild goal stays active.

### Window sizing, ShowHide exposure and live reference constants

SizeWindow ($A91D / InterfaceLib) now changes the shared window's local port
dimensions and global content/structure regions, preserving its origin, caller
clip and retained visible guest pixels. Newly created content is added to its
update region only when fUpdate is true, following Apple's [SizeWindow contract](https://dev.os9.ca/techpubs/mac/Toolbox/Toolbox-251.html).
Existing invalidation is clipped to the new content. Other windows uncovered by
shrink receive independent exposure invalidation. All affected region handles
are staged together and output fields validated before mutation; corrupt aliases
fail without partially changing port dimensions. GPU drawing repairs frame and
exposure damage while leaving retained content pixels intact.

ShowHide ($A908 / InterfaceLib) shares the exposure primitive: hiding/revealing
changes live visibility and update regions while preserving list order and
highlighting, as [documented by Apple](https://dev.os9.ca/techpubs/mac/Toolbox/Toolbox-237.html).
It does not implement ShowWindow/HideWindow activation semantics. Controls remain
owned and application-driven DrawControls uses the new visibility. GetWRefCon
($A917) and SetWRefCon ($A918) read/write live refCon memory with checked ABI
outputs; these do not require graphics or change MemErr.

All 205 tests pass (174 default, 31 optional GPU/MPW), strict all-target Clippy
passes and MPW fixtures rebuild. New checks cover exact 68K Boolean/word
marshalling and result faults, cross-CPU full-screen resize parity, direct guest
pixel retention, fUpdate false/true behavior, caller clip retention, background
exposure, ShowHide order/highlight preservation, control survival/redraw, live
references and atomic alias rejection. Paired compiled guests resize/reveal and
redraw their real control/window records.

Zero/zero size requests are no-ops; other nonpositive dimensions and geometry
changes during BeginUpdate remain explicit faults. Position/dragging,
ShowWindow/HideWindow/SelectWindow activation, custom definitions and native
presentation remain unfinished. Native System 7 parity was not tested. Fresh
Super Mines startup now reaches MoveWindow ($A91B), PC $0100B972, index 15431,
after SizeWindow and SetWRefCon. Original UI interaction, Centaurian 68K's
GetGDevice, original PPC sound imports and real Mixed Mode remain outstanding.
Next: window movement and activation/visibility lifecycle with correct pixel
transfer and exposure. Full rebuild goal stays active.

### Window movement, activation and invalidation

MoveWindow now preserves local coordinates and transfers retained visible pixels
through wgpu from an immutable guest bitmap snapshot, including overlapping
moves. Exposure damage enters shared update regions. SelectWindow/fronting moves,
ShowWindow and HideWindow share checked window state. Activation events use the
live CurDeactive/CurActivate slots; peeking preserves them, consumption clears
them, and OS-only retrieval excludes them. Hidden creation preserves activation
and close removes dangling pending pointers. InvalRect/ValidRect operate on the
current window's content and remain available during BeginUpdate.

Generated cross-CPU tests cover full-screen movement parity, overlap, retained
pixels, exposure, selection, visibility, activation ordering/peek/consume, output
faults and invalidation. Real MPW guests exercise MoveWindow, HideWindow,
ShowWindow, SelectWindow and activation retrieval. Strict all-target Clippy passes.
Comparison authorization permits legacy Systemless alone; native System 7 parity
is not claimed. Super Mines reaches unsupported SystemTask ($A9B4), PC $01010D8E,
trace index 15543 after movement, display and invalidation. Next: implement its
periodic service contract, then continue original application interaction.
Centaurian GetGDevice, PPC sound imports, production Mixed Mode and native
presentation remain open. The full rebuild goal remains active.

Full validation: 207 tests pass (175 default, 32 optional GPU/MPW), including
the rebuilt paired compiled window fixtures. No GPU tests were skipped.

### Empty periodic service and application-driven QuickDraw state

SystemTask now has checked 68K/PPC entry points into one shared runtime service.
With no supported desk-accessory/periodic driver registrations it preserves
guest state, event queues and registers, following the legacy Systemless
comparison. This does not implement driver callbacks or add a clock step.
PenSize now updates the live GrafPort words consumed by existing frame drawing.
RectInRgn tests actual decoded region intersections, preserving holes and
half-open edges. Read-only native Boolean outputs fail before the ABI pops args.

Generated guests verify general-purpose register and low-memory preservation
for SystemTask,
vertical/horizontal pen ordering and word truncation, rectangular queries within
complex-region holes, empty/inverted rectangles, edge contact and native result
padding/permissions. Rebuilt MPW 68K/PPC windows exercise all three services.
Fresh Super Mines startup reaches GetPicture ($A9BC), PC $01000AE2, trace index
17789. Next: connect picture resource loading to the shared Resource Manager,
then implement picture drawing through wgpu with bounded fixtures. Original
application interaction, PPC sound, Color QuickDraw, production Mixed Mode and
native presentation remain outstanding. Full rebuild goal remains active.

Validation: 210 tests pass (178 default, 32 optional GPU/MPW), strict all-target
Clippy passes, and MPW fixtures rebuild. No GPU tests were skipped.

### Picture resources and offscreen port setup

GetPicture delegates to the shared PICT resource lookup, preserving identity and
release/reload semantics across architectures. PortSize and MovePortTo implement
the ordinary GrafPort setup that Super Mines performs before DrawPicture.
PortSize preserves the local origin and regions; MovePortTo preserves bitmap
dimensions and changes only its global mapping. These APIs do not recompose
window geometry or provide color GrafPort support.

New generated tests cover missing resources/ResErr, negative IDs, shared
GetResource/GetPicture identity, modified bytes, release/reload, native result
permissions/padding, port dimensions/mapping and both CPU conventions. An
offscreen GPU check verifies exact pixels after resizing/repositioning the port
and confirms visibility storage is unchanged. MPW window guests now load a PICT
resource and inspect the setup fields through the real headers.

Super Mines loads PICT 202 and reaches DrawPicture ($A8F6), PC $01000BB2, index
17979. Its picture is v1 PackBitsRect/srcOr. Next: a bounded picture decoder with
GPU bitmap scaling/transfer and clip composition, checked with generated streams
and original resource evidence. Unknown operations must remain explicit errors.
Original UI interaction, production Mixed Mode and native presentation are still
unfinished. Full rebuild goal remains active.

Validation: all 213 tests pass (180 default, 33 optional GPU/MPW), MPW fixtures
rebuild and strict all-target Clippy passes. The strengthened port parity check
also passes in a focused rerun; production code was unchanged throughout.

### PICT bitmap decoding and GPU picture drawing

DrawPicture now serves both CPU adapters through one bounded parser and shared
QuickDraw target. PICT v1/v2 monochrome bitmap commands support raw/PackBits
rows, normalized source rectangles, picture clips, bitmap masks, comments/header
metadata and all eight source transfer modes. The GPU scales immutable decoded
indices and applies clipping/colors/transfers. CPU work only decodes storage;
packed guest pixels and padding synchronize through the existing BitMap boundary.

Focused generated tests compare raw/packed v1/v2 commands, malformed/truncated
streams, metadata alignment, clip/mask intersection and bounded input. Real
68K/PPC calls draw equivalent scaled pixels while preserving port fields and
caller stack padding. GPU mode tests intersect mapped picture and caller clips,
mix direct guest writes and reject a malformed trailing opcode before any
drawing. MPW windows now draw a generated PICT and inspect guest bitmap bits.

Super Mines PICT 202 completes DrawPicture; startup reaches CopyBits ($A8EC),
PC $01009634, trace index 18066. Next: shared CopyBits source/destination BitMap
transfer, including overlap, scaling and optional regions, then follow the
application through its first complete update cycle. Color PixMaps, vector/text
picture commands, recording/custom procedures, original UI interaction, native
presentation and production Mixed Mode remain unfinished. Full rebuild goal
stays active; no native System 7 comparison was required or claimed.

Validation: all 218 tests pass (183 default, 35 optional GPU/MPW); strict
all-target Clippy passes and MPW fixtures rebuild. No GPU tests were skipped.

### Shared CopyBits transfer

Both CPU adapters now dispatch CopyBits into the shared QuickDraw/GPU path.
Monochrome BitMaps support source crops/scaling, all eight source modes, live
foreground/background colors, complex optional masks and immutable snapshots for
overlap. GPU reductions OR source footprints, correcting the earlier nearest-
sampling behavior for compressed monochrome pictures too. Destination padding survives transfer. Current-port clipping/visibility
translate between descriptor origins; unrelated destination storage does not
inherit current-port clipping. Full legacy path inspection established that
CopyBits must honor offscreen current-port visibility, distinct from painting.

Generated checks cover exact 22-byte Pascal/six-register PPC arguments, void
results, truncated stacks and inert NIL parameters without graphics. GPU checks
compare both CPUs with read-only sources, hole masks, downscaling, all modes and
swapped colors. They cover overlap, empty visibility, independent destinations,
translated alias clips and read-only destination faults. MPW window guests now
CopyBits a PICT-rendered area and inspect canonical screen bytes.

Fresh Super Mines startup completes its CopyBits calls and stops at LineTo
($A891), PC $01000D00, trace index 20686. Next: pen-line drawing through the shared QuickDraw/GPU implementation, then
the application update cycle and actual interactions. Color PixMaps/extended CopyBits modes,
custom procedures, native presentation and real Mixed Mode remain unfinished.
Full rebuild goal remains active.

Validation: all 223 tests pass (185 default, 38 optional GPU/MPW), including
rebuilt paired compiler fixtures. Strict all-target Clippy and sequential changed-
file formatting checks pass. No GPU tests were skipped.

### GPU pen-line drawing

LineTo and Line now share live pen state and GPU coverage across 68K/PPC. Pen
patterns/modes, rectangular dimensions, visibility and caller clips use the same
QuickDraw/BitMap synchronization path as existing drawing. Hidden/zero-size pens
still advance; relative coordinates saturate like the legacy adapter. Output
permissions are checked before drawing. Unsupported recording/custom procedures
and negative pen dimensions remain explicit errors.

The shader evaluates the union of rectangular pen positions once per pixel,
avoiding repeated XOR at overlapping positions. Bounded integer geometry covers
all octants and full signed-word endpoint spans. Endpoint-preserving placement
avoids inheriting the legacy fixed-point diagonal bias/overflow; native diagonal
rounding is not claimed. Tests independently enumerate point/brush unions and
check patterns, XOR, clipping holes, pen state, ABI truncation/results and extreme
coordinates. MPW windows verify a changed pixel, XOR restoration and relative
Line pen advancement on both architectures.

Fresh Super Mines startup passes LineTo and reaches EraseRect ($A8A3), PC
$01000D62, trace index 20712. Next: rectangle erase/frame/invert/fill operations
through shared drawing, then the application's full update cycle and actual UI
interaction. Recording/callbacks, native presentation, Color QuickDraw and
production Mixed Mode remain unfinished. Full rebuild goal remains active.

Validation: all 228 tests pass (188 default, 40 optional GPU/MPW). The stronger
clip-hole assertions also pass in a focused rerun; production code is unchanged.
Strict all-target Clippy, MPW rebuilding and sequential changed-file formatting
checks pass. No GPU tests were skipped.

### Shared rectangle drawing attributes

FrameRect, EraseRect, InvertRect and FillRect now use shared checked ABIs and
QuickDraw GPU drawing. Erase selects bkPat, Fill selects a caller pattern with
patCopy and updates fillPat, and Invert ignores pen/color state to toggle bits.
Pen attributes/location are preserved. FillPat output permissions are checked
before drawing. Frames honor hidden pens and either zero pen dimension; the
zero-dimension rule is corrected for framed rounded rectangles too.

Generated 68K/PPC checks verify Pascal pointer order/popping, register truncation,
void results and truncated arguments. GPU checks compare exact frame/background/
invert/fill pixels, pen-state preservation, FillPat changes, clipping holes and
read-only output faults. Rebuilt MPW windows erase, invert twice, frame and fill
a rectangle, checking guest screen bits and the live fillPat field.

Super Mines now passes EraseRect and reaches a later DrawPicture at PC $0100E988,
trace index 21273, which explicitly faults on PICT opcode $001E (DefHilite).
Next: render PICT 184 through its version-2 DefHilite metadata and indexed
8-bit PackBitsRgn ($0099, rowBytes $80A8) with color table/mask conversion.
The resource is 4424 bytes with a 168-by-58 frame; metadata handling alone
will not complete this application-driven slice.
Original application interaction, native presentation, Color QuickDraw and
production Mixed Mode remain unfinished. Full rebuild goal stays active.

Validation: all 232 tests pass (190 default, 42 optional GPU/MPW), strict
all-target Clippy passes, MPW fixtures rebuild and sequential changed-file
formatting checks pass. No GPU tests were skipped.

### Indexed PICT 184 rendering

The complete PICT 184 slice now executes: version-2 default-highlight metadata,
8-bit indexed PixMap records, inline ColorTables, PackBitsRgn data and region
masks. CPU parsing preserves source indices/palette bytes; GPU color matching
converts RGB to the current monochrome destination before clipping/scaling/
transfer. Source layout/packing, table bounds/index identity, row lengths and
undefined logical indices are explicit checked boundaries. PackType 1 raw rows
and both byte/word packed-row lengths are supported. Highlight metadata is
picture-local; actual highlight rendering modes remain explicit errors.

Generated checks cover ordered/explicit tables, sparse high source indices,
masked pictures, raw/packed/long rows, metadata reset, all truncated prefixes and
malformed table/layout cases. Both CPU adapters produce identical pixels and
preserve guest port/row padding. Mutating the loaded resource's color table
changes the next GPU draw. MPW windows load/draw an indexed resource and inspect
black/white screen bits, including a source index whose numeric value opposes
its actual palette color.

Fresh Super Mines startup completes PICT 184 and stops at TETextBox ($A9CE),
PC $0100E766, trace index 21385. Next: bounded text-box layout and drawing through
the existing shared font/text system, including wrapping/alignment and preserved
port state, then continue the application's update cycle and interactions.
Other PixMap depths/direct packing, Color Manager/dithering and color destinations,
original interaction acceptance, native presentation and production Mixed Mode
remain unfinished. Full rebuild goal remains active.

Validation: all 235 tests pass (192 default, 43 optional GPU/MPW). Strict
all-target Clippy, MPW rebuilding and sequential changed-file formatting checks
pass. No GPU tests were skipped.


### Transient TextBox layout and bold source strikes

TETextBox ($A9CE) and PPC TETextBox/TextBox imports decode into the same signed
length/alignment request. Temporary plain TextEdit records reuse word wrapping,
font advances, view clipping and GPU redraw with erasure; the record/text graph is
always disposed. Caller port, pen and source text remain intact. Empty text erases
its view. Invalid input and allocation failures are checked before drawing.

Super Mines requests bold family 3 at size 9. Shared font selection now synthesizes
one-pixel bold source masks and advances, retaining vertical metrics, bearings,
missing-character mapping and original guest FONT bytes. Other styles remain
explicit errors; this is not a native styled-font parity claim.

Regression checks cover both ABI layouts and short-stack preservation, exact bold
masks/metrics and malformed fonts, exact GPU alignment/word-wrap/clipping pixels,
empty erasure, repeated-call heap capacity, invalid input and heap-exhaustion pixel
preservation. Rebuilt 68K/PPC MPW TextEdit programs draw bold centered text and
empty right-aligned text while checking pen/face/MemError state.

Fresh Super Mines startup passes its TextBox calls and stops at GetMouse ($A972),
PC $0100E4A8, trace index 22664. Next: implement GetMouse through shared queued
input/current-port coordinate conversion, then continue bounded original update
and input interactions. Centaurian GetGDevice and strong PPC sound imports remain
open, as do native presentation, Color QuickDraw and production Mixed Mode.
The full rebuild goal remains active. Native System 7 comparison is optional;
legacy Systemless comparison alone is authorized.

Validation: all 238 tests pass (194 default, 44 optional GPU/MPW); no GPU tests
were skipped. Strict all-target Clippy, rebuilt MPW fixtures, sequential changed-
file formatting and diff whitespace checks pass. No native System 7 run was used.


### Mouse coordinates and rectangle hit testing

GetMouse now reads the live guest Mouse point ($0830), snapshots the current
port's bitmap origin, converts with the same signed-word helper as GlobalToLocal
and writes exactly one Point. Both real CPU adapters preserve void result state.
Host pointer motion publishes the same letterboxed/clamped point to guest memory,
so guest recentering and host motion have an explicit shared boundary. Output and
color-port faults cannot publish a partial point or refresh the host cache.

PtInRect follows immediately in Super Mines' polling path. Both adapters share a
read-only signed-edge predicate: top/left included, bottom/right excluded; empty
and inverted rectangles contain no pixels. Native Pascal Boolean result storage
is checked before stack changes, while PPC uses a packed Point and r4 Rect pointer.
Generated checks cover negative/extreme coordinates, live origins, output guards,
readonly destinations, short/resultless stacks, exact Boolean bytes and edges.
Rebuilt MPW 68K/PPC guests read changed Mouse globals and verify both services;
acceptance requires their calls before the success marker.

Super Mines now completes 100,000 instruction boundaries without a service fault:
744 GetMouse/PtInRect checks and 750 ToolboxEvent calls. Its offscreen screenshot
shows the original demonstration splash. The bounded probe_mouse example injects
a mouse-down at global (256,171); the event is consumed and reaches unsupported
FindWindow ($A92C), before the application handles the click. This is polling and
input-delivery evidence, not successful original interaction acceptance. Next:
shared FindWindow hit testing/window-part results, then continue the same click.
Centaurian devices/sound, native presentation and production Mixed Mode remain
unfinished. The full rebuild goal remains active.

Validation: all 243 tests pass (199 default, 44 optional GPU/MPW); none skipped.
Strict all-target Clippy, final MPW rebuilding, sequential changed-file formatting
and whitespace checks pass. The original idle trace ends only at its explicit
instruction budget; the click probe explicitly fails at FindWindow. No native
System 7 execution or native presentation pass is claimed.


### Original splash dismissal through shared hit queries

FindWindow now shares global-point/output marshalling across 68K/PPC. It uses
screen bounds, live menu height, WindowList order and checked content/structure
regions; hidden windows are skipped and frame occlusion does not leak hits to
lower windows. The supported plain/modal/no-grow-document definitions report
content, title/active-close, menu/desktop and negative-kind system window parts.
The query preserves port, activation/list state and pixels, and works without a
GPU after window creation. Custom WDEFs remain explicit errors.

FindControl validates a live owner and complete control list before searching,
reuses TestControl for the same local coordinates/visibility/hilite semantics and
returns the first matching part/handle. StillDown is a non-consuming OS-queue
predicate for pending mouse events, following Apple Toolbox Essentials p. 2-109.
It intentionally differs from legacy's physical-state/queued-mouse-up check;
no native System 7 run is required. SetPort(nil) restores the saved empty port
state without making it drawable.

Generated tests cover both real CPUs, exact part/output/result guards, active/
inactive close boxes, hidden/overlapping windows, frame occlusion, complex region
holes, live control bounds, disabled/hidden/overlapping controls, malformed lists,
custom definitions, read-only outputs and nil-port restoration. MPW window guests
check FindWindow/FindControl parts, queued mouse-up tracking and nil restoration;
acceptance requires their actual service calls before the success marker.

The bounded original Super Mines probe presses/releases at global (256,171).
Mouse-down completes its 30,000-boundary budget; mouse-up passes hit testing and
tracking, hides the splash, disposes its control/resource/window graph and restores
its nil port. Before/after offscreen images show splash then desktop. Execution
continues to unsupported PBHOpen ($A200), with post-trap PC $010124D2. This proves
one original splash-dismissal interaction; gameplay and complete lifecycle still
remain unverified. Next: hierarchical data-fork open/access/error semantics and
continue the same transition. Devices/sound, native presentation, Color QuickDraw
and production Mixed Mode remain unfinished. Full rebuild goal stays active.

Validation: all 248 tests pass (203 default, 45 optional GPU/MPW); none skipped.
The final focused query rerun also verifies preserved MemErr. Strict all-target
Clippy, rebuilt MPW fixtures, sequential changed-file formatting and whitespace
checks pass. Native System 7 comparison and native presentation are not claimed.


### Shared settings-file lifecycle

The next original request is PBHOpen ($A200), permission 3, for the raw Pascal
name Super Mines Demo Settings in directory 17. The shared file model now has
mutable guest-created files alongside the locked loaded application. Stable node
IDs, parent/name lookup, Finder metadata, dates and fork bytes feed catalog,
PB[H]GetFInfo and FCB projections. Non-root catalog input now honors ioDirID;
older root-only fixtures were corrected to pass their actual parent rather than
reuse an output file ID. No host paths enter file services.

Both ABIs route synchronous data/resource open, creation, read/write, mark/EOF,
flush, Finder-info update and close into the same state. Access paths own marks;
forks own bytes/EOF. Current permission can downgrade to reading, write-only is
Macintosh's read/write alias, and shared/exclusive writing differs per fork.
Loaded application bytes remain locked. Guest-created data/resource forks remain
independent through writes/truncation/close/reopen. Reads include partial EOF and
newline completion; zero-fill extension and real metadata retention are checked.

Storage is bounded to 1024 created nodes, 256 simultaneous paths and 64 MiB mutable
logical bytes. Validation occurs before reference/state/output publication;
readonly/unmapped buffers and parameter blocks cannot partially alter files or
marks. Async traps and async=true generic PPC imports remain explicit failures.
Resource Manager maps are not automatically created from these raw fork bytes.
Persistence, deletion/rename, imported metadata, working directories, raw physical
allocation/range locking and read-verify flags remain unfinished.

Generated checks use both real CPUs and inspect bytes, signed error results/CCR,
marks, EOF/counts, sharing/permissions, fork isolation, Finder/catalog/FCB identity,
non-root queries, bad buffers, bounded exhaustion and explicit async boundaries.
MPW environment guests execute a complete create/write/read/seek/truncate/metadata/
close/reopen cycle on both architectures; acceptance requires every operation.
The fixture includes Devices.h for Apple's register-based I/O declarations.

The original bounded click now dismisses the splash, creates the missing settings
file, applies its Finder type/creator, opens it read/write and performs multiple
EOF/position/write/read operations. It reaches GetMHandle ($A949), post-trap
PC $01005148. Next: menu handle/resource identity and the application's menu setup,
then continue the same transition. Gameplay, complete lifecycles, native
presentation, devices/sound, Color QuickDraw and production Mixed Mode remain
unfinished. The full rebuild goal stays active.

Validation: the 253-test suite passes (208 default, 45 optional GPU/MPW), with no
GPU skips. Final focused file/catalog and compiled-environment reruns also pass
after the HFS-name validation fix. Strict all-target Clippy, sequential formatting
and whitespace checks pass. MPW guests were rebuilt with Devices.h and explicit
catalog parents; acceptance confirms actual create/read/write/metadata operations.
Native System 7 execution and cross-process settings persistence are not claimed.


### Live menu lookup and honest capability queries

GetMHandle/GetMenuHandle now searches the live DynamicMenuList: hierarchical
entries precede regular entries, IDs are signed guest words, relocated handles
retain identity, ID edits are immediately visible and misses return nil. It
validates list/handle storage, preserves MemErr and needs no GPU or color table.
It performs lookup only; it does not manufacture or load menus to satisfy a miss.

The original transition next asks Gestalt for ppc-space (PPC communications, not
PowerPC instruction execution) and evnt. Both unimplemented managers return the
undefined-selector error rather than an advertised capability. Shared sysv/proc
answers match SysEnvirons. Native A0 response/D0 error and SDK A1 store are kept
distinct from PPC's result-pointer/r3 error ABI. Errors preserve A0/PPC output;
registration/custom-selector callbacks remain unsupported.

Generated checks cover signed IDs, duplicate precedence, live edits/relocation,
missing IDs, malformed list/handle storage, color-independent lookup, exact return
slots, MemErr and both real CPUs. Gestalt checks cover native caller glue, PPC
pointer stores, undefined selectors, register/stack guards, readonly/nil outputs
and rejected registration traps. Rebuilt MPW guests require six live-menu queries
and three capability queries on each CPU before the success marker.

Super Mines now passes GetMHandle(4), closes its settings file and completes the
two unsupported-capability probes through normal guest error results. It reaches
GetNewMBar ($A9C0), post-trap PC $0100C4DE. Next: MBAR/MENU resource-backed menu
construction, installation, sizing and GPU title drawing, then continue the same
application transition. This is bootstrap progress, not completed menus or gameplay.
Original PPC interaction, devices/sound, native presentation, Color QuickDraw and
production Mixed Mode remain unfinished. Full rebuild goal remains active.

Validation: all 257 tests pass (211 default, 46 optional GPU/MPW), with no skips.
Strict all-target Clippy, rebuilt menu/environment compiler guests, sequential
changed-file formatting and whitespace checks pass. No native System 7 execution
or native-presentation pass is claimed.
