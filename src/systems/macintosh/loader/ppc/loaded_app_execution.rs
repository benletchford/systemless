//! PowerPC loaded application HLE execution engine on [`PpcLoadedApp`].

use super::*;
use super::loaded_app::{PpcGlmCallbackOperation, PpcGlmCallbackState};

fn ppc_glm_zero_guest_range(memory: &mut PpcSectionMem, pointer: u32, size: u32) -> bool {
    const ZEROES: [u8; 4096] = [0; 4096];
    let mut written = 0;
    while written < size {
        let chunk = (size - written).min(ZEROES.len() as u32);
        let Some(address) = pointer.checked_add(written) else {
            return false;
        };
        if memory.write_bytes(address, &ZEROES[..chunk as usize]).is_none() {
            return false;
        }
        written += chunk;
    }
    true
}

fn ppc_glm_copy_guest_range(
    memory: &mut PpcSectionMem,
    source: u32,
    destination: u32,
    size: u32,
) -> bool {
    if source == destination {
        return true;
    }
    let mut bytes = [0; 4096];
    let mut copied = 0;
    while copied < size {
        let chunk = (size - copied).min(bytes.len() as u32);
        let (Some(from), Some(to)) = (
            source.checked_add(copied),
            destination.checked_add(copied),
        ) else {
            return false;
        };
        if memory
            .read_bytes_into(from, &mut bytes[..chunk as usize])
            .is_none()
            || memory.write_bytes(to, &bytes[..chunk as usize]).is_none()
        {
            return false;
        }
        copied += chunk;
    }
    true
}

fn ppc_glm_begin_guest_callback(
    cpu: &mut PpcCpu,
    memory: &mut PpcSectionMem,
    callback_stack: &mut Vec<PpcGlmCallbackState>,
    target: PpcCallbackTarget,
    argument: u32,
    state: PpcGlmCallbackState,
) -> Option<PpcImportAction> {
    install_powerpc_call_arguments(cpu, memory, &[argument])?;
    callback_stack.push(state);
    let action = GuestCallEffect::call_guest(
        GuestCallRequest::new(GuestCallTarget {
            isa: GuestIsa::PowerPc,
            entry: target.entry,
            rtoc: target.rtoc,
        }),
        GuestCallContinuation::to_powerpc(
            PPC_GUEST_CALL_RETURN_PC,
            state.import_pc,
            state.restore_rtoc,
            PpcNativeReturnGpr3::Preserve,
        ),
    )
    .into_ppc_import_action();
    if action.is_none() {
        callback_stack.pop();
    }
    action
}

fn ppc_hle_import_trace_same_run(
    left: &PpcHleImportTraceEntry,
    right: &PpcHleImportTraceEntry,
) -> bool {
    left.library_name == right.library_name && left.symbol_name == right.symbol_name
}

fn push_ppc_hle_import_trace_entry(
    trace: &mut Vec<PpcHleImportTraceEntry>,
    entry: PpcHleImportTraceEntry,
) {
    if let Some(last) = trace.last_mut() {
        if ppc_hle_import_trace_same_run(last, &entry) {
            last.repeat_count = last.repeat_count.saturating_add(entry.repeat_count);
            return;
        }
    }
    trace.push(entry);
}

