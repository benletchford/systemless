use super::*;

fn create_test_input_sprocket_list(loaded: &mut PpcLoadedApp, elements: &[u32]) -> u32 {
    let scratch = PPC_DATA_BASE + 0x5000;
    loaded
        .memory
        .add_region(scratch, vec![0; elements.len() * 4 + 4]);
    for (index, element) in elements.iter().enumerate() {
        loaded
            .memory
            .write_u32_be(scratch + index as u32 * 4, *element)
            .unwrap();
    }
    let output = scratch + elements.len() as u32 * 4;
    let saved_cpu = loaded.cpu.clone();
    let saved_target = loaded.imports[0].dispatcher_target.clone();
    loaded.cpu.gpr[3] = elements.len() as u32;
    loaded.cpu.gpr[4] = scratch;
    loaded.cpu.gpr[5] = output;
    loaded.cpu.gpr[6] = 0;
    run_test_import(loaded, PpcImportDispatcherTarget::ISpElementListNew);
    assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_NO_ERR));
    let list = loaded.memory.read_u32_be(output).unwrap();
    loaded.cpu = saved_cpu;
    loaded.imports[0].dispatcher_target = saved_target;
    list
}

#[test]
fn global_input_sprocket_element_list_resolves_and_enumerates_the_supported_devices() {
    let pef = synthetic_pef_with_library_import(b"InputSprocketLib", b"ISpGetGlobalElementList");
    let mut loaded = load_pef_application(&pef).unwrap();
    let output = PPC_DATA_BASE + 0x1000;
    let count = output + 4;
    let elements = output + 8;
    loaded.memory.add_region(output, vec![0xAA; 24]);
    loaded.cpu.gpr[3] = output;
    let probe = loaded.run_with_hle_imports(64);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_NO_ERR));
    let list = loaded.memory.read_u32_be(output).unwrap();
    assert_eq!(list, PPC_ISP_GLOBAL_ELEMENT_LIST);

    loaded.cpu.gpr[3] = list;
    loaded.cpu.gpr[4] = 2;
    loaded.cpu.gpr[5] = count;
    loaded.cpu.gpr[6] = elements;
    run_test_import(
        &mut loaded,
        PpcImportDispatcherTarget::ISpElementListExtract,
    );
    assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_NO_ERR));
    assert_eq!(loaded.memory.read_u32_be(count), Some(4));
    assert_eq!(
        loaded.memory.read_u32_be(elements),
        Some(PPC_ISP_KEYBOARD_ELEMENT)
    );
    assert_eq!(
        loaded.memory.read_u32_be(elements + 4),
        Some(PPC_ISP_MOUSE_X_ELEMENT)
    );
    assert_eq!(loaded.memory.read_u32_be(elements + 8), Some(0xAAAA_AAAA));

    loaded.cpu.gpr[3] = output;
    run_test_import(
        &mut loaded,
        PpcImportDispatcherTarget::ISpGetGlobalElementList,
    );
    assert_eq!(loaded.memory.read_u32_be(output), Some(list));
}

#[test]
fn global_input_sprocket_element_list_rejects_invalid_outputs_without_partial_writes() {
    let pef = synthetic_pef_with_library_import(b"InputSprocketLib", b"ISpGetGlobalElementList");
    let mut loaded = load_pef_application(&pef).unwrap();
    let short_output = PPC_DATA_BASE + 0x1000;
    loaded.memory.add_region(short_output, vec![0xAA; 3]);
    for output in [0, short_output, u32::MAX] {
        loaded.cpu.gpr[3] = output;
        run_test_import(
            &mut loaded,
            PpcImportDispatcherTarget::ISpGetGlobalElementList,
        );
        assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_PARAM_ERR));
    }
    assert_eq!(loaded.memory.read_u8(short_output), Some(0xAA));
    assert_eq!(loaded.memory.read_u8(short_output + 2), Some(0xAA));
}

#[test]
fn hle_import_runner_handles_input_sprocket_version_structure_result() {
    let pef = synthetic_pef_with_library_import(b"InputSprocketLib", b"ISpGetVersion");
    let mut loaded = load_pef_application(&pef).unwrap();
    let version_ptr = PPC_DATA_BASE + 0x1000;
    loaded.memory.add_region(version_ptr, vec![0; 4]);
    loaded.cpu.gpr[3] = version_ptr;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], version_ptr);
    assert_eq!(loaded.memory.read_u32_be(version_ptr), Some(0x0173_8000));
}

#[test]
fn hle_import_runner_creates_input_sprocket_element_list() {
    let pef = synthetic_pef_with_library_import(b"InputSprocketLib", b"ISpElementList_New");
    let mut loaded = load_pef_application(&pef).unwrap();
    let elements_ptr = PPC_DATA_BASE + 0x1000;
    let out_list_ptr = PPC_DATA_BASE + 0x1100;
    loaded.memory.add_region(elements_ptr, vec![0; 8]);
    loaded.memory.add_region(out_list_ptr, vec![0; 4]);
    loaded.cpu.gpr[3] = 2;
    loaded.cpu.gpr[4] = elements_ptr;
    loaded.cpu.gpr[5] = out_list_ptr;
    loaded.cpu.gpr[6] = 0x1234;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_NO_ERR));
    let list = loaded.memory.read_u32_be(out_list_ptr).unwrap();
    assert_ne!(list, 0);
    assert_eq!(loaded.memory.read_u32_be(list), Some(2));
    assert_eq!(loaded.memory.read_u32_be(list + 4), Some(0x1234));
}

#[test]
fn hle_import_runner_adds_input_sprocket_elements_to_list() {
    let pef = synthetic_pef_with_library_import(b"InputSprocketLib", b"ISpElementList_AddElements");
    let mut loaded = load_pef_application(&pef).unwrap();
    let list = create_test_input_sprocket_list(&mut loaded, &[0, 0]);
    let elements_ptr = PPC_DATA_BASE + 0x1100;
    loaded.memory.add_region(elements_ptr, vec![0; 8]);
    loaded.memory.write_u32_be(elements_ptr, 0x1111).unwrap();
    loaded
        .memory
        .write_u32_be(elements_ptr + 4, 0x2222)
        .unwrap();
    loaded.cpu.gpr[3] = list;
    loaded.cpu.gpr[4] = 7;
    loaded.cpu.gpr[5] = 2;
    loaded.cpu.gpr[6] = elements_ptr;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_NO_ERR));
    assert_eq!(loaded.memory.read_u32_be(list), Some(4));
    assert_eq!(
        ppc_isp_element_list_read_entry(&mut loaded.memory, list, 2),
        Some((0x1111, 7, 0))
    );
    assert_eq!(
        ppc_isp_element_list_read_entry(&mut loaded.memory, list, 3),
        Some((0x2222, 7, 0))
    );
}

#[test]
fn hle_import_runner_polls_empty_input_sprocket_element_list() {
    let pef =
        synthetic_pef_with_library_import(b"InputSprocketLib", b"ISpElementList_GetNextEvent");
    let mut loaded = load_pef_application(&pef).unwrap();
    let was_event_ptr = PPC_DATA_BASE + 0x1000;
    loaded.memory.add_region(was_event_ptr, vec![0xff]);
    loaded.cpu.gpr[3] = create_test_input_sprocket_list(&mut loaded, &[]);
    loaded.cpu.gpr[6] = was_event_ptr;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_NO_ERR));
    assert_eq!(loaded.memory.read_u8(was_event_ptr), Some(0));
}

#[test]
fn hle_import_runner_polls_empty_input_sprocket_element_without_writing_size() {
    let pef = synthetic_pef_with_library_import(b"InputSprocketLib", b"ISpElement_GetNextEvent");
    let mut loaded = load_pef_application(&pef).unwrap();
    let event_ptr = PPC_DATA_BASE + 0x1000;
    let was_event_ptr = PPC_DATA_BASE + 0x1100;
    loaded.memory.add_region(event_ptr, vec![0x5a; 20]);
    loaded.memory.add_region(was_event_ptr, vec![0xff]);
    loaded.cpu.gpr[3] = PPC_HEAP_BASE;
    loaded.cpu.gpr[4] = 20;
    loaded.cpu.gpr[5] = event_ptr;
    loaded.cpu.gpr[6] = was_event_ptr;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_NO_ERR));
    assert_eq!(loaded.memory.read_u8(was_event_ptr), Some(0));
    assert_eq!(loaded.memory.read_u8(event_ptr), Some(0x5a));
}

#[test]
fn input_sprocket_element_list_delivers_button_press_and_release_once() {
    let pef = synthetic_pef_with_library_import(b"InputSprocketLib", b"ISpGetVersion");
    let mut loaded = load_pef_application(&pef).unwrap();
    let event_ptr = PPC_DATA_BASE + 0x2000;
    let was_event_ptr = PPC_DATA_BASE + 0x2100;
    let element = 0x0300_1000;
    let list = create_test_input_sprocket_list(&mut loaded, &[element]);
    loaded.memory.add_region(event_ptr, vec![0; 20]);
    loaded.memory.add_region(was_event_ptr, vec![0; 1]);
    assert!(ppc_isp_element_list_write_entry(
        &mut loaded.memory,
        list,
        0,
        element,
        4,
        0,
    ));
    loaded
        .input_sprocket_virtual_elements
        .push(PpcInputSprocketVirtualElementRecord {
            element,
            need_index: 4,
            need_source: 0,
            kind: PPC_ISP_ELEMENT_KIND_BUTTON,
            default_state: 0,
            action_binding: PpcInputSprocketActionBinding::ButtonFire,
            keyboard_binding: None,
            need_name: "Fire".to_string(),
            need_record: Vec::new(),
        });
    loaded.cpu.gpr[3] = list;
    loaded.cpu.gpr[4] = PPC_ISP_ELEMENT_EVENT_SIZE;
    loaded.cpu.gpr[5] = event_ptr;
    loaded.cpu.gpr[6] = was_event_ptr;
    let active = PpcInputSprocketState {
        initialized: true,
        keyboard_active: true,
        mouse_active: true,
        ..PpcInputSprocketState::default()
    };
    let mut pressed = PpcInputSnapshot::default();
    pressed.key_map[(PPC_KEY_SPACE / 8) as usize] |= 1u8 << (PPC_KEY_SPACE % 8);

    assert_eq!(
        ppc_isp_element_list_get_next_event(
            &mut loaded.cpu,
            &mut loaded.memory,
            pressed,
            active,
            &loaded.input_sprocket_virtual_elements,
            123,
        ),
        PPC_NO_ERR
    );
    assert_eq!(loaded.memory.read_u8(was_event_ptr), Some(1));
    assert_eq!(loaded.memory.read_u32_be(event_ptr), Some(0));
    assert_eq!(loaded.memory.read_u32_be(event_ptr + 4), Some(2_044_875));
    assert_eq!(loaded.memory.read_u32_be(event_ptr + 8), Some(element));
    assert_eq!(loaded.memory.read_u32_be(event_ptr + 12), Some(4));
    assert_eq!(
        loaded.memory.read_u32_be(event_ptr + 16),
        Some(PPC_ISP_BUTTON_PRESSED)
    );

    assert_eq!(
        ppc_isp_element_list_get_next_event(
            &mut loaded.cpu,
            &mut loaded.memory,
            pressed,
            active,
            &loaded.input_sprocket_virtual_elements,
            124,
        ),
        PPC_NO_ERR
    );
    assert_eq!(loaded.memory.read_u8(was_event_ptr), Some(0));

    assert_eq!(
        ppc_isp_element_list_get_next_event(
            &mut loaded.cpu,
            &mut loaded.memory,
            PpcInputSnapshot::default(),
            active,
            &loaded.input_sprocket_virtual_elements,
            125,
        ),
        PPC_NO_ERR
    );
    assert_eq!(loaded.memory.read_u8(was_event_ptr), Some(1));
    assert_eq!(loaded.memory.read_u32_be(event_ptr + 16), Some(0));
}

#[test]
fn hle_import_runner_flushes_input_sprocket_element_list() {
    let pef = synthetic_pef_with_library_import(b"InputSprocketLib", b"ISpElementList_Flush");
    let mut loaded = load_pef_application(&pef).unwrap();
    loaded.cpu.gpr[3] = create_test_input_sprocket_list(&mut loaded, &[]);

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_NO_ERR));
}

#[test]
fn hle_import_runner_handles_input_sprocket_virtual_elements() {
    let pef =
        synthetic_pef_with_library_import(b"InputSprocketLib", b"ISpElement_NewVirtualFromNeeds");
    let mut loaded = load_pef_application(&pef).unwrap();
    let needs_ptr = PPC_DATA_BASE + 0x1000;
    let elements_out_ptr = PPC_DATA_BASE + 0x1100;
    loaded
        .memory
        .add_region(needs_ptr, vec![0; (PPC_ISP_NEED_SIZE * 2) as usize]);
    loaded.memory.add_region(elements_out_ptr, vec![0; 8]);
    write_ppc_pstring(&mut loaded.memory, needs_ptr, b"Yaw");
    write_ppc_pstring(
        &mut loaded.memory,
        needs_ptr + PPC_ISP_NEED_SIZE,
        b"Primary Trigger",
    );
    loaded
        .memory
        .write_u32_be(
            needs_ptr + PPC_ISP_NEED_KIND_OFFSET,
            PPC_ISP_ELEMENT_KIND_AXIS,
        )
        .unwrap();
    loaded
        .memory
        .write_u32_be(
            needs_ptr + PPC_ISP_NEED_SIZE + PPC_ISP_NEED_KIND_OFFSET,
            PPC_ISP_ELEMENT_KIND_BUTTON,
        )
        .unwrap();
    loaded.cpu.gpr[3] = 2;
    loaded.cpu.gpr[4] = needs_ptr;
    loaded.cpu.gpr[5] = elements_out_ptr;
    loaded.cpu.gpr[6] = 0;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_NO_ERR));
    let axis_element = loaded.memory.read_u32_be(elements_out_ptr).unwrap();
    let button_element = loaded.memory.read_u32_be(elements_out_ptr + 4).unwrap();
    assert_ne!(axis_element, 0);
    assert_ne!(button_element, 0);
    assert_eq!(
        loaded.memory.read_u32_be(axis_element),
        Some(PPC_ISP_ELEMENT_KIND_AXIS)
    );
    assert_eq!(
        loaded.memory.read_u32_be(axis_element + 4),
        Some(PPC_ISP_AXIS_MIDDLE)
    );
    assert_eq!(
        loaded
            .memory
            .read_u32_be(axis_element + PPC_ISP_ELEMENT_NEED_INDEX_OFFSET),
        Some(0)
    );
    assert_eq!(
        loaded
            .memory
            .read_u32_be(axis_element + PPC_ISP_ELEMENT_NEED_SOURCE_OFFSET),
        Some(needs_ptr)
    );
    assert_eq!(
        loaded
            .memory
            .read_u8(axis_element + PPC_ISP_ELEMENT_NEED_RECORD_OFFSET),
        Some(3)
    );
    assert_eq!(
        loaded
            .memory
            .read_u8(axis_element + PPC_ISP_ELEMENT_NEED_RECORD_OFFSET + 1),
        Some(b'Y')
    );
    assert_eq!(
        loaded.memory.read_u32_be(button_element),
        Some(PPC_ISP_ELEMENT_KIND_BUTTON)
    );
    assert_eq!(loaded.memory.read_u32_be(button_element + 4), Some(0));
    assert_eq!(loaded.input_sprocket.virtual_element_count, 2);
    assert_eq!(loaded.input_sprocket.last_virtual_need_count, 2);
    assert_eq!(loaded.input_sprocket.last_virtual_needs_ptr, needs_ptr);
    assert_eq!(
        loaded.input_sprocket.last_virtual_elements_out_ptr,
        elements_out_ptr
    );
    assert_eq!(loaded.input_sprocket_virtual_elements.len(), 2);
    let axis_record = &loaded.input_sprocket_virtual_elements[0];
    assert_eq!(axis_record.element, axis_element);
    assert_eq!(axis_record.need_index, 0);
    assert_eq!(axis_record.need_source, needs_ptr);
    assert_eq!(axis_record.kind, PPC_ISP_ELEMENT_KIND_AXIS);
    assert_eq!(axis_record.default_state, PPC_ISP_AXIS_MIDDLE);
    assert_eq!(
        axis_record.action_binding,
        PpcInputSprocketActionBinding::AxisYaw
    );
    assert_eq!(axis_record.need_name, "Yaw");
    assert_eq!(axis_record.need_record.len(), PPC_ISP_NEED_SIZE as usize);
    assert_eq!(axis_record.need_record[0], 3);
    assert_eq!(&axis_record.need_record[1..4], b"Yaw");
    assert_eq!(
        ppc_isp_need_record_kind(&axis_record.need_record),
        Some(PPC_ISP_ELEMENT_KIND_AXIS)
    );
    let button_record = &loaded.input_sprocket_virtual_elements[1];
    assert_eq!(button_record.element, button_element);
    assert_eq!(button_record.need_index, 1);
    assert_eq!(button_record.need_source, needs_ptr + PPC_ISP_NEED_SIZE);
    assert_eq!(button_record.kind, PPC_ISP_ELEMENT_KIND_BUTTON);
    assert_eq!(button_record.default_state, 0);
    assert_eq!(
        button_record.action_binding,
        PpcInputSprocketActionBinding::ButtonPrimary
    );
    assert_eq!(button_record.need_name, "Primary Trigger");
    assert_eq!(button_record.need_record.len(), PPC_ISP_NEED_SIZE as usize);
    assert_eq!(
        ppc_isp_need_record_kind(&button_record.need_record),
        Some(PPC_ISP_ELEMENT_KIND_BUTTON)
    );
}

