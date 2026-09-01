//! Idle cycle detection, probing, and execution acceleration.

use crate::trap::TrapDispatcher;

// Cap how many ticks the fast-forward will advance in one shot,
// to protect against pathological target values (e.g. overflowed
// unsigned register values being misinterpreted as huge-future
// ticks). If the cap trips, we fall back to normal spin — still
// correct, just not fast.
pub(crate) const SPIN_FASTFWD_MAX_TICKS: u32 = 1_000_000;

/// Outcome of `advance_until_tick`. Used to distinguish the "we
/// advanced, please synthesise the exit state" happy path from
/// the abort paths: tick_cap reached (caller must break the
/// outer run loop), pathological target difference (caller must
/// NOT synthesise — let the guest spin normally), and interrupt
/// callback injection (caller must leave the CPU at the callback
/// trampoline).
pub(crate) enum AdvanceResult {
    Advanced,
    CapHit,
    Interrupted,
    TooFar,
}

/// Processor state that can affect guest execution at a candidate idle-cycle
/// boundary. JIT/decode caches and remaining host batch cycles are deliberately
/// excluded, while precise prefetch and loop-mode state remain part of the
/// proof.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct CpuArchitecturalSnapshot {
    pub(crate) dar: [u32; 16],
    pub(crate) dar_save: [u32; 16],
    pub(crate) sr_save: u16,
    pub(crate) ppc: u32,
    pub(crate) stack_pointers: [u32; 8],
    pub(crate) pc: u32,
    pub(crate) sr: u16,
    pub(crate) vbr: u32,
    pub(crate) sfc: u32,
    pub(crate) dfc: u32,
    pub(crate) cacr: u32,
    pub(crate) caar: u32,
    pub(crate) cacr_pending_ops: u32,
    pub(crate) itt: [u32; 2],
    pub(crate) dtt: [u32; 2],
    pub(crate) ir: u32,
    pub(crate) fpr: [m68k::fpu::FloatX80; 8],
    pub(crate) fpiar: u32,
    pub(crate) fpsr: u32,
    pub(crate) fpcr: u32,
    pub(crate) mmu: [u32; 14],
    pub(crate) pmmu_enabled: bool,
    pub(crate) int_level: u32,
    pub(crate) stopped: u32,
    pub(crate) change_of_flow: bool,
    pub(crate) loop_mode: bool,
    pub(crate) loop_body_word: u16,
    pub(crate) loop_dbcc_word: u16,
    pub(crate) prefetch: [u16; 2],
    pub(crate) prefetch_count: u8,
    pub(crate) consume_without_prefetch: bool,
    pub(crate) pending_sync_clocks: u32,
    pub(crate) run_mode: u32,
    pub(crate) fpu_just_reset: bool,
    pub(crate) reset_cycles: u32,
    pub(crate) virq_state: u32,
    pub(crate) nmi_pending: u32,
    pub(crate) exception_processing: bool,
}

impl CpuArchitecturalSnapshot {
    pub(crate) fn capture(cpu: &m68k::CpuCore) -> Self {
        Self {
            dar: cpu.dar,
            dar_save: cpu.dar_save,
            sr_save: cpu.sr_save,
            ppc: cpu.ppc,
            stack_pointers: cpu.sp,
            pc: cpu.pc,
            sr: cpu.get_sr(),
            vbr: cpu.vbr,
            sfc: cpu.sfc,
            dfc: cpu.dfc,
            cacr: cpu.cacr,
            caar: cpu.caar,
            cacr_pending_ops: cpu.cacr_pending_ops,
            itt: [cpu.itt0, cpu.itt1],
            dtt: [cpu.dtt0, cpu.dtt1],
            ir: cpu.ir,
            fpr: cpu.fpr,
            fpiar: cpu.fpiar,
            fpsr: cpu.fpsr,
            fpcr: cpu.fpcr,
            mmu: [
                cpu.mmu_crp_aptr,
                cpu.mmu_crp_limit,
                cpu.mmu_srp_aptr,
                cpu.mmu_srp_limit,
                cpu.mmu_tc,
                cpu.mmu_sr,
                cpu.mmu_tt0,
                cpu.mmu_tt1,
                cpu.dacr0,
                cpu.dacr1,
                cpu.iacr0,
                cpu.iacr1,
                cpu.pcr,
                cpu.buscr,
            ],
            pmmu_enabled: cpu.pmmu_enabled,
            int_level: cpu.int_level,
            stopped: cpu.stopped,
            // Deliberately normalized: `change_of_flow` is the m68k core's
            // internal did-the-last-instruction-branch bookkeeping (a trace
            // and loop-mode heuristic input), not architectural state. Its
            // value at a trap site depends on whether execution arrived via
            // the interpreter or a compiled trace, so comparing it makes
            // wait-identity proofs fail whenever the JIT compiles part of a
            // wait loop: measured on the SC2K boot census, the flag was the
            // sole differing field in every sampled proof failure, and each
            // lost proof is a lost tick fast-forward.
            change_of_flow: false,
            loop_mode: cpu.loop_mode,
            loop_body_word: cpu.loop_body_word,
            loop_dbcc_word: cpu.loop_dbcc_word,
            prefetch: cpu.prefetch_queue,
            prefetch_count: cpu.prefetch_count,
            consume_without_prefetch: cpu.consume_without_prefetch,
            pending_sync_clocks: cpu.pending_sync_clocks,
            run_mode: cpu.run_mode,
            fpu_just_reset: cpu.fpu_just_reset,
            reset_cycles: cpu.reset_cycles,
            virq_state: cpu.virq_state,
            nmi_pending: cpu.nmi_pending,
            exception_processing: cpu.exception_processing,
        }
    }
}

