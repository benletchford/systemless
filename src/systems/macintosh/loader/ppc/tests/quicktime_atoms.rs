use super::*;

fn call(loaded: &mut PpcLoadedApp, name: &str, args: &[u32]) -> u32 {
    let target = dispatcher_target_for_import("QuickTimeLib", name);
    assert!(matches!(target, PpcImportDispatcherTarget::QtAtom(_)));
    for library in ["CarbonLib", "Apple;Carbon;Multimedia"] {
        assert_eq!(dispatcher_target_for_import(library, name), target);
    }
    loaded.cpu.gpr[3..3 + args.len()].copy_from_slice(args);
    run_test_import(loaded, target);
    loaded.cpu.gpr[3]
}

fn fixture() -> (PpcLoadedApp, u32) {
    let mut loaded = load_pef_application(&synthetic_pef_with_library_import(
        b"QuickTimeLib",
        b"QTNewAtomContainer",
    ))
    .unwrap();
    let out = PPC_DATA_BASE + 0x4000;
    loaded.memory.add_region(out, vec![0xa5; 128]);
    (loaded, out)
}

#[test]
fn quicktime_atoms_create_nested_locks_and_dispose_owned_container() {
    let (mut loaded, out) = fixture();
    assert_eq!(call(&mut loaded, "QTNewAtomContainer", &[out]), 0);
    let handle = loaded.memory.read_u32_be(out).unwrap();
    let ptr = loaded.memory.read_u32_be(handle).unwrap();
    assert_eq!(loaded.memory.read_u32_be(ptr + 12), Some(20));
    assert_eq!(
        loaded.memory.read_u32_be(ptr + 16),
        Some(u32::from_be_bytes(*b"sean"))
    );
    assert_eq!(
        call(&mut loaded, "QTCountChildrenOfType", &[handle, 0, 0]),
        0
    );
    for count in [1, 2] {
        assert_eq!(call(&mut loaded, "QTLockContainer", &[handle]), 0);
        assert_eq!(loaded.memory.read_u16_be(ptr + 10), Some(count));
    }
    assert_eq!(call(&mut loaded, "QTUnlockContainer", &[handle]), 0);
    loaded.cpu.gpr[3] = handle;
    run_test_import(&mut loaded, PpcImportDispatcherTarget::HGetState);
    assert_ne!(loaded.cpu.gpr[3] & 0x80, 0);
    assert_eq!(call(&mut loaded, "QTUnlockContainer", &[handle]), 0);
    loaded.cpu.gpr[3] = handle;
    run_test_import(&mut loaded, PpcImportDispatcherTarget::HGetState);
    assert_eq!(loaded.cpu.gpr[3] & 0x80, 0);
    assert_eq!(
        call(&mut loaded, "QTUnlockContainer", &[handle]) as i32,
        -50
    );
    assert_eq!(call(&mut loaded, "QTDisposeAtomContainer", &[handle]), 0);
    assert_eq!(
        call(&mut loaded, "QTLockContainer", &[handle]) as i32,
        -2107
    );
    assert!(!loaded
        .handles()
        .iter()
        .any(|record| record.handle == handle));
}

fn atom(kind: &[u8; 4], id: u32, children: u16, data: &[u8]) -> Vec<u8> {
    let mut bytes = Vec::new();
    bytes.extend_from_slice(&(20u32 + data.len() as u32).to_be_bytes());
    bytes.extend_from_slice(kind);
    bytes.extend_from_slice(&id.to_be_bytes());
    bytes.extend_from_slice(&[0; 2]);
    bytes.extend_from_slice(&children.to_be_bytes());
    bytes.extend_from_slice(&[0; 4]);
    bytes.extend_from_slice(data);
    bytes
}

fn imported_container(loaded: &mut PpcLoadedApp, out: u32, bytes: &[u8]) -> u32 {
    // Model a container read from a file into an ordinary movable Handle.
    loaded.cpu.gpr[3] = bytes.len() as u32;
    run_test_import(loaded, PpcImportDispatcherTarget::NewHandle { clear: true });
    let handle = loaded.cpu.gpr[3];
    let ptr = loaded.memory.read_u32_be(handle).unwrap();
    for (offset, byte) in bytes.iter().enumerate() {
        loaded.memory.write_u8(ptr + offset as u32, *byte).unwrap();
    }
    loaded.memory.write_u32_be(out, handle).unwrap();
    handle
}

