//! PowerPC Trap Manager logical address translation, table queries, and live trap dispatch.

use std::collections::HashMap;

use super::*;

#[cfg(test)]
pub(crate) fn ppc_raw_trap_table_entry(trap_word: u16, toolbox: bool) -> u32 {
    let kind = if toolbox {
        TrapTableKind::Toolbox
    } else {
        TrapTableKind::OperatingSystem
    };
    TrapManager::table_address(trap_word, kind)
}

pub(crate) fn ppc_logical_trap_address(
    memory: &mut PpcSectionMem,
    trap_word: u16,
    toolbox: bool,
) -> Option<u32> {
    let kind = if toolbox {
        TrapTableKind::Toolbox
    } else {
        TrapTableKind::OperatingSystem
    };
    let protected_memory = memory.shared_view();
    TrapManager::get_address_with_provenance(
        trap_word,
        kind,
        |operation| match operation {
            TrapManagerMemoryOp::ReadLong(address) => {
                Some(TrapManagerMemoryResult::Long(memory.read_u32_be(address)?))
            }
            TrapManagerMemoryOp::WriteLong { .. }
            | TrapManagerMemoryOp::WriteProtectedLong { .. } => None,
        },
        move |address| protected_memory.is_shared_readonly_range(address, 4),
    )
}

/// Resolve a native import through the process's live Trap Manager entry
/// before any HLE shortcut runs. The default identity comes from the system
/// gateway registry, never from opcode inspection, so saved defaults remain
/// callable while direct table writes and SetTrapAddress patches take effect
/// immediately.
pub(crate) fn ppc_live_trap_import_action(
    target: &PpcImportDispatcherTarget,
    default_gateways: &HashMap<u16, u32>,
    cpu: &mut PpcCpu,
    process_memory_manager: &mut ProcessNativeMemoryManager,
    memory: &mut PpcSectionMem,
    toolbox_startup: &mut PpcToolboxStartupState,
) -> Result<Option<PpcImportAction>, ()> {
    let (trap_word, toolbox, proc_info, argument_count) = match target {
        // pascal Boolean StillDown(void)
        PpcImportDispatcherTarget::StillDown => (0xA973, true, 0x10, 0),
        // pascal Boolean Button(void)
        PpcImportDispatcherTarget::Button => (0xA974, true, 0x10, 0),
        // pascal LONGINT TickCount(void)
        PpcImportDispatcherTarget::TickCount => (0xA975, true, 0x30, 0),
        // pascal void GetKeys(KeyMap *)
        PpcImportDispatcherTarget::GetKeys => (0xA976, true, 0xC0, 1),
        // pascal Boolean WaitMouseUp(void)
        PpcImportDispatcherTarget::WaitMouseUp => (0xA977, true, 0x10, 0),
        _ => return Ok(None),
    };
    let kind = if toolbox {
        TrapTableKind::Toolbox
    } else {
        TrapTableKind::OperatingSystem
    };
    let table_entry = TrapManager::table_address(trap_word, kind);
    if memory.read_u32_be(table_entry).is_none() {
        // A detached PEF adapter has no process trap topology until the runner
        // attaches it. That standalone case retains the HLE import behavior.
        return Ok(None);
    }
    let handler = ppc_logical_trap_address(memory, trap_word, toolbox).ok_or(())?;
    let canonical_word = crate::trap::manager::raw_trap_route(trap_word).canonical_word;
    if default_gateways.get(&canonical_word) == Some(&handler) {
        return Ok(None);
    }
    if handler == 0 {
        return if default_gateways.is_empty() {
            // Standalone PEF construction currently exposes zero-filled low
            // memory without materializing system gateways (#1491).
            Ok(None)
        } else {
            Err(())
        };
    }

    let heap = process_memory_manager.native_heap_state().ok_or(())?;
    if argument_count > 6 {
        return Err(());
    }
    let mut saved_arguments = [0; 8];
    saved_arguments.copy_from_slice(&cpu.gpr[3..11]);
    for index in (0..argument_count).rev() {
        cpu.gpr[5 + index] = saved_arguments[index];
    }
    cpu.gpr[3] = handler;
    cpu.gpr[4] = proc_info;
    let mut heap_cursor = heap.heap_cursor;
    let heap_limit = process_memory_manager.native_allocation_limit(heap.heap_limit);
    let action = ppc_call_universal_proc(
        cpu,
        process_memory_manager,
        memory,
        &mut heap_cursor,
        heap_limit,
        toolbox_startup,
        GuestIsa::M68k,
    );
    if action.is_none() {
        cpu.gpr[3..11].copy_from_slice(&saved_arguments);
        return Err(());
    }
    Ok(action)
}

pub(crate) fn ppc_set_logical_trap_address(
    memory: &mut PpcSectionMem,
    trap_word: u16,
    toolbox: bool,
    handler: u32,
) -> bool {
    let kind = if toolbox {
        TrapTableKind::Toolbox
    } else {
        TrapTableKind::OperatingSystem
    };
    let protected_memory = memory.shared_view();
    let result = TrapManager::set_address_with_provenance(
        trap_word,
        kind,
        handler,
        |operation| match operation {
            TrapManagerMemoryOp::ReadLong(address) => {
                Some(TrapManagerMemoryResult::Long(memory.read_u32_be(address)?))
            }
            TrapManagerMemoryOp::WriteLong { address, value } => memory
                .write_u32_be(address, value)
                .map(|()| TrapManagerMemoryResult::Written),
            TrapManagerMemoryOp::WriteProtectedLong { address, value } => memory
                .write_shared_system_u32_be(address, value)
                .map(|()| TrapManagerMemoryResult::Written),
        },
        move |address| protected_memory.is_shared_readonly_range(address, 4),
    );
    if matches!(result, Err(TrapManagerSetError::InvalidComeFromHead)) {
        // NSetTrapAddress raises system error 12 for this malformed splice.
        // Inside Macintosh: Operating System Utilities (1994), p. 8-30.
        let _ = memory.write_u16_be(crate::memory::globals::addr::DS_ERR_CODE, 12);
    }
    result.is_ok()
}
