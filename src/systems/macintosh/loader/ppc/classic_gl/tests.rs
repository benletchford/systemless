use super::*;
use crate::loader::ppc::classic_gl_framebuffer::ClassicGlColorBuffer;

const W: u32 = 8;
const H: u32 = 8;

fn context() -> (GlContext, ClassicGlFramebuffer) {
    let mut ctx = GlContext::new(true);
    ctx.attach_drawable(W, H);
    (ctx, ClassicGlFramebuffer::new(W, H, true).unwrap())
}

fn run(ctx: &mut GlContext, fb: &mut ClassicGlFramebuffer, calls: Vec<GlCall>) {
    let mut target = Some(fb);
    for call in calls {
        ctx.submit(call, &mut target);
    }
}

fn pixel(fb: &ClassicGlFramebuffer, x: u32, y: u32) -> [u8; 4] {
    fb.pixel(ClassicGlColorBuffer::Back, x, y).unwrap()
}

fn ortho() -> Vec<GlCall> {
    vec![
        GlCall::MatrixMode(PROJECTION),
        GlCall::LoadMatrix(Mat4::IDENTITY),
        GlCall::MultMatrix(Mat4::ortho(0.0, f64::from(W), 0.0, f64::from(H), -1.0, 1.0)),
        GlCall::MatrixMode(MODELVIEW),
        GlCall::LoadMatrix(Mat4::IDENTITY),
    ]
}

fn quad(x0: f32, y0: f32, x1: f32, y1: f32, z: f32) -> Vec<GlCall> {
    vec![
        GlCall::Begin(QUADS),
        GlCall::TexCoord(0, [0.0, 0.0, 0.0, 1.0]),
        GlCall::Vertex([x0, y0, z, 1.0]),
        GlCall::TexCoord(0, [1.0, 0.0, 0.0, 1.0]),
        GlCall::Vertex([x1, y0, z, 1.0]),
        GlCall::TexCoord(0, [1.0, 1.0, 0.0, 1.0]),
        GlCall::Vertex([x1, y1, z, 1.0]),
        GlCall::TexCoord(0, [0.0, 1.0, 0.0, 1.0]),
        GlCall::Vertex([x0, y1, z, 1.0]),
        GlCall::End,
    ]
}

#[test]
fn clear_writes_the_back_buffer_with_the_clear_color() {
    let (mut ctx, mut fb) = context();
    run(
        &mut ctx,
        &mut fb,
        vec![
            GlCall::ClearColor([1.0, 0.5, 0.0, 1.0]),
            GlCall::Clear(COLOR_BUFFER_BIT),
        ],
    );
    assert_eq!(pixel(&fb, 3, 3), [255, 128, 0, 255]);
    assert_eq!(
        fb.pixel(ClassicGlColorBuffer::Front, 3, 3),
        Some([0, 0, 0, 0])
    );
}

#[test]
fn ortho_rect_covers_exactly_the_specified_pixels() {
    let (mut ctx, mut fb) = context();
    let mut calls = ortho();
    calls.push(GlCall::Color([0.0, 1.0, 0.0, 1.0]));
    calls.push(GlCall::Rect([2.0, 2.0, 5.0, 4.0]));
    run(&mut ctx, &mut fb, calls);
    for y in 0..H {
        for x in 0..W {
            let inside = (2..5).contains(&x) && (2..4).contains(&y);
            assert_eq!(pixel(&fb, x, y)[1] == 255, inside, "pixel {x},{y}");
        }
    }
}

#[test]
fn adjacent_triangles_never_cover_a_shared_pixel_twice() {
    let (mut ctx, mut fb) = context();
    let mut calls = ortho();
    calls.extend([
        GlCall::Enable(BLEND, true),
        GlCall::BlendFunc(ONE, ONE),
        GlCall::Color([0.25, 0.0, 0.0, 1.0]),
        GlCall::Begin(TRIANGLES),
        GlCall::Vertex([0.0, 0.0, 0.0, 1.0]),
        GlCall::Vertex([8.0, 0.0, 0.0, 1.0]),
        GlCall::Vertex([8.0, 8.0, 0.0, 1.0]),
        GlCall::Vertex([0.0, 0.0, 0.0, 1.0]),
        GlCall::Vertex([8.0, 8.0, 0.0, 1.0]),
        GlCall::Vertex([0.0, 8.0, 0.0, 1.0]),
        GlCall::End,
    ]);
    run(&mut ctx, &mut fb, calls);
    for y in 0..H {
        for x in 0..W {
            assert_eq!(pixel(&fb, x, y)[0], 64, "pixel {x},{y}");
        }
    }
}