#[test]
fn quicktime_atoms_walk_file_loaded_tree_by_type_id_and_index_and_return_live_data() {
    let (mut loaded, out) = fixture();
    let mut children = atom(b"abcd", 1000, 0, b"one"); // offset32; 23 bytes
    children.extend(atom(b"wxyz", 8, 0, b"two!")); // offset55; 24 bytes
    children.extend(atom(b"abcd", 900, 1, &atom(b"abcd", 100, 0, b"nested"))); // offset79
    let mut bytes = vec![0; 12];
    bytes.extend(atom(b"sean", 1, 3, &children));
    let handle = imported_container(&mut loaded, out, &bytes);
    let kind = u32::from_be_bytes(*b"abcd");
    assert_eq!(
        call(&mut loaded, "QTCountChildrenOfType", &[handle, 0, kind]),
        2
    );
    assert_eq!(
        call(&mut loaded, "QTCountChildrenOfType", &[handle, 0, 0]),
        3
    );
    assert_eq!(
        call(&mut loaded, "QTGetNextChildType", &[handle, 0, 0]),
        kind
    );
    assert_eq!(
        call(&mut loaded, "QTGetNextChildType", &[handle, 0, kind]),
        u32::from_be_bytes(*b"wxyz")
    );
    assert_eq!(
        call(
            &mut loaded,
            "QTFindChildByID",
            &[handle, 0, kind, 900, out + 8]
        ),
        79
    );
    assert_eq!(loaded.memory.read_u16_be(out + 8), Some(2));
    assert_eq!(loaded.memory.read_u16_be(out + 10), Some(0xa5a5)); // short output only
    assert_eq!(
        call(
            &mut loaded,
            "QTFindChildByIndex",
            &[handle, 0, kind, 1, out + 12]
        ),
        32
    );
    assert_eq!(loaded.memory.read_u32_be(out + 12), Some(1000));
    assert_eq!(
        call(&mut loaded, "QTFindChildByIndex", &[handle, 0, kind, 0, 0]),
        0
    );
    assert_eq!(
        call(&mut loaded, "QTFindChildByID", &[handle, 0, kind, 100, 0]),
        0
    ); // no descendant search
    assert_eq!(
        call(&mut loaded, "QTFindChildByID", &[handle, 79, kind, 100, 0]),
        99
    );
    assert_eq!(
        call(
            &mut loaded,
            "QTGetAtomTypeAndID",
            &[handle, 99, out + 16, out + 20]
        ),
        0
    );
    assert_eq!(loaded.memory.read_u32_be(out + 16), Some(kind));
    assert_eq!(loaded.memory.read_u32_be(out + 20), Some(100));
    assert_eq!(
        call(
            &mut loaded,
            "QTGetAtomDataPtr",
            &[handle, 79, out + 24, out + 28]
        ) as i32,
        -2102
    );
    assert_eq!(call(&mut loaded, "QTLockContainer", &[handle]), 0);
    assert_eq!(
        call(
            &mut loaded,
            "QTGetAtomDataPtr",
            &[handle, 99, out + 24, out + 28]
        ),
        0
    );
    assert_eq!(loaded.memory.read_u32_be(out + 24), Some(6));
    let data = loaded.memory.read_u32_be(out + 28).unwrap();
    assert_eq!(
        loaded.memory.read_u32_be(data),
        Some(u32::from_be_bytes(*b"nest"))
    );
    loaded.memory.write_u8(data, b'N').unwrap();
    let ptr = loaded.memory.read_u32_be(handle).unwrap();
    assert_eq!(loaded.memory.read_u8(ptr + 119), Some(b'N'));
    assert_eq!(
        call(&mut loaded, "QTGetAtomDataPtr", &[handle, 100, 0, out + 28]) as i32,
        -2106
    );
}

#[test]
fn quicktime_atoms_reject_truncated_or_inconsistent_trees_without_writing_outputs() {
    let (mut loaded, out) = fixture();
    for bytes in [
        vec![0; 12],
        {
            let mut bytes = vec![0; 12];
            bytes.extend(atom(b"sean", 1, 0, b"not children"));
            bytes
        },
        {
            let mut bytes = vec![0; 12];
            bytes.extend(atom(b"sean", 1, 1, &[0; 19]));
            bytes
        },
        {
            let mut bytes = vec![0; 12];
            bytes.extend(atom(b"sean", 1, 1, &atom(b"leaf", 1, 0, b"x")));
            bytes[32..36].copy_from_slice(&u32::MAX.to_be_bytes());
            bytes
        },
    ] {
        let handle = imported_container(&mut loaded, out, &bytes);
        assert_eq!(
            call(
                &mut loaded,
                "QTGetAtomDataPtr",
                &[handle, 32, out + 16, out + 20]
            ) as i32,
            -2107
        );
        assert_eq!(loaded.memory.read_u32_be(out + 16), Some(0xa5a5a5a5));
        assert_eq!(loaded.memory.read_u32_be(out + 20), Some(0xa5a5a5a5));
        assert_eq!(
            call(
                &mut loaded,
                "QTFindChildByIndex",
                &[handle, 0, u32::from_be_bytes(*b"leaf"), 1, 0]
            ),
            0
        );
    }
}

