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


### Resource-backed menu bars and shared GPU titles

GetMenu and GetNewMBar now parse checked MENU/MBAR resources through the shared
Resource Manager. Ordered MBAR entries hold the same live MENU handles returned
by GetResource; menu IDs remain independent of resource IDs. Standard menu sizing
uses the shared FONT service, including supported bold item strikes, separators,
icon reservations and command-column space. Optional same-ID mctb resources merge
into the manager's color table. Missing child menus return nil/resNotFound without
installing a partial bar. Prepared menu dimensions, list storage and color changes
publish together through the heap's checked handle replacement operation. Resource
cache loads may remain cached on a failed preparation.

SetMenuBar copies checked list storage into the stable current-list handle;
disposing the supplied snapshot does not invalidate installed menu entries.
ClearMenuBar preserves the bar's resource ID. DrawMenuBar renders live titles with
wgpu, sharing bitmap glyph geometry with ordinary text rather than maintaining a
second font rasterizer. It preserves the application GrafPort/current-port slot,
including nil, clips to the bar and validates records/fonts/colors before painting.
Disabled titles use the monochrome checker pattern. Monochrome foreground lookup
uses per-menu RGB1 then global RGB1; nonmonochrome colors and distinct per-title
backgrounds fail explicitly. Standard hosted definitions require a zero MDEF field;
guest MDEF/MBDF callbacks, popup rendering/tracking and broader styles remain gaps.
Popup size reservations are an initial legacy-based implementation, not native
pixel-parity acceptance.

Generated checks cover truncated resource fields, signed/ordered/duplicate MBAR
IDs, both ABI adapters, live resource identity, missing-child failure, installed
snapshot lifetime, exact title pixels, nil current ports and malformed-live-record
rejection without partial screen updates. MPW menu guests now load resources,
install/free the returned bar, draw a title and require missing-child ResError on
both CPUs. Acceptance requires eight live-menu lookups and the final success event.

The original bounded Super Mines click now passes GetNewMBar(1000), SetMenuBar
and GetMHandle(1). Its next missing call is CountMItems ($A950), post-trap
PC $0100C4FC. Next: shared menu-item queries and application-driven menu mutation, then
continue this transition through actual menu drawing and interaction. The original
has not yet exercised populated DrawMenuBar; GPU title acceptance comes from the
focused/compiled fixtures. No gameplay, native System 7 or native-presentation
acceptance is claimed. Full rebuild goal remains active.

Continuation ABI: CountMItems ($A950) takes a MenuHandle long at SP, returns a
word at SP+4 and pops only the four-byte argument. It must count the checked live
record even when the menu is not installed; PPC CountMItems/CountMenuItems should
reach that same operation. Preserve handle identity and caller-owned storage.

Validation: all 260 tests pass (213 default, 47 optional GPU/MPW), with no skips.
Strict all-target Clippy, changed-file sequential formatting and whitespace checks
pass. The rebuilt MPW menu fixture succeeds on both CPUs. Earlier runs exposed the
fixture's stale six-query expectation; the final suite includes the corrected
eight-query acceptance and all final source changes. Native presentation and
native System 7 execution are not claimed.


### Live menu items and original menu setup

CountMItems/CountMenuItems now counts checked live records even outside the menu
bar, without graphics or font initialization, preserving MemErr. Native return
storage is validated before its four-byte handle argument is popped; PPC returns
the same shared count through r3.

AddResMenu/AppendResMenu loads named resources from the process's current single
application map, filters hidden/empty names, suppresses exact duplicate names and
appends sorted literal item text without reordering existing items. Appended items
are enabled, plain, unmarked, iconless and have no keyboard equivalent. ResLoad's
byte is set true without changing its neighboring byte. Resource handles remain
Resource Manager-owned. Cached loads may survive a failed append, but staged menu
replacement cannot partially publish item storage. FONT requests include FOND
names; non-Roman font IDs fail explicitly. Alphabetical ordering currently follows
legacy Systemless's MacRoman decoding plus Unicode lowercase comparison, not full
Script Manager collation. Additional open resource maps and Apple Menu Items
folder handling remain gaps in the resource/filesystem model.

SetItemMark, CheckItem/CheckMenuItem and SetItemCmd share checked in-place item-byte
mutation, preserving adjacent attributes, record flags, handle identity and MemErr.
Native short character arguments use the low byte; CheckItem instead reads the
Boolean's first stack byte and ignores stale padding. PPC narrows each parameter
according to its declared width and preserves void results. SetItem/SetMenuItemText
rebuilds text through atomic heap replacement while retaining attributes and other
items. Failed locked growth preserves storage; invalid item indexes are no-ops.
Empty text remains an explicit unsupported boundary. These setters do not claim
completed popup sizing/tracking or guest definition callbacks.

Generated checks cover mutable/relocated uninstalled records, malformed/nil
handles, exact stack results, signed items, Boolean padding, preserved flags and
attributes, literal resource names, sorting/filtering/duplicates, default enable
bits, resource ownership, failed growth and source-pointer faults. MPW menu guests
now execute counts, named-resource append, repeated append, mark/check/uncheck,
command-byte changes and growing/shrinking item text on both real CPUs. SDK C names
AppendResMenu/SetMenuItemText are used; the older names were undeclared and caused
link errors in the initial fixture build, which was corrected before acceptance.

The bounded Super Mines transition passes CountMItems, AddResMenu('DRVR') and its
first populated DrawMenuBar. Offscreen inspection shows File, Edit and Super Mines
titles on the desktop after the splash is dismissed. It sets menu 4/item 4's mark,
changes item 5's text and command byte, continues settings reads and constructs
WIND 1000 plus two controls. It stops at NewMenu ($A931), post-trap
PC $0100DB86. This is menu setup and further resource-window construction, not
completed gameplay or menu interaction. The next original-app milestone is dynamic menu construction. Resource-file
creation/opening, real maps in the shared fork store and map/handle identity
across both CPUs also remain required. Native
presentation, original PPC interaction and production Mixed Mode remain unfinished.
The full rebuild goal stays active.

Validation: all 268 tests pass (221 default, 47 optional GPU/MPW), with no skips.
Strict all-target Clippy, sequential changed-file formatting and whitespace checks
pass. Rebuilt MPW guests execute the new menu operations on both CPUs. The original
probe reaches NewMenu and its offscreen image shows the menu titles; it
propagates that unsupported call rather than treating the run as application
completion. Legacy comparison is permitted; native System 7 and native host
presentation are not claimed.


### Resource-file creation, menu construction and pen defaults

Correction to the previous continuation record: decimal service 43313 is $A931
(NewMenu), not $A9B1 (CreateResFile). The earlier hexadecimal conversion was wrong.
The corrected original-app boundary above and the current trace below distinguish
application evidence from the independently exercised Resource Manager feature.

ResourceFork::encode now writes checked classic resource headers, length-prefixed
data, reference/type lists, named/unnamed entries and map attributes. It preserves
resource IDs, MacRoman name bytes and resource attributes, aligns data records and
map starts to words, and rejects unrepresentable 24-bit data offsets, 16-bit map/name
references and oversized strings/counts before publishing an output. Empty maps
encode to a real 286-byte fork. Semantic round trips and deterministic re-encoding
are checked; this is not yet a complete writable Resource Manager implementation.

CreateResFile uses that encoding and the existing File Manager catalog/fork store.
It creates a new file with an empty data fork or fills an existing zero-length
resource fork without replacing its data or Finder metadata. An existing map is
left unchanged with dupFNErr; locked application forks and invalid names/parents
return errors. Capacity checks precede catalog mutation, so disk exhaustion cannot
leave a partial file. Existing raw access paths observe the same updated EOF and
bytes. Both CPU adapters preserve the void result and publish ResErr without
changing CurResFile. File Manager creation now uses the same checked node-creation
helper. Additional resource maps, resource-file open/close/search, changed-resource
writes and host persistence remain unfinished.

NewMenu allocates an ordinary caller-owned standard MENU record with a signed ID,
Pascal title, default enabled flags and an empty item terminator. It needs no GPU,
resource map or initialized menu bar and does not install itself. AppendMenu parses
item separators, icons, marks, keyboard equivalents, disabled flags and styles,
then atomically replaces the same live handle. Repeated or combined uppercase style
letters are supported as documented by Apple; this differs from the legacy parser's
one-letter-per-angle-bracket rule. Literal metacharacters still use SetMenuItemText.
Modifier-only specifications fail rather than publishing an empty item terminator.
Bounds, input faults and result storage are checked before relevant mutations.
Hosted MDEF, popup tracking and additional styles' drawing remain incomplete.

PenNormal shares a checked current-port operation that resets only pen size to
1 by 1, transfer mode to patCopy and pen pattern to black. Location, visibility,
text state, clipping and unrelated port bytes are preserved. It performs no drawing.
Generated tests run it through both real CPUs and compare the entire port record.
Rebuilt MPW menu guests create/append/count a menu and inspect exact pen defaults;
environment guests create a resource file, check duplicate ResErr/current refnum,
open the same raw fork and read its actual serialized header on both CPUs.

The original bounded Super Mines click now creates menu 5010, appends four game
names, shows WIND 1000 and draws its game/difficulty chooser, including title,
icons and borders. It passes PenNormal and reaches GetMenuItemText/GetItem ($A946),
post-trap PC $0101090C. Offscreen inspection confirms this chooser screen; labels
and setup are not yet complete, and no chooser selection, popup interaction or
gameplay is accepted. CreateResFile acceptance comes from generated/MPW fixtures;
the original has not called it in this transition. Next: live menu-item text reads,
then continue the chooser's setup/input path. Resource-file opening and writable
maps remain required as part of the full rebuild, alongside original PPC behavior,
native presentation, Color QuickDraw/devices and production Mixed Mode.
The full rebuild goal remains active.

Continuation ABI: the verified next trap is decimal 43334 = $A946, GetItem/
GetMenuItemText. Native arguments are output Pascal-string pointer at SP, signed
item word at SP+4 and MenuHandle at SP+6; pop 10, no function result. PPC takes
handle/item/output in r3/r4/r5 and must preserve its void return. Read the checked
live record (including uninstalled menus), snapshot text and validate the complete
output write before changing any destination bytes.

Validation: all 279 tests pass (232 default, 47 optional GPU/MPW), with no skips.
Strict all-target Clippy, sequential changed-file formatting and whitespace checks
pass. Rebuilt MPW menu/environment guests pass on both CPUs. Focused failures in
the fixture's Pattern structure arguments, malformed-spec example and timestamp
setup were corrected before the final suite. Offscreen chooser inspection is
accepted only for the bounded partial setup described above; native presentation,
native System 7 and actual chooser/gameplay interaction remain unverified.


### Live menu text and GPU polygon lifecycle

GetItem/GetMenuItemText now reads checked live MENU records, including uninstalled
menus, and writes a Pascal-string snapshot. It preserves MemErr, handles output
aliases, leaves guards untouched, clears an out-of-range item's output length and
validates the full destination before any write. Nil output is a no-op. Native
signed item/stack pointer order differs from PPC's handle/item/output registers;
both retain their void result. Rebuilt MPW guests require the actual item bytes.

The shared polygon manager now owns recording state tied to a live current port
and ordinary guest handle. OpenPoly initializes checked storage, sets polySave and
calls the equivalent of HidePen. Line/LineTo update live vertices and bounds even
while hidden; pen pattern, size and mode do not affect the saved definition.
ClosePoly clears polySave and balances visibility without moving the pen. Guest
suspension through a nil polySave is respected, as are ShowPen/HidePen visibility
changes. Nested/foreign/combined recording, disconnected line chains and custom
procedures fail explicitly. Recording grows through checked heap replacement;
locked growth/size-limit failures cannot partially advance the record or pen.
KillPoly disposes closed handles; OffsetPoly changes their signed-word coordinates
without allocating. Pure reads, drawing and offsets preserve the caller's MemErr.

FramePoly/PaintPoly/ErasePoly/InvertPoly/FillPoly now invoke wgpu coverage rather
than a CPU framebuffer rasterizer. Borrowed source vertices are serialized into
one bounded GPU command. Even-odd fill samples integer guest coordinates, matching
the permitted legacy Systemless reference; exact integer cross-products avoid its
floating-point intersections. Frame edges use the same line-brush predicate as
ordinary lines and form a coverage union, applying XOR once at shared corners.
Existing patterns, transfer modes, origin translation and region/visibility clips
remain shared. Frame operations preserve all port state; FillPoly follows the
existing fill-pattern field policy. Hidden/empty drawing needs no GPU.

Records are bounded to 64 KB and raster extents to 32767 per axis; GPU work is
bounded before execution. Signed-word offset wrapping is retained, while drawing
requires consistent bounds and supported extents. Color ports, region/picture
recording integration, custom procedures and native polygon pixel parity remain
gaps. This is source-geometry comparison with legacy Systemless, not a claimed
native System 7 or complete polygon compatibility pass.

Tests cover live headers/vertices, suspension, visibility, metadata and malformed
storage, locked growth, the 64-KB limit, visible-recording rollback without a GPU,
exact native/PPC arguments and return guards. GPU checks compare independent
scanline expectations for triangles, concave/self-intersecting contours and reverse
winding; cover XOR edge unions, invert/erase, supplied patterns, clip holes, visible
recording, negative/extreme bitmap origins and rejected excessive work. Existing
line pixel tests are rerun because their shader predicate is shared. Rebuilt MPW
menu guests execute the complete polygon record/draw/offset/dispose cycle and
visibility checks on both real CPUs before success.

The bounded Super Mines splash press/release now completes without a service fault
and its chooser reaches an event loop after drawing a polygon indicator. A second
click at (145,150) also completes, and offscreen inspection shows the chooser title,
Super Mines/Mini Mines labels and controls. This does not prove a selection. The
initial estimated confirmation click at (330,230) misses the live control bounds.
Guest records place the active upper control at local (304,192)-(384,210); a press
at global (402,231) reaches TrackControl ($A968), post-trap PC $0100EC06. The third
press stops there before release, so confirmation and gameplay remain unaccepted.
Next: proper held-mouse control tracking and its callback/return boundaries, then
continue this chooser interaction. Original PPC execution, writable resource maps,
native presentation, Color QuickDraw/devices and production Mixed Mode remain
required. The full rebuild goal stays active.

The bounded mouse diagnostic accepts multiple coordinate pairs, writes optional
per-click screens, prints the exact hexadecimal service error and stack arguments,
and reports live control hit bounds/status without changing guest state.

Continuation tracking evidence: the final probe's native stack at $0080FC6C
contains actionProc=0, local startPt=(v=201,h=344), control=$10002354 and a word
result slot at SP+12. TrackControl must pop 12 bytes only after validating that
result storage, remain active while the button is held, update hilite on pointer
motion and return the released part (or zero outside). The observed path needs no
action callback, but nonzero callback and custom-definition boundaries must remain
explicit until implemented; an immediate TestControl return would not satisfy it.

Validation: all 285 tests pass (237 default, 48 optional GPU/MPW), with no skips.
Strict all-target Clippy, sequential changed-file formatting and whitespace checks
pass. The rebuilt MPW menu guest passes on both real CPUs. Shader parse diagnostics
were corrected before pixel acceptance; focused test setup now separates hidden
recording from ordinary suspended drawing. The final multi-click probe reports
the exact TrackControl stack above and stops before its third release. Native
presentation, native System 7 parity and accepted confirmation/gameplay are not
claimed.


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

Validation: all 293 tests pass (240 default and 53 optional GPU/MPW), with zero
failures or skips. Strict all-target Clippy, sequential changed-file formatting
and whitespace checks pass. MPW tracking and updated menu guests were rebuilt.


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

Validation: all 297 tests pass (244 default and 53 optional GPU/MPW), with zero
failures or skips. The final expanded decimal memory-fault tests also pass
separately. Strict all-target Clippy, sequential changed-file formatting and
whitespace checks pass. The original probe reaches the CopyMask boundary above;
its partial numeric drawing is inspected offscreen, without a gameplay claim.


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

Validation: all 301 tests pass (245 default and 56 optional GPU/MPW), with zero
failures or skips. After adding CopyMask's Trap Manager veneer, the four mask
checks and four trap-patch regressions also pass on the final sources. Strict
all-target Clippy, sequential changed-file formatting and whitespace checks pass.
MPW drawing guests were rebuilt and pass on both real CPUs. The default and longer
original-app probes retain the distinct acceptance limits recorded above.


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

Validation: all 304 tests pass (248 default and 56 optional GPU/MPW), with zero
failures or skips. All 13 MPW fixture tests also pass with rebuilt tracking guests.
Strict all-target Clippy, sequential changed-file formatting and whitespace checks
pass. A test-only needless mutable borrow was corrected before strict acceptance.
The original probe and screenshot establish watch-cursor initialization and the
GetResInfo boundary above, without a board/gameplay acceptance claim.


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

Validation: all 308 tests pass (252 default and 56 optional GPU/MPW), with zero
failures or skips. Strict all-target Clippy, sequential changed-file formatting
and whitespace checks pass. Rebuilt MPW metadata/tracking guests pass on both
CPUs. Original tail runs are bounded Budget outcomes, not successful gameplay;
the explicit-clock run records clock 2188 and seed zero at the repeated-random
boundary. The full rebuild remains active.


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

Validation: all 312 tests pass (256 default and 56 optional GPU/MPW), with zero
failures or skips. Strict all-target Clippy, sequential changed-file formatting
and whitespace checks pass. Rebuilt MPW date reads pass on both CPUs. The original
board screenshot was inspected; the remaining SetEmptyRgn fault prevents claiming
completed updates or gameplay. The full rebuild goal remains active.


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

Validation: all 322 tests pass (265 default and 57 optional GPU/MPW), with zero
failures or skips. Strict all-target Clippy, sequential changed-file formatting
and whitespace checks pass. Rebuilt MPW window/wait guests pass on both real CPUs.
The original returns a window update before the IsDialogEvent fault; the selected
settle step correctly prevents the later board click. Gameplay is not accepted
and the full rebuild goal remains active.


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

Validation: all 325 tests pass (268 default and 57 optional GPU/MPW), with zero
failures or skips. Strict all-target Clippy, sequential changed-file formatting
and whitespace checks pass. The rebuilt MPW window guests pass on both real CPUs.
The original's bounded run reaches EmptyRect before the planned board click.


### Rectangle emptiness and first original board interaction (2026-10-15)

EmptyRect ($A8AE / PPC import) now uses a shared allocation-free signed endpoint
predicate over the live Rect. It treats zero/inverted dimensions as empty without
overflowing subtraction for the full signed coordinate range. Both real CPU
routes return canonical 0/1, preserve error globals and source bytes, and need
no port or graphics initialization. Native Boolean result publication is one
byte only. The adapter preflights that destination before popping its pointer;
the same preflight was added to IsDialogEvent after checking adjacent adapters.

