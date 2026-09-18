//! Typed Thread Manager dispatch for PowerPC imports.

use super::*;
use crate::guest_call::{
    NativeRetirement, NativeThreadContext, SharedGuestCallStack, ThreadStorage,
};
use crate::guest_procedure::{resolve_same_isa_thread_entry, GuestIsa, GuestProcedure};
use crate::thread_manager::{NewThreadCreationEdge, ThreadManager};

pub(super) struct PpcThreadDispatchContext<'a> {
    pub(super) binding: &'a PpcImportBinding,
    pub(super) cpu: &'a mut PpcCpu,
    pub(super) memory: &'a mut PpcSectionMem,
    pub(super) process_memory_manager: &'a mut ProcessNativeMemoryManager,
    pub(super) heap_cursor: &'a mut u32,
    pub(super) heap_limit: u32,
    pub(super) last_mem_error: &'a mut i16,
    pub(super) toolbox_startup: &'a mut PpcToolboxStartupState,
}

struct PpcNewThreadEdge<'a> {
    memory: &'a mut PpcSectionMem,
    memory_manager: &'a mut ProcessNativeMemoryManager,
    heap_cursor: &'a mut u32,
    last_mem_error: &'a mut i16,
    msr: u32,
    entry_pointer: u32,
    default_rtoc: u32,
    parameter: u32,
    result_destination: u32,
    thread_made: u32,
    target: Option<GuestProcedure>,
}

impl NewThreadCreationEdge for PpcNewThreadEdge<'_> {
    fn preflight(&mut self, _size: u32) -> std::result::Result<(), i16> {
        if self.thread_made == 0 || !ppc_memory_can_write_bytes(self.memory, self.thread_made, 4) {
            return Err(PPC_PARAM_ERR);
        }
        self.target = resolve_same_isa_thread_entry(
            self.memory,
            self.entry_pointer,
            self.default_rtoc,
            GuestIsa::PowerPc,
        );
        if self.target.is_some() {
            Ok(())
        } else {
            Err(PPC_PARAM_ERR)
        }
    }

    fn allocate_fresh(&mut self, size: u32) -> std::result::Result<ThreadStorage, i16> {
        let stack = self.memory_manager.new_native_ptr(self.memory, size, true);
        ppc_apply_process_native_allocator(
            self.memory_manager,
            self.memory,
            self.heap_cursor,
            self.last_mem_error,
        );
        if stack == 0 {
            return Err(PPC_MEM_FULL_ERR);
        }
        let Some(stack_limit) = stack.checked_add(size) else {
            self.memory_manager.dispose_native_ptr(stack);
            ppc_apply_process_native_allocator(
                self.memory_manager,
                self.memory,
                self.heap_cursor,
                self.last_mem_error,
            );
            return Err(PPC_MEM_FULL_ERR);
        };
        Ok(ThreadStorage {
            result_destination: self.result_destination,
            stack_base: stack,
            stack_limit,
            managed_pointer: true,
        })
    }

    fn prepare_and_publish(
        &mut self,
        execution: &SharedGuestCallStack,
        mut storage: ThreadStorage,
        suspended: bool,
    ) -> std::result::Result<Option<crate::guest_call::ExecutionTaskId>, i16> {
        let target = self.target.expect("successful preflight resolves a target");
        storage.result_destination = self.result_destination;
        storage.managed_pointer = true;
        let Some(stack_pointer) =
            (storage.stack_limit & !15).checked_sub(PPC_INITIAL_STACK_FRAME_SIZE)
        else {
            return Err(PPC_MEM_FULL_ERR);
        };
        if stack_pointer < storage.stack_base {
            return Err(PPC_MEM_FULL_ERR);
        }
        let mut context = PpcExecutionContext::fresh();
        let state = context.architectural_mut();
        state.msr = self.msr;
        state.pc = target.entry;
        state.lr = PPC_THREAD_RETURN_PC;
        state.gpr[1] = stack_pointer;
        state.gpr[2] = target.rtoc;
        state.gpr[3] = self.parameter;
        Ok(execution.create_native_thread(
            NativeThreadContext { context },
            storage,
            suspended,
            |task| {
                self.memory
                    .write_u32_be(self.thread_made, task.thread_id())
                    .is_some()
            },
        ))
    }

    fn release_fresh(&mut self, storage: ThreadStorage) {
        self.memory_manager.dispose_native_ptr(storage.stack_base);
    }

    fn finish_publication_attempt(&mut self) {
        ppc_apply_process_native_allocator(
            self.memory_manager,
            self.memory,
            self.heap_cursor,
            self.last_mem_error,
        );
    }
}