#[test]
fn quicktime_atoms_edit_nested_trees_and_reacquire_offsets_after_sibling_changes() {
    let (mut loaded, out) = fixture();
    assert_eq!(call(&mut loaded, "QTNewAtomContainer", &[out]), 0);
    let handle = loaded.memory.read_u32_be(out).unwrap();
    let kind = u32::from_be_bytes(*b"leaf");
    loaded
        .memory
        .write_u32_be(out + 40, u32::from_be_bytes(*b"abcd"))
        .unwrap();
    assert_eq!(
        call(
            &mut loaded,
            "QTInsertChild",
            &[handle, 0, kind, 100, 0, 4, out + 40, out + 4]
        ),
        0
    );
    let old_offset = loaded.memory.read_u32_be(out + 4).unwrap();
    assert_eq!(
        call(
            &mut loaded,
            "QTInsertChild",
            &[handle, 0, kind, 0, 1, 3, out + 40, out + 4]
        ),
        0
    );
    let first = loaded.memory.read_u32_be(out + 4).unwrap();
    assert_eq!(first, old_offset);
    let moved = call(&mut loaded, "QTFindChildByID", &[handle, 0, kind, 100, 0]);
    assert_eq!(moved, old_offset + 23);
    assert_eq!(
        call(
            &mut loaded,
            "QTGetAtomTypeAndID",
            &[handle, first, 0, out + 8]
        ),
        0
    );
    let assigned_id = loaded.memory.read_u32_be(out + 8).unwrap();
    assert_ne!(assigned_id, 0);
    assert_ne!(assigned_id, 100);
    // Shrinking the first leaf relocates its sibling but preserves sibling data.
    assert_eq!(
        call(&mut loaded, "QTSetAtomData", &[handle, first, 1, out + 40]),
        0
    );
    let moved = call(&mut loaded, "QTFindChildByID", &[handle, 0, kind, 100, 0]);
    assert_eq!(moved, old_offset + 21);
    assert_eq!(
        call(
            &mut loaded,
            "QTGetAtomDataPtr",
            &[handle, moved, out + 12, out + 16]
        ),
        0
    );
    let ptr = loaded.memory.read_u32_be(out + 16).unwrap();
    assert_eq!(
        loaded.memory.read_u32_be(ptr),
        Some(u32::from_be_bytes(*b"abcd"))
    );
    // An empty leaf can become a parent. Nested changes update every ancestor.
    let parent_kind = u32::from_be_bytes(*b"node");
    assert_eq!(
        call(
            &mut loaded,
            "QTInsertChild",
            &[handle, 0, parent_kind, 1, 0, 0, 0, out + 4]
        ),
        0
    );
    let parent = loaded.memory.read_u32_be(out + 4).unwrap();
    assert_eq!(
        call(
            &mut loaded,
            "QTInsertChild",
            &[handle, parent, kind, 9, 0, 4, out + 40, out + 4]
        ),
        0
    );
    let child = loaded.memory.read_u32_be(out + 4).unwrap();
    assert_eq!(
        call(&mut loaded, "QTSetAtomData", &[handle, child, 3, out + 40]),
        0
    );
    assert_eq!(
        call(
            &mut loaded,
            "QTCountChildrenOfType",
            &[handle, parent, kind]
        ),
        1
    );
    assert_eq!(call(&mut loaded, "QTRemoveAtom", &[handle, child]), 0);
    assert_eq!(
        call(&mut loaded, "QTCountChildrenOfType", &[handle, parent, 0]),
        0
    );
    assert_eq!(call(&mut loaded, "QTRemoveAtom", &[handle, first]), 0);
    assert_eq!(
        call(&mut loaded, "QTCountChildrenOfType", &[handle, 0, kind]),
        1
    );
    assert_eq!(call(&mut loaded, "QTRemoveChildren", &[handle, 0]), 0);
    let ptr = loaded.memory.read_u32_be(handle).unwrap();
    assert_eq!(loaded.memory.read_u32_be(ptr + 12), Some(20));
    assert_eq!(
        call(&mut loaded, "QTCountChildrenOfType", &[handle, 0, 0]),
        0
    );
}