#[test]
fn depth_test_keeps_the_nearer_surface_regardless_of_order() {
    let (mut ctx, mut fb) = context();
    let mut calls = ortho();
    calls.extend([
        GlCall::Enable(DEPTH_TEST, true),
        GlCall::ClearDepth(1.0),
        GlCall::Clear(DEPTH_BUFFER_BIT | COLOR_BUFFER_BIT),
        GlCall::Color([1.0, 0.0, 0.0, 1.0]),
    ]);
    // Ortho maps eye z to depth with near = -1, so larger z is nearer.
    calls.extend(quad(0.0, 0.0, 8.0, 8.0, 0.5));
    calls.push(GlCall::Color([0.0, 0.0, 1.0, 1.0]));
    calls.extend(quad(0.0, 0.0, 8.0, 8.0, -0.5));
    run(&mut ctx, &mut fb, calls);
    assert_eq!(pixel(&fb, 4, 4), [255, 0, 0, 255]);
}

#[test]
fn textures_modulate_the_fragment_color_with_nearest_sampling() {
    let (mut ctx, mut fb) = context();
    let checker = vec![
        [1.0, 1.0, 1.0, 1.0],
        [0.0, 0.0, 0.0, 1.0],
        [0.0, 0.0, 0.0, 1.0],
        [1.0, 1.0, 1.0, 1.0],
    ];
    let mut calls = ortho();
    calls.extend([
        GlCall::BindTexture(TEXTURE_2D, 7),
        GlCall::TexImage2D {
            level: 0,
            internal: RGBA,
            width: 2,
            height: 2,
            pixels: Some(Rc::new(checker)),
        },
        GlCall::TexParameter(
            TEXTURE_2D,
            TEXTURE_MIN_FILTER,
            [NEAREST as f32, 0.0, 0.0, 0.0],
        ),
        GlCall::TexParameter(
            TEXTURE_2D,
            TEXTURE_MAG_FILTER,
            [NEAREST as f32, 0.0, 0.0, 0.0],
        ),
        GlCall::Enable(TEXTURE_2D, true),
        GlCall::Color([1.0, 0.0, 1.0, 1.0]),
    ]);
    calls.extend(quad(0.0, 0.0, 8.0, 8.0, 0.0));
    run(&mut ctx, &mut fb, calls);
    assert_eq!(pixel(&fb, 1, 1), [255, 0, 255, 255]);
    assert_eq!(pixel(&fb, 6, 1), [0, 0, 0, 255]);
    assert_eq!(pixel(&fb, 6, 6), [255, 0, 255, 255]);
}

#[test]
fn mipmap_incomplete_texture_samples_its_base_level() {
    let (mut ctx, mut fb) = context();
    let mut calls = ortho();
    calls.extend([
        GlCall::BindTexture(TEXTURE_2D, 3),
        GlCall::TexImage2D {
            level: 0,
            internal: RGB,
            width: 2,
            height: 2,
            pixels: Some(Rc::new(vec![[1.0, 0.0, 1.0, 1.0]; 4])),
        },
        GlCall::Enable(TEXTURE_2D, true),
        GlCall::Color([1.0, 1.0, 1.0, 1.0]),
    ]);
    calls.extend(quad(0.0, 0.0, 8.0, 8.0, 0.0));
    run(&mut ctx, &mut fb, calls);
    // The default NEAREST_MIPMAP_LINEAR filter with only level 0 specified
    // still textures from the base level.
    assert_eq!(pixel(&fb, 4, 4), [255, 0, 255, 255]);
}

#[test]
fn display_lists_record_in_compile_mode_and_replay_on_call() {
    let (mut ctx, mut fb) = context();
    run(&mut ctx, &mut fb, ortho());
    ctx.compiling = Some((5, false, Vec::new()));
    let mut calls = vec![GlCall::Color([0.0, 0.0, 1.0, 1.0])];
    calls.extend(quad(0.0, 0.0, 8.0, 8.0, 0.0));
    run(&mut ctx, &mut fb, calls);
    let (list, _, recorded) = ctx.compiling.take().unwrap();
    ctx.lists.insert(list, Rc::new(recorded));
    assert_eq!(pixel(&fb, 4, 4), [0, 0, 0, 0], "compile mode must not draw");
    run(&mut ctx, &mut fb, vec![GlCall::CallList(5)]);
    assert_eq!(pixel(&fb, 4, 4), [0, 0, 255, 255]);
}

