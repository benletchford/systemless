//! Driver-owned InputSprocket devices and elements.
//!
//! Layouts and signatures follow Universal Interfaces 3.4.1 InputSprocket.h.

use super::*;
use std::collections::{BTreeMap, VecDeque};

#[cfg(test)]
thread_local! {
    static TEST_PUBLICATION_FAILURE_AFTER: std::cell::Cell<Option<usize>> = const { std::cell::Cell::new(None) };
}

#[cfg(test)]
pub(super) fn fail_publication_after_recipients_for_test(count: usize) {
    TEST_PUBLICATION_FAILURE_AFTER.with(|failure| failure.set(Some(count)));
}

#[cfg(test)]
fn publication_reservation_fails_for_test() -> bool {
    TEST_PUBLICATION_FAILURE_AFTER.with(|failure| match failure.get() {
        Some(0) => {
            failure.set(None);
            true
        }
        Some(remaining) => {
            failure.set(Some(remaining - 1));
            false
        }
        None => false,
    })
}

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
    enabled: bool,
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
    allocation: u32,
    when: u64,
    element: u32,
    ref_con: u32,
    data: DriverEventData,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum DriverEventData {
    Inline([u8; 4]),
    Bytes(Vec<u8>),
}

impl DriverEventData {
    fn from_vec(data: Vec<u8>) -> Self {
        match data.as_slice().try_into() {
            Ok(bytes) => Self::Inline(bytes),
            Err(_) => Self::Bytes(data),
        }
    }

    fn try_from_slice(data: &[u8]) -> Option<Self> {
        match data.try_into() {
            Ok(bytes) => Some(Self::Inline(bytes)),
            Err(_) => {
                let mut bytes = Vec::new();
                bytes.try_reserve_exact(data.len()).ok()?;
                bytes.extend_from_slice(data);
                Some(Self::Bytes(bytes))
            }
        }
    }
}

impl std::ops::Deref for DriverEventData {
    type Target = [u8];