#[test]
fn quicktime_atoms_rejected_edits_preserve_the_container_and_output() {
    let (mut loaded, out) = fixture();
    call(&mut loaded, "QTNewAtomContainer", &[out]);
    let handle = loaded.memory.read_u32_be(out).unwrap();
    let kind = u32::from_be_bytes(*b"leaf");
    assert_eq!(
        call(
            &mut loaded,
            "QTInsertChild",
            &[handle, 0, kind, 1, 0, 0, 0, out + 4]
        ),
        0
    );
    let leaf = loaded.memory.read_u32_be(out + 4).unwrap();
    let ptr = loaded.memory.read_u32_be(handle).unwrap();
    let before = ppc_memory_read_bytes(&mut loaded.memory, ptr, 52).unwrap();
    for (args, error) in [
        ([handle, 0, kind, 1, 0, 0, 0, out + 8], -2105),
        ([handle, 0, kind, 2, 4, 0, 0, out + 8], -2104),
        ([handle, 0, 0, 2, 0, 0, 0, out + 8], -2108),
        ([handle, 0, kind, 2, 0, u32::MAX, out + 40, out + 8], -50),
        ([handle, 0, kind, 2, 0, 4, 0xffff_fffe, out + 8], -50),
    ] {
        assert_eq!(call(&mut loaded, "QTInsertChild", &args) as i32, error);
        assert_eq!(
            ppc_memory_read_bytes(&mut loaded.memory, ptr, 52).unwrap(),
            before
        );
        assert_eq!(loaded.memory.read_u32_be(out + 8), Some(0xa5a5a5a5));
    }
    assert_eq!(call(&mut loaded, "QTLockContainer", &[handle]), 0);
    for (name, args) in [
        ("QTSetAtomData", vec![handle, leaf, 1, out + 40]),
        ("QTRemoveAtom", vec![handle, leaf]),
        ("QTRemoveChildren", vec![handle, 0]),
    ] {
        assert_eq!(call(&mut loaded, name, &args) as i32, -117);
    }
    assert_eq!(call(&mut loaded, "QTUnlockContainer", &[handle]), 0);
    assert_eq!(
        ppc_memory_read_bytes(&mut loaded.memory, ptr, 52).unwrap(),
        before
    );
}

#[test]
fn quicktime_atoms_copy_leaf_to_resized_handle_and_change_ids_without_moving_atoms() {
    let (mut loaded, out) = fixture();
    assert_eq!(call(&mut loaded, "QTNewAtomContainer", &[out]), 0);
    let container = loaded.memory.read_u32_be(out).unwrap();
    let kind = u32::from_be_bytes(*b"test");
    loaded.memory.write_u32_be(out + 32, 0x12345678).unwrap();
    assert_eq!(
        call(
            &mut loaded,
            "QTInsertChild",
            &[container, 0, kind, 10, 0, 4, out + 32, out + 4]
        ),
        0
    );
    let first = loaded.memory.read_u32_be(out + 4).unwrap();
    assert_eq!(
        call(
            &mut loaded,
            "QTInsertChild",
            &[container, 0, kind, 20, 0, 0, 0, out + 8]
        ),
        0
    );
    let second = loaded.memory.read_u32_be(out + 8).unwrap();
    loaded.cpu.gpr[3] = 1;
    run_test_import(
        &mut loaded,
        PpcImportDispatcherTarget::NewHandle { clear: true },
    );
    let destination = loaded.cpu.gpr[3];
    assert_eq!(
        call(
            &mut loaded,
            "QTCopyAtomDataToHandle",
            &[container, first, destination]
        ),
        0
    );
    let copied = loaded.memory.read_u32_be(destination).unwrap();
    assert_eq!(loaded.memory.read_u32_be(copied), Some(0x12345678));
    loaded.cpu.gpr[3] = destination;
    run_test_import(&mut loaded, PpcImportDispatcherTarget::GetHandleSize);
    assert_eq!(loaded.cpu.gpr[3], 4);
    assert_eq!(
        call(&mut loaded, "QTSetAtomID", &[container, first, 20]) as i32,
        -2105
    );
    assert_eq!(call(&mut loaded, "QTSetAtomID", &[container, first, 30]), 0);
    assert_eq!(
        call(&mut loaded, "QTFindChildByID", &[container, 0, kind, 30, 0]),
        first
    );
    assert_eq!(
        call(&mut loaded, "QTFindChildByID", &[container, 0, kind, 10, 0]),
        0
    );
    assert_eq!(call(&mut loaded, "QTSetAtomID", &[container, first, 30]), 0);
    assert_eq!(
        call(&mut loaded, "QTSetAtomID", &[container, 0, 30]) as i32,
        -2106
    );
    assert_eq!(
        call(
            &mut loaded,
            "QTCopyAtomDataToHandle",
            &[container, 12, destination]
        ) as i32,
        -2102
    );
    assert_eq!(
        call(
            &mut loaded,
            "QTCopyAtomDataToHandle",
            &[container, second, destination]
        ),
        0
    );
    loaded.cpu.gpr[3] = destination;
    run_test_import(&mut loaded, PpcImportDispatcherTarget::GetHandleSize);
    assert_eq!(loaded.cpu.gpr[3], 0);
}

