//! Driver-owned InputSprocket devices and elements.
//!
//! Layouts and signatures follow Universal Interfaces 3.4.1 InputSprocket.h.

use super::*;
use std::collections::{BTreeMap, VecDeque};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PpcInputSprocketDriverOperation {
    DeviceNew,
    DeviceDispose,
    ElementNew,
    ElementDispose,
    ElementGetComplexState,
    ElementPushSimpleData,
    ElementPushComplexData,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct DriverDevice {
    definition: Vec<u8>,
    meta_handler: u32,
    ref_con: u32,
    active: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct DriverElement {
    device: u32,
    definition: Vec<u8>,
    configuration: Vec<u8>,
    state: Vec<u8>,
    events: VecDeque<DriverEvent>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct DriverEvent {
    when: u64,
    data: Vec<u8>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(super) struct PpcInputSprocketDriverState {
    devices: BTreeMap<u32, DriverDevice>,
    elements: BTreeMap<u32, DriverElement>,
}

fn read_bytes(memory: &mut PpcSectionMem, address: u32, size: u32) -> Option<Vec<u8>> {
    if size == 0 {
        return Some(Vec::new());
    }
    if address == 0 || !ppc_memory_can_read_bytes(memory, address, size) {
        return None;
    }
    (0..size)
        .map(|offset| memory.read_u8(address + offset))
        .collect()
}

pub(super) fn dispatch_driver_import(
    binding: &PpcImportBinding,
    cpu: &PpcCpu,
    memory: &mut PpcSectionMem,
    manager: &mut ProcessNativeMemoryManager,
    heap_cursor: &mut u32,
    state: &mut PpcInputSprocketDriverState,
) -> Option<PpcImportAction> {
    // Existing application APIs share the same driver-owned element state.
    match binding.dispatcher_target {
        PpcImportDispatcherTarget::ISpDevicesExtract
        | PpcImportDispatcherTarget::ISpDevicesExtractByClass
            if !state.devices.is_empty() =>
        {
            let filtered = matches!(
                binding.dispatcher_target,
                PpcImportDispatcherTarget::ISpDevicesExtractByClass
            );
            let class = filtered.then_some(cpu.gpr[3]);
            let mut devices = Vec::new();
            for (reference, device_class) in [
                (PPC_ISP_KEYBOARD_DEVICE, PPC_ISP_DEVICE_CLASS_KEYBOARD),
                (PPC_ISP_MOUSE_DEVICE, PPC_ISP_DEVICE_CLASS_MOUSE),
            ] {
                if class.is_none_or(|class| class == device_class) {
                    devices.push(reference);
                }
            }
            devices.extend(state.devices.iter().filter_map(|(reference, record)| {
                let device_class =
                    u32::from_be_bytes(record.definition[64..68].try_into().unwrap());
                class
                    .is_none_or(|class| class == device_class)
                    .then_some(*reference)
            }));
            let offset = usize::from(filtered);
            let result = write_references(
                memory,
                &devices,
                cpu.gpr[3 + offset],
                cpu.gpr[4 + offset],
                cpu.gpr[5 + offset],
            );
            return Some(PpcImportAction::Return(ppc_i16_result(result)));
        }
        PpcImportDispatcherTarget::ISpDeviceGetDefinition
            if state.devices.contains_key(&cpu.gpr[3]) =>
        {
            let device = &state.devices[&cpu.gpr[3]];
            let result = copy_output(memory, cpu.gpr[5], cpu.gpr[4], &device.definition, true);
            return Some(PpcImportAction::Return(ppc_i16_result(result)));
        }
        PpcImportDispatcherTarget::ISpDeviceGetElementList
            if state.devices.contains_key(&cpu.gpr[3]) =>
        {
            let output = cpu.gpr[4];
            let result = if output == 0 || !ppc_memory_can_write_bytes(memory, output, 4) {
                PPC_PARAM_ERR
            } else {
                // Device lists are borrowed identities and follow live membership.
                let _ = memory.write_u32_be(output, cpu.gpr[3]);
                PPC_NO_ERR
            };
            return Some(PpcImportAction::Return(ppc_i16_result(result)));
        }
        PpcImportDispatcherTarget::ISpElementListExtract
            if state.devices.contains_key(&cpu.gpr[3])
                || (cpu.gpr[3] == PPC_ISP_GLOBAL_ELEMENT_LIST && !state.elements.is_empty()) =>
        {
            let global = cpu.gpr[3] == PPC_ISP_GLOBAL_ELEMENT_LIST;
            let mut elements = if global {
                vec![
                    PPC_ISP_KEYBOARD_ELEMENT,
                    PPC_ISP_MOUSE_X_ELEMENT,
                    PPC_ISP_MOUSE_Y_ELEMENT,
                    PPC_ISP_MOUSE_BUTTON_ELEMENT,
                ]
            } else {
                Vec::new()
            };
            elements.extend(state.elements.iter().filter_map(|(reference, element)| {
                (global || element.device == cpu.gpr[3]).then_some(*reference)
            }));
            let result = write_references(memory, &elements, cpu.gpr[4], cpu.gpr[5], cpu.gpr[6]);
            return Some(PpcImportAction::Return(ppc_i16_result(result)));
        }
        PpcImportDispatcherTarget::ISpElementGetInfo
            if state.elements.contains_key(&cpu.gpr[3]) =>
        {
            let element = &state.elements[&cpu.gpr[3]];
            let mut info = [0; 80];
            info[..4].copy_from_slice(&element.definition[76..80]);
            info[4..8].copy_from_slice(&element.definition[72..76]);
            info[8..72].copy_from_slice(&element.definition[8..72]);
            let result = copy_output(memory, cpu.gpr[4], 80, &info, true);
            return Some(PpcImportAction::Return(ppc_i16_result(result)));
        }
        PpcImportDispatcherTarget::ISpElementGetConfigurationInfo
            if state.elements.contains_key(&cpu.gpr[3]) =>
        {
            let element = &state.elements[&cpu.gpr[3]];
            let result = copy_output(memory, cpu.gpr[5], cpu.gpr[4], &element.configuration, true);
            return Some(PpcImportAction::Return(ppc_i16_result(result)));
        }
        PpcImportDispatcherTarget::ISpElementGetSimpleState
            if state.elements.contains_key(&cpu.gpr[3]) =>
        {
            let element = &state.elements[&cpu.gpr[3]];
            let output = cpu.gpr[4];
            let result = if element.state.len() != 4
                || output == 0
                || !ppc_memory_can_write_bytes(memory, output, 4)
            {
                PPC_PARAM_ERR
            } else {
                for (offset, byte) in element.state.iter().enumerate() {
                    let _ = memory.write_u8(output + offset as u32, *byte);
                }
                PPC_NO_ERR
            };
            return Some(PpcImportAction::Return(ppc_i16_result(result)));
        }
        PpcImportDispatcherTarget::InputSprocketCompatibility(
            PpcInputSprocketCompatibilityOperation::ElementGetNextEvent,
        ) if state.elements.contains_key(&cpu.gpr[3]) => {
            let result = read_event(cpu, memory, state.elements.get_mut(&cpu.gpr[3]).unwrap());
            return Some(PpcImportAction::Return(ppc_i16_result(result)));
        }
        _ => {}
    }
    let PpcImportDispatcherTarget::InputSprocketDriver(operation) = binding.dispatcher_target
    else {
        return None;
    };
    let result = match operation {
        PpcInputSprocketDriverOperation::DeviceNew => {
            let output = cpu.gpr[6];
            let definition = read_bytes(memory, cpu.gpr[3], 92);
            if output == 0 || !ppc_memory_can_write_bytes(memory, output, 4) || definition.is_none()
            {
                PPC_PARAM_ERR
            } else {
                let _ = memory.write_u32_be(output, 0);
                let reference = manager.new_native_ptr(memory, 4, true);
                if reference == 0 {
                    PPC_MEM_FULL_ERR
                } else {
                    state.devices.insert(
                        reference,
                        DriverDevice {
                            definition: definition.unwrap(),
                            meta_handler: cpu.gpr[4],
                            ref_con: cpu.gpr[5],
                            active: false,
                        },
                    );
                    let _ = memory.write_u32_be(output, reference);
                    PPC_NO_ERR
                }
            }
        }
        PpcInputSprocketDriverOperation::DeviceDispose => {
            let reference = cpu.gpr[3];
            if state.devices.remove(&reference).is_none() {
                PPC_PARAM_ERR
            } else {
                let owned: Vec<u32> = state
                    .elements
                    .iter()
                    .filter_map(|(element, record)| {
                        (record.device == reference).then_some(*element)
                    })
                    .collect();
                for element in owned {
                    state.elements.remove(&element);
                    manager.dispose_native_ptr(element);
                }
                manager.dispose_native_ptr(reference);
                PPC_NO_ERR
            }
        }
        PpcInputSprocketDriverOperation::ElementNew => {
            let definition_ptr = cpu.gpr[3];
            let output = cpu.gpr[4];
            let result = (|| {
                if output == 0 || !ppc_memory_can_write_bytes(memory, output, 4) {
                    return PPC_PARAM_ERR;
                }
                let Some(definition) = read_bytes(memory, definition_ptr, 104) else {
                    return PPC_PARAM_ERR;
                };
                let word = |offset: usize| {
                    u32::from_be_bytes(definition[offset..offset + 4].try_into().unwrap())
                };
                let device = word(0);
                if !state.devices.contains_key(&device) {
                    return PPC_PARAM_ERR;
                }
                let Some(configuration) = read_bytes(memory, word(80), word(84)) else {
                    return PPC_PARAM_ERR;
                };
                let data_size = word(88);
                let _ = memory.write_u32_be(output, 0);
                // Account for state storage against the guest heap before reserving
                // host memory; a driver cannot bypass its application partition.
                let reference = manager.new_native_ptr(memory, data_size.max(4), true);
                if reference == 0 {
                    return PPC_MEM_FULL_ERR;
                }
                let mut data = Vec::new();
                if data.try_reserve_exact(data_size as usize).is_err() {
                    manager.dispose_native_ptr(reference);
                    return PPC_MEM_FULL_ERR;
                }
                data.resize(data_size as usize, 0);
                state.elements.insert(
                    reference,
                    DriverElement {
                        device,
                        definition,
                        configuration,
                        state: data,
                        events: VecDeque::new(),
                    },
                );
                let _ = memory.write_u32_be(output, reference);
                PPC_NO_ERR
            })();
            result
        }
        PpcInputSprocketDriverOperation::ElementGetComplexState => {
            if let Some(element) = state.elements.get(&cpu.gpr[3]) {
                let output = cpu.gpr[5];
                let size = (cpu.gpr[4] as usize).min(element.state.len());
                if output == 0 || !ppc_memory_can_write_bytes(memory, output, size as u32) {
                    PPC_PARAM_ERR
                } else {
                    for (offset, byte) in element.state.iter().take(size).enumerate() {
                        let _ = memory.write_u8(output + offset as u32, *byte);
                    }
                    PPC_NO_ERR
                }
            } else {
                PPC_PARAM_ERR
            }
        }
        PpcInputSprocketDriverOperation::ElementPushSimpleData
        | PpcInputSprocketDriverOperation::ElementPushComplexData => {
            let simple = operation == PpcInputSprocketDriverOperation::ElementPushSimpleData;
            let timestamp = if simple { cpu.gpr[5] } else { cpu.gpr[6] };
            let data = if simple {
                Some(cpu.gpr[4].to_be_bytes().to_vec())
            } else {
                read_bytes(memory, cpu.gpr[5], cpu.gpr[4])
            };
            let time = read_bytes(memory, timestamp, 8)
                .map(|bytes| u64::from_be_bytes(bytes.try_into().unwrap()));
            match (state.elements.get_mut(&cpu.gpr[3]), data, time) {
                (Some(element), Some(data), Some(when)) if data.len() == element.state.len() => {
                    if element.events.try_reserve(1).is_err() {
                        PPC_MEM_FULL_ERR
                    } else {
                        element.state.copy_from_slice(&data);
                        element.events.push_back(DriverEvent { when, data });
                        PPC_NO_ERR
                    }
                }
                _ => PPC_PARAM_ERR,
            }
        }
        PpcInputSprocketDriverOperation::ElementDispose => {
            let reference = cpu.gpr[3];
            if state.elements.remove(&reference).is_none() {
                PPC_PARAM_ERR
            } else {
                manager.dispose_native_ptr(reference);
                PPC_NO_ERR
            }
        }
    };
    let mut last_mem_error = PPC_NO_ERR;
    ppc_apply_process_native_allocator(manager, memory, heap_cursor, &mut last_mem_error);
    Some(PpcImportAction::Return(ppc_i16_result(result)))
}

// InputSprocket.h requires even a truncated event to be dequeued, with
// paramErr returned when the caller did not supply room for all its data.
fn read_event(cpu: &PpcCpu, memory: &mut PpcSectionMem, element: &mut DriverElement) -> i16 {
    let size = cpu.gpr[4];
    let output = cpu.gpr[5];
    let was_event = cpu.gpr[6];
    if was_event == 0
        || !ppc_memory_can_write_bytes(memory, was_event, 1)
        || (size != 0 && (output == 0 || !ppc_memory_can_write_bytes(memory, output, size)))
    {
        return PPC_PARAM_ERR;
    }
    let Some(event) = element.events.pop_front() else {
        let _ = memory.write_u8(was_event, 0);
        return PPC_NO_ERR;
    };
    let header = [(event.when >> 32) as u32, event.when as u32, cpu.gpr[3], 0];
    for offset in 0..(size as usize).min(16 + event.data.len()) {
        let byte = if offset < 16 {
            header[offset / 4].to_be_bytes()[offset % 4]
        } else {
            event.data[offset - 16]
        };
        let _ = memory.write_u8(output + offset as u32, byte);
    }
    let _ = memory.write_u8(was_event, 1);
    if (size as usize) < 16 + event.data.len() {
        PPC_PARAM_ERR
    } else {
        PPC_NO_ERR
    }
}

fn copy_output(
    memory: &mut PpcSectionMem,
    output: u32,
    capacity: u32,
    data: &[u8],
    short_error: bool,
) -> i16 {
    let size = (capacity as usize).min(data.len());
    if output == 0 || !ppc_memory_can_write_bytes(memory, output, size as u32) {
        return PPC_PARAM_ERR;
    }
    for (offset, byte) in data.iter().take(size).enumerate() {
        let _ = memory.write_u8(output + offset as u32, *byte);
    }
    if short_error && (capacity as usize) < data.len() {
        PPC_PARAM_ERR
    } else {
        PPC_NO_ERR
    }
}

fn write_references(
    memory: &mut PpcSectionMem,
    references: &[u32],
    capacity: u32,
    count: u32,
    output: u32,
) -> i16 {
    let size = (capacity as usize).min(references.len());
    if count == 0
        || !ppc_memory_can_write_bytes(memory, count, 4)
        || (size != 0
            && (output == 0 || !ppc_memory_can_write_bytes(memory, output, size as u32 * 4)))
    {
        return PPC_PARAM_ERR;
    }
    let _ = memory.write_u32_be(count, references.len() as u32);
    for (index, reference) in references.iter().take(size).enumerate() {
        let _ = memory.write_u32_be(output + index as u32 * 4, *reference);
    }
    PPC_NO_ERR
}