#[test]
fn entry_points_decode_powerpc_registers() {
    let (mut ctx, mut fb) = context();
    let mut cpu = PpcCpu::new();
    let mut memory = PpcSectionMem::new();
    // glClearColor(0.0, 0.0, 1.0, 1.0): four floats in f1-f4.
    cpu.fpr[3] = 1.0f64.to_bits();
    cpu.fpr[4] = 1.0f64.to_bits();
    let clear_color = GlFunction::from_name("glClearColor").unwrap();
    call(&mut ctx, Some(&mut fb), clear_color, &cpu, &mut memory);
    cpu.gpr[3] = COLOR_BUFFER_BIT;
    call(
        &mut ctx,
        Some(&mut fb),
        GlFunction::from_name("glClear").unwrap(),
        &cpu,
        &mut memory,
    );
    assert_eq!(pixel(&fb, 0, 0), [0, 0, 255, 255]);
    cpu.gpr[3] = 0xFFFF_FFFF;
    call(
        &mut ctx,
        Some(&mut fb),
        GlFunction::from_name("glClear").unwrap(),
        &cpu,
        &mut memory,
    );
    assert!(matches!(
        call(
            &mut ctx,
            Some(&mut fb),
            GlFunction::from_name("glGetError").unwrap(),
            &cpu,
            &mut memory
        ),
        GlResult::Value(INVALID_VALUE)
    ));
}

#[test]
fn get_string_returns_terminated_guest_strings() {
    let (mut ctx, _) = context();
    let mut cpu = PpcCpu::new();
    let mut memory = PpcSectionMem::new();
    memory.add_region(super::super::PPC_IMPORT_DATA_BASE, vec![0; 0x1000]);
    cpu.gpr[3] = VERSION;
    let GlResult::Value(pointer) = call(
        &mut ctx,
        None,
        GlFunction::from_name("glGetString").unwrap(),
        &cpu,
        &mut memory,
    ) else {
        panic!("glGetString returns a pointer");
    };
    let bytes = super::super::memory::ppc_memory_read_bytes(&mut memory, pointer, 4).unwrap();
    assert_eq!(&bytes, b"1.2 ");
}

#[test]
fn read_pixels_packs_bottom_up_rgba_bytes() {
    let (mut ctx, mut fb) = context();
    run(
        &mut ctx,
        &mut fb,
        vec![
            GlCall::ClearColor([0.0, 1.0, 0.0, 1.0]),
            GlCall::Clear(COLOR_BUFFER_BIT),
        ],
    );
    let mut cpu = PpcCpu::new();
    let mut memory = PpcSectionMem::new();
    memory.add_region(0x1000, vec![0; 64]);
    cpu.gpr[3] = 0;
    cpu.gpr[4] = 0;
    cpu.gpr[5] = 2;
    cpu.gpr[6] = 1;
    cpu.gpr[7] = RGBA;
    cpu.gpr[8] = UNSIGNED_BYTE;
    cpu.gpr[9] = 0x1000;
    call(
        &mut ctx,
        Some(&mut fb),
        GlFunction::from_name("glReadPixels").unwrap(),
        &cpu,
        &mut memory,
    );
    let bytes = super::super::memory::ppc_memory_read_bytes(&mut memory, 0x1000, 8).unwrap();
    assert_eq!(bytes, vec![0, 255, 0, 255, 0, 255, 0, 255]);
}

#[test]
fn alpha_test_and_stencil_reject_fragments() {
    let (mut ctx, mut fb) = context();
    let mut calls = ortho();
    calls.extend([
        GlCall::Enable(STENCIL_TEST, true),
        GlCall::StencilFunc(ALWAYS, 1, 0xFF),
        GlCall::StencilOp(KEEP, KEEP, REPLACE),
        GlCall::Color([1.0, 0.0, 0.0, 1.0]),
    ]);
    calls.extend(quad(0.0, 0.0, 4.0, 8.0, 0.0));
    calls.extend([
        GlCall::StencilFunc(EQUAL, 1, 0xFF),
        GlCall::StencilOp(KEEP, KEEP, KEEP),
        GlCall::Color([0.0, 1.0, 0.0, 1.0]),
    ]);
    calls.extend(quad(0.0, 0.0, 8.0, 8.0, 0.0));
    calls.extend([
        GlCall::Enable(STENCIL_TEST, false),
        GlCall::Enable(ALPHA_TEST, true),
        GlCall::AlphaFunc(GREATER, 0.5),
        GlCall::Color([0.0, 0.0, 1.0, 0.25]),
    ]);
    calls.extend(quad(0.0, 0.0, 8.0, 8.0, 0.0));
    run(&mut ctx, &mut fb, calls);
    assert_eq!(pixel(&fb, 1, 1), [0, 255, 0, 255]);
    assert_eq!(pixel(&fb, 6, 1), [0, 0, 0, 0]);
}

