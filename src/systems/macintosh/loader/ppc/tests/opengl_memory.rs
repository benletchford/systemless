use super::*;

#[test]
fn glm_set_mode_accepts_documented_modes_and_rejects_unknown_mode() {
    for mode in 1..=4 {
        let pef = synthetic_pef_with_library_import(b"OpenGLMemory", b"glmSetMode");
        let mut loaded = load_pef_application(&pef).unwrap();
        assert_eq!(
            loaded.imports[0].dispatcher_target,
            PpcImportDispatcherTarget::GlmSetMode
        );
        loaded.cpu.gpr[3] = mode;

        let probe = loaded.run_with_hle_imports(64);

        assert!(matches!(probe.result, PpcRunResult::Halted { .. }));
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(loaded.glm_mode, Some(mode));
        assert_eq!(loaded.glm_error, 0);
        assert_eq!(loaded.cpu.gpr[3], mode); // void function preserves r3
    }

    let pef = synthetic_pef_with_library_import(b"OpenGLMemory", b"glmSetMode");
    let mut loaded = load_pef_application(&pef).unwrap();
    loaded.glm_mode = Some(2);
    loaded.cpu.gpr[3] = 5;
    let probe = loaded.run_with_hle_imports(64);
    assert!(matches!(probe.result, PpcRunResult::Halted { .. }));
    assert_eq!(loaded.glm_mode, Some(2));
    assert_eq!(loaded.glm_error, 1); // GLM_INVALID_ENUM
}

#[test]
fn glm_get_error_clears_the_pending_error() {
    let pef = synthetic_pef_with_library_import(b"OpenGLMemory", b"glmGetError");
    let mut loaded = load_pef_application(&pef).unwrap();
    assert_eq!(
        loaded.imports[0].dispatcher_target,
        PpcImportDispatcherTarget::GlmGetError
    );
    loaded.glm_error = 1;

    let probe = loaded.run_with_hle_imports(64);

    assert!(matches!(probe.result, PpcRunResult::Halted { .. }));
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], 1);
    assert_eq!(loaded.glm_error, 0);
}
