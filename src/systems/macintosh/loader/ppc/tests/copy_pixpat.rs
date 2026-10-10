use super::*;

#[test]
fn copy_pixpat_preserves_independent_nested_storage_after_source_disposal() {
    for library in [b"InterfaceLib".as_slice(), b"CarbonLib".as_slice()] {
        let mut loaded =
            load_pef_application(&synthetic_pef_with_library_import(library, b"CopyPixPat"))
                .unwrap();
        let new = PpcImportDispatcherTarget::QuickDrawCompatibility(
            PpcQuickDrawCompatibilityOperation::NewPixPat,
        );
        run_test_import(&mut loaded, new.clone());
        let source = loaded.cpu.gpr[3];
        run_test_import(&mut loaded, new);
        let destination = loaded.cpu.gpr[3];
        let source_ptr = loaded.memory.read_u32_be(source).unwrap();
        let destination_ptr = loaded.memory.read_u32_be(destination).unwrap();
        loaded.memory.write_u16_be(source_ptr + 14, 0x1234).unwrap();
        loaded
            .memory
            .write_bytes(source_ptr + 20, &[1, 2, 3, 4, 5, 6, 7, 8])
            .unwrap();
        let mut expected = Vec::new();
        for (offset, size) in [(6, 16u32), (10, 8), (16, 4)] {
            let handle = loaded.memory.read_u32_be(source_ptr + offset).unwrap();
            loaded.cpu.gpr[3] = handle;
            loaded.cpu.gpr[4] = size;
            run_test_import(&mut loaded, PpcImportDispatcherTarget::SetHandleSize);
            let ptr = loaded.memory.read_u32_be(handle).unwrap();
            let bytes = vec![offset as u8; size as usize];
            loaded.memory.write_bytes(ptr, &bytes).unwrap();
            expected.push((offset, handle, bytes));
        }
        let source_map = loaded.memory.read_u32_be(source_ptr + 2).unwrap();
        let source_map_ptr = loaded.memory.read_u32_be(source_map).unwrap();
        let source_table = loaded.memory.read_u32_be(source_map_ptr + 42).unwrap();
        loaded.cpu.gpr[3] = source_table;
        loaded.cpu.gpr[4] = 16;
        run_test_import(&mut loaded, PpcImportDispatcherTarget::SetHandleSize);
        let source_table_ptr = loaded.memory.read_u32_be(source_table).unwrap();
        loaded.memory.write_u16_be(source_table_ptr + 6, 0).unwrap();
        loaded
            .memory
            .write_u16_be(source_table_ptr + 10, 0x1357)
            .unwrap();
        let old_map = loaded.memory.read_u32_be(destination_ptr + 2).unwrap();
        loaded.cpu.gpr[3] = source;
        loaded.cpu.gpr[4] = destination;
        run_test_import(
            &mut loaded,
            PpcImportDispatcherTarget::QuickDrawCompatibility(
                PpcQuickDrawCompatibilityOperation::CopyPixPat,
            ),
        );
        assert_eq!(loaded.last_mem_error(), PPC_NO_ERR);
        assert_eq!(
            loaded.memory.read_u16_be(destination_ptr + 14),
            Some(0x1234)
        );
        let mut fallback = [0; 8];
        loaded
            .memory
            .read_bytes_into(destination_ptr + 20, &mut fallback)
            .unwrap();
        assert_eq!(fallback, [1, 2, 3, 4, 5, 6, 7, 8]);
        let copied_map = loaded.memory.read_u32_be(destination_ptr + 2).unwrap();
        assert_ne!(copied_map, source_map);
        assert!(
            !test_handle_records!(loaded)
                .iter()
                .any(|record| record.handle == old_map)
        );
        let copied_map_ptr = loaded.memory.read_u32_be(copied_map).unwrap();
        let copied_table = loaded.memory.read_u32_be(copied_map_ptr + 42).unwrap();
        assert_ne!(copied_table, source_table);
        loaded.cpu.gpr[3] = source;
        run_test_import(
            &mut loaded,
            PpcImportDispatcherTarget::QuickDrawCompatibility(
                PpcQuickDrawCompatibilityOperation::DisposePixPat,
            ),
        );
        for (offset, original_handle, bytes) in expected {
            let copy = loaded.memory.read_u32_be(destination_ptr + offset).unwrap();
            assert_ne!(copy, original_handle);
            let ptr = loaded.memory.read_u32_be(copy).unwrap();
            let mut actual = vec![0; bytes.len()];
            loaded.memory.read_bytes_into(ptr, &mut actual).unwrap();
            assert_eq!(actual, bytes);
        }
        let copied_table_ptr = loaded.memory.read_u32_be(copied_table).unwrap();
        assert_eq!(
            loaded.memory.read_u16_be(copied_table_ptr + 10),
            Some(0x1357)
        );
        loaded.cpu.gpr[3] = destination;
        run_test_import(
            &mut loaded,
            PpcImportDispatcherTarget::QuickDrawCompatibility(
                PpcQuickDrawCompatibilityOperation::DisposePixPat,
            ),
        );
        assert!(
            !test_handle_records!(loaded)
                .iter()
                .any(|record| record.handle == copied_map || record.handle == copied_table)
        );
    }
}