pub(crate) struct IdleCycleProbe {
    pub(crate) trap_pc: u32,
    pub(crate) tick: u32,
    pub(crate) cpu: CpuArchitecturalSnapshot,
    /// Same-site arrivals observed since the probe began without matching
    /// the starting CPU state. A wait cycle may have a small period (EV
    /// Override's crawl alternates two polled keycodes through D5, a
    /// strict period-2 cycle); the proof closes when an arrival matches
    /// the probe's origin with the write journal -- kept open across the
    /// whole period -- restored, and aborts past `IDLE_CYCLE_MAX_PERIOD`.
    pub(crate) arrivals: u8,
}

/// Longest wait-cycle period the exact-state prover will chase. Period-2
/// covers the measured EV Override crawl; 4 leaves headroom without
/// letting genuinely progressing loops hold a write journal open long.
pub(crate) const IDLE_CYCLE_MAX_PERIOD: u8 = 4;

/// Probes the prover will start at one poll site within one tick. A genuine
/// wait proves on its first probe (or its second, when the loop's first
/// iteration still carries setup); a site whose probes keep failing on
/// changed memory or CPU state is polling while it works. Without a budget
/// such a site re-arms a fresh write journal on every arrival -- EV
/// Override's boot started 928,781 probes in 17 s, 926,295 of them failing
/// on memory -- and each journal costs a hash insert per store plus fastmem
/// withdrawn for the whole core. Past the budget the site is left alone
/// until the tick changes.
pub(crate) const IDLE_CYCLE_MAX_PROBES_PER_TICK: u8 = 2;

/// Host-side Event Manager inputs that are not stored in guest RAM. A proven
/// idle cycle may remain parked across frontend calls only while these inputs
/// are unchanged and the Event Manager still has no deliverable event.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct IdleCycleHostSnapshot {
    pub(crate) mouse_pos: (i16, i16),
    pub(crate) mouse_button: bool,
    pub(crate) key_map: [u8; 16],
    pub(crate) caps_lock_physically_pressed: bool,
    /// Host mirror of the guest window chain, read by the admitted Window
    /// Manager queries. Every mutation of it is also written into the guest
    /// chain (journaled), so this is belt-and-braces: a parked cycle must
    /// not resume across a reordering the journal somehow missed.
    pub(crate) window_list: Vec<u32>,
    /// A native menu selection staged for MenuSelect. It is always paired
    /// with a pending event the resume gate sees; recorded here so the
    /// pairing is not the only thing standing between it and a proof.
    pub(crate) pending_native_menu_selection: Option<(i16, i16)>,
}

impl IdleCycleHostSnapshot {
    pub(crate) fn capture(dispatcher: &TrapDispatcher) -> Self {
        Self {
            mouse_pos: dispatcher.input_state.mouse_pos,
            mouse_button: dispatcher.input_state.mouse_button,
            key_map: *dispatcher.key_map_bytes(),
            caps_lock_physically_pressed: dispatcher.input_state.caps_lock_physically_pressed,
            window_list: dispatcher.window_list.clone(),
            pending_native_menu_selection: dispatcher.pending_native_menu_selection.snapshot(),
        }
    }
}

/// A complete null-event cycle that has already been proven to be an exact
/// identity operation. The bus write journal remains armed while the frontend
/// owns execution, so any guest-memory mutation invalidates the parked state
/// without hashing the whole emulated address space every frame.
pub(crate) struct ProvenIdleCycleSleep {
    pub(crate) trap_pc: u32,
    pub(crate) wake_tick: u32,
    pub(crate) tick: u32,
    pub(crate) cpu: CpuArchitecturalSnapshot,
    pub(crate) host: IdleCycleHostSnapshot,
}