#[test]
fn hle_import_runner_input_sprocket_virtual_elements_param_err_does_not_allocate() {
    let pef =
        synthetic_pef_with_library_import(b"InputSprocketLib", b"ISpElement_NewVirtualFromNeeds");
    let mut loaded = load_pef_application(&pef).unwrap();
    let needs_ptr = PPC_DATA_BASE + 0x1000;
    let elements_out_ptr = PPC_DATA_BASE + 0x1100;
    loaded
        .memory
        .add_region(needs_ptr, vec![0; PPC_ISP_NEED_SIZE as usize]);
    loaded.memory.add_region(elements_out_ptr, vec![0xaa; 2]);
    write_ppc_pstring(&mut loaded.memory, needs_ptr, b"Yaw");
    loaded
        .memory
        .write_u32_be(
            needs_ptr + PPC_ISP_NEED_KIND_OFFSET,
            PPC_ISP_ELEMENT_KIND_AXIS,
        )
        .unwrap();
    let heap_cursor = loaded.heap_cursor();
    let region_count = loaded.memory.region_count();
    let input_sprocket = loaded.input_sprocket;
    loaded.cpu.gpr[3] = 1;
    loaded.cpu.gpr[4] = needs_ptr;
    loaded.cpu.gpr[5] = elements_out_ptr;
    loaded.cpu.gpr[6] = 0;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_PARAM_ERR));
    assert_eq!(loaded.heap_cursor(), heap_cursor);
    assert_eq!(loaded.memory.region_count(), region_count);
    assert_eq!(loaded.memory.read_u8(elements_out_ptr), Some(0xaa));
    assert_eq!(loaded.memory.read_u8(elements_out_ptr + 1), Some(0xaa));
    assert_eq!(loaded.input_sprocket, input_sprocket);
    assert!(loaded.input_sprocket_virtual_elements.is_empty());
}

#[test]
fn hle_import_runner_input_sprocket_virtual_elements_heap_full_does_not_partially_allocate() {
    let pef =
        synthetic_pef_with_library_import(b"InputSprocketLib", b"ISpElement_NewVirtualFromNeeds");
    let mut loaded = load_pef_application(&pef).unwrap();
    let needs_ptr = PPC_DATA_BASE + 0x1000;
    let elements_out_ptr = PPC_DATA_BASE + 0x1100;
    loaded
        .memory
        .add_region(needs_ptr, vec![0; (PPC_ISP_NEED_SIZE * 2) as usize]);
    loaded.memory.add_region(elements_out_ptr, vec![0xaa; 8]);
    write_ppc_pstring(&mut loaded.memory, needs_ptr, b"Yaw");
    write_ppc_pstring(
        &mut loaded.memory,
        needs_ptr + PPC_ISP_NEED_SIZE,
        b"Primary Trigger",
    );
    loaded
        .memory
        .write_u32_be(
            needs_ptr + PPC_ISP_NEED_KIND_OFFSET,
            PPC_ISP_ELEMENT_KIND_AXIS,
        )
        .unwrap();
    loaded
        .memory
        .write_u32_be(
            needs_ptr + PPC_ISP_NEED_SIZE + PPC_ISP_NEED_KIND_OFFSET,
            PPC_ISP_ELEMENT_KIND_BUTTON,
        )
        .unwrap();
    let heap_cursor = loaded.heap_cursor();
    let region_count = loaded.memory.region_count();
    let input_sprocket = loaded.input_sprocket;
    let element_size = ppc_allocation_size(PPC_ISP_VIRTUAL_ELEMENT_RECORD_SIZE).unwrap();
    loaded.set_heap_limit(heap_cursor + element_size * 2 - 4);
    loaded.cpu.gpr[3] = 2;
    loaded.cpu.gpr[4] = needs_ptr;
    loaded.cpu.gpr[5] = elements_out_ptr;
    loaded.cpu.gpr[6] = 0;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_MEM_FULL_ERR));
    assert_eq!(loaded.heap_cursor(), heap_cursor);
    assert_eq!(loaded.memory.region_count(), region_count);
    for offset in 0..8 {
        assert_eq!(loaded.memory.read_u8(elements_out_ptr + offset), Some(0xaa));
    }
    assert_eq!(loaded.input_sprocket, input_sprocket);
    assert!(loaded.input_sprocket_virtual_elements.is_empty());
}

#[test]
fn hle_import_runner_uses_retained_input_sprocket_action_binding() {
    let pef =
        synthetic_pef_with_library_import(b"InputSprocketLib", b"ISpElement_NewVirtualFromNeeds");
    let mut loaded = load_pef_application(&pef).unwrap();
    let needs_ptr = PPC_DATA_BASE + 0x1000;
    let elements_out_ptr = PPC_DATA_BASE + 0x1100;
    let state_ptr = PPC_DATA_BASE + 0x1200;
    loaded
        .memory
        .add_region(needs_ptr, vec![0; PPC_ISP_NEED_SIZE as usize]);
    loaded.memory.add_region(elements_out_ptr, vec![0; 4]);
    loaded.memory.add_region(state_ptr, vec![0; 4]);
    write_ppc_pstring(&mut loaded.memory, needs_ptr, b"Turn Left");
    loaded
        .memory
        .write_u32_be(
            needs_ptr + PPC_ISP_NEED_KIND_OFFSET,
            PPC_ISP_ELEMENT_KIND_BUTTON,
        )
        .unwrap();
    loaded.cpu.gpr[3] = 1;
    loaded.cpu.gpr[4] = needs_ptr;
    loaded.cpu.gpr[5] = elements_out_ptr;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    let element = loaded.memory.read_u32_be(elements_out_ptr).unwrap();
    assert_eq!(
        loaded.input_sprocket_virtual_elements[0].action_binding,
        PpcInputSprocketActionBinding::ButtonLeft
    );
    write_ppc_pstring(
        &mut loaded.memory,
        element + PPC_ISP_ELEMENT_NEED_RECORD_OFFSET,
        b"Primary Trigger",
    );
    let mut input = PpcInputSnapshot::default();
    input.key_map[(PPC_KEY_LEFT / 8) as usize] |= 1u8 << (PPC_KEY_LEFT % 8);
    loaded.set_input_snapshot(input);
    loaded.cpu.pc = loaded.entry_pc;
    loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::ISpElementGetSimpleState;
    loaded.cpu.gpr[3] = element;
    loaded.cpu.gpr[4] = state_ptr;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_NO_ERR));
    assert_eq!(
        loaded.memory.read_u32_be(state_ptr),
        Some(PPC_ISP_BUTTON_PRESSED)
    );
}

#[test]
fn hle_import_runner_handles_input_sprocket_devices_extract() {
    let pef = synthetic_pef_with_library_import(b"InputSprocketLib", b"ISpDevices_Extract");
    let mut loaded = load_pef_application(&pef).unwrap();
    let scratch = PPC_DATA_BASE + 0x1000;
    let out_count_ptr = scratch;
    let buffer_ptr = scratch + 4;
    loaded.memory.add_region(scratch, vec![0; 12]);
    loaded.cpu.gpr[3] = 2;
    loaded.cpu.gpr[4] = out_count_ptr;
    loaded.cpu.gpr[5] = buffer_ptr;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_NO_ERR));
    assert_eq!(
        loaded.memory.read_u32_be(out_count_ptr),
        Some(PPC_ISP_DEVICE_COUNT)
    );
    assert_eq!(
        loaded.memory.read_u32_be(buffer_ptr),
        Some(PPC_ISP_KEYBOARD_DEVICE)
    );
    assert_eq!(
        loaded.memory.read_u32_be(buffer_ptr + 4),
        Some(PPC_ISP_MOUSE_DEVICE)
    );
}

#[test]
fn hle_import_runner_handles_input_sprocket_devices_extract_by_class() {
    let pef = synthetic_pef_with_library_import(b"InputSprocketLib", b"ISpDevices_ExtractByClass");
    let mut loaded = load_pef_application(&pef).unwrap();
    let scratch = PPC_DATA_BASE + 0x1000;
    let out_count_ptr = scratch;
    let buffer_ptr = scratch + 4;
    loaded.memory.add_region(scratch, vec![0; 8]);
    loaded.cpu.gpr[3] = PPC_ISP_DEVICE_CLASS_MOUSE;
    loaded.cpu.gpr[4] = 1;
    loaded.cpu.gpr[5] = out_count_ptr;
    loaded.cpu.gpr[6] = buffer_ptr;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_NO_ERR));
    assert_eq!(loaded.memory.read_u32_be(out_count_ptr), Some(1));
    assert_eq!(
        loaded.memory.read_u32_be(buffer_ptr),
        Some(PPC_ISP_MOUSE_DEVICE)
    );

    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.cpu.gpr[3] = u32::from_be_bytes(*b"joys");
    loaded.cpu.gpr[4] = 0;
    loaded.cpu.gpr[5] = out_count_ptr;
    loaded.cpu.gpr[6] = 0;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_NO_ERR));
    assert_eq!(loaded.memory.read_u32_be(out_count_ptr), Some(0));
}

#[test]
fn hle_import_runner_handles_input_sprocket_device_get_element_list() {
    let pef = synthetic_pef_with_library_import(b"InputSprocketLib", b"ISpDevice_GetElementList");
    let mut loaded = load_pef_application(&pef).unwrap();
    let out_element_list_ptr = PPC_DATA_BASE + 0x1000;
    loaded.memory.add_region(out_element_list_ptr, vec![0; 4]);
    loaded.cpu.gpr[3] = PPC_ISP_KEYBOARD_DEVICE;
    loaded.cpu.gpr[4] = out_element_list_ptr;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_NO_ERR));
    assert_eq!(
        loaded.memory.read_u32_be(out_element_list_ptr),
        Some(PPC_ISP_KEYBOARD_DEVICE)
    );
}

#[test]
fn hle_import_runner_handles_input_sprocket_element_list_extract() {
    let pef = synthetic_pef_with_library_import(b"InputSprocketLib", b"ISpElementList_Extract");
    let mut loaded = load_pef_application(&pef).unwrap();
    let scratch = PPC_DATA_BASE + 0x1000;
    let out_count_ptr = scratch;
    let buffer_ptr = scratch + 4;
    loaded.memory.add_region(scratch, vec![0; 8]);
    loaded.cpu.gpr[3] = PPC_ISP_MOUSE_DEVICE;
    loaded.cpu.gpr[4] = 1;
    loaded.cpu.gpr[5] = out_count_ptr;
    loaded.cpu.gpr[6] = buffer_ptr;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_NO_ERR));
    assert_eq!(loaded.memory.read_u32_be(out_count_ptr), Some(3));
    assert_eq!(
        loaded.memory.read_u32_be(buffer_ptr),
        Some(PPC_ISP_MOUSE_X_ELEMENT)
    );
}

#[test]
fn hle_import_runner_handles_input_sprocket_element_get_info() {
    let pef = synthetic_pef_with_library_import(b"InputSprocketLib", b"ISpElement_GetInfo");
    let mut loaded = load_pef_application(&pef).unwrap();
    let info_ptr = PPC_DATA_BASE + 0x1000;
    loaded
        .memory
        .add_region(info_ptr, vec![0xaa; PPC_ISP_ELEMENT_INFO_SIZE as usize]);
    loaded.cpu.gpr[3] = PPC_ISP_MOUSE_BUTTON_ELEMENT;
    loaded.cpu.gpr[4] = info_ptr;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_NO_ERR));
    assert_eq!(
        loaded.memory.read_u32_be(info_ptr),
        Some(PPC_ISP_ELEMENT_LABEL_MOUSE_ONE)
    );
    assert_eq!(
        loaded.memory.read_u32_be(info_ptr + 4),
        Some(PPC_ISP_ELEMENT_KIND_BUTTON)
    );
    assert_eq!(loaded.memory.read_u8(info_ptr + 8), Some(12));
    assert_eq!(loaded.memory.read_u8(info_ptr + 9), Some(b'M'));
    assert_eq!(loaded.memory.read_u32_be(info_ptr + 72), Some(0));
    assert_eq!(loaded.memory.read_u32_be(info_ptr + 76), Some(0));
}