Three new default tests cover signed extremes, inverted/zero/unit dimensions,
read-only inputs, bad memory, live changes, both ABIs, register/error guards,
protected results and stack overflow. Rebuilt MPW window guests require four
public calls with exact Boolean values before their existing success marker.
The shared behavior matches Apple's [rectangle definition](https://dev.os9.ca/techpubs/mac/QuickDraw/QuickDraw-19.html)
and legacy Systemless's endpoint predicate; its legacy word-sized Boolean write
is deliberately not reproduced.

The original Super Mines sequence (256,171), (145,150), (402,231), then a
2,000,000-boundary settle slice now reaches Waiting(10) without a service fault.
A press/release at (30,65), followed by a second bounded tail, also reaches
Waiting(10). The guest calls FindWindow, converts that real global point to local
(24,22), checks controls, then draws changed board cells. Visual inspection of
512x342 readbacks before/after that click confirms the upper-left board area
changes from covered cells to visible cells/numbers. Whole-image comparison
finds 4680 changed pixels (including cursor movement), bounds [22,55,411,236).
This proves one basic board interaction, not complete gameplay, flagging,
win/loss or application shutdown. No private binary or screenshot is committed.
The full rebuild goal remains active.

A follow-up run appends a File-menu press/release at global (35,10) after that
board click. The game routes it through FindWindow and looks up menu ID 2, then
faults at EnableItem ($A939), post-trap PC $01005200, SP $0080FE6E. The native
frame requests item 4 on handle $100020BC. This is the next executable milestone:
shared live menu enable/disable flags, then normal menu tracking/selection and
application shutdown. The File menu has not yet opened or selected a command.

