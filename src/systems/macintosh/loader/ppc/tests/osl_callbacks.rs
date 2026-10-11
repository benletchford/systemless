use super::*;

// Apple AEObjects.h: each callback returns OSErr and uses Pascal calling
// convention. Each parameter is a 32-bit pointer, DescType or long.
const OSL_CALLBACKS: [(&str, usize); 8] = [
    ("Accessor", 7),
    ("Compare", 4),
    ("Count", 4),
    ("DisposeToken", 1),
    ("GetMarkToken", 3),
    ("GetErrDesc", 1),
    ("Mark", 3),
    ("AdjustMarks", 3),
];

fn documented_osl_proc_info(parameters: usize) -> u32 {
    // MixedMode.h: kPascalStackBased=0, OSErr's result size is 2<<4;
    // four-byte stack parameters have size code 3, beginning at bit 6.
    (0..parameters).fold(2 << 4, |info, index| info | (3 << (6 + index * 2)))
}

#[test]
fn osl_callback_weak_imports_bind_all_constructor_and_disposal_families() {
    for (family, parameters) in OSL_CALLBACKS {
        let new = format!("NewOSL{family}UPP");
        let dispose = format!("DisposeOSL{family}UPP");
        let constructor = PpcImportDispatcherTarget::NewObjectSupportUPP {
            proc_info: documented_osl_proc_info(parameters),
        };
        for (symbol, target) in [
            (new, constructor),
            (dispose, PpcImportDispatcherTarget::DisposeRoutineDescriptor),
        ] {
            assert_eq!(dispatcher_target_for_import("CarbonLib", &symbol), target);
            let bindings = PpcImportBindingPlan::prepare(
                vec![PefResolvedImport {
                    library_index: 0,
                    symbol_index: 0,
                    library_name: "CarbonLib".into(),
                    symbol_name: symbol,
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
}

#[test]
fn osl_callback_descriptors_preserve_signatures_and_reuse_disposed_storage() {
    for (family, parameters) in OSL_CALLBACKS {
        let symbol = format!("NewOSL{family}UPP");
        let pef = synthetic_pef_with_library_import(b"CarbonLib", symbol.as_bytes());
        let mut loaded = load_pef_application(&pef).unwrap();
        let callback = PPC_CODE_BASE;
        loaded.cpu.gpr[3] = callback;
        // Execute the actual imported binding rather than overriding its target.
        let probe = loaded.run_with_hle_imports(64);
        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        let descriptor = loaded.cpu.gpr[3];
        assert_ne!(descriptor, 0);
        assert_ne!(descriptor, callback);
        assert_eq!(
            loaded.memory.read_u16_be(descriptor),
            Some(PPC_MIXED_MODE_TRAP)
        );
        let record = descriptor + PPC_ROUTINE_DESCRIPTOR_HEADER_SIZE;
        assert_eq!(
            loaded.memory.read_u32_be(record),
            Some(documented_osl_proc_info(parameters))
        );
        assert_eq!(
            loaded
                .memory
                .read_u32_be(record + PPC_ROUTINE_RECORD_PROC_DESCRIPTOR_OFFSET),
            Some(callback)
        );
        let dispose = dispatcher_target_for_import("CarbonLib", &format!("DisposeOSL{family}UPP"));
        loaded.cpu.gpr[3] = descriptor;
        run_test_import(&mut loaded, dispose);
        assert_eq!(loaded.cpu.gpr[3], descriptor, "void disposal preserves r3");
        loaded.cpu.gpr[3] = callback;
        run_test_import(
            &mut loaded,
            dispatcher_target_for_import("CarbonLib", &symbol),
        );
        assert_eq!(loaded.cpu.gpr[3], descriptor, "disposed storage is reused");
    }
}