#[test]
fn hle_import_runner_handles_input_sprocket_element_configuration_info() {
    let library = b"InputSprocketLib";
    let symbol = b"ISpElement_GetConfigurationInfo";
    let weak_pef = synthetic_pef_with_loader(synthetic_loader_with_symbol_class(
        library,
        symbol,
        0x82,
        &[sm_index_reloc(0x30, 0)],
    ));
    let mut weak_loaded = load_pef_application(&weak_pef).unwrap();
    assert_eq!(
        weak_loaded.imports[0].dispatcher_target,
        PpcImportDispatcherTarget::ISpElementGetConfigurationInfo
    );
    assert_ne!(weak_loaded.imports[0].address, 0);
    assert_eq!(
        weak_loaded.memory.read_u32_be(PPC_DATA_BASE),
        Some(weak_loaded.imports[0].address)
    );

    let mut loaded = load_pef_application(&synthetic_pef_with_library_import(library, symbol))
        .unwrap();
    let config_ptr = PPC_DATA_BASE + 0x1000;
    loaded.memory.add_region(config_ptr, vec![0xaa; 12]);
    loaded.cpu.gpr[3] = PPC_ISP_MOUSE_BUTTON_ELEMENT;
    loaded.cpu.gpr[4] = 4;
    loaded.cpu.gpr[5] = config_ptr;
    let probe = loaded.run_with_hle_imports(64);
    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_NO_ERR));
    assert_eq!(loaded.memory.read_u32_be(config_ptr), Some(1));
    assert_eq!(loaded.memory.read_u8(config_ptr + 4), Some(0xaa));

    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.gpr[3] = PPC_ISP_MOUSE_X_ELEMENT;
    loaded.cpu.gpr[4] = 8;
    loaded.cpu.gpr[5] = config_ptr;
    let probe = loaded.run_with_hle_imports(64);
    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_NO_ERR));
    assert_eq!(loaded.memory.read_u32_be(config_ptr), Some(0));
    assert_eq!(loaded.memory.read_u32_be(config_ptr + 4), Some(0));
    assert_eq!(loaded.memory.read_u8(config_ptr + 8), Some(0xaa));

    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.gpr[3] = PPC_ISP_KEYBOARD_ELEMENT;
    loaded.cpu.gpr[4] = 2;
    loaded.cpu.gpr[5] = config_ptr;
    loaded.memory.write_u32_be(config_ptr, 0xaaaa_aaaa).unwrap();
    let probe = loaded.run_with_hle_imports(64);
    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_PARAM_ERR));
    assert_eq!(loaded.memory.read_u32_be(config_ptr), Some(0x0000_aaaa));

    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.gpr[3] = PPC_ISP_MOUSE_BUTTON_ELEMENT;
    loaded.cpu.gpr[4] = 4;
    loaded.cpu.gpr[5] = 0;
    let probe = loaded.run_with_hle_imports(64);
    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_PARAM_ERR));
}

#[test]
fn hle_import_runner_handles_input_sprocket_simple_state() {
    let pef = synthetic_pef_with_library_import(b"InputSprocketLib", b"ISpElement_GetSimpleState");
    let mut loaded = load_pef_application(&pef).unwrap();
    let element = PPC_HEAP_BASE;
    let state_ptr = PPC_DATA_BASE + 0x1000;
    loaded.memory.add_region(element, vec![0; 8]);
    loaded.memory.add_region(state_ptr, vec![0; 4]);
    loaded
        .memory
        .write_u32_be(element, PPC_ISP_ELEMENT_KIND_AXIS)
        .unwrap();
    loaded
        .memory
        .write_u32_be(element + 4, PPC_ISP_AXIS_MIDDLE)
        .unwrap();
    loaded.set_heap_cursor(element + 8);
    loaded.cpu.gpr[3] = element;
    loaded.cpu.gpr[4] = state_ptr;

    let probe = loaded.run_with_hle_import_trace(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_NO_ERR));
    assert_eq!(
        loaded.memory.read_u32_be(state_ptr),
        Some(PPC_ISP_AXIS_MIDDLE)
    );
    assert_eq!(probe.input_sprocket_trace.len(), 1);
    let trace = &probe.input_sprocket_trace[0];
    assert_eq!(trace.import_index, 0);
    assert_eq!(trace.element, element);
    assert_eq!(trace.state_ptr, state_ptr);
    assert_eq!(trace.state, PPC_ISP_AXIS_MIDDLE);
    assert_eq!(trace.kind, PPC_ISP_ELEMENT_KIND_AXIS);
    assert_eq!(trace.kind_name, "axis");
    assert_eq!(trace.fallback_state, PPC_ISP_AXIS_MIDDLE);
    assert_eq!(trace.action_binding, "axis/directional");
    assert_eq!(trace.input, PpcInputSnapshot::default());
    assert_eq!(trace.input_sprocket, PpcInputSprocketState::default());
}

#[test]
fn hle_import_runner_prevalidates_input_sprocket_output_buffers() {
    let pef = synthetic_pef_with_library_import(b"InputSprocketLib", b"ISpDevices_Extract");
    let mut loaded = load_pef_application(&pef).unwrap();
    let short_count_ptr = PPC_DATA_BASE + 0x1000;
    let valid_count_ptr = PPC_DATA_BASE + 0x1100;
    let short_buffer_ptr = PPC_DATA_BASE + 0x1200;
    let short_state_ptr = PPC_DATA_BASE + 0x1300;
    let element = PPC_HEAP_BASE;
    loaded.memory.add_region(short_count_ptr, vec![0xb1; 3]);
    loaded.memory.add_region(valid_count_ptr, vec![0xb2; 4]);
    loaded.memory.add_region(short_buffer_ptr, vec![0xb3; 7]);
    loaded.memory.add_region(short_state_ptr, vec![0xb4; 3]);
    loaded.memory.add_region(element, vec![0; 8]);
    loaded
        .memory
        .write_u32_be(element, PPC_ISP_ELEMENT_KIND_AXIS)
        .unwrap();
    loaded
        .memory
        .write_u32_be(element + 4, PPC_ISP_AXIS_MIDDLE)
        .unwrap();
    loaded.cpu.gpr[3] = 2;
    loaded.cpu.gpr[4] = short_count_ptr;
    loaded.cpu.gpr[5] = short_buffer_ptr;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_PARAM_ERR));
    for offset in 0..3 {
        assert_eq!(loaded.memory.read_u8(short_count_ptr + offset), Some(0xb1));
    }
    for offset in 0..7 {
        assert_eq!(loaded.memory.read_u8(short_buffer_ptr + offset), Some(0xb3));
    }

    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.cpu.gpr[3] = 2;
    loaded.cpu.gpr[4] = valid_count_ptr;
    loaded.cpu.gpr[5] = short_buffer_ptr;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_PARAM_ERR));
    for offset in 0..4 {
        assert_eq!(loaded.memory.read_u8(valid_count_ptr + offset), Some(0xb2));
    }
    for offset in 0..7 {
        assert_eq!(loaded.memory.read_u8(short_buffer_ptr + offset), Some(0xb3));
    }

    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::ISpElementGetSimpleState;
    loaded.cpu.gpr[3] = element;
    loaded.cpu.gpr[4] = short_state_ptr;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_PARAM_ERR));
    for offset in 0..3 {
        assert_eq!(loaded.memory.read_u8(short_state_ptr + offset), Some(0xb4));
    }
}

#[test]
fn hle_import_runner_maps_input_snapshot_to_input_sprocket_state() {
    let pef = synthetic_pef_with_library_import(b"InputSprocketLib", b"ISpElement_GetSimpleState");
    let mut loaded = load_pef_application(&pef).unwrap();
    let element = PPC_HEAP_BASE;
    let state_ptr = PPC_DATA_BASE + 0x1000;
    loaded.memory.add_region(element, vec![0; 8]);
    loaded.memory.add_region(state_ptr, vec![0; 4]);
    loaded
        .memory
        .write_u32_be(element, PPC_ISP_ELEMENT_KIND_BUTTON)
        .unwrap();
    loaded.memory.write_u32_be(element + 4, 0).unwrap();
    loaded.set_heap_cursor(element + 8);
    loaded.cpu.gpr[3] = element;
    loaded.cpu.gpr[4] = state_ptr;
    loaded.set_input_snapshot(PpcInputSnapshot {
        mouse_button: true,
        ..PpcInputSnapshot::default()
    });

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_NO_ERR));
    assert_eq!(
        loaded.memory.read_u32_be(state_ptr),
        Some(PPC_ISP_BUTTON_PRESSED)
    );
}

#[test]
fn hle_import_runner_maps_named_input_sprocket_button_needs() {
    let pef =
        synthetic_pef_with_library_import(b"InputSprocketLib", b"ISpElement_NewVirtualFromNeeds");
    let mut loaded = load_pef_application(&pef).unwrap();
    let needs_ptr = PPC_DATA_BASE + 0x1000;
    let elements_out_ptr = PPC_DATA_BASE + 0x1100;
    let state_ptr = PPC_DATA_BASE + 0x1200;
    loaded
        .memory
        .add_region(needs_ptr, vec![0; PPC_ISP_NEED_SIZE as usize]);
    loaded.memory.add_region(elements_out_ptr, vec![0; 4]);
    loaded.memory.add_region(state_ptr, vec![0; 4]);
    write_ppc_pstring(&mut loaded.memory, needs_ptr, b"Turn Left");
    loaded
        .memory
        .write_u32_be(
            needs_ptr + PPC_ISP_NEED_KIND_OFFSET,
            PPC_ISP_ELEMENT_KIND_BUTTON,
        )
        .unwrap();
    loaded.cpu.gpr[3] = 1;
    loaded.cpu.gpr[4] = needs_ptr;
    loaded.cpu.gpr[5] = elements_out_ptr;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    let element = loaded.memory.read_u32_be(elements_out_ptr).unwrap();
    let mut input = PpcInputSnapshot::default();
    input.key_map[(PPC_KEY_LEFT / 8) as usize] |= 1u8 << (PPC_KEY_LEFT % 8);
    loaded.set_input_snapshot(input);
    loaded.cpu.pc = loaded.entry_pc;
    loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::ISpElementGetSimpleState;
    loaded.cpu.gpr[3] = element;
    loaded.cpu.gpr[4] = state_ptr;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_NO_ERR));
    assert_eq!(
        loaded.memory.read_u32_be(state_ptr),
        Some(PPC_ISP_BUTTON_PRESSED)
    );
}

#[test]
fn hle_import_runner_maps_named_input_sprocket_delta_needs() {
    let pef =
        synthetic_pef_with_library_import(b"InputSprocketLib", b"ISpElement_NewVirtualFromNeeds");
    let mut loaded = load_pef_application(&pef).unwrap();
    let needs_ptr = PPC_DATA_BASE + 0x1000;
    let elements_out_ptr = PPC_DATA_BASE + 0x1100;
    let state_ptr = PPC_DATA_BASE + 0x1200;
    loaded
        .memory
        .add_region(needs_ptr, vec![0; PPC_ISP_NEED_SIZE as usize]);
    loaded.memory.add_region(elements_out_ptr, vec![0; 4]);
    loaded.memory.add_region(state_ptr, vec![0; 4]);
    write_ppc_pstring(&mut loaded.memory, needs_ptr, b"Yaw (Classic Mouse)");
    loaded
        .memory
        .write_u32_be(
            needs_ptr + PPC_ISP_NEED_KIND_OFFSET,
            PPC_ISP_ELEMENT_KIND_DELTA,
        )
        .unwrap();
    loaded.cpu.gpr[3] = 1;
    loaded.cpu.gpr[4] = needs_ptr;
    loaded.cpu.gpr[5] = elements_out_ptr;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    let element = loaded.memory.read_u32_be(elements_out_ptr).unwrap();
    loaded.set_input_snapshot(PpcInputSnapshot {
        mouse_v: (ppc_main_screen_height() / 2) as i16,
        mouse_h: (ppc_main_screen_width() / 2) as i16 + 12,
        ..PpcInputSnapshot::default()
    });
    loaded.cpu.pc = loaded.entry_pc;
    loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::ISpElementGetSimpleState;
    loaded.cpu.gpr[3] = element;
    loaded.cpu.gpr[4] = state_ptr;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_NO_ERR));
    assert_eq!(
        loaded.memory.read_u32_be(state_ptr),
        Some((12i32 * 0x0001_0000) as u32)
    );
}

#[test]
fn input_sprocket_maps_walk_and_climb_axes_independently() {
    let active = PpcInputSprocketState {
        initialized: true,
        suspended: false,
        keyboard_active: true,
        mouse_active: false,
        ..PpcInputSprocketState::default()
    };
    let snapshot = |key: u8| {
        let mut input = PpcInputSnapshot::default();
        input.key_map[(key / 8) as usize] |= 1u8 << (key % 8);
        input
    };
    let walk = ppc_isp_action_binding(PPC_ISP_ELEMENT_KIND_AXIS, Some("Walk"));
    let climb = ppc_isp_action_binding(PPC_ISP_ELEMENT_KIND_AXIS, Some("Climb"));

    assert_eq!(walk, PpcInputSprocketActionBinding::AxisHorizontal);
    assert_eq!(climb, PpcInputSprocketActionBinding::AxisVertical);
    assert_eq!(
        ppc_isp_input_simple_state(
            PPC_ISP_ELEMENT_KIND_AXIS,
            PPC_ISP_AXIS_MIDDLE,
            snapshot(PPC_KEY_RIGHT),
            active,
            walk,
        ),
        PPC_ISP_AXIS_HIGH
    );
    assert_eq!(
        ppc_isp_input_simple_state(
            PPC_ISP_ELEMENT_KIND_AXIS,
            PPC_ISP_AXIS_MIDDLE,
            snapshot(PPC_KEY_RIGHT),
            active,
            climb,
        ),
        PPC_ISP_AXIS_MIDDLE
    );
    assert_eq!(
        ppc_isp_input_simple_state(
            PPC_ISP_ELEMENT_KIND_AXIS,
            PPC_ISP_AXIS_MIDDLE,
            snapshot(PPC_KEY_UP),
            active,
            walk,
        ),
        PPC_ISP_AXIS_MIDDLE
    );
    assert_eq!(
        ppc_isp_input_simple_state(
            PPC_ISP_ELEMENT_KIND_AXIS,
            PPC_ISP_AXIS_MIDDLE,
            snapshot(PPC_KEY_UP),
            active,
            climb,
        ),
        PPC_ISP_AXIS_LOW
    );
}

