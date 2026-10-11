//! PowerPC callback runner and interrupt frame methods on [`PpcLoadedApp`].
//!
//! Handles timer tasks, vertical retrace (VBL) tasks, sound completion,
//! sound doubleback, file completion, and thread switcher callbacks.

use super::*;

impl PpcLoadedApp {
    /// Notification responses retain foreground CPU state between bounded
    /// native slices or while a direct Mixed Mode callee owns execution.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn run_notification_response_with_process_services(
        &mut self, request: u32, instance: u64, completion: u32,
        max_cycles: u64, trace_imports: bool, trace_fetches: bool,
        memory_manager: &mut ProcessMemoryManager, cfm: &mut PpcCfmState,
    ) -> Option<PpcHleRunProbe> {
        self.assert_cfm_execution_owner(Some(cfm));
        if self.parked_interrupt_callback.is_some() { return None; }
        if let Some((r, i, c, _, _)) = self.notification_response_context.as_ref() {
            // A response may NMRemove its own record before it returns. Its
            // in-flight execution still owns this continuation afterward.
            if (*r, *i, *c) != (request, instance, completion) { return None; }
        } else if self.toolbox_startup.notification_requests.instance_id(request) != Some(instance)
            || self.toolbox_startup.notification_requests.response_started(request, instance) {
            return None;
        }
        let saved = self.cpu.capture_execution_context();
        let rtoc = if self.cpu.gpr[2] != 0 { self.cpu.gpr[2] } else { self.rtoc };
        let interrupt = self.interrupt_entry();
        let callback_sp;
        if let Some((_, _, _, context, sp)) = self.notification_response_context.take() {
            callback_sp = sp;
            self.cpu.install_execution_context(context);
        } else {
            let target = crate::guest_procedure::resolve_guest_procedure(&mut self.memory,
                completion, rtoc, None, GuestIsa::PowerPc, GuestIsa::PowerPc)?;
            if target.isa == GuestIsa::M68k && (!interrupt.may_park || target.proc_info == 0) {
                return None;
            }
            let mixed_heap = if target.isa == GuestIsa::M68k {
                Some(memory_manager.native_heap_state()?)
            } else { None };
            callback_sp = self.prepare_interrupt_callback_frame(rtoc)?;
            self.cpu.invalidate_reservation();
            self.cpu.pc = target.entry;
            self.cpu.lr = self.halt_pc;
            self.cpu.gpr[1] = callback_sp;
            self.cpu.gpr[2] = target.rtoc;
            if target.isa == GuestIsa::M68k {
                let heap = mixed_heap.expect("Mixed Mode heap validated before CPU entry");
                let mut cursor = heap.heap_cursor;
                let limit = memory_manager.native_allocation_limit(heap.heap_limit);
                let previous_mixed = self.toolbox_startup.mixed_mode_m68k.snapshot();
                let entered = super::dispatch_mixed_mode::ppc_begin_m68k_universal_proc(
                    &self.cpu, Some(memory_manager.native_mut()), &mut self.memory,
                    &mut cursor, limit, &mut self.toolbox_startup, target, target.proc_info,
                    None, vec![request], self.halt_pc, PpcNativeReturnGpr3::Preserve);
                if entered.is_none() {
                    self.toolbox_startup.mixed_mode_m68k.restore_snapshot(previous_mixed);
                    self.cpu.install_execution_context(saved);
                    return None;
                }
                let started = self.toolbox_startup.notification_requests.begin_response(request, instance);
                debug_assert!(started, "serialized notification ownership changed during entry");
                let probe = PpcHleRunProbe {
                    result: PpcRunResult::Halted { pc: self.cpu.pc, cycles: 0 },
                    handled_import_count: 0, last_import_index: None, unsupported_import_index: None,
                    import_trace: Vec::new(), draw_sprocket_trace: Vec::new(),
                    input_sprocket_trace: Vec::new(), fetch_histogram: None,
                };
                let parked = self.park_interrupt_callback_if_awaiting_m68k(PpcCallbackLevel::Task,
                    interrupt, Some(callback_sp), &probe, &saved, None, PpcInterruptReturnWork::None);
                debug_assert!(parked, "direct notification Mixed Mode entry must retain its caller");
                return Some(probe);
            }
            if install_powerpc_call_arguments(&mut self.cpu, &mut self.memory, &[request]).is_none() {
                self.cpu.install_execution_context(saved);
                return None;
            }
            if !self.toolbox_startup.notification_requests.begin_response(request, instance) {
                self.cpu.install_execution_context(saved);
                return None;
            }
        }
        let probe = self.run_with_hle_imports_with_trace(max_cycles, trace_imports,
            trace_fetches, Some(memory_manager), Some(cfm));
        let parked = self.park_interrupt_callback_if_awaiting_m68k(PpcCallbackLevel::Task,
            interrupt, Some(callback_sp), &probe, &saved, None, PpcInterruptReturnWork::None);
        if !parked {
            if matches!(probe.result, PpcRunResult::CycleLimit { .. }) {
                self.notification_response_context = Some((request, instance, completion,
                    self.cpu.capture_execution_context(), callback_sp));
            }
            self.cpu.install_execution_context(saved);
        }
        Some(probe)
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn run_sound_completion_callback_with_process_services(
        &mut self,
        completion: PpcSoundCompletionRecord,
        max_cycles: u64,
        trace_imports: bool,
        trace_fetches: bool,
        memory_manager: &mut ProcessMemoryManager,
        cfm: &mut PpcCfmState,
    ) -> PpcSoundCompletionCallProbe {
        self.run_sound_completion_callback_inner(
            completion,
            max_cycles,
            trace_imports,
            trace_fetches,
            Some(memory_manager),
            Some(cfm),
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn fire_timer_tasks_for_ticks_with_process_services(
        &mut self,
        start_tick: u32,
        elapsed_ticks: u32,
        max_callbacks: usize,
        max_cycles: u64,
        trace_imports: bool,
        trace_fetches: bool,
        memory_manager: &mut ProcessMemoryManager,
        cfm: &mut PpcCfmState,
    ) -> Vec<PpcTimerCallbackProbe> {
        self.fire_timer_tasks_for_ticks_inner(
            start_tick,
            elapsed_ticks,
            max_callbacks,
            max_cycles,
            trace_imports,
            trace_fetches,
            Some(memory_manager),
            Some(cfm),
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn fire_vbl_tasks_for_ticks_with_process_services(
        &mut self,
        start_tick: u32,
        elapsed_ticks: u32,
        max_callbacks: usize,
        max_cycles: u64,
        trace_imports: bool,
        trace_fetches: bool,
        memory_manager: &mut ProcessMemoryManager,
        cfm: &mut PpcCfmState,
    ) -> Vec<PpcVblCallbackProbe> {
        self.fire_vbl_tasks_for_ticks_inner(
            start_tick,
            elapsed_ticks,
            max_callbacks,
            max_cycles,
            trace_imports,
            trace_fetches,
            Some(memory_manager),
            Some(cfm),
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn run_sound_doubleback_callback_with_process_services(
        &mut self,
        doubleback: PpcSoundDoubleBackRecord,
        max_cycles: u64,
        trace_imports: bool,
        trace_fetches: bool,
        memory_manager: &mut ProcessMemoryManager,
        cfm: &mut PpcCfmState,
    ) -> PpcSoundCompletionCallProbe {
        self.run_sound_doubleback_callback_inner(
            doubleback,
            max_cycles,
            trace_imports,
            trace_fetches,
            Some(memory_manager),
            Some(cfm),
        )
    }

    pub fn run_sound_completion_callback(
        &mut self,
        completion: PpcSoundCompletionRecord,
        max_cycles: u64,
        trace_imports: bool,
        trace_fetches: bool,
    ) -> PpcSoundCompletionCallProbe {
        self.run_sound_completion_callback_inner(
            completion,
            max_cycles,
            trace_imports,
            trace_fetches,
            None,
            None,
        )
    }

    /// PowerPC System Software (1994), pp. 1-44–1-49: asynchronous
    /// callbacks need a private frame below the interrupted 224-byte Red Zone.
    /// Validate the engine owner's allocation and the complete writable frame
    /// before touching linkage, parameters or the interrupted registers.
    fn prepare_interrupt_callback_frame(&mut self, rtoc: u32) -> Option<u32> {
        let (base, limit) = self.toolbox_startup.execution.calls().native_stack_bounds(
            self.stack_base,
            self.stack_base.checked_add(self.stack_size)?,
        )?;
        self.prepare_interrupt_callback_frame_in_bounds(rtoc, base, limit)
    }

    fn prepare_interrupt_callback_frame_in_bounds(
        &mut self,
        rtoc: u32,
        base: u32,
        limit: u32,
    ) -> Option<u32> {
        let interrupted_sp = self.cpu.gpr[1];
        let frame_sp = interrupted_sp
            .checked_sub(PPC_INTERRUPT_RED_ZONE_SIZE)?
            .checked_sub(PPC_INITIAL_STACK_FRAME_SIZE)?
            & !15;
        if frame_sp < base
            || interrupted_sp > limit
            || !ppc_memory_can_write_bytes(&mut self.memory, frame_sp, PPC_INITIAL_STACK_FRAME_SIZE)
        {
            return None;
        }
        if !ppc_zero_guest_bytes(&mut self.memory, frame_sp, PPC_INITIAL_STACK_FRAME_SIZE) {
            return None;
        }
        self.memory
            .write_u32_be(frame_sp + PPC_LINKAGE_BACK_CHAIN_OFFSET, interrupted_sp)?;
        self.memory
            .write_u32_be(frame_sp + PPC_LINKAGE_SAVED_CR_OFFSET, self.cpu.cr)?;
        self.memory
            .write_u32_be(frame_sp + PPC_LINKAGE_SAVED_LR_OFFSET, self.cpu.lr)?;
        self.memory
            .write_u32_be(frame_sp + PPC_LINKAGE_SAVED_RTOC_OFFSET, rtoc)?;
        Some(frame_sp)
    }

    /// Whether interrupt-level delivery (VBL, DrawSprocket VBL, Time
    /// Manager, sound and File Manager completions) waits for a parked
    /// callback. A parked interrupt-level callback masks it until it returns,
    /// as the real interrupt handler would. A parked event-loop timer runs at
    /// task level, so interrupts still arrive while its 68K call is pending,
    /// as they do over foreground code waiting on a 68K call. Once that call
    /// has returned, the timer's continuation holds the CPU: an interrupt
    /// fired then would save the continuation as its interrupted context and
    /// could not itself park, so it waits until the timer returns.
    pub(crate) fn interrupts_masked(&self) -> bool {
        self.parked_interrupt_callback
            .as_ref()
            .is_some_and(|parked| match parked.level {
                PpcCallbackLevel::Interrupt => true,
                PpcCallbackLevel::Task => self
                    .toolbox_startup
                    .execution
                    .calls()
                    .task_top_call_id(parked.task)
                    .is_none_or(|call| call < parked.awaited_call),
            })
    }

    /// Event-loop timers wait for any parked callback, so a timer is never
    /// re-entered and never runs over a parked callback's registers.
    pub(crate) fn event_loop_timers_masked(&self) -> bool {
        self.parked_interrupt_callback.is_some()
    }

    /// The parked callback's 68K call has returned, so `cpu` holds the
    /// callback's continuation. It runs before any other 68K frame on its
    /// task; a later call the continuation submits has a larger ID and runs
    /// first.
    pub(crate) fn interrupt_continuation_ready(&self) -> bool {
        let calls = self.toolbox_startup.execution.calls();
        self.parked_interrupt_callback
            .as_ref()
            .is_some_and(|parked| {
                parked.task == calls.current_task()
                    && calls
                        .top_call_id()
                        .is_none_or(|call| call < parked.awaited_call)
            })
    }

    fn interrupt_entry(&self) -> PpcInterruptEntry {
        let calls = self.toolbox_startup.execution.calls();
        PpcInterruptEntry {
            task: calls.current_task(),
            top_call: calls.top_call_id(),
            // A callback that interrupts code already inside, or waiting on,
            // a 68K call keeps the old restore path: its own 68K call would
            // share that call's 68K registers and stack. Only one callback
            // is parked at a time.
            may_park: self.parked_interrupt_callback.is_none()
                && calls.current_native_task_owns_cpu()
                && !calls.current_task_has_m68k_frames(),
        }
    }

    /// A callback that stopped on a 68K Mixed Mode call it submitted stays
    /// installed: that call captures the callback's registers when it starts
    /// and resumes the callback when it returns. Park what it interrupted
    /// until the callback returns to `halt_pc`. Returns true when parked; the
    /// caller then skips its restore.
    ///
    /// A parked callback behaves like an interrupt handler that has not
    /// returned yet, not like a callback slice:
    /// - Its continuation runs in the foreground's native slices, without
    ///   the per-callback cycle cap.
    /// - A fault after its 68K call halts the application, where an
    ///   unparked callback that faults is abandoned and the interrupted
    ///   context restored.
    /// - If the 68K callee yields to another thread, interrupt-level
    ///   delivery stays masked until the owning task resumes and the
    ///   callback returns.
    #[allow(clippy::too_many_arguments)]
    fn park_interrupt_callback_if_awaiting_m68k(
        &mut self,
        level: PpcCallbackLevel,
        entry: PpcInterruptEntry,
        callback_sp: Option<u32>,
        probe: &PpcHleRunProbe,
        interrupted: &PpcExecutionContext,
        interrupted_refnum: Option<i16>,
        on_return: PpcInterruptReturnWork,
    ) -> bool {
        let Some(callback_sp) = callback_sp else {
            return false;
        };
        if !entry.may_park
            || probe.unsupported_import_index.is_some()
            || !matches!(probe.result, PpcRunResult::Halted { pc, .. } if pc != self.halt_pc)
        {
            return false;
        }
        let calls = self.toolbox_startup.execution.calls();
        if calls.current_task() != entry.task {
            return false;
        }
        let Some(awaited_call) = calls
            .unstarted_m68k_call()
            .filter(|call| entry.top_call.is_none_or(|top| *call > top))
        else {
            return false;
        };
        self.interrupt_callback_parks = self.interrupt_callback_parks.saturating_add(1);
        if ppc_hle_trace_enabled() {
            eprintln!(
                "[PPC-TRACE] {level:?}-level callback parked on 68K call: pc=${:08X} callback_sp=${callback_sp:08X} interrupted_pc=${:08X} parks={}",
                self.cpu.pc,
                interrupted.architectural().pc,
                self.interrupt_callback_parks,
            );
        }
        self.parked_interrupt_callback = Some(PpcParkedInterruptCallback {
            level,
            task: entry.task,
            awaited_call,
            callback_sp,
            interrupted: interrupted.clone(),
            interrupted_refnum,
            on_return,
        });
        true
    }

    /// Runs once the callback has really returned and every process record
    /// is back in place.
    pub(super) fn finish_interrupt_return_work(&mut self, work: PpcInterruptReturnWork) {
        match work {
            PpcInterruptReturnWork::None => {}
            PpcInterruptReturnWork::VblTask { task_ptr } => {
                self.retire_vbl_task_if_unscheduled(task_ptr);
            }
            PpcInterruptReturnWork::DoubleBack(doubleback) => {
                self.sound
                    .manager
                    .with_mut(|sound| sound.clear_doubleback_pending(&doubleback));
            }
        }
    }

    /// Inside Macintosh: Processes (1994), pp. 4-7–4-8: a VBL task must reset
    /// vblCount from its callback or the Vertical Retrace Manager removes the
    /// task after that execution.
    fn retire_vbl_task_if_unscheduled(&mut self, task_ptr: u32) {
        if self.memory.read_u16_be(task_ptr + 10) == Some(0) {
            self.vbl_tasks.with_mut(|vbl_tasks| {
                vbl_tasks.retain(|installed| installed.task_ptr != task_ptr);
                ppc_sync_vbl_task_links(&mut self.memory, vbl_tasks);
            });
        }
    }

    fn interrupt_callback_stack_fault(&self) -> PpcHleRunProbe {
        PpcHleRunProbe {
            result: PpcRunResult::MemoryFault {
                pc: self.cpu.pc,
                addr: ppc_interrupt_callback_stack_pointer(self.cpu.gpr[1]),
                was_write: true,
                cycles: 0,
            },
            handled_import_count: 0,
            last_import_index: None,
            unsupported_import_index: None,
            import_trace: Vec::new(),
            draw_sprocket_trace: Vec::new(),
            input_sprocket_trace: Vec::new(),
            fetch_histogram: None,
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn run_file_completion_callback_with_process_services(
        &mut self,
        parameter_block: u32,
        completion: u32,
        max_cycles: u64,
        trace_imports: bool,
        trace_fetches: bool,
        memory_manager: &mut ProcessMemoryManager,
        cfm: &mut PpcCfmState,
    ) -> PpcHleRunProbe {
        self.assert_cfm_execution_owner(Some(cfm));
        let saved_context = self.cpu.capture_execution_context();
        let continuing = self.file_completion_context.take();
        let default_rtoc = if saved_context.architectural().gpr[2] != 0 {
            saved_context.architectural().gpr[2]
        } else {
            self.rtoc
        };
        let target = ppc_resolve_callback_target(&mut self.memory, completion, default_rtoc, None)
            .unwrap_or(PpcCallbackTarget {
                entry: completion,
                rtoc: default_rtoc,
                proc_info: 0,
                routine_flags: 0,
            });
        if std::env::var_os("SYSTEMLESS_PPC_FILE_TRACE").is_some() {
            eprintln!(
                "[PPC-FILE-TRACE] completion target ptr=${completion:08X} entry=${:08X} rtoc=${:08X} words={:?},{:?}",
                target.entry,
                target.rtoc,
                self.memory.read_u32_be(completion),
                self.memory.read_u32_be(completion.wrapping_add(4)),
            );
        }
        let interrupt = self.interrupt_entry();
        let mut callback_frame = None;
        let probe = if let Some((context, callback_sp)) = continuing {
            callback_frame = Some(callback_sp);
            self.cpu.install_execution_context(context);
            self.run_with_hle_imports_with_trace(
                max_cycles,
                trace_imports,
                trace_fetches,
                Some(memory_manager),
                Some(cfm),
            )
        } else if let Some(callback_sp) = self.prepare_interrupt_callback_frame(default_rtoc) {
            callback_frame = Some(callback_sp);
            self.cpu.invalidate_reservation();
            self.cpu.pc = target.entry;
            self.cpu.lr = self.halt_pc;
            self.cpu.gpr[1] = callback_sp;
            self.cpu.gpr[2] = target.rtoc;
            let _ =
                install_powerpc_call_arguments(&mut self.cpu, &mut self.memory, &[parameter_block]);
            self.run_with_hle_imports_with_trace(
                max_cycles,
                trace_imports,
                trace_fetches,
                Some(memory_manager),
                Some(cfm),
            )
        } else {
            self.interrupt_callback_stack_fault()
        };
        let parked = self.park_interrupt_callback_if_awaiting_m68k(
            PpcCallbackLevel::Interrupt,
            interrupt,
            callback_frame,
            &probe,
            &saved_context,
            None,
            PpcInterruptReturnWork::None,
        );
        if !parked {
            // Inside Macintosh: Files (1992), "Completion Routines": the callback
            // completes after I/O; a runner slice is not a callback completion.
            if matches!(probe.result, PpcRunResult::CycleLimit { .. }) {
                self.file_completion_context = callback_frame
                    .map(|callback_sp| (self.cpu.capture_execution_context(), callback_sp));
            }
            self.cpu.install_execution_context(saved_context);
        }
        probe
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn run_thread_switcher_callback_with_process_services(
        &mut self,
        thread_id: u32,
        procedure: u32,
        parameter: u32,
        thread_context: Option<PpcExecutionContext>,
        max_cycles: u64,
        trace_imports: bool,
        trace_fetches: bool,
        memory_manager: &mut ProcessMemoryManager,
        cfm: &mut PpcCfmState,
    ) -> PpcHleRunProbe {
        self.assert_cfm_execution_owner(Some(cfm));
        let saved_context = self.cpu.capture_execution_context();
        let outgoing_context = thread_context.is_some();
        if let Some(context) = thread_context {
            self.cpu.install_execution_context(context);
        }
        let default_rtoc = if self.cpu.gpr[2] != 0 {
            self.cpu.gpr[2]
        } else {
            self.rtoc
        };
        let target = ppc_resolve_callback_target(&mut self.memory, procedure, default_rtoc, None)
            .unwrap_or(PpcCallbackTarget {
                entry: procedure,
                rtoc: default_rtoc,
                proc_info: 0,
                routine_flags: 0,
            });
        let callback_bounds = if outgoing_context {
            let task = crate::guest_call::ExecutionTaskId::from_thread_id(thread_id);
            if task == crate::guest_call::ExecutionTaskId::APPLICATION {
                self.stack_base
                    .checked_add(self.stack_size)
                    .map(|limit| (self.stack_base, limit))
            } else {
                self.toolbox_startup
                    .execution
                    .calls()
                    .thread_storage(task)
                    .map(|storage| (storage.stack_base, storage.stack_limit))
            }
        } else {
            None
        };
        let callback_sp = if let Some((base, limit)) = callback_bounds {
            self.prepare_interrupt_callback_frame_in_bounds(default_rtoc, base, limit)
        } else {
            self.prepare_interrupt_callback_frame(default_rtoc)
        };
        // Switchers run at task level, not interrupt level, so they are never
        // parked; report one that leaves a 68K call behind.
        let trace_thread = std::env::var_os("SYSTEMLESS_PPC_THREAD_TRACE").is_some();
        let top_call = trace_thread
            .then(|| self.toolbox_startup.execution.calls().top_call_id())
            .flatten();
        let probe = if let Some(callback_sp) = callback_sp {
            self.cpu.invalidate_reservation();
            self.cpu.pc = target.entry;
            self.cpu.lr = self.halt_pc;
            self.cpu.gpr[1] = callback_sp;
            self.cpu.gpr[2] = target.rtoc;
            let _ = install_powerpc_call_arguments(
                &mut self.cpu,
                &mut self.memory,
                &[thread_id, parameter],
            );
            self.run_with_hle_imports_with_trace(
                max_cycles,
                trace_imports,
                trace_fetches,
                Some(memory_manager),
                Some(cfm),
            )
        } else {
            self.interrupt_callback_stack_fault()
        };
        if trace_thread {
            if let Some(call) = self
                .toolbox_startup
                .execution
                .calls()
                .unstarted_m68k_call()
                .filter(|call| top_call.is_none_or(|top| *call > top))
            {
                eprintln!(
                    "[PPC-THREAD-TRACE] switcher thread={thread_id} left 68K call {call:?} unstarted pc=${:08X}",
                    self.cpu.pc
                );
            }
        }
        self.cpu.install_execution_context(saved_context);
        probe
    }

    fn run_sound_completion_callback_inner(
        &mut self,
        completion: PpcSoundCompletionRecord,
        max_cycles: u64,
        trace_imports: bool,
        trace_fetches: bool,
        process_memory_manager: Option<&mut ProcessMemoryManager>,
        mut process_cfm: Option<&mut PpcCfmState>,
    ) -> PpcSoundCompletionCallProbe {
        self.assert_cfm_execution_owner(process_cfm.as_deref());
        let saved_context = self.cpu.capture_execution_context();
        let default_rtoc = if saved_context.architectural().gpr[2] != 0 {
            saved_context.architectural().gpr[2]
        } else {
            self.rtoc
        };
        let target = ppc_resolve_callback_target(
            &mut self.memory,
            completion.completion,
            default_rtoc,
            None,
        )
        .unwrap_or(PpcCallbackTarget {
            entry: completion.completion,
            rtoc: default_rtoc,
            proc_info: 0,
            routine_flags: 0,
        });
        let interrupt = self.interrupt_entry();
        let mut callback_frame = None;
        let mut entered = false;
        let probe = if let Some(callback_sp) = self.prepare_interrupt_callback_frame(default_rtoc) {
            self.cpu.invalidate_reservation();
            entered = true;
            callback_frame = Some(callback_sp);
            self.cpu.pc = target.entry;
            self.cpu.lr = self.halt_pc;
            self.cpu.gpr[1] = callback_sp;
            self.cpu.gpr[2] = target.rtoc;
            let command_ptr = completion.command.and_then(|command| {
                // The PowerPC SndCallBackProcPtr signature is
                // (SndChannelPtr, SndCommand *). Keep the copied command in the
                // final eight bytes of the 64-byte callback frame, after its
                // linkage and eight-word parameter areas and before the
                // interrupted routine's protected Red Zone.
                let command_ptr = self.cpu.gpr[1].checked_add(
                    PPC_PARAMETER_AREA_OFFSET + PPC_NATIVE_PARAMETER_GPR_COUNT as u32 * 4,
                )?;
                self.memory.write_u16_be(command_ptr, command.command)?;
                self.memory
                    .write_u16_be(command_ptr + 2, command.param1 as u16)?;
                self.memory.write_u32_be(command_ptr + 4, command.param2)?;
                Some(command_ptr)
            });
            let _ = install_powerpc_call_arguments(
                &mut self.cpu,
                &mut self.memory,
                &[completion.channel, command_ptr.unwrap_or(0)],
            );

            self.run_with_hle_imports_with_trace(
                max_cycles,
                trace_imports,
                trace_fetches,
                process_memory_manager,
                process_cfm.as_deref_mut(),
            )
        } else {
            self.interrupt_callback_stack_fault()
        };
        let end_pc = self.cpu.pc;
        let end_sp = self.cpu.gpr[1];
        let end_r3 = self.cpu.gpr[3];
        let cycles = ppc_run_result_cycles(probe.result);
        let parked = self.park_interrupt_callback_if_awaiting_m68k(
            PpcCallbackLevel::Interrupt,
            interrupt,
            callback_frame,
            &probe,
            &saved_context,
            None,
            PpcInterruptReturnWork::None,
        );
        if !parked && entered {
            self.cpu.install_execution_context(saved_context);
        }

        PpcSoundCompletionCallProbe {
            invocation: PpcSoundCompletionInvocationRecord {
                file_playback_index: completion.file_playback_index,
                channel: completion.channel,
                completion: completion.completion,
                callback_entry: target.entry,
                callback_rtoc: target.rtoc,
                tick: completion.tick,
                instruction_count: completion.instruction_count,
                scheduled_tick: completion.scheduled_tick,
                scheduled_instruction_count: completion.scheduled_instruction_count,
                cycles,
                end_pc,
                end_sp,
                end_r3,
                result: probe.result,
                unsupported_import_index: probe.unsupported_import_index,
            },
            import_trace: probe.import_trace,
            fetch_histogram: probe.fetch_histogram,
        }
    }

    pub fn fire_timer_tasks_for_ticks(
        &mut self,
        start_tick: u32,
        elapsed_ticks: u32,
        max_callbacks: usize,
        max_cycles: u64,
        trace_imports: bool,
        trace_fetches: bool,
    ) -> Vec<PpcTimerCallbackProbe> {
        self.fire_timer_tasks_for_ticks_inner(
            start_tick,
            elapsed_ticks,
            max_callbacks,
            max_cycles,
            trace_imports,
            trace_fetches,
            None,
            None,
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn fire_timer_tasks_for_ticks_inner(
        &mut self,
        start_tick: u32,
        elapsed_ticks: u32,
        max_callbacks: usize,
        max_cycles: u64,
        trace_imports: bool,
        trace_fetches: bool,
        mut process_memory_manager: Option<&mut ProcessMemoryManager>,
        mut process_cfm: Option<&mut PpcCfmState>,
    ) -> Vec<PpcTimerCallbackProbe> {
        self.assert_cfm_execution_owner(process_cfm.as_deref());
        let mut probes = Vec::new();
        if elapsed_ticks == 0 || self.timer_tasks.is_empty() || max_callbacks == 0 {
            return probes;
        }
        for tick_offset in 0..elapsed_ticks {
            let current_tick = start_tick.wrapping_add(tick_offset).wrapping_add(1);
            let current_tick = self.publish_tick(current_tick);
            self.callback_scheduling
                .set_current_subtick(u64::from(current_tick) * 1_000_000);
            loop {
                // Interrupts stay masked while a callback waits on a 68K call;
                // due tasks stay due and fire after it returns.
                if self.interrupts_masked() {
                    break;
                }
                if probes.len() >= max_callbacks {
                    return probes;
                }
                let Some(task) = self
                    .timer_tasks
                    .iter()
                    .copied()
                    .filter(|task| {
                        task.architecture == CallbackTaskArchitecture::PowerPc
                            && task.active
                            && task.last_fired_tick != Some(current_tick)
                            && current_tick.wrapping_sub(task.fire_at_tick) < 0x8000_0000
                    })
                    .min_by_key(|task| task.fire_at_tick)
                else {
                    break;
                };
                self.timer_tasks.with_mut(|timer_tasks| {
                    if let Some(installed) = timer_tasks
                        .iter_mut()
                        .find(|installed| installed.task_ptr == task.task_ptr)
                    {
                        installed.active = false;
                        installed.last_fired_tick = Some(current_tick);
                    }
                });
                let q_type = self.memory.read_u16_be(task.task_ptr + 4).unwrap_or(0);
                let _ = self.memory.write_u16_be(task.task_ptr + 4, q_type & 0x7fff);
                if task.callback != 0 {
                    probes.push(self.run_timer_callback(
                        task.task_ptr,
                        task.callback,
                        max_cycles,
                        trace_imports,
                        trace_fetches,
                        process_memory_manager.as_deref_mut(),
                        process_cfm.as_deref_mut(),
                    ));
                }
            }
        }
        probes
    }

    /// Deliver Carbon event-loop timers only while the guest is polling or
    /// waiting in an event loop. Carbon Event Manager Programming Guide
    /// (2005), "Installing Timers": these are not interrupt callbacks.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn fire_event_loop_timers_for_ticks_with_process_services(
        &mut self,
        start_tick: u32,
        elapsed_ticks: u32,
        max_callbacks: usize,
        max_cycles: u64,
        trace_imports: bool,
        trace_fetches: bool,
        memory_manager: &mut ProcessMemoryManager,
        cfm: &mut PpcCfmState,
    ) -> Vec<PpcTimerCallbackProbe> {
        self.fire_event_loop_timers_for_ticks_inner(
            start_tick,
            elapsed_ticks,
            max_callbacks,
            max_cycles,
            trace_imports,
            trace_fetches,
            Some(memory_manager),
            Some(cfm),
        )
    }

    #[cfg(test)]
    pub(crate) fn fire_event_loop_timers_for_ticks(
        &mut self,
        start_tick: u32,
        elapsed_ticks: u32,
        max_callbacks: usize,
        max_cycles: u64,
        trace_imports: bool,
        trace_fetches: bool,
    ) -> Vec<PpcTimerCallbackProbe> {
        self.fire_event_loop_timers_for_ticks_inner(
            start_tick,
            elapsed_ticks,
            max_callbacks,
            max_cycles,
            trace_imports,
            trace_fetches,
            None,
            None,
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn fire_event_loop_timers_for_ticks_inner(
        &mut self,
        start_tick: u32,
        elapsed_ticks: u32,
        max_callbacks: usize,
        max_cycles: u64,
        trace_imports: bool,
        trace_fetches: bool,
        mut memory_manager: Option<&mut ProcessMemoryManager>,
        mut cfm: Option<&mut PpcCfmState>,
    ) -> Vec<PpcTimerCallbackProbe> {
        let mut probes = Vec::new();
        if elapsed_ticks == 0 || max_callbacks == 0 {
            return probes;
        }
        for offset in 0..elapsed_ticks {
            let current_tick = self.publish_tick(start_tick.wrapping_add(offset).wrapping_add(1));
            let is_polling = self
                .toolbox_startup
                .event_loop_poll_until_tick
                .is_some_and(|until| dispatch_event::ppc_tick_is_due(until, current_tick));
            if !is_polling {
                continue;
            }
            loop {
                if self.event_loop_timers_masked() {
                    break;
                }
                if probes.len() >= max_callbacks {
                    return probes;
                }
                let Some(index) = self
                    .toolbox_startup
                    .event_loop_timers
                    .iter()
                    .enumerate()
                    .filter(|(_, timer)| {
                        timer.next_fire_tick.is_some_and(|deadline| {
                            dispatch_event::ppc_tick_is_due(current_tick, deadline)
                        })
                    })
                    .max_by_key(|(_, timer)| {
                        current_tick.wrapping_sub(timer.next_fire_tick.unwrap())
                    })
                    .map(|(index, _)| index)
                else {
                    break;
                };
                let timer = self.toolbox_startup.event_loop_timers[index];
                self.toolbox_startup.event_loop_timers[index].next_fire_tick =
                    (timer.interval_ticks != 0)
                        .then(|| current_tick.wrapping_add(timer.interval_ticks));
                probes.push(self.run_timer_callback_with_arguments(
                    timer.timer_ref,
                    timer.callback,
                    &[timer.timer_ref, timer.user_data],
                    PpcCallbackLevel::Task,
                    max_cycles,
                    trace_imports,
                    trace_fetches,
                    memory_manager.as_deref_mut(),
                    cfm.as_deref_mut(),
                ));
            }
        }
        probes
    }

    /// Fixtures install a timer on an event loop that is polling until
    /// `poll_until_tick`.
    #[cfg(test)]
    pub(crate) fn install_test_event_loop_timer(
        &mut self,
        timer_ref: u32,
        callback: u32,
        first_tick: u32,
        interval_ticks: u32,
        poll_until_tick: u32,
    ) {
        self.toolbox_startup.event_loop_poll_until_tick = Some(poll_until_tick);
        self.toolbox_startup
            .event_loop_timers
            .push(dispatch_event::PpcEventLoopTimerRecord {
                timer_ref,
                callback,
                user_data: 0,
                next_fire_tick: Some(first_tick),
                interval_ticks,
            });
    }

    pub(crate) fn run_timer_callback(
        &mut self,
        task_ptr: u32,
        callback: u32,
        max_cycles: u64,
        trace_imports: bool,
        trace_fetches: bool,
        process_memory_manager: Option<&mut ProcessMemoryManager>,
        process_cfm: Option<&mut PpcCfmState>,
    ) -> PpcTimerCallbackProbe {
        self.run_timer_callback_with_arguments(
            task_ptr,
            callback,
            &[task_ptr],
            PpcCallbackLevel::Interrupt,
            max_cycles,
            trace_imports,
            trace_fetches,
            process_memory_manager,
            process_cfm,
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn run_timer_callback_with_arguments(
        &mut self,
        task_ptr: u32,
        callback: u32,
        arguments: &[u32],
        level: PpcCallbackLevel,
        max_cycles: u64,
        trace_imports: bool,
        trace_fetches: bool,
        process_memory_manager: Option<&mut ProcessMemoryManager>,
        mut process_cfm: Option<&mut PpcCfmState>,
    ) -> PpcTimerCallbackProbe {
        self.assert_cfm_execution_owner(process_cfm.as_deref());
        let saved_context = self.cpu.capture_execution_context();
        let saved_current_resource_refnum = self.current_resource_refnum();
        let default_rtoc = if saved_context.architectural().gpr[2] != 0 {
            saved_context.architectural().gpr[2]
        } else {
            self.rtoc
        };
        let target = ppc_resolve_callback_target(&mut self.memory, callback, default_rtoc, None)
            .unwrap_or(PpcCallbackTarget {
                entry: callback,
                rtoc: default_rtoc,
                proc_info: 0,
                routine_flags: 0,
            });
        let interrupt = self.interrupt_entry();
        let mut callback_frame = None;
        let mut entered = false;
        let probe = if let Some(callback_sp) = self.prepare_interrupt_callback_frame(default_rtoc) {
            self.cpu.invalidate_reservation();
            entered = true;
            callback_frame = Some(callback_sp);
            self.cpu.pc = target.entry;
            self.cpu.lr = self.halt_pc;
            self.cpu.gpr[1] = callback_sp;
            self.cpu.gpr[2] = target.rtoc;
            // Mixed Mode marshals the documented callback parameters into
            // the native PowerPC argument area.
            let _ = install_powerpc_call_arguments(&mut self.cpu, &mut self.memory, arguments);
            self.run_with_hle_imports_with_trace(
                max_cycles,
                trace_imports,
                trace_fetches,
                process_memory_manager,
                process_cfm.as_deref_mut(),
            )
        } else {
            self.interrupt_callback_stack_fault()
        };
        let end_pc = self.cpu.pc;
        let end_sp = self.cpu.gpr[1];
        let end_r3 = self.cpu.gpr[3];
        let cycles = ppc_run_result_cycles(probe.result);
        let parked = self.park_interrupt_callback_if_awaiting_m68k(
            level,
            interrupt,
            callback_frame,
            &probe,
            &saved_context,
            Some(saved_current_resource_refnum),
            PpcInterruptReturnWork::None,
        );
        if !parked {
            if entered {
                self.cpu.install_execution_context(saved_context);
            }
            self.set_current_resource_refnum(saved_current_resource_refnum);
        }

        PpcTimerCallbackProbe {
            invocation: PpcTimerCallbackInvocationRecord {
                task_ptr,
                callback,
                callback_entry: target.entry,
                callback_rtoc: target.rtoc,
                tick: self.current_tick(),
                cycles,
                end_pc,
                end_sp,
                end_r3,
                result: probe.result,
                unsupported_import_index: probe.unsupported_import_index,
            },
            import_trace: probe.import_trace,
            fetch_histogram: probe.fetch_histogram,
        }
    }

    pub fn fire_vbl_tasks_for_ticks(
        &mut self,
        start_tick: u32,
        elapsed_ticks: u32,
        max_callbacks: usize,
        max_cycles: u64,
        trace_imports: bool,
        trace_fetches: bool,
    ) -> Vec<PpcVblCallbackProbe> {
        self.fire_vbl_tasks_for_ticks_inner(
            start_tick,
            elapsed_ticks,
            max_callbacks,
            max_cycles,
            trace_imports,
            trace_fetches,
            None,
            None,
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn fire_vbl_tasks_for_ticks_inner(
        &mut self,
        start_tick: u32,
        elapsed_ticks: u32,
        max_callbacks: usize,
        max_cycles: u64,
        trace_imports: bool,
        trace_fetches: bool,
        mut process_memory_manager: Option<&mut ProcessMemoryManager>,
        mut process_cfm: Option<&mut PpcCfmState>,
    ) -> Vec<PpcVblCallbackProbe> {
        self.assert_cfm_execution_owner(process_cfm.as_deref());
        let mut probes = Vec::new();
        let has_dsp_vbl = self.draw_sprocket.vbl_proc.is_some()
            && self.draw_sprocket.context_state == PpcDspContextPlayState::Active;
        if elapsed_ticks == 0 || (self.vbl_tasks.is_empty() && !has_dsp_vbl) || max_callbacks == 0 {
            return probes;
        }
        for tick_offset in 0..elapsed_ticks {
            let current_tick =
                self.publish_tick(start_tick.wrapping_add(tick_offset).wrapping_add(1));
            self.callback_scheduling
                .set_current_subtick(u64::from(current_tick) * 1_000_000);
            // The Vertical Retrace Manager does not re-enter its queue while a
            // task is still running; masked ticks do not count down vblCount.
            if self.interrupts_masked() {
                continue;
            }
            if let (Some(context), Some(vbl_proc)) = (
                self.draw_sprocket.active_context,
                self.draw_sprocket.vbl_proc,
            ) {
                if self.draw_sprocket.context_state == PpcDspContextPlayState::Active {
                    let refcon = self.draw_sprocket.vbl_refcon.unwrap_or(0);
                    if probes.len() < max_callbacks {
                        probes.push(self.run_draw_sprocket_vbl_callback(
                            context,
                            refcon,
                            vbl_proc,
                            max_cycles,
                            trace_imports,
                            trace_fetches,
                            process_memory_manager.as_deref_mut(),
                            process_cfm.as_deref_mut(),
                        ));
                    }
                }
            }
            let tasks = (*self.vbl_tasks).clone();
            for task in tasks {
                if task.architecture != CallbackTaskArchitecture::PowerPc {
                    continue;
                }
                if self.interrupts_masked() {
                    break;
                }
                if probes.len() >= max_callbacks {
                    return probes;
                }
                let Some(count) = self.memory.read_u16_be(task.task_ptr + 10) else {
                    continue;
                };
                if count == 0 {
                    continue;
                }
                let next_count = count.saturating_sub(1);
                let _ = self.memory.write_u16_be(task.task_ptr + 10, next_count);
                if next_count != 0 {
                    continue;
                }
                let callback = self.memory.read_u32_be(task.task_ptr + 6).unwrap_or(0);
                if callback == 0 {
                    continue;
                }
                let parks = self.interrupt_callback_parks;
                probes.push(self.run_vbl_callback(
                    task.task_ptr,
                    callback,
                    max_cycles,
                    trace_imports,
                    trace_fetches,
                    process_memory_manager.as_deref_mut(),
                    process_cfm.as_deref_mut(),
                ));
                // A parked task is retired, if at all, when it really returns.
                if self.interrupt_callback_parks != parks {
                    break;
                }
                self.retire_vbl_task_if_unscheduled(task.task_ptr);
            }
        }
        probes
    }

    pub(crate) fn run_vbl_callback(
        &mut self,
        task_ptr: u32,
        callback: u32,
        max_cycles: u64,
        trace_imports: bool,
        trace_fetches: bool,
        process_memory_manager: Option<&mut ProcessMemoryManager>,
        mut process_cfm: Option<&mut PpcCfmState>,
    ) -> PpcVblCallbackProbe {
        self.assert_cfm_execution_owner(process_cfm.as_deref());
        let saved_context = self.cpu.capture_execution_context();
        let saved_current_resource_refnum = self.current_resource_refnum();
        let default_rtoc = if saved_context.architectural().gpr[2] != 0 {
            saved_context.architectural().gpr[2]
        } else {
            self.rtoc
        };
        let target = ppc_resolve_callback_target(&mut self.memory, callback, default_rtoc, None)
            .unwrap_or(PpcCallbackTarget {
                entry: callback,
                rtoc: default_rtoc,
                proc_info: 0,
                routine_flags: 0,
            });
        let interrupt = self.interrupt_entry();
        let mut callback_frame = None;
        let mut entered = false;
        let probe = if let Some(callback_sp) = self.prepare_interrupt_callback_frame(default_rtoc) {
            self.cpu.invalidate_reservation();
            entered = true;
            callback_frame = Some(callback_sp);
            self.cpu.pc = target.entry;
            self.cpu.lr = self.halt_pc;
            self.cpu.gpr[1] = callback_sp;
            self.cpu.gpr[2] = target.rtoc;
            // Inside Macintosh: Processes (1994), p. 4-12: the Vertical Retrace
            // Manager passes the VBL task record in A0 so a repetitive task can
            // reset vblCount. Inside Macintosh: PowerPC System Software (1994),
            // pp. 2-32–2-33 documents the register-based routine convention that
            // Mixed Mode uses to marshal that four-byte A0 parameter to native PPC.
            let _ = install_powerpc_call_arguments(&mut self.cpu, &mut self.memory, &[task_ptr]);
            self.run_with_hle_imports_with_trace(
                max_cycles,
                trace_imports,
                trace_fetches,
                process_memory_manager,
                process_cfm.as_deref_mut(),
            )
        } else {
            self.interrupt_callback_stack_fault()
        };
        let end_pc = self.cpu.pc;
        let end_sp = self.cpu.gpr[1];
        let end_r3 = self.cpu.gpr[3];
        let cycles = ppc_run_result_cycles(probe.result);
        let parked = self.park_interrupt_callback_if_awaiting_m68k(
            PpcCallbackLevel::Interrupt,
            interrupt,
            callback_frame,
            &probe,
            &saved_context,
            Some(saved_current_resource_refnum),
            PpcInterruptReturnWork::VblTask { task_ptr },
        );
        if !parked {
            if entered {
                self.cpu.install_execution_context(saved_context);
            }
            self.set_current_resource_refnum(saved_current_resource_refnum);
        }

        PpcVblCallbackProbe {
            invocation: PpcVblCallbackInvocationRecord {
                task_ptr,
                callback,
                callback_entry: target.entry,
                callback_rtoc: target.rtoc,
                tick: self.current_tick(),
                cycles,
                end_pc,
                end_sp,
                end_r3,
                result: probe.result,
                unsupported_import_index: probe.unsupported_import_index,
            },
            import_trace: probe.import_trace,
            fetch_histogram: probe.fetch_histogram,
        }
    }

    fn run_draw_sprocket_vbl_callback(
        &mut self,
        context: u32,
        refcon: u32,
        callback: u32,
        max_cycles: u64,
        trace_imports: bool,
        trace_fetches: bool,
        process_memory_manager: Option<&mut ProcessMemoryManager>,
        mut process_cfm: Option<&mut PpcCfmState>,
    ) -> PpcVblCallbackProbe {
        self.assert_cfm_execution_owner(process_cfm.as_deref());
        let saved_context = self.cpu.capture_execution_context();
        let saved_current_resource_refnum = self.current_resource_refnum();
        let default_rtoc = if saved_context.architectural().gpr[2] != 0 {
            saved_context.architectural().gpr[2]
        } else {
            self.rtoc
        };
        let target = ppc_resolve_callback_target(&mut self.memory, callback, default_rtoc, None)
            .unwrap_or(PpcCallbackTarget {
                entry: callback,
                rtoc: default_rtoc,
                proc_info: 0,
                routine_flags: 0,
            });
        let interrupt = self.interrupt_entry();
        let mut callback_frame = None;
        let mut entered = false;
        let probe = if let Some(callback_sp) = self.prepare_interrupt_callback_frame(default_rtoc) {
            self.cpu.invalidate_reservation();
            entered = true;
            callback_frame = Some(callback_sp);
            self.cpu.pc = target.entry;
            self.cpu.lr = self.halt_pc;
            self.cpu.gpr[1] = callback_sp;
            self.cpu.gpr[2] = target.rtoc;
            let _ =
                install_powerpc_call_arguments(&mut self.cpu, &mut self.memory, &[context, refcon]);
            self.run_with_hle_imports_with_trace(
                max_cycles,
                trace_imports,
                trace_fetches,
                process_memory_manager,
                process_cfm.as_deref_mut(),
            )
        } else {
            self.interrupt_callback_stack_fault()
        };
        let end_pc = self.cpu.pc;
        let end_sp = self.cpu.gpr[1];
        let end_r3 = self.cpu.gpr[3];
        let cycles = ppc_run_result_cycles(probe.result);
        let parked = self.park_interrupt_callback_if_awaiting_m68k(
            PpcCallbackLevel::Interrupt,
            interrupt,
            callback_frame,
            &probe,
            &saved_context,
            Some(saved_current_resource_refnum),
            PpcInterruptReturnWork::None,
        );
        if !parked {
            if entered {
                self.cpu.install_execution_context(saved_context);
            }
            self.set_current_resource_refnum(saved_current_resource_refnum);
        }

        PpcVblCallbackProbe {
            invocation: PpcVblCallbackInvocationRecord {
                task_ptr: 0,
                callback,
                callback_entry: target.entry,
                callback_rtoc: target.rtoc,
                tick: self.current_tick(),
                cycles,
                end_pc,
                end_sp,
                end_r3,
                result: probe.result,
                unsupported_import_index: probe.unsupported_import_index,
            },
            import_trace: probe.import_trace,
            fetch_histogram: probe.fetch_histogram,
        }
    }

    pub fn run_sound_doubleback_callback(
        &mut self,
        doubleback: PpcSoundDoubleBackRecord,
        max_cycles: u64,
        trace_imports: bool,
        trace_fetches: bool,
    ) -> PpcSoundCompletionCallProbe {
        self.run_sound_doubleback_callback_inner(
            doubleback,
            max_cycles,
            trace_imports,
            trace_fetches,
            None,
            None,
        )
    }

    fn run_sound_doubleback_callback_inner(
        &mut self,
        doubleback: PpcSoundDoubleBackRecord,
        max_cycles: u64,
        trace_imports: bool,
        trace_fetches: bool,
        process_memory_manager: Option<&mut ProcessMemoryManager>,
        mut process_cfm: Option<&mut PpcCfmState>,
    ) -> PpcSoundCompletionCallProbe {
        self.assert_cfm_execution_owner(process_cfm.as_deref());
        let saved_context = self.cpu.capture_execution_context();
        let default_rtoc = if saved_context.architectural().gpr[2] != 0 {
            saved_context.architectural().gpr[2]
        } else {
            self.rtoc
        };
        let target =
            ppc_resolve_callback_target(&mut self.memory, doubleback.callback, default_rtoc, None)
                .unwrap_or(PpcCallbackTarget {
                    entry: doubleback.callback,
                    rtoc: default_rtoc,
                    proc_info: 0,
                    routine_flags: 0,
                });
        let interrupt = self.interrupt_entry();
        let mut callback_frame = None;
        let mut entered = false;
        let probe = if let Some(callback_sp) = self.prepare_interrupt_callback_frame(default_rtoc) {
            self.cpu.invalidate_reservation();
            entered = true;
            callback_frame = Some(callback_sp);
            self.cpu.pc = target.entry;
            self.cpu.lr = self.halt_pc;
            self.cpu.gpr[1] = callback_sp;
            self.cpu.gpr[2] = target.rtoc;
            let _ = install_powerpc_call_arguments(
                &mut self.cpu,
                &mut self.memory,
                &[doubleback.channel, doubleback.exhausted_buffer],
            );

            self.run_with_hle_imports_with_trace(
                max_cycles,
                trace_imports,
                trace_fetches,
                process_memory_manager,
                process_cfm.as_deref_mut(),
            )
        } else {
            self.interrupt_callback_stack_fault()
        };
        let end_pc = self.cpu.pc;
        let end_sp = self.cpu.gpr[1];
        let end_r3 = self.cpu.gpr[3];
        let cycles = ppc_run_result_cycles(probe.result);
        let parked = self.park_interrupt_callback_if_awaiting_m68k(
            PpcCallbackLevel::Interrupt,
            interrupt,
            callback_frame,
            &probe,
            &saved_context,
            None,
            PpcInterruptReturnWork::DoubleBack(doubleback),
        );
        if !parked && entered {
            self.cpu.install_execution_context(saved_context);
        }

        PpcSoundCompletionCallProbe {
            invocation: PpcSoundCompletionInvocationRecord {
                file_playback_index: u32::MAX,
                channel: doubleback.channel,
                completion: doubleback.callback,
                callback_entry: target.entry,
                callback_rtoc: target.rtoc,
                tick: doubleback.tick,
                instruction_count: doubleback.instruction_count,
                scheduled_tick: doubleback.tick,
                scheduled_instruction_count: doubleback.instruction_count,
                cycles,
                end_pc,
                end_sp,
                end_r3,
                result: probe.result,
                unsupported_import_index: probe.unsupported_import_index,
            },
            import_trace: probe.import_trace,
            fetch_histogram: probe.fetch_histogram,
        }
    }
}

/// The callback's task as found when an interrupt-level callback was entered.
#[derive(Debug, Clone, Copy)]
struct PpcInterruptEntry {
    task: crate::guest_call::ExecutionTaskId,
    top_call: Option<crate::guest_call::CallId>,
    may_park: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct PpcCallbackTarget {
    pub(crate) entry: u32,
    pub(crate) rtoc: u32,
    pub(crate) proc_info: u32,
    pub(crate) routine_flags: u16,
}

pub(crate) fn ppc_resolve_callback_target(
    memory: &mut PpcSectionMem,
    proc_ptr: u32,
    default_rtoc: u32,
    selector: Option<u32>,
) -> Option<PpcCallbackTarget> {
    let procedure = resolve_guest_procedure(
        memory,
        proc_ptr,
        default_rtoc,
        selector,
        GuestIsa::PowerPc,
        GuestIsa::PowerPc,
    )?;
    (procedure.isa == GuestIsa::PowerPc).then_some(PpcCallbackTarget {
        entry: procedure.entry,
        rtoc: procedure.rtoc,
        proc_info: procedure.proc_info,
        routine_flags: procedure.routine_flags,
    })
}

struct PpcRetiredThreadStorageEdge<'a> {
    manager: &'a mut ProcessNativeMemoryManager,
}

impl RetiredThreadStorageEdge for PpcRetiredThreadStorageEdge<'_> {
    fn release_classic(&mut self, stack_base: u32) {
        self.manager
            .dispose_classic_ptr_from_native_import(stack_base);
    }

    fn release_native(&mut self, stack_base: u32) {
        self.manager.dispose_native_ptr(stack_base);
    }
}

pub(crate) fn ppc_release_retired_thread_storage(
    manager: &mut ProcessNativeMemoryManager,
    retirement: NativeRetirement,
    recycle: bool,
) {
    let storage = match retirement {
        NativeRetirement::Removed(storage) | NativeRetirement::Switched(storage) => storage,
    };
    ThreadManager::release_retired_storage(
        storage,
        recycle,
        &mut PpcRetiredThreadStorageEdge { manager },
    );
}