#[test]
fn copy_pixpat_rejects_short_source_without_replacing_destination() {
    let mut loaded = load_pef_application(&synthetic_pef_with_library_import(
        b"InterfaceLib",
        b"CopyPixPat",
    ))
    .unwrap();
    let new = PpcImportDispatcherTarget::QuickDrawCompatibility(
        PpcQuickDrawCompatibilityOperation::NewPixPat,
    );
    run_test_import(&mut loaded, new.clone());
    let source = loaded.cpu.gpr[3];
    run_test_import(&mut loaded, new);
    let destination = loaded.cpu.gpr[3];
    let destination_ptr = loaded.memory.read_u32_be(destination).unwrap();
    let mut before = [0; 28];
    loaded
        .memory
        .read_bytes_into(destination_ptr, &mut before)
        .unwrap();
    loaded.cpu.gpr[3] = source;
    loaded.cpu.gpr[4] = 20;
    run_test_import(&mut loaded, PpcImportDispatcherTarget::SetHandleSize);
    let count = test_handle_records!(loaded).len();
    loaded.cpu.gpr[3] = source;
    loaded.cpu.gpr[4] = destination;
    run_test_import(
        &mut loaded,
        PpcImportDispatcherTarget::QuickDrawCompatibility(
            PpcQuickDrawCompatibilityOperation::CopyPixPat,
        ),
    );
    assert_eq!(loaded.last_mem_error(), PPC_PARAM_ERR);
    let mut after = [0; 28];
    loaded
        .memory
        .read_bytes_into(destination_ptr, &mut after)
        .unwrap();
    assert_eq!(after, before);
    assert_eq!(test_handle_records!(loaded).len(), count);
    loaded.cpu.gpr[3] = destination;
    loaded.cpu.gpr[4] = destination;
    run_test_import(
        &mut loaded,
        PpcImportDispatcherTarget::QuickDrawCompatibility(
            PpcQuickDrawCompatibilityOperation::CopyPixPat,
        ),
    );
    assert_eq!(loaded.last_mem_error(), PPC_NO_ERR);
    loaded
        .memory
        .read_bytes_into(destination_ptr, &mut after)
        .unwrap();
    assert_eq!(after, before);
    assert_eq!(test_handle_records!(loaded).len(), count);
}