#[test]
fn input_sprocket_maps_reference_manual_button_names_to_default_keys() {
    let active = PpcInputSprocketState {
        initialized: true,
        suspended: false,
        keyboard_active: true,
        mouse_active: true,
        ..PpcInputSprocketState::default()
    };
    let snapshot = |keys: &[u8]| {
        let mut input = PpcInputSnapshot::default();
        for &key in keys {
            input.key_map[(key / 8) as usize] |= 1u8 << (key % 8);
        }
        input
    };
    for (name, binding, key) in [
        (
            "Jump",
            PpcInputSprocketActionBinding::ButtonJump,
            PPC_KEY_COMMAND,
        ),
        (
            "Fire",
            PpcInputSprocketActionBinding::ButtonFire,
            PPC_KEY_SPACE,
        ),
        (
            "Select Weapon",
            PpcInputSprocketActionBinding::ButtonWeapon,
            PPC_KEY_SHIFT,
        ),
        (
            "Pickup/Throw",
            PpcInputSprocketActionBinding::ButtonPickup,
            PPC_KEY_OPTION,
        ),
        (
            "Jet Up",
            PpcInputSprocketActionBinding::ButtonJetUp,
            PPC_KEY_A,
        ),
        (
            "Jet Down",
            PpcInputSprocketActionBinding::ButtonJetDown,
            PPC_KEY_Z,
        ),
        (
            "Swivel Camera Left",
            PpcInputSprocketActionBinding::ButtonCameraLeft,
            PPC_KEY_COMMA,
        ),
        (
            "Swivel Camera Right",
            PpcInputSprocketActionBinding::ButtonCameraRight,
            PPC_KEY_PERIOD,
        ),
        (
            "Pause",
            PpcInputSprocketActionBinding::ButtonPause,
            PPC_KEY_ESCAPE,
        ),
        (
            "Escape",
            PpcInputSprocketActionBinding::ButtonPause,
            PPC_KEY_ESCAPE,
        ),
        (
            "Return",
            PpcInputSprocketActionBinding::ButtonConfirm,
            PPC_KEY_RETURN,
        ),
        (
            "Zoom In",
            PpcInputSprocketActionBinding::ButtonZoomIn,
            PPC_KEY_1,
        ),
        (
            "Zoom Out",
            PpcInputSprocketActionBinding::ButtonZoomOut,
            PPC_KEY_2,
        ),
        (
            "Change Camera Mode",
            PpcInputSprocketActionBinding::ButtonCameraMode,
            PPC_KEY_TAB,
        ),
        (
            "Raise Volume",
            PpcInputSprocketActionBinding::ButtonVolumeUp,
            PPC_KEY_EQUAL,
        ),
        (
            "Lower Volume",
            PpcInputSprocketActionBinding::ButtonVolumeDown,
            PPC_KEY_MINUS,
        ),
        (
            "Toggle GPS",
            PpcInputSprocketActionBinding::ButtonToggleGps,
            PPC_KEY_G,
        ),
    ] {
        assert_eq!(
            ppc_isp_action_binding(PPC_ISP_ELEMENT_KIND_BUTTON, Some(name)),
            binding
        );
        assert_eq!(
            ppc_isp_input_simple_state(
                PPC_ISP_ELEMENT_KIND_BUTTON,
                0,
                snapshot(&[key]),
                active,
                binding,
            ),
            PPC_ISP_BUTTON_PRESSED,
            "{name} should read its manual default key"
        );
    }
    for (name, binding, keys, solo_key) in [
        (
            "Toggle Music",
            PpcInputSprocketActionBinding::ButtonToggleMusic,
            [PPC_KEY_CONTROL, PPC_KEY_M],
            PPC_KEY_M,
        ),
        (
            "Toggle Ambient Sound",
            PpcInputSprocketActionBinding::ButtonToggleAmbientSound,
            [PPC_KEY_CONTROL, PPC_KEY_B],
            PPC_KEY_B,
        ),
        (
            "Quit Application",
            PpcInputSprocketActionBinding::ButtonQuit,
            [PPC_KEY_COMMAND, PPC_KEY_Q],
            PPC_KEY_Q,
        ),
    ] {
        assert_eq!(
            ppc_isp_action_binding(PPC_ISP_ELEMENT_KIND_BUTTON, Some(name)),
            binding
        );
        assert_eq!(
            ppc_isp_input_simple_state(
                PPC_ISP_ELEMENT_KIND_BUTTON,
                0,
                snapshot(&keys),
                active,
                binding,
            ),
            PPC_ISP_BUTTON_PRESSED,
            "{name} should read its manual default chord"
        );
        assert_eq!(
            ppc_isp_input_simple_state(
                PPC_ISP_ELEMENT_KIND_BUTTON,
                0,
                snapshot(&[solo_key]),
                active,
                binding,
            ),
            0,
            "{name} should require its modifier"
        );
    }
}

#[test]
fn input_sprocket_keeps_jump_fire_and_pickup_distinct() {
    let active = PpcInputSprocketState {
        initialized: true,
        suspended: false,
        keyboard_active: true,
        mouse_active: false,
        ..PpcInputSprocketState::default()
    };
    let mut space = PpcInputSnapshot::default();
    space.key_map[(PPC_KEY_SPACE / 8) as usize] |= 1u8 << (PPC_KEY_SPACE % 8);
    assert_eq!(
        ppc_isp_input_simple_state(
            PPC_ISP_ELEMENT_KIND_BUTTON,
            0,
            space,
            active,
            PpcInputSprocketActionBinding::ButtonFire,
        ),
        PPC_ISP_BUTTON_PRESSED
    );
    assert_eq!(
        ppc_isp_input_simple_state(
            PPC_ISP_ELEMENT_KIND_BUTTON,
            0,
            space,
            active,
            PpcInputSprocketActionBinding::ButtonJump,
        ),
        0
    );
    assert_eq!(
        ppc_isp_input_simple_state(
            PPC_ISP_ELEMENT_KIND_BUTTON,
            0,
            space,
            active,
            PpcInputSprocketActionBinding::ButtonPickup,
        ),
        0
    );
}

#[test]
fn hle_import_runner_tracks_input_sprocket_lifecycle_and_device_activation() {
    let pef = synthetic_pef_with_library_import(b"InputSprocketLib", b"ISpInit");
    let mut loaded = load_pef_application(&pef).unwrap();
    loaded.input_sprocket = PpcInputSprocketState {
        initialized: false,
        suspended: true,
        keyboard_active: false,
        mouse_active: false,
        configure_count: 0,
        virtual_element_count: 0,
        last_virtual_need_count: 0,
        last_virtual_needs_ptr: 0,
        last_virtual_elements_out_ptr: 0,
        ..PpcInputSprocketState::default()
    };

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_NO_ERR));
    assert_eq!(
        loaded.input_sprocket,
        PpcInputSprocketState {
            initialized: true,
            suspended: false,
            keyboard_active: true,
            mouse_active: true,
            configure_count: 0,
            virtual_element_count: 0,
            last_virtual_need_count: 0,
            last_virtual_needs_ptr: 0,
            last_virtual_elements_out_ptr: 0,
            ..PpcInputSprocketState::default()
        }
    );

    loaded.cpu.pc = loaded.entry_pc;
    loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::ISpSuspend;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert!(loaded.input_sprocket.suspended);

    let element = PPC_HEAP_BASE;
    let state_ptr = PPC_DATA_BASE + 0x1000;
    loaded.memory.add_region(element, vec![0; 8]);
    loaded.memory.add_region(state_ptr, vec![0; 4]);
    loaded
        .memory
        .write_u32_be(element, PPC_ISP_ELEMENT_KIND_BUTTON)
        .unwrap();
    loaded.memory.write_u32_be(element + 4, 0).unwrap();
    loaded.set_input_snapshot(PpcInputSnapshot {
        mouse_button: true,
        ..PpcInputSnapshot::default()
    });
    loaded.cpu.pc = loaded.entry_pc;
    loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::ISpElementGetSimpleState;
    loaded.cpu.gpr[3] = element;
    loaded.cpu.gpr[4] = state_ptr;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.memory.read_u32_be(state_ptr), Some(0));

    loaded.cpu.pc = loaded.entry_pc;
    loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::ISpResume;
    loaded.cpu.gpr[3] = 0;
    loaded.cpu.gpr[4] = 0;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert!(!loaded.input_sprocket.suspended);

    loaded.cpu.pc = loaded.entry_pc;
    loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::ISpDevicesDeactivate;
    loaded.cpu.gpr[3] = PPC_ISP_MOUSE_DEVICE;
    loaded.cpu.gpr[4] = 0;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert!(!loaded.input_sprocket.mouse_active);
    assert!(loaded.input_sprocket.keyboard_active);

    loaded.cpu.pc = loaded.entry_pc;
    loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::ISpElementGetSimpleState;
    loaded.cpu.gpr[3] = element;
    loaded.cpu.gpr[4] = state_ptr;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.memory.read_u32_be(state_ptr), Some(0));

    let device_list_ptr = PPC_DATA_BASE + 0x1100;
    loaded.memory.add_region(device_list_ptr, vec![0; 8]);
    loaded
        .memory
        .write_u32_be(device_list_ptr, PPC_ISP_KEYBOARD_DEVICE)
        .unwrap();
    loaded
        .memory
        .write_u32_be(device_list_ptr + 4, PPC_ISP_MOUSE_DEVICE)
        .unwrap();
    loaded.cpu.pc = loaded.entry_pc;
    loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::ISpDevicesActivate;
    loaded.cpu.gpr[3] = 2;
    loaded.cpu.gpr[4] = device_list_ptr;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert!(loaded.input_sprocket.keyboard_active);
    assert!(loaded.input_sprocket.mouse_active);

    loaded
        .memory
        .write_u32_be(device_list_ptr + 4, 0xdead_beef)
        .unwrap();
    loaded.cpu.pc = loaded.entry_pc;
    loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::ISpDevicesDeactivate;
    loaded.cpu.gpr[3] = 2;
    loaded.cpu.gpr[4] = device_list_ptr;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_PARAM_ERR));
    assert!(loaded.input_sprocket.keyboard_active);
    assert!(loaded.input_sprocket.mouse_active);

    loaded.cpu.pc = loaded.entry_pc;
    loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::ISpConfigure;
    loaded.cpu.gpr[3] = 0;
    loaded.cpu.gpr[4] = 0;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.input_sprocket.configure_count, 1);

    loaded.cpu.pc = loaded.entry_pc;
    loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::ISpStop;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert!(!loaded.input_sprocket.initialized);
    assert!(!loaded.input_sprocket.keyboard_active);
    assert!(!loaded.input_sprocket.mouse_active);
}

#[test]
fn sprocket_trace_formatter_includes_input_sprocket_state() {
    let input_sprocket = PpcInputSprocketState {
        initialized: true,
        suspended: false,
        keyboard_active: true,
        mouse_active: false,
        configure_count: 2,
        virtual_element_count: 5,
        last_virtual_need_count: 3,
        last_virtual_needs_ptr: 0x0200_1000,
        last_virtual_elements_out_ptr: 0x0200_2000,
        ..PpcInputSprocketState::default()
    };
    let entry = PpcHleImportTraceEntry {
        import_index: 51,
        library_name: "InputSprocketLib".to_string(),
        symbol_name: "ISpElement_GetSimpleState".to_string(),
        pc: 0x01f0_1100,
        lr: 0x0100_2100,
        rtoc: 0x0200_3100,
        sp: 0x03fe_efc0,
        dispatcher_target: PpcImportDispatcherTarget::ISpElementGetSimpleState,
        repeat_count: 1,
    };

    assert_eq!(
        format_sprocket_trace(
            &entry,
            [0x0200_3000, 0x0200_4000, 0, 0, 0, 0],
            "return-preserve",
            &PpcDrawSprocketState::default(),
            &input_sprocket,
            &[],
        ),
        "[SPROCKET-TRACE] InputSprocketLib:ISpElement_GetSimpleState pc=$01F01100 lr=$01002100 rtoc=$02003100 sp=$03FEEFC0 r3=$02003000 r4=$02004000 r5=$00000000 r6=$00000000 r7=$00000000 r8=$00000000 action=return-preserve isp initialized=true suspended=false keyboard=true mouse=false virtuals=5 last_need_count=3 last_needs=$02001000 last_elements=$02002000 configure_count=2 setl=0 tset=0 keys=0"
    );
}

#[test]
fn sprocket_trace_formatter_includes_input_sprocket_virtual_bindings_on_creation() {
    let input_sprocket = PpcInputSprocketState {
        initialized: true,
        suspended: false,
        keyboard_active: true,
        mouse_active: true,
        configure_count: 0,
        virtual_element_count: 2,
        last_virtual_need_count: 2,
        last_virtual_needs_ptr: 0x0200_1000,
        last_virtual_elements_out_ptr: 0x0200_2000,
        ..PpcInputSprocketState::default()
    };
    let virtual_elements = vec![
        PpcInputSprocketVirtualElementRecord {
            element: 0x0300_0000,
            need_index: 0,
            need_source: 0x0200_1000,
            kind: PPC_ISP_ELEMENT_KIND_BUTTON,
            default_state: 0,
            action_binding: PpcInputSprocketActionBinding::ButtonFire,
            keyboard_binding: None,
            need_name: "Fire".to_string(),
            need_record: Vec::new(),
        },
        PpcInputSprocketVirtualElementRecord {
            element: 0x0300_0100,
            need_index: 1,
            need_source: 0x0200_1000 + PPC_ISP_NEED_SIZE,
            kind: PPC_ISP_ELEMENT_KIND_DELTA,
            default_state: 0,
            action_binding: PpcInputSprocketActionBinding::DeltaYaw,
            keyboard_binding: None,
            need_name: "Yaw (Mouse)".to_string(),
            need_record: Vec::new(),
        },
    ];
    let entry = PpcHleImportTraceEntry {
        import_index: 50,
        library_name: "InputSprocketLib".to_string(),
        symbol_name: "ISpElement_NewVirtualFromNeeds".to_string(),
        pc: 0x01f0_1000,
        lr: 0x0100_2000,
        rtoc: 0x0200_3000,
        sp: 0x03fe_f000,
        dispatcher_target: PpcImportDispatcherTarget::ISpElementNewVirtualFromNeeds,
        repeat_count: 1,
    };

    assert_eq!(
        format_sprocket_trace(
            &entry,
            [2, 0x0200_1000, 0x0200_2000, 0, 0, 0],
            "return($00000000)",
            &PpcDrawSprocketState::default(),
            &input_sprocket,
            &virtual_elements,
        ),
        "[SPROCKET-TRACE] InputSprocketLib:ISpElement_NewVirtualFromNeeds pc=$01F01000 lr=$01002000 rtoc=$02003000 sp=$03FEF000 r3=$00000002 r4=$02001000 r5=$02002000 r6=$00000000 r7=$00000000 r8=$00000000 action=return($00000000) isp initialized=true suspended=false keyboard=true mouse=true virtuals=2 last_need_count=2 last_needs=$02001000 last_elements=$02002000 configure_count=0 setl=0 tset=0 keys=0 last_bindings=[#0 button 'Fire'=button/fire,#1 delta 'Yaw (Mouse)'=delta/yaw]"
    );
}

