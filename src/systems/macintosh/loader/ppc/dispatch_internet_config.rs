//! Process-owned Internet Config instances for Carbon applications.

use super::*;
use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct PpcInternetConfigState {
    instances: BTreeMap<u32, u32>,
    // InternetConfig.h: IC 2.5 automatically selects the default configuration.
    // The empty default preferences set starts at an opaque, stable seed.
    pub(super) preferences_seed: u32,
}

impl Default for PpcInternetConfigState {
    fn default() -> Self {
        Self {
            instances: BTreeMap::new(),
            preferences_seed: 1,
        }
    }
}

pub(super) fn dispatch_internet_config_import(
    binding: &PpcImportBinding,
    cpu: &PpcCpu,
    memory: &mut PpcSectionMem,
    process_memory_manager: &mut ProcessNativeMemoryManager,
    heap_cursor: &mut u32,
    last_mem_error: &mut i16,
    toolbox_startup: &mut PpcToolboxStartupState,
) -> Option<PpcImportAction> {
    let state = &mut toolbox_startup.internet_config;
    // Apple InternetConfig.h: ICInstance is an opaque, process-local pointer.
    // ICStart(ICInstance *, OSType), ICStop(ICInstance), and
    // ICGetSeed(ICInstance, long *) return a 32-bit OSStatus in r3.
    let result: i32 = match binding.dispatcher_target {
        PpcImportDispatcherTarget::ICStart => {
            let output = cpu.gpr[3];
            if output == 0 || memory.read_u32_be(output).is_none() {
                i32::from(PPC_PARAM_ERR)
            } else {
                memory.write_u32_be(output, 0)?;
                let instance = process_memory_manager.new_native_ptr(memory, 4, true);
                ppc_apply_process_native_allocator(
                    process_memory_manager,
                    memory,
                    heap_cursor,
                    last_mem_error,
                );
                if instance == 0 {
                    i32::from(PPC_MEM_FULL_ERR)
                } else {
                    let signature = cpu.gpr[4];
                    if memory.write_u32_be(instance, signature).is_some()
                        && memory.write_u32_be(output, instance).is_some()
                    {
                        state.instances.insert(instance, signature);
                        0
                    } else {
                        let _ = process_memory_manager.dispose_native_ptr(instance);
                        ppc_apply_process_native_allocator(
                            process_memory_manager,
                            memory,
                            heap_cursor,
                            last_mem_error,
                        );
                        i32::from(PPC_PARAM_ERR)
                    }
                }
            }
        }
        PpcImportDispatcherTarget::ICStop => {
            let instance = cpu.gpr[3];
            if state.instances.remove(&instance).is_none() {
                i32::from(PPC_PARAM_ERR)
            } else {
                let result = process_memory_manager.dispose_native_ptr(instance);
                ppc_apply_process_native_allocator(
                    process_memory_manager,
                    memory,
                    heap_cursor,
                    last_mem_error,
                );
                if result.is_some() {
                    0
                } else {
                    i32::from(PPC_PARAM_ERR)
                }
            }
        }
        PpcImportDispatcherTarget::ICGetSeed => {
            let instance = cpu.gpr[3];
            let output = cpu.gpr[4];
            if !state.instances.contains_key(&instance)
                || output == 0
                || memory.read_u32_be(output).is_none()
            {
                i32::from(PPC_PARAM_ERR)
            } else {
                // This is a database seed, shared by all live instances, not
                // an instance identifier. It remains stable until preferences change.
                memory.write_u32_be(output, state.preferences_seed)?;
                0
            }
        }
        _ => return None,
    };
    Some(PpcImportAction::Return(result as u32))
}
