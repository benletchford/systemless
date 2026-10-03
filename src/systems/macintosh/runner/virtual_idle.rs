//! Skipping passes of a proven idle cycle without changing anything the
//! guest, the runner or the frame driver can observe.
//!
//! A proven cycle returns to the same CPU state with every byte it writes
//! restored, so executing another pass changes nothing but time. Parking to
//! the next tick (`ProvenIdleCycleSleep`) saves that time for the cycles it
//! has always covered, but it ends CPU slices at different points than
//! executing them would, and the frame drivers mix audio and run Sound
//! Manager callbacks at slice boundaries. Cycles newly admitted to the proof
//! instead take a *virtual cursor*:
//!
//! 1. At a poll site where the exact-cycle prover that parks cycles has
//!    given up for the tick, one pass is observed at full speed and recorded
//!    as it runs ([`RecordedPass`]: its steps, the tick units each trap adds,
//!    and the starting value of every word it writes). It is kept only if it
//!    returns to its starting CPU state with every written word restored,
//!    calls only admitted traps, runs no interrupt code, leaves that
//!    prover's state untouched, and accounts for itself exactly: every
//!    counted step executed, and the budget it spent equal to those steps
//!    plus its traps' units. The prover itself is not changed: it still
//!    decides every park exactly as before, and a cycle it leaves alone for
//!    a whole pass is one it leaves alone for the rest of the tick (its
//!    per-site state is keyed by the tick), so skipping those passes skips
//!    only no-ops of its own.
//! 2. The real CPU then stays at the pass start while the slice advances a
//!    cursor through the recording. The slice reports exactly the steps it
//!    would have executed and charges exactly their units; the cursor stops
//!    before the tick boundary and before a Time Manager task falls due.
//! 3. Before anything could observe the CPU or change what the loop reads --
//!    a callback, pending interrupt work, changed input, the tick boundary,
//!    and in any case the end of the slice -- the cursor *materializes*:
//!    memory returns to the pass start, the steps up to the cursor execute
//!    for real without being counted again, and execution continues from
//!    the exact state. Code outside the slice loop therefore never sees a
//!    virtual cursor (a sound callback entered between slices saves the CPU
//!    as the context it returns to), and the next slice observes afresh.

use super::*;

/// Observations a poll site may start per tick without one being recorded.
/// A recorded one resets the count: carried across slices, it is resumed
/// rather than observed again, so only sites that keep failing are capped.
const VIRTUAL_IDLE_FAILED_ATTEMPTS_PER_TICK: u8 = 3;

/// Longest pass observed or recorded. Poll loops are a few hundred steps;
/// an observation still open well past that is following ordinary code
/// with the write journal (and so the slow store paths) armed.
const VIRTUAL_IDLE_MAX_PASS_STEPS: u64 = 1 << 13;

/// Traps a recorded pass may call: everything the parking prover admits,
/// plus pure transforms and save/restore pairs whose effects are all
/// journaled guest writes or stack results. The additions are withheld
/// during Toolbox UI tracking, whose frames end on tracking refires rather
/// than instruction counts.
pub(crate) fn virtual_idle_trap_is_journal_complete(
    opcode: u16,
    null_event: bool,
    system_task_idle: bool,
    sound_command_rejected: bool,
    selector: u32,
    ui_tracking: bool,
) -> bool {
    idle_cycle_trap_is_journal_complete(
        opcode,
        null_event,
        system_task_idle,
        sound_command_rejected,
        selector,
    ) || (!ui_tracking
        && matches!(
            canonical_trap_number(opcode),
            // AddPt, SubPt, SetPt, EqualPt; EqualRect .. Pt2Rect, EmptyRect
            // (quickdraw.rs): operands from the stack and the records it
            // points to, results on the stack or in VAR records.
            (true, 0x007E..=0x0081)
                | (true, 0x00A6..=0x00AC)
                | (true, 0x00AE)
                // BitAnd .. BitClr (toolbox.rs): pure bit arithmetic.
                | (true, 0x0058..=0x005F)
                // SetPort, GetPort, SetGDevice, GetGDevice: ThePort and
                // TheGDevice are journaled; the dispatcher mirrors are pure
                // functions of them and guest RAM.
                | (true, 0x0073)
                | (true, 0x0074)
                | (true, 0x0231)
                | (true, 0x0232)
                // CopyMask: stores destination pixels through the bus only.
                | (true, 0x0017)
        ))
}

