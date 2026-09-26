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
    /// Tick budget when the probe began, so a closing cycle knows the
    /// instruction units one pass costs.
    pub(crate) budget: i32,
}

/// A same-site pass that returned to its origin CPU state having changed
/// only a few words of RAM, awaiting a verification pass with those words
/// watched (see `verified_idle_counter`).
pub(crate) struct IdleCounterCandidate {
    pub(crate) trap_pc: u32,
    pub(crate) tick: u32,
    pub(crate) changes: Vec<(u32, u32, u32)>,
    pub(crate) units: i32,
}

/// A loop counter proven to be touched only by one add- or
/// subtract-immediate instruction per pass.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct IdleCounter {
    pub(crate) address: u32,
    pub(crate) width: u32,
    pub(crate) step: u32,
}

/// Whether two passes changed the same words by the same amounts.
pub(crate) fn same_idle_steps(a: &[(u32, u32, u32)], b: &[(u32, u32, u32)]) -> bool {
    a.len() == b.len()
        && a.iter().zip(b).all(|(&(wa, oa, na), &(wb, ob, nb))| {
            wa == wb && na.wrapping_sub(oa) == nb.wrapping_sub(ob)
        })
}

/// Operand width of an ADDQ, SUBQ, ADDI or SUBI whose destination is a
/// memory operand, the only instructions admitted to update an idle
/// counter: each reads its destination, writes the sum back and sets flags,
/// and touches nothing else. M68000 Programmer's Reference Manual (1992),
/// pp. 4-4, 4-9, 4-174, 4-179.
pub(crate) fn add_immediate_to_memory_width(opcode: u16) -> Option<u32> {
    let size = (opcode >> 6) & 3;
    let mode = (opcode >> 3) & 7;
    let reg = opcode & 7;
    let add_quick = opcode & 0xF000 == 0x5000;
    let add_immediate = matches!(opcode & 0xFF00, 0x0400 | 0x0600);
    let memory = matches!(mode, 2..=6) || (mode == 7 && reg <= 1);
    ((add_quick || add_immediate) && size != 3 && memory).then(|| [1, 2, 4][size as usize])
}

/// Decide from a verification pass's watched accesses whether the words a
/// pass changes form one counter that nothing but its own update reads:
/// every access comes from the same add/subtract-immediate instruction,
/// which first reads its whole operand and then writes within it (the bus
/// may store only the bytes that changed), and the operand covers every
/// changed byte. `old_and_new_byte` gives each byte's value before and
/// after the pass (from the journal) so the per-pass step can be recovered.
pub(crate) fn verified_idle_counter(
    hits: &[crate::memory::AccessHit],
    changes: &[(u32, u32, u32)],
    opcode_at: impl Fn(u32) -> u16,
    old_and_new_byte: impl Fn(u32) -> (u8, u8),
) -> Option<IdleCounter> {
    use crate::memory::AccessSource;
    let [read, writes @ ..] = hits else {
        return None;
    };
    let AccessSource::Instruction(pc) = read.source else {
        return None;
    };
    if read.write || add_immediate_to_memory_width(opcode_at(pc)) != Some(read.len) {
        return None;
    }
    let (address, width) = (read.address, read.len);
    let inside = |hit: &crate::memory::AccessHit| {
        hit.write
            && hit.source == read.source
            && hit.address >= address
            && u64::from(hit.address) + u64::from(hit.len) <= u64::from(address) + u64::from(width)
    };
    if writes.is_empty() || !writes.iter().all(inside) {
        return None;
    }
    let covered = changes.iter().all(|&(word, old, new)| {
        (0..4).all(|i| {
            let shift = 24 - 8 * i;
            (old >> shift) & 0xFF == (new >> shift) & 0xFF
                || (address..address + width).contains(&(word + i))
        })
    });
    if !covered {
        return None;
    }
    let (mut old, mut new) = (0u32, 0u32);
    for i in 0..width {
        let (o, n) = old_and_new_byte(address + i);
        old = (old << 8) | u32::from(o);
        new = (new << 8) | u32::from(n);
    }
    let mask = if width == 4 { u32::MAX } else { (1 << (8 * width)) - 1 };
    let step = new.wrapping_sub(old) & mask;
    (step != 0).then_some(IdleCounter { address, width, step })
}

/// How many more passes may add `step` to a counter now holding `value`
/// while every one of them sets the same condition codes as the pass just
/// observed: the results stay nonzero, on the same side of the sign bit,
/// and never carry or borrow out of `width` bytes.
pub(crate) fn passes_with_unchanged_flags(value: u32, step: u32, width: u32) -> u32 {
    let mask = if width == 4 { u32::MAX } else { (1u32 << (8 * width)) - 1 };
    let sign = 1u32 << (8 * width - 1);
    let (value, step) = (value & mask, step & mask);
    if step & sign == 0 {
        // Adding: the observed pass computed value from value - step.
        let Some(previous) = value.checked_sub(step) else {
            return 0;
        };
        if value == 0 || previous & sign != value & sign {
            return 0;
        }
        let limit = if value & sign == 0 { sign - 1 } else { mask };
        (limit - value) / step
    } else {
        // Subtracting `magnitude` each pass.
        let magnitude = (mask - step) + 1;
        let previous = u64::from(value) + u64::from(magnitude);
        if previous > u64::from(mask) || (previous as u32) & sign != value & sign {
            return 0;
        }
        let floor = if value & sign == 0 { 1 } else { sign };
        if value < floor {
            return 0;
        }
        (value - floor) / magnitude
    }
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
    /// Each sound channel's activity, which decides whether SndDoCommand
    /// queues a command or executes it. Playback ends only in frame-boundary
    /// audio servicing, so a parked cycle must not resume across a change.
    pub(crate) sound_channels_busy: Vec<(u32, bool)>,
}

impl IdleCycleHostSnapshot {
    pub(crate) fn capture(dispatcher: &TrapDispatcher) -> Self {
        Self {
            mouse_pos: dispatcher.input_state.mouse_position(),
            mouse_button: dispatcher.input_state.mouse_button_pressed(),
            key_map: dispatcher.key_map_bytes(),
            caps_lock_physically_pressed: dispatcher
                .input_state
                .caps_lock_physically_pressed(),
            window_list: dispatcher.window_list.to_vec(),
            pending_native_menu_selection: dispatcher.pending_native_menu_selection.snapshot(),
            sound_channels_busy: dispatcher.sound_channels_busy(),
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
