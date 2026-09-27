use super::*;

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
        assert_eq!(loaded.memory.read_u32_be(version_ptr), Some(0x0170_8000));
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
        let pef = synthetic_pef_with_library_import(
            b"InputSprocketLib",
            b"ISpElementList_AddElements",
        );
        let mut loaded = load_pef_application(&pef).unwrap();
        let list = PPC_DATA_BASE + 0x1000;
        let elements_ptr = PPC_DATA_BASE + 0x1100;
        loaded
            .memory
            .add_region(list, vec![0; PPC_ISP_ELEMENT_LIST_RECORD_SIZE as usize]);
        loaded.memory.add_region(elements_ptr, vec![0; 8]);
        loaded.memory.write_u32_be(list, 2).unwrap();
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
        loaded.cpu.gpr[3] = PPC_HEAP_BASE;
        loaded.cpu.gpr[6] = was_event_ptr;

        let probe = loaded.run_with_hle_imports(64);

        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_NO_ERR));
        assert_eq!(loaded.memory.read_u8(was_event_ptr), Some(0));
    }

    #[test]
    fn input_sprocket_element_list_delivers_button_press_and_release_once() {
        let pef = synthetic_pef_with_library_import(b"InputSprocketLib", b"ISpGetVersion");
        let mut loaded = load_pef_application(&pef).unwrap();
        let list = PPC_DATA_BASE + 0x1000;
        let event_ptr = PPC_DATA_BASE + 0x2000;
        let was_event_ptr = PPC_DATA_BASE + 0x2100;
        let element = 0x0300_1000;
        loaded
            .memory
            .add_region(list, vec![0; PPC_ISP_ELEMENT_LIST_RECORD_SIZE as usize]);
        loaded.memory.add_region(event_ptr, vec![0; 20]);
        loaded.memory.add_region(was_event_ptr, vec![0; 1]);
        loaded.memory.write_u32_be(list, 1).unwrap();
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
        assert_eq!(loaded.memory.read_u32_be(event_ptr + 4), Some(123));
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
        loaded.cpu.gpr[3] = PPC_HEAP_BASE;

        let probe = loaded.run_with_hle_imports(64);

        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_NO_ERR));
    }

    #[test]
    fn hle_import_runner_handles_input_sprocket_virtual_elements() {
        let pef = synthetic_pef_with_library_import(
            b"InputSprocketLib",
            b"ISpElement_NewVirtualFromNeeds",
        );
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
        let pef = synthetic_pef_with_library_import(
            b"InputSprocketLib",
            b"ISpElement_NewVirtualFromNeeds",
        );
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
        let pef = synthetic_pef_with_library_import(
            b"InputSprocketLib",
            b"ISpElement_NewVirtualFromNeeds",
        );
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
        let pef = synthetic_pef_with_library_import(
            b"InputSprocketLib",
            b"ISpElement_NewVirtualFromNeeds",
        );
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
        let pef =
            synthetic_pef_with_library_import(b"InputSprocketLib", b"ISpDevices_ExtractByClass");
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
        let pef =
            synthetic_pef_with_library_import(b"InputSprocketLib", b"ISpDevice_GetElementList");
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
    fn hle_import_runner_handles_input_sprocket_simple_state() {
        let pef =
            synthetic_pef_with_library_import(b"InputSprocketLib", b"ISpElement_GetSimpleState");
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
        let pef =
            synthetic_pef_with_library_import(b"InputSprocketLib", b"ISpElement_GetSimpleState");
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
        let pef = synthetic_pef_with_library_import(
            b"InputSprocketLib",
            b"ISpElement_NewVirtualFromNeeds",
        );
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
        let pef = synthetic_pef_with_library_import(
            b"InputSprocketLib",
            b"ISpElement_NewVirtualFromNeeds",
        );
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
        "[SPROCKET-TRACE] InputSprocketLib:ISpElement_GetSimpleState pc=$01F01100 lr=$01002100 rtoc=$02003100 sp=$03FEEFC0 r3=$02003000 r4=$02004000 r5=$00000000 r6=$00000000 r7=$00000000 r8=$00000000 action=return-preserve isp initialized=true suspended=false keyboard=true mouse=false virtuals=5 last_need_count=3 last_needs=$02001000 last_elements=$02002000 configure_count=2"
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
    };
    let virtual_elements = vec![
        PpcInputSprocketVirtualElementRecord {
            element: 0x0300_0000,
            need_index: 0,
            need_source: 0x0200_1000,
            kind: PPC_ISP_ELEMENT_KIND_BUTTON,
            default_state: 0,
            action_binding: PpcInputSprocketActionBinding::ButtonFire,
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
        "[SPROCKET-TRACE] InputSprocketLib:ISpElement_NewVirtualFromNeeds pc=$01F01000 lr=$01002000 rtoc=$02003000 sp=$03FEF000 r3=$00000002 r4=$02001000 r5=$02002000 r6=$00000000 r7=$00000000 r8=$00000000 action=return($00000000) isp initialized=true suspended=false keyboard=true mouse=true virtuals=2 last_need_count=2 last_needs=$02001000 last_elements=$02002000 configure_count=0 last_bindings=[#0 button 'Fire'=button/fire,#1 delta 'Yaw (Mouse)'=delta/yaw]"
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