#[test]
fn import_bindings_classify_input_sprocket_imports() {
    assert_eq!(
        dispatcher_target_for_import("InputSprocketLib", "ISpElement_NewVirtualFromNeeds"),
        PpcImportDispatcherTarget::ISpElementNewVirtualFromNeeds
    );
    assert_eq!(
        dispatcher_target_for_import("InputSprocketLib", "ISpDevices_Extract"),
        PpcImportDispatcherTarget::ISpDevicesExtract
    );
    assert_eq!(
        dispatcher_target_for_import("InputSprocketLib", "ISpDevices_ExtractByClass"),
        PpcImportDispatcherTarget::ISpDevicesExtractByClass
    );
    assert_eq!(
        dispatcher_target_for_import("InputSprocketLib", "ISpDevice_GetElementList"),
        PpcImportDispatcherTarget::ISpDeviceGetElementList
    );
    assert_eq!(
        dispatcher_target_for_import("InputSprocketLib", "ISpElementList_Extract"),
        PpcImportDispatcherTarget::ISpElementListExtract
    );
    assert_eq!(
        dispatcher_target_for_import("InputSprocketLib", "ISpElement_GetInfo"),
        PpcImportDispatcherTarget::ISpElementGetInfo
    );
    assert_eq!(
        dispatcher_target_for_import("InputSprocketLib", "ISpElement_GetSimpleState"),
        PpcImportDispatcherTarget::ISpElementGetSimpleState
    );
    assert_eq!(
        dispatcher_target_for_import("InputSprocketLib", "ISpInit"),
        PpcImportDispatcherTarget::ISpInit
    );
    assert_eq!(
        dispatcher_target_for_import("InputSprocketLib", "ISpStop"),
        PpcImportDispatcherTarget::ISpStop
    );
    assert_eq!(
        dispatcher_target_for_import("InputSprocketLib", "ISpSuspend"),
        PpcImportDispatcherTarget::ISpSuspend
    );
    assert_eq!(
        dispatcher_target_for_import("InputSprocketLib", "ISpResume"),
        PpcImportDispatcherTarget::ISpResume
    );
    assert_eq!(
        dispatcher_target_for_import("InputSprocketLib", "ISpDevices_Activate"),
        PpcImportDispatcherTarget::ISpDevicesActivate
    );
    assert_eq!(
        dispatcher_target_for_import("InputSprocketLib", "ISpDevices_Deactivate"),
        PpcImportDispatcherTarget::ISpDevicesDeactivate
    );
    assert_eq!(
        dispatcher_target_for_import("InputSprocketLib", "ISpConfigure"),
        PpcImportDispatcherTarget::ISpConfigure
    );
}

#[test]
fn import_bindings_classify_input_sprocket_compatibility_imports() {
    for (symbol, operation) in [
        (
            "ISpDevices_ActivateClass",
            PpcInputSprocketCompatibilityOperation::DevicesActivateClass,
        ),
        (
            "ISpDevices_DeactivateClass",
            PpcInputSprocketCompatibilityOperation::DevicesDeactivateClass,
        ),
        (
            "ISpElement_DisposeVirtual",
            PpcInputSprocketCompatibilityOperation::ElementDisposeVirtual,
        ),
        (
            "ISpElement_Flush",
            PpcInputSprocketCompatibilityOperation::ElementFlush,
        ),
        (
            "ISpElement_GetNextEvent",
            PpcInputSprocketCompatibilityOperation::ElementGetNextEvent,
        ),
        ("ISpTickle", PpcInputSprocketCompatibilityOperation::Tickle),
    ] {
        assert_eq!(
            dispatcher_target_for_import("InputSprocketLib", symbol),
            PpcImportDispatcherTarget::InputSprocketCompatibility(operation),
        );
    }
}

#[test]
fn hle_import_runner_dispatches_input_sprocket_compatibility_operations() {
    let pef = synthetic_pef_with_library_import(b"InputSprocketLib", b"ISpDevices_ActivateClass");
    let mut loaded = load_pef_application(&pef).unwrap();
    let probe = loaded.run_with_hle_imports(64);
    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_NO_ERR));
    assert!(loaded.input_sprocket.keyboard_active);
    assert!(loaded.input_sprocket.mouse_active);
}

#[test]
fn input_sprocket_rejects_resource_counts_larger_than_the_data() {
    let mut resource = [0u8; 8];
    resource[4..8].copy_from_slice(&u32::MAX.to_be_bytes());
    assert!(ppc_isp_parse_setl_entries(&resource).is_none());
    assert!(ppc_isp_parse_tset_keycodes(&resource).is_none());
}

#[test]
fn input_sprocket_setl_tset_defaults_assign_keycodes_in_need_order() {
    // Synthetic 'setl': version, count=1, an ISpDeviceDefinition naming the
    // keyboard ('keyd'/'appl'), and a trailing tset ResID 493.
    let entry = PPC_ISP_SETL_HEADER_SIZE as usize;
    let mut setl = vec![0u8; entry + PPC_ISP_SETL_ENTRY_SIZE as usize];
    setl[0..4].copy_from_slice(&2u32.to_be_bytes());
    setl[4..8].copy_from_slice(&1u32.to_be_bytes());
    let class = entry + PPC_ISP_SETL_ENTRY_DEVICE_CLASS_OFFSET as usize;
    let creator = entry + PPC_ISP_SETL_ENTRY_DEVICE_CREATOR_OFFSET as usize;
    let tset_id = entry + PPC_ISP_SETL_ENTRY_TSET_ID_OFFSET as usize;
    setl[class..class + 4].copy_from_slice(b"keyd");
    setl[creator..creator + 4].copy_from_slice(b"appl");
    setl[tset_id..tset_id + 2].copy_from_slice(&493i16.to_be_bytes());

    // Synthetic 'tset': version, count=14, the Deimos keyboard map.
    let keys: [u8; 14] = [
        0x7b, 0x7c, 0x7d, 0x7e, 0x3a, 0x37, 0x31, 0x56, 0x58, 0x57, 0x5b, 0x75, 0x77, 0x79,
    ];
    let mut tset = vec![
        0u8;
        PPC_ISP_TSET_HEADER_SIZE as usize
            + keys.len() * PPC_ISP_TSET_ENTRY_SIZE as usize
    ];
    tset[0..4].copy_from_slice(&1u32.to_be_bytes());
    tset[4..8].copy_from_slice(&(keys.len() as u32).to_be_bytes());
    for (index, key) in keys.iter().copied().enumerate() {
        let base = PPC_ISP_TSET_HEADER_SIZE as usize + index * PPC_ISP_TSET_ENTRY_SIZE as usize;
        tset[base..base + 2].copy_from_slice(&u16::from(key).to_be_bytes());
    }

    let entries = ppc_isp_parse_setl_entries(&setl).unwrap();
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].device_class, PPC_ISP_DEVICE_CLASS_KEYBOARD);
    assert_eq!(entries[0].tset_id, 493);
    assert_eq!(ppc_isp_parse_tset_keycodes(&tset).unwrap(), keys.to_vec());

    let record = |res_type: [u8; 4], res_id: i16, data: Vec<u8>| PpcVfsResourceRecord {
        ref_num: 0,
        path: "synthetic".to_string(),
        res_type: u32::from_be_bytes(res_type),
        res_id,
        name: Vec::new(),
        data: data.into(),
        raw_data: None,
        raw_attrs: None,
        attrs: 0,
        handle: 0,
    };
    let resources = [record(*b"setl", 7, setl), record(*b"tset", 493, tset)];
    let defaults = ppc_isp_load_default_keycodes(&resources, 0, 7).unwrap();
    assert_eq!(defaults.set_list_id, 7);
    assert_eq!(defaults.tset_id, 493);
    assert_eq!(defaults.keycode_count, 14);
    assert_eq!(&defaults.keycodes[..14], &keys);

    // Axis slot order is (minimum, maximum): right key reports AXIS_HIGH.
    let mut input = PpcInputSnapshot::default();
    input.key_map[(keys[1] / 8) as usize] |= 1u8 << (keys[1] % 8);
    let state = ppc_isp_input_simple_state_with_binding(
        PPC_ISP_ELEMENT_KIND_AXIS,
        PPC_ISP_AXIS_MIDDLE,
        input,
        PpcInputSprocketState {
            keyboard_defaults: defaults,
            ..PpcInputSprocketState::default()
        },
        PpcInputSprocketActionBinding::AxisHorizontal,
        Some(PpcInputSprocketKeyboardBinding::axis(keys[0], keys[1])),
    );
    assert_eq!(state, PPC_ISP_AXIS_HIGH);
}

#[test]
fn mouse_device_elements_report_movement_and_button_events() {
    let mut loaded = load_pef_application(&synthetic_pef_with_library_import(
        b"InputSprocketLib",
        b"ISpTickle",
    ))
    .unwrap();
    let scratch = PPC_DATA_BASE + 0x1000;
    let (event, was_event) = (scratch, scratch + 0x40);
    loaded.memory.add_region(scratch, vec![0; 0x80]);
    let mut state = PpcInputSprocketState::default();
    let mut virtual_elements = Vec::new();
    let mut next = |loaded: &mut PpcLoadedApp,
                    state: &mut PpcInputSprocketState,
                    element: u32,
                    input: PpcInputSnapshot| {
        loaded.cpu.gpr[3] = element;
        loaded.cpu.gpr[4] = PPC_ISP_ELEMENT_EVENT_SIZE;
        loaded.cpu.gpr[5] = event;
        loaded.cpu.gpr[6] = was_event;
        let action = dispatch_inputsprocket::ppc_dispatch_input_sprocket_compatibility(
            PpcInputSprocketCompatibilityOperation::ElementGetNextEvent,
            &mut loaded.cpu,
            &mut loaded.memory,
            input,
            1_000_000,
            state,
            &mut virtual_elements,
        );
        assert!(matches!(action, PpcImportAction::Return(0)));
        (loaded.memory.read_u8(was_event) == Some(1))
            .then(|| loaded.memory.read_u32_be(event + 16).unwrap())
    };
    let at = |h: i16, v: i16, button: bool| PpcInputSnapshot {
        mouse_h: h,
        mouse_v: v,
        mouse_button: button,
        ..PpcInputSnapshot::default()
    };

    // The first read establishes the position without reporting movement.
    assert_eq!(
        next(
            &mut loaded,
            &mut state,
            PPC_ISP_MOUSE_X_ELEMENT,
            at(100, 50, false)
        ),
        None
    );
    assert_eq!(
        next(
            &mut loaded,
            &mut state,
            PPC_ISP_MOUSE_Y_ELEMENT,
            at(100, 50, false)
        ),
        None
    );
    assert_eq!(
        next(
            &mut loaded,
            &mut state,
            PPC_ISP_MOUSE_X_ELEMENT,
            at(103, 40, false)
        ),
        Some(3 << 16)
    );
    assert_eq!(
        next(
            &mut loaded,
            &mut state,
            PPC_ISP_MOUSE_Y_ELEMENT,
            at(103, 40, false)
        ),
        Some(10 << 16)
    );
    assert_eq!(loaded.memory.read_u64_be(event), Some(16_625_000_000));
    assert_eq!(
        loaded.memory.read_u32_be(event + 8),
        Some(PPC_ISP_MOUSE_Y_ELEMENT)
    );
    assert_eq!(
        next(
            &mut loaded,
            &mut state,
            PPC_ISP_MOUSE_X_ELEMENT,
            at(103, 40, false)
        ),
        None
    );
    assert_eq!(
        next(
            &mut loaded,
            &mut state,
            PPC_ISP_MOUSE_BUTTON_ELEMENT,
            at(103, 40, true)
        ),
        Some(1)
    );
    assert_eq!(
        next(
            &mut loaded,
            &mut state,
            PPC_ISP_MOUSE_BUTTON_ELEMENT,
            at(103, 40, true)
        ),
        None
    );
    assert_eq!(
        next(
            &mut loaded,
            &mut state,
            PPC_ISP_MOUSE_BUTTON_ELEMENT,
            at(103, 40, false)
        ),
        Some(0)
    );

    // A deactivated mouse reports nothing.
    state.mouse_active = false;
    assert_eq!(
        next(
            &mut loaded,
            &mut state,
            PPC_ISP_MOUSE_BUTTON_ELEMENT,
            at(103, 40, true)
        ),
        None
    );
}

#[test]
fn mouse_device_simple_state_reports_button_and_movement() {
    let mut loaded = load_pef_application(&synthetic_pef_with_library_import(
        b"InputSprocketLib",
        b"ISpTickle",
    ))
    .unwrap();
    let state_ptr = PPC_DATA_BASE + 0x1000;
    loaded.memory.add_region(state_ptr, vec![0xff; 4]);
    let mut state = PpcInputSprocketState::default();
    let pressed = PpcInputSnapshot {
        mouse_button: true,
        ..PpcInputSnapshot::default()
    };
    for (element, input, expected) in [
        (PPC_ISP_MOUSE_BUTTON_ELEMENT, pressed, 1),
        (PPC_ISP_MOUSE_BUTTON_ELEMENT, PpcInputSnapshot::default(), 0),
        // A delta element reports the movement since its last read.
        (PPC_ISP_MOUSE_X_ELEMENT, pressed, 0),
        (
            PPC_ISP_MOUSE_X_ELEMENT,
            PpcInputSnapshot {
                mouse_h: 5,
                ..pressed
            },
            5 << 16,
        ),
        (
            PPC_ISP_MOUSE_X_ELEMENT,
            PpcInputSnapshot {
                mouse_h: 5,
                ..pressed
            },
            0,
        ),
    ] {
        loaded.cpu.gpr[3] = element;
        loaded.cpu.gpr[4] = state_ptr;
        assert_eq!(
            ppc_isp_element_get_simple_state(
                &mut loaded.cpu,
                &mut loaded.memory,
                input,
                &mut state,
                &[],
            ),
            PPC_NO_ERR
        );
        assert_eq!(loaded.memory.read_u32_be(state_ptr), Some(expected));
    }
}

#[test]
fn inputsprocket_uptime_returns_the_virtual_absolute_clock() {
    let target = dispatcher_target_for_import("InputSprocketLib", "ISpUptime");
    assert_eq!(target, PpcImportDispatcherTarget::Microseconds);
    let mut loaded = load_pef_application(&synthetic_pef_with_library_import(
        b"InputSprocketLib",
        b"ISpUptime",
    ))
    .unwrap();
    let time_ptr = PPC_DATA_BASE + 0x1000;
    loaded.memory.add_region(time_ptr, vec![0xaa; 16]);
    loaded.cpu.gpr[3] = time_ptr;
    loaded.set_tick_count(1_000_000);
    loaded.set_clock_cycle_timing(64, 0);
    let probe = loaded.run_with_hle_imports(64);
    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    let absolute = loaded.memory.read_u64_be(time_ptr).unwrap();
    assert_eq!(absolute, ppc_virtual_microseconds(1_000_000, 64, 0, 4));
    assert!(absolute > u64::from(u32::MAX));
    assert_eq!(
        loaded.memory.read_u64_be(time_ptr + 8),
        Some(0xaaaaaaaaaaaaaaaa)
    );
    loaded.cpu.gpr[3..6].copy_from_slice(&[time_ptr + 8, (absolute >> 32) as u32, absolute as u32]);
    let action = dispatch_simple_hot_import_fast(
        &PpcImportDispatcherTarget::AbsoluteToNanoseconds,
        &mut loaded.cpu,
        &mut loaded.memory,
        absolute,
    );
    assert!(matches!(action, Some(PpcImportAction::ReturnPreserve)));
    assert_eq!(
        loaded.memory.read_u64_be(time_ptr + 8),
        Some(absolute * 1_000)
    );
}