#[test]
fn quicktime_atoms_copy_subtrees_independently_and_check_buffer_capacity() {
    let (mut loaded, out) = fixture();
    assert_eq!(call(&mut loaded, "QTNewAtomContainer", &[out]), 0);
    let container = loaded.memory.read_u32_be(out).unwrap();
    let kind = u32::from_be_bytes(*b"node");
    assert_eq!(
        call(
            &mut loaded,
            "QTInsertChild",
            &[container, 0, kind, 1, 0, 0, 0, out + 4]
        ),
        0
    );
    let parent = loaded.memory.read_u32_be(out + 4).unwrap();
    loaded.memory.write_u32_be(out + 32, 0x12345678).unwrap();
    assert_eq!(
        call(
            &mut loaded,
            "QTInsertChild",
            &[container, parent, kind, 2, 0, 4, out + 32, out + 8]
        ),
        0
    );
    let leaf = loaded.memory.read_u32_be(out + 8).unwrap();
    assert_eq!(
        call(&mut loaded, "QTCopyAtom", &[container, parent, out + 12]),
        0
    );
    let copy = loaded.memory.read_u32_be(out + 12).unwrap();
    assert_ne!(copy, container);
    assert_eq!(call(&mut loaded, "QTDisposeAtomContainer", &[container]), 0);
    let copied_parent = call(&mut loaded, "QTFindChildByID", &[copy, 0, kind, 1, 0]);
    let copied_leaf = call(
        &mut loaded,
        "QTFindChildByID",
        &[copy, copied_parent, kind, 2, 0],
    );
    assert_eq!(copied_leaf, leaf);
    for (allow_less, capacity) in [(0, 3), (1, 3), (0, 8)] {
        assert_eq!(
            call(
                &mut loaded,
                "QTCopyAtomDataToPtr",
                &[copy, copied_leaf, allow_less, capacity, out + 64, out + 80]
            ) as i32,
            -50
        );
        assert_eq!(loaded.memory.read_u32_be(out + 64), Some(0xa5a5a5a5));
        assert_eq!(loaded.memory.read_u32_be(out + 80), Some(0xa5a5a5a5));
    }
    assert_eq!(
        call(
            &mut loaded,
            "QTCopyAtomDataToPtr",
            &[copy, copied_leaf, 1, 8, out + 64, out + 80]
        ),
        0
    );
    assert_eq!(loaded.memory.read_u32_be(out + 64), Some(0x12345678));
    assert_eq!(loaded.memory.read_u32_be(out + 68), Some(0xa5a5a5a5));
    assert_eq!(loaded.memory.read_u32_be(out + 80), Some(4));
    assert_eq!(
        call(
            &mut loaded,
            "QTCopyAtomDataToPtr",
            &[copy, copied_leaf, 0, 4, out + 64, 0]
        ),
        0
    );
    loaded.memory.write_u32_be(out + 64, 0xa5a5a5a5).unwrap();
    assert_eq!(
        call(
            &mut loaded,
            "QTCopyAtomDataToPtr",
            &[copy, copied_leaf, 0, 4, out + 64, 0xffffffff]
        ) as i32,
        -50
    );
    assert_eq!(loaded.memory.read_u32_be(out + 64), Some(0xa5a5a5a5));
}

