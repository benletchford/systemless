mod support;
use systemless_future::{
    cpu::{Architecture, Cpu},
    display::Display,
    graphics::{Graphics, Rect},
    load::{LoadedApplication, STACK_BASE},
    memory::{GuestPtr, Permissions},
    quickdraw::SCREEN,
    runtime::Stop,
};
const PORT: u32 = 0x50000000;
fn invoke(a: &mut LoadedApplication, index: u32, args: &[u32]) -> u32 {
    let tv = a
        .runtime
        .memory
        .u32(GuestPtr(a.info.toc_or_a5 + 8 + index * 4))
        .unwrap();
    let pc = a.runtime.memory.u32(GuestPtr(tv)).unwrap();
    a.runtime.cpu = Cpu::new(Architecture::PowerPc, pc, STACK_BASE + 0x8000);
    if let Cpu::PowerPc(c) = &mut a.runtime.cpu {
        for (i, v) in args.iter().enumerate() {
            c.gpr[3 + i] = *v;
        }
    }
    assert_eq!(a.runtime.advance(1).unwrap(), Stop::Budget);
    let Cpu::PowerPc(c) = &a.runtime.cpu else {
        panic!()
    };
    c.gpr[3]
}
fn native(a: &mut LoadedApplication, offset: u32, args: &[u8]) {
    let sp = STACK_BASE + 0x8000;
    let mut f = args.to_vec();
    f.extend([0x5a; 4]);
    a.runtime.memory.write(GuestPtr(sp), &f).unwrap();
    a.runtime.cpu = Cpu::new(Architecture::M68k, 0x51000000 + offset, sp);
    if let Cpu::M68k(c) = &mut a.runtime.cpu {
        c.dar[0] = 0x12345678;
    }
    assert_eq!(a.runtime.advance(1).unwrap(), Stop::Budget);
    let Cpu::M68k(c) = &a.runtime.cpu else {
        panic!()
    };
    assert_eq!(c.dar[15], sp + args.len() as u32);
    assert_eq!(c.dar[0], 0x12345678);
    assert_eq!(
        a.runtime
            .memory
            .read(GuestPtr(sp + args.len() as u32), 4)
            .unwrap(),
        [0x5a; 4]
    );
}
fn rectangle(a: &mut LoadedApplication, at: u32, r: Rect) {
    let b: Vec<_> = [r.top, r.left, r.bottom, r.right]
        .into_iter()
        .flat_map(|v| (v as i16).to_be_bytes())
        .collect();
    a.runtime.memory.write(GuestPtr(at), &b).unwrap();
}

