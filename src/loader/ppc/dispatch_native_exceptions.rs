//! PowerPC native exception dispatch and frame transitions.

use super::*;

pub(super) struct PpcNativeExceptionDispatchContext<'a> {
    pub(super) binding: &'a PpcImportBinding,
    pub(super) cpu: &'a mut PpcCpu,
    pub(super) native_exception_handler: &'a Cell<u32>,
}

pub(super) fn dispatch_native_exception_import(
    context: PpcNativeExceptionDispatchContext<'_>,
) -> Option<PpcImportAction> {
    let PpcNativeExceptionDispatchContext {
        binding,
        cpu,
        native_exception_handler,
    } = context;

    match binding.dispatcher_target {
        PpcImportDispatcherTarget::InstallExceptionHandler => {
            // Replaces the native exception handler for the current application
            // context and returns the previous transition-vector pointer.
            // ExceptionHandlerTPP InstallExceptionHandler(ExceptionHandlerTPP theHandler);
            // Inside Macintosh: PowerPC System Software (1994), pp. 4-6, 4-17
            let previous = native_exception_handler.replace(cpu.gpr[3]);
            Some(PpcImportAction::Return(previous))
        }
        _ => None,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum PpcNativeExceptionCause {
    Processor(PpcException),
    UnmappedMemory { address: u32, was_write: bool },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct PpcNativeExceptionContext {
    cause: PpcNativeExceptionCause,
    pc: u32,
    information: u32,
    machine_state: u32,
    register_image: u32,
    fpu_image: u32,
    vector_image: u32,
    memory_information: Option<u32>,
}

pub(super) fn ppc_native_exception_kind(exception: PpcException) -> Option<u32> {
    match exception {
        PpcException::IllegalInstruction { .. } => Some(PPC_ILLEGAL_INSTRUCTION_EXCEPTION),
        PpcException::ProgramTrap { .. } => Some(PPC_TRAP_EXCEPTION),
        _ => None,
    }
}

fn ppc_native_exception_cause_kind(cause: PpcNativeExceptionCause) -> Option<u32> {
    match cause {
        PpcNativeExceptionCause::Processor(exception) => ppc_native_exception_kind(exception),
        PpcNativeExceptionCause::UnmappedMemory { .. } => Some(PPC_UNMAPPED_MEMORY_EXCEPTION),
    }
}

pub(super) fn ppc_native_exception_result(
    context: PpcNativeExceptionContext,
    cycles: u64,
) -> PpcRunResult {
    match context.cause {
        PpcNativeExceptionCause::Processor(exception) => PpcRunResult::Exception {
            pc: context.pc,
            exception,
            cycles,
        },
        PpcNativeExceptionCause::UnmappedMemory { address, was_write } => {
            PpcRunResult::MemoryFault {
                pc: context.pc,
                addr: address,
                was_write,
                cycles,
            }
        }
    }
}

fn ppc_write_exception_wide(memory: &mut PpcSectionMem, addr: u32, value: u32) -> Option<()> {
    memory.write_u32_be(addr, 0)?;
    memory.write_u32_be(addr.checked_add(4)?, value)?;
    Some(())
}

fn ppc_read_exception_wide(memory: &mut PpcSectionMem, addr: u32) -> Option<u32> {
    memory.read_u32_be(addr.checked_add(4)?)
}

pub(super) fn ppc_begin_native_exception(
    cpu: &mut PpcCpu,
    memory: &mut PpcSectionMem,
    stack_base: u32,
    handler: u32,
    pc: u32,
    cause: PpcNativeExceptionCause,
) -> Option<PpcNativeExceptionContext> {
    let kind = ppc_native_exception_cause_kind(cause)?;
    // InstallExceptionHandler accepts an ExceptionHandlerTPP: a native
    // transition vector containing the entry point and TOC, not a UPP.
    // Inside Macintosh: PowerPC System Software (1994), p. 4-17
    let entry = memory.read_u32_be(handler)?;
    let rtoc = memory.read_u32_be(handler.checked_add(4)?)?;
    if entry == 0 {
        return None;
    }

    let records_size = PPC_EXCEPTION_INFORMATION_SIZE
        .checked_add(PPC_EXCEPTION_MACHINE_INFORMATION_SIZE)?
        .checked_add(PPC_EXCEPTION_REGISTER_INFORMATION_SIZE)?
        .checked_add(PPC_EXCEPTION_FPU_INFORMATION_SIZE)?
        .checked_add(PPC_EXCEPTION_VECTOR_INFORMATION_SIZE)?
        .checked_add(match cause {
            PpcNativeExceptionCause::UnmappedMemory { .. } => PPC_EXCEPTION_MEMORY_INFORMATION_SIZE,
            PpcNativeExceptionCause::Processor(_) => 0,
        })?;
    let frame_and_records = PPC_INITIAL_STACK_FRAME_SIZE.checked_add(records_size)?;
    let callback_sp = cpu.gpr[1]
        .checked_sub(PPC_INTERRUPT_RED_ZONE_SIZE)?
        .checked_sub(frame_and_records)?
        & !0xFu32;
    if callback_sp < stack_base
        || !ppc_memory_can_write_bytes(memory, callback_sp, frame_and_records)
    {
        return None;
    }

    let information = callback_sp.checked_add(PPC_INITIAL_STACK_FRAME_SIZE)?;
    let machine_state = information.checked_add(PPC_EXCEPTION_INFORMATION_SIZE)?;
    let register_image = machine_state.checked_add(PPC_EXCEPTION_MACHINE_INFORMATION_SIZE)?;
    let fpu_image = register_image.checked_add(PPC_EXCEPTION_REGISTER_INFORMATION_SIZE)?;
    let vector_image = fpu_image.checked_add(PPC_EXCEPTION_FPU_INFORMATION_SIZE)?;
    let memory_information = match cause {
        PpcNativeExceptionCause::UnmappedMemory { .. } => {
            Some(vector_image.checked_add(PPC_EXCEPTION_VECTOR_INFORMATION_SIZE)?)
        }
        PpcNativeExceptionCause::Processor(_) => None,
    };
    if !ppc_zero_guest_bytes(memory, callback_sp, frame_and_records) {
        return None;
    }

    // Universal Interfaces 3.4, MachineExceptions.h defines these PowerPC
    // records with 680x0 field alignment. The handler may edit any saved
    // register image; a noErr return restores those edited values.
    memory.write_u32_be(information, kind)?;
    memory.write_u32_be(information + 4, machine_state)?;
    memory.write_u32_be(information + 8, register_image)?;
    memory.write_u32_be(information + 12, fpu_image)?;
    memory.write_u32_be(information + 16, memory_information.unwrap_or(0))?;
    memory.write_u32_be(information + 20, vector_image)?;

    if let (
        PpcNativeExceptionCause::UnmappedMemory { address, was_write },
        Some(memory_information),
    ) = (cause, memory_information)
    {
        // MemoryExceptionInformation describes the affected area, logical
        // address, status, and reference kind. This HLE has one application
        // address space, identified by its nonzero stack-area base.
        // Inside Macintosh: PowerPC System Software (1994), pp. 4-11, 4-15
        memory.write_u32_be(memory_information, stack_base)?;
        memory.write_u32_be(memory_information + 4, address)?;
        memory.write_u32_be(memory_information + 8, PPC_UNMAPPED_MEMORY_ERROR)?;
        memory.write_u32_be(
            memory_information + 12,
            if was_write {
                PPC_WRITE_REFERENCE
            } else {
                PPC_READ_REFERENCE
            },
        )?;
    }

    ppc_write_exception_wide(memory, machine_state, cpu.ctr)?;
    ppc_write_exception_wide(memory, machine_state + 8, cpu.lr)?;
    ppc_write_exception_wide(memory, machine_state + 16, pc)?;
    memory.write_u32_be(machine_state + 24, cpu.cr)?;
    memory.write_u32_be(machine_state + 28, cpu.xer)?;
    memory.write_u32_be(machine_state + 32, cpu.msr)?;
    memory.write_u32_be(machine_state + 40, kind)?;
    for (index, value) in cpu.gpr.iter().copied().enumerate() {
        ppc_write_exception_wide(
            memory,
            register_image.checked_add(u32::try_from(index).ok()?.checked_mul(8)?)?,
            value,
        )?;
    }
    for (index, value) in cpu.fpr.iter().copied().enumerate() {
        memory.write_u64_be(
            fpu_image.checked_add(u32::try_from(index).ok()?.checked_mul(8)?)?,
            value,
        )?;
    }
    memory.write_u32_be(fpu_image + 256, cpu.fpscr)?;

    memory.write_u32_be(callback_sp + PPC_LINKAGE_BACK_CHAIN_OFFSET, cpu.gpr[1])?;
    memory.write_u32_be(callback_sp + PPC_LINKAGE_SAVED_CR_OFFSET, cpu.cr)?;
    memory.write_u32_be(callback_sp + PPC_LINKAGE_SAVED_LR_OFFSET, cpu.lr)?;
    memory.write_u32_be(callback_sp + PPC_LINKAGE_SAVED_RTOC_OFFSET, cpu.gpr[2])?;

    cpu.pc = entry;
    cpu.lr = PPC_HALT_PC;
    cpu.gpr[1] = callback_sp;
    cpu.gpr[2] = rtoc;
    install_powerpc_call_arguments(cpu, memory, &[information])?;

    Some(PpcNativeExceptionContext {
        cause,
        pc,
        information,
        machine_state,
        register_image,
        fpu_image,
        vector_image,
        memory_information,
    })
}

pub(super) fn ppc_restore_native_exception(
    cpu: &mut PpcCpu,
    memory: &mut PpcSectionMem,
    context: PpcNativeExceptionContext,
) -> Option<()> {
    let frame_is_valid = memory.read_u32_be(context.information + 4)? == context.machine_state
        && memory.read_u32_be(context.information + 8)? == context.register_image
        && memory.read_u32_be(context.information + 12)? == context.fpu_image
        && memory.read_u32_be(context.information + 16)? == context.memory_information.unwrap_or(0)
        && memory.read_u32_be(context.information + 20)? == context.vector_image;
    cpu.ctr = ppc_read_exception_wide(memory, context.machine_state)?;
    cpu.lr = ppc_read_exception_wide(memory, context.machine_state + 8)?;
    cpu.pc = ppc_read_exception_wide(memory, context.machine_state + 16)?;
    cpu.cr = memory.read_u32_be(context.machine_state + 24)?;
    cpu.xer = memory.read_u32_be(context.machine_state + 28)?;
    cpu.msr = memory.read_u32_be(context.machine_state + 32)?;
    for index in 0..cpu.gpr.len() {
        cpu.gpr[index] = ppc_read_exception_wide(
            memory,
            context
                .register_image
                .checked_add(u32::try_from(index).ok()?.checked_mul(8)?)?,
        )?;
    }
    for index in 0..cpu.fpr.len() {
        cpu.fpr[index] = memory.read_u64_be(
            context
                .fpu_image
                .checked_add(u32::try_from(index).ok()?.checked_mul(8)?)?,
        )?;
    }
    cpu.fpscr = memory.read_u32_be(context.fpu_image + 256)?;
    frame_is_valid.then_some(())
}