#[test]
fn quicktime_atoms_read_borrowed_handles_and_reject_unreadable_root_extent() {
    let (mut loaded, out) = fixture();
    let kind = u32::from_be_bytes(*b"data");
    let payload = atom(b"data", 1, 0, &[9, 8, 7]);
    let mut bytes = vec![0; 12];
    bytes.extend(atom(b"sean", 1, 1, &payload));
    let data = PPC_DATA_BASE + 0x8000;
    loaded.memory.add_region(data, bytes);
    loaded.memory.write_u32_be(out, data).unwrap();
    let child = call(&mut loaded, "QTFindChildByID", &[out, 0, kind, 1, 0]);
    assert_eq!(child, 32);
    assert_eq!(
        call(
            &mut loaded,
            "QTCopyAtomDataToPtr",
            &[out, child, 0, 3, out + 32, out + 40]
        ),
        0
    );
    assert_eq!(loaded.memory.read_u32_be(out + 32), Some(0x090807a5));
    assert_eq!(loaded.memory.read_u32_be(out + 40), Some(3));
    assert_eq!(
        call(&mut loaded, "QTGetAtomDataPtr", &[out, child, 0, out + 44]),
        0
    );
    assert_eq!(loaded.memory.read_u32_be(out + 44), Some(data + 52));
    loaded.memory.write_u32_be(data + 12, 0xfffffff8).unwrap();
    assert_eq!(
        call(
            &mut loaded,
            "QTGetAtomDataPtr",
            &[out, child, out + 48, out + 52]
        ) as i32,
        -2107
    );
    assert_eq!(loaded.memory.read_u32_be(out + 48), Some(0xa5a5a5a5));
    loaded.memory.write_u32_be(data + 12, 0x1000).unwrap();
    assert_eq!(
        call(
            &mut loaded,
            "QTGetAtomDataPtr",
            &[out, child, out + 48, out + 52]
        ) as i32,
        -2107
    );
    assert_eq!(loaded.memory.read_u32_be(out + 52), Some(0xa5a5a5a5));
}

#[test]
fn quicktime_atoms_iterate_immediate_children_across_types_and_signal_end() {
    let (mut loaded, out) = fixture();
    assert_eq!(call(&mut loaded, "QTNewAtomContainer", &[out]), 0);
    let handle = loaded.memory.read_u32_be(out).unwrap();
    let kind = u32::from_be_bytes(*b"tree");
    assert_eq!(
        call(&mut loaded, "QTNextChildAnyType", &[handle, 0, 0, out + 4]),
        0
    );
    assert_eq!(loaded.memory.read_u32_be(out + 4), Some(0));
    assert_eq!(
        call(
            &mut loaded,
            "QTInsertChild",
            &[handle, 0, kind, 1, 0, 0, 0, out + 4]
        ),
        0
    );
    let parent = loaded.memory.read_u32_be(out + 4).unwrap();
    assert_eq!(
        call(
            &mut loaded,
            "QTInsertChild",
            &[handle, parent, kind, 2, 0, 0, 0, out + 8]
        ),
        0
    );
    let nested = loaded.memory.read_u32_be(out + 8).unwrap();
    assert_eq!(
        call(
            &mut loaded,
            "QTInsertChild",
            &[handle, 0, kind + 1, 1, 0, 0, 0, out + 12]
        ),
        0
    );
    let sibling = loaded.memory.read_u32_be(out + 12).unwrap();
    for (current, expected) in [(0, parent), (parent, sibling), (sibling, 0)] {
        assert_eq!(
            call(
                &mut loaded,
                "QTNextChildAnyType",
                &[handle, 0, current, out + 16]
            ),
            0
        );
        assert_eq!(loaded.memory.read_u32_be(out + 16), Some(expected));
    }
    assert_eq!(
        call(
            &mut loaded,
            "QTNextChildAnyType",
            &[handle, parent, 0, out + 16]
        ),
        0
    );
    assert_eq!(loaded.memory.read_u32_be(out + 16), Some(nested));
    assert_eq!(
        call(
            &mut loaded,
            "QTNextChildAnyType",
            &[handle, 0, nested, out + 16]
        ) as i32,
        -2106
    );
    assert_eq!(loaded.memory.read_u32_be(out + 16), Some(nested));
    assert_eq!(
        call(&mut loaded, "QTNextChildAnyType", &[handle, 0, 0, 0]) as i32,
        -50
    );
}