#[test]
fn owned_input_sprocket_lists_extract_and_dispose() {
    let pef = synthetic_pef_with_library_import(b"InputSprocketLib", b"ISpElementList_Dispose");
    let mut loaded = load_pef_application(&pef).unwrap();
    let scratch = PPC_DATA_BASE + 0x1000;
    loaded.memory.add_region(scratch, vec![0xaa; 64]);
    loaded
        .memory
        .write_u32_be(scratch, PPC_ISP_MOUSE_X_ELEMENT)
        .unwrap();
    loaded
        .memory
        .write_u32_be(scratch + 4, PPC_ISP_MOUSE_BUTTON_ELEMENT)
        .unwrap();
    loaded.cpu.gpr[3] = 2;
    loaded.cpu.gpr[4] = scratch;
    loaded.cpu.gpr[5] = scratch + 8;
    loaded.cpu.gpr[6] = 0;
    run_test_import(&mut loaded, PpcImportDispatcherTarget::ISpElementListNew);
    assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_NO_ERR));
    let list = loaded.memory.read_u32_be(scratch + 8).unwrap();
    loaded.cpu.gpr[3] = list;
    loaded.cpu.gpr[4] = 1;
    loaded.cpu.gpr[5] = scratch + 12;
    loaded.cpu.gpr[6] = scratch + 16;
    run_test_import(
        &mut loaded,
        PpcImportDispatcherTarget::ISpElementListExtract,
    );
    assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_NO_ERR));
    assert_eq!(loaded.memory.read_u32_be(scratch + 12), Some(2));
    assert_eq!(
        loaded.memory.read_u32_be(scratch + 16),
        Some(PPC_ISP_MOUSE_X_ELEMENT)
    );
    assert_eq!(loaded.memory.read_u32_be(scratch + 20), Some(0xaaaa_aaaa));

    // A zero-capacity query still returns the full count.
    loaded.cpu.gpr[3] = list;
    loaded.cpu.gpr[4] = 0;
    loaded.cpu.gpr[5] = scratch + 12;
    loaded.cpu.gpr[6] = 0;
    run_test_import(
        &mut loaded,
        PpcImportDispatcherTarget::ISpElementListExtract,
    );
    assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_NO_ERR));
    assert_eq!(loaded.memory.read_u32_be(scratch + 12), Some(2));

    loaded.cpu.gpr[3] = list;
    run_test_import(
        &mut loaded,
        PpcImportDispatcherTarget::ISpElementListDispose,
    );
    assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_NO_ERR));
    for reference in [
        list,
        0,
        scratch,
        PPC_ISP_MOUSE_DEVICE,
        PPC_ISP_GLOBAL_ELEMENT_LIST,
    ] {
        loaded.cpu.gpr[3] = reference;
        run_test_import(
            &mut loaded,
            PpcImportDispatcherTarget::ISpElementListDispose,
        );
        assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_PARAM_ERR));
    }
    loaded.cpu.gpr[3] = list;
    loaded.cpu.gpr[4] = 0;
    loaded.cpu.gpr[5] = scratch + 12;
    loaded.cpu.gpr[6] = 0;
    loaded
        .memory
        .write_u32_be(scratch + 12, 0x1234_5678)
        .unwrap();
    run_test_import(
        &mut loaded,
        PpcImportDispatcherTarget::ISpElementListExtract,
    );
    assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_PARAM_ERR));
    assert_eq!(loaded.memory.read_u32_be(scratch + 12), Some(0x1234_5678));

    // Other list operations must reject the disposed reference too.
    loaded.cpu.gpr[3] = list;
    loaded.cpu.gpr[4] = 0;
    loaded.cpu.gpr[5] = 0;
    loaded.cpu.gpr[6] = 0;
    run_test_import(
        &mut loaded,
        PpcImportDispatcherTarget::ISpElementListAddElements,
    );
    assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_PARAM_ERR));
    loaded.cpu.gpr[3] = list;
    run_test_import(&mut loaded, PpcImportDispatcherTarget::ISpElementListFlush);
    assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_PARAM_ERR));
    loaded.cpu.gpr[3] = list;
    loaded.cpu.gpr[4] = 20;
    loaded.cpu.gpr[5] = scratch + 24;
    loaded.cpu.gpr[6] = scratch + 48;
    run_test_import(
        &mut loaded,
        PpcImportDispatcherTarget::ISpElementListGetNextEvent,
    );
    assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_PARAM_ERR));
    assert_eq!(loaded.memory.read_u8(scratch + 48), Some(0xaa));
    assert_eq!(loaded.memory.read_u32_be(scratch + 24), Some(0xaaaa_aaaa));

    // The released allocation is reusable by the next list.
    loaded.cpu.gpr[3] = 2;
    loaded.cpu.gpr[4] = scratch;
    loaded.cpu.gpr[5] = scratch + 8;
    loaded.cpu.gpr[6] = 0;
    run_test_import(&mut loaded, PpcImportDispatcherTarget::ISpElementListNew);
    assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_NO_ERR));
    assert_eq!(loaded.memory.read_u32_be(scratch + 8), Some(list));
}

#[test]
fn inputsprocket_device_activity_queries_follow_class_activation() {
    use PpcInputSprocketCompatibilityOperation as Op;
    let mut loaded = load_pef_application(&synthetic_pef_with_library_import(
        b"InputSprocketLib",
        b"ISpDevice_IsActive",
    ))
    .unwrap();
    let out = PPC_DATA_BASE + 0x1000;
    loaded.memory.add_region(out, vec![0xaa; 16]);
    let call = |loaded: &mut PpcLoadedApp, operation, args: &[u32]| {
        loaded.cpu.gpr[3..3 + args.len()].copy_from_slice(args);
        run_test_import(
            loaded,
            PpcImportDispatcherTarget::InputSprocketCompatibility(operation),
        );
        loaded.cpu.gpr[3]
    };
    for class in [PPC_ISP_DEVICE_CLASS_KEYBOARD, PPC_ISP_DEVICE_CLASS_MOUSE] {
        assert_eq!(call(&mut loaded, Op::DevicesDeactivateClass, &[class]), 0);
    }
    assert_eq!(
        call(
            &mut loaded,
            Op::DevicesActivateClass,
            &[PPC_ISP_DEVICE_CLASS_KEYBOARD]
        ),
        0
    );
    for (device, expected) in [(PPC_ISP_KEYBOARD_DEVICE, 1), (PPC_ISP_MOUSE_DEVICE, 0)] {
        assert_eq!(call(&mut loaded, Op::DeviceIsActive, &[device, out]), 0);
        assert_eq!(loaded.memory.read_u8(out), Some(expected));
        assert_eq!(loaded.memory.read_u8(out + 1), Some(0xaa));
    }
    // A class without a device must not activate unrelated classes.
    assert_eq!(
        call(
            &mut loaded,
            Op::DevicesActivateClass,
            &[u32::from_be_bytes(*b"joys")]
        ),
        0
    );
    assert_eq!(
        call(
            &mut loaded,
            Op::DeviceIsActive,
            &[PPC_ISP_MOUSE_DEVICE, out]
        ),
        0
    );
    assert_eq!(loaded.memory.read_u8(out), Some(0));
    assert_eq!(
        call(
            &mut loaded,
            Op::DevicesActivateClass,
            &[PPC_ISP_DEVICE_CLASS_MOUSE]
        ),
        0
    );
    assert_eq!(
        call(
            &mut loaded,
            Op::DevicesDeactivateClass,
            &[PPC_ISP_DEVICE_CLASS_KEYBOARD]
        ),
        0
    );
    for (device, expected) in [(PPC_ISP_KEYBOARD_DEVICE, 0), (PPC_ISP_MOUSE_DEVICE, 1)] {
        assert_eq!(call(&mut loaded, Op::DeviceIsActive, &[device, out]), 0);
        assert_eq!(loaded.memory.read_u8(out), Some(expected));
    }
    loaded.memory.write_u8(out, 0x5a).unwrap();
    assert_eq!(
        call(&mut loaded, Op::DeviceIsActive, &[0, out]),
        ppc_i16_result(PPC_PARAM_ERR)
    );
    assert_eq!(loaded.memory.read_u8(out), Some(0x5a));
    for invalid in [0, 0xeeeeeeee] {
        assert_eq!(
            call(
                &mut loaded,
                Op::DeviceIsActive,
                &[PPC_ISP_MOUSE_DEVICE, invalid]
            ),
            ppc_i16_result(PPC_PARAM_ERR)
        );
    }
}

#[test]
fn inputsprocket_suspend_masks_raw_mouse_and_resume_drops_inactive_motion() {
    use PpcInputSprocketCompatibilityOperation as Op;
    let mut loaded = load_pef_application(&synthetic_pef_with_library_import(
        b"InputSprocketLib",
        b"ISpDevice_IsActive",
    ))
    .unwrap();
    let base = PPC_DATA_BASE + 0x1000;
    let event = base + 16;
    let was_event = base + 40;
    loaded.memory.add_region(base, vec![0xaa; 64]);
    let call = |loaded: &mut PpcLoadedApp, target, args: &[u32]| {
        loaded.cpu.gpr[3..3 + args.len()].copy_from_slice(args);
        run_test_import(loaded, target);
        loaded.cpu.gpr[3]
    };
    let compat = PpcImportDispatcherTarget::InputSprocketCompatibility;
    let input = |h, button| PpcInputSnapshot {
        mouse_h: h,
        mouse_v: 50,
        mouse_button: button,
        ..PpcInputSnapshot::default()
    };
    // Keep keyboard disabled so Resume must restore the prior choices.
    assert_eq!(
        call(
            &mut loaded,
            compat(Op::DevicesDeactivateClass),
            &[PPC_ISP_DEVICE_CLASS_KEYBOARD]
        ),
        0
    );
    loaded.set_input_snapshot(input(100, false));
    assert_eq!(
        call(
            &mut loaded,
            PpcImportDispatcherTarget::ISpElementGetSimpleState,
            &[PPC_ISP_MOUSE_X_ELEMENT, base]
        ),
        0
    );
    assert_eq!(loaded.memory.read_u32_be(base), Some(0));
    assert_eq!(
        call(&mut loaded, PpcImportDispatcherTarget::ISpSuspend, &[]),
        0
    );
    loaded.set_input_snapshot(input(500, true));
    for device in [PPC_ISP_KEYBOARD_DEVICE, PPC_ISP_MOUSE_DEVICE] {
        assert_eq!(
            call(&mut loaded, compat(Op::DeviceIsActive), &[device, base]),
            0
        );
        assert_eq!(loaded.memory.read_u8(base), Some(0));
    }
    assert_eq!(
        call(
            &mut loaded,
            PpcImportDispatcherTarget::ISpElementGetSimpleState,
            &[PPC_ISP_MOUSE_BUTTON_ELEMENT, base]
        ),
        0
    );
    assert_eq!(loaded.memory.read_u32_be(base), Some(0));
    assert_eq!(
        call(
            &mut loaded,
            compat(Op::ElementGetNextEvent),
            &[
                PPC_ISP_MOUSE_BUTTON_ELEMENT,
                PPC_ISP_ELEMENT_EVENT_SIZE,
                event,
                was_event
            ]
        ),
        0
    );
    assert_eq!(loaded.memory.read_u8(was_event), Some(0));
    assert_eq!(loaded.memory.read_u32_be(event), Some(0xaaaaaaaa));
    assert_eq!(
        call(&mut loaded, PpcImportDispatcherTarget::ISpResume, &[]),
        0
    );
    for (device, expected) in [(PPC_ISP_KEYBOARD_DEVICE, 0), (PPC_ISP_MOUSE_DEVICE, 1)] {
        assert_eq!(
            call(&mut loaded, compat(Op::DeviceIsActive), &[device, base]),
            0
        );
        assert_eq!(loaded.memory.read_u8(base), Some(expected));
    }
    assert_eq!(
        call(
            &mut loaded,
            PpcImportDispatcherTarget::ISpElementGetSimpleState,
            &[PPC_ISP_MOUSE_X_ELEMENT, base]
        ),
        0
    );
    assert_eq!(loaded.memory.read_u32_be(base), Some(0));
    loaded.set_input_snapshot(input(503, true));
    assert_eq!(
        call(
            &mut loaded,
            PpcImportDispatcherTarget::ISpElementGetSimpleState,
            &[PPC_ISP_MOUSE_X_ELEMENT, base]
        ),
        0
    );
    assert_eq!(loaded.memory.read_u32_be(base), Some(3 << 16));
    assert_eq!(
        call(
            &mut loaded,
            compat(Op::ElementGetNextEvent),
            &[
                PPC_ISP_MOUSE_BUTTON_ELEMENT,
                PPC_ISP_ELEMENT_EVENT_SIZE,
                event,
                was_event
            ]
        ),
        0
    );
    assert_eq!(loaded.memory.read_u8(was_event), Some(1));
    assert_eq!(loaded.memory.read_u32_be(event + 16), Some(1));

    assert_eq!(
        call(
            &mut loaded,
            compat(Op::DevicesDeactivateClass),
            &[PPC_ISP_DEVICE_CLASS_MOUSE]
        ),
        0
    );
    loaded.set_input_snapshot(input(900, true));
    assert_eq!(
        call(
            &mut loaded,
            compat(Op::DevicesActivateClass),
            &[PPC_ISP_DEVICE_CLASS_MOUSE]
        ),
        0
    );
    assert_eq!(
        call(
            &mut loaded,
            PpcImportDispatcherTarget::ISpElementGetSimpleState,
            &[PPC_ISP_MOUSE_X_ELEMENT, base]
        ),
        0
    );
    assert_eq!(loaded.memory.read_u32_be(base), Some(0));
}

