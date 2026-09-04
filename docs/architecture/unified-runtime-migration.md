# Unified Runtime Migration Contract

This document is the implementation contract for making 68K and PowerPC two
execution engines inside one Macintosh process. It is deliberately stricter
than a parity promise: equal results produced by duplicate implementations do
not count as unification.

The migration is complete only when each process-visible fact has one
authoritative owner, every ABI path resolves the same semantic operation, and
cross-ISA work is scheduled by a task-aware execution layer rather than by the
foreground CPU adapter.

## Architectural laws

1. Guest bytes have one owner. A store made through either CPU adapter is
   immediately visible through the other adapter and through the HLE.
2. Each process-visible fact has exactly one authoritative owner. Cached
   projections are allowed only when they are derived, invalidated, and tested.
3. ABI adapters decode and encode calls. They do not implement manager policy.
4. A semantic operation is implemented once and can request guest execution
   without knowing which CPU was active when the operation began.
5. Continuations belong to an execution task. Cooperative thread switches do
   not expose or consume another task's suspended calls.
6. Host integrations observe semantic effects. They do not choose guest
   behavior from the active ISA.
7. Compatibility during migration is explicit and temporary. A compatibility
   projection must name its authority, update boundary, and deletion gate.

## Target ownership

| Concern | Authority | 68K edge | PowerPC edge | Compatibility debt to delete |
| --- | --- | --- | --- | --- |
| Guest bytes and mappings | `GuestAddressSpace` | `MemoryBus` adapter | `PpcMemory` adapter | Flat-RAM/foreign-sparse routing split |
| Handles, pointers, zones | Process Memory Manager | Trap ABI | InterfaceLib ABI | Adapter-owned mirrors and adoption paths |
| Trap vectors and protected chains | Guest trap-table bytes plus `TrapManager` policy | A-line/Trap Manager ABI | InterfaceLib ABI | Pre-materialization snapshots and duplicate helpers |
| TickCount | Low-memory `Ticks` bytes | `_TickCount` stack result | `TickCount`/`LMGetTicks` return | Adapter `tick_count` and projection fields |
| Events and input | Process Event/Input services | Event Manager traps | InterfaceLib ABI | Per-adapter queue/input projections |
| Menus, windows, controls, dialogs | Process manager services | Toolbox traps | InterfaceLib ABI | `PpcToolboxStartupState` manager copies |
| QuickDraw | Process QuickDraw service plus guest structures | Toolbox traps | InterfaceLib ABI | Adapter color/port/pixel projections |
| Files and resources | Process File/Resource services | OS/Toolbox traps | InterfaceLib ABI | Duplicate loader/import dispatch policy |
| Sound and scheduled callbacks | Process Sound/Scheduler services | traps and interrupt gateways | InterfaceLib ABI | runner-owned callback stacks/trampolines |
| Mixed Mode calls | Task-aware execution kernel | 68K call adapter | PPC call adapter | runner orchestration and direct `CallNative` sites |
| Presentation | Host observer of semantic effects | none | none | active-ISA presentation branches |

`ProcessContext` is the composition root for these authorities. It must become
an explicit set of services rather than a bag of shared handles that are
attached into several large owners.

## Owner migration ledger

### `FixtureRunner`

| Current field family | Target | Migration rule |
| --- | --- | --- |
| `cpu`, `ppc_app`, `ppc_companion`, `parked_m68k_cpus` | `ExecutionKernel` and task records | Move scheduling and parked contexts together; the runner asks the kernel to advance work. |
| `bus` | Process address-space service | Keep one public runner view, but remove foreign-memory routing once both CPU adapters use the same router. |
| `dispatcher`, `process_context` | `MacintoshProcess` composition root | The runner owns one process, not a dispatcher plus an attachment bag. |
| tick budget, idle detection, deterministic launch overrides | Process clock and execution policy | Separate guest clock authority from host pacing and optimization. |
| callback trampolines, active callback, deferred refires | Scheduler/execution effects | Allocate gateways through one service; queue guest calls as effects. |
| audio, framebuffer mirrors, GPU frame | Host presentation | Consume process effects and guest surfaces; never select semantics by ISA. |
| diagnostics and halt snapshots | Execution observer | Observe kernel/task outcomes without owning guest state. |

### `ProcessContext`