#[test]
fn copy_pixpat_allocation_failure_keeps_destination_and_releases_partial_copies() {
    for successful_copies in [0usize, 1, 2] {
        let mut loaded = load_pef_application(&synthetic_pef_with_library_import(
            b"InterfaceLib",
            b"CopyPixPat",
        ))
        .unwrap();
        let new = PpcImportDispatcherTarget::QuickDrawCompatibility(
            PpcQuickDrawCompatibilityOperation::NewPixPat,
        );
        run_test_import(&mut loaded, new.clone());
        let source = loaded.cpu.gpr[3];
        run_test_import(&mut loaded, new);
        let destination = loaded.cpu.gpr[3];
        let destination_ptr = loaded.memory.read_u32_be(destination).unwrap();
        let mut before = [0; 28];
        loaded
            .memory
            .read_bytes_into(destination_ptr, &mut before)
            .unwrap();
        let source_ptr = loaded.memory.read_u32_be(source).unwrap();
        for offset in [6, 10, 16] {
            let handle = loaded.memory.read_u32_be(source_ptr + offset).unwrap();
            loaded.cpu.gpr[3] = handle;
            loaded.cpu.gpr[4] = 16;
            run_test_import(&mut loaded, PpcImportDispatcherTarget::SetHandleSize);
        }
        let records = test_handle_records!(loaded).to_vec();
        let map = loaded.memory.read_u32_be(source_ptr + 2).unwrap();
        let map_ptr = loaded.memory.read_u32_be(map).unwrap();
        let mut graph = vec![map, loaded.memory.read_u32_be(map_ptr + 42).unwrap()];
        for offset in [6, 10, 16] {
            graph.push(loaded.memory.read_u32_be(source_ptr + offset).unwrap());
        }
        graph.sort_unstable();
        let sizes: Vec<u32> = graph
            .iter()
            .map(|handle| {
                records
                    .iter()
                    .find(|record| record.handle == *handle)
                    .unwrap()
                    .size
            })
            .collect();
        let capacity: u32 = sizes
            .iter()
            .take(successful_copies)
            .map(|size| 16 + ((*size + 15) & !15).max(16))
            .sum();
        let first_ptr = loaded.memory.read_u32_be(graph[0]).unwrap();
        let mut first_bytes = vec![0; sizes[0] as usize];
        loaded
            .memory
            .read_bytes_into(first_ptr, &mut first_bytes)
            .unwrap();
        let cursor = loaded.heap_cursor();
        loaded
            .memory
            .write_bytes(cursor + 16, &vec![0xa5; first_bytes.len()])
            .unwrap();
        loaded.with_process_memory_manager(|_, manager| {
            manager.mutate_native_allocator(|allocator| {
                // The allocator treats the heap limit as an exclusive ceiling.
                allocator.heap.heap_limit = cursor + capacity + 1;
                allocator.free_ptr_blocks.clear();
                allocator.free_handle_blocks.clear();
            });
        });
        loaded.cpu.gpr[3] = source;
        loaded.cpu.gpr[4] = destination;
        run_test_import(
            &mut loaded,
            PpcImportDispatcherTarget::QuickDrawCompatibility(
                PpcQuickDrawCompatibilityOperation::CopyPixPat,
            ),
        );
        assert_eq!(
            loaded.last_mem_error(),
            PPC_MEM_FULL_ERR,
            "capacity {capacity}"
        );
        run_test_import(&mut loaded, PpcImportDispatcherTarget::MemError);
        assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_MEM_FULL_ERR));
        let mut after = [0; 28];
        loaded
            .memory
            .read_bytes_into(destination_ptr, &mut after)
            .unwrap();
        assert_eq!(after, before, "capacity {capacity}");
        assert_eq!(
            test_handle_records!(loaded),
            records.as_slice(),
            "capacity {capacity}"
        );
        if successful_copies != 0 {
            let mut temporary = vec![0xa5; first_bytes.len()];
            loaded
                .memory
                .read_bytes_into(cursor + 16, &mut temporary)
                .unwrap();
            assert_eq!(
                temporary, first_bytes,
                "failure must follow the first successful copy"
            );
        }
    }
}