#[test]
fn quicktime_atoms_bulk_edits_preserve_ids_order_and_independent_sources() {
    let (mut loaded, out) = fixture();
    let kind = u32::from_be_bytes(*b"node");
    let other_kind = u32::from_be_bytes(*b"data");
    loaded.memory.write_u32_be(out + 96, 0x11223344).unwrap();
    assert_eq!(call(&mut loaded, "QTNewAtomContainer", &[out]), 0);
    let target = loaded.memory.read_u32_be(out).unwrap();
    for (id, size) in [(1, 1), (2, 0)] {
        assert_eq!(
            call(
                &mut loaded,
                "QTInsertChild",
                &[target, 0, kind, id, 0, size, out + 96, out + 4]
            ),
            0
        );
    }
    let parent = loaded.memory.read_u32_be(out + 4).unwrap();
    assert_eq!(
        call(
            &mut loaded,
            "QTInsertChild",
            &[target, parent, other_kind, 8, 0, 3, out + 96, out + 8]
        ),
        0
    );
    let original_child = loaded.memory.read_u32_be(out + 8).unwrap();
    assert_eq!(
        call(&mut loaded, "QTGetAtomParent", &[target, original_child]),
        parent
    );
    assert_eq!(call(&mut loaded, "QTGetAtomParent", &[target, parent]), 0);
    assert_eq!(
        call(&mut loaded, "QTGetAtomParent", &[target, 0xffffffff]),
        0
    );

    // Duplicate the whole locked tree; its independent copy starts unlocked.
    assert_eq!(call(&mut loaded, "QTLockContainer", &[target]), 0);
    assert_eq!(call(&mut loaded, "QTCopyAtom", &[target, 0, out + 12]), 0);
    let source = loaded.memory.read_u32_be(out + 12).unwrap();
    let source_ptr = loaded.memory.read_u32_be(source).unwrap();
    assert_eq!(loaded.memory.read_u16_be(source_ptr + 10), Some(0));
    assert_eq!(call(&mut loaded, "QTUnlockContainer", &[target]), 0);
    assert_eq!(
        call(&mut loaded, "QTInsertChildren", &[target, parent, source]),
        0
    );
    let parent = call(&mut loaded, "QTFindChildByID", &[target, 0, kind, 2, 0]);
    assert_eq!(
        call(&mut loaded, "QTCountChildrenOfType", &[target, parent, 0]),
        3
    );
    assert_eq!(
        call(&mut loaded, "QTInsertChildren", &[target, parent, source]) as i32,
        -2105
    );
    assert_eq!(
        call(&mut loaded, "QTCountChildrenOfType", &[target, parent, 0]),
        3
    );

    let source_parent = call(&mut loaded, "QTFindChildByID", &[source, 0, kind, 2, 0]);
    let first = call(&mut loaded, "QTFindChildByID", &[target, 0, kind, 1, 0]);
    assert_eq!(
        call(
            &mut loaded,
            "QTReplaceAtom",
            &[target, first, source, source_parent]
        ),
        0
    );
    let first = call(
        &mut loaded,
        "QTFindChildByID",
        &[target, 0, kind, 1, out + 16],
    );
    assert_eq!(loaded.memory.read_u16_be(out + 16), Some(1));
    let replaced_child = call(
        &mut loaded,
        "QTFindChildByID",
        &[target, first, other_kind, 8, 0],
    );
    assert_ne!(replaced_child, 0);
    assert_eq!(
        call(&mut loaded, "QTGetAtomParent", &[target, replaced_child]),
        first
    );
    let second = call(&mut loaded, "QTFindChildByID", &[target, 0, kind, 2, 0]);
    assert_eq!(
        call(&mut loaded, "QTSwapAtoms", &[target, first, second]),
        0
    );
    let first = call(
        &mut loaded,
        "QTFindChildByID",
        &[target, 0, kind, 1, out + 16],
    );
    assert_eq!(loaded.memory.read_u16_be(out + 16), Some(1));
    let second = call(
        &mut loaded,
        "QTFindChildByID",
        &[target, 0, kind, 2, out + 16],
    );
    assert_eq!(loaded.memory.read_u16_be(out + 16), Some(2));
    assert_eq!(
        call(&mut loaded, "QTCountChildrenOfType", &[target, first, 0]),
        3
    );
    assert_eq!(
        call(&mut loaded, "QTCountChildrenOfType", &[target, second, 0]),
        1
    );

    // Replacing from the same container snapshots the source before resizing.
    assert_eq!(
        call(
            &mut loaded,
            "QTReplaceAtom",
            &[target, first, target, second]
        ),
        0
    );
    assert_eq!(
        call(&mut loaded, "QTCountChildrenOfType", &[target, first, 0]),
        1
    );
    let source_first = call(&mut loaded, "QTFindChildByID", &[source, 0, kind, 1, 0]);
    assert_eq!(
        call(
            &mut loaded,
            "QTCopyAtomDataToPtr",
            &[source, source_first, 0, 1, out + 64, 0]
        ),
        0
    );
    assert_eq!(loaded.memory.read_u8(out + 64), Some(0x11));
    assert_eq!(
        call(
            &mut loaded,
            "QTCountChildrenOfType",
            &[source, source_parent, 0]
        ),
        1
    );
    let child = call(
        &mut loaded,
        "QTFindChildByID",
        &[target, first, other_kind, 8, 0],
    );
    assert_eq!(
        call(&mut loaded, "QTSwapAtoms", &[target, first, child]) as i32,
        -2103
    );
    assert_eq!(
        call(
            &mut loaded,
            "QTReplaceAtom",
            &[target, first, target, child]
        ) as i32,
        -2103
    );
    assert_eq!(
        call(&mut loaded, "QTCountChildrenOfType", &[target, first, 0]),
        1
    );
}