/// The parking prover's state, which a recorded pass must leave unchanged.
#[derive(Clone, PartialEq)]
pub(crate) struct IdleProverState {
    probe: bool,
    counter: bool,
    sleep: bool,
    last_seen: Option<(u32, u32)>,
    sites: [IdleCycleSiteRecord; IDLE_CYCLE_SITE_SLOTS],
}

/// One pass of a proven idle cycle, recorded step by step.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct RecordedPass {
    /// Steps in the pass, as the runner counts them (instructions plus the
    /// opcode word of each A-line exit).
    pub(crate) steps: u64,
    /// Units a trap charges beyond its step, by the offset of that step.
    pub(crate) extra_units: Vec<(u64, i64)>,
    /// Every word the pass writes, with its value at the pass start.
    pub(crate) start_values: Vec<(u32, u32)>,
}

impl RecordedPass {
    /// Tick units charged by the first `offset` steps of a pass.
    pub(crate) fn units_to(&self, offset: u64) -> i64 {
        offset as i64
            + self
                .extra_units
                .iter()
                .take_while(|(at, _)| *at <= offset)
                .map(|(_, units)| *units)
                .sum::<i64>()
    }

    /// Tick units charged by the `count` steps after `offset` (which may
    /// run into later passes).
    pub(crate) fn units_after(&self, offset: u64, count: u64) -> i64 {
        let end = offset + count;
        let passes = (end / self.steps) as i64;
        passes * self.units_to(self.steps) + self.units_to(end % self.steps) - self.units_to(offset)
    }

    /// The most steps, up to `limit`, after `offset` whose units stay within
    /// `units`.
    pub(crate) fn steps_within(&self, offset: u64, limit: u64, units: i64) -> u64 {
        if units <= 0 {
            return 0;
        }
        let (mut low, mut high) = (0u64, limit);
        while low < high {
            let mid = low + (high - low).div_ceil(2);
            if self.units_after(offset, mid) <= units {
                low = mid;
            } else {
                high = mid - 1;
            }
        }
        low
    }
}

/// A pass being observed from a poll site.
pub(crate) struct PassRecorder {
    pub(crate) prover: IdleProverState,
    pub(crate) trap_pc: u32,
    pub(crate) tick: u32,
    pub(crate) cpu: CpuArchitecturalSnapshot,
    pub(crate) arrivals: u8,
    pub(crate) start_instructions: u64,
    /// Tick budget at the pass start.
    pub(crate) start_budget: i32,
    /// Steps the slice loop actually executed during the pass. Anything
    /// that advanced the instruction count without executing (a spin
    /// fast-forward) makes it fall short, and the pass is not recorded.
    pub(crate) executed: u64,
    /// `(offset, units)` for each trap that charged extra units.
    pub(crate) extra_units: Vec<(u64, i64)>,
    /// Return PCs of trap-patch calls into native traps that were in flight
    /// when the pass began. A pass that starts or retires one is not a
    /// repeatable cycle; a call merely pending throughout it (a patch
    /// further up the guest's call stack) cannot retire during a skipped
    /// pass, because that takes executing its return PC.
    pub(crate) native_returns: Vec<u32>,
}

/// A proven cycle whose passes are being skipped: the CPU is held at the
/// pass start while `offset` counts the steps taken into the current pass.
pub(crate) struct VirtualIdleCycle {
    pub(crate) tick: u32,
    pub(crate) pass: RecordedPass,
    pub(crate) offset: u64,
    pub(crate) host: IdleCycleHostSnapshot,
    pub(crate) start: PassStart,
}

/// The state a recorded pass starts from, beyond the memory it writes.
pub(crate) struct PassStart {
    pub(crate) trap_pc: u32,
    pub(crate) cpu: CpuArchitecturalSnapshot,
    pub(crate) prover: IdleProverState,
    pub(crate) native_returns: Vec<u32>,
}