    fn deref(&self) -> &Self::Target {
        match self {
            Self::Inline(bytes) => bytes,
            Self::Bytes(bytes) => bytes,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct BuiltinMouseEvent {
    element: u32,
    data: u32,
    when: u64,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(super) struct PpcInputSprocketDriverState {
    devices: BTreeMap<u32, DriverDevice>,
    elements: BTreeMap<u32, DriverElement>,
    callbacks: Vec<ActivationCall>,
    lists: BTreeMap<u32, VecDeque<DriverEvent>>,
    mouse_subscribed: bool,
    mouse_previous: Option<PpcInputSnapshot>,
    mouse_pending: VecDeque<BuiltinMouseEvent>,
    mouse_events: BTreeMap<u32, VecDeque<DriverEvent>>,
}

impl PpcInputSprocketDriverState {
    pub(super) fn note_mouse_sample(&mut self, input: PpcInputSnapshot, tick: u32, active: bool) {
        let previous = self.mouse_previous.replace(input);
        if !active {
            if let Some(previous) = previous {
                self.mouse_previous.as_mut().unwrap().mouse_button = previous.mouse_button;
            }
            return;
        }
        let Some(previous) = previous.filter(|_| self.mouse_subscribed) else {
            return;
        };
        let when = crate::time_manager::evaluate_microseconds(tick);
        let dx = i32::from(input.mouse_h) - i32::from(previous.mouse_h);
        let dy = i32::from(previous.mouse_v) - i32::from(input.mouse_v);
        for (element, value) in [
            (
                PPC_ISP_MOUSE_X_ELEMENT,
                (dx != 0).then_some(dx.wrapping_shl(16) as u32),
            ),
            (
                PPC_ISP_MOUSE_Y_ELEMENT,
                (dy != 0).then_some(dy.wrapping_shl(16) as u32),
            ),
            (
                PPC_ISP_MOUSE_BUTTON_ELEMENT,
                (input.mouse_button != previous.mouse_button)
                    .then_some(u32::from(input.mouse_button)),
            ),
        ] {
            if let Some(data) = value {
                self.mouse_pending.push_back(BuiltinMouseEvent {
                    element,
                    data,
                    when,
                });
            }
        }
    }
}

fn publish_mouse_events(
    state: &mut PpcInputSprocketDriverState,
    memory: &mut PpcSectionMem,
    manager: &mut ProcessNativeMemoryManager,
) -> bool {
    while let Some(event) = state.mouse_pending.front().copied() {
        let queue = state.mouse_events.entry(event.element).or_default();
        if queue.try_reserve(1).is_err() {
            return false;
        }
        // Physical samples originate outside the guest partition. Keeping their
        // queued copies in host memory lets the guest drain or flush them even
        // when its heap is full.
        let data = event.data.to_be_bytes();
        if !enqueue_list_events(
            &mut state.lists,
            memory,
            manager,
            event.element,
            PPC_ISP_MOUSE_DEVICE,
            event.when,
            &data,
            false,
        ) {
            return false;
        }
        queue.push_back(DriverEvent {
            allocation: 0,
            when: event.when,
            element: event.element,
            ref_con: 0,
            data: DriverEventData::Inline(data),
        });
        state.mouse_pending.pop_front();
    }
    true
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ActivationCall {
    import_pc: u32,
    return_pc: u32,
    rtoc: u32,
    pending: VecDeque<(u32, bool)>,
    current: Option<(u32, bool)>,
    invoking: bool,
    update_requested: bool,
    selector: u32,
    arguments: Vec<u32>,
    scratch: u32,
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
    cpu: &mut PpcCpu,
    memory: &mut PpcSectionMem,
    manager: &mut ProcessNativeMemoryManager,
    heap_cursor: &mut u32,
    state: &mut PpcInputSprocketDriverState,
    input: &mut PpcInputSprocketState,
    snapshot: PpcInputSnapshot,
    virtual_elements: &[PpcInputSprocketVirtualElementRecord],
) -> Option<PpcImportAction> {
    if binding.dispatcher_target == PpcImportDispatcherTarget::ISpShutdown {
        // Shutdown unloads InputSprocket's drivers; discard their pending
        // physical events and stop sampling until the next library use.
        state.mouse_subscribed = false;
        state.mouse_previous = None;
        state.mouse_pending.clear();
        for events in state.mouse_events.values_mut() {
            clear_queue(events, manager);
        }
        for element in [
            PPC_ISP_MOUSE_X_ELEMENT,
            PPC_ISP_MOUSE_Y_ELEMENT,
            PPC_ISP_MOUSE_BUTTON_ELEMENT,
        ] {
            remove_list_events(&mut state.lists, element, manager);
        }
    } else if binding.library_name == "InputSprocketLib" && !state.mouse_subscribed {
        state.mouse_subscribed = true;
        state.mouse_previous = Some(snapshot);
    }
    if binding.library_name == "InputSprocketLib" && !publish_mouse_events(state, memory, manager) {
        sync_heap(manager, memory, heap_cursor);
        return Some(PpcImportAction::Return(ppc_i16_result(PPC_MEM_FULL_ERR)));
    }
    if cpu.lr == cpu.pc
        && state
            .callbacks
            .last()
            .is_some_and(|call| call.import_pc == cpu.pc)
    {
        let action = resume_activation(cpu, memory, manager, state);
        sync_heap(manager, memory, heap_cursor);
        return Some(action);
    }
    if let Some(action) = activate_devices(binding, cpu, memory, manager, state, input) {
        sync_heap(manager, memory, heap_cursor);
        return Some(action);
    }
    // Keep list queues separate from the individual element queue: consuming or
    // flushing one subscription must not consume another subscription's events.
    match binding.dispatcher_target {
        PpcImportDispatcherTarget::ISpElementListNew => {
            let output = cpu.gpr[5];
            let result =
                ppc_isp_element_list_new(cpu, manager, memory, heap_cursor, virtual_elements);
            if result == PPC_NO_ERR {
                let reference = memory.read_u32_be(output).unwrap();
                state.lists.insert(reference, VecDeque::new());
            }
            return Some(PpcImportAction::Return(ppc_i16_result(result)));
        }
        PpcImportDispatcherTarget::ISpElementListDispose => {
            let reference = cpu.gpr[3];
            let result = ppc_isp_element_list_dispose(cpu, manager, memory, heap_cursor);
            if result == PPC_NO_ERR {
                if let Some(mut events) = state.lists.remove(&reference) {
                    clear_queue(&mut events, manager);
                }
                sync_heap(manager, memory, heap_cursor);
            }
            return Some(PpcImportAction::Return(ppc_i16_result(result)));
        }
        PpcImportDispatcherTarget::ISpElementListGetNextEvent
            if state.lists.contains_key(&cpu.gpr[3]) =>
        {
            let reference = cpu.gpr[3];
            let borrowed = reference == PPC_ISP_GLOBAL_ELEMENT_LIST
                || reference == PPC_ISP_MOUSE_DEVICE
                || reference == PPC_ISP_KEYBOARD_DEVICE
                || state.devices.contains_key(&reference);
            if !borrowed && ppc_isp_owned_element_list_count(memory, reference).is_none() {
                return Some(PpcImportAction::Return(ppc_i16_result(PPC_PARAM_ERR)));
            }
            let events = state.lists.get_mut(&reference).unwrap();
            if borrowed || !events.is_empty() {
                let result = read_event(cpu, memory, manager, events);
                sync_heap(manager, memory, heap_cursor);
                return Some(PpcImportAction::Return(ppc_i16_result(result)));
            }
            // An owned list can also contain virtual elements, polled below.
        }
        PpcImportDispatcherTarget::ISpElementListFlush if state.lists.contains_key(&cpu.gpr[3]) => {
            let reference = cpu.gpr[3];
            let borrowed = reference == PPC_ISP_GLOBAL_ELEMENT_LIST
                || reference == PPC_ISP_MOUSE_DEVICE
                || reference == PPC_ISP_KEYBOARD_DEVICE
                || state.devices.contains_key(&reference);
            let result = if borrowed {
                PPC_NO_ERR
            } else {
                ppc_isp_element_list_flush(cpu, memory, snapshot, *input, virtual_elements)
            };
            if result == PPC_NO_ERR {
                clear_queue(state.lists.get_mut(&reference).unwrap(), manager);
                sync_heap(manager, memory, heap_cursor);
            }
            return Some(PpcImportAction::Return(ppc_i16_result(result)));
        }
        _ => {}
    }
    // Existing application APIs share the same driver-owned element state.
    match binding.dispatcher_target {
        PpcImportDispatcherTarget::ISpGetGlobalElementList => {
            let output = cpu.gpr[3];
            if output == 0 || !ppc_memory_can_write_bytes(memory, output, 4) {
                return Some(PpcImportAction::Return(ppc_i16_result(PPC_PARAM_ERR)));
            }
            let _ = memory.write_u32_be(output, PPC_ISP_GLOBAL_ELEMENT_LIST);
            state.lists.entry(PPC_ISP_GLOBAL_ELEMENT_LIST).or_default();
            return Some(PpcImportAction::Return(0));
        }
        PpcImportDispatcherTarget::ISpDeviceGetElementList
            if matches!(cpu.gpr[3], PPC_ISP_MOUSE_DEVICE | PPC_ISP_KEYBOARD_DEVICE) =>
        {
            let result = ppc_isp_device_get_element_list(cpu, memory);
            if result == PPC_NO_ERR {
                state.lists.entry(cpu.gpr[3]).or_default();
            }
            return Some(PpcImportAction::Return(ppc_i16_result(result)));
        }
        PpcImportDispatcherTarget::InputSprocketCompatibility(
            PpcInputSprocketCompatibilityOperation::ElementFlush,
        ) if matches!(
            cpu.gpr[3],
            PPC_ISP_MOUSE_X_ELEMENT | PPC_ISP_MOUSE_Y_ELEMENT | PPC_ISP_MOUSE_BUTTON_ELEMENT
        ) =>
        {
            if let Some(events) = state.mouse_events.get_mut(&cpu.gpr[3]) {
                clear_queue(events, manager);
            }
            sync_heap(manager, memory, heap_cursor);
            return Some(PpcImportAction::Return(0));
        }
        PpcImportDispatcherTarget::InputSprocketCompatibility(
            PpcInputSprocketCompatibilityOperation::ElementGetNextEvent,
        ) if matches!(
            cpu.gpr[3],
            PPC_ISP_MOUSE_X_ELEMENT | PPC_ISP_MOUSE_Y_ELEMENT | PPC_ISP_MOUSE_BUTTON_ELEMENT
        ) =>
        {
            let events = state.mouse_events.entry(cpu.gpr[3]).or_default();
            if ppc_isp_device_is_active(input, PPC_ISP_MOUSE_DEVICE) != Some(true) {
                clear_queue(events, manager);
            }
            let result = read_event(cpu, memory, manager, events);
            sync_heap(manager, memory, heap_cursor);
            return Some(PpcImportAction::Return(ppc_i16_result(result)));
        }
        PpcImportDispatcherTarget::InputSprocketCompatibility(
            PpcInputSprocketCompatibilityOperation::DeviceIsActive,
        ) if state.devices.contains_key(&cpu.gpr[3]) => {
            let output = cpu.gpr[4];
            let result = if output == 0 || !ppc_memory_can_write_bytes(memory, output, 1) {
                PPC_PARAM_ERR
            } else {
                let active = state.devices[&cpu.gpr[3]].enabled && !input.suspended;
                let _ = memory.write_u8(output, u8::from(active));
                PPC_NO_ERR
            };
            return Some(PpcImportAction::Return(ppc_i16_result(result)));
        }
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
                let enabled = state.devices[&element.device].enabled && !input.suspended;
                for (offset, byte) in element.state.iter().enumerate() {
                    let _ =
                        memory.write_u8(output + offset as u32, if enabled { *byte } else { 0 });
                }
                PPC_NO_ERR
            };
            return Some(PpcImportAction::Return(ppc_i16_result(result)));
        }
        PpcImportDispatcherTarget::InputSprocketCompatibility(
            PpcInputSprocketCompatibilityOperation::ElementFlush,
        ) if state.elements.contains_key(&cpu.gpr[3]) => {
            clear_events(state.elements.get_mut(&cpu.gpr[3]).unwrap(), manager);
            sync_heap(manager, memory, heap_cursor);
            return Some(PpcImportAction::Return(0));
        }
        PpcImportDispatcherTarget::InputSprocketCompatibility(
            PpcInputSprocketCompatibilityOperation::ElementGetNextEvent,
        ) if state.elements.contains_key(&cpu.gpr[3]) => {
            let element = state.elements.get_mut(&cpu.gpr[3]).unwrap();
            if input.suspended || !state.devices[&element.device].enabled {
                clear_events(element, manager);
            }
            let result = read_event(cpu, memory, manager, &mut element.events);
            sync_heap(manager, memory, heap_cursor);
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
                            enabled: false,
                        },
                    );
                    state.lists.entry(PPC_ISP_GLOBAL_ELEMENT_LIST).or_default();
                    state.lists.insert(reference, VecDeque::new());
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
                    if let Some(mut record) = state.elements.remove(&element) {
                        clear_events(&mut record, manager);
                        remove_list_events(&mut state.lists, element, manager);
                        remove_list_memberships(&state.lists, element, memory);
                    }
                    manager.dispose_native_ptr(element);
                }
                state.lists.remove(&reference);
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
                    let enabled = state.devices[&element.device].enabled && !input.suspended;
                    for (offset, byte) in element.state.iter().take(size).enumerate() {
                        let _ = memory
                            .write_u8(output + offset as u32, if enabled { *byte } else { 0 });
                    }
                    PPC_NO_ERR
                }
            } else {
                let output = cpu.gpr[5];
                let size = cpu.gpr[4].min(4);
                if output == 0 || !ppc_memory_can_write_bytes(memory, output, size) {
                    PPC_PARAM_ERR
                } else if let Some(value) = ppc_isp_element_state_value(
                    cpu.gpr[3],
                    memory,
                    snapshot,
                    input,
                    virtual_elements,
                ) {
                    copy_output(memory, output, size, &value.to_be_bytes(), false)
                } else {
                    PPC_PARAM_ERR
                }
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
                    let enabled = state.devices[&element.device].enabled && !input.suspended;
                    if !enabled {
                        element.state.copy_from_slice(&data);
                        PPC_NO_ERR
                    } else if element.events.try_reserve(1).is_err() {
                        PPC_MEM_FULL_ERR
                    } else {
                        let size = u32::try_from(data.len())
                            .ok()
                            .and_then(|size| size.checked_add(16));
                        let allocation =
                            size.map_or(0, |size| manager.new_native_ptr(memory, size, false));
                        if allocation == 0 {
                            PPC_MEM_FULL_ERR
                        } else if !enqueue_list_events(
                            &mut state.lists,
                            memory,
                            manager,
                            cpu.gpr[3],
                            element.device,
                            when,
                            &data,
                            true,
                        ) {
                            manager.dispose_native_ptr(allocation);
                            PPC_MEM_FULL_ERR
                        } else {
                            element.state.copy_from_slice(&data);
                            element.events.push_back(DriverEvent {
                                allocation,
                                when,
                                element: cpu.gpr[3],
                                ref_con: 0,
                                data: DriverEventData::from_vec(data),
                            });
                            PPC_NO_ERR
                        }
                    }
                }
                _ => PPC_PARAM_ERR,
            }
        }
        PpcInputSprocketDriverOperation::ElementDispose => {
            let reference = cpu.gpr[3];
            if let Some(mut element) = state.elements.remove(&reference) {
                clear_events(&mut element, manager);
                remove_list_events(&mut state.lists, reference, manager);
                remove_list_memberships(&state.lists, reference, memory);
                manager.dispose_native_ptr(reference);
                PPC_NO_ERR
            } else {
                PPC_PARAM_ERR
            }
        }
    };
    let mut last_mem_error = PPC_NO_ERR;
    ppc_apply_process_native_allocator(manager, memory, heap_cursor, &mut last_mem_error);
    Some(PpcImportAction::Return(ppc_i16_result(result)))
}