pub(super) fn dispatch_thread_import(
    context: PpcThreadDispatchContext<'_>,
) -> Option<PpcImportAction> {
    let PpcThreadDispatchContext {
        binding,
        cpu,
        memory,
        process_memory_manager,
        heap_cursor,
        heap_limit,
        last_mem_error,
        toolbox_startup,
    } = context;

    match binding.dispatcher_target {
        PpcImportDispatcherTarget::GetCurrentThread => {
            // OSErr GetCurrentThread(ThreadID *currentThreadID);
            // Inside Macintosh: Thread Manager (1999), p. 62.
            let id = ThreadManager::new(toolbox_startup.execution.calls()).current_thread();
            let result = if cpu.gpr[3] != 0 && memory.write_u32_be(cpu.gpr[3], id).is_some() {
                PPC_NO_ERR
            } else {
                PPC_PARAM_ERR
            };
            Some(PpcImportAction::Return(ppc_i16_result(result)))
        }
        PpcImportDispatcherTarget::GetThreadState => {
            // OSErr GetThreadState(ThreadID thread, ThreadState *state);
            // Inside Macintosh: Thread Manager (1999), pp. 45, 63.
            let result = if cpu.gpr[4] == 0 {
                PPC_PARAM_ERR
            } else {
                match ThreadManager::new(toolbox_startup.execution.calls()).state(cpu.gpr[3]) {
                    Ok(state) if memory.write_u16_be(cpu.gpr[4], state).is_some() => PPC_NO_ERR,
                    Ok(_) => PPC_PARAM_ERR,
                    Err(error) => error,
                }
            };
            Some(PpcImportAction::Return(ppc_i16_result(result)))
        }
        PpcImportDispatcherTarget::GetThreadCurrentTaskRef => {
            // OSErr GetThreadCurrentTaskRef(ThreadTaskRef *reference);
            // Inside Macintosh: Thread Manager (1999), p. 73.
            let reference = ThreadManager::new(toolbox_startup.execution.calls()).task_reference();
            let result = if cpu.gpr[3] != 0 && memory.write_u32_be(cpu.gpr[3], reference).is_some()
            {
                0
            } else {
                PPC_PARAM_ERR
            };
            Some(PpcImportAction::Return(ppc_i16_result(result)))
        }
        PpcImportDispatcherTarget::GetThreadStateGivenTaskRef => {
            // OSErr GetThreadStateGivenTaskRef(ThreadTaskRef, ThreadID, ThreadState *);
            // Inside Macintosh: Thread Manager (1999), pp. 74–75.
            let result = if cpu.gpr[5] == 0 {
                PPC_PARAM_ERR
            } else {
                match ThreadManager::new(toolbox_startup.execution.calls())
                    .state_given_task(cpu.gpr[3], cpu.gpr[4])
                {
                    Ok(state) if memory.write_u16_be(cpu.gpr[5], state).is_some() => 0,
                    Ok(_) => PPC_PARAM_ERR,
                    Err(error) => error,
                }
            };
            Some(PpcImportAction::Return(ppc_i16_result(result)))
        }
        PpcImportDispatcherTarget::SetThreadReadyGivenTaskRef => {
            // OSErr SetThreadReadyGivenTaskRef(ThreadTaskRef reference, ThreadID thread);
            // Inside Macintosh: Thread Manager (1999), pp. 75–76.
            let result = ThreadManager::new(toolbox_startup.execution.calls())
                .ready_given_task(cpu.gpr[3], cpu.gpr[4]);
            Some(PpcImportAction::Return(ppc_i16_result(result)))
        }
        PpcImportDispatcherTarget::SetThreadState
        | PpcImportDispatcherTarget::SetThreadStateEndCritical => {
            // SetThreadState / SetThreadStateEndCritical
            // Set state and optionally exit a critical section atomically.
            // OSErr (ThreadID thread, ThreadState state, ThreadID suggested);
            // Inside Macintosh: Thread Manager (1999), pp. 67–72.
            let thread = cpu.gpr[3];
            let state = cpu.gpr[4] as u16;
            let suggested = cpu.gpr[5];
            let end_critical = matches!(
                binding.dispatcher_target,
                PpcImportDispatcherTarget::SetThreadStateEndCritical
            );
            Some(
                match toolbox_startup.execution.calls().set_native_thread_state(
                    cpu,
                    thread,
                    state,
                    suggested,
                    end_critical,
                ) {
                    Ok(true) => PpcImportAction::Yield(1),
                    Ok(false) => PpcImportAction::Return(0),
                    Err(error) => PpcImportAction::Return(ppc_i16_result(error)),
                },
            )
        }
        PpcImportDispatcherTarget::CreateThreadPool => {
            // OSErr CreateThreadPool(ThreadStyle, short, Size);
            // Thread Manager (1999), pp. 50–51: all allocations or none.
            let manager = ThreadManager::new(toolbox_startup.execution.calls());
            let result = manager.create_pool(
                GuestIsa::PowerPc,
                cpu.gpr[3],
                cpu.gpr[4] as i16,
                cpu.gpr[5],
                |size| {
                    let base = process_memory_manager.new_native_ptr(memory, size, true);
                    (base != 0).then_some(crate::guest_call::ThreadStorage {
                        stack_base: base,
                        stack_limit: base.saturating_add(size),
                        managed_pointer: true,
                        ..Default::default()
                    })
                },
            );
            let error = match result {
                Ok(()) => 0,
                Err((error, storage)) => {
                    for stack in storage {
                        process_memory_manager.dispose_native_ptr(stack.stack_base);
                    }
                    error
                }
            };
            ppc_apply_process_native_allocator(
                process_memory_manager,
                memory,
                heap_cursor,
                last_mem_error,
            );
            Some(PpcImportAction::Return(ppc_i16_result(error)))
        }
        PpcImportDispatcherTarget::GetFreeThreadCount
        | PpcImportDispatcherTarget::GetSpecificFreeThreadCount
        | PpcImportDispatcherTarget::GetDefaultThreadStackSize => {
            // OSErr GetFreeThreadCount(ThreadStyle, short *);
            // OSErr GetSpecificFreeThreadCount(ThreadStyle, Size, short *);
            // OSErr GetDefaultThreadStackSize(ThreadStyle, Size *);
            // Thread Manager (1999), pp. 52–55.
            let specific =
                binding.dispatcher_target == PpcImportDispatcherTarget::GetSpecificFreeThreadCount;
            let default_size =
                binding.dispatcher_target == PpcImportDispatcherTarget::GetDefaultThreadStackSize;
            let output = if specific { cpu.gpr[5] } else { cpu.gpr[4] };
            let manager = ThreadManager::new(toolbox_startup.execution.calls());
            let value = if default_size {
                ThreadManager::stack_size(GuestIsa::PowerPc, cpu.gpr[3], 0)
            } else {
                manager
                    .free_count(
                        GuestIsa::PowerPc,
                        cpu.gpr[3],
                        if specific { cpu.gpr[4] } else { 0 },
                    )
                    .map(u32::from)
            };
            let result = match value {
                Err(error) => error,
                Ok(value) if output != 0 => {
                    let written = if default_size {
                        memory.write_u32_be(output, value)
                    } else {
                        memory.write_u16_be(output, value as u16)
                    };
                    if written.is_some() {
                        0
                    } else {
                        PPC_PARAM_ERR
                    }
                }
                Ok(_) => PPC_PARAM_ERR,
            };
            Some(PpcImportAction::Return(ppc_i16_result(result)))
        }
        PpcImportDispatcherTarget::ThreadCurrentStackSpace => {
            // OSErr ThreadCurrentStackSpace(ThreadID, unsigned long *);
            // Thread Manager (1999), pp. 17–18 and 61.
            let output = cpu.gpr[4];
            let manager = ThreadManager::new(toolbox_startup.execution.calls());
            let result = match manager.stack_space(
                cpu.gpr[3],
                GuestIsa::PowerPc,
                cpu.gpr[1],
                |isa| match isa {
                    GuestIsa::M68k => memory
                        .read_u32_be(crate::memory::globals::addr::APPL_LIMIT)
                        .unwrap_or(0),
                    GuestIsa::PowerPc => process_memory_manager.application_heap_limit(heap_limit),
                },
            ) {
                Err(error) => error,
                Ok(value) if output != 0 && memory.write_u32_be(output, value).is_some() => 0,
                Ok(_) => PPC_PARAM_ERR,
            };
            Some(PpcImportAction::Return(ppc_i16_result(result)))
        }
        PpcImportDispatcherTarget::NewThread => {
            // OSErr NewThread(ThreadStyle, ThreadEntryUPP, void *, Size,
            //                 ThreadOptions, void **, ThreadID *);
            // Inside Macintosh: Thread Manager (1999), pp. 55–58.
            let style = cpu.gpr[3];
            let entry = cpu.gpr[4];
            let param = cpu.gpr[5];
            let size = cpu.gpr[6];
            let options = cpu.gpr[7];
            let result_destination = cpu.gpr[8];
            let made = cpu.gpr[9];
            let execution = toolbox_startup.execution.calls().shared_handle();
            let mut edge = PpcNewThreadEdge {
                memory,
                memory_manager: process_memory_manager,
                heap_cursor,
                last_mem_error,
                msr: cpu.msr,
                entry_pointer: entry,
                default_rtoc: cpu.gpr[2],
                parameter: param,
                result_destination,
                thread_made: made,
                target: None,
            };
            let result = ThreadManager::new(&execution)
                .create_thread(GuestIsa::PowerPc, style, size, options, &mut edge)
                .map_or_else(|error| error, |_| PPC_NO_ERR);
            if result != PPC_NO_ERR && made != 0 {
                let _ = edge.memory.write_u32_be(made, 0);
            }
            Some(PpcImportAction::Return(ppc_i16_result(result)))
        }
        PpcImportDispatcherTarget::YieldToThread | PpcImportDispatcherTarget::YieldToAnyThread => {
            // OSErr YieldToThread(ThreadID); OSErr YieldToAnyThread(void);
            // Inside Macintosh: Thread Manager (1999), pp. 64–66.
            let suggested = if binding.dispatcher_target == PpcImportDispatcherTarget::YieldToThread
            {
                cpu.gpr[3]
            } else {
                0
            };
            Some(
                match toolbox_startup
                    .execution
                    .calls()
                    .yield_native_thread(cpu, suggested)
                {
                    Ok(true) => PpcImportAction::Yield(1),
                    Ok(false) => PpcImportAction::Return(0),
                    Err(error) => PpcImportAction::Return(ppc_i16_result(error)),
                },
            )
        }
        PpcImportDispatcherTarget::DisposeThread => {
            // OSErr DisposeThread(ThreadID, void *, Boolean);
            // Inside Macintosh: Thread Manager (1999), pp. 59–60.
            let calls = toolbox_startup.execution.calls();
            let task = crate::guest_call::ExecutionTaskId::from_thread_id(
                ThreadManager::new(calls).resolve_thread(cpu.gpr[3]),
            );
            let result = cpu.gpr[4];
            let recycle = cpu.gpr[5] as u8 != 0;
            Some(
                match calls.retire_native_thread(task, cpu, recycle, |context| {
                    context.result_destination == 0
                        || memory
                            .write_u32_be(context.result_destination, result)
                            .is_some()
                }) {
                    Ok(retirement) => {
                        let switched = matches!(retirement, NativeRetirement::Switched(_));
                        ppc_release_retired_thread_storage(
                            process_memory_manager,
                            retirement,
                            recycle,
                        );
                        if switched {
                            PpcImportAction::Yield(1)
                        } else {
                            PpcImportAction::Return(0)
                        }
                    }
                    Err(error) => PpcImportAction::Return(ppc_i16_result(error)),
                },
            )
        }
        PpcImportDispatcherTarget::ThreadBeginCritical => Some(PpcImportAction::Return(
            ppc_i16_result(ThreadManager::new(toolbox_startup.execution.calls()).begin_critical()),
        )),
        PpcImportDispatcherTarget::ThreadEndCritical => Some(PpcImportAction::Return(
            ppc_i16_result(ThreadManager::new(toolbox_startup.execution.calls()).end_critical()),
        )),
        _ => None,
    }
}
