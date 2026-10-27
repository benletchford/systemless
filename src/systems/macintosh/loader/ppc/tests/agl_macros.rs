use super::super::classic_gl_agl::PpcAglPixelFormatRequest;
use super::*;

fn create_context(loaded: &mut PpcLoadedApp, format: u32) -> u32 {
    loaded.cpu.gpr[3] = format;
    loaded.cpu.gpr[4] = 0;
    run_test_import(loaded, PpcImportDispatcherTarget::AglCreateContext);
    let context = loaded.cpu.gpr[3];
    assert_ne!(context, 0);
    context
}

fn call_macro(loaded: &mut PpcLoadedApp, context: u32, offset: u16) {
    let code = PPC_CODE_BASE + 0x1000;
    let words = [
        0x8183_0000 | u32::from(offset), // lwz r12, offset(r3): procedure TVector
        0x8063_0000,                     // lwz r3, 0(r3): GLI renderer
        0x800c_0000,                     // lwz r0, 0(r12)
        0x804c_0004,                     // lwz r2, 4(r12)
        0x7c09_03a6,                     // mtctr r0
        0x4e80_0420,                     // bctr (tail call, LR already halt)
    ];
    loaded
        .memory
        .add_region(code, words.into_iter().flat_map(u32::to_be_bytes).collect());
    loaded.cpu.pc = code;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.cpu.gpr[3] = context;
    let probe = loaded.run_with_hle_imports(64);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.pc, PPC_HALT_PC);
}

#[test]
fn agl_context_record_supports_guest_macro_calls_without_changing_current_context() {
    let mut loaded = load_pef_application(&synthetic_pef_with_library_import(
        b"OpenGLLibrary",
        b"aglCreateContext",
    ))
    .unwrap();
    let format = loaded.agl.choose_pixel_format(PpcAglPixelFormatRequest {
        rgba: true,
        ..Default::default()
    });
    let first = create_context(&mut loaded, format);
    let second = create_context(&mut loaded, format);
    assert_ne!(first, second);
    assert_eq!(loaded.memory.read_u32_be(first), Some(first));
    assert_eq!(loaded.memory.read_u32_be(second), Some(second));
    assert_eq!(
        loaded
            .process_memory_manager
            .0
            .borrow_mut()
            .native_ptr_size(first),
        super::super::agl_macros::PPC_AGL_CONTEXT_BYTES
    );
    assert!(loaded.agl.set_current_context(second));
    for (index, value) in [0.25_f64, 0.5, 0.75, 1.0].into_iter().enumerate() {
        loaded.cpu.fpr[index + 1] = value.to_bits();
    }
    call_macro(&mut loaded, first, 0x34); // SDK clear_color slot 12
    assert_eq!(loaded.agl.current_context(), second);
    assert_eq!(
        loaded
            .agl
            .context(first)
            .unwrap()
            .gl
            .state
            .raster
            .clear_color,
        [0.25, 0.5, 0.75, 1.0]
    );
    assert_eq!(
        loaded
            .agl
            .context(second)
            .unwrap()
            .gl
            .state
            .raster
            .clear_color,
        [0.0; 4]
    );
    // glViewport's four integers begin at r4, after the GLIContext argument.
    loaded.cpu.gpr[4..8].copy_from_slice(&[5, 7, 320, 240]);
    call_macro(&mut loaded, first, 4 + 335 * 4);
    assert_eq!(loaded.agl.current_context(), second);
    assert_eq!(
        loaded.agl.context(first).unwrap().gl.state.raster.viewport,
        [5, 7, 320, 240]
    );
    loaded.cpu.gpr[3] = first;
    run_test_import(&mut loaded, PpcImportDispatcherTarget::AglDestroyContext);
    assert_eq!(loaded.cpu.gpr[3], 1);
    assert!(loaded.agl.context(first).is_none());
    assert_eq!(loaded.memory.read_u32_be(first), Some(0));
    assert_eq!(
        loaded
            .process_memory_manager
            .0
            .borrow_mut()
            .native_ptr_size(first),
        0
    );
    assert_eq!(loaded.agl.current_context(), second);
}

#[test]
fn agl_macro_reserved_slots_do_not_consume_cfm_import_capacity() {
    use super::super::agl_macros::method_name;
    assert_eq!(method_name(PPC_AGL_METHOD_IMPORT_BASE - 1), None);
    assert_eq!(
        method_name(PPC_AGL_METHOD_IMPORT_BASE + 12),
        Some("glClearColor")
    );
    assert_eq!(
        method_name(PPC_AGL_METHOD_IMPORT_BASE + 335),
        Some("glViewport")
    );
    assert_eq!(
        method_name(PPC_AGL_METHOD_IMPORT_BASE + 336),
        Some("glBlendColorEXT")
    );
    assert_eq!(method_name(PPC_IMPORT_SLOT_COUNT), None);
    assert!(PPC_AGL_METHOD_IMPORT_BASE > PPC_IMPORT_CAPACITY);
}