#[test]
fn diffuse_lighting_follows_the_normal() {
    let (mut ctx, mut fb) = context();
    let mut calls = ortho();
    calls.extend([
        GlCall::Enable(LIGHTING, true),
        GlCall::Enable(LIGHT0, true),
        GlCall::LightModel(LIGHT_MODEL_AMBIENT, [0.0, 0.0, 0.0, 1.0]),
        GlCall::Material(FRONT_AND_BACK, DIFFUSE, [1.0, 1.0, 1.0, 1.0]),
        GlCall::Material(FRONT_AND_BACK, AMBIENT, [0.0, 0.0, 0.0, 1.0]),
        // LIGHT0 is directional along +z by default.
        GlCall::Normal([0.0, 0.0, 1.0]),
    ]);
    calls.extend(quad(0.0, 0.0, 4.0, 8.0, 0.0));
    calls.push(GlCall::Normal([1.0, 0.0, 0.0]));
    calls.extend(quad(4.0, 0.0, 8.0, 8.0, 0.0));
    run(&mut ctx, &mut fb, calls);
    assert_eq!(pixel(&fb, 1, 1), [255, 255, 255, 255]);
    assert_eq!(pixel(&fb, 6, 1), [0, 0, 0, 255]);
}

#[test]
fn linear_fog_blends_toward_the_fog_color() {
    let (mut ctx, mut fb) = context();
    let mut calls = ortho();
    calls.extend([
        GlCall::Enable(FOG, true),
        GlCall::Fog(FOG_MODE, [LINEAR as f32, 0.0, 0.0, 0.0]),
        GlCall::Fog(FOG_START, [0.0, 0.0, 0.0, 0.0]),
        GlCall::Fog(FOG_END, [1.0, 0.0, 0.0, 0.0]),
        GlCall::Fog(FOG_COLOR, [1.0, 1.0, 1.0, 1.0]),
        GlCall::Color([0.0, 0.0, 0.0, 1.0]),
    ]);
    // Eye distance 0.5 gives a fog factor of 0.5.
    calls.extend(quad(0.0, 0.0, 8.0, 8.0, -0.5));
    run(&mut ctx, &mut fb, calls);
    let p = pixel(&fb, 4, 4);
    assert!((126..=129).contains(&p[0]), "{p:?}");
}

#[test]
fn bitmap_latches_raster_color_obeys_scissor_and_advances_even_when_clipped() {
    let (mut ctx, mut fb) = context();
    let bitmap = bitmap::Bitmap {
        width: 2,
        height: 2,
        origin: [0.0; 2],
        advance: [3.0, 1.0],
        pixels: Rc::new(vec![1, 0, 1, 1]),
    };
    let mut calls = ortho();
    calls.extend([
        GlCall::Color([1.0, 0.0, 0.0, 1.0]),
        GlCall::RasterPos([2.0, 2.0, 0.0, 1.0]),
        GlCall::Color([0.0, 1.0, 0.0, 1.0]),
        GlCall::Scissor([2, 2, 1, 2]),
        GlCall::Enable(SCISSOR_TEST, true),
        GlCall::Bitmap(bitmap.clone()),
    ]);
    run(&mut ctx, &mut fb, calls);
    assert_eq!(pixel(&fb, 2, 2), [255, 0, 0, 255]);
    assert_eq!(pixel(&fb, 2, 3), [255, 0, 0, 255]);
    assert_eq!(pixel(&fb, 3, 3), [0; 4]);
    assert_eq!(ctx.state.raster.position.window, [5.0, 3.0, 0.5, 1.0]);
    run(&mut ctx, &mut fb, vec![GlCall::Bitmap(bitmap.clone())]);
    assert_eq!(ctx.state.raster.position.window[0], 8.0);
    run(
        &mut ctx,
        &mut fb,
        vec![GlCall::RasterPos([-1.0, 0.0, 0.0, 1.0])],
    );
    assert!(!ctx.state.raster.position.valid);
    let before = ctx.state.raster.position.window;
    run(&mut ctx, &mut fb, vec![GlCall::Bitmap(bitmap)]);
    assert_eq!(ctx.state.raster.position.window, before);
}

#[test]
fn bitmap_unpacking_captures_guest_bits_in_display_lists() {
    let (mut ctx, mut fb) = context();
    let mut cpu = PpcCpu::new();
    let mut memory = PpcSectionMem::new();
    let pointer = 0x10000;
    // Skip one row and three bits. LSB-first, rows padded to four bytes.
    memory.add_region(
        pointer,
        vec![0, 0, 0, 0, 0b0010_1000, 0, 0, 0, 0b0011_0000, 0, 0, 0],
    );
    ctx.state.client.unpack.skip_rows = 1;
    ctx.state.client.unpack.skip_pixels = 3;
    ctx.state.client.unpack.lsb_first = true;
    ctx.compiling = Some((77, false, Vec::new()));
    cpu.gpr[3] = 3;
    cpu.gpr[4] = 2;
    cpu.gpr[9] = pointer;
    cpu.fpr[3] = 4.0f64.to_bits();
    call(
        &mut ctx,
        Some(&mut fb),
        GlFunction::from_name("glBitmap").unwrap(),
        &cpu,
        &mut memory,
    );
    let (_, _, commands) = ctx.compiling.take().unwrap();
    ctx.lists.insert(77, Rc::new(commands));
    for i in 0..12 {
        memory.write_u8(pointer + i, 0).unwrap();
    }
    let mut calls = ortho();
    calls.extend([
        GlCall::RasterPos([1.0, 1.0, 0.0, 1.0]),
        GlCall::CallList(77),
    ]);
    run(&mut ctx, &mut fb, calls);
    for (x, y) in [(1, 1), (3, 1), (2, 2), (3, 2)] {
        assert_eq!(pixel(&fb, x, y), [255; 4]);
    }
    assert_eq!(pixel(&fb, 2, 1), [0; 4]);
    assert_eq!(pixel(&fb, 1, 2), [0; 4]);
    assert_eq!(ctx.state.raster.position.window[0], 5.0);
    // Invalid dimensions must not replace the raster state or read memory.
    cpu.gpr[3] = u32::MAX;
    call(
        &mut ctx,
        Some(&mut fb),
        GlFunction::from_name("glBitmap").unwrap(),
        &cpu,
        &mut memory,
    );
    assert_eq!(ctx.state.take_error(), INVALID_VALUE);
}

