use super::*;

#[test]
fn hle_import_runner_handles_collection_create_add_get_and_invalid_index() {
    let pef = synthetic_pef_with_library_import(b"ColMgrLib", b"NewCollection");
    let mut loaded = load_pef_application(&pef).unwrap();
    run_test_import(
        &mut loaded,
        PpcImportDispatcherTarget::Collection(PpcCollectionOperation::New),
    );
    let collection = loaded.cpu.gpr[3];
    assert_ne!(collection, 0);

    let source = PPC_DATA_BASE + 0x1000;
    let destination = PPC_DATA_BASE + 0x1100;
    let size_pointer = PPC_DATA_BASE + 0x1200;
    loaded.memory.add_region(source, b"native".to_vec());
    loaded.memory.add_region(destination, vec![0; 16]);
    loaded.memory.add_region(size_pointer, vec![0; 4]);
    let tag = u32::from_be_bytes(*b"data");
    loaded.cpu.gpr[3] = collection;
    loaded.cpu.gpr[4] = tag;
    loaded.cpu.gpr[5] = (-7i32) as u32;
    loaded.cpu.gpr[6] = 6;
    loaded.cpu.gpr[7] = source;
    run_test_import(
        &mut loaded,
        PpcImportDispatcherTarget::Collection(PpcCollectionOperation::AddItem),
    );
    assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_NO_ERR));

    loaded.memory.write_u32_be(size_pointer, 16).unwrap();
    loaded.cpu.gpr[3] = collection;
    loaded.cpu.gpr[4] = tag;
    loaded.cpu.gpr[5] = (-7i32) as u32;
    loaded.cpu.gpr[6] = size_pointer;
    loaded.cpu.gpr[7] = destination;
    run_test_import(
        &mut loaded,
        PpcImportDispatcherTarget::Collection(PpcCollectionOperation::GetItem),
    );
    assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_NO_ERR));
    assert_eq!(loaded.memory.read_u32_be(size_pointer), Some(6));
    assert_eq!(
        ppc_memory_read_bytes(&mut loaded.memory, destination, 6),
        Some(b"native".to_vec())
    );

    loaded.cpu.gpr[3] = collection;
    loaded.cpu.gpr[4] = u32::MAX;
    run_test_import(
        &mut loaded,
        PpcImportDispatcherTarget::Collection(PpcCollectionOperation::RemoveIndexedItem),
    );
    assert_eq!(
        loaded.cpu.gpr[3],
        ppc_i16_result(crate::collection_manager::COLLECTION_INDEX_RANGE_ERR)
    );
}

#[test]
fn hle_import_runner_collection_clone_shares_owner_count() {
    let pef = synthetic_pef_with_library_import(b"ColMgrLib", b"NewCollection");
    let mut loaded = load_pef_application(&pef).unwrap();
    run_test_import(
        &mut loaded,
        PpcImportDispatcherTarget::Collection(PpcCollectionOperation::New),
    );
    let collection = loaded.cpu.gpr[3];

    loaded.cpu.gpr[3] = collection;
    run_test_import(
        &mut loaded,
        PpcImportDispatcherTarget::Collection(PpcCollectionOperation::Clone),
    );
    assert_eq!(loaded.cpu.gpr[3], collection);

    loaded.cpu.gpr[3] = collection;
    run_test_import(
        &mut loaded,
        PpcImportDispatcherTarget::Collection(PpcCollectionOperation::CountOwners),
    );
    assert_eq!(loaded.cpu.gpr[3], 2);
}