/// A recording carried across a slice boundary, with a write journal armed
/// from the end of that slice. When the next slice arrives back at the
/// pass start in the same tick and the whole machine provably equals the
/// recorded start, the cursor resumes without observing the pass again.
pub(crate) struct KeptIdleRecording {
    pub(crate) tick: u32,
    pub(crate) pass: RecordedPass,
    pub(crate) host: IdleCycleHostSnapshot,
    pub(crate) start: PassStart,
    /// The journal it armed (`MacMemoryBus::write_probe_generation`).
    pub(crate) journal: u64,
}

impl FixtureRunner {
    /// Whether a proven cycle may be recorded and skipped at all. Anything
    /// that watches individual instructions, a debugger, or address
    /// translation that the word rewrites do not model keeps executing.
    pub(crate) fn virtual_idle_allowed(&self) -> bool {
        self.virtual_idle_enabled
            && !per_instruction_diagnostics_active()
            && !trace_opcode_counts_enabled()
            && !trace_hot_pc_enabled()
            && self.debug_step_units_remaining().is_none()
            && self.bus.guest_ram_is_identity_mapped()
    }

    fn idle_prover_state(&self) -> IdleProverState {
        IdleProverState {
            probe: self.idle_cycle_probe.is_some(),
            counter: self.idle_counter.is_some(),
            sleep: self.idle_cycle_sleep.is_some(),
            last_seen: self.idle_cycle_last_seen,
            sites: self.idle_cycle_sites,
        }
    }

    /// Whether the parking prover holds the write journal.
    fn idle_prover_owns_journal(&self) -> bool {
        self.idle_cycle_probe.is_some() || self.idle_cycle_sleep.is_some()
    }

    /// Start observing a pass from the poll site at `trap_pc`.
    fn begin_pass_recording(&mut self, trap_pc: u32, tick: u32, cpu: CpuArchitecturalSnapshot) {
        self.bus.begin_write_probe();
        self.pass_recorder = Some(PassRecorder {
            prover: self.idle_prover_state(),
            trap_pc,
            tick,
            cpu,
            arrivals: 0,
            start_instructions: self.total_instructions,
            start_budget: self.tick_budget,
            executed: 0,
            extra_units: Vec::new(),
            native_returns: self.native_trap_return_pcs(),
        });
    }

    fn native_trap_call_count(&self) -> usize {
        self.dispatcher.pending_native_trap_call_count()
    }

    fn native_trap_return_pcs(&self) -> Vec<u32> {
        let mut pcs = Vec::new();
        self.dispatcher.append_pending_native_trap_return_pcs(&mut pcs);
        pcs.sort_unstable();
        pcs
    }

    pub(crate) fn cancel_pass_recording(&mut self) {
        if self.pass_recorder.take().is_some() && !self.idle_prover_owns_journal() {
            self.bus.cancel_write_probe();
        }
    }

    /// Check a trap dispatched during an observed pass: an unadmitted trap
    /// or outcome, a callback, a guest patch, or the parking prover taking
    /// the journal ends the observation.
    pub(crate) fn record_pass_trap(&mut self, opcode: u16, null_event: bool, extra_units: i32) {
        let Some(mut recorder) = self.pass_recorder.take() else {
            return;
        };
        let offset = self.total_instructions - recorder.start_instructions;
        let plain = offset <= VIRTUAL_IDLE_MAX_PASS_STEPS
            && self.native_trap_call_count() == recorder.native_returns.len()
            && self.virtual_pass_trap_is_plain(opcode, null_event);
        if extra_units > 0 {
            recorder.extra_units.push((offset, i64::from(extra_units)));
        }
        self.pass_recorder = Some(recorder);
        if !plain {
            self.cancel_pass_recording();
        }
    }

    fn virtual_pass_trap_is_plain(&self, opcode: u16, null_event: bool) -> bool {
        let sound_command_rejected = canonical_trap_number(opcode) == (true, 0x0003)
            && self.bus.read_word(self.m68k.cpu.core.a(7)) as i16 == SOUND_QUEUE_FULL;
        !self.idle_prover_owns_journal()
            && self.active_interrupt_callback.is_none()
            && virtual_idle_trap_is_journal_complete(
                opcode,
                null_event,
                !self.dispatcher.system_task_has_periodic_work(&self.bus),
                sound_command_rejected,
                self.m68k.cpu.read_reg(Register::D0),
                self.is_ui_tracking_active(),
            )
    }