#[test]
fn agl_font_lists_draw_glyphs_and_keep_spacing_when_shared() {
    let mut ctx = GlContext::new(true);
    ctx.attach_drawable(64, 32);
    let mut fb = ClassicGlFramebuffer::new(64, 32, true).unwrap();
    ctx.use_font(4, 0, 12, i32::from(b'A'), 2, 200).unwrap();
    assert_eq!(ctx.lists.len(), 2);
    // Lists are shared objects, so they can render after ownership moves.
    let objects = ctx.take_shared_objects();
    let mut other = GlContext::new(true);
    other.attach_drawable(64, 32);
    other.put_shared_objects(objects);
    let calls = vec![
        GlCall::MatrixMode(PROJECTION),
        GlCall::LoadMatrix(Mat4::ortho(0.0, 64.0, 0.0, 32.0, -1.0, 1.0)),
        GlCall::RasterPos([2.0, 10.0, 0.0, 1.0]),
        GlCall::CallList(200),
        GlCall::CallList(201),
    ];
    run(&mut other, &mut fb, calls);
    let advance = crate::quickdraw::text::get_glyph(4, 12, 'A')
        .unwrap()
        .0
        .advance as f32;
    assert_eq!(other.state.raster.position.window[0], 2.0 + 2.0 * advance);
    assert!((10..24).any(|y| (2..9).any(|x| pixel(&fb, x, y)[0] != 0)));
    assert!((10..24).any(|y| (9..18).any(|x| pixel(&fb, x, y)[0] != 0)));
    assert!(other.use_font(4, 0, 12, 0, -1, 300).is_err());
    assert!(!other.lists.contains_key(&300));
}

#[test]
fn raster_position_vector_queries_and_bitmap_depth_test_use_latched_state() {
    let (mut ctx, mut fb) = context();
    let mut cpu = PpcCpu::new();
    let mut memory = PpcSectionMem::new();
    let pointer = 0x20000;
    memory.add_region(pointer, vec![0; 64]);
    for (index, value) in [0.0f64, 0.0, -0.5, 1.0].into_iter().enumerate() {
        memory
            .write_u64_be(pointer + index as u32 * 8, value.to_bits())
            .unwrap();
    }
    cpu.gpr[3] = pointer;
    call(
        &mut ctx,
        Some(&mut fb),
        GlFunction::from_name("glRasterPos4dv").unwrap(),
        &cpu,
        &mut memory,
    );
    cpu.gpr[3] = CURRENT_RASTER_POSITION;
    cpu.gpr[4] = pointer + 32;
    call(
        &mut ctx,
        Some(&mut fb),
        GlFunction::from_name("glGetFloatv").unwrap(),
        &cpu,
        &mut memory,
    );
    assert_eq!(
        (0..4)
            .map(|i| f32::from_bits(memory.read_u32_be(pointer + 32 + 4 * i).unwrap()))
            .collect::<Vec<_>>(),
        vec![4.0, 4.0, 0.25, 1.0]
    );
    let bitmap = bitmap::Bitmap {
        width: 1,
        height: 1,
        origin: [0.0; 2],
        advance: [0.0; 2],
        pixels: Rc::new(vec![1]),
    };
    run(
        &mut ctx,
        &mut fb,
        vec![
            GlCall::Enable(DEPTH_TEST, true),
            GlCall::Bitmap(bitmap.clone()),
            GlCall::Color([1.0, 0.0, 0.0, 1.0]),
            GlCall::RasterPos([0.0, 0.0, 0.5, 1.0]),
            GlCall::Bitmap(bitmap),
        ],
    );
    assert_eq!(pixel(&fb, 4, 4), [255; 4]);
    assert_eq!(fb.depth_at(4, 4), Some(0.25));
    assert_eq!(ctx.state.take_error(), NO_ERROR);
}

