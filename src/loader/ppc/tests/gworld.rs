use super::*;

    #[test]
    fn hle_import_runner_handles_new_gworld_allocation() {
        let pef = synthetic_pef_with_import(b"NewGWorld");
        let mut loaded = load_pef_application(&pef).unwrap();
        let scratch = PPC_DATA_BASE + 0x1000;
        let bounds_ptr = scratch;
        let gworld_out_ptr = scratch + 8;
        loaded.memory.add_region(scratch, vec![0; 16]);
        ppc_write_rect(&mut loaded.memory, bounds_ptr, 0, 0, 64, 128).unwrap();
        loaded.cpu.gpr[3] = gworld_out_ptr;
        loaded.cpu.gpr[4] = 16;
        loaded.cpu.gpr[5] = bounds_ptr;
        loaded.cpu.gpr[6] = 0;
        loaded.cpu.gpr[7] = 0;
        loaded.cpu.gpr[8] = 0;
        let gworld_count = loaded.gworlds.len();

        let probe = loaded.run_with_hle_imports(64);

        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(loaded.cpu.gpr[3] as u16 as i16, PPC_NO_ERR);
        assert_eq!(loaded.last_mem_error(), PPC_NO_ERR);
        assert_eq!(loaded.gworlds.len(), gworld_count + 1);
        let gworld = loaded.memory.read_u32_be(gworld_out_ptr).unwrap();
        let record = loaded
            .gworlds
            .iter()
            .find(|record| record.port == gworld)
            .unwrap();
        assert_eq!(record.width, 128);
        assert_eq!(record.height, 64);
        assert_eq!(record.depth, 16);
        assert_eq!(record.row_bytes, 272);
        assert_eq!(
            loaded.memory.read_u32_be(record.port + 2),
            Some(record.pixmap_handle)
        );
        assert_eq!(
            loaded.memory.read_u32_be(record.pixmap_handle),
            Some(record.pixmap)
        );
        assert_eq!(
            loaded.memory.read_u32_be(record.pixmap),
            Some(record.base_addr)
        );
        assert_eq!(
            loaded.memory.read_u16_be(record.pixmap + 4),
            Some(0x8000 | 272)
        );
        let logical_pixel_end = record.base_addr + record.row_bytes * record.height;
        assert!(record.pixmap >= logical_pixel_end + record.row_bytes);
        for address in logical_pixel_end..logical_pixel_end + record.row_bytes {
            loaded.memory.write_u8(address, 0xff).unwrap();
        }
        assert_eq!(
            loaded.memory.read_u32_be(record.pixmap),
            Some(record.base_addr)
        );
        assert_eq!(
            loaded.memory.read_u16_be(record.pixmap + 4),
            Some(0x8000 | record.row_bytes as u16)
        );
    }

    #[test]
    fn hle_import_runner_new_gworld_depth_zero_copies_main_screen_color_table() {
        let pef = synthetic_pef_with_import(b"NewGWorld");
        let mut loaded = load_pef_application(&pef).unwrap();
        let scratch = PPC_DATA_BASE + 0x1000;
        let bounds_ptr = scratch;
        let gworld_out_ptr = scratch + 8;
        loaded.memory.add_region(scratch, vec![0; 16]);
        ppc_write_rect(&mut loaded.memory, bounds_ptr, 0, 0, 16, 16).unwrap();
        let main_table = ppc_copy_color_table_bytes(&mut loaded.memory, PPC_MAIN_CTABLE_HANDLE)
            .expect("main device ColorTable");
        // Depth zero rescans physical screen devices, not an arbitrary
        // offscreen/custom device that happens to be current.
        loaded
            .current_gdevice
            .with_mut(|current_gdevice| *current_gdevice = 0x0bad_cafe);
        let handle_count = test_handle_records!(loaded).len();
        loaded.cpu.gpr[3] = gworld_out_ptr;
        loaded.cpu.gpr[4] = 0;
        loaded.cpu.gpr[5] = bounds_ptr;
        loaded.cpu.gpr[6] = 0;
        loaded.cpu.gpr[7] = 0;
        loaded.cpu.gpr[8] = 0;

        let probe = loaded.run_with_hle_imports(64);

        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(loaded.cpu.gpr[3] as u16 as i16, PPC_NO_ERR);
        let gworld = loaded.memory.read_u32_be(gworld_out_ptr).unwrap();
        let record = loaded
            .gworlds
            .iter()
            .find(|record| record.port == gworld)
            .unwrap();
        assert_eq!(record.depth, PPC_MAIN_PIXEL_DEPTH);
        assert_eq!(record.gdevice, PPC_MAIN_GDEVICE);
        let private_handle = loaded.memory.read_u32_be(record.pixmap + 42).unwrap();
        assert_ne!(private_handle, 0);
        assert_ne!(private_handle, PPC_MAIN_CTABLE_HANDLE);
        assert_eq!(test_handle_records!(loaded).len(), handle_count + 1);
        assert_eq!(
            ppc_copy_color_table_bytes(&mut loaded.memory, private_handle),
            Some(main_table)
        );

        // Imaging With QuickDraw (1994), pp. 6-16--6-18: pixelDepth 0
        // copies the selected screen device's ColorTable. Later device
        // palette animation must therefore not mutate the offscreen copy.
        let main_ptr = loaded.memory.read_u32_be(PPC_MAIN_CTABLE_HANDLE).unwrap();
        let private_ptr = loaded.memory.read_u32_be(private_handle).unwrap();
        loaded.memory.write_u16_be(main_ptr + 10, 0x1234).unwrap();
        assert_ne!(
            loaded.memory.read_u16_be(private_ptr + 10),
            loaded.memory.read_u16_be(main_ptr + 10)
        );
    }

    #[test]
    fn hle_import_runner_new_gworld_uses_explicit_device_color_table() {
        let pef = synthetic_pef_with_import(b"NewGWorld");
        let mut loaded = load_pef_application(&pef).unwrap();
        let scratch = PPC_DATA_BASE + 0x1000;
        loaded.memory.add_region(scratch, vec![0; 16]);
        ppc_write_rect(&mut loaded.memory, scratch, 0, 0, 16, 16).unwrap();
        let main_table = ppc_copy_color_table_bytes(&mut loaded.memory, PPC_MAIN_CTABLE_HANDLE)
            .expect("main device ColorTable");
        loaded.cpu.gpr[3] = scratch + 8;
        // Imaging With QuickDraw (1994), p. 6-18: noNewDevice uses the
        // supplied device's depth and ColorTable after validating the literal
        // pixelDepth argument. The otherwise-valid 16-bit depth and invalid
        // caller ColorTable must therefore both be ignored here.
        loaded.cpu.gpr[4] = 16;
        loaded.cpu.gpr[5] = scratch;
        loaded.cpu.gpr[6] = 0x0bad_cafe;
        loaded.cpu.gpr[7] = PPC_MAIN_GDEVICE;
        loaded.cpu.gpr[8] = 1 << 1;

        let probe = loaded.run_with_hle_imports(64);

        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(loaded.cpu.gpr[3] as u16 as i16, PPC_NO_ERR);
        let port = loaded.memory.read_u32_be(scratch + 8).unwrap();
        let record = loaded
            .gworlds
            .iter()
            .find(|record| record.port == port)
            .unwrap();
        assert_eq!(record.depth, PPC_MAIN_PIXEL_DEPTH);
        assert_eq!(record.gdevice, PPC_MAIN_GDEVICE);
        let ctable = loaded.memory.read_u32_be(record.pixmap + 42).unwrap();
        assert_ne!(ctable, PPC_MAIN_CTABLE_HANDLE);
        assert_eq!(
            ppc_copy_color_table_bytes(&mut loaded.memory, ctable),
            Some(main_table)
        );
        let main_ptr = loaded.memory.read_u32_be(PPC_MAIN_CTABLE_HANDLE).unwrap();
        let private_ptr = loaded.memory.read_u32_be(ctable).unwrap();
        loaded.memory.write_u16_be(main_ptr + 10, 0x1234).unwrap();
        assert_ne!(
            loaded.memory.read_u16_be(private_ptr + 10),
            loaded.memory.read_u16_be(main_ptr + 10)
        );
    }

    #[test]
    fn hle_import_runner_new_gworld_rejects_invalid_literal_depth_with_explicit_device() {
        let pef = synthetic_pef_with_import(b"NewGWorld");
        let mut loaded = load_pef_application(&pef).unwrap();
        let scratch = PPC_DATA_BASE + 0x1000;
        let bounds_ptr = scratch;
        let gworld_out_ptr = scratch + 8;
        loaded.memory.add_region(scratch, vec![0; 16]);
        ppc_write_rect(&mut loaded.memory, bounds_ptr, 0, 0, 2, 2).unwrap();
        loaded
            .memory
            .write_u32_be(gworld_out_ptr, 0xdead_beef)
            .unwrap();
        let heap_cursor_before = loaded.heap_cursor();
        let handles_before = test_handle_records!(loaded).clone();
        let gworlds_before = loaded.gworlds.clone();
        let allocations_before = loaded.toolbox_startup.gworld_allocations.clone();
        let region_count_before = loaded.memory.region_count();

        for requested_depth in [u32::from(u16::MAX), 3] {
            loaded.cpu.gpr[3] = gworld_out_ptr;
            loaded.cpu.gpr[4] = requested_depth;
            loaded.cpu.gpr[5] = bounds_ptr;
            loaded.cpu.gpr[6] = 0;
            loaded.cpu.gpr[7] = PPC_MAIN_GDEVICE;
            loaded.cpu.gpr[8] = 1 << 1; // noNewDevice
            run_test_import(&mut loaded, PpcImportDispatcherTarget::NewGWorld);

            assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_C_DEPTH_ERR));
            assert_eq!(loaded.last_mem_error(), PPC_C_DEPTH_ERR);
            assert_eq!(*loaded.toolbox_startup.last_quickdraw_error, PPC_C_DEPTH_ERR);
            run_test_import(&mut loaded, PpcImportDispatcherTarget::QDError);
            assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_C_DEPTH_ERR));
            assert_eq!(loaded.memory.read_u32_be(gworld_out_ptr), Some(0xdead_beef));
            assert_eq!(loaded.heap_cursor(), heap_cursor_before);
            assert_eq!(test_handle_records!(loaded), handles_before);
            assert_eq!(loaded.gworlds, gworlds_before);
            assert_eq!(
                loaded.toolbox_startup.gworld_allocations,
                allocations_before
            );
            assert_eq!(loaded.memory.region_count(), region_count_before);
        }
    }

    #[test]
    fn hle_import_runner_new_pixmap_allocates_an_uninitialized_color_table() {
        let pef = synthetic_pef_with_import(b"NewPixMap");
        let mut loaded = load_pef_application(&pef).unwrap();

        let probe = loaded.run_with_hle_imports(64);

        assert_eq!(probe.unsupported_import_index, None);
        let pixmap_handle = loaded.cpu.gpr[3];
        assert_ne!(pixmap_handle, 0);
        let pixmap = loaded.memory.read_u32_be(pixmap_handle).unwrap();
        let private_handle = loaded.memory.read_u32_be(pixmap + 42).unwrap();
        assert_ne!(private_handle, 0);
        assert_ne!(private_handle, PPC_MAIN_CTABLE_HANDLE);
        let private_ptr = loaded.memory.read_u32_be(private_handle).unwrap();
        let mut header = [0xff; 8];
        loaded
            .memory
            .read_bytes_into(private_ptr, &mut header)
            .unwrap();
        assert_eq!(header, [0; 8]);
    }

    #[test]
    fn hle_import_runner_update_gworld_flags_control_color_table_pixel_mapping() {
        let pef = synthetic_pef_with_import(b"NewGWorld");
        let mut loaded = load_pef_application(&pef).unwrap();
        let scratch = PPC_DATA_BASE + 0x1000;
        let bounds_ptr = scratch;
        let gworld_out_ptr = scratch + 8;
        loaded.memory.add_region(scratch, vec![0; 16]);
        ppc_write_rect(&mut loaded.memory, bounds_ptr, 0, 0, 16, 16).unwrap();
        loaded.cpu.gpr[3] = gworld_out_ptr;
        loaded.cpu.gpr[4] = 0;
        loaded.cpu.gpr[5] = bounds_ptr;
        loaded.cpu.gpr[6] = 0;
        loaded.cpu.gpr[7] = 0;
        loaded.cpu.gpr[8] = 0;
        loaded.run_with_hle_imports(64);
        let gworld = loaded.memory.read_u32_be(gworld_out_ptr).unwrap();
        let record = *loaded
            .gworlds
            .iter()
            .find(|record| record.port == gworld)
            .unwrap();
        loaded.memory.write_u8(record.base_addr, 42).unwrap();

        let main_table = loaded.memory.read_u32_be(PPC_MAIN_CTABLE_HANDLE).unwrap();
        let old_color = [
            loaded
                .memory
                .read_u16_be(main_table + 8 + 42 * 8 + 2)
                .unwrap(),
            loaded
                .memory
                .read_u16_be(main_table + 8 + 42 * 8 + 4)
                .unwrap(),
            loaded
                .memory
                .read_u16_be(main_table + 8 + 42 * 8 + 6)
                .unwrap(),
        ];
        for channel in 0..3u32 {
            loaded
                .memory
                .write_u16_be(main_table + 8 + 42 * 8 + 2 + channel * 2, 0)
                .unwrap();
            loaded
                .memory
                .write_u16_be(
                    main_table + 8 + 77 * 8 + 2 + channel * 2,
                    old_color[channel as usize],
                )
                .unwrap();
        }

        loaded.cpu.pc = loaded.entry_pc;
        loaded.cpu.lr = PPC_HALT_PC;
        loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::UpdateGWorld;
        loaded.cpu.gpr[3] = gworld_out_ptr;
        loaded.cpu.gpr[4] = 0;
        loaded.cpu.gpr[5] = bounds_ptr;
        loaded.cpu.gpr[6] = 0;
        loaded.cpu.gpr[7] = 0;
        loaded.cpu.gpr[8] = 0;

        let probe = loaded.run_with_hle_imports(64);

        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(loaded.cpu.gpr[3] & (1 << 16), 0, "mapPix without clipPix");
        assert_eq!(loaded.cpu.gpr[3] & (1 << 20), 0, "reallocPix");
        let updated = *loaded
            .gworlds
            .iter()
            .find(|updated| updated.port == gworld)
            .unwrap();
        assert_eq!(updated.base_addr, record.base_addr);
        assert_eq!(loaded.memory.read_u8(updated.base_addr), Some(42));
        let private_handle = loaded.memory.read_u32_be(updated.pixmap + 42).unwrap();
        assert_eq!(
            ppc_copy_color_table_bytes(&mut loaded.memory, private_handle),
            ppc_copy_color_table_bytes(&mut loaded.memory, PPC_MAIN_CTABLE_HANDLE)
        );

        // With clipPix, preserve the same pixel bytes through an RGB mapping.
        // The existing allocation has enough capacity, so this updates in
        // place and must not falsely report reallocPix.
        for channel in 0..3u32 {
            loaded
                .memory
                .write_u16_be(
                    main_table + 8 + 42 * 8 + 2 + channel * 2,
                    old_color[channel as usize],
                )
                .unwrap();
            loaded
                .memory
                .write_u16_be(main_table + 8 + 77 * 8 + 2 + channel * 2, 0)
                .unwrap();
        }
        loaded.cpu.gpr[3] = gworld_out_ptr;
        loaded.cpu.gpr[4] = 0;
        loaded.cpu.gpr[5] = bounds_ptr;
        loaded.cpu.gpr[6] = 0;
        loaded.cpu.gpr[7] = 0;
        loaded.cpu.gpr[8] = 1 << 28;
        run_test_import(&mut loaded, PpcImportDispatcherTarget::UpdateGWorld);

        assert_ne!(loaded.cpu.gpr[3] & (1 << 16), 0, "mapPix");
        assert_eq!(loaded.cpu.gpr[3] & (1 << 20), 0, "reallocPix");
        let remapped = *loaded
            .gworlds
            .iter()
            .find(|updated| updated.port == gworld)
            .unwrap();
        assert_eq!(remapped.base_addr, record.base_addr);
        assert_eq!(loaded.memory.read_u8(remapped.base_addr), Some(77));
        assert_eq!(
            loaded.memory.read_u32_be(remapped.pixmap + 42),
            Some(private_handle)
        );
    }

    #[test]
    fn new_gworld_allocations_are_immediately_process_owned_and_cross_isa_visible() {
        let pef = synthetic_pef_with_import(b"NewGWorld");
        let mut native = load_pef_application(&pef).unwrap();
        let mut context = ProcessContext::default();
        native.attach_unconverted_process_services(&mut context);
        let detached = context.memory_manager_mut().detached_clone();
        let scratch = PPC_DATA_BASE + 0x1000;
        let bounds_ptr = scratch;
        let gworld_out_ptr = scratch + 8;
        native.memory.add_region(scratch, vec![0; 16]);
        ppc_write_rect(&mut native.memory, bounds_ptr, 0, 0, 4, 4).unwrap();
        native.cpu.gpr[3] = gworld_out_ptr;
        native.cpu.gpr[4] = 8;
        native.cpu.gpr[5] = bounds_ptr;
        native.cpu.gpr[6] = 0;
        native.cpu.gpr[7] = 0;
        native.cpu.gpr[8] = 0;

        run_test_import(&mut native, PpcImportDispatcherTarget::NewGWorld);

        assert_eq!(native.cpu.gpr[3] as u16 as i16, PPC_NO_ERR);
        let port = native.memory.read_u32_be(gworld_out_ptr).unwrap();
        let world = *native
            .gworlds
            .iter()
            .find(|world| world.port == port)
            .unwrap();
        assert!(native
            .gworld_pixel_states
            .has_quickdraw_pixel_state(world.pixmap_handle));
        let ctable_handle = native.memory.read_u32_be(world.pixmap + 42).unwrap();
        let ctable = context
            .memory_manager_mut()
            .native_allocation(ctable_handle)
            .unwrap();
        let allocator = context
            .memory_manager_mut()
            .native_allocator_snapshot()
            .unwrap();
        assert!(allocator
            .ptrs
            .iter()
            .any(|record| record.ptr == world.base_addr));
        assert_eq!(
            context.memory_manager_mut().recover_handle(ctable.ptr),
            Some(ctable_handle)
        );
        assert!(!detached
            .native_allocator()
            .unwrap()
            .ptrs
            .iter()
            .any(|record| record.ptr == world.base_addr));
        assert_eq!(detached.native_allocation(ctable_handle), None);

        let shared = native.memory.shared_view();
        let mut classic_bus = MacMemoryBus::new(0x2000);
        classic_bus.attach_guest_address_space(shared);
        context.attach_classic_memory_bus(&mut classic_bus);
        classic_bus.write_byte(world.base_addr, 0xa5);
        assert_eq!(native.memory.read_u8(world.base_addr), Some(0xa5));
        native.memory.write_u8(world.base_addr + 1, 0x5a).unwrap();
        assert_eq!(classic_bus.read_byte(world.base_addr + 1), 0x5a);

        native.cpu.gpr[3] = port;
        run_test_import(&mut native, PpcImportDispatcherTarget::DisposeGWorld);
        assert!(
            native
                .gworld_pixel_states
                .has_quickdraw_pixel_state(world.pixmap_handle),
            "local disposal must not delete non-owning process pixel state"
        );
        let allocator = context
            .memory_manager_mut()
            .native_allocator_snapshot()
            .unwrap();
        assert!(!allocator
            .ptrs
            .iter()
            .any(|record| record.ptr == world.base_addr));
        assert_eq!(
            context
                .memory_manager_mut()
                .native_allocation(ctable_handle),
            None
        );
    }

    #[test]
    fn hle_import_runner_update_gworld_writes_coherent_metadata_at_every_depth() {
        let pef = synthetic_pef_with_import(b"NewGWorld");
        let mut loaded = load_pef_application(&pef).unwrap();
        let scratch = PPC_DATA_BASE + 0x1000;
        let bounds_ptr = scratch;
        let gworld_out_ptr = scratch + 8;
        loaded.memory.add_region(scratch, vec![0; 16]);
        ppc_write_rect(&mut loaded.memory, bounds_ptr, 2, 3, 5, 8).unwrap();
        loaded.cpu.gpr[3] = gworld_out_ptr;
        loaded.cpu.gpr[4] = 8;
        loaded.cpu.gpr[5] = bounds_ptr;
        loaded.cpu.gpr[6] = 0;
        loaded.cpu.gpr[7] = 0;
        loaded.cpu.gpr[8] = 0;
        run_test_import(&mut loaded, PpcImportDispatcherTarget::NewGWorld);
        assert_eq!(loaded.cpu.gpr[3] as u16 as i16, PPC_NO_ERR);
        let gworld = loaded.memory.read_u32_be(gworld_out_ptr).unwrap();

        for depth in [1u32, 2, 4, 8, 16, 32] {
            loaded.cpu.gpr[3] = gworld_out_ptr;
            loaded.cpu.gpr[4] = depth;
            loaded.cpu.gpr[5] = bounds_ptr;
            loaded.cpu.gpr[6] = 0;
            loaded.cpu.gpr[7] = 0;
            loaded.cpu.gpr[8] = 1 << 28;
            run_test_import(&mut loaded, PpcImportDispatcherTarget::UpdateGWorld);
            assert_eq!(loaded.cpu.gpr[3] & (1 << 31), 0, "{depth}bpp failed");
            assert_eq!(loaded.last_mem_error(), PPC_NO_ERR);

            let record = loaded
                .gworlds
                .iter()
                .find(|record| record.port == gworld)
                .unwrap();
            let pixmap = record.pixmap;
            let (pixel_type, component_count, component_size) = match depth {
                1 | 2 | 4 | 8 => (0, 1, depth as u16),
                16 => (16, 3, 5),
                32 => (16, 3, 8),
                _ => unreachable!(),
            };
            assert_eq!(record.depth, depth);
            assert_eq!(record.row_bytes, ppc_row_bytes(5, depth).unwrap());
            assert_eq!(loaded.memory.read_u16_be(pixmap + 30), Some(pixel_type));
            assert_eq!(loaded.memory.read_u16_be(pixmap + 32), Some(depth as u16));
            assert_eq!(
                loaded.memory.read_u16_be(pixmap + 34),
                Some(component_count)
            );
            assert_eq!(loaded.memory.read_u16_be(pixmap + 36), Some(component_size));
            assert_eq!(loaded.memory.read_u32_be(pixmap + 38), Some(0));
            let ctable = loaded.memory.read_u32_be(pixmap + 42).unwrap();
            if depth <= 8 {
                assert_ne!(ctable, 0, "{depth}bpp indexed PixMap has no pmTable");
                assert_eq!(
                    ppc_copy_color_table_bytes(&mut loaded.memory, ctable),
                    ppc_default_color_table_bytes(depth)
                );
            } else {
                assert_eq!(ctable, 0, "{depth}bpp direct PixMap retained a pmTable");
            }
        }
    }

    #[test]
    fn update_gworld_allocations_are_immediately_process_owned_and_cross_isa_visible() {
        let pef = synthetic_pef_with_import(b"NewGWorld");
        let mut native = load_pef_application(&pef).unwrap();
        let mut context = ProcessContext::default();
        native.attach_unconverted_process_services(&mut context);
        let scratch = PPC_DATA_BASE + 0x1000;
        let bounds_ptr = scratch;
        let gworld_out_ptr = scratch + 8;
        native.memory.add_region(scratch, vec![0; 16]);
        ppc_write_rect(&mut native.memory, bounds_ptr, 0, 0, 2, 2).unwrap();
        native.cpu.gpr[3] = gworld_out_ptr;
        native.cpu.gpr[4] = 8;
        native.cpu.gpr[5] = bounds_ptr;
        native.cpu.gpr[6] = 0;
        native.cpu.gpr[7] = 0;
        native.cpu.gpr[8] = 0;
        run_test_import(&mut native, PpcImportDispatcherTarget::NewGWorld);
        let gworld = native.memory.read_u32_be(gworld_out_ptr).unwrap();
        let original = *native
            .gworlds
            .iter()
            .find(|record| record.port == gworld)
            .unwrap();
        let detached = context.memory_manager_mut().detached_clone();

        ppc_write_rect(&mut native.memory, bounds_ptr, 0, 0, 96, 96).unwrap();
        native.cpu.gpr[3] = gworld_out_ptr;
        native.cpu.gpr[4] = 32;
        native.cpu.gpr[5] = bounds_ptr;
        native.cpu.gpr[6] = 0;
        native.cpu.gpr[7] = 0;
        native.cpu.gpr[8] = 1 << 28;
        run_test_import(&mut native, PpcImportDispatcherTarget::UpdateGWorld);

        assert_eq!(native.cpu.gpr[3] & (1 << 31), 0);
        let updated = *native
            .gworlds
            .iter()
            .find(|record| record.port == gworld)
            .unwrap();
        assert_ne!(updated.base_addr, original.base_addr);
        let allocator = context
            .memory_manager_mut()
            .native_allocator_snapshot()
            .unwrap();
        assert!(allocator
            .ptrs
            .iter()
            .any(|record| record.ptr == original.base_addr));
        assert!(allocator
            .ptrs
            .iter()
            .any(|record| record.ptr == updated.base_addr));
        assert!(!detached
            .native_allocator()
            .unwrap()
            .ptrs
            .iter()
            .any(|record| record.ptr == updated.base_addr));

        let shared = native.memory.shared_view();
        let mut classic_bus = MacMemoryBus::new(0x2000);
        classic_bus.attach_guest_address_space(shared);
        context.attach_classic_memory_bus(&mut classic_bus);
        classic_bus.write_byte(updated.base_addr, 0xa5);
        assert_eq!(native.memory.read_u8(updated.base_addr), Some(0xa5));
        native.memory.write_u8(updated.base_addr + 1, 0x5a).unwrap();
        assert_eq!(classic_bus.read_byte(updated.base_addr + 1), 0x5a);
    }

    #[test]
    fn hle_import_runner_update_gworld_does_not_mutate_caller_owned_gdevice() {
        let pef = synthetic_pef_with_import(b"NewGWorld");
        let mut loaded = load_pef_application(&pef).unwrap();
        let scratch = PPC_DATA_BASE + 0x1000;
        let bounds_ptr = scratch;
        let gworld_out_ptr = scratch + 8;
        loaded.memory.add_region(scratch, vec![0; 16]);
        ppc_write_rect(&mut loaded.memory, bounds_ptr, 0, 0, 2, 2).unwrap();
        loaded.cpu.gpr[3] = gworld_out_ptr;
        loaded.cpu.gpr[4] = 8;
        loaded.cpu.gpr[5] = bounds_ptr;
        loaded.cpu.gpr[6] = 0;
        loaded.cpu.gpr[7] = 0;
        loaded.cpu.gpr[8] = 0;
        run_test_import(&mut loaded, PpcImportDispatcherTarget::NewGWorld);
        let gworld = loaded.memory.read_u32_be(gworld_out_ptr).unwrap();

        let gdevice_handle = PPC_DATA_BASE + 0x5000;
        let gdevice = PPC_DATA_BASE + 0x5100;
        let pixmap_handle = PPC_DATA_BASE + 0x5200;
        let pixmap = PPC_DATA_BASE + 0x5300;
        loaded.memory.add_region(gdevice_handle, vec![0; 4]);
        loaded.memory.add_region(gdevice, vec![0; 64]);
        loaded.memory.add_region(pixmap_handle, vec![0; 4]);
        loaded
            .memory
            .add_region(pixmap, vec![0; PPC_PIXMAP_SIZE as usize]);
        loaded.memory.write_u32_be(gdevice_handle, gdevice).unwrap();
        loaded
            .memory
            .write_u32_be(gdevice + 22, pixmap_handle)
            .unwrap();
        loaded.memory.write_u32_be(pixmap_handle, pixmap).unwrap();
        ppc_write_pixmap(
            &mut loaded.memory,
            pixmap,
            0x0600_0000,
            ppc_row_bytes(40, 16).unwrap(),
            -10,
            -20,
            10,
            20,
            16,
        )
        .unwrap();
        ppc_write_rect(&mut loaded.memory, gdevice + 34, -10, -20, 10, 20).unwrap();
        let gdevice_before = ppc_memory_read_bytes(&mut loaded.memory, gdevice, 64).unwrap();
        let device_pixmap_before =
            ppc_memory_read_bytes(&mut loaded.memory, pixmap, PPC_PIXMAP_SIZE).unwrap();

        ppc_write_rect(&mut loaded.memory, bounds_ptr, 5, 6, 8, 10).unwrap();
        loaded
            .current_gworld
            .with_mut(|current_gworld| *current_gworld = gworld);
        loaded
            .current_gdevice
            .with_mut(|current_gdevice| *current_gdevice = PPC_MAIN_GDEVICE);
        loaded
            .toolbox_startup
            .last_quickdraw_error
            .with_mut(|error| *error = PPC_PARAM_ERR);
        loaded.cpu.gpr[3] = gworld_out_ptr;
        loaded.cpu.gpr[4] = u32::from(u16::MAX); // ignored when aGDevice is non-NIL
        loaded.cpu.gpr[5] = bounds_ptr;
        loaded.cpu.gpr[6] = 0;
        loaded.cpu.gpr[7] = gdevice_handle;
        loaded.cpu.gpr[8] = 1 << 28;
        run_test_import(&mut loaded, PpcImportDispatcherTarget::UpdateGWorld);

        assert_eq!(loaded.cpu.gpr[3] & (1 << 31), 0);
        assert_eq!(*loaded.toolbox_startup.last_quickdraw_error, PPC_NO_ERR);
        let updated = loaded
            .gworlds
            .iter()
            .find(|record| record.port == gworld)
            .unwrap();
        assert_eq!(updated.gdevice, gdevice_handle);
        assert_eq!(updated.depth, 16);
        assert_eq!(*loaded.current_gworld, gworld);
        assert_eq!(*loaded.current_gdevice, gdevice_handle);
        run_test_import(&mut loaded, PpcImportDispatcherTarget::GetGDevice);
        assert_eq!(loaded.cpu.gpr[3], gdevice_handle);
        assert_eq!(
            ppc_memory_read_bytes(&mut loaded.memory, gdevice, 64),
            Some(gdevice_before.clone())
        );
        assert_eq!(
            ppc_memory_read_bytes(&mut loaded.memory, pixmap, PPC_PIXMAP_SIZE),
            Some(device_pixmap_before)
        );

        loaded.memory.write_u16_be(pixmap + 32, 0).unwrap();
        let invalid_device_pixmap =
            ppc_memory_read_bytes(&mut loaded.memory, pixmap, PPC_PIXMAP_SIZE).unwrap();
        let world_before = *loaded
            .gworlds
            .iter()
            .find(|record| record.port == gworld)
            .unwrap();
        let world_pixmap_before =
            ppc_memory_read_bytes(&mut loaded.memory, world_before.pixmap, PPC_PIXMAP_SIZE)
                .unwrap();
        let world_pixels_before = ppc_memory_read_bytes(
            &mut loaded.memory,
            world_before.base_addr,
            world_before.row_bytes * world_before.height,
        )
        .unwrap();
        let handles_before = test_handle_records!(loaded).clone();
        let allocations_before = loaded.toolbox_startup.gworld_allocations.clone();
        let heap_cursor_before = loaded.heap_cursor();
        ppc_write_rect(&mut loaded.memory, bounds_ptr, 0, 0, 1, 1).unwrap();
        loaded.cpu.gpr[3] = gworld_out_ptr;
        loaded.cpu.gpr[4] = 8;
        loaded.cpu.gpr[5] = bounds_ptr;
        loaded.cpu.gpr[6] = 0;
        loaded.cpu.gpr[7] = gdevice_handle;
        loaded.cpu.gpr[8] = 1 << 28;
        run_test_import(&mut loaded, PpcImportDispatcherTarget::UpdateGWorld);

        assert_eq!(loaded.cpu.gpr[3], 1 << 31);
        assert_eq!(loaded.last_mem_error(), PPC_C_DEPTH_ERR);
        assert_eq!(
            loaded.gworlds.iter().find(|record| record.port == gworld),
            Some(&world_before)
        );
        assert_eq!(test_handle_records!(loaded), handles_before);
        assert_eq!(
            loaded.toolbox_startup.gworld_allocations,
            allocations_before
        );
        assert_eq!(loaded.heap_cursor(), heap_cursor_before);
        assert_eq!(
            ppc_memory_read_bytes(&mut loaded.memory, world_before.pixmap, PPC_PIXMAP_SIZE),
            Some(world_pixmap_before)
        );
        assert_eq!(
            ppc_memory_read_bytes(
                &mut loaded.memory,
                world_before.base_addr,
                world_before.row_bytes * world_before.height,
            ),
            Some(world_pixels_before)
        );
        assert_eq!(
            ppc_memory_read_bytes(&mut loaded.memory, gdevice, 64),
            Some(gdevice_before)
        );
        assert_eq!(
            ppc_memory_read_bytes(&mut loaded.memory, pixmap, PPC_PIXMAP_SIZE),
            Some(invalid_device_pixmap)
        );
        run_test_import(&mut loaded, PpcImportDispatcherTarget::QDError);
        assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_C_DEPTH_ERR));
    }

    #[test]
    fn hle_import_runner_update_gworld_translates_cross_depth_clip_and_stretch_pixels() {
        const CLIP_PIX: u32 = 1 << 28;
        const STRETCH_PIX: u32 = 1 << 29;

        assert_eq!(
            ppc_gworld_rgb_to_pixel(
                PpcRgbColor {
                    red: 0x07ff,
                    green: 0,
                    blue: 0,
                },
                16,
                None,
            ),
            Some(0),
            "16-bit direct conversion truncates to the most significant 5 bits"
        );

        let pef = synthetic_pef_with_import(b"NewGWorld");
        let mut loaded = load_pef_application(&pef).unwrap();
        let scratch = PPC_DATA_BASE + 0x1000;
        let bounds_ptr = scratch;
        let gworld_out_ptr = scratch + 8;
        loaded.memory.add_region(scratch, vec![0; 16]);
        let source_colors = [
            [0xffff, 0, 0],
            [0, 0xffff, 0],
            [0, 0, 0xffff],
            [0xffff, 0xffff, 0xffff],
        ];
        let source_ctable = ppc_alloc_handle_with_bytes(
            &mut loaded.memory,
            test_heap_cursor!(loaded),
            test_heap_limit!(loaded),
            test_handles!(loaded),
            &ppc_color_table_bytes(0x1234_5678, &source_colors).unwrap(),
        );
        assert_ne!(source_ctable, 0);
        ppc_write_rect(&mut loaded.memory, bounds_ptr, 0, 0, 2, 2).unwrap();
        loaded.cpu.gpr[3] = gworld_out_ptr;
        loaded.cpu.gpr[4] = 8;
        loaded.cpu.gpr[5] = bounds_ptr;
        loaded.cpu.gpr[6] = source_ctable;
        loaded.cpu.gpr[7] = 0;
        loaded.cpu.gpr[8] = 0;
        run_test_import(&mut loaded, PpcImportDispatcherTarget::NewGWorld);
        let gworld = loaded.memory.read_u32_be(gworld_out_ptr).unwrap();
        let source = *loaded
            .gworlds
            .iter()
            .find(|record| record.port == gworld)
            .unwrap();
        let source_bits = ppc_pixmap_bits_from_record(source).unwrap();
        for (x, y, pixel) in [(0, 0, 0), (1, 0, 1), (0, 1, 2), (1, 1, 3)] {
            ppc_write_pixmap_raw_pixel(&mut loaded.memory, source_bits, x, y, pixel).unwrap();
        }

        // clipPix keeps the top-left portion and performs the indexed-to-555
        // translation before clipping the bottom row.
        ppc_write_rect(&mut loaded.memory, bounds_ptr, 0, 0, 1, 2).unwrap();
        loaded.cpu.gpr[3] = gworld_out_ptr;
        loaded.cpu.gpr[4] = 16;
        loaded.cpu.gpr[5] = bounds_ptr;
        loaded.cpu.gpr[6] = 0;
        loaded.cpu.gpr[7] = 0;
        loaded.cpu.gpr[8] = CLIP_PIX;
        run_test_import(&mut loaded, PpcImportDispatcherTarget::UpdateGWorld);
        assert_ne!(loaded.cpu.gpr[3] & CLIP_PIX, 0);
        assert_eq!(
            loaded.cpu.gpr[3] & (1 << 16),
            0,
            "no destination ColorTable"
        );
        assert_ne!(loaded.cpu.gpr[3] & (1 << 17), 0, "newDepth");
        let direct16 = *loaded
            .gworlds
            .iter()
            .find(|record| record.port == gworld)
            .unwrap();
        let direct16_bits = ppc_pixmap_bits_from_record(direct16).unwrap();
        assert_eq!(
            ppc_read_pixmap_raw_pixel(&mut loaded.memory, direct16_bits, 0, 0),
            Some(0x7c00)
        );
        assert_eq!(
            ppc_read_pixmap_raw_pixel(&mut loaded.memory, direct16_bits, 1, 0),
            Some(0x03e0)
        );

        // stretchPix scales the translated direct image in both axes. The
        // high byte of every 32-bit direct pixel remains the documented zero.
        ppc_write_rect(&mut loaded.memory, bounds_ptr, 0, 0, 2, 4).unwrap();
        loaded.cpu.gpr[3] = gworld_out_ptr;
        loaded.cpu.gpr[4] = 32;
        loaded.cpu.gpr[5] = bounds_ptr;
        loaded.cpu.gpr[6] = 0;
        loaded.cpu.gpr[7] = 0;
        loaded.cpu.gpr[8] = STRETCH_PIX;
        run_test_import(&mut loaded, PpcImportDispatcherTarget::UpdateGWorld);
        assert_ne!(loaded.cpu.gpr[3] & STRETCH_PIX, 0);
        let direct32 = *loaded
            .gworlds
            .iter()
            .find(|record| record.port == gworld)
            .unwrap();
        let direct32_bits = ppc_pixmap_bits_from_record(direct32).unwrap();
        for y in 0..2 {
            assert_eq!(
                ppc_read_pixmap_raw_pixel(&mut loaded.memory, direct32_bits, 0, y),
                Some(0x00ff_0000)
            );
            assert_eq!(
                ppc_read_pixmap_raw_pixel(&mut loaded.memory, direct32_bits, 1, y),
                Some(0x00ff_0000)
            );
            assert_eq!(
                ppc_read_pixmap_raw_pixel(&mut loaded.memory, direct32_bits, 2, y),
                Some(0x0000_ff00)
            );
            assert_eq!(
                ppc_read_pixmap_raw_pixel(&mut loaded.memory, direct32_bits, 3, y),
                Some(0x0000_ff00)
            );
        }

        let mut target_colors = vec![[0; 3]; 16];
        target_colors[1] = [0xffff, 0, 0];
        target_colors[2] = [0, 0xffff, 0];
        target_colors[3] = [0, 0, 0xffff];
        target_colors[4] = [0xffff, 0xffff, 0xffff];
        let target_ctable = ppc_alloc_handle_with_bytes(
            &mut loaded.memory,
            test_heap_cursor!(loaded),
            test_heap_limit!(loaded),
            test_handles!(loaded),
            &ppc_color_table_bytes(0x8765_4321, &target_colors).unwrap(),
        );
        assert_ne!(target_ctable, 0);
        ppc_write_rect(&mut loaded.memory, bounds_ptr, 0, 0, 2, 3).unwrap();
        loaded.cpu.gpr[3] = gworld_out_ptr;
        loaded.cpu.gpr[4] = 4;
        loaded.cpu.gpr[5] = bounds_ptr;
        loaded.cpu.gpr[6] = target_ctable;
        loaded.cpu.gpr[7] = 0;
        loaded.cpu.gpr[8] = CLIP_PIX;
        run_test_import(&mut loaded, PpcImportDispatcherTarget::UpdateGWorld);
        assert_ne!(loaded.cpu.gpr[3] & (1 << 16), 0, "mapPix");
        let indexed4 = *loaded
            .gworlds
            .iter()
            .find(|record| record.port == gworld)
            .unwrap();
        let indexed4_bits = ppc_pixmap_bits_from_record(indexed4).unwrap();
        for y in 0..2 {
            assert_eq!(
                ppc_read_pixmap_raw_pixel(&mut loaded.memory, indexed4_bits, 0, y),
                Some(1)
            );
            assert_eq!(
                ppc_read_pixmap_raw_pixel(&mut loaded.memory, indexed4_bits, 1, y),
                Some(1)
            );
            assert_eq!(
                ppc_read_pixmap_raw_pixel(&mut loaded.memory, indexed4_bits, 2, y),
                Some(2)
            );
        }
    }

    #[test]
    fn hle_import_runner_update_gworld_maps_to_sparse_color_table_values() {
        const CLIP_PIX: u32 = 1 << 28;

        let pef = synthetic_pef_with_import(b"NewGWorld");
        let mut loaded = load_pef_application(&pef).unwrap();
        let scratch = PPC_DATA_BASE + 0x1000;
        let bounds_ptr = scratch;
        let gworld_out_ptr = scratch + 8;
        loaded.memory.add_region(scratch, vec![0; 16]);
        ppc_write_rect(&mut loaded.memory, bounds_ptr, 0, 0, 1, 1).unwrap();
        loaded.cpu.gpr[3] = gworld_out_ptr;
        loaded.cpu.gpr[4] = 16;
        loaded.cpu.gpr[5] = bounds_ptr;
        loaded.cpu.gpr[6] = 0;
        loaded.cpu.gpr[7] = 0;
        loaded.cpu.gpr[8] = 0;
        run_test_import(&mut loaded, PpcImportDispatcherTarget::NewGWorld);
        let gworld = loaded.memory.read_u32_be(gworld_out_ptr).unwrap();
        let source = *loaded
            .gworlds
            .iter()
            .find(|record| record.port == gworld)
            .unwrap();
        loaded.memory.write_u16_be(source.base_addr, 0).unwrap();

        let mut sparse_bytes = ppc_color_table_bytes(0x1234_5678, &[[0, 0, 0]]).unwrap();
        sparse_bytes[8..10].copy_from_slice(&9u16.to_be_bytes());
        let sparse_ctable = ppc_alloc_handle_with_bytes(
            &mut loaded.memory,
            test_heap_cursor!(loaded),
            test_heap_limit!(loaded),
            test_handles!(loaded),
            &sparse_bytes,
        );
        assert_ne!(sparse_ctable, 0);

        loaded.cpu.gpr[3] = gworld_out_ptr;
        loaded.cpu.gpr[4] = 8;
        loaded.cpu.gpr[5] = bounds_ptr;
        loaded.cpu.gpr[6] = sparse_ctable;
        loaded.cpu.gpr[7] = 0;
        loaded.cpu.gpr[8] = CLIP_PIX;
        run_test_import(&mut loaded, PpcImportDispatcherTarget::UpdateGWorld);

        assert_ne!(loaded.cpu.gpr[3] & (1 << 16), 0, "mapPix");
        assert_ne!(loaded.cpu.gpr[3] & (1 << 17), 0, "newDepth");
        let indexed = *loaded
            .gworlds
            .iter()
            .find(|record| record.port == gworld)
            .unwrap();
        assert_eq!(loaded.memory.read_u8(indexed.base_addr), Some(9));
    }

    #[test]
    fn hle_import_runner_update_gworld_preserves_rect_span_beyond_i16_max() {
        const CLIP_PIX: u32 = 1 << 28;

        let pef = synthetic_pef_with_import(b"NewGWorld");
        let mut loaded = load_pef_application(&pef).unwrap();
        let scratch = PPC_DATA_BASE + 0x1000;
        let bounds_ptr = scratch;
        let gworld_out_ptr = scratch + 8;
        loaded.memory.add_region(scratch, vec![0; 16]);
        ppc_write_rect(&mut loaded.memory, bounds_ptr, 0, i16::MIN, 1, 0).unwrap();
        loaded.cpu.gpr[3] = gworld_out_ptr;
        loaded.cpu.gpr[4] = 1;
        loaded.cpu.gpr[5] = bounds_ptr;
        loaded.cpu.gpr[6] = 0;
        loaded.cpu.gpr[7] = 0;
        loaded.cpu.gpr[8] = 0;
        run_test_import(&mut loaded, PpcImportDispatcherTarget::NewGWorld);
        let gworld = loaded.memory.read_u32_be(gworld_out_ptr).unwrap();
        let source = *loaded
            .gworlds
            .iter()
            .find(|record| record.port == gworld)
            .unwrap();
        assert_eq!(source.width, 32_768);
        let last_byte = source.base_addr + 32_767 / 8;
        loaded.memory.write_u8(last_byte, 1).unwrap();

        let reversed_ctable = ppc_alloc_handle_with_bytes(
            &mut loaded.memory,
            test_heap_cursor!(loaded),
            test_heap_limit!(loaded),
            test_handles!(loaded),
            &ppc_color_table_bytes(0x1234_5678, &[[0, 0, 0], [0xffff; 3]]).unwrap(),
        );
        assert_ne!(reversed_ctable, 0);
        loaded.cpu.gpr[3] = gworld_out_ptr;
        loaded.cpu.gpr[4] = 1;
        loaded.cpu.gpr[5] = bounds_ptr;
        loaded.cpu.gpr[6] = reversed_ctable;
        loaded.cpu.gpr[7] = 0;
        loaded.cpu.gpr[8] = CLIP_PIX;
        run_test_import(&mut loaded, PpcImportDispatcherTarget::UpdateGWorld);

        assert_eq!(loaded.cpu.gpr[3] & (1 << 31), 0);
        let updated = *loaded
            .gworlds
            .iter()
            .find(|record| record.port == gworld)
            .unwrap();
        assert_eq!(
            ppc_read_gworld_record_raw_pixel(&mut loaded.memory, updated, 32_767, 0),
            Some(0)
        );
    }

    #[test]
    fn hle_import_runner_update_gworld_dithers_direct_gradient_to_indexed_pixels() {
        const CLIP_PIX: u32 = 1 << 28;
        const DITHER_PIX: u32 = 1 << 30;

        let pef = synthetic_pef_with_import(b"NewGWorld");
        let mut loaded = load_pef_application(&pef).unwrap();
        let scratch = PPC_DATA_BASE + 0x1000;
        let bounds_ptr = scratch;
        let gworld_out_ptr = scratch + 8;
        loaded.memory.add_region(scratch, vec![0; 16]);
        ppc_write_rect(&mut loaded.memory, bounds_ptr, 0, 0, 1, 16).unwrap();
        loaded.cpu.gpr[3] = gworld_out_ptr;
        loaded.cpu.gpr[4] = 16;
        loaded.cpu.gpr[5] = bounds_ptr;
        loaded.cpu.gpr[6] = 0;
        loaded.cpu.gpr[7] = 0;
        loaded.cpu.gpr[8] = 0;
        run_test_import(&mut loaded, PpcImportDispatcherTarget::NewGWorld);
        let gworld = loaded.memory.read_u32_be(gworld_out_ptr).unwrap();
        let source = *loaded
            .gworlds
            .iter()
            .find(|record| record.port == gworld)
            .unwrap();
        let source_base = source.base_addr;
        let mut source_pixels = Vec::new();
        for x in 0..16u32 {
            let level = 12 + x / 2;
            let pixel = (level << 10) | (level << 5) | level;
            source_pixels.push(pixel);
            loaded
                .memory
                .write_u16_be(source.base_addr + x * 2, pixel as u16)
                .unwrap();
        }
        let destination_clut =
            ppc_color_table_clut_from_bytes(&ppc_default_color_table_bytes(1).unwrap()).unwrap();
        let nearest = source_pixels
            .iter()
            .copied()
            .map(|pixel| {
                let rgb = ppc_gworld_pixel_to_rgb(pixel, 16, None).unwrap();
                ppc_gworld_rgb_to_pixel(rgb, 1, Some(&destination_clut)).unwrap() as u8
            })
            .collect::<Vec<_>>();

        loaded.cpu.gpr[3] = gworld_out_ptr;
        loaded.cpu.gpr[4] = 1;
        loaded.cpu.gpr[5] = bounds_ptr;
        loaded.cpu.gpr[6] = 0;
        loaded.cpu.gpr[7] = 0;
        loaded.cpu.gpr[8] = CLIP_PIX | DITHER_PIX;
        run_test_import(&mut loaded, PpcImportDispatcherTarget::UpdateGWorld);

        assert_ne!(loaded.cpu.gpr[3] & DITHER_PIX, 0);
        assert_ne!(loaded.cpu.gpr[3] & (1 << 17), 0, "newDepth");
        assert_eq!(loaded.cpu.gpr[3] & (1 << 20), 0, "reallocPix");
        let indexed = *loaded
            .gworlds
            .iter()
            .find(|record| record.port == gworld)
            .unwrap();
        assert_eq!(indexed.base_addr, source_base);
        let bits = ppc_pixmap_bits_from_record(indexed).unwrap();
        let dithered = (0..16)
            .map(|x| ppc_read_pixmap_raw_pixel(&mut loaded.memory, bits, x, 0).unwrap() as u8)
            .collect::<Vec<_>>();
        assert_ne!(dithered, nearest);
        assert!(dithered.contains(&0));
        assert!(dithered.contains(&1));
    }

    #[test]
    fn hle_import_runner_update_gworld_allocation_preflight_failure_is_atomic() {
        const GW_FLAG_ERR: u32 = 1 << 31;
        const CLIP_PIX: u32 = 1 << 28;

        let pef = synthetic_pef_with_import(b"NewGWorld");
        let mut loaded = load_pef_application(&pef).unwrap();
        let scratch = PPC_DATA_BASE + 0x1000;
        let bounds_ptr = scratch;
        let gworld_out_ptr = scratch + 8;
        loaded.memory.add_region(scratch, vec![0; 16]);
        ppc_write_rect(&mut loaded.memory, bounds_ptr, 0, 0, 4, 4).unwrap();
        loaded.cpu.gpr[3] = gworld_out_ptr;
        loaded.cpu.gpr[4] = 1;
        loaded.cpu.gpr[5] = bounds_ptr;
        loaded.cpu.gpr[6] = 0;
        loaded.cpu.gpr[7] = 0;
        loaded.cpu.gpr[8] = 0;
        run_test_import(&mut loaded, PpcImportDispatcherTarget::NewGWorld);
        let gworld = loaded.memory.read_u32_be(gworld_out_ptr).unwrap();
        let old = *loaded
            .gworlds
            .iter()
            .find(|record| record.port == gworld)
            .unwrap();
        loaded.memory.write_u8(old.base_addr, 0xa5).unwrap();

        let old_pixel_size = old.row_bytes * old.height + old.row_bytes.max(64);
        let pixmap_before =
            ppc_memory_read_bytes(&mut loaded.memory, old.pixmap, PPC_PIXMAP_SIZE).unwrap();
        let pixels_before =
            ppc_memory_read_bytes(&mut loaded.memory, old.base_addr, old_pixel_size).unwrap();
        let ctable_handle = loaded.memory.read_u32_be(old.pixmap + 42).unwrap();
        let ctable_master_before = loaded.memory.read_u32_be(ctable_handle);
        let ctable_before = ppc_copy_color_table_bytes(&mut loaded.memory, ctable_handle).unwrap();
        let port_rect_before = ppc_memory_read_bytes(&mut loaded.memory, old.port + 16, 8).unwrap();
        let handles_before = test_handle_records!(loaded).clone();
        let gworlds_before = loaded.gworlds.clone();
        let allocations_before = loaded.toolbox_startup.gworld_allocations.clone();
        let heap_cursor_before = loaded.heap_cursor();
        let region_count_before = loaded.memory.region_count();

        ppc_write_rect(&mut loaded.memory, bounds_ptr, 0, 0, 64, 64).unwrap();
        let new_row_bytes = ppc_row_bytes(64, 8).unwrap();
        let pixel_allocation_size = new_row_bytes * 64 + new_row_bytes.max(64);
        let table_size = u32::try_from(ppc_default_color_table_bytes(8).unwrap().len()).unwrap();
        // Set the limit exactly at the end of the requested allocation
        // sequence. The allocator requires the final cursor to remain below
        // the limit, so UpdateGWorld must reject the request before mutating
        // the live world.
        let allocation_limit = heap_cursor_before
            + ppc_allocation_size(table_size).unwrap()
            + ppc_allocation_size(pixel_allocation_size).unwrap();
        loaded.set_heap_limit(allocation_limit);
        loaded.cpu.gpr[3] = gworld_out_ptr;
        loaded.cpu.gpr[4] = 8;
        loaded.cpu.gpr[5] = bounds_ptr;
        loaded.cpu.gpr[6] = 0;
        loaded.cpu.gpr[7] = 0;
        loaded.cpu.gpr[8] = CLIP_PIX;
        run_test_import(&mut loaded, PpcImportDispatcherTarget::UpdateGWorld);

        assert_eq!(loaded.cpu.gpr[3], GW_FLAG_ERR);
        assert_eq!(loaded.last_mem_error(), PPC_MEM_FULL_ERR);
        assert_eq!(loaded.heap_cursor(), heap_cursor_before);
        assert_eq!(test_handle_records!(loaded), handles_before);
        assert_eq!(loaded.gworlds, gworlds_before);
        assert_eq!(
            loaded.toolbox_startup.gworld_allocations,
            allocations_before
        );
        assert_eq!(loaded.memory.region_count(), region_count_before);
        assert_eq!(loaded.memory.read_u32_be(gworld_out_ptr), Some(gworld));
        assert_eq!(
            ppc_memory_read_bytes(&mut loaded.memory, old.pixmap, PPC_PIXMAP_SIZE),
            Some(pixmap_before)
        );
        assert_eq!(
            ppc_memory_read_bytes(&mut loaded.memory, old.base_addr, old_pixel_size),
            Some(pixels_before)
        );
        assert_eq!(
            loaded.memory.read_u32_be(ctable_handle),
            ctable_master_before
        );
        assert_eq!(
            ppc_copy_color_table_bytes(&mut loaded.memory, ctable_handle),
            Some(ctable_before)
        );
        assert_eq!(
            ppc_memory_read_bytes(&mut loaded.memory, old.port + 16, 8),
            Some(port_rect_before)
        );
        run_test_import(&mut loaded, PpcImportDispatcherTarget::QDError);
        assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_MEM_FULL_ERR));
    }

    #[test]
    fn hle_import_runner_update_gworld_rejects_invalid_inputs_atomically() {
        const GW_FLAG_ERR: u32 = 1 << 31;

        let pef = synthetic_pef_with_import(b"NewGWorld");
        let mut loaded = load_pef_application(&pef).unwrap();
        let scratch = PPC_DATA_BASE + 0x1000;
        let bounds_ptr = scratch;
        let gworld_out_ptr = scratch + 8;
        loaded.memory.add_region(scratch, vec![0; 16]);
        ppc_write_rect(&mut loaded.memory, bounds_ptr, 0, 0, 2, 2).unwrap();
        loaded.cpu.gpr[3] = gworld_out_ptr;
        loaded.cpu.gpr[4] = 8;
        loaded.cpu.gpr[5] = bounds_ptr;
        loaded.cpu.gpr[6] = 0;
        loaded.cpu.gpr[7] = 0;
        loaded.cpu.gpr[8] = 0;
        run_test_import(&mut loaded, PpcImportDispatcherTarget::NewGWorld);
        let gworld = loaded.memory.read_u32_be(gworld_out_ptr).unwrap();
        let old = *loaded
            .gworlds
            .iter()
            .find(|record| record.port == gworld)
            .unwrap();
        loaded.memory.write_u8(old.base_addr, 0x5a).unwrap();

        let invalid_ctable = ppc_alloc_handle(
            &mut loaded.memory,
            test_heap_cursor!(loaded),
            test_heap_limit!(loaded),
            test_handles!(loaded),
            8,
            true,
        );
        assert_ne!(invalid_ctable, 0);
        loaded.memory.write_u32_be(invalid_ctable, 0).unwrap();

        let pixel_size = old.row_bytes * old.height + old.row_bytes.max(64);
        let pixmap_before =
            ppc_memory_read_bytes(&mut loaded.memory, old.pixmap, PPC_PIXMAP_SIZE).unwrap();
        let pixels_before =
            ppc_memory_read_bytes(&mut loaded.memory, old.base_addr, pixel_size).unwrap();
        let old_ctable = loaded.memory.read_u32_be(old.pixmap + 42).unwrap();
        let ctable_before = ppc_copy_color_table_bytes(&mut loaded.memory, old_ctable).unwrap();
        let handles_before = test_handle_records!(loaded).clone();
        let gworlds_before = loaded.gworlds.clone();
        let allocations_before = loaded.toolbox_startup.gworld_allocations.clone();
        let heap_cursor_before = loaded.heap_cursor();
        let region_count_before = loaded.memory.region_count();

        // A non-NIL cTable must be used as supplied. Falling back to the
        // default table for an invalid explicit handle would silently accept
        // a different request than the caller made.
        loaded.cpu.gpr[3] = gworld_out_ptr;
        loaded.cpu.gpr[4] = 8;
        loaded.cpu.gpr[5] = bounds_ptr;
        loaded.cpu.gpr[6] = invalid_ctable;
        loaded.cpu.gpr[7] = 0;
        loaded.cpu.gpr[8] = 1 << 28;
        run_test_import(&mut loaded, PpcImportDispatcherTarget::UpdateGWorld);
        assert_eq!(loaded.cpu.gpr[3], GW_FLAG_ERR);
        assert_eq!(loaded.last_mem_error(), PPC_PARAM_ERR);
        assert_eq!(*loaded.toolbox_startup.last_quickdraw_error, PPC_PARAM_ERR);
        run_test_import(&mut loaded, PpcImportDispatcherTarget::QDError);
        assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_PARAM_ERR));

        // PixMap.rowBytes has only a 14-bit byte-count payload. Reject a
        // direct image whose scanline cannot be represented instead of
        // truncating the live record while retaining a larger host rowBytes.
        ppc_write_rect(&mut loaded.memory, bounds_ptr, 0, i16::MIN, 1, i16::MAX).unwrap();
        loaded.cpu.gpr[3] = gworld_out_ptr;
        loaded.cpu.gpr[4] = 32;
        loaded.cpu.gpr[5] = bounds_ptr;
        loaded.cpu.gpr[6] = 0;
        loaded.cpu.gpr[7] = 0;
        loaded.cpu.gpr[8] = 1 << 28;
        run_test_import(&mut loaded, PpcImportDispatcherTarget::UpdateGWorld);
        assert_eq!(loaded.cpu.gpr[3], GW_FLAG_ERR);
        assert_eq!(loaded.last_mem_error(), PPC_PARAM_ERR);
        run_test_import(&mut loaded, PpcImportDispatcherTarget::QDError);
        assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_PARAM_ERR));

        ppc_write_rect(&mut loaded.memory, bounds_ptr, 0, 0, 2, 2).unwrap();
        for flags in [(1 << 28) | (1 << 29), 1 << 30, 1 << 1, 1 << 16] {
            loaded.cpu.gpr[3] = gworld_out_ptr;
            loaded.cpu.gpr[4] = 8;
            loaded.cpu.gpr[5] = bounds_ptr;
            loaded.cpu.gpr[6] = 0;
            loaded.cpu.gpr[7] = 0;
            loaded.cpu.gpr[8] = flags;
            run_test_import(&mut loaded, PpcImportDispatcherTarget::UpdateGWorld);
            assert_eq!(loaded.cpu.gpr[3], GW_FLAG_ERR, "flags={flags:#010x}");
            assert_eq!(loaded.last_mem_error(), PPC_PARAM_ERR);
            run_test_import(&mut loaded, PpcImportDispatcherTarget::QDError);
            assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_PARAM_ERR));
        }

        loaded.cpu.gpr[3] = gworld_out_ptr;
        loaded.cpu.gpr[4] = u32::from(u16::MAX);
        loaded.cpu.gpr[5] = bounds_ptr;
        loaded.cpu.gpr[6] = 0;
        loaded.cpu.gpr[7] = 0;
        loaded.cpu.gpr[8] = 0;
        run_test_import(&mut loaded, PpcImportDispatcherTarget::UpdateGWorld);
        assert_eq!(loaded.cpu.gpr[3], GW_FLAG_ERR);
        assert_eq!(loaded.last_mem_error(), PPC_C_DEPTH_ERR);
        run_test_import(&mut loaded, PpcImportDispatcherTarget::QDError);
        assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_C_DEPTH_ERR));

        loaded.cpu.gpr[3] = gworld_out_ptr;
        loaded.cpu.gpr[4] = 8;
        loaded.cpu.gpr[5] = bounds_ptr;
        loaded.cpu.gpr[6] = 0;
        loaded.cpu.gpr[7] = 0;
        loaded.cpu.gpr[8] = (1 << 3) | (1 << 28); // keepLocal + clipPix
        run_test_import(&mut loaded, PpcImportDispatcherTarget::UpdateGWorld);
        assert_eq!(loaded.cpu.gpr[3] & GW_FLAG_ERR, 0);
        assert_eq!(*loaded.toolbox_startup.last_quickdraw_error, PPC_NO_ERR);

        assert_eq!(loaded.heap_cursor(), heap_cursor_before);
        assert_eq!(test_handle_records!(loaded), handles_before);
        assert_eq!(loaded.gworlds, gworlds_before);
        assert_eq!(
            loaded.toolbox_startup.gworld_allocations,
            allocations_before
        );
        assert_eq!(loaded.memory.region_count(), region_count_before);
        assert_eq!(loaded.memory.read_u32_be(gworld_out_ptr), Some(gworld));
        assert_eq!(
            ppc_memory_read_bytes(&mut loaded.memory, old.pixmap, PPC_PIXMAP_SIZE),
            Some(pixmap_before)
        );
        assert_eq!(
            ppc_memory_read_bytes(&mut loaded.memory, old.base_addr, pixel_size),
            Some(pixels_before)
        );
        assert_eq!(
            ppc_copy_color_table_bytes(&mut loaded.memory, old_ctable),
            Some(ctable_before)
        );
        assert_eq!(loaded.memory.read_u32_be(invalid_ctable), Some(0));
    }

    #[test]
    fn hle_import_runner_update_gworld_reuse_preflight_is_atomic() {
        const GW_FLAG_ERR: u32 = 1 << 31;

        let pef = synthetic_pef_with_import(b"NewGWorld");
        let mut loaded = load_pef_application(&pef).unwrap();
        let scratch = PPC_DATA_BASE + 0x1000;
        let bounds_ptr = scratch;
        let gworld_out_ptr = scratch + 8;
        loaded.memory.add_region(scratch, vec![0; 16]);
        ppc_write_rect(&mut loaded.memory, bounds_ptr, 0, 0, 2, 2).unwrap();
        loaded.cpu.gpr[3] = gworld_out_ptr;
        loaded.cpu.gpr[4] = 8;
        loaded.cpu.gpr[5] = bounds_ptr;
        loaded.cpu.gpr[6] = 0;
        loaded.cpu.gpr[7] = 0;
        loaded.cpu.gpr[8] = 0;
        run_test_import(&mut loaded, PpcImportDispatcherTarget::NewGWorld);
        let gworld = loaded.memory.read_u32_be(gworld_out_ptr).unwrap();
        let old = *loaded
            .gworlds
            .iter()
            .find(|record| record.port == gworld)
            .unwrap();
        loaded.memory.write_u8(old.base_addr, 0x5a).unwrap();
        let private_ctable = loaded.memory.read_u32_be(old.pixmap + 42).unwrap();
        loaded.memory.write_u32_be(private_ctable, 0).unwrap();

        let replacement_colors = vec![[0x1111, 0x2222, 0x3333]; 256];
        let replacement = ppc_alloc_handle_with_bytes(
            &mut loaded.memory,
            test_heap_cursor!(loaded),
            test_heap_limit!(loaded),
            test_handles!(loaded),
            &ppc_color_table_bytes(0x9876_5432, &replacement_colors).unwrap(),
        );
        assert_ne!(replacement, 0);
        let pixmap_before =
            ppc_memory_read_bytes(&mut loaded.memory, old.pixmap, PPC_PIXMAP_SIZE).unwrap();
        let pixels_before = ppc_memory_read_bytes(
            &mut loaded.memory,
            old.base_addr,
            old.row_bytes * old.height + old.row_bytes.max(64),
        )
        .unwrap();
        let handles_before = test_handle_records!(loaded).clone();
        let gworlds_before = loaded.gworlds.clone();
        let allocations_before = loaded.toolbox_startup.gworld_allocations.clone();
        let heap_cursor_before = loaded.heap_cursor();

        loaded.cpu.gpr[3] = gworld_out_ptr;
        loaded.cpu.gpr[4] = 8;
        loaded.cpu.gpr[5] = bounds_ptr;
        loaded.cpu.gpr[6] = replacement;
        loaded.cpu.gpr[7] = 0;
        loaded.cpu.gpr[8] = 0;
        run_test_import(&mut loaded, PpcImportDispatcherTarget::UpdateGWorld);

        assert_eq!(loaded.cpu.gpr[3], GW_FLAG_ERR);
        assert_eq!(loaded.last_mem_error(), PPC_PARAM_ERR);
        assert_eq!(loaded.memory.read_u32_be(private_ctable), Some(0));
        assert_eq!(loaded.heap_cursor(), heap_cursor_before);
        assert_eq!(test_handle_records!(loaded), handles_before);
        assert_eq!(loaded.gworlds, gworlds_before);
        assert_eq!(
            loaded.toolbox_startup.gworld_allocations,
            allocations_before
        );
        assert_eq!(
            ppc_memory_read_bytes(&mut loaded.memory, old.pixmap, PPC_PIXMAP_SIZE),
            Some(pixmap_before)
        );
        assert_eq!(
            ppc_memory_read_bytes(
                &mut loaded.memory,
                old.base_addr,
                old.row_bytes * old.height + old.row_bytes.max(64),
            ),
            Some(pixels_before)
        );
        run_test_import(&mut loaded, PpcImportDispatcherTarget::QDError);
        assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_PARAM_ERR));
    }

    #[test]
    fn hle_import_runner_dispose_gworld_reclaims_top_allocation() {
        let pef = synthetic_pef_with_import(b"NewGWorld");
        let mut loaded = load_pef_application(&pef).unwrap();
        let scratch = PPC_DATA_BASE + 0x1000;
        let bounds_ptr = scratch;
        let gworld_out_ptr = scratch + 8;
        loaded.memory.add_region(scratch, vec![0; 16]);
        ppc_write_rect(&mut loaded.memory, bounds_ptr, 0, 0, 16, 16).unwrap();
        let heap_cursor = loaded.heap_cursor();
        loaded.cpu.gpr[3] = gworld_out_ptr;
        loaded.cpu.gpr[4] = 8;
        loaded.cpu.gpr[5] = bounds_ptr;
        loaded.cpu.gpr[6] = 0;
        loaded.cpu.gpr[7] = 0;
        loaded.cpu.gpr[8] = 0;

        let probe = loaded.run_with_hle_imports(64);
        assert_eq!(probe.unsupported_import_index, None);
        assert_ne!(loaded.heap_cursor(), heap_cursor);
        let gworld = loaded.memory.read_u32_be(gworld_out_ptr).unwrap();

        loaded.cpu.pc = loaded.entry_pc;
        loaded.cpu.lr = PPC_HALT_PC;
        loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::DisposeGWorld;
        loaded.cpu.gpr[3] = gworld;

        let probe = loaded.run_with_hle_imports(64);

        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(loaded.heap_cursor(), heap_cursor);
        assert!(!loaded.gworlds.iter().any(|record| record.port == gworld));
    }

    #[test]
    fn hle_import_runner_dispose_gworld_reuses_non_tail_allocation() {
        let pef = synthetic_pef_with_import(b"NewGWorld");
        let mut loaded = load_pef_application(&pef).unwrap();
        let scratch = PPC_DATA_BASE + 0x1000;
        let bounds_ptr = scratch;
        let gworld_out_ptr = scratch + 8;
        loaded.memory.add_region(scratch, vec![0; 16]);
        ppc_write_rect(&mut loaded.memory, bounds_ptr, 0, 0, 70, 24).unwrap();
        loaded.cpu.gpr[3] = gworld_out_ptr;
        loaded.cpu.gpr[4] = 8;
        loaded.cpu.gpr[5] = bounds_ptr;
        loaded.cpu.gpr[6] = 0;
        loaded.cpu.gpr[7] = PPC_MAIN_GDEVICE;
        loaded.cpu.gpr[8] = 1 << 1;
        run_test_import(&mut loaded, PpcImportDispatcherTarget::NewGWorld);
        let first_port = loaded.memory.read_u32_be(gworld_out_ptr).unwrap();
        let first_base = loaded
            .gworlds
            .iter()
            .find(|record| record.port == first_port)
            .unwrap()
            .base_addr;

        let retained = loaded.with_ptr_allocator_records(|loaded, ptrs, free_ptr_blocks| {
            ppc_alloc_ptr(
                &mut loaded.memory,
                test_heap_cursor!(loaded),
                test_heap_limit!(loaded),
                ptrs,
                free_ptr_blocks,
                4096,
                true,
            )
        });
        assert_ne!(retained, 0);
        let heap_cursor_with_retained_data = loaded.heap_cursor();

        loaded.cpu.gpr[3] = first_port;
        run_test_import(&mut loaded, PpcImportDispatcherTarget::DisposeGWorld);

        assert_eq!(loaded.heap_cursor(), heap_cursor_with_retained_data);
        assert!(loaded
            .free_ptr_blocks()
            .iter()
            .any(|record| record.ptr == first_base));

        loaded.cpu.gpr[3] = gworld_out_ptr;
        loaded.cpu.gpr[4] = 8;
        loaded.cpu.gpr[5] = bounds_ptr;
        loaded.cpu.gpr[6] = 0;
        loaded.cpu.gpr[7] = PPC_MAIN_GDEVICE;
        loaded.cpu.gpr[8] = 1 << 1;
        run_test_import(&mut loaded, PpcImportDispatcherTarget::NewGWorld);
        let second_port = loaded.memory.read_u32_be(gworld_out_ptr).unwrap();
        let second_base = loaded
            .gworlds
            .iter()
            .find(|record| record.port == second_port)
            .unwrap()
            .base_addr;

        assert_eq!(second_base, first_base);
        assert_eq!(loaded.heap_cursor(), heap_cursor_with_retained_data);
    }

    #[test]
    fn hle_import_runner_update_then_dispose_preserves_unrelated_heap_tail() {
        const CLIP_PIX: u32 = 1 << 28;

        let pef = synthetic_pef_with_import(b"NewGWorld");
        let mut loaded = load_pef_application(&pef).unwrap();
        let scratch = PPC_DATA_BASE + 0x1000;
        let bounds_ptr = scratch;
        let gworld_out_ptr = scratch + 8;
        loaded.memory.add_region(scratch, vec![0; 16]);
        ppc_write_rect(&mut loaded.memory, bounds_ptr, 0, 0, 2, 2).unwrap();
        let initial_handle_count = test_handle_records!(loaded).len();
        loaded.cpu.gpr[3] = gworld_out_ptr;
        loaded.cpu.gpr[4] = 1;
        loaded.cpu.gpr[5] = bounds_ptr;
        loaded.cpu.gpr[6] = 0;
        loaded.cpu.gpr[7] = 0;
        loaded.cpu.gpr[8] = 0;
        run_test_import(&mut loaded, PpcImportDispatcherTarget::NewGWorld);
        let gworld = loaded.memory.read_u32_be(gworld_out_ptr).unwrap();
        let private_ctable = loaded
            .toolbox_startup
            .gworld_allocations
            .get(&gworld)
            .unwrap()
            .ctable_handle;
        assert_ne!(private_ctable, 0);
        assert_eq!(test_handle_records!(loaded).len(), initial_handle_count + 1);

        let sentinel_bytes = b"unrelated allocation";
        let sentinel = ppc_alloc_handle_with_bytes(
            &mut loaded.memory,
            test_heap_cursor!(loaded),
            test_heap_limit!(loaded),
            test_handles!(loaded),
            sentinel_bytes,
        );
        assert_ne!(sentinel, 0);
        let sentinel_ptr = loaded.memory.read_u32_be(sentinel).unwrap();
        let cursor_before_update = loaded.heap_cursor();
        let handle_count_with_sentinel = test_handle_records!(loaded).len();

        for (depth, width, height) in [
            (8u32, 64i16, 64i16),
            (16, 80, 80),
            (4, 32, 32),
            (32, 96, 96),
            (8, 8, 8),
        ] {
            let before = *loaded
                .gworlds
                .iter()
                .find(|record| record.port == gworld)
                .unwrap();
            let allocation_before = *loaded
                .toolbox_startup
                .gworld_allocations
                .get(&gworld)
                .unwrap();
            let heap_cursor_before = loaded.heap_cursor();
            ppc_write_rect(&mut loaded.memory, bounds_ptr, 0, 0, height, width).unwrap();
            loaded.cpu.gpr[3] = gworld_out_ptr;
            loaded.cpu.gpr[4] = depth;
            loaded.cpu.gpr[5] = bounds_ptr;
            loaded.cpu.gpr[6] = 0;
            loaded.cpu.gpr[7] = 0;
            loaded.cpu.gpr[8] = CLIP_PIX;
            run_test_import(&mut loaded, PpcImportDispatcherTarget::UpdateGWorld);
            assert_eq!(loaded.cpu.gpr[3] & (1 << 31), 0, "depth={depth}");
            assert_eq!(
                test_handle_records!(loaded).len(),
                handle_count_with_sentinel
            );
            let result = loaded.cpu.gpr[3];
            let updated = *loaded
                .gworlds
                .iter()
                .find(|record| record.port == gworld)
                .unwrap();
            let new_row_bytes = ppc_row_bytes(width as u32, depth).unwrap();
            let required_capacity =
                ppc_allocation_size(new_row_bytes * height as u32 + new_row_bytes.max(64)).unwrap();
            if depth > 8
                && before
                    .base_addr
                    .checked_add(allocation_before.pixel_capacity)
                    == Some(heap_cursor_before)
                && required_capacity > allocation_before.pixel_capacity
            {
                assert_eq!(updated.base_addr, before.base_addr);
                assert_eq!(result & (1 << 20), 0, "reallocPix");
                assert_eq!(
                    loaded.heap_cursor() - heap_cursor_before,
                    required_capacity - allocation_before.pixel_capacity
                );
            }
            let allocation = loaded
                .toolbox_startup
                .gworld_allocations
                .get(&gworld)
                .unwrap();
            assert_eq!(allocation.ctable_handle, private_ctable);
            assert_eq!(allocation.origin_base, cursor_before_update);
            let pixmap = loaded
                .gworlds
                .iter()
                .find(|record| record.port == gworld)
                .unwrap()
                .pixmap;
            if depth <= 8 {
                assert_eq!(loaded.memory.read_u32_be(pixmap + 42), Some(private_ctable));
            } else {
                assert_eq!(loaded.memory.read_u32_be(pixmap + 42), Some(0));
            }
        }

        loaded.cpu.gpr[3] = gworld;
        run_test_import(&mut loaded, PpcImportDispatcherTarget::DisposeGWorld);

        assert_eq!(loaded.heap_cursor(), cursor_before_update);
        assert_eq!(loaded.memory.read_u32_be(sentinel), Some(sentinel_ptr));
        assert_eq!(
            ppc_memory_read_bytes(
                &mut loaded.memory,
                sentinel_ptr,
                u32::try_from(sentinel_bytes.len()).unwrap(),
            ),
            Some(sentinel_bytes.to_vec())
        );
        assert!(loaded
            .handles()
            .iter()
            .any(|record| record.handle == sentinel));
        assert!(!loaded
            .handles()
            .iter()
            .any(|record| record.handle == private_ctable));
        assert_eq!(test_handle_records!(loaded).len(), initial_handle_count + 1);
        assert!(!loaded.gworlds.iter().any(|record| record.port == gworld));
        assert!(!loaded
            .toolbox_startup
            .gworld_allocations
            .contains_key(&gworld));
    }

    #[test]
    fn explicit_foreground_indices_follow_port_switch_and_valid_disposal() {
        let pef = synthetic_pef_with_import(b"SetGWorld");
        let mut loaded = load_pef_application(&pef).unwrap();
        let port = PPC_DATA_BASE + 0x1000;
        loaded
            .memory
            .add_region(port, vec![0; PPC_CGRAF_PORT_SIZE as usize]);
        loaded.memory.write_u16_be(port + 6, 0xc000).unwrap();
        let main_color = PpcRgbColor {
            red: 0x1111,
            green: 0x2222,
            blue: 0x3333,
        };
        let port_color = PpcRgbColor {
            red: 0xaaaa,
            green: 0xbbbb,
            blue: 0xcccc,
        };
        ppc_write_port_rgb_color(
            &mut loaded.memory,
            PPC_MAIN_GWORLD,
            PPC_CGRAF_PORT_RGB_FG_COLOR_OFFSET,
            main_color,
        )
        .unwrap();
        ppc_write_port_rgb_color(
            &mut loaded.memory,
            port,
            PPC_CGRAF_PORT_RGB_FG_COLOR_OFFSET,
            port_color,
        )
        .unwrap();
        let record = PpcGWorldRecord {
            ui_theme: crate::ui_theme::UiThemeId::ClassicSystem7,
            port,
            pixmap_handle: 0,
            pixmap: 0,
            base_addr: port + PPC_CGRAF_PORT_SIZE,
            gdevice: PPC_MAIN_GDEVICE,
            width: 1,
            height: 1,
            depth: 8,
            row_bytes: 1,
            pixels_locked: false,
            pixels_no_purge: false,
        };
        loaded.gworlds.push(record);
        loaded.quickdraw_fore_indices.insert(PPC_MAIN_GWORLD, 11);
        loaded.quickdraw_fore_indices.insert(port, 22);
        loaded.cpu.gpr[3] = port;
        loaded.cpu.gpr[4] = 0;

        loaded.run_with_hle_imports(64);

        assert_eq!(*loaded.current_gworld, port);
        assert_eq!(loaded.quickdraw_fore_color, port_color);
        assert_eq!(loaded.quickdraw_fore_indices.get(&port), Some(&22));

        loaded.cpu.pc = loaded.entry_pc;
        loaded.cpu.lr = PPC_HALT_PC;
        loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::CloseCPort;
        loaded.cpu.gpr[3] = port;
        loaded.run_with_hle_imports(64);

        assert_eq!(*loaded.current_gworld, PPC_MAIN_GWORLD);
        assert_eq!(loaded.quickdraw_fore_color, main_color);
        assert_eq!(
            loaded.quickdraw_fore_indices.get(&PPC_MAIN_GWORLD),
            Some(&11)
        );
        assert!(!loaded.quickdraw_fore_indices.contains_key(&port));

        loaded.cpu.pc = loaded.entry_pc;
        loaded.cpu.lr = PPC_HALT_PC;
        loaded.cpu.gpr[3] = PPC_MAIN_GWORLD;
        loaded.run_with_hle_imports(64);
        assert_eq!(
            loaded.quickdraw_fore_indices.get(&PPC_MAIN_GWORLD),
            Some(&11)
        );

        loaded.gworlds.push(record);
        loaded
            .current_gworld
            .with_mut(|current_gworld| *current_gworld = port);
        loaded.quickdraw_fore_color = port_color;
        loaded.quickdraw_fore_indices.insert(port, 22);
        loaded.cpu.pc = loaded.entry_pc;
        loaded.cpu.lr = PPC_HALT_PC;
        loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::DisposeGWorld;
        loaded.cpu.gpr[3] = port;
        loaded.run_with_hle_imports(64);

        assert_eq!(*loaded.current_gworld, PPC_MAIN_GWORLD);
        assert_eq!(loaded.quickdraw_fore_color, main_color);
        assert!(!loaded.quickdraw_fore_indices.contains_key(&port));
        assert_eq!(
            loaded.quickdraw_fore_indices.get(&PPC_MAIN_GWORLD),
            Some(&11)
        );

        loaded.cpu.pc = loaded.entry_pc;
        loaded.cpu.lr = PPC_HALT_PC;
        loaded.cpu.gpr[3] = PPC_MAIN_GWORLD;
        loaded.run_with_hle_imports(64);
        assert_eq!(
            loaded.quickdraw_fore_indices.get(&PPC_MAIN_GWORLD),
            Some(&11)
        );
    }

    #[test]
    fn hle_import_runner_new_gworld_heap_full_does_not_partially_allocate() {
        let pef = synthetic_pef_with_import(b"NewGWorld");
        let mut loaded = load_pef_application(&pef).unwrap();
        let scratch = PPC_DATA_BASE + 0x1000;
        let bounds_ptr = scratch;
        let gworld_out_ptr = scratch + 8;
        loaded.memory.add_region(scratch, vec![0; 16]);
        ppc_write_rect(&mut loaded.memory, bounds_ptr, 0, 0, 64, 128).unwrap();
        loaded
            .memory
            .write_u32_be(gworld_out_ptr, 0xdead_beef)
            .unwrap();
        let heap_cursor = loaded.heap_cursor();
        let region_count = loaded.memory.region_count();
        let gworld_count = loaded.gworlds.len();
        let buffer_size = ppc_row_bytes(128, 16).unwrap() * 64;
        let pixel_allocation_size = buffer_size + ppc_row_bytes(128, 16).unwrap();
        let required = ppc_heap_allocation_sequence_size(&[
            pixel_allocation_size,
            PPC_PIXMAP_SIZE,
            4,
            PPC_CGRAF_PORT_SIZE,
        ])
        .unwrap();
        loaded.set_heap_limit(heap_cursor + required - 4);
        assert_eq!(loaded.memory.read_u8(heap_cursor), Some(0));
        loaded.cpu.gpr[3] = gworld_out_ptr;
        loaded.cpu.gpr[4] = 16;
        loaded.cpu.gpr[5] = bounds_ptr;
        loaded.cpu.gpr[6] = 0;
        loaded.cpu.gpr[7] = 0;
        loaded.cpu.gpr[8] = 0;

        let probe = loaded.run_with_hle_imports(64);

        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(loaded.cpu.gpr[3] as u16 as i16, PPC_MEM_FULL_ERR);
        assert_eq!(loaded.last_mem_error(), PPC_MEM_FULL_ERR);
        assert_eq!(loaded.memory.read_u32_be(gworld_out_ptr), Some(0xdead_beef));
        assert_eq!(loaded.heap_cursor(), heap_cursor);
        assert_eq!(loaded.memory.region_count(), region_count);
        assert_eq!(loaded.memory.read_u8(heap_cursor), Some(0));
        assert_eq!(loaded.gworlds.len(), gworld_count);
    }

    #[test]
    fn hle_import_runner_new_gworld_param_err_does_not_partially_allocate() {
        let pef = synthetic_pef_with_import(b"NewGWorld");
        let mut loaded = load_pef_application(&pef).unwrap();
        let scratch = PPC_DATA_BASE + 0x1000;
        let bounds_ptr = scratch;
        loaded.memory.add_region(scratch, vec![0; 8]);
        ppc_write_rect(&mut loaded.memory, bounds_ptr, 0, 0, 64, 128).unwrap();
        let heap_cursor = loaded.heap_cursor();
        let region_count = loaded.memory.region_count();
        let gworld_count = loaded.gworlds.len();
        assert_eq!(loaded.memory.read_u8(heap_cursor), Some(0));
        loaded.cpu.gpr[3] = 0x06ff_0000;
        loaded.cpu.gpr[4] = 16;
        loaded.cpu.gpr[5] = bounds_ptr;
        loaded.cpu.gpr[6] = 0;
        loaded.cpu.gpr[7] = 0;
        loaded.cpu.gpr[8] = 0;

        let probe = loaded.run_with_hle_imports(64);

        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(loaded.cpu.gpr[3] as u16 as i16, PPC_PARAM_ERR);
        assert_eq!(loaded.last_mem_error(), PPC_PARAM_ERR);
        assert_eq!(loaded.heap_cursor(), heap_cursor);
        assert_eq!(loaded.memory.region_count(), region_count);
        assert_eq!(loaded.memory.read_u8(heap_cursor), Some(0));
        assert_eq!(loaded.gworlds.len(), gworld_count);
    }

    #[test]
    fn hle_import_runner_new_gworld_rejects_explicit_device_without_depth_atomically() {
        let pef = synthetic_pef_with_import(b"NewGWorld");
        let mut loaded = load_pef_application(&pef).unwrap();
        let scratch = PPC_DATA_BASE + 0x1000;
        let bounds_ptr = scratch;
        let gworld_out_ptr = scratch + 8;
        let gdevice_handle = PPC_DATA_BASE + 0x5000;
        let gdevice = PPC_DATA_BASE + 0x5100;
        let pixmap_handle = PPC_DATA_BASE + 0x5200;
        let pixmap = PPC_DATA_BASE + 0x5300;
        loaded.memory.add_region(scratch, vec![0; 16]);
        loaded.memory.add_region(gdevice_handle, vec![0; 4]);
        loaded.memory.add_region(gdevice, vec![0; 64]);
        loaded.memory.add_region(pixmap_handle, vec![0; 4]);
        loaded
            .memory
            .add_region(pixmap, vec![0; PPC_PIXMAP_SIZE as usize]);
        ppc_write_rect(&mut loaded.memory, bounds_ptr, 0, 0, 2, 2).unwrap();
        loaded
            .memory
            .write_u32_be(gworld_out_ptr, 0xdead_beef)
            .unwrap();
        loaded.memory.write_u32_be(gdevice_handle, gdevice).unwrap();
        loaded
            .memory
            .write_u32_be(gdevice + 22, pixmap_handle)
            .unwrap();
        loaded.memory.write_u32_be(pixmap_handle, pixmap).unwrap();
        assert_eq!(loaded.memory.read_u16_be(pixmap + 32), Some(0));
        let heap_cursor_before = loaded.heap_cursor();
        let handles_before = test_handle_records!(loaded).clone();
        let gworlds_before = loaded.gworlds.clone();
        let region_count_before = loaded.memory.region_count();

        loaded.cpu.gpr[3] = gworld_out_ptr;
        loaded.cpu.gpr[4] = 8;
        loaded.cpu.gpr[5] = bounds_ptr;
        loaded.cpu.gpr[6] = 0;
        loaded.cpu.gpr[7] = gdevice_handle;
        loaded.cpu.gpr[8] = 1 << 1; // noNewDevice
        run_test_import(&mut loaded, PpcImportDispatcherTarget::NewGWorld);

        assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_C_DEPTH_ERR));
        assert_eq!(loaded.last_mem_error(), PPC_C_DEPTH_ERR);
        run_test_import(&mut loaded, PpcImportDispatcherTarget::QDError);
        assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_C_DEPTH_ERR));
        assert_eq!(loaded.memory.read_u32_be(gworld_out_ptr), Some(0xdead_beef));
        assert_eq!(loaded.heap_cursor(), heap_cursor_before);
        assert_eq!(test_handle_records!(loaded), handles_before);
        assert_eq!(loaded.gworlds, gworlds_before);
        assert_eq!(loaded.memory.region_count(), region_count_before);
        assert_eq!(loaded.memory.read_u16_be(pixmap + 32), Some(0));

        loaded.cpu.gpr[3] = gworld_out_ptr;
        loaded.cpu.gpr[4] = u32::from(u16::MAX); // -1 is not depth zero
        loaded.cpu.gpr[5] = bounds_ptr;
        loaded.cpu.gpr[6] = 0;
        loaded.cpu.gpr[7] = 0;
        loaded.cpu.gpr[8] = 0;
        run_test_import(&mut loaded, PpcImportDispatcherTarget::NewGWorld);
        assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_C_DEPTH_ERR));
        assert_eq!(*loaded.toolbox_startup.last_quickdraw_error, PPC_C_DEPTH_ERR);
        assert_eq!(loaded.memory.read_u32_be(gworld_out_ptr), Some(0xdead_beef));
        assert_eq!(loaded.heap_cursor(), heap_cursor_before);
        assert_eq!(test_handle_records!(loaded), handles_before);
        assert_eq!(loaded.gworlds, gworlds_before);
    }

    #[test]
    fn hle_import_runner_new_gworld_sanitizes_inverted_pixmap_bounds() {
        let pef = synthetic_pef_with_import(b"NewGWorld");
        let mut loaded = load_pef_application(&pef).unwrap();
        let scratch = PPC_DATA_BASE + 0x1000;
        let bounds_ptr = scratch;
        let gworld_out_ptr = scratch + 8;
        loaded.memory.add_region(scratch, vec![0; 16]);
        ppc_write_rect(&mut loaded.memory, bounds_ptr, 0, 0, -21116, -31837).unwrap();
        let gworld_count = loaded.gworlds.len();
        loaded.cpu.gpr[3] = gworld_out_ptr;
        loaded.cpu.gpr[4] = 8;
        loaded.cpu.gpr[5] = bounds_ptr;
        loaded.cpu.gpr[6] = 0;
        loaded.cpu.gpr[7] = 0;
        loaded.cpu.gpr[8] = 0;

        let probe = loaded.run_with_hle_imports(64);

        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(loaded.cpu.gpr[3] as u16 as i16, PPC_NO_ERR);
        assert_eq!(loaded.last_mem_error(), PPC_NO_ERR);
        assert_eq!(loaded.gworlds.len(), gworld_count + 1);
        let gworld = loaded.memory.read_u32_be(gworld_out_ptr).unwrap();
        let record = loaded
            .gworlds
            .iter()
            .find(|record| record.port == gworld)
            .unwrap();
        assert_eq!(record.width, 1);
        assert_eq!(record.height, 1);
        assert_eq!(loaded.memory.read_u16_be(record.pixmap + 6), Some(0));
        assert_eq!(loaded.memory.read_u16_be(record.pixmap + 8), Some(0));
        assert_eq!(loaded.memory.read_u16_be(record.pixmap + 10), Some(1));
        assert_eq!(loaded.memory.read_u16_be(record.pixmap + 12), Some(1));
    }

    #[test]
    fn hle_import_runner_handles_get_gworld_pixmap() {
        let pef = synthetic_pef_with_import(b"GetGWorldPixMap");
        let mut loaded = load_pef_application(&pef).unwrap();
        loaded.cpu.gpr[3] = PPC_MAIN_GWORLD;

        let probe = loaded.run_with_hle_imports(64);

        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(loaded.cpu.gpr[3], PPC_MAIN_PIXMAP_HANDLE);
    }

    #[test]
    fn hle_import_runner_handles_get_gworld_device() {
        let pef = synthetic_pef_with_import(b"GetGWorldDevice");
        let mut loaded = load_pef_application(&pef).unwrap();
        loaded.cpu.gpr[3] = PPC_MAIN_GWORLD;

        let probe = loaded.run_with_hle_imports(64);

        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(loaded.cpu.gpr[3], PPC_MAIN_GDEVICE);
    }

    #[test]
    fn hle_import_runner_handles_get_pix_base_addr() {
        let pef = synthetic_pef_with_import(b"GetPixBaseAddr");
        let mut loaded = load_pef_application(&pef).unwrap();
        loaded.cpu.gpr[3] = PPC_MAIN_PIXMAP_HANDLE;

        let probe = loaded.run_with_hle_imports(64);

        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(loaded.cpu.gpr[3], PPC_MAIN_SCREEN_BASE);
    }

    #[test]
    fn hle_import_runner_handles_lock_pixels_true() {
        let pef = synthetic_pef_with_import(b"LockPixels");
        let mut loaded = load_pef_application(&pef).unwrap();
        loaded.cpu.gpr[3] = PPC_MAIN_PIXMAP_HANDLE;

        let probe = loaded.run_with_hle_imports(64);

        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(loaded.cpu.gpr[3], 1);
        assert!(
            loaded
                .gworlds
                .iter()
                .find(|record| record.pixmap_handle == PPC_MAIN_PIXMAP_HANDLE)
                .unwrap()
                .pixels_locked
        );

        loaded.cpu.pc = loaded.entry_pc;
        loaded.cpu.lr = PPC_HALT_PC;
        loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::NoPurgePixels;
        loaded.cpu.gpr[3] = PPC_MAIN_PIXMAP_HANDLE;

        let probe = loaded.run_with_hle_imports(64);

        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        assert!(
            loaded
                .gworlds
                .iter()
                .find(|record| record.pixmap_handle == PPC_MAIN_PIXMAP_HANDLE)
                .unwrap()
                .pixels_no_purge
        );

        loaded.cpu.pc = loaded.entry_pc;
        loaded.cpu.lr = PPC_HALT_PC;
        loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::UnlockPixels;
        loaded.cpu.gpr[3] = PPC_MAIN_PIXMAP_HANDLE;

        let probe = loaded.run_with_hle_imports(64);

        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        assert!(
            !loaded
                .gworlds
                .iter()
                .find(|record| record.pixmap_handle == PPC_MAIN_PIXMAP_HANDLE)
                .unwrap()
                .pixels_locked
        );

        loaded.cpu.pc = loaded.entry_pc;
        loaded.cpu.lr = PPC_HALT_PC;
        loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::LockPixels;
        loaded.cpu.gpr[3] = 0x0bad_cafe;

        let probe = loaded.run_with_hle_imports(64);

        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(loaded.cpu.gpr[3], 0);
        assert!(!loaded
            .gworlds
            .iter()
            .any(|record| record.pixmap_handle == 0x0bad_cafe
                || (record.pixmap_handle == PPC_MAIN_PIXMAP_HANDLE && record.pixels_locked)));
    }

    #[test]
    fn native_gworld_pixel_state_flags_update_and_preserve_local_records() {
        // Imaging With QuickDraw (1994), pp. 6-16--6-21 and 6-34--6-38:
        // pixPurge seeds purgeability, keepLocal is retained in the state
        // word, and UpdateGWorld does not discard state for a retained PMH.
        let mut loaded =
            load_pef_application(&synthetic_pef_with_import(b"NewGWorld")).unwrap();
        let scratch = PPC_DATA_BASE + 0x1800;
        let bounds_ptr = scratch;
        let first_out_ptr = scratch + 8;
        let second_out_ptr = scratch + 12;
        loaded.memory.add_region(scratch, vec![0; 32]);
        ppc_write_rect(&mut loaded.memory, bounds_ptr, 0, 0, 4, 4).unwrap();
        let initial_gworlds = loaded.gworlds.len();

        loaded.cpu.gpr[3] = first_out_ptr;
        loaded.cpu.gpr[4] = 8;
        loaded.cpu.gpr[5] = bounds_ptr;
        loaded.cpu.gpr[6] = 0;
        loaded.cpu.gpr[7] = 0;
        loaded.cpu.gpr[8] = 0;
        run_test_import(&mut loaded, PpcImportDispatcherTarget::NewGWorld);
        let first_port = loaded.memory.read_u32_be(first_out_ptr).unwrap();
        let first_pmh = ppc_gworld_pixmap(&mut loaded.memory, &loaded.gworlds, first_port);
        assert_ne!(first_pmh, 0);
        assert_eq!(loaded.gworld_pixel_states.quickdraw_pixel_state(first_pmh), 0);
        assert!(loaded
            .gworlds
            .iter()
            .find(|record| record.port == first_port)
            .unwrap()
            .pixels_no_purge);

        loaded.cpu.gpr[3] = second_out_ptr;
        loaded.cpu.gpr[4] = 8;
        loaded.cpu.gpr[5] = bounds_ptr;
        loaded.cpu.gpr[6] = 0;
        loaded.cpu.gpr[7] = 0;
        loaded.cpu.gpr[8] = (1 << 0) | (1 << 3); // pixPurge + keepLocal
        run_test_import(&mut loaded, PpcImportDispatcherTarget::NewGWorld);
        let second_port = loaded.memory.read_u32_be(second_out_ptr).unwrap();
        let second_pmh = ppc_gworld_pixmap(&mut loaded.memory, &loaded.gworlds, second_port);
        assert_ne!(second_pmh, first_pmh);
        assert_eq!(
            loaded.gworld_pixel_states.quickdraw_pixel_state(second_pmh),
            (1 << 6) | (1 << 3)
        );
        let second_record = loaded
            .gworlds
            .iter()
            .find(|record| record.port == second_port)
            .unwrap();
        assert_eq!((second_record.width, second_record.height), (4, 4));
        assert!(!second_record.pixels_no_purge);
        assert_eq!(loaded.gworlds.len(), initial_gworlds + 2);
        assert!(loaded
            .gworlds
            .iter()
            .any(|record| record.port == PPC_MAIN_GWORLD));

        loaded.cpu.gpr[3] = second_pmh;
        run_test_import(&mut loaded, PpcImportDispatcherTarget::GetPixelsState);
        assert_eq!(loaded.cpu.gpr[3], 0x48);
        loaded.cpu.gpr[3] = second_pmh;
        loaded.cpu.gpr[4] = 1 << 7;
        run_test_import(&mut loaded, PpcImportDispatcherTarget::SetPixelsState);
        assert_eq!(loaded.gworld_pixel_states.quickdraw_pixel_state(second_pmh), 0x88);

        loaded.cpu.gpr[3] = second_pmh;
        run_test_import(&mut loaded, PpcImportDispatcherTarget::AllowPurgePixels);
        assert_eq!(loaded.gworld_pixel_states.quickdraw_pixel_state(second_pmh), 0xc8);
        loaded.cpu.gpr[3] = second_pmh;
        run_test_import(&mut loaded, PpcImportDispatcherTarget::LockPixels);
        assert_eq!(loaded.gworld_pixel_states.quickdraw_pixel_state(second_pmh), 0xc8);
        assert!(loaded
            .gworlds
            .iter()
            .find(|record| record.pixmap_handle == second_pmh)
            .unwrap()
            .pixels_locked);
        loaded.cpu.gpr[3] = second_pmh;
        run_test_import(&mut loaded, PpcImportDispatcherTarget::NoPurgePixels);
        assert_eq!(loaded.gworld_pixel_states.quickdraw_pixel_state(second_pmh), 0x88);
        assert!(loaded
            .gworlds
            .iter()
            .find(|record| record.pixmap_handle == second_pmh)
            .unwrap()
            .pixels_no_purge);
        loaded.cpu.gpr[3] = second_pmh;
        run_test_import(&mut loaded, PpcImportDispatcherTarget::AllowPurgePixels);
        assert_eq!(loaded.gworld_pixel_states.quickdraw_pixel_state(second_pmh), 0xc8);
        loaded.cpu.gpr[3] = second_pmh;
        run_test_import(&mut loaded, PpcImportDispatcherTarget::NoPurgePixels);
        assert_eq!(loaded.gworld_pixel_states.quickdraw_pixel_state(second_pmh), 0x88);
        assert_eq!(loaded.gworld_pixel_states.quickdraw_pixel_state(first_pmh), 0);

        // A same-sized UpdateGWorld keeps the PMH and therefore its process
        // state, while the geometry/allocation record remains native-local.
        loaded.cpu.gpr[3] = second_out_ptr;
        loaded.cpu.gpr[4] = 8;
        loaded.cpu.gpr[5] = bounds_ptr;
        loaded.cpu.gpr[6] = 0;
        loaded.cpu.gpr[7] = 0;
        loaded.cpu.gpr[8] = 0;
        run_test_import(&mut loaded, PpcImportDispatcherTarget::UpdateGWorld);
        assert_eq!(loaded.gworld_pixel_states.quickdraw_pixel_state(second_pmh), 0x88);
        assert_eq!(loaded.gworld_pixel_states.quickdraw_pixel_state(first_pmh), 0);
        assert!(loaded
            .gworlds
            .iter()
            .any(|record| record.port == second_port && record.pixmap_handle == second_pmh));
    }