#[test]
fn input_sprocket_driver_device_and_element_lifetimes() {
    let pef = synthetic_pef_with_library_import(b"InputSprocketLib", b"ISpDevice_New");
    let mut loaded = load_pef_application(&pef).unwrap();
    let scratch = PPC_DATA_BASE + 0x1000;
    loaded.memory.add_region(scratch, vec![0; 256]);
    loaded
        .memory
        .write_u32_be(scratch + 64, u32::from_be_bytes(*b"joys"))
        .unwrap();
    loaded.cpu.gpr[3] = scratch;
    loaded.cpu.gpr[4] = 0;
    loaded.cpu.gpr[5] = 123;
    loaded.cpu.gpr[6] = scratch + 96;
    let probe = loaded.run_with_hle_imports(64);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_NO_ERR));
    let device = loaded.memory.read_u32_be(scratch + 96).unwrap();
    assert_ne!(device, 0);
    let definition = scratch + 112;
    loaded.memory.write_u32_be(definition, device).unwrap();
    loaded
        .memory
        .write_u32_be(definition + 72, PPC_ISP_ELEMENT_KIND_BUTTON)
        .unwrap();
    loaded.memory.write_u32_be(definition + 88, 4).unwrap();
    loaded.cpu.gpr[3] = definition;
    loaded.cpu.gpr[4] = scratch + 220;
    run_test_import(
        &mut loaded,
        PpcImportDispatcherTarget::InputSprocketDriver(PpcInputSprocketDriverOperation::ElementNew),
    );
    assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_NO_ERR));
    let element = loaded.memory.read_u32_be(scratch + 220).unwrap();
    assert_ne!(element, 0);
    assert_ne!(element, device);
    let queries = PPC_DATA_BASE + 0x3000;
    loaded.memory.add_region(queries, vec![0xaa; 256]);
    loaded.cpu.gpr[3] = u32::from_be_bytes(*b"joys");
    loaded.cpu.gpr[4] = 1;
    loaded.cpu.gpr[5] = queries;
    loaded.cpu.gpr[6] = queries + 4;
    run_test_import(
        &mut loaded,
        PpcImportDispatcherTarget::ISpDevicesExtractByClass,
    );
    assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_NO_ERR));
    assert_eq!(loaded.memory.read_u32_be(queries), Some(1));
    assert_eq!(loaded.memory.read_u32_be(queries + 4), Some(device));
    assert_eq!(loaded.memory.read_u32_be(queries + 8), Some(0xaaaa_aaaa));
    loaded.cpu.gpr[3] = device;
    loaded.cpu.gpr[4] = 92;
    loaded.cpu.gpr[5] = queries + 16;
    run_test_import(
        &mut loaded,
        PpcImportDispatcherTarget::ISpDeviceGetDefinition,
    );
    assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_NO_ERR));
    assert_eq!(
        loaded.memory.read_u32_be(queries + 80),
        Some(u32::from_be_bytes(*b"joys"))
    );
    loaded.cpu.gpr[3] = element;
    loaded.cpu.gpr[4] = queries + 112;
    run_test_import(&mut loaded, PpcImportDispatcherTarget::ISpElementGetInfo);
    assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_NO_ERR));
    assert_eq!(
        loaded.memory.read_u32_be(queries + 116),
        Some(PPC_ISP_ELEMENT_KIND_BUTTON)
    );
    loaded.cpu.gpr[3] = device;
    loaded.cpu.gpr[4] = queries;
    run_test_import(
        &mut loaded,
        PpcImportDispatcherTarget::ISpDeviceGetElementList,
    );
    assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_NO_ERR));
    loaded.cpu.gpr[3] = loaded.memory.read_u32_be(queries).unwrap();
    loaded.cpu.gpr[4] = 1;
    loaded.cpu.gpr[5] = queries;
    loaded.cpu.gpr[6] = queries + 4;
    run_test_import(
        &mut loaded,
        PpcImportDispatcherTarget::ISpElementListExtract,
    );
    assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_NO_ERR));
    assert_eq!(loaded.memory.read_u32_be(queries), Some(1));
    assert_eq!(loaded.memory.read_u32_be(queries + 4), Some(element));

    loaded.memory.write_u32_be(scratch + 240, device).unwrap();
    loaded.cpu.gpr[3] = 1;
    loaded.cpu.gpr[4] = scratch + 240;
    run_test_import(&mut loaded, PpcImportDispatcherTarget::ISpDevicesActivate);
    assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_NO_ERR));
    loaded.memory.write_u32_be(scratch + 224, 1).unwrap();
    loaded.memory.write_u32_be(scratch + 228, 42).unwrap();
    loaded.cpu.gpr[3] = 0;
    loaded.cpu.gpr[4] = 0;
    loaded.cpu.gpr[5] = queries;
    loaded.cpu.gpr[6] = 0;
    run_test_import(&mut loaded, PpcImportDispatcherTarget::ISpElementListNew);
    assert_eq!(loaded.cpu.gpr[3], 0);
    let list = loaded.memory.read_u32_be(queries).unwrap();
    loaded.cpu.gpr[3] = list;
    loaded.cpu.gpr[4] = 0x1234;
    loaded.cpu.gpr[5] = 1;
    loaded.cpu.gpr[6] = scratch + 220;
    run_test_import(
        &mut loaded,
        PpcImportDispatcherTarget::ISpElementListAddElements,
    );
    assert_eq!(loaded.cpu.gpr[3], 0);
    for value in [7, 9] {
        loaded.cpu.gpr[3] = element;
        loaded.cpu.gpr[4] = value;
        loaded.cpu.gpr[5] = scratch + 224;
        run_test_import(
            &mut loaded,
            PpcImportDispatcherTarget::InputSprocketDriver(
                PpcInputSprocketDriverOperation::ElementPushSimpleData,
            ),
        );
        assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_NO_ERR));
    }
    loaded.cpu.gpr[3] = element;
    loaded.cpu.gpr[4] = scratch + 232;
    run_test_import(
        &mut loaded,
        PpcImportDispatcherTarget::ISpElementGetSimpleState,
    );
    assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_NO_ERR));
    assert_eq!(loaded.memory.read_u32_be(scratch + 232), Some(9));
    let event_output = PPC_DATA_BASE + 0x2000;
    loaded.memory.add_region(event_output, vec![0xaa; 32]);
    for value in [7, 9] {
        loaded.cpu.gpr[3] = element;
        loaded.cpu.gpr[4] = 20;
        loaded.cpu.gpr[5] = event_output;
        loaded.cpu.gpr[6] = event_output + 24;
        run_test_import(
            &mut loaded,
            PpcImportDispatcherTarget::InputSprocketCompatibility(
                PpcInputSprocketCompatibilityOperation::ElementGetNextEvent,
            ),
        );
        assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_NO_ERR));
        assert_eq!(loaded.memory.read_u8(event_output + 24), Some(1));
        assert_eq!(loaded.memory.read_u32_be(event_output), Some(1));
        assert_eq!(loaded.memory.read_u32_be(event_output + 4), Some(42));
        assert_eq!(loaded.memory.read_u32_be(event_output + 8), Some(element));
        assert_eq!(loaded.memory.read_u32_be(event_output + 16), Some(value));
        assert_eq!(
            loaded.memory.read_u32_be(event_output + 20),
            Some(0xaaaa_aaaa)
        );
    }
    // Each list still owns both events after the individual queue is consumed.
    for (reference, ref_con) in [
        (list, 0x1234),
        (device, 0),
        (PPC_ISP_GLOBAL_ELEMENT_LIST, 0),
    ] {
        for value in [7, 9] {
            loaded.cpu.gpr[3] = reference;
            loaded.cpu.gpr[4] = 20;
            loaded.cpu.gpr[5] = event_output;
            loaded.cpu.gpr[6] = event_output + 24;
            run_test_import(
                &mut loaded,
                PpcImportDispatcherTarget::ISpElementListGetNextEvent,
            );
            assert_eq!(loaded.cpu.gpr[3], 0);
            assert_eq!(loaded.memory.read_u8(event_output + 24), Some(1));
            assert_eq!(loaded.memory.read_u32_be(event_output + 8), Some(element));
            assert_eq!(loaded.memory.read_u32_be(event_output + 12), Some(ref_con));
            assert_eq!(loaded.memory.read_u32_be(event_output + 16), Some(value));
        }
    }
    let old_limit = loaded.heap_limit();
    loaded.set_heap_limit(loaded.heap_cursor() + 257);
    let mut exhausted = false;
    let mut queued = 0;
    for value in 0..16 {
        loaded.cpu.gpr[3] = element;
        loaded.cpu.gpr[4] = value;
        loaded.cpu.gpr[5] = scratch + 224;
        run_test_import(
            &mut loaded,
            PpcImportDispatcherTarget::InputSprocketDriver(
                PpcInputSprocketDriverOperation::ElementPushSimpleData,
            ),
        );
        if loaded.cpu.gpr[3] == ppc_i16_result(PPC_MEM_FULL_ERR) {
            exhausted = true;
            break;
        }
        assert_eq!(loaded.cpu.gpr[3], 0);
        queued += 1;
    }
    assert!(
        queued > 0,
        "the fixture must allow events before exhaustion"
    );
    assert!(exhausted, "unread events must consume the guest heap");
    loaded.cpu.gpr[3] = element;
    loaded.cpu.gpr[4] = scratch + 232;
    run_test_import(
        &mut loaded,
        PpcImportDispatcherTarget::ISpElementGetSimpleState,
    );
    assert_eq!(loaded.memory.read_u32_be(scratch + 232), Some(queued - 1));
    // The rejected push changes neither state nor any recipient queue.
    for reference in [list, device, PPC_ISP_GLOBAL_ELEMENT_LIST] {
        for index in 0..=queued {
            loaded.cpu.gpr[3] = reference;
            loaded.cpu.gpr[4] = 20;
            loaded.cpu.gpr[5] = event_output;
            loaded.cpu.gpr[6] = event_output + 24;
            run_test_import(
                &mut loaded,
                PpcImportDispatcherTarget::ISpElementListGetNextEvent,
            );
            assert_eq!(loaded.cpu.gpr[3], 0);
            assert_eq!(
                loaded.memory.read_u8(event_output + 24),
                Some(u8::from(index < queued))
            );
            if index < queued {
                assert_eq!(loaded.memory.read_u32_be(event_output + 16), Some(index));
            }
        }
    }
    loaded.cpu.gpr[3] = element;
    run_test_import(
        &mut loaded,
        PpcImportDispatcherTarget::InputSprocketCompatibility(
            PpcInputSprocketCompatibilityOperation::ElementFlush,
        ),
    );
    assert_eq!(loaded.cpu.gpr[3], 0);
    for reference in [list, device, PPC_ISP_GLOBAL_ELEMENT_LIST] {
        loaded.cpu.gpr[3] = reference;
        run_test_import(&mut loaded, PpcImportDispatcherTarget::ISpElementListFlush);
        assert_eq!(loaded.cpu.gpr[3], 0);
    }
    loaded.cpu.gpr[3] = element;
    loaded.cpu.gpr[4] = 77;
    loaded.cpu.gpr[5] = scratch + 224;
    run_test_import(
        &mut loaded,
        PpcImportDispatcherTarget::InputSprocketDriver(
            PpcInputSprocketDriverOperation::ElementPushSimpleData,
        ),
    );
    assert_eq!(loaded.cpu.gpr[3], 0, "flushing releases event storage");
    // Flushing only an owned list leaves the device subscription intact.
    loaded.cpu.gpr[3] = list;
    run_test_import(&mut loaded, PpcImportDispatcherTarget::ISpElementListFlush);
    for (reference, expected) in [(list, 0), (device, 1)] {
        loaded.cpu.gpr[3] = reference;
        loaded.cpu.gpr[4] = 20;
        loaded.cpu.gpr[5] = event_output;
        loaded.cpu.gpr[6] = event_output + 24;
        run_test_import(
            &mut loaded,
            PpcImportDispatcherTarget::ISpElementListGetNextEvent,
        );
        assert_eq!(loaded.cpu.gpr[3], 0);
        assert_eq!(loaded.memory.read_u8(event_output + 24), Some(expected));
    }
    loaded.cpu.gpr[3] = list;
    run_test_import(
        &mut loaded,
        PpcImportDispatcherTarget::ISpElementListDispose,
    );
    assert_eq!(loaded.cpu.gpr[3], 0);
    loaded.set_heap_limit(old_limit);
    // Device disposal removes every occurrence while preserving other members.
    loaded.memory.write_u32_be(queries + 160, element).unwrap();
    loaded
        .memory
        .write_u32_be(queries + 164, PPC_ISP_MOUSE_BUTTON_ELEMENT)
        .unwrap();
    loaded.memory.write_u32_be(queries + 168, element).unwrap();
    loaded.cpu.gpr[3] = 3;
    loaded.cpu.gpr[4] = queries + 160;
    loaded.cpu.gpr[5] = queries + 172;
    loaded.cpu.gpr[6] = 0;
    run_test_import(&mut loaded, PpcImportDispatcherTarget::ISpElementListNew);
    assert_eq!(loaded.cpu.gpr[3], 0);
    let surviving_list = loaded.memory.read_u32_be(queries + 172).unwrap();
    loaded.cpu.gpr[3] = device;
    run_test_import(
        &mut loaded,
        PpcImportDispatcherTarget::InputSprocketDriver(
            PpcInputSprocketDriverOperation::DeviceDispose,
        ),
    );
    assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_NO_ERR));
    loaded.cpu.gpr[3] = surviving_list;
    loaded.cpu.gpr[4] = 3;
    loaded.cpu.gpr[5] = queries + 176;
    loaded.cpu.gpr[6] = queries + 180;
    run_test_import(
        &mut loaded,
        PpcImportDispatcherTarget::ISpElementListExtract,
    );
    assert_eq!(loaded.cpu.gpr[3], 0);
    assert_eq!(loaded.memory.read_u32_be(queries + 176), Some(1));
    assert_eq!(
        loaded.memory.read_u32_be(queries + 180),
        Some(PPC_ISP_MOUSE_BUTTON_ELEMENT)
    );
    loaded.cpu.gpr[3] = element;
    run_test_import(
        &mut loaded,
        PpcImportDispatcherTarget::InputSprocketDriver(
            PpcInputSprocketDriverOperation::ElementDispose,
        ),
    );
    assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_PARAM_ERR));
    loaded.cpu.gpr[3] = definition;
    loaded.cpu.gpr[4] = scratch + 220;
    run_test_import(
        &mut loaded,
        PpcImportDispatcherTarget::InputSprocketDriver(PpcInputSprocketDriverOperation::ElementNew),
    );
    assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_PARAM_ERR));
}