#[test]
fn apple_transform_hint_round_trips_lists_and_saved_state() {
    let (mut ctx, mut fb) = context();
    assert!(GL_EXTENSIONS
        .split_whitespace()
        .any(|name| name == "GL_APPLE_transform_hint"));
    assert_eq!(
        get_values(&ctx, TRANSFORM_HINT_APPLE),
        Some((vec![f64::from(DONT_CARE)], false))
    );
    ctx.compiling = Some((9, false, Vec::new()));
    run(
        &mut ctx,
        &mut fb,
        vec![GlCall::Hint(TRANSFORM_HINT_APPLE, FASTEST)],
    );
    let (list, _, recorded) = ctx.compiling.take().unwrap();
    ctx.lists.insert(list, Rc::new(recorded));
    assert_eq!(ctx.state.raster.hints[5], DONT_CARE);
    run(
        &mut ctx,
        &mut fb,
        vec![
            GlCall::CallList(9),
            GlCall::PushAttrib(HINT_BIT),
            GlCall::Hint(TRANSFORM_HINT_APPLE, NICEST),
        ],
    );
    assert_eq!(
        get_values(&ctx, TRANSFORM_HINT_APPLE),
        Some((vec![f64::from(NICEST)], false))
    );
    run(&mut ctx, &mut fb, vec![GlCall::PopAttrib]);
    assert_eq!(
        get_values(&ctx, TRANSFORM_HINT_APPLE),
        Some((vec![f64::from(FASTEST)], false))
    );
    assert_eq!(ctx.state.error, NO_ERROR);
}

#[test]
fn hint_rejects_invalid_modes_without_changing_state() {
    for target in [
        PERSPECTIVE_CORRECTION_HINT,
        POINT_SMOOTH_HINT,
        LINE_SMOOTH_HINT,
        POLYGON_SMOOTH_HINT,
        FOG_HINT,
        TRANSFORM_HINT_APPLE,
    ] {
        let (mut ctx, mut fb) = context();
        run(
            &mut ctx,
            &mut fb,
            vec![GlCall::Hint(target, NICEST), GlCall::Hint(target, 0)],
        );
        assert_eq!(ctx.state.error, INVALID_ENUM);
        assert_eq!(
            get_values(&ctx, target),
            Some((vec![f64::from(NICEST)], false))
        );
    }
}

#[test]
fn hint_attribute_mask_isolated_through_public_entry_points() {
    for mask in [0, HINT_BIT, COLOR_BUFFER_BIT, ALL_ATTRIB_BITS] {
        let (mut ctx, mut fb) = context();
        let mut cpu = PpcCpu::new();
        let mut memory = PpcSectionMem::new();
        cpu.gpr[3] = mask;
        call(
            &mut ctx,
            Some(&mut fb),
            GlFunction::from_name("glPushAttrib").unwrap(),
            &cpu,
            &mut memory,
        );
        run(
            &mut ctx,
            &mut fb,
            vec![
                GlCall::Hint(TRANSFORM_HINT_APPLE, FASTEST),
                GlCall::ClearColor([1.0, 0.0, 0.0, 1.0]),
            ],
        );
        call(
            &mut ctx,
            Some(&mut fb),
            GlFunction::from_name("glPopAttrib").unwrap(),
            &cpu,
            &mut memory,
        );
        assert_eq!(
            ctx.state.raster.hints[5],
            if mask & HINT_BIT != 0 {
                DONT_CARE
            } else {
                FASTEST
            }
        );
        assert_eq!(
            ctx.state.raster.clear_color,
            if mask & COLOR_BUFFER_BIT != 0 {
                [0.0; 4]
            } else {
                [1.0, 0.0, 0.0, 1.0]
            }
        );
        assert_eq!(ctx.state.error, NO_ERROR);
    }
}

#[test]
fn attribute_groups_preserve_unselected_viewport_and_depth_changes() {
    let (mut ctx, mut fb) = context();
    run(
        &mut ctx,
        &mut fb,
        vec![
            GlCall::PushAttrib(COLOR_BUFFER_BIT),
            GlCall::ClearColor([1.0, 0.0, 0.0, 1.0]),
            GlCall::Viewport([1, 2, 3, 4]),
            GlCall::Enable(DEPTH_TEST, true),
            GlCall::PopAttrib,
        ],
    );
    assert_eq!(ctx.state.raster.clear_color, [0.0; 4]);
    assert_eq!(ctx.state.raster.viewport, [1, 2, 3, 4]);
    assert!(ctx.state.raster.enables.depth_test);
    assert_eq!(ctx.state.error, NO_ERROR);
}