| Current field family | Target | Migration rule |
| --- | --- | --- |
| `memory`, `memory_manager` | Address-space and Memory Manager services | Preserve one byte authority and one allocator authority. |
| `tick_state` | Process clock | Low-memory `Ticks` is authoritative; pacing metadata is not guest state. |
| queues, input, menu tracking, window list | Event/Menu/Window services | Expose semantic methods rather than shared-field attachment. |
| `guest_calls`, `mixed_mode_m68k`, callback scheduling | Execution kernel | Index all suspended execution by task. |
| files, Apple events, sound, timers, VBL tasks | Manager services | Give each service explicit process scope and effect outputs. |
| QuickDraw/color/cursor/dialog/list/control/text state | Manager services | Group by classic manager and remove adapter mirrors after paired-ISA tests exist. |

### `TrapDispatcher`

`TrapDispatcher` is a 68K ABI adapter plus a temporary compatibility owner. Its
trap decoding, register/stack marshaling, and 68K gateway mechanics remain at
the edge. Manager collections, queues, caches, process policy, and callback
state move behind process services. Each move must replace direct field access
with a semantic method before the field is removed.

### `PpcLoadedApp` and `PpcToolboxStartupState`

`PpcLoadedApp` retains PPC registers, loader/CFM metadata, import bindings,
section layout, and PPC-only ABI state. Macintosh managers, clocks, queues,
files, sound, QuickDraw, callbacks, and Toolbox startup state move to the
process. `PpcToolboxStartupState` is deleted when its last compatibility field
has moved; it is not the target home for new behavior.

## Semantic operation ledger

Every migrated operation records one decode path per ABI, one shared semantic
implementation, its authoritative state, and its effects. The first five
walking slices cover the architecture's hardest boundaries.

| Operation | 68K decode/encode | PPC decode/encode | Shared authority | Effect | Deletion gate |
| --- | --- | --- | --- | --- | --- |
| `TickCount` / `LMGetTicks` | `_TickCount` writes the Pascal result slot | InterfaceLib returns in `r3` | low-memory `Ticks` | none | remove both adapters' tick mirrors and projection methods |
| `NewHandle` | Memory Manager trap arguments/result | InterfaceLib arguments/result | process Memory Manager plus guest master pointer | allocation/error | remove duplicate native allocator dispatch policy |
| `NGetTrapAddress` / `NSetTrapAddress` | Trap Manager register ABI | InterfaceLib ABI | raw guest trap-table bytes and `TrapManager` | protected write or system error | remove snapshots and PPC-only helpers |
| `MenuSelect` plus custom MDEF | Toolbox stack/register ABI | InterfaceLib ABI | process Menu Manager | `CallGuest`, menu change, host menu effect | remove runner/loader callback state and active-ISA branches |
| `CopyBits` | QuickDraw trap ABI | InterfaceLib ABI | process QuickDraw and guest pixels | dirty-region/presentation effect | remove duplicate raster policy and adapter pixel projections |

Later waves use the same ledger schema for Events/Input, Windows/Controls,
Dialogs/TextEdit, Files/Resources, Sound, Time/VBL, AppleEvents, and QD3D.

### Current slice status

| Slice | Shared proof already present | Remaining deletion gate |
| --- | --- | --- |
| TickCount | low-memory `Ticks` is the only scalar authority; both ABI paths observe direct guest stores, and multi-VBL native slices preserve callback chronology | complete: the adapter scalar/projection fields have been deleted |
| NewHandle | all ordinary current/system and clear/non-clear 68K variants plus native `NewHandle` imports decode to one process Memory Manager request/result operation; paired tests compare error, size, initial state, and clear policy without requiring identical addresses | complete for ordinary `NewHandle`; `TempNewHandle` remains a deliberately separate operation and the physical classic/native allocation backends retain their compatible layouts |
| Trap Manager | one `TrapManager` resolves raw cells for both ABIs; protected come-from chains require registered read-only system provenance and reject guest-data signature spoofing | remove the pre-materialization table snapshot and route full-runner mixed-ISA coverage through the service |
| MenuSelect/MDEF | a native menu operation can run a 68K MDEF and observe its mutation | replace bespoke menu callback/trampoline state with a resumable process effect |
| CopyBits | both implementations operate on shared pixels and have broad format coverage | extract one QuickDraw transfer operation; the current 68K and PPC algorithms are still separate implementations |

