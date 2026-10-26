use super::*;

#[test]
fn glide_absent_hardware_queries_and_acquisition_do_not_expose_guest_memory() {
    let mut loaded = load_pef_application(&synthetic_pef_with_library_import(
        b"3DfxGlideLib2.x",
        b"grSstQueryHardware",
    ))
    .unwrap();
    let output = PPC_DATA_BASE + 0x1000;
    loaded.memory.add_region(output, vec![0xa5; 16]);
    loaded.cpu.gpr[3] = output + 4;
    run_test_import(
        &mut loaded,
        dispatcher_target_for_import("3DfxGlideLib2.x", "grSstQueryHardware"),
    );
    assert_eq!(loaded.cpu.gpr[3], 0);
    assert_eq!(
        ppc_memory_read_bytes(&mut loaded.memory, output, 16),
        Some([vec![0xa5; 4], vec![0; 4], vec![0xa5; 8]].concat())
    );
    loaded.memory.add_region(output + 0x100, vec![0x5a; 2]);
    for address in [0, output + 0x100, u32::MAX - 1] {
        loaded.cpu.gpr[3] = address;
        run_test_import(
            &mut loaded,
            dispatcher_target_for_import("3DfxGlideLib2.x", "grSstQueryHardware"),
        );
        assert_eq!(loaded.cpu.gpr[3], 0);
        assert_eq!(loaded.memory.read_u16_be(output + 0x100), Some(0x5a5a));
    }
    for symbol in [
        "grSstWinOpen",
        "grLfbLock",
        "grLfbUnlock",
        "grBufferNumPending",
        "grTexMinAddress",
        "grTexMaxAddress",
    ] {
        loaded.cpu.gpr[3] = output;
        loaded.cpu.gpr[8] = output; // grLfbLock's info pointer.
        run_test_import(
            &mut loaded,
            dispatcher_target_for_import("3DfxGlideLib2.x", symbol),
        );
        assert_eq!(loaded.cpu.gpr[3], 0, "{symbol}");
        assert_eq!(loaded.memory.read_u32_be(output), Some(0xa5a5_a5a5));
    }
    for symbol in [
        "grGlideInit",
        "grGlideShutdown",
        "grSstSelect",
        "grSstWinClose",
        "grAlphaBlendFunction",
        "grAlphaCombine",
        "grBufferSwap",
        "grColorCombine",
        "grConstantColorValue",
        "grDrawLine",
        "grDrawPolygonVertexList",
        "grDrawTriangle",
        "grFogMode",
        "grGammaCorrectionValue",
        "grTexClampMode",
        "grTexCombine",
        "grTexDownloadMipMap",
        "grTexDownloadTable",
        "grTexFilterMode",
        "grTexSource",
    ] {
        let target = dispatcher_target_for_import("3DfxGlideLib2.x", symbol);
        assert_eq!(
            target,
            PpcImportDispatcherTarget::Glide(PpcGlideOperation::NoDeviceStateChange)
        );
        loaded.cpu.gpr[3] = output;
        run_test_import(&mut loaded, target);
        assert_eq!(loaded.memory.read_u32_be(output), Some(0xa5a5_a5a5));
    }
}

#[test]
fn glide_texture_memory_queries_use_the_classic_gpr_and_grtexinfo_layouts() {
    let mut loaded = load_pef_application(&synthetic_pef_with_library_import(
        b"3DfxGlideLib2.x",
        b"grTexCalcMemRequired",
    ))
    .unwrap();
    loaded.cpu.gpr[3] = 8; // small LOD 1
    loaded.cpu.gpr[4] = 0; // large LOD 256
    loaded.cpu.gpr[5] = 3; // square
    loaded.cpu.gpr[6] = 0; // RGB332
    run_test_import(
        &mut loaded,
        dispatcher_target_for_import("3DfxGlideLib2.x", "grTexCalcMemRequired"),
    );
    assert_eq!(loaded.cpu.gpr[3], 87384);
    let info = PPC_DATA_BASE + 0x1000;
    loaded.memory.add_region(info, vec![0; 20]);
    for (offset, value) in [(0, 8), (4, 0), (8, 3), (12, 11), (16, u32::MAX)] {
        loaded.memory.write_u32_be(info + offset, value).unwrap();
    }
    loaded.cpu.gpr[3] = 1; // even levels only
    loaded.cpu.gpr[4] = info;
    run_test_import(
        &mut loaded,
        dispatcher_target_for_import("3DfxGlideLib2.x", "grTexTextureMemRequired"),
    );
    assert_eq!(loaded.cpu.gpr[3], 139816);
    assert_eq!(loaded.memory.read_u32_be(info + 16), Some(u32::MAX));
    loaded.cpu.gpr[3] = 3;
    loaded.cpu.gpr[4] = u32::MAX - 4;
    run_test_import(
        &mut loaded,
        dispatcher_target_for_import("3DfxGlideLib2.x", "grTexTextureMemRequired"),
    );
    assert_eq!(loaded.cpu.gpr[3], 0);
}
