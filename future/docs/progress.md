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

1. Add a checked resource-fork parser with bounds/overlap/type/id tests. Read
   baseline loader/resource parsing as evidence and verify on-disk layouts
   against primary Apple documentation. Return unsupported/malformed errors.
2. Add minimal CODE 0 jump-table/segment loading and PEF section, relocation,
   import and transition-vector loading into the same Memory. Use the ppc
   crate's focused PEF parser only after checking its API and semantics.
3. Introduce canonical process state: shared heap/handles with guest master
   pointers, resource identity, VFS forks and deterministic event queue. Keep
   host paths/input policy out of ABI handlers.
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