The address-space prerequisite is also active: scalar and bulk accesses from
both CPU bus contracts use one precedence router across shared, read-only,
sparse, flat, mixed, and unmapped ranges. Bulk writes preflight the complete
destination before mutating bytes, and overlapping copies retain memmove
semantics. The compatibility flat/sparse storage backends remain deliberately
different; routing authority, not storage shape, is what is unified.

The guest-call boundary now exposes architecture-neutral requests,
continuations, return policies, and effects. Direct `CallNative` construction
outside that boundary is guarded at zero. A CPU-free continuation store owns
monotonic call identity, task-local LIFO order, and pending/active/completed
phases, and rejects mismatched or out-of-order transitions transactionally.
`SharedGuestCallStack` remains a compatibility facade whose concrete CPU frame
projection is keyed by `CallId`; the runner still owns parked 68K contexts.
Result completion validates the exact void/value ABI and applies retryable
memory results before retiring the continuation or consuming a parked context.
Moving both concrete context banks and scheduling behind the execution kernel,
then removing current-task stamping at the compatibility boundary, are the next
deletion gates.

## Audit gap matrix

This snapshot separates completed architectural proof from remaining migration
work. A proven entry means the authority or invariant exists; it does not imply
that the broader manager family has already moved.

| Area | Proven now | Remaining gap | Next bounded change |
| --- | --- | --- | --- |
| Address space | one precedence router covers shared, read-only, sparse, flat, mixed, and unmapped bytes for both CPU contracts; scalar and bulk writes preflight atomically | flat RAM and native sparse regions remain different storage backends | keep the router as authority; remove storage-specific queries from manager code as each caller migrates |
| Clock | low-memory `Ticks` is canonical and native callback replay preserves every elapsed tick | host pacing still lives in the runner | move pacing metadata behind a process clock without adding another guest tick scalar |
| Memory Manager | ordinary current/system and clear/non-clear `NewHandle` variants share one request/result policy | other handle/pointer/zone operations and `TempNewHandle` have not yet been sliced | add one ledger row and paired-ABI proof per operation; preserve distinct physical layouts only where guest-observable |
| Trap Manager | both ABIs read raw guest table bytes and use explicit read-only system provenance for privileged chain writes | the pre-materialization compatibility map still exists | initialize raw tables on every launch path, prove startup equivalence, then delete the map and test-only signature compatibility helpers |
| Execution | CPU-free call IDs, task-local LIFO phases, exact result ABI validation, and retryable completion are enforced | concrete parked CPU contexts and runnable engine slots remain runner-owned | move context banks into kernel task records and replace depth inference with `(task, CallId)` lookup |
| Menus and callbacks | shared menu state and a native-to-68K MDEF proof exist | `MenuSelect` continuation, scratch storage, and callback gateways remain adapter/runner-specific | execute M2 as one typed resumable Menu Manager operation |
| QuickDraw | both ABIs see the same guest pixels and color state | `CopyBits` raster policy is duplicated | execute M3 format-family by format-family with differential tests and deletion after each family |
| Remaining managers | many process handles are already attached through `ProcessContext` | attachment is not ownership; adapter fields and policy branches remain across Event, Window, File, Resource, Sound, AppleEvent, and QD3D code | migrate in the M4 dependency order and lower a guardrail in every state-removal patch |
| Composition/host | presentation can consume shared framebuffer and menu state | runner and adapters still own manager-specific scheduling and host-facing mirrors | complete M5 only after manager authorities and effects are stable |

### Hardened transition invariants

- A mixed wide write or bulk copy either commits every destination byte or no
  destination byte, including read-only sparse overlays and protected code.
- A permanent trap chain requires both the come-from signature and registered
  system-owned read-only provenance; this is enforced at both ABI edges.
- A void callback preserves native `r3`; a value-returning callback cannot
  complete without its value.
- Failed cross-ISA result placement retains the completed continuation and its
  parked caller context for retry.
- Advancing the process clock during a 68K callback services due native tick
  callbacks only while the same execution task still owns the phase.

This table must describe code, not aspiration. Update a row only when its proof
and deletion gate change in the same patch.

## Shared operation and effect boundary

A manager operation returns semantic completion or a typed effect. The initial
effect vocabulary is intentionally small:

