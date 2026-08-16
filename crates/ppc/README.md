# ppc

From-scratch 32-bit PowerPC user-mode interpreter — decoder, register
state, dispatch loop, memory-bus trait. Architecture-only: no Mac
specifics, no PEF, no Toolbox. Consumers (e.g. systemless) wire in
their own loader and HLE dispatcher on top.

Mirrors the shape of [`m68k`](https://crates.io/crates/m68k) for the
classic-Macintosh emulator family: a thin, stateless ISA interpreter
that a host crate composes with platform-specific glue (memory map,
ROM/firmware emulation, syscall/trap dispatch).

## What's covered

- **PpcCpu** — full 32-bit PowerPC architectural state: `gpr[32]`,
  `fpr[32]`, `cr`, `lr`, `ctr`, `xer`, `fpscr`, `msr`, `pc`, plus
  the single-core load/store reservation used by `lwarx` / `stwcx.`.
  MSR defaults to Classic Mac user-mode assumptions: big-endian
  execution and floating-point available.
- **Decoder** (`decode`) — ~120 mnemonics across the integer ISA
  (every ALU, logical, shift/rotate, memory family including byte-
  reversed, string, cache-block, and reservation forms, branch,
  multiply, divide, sign-extension, cntlzw, carry-extend, CR-logical,
  SPR moves, CR moves, memory/cache barriers and hints) plus the
  everyday IEEE-754 floating-point surface (add/sub/mul/div both
  precisions, fused multiply-add family, sqrt, reciprocal estimates,
  sign manipulation,
  comparison, integer↔float conversion, round-to-single, fsel).
- **Run loop** — `PpcCpu::run` fetches big-endian words, decodes
  via [`decode`], dispatches via [`PpcCpu::step`], with clean
  `CycleLimit` / `Halted` / `Unimplemented` / `MemoryFault` /
  `Exception` / `FetchFault` exits.
- **Architected exceptions** — program traps (`tw`/`twi`), system calls
  (`sc`), strict alignment faults, and floating-point-unavailable
  faults, plus reserved-opcode / invalid-form illegal instructions,
  surface as structured `PpcException` values without advancing the
  faulting PC.
- **Fetch observers** — `run_with_fetch_observer` and
  `run_with_imports_and_fetch_observer` report successful `(pc, word)`
  fetches to a caller-provided observer. `PpcFetchHistogram` records
  primary, secondary, and raw-word counts for long reachable-code
  diagnostics without retaining every PC, and can summarize which
  fetched words are still unsupported by the decoder.
- **PpcMemory trait** — byte-granular reads / writes plus default
  big-endian convenience methods at u16 / u32 / u64 granularity.
  Implementors return `None` on unmapped accesses; the dispatcher
  surfaces those as `MemoryFault` step results.
- **PpcSectionMem** — multi-region in-memory bus implementation,
  ready-made for hosts that need to map code / data / stack /
  synthetic regions at arbitrary base addresses with read-only and
  read/write attributes.
- **Import-trap helpers** — `run_with_imports` and
  `run_with_import_trace` route fetches that fall in a caller-defined
  PC range to a handler closure or a trace `Vec`, so HLEs can
  bind imported library calls to host-side dispatchers. Handlers can
  return, preserve GPR3, enter guest PPC callbacks, halt, or raise a
  structured host-import exception.

## What's NOT covered

- **PEF**, **CFM**, **Mach-O**, or any other container format —
  consumer crates parse those.
- **Memory Manager / QuickDraw / Window Manager / etc.** —
  HLE-specific, lives in the consumer.
- **Supervisor mode** and MMU translation — this is a user-mode
  interpreter sufficient for running classic Mac PowerPC application
  binaries through their CFM trampolines. Only the minimal MSR state
  needed for user-mode byte order and FP-availability checks is modeled.

References: *PowerPC User Instruction Set Architecture, Book I,
Version 2.01*. Encoding citations are inline at every dispatch
site in `decode.rs`.
