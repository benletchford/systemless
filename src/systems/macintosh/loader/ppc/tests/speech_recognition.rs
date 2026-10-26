use super::*;

#[test]
fn speech_recognition_open_reports_unavailable_without_creating_objects() {
    for library in ["SpeechRecognitionLib", "CarbonLib"] {
        let mut loaded = load_pef_application(&synthetic_pef_with_library_import(
            library.as_bytes(),
            b"SROpenRecognitionSystem",
        ))
        .unwrap();
        let output = PPC_DATA_BASE + 0x1000;
        loaded.memory.add_region(output, vec![0xa5; 12]);
        let before = loaded.handles();
        loaded.cpu.gpr[3] = output + 4;
        loaded.cpu.gpr[4] = 0;
        run_test_import(
            &mut loaded,
            dispatcher_target_for_import(library, "SROpenRecognitionSystem"),
        );
        assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(-5100));
        assert_eq!(
            ppc_memory_read_bytes(&mut loaded.memory, output, 12),
            Some([vec![0xa5; 4], vec![0; 4], vec![0xa5; 4]].concat())
        );
        assert_eq!(loaded.handles(), before);
        for pointer in [0, u32::MAX - 1, output + 10] {
            loaded.cpu.gpr[3] = pointer;
            run_test_import(
                &mut loaded,
                dispatcher_target_for_import(library, "SROpenRecognitionSystem"),
            );
            assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(-5105));
            assert_eq!(loaded.memory.read_u16_be(output + 10), Some(0xa5a5));
        }
    }
}

#[test]
fn speech_recognition_invalid_objects_clear_references_without_writing_property_buffers() {
    let mut loaded = load_pef_application(&synthetic_pef_with_library_import(
        b"SpeechRecognitionLib",
        b"SRNewRecognizer",
    ))
    .unwrap();
    let output = PPC_DATA_BASE + 0x1000;
    loaded.memory.add_region(output, vec![0xa5; 32]);
    let before = loaded.handles();
    for symbol in [
        "SRNewRecognizer",
        "SRNewLanguageModel",
        "SRGetIndexedItem",
        "SRGetSpeechSource",
    ] {
        loaded.memory.write_u32_be(output + 4, u32::MAX).unwrap();
        loaded.cpu.gpr[3] = 0x12345678;
        loaded.cpu.gpr[4] = output + 4;
        run_test_import(
            &mut loaded,
            dispatcher_target_for_import("SpeechRecognitionLib", symbol),
        );
        assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(-5104));
        assert_eq!(loaded.memory.read_u32_be(output + 4), Some(0));
    }
    loaded.cpu.gpr[3] = 0x12345678;
    loaded.cpu.gpr[4] = u32::from_be_bytes(*b"vers");
    loaded.cpu.gpr[5] = output + 16;
    loaded.cpu.gpr[6] = output + 4;
    loaded.memory.write_u32_be(output + 4, 8).unwrap();
    run_test_import(
        &mut loaded,
        dispatcher_target_for_import("SpeechRecognitionLib", "SRGetProperty"),
    );
    assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(-5104));
    assert_eq!(loaded.memory.read_u32_be(output + 4), Some(0));
    assert_eq!(
        ppc_memory_read_bytes(&mut loaded.memory, output + 16, 8),
        Some(vec![0xa5; 8])
    );
    for symbol in [
        "SRCloseRecognitionSystem",
        "SRContinueRecognition",
        "SREmptyLanguageObject",
        "SRReleaseObject",
        "SRSetLanguageModel",
        "SRSetProperty",
        "SRStartListening",
        "SRStopListening",
        "SRAddText",
    ] {
        loaded.cpu.gpr[3] = 0x12345678;
        run_test_import(
            &mut loaded,
            dispatcher_target_for_import("SpeechRecognitionLib", symbol),
        );
        assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(-5104), "{symbol}");
    }
    run_test_import(
        &mut loaded,
        dispatcher_target_for_import("SpeechRecognitionLib", "SRIdle"),
    );
    assert_eq!(loaded.cpu.gpr[3], 0);
    assert_eq!(loaded.handles(), before);
}