```text
Complete(result)
CallGuest { task, target, proc_info, arguments, continuation }
ScheduleCallback { task, deadline, target, arguments }
RaiseSystemError(code)
Present(change)
```

The execution kernel resolves `CallGuest` through the architecture-neutral
procedure resolver, runs the selected CPU adapter, and resumes the same manager
continuation on the same task. Managers never return a PPC import action or
manipulate a 68K program counter directly.

## Migration sequence

1. Keep the shape guardrail green and lower its baselines only when debt is
   removed. New fields or direct native-call sites are regressions.
2. Finish the five walking slices end to end, including paired-ISA and nested
   cross-ISA tests, before moving broad manager families.
3. Introduce the task-aware execution kernel and typed effect boundary; move
   callback scheduling and parked CPU contexts behind it.
4. Replace the flat-RAM/foreign-sparse overlay with one address router that
   implements both CPU bus contracts.
5. Move manager state in dependency order: Memory/Trap/Clock, Event/Input,
   Menu/Window/Control/Dialog/TextEdit, QuickDraw, File/Resource, Sound/Timers,
   then AppleEvents/QD3D.
6. Reduce `TrapDispatcher` and `PpcLoadedApp` to ABI/CPU edges, delete
   `PpcToolboxStartupState`, and replace `ProcessContext` attachments with an
   explicit `MacintoshProcess` service graph.
7. Convert host integrations to semantic-effect consumers and remove every
   branch that selects guest behavior from the active ISA.

## Executable milestones

The sequence above is delivered as bounded milestones. A milestone is complete
only when its old owner is deleted or is named here as a checked compatibility
projection. Moving code without closing the deletion gate does not advance the
migration.

### M0 — Foundations and walking slices

Status: active; address routing, TickCount, ordinary `NewHandle`, and the shared
Trap Manager path are implemented.

1. Keep one precedence router for both CPU memory contracts. Every scalar,
   range, and bulk mutation preflights its complete destination, including
   mixed shared/sparse/flat spans and protected trap-chain writes.
2. Keep low-memory `Ticks` as the only guest clock value. Host pacing may retain
   an observed epoch, but it cannot publish a stale value over a direct guest
   store.
3. Keep ordinary `NewHandle` policy in the process Memory Manager. The classic
   and native physical allocators may retain their observable layout; signed
   size validation, clear policy, initial state, error, reverse index, and
   commit/rollback rules belong to the shared operation. `TempNewHandle` is a
   separate ledger entry.
4. Finish the Trap Manager deletion gate by removing the pre-materialization
   mirror once every launch path initializes guest table bytes. Protected
   come-from writes must use explicit system provenance, not a signature that
   writable application memory can spoof.

Exit proof: focused atomicity tests, paired ABI tests, raw guest mutation in
both directions, full library tests, and no increase in any guardrail count.

### M1 — Execution kernel owns identity and contexts

Status: call identity/order/phase is implemented; concrete context ownership is
next.

1. Move neutral request, continuation, result, and task-effect value objects
   from `guest_call` into `execution_kernel`; move PPC action conversion into a
   PPC edge adapter.
2. Replace runner depth inference with `(ExecutionTaskId, CallId)` keys. Move
   `ParkedM68kContexts`, parked PPC callers, and cooperative task snapshots into
   kernel-owned context banks.
3. Make activation and completion two-phase: validate and preflight result
   placement first, then atomically commit the continuation phase and concrete
   context move. A failed memory result or ABI adaptation remains retryable.
4. Make the kernel current-task cursor authoritative. Thread Manager operations
   emit switch/yield/retire effects; dispatcher task fields remain checked
   mirrors only until their call sites migrate.
5. Move native application/companion execution slots behind bounded engine
   leases. The runner asks which task/engine is runnable and retains only host
   pacing, loading, presentation, and diagnostics.

Exit proof: no concrete CPU/import-action references in `execution_kernel`, no
depth-based parked-context lookup, task retirement accounts for both live calls
and contexts, failed transitions are transactional, and nested callbacks pass
in both directions across a cooperative task switch.

### M2 — Menu Manager as a resumable service

Status: shared state and a cross-ISA MDEF proof exist; callback orchestration is
still adapter-owned.

1. Define `MenuSelectRequest`, `MenuSelectResult`, and a process-owned tracking
   continuation containing only Macintosh menu concepts and presentation
   changes.
