mod support;
use systemless_future::{
    bitmap::Bitmap,
    cpu::{Architecture, Cpu},
    cursor::{Cursor, Op},
    events::Input,
    graphics::{Draw, Graphics, Rect},
    load::{LoadedApplication, STACK_BASE},
    memory::{GuestPtr, Memory, Permissions},
    runtime::Stop,
};
fn record() -> Vec<u8> {
    let mut b = vec![0; 68];
    b[..2].copy_from_slice(&0x5000u16.to_be_bytes());
    b[32..34].copy_from_slice(&0x3000u16.to_be_bytes());
    b
}
fn invoke(app: &mut LoadedApplication, index: u32, arg: u32) {
    let tv = app
        .runtime
        .memory
        .u32(GuestPtr(app.info.toc_or_a5 + 8 + index * 4))
        .unwrap();
    let pc = app.runtime.memory.u32(GuestPtr(tv)).unwrap();
    app.runtime.cpu = Cpu::new(Architecture::PowerPc, pc, STACK_BASE + 0x8000);
    let Cpu::PowerPc(c) = &mut app.runtime.cpu else {
        panic!()
    };
    c.gpr[3] = arg;
    assert_eq!(app.runtime.advance(1).unwrap(), Stop::Budget);
    let Cpu::PowerPc(c) = &app.runtime.cpu else {
        panic!()
    };
    assert_eq!(c.gpr[3], arg);
}
#[test]
fn real_cursor_adapters_share_copied_shape_hide_depth_and_obscuring() {
    let f = support::pef_program(
        &[0x4e800020],
        &[
            b"InitGraf",
            b"InitCursor",
            b"HideCursor",
            b"ShowCursor",
            b"ObscureCursor",
        ],
        32,
    );
    let mut app = LoadedApplication::pef(&f.bytes, None).unwrap();
    app.runtime
        .memory
        .map(GuestPtr(0x1000), vec![0; 512], Permissions::RW)
        .unwrap();
    app.runtime
        .memory
        .map(GuestPtr(0x1200), record(), Permissions::RW)
        .unwrap();
    app.runtime
        .memory
        .map(
            GuestPtr(0x50000000),
            vec![0xa8, 0x51, 0xa8, 0x53, 0xa8, 0x50],
            Permissions::RX,
        )
        .unwrap();
    invoke(&mut app, 0, 0x10ca);
    invoke(&mut app, 1, 0xabcdef);
    assert!(app.runtime.process.cursor.visible());
    invoke(&mut app, 2, 11);
    invoke(&mut app, 2, 12);
    assert!(!app.runtime.process.cursor.visible());
    let stack = STACK_BASE + 0x8000;
    app.runtime
        .memory
        .write(GuestPtr(stack), &0x1200u32.to_be_bytes())
        .unwrap();
    app.runtime.cpu = Cpu::new(Architecture::M68k, 0x50000000, stack);
    assert_eq!(app.runtime.advance(1).unwrap(), Stop::Budget);
    let Cpu::M68k(c) = &app.runtime.cpu else {
        panic!()
    };
    assert_eq!(c.dar[15], stack + 4);
    // SetCursor copies its source by value and preserves the hide level.
    app.runtime
        .memory
        .write(GuestPtr(0x1200), &[0; 68])
        .unwrap();
    assert!(!app.runtime.process.cursor.visible());
    app.runtime.cpu = Cpu::new(Architecture::M68k, 0x50000002, stack);
    assert_eq!(app.runtime.advance(1).unwrap(), Stop::Budget);
    assert!(!app.runtime.process.cursor.visible());
    invoke(&mut app, 3, 13);
    assert!(app.runtime.process.cursor.visible());
    invoke(&mut app, 3, 14);
    assert!(app.runtime.process.cursor.visible());
    match app.runtime.process.cursor.draw(app.runtime.process.input) {
        Draw::Cursor { data, mask, .. } => {
            assert_eq!(data[0], 0x5000);
            assert_eq!(mask[0], 0x3000);
        }
        _ => panic!(),
    }
    invoke(&mut app, 4, 15);
    assert!(!app.runtime.process.cursor.visible());
    app.runtime
        .process
        .cursor
        .observe(app.runtime.process.input);
    assert!(!app.runtime.process.cursor.visible());
    app.runtime.process.input.horizontal = 3;
    app.runtime
        .process
        .cursor
        .observe(app.runtime.process.input);
    assert!(app.runtime.process.cursor.visible());
    let input = app.runtime.process.input;
    assert!(app
        .runtime
        .process
        .cursor
        .apply(
            &app.runtime.memory,
            app.runtime.process.ports,
            input,
            Op::Set(GuestPtr(0x1230))
        )
        .is_err());
    assert!(app
        .runtime
        .process
        .cursor
        .apply(
            &app.runtime.memory,
            app.runtime.process.ports,
            input,
            Op::Set(GuestPtr(0))
        )
        .is_err());
    match app.runtime.process.cursor.draw(input) {
        Draw::Cursor { data, .. } => assert_eq!(data[0], 0x5000),
        _ => panic!(),
    }
    invoke(&mut app, 2, 16);
    app.runtime.cpu = Cpu::new(Architecture::M68k, 0x50000004, stack);
    assert_eq!(app.runtime.advance(1).unwrap(), Stop::Budget);
    assert!(app.runtime.process.cursor.visible());
    match app.runtime.process.cursor.draw(input) {
        Draw::Cursor { data, x, y, .. } => {
            assert_eq!(data[0], 0x8000);
            assert_eq!((x, y), (2, -1));
        }
        _ => panic!(),
    }
}
#[test]
#[ignore = "requires a real wgpu adapter"]
fn gpu_cursor_truth_table_clipping_motion_and_hide_preserve_readonly_guest_pixels() {
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
    let mut gpu =
        pollster::block_on(Graphics::new(&instance, None, 8, 2, GuestPtr(0x4000))).unwrap();
    gpu.palette[0] = [255; 4];
    gpu.palette[1] = [0, 0, 0, 255];
    let mut m = Memory::default();
    m.map(GuestPtr(0x1000), record(), Permissions::RW).unwrap();
    m.map(GuestPtr(0x2000), vec![0xf0, 0xaa], Permissions::RX)
        .unwrap();
    let bitmap = Bitmap {
        base: GuestPtr(0x2000),
        row_bytes: 1,
        bounds: Rect::new(0, 0, 8, 2),
    };
    let mut cursor = Cursor::default();
    let input = Input::default();
    cursor
        .apply(&m, None, input, Op::Set(GuestPtr(0x1000)))
        .unwrap();
    for (x, expected) in [(0, 0x90), (-1, 0x30), (6, 0xf1)] {
        let moved = Input {
            horizontal: x,
            ..input
        };
        cursor.observe(moved);
        bitmap.present_draw(&gpu, &m, cursor.draw(moved)).unwrap();
        let rgba = gpu.read_rgba().unwrap();
        let bits = (0..8).fold(0u8, |b, x| (b << 1) | u8::from(rgba[x * 4] == 0));
        assert_eq!(bits, expected);
        assert_eq!(m.read(GuestPtr(0x2000), 2).unwrap(), [0xf0, 0xaa]);
    }
    cursor.apply(&m, None, input, Op::Hide).unwrap();
    bitmap.present_draw(&gpu, &m, cursor.draw(input)).unwrap();
    let rgba = gpu.read_rgba().unwrap();
    let bits = (0..8).fold(0u8, |b, x| (b << 1) | u8::from(rgba[x * 4] == 0));
    assert_eq!(bits, 0xf0);
    cursor.inside = false;
    cursor.apply(&m, None, input, Op::Show).unwrap();
    assert!(!cursor.visible());
    assert!(bitmap
        .present_draw(
            &gpu,
            &m,
            Draw::Fill {
                rect: Rect::new(0, 0, 1, 1),
                clip: bitmap.bounds,
                pattern: [0; 8],
                foreground: 1,
                background: 0,
                mode: systemless_future::graphics::Transfer::Copy
            }
        )
        .is_err());
    assert_eq!(m.read(GuestPtr(0x2000), 2).unwrap(), [0xf0, 0xaa]);
}
