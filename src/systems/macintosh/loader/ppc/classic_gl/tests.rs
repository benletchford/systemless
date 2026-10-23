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