impl PpcLoadedApp {
    pub(crate) fn assert_cfm_execution_owner(&self, process_cfm: Option<&PpcCfmState>) {
        assert!(
            process_cfm.is_some() || self.cfm.is_some(),
            "installed native execution requires process CFM services"
        );
        assert!(
            process_cfm.is_none() || self.cfm.is_none(),
            "move the standalone CFM seed before using process services"
        );
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn run_with_process_services(
        &mut self,
        max_cycles: u64,
        trace_imports: bool,
        trace_fetches: bool,
        memory_manager: &mut ProcessMemoryManager,
        cfm: &mut PpcCfmState,
    ) -> PpcHleRunProbe {
        self.run_with_hle_imports_with_trace(
            max_cycles,
            trace_imports,
            trace_fetches,
            Some(memory_manager),
            Some(cfm),
        )
    }

    pub fn run_with_hle_imports(&mut self, max_cycles: u64) -> PpcHleRunProbe {
        self.run_with_hle_imports_with_trace(max_cycles, false, false, None, None)
    }

    pub fn run_with_hle_import_trace(&mut self, max_cycles: u64) -> PpcHleRunProbe {
        self.run_with_hle_imports_with_trace(max_cycles, true, false, None, None)
    }

    pub fn run_with_hle_import_fetch_histogram(&mut self, max_cycles: u64) -> PpcHleRunProbe {
        self.run_with_hle_imports_with_trace(max_cycles, false, true, None, None)
    }

    pub fn run_with_hle_import_trace_and_fetch_histogram(
        &mut self,
        max_cycles: u64,
    ) -> PpcHleRunProbe {
        self.run_with_hle_imports_with_trace(max_cycles, true, true, None, None)
    }

    #[cfg(test)]
    pub(crate) fn run_with_process_memory_manager(
        &mut self,
        max_cycles: u64,
        trace_imports: bool,
        trace_fetches: bool,
        memory_manager: &mut ProcessMemoryManager,
    ) -> PpcHleRunProbe {
        self.run_with_hle_imports_with_trace(
            max_cycles,
            trace_imports,
            trace_fetches,
            Some(memory_manager),
            None,
        )
    }

    pub(crate) fn run_with_hle_imports_with_trace(
        &mut self,
        max_cycles: u64,
        trace_imports: bool,
        trace_fetches: bool,
        process_memory_manager: Option<&mut ProcessMemoryManager>,
        mut process_cfm: Option<&mut PpcCfmState>,
    ) -> PpcHleRunProbe {
        self.assert_cfm_execution_owner(process_cfm.as_deref());
        let guest_calls = self.toolbox_startup.execution.calls().shared_handle();
        let trap_default_gateways = self.trap_default_gateways.clone();
        // Wakeup selects and prepares a saved context before any native step.
        guest_calls.resume_ready_task();
        if !guest_calls.current_task_is_running()
            || guest_calls.has_classic_task_handoff()
            || (guest_calls.has_pending_task_handoff()
                && !guest_calls.prepare_native_task(&mut self.cpu))
        {
            return PpcHleRunProbe {
                result: PpcRunResult::CycleLimit { cycles: 0 },
                handled_import_count: 0,
                last_import_index: None,
                unsupported_import_index: None,
                import_trace: Vec::new(),
                draw_sprocket_trace: Vec::new(),
                input_sprocket_trace: Vec::new(),
                fetch_histogram: None,
            };
        }
        let standalone_memory_manager = process_memory_manager
            .is_none()
            .then(|| self.process_memory_manager.0.clone());
        let mut standalone_memory_manager_borrow;
        let process_memory_manager = if let Some(memory_manager) = process_memory_manager {
            memory_manager
        } else {
            standalone_memory_manager_borrow = standalone_memory_manager
                .as_ref()
                .expect("standalone process Memory Manager created")
                .borrow_mut();
            assert!(
                standalone_memory_manager_borrow.has_native_allocator(),
                "loaded adapter owns a native allocator before execution"
            );
            &mut standalone_memory_manager_borrow
        };
        let mut import_run_state = PpcImportRunState::from_parts(
            std::mem::take(&mut self.imports),
            self.import_count,
            ppc_import_layout(),
        );
        let q3_start_rendering_import_index = import_run_state
            .bindings()
            .iter()
            .find(|binding| {
                binding.dispatcher_target == PpcImportDispatcherTarget::Q3ViewStartRendering
            })
            .map(|binding| binding.symbol_index);
        let q3_end_rendering_import_index = import_run_state
            .bindings()
            .iter()
            .find(|binding| {
                binding.dispatcher_target == PpcImportDispatcherTarget::Q3ViewEndRendering
            })
            .map(|binding| binding.symbol_index);
        let input = self.current_input_snapshot();
        self.mirror_input_low_memory(input);
        let event_queue = std::mem::take(&mut self.event_queue);
        let process_memory_manager = process_memory_manager.native_mut();
        let _ = self.current_tick();
        let tick_state = self.tick_state.shared_handle();
        let clock_cycles_per_tick = self.clock_cycles_per_tick;
        let clock_cycle_phase = self.clock_cycle_phase;
        let stack_base = self.stack_base;
        let stack_top = self.stack_base + self.stack_size;
        let mut process_file_system = self.process_file_system.shared_handle();
        let current_resource_refnum = process_file_system.current_resource_file.shared_handle();
        let mut last_resource_error = self
            .memory
            .read_u16_be(crate::memory::globals::addr::RES_ERR)
            .unwrap_or(0) as i16;
        let resource_policy = process_file_system.policy.shared_handle();
        let native_exception_handler = Cell::new(self.native_exception_handler);
        let mut native_exception_stack = std::mem::take(&mut self.native_exception_stack);
        let mut stdc_qsort_stack = std::mem::take(&mut self.stdc_qsort_stack);
        let mut dialog_callback_stack = std::mem::take(&mut self.dialog_callback_stack);
        let mut collection_callback_stack = std::mem::take(&mut self.collection_callback_stack);
        let mut pending_file_completions = std::mem::take(&mut self.pending_file_completions);
        let mut apple_events = std::mem::take(&mut self.apple_events);
        let mut standalone_cfm = if process_cfm.is_none() {
            self.cfm.take()
        } else {
            None
        };
        let cfm = if let Some(cfm) = process_cfm.as_deref_mut() {
            cfm
        } else {
            standalone_cfm.as_mut().expect("standalone CFM seed exists")
        };
        let (mut cfm_connections, mut cfm_library_fragments, mut next_cfm_connection_id) = (
            &mut cfm.connections,
            &mut cfm.library_fragments,
            &mut cfm.next_connection_id,
        );
        let controls = std::mem::take(&mut self.controls);
        let mut aliases = std::mem::take(&mut self.aliases);
        let mut gworlds = std::mem::take(&mut self.gworlds);
        let mut agl = std::mem::take(&mut self.agl);
        let gworld_pixel_states = self.gworld_pixel_states.shared_handle();
        let window_list = self.window_list.shared_handle();
        if window_list.is_empty() {
            window_list.with_mut(|windows| {
                windows.extend(
                    gworlds
                        .iter()
                        .rev()
                        .map(|record| record.port)
                        .filter(|port| !matches!(*port, PPC_MAIN_GWORLD | PPC_DSP_BACK_GWORLD)),
                );
            });
        }
        let mut q3_objects = std::mem::take(&mut self.q3_objects);
        let mut q3_object_refs = std::mem::take(&mut self.q3_object_refs);
        let mut next_q3_object = self.next_q3_object;
        let mut q3_error_state = self.q3_error_state;
        let mut q3_lifecycle = self.q3_lifecycle;
        let mut q3_memory_storages = std::mem::take(&mut self.q3_memory_storages);
        let mut q3_files = std::mem::take(&mut self.q3_files);
        let mut q3_group_memberships = std::mem::take(&mut self.q3_group_memberships);
        let mut q3_file_groups = std::mem::take(&mut self.q3_file_groups);
        let mut q3_views = std::mem::take(&mut self.q3_views);
        let mut q3_submissions = std::mem::take(&mut self.q3_submissions);
        let mut q3_view_transforms = std::mem::take(&mut self.q3_view_transforms);
        let mut q3_submission_transforms = std::mem::take(&mut self.q3_submission_transforms);
        let mut q3_view_materials = std::mem::take(&mut self.q3_view_materials);
        let mut q3_submission_materials = std::mem::take(&mut self.q3_submission_materials);
        let mut q3_submission_lights = std::mem::take(&mut self.q3_submission_lights);
        let mut q3_view_state_stack = std::mem::take(&mut self.q3_view_state_stack);
        let mut q3_completed_frames = std::mem::take(&mut self.q3_completed_frames);
        let mut q3_retained_frames = std::mem::take(&mut self.q3_retained_frames);
        let mut q3_state_only_completed_frame_batches =
            std::mem::take(&mut self.q3_state_only_completed_frame_batches);
        let mut q3_fog_styles = std::mem::take(&mut self.q3_fog_styles);
        let mut q3_immediate_trimeshes = std::mem::take(&mut self.q3_immediate_trimeshes);
        let mut q3_attributes = std::mem::take(&mut self.q3_attributes);
        let mut q3_shader_uv_transforms = std::mem::take(&mut self.q3_shader_uv_transforms);
        let mut q3_shader_boundaries = std::mem::take(&mut self.q3_shader_boundaries);
        let mut q3_mipmap_textures = std::mem::take(&mut self.q3_mipmap_textures);
        let mut q3_texture_shaders = std::mem::take(&mut self.q3_texture_shaders);
        let mut q3_renderer_preferences = std::mem::take(&mut self.q3_renderer_preferences);
        let mut q3_draw_contexts = std::mem::take(&mut self.q3_draw_contexts);
        let mut q3_trimeshes = std::mem::take(&mut self.q3_trimeshes);
        let mut q3_styles = std::mem::take(&mut self.q3_styles);
        let mut q3_cameras = std::mem::take(&mut self.q3_cameras);
        let mut q3_lights = std::mem::take(&mut self.q3_lights);
        let mut input_sprocket = self.input_sprocket;
        let mut input_sprocket_virtual_elements =
            std::mem::take(&mut self.input_sprocket_virtual_elements);
        let mut toolbox_startup = std::mem::take(&mut self.toolbox_startup);
        let mut quicktime = std::mem::take(&mut self.quicktime);
        let mut sound = std::mem::take(&mut self.sound);
        let mut glm_mode = self.glm_mode;
        let mut glm_callbacks = self.glm_callbacks;
        let mut glm_callback_stack = std::mem::take(&mut self.glm_callback_stack);
        let mut glm_allocations = std::mem::take(&mut self.glm_allocations);
        let mut glm_page_free_all_queue = std::mem::take(&mut self.glm_page_free_all_queue);
        let mut glm_error = self.glm_error;
        let timer_tasks = std::mem::take(&mut self.timer_tasks);
        let vbl_tasks = std::mem::take(&mut self.vbl_tasks);
        let callback_scheduling = self.callback_scheduling.shared_handle();
        // Keep File and Resource Manager records in their process-owned
        // managers for the whole native execution slice. The classic
        // adapter can enter through Mixed Mode while an import is running,
        // so taking these records out and restoring them at slice teardown
        // would leave the process temporarily with an empty manager view.
        // (Inside Macintosh: Files, 1992, pp. 1-7–1-9; Inside Macintosh
        // Volume I, 1985, pp. I-109–I-110.)
        let current_gworld = self.current_gworld.shared_handle();
        let current_gdevice = self.current_gdevice.shared_handle();
        let quickdraw_op_colors = self.quickdraw_op_colors.shared_handle();
        let quickdraw_hilite_colors = self.quickdraw_hilite_colors.shared_handle();
        let screen_clut = self.screen_clut.shared_handle();
        let color_manager_clut = self.color_manager_clut.shared_handle();
        let display_gamma = self.display_gamma.shared_handle();
        let mut quickdraw_fore_color = self.quickdraw_fore_color;
        let mut quickdraw_fore_indices = std::mem::take(&mut self.quickdraw_fore_indices);
        let mut quickdraw_back_color = self.quickdraw_back_color;
        let mut quickdraw_pen_h = self.quickdraw_pen_h;
        let mut quickdraw_pen_v = self.quickdraw_pen_v;
        let mut quickdraw_text_mode = self.quickdraw_text_mode;
        let mut quickdraw_text_size = self.quickdraw_text_size;
        let process_quickdraw_port_state_attached = self.process_quickdraw_port_state_attached;
        let cursor_state = std::mem::take(&mut self.cursor_state);
        let vfs_volumes = self.vfs_volumes.shared_handle();
        let vfs_directories = self.vfs_directories.shared_handle();
        let next_vfs_dir_id = self.next_vfs_dir_id.shared_handle();
        let default_dir_id = self.default_dir_id.shared_handle();
        let working_directories = self.working_directories.shared_handle();
        let next_working_directory_ref_num = self.next_working_directory_ref_num.shared_handle();
        let application_working_directory_ref_num =
            self.application_working_directory_ref_num.shared_handle();
        let param_text = self.param_text.shared_handle();
        let mut scrap = std::mem::take(&mut self.scrap);
        let list_manager = std::mem::take(&mut self.list_manager);
        let collections = self.collections.shared_handle();
        let mut draw_sprocket = std::mem::take(&mut self.draw_sprocket);
        let mut handled_import_count = 0u32;
        let mut last_import_index = None;
        let mut unsupported_import_index = None;
        let mut import_trace = Vec::new();
        let mut draw_sprocket_trace = Vec::new();
        let mut input_sprocket_trace = Vec::new();
        let mut fetch_histogram = PpcFetchHistogram::new();
        let trace_ppc = ppc_trace_enabled();
        let trace_sprocket = sprocket_trace_enabled();
        let trace_qd3d = qd3d_trace_enabled();
        let trace_pc_range = ppc_trace_pc_range();
        let trace_recent_on_halt = ppc_recent_imports_on_halt_enabled();
        let mut recent_imports = VecDeque::<PpcHleImportTraceEntry>::new();
        let mut idle_poll_counts = HashMap::<u32, u32>::new();
        let mut tick_count_idle_poll = PpcTickCountIdlePollState::default();
        let needs_fetch_observer = trace_fetches || trace_ppc || trace_pc_range.is_some();
        let mut interrupt_return_work = None;
        let mut fetch_observer = PpcHleFetchObserver {
            histogram: if trace_fetches {
                Some(&mut fetch_histogram)
            } else {
                None
            },
            trace_fetches: trace_ppc,
            trace_pc_range,
        };

        let result = {
            type Mem = PpcSectionMem;
            let mut handle_import = |elapsed, index, cpu: &mut PpcCpu, memory: &mut Mem| {
                if index == PPC_THREAD_RETURN_IMPORT_INDEX {
                    // ThreadEntryProc returns its result in R3. Retire only
                    // after validating the successor and result destination.
                    // Inside Macintosh: Thread Manager (1999), pp. 59–60.
                    let task = guest_calls.current_task();
                    let result = cpu.gpr[3];
                    if let Some((procedure, parameter)) = guest_calls.take_thread_terminator(task) {
                        if procedure != 0 {
                            // The terminator takes the retiring ID and its
                            // registered parameter. Its return value does not
                            // replace the thread entry's result.
                            // Thread Manager (1999), pp. 81–82, 88–89.
                            if install_powerpc_call_arguments(
                                cpu,
                                memory,
                                &[task.thread_id(), parameter],
                            )
                            .is_none()
                            {
                                return PpcImportAction::Halt;
                            }
                            return GuestCallEffect::call_guest(
                                GuestCallRequest::new(GuestCallTarget {
                                    isa: GuestIsa::PowerPc,
                                    entry: procedure,
                                    rtoc: cpu.gpr[2],
                                }),
                                GuestCallContinuation::to_powerpc(
                                    PPC_GUEST_CALL_RETURN_PC,
                                    PPC_THREAD_RETURN_PC,
                                    cpu.gpr[2],
                                    PpcNativeReturnGpr3::Set(result),
                                ),
                            )
                            .into_ppc_import_action()
                            .unwrap_or(PpcImportAction::Halt);
                        }
                    }
                    if let Ok(retirement) =
                        guest_calls.retire_native_thread(task, cpu, false, |context| {
                            context.result_destination == 0
                                || memory
                                    .write_u32_be(context.result_destination, result)
                                    .is_some()
                        })
                    {
                        ppc_release_retired_thread_storage(
                            process_memory_manager,
                            retirement,
                            false,
                        );
                    }
                    return PpcImportAction::Yield(1);
                }
                if index == PPC_GUEST_CALL_RETURN_IMPORT_INDEX {
                    let mut resource_call = None;
                    if toolbox_startup
                        .execution
                        .menu()
                        .ready_call(GuestIsa::PowerPc)
                        .is_some()
                        || guest_calls
                            .ready_menu_bar_build(GuestIsa::PowerPc)
                            .is_some()
                        || guest_calls.complete_powerpc_resuming_operation(
                            cpu,
                            process_memory_manager,
                            |operation, result| match operation {
                                crate::guest_call::ManagerContinuation::Menu(
                                    crate::guest_call::MenuManagerContinuation::Definition(
                                        operation,
                                    ),
                                ) => {
                                    operation.complete(memory);
                                    result
                                }
                                crate::guest_call::ManagerContinuation::Menu(
                                    crate::guest_call::MenuManagerContinuation::Hook(_),
                                ) => unreachable!("MenuHook completes after native caller restore"),
                                crate::guest_call::ManagerContinuation::Cfm(
                                    CfmOperation::Load(load),
                                ) => ppc_complete_cfm_load(
                                    load,
                                    result,
                                    memory,
                                    &mut cfm_connections,
                                ),
                                crate::guest_call::ManagerContinuation::Cfm(
                                    CfmOperation::Resource(call),
                                ) => match call.complete(result, &mut cfm_connections, memory) {
                                    Ok(call) => {
                                        resource_call = Some(call);
                                        0
                                    }
                                    Err(error) => ppc_i16_result(error.os_error()),
                                },
                            },
                        )
                    {
                        if guest_calls
                            .ready_menu_bar_build(GuestIsa::PowerPc)
                            .is_some()
                        {
                            let heap = process_memory_manager
                                .native_heap_state()
                                .expect("native allocator registered during execution");
                            let mut cursor = heap.heap_cursor;
                            let limit =
                                process_memory_manager.native_allocation_limit(heap.heap_limit);
                            return ppc_continue_menu_bar_build(
                                cpu,
                                process_memory_manager,
                                memory,
                                &mut cursor,
                                limit,
                                &mut toolbox_startup,
                                &process_file_system.resource_manager.vfs_resources,
                                *current_resource_refnum,
                            );
                        }
                        if let Some((_call, _scope)) = toolbox_startup
                            .execution
                            .resume_menu_call(GuestIsa::PowerPc)
                        {
                            let heap = process_memory_manager
                                .native_heap_state()
                                .expect("native allocator registered during execution");
                            let mut cursor = heap.heap_cursor;
                            let limit =
                                process_memory_manager.native_allocation_limit(heap.heap_limit);
                            return current_gworld.with_mut(|current_gworld| {
                                current_gdevice.with_mut(|current_gdevice| {
                                    ppc_step_menu_tracking(
                                        cpu,
                                        process_memory_manager,
                                        memory,
                                        &mut cursor,
                                        limit,
                                        &gworlds,
                                        &screen_clut,
                                        &mut toolbox_startup,
                                        current_gworld,
                                        current_gdevice,
                                        input,
                                        &process_file_system.resource_manager.vfs_resources,
                                        *current_resource_refnum,
                                    )
                                    .unwrap_or(PpcImportAction::Halt)
                                })
                            });
                        }
                        if let Some(call) = resource_call {
                            if let Err(error) = ppc_invoke_prepared_resource(
                                cpu,
                                memory,
                                &guest_calls,
                                call,
                                cpu.pc,
                            ) {
                                cpu.gpr[3] = ppc_i16_result(error);
                            }
                            return PpcImportAction::Continue;
                        }
                        let native_heap = process_memory_manager
                            .native_heap_state()
                            .expect("native allocator registered during execution");
                        let mut heap_cursor = native_heap.heap_cursor;
                        let mut last_mem_error = native_heap.last_mem_error;
                        let heap_limit = process_memory_manager
                            .native_allocation_limit(native_heap.heap_limit);
                        let handles = &mut process_memory_manager.native_handle_records().to_vec();
                        if let Some(action) = ppc_continue_ae_resolve_cleanup(
                            cpu,
                            guest_calls.depth(),
                            &mut apple_events,
                            &mut *process_memory_manager,
                            memory,
                            &mut heap_cursor,
                            heap_limit,
                            &mut last_mem_error,
                            handles,
                            &mut toolbox_startup,
                        ) {
                            process_memory_manager.set_native_mem_error(last_mem_error);
                            return guest_calls.externalize_powerpc_action(cpu, action);
                        }
                        if ppc_complete_token_disposal(
                            cpu,
                            guest_calls.depth(),
                            &mut apple_events,
                            &mut *process_memory_manager,
                            memory,
                            &mut heap_cursor,
                            heap_limit,
                            &mut last_mem_error,
                            handles,
                        ) {
                            process_memory_manager.set_native_mem_error(last_mem_error);
                            return PpcImportAction::Continue;
                        }
                        if let Some(action) = ppc_continue_ae_resolve(
                            cpu,
                            guest_calls.depth(),
                            &mut apple_events,
                            &mut *process_memory_manager,
                            memory,
                            &mut heap_cursor,
                            heap_limit,
                            &mut last_mem_error,
                            handles,
                            &mut toolbox_startup,
                        ) {
                            process_memory_manager.set_native_mem_error(last_mem_error);
                            return guest_calls.externalize_powerpc_action(cpu, action);
                        }
                        ppc_complete_apple_event_dispatch(
                            &mut apple_events,
                            guest_calls.depth(),
                            &mut *process_memory_manager,
                            memory,
                            &mut heap_cursor,
                            &mut last_mem_error,
                            handles,
                        );
                        process_memory_manager.set_native_mem_error(last_mem_error);
                        return PpcImportAction::Continue;
                    }
                    if guest_calls.complete_powerpc_for_m68k(cpu) {
                        return PpcImportAction::Halt;
                    }
                    unsupported_import_index = Some(index);
                    return PpcImportAction::Halt;
                }
                if index == PPC_STD_FILTER_IMPORT_INDEX {
                    return PpcImportAction::Return(ppc_standard_filter_proc(cpu, memory));
                }
                last_import_index = Some(index);
                // A Mixed Mode callback can advance process time while the
                // native slice is suspended. Refresh the whole-tick baseline
                // before every import so TickCount and EventRecord.when use
                // the same canonical process clock while retaining this
                // slice's native cycle phase.
                let process_tick = memory
                    .read_u32_be(crate::memory::globals::addr::TICKS)
                    .map(|guest_ticks| tick_state.read_tick_count(guest_ticks))
                    .unwrap_or_else(|| tick_state.current_tick());
                let mut import_tick_count = ppc_virtual_tick_count(
                    process_tick,
                    clock_cycles_per_tick,
                    clock_cycle_phase,
                    elapsed,
                );
                let dispatcher_target = import_run_state.dispatcher_target_cloned(index);
                let is_tick_count_import = dispatcher_target
                    .as_ref()
                    .is_some_and(|target| *target == PpcImportDispatcherTarget::TickCount);
                // Most per-frame imports are handled by the small fast path
                // below. Defer cloning their library and symbol strings until
                // tracing or the general dispatcher actually needs them.
                let mut binding = trace_recent_on_halt
                    .then(|| import_run_state.binding_cloned(index))
                    .flatten();
                if !is_tick_count_import {
                    tick_count_idle_poll.reset();
                }
                if !trace_imports && !trace_ppc && !trace_qd3d {
                    if q3_start_rendering_import_index == Some(index) {
                        let action =
                            PpcImportAction::Return(u32::from(ppc_q3_view_start_rendering(
                                cpu,
                                &mut q3_views,
                                &q3_objects,
                                &mut q3_submissions,
                                &mut q3_submission_transforms,
                                &mut q3_submission_materials,
                                &mut q3_submission_lights,
                                &mut q3_error_state,
                            )));
                        handled_import_count = handled_import_count.saturating_add(1);
                        return ppc_import_action_with_extra_cycles(
                            action,
                            PPC_Q3_HOT_IMPORT_EXTRA_CYCLES,
                        );
                    }
                    if q3_end_rendering_import_index == Some(index) {
                        let action = dispatch_q3_view_end_rendering_import(
                            cpu,
                            &mut q3_views,
                            &q3_objects,
                            &mut q3_submissions,
                            &mut q3_submission_transforms,
                            &mut q3_submission_materials,
                            &mut q3_submission_lights,
                            &mut q3_completed_frames,
                            &mut q3_retained_frames,
                            &mut q3_state_only_completed_frame_batches,
                            &q3_draw_contexts,
                            &q3_trimeshes,
                            &gworlds,
                            *current_gworld,
                            &mut q3_error_state,
                            input.is_idle(),
                        );
                        handled_import_count = handled_import_count.saturating_add(1);
                        return ppc_import_action_with_extra_cycles(
                            action,
                            PPC_Q3_HOT_IMPORT_EXTRA_CYCLES,
                        );
                    }
                }
                let Some(dispatcher_target) = dispatcher_target.as_ref() else {
                    unsupported_import_index = Some(index);
                    if trace_ppc {
                        eprintln!(
                            "{}",
                            format_ppc_trace_unknown_import(
                                index, cpu.pc, cpu.lr, cpu.gpr[2], cpu.gpr[1]
                            )
                        );
                    }
                    return PpcImportAction::Halt;
                };
                match ppc_live_trap_import_action(
                    dispatcher_target,
                    &trap_default_gateways,
                    cpu,
                    &mut *process_memory_manager,
                    memory,
                    &mut toolbox_startup,
                ) {
                    Ok(Some(action)) => {
                        if trace_imports {
                            if binding.is_none() {
                                binding = import_run_state.binding_cloned(index);
                            }
                            let binding = binding
                                .as_ref()
                                .expect("live trap import tracing resolves a known binding");
                            push_ppc_hle_import_trace_entry(
                                &mut import_trace,
                                PpcHleImportTraceEntry {
                                    import_index: index,
                                    library_name: binding.library_name.clone(),
                                    symbol_name: binding.symbol_name.clone(),
                                    pc: cpu.pc,
                                    lr: cpu.lr,
                                    rtoc: cpu.gpr[2],
                                    sp: cpu.gpr[1],
                                    dispatcher_target: dispatcher_target.clone(),
                                    repeat_count: 1,
                                },
                            );
                        }
                        handled_import_count = handled_import_count.saturating_add(1);
                        return guest_calls.externalize_powerpc_action(cpu, action);
                    }
                    Ok(None) => {}
                    Err(()) => return PpcImportAction::Halt,
                }
                if trace_recent_on_halt {
                    let binding = binding
                        .as_ref()
                        .expect("recent import tracing resolves a known binding");
                    let entry = PpcHleImportTraceEntry {
                        import_index: index,
                        library_name: binding.library_name.clone(),
                        symbol_name: binding.symbol_name.clone(),
                        pc: cpu.pc,
                        lr: cpu.lr,
                        rtoc: cpu.gpr[2],
                        sp: cpu.gpr[1],
                        dispatcher_target: binding.dispatcher_target.clone(),
                        repeat_count: 1,
                    };
                    match recent_imports.back_mut() {
                        Some(last) if ppc_hle_import_trace_same_run(last, &entry) => {
                            last.repeat_count = last.repeat_count.saturating_add(1);
                        }
                        _ => recent_imports.push_back(entry),
                    }
                    while recent_imports.len() > 64 {
                        recent_imports.pop_front();
                    }
                }
                // A null event cannot select an item. Pending callbacks
                // still need the general dispatch path.
                let null_dialog_poll = matches!(
                    dispatcher_target,
                    PpcImportDispatcherTarget::DialogCompatibility(
                        PpcDialogCompatibilityOperation::DialogSelect
                    )
                ) && dialog_callback_stack.is_empty()
                    && memory.read_u16_be(cpu.gpr[3]) == Some(0);
                if !trace_ppc
                    && (*dispatcher_target != PpcImportDispatcherTarget::TickCount
                        || !trace_imports)
                    && (matches!(
                        dispatcher_target,
                        PpcImportDispatcherTarget::Button
                            | PpcImportDispatcherTarget::StillDown
                            | PpcImportDispatcherTarget::WaitMouseUp
                            | PpcImportDispatcherTarget::GetKeys
                            | PpcImportDispatcherTarget::TickCount
                            | PpcImportDispatcherTarget::Microseconds
                            | PpcImportDispatcherTarget::GetCurrentThread
                            | PpcImportDispatcherTarget::YieldToThread
                            | PpcImportDispatcherTarget::GetMenuHandle
                            | PpcImportDispatcherTarget::StdFilterProc
                            | PpcImportDispatcherTarget::EnableMenuItem
                            | PpcImportDispatcherTarget::DisableMenuItem
                    ) || null_dialog_poll)
                {
                    if trace_imports {
                        if binding.is_none() {
                            binding = import_run_state.binding_cloned(index);
                        }
                        let binding = binding
                            .as_ref()
                            .expect("import tracing resolves a known binding");
                        push_ppc_hle_import_trace_entry(
                            &mut import_trace,
                            PpcHleImportTraceEntry {
                                import_index: index,
                                library_name: binding.library_name.clone(),
                                symbol_name: binding.symbol_name.clone(),
                                pc: cpu.pc,
                                lr: cpu.lr,
                                rtoc: cpu.gpr[2],
                                sp: cpu.gpr[1],
                                dispatcher_target: dispatcher_target.clone(),
                                repeat_count: 1,
                            },
                        );
                    }
                    let action = match dispatcher_target {
                        PpcImportDispatcherTarget::Button => {
                            toolbox_startup.last_button_result = Some(input.mouse_button);
                            dispatch_button_import(cpu, input, Some(&mut idle_poll_counts))
                        }
                        PpcImportDispatcherTarget::StillDown => {
                            let (still_down, action) = event_queue.with_ref(|queue| {
                                let still_down = ppc_still_down_result(input, queue);
                                let action = dispatch_still_down_import(
                                    cpu,
                                    input,
                                    queue,
                                    Some(&mut idle_poll_counts),
                                );
                                (still_down, action)
                            });
                            toolbox_startup.last_still_down_result = Some(still_down);
                            action
                        }
                        PpcImportDispatcherTarget::WaitMouseUp => {
                            let result = event_queue
                                .with_mut(|queue| ppc_wait_mouse_up_result(input, queue));
                            toolbox_startup.last_wait_mouse_up_result = Some(result);
                            PpcImportAction::Return(u32::from(result))
                        }
                        PpcImportDispatcherTarget::GetKeys => {
                            dispatch_getkeys_import(cpu, memory, input, Some(&mut idle_poll_counts))
                        }
                        PpcImportDispatcherTarget::TickCount => dispatch_tick_count_import(
                            cpu,
                            import_tick_count,
                            ppc_cycles_until_next_tick(
                                clock_cycles_per_tick,
                                clock_cycle_phase,
                                elapsed,
                            )
                            .min(max_cycles.saturating_sub(elapsed)),
                            Some(&mut tick_count_idle_poll),
                        ),
                        PpcImportDispatcherTarget::Microseconds => dispatch_microseconds_import(
                            cpu,
                            memory,
                            ppc_virtual_microseconds(
                                process_tick,
                                clock_cycles_per_tick,
                                clock_cycle_phase,
                                elapsed,
                            ),
                            Some(&mut idle_poll_counts),
                        ),
                        PpcImportDispatcherTarget::GetCurrentThread => {
                            let id = ThreadManager::new(toolbox_startup.execution.calls())
                                .current_thread();
                            let result = if cpu.gpr[3] != 0
                                && memory.write_u32_be(cpu.gpr[3], id).is_some()
                            {
                                PPC_NO_ERR
                            } else {
                                PPC_PARAM_ERR
                            };
                            PpcImportAction::Return(ppc_i16_result(result))
                        }
                        PpcImportDispatcherTarget::YieldToThread => {
                            let suggested = cpu.gpr[3];
                            let action = match toolbox_startup
                                .execution
                                .calls()
                                .yield_native_thread(cpu, suggested)
                            {
                                Ok(true) => PpcImportAction::Yield(1),
                                Ok(false) => PpcImportAction::Return(0),
                                Err(error) => PpcImportAction::Return(ppc_i16_result(error)),
                            };
                            if std::env::var_os("SYSTEMLESS_PPC_THREAD_TRACE").is_some() {
                                eprintln!("[PPC-THREAD-TRACE] YieldToThread caller={} suggested={} action={:?} pc=${:08X}", toolbox_startup.execution.calls().current_task().thread_id(), suggested, action, cpu.pc);
                            }
                            action
                        }
                        PpcImportDispatcherTarget::GetMenuHandle => {
                            let menu_list = ppc_current_menu_list(memory);
                            PpcImportAction::Return(ppc_get_menu_handle(
                                memory,
                                menu_list,
                                cpu.gpr[3] as u16 as i16,
                            ))
                        }
                        PpcImportDispatcherTarget::StdFilterProc => {
                            PpcImportAction::Return(ppc_standard_filter_proc(cpu, memory))
                        }
                        PpcImportDispatcherTarget::DialogCompatibility(
                            PpcDialogCompatibilityOperation::DialogSelect,
                        ) => PpcImportAction::Return(0),
                        PpcImportDispatcherTarget::EnableMenuItem
                        | PpcImportDispatcherTarget::DisableMenuItem => {
                            ppc_set_menu_item_enabled(
                                memory,
                                process_memory_manager.native_handle_records(),
                                cpu.gpr[3],
                                cpu.gpr[4] as u16 as i16,
                                *dispatcher_target == PpcImportDispatcherTarget::EnableMenuItem,
                            );
                            PpcImportAction::ReturnPreserve
                        }
                        _ => unreachable!(),
                    };
                    handled_import_count = handled_import_count.saturating_add(1);
                    return action;
                }
                let Some(binding) = binding.or_else(|| import_run_state.binding_cloned(index))
                else {
                    unsupported_import_index = Some(index);
                    if trace_ppc {
                        eprintln!(
                            "{}",
                            format_ppc_trace_unknown_import(
                                index, cpu.pc, cpu.lr, cpu.gpr[2], cpu.gpr[1]
                            )
                        );
                    }
                    return PpcImportAction::Halt;
                };
                let binding = &binding;
                if !trace_ppc && !trace_qd3d {
                    if q3_start_rendering_import_index == Some(index) {
                        if trace_imports {
                            push_ppc_hle_import_trace_entry(
                                &mut import_trace,
                                PpcHleImportTraceEntry {
                                    import_index: index,
                                    library_name: binding.library_name.clone(),
                                    symbol_name: binding.symbol_name.clone(),
                                    pc: cpu.pc,
                                    lr: cpu.lr,
                                    rtoc: cpu.gpr[2],
                                    sp: cpu.gpr[1],
                                    dispatcher_target: binding.dispatcher_target.clone(),
                                    repeat_count: 1,
                                },
                            );
                        }
                        let action =
                            PpcImportAction::Return(u32::from(ppc_q3_view_start_rendering(
                                cpu,
                                &mut q3_views,
                                &q3_objects,
                                &mut q3_submissions,
                                &mut q3_submission_transforms,
                                &mut q3_submission_materials,
                                &mut q3_submission_lights,
                                &mut q3_error_state,
                            )));
                        handled_import_count = handled_import_count.saturating_add(1);
                        return ppc_import_action_with_extra_cycles(
                            action,
                            ppc_import_extra_cycles_for_binding(binding),
                        );
                    }
                    if q3_end_rendering_import_index == Some(index) {
                        if trace_imports {
                            push_ppc_hle_import_trace_entry(
                                &mut import_trace,
                                PpcHleImportTraceEntry {
                                    import_index: index,
                                    library_name: binding.library_name.clone(),
                                    symbol_name: binding.symbol_name.clone(),
                                    pc: cpu.pc,
                                    lr: cpu.lr,
                                    rtoc: cpu.gpr[2],
                                    sp: cpu.gpr[1],
                                    dispatcher_target: binding.dispatcher_target.clone(),
                                    repeat_count: 1,
                                },
                            );
                        }
                        let action = dispatch_q3_view_end_rendering_import(
                            cpu,
                            &mut q3_views,
                            &q3_objects,
                            &mut q3_submissions,
                            &mut q3_submission_transforms,
                            &mut q3_submission_materials,
                            &mut q3_submission_lights,
                            &mut q3_completed_frames,
                            &mut q3_retained_frames,
                            &mut q3_state_only_completed_frame_batches,
                            &q3_draw_contexts,
                            &q3_trimeshes,
                            &gworlds,
                            *current_gworld,
                            &mut q3_error_state,
                            input.is_idle(),
                        );
                        handled_import_count = handled_import_count.saturating_add(1);
                        return ppc_import_action_with_extra_cycles(
                            action,
                            ppc_import_extra_cycles_for_binding(binding),
                        );
                    }
                }
                let trace_this_sprocket = trace_sprocket && is_sprocket_import(binding);
                let trace_this_qd3d = trace_qd3d
                    && (is_quickdraw_3d_library(&binding.library_name)
                        || is_quickdraw_3d_accelerator_library(&binding.library_name));
                let entry = if trace_imports || trace_ppc || trace_this_sprocket || trace_this_qd3d
                {
                    let entry = PpcHleImportTraceEntry {
                        import_index: index,
                        library_name: binding.library_name.clone(),
                        symbol_name: binding.symbol_name.clone(),
                        pc: cpu.pc,
                        lr: cpu.lr,
                        rtoc: cpu.gpr[2],
                        sp: cpu.gpr[1],
                        dispatcher_target: binding.dispatcher_target.clone(),
                        repeat_count: 1,
                    };
                    if trace_ppc {
                        eprintln!("{}", format_ppc_trace_import(&entry));
                    }
                    if trace_imports {
                        push_ppc_hle_import_trace_entry(&mut import_trace, entry.clone());
                    }
                    Some(entry)
                } else {
                    None
                };
                let import_args = [
                    cpu.gpr[3], cpu.gpr[4], cpu.gpr[5], cpu.gpr[6], cpu.gpr[7], cpu.gpr[8],
                ];
                let qd3d_before = if trace_this_qd3d {
                    Some(qd3d_trace_snapshot(
                        &q3_objects,
                        &q3_views,
                        &q3_submissions,
                        &q3_completed_frames,
                        &q3_state_only_completed_frame_batches,
                        &q3_memory_storages,
                        &q3_files,
                        &q3_group_memberships,
                        &q3_trimeshes,
                        &q3_draw_contexts,
                        &q3_mipmap_textures,
                        &q3_texture_shaders,
                        &q3_styles,
                        &q3_cameras,
                        &q3_lights,
                    ))
                } else {
                    None
                };

                // ResErr is canonical process low memory. Refresh it at each
                // import boundary so a preceding 68K callback is visible to
                // native Resource Manager entry points immediately.
                last_resource_error = memory
                    .read_u16_be(crate::memory::globals::addr::RES_ERR)
                    .unwrap_or(last_resource_error as u16)
                    as i16;

                let native_heap = process_memory_manager
                    .native_heap_state()
                    .expect("native allocator registered during execution");
                let mut heap_cursor = native_heap.heap_cursor;
                let native_heap_ceiling = native_heap.heap_limit;
                // Allocation and capacity imports observe ApplLimit inside
                // the mapped native heap. Keep the physical ceiling separate
                // for SetApplLimit validation and stack protection. Inside
                // Macintosh: Memory (1992), pp. 2-42--2-44 and 2-83--2-85.
                let heap_limit =
                    process_memory_manager.native_allocation_limit(native_heap_ceiling);
                let mut last_mem_error = native_heap.last_mem_error;
                if process_quickdraw_port_state_attached {
                    ppc_restore_process_port_draw_state(
                        memory,
                        *current_gworld,
                        &mut quickdraw_fore_color,
                        &mut quickdraw_back_color,
                        &mut quickdraw_pen_h,
                        &mut quickdraw_pen_v,
                        &mut quickdraw_text_mode,
                        &mut quickdraw_text_size,
                    );
                }
                let action = if binding.dispatcher_target == PpcImportDispatcherTarget::GlmPageFreeAll {
                    let pending = (cpu.lr == cpu.pc)
                        .then(|| glm_callback_stack.last().copied())
                        .flatten()
                        .filter(|state| {
                            state.import_pc == cpu.pc
                                && matches!(
                                    state.operation,
                                    PpcGlmCallbackOperation::Free { free_all: true, .. }
                                )
                        });
                    let reentrant = pending.is_none() && !glm_page_free_all_queue.is_empty();
                    let preserved = if let Some(state) = pending {
                        glm_callback_stack.pop();
                        let PpcGlmCallbackOperation::Free { pointer, result, .. } =
                            state.operation else { unreachable!() };
                        glm_allocations.remove(&pointer);
                        cpu.lr = state.final_pc;
                        cpu.gpr[2] = state.restore_rtoc;
                        result
                    } else if glm_page_free_all_queue.is_empty() {
                        let mut pointers: Vec<u32> = glm_allocations.keys().copied().collect();
                        pointers.sort_unstable();
                        glm_page_free_all_queue.extend(pointers);
                        cpu.gpr[3]
                    } else {
                        glm_error = 3; // GLM_INVALID_OPERATION (reentrant free-all)
                        cpu.gpr[3]
                    };
                    let mut callback_action = None;
                    if !reentrant {
                        while let Some(pointer) = glm_page_free_all_queue.pop_front() {
                            match glm_allocations.get(&pointer).copied() {
                                Some((true, _)) => {
                                    let Some(target) = glm_callbacks[1] else {
                                        glm_error = 3; // GLM_INVALID_OPERATION
                                        glm_page_free_all_queue.clear();
                                        break;
                                    };
                                    let state = PpcGlmCallbackState {
                                        import_pc: cpu.pc,
                                        final_pc: cpu.lr,
                                        restore_rtoc: cpu.gpr[2],
                                        operation: PpcGlmCallbackOperation::Free {
                                            pointer,
                                            result: preserved,
                                            replacement_size: None,
                                            free_all: true,
                                        },
                                    };
                                    callback_action = ppc_glm_begin_guest_callback(
                                        cpu,
                                        memory,
                                        &mut glm_callback_stack,
                                        target,
                                        pointer,
                                        state,
                                    );
                                    if callback_action.is_none() {
                                        glm_error = 3; // GLM_INVALID_OPERATION
                                        glm_page_free_all_queue.clear();
                                    }
                                    break;
                                }
                                Some((false, _)) => {
                                    process_memory_manager.dispose_native_ptr(pointer);
                                    glm_allocations.remove(&pointer);
                                    ppc_apply_process_native_allocator(
                                        process_memory_manager,
                                        memory,
                                        &mut heap_cursor,
                                        &mut last_mem_error,
                                    );
                                }
                                None => {}
                            }
                        }
                    }
                    callback_action.or(Some(PpcImportAction::Return(preserved)))
                } else if binding.dispatcher_target == PpcImportDispatcherTarget::GlmRealloc {
                    let pending = (cpu.lr == cpu.pc)
                        .then(|| glm_callback_stack.last().copied())
                        .flatten()
                        .filter(|state| state.import_pc == cpu.pc);
                    if let Some(state) = pending {
                        glm_callback_stack.pop();
                        cpu.lr = state.final_pc;
                        cpu.gpr[2] = state.restore_rtoc;
                        match state.operation {
                            PpcGlmCallbackOperation::Allocate {
                                size,
                                replace: Some((old_pointer, copy_size)),
                                ..
                            } => {
                                let new_pointer = cpu.gpr[3];
                                if new_pointer == 0 {
                                    glm_error = 4; // GLM_OUT_OF_MEMORY
                                    Some(PpcImportAction::Return(0))
                                } else if new_pointer == old_pointer {
                                    glm_allocations.insert(old_pointer, (true, size));
                                    Some(PpcImportAction::Return(old_pointer))
                                } else if !ppc_glm_copy_guest_range(
                                    memory,
                                    old_pointer,
                                    new_pointer,
                                    copy_size,
                                ) {
                                    glm_error = 2; // GLM_INVALID_VALUE
                                    if let Some(free_target) = glm_callbacks[1] {
                                        let cleanup = PpcGlmCallbackState {
                                            import_pc: cpu.pc,
                                            final_pc: state.final_pc,
                                            restore_rtoc: state.restore_rtoc,
                                            operation: PpcGlmCallbackOperation::Free {
                                                pointer: new_pointer,
                                                result: 0,
                                                replacement_size: None,
                                                free_all: false,
                                            },
                                        };
                                        ppc_glm_begin_guest_callback(
                                            cpu,
                                            memory,
                                            &mut glm_callback_stack,
                                            free_target,
                                            new_pointer,
                                            cleanup,
                                        )
                                        .or(Some(PpcImportAction::Return(0)))
                                    } else {
                                        Some(PpcImportAction::Return(0))
                                    }
                                } else if let Some(free_target) = glm_callbacks[1] {
                                    let next = PpcGlmCallbackState {
                                        import_pc: cpu.pc,
                                        final_pc: state.final_pc,
                                        restore_rtoc: state.restore_rtoc,
                                        operation: PpcGlmCallbackOperation::Free {
                                            pointer: old_pointer,
                                            result: new_pointer,
                                            replacement_size: Some(size),
                                            free_all: false,
                                        },
                                    };
                                    let action = ppc_glm_begin_guest_callback(
                                        cpu,
                                        memory,
                                        &mut glm_callback_stack,
                                        free_target,
                                        old_pointer,
                                        next,
                                    );
                                    if action.is_none() {
                                        glm_error = 3; // GLM_INVALID_OPERATION
                                    }
                                    action.or(Some(PpcImportAction::Return(0)))
                                } else {
                                    glm_error = 3; // GLM_INVALID_OPERATION
                                    Some(PpcImportAction::Return(0))
                                }
                            }
                            PpcGlmCallbackOperation::Allocate {
                                size,
                                replace: None,
                                ..
                            } => {
                                let pointer = cpu.gpr[3];
                                if pointer == 0 {
                                    glm_error = 4; // GLM_OUT_OF_MEMORY
                                } else {
                                    glm_allocations.insert(pointer, (true, size));
                                }
                                Some(PpcImportAction::Return(pointer))
                            }
                            PpcGlmCallbackOperation::Free {
                                pointer,
                                result,
                                replacement_size,
                                free_all: _,
                            } => {
                                glm_allocations.remove(&pointer);
                                if let Some(size) = replacement_size {
                                    glm_allocations.insert(result, (true, size));
                                }
                                Some(PpcImportAction::Return(result))
                            }
                        }
                    } else {
                        let old_pointer = cpu.gpr[3];
                        let size = cpu.gpr[4];
                        let previous = if old_pointer == 0 {
                            None
                        } else {
                            glm_allocations.get(&old_pointer).copied()
                        };
                        if old_pointer != 0 && previous.is_none() {
                            glm_error = 2; // GLM_INVALID_VALUE
                            Some(PpcImportAction::Return(0))
                        } else if size == 0 && old_pointer != 0 {
                            if previous.is_some_and(|(callback, _)| callback) {
                                if let Some(target) = glm_callbacks[1] {
                                    let state = PpcGlmCallbackState {
                                        import_pc: cpu.pc,
                                        final_pc: cpu.lr,
                                        restore_rtoc: cpu.gpr[2],
                                        operation: PpcGlmCallbackOperation::Free {
                                            pointer: old_pointer,
                                            result: 0,
                                            replacement_size: None,
                                            free_all: false,
                                        },
                                    };
                                    let action = ppc_glm_begin_guest_callback(
                                        cpu,
                                        memory,
                                        &mut glm_callback_stack,
                                        target,
                                        old_pointer,
                                        state,
                                    );
                                    if action.is_none() {
                                        glm_error = 3; // GLM_INVALID_OPERATION
                                    }
                                    action.or(Some(PpcImportAction::Return(0)))
                                } else {
                                    glm_error = 3; // GLM_INVALID_OPERATION
                                    Some(PpcImportAction::Return(0))
                                }
                            } else {
                                process_memory_manager.dispose_native_ptr(old_pointer);
                                ppc_apply_process_native_allocator(
                                    process_memory_manager,
                                    memory,
                                    &mut heap_cursor,
                                    &mut last_mem_error,
                                );
                                glm_allocations.remove(&old_pointer);
                                Some(PpcImportAction::Return(0))
                            }
                        } else if previous.is_some_and(|(callback, _)| callback)
                            || (old_pointer == 0 && glm_mode == Some(1))
                        {
                            if let Some(target) = glm_callbacks[0]
                                .filter(|_| old_pointer == 0 || glm_callbacks[1].is_some())
                            {
                                let replace = previous.map(|(_, old_size)| {
                                    (old_pointer, old_size.min(size))
                                });
                                let state = PpcGlmCallbackState {
                                    import_pc: cpu.pc,
                                    final_pc: cpu.lr,
                                    restore_rtoc: cpu.gpr[2],
                                    operation: PpcGlmCallbackOperation::Allocate {
                                        size,
                                        zero_on_return: false,
                                        replace,
                                    },
                                };
                                let action = ppc_glm_begin_guest_callback(
                                    cpu,
                                    memory,
                                    &mut glm_callback_stack,
                                    target,
                                    size,
                                    state,
                                );
                                if action.is_none() {
                                    glm_error = 3; // GLM_INVALID_OPERATION
                                }
                                action.or(Some(PpcImportAction::Return(0)))
                            } else {
                                glm_error = 3; // GLM_INVALID_OPERATION
                                Some(PpcImportAction::Return(0))
                            }
                        } else {
                            let pointer = process_memory_manager.reallocate_native_ptr(
                                memory,
                                old_pointer,
                                size,
                            );
                            ppc_apply_process_native_allocator(
                                process_memory_manager,
                                memory,
                                &mut heap_cursor,
                                &mut last_mem_error,
                            );
                            if pointer == 0 {
                                glm_error = 4; // GLM_OUT_OF_MEMORY
                            } else {
                                if old_pointer != 0 {
                                    glm_allocations.remove(&old_pointer);
                                }
                                glm_allocations.insert(pointer, (false, size));
                            }
                            Some(PpcImportAction::Return(pointer))
                        }
                    }
                } else if matches!(
                    binding.dispatcher_target,
                    PpcImportDispatcherTarget::GlmMalloc | PpcImportDispatcherTarget::GlmCalloc
                ) {
                    let is_calloc = binding.dispatcher_target == PpcImportDispatcherTarget::GlmCalloc;
                    let requested_size = if is_calloc {
                        cpu.gpr[3].checked_mul(cpu.gpr[4])
                    } else {
                        Some(cpu.gpr[3])
                    };
                    if cpu.lr == cpu.pc
                        && glm_callback_stack.last().is_some_and(|state| {
                            state.import_pc == cpu.pc
                                && matches!(state.operation, PpcGlmCallbackOperation::Free { .. })
                        })
                    {
                        let state = glm_callback_stack.pop().unwrap();
                        let PpcGlmCallbackOperation::Free { pointer, result, .. } =
                            state.operation else { unreachable!() };
                        cpu.lr = state.final_pc;
                        cpu.gpr[2] = state.restore_rtoc;
                        glm_allocations.remove(&pointer);
                        Some(PpcImportAction::Return(result))
                    } else if cpu.lr == cpu.pc
                        && glm_callback_stack
                            .last()
                            .is_some_and(|state| {
                                state.import_pc == cpu.pc
                                    && matches!(state.operation, PpcGlmCallbackOperation::Allocate { .. })
                            })
                    {
                        let state = glm_callback_stack.pop().unwrap();
                        let PpcGlmCallbackOperation::Allocate { size, zero_on_return, .. } =
                            state.operation else { unreachable!() };
                        let pointer = cpu.gpr[3];
                        cpu.lr = state.final_pc;
                        cpu.gpr[2] = state.restore_rtoc;
                        if pointer == 0 {
                            glm_error = 4; // GLM_OUT_OF_MEMORY
                            Some(PpcImportAction::Return(0))
                        } else if zero_on_return
                            && !ppc_glm_zero_guest_range(memory, pointer, size)
                        {
                            glm_error = 2; // GLM_INVALID_VALUE
                            if let Some(free_target) = glm_callbacks[1] {
                                let cleanup = PpcGlmCallbackState {
                                    import_pc: cpu.pc,
                                    final_pc: state.final_pc,
                                    restore_rtoc: state.restore_rtoc,
                                    operation: PpcGlmCallbackOperation::Free {
                                        pointer,
                                        result: 0,
                                        replacement_size: None,
                                        free_all: false,
                                    },
                                };
                                ppc_glm_begin_guest_callback(
                                    cpu,
                                    memory,
                                    &mut glm_callback_stack,
                                    free_target,
                                    pointer,
                                    cleanup,
                                )
                                .or(Some(PpcImportAction::Return(0)))
                            } else {
                                Some(PpcImportAction::Return(0))
                            }
                        } else {
                            glm_allocations.insert(pointer, (true, size));
                            Some(PpcImportAction::Return(pointer))
                        }
                    } else if requested_size.is_none() {
                        glm_error = 2; // GLM_INVALID_VALUE
                        Some(PpcImportAction::Return(0))
                    } else if glm_mode == Some(1) {
                        let requested_size = requested_size.unwrap();
                        if let Some(target) = glm_callbacks[0] {
                            let state = PpcGlmCallbackState {
                                import_pc: cpu.pc,
                                final_pc: cpu.lr,
                                restore_rtoc: cpu.gpr[2],
                                operation: PpcGlmCallbackOperation::Allocate {
                                    size: requested_size,
                                    zero_on_return: is_calloc,
                                    replace: None,
                                },
                            };
                            let action = ppc_glm_begin_guest_callback(
                                cpu,
                                memory,
                                &mut glm_callback_stack,
                                target,
                                requested_size,
                                state,
                            );
                            if action.is_none() {
                                glm_error = 3; // GLM_INVALID_OPERATION
                            }
                            action.or(Some(PpcImportAction::Return(0)))
                        } else {
                            glm_error = 3; // GLM_INVALID_OPERATION
                            Some(PpcImportAction::Return(0))
                        }
                    } else {
                        let pointer = process_memory_manager.new_native_ptr(
                            memory,
                            requested_size.unwrap(),
                            is_calloc,
                        );
                        ppc_apply_process_native_allocator(
                            process_memory_manager,
                            memory,
                            &mut heap_cursor,
                            &mut last_mem_error,
                        );
                        if pointer == 0 {
                            glm_error = 4; // GLM_OUT_OF_MEMORY
                        } else {
                            glm_allocations.insert(pointer, (false, requested_size.unwrap()));
                        }
                        Some(PpcImportAction::Return(pointer))
                    }
                } else if binding.dispatcher_target == PpcImportDispatcherTarget::GlmFree {
                    if cpu.lr == cpu.pc
                        && glm_callback_stack
                            .last()
                            .is_some_and(|state| {
                                state.import_pc == cpu.pc
                                    && matches!(state.operation, PpcGlmCallbackOperation::Free { .. })
                            })
                    {
                        let state = glm_callback_stack.pop().unwrap();
                        let PpcGlmCallbackOperation::Free { pointer, result, replacement_size, free_all: _ } =
                            state.operation else { unreachable!() };
                        cpu.lr = state.final_pc;
                        cpu.gpr[2] = state.restore_rtoc;
                        glm_allocations.remove(&pointer);
                        if let Some(size) = replacement_size {
                            glm_allocations.insert(result, (true, size));
                        }
                        Some(PpcImportAction::Return(result))
                    } else {
                        let pointer = cpu.gpr[3];
                        match glm_allocations.get(&pointer).copied() {
                            None if pointer == 0 => Some(PpcImportAction::ReturnPreserve),
                            None => {
                                glm_error = 2; // GLM_INVALID_VALUE
                                Some(PpcImportAction::ReturnPreserve)
                            }
                            Some((true, _)) => {
                                if let Some(target) = glm_callbacks[1] {
                                    let state = PpcGlmCallbackState {
                                        import_pc: cpu.pc,
                                        final_pc: cpu.lr,
                                        restore_rtoc: cpu.gpr[2],
                                        operation: PpcGlmCallbackOperation::Free {
                                            pointer,
                                            result: pointer,
                                            replacement_size: None,
                                            free_all: false,
                                        },
                                    };
                                    let action = ppc_glm_begin_guest_callback(
                                        cpu,
                                        memory,
                                        &mut glm_callback_stack,
                                        target,
                                        pointer,
                                        state,
                                    );
                                    if action.is_none() {
                                        glm_error = 3; // GLM_INVALID_OPERATION
                                    }
                                    action.or(Some(PpcImportAction::ReturnPreserve))
                                } else {
                                    glm_error = 3; // GLM_INVALID_OPERATION
                                    Some(PpcImportAction::ReturnPreserve)
                                }
                            }
                            Some((false, _)) => {
                                process_memory_manager.dispose_native_ptr(pointer);
                                ppc_apply_process_native_allocator(
                                    process_memory_manager,
                                    memory,
                                    &mut heap_cursor,
                                    &mut last_mem_error,
                                );
                                glm_allocations.remove(&pointer);
                                Some(PpcImportAction::ReturnPreserve)
                            }
                        }
                    }
                } else if binding.dispatcher_target == PpcImportDispatcherTarget::GlmSetMode {
                    // AGL/glm.h (Mac OS 9): glmSetMode is void and accepts
                    // exactly these four memory configuration selectors.
                    match cpu.gpr[3] {
                        1..=4 => glm_mode = Some(cpu.gpr[3]),
                        _ => glm_error = 1, // GLM_INVALID_ENUM
                    }
                    Some(PpcImportAction::ReturnPreserve)
                } else if binding.dispatcher_target == PpcImportDispatcherTarget::GlmGetError {
                    let error = glm_error;
                    glm_error = 0;
                    Some(PpcImportAction::Return(error))
                } else if binding.dispatcher_target == PpcImportDispatcherTarget::GlmSetFunc {
                    // glm.h: GLMfunctions is a union of function pointers, passed
                    // directly in r4 by the native PowerPC ABI.
                    let selector = cpu.gpr[3];
                    let pointer = cpu.gpr[4];
                    if !(1..=8).contains(&selector) {
                        glm_error = 1; // GLM_INVALID_ENUM
                    } else if pointer == 0 {
                        glm_callbacks[(selector - 1) as usize] = None;
                    } else if let Some(target) = ppc_resolve_callback_target(
                        memory, pointer, cpu.gpr[2], None,
                    ).filter(|target| memory.read_u32_be(target.entry).is_some()) {
                        glm_callbacks[(selector - 1) as usize] = Some(target);
                    } else {
                        glm_error = 2; // GLM_INVALID_VALUE
                    }
                    Some(PpcImportAction::ReturnPreserve)
                } else if binding.dispatcher_target == PpcImportDispatcherTarget::GetKeys {
                    Some(dispatch_getkeys_import(
                        cpu,
                        memory,
                        input,
                        Some(&mut idle_poll_counts),
                    ))
                } else if let Some(action) = dispatch_qd3d::dispatch_q3_math_import_fast(
                    dispatch_qd3d::PpcQ3MathDispatchContext {
                        target: &binding.dispatcher_target,
                        cpu,
                        memory,
                        q3_objects: &mut q3_objects,
                        next_q3_object: &mut next_q3_object,
                        q3_error_state: &mut q3_error_state,
                    },
                ) {
                    Some(action)
                } else if let Some(action) = dispatch_qd3d::dispatch_q3_shader_style_import_fast(
                    dispatch_qd3d::PpcQ3ShaderStyleDispatchContext {
                        target: &binding.dispatcher_target,
                        cpu,
                        memory,
                        q3_objects: &mut q3_objects,
                        q3_object_refs: &mut q3_object_refs,
                        q3_error_state: &mut q3_error_state,
                        next_q3_object: &mut next_q3_object,
                        q3_texture_shaders: &mut q3_texture_shaders,
                        q3_mipmap_textures: &mut q3_mipmap_textures,
                        q3_shader_uv_transforms: &mut q3_shader_uv_transforms,
                        q3_shader_boundaries: &mut q3_shader_boundaries,
                        q3_styles: &mut q3_styles,
                    },
                ) {
                    Some(action)
                } else if let Some(action) = dispatch_qd3d::dispatch_q3_scene_import_fast(
                    dispatch_qd3d::PpcQ3SceneDispatchContext {
                        target: &binding.dispatcher_target,
                        cpu,
                        memory,
                        q3_objects: &mut q3_objects,
                        next_q3_object: &mut next_q3_object,
                        q3_cameras: &mut q3_cameras,
                        q3_lights: &mut q3_lights,
                        q3_error_state: &mut q3_error_state,
                    },
                ) {
                    Some(action)
                } else if let Some(action) = dispatch_qd3d::dispatch_q3_object_renderer_import_fast(
                    dispatch_qd3d::PpcQ3ObjectRendererDispatchContext {
                        target: &binding.dispatcher_target,
                        cpu,
                        memory,
                        stores: PpcQ3ObjectStores {
                            q3_objects: &mut q3_objects,
                            q3_object_refs: &mut q3_object_refs,
                            q3_renderer_preferences: &mut q3_renderer_preferences,
                            q3_files: &mut q3_files,
                            q3_group_memberships: &mut q3_group_memberships,
                            q3_file_groups: &mut q3_file_groups,
                            q3_views: &mut q3_views,
                            q3_submissions: &mut q3_submissions,
                            q3_view_transforms: &mut q3_view_transforms,
                            q3_submission_transforms: &mut q3_submission_transforms,
                            q3_view_materials: &mut q3_view_materials,
                            q3_submission_materials: &mut q3_submission_materials,
                            q3_submission_lights: &mut q3_submission_lights,
                            q3_view_state_stack: &mut q3_view_state_stack,
                            q3_completed_frames: &mut q3_completed_frames,
                            q3_retained_frames: &mut q3_retained_frames,
                            q3_fog_styles: &mut q3_fog_styles,
                            q3_memory_storages: &mut q3_memory_storages,
                            q3_attributes: &mut q3_attributes,
                            q3_shader_uv_transforms: &mut q3_shader_uv_transforms,
                            q3_shader_boundaries: &mut q3_shader_boundaries,
                            q3_mipmap_textures: &mut q3_mipmap_textures,
                            q3_texture_shaders: &mut q3_texture_shaders,
                            q3_draw_contexts: &mut q3_draw_contexts,
                            q3_trimeshes: &mut q3_trimeshes,
                            q3_styles: &mut q3_styles,
                            q3_cameras: &mut q3_cameras,
                            q3_lights: &mut q3_lights,
                        },
                        q3_error_state: &mut q3_error_state,
                        next_q3_object: &mut next_q3_object,
                    },
                ) {
                    Some(action)
                } else if let Some(action) = dispatch_qd3d::dispatch_q3_object_group_import_fast(
                    dispatch_qd3d::PpcQ3ObjectGroupDispatchContext {
                        target: &binding.dispatcher_target,
                        cpu,
                        memory,
                        q3_objects: &q3_objects,
                        q3_object_refs: &mut q3_object_refs,
                        q3_group_memberships: &q3_group_memberships,
                        q3_file_groups: &q3_file_groups,
                        q3_lights: &q3_lights,
                        q3_error_state: &mut q3_error_state,
                    },
                ) {
                    Some(action)
                } else if let Some(action) = dispatch_simple_hot_import_fast(
                    &binding.dispatcher_target,
                    cpu,
                    memory,
                    ppc_virtual_microseconds(
                        process_tick,
                        clock_cycles_per_tick,
                        clock_cycle_phase,
                        elapsed,
                    ),
                ) {
                    Some(action)
                } else if let Some(action) = dispatch_qd3d::dispatch_q3_submit_import_fast(
                    dispatch_qd3d::PpcQ3SubmitDispatchContext {
                        target: &binding.dispatcher_target,
                        cpu,
                        memory,
                        q3_objects: &q3_objects,
                        q3_group_memberships: &q3_group_memberships,
                        q3_views: &mut q3_views,
                        q3_view_transforms: &mut q3_view_transforms,
                        q3_submissions: &mut q3_submissions,
                        q3_submission_transforms: &mut q3_submission_transforms,
                        q3_view_materials: &mut q3_view_materials,
                        q3_submission_materials: &mut q3_submission_materials,
                        q3_submission_lights: &mut q3_submission_lights,
                        q3_view_state_stack: &mut q3_view_state_stack,
                        q3_attributes: &q3_attributes,
                        q3_styles: &q3_styles,
                        q3_shader_boundaries: &q3_shader_boundaries,
                        q3_shader_uv_transforms: &q3_shader_uv_transforms,
                        q3_texture_shaders: &q3_texture_shaders,
                        q3_mipmap_textures: &q3_mipmap_textures,
                        q3_trimeshes: &q3_trimeshes,
                        q3_fog_styles: &mut q3_fog_styles,
                        q3_lights: &q3_lights,
                        q3_immediate_trimeshes: &mut q3_immediate_trimeshes,
                        q3_error_state: &mut q3_error_state,
                    },
                ) {
                    Some(action)
                } else {
                    // Borrow process-owned File and Resource Manager records
                    // only for this import. A Mixed Mode continuation may
                    // expose another adapter between imports, so no mutable
                    // reference into an UnsafeCell-backed process collection
                    // may outlive this dispatch call or be retained in its
                    // returned action.
                    let action = process_file_system.with_mut(|file_system| {
                        let launched_app_path = file_system.launched_app_path.clone();
                        let ProcessFileSystemState {
                            files,
                            writable_refnums,
                            stdio_streams,
                            vfs_files,
                            deleted_vfs_file_paths,
                            resource_manager,
                            next_file_ref_num,
                            ..
                        } = file_system;
                        screen_clut.with_mut(|screen_clut| {
                            color_manager_clut.with_mut(|color_manager_clut| {
                                event_queue.with_mut(|event_queue| {
                                    controls.with_mut(|controls| {
                                        list_manager.with_mut(|list_manager| {
                                            writable_refnums.with_mut(|writable_refnums| {
                                            vfs_directories.with_mut(|vfs_directories| {
                                            working_directories.with_mut(|working_directories| {
                                            next_working_directory_ref_num.with_mut(|next_working_directory_ref_num| {
                                            application_working_directory_ref_num.with_mut(|application_working_directory_ref_num| {
                                            next_vfs_dir_id.with_mut(|next_vfs_dir_id| {
                                            current_resource_refnum.with_mut(|current_resource_refnum| {
                                            resource_manager.with_mut(|resource_manager| {
                                            let ProcessResourceManagerState {
                                                resource_files,
                                                vfs_resource_files,
                                                vfs_resources,
                                                ..
                                            } = resource_manager;
                                            current_gworld.with_mut(|current_gworld| {
                                            current_gdevice.with_mut(|current_gdevice| {
                                            files.with_mut(|files| {
                                            let previous_port = *current_gworld;
                                            let action = dispatch_supported_import(PpcDispatchContext {
                                            binding,
                                            agl: &mut agl,
                                            cpu,
                                            memory,
                                            process_memory_manager: &mut *process_memory_manager,
                                            heap_cursor: &mut heap_cursor,
                                            heap_limit,
                                            stack_base,
                                            stack_top,
                                            native_heap_ceiling,
                                            last_mem_error: &mut last_mem_error,
                                            tick_count: &mut import_tick_count,
                                            cycles_per_tick: clock_cycles_per_tick,
                                            current_resource_refnum,
                                            last_resource_error: &mut last_resource_error,
                                            resource_policy: &resource_policy,
                                            native_exception_handler: &native_exception_handler,
                                            stdc_qsort_stack: &mut stdc_qsort_stack,
                                            dialog_callback_stack: &mut dialog_callback_stack,
                                            collection_callback_stack: &mut collection_callback_stack,
                                            apple_events: &mut apple_events,
                                            cfm_connections: &mut cfm_connections,
                                            cfm_library_fragments: &mut cfm_library_fragments,
                                            next_cfm_connection_id: &mut next_cfm_connection_id,
                                            import_run_state: &mut import_run_state,
                                            controls,
                                            aliases: &mut aliases,
                                            gworlds: &mut gworlds,
                                            gworld_pixel_states: &gworld_pixel_states,
                                            window_list: &window_list,
                                            q3_objects: &mut q3_objects,
                                            q3_object_refs: &mut q3_object_refs,
                                            next_q3_object: &mut next_q3_object,
                                            q3_error_state: &mut q3_error_state,
                                            q3_lifecycle: &mut q3_lifecycle,
                                            q3_memory_storages: &mut q3_memory_storages,
                                            q3_files: &mut q3_files,
                                            q3_group_memberships: &mut q3_group_memberships,
                                            q3_file_groups: &mut q3_file_groups,
                                            q3_views: &mut q3_views,
                                            q3_submissions: &mut q3_submissions,
                                            q3_view_transforms: &mut q3_view_transforms,
                                            q3_submission_transforms: &mut q3_submission_transforms,
                                            q3_view_materials: &mut q3_view_materials,
                                            q3_submission_materials: &mut q3_submission_materials,
                                            q3_submission_lights: &mut q3_submission_lights,
                                            q3_view_state_stack: &mut q3_view_state_stack,
                                            q3_completed_frames: &mut q3_completed_frames,
                                            q3_retained_frames: &mut q3_retained_frames,
                                            q3_state_only_completed_frame_batches:
                                                &mut q3_state_only_completed_frame_batches,
                                            q3_fog_styles: &mut q3_fog_styles,
                                            q3_attributes: &mut q3_attributes,
                                            q3_shader_uv_transforms: &mut q3_shader_uv_transforms,
                                            q3_shader_boundaries: &mut q3_shader_boundaries,
                                            q3_mipmap_textures: &mut q3_mipmap_textures,
                                            q3_texture_shaders: &mut q3_texture_shaders,
                                            q3_renderer_preferences: &mut q3_renderer_preferences,
                                            q3_draw_contexts: &mut q3_draw_contexts,
                                            q3_trimeshes: &mut q3_trimeshes,
                                            q3_styles: &mut q3_styles,
                                            q3_cameras: &mut q3_cameras,
                                            q3_lights: &mut q3_lights,
                                            input_sprocket: &mut input_sprocket,
                                            input_sprocket_virtual_elements:
                                                &mut input_sprocket_virtual_elements,
                                            toolbox_startup: &mut toolbox_startup,
                                            quicktime: &mut quicktime,
                                            sound: &mut sound,
                                            timer_tasks: &timer_tasks,
                                            vbl_tasks: &vbl_tasks,
                                            callback_scheduling: &callback_scheduling,
                                            files,
                                            writable_refnums,
                                            vfs_files,
                                            stdio_streams,
                                            deleted_vfs_file_paths,
                                            resource_files,
                                            vfs_resource_files,
                                            vfs_resources,
                                            next_file_ref_num,
                                            current_gworld,
                                            current_gdevice,
                                            quickdraw_op_colors: &quickdraw_op_colors,
                                            quickdraw_hilite_colors: &quickdraw_hilite_colors,
                                            screen_clut,
                                            color_manager_clut,
                                            display_gamma: &display_gamma,
                                            quickdraw_fore_color: &mut quickdraw_fore_color,
                                            quickdraw_fore_indices: &mut quickdraw_fore_indices,
                                            quickdraw_back_color: &mut quickdraw_back_color,
                                            quickdraw_pen_h: &mut quickdraw_pen_h,
                                            quickdraw_pen_v: &mut quickdraw_pen_v,
                                            quickdraw_text_mode: &mut quickdraw_text_mode,
                                            quickdraw_text_size: &mut quickdraw_text_size,
                                            cursor_state: &cursor_state,
                                            vfs_volumes: &vfs_volumes,
                                            vfs_directories,
                                            next_vfs_dir_id,
                                            default_dir_id: *default_dir_id,
                                            working_directories,
                                            next_working_directory_ref_num,
                                            application_working_directory_ref_num,
                                            launched_app_path: launched_app_path.as_deref(),
                                            param_text: &param_text,
                                            scrap: &mut scrap,
                                            list_manager,
                                            collections: &collections,
                                            input,
                                            event_queue,
                                            draw_sprocket: &mut draw_sprocket,
                                            });
                                            // QDGlobals.thePort is guest-visible state. Keep it
                                            // consistent with SetPort and Toolbox operations that
                                            // select a window or dialog port internally.
                                            if toolbox_startup.init_graf_global_ptr != 0
                                                && previous_port != *current_gworld
                                                && matches!(
                                                    binding.dispatcher_target,
                                                    PpcImportDispatcherTarget::SetPort
                                                        | PpcImportDispatcherTarget::SetPortWindowPort
                                                        | PpcImportDispatcherTarget::SetPortDialogPort
                                                        | PpcImportDispatcherTarget::SetGWorld
                                                )
                                            {
                                                let _ = memory.write_u32_be(
                                                    toolbox_startup.init_graf_global_ptr,
                                                    *current_gworld,
                                                );
                                            }
                                            action
                                            })
                                            })
                                            })
                                            })
                                            })
                                            })
                                            })
                                            })
                                            })
                                            })
                                            })
                                        })
                                    })
                                })
                            })
                        })
                    });
                    action
                };

                ppc_sync_process_window_list(memory, &window_list);

                // MemError is process Memory Manager state even when a
                // Toolbox helper reports it through the native ABI cache.
                // Publish at the import boundary so a following 68K callback
                // observes the result immediately, not at slice teardown.
                process_memory_manager.set_native_mem_error(last_mem_error);
                let _ = memory.write_u16_be(
                    crate::memory::globals::addr::RES_ERR,
                    last_resource_error as u16,
                );

                // Resource records are the native Resource Manager's parsed
                // view, while the classic adapter opens the same process fork
                // through its byte map. Publish every dirty parsed mutation at
                // the import boundary so a following 68K callback observes it
                // without waiting for runner teardown or host persistence.
                process_file_system
                    .resource_manager
                    .with_mut(|resource_manager| {
                        ppc_publish_resource_fork_bytes(
                            &mut resource_manager.vfs_resource_files,
                            &resource_manager.vfs_resources,
                            true,
                        );
                    });

                let current_default_dir_id = *default_dir_id;
                let updated_default_dir_id = memory
                    .read_u32_be(crate::memory::globals::addr::CUR_DIR_STORE)
                    .unwrap_or(current_default_dir_id);
                default_dir_id.with_mut(|default_dir_id| {
                    *default_dir_id = updated_default_dir_id;
                });

                match action {
                    Some(action) => {
                        // Asynchronous File Manager calls return noErr as soon as
                        // the request is queued. Keep the completion for a later
                        // interrupt-work boundary without yielding out of an
                        // interrupt-time callback that called PBReadAsync.
                        // Inside Macintosh: Files (1992), 2-8–2-9.
                        let action = if matches!(
                            binding.library_name.as_str(),
                            "InterfaceLib" | "CarbonLib"
                        )
                            && binding.symbol_name == "PBReadAsync"
                            && matches!(action, PpcImportAction::Return(_))
                        {
                            let parameter_block = cpu.gpr[3];
                            let completion = memory.read_u32_be(parameter_block + 12).unwrap_or(0);
                            if completion != 0 {
                                pending_file_completions.push_back((parameter_block, completion));
                                PpcImportAction::Return(0)
                            } else {
                                action
                            }
                        } else {
                            action
                        };
                        let action = ppc_import_action_with_extra_cycles(
                            action,
                            ppc_import_extra_cycles_for_binding(binding),
                        );
                        if trace_imports {
                            if let (Some(action_name), Some(result)) = (
                                ppc_draw_sprocket_action_name(&binding.dispatcher_target),
                                ppc_i16_return_value(&action),
                            ) {
                                draw_sprocket_trace.push(ppc_draw_sprocket_trace_entry(
                                    index,
                                    cpu.pc,
                                    memory,
                                    cpu,
                                    action_name,
                                    result,
                                    &draw_sprocket,
                                ));
                            }
                        }
                        if trace_imports
                            && binding.dispatcher_target
                                == PpcImportDispatcherTarget::ISpElementGetSimpleState
                            && matches!(action, PpcImportAction::Return(0))
                        {
                            if let Some(entry) = ppc_isp_simple_state_trace_entry(
                                index,
                                cpu.pc,
                                cpu,
                                memory,
                                input,
                                input_sprocket,
                                &input_sprocket_virtual_elements,
                            ) {
                                input_sprocket_trace.push(entry);
                            }
                        }
                        if trace_this_sprocket {
                            if let Some(entry) = entry.as_ref() {
                                eprintln!(
                                    "{}",
                                    format_sprocket_trace(
                                        entry,
                                        import_args,
                                        &format_sprocket_action(&action),
                                        &draw_sprocket,
                                        &input_sprocket,
                                        &input_sprocket_virtual_elements
                                    )
                                );
                            }
                        }
                        if trace_this_qd3d {
                            if let (Some(entry), Some(before)) =
                                (entry.as_ref(), qd3d_before.as_ref())
                            {
                                let after = qd3d_trace_snapshot(
                                    &q3_objects,
                                    &q3_views,
                                    &q3_submissions,
                                    &q3_completed_frames,
                                    &q3_state_only_completed_frame_batches,
                                    &q3_memory_storages,
                                    &q3_files,
                                    &q3_group_memberships,
                                    &q3_trimeshes,
                                    &q3_draw_contexts,
                                    &q3_mipmap_textures,
                                    &q3_texture_shaders,
                                    &q3_styles,
                                    &q3_cameras,
                                    &q3_lights,
                                );
                                eprintln!(
                                    "{}",
                                    format_qd3d_trace(
                                        entry,
                                        import_args,
                                        &format_hle_import_action(&action),
                                        before,
                                        &after
                                    )
                                );
                            }
                        }
                        handled_import_count = handled_import_count.saturating_add(1);
                        guest_calls.externalize_powerpc_action(cpu, action)
                    }
                    None => {
                        if trace_this_sprocket {
                            if let Some(entry) = entry.as_ref() {
                                eprintln!(
                                    "{}",
                                    format_sprocket_trace(
                                        entry,
                                        import_args,
                                        "unsupported",
                                        &draw_sprocket,
                                        &input_sprocket,
                                        &input_sprocket_virtual_elements
                                    )
                                );
                            }
                        }
                        if trace_this_qd3d {
                            if let (Some(entry), Some(before)) =
                                (entry.as_ref(), qd3d_before.as_ref())
                            {
                                let after = qd3d_trace_snapshot(
                                    &q3_objects,
                                    &q3_views,
                                    &q3_submissions,
                                    &q3_completed_frames,
                                    &q3_state_only_completed_frame_batches,
                                    &q3_memory_storages,
                                    &q3_files,
                                    &q3_group_memberships,
                                    &q3_trimeshes,
                                    &q3_draw_contexts,
                                    &q3_mipmap_textures,
                                    &q3_texture_shaders,
                                    &q3_styles,
                                    &q3_cameras,
                                    &q3_lights,
                                );
                                eprintln!(
                                    "{}",
                                    format_qd3d_trace(
                                        entry,
                                        import_args,
                                        "unsupported",
                                        before,
                                        &after
                                    )
                                );
                            }
                        }
                        unsupported_import_index = Some(index);
                        PpcImportAction::Halt
                    }
                }
            };

            let mut total_cycles = 0u64;
            loop {
                let remaining_cycles = max_cycles.saturating_sub(total_cycles);
                if remaining_cycles == 0 {
                    break PpcRunResult::CycleLimit {
                        cycles: total_cycles,
                    };
                }
                let step_result = if let Some(range) = ppc_watch_range() {
                    let mut write_observer = PpcWatchObserver { range };
                    self.cpu.run_with_imports_and_observers_and_cycle_handler(
                        &mut self.memory,
                        remaining_cycles,
                        self.halt_pc,
                        self.import_trap_base,
                        PPC_IMPORT_SLOT_COUNT,
                        &mut fetch_observer,
                        &mut write_observer,
                        &mut handle_import,
                    )
                } else if needs_fetch_observer {
                    self.cpu
                        .run_with_imports_and_fetch_observer_and_cycle_handler(
                            &mut self.memory,
                            remaining_cycles,
                            self.halt_pc,
                            self.import_trap_base,
                            PPC_IMPORT_SLOT_COUNT,
                            &mut fetch_observer,
                            &mut handle_import,
                        )
                } else {
                    self.cpu.run_with_imports_and_cycle_handler(
                        &mut self.memory,
                        remaining_cycles,
                        self.halt_pc,
                        self.import_trap_base,
                        PPC_IMPORT_SLOT_COUNT,
                        &mut handle_import,
                    )
                };
                total_cycles = total_cycles.saturating_add(ppc_run_result_cycles(step_result));

                match step_result {
                    // A parked callback returned on its own frame:
                    // resume the code it interrupted. Exception handlers and
                    // the interrupted code's own returns hold a different r1.
                    PpcRunResult::Halted { pc, .. }
                        if pc == self.halt_pc
                            && self
                                .parked_interrupt_callback
                                .as_ref()
                                .is_some_and(|parked| {
                                    parked.callback_sp == self.cpu.gpr[1]
                                        && parked.task == guest_calls.current_task()
                                }) =>
                    {
                        let parked = self
                            .parked_interrupt_callback
                            .take()
                            .expect("matched a parked callback");
                        if ppc_hle_trace_enabled() {
                            eprintln!(
                                "[PPC-TRACE] parked callback returned: callback_sp=${:08X} resume_pc=${:08X}",
                                parked.callback_sp,
                                parked.interrupted.architectural().pc,
                            );
                        }
                        self.cpu.install_execution_context(parked.interrupted);
                        if let Some(refnum) = parked.interrupted_refnum {
                            self.set_current_resource_refnum(refnum);
                        }
                        // VBL and sound records are out of `self` until the
                        // slice ends; finish their work once they are back.
                        interrupt_return_work = Some(parked.on_return);
                        // The interrupted code was itself waiting on a 68K
                        // call: hand the slice back so the runner resumes it.
                        if guest_calls.has_m68k_execution() {
                            break PpcRunResult::CycleLimit {
                                cycles: total_cycles,
                            };
                        }
                    }
                    PpcRunResult::Exception { pc, exception, .. }
                        if native_exception_handler.get() != 0
                            && ppc_native_exception_kind(exception).is_some() =>
                    {
                        let Some(context) = ppc_begin_native_exception(
                            &mut self.cpu,
                            &mut self.memory,
                            self.stack_base,
                            native_exception_handler.get(),
                            pc,
                            PpcNativeExceptionCause::Processor(exception),
                        ) else {
                            break ppc_run_result_with_cycles(step_result, total_cycles);
                        };
                        native_exception_stack.push(context);
                    }
                    PpcRunResult::MemoryFault {
                        pc,
                        addr,
                        was_write,
                        ..
                    } if native_exception_handler.get() != 0 => {
                        let Some(context) = ppc_begin_native_exception(
                            &mut self.cpu,
                            &mut self.memory,
                            self.stack_base,
                            native_exception_handler.get(),
                            pc,
                            PpcNativeExceptionCause::UnmappedMemory {
                                address: addr,
                                was_write,
                            },
                        ) else {
                            break ppc_run_result_with_cycles(step_result, total_cycles);
                        };
                        native_exception_stack.push(context);
                    }
                    PpcRunResult::Halted { pc, .. }
                        if pc == self.halt_pc && !native_exception_stack.is_empty() =>
                    {
                        let context = native_exception_stack.pop().expect("checked nonempty");
                        let handler_result = self.cpu.gpr[3];
                        let restored =
                            ppc_restore_native_exception(&mut self.cpu, &mut self.memory, context)
                                .is_some();
                        if handler_result == 0 && restored {
                            continue;
                        }
                        native_exception_stack.clear();
                        break ppc_native_exception_result(context, total_cycles);
                    }
                    PpcRunResult::CycleLimit { .. } => {
                        break ppc_run_result_with_cycles(step_result, total_cycles);
                    }
                    _ => {
                        native_exception_stack.clear();
                        break ppc_run_result_with_cycles(step_result, total_cycles);
                    }
                }
            }
        };
        drop(fetch_observer);

        // Native execution never projects a scalar baseline back into guest
        // memory. Low-memory `Ticks` remains the source of truth, including
        // when a nested Mixed Mode callback changed it during this slice.
        self.native_exception_handler = native_exception_handler.get();
        self.native_exception_stack = native_exception_stack;
        self.stdc_qsort_stack = stdc_qsort_stack;
        self.dialog_callback_stack = dialog_callback_stack;
        self.collection_callback_stack = collection_callback_stack;
        self.pending_file_completions = pending_file_completions;
        self.apple_events = apple_events;
        self.cfm = standalone_cfm;
        (self.imports, self.import_count) = import_run_state.into_parts();
        self.controls = controls;
        self.aliases = aliases;
        self.gworlds = gworlds;
        self.agl = agl;
        self.q3_objects = q3_objects;
        self.q3_object_refs = q3_object_refs;
        self.next_q3_object = next_q3_object;
        self.q3_error_state = q3_error_state;
        self.q3_lifecycle = q3_lifecycle;
        self.q3_memory_storages = q3_memory_storages;
        self.q3_files = q3_files;
        self.q3_group_memberships = q3_group_memberships;
        self.q3_file_groups = q3_file_groups;
        self.q3_views = q3_views;
        self.q3_submissions = q3_submissions;
        self.q3_view_transforms = q3_view_transforms;
        self.q3_submission_transforms = q3_submission_transforms;
        self.q3_view_materials = q3_view_materials;
        self.q3_submission_materials = q3_submission_materials;
        self.q3_submission_lights = q3_submission_lights;
        self.q3_view_state_stack = q3_view_state_stack;
        self.q3_completed_frames = q3_completed_frames;
        self.q3_retained_frames = q3_retained_frames;
        self.q3_state_only_completed_frame_batches = q3_state_only_completed_frame_batches;
        self.q3_fog_styles = q3_fog_styles;
        self.q3_immediate_trimeshes = q3_immediate_trimeshes;
        self.q3_attributes = q3_attributes;
        self.q3_shader_uv_transforms = q3_shader_uv_transforms;
        self.q3_shader_boundaries = q3_shader_boundaries;
        self.q3_mipmap_textures = q3_mipmap_textures;
        self.q3_texture_shaders = q3_texture_shaders;
        self.q3_renderer_preferences = q3_renderer_preferences;
        self.q3_draw_contexts = q3_draw_contexts;
        self.q3_trimeshes = q3_trimeshes;
        self.q3_styles = q3_styles;
        self.q3_cameras = q3_cameras;
        self.q3_lights = q3_lights;
        self.input_sprocket = input_sprocket;
        self.input_sprocket_virtual_elements = input_sprocket_virtual_elements;
        self.toolbox_startup = toolbox_startup;
        self.quicktime = quicktime;
        self.sound = sound;
        self.glm_mode = glm_mode;
        self.glm_callbacks = glm_callbacks;
        self.glm_callback_stack = glm_callback_stack;
        self.glm_allocations = glm_allocations;
        self.glm_page_free_all_queue = glm_page_free_all_queue;
        self.glm_error = glm_error;
        self.timer_tasks = timer_tasks;
        self.vbl_tasks = vbl_tasks;
        self.quickdraw_fore_color = quickdraw_fore_color;
        self.quickdraw_fore_indices = quickdraw_fore_indices;
        self.quickdraw_back_color = quickdraw_back_color;
        self.quickdraw_pen_h = quickdraw_pen_h;
        self.quickdraw_pen_v = quickdraw_pen_v;
        self.quickdraw_text_mode = quickdraw_text_mode;
        self.quickdraw_text_size = quickdraw_text_size;
        self.cursor_state = cursor_state;
        process_file_system.with_mut(ProcessFileSystemState::publish_native_vfs_catalogue);
        self.scrap = scrap;
        self.list_manager = list_manager;
        self.event_queue = event_queue;
        self.draw_sprocket = draw_sprocket;
        if let Some(work) = interrupt_return_work {
            self.finish_interrupt_return_work(work);
        }
        if trace_recent_on_halt && !matches!(result, PpcRunResult::CycleLimit { .. }) {
            let indirect = self.cpu.gpr[12];
            eprintln!(
                "[PPC-RECENT-IMPORTS] result={result:?} pc=${:08X} lr=${:08X} ctr=${:08X} r2=${:08X} r12=${:08X} indirect=({:?}, {:?})",
                self.cpu.pc,
                self.cpu.lr,
                self.cpu.ctr,
                self.cpu.gpr[2],
                indirect,
                self.memory.read_u32_be(indirect),
                self.memory.read_u32_be(indirect.wrapping_add(4)),
            );
            for entry in &recent_imports {
                eprintln!("{}", format_ppc_trace_import(entry));
            }
        }
        PpcHleRunProbe {
            result,
            handled_import_count,
            last_import_index,
            unsupported_import_index,
            import_trace,
            draw_sprocket_trace,
            input_sprocket_trace,
            fetch_histogram: trace_fetches.then_some(fetch_histogram),
        }
    }
}

pub(crate) fn ppc_virtual_microseconds(
    tick_count: u32,
    cycles_per_tick: u32,
    cycle_phase: u32,
    elapsed_cycles: u64,
) -> u64 {
    let cycles_per_tick = u64::from(cycles_per_tick.max(1));
    let elapsed_cycles = u64::from(cycle_phase).saturating_add(elapsed_cycles);
    u64::from(tick_count)
        .saturating_mul(PPC_MICROSECONDS_PER_TICK)
        .saturating_add(elapsed_cycles.saturating_mul(PPC_MICROSECONDS_PER_TICK) / cycles_per_tick)
}

pub(crate) fn ppc_virtual_tick_count(
    tick_count: u32,
    cycles_per_tick: u32,
    cycle_phase: u32,
    elapsed_cycles: u64,
) -> u32 {
    // Inside Macintosh: Processes (1993), p. 3-46: TickCount is the low-memory
    // time counter maintained by the vertical retrace interrupt. The native
    // runner keeps the persisted counter at the start of an execution slice,
    // so imports within that slice must include elapsed guest cycles. This is
    // especially important when an idle poll is accelerated with extra cycles:
    // inventing a caller-local future tick can make two Toolbox clock reads
    // disagree and send applications down their fatal startup path.
    let cycles_per_tick = u64::from(cycles_per_tick.max(1));
    let elapsed_cycles = u64::from(cycle_phase).saturating_add(elapsed_cycles);
    tick_count.wrapping_add((elapsed_cycles / cycles_per_tick) as u32)
}

pub(crate) fn ppc_cycles_until_next_tick(
    cycles_per_tick: u32,
    cycle_phase: u32,
    elapsed_cycles: u64,
) -> u64 {
    let cycles_per_tick = u64::from(cycles_per_tick.max(1));
    let cycle_phase = u64::from(cycle_phase).saturating_add(elapsed_cycles) % cycles_per_tick;
    cycles_per_tick - cycle_phase
}

pub(crate) fn dispatch_simple_hot_import_fast(
    target: &PpcImportDispatcherTarget,
    cpu: &mut PpcCpu,
    memory: &mut PpcSectionMem,
    microseconds: u64,
) -> Option<PpcImportAction> {
    match target {
        PpcImportDispatcherTarget::Microseconds => Some(dispatch_microseconds_import(
            cpu,
            memory,
            microseconds,
            None,
        )),
        PpcImportDispatcherTarget::AbsoluteToNanoseconds => {
            // PowerPC's struct-return ABI places the output pointer in r3 and
            // the 64-bit AbsoluteTime input in r4:r5. Our virtual absolute
            // clock counts microseconds, so conversion to nanoseconds is exact.
            let output = cpu.gpr[3];
            let absolute = (u64::from(cpu.gpr[4]) << 32) | u64::from(cpu.gpr[5]);
            if output != 0 && ppc_memory_can_write_bytes(memory, output, 8) {
                let _ = memory.write_u64_be(output, absolute.saturating_mul(1_000));
            }
            Some(PpcImportAction::ReturnPreserve)
        }
        _ => dispatch_math::dispatch_math_import(target, cpu, memory),
    }
}

pub(crate) fn ppc_random(memory: &mut PpcSectionMem) -> u16 {
    ppc_random_at(memory, PPC_RAND_SEED_ADDR)
}

pub(crate) fn ppc_random_at(memory: &mut PpcSectionMem, seed_addr: u32) -> u16 {
    let old_seed = memory.read_u32_be(seed_addr).unwrap_or(1);
    let seed = if old_seed == 0 { 1 } else { old_seed };
    let new_seed = ((u64::from(seed) * 16_807) % 2_147_483_647) as u32;
    let _ = memory.write_u32_be(seed_addr, new_seed);
    let result = new_seed as u16;
    if result == 0x8000 {
        0
    } else {
        result
    }
}

pub(crate) fn ppc_i16_result(value: i16) -> u32 {
    i32::from(value) as u32
}

pub(crate) fn ppc_run_result_cycles(result: PpcRunResult) -> u64 {
    match result {
        PpcRunResult::CycleLimit { cycles }
        | PpcRunResult::Halted { cycles, .. }
        | PpcRunResult::Unimplemented { cycles, .. }
        | PpcRunResult::MemoryFault { cycles, .. }
        | PpcRunResult::Exception { cycles, .. }
        | PpcRunResult::FetchFault { cycles, .. } => cycles,
    }
}

pub(crate) fn ppc_run_result_with_cycles(result: PpcRunResult, cycles: u64) -> PpcRunResult {
    match result {
        PpcRunResult::CycleLimit { .. } => PpcRunResult::CycleLimit { cycles },
        PpcRunResult::Halted { pc, .. } => PpcRunResult::Halted { pc, cycles },
        PpcRunResult::Unimplemented { pc, error, .. } => {
            PpcRunResult::Unimplemented { pc, error, cycles }
        }
        PpcRunResult::MemoryFault {
            pc,
            addr,
            was_write,
            ..
        } => PpcRunResult::MemoryFault {
            pc,
            addr,
            was_write,
            cycles,
        },
        PpcRunResult::Exception { pc, exception, .. } => PpcRunResult::Exception {
            pc,
            exception,
            cycles,
        },
        PpcRunResult::FetchFault { pc, .. } => PpcRunResult::FetchFault { pc, cycles },
    }
}