2. Decode 68K `_MenuSelect` and the InterfaceLib import into that operation.
   Replace direct PC/stack mutation and PPC-specific actions with
   `GuestCallEffect::CallGuest` carrying a typed MDEF continuation.
3. Move MDEF argument construction and scratch ownership behind the Menu
   Manager. Keep only ABI frame encoding in the 68K/PPC adapters.
4. Resume the same operation by `(task, CallId)` after every draw/choose/size
   callback. A task switch suspends tracking without exposing it to another
   task.
5. Emit semantic menu/presentation changes to the host and delete native menu
   selection mirrors, manager-specific callback stacks, and menu trampolines
   outside the execution gateway service.

Exit proof: native→68K and 68K→native MDEF fixtures mutate the same live menu,
nested Menu Manager calls preserve task ownership, cancellation and no-hit paths
are transactional, and host presentation does not branch on the active ISA.

### M3 — One QuickDraw transfer engine

Status: both ABI paths share pixels and color state, but their `CopyBits`
algorithms remain separate.

1. Introduce neutral `CopyBitsRequest`/`CopyBitsResult` values for bitmap
   descriptors, rectangles, transfer mode, mask region, clip, and destination
   damage. Decode guest structures once at each ABI edge.
2. Extract a memory/manager-neutral raster kernel over explicit source and
   destination access traits. Port one format/mode family at a time, beginning
   with identity 1/8-bpp `srcCopy`, then scaling/packing, masks, boolean modes,
   palette translation, and direct color.
3. Run the existing classic and native implementations as differential oracles
   during each port. Once a family is equivalent, route both ABIs to the shared
   kernel and delete both superseded branches for that family.
4. Return a destination damage effect; screen refresh, window snapshots, and
   host composition consume that effect after guest pixels are committed.

Exit proof: paired-ISA tests cover every supported depth/mode family, overlap
uses a source snapshot, clips/masks and palette identity agree, sentinel or
unmapped addresses are inert, and only one raster policy implementation remains.

### M4 — Manager dependency waves

Move state only after its dependencies are process-owned:

1. Event/Input, then Menu/Window/Control/Dialog/TextEdit.
2. QuickDraw/color/device state.
3. File/Resource and Standard File.
4. Sound/Time/VBL and callback scheduling.
5. AppleEvents and QD3D.

For each operation, add one neutral request/result/effect boundary, paired ABI
proofs, a mixed-ISA visibility proof, and an explicit deletion of adapter fields
or synchronization paths. Lower the corresponding guardrail baseline in the
same patch.

### M5 — Composition root and host boundary

1. Replace attachment-style `ProcessContext` plumbing with a private
   `MacintoshProcess` service graph that owns the address space, managers,
   execution kernel, tasks, and guest clock.
2. Reduce `TrapDispatcher` to the 68K trap ABI/gateway edge and
   `PpcLoadedApp` to PPC/CFM/loader/ABI state; delete
   `PpcToolboxStartupState` after its final manager projection moves.
3. Publish immutable snapshots and semantic presentation effects to desktop and
   web frontends. Host code may pace, render, mix, persist, and diagnose, but it
   cannot choose guest semantics from the active ISA.

Exit proof: adapter/runner manager-state and trampoline guardrails reach zero,
the runner does not orchestrate individual guest callbacks, and deterministic
68K/PPC showcase and play/oracle checkpoints agree.

## Required verification

- Focused unit tests for each semantic operation and compatibility projection.
- Paired-ISA tests that perform the same operation through both ABIs and compare
  authoritative guest state, not just return values.
- Mixed-ISA tests where one architecture mutates state and the other observes
  it immediately.
- Nested callback tests in both directions and across cooperative task switches.
- Full library tests with default features disabled and with test support.
- The fat Toolbox showcase and deterministic play/oracle checkpoints for both
  architecture slices.
- Guardrail counts reach zero for manager state, callback trampolines outside
  the execution layer, and direct native-call actions outside the kernel.

## Completion definition

The migration is complete when no process-visible manager state is owned by a
CPU adapter, the runner does not orchestrate individual cross-ISA callbacks,
both CPUs execute against one address router, every callback belongs to a task,
and the public host boundary consumes the same semantic effects regardless of
which ISA produced them.