fn setup(width: u16, height: u16) -> LoadedApplication {
    let f = support::pef_program(
        &[0x4e800020],
        &[
            b"InitGraf",
            b"OpenPort",
            b"OpenCPort",
            b"PaintRect",
            b"RGBForeColor",
            b"SetPort",
            b"CopyBits",
            b"SetCursor",
            b"HideCursor",
            b"InitWindows",
            b"NewWindow",
            b"MoveWindow",
            b"InitFonts",
            b"InitMenus",
            b"DrawMenuBar",
            b"SysEnvirons",
            b"NewCWindow",
            b"SetGDevice",
            b"HideWindow",
            b"ShowWindow",
            b"RGBBackColor",
        ],
        64,
    );
    let mut a = LoadedApplication::pef(&f.bytes, None)
        .unwrap()
        .with_display(Display::indexed8(width, height).unwrap())
        .unwrap();
    a.runtime
        .memory
        .map(GuestPtr(PORT), vec![0; 2048], Permissions::RW)
        .unwrap();
    a.runtime
        .memory
        .map(
            GuestPtr(0x51000000),
            vec![0xa8, 0xa2, 0xaa, 0x14, 0xa9, 0x1b],
            Permissions::RX,
        )
        .unwrap();
    invoke(&mut a, 0, &[0x600]);
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
    a.runtime.graphics = Some(
        pollster::block_on(Graphics::new(
            &instance,
            None,
            u32::from(width),
            u32::from(height),
            GuestPtr(0x4000),
        ))
        .unwrap(),
    );
    a
}
fn pixel(a: &LoadedApplication, x: u32, y: u32) -> u8 {
    let stride = a.runtime.display().width().div_ceil(2) * 2;
    a.runtime
        .memory
        .read(GuestPtr(SCREEN.0 + y * stride + x), 1)
        .unwrap()[0]
}
fn rgba(a: &LoadedApplication, x: usize, y: usize) -> Vec<u8> {
    let width = a.runtime.display().width() as usize;
    let b = a.runtime.graphics.as_ref().unwrap().read_rgba().unwrap();
    b[(y * width + x) * 4..(y * width + x) * 4 + 4].to_vec()
}
fn main_map(a: &LoadedApplication) -> u32 {
    let m = &a.runtime.memory;
    let h = m.u32(systemless_future::devices::MAIN).unwrap();
    let gd = m.u32(GuestPtr(h)).unwrap();
    let ph = m.u32(GuestPtr(gd + 22)).unwrap();
    m.u32(GuestPtr(ph)).unwrap()
}
#[test]
fn display_configuration_is_bounded_and_requires_unexecuted_application() {
    for value in [
        "0x480x8",
        "640x0x8",
        "4097x480x8",
        "640x480x32",
        "640x480",
        "999999x1x8",
    ] {
        assert!(value.parse::<Display>().is_err());
    }
    let f = support::pef_program(&[0x4e800020], &[b"InitGraf"], 64);
    let mut a = LoadedApplication::pef(&f.bytes, None).unwrap();
    assert_eq!(a.runtime.display(), Display::default());
    invoke(&mut a, 0, &[0x600]);
    assert!(a.with_display(Display::indexed8(65, 40).unwrap()).is_err());
    let a = LoadedApplication::pef(&f.bytes, None)
        .unwrap()
        .with_display("65x40x8".parse().unwrap())
        .unwrap();
    let pm = main_map(&a);
    assert_eq!(a.runtime.memory.u16(GuestPtr(pm + 32)).unwrap(), 8);
    assert_eq!(a.runtime.memory.u16(GuestPtr(pm + 4)).unwrap(), 0x8042);
    assert_eq!(
        a.runtime.memory.read(SCREEN, 66 * 40).unwrap(),
        vec![0; 66 * 40]
    );
    assert!(a
        .runtime
        .memory
        .read(GuestPtr(SCREEN.0 + 66 * 40), 1)
        .is_err());
}
#[test]
#[ignore = "requires a real wgpu adapter"]
fn basic_and_color_screen_views_share_pixels_palette_copies_and_cursor_without_shadow_storage() {
    let mut a = setup(65, 40);
    invoke(&mut a, 1, &[PORT]);
    rectangle(&mut a, PORT + 512, Rect::new(2, 3, 8, 6));
    native(&mut a, 0, &(PORT + 512).to_be_bytes());
    assert_eq!(pixel(&a, 2, 3), 255);
    assert_eq!(pixel(&a, 1, 3), 0);
    a.runtime
        .memory
        .write(GuestPtr(SCREEN.0 + 66 * 3 + 64), &[28, 0xa5])
        .unwrap();
    invoke(&mut a, 2, &[PORT + 128]);
    a.runtime
        .memory
        .write(GuestPtr(PORT + 600), &[0xff, 0xff, 0, 0, 0, 0])
        .unwrap();
    native(&mut a, 2, &(PORT + 600).to_be_bytes());
    rectangle(&mut a, PORT + 512, Rect::new(9, 3, 12, 6));
    invoke(&mut a, 3, &[PORT + 512]);
    assert_eq!(pixel(&a, 9, 3), 31);
    a.runtime.refresh_screen().unwrap();
    assert_eq!(rgba(&a, 9, 3), [255, 0, 0, 255]);
    assert_eq!(rgba(&a, 64, 3), [255, 0, 255, 255]);
    assert_eq!(
        a.runtime
            .memory
            .read(GuestPtr(SCREEN.0 + 66 * 3 + 65), 1)
            .unwrap(),
        [0xa5]
    );
    // CopyBits accepts a basic screen descriptor as a color source/destination.
    invoke(&mut a, 5, &[PORT]);
    rectangle(&mut a, PORT + 520, Rect::new(20, 3, 23, 6));
    invoke(
        &mut a,
        6,
        &[PORT + 2, PORT + 2, PORT + 512, PORT + 520, 0, 0],
    );
    assert_eq!(pixel(&a, 20, 3), 31);
    // A live table change affects refresh and cursor presentation, not canonical indices.
    let pm = main_map(&a);
    let th = a.runtime.memory.u32(GuestPtr(pm + 42)).unwrap();
    let ct = a.runtime.memory.u32(GuestPtr(th)).unwrap();
    a.runtime
        .memory
        .write(GuestPtr(ct + 10 + 31 * 8), &[0, 0, 0xff, 0xff, 0, 0])
        .unwrap();
    a.runtime.refresh_screen().unwrap();
    assert_eq!(rgba(&a, 20, 3), [0, 255, 0, 255]);
    let mut cursor = vec![0; 68];
    cursor[..2].copy_from_slice(&0x5000u16.to_be_bytes());
    cursor[32..34].copy_from_slice(&0x3000u16.to_be_bytes());
    a.runtime
        .memory
        .write(GuestPtr(PORT + 700), &cursor)
        .unwrap();
    a.runtime.process.input.horizontal = 9;
    a.runtime.process.input.vertical = 3;
    invoke(&mut a, 7, &[PORT + 700]);
    let snapshot = a.runtime.memory.read(SCREEN, 66 * 40).unwrap().to_vec();
    a.runtime.refresh_screen().unwrap();
    assert_eq!(rgba(&a, 9, 3), [0, 255, 0, 255]);
    assert_eq!(rgba(&a, 10, 3), [255, 0, 255, 255]);
    assert_eq!(rgba(&a, 11, 3), [255, 255, 255, 255]);
    assert_eq!(rgba(&a, 12, 3), [0, 0, 0, 255]);
    assert_eq!(a.runtime.memory.read(SCREEN, 66 * 40).unwrap(), snapshot);
    invoke(&mut a, 8, &[]);
    a.runtime.refresh_screen().unwrap();
    assert_eq!(rgba(&a, 10, 3), [0, 255, 0, 255]);
    // Do not reinterpret an unsupported interior color-screen base as one-bit storage.
    let view = systemless_future::bitmap::Bitmap {
        base: GuestPtr(SCREEN.0 + 66),
        row_bytes: 66,
        bounds: Rect::new(0, 0, 65, 39),
    };
    let snapshot = a.runtime.memory.read(SCREEN, 66 * 40).unwrap().to_vec();
    assert!(view
        .present(a.runtime.graphics.as_ref().unwrap(), &a.runtime.memory)
        .is_err());
    assert_eq!(a.runtime.memory.read(SCREEN, 66 * 40).unwrap(), snapshot);
}
#[test]
#[ignore = "requires a real wgpu adapter"]
fn window_moves_and_menu_drawing_preserve_real_color_screen_content_on_both_cpu_paths() {
    let mut a = setup(240, 180);
    invoke(&mut a, 12, &[]);
    invoke(&mut a, 9, &[]);
    invoke(&mut a, 13, &[]);
    rectangle(&mut a, PORT + 512, Rect::new(40, 60, 100, 110));
    a.runtime
        .memory
        .write(GuestPtr(PORT + 600), b"\x04test")
        .unwrap();
    let w = invoke(
        &mut a,
        10,
        &[0, PORT + 512, PORT + 600, 1, 2, u32::MAX, 0, 0],
    );
    assert_ne!(w, 0);
    a.runtime
        .memory
        .write(GuestPtr(SCREEN.0 + 70 * 240 + 50), &[31])
        .unwrap();
    a.runtime
        .memory
        .write(GuestPtr(SCREEN.0 + 80 * 240 + 60), &[227])
        .unwrap();
    invoke(&mut a, 11, &[w, 50, 70, 0]);
    assert_eq!(pixel(&a, 60, 80), 31);
    assert_eq!(pixel(&a, 70, 90), 227);
    // Native MoveWindow moves the same colored pixels again.
    let mut frame = vec![0, 0];
    frame.extend(80i16.to_be_bytes());
    frame.extend(60i16.to_be_bytes());
    frame.extend(w.to_be_bytes());
    native(&mut a, 4, &frame);
    assert_eq!(pixel(&a, 70, 90), 31);
    assert_eq!(pixel(&a, 80, 100), 227);
    let before = a.runtime.memory.read(SCREEN, 240 * 180).unwrap().to_vec();
    invoke(&mut a, 14, &[]);
    assert_eq!(
        &a.runtime.memory.read(SCREEN, 240 * 180).unwrap()[20 * 240..],
        &before[20 * 240..]
    );
    a.runtime.refresh_screen().unwrap();
    assert_eq!(rgba(&a, 70, 90), [255, 0, 0, 255]);
    assert_eq!(rgba(&a, 80, 100), [0, 255, 0, 255]);
}

