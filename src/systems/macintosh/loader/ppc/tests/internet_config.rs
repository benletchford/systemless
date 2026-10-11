use super::*;

#[test]
fn internet_config_weak_imports_bind_to_typed_lifecycle_operations() {
    let operations = [
        ("ICStart", PpcImportDispatcherTarget::ICStart),
        ("ICStop", PpcImportDispatcherTarget::ICStop),
        ("ICGetSeed", PpcImportDispatcherTarget::ICGetSeed),
    ];
    for (name, target) in operations {
        assert_eq!(dispatcher_target_for_import("CarbonLib", name), target);
        let bindings = PpcImportBindingPlan::prepare(
            vec![PefResolvedImport {
                library_index: 0,
                symbol_index: 0,
                library_name: "CarbonLib".into(),
                symbol_name: name.into(),
                class: 2,
                weak: true,
            }],
            1,
            0,
            ppc_import_layout(),
            &SystemlessPpcImportBindingPolicy,
        )
        .unwrap()
        .into_initial_bindings();
        assert_eq!(bindings[0].address, PPC_IMPORT_TVECTOR_BASE);
        assert_eq!(bindings[0].dispatcher_target, target);
    }
}

#[test]
fn internet_config_instances_share_seed_and_dispose_only_owned_storage() {
    let mut loaded = load_pef_application(&synthetic_pef_with_import(b"ICStart")).unwrap();
    let out = PPC_STACK_TOP - 64;
    let seed_out = out + 16;
    let creator = u32::from_be_bytes(*b"MSol");
    let mut instances = Vec::new();
    for output in [out, out + 4] {
        loaded.cpu.gpr[3] = output;
        loaded.cpu.gpr[4] = creator;
        run_test_import(&mut loaded, PpcImportDispatcherTarget::ICStart);
        assert_eq!(loaded.cpu.gpr[3], 0);
        let instance = loaded.memory.read_u32_be(output).unwrap();
        assert_ne!(instance, 0);
        assert!(loaded.ptrs().iter().any(|p| p.ptr == instance));
        assert_eq!(loaded.memory.read_u32_be(instance), Some(creator));
        instances.push(instance);
    }
    assert_ne!(instances[0], instances[1]);
    loaded.toolbox_startup.internet_config.preferences_seed = 0x8765_4321;
    loaded
        .memory
        .write_u32_be(seed_out - 4, 0xAABB_CCDD)
        .unwrap();
    loaded
        .memory
        .write_u32_be(seed_out + 4, 0x1122_3344)
        .unwrap();
    for instance in &instances {
        loaded.cpu.gpr[3] = *instance;
        loaded.cpu.gpr[4] = seed_out;
        run_test_import(&mut loaded, PpcImportDispatcherTarget::ICGetSeed);
        assert_eq!(loaded.cpu.gpr[3], 0);
        assert_eq!(loaded.memory.read_u32_be(seed_out), Some(0x8765_4321));
    }
    assert_eq!(loaded.memory.read_u32_be(seed_out - 4), Some(0xAABB_CCDD));
    assert_eq!(loaded.memory.read_u32_be(seed_out + 4), Some(0x1122_3344));
    loaded.cpu.gpr[3] = instances[0];
    run_test_import(&mut loaded, PpcImportDispatcherTarget::ICStop);
    assert_eq!(loaded.cpu.gpr[3], 0);
    assert!(!loaded.ptrs().iter().any(|p| p.ptr == instances[0]));
    loaded.cpu.gpr[3] = instances[0];
    loaded.cpu.gpr[4] = seed_out;
    run_test_import(&mut loaded, PpcImportDispatcherTarget::ICGetSeed);
    assert_eq!(loaded.cpu.gpr[3] as i32, i32::from(PPC_PARAM_ERR));
    assert_eq!(loaded.memory.read_u32_be(seed_out), Some(0x8765_4321));
    loaded.cpu.gpr[3] = instances[0];
    run_test_import(&mut loaded, PpcImportDispatcherTarget::ICStop);
    assert_eq!(loaded.cpu.gpr[3] as i32, i32::from(PPC_PARAM_ERR));
    assert!(loaded.ptrs().iter().any(|p| p.ptr == instances[1]));

    loaded.cpu.gpr[3] = 16;
    run_test_import(
        &mut loaded,
        PpcImportDispatcherTarget::NewPtr { clear: true },
    );
    let unrelated = loaded.cpu.gpr[3];
    assert_ne!(unrelated, 0);
    loaded.cpu.gpr[3] = unrelated;
    run_test_import(&mut loaded, PpcImportDispatcherTarget::ICStop);
    assert_eq!(loaded.cpu.gpr[3] as i32, i32::from(PPC_PARAM_ERR));
    assert!(loaded.ptrs().iter().any(|p| p.ptr == unrelated));
}

#[test]
fn internet_config_invalid_outputs_do_not_allocate_or_overwrite_memory() {
    let mut loaded = load_pef_application(&synthetic_pef_with_import(b"ICStart")).unwrap();
    let out = PPC_STACK_TOP - 64;
    loaded.cpu.gpr[3] = out;
    loaded.cpu.gpr[4] = u32::from_be_bytes(*b"TEST");
    run_test_import(&mut loaded, PpcImportDispatcherTarget::ICStart);
    let instance = loaded.memory.read_u32_be(out).unwrap();
    let cursor = loaded.heap_cursor();
    let ptr_count = loaded.ptrs().len();
    for output in [0, u32::MAX - 1] {
        loaded.cpu.gpr[3] = output;
        run_test_import(&mut loaded, PpcImportDispatcherTarget::ICStart);
        assert_eq!(loaded.cpu.gpr[3] as i32, i32::from(PPC_PARAM_ERR));
        assert_eq!(loaded.heap_cursor(), cursor);
        assert_eq!(loaded.ptrs().len(), ptr_count);
        loaded.cpu.gpr[3] = instance;
        loaded.cpu.gpr[4] = output;
        run_test_import(&mut loaded, PpcImportDispatcherTarget::ICGetSeed);
        assert_eq!(loaded.cpu.gpr[3] as i32, i32::from(PPC_PARAM_ERR));
    }
    assert_eq!(loaded.memory.read_u32_be(out), Some(instance));
}

#[test]
fn internet_config_failed_allocation_clears_output_and_leaves_no_live_instance() {
    let mut loaded = load_pef_application(&synthetic_pef_with_import(b"ICStart")).unwrap();
    let out = PPC_STACK_TOP - 64;
    loaded.cpu.gpr[3] = loaded.heap_cursor();
    run_test_import(&mut loaded, PpcImportDispatcherTarget::SetApplLimit);
    let cursor = loaded.heap_cursor();
    let count = loaded.ptrs().len();
    loaded.memory.write_u32_be(out, 0xDEAD_BEEF).unwrap();
    loaded.cpu.gpr[3] = out;
    loaded.cpu.gpr[4] = u32::from_be_bytes(*b"TEST");
    run_test_import(&mut loaded, PpcImportDispatcherTarget::ICStart);
    assert_eq!(loaded.cpu.gpr[3] as i32, i32::from(PPC_MEM_FULL_ERR));
    assert_eq!(loaded.memory.read_u32_be(out), Some(0));
    assert_eq!(loaded.heap_cursor(), cursor);
    assert_eq!(loaded.ptrs().len(), count);
}