// InputSprocket.h requires even a truncated event to be dequeued, with
// paramErr returned when the caller did not supply room for all its data.
fn read_event(
    cpu: &PpcCpu,
    memory: &mut PpcSectionMem,
    manager: &mut ProcessNativeMemoryManager,
    events: &mut VecDeque<DriverEvent>,
) -> i16 {
    let size = cpu.gpr[4];
    let output = cpu.gpr[5];
    let was_event = cpu.gpr[6];
    if was_event == 0
        || !ppc_memory_can_write_bytes(memory, was_event, 1)
        || (size != 0 && (output == 0 || !ppc_memory_can_write_bytes(memory, output, size)))
    {
        return PPC_PARAM_ERR;
    }
    let Some(event) = events.pop_front() else {
        let _ = memory.write_u8(was_event, 0);
        return PPC_NO_ERR;
    };
    let header = [
        (event.when >> 32) as u32,
        event.when as u32,
        event.element,
        event.ref_con,
    ];
    for offset in 0..(size as usize).min(16 + event.data.len()) {
        let byte = if offset < 16 {
            header[offset / 4].to_be_bytes()[offset % 4]
        } else {
            event.data[offset - 16]
        };
        let _ = memory.write_u8(output + offset as u32, byte);
    }
    let _ = memory.write_u8(was_event, 1);
    dispose_event_allocation(manager, event.allocation);
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

fn finish_activation(
    cpu: &mut PpcCpu,
    manager: &mut ProcessNativeMemoryManager,
    state: &mut PpcInputSprocketDriverState,
    result: u32,
) -> PpcImportAction {
    let call = state.callbacks.pop().unwrap();
    if call.scratch != 0 {
        manager.dispose_native_ptr(call.scratch);
    }
    cpu.lr = call.return_pc;
    cpu.gpr[2] = call.rtoc;
    PpcImportAction::Return(result)
}

fn call_driver(
    cpu: &mut PpcCpu,
    memory: &mut PpcSectionMem,
    call: &ActivationCall,
    procedure: u32,
    arguments: &[u32],
) -> Option<PpcImportAction> {
    let target = ppc_resolve_callback_target(memory, procedure, call.rtoc, None)?;
    install_powerpc_call_arguments(cpu, memory, arguments)?;
    GuestCallEffect::call_guest(
        GuestCallRequest::new(GuestCallTarget {
            isa: GuestIsa::PowerPc,
            entry: target.entry,
            rtoc: target.rtoc,
        }),
        GuestCallContinuation::to_powerpc(
            PPC_GUEST_CALL_RETURN_PC,
            call.import_pc,
            call.rtoc,
            PpcNativeReturnGpr3::Preserve,
        ),
    )
    .into_ppc_import_action()
}

fn next_activation(
    cpu: &mut PpcCpu,
    memory: &mut PpcSectionMem,
    manager: &mut ProcessNativeMemoryManager,
    state: &mut PpcInputSprocketDriverState,
) -> PpcImportAction {
    loop {
        let call = state.callbacks.last_mut().unwrap();
        let Some((reference, active)) = call.pending.pop_front() else {
            return finish_activation(cpu, manager, state, 0);
        };
        let Some(device) = state.devices.get_mut(&reference) else {
            return finish_activation(cpu, manager, state, ppc_i16_result(PPC_PARAM_ERR));
        };
        if call.selector == 13 && device.enabled == active {
            if call.update_requested {
                device.active = active;
            }
            continue;
        }
        if device.meta_handler == 0 {
            if call.selector == 13 || call.selector == 2 {
                device.enabled = active;
                if call.update_requested {
                    device.active = active;
                }
                if !active {
                    for (element_ref, element) in state
                        .elements
                        .iter_mut()
                        .filter(|(_, element)| element.device == reference)
                    {
                        clear_events(element, manager);
                        remove_list_events(&mut state.lists, *element_ref, manager);
                    }
                }
            }
            continue;
        }
        call.current = Some((reference, active));
        call.invoking = false;
        if let Some(action) = call_driver(
            cpu,
            memory,
            call,
            device.meta_handler,
            &[device.ref_con, call.selector],
        ) {
            return action;
        }
        return finish_activation(cpu, manager, state, ppc_i16_result(PPC_PARAM_ERR));
    }
}

fn resume_activation(
    cpu: &mut PpcCpu,
    memory: &mut PpcSectionMem,
    manager: &mut ProcessNativeMemoryManager,
    state: &mut PpcInputSprocketDriverState,
) -> PpcImportAction {
    let call = state.callbacks.last_mut().unwrap();
    let (reference, active) = call.current.unwrap();
    let Some(device) = state.devices.get_mut(&reference) else {
        return finish_activation(cpu, manager, state, ppc_i16_result(PPC_PARAM_ERR));
    };
    if !call.invoking {
        let procedure = cpu.gpr[3];
        if procedure != 0 {
            call.invoking = true;
            let mut arguments = vec![device.ref_con];
            if call.selector == 13 {
                arguments.push(u32::from(active));
            } else {
                arguments.extend_from_slice(&call.arguments);
            }
            if let Some(action) = call_driver(cpu, memory, call, procedure, &arguments) {
                return action;
            }
            return finish_activation(cpu, manager, state, ppc_i16_result(PPC_PARAM_ERR));
        }
    } else if cpu.gpr[3] != 0 {
        return finish_activation(cpu, manager, state, cpu.gpr[3]);
    }
    if call.selector == 13 || call.selector == 2 {
        device.enabled = active;
        if call.update_requested {
            device.active = active;
        }
        if !active {
            for (element_ref, element) in state
                .elements
                .iter_mut()
                .filter(|(_, element)| element.device == reference)
            {
                clear_events(element, manager);
                remove_list_events(&mut state.lists, *element_ref, manager);
            }
        }
    }
    next_activation(cpu, memory, manager, state)
}

fn activate_devices(
    binding: &PpcImportBinding,
    cpu: &mut PpcCpu,
    memory: &mut PpcSectionMem,
    manager: &mut ProcessNativeMemoryManager,
    state: &mut PpcInputSprocketDriverState,
    input: &mut PpcInputSprocketState,
) -> Option<PpcImportAction> {
    if state.devices.is_empty() {
        return None;
    }
    let selector = match binding.dispatcher_target {
        PpcImportDispatcherTarget::ISpStop | PpcImportDispatcherTarget::ISpShutdown => Some(2),
        PpcImportDispatcherTarget::InputSprocketCompatibility(
            PpcInputSprocketCompatibilityOperation::Tickle,
        ) => Some(15),
        _ => None,
    };
    if let Some(selector) = selector {
        if selector == 2 {
            ppc_isp_stop(input);
        }
        let pending = state
            .devices
            .iter()
            .filter_map(|(reference, device)| {
                (selector == 2 || (device.enabled && !input.suspended))
                    .then_some((*reference, false))
            })
            .collect();
        state.callbacks.push(ActivationCall {
            import_pc: cpu.pc,
            return_pc: cpu.lr,
            rtoc: cpu.gpr[2],
            pending,
            current: None,
            invoking: false,
            update_requested: selector == 2,
            selector,
            arguments: Vec::new(),
            scratch: 0,
        });
        return Some(next_activation(cpu, memory, manager, state));
    }
    if matches!(
        binding.dispatcher_target,
        PpcImportDispatcherTarget::ISpSuspend | PpcImportDispatcherTarget::ISpResume
    ) {
        let suspend = matches!(
            binding.dispatcher_target,
            PpcImportDispatcherTarget::ISpSuspend
        );
        if suspend {
            ppc_isp_suspend(input);
        } else {
            ppc_isp_resume(input);
        }
        let pending = state
            .devices
            .iter()
            .map(|(reference, device)| (*reference, !suspend && device.active))
            .collect();
        state.callbacks.push(ActivationCall {
            import_pc: cpu.pc,
            return_pc: cpu.lr,
            rtoc: cpu.gpr[2],
            pending,
            current: None,
            invoking: false,
            update_requested: false,
            selector: 13,
            arguments: Vec::new(),
            scratch: 0,
        });
        return Some(next_activation(cpu, memory, manager, state));
    }
    let (active, class) = match binding.dispatcher_target {
        PpcImportDispatcherTarget::ISpDevicesActivate => (true, None),
        PpcImportDispatcherTarget::ISpDevicesDeactivate => (false, None),
        PpcImportDispatcherTarget::InputSprocketCompatibility(
            PpcInputSprocketCompatibilityOperation::DevicesActivateClass,
        ) => (true, Some(cpu.gpr[3])),
        PpcImportDispatcherTarget::InputSprocketCompatibility(
            PpcInputSprocketCompatibilityOperation::DevicesDeactivateClass,
        ) => (false, Some(cpu.gpr[3])),
        _ => return None,
    };
    let devices = if let Some(class) = class {
        let mut devices: Vec<u32> = state
            .devices
            .iter()
            .filter_map(|(reference, device)| {
                (u32::from_be_bytes(device.definition[64..68].try_into().unwrap()) == class)
                    .then_some(*reference)
            })
            .collect();
        if class == PPC_ISP_DEVICE_CLASS_KEYBOARD {
            devices.push(PPC_ISP_KEYBOARD_DEVICE);
        }
        if class == PPC_ISP_DEVICE_CLASS_MOUSE {
            devices.push(PPC_ISP_MOUSE_DEVICE);
        }
        devices
    } else {
        // Universal Interfaces: the arguments are (count, device-reference array).
        let Some(size) = cpu.gpr[3].checked_mul(4) else {
            return Some(PpcImportAction::Return(ppc_i16_result(PPC_PARAM_ERR)));
        };
        let Some(bytes) = read_bytes(memory, cpu.gpr[4], size) else {
            return Some(PpcImportAction::Return(ppc_i16_result(PPC_PARAM_ERR)));
        };
        bytes
            .chunks_exact(4)
            .map(|word| u32::from_be_bytes(word.try_into().unwrap()))
            .collect()
    };
    if devices.iter().any(|reference| {
        !ppc_isp_known_device(*reference) && !state.devices.contains_key(reference)
    }) {
        return Some(PpcImportAction::Return(ppc_i16_result(PPC_PARAM_ERR)));
    }
    let mut pending = VecDeque::new();
    for reference in devices {
        if ppc_isp_known_device(reference) {
            ppc_isp_set_device_active(input, reference, active);
        } else {
            if input.suspended {
                state.devices.get_mut(&reference).unwrap().active = active;
            }
            pending.push_back((reference, active && !input.suspended));
        }
    }
    state.callbacks.push(ActivationCall {
        import_pc: cpu.pc,
        return_pc: cpu.lr,
        rtoc: cpu.gpr[2],
        pending,
        current: None,
        invoking: false,
        update_requested: !input.suspended,
        selector: 13,
        arguments: Vec::new(),
        scratch: 0,
    });
    Some(next_activation(cpu, memory, manager, state))
}

fn clear_events(element: &mut DriverElement, manager: &mut ProcessNativeMemoryManager) {
    clear_queue(&mut element.events, manager);
}

fn clear_queue(events: &mut VecDeque<DriverEvent>, manager: &mut ProcessNativeMemoryManager) {
    for event in events.drain(..) {
        dispose_event_allocation(manager, event.allocation);
    }
}

fn dispose_event_allocation(manager: &mut ProcessNativeMemoryManager, allocation: u32) {
    if allocation != 0 {
        manager.dispose_native_ptr(allocation);
    }
}

fn remove_list_events(
    lists: &mut BTreeMap<u32, VecDeque<DriverEvent>>,
    element: u32,
    manager: &mut ProcessNativeMemoryManager,
) {
    for events in lists.values_mut() {
        events.retain(|event| {
            if event.element == element {
                dispose_event_allocation(manager, event.allocation);
                false
            } else {
                true
            }
        });
    }
}

// References can be reused by the guest allocator. Remove disposed elements
// from subscriptions as well as queues so a later element cannot inherit them.
fn remove_list_memberships(
    lists: &BTreeMap<u32, VecDeque<DriverEvent>>,
    element: u32,
    memory: &mut PpcSectionMem,
) {
    for reference in lists.keys().copied() {
        let Some(count) = ppc_isp_owned_element_list_count(memory, reference) else {
            continue;
        };
        if !ppc_memory_can_write_bytes(memory, reference, PPC_ISP_ELEMENT_LIST_RECORD_SIZE) {
            continue;
        }
        let mut retained = 0;
        for index in 0..count {
            let (candidate, ref_con, previous) =
                ppc_isp_element_list_read_entry(memory, reference, index).unwrap();
            if candidate != element {
                ppc_isp_element_list_write_entry(
                    memory, reference, retained, candidate, ref_con, previous,
                );
                retained += 1;
            }
        }
        for index in retained..count {
            ppc_isp_element_list_write_entry(memory, reference, index, 0, 0, 0);
        }
        let _ = memory.write_u32_be(reference, retained);
    }
}

// Reserve every recipient before publishing any copy or state. Driver-pushed
// events also reserve guest memory, so exhaustion cannot cause partial delivery.
fn enqueue_list_events(
    lists: &mut BTreeMap<u32, VecDeque<DriverEvent>>,
    memory: &mut PpcSectionMem,
    manager: &mut ProcessNativeMemoryManager,
    element: u32,
    device: u32,
    when: u64,
    data: &[u8],
    guest_allocated: bool,
) -> bool {
    let mut pending: Vec<(u32, DriverEvent)> = Vec::new();
    for (reference, events) in lists.iter_mut() {
        let ref_con = if *reference == device || *reference == PPC_ISP_GLOBAL_ELEMENT_LIST {
            Some(0)
        } else {
            ppc_isp_owned_element_list_count(memory, *reference).and_then(|count| {
                (0..count).find_map(|index| {
                    let (candidate, ref_con, _) =
                        ppc_isp_element_list_read_entry(memory, *reference, index)?;
                    (candidate == element).then_some(ref_con)
                })
            })
        };
        let Some(ref_con) = ref_con else { continue };
        #[cfg(test)]
        let forced_failure = publication_reservation_fails_for_test();
        #[cfg(not(test))]
        let forced_failure = false;
        if forced_failure || events.try_reserve(1).is_err() || pending.try_reserve(1).is_err() {
            for (_, event) in pending {
                dispose_event_allocation(manager, event.allocation);
            }
            return false;
        }
        let Some(event_data) = DriverEventData::try_from_slice(data) else {
            for (_, event) in pending {
                dispose_event_allocation(manager, event.allocation);
            }
            return false;
        };
        let allocation = if guest_allocated {
            u32::try_from(data.len())
                .ok()
                .and_then(|size| size.checked_add(16))
                .map_or(0, |size| manager.new_native_ptr(memory, size, false))
        } else {
            0
        };
        if guest_allocated && allocation == 0 {
            for (_, event) in pending {
                dispose_event_allocation(manager, event.allocation);
            }
            return false;
        }
        pending.push((
            *reference,
            DriverEvent {
                allocation,
                when,
                element,
                ref_con,
                data: event_data,
            },
        ));
    }
    for (reference, event) in pending {
        lists.get_mut(&reference).unwrap().push_back(event);
    }
    true
}

fn sync_heap(manager: &ProcessNativeMemoryManager, memory: &mut PpcSectionMem, cursor: &mut u32) {
    let mut last_mem_error = PPC_NO_ERR;
    ppc_apply_process_native_allocator(manager, memory, cursor, &mut last_mem_error);
}

pub(super) fn initialize_drivers(
    cpu: &mut PpcCpu,
    memory: &mut PpcSectionMem,
    manager: &mut ProcessNativeMemoryManager,
    heap_cursor: &mut u32,
    state: &mut PpcInputSprocketDriverState,
) -> PpcImportAction {
    if state.devices.is_empty() {
        return PpcImportAction::Return(0);
    }
    let count = cpu.gpr[3];
    let needs = cpu.gpr[4];
    let elements = cpu.gpr[5];
    let valid = count
        .checked_mul(PPC_ISP_NEED_SIZE)
        .zip(count.checked_mul(4))
        .is_some_and(|(need_size, element_size)| {
            count == 0
                || (needs != 0
                    && elements != 0
                    && ppc_memory_can_read_bytes(memory, needs, need_size)
                    && ppc_memory_can_read_bytes(memory, elements, element_size))
        });
    if !valid {
        return PpcImportAction::Return(ppc_i16_result(PPC_PARAM_ERR));
    }
    // Each driver sees which needs previous drivers have already configured.
    let used = manager.new_native_ptr(memory, count.max(1), true);
    if used == 0 {
        sync_heap(manager, memory, heap_cursor);
        return PpcImportAction::Return(ppc_i16_result(PPC_MEM_FULL_ERR));
    }
    let arguments = vec![count, needs, elements, used, cpu.gpr[6], cpu.gpr[7], 0, 0];
    let pending = state
        .devices
        .keys()
        .map(|reference| (*reference, false))
        .collect();
    state.callbacks.push(ActivationCall {
        import_pc: cpu.pc,
        return_pc: cpu.lr,
        rtoc: cpu.gpr[2],
        pending,
        current: None,
        invoking: false,
        update_requested: false,
        selector: 1,
        arguments,
        scratch: used,
    });
    let action = next_activation(cpu, memory, manager, state);
    sync_heap(manager, memory, heap_cursor);
    action
}