#[test]
fn input_sprocket_driver_activation_calls_guest_metahandler_and_propagates_errors() {
    let pef = synthetic_pef_with_library_import(b"InputSprocketLib", b"ISpDevice_New");
    let mut loaded = load_pef_application(&pef).unwrap();
    let scratch = PPC_DATA_BASE + 0x1000;
    loaded.memory.add_region(scratch, vec![0; 512]);
    let context = scratch + 128;
    let meta = scratch + 256;
    let callback = scratch + 288;
    let meta_vector = scratch + 320;
    let callback_vector = scratch + 328;
    // Metahandler: record selector, return the stored callback TVector.
    for (index, word) in [0x9083_0000, 0x8063_0004, BLR].into_iter().enumerate() {
        loaded
            .memory
            .write_u32_be(meta + index as u32 * 4, word)
            .unwrap();
    }
    // SetActive: record Boolean, return the configured OSStatus.
    for (index, word) in [0x9083_0008, 0x8063_000c, BLR].into_iter().enumerate() {
        loaded
            .memory
            .write_u32_be(callback + index as u32 * 4, word)
            .unwrap();
    }
    for (vector, entry) in [(meta_vector, meta), (callback_vector, callback)] {
        loaded.memory.write_u32_be(vector, entry).unwrap();
        loaded
            .memory
            .write_u32_be(vector + 4, loaded.cpu.gpr[2])
            .unwrap();
    }
    loaded
        .memory
        .write_u32_be(context + 4, callback_vector)
        .unwrap();
    loaded.cpu.gpr[3] = scratch;
    loaded.cpu.gpr[4] = meta_vector;
    loaded.cpu.gpr[5] = context;
    loaded.cpu.gpr[6] = scratch + 96;
    run_test_import(
        &mut loaded,
        PpcImportDispatcherTarget::InputSprocketDriver(PpcInputSprocketDriverOperation::DeviceNew),
    );
    assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_NO_ERR));
    let device = loaded.memory.read_u32_be(scratch + 96).unwrap();
    loaded.memory.write_u32_be(scratch + 100, device).unwrap();
    let element_definition = scratch + 352;
    loaded
        .memory
        .write_u32_be(element_definition, device)
        .unwrap();
    loaded
        .memory
        .write_u32_be(element_definition + 88, 4)
        .unwrap();
    loaded.cpu.gpr[3] = element_definition;
    loaded.cpu.gpr[4] = scratch + 460;
    run_test_import(
        &mut loaded,
        PpcImportDispatcherTarget::InputSprocketDriver(PpcInputSprocketDriverOperation::ElementNew),
    );
    assert_eq!(loaded.cpu.gpr[3], 0);
    let element = loaded.memory.read_u32_be(scratch + 460).unwrap();
    let needs = PPC_DATA_BASE + 0x4000;
    loaded.memory.add_region(needs, vec![0; 128]);
    // Init records count, needs, virtual array, used array, and creator codes.
    for (index, word) in [
        0x9083_0008,
        0x90a3_0014,
        0x90c3_0018,
        0x90e3_001c,
        0x9103_0020,
        0x9123_0024,
        0x8063_000c,
        BLR,
    ]
    .into_iter()
    .enumerate()
    {
        loaded
            .memory
            .write_u32_be(callback + index as u32 * 4, word)
            .unwrap();
    }
    let before_init = loaded.heap_cursor();
    for error in [0, ppc_i16_result(PPC_PARAM_ERR)] {
        loaded.memory.write_u32_be(context + 12, error).unwrap();
        loaded.cpu.pc = loaded.entry_pc;
        loaded.cpu.lr = PPC_HALT_PC;
        loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::ISpInit;
        loaded.cpu.gpr[3] = 1;
        loaded.cpu.gpr[4] = needs;
        loaded.cpu.gpr[5] = needs + 96;
        loaded.cpu.gpr[6] = 0x1234_5678;
        loaded.cpu.gpr[7] = 0xaabb_ccdd;
        loaded.cpu.gpr[8] = 0;
        loaded.cpu.gpr[9] = 0;
        loaded.cpu.gpr[10] = 0;
        let probe = loaded.run_with_hle_imports(256);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(loaded.cpu.gpr[3], error);
        assert_eq!(loaded.memory.read_u32_be(context), Some(1));
        assert_eq!(loaded.memory.read_u32_be(context + 8), Some(1));
        assert_eq!(loaded.memory.read_u32_be(context + 20), Some(needs));
        assert_eq!(loaded.memory.read_u32_be(context + 24), Some(needs + 96));
        assert_ne!(loaded.memory.read_u32_be(context + 28), Some(0));
        assert_eq!(loaded.memory.read_u32_be(context + 32), Some(0x1234_5678));
        assert_eq!(loaded.memory.read_u32_be(context + 36), Some(0xaabb_ccdd));
        assert_eq!(
            loaded.heap_cursor(),
            before_init,
            "Init scratch must be released on both exits"
        );
    }
    for (index, word) in [0x9083_0008, 0x8063_000c, BLR].into_iter().enumerate() {
        loaded
            .memory
            .write_u32_be(callback + index as u32 * 4, word)
            .unwrap();
    }
    let original_rtoc = loaded.cpu.gpr[2];
    for (active, error) in [
        (true, 0),
        (false, ppc_i16_result(PPC_PARAM_ERR)),
        (false, 0),
    ] {
        if error != 0 {
            loaded.cpu.gpr[3] = element;
            loaded.cpu.gpr[4] = 33;
            loaded.cpu.gpr[5] = scratch + 464;
            run_test_import(
                &mut loaded,
                PpcImportDispatcherTarget::InputSprocketDriver(
                    PpcInputSprocketDriverOperation::ElementPushSimpleData,
                ),
            );
            assert_eq!(loaded.cpu.gpr[3], 0);
        }
        loaded.memory.write_u32_be(context + 12, error).unwrap();
        loaded.cpu.pc = loaded.entry_pc;
        loaded.cpu.lr = PPC_HALT_PC;
        loaded.imports[0].dispatcher_target = if active {
            PpcImportDispatcherTarget::ISpDevicesActivate
        } else {
            PpcImportDispatcherTarget::ISpDevicesDeactivate
        };
        loaded.cpu.gpr[3] = 1;
        loaded.cpu.gpr[4] = scratch + 100;
        let probe = loaded.run_with_hle_imports(256);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(loaded.cpu.gpr[3], error);
        assert_eq!(loaded.cpu.gpr[2], original_rtoc);
        assert_eq!(loaded.memory.read_u32_be(context), Some(13));
        assert_eq!(
            loaded.memory.read_u32_be(context + 8),
            Some(u32::from(active))
        );
        loaded.cpu.gpr[3] = device;
        loaded.cpu.gpr[4] = scratch + 104;
        run_test_import(
            &mut loaded,
            PpcImportDispatcherTarget::InputSprocketCompatibility(
                PpcInputSprocketCompatibilityOperation::DeviceIsActive,
            ),
        );
        assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_NO_ERR));
        assert_eq!(
            loaded.memory.read_u8(scratch + 104),
            Some(u8::from(active || error != 0))
        );
        if error != 0 {
            loaded.cpu.gpr[3] = element;
            loaded.cpu.gpr[4] = 20;
            loaded.cpu.gpr[5] = scratch + 480;
            loaded.cpu.gpr[6] = scratch + 504;
            run_test_import(
                &mut loaded,
                PpcImportDispatcherTarget::InputSprocketCompatibility(
                    PpcInputSprocketCompatibilityOperation::ElementGetNextEvent,
                ),
            );
            assert_eq!(loaded.cpu.gpr[3], 0);
            assert_eq!(loaded.memory.read_u8(scratch + 504), Some(1));
            assert_eq!(loaded.memory.read_u32_be(scratch + 496), Some(33));
        }
        if active {
            loaded.memory.write_u32_be(context, 0).unwrap();
            loaded.cpu.pc = loaded.entry_pc;
            loaded.cpu.lr = PPC_HALT_PC;
            loaded.imports[0].dispatcher_target =
                PpcImportDispatcherTarget::InputSprocketCompatibility(
                    PpcInputSprocketCompatibilityOperation::Tickle,
                );
            let probe = loaded.run_with_hle_imports(256);
            assert_eq!(probe.unsupported_import_index, None);
            assert_eq!(loaded.cpu.gpr[3], 0);
            assert_eq!(loaded.memory.read_u32_be(context), Some(15));
            for (target, expected) in [
                (PpcImportDispatcherTarget::ISpSuspend, false),
                (PpcImportDispatcherTarget::ISpResume, true),
            ] {
                loaded.cpu.pc = loaded.entry_pc;
                loaded.cpu.lr = PPC_HALT_PC;
                loaded.imports[0].dispatcher_target = target;
                let probe = loaded.run_with_hle_imports(256);
                assert_eq!(probe.unsupported_import_index, None);
                assert_eq!(loaded.cpu.gpr[3], 0);
                assert_eq!(
                    loaded.memory.read_u32_be(context + 8),
                    Some(u32::from(expected))
                );
                loaded.cpu.gpr[3] = device;
                loaded.cpu.gpr[4] = scratch + 104;
                run_test_import(
                    &mut loaded,
                    PpcImportDispatcherTarget::InputSprocketCompatibility(
                        PpcInputSprocketCompatibilityOperation::DeviceIsActive,
                    ),
                );
                assert_eq!(
                    loaded.memory.read_u8(scratch + 104),
                    Some(u8::from(expected))
                );
            }
        }
    }
    loaded.memory.write_u32_be(context, 0).unwrap();
    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::ISpStop;
    let probe = loaded.run_with_hle_imports(256);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], 0);
    assert_eq!(loaded.memory.read_u32_be(context), Some(2));
}

#[test]
fn input_sprocket_complex_state_shares_mouse_delta_and_validates_output_first() {
    let pef = synthetic_pef_with_library_import(b"InputSprocketLib", b"ISpElement_GetComplexState");
    let mut loaded = load_pef_application(&pef).unwrap();
    let output = PPC_DATA_BASE + 0x1000;
    loaded.memory.add_region(output, vec![0xaa; 16]);
    let mut input = PpcInputSnapshot::default();
    input.mouse_h = 100;
    loaded.set_input_snapshot(input);
    loaded.cpu.gpr[3] = PPC_ISP_MOUSE_X_ELEMENT;
    loaded.cpu.gpr[4] = 4;
    loaded.cpu.gpr[5] = output;
    run_test_import(
        &mut loaded,
        PpcImportDispatcherTarget::InputSprocketDriver(
            PpcInputSprocketDriverOperation::ElementGetComplexState,
        ),
    );
    assert_eq!(loaded.cpu.gpr[3], 0);
    assert_eq!(loaded.memory.read_u32_be(output), Some(0));

    input.mouse_h = 103;
    loaded.set_input_snapshot(input);
    loaded.cpu.gpr[3] = PPC_ISP_MOUSE_X_ELEMENT;
    loaded.cpu.gpr[4] = 4;
    loaded.cpu.gpr[5] = 0;
    run_test_import(
        &mut loaded,
        PpcImportDispatcherTarget::InputSprocketDriver(
            PpcInputSprocketDriverOperation::ElementGetComplexState,
        ),
    );
    assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_PARAM_ERR));
    loaded.cpu.gpr[3] = PPC_ISP_MOUSE_X_ELEMENT;
    loaded.cpu.gpr[4] = output;
    run_test_import(
        &mut loaded,
        PpcImportDispatcherTarget::ISpElementGetSimpleState,
    );
    assert_eq!(loaded.cpu.gpr[3], 0);
    assert_eq!(loaded.memory.read_u32_be(output), Some(3 << 16));

    input.mouse_h = 107;
    loaded.set_input_snapshot(input);
    loaded.cpu.gpr[3] = PPC_ISP_MOUSE_X_ELEMENT;
    loaded.cpu.gpr[4] = 2;
    loaded.cpu.gpr[5] = output + 4;
    run_test_import(
        &mut loaded,
        PpcImportDispatcherTarget::InputSprocketDriver(
            PpcInputSprocketDriverOperation::ElementGetComplexState,
        ),
    );
    assert_eq!(loaded.cpu.gpr[3], 0);
    assert_eq!(loaded.memory.read_u16_be(output + 4), Some(4));
    assert_eq!(loaded.memory.read_u16_be(output + 6), Some(0xaaaa));
}

#[test]
fn input_sprocket_driver_complex_payloads_truncate_state_and_dequeue_short_events() {
    let pef = synthetic_pef_with_library_import(b"InputSprocketLib", b"ISpDevice_New");
    let mut loaded = load_pef_application(&pef).unwrap();
    let scratch = PPC_DATA_BASE + 0x1000;
    let output = PPC_DATA_BASE + 0x2000;
    loaded.memory.add_region(scratch, vec![0; 256]);
    loaded.memory.add_region(output, vec![0xaa; 64]);
    loaded.cpu.gpr[3] = scratch;
    loaded.cpu.gpr[4] = 0;
    loaded.cpu.gpr[5] = 0;
    loaded.cpu.gpr[6] = scratch + 96;
    run_test_import(
        &mut loaded,
        PpcImportDispatcherTarget::InputSprocketDriver(PpcInputSprocketDriverOperation::DeviceNew),
    );
    let device = loaded.memory.read_u32_be(scratch + 96).unwrap();
    loaded.cpu.gpr[3] = 1;
    loaded.cpu.gpr[4] = scratch + 96;
    run_test_import(&mut loaded, PpcImportDispatcherTarget::ISpDevicesActivate);
    assert_eq!(loaded.cpu.gpr[3], 0);
    loaded.memory.write_u32_be(scratch + 112, device).unwrap();
    loaded.memory.write_u32_be(scratch + 200, 8).unwrap();
    loaded.cpu.gpr[3] = scratch + 112;
    loaded.cpu.gpr[4] = scratch + 220;
    run_test_import(
        &mut loaded,
        PpcImportDispatcherTarget::InputSprocketDriver(PpcInputSprocketDriverOperation::ElementNew),
    );
    assert_eq!(loaded.cpu.gpr[3], 0);
    let element = loaded.memory.read_u32_be(scratch + 220).unwrap();
    loaded.memory.write_u32_be(scratch + 224, 1).unwrap();
    loaded.memory.write_u32_be(scratch + 228, 2).unwrap();
    loaded
        .memory
        .write_u32_be(scratch + 232, 0x1122_3344)
        .unwrap();
    loaded
        .memory
        .write_u32_be(scratch + 236, 0x5566_7788)
        .unwrap();
    for size in [4, 8] {
        loaded.cpu.gpr[3] = element;
        loaded.cpu.gpr[4] = size;
        loaded.cpu.gpr[5] = scratch + 232;
        loaded.cpu.gpr[6] = scratch + 224;
        run_test_import(
            &mut loaded,
            PpcImportDispatcherTarget::InputSprocketDriver(
                PpcInputSprocketDriverOperation::ElementPushComplexData,
            ),
        );
        assert_eq!(
            loaded.cpu.gpr[3],
            if size == 8 {
                0
            } else {
                ppc_i16_result(PPC_PARAM_ERR)
            }
        );
    }
    loaded.cpu.gpr[3] = element;
    loaded.cpu.gpr[4] = 3;
    loaded.cpu.gpr[5] = output;
    run_test_import(
        &mut loaded,
        PpcImportDispatcherTarget::InputSprocketDriver(
            PpcInputSprocketDriverOperation::ElementGetComplexState,
        ),
    );
    assert_eq!(loaded.cpu.gpr[3], 0);
    assert_eq!(loaded.memory.read_u32_be(output), Some(0x1122_33aa));
    loaded.cpu.gpr[3] = element;
    loaded.cpu.gpr[4] = 19;
    loaded.cpu.gpr[5] = output + 8;
    loaded.cpu.gpr[6] = output + 40;
    run_test_import(
        &mut loaded,
        PpcImportDispatcherTarget::InputSprocketCompatibility(
            PpcInputSprocketCompatibilityOperation::ElementGetNextEvent,
        ),
    );
    assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_PARAM_ERR));
    assert_eq!(loaded.memory.read_u8(output + 40), Some(1));
    assert_eq!(loaded.memory.read_u32_be(output + 8), Some(1));
    assert_eq!(loaded.memory.read_u32_be(output + 12), Some(2));
    assert_eq!(loaded.memory.read_u32_be(output + 24), Some(0x1122_33aa));
    loaded.cpu.gpr[3] = element;
    run_test_import(
        &mut loaded,
        PpcImportDispatcherTarget::InputSprocketCompatibility(
            PpcInputSprocketCompatibilityOperation::ElementGetNextEvent,
        ),
    );
    assert_eq!(loaded.cpu.gpr[3], 0);
    assert_eq!(loaded.memory.read_u8(output + 40), Some(0));
}
