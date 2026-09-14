use super::*;

#[test]
fn appletalk_compatibility_imports_pre_resolve_to_typed_operations() {
    for (symbol, operation) in [
        (
            "GetBridgeAddress",
            PpcAppleTalkCompatibilityOperation::GetBridgeAddress,
        ),
        (
            "GetNodeAddress",
            PpcAppleTalkCompatibilityOperation::GetNodeAddress,
        ),
        ("GetZoneList", PpcAppleTalkCompatibilityOperation::GetZoneList),
        ("MPPOpen", PpcAppleTalkCompatibilityOperation::MppOpen),
        ("NBPExtract", PpcAppleTalkCompatibilityOperation::NbpExtract),
        ("NBPSetEntity", PpcAppleTalkCompatibilityOperation::NbpSetEntity),
        ("NBPSetNTE", PpcAppleTalkCompatibilityOperation::NbpSetNte),
        ("PCloseSkt", PpcAppleTalkCompatibilityOperation::PCloseSkt),
        ("PKillNBP", PpcAppleTalkCompatibilityOperation::PKillNbp),
        ("PLookupName", PpcAppleTalkCompatibilityOperation::PLookupName),
        ("POpenSkt", PpcAppleTalkCompatibilityOperation::POpenSkt),
        (
            "PRegisterName",
            PpcAppleTalkCompatibilityOperation::PRegisterName,
        ),
        ("PRemoveName", PpcAppleTalkCompatibilityOperation::PRemoveName),
        ("PSetSelfSend", PpcAppleTalkCompatibilityOperation::PSetSelfSend),
        ("PWriteDDP", PpcAppleTalkCompatibilityOperation::PWriteDdp),
        ("StandardNBP", PpcAppleTalkCompatibilityOperation::StandardNbp),
    ] {
        assert_eq!(
            dispatcher_target_for_import("InterfaceLib", symbol),
            PpcImportDispatcherTarget::AppleTalkCompatibility(operation),
            "unexpected AppleTalk dispatch target for {symbol}",
        );
    }
}

#[test]
fn native_ppc_munger_replaces_live_handle_contents() {
    let mut memory = PpcSectionMem::new();
    memory.add_region(0x1000, vec![0; 4]);
    memory.add_region(0x1100, b"one two three".to_vec());
    memory.add_region(0x2000, b"twoTWO".to_vec());
    memory.write_u32_be(0x1000, 0x1100).unwrap();
    let mut handles = vec![PpcHandleRecord {
        handle: 0x1000,
        ptr: 0x1100,
        size: 13,
        capacity: 13,
    }];
    let mut cpu = PpcCpu::new();
    cpu.gpr[3] = 0x1000;
    cpu.gpr[4] = 0;
    cpu.gpr[5] = 0x2000;
    cpu.gpr[6] = 3;
    cpu.gpr[7] = 0x2003;
    cpu.gpr[8] = 3;
    let mut heap_cursor = 0x3000;
    let mut last_mem_error = PPC_NO_ERR;
    assert_eq!(
        ppc_munger_compatibility(
            &cpu,
            None,
            &mut memory,
            &mut heap_cursor,
            0x5000,
            &mut last_mem_error,
            &mut handles,
        ),
        4
    );
    assert_eq!(
        ppc_handle_bytes(&mut memory, &handles, 0x1000),
        Some(b"one TWO three".to_vec())
    );
    assert_eq!(last_mem_error, PPC_NO_ERR);
}

#[test]
fn native_ppc_missing_classic_hardware_reports_canonical_errors() {
    let mut memory = PpcSectionMem::new();
    memory.add_region(0x1000, vec![0xaa; 8]);
    let mut cpu = PpcCpu::new();
    cpu.gpr[3] = 0x1000;
    cpu.gpr[4] = 0x1004;
    assert_eq!(
        ppc_dispatch_appletalk_compatibility(
            PpcAppleTalkCompatibilityOperation::GetNodeAddress,
            &mut cpu,
            &mut memory,
        ),
        PpcImportAction::Return(ppc_i16_result(PPC_NO_MPP_ERR))
    );
    assert_eq!(memory.read_u8(0x1000), Some(0));
    assert_eq!(memory.read_u16_be(0x1004), Some(0));
}

#[test]
fn native_ppc_nbp_entity_packs_three_pascal_components() {
    let mut memory = PpcSectionMem::new();
    memory.add_region(0x1000, vec![0; 99]);
    memory.add_region(0x1100, [3, b'O', b'b', b'j'].to_vec());
    memory.add_region(0x1200, [4, b'T', b'y', b'p', b'e'].to_vec());
    memory.add_region(0x1300, [5, b'Z', b'o', b'n', b'e', b'!'].to_vec());
    let mut cpu = PpcCpu::new();
    cpu.gpr[3] = 0x1000;
    cpu.gpr[4] = 0x1100;
    cpu.gpr[5] = 0x1200;
    cpu.gpr[6] = 0x1300;

    assert_eq!(
        ppc_dispatch_appletalk_compatibility(
            PpcAppleTalkCompatibilityOperation::NbpSetEntity,
            &mut cpu,
            &mut memory,
        ),
        PpcImportAction::ReturnPreserve,
    );
    assert_eq!(
        ppc_memory_read_bytes(&mut memory, 0x1000, 4),
        Some(vec![3, b'O', b'b', b'j']),
    );
    assert_eq!(
        ppc_memory_read_bytes(&mut memory, 0x1000 + 33, 5),
        Some(vec![4, b'T', b'y', b'p', b'e']),
    );
    assert_eq!(
        ppc_memory_read_bytes(&mut memory, 0x1000 + 66, 6),
        Some(vec![5, b'Z', b'o', b'n', b'e', b'!']),
    );
}
