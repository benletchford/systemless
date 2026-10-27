use super::*;

#[test]
fn agl_use_font_creates_lists_in_the_named_context_without_changing_current_context() {
    use super::super::classic_gl::{consts::BACK, functions::GlFunction};
    use super::super::classic_gl_agl::{PpcAglFunction, PpcAglPixelFormatRequest, AGL_BAD_CONTEXT};
    use super::super::classic_gl_framebuffer::{ClassicGlColorBuffer, ClassicGlFramebuffer};
    let mut loaded = load_pef_application(&synthetic_pef_with_library_import(
        b"OpenGLLibrary",
        b"aglUseFont",
    ))
    .unwrap();
    let format = loaded.agl.choose_pixel_format(PpcAglPixelFormatRequest {
        rgba: true,
        double_buffered: true,
        ..Default::default()
    });
    let font_context = loaded.agl.create_context(format, 0);
    let other_context = loaded.agl.create_context(format, 0);
    assert!(loaded.agl.set_current_context(other_context));
    loaded.cpu.gpr[3] = font_context;
    loaded.cpu.gpr[4] = 4; // Monaco
    loaded.cpu.gpr[5] = 0;
    loaded.cpu.gpr[6] = 12;
    loaded.cpu.gpr[7] = u32::from(b'A');
    loaded.cpu.gpr[8] = 1;
    loaded.cpu.gpr[9] = 100;
    run_test_import(
        &mut loaded,
        PpcImportDispatcherTarget::Agl(PpcAglFunction::UseFont),
    );
    assert_eq!(loaded.cpu.gpr[3], 1);
    assert_eq!(loaded.agl.current_context(), other_context);
    loaded.cpu.gpr[3] = 100;
    run_test_import(
        &mut loaded,
        PpcImportDispatcherTarget::Gl(GlFunction::from_name("glIsList").unwrap()),
    );
    assert_eq!(loaded.cpu.gpr[3], 0);
    let shared = loaded.agl.create_context(format, font_context);
    assert!(loaded.agl.set_current_context(shared));
    let context = loaded.agl.context_mut(shared).unwrap();
    context.framebuffer = Some(ClassicGlFramebuffer::new(64, 32, true).unwrap());
    context.gl.attach_drawable(64, 32);
    context.gl.state.raster.draw_buffer = BACK;
    loaded.cpu.gpr[3] = 100;
    run_test_import(
        &mut loaded,
        PpcImportDispatcherTarget::Gl(GlFunction::from_name("glIsList").unwrap()),
    );
    assert_eq!(loaded.cpu.gpr[3], 1);
    // Initial identity matrices map (0,0) to the drawable center.
    loaded.cpu.gpr[3] = 0;
    loaded.cpu.gpr[4] = 0;
    run_test_import(
        &mut loaded,
        PpcImportDispatcherTarget::Gl(GlFunction::from_name("glRasterPos2i").unwrap()),
    );
    loaded.cpu.gpr[3] = 100;
    run_test_import(
        &mut loaded,
        PpcImportDispatcherTarget::Gl(GlFunction::from_name("glCallList").unwrap()),
    );
    let fb = loaded
        .agl
        .context(shared)
        .unwrap()
        .framebuffer
        .as_ref()
        .unwrap();
    assert!((16..30)
        .any(|y| (32..44).any(|x| fb.pixel(ClassicGlColorBuffer::Back, x, y).unwrap()[0] != 0)));
    loaded.cpu.gpr[3] = font_context;
    loaded.cpu.gpr[6] = 65548; // Must not wrap to a 12-point QuickDraw size.
    run_test_import(
        &mut loaded,
        PpcImportDispatcherTarget::Agl(PpcAglFunction::UseFont),
    );
    assert_eq!(loaded.cpu.gpr[3], 0);
    assert_eq!(
        loaded.agl.get_error(),
        super::super::classic_gl_agl::AGL_BAD_ALLOC
    );
    assert_eq!(loaded.agl.current_context(), shared);
    loaded.cpu.gpr[3] = 0;
    run_test_import(
        &mut loaded,
        PpcImportDispatcherTarget::Agl(PpcAglFunction::UseFont),
    );
    assert_eq!(loaded.cpu.gpr[3], 0);
    assert_eq!(loaded.agl.get_error(), AGL_BAD_CONTEXT);
    assert_eq!(loaded.agl.current_context(), shared);
}