#[test]
fn current_and_transform_groups_restore_values_without_restoring_matrices() {
    let (mut ctx, mut fb) = context();
    let initial_color = ctx.state.current.color;
    let initial_raster = ctx.state.raster.position.window;
    run(
        &mut ctx,
        &mut fb,
        vec![
            GlCall::PushAttrib(CURRENT_BIT | TRANSFORM_BIT),
            GlCall::Color([0.25, 0.5, 0.75, 1.0]),
            GlCall::RasterPos([0.5, 0.5, 0.0, 1.0]),
            GlCall::MatrixMode(PROJECTION),
            GlCall::LoadMatrix(Mat4::ortho(0.0, 10.0, 0.0, 10.0, -1.0, 1.0)),
            GlCall::Enable(NORMALIZE, true),
            GlCall::PopAttrib,
        ],
    );
    assert_eq!(ctx.state.current.color, initial_color);
    assert_eq!(ctx.state.raster.position.window, initial_raster);
    assert_eq!(ctx.state.raster.matrix_mode, MODELVIEW);
    assert!(!ctx.state.raster.enables.normalize);
    assert_ne!(ctx.state.projection[0].0, Mat4::IDENTITY.0);
    assert_eq!(ctx.state.error, NO_ERROR);
}

#[test]
fn enable_snapshot_does_not_restore_light_or_texture_parameters() {
    let (mut ctx, _) = context();
    ctx.state.push_attributes(ENABLE_BIT);
    ctx.state.raster.lights[2].enabled = true;
    ctx.state.raster.lights[2].ambient = [0.5; 4];
    ctx.state.raster.units[1].enabled_2d = true;
    ctx.state.raster.units[1].gen[2].enabled = true;
    ctx.state.raster.units[1].gen[2].object_plane = [0.25; 4];
    ctx.state.pop_attributes();
    assert!(!ctx.state.raster.lights[2].enabled);
    assert_eq!(ctx.state.raster.lights[2].ambient, [0.5; 4]);
    assert!(!ctx.state.raster.units[1].enabled_2d);
    assert!(!ctx.state.raster.units[1].gen[2].enabled);
    assert_eq!(ctx.state.raster.units[1].gen[2].object_plane, [0.25; 4]);
}

#[test]
fn texture_attribute_snapshot_restores_bound_parameters_but_keeps_image_changes() {
    let (mut ctx, _) = context();
    ctx.state.raster.units[0].binding_2d = 7;
    ctx.state.raster.units[1].binding_2d = 8;
    for name in [7, 8, 9] {
        ctx.state.textures.insert(name, texture::Texture::default());
    }
    ctx.state.push_attributes(TEXTURE_BIT);
    ctx.state.raster.units[0].binding_2d = 9;
    ctx.state.raster.active_unit = 1;
    for name in [7, 8, 9] {
        let texture = ctx.state.textures.get_mut(&name).unwrap();
        texture.wrap_s = CLAMP;
        texture.levels.push(Some(texture::TexImage {
            width: 1,
            height: 1,
            texels: vec![[255, 0, 0, 255]],
        }));
    }
    ctx.state.pop_attributes();
    assert_eq!(ctx.state.raster.units[0].binding_2d, 7);
    assert_eq!(ctx.state.raster.units[1].binding_2d, 8);
    assert_eq!(ctx.state.raster.active_unit, 0);
    for name in [7, 8, 9] {
        let texture = &ctx.state.textures[&name];
        assert_eq!(texture.wrap_s, if name == 9 { CLAMP } else { REPEAT });
        assert_eq!(
            texture.levels[0].as_ref().unwrap().texels,
            [[255, 0, 0, 255]]
        );
    }
}

#[test]
fn client_attribute_groups_execute_immediately_during_list_compilation() {
    for mask in [
        CLIENT_PIXEL_STORE_BIT,
        CLIENT_VERTEX_ARRAY_BIT,
        CLIENT_ALL_ATTRIB_BITS,
        0,
    ] {
        let (mut ctx, mut fb) = context();
        let mut cpu = PpcCpu::new();
        let mut memory = PpcSectionMem::new();
        ctx.compiling = Some((17, false, Vec::new()));
        cpu.gpr[3] = mask;
        call(
            &mut ctx,
            Some(&mut fb),
            GlFunction::from_name("glPushClientAttrib").unwrap(),
            &cpu,
            &mut memory,
        );
        assert_eq!(ctx.state.client_attrib_stack.len(), 1);
        ctx.state.client.unpack.alignment = 1;
        ctx.state.client.vertex.pointer = 0x1234;
        ctx.state.client.active_unit = 1;
        call(
            &mut ctx,
            Some(&mut fb),
            GlFunction::from_name("glPopClientAttrib").unwrap(),
            &cpu,
            &mut memory,
        );
        assert_eq!(
            ctx.state.client.unpack.alignment,
            if mask & CLIENT_PIXEL_STORE_BIT != 0 {
                4
            } else {
                1
            }
        );
        assert_eq!(
            ctx.state.client.vertex.pointer,
            if mask & CLIENT_VERTEX_ARRAY_BIT != 0 {
                0
            } else {
                0x1234
            }
        );
        assert_eq!(
            ctx.state.client.active_unit,
            if mask & CLIENT_VERTEX_ARRAY_BIT != 0 {
                0
            } else {
                1
            }
        );
        assert!(ctx.compiling.as_ref().unwrap().2.is_empty());
        assert_eq!(ctx.state.error, NO_ERROR);
    }
}