    /// An arrival at a poll site, after the parking prover has seen it.
    /// Closes a recording that returned to its starting state, or starts
    /// one here when the parking prover holds no journal.
    pub(crate) fn note_virtual_anchor_arrival(&mut self, trap_pc: u32) {
        if self.pass_recorder.is_some() && self.idle_prover_owns_journal() {
            // The parking prover began a probe and took the journal.
            self.pass_recorder = None;
        }
        if !self.virtual_idle_allowed() || self.virtual_idle_cycle.is_some() {
            return;
        }
        // Poll sites are hit constantly, so the CPU snapshot is taken only
        // on the paths that compare or keep it.
        let tick = self.guest_tick();
        let snapshot = |runner: &Self| CpuArchitecturalSnapshot::capture(&runner.m68k.cpu.core);
        if self
            .virtual_idle_kept
            .as_ref()
            .is_some_and(|kept| kept.start.trap_pc == trap_pc || kept.tick != tick)
        {
            let kept = self.virtual_idle_kept.take().unwrap();
            if self.resume_kept_idle_recording(kept, tick, &snapshot(self)) {
                return;
            }
        }
        if let Some(recorder) = self.pass_recorder.as_ref() {
            if recorder.trap_pc != trap_pc || recorder.tick != tick {
                return;
            }
            if snapshot(self) != self.pass_recorder.as_ref().unwrap().cpu {
                let recorder = self.pass_recorder.as_mut().unwrap();
                recorder.arrivals += 1;
                if recorder.arrivals >= IDLE_CYCLE_MAX_PERIOD {
                    self.cancel_pass_recording();
                }
                return;
            }
            self.close_pass_recording(tick);
            return;
        }
        // Only where the parking prover has given up on the site for this
        // tick: until then it may still probe there, and an observation
        // it interrupts is wasted work under a slow-path journal.
        if self.idle_prover_owns_journal()
            || self.idle_counter.is_some()
            || !self.idle_cycle_site_is_busy(trap_pc, tick)
        {
            return;
        }
        // Failed observations per (site, tick), one slot per recent site
        // (several poll sites interleave within a tick).
        let slot = self
            .virtual_idle_attempts
            .iter()
            .position(|&(site, _, _)| site == trap_pc)
            .unwrap_or_else(|| {
                self.virtual_idle_attempts
                    .iter()
                    .enumerate()
                    .min_by_key(|(_, &(_, at, _))| at)
                    .map(|(index, _)| index)
                    .unwrap()
            });
        let (site, at, failures) = self.virtual_idle_attempts[slot];
        let failures = if site == trap_pc && at == tick { failures } else { 0 };
        if failures >= VIRTUAL_IDLE_FAILED_ATTEMPTS_PER_TICK {
            return;
        }
        self.virtual_idle_attempts[slot] = (trap_pc, tick, failures + 1);
        let cpu = snapshot(self);
        self.begin_pass_recording(trap_pc, tick, cpu);
    }

    fn close_pass_recording(&mut self, tick: u32) {
        let recorder = self.pass_recorder.take().unwrap();
        let recorder_site = recorder.trap_pc;
        let mut start_values = Vec::new();
        let journal_ok = self.bus.write_probe_words(&mut start_values);
        let restored = self.bus.finish_write_probe_changes();
        let steps = self.total_instructions - recorder.start_instructions;
        let extra: i64 = recorder.extra_units.iter().map(|&(_, units)| units).sum();
        // The pass is recorded only when its own accounting closes: every
        // counted step was executed, and the budget it spent is exactly
        // those steps plus its traps' extra units.
        if !journal_ok
            || restored.as_ref().is_none_or(|changes| !changes.is_empty())
            || recorder.prover != self.idle_prover_state()
            || recorder.native_returns != self.native_trap_return_pcs()
            || steps == 0
            || steps > VIRTUAL_IDLE_MAX_PASS_STEPS
            || recorder.executed != steps
            || i64::from(recorder.start_budget) - i64::from(self.tick_budget) != steps as i64 + extra
        {
            return;
        }
        start_values.sort_unstable();
        if let Some(slot) = self
            .virtual_idle_attempts
            .iter_mut()
            .find(|(site, _, _)| *site == recorder_site)
        {
            slot.2 = 0;
        }
        self.virtual_idle_entries += 1;
        self.virtual_idle_cycle = Some(VirtualIdleCycle {
            tick,
            pass: RecordedPass {
                steps,
                extra_units: recorder.extra_units,
                start_values,
            },
            offset: 0,
            host: IdleCycleHostSnapshot::capture(&self.dispatcher),
            start: PassStart {
                trap_pc: recorder.trap_pc,
                cpu: recorder.cpu,
                prover: recorder.prover,
                native_returns: recorder.native_returns,
            },
        });
    }