#[test]
fn quicktime_atoms_swap_unequal_leaves_between_parents_and_keep_tree_sizes_valid() {
    let (mut loaded, out) = fixture();
    let parent_kind = u32::from_be_bytes(*b"prnt");
    let leaf_kind = u32::from_be_bytes(*b"leaf");
    assert_eq!(call(&mut loaded, "QTNewAtomContainer", &[out]), 0);
    let handle = loaded.memory.read_u32_be(out).unwrap();
    loaded.memory.write_u32_be(out + 96, 0xaabbccdd).unwrap();
    for id in [1, 2] {
        assert_eq!(
            call(
                &mut loaded,
                "QTInsertChild",
                &[handle, 0, parent_kind, id, 0, 0, 0, out + 4]
            ),
            0
        );
        let parent = loaded.memory.read_u32_be(out + 4).unwrap();
        assert_eq!(
            call(
                &mut loaded,
                "QTInsertChild",
                &[
                    handle,
                    parent,
                    leaf_kind,
                    id,
                    0,
                    if id == 1 { 1 } else { 4 },
                    out + 96,
                    out + 8
                ]
            ),
            0
        );
    }
    let p1 = call(
        &mut loaded,
        "QTFindChildByID",
        &[handle, 0, parent_kind, 1, 0],
    );
    let p2 = call(
        &mut loaded,
        "QTFindChildByID",
        &[handle, 0, parent_kind, 2, 0],
    );
    let a1 = call(
        &mut loaded,
        "QTFindChildByID",
        &[handle, p1, leaf_kind, 1, 0],
    );
    let a2 = call(
        &mut loaded,
        "QTFindChildByID",
        &[handle, p2, leaf_kind, 2, 0],
    );
    assert_eq!(call(&mut loaded, "QTLockContainer", &[handle]), 0);
    assert_eq!(
        call(&mut loaded, "QTSwapAtoms", &[handle, a1, a2]) as i32,
        -117
    );
    assert_eq!(call(&mut loaded, "QTUnlockContainer", &[handle]), 0);
    assert_eq!(call(&mut loaded, "QTSwapAtoms", &[handle, a2, a1]), 0);
    let p2 = call(
        &mut loaded,
        "QTFindChildByID",
        &[handle, 0, parent_kind, 2, 0],
    );
    let a2 = call(
        &mut loaded,
        "QTFindChildByID",
        &[handle, p2, leaf_kind, 2, 0],
    );
    assert_eq!(
        call(
            &mut loaded,
            "QTCopyAtomDataToPtr",
            &[handle, a1, 0, 4, out + 64, 0]
        ),
        0
    );
    assert_eq!(loaded.memory.read_u32_be(out + 64), Some(0xaabbccdd));
    assert_eq!(
        call(
            &mut loaded,
            "QTCopyAtomDataToPtr",
            &[handle, a2, 0, 1, out + 68, 0]
        ),
        0
    );
    assert_eq!(loaded.memory.read_u8(out + 68), Some(0xaa));
    let ptr = loaded.memory.read_u32_be(handle).unwrap();
    assert_eq!(loaded.memory.read_u32_be(ptr + p1), Some(44));
    assert_eq!(loaded.memory.read_u32_be(ptr + p2), Some(41));
    assert_eq!(loaded.memory.read_u32_be(ptr + 12), Some(105));
}