#[test]
fn attribute_stacks_report_limits_without_corrupting_saved_entries() {
    let (mut ctx, _) = context();
    ctx.state.push_attributes(0x8000_0000);
    ctx.state.push_client_attributes(0x100);
    assert_eq!(ctx.state.take_error(), INVALID_VALUE);
    assert!(ctx.state.attrib_stack.is_empty());
    assert!(ctx.state.client_attrib_stack.is_empty());
    for _ in 0..16 {
        ctx.state.push_attributes(0);
        ctx.state.push_client_attributes(0);
    }
    ctx.state.push_attributes(0);
    assert_eq!(ctx.state.take_error(), STACK_OVERFLOW);
    ctx.state.push_client_attributes(0);
    assert_eq!(ctx.state.take_error(), STACK_OVERFLOW);
    for _ in 0..16 {
        ctx.state.pop_attributes();
        ctx.state.pop_client_attributes();
    }
    assert_eq!(ctx.state.take_error(), NO_ERROR);
    ctx.state.pop_attributes();
    assert_eq!(ctx.state.take_error(), STACK_UNDERFLOW);
    ctx.state.pop_client_attributes();
    assert_eq!(ctx.state.take_error(), STACK_UNDERFLOW);
}

#[test]
fn server_attribute_stacks_are_nested_and_evaluated_when_lists_execute() {
    let (mut ctx, mut fb) = context();
    ctx.compiling = Some((21, false, Vec::new()));
    run(
        &mut ctx,
        &mut fb,
        vec![
            GlCall::PushAttrib(COLOR_BUFFER_BIT),
            GlCall::ClearColor([1.0, 0.0, 0.0, 1.0]),
            GlCall::PushAttrib(VIEWPORT_BIT),
            GlCall::Viewport([2, 3, 4, 5]),
            GlCall::PopAttrib,
            GlCall::PopAttrib,
        ],
    );
    let (name, _, commands) = ctx.compiling.take().unwrap();
    assert!(ctx.state.attrib_stack.is_empty());
    ctx.lists.insert(name, Rc::new(commands));
    ctx.state.raster.clear_color = [0.0, 0.0, 1.0, 1.0];
    ctx.state.raster.viewport = [0, 0, 6, 7];
    run(&mut ctx, &mut fb, vec![GlCall::CallList(21)]);
    assert_eq!(ctx.state.raster.clear_color, [0.0, 0.0, 1.0, 1.0]);
    assert_eq!(ctx.state.raster.viewport, [0, 0, 6, 7]);
    assert!(ctx.state.attrib_stack.is_empty());
    assert_eq!(ctx.state.error, NO_ERROR);
}

#[test]
fn client_attribute_operations_inside_begin_do_not_change_the_stack() {
    let (mut ctx, mut fb) = context();
    let mut cpu = PpcCpu::new();
    let mut memory = PpcSectionMem::new();
    ctx.state.push_client_attributes(CLIENT_ALL_ATTRIB_BITS);
    run(&mut ctx, &mut fb, vec![GlCall::Begin(POINTS)]);
    cpu.gpr[3] = CLIENT_ALL_ATTRIB_BITS;
    for function in ["glPushClientAttrib", "glPopClientAttrib"] {
        call(
            &mut ctx,
            Some(&mut fb),
            GlFunction::from_name(function).unwrap(),
            &cpu,
            &mut memory,
        );
        assert_eq!(ctx.state.take_error(), INVALID_OPERATION);
        assert_eq!(ctx.state.client_attrib_stack.len(), 1);
    }
    run(&mut ctx, &mut fb, vec![GlCall::End]);
    ctx.state.pop_client_attributes();
    assert_eq!(ctx.state.error, NO_ERROR);
}

#[test]
fn polygon_group_restores_stipple_enable_without_restoring_line_state() {
    let (mut ctx, mut fb) = context();
    run(
        &mut ctx,
        &mut fb,
        vec![
            GlCall::PushAttrib(POLYGON_BIT),
            GlCall::Enable(POLYGON_STIPPLE, true),
            GlCall::Enable(LINE_STIPPLE, true),
            GlCall::PopAttrib,
        ],
    );
    assert!(!ctx.state.raster.enables.polygon_stipple);
    assert!(ctx.state.raster.enables.line_stipple);
    assert_eq!(ctx.state.error, NO_ERROR);
}
