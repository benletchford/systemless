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
    pub(super) vfs_resources: &'a [PpcVfsResourceRecord],
    pub(super) current_resource_refnum: i16,
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
        vfs_resources,
        current_resource_refnum,
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
        PpcImportDispatcherTarget::ISpElementGetConfigurationInfo => {
            Some(PpcImportAction::Return(ppc_i16_result(
                ppc_isp_element_get_configuration_info(cpu, memory),
            )))
        }
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
            ppc_isp_init(input_sprocket, PpcInputSprocketKeyboardDefaults::default()),
        ))),
        PpcImportDispatcherTarget::ISpShutdown => Some(PpcImportAction::Return(ppc_i16_result(
            ppc_isp_stop(input_sprocket),
        ))),
        PpcImportDispatcherTarget::ISpInit => {
            // ISpInit(count, needs, virtuals, appCreator, subCreator,
            // flags, setListResourceId, reserved): argument 7 arrives in r9.
            let set_list_resource_id = cpu.gpr[9] as u16 as i16;
            let defaults = ppc_isp_load_default_keycodes(
                vfs_resources,
                current_resource_refnum,
                set_list_resource_id,
            )
            .unwrap_or_default();
            let result = ppc_isp_init(input_sprocket, defaults);
            // The game may have created its virtual elements before ISpInit.
            ppc_isp_assign_keyboard_defaults(input_sprocket, input_sprocket_virtual_elements);
            Some(PpcImportAction::Return(ppc_i16_result(result)))
        }
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
        PpcImportDispatcherTarget::InputSprocketCompatibility(operation) => {
            Some(ppc_dispatch_input_sprocket_compatibility(
                operation,
                cpu,
                memory,
                input_sprocket,
                input_sprocket_virtual_elements,
            ))
        }
        _ => None,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PpcInputSprocketCompatibilityOperation {
    DevicesActivateClass,
    DevicesDeactivateClass,
    ElementDisposeVirtual,
    ElementFlush,
    ElementGetNextEvent,
    Tickle,
}

pub(super) fn ppc_dispatch_input_sprocket_compatibility(
    operation: PpcInputSprocketCompatibilityOperation,
    cpu: &mut PpcCpu,
    memory: &mut PpcSectionMem,
    input_sprocket: &mut PpcInputSprocketState,
    virtual_elements: &mut Vec<PpcInputSprocketVirtualElementRecord>,
) -> PpcImportAction {
    match operation {
        PpcInputSprocketCompatibilityOperation::DevicesActivateClass => {
            input_sprocket.keyboard_active = true;
            input_sprocket.mouse_active = true;
            PpcImportAction::Return(ppc_i16_result(PPC_NO_ERR))
        }
        // InputSprocket.h: ISpDevices_DeactivateClass(ISpDeviceClass) stops
        // every device of that class, such as kISpDeviceClass_Mouse, so the
        // game reads it through the Event Manager instead. Classes with no
        // emulated device have nothing to deactivate.
        PpcInputSprocketCompatibilityOperation::DevicesDeactivateClass => {
            match cpu.gpr[3] {
                PPC_ISP_DEVICE_CLASS_KEYBOARD => input_sprocket.keyboard_active = false,
                PPC_ISP_DEVICE_CLASS_MOUSE => input_sprocket.mouse_active = false,
                _ => {}
            }
            PpcImportAction::Return(ppc_i16_result(PPC_NO_ERR))
        }
        PpcInputSprocketCompatibilityOperation::ElementDisposeVirtual => {
            virtual_elements.retain(|element| element.element != cpu.gpr[3]);
            input_sprocket.virtual_element_count =
                u32::try_from(virtual_elements.len()).unwrap_or(u32::MAX);
            PpcImportAction::Return(ppc_i16_result(PPC_NO_ERR))
        }
        PpcInputSprocketCompatibilityOperation::ElementGetNextEvent => {
            // Apple Game Sprockets Legacy Reference (2003), p. 62:
            // OSStatus ISpElement_GetNextEvent(ISpElementReference inElement,
            //     UInt32 bufSize, ISpElementEventPtr event, Boolean *wasEvent).
            // An idle element leaves the event buffer untouched and clears the
            // Boolean output, which is the fourth argument, not bufSize.
            let result = if cpu.gpr[6] != 0 && memory.write_u8(cpu.gpr[6], 0).is_some() {
                PPC_NO_ERR
            } else {
                PPC_PARAM_ERR
            };
            PpcImportAction::Return(ppc_i16_result(result))
        }
        PpcInputSprocketCompatibilityOperation::ElementFlush
        | PpcInputSprocketCompatibilityOperation::Tickle => {
            PpcImportAction::Return(ppc_i16_result(PPC_NO_ERR))
        }
    }
}