    /// Whether nothing acts at the next loop top: no callback or interrupt
    /// work is pending, no task is due, the clock is running, and the input
    /// and events the cycle reads are unchanged.
    fn virtual_idle_cycle_is_quiet(&mut self, sound_work_only: bool) -> bool {
        let Some(cycle) = self.virtual_idle_cycle.as_ref() else {
            return false;
        };
        let (tick, host) = (cycle.tick, cycle.host.clone());
        !sound_work_only
            && !self.halted
            && self.guest_tick() == tick
            && self.active_interrupt_callback.is_none()
            && self.frozen_ticks.is_none()
            && !self.callback_suspends_guest_clock()
            && !self.has_pending_sound_work()
            && !self.dispatcher.vbl_tasks.with_ref(|tasks| {
                tasks
                    .iter()
                    .any(|task| task.architecture == CallbackTaskArchitecture::M68k && task.pending)
            })
            && self.dispatcher.pending_file_completions.is_empty()
            && !self.dispatcher.adb.has_pending_packet()
            && self.dispatcher.deferred_tasks.is_empty()
            && self.dispatcher.pending_wait_sleep_ticks == 0
            && self.dispatcher.pending_delay_ticks == 0
            && self.dispatcher.pending_launch_app.is_none()
            && self.deferred_tracking_refire_pc.is_none()
            && !self.dispatcher.has_ready_menu_tracking()
            && !self.dispatcher.input_state.has_key_repeat()
            && !self.dispatcher.system_task_has_periodic_work(&self.bus)
            && self
                .next_m68k_timer_subtick()
                .is_none_or(|due| due > self.current_timer_subtick())
            && host == IdleCycleHostSnapshot::capture(&self.dispatcher)
            && self
                .dispatcher
                .peek_toolbox_event(&self.bus, u16::MAX)
                .is_none()
    }

    /// Units that can be charged before the next M68k Time Manager task
    /// falls due at a loop top.
    fn units_before_next_m68k_timer(&self) -> Option<i64> {
        const SUBTICKS_PER_TICK: i128 = 1_000_000;
        let due = self.next_m68k_timer_subtick()?;
        let per_tick = i128::from(self.instructions_per_tick.max(1));
        let tick_base = i128::from(self.guest_tick()) * SUBTICKS_PER_TICK;
        let room = i128::from(due) - tick_base;
        if room >= SUBTICKS_PER_TICK {
            return None;
        }
        let elapsed = per_tick - i128::from(self.tick_budget.max(0)).min(per_tick);
        // The subtick after charging u units stays below `due` while
        // (elapsed + u) * SUBTICKS < room * per_tick.
        let limit = (room * per_tick + SUBTICKS_PER_TICK - 1) / SUBTICKS_PER_TICK - 1 - elapsed;
        Some(limit.clamp(0, i128::from(i64::MAX)) as i64)
    }

