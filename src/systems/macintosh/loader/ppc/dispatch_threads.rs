//! Typed Thread Manager dispatch for PowerPC imports.

use super::*;
use crate::guest_call::{
    NativeRetirement, NativeThreadContext, SharedGuestCallStack, ThreadStorage,
};
use crate::guest_procedure::{resolve_guest_procedure, GuestIsa, GuestProcedure};
use crate::thread_manager::{NewThreadCreationEdge, ThreadManager};
use std::collections::HashMap;

const MP_INVALID_ID_ERR: i32 = -29299;
const MP_INSUFFICIENT_RESOURCES_ERR: i32 = -29298;
const MP_TIMEOUT_ERR: i32 = -29296;

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub(super) struct PpcMpSemaphoreState {
    next_id: u32,
    semaphores: HashMap<u32, (u32, u32)>, // maximum, current value
}

// C stack convention, void result, two 4-byte arguments.
// CarbonCore/MixedMode.h: kCStackBased | STACK_ROUTINE_PARAMETER(1, 3)
//   | STACK_ROUTINE_PARAMETER(2, 3).
const PPC_THREAD_CALLBACK_PROC_INFO: u32 = 0x03C1;
// C stack convention, 4-byte result, one 4-byte argument.
const PPC_THREAD_ENTRY_PROC_INFO: u32 = 0x00F1;

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
        self.target = resolve_guest_procedure(
            self.memory,
            self.entry_pointer,
            self.default_rtoc,
            None,
            GuestIsa::PowerPc,
            GuestIsa::PowerPc,
        )
        .filter(|target| target.isa == GuestIsa::PowerPc);
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
        PpcImportDispatcherTarget::MpCreateSemaphore => {
            // Multiprocessing Services Reference (2008), MPCreateSemaphore.
            let (maximum, initial, out) = (cpu.gpr[3], cpu.gpr[4], cpu.gpr[5]);
            let result = if maximum == 0
                || initial > maximum
                || out == 0
                || !ppc_memory_can_write_bytes(memory, out, 4)
            {
                i32::from(PPC_PARAM_ERR)
            } else {
                let state = &mut toolbox_startup.mp_semaphores;
                let id = state.next_id.wrapping_add(1).max(1);
                if state.semaphores.contains_key(&id) {
                    MP_INSUFFICIENT_RESOURCES_ERR
                } else {
                    state.next_id = id;
                    state.semaphores.insert(id, (maximum, initial));
                    let _ = memory.write_u32_be(out, id);
                    0
                }
            };
            Some(PpcImportAction::Return(result as u32))
        }
        PpcImportDispatcherTarget::MpDeleteSemaphore => {
            let result = if toolbox_startup
                .mp_semaphores
                .semaphores
                .remove(&cpu.gpr[3])
                .is_some()
            {
                0
            } else {
                MP_INVALID_ID_ERR
            };
            Some(PpcImportAction::Return(result as u32))
        }
        PpcImportDispatcherTarget::MpSignalSemaphore => {
            let result = match toolbox_startup.mp_semaphores.semaphores.get_mut(&cpu.gpr[3]) {
                Some((maximum, value)) if *value < *maximum => {
                    *value += 1;
                    0
                }
                Some(_) => MP_INSUFFICIENT_RESOURCES_ERR,
                None => MP_INVALID_ID_ERR,
            };
            Some(PpcImportAction::Return(result as u32))
        }
        PpcImportDispatcherTarget::MpWaitOnSemaphore => {
            // Apple advises kDurationImmediate from a cooperative task.
            // This guest context is cooperative, so never block the host thread.
            let result = match toolbox_startup.mp_semaphores.semaphores.get_mut(&cpu.gpr[3]) {
                Some((_, value)) if *value > 0 => {
                    *value -= 1;
                    0
                }
                Some(_) if cpu.gpr[4] == 0 => MP_TIMEOUT_ERR,
                Some(_) => MP_INSUFFICIENT_RESOURCES_ERR,
                None => MP_INVALID_ID_ERR,
            };
            Some(PpcImportAction::Return(result as u32))
        }
        PpcImportDispatcherTarget::NewThreadEntryUPP
        | PpcImportDispatcherTarget::NewThreadTerminationUPP
        | PpcImportDispatcherTarget::NewThreadSwitchUPP => {
            // CarbonCore/Threads.h declares the entry callback as a function
            // with one pointer argument and a pointer result; termination and
            // switch callbacks take a ThreadID and context with no result.
            // Carbon CFM creates a Mixed Mode routine descriptor for each UPP.
            // Carbon Porting Guide (2002), p. 22; Threads.h, MixedMode.h.
            let proc_info = if matches!(
                binding.dispatcher_target,
                PpcImportDispatcherTarget::NewThreadEntryUPP
            ) {
                PPC_THREAD_ENTRY_PROC_INFO
            } else {
                PPC_THREAD_CALLBACK_PROC_INFO
            };
            let descriptor = dispatch_mixed_mode::ppc_new_routine_descriptor(
                cpu.gpr[3],
                proc_info,
                PPC_ROUTINE_RECORD_POWERPC_ISA,
                process_memory_manager,
                memory,
                heap_cursor,
                last_mem_error,
                &mut toolbox_startup.system_allocations,
            );
            Some(PpcImportAction::Return(descriptor))
        }
        PpcImportDispatcherTarget::DisposeThreadEntryUPP
        | PpcImportDispatcherTarget::DisposeThreadTerminationUPP
        | PpcImportDispatcherTarget::DisposeThreadSwitchUPP => {
            // CarbonCore/Threads.h: dispose the UPP returned by its matching
            // constructor. A null UPP requires no release.
            let descriptor = cpu.gpr[3];
            if descriptor != 0 && !toolbox_startup.system_allocations.release(descriptor) {
                let _ = process_memory_manager.dispose_native_ptr(descriptor);
            }
            ppc_apply_process_native_allocator(
                process_memory_manager,
                memory,
                heap_cursor,
                last_mem_error,
            );
            Some(PpcImportAction::Return(0))
        }
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
        PpcImportDispatcherTarget::SetThreadTerminator => {
            // OSErr SetThreadTerminator(ThreadID thread,
            //     ThreadTerminationProcPtr threadTerminator, void *terminationProcParam);
            // Inside Macintosh: Thread Manager (1999), pp. 81–82.
            let calls = toolbox_startup.execution.calls();
            let thread = ThreadManager::new(calls).resolve_thread(cpu.gpr[3]);
            let task = crate::guest_call::ExecutionTaskId::from_thread_id(thread);
            let result = calls
                .set_thread_terminator(task, cpu.gpr[4], cpu.gpr[5])
                .map_or_else(|error| error, |()| PPC_NO_ERR);
            Some(PpcImportAction::Return(ppc_i16_result(result)))
        }
        PpcImportDispatcherTarget::SetThreadSwitcher => {
            // OSErr SetThreadSwitcher(ThreadID thread,
            //     ThreadSwitchProcPtr threadSwitcher, void *switchProcParam,
            //     Boolean inOrOut);
            // Inside Macintosh: Thread Manager (1999), pp. 79–81.
            let calls = toolbox_startup.execution.calls();
            let thread = ThreadManager::new(calls).resolve_thread(cpu.gpr[3]);
            let task = crate::guest_call::ExecutionTaskId::from_thread_id(thread);
            let result = calls
                .set_thread_switcher(task, cpu.gpr[4], cpu.gpr[5], cpu.gpr[6] as u8 != 0)
                .map_or_else(|error| error, |()| PPC_NO_ERR);
            if std::env::var_os("SYSTEMLESS_PPC_THREAD_TRACE").is_some() {
                eprintln!("[PPC-THREAD-TRACE] SetThreadSwitcher thread={} procedure=${:08X} parameter=${:08X} in={} result={}", thread, cpu.gpr[4], cpu.gpr[5], cpu.gpr[6] as u8 != 0, result);
            }
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
            let action = match toolbox_startup.execution.calls().set_native_thread_state(
                    cpu,
                    thread,
                    state,
                    suggested,
                    end_critical,
                ) {
                    Ok(true) => PpcImportAction::Yield(1),
                    Ok(false) => PpcImportAction::Return(0),
                    Err(error) => PpcImportAction::Return(ppc_i16_result(error)),
                };
            if std::env::var_os("SYSTEMLESS_PPC_THREAD_TRACE").is_some() {
                eprintln!("[PPC-THREAD-TRACE] SetThreadState caller={} thread={} state={} suggested={} action={:?} pc=${:08X}", toolbox_startup.execution.calls().current_task().thread_id(), thread, state, suggested, action, cpu.pc);
            }
            Some(action)
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
            if std::env::var_os("SYSTEMLESS_PPC_THREAD_TRACE").is_some() {
                eprintln!("[PPC-THREAD-TRACE] NewThread caller={} entry=${entry:08X} param=${param:08X} size={} options=${options:08X} made={:?} result={}", execution.current_task().thread_id(), size, edge.memory.read_u32_be(made), result);
            }
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
            Some(action)
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