#[test]
#[ignore = "requires a real wgpu adapter"]
fn color_environment_follows_real_configured_display_on_both_cpus() {
    let mut a = setup(65, 40);
    assert_eq!(invoke(&mut a, 15, &[2, PORT + 900]), 0);
    let expected = a
        .runtime
        .memory
        .read(GuestPtr(PORT + 900), 16)
        .unwrap()
        .to_vec();
    assert_eq!(expected[9], 1);
    assert_eq!(expected[8], 0);
    a.runtime
        .memory
        .map(GuestPtr(0x53000000), vec![0xa0, 0x90], Permissions::RX)
        .unwrap();
    a.runtime.cpu = Cpu::new(Architecture::M68k, 0x53000000, STACK_BASE + 0x8000);
    if let Cpu::M68k(c) = &mut a.runtime.cpu {
        c.dar[0] = 2;
        c.dar[8] = PORT + 920;
    }
    assert_eq!(a.runtime.advance(1).unwrap(), Stop::Budget);
    assert_eq!(
        a.runtime.memory.read(GuestPtr(PORT + 920), 16).unwrap(),
        expected
    );
}

#[test]
#[ignore = "requires a real wgpu adapter"]
fn visible_color_windows_use_main_screen_and_repaint_actual_colors_with_offscreen_device_selected()
{
    let mut a = setup(240, 180);
    invoke(&mut a, 12, &[]);
    invoke(&mut a, 9, &[]);
    invoke(&mut a, 13, &[]);
    let pm = main_map(&a);
    let mut map = a.runtime.memory.read(GuestPtr(pm), 50).unwrap().to_vec();
    support::w32(&mut map, 0, 0x54000000);
    support::w16(&mut map, 4, 0x8010);
    for (i, v) in [0, 0, 16, 16].into_iter().enumerate() {
        support::w16(&mut map, 6 + i * 2, v);
    }
    a.runtime
        .memory
        .map(GuestPtr(0x54000000), vec![0; 256], Permissions::RW)
        .unwrap();
    let ph = a
        .runtime
        .process
        .new_handle(&mut a.runtime.memory, 50, false)
        .unwrap();
    let pd = a.runtime.memory.u32(GuestPtr(ph)).unwrap();
    a.runtime.memory.write(GuestPtr(pd), &map).unwrap();
    let main = a
        .runtime
        .memory
        .u32(systemless_future::devices::MAIN)
        .unwrap();
    let gd = a.runtime.memory.u32(GuestPtr(main)).unwrap();
    let mut device = a.runtime.memory.read(GuestPtr(gd), 62).unwrap().to_vec();
    support::w32(&mut device, 22, ph);
    support::w16(&mut device, 20, 0x4001);
    let dh = a
        .runtime
        .process
        .new_handle(&mut a.runtime.memory, 62, false)
        .unwrap();
    let dd = a.runtime.memory.u32(GuestPtr(dh)).unwrap();
    a.runtime.memory.write(GuestPtr(dd), &device).unwrap();
    invoke(&mut a, 17, &[dh]);
    rectangle(&mut a, PORT + 512, Rect::new(40, 60, 100, 110));
    a.runtime
        .memory
        .write(GuestPtr(PORT + 600), b"\x05color")
        .unwrap();
    let w = invoke(
        &mut a,
        16,
        &[0, PORT + 512, PORT + 600, 0, 2, u32::MAX, 0, 0],
    );
    assert_ne!(w, 0);
    let wh = a.runtime.memory.u32(GuestPtr(w + 2)).unwrap();
    let wp = a.runtime.memory.u32(GuestPtr(wh)).unwrap();
    assert_eq!(a.runtime.memory.u32(GuestPtr(wp)).unwrap(), SCREEN.0);
    assert_eq!(a.runtime.memory.u16(GuestPtr(wp + 4)).unwrap(), 0x80f0);
    assert_eq!(
        a.runtime
            .memory
            .u32(systemless_future::devices::CURRENT)
            .unwrap(),
        dh
    );
    invoke(&mut a, 5, &[w]);
    a.runtime
        .memory
        .write(GuestPtr(PORT + 700), &[0xff, 0xff, 0, 0, 0, 0])
        .unwrap();
    invoke(&mut a, 4, &[PORT + 700]);
    a.runtime
        .memory
        .write(GuestPtr(PORT + 700), &[0, 0, 0xff, 0xff, 0, 0])
        .unwrap();
    invoke(&mut a, 20, &[PORT + 700]);
    invoke(&mut a, 19, &[w]);
    assert_eq!(pixel(&a, 60, 80), 227);
    invoke(&mut a, 18, &[w]);
    invoke(&mut a, 19, &[w]);
    assert_eq!(pixel(&a, 60, 80), 227);
    invoke(&mut a, 11, &[w, 50, 70, 0]);
    assert_eq!(pixel(&a, 70, 90), 227);
    a.runtime.refresh_screen().unwrap();
    assert_eq!(rgba(&a, 70, 90), [0, 255, 0, 255]);
    assert_eq!(
        a.runtime
            .memory
            .u32(systemless_future::devices::CURRENT)
            .unwrap(),
        dh
    );
    assert_eq!(
        a.runtime.memory.read(GuestPtr(0x54000000), 256).unwrap(),
        vec![0; 256]
    );
}