    /// Advance a virtual cycle by up to `limit` steps at a loop top. Returns
    /// the steps taken, already counted in `total_instructions` and charged
    /// to the tick budget. Takes nothing (and materializes) when anything
    /// would act at this loop top.
    pub(crate) fn advance_virtual_idle_cycle(&mut self, limit: usize, sound_work_only: bool) -> usize {
        if self.virtual_idle_cycle.is_none() {
            return 0;
        }
        if !self.virtual_idle_cycle_is_quiet(sound_work_only) {
            self.materialize_virtual_idle_cycle();
            return 0;
        }
        // Stop while at least two units remain: the loop top charges the
        // tick boundary's instruction ahead of it once one remains.
        let mut units = i64::from(self.tick_budget) - 2;
        if let Some(timer) = self.units_before_next_m68k_timer() {
            units = units.min(timer);
        }
        let cycle = self.virtual_idle_cycle.as_mut().unwrap();
        let steps = cycle.pass.steps_within(cycle.offset, limit as u64, units);
        if steps == 0 {
            self.materialize_virtual_idle_cycle();
            return 0;
        }
        let charged = cycle.pass.units_after(cycle.offset, steps);
        cycle.offset = (cycle.offset + steps) % cycle.pass.steps;
        self.total_instructions = self.total_instructions.wrapping_add(steps);
        self.tick_budget -= charged as i32;
        if (steps as usize) < limit {
            // Stopped for the tick boundary or a timer: execute from here.
            self.materialize_virtual_idle_cycle();
        }
        steps as usize
    }

    /// Make the cursor's position real: memory returns to the pass start,
    /// and the steps up to the cursor execute again without being counted
    /// or charged a second time.
    pub(crate) fn materialize_virtual_idle_cycle(&mut self) {
        let _ = self.take_materialized_virtual_idle_cycle();
    }

    /// End a slice: materialize, and keep the recording with a journal
    /// armed so the next slice can resume it without observing again.
    pub(crate) fn keep_virtual_idle_cycle_across_slice(&mut self) {
        let Some(cycle) = self.take_materialized_virtual_idle_cycle() else {
            return;
        };
        if self.idle_prover_owns_journal() {
            return;
        }
        self.bus.begin_write_probe();
        self.virtual_idle_kept = Some(KeptIdleRecording {
            tick: cycle.tick,
            pass: cycle.pass,
            host: cycle.host,
            start: cycle.start,
            journal: self.bus.write_probe_generation(),
        });
    }

    /// Drop a kept recording, closing its journal if that is still armed.
    pub(crate) fn drop_kept_idle_recording(&mut self) {
        if let Some(kept) = self.virtual_idle_kept.take() {
            if self.kept_journal_is_armed(&kept) {
                self.bus.cancel_write_probe();
            }
        }
    }

    fn kept_journal_is_armed(&self, kept: &KeptIdleRecording) -> bool {
        self.bus.write_probe_generation() == kept.journal && !self.idle_prover_owns_journal()
    }

    /// Resume a kept recording at its pass start when the machine provably
    /// equals the recorded start: same tick, CPU, parking-prover state,
    /// trap-patch calls and host input, and every word written since the
    /// journal was armed holding its pass-start value (a word the pass
    /// writes) or its value before (any other word). A word the journal did
    /// not see is either outside the pass, and so unchanged since the pass
    /// start, or written by the pass only before the slice ended, and so
    /// already back at its start value.
    fn resume_kept_idle_recording(
        &mut self,
        kept: KeptIdleRecording,
        tick: u32,
        cpu: &CpuArchitecturalSnapshot,
    ) -> bool {
        let armed = self.kept_journal_is_armed(&kept);
        let mut journaled = Vec::new();
        let journal_ok = armed && self.bus.write_probe_words(&mut journaled);
        if armed {
            self.bus.cancel_write_probe();
        }
        if !journal_ok
            || tick != kept.tick
            || *cpu != kept.start.cpu
            || self.idle_prover_state() != kept.start.prover
            || self.native_trap_return_pcs() != kept.start.native_returns
            || IdleCycleHostSnapshot::capture(&self.dispatcher) != kept.host
        {
            return false;
        }
        let at_start = journaled.iter().all(|&(word, before)| {
            let start = match kept.pass.start_values.binary_search_by_key(&word, |&(w, _)| w) {
                Ok(index) => kept.pass.start_values[index].1,
                Err(_) => before,
            };
            self.bus.journaled_word(word) == start
        });
        if !at_start {
            return false;
        }
        self.virtual_idle_entries += 1;
        self.virtual_idle_cycle = Some(VirtualIdleCycle {
            tick,
            pass: kept.pass,
            offset: 0,
            host: kept.host,
            start: kept.start,
        });
        true
    }

