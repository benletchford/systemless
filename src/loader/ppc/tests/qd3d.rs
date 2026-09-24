use super::*;

#[test]
fn quickdraw_3d_vector_length_and_strided_point_bounds() {
    let pef = synthetic_pef_with_library_import(b"QuickDraw\xaa 3D", b"Q3Vector3D_Length");
    let mut loaded = load_pef_application(&pef).unwrap();
    let vector_ptr = PPC_DATA_BASE + 0x1000;
    loaded.memory.add_region(vector_ptr, vec![0; 64]);
    ppc_write_q3_vector3d(&mut loaded.memory, vector_ptr, (3.0, 4.0, 12.0)).unwrap();
    loaded.cpu.gpr[3] = vector_ptr;
    let probe = loaded.run_with_hle_imports(64);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(f64::from_bits(loaded.cpu.fpr[1]), 13.0);

    let pef =
        synthetic_pef_with_library_import(b"QuickDraw\xaa 3D", b"Q3BoundingBox_SetFromPoints3D");
    let mut loaded = load_pef_application(&pef).unwrap();
    let points_ptr = PPC_DATA_BASE + 0x1000;
    let box_ptr = PPC_DATA_BASE + 0x1100;
    loaded.memory.add_region(points_ptr, vec![0; 64]);
    loaded
        .memory
        .add_region(box_ptr, vec![0; PPC_Q3_BOUNDING_BOX_SIZE as usize]);
    ppc_write_q3_vector3d(&mut loaded.memory, points_ptr, (3.0, -4.0, 5.0)).unwrap();
    ppc_write_q3_vector3d(&mut loaded.memory, points_ptr + 16, (-2.0, 7.0, 1.0)).unwrap();
    loaded.cpu.gpr[3] = box_ptr;
    loaded.cpu.gpr[4] = points_ptr;
    loaded.cpu.gpr[5] = 2;
    loaded.cpu.gpr[6] = 16;
    let probe = loaded.run_with_hle_imports(64);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], box_ptr);
    assert_eq!(
        ppc_read_q3_vector3d(&mut loaded.memory, box_ptr),
        Some((-2.0, -4.0, 1.0))
    );
    assert_eq!(
        ppc_read_q3_vector3d(&mut loaded.memory, box_ptr + 12),
        Some((3.0, 7.0, 5.0))
    );
    assert_eq!(loaded.memory.read_u32_be(box_ptr + 24), Some(0));
}

#[test]
fn quickdraw_3d_bounding_sphere_contains_submitted_trimesh_points() {
    let pef = synthetic_pef_with_library_import(b"QuickDraw\xaa 3D", b"Q3View_StartBoundingSphere");
    let mut loaded = load_pef_application(&pef).unwrap();
    let view = PPC_Q3_OBJECT_BASE;
    let trimesh = PPC_Q3_OBJECT_BASE + PPC_Q3_OBJECT_STRIDE;
    let sphere_ptr = PPC_DATA_BASE + 0x1000;
    let points_ptr = PPC_DATA_BASE + 0x1100;
    loaded
        .memory
        .add_region(sphere_ptr, vec![0; PPC_Q3_BOUNDING_SPHERE_SIZE as usize]);
    loaded.memory.add_region(points_ptr, vec![0; 24]);
    ppc_write_q3_vector3d(&mut loaded.memory, points_ptr, (0.0, 0.0, 0.0)).unwrap();
    ppc_write_q3_vector3d(&mut loaded.memory, points_ptr + 12, (0.0, 0.0, 10.0)).unwrap();
    loaded
        .q3_objects
        .push(test_q3_object(view, PPC_Q3_TYPE_VIEW));
    let mut data = vec![0; PPC_Q3_TRIMESH_DATA_SIZE as usize];
    ppc_q3_trimesh_header_put_u32(&mut data, PPC_Q3_TRIMESH_NUM_POINTS_OFFSET, 2).unwrap();
    ppc_q3_trimesh_header_put_u32(&mut data, PPC_Q3_TRIMESH_POINTS_OFFSET, points_ptr).unwrap();
    loaded.q3_trimeshes.push(PpcQ3TriMeshRecord {
        trimesh,
        data,
        triangle_attribute_sets: Vec::new(),
        get_data_copies: Vec::new(),
    });
    loaded.cpu.gpr[3] = view;
    let probe = loaded.run_with_hle_imports(64);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], 1);
    loaded.q3_submissions.push(PpcQ3SubmissionRecord {
        view,
        kind: PpcQ3SubmissionKind::TriMesh,
        primary: trimesh,
        secondary: 0,
    });
    let mut local_to_world = ppc_q3_matrix4x4_identity();
    local_to_world[3][0] = 10.0;
    loaded
        .q3_submission_transforms
        .push(PpcQ3SubmissionTransformRecord {
            view,
            kind: PpcQ3SubmissionKind::TriMesh,
            primary: trimesh,
            secondary: 0,
            local_to_world,
        });
    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::Q3ViewEndBoundingSphere;
    loaded.cpu.gpr[3] = view;
    loaded.cpu.gpr[4] = sphere_ptr;
    let probe = loaded.run_with_hle_imports(64);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], PPC_Q3_VIEW_STATUS_DONE);
    assert_eq!(
        ppc_read_q3_vector3d(&mut loaded.memory, sphere_ptr),
        Some((10.0, 0.0, 5.0))
    );
    assert_eq!(
        ppc_read_f32_be(&mut loaded.memory, sphere_ptr + 12),
        Some(5.0)
    );
    assert_eq!(loaded.memory.read_u32_be(sphere_ptr + 16), Some(0));
}

#[test]
fn hle_import_runner_handles_quickdraw_3d_initialize_success() {
    let pef = synthetic_pef_with_library_import(b"QuickDraw\xaa 3D", b"Q3Initialize");
    let mut loaded = load_pef_application(&pef).unwrap();
    assert_eq!(loaded.q3_lifecycle, PpcQ3LifecycleState::default());

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], 1);
    assert_eq!(loaded.q3_lifecycle.initialize_count, 1);
    assert_eq!(loaded.q3_lifecycle.exit_count, 0);
    assert_eq!(loaded.q3_lifecycle.initialized_depth, 1);
    assert!(loaded.q3_lifecycle.initialized());
}

#[test]
fn hle_import_runner_handles_quickdraw_3d_exit_success() {
    let pef = synthetic_pef_with_library_import(b"QuickDraw\xaa 3D", b"Q3Exit");
    let mut loaded = load_pef_application(&pef).unwrap();
    loaded.q3_lifecycle.initialized_depth = 1;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], 1);
    assert_eq!(loaded.q3_lifecycle.initialize_count, 0);
    assert_eq!(loaded.q3_lifecycle.exit_count, 1);
    assert_eq!(loaded.q3_lifecycle.initialized_depth, 0);
    assert!(!loaded.q3_lifecycle.initialized());

    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], 1);
    assert_eq!(loaded.q3_lifecycle.exit_count, 2);
    assert_eq!(loaded.q3_lifecycle.initialized_depth, 0);
}

#[test]
fn hle_import_runner_reports_quickdraw_3d_version_after_initialization() {
    let pef = synthetic_pef_with_library_import(b"QuickDraw\xaa 3D", b"Q3GetVersion");
    let mut loaded = load_pef_application(&pef).unwrap();
    let major_ptr = PPC_DATA_BASE + 0x1000;
    let minor_ptr = PPC_DATA_BASE + 0x1004;
    loaded.memory.add_region(major_ptr, vec![0; 8]);
    loaded.cpu.gpr[3] = major_ptr;
    loaded.cpu.gpr[4] = minor_ptr;

    let probe = loaded.run_with_hle_imports(64);
    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(loaded.cpu.gpr[3], 0);
    assert_eq!(loaded.memory.read_u32_be(major_ptr), Some(0));

    loaded.q3_lifecycle.initialized_depth = 1;
    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.cpu.gpr[3] = major_ptr;
    loaded.cpu.gpr[4] = minor_ptr;
    let probe = loaded.run_with_hle_imports(64);
    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], 1);
    assert_eq!(loaded.memory.read_u32_be(major_ptr), Some(1));
    assert_eq!(loaded.memory.read_u32_be(minor_ptr), Some(6));
}

#[test]
fn hle_import_runner_handles_q3_memory_storage_new() {
    let pef = synthetic_pef_with_library_import(b"QuickDraw\xaa 3D", b"Q3MemoryStorage_New");
    let mut loaded = load_pef_application(&pef).unwrap();
    let source_ptr = PPC_DATA_BASE + 0x1000;
    let output_ptr = PPC_DATA_BASE + 0x1100;
    let size_read_ptr = PPC_DATA_BASE + 0x1200;
    loaded.memory.add_region(source_ptr, vec![1, 2, 3, 4]);
    loaded.memory.add_region(output_ptr, vec![0; 8]);
    loaded.memory.add_region(size_read_ptr, vec![0; 4]);
    loaded.cpu.gpr[3] = source_ptr;
    loaded.cpu.gpr[4] = 4;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    let storage = loaded.cpu.gpr[3];
    assert_eq!(storage, PPC_Q3_OBJECT_BASE);
    assert_eq!(
        loaded.next_q3_object,
        PPC_Q3_OBJECT_BASE + PPC_Q3_OBJECT_STRIDE
    );
    assert_eq!(loaded.q3_objects.len(), 1);
    assert_eq!(loaded.q3_objects[0].object, storage);
    assert_eq!(loaded.q3_objects[0].kind, PpcQ3ObjectKind::MemoryStorage);
    assert_eq!(loaded.q3_objects[0].object_type, PPC_Q3_STORAGE_TYPE_MEMORY);
    assert_eq!(loaded.q3_objects[0].data_ptr, PPC_HEAP_BASE);
    assert_eq!(loaded.q3_objects[0].data_size, 4);
    assert_eq!(
        loaded.q3_memory_storages,
        vec![PpcQ3MemoryStorageRecord {
            storage,
            buffer_ptr: PPC_HEAP_BASE,
            valid_size: 4,
            buffer_size: 4,
            owns_buffer: true,
        }]
    );
    loaded.memory.write_u8(source_ptr, 9).unwrap();
    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::Q3StorageGetData;
    loaded.cpu.gpr[3] = storage;
    loaded.cpu.gpr[4] = 0;
    loaded.cpu.gpr[5] = 4;
    loaded.cpu.gpr[6] = output_ptr;
    loaded.cpu.gpr[7] = size_read_ptr;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], 1);
    assert_eq!(loaded.memory.read_u32_be(size_read_ptr), Some(4));
    assert_eq!(loaded.memory.read_u8(output_ptr), Some(1));
    assert_eq!(loaded.memory.read_u8(output_ptr + 1), Some(2));
    assert_eq!(loaded.memory.read_u8(output_ptr + 2), Some(3));
    assert_eq!(loaded.memory.read_u8(output_ptr + 3), Some(4));
}

#[test]
fn hle_import_runner_handles_q3_memory_storage_buffer_state() {
    let pef =
        synthetic_pef_with_library_import(b"QuickDraw\xaa 3D", b"Q3MemoryStorage_NewBuffer");
    let mut loaded = load_pef_application(&pef).unwrap();
    let buffer_ptr = PPC_DATA_BASE + 0x1000;
    let output_ptr = PPC_DATA_BASE + 0x1080;
    let buffer_out_ptr = PPC_DATA_BASE + 0x10c0;
    let valid_size_out_ptr = PPC_DATA_BASE + 0x10c4;
    let buffer_size_out_ptr = PPC_DATA_BASE + 0x10c8;
    let source_ptr = PPC_DATA_BASE + 0x1100;
    loaded
        .memory
        .add_region(buffer_ptr, vec![10, 11, 12, 13, 14, 15, 0, 0]);
    loaded.memory.add_region(output_ptr, vec![0; 16]);
    loaded.memory.add_region(buffer_out_ptr, vec![0; 12]);
    loaded
        .memory
        .add_region(source_ptr, vec![21, 22, 23, 24, 25, 26]);
    loaded.cpu.gpr[3] = buffer_ptr;
    loaded.cpu.gpr[4] = 6;
    loaded.cpu.gpr[5] = 8;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    let storage = loaded.cpu.gpr[3];
    assert_eq!(storage, PPC_Q3_OBJECT_BASE);
    assert_eq!(loaded.q3_objects[0].data_ptr, buffer_ptr);
    assert_eq!(loaded.q3_objects[0].data_size, 6);
    assert_eq!(
        loaded.q3_memory_storages[0],
        PpcQ3MemoryStorageRecord {
            storage,
            buffer_ptr,
            valid_size: 6,
            buffer_size: 8,
            owns_buffer: false,
        }
    );

    loaded.memory.write_u8(buffer_ptr, 99).unwrap();
    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::Q3StorageGetData;
    loaded.cpu.gpr[3] = storage;
    loaded.cpu.gpr[4] = 0;
    loaded.cpu.gpr[5] = 2;
    loaded.cpu.gpr[6] = output_ptr;
    loaded.cpu.gpr[7] = valid_size_out_ptr;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], 1);
    assert_eq!(loaded.memory.read_u32_be(valid_size_out_ptr), Some(2));
    assert_eq!(loaded.memory.read_u8(output_ptr), Some(99));
    assert_eq!(loaded.memory.read_u8(output_ptr + 1), Some(11));

    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::Q3MemoryStorageGetBuffer;
    loaded.cpu.gpr[3] = storage;
    loaded.cpu.gpr[4] = buffer_out_ptr;
    loaded.cpu.gpr[5] = valid_size_out_ptr;
    loaded.cpu.gpr[6] = buffer_size_out_ptr;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], 1);
    assert_eq!(loaded.memory.read_u32_be(buffer_out_ptr), Some(buffer_ptr));
    assert_eq!(loaded.memory.read_u32_be(valid_size_out_ptr), Some(6));
    assert_eq!(loaded.memory.read_u32_be(buffer_size_out_ptr), Some(8));

    let invalid_buffer_out_ptr = PPC_DATA_BASE + 0x1120;
    let invalid_valid_size_out_ptr = PPC_DATA_BASE + 0x1130;
    let invalid_buffer_size_out_ptr = PPC_DATA_BASE + 0x1140;
    loaded
        .memory
        .add_region(invalid_buffer_out_ptr, vec![0xaa; 4]);
    loaded
        .memory
        .add_region(invalid_valid_size_out_ptr, vec![0xbb; 2]);
    loaded
        .memory
        .add_region(invalid_buffer_size_out_ptr, vec![0xcc; 4]);
    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::Q3MemoryStorageGetBuffer;
    loaded.cpu.gpr[3] = storage;
    loaded.cpu.gpr[4] = invalid_buffer_out_ptr;
    loaded.cpu.gpr[5] = invalid_valid_size_out_ptr;
    loaded.cpu.gpr[6] = invalid_buffer_size_out_ptr;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], 0);
    assert_eq!(
        loaded.memory.read_u32_be(invalid_buffer_out_ptr),
        Some(0xaaaa_aaaa)
    );
    assert_eq!(
        loaded.memory.read_u8(invalid_valid_size_out_ptr),
        Some(0xbb)
    );
    assert_eq!(
        loaded.memory.read_u8(invalid_valid_size_out_ptr + 1),
        Some(0xbb)
    );
    assert_eq!(
        loaded.memory.read_u32_be(invalid_buffer_size_out_ptr),
        Some(0xcccc_cccc)
    );

    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::Q3StorageGetType;
    loaded.cpu.gpr[3] = storage;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], PPC_Q3_STORAGE_TYPE_MEMORY);

    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::Q3MemoryStorageGetType;
    loaded.cpu.gpr[3] = storage;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], PPC_Q3_MEMORY_STORAGE_TYPE_HANDLE);

    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::Q3StorageSetData;
    loaded.cpu.gpr[3] = storage;
    loaded.cpu.gpr[4] = 6;
    loaded.cpu.gpr[5] = 6;
    loaded.cpu.gpr[6] = source_ptr;
    loaded.cpu.gpr[7] = valid_size_out_ptr;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], 1);
    assert_eq!(loaded.memory.read_u32_be(valid_size_out_ptr), Some(6));
    assert_eq!(loaded.q3_memory_storages[0].valid_size, 12);
    assert!(loaded.q3_memory_storages[0].buffer_size >= 12);
    assert!(loaded.q3_memory_storages[0].owns_buffer);
    assert_ne!(loaded.q3_memory_storages[0].buffer_ptr, buffer_ptr);
    assert_eq!(
        loaded.q3_objects[0].data_ptr,
        loaded.q3_memory_storages[0].buffer_ptr
    );
    assert_eq!(loaded.q3_objects[0].data_size, 12);

    let wrong_class = storage + PPC_Q3_OBJECT_STRIDE;
    let wrong_record = PpcQ3MemoryStorageRecord {
        storage: wrong_class,
        buffer_ptr: source_ptr,
        valid_size: 3,
        buffer_size: 6,
        owns_buffer: false,
    };
    loaded.q3_objects.push(PpcQ3ObjectRecord {
        object: wrong_class,
        kind: PpcQ3ObjectKind::MemoryStorage,
        object_type: PPC_Q3_TEXTURE_TYPE_MIPMAP,
        source: PpcQ3ObjectSource::default(),
        data_ptr: source_ptr,
        data_size: 3,
    });
    loaded.q3_memory_storages.push(wrong_record);

    loaded.q3_error_state = PpcQ3ErrorState::default();
    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::Q3StorageGetType;
    loaded.cpu.gpr[3] = wrong_class;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], PPC_Q3_TYPE_NONE);
    assert_eq!(
        loaded.q3_error_state.first_error,
        PPC_Q3_ERROR_INVALID_OBJECT_PARAMETER
    );

    loaded.q3_error_state = PpcQ3ErrorState::default();
    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::Q3MemoryStorageGetType;
    loaded.cpu.gpr[3] = wrong_class;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], PPC_Q3_TYPE_NONE);
    assert_eq!(
        loaded.q3_error_state.first_error,
        PPC_Q3_ERROR_INVALID_OBJECT_PARAMETER
    );

    loaded
        .memory
        .write_u32_be(buffer_out_ptr, 0xaaaa_aaaa)
        .unwrap();
    loaded
        .memory
        .write_u32_be(valid_size_out_ptr, 0xbbbb_bbbb)
        .unwrap();
    loaded
        .memory
        .write_u32_be(buffer_size_out_ptr, 0xcccc_cccc)
        .unwrap();
    loaded.q3_error_state = PpcQ3ErrorState::default();
    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::Q3MemoryStorageGetBuffer;
    loaded.cpu.gpr[3] = wrong_class;
    loaded.cpu.gpr[4] = buffer_out_ptr;
    loaded.cpu.gpr[5] = valid_size_out_ptr;
    loaded.cpu.gpr[6] = buffer_size_out_ptr;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], 0);
    assert_eq!(loaded.memory.read_u32_be(buffer_out_ptr), Some(0xaaaa_aaaa));
    assert_eq!(
        loaded.memory.read_u32_be(valid_size_out_ptr),
        Some(0xbbbb_bbbb)
    );
    assert_eq!(
        loaded.memory.read_u32_be(buffer_size_out_ptr),
        Some(0xcccc_cccc)
    );
    assert_eq!(
        loaded.q3_error_state.first_error,
        PPC_Q3_ERROR_INVALID_OBJECT_PARAMETER
    );

    loaded.q3_error_state = PpcQ3ErrorState::default();
    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::Q3MemoryStorageSetBuffer;
    loaded.cpu.gpr[3] = wrong_class;
    loaded.cpu.gpr[4] = 0;
    loaded.cpu.gpr[5] = 0;
    loaded.cpu.gpr[6] = 0;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], 0);
    assert_eq!(
        loaded
            .q3_memory_storages
            .iter()
            .find(|record| record.storage == wrong_class)
            .copied(),
        Some(wrong_record)
    );
    assert_eq!(
        loaded.q3_error_state.first_error,
        PPC_Q3_ERROR_INVALID_OBJECT_PARAMETER
    );
}

#[test]
fn hle_import_runner_handles_q3_fsspec_storage_from_vfs_data_fork() {
    let pef = synthetic_pef_with_library_import(b"QuickDraw\xaa 3D", b"Q3FSSpecStorage_New");
    let mut loaded = load_pef_application(&pef).unwrap();
    let spec_ptr = PPC_DATA_BASE + 0x1000;
    let size_out_ptr = spec_ptr + 0x80;
    let object_type_out_ptr = spec_ptr + 0x84;
    let file = PPC_Q3_OBJECT_BASE + PPC_Q3_OBJECT_STRIDE * 10;
    let mut storage_bytes = Vec::new();
    storage_bytes.extend_from_slice(b"3DMF");
    storage_bytes.extend_from_slice(&0u32.to_be_bytes());
    storage_bytes.extend_from_slice(b"cntr");
    storage_bytes.extend_from_slice(&0u32.to_be_bytes());
    loaded.memory.add_region(spec_ptr, vec![0; 0x100]);
    write_ppc_fsspec(
        &mut loaded.memory,
        spec_ptr,
        PPC_BOOT_VOLUME_REF_NUM,
        PPC_ROOT_DIR_ID,
        b"model.3dmf",
    );
    loaded.push_test_vfs_file(PpcVfsFileRecord {
        path: "model.3dmf".to_string(),
        data: (storage_bytes.clone()).into(),
        creator: 0,
        file_type: u32::from_be_bytes(*b"3DMF"),
        finder_flags: 0,
        dirty: false,
    });
    loaded.cpu.gpr[3] = spec_ptr;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    let storage = loaded.cpu.gpr[3];
    assert_eq!(storage, PPC_Q3_OBJECT_BASE);
    assert_eq!(loaded.q3_objects.len(), 1);
    assert_eq!(loaded.q3_objects[0].object, storage);
    assert_eq!(loaded.q3_objects[0].kind, PpcQ3ObjectKind::Generic);
    assert_eq!(
        loaded.q3_objects[0].object_type,
        PPC_Q3_STORAGE_TYPE_MACINTOSH
    );
    assert_eq!(loaded.q3_objects[0].data_ptr, PPC_HEAP_BASE);
    assert_eq!(
        loaded.q3_objects[0].data_size,
        u32::try_from(storage_bytes.len()).unwrap()
    );
    assert!(loaded.q3_memory_storages.is_empty());
    for (offset, byte) in storage_bytes.iter().copied().enumerate() {
        assert_eq!(
            loaded
                .memory
                .read_u8(PPC_HEAP_BASE + u32::try_from(offset).unwrap()),
            Some(byte)
        );
    }

    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::Q3StorageGetType;
    loaded.cpu.gpr[3] = storage;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], PPC_Q3_STORAGE_TYPE_MACINTOSH);

    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::Q3StorageGetSize;
    loaded.cpu.gpr[3] = storage;
    loaded.cpu.gpr[4] = size_out_ptr;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], 1);
    assert_eq!(
        loaded.memory.read_u32_be(size_out_ptr),
        Some(u32::try_from(storage_bytes.len()).unwrap())
    );

    loaded
        .q3_objects
        .push(test_q3_object(file, PPC_Q3_TYPE_FILE));
    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::Q3FileSetStorage;
    loaded.cpu.gpr[3] = file;
    loaded.cpu.gpr[4] = storage;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], 1);

    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::Q3FileOpenRead;
    loaded.cpu.gpr[3] = file;
    loaded.cpu.gpr[4] = object_type_out_ptr;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], 1);
    assert_eq!(loaded.memory.read_u32_be(object_type_out_ptr), Some(0));

    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::Q3FileReadObject;
    loaded.cpu.gpr[3] = file;

    let probe = loaded.run_with_hle_imports(64);

    let read_object = loaded.cpu.gpr[3];
    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(read_object, PPC_Q3_OBJECT_BASE + PPC_Q3_OBJECT_STRIDE);
    assert_eq!(loaded.q3_objects[2].object, read_object);
    assert_eq!(loaded.q3_objects[2].object_type, PPC_Q3_TYPE_CONTAINER);
    assert_eq!(loaded.q3_objects[2].data_ptr, PPC_HEAP_BASE + 8);
    assert_eq!(loaded.q3_objects[2].data_size, 8);
    assert_eq!(loaded.q3_objects[2].source.file, file);
    assert_eq!(loaded.q3_objects[2].source.offset, 8);
}

#[test]
fn hle_import_runner_q3_fsspec_storage_resolves_stale_alias_dir_ids() {
    let pef = synthetic_pef_with_library_import(b"QuickDraw\xaa 3D", b"Q3FSSpecStorage_New");
    let mut loaded = load_pef_application(&pef).unwrap();
    let spec_ptr = PPC_DATA_BASE + 0x1000;
    let storage_bytes = b"3DMFmodel".to_vec();
    loaded.memory.add_region(spec_ptr, vec![0; 0x100]);
    write_ppc_fsspec(
        &mut loaded.memory,
        spec_ptr,
        PPC_BOOT_VOLUME_REF_NUM,
        13720,
        b"New T-REX_Finished.3df",
    );
    loaded.push_test_vfs_file(PpcVfsFileRecord {
        path: "Data/Skeletons/New T-REX_Finished.3df".to_string(),
        data: (storage_bytes.clone()).into(),
        creator: 0,
        file_type: u32::from_be_bytes(*b"3DMF"),
        finder_flags: 0,
        dirty: false,
    });
    loaded.cpu.gpr[3] = spec_ptr;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    let storage = loaded.cpu.gpr[3];
    assert_eq!(storage, PPC_Q3_OBJECT_BASE);
    assert_eq!(
        loaded.q3_objects[0].data_size,
        u32::try_from(storage_bytes.len()).unwrap()
    );
    for (offset, byte) in storage_bytes.iter().copied().enumerate() {
        assert_eq!(
            loaded
                .memory
                .read_u8(PPC_HEAP_BASE + u32::try_from(offset).unwrap()),
            Some(byte)
        );
    }
}

#[test]
fn hle_import_runner_handles_q3_generic_new_object() {
    let pef = synthetic_pef_with_library_import(b"QuickDraw\xaa 3D", b"Q3Unknown_New");
    let mut loaded = load_pef_application(&pef).unwrap();
    loaded.cpu.gpr[3] = PPC_DATA_BASE;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], PPC_Q3_OBJECT_BASE);
    assert_eq!(loaded.q3_objects.len(), 1);
    assert_eq!(loaded.q3_objects[0].kind, PpcQ3ObjectKind::Generic);
    assert_eq!(loaded.q3_objects[0].data_ptr, PPC_DATA_BASE);
}

#[test]
fn hle_import_runner_handles_q3_view_new_object_state() {
    let pef = synthetic_pef_with_library_import(b"QuickDraw\xaa 3D", b"Q3View_New");
    let mut loaded = load_pef_application(&pef).unwrap();

    let probe = loaded.run_with_hle_imports(64);

    let view = loaded.cpu.gpr[3];
    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(view, PPC_Q3_OBJECT_BASE);
    assert_eq!(loaded.q3_objects.len(), 1);
    assert_eq!(loaded.q3_objects[0].object, view);
    assert_eq!(loaded.q3_objects[0].kind, PpcQ3ObjectKind::Generic);
    assert_eq!(loaded.q3_objects[0].object_type, PPC_Q3_TYPE_VIEW);
    assert_eq!(loaded.q3_objects[0].data_ptr, 0);
    assert_eq!(loaded.q3_objects[0].data_size, 0);
    assert_eq!(loaded.q3_views, vec![PpcQ3ViewStateRecord::new(view)]);

    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::Q3ObjectGetType;
    loaded.cpu.gpr[3] = view;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], PPC_Q3_TYPE_VIEW);

    let renderer = PPC_Q3_OBJECT_BASE + PPC_Q3_OBJECT_STRIDE;
    loaded.q3_objects.push(PpcQ3ObjectRecord {
        object: renderer,
        kind: PpcQ3ObjectKind::Generic,
        object_type: PPC_Q3_RENDERER_TYPE_GENERIC,
        source: PpcQ3ObjectSource::default(),
        data_ptr: 0,
        data_size: 0,
    });
    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::Q3ViewSetRenderer;
    loaded.cpu.gpr[3] = view;
    loaded.cpu.gpr[4] = renderer;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], 1);
    assert_eq!(loaded.q3_views.len(), 1);
    assert_eq!(loaded.q3_views[0].renderer, renderer);

    let renderer_out_ptr = PPC_DATA_BASE + 0x1000;
    loaded.memory.add_region(renderer_out_ptr, vec![0xaa; 4]);
    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::Q3ViewGetRenderer;
    loaded.cpu.gpr[3] = view;
    loaded.cpu.gpr[4] = renderer_out_ptr;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], 1);
    assert_eq!(loaded.memory.read_u32_be(renderer_out_ptr), Some(renderer));
}

#[test]
fn hle_import_runner_handles_q3_file_new_object_state() {
    let pef = synthetic_pef_with_library_import(b"QuickDraw\xaa 3D", b"Q3File_New");
    let mut loaded = load_pef_application(&pef).unwrap();

    let probe = loaded.run_with_hle_imports(64);

    let file = loaded.cpu.gpr[3];
    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(file, PPC_Q3_OBJECT_BASE);
    assert_eq!(loaded.q3_objects.len(), 1);
    assert_eq!(loaded.q3_objects[0].object, file);
    assert_eq!(loaded.q3_objects[0].kind, PpcQ3ObjectKind::Generic);
    assert_eq!(loaded.q3_objects[0].object_type, PPC_Q3_TYPE_FILE);
    assert_eq!(loaded.q3_objects[0].data_ptr, 0);
    assert_eq!(loaded.q3_objects[0].data_size, 0);
    assert_eq!(
        loaded.q3_files,
        vec![PpcQ3FileRecord {
            file,
            storage: 0,
            is_open: false,
            object_type: 0,
            read_offset: 0,
            read_object: 0,
        }]
    );

    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::Q3ObjectGetType;
    loaded.cpu.gpr[3] = file;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], PPC_Q3_TYPE_FILE);

    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::Q3MemoryStorageNewBuffer;
    loaded.cpu.gpr[3] = 0;
    loaded.cpu.gpr[4] = 0;
    loaded.cpu.gpr[5] = 0;

    let probe = loaded.run_with_hle_imports(64);

    let storage = PPC_Q3_OBJECT_BASE + PPC_Q3_OBJECT_STRIDE;
    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], storage);

    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::Q3FileSetStorage;
    loaded.cpu.gpr[3] = file;
    loaded.cpu.gpr[4] = storage;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], 1);
    assert_eq!(loaded.q3_files[0].storage, storage);
}

#[test]
fn hle_import_runner_retains_q3_file_storage_until_file_dispose() {
    fn run_q3_call(loaded: &mut PpcLoadedApp, target: PpcImportDispatcherTarget) -> u32 {
        loaded.cpu.pc = loaded.entry_pc;
        loaded.cpu.lr = PPC_HALT_PC;
        loaded.imports[0].dispatcher_target = target;
        let probe = loaded.run_with_hle_imports(64);
        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        loaded.cpu.gpr[3]
    }

    let pef = synthetic_pef_with_library_import(b"QuickDraw\xaa 3D", b"Q3File_New");
    let mut loaded = load_pef_application(&pef).unwrap();

    let file = run_q3_call(&mut loaded, PpcImportDispatcherTarget::Q3FileNew);

    loaded.cpu.gpr[3] = 0;
    loaded.cpu.gpr[4] = 0;
    loaded.cpu.gpr[5] = 0;
    let storage_a = run_q3_call(
        &mut loaded,
        PpcImportDispatcherTarget::Q3MemoryStorageNewBuffer,
    );

    loaded.cpu.gpr[3] = 0;
    loaded.cpu.gpr[4] = 0;
    loaded.cpu.gpr[5] = 0;
    let storage_b = run_q3_call(
        &mut loaded,
        PpcImportDispatcherTarget::Q3MemoryStorageNewBuffer,
    );

    loaded.cpu.gpr[3] = file;
    loaded.cpu.gpr[4] = storage_a;
    assert_eq!(
        run_q3_call(&mut loaded, PpcImportDispatcherTarget::Q3FileSetStorage),
        1
    );
    assert_eq!(
        ppc_q3_object_reference_count(&loaded.q3_objects, &loaded.q3_object_refs, storage_a),
        Some(2)
    );

    loaded.cpu.gpr[3] = file;
    loaded.cpu.gpr[4] = file;
    assert_eq!(
        run_q3_call(&mut loaded, PpcImportDispatcherTarget::Q3FileSetStorage),
        0
    );
    assert_eq!(loaded.q3_files[0].storage, storage_a);

    loaded.cpu.gpr[3] = storage_a;
    assert_eq!(
        run_q3_call(&mut loaded, PpcImportDispatcherTarget::Q3ObjectDispose),
        1
    );
    assert_eq!(
        ppc_q3_object_reference_count(&loaded.q3_objects, &loaded.q3_object_refs, storage_a),
        Some(1)
    );
    assert!(ppc_q3_object_exists(&loaded.q3_objects, storage_a));

    loaded.cpu.gpr[3] = file;
    loaded.cpu.gpr[4] = storage_b;
    assert_eq!(
        run_q3_call(&mut loaded, PpcImportDispatcherTarget::Q3FileSetStorage),
        1
    );
    assert!(!ppc_q3_object_exists(&loaded.q3_objects, storage_a));
    assert_eq!(
        ppc_q3_object_reference_count(&loaded.q3_objects, &loaded.q3_object_refs, storage_b),
        Some(2)
    );
    assert_eq!(loaded.q3_files[0].storage, storage_b);

    loaded.cpu.gpr[3] = storage_b;
    assert_eq!(
        run_q3_call(&mut loaded, PpcImportDispatcherTarget::Q3ObjectDispose),
        1
    );
    assert_eq!(
        ppc_q3_object_reference_count(&loaded.q3_objects, &loaded.q3_object_refs, storage_b),
        Some(1)
    );

    loaded.cpu.gpr[3] = file;
    assert_eq!(
        run_q3_call(&mut loaded, PpcImportDispatcherTarget::Q3ObjectDispose),
        1
    );
    assert!(!ppc_q3_object_exists(&loaded.q3_objects, file));
    assert!(!ppc_q3_object_exists(&loaded.q3_objects, storage_b));
    assert!(loaded.q3_files.is_empty());
    assert!(loaded.q3_object_refs.is_empty());
}

#[test]
fn hle_import_runner_handles_q3_display_and_illumination_objects() {
    let pef = synthetic_pef_with_library_import(b"QuickDraw\xaa 3D", b"Q3DisplayGroup_New");
    let mut loaded = load_pef_application(&pef).unwrap();
    let calls = [
        (
            PpcImportDispatcherTarget::Q3DisplayGroupNew,
            PPC_Q3_GROUP_TYPE_DISPLAY,
        ),
        (
            PpcImportDispatcherTarget::Q3OrderedDisplayGroupNew,
            PPC_Q3_GROUP_TYPE_ORDERED_DISPLAY,
        ),
        (
            PpcImportDispatcherTarget::Q3LambertIlluminationNew,
            PPC_Q3_ILLUMINATION_TYPE_LAMBERT,
        ),
        (
            PpcImportDispatcherTarget::Q3NullIlluminationNew,
            PPC_Q3_ILLUMINATION_TYPE_NULL,
        ),
        (
            PpcImportDispatcherTarget::Q3PhongIlluminationNew,
            PPC_Q3_ILLUMINATION_TYPE_PHONG,
        ),
    ];

    for (index, (target, object_type)) in calls.into_iter().enumerate() {
        loaded.cpu.pc = loaded.entry_pc;
        loaded.cpu.lr = PPC_HALT_PC;
        loaded.imports[0].dispatcher_target = target;
        loaded.cpu.gpr[3] = 0;

        let probe = loaded.run_with_hle_imports(64);

        let object = PPC_Q3_OBJECT_BASE
            + PPC_Q3_OBJECT_STRIDE * u32::try_from(index).expect("small test object index");
        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(loaded.cpu.gpr[3], object);
        assert_eq!(loaded.q3_objects[index].object, object);
        assert_eq!(loaded.q3_objects[index].kind, PpcQ3ObjectKind::Generic);
        assert_eq!(loaded.q3_objects[index].object_type, object_type);
        assert_eq!(loaded.q3_objects[index].data_ptr, 0);
        assert_eq!(loaded.q3_objects[index].data_size, 0);
    }
}

#[test]
fn hle_import_runner_adds_object_to_ordered_display_group() {
    fn run_q3_call(loaded: &mut PpcLoadedApp, target: PpcImportDispatcherTarget) -> u32 {
        loaded.cpu.pc = loaded.entry_pc;
        loaded.cpu.lr = PPC_HALT_PC;
        loaded.imports[0].dispatcher_target = target;
        let probe = loaded.run_with_hle_imports(64);
        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        loaded.cpu.gpr[3]
    }

    let pef = synthetic_pef_with_library_import(b"QuickDraw\xaa 3D", b"Q3OrderedDisplayGroup_New");
    let mut loaded = load_pef_application(&pef).unwrap();
    let group = run_q3_call(&mut loaded, PpcImportDispatcherTarget::Q3OrderedDisplayGroupNew);
    assert_eq!(
        ppc_q3_object_type_for_handle(&loaded.q3_objects, group),
        PPC_Q3_GROUP_TYPE_ORDERED_DISPLAY
    );

    let object = ppc_q3_alloc_object(
        &mut loaded.q3_objects,
        &mut loaded.next_q3_object,
        PpcQ3ObjectKind::Generic,
        PPC_Q3_TRANSFORM_TYPE_MATRIX,
        0,
        0,
    );
    loaded.cpu.gpr[3] = group;
    loaded.cpu.gpr[4] = object;
    assert_ne!(
        run_q3_call(&mut loaded, PpcImportDispatcherTarget::Q3GroupAddObject),
        0
    );
    assert!(loaded
        .q3_group_memberships
        .iter()
        .any(|member| member.group == group && member.object == object));
}

#[test]
fn hle_import_runner_gets_world_to_frustum_matrix_during_rendering() {
    let pef = synthetic_pef_with_library_import(
        b"QuickDraw\xaa 3D",
        b"Q3View_GetWorldToFrustumMatrixState",
    );
    let mut loaded = load_pef_application(&pef).unwrap();
    let view = ppc_q3_view_new(
        &mut loaded.q3_objects,
        &mut loaded.next_q3_object,
        &mut loaded.q3_views,
    );
    let camera = ppc_q3_alloc_object(
        &mut loaded.q3_objects,
        &mut loaded.next_q3_object,
        PpcQ3ObjectKind::Generic,
        PPC_Q3_CAMERA_TYPE_VIEW_ANGLE_ASPECT,
        0,
        0,
    );
    let camera_record = PpcQ3CameraRecord {
        camera,
        camera_type: PPC_Q3_CAMERA_TYPE_VIEW_ANGLE_ASPECT,
        placement: PpcQ3CameraPlacement {
            camera_location: (1.0, 2.0, 3.0),
            point_of_interest: (1.0, 2.0, 2.0),
            up_vector: (0.0, 1.0, 0.0),
        },
        range_hither: 1.0,
        range_yon: 100.0,
        viewport_origin: (-1.0, -1.0),
        viewport_width: 2.0,
        viewport_height: 2.0,
        projection: PpcQ3CameraProjection::ViewAngleAspect {
            fov: std::f32::consts::FRAC_PI_2,
            aspect_ratio_x_to_y: 4.0 / 3.0,
        },
    };
    loaded.q3_cameras.push(camera_record);
    loaded.q3_views[0].camera = camera;
    let output_ptr = PPC_DATA_BASE + 0x1400;
    loaded
        .memory
        .add_region(output_ptr, vec![0xa5; PPC_Q3_MATRIX4X4_SIZE as usize]);

    let run_call = |loaded: &mut PpcLoadedApp| {
        loaded.cpu.pc = loaded.entry_pc;
        loaded.cpu.lr = PPC_HALT_PC;
        loaded.imports[0].dispatcher_target =
            PpcImportDispatcherTarget::Q3ViewGetWorldToFrustumMatrixState;
        loaded.cpu.gpr[3] = view;
        loaded.cpu.gpr[4] = output_ptr;
        let probe = loaded.run_with_hle_imports(64);
        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        loaded.cpu.gpr[3]
    };

    assert_eq!(run_call(&mut loaded), 0);
    assert_eq!(loaded.memory.read_u8(output_ptr), Some(0xa5));

    loaded.q3_views[0].rendering_depth = 1;
    assert_eq!(run_call(&mut loaded), 1);
    let actual = ppc_read_q3_matrix4x4(&mut loaded.memory, output_ptr).unwrap();
    let world_to_view = ppc_q3_camera_world_to_view_matrix(camera_record.placement).unwrap();
    let view_to_frustum = ppc_q3_camera_view_to_frustum_matrix(&camera_record).unwrap();
    assert_eq!(actual, ppc_q3_matrix4x4_multiply_values(world_to_view, view_to_frustum));
}

#[test]
fn quickdraw_3d_frustum_to_window_matrix_maps_pane_corners() {
    let pane = PpcQ3ViewportRect {
        left: 10,
        top: 20,
        right: 650,
        bottom: 500,
    };
    let matrix = ppc_q3_frustum_to_window_matrix(pane).unwrap();
    assert_eq!(
        ppc_q3_point3d_transform_values((-1.0, 1.0, 0.25), matrix),
        (10.0, 20.0, 0.25)
    );
    assert_eq!(
        ppc_q3_point3d_transform_values((1.0, -1.0, 0.75), matrix),
        (650.0, 500.0, 0.75)
    );
    assert_eq!(matrix[2][2], 1.0);
}

#[test]
fn hle_import_runner_handles_q3_renderer_type_state() {
    let pef = synthetic_pef_with_library_import(b"QuickDraw\xaa 3D", b"Q3Renderer_NewFromType");
    let mut loaded = load_pef_application(&pef).unwrap();
    let renderer_type = u32::from_be_bytes(*b"irnd");
    loaded.cpu.gpr[3] = renderer_type;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    let renderer = loaded.cpu.gpr[3];
    assert_eq!(renderer, PPC_Q3_OBJECT_BASE);
    assert_eq!(loaded.q3_objects.len(), 1);
    assert_eq!(loaded.q3_objects[0].kind, PpcQ3ObjectKind::Generic);
    assert_eq!(loaded.q3_objects[0].object_type, renderer_type);
    assert_eq!(loaded.q3_objects[0].data_ptr, 0);

    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::Q3RendererGetType;
    loaded.cpu.gpr[3] = renderer;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], renderer_type);

    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::Q3ObjectGetType;
    loaded.cpu.gpr[3] = renderer;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], renderer_type);

    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::Q3RendererSync;
    loaded.cpu.gpr[3] = renderer;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], 1);

    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::Q3RendererFlush;
    loaded.cpu.gpr[3] = renderer;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], 1);

    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.cpu.gpr[3] = renderer + PPC_Q3_OBJECT_STRIDE;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], 0);
    assert_eq!(
        loaded.q3_error_state.first_error,
        PPC_Q3_ERROR_INVALID_OBJECT_PARAMETER
    );
    loaded.q3_error_state = PpcQ3ErrorState::default();
    let next_renderer = loaded.next_q3_object;

    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::Q3RendererNewFromType;
    loaded.cpu.gpr[3] = PPC_Q3_TYPE_NONE;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], 0);
    assert_eq!(loaded.q3_objects.len(), 1);
    assert_eq!(loaded.next_q3_object, next_renderer);
    assert_eq!(
        loaded.q3_error_state.first_error,
        PPC_Q3_ERROR_INVALID_OBJECT_PARAMETER
    );
    loaded.q3_error_state = PpcQ3ErrorState::default();

    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.cpu.gpr[3] = PPC_Q3_TYPE_TRIMESH;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], 0);
    assert_eq!(loaded.q3_objects.len(), 1);
    assert_eq!(loaded.next_q3_object, next_renderer);
    assert_eq!(
        loaded.q3_error_state.first_error,
        PPC_Q3_ERROR_INVALID_OBJECT_PARAMETER
    );
}

#[test]
fn hle_import_runner_tracks_q3_interactive_renderer_preferences() {
    let pef = synthetic_pef_with_library_import(
        b"QuickDraw\xaa 3D",
        b"Q3InteractiveRenderer_SetDoubleBufferBypass",
    );
    let mut loaded = load_pef_application(&pef).unwrap();
    let renderer = PPC_Q3_OBJECT_BASE;
    let renderer_type = u32::from_be_bytes(*b"irnd");
    loaded.q3_objects.push(PpcQ3ObjectRecord {
        object: renderer,
        kind: PpcQ3ObjectKind::Generic,
        object_type: renderer_type,
        source: PpcQ3ObjectSource::default(),
        data_ptr: 0,
        data_size: 0,
    });
    loaded.cpu.gpr[3] = renderer;
    loaded.cpu.gpr[4] = 1;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], 1);
    assert_eq!(
        loaded.q3_renderer_preferences,
        vec![PpcQ3RendererPreferenceRecord {
            renderer,
            double_buffer_bypass: Some(1),
            preference_vendor: None,
            preference_engine: None,
            rave_context_hints: None,
            rave_texture_filter: None,
        }]
    );

    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.imports[0].dispatcher_target =
        PpcImportDispatcherTarget::Q3InteractiveRendererSetPreferences;
    loaded.cpu.gpr[3] = renderer;
    loaded.cpu.gpr[4] = PPC_QA_VENDOR_APPLE;
    loaded.cpu.gpr[5] = PPC_QA_ENGINE_APPLE_SW;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], 1);
    assert_eq!(
        loaded.q3_renderer_preferences[0],
        PpcQ3RendererPreferenceRecord {
            renderer,
            double_buffer_bypass: Some(1),
            preference_vendor: Some(PPC_QA_VENDOR_APPLE),
            preference_engine: Some(PPC_QA_ENGINE_APPLE_SW),
            rave_context_hints: None,
            rave_texture_filter: None,
        }
    );

    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.imports[0].dispatcher_target =
        PpcImportDispatcherTarget::Q3InteractiveRendererSetRaveTextureFilter;
    loaded.cpu.gpr[3] = renderer;
    loaded.cpu.gpr[4] = 2;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], 1);
    assert_eq!(
        loaded.q3_renderer_preferences[0],
        PpcQ3RendererPreferenceRecord {
            renderer,
            double_buffer_bypass: Some(1),
            preference_vendor: Some(PPC_QA_VENDOR_APPLE),
            preference_engine: Some(PPC_QA_ENGINE_APPLE_SW),
            rave_context_hints: None,
            rave_texture_filter: Some(2),
        }
    );

    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.imports[0].dispatcher_target =
        PpcImportDispatcherTarget::Q3InteractiveRendererSetRaveContextHints;
    loaded.cpu.gpr[3] = renderer;
    loaded.cpu.gpr[4] = 0x1234_5678;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], 1);
    assert_eq!(
        loaded.q3_renderer_preferences[0],
        PpcQ3RendererPreferenceRecord {
            renderer,
            double_buffer_bypass: Some(1),
            preference_vendor: Some(PPC_QA_VENDOR_APPLE),
            preference_engine: Some(PPC_QA_ENGINE_APPLE_SW),
            rave_context_hints: Some(0x1234_5678),
            rave_texture_filter: Some(2),
        }
    );

    let hints_out_ptr = PPC_DATA_BASE + 0x1000;
    let draw_contexts_out_ptr = hints_out_ptr + 4;
    let engines_out_ptr = hints_out_ptr + 8;
    let count_out_ptr = hints_out_ptr + 12;
    loaded.memory.add_region(hints_out_ptr, vec![0xff; 0x20]);

    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.imports[0].dispatcher_target =
        PpcImportDispatcherTarget::Q3InteractiveRendererGetRaveContextHints;
    loaded.cpu.gpr[3] = renderer;
    loaded.cpu.gpr[4] = hints_out_ptr;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], 1);
    assert_eq!(loaded.memory.read_u32_be(hints_out_ptr), Some(0x1234_5678));

    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.imports[0].dispatcher_target =
        PpcImportDispatcherTarget::Q3InteractiveRendererGetRaveDrawContexts;
    loaded.cpu.gpr[3] = renderer;
    loaded.cpu.gpr[4] = draw_contexts_out_ptr;
    loaded.cpu.gpr[5] = engines_out_ptr;
    loaded.cpu.gpr[6] = count_out_ptr;
    loaded.cpu.gpr[7] = 0;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], 1);
    assert_eq!(
        loaded.memory.read_u32_be(draw_contexts_out_ptr),
        Some(PPC_QA_DRAW_CONTEXT)
    );
    assert_eq!(
        loaded.memory.read_u32_be(engines_out_ptr),
        Some(PPC_QA_ENGINE)
    );
    assert_eq!(loaded.memory.read_u32_be(count_out_ptr), Some(1));
    assert_eq!(
        loaded.memory.read_u32_be(PPC_QA_DRAW_CONTEXT),
        Some(PPC_QA_DRAW_CONTEXT_PRIVATE)
    );
    assert_eq!(
        loaded.memory.read_u32_be(PPC_QA_DRAW_CONTEXT + 4),
        Some(PPC_QA_METHOD_RETURN_ZERO_TVECTOR)
    );
    assert_eq!(
        loaded.memory.read_u32_be(PPC_QA_METHOD_RETURN_ZERO_TVECTOR),
        Some(PPC_QA_METHOD_RETURN_ZERO_ENTRY)
    );
    assert_eq!(
        loaded.memory.read_u32_be(PPC_QA_METHOD_RETURN_ZERO_ENTRY),
        Some(0x3860_0000)
    );
    assert_eq!(
        loaded
            .memory
            .read_u32_be(PPC_QA_METHOD_RETURN_ZERO_ENTRY + 4),
        Some(BLR)
    );

    let invalid_draw_contexts_out_ptr = PPC_DATA_BASE + 0x1100;
    let invalid_engines_out_ptr = PPC_DATA_BASE + 0x1200;
    let invalid_count_out_ptr = PPC_DATA_BASE + 0x1300;
    loaded
        .memory
        .add_region(invalid_draw_contexts_out_ptr, vec![0xaa; 4]);
    loaded
        .memory
        .add_region(invalid_engines_out_ptr, vec![0xbb; 2]);
    loaded
        .memory
        .add_region(invalid_count_out_ptr, vec![0xcc; 4]);
    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.imports[0].dispatcher_target =
        PpcImportDispatcherTarget::Q3InteractiveRendererGetRaveDrawContexts;
    loaded.cpu.gpr[3] = renderer;
    loaded.cpu.gpr[4] = invalid_draw_contexts_out_ptr;
    loaded.cpu.gpr[5] = invalid_engines_out_ptr;
    loaded.cpu.gpr[6] = invalid_count_out_ptr;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], 0);
    assert_eq!(
        loaded.memory.read_u32_be(invalid_draw_contexts_out_ptr),
        Some(0xaaaa_aaaa)
    );
    assert_eq!(loaded.memory.read_u8(invalid_engines_out_ptr), Some(0xbb));
    assert_eq!(
        loaded.memory.read_u8(invalid_engines_out_ptr + 1),
        Some(0xbb)
    );
    assert_eq!(
        loaded.memory.read_u32_be(invalid_count_out_ptr),
        Some(0xcccc_cccc)
    );
    assert_eq!(loaded.q3_error_state, PpcQ3ErrorState::default());

    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.imports[0].dispatcher_target =
        PpcImportDispatcherTarget::Q3InteractiveRendererSetRaveTextureFilter;
    loaded.cpu.gpr[3] = renderer + PPC_Q3_OBJECT_STRIDE;
    loaded.cpu.gpr[4] = 0;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], 0);
    assert_eq!(loaded.q3_renderer_preferences.len(), 1);
    assert_eq!(
        loaded.q3_error_state.first_error,
        PPC_Q3_ERROR_INVALID_OBJECT_PARAMETER
    );
}

#[test]
fn hle_import_runner_handles_q3_pixmap_draw_context_state() {
    let pef =
        synthetic_pef_with_library_import(b"QuickDraw\xaa 3D", b"Q3PixmapDrawContext_New");
    let mut loaded = load_pef_application(&pef).unwrap();
    let scratch_ptr = PPC_DATA_BASE + 0x1000;
    let data_ptr = scratch_ptr;
    let pane_out_ptr = scratch_ptr + 0x100;
    loaded.memory.add_region(scratch_ptr, vec![0; 0x200]);
    ppc_write_q3_area(
        &mut loaded.memory,
        data_ptr + PPC_Q3_DRAW_CONTEXT_PANE_OFFSET,
        9.0,
        9.0,
        9.0,
        9.0,
    )
    .unwrap();
    loaded
        .memory
        .write_u32_be(data_ptr + PPC_Q3_DRAW_CONTEXT_PANE_STATE_OFFSET, 0)
        .unwrap();
    loaded
        .memory
        .write_u32_be(data_ptr + PPC_Q3_DRAW_CONTEXT_DATA_SIZE, 0x1234_5678)
        .unwrap();
    loaded
        .memory
        .write_u32_be(data_ptr + PPC_Q3_PIXMAP_DRAW_CONTEXT_WIDTH_OFFSET, 320)
        .unwrap();
    loaded
        .memory
        .write_u32_be(data_ptr + PPC_Q3_PIXMAP_DRAW_CONTEXT_HEIGHT_OFFSET, 240)
        .unwrap();
    loaded
        .memory
        .write_u32_be(data_ptr + PPC_Q3_DRAW_CONTEXT_DATA_SIZE + 12, 1280)
        .unwrap();
    loaded
        .memory
        .write_u32_be(data_ptr + PPC_Q3_DRAW_CONTEXT_DATA_SIZE + 16, 32)
        .unwrap();
    loaded
        .memory
        .write_u32_be(
            data_ptr + PPC_Q3_DRAW_CONTEXT_DATA_SIZE + 20,
            PPC_Q3_PIXEL_TYPE_RGB32,
        )
        .unwrap();
    loaded
        .memory
        .write_u32_be(
            data_ptr + PPC_Q3_DRAW_CONTEXT_DATA_SIZE + 24,
            PPC_Q3_ENDIAN_BIG,
        )
        .unwrap();
    loaded
        .memory
        .write_u32_be(
            data_ptr + PPC_Q3_DRAW_CONTEXT_DATA_SIZE + 28,
            PPC_Q3_ENDIAN_BIG,
        )
        .unwrap();
    let copied_data = ppc_q3_read_bytes(
        &mut loaded.memory,
        data_ptr,
        PPC_Q3_PIXMAP_DRAW_CONTEXT_DATA_SIZE,
    )
    .unwrap();
    loaded.cpu.gpr[3] = data_ptr;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    let draw_context = loaded.cpu.gpr[3];
    assert_eq!(draw_context, PPC_Q3_OBJECT_BASE);
    assert_eq!(
        loaded.q3_objects[0].object_type,
        PPC_Q3_DRAW_CONTEXT_TYPE_PIXMAP
    );
    assert_eq!(
        loaded.q3_objects[0].data_size,
        PPC_Q3_PIXMAP_DRAW_CONTEXT_DATA_SIZE
    );
    assert_eq!(
        loaded.q3_draw_contexts,
        vec![PpcQ3DrawContextRecord {
            draw_context,
            draw_context_type: PPC_Q3_DRAW_CONTEXT_TYPE_PIXMAP,
            data: copied_data,
        }]
    );

    loaded
        .memory
        .write_u32_be(data_ptr + PPC_Q3_PIXMAP_DRAW_CONTEXT_WIDTH_OFFSET, 640)
        .unwrap();
    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::Q3DrawContextGetPane;
    loaded.cpu.gpr[3] = draw_context;
    loaded.cpu.gpr[4] = pane_out_ptr;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], 1);
    assert_eq!(
        ppc_read_q3_vector2d(&mut loaded.memory, pane_out_ptr),
        Some((0.0, 0.0))
    );
    assert_eq!(
        ppc_read_q3_vector2d(&mut loaded.memory, pane_out_ptr + 8),
        Some((320.0, 240.0))
    );

    loaded.q3_error_state = PpcQ3ErrorState::default();
    loaded.q3_objects[0].object_type = PPC_Q3_TEXTURE_TYPE_MIPMAP;
    for offset in 0..PPC_Q3_DRAW_CONTEXT_PANE_SIZE {
        loaded.memory.write_u8(pane_out_ptr + offset, 0xdd).unwrap();
    }
    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.cpu.gpr[3] = draw_context;
    loaded.cpu.gpr[4] = pane_out_ptr;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], 0);
    for offset in 0..PPC_Q3_DRAW_CONTEXT_PANE_SIZE {
        assert_eq!(loaded.memory.read_u8(pane_out_ptr + offset), Some(0xdd));
    }
    assert_eq!(
        loaded.q3_error_state.first_error,
        PPC_Q3_ERROR_INVALID_OBJECT_PARAMETER
    );
}

#[test]
fn hle_import_runner_handles_q3_mac_draw_context_explicit_pane() {
    let pef = synthetic_pef_with_library_import(b"QuickDraw\xaa 3D", b"Q3MacDrawContext_New");
    let mut loaded = load_pef_application(&pef).unwrap();
    let scratch_ptr = PPC_DATA_BASE + 0x1000;
    let data_ptr = scratch_ptr;
    let pane_out_ptr = scratch_ptr + 0x100;
    loaded.memory.add_region(scratch_ptr, vec![0; 0x200]);
    ppc_write_q3_area(
        &mut loaded.memory,
        data_ptr + PPC_Q3_DRAW_CONTEXT_PANE_OFFSET,
        10.0,
        20.0,
        630.0,
        460.0,
    )
    .unwrap();
    loaded
        .memory
        .write_u32_be(data_ptr + PPC_Q3_DRAW_CONTEXT_PANE_STATE_OFFSET, 1)
        .unwrap();
    loaded
        .memory
        .write_u32_be(data_ptr + PPC_Q3_DRAW_CONTEXT_DATA_SIZE, 0x0102_0304)
        .unwrap();
    let copied_data = ppc_q3_read_bytes(
        &mut loaded.memory,
        data_ptr,
        PPC_Q3_MAC_DRAW_CONTEXT_DATA_SIZE,
    )
    .unwrap();
    loaded.cpu.gpr[3] = data_ptr;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    let draw_context = loaded.cpu.gpr[3];
    assert_eq!(draw_context, PPC_Q3_OBJECT_BASE);
    assert_eq!(
        loaded.q3_objects[0].object_type,
        PPC_Q3_DRAW_CONTEXT_TYPE_MACINTOSH
    );
    assert_eq!(
        loaded.q3_draw_contexts,
        vec![PpcQ3DrawContextRecord {
            draw_context,
            draw_context_type: PPC_Q3_DRAW_CONTEXT_TYPE_MACINTOSH,
            data: copied_data,
        }]
    );

    loaded
        .memory
        .write_u32_be(data_ptr + PPC_Q3_DRAW_CONTEXT_PANE_STATE_OFFSET, 0)
        .unwrap();
    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::Q3DrawContextGetPane;
    loaded.cpu.gpr[3] = draw_context;
    loaded.cpu.gpr[4] = pane_out_ptr;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], 1);
    assert_eq!(
        ppc_read_q3_vector2d(&mut loaded.memory, pane_out_ptr),
        Some((10.0, 20.0))
    );
    assert_eq!(
        ppc_read_q3_vector2d(&mut loaded.memory, pane_out_ptr + 8),
        Some((630.0, 460.0))
    );
}

#[test]
fn q3_mac_draw_context_front_buffer_uses_registered_port() {
    let pef = synthetic_pef_with_library_import(b"QuickDraw\xaa 3D", b"Q3MacDrawContext_New");
    let mut loaded = load_pef_application(&pef).unwrap();
    let draw_context = PPC_Q3_OBJECT_BASE;
    let window_port = PPC_HEAP_BASE + 0x2000;
    let window_base = PPC_HEAP_BASE + 0x4000;
    loaded.gworlds.push(PpcGWorldRecord {
        ui_theme: crate::ui_theme::UiThemeId::ClassicSystem7,
        port: window_port,
        pixmap_handle: 0,
        pixmap: 0,
        base_addr: window_base,
        gdevice: PPC_MAIN_GDEVICE,
        width: 320,
        height: 240,
        depth: 16,
        row_bytes: 640,
        pixels_locked: false,
        pixels_no_purge: false,
    });
    loaded.q3_objects.push(PpcQ3ObjectRecord {
        object: draw_context,
        kind: PpcQ3ObjectKind::Generic,
        object_type: PPC_Q3_DRAW_CONTEXT_TYPE_MACINTOSH,
        source: PpcQ3ObjectSource::default(),
        data_ptr: 0,
        data_size: PPC_Q3_MAC_DRAW_CONTEXT_DATA_SIZE,
    });
    let mut data = vec![0; PPC_Q3_MAC_DRAW_CONTEXT_DATA_SIZE as usize];
    data[PPC_Q3_MAC_DRAW_CONTEXT_PORT_OFFSET as usize
        ..PPC_Q3_MAC_DRAW_CONTEXT_PORT_OFFSET as usize + 4]
        .copy_from_slice(&window_port.to_be_bytes());
    loaded.q3_draw_contexts.push(PpcQ3DrawContextRecord {
        draw_context,
        draw_context_type: PPC_Q3_DRAW_CONTEXT_TYPE_MACINTOSH,
        data,
    });

    assert_eq!(
        loaded.q3_draw_context_front_buffer(draw_context),
        Some(PpcFrontBuffer {
            base_addr: window_base,
            row_bytes: 640,
            width: 320,
            height: 240,
            depth: 16,
        })
    );

    let pane_out_ptr = PPC_DATA_BASE + 0x1000;
    loaded.memory.add_region(pane_out_ptr, vec![0xaa; 16]);
    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::Q3DrawContextGetPane;
    loaded.cpu.gpr[3] = draw_context;
    loaded.cpu.gpr[4] = pane_out_ptr;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], 1);
    assert_eq!(
        ppc_read_q3_vector2d(&mut loaded.memory, pane_out_ptr),
        Some((0.0, 0.0))
    );
    assert_eq!(
        ppc_read_q3_vector2d(&mut loaded.memory, pane_out_ptr + 8),
        Some((320.0, 240.0))
    );
}

#[test]
fn hle_import_runner_handles_q3_mipmap_texture_new_with_copied_state() {
    let pef = synthetic_pef_with_library_import(b"QuickDraw\xaa 3D", b"Q3MipmapTexture_New");
    let mut loaded = load_pef_application(&pef).unwrap();
    let mipmap_ptr = PPC_DATA_BASE + 0x1000;
    let mut mipmap = vec![0; PPC_Q3_MIPMAP_COPY_SIZE as usize];
    for (index, byte) in mipmap.iter_mut().enumerate() {
        *byte = (index.wrapping_mul(3) & 0xff) as u8;
    }
    loaded.memory.add_region(mipmap_ptr, mipmap.clone());
    loaded.cpu.gpr[3] = mipmap_ptr;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], PPC_Q3_OBJECT_BASE);
    assert_eq!(loaded.q3_objects.len(), 1);
    assert_eq!(loaded.q3_objects[0].kind, PpcQ3ObjectKind::Generic);
    assert_eq!(loaded.q3_objects[0].object_type, PPC_Q3_TEXTURE_TYPE_MIPMAP);
    assert_eq!(loaded.q3_objects[0].data_ptr, 0);
    assert_eq!(loaded.q3_objects[0].data_size, PPC_Q3_MIPMAP_COPY_SIZE);
    assert_eq!(
        loaded.q3_mipmap_textures,
        vec![PpcQ3MipmapTextureRecord {
            texture: PPC_Q3_OBJECT_BASE,
            mipmap: mipmap.clone(),
        }]
    );

    loaded.memory.write_u8(mipmap_ptr, 0xff).unwrap();
    assert_eq!(loaded.q3_mipmap_textures[0].mipmap[0], mipmap[0]);
}

#[test]
fn hle_import_runner_retains_q3_mipmap_storage_until_texture_dispose() {
    let pef = synthetic_pef_with_library_import(b"QuickDraw\xaa 3D", b"Q3MipmapTexture_New");
    let mut loaded = load_pef_application(&pef).unwrap();
    let storage = PPC_Q3_OBJECT_BASE;
    let texture = storage + PPC_Q3_OBJECT_STRIDE;
    let mipmap_ptr = PPC_DATA_BASE + 0x1000;
    let mut mipmap = vec![0; PPC_Q3_MIPMAP_COPY_SIZE as usize];
    mipmap[PPC_Q3_MIPMAP_IMAGE_OFFSET as usize..][..4].copy_from_slice(&storage.to_be_bytes());
    loaded.memory.add_region(mipmap_ptr, mipmap);
    loaded.q3_objects.push(PpcQ3ObjectRecord {
        object: storage,
        kind: PpcQ3ObjectKind::Generic,
        object_type: PPC_Q3_STORAGE_TYPE_MEMORY,
        source: PpcQ3ObjectSource::default(),
        data_ptr: 0,
        data_size: 0,
    });
    loaded.next_q3_object = texture;
    loaded.cpu.gpr[3] = mipmap_ptr;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], texture);
    assert_eq!(
        ppc_q3_object_reference_count(&loaded.q3_objects, &loaded.q3_object_refs, storage),
        Some(2)
    );

    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::Q3ObjectDispose;
    loaded.cpu.gpr[3] = storage;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], 1);
    assert!(ppc_q3_object_exists(&loaded.q3_objects, storage));
    assert!(ppc_q3_object_exists(&loaded.q3_objects, texture));
    assert_eq!(
        ppc_q3_object_reference_count(&loaded.q3_objects, &loaded.q3_object_refs, storage),
        Some(1)
    );

    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.cpu.gpr[3] = texture;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], 1);
    assert!(loaded.q3_objects.is_empty());
    assert!(loaded.q3_object_refs.is_empty());
    assert!(loaded.q3_mipmap_textures.is_empty());
}

#[test]
fn hle_import_runner_validates_q3_mipmap_texture_image_storage() {
    let pef = synthetic_pef_with_library_import(b"QuickDraw\xaa 3D", b"Q3MipmapTexture_New");
    let mut loaded = load_pef_application(&pef).unwrap();
    let wrong_storage = PPC_Q3_OBJECT_BASE;
    let texture = wrong_storage + PPC_Q3_OBJECT_STRIDE;
    let mipmap_ptr = PPC_DATA_BASE + 0x1000;
    let raw_image_storage = 0x0bad_cafe;
    let mut mipmap = vec![0; PPC_Q3_MIPMAP_COPY_SIZE as usize];
    mipmap[PPC_Q3_MIPMAP_IMAGE_OFFSET as usize..][..4]
        .copy_from_slice(&wrong_storage.to_be_bytes());
    loaded.memory.add_region(mipmap_ptr, mipmap.clone());
    loaded
        .q3_objects
        .push(test_q3_object(wrong_storage, PPC_Q3_TYPE_TRIMESH));
    loaded.next_q3_object = texture;
    loaded.cpu.gpr[3] = mipmap_ptr;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], 0);
    assert_eq!(loaded.q3_objects.len(), 1);
    assert_eq!(loaded.next_q3_object, texture);
    assert!(loaded.q3_mipmap_textures.is_empty());
    assert!(loaded.q3_object_refs.is_empty());
    assert_eq!(
        loaded.q3_error_state.first_error,
        PPC_Q3_ERROR_INVALID_OBJECT_PARAMETER
    );

    loaded.q3_error_state = PpcQ3ErrorState::default();
    loaded
        .memory
        .write_u32_be(mipmap_ptr + PPC_Q3_MIPMAP_IMAGE_OFFSET, raw_image_storage)
        .unwrap();
    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.cpu.gpr[3] = mipmap_ptr;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], texture);
    assert_eq!(loaded.q3_objects.len(), 2);
    assert_eq!(loaded.q3_objects[1].object_type, PPC_Q3_TEXTURE_TYPE_MIPMAP);
    assert!(loaded.q3_object_refs.is_empty());
    let stored_image_storage =
        ppc_q3_mipmap_texture_image_storage(loaded.q3_mipmap_textures[0].mipmap.as_slice());
    assert_eq!(stored_image_storage, raw_image_storage);
    assert_eq!(loaded.q3_error_state, PpcQ3ErrorState::default());

    loaded.q3_mipmap_textures[0].mipmap[PPC_Q3_MIPMAP_IMAGE_OFFSET as usize..][..4]
        .copy_from_slice(&wrong_storage.to_be_bytes());
    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::Q3ObjectDispose;
    loaded.cpu.gpr[3] = texture;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], 1);
    assert!(ppc_q3_object_exists(&loaded.q3_objects, wrong_storage));
    assert!(!ppc_q3_object_exists(&loaded.q3_objects, texture));
    assert!(loaded.q3_object_refs.is_empty());
    assert!(loaded.q3_mipmap_textures.is_empty());
}

#[test]
fn hle_import_runner_handles_q3_texture_shader_new_links_texture() {
    let pef = synthetic_pef_with_library_import(b"QuickDraw\xaa 3D", b"Q3TextureShader_New");
    let mut loaded = load_pef_application(&pef).unwrap();
    let texture = PPC_Q3_OBJECT_BASE;
    let shader = PPC_Q3_OBJECT_BASE + PPC_Q3_OBJECT_STRIDE;
    loaded.q3_objects.push(PpcQ3ObjectRecord {
        object: texture,
        kind: PpcQ3ObjectKind::Generic,
        object_type: PPC_Q3_TEXTURE_TYPE_MIPMAP,
        source: PpcQ3ObjectSource::default(),
        data_ptr: 0,
        data_size: PPC_Q3_MIPMAP_COPY_SIZE,
    });
    loaded.next_q3_object = shader;
    loaded.cpu.gpr[3] = texture;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], shader);
    assert_eq!(loaded.q3_objects.len(), 2);
    assert_eq!(
        loaded.q3_objects[1].object_type,
        PPC_Q3_SURFACE_SHADER_TYPE_TEXTURE
    );
    assert_eq!(loaded.q3_objects[1].data_ptr, texture);
    assert_eq!(
        loaded.q3_texture_shaders,
        vec![PpcQ3TextureShaderRecord { shader, texture }]
    );
    assert_eq!(
        ppc_q3_object_reference_count(&loaded.q3_objects, &loaded.q3_object_refs, texture),
        Some(2)
    );

    let wrong_class = shader + PPC_Q3_OBJECT_STRIDE * 8;
    let next_after_shader = loaded.next_q3_object;
    loaded.q3_objects.push(PpcQ3ObjectRecord {
        object: wrong_class,
        kind: PpcQ3ObjectKind::Generic,
        object_type: PPC_Q3_STYLE_TYPE_FILL,
        source: PpcQ3ObjectSource::default(),
        data_ptr: 0,
        data_size: 4,
    });
    loaded.q3_error_state = PpcQ3ErrorState::default();
    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.cpu.gpr[3] = wrong_class;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], 0);
    assert_eq!(loaded.q3_objects.len(), 3);
    assert_eq!(loaded.next_q3_object, next_after_shader);
    assert_eq!(
        loaded.q3_texture_shaders,
        vec![PpcQ3TextureShaderRecord { shader, texture }]
    );
    assert_eq!(
        ppc_q3_object_reference_count(&loaded.q3_objects, &loaded.q3_object_refs, wrong_class),
        Some(1)
    );
    assert_eq!(
        loaded.q3_error_state.first_error,
        PPC_Q3_ERROR_INVALID_OBJECT_PARAMETER
    );
}

#[test]
fn hle_import_runner_retains_q3_texture_until_texture_shader_dispose() {
    let pef = synthetic_pef_with_library_import(b"QuickDraw\xaa 3D", b"Q3TextureShader_New");
    let mut loaded = load_pef_application(&pef).unwrap();
    let texture = PPC_Q3_OBJECT_BASE;
    let shader = PPC_Q3_OBJECT_BASE + PPC_Q3_OBJECT_STRIDE;
    loaded.q3_objects.push(PpcQ3ObjectRecord {
        object: texture,
        kind: PpcQ3ObjectKind::Generic,
        object_type: PPC_Q3_TEXTURE_TYPE_MIPMAP,
        source: PpcQ3ObjectSource::default(),
        data_ptr: 0,
        data_size: PPC_Q3_MIPMAP_COPY_SIZE,
    });
    loaded.q3_mipmap_textures.push(PpcQ3MipmapTextureRecord {
        texture,
        mipmap: vec![0; PPC_Q3_MIPMAP_COPY_SIZE as usize],
    });
    loaded.next_q3_object = shader;
    loaded.cpu.gpr[3] = texture;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], shader);
    assert_eq!(
        ppc_q3_object_reference_count(&loaded.q3_objects, &loaded.q3_object_refs, texture),
        Some(2)
    );

    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::Q3ObjectDispose;
    loaded.cpu.gpr[3] = texture;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], 1);
    assert!(ppc_q3_object_exists(&loaded.q3_objects, texture));
    assert!(ppc_q3_object_exists(&loaded.q3_objects, shader));
    assert_eq!(
        ppc_q3_object_reference_count(&loaded.q3_objects, &loaded.q3_object_refs, texture),
        Some(1)
    );
    assert_eq!(
        loaded.q3_texture_shaders,
        vec![PpcQ3TextureShaderRecord { shader, texture }]
    );
    assert_eq!(loaded.q3_mipmap_textures.len(), 1);

    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.cpu.gpr[3] = shader;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], 1);
    assert!(loaded.q3_objects.is_empty());
    assert!(loaded.q3_object_refs.is_empty());
    assert!(loaded.q3_texture_shaders.is_empty());
    assert!(loaded.q3_mipmap_textures.is_empty());
}

#[test]
fn hle_import_runner_handles_q3_texture_shader_new_with_untracked_texture_pointer() {
    let pef = synthetic_pef_with_library_import(b"QuickDraw\xaa 3D", b"Q3TextureShader_New");
    let mut loaded = load_pef_application(&pef).unwrap();
    let texture = PPC_DATA_BASE + 0x1000;
    loaded.cpu.gpr[3] = texture;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], PPC_Q3_OBJECT_BASE);
    assert_eq!(loaded.q3_objects.len(), 1);
    assert_eq!(
        loaded.q3_objects[0].object_type,
        PPC_Q3_SURFACE_SHADER_TYPE_TEXTURE
    );
    assert_eq!(loaded.q3_objects[0].data_ptr, texture);
    assert_eq!(
        loaded.q3_texture_shaders,
        vec![PpcQ3TextureShaderRecord {
            shader: PPC_Q3_OBJECT_BASE,
            texture,
        }]
    );
}

#[test]
fn hle_import_runner_handles_q3_object_dispose() {
    let pef = synthetic_pef_with_library_import(b"QuickDraw\xaa 3D", b"Q3Object_Dispose");
    let mut loaded = load_pef_application(&pef).unwrap();
    loaded.q3_objects.push(PpcQ3ObjectRecord {
        object: PPC_Q3_OBJECT_BASE,
        kind: PpcQ3ObjectKind::Generic,
        object_type: PPC_Q3_TYPE_NONE,
        source: PpcQ3ObjectSource::default(),
        data_ptr: 0,
        data_size: 0,
    });
    loaded.cpu.gpr[3] = PPC_Q3_OBJECT_BASE;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], 1);
    assert!(loaded.q3_objects.is_empty());
}

#[test]
fn hle_import_runner_duplicates_q3_object_references() {
    fn run_q3_call(loaded: &mut PpcLoadedApp, target: PpcImportDispatcherTarget) -> u32 {
        loaded.cpu.pc = loaded.entry_pc;
        loaded.cpu.lr = PPC_HALT_PC;
        loaded.imports[0].dispatcher_target = target;
        let probe = loaded.run_with_hle_imports(64);
        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        loaded.cpu.gpr[3]
    }

    let pef = synthetic_pef_with_library_import(b"QuickDraw\xaa 3D", b"Q3Object_Duplicate");
    let mut loaded = load_pef_application(&pef).unwrap();
    let object = PPC_Q3_OBJECT_BASE;
    loaded.q3_objects.push(PpcQ3ObjectRecord {
        object,
        kind: PpcQ3ObjectKind::Generic,
        object_type: PPC_Q3_TYPE_TRIMESH,
        source: PpcQ3ObjectSource::default(),
        data_ptr: 0,
        data_size: 0,
    });

    loaded.cpu.gpr[3] = object;
    assert_eq!(
        run_q3_call(&mut loaded, PpcImportDispatcherTarget::Q3ObjectDuplicate),
        object
    );
    assert_eq!(
        ppc_q3_object_reference_count(&loaded.q3_objects, &loaded.q3_object_refs, object),
        Some(2)
    );

    loaded.cpu.gpr[3] = object;
    assert_eq!(
        run_q3_call(&mut loaded, PpcImportDispatcherTarget::Q3ObjectDispose),
        1
    );
    assert!(ppc_q3_object_exists(&loaded.q3_objects, object));
    assert_eq!(
        ppc_q3_object_reference_count(&loaded.q3_objects, &loaded.q3_object_refs, object),
        Some(1)
    );

    loaded.cpu.gpr[3] = object;
    assert_eq!(
        run_q3_call(&mut loaded, PpcImportDispatcherTarget::Q3ObjectDispose),
        1
    );
    assert!(!ppc_q3_object_exists(&loaded.q3_objects, object));
    assert!(loaded.q3_object_refs.is_empty());

    loaded.cpu.gpr[3] = object;
    assert_eq!(
        run_q3_call(&mut loaded, PpcImportDispatcherTarget::Q3ObjectDuplicate),
        0
    );
    assert_eq!(
        loaded.q3_error_state.first_error,
        PPC_Q3_ERROR_INVALID_OBJECT_PARAMETER
    );
    assert_eq!(
        loaded.q3_error_state.last_error,
        PPC_Q3_ERROR_INVALID_OBJECT_PARAMETER
    );
}

#[test]
fn hle_import_runner_gets_q3_shared_object_references() {
    fn run_q3_call(loaded: &mut PpcLoadedApp, target: PpcImportDispatcherTarget) -> u32 {
        loaded.cpu.pc = loaded.entry_pc;
        loaded.cpu.lr = PPC_HALT_PC;
        loaded.imports[0].dispatcher_target = target;
        let probe = loaded.run_with_hle_imports(64);
        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        loaded.cpu.gpr[3]
    }

    let pef = synthetic_pef_with_library_import(b"QuickDraw\xaa 3D", b"Q3Shared_GetReference");
    let mut loaded = load_pef_application(&pef).unwrap();
    let shared_object = PPC_Q3_OBJECT_BASE;
    let view_object = PPC_Q3_OBJECT_BASE + PPC_Q3_OBJECT_STRIDE;
    loaded.q3_objects.push(PpcQ3ObjectRecord {
        object: shared_object,
        kind: PpcQ3ObjectKind::Generic,
        object_type: PPC_Q3_TYPE_TRIMESH,
        source: PpcQ3ObjectSource::default(),
        data_ptr: 0,
        data_size: 0,
    });
    loaded.q3_objects.push(PpcQ3ObjectRecord {
        object: view_object,
        kind: PpcQ3ObjectKind::Generic,
        object_type: PPC_Q3_TYPE_VIEW,
        source: PpcQ3ObjectSource::default(),
        data_ptr: 0,
        data_size: 0,
    });

    loaded.cpu.gpr[3] = shared_object;
    assert_eq!(
        run_q3_call(&mut loaded, PpcImportDispatcherTarget::Q3SharedGetReference),
        shared_object
    );
    assert_eq!(
        ppc_q3_object_reference_count(
            &loaded.q3_objects,
            &loaded.q3_object_refs,
            shared_object
        ),
        Some(2)
    );

    loaded.cpu.gpr[3] = shared_object;
    assert_eq!(
        run_q3_call(&mut loaded, PpcImportDispatcherTarget::Q3SharedIsReferenced),
        1
    );

    loaded.cpu.gpr[3] = shared_object;
    assert_eq!(
        run_q3_call(&mut loaded, PpcImportDispatcherTarget::Q3SharedGetType),
        PPC_Q3_SHARED_TYPE_SHAPE
    );

    loaded.cpu.gpr[3] = shared_object;
    assert_eq!(
        run_q3_call(&mut loaded, PpcImportDispatcherTarget::Q3ObjectDispose),
        1
    );
    assert!(ppc_q3_object_exists(&loaded.q3_objects, shared_object));
    assert_eq!(
        ppc_q3_object_reference_count(
            &loaded.q3_objects,
            &loaded.q3_object_refs,
            shared_object
        ),
        Some(1)
    );

    loaded.cpu.gpr[3] = shared_object;
    assert_eq!(
        run_q3_call(&mut loaded, PpcImportDispatcherTarget::Q3SharedIsReferenced),
        0
    );

    loaded.cpu.gpr[3] = view_object;
    assert_eq!(
        run_q3_call(&mut loaded, PpcImportDispatcherTarget::Q3SharedGetReference),
        0
    );
    loaded.cpu.gpr[3] = view_object;
    assert_eq!(
        run_q3_call(&mut loaded, PpcImportDispatcherTarget::Q3SharedGetType),
        PPC_Q3_TYPE_NONE
    );
    assert_eq!(
        ppc_q3_object_reference_count(&loaded.q3_objects, &loaded.q3_object_refs, view_object),
        Some(1)
    );
    assert_eq!(
        loaded.q3_error_state.first_error,
        PPC_Q3_ERROR_INVALID_OBJECT_PARAMETER
    );
    assert_eq!(
        loaded.q3_error_state.last_error,
        PPC_Q3_ERROR_INVALID_OBJECT_PARAMETER
    );
}

#[test]
fn hle_import_runner_handles_q3_shape_type_queries() {
    fn run_q3_call(loaded: &mut PpcLoadedApp, target: PpcImportDispatcherTarget) -> u32 {
        loaded.cpu.pc = loaded.entry_pc;
        loaded.cpu.lr = PPC_HALT_PC;
        loaded.imports[0].dispatcher_target = target;
        let probe = loaded.run_with_hle_imports(64);
        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        loaded.cpu.gpr[3]
    }

    let pef = synthetic_pef_with_library_import(b"QuickDraw\xaa 3D", b"Q3Shape_GetType");
    let mut loaded = load_pef_application(&pef).unwrap();
    let geometry = PPC_Q3_OBJECT_BASE;
    let group = PPC_Q3_OBJECT_BASE + PPC_Q3_OBJECT_STRIDE;
    let camera = PPC_Q3_OBJECT_BASE + PPC_Q3_OBJECT_STRIDE * 2;
    let view = PPC_Q3_OBJECT_BASE + PPC_Q3_OBJECT_STRIDE * 3;
    loaded.q3_objects.push(PpcQ3ObjectRecord {
        object: geometry,
        kind: PpcQ3ObjectKind::Generic,
        object_type: PPC_Q3_TYPE_TRIMESH,
        source: PpcQ3ObjectSource::default(),
        data_ptr: 0,
        data_size: 0,
    });
    loaded.q3_objects.push(PpcQ3ObjectRecord {
        object: group,
        kind: PpcQ3ObjectKind::Generic,
        object_type: PPC_Q3_GROUP_TYPE_DISPLAY,
        source: PpcQ3ObjectSource::default(),
        data_ptr: 0,
        data_size: 0,
    });
    loaded.q3_objects.push(PpcQ3ObjectRecord {
        object: camera,
        kind: PpcQ3ObjectKind::Generic,
        object_type: PPC_Q3_CAMERA_TYPE_VIEW_ANGLE_ASPECT,
        source: PpcQ3ObjectSource::default(),
        data_ptr: 0,
        data_size: 0,
    });
    loaded.q3_objects.push(PpcQ3ObjectRecord {
        object: view,
        kind: PpcQ3ObjectKind::Generic,
        object_type: PPC_Q3_TYPE_VIEW,
        source: PpcQ3ObjectSource::default(),
        data_ptr: 0,
        data_size: 0,
    });

    loaded.cpu.gpr[3] = geometry;
    assert_eq!(
        run_q3_call(&mut loaded, PpcImportDispatcherTarget::Q3ShapeGetType),
        PPC_Q3_SHAPE_TYPE_GEOMETRY
    );

    loaded.cpu.gpr[3] = group;
    assert_eq!(
        run_q3_call(&mut loaded, PpcImportDispatcherTarget::Q3ShapeGetType),
        PPC_Q3_SHAPE_TYPE_GROUP
    );

    loaded.cpu.gpr[3] = camera;
    assert_eq!(
        run_q3_call(&mut loaded, PpcImportDispatcherTarget::Q3ShapeGetType),
        PPC_Q3_SHAPE_TYPE_CAMERA
    );

    loaded.cpu.gpr[3] = geometry;
    assert_eq!(
        run_q3_call(&mut loaded, PpcImportDispatcherTarget::Q3ShapeGetLeafType),
        PPC_Q3_TYPE_TRIMESH
    );

    loaded.cpu.gpr[3] = view;
    assert_eq!(
        run_q3_call(&mut loaded, PpcImportDispatcherTarget::Q3ShapeGetType),
        PPC_Q3_TYPE_NONE
    );
    assert_eq!(
        loaded.q3_error_state.first_error,
        PPC_Q3_ERROR_INVALID_OBJECT_PARAMETER
    );
    assert_eq!(
        loaded.q3_error_state.last_error,
        PPC_Q3_ERROR_INVALID_OBJECT_PARAMETER
    );
}

#[test]
fn hle_import_runner_reports_q3_error_state() {
    let pef = synthetic_pef_with_library_import(b"QuickDraw\xaa 3D", b"Q3Object_Dispose");
    let mut loaded = load_pef_application(&pef).unwrap();
    let first_error_out_ptr = PPC_DATA_BASE + 0x1000;
    let short_first_error_out_ptr = PPC_DATA_BASE + 0x1100;
    loaded.memory.add_region(first_error_out_ptr, vec![0xaa; 4]);
    loaded
        .memory
        .add_region(short_first_error_out_ptr, vec![0xee; 2]);
    loaded.cpu.gpr[3] = PPC_Q3_OBJECT_BASE;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], 0);
    assert_eq!(
        loaded.q3_error_state.first_error,
        PPC_Q3_ERROR_INVALID_OBJECT_PARAMETER
    );
    assert_eq!(
        loaded.q3_error_state.last_error,
        PPC_Q3_ERROR_INVALID_OBJECT_PARAMETER
    );

    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::Q3ErrorGet;
    loaded.cpu.gpr[3] = short_first_error_out_ptr;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], PPC_Q3_ERROR_INVALID_OBJECT_PARAMETER);
    assert_eq!(
        loaded.memory.read_u16_be(short_first_error_out_ptr),
        Some(0xeeee)
    );
    assert_eq!(
        loaded.q3_error_state.first_error,
        PPC_Q3_ERROR_INVALID_OBJECT_PARAMETER
    );
    assert_eq!(
        loaded.q3_error_state.last_error,
        PPC_Q3_ERROR_INVALID_OBJECT_PARAMETER
    );
    assert!(!loaded.q3_error_state.clear_on_next_q3_call);

    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::Q3ErrorGet;
    loaded.cpu.gpr[3] = first_error_out_ptr;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], PPC_Q3_ERROR_INVALID_OBJECT_PARAMETER);
    assert_eq!(
        loaded.memory.read_u32_be(first_error_out_ptr),
        Some(PPC_Q3_ERROR_INVALID_OBJECT_PARAMETER)
    );
    assert!(loaded.q3_error_state.clear_on_next_q3_call);

    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.cpu.gpr[3] = 0;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], PPC_Q3_ERROR_INVALID_OBJECT_PARAMETER);
    assert_eq!(
        loaded.q3_error_state.first_error,
        PPC_Q3_ERROR_INVALID_OBJECT_PARAMETER
    );

    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.imports[0].symbol_name = "Q3Initialize".to_string();
    loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::Q3Initialize;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], 1);
    assert_eq!(loaded.q3_error_state, PpcQ3ErrorState::default());
    assert_eq!(loaded.q3_lifecycle.initialize_count, 1);
    assert_eq!(loaded.q3_lifecycle.initialized_depth, 1);
}

#[test]
fn hle_import_runner_keeps_q3_texture_shader_link_when_texture_disposed() {
    let pef = synthetic_pef_with_library_import(b"QuickDraw\xaa 3D", b"Q3Object_Dispose");
    let mut loaded = load_pef_application(&pef).unwrap();
    let texture = PPC_Q3_OBJECT_BASE;
    let shader = PPC_Q3_OBJECT_BASE + PPC_Q3_OBJECT_STRIDE;
    loaded.q3_objects.push(PpcQ3ObjectRecord {
        object: texture,
        kind: PpcQ3ObjectKind::Generic,
        object_type: PPC_Q3_TEXTURE_TYPE_MIPMAP,
        source: PpcQ3ObjectSource::default(),
        data_ptr: 0,
        data_size: PPC_Q3_MIPMAP_COPY_SIZE,
    });
    loaded.q3_objects.push(PpcQ3ObjectRecord {
        object: shader,
        kind: PpcQ3ObjectKind::Generic,
        object_type: PPC_Q3_SURFACE_SHADER_TYPE_TEXTURE,
        source: PpcQ3ObjectSource::default(),
        data_ptr: texture,
        data_size: 0,
    });
    loaded
        .q3_texture_shaders
        .push(PpcQ3TextureShaderRecord { shader, texture });
    loaded.cpu.gpr[3] = texture;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], 1);
    assert_eq!(loaded.q3_objects.len(), 1);
    assert_eq!(loaded.q3_objects[0].object, shader);
    assert_eq!(
        loaded.q3_texture_shaders,
        vec![PpcQ3TextureShaderRecord { shader, texture }]
    );
}

#[test]
fn q3_object_type_matching_handles_known_quickdraw_3d_hierarchy() {
    assert!(ppc_q3_object_type_matches(
        PPC_Q3_TYPE_TRIMESH,
        PPC_Q3_TYPE_TRIMESH
    ));
    assert!(ppc_q3_object_type_matches(
        PPC_Q3_TYPE_TRIMESH,
        PPC_Q3_SHAPE_TYPE_GEOMETRY
    ));
    assert!(ppc_q3_object_type_matches(
        PPC_Q3_TYPE_TRIMESH,
        PPC_Q3_SHARED_TYPE_SHAPE
    ));
    assert!(ppc_q3_object_type_matches(
        PPC_Q3_SURFACE_SHADER_TYPE_TEXTURE,
        PPC_Q3_SHADER_TYPE_SURFACE
    ));
    assert!(ppc_q3_object_type_matches(
        PPC_Q3_SURFACE_SHADER_TYPE_TEXTURE,
        PPC_Q3_SHAPE_TYPE_SHADER
    ));
    assert!(ppc_q3_object_type_matches(
        PPC_Q3_SHADER_TYPE_SURFACE,
        PPC_Q3_SHAPE_TYPE_SHADER
    ));
    assert!(ppc_q3_object_type_matches(
        PPC_Q3_ILLUMINATION_TYPE_PHONG,
        PPC_Q3_SHADER_TYPE_ILLUMINATION
    ));
    assert!(ppc_q3_object_type_matches(
        PPC_Q3_SHADER_TYPE_ILLUMINATION,
        PPC_Q3_SHAPE_TYPE_SHADER
    ));
    assert!(ppc_q3_object_type_matches(
        PPC_Q3_LIGHT_TYPE_SPOT,
        PPC_Q3_SHAPE_TYPE_LIGHT
    ));
    assert!(ppc_q3_object_type_matches(
        PPC_Q3_CAMERA_TYPE_ORTHOGRAPHIC,
        PPC_Q3_SHAPE_TYPE_CAMERA
    ));
    assert!(ppc_q3_object_type_matches(
        PPC_Q3_CAMERA_TYPE_VIEW_PLANE,
        PPC_Q3_SHARED_TYPE_SHAPE
    ));
    assert!(ppc_q3_object_type_matches(
        PPC_Q3_GROUP_TYPE_LIGHT,
        PPC_Q3_SHAPE_TYPE_GROUP
    ));
    assert!(ppc_q3_object_type_matches(
        PPC_Q3_GROUP_TYPE_ORDERED_DISPLAY,
        PPC_Q3_GROUP_TYPE_DISPLAY
    ));
    assert!(ppc_q3_object_type_matches(
        PPC_Q3_GROUP_TYPE_IO_PROXY_DISPLAY,
        PPC_Q3_SHAPE_TYPE_GROUP
    ));
    assert!(ppc_q3_object_type_matches(
        PPC_Q3_TYPE_ATTRIBUTE_SET,
        PPC_Q3_SHARED_TYPE_SET
    ));
    assert!(ppc_q3_object_type_matches(
        PPC_Q3_TEXTURE_TYPE_MIPMAP,
        PPC_Q3_OBJECT_TYPE_SHARED
    ));
    assert!(ppc_q3_object_type_matches(
        PPC_Q3_RENDERER_TYPE_INTERACTIVE,
        PPC_Q3_SHARED_TYPE_RENDERER
    ));
    assert!(ppc_q3_object_type_matches(
        PPC_Q3_STORAGE_TYPE_MEMORY,
        PPC_Q3_STORAGE_TYPE_MEMORY
    ));
    assert!(ppc_q3_object_type_matches(
        PPC_Q3_MEMORY_STORAGE_TYPE_HANDLE,
        PPC_Q3_STORAGE_TYPE_MEMORY
    ));
    assert!(!ppc_q3_object_type_matches(
        PPC_Q3_TYPE_TRIMESH,
        PPC_Q3_SHAPE_TYPE_LIGHT
    ));
    assert!(!ppc_q3_object_type_matches(
        PPC_Q3_TYPE_NONE,
        PPC_Q3_OBJECT_TYPE_SHARED
    ));
    assert!(!ppc_q3_object_type_matches(
        PPC_Q3_TYPE_TRIMESH,
        PPC_Q3_TYPE_NONE
    ));
}

#[test]
fn q3_object_drawable_matching_handles_known_quickdraw_3d_classes() {
    assert!(ppc_q3_object_type_is_drawable(PPC_Q3_TYPE_CONTAINER));
    assert!(ppc_q3_object_type_is_drawable(PPC_Q3_TYPE_TRIMESH));
    assert!(ppc_q3_object_type_is_drawable(
        PPC_Q3_SURFACE_SHADER_TYPE_TEXTURE
    ));
    assert!(ppc_q3_object_type_is_drawable(
        PPC_Q3_ILLUMINATION_TYPE_LAMBERT
    ));
    assert!(ppc_q3_object_type_is_drawable(PPC_Q3_STYLE_TYPE_BACKFACING));
    assert!(ppc_q3_object_type_is_drawable(PPC_Q3_TRANSFORM_TYPE_MATRIX));
    assert!(ppc_q3_object_type_is_drawable(PPC_Q3_GROUP_TYPE_DISPLAY));
    assert!(ppc_q3_object_type_is_drawable(
        PPC_Q3_GROUP_TYPE_ORDERED_DISPLAY
    ));
    assert!(ppc_q3_object_type_is_drawable(
        PPC_Q3_GROUP_TYPE_IO_PROXY_DISPLAY
    ));
    assert!(ppc_q3_object_type_is_drawable(PPC_Q3_TYPE_ATTRIBUTE_SET));
    assert!(!ppc_q3_object_type_is_drawable(PPC_Q3_GROUP_TYPE_LIGHT));
    assert!(!ppc_q3_object_type_is_drawable(PPC_Q3_LIGHT_TYPE_AMBIENT));
    assert!(!ppc_q3_object_type_is_drawable(
        PPC_Q3_CAMERA_TYPE_VIEW_ANGLE_ASPECT
    ));
    assert!(!ppc_q3_object_type_is_drawable(
        PPC_Q3_CAMERA_TYPE_ORTHOGRAPHIC
    ));
    assert!(!ppc_q3_object_type_is_drawable(
        PPC_Q3_CAMERA_TYPE_VIEW_PLANE
    ));
    assert!(!ppc_q3_object_type_is_drawable(PPC_Q3_TYPE_FILE));
    assert!(!ppc_q3_object_type_is_drawable(PPC_Q3_TEXTURE_TYPE_MIPMAP));
    assert!(!ppc_q3_object_type_is_drawable(
        PPC_Q3_DRAW_CONTEXT_TYPE_PIXMAP
    ));
    assert!(!ppc_q3_object_type_is_drawable(
        PPC_Q3_RENDERER_TYPE_INTERACTIVE
    ));
}

#[test]
fn hle_import_runner_handles_q3_object_type_queries() {
    let pef = synthetic_pef_with_library_import(b"QuickDraw\xaa 3D", b"Q3Object_GetType");
    let mut loaded = load_pef_application(&pef).unwrap();
    let object = PPC_Q3_OBJECT_BASE;
    let display_group = PPC_Q3_OBJECT_BASE + PPC_Q3_OBJECT_STRIDE;
    let memory_storage = PPC_Q3_OBJECT_BASE + PPC_Q3_OBJECT_STRIDE * 2;
    let shader = PPC_Q3_OBJECT_BASE + PPC_Q3_OBJECT_STRIDE * 3;
    loaded.q3_objects.push(PpcQ3ObjectRecord {
        object,
        kind: PpcQ3ObjectKind::Generic,
        object_type: PPC_Q3_TYPE_TRIMESH,
        source: PpcQ3ObjectSource::default(),
        data_ptr: 0,
        data_size: 0,
    });
    loaded.q3_objects.push(PpcQ3ObjectRecord {
        object: display_group,
        kind: PpcQ3ObjectKind::Generic,
        object_type: PPC_Q3_GROUP_TYPE_DISPLAY,
        source: PpcQ3ObjectSource::default(),
        data_ptr: 0,
        data_size: 0,
    });
    loaded.q3_objects.push(PpcQ3ObjectRecord {
        object: memory_storage,
        kind: PpcQ3ObjectKind::MemoryStorage,
        object_type: PPC_Q3_STORAGE_TYPE_MEMORY,
        source: PpcQ3ObjectSource::default(),
        data_ptr: 0,
        data_size: 0,
    });
    loaded.q3_objects.push(PpcQ3ObjectRecord {
        object: shader,
        kind: PpcQ3ObjectKind::Generic,
        object_type: PPC_Q3_SURFACE_SHADER_TYPE_TEXTURE,
        source: PpcQ3ObjectSource::default(),
        data_ptr: 0,
        data_size: 0,
    });
    loaded.cpu.gpr[3] = object;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], PPC_Q3_TYPE_TRIMESH);

    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::Q3ObjectIsDrawable;
    loaded.cpu.gpr[3] = object;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], 1);

    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.cpu.gpr[3] = display_group;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], 1);

    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::Q3ObjectIsType;
    loaded.cpu.gpr[3] = object;
    loaded.cpu.gpr[4] = PPC_Q3_TYPE_TRIMESH;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], 1);

    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.cpu.gpr[3] = object;
    loaded.cpu.gpr[4] = PPC_Q3_SHAPE_TYPE_GEOMETRY;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], 1);

    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.cpu.gpr[3] = object;
    loaded.cpu.gpr[4] = PPC_Q3_SHARED_TYPE_SHAPE;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], 1);

    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.cpu.gpr[3] = memory_storage;
    loaded.cpu.gpr[4] = PPC_Q3_STORAGE_TYPE_MEMORY;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], 1);

    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.cpu.gpr[3] = object;
    loaded.cpu.gpr[4] = PPC_Q3_TYPE_CONTAINER;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], 0);

    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::Q3ObjectGetType;
    loaded.cpu.gpr[3] = object;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], PPC_Q3_TYPE_TRIMESH);

    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::Q3GeometryGetType;
    loaded.cpu.gpr[3] = object;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], PPC_Q3_TYPE_TRIMESH);

    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::Q3GroupGetType;
    loaded.cpu.gpr[3] = display_group;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], PPC_Q3_GROUP_TYPE_DISPLAY);

    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::Q3ShaderGetType;
    loaded.cpu.gpr[3] = shader;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], PPC_Q3_SURFACE_SHADER_TYPE_TEXTURE);

    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::Q3ObjectGetLeafType;
    loaded.cpu.gpr[3] = object;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], PPC_Q3_TYPE_TRIMESH);

    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.cpu.gpr[3] = object + PPC_Q3_OBJECT_STRIDE * 4;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], PPC_Q3_TYPE_NONE);
    assert_eq!(loaded.q3_error_state.first_error, PPC_Q3_ERROR_NONE);

    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::Q3ObjectIsDrawable;
    loaded.cpu.gpr[3] = memory_storage;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], 0);
    assert_eq!(loaded.q3_error_state.first_error, PPC_Q3_ERROR_NONE);

    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.cpu.gpr[3] = object + PPC_Q3_OBJECT_STRIDE * 4;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], 0);
    assert_eq!(
        loaded.q3_error_state.first_error,
        PPC_Q3_ERROR_INVALID_OBJECT_PARAMETER
    );

    loaded.q3_error_state = PpcQ3ErrorState::default();
    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::Q3ObjectIsType;
    loaded.cpu.gpr[3] = object + PPC_Q3_OBJECT_STRIDE * 4;
    loaded.cpu.gpr[4] = PPC_Q3_SHARED_TYPE_SHAPE;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], 0);
    assert_eq!(
        loaded.q3_error_state.first_error,
        PPC_Q3_ERROR_INVALID_OBJECT_PARAMETER
    );

    loaded.q3_error_state = PpcQ3ErrorState::default();

    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::Q3GeometryGetType;
    loaded.cpu.gpr[3] = memory_storage;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], PPC_Q3_TYPE_NONE);
    assert_eq!(
        loaded.q3_error_state.first_error,
        PPC_Q3_ERROR_INVALID_OBJECT_PARAMETER
    );
    assert_eq!(
        loaded.q3_error_state.last_error,
        PPC_Q3_ERROR_INVALID_OBJECT_PARAMETER
    );
}

#[test]
fn hle_import_runner_handles_q3_texture_shader_get_texture() {
    let pef =
        synthetic_pef_with_library_import(b"QuickDraw\xaa 3D", b"Q3TextureShader_GetTexture");
    let mut loaded = load_pef_application(&pef).unwrap();
    let texture = PPC_Q3_OBJECT_BASE;
    let shader = PPC_Q3_OBJECT_BASE + PPC_Q3_OBJECT_STRIDE;
    let texture_out_ptr = PPC_DATA_BASE + 0x1000;
    loaded.memory.add_region(texture_out_ptr, vec![0; 4]);
    loaded.q3_objects.push(PpcQ3ObjectRecord {
        object: texture,
        kind: PpcQ3ObjectKind::Generic,
        object_type: PPC_Q3_TEXTURE_TYPE_MIPMAP,
        source: PpcQ3ObjectSource::default(),
        data_ptr: 0,
        data_size: PPC_Q3_MIPMAP_COPY_SIZE,
    });
    loaded.q3_objects.push(PpcQ3ObjectRecord {
        object: shader,
        kind: PpcQ3ObjectKind::Generic,
        object_type: PPC_Q3_SURFACE_SHADER_TYPE_TEXTURE,
        source: PpcQ3ObjectSource::default(),
        data_ptr: 0,
        data_size: 0,
    });
    loaded
        .q3_texture_shaders
        .push(PpcQ3TextureShaderRecord { shader, texture });
    loaded.cpu.gpr[3] = shader;
    loaded.cpu.gpr[4] = texture_out_ptr;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], 1);
    assert_eq!(loaded.memory.read_u32_be(texture_out_ptr), Some(texture));
    assert_eq!(
        ppc_q3_object_reference_count(&loaded.q3_objects, &loaded.q3_object_refs, texture),
        Some(2)
    );

    let wrong_class_texture = shader + PPC_Q3_OBJECT_STRIDE * 8;
    loaded.q3_objects.push(PpcQ3ObjectRecord {
        object: wrong_class_texture,
        kind: PpcQ3ObjectKind::Generic,
        object_type: PPC_Q3_STYLE_TYPE_FILL,
        source: PpcQ3ObjectSource::default(),
        data_ptr: 0,
        data_size: 4,
    });
    loaded.q3_texture_shaders[0].texture = wrong_class_texture;
    loaded
        .memory
        .write_u32_be(texture_out_ptr, 0xcccc_cccc)
        .unwrap();
    loaded.q3_error_state = PpcQ3ErrorState::default();
    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.cpu.gpr[3] = shader;
    loaded.cpu.gpr[4] = texture_out_ptr;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], 0);
    assert_eq!(
        loaded.memory.read_u32_be(texture_out_ptr),
        Some(0xcccc_cccc)
    );
    assert_eq!(
        ppc_q3_object_reference_count(
            &loaded.q3_objects,
            &loaded.q3_object_refs,
            wrong_class_texture,
        ),
        Some(1)
    );
    assert_eq!(
        loaded.q3_error_state.first_error,
        PPC_Q3_ERROR_INVALID_OBJECT_PARAMETER
    );
    loaded.q3_texture_shaders[0].texture = texture;
    loaded.q3_error_state = PpcQ3ErrorState::default();

    let wrong_class_shader = shader + PPC_Q3_OBJECT_STRIDE;
    loaded.q3_objects.push(PpcQ3ObjectRecord {
        object: wrong_class_shader,
        kind: PpcQ3ObjectKind::Generic,
        object_type: PPC_Q3_TEXTURE_TYPE_MIPMAP,
        source: PpcQ3ObjectSource::default(),
        data_ptr: texture,
        data_size: 0,
    });
    loaded
        .memory
        .write_u32_be(texture_out_ptr, 0xdddd_dddd)
        .unwrap();
    loaded.q3_error_state = PpcQ3ErrorState::default();
    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.cpu.gpr[3] = wrong_class_shader;
    loaded.cpu.gpr[4] = texture_out_ptr;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], 0);
    assert_eq!(
        loaded.memory.read_u32_be(texture_out_ptr),
        Some(0xdddd_dddd)
    );
    assert_eq!(
        ppc_q3_object_reference_count(&loaded.q3_objects, &loaded.q3_object_refs, texture),
        Some(2)
    );
    assert_eq!(
        loaded.q3_error_state.first_error,
        PPC_Q3_ERROR_INVALID_OBJECT_PARAMETER
    );
}

#[test]
fn hle_import_runner_handles_q3_texture_shader_get_texture_from_object_payload() {
    let pef =
        synthetic_pef_with_library_import(b"QuickDraw\xaa 3D", b"Q3TextureShader_GetTexture");
    let mut loaded = load_pef_application(&pef).unwrap();
    let shader = PPC_Q3_OBJECT_BASE;
    let texture = PPC_Q3_OBJECT_BASE + PPC_Q3_OBJECT_STRIDE;
    let texture_out_ptr = PPC_DATA_BASE + 0x1000;
    loaded.memory.add_region(texture_out_ptr, vec![0; 4]);
    loaded.q3_objects.push(PpcQ3ObjectRecord {
        object: shader,
        kind: PpcQ3ObjectKind::Generic,
        object_type: PPC_Q3_SURFACE_SHADER_TYPE_TEXTURE,
        source: PpcQ3ObjectSource::default(),
        data_ptr: texture,
        data_size: 0,
    });
    loaded.cpu.gpr[3] = shader;
    loaded.cpu.gpr[4] = texture_out_ptr;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], 1);
    assert_eq!(loaded.memory.read_u32_be(texture_out_ptr), Some(texture));
}

#[test]
fn hle_import_runner_prevalidates_q3_renderer_getter_outputs() {
    fn q3_test_object(
        object: u32,
        object_type: u32,
        data_ptr: u32,
        data_size: u32,
    ) -> PpcQ3ObjectRecord {
        PpcQ3ObjectRecord {
            object,
            kind: PpcQ3ObjectKind::Generic,
            object_type,
            source: PpcQ3ObjectSource::default(),
            data_ptr,
            data_size,
        }
    }

    let pef =
        synthetic_pef_with_library_import(b"QuickDraw\xaa 3D", b"Q3MipmapTexture_GetMipmap");
    let mut loaded = load_pef_application(&pef).unwrap();
    let texture = PPC_Q3_OBJECT_BASE;
    let draw_context = texture + PPC_Q3_OBJECT_STRIDE;
    let trimesh = texture + PPC_Q3_OBJECT_STRIDE * 2;
    let shader = texture + PPC_Q3_OBJECT_STRIDE * 3;
    let attribute_set = texture + PPC_Q3_OBJECT_STRIDE * 4;
    let transform = texture + PPC_Q3_OBJECT_STRIDE * 5;
    let mipmap_storage = texture + PPC_Q3_OBJECT_STRIDE * 6;
    let matrix_ptr = PPC_DATA_BASE + 0x2000;
    let mut mipmap = vec![0x11; PPC_Q3_MIPMAP_COPY_SIZE as usize];
    mipmap[PPC_Q3_MIPMAP_IMAGE_OFFSET as usize..][..4]
        .copy_from_slice(&mipmap_storage.to_be_bytes());
    loaded
        .memory
        .add_region(matrix_ptr, vec![0; PPC_Q3_MATRIX4X4_SIZE as usize]);
    loaded.q3_objects.extend([
        q3_test_object(
            texture,
            PPC_Q3_TEXTURE_TYPE_MIPMAP,
            0,
            PPC_Q3_MIPMAP_COPY_SIZE,
        ),
        q3_test_object(
            draw_context,
            PPC_Q3_DRAW_CONTEXT_TYPE_PIXMAP,
            0,
            PPC_Q3_PIXMAP_DRAW_CONTEXT_DATA_SIZE,
        ),
        q3_test_object(trimesh, PPC_Q3_TYPE_TRIMESH, 0, PPC_Q3_TRIMESH_DATA_SIZE),
        q3_test_object(shader, PPC_Q3_SURFACE_SHADER_TYPE_TEXTURE, 0, 0),
        q3_test_object(attribute_set, PPC_Q3_TYPE_ATTRIBUTE_SET, 0, 0),
        q3_test_object(
            transform,
            PPC_Q3_TRANSFORM_TYPE_MATRIX,
            matrix_ptr,
            PPC_Q3_MATRIX4X4_SIZE,
        ),
        q3_test_object(mipmap_storage, PPC_Q3_STORAGE_TYPE_MEMORY, 0, 0),
    ]);
    loaded
        .q3_mipmap_textures
        .push(PpcQ3MipmapTextureRecord { texture, mipmap });
    loaded.q3_draw_contexts.push(PpcQ3DrawContextRecord {
        draw_context,
        draw_context_type: PPC_Q3_DRAW_CONTEXT_TYPE_PIXMAP,
        data: vec![0x22; PPC_Q3_PIXMAP_DRAW_CONTEXT_DATA_SIZE as usize],
    });
    loaded.q3_trimeshes.push(PpcQ3TriMeshRecord {
        trimesh,
        data: vec![0x33; PPC_Q3_TRIMESH_DATA_SIZE as usize],
        triangle_attribute_sets: Vec::new(),
        get_data_copies: Vec::new(),
    });
    loaded
        .q3_shader_uv_transforms
        .push(PpcQ3ShaderUvTransformRecord {
            shader,
            matrix: ppc_q3_matrix3x3_identity(),
        });
    loaded.q3_attributes.push(PpcQ3AttributeRecord {
        attribute_set,
        attribute_type: PPC_Q3_ATTRIBUTE_TYPE_DIFFUSE_COLOR,
        data: vec![0x44; PPC_Q3_VECTOR3D_SIZE as usize],
    });

    let invalid_getters = [
        (
            PpcImportDispatcherTarget::Q3MipmapTextureGetMipmap,
            texture,
            0,
            4,
            PPC_Q3_MIPMAP_COPY_SIZE,
            0xd1u8,
        ),
        (
            PpcImportDispatcherTarget::Q3DrawContextGetPane,
            draw_context,
            0,
            4,
            PPC_Q3_DRAW_CONTEXT_PANE_SIZE,
            0xd2u8,
        ),
        (
            PpcImportDispatcherTarget::Q3TriMeshGetData,
            trimesh,
            0,
            4,
            PPC_Q3_TRIMESH_DATA_SIZE,
            0xd3u8,
        ),
        (
            PpcImportDispatcherTarget::Q3ShaderGetUVTransform,
            shader,
            0,
            4,
            PPC_Q3_MATRIX3X3_SIZE,
            0xd4u8,
        ),
        (
            PpcImportDispatcherTarget::Q3AttributeSetGet,
            attribute_set,
            PPC_Q3_ATTRIBUTE_TYPE_DIFFUSE_COLOR,
            5,
            PPC_Q3_VECTOR3D_SIZE,
            0xd5u8,
        ),
        (
            PpcImportDispatcherTarget::Q3TransformGetMatrix,
            transform,
            0,
            4,
            PPC_Q3_MATRIX4X4_SIZE,
            0xd6u8,
        ),
    ];
    for (index, (target, object, arg4, output_gpr, size, sentinel)) in
        invalid_getters.iter().enumerate()
    {
        let invalid_output_ptr = PPC_DATA_BASE + 0x5000 + (index as u32) * 0x400;
        loaded
            .memory
            .add_region(invalid_output_ptr, vec![*sentinel; (*size - 1) as usize]);
        loaded.cpu.pc = loaded.entry_pc;
        loaded.cpu.lr = PPC_HALT_PC;
        loaded.imports[0].dispatcher_target = target.clone();
        loaded.cpu.gpr[3] = *object;
        loaded.cpu.gpr[4] = if *output_gpr == 4 {
            invalid_output_ptr
        } else {
            *arg4
        };
        loaded.cpu.gpr[5] = if *output_gpr == 5 {
            invalid_output_ptr
        } else {
            0
        };

        let probe = loaded.run_with_hle_imports(64);

        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(loaded.cpu.gpr[3], 0);
        for offset in 0..(*size - 1) {
            assert_eq!(
                loaded.memory.read_u8(invalid_output_ptr + offset),
                Some(*sentinel)
            );
        }
    }
    assert_eq!(
        ppc_q3_object_reference_count(
            &loaded.q3_objects,
            &loaded.q3_object_refs,
            mipmap_storage
        ),
        Some(1)
    );
}

#[test]
fn hle_import_runner_prevalidates_q3_scalar_getter_outputs() {
    fn q3_test_object(
        object: u32,
        object_type: u32,
        data_ptr: u32,
        data_size: u32,
    ) -> PpcQ3ObjectRecord {
        PpcQ3ObjectRecord {
            object,
            kind: PpcQ3ObjectKind::Generic,
            object_type,
            source: PpcQ3ObjectSource::default(),
            data_ptr,
            data_size,
        }
    }

    let pef =
        synthetic_pef_with_library_import(b"QuickDraw\xaa 3D", b"Q3TextureShader_GetTexture");
    let mut loaded = load_pef_application(&pef).unwrap();
    let texture = PPC_Q3_OBJECT_BASE;
    let shader = texture + PPC_Q3_OBJECT_STRIDE;
    let fill_style = texture + PPC_Q3_OBJECT_STRIDE * 2;
    let group = texture + PPC_Q3_OBJECT_STRIDE * 3;
    let container = texture + PPC_Q3_OBJECT_STRIDE * 4;
    let trimesh = texture + PPC_Q3_OBJECT_STRIDE * 5;
    let view = texture + PPC_Q3_OBJECT_STRIDE * 6;
    let renderer = texture + PPC_Q3_OBJECT_STRIDE * 7;
    let draw_context = texture + PPC_Q3_OBJECT_STRIDE * 8;
    let camera = texture + PPC_Q3_OBJECT_STRIDE * 9;
    let light_group = texture + PPC_Q3_OBJECT_STRIDE * 10;
    loaded.q3_objects.extend([
        q3_test_object(texture, PPC_Q3_TEXTURE_TYPE_MIPMAP, 0, 0),
        q3_test_object(shader, PPC_Q3_SURFACE_SHADER_TYPE_TEXTURE, 0, 0),
        q3_test_object(fill_style, PPC_Q3_STYLE_TYPE_FILL, 0, 0),
        q3_test_object(group, PPC_Q3_GROUP_TYPE_DISPLAY, 0, 0),
        q3_test_object(container, PPC_Q3_TYPE_CONTAINER, 0, 0),
        q3_test_object(trimesh, PPC_Q3_TYPE_TRIMESH, 0, PPC_Q3_TRIMESH_DATA_SIZE),
        q3_test_object(view, PPC_Q3_TYPE_VIEW, 0, 0),
        q3_test_object(renderer, PPC_Q3_RENDERER_TYPE_GENERIC, 0, 0),
        q3_test_object(draw_context, PPC_Q3_DRAW_CONTEXT_TYPE_PIXMAP, 0, 0),
        q3_test_object(camera, PPC_Q3_CAMERA_TYPE_VIEW_ANGLE_ASPECT, 0, 0),
        q3_test_object(light_group, PPC_Q3_GROUP_TYPE_LIGHT, 0, 0),
    ]);
    loaded
        .q3_texture_shaders
        .push(PpcQ3TextureShaderRecord { shader, texture });
    loaded.q3_shader_boundaries.push(PpcQ3ShaderBoundaryRecord {
        shader,
        u_boundary: PPC_Q3_SHADER_UV_BOUNDARY_CLAMP,
        v_boundary: PPC_Q3_SHADER_UV_BOUNDARY_WRAP,
    });
    loaded.q3_styles.push(PpcQ3StyleRecord {
        style: fill_style,
        kind: PpcQ3StyleKind::Fill,
        value: PPC_Q3_FILL_STYLE_POINTS,
    });
    loaded.q3_group_memberships.extend([
        PpcQ3GroupMembershipRecord {
            group,
            object: container,
            before: None,
        },
        PpcQ3GroupMembershipRecord {
            group,
            object: trimesh,
            before: None,
        },
    ]);
    loaded.q3_views.push(PpcQ3ViewStateRecord {
        view,
        renderer,
        light_group,
        draw_context,
        camera,
        rendering_depth: 0,
        bounding_box_depth: 0,
        cancelled: false,
    });

    let invalid_getters = [
        (
            PpcImportDispatcherTarget::Q3TextureShaderGetTexture,
            shader,
            0,
            4,
            0xe1u8,
            Some(texture),
        ),
        (
            PpcImportDispatcherTarget::Q3ShaderGetUBoundary,
            shader,
            0,
            4,
            0xe2u8,
            None,
        ),
        (
            PpcImportDispatcherTarget::Q3FillStyleGet,
            fill_style,
            0,
            4,
            0xe3u8,
            None,
        ),
        (
            PpcImportDispatcherTarget::Q3GroupCountObjects,
            group,
            0,
            4,
            0xe4u8,
            None,
        ),
        (
            PpcImportDispatcherTarget::Q3GroupGetFirstPosition,
            group,
            0,
            4,
            0xe5u8,
            None,
        ),
        (
            PpcImportDispatcherTarget::Q3GroupGetFirstPositionOfType,
            group,
            PPC_Q3_TYPE_TRIMESH,
            5,
            0xe6u8,
            None,
        ),
        (
            PpcImportDispatcherTarget::Q3GroupGetPositionObject,
            group,
            1,
            5,
            0xe7u8,
            Some(container),
        ),
        (
            PpcImportDispatcherTarget::Q3ViewGetRenderer,
            view,
            0,
            4,
            0xe8u8,
            Some(renderer),
        ),
        (
            PpcImportDispatcherTarget::Q3ViewGetDrawContext,
            view,
            0,
            4,
            0xe9u8,
            Some(draw_context),
        ),
        (
            PpcImportDispatcherTarget::Q3ViewGetCamera,
            view,
            0,
            4,
            0xeau8,
            Some(camera),
        ),
        (
            PpcImportDispatcherTarget::Q3ViewGetLightGroup,
            view,
            0,
            4,
            0xebu8,
            Some(light_group),
        ),
    ];
    for (index, (target, object, arg4, output_gpr, sentinel, retained_object)) in
        invalid_getters.iter().enumerate()
    {
        let invalid_output_ptr = PPC_DATA_BASE + 0x7000 + (index as u32) * 0x100;
        loaded
            .memory
            .add_region(invalid_output_ptr, vec![*sentinel; 3]);
        loaded.cpu.pc = loaded.entry_pc;
        loaded.cpu.lr = PPC_HALT_PC;
        loaded.imports[0].dispatcher_target = target.clone();
        loaded.cpu.gpr[3] = *object;
        loaded.cpu.gpr[4] = if *output_gpr == 4 {
            invalid_output_ptr
        } else {
            *arg4
        };
        loaded.cpu.gpr[5] = if *output_gpr == 5 {
            invalid_output_ptr
        } else {
            0
        };

        let probe = loaded.run_with_hle_imports(64);

        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(loaded.cpu.gpr[3], 0);
        for offset in 0..3 {
            assert_eq!(
                loaded.memory.read_u8(invalid_output_ptr + offset),
                Some(*sentinel)
            );
        }
        if let Some(retained_object) = retained_object {
            assert_eq!(
                ppc_q3_object_reference_count(
                    &loaded.q3_objects,
                    &loaded.q3_object_refs,
                    *retained_object
                ),
                Some(1)
            );
        }
    }

    let file = texture + PPC_Q3_OBJECT_STRIDE * 11;
    let storage = texture + PPC_Q3_OBJECT_STRIDE * 12;
    let file_output_ptr = PPC_DATA_BASE + 0x7c00;
    loaded.memory.add_region(file_output_ptr, vec![0xec; 3]);
    loaded.q3_files.push(PpcQ3FileRecord {
        file,
        storage,
        is_open: false,
        object_type: PPC_Q3_TYPE_TRIMESH,
        read_offset: 7,
        read_object: 9,
    });
    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::Q3FileOpenRead;
    loaded.cpu.gpr[3] = file;
    loaded.cpu.gpr[4] = file_output_ptr;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], 0);
    assert_eq!(loaded.memory.read_u8(file_output_ptr), Some(0xec));
    assert_eq!(loaded.memory.read_u8(file_output_ptr + 1), Some(0xec));
    assert_eq!(loaded.memory.read_u8(file_output_ptr + 2), Some(0xec));
    assert_eq!(
        loaded.q3_files[0],
        PpcQ3FileRecord {
            file,
            storage,
            is_open: false,
            object_type: PPC_Q3_TYPE_TRIMESH,
            read_offset: 7,
            read_object: 9,
        }
    );
}

#[test]
fn hle_import_runner_prevalidates_q3_storage_get_data_outputs() {
    let pef = synthetic_pef_with_library_import(b"QuickDraw\xaa 3D", b"Q3Storage_GetData");
    let mut loaded = load_pef_application(&pef).unwrap();
    let storage = PPC_Q3_OBJECT_BASE;
    let source_ptr = PPC_DATA_BASE + 0x1000;
    let dest_ptr = PPC_DATA_BASE + 0x1100;
    let size_read_ptr = PPC_DATA_BASE + 0x1200;
    let size_out_ptr = PPC_DATA_BASE + 0x1300;
    loaded.memory.add_region(source_ptr, b"abcdef".to_vec());
    loaded.memory.add_region(dest_ptr, vec![0xfa; 2]);
    loaded.memory.add_region(size_read_ptr, vec![0xfb; 4]);
    loaded.memory.add_region(size_out_ptr, vec![0xfc; 3]);
    loaded.q3_objects.push(PpcQ3ObjectRecord {
        object: storage,
        kind: PpcQ3ObjectKind::MemoryStorage,
        object_type: PPC_Q3_TYPE_NONE,
        source: PpcQ3ObjectSource::default(),
        data_ptr: source_ptr,
        data_size: 6,
    });
    loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::Q3StorageGetSize;
    loaded.cpu.gpr[3] = storage;
    loaded.cpu.gpr[4] = size_out_ptr;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], 0);
    assert_eq!(loaded.memory.read_u8(size_out_ptr), Some(0xfc));
    assert_eq!(loaded.memory.read_u8(size_out_ptr + 1), Some(0xfc));
    assert_eq!(loaded.memory.read_u8(size_out_ptr + 2), Some(0xfc));

    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::Q3StorageGetData;
    loaded.cpu.gpr[3] = storage;
    loaded.cpu.gpr[4] = 2;
    loaded.cpu.gpr[5] = 3;
    loaded.cpu.gpr[6] = dest_ptr;
    loaded.cpu.gpr[7] = size_read_ptr;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], 0);
    assert_eq!(loaded.memory.read_u8(dest_ptr), Some(0xfa));
    assert_eq!(loaded.memory.read_u8(dest_ptr + 1), Some(0xfa));
    assert_eq!(loaded.memory.read_u32_be(size_read_ptr), Some(0xfbfb_fbfb));
}

#[test]
fn hle_import_runner_handles_q3_mipmap_texture_get_mipmap() {
    let pef =
        synthetic_pef_with_library_import(b"QuickDraw\xaa 3D", b"Q3MipmapTexture_GetMipmap");
    let mut loaded = load_pef_application(&pef).unwrap();
    let texture = PPC_Q3_OBJECT_BASE;
    let storage = texture + PPC_Q3_OBJECT_STRIDE;
    let mipmap_ptr = PPC_DATA_BASE + 0x1000;
    let mipmap_out_ptr = PPC_DATA_BASE + 0x1100;
    let mut mipmap = vec![0; PPC_Q3_MIPMAP_COPY_SIZE as usize];
    for (index, byte) in mipmap.iter_mut().enumerate() {
        *byte = index as u8;
    }
    mipmap[PPC_Q3_MIPMAP_IMAGE_OFFSET as usize..][..4].copy_from_slice(&storage.to_be_bytes());
    loaded.memory.add_region(mipmap_ptr, mipmap.clone());
    loaded
        .memory
        .add_region(mipmap_out_ptr, vec![0xaa; PPC_Q3_MIPMAP_COPY_SIZE as usize]);
    loaded.q3_objects.push(PpcQ3ObjectRecord {
        object: texture,
        kind: PpcQ3ObjectKind::Generic,
        object_type: PPC_Q3_TEXTURE_TYPE_MIPMAP,
        source: PpcQ3ObjectSource::default(),
        data_ptr: 0,
        data_size: PPC_Q3_MIPMAP_COPY_SIZE,
    });
    loaded.q3_objects.push(PpcQ3ObjectRecord {
        object: storage,
        kind: PpcQ3ObjectKind::Generic,
        object_type: PPC_Q3_STORAGE_TYPE_MEMORY,
        source: PpcQ3ObjectSource::default(),
        data_ptr: 0,
        data_size: 0,
    });
    loaded
        .q3_mipmap_textures
        .push(PpcQ3MipmapTextureRecord { texture, mipmap });
    loaded.cpu.gpr[3] = texture;
    loaded.cpu.gpr[4] = mipmap_out_ptr;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], 1);
    assert_eq!(loaded.memory.read_u32_be(mipmap_out_ptr), Some(storage));
    assert_eq!(
        loaded
            .memory
            .read_u8(mipmap_out_ptr + PPC_Q3_MIPMAP_COPY_SIZE - 1),
        Some((PPC_Q3_MIPMAP_COPY_SIZE - 1) as u8)
    );
    assert_eq!(
        ppc_q3_object_reference_count(&loaded.q3_objects, &loaded.q3_object_refs, storage),
        Some(2)
    );

    let wrong_storage = storage + PPC_Q3_OBJECT_STRIDE;
    loaded
        .q3_objects
        .push(test_q3_object(wrong_storage, PPC_Q3_TYPE_TRIMESH));
    loaded.q3_mipmap_textures[0].mipmap[PPC_Q3_MIPMAP_IMAGE_OFFSET as usize..][..4]
        .copy_from_slice(&wrong_storage.to_be_bytes());
    for offset in 0..PPC_Q3_MIPMAP_COPY_SIZE {
        loaded
            .memory
            .write_u8(mipmap_out_ptr + offset, 0xee)
            .unwrap();
    }
    loaded.q3_error_state = PpcQ3ErrorState::default();
    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.cpu.gpr[3] = texture;
    loaded.cpu.gpr[4] = mipmap_out_ptr;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], 0);
    assert_eq!(loaded.memory.read_u8(mipmap_out_ptr), Some(0xee));
    assert_eq!(
        loaded
            .memory
            .read_u8(mipmap_out_ptr + PPC_Q3_MIPMAP_COPY_SIZE - 1),
        Some(0xee)
    );
    assert_eq!(
        ppc_q3_object_reference_count(&loaded.q3_objects, &loaded.q3_object_refs, storage),
        Some(2)
    );
    assert_eq!(
        ppc_q3_object_reference_count(
            &loaded.q3_objects,
            &loaded.q3_object_refs,
            wrong_storage
        ),
        Some(1)
    );
    assert_eq!(
        loaded.q3_error_state.first_error,
        PPC_Q3_ERROR_INVALID_OBJECT_PARAMETER
    );
    loaded.q3_mipmap_textures[0].mipmap[PPC_Q3_MIPMAP_IMAGE_OFFSET as usize..][..4]
        .copy_from_slice(&storage.to_be_bytes());

    loaded.memory.write_u8(mipmap_out_ptr, 0xdd).unwrap();
    loaded.q3_error_state = PpcQ3ErrorState::default();
    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.cpu.gpr[3] = storage;
    loaded.cpu.gpr[4] = mipmap_out_ptr;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], 0);
    assert_eq!(loaded.memory.read_u8(mipmap_out_ptr), Some(0xdd));
    assert_eq!(
        ppc_q3_object_reference_count(&loaded.q3_objects, &loaded.q3_object_refs, storage),
        Some(2)
    );
    assert_eq!(
        loaded.q3_error_state.first_error,
        PPC_Q3_ERROR_INVALID_OBJECT_PARAMETER
    );
}

#[test]
fn hle_import_runner_handles_q3_mipmap_texture_get_mipmap_from_object_payload() {
    let pef =
        synthetic_pef_with_library_import(b"QuickDraw\xaa 3D", b"Q3MipmapTexture_GetMipmap");
    let mut loaded = load_pef_application(&pef).unwrap();
    let texture = PPC_Q3_OBJECT_BASE;
    let storage = texture + PPC_Q3_OBJECT_STRIDE;
    let mipmap_ptr = PPC_DATA_BASE + 0x1000;
    let mipmap_out_ptr = PPC_DATA_BASE + 0x1300;
    let mut mipmap = vec![0; PPC_Q3_MIPMAP_COPY_SIZE as usize];
    for (index, byte) in mipmap.iter_mut().enumerate() {
        *byte = 0xffu8.wrapping_sub(index as u8);
    }
    mipmap[PPC_Q3_MIPMAP_IMAGE_OFFSET as usize..][..4].copy_from_slice(&storage.to_be_bytes());
    loaded.memory.add_region(mipmap_ptr, mipmap);
    loaded
        .memory
        .add_region(mipmap_out_ptr, vec![0xaa; PPC_Q3_MIPMAP_COPY_SIZE as usize]);
    loaded.q3_objects.push(PpcQ3ObjectRecord {
        object: texture,
        kind: PpcQ3ObjectKind::Generic,
        object_type: PPC_Q3_TEXTURE_TYPE_MIPMAP,
        source: PpcQ3ObjectSource::default(),
        data_ptr: mipmap_ptr,
        data_size: PPC_Q3_MIPMAP_COPY_SIZE,
    });
    loaded.q3_objects.push(PpcQ3ObjectRecord {
        object: storage,
        kind: PpcQ3ObjectKind::Generic,
        object_type: PPC_Q3_STORAGE_TYPE_MEMORY,
        source: PpcQ3ObjectSource::default(),
        data_ptr: 0,
        data_size: 0,
    });
    loaded.cpu.gpr[3] = texture;
    loaded.cpu.gpr[4] = mipmap_out_ptr;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], 1);
    assert_eq!(loaded.memory.read_u32_be(mipmap_out_ptr), Some(storage));
    assert_eq!(
        loaded
            .memory
            .read_u8(mipmap_out_ptr + PPC_Q3_MIPMAP_COPY_SIZE - 1),
        Some(0xffu8.wrapping_sub((PPC_Q3_MIPMAP_COPY_SIZE - 1) as u8))
    );
    assert_eq!(
        ppc_q3_object_reference_count(&loaded.q3_objects, &loaded.q3_object_refs, storage),
        Some(2)
    );
}

#[test]
fn hle_import_runner_handles_q3_storage_get_size() {
    let pef = synthetic_pef_with_library_import(b"QuickDraw\xaa 3D", b"Q3Storage_GetSize");
    let mut loaded = load_pef_application(&pef).unwrap();
    let storage = PPC_Q3_OBJECT_BASE;
    let size_out_ptr = PPC_DATA_BASE + 0x1000;
    loaded.memory.add_region(size_out_ptr, vec![0; 4]);
    loaded.q3_objects.push(PpcQ3ObjectRecord {
        object: storage,
        kind: PpcQ3ObjectKind::MemoryStorage,
        object_type: PPC_Q3_STORAGE_TYPE_MEMORY,
        source: PpcQ3ObjectSource::default(),
        data_ptr: PPC_DATA_BASE,
        data_size: 4096,
    });
    loaded.cpu.gpr[3] = storage;
    loaded.cpu.gpr[4] = size_out_ptr;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], 1);
    assert_eq!(loaded.memory.read_u32_be(size_out_ptr), Some(4096));

    loaded.q3_objects[0].object_type = PPC_Q3_TEXTURE_TYPE_MIPMAP;
    loaded
        .memory
        .write_u32_be(size_out_ptr, 0xdddd_dddd)
        .unwrap();
    loaded.q3_error_state = PpcQ3ErrorState::default();
    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.cpu.gpr[3] = storage;
    loaded.cpu.gpr[4] = size_out_ptr;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], 0);
    assert_eq!(loaded.memory.read_u32_be(size_out_ptr), Some(0xdddd_dddd));
    assert_eq!(
        loaded.q3_error_state.first_error,
        PPC_Q3_ERROR_INVALID_OBJECT_PARAMETER
    );
}

#[test]
fn hle_import_runner_handles_q3_storage_get_data() {
    let pef = synthetic_pef_with_library_import(b"QuickDraw\xaa 3D", b"Q3Storage_GetData");
    let mut loaded = load_pef_application(&pef).unwrap();
    let storage = PPC_Q3_OBJECT_BASE;
    let source_ptr = PPC_DATA_BASE + 0x1000;
    let dest_ptr = PPC_DATA_BASE + 0x1100;
    let size_read_ptr = PPC_DATA_BASE + 0x1200;
    loaded.memory.add_region(source_ptr, b"abcdef".to_vec());
    loaded.memory.add_region(dest_ptr, vec![0; 3]);
    loaded.memory.add_region(size_read_ptr, vec![0; 4]);
    loaded.q3_objects.push(PpcQ3ObjectRecord {
        object: storage,
        kind: PpcQ3ObjectKind::MemoryStorage,
        object_type: PPC_Q3_STORAGE_TYPE_MEMORY,
        source: PpcQ3ObjectSource::default(),
        data_ptr: source_ptr,
        data_size: 6,
    });
    loaded.cpu.gpr[3] = storage;
    loaded.cpu.gpr[4] = 2;
    loaded.cpu.gpr[5] = 3;
    loaded.cpu.gpr[6] = dest_ptr;
    loaded.cpu.gpr[7] = size_read_ptr;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], 1);
    assert_eq!(loaded.memory.read_u32_be(size_read_ptr), Some(3));
    assert_eq!(loaded.memory.read_u8(dest_ptr), Some(b'c'));
    assert_eq!(loaded.memory.read_u8(dest_ptr + 1), Some(b'd'));
    assert_eq!(loaded.memory.read_u8(dest_ptr + 2), Some(b'e'));

    loaded.q3_objects[0].object_type = PPC_Q3_TEXTURE_TYPE_MIPMAP;
    for offset in 0..3 {
        loaded.memory.write_u8(dest_ptr + offset, 0xdd).unwrap();
    }
    loaded
        .memory
        .write_u32_be(size_read_ptr, 0xeeee_eeee)
        .unwrap();
    loaded.q3_error_state = PpcQ3ErrorState::default();
    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.cpu.gpr[3] = storage;
    loaded.cpu.gpr[4] = 2;
    loaded.cpu.gpr[5] = 3;
    loaded.cpu.gpr[6] = dest_ptr;
    loaded.cpu.gpr[7] = size_read_ptr;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], 0);
    for offset in 0..3 {
        assert_eq!(loaded.memory.read_u8(dest_ptr + offset), Some(0xdd));
    }
    assert_eq!(loaded.memory.read_u32_be(size_read_ptr), Some(0xeeee_eeee));
    assert_eq!(
        loaded.q3_error_state.first_error,
        PPC_Q3_ERROR_INVALID_OBJECT_PARAMETER
    );
}

#[test]
fn hle_import_runner_handles_q3_storage_set_data() {
    let pef = synthetic_pef_with_library_import(b"QuickDraw\xaa 3D", b"Q3Storage_SetData");
    let mut loaded = load_pef_application(&pef).unwrap();
    let storage = PPC_Q3_OBJECT_BASE;
    let storage_ptr = PPC_DATA_BASE + 0x1000;
    let source_ptr = PPC_DATA_BASE + 0x1100;
    let size_written_ptr = PPC_DATA_BASE + 0x1200;
    loaded.memory.add_region(storage_ptr, vec![0; 6]);
    loaded.memory.add_region(source_ptr, b"XYZ".to_vec());
    loaded.memory.add_region(size_written_ptr, vec![0; 4]);
    loaded.q3_objects.push(PpcQ3ObjectRecord {
        object: storage,
        kind: PpcQ3ObjectKind::MemoryStorage,
        object_type: PPC_Q3_STORAGE_TYPE_MEMORY,
        source: PpcQ3ObjectSource::default(),
        data_ptr: storage_ptr,
        data_size: 6,
    });
    loaded.cpu.gpr[3] = storage;
    loaded.cpu.gpr[4] = 2;
    loaded.cpu.gpr[5] = 3;
    loaded.cpu.gpr[6] = source_ptr;
    loaded.cpu.gpr[7] = size_written_ptr;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], 1);
    assert_eq!(loaded.memory.read_u32_be(size_written_ptr), Some(3));
    assert_eq!(loaded.memory.read_u8(storage_ptr + 2), Some(b'X'));
    assert_eq!(loaded.memory.read_u8(storage_ptr + 3), Some(b'Y'));
    assert_eq!(loaded.memory.read_u8(storage_ptr + 4), Some(b'Z'));

    loaded.q3_objects[0].object_type = PPC_Q3_TEXTURE_TYPE_MIPMAP;
    loaded.memory.write_u8(storage_ptr + 2, 0xaa).unwrap();
    loaded.memory.write_u8(storage_ptr + 3, 0xbb).unwrap();
    loaded.memory.write_u8(storage_ptr + 4, 0xcc).unwrap();
    loaded
        .memory
        .write_u32_be(size_written_ptr, 0xeeee_eeee)
        .unwrap();
    loaded.q3_error_state = PpcQ3ErrorState::default();
    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.cpu.gpr[3] = storage;
    loaded.cpu.gpr[4] = 2;
    loaded.cpu.gpr[5] = 3;
    loaded.cpu.gpr[6] = source_ptr;
    loaded.cpu.gpr[7] = size_written_ptr;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], 0);
    assert_eq!(
        loaded.memory.read_u32_be(size_written_ptr),
        Some(0xeeee_eeee)
    );
    assert_eq!(loaded.memory.read_u8(storage_ptr + 2), Some(0xaa));
    assert_eq!(loaded.memory.read_u8(storage_ptr + 3), Some(0xbb));
    assert_eq!(loaded.memory.read_u8(storage_ptr + 4), Some(0xcc));
    assert_eq!(
        loaded.q3_error_state.first_error,
        PPC_Q3_ERROR_INVALID_OBJECT_PARAMETER
    );
}

#[test]
fn hle_import_runner_handles_q3_vector3d_normalize() {
    let pef = synthetic_pef_with_library_import(b"QuickDraw\xaa 3D", b"Q3Vector3D_Normalize");
    let mut loaded = load_pef_application(&pef).unwrap();
    let vector_ptr = PPC_DATA_BASE + 0x1000;
    let result_ptr = PPC_DATA_BASE + 0x1100;
    loaded.memory.add_region(vector_ptr, vec![0; 12]);
    loaded.memory.add_region(result_ptr, vec![0; 12]);
    ppc_write_q3_vector3d(&mut loaded.memory, vector_ptr, (3.0, 4.0, 0.0)).unwrap();
    loaded.cpu.gpr[3] = vector_ptr;
    loaded.cpu.gpr[4] = result_ptr;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], result_ptr);
    let (x, y, z) = ppc_read_q3_vector3d(&mut loaded.memory, result_ptr).unwrap();
    assert!((x - 0.6).abs() < 1e-6);
    assert!((y - 0.8).abs() < 1e-6);
    assert_eq!(z, 0.0);
}

#[test]
fn hle_import_runner_handles_q3_vector_and_point_math() {
    let pef = synthetic_pef_with_library_import(b"QuickDraw\xaa 3D", b"Q3Vector2D_Normalize");
    let mut loaded = load_pef_application(&pef).unwrap();
    let scratch_ptr = PPC_DATA_BASE + 0x1000;
    let vector2_ptr = scratch_ptr;
    let vector2_result_ptr = scratch_ptr + 0x20;
    let vector3_left_ptr = scratch_ptr + 0x40;
    let vector3_right_ptr = scratch_ptr + 0x60;
    let vector3_result_ptr = scratch_ptr + 0x80;
    let point2_a_ptr = scratch_ptr + 0xa0;
    let point2_b_ptr = scratch_ptr + 0xc0;
    let point3_a_ptr = scratch_ptr + 0xe0;
    let point3_b_ptr = scratch_ptr + 0x100;
    let tri_a_ptr = scratch_ptr + 0x120;
    let tri_b_ptr = scratch_ptr + 0x140;
    let tri_c_ptr = scratch_ptr + 0x160;
    let tri_result_ptr = scratch_ptr + 0x180;
    loaded.memory.add_region(scratch_ptr, vec![0; 0x200]);
    ppc_write_q3_vector2d(&mut loaded.memory, vector2_ptr, (6.0, 8.0)).unwrap();
    loaded.cpu.gpr[3] = vector2_ptr;
    loaded.cpu.gpr[4] = vector2_result_ptr;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], vector2_result_ptr);
    let (x, y) = ppc_read_q3_vector2d(&mut loaded.memory, vector2_result_ptr).unwrap();
    assert!((x - 0.6).abs() < 1e-6);
    assert!((y - 0.8).abs() < 1e-6);

    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::Q3Vector3DCross;
    ppc_write_q3_vector3d(&mut loaded.memory, vector3_left_ptr, (1.0, 0.0, 0.0)).unwrap();
    ppc_write_q3_vector3d(&mut loaded.memory, vector3_right_ptr, (0.0, 1.0, 0.0)).unwrap();
    loaded.cpu.gpr[3] = vector3_left_ptr;
    loaded.cpu.gpr[4] = vector3_right_ptr;
    loaded.cpu.gpr[5] = vector3_result_ptr;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], vector3_result_ptr);
    assert_eq!(
        ppc_read_q3_vector3d(&mut loaded.memory, vector3_result_ptr).unwrap(),
        (0.0, 0.0, 1.0)
    );

    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::Q3Point2DDistance;
    ppc_write_q3_vector2d(&mut loaded.memory, point2_a_ptr, (1.0, 2.0)).unwrap();
    ppc_write_q3_vector2d(&mut loaded.memory, point2_b_ptr, (4.0, 6.0)).unwrap();
    loaded.cpu.gpr[3] = point2_a_ptr;
    loaded.cpu.gpr[4] = point2_b_ptr;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert!((f64::from_bits(loaded.cpu.fpr[1]) - 5.0).abs() < 1e-6);

    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::Q3Point3DDistance;
    ppc_write_q3_vector3d(&mut loaded.memory, point3_a_ptr, (1.0, 2.0, 3.0)).unwrap();
    ppc_write_q3_vector3d(&mut loaded.memory, point3_b_ptr, (4.0, 6.0, 15.0)).unwrap();
    loaded.cpu.gpr[3] = point3_a_ptr;
    loaded.cpu.gpr[4] = point3_b_ptr;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert!((f64::from_bits(loaded.cpu.fpr[1]) - 13.0).abs() < 1e-6);

    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::Q3Point3DCrossProductTri;
    ppc_write_q3_vector3d(&mut loaded.memory, tri_a_ptr, (0.0, 0.0, 0.0)).unwrap();
    ppc_write_q3_vector3d(&mut loaded.memory, tri_b_ptr, (1.0, 0.0, 0.0)).unwrap();
    ppc_write_q3_vector3d(&mut loaded.memory, tri_c_ptr, (0.0, 2.0, 0.0)).unwrap();
    loaded.cpu.gpr[3] = tri_a_ptr;
    loaded.cpu.gpr[4] = tri_b_ptr;
    loaded.cpu.gpr[5] = tri_c_ptr;
    loaded.cpu.gpr[6] = tri_result_ptr;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], tri_result_ptr);
    assert_eq!(
        ppc_read_q3_vector3d(&mut loaded.memory, tri_result_ptr).unwrap(),
        (0.0, 0.0, 2.0)
    );
}

#[test]
fn hle_import_runner_prevalidates_q3_vector_math_outputs() {
    fn run_vector_math_call(
        loaded: &mut PpcLoadedApp,
        target: PpcImportDispatcherTarget,
        args: &[u32],
    ) -> PpcHleRunProbe {
        loaded.cpu.pc = loaded.entry_pc;
        loaded.cpu.lr = PPC_HALT_PC;
        loaded.imports[0].dispatcher_target = target;
        for (index, arg) in args.iter().copied().enumerate() {
            loaded.cpu.gpr[3 + index] = arg;
        }
        loaded.run_with_hle_imports(64)
    }

    let pef = synthetic_pef_with_library_import(b"QuickDraw\xaa 3D", b"Q3Vector2D_Normalize");
    let mut loaded = load_pef_application(&pef).unwrap();
    let scratch_ptr = PPC_DATA_BASE + 0x1000;
    let vector2_ptr = scratch_ptr;
    let vector3_left_ptr = scratch_ptr + 0x20;
    let vector3_right_ptr = scratch_ptr + 0x40;
    let vector3_third_ptr = scratch_ptr + 0x60;
    let short_vector2_out = PPC_DATA_BASE + 0x4000;
    let short_vector3_out = PPC_DATA_BASE + 0x4100;
    loaded.memory.add_region(scratch_ptr, vec![0; 0x100]);
    loaded.memory.add_region(
        short_vector2_out,
        vec![0xa2; (PPC_Q3_VECTOR2D_SIZE - 1) as usize],
    );
    loaded.memory.add_region(
        short_vector3_out,
        vec![0xa3; (PPC_Q3_VECTOR3D_SIZE - 1) as usize],
    );
    ppc_write_q3_vector2d(&mut loaded.memory, vector2_ptr, (6.0, 8.0)).unwrap();
    ppc_write_q3_vector3d(&mut loaded.memory, vector3_left_ptr, (1.0, 0.0, 0.0)).unwrap();
    ppc_write_q3_vector3d(&mut loaded.memory, vector3_right_ptr, (0.0, 1.0, 0.0)).unwrap();
    ppc_write_q3_vector3d(&mut loaded.memory, vector3_third_ptr, (0.0, 2.0, 0.0)).unwrap();

    let probe = run_vector_math_call(
        &mut loaded,
        PpcImportDispatcherTarget::Q3Vector2DNormalize,
        &[vector2_ptr, short_vector2_out],
    );

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], 0);
    for offset in 0..(PPC_Q3_VECTOR2D_SIZE - 1) {
        assert_eq!(
            loaded.memory.read_u8(short_vector2_out + offset),
            Some(0xa2)
        );
    }

    let vector3_output_targets = [
        (
            PpcImportDispatcherTarget::Q3Vector3DNormalize,
            vec![vector3_left_ptr, short_vector3_out],
        ),
        (
            PpcImportDispatcherTarget::Q3Vector3DCross,
            vec![vector3_left_ptr, vector3_right_ptr, short_vector3_out],
        ),
        (
            PpcImportDispatcherTarget::Q3Point3DCrossProductTri,
            vec![
                vector3_left_ptr,
                vector3_right_ptr,
                vector3_third_ptr,
                short_vector3_out,
            ],
        ),
    ];
    for (target, args) in vector3_output_targets {
        for offset in 0..(PPC_Q3_VECTOR3D_SIZE - 1) {
            loaded
                .memory
                .write_u8(short_vector3_out + offset, 0xa3)
                .unwrap();
        }

        let probe = run_vector_math_call(&mut loaded, target, &args);

        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(loaded.cpu.gpr[3], 0);
        for offset in 0..(PPC_Q3_VECTOR3D_SIZE - 1) {
            assert_eq!(
                loaded.memory.read_u8(short_vector3_out + offset),
                Some(0xa3)
            );
        }
    }
}

#[test]
fn hle_import_runner_handles_q3_matrix_and_transform_math() {
    let pef =
        synthetic_pef_with_library_import(b"QuickDraw\xaa 3D", b"Q3Matrix4x4_SetTranslate");
    let mut loaded = load_pef_application(&pef).unwrap();
    let scratch_ptr = PPC_DATA_BASE + 0x1000;
    let translate_matrix_ptr = scratch_ptr;
    let scale_matrix_ptr = scratch_ptr + 0x80;
    let result_matrix_ptr = scratch_ptr + 0x100;
    let inverse_matrix_ptr = scratch_ptr + 0x180;
    let point_ptr = scratch_ptr + 0x200;
    let vector_ptr = scratch_ptr + 0x220;
    let result_ptr = scratch_ptr + 0x240;
    let transform = PPC_Q3_OBJECT_BASE;
    loaded.memory.add_region(scratch_ptr, vec![0; 0x300]);
    loaded.cpu.gpr[3] = translate_matrix_ptr;
    loaded.cpu.fpr[1] = 2.0f64.to_bits();
    loaded.cpu.fpr[2] = 3.0f64.to_bits();
    loaded.cpu.fpr[3] = 4.0f64.to_bits();

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], translate_matrix_ptr);
    let matrix = ppc_read_q3_matrix4x4(&mut loaded.memory, translate_matrix_ptr).unwrap();
    assert_eq!(matrix[0][0], 1.0);
    assert_eq!(matrix[3][0], 2.0);
    assert_eq!(matrix[3][1], 3.0);
    assert_eq!(matrix[3][2], 4.0);

    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::Q3Matrix4x4SetIdentity;
    loaded.cpu.gpr[3] = result_matrix_ptr;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], result_matrix_ptr);
    let identity = ppc_read_q3_matrix4x4(&mut loaded.memory, result_matrix_ptr).unwrap();
    assert_eq!(identity[0][0], 1.0);
    assert_eq!(identity[1][1], 1.0);
    assert_eq!(identity[2][2], 1.0);
    assert_eq!(identity[3][3], 1.0);

    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::Q3Matrix3x3SetTranslate;
    loaded.cpu.gpr[3] = result_matrix_ptr;
    loaded.cpu.fpr[1] = 5.0f64.to_bits();
    loaded.cpu.fpr[2] = 6.0f64.to_bits();

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], result_matrix_ptr);
    assert_eq!(
        ppc_read_f32_be(&mut loaded.memory, result_matrix_ptr + 24),
        Some(5.0)
    );
    assert_eq!(
        ppc_read_f32_be(&mut loaded.memory, result_matrix_ptr + 28),
        Some(6.0)
    );
    assert_eq!(
        ppc_read_f32_be(&mut loaded.memory, result_matrix_ptr + 32),
        Some(1.0)
    );

    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::Q3Matrix4x4Transpose;
    loaded.cpu.gpr[3] = translate_matrix_ptr;
    loaded.cpu.gpr[4] = result_matrix_ptr;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], result_matrix_ptr);
    let transposed = ppc_read_q3_matrix4x4(&mut loaded.memory, result_matrix_ptr).unwrap();
    assert_eq!(transposed[0][3], 2.0);
    assert_eq!(transposed[1][3], 3.0);
    assert_eq!(transposed[2][3], 4.0);

    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::Q3Matrix4x4SetRotateZ;
    loaded.cpu.gpr[3] = result_matrix_ptr;
    loaded.cpu.fpr[1] = std::f64::consts::FRAC_PI_2.to_bits();

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], result_matrix_ptr);

    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::Q3Point3DTransform;
    ppc_write_q3_vector3d(&mut loaded.memory, point_ptr, (1.0, 0.0, 0.0)).unwrap();
    loaded.cpu.gpr[3] = point_ptr;
    loaded.cpu.gpr[4] = result_matrix_ptr;
    loaded.cpu.gpr[5] = result_ptr;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    let (x, y, z) = ppc_read_q3_vector3d(&mut loaded.memory, result_ptr).unwrap();
    assert!(x.abs() < 1e-6);
    assert!((y - 1.0).abs() < 1e-6);
    assert_eq!(z, 0.0);

    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::Q3Point3DTransform;
    ppc_write_q3_vector3d(&mut loaded.memory, point_ptr, (1.0, 1.0, 1.0)).unwrap();
    loaded.cpu.gpr[3] = point_ptr;
    loaded.cpu.gpr[4] = translate_matrix_ptr;
    loaded.cpu.gpr[5] = result_ptr;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], result_ptr);
    assert_eq!(
        ppc_read_q3_vector3d(&mut loaded.memory, result_ptr).unwrap(),
        (3.0, 4.0, 5.0)
    );

    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::Q3Vector3DTransform;
    ppc_write_q3_vector3d(&mut loaded.memory, vector_ptr, (1.0, 1.0, 1.0)).unwrap();
    loaded.cpu.gpr[3] = vector_ptr;
    loaded.cpu.gpr[4] = translate_matrix_ptr;
    loaded.cpu.gpr[5] = result_ptr;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], result_ptr);
    assert_eq!(
        ppc_read_q3_vector3d(&mut loaded.memory, result_ptr).unwrap(),
        (1.0, 1.0, 1.0)
    );

    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::Q3Matrix4x4SetScale;
    loaded.cpu.gpr[3] = scale_matrix_ptr;
    loaded.cpu.fpr[1] = 2.0f64.to_bits();
    loaded.cpu.fpr[2] = 3.0f64.to_bits();
    loaded.cpu.fpr[3] = 4.0f64.to_bits();

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], scale_matrix_ptr);

    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::Q3Matrix4x4Multiply;
    loaded.cpu.gpr[3] = scale_matrix_ptr;
    loaded.cpu.gpr[4] = translate_matrix_ptr;
    loaded.cpu.gpr[5] = result_matrix_ptr;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], result_matrix_ptr);

    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::Q3Point3DTransform;
    loaded.cpu.gpr[3] = point_ptr;
    loaded.cpu.gpr[4] = result_matrix_ptr;
    loaded.cpu.gpr[5] = result_ptr;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(
        ppc_read_q3_vector3d(&mut loaded.memory, result_ptr).unwrap(),
        (4.0, 6.0, 8.0)
    );

    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::Q3Matrix4x4Invert;
    loaded.cpu.gpr[3] = translate_matrix_ptr;
    loaded.cpu.gpr[4] = inverse_matrix_ptr;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], inverse_matrix_ptr);
    let inverse = ppc_read_q3_matrix4x4(&mut loaded.memory, inverse_matrix_ptr).unwrap();
    assert_eq!(inverse[3][0], -2.0);
    assert_eq!(inverse[3][1], -3.0);
    assert_eq!(inverse[3][2], -4.0);

    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::Q3MatrixTransformNew;
    loaded.cpu.gpr[3] = translate_matrix_ptr;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], transform);
    assert_eq!(
        loaded.q3_objects,
        vec![PpcQ3ObjectRecord {
            object: transform,
            kind: PpcQ3ObjectKind::Generic,
            object_type: PPC_Q3_TRANSFORM_TYPE_MATRIX,
            source: PpcQ3ObjectSource::default(),
            data_ptr: translate_matrix_ptr,
            data_size: 64,
        }]
    );

    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::Q3MatrixTransformSet;
    loaded.cpu.gpr[3] = transform;
    loaded.cpu.gpr[4] = scale_matrix_ptr;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], 1);
    assert_eq!(loaded.q3_objects[0].data_ptr, scale_matrix_ptr);

    let non_transform = transform + PPC_Q3_OBJECT_STRIDE;
    loaded.q3_objects.push(PpcQ3ObjectRecord {
        object: non_transform,
        kind: PpcQ3ObjectKind::Generic,
        object_type: PPC_Q3_TYPE_TRIMESH,
        source: PpcQ3ObjectSource::default(),
        data_ptr: translate_matrix_ptr,
        data_size: 64,
    });
    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.cpu.gpr[3] = non_transform;
    loaded.cpu.gpr[4] = translate_matrix_ptr;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], 0);
    assert_eq!(loaded.q3_objects[1].data_ptr, translate_matrix_ptr);
    assert_eq!(
        loaded.q3_error_state.first_error,
        PPC_Q3_ERROR_INVALID_OBJECT_PARAMETER
    );
    assert_eq!(
        loaded.q3_error_state.last_error,
        PPC_Q3_ERROR_INVALID_OBJECT_PARAMETER
    );

    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::Q3TransformGetMatrix;
    loaded.cpu.gpr[3] = transform;
    loaded.cpu.gpr[4] = result_matrix_ptr;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], 1);
    let copied = ppc_read_q3_matrix4x4(&mut loaded.memory, result_matrix_ptr).unwrap();
    assert_eq!(copied[0][0], 2.0);
    assert_eq!(copied[1][1], 3.0);
    assert_eq!(copied[2][2], 4.0);

    loaded.q3_error_state = PpcQ3ErrorState::default();
    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.cpu.gpr[3] = non_transform;
    loaded.cpu.gpr[4] = result_matrix_ptr;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], 0);
    let preserved = ppc_read_q3_matrix4x4(&mut loaded.memory, result_matrix_ptr).unwrap();
    assert_eq!(preserved[0][0], 2.0);
    assert_eq!(preserved[1][1], 3.0);
    assert_eq!(preserved[2][2], 4.0);
    assert_eq!(
        loaded.q3_error_state.first_error,
        PPC_Q3_ERROR_INVALID_OBJECT_PARAMETER
    );
    assert_eq!(
        loaded.q3_error_state.last_error,
        PPC_Q3_ERROR_INVALID_OBJECT_PARAMETER
    );
}

#[test]
fn hle_import_runner_handles_q3_point_transform_arrays() {
    let pef =
        synthetic_pef_with_library_import(b"QuickDraw\xaa 3D", b"Q3Point3D_To3DTransformArray");
    let mut loaded = load_pef_application(&pef).unwrap();
    let scratch_ptr = PPC_DATA_BASE + 0x1000;
    let matrix_ptr = scratch_ptr;
    let input_ptr = scratch_ptr + 0x80;
    let output3d_ptr = scratch_ptr + 0x180;
    let output4d_ptr = scratch_ptr + 0x280;
    let input_stride = 20;
    let output3d_stride = 16;
    let output4d_stride = 24;
    loaded.memory.add_region(scratch_ptr, vec![0xcc; 0x400]);

    let mut matrix = ppc_q3_matrix4x4_identity();
    matrix[3][0] = 2.0;
    matrix[3][1] = 3.0;
    matrix[3][2] = 4.0;
    ppc_write_q3_matrix4x4(&mut loaded.memory, matrix_ptr, &matrix).unwrap();
    let input_points = [(1.0, 1.0, 1.0), (-1.0, 2.0, 3.0), (0.0, 0.0, 0.0)];
    for (index, point) in input_points.iter().copied().enumerate() {
        ppc_write_q3_vector3d(
            &mut loaded.memory,
            input_ptr + (index as u32 * input_stride),
            point,
        )
        .unwrap();
    }
    loaded.cpu.gpr[3] = input_ptr;
    loaded.cpu.gpr[4] = matrix_ptr;
    loaded.cpu.gpr[5] = output3d_ptr;
    loaded.cpu.gpr[6] = input_points.len() as u32;
    loaded.cpu.gpr[7] = input_stride;
    loaded.cpu.gpr[8] = output3d_stride;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], 1);
    let expected3d = [(3.0, 4.0, 5.0), (1.0, 5.0, 7.0), (2.0, 3.0, 4.0)];
    for (index, expected) in expected3d.iter().copied().enumerate() {
        let point_ptr = output3d_ptr + (index as u32 * output3d_stride);
        assert_eq!(
            ppc_read_q3_vector3d(&mut loaded.memory, point_ptr).unwrap(),
            expected
        );
        assert_eq!(loaded.memory.read_u8(point_ptr + 12), Some(0xcc));
    }

    matrix[0][3] = 0.5;
    ppc_write_q3_matrix4x4(&mut loaded.memory, matrix_ptr, &matrix).unwrap();
    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.imports[0].dispatcher_target =
        PpcImportDispatcherTarget::Q3Point3DTo4DTransformArray;
    loaded.cpu.gpr[3] = input_ptr;
    loaded.cpu.gpr[4] = matrix_ptr;
    loaded.cpu.gpr[5] = output4d_ptr;
    loaded.cpu.gpr[6] = 2;
    loaded.cpu.gpr[7] = input_stride;
    loaded.cpu.gpr[8] = output4d_stride;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], 1);
    let expected4d = [(3.0, 4.0, 5.0, 1.5), (1.0, 5.0, 7.0, 0.5)];
    for (index, expected) in expected4d.iter().copied().enumerate() {
        let point_ptr = output4d_ptr + (index as u32 * output4d_stride);
        let actual = (
            ppc_read_f32_be(&mut loaded.memory, point_ptr).unwrap(),
            ppc_read_f32_be(&mut loaded.memory, point_ptr + 4).unwrap(),
            ppc_read_f32_be(&mut loaded.memory, point_ptr + 8).unwrap(),
            ppc_read_f32_be(&mut loaded.memory, point_ptr + 12).unwrap(),
        );
        assert_eq!(actual.0, expected.0);
        assert_eq!(actual.1, expected.1);
        assert_eq!(actual.2, expected.2);
        assert!((actual.3 - expected.3).abs() < 1e-6);
        assert_eq!(loaded.memory.read_u8(point_ptr + 16), Some(0xcc));
    }

    ppc_write_q3_matrix4x4(&mut loaded.memory, matrix_ptr, &ppc_q3_matrix4x4_identity())
        .unwrap();
    for (index, point) in [(1.0, 2.0, 3.0), (4.0, 4.0, 3.0)].into_iter().enumerate() {
        ppc_write_q3_vector3d(
            &mut loaded.memory,
            input_ptr + index as u32 * PPC_Q3_POINT3D_SIZE,
            point,
        )
        .unwrap();
    }
    assert!(ppc_q3_point3d_transform_array(
        &mut loaded.memory,
        input_ptr,
        matrix_ptr,
        output4d_ptr,
        2,
        PPC_Q3_POINT3D_SIZE,
        16,
        true,
    ));
    assert_eq!(
        ppc_read_f32_be(&mut loaded.memory, output4d_ptr + 12),
        Some(1.0)
    );
}

#[test]
fn hle_import_runner_tracks_q3_file_storage_and_open_state() {
    let pef = synthetic_pef_with_library_import(b"QuickDraw\xaa 3D", b"Q3File_SetStorage");
    let mut loaded = load_pef_application(&pef).unwrap();
    let file = PPC_Q3_OBJECT_BASE;
    let storage = PPC_Q3_OBJECT_BASE + PPC_Q3_OBJECT_STRIDE;
    loaded.q3_objects.extend([
        test_q3_object(file, PPC_Q3_TYPE_FILE),
        test_q3_object(storage, PPC_Q3_STORAGE_TYPE_MEMORY),
    ]);
    loaded.cpu.gpr[3] = file;
    loaded.cpu.gpr[4] = storage;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], 1);
    assert_eq!(loaded.q3_files.len(), 1);
    assert_eq!(
        loaded.q3_files[0],
        PpcQ3FileRecord {
            file,
            storage,
            is_open: false,
            object_type: 0,
            read_offset: 0,
            read_object: 0,
        }
    );

    let pef = synthetic_pef_with_library_import(b"QuickDraw\xaa 3D", b"Q3File_OpenRead");
    let mut loaded = load_pef_application(&pef).unwrap();
    let object_type_out_ptr = PPC_DATA_BASE + 0x1000;
    loaded.memory.add_region(object_type_out_ptr, vec![0xaa; 4]);
    loaded
        .q3_objects
        .push(test_q3_object(file, PPC_Q3_TYPE_FILE));
    loaded.q3_files.push(PpcQ3FileRecord {
        file,
        storage,
        is_open: false,
        object_type: 0x1234_5678,
        read_offset: 99,
        read_object: 0xdead_beef,
    });
    loaded.cpu.gpr[3] = file;
    loaded.cpu.gpr[4] = object_type_out_ptr;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], 1);
    assert!(loaded.q3_files[0].is_open);
    assert_eq!(loaded.q3_files[0].read_offset, 0);
    assert_eq!(loaded.q3_files[0].read_object, 0);
    assert_eq!(loaded.memory.read_u32_be(object_type_out_ptr), Some(0));

    let pef = synthetic_pef_with_library_import(b"QuickDraw\xaa 3D", b"Q3File_Close");
    let mut loaded = load_pef_application(&pef).unwrap();
    loaded
        .q3_objects
        .push(test_q3_object(file, PPC_Q3_TYPE_FILE));
    loaded.q3_files.push(PpcQ3FileRecord {
        file,
        storage,
        is_open: true,
        object_type: 0,
        read_offset: 6,
        read_object: PPC_Q3_OBJECT_BASE + PPC_Q3_OBJECT_STRIDE,
    });
    loaded.cpu.gpr[3] = file;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], 1);
    assert!(!loaded.q3_files[0].is_open);
    assert_eq!(loaded.q3_files[0].read_offset, 0);
    assert_eq!(loaded.q3_files[0].read_object, 0);
}

#[test]
fn hle_import_runner_validates_q3_file_accessors() {
    fn run_q3_call(loaded: &mut PpcLoadedApp, target: PpcImportDispatcherTarget) -> u32 {
        loaded.cpu.pc = loaded.entry_pc;
        loaded.cpu.lr = PPC_HALT_PC;
        loaded.imports[0].dispatcher_target = target;
        let probe = loaded.run_with_hle_imports(64);
        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        loaded.cpu.gpr[3]
    }

    let pef = synthetic_pef_with_library_import(b"QuickDraw\xaa 3D", b"Q3File_SetStorage");
    let mut loaded = load_pef_application(&pef).unwrap();
    let file = PPC_Q3_OBJECT_BASE;
    let storage = PPC_Q3_OBJECT_BASE + PPC_Q3_OBJECT_STRIDE;
    let retained_storage = PPC_Q3_OBJECT_BASE + PPC_Q3_OBJECT_STRIDE * 2;
    let output_ptr = PPC_DATA_BASE + 0x1000;
    let original_record = PpcQ3FileRecord {
        file,
        storage: retained_storage,
        is_open: true,
        object_type: 0x1234_5678,
        read_offset: 7,
        read_object: 9,
    };
    loaded.memory.add_region(output_ptr, vec![0xcc; 4]);
    loaded.q3_objects.extend([
        test_q3_object(file, PPC_Q3_TEXTURE_TYPE_MIPMAP),
        test_q3_object(storage, PPC_Q3_STORAGE_TYPE_MEMORY),
        test_q3_object(retained_storage, PPC_Q3_STORAGE_TYPE_MEMORY),
    ]);
    loaded.q3_files.push(original_record);
    loaded.cpu.gpr[3] = file;
    loaded.cpu.gpr[4] = storage;

    assert_eq!(
        run_q3_call(&mut loaded, PpcImportDispatcherTarget::Q3FileSetStorage),
        0
    );
    assert_eq!(loaded.q3_files[0], original_record);
    assert_eq!(
        loaded.q3_error_state.first_error,
        PPC_Q3_ERROR_INVALID_OBJECT_PARAMETER
    );
    assert_eq!(
        ppc_q3_object_reference_count(&loaded.q3_objects, &loaded.q3_object_refs, storage),
        Some(1)
    );

    loaded.q3_error_state = PpcQ3ErrorState::default();
    loaded.q3_objects[0].object_type = PPC_Q3_TYPE_FILE;
    loaded.q3_objects[1].object_type = PPC_Q3_TEXTURE_TYPE_MIPMAP;
    loaded.cpu.gpr[3] = file;
    loaded.cpu.gpr[4] = storage;

    assert_eq!(
        run_q3_call(&mut loaded, PpcImportDispatcherTarget::Q3FileSetStorage),
        0
    );
    assert_eq!(loaded.q3_files[0], original_record);
    assert_eq!(
        loaded.q3_error_state.first_error,
        PPC_Q3_ERROR_INVALID_OBJECT_PARAMETER
    );

    loaded.q3_error_state = PpcQ3ErrorState::default();
    loaded.q3_objects[0].object_type = PPC_Q3_TEXTURE_TYPE_MIPMAP;
    loaded.q3_objects[1].object_type = PPC_Q3_STORAGE_TYPE_MEMORY;
    loaded.memory.write_u32_be(output_ptr, 0xcccc_cccc).unwrap();
    loaded.cpu.gpr[3] = file;
    loaded.cpu.gpr[4] = output_ptr;

    assert_eq!(
        run_q3_call(&mut loaded, PpcImportDispatcherTarget::Q3FileOpenRead),
        0
    );
    assert_eq!(loaded.memory.read_u32_be(output_ptr), Some(0xcccc_cccc));
    assert_eq!(loaded.q3_files[0], original_record);
    assert_eq!(
        loaded.q3_error_state.first_error,
        PPC_Q3_ERROR_INVALID_OBJECT_PARAMETER
    );

    loaded.q3_error_state = PpcQ3ErrorState::default();
    loaded.q3_objects[0].object_type = PPC_Q3_TYPE_FILE;
    loaded.q3_objects[1].object_type = PPC_Q3_STORAGE_TYPE_MEMORY;
    loaded.q3_objects[2].object_type = PPC_Q3_TEXTURE_TYPE_MIPMAP;
    let object_count = loaded.q3_objects.len();
    loaded.cpu.gpr[3] = file;

    assert_eq!(
        run_q3_call(&mut loaded, PpcImportDispatcherTarget::Q3FileReadObject),
        0
    );
    assert_eq!(loaded.q3_objects.len(), object_count);
    assert_eq!(loaded.q3_files[0], original_record);
    assert_eq!(
        loaded.q3_error_state.first_error,
        PPC_Q3_ERROR_INVALID_OBJECT_PARAMETER
    );

    loaded.q3_error_state = PpcQ3ErrorState::default();
    loaded.cpu.gpr[3] = file;

    assert_eq!(
        run_q3_call(&mut loaded, PpcImportDispatcherTarget::Q3FileIsEndOfFile),
        1
    );
    assert_eq!(loaded.q3_files[0], original_record);
    assert_eq!(
        loaded.q3_error_state.first_error,
        PPC_Q3_ERROR_INVALID_OBJECT_PARAMETER
    );

    loaded.q3_error_state = PpcQ3ErrorState::default();
    loaded.q3_objects[0].object_type = PPC_Q3_TEXTURE_TYPE_MIPMAP;
    loaded.q3_objects[1].object_type = PPC_Q3_STORAGE_TYPE_MEMORY;
    loaded.q3_objects[2].object_type = PPC_Q3_STORAGE_TYPE_MEMORY;
    loaded.cpu.gpr[3] = file;

    assert_eq!(
        run_q3_call(&mut loaded, PpcImportDispatcherTarget::Q3FileClose),
        0
    );
    assert_eq!(loaded.q3_files[0], original_record);
    assert_eq!(
        loaded.q3_error_state.first_error,
        PPC_Q3_ERROR_INVALID_OBJECT_PARAMETER
    );
}

#[test]
fn hle_import_runner_reads_q3_file_objects_and_records_hierarchy() {
    let file = PPC_Q3_OBJECT_BASE;
    let storage = PPC_Q3_OBJECT_BASE + PPC_Q3_OBJECT_STRIDE;
    let storage_ptr = PPC_DATA_BASE + 0x1000;
    let pef = synthetic_pef_with_library_import(b"QuickDraw\xaa 3D", b"Q3File_IsEndOfFile");
    let mut loaded = load_pef_application(&pef).unwrap();
    let mut storage_bytes = Vec::new();
    storage_bytes.extend_from_slice(b"3DMF");
    storage_bytes.extend_from_slice(&16u32.to_be_bytes());
    storage_bytes.extend_from_slice(&[0; 16]);
    storage_bytes.extend_from_slice(b"bgng");
    storage_bytes.extend_from_slice(&8u32.to_be_bytes());
    storage_bytes.extend_from_slice(b"dspg");
    storage_bytes.extend_from_slice(&0u32.to_be_bytes());
    storage_bytes.extend_from_slice(b"cntr");
    storage_bytes.extend_from_slice(&0u32.to_be_bytes());
    storage_bytes.extend_from_slice(b"bgng");
    storage_bytes.extend_from_slice(&8u32.to_be_bytes());
    storage_bytes.extend_from_slice(b"ogtg");
    storage_bytes.extend_from_slice(&0u32.to_be_bytes());
    storage_bytes.extend_from_slice(b"tmsh");
    storage_bytes.extend_from_slice(&4u32.to_be_bytes());
    storage_bytes.extend_from_slice(&[0xde, 0xad, 0xbe, 0xef]);
    storage_bytes.extend_from_slice(b"endg");
    storage_bytes.extend_from_slice(&0u32.to_be_bytes());
    storage_bytes.extend_from_slice(b"endg");
    storage_bytes.extend_from_slice(&0u32.to_be_bytes());
    loaded.memory.add_region(storage_ptr, storage_bytes.clone());
    loaded.q3_objects.push(PpcQ3ObjectRecord {
        object: storage,
        kind: PpcQ3ObjectKind::MemoryStorage,
        object_type: PPC_Q3_STORAGE_TYPE_MEMORY,
        source: PpcQ3ObjectSource::default(),
        data_ptr: storage_ptr,
        data_size: storage_bytes.len() as u32,
    });
    loaded
        .q3_objects
        .push(test_q3_object(file, PPC_Q3_TYPE_FILE));
    loaded.next_q3_object = PPC_Q3_OBJECT_BASE + PPC_Q3_OBJECT_STRIDE * 2;
    loaded.q3_files.push(PpcQ3FileRecord {
        file,
        storage,
        is_open: true,
        object_type: 0,
        read_offset: 0,
        read_object: 0,
    });
    loaded.cpu.gpr[3] = file;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], 0);

    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::Q3FileReadObject;
    loaded.cpu.gpr[3] = file;

    let probe = loaded.run_with_hle_imports(64);

    let read_object = PPC_Q3_OBJECT_BASE + PPC_Q3_OBJECT_STRIDE * 2;
    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], read_object);
    assert_eq!(loaded.q3_files[0].read_offset, 48);
    assert_eq!(loaded.q3_files[0].read_object, read_object);
    let display_group = PPC_Q3_OBJECT_BASE + PPC_Q3_OBJECT_STRIDE * 3;
    assert_eq!(
        loaded.q3_objects[2],
        PpcQ3ObjectRecord {
            object: read_object,
            kind: PpcQ3ObjectKind::Generic,
            object_type: PPC_Q3_TYPE_CONTAINER,
            source: PpcQ3ObjectSource {
                file,
                offset: 40,
                parent_group_type: u32::from_be_bytes(*b"dspg"),
                group_depth: 1,
            },
            data_ptr: storage_ptr + 40,
            data_size: 8,
        }
    );
    assert_eq!(
        loaded.q3_objects[3],
        PpcQ3ObjectRecord {
            object: display_group,
            kind: PpcQ3ObjectKind::Generic,
            object_type: u32::from_be_bytes(*b"dspg"),
            source: PpcQ3ObjectSource {
                file,
                offset: 24,
                parent_group_type: PPC_Q3_TYPE_NONE,
                group_depth: 1,
            },
            data_ptr: storage_ptr + 24,
            data_size: 16,
        }
    );
    assert_eq!(
        loaded.q3_file_groups,
        vec![PpcQ3FileGroupRecord {
            file,
            offset: 24,
            group: display_group,
            group_type: u32::from_be_bytes(*b"dspg"),
            parent_group: 0,
            group_depth: 1,
        }]
    );
    assert_eq!(
        loaded.q3_group_memberships,
        vec![PpcQ3GroupMembershipRecord {
            group: display_group,
            object: read_object,
            before: None,
        }]
    );

    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::Q3FileIsEndOfFile;
    loaded.cpu.gpr[3] = file;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], 0);

    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::Q3FileReadObject;
    loaded.cpu.gpr[3] = file;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    let read_object = PPC_Q3_OBJECT_BASE + PPC_Q3_OBJECT_STRIDE * 4;
    let object_group = PPC_Q3_OBJECT_BASE + PPC_Q3_OBJECT_STRIDE * 5;
    assert_eq!(loaded.cpu.gpr[3], read_object);
    assert_eq!(loaded.q3_files[0].read_offset, 76);
    assert_eq!(loaded.q3_files[0].read_object, read_object);
    assert_eq!(
        loaded.q3_objects[4],
        PpcQ3ObjectRecord {
            object: read_object,
            kind: PpcQ3ObjectKind::Generic,
            object_type: PPC_Q3_TYPE_TRIMESH,
            source: PpcQ3ObjectSource {
                file,
                offset: 64,
                parent_group_type: u32::from_be_bytes(*b"ogtg"),
                group_depth: 2,
            },
            data_ptr: storage_ptr + 64,
            data_size: 12,
        }
    );
    assert_eq!(
        loaded.q3_objects[5],
        PpcQ3ObjectRecord {
            object: object_group,
            kind: PpcQ3ObjectKind::Generic,
            object_type: u32::from_be_bytes(*b"ogtg"),
            source: PpcQ3ObjectSource {
                file,
                offset: 48,
                parent_group_type: u32::from_be_bytes(*b"dspg"),
                group_depth: 2,
            },
            data_ptr: storage_ptr + 48,
            data_size: 16,
        }
    );
    assert_eq!(
        loaded.q3_file_groups,
        vec![
            PpcQ3FileGroupRecord {
                file,
                offset: 24,
                group: display_group,
                group_type: u32::from_be_bytes(*b"dspg"),
                parent_group: 0,
                group_depth: 1,
            },
            PpcQ3FileGroupRecord {
                file,
                offset: 48,
                group: object_group,
                group_type: u32::from_be_bytes(*b"ogtg"),
                parent_group: display_group,
                group_depth: 2,
            },
        ]
    );
    assert_eq!(
        loaded.q3_group_memberships,
        vec![
            PpcQ3GroupMembershipRecord {
                group: display_group,
                object: PPC_Q3_OBJECT_BASE + PPC_Q3_OBJECT_STRIDE * 2,
                before: None,
            },
            PpcQ3GroupMembershipRecord {
                group: display_group,
                object: object_group,
                before: None,
            },
            PpcQ3GroupMembershipRecord {
                group: object_group,
                object: read_object,
                before: None,
            },
        ]
    );

    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::Q3FileIsEndOfFile;
    loaded.cpu.gpr[3] = file;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], 1);

    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::Q3FileReadObject;
    loaded.cpu.gpr[3] = file;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], 0);
    assert_eq!(loaded.q3_files[0].read_offset, storage_bytes.len() as u32);
}

#[test]
fn hle_import_runner_reads_q3_file_trimesh_body_data() {
    let file = PPC_Q3_OBJECT_BASE;
    let storage = PPC_Q3_OBJECT_BASE + PPC_Q3_OBJECT_STRIDE;
    let trimesh = PPC_Q3_OBJECT_BASE + PPC_Q3_OBJECT_STRIDE * 2;
    let storage_ptr = PPC_DATA_BASE + 0x1000;
    let output_ptr = PPC_DATA_BASE + 0x2000;
    let pef = synthetic_pef_with_library_import(b"QuickDraw\xaa 3D", b"Q3File_ReadObject");
    let mut loaded = load_pef_application(&pef).unwrap();
    let trimesh_body: Vec<u8> = (0..PPC_Q3_TRIMESH_DATA_SIZE)
        .map(|offset| 0x40u8.wrapping_add(offset as u8))
        .collect();
    let mut storage_bytes = Vec::new();
    storage_bytes.extend_from_slice(b"3DMF");
    storage_bytes.extend_from_slice(&0u32.to_be_bytes());
    storage_bytes.extend_from_slice(b"tmsh");
    storage_bytes.extend_from_slice(&PPC_Q3_TRIMESH_DATA_SIZE.to_be_bytes());
    storage_bytes.extend_from_slice(&trimesh_body);
    loaded.memory.add_region(storage_ptr, storage_bytes.clone());
    loaded
        .memory
        .add_region(output_ptr, vec![0; PPC_Q3_TRIMESH_DATA_SIZE as usize]);
    loaded.q3_objects.push(PpcQ3ObjectRecord {
        object: storage,
        kind: PpcQ3ObjectKind::MemoryStorage,
        object_type: PPC_Q3_STORAGE_TYPE_MEMORY,
        source: PpcQ3ObjectSource::default(),
        data_ptr: storage_ptr,
        data_size: storage_bytes.len() as u32,
    });
    loaded
        .q3_objects
        .push(test_q3_object(file, PPC_Q3_TYPE_FILE));
    loaded.next_q3_object = trimesh;
    loaded.q3_files.push(PpcQ3FileRecord {
        file,
        storage,
        is_open: true,
        object_type: 0,
        read_offset: 0,
        read_object: 0,
    });
    loaded.cpu.gpr[3] = file;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], trimesh);
    assert_eq!(loaded.q3_files[0].read_offset, storage_bytes.len() as u32);
    assert_eq!(
        loaded.q3_objects[2],
        PpcQ3ObjectRecord {
            object: trimesh,
            kind: PpcQ3ObjectKind::Generic,
            object_type: PPC_Q3_TYPE_TRIMESH,
            source: PpcQ3ObjectSource {
                file,
                offset: 8,
                parent_group_type: PPC_Q3_TYPE_NONE,
                group_depth: 0,
            },
            data_ptr: storage_ptr + 8,
            data_size: PPC_Q3_TRIMESH_DATA_SIZE + 8,
        }
    );
    assert_eq!(
        loaded.q3_trimeshes,
        vec![PpcQ3TriMeshRecord {
            trimesh,
            data: trimesh_body.clone(),
            triangle_attribute_sets: Vec::new(),
            get_data_copies: Vec::new(),
        }]
    );
    loaded.memory.write_u8(storage_ptr + 16, 0).unwrap();

    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::Q3TriMeshGetData;
    loaded.cpu.gpr[3] = trimesh;
    loaded.cpu.gpr[4] = output_ptr;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], 1);
    for offset in 0..PPC_Q3_TRIMESH_DATA_SIZE {
        assert_eq!(
            loaded.memory.read_u8(output_ptr + offset),
            Some(trimesh_body[offset as usize])
        );
    }
}

#[test]
fn hle_import_runner_decodes_q3_file_3dmf_trimesh_body_data() {
    fn append_attribute_array(
        body: &mut Vec<u8>,
        attribute_type: u32,
        which_array: u32,
        which_attr: u32,
        data: &[u8],
    ) {
        body.extend_from_slice(b"atar");
        body.extend_from_slice(&(20u32 + data.len() as u32).to_be_bytes());
        body.extend_from_slice(&attribute_type.to_be_bytes());
        body.extend_from_slice(&0u32.to_be_bytes());
        body.extend_from_slice(&which_array.to_be_bytes());
        body.extend_from_slice(&which_attr.to_be_bytes());
        body.extend_from_slice(&0u32.to_be_bytes());
        body.extend_from_slice(data);
    }

    let file = PPC_Q3_OBJECT_BASE;
    let storage = PPC_Q3_OBJECT_BASE + PPC_Q3_OBJECT_STRIDE;
    let trimesh = PPC_Q3_OBJECT_BASE + PPC_Q3_OBJECT_STRIDE * 2;
    let storage_ptr = PPC_DATA_BASE + 0x1000;
    let output_ptr = PPC_DATA_BASE + 0x2000;
    let pef = synthetic_pef_with_library_import(b"QuickDraw\xaa 3D", b"Q3File_ReadObject");
    let mut loaded = load_pef_application(&pef).unwrap();
    let mut trimesh_body = Vec::new();
    trimesh_body.extend_from_slice(&1u32.to_be_bytes()); // triangles
    trimesh_body.extend_from_slice(&1u32.to_be_bytes()); // triangle attribute types
    trimesh_body.extend_from_slice(&1u32.to_be_bytes()); // edges
    trimesh_body.extend_from_slice(&0u32.to_be_bytes()); // edge attribute types
    trimesh_body.extend_from_slice(&3u32.to_be_bytes()); // points
    trimesh_body.extend_from_slice(&2u32.to_be_bytes()); // vertex attribute types
    trimesh_body.extend_from_slice(&[0, 1, 2]); // triangle point indices
    trimesh_body.extend_from_slice(&[0, 2, 0, 0xff]); // edge point and triangle indices
    let point_bytes_start = trimesh_body.len();
    for point in [(1.0f32, 2.0f32, 3.0f32), (4.0, 5.0, 6.0), (7.0, 8.0, 9.0)] {
        for value in [point.0, point.1, point.2] {
            trimesh_body.extend_from_slice(&value.to_bits().to_be_bytes());
        }
    }
    for value in [-1.0f32, -2.0, -3.0, 7.0, 8.0, 9.0] {
        trimesh_body.extend_from_slice(&value.to_bits().to_be_bytes());
    }
    trimesh_body.extend_from_slice(&0u32.to_be_bytes());
    let point_bytes = trimesh_body[point_bytes_start..point_bytes_start + 36].to_vec();
    let mut triangle_normal_bytes = Vec::new();
    for value in [0.0f32, 0.0, 1.0] {
        triangle_normal_bytes.extend_from_slice(&value.to_bits().to_be_bytes());
    }
    let mut normal_bytes = Vec::new();
    for normal in [(0.0f32, 0.0f32, 1.0f32), (0.0, 1.0, 0.0), (1.0, 0.0, 0.0)] {
        for value in [normal.0, normal.1, normal.2] {
            normal_bytes.extend_from_slice(&value.to_bits().to_be_bytes());
        }
    }
    let mut uv_bytes = Vec::new();
    for uv in [(0.0f32, 0.0f32), (1.0, 0.0), (0.5, 1.0)] {
        for value in [uv.0, uv.1] {
            uv_bytes.extend_from_slice(&value.to_bits().to_be_bytes());
        }
    }
    let mut attribute_chunks = Vec::new();
    append_attribute_array(
        &mut attribute_chunks,
        PPC_Q3_ATTRIBUTE_TYPE_NORMAL,
        PPC_Q3_TRIMESH_ATTRIBUTE_ARRAY_TRIANGLE,
        0,
        &triangle_normal_bytes,
    );
    append_attribute_array(
        &mut attribute_chunks,
        PPC_Q3_ATTRIBUTE_TYPE_NORMAL,
        PPC_Q3_TRIMESH_ATTRIBUTE_ARRAY_VERTEX,
        0,
        &normal_bytes,
    );
    append_attribute_array(
        &mut attribute_chunks,
        PPC_Q3_ATTRIBUTE_TYPE_SURFACE_UV,
        PPC_Q3_TRIMESH_ATTRIBUTE_ARRAY_VERTEX,
        1,
        &uv_bytes,
    );
    let container_body_size = 8u32
        + u32::try_from(trimesh_body.len()).unwrap()
        + u32::try_from(attribute_chunks.len()).unwrap();
    let mut storage_bytes = Vec::new();
    storage_bytes.extend_from_slice(b"3DMF");
    storage_bytes.extend_from_slice(&0u32.to_be_bytes());
    storage_bytes.extend_from_slice(b"cntr");
    storage_bytes.extend_from_slice(&container_body_size.to_be_bytes());
    storage_bytes.extend_from_slice(b"tmsh");
    storage_bytes.extend_from_slice(&(trimesh_body.len() as u32).to_be_bytes());
    storage_bytes.extend_from_slice(&trimesh_body);
    storage_bytes.extend_from_slice(&attribute_chunks);
    loaded.memory.add_region(storage_ptr, storage_bytes.clone());
    loaded
        .memory
        .add_region(output_ptr, vec![0; PPC_Q3_TRIMESH_DATA_SIZE as usize]);
    loaded.q3_objects.push(PpcQ3ObjectRecord {
        object: storage,
        kind: PpcQ3ObjectKind::MemoryStorage,
        object_type: PPC_Q3_STORAGE_TYPE_MEMORY,
        source: PpcQ3ObjectSource::default(),
        data_ptr: storage_ptr,
        data_size: storage_bytes.len() as u32,
    });
    loaded
        .q3_objects
        .push(test_q3_object(file, PPC_Q3_TYPE_FILE));
    loaded.next_q3_object = trimesh;
    loaded.q3_files.push(PpcQ3FileRecord {
        file,
        storage,
        is_open: true,
        object_type: 0,
        read_offset: 0,
        read_object: 0,
    });
    loaded.cpu.gpr[3] = file;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], trimesh);
    let data = &loaded
        .q3_trimeshes
        .iter()
        .find(|record| record.trimesh == trimesh)
        .unwrap()
        .data;
    assert_eq!(
        ppc_q3_trimesh_header_u32(data, PPC_Q3_TRIMESH_NUM_TRIANGLES_OFFSET),
        Some(1)
    );
    assert_eq!(
        ppc_q3_trimesh_header_u32(data, PPC_Q3_TRIMESH_NUM_TRIANGLE_ATTRIBUTE_TYPES_OFFSET),
        Some(1)
    );
    assert_eq!(
        ppc_q3_trimesh_header_u32(data, PPC_Q3_TRIMESH_NUM_EDGES_OFFSET),
        Some(1)
    );
    assert_eq!(
        ppc_q3_trimesh_header_u32(data, PPC_Q3_TRIMESH_NUM_EDGE_ATTRIBUTE_TYPES_OFFSET),
        Some(0)
    );
    assert_eq!(
        ppc_q3_trimesh_header_u32(data, PPC_Q3_TRIMESH_NUM_POINTS_OFFSET),
        Some(3)
    );
    assert_eq!(
        ppc_q3_trimesh_header_u32(data, PPC_Q3_TRIMESH_NUM_VERTEX_ATTRIBUTE_TYPES_OFFSET),
        Some(2)
    );
    let triangles_ptr =
        ppc_q3_trimesh_header_u32(data, PPC_Q3_TRIMESH_TRIANGLES_OFFSET).unwrap();
    let triangle_attrs_ptr =
        ppc_q3_trimesh_header_u32(data, PPC_Q3_TRIMESH_TRIANGLE_ATTRIBUTE_TYPES_OFFSET)
            .unwrap();
    let edges_ptr = ppc_q3_trimesh_header_u32(data, PPC_Q3_TRIMESH_EDGES_OFFSET).unwrap();
    let points_ptr = ppc_q3_trimesh_header_u32(data, PPC_Q3_TRIMESH_POINTS_OFFSET).unwrap();
    let vertex_attrs_ptr =
        ppc_q3_trimesh_header_u32(data, PPC_Q3_TRIMESH_VERTEX_ATTRIBUTE_TYPES_OFFSET).unwrap();
    assert_eq!(loaded.memory.read_u32_be(triangles_ptr), Some(0));
    assert_eq!(loaded.memory.read_u32_be(triangles_ptr + 4), Some(1));
    assert_eq!(loaded.memory.read_u32_be(triangles_ptr + 8), Some(2));
    assert_eq!(loaded.memory.read_u32_be(edges_ptr), Some(0));
    assert_eq!(loaded.memory.read_u32_be(edges_ptr + 4), Some(2));
    assert_ne!(triangle_attrs_ptr, 0);
    assert_eq!(
        loaded.memory.read_u32_be(triangle_attrs_ptr),
        Some(PPC_Q3_ATTRIBUTE_TYPE_NORMAL)
    );
    let triangle_normals_ptr = loaded.memory.read_u32_be(triangle_attrs_ptr + 4).unwrap();
    assert_eq!(loaded.memory.read_u32_be(triangle_attrs_ptr + 8), Some(0));
    for (offset, byte) in point_bytes.iter().copied().enumerate() {
        assert_eq!(
            loaded
                .memory
                .read_u8(points_ptr + u32::try_from(offset).unwrap()),
            Some(byte)
        );
    }
    assert_ne!(vertex_attrs_ptr, 0);
    assert_eq!(
        loaded.memory.read_u32_be(vertex_attrs_ptr),
        Some(PPC_Q3_ATTRIBUTE_TYPE_NORMAL)
    );
    let normals_ptr = loaded.memory.read_u32_be(vertex_attrs_ptr + 4).unwrap();
    assert_eq!(loaded.memory.read_u32_be(vertex_attrs_ptr + 8), Some(0));
    assert_eq!(
        loaded
            .memory
            .read_u32_be(vertex_attrs_ptr + PPC_Q3_TRIMESH_ATTRIBUTE_DATA_SIZE),
        Some(PPC_Q3_ATTRIBUTE_TYPE_SURFACE_UV)
    );
    let uvs_ptr = loaded
        .memory
        .read_u32_be(vertex_attrs_ptr + PPC_Q3_TRIMESH_ATTRIBUTE_DATA_SIZE + 4)
        .unwrap();
    assert_eq!(
        loaded
            .memory
            .read_u32_be(vertex_attrs_ptr + PPC_Q3_TRIMESH_ATTRIBUTE_DATA_SIZE + 8),
        Some(0)
    );
    for (offset, byte) in normal_bytes.iter().copied().enumerate() {
        assert_eq!(
            loaded
                .memory
                .read_u8(normals_ptr + u32::try_from(offset).unwrap()),
            Some(byte)
        );
    }
    for (offset, byte) in triangle_normal_bytes.iter().copied().enumerate() {
        assert_eq!(
            loaded
                .memory
                .read_u8(triangle_normals_ptr + u32::try_from(offset).unwrap()),
            Some(byte)
        );
    }
    for (offset, byte) in uv_bytes.iter().copied().enumerate() {
        assert_eq!(
            loaded
                .memory
                .read_u8(uvs_ptr + u32::try_from(offset).unwrap()),
            Some(byte)
        );
    }

    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::Q3TriMeshGetData;
    loaded.cpu.gpr[3] = trimesh;
    loaded.cpu.gpr[4] = output_ptr;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], 1);
    assert_eq!(
        loaded
            .memory
            .read_u32_be(output_ptr + PPC_Q3_TRIMESH_NUM_POINTS_OFFSET),
        Some(3)
    );
}

#[test]
fn hle_import_runner_exposes_q3_file_container_root_trimesh_data() {
    let file = PPC_Q3_OBJECT_BASE;
    let storage = PPC_Q3_OBJECT_BASE + PPC_Q3_OBJECT_STRIDE;
    let container = PPC_Q3_OBJECT_BASE + PPC_Q3_OBJECT_STRIDE * 2;
    let child_trimesh = PPC_Q3_OBJECT_BASE + PPC_Q3_OBJECT_STRIDE * 3;
    let storage_ptr = PPC_DATA_BASE + 0x1000;
    let output_ptr = PPC_DATA_BASE + 0x2000;
    let pef = synthetic_pef_with_library_import(b"QuickDraw\xaa 3D", b"Q3File_ReadObject");
    let mut loaded = load_pef_application(&pef).unwrap();
    let mut trimesh_body = vec![0; PPC_Q3_TRIMESH_DATA_SIZE as usize];
    trimesh_body[PPC_Q3_TRIMESH_NUM_TRIANGLES_OFFSET as usize
        ..PPC_Q3_TRIMESH_NUM_TRIANGLES_OFFSET as usize + 4]
        .copy_from_slice(&1u32.to_be_bytes());
    trimesh_body[PPC_Q3_TRIMESH_NUM_POINTS_OFFSET as usize
        ..PPC_Q3_TRIMESH_NUM_POINTS_OFFSET as usize + 4]
        .copy_from_slice(&3u32.to_be_bytes());
    let container_body_size = 8 + PPC_Q3_TRIMESH_DATA_SIZE;
    let mut storage_bytes = Vec::new();
    storage_bytes.extend_from_slice(b"3DMF");
    storage_bytes.extend_from_slice(&0u32.to_be_bytes());
    storage_bytes.extend_from_slice(b"cntr");
    storage_bytes.extend_from_slice(&container_body_size.to_be_bytes());
    storage_bytes.extend_from_slice(b"tmsh");
    storage_bytes.extend_from_slice(&PPC_Q3_TRIMESH_DATA_SIZE.to_be_bytes());
    storage_bytes.extend_from_slice(&trimesh_body);
    loaded.memory.add_region(storage_ptr, storage_bytes.clone());
    loaded
        .memory
        .add_region(output_ptr, vec![0; PPC_Q3_TRIMESH_DATA_SIZE as usize]);
    loaded.q3_objects.push(PpcQ3ObjectRecord {
        object: storage,
        kind: PpcQ3ObjectKind::MemoryStorage,
        object_type: PPC_Q3_STORAGE_TYPE_MEMORY,
        source: PpcQ3ObjectSource::default(),
        data_ptr: storage_ptr,
        data_size: storage_bytes.len() as u32,
    });
    loaded
        .q3_objects
        .push(test_q3_object(file, PPC_Q3_TYPE_FILE));
    loaded.next_q3_object = container;
    loaded.q3_files.push(PpcQ3FileRecord {
        file,
        storage,
        is_open: true,
        object_type: 0,
        read_offset: 0,
        read_object: 0,
    });
    loaded.cpu.gpr[3] = file;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], container);
    assert_eq!(
        loaded.q3_objects[2],
        PpcQ3ObjectRecord {
            object: container,
            kind: PpcQ3ObjectKind::Generic,
            object_type: PPC_Q3_TYPE_TRIMESH,
            source: PpcQ3ObjectSource {
                file,
                offset: 8,
                parent_group_type: PPC_Q3_TYPE_NONE,
                group_depth: 0,
            },
            data_ptr: storage_ptr + 16,
            data_size: PPC_Q3_TRIMESH_DATA_SIZE + 8,
        }
    );
    assert_eq!(
        loaded.q3_group_memberships,
        vec![PpcQ3GroupMembershipRecord {
            group: container,
            object: child_trimesh,
            before: None,
        }]
    );
    assert_eq!(
        loaded
            .q3_trimeshes
            .iter()
            .find(|record| record.trimesh == container)
            .map(|record| record.data.as_slice()),
        Some(trimesh_body.as_slice())
    );

    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::Q3ObjectIsType;
    loaded.cpu.gpr[3] = container;
    loaded.cpu.gpr[4] = PPC_Q3_TYPE_TRIMESH;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], 1);

    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::Q3TriMeshGetData;
    loaded.cpu.gpr[3] = container;
    loaded.cpu.gpr[4] = output_ptr;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], 1);
    assert_eq!(
        loaded
            .memory
            .read_u32_be(output_ptr + PPC_Q3_TRIMESH_NUM_POINTS_OFFSET),
        Some(3)
    );
}

#[test]
fn hle_import_runner_keeps_grouped_q3_file_container_trimesh_wrapped() {
    let file = PPC_Q3_OBJECT_BASE;
    let storage = PPC_Q3_OBJECT_BASE + PPC_Q3_OBJECT_STRIDE;
    let container = PPC_Q3_OBJECT_BASE + PPC_Q3_OBJECT_STRIDE * 2;
    let display_group = PPC_Q3_OBJECT_BASE + PPC_Q3_OBJECT_STRIDE * 3;
    let child_trimesh = PPC_Q3_OBJECT_BASE + PPC_Q3_OBJECT_STRIDE * 4;
    let storage_ptr = PPC_DATA_BASE + 0x1000;
    let output_ptr = PPC_DATA_BASE + 0x2000;
    let pef = synthetic_pef_with_library_import(b"QuickDraw\xaa 3D", b"Q3File_ReadObject");
    let mut loaded = load_pef_application(&pef).unwrap();
    let mut trimesh_body = vec![0; PPC_Q3_TRIMESH_DATA_SIZE as usize];
    trimesh_body[PPC_Q3_TRIMESH_NUM_POINTS_OFFSET as usize
        ..PPC_Q3_TRIMESH_NUM_POINTS_OFFSET as usize + 4]
        .copy_from_slice(&3u32.to_be_bytes());
    let container_body_size = 8 + PPC_Q3_TRIMESH_DATA_SIZE;
    let mut storage_bytes = Vec::new();
    storage_bytes.extend_from_slice(b"3DMF");
    storage_bytes.extend_from_slice(&0u32.to_be_bytes());
    storage_bytes.extend_from_slice(b"bgng");
    storage_bytes.extend_from_slice(&8u32.to_be_bytes());
    storage_bytes.extend_from_slice(b"dspg");
    storage_bytes.extend_from_slice(&0u32.to_be_bytes());
    storage_bytes.extend_from_slice(b"cntr");
    storage_bytes.extend_from_slice(&container_body_size.to_be_bytes());
    storage_bytes.extend_from_slice(b"tmsh");
    storage_bytes.extend_from_slice(&PPC_Q3_TRIMESH_DATA_SIZE.to_be_bytes());
    storage_bytes.extend_from_slice(&trimesh_body);
    storage_bytes.extend_from_slice(b"endg");
    storage_bytes.extend_from_slice(&0u32.to_be_bytes());
    loaded.memory.add_region(storage_ptr, storage_bytes.clone());
    loaded
        .memory
        .add_region(output_ptr, vec![0; PPC_Q3_TRIMESH_DATA_SIZE as usize]);
    loaded.q3_objects.push(PpcQ3ObjectRecord {
        object: storage,
        kind: PpcQ3ObjectKind::MemoryStorage,
        object_type: PPC_Q3_STORAGE_TYPE_MEMORY,
        source: PpcQ3ObjectSource::default(),
        data_ptr: storage_ptr,
        data_size: storage_bytes.len() as u32,
    });
    loaded
        .q3_objects
        .push(test_q3_object(file, PPC_Q3_TYPE_FILE));
    loaded.next_q3_object = container;
    loaded.q3_files.push(PpcQ3FileRecord {
        file,
        storage,
        is_open: true,
        object_type: 0,
        read_offset: 0,
        read_object: 0,
    });
    loaded.cpu.gpr[3] = file;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], container);
    assert_eq!(
        loaded.q3_objects[2],
        PpcQ3ObjectRecord {
            object: container,
            kind: PpcQ3ObjectKind::Generic,
            object_type: PPC_Q3_TYPE_CONTAINER,
            source: PpcQ3ObjectSource {
                file,
                offset: 24,
                parent_group_type: u32::from_be_bytes(*b"dspg"),
                group_depth: 1,
            },
            data_ptr: storage_ptr + 24,
            data_size: container_body_size + 8,
        }
    );
    assert_eq!(
        loaded.q3_file_groups,
        vec![PpcQ3FileGroupRecord {
            file,
            offset: 8,
            group: display_group,
            group_type: u32::from_be_bytes(*b"dspg"),
            parent_group: 0,
            group_depth: 1,
        }]
    );
    assert_eq!(
        loaded.q3_group_memberships,
        vec![
            PpcQ3GroupMembershipRecord {
                group: display_group,
                object: container,
                before: None,
            },
            PpcQ3GroupMembershipRecord {
                group: container,
                object: child_trimesh,
                before: None,
            },
            PpcQ3GroupMembershipRecord {
                group: display_group,
                object: child_trimesh,
                before: None,
            },
        ]
    );
    assert_eq!(
        loaded
            .q3_trimeshes
            .iter()
            .find(|record| record.trimesh == child_trimesh)
            .map(|record| record.data.as_slice()),
        Some(trimesh_body.as_slice())
    );
    assert!(loaded
        .q3_trimeshes
        .iter()
        .all(|record| record.trimesh != container));

    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::Q3ObjectIsType;
    loaded.cpu.gpr[3] = container;
    loaded.cpu.gpr[4] = PPC_Q3_TYPE_TRIMESH;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], 0);

    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::Q3TriMeshGetData;
    loaded.cpu.gpr[3] = container;
    loaded.cpu.gpr[4] = output_ptr;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], 0);
}

#[test]
fn hle_import_runner_reads_q3_file_matrix_transform_body_data() {
    let file = PPC_Q3_OBJECT_BASE;
    let storage = PPC_Q3_OBJECT_BASE + PPC_Q3_OBJECT_STRIDE;
    let transform = PPC_Q3_OBJECT_BASE + PPC_Q3_OBJECT_STRIDE * 2;
    let storage_ptr = PPC_DATA_BASE + 0x1000;
    let output_ptr = PPC_DATA_BASE + 0x2000;
    let pef = synthetic_pef_with_library_import(b"QuickDraw\xaa 3D", b"Q3File_ReadObject");
    let mut loaded = load_pef_application(&pef).unwrap();
    let mut matrix = ppc_q3_matrix4x4_identity();
    matrix[3][0] = 7.0;
    matrix[3][1] = 8.0;
    matrix[3][2] = 9.0;
    let mut matrix_body = Vec::with_capacity(64);
    for row in matrix {
        for value in row {
            matrix_body.extend_from_slice(&value.to_bits().to_be_bytes());
        }
    }
    let mut storage_bytes = Vec::new();
    storage_bytes.extend_from_slice(b"3DMF");
    storage_bytes.extend_from_slice(&0u32.to_be_bytes());
    storage_bytes.extend_from_slice(b"mtrx");
    storage_bytes.extend_from_slice(&64u32.to_be_bytes());
    storage_bytes.extend_from_slice(&matrix_body);
    loaded.memory.add_region(storage_ptr, storage_bytes.clone());
    loaded.memory.add_region(output_ptr, vec![0; 64]);
    loaded.q3_objects.push(PpcQ3ObjectRecord {
        object: storage,
        kind: PpcQ3ObjectKind::MemoryStorage,
        object_type: PPC_Q3_STORAGE_TYPE_MEMORY,
        source: PpcQ3ObjectSource::default(),
        data_ptr: storage_ptr,
        data_size: storage_bytes.len() as u32,
    });
    loaded
        .q3_objects
        .push(test_q3_object(file, PPC_Q3_TYPE_FILE));
    loaded.next_q3_object = transform;
    loaded.q3_files.push(PpcQ3FileRecord {
        file,
        storage,
        is_open: true,
        object_type: 0,
        read_offset: 0,
        read_object: 0,
    });
    loaded.cpu.gpr[3] = file;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], transform);
    assert_eq!(
        loaded.q3_objects[2],
        PpcQ3ObjectRecord {
            object: transform,
            kind: PpcQ3ObjectKind::Generic,
            object_type: PPC_Q3_TRANSFORM_TYPE_MATRIX,
            source: PpcQ3ObjectSource {
                file,
                offset: 8,
                parent_group_type: PPC_Q3_TYPE_NONE,
                group_depth: 0,
            },
            data_ptr: storage_ptr + 8,
            data_size: 72,
        }
    );

    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::Q3TransformGetMatrix;
    loaded.cpu.gpr[3] = transform;
    loaded.cpu.gpr[4] = output_ptr;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], 1);
    assert_eq!(
        ppc_read_q3_matrix4x4(&mut loaded.memory, output_ptr).unwrap(),
        matrix
    );
}

#[test]
fn hle_import_runner_reads_q3_file_style_chunks() {
    let file = PPC_Q3_OBJECT_BASE;
    let storage = PPC_Q3_OBJECT_BASE + PPC_Q3_OBJECT_STRIDE;
    let container = PPC_Q3_OBJECT_BASE + PPC_Q3_OBJECT_STRIDE * 2;
    let fill_style = PPC_Q3_OBJECT_BASE + PPC_Q3_OBJECT_STRIDE * 3;
    let interpolation_style = PPC_Q3_OBJECT_BASE + PPC_Q3_OBJECT_STRIDE * 4;
    let backfacing_style = PPC_Q3_OBJECT_BASE + PPC_Q3_OBJECT_STRIDE * 5;
    let storage_ptr = PPC_DATA_BASE + 0x1000;
    let pef = synthetic_pef_with_library_import(b"QuickDraw\xaa 3D", b"Q3File_ReadObject");
    let mut loaded = load_pef_application(&pef).unwrap();
    let style_chunk_size = 8u32 + 4;
    let container_body_size = style_chunk_size * 3;
    let mut storage_bytes = Vec::new();
    storage_bytes.extend_from_slice(b"3DMF");
    storage_bytes.extend_from_slice(&0u32.to_be_bytes());
    storage_bytes.extend_from_slice(b"cntr");
    storage_bytes.extend_from_slice(&container_body_size.to_be_bytes());
    storage_bytes.extend_from_slice(b"fist");
    storage_bytes.extend_from_slice(&4u32.to_be_bytes());
    storage_bytes.extend_from_slice(&PPC_Q3_FILL_STYLE_EDGES.to_be_bytes());
    storage_bytes.extend_from_slice(b"intp");
    storage_bytes.extend_from_slice(&4u32.to_be_bytes());
    storage_bytes.extend_from_slice(&PPC_Q3_INTERPOLATION_STYLE_VERTEX.to_be_bytes());
    storage_bytes.extend_from_slice(b"bckf");
    storage_bytes.extend_from_slice(&4u32.to_be_bytes());
    storage_bytes.extend_from_slice(&PPC_Q3_BACKFACING_STYLE_FLIP.to_be_bytes());
    loaded.memory.add_region(storage_ptr, storage_bytes.clone());
    loaded.q3_objects.push(PpcQ3ObjectRecord {
        object: storage,
        kind: PpcQ3ObjectKind::MemoryStorage,
        object_type: PPC_Q3_STORAGE_TYPE_MEMORY,
        source: PpcQ3ObjectSource::default(),
        data_ptr: storage_ptr,
        data_size: storage_bytes.len() as u32,
    });
    loaded
        .q3_objects
        .push(test_q3_object(file, PPC_Q3_TYPE_FILE));
    loaded.next_q3_object = container;
    loaded.q3_files.push(PpcQ3FileRecord {
        file,
        storage,
        is_open: true,
        object_type: 0,
        read_offset: 0,
        read_object: 0,
    });
    loaded.cpu.gpr[3] = file;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], container);
    assert_eq!(
        loaded.q3_group_memberships,
        vec![
            PpcQ3GroupMembershipRecord {
                group: container,
                object: fill_style,
                before: None,
            },
            PpcQ3GroupMembershipRecord {
                group: container,
                object: interpolation_style,
                before: None,
            },
            PpcQ3GroupMembershipRecord {
                group: container,
                object: backfacing_style,
                before: None,
            },
        ]
    );
    assert_eq!(
        loaded.q3_styles,
        vec![
            PpcQ3StyleRecord {
                style: fill_style,
                kind: PpcQ3StyleKind::Fill,
                value: PPC_Q3_FILL_STYLE_EDGES,
            },
            PpcQ3StyleRecord {
                style: interpolation_style,
                kind: PpcQ3StyleKind::Interpolation,
                value: PPC_Q3_INTERPOLATION_STYLE_VERTEX,
            },
            PpcQ3StyleRecord {
                style: backfacing_style,
                kind: PpcQ3StyleKind::Backfacing,
                value: PPC_Q3_BACKFACING_STYLE_FLIP,
            },
            PpcQ3StyleRecord {
                style: container,
                kind: PpcQ3StyleKind::Fill,
                value: PPC_Q3_FILL_STYLE_EDGES,
            },
        ]
    );
}

#[test]
fn hle_import_runner_reads_q3_file_texture_shader_container_state() {
    let file = PPC_Q3_OBJECT_BASE;
    let storage = PPC_Q3_OBJECT_BASE + PPC_Q3_OBJECT_STRIDE;
    let container = PPC_Q3_OBJECT_BASE + PPC_Q3_OBJECT_STRIDE * 2;
    let shader = PPC_Q3_OBJECT_BASE + PPC_Q3_OBJECT_STRIDE * 3;
    let texture = PPC_Q3_OBJECT_BASE + PPC_Q3_OBJECT_STRIDE * 4;
    let view = PPC_Q3_OBJECT_BASE + PPC_Q3_OBJECT_STRIDE * 5;
    let storage_ptr = PPC_DATA_BASE + 0x1000;
    let texture_out_ptr = PPC_DATA_BASE + 0x3000;
    let mipmap_out_ptr = PPC_DATA_BASE + 0x3100;
    let pef = synthetic_pef_with_library_import(b"QuickDraw\xaa 3D", b"Q3File_ReadObject");
    let mut loaded = load_pef_application(&pef).unwrap();
    let mipmap: Vec<u8> = (0..PPC_Q3_MIPMAP_COPY_SIZE + 12)
        .map(|offset| 0x30u8.wrapping_add(offset as u8))
        .collect();
    let container_body_size = 8 + 8 + mipmap.len() as u32;
    let mut storage_bytes = Vec::new();
    storage_bytes.extend_from_slice(b"3DMF");
    storage_bytes.extend_from_slice(&0u32.to_be_bytes());
    storage_bytes.extend_from_slice(b"cntr");
    storage_bytes.extend_from_slice(&container_body_size.to_be_bytes());
    storage_bytes.extend_from_slice(b"txsu");
    storage_bytes.extend_from_slice(&0u32.to_be_bytes());
    storage_bytes.extend_from_slice(b"txmm");
    storage_bytes.extend_from_slice(&(mipmap.len() as u32).to_be_bytes());
    storage_bytes.extend_from_slice(&mipmap);
    loaded.memory.add_region(storage_ptr, storage_bytes.clone());
    loaded.memory.add_region(texture_out_ptr, vec![0; 4]);
    loaded
        .memory
        .add_region(mipmap_out_ptr, vec![0; mipmap.len()]);
    loaded.q3_objects.push(PpcQ3ObjectRecord {
        object: storage,
        kind: PpcQ3ObjectKind::MemoryStorage,
        object_type: PPC_Q3_STORAGE_TYPE_MEMORY,
        source: PpcQ3ObjectSource::default(),
        data_ptr: storage_ptr,
        data_size: storage_bytes.len() as u32,
    });
    loaded
        .q3_objects
        .push(test_q3_object(file, PPC_Q3_TYPE_FILE));
    loaded.next_q3_object = container;
    loaded.q3_files.push(PpcQ3FileRecord {
        file,
        storage,
        is_open: true,
        object_type: 0,
        read_offset: 0,
        read_object: 0,
    });
    loaded.cpu.gpr[3] = file;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], container);
    assert_eq!(loaded.q3_files[0].read_offset, storage_bytes.len() as u32);
    assert_eq!(
        loaded.q3_objects[2],
        PpcQ3ObjectRecord {
            object: container,
            kind: PpcQ3ObjectKind::Generic,
            object_type: PPC_Q3_SURFACE_SHADER_TYPE_TEXTURE,
            source: PpcQ3ObjectSource {
                file,
                offset: 8,
                parent_group_type: PPC_Q3_TYPE_NONE,
                group_depth: 0,
            },
            data_ptr: storage_ptr + 16,
            data_size: 8,
        }
    );
    assert_eq!(
        loaded.q3_objects[3],
        PpcQ3ObjectRecord {
            object: shader,
            kind: PpcQ3ObjectKind::Generic,
            object_type: PPC_Q3_SURFACE_SHADER_TYPE_TEXTURE,
            source: PpcQ3ObjectSource {
                file,
                offset: 16,
                parent_group_type: PPC_Q3_TYPE_CONTAINER,
                group_depth: 1,
            },
            data_ptr: storage_ptr + 16,
            data_size: 8,
        }
    );
    assert_eq!(
        loaded.q3_objects[4],
        PpcQ3ObjectRecord {
            object: texture,
            kind: PpcQ3ObjectKind::Generic,
            object_type: PPC_Q3_TEXTURE_TYPE_MIPMAP,
            source: PpcQ3ObjectSource {
                file,
                offset: 24,
                parent_group_type: PPC_Q3_TYPE_CONTAINER,
                group_depth: 1,
            },
            data_ptr: storage_ptr + 24,
            data_size: mipmap.len() as u32 + 8,
        }
    );
    assert_eq!(
        loaded.q3_group_memberships,
        vec![
            PpcQ3GroupMembershipRecord {
                group: container,
                object: shader,
                before: None,
            },
            PpcQ3GroupMembershipRecord {
                group: container,
                object: texture,
                before: None,
            },
        ]
    );
    assert_eq!(
        loaded.q3_texture_shaders,
        vec![
            PpcQ3TextureShaderRecord { shader, texture },
            PpcQ3TextureShaderRecord {
                shader: container,
                texture,
            },
        ]
    );
    assert_eq!(
        loaded.q3_mipmap_textures,
        vec![PpcQ3MipmapTextureRecord {
            texture,
            mipmap: mipmap.clone(),
        }]
    );
    loaded.memory.write_u8(storage_ptr + 32, 0xff).unwrap();
    assert_eq!(loaded.q3_mipmap_textures[0].mipmap[0], mipmap[0]);

    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::Q3TextureShaderGetTexture;
    loaded.cpu.gpr[3] = shader;
    loaded.cpu.gpr[4] = texture_out_ptr;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], 1);
    assert_eq!(loaded.memory.read_u32_be(texture_out_ptr), Some(texture));

    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::Q3MipmapTextureGetMipmap;
    loaded.cpu.gpr[3] = texture;
    loaded.cpu.gpr[4] = mipmap_out_ptr;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], 1);
    assert_eq!(loaded.memory.read_u8(mipmap_out_ptr), Some(mipmap[0]));
    assert_eq!(
        loaded
            .memory
            .read_u8(mipmap_out_ptr + u32::try_from(mipmap.len()).unwrap() - 1),
        Some(*mipmap.last().unwrap())
    );

    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::Q3ShaderSubmit;
    loaded
        .q3_objects
        .push(test_q3_object(view, PPC_Q3_TYPE_VIEW));
    loaded.cpu.gpr[3] = shader;
    loaded.cpu.gpr[4] = view;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], 1);
    assert_eq!(
        loaded.q3_submission_materials,
        vec![PpcQ3SubmissionMaterialRecord {
            view,
            kind: PpcQ3SubmissionKind::Shader,
            primary: shader,
            secondary: 0,
            shader,
            illumination_type: PPC_Q3_ILLUMINATION_TYPE_PHONG,
            styles: Vec::new(),
            fog_style: None,
            attributes: Vec::new(),
            shader_uv_transform: None,
            shader_boundary: None,
            texture_shader: Some(PpcQ3TextureShaderRecord { shader, texture }),
            mipmap_texture: Some(PpcQ3MipmapTextureRecord {
                texture,
                mipmap: mipmap.clone(),
            }),
        }]
    );
    loaded.q3_mipmap_textures[0].mipmap[0] = 0xaa;
    assert_eq!(
        loaded.q3_submission_materials[0]
            .mipmap_texture
            .as_ref()
            .map(|record| record.mipmap.as_slice()),
        Some(mipmap.as_slice())
    );
}

#[test]
fn hle_import_runner_reads_nested_q3_file_texture_shader_container_state() {
    let file = PPC_Q3_OBJECT_BASE;
    let storage = PPC_Q3_OBJECT_BASE + PPC_Q3_OBJECT_STRIDE;
    let outer_container = PPC_Q3_OBJECT_BASE + PPC_Q3_OBJECT_STRIDE * 2;
    let attribute = PPC_Q3_OBJECT_BASE + PPC_Q3_OBJECT_STRIDE * 3;
    let inner_container = PPC_Q3_OBJECT_BASE + PPC_Q3_OBJECT_STRIDE * 4;
    let shader = PPC_Q3_OBJECT_BASE + PPC_Q3_OBJECT_STRIDE * 5;
    let texture = PPC_Q3_OBJECT_BASE + PPC_Q3_OBJECT_STRIDE * 6;
    let storage_ptr = PPC_DATA_BASE + 0x1000;
    let pef = synthetic_pef_with_library_import(b"QuickDraw\xaa 3D", b"Q3File_ReadObject");
    let mut loaded = load_pef_application(&pef).unwrap();
    let mipmap = vec![0x11, 0x22, 0x33, 0x44, 0x55];
    let inner_body_size = 8 + 8 + mipmap.len() as u32;
    let outer_body_size = 8 + 8 + inner_body_size;
    let mut storage_bytes = Vec::new();
    storage_bytes.extend_from_slice(b"3DMF");
    storage_bytes.extend_from_slice(&0u32.to_be_bytes());
    storage_bytes.extend_from_slice(b"cntr");
    storage_bytes.extend_from_slice(&outer_body_size.to_be_bytes());
    storage_bytes.extend_from_slice(b"attr");
    storage_bytes.extend_from_slice(&0u32.to_be_bytes());
    storage_bytes.extend_from_slice(b"cntr");
    storage_bytes.extend_from_slice(&inner_body_size.to_be_bytes());
    storage_bytes.extend_from_slice(b"txsu");
    storage_bytes.extend_from_slice(&0u32.to_be_bytes());
    storage_bytes.extend_from_slice(b"txmm");
    storage_bytes.extend_from_slice(&(mipmap.len() as u32).to_be_bytes());
    storage_bytes.extend_from_slice(&mipmap);
    loaded.memory.add_region(storage_ptr, storage_bytes.clone());
    loaded.q3_objects.push(PpcQ3ObjectRecord {
        object: storage,
        kind: PpcQ3ObjectKind::MemoryStorage,
        object_type: PPC_Q3_STORAGE_TYPE_MEMORY,
        source: PpcQ3ObjectSource::default(),
        data_ptr: storage_ptr,
        data_size: storage_bytes.len() as u32,
    });
    loaded
        .q3_objects
        .push(test_q3_object(file, PPC_Q3_TYPE_FILE));
    loaded.next_q3_object = outer_container;
    loaded.q3_files.push(PpcQ3FileRecord {
        file,
        storage,
        is_open: true,
        object_type: 0,
        read_offset: 0,
        read_object: 0,
    });
    loaded.cpu.gpr[3] = file;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], outer_container);
    assert_eq!(
        loaded.q3_objects[3],
        PpcQ3ObjectRecord {
            object: attribute,
            kind: PpcQ3ObjectKind::Generic,
            object_type: u32::from_be_bytes(*b"attr"),
            source: PpcQ3ObjectSource {
                file,
                offset: 16,
                parent_group_type: PPC_Q3_TYPE_CONTAINER,
                group_depth: 1,
            },
            data_ptr: storage_ptr + 16,
            data_size: 8,
        }
    );
    assert_eq!(
        loaded.q3_group_memberships,
        vec![
            PpcQ3GroupMembershipRecord {
                group: outer_container,
                object: attribute,
                before: None,
            },
            PpcQ3GroupMembershipRecord {
                group: outer_container,
                object: inner_container,
                before: None,
            },
            PpcQ3GroupMembershipRecord {
                group: inner_container,
                object: shader,
                before: None,
            },
            PpcQ3GroupMembershipRecord {
                group: inner_container,
                object: texture,
                before: None,
            },
        ]
    );
    assert_eq!(
        loaded.q3_texture_shaders,
        vec![PpcQ3TextureShaderRecord { shader, texture }]
    );
    assert_eq!(
        loaded.q3_attributes,
        vec![
            PpcQ3AttributeRecord {
                attribute_set: attribute,
                attribute_type: PPC_Q3_ATTRIBUTE_TYPE_SURFACE_SHADER,
                data: shader.to_be_bytes().to_vec(),
            },
            PpcQ3AttributeRecord {
                attribute_set: outer_container,
                attribute_type: PPC_Q3_ATTRIBUTE_TYPE_SURFACE_SHADER,
                data: shader.to_be_bytes().to_vec(),
            },
        ]
    );
    assert_eq!(
        loaded.q3_mipmap_textures,
        vec![PpcQ3MipmapTextureRecord { texture, mipmap }]
    );
}

#[test]
fn hle_import_runner_reads_q3_file_attribute_material_state() {
    let file = PPC_Q3_OBJECT_BASE;
    let storage = PPC_Q3_OBJECT_BASE + PPC_Q3_OBJECT_STRIDE;
    let container = PPC_Q3_OBJECT_BASE + PPC_Q3_OBJECT_STRIDE * 2;
    let attribute = PPC_Q3_OBJECT_BASE + PPC_Q3_OBJECT_STRIDE * 3;
    let diffuse_chunk = PPC_Q3_OBJECT_BASE + PPC_Q3_OBJECT_STRIDE * 4;
    let ambient_chunk = PPC_Q3_OBJECT_BASE + PPC_Q3_OBJECT_STRIDE * 5;
    let specular_color_chunk = PPC_Q3_OBJECT_BASE + PPC_Q3_OBJECT_STRIDE * 6;
    let specular_control_chunk = PPC_Q3_OBJECT_BASE + PPC_Q3_OBJECT_STRIDE * 7;
    let transparency_chunk = PPC_Q3_OBJECT_BASE + PPC_Q3_OBJECT_STRIDE * 8;
    let storage_ptr = PPC_DATA_BASE + 0x1000;
    let output_ptr = PPC_DATA_BASE + 0x3000;
    let pef = synthetic_pef_with_library_import(b"QuickDraw\xaa 3D", b"Q3File_ReadObject");
    let mut loaded = load_pef_application(&pef).unwrap();
    let diffuse = vec![
        0x3d, 0xcc, 0xcc, 0xcd, 0x3e, 0x4c, 0xcc, 0xcd, 0x3e, 0x99, 0x99, 0x9a,
    ];
    let transparency = vec![
        0x3e, 0xcc, 0xcc, 0xcd, 0x3f, 0x00, 0x00, 0x00, 0x3f, 0x19, 0x99, 0x9a,
    ];
    let ambient_coefficient = 0.5f32.to_bits().to_be_bytes().to_vec();
    let specular_color = vec![
        0x3f, 0x00, 0x00, 0x00, 0x3f, 0x19, 0x99, 0x9a, 0x3f, 0x33, 0x33, 0x33,
    ];
    let specular_control = 60.0f32.to_bits().to_be_bytes().to_vec();
    let container_body_size = 8
        + 8
        + diffuse.len() as u32
        + 8
        + ambient_coefficient.len() as u32
        + 8
        + specular_color.len() as u32
        + 8
        + specular_control.len() as u32
        + 8
        + transparency.len() as u32;
    let mut storage_bytes = Vec::new();
    storage_bytes.extend_from_slice(b"3DMF");
    storage_bytes.extend_from_slice(&0u32.to_be_bytes());
    storage_bytes.extend_from_slice(b"cntr");
    storage_bytes.extend_from_slice(&container_body_size.to_be_bytes());
    storage_bytes.extend_from_slice(b"attr");
    storage_bytes.extend_from_slice(&0u32.to_be_bytes());
    storage_bytes.extend_from_slice(b"kdif");
    storage_bytes.extend_from_slice(&(diffuse.len() as u32).to_be_bytes());
    storage_bytes.extend_from_slice(&diffuse);
    storage_bytes.extend_from_slice(b"camb");
    storage_bytes.extend_from_slice(&(ambient_coefficient.len() as u32).to_be_bytes());
    storage_bytes.extend_from_slice(&ambient_coefficient);
    storage_bytes.extend_from_slice(b"kspc");
    storage_bytes.extend_from_slice(&(specular_color.len() as u32).to_be_bytes());
    storage_bytes.extend_from_slice(&specular_color);
    storage_bytes.extend_from_slice(b"cspc");
    storage_bytes.extend_from_slice(&(specular_control.len() as u32).to_be_bytes());
    storage_bytes.extend_from_slice(&specular_control);
    storage_bytes.extend_from_slice(b"kxpr");
    storage_bytes.extend_from_slice(&(transparency.len() as u32).to_be_bytes());
    storage_bytes.extend_from_slice(&transparency);
    loaded.memory.add_region(storage_ptr, storage_bytes.clone());
    loaded.memory.add_region(output_ptr, vec![0; 16]);
    loaded.q3_objects.push(PpcQ3ObjectRecord {
        object: storage,
        kind: PpcQ3ObjectKind::MemoryStorage,
        object_type: PPC_Q3_STORAGE_TYPE_MEMORY,
        source: PpcQ3ObjectSource::default(),
        data_ptr: storage_ptr,
        data_size: storage_bytes.len() as u32,
    });
    loaded
        .q3_objects
        .push(test_q3_object(file, PPC_Q3_TYPE_FILE));
    loaded.next_q3_object = container;
    loaded.q3_files.push(PpcQ3FileRecord {
        file,
        storage,
        is_open: true,
        object_type: 0,
        read_offset: 0,
        read_object: 0,
    });
    loaded.cpu.gpr[3] = file;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], container);
    assert_eq!(loaded.q3_files[0].read_offset, storage_bytes.len() as u32);
    assert_eq!(
        loaded.q3_objects[3],
        PpcQ3ObjectRecord {
            object: attribute,
            kind: PpcQ3ObjectKind::Generic,
            object_type: PPC_Q3_TYPE_ATTRIBUTE_SET,
            source: PpcQ3ObjectSource {
                file,
                offset: 16,
                parent_group_type: PPC_Q3_TYPE_CONTAINER,
                group_depth: 1,
            },
            data_ptr: storage_ptr + 16,
            data_size: 8,
        }
    );
    assert_eq!(
        loaded.q3_objects[4],
        PpcQ3ObjectRecord {
            object: diffuse_chunk,
            kind: PpcQ3ObjectKind::Generic,
            object_type: PPC_Q3_ATTRIBUTE_TYPE_DIFFUSE_COLOR_3DMF,
            source: PpcQ3ObjectSource {
                file,
                offset: 24,
                parent_group_type: PPC_Q3_TYPE_CONTAINER,
                group_depth: 1,
            },
            data_ptr: storage_ptr + 24,
            data_size: 20,
        }
    );
    assert_eq!(
        loaded.q3_objects[5],
        PpcQ3ObjectRecord {
            object: ambient_chunk,
            kind: PpcQ3ObjectKind::Generic,
            object_type: PPC_Q3_ATTRIBUTE_TYPE_AMBIENT_COEFFICIENT_3DMF,
            source: PpcQ3ObjectSource {
                file,
                offset: 44,
                parent_group_type: PPC_Q3_TYPE_CONTAINER,
                group_depth: 1,
            },
            data_ptr: storage_ptr + 44,
            data_size: 12,
        }
    );
    assert_eq!(
        loaded.q3_objects[6],
        PpcQ3ObjectRecord {
            object: specular_color_chunk,
            kind: PpcQ3ObjectKind::Generic,
            object_type: PPC_Q3_ATTRIBUTE_TYPE_SPECULAR_COLOR_3DMF,
            source: PpcQ3ObjectSource {
                file,
                offset: 56,
                parent_group_type: PPC_Q3_TYPE_CONTAINER,
                group_depth: 1,
            },
            data_ptr: storage_ptr + 56,
            data_size: 20,
        }
    );
    assert_eq!(
        loaded.q3_objects[7],
        PpcQ3ObjectRecord {
            object: specular_control_chunk,
            kind: PpcQ3ObjectKind::Generic,
            object_type: PPC_Q3_ATTRIBUTE_TYPE_SPECULAR_CONTROL_3DMF,
            source: PpcQ3ObjectSource {
                file,
                offset: 76,
                parent_group_type: PPC_Q3_TYPE_CONTAINER,
                group_depth: 1,
            },
            data_ptr: storage_ptr + 76,
            data_size: 12,
        }
    );
    assert_eq!(
        loaded.q3_objects[8],
        PpcQ3ObjectRecord {
            object: transparency_chunk,
            kind: PpcQ3ObjectKind::Generic,
            object_type: PPC_Q3_ATTRIBUTE_TYPE_TRANSPARENCY_COLOR_3DMF,
            source: PpcQ3ObjectSource {
                file,
                offset: 88,
                parent_group_type: PPC_Q3_TYPE_CONTAINER,
                group_depth: 1,
            },
            data_ptr: storage_ptr + 88,
            data_size: 20,
        }
    );
    assert_eq!(
        loaded.q3_attributes,
        vec![
            PpcQ3AttributeRecord {
                attribute_set: attribute,
                attribute_type: PPC_Q3_ATTRIBUTE_TYPE_DIFFUSE_COLOR,
                data: diffuse.clone(),
            },
            PpcQ3AttributeRecord {
                attribute_set: attribute,
                attribute_type: PPC_Q3_ATTRIBUTE_TYPE_AMBIENT_COEFFICIENT,
                data: ambient_coefficient.clone(),
            },
            PpcQ3AttributeRecord {
                attribute_set: attribute,
                attribute_type: PPC_Q3_ATTRIBUTE_TYPE_SPECULAR_COLOR,
                data: specular_color.clone(),
            },
            PpcQ3AttributeRecord {
                attribute_set: attribute,
                attribute_type: PPC_Q3_ATTRIBUTE_TYPE_SPECULAR_CONTROL,
                data: specular_control.clone(),
            },
            PpcQ3AttributeRecord {
                attribute_set: attribute,
                attribute_type: PPC_Q3_ATTRIBUTE_TYPE_TRANSPARENCY_COLOR,
                data: transparency.clone(),
            },
            PpcQ3AttributeRecord {
                attribute_set: container,
                attribute_type: PPC_Q3_ATTRIBUTE_TYPE_DIFFUSE_COLOR,
                data: diffuse.clone(),
            },
            PpcQ3AttributeRecord {
                attribute_set: container,
                attribute_type: PPC_Q3_ATTRIBUTE_TYPE_AMBIENT_COEFFICIENT,
                data: ambient_coefficient.clone(),
            },
            PpcQ3AttributeRecord {
                attribute_set: container,
                attribute_type: PPC_Q3_ATTRIBUTE_TYPE_SPECULAR_COLOR,
                data: specular_color.clone(),
            },
            PpcQ3AttributeRecord {
                attribute_set: container,
                attribute_type: PPC_Q3_ATTRIBUTE_TYPE_SPECULAR_CONTROL,
                data: specular_control.clone(),
            },
            PpcQ3AttributeRecord {
                attribute_set: container,
                attribute_type: PPC_Q3_ATTRIBUTE_TYPE_TRANSPARENCY_COLOR,
                data: transparency.clone(),
            },
        ]
    );
    loaded.memory.write_u8(storage_ptr + 32, 0xff).unwrap();
    assert_eq!(loaded.q3_attributes[0].data, diffuse);

    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::Q3AttributeSetGet;
    loaded.cpu.gpr[3] = attribute;
    loaded.cpu.gpr[4] = PPC_Q3_ATTRIBUTE_TYPE_DIFFUSE_COLOR;
    loaded.cpu.gpr[5] = output_ptr;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], 1);
    for offset in 0..12 {
        assert_eq!(
            loaded.memory.read_u8(output_ptr + offset),
            Some(diffuse[offset as usize])
        );
    }

    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.cpu.gpr[3] = attribute;
    loaded.cpu.gpr[4] = PPC_Q3_ATTRIBUTE_TYPE_AMBIENT_COEFFICIENT;
    loaded.cpu.gpr[5] = output_ptr;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], 1);
    for offset in 0..4 {
        assert_eq!(
            loaded.memory.read_u8(output_ptr + offset),
            Some(ambient_coefficient[offset as usize])
        );
    }

    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.cpu.gpr[3] = attribute;
    loaded.cpu.gpr[4] = PPC_Q3_ATTRIBUTE_TYPE_SPECULAR_COLOR;
    loaded.cpu.gpr[5] = output_ptr;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], 1);
    for offset in 0..12 {
        assert_eq!(
            loaded.memory.read_u8(output_ptr + offset),
            Some(specular_color[offset as usize])
        );
    }

    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.cpu.gpr[3] = attribute;
    loaded.cpu.gpr[4] = PPC_Q3_ATTRIBUTE_TYPE_SPECULAR_CONTROL;
    loaded.cpu.gpr[5] = output_ptr;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], 1);
    for offset in 0..4 {
        assert_eq!(
            loaded.memory.read_u8(output_ptr + offset),
            Some(specular_control[offset as usize])
        );
    }

    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.cpu.gpr[3] = attribute;
    loaded.cpu.gpr[4] = PPC_Q3_ATTRIBUTE_TYPE_TRANSPARENCY_COLOR;
    loaded.cpu.gpr[5] = output_ptr;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], 1);
    for offset in 0..12 {
        assert_eq!(
            loaded.memory.read_u8(output_ptr + offset),
            Some(transparency[offset as usize])
        );
    }
}

#[test]
fn hle_import_runner_resolves_q3_file_shared_attribute_references() {
    let file = PPC_Q3_OBJECT_BASE;
    let storage = PPC_Q3_OBJECT_BASE + PPC_Q3_OBJECT_STRIDE;
    let shared_container = PPC_Q3_OBJECT_BASE + PPC_Q3_OBJECT_STRIDE * 2;
    let nested_container = PPC_Q3_OBJECT_BASE + PPC_Q3_OBJECT_STRIDE * 3;
    let shared_attribute = PPC_Q3_OBJECT_BASE + PPC_Q3_OBJECT_STRIDE * 4;
    let geometry_container = PPC_Q3_OBJECT_BASE + PPC_Q3_OBJECT_STRIDE * 6;
    let child_trimesh = PPC_Q3_OBJECT_BASE + PPC_Q3_OBJECT_STRIDE * 7;
    let storage_ptr = PPC_DATA_BASE + 0x1000;
    let pef = synthetic_pef_with_library_import(b"QuickDraw\xaa 3D", b"Q3File_ReadObject");
    let mut loaded = load_pef_application(&pef).unwrap();
    let diffuse = [
        1.0f32.to_bits().to_be_bytes(),
        0.0f32.to_bits().to_be_bytes(),
        0.0f32.to_bits().to_be_bytes(),
    ]
    .concat();
    let mut shared_attribute_body = Vec::new();
    shared_attribute_body.extend_from_slice(b"attr");
    shared_attribute_body.extend_from_slice(&0u32.to_be_bytes());
    shared_attribute_body.extend_from_slice(b"kdif");
    shared_attribute_body.extend_from_slice(&(diffuse.len() as u32).to_be_bytes());
    shared_attribute_body.extend_from_slice(&diffuse);
    let mut shared_container_body = Vec::new();
    shared_container_body.extend_from_slice(b"cntr");
    shared_container_body
        .extend_from_slice(&(shared_attribute_body.len() as u32).to_be_bytes());
    shared_container_body.extend_from_slice(&shared_attribute_body);

    let mut storage_bytes = Vec::new();
    storage_bytes.extend_from_slice(b"3DMF");
    storage_bytes.extend_from_slice(&0u32.to_be_bytes());
    storage_bytes.extend_from_slice(b"cntr");
    storage_bytes.extend_from_slice(&(shared_container_body.len() as u32).to_be_bytes());
    let shared_attribute_offset = u32::try_from(storage_bytes.len()).unwrap();
    storage_bytes.extend_from_slice(&shared_container_body);

    let mut geometry_body = Vec::new();
    geometry_body.extend_from_slice(b"tmsh");
    geometry_body.extend_from_slice(&PPC_Q3_TRIMESH_DATA_SIZE.to_be_bytes());
    geometry_body.extend_from_slice(&vec![0; PPC_Q3_TRIMESH_DATA_SIZE as usize]);
    geometry_body.extend_from_slice(b"rfrn");
    geometry_body.extend_from_slice(&4u32.to_be_bytes());
    geometry_body.extend_from_slice(&1u32.to_be_bytes());
    storage_bytes.extend_from_slice(b"cntr");
    storage_bytes.extend_from_slice(&(geometry_body.len() as u32).to_be_bytes());
    storage_bytes.extend_from_slice(&geometry_body);

    storage_bytes.extend_from_slice(b"toc ");
    storage_bytes.extend_from_slice(&44u32.to_be_bytes());
    storage_bytes.extend_from_slice(&0u64.to_be_bytes());
    storage_bytes.extend_from_slice(&2u32.to_be_bytes());
    storage_bytes.extend_from_slice(&u32::MAX.to_be_bytes());
    storage_bytes.extend_from_slice(&1u32.to_be_bytes());
    storage_bytes.extend_from_slice(&16u32.to_be_bytes());
    storage_bytes.extend_from_slice(&1u32.to_be_bytes());
    storage_bytes.extend_from_slice(&1u32.to_be_bytes());
    storage_bytes.extend_from_slice(&(u64::from(shared_attribute_offset)).to_be_bytes());
    storage_bytes.extend_from_slice(&PPC_Q3_TYPE_ATTRIBUTE_SET.to_be_bytes());

    loaded.memory.add_region(storage_ptr, storage_bytes.clone());
    loaded.q3_objects.push(PpcQ3ObjectRecord {
        object: storage,
        kind: PpcQ3ObjectKind::MemoryStorage,
        object_type: PPC_Q3_STORAGE_TYPE_MEMORY,
        source: PpcQ3ObjectSource::default(),
        data_ptr: storage_ptr,
        data_size: storage_bytes.len() as u32,
    });
    loaded
        .q3_objects
        .push(test_q3_object(file, PPC_Q3_TYPE_FILE));
    loaded.next_q3_object = shared_container;
    loaded.q3_files.push(PpcQ3FileRecord {
        file,
        storage,
        is_open: true,
        object_type: 0,
        read_offset: 0,
        read_object: 0,
    });
    loaded.cpu.gpr[3] = file;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], shared_container);
    assert_eq!(
        ppc_q3_object_type_for_handle(&loaded.q3_objects, nested_container),
        PPC_Q3_TYPE_CONTAINER
    );

    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.cpu.gpr[3] = file;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], geometry_container);
    assert_eq!(
        loaded
            .q3_trimeshes
            .iter()
            .find(|record| record.trimesh == child_trimesh)
            .and_then(|record| ppc_q3_trimesh_header_u32(
                &record.data,
                PPC_Q3_TRIMESH_ATTRIBUTE_SET_OFFSET,
            )),
        Some(shared_attribute)
    );
    assert!(loaded.q3_attributes.iter().any(|record| {
        record.attribute_set == shared_attribute
            && record.attribute_type == PPC_Q3_ATTRIBUTE_TYPE_DIFFUSE_COLOR
            && record.data == diffuse
    }));
}

#[test]
fn hle_import_runner_tracks_q3_group_membership() {
    fn run_q3_call(loaded: &mut PpcLoadedApp, target: PpcImportDispatcherTarget) -> u32 {
        loaded.cpu.pc = loaded.entry_pc;
        loaded.cpu.lr = PPC_HALT_PC;
        loaded.imports[0].dispatcher_target = target;
        let probe = loaded.run_with_hle_imports(64);
        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        loaded.cpu.gpr[3]
    }

    let pef = synthetic_pef_with_library_import(b"QuickDraw\xaa 3D", b"Q3Group_AddObject");
    let mut loaded = load_pef_application(&pef).unwrap();
    let group = PPC_Q3_OBJECT_BASE;
    let first_object = PPC_Q3_OBJECT_BASE + PPC_Q3_OBJECT_STRIDE;
    let second_object = PPC_Q3_OBJECT_BASE + PPC_Q3_OBJECT_STRIDE * 2;
    let before_object = PPC_Q3_OBJECT_BASE + PPC_Q3_OBJECT_STRIDE * 3;
    let rejected_object = PPC_Q3_OBJECT_BASE + PPC_Q3_OBJECT_STRIDE * 4;
    loaded.q3_objects.extend([
        PpcQ3ObjectRecord {
            object: group,
            kind: PpcQ3ObjectKind::Generic,
            object_type: PPC_Q3_GROUP_TYPE_DISPLAY,
            source: PpcQ3ObjectSource::default(),
            data_ptr: 0,
            data_size: 0,
        },
        PpcQ3ObjectRecord {
            object: first_object,
            kind: PpcQ3ObjectKind::Generic,
            object_type: PPC_Q3_TYPE_TRIMESH,
            source: PpcQ3ObjectSource::default(),
            data_ptr: 0,
            data_size: 0,
        },
        PpcQ3ObjectRecord {
            object: second_object,
            kind: PpcQ3ObjectKind::Generic,
            object_type: PPC_Q3_TYPE_TRIMESH,
            source: PpcQ3ObjectSource::default(),
            data_ptr: 0,
            data_size: 0,
        },
        PpcQ3ObjectRecord {
            object: before_object,
            kind: PpcQ3ObjectKind::Generic,
            object_type: PPC_Q3_TYPE_TRIMESH,
            source: PpcQ3ObjectSource::default(),
            data_ptr: 0,
            data_size: 0,
        },
        PpcQ3ObjectRecord {
            object: rejected_object,
            kind: PpcQ3ObjectKind::Generic,
            object_type: PPC_Q3_TYPE_TRIMESH,
            source: PpcQ3ObjectSource::default(),
            data_ptr: 0,
            data_size: 0,
        },
    ]);
    loaded.cpu.gpr[3] = group;
    loaded.cpu.gpr[4] = first_object;

    assert_eq!(
        run_q3_call(&mut loaded, PpcImportDispatcherTarget::Q3GroupAddObject),
        1
    );

    loaded.cpu.gpr[3] = group;
    loaded.cpu.gpr[4] = second_object;

    assert_eq!(
        run_q3_call(&mut loaded, PpcImportDispatcherTarget::Q3GroupAddObject),
        2
    );

    loaded.cpu.gpr[3] = group;
    loaded.cpu.gpr[4] = 2;
    loaded.cpu.gpr[5] = before_object;

    assert_eq!(
        run_q3_call(
            &mut loaded,
            PpcImportDispatcherTarget::Q3GroupAddObjectBefore
        ),
        2
    );
    assert_eq!(
        loaded.q3_group_memberships,
        vec![
            PpcQ3GroupMembershipRecord {
                group,
                object: first_object,
                before: None,
            },
            PpcQ3GroupMembershipRecord {
                group,
                object: before_object,
                before: Some(2),
            },
            PpcQ3GroupMembershipRecord {
                group,
                object: second_object,
                before: None,
            },
        ]
    );
    for object in [first_object, second_object, before_object] {
        assert_eq!(
            ppc_q3_object_reference_count(&loaded.q3_objects, &loaded.q3_object_refs, object),
            Some(2)
        );
    }

    loaded.cpu.gpr[3] = group;
    loaded.cpu.gpr[4] = 99;
    loaded.cpu.gpr[5] = rejected_object;

    assert_eq!(
        run_q3_call(
            &mut loaded,
            PpcImportDispatcherTarget::Q3GroupAddObjectBefore
        ),
        0
    );
    assert_eq!(loaded.q3_group_memberships.len(), 3);
    assert_eq!(
        ppc_q3_object_reference_count(
            &loaded.q3_objects,
            &loaded.q3_object_refs,
            rejected_object
        ),
        Some(1)
    );
    assert_eq!(
        loaded.q3_error_state.first_error,
        PPC_Q3_ERROR_INVALID_OBJECT_PARAMETER
    );
}

#[test]
fn hle_import_runner_handles_q3_light_group_object_state() {
    fn run_q3_call(loaded: &mut PpcLoadedApp, target: PpcImportDispatcherTarget) -> u32 {
        loaded.cpu.pc = loaded.entry_pc;
        loaded.cpu.lr = PPC_HALT_PC;
        loaded.imports[0].dispatcher_target = target;
        let probe = loaded.run_with_hle_imports(64);
        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        loaded.cpu.gpr[3]
    }

    let pef = synthetic_pef_with_library_import(b"QuickDraw\xaa 3D", b"Q3LightGroup_New");
    let mut loaded = load_pef_application(&pef).unwrap();

    let light_group = run_q3_call(&mut loaded, PpcImportDispatcherTarget::Q3LightGroupNew);

    assert_eq!(light_group, PPC_Q3_OBJECT_BASE);
    assert_eq!(loaded.q3_objects[0].object_type, PPC_Q3_GROUP_TYPE_LIGHT);
    assert_eq!(loaded.q3_objects[0].data_size, 0);

    loaded.cpu.gpr[3] = light_group;
    assert_eq!(
        run_q3_call(&mut loaded, PpcImportDispatcherTarget::Q3ObjectGetType),
        PPC_Q3_GROUP_TYPE_LIGHT
    );

    let ambient_light = PPC_Q3_OBJECT_BASE + PPC_Q3_OBJECT_STRIDE;
    let non_light = PPC_Q3_OBJECT_BASE + PPC_Q3_OBJECT_STRIDE * 2;
    let object_out_ptr = PPC_DATA_BASE + 0x1000;
    loaded.memory.add_region(object_out_ptr, vec![0xaa; 4]);
    loaded.q3_lights.push(PpcQ3LightRecord {
        light: ambient_light,
        light_type: PPC_Q3_LIGHT_TYPE_AMBIENT,
        data: PpcQ3LightData {
            is_on: 1,
            brightness: 0.5,
            color: (1.0, 1.0, 1.0),
        },
        kind: PpcQ3LightKind::Ambient,
    });
    loaded.q3_objects.push(PpcQ3ObjectRecord {
        object: ambient_light,
        kind: PpcQ3ObjectKind::Generic,
        object_type: PPC_Q3_LIGHT_TYPE_AMBIENT,
        source: PpcQ3ObjectSource::default(),
        data_ptr: 0,
        data_size: PPC_Q3_LIGHT_DATA_SIZE,
    });
    loaded.q3_objects.push(PpcQ3ObjectRecord {
        object: non_light,
        kind: PpcQ3ObjectKind::Generic,
        object_type: PPC_Q3_TYPE_TRIMESH,
        source: PpcQ3ObjectSource::default(),
        data_ptr: 0,
        data_size: 0,
    });
    let view = PPC_Q3_OBJECT_BASE + PPC_Q3_OBJECT_STRIDE * 3;
    loaded.q3_objects.push(PpcQ3ObjectRecord {
        object: view,
        kind: PpcQ3ObjectKind::Generic,
        object_type: PPC_Q3_TYPE_VIEW,
        source: PpcQ3ObjectSource::default(),
        data_ptr: 0,
        data_size: 0,
    });

    loaded.cpu.gpr[3] = light_group;
    loaded.cpu.gpr[4] = ambient_light;
    assert_eq!(
        run_q3_call(&mut loaded, PpcImportDispatcherTarget::Q3GroupAddObject),
        1
    );
    assert_eq!(
        loaded.q3_group_memberships,
        vec![PpcQ3GroupMembershipRecord {
            group: light_group,
            object: ambient_light,
            before: None,
        }]
    );

    loaded.cpu.gpr[3] = light_group;
    loaded.cpu.gpr[4] = non_light;
    assert_eq!(
        run_q3_call(&mut loaded, PpcImportDispatcherTarget::Q3GroupAddObject),
        0
    );
    assert_eq!(loaded.q3_group_memberships.len(), 1);

    loaded
        .memory
        .write_u32_be(object_out_ptr, 0xeeee_eeee)
        .unwrap();
    loaded.q3_error_state = PpcQ3ErrorState::default();
    loaded.q3_group_memberships[0].object = non_light;
    loaded.cpu.gpr[3] = light_group;
    loaded.cpu.gpr[4] = 1;
    loaded.cpu.gpr[5] = object_out_ptr;
    assert_eq!(
        run_q3_call(
            &mut loaded,
            PpcImportDispatcherTarget::Q3GroupGetPositionObject
        ),
        0
    );
    assert_eq!(loaded.memory.read_u32_be(object_out_ptr), Some(0xeeee_eeee));
    assert_eq!(
        ppc_q3_object_reference_count(&loaded.q3_objects, &loaded.q3_object_refs, non_light),
        Some(1)
    );
    assert_eq!(
        loaded.q3_error_state.first_error,
        PPC_Q3_ERROR_INVALID_OBJECT_PARAMETER
    );
    loaded.q3_group_memberships[0].object = ambient_light;
    loaded.q3_error_state = PpcQ3ErrorState::default();

    loaded.cpu.gpr[3] = view;
    loaded.cpu.gpr[4] = light_group;
    assert_eq!(
        run_q3_call(&mut loaded, PpcImportDispatcherTarget::Q3ViewSetLightGroup),
        1
    );
    assert_eq!(loaded.q3_views[0].light_group, light_group);

    loaded.cpu.gpr[3] = view;
    loaded.cpu.gpr[4] = non_light;
    assert_eq!(
        run_q3_call(&mut loaded, PpcImportDispatcherTarget::Q3ViewSetLightGroup),
        0
    );
    assert_eq!(loaded.q3_views[0].light_group, light_group);
}

#[test]
fn hle_import_runner_retains_q3_view_light_group_until_view_dispose() {
    fn run_q3_call(loaded: &mut PpcLoadedApp, target: PpcImportDispatcherTarget) -> u32 {
        loaded.cpu.pc = loaded.entry_pc;
        loaded.cpu.lr = PPC_HALT_PC;
        loaded.imports[0].dispatcher_target = target;
        let probe = loaded.run_with_hle_imports(64);
        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        loaded.cpu.gpr[3]
    }

    let pef = synthetic_pef_with_library_import(b"QuickDraw\xaa 3D", b"Q3View_SetLightGroup");
    let mut loaded = load_pef_application(&pef).unwrap();
    let view = PPC_Q3_OBJECT_BASE;
    let light_group = PPC_Q3_OBJECT_BASE + PPC_Q3_OBJECT_STRIDE;
    loaded.q3_objects.push(PpcQ3ObjectRecord {
        object: view,
        kind: PpcQ3ObjectKind::Generic,
        object_type: PPC_Q3_TYPE_VIEW,
        source: PpcQ3ObjectSource::default(),
        data_ptr: 0,
        data_size: 0,
    });
    loaded.q3_objects.push(PpcQ3ObjectRecord {
        object: light_group,
        kind: PpcQ3ObjectKind::Generic,
        object_type: PPC_Q3_GROUP_TYPE_LIGHT,
        source: PpcQ3ObjectSource::default(),
        data_ptr: 0,
        data_size: 0,
    });

    loaded.cpu.gpr[3] = view;
    loaded.cpu.gpr[4] = light_group;
    assert_eq!(
        run_q3_call(&mut loaded, PpcImportDispatcherTarget::Q3ViewSetLightGroup),
        1
    );
    assert_eq!(
        ppc_q3_object_reference_count(&loaded.q3_objects, &loaded.q3_object_refs, light_group),
        Some(2)
    );

    loaded.cpu.gpr[3] = light_group;
    assert_eq!(
        run_q3_call(&mut loaded, PpcImportDispatcherTarget::Q3ObjectDispose),
        1
    );
    assert!(ppc_q3_object_exists(&loaded.q3_objects, light_group));
    assert_eq!(
        ppc_q3_object_reference_count(&loaded.q3_objects, &loaded.q3_object_refs, light_group),
        Some(1)
    );
    assert_eq!(loaded.q3_views[0].light_group, light_group);

    loaded.cpu.gpr[3] = view;
    assert_eq!(
        run_q3_call(&mut loaded, PpcImportDispatcherTarget::Q3ObjectDispose),
        1
    );
    assert!(loaded.q3_objects.is_empty());
    assert!(loaded.q3_object_refs.is_empty());
    assert!(loaded.q3_views.is_empty());
}

#[test]
fn hle_import_runner_retains_q3_view_owned_slots_until_view_dispose() {
    fn run_q3_call(loaded: &mut PpcLoadedApp, target: PpcImportDispatcherTarget) -> u32 {
        loaded.cpu.pc = loaded.entry_pc;
        loaded.cpu.lr = PPC_HALT_PC;
        loaded.imports[0].dispatcher_target = target;
        let probe = loaded.run_with_hle_imports(64);
        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        loaded.cpu.gpr[3]
    }

    let pef = synthetic_pef_with_library_import(b"QuickDraw\xaa 3D", b"Q3View_SetRenderer");
    let mut loaded = load_pef_application(&pef).unwrap();
    let view = PPC_Q3_OBJECT_BASE;
    let renderer = PPC_Q3_OBJECT_BASE + PPC_Q3_OBJECT_STRIDE;
    let draw_context = PPC_Q3_OBJECT_BASE + PPC_Q3_OBJECT_STRIDE * 2;
    let camera = PPC_Q3_OBJECT_BASE + PPC_Q3_OBJECT_STRIDE * 3;
    let replacement_renderer = PPC_Q3_OBJECT_BASE + PPC_Q3_OBJECT_STRIDE * 4;
    let light_group = PPC_Q3_OBJECT_BASE + PPC_Q3_OBJECT_STRIDE * 5;
    loaded.q3_objects.extend([
        PpcQ3ObjectRecord {
            object: view,
            kind: PpcQ3ObjectKind::Generic,
            object_type: PPC_Q3_TYPE_VIEW,
            source: PpcQ3ObjectSource::default(),
            data_ptr: 0,
            data_size: 0,
        },
        PpcQ3ObjectRecord {
            object: renderer,
            kind: PpcQ3ObjectKind::Generic,
            object_type: PPC_Q3_RENDERER_TYPE_INTERACTIVE,
            source: PpcQ3ObjectSource::default(),
            data_ptr: 0,
            data_size: 0,
        },
        PpcQ3ObjectRecord {
            object: draw_context,
            kind: PpcQ3ObjectKind::Generic,
            object_type: PPC_Q3_DRAW_CONTEXT_TYPE_PIXMAP,
            source: PpcQ3ObjectSource::default(),
            data_ptr: 0,
            data_size: PPC_Q3_PIXMAP_DRAW_CONTEXT_DATA_SIZE,
        },
        PpcQ3ObjectRecord {
            object: camera,
            kind: PpcQ3ObjectKind::Generic,
            object_type: PPC_Q3_CAMERA_TYPE_VIEW_ANGLE_ASPECT,
            source: PpcQ3ObjectSource::default(),
            data_ptr: 0,
            data_size: PPC_Q3_VIEW_ANGLE_ASPECT_CAMERA_DATA_SIZE,
        },
        PpcQ3ObjectRecord {
            object: replacement_renderer,
            kind: PpcQ3ObjectKind::Generic,
            object_type: PPC_Q3_RENDERER_TYPE_INTERACTIVE,
            source: PpcQ3ObjectSource::default(),
            data_ptr: 0,
            data_size: 0,
        },
        PpcQ3ObjectRecord {
            object: light_group,
            kind: PpcQ3ObjectKind::Generic,
            object_type: PPC_Q3_GROUP_TYPE_LIGHT,
            source: PpcQ3ObjectSource::default(),
            data_ptr: 0,
            data_size: 0,
        },
    ]);

    loaded.cpu.gpr[3] = view;
    loaded.cpu.gpr[4] = renderer;
    assert_eq!(
        run_q3_call(&mut loaded, PpcImportDispatcherTarget::Q3ViewSetRenderer),
        1
    );
    loaded.cpu.gpr[3] = view;
    loaded.cpu.gpr[4] = draw_context;
    assert_eq!(
        run_q3_call(&mut loaded, PpcImportDispatcherTarget::Q3ViewSetDrawContext),
        1
    );
    loaded.cpu.gpr[3] = view;
    loaded.cpu.gpr[4] = camera;
    assert_eq!(
        run_q3_call(&mut loaded, PpcImportDispatcherTarget::Q3ViewSetCamera),
        1
    );
    loaded.cpu.gpr[3] = view;
    loaded.cpu.gpr[4] = light_group;
    assert_eq!(
        run_q3_call(&mut loaded, PpcImportDispatcherTarget::Q3ViewSetLightGroup),
        1
    );

    assert_eq!(loaded.q3_views[0].renderer, renderer);
    assert_eq!(loaded.q3_views[0].draw_context, draw_context);
    assert_eq!(loaded.q3_views[0].camera, camera);
    assert_eq!(loaded.q3_views[0].light_group, light_group);
    for object in [renderer, draw_context, camera, light_group] {
        assert_eq!(
            ppc_q3_object_reference_count(&loaded.q3_objects, &loaded.q3_object_refs, object),
            Some(2)
        );
    }

    let renderer_out_ptr = PPC_DATA_BASE + 0x1000;
    let draw_context_out_ptr = renderer_out_ptr + 4;
    let camera_out_ptr = renderer_out_ptr + 8;
    let light_group_out_ptr = renderer_out_ptr + 12;
    loaded.memory.add_region(renderer_out_ptr, vec![0; 16]);
    loaded.cpu.gpr[3] = view;
    loaded.cpu.gpr[4] = renderer_out_ptr;
    assert_eq!(
        run_q3_call(&mut loaded, PpcImportDispatcherTarget::Q3ViewGetRenderer),
        1
    );
    assert_eq!(loaded.memory.read_u32_be(renderer_out_ptr), Some(renderer));
    assert_eq!(
        ppc_q3_object_reference_count(&loaded.q3_objects, &loaded.q3_object_refs, renderer),
        Some(3)
    );
    loaded.cpu.gpr[3] = view;
    loaded.cpu.gpr[4] = draw_context_out_ptr;
    assert_eq!(
        run_q3_call(&mut loaded, PpcImportDispatcherTarget::Q3ViewGetDrawContext),
        1
    );
    assert_eq!(
        loaded.memory.read_u32_be(draw_context_out_ptr),
        Some(draw_context)
    );
    assert_eq!(
        ppc_q3_object_reference_count(&loaded.q3_objects, &loaded.q3_object_refs, draw_context),
        Some(3)
    );
    loaded.cpu.gpr[3] = view;
    loaded.cpu.gpr[4] = camera_out_ptr;
    assert_eq!(
        run_q3_call(&mut loaded, PpcImportDispatcherTarget::Q3ViewGetCamera),
        1
    );
    assert_eq!(loaded.memory.read_u32_be(camera_out_ptr), Some(camera));
    assert_eq!(
        ppc_q3_object_reference_count(&loaded.q3_objects, &loaded.q3_object_refs, camera),
        Some(3)
    );
    loaded.cpu.gpr[3] = view;
    loaded.cpu.gpr[4] = light_group_out_ptr;
    assert_eq!(
        run_q3_call(&mut loaded, PpcImportDispatcherTarget::Q3ViewGetLightGroup),
        1
    );
    assert_eq!(
        loaded.memory.read_u32_be(light_group_out_ptr),
        Some(light_group)
    );
    assert_eq!(
        ppc_q3_object_reference_count(&loaded.q3_objects, &loaded.q3_object_refs, light_group),
        Some(3)
    );
    loaded.cpu.gpr[3] = renderer;
    assert_eq!(
        run_q3_call(&mut loaded, PpcImportDispatcherTarget::Q3ObjectDispose),
        1
    );
    assert_eq!(
        ppc_q3_object_reference_count(&loaded.q3_objects, &loaded.q3_object_refs, renderer),
        Some(2)
    );
    for object in [draw_context, camera, light_group] {
        loaded.cpu.gpr[3] = object;
        assert_eq!(
            run_q3_call(&mut loaded, PpcImportDispatcherTarget::Q3ObjectDispose),
            1
        );
        assert_eq!(
            ppc_q3_object_reference_count(&loaded.q3_objects, &loaded.q3_object_refs, object),
            Some(2)
        );
    }

    loaded.cpu.gpr[3] = view;
    loaded.cpu.gpr[4] = camera;
    assert_eq!(
        run_q3_call(&mut loaded, PpcImportDispatcherTarget::Q3ViewSetRenderer),
        0
    );
    loaded.cpu.gpr[3] = view;
    loaded.cpu.gpr[4] = renderer;
    assert_eq!(
        run_q3_call(&mut loaded, PpcImportDispatcherTarget::Q3ViewSetDrawContext),
        0
    );
    loaded.cpu.gpr[3] = view;
    loaded.cpu.gpr[4] = draw_context;
    assert_eq!(
        run_q3_call(&mut loaded, PpcImportDispatcherTarget::Q3ViewSetCamera),
        0
    );
    loaded.cpu.gpr[3] = view;
    loaded.cpu.gpr[4] = renderer;
    assert_eq!(
        run_q3_call(&mut loaded, PpcImportDispatcherTarget::Q3ViewSetLightGroup),
        0
    );
    assert_eq!(loaded.q3_views[0].renderer, renderer);
    assert_eq!(loaded.q3_views[0].draw_context, draw_context);
    assert_eq!(loaded.q3_views[0].camera, camera);
    assert_eq!(loaded.q3_views[0].light_group, light_group);

    loaded
        .memory
        .write_u32_be(renderer_out_ptr, 0xeeee_eeee)
        .unwrap();
    loaded.q3_error_state = PpcQ3ErrorState::default();
    loaded.q3_views[0].renderer = camera;
    loaded.cpu.gpr[3] = view;
    loaded.cpu.gpr[4] = renderer_out_ptr;
    assert_eq!(
        run_q3_call(&mut loaded, PpcImportDispatcherTarget::Q3ViewGetRenderer),
        0
    );
    assert_eq!(
        loaded.memory.read_u32_be(renderer_out_ptr),
        Some(0xeeee_eeee)
    );
    assert_eq!(
        ppc_q3_object_reference_count(&loaded.q3_objects, &loaded.q3_object_refs, camera),
        Some(2)
    );
    assert_eq!(
        loaded.q3_error_state.first_error,
        PPC_Q3_ERROR_INVALID_OBJECT_PARAMETER
    );
    loaded.q3_views[0].renderer = renderer;
    loaded.q3_error_state = PpcQ3ErrorState::default();

    loaded.cpu.gpr[3] = view;
    loaded.cpu.gpr[4] = replacement_renderer;
    assert_eq!(
        run_q3_call(&mut loaded, PpcImportDispatcherTarget::Q3ViewSetRenderer),
        1
    );
    assert_eq!(loaded.q3_views[0].renderer, replacement_renderer);
    assert_eq!(
        ppc_q3_object_reference_count(&loaded.q3_objects, &loaded.q3_object_refs, renderer),
        Some(1)
    );
    assert_eq!(
        ppc_q3_object_reference_count(
            &loaded.q3_objects,
            &loaded.q3_object_refs,
            replacement_renderer
        ),
        Some(2)
    );
    loaded.cpu.gpr[3] = view;
    loaded.cpu.gpr[4] = renderer;
    assert_eq!(
        run_q3_call(&mut loaded, PpcImportDispatcherTarget::Q3ViewSetRenderer),
        1
    );
    assert_eq!(loaded.q3_views[0].renderer, renderer);
    assert_eq!(
        ppc_q3_object_reference_count(&loaded.q3_objects, &loaded.q3_object_refs, renderer),
        Some(2)
    );
    assert_eq!(
        ppc_q3_object_reference_count(
            &loaded.q3_objects,
            &loaded.q3_object_refs,
            replacement_renderer
        ),
        Some(1)
    );

    loaded.cpu.gpr[3] = renderer;
    assert_eq!(
        run_q3_call(&mut loaded, PpcImportDispatcherTarget::Q3ObjectDispose),
        1
    );
    assert!(ppc_q3_object_exists(&loaded.q3_objects, renderer));
    assert_eq!(
        ppc_q3_object_reference_count(&loaded.q3_objects, &loaded.q3_object_refs, renderer),
        Some(1)
    );
    assert_eq!(loaded.q3_views[0].renderer, renderer);

    loaded.cpu.gpr[3] = view;
    assert_eq!(
        run_q3_call(&mut loaded, PpcImportDispatcherTarget::Q3ObjectDispose),
        1
    );
    assert!(!ppc_q3_object_exists(&loaded.q3_objects, view));
    assert!(!ppc_q3_object_exists(&loaded.q3_objects, renderer));
    assert!(ppc_q3_object_exists(&loaded.q3_objects, draw_context));
    assert!(ppc_q3_object_exists(&loaded.q3_objects, camera));
    assert!(ppc_q3_object_exists(&loaded.q3_objects, light_group));
    assert!(loaded.q3_views.is_empty());
    assert!(loaded.q3_object_refs.is_empty());
}

#[test]
fn hle_import_runner_retains_q3_light_group_members_until_group_dispose() {
    fn run_q3_call(loaded: &mut PpcLoadedApp, target: PpcImportDispatcherTarget) -> u32 {
        loaded.cpu.pc = loaded.entry_pc;
        loaded.cpu.lr = PPC_HALT_PC;
        loaded.imports[0].dispatcher_target = target;
        let probe = loaded.run_with_hle_imports(64);
        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        loaded.cpu.gpr[3]
    }

    let pef = synthetic_pef_with_library_import(b"QuickDraw\xaa 3D", b"Q3Group_AddObject");
    let mut loaded = load_pef_application(&pef).unwrap();
    let light_group = PPC_Q3_OBJECT_BASE;
    let ambient_light = PPC_Q3_OBJECT_BASE + PPC_Q3_OBJECT_STRIDE;
    loaded.q3_objects.push(PpcQ3ObjectRecord {
        object: light_group,
        kind: PpcQ3ObjectKind::Generic,
        object_type: PPC_Q3_GROUP_TYPE_LIGHT,
        source: PpcQ3ObjectSource::default(),
        data_ptr: 0,
        data_size: 0,
    });
    loaded.q3_objects.push(PpcQ3ObjectRecord {
        object: ambient_light,
        kind: PpcQ3ObjectKind::Generic,
        object_type: PPC_Q3_LIGHT_TYPE_AMBIENT,
        source: PpcQ3ObjectSource::default(),
        data_ptr: 0,
        data_size: PPC_Q3_LIGHT_DATA_SIZE,
    });
    loaded.q3_lights.push(PpcQ3LightRecord {
        light: ambient_light,
        light_type: PPC_Q3_LIGHT_TYPE_AMBIENT,
        data: PpcQ3LightData {
            is_on: 1,
            brightness: 0.5,
            color: (1.0, 1.0, 1.0),
        },
        kind: PpcQ3LightKind::Ambient,
    });

    loaded.cpu.gpr[3] = light_group;
    loaded.cpu.gpr[4] = ambient_light;
    assert_eq!(
        run_q3_call(&mut loaded, PpcImportDispatcherTarget::Q3GroupAddObject),
        1
    );
    assert_eq!(
        ppc_q3_object_reference_count(
            &loaded.q3_objects,
            &loaded.q3_object_refs,
            ambient_light
        ),
        Some(2)
    );

    loaded.cpu.gpr[3] = ambient_light;
    assert_eq!(
        run_q3_call(&mut loaded, PpcImportDispatcherTarget::Q3ObjectDispose),
        1
    );
    assert!(ppc_q3_object_exists(&loaded.q3_objects, ambient_light));
    assert_eq!(
        ppc_q3_object_reference_count(
            &loaded.q3_objects,
            &loaded.q3_object_refs,
            ambient_light
        ),
        Some(1)
    );
    assert_eq!(loaded.q3_group_memberships.len(), 1);

    loaded.cpu.gpr[3] = light_group;
    assert_eq!(
        run_q3_call(&mut loaded, PpcImportDispatcherTarget::Q3ObjectDispose),
        1
    );
    assert!(loaded.q3_objects.is_empty());
    assert!(loaded.q3_object_refs.is_empty());
    assert!(loaded.q3_group_memberships.is_empty());
    assert!(loaded.q3_lights.is_empty());
}

#[test]
fn hle_import_runner_releases_q3_group_member_on_position_remove() {
    fn run_q3_call(loaded: &mut PpcLoadedApp, target: PpcImportDispatcherTarget) -> u32 {
        loaded.cpu.pc = loaded.entry_pc;
        loaded.cpu.lr = PPC_HALT_PC;
        loaded.imports[0].dispatcher_target = target;
        let probe = loaded.run_with_hle_imports(64);
        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        loaded.cpu.gpr[3]
    }

    let pef = synthetic_pef_with_library_import(b"QuickDraw\xaa 3D", b"Q3Group_AddObject");
    let mut loaded = load_pef_application(&pef).unwrap();
    let light_group = PPC_Q3_OBJECT_BASE;
    let ambient_light = PPC_Q3_OBJECT_BASE + PPC_Q3_OBJECT_STRIDE;
    loaded.q3_objects.push(PpcQ3ObjectRecord {
        object: light_group,
        kind: PpcQ3ObjectKind::Generic,
        object_type: PPC_Q3_GROUP_TYPE_LIGHT,
        source: PpcQ3ObjectSource::default(),
        data_ptr: 0,
        data_size: 0,
    });
    loaded.q3_objects.push(PpcQ3ObjectRecord {
        object: ambient_light,
        kind: PpcQ3ObjectKind::Generic,
        object_type: PPC_Q3_LIGHT_TYPE_AMBIENT,
        source: PpcQ3ObjectSource::default(),
        data_ptr: 0,
        data_size: PPC_Q3_LIGHT_DATA_SIZE,
    });
    loaded.q3_lights.push(PpcQ3LightRecord {
        light: ambient_light,
        light_type: PPC_Q3_LIGHT_TYPE_AMBIENT,
        data: PpcQ3LightData {
            is_on: 1,
            brightness: 0.5,
            color: (1.0, 1.0, 1.0),
        },
        kind: PpcQ3LightKind::Ambient,
    });

    loaded.cpu.gpr[3] = light_group;
    loaded.cpu.gpr[4] = ambient_light;
    assert_eq!(
        run_q3_call(&mut loaded, PpcImportDispatcherTarget::Q3GroupAddObject),
        1
    );
    loaded.cpu.gpr[3] = ambient_light;
    assert_eq!(
        run_q3_call(&mut loaded, PpcImportDispatcherTarget::Q3ObjectDispose),
        1
    );

    loaded.cpu.gpr[3] = light_group;
    loaded.cpu.gpr[4] = 1;
    assert_eq!(
        run_q3_call(
            &mut loaded,
            PpcImportDispatcherTarget::Q3GroupRemovePosition
        ),
        1
    );
    assert_eq!(loaded.q3_objects.len(), 1);
    assert_eq!(loaded.q3_objects[0].object, light_group);
    assert!(loaded.q3_object_refs.is_empty());
    assert!(loaded.q3_group_memberships.is_empty());
    assert!(loaded.q3_lights.is_empty());
}

#[test]
fn hle_import_runner_handles_q3_group_position_queries() {
    let pef = synthetic_pef_with_library_import(b"QuickDraw\xaa 3D", b"Q3Group_CountObjects");
    let mut loaded = load_pef_application(&pef).unwrap();
    let group = PPC_Q3_OBJECT_BASE;
    let container = PPC_Q3_OBJECT_BASE + PPC_Q3_OBJECT_STRIDE;
    let trimesh = PPC_Q3_OBJECT_BASE + PPC_Q3_OBJECT_STRIDE * 2;
    let scratch_ptr = PPC_DATA_BASE + 0x1000;
    let count_out_ptr = scratch_ptr;
    let position_out_ptr = scratch_ptr + 4;
    let object_out_ptr = scratch_ptr + 8;
    loaded.memory.add_region(scratch_ptr, vec![0xaa; 12]);
    loaded.q3_objects.push(PpcQ3ObjectRecord {
        object: group,
        kind: PpcQ3ObjectKind::Generic,
        object_type: PPC_Q3_GROUP_TYPE_DISPLAY,
        source: PpcQ3ObjectSource::default(),
        data_ptr: 0,
        data_size: 0,
    });
    loaded.q3_objects.push(PpcQ3ObjectRecord {
        object: container,
        kind: PpcQ3ObjectKind::Generic,
        object_type: PPC_Q3_TYPE_CONTAINER,
        source: PpcQ3ObjectSource::default(),
        data_ptr: 0,
        data_size: 0,
    });
    loaded.q3_objects.push(PpcQ3ObjectRecord {
        object: trimesh,
        kind: PpcQ3ObjectKind::Generic,
        object_type: PPC_Q3_TYPE_TRIMESH,
        source: PpcQ3ObjectSource::default(),
        data_ptr: 0,
        data_size: 0,
    });
    loaded
        .q3_group_memberships
        .push(PpcQ3GroupMembershipRecord {
            group,
            object: container,
            before: None,
        });
    loaded
        .q3_group_memberships
        .push(PpcQ3GroupMembershipRecord {
            group,
            object: trimesh,
            before: None,
        });
    loaded.cpu.gpr[3] = group;
    loaded.cpu.gpr[4] = count_out_ptr;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], 1);
    assert_eq!(loaded.memory.read_u32_be(count_out_ptr), Some(2));

    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::Q3GroupGetFirstPosition;
    loaded.cpu.gpr[3] = group;
    loaded.cpu.gpr[4] = position_out_ptr;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], 1);
    assert_eq!(loaded.memory.read_u32_be(position_out_ptr), Some(1));

    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::Q3GroupGetPositionObject;
    loaded.cpu.gpr[3] = group;
    loaded.cpu.gpr[4] = 1;
    loaded.cpu.gpr[5] = object_out_ptr;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], 1);
    assert_eq!(loaded.memory.read_u32_be(object_out_ptr), Some(container));
    assert_eq!(
        ppc_q3_object_reference_count(&loaded.q3_objects, &loaded.q3_object_refs, container),
        Some(2)
    );

    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::Q3GroupGetNextPosition;
    loaded.cpu.gpr[3] = group;
    loaded.cpu.gpr[4] = position_out_ptr;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], 1);
    assert_eq!(loaded.memory.read_u32_be(position_out_ptr), Some(2));

    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.imports[0].dispatcher_target =
        PpcImportDispatcherTarget::Q3GroupGetFirstPositionOfType;
    loaded.cpu.gpr[3] = group;
    loaded.cpu.gpr[4] = PPC_Q3_TYPE_TRIMESH;
    loaded.cpu.gpr[5] = position_out_ptr;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], 1);
    assert_eq!(loaded.memory.read_u32_be(position_out_ptr), Some(2));

    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.imports[0].dispatcher_target =
        PpcImportDispatcherTarget::Q3GroupGetFirstPositionOfType;
    loaded.cpu.gpr[3] = group;
    loaded.cpu.gpr[4] = PPC_Q3_SHAPE_TYPE_GEOMETRY;
    loaded.cpu.gpr[5] = position_out_ptr;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], 1);
    assert_eq!(loaded.memory.read_u32_be(position_out_ptr), Some(2));

    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::Q3GroupGetPositionObject;
    loaded.cpu.gpr[3] = group;
    loaded.cpu.gpr[4] = 2;
    loaded.cpu.gpr[5] = object_out_ptr;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], 1);
    assert_eq!(loaded.memory.read_u32_be(object_out_ptr), Some(trimesh));
    assert_eq!(
        ppc_q3_object_reference_count(&loaded.q3_objects, &loaded.q3_object_refs, trimesh),
        Some(2)
    );

    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::Q3GroupRemovePosition;
    loaded.cpu.gpr[3] = group;
    loaded.cpu.gpr[4] = 2;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], 1);
    assert_eq!(
        loaded.q3_group_memberships,
        vec![PpcQ3GroupMembershipRecord {
            group,
            object: container,
            before: None,
        }]
    );

    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::Q3GroupCountObjects;
    loaded.cpu.gpr[3] = group;
    loaded.cpu.gpr[4] = count_out_ptr;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], 1);
    assert_eq!(loaded.memory.read_u32_be(count_out_ptr), Some(1));

    loaded
        .memory
        .write_u32_be(object_out_ptr, 0xeeee_eeee)
        .unwrap();
    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::Q3GroupGetPositionObject;
    loaded.cpu.gpr[3] = group;
    loaded.cpu.gpr[4] = 99;
    loaded.cpu.gpr[5] = object_out_ptr;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], 0);
    assert_eq!(loaded.memory.read_u32_be(object_out_ptr), Some(0xeeee_eeee));
    assert_eq!(
        loaded.q3_error_state.first_error,
        PPC_Q3_ERROR_INVALID_OBJECT_PARAMETER
    );
}

#[test]
fn hle_import_runner_uses_group_local_q3_positions() {
    let pef =
        synthetic_pef_with_library_import(b"QuickDraw\xaa 3D", b"Q3Group_GetFirstPosition");
    let mut loaded = load_pef_application(&pef).unwrap();
    let parent_group = PPC_Q3_OBJECT_BASE;
    let other_group = PPC_Q3_OBJECT_BASE + PPC_Q3_OBJECT_STRIDE;
    let other_object = PPC_Q3_OBJECT_BASE + PPC_Q3_OBJECT_STRIDE * 2;
    let first_object = PPC_Q3_OBJECT_BASE + PPC_Q3_OBJECT_STRIDE * 3;
    let second_object = PPC_Q3_OBJECT_BASE + PPC_Q3_OBJECT_STRIDE * 4;
    let position_out_ptr = PPC_DATA_BASE + 0x1000;
    let object_out_ptr = PPC_DATA_BASE + 0x1004;
    loaded.memory.add_region(position_out_ptr, vec![0xaa; 8]);
    loaded.q3_objects.extend([
        PpcQ3ObjectRecord {
            object: parent_group,
            kind: PpcQ3ObjectKind::Generic,
            object_type: PPC_Q3_GROUP_TYPE_DISPLAY,
            source: PpcQ3ObjectSource::default(),
            data_ptr: 0,
            data_size: 0,
        },
        PpcQ3ObjectRecord {
            object: other_group,
            kind: PpcQ3ObjectKind::Generic,
            object_type: PPC_Q3_GROUP_TYPE_DISPLAY,
            source: PpcQ3ObjectSource::default(),
            data_ptr: 0,
            data_size: 0,
        },
        PpcQ3ObjectRecord {
            object: other_object,
            kind: PpcQ3ObjectKind::Generic,
            object_type: PPC_Q3_TYPE_TRIMESH,
            source: PpcQ3ObjectSource::default(),
            data_ptr: 0,
            data_size: 0,
        },
        PpcQ3ObjectRecord {
            object: first_object,
            kind: PpcQ3ObjectKind::Generic,
            object_type: PPC_Q3_TYPE_TRIMESH,
            source: PpcQ3ObjectSource::default(),
            data_ptr: 0,
            data_size: 0,
        },
        PpcQ3ObjectRecord {
            object: second_object,
            kind: PpcQ3ObjectKind::Generic,
            object_type: PPC_Q3_TYPE_TRIMESH,
            source: PpcQ3ObjectSource::default(),
            data_ptr: 0,
            data_size: 0,
        },
    ]);
    loaded.q3_group_memberships.extend([
        PpcQ3GroupMembershipRecord {
            group: other_group,
            object: other_object,
            before: None,
        },
        PpcQ3GroupMembershipRecord {
            group: parent_group,
            object: first_object,
            before: None,
        },
        PpcQ3GroupMembershipRecord {
            group: parent_group,
            object: second_object,
            before: None,
        },
    ]);
    loaded.cpu.gpr[3] = parent_group;
    loaded.cpu.gpr[4] = position_out_ptr;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.memory.read_u32_be(position_out_ptr), Some(1));

    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::Q3GroupGetNextPosition;
    loaded.cpu.gpr[3] = parent_group;
    loaded.cpu.gpr[4] = position_out_ptr;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.memory.read_u32_be(position_out_ptr), Some(2));

    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::Q3GroupRemovePosition;
    loaded.cpu.gpr[3] = other_group;
    loaded.cpu.gpr[4] = 1;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], 1);

    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::Q3GroupGetPositionObject;
    loaded.cpu.gpr[3] = parent_group;
    loaded.cpu.gpr[4] = 2;
    loaded.cpu.gpr[5] = object_out_ptr;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], 1);
    assert_eq!(
        loaded.memory.read_u32_be(object_out_ptr),
        Some(second_object)
    );
}

#[test]
fn q3_file_container_geometry_is_mirrored_into_parent_group() {
    let group = PPC_Q3_OBJECT_BASE;
    let matrix = PPC_Q3_OBJECT_BASE + PPC_Q3_OBJECT_STRIDE;
    let container = PPC_Q3_OBJECT_BASE + PPC_Q3_OBJECT_STRIDE * 2;
    let trimesh = PPC_Q3_OBJECT_BASE + PPC_Q3_OBJECT_STRIDE * 3;
    let nested_container = PPC_Q3_OBJECT_BASE + PPC_Q3_OBJECT_STRIDE * 4;
    let nested_trimesh = PPC_Q3_OBJECT_BASE + PPC_Q3_OBJECT_STRIDE * 5;
    let mut q3_group_memberships = vec![
        PpcQ3GroupMembershipRecord {
            group,
            object: matrix,
            before: None,
        },
        PpcQ3GroupMembershipRecord {
            group,
            object: container,
            before: None,
        },
        PpcQ3GroupMembershipRecord {
            group: container,
            object: trimesh,
            before: None,
        },
        PpcQ3GroupMembershipRecord {
            group: container,
            object: nested_container,
            before: None,
        },
        PpcQ3GroupMembershipRecord {
            group: nested_container,
            object: nested_trimesh,
            before: None,
        },
    ];
    let q3_objects = vec![
        PpcQ3ObjectRecord {
            object: group,
            kind: PpcQ3ObjectKind::Generic,
            object_type: PPC_Q3_GROUP_TYPE_DISPLAY,
            source: PpcQ3ObjectSource::default(),
            data_ptr: 0,
            data_size: 0,
        },
        PpcQ3ObjectRecord {
            object: matrix,
            kind: PpcQ3ObjectKind::Generic,
            object_type: PPC_Q3_TRANSFORM_TYPE_MATRIX,
            source: PpcQ3ObjectSource::default(),
            data_ptr: 0,
            data_size: 0,
        },
        PpcQ3ObjectRecord {
            object: container,
            kind: PpcQ3ObjectKind::Generic,
            object_type: PPC_Q3_TYPE_CONTAINER,
            source: PpcQ3ObjectSource::default(),
            data_ptr: 0,
            data_size: 0,
        },
        PpcQ3ObjectRecord {
            object: trimesh,
            kind: PpcQ3ObjectKind::Generic,
            object_type: PPC_Q3_TYPE_TRIMESH,
            source: PpcQ3ObjectSource::default(),
            data_ptr: 0,
            data_size: PPC_Q3_TRIMESH_DATA_SIZE,
        },
        PpcQ3ObjectRecord {
            object: nested_container,
            kind: PpcQ3ObjectKind::Generic,
            object_type: PPC_Q3_TYPE_CONTAINER,
            source: PpcQ3ObjectSource::default(),
            data_ptr: 0,
            data_size: 0,
        },
        PpcQ3ObjectRecord {
            object: nested_trimesh,
            kind: PpcQ3ObjectKind::Generic,
            object_type: PPC_Q3_TYPE_TRIMESH,
            source: PpcQ3ObjectSource::default(),
            data_ptr: 0,
            data_size: PPC_Q3_TRIMESH_DATA_SIZE,
        },
    ];

    assert_eq!(
        ppc_q3_group_visible_objects(&q3_group_memberships, &q3_objects, group),
        vec![matrix, container, trimesh, nested_trimesh]
    );
    ppc_q3_file_mirror_container_geometry_into_group(
        &mut q3_group_memberships,
        &q3_objects,
        group,
        container,
    );

    assert_eq!(
        q3_group_memberships
            .iter()
            .filter(|record| record.group == group)
            .map(|record| record.object)
            .collect::<Vec<_>>(),
        vec![matrix, container, trimesh, nested_trimesh]
    );
    ppc_q3_file_mirror_container_geometry_into_group(
        &mut q3_group_memberships,
        &q3_objects,
        group,
        container,
    );
    assert_eq!(
        q3_group_memberships
            .iter()
            .filter(|record| record.group == group)
            .map(|record| record.object)
            .collect::<Vec<_>>(),
        vec![matrix, container, trimesh, nested_trimesh]
    );
    assert_eq!(
        ppc_q3_group_visible_objects(&q3_group_memberships, &q3_objects, group),
        vec![matrix, container, trimesh, nested_trimesh]
    );
}

#[test]
fn q3_group_positions_for_loaded_object_lists_stay_direct() {
    let group = PPC_Q3_OBJECT_BASE;
    let first_container = PPC_Q3_OBJECT_BASE + PPC_Q3_OBJECT_STRIDE;
    let first_trimesh = PPC_Q3_OBJECT_BASE + PPC_Q3_OBJECT_STRIDE * 2;
    let second_container = PPC_Q3_OBJECT_BASE + PPC_Q3_OBJECT_STRIDE * 3;
    let second_trimesh = PPC_Q3_OBJECT_BASE + PPC_Q3_OBJECT_STRIDE * 4;
    let third_container = PPC_Q3_OBJECT_BASE + PPC_Q3_OBJECT_STRIDE * 5;
    let third_trimesh = PPC_Q3_OBJECT_BASE + PPC_Q3_OBJECT_STRIDE * 6;
    let q3_group_memberships = vec![
        PpcQ3GroupMembershipRecord {
            group,
            object: first_container,
            before: None,
        },
        PpcQ3GroupMembershipRecord {
            group,
            object: second_container,
            before: None,
        },
        PpcQ3GroupMembershipRecord {
            group: first_container,
            object: first_trimesh,
            before: None,
        },
        PpcQ3GroupMembershipRecord {
            group: second_container,
            object: second_trimesh,
            before: None,
        },
        PpcQ3GroupMembershipRecord {
            group,
            object: third_container,
            before: None,
        },
        PpcQ3GroupMembershipRecord {
            group: third_container,
            object: third_trimesh,
            before: None,
        },
    ];
    let file_source = PpcQ3ObjectSource {
        file: PPC_Q3_OBJECT_BASE + PPC_Q3_OBJECT_STRIDE * 7,
        offset: 0,
        parent_group_type: PPC_Q3_TYPE_NONE,
        group_depth: 1,
    };
    let q3_objects = vec![
        PpcQ3ObjectRecord {
            object: group,
            kind: PpcQ3ObjectKind::Generic,
            object_type: PPC_Q3_GROUP_TYPE_DISPLAY,
            source: PpcQ3ObjectSource::default(),
            data_ptr: 0,
            data_size: 0,
        },
        PpcQ3ObjectRecord {
            object: first_container,
            kind: PpcQ3ObjectKind::Generic,
            object_type: PPC_Q3_TYPE_CONTAINER,
            source: file_source,
            data_ptr: 0,
            data_size: 0,
        },
        PpcQ3ObjectRecord {
            object: first_trimesh,
            kind: PpcQ3ObjectKind::Generic,
            object_type: PPC_Q3_TYPE_TRIMESH,
            source: file_source,
            data_ptr: 0,
            data_size: 0,
        },
        PpcQ3ObjectRecord {
            object: second_container,
            kind: PpcQ3ObjectKind::Generic,
            object_type: PPC_Q3_TYPE_CONTAINER,
            source: file_source,
            data_ptr: 0,
            data_size: 0,
        },
        PpcQ3ObjectRecord {
            object: second_trimesh,
            kind: PpcQ3ObjectKind::Generic,
            object_type: PPC_Q3_TYPE_TRIMESH,
            source: file_source,
            data_ptr: 0,
            data_size: 0,
        },
        PpcQ3ObjectRecord {
            object: third_container,
            kind: PpcQ3ObjectKind::Generic,
            object_type: PPC_Q3_TYPE_CONTAINER,
            source: file_source,
            data_ptr: 0,
            data_size: 0,
        },
        PpcQ3ObjectRecord {
            object: third_trimesh,
            kind: PpcQ3ObjectKind::Generic,
            object_type: PPC_Q3_TYPE_TRIMESH,
            source: file_source,
            data_ptr: 0,
            data_size: 0,
        },
    ];

    assert_eq!(
        ppc_q3_group_position_objects(&q3_group_memberships, &q3_objects, &[], group),
        vec![first_container, second_container, third_container]
    );
    assert_eq!(
        ppc_q3_group_visible_object_for_position(
            &q3_group_memberships,
            &q3_objects,
            &[],
            group,
            1
        ),
        Some(first_container)
    );
    assert_eq!(
        ppc_q3_group_visible_object_for_position(
            &q3_group_memberships,
            &q3_objects,
            &[],
            group,
            2
        ),
        Some(second_container)
    );
    assert_eq!(
        ppc_q3_group_visible_object_for_position(
            &q3_group_memberships,
            &q3_objects,
            &[],
            group,
            3
        ),
        Some(third_container)
    );
    assert_eq!(
        ppc_q3_group_visible_objects(&q3_group_memberships, &q3_objects, group),
        vec![
            first_container,
            first_trimesh,
            second_container,
            second_trimesh,
            third_container,
            third_trimesh
        ]
    );
}

#[test]
fn q3_group_positions_for_grouped_3dmf_model_lists_use_top_level_groups() {
    let group = PPC_Q3_OBJECT_BASE;
    let first_group = PPC_Q3_OBJECT_BASE + PPC_Q3_OBJECT_STRIDE;
    let first_a = PPC_Q3_OBJECT_BASE + PPC_Q3_OBJECT_STRIDE * 2;
    let first_b = PPC_Q3_OBJECT_BASE + PPC_Q3_OBJECT_STRIDE * 3;
    let second_group = PPC_Q3_OBJECT_BASE + PPC_Q3_OBJECT_STRIDE * 4;
    let second_a = PPC_Q3_OBJECT_BASE + PPC_Q3_OBJECT_STRIDE * 5;
    let second_b = PPC_Q3_OBJECT_BASE + PPC_Q3_OBJECT_STRIDE * 6;
    let third_group = PPC_Q3_OBJECT_BASE + PPC_Q3_OBJECT_STRIDE * 7;
    let third_a = PPC_Q3_OBJECT_BASE + PPC_Q3_OBJECT_STRIDE * 8;
    let third_b = PPC_Q3_OBJECT_BASE + PPC_Q3_OBJECT_STRIDE * 9;
    let file = PPC_Q3_OBJECT_BASE + PPC_Q3_OBJECT_STRIDE * 10;
    let file_group_source = |offset| PpcQ3ObjectSource {
        file,
        offset,
        parent_group_type: PPC_Q3_GROUP_TYPE_DISPLAY,
        group_depth: 2,
    };
    let file_child_source = |offset| PpcQ3ObjectSource {
        file,
        offset,
        parent_group_type: PPC_Q3_GROUP_TYPE_DISPLAY,
        group_depth: 2,
    };
    let q3_group_memberships = vec![
        PpcQ3GroupMembershipRecord {
            group,
            object: first_a,
            before: None,
        },
        PpcQ3GroupMembershipRecord {
            group,
            object: first_b,
            before: None,
        },
        PpcQ3GroupMembershipRecord {
            group,
            object: second_a,
            before: None,
        },
        PpcQ3GroupMembershipRecord {
            group,
            object: second_b,
            before: None,
        },
        PpcQ3GroupMembershipRecord {
            group,
            object: third_a,
            before: None,
        },
        PpcQ3GroupMembershipRecord {
            group,
            object: third_b,
            before: None,
        },
        PpcQ3GroupMembershipRecord {
            group: first_group,
            object: first_a,
            before: None,
        },
        PpcQ3GroupMembershipRecord {
            group: first_group,
            object: first_b,
            before: None,
        },
        PpcQ3GroupMembershipRecord {
            group: second_group,
            object: second_a,
            before: None,
        },
        PpcQ3GroupMembershipRecord {
            group: second_group,
            object: second_b,
            before: None,
        },
        PpcQ3GroupMembershipRecord {
            group: third_group,
            object: third_a,
            before: None,
        },
        PpcQ3GroupMembershipRecord {
            group: third_group,
            object: third_b,
            before: None,
        },
    ];
    let q3_file_groups = vec![
        PpcQ3FileGroupRecord {
            file,
            offset: 0x100,
            group: first_group,
            group_type: PPC_Q3_GROUP_TYPE_DISPLAY,
            parent_group: 0,
            group_depth: 2,
        },
        PpcQ3FileGroupRecord {
            file,
            offset: 0x500,
            group: second_group,
            group_type: PPC_Q3_GROUP_TYPE_DISPLAY,
            parent_group: 0,
            group_depth: 2,
        },
        PpcQ3FileGroupRecord {
            file,
            offset: 0x900,
            group: third_group,
            group_type: PPC_Q3_GROUP_TYPE_DISPLAY,
            parent_group: 0,
            group_depth: 2,
        },
    ];
    let q3_objects = vec![
        PpcQ3ObjectRecord {
            object: group,
            kind: PpcQ3ObjectKind::Generic,
            object_type: PPC_Q3_GROUP_TYPE_DISPLAY,
            source: PpcQ3ObjectSource::default(),
            data_ptr: 0,
            data_size: 0,
        },
        PpcQ3ObjectRecord {
            object: first_group,
            kind: PpcQ3ObjectKind::Generic,
            object_type: PPC_Q3_GROUP_TYPE_DISPLAY,
            source: file_group_source(0x100),
            data_ptr: 0,
            data_size: 16,
        },
        PpcQ3ObjectRecord {
            object: second_group,
            kind: PpcQ3ObjectKind::Generic,
            object_type: PPC_Q3_GROUP_TYPE_DISPLAY,
            source: file_group_source(0x500),
            data_ptr: 0,
            data_size: 16,
        },
        PpcQ3ObjectRecord {
            object: third_group,
            kind: PpcQ3ObjectKind::Generic,
            object_type: PPC_Q3_GROUP_TYPE_DISPLAY,
            source: file_group_source(0x900),
            data_ptr: 0,
            data_size: 16,
        },
        PpcQ3ObjectRecord {
            object: first_a,
            kind: PpcQ3ObjectKind::Generic,
            object_type: PPC_Q3_TYPE_CONTAINER,
            source: file_child_source(0x140),
            data_ptr: 0,
            data_size: 0,
        },
        PpcQ3ObjectRecord {
            object: first_b,
            kind: PpcQ3ObjectKind::Generic,
            object_type: PPC_Q3_TYPE_CONTAINER,
            source: file_child_source(0x300),
            data_ptr: 0,
            data_size: 0,
        },
        PpcQ3ObjectRecord {
            object: second_a,
            kind: PpcQ3ObjectKind::Generic,
            object_type: PPC_Q3_TYPE_CONTAINER,
            source: file_child_source(0x540),
            data_ptr: 0,
            data_size: 0,
        },
        PpcQ3ObjectRecord {
            object: second_b,
            kind: PpcQ3ObjectKind::Generic,
            object_type: PPC_Q3_TYPE_CONTAINER,
            source: file_child_source(0x700),
            data_ptr: 0,
            data_size: 0,
        },
        PpcQ3ObjectRecord {
            object: third_a,
            kind: PpcQ3ObjectKind::Generic,
            object_type: PPC_Q3_TYPE_CONTAINER,
            source: file_child_source(0x940),
            data_ptr: 0,
            data_size: 0,
        },
        PpcQ3ObjectRecord {
            object: third_b,
            kind: PpcQ3ObjectKind::Generic,
            object_type: PPC_Q3_TYPE_CONTAINER,
            source: file_child_source(0xb00),
            data_ptr: 0,
            data_size: 0,
        },
    ];

    assert_eq!(
        ppc_q3_group_position_objects(
            &q3_group_memberships,
            &q3_objects,
            &q3_file_groups,
            group
        ),
        vec![first_group, second_group, third_group]
    );
    assert_eq!(
        ppc_q3_group_visible_object_for_position(
            &q3_group_memberships,
            &q3_objects,
            &q3_file_groups,
            group,
            3
        ),
        Some(third_group)
    );
    assert_eq!(
        ppc_q3_group_visible_objects(&q3_group_memberships, &q3_objects, group),
        vec![first_a, first_b, second_a, second_b, third_a, third_b]
    );
}

#[test]
fn q3_group_positions_preserve_top_level_objects_before_nested_3dmf_groups() {
    let group = PPC_Q3_OBJECT_BASE;
    let root_object = PPC_Q3_OBJECT_BASE + PPC_Q3_OBJECT_STRIDE;
    let nested_group = PPC_Q3_OBJECT_BASE + PPC_Q3_OBJECT_STRIDE * 2;
    let nested_a = PPC_Q3_OBJECT_BASE + PPC_Q3_OBJECT_STRIDE * 3;
    let nested_b = PPC_Q3_OBJECT_BASE + PPC_Q3_OBJECT_STRIDE * 4;
    let file = PPC_Q3_OBJECT_BASE + PPC_Q3_OBJECT_STRIDE * 5;
    let source = |offset, group_depth| PpcQ3ObjectSource {
        file,
        offset,
        parent_group_type: PPC_Q3_GROUP_TYPE_DISPLAY,
        group_depth,
    };
    let q3_group_memberships = vec![
        PpcQ3GroupMembershipRecord {
            group,
            object: root_object,
            before: None,
        },
        PpcQ3GroupMembershipRecord {
            group,
            object: nested_a,
            before: None,
        },
        PpcQ3GroupMembershipRecord {
            group,
            object: nested_b,
            before: None,
        },
    ];
    let q3_file_groups = vec![PpcQ3FileGroupRecord {
        file,
        offset: 0x200,
        group: nested_group,
        group_type: PPC_Q3_GROUP_TYPE_DISPLAY,
        parent_group: 0,
        group_depth: 2,
    }];
    let q3_objects = vec![
        test_q3_object(group, PPC_Q3_GROUP_TYPE_DISPLAY),
        PpcQ3ObjectRecord {
            source: source(0x100, 1),
            ..test_q3_object(root_object, PPC_Q3_TYPE_CONTAINER)
        },
        PpcQ3ObjectRecord {
            source: source(0x200, 2),
            data_size: 16,
            ..test_q3_object(nested_group, PPC_Q3_GROUP_TYPE_DISPLAY)
        },
        PpcQ3ObjectRecord {
            source: source(0x220, 2),
            ..test_q3_object(nested_a, PPC_Q3_TYPE_CONTAINER)
        },
        PpcQ3ObjectRecord {
            source: source(0x240, 2),
            ..test_q3_object(nested_b, PPC_Q3_TYPE_CONTAINER)
        },
    ];

    assert_eq!(
        ppc_q3_group_position_objects(
            &q3_group_memberships,
            &q3_objects,
            &q3_file_groups,
            group,
        ),
        vec![root_object, nested_group]
    );
}

#[test]
fn hle_import_runner_tracks_q3_view_slots_and_render_state() {
    let view = PPC_Q3_OBJECT_BASE;
    let renderer = PPC_Q3_OBJECT_BASE + PPC_Q3_OBJECT_STRIDE;
    let pef = synthetic_pef_with_library_import(b"QuickDraw\xaa 3D", b"Q3View_SetRenderer");
    let mut loaded = load_pef_application(&pef).unwrap();
    let renderer_out_ptr = PPC_DATA_BASE + 0x1000;
    loaded.memory.add_region(renderer_out_ptr, vec![0; 4]);
    loaded.q3_objects.extend([
        test_q3_object(view, PPC_Q3_TYPE_VIEW),
        test_q3_object(renderer, PPC_Q3_RENDERER_TYPE_GENERIC),
    ]);
    loaded.cpu.gpr[3] = view;
    loaded.cpu.gpr[4] = renderer;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], 1);
    assert_eq!(loaded.q3_views.len(), 1);
    assert_eq!(loaded.q3_views[0].renderer, renderer);

    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::Q3ViewGetRenderer;
    loaded.cpu.gpr[3] = view;
    loaded.cpu.gpr[4] = renderer_out_ptr;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], 1);
    assert_eq!(loaded.memory.read_u32_be(renderer_out_ptr), Some(renderer));

    let unknown_renderer = renderer + PPC_Q3_OBJECT_STRIDE;
    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::Q3ViewSetRenderer;
    loaded.cpu.gpr[3] = view;
    loaded.cpu.gpr[4] = unknown_renderer;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], 0);
    assert_eq!(loaded.q3_views[0].renderer, renderer);
    assert_eq!(
        loaded.q3_error_state.first_error,
        PPC_Q3_ERROR_INVALID_OBJECT_PARAMETER
    );

    let pef = synthetic_pef_with_library_import(b"QuickDraw\xaa 3D", b"Q3View_StartRendering");
    let mut loaded = load_pef_application(&pef).unwrap();
    loaded
        .q3_objects
        .push(test_q3_object(view, PPC_Q3_TYPE_VIEW));
    loaded.cpu.gpr[3] = view;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], 1);
    assert_eq!(loaded.q3_views[0].rendering_depth, 1);
    assert!(!loaded.q3_views[0].cancelled);

    let unknown_view = view + PPC_Q3_OBJECT_STRIDE;
    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.cpu.gpr[3] = unknown_view;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], 0);
    assert_eq!(loaded.q3_views.len(), 1);
    assert_eq!(loaded.q3_views[0].rendering_depth, 1);
    assert_eq!(
        loaded.q3_error_state.first_error,
        PPC_Q3_ERROR_INVALID_OBJECT_PARAMETER
    );

    let pef = synthetic_pef_with_library_import(b"QuickDraw\xaa 3D", b"Q3View_Cancel");
    let mut loaded = load_pef_application(&pef).unwrap();
    loaded
        .q3_objects
        .push(test_q3_object(view, PPC_Q3_TYPE_VIEW));
    loaded.q3_views.push(PpcQ3ViewStateRecord {
        view,
        renderer,
        light_group: 0,
        draw_context: 0,
        camera: 0,
        rendering_depth: 1,
        bounding_box_depth: 1,
        cancelled: false,
    });
    loaded.cpu.gpr[3] = view;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], 1);
    assert!(loaded.q3_views[0].cancelled);
    assert_eq!(loaded.q3_views[0].rendering_depth, 0);
    assert_eq!(loaded.q3_views[0].bounding_box_depth, 0);
}

#[test]
fn hle_import_runner_tracks_q3_view_bounding_box_depth() {
    fn run_view_call(
        loaded: &mut PpcLoadedApp,
        target: PpcImportDispatcherTarget,
        view: u32,
        arg4: u32,
    ) -> u32 {
        loaded.cpu.pc = loaded.entry_pc;
        loaded.cpu.lr = PPC_HALT_PC;
        loaded.imports[0].dispatcher_target = target;
        loaded.cpu.gpr[3] = view;
        loaded.cpu.gpr[4] = arg4;

        let probe = loaded.run_with_hle_imports(64);

        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        loaded.cpu.gpr[3]
    }

    let view = PPC_Q3_OBJECT_BASE;
    let bounding_box_out_ptr = PPC_DATA_BASE + 0x1000;
    let pef =
        synthetic_pef_with_library_import(b"QuickDraw\xaa 3D", b"Q3View_StartBoundingBox");
    let mut loaded = load_pef_application(&pef).unwrap();
    loaded.memory.add_region(
        bounding_box_out_ptr,
        vec![0xaa; PPC_Q3_BOUNDING_BOX_SIZE as usize],
    );
    loaded
        .q3_objects
        .push(test_q3_object(view, PPC_Q3_TYPE_VIEW));

    assert_eq!(
        run_view_call(
            &mut loaded,
            PpcImportDispatcherTarget::Q3ViewStartBoundingBox,
            view,
            0,
        ),
        1
    );
    assert_eq!(loaded.q3_views[0].bounding_box_depth, 1);

    assert_eq!(
        run_view_call(
            &mut loaded,
            PpcImportDispatcherTarget::Q3ViewStartBoundingBox,
            view,
            0,
        ),
        1
    );
    assert_eq!(loaded.q3_views[0].bounding_box_depth, 2);
    loaded.q3_submissions.push(PpcQ3SubmissionRecord {
        view,
        kind: PpcQ3SubmissionKind::Object,
        primary: PPC_Q3_OBJECT_BASE + PPC_Q3_OBJECT_STRIDE,
        secondary: 0,
    });

    assert_eq!(
        run_view_call(
            &mut loaded,
            PpcImportDispatcherTarget::Q3ViewEndBoundingBox,
            view,
            bounding_box_out_ptr,
        ),
        PPC_Q3_VIEW_STATUS_DONE
    );
    assert_eq!(loaded.q3_views[0].bounding_box_depth, 1);
    assert_eq!(loaded.q3_submissions.len(), 1);

    assert_eq!(
        run_view_call(
            &mut loaded,
            PpcImportDispatcherTarget::Q3ViewEndBoundingBox,
            view,
            bounding_box_out_ptr,
        ),
        PPC_Q3_VIEW_STATUS_DONE
    );
    assert_eq!(loaded.q3_views[0].bounding_box_depth, 0);
    assert!(loaded.q3_submissions.is_empty());
    assert_eq!(
        ppc_read_f32_be(&mut loaded.memory, bounding_box_out_ptr),
        Some(-1.0)
    );
    assert_eq!(
        ppc_read_f32_be(
            &mut loaded.memory,
            bounding_box_out_ptr + PPC_Q3_BOUNDING_BOX_MAX_OFFSET
        ),
        Some(1.0)
    );
    assert_eq!(
        loaded
            .memory
            .read_u32_be(bounding_box_out_ptr + PPC_Q3_BOUNDING_BOX_IS_EMPTY_OFFSET),
        Some(0)
    );

    assert_eq!(
        run_view_call(
            &mut loaded,
            PpcImportDispatcherTarget::Q3ViewEndBoundingBox,
            view,
            bounding_box_out_ptr,
        ),
        PPC_Q3_VIEW_STATUS_DONE
    );
    assert_eq!(loaded.q3_views[0].bounding_box_depth, 0);

    assert_eq!(
        run_view_call(
            &mut loaded,
            PpcImportDispatcherTarget::Q3ViewStartBoundingBox,
            0,
            0,
        ),
        0
    );
    assert_eq!(loaded.q3_views.len(), 1);
    assert_eq!(loaded.q3_views[0].bounding_box_depth, 0);

    let unknown_view = view + PPC_Q3_OBJECT_STRIDE * 2;
    assert_eq!(
        run_view_call(
            &mut loaded,
            PpcImportDispatcherTarget::Q3ViewStartBoundingBox,
            unknown_view,
            0,
        ),
        0
    );
    assert_eq!(loaded.q3_views.len(), 1);
    assert_eq!(
        loaded.q3_error_state.first_error,
        PPC_Q3_ERROR_INVALID_OBJECT_PARAMETER
    );
}

#[test]
fn hle_import_runner_queues_completed_q3_frames_at_render_boundaries() {
    let view = PPC_Q3_OBJECT_BASE;
    let stale_trimesh = PPC_Q3_OBJECT_BASE + PPC_Q3_OBJECT_STRIDE;
    let trimesh = PPC_Q3_OBJECT_BASE + PPC_Q3_OBJECT_STRIDE * 2;
    let pef = synthetic_pef_with_library_import(b"QuickDraw\xaa 3D", b"Q3View_StartRendering");
    let mut loaded = load_pef_application(&pef).unwrap();
    loaded
        .q3_objects
        .push(test_q3_object(view, PPC_Q3_TYPE_VIEW));

    loaded.q3_submissions.push(PpcQ3SubmissionRecord {
        view,
        kind: PpcQ3SubmissionKind::TriMesh,
        primary: stale_trimesh,
        secondary: 0,
    });
    loaded
        .q3_submission_transforms
        .push(PpcQ3SubmissionTransformRecord {
            view,
            kind: PpcQ3SubmissionKind::TriMesh,
            primary: stale_trimesh,
            secondary: 0,
            local_to_world: ppc_q3_matrix4x4_identity(),
        });
    loaded
        .q3_submission_materials
        .push(PpcQ3SubmissionMaterialRecord {
            view,
            kind: PpcQ3SubmissionKind::TriMesh,
            primary: stale_trimesh,
            secondary: 0,
            shader: 0,
            illumination_type: PPC_Q3_ILLUMINATION_TYPE_PHONG,
            styles: Vec::new(),
            fog_style: None,
            attributes: Vec::new(),
            shader_uv_transform: None,
            shader_boundary: None,
            texture_shader: None,
            mipmap_texture: None,
        });
    loaded
        .q3_submission_lights
        .push(PpcQ3SubmissionLightRecord {
            view,
            kind: PpcQ3SubmissionKind::TriMesh,
            primary: stale_trimesh,
            secondary: 0,
            light_group: 0,
            lights: Vec::new(),
        });
    loaded.cpu.gpr[3] = view;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], 1);
    assert_eq!(loaded.q3_views[0].rendering_depth, 1);
    assert!(loaded.q3_submissions.is_empty());
    assert!(loaded.q3_submission_transforms.is_empty());
    assert!(loaded.q3_submission_materials.is_empty());
    assert!(loaded.q3_submission_lights.is_empty());
    assert!(loaded.q3_completed_frames.is_empty());

    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::Q3TriMeshSubmit;
    loaded.cpu.gpr[3] = trimesh;
    loaded.cpu.gpr[4] = view;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], 1);
    assert_eq!(
        loaded.q3_submissions,
        vec![PpcQ3SubmissionRecord {
            view,
            kind: PpcQ3SubmissionKind::TriMesh,
            primary: trimesh,
            secondary: 0,
        }]
    );

    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::Q3ViewEndRendering;
    loaded.cpu.gpr[3] = view;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], PPC_Q3_VIEW_STATUS_DONE);
    assert_eq!(loaded.q3_views[0].rendering_depth, 0);
    assert!(loaded.q3_submissions.is_empty());
    assert!(loaded.q3_submission_transforms.is_empty());
    assert!(loaded.q3_submission_materials.is_empty());
    assert!(loaded.q3_submission_lights.is_empty());
    assert_eq!(loaded.q3_completed_frames.len(), 1);
    assert_eq!(
        loaded.q3_completed_frames[0].submissions,
        vec![PpcQ3SubmissionRecord {
            view,
            kind: PpcQ3SubmissionKind::TriMesh,
            primary: trimesh,
            secondary: 0,
        }]
    );
    assert_eq!(loaded.q3_completed_frames[0].submission_transforms.len(), 1);
    assert_eq!(loaded.q3_completed_frames[0].submission_materials.len(), 1);
    assert_eq!(loaded.q3_completed_frames[0].submission_lights.len(), 1);

    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::Q3ViewStartRendering;
    loaded.cpu.gpr[3] = view;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], 1);
    assert_eq!(loaded.q3_views[0].rendering_depth, 1);

    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::Q3TriMeshSubmit;
    loaded.cpu.gpr[3] = stale_trimesh;
    loaded.cpu.gpr[4] = view;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], 1);
    assert_eq!(loaded.q3_submissions.len(), 1);

    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::Q3ViewCancel;
    loaded.cpu.gpr[3] = view;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], 1);
    assert!(loaded.q3_views[0].cancelled);
    assert_eq!(loaded.q3_views[0].rendering_depth, 0);
    assert!(loaded.q3_submissions.is_empty());
    assert!(loaded.q3_submission_transforms.is_empty());
    assert!(loaded.q3_submission_materials.is_empty());
    assert!(loaded.q3_submission_lights.is_empty());
    assert_eq!(loaded.q3_completed_frames.len(), 1);
}

#[test]
fn q3_completed_frame_fast_render_counts_state_only_frames_without_replay() {
    let view = PPC_Q3_OBJECT_BASE;
    let style = PPC_Q3_OBJECT_BASE + PPC_Q3_OBJECT_STRIDE;
    let pef = synthetic_pef_with_library_import(b"QuickDraw\xaa 3D", b"Q3View_StartRendering");
    let mut loaded = load_pef_application(&pef).unwrap();
    loaded.q3_completed_frames = (0..3)
        .map(|_| PpcQ3CompletedFrameRecord {
            view,
            submissions: vec![PpcQ3SubmissionRecord {
                view,
                kind: PpcQ3SubmissionKind::Style,
                primary: style,
                secondary: 0,
            }],
            submission_transforms: Vec::new(),
            submission_materials: Vec::new(),
            submission_lights: Vec::new(),
            retained_trimeshes: Vec::new(),
        })
        .collect();

    let stats = loaded.render_completed_q3_frames_to_front_buffer_fast();

    assert!(loaded.q3_completed_frames.is_empty());
    assert_eq!(stats.frames, 3);
    assert_eq!(stats.commands, 0);
    assert_eq!(stats.vertices, 0);
    assert_eq!(stats.triangles, 0);
    assert_eq!(stats.pixels, 0);
    let front_buffer = loaded.current_front_buffer().unwrap();
    assert_eq!(stats.target_base, Some(front_buffer.base_addr));
    assert_eq!(stats.target_row_bytes, Some(front_buffer.row_bytes));
    assert_eq!(stats.target_width, Some(front_buffer.width));
    assert_eq!(stats.target_height, Some(front_buffer.height));
    assert_eq!(stats.target_depth, Some(front_buffer.depth));
    assert_eq!(stats.target_source, Some("current_gworld"));
    assert_eq!(stats.target_draw_context, None);
    assert_eq!(stats.target_gworld, Some(*loaded.current_gworld));
    assert!(stats.target_consistent);
}

#[test]
fn q3_completed_frame_fast_render_records_state_only_target_metadata() {
    let view = PPC_Q3_OBJECT_BASE;
    let draw_context = PPC_Q3_OBJECT_BASE + PPC_Q3_OBJECT_STRIDE;
    let style = PPC_Q3_OBJECT_BASE + PPC_Q3_OBJECT_STRIDE * 2;
    let front_base = PPC_HEAP_BASE + 0x1000;
    let back_base = PPC_HEAP_BASE + 0x2000;
    let pef = synthetic_pef_with_library_import(b"QuickDraw\xaa 3D", b"Q3View_StartRendering");
    let mut loaded = load_pef_application(&pef).unwrap();
    loaded.memory.add_region(front_base, vec![0; 8 * 8 * 2]);
    loaded.memory.add_region(back_base, vec![0; 8 * 8 * 2]);
    loaded.gworlds = vec![
        PpcGWorldRecord {
            ui_theme: crate::ui_theme::UiThemeId::ClassicSystem7,
            port: PPC_MAIN_GWORLD,
            pixmap_handle: 0,
            pixmap: 0,
            base_addr: front_base,
            gdevice: PPC_MAIN_GDEVICE,
            width: 8,
            height: 8,
            depth: 16,
            row_bytes: 16,
            pixels_locked: false,
            pixels_no_purge: false,
        },
        PpcGWorldRecord {
            ui_theme: crate::ui_theme::UiThemeId::ClassicSystem7,
            port: PPC_DSP_BACK_GWORLD,
            pixmap_handle: 0,
            pixmap: 0,
            base_addr: back_base,
            gdevice: PPC_MAIN_GDEVICE,
            width: 8,
            height: 8,
            depth: 16,
            row_bytes: 16,
            pixels_locked: false,
            pixels_no_purge: false,
        },
    ];
    loaded
        .current_gworld
        .with_mut(|current_gworld| *current_gworld = PPC_MAIN_GWORLD);
    loaded
        .q3_objects
        .push(test_q3_object(view, PPC_Q3_TYPE_VIEW));
    loaded.q3_objects.push(PpcQ3ObjectRecord {
        object: draw_context,
        kind: PpcQ3ObjectKind::Generic,
        object_type: PPC_Q3_DRAW_CONTEXT_TYPE_MACINTOSH,
        source: PpcQ3ObjectSource::default(),
        data_ptr: 0,
        data_size: PPC_Q3_MAC_DRAW_CONTEXT_DATA_SIZE,
    });
    let mut draw_context_data = vec![0; PPC_Q3_MAC_DRAW_CONTEXT_DATA_SIZE as usize];
    draw_context_data[PPC_Q3_MAC_DRAW_CONTEXT_PORT_OFFSET as usize
        ..PPC_Q3_MAC_DRAW_CONTEXT_PORT_OFFSET as usize + 4]
        .copy_from_slice(&PPC_DSP_BACK_GWORLD.to_be_bytes());
    loaded.q3_draw_contexts.push(PpcQ3DrawContextRecord {
        draw_context,
        draw_context_type: PPC_Q3_DRAW_CONTEXT_TYPE_MACINTOSH,
        data: draw_context_data,
    });
    loaded.q3_views.push(PpcQ3ViewStateRecord {
        view,
        renderer: 0,
        light_group: 0,
        draw_context,
        camera: 0,
        rendering_depth: 0,
        bounding_box_depth: 0,
        cancelled: false,
    });
    loaded.q3_completed_frames.push(PpcQ3CompletedFrameRecord {
        view,
        submissions: vec![PpcQ3SubmissionRecord {
            view,
            kind: PpcQ3SubmissionKind::Style,
            primary: style,
            secondary: 0,
        }],
        submission_transforms: Vec::new(),
        submission_materials: Vec::new(),
        submission_lights: Vec::new(),
        retained_trimeshes: Vec::new(),
    });

    let stats = loaded.render_completed_q3_frames_to_front_buffer_fast();

    assert!(loaded.q3_completed_frames.is_empty());
    assert_eq!(stats.frames, 1);
    assert_eq!(stats.commands, 0);
    assert_eq!(stats.vertices, 0);
    assert_eq!(stats.triangles, 0);
    assert_eq!(stats.pixels, 0);
    assert_eq!(stats.target_base, Some(back_base));
    assert_eq!(stats.target_row_bytes, Some(16));
    assert_eq!(stats.target_width, Some(8));
    assert_eq!(stats.target_height, Some(8));
    assert_eq!(stats.target_depth, Some(16));
    assert_eq!(stats.target_source, Some("mac_draw_context"));
    assert_eq!(stats.target_draw_context, Some(draw_context));
    assert_eq!(stats.target_gworld, Some(PPC_DSP_BACK_GWORLD));
    assert!(stats.target_consistent);
}

#[test]
fn q3_end_rendering_aggregates_state_only_completed_frames() {
    let view = PPC_Q3_OBJECT_BASE;
    let style = PPC_Q3_OBJECT_BASE + PPC_Q3_OBJECT_STRIDE;
    let pef = synthetic_pef_with_library_import(b"QuickDraw\xaa 3D", b"Q3View_StartRendering");
    let mut loaded = load_pef_application(&pef).unwrap();
    loaded
        .q3_objects
        .push(test_q3_object(view, PPC_Q3_TYPE_VIEW));
    loaded.q3_views.push(PpcQ3ViewStateRecord {
        view,
        renderer: 0,
        light_group: 0,
        draw_context: 0,
        camera: 0,
        rendering_depth: 1,
        bounding_box_depth: 0,
        cancelled: false,
    });
    loaded.q3_submissions.push(PpcQ3SubmissionRecord {
        view,
        kind: PpcQ3SubmissionKind::Style,
        primary: style,
        secondary: 0,
    });
    loaded
        .q3_submission_transforms
        .push(PpcQ3SubmissionTransformRecord {
            view,
            kind: PpcQ3SubmissionKind::Style,
            primary: style,
            secondary: 0,
            local_to_world: ppc_q3_matrix4x4_identity(),
        });
    loaded
        .q3_submission_materials
        .push(PpcQ3SubmissionMaterialRecord {
            view,
            kind: PpcQ3SubmissionKind::Style,
            primary: style,
            secondary: 0,
            shader: 0,
            illumination_type: PPC_Q3_ILLUMINATION_TYPE_PHONG,
            styles: Vec::new(),
            fog_style: None,
            attributes: Vec::new(),
            shader_uv_transform: None,
            shader_boundary: None,
            texture_shader: None,
            mipmap_texture: None,
        });
    loaded
        .q3_submission_lights
        .push(PpcQ3SubmissionLightRecord {
            view,
            kind: PpcQ3SubmissionKind::Style,
            primary: style,
            secondary: 0,
            light_group: 0,
            lights: Vec::new(),
        });
    loaded.cpu.gpr[3] = view;

    let result = ppc_q3_view_end_rendering(
        &loaded.cpu,
        &mut loaded.q3_views,
        &loaded.q3_objects,
        &mut loaded.q3_submissions,
        &mut loaded.q3_submission_transforms,
        &mut loaded.q3_submission_materials,
        &mut loaded.q3_submission_lights,
        &mut loaded.q3_completed_frames,
        &mut loaded.q3_retained_frames,
        &mut loaded.q3_state_only_completed_frame_batches,
        &loaded.q3_draw_contexts,
        &loaded.gworlds,
        *loaded.current_gworld,
        &mut loaded.q3_error_state,
    );

    assert_eq!(result, PPC_Q3_VIEW_STATUS_DONE);
    assert_eq!(loaded.q3_views[0].rendering_depth, 0);
    assert!(loaded.q3_submissions.is_empty());
    assert!(loaded.q3_submission_transforms.is_empty());
    assert!(loaded.q3_submission_materials.is_empty());
    assert!(loaded.q3_submission_lights.is_empty());
    assert!(loaded.q3_completed_frames.is_empty());
    assert_eq!(loaded.q3_state_only_completed_frame_batches.len(), 1);
    assert_eq!(loaded.q3_state_only_completed_frame_batches[0].frames, 1);
    let front_buffer = loaded.current_front_buffer().unwrap();
    assert_eq!(
        loaded.q3_state_only_completed_frame_batches[0]
            .target
            .map(|target| target.front_buffer),
        Some(front_buffer)
    );

    let stats = loaded.render_completed_q3_frames_to_front_buffer_fast();
    assert_eq!(stats.frames, 1);
    assert_eq!(stats.commands, 0);
    assert_eq!(stats.pixels, 0);
    assert_eq!(stats.target_base, Some(front_buffer.base_addr));
    assert_eq!(stats.target_source, Some("current_gworld"));
    assert!(stats.target_consistent);
    assert!(loaded.q3_state_only_completed_frame_batches.is_empty());
}

#[test]
fn q3_end_rendering_reuses_retained_bounding_geometry_for_state_only_frames() {
    let view = PPC_Q3_OBJECT_BASE;
    let trimesh = PPC_Q3_OBJECT_BASE + PPC_Q3_OBJECT_STRIDE;
    let style = PPC_Q3_OBJECT_BASE + PPC_Q3_OBJECT_STRIDE * 2;
    let pef = synthetic_pef_with_library_import(b"QuickDraw\xaa 3D", b"Q3View_StartRendering");
    let mut loaded = load_pef_application(&pef).unwrap();
    loaded
        .q3_objects
        .push(test_q3_object(view, PPC_Q3_TYPE_VIEW));
    loaded.q3_views.push(PpcQ3ViewStateRecord {
        view,
        renderer: 0,
        light_group: 0,
        draw_context: 0,
        camera: 0,
        rendering_depth: 0,
        bounding_box_depth: 1,
        cancelled: false,
    });
    loaded.q3_submissions.push(PpcQ3SubmissionRecord {
        view,
        kind: PpcQ3SubmissionKind::TriMesh,
        primary: trimesh,
        secondary: 0,
    });
    loaded.cpu.gpr[3] = view;
    loaded.cpu.gpr[4] = PPC_DATA_BASE + 0x1000;
    loaded.memory.add_region(
        loaded.cpu.gpr[4],
        vec![0; PPC_Q3_BOUNDING_BOX_SIZE as usize],
    );

    let result = ppc_q3_view_end_bounding_box(
        &loaded.cpu,
        &mut loaded.memory,
        &mut loaded.q3_views,
        &loaded.q3_objects,
        &mut loaded.q3_submissions,
        &mut loaded.q3_submission_transforms,
        &mut loaded.q3_submission_materials,
        &mut loaded.q3_submission_lights,
        &mut loaded.q3_retained_frames,
        &loaded.q3_trimeshes,
        &mut loaded.q3_error_state,
    );

    assert_eq!(result, PPC_Q3_VIEW_STATUS_DONE);
    assert!(loaded.q3_submissions.is_empty());
    assert_eq!(loaded.q3_retained_frames.len(), 1);
    assert_eq!(
        loaded.q3_retained_frames[0].frame.submissions,
        vec![PpcQ3SubmissionRecord {
            view,
            kind: PpcQ3SubmissionKind::TriMesh,
            primary: trimesh,
            secondary: 0,
        }]
    );

    loaded.q3_views[0].rendering_depth = 1;
    loaded.q3_submissions.push(PpcQ3SubmissionRecord {
        view,
        kind: PpcQ3SubmissionKind::Style,
        primary: style,
        secondary: 0,
    });
    loaded.cpu.gpr[3] = view;

    let result = ppc_q3_view_end_rendering(
        &loaded.cpu,
        &mut loaded.q3_views,
        &loaded.q3_objects,
        &mut loaded.q3_submissions,
        &mut loaded.q3_submission_transforms,
        &mut loaded.q3_submission_materials,
        &mut loaded.q3_submission_lights,
        &mut loaded.q3_completed_frames,
        &mut loaded.q3_retained_frames,
        &mut loaded.q3_state_only_completed_frame_batches,
        &loaded.q3_draw_contexts,
        &loaded.gworlds,
        *loaded.current_gworld,
        &mut loaded.q3_error_state,
    );

    assert_eq!(result, PPC_Q3_VIEW_STATUS_DONE);
    assert!(loaded.q3_submissions.is_empty());
    assert_eq!(loaded.q3_completed_frames.len(), 1);
    assert_eq!(
        loaded.q3_completed_frames[0].submissions[0].kind,
        PpcQ3SubmissionKind::TriMesh
    );
    assert!(loaded.q3_state_only_completed_frame_batches.is_empty());
}

#[test]
fn hle_import_runner_fast_paths_idle_q3_state_only_render_boundaries() {
    let view = PPC_Q3_OBJECT_BASE;
    let style = PPC_Q3_OBJECT_BASE + PPC_Q3_OBJECT_STRIDE;
    let pef = synthetic_pef_with_library_import(b"QuickDraw\xaa 3D", b"Q3View_StartRendering");
    let mut loaded = load_pef_application(&pef).unwrap();
    loaded
        .q3_objects
        .push(test_q3_object(view, PPC_Q3_TYPE_VIEW));
    loaded.q3_views.push(PpcQ3ViewStateRecord {
        view,
        renderer: 0,
        light_group: 0,
        draw_context: 0,
        camera: 0,
        rendering_depth: 0,
        bounding_box_depth: 0,
        cancelled: false,
    });
    loaded.cpu.gpr[3] = view;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], 1);
    assert_eq!(loaded.q3_views[0].rendering_depth, 1);

    loaded.q3_submissions.push(PpcQ3SubmissionRecord {
        view,
        kind: PpcQ3SubmissionKind::Style,
        primary: style,
        secondary: 0,
    });
    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::Q3ViewEndRendering;
    loaded.cpu.gpr[3] = view;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], PPC_Q3_VIEW_STATUS_DONE);
    assert_eq!(loaded.q3_views[0].rendering_depth, 0);
    assert!(loaded.q3_submissions.is_empty());
    assert!(loaded.q3_completed_frames.is_empty());
    assert_eq!(loaded.q3_state_only_completed_frame_batches.len(), 1);
    assert_eq!(loaded.q3_state_only_completed_frame_batches[0].frames, 1);

    loaded.q3_views[0].rendering_depth = 1;
    loaded.q3_submissions.push(PpcQ3SubmissionRecord {
        view,
        kind: PpcQ3SubmissionKind::Style,
        primary: style,
        secondary: 0,
    });
    loaded.q3_retained_frames.push(PpcQ3RetainedFrameRecord {
        view,
        frame: PpcQ3CompletedFrameRecord {
            view,
            submissions: vec![PpcQ3SubmissionRecord {
                view,
                kind: PpcQ3SubmissionKind::TriMesh,
                primary: style + PPC_Q3_OBJECT_STRIDE,
                secondary: 0,
            }],
            submission_transforms: Vec::new(),
            submission_materials: Vec::new(),
            submission_lights: Vec::new(),
            retained_trimeshes: Vec::new(),
        },
    });
    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.cpu.gpr[3] = view;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert!(
        matches!(
            probe.result,
            PpcRunResult::Halted {
                cycles,
                ..
            } | PpcRunResult::CycleLimit {
                cycles,
                ..
            } if cycles >= PPC_Q3_IDLE_STATE_ONLY_FRAME_EXTRA_CYCLES
        ),
        "{:?}",
        probe.result
    );
    assert_eq!(loaded.cpu.gpr[3], PPC_Q3_VIEW_STATUS_DONE);
    assert!(loaded.q3_submissions.is_empty());
    assert_eq!(loaded.q3_completed_frames.len(), 1);
    assert!(loaded.q3_retained_frames.is_empty());

    loaded.q3_views[0].rendering_depth = 1;
    loaded.q3_submissions.push(PpcQ3SubmissionRecord {
        view,
        kind: PpcQ3SubmissionKind::Style,
        primary: style,
        secondary: 0,
    });
    loaded.set_input_snapshot(PpcInputSnapshot {
        key_map: {
            let mut key_map = [0; 16];
            key_map[(PPC_KEY_SPACE / 8) as usize] |= 1u8 << (PPC_KEY_SPACE % 8);
            key_map
        },
        ..PpcInputSnapshot::default()
    });
    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.cpu.gpr[3] = view;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert!(
        matches!(
            probe.result,
            PpcRunResult::Halted {
                cycles,
                ..
            } | PpcRunResult::CycleLimit {
                cycles,
                ..
            } if cycles < PPC_Q3_IDLE_STATE_ONLY_FRAME_EXTRA_CYCLES
        ),
        "{:?}",
        probe.result
    );
    assert_eq!(loaded.cpu.gpr[3], PPC_Q3_VIEW_STATUS_DONE);
    assert_eq!(loaded.q3_state_only_completed_frame_batches.len(), 1);
    assert_eq!(loaded.q3_state_only_completed_frame_batches[0].frames, 2);
}

#[test]
fn q3_hot_import_cost_keeps_reference_frames_within_a_guest_vbl() {
    const REFERENCE_Q3_CALLS_PER_FRAME: u64 = 250;
    const REFERENCE_CYCLES_PER_VBL: u64 = 416_000;

    assert!(
        REFERENCE_Q3_CALLS_PER_FRAME * PPC_Q3_HOT_IMPORT_EXTRA_CYCLES
            <= REFERENCE_CYCLES_PER_VBL,
        "synthetic QD3D costs must leave part of the frame for interpreted game code"
    );
}

#[test]
fn q3_trimesh_get_data_cache_restores_guest_arrays_in_place() {
    let source_points_ptr = PPC_DATA_BASE + 0x1000;
    let cached_points_ptr = PPC_DATA_BASE + 0x1040;
    let mut memory = PpcSectionMem::new();
    memory.add_region(source_points_ptr, vec![0; 0x80]);
    let mut source = vec![0; PPC_Q3_TRIMESH_DATA_SIZE as usize];
    let mut cached = source.clone();
    ppc_q3_trimesh_header_put_u32(&mut source, PPC_Q3_TRIMESH_NUM_POINTS_OFFSET, 2).unwrap();
    ppc_q3_trimesh_header_put_u32(&mut source, PPC_Q3_TRIMESH_POINTS_OFFSET, source_points_ptr)
        .unwrap();
    ppc_q3_trimesh_header_put_u32(&mut cached, PPC_Q3_TRIMESH_NUM_POINTS_OFFSET, 2).unwrap();
    ppc_q3_trimesh_header_put_u32(&mut cached, PPC_Q3_TRIMESH_POINTS_OFFSET, cached_points_ptr)
        .unwrap();
    let point_bytes: Vec<u8> = [1.0f32, 2.0, 3.0, -4.0, -5.0, -6.0]
        .into_iter()
        .flat_map(|value| value.to_bits().to_be_bytes())
        .collect();
    for (offset, byte) in point_bytes.iter().copied().enumerate() {
        memory
            .write_u8(source_points_ptr + u32::try_from(offset).unwrap(), byte)
            .unwrap();
        memory
            .write_u8(cached_points_ptr + u32::try_from(offset).unwrap(), 0xff)
            .unwrap();
    }

    let refreshed =
        ppc_q3_trimesh_refresh_get_data_arrays(&mut memory, &source, &cached).unwrap();

    assert_eq!(
        ppc_q3_trimesh_header_u32(&refreshed, PPC_Q3_TRIMESH_POINTS_OFFSET),
        Some(cached_points_ptr)
    );
    for (offset, byte) in point_bytes.iter().copied().enumerate() {
        assert_eq!(
            memory.read_u8(cached_points_ptr + u32::try_from(offset).unwrap()),
            Some(byte)
        );
    }
}

#[test]
fn hle_import_runner_q3_trimesh_get_data_always_returns_independent_arrays() {
    let pef = synthetic_pef_with_library_import(b"QuickDraw\xaa 3D", b"Q3TriMesh_New");
    let mut loaded = load_pef_application(&pef).unwrap();
    let data_ptr = PPC_DATA_BASE + 0x1000;
    let source_points_ptr = data_ptr + 0x80;
    let output_ptr = data_ptr + 0xc0;
    loaded.memory.add_region(data_ptr, vec![0; 0x180]);
    loaded
        .memory
        .write_u32_be(data_ptr + PPC_Q3_TRIMESH_NUM_POINTS_OFFSET, 2)
        .unwrap();
    loaded
        .memory
        .write_u32_be(data_ptr + PPC_Q3_TRIMESH_POINTS_OFFSET, source_points_ptr)
        .unwrap();
    let point_bytes: Vec<u8> = [1.0f32, 2.0, 3.0, -4.0, -5.0, -6.0]
        .into_iter()
        .flat_map(|value| value.to_bits().to_be_bytes())
        .collect();
    loaded
        .memory
        .write_bytes(source_points_ptr, &point_bytes)
        .unwrap();
    loaded.cpu.gpr[3] = data_ptr;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    let trimesh = loaded.cpu.gpr[3];
    let stored_points_ptr =
        ppc_q3_trimesh_header_u32(&loaded.q3_trimeshes[0].data, PPC_Q3_TRIMESH_POINTS_OFFSET)
            .unwrap();
    assert_ne!(stored_points_ptr, source_points_ptr);

    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::Q3TriMeshGetData;
    loaded.cpu.gpr[3] = trimesh;
    loaded.cpu.gpr[4] = output_ptr;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], 1);
    let output_points_ptr = loaded
        .memory
        .read_u32_be(output_ptr + PPC_Q3_TRIMESH_POINTS_OFFSET)
        .unwrap();
    assert_ne!(output_points_ptr, stored_points_ptr);
    assert_eq!(
        ppc_q3_read_bytes(&mut loaded.memory, output_points_ptr, 24),
        Some(point_bytes.clone())
    );
    loaded.memory.write_u8(output_points_ptr, 0xff).unwrap();
    assert_eq!(
        loaded.memory.read_u8(stored_points_ptr),
        Some(point_bytes[0])
    );

    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::Q3TriMeshEmptyData;
    loaded.cpu.gpr[3] = output_ptr;
    let heap_cursor_before_reuse = loaded.heap_cursor();

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(loaded.cpu.gpr[3], 1);
    assert_eq!(
        loaded
            .memory
            .read_u32_be(output_ptr + PPC_Q3_TRIMESH_POINTS_OFFSET),
        Some(0)
    );
    loaded.memory.write_u8(stored_points_ptr, 0x7f).unwrap();

    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::Q3TriMeshGetData;
    loaded.cpu.gpr[3] = trimesh;
    loaded.cpu.gpr[4] = output_ptr;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(loaded.cpu.gpr[3], 1);
    assert_eq!(loaded.heap_cursor(), heap_cursor_before_reuse);
    assert_eq!(
        loaded
            .memory
            .read_u32_be(output_ptr + PPC_Q3_TRIMESH_POINTS_OFFSET),
        Some(output_points_ptr)
    );
    assert_eq!(loaded.memory.read_u8(output_points_ptr), Some(0x7f));
}

#[test]
fn hle_import_runner_handles_q3_trimesh_data_state() {
    let pef = synthetic_pef_with_library_import(b"QuickDraw\xaa 3D", b"Q3TriMesh_New");
    let mut loaded = load_pef_application(&pef).unwrap();
    let scratch_ptr = PPC_DATA_BASE + 0x1000;
    let data1_ptr = scratch_ptr;
    let data2_ptr = scratch_ptr + 0x80;
    let output_ptr = scratch_ptr + 0x100;
    loaded.memory.add_region(scratch_ptr, vec![0; 0x300]);
    let data1: Vec<u8> = (0..PPC_Q3_TRIMESH_DATA_SIZE)
        .map(|offset| (offset as u8).wrapping_add(1))
        .collect();
    let data2: Vec<u8> = (0..PPC_Q3_TRIMESH_DATA_SIZE)
        .map(|offset| 0x80u8.wrapping_add(offset as u8))
        .collect();
    for offset in 0..PPC_Q3_TRIMESH_DATA_SIZE {
        loaded
            .memory
            .write_u8(data1_ptr + offset, data1[offset as usize])
            .unwrap();
        loaded
            .memory
            .write_u8(data2_ptr + offset, data2[offset as usize])
            .unwrap();
    }
    loaded.cpu.gpr[3] = data1_ptr;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    let trimesh = loaded.cpu.gpr[3];
    assert_eq!(trimesh, PPC_Q3_OBJECT_BASE);
    assert_eq!(loaded.q3_objects.len(), 1);
    assert_eq!(loaded.q3_objects[0].object_type, PPC_Q3_TYPE_TRIMESH);
    assert_eq!(loaded.q3_objects[0].data_ptr, 0);
    assert_eq!(loaded.q3_objects[0].data_size, PPC_Q3_TRIMESH_DATA_SIZE);
    assert_eq!(
        loaded.q3_trimeshes,
        vec![PpcQ3TriMeshRecord {
            trimesh,
            data: data1.clone(),
            triangle_attribute_sets: Vec::new(),
            get_data_copies: Vec::new(),
        }]
    );
    loaded.memory.write_u8(data1_ptr, 0).unwrap();
    assert_eq!(loaded.q3_trimeshes[0].data, data1);

    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::Q3ObjectIsDrawable;
    loaded.cpu.gpr[3] = trimesh;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], 1);

    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::Q3TriMeshSetData;
    loaded.cpu.gpr[3] = trimesh;
    loaded.cpu.gpr[4] = data2_ptr;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], 1);
    assert_eq!(loaded.q3_objects[0].data_ptr, 0);
    assert_eq!(loaded.q3_objects[0].data_size, PPC_Q3_TRIMESH_DATA_SIZE);
    assert_eq!(
        loaded.q3_trimeshes,
        vec![PpcQ3TriMeshRecord {
            trimesh,
            data: data2.clone(),
            triangle_attribute_sets: Vec::new(),
            get_data_copies: Vec::new(),
        }]
    );
    loaded.memory.write_u8(data2_ptr, 0).unwrap();

    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::Q3TriMeshGetData;
    loaded.cpu.gpr[3] = trimesh;
    loaded.cpu.gpr[4] = output_ptr;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], 1);
    for offset in 0..PPC_Q3_TRIMESH_DATA_SIZE {
        assert_eq!(
            loaded.memory.read_u8(output_ptr + offset),
            Some(data2[offset as usize])
        );
    }

    let wrong_class = trimesh + PPC_Q3_OBJECT_STRIDE;
    loaded.q3_objects.push(PpcQ3ObjectRecord {
        object: wrong_class,
        kind: PpcQ3ObjectKind::Generic,
        object_type: PPC_Q3_TEXTURE_TYPE_MIPMAP,
        source: PpcQ3ObjectSource::default(),
        data_ptr: 0,
        data_size: PPC_Q3_MIPMAP_COPY_SIZE,
    });
    let before_trimeshes = loaded.q3_trimeshes.clone();
    loaded.q3_error_state = PpcQ3ErrorState::default();
    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::Q3TriMeshSetData;
    loaded.cpu.gpr[3] = wrong_class;
    loaded.cpu.gpr[4] = data1_ptr;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], 0);
    assert_eq!(
        loaded
            .q3_objects
            .iter()
            .find(|record| record.object == wrong_class)
            .map(|record| record.object_type),
        Some(PPC_Q3_TEXTURE_TYPE_MIPMAP)
    );
    assert_eq!(loaded.q3_trimeshes, before_trimeshes);
    assert_eq!(
        loaded.q3_error_state.first_error,
        PPC_Q3_ERROR_INVALID_OBJECT_PARAMETER
    );

    for offset in 0..PPC_Q3_TRIMESH_DATA_SIZE {
        loaded.memory.write_u8(output_ptr + offset, 0xdd).unwrap();
    }
    loaded.q3_error_state = PpcQ3ErrorState::default();
    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::Q3TriMeshGetData;
    loaded.cpu.gpr[3] = wrong_class;
    loaded.cpu.gpr[4] = output_ptr;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], 0);
    for offset in 0..PPC_Q3_TRIMESH_DATA_SIZE {
        assert_eq!(loaded.memory.read_u8(output_ptr + offset), Some(0xdd));
    }
    assert_eq!(loaded.q3_trimeshes, before_trimeshes);
    assert_eq!(
        loaded.q3_error_state.first_error,
        PPC_Q3_ERROR_INVALID_OBJECT_PARAMETER
    );
}

#[test]
fn hle_import_runner_retains_q3_trimesh_triangle_attribute_sets() {
    fn run_q3_call(loaded: &mut PpcLoadedApp, target: PpcImportDispatcherTarget) -> u32 {
        loaded.cpu.pc = loaded.entry_pc;
        loaded.cpu.lr = PPC_HALT_PC;
        loaded.imports[0].dispatcher_target = target;
        let probe = loaded.run_with_hle_imports(64);
        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        loaded.cpu.gpr[3]
    }

    let pef = synthetic_pef_with_library_import(b"QuickDraw\xaa 3D", b"Q3TriMesh_New");
    let mut loaded = load_pef_application(&pef).unwrap();
    let scratch_ptr = PPC_DATA_BASE + 0x1000;
    let shader_ptr = scratch_ptr;
    let triangle_ptr = scratch_ptr + 0x20;
    let trimesh_data_ptr = scratch_ptr + 0x40;
    let attribute_values_ptr = scratch_ptr + 0x90;
    let attribute_table_ptr = scratch_ptr + 0xa0;
    let output_ptr = scratch_ptr + 0xc0;
    loaded.memory.add_region(scratch_ptr, vec![0; 0x120]);

    let attribute_set = run_q3_call(&mut loaded, PpcImportDispatcherTarget::Q3AttributeSetNew);
    loaded.cpu.gpr[3] = 0x1234_0000;
    let shader = run_q3_call(&mut loaded, PpcImportDispatcherTarget::Q3TextureShaderNew);
    loaded.memory.write_u32_be(shader_ptr, shader).unwrap();
    loaded.cpu.gpr[3] = attribute_set;
    loaded.cpu.gpr[4] = PPC_Q3_ATTRIBUTE_TYPE_SURFACE_SHADER;
    loaded.cpu.gpr[5] = shader_ptr;
    assert_eq!(
        run_q3_call(&mut loaded, PpcImportDispatcherTarget::Q3AttributeSetAdd),
        1
    );

    loaded.memory.write_u32_be(triangle_ptr, 0).unwrap();
    loaded.memory.write_u32_be(triangle_ptr + 4, 1).unwrap();
    loaded.memory.write_u32_be(triangle_ptr + 8, 2).unwrap();
    loaded
        .memory
        .write_u32_be(attribute_values_ptr, attribute_set)
        .unwrap();
    loaded
        .memory
        .write_u32_be(attribute_table_ptr, PPC_Q3_ATTRIBUTE_TYPE_SURFACE_SHADER)
        .unwrap();
    loaded
        .memory
        .write_u32_be(attribute_table_ptr + 4, attribute_values_ptr)
        .unwrap();
    loaded
        .memory
        .write_u32_be(
            trimesh_data_ptr + PPC_Q3_TRIMESH_ATTRIBUTE_SET_OFFSET,
            attribute_set,
        )
        .unwrap();
    loaded
        .memory
        .write_u32_be(trimesh_data_ptr + PPC_Q3_TRIMESH_NUM_TRIANGLES_OFFSET, 1)
        .unwrap();
    loaded
        .memory
        .write_u32_be(
            trimesh_data_ptr + PPC_Q3_TRIMESH_TRIANGLES_OFFSET,
            triangle_ptr,
        )
        .unwrap();
    loaded
        .memory
        .write_u32_be(
            trimesh_data_ptr + PPC_Q3_TRIMESH_NUM_TRIANGLE_ATTRIBUTE_TYPES_OFFSET,
            1,
        )
        .unwrap();
    loaded
        .memory
        .write_u32_be(
            trimesh_data_ptr + PPC_Q3_TRIMESH_TRIANGLE_ATTRIBUTE_TYPES_OFFSET,
            attribute_table_ptr,
        )
        .unwrap();
    loaded.cpu.gpr[3] = trimesh_data_ptr;
    let trimesh = run_q3_call(&mut loaded, PpcImportDispatcherTarget::Q3TriMeshNew);

    assert_eq!(loaded.q3_trimeshes.len(), 1);
    assert_eq!(loaded.q3_trimeshes[0].trimesh, trimesh);
    assert_eq!(
        loaded.q3_trimeshes[0].triangle_attribute_sets,
        vec![attribute_set]
    );
    let retained_data = &loaded.q3_trimeshes[0].data;
    assert_eq!(
        ppc_q3_trimesh_header_u32(retained_data, PPC_Q3_TRIMESH_NUM_TRIANGLES_OFFSET),
        Some(1)
    );
    assert_ne!(
        ppc_q3_trimesh_header_u32(retained_data, PPC_Q3_TRIMESH_TRIANGLES_OFFSET),
        Some(triangle_ptr)
    );
    assert_ne!(
        ppc_q3_trimesh_header_u32(
            retained_data,
            PPC_Q3_TRIMESH_TRIANGLE_ATTRIBUTE_TYPES_OFFSET
        ),
        Some(attribute_table_ptr)
    );
    assert_eq!(
        ppc_q3_object_reference_count(
            &loaded.q3_objects,
            &loaded.q3_object_refs,
            attribute_set,
        ),
        Some(2)
    );

    loaded.cpu.gpr[3] = attribute_set;
    assert_eq!(
        run_q3_call(&mut loaded, PpcImportDispatcherTarget::Q3ObjectDispose),
        1
    );
    assert_eq!(
        ppc_q3_object_reference_count(
            &loaded.q3_objects,
            &loaded.q3_object_refs,
            attribute_set,
        ),
        Some(1)
    );

    loaded.cpu.gpr[3] = attribute_set;
    loaded.cpu.gpr[4] = PPC_Q3_ATTRIBUTE_TYPE_SURFACE_SHADER;
    loaded.cpu.gpr[5] = output_ptr;
    assert_eq!(
        run_q3_call(&mut loaded, PpcImportDispatcherTarget::Q3AttributeSetGet),
        1
    );
    assert_eq!(loaded.memory.read_u32_be(output_ptr), Some(shader));

    loaded.cpu.gpr[3] = trimesh;
    assert_eq!(
        run_q3_call(&mut loaded, PpcImportDispatcherTarget::Q3ObjectDispose),
        1
    );
    assert!(!ppc_q3_object_exists(&loaded.q3_objects, attribute_set));
}

#[test]
fn hle_import_runner_handles_q3_trimesh_empty_data() {
    let pef = synthetic_pef_with_library_import(b"QuickDraw\xaa 3D", b"Q3TriMesh_EmptyData");
    let mut loaded = load_pef_application(&pef).unwrap();
    let data_ptr = PPC_DATA_BASE + 0x1000;
    loaded
        .memory
        .add_region(data_ptr, vec![0xaa; PPC_Q3_TRIMESH_DATA_SIZE as usize]);
    for offset in [
        PPC_Q3_TRIMESH_TRIANGLES_OFFSET,
        PPC_Q3_TRIMESH_TRIANGLE_ATTRIBUTE_TYPES_OFFSET,
        PPC_Q3_TRIMESH_EDGES_OFFSET,
        PPC_Q3_TRIMESH_EDGE_ATTRIBUTE_TYPES_OFFSET,
        PPC_Q3_TRIMESH_POINTS_OFFSET,
        PPC_Q3_TRIMESH_VERTEX_ATTRIBUTE_TYPES_OFFSET,
    ] {
        loaded
            .memory
            .write_u32_be(data_ptr + offset, 0x1234_0000 + offset)
            .unwrap();
    }
    loaded
        .memory
        .write_u32_be(data_ptr + PPC_Q3_TRIMESH_NUM_TRIANGLES_OFFSET, 7)
        .unwrap();
    loaded.cpu.gpr[3] = data_ptr;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], 1);
    assert_eq!(
        loaded
            .memory
            .read_u32_be(data_ptr + PPC_Q3_TRIMESH_NUM_TRIANGLES_OFFSET),
        Some(7)
    );
    for offset in [
        PPC_Q3_TRIMESH_TRIANGLES_OFFSET,
        PPC_Q3_TRIMESH_TRIANGLE_ATTRIBUTE_TYPES_OFFSET,
        PPC_Q3_TRIMESH_EDGES_OFFSET,
        PPC_Q3_TRIMESH_EDGE_ATTRIBUTE_TYPES_OFFSET,
        PPC_Q3_TRIMESH_POINTS_OFFSET,
        PPC_Q3_TRIMESH_VERTEX_ATTRIBUTE_TYPES_OFFSET,
    ] {
        assert_eq!(loaded.memory.read_u32_be(data_ptr + offset), Some(0));
    }

    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.cpu.gpr[3] = 0;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], 0);
}

#[test]
fn hle_import_runner_handles_q3_attribute_set_state() {
    let pef = synthetic_pef_with_library_import(b"QuickDraw\xaa 3D", b"Q3AttributeSet_New");
    let mut loaded = load_pef_application(&pef).unwrap();
    let scratch_ptr = PPC_DATA_BASE + 0x1000;
    let uv_ptr = scratch_ptr;
    let color_ptr = scratch_ptr + 0x20;
    let output_ptr = scratch_ptr + 0x40;
    let type_ptr = scratch_ptr + 0x60;
    loaded.memory.add_region(scratch_ptr, vec![0; 0x80]);
    ppc_write_f32_be(&mut loaded.memory, uv_ptr, 0.25).unwrap();
    ppc_write_f32_be(&mut loaded.memory, uv_ptr + 4, 0.5).unwrap();
    ppc_write_f32_be(&mut loaded.memory, color_ptr, 0.1).unwrap();
    ppc_write_f32_be(&mut loaded.memory, color_ptr + 4, 0.2).unwrap();
    ppc_write_f32_be(&mut loaded.memory, color_ptr + 8, 0.3).unwrap();
    loaded.cpu.gpr[3] = 0xdead_beef;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    let attribute_set = loaded.cpu.gpr[3];
    assert_eq!(attribute_set, PPC_Q3_OBJECT_BASE);
    assert_eq!(loaded.q3_objects.len(), 1);
    assert_eq!(loaded.q3_objects[0].object_type, PPC_Q3_TYPE_ATTRIBUTE_SET);
    assert_eq!(loaded.q3_objects[0].data_ptr, 0);
    assert_eq!(loaded.q3_objects[0].data_size, 0);
    assert!(loaded.q3_attributes.is_empty());

    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::Q3AttributeSetAdd;
    loaded.cpu.gpr[3] = attribute_set;
    loaded.cpu.gpr[4] = PPC_Q3_ATTRIBUTE_TYPE_SURFACE_UV;
    loaded.cpu.gpr[5] = uv_ptr;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], 1);
    assert_eq!(loaded.q3_attributes.len(), 1);
    assert_eq!(loaded.q3_attributes[0].attribute_set, attribute_set);
    assert_eq!(
        loaded.q3_attributes[0].attribute_type,
        PPC_Q3_ATTRIBUTE_TYPE_SURFACE_UV
    );
    assert_eq!(loaded.q3_attributes[0].data.len(), 8);

    ppc_write_f32_be(&mut loaded.memory, uv_ptr, 0.75).unwrap();
    ppc_write_f32_be(&mut loaded.memory, uv_ptr + 4, 1.0).unwrap();
    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::Q3AttributeSetGet;
    loaded.cpu.gpr[3] = attribute_set;
    loaded.cpu.gpr[4] = PPC_Q3_ATTRIBUTE_TYPE_SURFACE_UV;
    loaded.cpu.gpr[5] = output_ptr;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], 1);
    assert_eq!(ppc_read_f32_be(&mut loaded.memory, output_ptr), Some(0.25));
    assert_eq!(
        ppc_read_f32_be(&mut loaded.memory, output_ptr + 4),
        Some(0.5)
    );

    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::Q3AttributeSetAdd;
    loaded.cpu.gpr[3] = attribute_set;
    loaded.cpu.gpr[4] = PPC_Q3_ATTRIBUTE_TYPE_DIFFUSE_COLOR;
    loaded.cpu.gpr[5] = color_ptr;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], 1);
    assert_eq!(loaded.q3_attributes.len(), 2);

    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::Q3AttributeSetContains;
    loaded.cpu.gpr[3] = attribute_set;
    loaded.cpu.gpr[4] = PPC_Q3_ATTRIBUTE_TYPE_SURFACE_UV;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], 1);

    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.cpu.gpr[3] = attribute_set;
    loaded.cpu.gpr[4] = PPC_Q3_ATTRIBUTE_TYPE_SPECULAR_COLOR;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], 0);

    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.imports[0].dispatcher_target =
        PpcImportDispatcherTarget::Q3AttributeSetGetNextAttributeType;
    loaded.cpu.gpr[3] = attribute_set;
    loaded.cpu.gpr[4] = type_ptr;
    loaded
        .memory
        .write_u32_be(type_ptr, PPC_Q3_ATTRIBUTE_TYPE_NONE)
        .unwrap();

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], 1);
    assert_eq!(
        loaded.memory.read_u32_be(type_ptr),
        Some(PPC_Q3_ATTRIBUTE_TYPE_SURFACE_UV)
    );

    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.cpu.gpr[3] = attribute_set;
    loaded.cpu.gpr[4] = type_ptr;
    loaded
        .memory
        .write_u32_be(type_ptr, PPC_Q3_ATTRIBUTE_TYPE_SURFACE_UV)
        .unwrap();

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], 1);
    assert_eq!(
        loaded.memory.read_u32_be(type_ptr),
        Some(PPC_Q3_ATTRIBUTE_TYPE_DIFFUSE_COLOR)
    );

    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::Q3AttributeSetClear;
    loaded.cpu.gpr[3] = attribute_set;
    loaded.cpu.gpr[4] = PPC_Q3_ATTRIBUTE_TYPE_SURFACE_UV;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], 1);
    assert_eq!(loaded.q3_attributes.len(), 1);
    assert_eq!(
        loaded.q3_attributes[0].attribute_type,
        PPC_Q3_ATTRIBUTE_TYPE_DIFFUSE_COLOR
    );

    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::Q3ObjectDispose;
    loaded.cpu.gpr[3] = attribute_set;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], 1);
    assert!(loaded.q3_objects.is_empty());
    assert!(loaded.q3_attributes.is_empty());

    let wrong_class = attribute_set + PPC_Q3_OBJECT_STRIDE;
    loaded.q3_objects.push(PpcQ3ObjectRecord {
        object: wrong_class,
        kind: PpcQ3ObjectKind::Generic,
        object_type: PPC_Q3_TEXTURE_TYPE_MIPMAP,
        source: PpcQ3ObjectSource::default(),
        data_ptr: 0,
        data_size: PPC_Q3_MIPMAP_COPY_SIZE,
    });
    loaded.q3_error_state = PpcQ3ErrorState::default();
    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::Q3AttributeSetAdd;
    loaded.cpu.gpr[3] = wrong_class;
    loaded.cpu.gpr[4] = PPC_Q3_ATTRIBUTE_TYPE_DIFFUSE_COLOR;
    loaded.cpu.gpr[5] = color_ptr;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], 0);
    assert!(loaded.q3_attributes.is_empty());
    assert_eq!(
        loaded.q3_error_state.first_error,
        PPC_Q3_ERROR_INVALID_OBJECT_PARAMETER
    );

    loaded.q3_attributes.push(PpcQ3AttributeRecord {
        attribute_set: wrong_class,
        attribute_type: PPC_Q3_ATTRIBUTE_TYPE_SURFACE_UV,
        data: vec![0x77; 8],
    });
    for offset in 0..8 {
        loaded.memory.write_u8(output_ptr + offset, 0xdd).unwrap();
    }
    loaded.q3_error_state = PpcQ3ErrorState::default();
    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::Q3AttributeSetGet;
    loaded.cpu.gpr[3] = wrong_class;
    loaded.cpu.gpr[4] = PPC_Q3_ATTRIBUTE_TYPE_SURFACE_UV;
    loaded.cpu.gpr[5] = output_ptr;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], 0);
    for offset in 0..8 {
        assert_eq!(loaded.memory.read_u8(output_ptr + offset), Some(0xdd));
    }
    assert_eq!(
        loaded.q3_error_state.first_error,
        PPC_Q3_ERROR_INVALID_OBJECT_PARAMETER
    );

    loaded.q3_error_state = PpcQ3ErrorState::default();
    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::Q3AttributeSetContains;
    loaded.cpu.gpr[3] = wrong_class;
    loaded.cpu.gpr[4] = PPC_Q3_ATTRIBUTE_TYPE_SURFACE_UV;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], 0);
    assert_eq!(
        loaded.q3_error_state.first_error,
        PPC_Q3_ERROR_INVALID_OBJECT_PARAMETER
    );

    loaded
        .memory
        .write_u32_be(type_ptr, PPC_Q3_ATTRIBUTE_TYPE_NONE)
        .unwrap();
    loaded.q3_error_state = PpcQ3ErrorState::default();
    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.imports[0].dispatcher_target =
        PpcImportDispatcherTarget::Q3AttributeSetGetNextAttributeType;
    loaded.cpu.gpr[3] = wrong_class;
    loaded.cpu.gpr[4] = type_ptr;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], 0);
    assert_eq!(
        loaded.memory.read_u32_be(type_ptr),
        Some(PPC_Q3_ATTRIBUTE_TYPE_NONE)
    );
    assert_eq!(
        loaded.q3_error_state.first_error,
        PPC_Q3_ERROR_INVALID_OBJECT_PARAMETER
    );

    loaded.q3_error_state = PpcQ3ErrorState::default();
    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::Q3AttributeSetClear;
    loaded.cpu.gpr[3] = wrong_class;
    loaded.cpu.gpr[4] = PPC_Q3_ATTRIBUTE_TYPE_SURFACE_UV;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], 0);
    assert_eq!(loaded.q3_attributes.len(), 1);
    assert_eq!(
        loaded.q3_error_state.first_error,
        PPC_Q3_ERROR_INVALID_OBJECT_PARAMETER
    );
}

#[test]
fn hle_import_runner_retains_q3_attribute_surface_shader_until_clear_or_dispose() {
    fn run_q3_call(loaded: &mut PpcLoadedApp, target: PpcImportDispatcherTarget) -> u32 {
        loaded.cpu.pc = loaded.entry_pc;
        loaded.cpu.lr = PPC_HALT_PC;
        loaded.imports[0].dispatcher_target = target;
        let probe = loaded.run_with_hle_imports(64);
        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        loaded.cpu.gpr[3]
    }

    let pef = synthetic_pef_with_library_import(b"QuickDraw\xaa 3D", b"Q3AttributeSet_New");
    let mut loaded = load_pef_application(&pef).unwrap();
    let scratch_ptr = PPC_DATA_BASE + 0x1000;
    let shader_ptr = scratch_ptr;
    let output_ptr = scratch_ptr + 0x10;
    loaded.memory.add_region(scratch_ptr, vec![0; 0x40]);

    let attribute_set = run_q3_call(&mut loaded, PpcImportDispatcherTarget::Q3AttributeSetNew);

    loaded.cpu.gpr[3] = 0x1111_0000;
    let shader_a = run_q3_call(&mut loaded, PpcImportDispatcherTarget::Q3TextureShaderNew);

    loaded.cpu.gpr[3] = 0x2222_0000;
    let shader_b = run_q3_call(&mut loaded, PpcImportDispatcherTarget::Q3TextureShaderNew);

    loaded.memory.write_u32_be(shader_ptr, shader_a).unwrap();
    loaded.cpu.gpr[3] = attribute_set;
    loaded.cpu.gpr[4] = PPC_Q3_ATTRIBUTE_TYPE_SURFACE_SHADER;
    loaded.cpu.gpr[5] = shader_ptr;
    assert_eq!(
        run_q3_call(&mut loaded, PpcImportDispatcherTarget::Q3AttributeSetAdd),
        1
    );
    assert_eq!(
        ppc_q3_object_reference_count(&loaded.q3_objects, &loaded.q3_object_refs, shader_a),
        Some(2)
    );

    loaded.memory.write_u32_be(shader_ptr, shader_a).unwrap();
    loaded.cpu.gpr[3] = attribute_set;
    loaded.cpu.gpr[4] = PPC_Q3_ATTRIBUTE_TYPE_SURFACE_SHADER;
    loaded.cpu.gpr[5] = shader_ptr;
    assert_eq!(
        run_q3_call(&mut loaded, PpcImportDispatcherTarget::Q3AttributeSetAdd),
        1
    );
    assert_eq!(
        ppc_q3_object_reference_count(&loaded.q3_objects, &loaded.q3_object_refs, shader_a),
        Some(2)
    );

    loaded.cpu.gpr[3] = shader_a;
    assert_eq!(
        run_q3_call(&mut loaded, PpcImportDispatcherTarget::Q3ObjectDispose),
        1
    );
    assert_eq!(
        ppc_q3_object_reference_count(&loaded.q3_objects, &loaded.q3_object_refs, shader_a),
        Some(1)
    );

    loaded.cpu.gpr[3] = attribute_set;
    loaded.cpu.gpr[4] = PPC_Q3_ATTRIBUTE_TYPE_SURFACE_SHADER;
    loaded.cpu.gpr[5] = output_ptr;
    assert_eq!(
        run_q3_call(&mut loaded, PpcImportDispatcherTarget::Q3AttributeSetGet),
        1
    );
    assert_eq!(loaded.memory.read_u32_be(output_ptr), Some(shader_a));
    assert_eq!(
        ppc_q3_object_reference_count(&loaded.q3_objects, &loaded.q3_object_refs, shader_a),
        Some(2)
    );

    loaded.cpu.gpr[3] = shader_a;
    assert_eq!(
        run_q3_call(&mut loaded, PpcImportDispatcherTarget::Q3ObjectDispose),
        1
    );
    assert_eq!(
        ppc_q3_object_reference_count(&loaded.q3_objects, &loaded.q3_object_refs, shader_a),
        Some(1)
    );

    let wrong_surface_shader = shader_b + PPC_Q3_OBJECT_STRIDE * 8;
    loaded.q3_objects.push(PpcQ3ObjectRecord {
        object: wrong_surface_shader,
        kind: PpcQ3ObjectKind::Generic,
        object_type: PPC_Q3_STYLE_TYPE_FILL,
        source: PpcQ3ObjectSource::default(),
        data_ptr: 0,
        data_size: 4,
    });
    loaded
        .q3_attributes
        .iter_mut()
        .find(|record| {
            record.attribute_set == attribute_set
                && record.attribute_type == PPC_Q3_ATTRIBUTE_TYPE_SURFACE_SHADER
        })
        .unwrap()
        .data = wrong_surface_shader.to_be_bytes().to_vec();
    loaded.memory.write_u32_be(output_ptr, 0xeeee_eeee).unwrap();
    loaded.q3_error_state = PpcQ3ErrorState::default();
    loaded.cpu.gpr[3] = attribute_set;
    loaded.cpu.gpr[4] = PPC_Q3_ATTRIBUTE_TYPE_SURFACE_SHADER;
    loaded.cpu.gpr[5] = output_ptr;
    assert_eq!(
        run_q3_call(&mut loaded, PpcImportDispatcherTarget::Q3AttributeSetGet),
        0
    );
    assert_eq!(loaded.memory.read_u32_be(output_ptr), Some(0xeeee_eeee));
    assert_eq!(
        ppc_q3_object_reference_count(
            &loaded.q3_objects,
            &loaded.q3_object_refs,
            wrong_surface_shader,
        ),
        Some(1)
    );
    assert_eq!(
        loaded.q3_error_state.first_error,
        PPC_Q3_ERROR_INVALID_OBJECT_PARAMETER
    );
    loaded
        .q3_attributes
        .iter_mut()
        .find(|record| {
            record.attribute_set == attribute_set
                && record.attribute_type == PPC_Q3_ATTRIBUTE_TYPE_SURFACE_SHADER
        })
        .unwrap()
        .data = shader_a.to_be_bytes().to_vec();
    loaded.q3_error_state = PpcQ3ErrorState::default();

    loaded
        .memory
        .write_u32_be(shader_ptr, attribute_set)
        .unwrap();
    loaded.cpu.gpr[3] = attribute_set;
    loaded.cpu.gpr[4] = PPC_Q3_ATTRIBUTE_TYPE_SURFACE_SHADER;
    loaded.cpu.gpr[5] = shader_ptr;
    assert_eq!(
        run_q3_call(&mut loaded, PpcImportDispatcherTarget::Q3AttributeSetAdd),
        0
    );
    assert_eq!(
        loaded.q3_error_state.first_error,
        PPC_Q3_ERROR_INVALID_OBJECT_PARAMETER
    );
    loaded.q3_error_state = PpcQ3ErrorState::default();
    assert_eq!(
        ppc_q3_attributes_surface_shader(&loaded.q3_attributes),
        Some(shader_a)
    );

    loaded.memory.write_u32_be(shader_ptr, shader_b).unwrap();
    loaded.cpu.gpr[3] = attribute_set;
    loaded.cpu.gpr[4] = PPC_Q3_ATTRIBUTE_TYPE_SURFACE_SHADER;
    loaded.cpu.gpr[5] = shader_ptr;
    assert_eq!(
        run_q3_call(&mut loaded, PpcImportDispatcherTarget::Q3AttributeSetAdd),
        1
    );
    assert!(!ppc_q3_object_exists(&loaded.q3_objects, shader_a));
    assert_eq!(
        ppc_q3_object_reference_count(&loaded.q3_objects, &loaded.q3_object_refs, shader_b),
        Some(2)
    );
    assert_eq!(
        ppc_q3_attributes_surface_shader(&loaded.q3_attributes),
        Some(shader_b)
    );

    loaded.cpu.gpr[3] = attribute_set;
    loaded.cpu.gpr[4] = PPC_Q3_ATTRIBUTE_TYPE_SURFACE_SHADER;
    assert_eq!(
        run_q3_call(&mut loaded, PpcImportDispatcherTarget::Q3AttributeSetClear),
        1
    );
    assert!(loaded.q3_attributes.is_empty());
    assert_eq!(
        ppc_q3_object_reference_count(&loaded.q3_objects, &loaded.q3_object_refs, shader_b),
        Some(1)
    );

    loaded.memory.write_u32_be(shader_ptr, shader_b).unwrap();
    loaded.cpu.gpr[3] = attribute_set;
    loaded.cpu.gpr[4] = PPC_Q3_ATTRIBUTE_TYPE_SURFACE_SHADER;
    loaded.cpu.gpr[5] = shader_ptr;
    assert_eq!(
        run_q3_call(&mut loaded, PpcImportDispatcherTarget::Q3AttributeSetAdd),
        1
    );
    loaded.cpu.gpr[3] = shader_b;
    assert_eq!(
        run_q3_call(&mut loaded, PpcImportDispatcherTarget::Q3ObjectDispose),
        1
    );
    assert_eq!(
        ppc_q3_object_reference_count(&loaded.q3_objects, &loaded.q3_object_refs, shader_b),
        Some(1)
    );

    loaded.cpu.gpr[3] = attribute_set;
    assert_eq!(
        run_q3_call(&mut loaded, PpcImportDispatcherTarget::Q3ObjectDispose),
        1
    );
    assert!(!ppc_q3_object_exists(&loaded.q3_objects, attribute_set));
    assert!(!ppc_q3_object_exists(&loaded.q3_objects, shader_b));
    assert!(loaded.q3_attributes.is_empty());
    assert!(loaded.q3_object_refs.is_empty());
}

#[test]
fn hle_import_runner_handles_q3_shader_uv_transform_state() {
    let pef =
        synthetic_pef_with_library_import(b"QuickDraw\xaa 3D", b"Q3Shader_SetUVTransform");
    let mut loaded = load_pef_application(&pef).unwrap();
    let shader = PPC_Q3_OBJECT_BASE;
    let scratch_ptr = PPC_DATA_BASE + 0x1000;
    let matrix_ptr = scratch_ptr;
    let output_ptr = scratch_ptr + 0x40;
    loaded.memory.add_region(scratch_ptr, vec![0; 0x100]);
    loaded
        .q3_objects
        .push(test_q3_object(shader, PPC_Q3_SURFACE_SHADER_TYPE_TEXTURE));

    loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::Q3ShaderGetUVTransform;
    loaded.cpu.gpr[3] = shader;
    loaded.cpu.gpr[4] = output_ptr;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], 1);
    assert_eq!(
        ppc_read_q3_matrix3x3(&mut loaded.memory, output_ptr),
        Some(ppc_q3_matrix3x3_identity())
    );

    let first_matrix = [[1.0, 0.0, 0.25], [0.0, 2.0, 0.5], [0.0, 0.0, 1.0]];
    ppc_write_q3_matrix3x3(&mut loaded.memory, matrix_ptr, &first_matrix).unwrap();
    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::Q3ShaderSetUVTransform;
    loaded.cpu.gpr[3] = shader;
    loaded.cpu.gpr[4] = matrix_ptr;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], 1);
    assert_eq!(
        loaded.q3_shader_uv_transforms,
        vec![PpcQ3ShaderUvTransformRecord {
            shader,
            matrix: first_matrix,
        }]
    );

    let mutated_source = [[9.0, 9.0, 9.0], [9.0, 9.0, 9.0], [9.0, 9.0, 9.0]];
    ppc_write_q3_matrix3x3(&mut loaded.memory, matrix_ptr, &mutated_source).unwrap();
    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::Q3ShaderGetUVTransform;
    loaded.cpu.gpr[3] = shader;
    loaded.cpu.gpr[4] = output_ptr;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], 1);
    assert_eq!(
        ppc_read_q3_matrix3x3(&mut loaded.memory, output_ptr),
        Some(first_matrix)
    );

    let second_matrix = [[0.5, 0.0, 0.0], [0.0, 0.5, 0.0], [0.125, 0.25, 1.0]];
    ppc_write_q3_matrix3x3(&mut loaded.memory, matrix_ptr, &second_matrix).unwrap();
    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::Q3ShaderSetUVTransform;
    loaded.cpu.gpr[3] = shader;
    loaded.cpu.gpr[4] = matrix_ptr;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], 1);
    assert_eq!(loaded.q3_shader_uv_transforms.len(), 1);
    assert_eq!(loaded.q3_shader_uv_transforms[0].matrix, second_matrix);

    let rejected_matrix = [[3.0, 0.0, 0.0], [0.0, 3.0, 0.0], [0.0, 0.0, 1.0]];
    ppc_write_q3_matrix3x3(&mut loaded.memory, matrix_ptr, &rejected_matrix).unwrap();
    loaded.q3_objects[0].object_type = PPC_Q3_STYLE_TYPE_FILL;
    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::Q3ShaderSetUVTransform;
    loaded.cpu.gpr[3] = shader;
    loaded.cpu.gpr[4] = matrix_ptr;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], 0);
    assert_eq!(
        loaded.q3_error_state.first_error,
        PPC_Q3_ERROR_INVALID_OBJECT_PARAMETER
    );
    assert_eq!(loaded.q3_shader_uv_transforms.len(), 1);
    assert_eq!(loaded.q3_shader_uv_transforms[0].matrix, second_matrix);
    loaded.q3_objects[0].object_type = PPC_Q3_SURFACE_SHADER_TYPE_TEXTURE;
    loaded.q3_error_state = PpcQ3ErrorState::default();

    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::Q3ObjectDispose;
    loaded.cpu.gpr[3] = shader;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], 1);
    assert!(loaded.q3_objects.is_empty());
    assert!(loaded.q3_shader_uv_transforms.is_empty());
}

#[test]
fn hle_import_runner_handles_q3_shader_boundary_state() {
    let pef = synthetic_pef_with_library_import(b"QuickDraw\xaa 3D", b"Q3Shader_SetUBoundary");
    let mut loaded = load_pef_application(&pef).unwrap();
    let shader = PPC_Q3_OBJECT_BASE;
    let output_ptr = PPC_DATA_BASE + 0x1000;
    loaded.memory.add_region(output_ptr, vec![0; 8]);
    loaded.q3_objects.push(PpcQ3ObjectRecord {
        object: shader,
        kind: PpcQ3ObjectKind::Generic,
        object_type: PPC_Q3_SURFACE_SHADER_TYPE_TEXTURE,
        source: PpcQ3ObjectSource::default(),
        data_ptr: 0,
        data_size: 0,
    });

    loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::Q3ShaderGetUBoundary;
    loaded.cpu.gpr[3] = shader;
    loaded.cpu.gpr[4] = output_ptr;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], 1);
    assert_eq!(
        loaded.memory.read_u32_be(output_ptr),
        Some(PPC_Q3_SHADER_UV_BOUNDARY_WRAP)
    );

    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::Q3ShaderSetUBoundary;
    loaded.cpu.gpr[3] = shader;
    loaded.cpu.gpr[4] = PPC_Q3_SHADER_UV_BOUNDARY_CLAMP;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], 1);
    assert_eq!(
        loaded.q3_shader_boundaries,
        vec![PpcQ3ShaderBoundaryRecord {
            shader,
            u_boundary: PPC_Q3_SHADER_UV_BOUNDARY_CLAMP,
            v_boundary: PPC_Q3_SHADER_UV_BOUNDARY_WRAP,
        }]
    );

    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::Q3ShaderSetVBoundary;
    loaded.cpu.gpr[3] = shader;
    loaded.cpu.gpr[4] = PPC_Q3_SHADER_UV_BOUNDARY_CLAMP;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], 1);
    assert_eq!(loaded.q3_shader_boundaries.len(), 1);
    assert_eq!(
        loaded.q3_shader_boundaries[0],
        PpcQ3ShaderBoundaryRecord {
            shader,
            u_boundary: PPC_Q3_SHADER_UV_BOUNDARY_CLAMP,
            v_boundary: PPC_Q3_SHADER_UV_BOUNDARY_CLAMP,
        }
    );

    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::Q3ShaderGetVBoundary;
    loaded.cpu.gpr[3] = shader;
    loaded.cpu.gpr[4] = output_ptr + 4;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], 1);
    assert_eq!(
        loaded.memory.read_u32_be(output_ptr + 4),
        Some(PPC_Q3_SHADER_UV_BOUNDARY_CLAMP)
    );

    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::Q3ShaderSetUBoundary;
    loaded.cpu.gpr[3] = shader;
    loaded.cpu.gpr[4] = 2;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], 0);
    assert_eq!(
        loaded.q3_shader_boundaries[0].u_boundary,
        PPC_Q3_SHADER_UV_BOUNDARY_CLAMP
    );

    loaded.q3_objects[0].object_type = PPC_Q3_STYLE_TYPE_FILL;
    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::Q3ShaderSetUBoundary;
    loaded.cpu.gpr[3] = shader;
    loaded.cpu.gpr[4] = PPC_Q3_SHADER_UV_BOUNDARY_WRAP;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], 0);
    assert_eq!(
        loaded.q3_error_state.first_error,
        PPC_Q3_ERROR_INVALID_OBJECT_PARAMETER
    );
    assert_eq!(
        loaded.q3_shader_boundaries[0].u_boundary,
        PPC_Q3_SHADER_UV_BOUNDARY_CLAMP
    );
    loaded.q3_objects[0].object_type = PPC_Q3_SURFACE_SHADER_TYPE_TEXTURE;
    loaded.q3_error_state = PpcQ3ErrorState::default();

    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::Q3ObjectDispose;
    loaded.cpu.gpr[3] = shader;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], 1);
    assert!(loaded.q3_objects.is_empty());
    assert!(loaded.q3_shader_boundaries.is_empty());
}

#[test]
fn hle_import_runner_handles_q3_style_object_state() {
    let pef = synthetic_pef_with_library_import(b"QuickDraw\xaa 3D", b"Q3FillStyle_New");
    let mut loaded = load_pef_application(&pef).unwrap();
    let scratch_ptr = PPC_DATA_BASE + 0x1000;
    let output_ptr = scratch_ptr;
    loaded.memory.add_region(scratch_ptr, vec![0; 0x20]);
    loaded.cpu.gpr[3] = 2;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    let fill_style = loaded.cpu.gpr[3];
    assert_eq!(fill_style, PPC_Q3_OBJECT_BASE);
    assert_eq!(loaded.q3_objects[0].object_type, PPC_Q3_STYLE_TYPE_FILL);
    assert_eq!(
        loaded.q3_styles,
        vec![PpcQ3StyleRecord {
            style: fill_style,
            kind: PpcQ3StyleKind::Fill,
            value: 2,
        }]
    );

    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::Q3FillStyleGet;
    loaded.cpu.gpr[3] = fill_style;
    loaded.cpu.gpr[4] = output_ptr;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], 1);
    assert_eq!(loaded.memory.read_u32_be(output_ptr), Some(2));

    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::Q3FillStyleSet;
    loaded.cpu.gpr[3] = fill_style;
    loaded.cpu.gpr[4] = 1;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], 1);
    assert_eq!(loaded.q3_styles[0].value, 1);

    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.cpu.gpr[3] = fill_style;
    loaded.cpu.gpr[4] = 99;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], 0);
    assert_eq!(loaded.q3_styles[0].value, 1);

    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::Q3InterpolationStyleNew;
    loaded.cpu.gpr[3] = 1;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    let interpolation_style = loaded.cpu.gpr[3];
    assert_eq!(
        interpolation_style,
        PPC_Q3_OBJECT_BASE + PPC_Q3_OBJECT_STRIDE
    );
    assert_eq!(
        loaded.q3_objects[1].object_type,
        PPC_Q3_STYLE_TYPE_INTERPOLATION
    );
    assert_eq!(
        loaded.q3_styles[1],
        PpcQ3StyleRecord {
            style: interpolation_style,
            kind: PpcQ3StyleKind::Interpolation,
            value: 1,
        }
    );

    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::Q3BackfacingStyleNew;
    loaded.cpu.gpr[3] = 2;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    let backfacing_style = loaded.cpu.gpr[3];
    assert_eq!(
        backfacing_style,
        PPC_Q3_OBJECT_BASE + PPC_Q3_OBJECT_STRIDE * 2
    );
    assert_eq!(
        loaded.q3_objects[2].object_type,
        PPC_Q3_STYLE_TYPE_BACKFACING
    );
    assert_eq!(
        loaded.q3_styles[2],
        PpcQ3StyleRecord {
            style: backfacing_style,
            kind: PpcQ3StyleKind::Backfacing,
            value: 2,
        }
    );

    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.cpu.gpr[3] = 99;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], 0);
    assert_eq!(loaded.q3_styles.len(), 3);

    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::Q3OrientationStyleNew;
    loaded.cpu.gpr[3] = PPC_Q3_ORIENTATION_STYLE_CLOCKWISE;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    let orientation_style = loaded.cpu.gpr[3];
    assert_eq!(
        orientation_style,
        PPC_Q3_OBJECT_BASE + PPC_Q3_OBJECT_STRIDE * 3
    );
    assert_eq!(
        loaded.q3_objects[3].object_type,
        PPC_Q3_STYLE_TYPE_ORIENTATION
    );
    assert_eq!(
        loaded.q3_styles[3],
        PpcQ3StyleRecord {
            style: orientation_style,
            kind: PpcQ3StyleKind::Orientation,
            value: PPC_Q3_ORIENTATION_STYLE_CLOCKWISE,
        }
    );

    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::Q3OrientationStyleGet;
    loaded.cpu.gpr[3] = orientation_style;
    loaded.cpu.gpr[4] = output_ptr;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], 1);
    assert_eq!(
        loaded.memory.read_u32_be(output_ptr),
        Some(PPC_Q3_ORIENTATION_STYLE_CLOCKWISE)
    );

    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::Q3OrientationStyleSet;
    loaded.cpu.gpr[3] = orientation_style;
    loaded.cpu.gpr[4] = PPC_Q3_ORIENTATION_STYLE_COUNTER_CLOCKWISE;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], 1);
    assert_eq!(
        loaded.q3_styles[3].value,
        PPC_Q3_ORIENTATION_STYLE_COUNTER_CLOCKWISE
    );

    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::Q3FillStyleSet;
    loaded.cpu.gpr[3] = orientation_style;
    loaded.cpu.gpr[4] = PPC_Q3_FILL_STYLE_POINTS;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], 0);
    assert_eq!(
        loaded.q3_error_state.first_error,
        PPC_Q3_ERROR_INVALID_OBJECT_PARAMETER
    );
    assert_eq!(
        loaded.q3_styles[3].value,
        PPC_Q3_ORIENTATION_STYLE_COUNTER_CLOCKWISE
    );
    loaded.q3_error_state = PpcQ3ErrorState::default();

    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::Q3FillStyleGet;
    loaded.cpu.gpr[3] = orientation_style;
    loaded.cpu.gpr[4] = output_ptr;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], 0);
    assert_eq!(
        loaded.q3_error_state.first_error,
        PPC_Q3_ERROR_INVALID_OBJECT_PARAMETER
    );
    loaded.q3_error_state = PpcQ3ErrorState::default();

    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::Q3OrientationStyleSet;
    loaded.cpu.gpr[3] = orientation_style;
    loaded.cpu.gpr[4] = 2;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], 0);
    assert_eq!(
        loaded.q3_styles[3].value,
        PPC_Q3_ORIENTATION_STYLE_COUNTER_CLOCKWISE
    );

    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::Q3ObjectDispose;
    loaded.cpu.gpr[3] = fill_style;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], 1);
    assert_eq!(loaded.q3_styles.len(), 3);
    assert!(loaded
        .q3_styles
        .iter()
        .all(|record| record.style != fill_style));
}

#[test]
fn hle_import_runner_handles_q3_view_angle_aspect_camera_state() {
    let pef =
        synthetic_pef_with_library_import(b"QuickDraw\xaa 3D", b"Q3ViewAngleAspectCamera_New");
    let mut loaded = load_pef_application(&pef).unwrap();
    let scratch_ptr = PPC_DATA_BASE + 0x1000;
    let camera_data_ptr = scratch_ptr;
    let placement_ptr = scratch_ptr + 0x80;
    let output_ptr = scratch_ptr + 0xc0;
    loaded.memory.add_region(scratch_ptr, vec![0; 0x200]);
    let initial_placement = PpcQ3CameraPlacement {
        camera_location: (0.0, 0.0, 0.0),
        point_of_interest: (0.0, 0.0, -1.0),
        up_vector: (0.0, 1.0, 0.0),
    };
    ppc_write_q3_camera_placement(&mut loaded.memory, camera_data_ptr, initial_placement)
        .unwrap();
    ppc_write_f32_be(&mut loaded.memory, camera_data_ptr + 36, 0.1).unwrap();
    ppc_write_f32_be(&mut loaded.memory, camera_data_ptr + 40, 100.0).unwrap();
    ppc_write_q3_vector2d(&mut loaded.memory, camera_data_ptr + 44, (-1.0, -1.0)).unwrap();
    ppc_write_f32_be(&mut loaded.memory, camera_data_ptr + 52, 2.0).unwrap();
    ppc_write_f32_be(&mut loaded.memory, camera_data_ptr + 56, 2.0).unwrap();
    ppc_write_f32_be(
        &mut loaded.memory,
        camera_data_ptr + 60,
        std::f32::consts::FRAC_PI_2,
    )
    .unwrap();
    ppc_write_f32_be(&mut loaded.memory, camera_data_ptr + 64, 4.0 / 3.0).unwrap();
    loaded.cpu.gpr[3] = camera_data_ptr;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    let camera = loaded.cpu.gpr[3];
    assert_eq!(camera, PPC_Q3_OBJECT_BASE);
    assert_eq!(
        loaded.q3_objects[0].object_type,
        PPC_Q3_CAMERA_TYPE_VIEW_ANGLE_ASPECT
    );
    assert_eq!(
        loaded.q3_objects[0].data_size,
        PPC_Q3_VIEW_ANGLE_ASPECT_CAMERA_DATA_SIZE
    );
    assert_eq!(loaded.q3_cameras.len(), 1);
    assert_eq!(loaded.q3_cameras[0].camera, camera);
    assert_eq!(loaded.q3_cameras[0].placement, initial_placement);
    assert_eq!(
        loaded.q3_cameras[0].projection,
        PpcQ3CameraProjection::ViewAngleAspect {
            fov: std::f32::consts::FRAC_PI_2,
            aspect_ratio_x_to_y: 4.0 / 3.0,
        }
    );

    let mutated_source_placement = PpcQ3CameraPlacement {
        camera_location: (9.0, 9.0, 9.0),
        point_of_interest: (9.0, 9.0, 8.0),
        up_vector: (0.0, 1.0, 0.0),
    };
    ppc_write_q3_camera_placement(
        &mut loaded.memory,
        camera_data_ptr,
        mutated_source_placement,
    )
    .unwrap();
    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::Q3CameraGetPlacement;
    loaded.cpu.gpr[3] = camera;
    loaded.cpu.gpr[4] = output_ptr;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], 1);
    assert_eq!(
        ppc_read_q3_camera_placement(&mut loaded.memory, output_ptr),
        Some(initial_placement)
    );

    let updated_placement = PpcQ3CameraPlacement {
        camera_location: (1.0, 2.0, 3.0),
        point_of_interest: (1.0, 2.0, 2.0),
        up_vector: (0.0, 1.0, 0.0),
    };
    ppc_write_q3_camera_placement(&mut loaded.memory, placement_ptr, updated_placement)
        .unwrap();
    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::Q3CameraSetPlacement;
    loaded.cpu.gpr[3] = camera;
    loaded.cpu.gpr[4] = placement_ptr;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], 1);
    assert_eq!(loaded.q3_cameras[0].placement, updated_placement);

    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::Q3CameraGetRange;
    loaded.cpu.gpr[3] = camera;
    loaded.cpu.gpr[4] = output_ptr;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], 1);
    assert_eq!(
        ppc_read_q3_camera_range(&mut loaded.memory, output_ptr),
        Some((0.1, 100.0))
    );

    ppc_write_q3_camera_range(&mut loaded.memory, placement_ptr, 0.25, 250.0).unwrap();
    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::Q3CameraSetRange;
    loaded.cpu.gpr[3] = camera;
    loaded.cpu.gpr[4] = placement_ptr;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], 1);
    assert_eq!(loaded.q3_cameras[0].range_hither, 0.25);
    assert_eq!(loaded.q3_cameras[0].range_yon, 250.0);

    ppc_write_q3_camera_range(&mut loaded.memory, placement_ptr, 10.0, 5.0).unwrap();
    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.cpu.gpr[3] = camera;
    loaded.cpu.gpr[4] = placement_ptr;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], 0);
    assert_eq!(loaded.q3_cameras[0].range_hither, 0.25);
    assert_eq!(loaded.q3_cameras[0].range_yon, 250.0);

    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::Q3CameraGetViewPort;
    loaded.cpu.gpr[3] = camera;
    loaded.cpu.gpr[4] = output_ptr;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], 1);
    assert_eq!(
        ppc_read_q3_camera_viewport(&mut loaded.memory, output_ptr),
        Some(((-1.0, -1.0), 2.0, 2.0))
    );

    ppc_write_q3_camera_viewport(&mut loaded.memory, placement_ptr, (0.25, -0.5), 1.5, 1.25)
        .unwrap();
    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::Q3CameraSetViewPort;
    loaded.cpu.gpr[3] = camera;
    loaded.cpu.gpr[4] = placement_ptr;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], 1);
    assert_eq!(loaded.q3_cameras[0].viewport_origin, (0.25, -0.5));
    assert_eq!(loaded.q3_cameras[0].viewport_width, 1.5);
    assert_eq!(loaded.q3_cameras[0].viewport_height, 1.25);

    ppc_write_q3_camera_viewport(&mut loaded.memory, placement_ptr, (0.0, 0.0), 0.0, 1.0)
        .unwrap();
    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.cpu.gpr[3] = camera;
    loaded.cpu.gpr[4] = placement_ptr;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], 0);
    assert_eq!(loaded.q3_cameras[0].viewport_origin, (0.25, -0.5));
    assert_eq!(loaded.q3_cameras[0].viewport_width, 1.5);
    assert_eq!(loaded.q3_cameras[0].viewport_height, 1.25);

    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::Q3CameraGetWorldToView;
    loaded.cpu.gpr[3] = camera;
    loaded.cpu.gpr[4] = output_ptr;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], 1);
    assert_eq!(
        ppc_read_q3_matrix4x4(&mut loaded.memory, output_ptr),
        Some([
            [1.0, 0.0, -0.0, 0.0],
            [-0.0, 1.0, -0.0, 0.0],
            [0.0, 0.0, 1.0, 0.0],
            [-1.0, -2.0, -3.0, 1.0],
        ])
    );

    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::Q3CameraGetViewToFrustum;
    loaded.cpu.gpr[3] = camera;
    loaded.cpu.gpr[4] = output_ptr;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], 1);
    let frustum = ppc_read_q3_matrix4x4(&mut loaded.memory, output_ptr).unwrap();
    assert!((frustum[0][0] - 0.75).abs() < 0.0001);
    assert!((frustum[1][1] - 1.0).abs() < 0.0001);
    assert!((frustum[2][2] - 250.0 / 249.75).abs() < 0.0001);
    assert!((frustum[3][2] - 62.5 / 249.75).abs() < 0.0001);
    assert_eq!(frustum[2][3], -1.0);
    assert_eq!(frustum[3][3], 0.0);

    for offset in 0..PPC_Q3_CAMERA_PLACEMENT_SIZE {
        loaded.memory.write_u8(output_ptr + offset, 0xa6).unwrap();
    }
    loaded.q3_objects[0].object_type = PPC_Q3_STYLE_TYPE_FILL;
    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::Q3CameraGetPlacement;
    loaded.cpu.gpr[3] = camera;
    loaded.cpu.gpr[4] = output_ptr;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], 0);
    assert_eq!(
        loaded.q3_error_state.first_error,
        PPC_Q3_ERROR_INVALID_OBJECT_PARAMETER
    );
    for offset in 0..PPC_Q3_CAMERA_PLACEMENT_SIZE {
        assert_eq!(loaded.memory.read_u8(output_ptr + offset), Some(0xa6));
    }
    assert_eq!(loaded.q3_cameras[0].placement, updated_placement);
    loaded.q3_error_state = PpcQ3ErrorState::default();

    ppc_write_q3_camera_range(&mut loaded.memory, placement_ptr, 0.5, 500.0).unwrap();
    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::Q3CameraSetRange;
    loaded.cpu.gpr[3] = camera;
    loaded.cpu.gpr[4] = placement_ptr;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], 0);
    assert_eq!(
        loaded.q3_error_state.first_error,
        PPC_Q3_ERROR_INVALID_OBJECT_PARAMETER
    );
    assert_eq!(loaded.q3_cameras[0].range_hither, 0.25);
    assert_eq!(loaded.q3_cameras[0].range_yon, 250.0);
    loaded.q3_objects[0].object_type = PPC_Q3_CAMERA_TYPE_VIEW_ANGLE_ASPECT;
    loaded.q3_error_state = PpcQ3ErrorState::default();

    let invalid_getters = [
        (
            PpcImportDispatcherTarget::Q3CameraGetPlacement,
            PPC_Q3_CAMERA_PLACEMENT_SIZE,
            0xa1u8,
        ),
        (
            PpcImportDispatcherTarget::Q3CameraGetRange,
            PPC_Q3_CAMERA_RANGE_SIZE,
            0xa2u8,
        ),
        (
            PpcImportDispatcherTarget::Q3CameraGetViewPort,
            PPC_Q3_CAMERA_VIEWPORT_SIZE,
            0xa3u8,
        ),
        (
            PpcImportDispatcherTarget::Q3CameraGetWorldToView,
            PPC_Q3_MATRIX4X4_SIZE,
            0xa4u8,
        ),
        (
            PpcImportDispatcherTarget::Q3CameraGetViewToFrustum,
            PPC_Q3_MATRIX4X4_SIZE,
            0xa5u8,
        ),
    ];
    for (index, (target, size, sentinel)) in invalid_getters.iter().enumerate() {
        let invalid_output_ptr = PPC_DATA_BASE + 0x1400 + (index as u32) * 0x100;
        loaded
            .memory
            .add_region(invalid_output_ptr, vec![*sentinel; (*size - 1) as usize]);
        loaded.cpu.pc = loaded.entry_pc;
        loaded.cpu.lr = PPC_HALT_PC;
        loaded.imports[0].dispatcher_target = target.clone();
        loaded.cpu.gpr[3] = camera;
        loaded.cpu.gpr[4] = invalid_output_ptr;

        let probe = loaded.run_with_hle_imports(64);

        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(loaded.cpu.gpr[3], 0);
        for offset in 0..(*size - 1) {
            assert_eq!(
                loaded.memory.read_u8(invalid_output_ptr + offset),
                Some(*sentinel)
            );
        }
    }
}

#[test]
fn hle_import_runner_handles_q3_orthographic_and_view_plane_camera_state() {
    fn write_common_camera_data(
        memory: &mut PpcSectionMem,
        data_ptr: u32,
        placement: PpcQ3CameraPlacement,
    ) {
        ppc_write_q3_camera_placement(memory, data_ptr, placement).unwrap();
        ppc_write_f32_be(memory, data_ptr + 36, 0.5).unwrap();
        ppc_write_f32_be(memory, data_ptr + 40, 10.0).unwrap();
        ppc_write_q3_vector2d(memory, data_ptr + 44, (-1.0, -1.0)).unwrap();
        ppc_write_f32_be(memory, data_ptr + 52, 2.0).unwrap();
        ppc_write_f32_be(memory, data_ptr + 56, 2.0).unwrap();
    }

    fn assert_f32_close(actual: f32, expected: f32) {
        assert!(
            (actual - expected).abs() < 0.0001,
            "expected {expected}, got {actual}"
        );
    }

    let pef =
        synthetic_pef_with_library_import(b"QuickDraw\xaa 3D", b"Q3OrthographicCamera_New");
    let mut loaded = load_pef_application(&pef).unwrap();
    let scratch_ptr = PPC_DATA_BASE + 0x1000;
    let ortho_data_ptr = scratch_ptr;
    let view_plane_data_ptr = scratch_ptr + 0x80;
    let output_ptr = scratch_ptr + 0x120;
    loaded.memory.add_region(scratch_ptr, vec![0; 0x300]);
    let placement = PpcQ3CameraPlacement {
        camera_location: (0.0, 0.0, 0.0),
        point_of_interest: (0.0, 0.0, -1.0),
        up_vector: (0.0, 1.0, 0.0),
    };
    write_common_camera_data(&mut loaded.memory, ortho_data_ptr, placement);
    ppc_write_f32_be(
        &mut loaded.memory,
        ortho_data_ptr + PPC_Q3_CAMERA_DATA_SIZE,
        -2.0,
    )
    .unwrap();
    ppc_write_f32_be(
        &mut loaded.memory,
        ortho_data_ptr + PPC_Q3_CAMERA_DATA_SIZE + 4,
        1.0,
    )
    .unwrap();
    ppc_write_f32_be(
        &mut loaded.memory,
        ortho_data_ptr + PPC_Q3_CAMERA_DATA_SIZE + 8,
        2.0,
    )
    .unwrap();
    ppc_write_f32_be(
        &mut loaded.memory,
        ortho_data_ptr + PPC_Q3_CAMERA_DATA_SIZE + 12,
        -1.0,
    )
    .unwrap();
    loaded.cpu.gpr[3] = ortho_data_ptr;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    let orthographic_camera = loaded.cpu.gpr[3];
    assert_eq!(orthographic_camera, PPC_Q3_OBJECT_BASE);
    assert_eq!(
        loaded.q3_objects[0].object_type,
        PPC_Q3_CAMERA_TYPE_ORTHOGRAPHIC
    );
    assert_eq!(
        loaded.q3_objects[0].data_size,
        PPC_Q3_ORTHOGRAPHIC_CAMERA_DATA_SIZE
    );
    assert_eq!(
        loaded.q3_cameras[0].projection,
        PpcQ3CameraProjection::Orthographic {
            left: -2.0,
            top: 1.0,
            right: 2.0,
            bottom: -1.0,
        }
    );

    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::Q3ObjectIsType;
    loaded.cpu.gpr[3] = orthographic_camera;
    loaded.cpu.gpr[4] = PPC_Q3_SHAPE_TYPE_CAMERA;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], 1);

    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::Q3CameraGetViewToFrustum;
    loaded.cpu.gpr[3] = orthographic_camera;
    loaded.cpu.gpr[4] = output_ptr;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], 1);
    let frustum = ppc_read_q3_matrix4x4(&mut loaded.memory, output_ptr).unwrap();
    assert_f32_close(frustum[0][0], 0.5);
    assert_f32_close(frustum[1][1], 1.0);
    assert_f32_close(frustum[2][2], 1.0 / 9.5);
    assert_f32_close(frustum[3][2], 0.5 / 9.5);
    assert_eq!(frustum[2][3], 0.0);
    assert_eq!(frustum[3][3], 1.0);

    write_common_camera_data(&mut loaded.memory, view_plane_data_ptr, placement);
    ppc_write_f32_be(
        &mut loaded.memory,
        view_plane_data_ptr + PPC_Q3_CAMERA_DATA_SIZE,
        2.0,
    )
    .unwrap();
    ppc_write_f32_be(
        &mut loaded.memory,
        view_plane_data_ptr + PPC_Q3_CAMERA_DATA_SIZE + 4,
        2.0,
    )
    .unwrap();
    ppc_write_f32_be(
        &mut loaded.memory,
        view_plane_data_ptr + PPC_Q3_CAMERA_DATA_SIZE + 8,
        1.0,
    )
    .unwrap();
    ppc_write_f32_be(
        &mut loaded.memory,
        view_plane_data_ptr + PPC_Q3_CAMERA_DATA_SIZE + 12,
        0.5,
    )
    .unwrap();
    ppc_write_f32_be(
        &mut loaded.memory,
        view_plane_data_ptr + PPC_Q3_CAMERA_DATA_SIZE + 16,
        -0.25,
    )
    .unwrap();
    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::Q3ViewPlaneCameraNew;
    loaded.cpu.gpr[3] = view_plane_data_ptr;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    let view_plane_camera = loaded.cpu.gpr[3];
    assert_eq!(view_plane_camera, PPC_Q3_OBJECT_BASE + PPC_Q3_OBJECT_STRIDE);
    assert_eq!(
        loaded.q3_objects[1].object_type,
        PPC_Q3_CAMERA_TYPE_VIEW_PLANE
    );
    assert_eq!(
        loaded.q3_objects[1].data_size,
        PPC_Q3_VIEW_PLANE_CAMERA_DATA_SIZE
    );
    assert_eq!(
        loaded.q3_cameras[1].projection,
        PpcQ3CameraProjection::ViewPlane {
            view_plane: 2.0,
            half_width_at_view_plane: 2.0,
            half_height_at_view_plane: 1.0,
            center_x_on_view_plane: 0.5,
            center_y_on_view_plane: -0.25,
        }
    );

    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::Q3ObjectIsType;
    loaded.cpu.gpr[3] = view_plane_camera;
    loaded.cpu.gpr[4] = PPC_Q3_SHARED_TYPE_SHAPE;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], 1);

    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::Q3CameraGetViewToFrustum;
    loaded.cpu.gpr[3] = view_plane_camera;
    loaded.cpu.gpr[4] = output_ptr;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], 1);
    let frustum = ppc_read_q3_matrix4x4(&mut loaded.memory, output_ptr).unwrap();
    assert_f32_close(frustum[0][0], 1.0);
    assert_f32_close(frustum[1][1], 2.0);
    assert_f32_close(frustum[2][0], 0.25);
    assert_f32_close(frustum[2][1], -0.25);
    assert_f32_close(frustum[2][2], 10.0 / 9.5);
    assert_f32_close(frustum[3][2], 5.0 / 9.5);
    assert_eq!(frustum[2][3], -1.0);
    assert_eq!(frustum[3][3], 0.0);
}

#[test]
fn hle_import_runner_handles_q3_ambient_and_directional_light_state() {
    fn assert_q3_light_getter_short_output_preserved(
        loaded: &mut PpcLoadedApp,
        target: PpcImportDispatcherTarget,
        light: u32,
        invalid_output_ptr: u32,
        size: u32,
        sentinel: u8,
    ) {
        let short_size = size - 1;
        loaded
            .memory
            .add_region(invalid_output_ptr, vec![sentinel; short_size as usize]);
        loaded.cpu.pc = loaded.entry_pc;
        loaded.cpu.lr = PPC_HALT_PC;
        loaded.imports[0].dispatcher_target = target;
        loaded.cpu.gpr[3] = light;
        loaded.cpu.gpr[4] = invalid_output_ptr;

        let probe = loaded.run_with_hle_imports(64);

        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(loaded.cpu.gpr[3], 0);
        for offset in 0..short_size {
            assert_eq!(
                loaded.memory.read_u8(invalid_output_ptr + offset),
                Some(sentinel)
            );
        }
    }

    let pef = synthetic_pef_with_library_import(b"QuickDraw\xaa 3D", b"Q3AmbientLight_New");
    let mut loaded = load_pef_application(&pef).unwrap();
    let scratch_ptr = PPC_DATA_BASE + 0x1000;
    let ambient_data_ptr = scratch_ptr;
    let directional_data_ptr = scratch_ptr + 0x40;
    let output_ptr = scratch_ptr + 0x90;
    let color_ptr = scratch_ptr + 0xc0;
    let direction_ptr = scratch_ptr + 0xd0;
    loaded.memory.add_region(scratch_ptr, vec![0; 0x200]);

    let ambient_data = PpcQ3LightData {
        is_on: 1,
        brightness: 0.25,
        color: (0.1, 0.2, 0.3),
    };
    ppc_write_q3_light_data(&mut loaded.memory, ambient_data_ptr, ambient_data).unwrap();
    loaded.cpu.gpr[3] = ambient_data_ptr;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    let ambient_light = loaded.cpu.gpr[3];
    assert_eq!(ambient_light, PPC_Q3_OBJECT_BASE);
    assert_eq!(loaded.q3_objects[0].object_type, PPC_Q3_LIGHT_TYPE_AMBIENT);
    assert_eq!(loaded.q3_objects[0].data_size, PPC_Q3_LIGHT_DATA_SIZE);
    assert_eq!(loaded.q3_lights.len(), 1);
    assert_eq!(loaded.q3_lights[0].data, ambient_data);
    assert_eq!(loaded.q3_lights[0].kind, PpcQ3LightKind::Ambient);

    let mutated_ambient_data = PpcQ3LightData {
        is_on: 0,
        brightness: 0.9,
        color: (0.9, 0.8, 0.7),
    };
    ppc_write_q3_light_data(&mut loaded.memory, ambient_data_ptr, mutated_ambient_data)
        .unwrap();
    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::Q3LightGetType;
    loaded.cpu.gpr[3] = ambient_light;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], PPC_Q3_LIGHT_TYPE_AMBIENT);

    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::Q3LightGetState;
    loaded.cpu.gpr[3] = ambient_light;
    loaded.cpu.gpr[4] = output_ptr;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], 1);
    assert_eq!(loaded.memory.read_u32_be(output_ptr), Some(1));

    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::Q3LightSetState;
    loaded.cpu.gpr[3] = ambient_light;
    loaded.cpu.gpr[4] = 0;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], 1);
    assert_eq!(loaded.q3_lights[0].data.is_on, 0);

    loaded.q3_objects[0].object_type = PPC_Q3_STYLE_TYPE_FILL;
    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::Q3LightSetState;
    loaded.cpu.gpr[3] = ambient_light;
    loaded.cpu.gpr[4] = 1;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], 0);
    assert_eq!(
        loaded.q3_error_state.first_error,
        PPC_Q3_ERROR_INVALID_OBJECT_PARAMETER
    );
    assert_eq!(loaded.q3_lights[0].data.is_on, 0);
    loaded.q3_objects[0].object_type = PPC_Q3_LIGHT_TYPE_AMBIENT;
    loaded.q3_error_state = PpcQ3ErrorState::default();

    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::Q3LightSetBrightness;
    loaded.cpu.gpr[3] = ambient_light;
    loaded.cpu.fpr[1] = 0.75f64.to_bits();

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], 1);
    assert_eq!(loaded.q3_lights[0].data.brightness, 0.75);

    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::Q3LightGetBrightness;
    loaded.cpu.gpr[3] = ambient_light;
    loaded.cpu.gpr[4] = output_ptr;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], 1);
    assert_eq!(ppc_read_f32_be(&mut loaded.memory, output_ptr), Some(0.75));

    ppc_write_q3_color_rgb(&mut loaded.memory, color_ptr, (0.4, 0.5, 0.6)).unwrap();
    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::Q3LightSetColor;
    loaded.cpu.gpr[3] = ambient_light;
    loaded.cpu.gpr[4] = color_ptr;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], 1);
    assert_eq!(loaded.q3_lights[0].data.color, (0.4, 0.5, 0.6));

    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::Q3LightGetColor;
    loaded.cpu.gpr[3] = ambient_light;
    loaded.cpu.gpr[4] = output_ptr;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], 1);
    assert_eq!(
        ppc_read_q3_color_rgb(&mut loaded.memory, output_ptr),
        Some((0.4, 0.5, 0.6))
    );

    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::Q3LightSetData;
    loaded.cpu.gpr[3] = ambient_light;
    loaded.cpu.gpr[4] = ambient_data_ptr;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], 1);
    assert_eq!(loaded.q3_lights[0].data, mutated_ambient_data);

    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::Q3LightGetData;
    loaded.cpu.gpr[3] = ambient_light;
    loaded.cpu.gpr[4] = output_ptr;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], 1);
    assert_eq!(
        ppc_read_q3_light_data(&mut loaded.memory, output_ptr),
        Some(mutated_ambient_data)
    );

    let typed_ambient_data = PpcQ3LightData {
        is_on: 1,
        brightness: 0.6,
        color: (0.2, 0.4, 0.6),
    };
    ppc_write_q3_light_data(&mut loaded.memory, ambient_data_ptr, typed_ambient_data).unwrap();
    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::Q3AmbientLightSetData;
    loaded.cpu.gpr[3] = ambient_light;
    loaded.cpu.gpr[4] = ambient_data_ptr;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], 1);
    assert_eq!(loaded.q3_lights[0].data, typed_ambient_data);

    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::Q3AmbientLightGetData;
    loaded.cpu.gpr[3] = ambient_light;
    loaded.cpu.gpr[4] = output_ptr;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], 1);
    assert_eq!(
        ppc_read_q3_light_data(&mut loaded.memory, output_ptr),
        Some(typed_ambient_data)
    );

    let directional_data = PpcQ3LightData {
        is_on: 1,
        brightness: 0.5,
        color: (0.7, 0.8, 0.9),
    };
    ppc_write_q3_light_data(&mut loaded.memory, directional_data_ptr, directional_data)
        .unwrap();
    loaded
        .memory
        .write_u32_be(directional_data_ptr + 20, 0)
        .unwrap();
    ppc_write_q3_vector3d(
        &mut loaded.memory,
        directional_data_ptr + 24,
        (1.0, 2.0, 3.0),
    )
    .unwrap();
    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::Q3DirectionalLightNew;
    loaded.cpu.gpr[3] = directional_data_ptr;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    let directional_light = loaded.cpu.gpr[3];
    assert_eq!(directional_light, PPC_Q3_OBJECT_BASE + PPC_Q3_OBJECT_STRIDE);
    assert_eq!(
        loaded.q3_objects[1].object_type,
        PPC_Q3_LIGHT_TYPE_DIRECTIONAL
    );
    assert_eq!(
        loaded.q3_objects[1].data_size,
        PPC_Q3_DIRECTIONAL_LIGHT_DATA_SIZE
    );
    assert_eq!(
        loaded.q3_lights[1],
        PpcQ3LightRecord {
            light: directional_light,
            light_type: PPC_Q3_LIGHT_TYPE_DIRECTIONAL,
            data: directional_data,
            kind: PpcQ3LightKind::Directional {
                casts_shadows: 0,
                direction: (1.0, 2.0, 3.0),
            },
        }
    );

    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.imports[0].dispatcher_target =
        PpcImportDispatcherTarget::Q3DirectionalLightSetCastShadowsState;
    loaded.cpu.gpr[3] = ambient_light;
    loaded.cpu.gpr[4] = 1;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], 0);
    assert_eq!(
        loaded.q3_error_state.first_error,
        PPC_Q3_ERROR_INVALID_OBJECT_PARAMETER
    );
    assert_eq!(loaded.q3_lights[0].kind, PpcQ3LightKind::Ambient);
    loaded.q3_error_state = PpcQ3ErrorState::default();

    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.imports[0].dispatcher_target =
        PpcImportDispatcherTarget::Q3DirectionalLightGetCastShadowsState;
    loaded.cpu.gpr[3] = directional_light;
    loaded.cpu.gpr[4] = output_ptr;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], 1);
    assert_eq!(loaded.memory.read_u32_be(output_ptr), Some(0));

    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.imports[0].dispatcher_target =
        PpcImportDispatcherTarget::Q3DirectionalLightSetCastShadowsState;
    loaded.cpu.gpr[3] = directional_light;
    loaded.cpu.gpr[4] = 1;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], 1);
    assert!(matches!(
        loaded.q3_lights[1].kind,
        PpcQ3LightKind::Directional {
            casts_shadows: 1,
            ..
        }
    ));

    ppc_write_q3_vector3d(&mut loaded.memory, direction_ptr, (0.0, -1.0, 0.0)).unwrap();
    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.imports[0].dispatcher_target =
        PpcImportDispatcherTarget::Q3DirectionalLightSetDirection;
    loaded.cpu.gpr[3] = directional_light;
    loaded.cpu.gpr[4] = direction_ptr;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], 1);
    assert!(matches!(
        loaded.q3_lights[1].kind,
        PpcQ3LightKind::Directional {
            direction: (0.0, -1.0, 0.0),
            ..
        }
    ));

    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.imports[0].dispatcher_target =
        PpcImportDispatcherTarget::Q3DirectionalLightGetDirection;
    loaded.cpu.gpr[3] = directional_light;
    loaded.cpu.gpr[4] = output_ptr;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], 1);
    assert_eq!(
        ppc_read_q3_vector3d(&mut loaded.memory, output_ptr),
        Some((0.0, -1.0, 0.0))
    );

    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::Q3DirectionalLightGetData;
    loaded.cpu.gpr[3] = directional_light;
    loaded.cpu.gpr[4] = output_ptr;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], 1);
    assert_eq!(
        ppc_read_q3_directional_light_data(&mut loaded.memory, output_ptr),
        Some((directional_data, 1, (0.0, -1.0, 0.0)))
    );

    let typed_directional_data = PpcQ3LightData {
        is_on: 0,
        brightness: 0.35,
        color: (0.3, 0.2, 0.1),
    };
    ppc_write_q3_light_data(
        &mut loaded.memory,
        directional_data_ptr,
        typed_directional_data,
    )
    .unwrap();
    loaded
        .memory
        .write_u32_be(directional_data_ptr + 20, 0)
        .unwrap();
    ppc_write_q3_vector3d(
        &mut loaded.memory,
        directional_data_ptr + 24,
        (0.0, 1.0, 0.0),
    )
    .unwrap();
    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::Q3DirectionalLightSetData;
    loaded.cpu.gpr[3] = directional_light;
    loaded.cpu.gpr[4] = directional_data_ptr;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], 1);
    assert_eq!(
        loaded.q3_lights[1],
        PpcQ3LightRecord {
            light: directional_light,
            light_type: PPC_Q3_LIGHT_TYPE_DIRECTIONAL,
            data: typed_directional_data,
            kind: PpcQ3LightKind::Directional {
                casts_shadows: 0,
                direction: (0.0, 1.0, 0.0),
            },
        }
    );

    let generic_directional_data = PpcQ3LightData {
        is_on: 1,
        brightness: 0.65,
        color: (0.1, 0.3, 0.5),
    };
    ppc_write_q3_light_data(
        &mut loaded.memory,
        directional_data_ptr,
        generic_directional_data,
    )
    .unwrap();
    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::Q3LightSetData;
    loaded.cpu.gpr[3] = directional_light;
    loaded.cpu.gpr[4] = directional_data_ptr;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], 1);
    assert_eq!(loaded.q3_lights[1].data, generic_directional_data);
    assert!(matches!(
        loaded.q3_lights[1].kind,
        PpcQ3LightKind::Directional {
            casts_shadows: 0,
            direction: (0.0, 1.0, 0.0),
        }
    ));

    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::Q3DirectionalLightGetData;
    loaded.cpu.gpr[3] = directional_light;
    loaded.cpu.gpr[4] = output_ptr;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], 1);
    assert_eq!(
        ppc_read_q3_directional_light_data(&mut loaded.memory, output_ptr),
        Some((generic_directional_data, 0, (0.0, 1.0, 0.0)))
    );

    let invalid_getters = [
        (
            PpcImportDispatcherTarget::Q3LightGetState,
            ambient_light,
            4,
            0xb1u8,
        ),
        (
            PpcImportDispatcherTarget::Q3LightGetBrightness,
            ambient_light,
            4,
            0xb2u8,
        ),
        (
            PpcImportDispatcherTarget::Q3LightGetColor,
            ambient_light,
            PPC_Q3_VECTOR3D_SIZE,
            0xb3u8,
        ),
        (
            PpcImportDispatcherTarget::Q3LightGetData,
            ambient_light,
            PPC_Q3_LIGHT_DATA_SIZE,
            0xb4u8,
        ),
        (
            PpcImportDispatcherTarget::Q3AmbientLightGetData,
            ambient_light,
            PPC_Q3_LIGHT_DATA_SIZE,
            0xb5u8,
        ),
        (
            PpcImportDispatcherTarget::Q3DirectionalLightGetCastShadowsState,
            directional_light,
            4,
            0xb6u8,
        ),
        (
            PpcImportDispatcherTarget::Q3DirectionalLightGetDirection,
            directional_light,
            PPC_Q3_VECTOR3D_SIZE,
            0xb7u8,
        ),
        (
            PpcImportDispatcherTarget::Q3DirectionalLightGetData,
            directional_light,
            PPC_Q3_DIRECTIONAL_LIGHT_DATA_SIZE,
            0xb8u8,
        ),
    ];
    for (index, (target, light, size, sentinel)) in invalid_getters.iter().enumerate() {
        assert_q3_light_getter_short_output_preserved(
            &mut loaded,
            target.clone(),
            *light,
            PPC_DATA_BASE + 0x1400 + (index as u32) * 0x100,
            *size,
            *sentinel,
        );
    }
}

#[test]
fn hle_import_runner_handles_q3_point_and_spot_light_state() {
    fn run_q3_call(loaded: &mut PpcLoadedApp, target: PpcImportDispatcherTarget) -> u32 {
        loaded.cpu.pc = loaded.entry_pc;
        loaded.cpu.lr = PPC_HALT_PC;
        loaded.imports[0].dispatcher_target = target;
        let probe = loaded.run_with_hle_imports(64);
        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        loaded.cpu.gpr[3]
    }

    fn assert_q3_light_getter_short_output_preserved(
        loaded: &mut PpcLoadedApp,
        target: PpcImportDispatcherTarget,
        light: u32,
        invalid_output_ptr: u32,
        size: u32,
        sentinel: u8,
    ) {
        let short_size = size - 1;
        loaded
            .memory
            .add_region(invalid_output_ptr, vec![sentinel; short_size as usize]);
        loaded.cpu.pc = loaded.entry_pc;
        loaded.cpu.lr = PPC_HALT_PC;
        loaded.imports[0].dispatcher_target = target;
        loaded.cpu.gpr[3] = light;
        loaded.cpu.gpr[4] = invalid_output_ptr;

        let probe = loaded.run_with_hle_imports(64);

        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(loaded.cpu.gpr[3], 0);
        for offset in 0..short_size {
            assert_eq!(
                loaded.memory.read_u8(invalid_output_ptr + offset),
                Some(sentinel)
            );
        }
    }

    fn write_point_light_data(
        memory: &mut PpcSectionMem,
        ptr: u32,
        data: PpcQ3LightData,
        casts_shadows: u32,
        attenuation: u32,
        location: (f32, f32, f32),
    ) {
        ppc_write_q3_light_data(memory, ptr, data).unwrap();
        memory.write_u32_be(ptr + 20, casts_shadows).unwrap();
        memory.write_u32_be(ptr + 24, attenuation).unwrap();
        ppc_write_q3_vector3d(memory, ptr + 28, location).unwrap();
    }

    fn write_spot_light_data(
        memory: &mut PpcSectionMem,
        ptr: u32,
        data: PpcQ3LightData,
        casts_shadows: u32,
        attenuation: u32,
        location: (f32, f32, f32),
        direction: (f32, f32, f32),
        hot_angle: f32,
        outer_angle: f32,
        fall_off: u32,
    ) {
        ppc_write_q3_light_data(memory, ptr, data).unwrap();
        memory.write_u32_be(ptr + 20, casts_shadows).unwrap();
        memory.write_u32_be(ptr + 24, attenuation).unwrap();
        ppc_write_q3_vector3d(memory, ptr + 28, location).unwrap();
        ppc_write_q3_vector3d(memory, ptr + 40, direction).unwrap();
        ppc_write_f32_be(memory, ptr + 52, hot_angle).unwrap();
        ppc_write_f32_be(memory, ptr + 56, outer_angle).unwrap();
        memory.write_u32_be(ptr + 60, fall_off).unwrap();
    }

    let pef = synthetic_pef_with_library_import(b"QuickDraw\xaa 3D", b"Q3PointLight_New");
    let mut loaded = load_pef_application(&pef).unwrap();
    let scratch_ptr = PPC_DATA_BASE + 0x1000;
    let point_data_ptr = scratch_ptr;
    let spot_data_ptr = scratch_ptr + 0x80;
    let output_ptr = scratch_ptr + 0x100;
    let vector_ptr = scratch_ptr + 0x140;
    loaded.memory.add_region(scratch_ptr, vec![0; 0x200]);

    let point_data = PpcQ3LightData {
        is_on: 1,
        brightness: 0.8,
        color: (0.9, 0.7, 0.5),
    };
    write_point_light_data(
        &mut loaded.memory,
        point_data_ptr,
        point_data,
        1,
        1,
        (-2.0, 3.0, 4.0),
    );
    loaded.cpu.gpr[3] = point_data_ptr;

    let point_light = run_q3_call(&mut loaded, PpcImportDispatcherTarget::Q3PointLightNew);

    assert_eq!(point_light, PPC_Q3_OBJECT_BASE);
    assert_eq!(loaded.q3_objects[0].object_type, PPC_Q3_LIGHT_TYPE_POINT);
    assert_eq!(loaded.q3_objects[0].data_size, PPC_Q3_POINT_LIGHT_DATA_SIZE);
    assert_eq!(
        loaded.q3_lights[0],
        PpcQ3LightRecord {
            light: point_light,
            light_type: PPC_Q3_LIGHT_TYPE_POINT,
            data: point_data,
            kind: PpcQ3LightKind::Point {
                casts_shadows: 1,
                attenuation: 1,
                location: (-2.0, 3.0, 4.0),
            },
        }
    );

    loaded.cpu.gpr[3] = point_light;
    loaded.cpu.gpr[4] = 0;
    assert_eq!(
        run_q3_call(
            &mut loaded,
            PpcImportDispatcherTarget::Q3PointLightSetCastShadowsState,
        ),
        1
    );
    assert!(matches!(
        loaded.q3_lights[0].kind,
        PpcQ3LightKind::Point {
            casts_shadows: 0,
            ..
        }
    ));

    loaded.cpu.gpr[3] = point_light;
    loaded.cpu.gpr[4] = 2;
    assert_eq!(
        run_q3_call(
            &mut loaded,
            PpcImportDispatcherTarget::Q3PointLightSetAttenuation,
        ),
        1
    );
    loaded.cpu.gpr[3] = point_light;
    loaded.cpu.gpr[4] = output_ptr;
    assert_eq!(
        run_q3_call(
            &mut loaded,
            PpcImportDispatcherTarget::Q3PointLightGetAttenuation,
        ),
        1
    );
    assert_eq!(loaded.memory.read_u32_be(output_ptr), Some(2));

    ppc_write_q3_vector3d(&mut loaded.memory, vector_ptr, (5.0, 6.0, 7.0)).unwrap();
    loaded.cpu.gpr[3] = point_light;
    loaded.cpu.gpr[4] = vector_ptr;
    assert_eq!(
        run_q3_call(
            &mut loaded,
            PpcImportDispatcherTarget::Q3PointLightSetLocation,
        ),
        1
    );
    loaded.cpu.gpr[3] = point_light;
    loaded.cpu.gpr[4] = output_ptr;
    assert_eq!(
        run_q3_call(
            &mut loaded,
            PpcImportDispatcherTarget::Q3PointLightGetLocation,
        ),
        1
    );
    assert_eq!(
        ppc_read_q3_vector3d(&mut loaded.memory, output_ptr),
        Some((5.0, 6.0, 7.0))
    );

    let updated_point_data = PpcQ3LightData {
        is_on: 0,
        brightness: 0.25,
        color: (0.2, 0.4, 0.6),
    };
    write_point_light_data(
        &mut loaded.memory,
        point_data_ptr,
        updated_point_data,
        1,
        0,
        (1.0, 2.0, 3.0),
    );
    loaded.cpu.gpr[3] = point_light;
    loaded.cpu.gpr[4] = point_data_ptr;
    assert_eq!(
        run_q3_call(&mut loaded, PpcImportDispatcherTarget::Q3PointLightSetData),
        1
    );
    loaded.cpu.gpr[3] = point_light;
    loaded.cpu.gpr[4] = output_ptr;
    assert_eq!(
        run_q3_call(&mut loaded, PpcImportDispatcherTarget::Q3PointLightGetData),
        1
    );
    assert_eq!(
        ppc_read_q3_point_light_data(&mut loaded.memory, output_ptr),
        Some((updated_point_data, 1, 0, (1.0, 2.0, 3.0)))
    );

    write_point_light_data(
        &mut loaded.memory,
        point_data_ptr,
        updated_point_data,
        1,
        9,
        (9.0, 9.0, 9.0),
    );
    loaded.cpu.gpr[3] = point_light;
    loaded.cpu.gpr[4] = point_data_ptr;
    assert_eq!(
        run_q3_call(&mut loaded, PpcImportDispatcherTarget::Q3PointLightSetData),
        0
    );
    assert_eq!(
        loaded.q3_lights[0],
        PpcQ3LightRecord {
            light: point_light,
            light_type: PPC_Q3_LIGHT_TYPE_POINT,
            data: updated_point_data,
            kind: PpcQ3LightKind::Point {
                casts_shadows: 1,
                attenuation: 0,
                location: (1.0, 2.0, 3.0),
            },
        }
    );

    loaded.q3_objects[0].object_type = PPC_Q3_LIGHT_TYPE_SPOT;
    loaded.cpu.gpr[3] = point_light;
    loaded.cpu.gpr[4] = 2;
    assert_eq!(
        run_q3_call(
            &mut loaded,
            PpcImportDispatcherTarget::Q3PointLightSetAttenuation,
        ),
        0
    );
    assert_eq!(
        loaded.q3_error_state.first_error,
        PPC_Q3_ERROR_INVALID_OBJECT_PARAMETER
    );
    assert!(matches!(
        loaded.q3_lights[0].kind,
        PpcQ3LightKind::Point { attenuation: 0, .. }
    ));
    loaded.q3_objects[0].object_type = PPC_Q3_LIGHT_TYPE_POINT;
    loaded.q3_error_state = PpcQ3ErrorState::default();

    let spot_data = PpcQ3LightData {
        is_on: 1,
        brightness: 0.6,
        color: (0.3, 0.5, 0.7),
    };
    write_spot_light_data(
        &mut loaded.memory,
        spot_data_ptr,
        spot_data,
        0,
        2,
        (2.0, 3.0, 4.0),
        (0.0, -2.0, 0.0),
        0.2,
        0.6,
        3,
    );
    loaded.cpu.gpr[3] = spot_data_ptr;

    let spot_light = run_q3_call(&mut loaded, PpcImportDispatcherTarget::Q3SpotLightNew);

    assert_eq!(spot_light, PPC_Q3_OBJECT_BASE + PPC_Q3_OBJECT_STRIDE);
    assert_eq!(loaded.q3_objects[1].object_type, PPC_Q3_LIGHT_TYPE_SPOT);
    assert_eq!(loaded.q3_objects[1].data_size, PPC_Q3_SPOT_LIGHT_DATA_SIZE);
    assert_eq!(
        loaded.q3_lights[1],
        PpcQ3LightRecord {
            light: spot_light,
            light_type: PPC_Q3_LIGHT_TYPE_SPOT,
            data: spot_data,
            kind: PpcQ3LightKind::Spot {
                casts_shadows: 0,
                attenuation: 2,
                location: (2.0, 3.0, 4.0),
                direction: (0.0, -2.0, 0.0),
                hot_angle: 0.2,
                outer_angle: 0.6,
                fall_off: 3,
            },
        }
    );

    loaded.cpu.gpr[3] = spot_light;
    loaded.cpu.gpr[4] = 1;
    assert_eq!(
        run_q3_call(
            &mut loaded,
            PpcImportDispatcherTarget::Q3SpotLightSetCastShadowsState,
        ),
        1
    );
    loaded.cpu.gpr[3] = spot_light;
    loaded.cpu.gpr[4] = 1;
    assert_eq!(
        run_q3_call(
            &mut loaded,
            PpcImportDispatcherTarget::Q3SpotLightSetAttenuation,
        ),
        1
    );

    ppc_write_q3_vector3d(&mut loaded.memory, vector_ptr, (8.0, 9.0, 10.0)).unwrap();
    loaded.cpu.gpr[3] = spot_light;
    loaded.cpu.gpr[4] = vector_ptr;
    assert_eq!(
        run_q3_call(
            &mut loaded,
            PpcImportDispatcherTarget::Q3SpotLightSetLocation,
        ),
        1
    );
    ppc_write_q3_vector3d(&mut loaded.memory, vector_ptr, (1.0, 0.0, 0.0)).unwrap();
    loaded.cpu.gpr[3] = spot_light;
    loaded.cpu.gpr[4] = vector_ptr;
    assert_eq!(
        run_q3_call(
            &mut loaded,
            PpcImportDispatcherTarget::Q3SpotLightSetDirection,
        ),
        1
    );
    loaded.cpu.gpr[3] = spot_light;
    loaded.cpu.fpr[1] = (0.3f64).to_bits();
    assert_eq!(
        run_q3_call(
            &mut loaded,
            PpcImportDispatcherTarget::Q3SpotLightSetHotAngle
        ),
        1
    );
    loaded.cpu.gpr[3] = spot_light;
    loaded.cpu.fpr[1] = (0.7f64).to_bits();
    assert_eq!(
        run_q3_call(
            &mut loaded,
            PpcImportDispatcherTarget::Q3SpotLightSetOuterAngle,
        ),
        1
    );
    loaded.cpu.gpr[3] = spot_light;
    loaded.cpu.gpr[4] = 2;
    assert_eq!(
        run_q3_call(
            &mut loaded,
            PpcImportDispatcherTarget::Q3SpotLightSetFallOff
        ),
        1
    );

    loaded.cpu.gpr[3] = spot_light;
    loaded.cpu.gpr[4] = output_ptr;
    assert_eq!(
        run_q3_call(
            &mut loaded,
            PpcImportDispatcherTarget::Q3SpotLightGetCastShadowsState,
        ),
        1
    );
    assert_eq!(loaded.memory.read_u32_be(output_ptr), Some(1));
    loaded.cpu.gpr[3] = spot_light;
    loaded.cpu.gpr[4] = output_ptr;
    assert_eq!(
        run_q3_call(
            &mut loaded,
            PpcImportDispatcherTarget::Q3SpotLightGetAttenuation,
        ),
        1
    );
    assert_eq!(loaded.memory.read_u32_be(output_ptr), Some(1));
    loaded.cpu.gpr[3] = spot_light;
    loaded.cpu.gpr[4] = output_ptr;
    assert_eq!(
        run_q3_call(
            &mut loaded,
            PpcImportDispatcherTarget::Q3SpotLightGetLocation,
        ),
        1
    );
    assert_eq!(
        ppc_read_q3_vector3d(&mut loaded.memory, output_ptr),
        Some((8.0, 9.0, 10.0))
    );
    loaded.cpu.gpr[3] = spot_light;
    loaded.cpu.gpr[4] = output_ptr;
    assert_eq!(
        run_q3_call(
            &mut loaded,
            PpcImportDispatcherTarget::Q3SpotLightGetDirection,
        ),
        1
    );
    assert_eq!(
        ppc_read_q3_vector3d(&mut loaded.memory, output_ptr),
        Some((1.0, 0.0, 0.0))
    );
    loaded.cpu.gpr[3] = spot_light;
    loaded.cpu.gpr[4] = output_ptr;
    assert_eq!(
        run_q3_call(
            &mut loaded,
            PpcImportDispatcherTarget::Q3SpotLightGetHotAngle
        ),
        1
    );
    assert_eq!(ppc_read_f32_be(&mut loaded.memory, output_ptr), Some(0.3));
    loaded.cpu.gpr[3] = spot_light;
    loaded.cpu.gpr[4] = output_ptr;
    assert_eq!(
        run_q3_call(
            &mut loaded,
            PpcImportDispatcherTarget::Q3SpotLightGetOuterAngle,
        ),
        1
    );
    assert_eq!(ppc_read_f32_be(&mut loaded.memory, output_ptr), Some(0.7));
    loaded.cpu.gpr[3] = spot_light;
    loaded.cpu.gpr[4] = output_ptr;
    assert_eq!(
        run_q3_call(
            &mut loaded,
            PpcImportDispatcherTarget::Q3SpotLightGetFallOff
        ),
        1
    );
    assert_eq!(loaded.memory.read_u32_be(output_ptr), Some(2));

    let updated_spot_data = PpcQ3LightData {
        is_on: 0,
        brightness: 0.4,
        color: (0.8, 0.6, 0.4),
    };
    write_spot_light_data(
        &mut loaded.memory,
        spot_data_ptr,
        updated_spot_data,
        1,
        0,
        (-1.0, -2.0, -3.0),
        (0.0, 1.0, 1.0),
        0.1,
        0.5,
        1,
    );
    loaded.cpu.gpr[3] = spot_light;
    loaded.cpu.gpr[4] = spot_data_ptr;
    assert_eq!(
        run_q3_call(&mut loaded, PpcImportDispatcherTarget::Q3SpotLightSetData),
        1
    );
    loaded.cpu.gpr[3] = spot_light;
    loaded.cpu.gpr[4] = output_ptr;
    assert_eq!(
        run_q3_call(&mut loaded, PpcImportDispatcherTarget::Q3SpotLightGetData),
        1
    );
    assert_eq!(
        ppc_read_q3_spot_light_data(&mut loaded.memory, output_ptr),
        Some((
            updated_spot_data,
            1,
            0,
            (-1.0, -2.0, -3.0),
            (0.0, 1.0, 1.0),
            0.1,
            0.5,
            1,
        ))
    );

    loaded.q3_objects[1].object_type = PPC_Q3_LIGHT_TYPE_POINT;
    loaded.cpu.gpr[3] = spot_light;
    loaded.cpu.gpr[4] = 3;
    assert_eq!(
        run_q3_call(
            &mut loaded,
            PpcImportDispatcherTarget::Q3SpotLightSetFallOff
        ),
        0
    );
    assert_eq!(
        loaded.q3_error_state.first_error,
        PPC_Q3_ERROR_INVALID_OBJECT_PARAMETER
    );
    assert!(matches!(
        loaded.q3_lights[1].kind,
        PpcQ3LightKind::Spot { fall_off: 1, .. }
    ));
    loaded.q3_objects[1].object_type = PPC_Q3_LIGHT_TYPE_SPOT;
    loaded.q3_error_state = PpcQ3ErrorState::default();

    let invalid_getters = [
        (
            PpcImportDispatcherTarget::Q3PointLightGetCastShadowsState,
            point_light,
            4,
            0xc1u8,
        ),
        (
            PpcImportDispatcherTarget::Q3PointLightGetAttenuation,
            point_light,
            4,
            0xc2u8,
        ),
        (
            PpcImportDispatcherTarget::Q3PointLightGetLocation,
            point_light,
            PPC_Q3_VECTOR3D_SIZE,
            0xc3u8,
        ),
        (
            PpcImportDispatcherTarget::Q3PointLightGetData,
            point_light,
            PPC_Q3_POINT_LIGHT_DATA_SIZE,
            0xc4u8,
        ),
        (
            PpcImportDispatcherTarget::Q3SpotLightGetCastShadowsState,
            spot_light,
            4,
            0xc5u8,
        ),
        (
            PpcImportDispatcherTarget::Q3SpotLightGetAttenuation,
            spot_light,
            4,
            0xc6u8,
        ),
        (
            PpcImportDispatcherTarget::Q3SpotLightGetLocation,
            spot_light,
            PPC_Q3_VECTOR3D_SIZE,
            0xc7u8,
        ),
        (
            PpcImportDispatcherTarget::Q3SpotLightGetDirection,
            spot_light,
            PPC_Q3_VECTOR3D_SIZE,
            0xc8u8,
        ),
        (
            PpcImportDispatcherTarget::Q3SpotLightGetHotAngle,
            spot_light,
            4,
            0xc9u8,
        ),
        (
            PpcImportDispatcherTarget::Q3SpotLightGetOuterAngle,
            spot_light,
            4,
            0xcau8,
        ),
        (
            PpcImportDispatcherTarget::Q3SpotLightGetFallOff,
            spot_light,
            4,
            0xcbu8,
        ),
        (
            PpcImportDispatcherTarget::Q3SpotLightGetData,
            spot_light,
            PPC_Q3_SPOT_LIGHT_DATA_SIZE,
            0xccu8,
        ),
    ];
    for (index, (target, light, size, sentinel)) in invalid_getters.iter().enumerate() {
        assert_q3_light_getter_short_output_preserved(
            &mut loaded,
            target.clone(),
            *light,
            PPC_DATA_BASE + 0x1400 + (index as u32) * 0x100,
            *size,
            *sentinel,
        );
    }

    loaded.cpu.gpr[3] = spot_light;
    loaded.cpu.fpr[1] = (0.05f64).to_bits();
    assert_eq!(
        run_q3_call(
            &mut loaded,
            PpcImportDispatcherTarget::Q3SpotLightSetOuterAngle,
        ),
        0
    );
    assert!(matches!(
        loaded.q3_lights[1].kind,
        PpcQ3LightKind::Spot {
            outer_angle: 0.5,
            ..
        }
    ));
}

#[test]
fn hle_import_runner_records_q3_submit_command_order() {
    let view = PPC_Q3_OBJECT_BASE;
    let shader = PPC_Q3_OBJECT_BASE + PPC_Q3_OBJECT_STRIDE;
    let object = PPC_Q3_OBJECT_BASE + PPC_Q3_OBJECT_STRIDE * 2;
    let pef = synthetic_pef_with_library_import(b"QuickDraw\xaa 3D", b"Q3Shader_Submit");
    let mut loaded = load_pef_application(&pef).unwrap();
    loaded
        .q3_objects
        .push(test_q3_object(view, PPC_Q3_TYPE_VIEW));
    loaded
        .q3_objects
        .push(test_q3_object(shader, PPC_Q3_SURFACE_SHADER_TYPE_TEXTURE));
    loaded
        .q3_objects
        .push(test_q3_object(object, PPC_Q3_TYPE_CONTAINER));
    loaded.cpu.gpr[3] = shader;
    loaded.cpu.gpr[4] = view;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], 1);
    assert_eq!(loaded.q3_views[0].view, view);
    assert_eq!(
        loaded.q3_submissions,
        vec![PpcQ3SubmissionRecord {
            view,
            kind: PpcQ3SubmissionKind::Shader,
            primary: shader,
            secondary: 0,
        }]
    );

    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.cpu.gpr[3] = object;
    loaded.cpu.gpr[4] = view;
    loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::Q3ObjectSubmit;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], 1);
    assert_eq!(
        loaded.q3_submissions,
        vec![
            PpcQ3SubmissionRecord {
                view,
                kind: PpcQ3SubmissionKind::Shader,
                primary: shader,
                secondary: 0,
            },
            PpcQ3SubmissionRecord {
                view,
                kind: PpcQ3SubmissionKind::Object,
                primary: object,
                secondary: 0,
            },
        ]
    );

    let submission_count = loaded.q3_submissions.len();
    for (target, primary) in [
        (
            PpcImportDispatcherTarget::Q3ShaderSubmit,
            view + PPC_Q3_OBJECT_STRIDE * 10,
        ),
        (
            PpcImportDispatcherTarget::Q3StyleSubmit,
            view + PPC_Q3_OBJECT_STRIDE * 11,
        ),
        (PpcImportDispatcherTarget::Q3ObjectSubmit, view),
    ] {
        loaded.q3_error_state = PpcQ3ErrorState::default();
        loaded.cpu.pc = loaded.entry_pc;
        loaded.cpu.lr = PPC_HALT_PC;
        loaded.imports[0].dispatcher_target = target;
        loaded.cpu.gpr[3] = primary;
        loaded.cpu.gpr[4] = view;

        let probe = loaded.run_with_hle_imports(64);

        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(loaded.cpu.gpr[3], 0);
        assert_eq!(loaded.q3_submissions.len(), submission_count);
        assert_eq!(
            loaded.q3_error_state.first_error,
            PPC_Q3_ERROR_INVALID_OBJECT_PARAMETER
        );
    }
}

#[test]
fn hle_import_runner_expands_direct_q3_object_trimesh_submission() {
    let view = PPC_Q3_OBJECT_BASE;
    let trimesh = PPC_Q3_OBJECT_BASE + PPC_Q3_OBJECT_STRIDE;
    let pef = synthetic_pef_with_library_import(b"QuickDraw\xaa 3D", b"Q3Object_Submit");
    let mut loaded = load_pef_application(&pef).unwrap();
    loaded
        .q3_objects
        .push(test_q3_object(view, PPC_Q3_TYPE_VIEW));
    loaded
        .q3_objects
        .push(test_q3_object(trimesh, PPC_Q3_TYPE_TRIMESH));
    loaded.cpu.gpr[3] = trimesh;
    loaded.cpu.gpr[4] = view;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], 1);
    assert_eq!(
        loaded.q3_submissions,
        vec![
            PpcQ3SubmissionRecord {
                view,
                kind: PpcQ3SubmissionKind::Object,
                primary: trimesh,
                secondary: 0,
            },
            PpcQ3SubmissionRecord {
                view,
                kind: PpcQ3SubmissionKind::TriMesh,
                primary: trimesh,
                secondary: 0,
            },
        ]
    );
    assert_eq!(loaded.q3_submission_transforms.len(), 2);
    assert_eq!(loaded.q3_submission_materials.len(), 2);
    assert_eq!(loaded.q3_submission_lights.len(), 2);
}

#[test]
fn hle_import_runner_submits_illumination_shader_without_replacing_texture_shader() {
    let view = PPC_Q3_OBJECT_BASE;
    let texture_shader = PPC_Q3_OBJECT_BASE + PPC_Q3_OBJECT_STRIDE;
    let null_shader = PPC_Q3_OBJECT_BASE + PPC_Q3_OBJECT_STRIDE * 2;
    let trimesh = PPC_Q3_OBJECT_BASE + PPC_Q3_OBJECT_STRIDE * 3;
    let pef = synthetic_pef_with_library_import(b"QuickDraw\xaa 3D", b"Q3Shader_Submit");
    let mut loaded = load_pef_application(&pef).unwrap();
    loaded
        .q3_objects
        .push(test_q3_object(view, PPC_Q3_TYPE_VIEW));
    loaded.q3_objects.push(PpcQ3ObjectRecord {
        object: texture_shader,
        kind: PpcQ3ObjectKind::Generic,
        object_type: PPC_Q3_SURFACE_SHADER_TYPE_TEXTURE,
        source: PpcQ3ObjectSource::default(),
        data_ptr: 0,
        data_size: 0,
    });
    loaded.q3_objects.push(PpcQ3ObjectRecord {
        object: null_shader,
        kind: PpcQ3ObjectKind::Generic,
        object_type: PPC_Q3_ILLUMINATION_TYPE_NULL,
        source: PpcQ3ObjectSource::default(),
        data_ptr: 0,
        data_size: 0,
    });
    loaded.cpu.gpr[3] = texture_shader;
    loaded.cpu.gpr[4] = view;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], 1);
    assert_eq!(loaded.q3_view_materials[0].shader, texture_shader);
    assert_eq!(
        loaded.q3_view_materials[0].illumination_type,
        PPC_Q3_ILLUMINATION_TYPE_PHONG
    );

    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.cpu.gpr[3] = null_shader;
    loaded.cpu.gpr[4] = view;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], 1);
    assert_eq!(loaded.q3_view_materials[0].shader, texture_shader);
    assert_eq!(
        loaded.q3_view_materials[0].illumination_type,
        PPC_Q3_ILLUMINATION_TYPE_NULL
    );

    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::Q3TriMeshSubmit;
    loaded.cpu.gpr[3] = trimesh;
    loaded.cpu.gpr[4] = view;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], 1);
    assert_eq!(loaded.q3_submission_materials[2].shader, texture_shader);
    assert_eq!(
        loaded.q3_submission_materials[2].illumination_type,
        PPC_Q3_ILLUMINATION_TYPE_NULL
    );
}

#[test]
fn hle_import_runner_snapshots_q3_light_group_for_submissions() {
    let view = PPC_Q3_OBJECT_BASE;
    let light_group = PPC_Q3_OBJECT_BASE + PPC_Q3_OBJECT_STRIDE;
    let ambient_light = PPC_Q3_OBJECT_BASE + PPC_Q3_OBJECT_STRIDE * 2;
    let non_light = PPC_Q3_OBJECT_BASE + PPC_Q3_OBJECT_STRIDE * 3;
    let directional_light = PPC_Q3_OBJECT_BASE + PPC_Q3_OBJECT_STRIDE * 4;
    let trimesh = PPC_Q3_OBJECT_BASE + PPC_Q3_OBJECT_STRIDE * 5;
    let pef = synthetic_pef_with_library_import(b"QuickDraw\xaa 3D", b"Q3View_SetLightGroup");
    let mut loaded = load_pef_application(&pef).unwrap();
    let ambient = PpcQ3LightRecord {
        light: ambient_light,
        light_type: PPC_Q3_LIGHT_TYPE_AMBIENT,
        data: PpcQ3LightData {
            is_on: 1,
            brightness: 0.4,
            color: (0.2, 0.3, 0.4),
        },
        kind: PpcQ3LightKind::Ambient,
    };
    let directional = PpcQ3LightRecord {
        light: directional_light,
        light_type: PPC_Q3_LIGHT_TYPE_DIRECTIONAL,
        data: PpcQ3LightData {
            is_on: 1,
            brightness: 0.8,
            color: (0.9, 0.8, 0.7),
        },
        kind: PpcQ3LightKind::Directional {
            casts_shadows: 1,
            direction: (0.0, -1.0, 0.0),
        },
    };
    loaded.q3_lights.push(ambient);
    loaded.q3_lights.push(directional);
    loaded.q3_objects.extend([
        PpcQ3ObjectRecord {
            object: view,
            kind: PpcQ3ObjectKind::Generic,
            object_type: PPC_Q3_TYPE_VIEW,
            source: PpcQ3ObjectSource::default(),
            data_ptr: 0,
            data_size: 0,
        },
        PpcQ3ObjectRecord {
            object: light_group,
            kind: PpcQ3ObjectKind::Generic,
            object_type: PPC_Q3_GROUP_TYPE_LIGHT,
            source: PpcQ3ObjectSource::default(),
            data_ptr: 0,
            data_size: 0,
        },
        PpcQ3ObjectRecord {
            object: ambient_light,
            kind: PpcQ3ObjectKind::Generic,
            object_type: PPC_Q3_LIGHT_TYPE_AMBIENT,
            source: PpcQ3ObjectSource::default(),
            data_ptr: 0,
            data_size: PPC_Q3_LIGHT_DATA_SIZE,
        },
        PpcQ3ObjectRecord {
            object: non_light,
            kind: PpcQ3ObjectKind::Generic,
            object_type: PPC_Q3_TYPE_TRIMESH,
            source: PpcQ3ObjectSource::default(),
            data_ptr: 0,
            data_size: PPC_Q3_TRIMESH_DATA_SIZE,
        },
        PpcQ3ObjectRecord {
            object: directional_light,
            kind: PpcQ3ObjectKind::Generic,
            object_type: PPC_Q3_LIGHT_TYPE_DIRECTIONAL,
            source: PpcQ3ObjectSource::default(),
            data_ptr: 0,
            data_size: PPC_Q3_LIGHT_DATA_SIZE,
        },
        PpcQ3ObjectRecord {
            object: trimesh,
            kind: PpcQ3ObjectKind::Generic,
            object_type: PPC_Q3_TYPE_TRIMESH,
            source: PpcQ3ObjectSource::default(),
            data_ptr: 0,
            data_size: PPC_Q3_TRIMESH_DATA_SIZE,
        },
    ]);
    loaded
        .q3_group_memberships
        .push(PpcQ3GroupMembershipRecord {
            group: light_group,
            object: non_light,
            before: None,
        });
    loaded
        .q3_group_memberships
        .push(PpcQ3GroupMembershipRecord {
            group: light_group,
            object: ambient_light,
            before: None,
        });
    loaded
        .q3_group_memberships
        .push(PpcQ3GroupMembershipRecord {
            group: light_group,
            object: directional_light,
            before: None,
        });
    loaded.cpu.gpr[3] = view;
    loaded.cpu.gpr[4] = light_group;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], 1);
    assert_eq!(loaded.q3_views[0].light_group, light_group);

    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::Q3TriMeshSubmit;
    loaded.cpu.gpr[3] = trimesh;
    loaded.cpu.gpr[4] = view;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], 1);
    assert_eq!(
        loaded.q3_submission_lights,
        vec![PpcQ3SubmissionLightRecord {
            view,
            kind: PpcQ3SubmissionKind::TriMesh,
            primary: trimesh,
            secondary: 0,
            light_group,
            lights: vec![ambient, directional],
        }]
    );
    loaded.q3_lights[0].data.brightness = 1.0;
    assert_eq!(
        loaded.q3_submission_lights[0].lights[0].data.brightness,
        0.4
    );
}

#[test]
fn hle_import_runner_snapshots_q3_material_state_for_submissions() {
    let view = PPC_Q3_OBJECT_BASE;
    let shader = PPC_Q3_OBJECT_BASE + PPC_Q3_OBJECT_STRIDE;
    let style = PPC_Q3_OBJECT_BASE + PPC_Q3_OBJECT_STRIDE * 2;
    let trimesh = PPC_Q3_OBJECT_BASE + PPC_Q3_OBJECT_STRIDE * 3;
    let texture = PPC_Q3_OBJECT_BASE + PPC_Q3_OBJECT_STRIDE * 4;
    let fog_data_ptr = PPC_DATA_BASE + 0x1000;
    let pef = synthetic_pef_with_library_import(b"QuickDraw\xaa 3D", b"Q3Shader_Submit");
    let mut loaded = load_pef_application(&pef).unwrap();
    let mipmap: Vec<u8> = (0..PPC_Q3_MIPMAP_COPY_SIZE)
        .map(|offset| 0x20u8.wrapping_add(offset as u8))
        .collect();
    let fill_style = PpcQ3StyleRecord {
        style,
        kind: PpcQ3StyleKind::Fill,
        value: 2,
    };
    let fog_style = PpcQ3FogStyleData {
        state: 1,
        mode: PPC_Q3_FOG_MODE_LINEAR,
        fog_start: 12.0,
        fog_end: 96.0,
        density: 0.75,
        color: (1.0, 0.2, 0.3, 0.4),
    };
    let texture_shader = PpcQ3TextureShaderRecord { shader, texture };
    let mipmap_texture = PpcQ3MipmapTextureRecord {
        texture,
        mipmap: mipmap.clone(),
    };
    let shader_boundary = PpcQ3ShaderBoundaryRecord {
        shader,
        u_boundary: PPC_Q3_SHADER_UV_BOUNDARY_CLAMP,
        v_boundary: PPC_Q3_SHADER_UV_BOUNDARY_WRAP,
    };
    let shader_uv_transform = PpcQ3ShaderUvTransformRecord {
        shader,
        matrix: [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.25, 0.5, 1.0]],
    };
    loaded.q3_shader_boundaries.push(shader_boundary);
    loaded.q3_shader_uv_transforms.push(shader_uv_transform);
    loaded.q3_texture_shaders.push(texture_shader);
    loaded.q3_mipmap_textures.push(mipmap_texture.clone());
    loaded.q3_styles.push(fill_style);
    loaded
        .q3_objects
        .push(test_q3_object(view, PPC_Q3_TYPE_VIEW));
    loaded
        .q3_objects
        .push(test_q3_object(shader, PPC_Q3_SURFACE_SHADER_TYPE_TEXTURE));
    loaded
        .q3_objects
        .push(test_q3_object(style, PPC_Q3_STYLE_TYPE_FILL));
    loaded
        .memory
        .add_region(fog_data_ptr, vec![0; PPC_Q3_FOG_STYLE_DATA_SIZE as usize]);
    loaded
        .memory
        .write_u32_be(fog_data_ptr, fog_style.state)
        .unwrap();
    loaded
        .memory
        .write_u32_be(fog_data_ptr + 4, fog_style.mode)
        .unwrap();
    ppc_write_f32_be(&mut loaded.memory, fog_data_ptr + 8, fog_style.fog_start).unwrap();
    ppc_write_f32_be(&mut loaded.memory, fog_data_ptr + 12, fog_style.fog_end).unwrap();
    ppc_write_f32_be(&mut loaded.memory, fog_data_ptr + 16, fog_style.density).unwrap();
    ppc_write_f32_be(&mut loaded.memory, fog_data_ptr + 20, fog_style.color.0).unwrap();
    ppc_write_f32_be(&mut loaded.memory, fog_data_ptr + 24, fog_style.color.1).unwrap();
    ppc_write_f32_be(&mut loaded.memory, fog_data_ptr + 28, fog_style.color.2).unwrap();
    ppc_write_f32_be(&mut loaded.memory, fog_data_ptr + 32, fog_style.color.3).unwrap();
    loaded.cpu.gpr[3] = shader;
    loaded.cpu.gpr[4] = view;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], 1);
    assert_eq!(
        loaded.q3_view_materials,
        vec![PpcQ3ViewMaterialRecord {
            view,
            shader,
            illumination_type: PPC_Q3_ILLUMINATION_TYPE_PHONG,
            styles: Vec::new(),
            fog_style: None,
            attributes: Vec::new(),
        }]
    );
    assert_eq!(
        loaded.q3_submission_materials,
        vec![PpcQ3SubmissionMaterialRecord {
            view,
            kind: PpcQ3SubmissionKind::Shader,
            primary: shader,
            secondary: 0,
            shader,
            illumination_type: PPC_Q3_ILLUMINATION_TYPE_PHONG,
            styles: Vec::new(),
            fog_style: None,
            attributes: Vec::new(),
            shader_uv_transform: Some(shader_uv_transform),
            shader_boundary: Some(shader_boundary),
            texture_shader: Some(texture_shader),
            mipmap_texture: Some(mipmap_texture.clone()),
        }]
    );
    loaded.q3_mipmap_textures[0].mipmap[0] = 0xff;
    assert_eq!(
        loaded.q3_submission_materials[0]
            .mipmap_texture
            .as_ref()
            .map(|record| record.mipmap.as_slice()),
        Some(mipmap.as_slice())
    );
    loaded.q3_shader_uv_transforms[0].matrix[2][0] = 9.0;
    assert_eq!(
        loaded.q3_submission_materials[0]
            .shader_uv_transform
            .map(|record| record.matrix),
        Some(shader_uv_transform.matrix)
    );
    loaded.q3_shader_uv_transforms[0] = shader_uv_transform;
    loaded.q3_mipmap_textures[0] = mipmap_texture.clone();

    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::Q3StyleSubmit;
    loaded.cpu.gpr[3] = style;
    loaded.cpu.gpr[4] = view;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], 1);
    assert_eq!(
        loaded.q3_view_materials[0],
        PpcQ3ViewMaterialRecord {
            view,
            shader,
            illumination_type: PPC_Q3_ILLUMINATION_TYPE_PHONG,
            styles: vec![fill_style],
            fog_style: None,
            attributes: Vec::new(),
        }
    );

    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::Q3FogStyleSubmit;
    loaded.cpu.gpr[3] = fog_data_ptr;
    loaded.cpu.gpr[4] = view;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], 1);
    assert_eq!(loaded.q3_view_materials[0].fog_style, Some(fog_style));
    ppc_write_f32_be(&mut loaded.memory, fog_data_ptr + 8, 999.0).unwrap();

    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::Q3TriMeshSubmit;
    loaded.cpu.gpr[3] = trimesh;
    loaded.cpu.gpr[4] = view;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], 1);
    assert_eq!(
        loaded.q3_submission_materials[3],
        PpcQ3SubmissionMaterialRecord {
            view,
            kind: PpcQ3SubmissionKind::TriMesh,
            primary: trimesh,
            secondary: 0,
            shader,
            illumination_type: PPC_Q3_ILLUMINATION_TYPE_PHONG,
            styles: vec![fill_style],
            fog_style: Some(fog_style),
            attributes: Vec::new(),
            shader_uv_transform: Some(shader_uv_transform),
            shader_boundary: Some(shader_boundary),
            texture_shader: Some(texture_shader),
            mipmap_texture: Some(mipmap_texture),
        }
    );
}

#[test]
fn hle_import_runner_snapshots_q3_object_group_attributes_for_child_trimeshes() {
    let view = PPC_Q3_OBJECT_BASE;
    let group = PPC_Q3_OBJECT_BASE + PPC_Q3_OBJECT_STRIDE;
    let attribute_set = PPC_Q3_OBJECT_BASE + PPC_Q3_OBJECT_STRIDE * 2;
    let trimesh = PPC_Q3_OBJECT_BASE + PPC_Q3_OBJECT_STRIDE * 3;
    let shader = PPC_Q3_OBJECT_BASE + PPC_Q3_OBJECT_STRIDE * 4;
    let texture = PPC_Q3_OBJECT_BASE + PPC_Q3_OBJECT_STRIDE * 5;
    let pef = synthetic_pef_with_library_import(b"QuickDraw\xaa 3D", b"Q3Object_Submit");
    let mut loaded = load_pef_application(&pef).unwrap();
    let diffuse = vec![
        0x3d, 0xcc, 0xcc, 0xcd, 0x3e, 0x4c, 0xcc, 0xcd, 0x3e, 0x99, 0x99, 0x9a,
    ];
    let surface_shader = shader.to_be_bytes().to_vec();
    let mipmap = vec![0x10, 0x20, 0x30, 0x40];
    let texture_shader = PpcQ3TextureShaderRecord { shader, texture };
    let mipmap_texture = PpcQ3MipmapTextureRecord {
        texture,
        mipmap: mipmap.clone(),
    };
    let shader_boundary = PpcQ3ShaderBoundaryRecord {
        shader,
        u_boundary: PPC_Q3_SHADER_UV_BOUNDARY_WRAP,
        v_boundary: PPC_Q3_SHADER_UV_BOUNDARY_CLAMP,
    };
    loaded
        .q3_objects
        .push(test_q3_object(view, PPC_Q3_TYPE_VIEW));
    loaded.q3_objects.push(PpcQ3ObjectRecord {
        object: group,
        kind: PpcQ3ObjectKind::Generic,
        object_type: PPC_Q3_TYPE_CONTAINER,
        source: PpcQ3ObjectSource::default(),
        data_ptr: 0,
        data_size: 0,
    });
    loaded.q3_objects.push(PpcQ3ObjectRecord {
        object: attribute_set,
        kind: PpcQ3ObjectKind::Generic,
        object_type: PPC_Q3_TYPE_ATTRIBUTE_SET,
        source: PpcQ3ObjectSource::default(),
        data_ptr: 0,
        data_size: 0,
    });
    loaded.q3_objects.push(PpcQ3ObjectRecord {
        object: trimesh,
        kind: PpcQ3ObjectKind::Generic,
        object_type: PPC_Q3_TYPE_TRIMESH,
        source: PpcQ3ObjectSource::default(),
        data_ptr: 0,
        data_size: PPC_Q3_TRIMESH_DATA_SIZE,
    });
    loaded
        .q3_group_memberships
        .push(PpcQ3GroupMembershipRecord {
            group,
            object: attribute_set,
            before: None,
        });
    loaded
        .q3_group_memberships
        .push(PpcQ3GroupMembershipRecord {
            group,
            object: trimesh,
            before: None,
        });
    loaded.q3_attributes.push(PpcQ3AttributeRecord {
        attribute_set,
        attribute_type: PPC_Q3_ATTRIBUTE_TYPE_DIFFUSE_COLOR,
        data: diffuse.clone(),
    });
    loaded.q3_attributes.push(PpcQ3AttributeRecord {
        attribute_set,
        attribute_type: PPC_Q3_ATTRIBUTE_TYPE_SURFACE_SHADER,
        data: surface_shader.clone(),
    });
    loaded.q3_shader_boundaries.push(shader_boundary);
    loaded.q3_texture_shaders.push(texture_shader);
    loaded.q3_mipmap_textures.push(mipmap_texture.clone());
    loaded.cpu.gpr[3] = group;
    loaded.cpu.gpr[4] = view;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], 1);
    assert_eq!(
        loaded.q3_submissions,
        vec![
            PpcQ3SubmissionRecord {
                view,
                kind: PpcQ3SubmissionKind::Object,
                primary: group,
                secondary: 0,
            },
            PpcQ3SubmissionRecord {
                view,
                kind: PpcQ3SubmissionKind::TriMesh,
                primary: trimesh,
                secondary: 0,
            },
        ]
    );
    assert_eq!(
        loaded.q3_submission_materials[1],
        PpcQ3SubmissionMaterialRecord {
            view,
            kind: PpcQ3SubmissionKind::TriMesh,
            primary: trimesh,
            secondary: 0,
            shader,
            illumination_type: PPC_Q3_ILLUMINATION_TYPE_PHONG,
            styles: Vec::new(),
            fog_style: None,
            attributes: vec![
                PpcQ3AttributeRecord {
                    attribute_set,
                    attribute_type: PPC_Q3_ATTRIBUTE_TYPE_DIFFUSE_COLOR,
                    data: diffuse.clone(),
                },
                PpcQ3AttributeRecord {
                    attribute_set,
                    attribute_type: PPC_Q3_ATTRIBUTE_TYPE_SURFACE_SHADER,
                    data: surface_shader,
                },
            ],
            shader_uv_transform: None,
            shader_boundary: Some(shader_boundary),
            texture_shader: Some(texture_shader),
            mipmap_texture: Some(mipmap_texture),
        }
    );
    loaded.q3_attributes[0].data[0] = 0xff;
    loaded.q3_shader_boundaries[0].v_boundary = PPC_Q3_SHADER_UV_BOUNDARY_WRAP;
    loaded.q3_mipmap_textures[0].mipmap[0] = 0xff;
    assert_eq!(
        loaded.q3_submission_materials[1].attributes[0].data,
        diffuse
    );
    assert_eq!(
        loaded.q3_submission_materials[1]
            .shader_boundary
            .map(|record| record.v_boundary),
        Some(PPC_Q3_SHADER_UV_BOUNDARY_CLAMP)
    );
    assert_eq!(
        loaded.q3_submission_materials[1]
            .mipmap_texture
            .as_ref()
            .map(|record| record.mipmap.as_slice()),
        Some(mipmap.as_slice())
    );
}

#[test]
fn hle_import_runner_merges_q3_trimesh_attributes_with_group_material() {
    let view = PPC_Q3_OBJECT_BASE;
    let group = PPC_Q3_OBJECT_BASE + PPC_Q3_OBJECT_STRIDE;
    let group_attribute_set = PPC_Q3_OBJECT_BASE + PPC_Q3_OBJECT_STRIDE * 2;
    let trimesh = PPC_Q3_OBJECT_BASE + PPC_Q3_OBJECT_STRIDE * 3;
    let mesh_attribute_set = PPC_Q3_OBJECT_BASE + PPC_Q3_OBJECT_STRIDE * 4;
    let shader = PPC_Q3_OBJECT_BASE + PPC_Q3_OBJECT_STRIDE * 5;
    let texture = PPC_Q3_OBJECT_BASE + PPC_Q3_OBJECT_STRIDE * 6;
    let pef = synthetic_pef_with_library_import(b"QuickDraw\xaa 3D", b"Q3Object_Submit");
    let mut loaded = load_pef_application(&pef).unwrap();
    let transparency = vec![
        0x3f, 0x40, 0x00, 0x00, 0x3f, 0x40, 0x00, 0x00, 0x3f, 0x40, 0x00, 0x00,
    ];
    let surface_shader = shader.to_be_bytes().to_vec();
    let mipmap = vec![0x44, 0x55, 0x66, 0x77];
    let texture_shader = PpcQ3TextureShaderRecord { shader, texture };
    let mipmap_texture = PpcQ3MipmapTextureRecord {
        texture,
        mipmap: mipmap.clone(),
    };

    loaded
        .q3_objects
        .push(test_q3_object(view, PPC_Q3_TYPE_VIEW));
    loaded.q3_objects.push(PpcQ3ObjectRecord {
        object: group,
        kind: PpcQ3ObjectKind::Generic,
        object_type: PPC_Q3_TYPE_CONTAINER,
        source: PpcQ3ObjectSource::default(),
        data_ptr: 0,
        data_size: 0,
    });
    loaded.q3_objects.push(PpcQ3ObjectRecord {
        object: group_attribute_set,
        kind: PpcQ3ObjectKind::Generic,
        object_type: PPC_Q3_TYPE_ATTRIBUTE_SET,
        source: PpcQ3ObjectSource::default(),
        data_ptr: 0,
        data_size: 0,
    });
    loaded.q3_objects.push(PpcQ3ObjectRecord {
        object: mesh_attribute_set,
        kind: PpcQ3ObjectKind::Generic,
        object_type: PPC_Q3_TYPE_ATTRIBUTE_SET,
        source: PpcQ3ObjectSource::default(),
        data_ptr: 0,
        data_size: 0,
    });
    loaded.q3_objects.push(PpcQ3ObjectRecord {
        object: trimesh,
        kind: PpcQ3ObjectKind::Generic,
        object_type: PPC_Q3_TYPE_TRIMESH,
        source: PpcQ3ObjectSource::default(),
        data_ptr: 0,
        data_size: PPC_Q3_TRIMESH_DATA_SIZE,
    });
    loaded
        .q3_group_memberships
        .push(PpcQ3GroupMembershipRecord {
            group,
            object: group_attribute_set,
            before: None,
        });
    loaded
        .q3_group_memberships
        .push(PpcQ3GroupMembershipRecord {
            group,
            object: trimesh,
            before: None,
        });
    loaded.q3_attributes.push(PpcQ3AttributeRecord {
        attribute_set: group_attribute_set,
        attribute_type: PPC_Q3_ATTRIBUTE_TYPE_TRANSPARENCY_COLOR,
        data: transparency.clone(),
    });
    loaded.q3_attributes.push(PpcQ3AttributeRecord {
        attribute_set: mesh_attribute_set,
        attribute_type: PPC_Q3_ATTRIBUTE_TYPE_SURFACE_SHADER,
        data: surface_shader.clone(),
    });
    loaded.q3_trimeshes.push(PpcQ3TriMeshRecord {
        trimesh,
        data: vec![0; PPC_Q3_TRIMESH_DATA_SIZE as usize],
        triangle_attribute_sets: vec![mesh_attribute_set],
        get_data_copies: Vec::new(),
    });
    loaded.q3_texture_shaders.push(texture_shader);
    loaded.q3_mipmap_textures.push(mipmap_texture.clone());
    loaded.cpu.gpr[3] = group;
    loaded.cpu.gpr[4] = view;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], 1);
    assert_eq!(
        loaded.q3_submission_materials[1],
        PpcQ3SubmissionMaterialRecord {
            view,
            kind: PpcQ3SubmissionKind::TriMesh,
            primary: trimesh,
            secondary: 0,
            shader,
            illumination_type: PPC_Q3_ILLUMINATION_TYPE_PHONG,
            styles: Vec::new(),
            fog_style: None,
            attributes: vec![
                PpcQ3AttributeRecord {
                    attribute_set: group_attribute_set,
                    attribute_type: PPC_Q3_ATTRIBUTE_TYPE_TRANSPARENCY_COLOR,
                    data: transparency,
                },
                PpcQ3AttributeRecord {
                    attribute_set: mesh_attribute_set,
                    attribute_type: PPC_Q3_ATTRIBUTE_TYPE_SURFACE_SHADER,
                    data: surface_shader,
                },
            ],
            shader_uv_transform: None,
            shader_boundary: None,
            texture_shader: Some(texture_shader),
            mipmap_texture: Some(mipmap_texture),
        }
    );
}

#[test]
fn hle_import_runner_submits_q3_trimesh_before_stale_retained_memberships() {
    let view = PPC_Q3_OBJECT_BASE;
    let group = PPC_Q3_OBJECT_BASE + PPC_Q3_OBJECT_STRIDE;
    let trimesh = PPC_Q3_OBJECT_BASE + PPC_Q3_OBJECT_STRIDE * 2;
    let stale_nested_trimesh = PPC_Q3_OBJECT_BASE + PPC_Q3_OBJECT_STRIDE * 3;
    let pef = synthetic_pef_with_library_import(b"QuickDraw\xaa 3D", b"Q3Object_Submit");
    let mut loaded = load_pef_application(&pef).unwrap();
    loaded.q3_objects.extend([
        test_q3_object(view, PPC_Q3_TYPE_VIEW),
        test_q3_object(group, PPC_Q3_TYPE_CONTAINER),
        test_q3_object(trimesh, PPC_Q3_TYPE_TRIMESH),
        test_q3_object(stale_nested_trimesh, PPC_Q3_TYPE_TRIMESH),
    ]);
    loaded.q3_group_memberships.extend([
        PpcQ3GroupMembershipRecord {
            group,
            object: trimesh,
            before: None,
        },
        PpcQ3GroupMembershipRecord {
            group: trimesh,
            object: stale_nested_trimesh,
            before: None,
        },
    ]);
    loaded.cpu.gpr[3] = group;
    loaded.cpu.gpr[4] = view;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], 1);
    assert_eq!(
        loaded.q3_submissions,
        vec![
            PpcQ3SubmissionRecord {
                view,
                kind: PpcQ3SubmissionKind::Object,
                primary: group,
                secondary: 0,
            },
            PpcQ3SubmissionRecord {
                view,
                kind: PpcQ3SubmissionKind::TriMesh,
                primary: trimesh,
                secondary: 0,
            },
        ]
    );
}

#[test]
fn hle_import_runner_snapshots_q3_object_group_styles_for_child_trimeshes() {
    let view = PPC_Q3_OBJECT_BASE;
    let group = PPC_Q3_OBJECT_BASE + PPC_Q3_OBJECT_STRIDE;
    let style = PPC_Q3_OBJECT_BASE + PPC_Q3_OBJECT_STRIDE * 2;
    let trimesh = PPC_Q3_OBJECT_BASE + PPC_Q3_OBJECT_STRIDE * 3;
    let pef = synthetic_pef_with_library_import(b"QuickDraw\xaa 3D", b"Q3Object_Submit");
    let mut loaded = load_pef_application(&pef).unwrap();
    let fill_style = PpcQ3StyleRecord {
        style,
        kind: PpcQ3StyleKind::Fill,
        value: PPC_Q3_FILL_STYLE_EDGES,
    };
    loaded
        .q3_objects
        .push(test_q3_object(view, PPC_Q3_TYPE_VIEW));
    loaded.q3_objects.push(PpcQ3ObjectRecord {
        object: group,
        kind: PpcQ3ObjectKind::Generic,
        object_type: PPC_Q3_TYPE_CONTAINER,
        source: PpcQ3ObjectSource::default(),
        data_ptr: 0,
        data_size: 0,
    });
    loaded.q3_objects.push(PpcQ3ObjectRecord {
        object: style,
        kind: PpcQ3ObjectKind::Generic,
        object_type: PPC_Q3_STYLE_TYPE_FILL,
        source: PpcQ3ObjectSource::default(),
        data_ptr: 0,
        data_size: 4,
    });
    loaded.q3_objects.push(PpcQ3ObjectRecord {
        object: trimesh,
        kind: PpcQ3ObjectKind::Generic,
        object_type: PPC_Q3_TYPE_TRIMESH,
        source: PpcQ3ObjectSource::default(),
        data_ptr: 0,
        data_size: PPC_Q3_TRIMESH_DATA_SIZE,
    });
    loaded
        .q3_group_memberships
        .push(PpcQ3GroupMembershipRecord {
            group,
            object: style,
            before: None,
        });
    loaded
        .q3_group_memberships
        .push(PpcQ3GroupMembershipRecord {
            group,
            object: trimesh,
            before: None,
        });
    loaded.q3_styles.push(fill_style);
    loaded.cpu.gpr[3] = group;
    loaded.cpu.gpr[4] = view;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], 1);
    assert_eq!(
        loaded.q3_submissions,
        vec![
            PpcQ3SubmissionRecord {
                view,
                kind: PpcQ3SubmissionKind::Object,
                primary: group,
                secondary: 0,
            },
            PpcQ3SubmissionRecord {
                view,
                kind: PpcQ3SubmissionKind::TriMesh,
                primary: trimesh,
                secondary: 0,
            },
        ]
    );
    assert_eq!(
        loaded.q3_submission_materials[1],
        PpcQ3SubmissionMaterialRecord {
            view,
            kind: PpcQ3SubmissionKind::TriMesh,
            primary: trimesh,
            secondary: 0,
            shader: 0,
            illumination_type: PPC_Q3_ILLUMINATION_TYPE_PHONG,
            styles: vec![fill_style],
            fog_style: None,
            attributes: Vec::new(),
            shader_uv_transform: None,
            shader_boundary: None,
            texture_shader: None,
            mipmap_texture: None,
        }
    );
    loaded.q3_styles[0].value = PPC_Q3_FILL_STYLE_POINTS;
    assert_eq!(loaded.q3_submission_materials[1].styles, vec![fill_style]);
}

#[test]
fn hle_import_runner_snapshots_q3_object_group_transforms_for_child_trimeshes() {
    let view = PPC_Q3_OBJECT_BASE;
    let group = PPC_Q3_OBJECT_BASE + PPC_Q3_OBJECT_STRIDE;
    let transform = PPC_Q3_OBJECT_BASE + PPC_Q3_OBJECT_STRIDE * 2;
    let trimesh = PPC_Q3_OBJECT_BASE + PPC_Q3_OBJECT_STRIDE * 3;
    let matrix_ptr = PPC_DATA_BASE + 0x1000;
    let pef = synthetic_pef_with_library_import(b"QuickDraw\xaa 3D", b"Q3Object_Submit");
    let mut loaded = load_pef_application(&pef).unwrap();
    let mut matrix = ppc_q3_matrix4x4_identity();
    matrix[3][0] = 5.0;
    matrix[3][1] = 6.0;
    matrix[3][2] = 7.0;
    loaded.memory.add_region(matrix_ptr, vec![0; 64]);
    ppc_write_q3_matrix4x4(&mut loaded.memory, matrix_ptr, &matrix).unwrap();
    loaded
        .q3_objects
        .push(test_q3_object(view, PPC_Q3_TYPE_VIEW));
    loaded.q3_objects.push(PpcQ3ObjectRecord {
        object: group,
        kind: PpcQ3ObjectKind::Generic,
        object_type: PPC_Q3_TYPE_CONTAINER,
        source: PpcQ3ObjectSource::default(),
        data_ptr: 0,
        data_size: 0,
    });
    loaded.q3_objects.push(PpcQ3ObjectRecord {
        object: transform,
        kind: PpcQ3ObjectKind::Generic,
        object_type: PPC_Q3_TRANSFORM_TYPE_MATRIX,
        source: PpcQ3ObjectSource::default(),
        data_ptr: matrix_ptr,
        data_size: 64,
    });
    loaded.q3_objects.push(PpcQ3ObjectRecord {
        object: trimesh,
        kind: PpcQ3ObjectKind::Generic,
        object_type: PPC_Q3_TYPE_TRIMESH,
        source: PpcQ3ObjectSource::default(),
        data_ptr: 0,
        data_size: PPC_Q3_TRIMESH_DATA_SIZE,
    });
    loaded
        .q3_group_memberships
        .push(PpcQ3GroupMembershipRecord {
            group,
            object: transform,
            before: None,
        });
    loaded
        .q3_group_memberships
        .push(PpcQ3GroupMembershipRecord {
            group,
            object: trimesh,
            before: None,
        });
    loaded.cpu.gpr[3] = group;
    loaded.cpu.gpr[4] = view;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], 1);
    assert_eq!(
        loaded.q3_submission_transforms,
        vec![
            PpcQ3SubmissionTransformRecord {
                view,
                kind: PpcQ3SubmissionKind::Object,
                primary: group,
                secondary: 0,
                local_to_world: ppc_q3_matrix4x4_identity(),
            },
            PpcQ3SubmissionTransformRecord {
                view,
                kind: PpcQ3SubmissionKind::TriMesh,
                primary: trimesh,
                secondary: 0,
                local_to_world: matrix,
            },
        ]
    );
}

#[test]
fn hle_import_runner_propagates_nested_q3_object_group_state_to_parent_siblings() {
    let view = PPC_Q3_OBJECT_BASE;
    let group = PPC_Q3_OBJECT_BASE + PPC_Q3_OBJECT_STRIDE;
    let child_group = PPC_Q3_OBJECT_BASE + PPC_Q3_OBJECT_STRIDE * 2;
    let transform = PPC_Q3_OBJECT_BASE + PPC_Q3_OBJECT_STRIDE * 3;
    let style = PPC_Q3_OBJECT_BASE + PPC_Q3_OBJECT_STRIDE * 4;
    let nested_trimesh = PPC_Q3_OBJECT_BASE + PPC_Q3_OBJECT_STRIDE * 5;
    let sibling_trimesh = PPC_Q3_OBJECT_BASE + PPC_Q3_OBJECT_STRIDE * 6;
    let matrix_ptr = PPC_DATA_BASE + 0x1000;
    let pef = synthetic_pef_with_library_import(b"QuickDraw\xaa 3D", b"Q3Object_Submit");
    let mut loaded = load_pef_application(&pef).unwrap();
    let mut matrix = ppc_q3_matrix4x4_identity();
    matrix[3][0] = 9.0;
    matrix[3][1] = 10.0;
    matrix[3][2] = 11.0;
    let fill_style = PpcQ3StyleRecord {
        style,
        kind: PpcQ3StyleKind::Fill,
        value: PPC_Q3_FILL_STYLE_EDGES,
    };
    loaded.memory.add_region(matrix_ptr, vec![0; 64]);
    ppc_write_q3_matrix4x4(&mut loaded.memory, matrix_ptr, &matrix).unwrap();
    loaded.q3_objects.extend([
        test_q3_object(view, PPC_Q3_TYPE_VIEW),
        PpcQ3ObjectRecord {
            object: group,
            kind: PpcQ3ObjectKind::Generic,
            object_type: PPC_Q3_TYPE_CONTAINER,
            source: PpcQ3ObjectSource::default(),
            data_ptr: 0,
            data_size: 0,
        },
        PpcQ3ObjectRecord {
            object: child_group,
            kind: PpcQ3ObjectKind::Generic,
            object_type: PPC_Q3_TYPE_CONTAINER,
            source: PpcQ3ObjectSource::default(),
            data_ptr: 0,
            data_size: 0,
        },
        PpcQ3ObjectRecord {
            object: transform,
            kind: PpcQ3ObjectKind::Generic,
            object_type: PPC_Q3_TRANSFORM_TYPE_MATRIX,
            source: PpcQ3ObjectSource::default(),
            data_ptr: matrix_ptr,
            data_size: 64,
        },
        PpcQ3ObjectRecord {
            object: style,
            kind: PpcQ3ObjectKind::Generic,
            object_type: PPC_Q3_STYLE_TYPE_FILL,
            source: PpcQ3ObjectSource::default(),
            data_ptr: 0,
            data_size: 4,
        },
        PpcQ3ObjectRecord {
            object: nested_trimesh,
            kind: PpcQ3ObjectKind::Generic,
            object_type: PPC_Q3_TYPE_TRIMESH,
            source: PpcQ3ObjectSource::default(),
            data_ptr: 0,
            data_size: PPC_Q3_TRIMESH_DATA_SIZE,
        },
        PpcQ3ObjectRecord {
            object: sibling_trimesh,
            kind: PpcQ3ObjectKind::Generic,
            object_type: PPC_Q3_TYPE_TRIMESH,
            source: PpcQ3ObjectSource::default(),
            data_ptr: 0,
            data_size: PPC_Q3_TRIMESH_DATA_SIZE,
        },
    ]);
    loaded.q3_group_memberships.extend([
        PpcQ3GroupMembershipRecord {
            group,
            object: child_group,
            before: None,
        },
        PpcQ3GroupMembershipRecord {
            group: child_group,
            object: transform,
            before: None,
        },
        PpcQ3GroupMembershipRecord {
            group: child_group,
            object: style,
            before: None,
        },
        PpcQ3GroupMembershipRecord {
            group: child_group,
            object: nested_trimesh,
            before: None,
        },
        PpcQ3GroupMembershipRecord {
            group,
            object: sibling_trimesh,
            before: None,
        },
    ]);
    loaded.q3_styles.push(fill_style);
    loaded.cpu.gpr[3] = group;
    loaded.cpu.gpr[4] = view;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], 1);
    assert_eq!(
        loaded.q3_submissions,
        vec![
            PpcQ3SubmissionRecord {
                view,
                kind: PpcQ3SubmissionKind::Object,
                primary: group,
                secondary: 0,
            },
            PpcQ3SubmissionRecord {
                view,
                kind: PpcQ3SubmissionKind::TriMesh,
                primary: nested_trimesh,
                secondary: 0,
            },
            PpcQ3SubmissionRecord {
                view,
                kind: PpcQ3SubmissionKind::TriMesh,
                primary: sibling_trimesh,
                secondary: 0,
            },
        ]
    );
    assert_eq!(
        loaded.q3_submission_transforms,
        vec![
            PpcQ3SubmissionTransformRecord {
                view,
                kind: PpcQ3SubmissionKind::Object,
                primary: group,
                secondary: 0,
                local_to_world: ppc_q3_matrix4x4_identity(),
            },
            PpcQ3SubmissionTransformRecord {
                view,
                kind: PpcQ3SubmissionKind::TriMesh,
                primary: nested_trimesh,
                secondary: 0,
                local_to_world: matrix,
            },
            PpcQ3SubmissionTransformRecord {
                view,
                kind: PpcQ3SubmissionKind::TriMesh,
                primary: sibling_trimesh,
                secondary: 0,
                local_to_world: matrix,
            },
        ]
    );
    assert_eq!(loaded.q3_submission_materials[1].styles, vec![fill_style]);
    assert_eq!(loaded.q3_submission_materials[2].styles, vec![fill_style]);
}

#[test]
fn hle_import_runner_sorts_q3_ordered_display_group_children_for_submission() {
    let view = PPC_Q3_OBJECT_BASE;
    let group = PPC_Q3_OBJECT_BASE + PPC_Q3_OBJECT_STRIDE;
    let child_group = PPC_Q3_OBJECT_BASE + PPC_Q3_OBJECT_STRIDE * 2;
    let transform = PPC_Q3_OBJECT_BASE + PPC_Q3_OBJECT_STRIDE * 3;
    let style = PPC_Q3_OBJECT_BASE + PPC_Q3_OBJECT_STRIDE * 4;
    let attribute_set = PPC_Q3_OBJECT_BASE + PPC_Q3_OBJECT_STRIDE * 5;
    let shader = PPC_Q3_OBJECT_BASE + PPC_Q3_OBJECT_STRIDE * 6;
    let root_trimesh = PPC_Q3_OBJECT_BASE + PPC_Q3_OBJECT_STRIDE * 7;
    let child_trimesh = PPC_Q3_OBJECT_BASE + PPC_Q3_OBJECT_STRIDE * 8;
    let matrix_ptr = PPC_DATA_BASE + 0x1000;
    let pef = synthetic_pef_with_library_import(b"QuickDraw\xaa 3D", b"Q3Object_Submit");
    let mut loaded = load_pef_application(&pef).unwrap();
    let mut matrix = ppc_q3_matrix4x4_identity();
    matrix[3][0] = 2.0;
    matrix[3][1] = 3.0;
    matrix[3][2] = 4.0;
    let fill_style = PpcQ3StyleRecord {
        style,
        kind: PpcQ3StyleKind::Fill,
        value: PPC_Q3_FILL_STYLE_EDGES,
    };
    let diffuse = vec![
        0x3f, 0x00, 0x00, 0x00, 0x3e, 0x80, 0x00, 0x00, 0x3e, 0x00, 0x00, 0x00,
    ];
    loaded.memory.add_region(matrix_ptr, vec![0; 64]);
    ppc_write_q3_matrix4x4(&mut loaded.memory, matrix_ptr, &matrix).unwrap();
    loaded.q3_objects.extend([
        test_q3_object(view, PPC_Q3_TYPE_VIEW),
        PpcQ3ObjectRecord {
            object: group,
            kind: PpcQ3ObjectKind::Generic,
            object_type: PPC_Q3_GROUP_TYPE_ORDERED_DISPLAY,
            source: PpcQ3ObjectSource::default(),
            data_ptr: 0,
            data_size: 0,
        },
        PpcQ3ObjectRecord {
            object: child_group,
            kind: PpcQ3ObjectKind::Generic,
            object_type: PPC_Q3_GROUP_TYPE_DISPLAY,
            source: PpcQ3ObjectSource::default(),
            data_ptr: 0,
            data_size: 0,
        },
        PpcQ3ObjectRecord {
            object: transform,
            kind: PpcQ3ObjectKind::Generic,
            object_type: PPC_Q3_TRANSFORM_TYPE_MATRIX,
            source: PpcQ3ObjectSource::default(),
            data_ptr: matrix_ptr,
            data_size: 64,
        },
        PpcQ3ObjectRecord {
            object: style,
            kind: PpcQ3ObjectKind::Generic,
            object_type: PPC_Q3_STYLE_TYPE_FILL,
            source: PpcQ3ObjectSource::default(),
            data_ptr: 0,
            data_size: 4,
        },
        PpcQ3ObjectRecord {
            object: attribute_set,
            kind: PpcQ3ObjectKind::Generic,
            object_type: PPC_Q3_TYPE_ATTRIBUTE_SET,
            source: PpcQ3ObjectSource::default(),
            data_ptr: 0,
            data_size: 0,
        },
        PpcQ3ObjectRecord {
            object: shader,
            kind: PpcQ3ObjectKind::Generic,
            object_type: PPC_Q3_SURFACE_SHADER_TYPE_TEXTURE,
            source: PpcQ3ObjectSource::default(),
            data_ptr: 0,
            data_size: 0,
        },
        PpcQ3ObjectRecord {
            object: root_trimesh,
            kind: PpcQ3ObjectKind::Generic,
            object_type: PPC_Q3_TYPE_TRIMESH,
            source: PpcQ3ObjectSource::default(),
            data_ptr: 0,
            data_size: PPC_Q3_TRIMESH_DATA_SIZE,
        },
        PpcQ3ObjectRecord {
            object: child_trimesh,
            kind: PpcQ3ObjectKind::Generic,
            object_type: PPC_Q3_TYPE_TRIMESH,
            source: PpcQ3ObjectSource::default(),
            data_ptr: 0,
            data_size: PPC_Q3_TRIMESH_DATA_SIZE,
        },
    ]);
    loaded.q3_group_memberships.extend([
        PpcQ3GroupMembershipRecord {
            group,
            object: child_group,
            before: None,
        },
        PpcQ3GroupMembershipRecord {
            group,
            object: root_trimesh,
            before: None,
        },
        PpcQ3GroupMembershipRecord {
            group,
            object: shader,
            before: None,
        },
        PpcQ3GroupMembershipRecord {
            group,
            object: attribute_set,
            before: None,
        },
        PpcQ3GroupMembershipRecord {
            group,
            object: style,
            before: None,
        },
        PpcQ3GroupMembershipRecord {
            group,
            object: transform,
            before: None,
        },
        PpcQ3GroupMembershipRecord {
            group: child_group,
            object: child_trimesh,
            before: None,
        },
    ]);
    loaded.q3_styles.push(fill_style);
    loaded.q3_attributes.push(PpcQ3AttributeRecord {
        attribute_set,
        attribute_type: PPC_Q3_ATTRIBUTE_TYPE_DIFFUSE_COLOR,
        data: diffuse.clone(),
    });
    loaded.cpu.gpr[3] = group;
    loaded.cpu.gpr[4] = view;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], 1);
    assert_eq!(
        loaded.q3_submissions,
        vec![
            PpcQ3SubmissionRecord {
                view,
                kind: PpcQ3SubmissionKind::Object,
                primary: group,
                secondary: 0,
            },
            PpcQ3SubmissionRecord {
                view,
                kind: PpcQ3SubmissionKind::TriMesh,
                primary: root_trimesh,
                secondary: 0,
            },
            PpcQ3SubmissionRecord {
                view,
                kind: PpcQ3SubmissionKind::TriMesh,
                primary: child_trimesh,
                secondary: 0,
            },
        ]
    );
    assert_eq!(loaded.q3_submission_transforms[1].local_to_world, matrix);
    assert_eq!(loaded.q3_submission_transforms[2].local_to_world, matrix);
    for material in &loaded.q3_submission_materials[1..=2] {
        assert_eq!(material.shader, shader);
        assert_eq!(material.styles, vec![fill_style]);
        assert_eq!(
            material.attributes,
            vec![PpcQ3AttributeRecord {
                attribute_set,
                attribute_type: PPC_Q3_ATTRIBUTE_TYPE_DIFFUSE_COLOR,
                data: diffuse.clone(),
            }]
        );
    }
}

#[test]
fn hle_import_runner_uses_first_q3_io_proxy_display_group_representation() {
    let view = PPC_Q3_OBJECT_BASE;
    let proxy_group = PPC_Q3_OBJECT_BASE + PPC_Q3_OBJECT_STRIDE;
    let preferred_group = PPC_Q3_OBJECT_BASE + PPC_Q3_OBJECT_STRIDE * 2;
    let preferred_trimesh = PPC_Q3_OBJECT_BASE + PPC_Q3_OBJECT_STRIDE * 3;
    let fallback_trimesh = PPC_Q3_OBJECT_BASE + PPC_Q3_OBJECT_STRIDE * 4;
    let unsupported_object = PPC_Q3_OBJECT_BASE + PPC_Q3_OBJECT_STRIDE * 5;
    let pef = synthetic_pef_with_library_import(b"QuickDraw\xaa 3D", b"Q3Object_Submit");
    let mut loaded = load_pef_application(&pef).unwrap();
    loaded.q3_objects.extend([
        test_q3_object(view, PPC_Q3_TYPE_VIEW),
        PpcQ3ObjectRecord {
            object: proxy_group,
            kind: PpcQ3ObjectKind::Generic,
            object_type: PPC_Q3_GROUP_TYPE_IO_PROXY_DISPLAY,
            source: PpcQ3ObjectSource::default(),
            data_ptr: 0,
            data_size: 0,
        },
        PpcQ3ObjectRecord {
            object: preferred_group,
            kind: PpcQ3ObjectKind::Generic,
            object_type: PPC_Q3_GROUP_TYPE_DISPLAY,
            source: PpcQ3ObjectSource::default(),
            data_ptr: 0,
            data_size: 0,
        },
        PpcQ3ObjectRecord {
            object: preferred_trimesh,
            kind: PpcQ3ObjectKind::Generic,
            object_type: PPC_Q3_TYPE_TRIMESH,
            source: PpcQ3ObjectSource::default(),
            data_ptr: 0,
            data_size: PPC_Q3_TRIMESH_DATA_SIZE,
        },
        PpcQ3ObjectRecord {
            object: fallback_trimesh,
            kind: PpcQ3ObjectKind::Generic,
            object_type: PPC_Q3_TYPE_TRIMESH,
            source: PpcQ3ObjectSource::default(),
            data_ptr: 0,
            data_size: PPC_Q3_TRIMESH_DATA_SIZE,
        },
        PpcQ3ObjectRecord {
            object: unsupported_object,
            kind: PpcQ3ObjectKind::Generic,
            object_type: PPC_Q3_TYPE_NONE,
            source: PpcQ3ObjectSource::default(),
            data_ptr: 0,
            data_size: 0,
        },
    ]);
    loaded.q3_group_memberships.extend([
        PpcQ3GroupMembershipRecord {
            group: proxy_group,
            object: unsupported_object,
            before: None,
        },
        PpcQ3GroupMembershipRecord {
            group: proxy_group,
            object: preferred_group,
            before: None,
        },
        PpcQ3GroupMembershipRecord {
            group: proxy_group,
            object: fallback_trimesh,
            before: None,
        },
        PpcQ3GroupMembershipRecord {
            group: preferred_group,
            object: preferred_trimesh,
            before: None,
        },
    ]);
    loaded.cpu.gpr[3] = proxy_group;
    loaded.cpu.gpr[4] = view;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], 1);
    assert_eq!(
        loaded.q3_submissions,
        vec![
            PpcQ3SubmissionRecord {
                view,
                kind: PpcQ3SubmissionKind::Object,
                primary: proxy_group,
                secondary: 0,
            },
            PpcQ3SubmissionRecord {
                view,
                kind: PpcQ3SubmissionKind::TriMesh,
                primary: preferred_trimesh,
                secondary: 0,
            },
        ]
    );
    assert!(loaded
        .q3_submissions
        .iter()
        .all(|submission| submission.primary != fallback_trimesh));
}

#[test]
fn hle_import_runner_records_q3_direct_style_submissions() {
    let view = PPC_Q3_OBJECT_BASE;
    let pef = synthetic_pef_with_library_import(b"QuickDraw\xaa 3D", b"Q3FillStyle_Submit");
    let mut loaded = load_pef_application(&pef).unwrap();
    loaded
        .q3_objects
        .push(test_q3_object(view, PPC_Q3_TYPE_VIEW));
    loaded.cpu.gpr[3] = PPC_Q3_FILL_STYLE_EDGES;
    loaded.cpu.gpr[4] = view;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], 1);
    let fill_style = PpcQ3StyleRecord {
        style: 0,
        kind: PpcQ3StyleKind::Fill,
        value: PPC_Q3_FILL_STYLE_EDGES,
    };
    assert_eq!(
        loaded.q3_view_materials,
        vec![PpcQ3ViewMaterialRecord {
            view,
            shader: 0,
            illumination_type: PPC_Q3_ILLUMINATION_TYPE_PHONG,
            styles: vec![fill_style],
            fog_style: None,
            attributes: Vec::new(),
        }]
    );
    assert_eq!(
        loaded.q3_submissions,
        vec![PpcQ3SubmissionRecord {
            view,
            kind: PpcQ3SubmissionKind::Style,
            primary: PPC_Q3_FILL_STYLE_EDGES,
            secondary: PPC_Q3_STYLE_TYPE_FILL,
        }]
    );
    assert_eq!(
        loaded.q3_submission_transforms,
        vec![PpcQ3SubmissionTransformRecord {
            view,
            kind: PpcQ3SubmissionKind::Style,
            primary: PPC_Q3_FILL_STYLE_EDGES,
            secondary: PPC_Q3_STYLE_TYPE_FILL,
            local_to_world: ppc_q3_matrix4x4_identity(),
        }]
    );
    assert_eq!(
        loaded.q3_submission_materials,
        vec![PpcQ3SubmissionMaterialRecord {
            view,
            kind: PpcQ3SubmissionKind::Style,
            primary: PPC_Q3_FILL_STYLE_EDGES,
            secondary: PPC_Q3_STYLE_TYPE_FILL,
            shader: 0,
            illumination_type: PPC_Q3_ILLUMINATION_TYPE_PHONG,
            styles: vec![fill_style],
            fog_style: None,
            attributes: Vec::new(),
            shader_uv_transform: None,
            shader_boundary: None,
            texture_shader: None,
            mipmap_texture: None,
        }]
    );
    assert_eq!(
        loaded.q3_submission_lights,
        vec![PpcQ3SubmissionLightRecord {
            view,
            kind: PpcQ3SubmissionKind::Style,
            primary: PPC_Q3_FILL_STYLE_EDGES,
            secondary: PPC_Q3_STYLE_TYPE_FILL,
            light_group: 0,
            lights: Vec::new(),
        }]
    );

    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::Q3OrientationStyleSubmit;
    loaded.cpu.gpr[3] = 2;
    loaded.cpu.gpr[4] = view;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], 0);
    assert_eq!(loaded.q3_submissions.len(), 1);
    assert_eq!(loaded.q3_view_materials[0].styles, vec![fill_style]);

    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.cpu.gpr[3] = PPC_Q3_ORIENTATION_STYLE_CLOCKWISE;
    loaded.cpu.gpr[4] = view;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], 1);
    let orientation_style = PpcQ3StyleRecord {
        style: 0,
        kind: PpcQ3StyleKind::Orientation,
        value: PPC_Q3_ORIENTATION_STYLE_CLOCKWISE,
    };
    assert_eq!(
        loaded.q3_view_materials[0].styles,
        vec![fill_style, orientation_style]
    );
    assert_eq!(
        loaded.q3_submissions[1],
        PpcQ3SubmissionRecord {
            view,
            kind: PpcQ3SubmissionKind::Style,
            primary: PPC_Q3_ORIENTATION_STYLE_CLOCKWISE,
            secondary: PPC_Q3_STYLE_TYPE_ORIENTATION,
        }
    );
    assert_eq!(
        loaded.q3_submission_materials[1].styles,
        vec![fill_style, orientation_style]
    );

    let submission_count = loaded.q3_submissions.len();
    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::Q3FillStyleSubmit;
    loaded.cpu.gpr[3] = PPC_Q3_FILL_STYLE_POINTS;
    loaded.cpu.gpr[4] = view + PPC_Q3_OBJECT_STRIDE;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], 0);
    assert_eq!(loaded.q3_submissions.len(), submission_count);
    assert_eq!(
        loaded.q3_error_state.first_error,
        PPC_Q3_ERROR_INVALID_OBJECT_PARAMETER
    );
}

#[test]
fn hle_import_runner_records_q3_view_only_submissions() {
    let view = PPC_Q3_OBJECT_BASE;
    let pef = synthetic_pef_with_library_import(b"QuickDraw\xaa 3D", b"Q3Push_Submit");
    let mut loaded = load_pef_application(&pef).unwrap();
    loaded
        .q3_objects
        .push(test_q3_object(view, PPC_Q3_TYPE_VIEW));
    loaded.cpu.gpr[3] = view;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], 1);
    assert_eq!(
        loaded.q3_submissions,
        vec![PpcQ3SubmissionRecord {
            view,
            kind: PpcQ3SubmissionKind::Push,
            primary: 0,
            secondary: 0,
        }]
    );

    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.cpu.gpr[3] = view;
    loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::Q3ResetTransformSubmit;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], 1);
    assert_eq!(
        loaded.q3_submissions[1],
        PpcQ3SubmissionRecord {
            view,
            kind: PpcQ3SubmissionKind::ResetTransform,
            primary: 0,
            secondary: 0,
        }
    );

    let submission_count = loaded.q3_submissions.len();
    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.cpu.gpr[3] = view + PPC_Q3_OBJECT_STRIDE;
    loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::Q3PushSubmit;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], 0);
    assert_eq!(loaded.q3_submissions.len(), submission_count);
    assert_eq!(
        loaded.q3_error_state.first_error,
        PPC_Q3_ERROR_INVALID_OBJECT_PARAMETER
    );
}

#[test]
fn hle_import_runner_tracks_q3_transform_stack_for_submissions() {
    let view = PPC_Q3_OBJECT_BASE;
    let trimesh = PPC_Q3_OBJECT_BASE + PPC_Q3_OBJECT_STRIDE;
    let matrix_ptr = PPC_DATA_BASE + 0x1000;
    let second_matrix_ptr = PPC_DATA_BASE + 0x1040;
    let pef =
        synthetic_pef_with_library_import(b"QuickDraw\xaa 3D", b"Q3MatrixTransform_Submit");
    let mut loaded = load_pef_application(&pef).unwrap();
    loaded.memory.add_region(matrix_ptr, vec![0; 0x100]);
    loaded
        .q3_objects
        .push(test_q3_object(view, PPC_Q3_TYPE_VIEW));

    let mut translate_x = ppc_q3_matrix4x4_identity();
    translate_x[3][0] = 2.0;
    let mut translate_y = ppc_q3_matrix4x4_identity();
    translate_y[3][1] = 3.0;
    ppc_write_q3_matrix4x4(&mut loaded.memory, matrix_ptr, &translate_x).unwrap();
    ppc_write_q3_matrix4x4(&mut loaded.memory, second_matrix_ptr, &translate_y).unwrap();
    loaded.cpu.gpr[3] = matrix_ptr;
    loaded.cpu.gpr[4] = view;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], 1);
    assert_eq!(
        loaded.q3_submissions,
        vec![PpcQ3SubmissionRecord {
            view,
            kind: PpcQ3SubmissionKind::MatrixTransform,
            primary: matrix_ptr,
            secondary: 0,
        }]
    );
    assert_eq!(
        loaded.q3_view_transforms,
        vec![PpcQ3ViewTransformRecord {
            view,
            stack: vec![translate_x],
            local_to_world: translate_x,
        }]
    );
    assert_eq!(
        loaded.q3_submission_transforms,
        vec![PpcQ3SubmissionTransformRecord {
            view,
            kind: PpcQ3SubmissionKind::MatrixTransform,
            primary: matrix_ptr,
            secondary: 0,
            local_to_world: translate_x,
        }]
    );

    ppc_write_q3_matrix4x4(&mut loaded.memory, matrix_ptr, &ppc_q3_matrix4x4_identity())
        .unwrap();
    assert_eq!(loaded.q3_view_transforms[0].stack[0], translate_x);

    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::Q3TriMeshSubmit;
    loaded.cpu.gpr[3] = trimesh;
    loaded.cpu.gpr[4] = view;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], 1);
    assert_eq!(
        loaded.q3_submission_transforms[1],
        PpcQ3SubmissionTransformRecord {
            view,
            kind: PpcQ3SubmissionKind::TriMesh,
            primary: trimesh,
            secondary: 0,
            local_to_world: translate_x,
        }
    );

    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::Q3MatrixTransformSubmit;
    loaded.cpu.gpr[3] = second_matrix_ptr;
    loaded.cpu.gpr[4] = view;

    let probe = loaded.run_with_hle_imports(64);

    let combined = ppc_q3_matrix4x4_multiply_values(translate_x, translate_y);
    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], 1);
    assert_eq!(
        loaded.q3_view_transforms[0].stack,
        vec![translate_x, translate_y]
    );
    assert_eq!(loaded.q3_view_transforms[0].local_to_world, combined);
    assert_eq!(
        loaded.q3_submission_transforms[2],
        PpcQ3SubmissionTransformRecord {
            view,
            kind: PpcQ3SubmissionKind::MatrixTransform,
            primary: second_matrix_ptr,
            secondary: 0,
            local_to_world: combined,
        }
    );

    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::Q3ResetTransformSubmit;
    loaded.cpu.gpr[3] = view;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], 1);
    assert!(loaded.q3_view_transforms[0].stack.is_empty());
    assert_eq!(
        loaded.q3_view_transforms[0].local_to_world,
        ppc_q3_matrix4x4_identity()
    );
    assert_eq!(
        loaded.q3_submission_transforms[3],
        PpcQ3SubmissionTransformRecord {
            view,
            kind: PpcQ3SubmissionKind::ResetTransform,
            primary: 0,
            secondary: 0,
            local_to_world: ppc_q3_matrix4x4_identity(),
        }
    );
}

#[test]
fn hle_import_runner_restores_q3_view_state_for_push_pop() {
    let view = PPC_Q3_OBJECT_BASE;
    let initial_shader = PPC_Q3_OBJECT_BASE + PPC_Q3_OBJECT_STRIDE;
    let inner_shader = PPC_Q3_OBJECT_BASE + PPC_Q3_OBJECT_STRIDE * 2;
    let matrix_ptr = PPC_DATA_BASE + 0x1000;
    let pef = synthetic_pef_with_library_import(b"QuickDraw\xaa 3D", b"Q3Push_Submit");
    let mut loaded = load_pef_application(&pef).unwrap();
    loaded.memory.add_region(matrix_ptr, vec![0; 0x40]);
    loaded
        .q3_objects
        .push(test_q3_object(view, PPC_Q3_TYPE_VIEW));
    loaded.q3_objects.push(test_q3_object(
        inner_shader,
        PPC_Q3_SURFACE_SHADER_TYPE_TEXTURE,
    ));
    let mut initial_transform = ppc_q3_matrix4x4_identity();
    initial_transform[3][0] = 2.0;
    let mut inner_transform = ppc_q3_matrix4x4_identity();
    inner_transform[3][1] = 5.0;
    ppc_write_q3_matrix4x4(&mut loaded.memory, matrix_ptr, &inner_transform).unwrap();
    loaded.q3_view_transforms.push(PpcQ3ViewTransformRecord {
        view,
        stack: vec![initial_transform],
        local_to_world: initial_transform,
    });
    loaded.q3_view_materials.push(PpcQ3ViewMaterialRecord {
        view,
        shader: initial_shader,
        illumination_type: PPC_Q3_ILLUMINATION_TYPE_PHONG,
        styles: Vec::new(),
        fog_style: None,
        attributes: Vec::new(),
    });
    loaded.cpu.gpr[3] = view;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], 1);
    assert_eq!(
        loaded.q3_view_state_stack,
        vec![PpcQ3ViewStateSnapshotRecord {
            view,
            transform: Some(PpcQ3ViewTransformRecord {
                view,
                stack: vec![initial_transform],
                local_to_world: initial_transform,
            }),
            material: Some(PpcQ3ViewMaterialRecord {
                view,
                shader: initial_shader,
                illumination_type: PPC_Q3_ILLUMINATION_TYPE_PHONG,
                styles: Vec::new(),
                fog_style: None,
                attributes: Vec::new(),
            }),
        }]
    );

    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::Q3MatrixTransformSubmit;
    loaded.cpu.gpr[3] = matrix_ptr;
    loaded.cpu.gpr[4] = view;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], 1);
    let combined = ppc_q3_matrix4x4_multiply_values(initial_transform, inner_transform);
    assert_eq!(loaded.q3_view_transforms[0].local_to_world, combined);

    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::Q3ShaderSubmit;
    loaded.cpu.gpr[3] = inner_shader;
    loaded.cpu.gpr[4] = view;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], 1);
    assert_eq!(loaded.q3_view_materials[0].shader, inner_shader);

    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::Q3PopSubmit;
    loaded.cpu.gpr[3] = view;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], 1);
    assert!(loaded.q3_view_state_stack.is_empty());
    assert_eq!(
        loaded.q3_view_transforms,
        vec![PpcQ3ViewTransformRecord {
            view,
            stack: vec![initial_transform],
            local_to_world: initial_transform,
        }]
    );
    assert_eq!(loaded.q3_view_materials[0].shader, initial_shader);
    assert_eq!(
        loaded
            .q3_submissions
            .iter()
            .map(|record| record.kind)
            .collect::<Vec<_>>(),
        vec![
            PpcQ3SubmissionKind::Push,
            PpcQ3SubmissionKind::MatrixTransform,
            PpcQ3SubmissionKind::Shader,
            PpcQ3SubmissionKind::Pop,
        ]
    );
    assert_eq!(
        loaded.q3_submission_transforms[3].local_to_world,
        initial_transform
    );
    assert_eq!(loaded.q3_submission_materials[3].shader, initial_shader);

    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.cpu.gpr[3] = view;
    let submission_count = loaded.q3_submissions.len();

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], 0);
    assert_eq!(loaded.q3_submissions.len(), submission_count);
}

#[test]
fn hle_import_runner_records_q3_data_submissions() {
    let view = PPC_Q3_OBJECT_BASE;
    let trimesh_data = PPC_DATA_BASE + 0x1000;
    let pef = synthetic_pef_with_library_import(b"QuickDraw\xaa 3D", b"Q3TriMesh_Submit");
    let mut loaded = load_pef_application(&pef).unwrap();
    loaded
        .q3_objects
        .push(test_q3_object(view, PPC_Q3_TYPE_VIEW));
    loaded.cpu.gpr[3] = trimesh_data;
    loaded.cpu.gpr[4] = view;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], 1);
    assert_eq!(
        loaded.q3_submissions,
        vec![PpcQ3SubmissionRecord {
            view,
            kind: PpcQ3SubmissionKind::TriMesh,
            primary: trimesh_data,
            secondary: 0,
        }]
    );
}

#[test]
fn hle_import_runner_resolves_q3_trimesh_scene_commands() {
    let view = PPC_Q3_OBJECT_BASE;
    let camera = PPC_Q3_OBJECT_BASE + PPC_Q3_OBJECT_STRIDE;
    let light_group = PPC_Q3_OBJECT_BASE + PPC_Q3_OBJECT_STRIDE * 2;
    let light = PPC_Q3_OBJECT_BASE + PPC_Q3_OBJECT_STRIDE * 3;
    let shader = PPC_Q3_OBJECT_BASE + PPC_Q3_OBJECT_STRIDE * 4;
    let texture = PPC_Q3_OBJECT_BASE + PPC_Q3_OBJECT_STRIDE * 5;
    let style = PPC_Q3_OBJECT_BASE + PPC_Q3_OBJECT_STRIDE * 6;
    let trimesh_data = PPC_DATA_BASE + 0x1000;
    let pef = synthetic_pef_with_library_import(b"QuickDraw\xaa 3D", b"Q3TriMesh_Submit");
    let mut loaded = load_pef_application(&pef).unwrap();
    let trimesh: Vec<u8> = (0..PPC_Q3_TRIMESH_DATA_SIZE)
        .map(|offset| 0x50u8.wrapping_add(offset as u8))
        .collect();
    loaded.memory.add_region(trimesh_data, trimesh.clone());
    loaded
        .q3_objects
        .push(test_q3_object(view, PPC_Q3_TYPE_VIEW));

    loaded.q3_views.push(PpcQ3ViewStateRecord {
        view,
        renderer: 0x1111_2222,
        light_group,
        draw_context: 0x3333_4444,
        camera,
        rendering_depth: 1,
        bounding_box_depth: 0,
        cancelled: false,
    });
    let camera_record = PpcQ3CameraRecord {
        camera,
        camera_type: PPC_Q3_CAMERA_TYPE_VIEW_ANGLE_ASPECT,
        placement: PpcQ3CameraPlacement {
            camera_location: (1.0, 2.0, 3.0),
            point_of_interest: (4.0, 5.0, 6.0),
            up_vector: (0.0, 1.0, 0.0),
        },
        range_hither: 0.5,
        range_yon: 500.0,
        viewport_origin: (0.0, 0.0),
        viewport_width: 1.0,
        viewport_height: 1.0,
        projection: PpcQ3CameraProjection::ViewAngleAspect {
            fov: 0.75,
            aspect_ratio_x_to_y: 4.0 / 3.0,
        },
    };
    loaded.q3_cameras.push(camera_record);
    loaded
        .q3_group_memberships
        .push(PpcQ3GroupMembershipRecord {
            group: light_group,
            object: light,
            before: None,
        });
    let light_record = PpcQ3LightRecord {
        light,
        light_type: PPC_Q3_LIGHT_TYPE_AMBIENT,
        data: PpcQ3LightData {
            is_on: 1,
            brightness: 0.75,
            color: (0.25, 0.5, 1.0),
        },
        kind: PpcQ3LightKind::Ambient,
    };
    loaded.q3_lights.push(light_record);
    let mut local_to_world = ppc_q3_matrix4x4_identity();
    local_to_world[3][0] = 7.0;
    local_to_world[3][1] = 8.0;
    loaded.q3_view_transforms.push(PpcQ3ViewTransformRecord {
        view,
        stack: vec![local_to_world],
        local_to_world,
    });
    let fog_style = PpcQ3FogStyleData {
        state: 1,
        mode: PPC_Q3_FOG_MODE_LINEAR,
        fog_start: 2.0,
        fog_end: 9.0,
        density: 0.125,
        color: (0.1, 0.2, 0.3, 1.0),
    };
    let fill_style = PpcQ3StyleRecord {
        style,
        kind: PpcQ3StyleKind::Fill,
        value: 2,
    };
    let diffuse_attribute = PpcQ3AttributeRecord {
        attribute_set: PPC_Q3_OBJECT_BASE + PPC_Q3_OBJECT_STRIDE * 7,
        attribute_type: PPC_Q3_ATTRIBUTE_TYPE_DIFFUSE_COLOR,
        data: vec![1, 2, 3, 4],
    };
    loaded.q3_view_materials.push(PpcQ3ViewMaterialRecord {
        view,
        shader,
        illumination_type: PPC_Q3_ILLUMINATION_TYPE_PHONG,
        styles: vec![fill_style],
        fog_style: Some(fog_style),
        attributes: vec![diffuse_attribute.clone()],
    });
    let shader_boundary = PpcQ3ShaderBoundaryRecord {
        shader,
        u_boundary: PPC_Q3_SHADER_UV_BOUNDARY_CLAMP,
        v_boundary: PPC_Q3_SHADER_UV_BOUNDARY_WRAP,
    };
    loaded.q3_shader_boundaries.push(shader_boundary);
    let texture_shader = PpcQ3TextureShaderRecord { shader, texture };
    loaded.q3_texture_shaders.push(texture_shader);
    let mipmap_texture = PpcQ3MipmapTextureRecord {
        texture,
        mipmap: vec![0x5a; PPC_Q3_MIPMAP_COPY_SIZE as usize],
    };
    loaded.q3_mipmap_textures.push(mipmap_texture.clone());

    loaded.cpu.gpr[3] = trimesh_data;
    loaded.cpu.gpr[4] = view;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], 1);
    assert_eq!(
        loaded.q3_scene_commands(),
        vec![PpcQ3SceneCommand::TriMesh(PpcQ3SceneTriMeshCommand {
            submission_index: 0,
            view_state: loaded.q3_views[0],
            camera: Some(camera_record),
            geometry: PpcQ3SceneTriMeshGeometry {
                source: PpcQ3SceneTriMeshSource::DataPtr(trimesh_data),
                data: trimesh,
            },
            local_to_world,
            material: PpcQ3SubmissionMaterialRecord {
                view,
                kind: PpcQ3SubmissionKind::TriMesh,
                primary: trimesh_data,
                secondary: 0,
                shader,
                illumination_type: PPC_Q3_ILLUMINATION_TYPE_PHONG,
                styles: vec![fill_style],
                fog_style: Some(fog_style),
                attributes: vec![diffuse_attribute],
                shader_uv_transform: None,
                shader_boundary: Some(shader_boundary),
                texture_shader: Some(texture_shader),
                mipmap_texture: Some(mipmap_texture),
            },
            lights: PpcQ3SubmissionLightRecord {
                view,
                kind: PpcQ3SubmissionKind::TriMesh,
                primary: trimesh_data,
                secondary: 0,
                light_group,
                lights: vec![light_record],
            },
        })]
    );

    let object_trimesh = PPC_Q3_OBJECT_BASE + PPC_Q3_OBJECT_STRIDE * 8;
    let object_trimesh_data: Vec<u8> = (0..PPC_Q3_TRIMESH_DATA_SIZE)
        .map(|offset| 0xa0u8.wrapping_add(offset as u8))
        .collect();
    loaded.q3_objects.push(PpcQ3ObjectRecord {
        object: object_trimesh,
        kind: PpcQ3ObjectKind::Generic,
        object_type: PPC_Q3_TYPE_TRIMESH,
        source: PpcQ3ObjectSource::default(),
        data_ptr: 0,
        data_size: PPC_Q3_TRIMESH_DATA_SIZE,
    });
    loaded.q3_trimeshes.push(PpcQ3TriMeshRecord {
        trimesh: object_trimesh,
        data: object_trimesh_data.clone(),
        triangle_attribute_sets: Vec::new(),
        get_data_copies: Vec::new(),
    });
    loaded.q3_submissions.push(PpcQ3SubmissionRecord {
        view,
        kind: PpcQ3SubmissionKind::TriMesh,
        primary: object_trimesh,
        secondary: 0,
    });
    loaded
        .q3_submission_transforms
        .push(PpcQ3SubmissionTransformRecord {
            view,
            kind: PpcQ3SubmissionKind::TriMesh,
            primary: object_trimesh,
            secondary: 0,
            local_to_world,
        });
    let mut object_material = loaded.q3_submission_materials[0].clone();
    object_material.primary = object_trimesh;
    loaded.q3_submission_materials.push(object_material);
    let mut object_lights = loaded.q3_submission_lights[0].clone();
    object_lights.primary = object_trimesh;
    loaded.q3_submission_lights.push(object_lights);

    let commands = loaded.q3_scene_commands();
    assert_eq!(commands.len(), 2);
    let PpcQ3SceneCommand::TriMesh(command) = &commands[1] else {
        panic!("expected TriMesh scene command");
    };
    assert_eq!(command.submission_index, 1);
    assert_eq!(
        command.geometry.source,
        PpcQ3SceneTriMeshSource::Object(object_trimesh)
    );
    assert_eq!(command.geometry.data, object_trimesh_data);
    assert_eq!(command.material.primary, object_trimesh);
    assert_eq!(command.lights.primary, object_trimesh);

    let style_submission = PpcQ3SubmissionRecord {
        view,
        kind: PpcQ3SubmissionKind::Style,
        primary: style,
        secondary: 0,
    };
    loaded.q3_submissions.push(style_submission);
    loaded
        .q3_submission_transforms
        .push(PpcQ3SubmissionTransformRecord {
            view,
            kind: PpcQ3SubmissionKind::Style,
            primary: style,
            secondary: 0,
            local_to_world,
        });
    let mut style_material = loaded.q3_submission_materials[0].clone();
    style_material.kind = PpcQ3SubmissionKind::Style;
    style_material.primary = style;
    loaded.q3_submission_materials.push(style_material);
    let mut style_lights = loaded.q3_submission_lights[0].clone();
    style_lights.kind = PpcQ3SubmissionKind::Style;
    style_lights.primary = style;
    loaded.q3_submission_lights.push(style_lights);

    let commands = loaded.q3_scene_commands();
    assert_eq!(commands.len(), 3);
    let PpcQ3SceneCommand::Submission(command) = &commands[2] else {
        panic!("expected non-TriMesh scene command");
    };
    assert_eq!(command.submission_index, 2);
    assert_eq!(command.view_state.view, view);
    assert_eq!(command.submission, style_submission);
    assert_eq!(command.local_to_world, local_to_world);
    assert_eq!(command.material.primary, style);
    assert_eq!(command.lights.primary, style);
}

#[test]
fn q3_software_renderer_replays_trimesh_scene_commands_to_front_buffer() {
    let view = PPC_Q3_OBJECT_BASE;
    let attribute_set = PPC_Q3_OBJECT_BASE + PPC_Q3_OBJECT_STRIDE;
    let trimesh_data = PPC_DATA_BASE + 0x1000;
    let points_ptr = PPC_DATA_BASE + 0x1080;
    let triangles_ptr = PPC_DATA_BASE + 0x1100;
    let front_base = PPC_HEAP_BASE + 0x1000;
    let pef = synthetic_pef_with_library_import(b"QuickDraw\xaa 3D", b"Q3TriMesh_Submit");
    let mut loaded = load_pef_application(&pef).unwrap();
    loaded.memory.add_region(trimesh_data, vec![0; 0x200]);
    loaded.memory.add_region(front_base, vec![0; 8 * 8 * 2]);
    loaded.gworlds = vec![PpcGWorldRecord {
        ui_theme: crate::ui_theme::UiThemeId::ClassicSystem7,
        port: PPC_MAIN_GWORLD,
        pixmap_handle: 0,
        pixmap: 0,
        base_addr: front_base,
        gdevice: PPC_MAIN_GDEVICE,
        width: 8,
        height: 8,
        depth: 16,
        row_bytes: 16,
        pixels_locked: false,
        pixels_no_purge: false,
    }];
    loaded
        .current_gworld
        .with_mut(|current_gworld| *current_gworld = PPC_MAIN_GWORLD);

    loaded
        .memory
        .write_u32_be(trimesh_data + PPC_Q3_TRIMESH_NUM_TRIANGLES_OFFSET, 1)
        .unwrap();
    loaded
        .memory
        .write_u32_be(
            trimesh_data + PPC_Q3_TRIMESH_TRIANGLES_OFFSET,
            triangles_ptr,
        )
        .unwrap();
    loaded
        .memory
        .write_u32_be(trimesh_data + PPC_Q3_TRIMESH_NUM_POINTS_OFFSET, 3)
        .unwrap();
    loaded
        .memory
        .write_u32_be(trimesh_data + PPC_Q3_TRIMESH_POINTS_OFFSET, points_ptr)
        .unwrap();
    ppc_write_q3_vector3d(&mut loaded.memory, points_ptr, (0.0, 0.0, 0.0)).unwrap();
    ppc_write_q3_vector3d(&mut loaded.memory, points_ptr + 12, (0.0, 0.9, 0.0)).unwrap();
    ppc_write_q3_vector3d(&mut loaded.memory, points_ptr + 24, (0.9, 0.0, 0.0)).unwrap();
    loaded.memory.write_u32_be(triangles_ptr, 0).unwrap();
    loaded.memory.write_u32_be(triangles_ptr + 4, 1).unwrap();
    loaded.memory.write_u32_be(triangles_ptr + 8, 2).unwrap();

    let mut diffuse = Vec::new();
    for value in [0.0f32, 1.0, 0.0] {
        diffuse.extend_from_slice(&value.to_bits().to_be_bytes());
    }
    loaded.q3_views.push(PpcQ3ViewStateRecord::new(view));
    loaded.q3_submissions.push(PpcQ3SubmissionRecord {
        view,
        kind: PpcQ3SubmissionKind::TriMesh,
        primary: trimesh_data,
        secondary: 0,
    });
    loaded
        .q3_submission_transforms
        .push(PpcQ3SubmissionTransformRecord {
            view,
            kind: PpcQ3SubmissionKind::TriMesh,
            primary: trimesh_data,
            secondary: 0,
            local_to_world: ppc_q3_matrix4x4_identity(),
        });
    loaded
        .q3_submission_materials
        .push(PpcQ3SubmissionMaterialRecord {
            view,
            kind: PpcQ3SubmissionKind::TriMesh,
            primary: trimesh_data,
            secondary: 0,
            shader: 0,
            illumination_type: PPC_Q3_ILLUMINATION_TYPE_PHONG,
            styles: Vec::new(),
            fog_style: None,
            attributes: vec![PpcQ3AttributeRecord {
                attribute_set,
                attribute_type: PPC_Q3_ATTRIBUTE_TYPE_DIFFUSE_COLOR,
                data: diffuse,
            }],
            shader_uv_transform: None,
            shader_boundary: None,
            texture_shader: None,
            mipmap_texture: None,
        });
    loaded
        .q3_submission_lights
        .push(PpcQ3SubmissionLightRecord {
            view,
            kind: PpcQ3SubmissionKind::TriMesh,
            primary: trimesh_data,
            secondary: 0,
            light_group: 0,
            lights: Vec::new(),
        });

    let captured_commands = loaded.q3_scene_commands();
    assert_eq!(captured_commands.len(), 1);
    let replay = loaded.capture_q3_scene_replay(captured_commands);
    assert!(replay
        .memory_regions
        .iter()
        .any(|region| region.base_addr == points_ptr && region.data.len() == 36));
    assert!(replay
        .memory_regions
        .iter()
        .any(|region| region.base_addr == triangles_ptr && region.data.len() == 12));
    let replay_json = replay.to_json_pretty().unwrap();
    assert!(replay_json.contains("\"commands\""));
    assert!(replay_json.contains("\"memory_regions\""));
    let replay = PpcQ3SceneReplay::from_json_str(&replay_json).unwrap();

    loaded.q3_completed_frames.push(PpcQ3CompletedFrameRecord {
        view,
        submissions: loaded.q3_submissions.clone(),
        submission_transforms: loaded.q3_submission_transforms.clone(),
        submission_materials: loaded.q3_submission_materials.clone(),
        submission_lights: loaded.q3_submission_lights.clone(),
        retained_trimeshes: Vec::new(),
    });
    let gpu_frame = loaded
        .take_completed_q3_gpu_frame()
        .expect("simple filled triangle should use the GPU packet path");
    assert_eq!((gpu_frame.width, gpu_frame.height), (8, 8));
    assert_eq!(gpu_frame.viewport, [0, 0, 7, 7]);
    assert!(gpu_frame.textures.is_empty());
    assert_eq!(gpu_frame.draws.len(), 1);
    assert_eq!(gpu_frame.draws[0].vertices.len(), 3);
    assert_eq!(gpu_frame.draws[0].vertices[0].color, [0.0, 1.0, 0.0, 1.0]);
    assert!(loaded.q3_completed_frames.is_empty());
    assert_eq!(
        loaded.memory.read_u16_be(front_base + 4 * 16 + 4 * 2),
        Some(0)
    );

    loaded.q3_submission_lights[0]
        .lights
        .push(PpcQ3LightRecord {
            light: PPC_Q3_OBJECT_BASE + PPC_Q3_OBJECT_STRIDE * 2,
            light_type: PPC_Q3_LIGHT_TYPE_AMBIENT,
            data: PpcQ3LightData {
                is_on: 1,
                brightness: 1.0,
                color: (1.0, 1.0, 1.0),
            },
            kind: PpcQ3LightKind::Ambient,
        });
    loaded.q3_completed_frames.push(PpcQ3CompletedFrameRecord {
        view,
        submissions: loaded.q3_submissions.clone(),
        submission_transforms: loaded.q3_submission_transforms.clone(),
        submission_materials: loaded.q3_submission_materials.clone(),
        submission_lights: loaded.q3_submission_lights.clone(),
        retained_trimeshes: Vec::new(),
    });
    assert!(loaded.take_completed_q3_gpu_frame().is_none());
    assert_eq!(loaded.q3_completed_frames.len(), 1);
    let fallback_stats = loaded.render_completed_q3_frames_to_front_buffer_fast();
    assert_eq!(fallback_stats.frames, 1);
    assert!(fallback_stats.pixels >= 8);
    assert!(loaded.q3_completed_frames.is_empty());

    let mut replay_loaded = load_pef_application(&pef).unwrap();
    replay_loaded
        .memory
        .add_region(front_base, vec![0; 8 * 8 * 2]);
    replay_loaded.gworlds = vec![PpcGWorldRecord {
        ui_theme: crate::ui_theme::UiThemeId::ClassicSystem7,
        port: PPC_MAIN_GWORLD,
        pixmap_handle: 0,
        pixmap: 0,
        base_addr: front_base,
        gdevice: PPC_MAIN_GDEVICE,
        width: 8,
        height: 8,
        depth: 16,
        row_bytes: 16,
        pixels_locked: false,
        pixels_no_purge: false,
    }];
    replay_loaded
        .current_gworld
        .with_mut(|current_gworld| *current_gworld = PPC_MAIN_GWORLD);
    assert!(replay_loaded.q3_submissions.is_empty());
    assert!(replay_loaded.q3_submission_transforms.is_empty());
    assert!(replay_loaded.q3_submission_materials.is_empty());
    assert!(replay_loaded.q3_submission_lights.is_empty());

    let stats = replay_loaded.replay_q3_scene_replay_to_front_buffer(&replay);

    assert_eq!(stats.commands, 1);
    assert_eq!(stats.vertices, 3);
    assert_eq!(stats.triangles, 1);
    assert!(stats.pixels >= 8);
    assert_eq!(
        replay_loaded
            .memory
            .read_u16_be(front_base + 4 * 16 + 4 * 2),
        Some(0x03e0)
    );
    assert_eq!(
        replay_loaded
            .memory
            .read_u16_be(front_base + 4 * 2),
        Some(0x03e0)
    );
    assert_eq!(
        replay_loaded
            .memory
            .read_u16_be(front_base + 4 * 16 + 7 * 2),
        Some(0x03e0)
    );
    assert_eq!(
        replay_loaded
            .memory
            .read_u16_be(front_base + 3 * 16 + 5 * 2),
        Some(0x03e0)
    );
}

#[test]
fn q3_scene_replay_capture_includes_texture_image_memory() {
    let view = PPC_Q3_OBJECT_BASE;
    let shader = PPC_Q3_OBJECT_BASE + PPC_Q3_OBJECT_STRIDE;
    let texture = PPC_Q3_OBJECT_BASE + PPC_Q3_OBJECT_STRIDE * 2;
    let storage = PPC_Q3_OBJECT_BASE + PPC_Q3_OBJECT_STRIDE * 3;
    let image_ptr = PPC_DATA_BASE + 0x1000;
    let pef = synthetic_pef_with_library_import(b"QuickDraw\xaa 3D", b"Q3TriMesh_Submit");
    let mut loaded = load_pef_application(&pef).unwrap();
    loaded.memory.add_region(
        image_ptr,
        vec![0xee, 0xdd, 0xff, 0x00, 0x00, 0x00, 0xff, 0x00],
    );

    let mut mipmap = vec![0; PPC_Q3_MIPMAP_COPY_SIZE as usize];
    mipmap[PPC_Q3_MIPMAP_IMAGE_OFFSET as usize..][..4].copy_from_slice(&storage.to_be_bytes());
    mipmap[PPC_Q3_MIPMAP_PIXEL_TYPE_OFFSET as usize..][..4]
        .copy_from_slice(&PPC_Q3_PIXEL_TYPE_RGB24.to_be_bytes());
    mipmap[PPC_Q3_MIPMAP_BYTE_ORDER_OFFSET as usize..][..4]
        .copy_from_slice(&PPC_Q3_ENDIAN_BIG.to_be_bytes());
    mipmap[PPC_Q3_MIPMAP_BASE_WIDTH_OFFSET as usize..][..4]
        .copy_from_slice(&2u32.to_be_bytes());
    mipmap[PPC_Q3_MIPMAP_BASE_HEIGHT_OFFSET as usize..][..4]
        .copy_from_slice(&1u32.to_be_bytes());
    mipmap[PPC_Q3_MIPMAP_BASE_ROW_BYTES_OFFSET as usize..][..4]
        .copy_from_slice(&6u32.to_be_bytes());
    mipmap[PPC_Q3_MIPMAP_BASE_IMAGE_OFFSET_OFFSET as usize..][..4]
        .copy_from_slice(&2u32.to_be_bytes());

    loaded.q3_memory_storages.push(PpcQ3MemoryStorageRecord {
        storage,
        buffer_ptr: image_ptr,
        valid_size: 8,
        buffer_size: 8,
        owns_buffer: false,
    });
    let material = PpcQ3SubmissionMaterialRecord {
        view,
        kind: PpcQ3SubmissionKind::Shader,
        primary: shader,
        secondary: 0,
        shader,
        illumination_type: PPC_Q3_ILLUMINATION_TYPE_PHONG,
        styles: Vec::new(),
        fog_style: None,
        attributes: Vec::new(),
        shader_uv_transform: None,
        shader_boundary: None,
        texture_shader: Some(PpcQ3TextureShaderRecord { shader, texture }),
        mipmap_texture: Some(PpcQ3MipmapTextureRecord { texture, mipmap }),
    };
    let replay = loaded.capture_q3_scene_replay(vec![PpcQ3SceneCommand::Submission(
        PpcQ3SceneSubmissionCommand {
            submission_index: 0,
            view_state: PpcQ3ViewStateRecord::new(view),
            submission: PpcQ3SubmissionRecord {
                view,
                kind: PpcQ3SubmissionKind::Shader,
                primary: shader,
                secondary: 0,
            },
            local_to_world: ppc_q3_matrix4x4_identity(),
            material,
            lights: PpcQ3SubmissionLightRecord {
                view,
                kind: PpcQ3SubmissionKind::Shader,
                primary: shader,
                secondary: 0,
                light_group: 0,
                lights: Vec::new(),
            },
        },
    )]);

    assert_eq!(replay.memory_regions.len(), 1);
    assert_eq!(replay.memory_regions[0].base_addr, image_ptr + 2);
    assert_eq!(
        replay.memory_regions[0].data,
        vec![0xff, 0x00, 0x00, 0x00, 0xff, 0x00]
    );
}

#[test]
fn q3_software_renderer_uses_pixmap_draw_context_destination() {
    fn write_u32_into_slice(data: &mut [u8], offset: u32, value: u32) {
        let offset = offset as usize;
        data[offset..offset + 4].copy_from_slice(&value.to_be_bytes());
    }

    let view = PPC_Q3_OBJECT_BASE;
    let attribute_set = PPC_Q3_OBJECT_BASE + PPC_Q3_OBJECT_STRIDE;
    let draw_context = PPC_Q3_OBJECT_BASE + PPC_Q3_OBJECT_STRIDE * 2;
    let trimesh_data = PPC_DATA_BASE + 0x1000;
    let points_ptr = PPC_DATA_BASE + 0x1080;
    let triangles_ptr = PPC_DATA_BASE + 0x1100;
    let front_base = PPC_HEAP_BASE + 0x1000;
    let pixmap_base = PPC_HEAP_BASE + 0x3000;
    let pef = synthetic_pef_with_library_import(b"QuickDraw\xaa 3D", b"Q3TriMesh_Submit");
    let mut loaded = load_pef_application(&pef).unwrap();
    loaded.memory.add_region(trimesh_data, vec![0; 0x200]);
    loaded.memory.add_region(front_base, vec![0; 8 * 8 * 2]);
    loaded.memory.add_region(pixmap_base, vec![0; 8 * 8 * 2]);
    loaded.gworlds = vec![PpcGWorldRecord {
        ui_theme: crate::ui_theme::UiThemeId::ClassicSystem7,
        port: PPC_MAIN_GWORLD,
        pixmap_handle: 0,
        pixmap: 0,
        base_addr: front_base,
        gdevice: PPC_MAIN_GDEVICE,
        width: 8,
        height: 8,
        depth: 16,
        row_bytes: 16,
        pixels_locked: false,
        pixels_no_purge: false,
    }];
    loaded
        .current_gworld
        .with_mut(|current_gworld| *current_gworld = PPC_MAIN_GWORLD);
    loaded.q3_objects.push(PpcQ3ObjectRecord {
        object: draw_context,
        kind: PpcQ3ObjectKind::Generic,
        object_type: PPC_Q3_DRAW_CONTEXT_TYPE_PIXMAP,
        source: PpcQ3ObjectSource::default(),
        data_ptr: 0,
        data_size: PPC_Q3_PIXMAP_DRAW_CONTEXT_DATA_SIZE,
    });
    let mut draw_context_data = vec![0; PPC_Q3_PIXMAP_DRAW_CONTEXT_DATA_SIZE as usize];
    write_u32_into_slice(
        &mut draw_context_data,
        PPC_Q3_DRAW_CONTEXT_DATA_SIZE,
        pixmap_base,
    );
    write_u32_into_slice(
        &mut draw_context_data,
        PPC_Q3_PIXMAP_DRAW_CONTEXT_WIDTH_OFFSET,
        8,
    );
    write_u32_into_slice(
        &mut draw_context_data,
        PPC_Q3_PIXMAP_DRAW_CONTEXT_HEIGHT_OFFSET,
        8,
    );
    write_u32_into_slice(
        &mut draw_context_data,
        PPC_Q3_DRAW_CONTEXT_DATA_SIZE + 12,
        16,
    );
    write_u32_into_slice(
        &mut draw_context_data,
        PPC_Q3_DRAW_CONTEXT_DATA_SIZE + 16,
        16,
    );
    write_u32_into_slice(
        &mut draw_context_data,
        PPC_Q3_DRAW_CONTEXT_DATA_SIZE + 20,
        PPC_Q3_PIXEL_TYPE_RGB16,
    );
    loaded.q3_draw_contexts.push(PpcQ3DrawContextRecord {
        draw_context,
        draw_context_type: PPC_Q3_DRAW_CONTEXT_TYPE_PIXMAP,
        data: draw_context_data,
    });

    loaded
        .memory
        .write_u32_be(trimesh_data + PPC_Q3_TRIMESH_NUM_TRIANGLES_OFFSET, 1)
        .unwrap();
    loaded
        .memory
        .write_u32_be(
            trimesh_data + PPC_Q3_TRIMESH_TRIANGLES_OFFSET,
            triangles_ptr,
        )
        .unwrap();
    loaded
        .memory
        .write_u32_be(trimesh_data + PPC_Q3_TRIMESH_NUM_POINTS_OFFSET, 3)
        .unwrap();
    loaded
        .memory
        .write_u32_be(trimesh_data + PPC_Q3_TRIMESH_POINTS_OFFSET, points_ptr)
        .unwrap();
    ppc_write_q3_vector3d(&mut loaded.memory, points_ptr, (0.0, 0.0, 0.0)).unwrap();
    ppc_write_q3_vector3d(&mut loaded.memory, points_ptr + 12, (0.0, 0.9, 0.0)).unwrap();
    ppc_write_q3_vector3d(&mut loaded.memory, points_ptr + 24, (0.9, 0.0, 0.0)).unwrap();
    loaded.memory.write_u32_be(triangles_ptr, 0).unwrap();
    loaded.memory.write_u32_be(triangles_ptr + 4, 1).unwrap();
    loaded.memory.write_u32_be(triangles_ptr + 8, 2).unwrap();

    let mut diffuse = Vec::new();
    for value in [0.0f32, 1.0, 0.0] {
        diffuse.extend_from_slice(&value.to_bits().to_be_bytes());
    }
    let mut view_state = PpcQ3ViewStateRecord::new(view);
    view_state.draw_context = draw_context;
    loaded.q3_views.push(view_state);
    loaded.q3_submissions.push(PpcQ3SubmissionRecord {
        view,
        kind: PpcQ3SubmissionKind::TriMesh,
        primary: trimesh_data,
        secondary: 0,
    });
    loaded
        .q3_submission_transforms
        .push(PpcQ3SubmissionTransformRecord {
            view,
            kind: PpcQ3SubmissionKind::TriMesh,
            primary: trimesh_data,
            secondary: 0,
            local_to_world: ppc_q3_matrix4x4_identity(),
        });
    loaded
        .q3_submission_materials
        .push(PpcQ3SubmissionMaterialRecord {
            view,
            kind: PpcQ3SubmissionKind::TriMesh,
            primary: trimesh_data,
            secondary: 0,
            shader: 0,
            illumination_type: PPC_Q3_ILLUMINATION_TYPE_PHONG,
            styles: Vec::new(),
            fog_style: None,
            attributes: vec![PpcQ3AttributeRecord {
                attribute_set,
                attribute_type: PPC_Q3_ATTRIBUTE_TYPE_DIFFUSE_COLOR,
                data: diffuse,
            }],
            shader_uv_transform: None,
            shader_boundary: None,
            texture_shader: None,
            mipmap_texture: None,
        });
    loaded
        .q3_submission_lights
        .push(PpcQ3SubmissionLightRecord {
            view,
            kind: PpcQ3SubmissionKind::TriMesh,
            primary: trimesh_data,
            secondary: 0,
            light_group: 0,
            lights: Vec::new(),
        });

    let stats = loaded.render_q3_scene_commands_to_front_buffer();

    assert_eq!(stats.commands, 1);
    assert_eq!(stats.triangles, 1);
    assert!(stats.pixels >= 8);
    for offset in [4 * 16 + 4 * 2, 4 * 2, 4 * 16 + 7 * 2] {
        assert_eq!(
            loaded.memory.read_u16_be(pixmap_base + offset),
            Some(0x03e0)
        );
        assert_eq!(loaded.memory.read_u16_be(front_base + offset), Some(0));
    }
}

#[test]
fn q3_software_renderer_uses_mac_draw_context_registered_port_destination() {
    let view = PPC_Q3_OBJECT_BASE;
    let attribute_set = PPC_Q3_OBJECT_BASE + PPC_Q3_OBJECT_STRIDE;
    let draw_context = PPC_Q3_OBJECT_BASE + PPC_Q3_OBJECT_STRIDE * 2;
    let trimesh_data = PPC_DATA_BASE + 0x1000;
    let points_ptr = PPC_DATA_BASE + 0x1080;
    let triangles_ptr = PPC_DATA_BASE + 0x1100;
    let main_base = PPC_HEAP_BASE + 0x1000;
    let window_port = PPC_HEAP_BASE + 0x2000;
    let window_base = PPC_HEAP_BASE + 0x3000;
    let pef = synthetic_pef_with_library_import(b"QuickDraw\xaa 3D", b"Q3TriMesh_Submit");
    let mut loaded = load_pef_application(&pef).unwrap();
    loaded.memory.add_region(trimesh_data, vec![0; 0x200]);
    loaded.memory.add_region(main_base, vec![0; 8 * 8 * 2]);
    loaded.memory.add_region(window_base, vec![0; 8 * 8 * 2]);
    loaded.gworlds = vec![
        PpcGWorldRecord {
            ui_theme: crate::ui_theme::UiThemeId::ClassicSystem7,
            port: PPC_MAIN_GWORLD,
            pixmap_handle: 0,
            pixmap: 0,
            base_addr: main_base,
            gdevice: PPC_MAIN_GDEVICE,
            width: 8,
            height: 8,
            depth: 16,
            row_bytes: 16,
            pixels_locked: false,
            pixels_no_purge: false,
        },
        PpcGWorldRecord {
            ui_theme: crate::ui_theme::UiThemeId::ClassicSystem7,
            port: window_port,
            pixmap_handle: 0,
            pixmap: 0,
            base_addr: window_base,
            gdevice: PPC_MAIN_GDEVICE,
            width: 8,
            height: 8,
            depth: 16,
            row_bytes: 16,
            pixels_locked: false,
            pixels_no_purge: false,
        },
    ];
    loaded
        .current_gworld
        .with_mut(|current_gworld| *current_gworld = PPC_MAIN_GWORLD);
    loaded.q3_objects.push(PpcQ3ObjectRecord {
        object: draw_context,
        kind: PpcQ3ObjectKind::Generic,
        object_type: PPC_Q3_DRAW_CONTEXT_TYPE_MACINTOSH,
        source: PpcQ3ObjectSource::default(),
        data_ptr: 0,
        data_size: PPC_Q3_MAC_DRAW_CONTEXT_DATA_SIZE,
    });
    let mut draw_context_data = vec![0; PPC_Q3_MAC_DRAW_CONTEXT_DATA_SIZE as usize];
    draw_context_data[PPC_Q3_MAC_DRAW_CONTEXT_PORT_OFFSET as usize
        ..PPC_Q3_MAC_DRAW_CONTEXT_PORT_OFFSET as usize + 4]
        .copy_from_slice(&window_port.to_be_bytes());
    loaded.q3_draw_contexts.push(PpcQ3DrawContextRecord {
        draw_context,
        draw_context_type: PPC_Q3_DRAW_CONTEXT_TYPE_MACINTOSH,
        data: draw_context_data,
    });

    loaded
        .memory
        .write_u32_be(trimesh_data + PPC_Q3_TRIMESH_NUM_TRIANGLES_OFFSET, 1)
        .unwrap();
    loaded
        .memory
        .write_u32_be(
            trimesh_data + PPC_Q3_TRIMESH_TRIANGLES_OFFSET,
            triangles_ptr,
        )
        .unwrap();
    loaded
        .memory
        .write_u32_be(trimesh_data + PPC_Q3_TRIMESH_NUM_POINTS_OFFSET, 3)
        .unwrap();
    loaded
        .memory
        .write_u32_be(trimesh_data + PPC_Q3_TRIMESH_POINTS_OFFSET, points_ptr)
        .unwrap();
    ppc_write_q3_vector3d(&mut loaded.memory, points_ptr, (0.0, 0.0, 0.0)).unwrap();
    ppc_write_q3_vector3d(&mut loaded.memory, points_ptr + 12, (0.0, 0.9, 0.0)).unwrap();
    ppc_write_q3_vector3d(&mut loaded.memory, points_ptr + 24, (0.9, 0.0, 0.0)).unwrap();
    loaded.memory.write_u32_be(triangles_ptr, 0).unwrap();
    loaded.memory.write_u32_be(triangles_ptr + 4, 1).unwrap();
    loaded.memory.write_u32_be(triangles_ptr + 8, 2).unwrap();

    let mut diffuse = Vec::new();
    for value in [0.0f32, 1.0, 0.0] {
        diffuse.extend_from_slice(&value.to_bits().to_be_bytes());
    }
    let mut view_state = PpcQ3ViewStateRecord::new(view);
    view_state.draw_context = draw_context;
    loaded.q3_views.push(view_state);
    loaded.q3_submissions.push(PpcQ3SubmissionRecord {
        view,
        kind: PpcQ3SubmissionKind::TriMesh,
        primary: trimesh_data,
        secondary: 0,
    });
    loaded
        .q3_submission_transforms
        .push(PpcQ3SubmissionTransformRecord {
            view,
            kind: PpcQ3SubmissionKind::TriMesh,
            primary: trimesh_data,
            secondary: 0,
            local_to_world: ppc_q3_matrix4x4_identity(),
        });
    loaded
        .q3_submission_materials
        .push(PpcQ3SubmissionMaterialRecord {
            view,
            kind: PpcQ3SubmissionKind::TriMesh,
            primary: trimesh_data,
            secondary: 0,
            shader: 0,
            illumination_type: PPC_Q3_ILLUMINATION_TYPE_PHONG,
            styles: Vec::new(),
            fog_style: None,
            attributes: vec![PpcQ3AttributeRecord {
                attribute_set,
                attribute_type: PPC_Q3_ATTRIBUTE_TYPE_DIFFUSE_COLOR,
                data: diffuse,
            }],
            shader_uv_transform: None,
            shader_boundary: None,
            texture_shader: None,
            mipmap_texture: None,
        });
    loaded
        .q3_submission_lights
        .push(PpcQ3SubmissionLightRecord {
            view,
            kind: PpcQ3SubmissionKind::TriMesh,
            primary: trimesh_data,
            secondary: 0,
            light_group: 0,
            lights: Vec::new(),
        });

    let stats = loaded.render_q3_scene_commands_to_front_buffer();

    assert_eq!(stats.commands, 1);
    assert_eq!(stats.triangles, 1);
    assert!(stats.pixels >= 8);
    for offset in [4 * 16 + 4 * 2, 4 * 2, 4 * 16 + 7 * 2] {
        assert_eq!(
            loaded.memory.read_u16_be(window_base + offset),
            Some(0x03e0)
        );
        assert_eq!(loaded.memory.read_u16_be(main_base + offset), Some(0));
    }
}

#[test]
fn q3_completed_frame_renders_to_dsp_back_buffer_mac_draw_context_until_swap() {
    let view = PPC_Q3_OBJECT_BASE;
    let attribute_set = PPC_Q3_OBJECT_BASE + PPC_Q3_OBJECT_STRIDE;
    let draw_context = PPC_Q3_OBJECT_BASE + PPC_Q3_OBJECT_STRIDE * 2;
    let trimesh_data = PPC_DATA_BASE + 0x1000;
    let points_ptr = PPC_DATA_BASE + 0x1080;
    let triangles_ptr = PPC_DATA_BASE + 0x1100;
    let main_base = PPC_HEAP_BASE + 0x1000;
    let back_base = PPC_HEAP_BASE + 0x3000;
    let pef = synthetic_pef_with_library_import(b"DrawSprocketLib", b"DSpContext_SwapBuffers");
    let mut loaded = load_pef_application(&pef).unwrap();
    loaded.memory.add_region(trimesh_data, vec![0; 0x200]);
    loaded.memory.add_region(main_base, vec![0; 8 * 8 * 2]);
    loaded.memory.add_region(back_base, vec![0; 8 * 8 * 2]);
    loaded.gworlds = vec![
        PpcGWorldRecord {
            ui_theme: crate::ui_theme::UiThemeId::ClassicSystem7,
            port: PPC_MAIN_GWORLD,
            pixmap_handle: 0,
            pixmap: 0,
            base_addr: main_base,
            gdevice: PPC_MAIN_GDEVICE,
            width: 8,
            height: 8,
            depth: 16,
            row_bytes: 16,
            pixels_locked: false,
            pixels_no_purge: false,
        },
        PpcGWorldRecord {
            ui_theme: crate::ui_theme::UiThemeId::ClassicSystem7,
            port: PPC_DSP_BACK_GWORLD,
            pixmap_handle: 0,
            pixmap: 0,
            base_addr: back_base,
            gdevice: PPC_MAIN_GDEVICE,
            width: 8,
            height: 8,
            depth: 16,
            row_bytes: 16,
            pixels_locked: false,
            pixels_no_purge: false,
        },
    ];
    loaded
        .current_gworld
        .with_mut(|current_gworld| *current_gworld = PPC_MAIN_GWORLD);
    loaded.draw_sprocket.front_buffer_gworld = PPC_MAIN_GWORLD;
    loaded.draw_sprocket.back_buffer_gworld = PPC_DSP_BACK_GWORLD;

    loaded
        .memory
        .write_u32_be(trimesh_data + PPC_Q3_TRIMESH_NUM_TRIANGLES_OFFSET, 1)
        .unwrap();
    loaded
        .memory
        .write_u32_be(
            trimesh_data + PPC_Q3_TRIMESH_TRIANGLES_OFFSET,
            triangles_ptr,
        )
        .unwrap();
    loaded
        .memory
        .write_u32_be(trimesh_data + PPC_Q3_TRIMESH_NUM_POINTS_OFFSET, 3)
        .unwrap();
    loaded
        .memory
        .write_u32_be(trimesh_data + PPC_Q3_TRIMESH_POINTS_OFFSET, points_ptr)
        .unwrap();
    ppc_write_q3_vector3d(&mut loaded.memory, points_ptr, (0.0, 0.0, 0.0)).unwrap();
    ppc_write_q3_vector3d(&mut loaded.memory, points_ptr + 12, (0.0, 0.9, 0.0)).unwrap();
    ppc_write_q3_vector3d(&mut loaded.memory, points_ptr + 24, (0.9, 0.0, 0.0)).unwrap();
    loaded.memory.write_u32_be(triangles_ptr, 0).unwrap();
    loaded.memory.write_u32_be(triangles_ptr + 4, 1).unwrap();
    loaded.memory.write_u32_be(triangles_ptr + 8, 2).unwrap();

    loaded.q3_objects.push(test_q3_object(
        draw_context,
        PPC_Q3_DRAW_CONTEXT_TYPE_MACINTOSH,
    ));
    let mut draw_context_data = vec![0; PPC_Q3_MAC_DRAW_CONTEXT_DATA_SIZE as usize];
    draw_context_data[PPC_Q3_MAC_DRAW_CONTEXT_PORT_OFFSET as usize
        ..PPC_Q3_MAC_DRAW_CONTEXT_PORT_OFFSET as usize + 4]
        .copy_from_slice(&PPC_DSP_BACK_GWORLD.to_be_bytes());
    loaded.q3_draw_contexts.push(PpcQ3DrawContextRecord {
        draw_context,
        draw_context_type: PPC_Q3_DRAW_CONTEXT_TYPE_MACINTOSH,
        data: draw_context_data,
    });

    let mut diffuse = Vec::new();
    for value in [0.0f32, 1.0, 0.0] {
        diffuse.extend_from_slice(&value.to_bits().to_be_bytes());
    }
    let mut view_state = PpcQ3ViewStateRecord::new(view);
    view_state.draw_context = draw_context;
    loaded.q3_views.push(view_state);
    let submission = PpcQ3SubmissionRecord {
        view,
        kind: PpcQ3SubmissionKind::TriMesh,
        primary: trimesh_data,
        secondary: 0,
    };
    let transform = PpcQ3SubmissionTransformRecord {
        view,
        kind: PpcQ3SubmissionKind::TriMesh,
        primary: trimesh_data,
        secondary: 0,
        local_to_world: ppc_q3_matrix4x4_identity(),
    };
    let material = PpcQ3SubmissionMaterialRecord {
        view,
        kind: PpcQ3SubmissionKind::TriMesh,
        primary: trimesh_data,
        secondary: 0,
        shader: 0,
        illumination_type: PPC_Q3_ILLUMINATION_TYPE_PHONG,
        styles: Vec::new(),
        fog_style: None,
        attributes: vec![PpcQ3AttributeRecord {
            attribute_set,
            attribute_type: PPC_Q3_ATTRIBUTE_TYPE_DIFFUSE_COLOR,
            data: diffuse,
        }],
        shader_uv_transform: None,
        shader_boundary: None,
        texture_shader: None,
        mipmap_texture: None,
    };
    let lights = PpcQ3SubmissionLightRecord {
        view,
        kind: PpcQ3SubmissionKind::TriMesh,
        primary: trimesh_data,
        secondary: 0,
        light_group: 0,
        lights: Vec::new(),
    };
    loaded.q3_completed_frames.push(PpcQ3CompletedFrameRecord {
        view,
        submissions: vec![submission],
        submission_transforms: vec![transform],
        submission_materials: vec![material],
        submission_lights: vec![lights],
        retained_trimeshes: Vec::new(),
    });

    let stats = loaded.render_completed_q3_frames_to_front_buffer();

    assert_eq!(stats.frames, 1);
    assert_eq!(stats.commands, 1);
    assert_eq!(stats.triangles, 1);
    assert!(stats.pixels >= 8);
    assert!(loaded.q3_completed_frames.is_empty());
    assert_eq!(
        loaded
            .presented_front_buffer()
            .map(|front_buffer| front_buffer.base_addr),
        Some(main_base)
    );
    for offset in [4 * 16 + 4 * 2, 4 * 2, 4 * 16 + 7 * 2] {
        assert_eq!(loaded.memory.read_u16_be(back_base + offset), Some(0x03e0));
        assert_eq!(loaded.memory.read_u16_be(main_base + offset), Some(0));
    }

    loaded.draw_sprocket.started = true;
    loaded.draw_sprocket.reserved_context = Some(PPC_DSP_CONTEXT);
    loaded.draw_sprocket.active_context = Some(PPC_DSP_CONTEXT);
    loaded.cpu.gpr[3] = PPC_DSP_CONTEXT;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_NO_ERR));
    assert_eq!(loaded.draw_sprocket.front_buffer_gworld, PPC_MAIN_GWORLD);
    assert_eq!(loaded.draw_sprocket.back_buffer_gworld, PPC_DSP_BACK_GWORLD);
    assert_eq!(
        loaded
            .presented_front_buffer()
            .map(|front_buffer| front_buffer.base_addr),
        Some(main_base)
    );
    for offset in [4 * 16 + 4 * 2, 4 * 2, 4 * 16 + 7 * 2] {
        assert_eq!(loaded.memory.read_u16_be(main_base + offset), Some(0x03e0));
    }
}

#[test]
fn q3_software_renderer_respects_fill_style_edges_and_points() {
    fn diffuse_attribute(attribute_set: u32) -> PpcQ3AttributeRecord {
        let mut data = Vec::new();
        for value in [0.0f32, 1.0, 0.0] {
            data.extend_from_slice(&value.to_bits().to_be_bytes());
        }
        PpcQ3AttributeRecord {
            attribute_set,
            attribute_type: PPC_Q3_ATTRIBUTE_TYPE_DIFFUSE_COLOR,
            data,
        }
    }

    fn render_with_fill_style(
        fill_style: u32,
    ) -> (PpcLoadedApp, u32, PpcQ3SoftwareRenderStats) {
        let view = PPC_Q3_OBJECT_BASE;
        let attribute_set = PPC_Q3_OBJECT_BASE + PPC_Q3_OBJECT_STRIDE;
        let style = PPC_Q3_OBJECT_BASE + PPC_Q3_OBJECT_STRIDE * 2;
        let trimesh_data = PPC_DATA_BASE + 0x1000;
        let points_ptr = PPC_DATA_BASE + 0x1080;
        let triangles_ptr = PPC_DATA_BASE + 0x1100;
        let front_base = PPC_HEAP_BASE + 0x1000;
        let pef = synthetic_pef_with_library_import(b"QuickDraw\xaa 3D", b"Q3TriMesh_Submit");
        let mut loaded = load_pef_application(&pef).unwrap();
        loaded.memory.add_region(trimesh_data, vec![0; 0x200]);
        loaded.memory.add_region(front_base, vec![0; 8 * 8 * 2]);
        loaded.gworlds = vec![PpcGWorldRecord {
            ui_theme: crate::ui_theme::UiThemeId::ClassicSystem7,
            port: PPC_MAIN_GWORLD,
            pixmap_handle: 0,
            pixmap: 0,
            base_addr: front_base,
            gdevice: PPC_MAIN_GDEVICE,
            width: 8,
            height: 8,
            depth: 16,
            row_bytes: 16,
            pixels_locked: false,
            pixels_no_purge: false,
        }];
        loaded
            .current_gworld
            .with_mut(|current_gworld| *current_gworld = PPC_MAIN_GWORLD);

        loaded
            .memory
            .write_u32_be(trimesh_data + PPC_Q3_TRIMESH_NUM_TRIANGLES_OFFSET, 1)
            .unwrap();
        loaded
            .memory
            .write_u32_be(
                trimesh_data + PPC_Q3_TRIMESH_TRIANGLES_OFFSET,
                triangles_ptr,
            )
            .unwrap();
        loaded
            .memory
            .write_u32_be(trimesh_data + PPC_Q3_TRIMESH_NUM_POINTS_OFFSET, 3)
            .unwrap();
        loaded
            .memory
            .write_u32_be(trimesh_data + PPC_Q3_TRIMESH_POINTS_OFFSET, points_ptr)
            .unwrap();
        ppc_write_q3_vector3d(&mut loaded.memory, points_ptr, (-0.8, -0.8, 0.0)).unwrap();
        ppc_write_q3_vector3d(&mut loaded.memory, points_ptr + 12, (-0.8, 0.8, 0.0)).unwrap();
        ppc_write_q3_vector3d(&mut loaded.memory, points_ptr + 24, (0.8, -0.8, 0.0)).unwrap();
        loaded.memory.write_u32_be(triangles_ptr, 0).unwrap();
        loaded.memory.write_u32_be(triangles_ptr + 4, 1).unwrap();
        loaded.memory.write_u32_be(triangles_ptr + 8, 2).unwrap();

        loaded.q3_views.push(PpcQ3ViewStateRecord::new(view));
        loaded.q3_submissions.push(PpcQ3SubmissionRecord {
            view,
            kind: PpcQ3SubmissionKind::TriMesh,
            primary: trimesh_data,
            secondary: 0,
        });
        loaded
            .q3_submission_transforms
            .push(PpcQ3SubmissionTransformRecord {
                view,
                kind: PpcQ3SubmissionKind::TriMesh,
                primary: trimesh_data,
                secondary: 0,
                local_to_world: ppc_q3_matrix4x4_identity(),
            });
        loaded
            .q3_submission_materials
            .push(PpcQ3SubmissionMaterialRecord {
                view,
                kind: PpcQ3SubmissionKind::TriMesh,
                primary: trimesh_data,
                secondary: 0,
                shader: 0,
                illumination_type: PPC_Q3_ILLUMINATION_TYPE_PHONG,
                styles: vec![PpcQ3StyleRecord {
                    style,
                    kind: PpcQ3StyleKind::Fill,
                    value: fill_style,
                }],
                fog_style: None,
                attributes: vec![diffuse_attribute(attribute_set)],
                shader_uv_transform: None,
                shader_boundary: None,
                texture_shader: None,
                mipmap_texture: None,
            });
        loaded
            .q3_submission_lights
            .push(PpcQ3SubmissionLightRecord {
                view,
                kind: PpcQ3SubmissionKind::TriMesh,
                primary: trimesh_data,
                secondary: 0,
                light_group: 0,
                lights: Vec::new(),
            });

        let stats = loaded.render_q3_scene_commands_to_front_buffer();
        (loaded, front_base, stats)
    }

    let (mut edge_loaded, edge_front_base, edge_stats) =
        render_with_fill_style(PPC_Q3_FILL_STYLE_EDGES);
    assert_eq!(edge_stats.commands, 1);
    assert_eq!(edge_stats.triangles, 1);
    assert_eq!(
        edge_loaded
            .memory
            .read_u16_be(edge_front_base + 3 * 16 + 2),
        Some(0x03e0)
    );
    assert_eq!(
        edge_loaded
            .memory
            .read_u16_be(edge_front_base + 5 * 16 + 2 * 2),
        Some(0)
    );

    let (mut point_loaded, point_front_base, point_stats) =
        render_with_fill_style(PPC_Q3_FILL_STYLE_POINTS);
    assert_eq!(point_stats.commands, 1);
    assert_eq!(point_stats.triangles, 1);
    assert_eq!(point_stats.pixels, 3);
    assert_eq!(
        point_loaded
            .memory
            .read_u16_be(point_front_base + 6 * 16 + 2),
        Some(0x03e0)
    );
    assert_eq!(
        point_loaded
            .memory
            .read_u16_be(point_front_base + 3 * 16 + 2),
        Some(0)
    );
}

#[test]
fn q3_software_renderer_uses_trimesh_edge_attributes_for_edge_fill() {
    let view = PPC_Q3_OBJECT_BASE;
    let style = PPC_Q3_OBJECT_BASE + PPC_Q3_OBJECT_STRIDE;
    let trimesh_data = PPC_DATA_BASE + 0x1000;
    let points_ptr = PPC_DATA_BASE + 0x1080;
    let triangles_ptr = PPC_DATA_BASE + 0x1100;
    let edges_ptr = PPC_DATA_BASE + 0x1180;
    let edge_attrs_ptr = PPC_DATA_BASE + 0x1200;
    let edge_colors_ptr = PPC_DATA_BASE + 0x1280;
    let edge_alphas_ptr = PPC_DATA_BASE + 0x1300;
    let front_base = PPC_HEAP_BASE + 0x1000;
    let pef = synthetic_pef_with_library_import(b"QuickDraw\xaa 3D", b"Q3TriMesh_Submit");
    let mut loaded = load_pef_application(&pef).unwrap();
    loaded.memory.add_region(trimesh_data, vec![0; 0x600]);
    loaded.memory.add_region(front_base, vec![0; 8 * 8 * 2]);
    loaded.gworlds = vec![PpcGWorldRecord {
        ui_theme: crate::ui_theme::UiThemeId::ClassicSystem7,
        port: PPC_MAIN_GWORLD,
        pixmap_handle: 0,
        pixmap: 0,
        base_addr: front_base,
        gdevice: PPC_MAIN_GDEVICE,
        width: 8,
        height: 8,
        depth: 16,
        row_bytes: 16,
        pixels_locked: false,
        pixels_no_purge: false,
    }];
    loaded
        .current_gworld
        .with_mut(|current_gworld| *current_gworld = PPC_MAIN_GWORLD);

    loaded
        .memory
        .write_u32_be(trimesh_data + PPC_Q3_TRIMESH_NUM_TRIANGLES_OFFSET, 1)
        .unwrap();
    loaded
        .memory
        .write_u32_be(
            trimesh_data + PPC_Q3_TRIMESH_TRIANGLES_OFFSET,
            triangles_ptr,
        )
        .unwrap();
    loaded
        .memory
        .write_u32_be(trimesh_data + PPC_Q3_TRIMESH_NUM_EDGES_OFFSET, 3)
        .unwrap();
    loaded
        .memory
        .write_u32_be(trimesh_data + PPC_Q3_TRIMESH_EDGES_OFFSET, edges_ptr)
        .unwrap();
    loaded
        .memory
        .write_u32_be(
            trimesh_data + PPC_Q3_TRIMESH_NUM_EDGE_ATTRIBUTE_TYPES_OFFSET,
            2,
        )
        .unwrap();
    loaded
        .memory
        .write_u32_be(
            trimesh_data + PPC_Q3_TRIMESH_EDGE_ATTRIBUTE_TYPES_OFFSET,
            edge_attrs_ptr,
        )
        .unwrap();
    loaded
        .memory
        .write_u32_be(trimesh_data + PPC_Q3_TRIMESH_NUM_POINTS_OFFSET, 3)
        .unwrap();
    loaded
        .memory
        .write_u32_be(trimesh_data + PPC_Q3_TRIMESH_POINTS_OFFSET, points_ptr)
        .unwrap();

    for (index, point) in [(-0.8, -0.8, 0.0), (-0.8, 0.8, 0.0), (0.8, -0.8, 0.0)]
        .into_iter()
        .enumerate()
    {
        ppc_write_q3_vector3d(
            &mut loaded.memory,
            points_ptr + index as u32 * PPC_Q3_POINT3D_SIZE,
            point,
        )
        .unwrap();
    }
    for (offset, index) in [0u32, 1, 2].into_iter().enumerate() {
        loaded
            .memory
            .write_u32_be(triangles_ptr + offset as u32 * 4, index)
            .unwrap();
    }
    for (index, (a, b)) in [(0u32, 1u32), (1, 2), (2, 0)].into_iter().enumerate() {
        let edge_ptr = edges_ptr + index as u32 * PPC_Q3_TRIMESH_EDGE_DATA_SIZE;
        loaded.memory.write_u32_be(edge_ptr, a).unwrap();
        loaded.memory.write_u32_be(edge_ptr + 4, b).unwrap();
    }
    loaded
        .memory
        .write_u32_be(edge_attrs_ptr, PPC_Q3_ATTRIBUTE_TYPE_DIFFUSE_COLOR)
        .unwrap();
    loaded
        .memory
        .write_u32_be(edge_attrs_ptr + 4, edge_colors_ptr)
        .unwrap();
    loaded.memory.write_u32_be(edge_attrs_ptr + 8, 0).unwrap();
    loaded
        .memory
        .write_u32_be(
            edge_attrs_ptr + PPC_Q3_TRIMESH_ATTRIBUTE_DATA_SIZE,
            PPC_Q3_ATTRIBUTE_TYPE_TRANSPARENCY_COLOR,
        )
        .unwrap();
    loaded
        .memory
        .write_u32_be(
            edge_attrs_ptr + PPC_Q3_TRIMESH_ATTRIBUTE_DATA_SIZE + 4,
            edge_alphas_ptr,
        )
        .unwrap();
    loaded
        .memory
        .write_u32_be(edge_attrs_ptr + PPC_Q3_TRIMESH_ATTRIBUTE_DATA_SIZE + 8, 0)
        .unwrap();
    for (index, color) in [(1.0, 0.0, 0.0), (0.0, 0.0, 1.0), (0.0, 1.0, 0.0)]
        .into_iter()
        .enumerate()
    {
        ppc_write_q3_color_rgb(
            &mut loaded.memory,
            edge_colors_ptr + index as u32 * PPC_Q3_POINT3D_SIZE,
            color,
        )
        .unwrap();
    }
    for (index, alpha) in [(1.0, 1.0, 1.0), (0.5, 0.5, 0.5), (1.0, 1.0, 1.0)]
        .into_iter()
        .enumerate()
    {
        ppc_write_q3_color_rgb(
            &mut loaded.memory,
            edge_alphas_ptr + index as u32 * PPC_Q3_POINT3D_SIZE,
            alpha,
        )
        .unwrap();
    }

    loaded.q3_views.push(PpcQ3ViewStateRecord::new(view));
    loaded.q3_submissions.push(PpcQ3SubmissionRecord {
        view,
        kind: PpcQ3SubmissionKind::TriMesh,
        primary: trimesh_data,
        secondary: 0,
    });
    loaded
        .q3_submission_transforms
        .push(PpcQ3SubmissionTransformRecord {
            view,
            kind: PpcQ3SubmissionKind::TriMesh,
            primary: trimesh_data,
            secondary: 0,
            local_to_world: ppc_q3_matrix4x4_identity(),
        });
    loaded
        .q3_submission_materials
        .push(PpcQ3SubmissionMaterialRecord {
            view,
            kind: PpcQ3SubmissionKind::TriMesh,
            primary: trimesh_data,
            secondary: 0,
            shader: 0,
            illumination_type: PPC_Q3_ILLUMINATION_TYPE_PHONG,
            styles: vec![PpcQ3StyleRecord {
                style,
                kind: PpcQ3StyleKind::Fill,
                value: PPC_Q3_FILL_STYLE_EDGES,
            }],
            fog_style: None,
            attributes: Vec::new(),
            shader_uv_transform: None,
            shader_boundary: None,
            texture_shader: None,
            mipmap_texture: None,
        });
    loaded
        .q3_submission_lights
        .push(PpcQ3SubmissionLightRecord {
            view,
            kind: PpcQ3SubmissionKind::TriMesh,
            primary: trimesh_data,
            secondary: 0,
            light_group: 0,
            lights: Vec::new(),
        });

    let stats = loaded.render_q3_scene_commands_to_front_buffer();

    assert_eq!(stats.commands, 1);
    assert_eq!(stats.triangles, 1);
    assert!(stats.pixels > 0);
    assert_eq!(
        loaded.memory.read_u16_be(front_base + 3 * 16 + 2),
        Some(0x7c00)
    );
    assert_eq!(
        loaded.memory.read_u16_be(front_base + 3 * 16 + 3 * 2),
        Some(0x0010)
    );
    assert_eq!(
        loaded.memory.read_u16_be(front_base + 6 * 16 + 3 * 2),
        Some(0x03e0)
    );
}

#[test]
fn q3_software_renderer_uses_edge_specular_attributes_for_edge_fill() {
    fn color_attribute(
        attribute_set: u32,
        attribute_type: u32,
        rgb: [f32; 3],
    ) -> PpcQ3AttributeRecord {
        let mut data = Vec::new();
        for value in rgb {
            data.extend_from_slice(&value.to_bits().to_be_bytes());
        }
        PpcQ3AttributeRecord {
            attribute_set,
            attribute_type,
            data,
        }
    }

    let view = PPC_Q3_OBJECT_BASE;
    let attribute_set = PPC_Q3_OBJECT_BASE + PPC_Q3_OBJECT_STRIDE;
    let style = PPC_Q3_OBJECT_BASE + PPC_Q3_OBJECT_STRIDE * 2;
    let directional_light = PPC_Q3_OBJECT_BASE + PPC_Q3_OBJECT_STRIDE * 3;
    let trimesh_data = PPC_DATA_BASE + 0x1000;
    let points_ptr = PPC_DATA_BASE + 0x1080;
    let triangles_ptr = PPC_DATA_BASE + 0x1100;
    let edges_ptr = PPC_DATA_BASE + 0x1180;
    let edge_attrs_ptr = PPC_DATA_BASE + 0x1200;
    let edge_specular_colors_ptr = PPC_DATA_BASE + 0x1280;
    let edge_specular_controls_ptr = PPC_DATA_BASE + 0x1300;
    let vertex_attrs_ptr = PPC_DATA_BASE + 0x1380;
    let normals_ptr = PPC_DATA_BASE + 0x1400;
    let front_base = PPC_HEAP_BASE + 0x1000;
    let pef = synthetic_pef_with_library_import(b"QuickDraw\xaa 3D", b"Q3TriMesh_Submit");
    let mut loaded = load_pef_application(&pef).unwrap();
    loaded.memory.add_region(trimesh_data, vec![0; 0x600]);
    loaded.memory.add_region(front_base, vec![0; 8 * 8 * 2]);
    loaded.gworlds = vec![PpcGWorldRecord {
        ui_theme: crate::ui_theme::UiThemeId::ClassicSystem7,
        port: PPC_MAIN_GWORLD,
        pixmap_handle: 0,
        pixmap: 0,
        base_addr: front_base,
        gdevice: PPC_MAIN_GDEVICE,
        width: 8,
        height: 8,
        depth: 16,
        row_bytes: 16,
        pixels_locked: false,
        pixels_no_purge: false,
    }];
    loaded
        .current_gworld
        .with_mut(|current_gworld| *current_gworld = PPC_MAIN_GWORLD);

    loaded
        .memory
        .write_u32_be(trimesh_data + PPC_Q3_TRIMESH_NUM_TRIANGLES_OFFSET, 1)
        .unwrap();
    loaded
        .memory
        .write_u32_be(
            trimesh_data + PPC_Q3_TRIMESH_TRIANGLES_OFFSET,
            triangles_ptr,
        )
        .unwrap();
    loaded
        .memory
        .write_u32_be(trimesh_data + PPC_Q3_TRIMESH_NUM_EDGES_OFFSET, 3)
        .unwrap();
    loaded
        .memory
        .write_u32_be(trimesh_data + PPC_Q3_TRIMESH_EDGES_OFFSET, edges_ptr)
        .unwrap();
    loaded
        .memory
        .write_u32_be(
            trimesh_data + PPC_Q3_TRIMESH_NUM_EDGE_ATTRIBUTE_TYPES_OFFSET,
            2,
        )
        .unwrap();
    loaded
        .memory
        .write_u32_be(
            trimesh_data + PPC_Q3_TRIMESH_EDGE_ATTRIBUTE_TYPES_OFFSET,
            edge_attrs_ptr,
        )
        .unwrap();
    loaded
        .memory
        .write_u32_be(
            trimesh_data + PPC_Q3_TRIMESH_NUM_VERTEX_ATTRIBUTE_TYPES_OFFSET,
            1,
        )
        .unwrap();
    loaded
        .memory
        .write_u32_be(
            trimesh_data + PPC_Q3_TRIMESH_VERTEX_ATTRIBUTE_TYPES_OFFSET,
            vertex_attrs_ptr,
        )
        .unwrap();
    loaded
        .memory
        .write_u32_be(trimesh_data + PPC_Q3_TRIMESH_NUM_POINTS_OFFSET, 3)
        .unwrap();
    loaded
        .memory
        .write_u32_be(trimesh_data + PPC_Q3_TRIMESH_POINTS_OFFSET, points_ptr)
        .unwrap();
    for (index, point) in [(-0.8, -0.8, 0.0), (-0.8, 0.8, 0.0), (0.8, -0.8, 0.0)]
        .into_iter()
        .enumerate()
    {
        ppc_write_q3_vector3d(
            &mut loaded.memory,
            points_ptr + index as u32 * PPC_Q3_POINT3D_SIZE,
            point,
        )
        .unwrap();
        ppc_write_q3_vector3d(
            &mut loaded.memory,
            normals_ptr + index as u32 * PPC_Q3_POINT3D_SIZE,
            (0.0, 0.0, 1.0),
        )
        .unwrap();
    }
    for (offset, index) in [0u32, 1, 2].into_iter().enumerate() {
        loaded
            .memory
            .write_u32_be(triangles_ptr + offset as u32 * 4, index)
            .unwrap();
    }
    for (index, (a, b)) in [(0u32, 1u32), (1, 2), (2, 0)].into_iter().enumerate() {
        let edge_ptr = edges_ptr + index as u32 * PPC_Q3_TRIMESH_EDGE_DATA_SIZE;
        loaded.memory.write_u32_be(edge_ptr, a).unwrap();
        loaded.memory.write_u32_be(edge_ptr + 4, b).unwrap();
    }
    for (index, (attribute_type, data_ptr)) in [
        (
            PPC_Q3_ATTRIBUTE_TYPE_SPECULAR_COLOR,
            edge_specular_colors_ptr,
        ),
        (
            PPC_Q3_ATTRIBUTE_TYPE_SPECULAR_CONTROL,
            edge_specular_controls_ptr,
        ),
    ]
    .into_iter()
    .enumerate()
    {
        let entry_ptr = edge_attrs_ptr + index as u32 * PPC_Q3_TRIMESH_ATTRIBUTE_DATA_SIZE;
        loaded
            .memory
            .write_u32_be(entry_ptr, attribute_type)
            .unwrap();
        loaded.memory.write_u32_be(entry_ptr + 4, data_ptr).unwrap();
        loaded.memory.write_u32_be(entry_ptr + 8, 0).unwrap();
    }
    loaded
        .memory
        .write_u32_be(vertex_attrs_ptr, PPC_Q3_ATTRIBUTE_TYPE_NORMAL)
        .unwrap();
    loaded
        .memory
        .write_u32_be(vertex_attrs_ptr + 4, normals_ptr)
        .unwrap();
    loaded.memory.write_u32_be(vertex_attrs_ptr + 8, 0).unwrap();
    for index in 0..3 {
        ppc_write_q3_color_rgb(
            &mut loaded.memory,
            edge_specular_colors_ptr + index * PPC_Q3_POINT3D_SIZE,
            (0.0, 0.0, 1.0),
        )
        .unwrap();
        ppc_write_f32_be(
            &mut loaded.memory,
            edge_specular_controls_ptr + index * 4,
            1.0,
        )
        .unwrap();
    }

    let specular_control = 1.0f32.to_bits().to_be_bytes().to_vec();
    loaded.q3_views.push(PpcQ3ViewStateRecord::new(view));
    loaded.q3_submissions.push(PpcQ3SubmissionRecord {
        view,
        kind: PpcQ3SubmissionKind::TriMesh,
        primary: trimesh_data,
        secondary: 0,
    });
    loaded
        .q3_submission_transforms
        .push(PpcQ3SubmissionTransformRecord {
            view,
            kind: PpcQ3SubmissionKind::TriMesh,
            primary: trimesh_data,
            secondary: 0,
            local_to_world: ppc_q3_matrix4x4_identity(),
        });
    loaded
        .q3_submission_materials
        .push(PpcQ3SubmissionMaterialRecord {
            view,
            kind: PpcQ3SubmissionKind::TriMesh,
            primary: trimesh_data,
            secondary: 0,
            shader: 0,
            illumination_type: PPC_Q3_ILLUMINATION_TYPE_PHONG,
            styles: vec![PpcQ3StyleRecord {
                style,
                kind: PpcQ3StyleKind::Fill,
                value: PPC_Q3_FILL_STYLE_EDGES,
            }],
            fog_style: None,
            attributes: vec![
                color_attribute(
                    attribute_set,
                    PPC_Q3_ATTRIBUTE_TYPE_DIFFUSE_COLOR,
                    [0.0, 0.0, 0.0],
                ),
                color_attribute(
                    attribute_set,
                    PPC_Q3_ATTRIBUTE_TYPE_SPECULAR_COLOR,
                    [1.0, 0.0, 0.0],
                ),
                PpcQ3AttributeRecord {
                    attribute_set,
                    attribute_type: PPC_Q3_ATTRIBUTE_TYPE_SPECULAR_CONTROL,
                    data: specular_control,
                },
            ],
            shader_uv_transform: None,
            shader_boundary: None,
            texture_shader: None,
            mipmap_texture: None,
        });
    loaded
        .q3_submission_lights
        .push(PpcQ3SubmissionLightRecord {
            view,
            kind: PpcQ3SubmissionKind::TriMesh,
            primary: trimesh_data,
            secondary: 0,
            light_group: 0,
            lights: vec![PpcQ3LightRecord {
                light: directional_light,
                light_type: PPC_Q3_LIGHT_TYPE_DIRECTIONAL,
                data: PpcQ3LightData {
                    is_on: 1,
                    brightness: 1.0,
                    color: (1.0, 1.0, 1.0),
                },
                kind: PpcQ3LightKind::Directional {
                    casts_shadows: 0,
                    direction: (0.0, 0.0, -1.0),
                },
            }],
        });

    let stats = loaded.render_q3_scene_commands_to_front_buffer();

    assert_eq!(stats.commands, 1);
    assert_eq!(stats.triangles, 1);
    assert!(stats.pixels > 0);
    assert_eq!(
        loaded.memory.read_u16_be(front_base + 3 * 16 + 2),
        Some(0x001f)
    );
}

#[test]
fn q3_software_renderer_uses_edge_normals_for_edge_fill_directional_lights() {
    fn diffuse_attribute(attribute_set: u32) -> PpcQ3AttributeRecord {
        let mut data = Vec::new();
        for value in [1.0f32, 1.0, 1.0] {
            data.extend_from_slice(&value.to_bits().to_be_bytes());
        }
        PpcQ3AttributeRecord {
            attribute_set,
            attribute_type: PPC_Q3_ATTRIBUTE_TYPE_DIFFUSE_COLOR,
            data,
        }
    }

    let view = PPC_Q3_OBJECT_BASE;
    let attribute_set = PPC_Q3_OBJECT_BASE + PPC_Q3_OBJECT_STRIDE;
    let style = PPC_Q3_OBJECT_BASE + PPC_Q3_OBJECT_STRIDE * 2;
    let directional_light = PPC_Q3_OBJECT_BASE + PPC_Q3_OBJECT_STRIDE * 3;
    let trimesh_data = PPC_DATA_BASE + 0x1000;
    let points_ptr = PPC_DATA_BASE + 0x1080;
    let triangles_ptr = PPC_DATA_BASE + 0x1100;
    let edges_ptr = PPC_DATA_BASE + 0x1180;
    let edge_attrs_ptr = PPC_DATA_BASE + 0x1200;
    let edge_normals_ptr = PPC_DATA_BASE + 0x1280;
    let front_base = PPC_HEAP_BASE + 0x1000;
    let pef = synthetic_pef_with_library_import(b"QuickDraw\xaa 3D", b"Q3TriMesh_Submit");
    let mut loaded = load_pef_application(&pef).unwrap();
    loaded.memory.add_region(trimesh_data, vec![0; 0x400]);
    loaded.memory.add_region(front_base, vec![0; 8 * 8 * 2]);
    loaded.gworlds = vec![PpcGWorldRecord {
        ui_theme: crate::ui_theme::UiThemeId::ClassicSystem7,
        port: PPC_MAIN_GWORLD,
        pixmap_handle: 0,
        pixmap: 0,
        base_addr: front_base,
        gdevice: PPC_MAIN_GDEVICE,
        width: 8,
        height: 8,
        depth: 16,
        row_bytes: 16,
        pixels_locked: false,
        pixels_no_purge: false,
    }];
    loaded
        .current_gworld
        .with_mut(|current_gworld| *current_gworld = PPC_MAIN_GWORLD);

    loaded
        .memory
        .write_u32_be(trimesh_data + PPC_Q3_TRIMESH_NUM_TRIANGLES_OFFSET, 1)
        .unwrap();
    loaded
        .memory
        .write_u32_be(
            trimesh_data + PPC_Q3_TRIMESH_TRIANGLES_OFFSET,
            triangles_ptr,
        )
        .unwrap();
    loaded
        .memory
        .write_u32_be(trimesh_data + PPC_Q3_TRIMESH_NUM_EDGES_OFFSET, 3)
        .unwrap();
    loaded
        .memory
        .write_u32_be(trimesh_data + PPC_Q3_TRIMESH_EDGES_OFFSET, edges_ptr)
        .unwrap();
    loaded
        .memory
        .write_u32_be(
            trimesh_data + PPC_Q3_TRIMESH_NUM_EDGE_ATTRIBUTE_TYPES_OFFSET,
            1,
        )
        .unwrap();
    loaded
        .memory
        .write_u32_be(
            trimesh_data + PPC_Q3_TRIMESH_EDGE_ATTRIBUTE_TYPES_OFFSET,
            edge_attrs_ptr,
        )
        .unwrap();
    loaded
        .memory
        .write_u32_be(trimesh_data + PPC_Q3_TRIMESH_NUM_POINTS_OFFSET, 3)
        .unwrap();
    loaded
        .memory
        .write_u32_be(trimesh_data + PPC_Q3_TRIMESH_POINTS_OFFSET, points_ptr)
        .unwrap();

    for (index, point) in [(-0.8, -0.8, 0.0), (-0.8, 0.8, 0.0), (0.8, -0.8, 0.0)]
        .into_iter()
        .enumerate()
    {
        ppc_write_q3_vector3d(
            &mut loaded.memory,
            points_ptr + index as u32 * PPC_Q3_POINT3D_SIZE,
            point,
        )
        .unwrap();
    }
    for (offset, index) in [0u32, 1, 2].into_iter().enumerate() {
        loaded
            .memory
            .write_u32_be(triangles_ptr + offset as u32 * 4, index)
            .unwrap();
    }
    for (index, (a, b)) in [(0u32, 1u32), (1, 2), (2, 0)].into_iter().enumerate() {
        let edge_ptr = edges_ptr + index as u32 * PPC_Q3_TRIMESH_EDGE_DATA_SIZE;
        loaded.memory.write_u32_be(edge_ptr, a).unwrap();
        loaded.memory.write_u32_be(edge_ptr + 4, b).unwrap();
    }
    loaded
        .memory
        .write_u32_be(edge_attrs_ptr, PPC_Q3_ATTRIBUTE_TYPE_NORMAL)
        .unwrap();
    loaded
        .memory
        .write_u32_be(edge_attrs_ptr + 4, edge_normals_ptr)
        .unwrap();
    loaded.memory.write_u32_be(edge_attrs_ptr + 8, 0).unwrap();
    for (index, normal) in [(0.0, 0.0, 1.0), (1.0, 0.0, 0.0), (1.0, 0.0, 0.0)]
        .into_iter()
        .enumerate()
    {
        ppc_write_q3_vector3d(
            &mut loaded.memory,
            edge_normals_ptr + index as u32 * PPC_Q3_POINT3D_SIZE,
            normal,
        )
        .unwrap();
    }

    loaded.q3_views.push(PpcQ3ViewStateRecord::new(view));
    loaded.q3_submissions.push(PpcQ3SubmissionRecord {
        view,
        kind: PpcQ3SubmissionKind::TriMesh,
        primary: trimesh_data,
        secondary: 0,
    });
    loaded
        .q3_submission_transforms
        .push(PpcQ3SubmissionTransformRecord {
            view,
            kind: PpcQ3SubmissionKind::TriMesh,
            primary: trimesh_data,
            secondary: 0,
            local_to_world: ppc_q3_matrix4x4_identity(),
        });
    loaded
        .q3_submission_materials
        .push(PpcQ3SubmissionMaterialRecord {
            view,
            kind: PpcQ3SubmissionKind::TriMesh,
            primary: trimesh_data,
            secondary: 0,
            shader: 0,
            illumination_type: PPC_Q3_ILLUMINATION_TYPE_LAMBERT,
            styles: vec![PpcQ3StyleRecord {
                style,
                kind: PpcQ3StyleKind::Fill,
                value: PPC_Q3_FILL_STYLE_EDGES,
            }],
            fog_style: None,
            attributes: vec![diffuse_attribute(attribute_set)],
            shader_uv_transform: None,
            shader_boundary: None,
            texture_shader: None,
            mipmap_texture: None,
        });
    loaded
        .q3_submission_lights
        .push(PpcQ3SubmissionLightRecord {
            view,
            kind: PpcQ3SubmissionKind::TriMesh,
            primary: trimesh_data,
            secondary: 0,
            light_group: 0,
            lights: vec![PpcQ3LightRecord {
                light: directional_light,
                light_type: PPC_Q3_LIGHT_TYPE_DIRECTIONAL,
                data: PpcQ3LightData {
                    is_on: 1,
                    brightness: 1.0,
                    color: (0.0, 1.0, 0.0),
                },
                kind: PpcQ3LightKind::Directional {
                    casts_shadows: 0,
                    direction: (0.0, 0.0, -1.0),
                },
            }],
        });

    let stats = loaded.render_q3_scene_commands_to_front_buffer();

    assert_eq!(stats.commands, 1);
    assert_eq!(stats.triangles, 1);
    assert!(stats.pixels > 0);
    assert_eq!(
        loaded.memory.read_u16_be(front_base + 3 * 16 + 2),
        Some(0x03e0)
    );
    assert_eq!(
        loaded.memory.read_u16_be(front_base + 6 * 16 + 3 * 2),
        Some(0)
    );
}

#[test]
fn q3_software_renderer_uses_edge_ambient_coefficients_for_edge_fill() {
    fn diffuse_attribute(attribute_set: u32) -> PpcQ3AttributeRecord {
        let mut data = Vec::new();
        for value in [1.0f32, 1.0, 1.0] {
            data.extend_from_slice(&value.to_bits().to_be_bytes());
        }
        PpcQ3AttributeRecord {
            attribute_set,
            attribute_type: PPC_Q3_ATTRIBUTE_TYPE_DIFFUSE_COLOR,
            data,
        }
    }

    let view = PPC_Q3_OBJECT_BASE;
    let attribute_set = PPC_Q3_OBJECT_BASE + PPC_Q3_OBJECT_STRIDE;
    let style = PPC_Q3_OBJECT_BASE + PPC_Q3_OBJECT_STRIDE * 2;
    let ambient_light = PPC_Q3_OBJECT_BASE + PPC_Q3_OBJECT_STRIDE * 3;
    let trimesh_data = PPC_DATA_BASE + 0x1000;
    let points_ptr = PPC_DATA_BASE + 0x1080;
    let triangles_ptr = PPC_DATA_BASE + 0x1100;
    let edges_ptr = PPC_DATA_BASE + 0x1180;
    let edge_attrs_ptr = PPC_DATA_BASE + 0x1200;
    let edge_ambient_coefficients_ptr = PPC_DATA_BASE + 0x1280;
    let front_base = PPC_HEAP_BASE + 0x1000;
    let pef = synthetic_pef_with_library_import(b"QuickDraw\xaa 3D", b"Q3TriMesh_Submit");
    let mut loaded = load_pef_application(&pef).unwrap();
    loaded.memory.add_region(trimesh_data, vec![0; 0x400]);
    loaded.memory.add_region(front_base, vec![0; 8 * 8 * 2]);
    loaded.gworlds = vec![PpcGWorldRecord {
        ui_theme: crate::ui_theme::UiThemeId::ClassicSystem7,
        port: PPC_MAIN_GWORLD,
        pixmap_handle: 0,
        pixmap: 0,
        base_addr: front_base,
        gdevice: PPC_MAIN_GDEVICE,
        width: 8,
        height: 8,
        depth: 16,
        row_bytes: 16,
        pixels_locked: false,
        pixels_no_purge: false,
    }];
    loaded
        .current_gworld
        .with_mut(|current_gworld| *current_gworld = PPC_MAIN_GWORLD);

    loaded
        .memory
        .write_u32_be(trimesh_data + PPC_Q3_TRIMESH_NUM_TRIANGLES_OFFSET, 1)
        .unwrap();
    loaded
        .memory
        .write_u32_be(
            trimesh_data + PPC_Q3_TRIMESH_TRIANGLES_OFFSET,
            triangles_ptr,
        )
        .unwrap();
    loaded
        .memory
        .write_u32_be(trimesh_data + PPC_Q3_TRIMESH_NUM_EDGES_OFFSET, 3)
        .unwrap();
    loaded
        .memory
        .write_u32_be(trimesh_data + PPC_Q3_TRIMESH_EDGES_OFFSET, edges_ptr)
        .unwrap();
    loaded
        .memory
        .write_u32_be(
            trimesh_data + PPC_Q3_TRIMESH_NUM_EDGE_ATTRIBUTE_TYPES_OFFSET,
            1,
        )
        .unwrap();
    loaded
        .memory
        .write_u32_be(
            trimesh_data + PPC_Q3_TRIMESH_EDGE_ATTRIBUTE_TYPES_OFFSET,
            edge_attrs_ptr,
        )
        .unwrap();
    loaded
        .memory
        .write_u32_be(edge_attrs_ptr, PPC_Q3_ATTRIBUTE_TYPE_AMBIENT_COEFFICIENT)
        .unwrap();
    loaded
        .memory
        .write_u32_be(edge_attrs_ptr + 4, edge_ambient_coefficients_ptr)
        .unwrap();
    loaded.memory.write_u32_be(edge_attrs_ptr + 8, 0).unwrap();
    loaded
        .memory
        .write_u32_be(trimesh_data + PPC_Q3_TRIMESH_NUM_POINTS_OFFSET, 3)
        .unwrap();
    loaded
        .memory
        .write_u32_be(trimesh_data + PPC_Q3_TRIMESH_POINTS_OFFSET, points_ptr)
        .unwrap();

    for (index, point) in [(-0.8, -0.8, 0.0), (-0.8, 0.8, 0.0), (0.8, -0.8, 0.0)]
        .into_iter()
        .enumerate()
    {
        ppc_write_q3_vector3d(
            &mut loaded.memory,
            points_ptr + index as u32 * PPC_Q3_POINT3D_SIZE,
            point,
        )
        .unwrap();
    }
    for (offset, index) in [0u32, 1, 2].into_iter().enumerate() {
        loaded
            .memory
            .write_u32_be(triangles_ptr + offset as u32 * 4, index)
            .unwrap();
    }
    for (index, (a, b)) in [(0u32, 1u32), (1, 2), (2, 0)].into_iter().enumerate() {
        let edge_ptr = edges_ptr + index as u32 * PPC_Q3_TRIMESH_EDGE_DATA_SIZE;
        loaded.memory.write_u32_be(edge_ptr, a).unwrap();
        loaded.memory.write_u32_be(edge_ptr + 4, b).unwrap();
    }
    for (index, coefficient) in [0.25f32, 0.0, 0.0].into_iter().enumerate() {
        ppc_write_f32_be(
            &mut loaded.memory,
            edge_ambient_coefficients_ptr + index as u32 * 4,
            coefficient,
        )
        .unwrap();
    }

    let ambient_zero = 0.0f32.to_bits().to_be_bytes().to_vec();
    loaded.q3_views.push(PpcQ3ViewStateRecord::new(view));
    loaded.q3_submissions.push(PpcQ3SubmissionRecord {
        view,
        kind: PpcQ3SubmissionKind::TriMesh,
        primary: trimesh_data,
        secondary: 0,
    });
    loaded
        .q3_submission_transforms
        .push(PpcQ3SubmissionTransformRecord {
            view,
            kind: PpcQ3SubmissionKind::TriMesh,
            primary: trimesh_data,
            secondary: 0,
            local_to_world: ppc_q3_matrix4x4_identity(),
        });
    loaded
        .q3_submission_materials
        .push(PpcQ3SubmissionMaterialRecord {
            view,
            kind: PpcQ3SubmissionKind::TriMesh,
            primary: trimesh_data,
            secondary: 0,
            shader: 0,
            illumination_type: PPC_Q3_ILLUMINATION_TYPE_PHONG,
            styles: vec![PpcQ3StyleRecord {
                style,
                kind: PpcQ3StyleKind::Fill,
                value: PPC_Q3_FILL_STYLE_EDGES,
            }],
            fog_style: None,
            attributes: vec![
                diffuse_attribute(attribute_set),
                PpcQ3AttributeRecord {
                    attribute_set,
                    attribute_type: PPC_Q3_ATTRIBUTE_TYPE_AMBIENT_COEFFICIENT,
                    data: ambient_zero,
                },
            ],
            shader_uv_transform: None,
            shader_boundary: None,
            texture_shader: None,
            mipmap_texture: None,
        });
    loaded
        .q3_submission_lights
        .push(PpcQ3SubmissionLightRecord {
            view,
            kind: PpcQ3SubmissionKind::TriMesh,
            primary: trimesh_data,
            secondary: 0,
            light_group: 0,
            lights: vec![PpcQ3LightRecord {
                light: ambient_light,
                light_type: PPC_Q3_LIGHT_TYPE_AMBIENT,
                data: PpcQ3LightData {
                    is_on: 1,
                    brightness: 1.0,
                    color: (1.0, 0.0, 0.0),
                },
                kind: PpcQ3LightKind::Ambient,
            }],
        });

    let stats = loaded.render_q3_scene_commands_to_front_buffer();

    assert_eq!(stats.commands, 1);
    assert_eq!(stats.triangles, 1);
    assert!(stats.pixels > 0);
    assert_eq!(
        loaded.memory.read_u16_be(front_base + 3 * 16 + 2),
        Some(0x2000)
    );
    assert_eq!(
        loaded.memory.read_u16_be(front_base + 6 * 16 + 3 * 2),
        Some(0)
    );
}

#[test]
fn q3_software_renderer_respects_backfacing_remove_style() {
    fn diffuse_attribute(attribute_set: u32) -> PpcQ3AttributeRecord {
        let mut data = Vec::new();
        for value in [0.0f32, 1.0, 0.0] {
            data.extend_from_slice(&value.to_bits().to_be_bytes());
        }
        PpcQ3AttributeRecord {
            attribute_set,
            attribute_type: PPC_Q3_ATTRIBUTE_TYPE_DIFFUSE_COLOR,
            data,
        }
    }

    fn render_with_backfacing_style(
        backfacing_style: u32,
        orientation_style: Option<u32>,
    ) -> (PpcLoadedApp, u32, PpcQ3SoftwareRenderStats) {
        let view = PPC_Q3_OBJECT_BASE;
        let attribute_set = PPC_Q3_OBJECT_BASE + PPC_Q3_OBJECT_STRIDE;
        let style = PPC_Q3_OBJECT_BASE + PPC_Q3_OBJECT_STRIDE * 2;
        let trimesh_data = PPC_DATA_BASE + 0x1000;
        let points_ptr = PPC_DATA_BASE + 0x1080;
        let triangles_ptr = PPC_DATA_BASE + 0x1200;
        let front_base = PPC_HEAP_BASE + 0x1000;
        let pef = synthetic_pef_with_library_import(b"QuickDraw\xaa 3D", b"Q3TriMesh_Submit");
        let mut loaded = load_pef_application(&pef).unwrap();
        loaded.memory.add_region(trimesh_data, vec![0; 0x400]);
        loaded.memory.add_region(front_base, vec![0; 8 * 8 * 2]);
        loaded.gworlds = vec![PpcGWorldRecord {
            ui_theme: crate::ui_theme::UiThemeId::ClassicSystem7,
            port: PPC_MAIN_GWORLD,
            pixmap_handle: 0,
            pixmap: 0,
            base_addr: front_base,
            gdevice: PPC_MAIN_GDEVICE,
            width: 8,
            height: 8,
            depth: 16,
            row_bytes: 16,
            pixels_locked: false,
            pixels_no_purge: false,
        }];
        loaded
            .current_gworld
            .with_mut(|current_gworld| *current_gworld = PPC_MAIN_GWORLD);

        loaded
            .memory
            .write_u32_be(trimesh_data + PPC_Q3_TRIMESH_NUM_TRIANGLES_OFFSET, 2)
            .unwrap();
        loaded
            .memory
            .write_u32_be(
                trimesh_data + PPC_Q3_TRIMESH_TRIANGLES_OFFSET,
                triangles_ptr,
            )
            .unwrap();
        loaded
            .memory
            .write_u32_be(trimesh_data + PPC_Q3_TRIMESH_NUM_POINTS_OFFSET, 6)
            .unwrap();
        loaded
            .memory
            .write_u32_be(trimesh_data + PPC_Q3_TRIMESH_POINTS_OFFSET, points_ptr)
            .unwrap();

        for (index, point) in [
            (-0.9, -0.8, 0.0),
            (-0.1, -0.8, 0.0),
            (-0.9, 0.8, 0.0),
            (0.1, -0.8, 0.0),
            (0.1, 0.8, 0.0),
            (0.9, -0.8, 0.0),
        ]
        .into_iter()
        .enumerate()
        {
            ppc_write_q3_vector3d(
                &mut loaded.memory,
                points_ptr + index as u32 * PPC_Q3_POINT3D_SIZE,
                point,
            )
            .unwrap();
        }
        for (offset, index) in [0u32, 1, 2, 3, 4, 5].into_iter().enumerate() {
            loaded
                .memory
                .write_u32_be(triangles_ptr + offset as u32 * 4, index)
                .unwrap();
        }

        loaded.q3_views.push(PpcQ3ViewStateRecord::new(view));
        loaded.q3_submissions.push(PpcQ3SubmissionRecord {
            view,
            kind: PpcQ3SubmissionKind::TriMesh,
            primary: trimesh_data,
            secondary: 0,
        });
        loaded
            .q3_submission_transforms
            .push(PpcQ3SubmissionTransformRecord {
                view,
                kind: PpcQ3SubmissionKind::TriMesh,
                primary: trimesh_data,
                secondary: 0,
                local_to_world: ppc_q3_matrix4x4_identity(),
            });
        let mut styles = vec![PpcQ3StyleRecord {
            style,
            kind: PpcQ3StyleKind::Backfacing,
            value: backfacing_style,
        }];
        if let Some(orientation_style) = orientation_style {
            styles.push(PpcQ3StyleRecord {
                style: style + PPC_Q3_OBJECT_STRIDE,
                kind: PpcQ3StyleKind::Orientation,
                value: orientation_style,
            });
        }
        loaded
            .q3_submission_materials
            .push(PpcQ3SubmissionMaterialRecord {
                view,
                kind: PpcQ3SubmissionKind::TriMesh,
                primary: trimesh_data,
                secondary: 0,
                shader: 0,
                illumination_type: PPC_Q3_ILLUMINATION_TYPE_PHONG,
                styles,
                fog_style: None,
                attributes: vec![diffuse_attribute(attribute_set)],
                shader_uv_transform: None,
                shader_boundary: None,
                texture_shader: None,
                mipmap_texture: None,
            });
        loaded
            .q3_submission_lights
            .push(PpcQ3SubmissionLightRecord {
                view,
                kind: PpcQ3SubmissionKind::TriMesh,
                primary: trimesh_data,
                secondary: 0,
                light_group: 0,
                lights: Vec::new(),
            });

        let stats = loaded.render_q3_scene_commands_to_front_buffer();
        (loaded, front_base, stats)
    }

    let (mut both_loaded, both_front_base, both_stats) =
        render_with_backfacing_style(PPC_Q3_BACKFACING_STYLE_BOTH, None);
    assert_eq!(both_stats.triangles, 2);
    assert_eq!(
        both_loaded
            .memory
            .read_u16_be(both_front_base + 5 * 16 + 2),
        Some(0x03e0)
    );
    assert_eq!(
        both_loaded
            .memory
            .read_u16_be(both_front_base + 5 * 16 + 5 * 2),
        Some(0x03e0)
    );

    let (mut remove_loaded, remove_front_base, remove_stats) =
        render_with_backfacing_style(PPC_Q3_BACKFACING_STYLE_REMOVE, None);
    assert_eq!(remove_stats.triangles, 1);
    assert_eq!(
        remove_loaded
            .memory
            .read_u16_be(remove_front_base + 5 * 16 + 2),
        Some(0)
    );
    assert_eq!(
        remove_loaded
            .memory
            .read_u16_be(remove_front_base + 5 * 16 + 5 * 2),
        Some(0x03e0)
    );

    let (mut clockwise_loaded, clockwise_front_base, clockwise_stats) =
        render_with_backfacing_style(
            PPC_Q3_BACKFACING_STYLE_REMOVE,
            Some(PPC_Q3_ORIENTATION_STYLE_CLOCKWISE),
        );
    assert_eq!(clockwise_stats.triangles, 1);
    assert_eq!(
        clockwise_loaded
            .memory
            .read_u16_be(clockwise_front_base + 5 * 16 + 2),
        Some(0x03e0)
    );
    assert_eq!(
        clockwise_loaded
            .memory
            .read_u16_be(clockwise_front_base + 5 * 16 + 5 * 2),
        Some(0)
    );
}

#[test]
fn q3_software_renderer_flips_backfacing_normals_for_flip_style() {
    fn diffuse_attribute(attribute_set: u32) -> PpcQ3AttributeRecord {
        let mut data = Vec::new();
        for value in [1.0f32, 1.0, 1.0] {
            data.extend_from_slice(&value.to_bits().to_be_bytes());
        }
        PpcQ3AttributeRecord {
            attribute_set,
            attribute_type: PPC_Q3_ATTRIBUTE_TYPE_DIFFUSE_COLOR,
            data,
        }
    }

    fn render_with_backfacing_style(
        backfacing_style: u32,
    ) -> (PpcLoadedApp, u32, PpcQ3SoftwareRenderStats) {
        let view = PPC_Q3_OBJECT_BASE;
        let attribute_set = PPC_Q3_OBJECT_BASE + PPC_Q3_OBJECT_STRIDE;
        let style = PPC_Q3_OBJECT_BASE + PPC_Q3_OBJECT_STRIDE * 2;
        let directional_light = PPC_Q3_OBJECT_BASE + PPC_Q3_OBJECT_STRIDE * 3;
        let trimesh_data = PPC_DATA_BASE + 0x1000;
        let points_ptr = PPC_DATA_BASE + 0x1080;
        let triangles_ptr = PPC_DATA_BASE + 0x1200;
        let vertex_attrs_ptr = PPC_DATA_BASE + 0x1280;
        let normals_ptr = PPC_DATA_BASE + 0x12c0;
        let front_base = PPC_HEAP_BASE + 0x1000;
        let pef = synthetic_pef_with_library_import(b"QuickDraw\xaa 3D", b"Q3TriMesh_Submit");
        let mut loaded = load_pef_application(&pef).unwrap();
        loaded.memory.add_region(trimesh_data, vec![0; 0x400]);
        loaded.memory.add_region(front_base, vec![0; 8 * 8 * 2]);
        loaded.gworlds = vec![PpcGWorldRecord {
            ui_theme: crate::ui_theme::UiThemeId::ClassicSystem7,
            port: PPC_MAIN_GWORLD,
            pixmap_handle: 0,
            pixmap: 0,
            base_addr: front_base,
            gdevice: PPC_MAIN_GDEVICE,
            width: 8,
            height: 8,
            depth: 16,
            row_bytes: 16,
            pixels_locked: false,
            pixels_no_purge: false,
        }];
        loaded
            .current_gworld
            .with_mut(|current_gworld| *current_gworld = PPC_MAIN_GWORLD);

        loaded
            .memory
            .write_u32_be(trimesh_data + PPC_Q3_TRIMESH_NUM_TRIANGLES_OFFSET, 1)
            .unwrap();
        loaded
            .memory
            .write_u32_be(
                trimesh_data + PPC_Q3_TRIMESH_TRIANGLES_OFFSET,
                triangles_ptr,
            )
            .unwrap();
        loaded
            .memory
            .write_u32_be(trimesh_data + PPC_Q3_TRIMESH_NUM_POINTS_OFFSET, 3)
            .unwrap();
        loaded
            .memory
            .write_u32_be(trimesh_data + PPC_Q3_TRIMESH_POINTS_OFFSET, points_ptr)
            .unwrap();
        loaded
            .memory
            .write_u32_be(
                trimesh_data + PPC_Q3_TRIMESH_NUM_VERTEX_ATTRIBUTE_TYPES_OFFSET,
                1,
            )
            .unwrap();
        loaded
            .memory
            .write_u32_be(
                trimesh_data + PPC_Q3_TRIMESH_VERTEX_ATTRIBUTE_TYPES_OFFSET,
                vertex_attrs_ptr,
            )
            .unwrap();
        loaded
            .memory
            .write_u32_be(vertex_attrs_ptr, PPC_Q3_ATTRIBUTE_TYPE_NORMAL)
            .unwrap();
        loaded
            .memory
            .write_u32_be(vertex_attrs_ptr + 4, normals_ptr)
            .unwrap();
        loaded.memory.write_u32_be(vertex_attrs_ptr + 8, 0).unwrap();

        for (index, point) in [(0.1, -0.8, 0.0), (0.9, -0.8, 0.0), (0.1, 0.8, 0.0)]
            .into_iter()
            .enumerate()
        {
            ppc_write_q3_vector3d(
                &mut loaded.memory,
                points_ptr + index as u32 * PPC_Q3_POINT3D_SIZE,
                point,
            )
            .unwrap();
            ppc_write_q3_vector3d(
                &mut loaded.memory,
                normals_ptr + index as u32 * PPC_Q3_POINT3D_SIZE,
                (0.0, 0.0, -1.0),
            )
            .unwrap();
        }
        for (offset, index) in [0u32, 1, 2].into_iter().enumerate() {
            loaded
                .memory
                .write_u32_be(triangles_ptr + offset as u32 * 4, index)
                .unwrap();
        }

        loaded.q3_views.push(PpcQ3ViewStateRecord::new(view));
        loaded.q3_submissions.push(PpcQ3SubmissionRecord {
            view,
            kind: PpcQ3SubmissionKind::TriMesh,
            primary: trimesh_data,
            secondary: 0,
        });
        loaded
            .q3_submission_transforms
            .push(PpcQ3SubmissionTransformRecord {
                view,
                kind: PpcQ3SubmissionKind::TriMesh,
                primary: trimesh_data,
                secondary: 0,
                local_to_world: ppc_q3_matrix4x4_identity(),
            });
        loaded
            .q3_submission_materials
            .push(PpcQ3SubmissionMaterialRecord {
                view,
                kind: PpcQ3SubmissionKind::TriMesh,
                primary: trimesh_data,
                secondary: 0,
                shader: 0,
                illumination_type: PPC_Q3_ILLUMINATION_TYPE_PHONG,
                styles: vec![PpcQ3StyleRecord {
                    style,
                    kind: PpcQ3StyleKind::Backfacing,
                    value: backfacing_style,
                }],
                fog_style: None,
                attributes: vec![diffuse_attribute(attribute_set)],
                shader_uv_transform: None,
                shader_boundary: None,
                texture_shader: None,
                mipmap_texture: None,
            });
        loaded
            .q3_submission_lights
            .push(PpcQ3SubmissionLightRecord {
                view,
                kind: PpcQ3SubmissionKind::TriMesh,
                primary: trimesh_data,
                secondary: 0,
                light_group: 0,
                lights: vec![PpcQ3LightRecord {
                    light: directional_light,
                    light_type: PPC_Q3_LIGHT_TYPE_DIRECTIONAL,
                    data: PpcQ3LightData {
                        is_on: 1,
                        brightness: 1.0,
                        color: (0.0, 1.0, 0.0),
                    },
                    kind: PpcQ3LightKind::Directional {
                        casts_shadows: 0,
                        direction: (0.0, 0.0, -1.0),
                    },
                }],
            });

        let stats = loaded.render_q3_scene_commands_to_front_buffer();
        (loaded, front_base, stats)
    }

    let (mut both_loaded, both_front_base, both_stats) =
        render_with_backfacing_style(PPC_Q3_BACKFACING_STYLE_BOTH);
    assert_eq!(both_stats.triangles, 1);
    assert_eq!(
        both_loaded
            .memory
            .read_u16_be(both_front_base + 5 * 16 + 5 * 2),
        Some(0)
    );

    let (mut flip_loaded, flip_front_base, flip_stats) =
        render_with_backfacing_style(PPC_Q3_BACKFACING_STYLE_FLIP);
    assert_eq!(flip_stats.triangles, 1);
    assert_eq!(
        flip_loaded
            .memory
            .read_u16_be(flip_front_base + 5 * 16 + 5 * 2),
        Some(0x03e0)
    );
}

#[test]
fn q3_software_renderer_fills_trimeshes_with_depth_testing() {
    fn write_triangle_mesh(
        memory: &mut PpcSectionMem,
        trimesh_data: u32,
        points_ptr: u32,
        triangles_ptr: u32,
        z: f32,
    ) {
        memory
            .write_u32_be(trimesh_data + PPC_Q3_TRIMESH_NUM_TRIANGLES_OFFSET, 1)
            .unwrap();
        memory
            .write_u32_be(
                trimesh_data + PPC_Q3_TRIMESH_TRIANGLES_OFFSET,
                triangles_ptr,
            )
            .unwrap();
        memory
            .write_u32_be(trimesh_data + PPC_Q3_TRIMESH_NUM_POINTS_OFFSET, 3)
            .unwrap();
        memory
            .write_u32_be(trimesh_data + PPC_Q3_TRIMESH_POINTS_OFFSET, points_ptr)
            .unwrap();
        ppc_write_q3_vector3d(memory, points_ptr, (-0.8, -0.8, z)).unwrap();
        ppc_write_q3_vector3d(memory, points_ptr + 12, (-0.8, 0.8, z)).unwrap();
        ppc_write_q3_vector3d(memory, points_ptr + 24, (0.8, -0.8, z)).unwrap();
        memory.write_u32_be(triangles_ptr, 0).unwrap();
        memory.write_u32_be(triangles_ptr + 4, 1).unwrap();
        memory.write_u32_be(triangles_ptr + 8, 2).unwrap();
    }

    fn diffuse_attribute(attribute_set: u32, rgb: [f32; 3]) -> PpcQ3AttributeRecord {
        let mut data = Vec::new();
        for value in rgb {
            data.extend_from_slice(&value.to_bits().to_be_bytes());
        }
        PpcQ3AttributeRecord {
            attribute_set,
            attribute_type: PPC_Q3_ATTRIBUTE_TYPE_DIFFUSE_COLOR,
            data,
        }
    }

    let view = PPC_Q3_OBJECT_BASE;
    let near_attribute_set = PPC_Q3_OBJECT_BASE + PPC_Q3_OBJECT_STRIDE;
    let far_attribute_set = PPC_Q3_OBJECT_BASE + PPC_Q3_OBJECT_STRIDE * 2;
    let near_trimesh_data = PPC_DATA_BASE + 0x1000;
    let near_points_ptr = PPC_DATA_BASE + 0x1080;
    let near_triangles_ptr = PPC_DATA_BASE + 0x1100;
    let far_trimesh_data = PPC_DATA_BASE + 0x1200;
    let far_points_ptr = PPC_DATA_BASE + 0x1280;
    let far_triangles_ptr = PPC_DATA_BASE + 0x1300;
    let front_base = PPC_HEAP_BASE + 0x1000;
    let pef = synthetic_pef_with_library_import(b"QuickDraw\xaa 3D", b"Q3TriMesh_Submit");
    let mut loaded = load_pef_application(&pef).unwrap();
    loaded.memory.add_region(near_trimesh_data, vec![0; 0x400]);
    loaded.memory.add_region(front_base, vec![0; 8 * 8 * 2]);
    loaded.gworlds = vec![PpcGWorldRecord {
        ui_theme: crate::ui_theme::UiThemeId::ClassicSystem7,
        port: PPC_MAIN_GWORLD,
        pixmap_handle: 0,
        pixmap: 0,
        base_addr: front_base,
        gdevice: PPC_MAIN_GDEVICE,
        width: 8,
        height: 8,
        depth: 16,
        row_bytes: 16,
        pixels_locked: false,
        pixels_no_purge: false,
    }];
    loaded
        .current_gworld
        .with_mut(|current_gworld| *current_gworld = PPC_MAIN_GWORLD);

    write_triangle_mesh(
        &mut loaded.memory,
        near_trimesh_data,
        near_points_ptr,
        near_triangles_ptr,
        -0.5,
    );
    write_triangle_mesh(
        &mut loaded.memory,
        far_trimesh_data,
        far_points_ptr,
        far_triangles_ptr,
        0.5,
    );
    loaded.q3_views.push(PpcQ3ViewStateRecord::new(view));
    for (trimesh_data, attribute_set, rgb) in [
        (near_trimesh_data, near_attribute_set, [0.0, 0.0, 1.0]),
        (far_trimesh_data, far_attribute_set, [1.0, 0.0, 0.0]),
    ] {
        loaded.q3_submissions.push(PpcQ3SubmissionRecord {
            view,
            kind: PpcQ3SubmissionKind::TriMesh,
            primary: trimesh_data,
            secondary: 0,
        });
        loaded
            .q3_submission_transforms
            .push(PpcQ3SubmissionTransformRecord {
                view,
                kind: PpcQ3SubmissionKind::TriMesh,
                primary: trimesh_data,
                secondary: 0,
                local_to_world: ppc_q3_matrix4x4_identity(),
            });
        loaded
            .q3_submission_materials
            .push(PpcQ3SubmissionMaterialRecord {
                view,
                kind: PpcQ3SubmissionKind::TriMesh,
                primary: trimesh_data,
                secondary: 0,
                shader: 0,
                illumination_type: PPC_Q3_ILLUMINATION_TYPE_PHONG,
                styles: Vec::new(),
                fog_style: None,
                attributes: vec![diffuse_attribute(attribute_set, rgb)],
                shader_uv_transform: None,
                shader_boundary: None,
                texture_shader: None,
                mipmap_texture: None,
            });
        loaded
            .q3_submission_lights
            .push(PpcQ3SubmissionLightRecord {
                view,
                kind: PpcQ3SubmissionKind::TriMesh,
                primary: trimesh_data,
                secondary: 0,
                light_group: 0,
                lights: Vec::new(),
            });
    }

    let stats = loaded.render_q3_scene_commands_to_front_buffer();

    assert_eq!(stats.commands, 2);
    assert_eq!(stats.vertices, 6);
    assert_eq!(stats.triangles, 2);
    assert!(stats.pixels >= 15);
    assert_eq!(
        loaded.memory.read_u16_be(front_base + 4 * 16 + 3 * 2),
        Some(0x001f)
    );
    assert_ne!(
        loaded.memory.read_u16_be(front_base + 4 * 16 + 3 * 2),
        Some(0x7c00)
    );
}

#[test]
fn q3_software_renderer_sorts_transparent_filled_triangles_back_to_front() {
    fn write_triangle_mesh(
        memory: &mut PpcSectionMem,
        trimesh_data: u32,
        points_ptr: u32,
        triangles_ptr: u32,
        z: f32,
    ) {
        memory
            .write_u32_be(trimesh_data + PPC_Q3_TRIMESH_NUM_TRIANGLES_OFFSET, 1)
            .unwrap();
        memory
            .write_u32_be(
                trimesh_data + PPC_Q3_TRIMESH_TRIANGLES_OFFSET,
                triangles_ptr,
            )
            .unwrap();
        memory
            .write_u32_be(trimesh_data + PPC_Q3_TRIMESH_NUM_POINTS_OFFSET, 3)
            .unwrap();
        memory
            .write_u32_be(trimesh_data + PPC_Q3_TRIMESH_POINTS_OFFSET, points_ptr)
            .unwrap();
        ppc_write_q3_vector3d(memory, points_ptr, (-0.8, -0.8, z)).unwrap();
        ppc_write_q3_vector3d(memory, points_ptr + 12, (-0.8, 0.8, z)).unwrap();
        ppc_write_q3_vector3d(memory, points_ptr + 24, (0.8, -0.8, z)).unwrap();
        memory.write_u32_be(triangles_ptr, 0).unwrap();
        memory.write_u32_be(triangles_ptr + 4, 1).unwrap();
        memory.write_u32_be(triangles_ptr + 8, 2).unwrap();
    }

    fn color_attribute(
        attribute_set: u32,
        attribute_type: u32,
        rgb: [f32; 3],
    ) -> PpcQ3AttributeRecord {
        let mut data = Vec::new();
        for value in rgb {
            data.extend_from_slice(&value.to_bits().to_be_bytes());
        }
        PpcQ3AttributeRecord {
            attribute_set,
            attribute_type,
            data,
        }
    }

    let view = PPC_Q3_OBJECT_BASE;
    let near_attribute_set = PPC_Q3_OBJECT_BASE + PPC_Q3_OBJECT_STRIDE;
    let far_attribute_set = PPC_Q3_OBJECT_BASE + PPC_Q3_OBJECT_STRIDE * 2;
    let near_trimesh_data = PPC_DATA_BASE + 0x1000;
    let near_points_ptr = PPC_DATA_BASE + 0x1080;
    let near_triangles_ptr = PPC_DATA_BASE + 0x1100;
    let far_trimesh_data = PPC_DATA_BASE + 0x1200;
    let far_points_ptr = PPC_DATA_BASE + 0x1280;
    let far_triangles_ptr = PPC_DATA_BASE + 0x1300;
    let front_base = PPC_HEAP_BASE + 0x1000;
    let pef = synthetic_pef_with_library_import(b"QuickDraw\xaa 3D", b"Q3TriMesh_Submit");
    let mut loaded = load_pef_application(&pef).unwrap();
    loaded.memory.add_region(near_trimesh_data, vec![0; 0x400]);
    loaded.memory.add_region(front_base, vec![0; 8 * 8 * 2]);
    loaded.gworlds = vec![PpcGWorldRecord {
        ui_theme: crate::ui_theme::UiThemeId::ClassicSystem7,
        port: PPC_MAIN_GWORLD,
        pixmap_handle: 0,
        pixmap: 0,
        base_addr: front_base,
        gdevice: PPC_MAIN_GDEVICE,
        width: 8,
        height: 8,
        depth: 16,
        row_bytes: 16,
        pixels_locked: false,
        pixels_no_purge: false,
    }];
    loaded
        .current_gworld
        .with_mut(|current_gworld| *current_gworld = PPC_MAIN_GWORLD);

    write_triangle_mesh(
        &mut loaded.memory,
        near_trimesh_data,
        near_points_ptr,
        near_triangles_ptr,
        -0.5,
    );
    write_triangle_mesh(
        &mut loaded.memory,
        far_trimesh_data,
        far_points_ptr,
        far_triangles_ptr,
        0.5,
    );
    loaded.q3_views.push(PpcQ3ViewStateRecord::new(view));
    for (trimesh_data, attribute_set, rgb) in [
        (near_trimesh_data, near_attribute_set, [1.0, 0.0, 0.0]),
        (far_trimesh_data, far_attribute_set, [0.0, 0.0, 1.0]),
    ] {
        loaded.q3_submissions.push(PpcQ3SubmissionRecord {
            view,
            kind: PpcQ3SubmissionKind::TriMesh,
            primary: trimesh_data,
            secondary: 0,
        });
        loaded
            .q3_submission_transforms
            .push(PpcQ3SubmissionTransformRecord {
                view,
                kind: PpcQ3SubmissionKind::TriMesh,
                primary: trimesh_data,
                secondary: 0,
                local_to_world: ppc_q3_matrix4x4_identity(),
            });
        loaded
            .q3_submission_materials
            .push(PpcQ3SubmissionMaterialRecord {
                view,
                kind: PpcQ3SubmissionKind::TriMesh,
                primary: trimesh_data,
                secondary: 0,
                shader: 0,
                illumination_type: PPC_Q3_ILLUMINATION_TYPE_PHONG,
                styles: Vec::new(),
                fog_style: None,
                attributes: vec![
                    color_attribute(attribute_set, PPC_Q3_ATTRIBUTE_TYPE_DIFFUSE_COLOR, rgb),
                    color_attribute(
                        attribute_set,
                        PPC_Q3_ATTRIBUTE_TYPE_TRANSPARENCY_COLOR,
                        [0.5, 0.5, 0.5],
                    ),
                ],
                shader_uv_transform: None,
                shader_boundary: None,
                texture_shader: None,
                mipmap_texture: None,
            });
        loaded
            .q3_submission_lights
            .push(PpcQ3SubmissionLightRecord {
                view,
                kind: PpcQ3SubmissionKind::TriMesh,
                primary: trimesh_data,
                secondary: 0,
                light_group: 0,
                lights: Vec::new(),
            });
    }

    let stats = loaded.render_q3_scene_commands_to_front_buffer();
    let far_over_black =
        ppc_q3_blend_transparency_color((0.0, 0.0, 1.0), (0.0, 0.0, 0.0), (0.5, 0.5, 0.5));
    let expected = ppc_q3_rgb555(ppc_q3_blend_transparency_color(
        (1.0, 0.0, 0.0),
        far_over_black,
        (0.5, 0.5, 0.5),
    ));
    let near_only = ppc_q3_rgb555(ppc_q3_blend_transparency_color(
        (1.0, 0.0, 0.0),
        (0.0, 0.0, 0.0),
        (0.5, 0.5, 0.5),
    ));

    assert_eq!(stats.commands, 2);
    assert_eq!(stats.vertices, 6);
    assert_eq!(stats.triangles, 2);
    assert!(stats.pixels >= 30);
    assert_eq!(
        loaded.memory.read_u16_be(front_base + 4 * 16 + 3 * 2),
        Some(expected)
    );
    assert_ne!(
        loaded.memory.read_u16_be(front_base + 4 * 16 + 3 * 2),
        Some(near_only)
    );
}

#[test]
fn q3_software_renderer_sorts_transparent_points_and_edges_back_to_front() {
    fn write_trimesh(
        memory: &mut PpcSectionMem,
        trimesh_data: u32,
        points_ptr: u32,
        triangles_ptr: u32,
        z: f32,
        use_triangles: bool,
    ) {
        let points = if use_triangles {
            [(-0.8, -0.8, z), (-0.8, 0.8, z), (0.8, -0.8, z)]
        } else {
            [(0.0, 0.0, z), (0.0, 0.0, z), (0.0, 0.0, z)]
        };
        memory
            .write_u32_be(
                trimesh_data + PPC_Q3_TRIMESH_NUM_POINTS_OFFSET,
                if use_triangles { 3 } else { 1 },
            )
            .unwrap();
        memory
            .write_u32_be(trimesh_data + PPC_Q3_TRIMESH_POINTS_OFFSET, points_ptr)
            .unwrap();
        for (index, point) in points.into_iter().enumerate() {
            ppc_write_q3_vector3d(
                memory,
                points_ptr + index as u32 * PPC_Q3_POINT3D_SIZE,
                point,
            )
            .unwrap();
        }
        if use_triangles {
            memory
                .write_u32_be(trimesh_data + PPC_Q3_TRIMESH_NUM_TRIANGLES_OFFSET, 1)
                .unwrap();
            memory
                .write_u32_be(
                    trimesh_data + PPC_Q3_TRIMESH_TRIANGLES_OFFSET,
                    triangles_ptr,
                )
                .unwrap();
            for (offset, index) in [0u32, 1, 2].into_iter().enumerate() {
                memory
                    .write_u32_be(triangles_ptr + offset as u32 * 4, index)
                    .unwrap();
            }
        }
    }

    fn color_attribute(
        attribute_set: u32,
        attribute_type: u32,
        rgb: [f32; 3],
    ) -> PpcQ3AttributeRecord {
        let mut data = Vec::new();
        for value in rgb {
            data.extend_from_slice(&value.to_bits().to_be_bytes());
        }
        PpcQ3AttributeRecord {
            attribute_set,
            attribute_type,
            data,
        }
    }

    fn render_case(
        fill_style: u32,
        use_triangles: bool,
        sample_x: u32,
        sample_y: u32,
    ) -> (PpcLoadedApp, u32, PpcQ3SoftwareRenderStats, u16) {
        let view = PPC_Q3_OBJECT_BASE;
        let near_attribute_set = PPC_Q3_OBJECT_BASE + PPC_Q3_OBJECT_STRIDE;
        let far_attribute_set = PPC_Q3_OBJECT_BASE + PPC_Q3_OBJECT_STRIDE * 2;
        let style = PPC_Q3_OBJECT_BASE + PPC_Q3_OBJECT_STRIDE * 3;
        let near_trimesh_data = PPC_DATA_BASE + 0x1000;
        let near_points_ptr = PPC_DATA_BASE + 0x1080;
        let near_triangles_ptr = PPC_DATA_BASE + 0x1100;
        let far_trimesh_data = PPC_DATA_BASE + 0x1200;
        let far_points_ptr = PPC_DATA_BASE + 0x1280;
        let far_triangles_ptr = PPC_DATA_BASE + 0x1300;
        let front_base = PPC_HEAP_BASE + 0x1000;
        let pef = synthetic_pef_with_library_import(b"QuickDraw\xaa 3D", b"Q3TriMesh_Submit");
        let mut loaded = load_pef_application(&pef).unwrap();
        loaded.memory.add_region(near_trimesh_data, vec![0; 0x400]);
        loaded.memory.add_region(front_base, vec![0; 8 * 8 * 2]);
        loaded.gworlds = vec![PpcGWorldRecord {
            ui_theme: crate::ui_theme::UiThemeId::ClassicSystem7,
            port: PPC_MAIN_GWORLD,
            pixmap_handle: 0,
            pixmap: 0,
            base_addr: front_base,
            gdevice: PPC_MAIN_GDEVICE,
            width: 8,
            height: 8,
            depth: 16,
            row_bytes: 16,
            pixels_locked: false,
            pixels_no_purge: false,
        }];
        loaded
            .current_gworld
            .with_mut(|current_gworld| *current_gworld = PPC_MAIN_GWORLD);

        write_trimesh(
            &mut loaded.memory,
            near_trimesh_data,
            near_points_ptr,
            near_triangles_ptr,
            -0.5,
            use_triangles,
        );
        write_trimesh(
            &mut loaded.memory,
            far_trimesh_data,
            far_points_ptr,
            far_triangles_ptr,
            0.5,
            use_triangles,
        );
        loaded.q3_views.push(PpcQ3ViewStateRecord::new(view));
        for (trimesh_data, attribute_set, rgb) in [
            (near_trimesh_data, near_attribute_set, [1.0, 0.0, 0.0]),
            (far_trimesh_data, far_attribute_set, [0.0, 0.0, 1.0]),
        ] {
            loaded.q3_submissions.push(PpcQ3SubmissionRecord {
                view,
                kind: PpcQ3SubmissionKind::TriMesh,
                primary: trimesh_data,
                secondary: 0,
            });
            loaded
                .q3_submission_transforms
                .push(PpcQ3SubmissionTransformRecord {
                    view,
                    kind: PpcQ3SubmissionKind::TriMesh,
                    primary: trimesh_data,
                    secondary: 0,
                    local_to_world: ppc_q3_matrix4x4_identity(),
                });
            loaded
                .q3_submission_materials
                .push(PpcQ3SubmissionMaterialRecord {
                    view,
                    kind: PpcQ3SubmissionKind::TriMesh,
                    primary: trimesh_data,
                    secondary: 0,
                    shader: 0,
                    illumination_type: PPC_Q3_ILLUMINATION_TYPE_PHONG,
                    styles: vec![PpcQ3StyleRecord {
                        style,
                        kind: PpcQ3StyleKind::Fill,
                        value: fill_style,
                    }],
                    fog_style: None,
                    attributes: vec![
                        color_attribute(
                            attribute_set,
                            PPC_Q3_ATTRIBUTE_TYPE_DIFFUSE_COLOR,
                            rgb,
                        ),
                        color_attribute(
                            attribute_set,
                            PPC_Q3_ATTRIBUTE_TYPE_TRANSPARENCY_COLOR,
                            [0.5, 0.5, 0.5],
                        ),
                    ],
                    shader_uv_transform: None,
                    shader_boundary: None,
                    texture_shader: None,
                    mipmap_texture: None,
                });
            loaded
                .q3_submission_lights
                .push(PpcQ3SubmissionLightRecord {
                    view,
                    kind: PpcQ3SubmissionKind::TriMesh,
                    primary: trimesh_data,
                    secondary: 0,
                    light_group: 0,
                    lights: Vec::new(),
                });
        }

        let stats = loaded.render_q3_scene_commands_to_front_buffer();
        let pixel = loaded
            .memory
            .read_u16_be(front_base + sample_y * 16 + sample_x * 2)
            .unwrap();
        (loaded, front_base, stats, pixel)
    }

    let far_over_black =
        ppc_q3_blend_transparency_color((0.0, 0.0, 1.0), (0.0, 0.0, 0.0), (0.5, 0.5, 0.5));
    let expected = ppc_q3_rgb555(ppc_q3_blend_transparency_color(
        (1.0, 0.0, 0.0),
        far_over_black,
        (0.5, 0.5, 0.5),
    ));

    let (_point_loaded, _point_front_base, point_stats, point_pixel) =
        render_case(PPC_Q3_FILL_STYLE_POINTS, false, 4, 4);
    assert_eq!(point_stats.commands, 2);
    assert_eq!(point_stats.vertices, 2);
    assert_eq!(point_stats.triangles, 0);
    assert_eq!(point_stats.pixels, 2);
    assert_eq!(point_pixel, expected);

    let (_edge_loaded, _edge_front_base, edge_stats, edge_pixel) =
        render_case(PPC_Q3_FILL_STYLE_EDGES, true, 1, 3);
    assert_eq!(edge_stats.commands, 2);
    assert_eq!(edge_stats.vertices, 6);
    assert_eq!(edge_stats.triangles, 2);
    assert!(edge_stats.pixels > 0);
    assert_eq!(edge_pixel, expected);
}

#[test]
fn q3_software_renderer_blends_transparency_color_with_front_buffer() {
    fn color_attribute(
        attribute_set: u32,
        attribute_type: u32,
        rgb: [f32; 3],
    ) -> PpcQ3AttributeRecord {
        let mut data = Vec::new();
        for value in rgb {
            data.extend_from_slice(&value.to_bits().to_be_bytes());
        }
        PpcQ3AttributeRecord {
            attribute_set,
            attribute_type,
            data,
        }
    }

    let view = PPC_Q3_OBJECT_BASE;
    let attribute_set = PPC_Q3_OBJECT_BASE + PPC_Q3_OBJECT_STRIDE;
    let trimesh_data = PPC_DATA_BASE + 0x1000;
    let points_ptr = PPC_DATA_BASE + 0x1080;
    let triangles_ptr = PPC_DATA_BASE + 0x1100;
    let front_base = PPC_HEAP_BASE + 0x1000;
    let pef = synthetic_pef_with_library_import(b"QuickDraw\xaa 3D", b"Q3TriMesh_Submit");
    let mut loaded = load_pef_application(&pef).unwrap();
    let mut front_buffer_bytes = Vec::new();
    for _ in 0..64 {
        front_buffer_bytes.extend_from_slice(&0x001fu16.to_be_bytes());
    }
    loaded.memory.add_region(trimesh_data, vec![0; 0x200]);
    loaded.memory.add_region(front_base, front_buffer_bytes);
    loaded.gworlds = vec![PpcGWorldRecord {
        ui_theme: crate::ui_theme::UiThemeId::ClassicSystem7,
        port: PPC_MAIN_GWORLD,
        pixmap_handle: 0,
        pixmap: 0,
        base_addr: front_base,
        gdevice: PPC_MAIN_GDEVICE,
        width: 8,
        height: 8,
        depth: 16,
        row_bytes: 16,
        pixels_locked: false,
        pixels_no_purge: false,
    }];
    loaded
        .current_gworld
        .with_mut(|current_gworld| *current_gworld = PPC_MAIN_GWORLD);

    loaded
        .memory
        .write_u32_be(trimesh_data + PPC_Q3_TRIMESH_NUM_TRIANGLES_OFFSET, 1)
        .unwrap();
    loaded
        .memory
        .write_u32_be(
            trimesh_data + PPC_Q3_TRIMESH_TRIANGLES_OFFSET,
            triangles_ptr,
        )
        .unwrap();
    loaded
        .memory
        .write_u32_be(trimesh_data + PPC_Q3_TRIMESH_NUM_POINTS_OFFSET, 3)
        .unwrap();
    loaded
        .memory
        .write_u32_be(trimesh_data + PPC_Q3_TRIMESH_POINTS_OFFSET, points_ptr)
        .unwrap();
    ppc_write_q3_vector3d(&mut loaded.memory, points_ptr, (0.0, 0.0, 0.0)).unwrap();
    ppc_write_q3_vector3d(&mut loaded.memory, points_ptr + 12, (0.0, 0.9, 0.0)).unwrap();
    ppc_write_q3_vector3d(&mut loaded.memory, points_ptr + 24, (0.9, 0.0, 0.0)).unwrap();
    loaded.memory.write_u32_be(triangles_ptr, 0).unwrap();
    loaded.memory.write_u32_be(triangles_ptr + 4, 1).unwrap();
    loaded.memory.write_u32_be(triangles_ptr + 8, 2).unwrap();

    loaded.q3_views.push(PpcQ3ViewStateRecord::new(view));
    loaded.q3_submissions.push(PpcQ3SubmissionRecord {
        view,
        kind: PpcQ3SubmissionKind::TriMesh,
        primary: trimesh_data,
        secondary: 0,
    });
    loaded
        .q3_submission_transforms
        .push(PpcQ3SubmissionTransformRecord {
            view,
            kind: PpcQ3SubmissionKind::TriMesh,
            primary: trimesh_data,
            secondary: 0,
            local_to_world: ppc_q3_matrix4x4_identity(),
        });
    loaded
        .q3_submission_materials
        .push(PpcQ3SubmissionMaterialRecord {
            view,
            kind: PpcQ3SubmissionKind::TriMesh,
            primary: trimesh_data,
            secondary: 0,
            shader: 0,
            illumination_type: PPC_Q3_ILLUMINATION_TYPE_PHONG,
            styles: Vec::new(),
            fog_style: None,
            attributes: vec![
                color_attribute(
                    attribute_set,
                    PPC_Q3_ATTRIBUTE_TYPE_DIFFUSE_COLOR,
                    [1.0, 0.0, 0.0],
                ),
                color_attribute(
                    attribute_set,
                    PPC_Q3_ATTRIBUTE_TYPE_TRANSPARENCY_COLOR,
                    [0.25, 0.25, 0.25],
                ),
            ],
            shader_uv_transform: None,
            shader_boundary: None,
            texture_shader: None,
            mipmap_texture: None,
        });
    loaded
        .q3_submission_lights
        .push(PpcQ3SubmissionLightRecord {
            view,
            kind: PpcQ3SubmissionKind::TriMesh,
            primary: trimesh_data,
            secondary: 0,
            light_group: 0,
            lights: Vec::new(),
        });

    let stats = loaded.render_q3_scene_commands_to_front_buffer();

    assert_eq!(stats.commands, 1);
    assert_eq!(stats.triangles, 1);
    assert!(stats.pixels > 0);
    assert_eq!(
        loaded.memory.read_u16_be(front_base + 4 * 16 + 4 * 2),
        Some(0x2017)
    );
    assert_ne!(
        loaded.memory.read_u16_be(front_base + 4 * 16 + 4 * 2),
        Some(0x7c00)
    );
    assert_ne!(
        loaded.memory.read_u16_be(front_base + 4 * 16 + 4 * 2),
        Some(0x001f)
    );
}

#[test]
fn q3_software_material_blend_opacity_multiplies_texture_and_vertex_alpha() {
    let material = PpcQ3SoftwareMaterialSample {
        diffuse: (1.0, 1.0, 1.0),
        ambient_coefficient: 1.0,
        specular_color: (0.0, 0.0, 0.0),
        specular_control: 0.0,
        highlight_state: true,
        transparency: (1.0, 1.0, 1.0),
        illumination_type: PPC_Q3_ILLUMINATION_TYPE_PHONG,
        texture: None,
        uv_transform: None,
        shader_boundary: None,
        fog_style: None,
    };
    assert_eq!(
        ppc_q3_software_blend_opacity(&material, 0.5, Some(0.25)),
        0.125
    );

    let alpha_fog_material = PpcQ3SoftwareMaterialSample {
        fog_style: Some(PpcQ3FogStyleData {
            state: 1,
            mode: PPC_Q3_FOG_MODE_ALPHA,
            fog_start: 0.0,
            fog_end: 1.0,
            density: 1.0,
            color: (0.0, 0.0, 0.0, 1.0),
        }),
        ..material
    };
    assert_eq!(
        ppc_q3_software_blend_opacity(&alpha_fog_material, 0.5, Some(0.25)),
        0.5
    );
}

#[test]
fn q3_software_renderer_blends_vertex_transparency_color_with_front_buffer() {
    fn color_attribute(
        attribute_set: u32,
        attribute_type: u32,
        rgb: [f32; 3],
    ) -> PpcQ3AttributeRecord {
        let mut data = Vec::new();
        for value in rgb {
            data.extend_from_slice(&value.to_bits().to_be_bytes());
        }
        PpcQ3AttributeRecord {
            attribute_set,
            attribute_type,
            data,
        }
    }

    let view = PPC_Q3_OBJECT_BASE;
    let attribute_set = PPC_Q3_OBJECT_BASE + PPC_Q3_OBJECT_STRIDE;
    let trimesh_data = PPC_DATA_BASE + 0x1000;
    let points_ptr = PPC_DATA_BASE + 0x1080;
    let triangles_ptr = PPC_DATA_BASE + 0x1100;
    let vertex_attrs_ptr = PPC_DATA_BASE + 0x1180;
    let vertex_transparency_ptr = PPC_DATA_BASE + 0x1200;
    let front_base = PPC_HEAP_BASE + 0x1000;
    let pef = synthetic_pef_with_library_import(b"QuickDraw\xaa 3D", b"Q3TriMesh_Submit");
    let mut loaded = load_pef_application(&pef).unwrap();
    let mut front_buffer_bytes = Vec::new();
    for _ in 0..64 {
        front_buffer_bytes.extend_from_slice(&0x001fu16.to_be_bytes());
    }
    loaded.memory.add_region(trimesh_data, vec![0; 0x400]);
    loaded.memory.add_region(front_base, front_buffer_bytes);
    loaded.gworlds = vec![PpcGWorldRecord {
        ui_theme: crate::ui_theme::UiThemeId::ClassicSystem7,
        port: PPC_MAIN_GWORLD,
        pixmap_handle: 0,
        pixmap: 0,
        base_addr: front_base,
        gdevice: PPC_MAIN_GDEVICE,
        width: 8,
        height: 8,
        depth: 16,
        row_bytes: 16,
        pixels_locked: false,
        pixels_no_purge: false,
    }];
    loaded
        .current_gworld
        .with_mut(|current_gworld| *current_gworld = PPC_MAIN_GWORLD);

    loaded
        .memory
        .write_u32_be(trimesh_data + PPC_Q3_TRIMESH_NUM_TRIANGLES_OFFSET, 1)
        .unwrap();
    loaded
        .memory
        .write_u32_be(
            trimesh_data + PPC_Q3_TRIMESH_TRIANGLES_OFFSET,
            triangles_ptr,
        )
        .unwrap();
    loaded
        .memory
        .write_u32_be(trimesh_data + PPC_Q3_TRIMESH_NUM_POINTS_OFFSET, 3)
        .unwrap();
    loaded
        .memory
        .write_u32_be(trimesh_data + PPC_Q3_TRIMESH_POINTS_OFFSET, points_ptr)
        .unwrap();
    loaded
        .memory
        .write_u32_be(
            trimesh_data + PPC_Q3_TRIMESH_NUM_VERTEX_ATTRIBUTE_TYPES_OFFSET,
            1,
        )
        .unwrap();
    loaded
        .memory
        .write_u32_be(
            trimesh_data + PPC_Q3_TRIMESH_VERTEX_ATTRIBUTE_TYPES_OFFSET,
            vertex_attrs_ptr,
        )
        .unwrap();
    loaded
        .memory
        .write_u32_be(vertex_attrs_ptr, PPC_Q3_ATTRIBUTE_TYPE_TRANSPARENCY_COLOR)
        .unwrap();
    loaded
        .memory
        .write_u32_be(vertex_attrs_ptr + 4, vertex_transparency_ptr)
        .unwrap();
    loaded.memory.write_u32_be(vertex_attrs_ptr + 8, 0).unwrap();
    ppc_write_q3_vector3d(&mut loaded.memory, points_ptr, (0.0, 0.0, 0.0)).unwrap();
    ppc_write_q3_vector3d(&mut loaded.memory, points_ptr + 12, (0.0, 0.9, 0.0)).unwrap();
    ppc_write_q3_vector3d(&mut loaded.memory, points_ptr + 24, (0.9, 0.0, 0.0)).unwrap();
    for index in 0..3 {
        ppc_write_q3_color_rgb(
            &mut loaded.memory,
            vertex_transparency_ptr + index * PPC_Q3_POINT3D_SIZE,
            (0.25, 0.25, 0.25),
        )
        .unwrap();
    }
    loaded.memory.write_u32_be(triangles_ptr, 0).unwrap();
    loaded.memory.write_u32_be(triangles_ptr + 4, 1).unwrap();
    loaded.memory.write_u32_be(triangles_ptr + 8, 2).unwrap();

    loaded.q3_views.push(PpcQ3ViewStateRecord::new(view));
    loaded.q3_submissions.push(PpcQ3SubmissionRecord {
        view,
        kind: PpcQ3SubmissionKind::TriMesh,
        primary: trimesh_data,
        secondary: 0,
    });
    loaded
        .q3_submission_transforms
        .push(PpcQ3SubmissionTransformRecord {
            view,
            kind: PpcQ3SubmissionKind::TriMesh,
            primary: trimesh_data,
            secondary: 0,
            local_to_world: ppc_q3_matrix4x4_identity(),
        });
    loaded
        .q3_submission_materials
        .push(PpcQ3SubmissionMaterialRecord {
            view,
            kind: PpcQ3SubmissionKind::TriMesh,
            primary: trimesh_data,
            secondary: 0,
            shader: 0,
            illumination_type: PPC_Q3_ILLUMINATION_TYPE_PHONG,
            styles: Vec::new(),
            fog_style: None,
            attributes: vec![color_attribute(
                attribute_set,
                PPC_Q3_ATTRIBUTE_TYPE_DIFFUSE_COLOR,
                [1.0, 0.0, 0.0],
            )],
            shader_uv_transform: None,
            shader_boundary: None,
            texture_shader: None,
            mipmap_texture: None,
        });
    loaded
        .q3_submission_lights
        .push(PpcQ3SubmissionLightRecord {
            view,
            kind: PpcQ3SubmissionKind::TriMesh,
            primary: trimesh_data,
            secondary: 0,
            light_group: 0,
            lights: Vec::new(),
        });

    let stats = loaded.render_q3_scene_commands_to_front_buffer();

    assert_eq!(stats.commands, 1);
    assert_eq!(stats.triangles, 1);
    assert!(stats.pixels > 0);
    assert_eq!(
        loaded.memory.read_u16_be(front_base + 4 * 16 + 4 * 2),
        Some(0x2017)
    );
    assert_ne!(
        loaded.memory.read_u16_be(front_base + 4 * 16 + 4 * 2),
        Some(0x7c00)
    );
    assert_ne!(
        loaded.memory.read_u16_be(front_base + 4 * 16 + 4 * 2),
        Some(0x001f)
    );
}

#[test]
fn q3_software_renderer_uses_triangle_diffuse_and_transparency_colors() {
    fn color_attribute(
        attribute_set: u32,
        attribute_type: u32,
        rgb: [f32; 3],
    ) -> PpcQ3AttributeRecord {
        let mut data = Vec::new();
        for value in rgb {
            data.extend_from_slice(&value.to_bits().to_be_bytes());
        }
        PpcQ3AttributeRecord {
            attribute_set,
            attribute_type,
            data,
        }
    }

    let view = PPC_Q3_OBJECT_BASE;
    let attribute_set = PPC_Q3_OBJECT_BASE + PPC_Q3_OBJECT_STRIDE;
    let trimesh_data = PPC_DATA_BASE + 0x1000;
    let points_ptr = PPC_DATA_BASE + 0x1080;
    let triangles_ptr = PPC_DATA_BASE + 0x1100;
    let triangle_attrs_ptr = PPC_DATA_BASE + 0x1180;
    let triangle_diffuse_ptr = PPC_DATA_BASE + 0x1200;
    let triangle_transparency_ptr = PPC_DATA_BASE + 0x1240;
    let front_base = PPC_HEAP_BASE + 0x1000;
    let pef = synthetic_pef_with_library_import(b"QuickDraw\xaa 3D", b"Q3TriMesh_Submit");
    let mut loaded = load_pef_application(&pef).unwrap();
    let mut front_buffer_bytes = Vec::new();
    for _ in 0..64 {
        front_buffer_bytes.extend_from_slice(&0x001fu16.to_be_bytes());
    }
    loaded.memory.add_region(trimesh_data, vec![0; 0x500]);
    loaded.memory.add_region(front_base, front_buffer_bytes);
    loaded.gworlds = vec![PpcGWorldRecord {
        ui_theme: crate::ui_theme::UiThemeId::ClassicSystem7,
        port: PPC_MAIN_GWORLD,
        pixmap_handle: 0,
        pixmap: 0,
        base_addr: front_base,
        gdevice: PPC_MAIN_GDEVICE,
        width: 8,
        height: 8,
        depth: 16,
        row_bytes: 16,
        pixels_locked: false,
        pixels_no_purge: false,
    }];
    loaded
        .current_gworld
        .with_mut(|current_gworld| *current_gworld = PPC_MAIN_GWORLD);

    loaded
        .memory
        .write_u32_be(trimesh_data + PPC_Q3_TRIMESH_NUM_TRIANGLES_OFFSET, 1)
        .unwrap();
    loaded
        .memory
        .write_u32_be(
            trimesh_data + PPC_Q3_TRIMESH_TRIANGLES_OFFSET,
            triangles_ptr,
        )
        .unwrap();
    loaded
        .memory
        .write_u32_be(
            trimesh_data + PPC_Q3_TRIMESH_NUM_TRIANGLE_ATTRIBUTE_TYPES_OFFSET,
            2,
        )
        .unwrap();
    loaded
        .memory
        .write_u32_be(
            trimesh_data + PPC_Q3_TRIMESH_TRIANGLE_ATTRIBUTE_TYPES_OFFSET,
            triangle_attrs_ptr,
        )
        .unwrap();
    loaded
        .memory
        .write_u32_be(trimesh_data + PPC_Q3_TRIMESH_NUM_POINTS_OFFSET, 3)
        .unwrap();
    loaded
        .memory
        .write_u32_be(trimesh_data + PPC_Q3_TRIMESH_POINTS_OFFSET, points_ptr)
        .unwrap();

    loaded
        .memory
        .write_u32_be(triangle_attrs_ptr, PPC_Q3_ATTRIBUTE_TYPE_DIFFUSE_COLOR)
        .unwrap();
    loaded
        .memory
        .write_u32_be(triangle_attrs_ptr + 4, triangle_diffuse_ptr)
        .unwrap();
    loaded
        .memory
        .write_u32_be(triangle_attrs_ptr + 8, 0)
        .unwrap();
    loaded
        .memory
        .write_u32_be(
            triangle_attrs_ptr + PPC_Q3_TRIMESH_ATTRIBUTE_DATA_SIZE,
            PPC_Q3_ATTRIBUTE_TYPE_TRANSPARENCY_COLOR,
        )
        .unwrap();
    loaded
        .memory
        .write_u32_be(
            triangle_attrs_ptr + PPC_Q3_TRIMESH_ATTRIBUTE_DATA_SIZE + 4,
            triangle_transparency_ptr,
        )
        .unwrap();
    loaded
        .memory
        .write_u32_be(
            triangle_attrs_ptr + PPC_Q3_TRIMESH_ATTRIBUTE_DATA_SIZE + 8,
            0,
        )
        .unwrap();
    ppc_write_q3_color_rgb(&mut loaded.memory, triangle_diffuse_ptr, (0.0, 1.0, 0.0)).unwrap();
    ppc_write_q3_color_rgb(
        &mut loaded.memory,
        triangle_transparency_ptr,
        (0.25, 0.25, 0.25),
    )
    .unwrap();
    ppc_write_q3_vector3d(&mut loaded.memory, points_ptr, (0.0, 0.0, 0.0)).unwrap();
    ppc_write_q3_vector3d(&mut loaded.memory, points_ptr + 12, (0.0, 0.9, 0.0)).unwrap();
    ppc_write_q3_vector3d(&mut loaded.memory, points_ptr + 24, (0.9, 0.0, 0.0)).unwrap();
    loaded.memory.write_u32_be(triangles_ptr, 0).unwrap();
    loaded.memory.write_u32_be(triangles_ptr + 4, 1).unwrap();
    loaded.memory.write_u32_be(triangles_ptr + 8, 2).unwrap();

    loaded.q3_views.push(PpcQ3ViewStateRecord::new(view));
    loaded.q3_submissions.push(PpcQ3SubmissionRecord {
        view,
        kind: PpcQ3SubmissionKind::TriMesh,
        primary: trimesh_data,
        secondary: 0,
    });
    loaded
        .q3_submission_transforms
        .push(PpcQ3SubmissionTransformRecord {
            view,
            kind: PpcQ3SubmissionKind::TriMesh,
            primary: trimesh_data,
            secondary: 0,
            local_to_world: ppc_q3_matrix4x4_identity(),
        });
    loaded
        .q3_submission_materials
        .push(PpcQ3SubmissionMaterialRecord {
            view,
            kind: PpcQ3SubmissionKind::TriMesh,
            primary: trimesh_data,
            secondary: 0,
            shader: 0,
            illumination_type: PPC_Q3_ILLUMINATION_TYPE_PHONG,
            styles: Vec::new(),
            fog_style: None,
            attributes: vec![color_attribute(
                attribute_set,
                PPC_Q3_ATTRIBUTE_TYPE_DIFFUSE_COLOR,
                [1.0, 0.0, 0.0],
            )],
            shader_uv_transform: None,
            shader_boundary: None,
            texture_shader: None,
            mipmap_texture: None,
        });
    loaded
        .q3_submission_lights
        .push(PpcQ3SubmissionLightRecord {
            view,
            kind: PpcQ3SubmissionKind::TriMesh,
            primary: trimesh_data,
            secondary: 0,
            light_group: 0,
            lights: Vec::new(),
        });

    let stats = loaded.render_q3_scene_commands_to_front_buffer();

    assert_eq!(stats.commands, 1);
    assert_eq!(stats.triangles, 1);
    assert!(stats.pixels > 0);
    assert_eq!(
        loaded.memory.read_u16_be(front_base + 4 * 16 + 4 * 2),
        Some(0x0117)
    );
    assert_ne!(
        loaded.memory.read_u16_be(front_base + 4 * 16 + 4 * 2),
        Some(0x2017)
    );
    assert_ne!(
        loaded.memory.read_u16_be(front_base + 4 * 16 + 4 * 2),
        Some(0x03e0)
    );
}

#[test]
fn q3_software_renderer_uses_vertex_diffuse_colors() {
    fn color_attribute(
        attribute_set: u32,
        attribute_type: u32,
        rgb: [f32; 3],
    ) -> PpcQ3AttributeRecord {
        let mut data = Vec::new();
        for value in rgb {
            data.extend_from_slice(&value.to_bits().to_be_bytes());
        }
        PpcQ3AttributeRecord {
            attribute_set,
            attribute_type,
            data,
        }
    }

    let view = PPC_Q3_OBJECT_BASE;
    let attribute_set = PPC_Q3_OBJECT_BASE + PPC_Q3_OBJECT_STRIDE;
    let trimesh_data = PPC_DATA_BASE + 0x1000;
    let points_ptr = PPC_DATA_BASE + 0x1080;
    let triangles_ptr = PPC_DATA_BASE + 0x1100;
    let vertex_attrs_ptr = PPC_DATA_BASE + 0x1180;
    let vertex_colors_ptr = PPC_DATA_BASE + 0x1200;
    let front_base = PPC_HEAP_BASE + 0x1000;
    let pef = synthetic_pef_with_library_import(b"QuickDraw\xaa 3D", b"Q3TriMesh_Submit");
    let mut loaded = load_pef_application(&pef).unwrap();
    loaded.memory.add_region(trimesh_data, vec![0; 0x400]);
    loaded.memory.add_region(front_base, vec![0; 8 * 8 * 2]);
    loaded.gworlds = vec![PpcGWorldRecord {
        ui_theme: crate::ui_theme::UiThemeId::ClassicSystem7,
        port: PPC_MAIN_GWORLD,
        pixmap_handle: 0,
        pixmap: 0,
        base_addr: front_base,
        gdevice: PPC_MAIN_GDEVICE,
        width: 8,
        height: 8,
        depth: 16,
        row_bytes: 16,
        pixels_locked: false,
        pixels_no_purge: false,
    }];
    loaded
        .current_gworld
        .with_mut(|current_gworld| *current_gworld = PPC_MAIN_GWORLD);

    loaded
        .memory
        .write_u32_be(trimesh_data + PPC_Q3_TRIMESH_NUM_TRIANGLES_OFFSET, 1)
        .unwrap();
    loaded
        .memory
        .write_u32_be(
            trimesh_data + PPC_Q3_TRIMESH_TRIANGLES_OFFSET,
            triangles_ptr,
        )
        .unwrap();
    loaded
        .memory
        .write_u32_be(trimesh_data + PPC_Q3_TRIMESH_NUM_POINTS_OFFSET, 3)
        .unwrap();
    loaded
        .memory
        .write_u32_be(trimesh_data + PPC_Q3_TRIMESH_POINTS_OFFSET, points_ptr)
        .unwrap();
    loaded
        .memory
        .write_u32_be(
            trimesh_data + PPC_Q3_TRIMESH_NUM_VERTEX_ATTRIBUTE_TYPES_OFFSET,
            1,
        )
        .unwrap();
    loaded
        .memory
        .write_u32_be(
            trimesh_data + PPC_Q3_TRIMESH_VERTEX_ATTRIBUTE_TYPES_OFFSET,
            vertex_attrs_ptr,
        )
        .unwrap();
    loaded
        .memory
        .write_u32_be(vertex_attrs_ptr, PPC_Q3_ATTRIBUTE_TYPE_DIFFUSE_COLOR)
        .unwrap();
    loaded
        .memory
        .write_u32_be(vertex_attrs_ptr + 4, vertex_colors_ptr)
        .unwrap();
    loaded.memory.write_u32_be(vertex_attrs_ptr + 8, 0).unwrap();
    ppc_write_q3_vector3d(&mut loaded.memory, points_ptr, (0.0, 0.0, 0.0)).unwrap();
    ppc_write_q3_vector3d(&mut loaded.memory, points_ptr + 12, (0.0, 0.9, 0.0)).unwrap();
    ppc_write_q3_vector3d(&mut loaded.memory, points_ptr + 24, (0.9, 0.0, 0.0)).unwrap();
    for index in 0..3 {
        ppc_write_q3_color_rgb(
            &mut loaded.memory,
            vertex_colors_ptr + index * PPC_Q3_POINT3D_SIZE,
            (0.0, 0.0, 1.0),
        )
        .unwrap();
    }
    loaded.memory.write_u32_be(triangles_ptr, 0).unwrap();
    loaded.memory.write_u32_be(triangles_ptr + 4, 1).unwrap();
    loaded.memory.write_u32_be(triangles_ptr + 8, 2).unwrap();

    loaded.q3_views.push(PpcQ3ViewStateRecord::new(view));
    loaded.q3_submissions.push(PpcQ3SubmissionRecord {
        view,
        kind: PpcQ3SubmissionKind::TriMesh,
        primary: trimesh_data,
        secondary: 0,
    });
    loaded
        .q3_submission_transforms
        .push(PpcQ3SubmissionTransformRecord {
            view,
            kind: PpcQ3SubmissionKind::TriMesh,
            primary: trimesh_data,
            secondary: 0,
            local_to_world: ppc_q3_matrix4x4_identity(),
        });
    loaded
        .q3_submission_materials
        .push(PpcQ3SubmissionMaterialRecord {
            view,
            kind: PpcQ3SubmissionKind::TriMesh,
            primary: trimesh_data,
            secondary: 0,
            shader: 0,
            illumination_type: PPC_Q3_ILLUMINATION_TYPE_PHONG,
            styles: Vec::new(),
            fog_style: None,
            attributes: vec![color_attribute(
                attribute_set,
                PPC_Q3_ATTRIBUTE_TYPE_DIFFUSE_COLOR,
                [1.0, 0.0, 0.0],
            )],
            shader_uv_transform: None,
            shader_boundary: None,
            texture_shader: None,
            mipmap_texture: None,
        });
    loaded
        .q3_submission_lights
        .push(PpcQ3SubmissionLightRecord {
            view,
            kind: PpcQ3SubmissionKind::TriMesh,
            primary: trimesh_data,
            secondary: 0,
            light_group: 0,
            lights: Vec::new(),
        });

    let stats = loaded.render_q3_scene_commands_to_front_buffer();

    assert_eq!(stats.commands, 1);
    assert_eq!(stats.triangles, 1);
    assert!(stats.pixels > 0);
    assert_eq!(
        loaded.memory.read_u16_be(front_base + 4 * 16 + 4 * 2),
        Some(0x001f)
    );
    assert_ne!(
        loaded.memory.read_u16_be(front_base + 4 * 16 + 4 * 2),
        Some(0x7c00)
    );
}

#[test]
fn q3_software_renderer_applies_captured_light_colors_to_materials() {
    let view = PPC_Q3_OBJECT_BASE;
    let attribute_set = PPC_Q3_OBJECT_BASE + PPC_Q3_OBJECT_STRIDE;
    let ambient_light = PPC_Q3_OBJECT_BASE + PPC_Q3_OBJECT_STRIDE * 2;
    let directional_light = PPC_Q3_OBJECT_BASE + PPC_Q3_OBJECT_STRIDE * 3;
    let off_light = PPC_Q3_OBJECT_BASE + PPC_Q3_OBJECT_STRIDE * 4;
    let trimesh_data = PPC_DATA_BASE + 0x1000;
    let points_ptr = PPC_DATA_BASE + 0x1080;
    let triangles_ptr = PPC_DATA_BASE + 0x1100;
    let front_base = PPC_HEAP_BASE + 0x1000;
    let pef = synthetic_pef_with_library_import(b"QuickDraw\xaa 3D", b"Q3TriMesh_Submit");
    let mut loaded = load_pef_application(&pef).unwrap();
    loaded.memory.add_region(trimesh_data, vec![0; 0x200]);
    loaded.memory.add_region(front_base, vec![0; 8 * 8 * 2]);
    loaded.gworlds = vec![PpcGWorldRecord {
        ui_theme: crate::ui_theme::UiThemeId::ClassicSystem7,
        port: PPC_MAIN_GWORLD,
        pixmap_handle: 0,
        pixmap: 0,
        base_addr: front_base,
        gdevice: PPC_MAIN_GDEVICE,
        width: 8,
        height: 8,
        depth: 16,
        row_bytes: 16,
        pixels_locked: false,
        pixels_no_purge: false,
    }];
    loaded
        .current_gworld
        .with_mut(|current_gworld| *current_gworld = PPC_MAIN_GWORLD);

    loaded
        .memory
        .write_u32_be(trimesh_data + PPC_Q3_TRIMESH_NUM_TRIANGLES_OFFSET, 1)
        .unwrap();
    loaded
        .memory
        .write_u32_be(
            trimesh_data + PPC_Q3_TRIMESH_TRIANGLES_OFFSET,
            triangles_ptr,
        )
        .unwrap();
    loaded
        .memory
        .write_u32_be(trimesh_data + PPC_Q3_TRIMESH_NUM_POINTS_OFFSET, 3)
        .unwrap();
    loaded
        .memory
        .write_u32_be(trimesh_data + PPC_Q3_TRIMESH_POINTS_OFFSET, points_ptr)
        .unwrap();
    ppc_write_q3_vector3d(&mut loaded.memory, points_ptr, (0.0, 0.0, 0.0)).unwrap();
    ppc_write_q3_vector3d(&mut loaded.memory, points_ptr + 12, (0.0, 0.9, 0.0)).unwrap();
    ppc_write_q3_vector3d(&mut loaded.memory, points_ptr + 24, (0.9, 0.0, 0.0)).unwrap();
    loaded.memory.write_u32_be(triangles_ptr, 0).unwrap();
    loaded.memory.write_u32_be(triangles_ptr + 4, 1).unwrap();
    loaded.memory.write_u32_be(triangles_ptr + 8, 2).unwrap();

    let mut diffuse = Vec::new();
    for value in [1.0f32, 1.0, 1.0] {
        diffuse.extend_from_slice(&value.to_bits().to_be_bytes());
    }
    loaded.q3_views.push(PpcQ3ViewStateRecord::new(view));
    loaded.q3_submissions.push(PpcQ3SubmissionRecord {
        view,
        kind: PpcQ3SubmissionKind::TriMesh,
        primary: trimesh_data,
        secondary: 0,
    });
    loaded
        .q3_submission_transforms
        .push(PpcQ3SubmissionTransformRecord {
            view,
            kind: PpcQ3SubmissionKind::TriMesh,
            primary: trimesh_data,
            secondary: 0,
            local_to_world: ppc_q3_matrix4x4_identity(),
        });
    loaded
        .q3_submission_materials
        .push(PpcQ3SubmissionMaterialRecord {
            view,
            kind: PpcQ3SubmissionKind::TriMesh,
            primary: trimesh_data,
            secondary: 0,
            shader: 0,
            illumination_type: PPC_Q3_ILLUMINATION_TYPE_PHONG,
            styles: Vec::new(),
            fog_style: None,
            attributes: vec![PpcQ3AttributeRecord {
                attribute_set,
                attribute_type: PPC_Q3_ATTRIBUTE_TYPE_DIFFUSE_COLOR,
                data: diffuse,
            }],
            shader_uv_transform: None,
            shader_boundary: None,
            texture_shader: None,
            mipmap_texture: None,
        });
    loaded
        .q3_submission_lights
        .push(PpcQ3SubmissionLightRecord {
            view,
            kind: PpcQ3SubmissionKind::TriMesh,
            primary: trimesh_data,
            secondary: 0,
            light_group: PPC_Q3_OBJECT_BASE + PPC_Q3_OBJECT_STRIDE * 5,
            lights: vec![
                PpcQ3LightRecord {
                    light: ambient_light,
                    light_type: PPC_Q3_LIGHT_TYPE_AMBIENT,
                    data: PpcQ3LightData {
                        is_on: 1,
                        brightness: 0.5,
                        color: (1.0, 0.0, 0.0),
                    },
                    kind: PpcQ3LightKind::Ambient,
                },
                PpcQ3LightRecord {
                    light: directional_light,
                    light_type: PPC_Q3_LIGHT_TYPE_DIRECTIONAL,
                    data: PpcQ3LightData {
                        is_on: 1,
                        brightness: 0.25,
                        color: (0.0, 1.0, 0.0),
                    },
                    kind: PpcQ3LightKind::Directional {
                        casts_shadows: 0,
                        // The triangle winding produces a -Z surface normal.
                        direction: (0.0, 0.0, 1.0),
                    },
                },
                PpcQ3LightRecord {
                    light: off_light,
                    light_type: PPC_Q3_LIGHT_TYPE_AMBIENT,
                    data: PpcQ3LightData {
                        is_on: 0,
                        brightness: 1.0,
                        color: (0.0, 0.0, 1.0),
                    },
                    kind: PpcQ3LightKind::Ambient,
                },
            ],
        });

    let stats = loaded.render_q3_scene_commands_to_front_buffer();

    assert_eq!(stats.commands, 1);
    assert_eq!(stats.triangles, 1);
    assert!(stats.pixels > 0);
    assert_eq!(
        loaded.memory.read_u16_be(front_base + 4 * 16 + 4 * 2),
        Some(0x4100)
    );
    // Omitting diffuse attributes uses the view's half-grey material,
    // so both enabled light channels contribute half the previous color.
    loaded.q3_submission_materials[0].attributes.clear();
    loaded.render_q3_scene_commands_to_front_buffer();
    assert_eq!(
        loaded.memory.read_u16_be(front_base + 4 * 16 + 4 * 2),
        Some(0x2080)
    );
}

#[test]
fn q3_software_renderer_applies_ambient_coefficient_to_ambient_lights() {
    let view = PPC_Q3_OBJECT_BASE;
    let attribute_set = PPC_Q3_OBJECT_BASE + PPC_Q3_OBJECT_STRIDE;
    let ambient_light = PPC_Q3_OBJECT_BASE + PPC_Q3_OBJECT_STRIDE * 2;
    let directional_light = PPC_Q3_OBJECT_BASE + PPC_Q3_OBJECT_STRIDE * 3;
    let mut diffuse = Vec::new();
    for value in [1.0f32, 1.0, 1.0] {
        diffuse.extend_from_slice(&value.to_bits().to_be_bytes());
    }
    let ambient_coefficient = 0.25f32.to_bits().to_be_bytes().to_vec();
    let material = PpcQ3SubmissionMaterialRecord {
        view,
        kind: PpcQ3SubmissionKind::TriMesh,
        primary: PPC_DATA_BASE,
        secondary: 0,
        shader: 0,
        illumination_type: PPC_Q3_ILLUMINATION_TYPE_PHONG,
        styles: Vec::new(),
        fog_style: None,
        attributes: vec![
            PpcQ3AttributeRecord {
                attribute_set,
                attribute_type: PPC_Q3_ATTRIBUTE_TYPE_DIFFUSE_COLOR,
                data: diffuse,
            },
            PpcQ3AttributeRecord {
                attribute_set,
                attribute_type: PPC_Q3_ATTRIBUTE_TYPE_AMBIENT_COEFFICIENT,
                data: ambient_coefficient,
            },
        ],
        shader_uv_transform: None,
        shader_boundary: None,
        texture_shader: None,
        mipmap_texture: None,
    };
    let lights = PpcQ3SubmissionLightRecord {
        view,
        kind: PpcQ3SubmissionKind::TriMesh,
        primary: PPC_DATA_BASE,
        secondary: 0,
        light_group: 0,
        lights: vec![
            PpcQ3LightRecord {
                light: ambient_light,
                light_type: PPC_Q3_LIGHT_TYPE_AMBIENT,
                data: PpcQ3LightData {
                    is_on: 1,
                    brightness: 1.0,
                    color: (1.0, 0.0, 0.0),
                },
                kind: PpcQ3LightKind::Ambient,
            },
            PpcQ3LightRecord {
                light: directional_light,
                light_type: PPC_Q3_LIGHT_TYPE_DIRECTIONAL,
                data: PpcQ3LightData {
                    is_on: 1,
                    brightness: 1.0,
                    color: (0.0, 1.0, 0.0),
                },
                kind: PpcQ3LightKind::Directional {
                    casts_shadows: 0,
                    direction: (0.0, 0.0, -1.0),
                },
            },
        ],
    };
    let sample = ppc_q3_software_material_sample(&[], &[], &material);
    let mut memory = PpcSectionMem::new();

    assert_eq!(
        ppc_q3_rgb555(ppc_q3_software_material_color(
            &mut memory,
            &sample,
            &lights,
            None,
            None,
            None,
            None,
            None,
            0.0,
            None,
        )),
        0x23e0
    );
}

#[test]
fn q3_software_renderer_uses_vertex_ambient_coefficients_for_ambient_lights() {
    let view = PPC_Q3_OBJECT_BASE;
    let attribute_set = PPC_Q3_OBJECT_BASE + PPC_Q3_OBJECT_STRIDE;
    let ambient_light = PPC_Q3_OBJECT_BASE + PPC_Q3_OBJECT_STRIDE * 2;
    let trimesh_data = PPC_DATA_BASE + 0x1000;
    let points_ptr = PPC_DATA_BASE + 0x1080;
    let triangles_ptr = PPC_DATA_BASE + 0x1100;
    let vertex_attrs_ptr = PPC_DATA_BASE + 0x1180;
    let ambient_coefficients_ptr = PPC_DATA_BASE + 0x1200;
    let front_base = PPC_HEAP_BASE + 0x1000;
    let pef = synthetic_pef_with_library_import(b"QuickDraw\xaa 3D", b"Q3TriMesh_Submit");
    let mut loaded = load_pef_application(&pef).unwrap();
    loaded.memory.add_region(trimesh_data, vec![0; 0x400]);
    loaded.memory.add_region(front_base, vec![0; 8 * 8 * 2]);
    loaded.gworlds = vec![PpcGWorldRecord {
        ui_theme: crate::ui_theme::UiThemeId::ClassicSystem7,
        port: PPC_MAIN_GWORLD,
        pixmap_handle: 0,
        pixmap: 0,
        base_addr: front_base,
        gdevice: PPC_MAIN_GDEVICE,
        width: 8,
        height: 8,
        depth: 16,
        row_bytes: 16,
        pixels_locked: false,
        pixels_no_purge: false,
    }];
    loaded
        .current_gworld
        .with_mut(|current_gworld| *current_gworld = PPC_MAIN_GWORLD);

    loaded
        .memory
        .write_u32_be(trimesh_data + PPC_Q3_TRIMESH_NUM_TRIANGLES_OFFSET, 1)
        .unwrap();
    loaded
        .memory
        .write_u32_be(
            trimesh_data + PPC_Q3_TRIMESH_TRIANGLES_OFFSET,
            triangles_ptr,
        )
        .unwrap();
    loaded
        .memory
        .write_u32_be(trimesh_data + PPC_Q3_TRIMESH_NUM_POINTS_OFFSET, 3)
        .unwrap();
    loaded
        .memory
        .write_u32_be(trimesh_data + PPC_Q3_TRIMESH_POINTS_OFFSET, points_ptr)
        .unwrap();
    loaded
        .memory
        .write_u32_be(
            trimesh_data + PPC_Q3_TRIMESH_NUM_VERTEX_ATTRIBUTE_TYPES_OFFSET,
            1,
        )
        .unwrap();
    loaded
        .memory
        .write_u32_be(
            trimesh_data + PPC_Q3_TRIMESH_VERTEX_ATTRIBUTE_TYPES_OFFSET,
            vertex_attrs_ptr,
        )
        .unwrap();
    loaded
        .memory
        .write_u32_be(vertex_attrs_ptr, PPC_Q3_ATTRIBUTE_TYPE_AMBIENT_COEFFICIENT)
        .unwrap();
    loaded
        .memory
        .write_u32_be(vertex_attrs_ptr + 4, ambient_coefficients_ptr)
        .unwrap();
    loaded.memory.write_u32_be(vertex_attrs_ptr + 8, 0).unwrap();
    ppc_write_q3_vector3d(&mut loaded.memory, points_ptr, (0.0, 0.0, 0.0)).unwrap();
    ppc_write_q3_vector3d(&mut loaded.memory, points_ptr + 12, (0.0, 0.9, 0.0)).unwrap();
    ppc_write_q3_vector3d(&mut loaded.memory, points_ptr + 24, (0.9, 0.0, 0.0)).unwrap();
    for index in 0..3 {
        ppc_write_f32_be(
            &mut loaded.memory,
            ambient_coefficients_ptr + index * 4,
            0.25,
        )
        .unwrap();
    }
    loaded.memory.write_u32_be(triangles_ptr, 0).unwrap();
    loaded.memory.write_u32_be(triangles_ptr + 4, 1).unwrap();
    loaded.memory.write_u32_be(triangles_ptr + 8, 2).unwrap();

    let mut diffuse = Vec::new();
    for value in [1.0f32, 1.0, 1.0] {
        diffuse.extend_from_slice(&value.to_bits().to_be_bytes());
    }
    let ambient_zero = 0.0f32.to_bits().to_be_bytes().to_vec();
    loaded.q3_views.push(PpcQ3ViewStateRecord::new(view));
    loaded.q3_submissions.push(PpcQ3SubmissionRecord {
        view,
        kind: PpcQ3SubmissionKind::TriMesh,
        primary: trimesh_data,
        secondary: 0,
    });
    loaded
        .q3_submission_transforms
        .push(PpcQ3SubmissionTransformRecord {
            view,
            kind: PpcQ3SubmissionKind::TriMesh,
            primary: trimesh_data,
            secondary: 0,
            local_to_world: ppc_q3_matrix4x4_identity(),
        });
    loaded
        .q3_submission_materials
        .push(PpcQ3SubmissionMaterialRecord {
            view,
            kind: PpcQ3SubmissionKind::TriMesh,
            primary: trimesh_data,
            secondary: 0,
            shader: 0,
            illumination_type: PPC_Q3_ILLUMINATION_TYPE_PHONG,
            styles: Vec::new(),
            fog_style: None,
            attributes: vec![
                PpcQ3AttributeRecord {
                    attribute_set,
                    attribute_type: PPC_Q3_ATTRIBUTE_TYPE_DIFFUSE_COLOR,
                    data: diffuse,
                },
                PpcQ3AttributeRecord {
                    attribute_set,
                    attribute_type: PPC_Q3_ATTRIBUTE_TYPE_AMBIENT_COEFFICIENT,
                    data: ambient_zero,
                },
            ],
            shader_uv_transform: None,
            shader_boundary: None,
            texture_shader: None,
            mipmap_texture: None,
        });
    loaded
        .q3_submission_lights
        .push(PpcQ3SubmissionLightRecord {
            view,
            kind: PpcQ3SubmissionKind::TriMesh,
            primary: trimesh_data,
            secondary: 0,
            light_group: 0,
            lights: vec![PpcQ3LightRecord {
                light: ambient_light,
                light_type: PPC_Q3_LIGHT_TYPE_AMBIENT,
                data: PpcQ3LightData {
                    is_on: 1,
                    brightness: 1.0,
                    color: (1.0, 0.0, 0.0),
                },
                kind: PpcQ3LightKind::Ambient,
            }],
        });

    let stats = loaded.render_q3_scene_commands_to_front_buffer();

    assert_eq!(stats.commands, 1);
    assert_eq!(stats.triangles, 1);
    assert!(stats.pixels > 0);
    assert_eq!(
        loaded.memory.read_u16_be(front_base + 4 * 16 + 4 * 2),
        Some(0x2000)
    );
}

#[test]
fn q3_software_renderer_uses_triangle_ambient_coefficients_for_ambient_lights() {
    let view = PPC_Q3_OBJECT_BASE;
    let attribute_set = PPC_Q3_OBJECT_BASE + PPC_Q3_OBJECT_STRIDE;
    let ambient_light = PPC_Q3_OBJECT_BASE + PPC_Q3_OBJECT_STRIDE * 2;
    let trimesh_data = PPC_DATA_BASE + 0x1000;
    let points_ptr = PPC_DATA_BASE + 0x1080;
    let triangles_ptr = PPC_DATA_BASE + 0x1100;
    let triangle_attrs_ptr = PPC_DATA_BASE + 0x1180;
    let ambient_coefficient_ptr = PPC_DATA_BASE + 0x1200;
    let front_base = PPC_HEAP_BASE + 0x1000;
    let pef = synthetic_pef_with_library_import(b"QuickDraw\xaa 3D", b"Q3TriMesh_Submit");
    let mut loaded = load_pef_application(&pef).unwrap();
    loaded.memory.add_region(trimesh_data, vec![0; 0x400]);
    loaded.memory.add_region(front_base, vec![0; 8 * 8 * 2]);
    loaded.gworlds = vec![PpcGWorldRecord {
        ui_theme: crate::ui_theme::UiThemeId::ClassicSystem7,
        port: PPC_MAIN_GWORLD,
        pixmap_handle: 0,
        pixmap: 0,
        base_addr: front_base,
        gdevice: PPC_MAIN_GDEVICE,
        width: 8,
        height: 8,
        depth: 16,
        row_bytes: 16,
        pixels_locked: false,
        pixels_no_purge: false,
    }];
    loaded
        .current_gworld
        .with_mut(|current_gworld| *current_gworld = PPC_MAIN_GWORLD);

    loaded
        .memory
        .write_u32_be(trimesh_data + PPC_Q3_TRIMESH_NUM_TRIANGLES_OFFSET, 1)
        .unwrap();
    loaded
        .memory
        .write_u32_be(
            trimesh_data + PPC_Q3_TRIMESH_TRIANGLES_OFFSET,
            triangles_ptr,
        )
        .unwrap();
    loaded
        .memory
        .write_u32_be(
            trimesh_data + PPC_Q3_TRIMESH_NUM_TRIANGLE_ATTRIBUTE_TYPES_OFFSET,
            1,
        )
        .unwrap();
    loaded
        .memory
        .write_u32_be(
            trimesh_data + PPC_Q3_TRIMESH_TRIANGLE_ATTRIBUTE_TYPES_OFFSET,
            triangle_attrs_ptr,
        )
        .unwrap();
    loaded
        .memory
        .write_u32_be(
            triangle_attrs_ptr,
            PPC_Q3_ATTRIBUTE_TYPE_AMBIENT_COEFFICIENT,
        )
        .unwrap();
    loaded
        .memory
        .write_u32_be(triangle_attrs_ptr + 4, ambient_coefficient_ptr)
        .unwrap();
    loaded
        .memory
        .write_u32_be(triangle_attrs_ptr + 8, 0)
        .unwrap();
    loaded
        .memory
        .write_u32_be(trimesh_data + PPC_Q3_TRIMESH_NUM_POINTS_OFFSET, 3)
        .unwrap();
    loaded
        .memory
        .write_u32_be(trimesh_data + PPC_Q3_TRIMESH_POINTS_OFFSET, points_ptr)
        .unwrap();
    ppc_write_q3_vector3d(&mut loaded.memory, points_ptr, (0.0, 0.0, 0.0)).unwrap();
    ppc_write_q3_vector3d(&mut loaded.memory, points_ptr + 12, (0.0, 0.9, 0.0)).unwrap();
    ppc_write_q3_vector3d(&mut loaded.memory, points_ptr + 24, (0.9, 0.0, 0.0)).unwrap();
    ppc_write_f32_be(&mut loaded.memory, ambient_coefficient_ptr, 0.25).unwrap();
    loaded.memory.write_u32_be(triangles_ptr, 0).unwrap();
    loaded.memory.write_u32_be(triangles_ptr + 4, 1).unwrap();
    loaded.memory.write_u32_be(triangles_ptr + 8, 2).unwrap();

    let mut diffuse = Vec::new();
    for value in [1.0f32, 1.0, 1.0] {
        diffuse.extend_from_slice(&value.to_bits().to_be_bytes());
    }
    let ambient_zero = 0.0f32.to_bits().to_be_bytes().to_vec();
    loaded.q3_views.push(PpcQ3ViewStateRecord::new(view));
    loaded.q3_submissions.push(PpcQ3SubmissionRecord {
        view,
        kind: PpcQ3SubmissionKind::TriMesh,
        primary: trimesh_data,
        secondary: 0,
    });
    loaded
        .q3_submission_transforms
        .push(PpcQ3SubmissionTransformRecord {
            view,
            kind: PpcQ3SubmissionKind::TriMesh,
            primary: trimesh_data,
            secondary: 0,
            local_to_world: ppc_q3_matrix4x4_identity(),
        });
    loaded
        .q3_submission_materials
        .push(PpcQ3SubmissionMaterialRecord {
            view,
            kind: PpcQ3SubmissionKind::TriMesh,
            primary: trimesh_data,
            secondary: 0,
            shader: 0,
            illumination_type: PPC_Q3_ILLUMINATION_TYPE_PHONG,
            styles: Vec::new(),
            fog_style: None,
            attributes: vec![
                PpcQ3AttributeRecord {
                    attribute_set,
                    attribute_type: PPC_Q3_ATTRIBUTE_TYPE_DIFFUSE_COLOR,
                    data: diffuse,
                },
                PpcQ3AttributeRecord {
                    attribute_set,
                    attribute_type: PPC_Q3_ATTRIBUTE_TYPE_AMBIENT_COEFFICIENT,
                    data: ambient_zero,
                },
            ],
            shader_uv_transform: None,
            shader_boundary: None,
            texture_shader: None,
            mipmap_texture: None,
        });
    loaded
        .q3_submission_lights
        .push(PpcQ3SubmissionLightRecord {
            view,
            kind: PpcQ3SubmissionKind::TriMesh,
            primary: trimesh_data,
            secondary: 0,
            light_group: 0,
            lights: vec![PpcQ3LightRecord {
                light: ambient_light,
                light_type: PPC_Q3_LIGHT_TYPE_AMBIENT,
                data: PpcQ3LightData {
                    is_on: 1,
                    brightness: 1.0,
                    color: (1.0, 0.0, 0.0),
                },
                kind: PpcQ3LightKind::Ambient,
            }],
        });

    let stats = loaded.render_q3_scene_commands_to_front_buffer();

    assert_eq!(stats.commands, 1);
    assert_eq!(stats.triangles, 1);
    assert!(stats.pixels > 0);
    assert_eq!(
        loaded.memory.read_u16_be(front_base + 4 * 16 + 4 * 2),
        Some(0x2000)
    );
}

#[test]
fn q3_software_renderer_honors_illumination_shader_model() {
    let view = PPC_Q3_OBJECT_BASE;
    let light = PPC_Q3_OBJECT_BASE + PPC_Q3_OBJECT_STRIDE;
    let lights = PpcQ3SubmissionLightRecord {
        view,
        kind: PpcQ3SubmissionKind::TriMesh,
        primary: PPC_DATA_BASE,
        secondary: 0,
        light_group: 0,
        lights: vec![PpcQ3LightRecord {
            light,
            light_type: PPC_Q3_LIGHT_TYPE_DIRECTIONAL,
            data: PpcQ3LightData {
                is_on: 1,
                brightness: 1.0,
                color: (1.0, 1.0, 1.0),
            },
            kind: PpcQ3LightKind::Directional {
                casts_shadows: 0,
                direction: (0.0, 0.0, -1.0),
            },
        }],
    };
    let mut memory = PpcSectionMem::new();
    let sample = PpcQ3SoftwareMaterialSample {
        diffuse: (0.25, 0.25, 0.25),
        ambient_coefficient: 0.0,
        specular_color: (0.5, 0.5, 0.5),
        specular_control: 1.0,
        highlight_state: true,
        transparency: (1.0, 1.0, 1.0),
        illumination_type: PPC_Q3_ILLUMINATION_TYPE_NULL,
        texture: None,
        uv_transform: None,
        shader_boundary: None,
        fog_style: None,
    };

    let null_color = ppc_q3_software_material_color(
        &mut memory,
        &sample,
        &lights,
        None,
        None,
        Some((0.0, 0.0, 1.0)),
        None,
        None,
        0.0,
        None,
    );
    let lambert_color = ppc_q3_software_material_color(
        &mut memory,
        &PpcQ3SoftwareMaterialSample {
            illumination_type: PPC_Q3_ILLUMINATION_TYPE_LAMBERT,
            ..sample
        },
        &lights,
        None,
        None,
        Some((0.0, 0.0, 1.0)),
        None,
        None,
        0.0,
        None,
    );
    let phong_color = ppc_q3_software_material_color(
        &mut memory,
        &PpcQ3SoftwareMaterialSample {
            illumination_type: PPC_Q3_ILLUMINATION_TYPE_PHONG,
            ..sample
        },
        &lights,
        None,
        None,
        Some((0.0, 0.0, 1.0)),
        None,
        None,
        0.0,
        None,
    );

    assert_eq!(null_color, (0.25, 0.25, 0.25));
    assert_eq!(lambert_color, (0.25, 0.25, 0.25));
    assert_eq!(phong_color, (0.75, 0.75, 0.75));
}

#[test]
fn q3_software_renderer_honors_highlight_state_for_phong_specular() {
    fn color_attribute(
        attribute_set: u32,
        attribute_type: u32,
        rgb: [f32; 3],
    ) -> PpcQ3AttributeRecord {
        let mut data = Vec::new();
        for value in rgb {
            data.extend_from_slice(&value.to_bits().to_be_bytes());
        }
        PpcQ3AttributeRecord {
            attribute_set,
            attribute_type,
            data,
        }
    }

    fn scalar_attribute(
        attribute_set: u32,
        attribute_type: u32,
        value: f32,
    ) -> PpcQ3AttributeRecord {
        PpcQ3AttributeRecord {
            attribute_set,
            attribute_type,
            data: value.to_bits().to_be_bytes().to_vec(),
        }
    }

    fn switch_attribute(attribute_set: u32, value: u32) -> PpcQ3AttributeRecord {
        PpcQ3AttributeRecord {
            attribute_set,
            attribute_type: PPC_Q3_ATTRIBUTE_TYPE_HIGHLIGHT_STATE,
            data: value.to_be_bytes().to_vec(),
        }
    }

    let view = PPC_Q3_OBJECT_BASE;
    let attribute_set = PPC_Q3_OBJECT_BASE + PPC_Q3_OBJECT_STRIDE;
    let light = PPC_Q3_OBJECT_BASE + PPC_Q3_OBJECT_STRIDE * 2;
    let lights = PpcQ3SubmissionLightRecord {
        view,
        kind: PpcQ3SubmissionKind::TriMesh,
        primary: PPC_DATA_BASE,
        secondary: 0,
        light_group: 0,
        lights: vec![PpcQ3LightRecord {
            light,
            light_type: PPC_Q3_LIGHT_TYPE_DIRECTIONAL,
            data: PpcQ3LightData {
                is_on: 1,
                brightness: 1.0,
                color: (1.0, 1.0, 1.0),
            },
            kind: PpcQ3LightKind::Directional {
                casts_shadows: 0,
                direction: (0.0, 0.0, -1.0),
            },
        }],
    };
    let material_with_highlight_state = |highlight_state| PpcQ3SubmissionMaterialRecord {
        view,
        kind: PpcQ3SubmissionKind::TriMesh,
        primary: PPC_DATA_BASE,
        secondary: 0,
        shader: 0,
        illumination_type: PPC_Q3_ILLUMINATION_TYPE_PHONG,
        styles: Vec::new(),
        fog_style: None,
        attributes: vec![
            color_attribute(
                attribute_set,
                PPC_Q3_ATTRIBUTE_TYPE_DIFFUSE_COLOR,
                [0.25, 0.25, 0.25],
            ),
            color_attribute(
                attribute_set,
                PPC_Q3_ATTRIBUTE_TYPE_SPECULAR_COLOR,
                [0.5, 0.5, 0.5],
            ),
            scalar_attribute(attribute_set, PPC_Q3_ATTRIBUTE_TYPE_SPECULAR_CONTROL, 1.0),
            switch_attribute(attribute_set, highlight_state),
        ],
        shader_uv_transform: None,
        shader_boundary: None,
        texture_shader: None,
        mipmap_texture: None,
    };
    let mut memory = PpcSectionMem::new();
    let highlighted =
        ppc_q3_software_material_sample(&[], &[], &material_with_highlight_state(1));
    let muted = ppc_q3_software_material_sample(&[], &[], &material_with_highlight_state(0));

    assert_eq!(
        ppc_q3_software_material_color(
            &mut memory,
            &highlighted,
            &lights,
            None,
            None,
            Some((0.0, 0.0, 1.0)),
            None,
            None,
            0.0,
            None,
        ),
        (0.75, 0.75, 0.75)
    );
    assert_eq!(
        ppc_q3_software_material_color(
            &mut memory,
            &muted,
            &lights,
            None,
            None,
            Some((0.0, 0.0, 1.0)),
            None,
            None,
            0.0,
            None,
        ),
        (0.25, 0.25, 0.25)
    );
}

fn q3_software_test_color_attribute(
    attribute_set: u32,
    attribute_type: u32,
    rgb: [f32; 3],
) -> PpcQ3AttributeRecord {
    let mut data = Vec::new();
    for value in rgb {
        data.extend_from_slice(&value.to_bits().to_be_bytes());
    }
    PpcQ3AttributeRecord {
        attribute_set,
        attribute_type,
        data,
    }
}

fn q3_software_test_scalar_attribute(
    attribute_set: u32,
    attribute_type: u32,
    value: f32,
) -> PpcQ3AttributeRecord {
    PpcQ3AttributeRecord {
        attribute_set,
        attribute_type,
        data: value.to_bits().to_be_bytes().to_vec(),
    }
}

fn q3_software_test_write_attribute_entry(
    memory: &mut PpcSectionMem,
    attrs_ptr: u32,
    index: usize,
    attribute_type: u32,
    data_ptr: u32,
) {
    let entry_ptr = attrs_ptr + index as u32 * PPC_Q3_TRIMESH_ATTRIBUTE_DATA_SIZE;
    memory.write_u32_be(entry_ptr, attribute_type).unwrap();
    memory.write_u32_be(entry_ptr + 4, data_ptr).unwrap();
    memory.write_u32_be(entry_ptr + 8, 0).unwrap();
}

fn q3_software_test_front_gworld(front_base: u32) -> PpcGWorldRecord {
    PpcGWorldRecord {
        ui_theme: crate::ui_theme::UiThemeId::ClassicSystem7,
        port: PPC_MAIN_GWORLD,
        pixmap_handle: 0,
        pixmap: 0,
        base_addr: front_base,
        gdevice: PPC_MAIN_GDEVICE,
        width: 8,
        height: 8,
        depth: 16,
        row_bytes: 16,
        pixels_locked: false,
        pixels_no_purge: false,
    }
}

fn q3_software_test_push_phong_specular_submission(
    loaded: &mut PpcLoadedApp,
    view: u32,
    trimesh_data: u32,
    attribute_set: u32,
    styles: Vec<PpcQ3StyleRecord>,
    directional_light: u32,
) {
    loaded.q3_views.push(PpcQ3ViewStateRecord::new(view));
    loaded.q3_submissions.push(PpcQ3SubmissionRecord {
        view,
        kind: PpcQ3SubmissionKind::TriMesh,
        primary: trimesh_data,
        secondary: 0,
    });
    loaded
        .q3_submission_transforms
        .push(PpcQ3SubmissionTransformRecord {
            view,
            kind: PpcQ3SubmissionKind::TriMesh,
            primary: trimesh_data,
            secondary: 0,
            local_to_world: ppc_q3_matrix4x4_identity(),
        });
    loaded
        .q3_submission_materials
        .push(PpcQ3SubmissionMaterialRecord {
            view,
            kind: PpcQ3SubmissionKind::TriMesh,
            primary: trimesh_data,
            secondary: 0,
            shader: 0,
            illumination_type: PPC_Q3_ILLUMINATION_TYPE_PHONG,
            styles,
            fog_style: None,
            attributes: vec![
                q3_software_test_color_attribute(
                    attribute_set,
                    PPC_Q3_ATTRIBUTE_TYPE_DIFFUSE_COLOR,
                    [0.0, 0.0, 0.0],
                ),
                q3_software_test_color_attribute(
                    attribute_set,
                    PPC_Q3_ATTRIBUTE_TYPE_SPECULAR_COLOR,
                    [1.0, 0.0, 0.0],
                ),
                q3_software_test_scalar_attribute(
                    attribute_set,
                    PPC_Q3_ATTRIBUTE_TYPE_SPECULAR_CONTROL,
                    1.0,
                ),
            ],
            shader_uv_transform: None,
            shader_boundary: None,
            texture_shader: None,
            mipmap_texture: None,
        });
    loaded
        .q3_submission_lights
        .push(PpcQ3SubmissionLightRecord {
            view,
            kind: PpcQ3SubmissionKind::TriMesh,
            primary: trimesh_data,
            secondary: 0,
            light_group: 0,
            lights: vec![PpcQ3LightRecord {
                light: directional_light,
                light_type: PPC_Q3_LIGHT_TYPE_DIRECTIONAL,
                data: PpcQ3LightData {
                    is_on: 1,
                    brightness: 1.0,
                    color: (1.0, 1.0, 1.0),
                },
                kind: PpcQ3LightKind::Directional {
                    casts_shadows: 0,
                    direction: (0.0, 0.0, -1.0),
                },
            }],
        });
}

#[test]
fn q3_software_renderer_honors_vertex_highlight_states_for_phong_specular() {
    let view = PPC_Q3_OBJECT_BASE;
    let attribute_set = PPC_Q3_OBJECT_BASE + PPC_Q3_OBJECT_STRIDE;
    let directional_light = PPC_Q3_OBJECT_BASE + PPC_Q3_OBJECT_STRIDE * 2;
    let trimesh_data = PPC_DATA_BASE + 0x1000;
    let points_ptr = PPC_DATA_BASE + 0x1080;
    let triangles_ptr = PPC_DATA_BASE + 0x1100;
    let vertex_attrs_ptr = PPC_DATA_BASE + 0x1180;
    let normals_ptr = PPC_DATA_BASE + 0x1200;
    let highlight_states_ptr = PPC_DATA_BASE + 0x1280;
    let front_base = PPC_HEAP_BASE + 0x1000;
    let pef = synthetic_pef_with_library_import(b"QuickDraw\xaa 3D", b"Q3TriMesh_Submit");
    let mut loaded = load_pef_application(&pef).unwrap();
    let mut front_buffer_bytes = Vec::new();
    for _ in 0..64 {
        front_buffer_bytes.extend_from_slice(&0x7c00u16.to_be_bytes());
    }
    loaded.memory.add_region(trimesh_data, vec![0; 0x500]);
    loaded.memory.add_region(front_base, front_buffer_bytes);
    loaded.gworlds = vec![q3_software_test_front_gworld(front_base)];
    loaded
        .current_gworld
        .with_mut(|current_gworld| *current_gworld = PPC_MAIN_GWORLD);

    loaded
        .memory
        .write_u32_be(trimesh_data + PPC_Q3_TRIMESH_NUM_TRIANGLES_OFFSET, 1)
        .unwrap();
    loaded
        .memory
        .write_u32_be(
            trimesh_data + PPC_Q3_TRIMESH_TRIANGLES_OFFSET,
            triangles_ptr,
        )
        .unwrap();
    loaded
        .memory
        .write_u32_be(trimesh_data + PPC_Q3_TRIMESH_NUM_POINTS_OFFSET, 3)
        .unwrap();
    loaded
        .memory
        .write_u32_be(trimesh_data + PPC_Q3_TRIMESH_POINTS_OFFSET, points_ptr)
        .unwrap();
    loaded
        .memory
        .write_u32_be(
            trimesh_data + PPC_Q3_TRIMESH_NUM_VERTEX_ATTRIBUTE_TYPES_OFFSET,
            2,
        )
        .unwrap();
    loaded
        .memory
        .write_u32_be(
            trimesh_data + PPC_Q3_TRIMESH_VERTEX_ATTRIBUTE_TYPES_OFFSET,
            vertex_attrs_ptr,
        )
        .unwrap();
    q3_software_test_write_attribute_entry(
        &mut loaded.memory,
        vertex_attrs_ptr,
        0,
        PPC_Q3_ATTRIBUTE_TYPE_NORMAL,
        normals_ptr,
    );
    q3_software_test_write_attribute_entry(
        &mut loaded.memory,
        vertex_attrs_ptr,
        1,
        PPC_Q3_ATTRIBUTE_TYPE_HIGHLIGHT_STATE,
        highlight_states_ptr,
    );
    ppc_write_q3_vector3d(&mut loaded.memory, points_ptr, (0.0, 0.0, 0.0)).unwrap();
    ppc_write_q3_vector3d(&mut loaded.memory, points_ptr + 12, (0.0, 0.9, 0.0)).unwrap();
    ppc_write_q3_vector3d(&mut loaded.memory, points_ptr + 24, (0.9, 0.0, 0.0)).unwrap();
    for index in 0..3 {
        ppc_write_q3_vector3d(
            &mut loaded.memory,
            normals_ptr + index * PPC_Q3_POINT3D_SIZE,
            (0.0, 0.0, 1.0),
        )
        .unwrap();
        loaded
            .memory
            .write_u32_be(highlight_states_ptr + index * 4, 0)
            .unwrap();
    }
    loaded.memory.write_u32_be(triangles_ptr, 0).unwrap();
    loaded.memory.write_u32_be(triangles_ptr + 4, 1).unwrap();
    loaded.memory.write_u32_be(triangles_ptr + 8, 2).unwrap();

    q3_software_test_push_phong_specular_submission(
        &mut loaded,
        view,
        trimesh_data,
        attribute_set,
        Vec::new(),
        directional_light,
    );

    let stats = loaded.render_q3_scene_commands_to_front_buffer();

    assert_eq!(stats.commands, 1);
    assert_eq!(stats.triangles, 1);
    assert!(stats.pixels > 0);
    assert_eq!(
        loaded.memory.read_u16_be(front_base + 4 * 16 + 4 * 2),
        Some(0)
    );
}

#[test]
fn q3_software_renderer_honors_triangle_highlight_states_for_phong_specular() {
    let view = PPC_Q3_OBJECT_BASE;
    let attribute_set = PPC_Q3_OBJECT_BASE + PPC_Q3_OBJECT_STRIDE;
    let directional_light = PPC_Q3_OBJECT_BASE + PPC_Q3_OBJECT_STRIDE * 2;
    let trimesh_data = PPC_DATA_BASE + 0x1000;
    let points_ptr = PPC_DATA_BASE + 0x1080;
    let triangles_ptr = PPC_DATA_BASE + 0x1100;
    let triangle_attrs_ptr = PPC_DATA_BASE + 0x1180;
    let highlight_states_ptr = PPC_DATA_BASE + 0x1200;
    let vertex_attrs_ptr = PPC_DATA_BASE + 0x1280;
    let normals_ptr = PPC_DATA_BASE + 0x1300;
    let front_base = PPC_HEAP_BASE + 0x1000;
    let pef = synthetic_pef_with_library_import(b"QuickDraw\xaa 3D", b"Q3TriMesh_Submit");
    let mut loaded = load_pef_application(&pef).unwrap();
    let mut front_buffer_bytes = Vec::new();
    for _ in 0..64 {
        front_buffer_bytes.extend_from_slice(&0x7c00u16.to_be_bytes());
    }
    loaded.memory.add_region(trimesh_data, vec![0; 0x500]);
    loaded.memory.add_region(front_base, front_buffer_bytes);
    loaded.gworlds = vec![q3_software_test_front_gworld(front_base)];
    loaded
        .current_gworld
        .with_mut(|current_gworld| *current_gworld = PPC_MAIN_GWORLD);

    loaded
        .memory
        .write_u32_be(trimesh_data + PPC_Q3_TRIMESH_NUM_TRIANGLES_OFFSET, 1)
        .unwrap();
    loaded
        .memory
        .write_u32_be(
            trimesh_data + PPC_Q3_TRIMESH_TRIANGLES_OFFSET,
            triangles_ptr,
        )
        .unwrap();
    loaded
        .memory
        .write_u32_be(
            trimesh_data + PPC_Q3_TRIMESH_NUM_TRIANGLE_ATTRIBUTE_TYPES_OFFSET,
            1,
        )
        .unwrap();
    loaded
        .memory
        .write_u32_be(
            trimesh_data + PPC_Q3_TRIMESH_TRIANGLE_ATTRIBUTE_TYPES_OFFSET,
            triangle_attrs_ptr,
        )
        .unwrap();
    loaded
        .memory
        .write_u32_be(trimesh_data + PPC_Q3_TRIMESH_NUM_POINTS_OFFSET, 3)
        .unwrap();
    loaded
        .memory
        .write_u32_be(trimesh_data + PPC_Q3_TRIMESH_POINTS_OFFSET, points_ptr)
        .unwrap();
    loaded
        .memory
        .write_u32_be(
            trimesh_data + PPC_Q3_TRIMESH_NUM_VERTEX_ATTRIBUTE_TYPES_OFFSET,
            1,
        )
        .unwrap();
    loaded
        .memory
        .write_u32_be(
            trimesh_data + PPC_Q3_TRIMESH_VERTEX_ATTRIBUTE_TYPES_OFFSET,
            vertex_attrs_ptr,
        )
        .unwrap();
    q3_software_test_write_attribute_entry(
        &mut loaded.memory,
        triangle_attrs_ptr,
        0,
        PPC_Q3_ATTRIBUTE_TYPE_HIGHLIGHT_STATE,
        highlight_states_ptr,
    );
    q3_software_test_write_attribute_entry(
        &mut loaded.memory,
        vertex_attrs_ptr,
        0,
        PPC_Q3_ATTRIBUTE_TYPE_NORMAL,
        normals_ptr,
    );
    ppc_write_q3_vector3d(&mut loaded.memory, points_ptr, (0.0, 0.0, 0.0)).unwrap();
    ppc_write_q3_vector3d(&mut loaded.memory, points_ptr + 12, (0.0, 0.9, 0.0)).unwrap();
    ppc_write_q3_vector3d(&mut loaded.memory, points_ptr + 24, (0.9, 0.0, 0.0)).unwrap();
    for index in 0..3 {
        ppc_write_q3_vector3d(
            &mut loaded.memory,
            normals_ptr + index * PPC_Q3_POINT3D_SIZE,
            (0.0, 0.0, 1.0),
        )
        .unwrap();
    }
    loaded.memory.write_u32_be(highlight_states_ptr, 0).unwrap();
    loaded.memory.write_u32_be(triangles_ptr, 0).unwrap();
    loaded.memory.write_u32_be(triangles_ptr + 4, 1).unwrap();
    loaded.memory.write_u32_be(triangles_ptr + 8, 2).unwrap();

    q3_software_test_push_phong_specular_submission(
        &mut loaded,
        view,
        trimesh_data,
        attribute_set,
        Vec::new(),
        directional_light,
    );

    let stats = loaded.render_q3_scene_commands_to_front_buffer();

    assert_eq!(stats.commands, 1);
    assert_eq!(stats.triangles, 1);
    assert!(stats.pixels > 0);
    assert_eq!(
        loaded.memory.read_u16_be(front_base + 4 * 16 + 4 * 2),
        Some(0)
    );
}

#[test]
fn q3_software_renderer_honors_edge_highlight_states_for_phong_specular() {
    let view = PPC_Q3_OBJECT_BASE;
    let attribute_set = PPC_Q3_OBJECT_BASE + PPC_Q3_OBJECT_STRIDE;
    let style = PPC_Q3_OBJECT_BASE + PPC_Q3_OBJECT_STRIDE * 2;
    let directional_light = PPC_Q3_OBJECT_BASE + PPC_Q3_OBJECT_STRIDE * 3;
    let trimesh_data = PPC_DATA_BASE + 0x1000;
    let points_ptr = PPC_DATA_BASE + 0x1080;
    let triangles_ptr = PPC_DATA_BASE + 0x1100;
    let edges_ptr = PPC_DATA_BASE + 0x1180;
    let edge_attrs_ptr = PPC_DATA_BASE + 0x1200;
    let edge_specular_colors_ptr = PPC_DATA_BASE + 0x1280;
    let edge_specular_controls_ptr = PPC_DATA_BASE + 0x1300;
    let edge_highlight_states_ptr = PPC_DATA_BASE + 0x1380;
    let vertex_attrs_ptr = PPC_DATA_BASE + 0x1400;
    let normals_ptr = PPC_DATA_BASE + 0x1480;
    let front_base = PPC_HEAP_BASE + 0x1000;
    let pef = synthetic_pef_with_library_import(b"QuickDraw\xaa 3D", b"Q3TriMesh_Submit");
    let mut loaded = load_pef_application(&pef).unwrap();
    let mut front_buffer_bytes = Vec::new();
    for _ in 0..64 {
        front_buffer_bytes.extend_from_slice(&0x7c00u16.to_be_bytes());
    }
    loaded.memory.add_region(trimesh_data, vec![0; 0x600]);
    loaded.memory.add_region(front_base, front_buffer_bytes);
    loaded.gworlds = vec![q3_software_test_front_gworld(front_base)];
    loaded
        .current_gworld
        .with_mut(|current_gworld| *current_gworld = PPC_MAIN_GWORLD);

    loaded
        .memory
        .write_u32_be(trimesh_data + PPC_Q3_TRIMESH_NUM_TRIANGLES_OFFSET, 1)
        .unwrap();
    loaded
        .memory
        .write_u32_be(
            trimesh_data + PPC_Q3_TRIMESH_TRIANGLES_OFFSET,
            triangles_ptr,
        )
        .unwrap();
    loaded
        .memory
        .write_u32_be(trimesh_data + PPC_Q3_TRIMESH_NUM_EDGES_OFFSET, 3)
        .unwrap();
    loaded
        .memory
        .write_u32_be(trimesh_data + PPC_Q3_TRIMESH_EDGES_OFFSET, edges_ptr)
        .unwrap();
    loaded
        .memory
        .write_u32_be(
            trimesh_data + PPC_Q3_TRIMESH_NUM_EDGE_ATTRIBUTE_TYPES_OFFSET,
            3,
        )
        .unwrap();
    loaded
        .memory
        .write_u32_be(
            trimesh_data + PPC_Q3_TRIMESH_EDGE_ATTRIBUTE_TYPES_OFFSET,
            edge_attrs_ptr,
        )
        .unwrap();
    loaded
        .memory
        .write_u32_be(
            trimesh_data + PPC_Q3_TRIMESH_NUM_VERTEX_ATTRIBUTE_TYPES_OFFSET,
            1,
        )
        .unwrap();
    loaded
        .memory
        .write_u32_be(
            trimesh_data + PPC_Q3_TRIMESH_VERTEX_ATTRIBUTE_TYPES_OFFSET,
            vertex_attrs_ptr,
        )
        .unwrap();
    loaded
        .memory
        .write_u32_be(trimesh_data + PPC_Q3_TRIMESH_NUM_POINTS_OFFSET, 3)
        .unwrap();
    loaded
        .memory
        .write_u32_be(trimesh_data + PPC_Q3_TRIMESH_POINTS_OFFSET, points_ptr)
        .unwrap();
    for (index, point) in [(-0.8, -0.8, 0.0), (-0.8, 0.8, 0.0), (0.8, -0.8, 0.0)]
        .into_iter()
        .enumerate()
    {
        ppc_write_q3_vector3d(
            &mut loaded.memory,
            points_ptr + index as u32 * PPC_Q3_POINT3D_SIZE,
            point,
        )
        .unwrap();
        ppc_write_q3_vector3d(
            &mut loaded.memory,
            normals_ptr + index as u32 * PPC_Q3_POINT3D_SIZE,
            (0.0, 0.0, 1.0),
        )
        .unwrap();
    }
    for (offset, index) in [0u32, 1, 2].into_iter().enumerate() {
        loaded
            .memory
            .write_u32_be(triangles_ptr + offset as u32 * 4, index)
            .unwrap();
    }
    for (index, (a, b)) in [(0u32, 1u32), (1, 2), (2, 0)].into_iter().enumerate() {
        let edge_ptr = edges_ptr + index as u32 * PPC_Q3_TRIMESH_EDGE_DATA_SIZE;
        loaded.memory.write_u32_be(edge_ptr, a).unwrap();
        loaded.memory.write_u32_be(edge_ptr + 4, b).unwrap();
    }
    for (index, (attribute_type, data_ptr)) in [
        (
            PPC_Q3_ATTRIBUTE_TYPE_SPECULAR_COLOR,
            edge_specular_colors_ptr,
        ),
        (
            PPC_Q3_ATTRIBUTE_TYPE_SPECULAR_CONTROL,
            edge_specular_controls_ptr,
        ),
        (
            PPC_Q3_ATTRIBUTE_TYPE_HIGHLIGHT_STATE,
            edge_highlight_states_ptr,
        ),
    ]
    .into_iter()
    .enumerate()
    {
        q3_software_test_write_attribute_entry(
            &mut loaded.memory,
            edge_attrs_ptr,
            index,
            attribute_type,
            data_ptr,
        );
    }
    q3_software_test_write_attribute_entry(
        &mut loaded.memory,
        vertex_attrs_ptr,
        0,
        PPC_Q3_ATTRIBUTE_TYPE_NORMAL,
        normals_ptr,
    );
    for index in 0..3 {
        ppc_write_q3_color_rgb(
            &mut loaded.memory,
            edge_specular_colors_ptr + index * PPC_Q3_POINT3D_SIZE,
            (0.0, 0.0, 1.0),
        )
        .unwrap();
        ppc_write_f32_be(
            &mut loaded.memory,
            edge_specular_controls_ptr + index * 4,
            1.0,
        )
        .unwrap();
    }
    for (index, highlight_state) in [0u32, 1, 1].into_iter().enumerate() {
        loaded
            .memory
            .write_u32_be(
                edge_highlight_states_ptr + index as u32 * 4,
                highlight_state,
            )
            .unwrap();
    }

    q3_software_test_push_phong_specular_submission(
        &mut loaded,
        view,
        trimesh_data,
        attribute_set,
        vec![PpcQ3StyleRecord {
            style,
            kind: PpcQ3StyleKind::Fill,
            value: PPC_Q3_FILL_STYLE_EDGES,
        }],
        directional_light,
    );

    let stats = loaded.render_q3_scene_commands_to_front_buffer();

    assert_eq!(stats.commands, 1);
    assert_eq!(stats.triangles, 1);
    assert!(stats.pixels > 0);
    assert_eq!(
        loaded.memory.read_u16_be(front_base + 3 * 16 + 2),
        Some(0)
    );
    assert_eq!(
        loaded.memory.read_u16_be(front_base + 6 * 16 + 3 * 2),
        Some(0x001f)
    );
}

#[test]
fn q3_software_renderer_uses_camera_view_direction_for_specular_lights() {
    let light = PpcQ3LightRecord {
        light: PPC_Q3_OBJECT_BASE,
        light_type: PPC_Q3_LIGHT_TYPE_DIRECTIONAL,
        data: PpcQ3LightData {
            is_on: 1,
            brightness: 1.0,
            color: (1.0, 1.0, 1.0),
        },
        kind: PpcQ3LightKind::Directional {
            casts_shadows: 0,
            direction: (0.0, 0.0, -1.0),
        },
    };
    let front_camera = PpcQ3CameraRecord {
        camera: PPC_Q3_OBJECT_BASE + PPC_Q3_OBJECT_STRIDE,
        camera_type: PPC_Q3_CAMERA_TYPE_VIEW_ANGLE_ASPECT,
        placement: PpcQ3CameraPlacement {
            camera_location: (0.0, 0.0, 5.0),
            point_of_interest: (0.0, 0.0, 0.0),
            up_vector: (0.0, 1.0, 0.0),
        },
        range_hither: 1.0,
        range_yon: 10.0,
        viewport_origin: (-1.0, 1.0),
        viewport_width: 2.0,
        viewport_height: 2.0,
        projection: PpcQ3CameraProjection::ViewAngleAspect {
            fov: std::f32::consts::FRAC_PI_2,
            aspect_ratio_x_to_y: 1.0,
        },
    };
    let front_view = ppc_q3_software_view_direction(Some(front_camera), (0.0, 0.0, 0.0));

    assert_eq!(front_view, Some((0.0, 0.0, 1.0)));
    assert_eq!(
        ppc_q3_software_specular_contribution(
            &light,
            (0.0, 0.0, 1.0),
            (0.0, 0.0, -1.0),
            front_view,
            1.0,
            (1.0, 0.0, 0.0),
            1.0,
        ),
        Some((1.0, 0.0, 0.0))
    );
    assert_eq!(
        ppc_q3_software_specular_contribution(
            &light,
            (0.0, 0.0, 1.0),
            (0.0, 0.0, -1.0),
            Some((0.0, 1.0, 0.0)),
            1.0,
            (1.0, 0.0, 0.0),
            1.0,
        ),
        None
    );
    // A zero exponent is a valid broad highlight, not an absent attribute.
    assert_eq!(
        ppc_q3_software_specular_contribution(
            &light,
            (0.0, 0.0, 1.0),
            (0.0, 0.0, -1.0),
            Some((0.0, 1.0, 1.0)),
            1.0,
            (0.5, 0.5, 0.5),
            0.0,
        ),
        Some((0.5, 0.5, 0.5))
    );
}

#[test]
fn q3_software_renderer_applies_point_and_spot_specular_from_camera_view_direction() {
    let view = PPC_Q3_OBJECT_BASE;
    let point_light = PpcQ3LightRecord {
        light: PPC_Q3_OBJECT_BASE + PPC_Q3_OBJECT_STRIDE,
        light_type: PPC_Q3_LIGHT_TYPE_POINT,
        data: PpcQ3LightData {
            is_on: 1,
            brightness: 1.0,
            color: (1.0, 0.0, 0.0),
        },
        kind: PpcQ3LightKind::Point {
            casts_shadows: 0,
            attenuation: PPC_Q3_ATTENUATION_TYPE_NONE,
            location: (0.0, 0.0, 1.0),
        },
    };
    let spot_light = PpcQ3LightRecord {
        light: PPC_Q3_OBJECT_BASE + PPC_Q3_OBJECT_STRIDE * 2,
        light_type: PPC_Q3_LIGHT_TYPE_SPOT,
        data: PpcQ3LightData {
            is_on: 1,
            brightness: 1.0,
            color: (0.0, 1.0, 0.0),
        },
        kind: PpcQ3LightKind::Spot {
            casts_shadows: 0,
            attenuation: PPC_Q3_ATTENUATION_TYPE_NONE,
            location: (0.0, 0.0, 1.0),
            direction: (0.0, 0.0, -1.0),
            hot_angle: 0.25,
            outer_angle: 0.5,
            fall_off: PPC_Q3_FALL_OFF_TYPE_NONE,
        },
    };
    let lights = PpcQ3SubmissionLightRecord {
        view,
        kind: PpcQ3SubmissionKind::TriMesh,
        primary: PPC_DATA_BASE,
        secondary: 0,
        light_group: 0,
        lights: vec![point_light, spot_light],
    };

    let phong_front = ppc_q3_software_apply_lights(
        (0.0, 0.0, 0.0),
        0.0,
        (1.0, 1.0, 1.0),
        1.0,
        PPC_Q3_ILLUMINATION_TYPE_PHONG,
        &lights,
        Some((0.0, 0.0, 1.0)),
        Some((0.0, 0.0, 0.0)),
        Some((0.0, 0.0, 1.0)),
    );
    let phong_side = ppc_q3_software_apply_lights(
        (0.0, 0.0, 0.0),
        0.0,
        (1.0, 1.0, 1.0),
        1.0,
        PPC_Q3_ILLUMINATION_TYPE_PHONG,
        &lights,
        Some((0.0, 0.0, 1.0)),
        Some((0.0, 0.0, 0.0)),
        Some((0.0, 1.0, 0.0)),
    );
    let lambert_front = ppc_q3_software_apply_lights(
        (0.0, 0.0, 0.0),
        0.0,
        (1.0, 1.0, 1.0),
        1.0,
        PPC_Q3_ILLUMINATION_TYPE_LAMBERT,
        &lights,
        Some((0.0, 0.0, 1.0)),
        Some((0.0, 0.0, 0.0)),
        Some((0.0, 0.0, 1.0)),
    );

    assert_eq!(phong_front, (1.0, 1.0, 0.0));
    assert_eq!(phong_side, (0.0, 0.0, 0.0));
    assert_eq!(lambert_front, (0.0, 0.0, 0.0));
}

#[test]
fn q3_software_renderer_applies_point_attenuation_and_spot_falloff_scales() {
    fn assert_close(actual: f32, expected: f32) {
        assert!(
            (actual - expected).abs() < 0.0001,
            "expected {expected}, got {actual}"
        );
    }

    let point_light = PpcQ3LightRecord {
        light: PPC_Q3_OBJECT_BASE,
        light_type: PPC_Q3_LIGHT_TYPE_POINT,
        data: PpcQ3LightData {
            is_on: 1,
            brightness: 1.0,
            color: (0.0, 1.0, 0.0),
        },
        kind: PpcQ3LightKind::Point {
            casts_shadows: 0,
            attenuation: PPC_Q3_ATTENUATION_TYPE_INVERSE_DISTANCE,
            location: (0.0, 0.0, 2.0),
        },
    };
    let point_lights = PpcQ3SubmissionLightRecord {
        view: PPC_Q3_OBJECT_BASE + PPC_Q3_OBJECT_STRIDE,
        kind: PpcQ3SubmissionKind::TriMesh,
        primary: PPC_DATA_BASE,
        secondary: 0,
        light_group: 0,
        lights: vec![point_light],
    };

    let point_color = ppc_q3_software_apply_lights(
        (1.0, 1.0, 1.0),
        0.0,
        (0.0, 0.0, 0.0),
        0.0,
        PPC_Q3_ILLUMINATION_TYPE_LAMBERT,
        &point_lights,
        Some((0.0, 0.0, 1.0)),
        Some((0.0, 0.0, 0.0)),
        Some((0.0, 0.0, 1.0)),
    );
    assert_close(point_color.0, 0.0);
    assert_close(point_color.1, 0.5);
    assert_close(point_color.2, 0.0);

    let spot_angle = 0.5f32;
    let spot_distance = 2.0f32;
    let spot_light_direction = (spot_angle.sin(), 0.0, -spot_angle.cos());
    let spot_world = (
        spot_light_direction.0 * spot_distance,
        0.0,
        spot_light_direction.2 * spot_distance,
    );
    let spot_light = PpcQ3LightRecord {
        light: PPC_Q3_OBJECT_BASE + PPC_Q3_OBJECT_STRIDE * 2,
        light_type: PPC_Q3_LIGHT_TYPE_SPOT,
        data: PpcQ3LightData {
            is_on: 1,
            brightness: 1.0,
            color: (1.0, 0.0, 0.0),
        },
        kind: PpcQ3LightKind::Spot {
            casts_shadows: 0,
            attenuation: PPC_Q3_ATTENUATION_TYPE_NONE,
            location: (0.0, 0.0, 0.0),
            direction: (0.0, 0.0, -1.0),
            hot_angle: 0.25,
            outer_angle: 0.75,
            fall_off: PPC_Q3_FALL_OFF_TYPE_LINEAR,
        },
    };
    let spot_lights = PpcQ3SubmissionLightRecord {
        view: PPC_Q3_OBJECT_BASE + PPC_Q3_OBJECT_STRIDE,
        kind: PpcQ3SubmissionKind::TriMesh,
        primary: PPC_DATA_BASE,
        secondary: 0,
        light_group: 0,
        lights: vec![spot_light],
    };
    let spot_color = ppc_q3_software_apply_lights(
        (1.0, 1.0, 1.0),
        0.0,
        (0.0, 0.0, 0.0),
        0.0,
        PPC_Q3_ILLUMINATION_TYPE_LAMBERT,
        &spot_lights,
        Some((
            -spot_light_direction.0,
            -spot_light_direction.1,
            -spot_light_direction.2,
        )),
        Some(spot_world),
        Some((0.0, 0.0, 1.0)),
    );
    assert_close(spot_color.0, 0.5);
    assert_close(spot_color.1, 0.0);
    assert_close(spot_color.2, 0.0);
}

#[test]
fn q3_software_light_helpers_cover_documented_attenuation_and_spot_falloff_modes() {
    fn assert_close(actual: f32, expected: f32) {
        assert!(
            (actual - expected).abs() < 0.0001,
            "expected {expected}, got {actual}"
        );
    }

    let (direction, attenuation) = ppc_q3_software_point_light_direction_and_attenuation(
        (0.0, 0.0, 0.0),
        (0.0, 0.0, 2.0),
        PPC_Q3_ATTENUATION_TYPE_NONE,
    )
    .unwrap();
    assert_close(direction.0, 0.0);
    assert_close(direction.1, 0.0);
    assert_close(direction.2, -1.0);
    assert_close(attenuation, 1.0);

    let (_, attenuation) = ppc_q3_software_point_light_direction_and_attenuation(
        (0.0, 0.0, 0.0),
        (0.0, 0.0, 2.0),
        PPC_Q3_ATTENUATION_TYPE_INVERSE_DISTANCE,
    )
    .unwrap();
    assert_close(attenuation, 0.5);

    let (_, attenuation) = ppc_q3_software_point_light_direction_and_attenuation(
        (0.0, 0.0, 0.0),
        (0.0, 0.0, 2.0),
        PPC_Q3_ATTENUATION_TYPE_INVERSE_DISTANCE_SQUARED,
    )
    .unwrap();
    assert_close(attenuation, 0.25);

    assert_eq!(
        ppc_q3_software_point_light_direction_and_attenuation(
            (0.0, 0.0, 0.0),
            (0.0, 0.0, 0.0),
            PPC_Q3_ATTENUATION_TYPE_NONE,
        ),
        None
    );

    let spot_direction = (0.0, 0.0, -1.0);
    let center = (0.0, 0.0, -1.0);
    let mid_angle = 0.5f32;
    let mid = (mid_angle.sin(), 0.0, -mid_angle.cos());
    let three_quarter_angle = 0.375f32;
    let three_quarter = (three_quarter_angle.sin(), 0.0, -three_quarter_angle.cos());
    let outside_angle = 0.8f32;
    let outside = (outside_angle.sin(), 0.0, -outside_angle.cos());

    assert_close(
        ppc_q3_software_spot_factor(
            center,
            spot_direction,
            0.25,
            0.75,
            PPC_Q3_FALL_OFF_TYPE_LINEAR,
        )
        .unwrap(),
        1.0,
    );
    assert_close(
        ppc_q3_software_spot_factor(mid, spot_direction, 0.25, 0.75, PPC_Q3_FALL_OFF_TYPE_NONE)
            .unwrap(),
        1.0,
    );
    assert_close(
        ppc_q3_software_spot_factor(
            mid,
            spot_direction,
            0.25,
            0.75,
            PPC_Q3_FALL_OFF_TYPE_LINEAR,
        )
        .unwrap(),
        0.5,
    );
    assert_close(
        ppc_q3_software_spot_factor(
            mid,
            spot_direction,
            0.25,
            0.75,
            PPC_Q3_FALL_OFF_TYPE_EXPONENTIAL,
        )
        .unwrap(),
        0.25,
    );
    assert_close(
        ppc_q3_software_spot_factor(
            three_quarter,
            spot_direction,
            0.25,
            0.75,
            PPC_Q3_FALL_OFF_TYPE_COSINE,
        )
        .unwrap(),
        0.8535534,
    );
    assert_close(
        ppc_q3_software_spot_factor(
            outside,
            spot_direction,
            0.25,
            0.75,
            PPC_Q3_FALL_OFF_TYPE_LINEAR,
        )
        .unwrap(),
        0.0,
    );
}

#[test]
fn q3_software_renderer_applies_specular_attributes_to_directional_lights() {
    fn color_attribute(
        attribute_set: u32,
        attribute_type: u32,
        rgb: [f32; 3],
    ) -> PpcQ3AttributeRecord {
        let mut data = Vec::new();
        for value in rgb {
            data.extend_from_slice(&value.to_bits().to_be_bytes());
        }
        PpcQ3AttributeRecord {
            attribute_set,
            attribute_type,
            data,
        }
    }

    let view = PPC_Q3_OBJECT_BASE;
    let attribute_set = PPC_Q3_OBJECT_BASE + PPC_Q3_OBJECT_STRIDE;
    let directional_light = PPC_Q3_OBJECT_BASE + PPC_Q3_OBJECT_STRIDE * 2;
    let trimesh_data = PPC_DATA_BASE + 0x1000;
    let points_ptr = PPC_DATA_BASE + 0x1080;
    let triangles_ptr = PPC_DATA_BASE + 0x1100;
    let vertex_attrs_ptr = PPC_DATA_BASE + 0x1180;
    let normals_ptr = PPC_DATA_BASE + 0x1200;
    let front_base = PPC_HEAP_BASE + 0x1000;
    let pef = synthetic_pef_with_library_import(b"QuickDraw\xaa 3D", b"Q3TriMesh_Submit");
    let mut loaded = load_pef_application(&pef).unwrap();
    loaded.memory.add_region(trimesh_data, vec![0; 0x400]);
    loaded.memory.add_region(front_base, vec![0; 8 * 8 * 2]);
    loaded.gworlds = vec![PpcGWorldRecord {
        ui_theme: crate::ui_theme::UiThemeId::ClassicSystem7,
        port: PPC_MAIN_GWORLD,
        pixmap_handle: 0,
        pixmap: 0,
        base_addr: front_base,
        gdevice: PPC_MAIN_GDEVICE,
        width: 8,
        height: 8,
        depth: 16,
        row_bytes: 16,
        pixels_locked: false,
        pixels_no_purge: false,
    }];
    loaded
        .current_gworld
        .with_mut(|current_gworld| *current_gworld = PPC_MAIN_GWORLD);

    loaded
        .memory
        .write_u32_be(trimesh_data + PPC_Q3_TRIMESH_NUM_TRIANGLES_OFFSET, 1)
        .unwrap();
    loaded
        .memory
        .write_u32_be(
            trimesh_data + PPC_Q3_TRIMESH_TRIANGLES_OFFSET,
            triangles_ptr,
        )
        .unwrap();
    loaded
        .memory
        .write_u32_be(trimesh_data + PPC_Q3_TRIMESH_NUM_POINTS_OFFSET, 3)
        .unwrap();
    loaded
        .memory
        .write_u32_be(trimesh_data + PPC_Q3_TRIMESH_POINTS_OFFSET, points_ptr)
        .unwrap();
    loaded
        .memory
        .write_u32_be(
            trimesh_data + PPC_Q3_TRIMESH_NUM_VERTEX_ATTRIBUTE_TYPES_OFFSET,
            1,
        )
        .unwrap();
    loaded
        .memory
        .write_u32_be(
            trimesh_data + PPC_Q3_TRIMESH_VERTEX_ATTRIBUTE_TYPES_OFFSET,
            vertex_attrs_ptr,
        )
        .unwrap();
    loaded
        .memory
        .write_u32_be(vertex_attrs_ptr, PPC_Q3_ATTRIBUTE_TYPE_NORMAL)
        .unwrap();
    loaded
        .memory
        .write_u32_be(vertex_attrs_ptr + 4, normals_ptr)
        .unwrap();
    loaded.memory.write_u32_be(vertex_attrs_ptr + 8, 0).unwrap();
    ppc_write_q3_vector3d(&mut loaded.memory, points_ptr, (0.0, 0.0, 0.0)).unwrap();
    ppc_write_q3_vector3d(&mut loaded.memory, points_ptr + 12, (0.0, 0.9, 0.0)).unwrap();
    ppc_write_q3_vector3d(&mut loaded.memory, points_ptr + 24, (0.9, 0.0, 0.0)).unwrap();
    for index in 0..3 {
        ppc_write_q3_vector3d(
            &mut loaded.memory,
            normals_ptr + index * PPC_Q3_POINT3D_SIZE,
            (0.0, 0.0, 1.0),
        )
        .unwrap();
    }
    loaded.memory.write_u32_be(triangles_ptr, 0).unwrap();
    loaded.memory.write_u32_be(triangles_ptr + 4, 1).unwrap();
    loaded.memory.write_u32_be(triangles_ptr + 8, 2).unwrap();

    let specular_control = 1.0f32.to_bits().to_be_bytes().to_vec();
    loaded.q3_views.push(PpcQ3ViewStateRecord::new(view));
    loaded.q3_submissions.push(PpcQ3SubmissionRecord {
        view,
        kind: PpcQ3SubmissionKind::TriMesh,
        primary: trimesh_data,
        secondary: 0,
    });
    loaded
        .q3_submission_transforms
        .push(PpcQ3SubmissionTransformRecord {
            view,
            kind: PpcQ3SubmissionKind::TriMesh,
            primary: trimesh_data,
            secondary: 0,
            local_to_world: ppc_q3_matrix4x4_identity(),
        });
    loaded
        .q3_submission_materials
        .push(PpcQ3SubmissionMaterialRecord {
            view,
            kind: PpcQ3SubmissionKind::TriMesh,
            primary: trimesh_data,
            secondary: 0,
            shader: 0,
            illumination_type: PPC_Q3_ILLUMINATION_TYPE_PHONG,
            styles: Vec::new(),
            fog_style: None,
            attributes: vec![
                color_attribute(
                    attribute_set,
                    PPC_Q3_ATTRIBUTE_TYPE_DIFFUSE_COLOR,
                    [0.0, 0.0, 0.0],
                ),
                color_attribute(
                    attribute_set,
                    PPC_Q3_ATTRIBUTE_TYPE_SPECULAR_COLOR,
                    [1.0, 0.0, 0.0],
                ),
                PpcQ3AttributeRecord {
                    attribute_set,
                    attribute_type: PPC_Q3_ATTRIBUTE_TYPE_SPECULAR_CONTROL,
                    data: specular_control,
                },
            ],
            shader_uv_transform: None,
            shader_boundary: None,
            texture_shader: None,
            mipmap_texture: None,
        });
    loaded
        .q3_submission_lights
        .push(PpcQ3SubmissionLightRecord {
            view,
            kind: PpcQ3SubmissionKind::TriMesh,
            primary: trimesh_data,
            secondary: 0,
            light_group: 0,
            lights: vec![PpcQ3LightRecord {
                light: directional_light,
                light_type: PPC_Q3_LIGHT_TYPE_DIRECTIONAL,
                data: PpcQ3LightData {
                    is_on: 1,
                    brightness: 1.0,
                    color: (1.0, 1.0, 1.0),
                },
                kind: PpcQ3LightKind::Directional {
                    casts_shadows: 0,
                    direction: (0.0, 0.0, -1.0),
                },
            }],
        });

    let stats = loaded.render_q3_scene_commands_to_front_buffer();

    assert_eq!(stats.commands, 1);
    assert_eq!(stats.triangles, 1);
    assert!(stats.pixels > 0);
    assert_eq!(
        loaded.memory.read_u16_be(front_base + 4 * 16 + 4 * 2),
        Some(0x7c00)
    );
}

#[test]
fn q3_software_renderer_uses_vertex_specular_attributes_for_phong() {
    fn color_attribute(
        attribute_set: u32,
        attribute_type: u32,
        rgb: [f32; 3],
    ) -> PpcQ3AttributeRecord {
        let mut data = Vec::new();
        for value in rgb {
            data.extend_from_slice(&value.to_bits().to_be_bytes());
        }
        PpcQ3AttributeRecord {
            attribute_set,
            attribute_type,
            data,
        }
    }

    let view = PPC_Q3_OBJECT_BASE;
    let attribute_set = PPC_Q3_OBJECT_BASE + PPC_Q3_OBJECT_STRIDE;
    let directional_light = PPC_Q3_OBJECT_BASE + PPC_Q3_OBJECT_STRIDE * 2;
    let trimesh_data = PPC_DATA_BASE + 0x1000;
    let points_ptr = PPC_DATA_BASE + 0x1080;
    let triangles_ptr = PPC_DATA_BASE + 0x1100;
    let vertex_attrs_ptr = PPC_DATA_BASE + 0x1180;
    let normals_ptr = PPC_DATA_BASE + 0x1200;
    let specular_colors_ptr = PPC_DATA_BASE + 0x1280;
    let specular_controls_ptr = PPC_DATA_BASE + 0x1300;
    let front_base = PPC_HEAP_BASE + 0x1000;
    let pef = synthetic_pef_with_library_import(b"QuickDraw\xaa 3D", b"Q3TriMesh_Submit");
    let mut loaded = load_pef_application(&pef).unwrap();
    loaded.memory.add_region(trimesh_data, vec![0; 0x500]);
    loaded.memory.add_region(front_base, vec![0; 8 * 8 * 2]);
    loaded.gworlds = vec![PpcGWorldRecord {
        ui_theme: crate::ui_theme::UiThemeId::ClassicSystem7,
        port: PPC_MAIN_GWORLD,
        pixmap_handle: 0,
        pixmap: 0,
        base_addr: front_base,
        gdevice: PPC_MAIN_GDEVICE,
        width: 8,
        height: 8,
        depth: 16,
        row_bytes: 16,
        pixels_locked: false,
        pixels_no_purge: false,
    }];
    loaded
        .current_gworld
        .with_mut(|current_gworld| *current_gworld = PPC_MAIN_GWORLD);

    loaded
        .memory
        .write_u32_be(trimesh_data + PPC_Q3_TRIMESH_NUM_TRIANGLES_OFFSET, 1)
        .unwrap();
    loaded
        .memory
        .write_u32_be(
            trimesh_data + PPC_Q3_TRIMESH_TRIANGLES_OFFSET,
            triangles_ptr,
        )
        .unwrap();
    loaded
        .memory
        .write_u32_be(trimesh_data + PPC_Q3_TRIMESH_NUM_POINTS_OFFSET, 3)
        .unwrap();
    loaded
        .memory
        .write_u32_be(trimesh_data + PPC_Q3_TRIMESH_POINTS_OFFSET, points_ptr)
        .unwrap();
    loaded
        .memory
        .write_u32_be(
            trimesh_data + PPC_Q3_TRIMESH_NUM_VERTEX_ATTRIBUTE_TYPES_OFFSET,
            3,
        )
        .unwrap();
    loaded
        .memory
        .write_u32_be(
            trimesh_data + PPC_Q3_TRIMESH_VERTEX_ATTRIBUTE_TYPES_OFFSET,
            vertex_attrs_ptr,
        )
        .unwrap();
    for (index, (attribute_type, data_ptr)) in [
        (PPC_Q3_ATTRIBUTE_TYPE_NORMAL, normals_ptr),
        (PPC_Q3_ATTRIBUTE_TYPE_SPECULAR_COLOR, specular_colors_ptr),
        (
            PPC_Q3_ATTRIBUTE_TYPE_SPECULAR_CONTROL,
            specular_controls_ptr,
        ),
    ]
    .into_iter()
    .enumerate()
    {
        let entry_ptr = vertex_attrs_ptr + index as u32 * PPC_Q3_TRIMESH_ATTRIBUTE_DATA_SIZE;
        loaded
            .memory
            .write_u32_be(entry_ptr, attribute_type)
            .unwrap();
        loaded.memory.write_u32_be(entry_ptr + 4, data_ptr).unwrap();
        loaded.memory.write_u32_be(entry_ptr + 8, 0).unwrap();
    }
    ppc_write_q3_vector3d(&mut loaded.memory, points_ptr, (0.0, 0.0, 0.0)).unwrap();
    ppc_write_q3_vector3d(&mut loaded.memory, points_ptr + 12, (0.0, 0.9, 0.0)).unwrap();
    ppc_write_q3_vector3d(&mut loaded.memory, points_ptr + 24, (0.9, 0.0, 0.0)).unwrap();
    for index in 0..3 {
        ppc_write_q3_vector3d(
            &mut loaded.memory,
            normals_ptr + index * PPC_Q3_POINT3D_SIZE,
            (0.0, 0.0, 1.0),
        )
        .unwrap();
        ppc_write_q3_color_rgb(
            &mut loaded.memory,
            specular_colors_ptr + index * PPC_Q3_POINT3D_SIZE,
            (0.0, 0.0, 1.0),
        )
        .unwrap();
        ppc_write_f32_be(&mut loaded.memory, specular_controls_ptr + index * 4, 1.0).unwrap();
    }
    loaded.memory.write_u32_be(triangles_ptr, 0).unwrap();
    loaded.memory.write_u32_be(triangles_ptr + 4, 1).unwrap();
    loaded.memory.write_u32_be(triangles_ptr + 8, 2).unwrap();

    let specular_control = 1.0f32.to_bits().to_be_bytes().to_vec();
    loaded.q3_views.push(PpcQ3ViewStateRecord::new(view));
    loaded.q3_submissions.push(PpcQ3SubmissionRecord {
        view,
        kind: PpcQ3SubmissionKind::TriMesh,
        primary: trimesh_data,
        secondary: 0,
    });
    loaded
        .q3_submission_transforms
        .push(PpcQ3SubmissionTransformRecord {
            view,
            kind: PpcQ3SubmissionKind::TriMesh,
            primary: trimesh_data,
            secondary: 0,
            local_to_world: ppc_q3_matrix4x4_identity(),
        });
    loaded
        .q3_submission_materials
        .push(PpcQ3SubmissionMaterialRecord {
            view,
            kind: PpcQ3SubmissionKind::TriMesh,
            primary: trimesh_data,
            secondary: 0,
            shader: 0,
            illumination_type: PPC_Q3_ILLUMINATION_TYPE_PHONG,
            styles: Vec::new(),
            fog_style: None,
            attributes: vec![
                color_attribute(
                    attribute_set,
                    PPC_Q3_ATTRIBUTE_TYPE_DIFFUSE_COLOR,
                    [0.0, 0.0, 0.0],
                ),
                color_attribute(
                    attribute_set,
                    PPC_Q3_ATTRIBUTE_TYPE_SPECULAR_COLOR,
                    [1.0, 0.0, 0.0],
                ),
                PpcQ3AttributeRecord {
                    attribute_set,
                    attribute_type: PPC_Q3_ATTRIBUTE_TYPE_SPECULAR_CONTROL,
                    data: specular_control,
                },
            ],
            shader_uv_transform: None,
            shader_boundary: None,
            texture_shader: None,
            mipmap_texture: None,
        });
    loaded
        .q3_submission_lights
        .push(PpcQ3SubmissionLightRecord {
            view,
            kind: PpcQ3SubmissionKind::TriMesh,
            primary: trimesh_data,
            secondary: 0,
            light_group: 0,
            lights: vec![PpcQ3LightRecord {
                light: directional_light,
                light_type: PPC_Q3_LIGHT_TYPE_DIRECTIONAL,
                data: PpcQ3LightData {
                    is_on: 1,
                    brightness: 1.0,
                    color: (1.0, 1.0, 1.0),
                },
                kind: PpcQ3LightKind::Directional {
                    casts_shadows: 0,
                    direction: (0.0, 0.0, -1.0),
                },
            }],
        });

    let stats = loaded.render_q3_scene_commands_to_front_buffer();

    assert_eq!(stats.commands, 1);
    assert_eq!(stats.triangles, 1);
    assert!(stats.pixels > 0);
    assert_eq!(
        loaded.memory.read_u16_be(front_base + 4 * 16 + 4 * 2),
        Some(0x001f)
    );
}

#[test]
fn q3_software_renderer_uses_triangle_specular_attributes_for_phong() {
    fn color_attribute(
        attribute_set: u32,
        attribute_type: u32,
        rgb: [f32; 3],
    ) -> PpcQ3AttributeRecord {
        let mut data = Vec::new();
        for value in rgb {
            data.extend_from_slice(&value.to_bits().to_be_bytes());
        }
        PpcQ3AttributeRecord {
            attribute_set,
            attribute_type,
            data,
        }
    }

    let view = PPC_Q3_OBJECT_BASE;
    let attribute_set = PPC_Q3_OBJECT_BASE + PPC_Q3_OBJECT_STRIDE;
    let directional_light = PPC_Q3_OBJECT_BASE + PPC_Q3_OBJECT_STRIDE * 2;
    let trimesh_data = PPC_DATA_BASE + 0x1000;
    let points_ptr = PPC_DATA_BASE + 0x1080;
    let triangles_ptr = PPC_DATA_BASE + 0x1100;
    let triangle_attrs_ptr = PPC_DATA_BASE + 0x1180;
    let normals_ptr = PPC_DATA_BASE + 0x1200;
    let vertex_attrs_ptr = PPC_DATA_BASE + 0x1280;
    let specular_color_ptr = PPC_DATA_BASE + 0x1300;
    let specular_control_ptr = PPC_DATA_BASE + 0x1340;
    let front_base = PPC_HEAP_BASE + 0x1000;
    let pef = synthetic_pef_with_library_import(b"QuickDraw\xaa 3D", b"Q3TriMesh_Submit");
    let mut loaded = load_pef_application(&pef).unwrap();
    loaded.memory.add_region(trimesh_data, vec![0; 0x500]);
    loaded.memory.add_region(front_base, vec![0; 8 * 8 * 2]);
    loaded.gworlds = vec![PpcGWorldRecord {
        ui_theme: crate::ui_theme::UiThemeId::ClassicSystem7,
        port: PPC_MAIN_GWORLD,
        pixmap_handle: 0,
        pixmap: 0,
        base_addr: front_base,
        gdevice: PPC_MAIN_GDEVICE,
        width: 8,
        height: 8,
        depth: 16,
        row_bytes: 16,
        pixels_locked: false,
        pixels_no_purge: false,
    }];
    loaded
        .current_gworld
        .with_mut(|current_gworld| *current_gworld = PPC_MAIN_GWORLD);

    loaded
        .memory
        .write_u32_be(trimesh_data + PPC_Q3_TRIMESH_NUM_TRIANGLES_OFFSET, 1)
        .unwrap();
    loaded
        .memory
        .write_u32_be(
            trimesh_data + PPC_Q3_TRIMESH_TRIANGLES_OFFSET,
            triangles_ptr,
        )
        .unwrap();
    loaded
        .memory
        .write_u32_be(trimesh_data + PPC_Q3_TRIMESH_NUM_POINTS_OFFSET, 3)
        .unwrap();
    loaded
        .memory
        .write_u32_be(trimesh_data + PPC_Q3_TRIMESH_POINTS_OFFSET, points_ptr)
        .unwrap();
    loaded
        .memory
        .write_u32_be(
            trimesh_data + PPC_Q3_TRIMESH_NUM_TRIANGLE_ATTRIBUTE_TYPES_OFFSET,
            2,
        )
        .unwrap();
    loaded
        .memory
        .write_u32_be(
            trimesh_data + PPC_Q3_TRIMESH_TRIANGLE_ATTRIBUTE_TYPES_OFFSET,
            triangle_attrs_ptr,
        )
        .unwrap();
    for (index, (attribute_type, data_ptr)) in [
        (PPC_Q3_ATTRIBUTE_TYPE_SPECULAR_COLOR, specular_color_ptr),
        (PPC_Q3_ATTRIBUTE_TYPE_SPECULAR_CONTROL, specular_control_ptr),
    ]
    .into_iter()
    .enumerate()
    {
        let entry_ptr = triangle_attrs_ptr + index as u32 * PPC_Q3_TRIMESH_ATTRIBUTE_DATA_SIZE;
        loaded
            .memory
            .write_u32_be(entry_ptr, attribute_type)
            .unwrap();
        loaded.memory.write_u32_be(entry_ptr + 4, data_ptr).unwrap();
        loaded.memory.write_u32_be(entry_ptr + 8, 0).unwrap();
    }
    loaded
        .memory
        .write_u32_be(
            trimesh_data + PPC_Q3_TRIMESH_NUM_VERTEX_ATTRIBUTE_TYPES_OFFSET,
            1,
        )
        .unwrap();
    loaded
        .memory
        .write_u32_be(
            trimesh_data + PPC_Q3_TRIMESH_VERTEX_ATTRIBUTE_TYPES_OFFSET,
            vertex_attrs_ptr,
        )
        .unwrap();
    loaded
        .memory
        .write_u32_be(vertex_attrs_ptr, PPC_Q3_ATTRIBUTE_TYPE_NORMAL)
        .unwrap();
    loaded
        .memory
        .write_u32_be(vertex_attrs_ptr + 4, normals_ptr)
        .unwrap();
    loaded.memory.write_u32_be(vertex_attrs_ptr + 8, 0).unwrap();
    ppc_write_q3_vector3d(&mut loaded.memory, points_ptr, (0.0, 0.0, 0.0)).unwrap();
    ppc_write_q3_vector3d(&mut loaded.memory, points_ptr + 12, (0.0, 0.9, 0.0)).unwrap();
    ppc_write_q3_vector3d(&mut loaded.memory, points_ptr + 24, (0.9, 0.0, 0.0)).unwrap();
    for index in 0..3 {
        ppc_write_q3_vector3d(
            &mut loaded.memory,
            normals_ptr + index * PPC_Q3_POINT3D_SIZE,
            (0.0, 0.0, 1.0),
        )
        .unwrap();
    }
    ppc_write_q3_color_rgb(&mut loaded.memory, specular_color_ptr, (0.0, 0.0, 1.0)).unwrap();
    ppc_write_f32_be(&mut loaded.memory, specular_control_ptr, 1.0).unwrap();
    loaded.memory.write_u32_be(triangles_ptr, 0).unwrap();
    loaded.memory.write_u32_be(triangles_ptr + 4, 1).unwrap();
    loaded.memory.write_u32_be(triangles_ptr + 8, 2).unwrap();

    let specular_control = 1.0f32.to_bits().to_be_bytes().to_vec();
    loaded.q3_views.push(PpcQ3ViewStateRecord::new(view));
    loaded.q3_submissions.push(PpcQ3SubmissionRecord {
        view,
        kind: PpcQ3SubmissionKind::TriMesh,
        primary: trimesh_data,
        secondary: 0,
    });
    loaded
        .q3_submission_transforms
        .push(PpcQ3SubmissionTransformRecord {
            view,
            kind: PpcQ3SubmissionKind::TriMesh,
            primary: trimesh_data,
            secondary: 0,
            local_to_world: ppc_q3_matrix4x4_identity(),
        });
    loaded
        .q3_submission_materials
        .push(PpcQ3SubmissionMaterialRecord {
            view,
            kind: PpcQ3SubmissionKind::TriMesh,
            primary: trimesh_data,
            secondary: 0,
            shader: 0,
            illumination_type: PPC_Q3_ILLUMINATION_TYPE_PHONG,
            styles: Vec::new(),
            fog_style: None,
            attributes: vec![
                color_attribute(
                    attribute_set,
                    PPC_Q3_ATTRIBUTE_TYPE_DIFFUSE_COLOR,
                    [0.0, 0.0, 0.0],
                ),
                color_attribute(
                    attribute_set,
                    PPC_Q3_ATTRIBUTE_TYPE_SPECULAR_COLOR,
                    [1.0, 0.0, 0.0],
                ),
                PpcQ3AttributeRecord {
                    attribute_set,
                    attribute_type: PPC_Q3_ATTRIBUTE_TYPE_SPECULAR_CONTROL,
                    data: specular_control,
                },
            ],
            shader_uv_transform: None,
            shader_boundary: None,
            texture_shader: None,
            mipmap_texture: None,
        });
    loaded
        .q3_submission_lights
        .push(PpcQ3SubmissionLightRecord {
            view,
            kind: PpcQ3SubmissionKind::TriMesh,
            primary: trimesh_data,
            secondary: 0,
            light_group: 0,
            lights: vec![PpcQ3LightRecord {
                light: directional_light,
                light_type: PPC_Q3_LIGHT_TYPE_DIRECTIONAL,
                data: PpcQ3LightData {
                    is_on: 1,
                    brightness: 1.0,
                    color: (1.0, 1.0, 1.0),
                },
                kind: PpcQ3LightKind::Directional {
                    casts_shadows: 0,
                    direction: (0.0, 0.0, -1.0),
                },
            }],
        });

    let stats = loaded.render_q3_scene_commands_to_front_buffer();

    assert_eq!(stats.commands, 1);
    assert_eq!(stats.triangles, 1);
    assert!(stats.pixels > 0);
    assert_eq!(
        loaded.memory.read_u16_be(front_base + 4 * 16 + 4 * 2),
        Some(0x001f)
    );
}

#[test]
fn q3_software_renderer_uses_vertex_normals_for_directional_lights() {
    fn write_lit_triangle_mesh(
        memory: &mut PpcSectionMem,
        trimesh_data: u32,
        points_ptr: u32,
        triangles_ptr: u32,
        vertex_attrs_ptr: u32,
        normals_ptr: u32,
        points: [(f32, f32, f32); 3],
        normal: (f32, f32, f32),
    ) {
        memory
            .write_u32_be(trimesh_data + PPC_Q3_TRIMESH_NUM_TRIANGLES_OFFSET, 1)
            .unwrap();
        memory
            .write_u32_be(
                trimesh_data + PPC_Q3_TRIMESH_TRIANGLES_OFFSET,
                triangles_ptr,
            )
            .unwrap();
        memory
            .write_u32_be(trimesh_data + PPC_Q3_TRIMESH_NUM_POINTS_OFFSET, 3)
            .unwrap();
        memory
            .write_u32_be(trimesh_data + PPC_Q3_TRIMESH_POINTS_OFFSET, points_ptr)
            .unwrap();
        memory
            .write_u32_be(
                trimesh_data + PPC_Q3_TRIMESH_NUM_VERTEX_ATTRIBUTE_TYPES_OFFSET,
                1,
            )
            .unwrap();
        memory
            .write_u32_be(
                trimesh_data + PPC_Q3_TRIMESH_VERTEX_ATTRIBUTE_TYPES_OFFSET,
                vertex_attrs_ptr,
            )
            .unwrap();
        memory
            .write_u32_be(vertex_attrs_ptr, PPC_Q3_ATTRIBUTE_TYPE_NORMAL)
            .unwrap();
        memory
            .write_u32_be(vertex_attrs_ptr + 4, normals_ptr)
            .unwrap();
        memory.write_u32_be(vertex_attrs_ptr + 8, 0).unwrap();
        for (index, point) in points.into_iter().enumerate() {
            ppc_write_q3_vector3d(memory, points_ptr + index as u32 * 12, point).unwrap();
            ppc_write_q3_vector3d(memory, normals_ptr + index as u32 * 12, normal).unwrap();
        }
        memory.write_u32_be(triangles_ptr, 0).unwrap();
        memory.write_u32_be(triangles_ptr + 4, 1).unwrap();
        memory.write_u32_be(triangles_ptr + 8, 2).unwrap();
    }

    fn diffuse_attribute(attribute_set: u32) -> PpcQ3AttributeRecord {
        let mut data = Vec::new();
        for value in [1.0f32, 1.0, 1.0] {
            data.extend_from_slice(&value.to_bits().to_be_bytes());
        }
        PpcQ3AttributeRecord {
            attribute_set,
            attribute_type: PPC_Q3_ATTRIBUTE_TYPE_DIFFUSE_COLOR,
            data,
        }
    }

    let view = PPC_Q3_OBJECT_BASE;
    let ambient_light = PPC_Q3_OBJECT_BASE + PPC_Q3_OBJECT_STRIDE;
    let directional_light = PPC_Q3_OBJECT_BASE + PPC_Q3_OBJECT_STRIDE * 2;
    let front_base = PPC_HEAP_BASE + 0x1000;
    let left_trimesh_data = PPC_DATA_BASE + 0x1000;
    let left_points_ptr = PPC_DATA_BASE + 0x1080;
    let left_triangles_ptr = PPC_DATA_BASE + 0x1100;
    let left_vertex_attrs_ptr = PPC_DATA_BASE + 0x1180;
    let left_normals_ptr = PPC_DATA_BASE + 0x11c0;
    let right_trimesh_data = PPC_DATA_BASE + 0x1200;
    let right_points_ptr = PPC_DATA_BASE + 0x1280;
    let right_triangles_ptr = PPC_DATA_BASE + 0x1300;
    let right_vertex_attrs_ptr = PPC_DATA_BASE + 0x1380;
    let right_normals_ptr = PPC_DATA_BASE + 0x13c0;
    let pef = synthetic_pef_with_library_import(b"QuickDraw\xaa 3D", b"Q3TriMesh_Submit");
    let mut loaded = load_pef_application(&pef).unwrap();
    loaded
        .memory
        .add_region(PPC_DATA_BASE + 0x1000, vec![0; 0x800]);
    loaded.memory.add_region(front_base, vec![0; 8 * 8 * 2]);
    loaded.gworlds = vec![PpcGWorldRecord {
        ui_theme: crate::ui_theme::UiThemeId::ClassicSystem7,
        port: PPC_MAIN_GWORLD,
        pixmap_handle: 0,
        pixmap: 0,
        base_addr: front_base,
        gdevice: PPC_MAIN_GDEVICE,
        width: 8,
        height: 8,
        depth: 16,
        row_bytes: 16,
        pixels_locked: false,
        pixels_no_purge: false,
    }];
    loaded
        .current_gworld
        .with_mut(|current_gworld| *current_gworld = PPC_MAIN_GWORLD);

    write_lit_triangle_mesh(
        &mut loaded.memory,
        left_trimesh_data,
        left_points_ptr,
        left_triangles_ptr,
        left_vertex_attrs_ptr,
        left_normals_ptr,
        [(-0.9, -0.8, 0.0), (-0.1, -0.8, 0.0), (-0.9, 0.8, 0.0)],
        (0.0, 0.0, 1.0),
    );
    write_lit_triangle_mesh(
        &mut loaded.memory,
        right_trimesh_data,
        right_points_ptr,
        right_triangles_ptr,
        right_vertex_attrs_ptr,
        right_normals_ptr,
        [(0.1, -0.8, 0.0), (0.9, -0.8, 0.0), (0.1, 0.8, 0.0)],
        (0.0, 0.0, -1.0),
    );
    loaded.q3_views.push(PpcQ3ViewStateRecord::new(view));
    let lights = vec![
        PpcQ3LightRecord {
            light: ambient_light,
            light_type: PPC_Q3_LIGHT_TYPE_AMBIENT,
            data: PpcQ3LightData {
                is_on: 1,
                brightness: 0.25,
                color: (1.0, 0.0, 0.0),
            },
            kind: PpcQ3LightKind::Ambient,
        },
        PpcQ3LightRecord {
            light: directional_light,
            light_type: PPC_Q3_LIGHT_TYPE_DIRECTIONAL,
            data: PpcQ3LightData {
                is_on: 1,
                brightness: 0.75,
                color: (0.0, 1.0, 0.0),
            },
            kind: PpcQ3LightKind::Directional {
                casts_shadows: 0,
                direction: (0.0, 0.0, -1.0),
            },
        },
    ];
    for (index, (trimesh_data, attribute_set)) in [
        (
            left_trimesh_data,
            PPC_Q3_OBJECT_BASE + PPC_Q3_OBJECT_STRIDE * 3,
        ),
        (
            right_trimesh_data,
            PPC_Q3_OBJECT_BASE + PPC_Q3_OBJECT_STRIDE * 4,
        ),
    ]
    .into_iter()
    .enumerate()
    {
        loaded.q3_submissions.push(PpcQ3SubmissionRecord {
            view,
            kind: PpcQ3SubmissionKind::TriMesh,
            primary: trimesh_data,
            secondary: 0,
        });
        loaded
            .q3_submission_transforms
            .push(PpcQ3SubmissionTransformRecord {
                view,
                kind: PpcQ3SubmissionKind::TriMesh,
                primary: trimesh_data,
                secondary: 0,
                local_to_world: ppc_q3_matrix4x4_identity(),
            });
        loaded
            .q3_submission_materials
            .push(PpcQ3SubmissionMaterialRecord {
                view,
                kind: PpcQ3SubmissionKind::TriMesh,
                primary: trimesh_data,
                secondary: 0,
                shader: 0,
                // Isolate diffuse normal handling from specular highlights.
                illumination_type: PPC_Q3_ILLUMINATION_TYPE_LAMBERT,
                styles: Vec::new(),
                fog_style: None,
                attributes: vec![diffuse_attribute(attribute_set)],
                shader_uv_transform: None,
                shader_boundary: None,
                texture_shader: None,
                mipmap_texture: None,
            });
        loaded
            .q3_submission_lights
            .push(PpcQ3SubmissionLightRecord {
                view,
                kind: PpcQ3SubmissionKind::TriMesh,
                primary: trimesh_data,
                secondary: 0,
                light_group: index as u32,
                lights: lights.clone(),
            });
    }

    let stats = loaded.render_q3_scene_commands_to_front_buffer();

    assert_eq!(stats.commands, 2);
    assert_eq!(stats.triangles, 2);
    assert!(stats.pixels > 0);
    assert_eq!(
        loaded.memory.read_u16_be(front_base + 4 * 16 + 2),
        Some(0x22e0)
    );
    assert_eq!(
        loaded.memory.read_u16_be(front_base + 4 * 16 + 5 * 2),
        Some(0x2000)
    );

    // Both triangles have counterclockwise geometry. Omitting the normal
    // attributes must light both like the explicit +Z normal, rather than
    // retaining the right triangle's deliberately opposing vertex normal.
    for data in [left_trimesh_data, right_trimesh_data] {
        loaded
            .memory
            .write_u32_be(data + PPC_Q3_TRIMESH_NUM_VERTEX_ATTRIBUTE_TYPES_OFFSET, 0)
            .unwrap();
    }
    let stats = loaded.render_q3_scene_commands_to_front_buffer();
    assert_eq!(stats.triangles, 2);
    for x in [1, 5] {
        assert_eq!(
            loaded.memory.read_u16_be(front_base + 4 * 16 + x * 2),
            Some(0x22e0)
        );
    }

    // Clockwise front faces reverse generated normals; ambient lighting
    // remains, but a light from +Z no longer illuminates either surface.
    for material in &mut loaded.q3_submission_materials {
        material.styles.push(PpcQ3StyleRecord {
            style: 0,
            kind: PpcQ3StyleKind::Orientation,
            value: PPC_Q3_ORIENTATION_STYLE_CLOCKWISE,
        });
    }
    loaded.render_q3_scene_commands_to_front_buffer();
    for x in [1, 5] {
        assert_eq!(
            loaded.memory.read_u16_be(front_base + 4 * 16 + x * 2),
            Some(0x2000)
        );
    }
}

#[test]
fn q3_software_renderer_uses_triangle_normals_for_directional_lights() {
    fn write_lit_triangle_mesh(
        memory: &mut PpcSectionMem,
        trimesh_data: u32,
        points_ptr: u32,
        triangles_ptr: u32,
        triangle_attrs_ptr: u32,
        normal_ptr: u32,
        points: [(f32, f32, f32); 3],
        normal: (f32, f32, f32),
    ) {
        memory
            .write_u32_be(trimesh_data + PPC_Q3_TRIMESH_NUM_TRIANGLES_OFFSET, 1)
            .unwrap();
        memory
            .write_u32_be(
                trimesh_data + PPC_Q3_TRIMESH_TRIANGLES_OFFSET,
                triangles_ptr,
            )
            .unwrap();
        memory
            .write_u32_be(trimesh_data + PPC_Q3_TRIMESH_NUM_POINTS_OFFSET, 3)
            .unwrap();
        memory
            .write_u32_be(trimesh_data + PPC_Q3_TRIMESH_POINTS_OFFSET, points_ptr)
            .unwrap();
        memory
            .write_u32_be(
                trimesh_data + PPC_Q3_TRIMESH_NUM_TRIANGLE_ATTRIBUTE_TYPES_OFFSET,
                1,
            )
            .unwrap();
        memory
            .write_u32_be(
                trimesh_data + PPC_Q3_TRIMESH_TRIANGLE_ATTRIBUTE_TYPES_OFFSET,
                triangle_attrs_ptr,
            )
            .unwrap();
        memory
            .write_u32_be(triangle_attrs_ptr, PPC_Q3_ATTRIBUTE_TYPE_NORMAL)
            .unwrap();
        memory
            .write_u32_be(triangle_attrs_ptr + 4, normal_ptr)
            .unwrap();
        memory.write_u32_be(triangle_attrs_ptr + 8, 0).unwrap();
        ppc_write_q3_vector3d(memory, normal_ptr, normal).unwrap();
        for (index, point) in points.into_iter().enumerate() {
            ppc_write_q3_vector3d(memory, points_ptr + index as u32 * 12, point).unwrap();
        }
        memory.write_u32_be(triangles_ptr, 0).unwrap();
        memory.write_u32_be(triangles_ptr + 4, 1).unwrap();
        memory.write_u32_be(triangles_ptr + 8, 2).unwrap();
    }

    fn diffuse_attribute(attribute_set: u32) -> PpcQ3AttributeRecord {
        let mut data = Vec::new();
        for value in [1.0f32, 1.0, 1.0] {
            data.extend_from_slice(&value.to_bits().to_be_bytes());
        }
        PpcQ3AttributeRecord {
            attribute_set,
            attribute_type: PPC_Q3_ATTRIBUTE_TYPE_DIFFUSE_COLOR,
            data,
        }
    }

    let view = PPC_Q3_OBJECT_BASE;
    let directional_light = PPC_Q3_OBJECT_BASE + PPC_Q3_OBJECT_STRIDE;
    let front_base = PPC_HEAP_BASE + 0x1000;
    let left_trimesh_data = PPC_DATA_BASE + 0x1000;
    let left_points_ptr = PPC_DATA_BASE + 0x1080;
    let left_triangles_ptr = PPC_DATA_BASE + 0x1100;
    let left_triangle_attrs_ptr = PPC_DATA_BASE + 0x1180;
    let left_normal_ptr = PPC_DATA_BASE + 0x11c0;
    let right_trimesh_data = PPC_DATA_BASE + 0x1200;
    let right_points_ptr = PPC_DATA_BASE + 0x1280;
    let right_triangles_ptr = PPC_DATA_BASE + 0x1300;
    let right_triangle_attrs_ptr = PPC_DATA_BASE + 0x1380;
    let right_normal_ptr = PPC_DATA_BASE + 0x13c0;
    let pef = synthetic_pef_with_library_import(b"QuickDraw\xaa 3D", b"Q3TriMesh_Submit");
    let mut loaded = load_pef_application(&pef).unwrap();
    loaded
        .memory
        .add_region(PPC_DATA_BASE + 0x1000, vec![0; 0x800]);
    loaded.memory.add_region(front_base, vec![0; 8 * 8 * 2]);
    loaded.gworlds = vec![PpcGWorldRecord {
        ui_theme: crate::ui_theme::UiThemeId::ClassicSystem7,
        port: PPC_MAIN_GWORLD,
        pixmap_handle: 0,
        pixmap: 0,
        base_addr: front_base,
        gdevice: PPC_MAIN_GDEVICE,
        width: 8,
        height: 8,
        depth: 16,
        row_bytes: 16,
        pixels_locked: false,
        pixels_no_purge: false,
    }];
    loaded
        .current_gworld
        .with_mut(|current_gworld| *current_gworld = PPC_MAIN_GWORLD);

    write_lit_triangle_mesh(
        &mut loaded.memory,
        left_trimesh_data,
        left_points_ptr,
        left_triangles_ptr,
        left_triangle_attrs_ptr,
        left_normal_ptr,
        [(-0.9, -0.8, 0.0), (-0.1, -0.8, 0.0), (-0.9, 0.8, 0.0)],
        (0.0, 0.0, 1.0),
    );
    write_lit_triangle_mesh(
        &mut loaded.memory,
        right_trimesh_data,
        right_points_ptr,
        right_triangles_ptr,
        right_triangle_attrs_ptr,
        right_normal_ptr,
        [(0.1, -0.8, 0.0), (0.9, -0.8, 0.0), (0.1, 0.8, 0.0)],
        (1.0, 0.0, 0.0),
    );
    loaded.q3_views.push(PpcQ3ViewStateRecord::new(view));
    let lights = vec![PpcQ3LightRecord {
        light: directional_light,
        light_type: PPC_Q3_LIGHT_TYPE_DIRECTIONAL,
        data: PpcQ3LightData {
            is_on: 1,
            brightness: 1.0,
            color: (0.0, 1.0, 0.0),
        },
        kind: PpcQ3LightKind::Directional {
            casts_shadows: 0,
            direction: (0.0, 0.0, -1.0),
        },
    }];
    for (index, (trimesh_data, attribute_set)) in [
        (
            left_trimesh_data,
            PPC_Q3_OBJECT_BASE + PPC_Q3_OBJECT_STRIDE * 2,
        ),
        (
            right_trimesh_data,
            PPC_Q3_OBJECT_BASE + PPC_Q3_OBJECT_STRIDE * 3,
        ),
    ]
    .into_iter()
    .enumerate()
    {
        loaded.q3_submissions.push(PpcQ3SubmissionRecord {
            view,
            kind: PpcQ3SubmissionKind::TriMesh,
            primary: trimesh_data,
            secondary: 0,
        });
        loaded
            .q3_submission_transforms
            .push(PpcQ3SubmissionTransformRecord {
                view,
                kind: PpcQ3SubmissionKind::TriMesh,
                primary: trimesh_data,
                secondary: 0,
                local_to_world: ppc_q3_matrix4x4_identity(),
            });
        loaded
            .q3_submission_materials
            .push(PpcQ3SubmissionMaterialRecord {
                view,
                kind: PpcQ3SubmissionKind::TriMesh,
                primary: trimesh_data,
                secondary: 0,
                shader: 0,
                illumination_type: PPC_Q3_ILLUMINATION_TYPE_LAMBERT,
                styles: Vec::new(),
                fog_style: None,
                attributes: vec![diffuse_attribute(attribute_set)],
                shader_uv_transform: None,
                shader_boundary: None,
                texture_shader: None,
                mipmap_texture: None,
            });
        loaded
            .q3_submission_lights
            .push(PpcQ3SubmissionLightRecord {
                view,
                kind: PpcQ3SubmissionKind::TriMesh,
                primary: trimesh_data,
                secondary: 0,
                light_group: index as u32,
                lights: lights.clone(),
            });
    }

    let stats = loaded.render_q3_scene_commands_to_front_buffer();

    assert_eq!(stats.commands, 2);
    assert_eq!(stats.triangles, 2);
    assert!(stats.pixels > 0);
    assert_eq!(
        loaded.memory.read_u16_be(front_base + 4 * 16 + 2),
        Some(0x03e0)
    );
    assert_eq!(
        loaded.memory.read_u16_be(front_base + 4 * 16 + 5 * 2),
        Some(0)
    );
}

#[test]
fn q3_software_renderer_applies_point_and_spot_lights_from_world_positions() {
    fn write_lit_triangle_mesh(
        memory: &mut PpcSectionMem,
        trimesh_data: u32,
        points_ptr: u32,
        triangles_ptr: u32,
        vertex_attrs_ptr: u32,
        normals_ptr: u32,
    ) {
        memory
            .write_u32_be(trimesh_data + PPC_Q3_TRIMESH_NUM_TRIANGLES_OFFSET, 1)
            .unwrap();
        memory
            .write_u32_be(
                trimesh_data + PPC_Q3_TRIMESH_TRIANGLES_OFFSET,
                triangles_ptr,
            )
            .unwrap();
        memory
            .write_u32_be(trimesh_data + PPC_Q3_TRIMESH_NUM_POINTS_OFFSET, 3)
            .unwrap();
        memory
            .write_u32_be(trimesh_data + PPC_Q3_TRIMESH_POINTS_OFFSET, points_ptr)
            .unwrap();
        memory
            .write_u32_be(
                trimesh_data + PPC_Q3_TRIMESH_NUM_VERTEX_ATTRIBUTE_TYPES_OFFSET,
                1,
            )
            .unwrap();
        memory
            .write_u32_be(
                trimesh_data + PPC_Q3_TRIMESH_VERTEX_ATTRIBUTE_TYPES_OFFSET,
                vertex_attrs_ptr,
            )
            .unwrap();
        memory
            .write_u32_be(vertex_attrs_ptr, PPC_Q3_ATTRIBUTE_TYPE_NORMAL)
            .unwrap();
        memory
            .write_u32_be(vertex_attrs_ptr + 4, normals_ptr)
            .unwrap();
        memory.write_u32_be(vertex_attrs_ptr + 8, 0).unwrap();
        for (index, point) in [(-0.9, -0.9, 0.0), (-0.9, 0.9, 0.0), (0.9, -0.9, 0.0)]
            .into_iter()
            .enumerate()
        {
            ppc_write_q3_vector3d(
                memory,
                points_ptr + index as u32 * PPC_Q3_POINT3D_SIZE,
                point,
            )
            .unwrap();
            ppc_write_q3_vector3d(
                memory,
                normals_ptr + index as u32 * PPC_Q3_POINT3D_SIZE,
                (0.0, 0.0, 1.0),
            )
            .unwrap();
        }
        memory.write_u32_be(triangles_ptr, 0).unwrap();
        memory.write_u32_be(triangles_ptr + 4, 1).unwrap();
        memory.write_u32_be(triangles_ptr + 8, 2).unwrap();
    }

    fn diffuse_attribute(attribute_set: u32) -> PpcQ3AttributeRecord {
        let mut data = Vec::new();
        for value in [1.0f32, 1.0, 1.0] {
            data.extend_from_slice(&value.to_bits().to_be_bytes());
        }
        PpcQ3AttributeRecord {
            attribute_set,
            attribute_type: PPC_Q3_ATTRIBUTE_TYPE_DIFFUSE_COLOR,
            data,
        }
    }

    fn render_with_light(
        light_type: u32,
        light_kind: PpcQ3LightKind,
    ) -> (PpcLoadedApp, u32, PpcQ3SoftwareRenderStats) {
        let view = PPC_Q3_OBJECT_BASE;
        let attribute_set = PPC_Q3_OBJECT_BASE + PPC_Q3_OBJECT_STRIDE;
        let light = PPC_Q3_OBJECT_BASE + PPC_Q3_OBJECT_STRIDE * 2;
        let trimesh_data = PPC_DATA_BASE + 0x1000;
        let points_ptr = PPC_DATA_BASE + 0x1080;
        let triangles_ptr = PPC_DATA_BASE + 0x1100;
        let vertex_attrs_ptr = PPC_DATA_BASE + 0x1180;
        let normals_ptr = PPC_DATA_BASE + 0x11c0;
        let front_base = PPC_HEAP_BASE + 0x1000;
        let pef = synthetic_pef_with_library_import(b"QuickDraw\xaa 3D", b"Q3TriMesh_Submit");
        let mut loaded = load_pef_application(&pef).unwrap();
        loaded.memory.add_region(trimesh_data, vec![0; 0x400]);
        loaded.memory.add_region(front_base, vec![0; 8 * 8 * 2]);
        loaded.gworlds = vec![PpcGWorldRecord {
            ui_theme: crate::ui_theme::UiThemeId::ClassicSystem7,
            port: PPC_MAIN_GWORLD,
            pixmap_handle: 0,
            pixmap: 0,
            base_addr: front_base,
            gdevice: PPC_MAIN_GDEVICE,
            width: 8,
            height: 8,
            depth: 16,
            row_bytes: 16,
            pixels_locked: false,
            pixels_no_purge: false,
        }];
        loaded
            .current_gworld
            .with_mut(|current_gworld| *current_gworld = PPC_MAIN_GWORLD);
        write_lit_triangle_mesh(
            &mut loaded.memory,
            trimesh_data,
            points_ptr,
            triangles_ptr,
            vertex_attrs_ptr,
            normals_ptr,
        );
        loaded.q3_views.push(PpcQ3ViewStateRecord::new(view));
        loaded.q3_submissions.push(PpcQ3SubmissionRecord {
            view,
            kind: PpcQ3SubmissionKind::TriMesh,
            primary: trimesh_data,
            secondary: 0,
        });
        loaded
            .q3_submission_transforms
            .push(PpcQ3SubmissionTransformRecord {
                view,
                kind: PpcQ3SubmissionKind::TriMesh,
                primary: trimesh_data,
                secondary: 0,
                local_to_world: ppc_q3_matrix4x4_identity(),
            });
        loaded
            .q3_submission_materials
            .push(PpcQ3SubmissionMaterialRecord {
                view,
                kind: PpcQ3SubmissionKind::TriMesh,
                primary: trimesh_data,
                secondary: 0,
                shader: 0,
                illumination_type: PPC_Q3_ILLUMINATION_TYPE_PHONG,
                styles: Vec::new(),
                fog_style: None,
                attributes: vec![diffuse_attribute(attribute_set)],
                shader_uv_transform: None,
                shader_boundary: None,
                texture_shader: None,
                mipmap_texture: None,
            });
        loaded
            .q3_submission_lights
            .push(PpcQ3SubmissionLightRecord {
                view,
                kind: PpcQ3SubmissionKind::TriMesh,
                primary: trimesh_data,
                secondary: 0,
                light_group: 0,
                lights: vec![PpcQ3LightRecord {
                    light,
                    light_type,
                    data: PpcQ3LightData {
                        is_on: 1,
                        brightness: 1.0,
                        color: (0.0, 1.0, 0.0),
                    },
                    kind: light_kind,
                }],
            });

        let stats = loaded.render_q3_scene_commands_to_front_buffer();
        (loaded, front_base, stats)
    }

    fn read_pixel(loaded: &mut PpcLoadedApp, front_base: u32, x: u32, y: u32) -> u16 {
        loaded
            .memory
            .read_u16_be(front_base + y * 16 + x * 2)
            .unwrap()
    }

    fn green_component(pixel: u16) -> u16 {
        (pixel & 0x03e0) >> 5
    }

    fn any_nonzero_pixel(loaded: &mut PpcLoadedApp, front_base: u32) -> bool {
        (0..8).any(|y| (0..8).any(|x| read_pixel(loaded, front_base, x, y) != 0))
    }

    let (mut point_loaded, point_front_base, point_stats) = render_with_light(
        PPC_Q3_LIGHT_TYPE_POINT,
        PpcQ3LightKind::Point {
            casts_shadows: 0,
            attenuation: PPC_Q3_ATTENUATION_TYPE_NONE,
            location: (0.0, 0.0, 1.0),
        },
    );
    assert_eq!(point_stats.commands, 1);
    assert_eq!(point_stats.triangles, 1);
    assert!(point_stats.pixels > 0);
    let point_pixel = read_pixel(&mut point_loaded, point_front_base, 3, 4);
    assert_ne!(point_pixel & 0x03e0, 0);
    assert_eq!(point_pixel & 0x7c1f, 0);

    let (mut dim_point_loaded, dim_point_front_base, dim_point_stats) = render_with_light(
        PPC_Q3_LIGHT_TYPE_POINT,
        PpcQ3LightKind::Point {
            casts_shadows: 0,
            attenuation: PPC_Q3_ATTENUATION_TYPE_INVERSE_DISTANCE_SQUARED,
            location: (0.0, 0.0, 2.0),
        },
    );
    assert_eq!(dim_point_stats.commands, 1);
    assert_eq!(dim_point_stats.triangles, 1);
    assert!(dim_point_stats.pixels > 0);
    let dim_point_pixel = read_pixel(&mut dim_point_loaded, dim_point_front_base, 3, 4);
    assert!(green_component(dim_point_pixel) > 0);
    assert!(green_component(dim_point_pixel) < green_component(point_pixel));
    assert_eq!(dim_point_pixel & 0x7c1f, 0);

    let (mut spot_loaded, spot_front_base, spot_stats) = render_with_light(
        PPC_Q3_LIGHT_TYPE_SPOT,
        PpcQ3LightKind::Spot {
            casts_shadows: 0,
            attenuation: PPC_Q3_ATTENUATION_TYPE_NONE,
            location: (0.0, 0.0, 1.0),
            direction: (0.0, 0.0, -1.0),
            hot_angle: 0.7,
            outer_angle: 0.9,
            fall_off: PPC_Q3_FALL_OFF_TYPE_LINEAR,
        },
    );
    assert_eq!(spot_stats.commands, 1);
    assert_eq!(spot_stats.triangles, 1);
    assert!(spot_stats.pixels > 0);
    let spot_pixel = read_pixel(&mut spot_loaded, spot_front_base, 3, 4);
    assert_ne!(spot_pixel & 0x03e0, 0);
    assert_eq!(spot_pixel & 0x7c1f, 0);

    let (mut dim_spot_loaded, dim_spot_front_base, dim_spot_stats) = render_with_light(
        PPC_Q3_LIGHT_TYPE_SPOT,
        PpcQ3LightKind::Spot {
            casts_shadows: 0,
            attenuation: PPC_Q3_ATTENUATION_TYPE_INVERSE_DISTANCE,
            location: (0.0, 0.0, 2.0),
            direction: (0.0, 0.0, -1.0),
            hot_angle: 0.0,
            outer_angle: 1.2,
            fall_off: PPC_Q3_FALL_OFF_TYPE_LINEAR,
        },
    );
    assert_eq!(dim_spot_stats.commands, 1);
    assert_eq!(dim_spot_stats.triangles, 1);
    assert!(dim_spot_stats.pixels > 0);
    let dim_spot_pixel = read_pixel(&mut dim_spot_loaded, dim_spot_front_base, 3, 4);
    assert!(green_component(dim_spot_pixel) > 0);
    assert!(green_component(dim_spot_pixel) < green_component(spot_pixel));
    assert_eq!(dim_spot_pixel & 0x7c1f, 0);

    let (mut away_loaded, away_front_base, away_stats) = render_with_light(
        PPC_Q3_LIGHT_TYPE_SPOT,
        PpcQ3LightKind::Spot {
            casts_shadows: 0,
            attenuation: PPC_Q3_ATTENUATION_TYPE_NONE,
            location: (0.0, 0.0, 1.0),
            direction: (0.0, 0.0, 1.0),
            hot_angle: 0.7,
            outer_angle: 0.9,
            fall_off: PPC_Q3_FALL_OFF_TYPE_LINEAR,
        },
    );
    assert_eq!(away_stats.commands, 1);
    assert_eq!(away_stats.triangles, 1);
    assert!(away_stats.pixels > 0);
    assert!(!any_nonzero_pixel(&mut away_loaded, away_front_base));
}

#[test]
fn q3_software_renderer_respects_interpolation_style_shading_modes() {
    fn diffuse_attribute(attribute_set: u32) -> PpcQ3AttributeRecord {
        let mut data = Vec::new();
        for value in [1.0f32, 1.0, 1.0] {
            data.extend_from_slice(&value.to_bits().to_be_bytes());
        }
        PpcQ3AttributeRecord {
            attribute_set,
            attribute_type: PPC_Q3_ATTRIBUTE_TYPE_DIFFUSE_COLOR,
            data,
        }
    }

    fn render_with_interpolation_style(
        interpolation_style: u32,
    ) -> (PpcLoadedApp, u32, PpcQ3SoftwareRenderStats) {
        let view = PPC_Q3_OBJECT_BASE;
        let attribute_set = PPC_Q3_OBJECT_BASE + PPC_Q3_OBJECT_STRIDE;
        let style = PPC_Q3_OBJECT_BASE + PPC_Q3_OBJECT_STRIDE * 2;
        let directional_light = PPC_Q3_OBJECT_BASE + PPC_Q3_OBJECT_STRIDE * 3;
        let trimesh_data = PPC_DATA_BASE + 0x1000;
        let points_ptr = PPC_DATA_BASE + 0x1080;
        let triangles_ptr = PPC_DATA_BASE + 0x1100;
        let vertex_attrs_ptr = PPC_DATA_BASE + 0x1180;
        let normals_ptr = PPC_DATA_BASE + 0x11c0;
        let front_base = PPC_HEAP_BASE + 0x1000;
        let pef = synthetic_pef_with_library_import(b"QuickDraw\xaa 3D", b"Q3TriMesh_Submit");
        let mut loaded = load_pef_application(&pef).unwrap();
        loaded.memory.add_region(trimesh_data, vec![0; 0x400]);
        loaded.memory.add_region(front_base, vec![0; 8 * 8 * 2]);
        loaded.gworlds = vec![PpcGWorldRecord {
            ui_theme: crate::ui_theme::UiThemeId::ClassicSystem7,
            port: PPC_MAIN_GWORLD,
            pixmap_handle: 0,
            pixmap: 0,
            base_addr: front_base,
            gdevice: PPC_MAIN_GDEVICE,
            width: 8,
            height: 8,
            depth: 16,
            row_bytes: 16,
            pixels_locked: false,
            pixels_no_purge: false,
        }];
        loaded
            .current_gworld
            .with_mut(|current_gworld| *current_gworld = PPC_MAIN_GWORLD);

        loaded
            .memory
            .write_u32_be(trimesh_data + PPC_Q3_TRIMESH_NUM_TRIANGLES_OFFSET, 1)
            .unwrap();
        loaded
            .memory
            .write_u32_be(
                trimesh_data + PPC_Q3_TRIMESH_TRIANGLES_OFFSET,
                triangles_ptr,
            )
            .unwrap();
        loaded
            .memory
            .write_u32_be(trimesh_data + PPC_Q3_TRIMESH_NUM_POINTS_OFFSET, 3)
            .unwrap();
        loaded
            .memory
            .write_u32_be(trimesh_data + PPC_Q3_TRIMESH_POINTS_OFFSET, points_ptr)
            .unwrap();
        loaded
            .memory
            .write_u32_be(
                trimesh_data + PPC_Q3_TRIMESH_NUM_VERTEX_ATTRIBUTE_TYPES_OFFSET,
                1,
            )
            .unwrap();
        loaded
            .memory
            .write_u32_be(
                trimesh_data + PPC_Q3_TRIMESH_VERTEX_ATTRIBUTE_TYPES_OFFSET,
                vertex_attrs_ptr,
            )
            .unwrap();
        loaded
            .memory
            .write_u32_be(vertex_attrs_ptr, PPC_Q3_ATTRIBUTE_TYPE_NORMAL)
            .unwrap();
        loaded
            .memory
            .write_u32_be(vertex_attrs_ptr + 4, normals_ptr)
            .unwrap();
        loaded.memory.write_u32_be(vertex_attrs_ptr + 8, 0).unwrap();

        for (index, point) in [(-0.8, -0.8, 0.0), (0.8, -0.8, 0.0), (-0.8, 0.8, 0.0)]
            .into_iter()
            .enumerate()
        {
            ppc_write_q3_vector3d(
                &mut loaded.memory,
                points_ptr + index as u32 * PPC_Q3_POINT3D_SIZE,
                point,
            )
            .unwrap();
        }
        for (index, normal) in [(0.0, 0.0, 1.0), (0.0, 0.0, -1.0), (0.0, 0.0, -1.0)]
            .into_iter()
            .enumerate()
        {
            ppc_write_q3_vector3d(
                &mut loaded.memory,
                normals_ptr + index as u32 * PPC_Q3_POINT3D_SIZE,
                normal,
            )
            .unwrap();
        }
        loaded.memory.write_u32_be(triangles_ptr, 0).unwrap();
        loaded.memory.write_u32_be(triangles_ptr + 4, 1).unwrap();
        loaded.memory.write_u32_be(triangles_ptr + 8, 2).unwrap();

        loaded.q3_views.push(PpcQ3ViewStateRecord::new(view));
        loaded.q3_submissions.push(PpcQ3SubmissionRecord {
            view,
            kind: PpcQ3SubmissionKind::TriMesh,
            primary: trimesh_data,
            secondary: 0,
        });
        loaded
            .q3_submission_transforms
            .push(PpcQ3SubmissionTransformRecord {
                view,
                kind: PpcQ3SubmissionKind::TriMesh,
                primary: trimesh_data,
                secondary: 0,
                local_to_world: ppc_q3_matrix4x4_identity(),
            });
        loaded
            .q3_submission_materials
            .push(PpcQ3SubmissionMaterialRecord {
                view,
                kind: PpcQ3SubmissionKind::TriMesh,
                primary: trimesh_data,
                secondary: 0,
                shader: 0,
                illumination_type: PPC_Q3_ILLUMINATION_TYPE_PHONG,
                styles: vec![PpcQ3StyleRecord {
                    style,
                    kind: PpcQ3StyleKind::Interpolation,
                    value: interpolation_style,
                }],
                fog_style: None,
                attributes: vec![diffuse_attribute(attribute_set)],
                shader_uv_transform: None,
                shader_boundary: None,
                texture_shader: None,
                mipmap_texture: None,
            });
        loaded
            .q3_submission_lights
            .push(PpcQ3SubmissionLightRecord {
                view,
                kind: PpcQ3SubmissionKind::TriMesh,
                primary: trimesh_data,
                secondary: 0,
                light_group: 0,
                lights: vec![PpcQ3LightRecord {
                    light: directional_light,
                    light_type: PPC_Q3_LIGHT_TYPE_DIRECTIONAL,
                    data: PpcQ3LightData {
                        is_on: 1,
                        brightness: 1.0,
                        color: (0.0, 1.0, 0.0),
                    },
                    kind: PpcQ3LightKind::Directional {
                        casts_shadows: 0,
                        direction: (0.0, 0.0, -1.0),
                    },
                }],
            });

        let stats = loaded.render_q3_scene_commands_to_front_buffer();
        (loaded, front_base, stats)
    }

    let (mut flat_loaded, flat_front_base, flat_stats) =
        render_with_interpolation_style(PPC_Q3_INTERPOLATION_STYLE_NONE);
    assert_eq!(flat_stats.commands, 1);
    assert_eq!(flat_stats.triangles, 1);
    assert!(flat_stats.pixels > 0);
    assert_eq!(
        flat_loaded
            .memory
            .read_u16_be(flat_front_base + 5 * 16 + 2 * 2),
        Some(0)
    );

    let (mut vertex_loaded, vertex_front_base, vertex_stats) =
        render_with_interpolation_style(PPC_Q3_INTERPOLATION_STYLE_VERTEX);
    assert_eq!(vertex_stats.commands, 1);
    assert_eq!(vertex_stats.triangles, 1);
    assert!(vertex_stats.pixels > 0);
    let vertex_pixel = vertex_loaded
        .memory
        .read_u16_be(vertex_front_base + 5 * 16 + 2 * 2)
        .unwrap();
    assert_ne!(vertex_pixel, 0);
    assert_ne!(vertex_pixel, 0x03e0);
    assert_ne!(vertex_pixel & 0x03e0, 0);
    assert_eq!(vertex_pixel & 0x7c1f, 0);

    let (mut pixel_loaded, pixel_front_base, pixel_stats) =
        render_with_interpolation_style(PPC_Q3_INTERPOLATION_STYLE_PIXEL);
    assert_eq!(pixel_stats.commands, 1);
    assert_eq!(pixel_stats.triangles, 1);
    assert!(pixel_stats.pixels > 0);
    assert_eq!(
        pixel_loaded
            .memory
            .read_u16_be(pixel_front_base + 5 * 16 + 2 * 2),
        Some(0x03e0)
    );
}

#[test]
fn q3_software_renderer_applies_linear_fog_style_to_materials() {
    let view = PPC_Q3_OBJECT_BASE;
    let attribute_set = PPC_Q3_OBJECT_BASE + PPC_Q3_OBJECT_STRIDE;
    let trimesh_data = PPC_DATA_BASE + 0x1000;
    let points_ptr = PPC_DATA_BASE + 0x1080;
    let triangles_ptr = PPC_DATA_BASE + 0x1100;
    let front_base = PPC_HEAP_BASE + 0x1000;
    let pef = synthetic_pef_with_library_import(b"QuickDraw\xaa 3D", b"Q3TriMesh_Submit");
    let mut loaded = load_pef_application(&pef).unwrap();
    loaded.memory.add_region(trimesh_data, vec![0; 0x200]);
    loaded.memory.add_region(front_base, vec![0; 8 * 8 * 2]);
    loaded.gworlds = vec![PpcGWorldRecord {
        ui_theme: crate::ui_theme::UiThemeId::ClassicSystem7,
        port: PPC_MAIN_GWORLD,
        pixmap_handle: 0,
        pixmap: 0,
        base_addr: front_base,
        gdevice: PPC_MAIN_GDEVICE,
        width: 8,
        height: 8,
        depth: 16,
        row_bytes: 16,
        pixels_locked: false,
        pixels_no_purge: false,
    }];
    loaded
        .current_gworld
        .with_mut(|current_gworld| *current_gworld = PPC_MAIN_GWORLD);

    loaded
        .memory
        .write_u32_be(trimesh_data + PPC_Q3_TRIMESH_NUM_TRIANGLES_OFFSET, 1)
        .unwrap();
    loaded
        .memory
        .write_u32_be(
            trimesh_data + PPC_Q3_TRIMESH_TRIANGLES_OFFSET,
            triangles_ptr,
        )
        .unwrap();
    loaded
        .memory
        .write_u32_be(trimesh_data + PPC_Q3_TRIMESH_NUM_POINTS_OFFSET, 3)
        .unwrap();
    loaded
        .memory
        .write_u32_be(trimesh_data + PPC_Q3_TRIMESH_POINTS_OFFSET, points_ptr)
        .unwrap();
    ppc_write_q3_vector3d(&mut loaded.memory, points_ptr, (0.0, 0.0, 1.0)).unwrap();
    ppc_write_q3_vector3d(&mut loaded.memory, points_ptr + 12, (0.0, 0.9, 1.0)).unwrap();
    ppc_write_q3_vector3d(&mut loaded.memory, points_ptr + 24, (0.9, 0.0, 1.0)).unwrap();
    loaded.memory.write_u32_be(triangles_ptr, 0).unwrap();
    loaded.memory.write_u32_be(triangles_ptr + 4, 1).unwrap();
    loaded.memory.write_u32_be(triangles_ptr + 8, 2).unwrap();

    let mut diffuse = Vec::new();
    for value in [1.0f32, 0.0, 0.0] {
        diffuse.extend_from_slice(&value.to_bits().to_be_bytes());
    }
    loaded.q3_views.push(PpcQ3ViewStateRecord::new(view));
    loaded.q3_submissions.push(PpcQ3SubmissionRecord {
        view,
        kind: PpcQ3SubmissionKind::TriMesh,
        primary: trimesh_data,
        secondary: 0,
    });
    loaded
        .q3_submission_transforms
        .push(PpcQ3SubmissionTransformRecord {
            view,
            kind: PpcQ3SubmissionKind::TriMesh,
            primary: trimesh_data,
            secondary: 0,
            local_to_world: ppc_q3_matrix4x4_identity(),
        });
    loaded
        .q3_submission_materials
        .push(PpcQ3SubmissionMaterialRecord {
            view,
            kind: PpcQ3SubmissionKind::TriMesh,
            primary: trimesh_data,
            secondary: 0,
            shader: 0,
            illumination_type: PPC_Q3_ILLUMINATION_TYPE_PHONG,
            styles: Vec::new(),
            fog_style: Some(PpcQ3FogStyleData {
                state: 1,
                mode: PPC_Q3_FOG_MODE_LINEAR,
                fog_start: 0.0,
                fog_end: 1.0,
                density: 1.0,
                color: (0.0, 0.0, 1.0, 1.0),
            }),
            attributes: vec![PpcQ3AttributeRecord {
                attribute_set,
                attribute_type: PPC_Q3_ATTRIBUTE_TYPE_DIFFUSE_COLOR,
                data: diffuse,
            }],
            shader_uv_transform: None,
            shader_boundary: None,
            texture_shader: None,
            mipmap_texture: None,
        });
    loaded
        .q3_submission_lights
        .push(PpcQ3SubmissionLightRecord {
            view,
            kind: PpcQ3SubmissionKind::TriMesh,
            primary: trimesh_data,
            secondary: 0,
            light_group: 0,
            lights: Vec::new(),
        });

    let stats = loaded.render_q3_scene_commands_to_front_buffer();

    assert_eq!(stats.commands, 1);
    assert_eq!(stats.triangles, 1);
    assert!(stats.pixels > 0);
    assert_eq!(
        loaded.memory.read_u16_be(front_base + 4 * 16 + 4 * 2),
        Some(0x001f)
    );
}

#[test]
fn q3_software_renderer_applies_exponential_fog_modes_to_materials() {
    fn assert_close(actual: f32, expected: f32) {
        assert!(
            (actual - expected).abs() < 0.0001,
            "expected {expected}, got {actual}"
        );
    }

    let view = PPC_Q3_OBJECT_BASE;
    let lights = PpcQ3SubmissionLightRecord {
        view,
        kind: PpcQ3SubmissionKind::TriMesh,
        primary: PPC_DATA_BASE,
        secondary: 0,
        light_group: 0,
        lights: Vec::new(),
    };
    let sample = PpcQ3SoftwareMaterialSample {
        diffuse: (1.0, 0.0, 0.0),
        ambient_coefficient: 1.0,
        specular_color: (0.0, 0.0, 0.0),
        specular_control: 0.0,
        highlight_state: true,
        transparency: (1.0, 1.0, 1.0),
        illumination_type: PPC_Q3_ILLUMINATION_TYPE_PHONG,
        texture: None,
        uv_transform: None,
        shader_boundary: None,
        fog_style: None,
    };
    let mut memory = PpcSectionMem::new();

    for (mode, expected_factor) in [
        (PPC_Q3_FOG_MODE_EXPONENTIAL, (-0.5f32).exp()),
        (PPC_Q3_FOG_MODE_EXPONENTIAL_SQUARED, (-0.25f32).exp()),
    ] {
        let color = ppc_q3_software_material_color(
            &mut memory,
            &PpcQ3SoftwareMaterialSample {
                fog_style: Some(PpcQ3FogStyleData {
                    state: 1,
                    mode,
                    fog_start: 0.0,
                    fog_end: 1.0,
                    density: 0.5,
                    color: (0.0, 0.0, 1.0, 1.0),
                }),
                ..sample
            },
            &lights,
            None,
            None,
            None,
            None,
            None,
            1.0,
            None,
        );

        assert_close(color.0, expected_factor);
        assert_close(color.1, 0.0);
        assert_close(color.2, 1.0 - expected_factor);
    }
}

#[test]
fn q3_software_renderer_applies_alpha_fog_from_vertex_transparency() {
    fn color_attribute(
        attribute_set: u32,
        attribute_type: u32,
        rgb: [f32; 3],
    ) -> PpcQ3AttributeRecord {
        let mut data = Vec::new();
        for value in rgb {
            data.extend_from_slice(&value.to_bits().to_be_bytes());
        }
        PpcQ3AttributeRecord {
            attribute_set,
            attribute_type,
            data,
        }
    }

    let view = PPC_Q3_OBJECT_BASE;
    let attribute_set = PPC_Q3_OBJECT_BASE + PPC_Q3_OBJECT_STRIDE;
    let trimesh_data = PPC_DATA_BASE + 0x1000;
    let points_ptr = PPC_DATA_BASE + 0x1080;
    let triangles_ptr = PPC_DATA_BASE + 0x1100;
    let vertex_attrs_ptr = PPC_DATA_BASE + 0x1180;
    let vertex_transparency_ptr = PPC_DATA_BASE + 0x1200;
    let front_base = PPC_HEAP_BASE + 0x1000;
    let pef = synthetic_pef_with_library_import(b"QuickDraw\xaa 3D", b"Q3TriMesh_Submit");
    let mut loaded = load_pef_application(&pef).unwrap();
    loaded.memory.add_region(trimesh_data, vec![0; 0x400]);
    loaded.memory.add_region(front_base, vec![0; 8 * 8 * 2]);
    loaded.gworlds = vec![PpcGWorldRecord {
        ui_theme: crate::ui_theme::UiThemeId::ClassicSystem7,
        port: PPC_MAIN_GWORLD,
        pixmap_handle: 0,
        pixmap: 0,
        base_addr: front_base,
        gdevice: PPC_MAIN_GDEVICE,
        width: 8,
        height: 8,
        depth: 16,
        row_bytes: 16,
        pixels_locked: false,
        pixels_no_purge: false,
    }];
    loaded
        .current_gworld
        .with_mut(|current_gworld| *current_gworld = PPC_MAIN_GWORLD);

    loaded
        .memory
        .write_u32_be(trimesh_data + PPC_Q3_TRIMESH_NUM_TRIANGLES_OFFSET, 1)
        .unwrap();
    loaded
        .memory
        .write_u32_be(
            trimesh_data + PPC_Q3_TRIMESH_TRIANGLES_OFFSET,
            triangles_ptr,
        )
        .unwrap();
    loaded
        .memory
        .write_u32_be(trimesh_data + PPC_Q3_TRIMESH_NUM_POINTS_OFFSET, 3)
        .unwrap();
    loaded
        .memory
        .write_u32_be(trimesh_data + PPC_Q3_TRIMESH_POINTS_OFFSET, points_ptr)
        .unwrap();
    loaded
        .memory
        .write_u32_be(
            trimesh_data + PPC_Q3_TRIMESH_NUM_VERTEX_ATTRIBUTE_TYPES_OFFSET,
            1,
        )
        .unwrap();
    loaded
        .memory
        .write_u32_be(
            trimesh_data + PPC_Q3_TRIMESH_VERTEX_ATTRIBUTE_TYPES_OFFSET,
            vertex_attrs_ptr,
        )
        .unwrap();
    loaded
        .memory
        .write_u32_be(vertex_attrs_ptr, PPC_Q3_ATTRIBUTE_TYPE_TRANSPARENCY_COLOR)
        .unwrap();
    loaded
        .memory
        .write_u32_be(vertex_attrs_ptr + 4, vertex_transparency_ptr)
        .unwrap();
    loaded.memory.write_u32_be(vertex_attrs_ptr + 8, 0).unwrap();
    ppc_write_q3_vector3d(&mut loaded.memory, points_ptr, (0.0, 0.0, 100.0)).unwrap();
    ppc_write_q3_vector3d(&mut loaded.memory, points_ptr + 12, (0.0, 0.9, 100.0)).unwrap();
    ppc_write_q3_vector3d(&mut loaded.memory, points_ptr + 24, (0.9, 0.0, 100.0)).unwrap();
    for index in 0..3 {
        ppc_write_q3_color_rgb(
            &mut loaded.memory,
            vertex_transparency_ptr + index * PPC_Q3_POINT3D_SIZE,
            (0.25, 0.25, 0.25),
        )
        .unwrap();
    }
    loaded.memory.write_u32_be(triangles_ptr, 0).unwrap();
    loaded.memory.write_u32_be(triangles_ptr + 4, 1).unwrap();
    loaded.memory.write_u32_be(triangles_ptr + 8, 2).unwrap();

    loaded.q3_views.push(PpcQ3ViewStateRecord::new(view));
    loaded.q3_submissions.push(PpcQ3SubmissionRecord {
        view,
        kind: PpcQ3SubmissionKind::TriMesh,
        primary: trimesh_data,
        secondary: 0,
    });
    loaded
        .q3_submission_transforms
        .push(PpcQ3SubmissionTransformRecord {
            view,
            kind: PpcQ3SubmissionKind::TriMesh,
            primary: trimesh_data,
            secondary: 0,
            local_to_world: ppc_q3_matrix4x4_identity(),
        });
    loaded
        .q3_submission_materials
        .push(PpcQ3SubmissionMaterialRecord {
            view,
            kind: PpcQ3SubmissionKind::TriMesh,
            primary: trimesh_data,
            secondary: 0,
            shader: 0,
            illumination_type: PPC_Q3_ILLUMINATION_TYPE_PHONG,
            styles: Vec::new(),
            fog_style: Some(PpcQ3FogStyleData {
                state: 1,
                mode: PPC_Q3_FOG_MODE_ALPHA,
                fog_start: 0.0,
                fog_end: 1.0,
                density: 1.0,
                color: (0.0, 0.0, 1.0, 1.0),
            }),
            attributes: vec![color_attribute(
                attribute_set,
                PPC_Q3_ATTRIBUTE_TYPE_DIFFUSE_COLOR,
                [1.0, 0.0, 0.0],
            )],
            shader_uv_transform: None,
            shader_boundary: None,
            texture_shader: None,
            mipmap_texture: None,
        });
    loaded
        .q3_submission_lights
        .push(PpcQ3SubmissionLightRecord {
            view,
            kind: PpcQ3SubmissionKind::TriMesh,
            primary: trimesh_data,
            secondary: 0,
            light_group: 0,
            lights: Vec::new(),
        });

    let stats = loaded.render_q3_scene_commands_to_front_buffer();

    assert_eq!(stats.commands, 1);
    assert_eq!(stats.triangles, 1);
    assert!(stats.pixels > 0);
    assert_eq!(
        loaded.memory.read_u16_be(front_base + 4 * 16 + 4 * 2),
        Some(0x2017)
    );
    assert_ne!(
        loaded.memory.read_u16_be(front_base + 4 * 16 + 4 * 2),
        Some(0x7c00)
    );
    assert_ne!(
        loaded.memory.read_u16_be(front_base + 4 * 16 + 4 * 2),
        Some(0x001f)
    );
}

#[test]
fn q3_software_renderer_applies_captured_mipmap_texture_color() {
    let view = PPC_Q3_OBJECT_BASE;
    let attribute_set = PPC_Q3_OBJECT_BASE + PPC_Q3_OBJECT_STRIDE;
    let texture = PPC_Q3_OBJECT_BASE + PPC_Q3_OBJECT_STRIDE * 2;
    let shader = PPC_Q3_OBJECT_BASE + PPC_Q3_OBJECT_STRIDE * 3;
    let storage = PPC_Q3_OBJECT_BASE + PPC_Q3_OBJECT_STRIDE * 4;
    let trimesh_data = PPC_DATA_BASE + 0x1000;
    let points_ptr = PPC_DATA_BASE + 0x1080;
    let triangles_ptr = PPC_DATA_BASE + 0x1100;
    let image_ptr = PPC_DATA_BASE + 0x1200;
    let front_base = PPC_HEAP_BASE + 0x1000;
    let pef = synthetic_pef_with_library_import(b"QuickDraw\xaa 3D", b"Q3TriMesh_Submit");
    let mut loaded = load_pef_application(&pef).unwrap();
    loaded.memory.add_region(trimesh_data, vec![0; 0x400]);
    loaded.memory.add_region(front_base, vec![0; 8 * 8 * 2]);
    loaded.gworlds = vec![PpcGWorldRecord {
        ui_theme: crate::ui_theme::UiThemeId::ClassicSystem7,
        port: PPC_MAIN_GWORLD,
        pixmap_handle: 0,
        pixmap: 0,
        base_addr: front_base,
        gdevice: PPC_MAIN_GDEVICE,
        width: 8,
        height: 8,
        depth: 16,
        row_bytes: 16,
        pixels_locked: false,
        pixels_no_purge: false,
    }];
    loaded
        .current_gworld
        .with_mut(|current_gworld| *current_gworld = PPC_MAIN_GWORLD);

    loaded
        .memory
        .write_u32_be(trimesh_data + PPC_Q3_TRIMESH_NUM_TRIANGLES_OFFSET, 1)
        .unwrap();
    loaded
        .memory
        .write_u32_be(
            trimesh_data + PPC_Q3_TRIMESH_TRIANGLES_OFFSET,
            triangles_ptr,
        )
        .unwrap();
    loaded
        .memory
        .write_u32_be(trimesh_data + PPC_Q3_TRIMESH_NUM_POINTS_OFFSET, 3)
        .unwrap();
    loaded
        .memory
        .write_u32_be(trimesh_data + PPC_Q3_TRIMESH_POINTS_OFFSET, points_ptr)
        .unwrap();
    ppc_write_q3_vector3d(&mut loaded.memory, points_ptr, (0.0, 0.0, 0.0)).unwrap();
    ppc_write_q3_vector3d(&mut loaded.memory, points_ptr + 12, (0.0, 0.9, 0.0)).unwrap();
    ppc_write_q3_vector3d(&mut loaded.memory, points_ptr + 24, (0.9, 0.0, 0.0)).unwrap();
    loaded.memory.write_u32_be(triangles_ptr, 0).unwrap();
    loaded.memory.write_u32_be(triangles_ptr + 4, 1).unwrap();
    loaded.memory.write_u32_be(triangles_ptr + 8, 2).unwrap();
    loaded.memory.write_u8(image_ptr, 0).unwrap();
    loaded.memory.write_u8(image_ptr + 1, 0).unwrap();
    loaded.memory.write_u8(image_ptr + 2, 255).unwrap();

    let mut diffuse = Vec::new();
    for value in [1.0f32, 1.0, 1.0] {
        diffuse.extend_from_slice(&value.to_bits().to_be_bytes());
    }
    let mut mipmap = vec![0; PPC_Q3_MIPMAP_COPY_SIZE as usize];
    mipmap[PPC_Q3_MIPMAP_IMAGE_OFFSET as usize..][..4].copy_from_slice(&storage.to_be_bytes());
    mipmap[PPC_Q3_MIPMAP_PIXEL_TYPE_OFFSET as usize..][..4]
        .copy_from_slice(&PPC_Q3_PIXEL_TYPE_RGB24.to_be_bytes());
    mipmap[PPC_Q3_MIPMAP_BYTE_ORDER_OFFSET as usize..][..4]
        .copy_from_slice(&PPC_Q3_ENDIAN_BIG.to_be_bytes());
    mipmap[PPC_Q3_MIPMAP_BASE_WIDTH_OFFSET as usize..][..4]
        .copy_from_slice(&1u32.to_be_bytes());
    mipmap[PPC_Q3_MIPMAP_BASE_HEIGHT_OFFSET as usize..][..4]
        .copy_from_slice(&1u32.to_be_bytes());
    mipmap[PPC_Q3_MIPMAP_BASE_ROW_BYTES_OFFSET as usize..][..4]
        .copy_from_slice(&3u32.to_be_bytes());
    mipmap[PPC_Q3_MIPMAP_BASE_IMAGE_OFFSET_OFFSET as usize..][..4]
        .copy_from_slice(&0u32.to_be_bytes());

    loaded.q3_memory_storages.push(PpcQ3MemoryStorageRecord {
        storage,
        buffer_ptr: image_ptr,
        valid_size: 3,
        buffer_size: 3,
        owns_buffer: false,
    });
    loaded.q3_views.push(PpcQ3ViewStateRecord::new(view));
    loaded.q3_submissions.push(PpcQ3SubmissionRecord {
        view,
        kind: PpcQ3SubmissionKind::TriMesh,
        primary: trimesh_data,
        secondary: 0,
    });
    loaded
        .q3_submission_transforms
        .push(PpcQ3SubmissionTransformRecord {
            view,
            kind: PpcQ3SubmissionKind::TriMesh,
            primary: trimesh_data,
            secondary: 0,
            local_to_world: ppc_q3_matrix4x4_identity(),
        });
    loaded
        .q3_submission_materials
        .push(PpcQ3SubmissionMaterialRecord {
            view,
            kind: PpcQ3SubmissionKind::TriMesh,
            primary: trimesh_data,
            secondary: 0,
            shader,
            illumination_type: PPC_Q3_ILLUMINATION_TYPE_PHONG,
            styles: Vec::new(),
            fog_style: None,
            attributes: vec![PpcQ3AttributeRecord {
                attribute_set,
                attribute_type: PPC_Q3_ATTRIBUTE_TYPE_DIFFUSE_COLOR,
                data: diffuse,
            }],
            shader_uv_transform: None,
            shader_boundary: None,
            texture_shader: Some(PpcQ3TextureShaderRecord { shader, texture }),
            mipmap_texture: Some(PpcQ3MipmapTextureRecord { texture, mipmap }),
        });
    loaded
        .q3_submission_lights
        .push(PpcQ3SubmissionLightRecord {
            view,
            kind: PpcQ3SubmissionKind::TriMesh,
            primary: trimesh_data,
            secondary: 0,
            light_group: 0,
            lights: Vec::new(),
        });

    let stats = loaded.render_q3_scene_commands_to_front_buffer();

    assert_eq!(stats.commands, 1);
    assert_eq!(stats.triangles, 1);
    assert!(stats.pixels > 0);
    assert_eq!(
        loaded.memory.read_u16_be(front_base + 4 * 16 + 4 * 2),
        Some(0x001f)
    );
}

#[test]
fn q3_software_renderer_blends_argb_texture_alpha() {
    let view = PPC_Q3_OBJECT_BASE;
    let attribute_set = PPC_Q3_OBJECT_BASE + PPC_Q3_OBJECT_STRIDE;
    let texture = PPC_Q3_OBJECT_BASE + PPC_Q3_OBJECT_STRIDE * 2;
    let shader = PPC_Q3_OBJECT_BASE + PPC_Q3_OBJECT_STRIDE * 3;
    let storage = PPC_Q3_OBJECT_BASE + PPC_Q3_OBJECT_STRIDE * 4;
    let trimesh_data = PPC_DATA_BASE + 0x1000;
    let points_ptr = PPC_DATA_BASE + 0x1080;
    let triangles_ptr = PPC_DATA_BASE + 0x1100;
    let image_ptr = PPC_DATA_BASE + 0x1200;
    let front_base = PPC_HEAP_BASE + 0x1000;
    let pef = synthetic_pef_with_library_import(b"QuickDraw\xaa 3D", b"Q3TriMesh_Submit");
    let mut loaded = load_pef_application(&pef).unwrap();
    let mut front_buffer_bytes = Vec::new();
    for _ in 0..64 {
        front_buffer_bytes.extend_from_slice(&0x001fu16.to_be_bytes());
    }
    loaded.memory.add_region(trimesh_data, vec![0; 0x400]);
    loaded.memory.add_region(front_base, front_buffer_bytes);
    loaded.gworlds = vec![PpcGWorldRecord {
        ui_theme: crate::ui_theme::UiThemeId::ClassicSystem7,
        port: PPC_MAIN_GWORLD,
        pixmap_handle: 0,
        pixmap: 0,
        base_addr: front_base,
        gdevice: PPC_MAIN_GDEVICE,
        width: 8,
        height: 8,
        depth: 16,
        row_bytes: 16,
        pixels_locked: false,
        pixels_no_purge: false,
    }];
    loaded
        .current_gworld
        .with_mut(|current_gworld| *current_gworld = PPC_MAIN_GWORLD);

    loaded
        .memory
        .write_u32_be(trimesh_data + PPC_Q3_TRIMESH_NUM_TRIANGLES_OFFSET, 1)
        .unwrap();
    loaded
        .memory
        .write_u32_be(
            trimesh_data + PPC_Q3_TRIMESH_TRIANGLES_OFFSET,
            triangles_ptr,
        )
        .unwrap();
    loaded
        .memory
        .write_u32_be(trimesh_data + PPC_Q3_TRIMESH_NUM_POINTS_OFFSET, 3)
        .unwrap();
    loaded
        .memory
        .write_u32_be(trimesh_data + PPC_Q3_TRIMESH_POINTS_OFFSET, points_ptr)
        .unwrap();
    ppc_write_q3_vector3d(&mut loaded.memory, points_ptr, (0.0, 0.0, 0.0)).unwrap();
    ppc_write_q3_vector3d(&mut loaded.memory, points_ptr + 12, (0.0, 0.9, 0.0)).unwrap();
    ppc_write_q3_vector3d(&mut loaded.memory, points_ptr + 24, (0.9, 0.0, 0.0)).unwrap();
    loaded.memory.write_u32_be(triangles_ptr, 0).unwrap();
    loaded.memory.write_u32_be(triangles_ptr + 4, 1).unwrap();
    loaded.memory.write_u32_be(triangles_ptr + 8, 2).unwrap();
    loaded.memory.write_u8(image_ptr, 64).unwrap();
    loaded.memory.write_u8(image_ptr + 1, 255).unwrap();
    loaded.memory.write_u8(image_ptr + 2, 0).unwrap();
    loaded.memory.write_u8(image_ptr + 3, 0).unwrap();

    let mut diffuse = Vec::new();
    for value in [1.0f32, 1.0, 1.0] {
        diffuse.extend_from_slice(&value.to_bits().to_be_bytes());
    }
    let mut mipmap = vec![0; PPC_Q3_MIPMAP_COPY_SIZE as usize];
    mipmap[PPC_Q3_MIPMAP_IMAGE_OFFSET as usize..][..4].copy_from_slice(&storage.to_be_bytes());
    mipmap[PPC_Q3_MIPMAP_PIXEL_TYPE_OFFSET as usize..][..4]
        .copy_from_slice(&PPC_Q3_PIXEL_TYPE_ARGB32.to_be_bytes());
    mipmap[PPC_Q3_MIPMAP_BYTE_ORDER_OFFSET as usize..][..4]
        .copy_from_slice(&PPC_Q3_ENDIAN_BIG.to_be_bytes());
    mipmap[PPC_Q3_MIPMAP_BASE_WIDTH_OFFSET as usize..][..4]
        .copy_from_slice(&1u32.to_be_bytes());
    mipmap[PPC_Q3_MIPMAP_BASE_HEIGHT_OFFSET as usize..][..4]
        .copy_from_slice(&1u32.to_be_bytes());
    mipmap[PPC_Q3_MIPMAP_BASE_ROW_BYTES_OFFSET as usize..][..4]
        .copy_from_slice(&4u32.to_be_bytes());
    mipmap[PPC_Q3_MIPMAP_BASE_IMAGE_OFFSET_OFFSET as usize..][..4]
        .copy_from_slice(&0u32.to_be_bytes());

    loaded.q3_memory_storages.push(PpcQ3MemoryStorageRecord {
        storage,
        buffer_ptr: image_ptr,
        valid_size: 4,
        buffer_size: 4,
        owns_buffer: false,
    });
    loaded.q3_views.push(PpcQ3ViewStateRecord::new(view));
    loaded.q3_submissions.push(PpcQ3SubmissionRecord {
        view,
        kind: PpcQ3SubmissionKind::TriMesh,
        primary: trimesh_data,
        secondary: 0,
    });
    loaded
        .q3_submission_transforms
        .push(PpcQ3SubmissionTransformRecord {
            view,
            kind: PpcQ3SubmissionKind::TriMesh,
            primary: trimesh_data,
            secondary: 0,
            local_to_world: ppc_q3_matrix4x4_identity(),
        });
    loaded
        .q3_submission_materials
        .push(PpcQ3SubmissionMaterialRecord {
            view,
            kind: PpcQ3SubmissionKind::TriMesh,
            primary: trimesh_data,
            secondary: 0,
            shader,
            illumination_type: PPC_Q3_ILLUMINATION_TYPE_PHONG,
            styles: Vec::new(),
            fog_style: None,
            attributes: vec![PpcQ3AttributeRecord {
                attribute_set,
                attribute_type: PPC_Q3_ATTRIBUTE_TYPE_DIFFUSE_COLOR,
                data: diffuse,
            }],
            shader_uv_transform: None,
            shader_boundary: None,
            texture_shader: Some(PpcQ3TextureShaderRecord { shader, texture }),
            mipmap_texture: Some(PpcQ3MipmapTextureRecord { texture, mipmap }),
        });
    loaded
        .q3_submission_lights
        .push(PpcQ3SubmissionLightRecord {
            view,
            kind: PpcQ3SubmissionKind::TriMesh,
            primary: trimesh_data,
            secondary: 0,
            light_group: 0,
            lights: Vec::new(),
        });

    let stats = loaded.render_q3_scene_commands_to_front_buffer();

    assert_eq!(stats.commands, 1);
    assert_eq!(stats.triangles, 1);
    assert!(stats.pixels > 0);
    assert_eq!(
        loaded.memory.read_u16_be(front_base + 4 * 16 + 4 * 2),
        Some(0x2017)
    );
    assert_ne!(
        loaded.memory.read_u16_be(front_base + 4 * 16 + 4 * 2),
        Some(0x7c00)
    );
    assert_ne!(
        loaded.memory.read_u16_be(front_base + 4 * 16 + 4 * 2),
        Some(0x001f)
    );
}

#[test]
fn q3_software_renderer_composes_texture_material_and_vertex_opacity() {
    fn color_attribute(
        attribute_set: u32,
        attribute_type: u32,
        rgb: [f32; 3],
    ) -> PpcQ3AttributeRecord {
        let mut data = Vec::new();
        for value in rgb {
            data.extend_from_slice(&value.to_bits().to_be_bytes());
        }
        PpcQ3AttributeRecord {
            attribute_set,
            attribute_type,
            data,
        }
    }

    let view = PPC_Q3_OBJECT_BASE;
    let attribute_set = PPC_Q3_OBJECT_BASE + PPC_Q3_OBJECT_STRIDE;
    let texture = PPC_Q3_OBJECT_BASE + PPC_Q3_OBJECT_STRIDE * 2;
    let shader = PPC_Q3_OBJECT_BASE + PPC_Q3_OBJECT_STRIDE * 3;
    let storage = PPC_Q3_OBJECT_BASE + PPC_Q3_OBJECT_STRIDE * 4;
    let trimesh_data = PPC_DATA_BASE + 0x1000;
    let points_ptr = PPC_DATA_BASE + 0x1080;
    let triangles_ptr = PPC_DATA_BASE + 0x1100;
    let vertex_attrs_ptr = PPC_DATA_BASE + 0x1180;
    let vertex_alphas_ptr = PPC_DATA_BASE + 0x11c0;
    let image_ptr = PPC_DATA_BASE + 0x1240;
    let front_base = PPC_HEAP_BASE + 0x1000;
    let pef = synthetic_pef_with_library_import(b"QuickDraw\xaa 3D", b"Q3TriMesh_Submit");
    let mut loaded = load_pef_application(&pef).unwrap();
    let mut front_buffer_bytes = Vec::new();
    for _ in 0..64 {
        front_buffer_bytes.extend_from_slice(&0x001fu16.to_be_bytes());
    }
    loaded.memory.add_region(trimesh_data, vec![0; 0x400]);
    loaded.memory.add_region(front_base, front_buffer_bytes);
    loaded.gworlds = vec![PpcGWorldRecord {
        ui_theme: crate::ui_theme::UiThemeId::ClassicSystem7,
        port: PPC_MAIN_GWORLD,
        pixmap_handle: 0,
        pixmap: 0,
        base_addr: front_base,
        gdevice: PPC_MAIN_GDEVICE,
        width: 8,
        height: 8,
        depth: 16,
        row_bytes: 16,
        pixels_locked: false,
        pixels_no_purge: false,
    }];
    loaded
        .current_gworld
        .with_mut(|current_gworld| *current_gworld = PPC_MAIN_GWORLD);

    loaded
        .memory
        .write_u32_be(trimesh_data + PPC_Q3_TRIMESH_NUM_TRIANGLES_OFFSET, 1)
        .unwrap();
    loaded
        .memory
        .write_u32_be(
            trimesh_data + PPC_Q3_TRIMESH_TRIANGLES_OFFSET,
            triangles_ptr,
        )
        .unwrap();
    loaded
        .memory
        .write_u32_be(trimesh_data + PPC_Q3_TRIMESH_NUM_POINTS_OFFSET, 3)
        .unwrap();
    loaded
        .memory
        .write_u32_be(trimesh_data + PPC_Q3_TRIMESH_POINTS_OFFSET, points_ptr)
        .unwrap();
    loaded
        .memory
        .write_u32_be(
            trimesh_data + PPC_Q3_TRIMESH_NUM_VERTEX_ATTRIBUTE_TYPES_OFFSET,
            1,
        )
        .unwrap();
    loaded
        .memory
        .write_u32_be(
            trimesh_data + PPC_Q3_TRIMESH_VERTEX_ATTRIBUTE_TYPES_OFFSET,
            vertex_attrs_ptr,
        )
        .unwrap();
    loaded
        .memory
        .write_u32_be(vertex_attrs_ptr, PPC_Q3_ATTRIBUTE_TYPE_TRANSPARENCY_COLOR)
        .unwrap();
    loaded
        .memory
        .write_u32_be(vertex_attrs_ptr + 4, vertex_alphas_ptr)
        .unwrap();
    loaded.memory.write_u32_be(vertex_attrs_ptr + 8, 0).unwrap();
    ppc_write_q3_vector3d(&mut loaded.memory, points_ptr, (0.0, 0.0, 0.0)).unwrap();
    ppc_write_q3_vector3d(&mut loaded.memory, points_ptr + 12, (0.0, 0.9, 0.0)).unwrap();
    ppc_write_q3_vector3d(&mut loaded.memory, points_ptr + 24, (0.9, 0.0, 0.0)).unwrap();
    for index in 0..3 {
        ppc_write_q3_color_rgb(
            &mut loaded.memory,
            vertex_alphas_ptr + index * PPC_Q3_POINT3D_SIZE,
            (0.5, 0.5, 0.5),
        )
        .unwrap();
    }
    loaded.memory.write_u32_be(triangles_ptr, 0).unwrap();
    loaded.memory.write_u32_be(triangles_ptr + 4, 1).unwrap();
    loaded.memory.write_u32_be(triangles_ptr + 8, 2).unwrap();
    loaded.memory.write_u8(image_ptr, 128).unwrap();
    loaded.memory.write_u8(image_ptr + 1, 255).unwrap();
    loaded.memory.write_u8(image_ptr + 2, 0).unwrap();
    loaded.memory.write_u8(image_ptr + 3, 0).unwrap();

    let mut mipmap = vec![0; PPC_Q3_MIPMAP_COPY_SIZE as usize];
    mipmap[PPC_Q3_MIPMAP_IMAGE_OFFSET as usize..][..4].copy_from_slice(&storage.to_be_bytes());
    mipmap[PPC_Q3_MIPMAP_PIXEL_TYPE_OFFSET as usize..][..4]
        .copy_from_slice(&PPC_Q3_PIXEL_TYPE_ARGB32.to_be_bytes());
    mipmap[PPC_Q3_MIPMAP_BYTE_ORDER_OFFSET as usize..][..4]
        .copy_from_slice(&PPC_Q3_ENDIAN_BIG.to_be_bytes());
    mipmap[PPC_Q3_MIPMAP_BASE_WIDTH_OFFSET as usize..][..4]
        .copy_from_slice(&1u32.to_be_bytes());
    mipmap[PPC_Q3_MIPMAP_BASE_HEIGHT_OFFSET as usize..][..4]
        .copy_from_slice(&1u32.to_be_bytes());
    mipmap[PPC_Q3_MIPMAP_BASE_ROW_BYTES_OFFSET as usize..][..4]
        .copy_from_slice(&4u32.to_be_bytes());
    mipmap[PPC_Q3_MIPMAP_BASE_IMAGE_OFFSET_OFFSET as usize..][..4]
        .copy_from_slice(&0u32.to_be_bytes());

    loaded.q3_memory_storages.push(PpcQ3MemoryStorageRecord {
        storage,
        buffer_ptr: image_ptr,
        valid_size: 4,
        buffer_size: 4,
        owns_buffer: false,
    });
    loaded.q3_views.push(PpcQ3ViewStateRecord::new(view));
    loaded.q3_submissions.push(PpcQ3SubmissionRecord {
        view,
        kind: PpcQ3SubmissionKind::TriMesh,
        primary: trimesh_data,
        secondary: 0,
    });
    loaded
        .q3_submission_transforms
        .push(PpcQ3SubmissionTransformRecord {
            view,
            kind: PpcQ3SubmissionKind::TriMesh,
            primary: trimesh_data,
            secondary: 0,
            local_to_world: ppc_q3_matrix4x4_identity(),
        });
    loaded
        .q3_submission_materials
        .push(PpcQ3SubmissionMaterialRecord {
            view,
            kind: PpcQ3SubmissionKind::TriMesh,
            primary: trimesh_data,
            secondary: 0,
            shader,
            illumination_type: PPC_Q3_ILLUMINATION_TYPE_PHONG,
            styles: Vec::new(),
            fog_style: None,
            attributes: vec![
                color_attribute(attribute_set, PPC_Q3_ATTRIBUTE_TYPE_DIFFUSE_COLOR, [1.0; 3]),
                color_attribute(
                    attribute_set,
                    PPC_Q3_ATTRIBUTE_TYPE_TRANSPARENCY_COLOR,
                    [0.5; 3],
                ),
            ],
            shader_uv_transform: None,
            shader_boundary: None,
            texture_shader: Some(PpcQ3TextureShaderRecord { shader, texture }),
            mipmap_texture: Some(PpcQ3MipmapTextureRecord { texture, mipmap }),
        });
    loaded
        .q3_submission_lights
        .push(PpcQ3SubmissionLightRecord {
            view,
            kind: PpcQ3SubmissionKind::TriMesh,
            primary: trimesh_data,
            secondary: 0,
            light_group: 0,
            lights: Vec::new(),
        });

    let stats = loaded.render_q3_scene_commands_to_front_buffer();
    let texture_opacity = 128.0 / 255.0;
    let vertex_opacity = 0.5;
    let expected_transparency = ppc_q3_software_effective_transparency(
        (0.5, 0.5, 0.5),
        texture_opacity * vertex_opacity,
    );
    let expected = ppc_q3_rgb555(ppc_q3_blend_transparency_color(
        (1.0, 0.0, 0.0),
        (0.0, 0.0, 1.0),
        expected_transparency,
    ));

    assert_eq!(stats.commands, 1);
    assert_eq!(stats.triangles, 1);
    assert!(stats.pixels > 0);
    assert_eq!(
        loaded.memory.read_u16_be(front_base + 4 * 16 + 4 * 2),
        Some(expected)
    );
    assert_ne!(
        loaded.memory.read_u16_be(front_base + 4 * 16 + 4 * 2),
        Some(0x2017)
    );
    assert_ne!(
        loaded.memory.read_u16_be(front_base + 4 * 16 + 4 * 2),
        Some(0x001f)
    );
}

#[test]
fn q3_software_texture_reads_argb16_alpha_bit() {
    let opaque_red = ppc_q3_software_texture_pixel_color(
        PPC_Q3_PIXEL_TYPE_ARGB16,
        PPC_Q3_ENDIAN_BIG,
        &[0xfc, 0x00, 0x00, 0x00],
    )
    .unwrap();
    assert_eq!(opaque_red.color, (1.0, 0.0, 0.0));
    assert_eq!(opaque_red.opacity, 1.0);

    let transparent_red = ppc_q3_software_texture_pixel_color(
        PPC_Q3_PIXEL_TYPE_ARGB16,
        PPC_Q3_ENDIAN_BIG,
        &[0x7c, 0x00, 0x00, 0x00],
    )
    .unwrap();
    assert_eq!(transparent_red.color, (1.0, 0.0, 0.0));
    assert_eq!(transparent_red.opacity, 0.0);
}

#[test]
fn q3_software_renderer_samples_captured_mipmap_with_vertex_uvs() {
    let view = PPC_Q3_OBJECT_BASE;
    let attribute_set = PPC_Q3_OBJECT_BASE + PPC_Q3_OBJECT_STRIDE;
    let texture = PPC_Q3_OBJECT_BASE + PPC_Q3_OBJECT_STRIDE * 2;
    let shader = PPC_Q3_OBJECT_BASE + PPC_Q3_OBJECT_STRIDE * 3;
    let storage = PPC_Q3_OBJECT_BASE + PPC_Q3_OBJECT_STRIDE * 4;
    let trimesh_data = PPC_DATA_BASE + 0x1000;
    let points_ptr = PPC_DATA_BASE + 0x1080;
    let triangles_ptr = PPC_DATA_BASE + 0x1100;
    let vertex_attrs_ptr = PPC_DATA_BASE + 0x1180;
    let uvs_ptr = PPC_DATA_BASE + 0x11c0;
    let image_ptr = PPC_DATA_BASE + 0x1240;
    let front_base = PPC_HEAP_BASE + 0x1000;
    let pef = synthetic_pef_with_library_import(b"QuickDraw\xaa 3D", b"Q3TriMesh_Submit");
    let mut loaded = load_pef_application(&pef).unwrap();
    loaded.memory.add_region(trimesh_data, vec![0; 0x800]);
    loaded.memory.add_region(front_base, vec![0; 8 * 8 * 2]);
    loaded.gworlds = vec![PpcGWorldRecord {
        ui_theme: crate::ui_theme::UiThemeId::ClassicSystem7,
        port: PPC_MAIN_GWORLD,
        pixmap_handle: 0,
        pixmap: 0,
        base_addr: front_base,
        gdevice: PPC_MAIN_GDEVICE,
        width: 8,
        height: 8,
        depth: 16,
        row_bytes: 16,
        pixels_locked: false,
        pixels_no_purge: false,
    }];
    loaded
        .current_gworld
        .with_mut(|current_gworld| *current_gworld = PPC_MAIN_GWORLD);

    loaded
        .memory
        .write_u32_be(trimesh_data + PPC_Q3_TRIMESH_NUM_TRIANGLES_OFFSET, 1)
        .unwrap();
    loaded
        .memory
        .write_u32_be(
            trimesh_data + PPC_Q3_TRIMESH_TRIANGLES_OFFSET,
            triangles_ptr,
        )
        .unwrap();
    loaded
        .memory
        .write_u32_be(trimesh_data + PPC_Q3_TRIMESH_NUM_POINTS_OFFSET, 3)
        .unwrap();
    loaded
        .memory
        .write_u32_be(trimesh_data + PPC_Q3_TRIMESH_POINTS_OFFSET, points_ptr)
        .unwrap();
    loaded
        .memory
        .write_u32_be(
            trimesh_data + PPC_Q3_TRIMESH_NUM_VERTEX_ATTRIBUTE_TYPES_OFFSET,
            1,
        )
        .unwrap();
    loaded
        .memory
        .write_u32_be(
            trimesh_data + PPC_Q3_TRIMESH_VERTEX_ATTRIBUTE_TYPES_OFFSET,
            vertex_attrs_ptr,
        )
        .unwrap();
    ppc_write_q3_vector3d(&mut loaded.memory, points_ptr, (-1.0, -1.0, 0.0)).unwrap();
    ppc_write_q3_vector3d(&mut loaded.memory, points_ptr + 12, (1.0, -1.0, 0.0)).unwrap();
    ppc_write_q3_vector3d(&mut loaded.memory, points_ptr + 24, (-1.0, 1.0, 0.0)).unwrap();
    loaded.memory.write_u32_be(triangles_ptr, 0).unwrap();
    loaded.memory.write_u32_be(triangles_ptr + 4, 1).unwrap();
    loaded.memory.write_u32_be(triangles_ptr + 8, 2).unwrap();
    loaded
        .memory
        .write_u32_be(vertex_attrs_ptr, PPC_Q3_ATTRIBUTE_TYPE_SURFACE_UV)
        .unwrap();
    loaded
        .memory
        .write_u32_be(vertex_attrs_ptr + 4, uvs_ptr)
        .unwrap();
    loaded.memory.write_u32_be(vertex_attrs_ptr + 8, 0).unwrap();
    ppc_write_q3_vector2d(&mut loaded.memory, uvs_ptr, (0.0, 0.0)).unwrap();
    ppc_write_q3_vector2d(&mut loaded.memory, uvs_ptr + 8, (1.0, 0.0)).unwrap();
    ppc_write_q3_vector2d(&mut loaded.memory, uvs_ptr + 16, (0.0, 0.0)).unwrap();
    loaded.memory.write_u8(image_ptr, 255).unwrap();
    loaded.memory.write_u8(image_ptr + 1, 0).unwrap();
    loaded.memory.write_u8(image_ptr + 2, 0).unwrap();
    loaded.memory.write_u8(image_ptr + 3, 0).unwrap();
    loaded.memory.write_u8(image_ptr + 4, 0).unwrap();
    loaded.memory.write_u8(image_ptr + 5, 255).unwrap();

    let mut diffuse = Vec::new();
    for value in [1.0f32, 1.0, 1.0] {
        diffuse.extend_from_slice(&value.to_bits().to_be_bytes());
    }
    let mut mipmap = vec![0; PPC_Q3_MIPMAP_COPY_SIZE as usize];
    mipmap[PPC_Q3_MIPMAP_IMAGE_OFFSET as usize..][..4].copy_from_slice(&storage.to_be_bytes());
    mipmap[PPC_Q3_MIPMAP_PIXEL_TYPE_OFFSET as usize..][..4]
        .copy_from_slice(&PPC_Q3_PIXEL_TYPE_RGB24.to_be_bytes());
    mipmap[PPC_Q3_MIPMAP_BYTE_ORDER_OFFSET as usize..][..4]
        .copy_from_slice(&PPC_Q3_ENDIAN_BIG.to_be_bytes());
    mipmap[PPC_Q3_MIPMAP_BASE_WIDTH_OFFSET as usize..][..4]
        .copy_from_slice(&2u32.to_be_bytes());
    mipmap[PPC_Q3_MIPMAP_BASE_HEIGHT_OFFSET as usize..][..4]
        .copy_from_slice(&1u32.to_be_bytes());
    mipmap[PPC_Q3_MIPMAP_BASE_ROW_BYTES_OFFSET as usize..][..4]
        .copy_from_slice(&6u32.to_be_bytes());
    mipmap[PPC_Q3_MIPMAP_BASE_IMAGE_OFFSET_OFFSET as usize..][..4]
        .copy_from_slice(&0u32.to_be_bytes());

    loaded.q3_memory_storages.push(PpcQ3MemoryStorageRecord {
        storage,
        buffer_ptr: image_ptr,
        valid_size: 6,
        buffer_size: 6,
        owns_buffer: false,
    });
    loaded.q3_views.push(PpcQ3ViewStateRecord::new(view));
    loaded.q3_submissions.push(PpcQ3SubmissionRecord {
        view,
        kind: PpcQ3SubmissionKind::TriMesh,
        primary: trimesh_data,
        secondary: 0,
    });
    loaded
        .q3_submission_transforms
        .push(PpcQ3SubmissionTransformRecord {
            view,
            kind: PpcQ3SubmissionKind::TriMesh,
            primary: trimesh_data,
            secondary: 0,
            local_to_world: ppc_q3_matrix4x4_identity(),
        });
    loaded
        .q3_submission_materials
        .push(PpcQ3SubmissionMaterialRecord {
            view,
            kind: PpcQ3SubmissionKind::TriMesh,
            primary: trimesh_data,
            secondary: 0,
            shader,
            illumination_type: PPC_Q3_ILLUMINATION_TYPE_PHONG,
            styles: Vec::new(),
            fog_style: None,
            attributes: vec![PpcQ3AttributeRecord {
                attribute_set,
                attribute_type: PPC_Q3_ATTRIBUTE_TYPE_DIFFUSE_COLOR,
                data: diffuse,
            }],
            shader_uv_transform: None,
            shader_boundary: Some(PpcQ3ShaderBoundaryRecord {
                shader,
                u_boundary: PPC_Q3_SHADER_UV_BOUNDARY_CLAMP,
                v_boundary: PPC_Q3_SHADER_UV_BOUNDARY_CLAMP,
            }),
            texture_shader: Some(PpcQ3TextureShaderRecord { shader, texture }),
            mipmap_texture: Some(PpcQ3MipmapTextureRecord { texture, mipmap }),
        });
    loaded
        .q3_submission_lights
        .push(PpcQ3SubmissionLightRecord {
            view,
            kind: PpcQ3SubmissionKind::TriMesh,
            primary: trimesh_data,
            secondary: 0,
            light_group: 0,
            lights: Vec::new(),
        });

    let stats = loaded.render_q3_scene_commands_to_front_buffer();

    assert_eq!(stats.commands, 1);
    assert_eq!(stats.triangles, 1);
    assert!(stats.pixels > 0);
    assert_eq!(
        loaded.memory.read_u16_be(front_base + 5 * 16 + 4 * 2),
        Some(0x001f)
    );

    loaded.q3_submission_materials[0].styles = vec![PpcQ3StyleRecord {
        style: PPC_Q3_OBJECT_BASE + PPC_Q3_OBJECT_STRIDE * 5,
        kind: PpcQ3StyleKind::Interpolation,
        value: PPC_Q3_INTERPOLATION_STYLE_VERTEX,
    }];
    for offset in 0..(8 * 8 * 2) {
        loaded.memory.write_u8(front_base + offset, 0).unwrap();
    }
    let vertex_stats = loaded.render_q3_scene_commands_to_front_buffer();
    assert_eq!(vertex_stats.triangles, 1);
    assert_eq!(
        loaded.memory.read_u16_be(front_base + 5 * 16 + 4 * 2),
        Some(0x001f),
        "vertex lighting must still sample the texture per pixel"
    );
}

#[test]
fn q3_software_renderer_honors_shader_uv_boundary_modes() {
    fn render_boundary_sample(u_boundary: u32) -> u16 {
        let view = PPC_Q3_OBJECT_BASE;
        let attribute_set = PPC_Q3_OBJECT_BASE + PPC_Q3_OBJECT_STRIDE;
        let texture = PPC_Q3_OBJECT_BASE + PPC_Q3_OBJECT_STRIDE * 2;
        let shader = PPC_Q3_OBJECT_BASE + PPC_Q3_OBJECT_STRIDE * 3;
        let storage = PPC_Q3_OBJECT_BASE + PPC_Q3_OBJECT_STRIDE * 4;
        let trimesh_data = PPC_DATA_BASE + 0x1000;
        let points_ptr = PPC_DATA_BASE + 0x1080;
        let triangles_ptr = PPC_DATA_BASE + 0x1100;
        let vertex_attrs_ptr = PPC_DATA_BASE + 0x1180;
        let uvs_ptr = PPC_DATA_BASE + 0x11c0;
        let image_ptr = PPC_DATA_BASE + 0x1240;
        let front_base = PPC_HEAP_BASE + 0x1000;
        let pef = synthetic_pef_with_library_import(b"QuickDraw\xaa 3D", b"Q3TriMesh_Submit");
        let mut loaded = load_pef_application(&pef).unwrap();
        loaded.memory.add_region(trimesh_data, vec![0; 0x800]);
        loaded.memory.add_region(front_base, vec![0; 8 * 8 * 2]);
        loaded.gworlds = vec![PpcGWorldRecord {
            ui_theme: crate::ui_theme::UiThemeId::ClassicSystem7,
            port: PPC_MAIN_GWORLD,
            pixmap_handle: 0,
            pixmap: 0,
            base_addr: front_base,
            gdevice: PPC_MAIN_GDEVICE,
            width: 8,
            height: 8,
            depth: 16,
            row_bytes: 16,
            pixels_locked: false,
            pixels_no_purge: false,
        }];
        loaded
            .current_gworld
            .with_mut(|current_gworld| *current_gworld = PPC_MAIN_GWORLD);

        loaded
            .memory
            .write_u32_be(trimesh_data + PPC_Q3_TRIMESH_NUM_TRIANGLES_OFFSET, 1)
            .unwrap();
        loaded
            .memory
            .write_u32_be(
                trimesh_data + PPC_Q3_TRIMESH_TRIANGLES_OFFSET,
                triangles_ptr,
            )
            .unwrap();
        loaded
            .memory
            .write_u32_be(trimesh_data + PPC_Q3_TRIMESH_NUM_POINTS_OFFSET, 3)
            .unwrap();
        loaded
            .memory
            .write_u32_be(trimesh_data + PPC_Q3_TRIMESH_POINTS_OFFSET, points_ptr)
            .unwrap();
        loaded
            .memory
            .write_u32_be(
                trimesh_data + PPC_Q3_TRIMESH_NUM_VERTEX_ATTRIBUTE_TYPES_OFFSET,
                1,
            )
            .unwrap();
        loaded
            .memory
            .write_u32_be(
                trimesh_data + PPC_Q3_TRIMESH_VERTEX_ATTRIBUTE_TYPES_OFFSET,
                vertex_attrs_ptr,
            )
            .unwrap();
        ppc_write_q3_vector3d(&mut loaded.memory, points_ptr, (-1.0, -1.0, 0.0)).unwrap();
        ppc_write_q3_vector3d(&mut loaded.memory, points_ptr + 12, (1.0, -1.0, 0.0)).unwrap();
        ppc_write_q3_vector3d(&mut loaded.memory, points_ptr + 24, (-1.0, 1.0, 0.0)).unwrap();
        loaded.memory.write_u32_be(triangles_ptr, 0).unwrap();
        loaded.memory.write_u32_be(triangles_ptr + 4, 1).unwrap();
        loaded.memory.write_u32_be(triangles_ptr + 8, 2).unwrap();
        loaded
            .memory
            .write_u32_be(vertex_attrs_ptr, PPC_Q3_ATTRIBUTE_TYPE_SURFACE_UV)
            .unwrap();
        loaded
            .memory
            .write_u32_be(vertex_attrs_ptr + 4, uvs_ptr)
            .unwrap();
        loaded.memory.write_u32_be(vertex_attrs_ptr + 8, 0).unwrap();
        for index in 0..3 {
            ppc_write_q3_vector2d(&mut loaded.memory, uvs_ptr + index * 8, (1.25, 0.5))
                .unwrap();
        }
        loaded.memory.write_u8(image_ptr, 255).unwrap();
        loaded.memory.write_u8(image_ptr + 1, 0).unwrap();
        loaded.memory.write_u8(image_ptr + 2, 0).unwrap();
        loaded.memory.write_u8(image_ptr + 3, 0).unwrap();
        loaded.memory.write_u8(image_ptr + 4, 0).unwrap();
        loaded.memory.write_u8(image_ptr + 5, 255).unwrap();

        let mut diffuse = Vec::new();
        for value in [1.0f32, 1.0, 1.0] {
            diffuse.extend_from_slice(&value.to_bits().to_be_bytes());
        }
        let mut mipmap = vec![0; PPC_Q3_MIPMAP_COPY_SIZE as usize];
        mipmap[PPC_Q3_MIPMAP_IMAGE_OFFSET as usize..][..4]
            .copy_from_slice(&storage.to_be_bytes());
        mipmap[PPC_Q3_MIPMAP_PIXEL_TYPE_OFFSET as usize..][..4]
            .copy_from_slice(&PPC_Q3_PIXEL_TYPE_RGB24.to_be_bytes());
        mipmap[PPC_Q3_MIPMAP_BYTE_ORDER_OFFSET as usize..][..4]
            .copy_from_slice(&PPC_Q3_ENDIAN_BIG.to_be_bytes());
        mipmap[PPC_Q3_MIPMAP_BASE_WIDTH_OFFSET as usize..][..4]
            .copy_from_slice(&2u32.to_be_bytes());
        mipmap[PPC_Q3_MIPMAP_BASE_HEIGHT_OFFSET as usize..][..4]
            .copy_from_slice(&1u32.to_be_bytes());
        mipmap[PPC_Q3_MIPMAP_BASE_ROW_BYTES_OFFSET as usize..][..4]
            .copy_from_slice(&6u32.to_be_bytes());
        mipmap[PPC_Q3_MIPMAP_BASE_IMAGE_OFFSET_OFFSET as usize..][..4]
            .copy_from_slice(&0u32.to_be_bytes());

        loaded.q3_memory_storages.push(PpcQ3MemoryStorageRecord {
            storage,
            buffer_ptr: image_ptr,
            valid_size: 6,
            buffer_size: 6,
            owns_buffer: false,
        });
        loaded.q3_views.push(PpcQ3ViewStateRecord::new(view));
        loaded.q3_submissions.push(PpcQ3SubmissionRecord {
            view,
            kind: PpcQ3SubmissionKind::TriMesh,
            primary: trimesh_data,
            secondary: 0,
        });
        loaded
            .q3_submission_transforms
            .push(PpcQ3SubmissionTransformRecord {
                view,
                kind: PpcQ3SubmissionKind::TriMesh,
                primary: trimesh_data,
                secondary: 0,
                local_to_world: ppc_q3_matrix4x4_identity(),
            });
        loaded
            .q3_submission_materials
            .push(PpcQ3SubmissionMaterialRecord {
                view,
                kind: PpcQ3SubmissionKind::TriMesh,
                primary: trimesh_data,
                secondary: 0,
                shader,
                illumination_type: PPC_Q3_ILLUMINATION_TYPE_PHONG,
                styles: Vec::new(),
                fog_style: None,
                attributes: vec![PpcQ3AttributeRecord {
                    attribute_set,
                    attribute_type: PPC_Q3_ATTRIBUTE_TYPE_DIFFUSE_COLOR,
                    data: diffuse,
                }],
                shader_uv_transform: None,
                shader_boundary: Some(PpcQ3ShaderBoundaryRecord {
                    shader,
                    u_boundary,
                    v_boundary: PPC_Q3_SHADER_UV_BOUNDARY_CLAMP,
                }),
                texture_shader: Some(PpcQ3TextureShaderRecord { shader, texture }),
                mipmap_texture: Some(PpcQ3MipmapTextureRecord { texture, mipmap }),
            });
        loaded
            .q3_submission_lights
            .push(PpcQ3SubmissionLightRecord {
                view,
                kind: PpcQ3SubmissionKind::TriMesh,
                primary: trimesh_data,
                secondary: 0,
                light_group: 0,
                lights: Vec::new(),
            });

        let stats = loaded.render_q3_scene_commands_to_front_buffer();

        assert_eq!(stats.commands, 1);
        assert_eq!(stats.triangles, 1);
        assert!(stats.pixels > 0);
        loaded
            .memory
            .read_u16_be(front_base + 5 * 16 + 4 * 2)
            .unwrap()
    }

    let wrapped = render_boundary_sample(PPC_Q3_SHADER_UV_BOUNDARY_WRAP);
    let clamped = render_boundary_sample(PPC_Q3_SHADER_UV_BOUNDARY_CLAMP);

    assert_eq!(wrapped, 0x7c00);
    assert_eq!(clamped, 0x001f);
    assert_ne!(wrapped, clamped);
}

#[test]
fn q3_software_renderer_applies_shader_uv_transform_before_texture_sampling() {
    let view = PPC_Q3_OBJECT_BASE;
    let attribute_set = PPC_Q3_OBJECT_BASE + PPC_Q3_OBJECT_STRIDE;
    let texture = PPC_Q3_OBJECT_BASE + PPC_Q3_OBJECT_STRIDE * 2;
    let shader = PPC_Q3_OBJECT_BASE + PPC_Q3_OBJECT_STRIDE * 3;
    let storage = PPC_Q3_OBJECT_BASE + PPC_Q3_OBJECT_STRIDE * 4;
    let trimesh_data = PPC_DATA_BASE + 0x1000;
    let points_ptr = PPC_DATA_BASE + 0x1080;
    let triangles_ptr = PPC_DATA_BASE + 0x1100;
    let vertex_attrs_ptr = PPC_DATA_BASE + 0x1180;
    let uvs_ptr = PPC_DATA_BASE + 0x11c0;
    let image_ptr = PPC_DATA_BASE + 0x1240;
    let front_base = PPC_HEAP_BASE + 0x1000;
    let pef = synthetic_pef_with_library_import(b"QuickDraw\xaa 3D", b"Q3TriMesh_Submit");
    let mut loaded = load_pef_application(&pef).unwrap();
    loaded.memory.add_region(trimesh_data, vec![0; 0x800]);
    loaded.memory.add_region(front_base, vec![0; 8 * 8 * 2]);
    loaded.gworlds = vec![PpcGWorldRecord {
        ui_theme: crate::ui_theme::UiThemeId::ClassicSystem7,
        port: PPC_MAIN_GWORLD,
        pixmap_handle: 0,
        pixmap: 0,
        base_addr: front_base,
        gdevice: PPC_MAIN_GDEVICE,
        width: 8,
        height: 8,
        depth: 16,
        row_bytes: 16,
        pixels_locked: false,
        pixels_no_purge: false,
    }];
    loaded
        .current_gworld
        .with_mut(|current_gworld| *current_gworld = PPC_MAIN_GWORLD);

    loaded
        .memory
        .write_u32_be(trimesh_data + PPC_Q3_TRIMESH_NUM_TRIANGLES_OFFSET, 1)
        .unwrap();
    loaded
        .memory
        .write_u32_be(
            trimesh_data + PPC_Q3_TRIMESH_TRIANGLES_OFFSET,
            triangles_ptr,
        )
        .unwrap();
    loaded
        .memory
        .write_u32_be(trimesh_data + PPC_Q3_TRIMESH_NUM_POINTS_OFFSET, 3)
        .unwrap();
    loaded
        .memory
        .write_u32_be(trimesh_data + PPC_Q3_TRIMESH_POINTS_OFFSET, points_ptr)
        .unwrap();
    loaded
        .memory
        .write_u32_be(
            trimesh_data + PPC_Q3_TRIMESH_NUM_VERTEX_ATTRIBUTE_TYPES_OFFSET,
            1,
        )
        .unwrap();
    loaded
        .memory
        .write_u32_be(
            trimesh_data + PPC_Q3_TRIMESH_VERTEX_ATTRIBUTE_TYPES_OFFSET,
            vertex_attrs_ptr,
        )
        .unwrap();
    ppc_write_q3_vector3d(&mut loaded.memory, points_ptr, (-1.0, -1.0, 0.0)).unwrap();
    ppc_write_q3_vector3d(&mut loaded.memory, points_ptr + 12, (1.0, -1.0, 0.0)).unwrap();
    ppc_write_q3_vector3d(&mut loaded.memory, points_ptr + 24, (-1.0, 1.0, 0.0)).unwrap();
    loaded.memory.write_u32_be(triangles_ptr, 0).unwrap();
    loaded.memory.write_u32_be(triangles_ptr + 4, 1).unwrap();
    loaded.memory.write_u32_be(triangles_ptr + 8, 2).unwrap();
    loaded
        .memory
        .write_u32_be(vertex_attrs_ptr, PPC_Q3_ATTRIBUTE_TYPE_SURFACE_UV)
        .unwrap();
    loaded
        .memory
        .write_u32_be(vertex_attrs_ptr + 4, uvs_ptr)
        .unwrap();
    loaded.memory.write_u32_be(vertex_attrs_ptr + 8, 0).unwrap();
    for index in 0..3 {
        ppc_write_q3_vector2d(&mut loaded.memory, uvs_ptr + index * 8, (0.0, 0.0)).unwrap();
    }
    loaded.memory.write_u8(image_ptr, 255).unwrap();
    loaded.memory.write_u8(image_ptr + 1, 0).unwrap();
    loaded.memory.write_u8(image_ptr + 2, 0).unwrap();
    loaded.memory.write_u8(image_ptr + 3, 0).unwrap();
    loaded.memory.write_u8(image_ptr + 4, 0).unwrap();
    loaded.memory.write_u8(image_ptr + 5, 255).unwrap();

    let mut diffuse = Vec::new();
    for value in [1.0f32, 1.0, 1.0] {
        diffuse.extend_from_slice(&value.to_bits().to_be_bytes());
    }
    let mut mipmap = vec![0; PPC_Q3_MIPMAP_COPY_SIZE as usize];
    mipmap[PPC_Q3_MIPMAP_IMAGE_OFFSET as usize..][..4].copy_from_slice(&storage.to_be_bytes());
    mipmap[PPC_Q3_MIPMAP_PIXEL_TYPE_OFFSET as usize..][..4]
        .copy_from_slice(&PPC_Q3_PIXEL_TYPE_RGB24.to_be_bytes());
    mipmap[PPC_Q3_MIPMAP_BYTE_ORDER_OFFSET as usize..][..4]
        .copy_from_slice(&PPC_Q3_ENDIAN_BIG.to_be_bytes());
    mipmap[PPC_Q3_MIPMAP_BASE_WIDTH_OFFSET as usize..][..4]
        .copy_from_slice(&2u32.to_be_bytes());
    mipmap[PPC_Q3_MIPMAP_BASE_HEIGHT_OFFSET as usize..][..4]
        .copy_from_slice(&1u32.to_be_bytes());
    mipmap[PPC_Q3_MIPMAP_BASE_ROW_BYTES_OFFSET as usize..][..4]
        .copy_from_slice(&6u32.to_be_bytes());
    mipmap[PPC_Q3_MIPMAP_BASE_IMAGE_OFFSET_OFFSET as usize..][..4]
        .copy_from_slice(&0u32.to_be_bytes());

    loaded.q3_memory_storages.push(PpcQ3MemoryStorageRecord {
        storage,
        buffer_ptr: image_ptr,
        valid_size: 6,
        buffer_size: 6,
        owns_buffer: false,
    });
    loaded.q3_views.push(PpcQ3ViewStateRecord::new(view));
    loaded.q3_submissions.push(PpcQ3SubmissionRecord {
        view,
        kind: PpcQ3SubmissionKind::TriMesh,
        primary: trimesh_data,
        secondary: 0,
    });
    loaded
        .q3_submission_transforms
        .push(PpcQ3SubmissionTransformRecord {
            view,
            kind: PpcQ3SubmissionKind::TriMesh,
            primary: trimesh_data,
            secondary: 0,
            local_to_world: ppc_q3_matrix4x4_identity(),
        });
    loaded
        .q3_submission_materials
        .push(PpcQ3SubmissionMaterialRecord {
            view,
            kind: PpcQ3SubmissionKind::TriMesh,
            primary: trimesh_data,
            secondary: 0,
            shader,
            illumination_type: PPC_Q3_ILLUMINATION_TYPE_PHONG,
            styles: Vec::new(),
            fog_style: None,
            attributes: vec![PpcQ3AttributeRecord {
                attribute_set,
                attribute_type: PPC_Q3_ATTRIBUTE_TYPE_DIFFUSE_COLOR,
                data: diffuse,
            }],
            shader_uv_transform: Some(PpcQ3ShaderUvTransformRecord {
                shader,
                matrix: [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.75, 0.0, 1.0]],
            }),
            shader_boundary: Some(PpcQ3ShaderBoundaryRecord {
                shader,
                u_boundary: PPC_Q3_SHADER_UV_BOUNDARY_CLAMP,
                v_boundary: PPC_Q3_SHADER_UV_BOUNDARY_CLAMP,
            }),
            texture_shader: Some(PpcQ3TextureShaderRecord { shader, texture }),
            mipmap_texture: Some(PpcQ3MipmapTextureRecord { texture, mipmap }),
        });
    loaded
        .q3_submission_lights
        .push(PpcQ3SubmissionLightRecord {
            view,
            kind: PpcQ3SubmissionKind::TriMesh,
            primary: trimesh_data,
            secondary: 0,
            light_group: 0,
            lights: Vec::new(),
        });

    let stats = loaded.render_q3_scene_commands_to_front_buffer();

    assert_eq!(stats.commands, 1);
    assert_eq!(stats.triangles, 1);
    assert!(stats.pixels > 0);
    assert_eq!(
        loaded.memory.read_u16_be(front_base + 5 * 16 + 4 * 2),
        Some(0x001f)
    );
}

#[test]
fn q3_software_renderer_clips_partially_offscreen_trimeshes() {
    let view = PPC_Q3_OBJECT_BASE;
    let attribute_set = PPC_Q3_OBJECT_BASE + PPC_Q3_OBJECT_STRIDE;
    let trimesh_data = PPC_DATA_BASE + 0x1000;
    let points_ptr = PPC_DATA_BASE + 0x1080;
    let triangles_ptr = PPC_DATA_BASE + 0x1100;
    let front_base = PPC_HEAP_BASE + 0x1000;
    let pef = synthetic_pef_with_library_import(b"QuickDraw\xaa 3D", b"Q3TriMesh_Submit");
    let mut loaded = load_pef_application(&pef).unwrap();
    loaded.memory.add_region(trimesh_data, vec![0; 0x200]);
    loaded.memory.add_region(front_base, vec![0; 8 * 8 * 2]);
    loaded.gworlds = vec![PpcGWorldRecord {
        ui_theme: crate::ui_theme::UiThemeId::ClassicSystem7,
        port: PPC_MAIN_GWORLD,
        pixmap_handle: 0,
        pixmap: 0,
        base_addr: front_base,
        gdevice: PPC_MAIN_GDEVICE,
        width: 8,
        height: 8,
        depth: 16,
        row_bytes: 16,
        pixels_locked: false,
        pixels_no_purge: false,
    }];
    loaded
        .current_gworld
        .with_mut(|current_gworld| *current_gworld = PPC_MAIN_GWORLD);

    loaded
        .memory
        .write_u32_be(trimesh_data + PPC_Q3_TRIMESH_NUM_TRIANGLES_OFFSET, 1)
        .unwrap();
    loaded
        .memory
        .write_u32_be(
            trimesh_data + PPC_Q3_TRIMESH_TRIANGLES_OFFSET,
            triangles_ptr,
        )
        .unwrap();
    loaded
        .memory
        .write_u32_be(trimesh_data + PPC_Q3_TRIMESH_NUM_POINTS_OFFSET, 3)
        .unwrap();
    loaded
        .memory
        .write_u32_be(trimesh_data + PPC_Q3_TRIMESH_POINTS_OFFSET, points_ptr)
        .unwrap();
    ppc_write_q3_vector3d(&mut loaded.memory, points_ptr, (-1.5, -0.8, 0.0)).unwrap();
    ppc_write_q3_vector3d(&mut loaded.memory, points_ptr + 12, (-1.5, 0.8, 0.0)).unwrap();
    ppc_write_q3_vector3d(&mut loaded.memory, points_ptr + 24, (0.8, 0.0, 0.0)).unwrap();
    loaded.memory.write_u32_be(triangles_ptr, 0).unwrap();
    loaded.memory.write_u32_be(triangles_ptr + 4, 1).unwrap();
    loaded.memory.write_u32_be(triangles_ptr + 8, 2).unwrap();

    let mut diffuse = Vec::new();
    for value in [0.0f32, 1.0, 0.0] {
        diffuse.extend_from_slice(&value.to_bits().to_be_bytes());
    }
    loaded.q3_views.push(PpcQ3ViewStateRecord::new(view));
    loaded.q3_submissions.push(PpcQ3SubmissionRecord {
        view,
        kind: PpcQ3SubmissionKind::TriMesh,
        primary: trimesh_data,
        secondary: 0,
    });
    loaded
        .q3_submission_transforms
        .push(PpcQ3SubmissionTransformRecord {
            view,
            kind: PpcQ3SubmissionKind::TriMesh,
            primary: trimesh_data,
            secondary: 0,
            local_to_world: ppc_q3_matrix4x4_identity(),
        });
    loaded
        .q3_submission_materials
        .push(PpcQ3SubmissionMaterialRecord {
            view,
            kind: PpcQ3SubmissionKind::TriMesh,
            primary: trimesh_data,
            secondary: 0,
            shader: 0,
            illumination_type: PPC_Q3_ILLUMINATION_TYPE_PHONG,
            styles: Vec::new(),
            fog_style: None,
            attributes: vec![PpcQ3AttributeRecord {
                attribute_set,
                attribute_type: PPC_Q3_ATTRIBUTE_TYPE_DIFFUSE_COLOR,
                data: diffuse,
            }],
            shader_uv_transform: None,
            shader_boundary: None,
            texture_shader: None,
            mipmap_texture: None,
        });
    loaded
        .q3_submission_lights
        .push(PpcQ3SubmissionLightRecord {
            view,
            kind: PpcQ3SubmissionKind::TriMesh,
            primary: trimesh_data,
            secondary: 0,
            light_group: 0,
            lights: Vec::new(),
        });

    let stats = loaded.render_q3_scene_commands_to_front_buffer();

    assert_eq!(stats.commands, 1);
    assert_eq!(stats.triangles, 1);
    assert!(stats.pixels > 0);
    let mut left_edge_pixels = 0;
    for y in 0..8 {
        if loaded.memory.read_u16_be(front_base + y * 16) == Some(0x03e0) {
            left_edge_pixels += 1;
        }
    }
    assert!(left_edge_pixels > 0);
}

#[test]
fn q3_software_renderer_clips_camera_near_and_far_depth() {
    fn write_triangle_mesh(
        memory: &mut PpcSectionMem,
        trimesh_data: u32,
        points_ptr: u32,
        triangles_ptr: u32,
        vertices: [(f32, f32, f32); 3],
    ) {
        memory
            .write_u32_be(trimesh_data + PPC_Q3_TRIMESH_NUM_TRIANGLES_OFFSET, 1)
            .unwrap();
        memory
            .write_u32_be(
                trimesh_data + PPC_Q3_TRIMESH_TRIANGLES_OFFSET,
                triangles_ptr,
            )
            .unwrap();
        memory
            .write_u32_be(trimesh_data + PPC_Q3_TRIMESH_NUM_POINTS_OFFSET, 3)
            .unwrap();
        memory
            .write_u32_be(trimesh_data + PPC_Q3_TRIMESH_POINTS_OFFSET, points_ptr)
            .unwrap();
        for (index, vertex) in vertices.into_iter().enumerate() {
            ppc_write_q3_vector3d(
                memory,
                points_ptr + (index as u32 * PPC_Q3_POINT3D_SIZE),
                vertex,
            )
            .unwrap();
        }
        memory.write_u32_be(triangles_ptr, 0).unwrap();
        memory.write_u32_be(triangles_ptr + 4, 1).unwrap();
        memory.write_u32_be(triangles_ptr + 8, 2).unwrap();
    }

    fn diffuse_attribute(attribute_set: u32, rgb: [f32; 3]) -> PpcQ3AttributeRecord {
        let mut data = Vec::new();
        for value in rgb {
            data.extend_from_slice(&value.to_bits().to_be_bytes());
        }
        PpcQ3AttributeRecord {
            attribute_set,
            attribute_type: PPC_Q3_ATTRIBUTE_TYPE_DIFFUSE_COLOR,
            data,
        }
    }

    let view = PPC_Q3_OBJECT_BASE;
    let camera = PPC_Q3_OBJECT_BASE + PPC_Q3_OBJECT_STRIDE;
    let far_attribute_set = PPC_Q3_OBJECT_BASE + PPC_Q3_OBJECT_STRIDE * 2;
    let near_attribute_set = PPC_Q3_OBJECT_BASE + PPC_Q3_OBJECT_STRIDE * 3;
    let far_trimesh_data = PPC_DATA_BASE + 0x1000;
    let far_points_ptr = PPC_DATA_BASE + 0x1080;
    let far_triangles_ptr = PPC_DATA_BASE + 0x1100;
    let near_trimesh_data = PPC_DATA_BASE + 0x1200;
    let near_points_ptr = PPC_DATA_BASE + 0x1280;
    let near_triangles_ptr = PPC_DATA_BASE + 0x1300;
    let front_base = PPC_HEAP_BASE + 0x1000;
    let pef = synthetic_pef_with_library_import(b"QuickDraw\xaa 3D", b"Q3TriMesh_Submit");
    let mut loaded = load_pef_application(&pef).unwrap();
    loaded.memory.add_region(far_trimesh_data, vec![0; 0x400]);
    loaded.memory.add_region(front_base, vec![0; 8 * 8 * 2]);
    loaded.gworlds = vec![PpcGWorldRecord {
        ui_theme: crate::ui_theme::UiThemeId::ClassicSystem7,
        port: PPC_MAIN_GWORLD,
        pixmap_handle: 0,
        pixmap: 0,
        base_addr: front_base,
        gdevice: PPC_MAIN_GDEVICE,
        width: 8,
        height: 8,
        depth: 16,
        row_bytes: 16,
        pixels_locked: false,
        pixels_no_purge: false,
    }];
    loaded
        .current_gworld
        .with_mut(|current_gworld| *current_gworld = PPC_MAIN_GWORLD);

    write_triangle_mesh(
        &mut loaded.memory,
        far_trimesh_data,
        far_points_ptr,
        far_triangles_ptr,
        [(-4.0, -1.0, -8.0), (-4.0, 1.0, -8.0), (-2.0, 0.0, -8.0)],
    );
    write_triangle_mesh(
        &mut loaded.memory,
        near_trimesh_data,
        near_points_ptr,
        near_triangles_ptr,
        [(-0.2, -0.2, -0.5), (-0.2, 0.2, -0.5), (0.8, 0.0, -2.0)],
    );

    let mut view_state = PpcQ3ViewStateRecord::new(view);
    view_state.camera = camera;
    loaded.q3_views.push(view_state);
    loaded.q3_cameras.push(PpcQ3CameraRecord {
        camera,
        camera_type: PPC_Q3_CAMERA_TYPE_VIEW_ANGLE_ASPECT,
        placement: PpcQ3CameraPlacement {
            camera_location: (0.0, 0.0, 0.0),
            point_of_interest: (0.0, 0.0, -1.0),
            up_vector: (0.0, 1.0, 0.0),
        },
        range_hither: 1.0,
        range_yon: 5.0,
        viewport_origin: (-1.0, 1.0),
        viewport_width: 2.0,
        viewport_height: 2.0,
        projection: PpcQ3CameraProjection::ViewAngleAspect {
            fov: std::f32::consts::FRAC_PI_2,
            aspect_ratio_x_to_y: 1.0,
        },
    });
    for (trimesh_data, attribute_set, rgb) in [
        (far_trimesh_data, far_attribute_set, [1.0, 0.0, 0.0]),
        (near_trimesh_data, near_attribute_set, [0.0, 1.0, 0.0]),
    ] {
        loaded.q3_submissions.push(PpcQ3SubmissionRecord {
            view,
            kind: PpcQ3SubmissionKind::TriMesh,
            primary: trimesh_data,
            secondary: 0,
        });
        loaded
            .q3_submission_transforms
            .push(PpcQ3SubmissionTransformRecord {
                view,
                kind: PpcQ3SubmissionKind::TriMesh,
                primary: trimesh_data,
                secondary: 0,
                local_to_world: ppc_q3_matrix4x4_identity(),
            });
        loaded
            .q3_submission_materials
            .push(PpcQ3SubmissionMaterialRecord {
                view,
                kind: PpcQ3SubmissionKind::TriMesh,
                primary: trimesh_data,
                secondary: 0,
                shader: 0,
                illumination_type: PPC_Q3_ILLUMINATION_TYPE_PHONG,
                styles: Vec::new(),
                fog_style: None,
                attributes: vec![diffuse_attribute(attribute_set, rgb)],
                shader_uv_transform: None,
                shader_boundary: None,
                texture_shader: None,
                mipmap_texture: None,
            });
        loaded
            .q3_submission_lights
            .push(PpcQ3SubmissionLightRecord {
                view,
                kind: PpcQ3SubmissionKind::TriMesh,
                primary: trimesh_data,
                secondary: 0,
                light_group: 0,
                lights: Vec::new(),
            });
    }

    let stats = loaded.render_q3_scene_commands_to_front_buffer();

    assert_eq!(stats.commands, 2);
    assert_eq!(stats.vertices, 6);
    assert_eq!(stats.triangles, 1);
    assert!(stats.pixels > 0);
    let mut red_pixels = 0;
    let mut green_pixels = 0;
    for y in 0..8 {
        for x in 0..8 {
            match loaded.memory.read_u16_be(front_base + y * 16 + x * 2) {
                Some(0x7c00) => red_pixels += 1,
                Some(0x03e0) => green_pixels += 1,
                _ => {}
            }
        }
    }
    assert_eq!(red_pixels, 0);
    assert!(green_pixels > 0);
}

#[test]
fn q3_software_triangle_edge_walker_matches_direct_edge_equations() {
    fn point(x: i32, y: i32) -> PpcQ3SoftwareProjectedPoint {
        PpcQ3SoftwareProjectedPoint {
            x,
            y,
            z: 0.0,
            reciprocal_w: 1.0,
            world: (0.0, 0.0, 0.0),
            view_direction: None,
            fog_depth: 0.0,
            uv: None,
            diffuse: None,
            ambient_coefficient: None,
            normal: None,
            specular_color: None,
            specular_control: None,
            highlight_state: None,
            vertex_alpha: None,
        }
    }

    let vertices = [point(3, 2), point(19, 7), point(8, 23)];
    let start_x = -4;
    let start_y = -3;
    let (mut row_weights, x_steps, y_steps) =
        ppc_q3_software_triangle_edge_walker(vertices, start_x, start_y);

    for y in start_y..=28 {
        let mut weights = row_weights;
        for x in start_x..=25 {
            let sample = point(x, y);
            assert_eq!(
                weights,
                [
                    ppc_q3_software_edge_value(vertices[1], vertices[2], sample),
                    ppc_q3_software_edge_value(vertices[2], vertices[0], sample),
                    ppc_q3_software_edge_value(vertices[0], vertices[1], sample),
                ]
            );
            for index in 0..3 {
                weights[index] += x_steps[index];
            }
        }
        for index in 0..3 {
            row_weights[index] += y_steps[index];
        }
    }
}

#[test]
fn q3_software_renderer_clips_camera_frustum_in_clip_space() {
    let view = PPC_Q3_OBJECT_BASE;
    let front_buffer = PpcFrontBuffer {
        base_addr: PPC_HEAP_BASE,
        row_bytes: 32 * 2,
        width: 32,
        height: 32,
        depth: 16,
    };
    let camera = PpcQ3CameraRecord {
        camera: PPC_Q3_OBJECT_BASE + PPC_Q3_OBJECT_STRIDE,
        camera_type: PPC_Q3_CAMERA_TYPE_VIEW_ANGLE_ASPECT,
        placement: PpcQ3CameraPlacement {
            camera_location: (0.0, 0.0, 0.0),
            point_of_interest: (0.0, 0.0, -1.0),
            up_vector: (0.0, 1.0, 0.0),
        },
        range_hither: 1.0,
        range_yon: 10.0,
        viewport_origin: (-1.0, 1.0),
        viewport_width: 2.0,
        viewport_height: 2.0,
        projection: PpcQ3CameraProjection::ViewAngleAspect {
            fov: std::f32::consts::FRAC_PI_2,
            aspect_ratio_x_to_y: 1.0,
        },
    };
    let command = PpcQ3SceneTriMeshCommand {
        submission_index: 0,
        view_state: PpcQ3ViewStateRecord::new(view),
        camera: Some(camera),
        geometry: PpcQ3SceneTriMeshGeometry {
            source: PpcQ3SceneTriMeshSource::DataPtr(0),
            data: Vec::new(),
        },
        local_to_world: ppc_q3_matrix4x4_identity(),
        material: PpcQ3SubmissionMaterialRecord {
            view,
            kind: PpcQ3SubmissionKind::TriMesh,
            primary: 0,
            secondary: 0,
            shader: 0,
            illumination_type: PPC_Q3_ILLUMINATION_TYPE_PHONG,
            styles: Vec::new(),
            fog_style: None,
            attributes: Vec::new(),
            shader_uv_transform: None,
            shader_boundary: None,
            texture_shader: None,
            mipmap_texture: None,
        },
        lights: PpcQ3SubmissionLightRecord {
            view,
            kind: PpcQ3SubmissionKind::TriMesh,
            primary: 0,
            secondary: 0,
            light_group: 0,
            lights: Vec::new(),
        },
    };
    let vertices = [(-8.0, 8.0, 0.5), (-0.35, -0.35, -2.0), (0.35, -0.35, -2.0)].map(|point| {
        ppc_q3_software_project_point(
            &command,
            front_buffer,
            PpcQ3ViewportRect::full(front_buffer),
            point,
        )
        .unwrap()
    });

    assert!(vertices.iter().any(|vertex| {
        vertex.x < 0.0
            || vertex.x > front_buffer.width.saturating_sub(1) as f32
            || vertex.y < 0.0
            || vertex.y > front_buffer.height.saturating_sub(1) as f32
    }));
    assert!(vertices
        .iter()
        .any(|vertex| !vertex.clip.is_some_and(ppc_q3_software_clip_inside_frustum)));

    let clipped = ppc_q3_software_clip_projected_triangle(
        Some(camera),
        front_buffer,
        PpcQ3ViewportRect::full(front_buffer),
        vertices,
    );

    assert!(clipped.len() >= 3, "{:?}", clipped);
    let max_x = front_buffer.width.saturating_sub(1) as f32;
    let max_y = front_buffer.height.saturating_sub(1) as f32;
    for vertex in &clipped {
        assert!(vertex.clip.is_some_and(ppc_q3_software_clip_inside_frustum));
        assert!(
            vertex.x >= -0.01 && vertex.x <= max_x + 0.01,
            "{:?}",
            clipped
        );
        assert!(
            vertex.y >= -0.01 && vertex.y <= max_y + 0.01,
            "{:?}",
            clipped
        );
    }
}

#[test]
fn q3_software_renderer_maps_qd3d_full_viewport_to_front_buffer() {
    let front_buffer = PpcFrontBuffer {
        base_addr: PPC_HEAP_BASE,
        row_bytes: 640 * 2,
        width: 640,
        height: 480,
        depth: 16,
    };
    let camera = PpcQ3CameraRecord {
        camera: PPC_Q3_OBJECT_BASE,
        camera_type: PPC_Q3_CAMERA_TYPE_VIEW_ANGLE_ASPECT,
        placement: PpcQ3CameraPlacement {
            camera_location: (0.0, 0.0, 0.0),
            point_of_interest: (0.0, 0.0, -1.0),
            up_vector: (0.0, 1.0, 0.0),
        },
        range_hither: 1.0,
        range_yon: 10.0,
        viewport_origin: (-1.0, 1.0),
        viewport_width: 2.0,
        viewport_height: 2.0,
        projection: PpcQ3CameraProjection::ViewAngleAspect {
            fov: std::f32::consts::FRAC_PI_2,
            aspect_ratio_x_to_y: 4.0 / 3.0,
        },
    };

    assert_eq!(
        ppc_q3_software_projected_to_screen(
            (-1.0, 1.0, 0.0),
            Some(camera),
            front_buffer,
            PpcQ3ViewportRect::full(front_buffer),
        ),
        (0.0, 0.0)
    );
    assert_eq!(
        ppc_q3_software_projected_to_screen(
            (1.0, -1.0, 0.0),
            Some(camera),
            front_buffer,
            PpcQ3ViewportRect::full(front_buffer),
        ),
        (639.0, 479.0)
    );
    assert_eq!(
        ppc_q3_software_projected_to_screen(
            (0.0, 0.0, 0.0),
            Some(camera),
            front_buffer,
            PpcQ3ViewportRect::full(front_buffer),
        ),
        (319.5, 239.5)
    );
}

#[test]
fn q3_software_renderer_maps_camera_viewport_to_front_buffer_region() {
    fn write_triangle_mesh(
        memory: &mut PpcSectionMem,
        trimesh_data: u32,
        points_ptr: u32,
        triangles_ptr: u32,
    ) {
        memory
            .write_u32_be(trimesh_data + PPC_Q3_TRIMESH_NUM_TRIANGLES_OFFSET, 1)
            .unwrap();
        memory
            .write_u32_be(
                trimesh_data + PPC_Q3_TRIMESH_TRIANGLES_OFFSET,
                triangles_ptr,
            )
            .unwrap();
        memory
            .write_u32_be(trimesh_data + PPC_Q3_TRIMESH_NUM_POINTS_OFFSET, 3)
            .unwrap();
        memory
            .write_u32_be(trimesh_data + PPC_Q3_TRIMESH_POINTS_OFFSET, points_ptr)
            .unwrap();
        ppc_write_q3_vector3d(memory, points_ptr, (-1.6, -1.6, -2.0)).unwrap();
        ppc_write_q3_vector3d(memory, points_ptr + 12, (-1.6, 1.6, -2.0)).unwrap();
        ppc_write_q3_vector3d(memory, points_ptr + 24, (1.6, -1.6, -2.0)).unwrap();
        memory.write_u32_be(triangles_ptr, 0).unwrap();
        memory.write_u32_be(triangles_ptr + 4, 1).unwrap();
        memory.write_u32_be(triangles_ptr + 8, 2).unwrap();
    }

    fn diffuse_attribute(attribute_set: u32) -> PpcQ3AttributeRecord {
        let mut data = Vec::new();
        for value in [0.0f32, 1.0, 0.0] {
            data.extend_from_slice(&value.to_bits().to_be_bytes());
        }
        PpcQ3AttributeRecord {
            attribute_set,
            attribute_type: PPC_Q3_ATTRIBUTE_TYPE_DIFFUSE_COLOR,
            data,
        }
    }

    let view = PPC_Q3_OBJECT_BASE;
    let camera = PPC_Q3_OBJECT_BASE + PPC_Q3_OBJECT_STRIDE;
    let attribute_set = PPC_Q3_OBJECT_BASE + PPC_Q3_OBJECT_STRIDE * 2;
    let trimesh_data = PPC_DATA_BASE + 0x1000;
    let points_ptr = PPC_DATA_BASE + 0x1080;
    let triangles_ptr = PPC_DATA_BASE + 0x1100;
    let front_base = PPC_HEAP_BASE + 0x1000;
    let pef = synthetic_pef_with_library_import(b"QuickDraw\xaa 3D", b"Q3TriMesh_Submit");
    let mut loaded = load_pef_application(&pef).unwrap();
    loaded.memory.add_region(trimesh_data, vec![0; 0x200]);
    loaded.memory.add_region(front_base, vec![0; 8 * 8 * 2]);
    loaded.gworlds = vec![PpcGWorldRecord {
        ui_theme: crate::ui_theme::UiThemeId::ClassicSystem7,
        port: PPC_MAIN_GWORLD,
        pixmap_handle: 0,
        pixmap: 0,
        base_addr: front_base,
        gdevice: PPC_MAIN_GDEVICE,
        width: 8,
        height: 8,
        depth: 16,
        row_bytes: 16,
        pixels_locked: false,
        pixels_no_purge: false,
    }];
    loaded
        .current_gworld
        .with_mut(|current_gworld| *current_gworld = PPC_MAIN_GWORLD);
    write_triangle_mesh(&mut loaded.memory, trimesh_data, points_ptr, triangles_ptr);

    let mut view_state = PpcQ3ViewStateRecord::new(view);
    view_state.camera = camera;
    loaded.q3_views.push(view_state);
    loaded.q3_cameras.push(PpcQ3CameraRecord {
        camera,
        camera_type: PPC_Q3_CAMERA_TYPE_VIEW_ANGLE_ASPECT,
        placement: PpcQ3CameraPlacement {
            camera_location: (0.0, 0.0, 0.0),
            point_of_interest: (0.0, 0.0, -1.0),
            up_vector: (0.0, 1.0, 0.0),
        },
        range_hither: 1.0,
        range_yon: 5.0,
        viewport_origin: (0.0, 1.0),
        viewport_width: 1.0,
        viewport_height: 2.0,
        projection: PpcQ3CameraProjection::ViewAngleAspect {
            fov: std::f32::consts::FRAC_PI_2,
            aspect_ratio_x_to_y: 1.0,
        },
    });
    loaded.q3_submissions.push(PpcQ3SubmissionRecord {
        view,
        kind: PpcQ3SubmissionKind::TriMesh,
        primary: trimesh_data,
        secondary: 0,
    });
    loaded
        .q3_submission_transforms
        .push(PpcQ3SubmissionTransformRecord {
            view,
            kind: PpcQ3SubmissionKind::TriMesh,
            primary: trimesh_data,
            secondary: 0,
            local_to_world: ppc_q3_matrix4x4_identity(),
        });
    loaded
        .q3_submission_materials
        .push(PpcQ3SubmissionMaterialRecord {
            view,
            kind: PpcQ3SubmissionKind::TriMesh,
            primary: trimesh_data,
            secondary: 0,
            shader: 0,
            illumination_type: PPC_Q3_ILLUMINATION_TYPE_PHONG,
            styles: Vec::new(),
            fog_style: None,
            attributes: vec![diffuse_attribute(attribute_set)],
            shader_uv_transform: None,
            shader_boundary: None,
            texture_shader: None,
            mipmap_texture: None,
        });
    loaded
        .q3_submission_lights
        .push(PpcQ3SubmissionLightRecord {
            view,
            kind: PpcQ3SubmissionKind::TriMesh,
            primary: trimesh_data,
            secondary: 0,
            light_group: 0,
            lights: Vec::new(),
        });

    let stats = loaded.render_q3_scene_commands_to_front_buffer();

    assert_eq!(stats.commands, 1);
    assert_eq!(stats.triangles, 1);
    assert!(stats.pixels > 0);
    let mut left_half_green_pixels = 0;
    let mut right_half_green_pixels = 0;
    for y in 0..8 {
        for x in 0..8 {
            if loaded.memory.read_u16_be(front_base + y * 16 + x * 2) != Some(0x03e0) {
                continue;
            }
            if x < 4 {
                left_half_green_pixels += 1;
            } else {
                right_half_green_pixels += 1;
            }
        }
    }
    assert_eq!(left_half_green_pixels, 0);
    assert!(right_half_green_pixels > 0);
}

#[test]
fn q3_software_renderer_maps_explicit_draw_context_pane_to_front_buffer_region() {
    let view = PPC_Q3_OBJECT_BASE;
    let camera = PPC_Q3_OBJECT_BASE + PPC_Q3_OBJECT_STRIDE;
    let draw_context = PPC_Q3_OBJECT_BASE + PPC_Q3_OBJECT_STRIDE * 2;
    let attribute_set = PPC_Q3_OBJECT_BASE + PPC_Q3_OBJECT_STRIDE * 3;
    let trimesh_data = PPC_DATA_BASE + 0x1000;
    let points_ptr = PPC_DATA_BASE + 0x1080;
    let triangles_ptr = PPC_DATA_BASE + 0x1100;
    let front_base = PPC_HEAP_BASE + 0x1000;
    let pef = synthetic_pef_with_library_import(b"QuickDraw\xaa 3D", b"Q3TriMesh_Submit");
    let mut loaded = load_pef_application(&pef).unwrap();
    loaded.memory.add_region(trimesh_data, vec![0; 0x200]);
    loaded.memory.add_region(front_base, vec![0; 8 * 8 * 2]);
    loaded.gworlds = vec![PpcGWorldRecord {
        ui_theme: crate::ui_theme::UiThemeId::ClassicSystem7,
        port: PPC_MAIN_GWORLD,
        pixmap_handle: 0,
        pixmap: 0,
        base_addr: front_base,
        gdevice: PPC_MAIN_GDEVICE,
        width: 8,
        height: 8,
        depth: 16,
        row_bytes: 16,
        pixels_locked: false,
        pixels_no_purge: false,
    }];
    loaded
        .current_gworld
        .with_mut(|current_gworld| *current_gworld = PPC_MAIN_GWORLD);
    loaded.q3_objects.push(PpcQ3ObjectRecord {
        object: draw_context,
        kind: PpcQ3ObjectKind::Generic,
        object_type: PPC_Q3_DRAW_CONTEXT_TYPE_MACINTOSH,
        source: PpcQ3ObjectSource::default(),
        data_ptr: 0,
        data_size: PPC_Q3_MAC_DRAW_CONTEXT_DATA_SIZE,
    });
    let mut draw_context_data = vec![0; PPC_Q3_MAC_DRAW_CONTEXT_DATA_SIZE as usize];
    ppc_q3_write_u32_to_slice(
        &mut draw_context_data,
        PPC_Q3_DRAW_CONTEXT_CLEAR_METHOD_OFFSET,
        PPC_Q3_CLEAR_METHOD_WITH_COLOR,
    )
    .unwrap();
    draw_context_data[PPC_Q3_DRAW_CONTEXT_CLEAR_COLOR_RED_OFFSET as usize
        ..PPC_Q3_DRAW_CONTEXT_CLEAR_COLOR_RED_OFFSET as usize + 4]
        .copy_from_slice(&1.0f32.to_bits().to_be_bytes());
    ppc_q3_write_u32_to_slice(
        &mut draw_context_data,
        PPC_Q3_MAC_DRAW_CONTEXT_PORT_OFFSET,
        PPC_MAIN_GWORLD,
    )
    .unwrap();
    ppc_q3_write_u32_to_slice(
        &mut draw_context_data,
        PPC_Q3_DRAW_CONTEXT_PANE_STATE_OFFSET,
        1,
    )
    .unwrap();
    for (offset, value) in [(0, 2.0f32), (4, 1.0f32), (8, 6.0f32), (12, 5.0f32)] {
        draw_context_data[PPC_Q3_DRAW_CONTEXT_PANE_OFFSET as usize + offset
            ..PPC_Q3_DRAW_CONTEXT_PANE_OFFSET as usize + offset + 4]
            .copy_from_slice(&value.to_bits().to_be_bytes());
    }
    loaded.q3_draw_contexts.push(PpcQ3DrawContextRecord {
        draw_context,
        draw_context_type: PPC_Q3_DRAW_CONTEXT_TYPE_MACINTOSH,
        data: draw_context_data,
    });
    loaded
        .memory
        .write_u32_be(trimesh_data + PPC_Q3_TRIMESH_NUM_TRIANGLES_OFFSET, 1)
        .unwrap();
    loaded
        .memory
        .write_u32_be(
            trimesh_data + PPC_Q3_TRIMESH_TRIANGLES_OFFSET,
            triangles_ptr,
        )
        .unwrap();
    loaded
        .memory
        .write_u32_be(trimesh_data + PPC_Q3_TRIMESH_NUM_POINTS_OFFSET, 3)
        .unwrap();
    loaded
        .memory
        .write_u32_be(trimesh_data + PPC_Q3_TRIMESH_POINTS_OFFSET, points_ptr)
        .unwrap();
    ppc_write_q3_vector3d(&mut loaded.memory, points_ptr, (-2.0, -2.0, -2.0)).unwrap();
    ppc_write_q3_vector3d(&mut loaded.memory, points_ptr + 12, (-2.0, 2.0, -2.0)).unwrap();
    ppc_write_q3_vector3d(&mut loaded.memory, points_ptr + 24, (2.0, -2.0, -2.0)).unwrap();
    loaded.memory.write_u32_be(triangles_ptr, 0).unwrap();
    loaded.memory.write_u32_be(triangles_ptr + 4, 1).unwrap();
    loaded.memory.write_u32_be(triangles_ptr + 8, 2).unwrap();

    let mut view_state = PpcQ3ViewStateRecord::new(view);
    view_state.camera = camera;
    view_state.draw_context = draw_context;
    loaded.q3_views.push(view_state);
    loaded.q3_cameras.push(PpcQ3CameraRecord {
        camera,
        camera_type: PPC_Q3_CAMERA_TYPE_VIEW_ANGLE_ASPECT,
        placement: PpcQ3CameraPlacement {
            camera_location: (0.0, 0.0, 0.0),
            point_of_interest: (0.0, 0.0, -1.0),
            up_vector: (0.0, 1.0, 0.0),
        },
        range_hither: 1.0,
        range_yon: 5.0,
        viewport_origin: (-1.0, 1.0),
        viewport_width: 2.0,
        viewport_height: 2.0,
        projection: PpcQ3CameraProjection::ViewAngleAspect {
            fov: std::f32::consts::FRAC_PI_2,
            aspect_ratio_x_to_y: 1.0,
        },
    });
    loaded.q3_submissions.push(PpcQ3SubmissionRecord {
        view,
        kind: PpcQ3SubmissionKind::TriMesh,
        primary: trimesh_data,
        secondary: 0,
    });
    loaded
        .q3_submission_transforms
        .push(PpcQ3SubmissionTransformRecord {
            view,
            kind: PpcQ3SubmissionKind::TriMesh,
            primary: trimesh_data,
            secondary: 0,
            local_to_world: ppc_q3_matrix4x4_identity(),
        });
    let mut diffuse = Vec::new();
    for value in [0.0f32, 1.0, 0.0] {
        diffuse.extend_from_slice(&value.to_bits().to_be_bytes());
    }
    loaded
        .q3_submission_materials
        .push(PpcQ3SubmissionMaterialRecord {
            view,
            kind: PpcQ3SubmissionKind::TriMesh,
            primary: trimesh_data,
            secondary: 0,
            shader: 0,
            illumination_type: PPC_Q3_ILLUMINATION_TYPE_PHONG,
            styles: Vec::new(),
            fog_style: None,
            attributes: vec![PpcQ3AttributeRecord {
                attribute_set,
                attribute_type: PPC_Q3_ATTRIBUTE_TYPE_DIFFUSE_COLOR,
                data: diffuse,
            }],
            shader_uv_transform: None,
            shader_boundary: None,
            texture_shader: None,
            mipmap_texture: None,
        });
    loaded
        .q3_submission_lights
        .push(PpcQ3SubmissionLightRecord {
            view,
            kind: PpcQ3SubmissionKind::TriMesh,
            primary: trimesh_data,
            secondary: 0,
            light_group: 0,
            lights: Vec::new(),
        });

    let stats = loaded.render_q3_scene_commands_to_front_buffer();

    assert_eq!(stats.commands, 1);
    assert_eq!(stats.triangles, 1);
    assert!(stats.pixels > 0);
    assert_eq!(
        loaded.memory.read_u16_be(front_base + 16 + 5 * 2),
        Some(0x7c00),
        "draw-context clear color must fill uncovered pane pixels"
    );
    for y in 0..8 {
        for x in 0..8 {
            let pixel = loaded.memory.read_u16_be(front_base + y * 16 + x * 2);
            if (2..6).contains(&x) && (1..5).contains(&y) {
                continue;
            }
            assert_ne!(pixel, Some(0x03e0), "green pixel outside pane at {x},{y}");
        }
    }
}

#[test]
fn q3_software_surface_maps_rgb555_through_indexed_clut() {
    let front_base = PPC_HEAP_BASE + 0x1000;
    let pef = synthetic_pef_with_library_import(b"QuickDraw\xaa 3D", b"Q3TriMesh_Submit");
    let mut loaded = load_pef_application(&pef).unwrap();
    loaded.memory.add_region(front_base, vec![0; 4]);
    let front_buffer = PpcFrontBuffer {
        base_addr: front_base,
        row_bytes: 2,
        width: 2,
        height: 2,
        depth: 8,
    };
    let mut clut = [[0; 3]; 256];
    clut[42] = [0, u16::MAX, u16::MAX];
    let surface =
        PpcQ3SoftwareFrontBufferSurface::new(&mut loaded.memory, front_buffer, Some(clut));

    assert!(surface.write_pixel(&mut loaded.memory, (1, 0), 0x03ff));
    assert_eq!(loaded.memory.read_u8(front_base + 1), Some(42));
    assert_eq!(surface.read_pixel(&mut loaded.memory, (1, 0)), Some(0x03ff));
}

#[test]
fn q3_mac_draw_context_translates_port_local_pane_to_framebuffer() {
    let front_buffer = PpcFrontBuffer {
        base_addr: PPC_MAIN_SCREEN_BASE,
        row_bytes: 1600,
        width: 800,
        height: 600,
        depth: 16,
    };
    let mut data = vec![0; PPC_Q3_MAC_DRAW_CONTEXT_DATA_SIZE as usize];
    ppc_q3_write_u32_to_slice(&mut data, PPC_Q3_DRAW_CONTEXT_PANE_STATE_OFFSET, 1).unwrap();
    for (offset, value) in [(0, 30.0f32), (4, 80.0), (8, 125.0), (12, 130.0)] {
        data[PPC_Q3_DRAW_CONTEXT_PANE_OFFSET as usize + offset
            ..PPC_Q3_DRAW_CONTEXT_PANE_OFFSET as usize + offset + 4]
            .copy_from_slice(&value.to_bits().to_be_bytes());
    }
    let record = PpcQ3DrawContextRecord {
        draw_context: PPC_Q3_OBJECT_BASE,
        draw_context_type: PPC_Q3_DRAW_CONTEXT_TYPE_MACINTOSH,
        data,
    };

    assert_eq!(
        ppc_q3_draw_context_viewport_with_origin(&record, front_buffer, 40, 40),
        Some(PpcQ3ViewportRect {
            left: 70,
            top: 120,
            right: 165,
            bottom: 170,
        })
    );
}

#[test]
fn q3_software_renderer_projects_orthographic_and_view_plane_cameras() {
    fn assert_close(actual: f32, expected: f32) {
        assert!(
            (actual - expected).abs() < 0.001,
            "expected {expected}, got {actual}"
        );
    }

    fn projection_command(view: u32, camera: PpcQ3CameraRecord) -> PpcQ3SceneTriMeshCommand {
        PpcQ3SceneTriMeshCommand {
            submission_index: 0,
            view_state: PpcQ3ViewStateRecord::new(view),
            camera: Some(camera),
            geometry: PpcQ3SceneTriMeshGeometry {
                source: PpcQ3SceneTriMeshSource::DataPtr(0),
                data: Vec::new(),
            },
            local_to_world: ppc_q3_matrix4x4_identity(),
            material: PpcQ3SubmissionMaterialRecord {
                view,
                kind: PpcQ3SubmissionKind::TriMesh,
                primary: 0,
                secondary: 0,
                shader: 0,
                illumination_type: PPC_Q3_ILLUMINATION_TYPE_PHONG,
                styles: Vec::new(),
                fog_style: None,
                attributes: Vec::new(),
                shader_uv_transform: None,
                shader_boundary: None,
                texture_shader: None,
                mipmap_texture: None,
            },
            lights: PpcQ3SubmissionLightRecord {
                view,
                kind: PpcQ3SubmissionKind::TriMesh,
                primary: 0,
                secondary: 0,
                light_group: 0,
                lights: Vec::new(),
            },
        }
    }

    let view = PPC_Q3_OBJECT_BASE;
    let front_buffer = PpcFrontBuffer {
        base_addr: PPC_HEAP_BASE,
        row_bytes: 640 * 2,
        width: 640,
        height: 480,
        depth: 16,
    };
    let placement = PpcQ3CameraPlacement {
        camera_location: (0.0, 0.0, 0.0),
        point_of_interest: (0.0, 0.0, -1.0),
        up_vector: (0.0, 1.0, 0.0),
    };
    let orthographic = PpcQ3CameraRecord {
        camera: PPC_Q3_OBJECT_BASE + PPC_Q3_OBJECT_STRIDE,
        camera_type: PPC_Q3_CAMERA_TYPE_ORTHOGRAPHIC,
        placement,
        range_hither: 1.0,
        range_yon: 10.0,
        viewport_origin: (-1.0, 1.0),
        viewport_width: 2.0,
        viewport_height: 2.0,
        projection: PpcQ3CameraProjection::Orthographic {
            left: -2.0,
            top: 1.0,
            right: 2.0,
            bottom: -1.0,
        },
    };
    let orthographic_command = projection_command(view, orthographic);

    let top_left = ppc_q3_software_project_point(
        &orthographic_command,
        front_buffer,
        PpcQ3ViewportRect::full(front_buffer),
        (-2.0, 1.0, -1.0),
    )
    .unwrap();
    let bottom_right = ppc_q3_software_project_point(
        &orthographic_command,
        front_buffer,
        PpcQ3ViewportRect::full(front_buffer),
        (2.0, -1.0, -10.0),
    )
    .unwrap();
    assert_close(top_left.x, 0.0);
    assert_close(top_left.y, 0.0);
    assert_close(top_left.z, -1.0);
    assert_close(bottom_right.x, 639.0);
    assert_close(bottom_right.y, 479.0);
    assert_close(bottom_right.z, 1.0);

    let view_plane = PpcQ3CameraRecord {
        camera: PPC_Q3_OBJECT_BASE + PPC_Q3_OBJECT_STRIDE * 2,
        camera_type: PPC_Q3_CAMERA_TYPE_VIEW_PLANE,
        placement,
        range_hither: 1.0,
        range_yon: 10.0,
        viewport_origin: (-1.0, 1.0),
        viewport_width: 2.0,
        viewport_height: 2.0,
        projection: PpcQ3CameraProjection::ViewPlane {
            view_plane: 2.0,
            half_width_at_view_plane: 2.0,
            half_height_at_view_plane: 1.0,
            center_x_on_view_plane: 0.5,
            center_y_on_view_plane: -0.25,
        },
    };
    let view_plane_command = projection_command(view, view_plane);

    let centered = ppc_q3_software_project_point(
        &view_plane_command,
        front_buffer,
        PpcQ3ViewportRect::full(front_buffer),
        (0.5, -0.25, -2.0),
    )
    .unwrap();
    let top_left = ppc_q3_software_project_point(
        &view_plane_command,
        front_buffer,
        PpcQ3ViewportRect::full(front_buffer),
        (-1.5, 0.75, -2.0),
    )
    .unwrap();
    let bottom_right = ppc_q3_software_project_point(
        &view_plane_command,
        front_buffer,
        PpcQ3ViewportRect::full(front_buffer),
        (2.5, -1.25, -2.0),
    )
    .unwrap();
    assert_close(centered.x, 319.5);
    assert_close(centered.y, 239.5);
    assert_close(top_left.x, 0.0);
    assert_close(top_left.y, 0.0);
    assert_close(bottom_right.x, 639.0);
    assert_close(bottom_right.y, 479.0);
}

#[test]
fn hle_import_runner_records_q3_fog_style_submit_state() {
    let view = PPC_Q3_OBJECT_BASE;
    let fog_data_ptr = PPC_DATA_BASE + 0x1000;
    let pef = synthetic_pef_with_library_import(b"QuickDraw\xaa 3D", b"Q3FogStyle_Submit");
    let mut loaded = load_pef_application(&pef).unwrap();
    loaded
        .q3_objects
        .push(test_q3_object(view, PPC_Q3_TYPE_VIEW));
    loaded
        .memory
        .add_region(fog_data_ptr, vec![0; PPC_Q3_FOG_STYLE_DATA_SIZE as usize]);
    loaded.memory.write_u32_be(fog_data_ptr, 1).unwrap();
    loaded
        .memory
        .write_u32_be(fog_data_ptr + 4, PPC_Q3_FOG_MODE_LINEAR)
        .unwrap();
    ppc_write_f32_be(&mut loaded.memory, fog_data_ptr + 8, 12.0).unwrap();
    ppc_write_f32_be(&mut loaded.memory, fog_data_ptr + 12, 96.0).unwrap();
    ppc_write_f32_be(&mut loaded.memory, fog_data_ptr + 16, 0.75).unwrap();
    ppc_write_f32_be(&mut loaded.memory, fog_data_ptr + 20, 1.0).unwrap();
    ppc_write_f32_be(&mut loaded.memory, fog_data_ptr + 24, 0.2).unwrap();
    ppc_write_f32_be(&mut loaded.memory, fog_data_ptr + 28, 0.3).unwrap();
    ppc_write_f32_be(&mut loaded.memory, fog_data_ptr + 32, 0.4).unwrap();
    loaded.cpu.gpr[3] = fog_data_ptr;
    loaded.cpu.gpr[4] = view;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], 1);
    assert_eq!(
        loaded.q3_submissions,
        vec![PpcQ3SubmissionRecord {
            view,
            kind: PpcQ3SubmissionKind::FogStyle,
            primary: fog_data_ptr,
            secondary: 1,
        }]
    );
    ppc_write_f32_be(&mut loaded.memory, fog_data_ptr + 8, 999.0).unwrap();
    assert_eq!(
        loaded.q3_fog_styles,
        vec![PpcQ3FogStyleRecord {
            view,
            data_ptr: fog_data_ptr,
            data: PpcQ3FogStyleData {
                state: 1,
                mode: PPC_Q3_FOG_MODE_LINEAR,
                fog_start: 12.0,
                fog_end: 96.0,
                density: 0.75,
                color: (1.0, 0.2, 0.3, 0.4),
            },
        }]
    );

    let submission_count = loaded.q3_submissions.len();
    let fog_style_count = loaded.q3_fog_styles.len();
    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.cpu.gpr[3] = fog_data_ptr;
    loaded.cpu.gpr[4] = view + PPC_Q3_OBJECT_STRIDE;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], 0);
    assert_eq!(loaded.q3_submissions.len(), submission_count);
    assert_eq!(loaded.q3_fog_styles.len(), fog_style_count);
    assert_eq!(
        loaded.q3_error_state.first_error,
        PPC_Q3_ERROR_INVALID_OBJECT_PARAMETER
    );
}

#[test]
fn hle_import_runner_removes_q3_side_state_on_object_dispose() {
    let pef = synthetic_pef_with_library_import(b"QuickDraw\xaa 3D", b"Q3Object_Dispose");
    let mut loaded = load_pef_application(&pef).unwrap();
    let object = PPC_Q3_OBJECT_BASE;
    let surviving_view = PPC_Q3_OBJECT_BASE + PPC_Q3_OBJECT_STRIDE * 13;
    let surviving_shader = PPC_Q3_OBJECT_BASE + PPC_Q3_OBJECT_STRIDE * 14;
    let surviving_attribute_set = PPC_Q3_OBJECT_BASE + PPC_Q3_OBJECT_STRIDE * 15;
    let secondary_only_view = PPC_Q3_OBJECT_BASE + PPC_Q3_OBJECT_STRIDE * 16;
    let secondary_only_primary = PPC_Q3_OBJECT_BASE + PPC_Q3_OBJECT_STRIDE * 17;
    let secondary_only_shader = PPC_Q3_OBJECT_BASE + PPC_Q3_OBJECT_STRIDE * 18;
    let secondary_only_light_group = PPC_Q3_OBJECT_BASE + PPC_Q3_OBJECT_STRIDE * 19;
    let secondary_only_light = PPC_Q3_OBJECT_BASE + PPC_Q3_OBJECT_STRIDE * 20;
    let file_owned_group = PPC_Q3_OBJECT_BASE + PPC_Q3_OBJECT_STRIDE * 21;
    let file_owned_parent_group = PPC_Q3_OBJECT_BASE + PPC_Q3_OBJECT_STRIDE * 22;
    loaded.q3_objects.push(PpcQ3ObjectRecord {
        object,
        kind: PpcQ3ObjectKind::Generic,
        object_type: PPC_Q3_TYPE_NONE,
        source: PpcQ3ObjectSource::default(),
        data_ptr: 0,
        data_size: 0,
    });
    loaded
        .q3_renderer_preferences
        .push(PpcQ3RendererPreferenceRecord {
            renderer: object,
            double_buffer_bypass: Some(1),
            preference_vendor: Some(PPC_QA_VENDOR_APPLE),
            preference_engine: Some(PPC_QA_ENGINE_APPLE_SW),
            rave_context_hints: Some(0x1234_5678),
            rave_texture_filter: Some(2),
        });
    loaded.q3_files.push(PpcQ3FileRecord {
        file: object,
        storage: PPC_Q3_OBJECT_BASE + PPC_Q3_OBJECT_STRIDE,
        is_open: true,
        object_type: 0,
        read_offset: 0,
        read_object: PPC_Q3_OBJECT_BASE + PPC_Q3_OBJECT_STRIDE * 2,
    });
    loaded
        .q3_group_memberships
        .push(PpcQ3GroupMembershipRecord {
            group: object,
            object: PPC_Q3_OBJECT_BASE + PPC_Q3_OBJECT_STRIDE,
            before: None,
        });
    loaded.q3_file_groups.push(PpcQ3FileGroupRecord {
        file: PPC_Q3_OBJECT_BASE + PPC_Q3_OBJECT_STRIDE * 3,
        offset: 24,
        group: object,
        group_type: u32::from_be_bytes(*b"dspg"),
        parent_group: 0,
        group_depth: 1,
    });
    loaded.q3_file_groups.push(PpcQ3FileGroupRecord {
        file: object,
        offset: 48,
        group: file_owned_group,
        group_type: u32::from_be_bytes(*b"ogtg"),
        parent_group: file_owned_parent_group,
        group_depth: 2,
    });
    loaded.q3_views.push(PpcQ3ViewStateRecord::new(object));
    loaded.q3_submissions.push(PpcQ3SubmissionRecord {
        view: object,
        kind: PpcQ3SubmissionKind::Object,
        primary: object,
        secondary: 0,
    });
    loaded.q3_submissions.push(PpcQ3SubmissionRecord {
        view: secondary_only_view,
        kind: PpcQ3SubmissionKind::TriMesh,
        primary: secondary_only_primary,
        secondary: object,
    });
    loaded.q3_view_transforms.push(PpcQ3ViewTransformRecord {
        view: object,
        stack: vec![ppc_q3_matrix4x4_identity()],
        local_to_world: ppc_q3_matrix4x4_identity(),
    });
    loaded
        .q3_submission_transforms
        .push(PpcQ3SubmissionTransformRecord {
            view: object,
            kind: PpcQ3SubmissionKind::Object,
            primary: object,
            secondary: 0,
            local_to_world: ppc_q3_matrix4x4_identity(),
        });
    loaded
        .q3_submission_transforms
        .push(PpcQ3SubmissionTransformRecord {
            view: secondary_only_view,
            kind: PpcQ3SubmissionKind::TriMesh,
            primary: secondary_only_primary,
            secondary: object,
            local_to_world: ppc_q3_matrix4x4_identity(),
        });
    loaded.q3_view_materials.push(PpcQ3ViewMaterialRecord {
        view: object,
        shader: object,
        illumination_type: PPC_Q3_ILLUMINATION_TYPE_PHONG,
        styles: vec![PpcQ3StyleRecord {
            style: object,
            kind: PpcQ3StyleKind::Fill,
            value: 1,
        }],
        fog_style: Some(PpcQ3FogStyleData {
            state: 1,
            mode: PPC_Q3_FOG_MODE_LINEAR,
            fog_start: 1.0,
            fog_end: 2.0,
            density: 0.5,
            color: (0.1, 0.2, 0.3, 1.0),
        }),
        attributes: Vec::new(),
    });
    loaded.q3_view_materials.push(PpcQ3ViewMaterialRecord {
        view: surviving_view,
        shader: surviving_shader,
        illumination_type: PPC_Q3_ILLUMINATION_TYPE_PHONG,
        styles: Vec::new(),
        fog_style: None,
        attributes: vec![
            PpcQ3AttributeRecord {
                attribute_set: object,
                attribute_type: PPC_Q3_ATTRIBUTE_TYPE_DIFFUSE_COLOR,
                data: vec![0; 12],
            },
            PpcQ3AttributeRecord {
                attribute_set: surviving_attribute_set,
                attribute_type: PPC_Q3_ATTRIBUTE_TYPE_DIFFUSE_COLOR,
                data: vec![1; 12],
            },
        ],
    });
    loaded
        .q3_submission_materials
        .push(PpcQ3SubmissionMaterialRecord {
            view: object,
            kind: PpcQ3SubmissionKind::Object,
            primary: object,
            secondary: 0,
            shader: object,
            illumination_type: PPC_Q3_ILLUMINATION_TYPE_PHONG,
            styles: vec![PpcQ3StyleRecord {
                style: object,
                kind: PpcQ3StyleKind::Fill,
                value: 1,
            }],
            fog_style: Some(PpcQ3FogStyleData {
                state: 1,
                mode: PPC_Q3_FOG_MODE_LINEAR,
                fog_start: 1.0,
                fog_end: 2.0,
                density: 0.5,
                color: (0.1, 0.2, 0.3, 1.0),
            }),
            attributes: Vec::new(),
            shader_uv_transform: Some(PpcQ3ShaderUvTransformRecord {
                shader: object,
                matrix: ppc_q3_matrix3x3_identity(),
            }),
            shader_boundary: Some(PpcQ3ShaderBoundaryRecord {
                shader: object,
                u_boundary: PPC_Q3_SHADER_UV_BOUNDARY_CLAMP,
                v_boundary: PPC_Q3_SHADER_UV_BOUNDARY_WRAP,
            }),
            texture_shader: Some(PpcQ3TextureShaderRecord {
                shader: object,
                texture: object,
            }),
            mipmap_texture: Some(PpcQ3MipmapTextureRecord {
                texture: object,
                mipmap: vec![0; PPC_Q3_MIPMAP_COPY_SIZE as usize],
            }),
        });
    loaded
        .q3_submission_materials
        .push(PpcQ3SubmissionMaterialRecord {
            view: PPC_Q3_OBJECT_BASE + PPC_Q3_OBJECT_STRIDE,
            kind: PpcQ3SubmissionKind::TriMesh,
            primary: PPC_Q3_OBJECT_BASE + PPC_Q3_OBJECT_STRIDE * 2,
            secondary: 0,
            shader: PPC_Q3_OBJECT_BASE + PPC_Q3_OBJECT_STRIDE * 3,
            illumination_type: PPC_Q3_ILLUMINATION_TYPE_PHONG,
            styles: Vec::new(),
            fog_style: None,
            attributes: Vec::new(),
            shader_uv_transform: None,
            shader_boundary: None,
            texture_shader: Some(PpcQ3TextureShaderRecord {
                shader: PPC_Q3_OBJECT_BASE + PPC_Q3_OBJECT_STRIDE * 3,
                texture: object,
            }),
            mipmap_texture: None,
        });
    loaded
        .q3_submission_materials
        .push(PpcQ3SubmissionMaterialRecord {
            view: PPC_Q3_OBJECT_BASE + PPC_Q3_OBJECT_STRIDE * 7,
            kind: PpcQ3SubmissionKind::TriMesh,
            primary: PPC_Q3_OBJECT_BASE + PPC_Q3_OBJECT_STRIDE * 8,
            secondary: 0,
            shader: PPC_Q3_OBJECT_BASE + PPC_Q3_OBJECT_STRIDE * 9,
            illumination_type: PPC_Q3_ILLUMINATION_TYPE_PHONG,
            styles: Vec::new(),
            fog_style: None,
            attributes: vec![PpcQ3AttributeRecord {
                attribute_set: object,
                attribute_type: PPC_Q3_ATTRIBUTE_TYPE_DIFFUSE_COLOR,
                data: vec![0; 12],
            }],
            shader_uv_transform: None,
            shader_boundary: Some(PpcQ3ShaderBoundaryRecord {
                shader: object,
                u_boundary: PPC_Q3_SHADER_UV_BOUNDARY_WRAP,
                v_boundary: PPC_Q3_SHADER_UV_BOUNDARY_CLAMP,
            }),
            texture_shader: None,
            mipmap_texture: None,
        });
    loaded
        .q3_submission_materials
        .push(PpcQ3SubmissionMaterialRecord {
            view: secondary_only_view,
            kind: PpcQ3SubmissionKind::TriMesh,
            primary: secondary_only_primary,
            secondary: object,
            shader: secondary_only_shader,
            illumination_type: PPC_Q3_ILLUMINATION_TYPE_PHONG,
            styles: Vec::new(),
            fog_style: None,
            attributes: Vec::new(),
            shader_uv_transform: None,
            shader_boundary: None,
            texture_shader: None,
            mipmap_texture: None,
        });
    loaded
        .q3_submission_lights
        .push(PpcQ3SubmissionLightRecord {
            view: object,
            kind: PpcQ3SubmissionKind::Object,
            primary: object,
            secondary: 0,
            light_group: object,
            lights: vec![PpcQ3LightRecord {
                light: object,
                light_type: PPC_Q3_LIGHT_TYPE_AMBIENT,
                data: PpcQ3LightData {
                    is_on: 1,
                    brightness: 0.5,
                    color: (1.0, 1.0, 1.0),
                },
                kind: PpcQ3LightKind::Ambient,
            }],
        });
    loaded
        .q3_submission_lights
        .push(PpcQ3SubmissionLightRecord {
            view: secondary_only_view,
            kind: PpcQ3SubmissionKind::Object,
            primary: secondary_only_primary,
            secondary: object,
            light_group: secondary_only_light_group,
            lights: vec![PpcQ3LightRecord {
                light: secondary_only_light,
                light_type: PPC_Q3_LIGHT_TYPE_AMBIENT,
                data: PpcQ3LightData {
                    is_on: 1,
                    brightness: 0.5,
                    color: (1.0, 1.0, 1.0),
                },
                kind: PpcQ3LightKind::Ambient,
            }],
        });
    loaded
        .q3_view_state_stack
        .push(PpcQ3ViewStateSnapshotRecord {
            view: object,
            transform: Some(PpcQ3ViewTransformRecord {
                view: object,
                stack: vec![ppc_q3_matrix4x4_identity()],
                local_to_world: ppc_q3_matrix4x4_identity(),
            }),
            material: Some(PpcQ3ViewMaterialRecord {
                view: object,
                shader: object,
                illumination_type: PPC_Q3_ILLUMINATION_TYPE_PHONG,
                styles: vec![PpcQ3StyleRecord {
                    style: object,
                    kind: PpcQ3StyleKind::Fill,
                    value: 1,
                }],
                fog_style: None,
                attributes: vec![PpcQ3AttributeRecord {
                    attribute_set: object,
                    attribute_type: PPC_Q3_ATTRIBUTE_TYPE_DIFFUSE_COLOR,
                    data: vec![0; 12],
                }],
            }),
        });
    loaded.q3_completed_frames.push(PpcQ3CompletedFrameRecord {
        view: object,
        submissions: vec![PpcQ3SubmissionRecord {
            view: object,
            kind: PpcQ3SubmissionKind::Object,
            primary: object,
            secondary: 0,
        }],
        submission_transforms: Vec::new(),
        submission_materials: Vec::new(),
        submission_lights: Vec::new(),
        retained_trimeshes: Vec::new(),
    });
    loaded.q3_completed_frames.push(PpcQ3CompletedFrameRecord {
        view: PPC_Q3_OBJECT_BASE + PPC_Q3_OBJECT_STRIDE * 4,
        submissions: Vec::new(),
        submission_transforms: Vec::new(),
        submission_materials: vec![PpcQ3SubmissionMaterialRecord {
            view: PPC_Q3_OBJECT_BASE + PPC_Q3_OBJECT_STRIDE * 4,
            kind: PpcQ3SubmissionKind::TriMesh,
            primary: PPC_Q3_OBJECT_BASE + PPC_Q3_OBJECT_STRIDE * 5,
            secondary: 0,
            shader: PPC_Q3_OBJECT_BASE + PPC_Q3_OBJECT_STRIDE * 6,
            illumination_type: PPC_Q3_ILLUMINATION_TYPE_PHONG,
            styles: Vec::new(),
            fog_style: None,
            attributes: Vec::new(),
            shader_uv_transform: None,
            shader_boundary: None,
            texture_shader: Some(PpcQ3TextureShaderRecord {
                shader: PPC_Q3_OBJECT_BASE + PPC_Q3_OBJECT_STRIDE * 6,
                texture: object,
            }),
            mipmap_texture: None,
        }],
        submission_lights: Vec::new(),
        retained_trimeshes: Vec::new(),
    });
    loaded.q3_completed_frames.push(PpcQ3CompletedFrameRecord {
        view: PPC_Q3_OBJECT_BASE + PPC_Q3_OBJECT_STRIDE * 10,
        submissions: Vec::new(),
        submission_transforms: Vec::new(),
        submission_materials: vec![PpcQ3SubmissionMaterialRecord {
            view: PPC_Q3_OBJECT_BASE + PPC_Q3_OBJECT_STRIDE * 10,
            kind: PpcQ3SubmissionKind::TriMesh,
            primary: PPC_Q3_OBJECT_BASE + PPC_Q3_OBJECT_STRIDE * 11,
            secondary: 0,
            shader: PPC_Q3_OBJECT_BASE + PPC_Q3_OBJECT_STRIDE * 12,
            illumination_type: PPC_Q3_ILLUMINATION_TYPE_PHONG,
            styles: Vec::new(),
            fog_style: None,
            attributes: vec![PpcQ3AttributeRecord {
                attribute_set: object,
                attribute_type: PPC_Q3_ATTRIBUTE_TYPE_DIFFUSE_COLOR,
                data: vec![0; 12],
            }],
            shader_uv_transform: None,
            shader_boundary: Some(PpcQ3ShaderBoundaryRecord {
                shader: object,
                u_boundary: PPC_Q3_SHADER_UV_BOUNDARY_CLAMP,
                v_boundary: PPC_Q3_SHADER_UV_BOUNDARY_WRAP,
            }),
            texture_shader: None,
            mipmap_texture: None,
        }],
        submission_lights: Vec::new(),
        retained_trimeshes: Vec::new(),
    });
    loaded.q3_fog_styles.push(PpcQ3FogStyleRecord {
        view: object,
        data_ptr: PPC_DATA_BASE,
        data: PpcQ3FogStyleData {
            state: 1,
            mode: PPC_Q3_FOG_MODE_LINEAR,
            fog_start: 0.0,
            fog_end: 100.0,
            density: 1.0,
            color: (1.0, 1.0, 1.0, 1.0),
        },
    });
    loaded.q3_memory_storages.push(PpcQ3MemoryStorageRecord {
        storage: object,
        buffer_ptr: PPC_DATA_BASE,
        valid_size: 16,
        buffer_size: 32,
        owns_buffer: false,
    });
    loaded.q3_attributes.push(PpcQ3AttributeRecord {
        attribute_set: object,
        attribute_type: PPC_Q3_ATTRIBUTE_TYPE_DIFFUSE_COLOR,
        data: vec![0; 12],
    });
    loaded
        .q3_shader_uv_transforms
        .push(PpcQ3ShaderUvTransformRecord {
            shader: object,
            matrix: ppc_q3_matrix3x3_identity(),
        });
    loaded.q3_shader_boundaries.push(PpcQ3ShaderBoundaryRecord {
        shader: object,
        u_boundary: PPC_Q3_SHADER_UV_BOUNDARY_CLAMP,
        v_boundary: PPC_Q3_SHADER_UV_BOUNDARY_WRAP,
    });
    loaded.q3_mipmap_textures.push(PpcQ3MipmapTextureRecord {
        texture: object,
        mipmap: vec![0; PPC_Q3_MIPMAP_COPY_SIZE as usize],
    });
    loaded.q3_texture_shaders.push(PpcQ3TextureShaderRecord {
        shader: object,
        texture: PPC_Q3_OBJECT_BASE + PPC_Q3_OBJECT_STRIDE,
    });
    loaded.q3_trimeshes.push(PpcQ3TriMeshRecord {
        trimesh: object,
        data: vec![0; PPC_Q3_TRIMESH_DATA_SIZE as usize],
        triangle_attribute_sets: Vec::new(),
        get_data_copies: Vec::new(),
    });
    loaded.q3_styles.push(PpcQ3StyleRecord {
        style: object,
        kind: PpcQ3StyleKind::Fill,
        value: 1,
    });
    loaded.q3_cameras.push(PpcQ3CameraRecord {
        camera: object,
        camera_type: PPC_Q3_CAMERA_TYPE_VIEW_ANGLE_ASPECT,
        placement: PpcQ3CameraPlacement {
            camera_location: (0.0, 0.0, 0.0),
            point_of_interest: (0.0, 0.0, -1.0),
            up_vector: (0.0, 1.0, 0.0),
        },
        range_hither: 0.1,
        range_yon: 100.0,
        viewport_origin: (-1.0, 1.0),
        viewport_width: 2.0,
        viewport_height: 2.0,
        projection: PpcQ3CameraProjection::ViewAngleAspect {
            fov: std::f32::consts::FRAC_PI_2,
            aspect_ratio_x_to_y: 4.0 / 3.0,
        },
    });
    loaded.q3_lights.push(PpcQ3LightRecord {
        light: object,
        light_type: PPC_Q3_LIGHT_TYPE_AMBIENT,
        data: PpcQ3LightData {
            is_on: 1,
            brightness: 0.5,
            color: (1.0, 1.0, 1.0),
        },
        kind: PpcQ3LightKind::Ambient,
    });
    loaded.cpu.gpr[3] = object;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], 1);
    assert!(loaded.q3_objects.is_empty());
    assert!(loaded.q3_renderer_preferences.is_empty());
    assert!(loaded.q3_files.is_empty());
    assert!(loaded.q3_group_memberships.is_empty());
    assert_eq!(
        loaded.q3_file_groups,
        vec![PpcQ3FileGroupRecord {
            file: object,
            offset: 48,
            group: file_owned_group,
            group_type: u32::from_be_bytes(*b"ogtg"),
            parent_group: file_owned_parent_group,
            group_depth: 2,
        }]
    );
    assert!(loaded.q3_views.is_empty());
    assert!(loaded.q3_submissions.is_empty());
    assert!(loaded.q3_view_transforms.is_empty());
    assert!(loaded.q3_submission_transforms.is_empty());
    assert_eq!(
        loaded.q3_view_materials,
        vec![PpcQ3ViewMaterialRecord {
            view: surviving_view,
            shader: surviving_shader,
            illumination_type: PPC_Q3_ILLUMINATION_TYPE_PHONG,
            styles: Vec::new(),
            fog_style: None,
            attributes: vec![PpcQ3AttributeRecord {
                attribute_set: surviving_attribute_set,
                attribute_type: PPC_Q3_ATTRIBUTE_TYPE_DIFFUSE_COLOR,
                data: vec![1; 12],
            }],
        }]
    );
    assert!(loaded.q3_submission_materials.is_empty());
    assert!(loaded.q3_submission_lights.is_empty());
    assert!(loaded.q3_view_state_stack.is_empty());
    assert!(loaded.q3_completed_frames.is_empty());
    assert!(loaded.q3_fog_styles.is_empty());
    assert!(loaded.q3_memory_storages.is_empty());
    assert!(loaded.q3_attributes.is_empty());
    assert!(loaded.q3_shader_uv_transforms.is_empty());
    assert!(loaded.q3_shader_boundaries.is_empty());
    assert!(loaded.q3_mipmap_textures.is_empty());
    assert!(loaded.q3_texture_shaders.is_empty());
    assert!(loaded.q3_trimeshes.is_empty());
    assert!(loaded.q3_styles.is_empty());
    assert!(loaded.q3_cameras.is_empty());
    assert!(loaded.q3_lights.is_empty());
}

#[test]
fn hle_import_runner_handles_qa_first_engine() {
    let pef = synthetic_pef_with_library_import(
        b"QuickDraw\xaa 3D Accelerator",
        b"QADeviceGetFirstEngine",
    );
    let mut loaded = load_pef_application(&pef).unwrap();
    loaded.cpu.gpr[3] = 0;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], PPC_QA_ENGINE);
    assert_eq!(loaded.memory.read_u8(PPC_QA_ENGINE), Some(0));
}

#[test]
fn hle_import_runner_handles_qa_next_engine_end_of_list() {
    let pef = synthetic_pef_with_library_import(
        b"QuickDraw\xaa 3D Accelerator",
        b"QADeviceGetNextEngine",
    );
    let mut loaded = load_pef_application(&pef).unwrap();
    loaded.cpu.gpr[3] = 0;
    loaded.cpu.gpr[4] = PPC_QA_ENGINE;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], 0);
}

#[test]
fn hle_import_runner_handles_qa_engine_gestalt() {
    let pef =
        synthetic_pef_with_library_import(b"QuickDraw\xaa 3D Accelerator", b"QAEngineGestalt");
    let mut loaded = load_pef_application(&pef).unwrap();
    let response_ptr = PPC_DATA_BASE + 0x1000;
    loaded.memory.add_region(response_ptr, vec![0xaa; 64]);
    loaded.cpu.gpr[3] = PPC_QA_ENGINE;
    loaded.cpu.gpr[4] = 0;
    loaded.cpu.gpr[5] = response_ptr;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], 0);
    assert_eq!(
        loaded.memory.read_u32_be(response_ptr),
        Some(ppc_qa_optional_features())
    );

    for (selector, expected) in [
        (1, ppc_qa_fast_features()),
        (2, PPC_QA_VENDOR_APPLE),
        (3, PPC_QA_ENGINE_APPLE_SW),
        (4, 1),
        (5, PPC_QA_ENGINE_NAME.len() as u32),
        (7, PPC_QA_AVAILABLE_TEXTURE_MEMORY),
        (42, 0),
    ] {
        let mut loaded = load_pef_application(&pef).unwrap();
        loaded.memory.add_region(response_ptr, vec![0xaa; 4]);
        loaded.cpu.gpr[3] = PPC_QA_ENGINE;
        loaded.cpu.gpr[4] = selector;
        loaded.cpu.gpr[5] = response_ptr;

        let probe = loaded.run_with_hle_imports(64);

        assert_eq!(probe.handled_import_count, 1, "selector {selector}");
        assert_eq!(probe.unsupported_import_index, None, "selector {selector}");
        assert_eq!(loaded.cpu.gpr[3], 0, "selector {selector}");
        assert_eq!(
            loaded.memory.read_u32_be(response_ptr),
            Some(expected),
            "selector {selector}"
        );
    }

    let mut loaded = load_pef_application(&pef).unwrap();
    loaded.memory.add_region(response_ptr, vec![0xaa; 64]);
    loaded.cpu.gpr[3] = PPC_QA_ENGINE;
    loaded.cpu.gpr[4] = 6;
    loaded.cpu.gpr[5] = response_ptr;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], 0);
    for (offset, byte) in PPC_QA_ENGINE_NAME.iter().copied().enumerate() {
        assert_eq!(
            loaded.memory.read_u8(response_ptr + offset as u32),
            Some(byte)
        );
    }
    assert_eq!(
        loaded
            .memory
            .read_u8(response_ptr + PPC_QA_ENGINE_NAME.len() as u32),
        Some(0)
    );

    let mut loaded = load_pef_application(&pef).unwrap();
    let short_word_response_ptr = PPC_DATA_BASE + 0x1100;
    loaded
        .memory
        .add_region(short_word_response_ptr, vec![0xee; 2]);
    loaded.cpu.gpr[3] = PPC_QA_ENGINE;
    loaded.cpu.gpr[4] = 0;
    loaded.cpu.gpr[5] = short_word_response_ptr;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_PARAM_ERR));
    assert_eq!(loaded.memory.read_u8(short_word_response_ptr), Some(0xee));
    assert_eq!(
        loaded.memory.read_u8(short_word_response_ptr + 1),
        Some(0xee)
    );

    let mut loaded = load_pef_application(&pef).unwrap();
    let short_name_response_ptr = PPC_DATA_BASE + 0x1200;
    loaded
        .memory
        .add_region(short_name_response_ptr, vec![0xdd; 4]);
    loaded.cpu.gpr[3] = PPC_QA_ENGINE;
    loaded.cpu.gpr[4] = 6;
    loaded.cpu.gpr[5] = short_name_response_ptr;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_PARAM_ERR));
    assert_eq!(
        loaded.memory.read_u32_be(short_name_response_ptr),
        Some(0xdddd_dddd)
    );

    let mut loaded = load_pef_application(&pef).unwrap();
    let invalid_engine_response_ptr = PPC_DATA_BASE + 0x1300;
    loaded
        .memory
        .add_region(invalid_engine_response_ptr, vec![0xcc; 4]);
    loaded.cpu.gpr[3] = PPC_QA_ENGINE + 4;
    loaded.cpu.gpr[4] = 0;
    loaded.cpu.gpr[5] = invalid_engine_response_ptr;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_PARAM_ERR));
    assert_eq!(
        loaded.memory.read_u32_be(invalid_engine_response_ptr),
        Some(0xcccc_cccc)
    );

    let mut loaded = load_pef_application(&pef).unwrap();
    loaded.cpu.gpr[3] = PPC_QA_ENGINE;
    loaded.cpu.gpr[4] = 0;
    loaded.cpu.gpr[5] = 0;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_PARAM_ERR));
}