#[test]
fn copy_pixpat_copies_basic_pattern_without_a_pixel_map() {
    for library in [b"InterfaceLib".as_slice(), b"CarbonLib".as_slice()] {
        let mut loaded =
            load_pef_application(&synthetic_pef_with_library_import(library, b"CopyPixPat"))
                .unwrap();
        loaded.cpu.gpr[3] = 28;
        run_test_import(
            &mut loaded,
            PpcImportDispatcherTarget::NewHandle { clear: true },
        );
        let source = loaded.cpu.gpr[3];
        let source_ptr = loaded.memory.read_u32_be(source).unwrap();
        loaded.memory.write_bytes(source_ptr, &[0; 28]).unwrap();
        let fallback = [0x80, 0x40, 0x20, 0x10, 8, 4, 2, 1];
        loaded
            .memory
            .write_bytes(source_ptr + 20, &fallback)
            .unwrap();
        run_test_import(
            &mut loaded,
            PpcImportDispatcherTarget::QuickDrawCompatibility(
                PpcQuickDrawCompatibilityOperation::NewPixPat,
            ),
        );
        let destination = loaded.cpu.gpr[3];
        let destination_ptr = loaded.memory.read_u32_be(destination).unwrap();
        let old_map = loaded.memory.read_u32_be(destination_ptr + 2).unwrap();
        loaded.cpu.gpr[3] = source;
        loaded.cpu.gpr[4] = destination;
        run_test_import(
            &mut loaded,
            PpcImportDispatcherTarget::QuickDrawCompatibility(
                PpcQuickDrawCompatibilityOperation::CopyPixPat,
            ),
        );
        assert_eq!(loaded.last_mem_error(), PPC_NO_ERR);
        assert_eq!(
            loaded.memory.read_u32_be(destination),
            Some(destination_ptr)
        );
        assert_eq!(loaded.memory.read_u32_be(destination_ptr + 2), Some(0));
        assert!(
            !test_handle_records!(loaded)
                .iter()
                .any(|record| record.handle == old_map)
        );
        loaded.cpu.gpr[3] = source;
        run_test_import(&mut loaded, PpcImportDispatcherTarget::DisposeHandle);
        let mut copied = [0; 8];
        loaded
            .memory
            .read_bytes_into(destination_ptr + 20, &mut copied)
            .unwrap();
        assert_eq!(copied, fallback);
        loaded.cpu.gpr[3] = destination;
        loaded.cpu.gpr[4] = destination;
        run_test_import(
            &mut loaded,
            PpcImportDispatcherTarget::QuickDrawCompatibility(
                PpcQuickDrawCompatibilityOperation::CopyPixPat,
            ),
        );
        assert_eq!(loaded.last_mem_error(), PPC_NO_ERR);
        loaded
            .memory
            .read_bytes_into(destination_ptr + 20, &mut copied)
            .unwrap();
        assert_eq!(copied, fallback);
    }
}

#[test]
fn copy_pixpat_rejects_short_pixel_maps_before_mutation() {
    for short_source in [true, false] {
        let mut loaded = load_pef_application(&synthetic_pef_with_import(b"CopyPixPat")).unwrap();
        let new = PpcImportDispatcherTarget::QuickDrawCompatibility(
            PpcQuickDrawCompatibilityOperation::NewPixPat,
        );
        run_test_import(&mut loaded, new.clone());
        let source = loaded.cpu.gpr[3];
        run_test_import(&mut loaded, new);
        let destination = loaded.cpu.gpr[3];
        let destination_ptr = loaded.memory.read_u32_be(destination).unwrap();
        let short_parent = loaded
            .memory
            .read_u32_be(if short_source { source } else { destination })
            .unwrap();
        let map = loaded.memory.read_u32_be(short_parent + 2).unwrap();
        loaded.cpu.gpr[3] = map;
        loaded.cpu.gpr[4] = 20;
        run_test_import(&mut loaded, PpcImportDispatcherTarget::SetHandleSize);
        let mut before = [0; 28];
        loaded
            .memory
            .read_bytes_into(destination_ptr, &mut before)
            .unwrap();
        let handles_before = test_handle_records!(loaded).len();
        loaded.cpu.gpr[3] = source;
        loaded.cpu.gpr[4] = destination;
        run_test_import(
            &mut loaded,
            PpcImportDispatcherTarget::QuickDrawCompatibility(
                PpcQuickDrawCompatibilityOperation::CopyPixPat,
            ),
        );
        assert_eq!(loaded.last_mem_error(), PPC_PARAM_ERR);
        let mut after = [0; 28];
        loaded
            .memory
            .read_bytes_into(destination_ptr, &mut after)
            .unwrap();
        assert_eq!(after, before);
        assert_eq!(test_handle_records!(loaded).len(), handles_before);
    }
}
