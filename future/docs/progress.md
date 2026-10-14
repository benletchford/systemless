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
