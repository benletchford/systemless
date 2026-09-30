//! PowerPC callback runner and interrupt frame methods on [`PpcLoadedApp`].
//!
//! Handles timer tasks, vertical retrace (VBL) tasks, sound completion,
//! sound doubleback, file completion, and thread switcher callbacks.

use super::*;

impl PpcLoadedApp {
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
        let probe = if let Some(context) = continuing {
            self.cpu.install_execution_context(context);
            self.run_with_hle_imports_with_trace(
                max_cycles,
                trace_imports,
                trace_fetches,
                Some(memory_manager),
                Some(cfm),
            )
        } else if let Some(callback_sp) = self.prepare_interrupt_callback_frame(default_rtoc) {
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
        // Inside Macintosh: Files (1992), "Completion Routines": the callback
        // completes after I/O; a runner slice is not a callback completion.
        if matches!(probe.result, PpcRunResult::CycleLimit { .. }) {
            self.file_completion_context = Some(self.cpu.capture_execution_context());
        }
        self.cpu.install_execution_context(saved_context);
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
        let mut entered = false;
        let probe = if let Some(callback_sp) = self.prepare_interrupt_callback_frame(default_rtoc) {
            self.cpu.invalidate_reservation();
            entered = true;
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
        if entered {
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

    pub(crate) fn run_timer_callback(
        &mut self,
        task_ptr: u32,
        callback: u32,
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
        let mut entered = false;
        let probe = if let Some(callback_sp) = self.prepare_interrupt_callback_frame(default_rtoc) {
            self.cpu.invalidate_reservation();
            entered = true;
            self.cpu.pc = target.entry;
            self.cpu.lr = self.halt_pc;
            self.cpu.gpr[1] = callback_sp;
            self.cpu.gpr[2] = target.rtoc;
            // Inside Macintosh: Processes (1994), pp. 3-21--3-22: the Time
            // Manager passes the expired TMTask record to its callback. Mixed
            // Mode marshals that pointer into the native PowerPC argument area.
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
        if entered {
            self.cpu.install_execution_context(saved_context);
        }
        self.set_current_resource_refnum(saved_current_resource_refnum);

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
                probes.push(self.run_vbl_callback(
                    task.task_ptr,
                    callback,
                    max_cycles,
                    trace_imports,
                    trace_fetches,
                    process_memory_manager.as_deref_mut(),
                    process_cfm.as_deref_mut(),
                ));
                // Inside Macintosh: Processes (1994), pp. 4-7–4-8: a VBL
                // task must reset vblCount from its callback or the Vertical
                // Retrace Manager removes the task after that execution.
                if self.memory.read_u16_be(task.task_ptr + 10) == Some(0) {
                    self.vbl_tasks.with_mut(|vbl_tasks| {
                        vbl_tasks.retain(|installed| installed.task_ptr != task.task_ptr);
                        ppc_sync_vbl_task_links(&mut self.memory, vbl_tasks);
                    });
                }
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
        let mut entered = false;
        let probe = if let Some(callback_sp) = self.prepare_interrupt_callback_frame(default_rtoc) {
            self.cpu.invalidate_reservation();
            entered = true;
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
        if entered {
            self.cpu.install_execution_context(saved_context);
        }
        self.set_current_resource_refnum(saved_current_resource_refnum);

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
        let mut entered = false;
        let probe = if let Some(callback_sp) = self.prepare_interrupt_callback_frame(default_rtoc) {
            self.cpu.invalidate_reservation();
            entered = true;
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
        if entered {
            self.cpu.install_execution_context(saved_context);
        }
        self.set_current_resource_refnum(saved_current_resource_refnum);

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
        let mut entered = false;
        let probe = if let Some(callback_sp) = self.prepare_interrupt_callback_frame(default_rtoc) {
            self.cpu.invalidate_reservation();
            entered = true;
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
        if entered {
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