The next menu slice should preserve the per-item enable mask when toggling item
zero (the whole menu); items 1..31 have individual bits and items beyond 31
cannot be individually disabled. Primary contracts: Apple's
[EnableItem](https://dev.os9.ca/techpubs/mac/Toolbox/Toolbox-150.html) and
[DisableItem](https://dev.os9.ca/techpubs/mac/Toolbox/Toolbox-151.html).

Validation: all 328 tests pass (271 default and 57 optional GPU/MPW), with zero
failures or skips. Strict all-target Clippy, sequential changed-file formatting
and whitespace checks pass. Rebuilt MPW window guests validate EmptyRect on
both CPUs. Original readbacks verify one changed board area after a real click,
and the follow-up File-menu run identifies EnableItem as the next lifecycle gap.


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

Validation: all 330 tests pass (273 default and 57 optional GPU/MPW), with zero
failures or skips. Strict all-target Clippy, sequential changed-file formatting
and whitespace checks pass. Rebuilt MPW menu guests inspect real flags through
both compiler targets and complete existing pixel/lifecycle acceptance. The
original File-menu run passes both enable operations and stops at MenuSelect.


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

Validation: all 338 tests pass (277 default and 61 optional GPU/MPW), with zero
failures or skips, including the final divider/padding checks. Strict all-target
Clippy, sequential changed-file formatting, shell syntax and whitespace checks
pass. Rebuilt MPW menu tracking guests complete the real host-driven interaction
on both CPUs. The private original fork's SHA-256 remains unchanged. Its New
Game and Quit commands reach confirmation construction, then MoveControl;
confirmation acceptance/shutdown and the full rebuild remain incomplete.


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

Validation: all 343 tests pass (279 default and 64 optional GPU/MPW), with zero
failures or skips. The final focused geometry run additionally covers hidden
owner/no-GPU invalidation on both CPUs. Strict all-target Clippy, sequential
changed-file formatting and whitespace checks pass. Rebuilt MPW window guests
complete public move/size calls and their existing lifetime/pixel acceptance.
The original Quit run reaches TEGetHeight before displaying its confirmation;
shutdown and the full rebuild remain incomplete.


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

The original Super Mines Quit route passes TEGetHeight with start=32767/end=0,
continues prompt layout and control relocation, then stops at GetIcon ($A9BB),
icon ID 0, post-trap PC $0100E9A2, SP $0080FBF2. Evidence is the private bounded
run /tmp/systemless-mines-teheight-probe.txt; original binaries remain outside
Git. The confirmation has not displayed or been accepted and shutdown has not
completed. Next: resource-backed icon lookup/drawing, then continue the real
confirmation interaction. Full rebuild and mixed-mode work remain active.

Contract: Apple's [TextEdit reference](https://dev.os9.ca/techpubs/mac/pdf/Text/TextEdit.pdf),
TEGetHeight, p. 2-92; pinned MPW Interfaces/CIncludes/TextEdit.h for the ABI.

Legacy inspection for the next slice: GetIcon delegates to resource lookup and
its built-in icon fallback supports ID 1 only, not the requested ID 0. Preserve
resource-miss semantics; establish whether the application supplies ICON 0
before introducing fallback artwork.

Validation: all 346 tests pass (282 default and 64 optional GPU/MPW), with zero
failures or skips. Strict all-target Clippy, sequential changed-file stdin
formatting, fixture shell syntax and whitespace checks pass. Both MPW targets
were rebuilt from the public headers and the original Quit path was rerun.
The full rebuild goal remains active; this is one executable milestone.


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

Inspection of the original Super Mines resource map finds ICON 128 and 129,
128 bytes each, but no ICON 0. The original Quit route now passes the real
missing-resource lookup and displays its prompt and Yes/No/Cancel controls.
Private screenshot /tmp/systemless-mines-geticon-after.png verifies the visible
"Save game before quitting?" prompt; the bounded guest continues its custom
event loop without a service fault. No native System 7 comparison was required.

Contract: Apple's [Icon Utilities](https://dev.os9.ca/techpubs/mac/MoreToolbox/MoreToolbox-287.html)
and [resource disposal](https://dev.os9.ca/techpubs/mac/MoreToolbox/MoreToolbox-290.html).

The follow-up Quit run presses No at global (270,86), after the identical splash,
chooser, board and File-menu sequence. It disposes the confirmation TextEdit
record, all four controls, its window and the game window, then stops at
GetWMgrPort ($A910), post-trap PC $0101161A, SP $0080FD1C. The confirmation
acceptance/cleanup is verified; ExitToShell has not been reached. The save and
Cancel branches and New Game confirmation were not exercised in this slice.
Evidence: /tmp/systemless-mines-quit-no-probe.txt. Next: shared Window Manager
port identity/access and continuation of original shutdown. Full rebuild remains
active, with PPC originals and real Mixed Mode still outstanding.

Validation for this adapter milestone: 282 default tests pass, zero failures,
64 optional tests explicitly skipped; the preceding complete 346-test run is
recorded above. The final focused PICT/ICON real-CPU test, strict all-target
Clippy, sequential changed-file formatting and whitespace checks pass. Original
confirmation display and No-button cleanup were rerun with bounded execution.


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

The original Super Mines sequence dismisses the splash, confirms its chooser,
changes board cells, selects File > Quit with a real press/drag/release, then
presses No at global (270,86). It now passes GetWMgrPort, sets the returned port,
continues final menu/cursor/pointer/region and file cleanup, and reaches
ExitToShell with code 0 on click 6 release. This is bounded original 68K execution
with no guest patching. Evidence: /tmp/systemless-mines-wmgr-quit-probe.txt;
post-exit PC $01000030. Original files and screenshots remain outside Git.
Save/Cancel, New Game, wider gameplay, original PPC acceptance and production
Mixed Mode remain independent incomplete requirements. The full rebuild goal
remains active.

Contract: Apple's [GetWMgrPort](https://leopard-adc.pepas.com/documentation/mac/Toolbox/Toolbox-276.html),
Macintosh Toolbox Essentials, p. 4-114. Legacy Systemless is comparison evidence;
native System 7 comparison is optional under the user's authorization.

A separate original New Game run uses drag target (60,35), declines saving at
(270,86), and visibly returns to the game/difficulty chooser without a service
fault. Evidence: /tmp/systemless-mines-newgame-no-probe.txt and its private
after.png readback. This establishes chooser return, not yet a fresh board or
save support. The Quit run's No branch reaches Exit(0); keep those distinct.

The follow-up New Game run confirms the chooser at (402,231). It draws a new
board with a visibly different layout, returns Waiting { ticks: 10 }, and stays
at that normal wait through the final bounded advance. Evidence:
/tmp/systemless-mines-newgame-restart-probe.txt and its private after.png. This
verifies no-save New Game restart; it does not establish complete gameplay or
saving. Both original lifecycle runs use actual host input, not guest patches.

The separate Quit/Cancel run presses Cancel at (367,86) and returns to the
existing game's Waiting { ticks: 10 } state without Exit or a service fault.
Private evidence: /tmp/systemless-mines-quit-cancel-probe.txt, step-4 and after
PPM/PNG readbacks. Comparison of x=20..496, y=58..319 finds 123 changed pixels;
53 are pointer movement and 70 form a former confirmation edge (x=409..422 at
y=58 and x=422 through y=114). Board/counter pixels outside those areas remain
identical. Thus Cancel lifecycle works, but exact damage restoration is not
accepted. Next: reproduce/fix that dialog-edge exposure in a focused window
fixture, then verify save-path dialogs/files and broader original gameplay.
Do not mistake no-save Quit/New Game success for full application compatibility.

Validation: all 347 tests pass (283 default and 64 optional GPU/MPW), with zero
failures or skips. The pinned MPW image rebuilt both window targets and the 68K
resource fork; their repeated public GetWMgrPort queries and existing lifecycle
pixel checks pass. Strict all-target Clippy, sequential changed-file formatting,
fixture shell syntax and whitespace checks pass. Original No-save Quit, New Game
restart and Cancel were exercised independently. The full rebuild remains active.


### Cancel damage investigation and original save entry

The prior 70-pixel Cancel difference is inside the game board border, not the
modal window's outer edge. Read-only instruction tracing of (422,60)/(410,58)
shows their initial black value comes from the application's PaintRect at
PC $01006EF2. Showing the confirmation makes them white; the later update
redraw does not restore their black value. The redraw executes with a normal
one-pixel black pen and a visible/update region covering these points. Initial
and redraw WindowRecords retain the same bitmap origin. This narrows the next
investigation to the app's initial/background and later object/bitmap redraw
paths; it does not yet prove a Window Manager defect or an application defect.
No speculative clipping, port movement or snapshot-restoration change is made.

The existing bounded probe now accepts SYSTEMLESS_PROBE_TRACE_PIXELS=422:60,410:58
(or 1..16 X:Y points inside its 512x342 display). It samples canonical guest
screen bits, excluding the presentation cursor, and reports the exact instruction
PC and services when values change. It caps emitted changes at 256, uses the
existing execution budgets, and never patches guest memory/registers. Pixel and
seed traces are independently enabled. Invalid/outside coordinates fail before
reading an application. Private traces remain outside Git.

Legacy comparison uses the preserved baseline 634c0e9120a695224d71c621fbbe33709670d707,
compiled with the ci-test profile, original archive, classic 68K preference and
512x342/1-bit display. The first bounded scripts completed actions but never
reached the confirmation (zero assertions); they are not compatibility passes.
A longer settling run panicked in legacy quickdraw/raster/spans.rs:73 with an
arithmetic overflow. Thus this comparison supplies no reliable confirmation
pixel oracle. Evidence: /tmp/systemless-legacy-cancel-run{,2,3} and matching logs;
/tmp/systemless-legacy-cancel3.json records the failed longer scenario. No native
System 7 comparison is required, and the baseline source is unchanged.

The original Yes-save branch is now independently exercised: after splash,
chooser, changed board and File > Quit, press Yes at (175,86). The confirmation
is disposed, then the guest stops at Pack3 $A9EA selector 1 (classic SFPutFile),
post-trap PC $0100878A, SP $0080FBB8. The frame has reply pointer $0080FC3A,
nil dialog hook, default-name pointer $003FE77C, prompt pointer $003FE7C8 and
where=(100,100). Evidence: /tmp/systemless-mines-quit-save-probe.txt. No file
selection, successful save or saved-game reload is claimed.

Next executable milestones: shared cooperative Standard File save UI, checked
classic reply/file identity and the actual save/reload path, while continuing
focused board-redraw investigation. Neither a silent cancellation stub nor an
unverified framebuffer workaround satisfies those requirements. The complete
rebuild, original PPC compatibility and production Mixed Mode remain active.

Validation: the reusable trace's original Cancel run has an identical service
call history and all nine PPM readbacks match the untraced run byte for byte.
Final strict all-target Clippy, changed-file stdin formatting and whitespace
checks pass. Five malformed/outside/oversized trace inputs are rejected before
application loading. Runtime semantics are unchanged; the preceding full
347-test GPU/MPW run remains the runtime validation, not a newly rerun suite.


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
fork permissions and catalog ownership remain shared. The default volume still
means the existing application-volume root; GetVol/SetVol/default-directory
selection and multiple volumes remain separate work.

Three default tests exercise both real CPUs, shared identity, path-based opens,
owner separation, actual file creation in a selected folder, classic/HFS lookup,
explicit root override, catalog enumeration, names/errors, table exhaustion and
reuse, access-path survival after CloseWD, and output faults without publication.
The public-header working.c fixture creates Saves, opens its WD twice, creates
and opens Game there, verifies the root does not contain Game, queries the WD,
closes it and verifies its stale reference fails. Both compiled architectures
reach the explicit success marker and Exit(0).

This is the required file-identity foundation for the next cooperative Standard
File save dialog. SFPutFile UI/selection/cancellation and successful original
save/reload are still unfinished; the original currently still stops at Pack3
selector 1. Preserve the full save-dialog milestone, including visible controls,
filename editing, folder/catalog navigation, replacement confirmation, checked
SFReply publication and real file operations. A silent cancellation or root-only
volume substitute does not meet that milestone. The full rebuild remains active.

Contracts: Apple's [original Standard File reply](https://leopard-adc.pepas.com/documentation/mac/Files/Files-316.html),
[File Manager reference](https://leopard-adc.pepas.com/documentation/mac/pdf/Files/File_Manager.pdf),
pp. 2-201–2-204, and [Standard File Q&A](https://leopard-adc.pepas.com/technotes/fl/fl_525.html).

The original no-save Quit regression still reaches Exit(0), recorded in
/tmp/systemless-mines-wd-quit-probe.txt. The original Yes-save path is unchanged
at the unsupported Pack3/SFPutFile entry; its visible dialog remains next.

Validation: all 351 tests pass (286 default and 65 optional GPU/MPW), with zero
failures or skips. The final expanded WD tests also pass independently. Both
MPW targets were rebuilt from public headers, and the original no-save Quit
regression reaches Exit(0). Strict all-target Clippy, sequential changed-file
stdin formatting, fixture shell syntax and whitespace checks pass. No original
save-dialog/save/reload success is claimed. The full rebuild goal stays active.


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
requirements are checked explicitly. Nonzero hooks, other Pack3 selectors,
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
nodes; host paths and persistence remain outside this File Manager model.

Real-CPU tests verify exact ABI frames, bad outputs/hooks/selectors, no-spin
waits, matching UI pixels across both architectures, cancellation and guards,
folder selection/editing, replacement/back-out, disabled names, paging/Up,
long names, WD exhaustion and caller state restoration. The public-header
standard_file.c program suspends at the dialog on both compiler targets. Save
returns through WD lookup to real Delete/Create/Open/Write/Close/GetFInfo calls,
and the host verifies the resulting bytes are exactly ABC. Cancel returns its
own explicit success marker and creates no file.

The original Super Mines Yes-save route is now exercised through the visible
SFPutFile dialog. From the changed board, select File > Quit, press Yes at
(175,86), then Save at (407,299). The guest passes PBHDelete, creates/writes/closes
Super Mines Game and reaches ExitToShell/Exit(0), post-exit PC $01000030. The
process-local catalog contains its 1,972-byte data fork (no resource fork), with
prefix 00 03 00 00 00 00 00 00 04 B0 00 00 0C DF 00 00. Evidence:
/tmp/systemless-mines-save-catalog-probe.txt; the step-6 PNG confirms the visible
filename, catalog and Save/Cancel controls. Private files/screenshots stay out
of Git. SYSTEMLESS_PROBE_CATALOG=1 optionally prints catalog names, fork sizes
and 16-byte prefixes for bounded original-app diagnostics.

A separate original run presses Cancel at (407,269). It returns to the game's
Waiting { ticks: 10 } state, and catalog readback confirms no Super Mines Game
file. Evidence: /tmp/systemless-mines-save-cancel-probe.txt and private after.png.
This verifies cancellation and save-to-process-local-bytes, not persistence or
successful reload. The earlier board-border redraw difference remains unresolved.

The next original reload slice is concrete: File > Open Game (drag to (60,52)),
then decline saving at (270,86). It stops at SFGetFile, Pack3 selector 2, PC
$010086EC, SP $0080FBCE. Its frame carries reply $0080FC56, nil dialog hook,
type-list pointer $0080FCBE, one type, file-filter callback $004002F2, prompt
$003FFD76 and where=(100,100). Evidence: /tmp/systemless-mines-open-next-probe.txt.
Implement the cooperative open picker AND real guest filter invocation; ignoring
that callback or returning false cancellation would not satisfy the slice.
Persistent file backing, original PPC application acceptance, production Mixed
Mode and the rest of the rebuild remain active.

Contracts: Apple's [Standard File Package](https://dev.os9.ca/techpubs/mac/pdf/Files/StdFilePkg.pdf),
[SFReply](https://leopard-adc.pepas.com/documentation/mac/Files/Files-316.html),
and [PBHDelete](https://leopard-adc.pepas.com/documentation/mac/Files/Files-236.html).

Validation: all 357 tests pass (288 default and 69 optional GPU/MPW), with zero
failures or skips. Both public-header Standard File targets were rebuilt and
exercise Save and Cancel through actual guest startup/return/file I/O. Final
ABI and deletion regressions also pass after the canonical OS-trap table cleanup.
Strict all-target Clippy, sequential changed-file stdin formatting, fixture shell
syntax and whitespace checks pass. Original Save, save-dialog Cancel and the
next Open Game fault were independently exercised. Persistent storage, reload,
real filter callbacks and the complete rebuild remain unfinished and active.


### Cooperative classic open dialog and original saved-game reload

SFGetFile now shares the retained Standard File window, controls, catalog,
input queue, paging/navigation, cancellation and wgpu rendering with SFPutFile.
Native Pack3 selector 2 reads exactly 28 bytes, validates the 74-byte reply,
type list and nil hook before popping; PPC uses the seven public arguments with
packed Point, signed type count and a void result. The prompt is ignored.
Type counts -1 and 0–4 are explicit: all, none, or matching actual Finder types.
Folders remain navigable. Selection/Open, Return, double-click and Cancel are
handled cooperatively. Success publishes the real type/name/WD; Cancel changes
only good. Typed filename editing remains exclusive to the save picker.

Real guest file filters now run after type matching, with true suppressing a
file. The File Manager constructs a catalog PB using its existing FileParam
metadata projection and actual parent/fork lengths. Runtime suspends the whole
caller CPU, marshals a checked Pascal/PowerPC callback frame and resumes the
ordinary instruction-budget loop. Native callbacks inherit A5; PPC uses the
transition-vector entry/TOC. A retained continuation holds the dialog until the
Boolean return, preserving caller private/register/stack state. Toolbox calls
from filters use the same process. No matching files and nil filters need no
callback. Empty UI waits execute no guest instructions; looping filters consume
only the caller's requested budget. A depth limit of 32 remains explicit.

Supported UPPs are raw 68K code and version-7 single-record prepared absolute
same-ISA RoutineDescriptors with procInfo $D0. The compiled PPC fixture uses
Apple's BUILD_ROUTINE_DESCRIPTOR directly. Unsupported flags, relative or
unprepared fragments, fat/cross-ISA descriptors explicitly fault. General
Mixed Mode, dynamic descriptor creation and CallUniversalProc are unfinished;
this production same-ISA path does not replace those rebuild requirements.

Public-header open_file.c creates GAME-typed Hidden/Visible files with ABC,
checks file metadata in its real callback, posts two callback markers and
rejects Hidden. Both compiler targets produce identical picker pixels, wait
without spinning and complete Open or Cancel. On Open the guest validates the
returned type/name/WD and reads ABC through File Manager; both outcomes reach
explicit success markers, Exit(0) and a disposed modal window. ABI fault cases,
Pascal pointer/Boolean layout, invalid descriptors/stack frames and actual
budgeted looping filters have separate regression coverage.

Original Super Mines now completes Save Game then Open Game in one process.
The picker lists/selects its real 1,972-byte saved game; the original nonzero
filter executes, the guest opens/reads/closes the selected file, redraws the
board and resumes Waiting{ticks:10} at $0100C798. Evidence:
/tmp/systemless-mines-reload-probe.txt and step-7/step-8/after images; the exact
bounded multi-drag recipe is in original-applications.md. The earlier empty
Open dialog and the saved-file selection were separately inspected. No original
binary or screenshot is committed. Full original PPC acceptance, host-backed
persistence and the complete rebuild are still active; the earlier Quit-Cancel
border discrepancy remains open.

Contracts: Apple [Standard File Package](https://dev.os9.ca/techpubs/mac/pdf/Files/StdFilePkg.pdf),
[File Filter technical note](https://leopard-adc.pepas.com/technotes/fl/fl_33.html),
[Mixed Mode Manager](https://leopard-adc.pepas.com/documentation/Carbon/Reference/Mixed_Mode_Manager/index.html)
and the pinned MPW StandardFile.h/MixedMode.h public headers. No native System 7
comparison is required by the user; no native parity is claimed.

Next executable milestones: explicit host-backed save/reload across process
launches, further original-app lifecycle/gameplay checks, then continue the
original PPC compatibility gaps and production cross-ISA Mixed Mode. Keep the
overall goal active until the complete rebuild is actually finished.

Validation: all 362 tests pass (291 default and 71 optional GPU/MPW), zero failed
or skipped, across 60 result groups. The new public-header targets were rebuilt
with the pinned MPW image, then Open and Cancel ran on both real CPU engines.
The original same-process save/reload was separately reproduced and its picker
and final board were visually inspected. No complete-rebuild claim is made.
Final focused callback/ABI tests, strict all-target Clippy, sequential changed-file
stdin formatting checks, fixture shell syntax and whitespace checks also pass.


### Portable writable volumes and original reload after restart

The File Manager now exports and transactionally restores one versioned writable
catalog image. Both forks, raw Macintosh names, catalog IDs/parents, Finder bytes
and creation/modification dates persist. The loaded application image remains
separate and its loader-owned open resource reference survives import. Live file
and WD references, access marks, CPU/heap/UI state are never restored. The same
catalog is reusable sequentially by CODE and PEF guests.

The pure codec validates CRC-32, version/reserved/length fields, bounds, names,
IDs, sibling uniqueness, parent existence/type and directory cycles before
committing. It keeps the shared 64 MiB aggregate fork and 1,024-node limits,
retains next-ID identity and reports exhausted IDs without wrapping or losing
File Manager ioResult publication. Corrupt/truncated images leave the original
catalog intact. The complete format is documented in volumes.md; it is an
independent writable-volume format, not an HFS disk image or emulator save state.

The host Volume owns explicit paths, bounded reads, a stable-sidecar exclusive
OS lock, create-new temporary writes, file synchronization, atomic replacement
and Unix parent-directory synchronization. Failed replacement leaves the old
complete image available and supports retry. Successful File Manager mutations
advance a host revision; unchanged/read-only/access-path boundaries do not
rewrite the image. Host I/O stays outside guest services. --volume PATH works
with both loaded application formats in headless/native launch paths; no-volume
behavior remains process-local. Checkpoints occur after advance/draw boundaries
and on normal host close, including successful file changes before a later
budget/fault stop. Host I/O failures are reported by the host, not translated
into already-completed guest service results. Per-service flush durability,
concurrent mounting, in-flight operation crash recovery and native-window
behavioral acceptance remain unfinished.

Public-header persistence.c creates Saves:Game with Finder type SAVE, data
PERSIST and resource RSRC. A fresh PPC process reopens the 68K-written image,
validates metadata and reads both forks. Another fresh 68K runtime and actual
CLI child processes repeat the read with identical saved bytes. New tests cover
codec corruption/topology/limits, exclusive locking, atomic failure/retry,
mutation checkpoints, error/unchanged paths, CLI malformed-image rejection and
budget-boundary checkpointing. The compiler fixtures need no graphics adapter.

Original Super Mines now saves and exits in one host process, then reloads its
1,972-byte game in a separate fresh host process. The persisted Preferences and
settings file also return. The second run executes its actual file filter,
selects/reads the saved game, redraws the board and resumes Waiting{ticks:10} at
$0100C798. Its picker and final board were visually inspected. Exact two-process
recipes and logs are recorded in original-applications.md. Private binaries,
volume images and screenshots remain outside Git.

Next executable milestones: continue original PPC application loading through
its concrete unresolved import/service gaps, expand original lifecycle/gameplay
coverage and production cross-ISA callbacks, and resolve remaining graphics,
File Manager flush/error and native host contracts. The full rebuild goal stays
active until all original requirements are fulfilled.

The next PPC gap was refreshed against the actual restored Centaurian FAT
application: the current PEF loader still rejects the strong InterfaceLib
SndDisposeChannel import before execution. Data/resource sizes and SHA-256 values
match the original recorded inputs. Evidence:
/tmp/systemless-volume-centaurian-next.txt. Implement real shared Sound Manager
channel lifetime/ABI behavior rather than aliasing this import to success, then
continue the original load through its next concrete gap.

Validation: all 371 tests pass (299 default and 72 optional GPU/MPW), with zero
failures or skips across 61 result groups. Strict all-target Clippy, sequential
changed-file stdin formatting, fixture shell syntax and whitespace checks pass.
The original game's two processes were separately run and visually inspected;
the restored Centaurian PPC input was freshly checked for its next loader gap.
No native System 7 comparison or completed-rebuild claim is made.


### Shared Sound Manager channel creation/disposal and next graphics-device gap

SndNewChannel and SndDisposeChannel now use one process registry, guest allocation
model and state implementation from native $A807/$A801 and PPC InterfaceLib.
Creation retains synth/init configuration, allocates a genuine command queue,
initializes a big-endian channel record and publishes through the caller pointer.
A nil input gets a manager-owned 1,060-byte SndChannel and 128-command queue;
a supplied record preserves userInfo, storage and a positive custom qLength.
All records link through the shared registered channel list. The reserved firstMod
field carries queue allocation identity; callback addresses are stored opaquely.

Disposal checks registration/allocation/private idle-state consistency, repairs
nextChan, frees the internal queue and frees only a manager-owned channel record.
Caller-owned records, userInfo/callback values and referenced data/handles survive.
Nil, unknown, duplicate and corrupt records return badChannel. The supported
idle slice makes both quiet modes immediately complete; unsupported playback is
not marked played or silently discarded. Synths 0/5 and known uncompressed flags
are supported for channel configuration; unknown synths and unsupported synth/
configuration requests return explicit errors. The 64-channel runtime bound and
allocation-failure rollback are tested. Queue execution, status, synthesis,
callbacks, deferred busy disposal and host audio output remain unfinished.

The native adapter validates exact 14-/6-byte argument frames plus the OSErr word
before changing SP. It decodes signed synths, complete init/callback pointers and
the high Boolean byte; PPC uses the public register arguments and signed r3 error.
Runtime preserves process Memory/Resource errors and their guest globals. Tests
exercise creation on one real CPU and disposal on the other, full live list
links, owned/caller-owned storage, guards/user data, invalid requests/frames,
corruption, heap exhaustion/rollback and channel capacity/reuse.

Public-header sound_channels.c now compiles/runs on both MPW targets. It checks
owned/custom channels, unlinking, actual caller Ptr/Handle survival, duplicate/nil
errors, an explicit success marker and Exit(0). This is real channel lifetime
acceptance, not sound playback or an original PPC application launch.

Centaurian's original PPC load now gets past its prior sound import and stops at
Index2Color. Its refreshed 68K startup still stops at GetGDevice ($AA32). Evidence:
/tmp/systemless-channels-centaurian-next.txt and
/tmp/systemless-channels-centaurian-68k-next.txt. Shared graphics-device/Color
QuickDraw state is therefore the next executable milestone for both originals.
Further sound commands and all broader rebuild requirements remain active.

In response to the gameplay question: Super Mines has the bounded automated/
offscreen board/menu/save/reload coverage already recorded. A native-window test
was launched but could not be inspected or driven because the desktop was locked;
the test process was closed. No live desktop or win/loss acceptance is claimed.

Contracts: Apple's [Sound Manager](https://dev.os9.ca/techpubs/mac/pdf/Sound/Sound_Manager.pdf)
allocation/disposal and [channel records](https://dev.os9.ca/techpubs/mac/Sound/Sound-73.html)
(Inside Macintosh: Sound, pp. 2-20–2-25, 2-103, 2-128–2-130), plus the pinned MPW
Sound.h/Memory.h public headers. Native System 7 comparison remains optional.
The complete rebuild goal remains active and is not marked complete.

Validation: all 377 tests pass (304 default and 73 optional GPU/MPW), zero failed
or skipped across 62 result groups. Final focused channel/volume regressions also
pass after retaining configuration and expanding signed/full-width ABI cases.
Strict all-target Clippy, sequential changed-file stdin formatting, fixture shell
syntax and whitespace checks pass. MPW outputs were rebuilt with the pinned image.
The full rebuild remains active; the native GUI attempt is not counted as passed.


## Original-game priorities and MoreMasters

The user selected original Escape Velocity and original Marathon as the next
compatibility targets. Initial diagnostics and acceptance criteria are recorded
in original-applications.md, with original private resource-fork fingerprints.
MoreMasters now reserves real process-owned 64-slot master-pointer blocks shared
by both CPU adapters. Reused pooled handles keep master storage reserved after
disposal. The existing on-demand individual-master fallback remains; full guest
heap-zone records/configurable moreMast/system-zone selection are unfinished.
Marathon 1 v1.2's 68K startup advances from MoreMasters to Alert ($A985). The ParamText diagnostic bytes confirm the alert requests its missing
Map/Shapes/Sounds companion files and reports -35. Gameplay has not been reached. EV 1.0.5 stops at
SetApplLimit ($A02D). PPC loads stop at SndSoundManagerVersion (EV) and
NewRoutineDescriptor (Marathon). Next work should mount Marathon companion assets and
implement real application-limit semantics for EV;
none of these observations establish either game as playable.


Follow-up Marathon evidence: a disposable SLVOLUME imported the original
Map/Shapes/Sounds/Music forks and Finder metadata into root directory 2. Shapes
and Sounds are resource-only files. The unchanged original passes its prior
missing-file alert and stops at the hierarchical GetVol PB trap ($A214), recorded
in /tmp/systemless-marathon-with-assets.txt. Implement the shared current
volume/directory query next; general host-folder import remains unfinished.


Validation for the MoreMasters milestone: 380 tests passed with no failures or
ignored tests under `cargo test --manifest-path future/Cargo.toml --locked --
--include-ignored` (including GPU/compiled guest fixtures). All three new focused
reservation tests also pass independently. Strict all-target Clippy, sequential
changed-source stdin formatting and whitespace checks pass. Detailed logs:
/tmp/systemless-moremasters-full.txt, /tmp/systemless-moremasters-final-focused.txt
and /tmp/systemless-moremasters-final-lint.txt. The full rebuild remains active.


## Current-volume queries advance original Marathon

Shared PBGetVolSync/PBHGetVolSync ($A014/$A214) now return the existing virtual
catalog's default Application Volume/root directory, with exact classic/HFS PB
fields and optional Pascal names. SetVol/HSetVol and selectable defaults remain
unfinished. Three new query tests execute both CPUs and cover nil names, absent
catalogs, short/read-only outputs, and preservation of unrelated fields/state.
The tracer accepts a bounded read-only --volume snapshot for original-game
startup diagnosis. Marathon v1.2 with original companion forks now passes GetVol
and stops at SoundDispatch selector $000C0008 (SndSoundManagerVersion). This
matches EV's unresolved PPC sound-version import. Actual sound execution and
audio output remain unfinished; neither original game is playable yet. See
original-applications.md for reproduction and limitations. The full rebuild
remains active.


Current-volume validation: 54 relevant shared-library/file-I/O/resource-file/
working-directory/query tests pass with no failures or ignored tests. Strict
all-target Clippy, sequential changed-source formatting and whitespace checks
pass. Logs: /tmp/systemless-getvol-regression.txt and
/tmp/systemless-getvol-final-lint.txt. The previous milestone's complete 380-test
GPU/MPW run remains historical evidence; unrelated GPU tests were not repeated
for these non-rendering query routes.


## CODE application partitions and absolute heap limits

EV's real SetApplLimit argument ($003DDAD8) exposed the old high-address heap's
mismatch with its A5 world. CODE preparation now selects the shared Process
allocator reservation $00010000..A5-world-base before any guest allocation;
PEF keeps the default reservation. Bounds/preflight/resize paths use that
reservation. SetApplLimit is implemented through both ABIs with real guest
ApplLimit state, bounds errors and MemErr publication. Already-grown zones and
live blocks are preserved when the limit is lowered. Lazy heap growth, guest
zone headers, multiple zones/partition sizing and general stack placement remain
unfinished. The original EV now advances to GetGDevice ($AA32). Marathon's
companion-file startup still reaches its sound-version query. Neither original
is playable. Reproduction and exact limitations are in original-applications.md.


Final partition/limit validation: all 386 tests pass with zero failures or
ignored tests under the complete locked acceptance command, including GPU and
MPW coverage. The corrected public-header environment guest executes both real
SetApplLimit calls on both CPUs; its focused acceptance test also passes.
Strict all-target Clippy, sequential changed-source stdin formatting, MPW fixture
shell syntax and whitespace checks pass. Logs:
/tmp/systemless-appllimit-acceptance.txt,
/tmp/systemless-appllimit-mpw-final.txt and
/tmp/systemless-appllimit-lint-final.txt. Original EV passes SetApplLimit and
stops at GetGDevice; original Marathon retains its sound-version boundary;
Super Mines Save/Open and the final board were reverified. Full rebuild active.


## Shared graphics-device records and indexed CopyBits sources

The process owns a real initial monochrome GDevice/PixMap/ColorTable graph,
created lazily with allocation rollback and published SDK-verified device globals.
Device queries and Index2Color share live guest records across both ABIs and
preserve error state. The actual display remains 512x342/one bit. NewCWindow,
color destination ports, mode changes, eager system-heap device initialization,
multiple devices and general setters remain unfinished.

A GPU check exposed that CopyBits rejected the returned PixMap. A checked live
indexed-source decoder now supports 1/2/4/8-bit packed sources and color tables,
reusing the existing palette-to-monochrome wgpu path and source snapshots. CPU,
public-header compiled guest, decoder and GPU readback tests cover the new slice.
Original EV now reaches NewCWindow ($AA45); original PPC Centaurian's next strong
import is BackPixPat. No original-game playback/launch or native color parity is
claimed. See original-applications.md and architecture.md for exact evidence,
contracts and limitations. The full rebuild goal remains active.


Final device/indexed-source validation: all 393 tests pass with zero failures or
ignored tests under the complete locked acceptance command, including the new
GPU copy/readback case and public-header device fixture. Strict all-target
Clippy, sequential changed-source stdin formatting, MPW build-script syntax and
whitespace checks pass. Logs: /tmp/systemless-device-acceptance-final.txt,
/tmp/systemless-device-lint-acceptance.txt, /tmp/systemless-device-mpw-final.txt
and /tmp/systemless-device-gpu-final.txt. Super Mines Save/Open was rerun and its
final PPM is byte-identical to the preceding heap-layout milestone, with normal
Waiting{ticks:10} after reopening its 1,972-byte game. Fresh original-game
boundaries are recorded in original-applications.md. Full rebuild remains active.


## Shared CGrafPort attached-handle lifetime

The next EV requirement is NewCWindow. Its port prerequisite now has actual
OpenCPort/InitCPort/CloseCPort services, one shared registry, real SDK layouts,
seven owned attached handles, and borrowed device palette/pixel lifetime.
Reset reuses existing guest allocations, including under heap exhaustion;
partial open OOM rolls back without publishing the caller record/current port.
Close validates all owned fields before freeing, retaining caller storage,
borrowed/custom palettes and other ports. InitCPort on a basic port is inert.
Native frame/void/PPC marshalling and public-header compiled guests pass.

Color-window integration/rendering remains next: introduce shared port bitmap
and PixPat accessors, then adapt existing painting, text, geometry and window
creation/rollback/disposal. Legacy inline-field mutations now reject color
ports rather than corrupting their attached handles. No Color QuickDraw
capability or original gameplay acceptance is claimed. The rebuild remains
active; exact ownership/default/error limits are in architecture.md.


EV's captured NewCWindow frame specifically requests nil storage, hidden
plainDBoxProc=2, behind=-1, no close box and refCon=0; bounds/title pointers are
$0080FB4C/$003FEFEE. The visible Boolean padding byte is $94, so preserve the
existing byte-based Boolean ABI decoding. Evidence:
/tmp/systemless-ev-color-window-frame.txt. This adds a concrete standard WDEF 2
case to the next shared window/color-port integration.


Final CGrafPort lifetime validation: all 397 tests pass with zero failures or
ignored tests under the complete locked acceptance command, including GPU and
compiled Macintosh guest fixtures. Three new ownership/ABI/failure tests and
one public-header compiled guest cover the slice. Strict all-target Clippy,
sequential changed-source formatting, fixture script syntax and whitespace
checks pass. Logs: /tmp/systemless-color-ports-acceptance-final.txt,
/tmp/systemless-color-ports-lint-verified.txt,
/tmp/systemless-color-ports-final-focused.txt and
/tmp/systemless-color-ports-mpw-final.txt. EV still requires NewCWindow; Marathon
still requires sound-version support. Super Mines retains its save/open result
and byte-identical final board. The complete rebuild remains active.


## Color-port layout and one-bit drawing

Shared checked accessors distinguish GrafPort inline bitmap/patterns from
CGrafPort PixMap/PixPat handles. Pen, geometry and rectangle/polygon drawing,
coordinate conversion, text, TextEdit and control painting use the appropriate
storage. Temporary text/control patterns are restored in their attached records.
Black/white ForeColor/BackColor update RGB and indices. CopyBits/CopyMask can
target canonical one-bit PixMaps; other destination depths/palettes and expanded
color patterns remain explicit errors.

The GPU regression creates ports and invokes PaintRect through each actual CPU
adapter, then verifies exact pixels, attached background/pen/fill storage, PenNormal, white foreground, GPU
readback, SetOrigin preservation and unsupported-depth failure atomicity.
NewCWindow remains EV's blocker; Marathon sound/Mixed Mode work is outstanding.
Neither original has verified gameplay. The full rebuild goal remains active.

Final validation covers 398 unique tests with no skips: the complete acceptance
run passed 397 and exposed one incomplete-color-port setter rejection failure.
The setter now validates its canonical one-bit bitmap before any color writes;
all 19 affected ports/color-port/text tests pass after that correction. The
initial malformed-marker regression was also corrected and the complete rerun
passes GetMouse. Logs: /tmp/systemless-layout-acceptance-final.txt and
/tmp/systemless-layout-final-rechecks.txt. The strengthened color-port fixture
invokes PaintRect through both actual CPUs and checks text selection/drawing,
tagged PixMap copying and pattern restoration. Strict lint and changed-source
format checks are recorded separately. NewCWindow and original-game sound
remain unfinished; no gameplay or full rebuild completion is claimed.


## Original EV reaches the shared sound boundary

NewCWindow now uses the existing shared Window Manager with actual CGrafPort
ownership, rollback, painting, geometry, updates and teardown. EV's hidden
all-zero setup rectangle is supported; subsequent sizing preserves its origin.
Both CPUs pass exact frame/Boolean-padding tests and a real GPU lifecycle test;
heap-exhaustion tests verify no published record/list/current-port changes or
leaked partial attachments. A public-header compiler fixture validates packed
CWindowRecord fields, child records, pixel writes, movement, resizing, updates,
CloseWindow/DisposeWindow and borrowed palette lifetime on actual 68K and PPC.

The same original executable then required BitAnd, SetGDevice and FreeMem.
Logical operations share exact full-width integer semantics; selection uses the
registered device; FreeMem reports real free extents and distinguishes total
free space from the largest allocatable block. No sound/version bypass was added.
EV now reaches SoundDispatch ($A800), D0=$000C0008, at PC $0001436C. This is the
same sound-version request previously reached by Marathon. Neither game has
verified gameplay; original PPC sound imports/Mixed Mode remain outstanding.
Trace evidence: /tmp/systemless-ev-newcwindow-sound-trace.txt. The bounded tracer
now has --services-only to avoid unnecessary instruction log volume.

The complete acceptance run passes all 407 tests with zero failures or skips,
including GPU tests and all public-header compiler guests. Focused manager and
compiler runs also pass. Logs: /tmp/systemless-newcwindow-acceptance.txt,
/tmp/systemless-newcwindow-managers-final.txt,
/tmp/systemless-newcwindow-mpw-final.txt and
/tmp/systemless-newcwindow-mpw-build-final.txt. Changed Rust files were checked
sequentially through stdin; fixture shell syntax and whitespace checks pass.
Super Mines Save/Open was rebuilt and rerun against this source, reopened its
1,972-byte game and returned to Waiting{ticks:10} at $0100C798. Its final PPM is
byte-identical to the preceding inspected milestone. Evidence:
/tmp/systemless-mines-cwindow-reload.txt and associated after.ppm. Marathon with
its original companion volume still reaches $A800:
/tmp/systemless-marathon-cwindow-next.txt. The full rebuild remains active.

Strict all-target lint also passes with -D warnings:
/tmp/systemless-newcwindow-lint.txt. No complete-rebuild or original-gameplay
claim is made; the next executable milestone is real sound processing/output
at the shared SndSoundManagerVersion boundary, followed by further display and
PPC routine-descriptor/Mixed Mode work.


## Real sampled-immediate sound and native audio transport

Both actual CPU adapters now call shared SndDoImmediate command processing.
Standard eight-bit mono buffers produce real PCM from live guest samples, with
bounded mixing, interpolation/no-interpolation, pan, channel/master volume,
quiet/disposal and completion updates. Rational phase carry fixes the exact
8-to-48 kHz sample-boundary error found by inspecting native WAV captures.
Headless virtual waits advance sound completion without a device or unbounded
sample generation. Guest exit/fault cancels playback in the common Runtime.

A thin native CPAL output owns only mixed frames. Actual native callback probes
for SDK-compiled 68K and PPC guests each consumed 2,400 frames at 48 kHz; their
WAV captures are byte-identical and match all expected samples and silent tails.
The final 68K probe recorded one recoverable Xrun; the PPC probe recorded zero.
No continuous-playback quality or live-window timing acceptance is claimed.
Evidence: /tmp/systemless-audio-native-profile-final.txt,
/tmp/systemless-audio-ppc-native-profile-final.txt,
/tmp/systemless-sampled-native-profile-final.wav and
/tmp/systemless-sampled-ppc-native-profile-final.wav. The SDK fixture also verifies both
real CPUs resume after rendering and dispose the channel. A separate CLI test
runs both compiled applications to Exit(0) with no host audio device.

This is a required sound foundation, not a version-only startup bypass.
SndDoCommand FIFO/resource playback, sound callbacks, other header formats,
channel-status/version queries and original-game playback remain unfinished.
The full rebuild and both original-game targets remain active.

An initial-volume-seven diagnostic exposed Super Mines' unimplemented HGetState
($A069) sound-enabled path: /tmp/systemless-mines-sampled-reload.txt. That
unsolicited profile change was removed; the existing initial volume is retained
and the sound fixture explicitly sets volume seven. Sound-enabled Super Mines
is not accepted and needs real handle-state/resource sound work.

Final original refreshes still stop at $A800: /tmp/systemless-ev-sampled-next.txt
and /tmp/systemless-marathon-sampled-next.txt. Next work must include real queued
commands/resource playback and callbacks before providing the requested version
contract; HGetState and related real handle attributes are also required by the
sound-enabled Super Mines path. Keep initial-volume preference work distinct
from proving that sound-enabled game paths work.

Final complete acceptance passes all 417 tests with zero failures or skips:
/tmp/systemless-sampled-acceptance-final.txt. The original-SDK build and focused
sound/CLI tests also pass: /tmp/systemless-sampled-sound-mpw-build-final.txt,
/tmp/systemless-sampled-final-mpw-tests.txt and
/tmp/systemless-sampled-resampling-final.txt. Native callback probes remain
separate from the test count and from continuous/native-window acceptance.
Changed Rust sources pass sequential stdin formatting, fixture script syntax
and whitespace checks. The new audio diagnostic obeys the loader input bound.

Super Mines' retained-profile Save/Open rerun reopens its 1,972-byte game,
returns to Waiting{ticks:10} at $0100C798 and produces a PPM byte-identical to
the preceding inspected milestone. Evidence:
/tmp/systemless-mines-sampled-final-reload.txt and associated after.ppm. This
retains the prior muted-profile interaction evidence; the volume-seven path
remains blocked at HGetState and must not be presented as working sound.
Full rebuild and original-game gameplay acceptance remain incomplete.

Strict all-target lint passes with -D warnings:
/tmp/systemless-sampled-lint-final.txt. Native transport uses verified CPAL
0.18.2 APIs (crate MSRV 1.85, below this package's 1.93 requirement). Full
continuous/native-window sound and original-game acceptance remain unproven.


## Resource-sound handle state and high-movement boundary

Both CPU adapters now expose real lock/purge/resource flags and signed-byte
HGetState results. Relocation preserves flags; restoring the lock bit changes
actual resize behavior. Resource attributes initialize live state and detach
clears only the resource bit. A public-SDK fixture checks both compiler ABIs,
resource flag restoration, detach/reload, nil/freed errors and lock-sensitive
movement. Nil-master lock calls now report -109; nonnil foreign master edits
still fault. Existing tests were updated for this corrected documented distinction.

MoveHHi stages actual upward movement through an adjacent free interval, retains
payload/flags/free-space totals and does not cross another allocation. Locked
handles report -117. General compaction, actual purging and changed-resource
writeback remain unfinished, explicitly separate from supported operations.

Sound-enabled original Super Mines passes HGetState/HNoPurge, SndNewChannel,
MoveHHi and HLock, then stops at SndPlay ($A805). Evidence:
/tmp/systemless-mines-movehi-sound-full.txt. The diagnostic now has an explicit
SYSTEMLESS_PROBE_SOUND_VOLUME selection; the initial runtime volume is unchanged.
Resource metadata inspection identifies snd 12244 as format 1 with sampledSynth,
init $A0, one offset bufferCmd, standard unsigned 8-bit mono header and rate
$56EE8BA3. Its callback-bearing channel will require real resource/queued sound
and callback completion. Private resource bytes remain outside Git.

Both original EV/Marathon sound-version boundaries and PPC/Mixed Mode work
remain; no full rebuild or original-game gameplay completion is claimed.

Continuation for resource sound: SndPlay must accept ordinary loaded handles as
well as Resource Manager identities, resolve offset command pointers within the
checked handle span, and enqueue commands in channel order. Asynchronous source
handles remain the caller's residency responsibility; do not copy private game
samples into fixtures. A nil channel forces synchronous behavior even when the
async argument is true. Waiting/full-queue behavior must suspend the guest rather
than claim completion. Completion callbacks belong on the Runtime/CPU boundary,
never the native audio thread. Primary contract:
https://dev.os9.ca/techpubs/mac/Sound/Sound-35.html.

Final acceptance: all 422 tests pass with zero failures or skips, including the
compiled SDK fixture and real GPU checks. Full run:
/tmp/systemless-handle-acceptance-final.txt. Strict all-target lint with warnings
denied passes: /tmp/systemless-handle-final-preflight-lint.txt. Focused heap,
resource-lifetime and handle-state regressions, changed-source formatting,
fixture shell syntax and whitespace checks also pass.

Retained-profile original Super Mines Save/Open still reopens its 1,972-byte
game and returns to Waiting{ticks:10} at $0100C798. The final screenshot is
byte-identical to the preceding milestone. Evidence:
/tmp/systemless-mines-handle-profile-reload.txt and associated after.ppm.
Sound-enabled SndPlay, original EV/Marathon gameplay and the full rebuild remain
unfinished; this checkpoint does not complete the goal.


## Shared sound FIFO and guest callbacks

SndDoCommand now reaches one shared guest-visible ring from native $A803 and
PPC imports. Full no-wait requests preserve pending state and report -203; waiting
requests suspend the caller without CPU spinning and retain copied command bytes.
Buffers/control commands run in order across a single PCM quantum. Flush and quiet
have distinct effects; caller-sized rings validate actual storage extents. The
analytic silent queue clock matches a 48 kHz sink across fractional rates and is
bounded by command count for enormous virtual waits.

Queued callbacks execute as budgeted guest code through the shared Runtime, with
same-ISA Pascal $3C0 descriptors/frames, full caller retention and suspended wait
restoration. They may invoke Toolbox services and quietly dispose/reallocate their
own channel. Channel generations prevent a stale callback return from clearing
new playback at the same address. Internal bookkeeping preserves MemErr/ResErr.
No callback executes on a native transport thread.

Public SDK fixture queued_sound builds with SC/MrC/Link/PPCLink and verifies exact
PCM, nested service calls, restored WaitNextEvent and channel address reuse for
both real CPUs. The corresponding headless CLI runs also exit successfully with
no host audio device. Evidence: /tmp/systemless-queue-sdk-build-accepted.txt and
/tmp/systemless-queue-sdk-accepted.txt. Focused queue, immediate playback and
channel lifetime checks are supplemented by the complete acceptance run below.

SndPlay/resource offset resolution, init-rate overrides, channel-status/version
APIs, pause/wait/sync commands, additional sample formats and nonquiet disposal
waiting remain unfinished. The original EV/Marathon startup boundaries and
sound-enabled Mines SndPlay boundary are not bypassed. Next implement loaded
resource command parsing and true synchronous/asynchronous SndPlay, then continue
capability/version and production routine-descriptor/Mixed Mode work. Native
queued audio timing and the full rebuild/gameplay acceptance remain unproven.

Original retained-profile Mines Save/Open regression passes after the Runtime
wait/callback changes. It reopens the 1,972-byte game and returns to
Waiting{ticks:10} at $0100C798; final PPM bytes match the preceding handle-state
milestone. Evidence: /tmp/systemless-mines-queue-profile-reload.txt and after.ppm.
Changed Rust files pass sequential stdin formatting checks; the fixture shell
script passes syntax validation and the diff has no whitespace errors.

Review added a generation guard for full-queue submissions as well as callback
returns. If the target is disposed/recreated while the caller is suspended, the
old submission now returns badChannel and leaves the replacement's controls
untouched. A focused regression exercises this through both CPU adapters. The
outdated broad run was intentionally stopped for this code change; final evidence is the final callback-borrow acceptance run below, not either
superseded broad log.

Resource parsing continuation: Apple's sound-storage contract places format-1
commands after the four-byte prefix, six bytes per data-type entry and the
command count word. Format 2 has an application-owned reference-count word;
it must not be treated as a variable reference list. A command's $8000 bit makes
param2 relative to the beginning of the loaded block; otherwise it remains an
absolute pointer. Preserve both forms and validate all relative header/sample
spans against actual handle size. Source:
https://leopard-adc.pepas.com/documentation/mac/Sound/Sound-60.html.
The legacy implementation is reference evidence only: its explicitly deferred
nil-channel playback is not proof of true synchronous SndPlay. The rebuild must
suspend/resume that guest call and release the internal channel after completion.

The callback's command pointer is a mutable borrow while that guest callback is
running. Native and PPC synthetic callbacks now modify those bytes, and the
public SDK callback reuses the passed command for SndDoImmediate(getAmpCmd).
This avoids diagnosing a valid mutable parameter as private-channel corruption.
The compiled recycling callback is a continuation-ownership stress fixture:
allocation/disposal at native interrupt time is not a supported Macintosh usage.
The rebuild currently has cooperative callback scheduling, not hardware interrupt
priority emulation. Reference:
https://leopard-adc.pepas.com/documentation/mac/Sound/Sound-131.html.

Additional original-game evidence: static companion-fork inspection identifies
63 EV sound/music resources (nine format 1, fifty-four format 2) and 104 Marathon
format-1 sounds. All have one offset bufferCmd and bounded inline standard samples.
EV has fifteen stale/degenerate loop metadata cases and one zero base-frequency
case that the current strict header validator rejects. Resolve the command-specific
use of these fields during resource playback; do not alter private assets or
infer full game audio coverage from this asset inventory. Hashes and details are
recorded in original-applications.md; metadata log:
/tmp/systemless-original-game-sound-metadata.txt.

Clock continuation: host elapse_ticks ages event waits saved under sound callbacks,
but the instruction-derived TickCount path currently calls calendar.advance
directly. It therefore does not age those suspended counters. Unify the clock
paths and test a long, budgeted callback crossing a pending event deadline as
part of synchronous playback/timer fidelity; current checks cover explicit host
time and do not establish complete callback/timer parity.

Final callback-borrow acceptance passes all 432 tests with zero failures or skips,
including real GPU checks and all 26 public-header compiler/CLI tests. Evidence:
/tmp/systemless-queue-borrow-acceptance.txt (terminal success),
/tmp/systemless-queue-borrow-focused.txt,
/tmp/systemless-queue-borrow-sdk-build.txt and
/tmp/systemless-queue-borrow-sdk-test.txt. Strict all-target lint with -D warnings
passes: /tmp/systemless-queue-borrow-lint.txt. Changed Rust files pass sequential
stdin formatting checks; fixture script syntax and whitespace checks pass.
Superseded broad runs were intentionally stopped for the lifetime/borrow fixes
and are not final acceptance evidence. Native queued timing, virtual-clock
integration, resource playback and full original-game/rebuild acceptance remain
unfinished. Do not mark the goal complete at this checkpoint.


## Loaded resource playback through the shared FIFO

SndPlay now works through native $A805 and PPC imports, using the existing shared
queue/renderer. Format-1/format-2 headers and relative sample spans are bounded
by actual handle size. Source bytes/flags remain unchanged, ordinary loaded
handles work without Resource Manager identity, and samples remain live. The
one-shot buffer path no longer rejects unused pitch/loop metadata, including the
previously observed EV cases. Unknown command families and unsupported encodings
remain explicit errors; rate attribute hints retain metadata while the portable
host clock/source-rate resampler controls output.

Per-call reference-counted completion tags survive queued and active work, including
guest callbacks, and are released by real completion/quiet/flush/disposal. This
provides true scoped sync waits while unrelated later audio may keep playing.
Nil-channel calls force sync and release internal allocations; async calls wait
for submission capacity if necessary and then return early. Resource waits retain
channel generations and survive guest callback suspension. Self-blocking callback
reentrancy returns busy. A new compiled fixture and focused boundary tests cover
these paths, OOM rollback, invalid/nil sources, relative versus absolute headers,
source flags, copied commands, live samples and error-global preservation.

The previous virtual-clock gap is fixed: instruction-derived ticks now use
elapse_ticks, aging event waits held under sound callbacks. A long callback in
both compiled CPU fixtures expires its saved EventWait without host elapse.
The earlier queue SDK controller now accounts for this valid deadline expiry
instead of assuming that only explicit host ticks can resume it.

probe_resource_sound uses actual synthetic 68K/PPC call instructions and compares
32-frame PCM prefixes per resource. It accepted all 59 EV Sounds, four EV Music
and 104 Marathon Sounds resources (334 calls total), including the EV metadata
cases; every internal channel was released after silent completion. It does not
run original application code, capture full waveforms or establish native audio.
The template is the public compiled resource_sound fixture, not a private game.
Final corpus logs: /tmp/systemless-ev-sounds-final-playback.txt,
/tmp/systemless-ev-music-final-playback.txt and
/tmp/systemless-marathon-sounds-final-playback.txt.

Original sound-enabled Mines now performs resource playback, returns from its real
callback, restores saved resource flags, disposes/reuses its channel and completes
Save/Open. With volume 7 and 120 virtual ticks per input step, it reopens the
1,972-byte game and returns to Waiting{ticks:10} at $0100C798; one completed sound
callback is recorded. The resulting board screenshot was inspected. Evidence:
/tmp/systemless-mines-resource-sound-clocked.txt and associated after.ppm/png.
probe_mouse now advances sound alongside explicitly requested virtual ticks;
its default zero-tick profile remains unchanged. The retained muted-profile run
also completes Save/Open and its final PPM is byte-identical to the queue milestone:
/tmp/systemless-mines-resource-muted-profile.txt and associated after.ppm.
These are different timing profiles, not a native sound/timing acceptance claim.

The next executable slice is accurate sound status/capability/version contracts,
then new original EV/Marathon startup traces and production routine-descriptor/
Mixed Mode support for PPC. Additional sample formats, note synthesis, pause/wait/
sync commands and active nonquiet disposal still remain. Full gameplay, native
queued timing and the full independent rebuild remain unfinished.

Final full acceptance succeeds with all 444 tests passing, zero failures and
zero skips, including real GPU checks and all 28 compiled SDK/CLI fixtures.
Terminal evidence: /tmp/systemless-resource-sound-acceptance-all.txt.
Focused checks: /tmp/systemless-resource-sound-acceptance-focused.txt and
/tmp/systemless-resource-sound-acceptance-sdk.txt. Strict all-target lint with
-D warnings passes: /tmp/systemless-resource-sound-acceptance-lint.txt.
Changed-source sequential stdin formatting, fixture shell syntax and whitespace
checks pass. Original corpus/probe processes also exited successfully. Private
assets/captures are excluded from Git. This is a resource-playback checkpoint;
full original-game/native-timing and rebuild acceptance remain unfinished.


## 2026-10-16: sound status, extended PCM and handle duplication

- Added shared live channel/manager status, version 3.0 ABI target and bounded
  sound capabilities (stereo, multichannel mixing, software 16-bit samples).
- Standard unsigned 8-bit mono and extended 8/16-bit mono/stereo playback use
  checked frame/channel/sample-width extents. Signed 16-bit samples are big endian.
  Mono channel output averages stereo sources; explicit stereo preserves sides.
- Both CPU adapters use the same services. An actual MPW-compiled fixture caught
  and now verifies the PowerPC hidden structure-return buffer for NumVersion;
  the 68K adapter uses its Pascal stack result. The compiled stereo/status/resource
  fixture passes on both architectures. Focused regression and strict all-target lint pass.
- Original EV 1.0.5 68K now passes the previous SoundDispatch version blocker and
  executes HandToHand during offscreen setup and next stops at SetDeviceAttribute
  (AA2D), PC $00014942. Original Marathon 1.2 68K passes
  the same sound call, then reports that Color QuickDraw is required; its subsequent
  error-dialog path stops at A985. These are startup traces, not playable games.
- Do not advertise Color QuickDraw merely to bypass the capability check: the
  current environment intentionally does not claim it. Complete the actual color
  drawing/display slice and verify it before updating the profile.
- Sound recording, codecs, disk/double-buffer playback and complete Sound Manager
  compatibility remain unsupported. Version 3.0 is an ABI target, not full parity.
  The full rebuild goal remains active. No original game data was added to Git.


### Next original-game implementation slice

The refreshed EV trace after HandToHand is /tmp/systemless-ev-hand-copy-next.txt:
AA2D SetDeviceAttribute at $00014942, with a guest-created 62-byte GDevice handle.
Implement actual gdFlags mutation with owned-record validation and atomic bounds
checks, not a successful no-op. Attribute operations must support application
created offscreen device records as well as the initial display device. Preserve
the one real shared process/device model and both ABI paths. The 68K Boolean is
at the high byte of its two-byte Pascal slot; verify with a public-header fixture.
Source: [Apple SetDeviceAttribute](https://leopard-adc.pepas.com/documentation/mac/QuickDraw/QuickDraw-281.html).
Do not conflate these per-record flags with advertising system Color QuickDraw.
Full indexed drawing destinations and display/palette behavior remain required
before publishing that broader capability to Marathon.


Final checkpoint validation: 454 tests pass, zero failures or skips, including
all 30 compiler/CLI fixtures and GPU checks. Strict all-target lint, sequential
changed-source stdin formatting, fixture shell syntax and whitespace checks pass.
Evidence: /tmp/systemless-sound-status-hand-copy-all.txt and
/tmp/systemless-hand-copy-lint.txt. The rebuilt handle_state fixture verifies
actual resource duplication and native calling conventions on both CPUs.

The original sound-enabled Super Mines Save/Open refresh again completes one
sound callback, reopens its 1,972-byte game and returns to Waiting{ticks:10},
PC $0100C798. Its board screenshot was visually inspected. Evidence:
/tmp/systemless-mines-status-stereo.txt and after.ppm/png. The 167 original sound
resource prefix/completion results match the preceding checkpoint exactly;
see /tmp/systemless-{ev-sounds,ev-music,marathon-sounds}-stereo-check.txt.
The full rebuild and original EV/Marathon gameplay acceptance remain unfinished.


## 2026-10-16: application-owned offscreen device attributes

SetDeviceAttribute (AA2D / InterfaceLib) now changes the actual gdFlags word
through the shared device service. TestDeviceAttribute supports the same owned
application-created offscreen records, not only the initial screen. Both validate
live handle ownership, 62-byte record bounds and bit range before access, preserve
unrelated bytes and MemErr, and neither initialize nor select a display. No
Color QuickDraw capability reporting changed. 68K marshals the high byte of its
Pascal Boolean slot; PowerPC marshals the low Boolean byte and preserves void
return state.

Original EV 1.0.5 68K executes the noDriver attribute update on its offscreen
record and reaches MakeITable (AA39), PC $00014964. Evidence:
/tmp/systemless-ev-device-attributes-next.txt. The next slice must build real
inverse color tables and use them consistently with palette matching; a dummy
allocation or successful no-op would not establish offscreen graphics support.
[Apple inverse-table semantics](https://dev.os9.ca/techpubs/mac/ACI/ACI-103.html)
include resolutions 3/4/5, RGB bit concatenation and additional hidden-color
information. Legacy matching uses ROM-style propagation/tie-breaking, not a
simple full-precision nearest-color scan. Treat legacy output as comparison
material, keep authoritative documented behavior distinguishable, and test
nontrivial tables before claiming conformance.

Device/color-port/window regressions: 23 pass, including real GPU checks, zero
failures or skips. Strict all-target lint and individual-file formatting pass.
Logs: /tmp/systemless-device-attributes-focused-final.txt and
/tmp/systemless-device-attributes-lint.txt. All 30 freshly rebuilt public-header compiler/CLI
fixtures pass, including actual 68K/PPC offscreen attribute calls. Log:
/tmp/systemless-device-attributes-sdk-test.txt. The preceding full suite remains the
454-test checkpoint; no full-suite rerun is claimed for this device extension.
The full rebuild and original-game acceptance remain active.


## 2026-10-16: generated inverse lookup tables and shared QDError

The shared Color Manager now builds the public inverse-table data for 3/4/5-bit
resolutions in real owned guest handles: color seed, resolution and RGB-indexed
lookup bytes. It supports device ordinals and sparse explicit pixel values,
resizes through staged allocator updates, preserves source bytes/flags and
publishes resolution/OOM errors through one process QDError state. Nil parameters
resolve through the current GDevice; the initial device now has resolution 4.
The two CPU adapters decode actual Pascal frames/import calls and share this
state. Diagnostic traces now print the latest service at a fault.

Matching currently measures integer RGB distance at each quantized cell center.
No ROM lookup blobs are embedded. This is the public lookup portion, not complete
ROM propagation/tie or hidden-color matching conformance. Tagged/reserved client
entries fail explicitly pending reservation ownership; Color2Index, palette
invalidation and full color drawing remain unfinished. Temporary matching work
uses bounded host memory rather than emulating native scratch-heap pressure.

Both CPU tests cover entire two-color lookup cubes at all three resolutions,
nontrivial sparse RGB pixels, byte order, source preservation, default resolution/
handle reuse, invalid resolution, allocation rollback and errors posted by one
CPU then queried by the other in the same process. A fresh public-header
Macintosh fixture inspects documented table bytes and QDError on both CPUs while
allowing implementation-private table storage. All 57 focused tests pass with
zero failures or skips (27 inverse/device/color-port/window tests including GPU,
plus 30 compiled/CLI fixtures). Strict all-target lint, sequential changed-file
formatting and whitespace checks pass. This is focused validation, not a new
full-suite result. Logs: /tmp/systemless-inverse-focused.txt,
/tmp/systemless-inverse-sdk-tests.txt, /tmp/systemless-inverse-cross-cpu-final.txt
and /tmp/systemless-inverse-final-lint.txt.

Original EV 1.0.5 68K completes four MakeITable calls and reaches SetGDevice for
its own offscreen handle 79676, PC $00017694. It stops because selection currently
accepts only the initial display. Trace: /tmp/systemless-ev-inverse-detail.txt.
Next implement real selection/current-device state for owned offscreen records,
retain the actual main/list device, and make palette/default-table queries use
that selected record. Do not infer a nil selection from the already-popped fault
stack. Full color destinations, original-game gameplay and the rebuild remain
unfinished; no capability flag was raised to bypass Marathon's startup check.


## 2026-10-16: selected offscreen devices and empty paint requests

SetGDevice now selects any live owned 62-byte GDevice record without registering
it as a screen or initializing the main display. GetGDevice and Index2Color read
that selected record. Main/list device identity remains separate; initializing
the initial display later preserves an existing offscreen selection. GetNextDevice
reads owned links and rejects direct self-links. Record ownership, nested palette
bounds and output storage remain checked. Nil/freed selection fails without
changing current state. Both CPU adapters use these same services.

The original EV trace next revealed PaintRect on an all-zero rectangle while
preparing a zero-height offscreen bitmap. Empty geometry now returns before
requiring pixel storage, after validating the port/custom-procedure boundary.
Nonempty geometry still validates the bitmap. A GPU regression executes actual
68K/PPC empty PaintRect calls, verifies unchanged port/bitmap/guest pixels/GPU
readback, and separately rejects nonempty drawing on the empty bitmap.

Tests also switch CPUs through one selected offscreen record, observe live color
table changes, retain main/list identity, and build default inverse tables using
the selected offscreen palette/resolution without changing the main table. The
public-header compiler fixture switches between a duplicated GDevice and the
main display and performs palette queries. All 461 tests pass with zero failures
or skips, including 30 compiler/CLI fixtures and real GPU checks. Strict all-target
lint, sequential changed-file formatting and whitespace checks pass. Logs:
/tmp/systemless-offscreen-selection-all.txt and
/tmp/systemless-offscreen-final-lint.txt.

Original EV now selects its offscreen devices, completes both empty paints and
restores the main display. It next stops at missing Get1Resource ($A81F), PC
$00016168; the untouched native frame requests resource 'RBlt', ID 1. Evidence:
/tmp/systemless-ev-empty-paint-next.txt. The earlier empty-rectangle evidence is
/tmp/systemless-ev-offscreen-paint-detail.txt. Next add actual current-map resource
lookup, distinct from the existing full-chain lookup/fallback provider. Verify
$A81F versus $A820 (Get1NamedResource) using public headers, and preserve real
resource identity/errors. Do not alias Get1Resource to a full-chain cursor lookup.
No title/menu/gameplay or full Color QuickDraw acceptance is established; the
original Marathon and full rebuild requirements remain open.


The sound-enabled original Super Mines Save/Open regression completes one sound
callback, reopens the 1,972-byte game and returns to Waiting{ticks:10}, PC
$0100C798. The restored board was visually inspected; its pixels match the prior
sound/status checkpoint exactly. Evidence: /tmp/systemless-mines-offscreen.txt
and after.ppm/png; comparison against /tmp/systemless-mines-status-stereo-after.ppm.
This does not establish native window/audio or win/loss acceptance.


## 2026-10-16: current-map resource lookup

Get1Resource ($A81F / InterfaceLib) now searches the installed application map
before consulting shared cached identity. Found resources use the existing loader,
flags, release/reload behavior and handle cache. It cannot see the built-in cursor
fallback provider, even when GetResource has already cached that cursor. Missing
IDs in an existing current-map type report resNotFound (-192); absent types return
nil with noErr, as specified by Apple. The existing $A820 Get1NamedResource remains
separate. All four native ID/name lookup frames now validate writable result slots
before popping arguments or invoking shared loading.

Both actual CPUs test shared application identity, signed IDs, live bytes,
release/reload, cached fallback exclusion and missing-type versus missing-ID
errors. Public-header guests verify native trap/import mappings and handle identity.
The new fallback-allocation fixture case runs after the existing pointer-layout
checks: inserting it earlier legitimately changed heap fragmentation and invalidated
that fixture's expected adjacent-block arrangement. No allocator assertion was
removed or weakened. All 47 focused resource/compiler/CLI tests pass with zero
failures or skips; strict all-target lint, sequential changed-file formatting
and whitespace checks pass. Logs: /tmp/systemless-get1-final-acceptance.txt and
/tmp/systemless-get1-final-lint.txt. The preceding full suite passed 461 tests;
no full-suite rerun is claimed for this resource extension.

Original EV now executes twelve Get1Resource requests for RBlt/MBlt resources
and proceeds to GetNewDialog ($A97C), PC $0002577C. Its native frame requests dialog
3000, nil storage and the front-window position (-1). Evidence:
/tmp/systemless-ev-get1-next.txt. Next inspect the original DLOG/DITL and implement
real dialog-record/item loading, ownership and drawing; do not synthesize a
successful dialog pointer. No title/menu/gameplay acceptance is established.
Multiple open resource maps, UseResFile and resource-load policy changes remain
unfinished; this extension uses the current installed application map. Full
Marathon and rebuild requirements remain active.
Source: [Apple Get1Resource](https://dev.os9.ca/techpubs/mac/MoreToolbox/MoreToolbox-51.html).


## 2026-10-16: resource-backed dialog construction

GetNewDialog ($A97C / InterfaceLib) now parses DLOG and DITL into shared
Window/Control Manager state and a real 170-byte DialogRecord. Each instance
owns an independent DITL copy and raw static-text handles; controls use the shared
control records, while icon/picture handles are actual loaded resources. Original
resources remain loaded and unchanged. GetDialogItem ($A98D), CloseDialog ($A982)
and DisposeDialog ($A983), including their PPC imports, operate on this ownership.
Native result/output storage is validated before consuming arguments. Caller
storage is checked for the complete record; failed construction restores it and
releases constructor allocations. The previous drawing port is restored.

Supported templates use standard modal/plain/no-grow definitions and default or
centerMainScreen positioning. Standard buttons, checkboxes, radio controls,
static text, icon/picture references and empty user items are parsed; unsupported
edit text, resource controls, help/custom definitions, color tables and positioning
modes fail explicitly. Controls draw on creation and are validated from the update
region; non-control item update drawing and modal interaction remain unfinished.
This follows Apple's creation/update boundary without synthesizing a dialog result.

Real 68K/PPC tests verify independent instances and text ownership, raw item types
and bounds, source DITL retention, Close/Dispose and caller storage guards. They also
exercise missing resources, scratch-allocation failure, unsupported items before
window publication and native writable-output preflight. The focused tests pass on
a real GPU adapter; strict all-target lint, sequential formatting and whitespace
checks pass. Full-suite evidence follows below.

Original EV now creates dialog 3000 and sets its port. Its next stop is
DialogDispatch ($AA68), D0=$0304, SetDialogDefaultItem, PC $000257A8:
/tmp/systemless-ev-dialog-next.txt. This is record/control construction evidence,
not complete dialog rendering, title/menu or gameplay. Implement default-button
state and drawing, item text/update drawing and real modal event handling next.
Marathon still requires actual Color QuickDraw support; PPC EV volume parameters
and Marathon production routine descriptors remain open. Neither original is
playable, and the full rebuild goal remains active.
Sources: [Apple GetNewDialog](https://dev.os9.ca/techpubs/mac/Toolbox/Toolbox-402.html),
[DITL format](https://dev.os9.ca/techpubs/mac/Toolbox/Toolbox-438.html),
[Dialog Manager helpers](https://leopard-adc.pepas.com/technotes/tn/tn1148.html).

Final acceptance: all 467 tests pass, zero failures and zero skips, including real
GPU tests and all 30 public compiler/CLI fixtures. Full all-target run:
/tmp/systemless-dialog-all.txt. Strict all-target lint:
/tmp/systemless-dialog-final-lint.txt. Sequential changed-file formatting and
whitespace checks pass. The original EV trace is separate application evidence;
no native gameplay or complete dialog rendering is claimed.


## 2026-10-16: dialog default-button state and outline

SetDialogDefaultItem now routes the exact $AA68 D0=$0304 native selector and
InterfaceLib import to one shared dialog operation. Native result storage is
validated before popping six argument bytes; OSErr uses the two-byte Pascal
result or signed PPC r3. The selected item is stored at DialogRecord+168.
Registered push buttons use their live ControlRecord bounds, not stale DITL
geometry; invalid dialogs/items return paramErr. Item zero removes the automatic
outline. No modal result or keyboard event is synthesized.

The default outline uses a three-pixel frame outside the button, matching the
existing Systemless geometry (four-pixel outset, height-derived corner diameter)
and shared rounded-rectangle coverage. Replacing/removing a default erases the old
outline with the dialog background pattern. Pen mode/pattern, colors, current
port and error authorities are preserved. Hidden dialogs retain their selection
without requiring a GPU; ShowWindow redraws their real controls and pending
outline. General dialog update/exposure drawing and modal keyboard activation
remain unfinished. No ROM geometry or pixels are embedded.

Real CPU/GPU tests verify straight-edge thickness and gap, independent button
selection, old-outline removal, unchanged button pixels, exact bitmap restoration,
GPU readback, caller port/pen/errors and hidden/show/disposed lifetime. The test
explicitly configures the classic black/white GPU palette; its initial readback
failure used the generic index-to-gray palette, while guest bitmap geometry was
already correct. The final focused dialog/control/window run passes 25 tests,
zero failures/skips: /tmp/systemless-default-final-tests.txt. The preceding full
suite passed 467 tests; no new full-suite run is claimed for this extension.

Original EV now passes SetDialogDefaultItem, ShowWindow and SelectWindow, then
stops at DrawDialog ($A981), PC $000257D2, for the same dialog:
/tmp/systemless-ev-default-next.txt. Next implement real dialog item drawing
(static text/parameters, icon and picture resources, controls/default outlines)
before modal event handling. No complete dialog rendering, title/menu or gameplay
acceptance is established. Marathon's actual Color QuickDraw and PPC routine
requirements remain open; the full rebuild goal remains active.
Sources: [Apple dialog helper calls](https://leopard-adc.pepas.com/technotes/tn/tn1148.html),
[Apple default-button drawing example](https://dev.os9.ca/techpubs/mac/Toolbox/Toolbox-383.html).

The public dialogs.c fixture now calls the real header declaration with signed
item arguments and verifies paramErr for a nil dialog. Actual 68K/PPC service
traces prove both calls execute. All 30 freshly rebuilt compiler/CLI tests pass,
zero failures/skips: /tmp/systemless-default-sdk-tests.txt. Combined with the
25 focused checks, this extension has 55 passing checks. Strict all-target lint
passes: /tmp/systemless-default-lint.txt; sequential formatting, shell syntax and
whitespace checks pass. Build log: /tmp/systemless-default-sdk-build.txt.


## 2026-10-16: actual dialog contents through shared drawing

DrawDialog ($A981 / InterfaceLib) now reads the copied live DITL and draws static
text through shared TextEdit layout, PICT handles through the existing picture
renderer, 32x32 one-bit ICON handles through wgpu bitmap transfers, and controls
and automatic default outlines through their existing managers. Static text
expands the four live DAStrings without rewriting the source handle or recursively
interpreting substituted characters. Layout is bounded by the TextEdit limit.
The drawing port, record drawing fields and error authorities are preserved.
Transient TextEdit and scratch allocations are released after use.

The effective port/visible/clip intersection selects drawing work. Hidden dialogs
need no GPU, clipped items do not dispatch rendering, and nonnil visible user-item
draw callbacks fail explicitly. Edit-text items remain unsupported at construction;
real callback execution and UpdateDialog/modal event handling remain unfinished.
Missing image handles draw nothing; malformed visible image resources fail before
earlier static items are painted. No original resource bytes are embedded.

Synthetic public DLOG/DITL/PICT/ICON resources test scaled image pixels, wrapped
static text and parameters, source retention, full GPU/guest bitmap agreement,
68K/PPC byte-identical redraws, void return behavior, hidden drawing, no retained
scratch/TextEdit allocations, and cleanup after malformed-picture faults. The
heap check brackets DrawDialog itself: Hide/Show legitimately resizes window
update regions. Runtime faults remain terminal; the fault test verifies manager
restoration through shared-service cleanup rather than attempting CPU resumption.
All 65 focused library/dialog/TextEdit/picture tests pass, no failures/skips:
/tmp/systemless-draw-dialog-acceptance.txt. Broader compiler/trap and lint results
follow below. The preceding full suite passed 467 tests; no new full-suite run is
claimed for this extension.

Original EV now executes DrawDialog, gets button/static-text items and draws an
additional default border using its own native QuickDraw calls. It next stops at
SetDialogItemText ($A98F), PC $00025816. The guest Pascal string reports failure
opening the Monitor Tool. Thus this is an application error-reporting dialog,
not title/menu or gameplay acceptance. Evidence:
/tmp/systemless-ev-draw-dialog-next.txt. Implement real item text replacement and
redraw, then investigate the actual Monitor Tool failure; do not fake success or
change graphics capability reporting to bypass it. Original Marathon and the
full rebuild requirements remain open.
Source: [Apple DrawDialog](https://dev.os9.ca/techpubs/mac/Toolbox/Toolbox-429.html).

Final extension acceptance: 99 selected tests pass, zero failures/skips (65
library/dialog/TextEdit/picture checks plus all 30 compiler/CLI fixtures and four
trap-patching checks). Additional log: /tmp/systemless-draw-dialog-sdk-tests.txt.
Strict all-target lint passes: /tmp/systemless-draw-dialog-final-lint.txt.
Sequential changed-file formatting and whitespace checks pass. The prior full
467-test run is historical, not a full-suite rerun for this extension.


## 2026-10-16: live item-text access and immediate redraw

SetDialogItemText ($A98F / SetIText) and GetDialogItemText ($A990 / GetIText)
now marshal through one shared service on both CPUs. Text handles contain raw
characters; accessor strings are Pascal bytes. Get returns at most 255 characters
and validates the complete actual output before writing. Native input strings
and the initial output byte are checked before stack pop. Source bytes are frozen
before a resize, including input aliases into the handle being changed. Shared
staged handle replacement preserves flags/identity and leaves data unchanged on
allocation failure. Successful setters publish MemErr; getters preserve error
state and both operations preserve ResErr.

Known static text items redraw immediately through shared TextEdit, limited to
their own rectangle and effective clipping. This erases old trailing glyphs when
shortened or emptied without redrawing unrelated images/controls. Hidden item
changes require no GPU. Generic owned raw-text handles also support the storage
conversion without inventing an owner or a UI. Edit-text construction and modal
handling remain unfinished. Drawing failures remain explicit runtime faults.

Actual CPU/GPU tests cover both accessor routes, void returns, input aliases and guarded outputs, flags, failed growth without mutation, empty/max-length/truncated reads,
output guards, initial native preflight, hidden changes and item-only erasure.
A new public-header compiler case verifies raw/Pascal conversions and empty text
through native/PPC mappings. The OOM fixture measures actual remaining heap space
because allocation rounds to its alignment; it does not assume one free byte from
a request leaving one nominal byte. All 94 selected library/dialog/TextEdit,
compiler/CLI and trap tests pass with zero failures/skips:
/tmp/systemless-itemtext-acceptance.txt. Fresh fixture build:
/tmp/systemless-itemtext-sdk-build.txt. Strict all-target lint, sequential
changed-file formatting, shell syntax and whitespace checks pass; lint evidence:
/tmp/systemless-itemtext-final-lint.txt. No new full-suite run is claimed; the
preceding complete run passed 467 tests.

Original EV now changes/draws the static error text, flushes events and reaches
ModalDialog ($A991), PC $0002582C. Its message reports Monitor Tool error ID -4.
Read-only tracing also confirms current GDevice PixMap depth 1 and RBlt/MBlt
resources only for ID 8 among requested depths 1/2/4/8/16/32. This supports a
possible depth incompatibility, but does not prove the precise error branch.
Evidence: /tmp/systemless-ev-itemtext-next.txt; detailed native instruction trace:
/tmp/systemless-ev-monitor-instructions.txt. Diagnostics change no guest state.

Prioritize actual indexed-color destination/display support, with coherent guest
pixel bytes, PixMaps/palettes, wgpu transfers, classic-port operations and truthful
capability reporting, to address the real EV/Marathon requirements. Do not expose
an 8-bit PixMap over one-bit storage or force success through the Monitor Tool
path. ModalDialog remains the next observed error-UI service, but passing it alone
will not make EV playable. Neither original game nor the full rebuild is complete.
Sources: [Apple SetDialogItemText](https://dev.os9.ca/techpubs/mac/Toolbox/Toolbox-419.html),
[GetDialogItemText](https://dev.os9.ca/techpubs/mac/Toolbox/Toolbox-418.html).


## Packed indexed destination milestone

One shared Surface boundary now decodes/encodes packed 1/2/4/8-bit guest storage
for both CPUs. Indexed 2/4/8-bit destinations render through wgpu with live
16-bit palette colors, nearest-color matching, pattern and copy modes, complex
clips, negative origins, overlap snapshots and duplicate-index-preserving
same-palette copies. Direct guest writes are reloaded, padding/tail bits retained
and packed bytes synchronized before returning. RGBForeColor/RGBBackColor retain
requested RGB and actual device-matched indices. InitCPort/OpenCPort can copy a
selected indexed device's format/table without taking pixel/table ownership.
Controls, text and pictures share the new destination boundary.

Nearest-color ties choose the lowest pixel index; GPU distance arithmetic retains
full precision using integer carry. This deterministic policy is not a claim of
ROM tie/propagation equivalence or native correctness for undefined colored
Boolean combinations. Matching work is bounded. Palette-relative/reserved-client
semantics, custom search/complement procedures, arithmetic/dither transfers,
expanded PixPat, direct color and indexed cursor presentation remain unfinished.
The main display remains genuinely one-bit 512×342. Real configurable color-screen
storage, coherent basic/color port views, window composition and palette/device
lifecycle are the next executable milestone; capability reporting must follow
actual behavior rather than bypass either original game's checks.

Seven new tests exercise real native/PPC calls, all packed color depths, all
eight source/pattern modes, sparse/reordered tables, full-precision RGB reference
matching, CPU/GPU synchronization, duplicate indices, overlapping copies,
complex clips, readonly storage, guarded padding and native input preflight.
All pass. A freshly rebuilt public-header SC68K/MrC color-port fixture additionally
initializes from an actual four-bit selected device, paints green into packed
storage and verifies GPU readback on both architectures. All 30 compiler fixture
artifacts rebuilt successfully: /tmp/systemless-indexed-sdk-final-build.txt;
compiled color acceptance: /tmp/systemless-indexed-compiled-color.txt. Strict
all-target lint and sequential changed-file formatting pass; lint:
/tmp/systemless-indexed-final-lint.txt.

Original traces retain EV's Monitor Tool ID -4/ModalDialog boundary and Marathon's
Color QuickDraw requirement/Alert boundary. The new offscreen foundation does not
yet resolve those screen requirements or make either game playable. Sound-enabled
Super Mines Save/Open again restores 1,972 bytes, completes one callback and waits
normally; inspected final pixels exactly match the preceding checkpoint.
Evidence: /tmp/systemless-{ev,marathon,mines}-indexed-foundation.txt. PPC imports
PBHGetVolParmsSync and NewRoutineDescriptor remain separate known blockers.
The full rebuild goal stays active.

Primary references: Apple's [Color QuickDraw drawing semantics](https://dev.os9.ca/techpubs/mac/QuickDraw/QuickDraw-199.html)
and [CopyBits](https://dev.os9.ca/techpubs/mac/QuickDraw/QuickDraw-166.html).

Continuation points for the color-screen slice: `quickdraw::install`,
`screen_bitmap` and `initialize` currently hardcode one-bit storage/stride;
`devices::Manager::initialize` creates its matching two-entry table;
`Runtime::refresh_screen` explicitly presents that fixed screen; window move and
composition paths still unpack its basic bitmap. Replace these together with a
single display-owned format/storage model. Keep the canonical screen distinct
from selected offscreen devices and current application ports. Test direct screen
writes followed by Toolbox drawing and refresh, basic/color port sharing,
window moves/exposure, cursor overlay and palette changes on both CPUs before
using the new display for original-game acceptance.

Final full acceptance: 482 tests passed, zero failures and zero skips, with
`--locked --all-targets --no-fail-fast -- --include-ignored`, exit 0. Evidence:
/tmp/systemless-indexed-all.txt. This includes all 30 freshly built compiler
fixtures and the seven new indexed destination tests. Strict all-target lint,
sequential changed-file formatting and whitespace checks also passed.


## Real indexed main display

Process now owns a bounded display policy. Consuming application preparation can
select an actual 8-bit main framebuffer before execution; default behavior stays
512×342 monochrome. `--display WIDTHxHEIGHTx8` works in file/headless/window hosts
and the trace example. Main-device PixMap, table, bounds, stride, InitGraf globals
and GPU dimensions agree. The initial color table is an explicit host RGB332
policy, not a claimed Macintosh ROM CLUT. There is no monochrome shadow storage.

Basic screen views with the full main base/extent (including translated bounds)
resolve the main device independently of selected offscreen devices. Color-port
and basic operations share canonical indexed pixels. CopyBits carries the actual
palette for basic screen descriptors; window moves copy immutable index snapshots;
menu overlays restore exact indices rather than recoloring snapshots. Cursor
white/black/inverted pixels are GPU presentation only and preserve guest bytes.
Live palette changes affect refresh. Unsupported interior basic-screen bases
fail explicitly rather than being reinterpreted as monochrome. Runtime depth
switching, palette-manager lifecycle, direct color, grayscale flags and broader
Color QuickDraw remain unfinished. SysEnvirons.hasColorQD now follows the actual
configured color profile; default mono and unknown Gestalt selectors stay as
before. No 32-Bit QuickDraw version or full API compatibility is advertised.

Five new display tests cover bounds/configuration lifetime, actual owned graph
and allocation extent, odd-width row padding, native/PPC drawing into one screen,
basic-descriptor color copies, live palette changes, cursor pixels/canonical
storage, explicit interior-base rejection, colored native/PPC window moves,
menu-bar damage limits and shared environment reporting. A new menu-tracking
test restores a nontrivial full-color indexed background on both CPUs. Earlier
focused runs pass; final full acceptance is recorded below when complete.

The public-header compiled 68K probe completes with Exit(0), 1,731 instructions
on a 65×40×8 headless display: /tmp/systemless-color-display-cli.txt. Native window
smoke attempts exit the guest successfully but time out before successful
presentation/resize on both the color and unchanged default mono profiles:
/tmp/systemless-color-display-window.txt and
/tmp/systemless-display-default-window.txt. This is an unresolved current-host
native presentation limitation; neither attempt is counted as a pass. The
exact cause is unverified. Offscreen acceptance is separate evidence.

Original Marathon 68K passes the Color QuickDraw rejection on the real 640×480×8
profile and next requests PBHSetVol ($A215), PC $01001ECE, after real asset and
working-directory queries. EV still reports Monitor Tool ID -4 despite genuine
8-bit screen storage and reaches ModalDialog ($A991), PC $000260B4. This rules out
depth alone as a sufficient explanation. Logs:
/tmp/systemless-marathon-color-display.txt and
/tmp/systemless-ev-color-display.txt. Neither game is playable.
Original Super Mines on the default mono profile again completes sound-enabled
Save/Open, reopens 1,972 bytes, completes one callback and returns to its normal
wait; final screenshot inspected and byte-identical to the indexed-foundation
checkpoint. /tmp/systemless-mines-color-display.txt and matching -after.ppm.

Next original-game slice: implement actual default-volume/directory selection
for PBSetVol/PBHSetVol, not a success stub. Current `files/working.rs` assumes root
2 in `parent` and Current queries. Selection must affect relative file, catalog
and Standard File operations consistently while preserving valid working-
directory identities, and honor the distinction between classic and hierarchical
queries. Check persistence boundaries deliberately. Primary contracts:
[Apple GetVol](https://dev.os9.ca/techpubs/mac/Files/Files-162.html) and
[File Manager signatures/trap macros](https://dev.os9.ca/techpubs/mac/Files/Files-301A.html).
Continue EV Monitor Tool diagnosis and PPC PBHGetVolParmsSync/NewRoutineDescriptor
requirements. The complete rebuild goal remains active.

Review additionally found a one-bit assumption in visible color-window background
painting. Window composition now carries the source window's actual palette RGB
through the shared clipped paint service into the main screen. NewCWindow binds
to the main device while preserving any application-selected offscreen device.
The new visible-window test verifies main base/stride, selection preservation,
green hide/show/repaint/movement pixels and untouched offscreen storage. The
focused test and all 12 existing window tests pass:
/tmp/systemless-visible-color-window-final.txt and
/tmp/systemless-color-window-composition.txt. Final all-target acceptance follows
this correction; superseded partial runs are not used as final evidence.

Additional read-only EV diagnosis while final validation runs: the earlier
instruction trace's first D0=-4 follows PBGetFCBInfo returning -51, then application
CODE assigns D3=-4 and returns it. The relevant original CODE 12 bytes at offset
$72E0 are `31 78 0A 58 00 18 70 08 A2 60 66 24`: the guest copies low-memory
$0A58 into ioRefNum (+24), then calls PBGetFCBInfo (selector 8). $0A58 is SysMap,
the system resource-file reference, **not** CurApRefNum ($0900); do not alias it to
the application's file merely to pass the check. The rebuild does not own an
installed System file, and currently leaves this word zero. This is a concrete
FCB/system-resource identity lead, not yet proof that this precise error instance
propagates into the later Monitor Tool dialog. Trace context: approximately
instruction 798545–798560 in /tmp/systemless-ev-monitor-instructions.txt. Current
color traces still reach the same reported ID -4. Investigate genuine virtual
system-resource ownership/metadata and error propagation; no fabricated FCB or
success result has been added.

Final-source original refresh uses /tmp/systemless-marathon-color-display-complete.txt
(PBHSetVol $A215, PC $01001ECE), /tmp/systemless-ev-color-display-complete.txt
(Monitor Tool ID -4, ModalDialog $A991, PC $000260B4), and
/tmp/systemless-mines-color-display-complete.txt (restored game, one sound
callback, normal wait). The final Super Mines -after.ppm was inspected and is
byte-identical to the preceding indexed-foundation checkpoint. These refreshes
use the completed visible-color-window composition code.

The final file frontend also runs both public-header compiled resource/heap
probes successfully on a real 65×40×8 headless display: 68K Exit(0), 1,731
instructions/67 service calls, and PPC Exit(0), 1,086 instructions/72 calls.
Evidence: /tmp/systemless-color-display-complete-cli-{68k,ppc}.txt. These use
actual CODE/resource-fork and PEF plus resource-fork loading respectively.

Final acceptance of the completed display and visible-color-window implementation:
488 tests passed, zero failures and zero skips; all-target run exited 0 with
`--locked --all-targets --no-fail-fast -- --include-ignored`. This includes all
30 compiler cases and the six new display/menu tests. Evidence:
/tmp/systemless-color-display-complete-all.txt. Strict all-target lint exited 0:
/tmp/systemless-color-display-complete-lint.txt. Final diagnostic build exited 0:
/tmp/systemless-color-display-complete-examples.txt. Sequential changed-source
formatting and staged whitespace checks pass. Native-window smoke timeouts remain
explicitly unpassed; full game acceptance and the rebuild are not complete.


## Real default-volume and directory selection

Files owns process-local selected directory/reference/owner state. PBSetVolSync,
PBHSetVolSync and native $A015/$A215 use the same implementation and real catalog
paths. Selection is published only after mapped writable PB validation, frozen
name input and successful resolution. Hierarchical full paths override an invalid
reference, WD references ignore stale directory IDs, and missing volumes/folders
produce actual errors. Classic volume selection returns to the root. Current
queries preserve classic/hierarchical reference and optional-name distinctions.

Default relative operations and Standard File initial browsing use the selected
folder. Classic explicit physical-volume references retain root behavior; WD
references retain their folders. Explicit-volume catalog queries with zero
directory ID address the root, whereas default queries use the current folder.
A selected directory cannot be deleted; closing its WD clears the reference and
retains physical directory identity. Default selection is process state, not a
catalog modification or persisted volume setting. Multi-volume selection,
asynchronous completion, high-level wrappers and broader search fallback remain
unfinished. No success stub or host path was introduced.

Three new actual-CPU tests verify cross-CPU selection, real relative file effects,
classic/HFS/catalog distinctions, full/partial names, invalid selections, guarded
and readonly PBs, WD lifetime, selected-directory protection and fresh-process
volume restoration. Existing WD tests remain. The public-header working.c fixture
now checks selection/query and default file lookup through SC68K and MrC/PPC,
including stale ioWDDirID with a WD reference and resetting to root. All compiler
artifacts rebuilt successfully: /tmp/systemless-setvol-sdk-build.txt.

Original Marathon 68K passes PBHSetVol and next requests OpenRFPerm ($A9C4),
PC $010023C6. Its resource-map opening/selection/lifetime slice is next; preserve
real file access, parsed maps, resource identity and error behavior across both
CPUs. The initial trace is /tmp/systemless-marathon-setvol-first.txt; refresh is
/tmp/systemless-marathon-setvol-final.txt. Neither original game is playable.
The full rebuild goal stays active. Final validation is recorded below after
selected file/resource/compiler/UI acceptance completes; the preceding full
all-target checkpoint remains 488 passes, not a new full-suite claim.

Final-source refresh: /tmp/systemless-marathon-setvol-complete.txt confirms
OpenRFPerm ($A9C4), PC $010023C6; /tmp/systemless-ev-setvol-complete.txt retains
Monitor Tool ID -4/ModalDialog ($A991), PC $000260B4. Super Mines again reopens its
1,972-byte game, completes one actual sound callback and returns to normal wait
at $0100C798. Its final screenshot was inspected and its -after.ppm is byte-for-byte
identical to the preceding color-display checkpoint. Evidence:
/tmp/systemless-mines-setvol-complete.txt. Native-window presentation remains
unverified; no new native smoke pass is claimed.

Final selected acceptance passes 103 tests, zero failures and zero skips, exit 0:
/tmp/systemless-setvol-complete-acceptance.txt. This covers library, volume/file
I/O and metadata, working directories, persistence, resource files, Standard File,
trap patches and all 30 freshly rebuilt compiler/CLI cases. The final explicit-
volume catalog test passes on the corrected code; intermediate failures are not
final evidence. Strict all-target lint exits 0:
/tmp/systemless-setvol-verified-lint.txt. Sequential staged Rust formatting and
whitespace checks pass. No new full all-target suite is claimed; its preceding
checkpoint remains 488 passes. Neither game nor the rebuild is complete.


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


Resource-map checkpoint validation: four new final-source tests pass, zero
failures/skips, exit 0 (/tmp/systemless-resource-maps-four-tests.txt). These cover
actual 68K/PPC opening/selection/closing/home calls, duplicate IDs with distinct
names, fallback search, managed raw-close protection, detached lifetimes, errors,
native result boundaries and preload rollback. The preceding 68 selected
regressions passed before the final selection/builtin/preflight refinements;
that earlier run is not claimed as final-source full acceptance. Final-source
strict all-target lint exits 0 (/tmp/systemless-resource-maps-lint.txt), sequential
changed-file formatting and git diff --check pass.

Original traces: /tmp/systemless-{marathon,ev}-resource-maps.txt. Marathon's
instruction trace (/tmp/systemless-marathon-resource-maps-instructions.txt) shows
ResError = 0 after OpenRFPerm, then three sound channels and missing InsTime
($A058). EV retains Monitor Tool -4 and missing ModalDialog ($A991). The refreshed
Super Mines regression (/tmp/systemless-mines-resource-maps.txt) completes one
sound callback, reopens 1,972 bytes and waits at $0100C798; its inspected screenshot
is byte-identical to /tmp/systemless-mines-setvol-complete-after.ppm. No native
window/audio or original-game gameplay pass is claimed.

Full all-target acceptance remains RUNNING, not passed, in command session 98583
(/tmp/systemless-resource-maps-all-tests.txt). Resume that same job rather than
restarting it. It has completed compilation and its first library/CPU/graphics
modules without a failure so far. Record its final exit and totals before
claiming broad acceptance. Next executable implementation: shared Time Manager
task ownership, scheduling and state-preserving same-ISA callbacks for Marathon;
continue EV's genuine SysMap/FCB/Monitor Tool diagnosis. Keep the rebuild active.


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


Timer checkpoint verification: seven final integration tests and two internal
ownership/clock tests pass with zero failures/skips. The focused calendar,
sound-queue and waiting regressions pass 16 further tests; the earlier six-timer
run is superseded by the final seven-case run. The compiled timer fixture passes
on both SC68K and MrC/PPC, executing three real callbacks per guest, cancellation,
rearming, idle resumption and ExitToShell. SC callback entry uses the Pascal A1
reader from IM: Processes listing 3-3; a register-parameter declaration alone
incorrectly generated a stack parameter and was corrected in the fixture. The
runtime continues to supply the documented register ABI. The diagnostic trace
now supports --elapse-waits for bounded virtual idle-time investigations.

Evidence: /tmp/systemless-timers-seven-tests.txt,
/tmp/systemless-timers-final-ownership.txt,
/tmp/systemless-timers-focused-final.txt,
/tmp/systemless-timers-sdk-final.txt and
/tmp/systemless-timers-sdk-verified.txt. All original compiler artifacts rebuilt;
final timer artifacts were regenerated from the corrected volatile-counter source.
Strict all-target lint passes /tmp/systemless-timers-lint-final.txt (exit 0), and
the final tracing/test diagnostics pass /tmp/systemless-timers-diagnostics-lint.txt
(exit 0). Sequential changed-file formatting and whitespace checks pass.

The older resource-map broad suite was deliberately stopped as SUPERSEDED because
source and selected binaries changed during the timer milestone. Its exit is 143,
not a pass; it cannot establish a single final-source checkpoint. Replacement
complete acceptance covers all 79 integration targets in two normal Cargo batches,
40/39 targets, with the library, binary and all five examples in batch 1 and all
ignored tests enabled in both. Coverage is recorded in
/tmp/systemless-timers-full-coverage.json. Both jobs remain running:
command sessions 21627 and 76629; logs /tmp/systemless-timers-full-{1,2}.txt.
Continue these exact jobs. Keep runtime/test/compiler source fixed until both
finish; source fingerprint /tmp/systemless-timers-full-source.sha256 is
c188ed76478efa9531be7c04002816f832488e4268061507aff6b9d68f289e43.
Do not claim a full pass until both exits, totals, failures and skips are inspected.
Documentation edits do not affect that code fingerprint.

Final original traces /tmp/systemless-{marathon,ev}-timers-final.txt confirm
Marathon's next HasDepth boundary and EV's unchanged Monitor Tool -4/ModalDialog
boundary. Refresh original Super Mines after the current probe_mouse build
(command session 37344, /tmp/systemless-timers-mouse-probe-build.txt), before
claiming the shared callback/wait refactor preserves that interaction regression.
Next implementation after full acceptance: actual display mode/capability
semantics for HasDepth/SetDepth, with real device records and pixels, then further
Marathon startup. EV's System-resource FCB/Monitor Tool path remains a separate
actual missing-capability diagnosis. No original-gameplay or full-rebuild claim.


Final timer-source Super Mines refresh passes, exit 0:
/tmp/systemless-mines-timers-final.txt. It completes one sound callback, reopens
its 1,972-byte game and returns to event waiting at $0100C798. The final screenshot
was inspected and is byte-identical to the resource-map checkpoint. Evidence:
/tmp/systemless-mines-timers-final-after.ppm and -after.png. This closes the
interaction-refresh step above; native presentation/audio and win/loss acceptance
remain open. Both exhaustive acceptance batches remain running and must be resumed
before changing runtime/test/compiler source. The full rebuild stays active.


### Timer checkpoint follow-up: CI and next display slice

GitHub's two Future runtime jobs for 28a3e05e07c24ebd5998e3872c5b8afa4c47d67c
pass their default cargo test step but fail strict all-target Clippy. The remote
stable toolchain is Rust 1.99; the local prior all-target pass used Rust 1.95.
The reported failures are the newer constant-size chunk-iteration lint and a
byte-literal lint. Evidence: /tmp/systemless-timers-ci-run-details.json and
/tmp/systemless-timers-ci-lints.txt. Do not describe current CI as green.

The repairs were prepared and validated in the isolated source tree
/tmp/systemless-future-ci-style/future, then promoted after both exhaustive
batches completed (see the following acceptance update). Patch:
/tmp/systemless-ci-style.patch; changed-file list:
/tmp/systemless-ci-style-changed-files.txt. It applies cleanly to the current
worktree with git apply --check. Strict Rust 1.99 all-target lint passes, exit 0:
/tmp/systemless-ci-style-rust199-clippy-acceptance.txt. Existing affected tests
pass 114 cases, zero failures/skips, exit 0:
/tmp/systemless-ci-style-rust199-tests.txt. This includes actual-GPU menu,
dialog, TextEdit, polygon and indexed-pixel cases. No warnings were suppressed
or toolchain pinned backward. The APIs used are stable since Rust 1.88; an
isolated Rust 1.93 minimum-version check also passes, exit 0:
/tmp/systemless-ci-style-msrv-check.txt.

The timer batches 21627/76629 have now completed successfully; the source
fingerprint was verified before promoting the patch. Compiler repair commit:
e08716f8f, pushed to the authorized dev/future branch.

Native smoke /tmp/systemless-timers-native-color-smoke.txt exits 1 after 3,144
guest instructions exit successfully and surface presentation times out. The
native UI tool separately confirms the Mac is locked and cannot inspect apps.
No native-window pass is claimed. Retry on an unlocked desktop before attributing
past timeouts to an implementation defect; the current lock does not prove the
cause of older failures.

Marathon's HasDepth request is the SDK D0-selector form: trap $AAA2, D0 low word
$0A14, flags 1, whichFlags 1, depth 8, GDHandle $00010884. The native arguments
occupy ten bytes (flags/whichFlags/depth words, then device long), followed by the
word result; do not consume a nonexistent stacked selector. The pinned SDK's
Palettes.h and Apple's HasDepth/SetDepth reference agree. Mode IDs are capability
identities accepted by SetDepth; a result must correspond to actual device mode,
pixels and palette. Current GDevice initialization leaves gdMode zero. Future
mode work must reconcile gdMode, screen storage, QuickDraw globals and existing
basic/color screen views, plus composition/redraw and palette state. Do not
advertise unsupported depths, grayscale, direct color or fake a successful switch.
References:
https://leopard-adc.pepas.com/documentation/mac/QuickDraw/QuickDraw-295.html
https://leopard-adc.pepas.com/documentation/mac/QuickDraw/QuickDraw-296.html
SDK evidence: /tmp/systemless-display-depth-sdk.txt; invalid-depth cDepthErr is
-157 in the SDK (/tmp/systemless-display-depth-errors-sdk.txt).


Fresh EV diagnosis on the unchanged timer checkpoint now proves propagation of
the earlier SysMap/FCB lead. /tmp/systemless-ev-timers-current-instructions.txt
runs the original EV with actual 640x480x8 pixels; concise checked evidence is
/tmp/systemless-ev-current-error-propagation.txt. Original CODE 12 bytes at $72E0
are 31 78 0A 58 00 18 70 08 A2 60 66 24: copy SysMap ($0A58) into the real FCB
parameter block, then invoke FCBInfo selector 8. Instruction 798545 at $0001B2EC
calls it; 798546 receives -51. The helper returns a nonzero error, and instruction
798557 at $00019974 assigns -4 to D3. The function returns that value through D0,
the caller retains its word, and instructions 799413–799421 pass it to NumToString.
The trace records Decimal(Format { number: -4, ... }) and the resulting Monitor
Tool ID = -4 text, then the known missing ModalDialog boundary at $000260B4.
This is now verified value propagation, not merely the first observed -4.

This fallback exposes a missing ROM-free owned System resource-file /
boot-directory capability, but later diagnosis below identifies an earlier
trap-discovery error that avoids this fallback. SysMap is the
System resource-file reference, not CurApRefNum; do not alias it to the app FCB
or force success. Real map/search/handle ownership, file metadata and folder
identity must agree. Resolving this first actual failure does not prove that the
rest of Monitor Tool initialization or gameplay will work. Neither original game
nor the full rebuild is complete.


### Completed acceptance and Escape Velocity trap discovery

Both frozen timer-checkpoint acceptance batches completed with exit 0: 311 and
194 tests, 505 total, zero failures and zero ignored. All 79 integration targets
and the library, binary and five examples were covered with ignored cases enabled.
The source fingerprint was verified unchanged before subsequent implementation.
Earlier references to sessions 21627/76629 as running are now superseded.

The Rust 1.99 compatibility patch was validated separately with strict all-target
Clippy, 114 affected existing tests (zero failed/ignored), and the Rust 1.93 minimum
compiler check. All 12 promoted files match those validated sources exactly.
These mechanical slice-array changes preserve the previous remainder handling.

EV's earlier fallback diagnosis is superseded as the immediate startup blocker:
the runtime already implements SysEnvirons, but its trap table omitted $A090.
The original tests GetOSTrapAddress($90) against Unimplemented, then falls back to
the SysMap lookup. Registering the real existing service avoids that fallback;
the isolated original trace now invokes SysEnvirons and advances to missing
trap $AA3D at $0001ACFE, after selecting the current graphics device. Evidence:
/tmp/systemless-ev-sysenv-experiment.txt. No System file was fabricated or aliased
to application resources. Other unadvertised traps require individual review.
A new actual-CPU regression checks PPC/68K lookup identity and calls the returned
68K veneer, verifying the shared profile, register results and restored stack.
Neither original EV nor Marathon has reached verified gameplay. Marathon remains
at HasDepth ($AAA2 selector $0A14). The full rebuild remains active.


The next EV trap $AA3D is ProtectEntry (not GetPixBaseAddr). Apple's Advanced
Color Imaging, Color Manager, “Managing the Device CLUT” explains that protection
prevents SetEntries changes; reservation separately excludes an index from RGB
matching. Both affect device-table invalidation. A coherent next slice must retain
this per-device state and honor it in palette updates/matching; do not insert a
no-op just to pass startup. Primary reference:
https://dev.os9.ca/techpubs/mac/ACI/ACI-107.html


Final combined source passes strict Rust 1.99 all-target Clippy, exit 0:
/tmp/systemless-ev-trap-final-clippy.txt. Sequential formatting checks for the
registration and new regression, and git diff --check, also pass. Focused actual-CPU verification passes 43 cases (6 environment, 33 loading,
4 trap patching), zero failed/ignored, exit 0: /tmp/systemless-ev-trap-focused.txt.
The final-source tracer builds successfully and the real original again reaches
ProtectEntry $AA3D, PC $0001ACFE: /tmp/systemless-ev-trap-main-original.txt.
Its diagnostic exit 1 is the expected explicit unsupported service, not a gameplay
pass. No Monitor Tool error-dialog success is inferred from the earlier path.


## Host-backed display mode queries

HasDepth and SetDepth now decode through the same devices manager on 68K and
PPC. The native PaletteDispatch $AAA2 uses the documented D0 selectors $0A14 /
$0A13, ten argument bytes and a word result. Unsupported selectors fail explicitly;
result permissions are checked before stack adjustment. PPC imports use r3-r6.

The existing host-selected display has one actual backed mode, identity 128,
published in gdMode. HasDepth advertises only that profile's pixel depth and
color format. SetDepth accepts its returned mode ID or its raw depth, preserves
existing storage, and rejects other depths/formats with cDepthErr (-157). This is
not runtime depth switching: different depths, grayscale, direct color and extra
video devices remain unfinished. Copied/offscreen GDevices do not inherit the
main screen's mode capabilities. A record inconsistent with checked live pixels
fails explicitly. No alternate mode is advertised before its storage can exist.

The public-header devices fixture now queries mode identity and unsupported
color depth, accepts current mode ID/raw depth, and verifies failed selection
preserves gdMode. Both original compiler artifacts were regenerated successfully:
/tmp/systemless-display-modes-sdk-build.txt. Primary ABI/semantics references:
https://leopard-adc.pepas.com/documentation/mac/QuickDraw/QuickDraw-295.html
https://leopard-adc.pepas.com/documentation/mac/QuickDraw/QuickDraw-296.html

Strict final-source Rust 1.99 all-target Clippy passes, exit 0:
/tmp/systemless-display-modes-clippy-acceptance.txt. The old checkpoint 110d4178a
has successful GitHub Future runtime CPU-contract jobs; the broader Ubuntu build
was still running at observation (/tmp/systemless-display-mode-pr-before.json).
This does not establish CI success for the unpushed display-mode change.


Actual original Marathon now passes HasDepth(depth=8, whichFlags=1, flags=1) on
its backed 640x480x8 screen, then allocates further regions/handles and reaches
GetNewCWindow $AA46, PC $01007F10. Evidence:
/tmp/systemless-marathon-display-modes-first.txt (explicit missing-service exit 1).
EV's regression preserves ProtectEntry $AA3D at $0001ACFE:
/tmp/systemless-ev-display-modes-regression.txt (also exit 1). Both raw-fork hashes
were rechecked unchanged. Neither trace demonstrates gameplay or native display.
The rebuilt public-header device fixture passes on both CPU engines, with all
five new display calls exercised and ExitToShell(0):
/tmp/systemless-display-modes-sdk-tests.txt, exit 0.

Next: implement resource-backed color-window construction for Marathon using the
shared Window Manager's existing resource ownership and actual CGrafPort graph.
GetNewWindow/from_resource currently creates a basic window; NewCWindow already
owns the color graph. Retain real WIND resource parsing and handle lifetimes,
validate the native/PPC ABI and report unsupported custom color/WDEF features.
EV's ProtectEntry/ReserveEntry/SetEntries slice must retain coherent device/client
ownership, palette updates and matching. Do not silently accept palette calls.
Different-depth switching remains open and requires coherent pixel conversion,
port/device/global records, palette state and redraw. The full rebuild stays active.


Final display-mode acceptance: 18 distinct device/display/mode integration tests
plus the compiler-fixture test pass (19 checks), zero failures/ignored. Actual
GPU cases were explicitly enabled. The final mode-target rerun after applying the
project's explicit-GPU annotation passes all five cases with --include-ignored,
including readback; none skipped. Evidence:
/tmp/systemless-display-modes-tests.txt,
/tmp/systemless-display-modes-final-tests.txt,
/tmp/systemless-display-modes-sdk-tests.txt.
Strict final-source all-target Rust 1.99 Clippy passes, exit 0:
/tmp/systemless-display-modes-clippy-final-acceptance.txt.
Sequential changed-file formatting and git diff --check pass. The earlier
505-test full acceptance remains evidence for its frozen timer checkpoint; no
new full-suite pass is claimed for this mode milestone. Native presentation and
full runtime depth switching are still unverified/unfinished, respectively.


## Resource-backed color windows advance original Marathon

GetNewCWindow ($AA46 / PPC InterfaceLib) now decodes into the same ResourceWindow
request and Window Manager as GetNewWindow. WIND title/bounds/flags/refCon come
from the live loaded resource handle. Matching wctb is loaded through the current
resource chain and parsed as window-part RGB settings, never as a device palette.
Classic GetNewWindow also creates a color port when matching wctb is present,
as specified in the classic CGrafPort reference. The color creation path owns
independent attached records and copies color settings into window-owned state.
It rereads the WIND master after resource allocations rather than retaining a
stale data pointer. Existing windows are unaffected by later template edits.

Content RGB and actual selected index initialize the live CGrafPort; content and
plain/modal frames render through the existing wgpu path and retain their theme
across hide/show and resize. Missing optional colors use the existing host standard
black/white window policy. Duplicate/unknown parts, bad extents and unsupported
custom document title/frame themes fail explicitly before publishing a window.
General custom WDEFs, arbitrary document-title themes, public AuxWinRec access,
SetWinColor/GetAuxWin and broader palette/window-color management remain open.
The implementation does not claim those unimplemented APIs.

The initial test run /tmp/systemless-color-window-tests.txt was deliberately
superseded (exit 130) after the classic color-aware GetNewWindow correction.
It is not an acceptance pass. Final runtime/test acceptance completed with exit 0:
/tmp/systemless-color-window-final-tests.txt passes 20 tests (5 display,
3 resource color-window and 12 window regressions), all GPU cases enabled,
zero failed/ignored. The compiler fixture test also completes with exit 0 on
both real CPU engines: /tmp/systemless-color-window-sdk-test.txt, one test,
zero failed/ignored. Total: 21 distinct focused checks. Both command sessions
85285 and 68552 are terminal; no continuation or restart is needed. Strict final-source Rust 1.99
all-target Clippy passes: /tmp/systemless-color-window-acceptance-lint.txt (exit 0).

Original Marathon now creates and shows both WIND 129 and WIND 128 color windows,
resizes the first to 640x480, and completes HasDepth plus SetDepth(mode 128) on
the existing indexed8 display. Its next missing service is SlotVInstall ($A06F),
PC $01009364, D0 slot 0 and A0 $0001821C. The guest VBLTask header has qType 1,
vblAddr $010093BE and count/phase 0. Verify VBL semantics and actual callback
scheduling rather than fabricating installation. Public SDK Retrace.h confirms
A0/D0 native arguments and D0 OSErr; evidence:
/tmp/systemless-marathon-slot-vbl-sdk.txt. This is not SlotManager ($A06E).
Trace: /tmp/systemless-marathon-color-window-first.txt, exit 1 for the explicit
remaining service. EV still reaches ProtectEntry $AA3D at $0001ACFE:
/tmp/systemless-ev-color-window-regression.txt, also exit 1. No gameplay pass.

Primary references:
https://leopard-adc.pepas.com/documentation/mac/Toolbox/Toolbox-225.html
https://leopard-adc.pepas.com/documentation/mac/QuickDraw/QuickDraw-203.html
SDK window-part IDs/record declarations: /tmp/systemless-color-window-sdk.txt.
The original compiler fixtures were regenerated with synthetic WIND/wctb resources
and real GetNewCWindow calls: /tmp/systemless-color-window-sdk-build.txt, exit 0.


Final sequential changed-file formatting and whitespace checks pass. The worktree
contains only this resource-color-window milestone. No new full-suite result is
claimed; earlier 505-test acceptance belongs to its recorded frozen checkpoint.
Native window/audio and original title/menu/gameplay remain unverified. Keep the
full rebuild active. Next executable work: real per-display VBL task ownership,
queue/count/phase semantics and guarded callbacks for Marathon's SlotVInstall.


## Shared vertical retrace scheduling

The next Marathon requirement now has real VInstall/VRemove, SlotVInstall /
SlotVRemove and GetVBLQHdr behavior shared by both CPU backends. Owned system and
slot-0 queues publish actual headers/linkage and inVBL flags. They use the existing
checked monotonic time counter at the host's virtual 60 Hz, with dormant zero
counts, one-time phase offsets, live countdown edits, rearming and automatic
unlinking. Earlier callbacks can edit or disable later tasks during the same
ordered refresh scan. Runtime guards overlap with Time Manager records and with
both VBL headers/the screen-queue pointer. Record bounds, type/linkage corruption,
invalid slots and attempts to mutate a queue from its callback fail explicitly.

Callbacks execute real guest instructions with A0 on 68K and a validated prepared
PPC VBLUPP ($9802)/r3 on PPC. The shared callback adapter retains Timer Manager A1
and TimerUPP ($B802). Whole CPU state and suspended event/control/menu/file/sound
waits are retained; callback returns restore them. The modeled 68K mask defers
callbacks and VBL entry sets a level-one mask. Timer and VBL callbacks serialize.
A conservative VBL service allowlist rejects allocation/blocking calls. Exit/fault
clears ownership and published queues. Full hardware interrupt/supervisor-stack,
PPC MSR, overrun/coalescing and Mixed Mode behavior remain open; so do additional
slots/rates and AttachVBL/DoVBLTask. No broad VBL Gestalt feature is fabricated.

Original Marathon now installs its slot-0 task and proceeds to GetMaxDevice
($AA27), PC $01034B98, with rectangle pointer $0080FEC8. Diagnostic exit 1 remains
a missing service, not a gameplay pass. The captured task at $0001821C retains
qType 1, callback $010093BE, count 0 and phase 0 at this boundary; completed VBL
callbacks are 0. Fault cleanup changes linkage, not this countdown. Thus original
VBL callback acceptance is still open even though real callback execution is
verified separately by both compiler guests and synthetic CPU tests. Evidence:
/tmp/systemless-marathon-vbl-final.txt; ABI reference:
/tmp/systemless-marathon-getmaxdevice-sdk.txt. EV remains at ProtectEntry $AA3D,
PC $0001ACFE: /tmp/systemless-ev-vbl-final.txt, explicit exit 1.

The initial SDK generation succeeded in /tmp/systemless-vbl-sdk-build.txt. The
final C fixture removed a race-prone exact countdown assertion, was regenerated
alone through the same pinned MPW image, and its staged source was byte-compared
to the repository file: /tmp/systemless-vbl-sdk-final-build.txt, exit 0. Both
SC68K and MrC-PPC programs execute three rearmed slot callbacks, observe the active
queue flag, auto-remove, then install/cancel a phased system task and exit 0.
The older Time Manager compiler guests are also replayed to protect A1 marshalling.
Earlier focused passes before the ordered-scan/header guard fixes are preliminary;
final acceptance is recorded below.

The Super Mines save/open/sound interaction regression completed one sound
callback, reopened its 1,972-byte game and returned to waiting at $0100C798.
Its after.ppm is byte-identical to the inspected timer checkpoint:
/tmp/systemless-mines-vbl-final.txt and /tmp/systemless-mines-vbl-final-after.ppm.
The final-source replay is recorded below. Native presentation/audio and win/loss remain open.

Primary task/callback/queue references:
https://dev.os9.ca/techpubs/mac/pdf/Processes/Vertical_Retrace_Mgr.pdf
https://leopard-adc.pepas.com/technotes/tb/tb_35.html
Pinned SDK evidence: /tmp/systemless-vbl-record-sdk.txt and
/tmp/systemless-marathon-slot-vbl-sdk.txt. Legacy time_manager.rs independently
adds the initial phase to the count; this is reference evidence, not a native
conformance claim. Original raw-fork hashes were rechecked unchanged.


Final VBL acceptance (2026-10-16): 108 distinct checks passed, zero failed and
zero ignored. The 106 library/CPU/service checks are in
/tmp/systemless-vbl-final-word-acceptance.txt; real compiler VBL and Timer guests
are in /tmp/systemless-vbl-word-sdk.txt and
/tmp/systemless-vbl-word-timer-sdk.txt. Both compiler tests exercise both CPUs;
the VBL programs each complete three callbacks. Counts and initial phase use
16-bit wrapping words, including the $7FFF + 1 boundary regression. Strict
Rust 1.99 all-target Clippy passes (/tmp/systemless-vbl-word-acceptance-lint.txt),
as does the Rust 1.93 library check (/tmp/systemless-vbl-msrv-check.txt).
Sequential changed-file formatting and whitespace checks pass. This is focused
acceptance, not a rerun of the earlier 505-test full-suite checkpoint.

Final original replays: Marathon /tmp/systemless-marathon-vbl-word-acceptance.txt
exits 1 at GetMaxDevice, with zero completed VBL callbacks and a dormant task;
EV /tmp/systemless-ev-vbl-word-acceptance.txt exits 1 at ProtectEntry.
Super Mines /tmp/systemless-mines-vbl-word-acceptance.txt exits 0, completes one
sound callback, reopens the 1,972-byte game and waits at $0100C798. Its final
/tmp/systemless-mines-vbl-word-acceptance-after.ppm is byte-identical to
/tmp/systemless-mines-timers-final-after.ppm. These diagnostics do not establish
native presentation, audible output, original-game gameplay or win/loss behavior.
Final non-Markdown source fingerprint:
77ce05705cc52b7e1deac7db7a9f06a6b21cb5a177ead9c1b4d790e16025c275.
Next: implement shared GetMaxDevice with documented display selection semantics,
continue Marathon startup, then implement EV palette ownership/protection. Keep
the full rebuild goal active and the PR in draft.


## GetMaxDevice checkpoint (2026-10-16)

Shared GetMaxDevice ($AA27/PPC import) now returns the one registered host display
when its bounds intersect the global rectangle by positive area, otherwise nil.
It validates input before lazy allocation, preserves selected offscreen device
state and rejects an unregistered display list. Multiple host displays and depth
ranking among them remain unfinished. Apple's QuickDraw-288 and QD13 specify
greatest depth, not intersection area; sources are in architecture.md.

Nine device integration checks pass with zero failed/ignored, including both CPU
ABIs, empty/inverted/disjoint/one-pixel rectangles, offscreen selection retention,
and the real GPU CopyBits/readback regression. Evidence:
/tmp/systemless-maxdevice-tests.txt, terminal exit 0. Strict Rust 1.99 all-target
Clippy passes: /tmp/systemless-maxdevice-lint.txt, exit 0. Changed-file sequential
formatting and whitespace checks pass. No full-suite rerun is claimed.

Original Marathon passes GetMaxDevice, allocates 121,856-byte drawing storage and
a 122-byte port, then stops at NewGDevice ($AA2F), PC $01034C08. Evidence:
/tmp/systemless-marathon-maxdevice.txt, diagnostic exit 1. Its VBL task remains
dormant and no callbacks complete before this boundary. EV's previously verified
ProtectEntry boundary is unchanged by scope; it was not replayed this checkpoint.
Neither original game is playable. Next: shared owned NewGDevice/DisposeGDevice
lifecycle with correct detached offscreen records and public ABI fixtures, then
continue Marathon startup. Keep the full rebuild active.


## Detached graphics-device checkpoint (2026-10-16)

Both CPU adapters now share real no-driver NewGDevice(mode -1) and owned
DisposeGDevice/DisposGDevice. Four independent handles retain a detached GDevice,
PixMap, empty ColorTable and inverse table. Allocation rollback returns nil and
MemError -108 without leaking storage; disposal frees the complete original graph.
Main/foreign/current-device disposal and replaced supporting handles fail
explicitly. Driver-backed modes, handle replacement/transfer and separate system
heap allocation remain unfinished; no screen clone or false driver support is used.

Final acceptance: 11 device integration checks (including real GPU readback) plus
the updated pinned-SDK compiler fixture on both CPU engines pass: 12 distinct
checks, zero failed/ignored. Logs: /tmp/systemless-newdevice-tests.txt and
/tmp/systemless-newdevice-sdk-test.txt. Pinned compilation:
/tmp/systemless-newdevice-sdk-build.txt. Strict final all-target Rust 1.99 lint
passes (/tmp/systemless-newdevice-final-lint.txt); sequential changed-file format
and whitespace checks pass. No full-suite or new minimum-version run is claimed.

Original Marathon passes NewGDevice(0,-1), fills its offscreen records, then
requests DisposeCTable ($AA24), PC $01034C96, argument $00010C30. Diagnostic exit 1:
/tmp/systemless-marathon-newdevice.txt. Trap identity verified against the pinned
SDK: /tmp/systemless-marathon-aa24-sdk.txt. No completed sound/timer/VBL callbacks
precede this boundary. EV was not replayed; its previous ProtectEntry boundary is
still the latest evidence. Neither original game is playable. Next executable
slice: real ColorTable disposal/replacement ownership and subsequent Marathon
setup, preserving main palettes and detached device cleanup. Full rebuild stays
active; draft PR and no merge/release/deployment restrictions remain in force.


## ColorTable replacement checkpoint (2026-10-16)

DisposeCTable ($AA24/PPC import) now frees a checked owned ColorTable and forgets
its detached-device ownership before the master pointer can be reused. It leaves
the caller's PixMap field unchanged; the caller installs its replacement.
DisposeGDevice can then free an independent live replacement, or a nil table,
while rejecting main display storage, structural handles and replacement tables
shared with another detached device. Replacement of PixMap/inverse handles and
general borrowed-table lifecycle remain unfinished. Main palettes stay protected.

Final acceptance: 13 device tests plus one pinned-SDK compiler-fixture test running
both CPUs pass, zero failed/ignored. GPU CopyBits/readback executed. Logs:
/tmp/systemless-disposetable-final-tests.txt and
/tmp/systemless-disposetable-final-sdk-test.txt (its filtered device target runs
zero tests; only the one MPW result is counted). The earlier 12-test run predates
final guards and is preliminary. Compiler build:
/tmp/systemless-disposetable-sdk-build.txt, no compiler/linker errors.
Strict Rust 1.99 all-target lint passes:
/tmp/systemless-disposetable-acceptance-lint.txt. Sequential changed-file formatting
and whitespace checks pass. No full-suite or minimum-version rerun is claimed.

Original Marathon now disposes the initial table, duplicates its chosen table,
builds a 3-bit inverse table and selects the offscreen device. It then faults in
ColorPort Open at PC $01034CEE with Memory read address 0. This is a runtime fault,
not another missing trap or a gameplay pass. Evidence:
/tmp/systemless-marathon-disposetable.txt, exit 1. The current PixMap has a zero
baseAddr during setup; investigate color-port initialization's use of pixel-storage
validation for default color metadata. No callbacks complete before this boundary.
EV was not replayed and remains at its previously observed ProtectEntry boundary.
Neither original is playable. Full rebuild stays active; next work is the actual
color-port fault and subsequent Marathon startup.


## Color-port metadata checkpoint (2026-10-16)

Fixed the original Marathon ColorPort Open read-at-zero fault. Default color
selection only needs PixMap geometry/format and palette metadata; indexed parsing
no longer reads its pixel buffer. Actual source/destination drawing still unpacks
checked storage and rejects a missing buffer. New tests exercise initialization
before offscreen buffer attachment through both CPUs and explicitly assert drawing
still fails with nil storage.

Final acceptance: 63 checks pass, zero failed/ignored: 38 library, five color-port,
13 device and seven indexed-destination tests. GPU cases were enabled and executed.
Logs: /tmp/systemless-port-metadata-tests.txt and
/tmp/systemless-port-metadata-drawing-tests.txt, both terminal exit 0. Strict
Rust 1.99 all-target lint passes (/tmp/systemless-port-metadata-lint.txt).
Sequential changed-file formatting and whitespace checks pass. This is focused
acceptance, not a full-suite or new minimum-version result.

Original Marathon passes color-port setup and completes its first real Timer
Manager callback (completed timer=1, sound=0, VBL=0). It then requests Elems68K /
Pack5 ($A9EC), PC $0103CE22, selector $001E with extended operand at $0080FDBA.
The pinned SDK Traps.h confirms the package identity:
/tmp/systemless-marathon-a9ec-sdk.txt. Evidence:
/tmp/systemless-marathon-port-metadata.txt, explicit missing-service exit 1.
No title/menu/gameplay or audible-output acceptance is inferred. EV was not
replayed and remains at its previously verified ProtectEntry boundary. Neither
original game is playable. Next: document and implement the actual SANE elementary
operation, numeric representation/exception behavior and ABI, then continue
Marathon. Keep the full rebuild active and PR in draft.


## Extended arctangent checkpoint (2026-10-16)

Implemented shared default-nearest SANE FATANX ($A9EC, selector $001E) with the
pinned astro-float-num 0.3.7 pure-Rust engine. Native ABI marshals a ten-byte
extended destination, preflights memory and pops six bytes. Numeric evaluation
preserves all 64 significand bits; signed zero, infinity, NaN payload quieting and
subnormal inputs are explicit. Process sticky status is retained, but public SANE
environment/alternate rounding/precision/halt handlers, other selectors, FP68K
and PPC math imports remain unfinished. Noncanonical operands fault explicitly.
No complete SANE capability or CPU math parity is advertised.

Final acceptance: 52 CPU/service checks pass, zero failed/ignored (39 library,
two numeric integration, seven timer and four trap patch tests).
/tmp/systemless-sane-acceptance.txt, terminal exit 0. The new numeric check proves
atan(1) and atan(1 + 2^-63) differ in the correct lowest output bit. Signed and
special cases, native register/frame preservation, bad destinations and unsupported
selectors are tested. Strict Rust 1.99 all-target lint passes:
/tmp/systemless-sane-lint-final.txt. Rust 1.93 library check passes:
/tmp/systemless-sane-msrv.txt. Sequential changed-file formatting and whitespace
checks pass. Initial compilation/lint errors were corrected and superseded; they
are not acceptance results. No full-suite, GPU suite or wasm build is claimed.

Original Marathon consumes the extended pi/4 arctangent and reaches FP68K /
Pack4 ($A9EB), PC $0101DC00, selector $0004 (FMULX), destination $0080FE10,
source $0080FE1A. Pinned SANEMacs.a confirms FATANX and FMULX:
/tmp/systemless-sane-sdk-atan.txt and /tmp/systemless-sane-sdk-multiply.txt.
Evidence: /tmp/systemless-marathon-sane-atan.txt, missing-service exit 1;
completed timer=1, sound=0, VBL=0. Native title/menu/gameplay and audible output
remain unverified. EV was not replayed and its last boundary is ProtectEntry.
Neither original is playable. Next: shared FP68K extended arithmetic and conversion
with correct rounding/range/exception state, then continue Marathon startup.
Full rebuild remains active; PR stays draft.
