//! Typed InputSprocket dispatch for PowerPC imports.

use super::*;

pub(super) struct PpcInputSprocketDispatchContext<'a> {
    pub(super) binding: &'a PpcImportBinding,
    pub(super) cpu: &'a mut PpcCpu,
    pub(super) memory: &'a mut PpcSectionMem,
    pub(super) process_memory_manager: &'a mut ProcessNativeMemoryManager,
    pub(super) heap_cursor: &'a mut u32,
    pub(super) heap_limit: u32,
    pub(super) input_sprocket: &'a mut PpcInputSprocketState,
    pub(super) input_sprocket_virtual_elements: &'a mut Vec<PpcInputSprocketVirtualElementRecord>,
    pub(super) input: PpcInputSnapshot,
    pub(super) tick_count: u32,
}

pub(super) fn dispatch_inputsprocket_import(
    context: PpcInputSprocketDispatchContext<'_>,
) -> Option<PpcImportAction> {
    let PpcInputSprocketDispatchContext {
        binding,
        cpu,
        memory,
        process_memory_manager,
        heap_cursor,
        heap_limit,
        input_sprocket,
        input_sprocket_virtual_elements,
        input,
        tick_count,
    } = context;

    match binding.dispatcher_target {
        PpcImportDispatcherTarget::ISpElementNewVirtualFromNeeds => Some(PpcImportAction::Return(
            ppc_i16_result(ppc_isp_element_new_virtual_from_needs(
                cpu,
                process_memory_manager,
                memory,
                heap_cursor,
                heap_limit,
                input_sprocket,
                input_sprocket_virtual_elements,
            )),
        )),
        PpcImportDispatcherTarget::ISpElementListNew => Some(PpcImportAction::Return(
            ppc_i16_result(ppc_isp_element_list_new(
                cpu,
                process_memory_manager,
                memory,
                heap_cursor,
                input_sprocket_virtual_elements,
            )),
        )),
        PpcImportDispatcherTarget::ISpElementListAddElements => {
            Some(PpcImportAction::Return(ppc_i16_result(
                ppc_isp_element_list_add_elements(cpu, memory, input_sprocket_virtual_elements),
            )))
        }
        PpcImportDispatcherTarget::ISpElementListGetNextEvent => Some(PpcImportAction::Return(
            ppc_i16_result(ppc_isp_element_list_get_next_event(
                cpu,
                memory,
                input,
                *input_sprocket,
                input_sprocket_virtual_elements,
                tick_count,
            )),
        )),
        PpcImportDispatcherTarget::ISpElementListFlush => Some(PpcImportAction::Return(
            ppc_i16_result(ppc_isp_element_list_flush(
                cpu,
                memory,
                input,
                *input_sprocket,
                input_sprocket_virtual_elements,
            )),
        )),
        PpcImportDispatcherTarget::ISpDevicesExtract => Some(PpcImportAction::Return(
            ppc_i16_result(ppc_isp_devices_extract(cpu, memory)),
        )),
        PpcImportDispatcherTarget::ISpDevicesExtractByClass => Some(PpcImportAction::Return(
            ppc_i16_result(ppc_isp_devices_extract_by_class(cpu, memory)),
        )),
        PpcImportDispatcherTarget::ISpDeviceGetDefinition => Some(PpcImportAction::Return(
            ppc_i16_result(ppc_isp_device_get_definition(cpu, memory)),
        )),
        PpcImportDispatcherTarget::ISpDeviceGetElementList => Some(PpcImportAction::Return(
            ppc_i16_result(ppc_isp_device_get_element_list(cpu, memory)),
        )),
        PpcImportDispatcherTarget::ISpElementListExtract => Some(PpcImportAction::Return(
            ppc_i16_result(ppc_isp_element_list_extract(cpu, memory)),
        )),
        PpcImportDispatcherTarget::ISpElementGetInfo => Some(PpcImportAction::Return(
            ppc_i16_result(ppc_isp_element_get_info(cpu, memory)),
        )),
        PpcImportDispatcherTarget::ISpElementGetSimpleState => Some(PpcImportAction::Return(
            ppc_i16_result(ppc_isp_element_get_simple_state(
                cpu,
                memory,
                input,
                *input_sprocket,
                input_sprocket_virtual_elements,
            )),
        )),
        // Apple InputSprocket.h 1.7 (QuickTime 6.0.2 SDK) declares
        // ISpGetVersion as returning the four-byte NumVersion structure.
        // The CFM PowerPC structure-result pointer is passed in r3, matching
        // SndSoundManagerVersion above. NumVersion 1.7 final is 01 70 80 00.
        PpcImportDispatcherTarget::ISpGetVersion => {
            if cpu.gpr[3] != 0 && ppc_memory_can_write_bytes(memory, cpu.gpr[3], 4) {
                let _ = memory.write_u32_be(cpu.gpr[3], 0x0170_8000);
            }
            Some(PpcImportAction::ReturnPreserve)
        }
        PpcImportDispatcherTarget::ISpStartup => Some(PpcImportAction::Return(ppc_i16_result(
            ppc_isp_init(input_sprocket),
        ))),
        PpcImportDispatcherTarget::ISpShutdown => Some(PpcImportAction::Return(ppc_i16_result(
            ppc_isp_stop(input_sprocket),
        ))),
        PpcImportDispatcherTarget::ISpInit => Some(PpcImportAction::Return(ppc_i16_result(
            ppc_isp_init(input_sprocket),
        ))),
        PpcImportDispatcherTarget::ISpStop => Some(PpcImportAction::Return(ppc_i16_result(
            ppc_isp_stop(input_sprocket),
        ))),
        PpcImportDispatcherTarget::ISpSuspend => Some(PpcImportAction::Return(ppc_i16_result(
            ppc_isp_suspend(input_sprocket),
        ))),
        PpcImportDispatcherTarget::ISpResume => Some(PpcImportAction::Return(ppc_i16_result(
            ppc_isp_resume(input_sprocket),
        ))),
        PpcImportDispatcherTarget::ISpDevicesActivate => Some(PpcImportAction::Return(
            ppc_i16_result(ppc_isp_devices_activate(cpu, memory, input_sprocket)),
        )),
        PpcImportDispatcherTarget::ISpDevicesDeactivate => Some(PpcImportAction::Return(
            ppc_i16_result(ppc_isp_devices_deactivate(cpu, memory, input_sprocket)),
        )),
        PpcImportDispatcherTarget::ISpConfigure => Some(PpcImportAction::Return(ppc_i16_result(
            ppc_isp_configure(input_sprocket),
        ))),
        _ => None,
    }
}