#[test]
fn import_bindings_classify_gworld_state_imports() {
    assert_eq!(
        dispatcher_target_for_import("InterfaceLib", "GetPort"),
        PpcImportDispatcherTarget::GetPort
    );
    assert_eq!(
        dispatcher_target_for_import("InterfaceLib", "GetWMgrPort"),
        PpcImportDispatcherTarget::GetWMgrPort
    );
    assert_eq!(
        dispatcher_target_for_import("InterfaceLib", "GetCWMgrPort"),
        PpcImportDispatcherTarget::GetWMgrPort
    );
    assert_eq!(
        dispatcher_target_for_import("InterfaceLib", "SetPort"),
        PpcImportDispatcherTarget::SetPort
    );
    assert_eq!(
        dispatcher_target_for_import("InterfaceLib", "GetGDevice"),
        PpcImportDispatcherTarget::GetGDevice
    );
    assert_eq!(
        dispatcher_target_for_import("InterfaceLib", "SetGDevice"),
        PpcImportDispatcherTarget::SetGDevice
    );
    assert_eq!(
        dispatcher_target_for_import("InterfaceLib", "GetDeviceList"),
        PpcImportDispatcherTarget::GetDeviceList
    );
    assert_eq!(
        dispatcher_target_for_import("InterfaceLib", "GetNextDevice"),
        PpcImportDispatcherTarget::GetNextDevice
    );
    assert_eq!(
        dispatcher_target_for_import("InterfaceLib", "GetMainDevice"),
        PpcImportDispatcherTarget::GetMainDevice
    );
    assert_eq!(
        dispatcher_target_for_import("InterfaceLib", "GetMBarHeight"),
        PpcImportDispatcherTarget::GetMBarHeight
    );
    assert_eq!(
        dispatcher_target_for_import("InterfaceLib", "TestDeviceAttribute"),
        PpcImportDispatcherTarget::TestDeviceAttribute
    );
    assert_eq!(
        dispatcher_target_for_import("InterfaceLib", "HasDepth"),
        PpcImportDispatcherTarget::HasDepth
    );
    assert_eq!(
        dispatcher_target_for_import("InterfaceLib", "SetDepth"),
        PpcImportDispatcherTarget::SetDepth
    );
    assert_eq!(
        dispatcher_target_for_import("InterfaceLib", "DMGetGDeviceByDisplayID"),
        PpcImportDispatcherTarget::DMGetGDeviceByDisplayID
    );
    assert_eq!(
        dispatcher_target_for_import("InterfaceLib", "NewCWindow"),
        PpcImportDispatcherTarget::NewCWindow
    );
    assert_eq!(
        dispatcher_target_for_import("InterfaceLib", "GetNewCWindow"),
        PpcImportDispatcherTarget::GetNewCWindow
    );
    assert_eq!(
        dispatcher_target_for_import("InterfaceLib", "GetWRefCon"),
        PpcImportDispatcherTarget::GetWRefCon
    );
    assert_eq!(
        dispatcher_target_for_import("InterfaceLib", "SetWRefCon"),
        PpcImportDispatcherTarget::SetWRefCon
    );
    assert_eq!(
        dispatcher_target_for_import("InterfaceLib", "SizeWindow"),
        PpcImportDispatcherTarget::SizeWindow
    );
    assert_eq!(
        dispatcher_target_for_import("InterfaceLib", "MoveWindow"),
        PpcImportDispatcherTarget::MoveWindow
    );
    assert_eq!(
        dispatcher_target_for_import("InterfaceLib", "ShowWindow"),
        PpcImportDispatcherTarget::ShowWindow
    );
    assert_eq!(
        dispatcher_target_for_import("InterfaceLib", "HideWindow"),
        PpcImportDispatcherTarget::HideWindow
    );
    assert_eq!(
        dispatcher_target_for_import("InterfaceLib", "ShowHide"),
        PpcImportDispatcherTarget::ShowHide
    );
    assert_eq!(
        dispatcher_target_for_import("InterfaceLib", "CloseWindow"),
        PpcImportDispatcherTarget::CloseWindow
    );
    assert_eq!(
        dispatcher_target_for_import("InterfaceLib", "SelectWindow"),
        PpcImportDispatcherTarget::SelectWindow
    );
    assert_eq!(
        dispatcher_target_for_import("InterfaceLib", "SetWinColor"),
        PpcImportDispatcherTarget::SetWinColor
    );
    assert_eq!(
        dispatcher_target_for_import("InterfaceLib", "CalcVisBehind"),
        PpcImportDispatcherTarget::CalcVisBehind
    );
    assert_eq!(
        dispatcher_target_for_import("InterfaceLib", "PaintBehind"),
        PpcImportDispatcherTarget::PaintBehind
    );
    assert_eq!(
        dispatcher_target_for_import("InterfaceLib", "PaintOne"),
        PpcImportDispatcherTarget::PaintOne
    );
    assert_eq!(
        dispatcher_target_for_import("InterfaceLib", "ActivatePalette"),
        PpcImportDispatcherTarget::ActivatePalette
    );
    assert_eq!(
        dispatcher_target_for_import("InterfaceLib", "SetPalette"),
        PpcImportDispatcherTarget::NSetPalette
    );
    assert_eq!(
        dispatcher_target_for_import("InterfaceLib", "NSetPalette"),
        PpcImportDispatcherTarget::NSetPalette
    );
    assert_eq!(
        dispatcher_target_for_import("InterfaceLib", "GetPalette"),
        PpcImportDispatcherTarget::GetPalette
    );
    assert_eq!(
        dispatcher_target_for_import("InterfaceLib", "NewGWorld"),
        PpcImportDispatcherTarget::NewGWorld
    );
    assert_eq!(
        dispatcher_target_for_import("InterfaceLib", "DisposeGWorld"),
        PpcImportDispatcherTarget::DisposeGWorld
    );
    assert_eq!(
        dispatcher_target_for_import("InterfaceLib", "GetGWorld"),
        PpcImportDispatcherTarget::GetGWorld
    );
    assert_eq!(
        dispatcher_target_for_import("InterfaceLib", "SetGWorld"),
        PpcImportDispatcherTarget::SetGWorld
    );
    assert_eq!(
        dispatcher_target_for_import("InterfaceLib", "GetGWorldDevice"),
        PpcImportDispatcherTarget::GetGWorldDevice
    );
    assert_eq!(
        dispatcher_target_for_import("InterfaceLib", "GetGWorldPixMap"),
        PpcImportDispatcherTarget::GetGWorldPixMap
    );
    assert_eq!(
        dispatcher_target_for_import("InterfaceLib", "GetPixBaseAddr"),
        PpcImportDispatcherTarget::GetPixBaseAddr
    );
    assert_eq!(
        dispatcher_target_for_import("InterfaceLib", "LockPixels"),
        PpcImportDispatcherTarget::LockPixels
    );
    assert_eq!(
        dispatcher_target_for_import("InterfaceLib", "UnlockPixels"),
        PpcImportDispatcherTarget::UnlockPixels
    );
    assert_eq!(
        dispatcher_target_for_import("InterfaceLib", "GetPixelsState"),
        PpcImportDispatcherTarget::GetPixelsState
    );
    assert_eq!(
        dispatcher_target_for_import("InterfaceLib", "SetPixelsState"),
        PpcImportDispatcherTarget::SetPixelsState
    );
    assert_eq!(
        dispatcher_target_for_import("InterfaceLib", "AllowPurgePixels"),
        PpcImportDispatcherTarget::AllowPurgePixels
    );
    assert_eq!(
        dispatcher_target_for_import("InterfaceLib", "NoPurgePixels"),
        PpcImportDispatcherTarget::NoPurgePixels
    );
    assert_eq!(
        dispatcher_target_for_import("InterfaceLib", "CopyBits"),
        PpcImportDispatcherTarget::CopyBits
    );
    assert_eq!(
        dispatcher_target_for_import("InterfaceLib", "BitMapToRegion"),
        PpcImportDispatcherTarget::BitMapToRegion
    );
    assert_eq!(
        dispatcher_target_for_import("InterfaceLib", "GetPenState"),
        PpcImportDispatcherTarget::GetPenState
    );
    assert_eq!(
        dispatcher_target_for_import("InterfaceLib", "SetPenState"),
        PpcImportDispatcherTarget::SetPenState
    );
    assert_eq!(
        dispatcher_target_for_import("InterfaceLib", "NewRgn"),
        PpcImportDispatcherTarget::NewRgn
    );
    assert_eq!(
        dispatcher_target_for_import("InterfaceLib", "DisposeRgn"),
        PpcImportDispatcherTarget::DisposeRgn
    );
    assert_eq!(
        dispatcher_target_for_import("InterfaceLib", "OpenRgn"),
        PpcImportDispatcherTarget::OpenRgn
    );
    assert_eq!(
        dispatcher_target_for_import("InterfaceLib", "CloseRgn"),
        PpcImportDispatcherTarget::CloseRgn
    );
    assert_eq!(
        dispatcher_target_for_import("InterfaceLib", "SetEmptyRgn"),
        PpcImportDispatcherTarget::SetEmptyRgn
    );
    assert_eq!(
        dispatcher_target_for_import("InterfaceLib", "SetRectRgn"),
        PpcImportDispatcherTarget::SetRectRgn
    );
    assert_eq!(
        dispatcher_target_for_import("InterfaceLib", "RectRgn"),
        PpcImportDispatcherTarget::RectRgn
    );
    assert_eq!(
        dispatcher_target_for_import("InterfaceLib", "EmptyRgn"),
        PpcImportDispatcherTarget::EmptyRgn
    );
}