    fn take_materialized_virtual_idle_cycle(&mut self) -> Option<VirtualIdleCycle> {
        let cycle = self.virtual_idle_cycle.take()?;
        for &(word, value) in &cycle.pass.start_values {
            if self.bus.journaled_word(word) != value {
                self.bus.write_long(word, value);
            }
        }
        let expected_budget = self.tick_budget;
        self.tick_budget += cycle.pass.units_to(cycle.offset) as i32;
        self.replay_idle_steps(cycle.offset);
        debug_assert_eq!(self.tick_budget, expected_budget);
        self.tick_budget = expected_budget;
        Some(cycle)
    }

    /// Execute `steps` steps of a recorded pass without counting them: they
    /// were counted when the cursor advanced over them. The recorded pass
    /// contained only admitted traps returning straight to their callers.
    fn replay_idle_steps(&mut self, mut steps: u64) {
        while steps > 0 && !self.halted {
            let previous_mouse = self.bus.read_long(crate::memory::globals::addr::MOUSE_LOC2);
            let batch = self
                .m68k
                .cpu
                .run_batch(&mut self.bus, steps.min(u64::from(u32::MAX)) as u32, &[0]);
            self.sync_guest_mouse_position(previous_mouse);
            self.dispatcher
                .retire_returned_native_trap_call(&mut self.m68k.cpu);
            let executed = u64::from(batch.instructions)
                + u64::from(matches!(
                    batch.exit,
                    BatchExit::AlineTrap { .. } | BatchExit::FlineTrap { .. }
                ));
            steps = steps.saturating_sub(executed);
            self.tick_budget -= executed as i32;
            match batch.exit {
                BatchExit::BudgetExhausted | BatchExit::WatchedPc { .. } => {}
                BatchExit::AlineTrap { opcode } => {
                    if self.dispatch_classic_with_process_services(opcode).is_err() {
                        self.halted = true;
                        return;
                    }
                    let extra = hle_trap_extra_tick_cost(opcode)
                        .saturating_add(self.dispatcher.take_hle_tick_cost());
                    self.tick_budget -= extra;
                }
                _ => return,
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::RecordedPass;

    /// Ten steps; the trap at step 3 adds five units, the one ending the
    /// pass (step 10) adds two: 17 units a pass.
    fn pass() -> RecordedPass {
        RecordedPass {
            steps: 10,
            extra_units: vec![(3, 5), (10, 2)],
            start_values: Vec::new(),
        }
    }

    #[test]
    fn a_trap_charges_its_extra_units_at_its_own_step() {
        let pass = pass();
        assert_eq!(pass.units_to(0), 0);
        assert_eq!(pass.units_to(2), 2);
        assert_eq!(pass.units_to(3), 8);
        assert_eq!(pass.units_to(9), 14);
        assert_eq!(pass.units_to(10), 17);
    }

    #[test]
    fn units_after_cross_pass_boundaries() {
        let pass = pass();
        // Steps 9 and 10 (1 + 3), then steps 1..=3 of the next pass (3 + 5).
        assert_eq!(pass.units_after(8, 5), 12);
        // Two whole passes, then steps 1..=5 (5 + 5).
        assert_eq!(pass.units_after(0, 25), 2 * 17 + 10);
        assert_eq!(pass.units_after(7, 0), 0);
        for offset in 0..10 {
            assert_eq!(pass.units_after(offset, 10), 17, "a whole pass from {offset}");
        }
    }

    #[test]
    fn steps_within_is_the_longest_run_that_fits() {
        let pass = pass();
        for offset in 0..10 {
            for units in 0..60 {
                for limit in [0, 7, 40] {
                    let steps = pass.steps_within(offset, limit, units);
                    assert!(steps <= limit);
                    assert!(steps == 0 || pass.units_after(offset, steps) <= units);
                    assert!(
                        steps == limit || pass.units_after(offset, steps + 1) > units,
                        "offset {offset}, {units} units, limit {limit}: {steps} is not the longest"
                    );
                }
            }
        }
    }
}
