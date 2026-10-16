mod support;
use systemless_future::{
    cpu::{Architecture, Cpu},
    graphics::Graphics,
    load::{resource::ResourceFork, LoadedApplication, STACK_BASE},
    memory::{GuestPtr, Permissions},
    runtime::Stop,
};
fn invoke(app: &mut LoadedApplication, index: u32, args: &[u32]) -> u32 {
    let vector = app
        .runtime
        .memory
        .u32(GuestPtr(app.info.toc_or_a5 + 8 + index * 4))
        .unwrap();
    let pc = app.runtime.memory.u32(GuestPtr(vector)).unwrap();
    app.runtime.cpu = Cpu::new(Architecture::PowerPc, pc, STACK_BASE + 0x8000);
    if let Cpu::PowerPc(c) = &mut app.runtime.cpu {
        for (i, arg) in args.iter().enumerate() {
            c.gpr[3 + i] = *arg;
        }
    }
    assert_eq!(app.runtime.advance(1).unwrap(), Stop::Budget);
    let Cpu::PowerPc(c) = &app.runtime.cpu else {
        panic!()
    };
    c.gpr[3]
}
fn m68(app: &mut LoadedApplication, offset: u32, frame: &[u8], pop: u32, result: bool) -> u32 {
    let stack = STACK_BASE + 0x8000;
    let mut bytes = frame.to_vec();
    bytes.extend([0x5a; 6]);
    app.runtime.memory.write(GuestPtr(stack), &bytes).unwrap();
    app.runtime.cpu = Cpu::new(Architecture::M68k, 0x50000000 + offset, stack);
    assert_eq!(app.runtime.advance(1).unwrap(), Stop::Budget);
    let Cpu::M68k(c) = &app.runtime.cpu else {
        panic!()
    };
    assert_eq!(c.dar[15], stack + pop);
    if result {
        assert_eq!(
            app.runtime.memory.u16(GuestPtr(stack + pop + 4)).unwrap(),
            0x5a5a
        );
        app.runtime.memory.u32(GuestPtr(stack + pop)).unwrap()
    } else {
        0
    }
}

fn fixtures(kind: u8) -> Vec<u8> {
    let mut dlog = vec![0; 24];
    for (at, v) in [
        (0, 40),
        (2, 40),
        (4, 160),
        (6, 280),
        (8, 1),
        (18, 128),
        (22, 0x280a),
    ] {
        support::w16(&mut dlog, at, v);
    }
    dlog[10] = 1;
    dlog[21] = 0xa5; // Pascal title padding is unspecified.
    let mut ditl = 1u16.to_be_bytes().to_vec();
    for (k, data, rect) in [
        (4, b"OK".as_slice(), [80i16, 150, 100, 210]),
        (kind, b"Shared dialog".as_slice(), [10i16, 10, 40, 220]),
    ] {
        ditl.extend([0; 4]);
        for v in rect {
            ditl.extend(v.to_be_bytes());
        }
        ditl.push(k);
        ditl.push(data.len() as u8);
        ditl.extend(data);
        if !ditl.len().is_multiple_of(2) {
            ditl.push(0xa5);
        }
    }
    if kind & 0x7f == 16 {
        ditl[..2].copy_from_slice(&2u16.to_be_bytes());
        ditl.extend([0; 4]);
        for v in [45i16, 10, 65, 220] {
            ditl.extend(v.to_be_bytes());
        }
        ditl.extend([0x90, 5]);
        ditl.extend(b"Other");
        if !ditl.len().is_multiple_of(2) {
            ditl.push(0xa5);
        }
    }
    let mut drawing_dlog = dlog.clone();
    support::w16(&mut drawing_dlog, 18, 129);
    let mut drawing_items = 3u16.to_be_bytes().to_vec();
    for (kind, data, rect) in [
        (4, b"OK".to_vec(), [80i16, 150, 100, 210]),
        (0x88, b"^0 ^1\rsecond line".to_vec(), [5, 5, 40, 125]),
        (0xc0, 500i16.to_be_bytes().to_vec(), [45, 10, 61, 26]),
        (0xa0, 501i16.to_be_bytes().to_vec(), [45, 40, 77, 72]),
    ] {
        drawing_items.extend([0; 4]);
        for v in rect {
            drawing_items.extend(v.to_be_bytes());
        }
        drawing_items.push(kind);
        drawing_items.push(data.len() as u8);
        drawing_items.extend(data);
        if !drawing_items.len().is_multiple_of(2) {
            drawing_items.push(0);
        }
    }
    let mut picture = vec![0, 0];
    for v in [0i16, 0, 2, 2] {
        picture.extend(v.to_be_bytes());
    }
    picture.extend([0x11, 1, 0x90, 0, 1]);
    for _ in 0..3 {
        for v in [0i16, 0, 2, 2] {
            picture.extend(v.to_be_bytes());
        }
    }
    picture.extend([0, 0, 0x80, 0x40, 0xff]);
    let n = picture.len() as u16;
    support::w16(&mut picture, 0, n);
    let icon = (0..128)
        .map(|i| if i % 4 == 0 { 0x80 } else { 0 })
        .collect();
    support::resource(vec![
        (*b"DLOG", 128, None, dlog),
        (*b"DITL", 128, None, ditl),
        (*b"DLOG", 129, None, drawing_dlog),
        (*b"DITL", 129, None, drawing_items),
        (*b"PICT", 500, None, picture),
        (*b"ICON", 501, None, icon),
    ])
}
fn setup(kind: u8) -> LoadedApplication {
    let f = support::pef_program(
        &[0x4e800020],
        &[
            b"InitGraf",
            b"InitWindows",
            b"GetNewDialog",
            b"GetDialogItem",
            b"CloseDialog",
            b"DisposeDialog",
            b"InitFonts",
            b"SetDialogDefaultItem",
            b"ShowWindow",
            b"HideWindow",
            b"DrawDialog",
            b"ParamText",
            b"SetDialogItemText",
            b"GetDialogItemText",
            b"SelectDialogItemText",
            b"SelIText",
            b"TEKey",
        ],
        512,
    );
    let mut a = LoadedApplication::pef(&f.bytes, None).unwrap();
    a.runtime
        .process
        .install_application_resources(ResourceFork::parse(&fixtures(kind)).unwrap());
    a.runtime
        .memory
        .map(
            GuestPtr(0x50000000),
            vec![
                0xa9, 0x7c, 0xa9, 0x8d, 0xa9, 0x82, 0xa9, 0x83, 0xaa, 0x68, 0xa9, 0x81, 0xa9, 0x8f,
                0xa9, 0x90, 0xa9, 0x7e,
            ],
            Permissions::RX,
        )
        .unwrap();
    a.runtime
        .memory
        .map(GuestPtr(0x51000000), vec![0x5a; 256], Permissions::RW)
        .unwrap();
    let data = a.info.toc_or_a5;
    invoke(&mut a, 0, &[data + 330]);
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
    let mut gpu =
        pollster::block_on(Graphics::new(&instance, None, 512, 342, GuestPtr(0x4000))).unwrap();
    gpu.palette[0] = [255; 4];
    gpu.palette[1] = [0, 0, 0, 255];
    a.runtime.graphics = Some(gpu);
    invoke(&mut a, 6, &[]);
    invoke(&mut a, 1, &[]);
    a
}
fn handle_data(a: &LoadedApplication, h: u32, n: usize) -> Vec<u8> {
    let ptr = a.runtime.memory.u32(GuestPtr(h)).unwrap();
    a.runtime.memory.read(GuestPtr(ptr), n).unwrap().to_vec()
}
fn item(a: &mut LoadedApplication, w: u32, number: u32, native: bool) -> (u16, u32, Vec<u8>) {
    let base = 0x51000000u32;
    if native {
        let mut f = (base + 8).to_be_bytes().to_vec();
        f.extend((base + 4).to_be_bytes());
        f.extend(base.to_be_bytes());
        f.extend((number as i16).to_be_bytes());
        f.extend(w.to_be_bytes());
        m68(a, 2, &f, 18, false);
    } else {
        invoke(a, 3, &[w, number, base, base + 4, base + 8]);
    }
    (
        a.runtime.memory.u16(GuestPtr(base)).unwrap(),
        a.runtime.memory.u32(GuestPtr(base + 4)).unwrap(),
        a.runtime
            .memory
            .read(GuestPtr(base + 8), 8)
            .unwrap()
            .to_vec(),
    )
}
#[test]
#[ignore = "requires a real wgpu adapter"]
fn both_cpus_share_real_dialog_records_independent_items_and_cleanup() {
    let mut a = setup(0x88);
    let slot = a.runtime.process.ports.unwrap().current_slot;
    let previous = a.runtime.memory.u32(slot).unwrap();
    let source = a
        .runtime
        .process
        .get_resource(&mut a.runtime.memory, *b"DITL", 128)
        .unwrap();
    let original = handle_data(&a, source, 46);
    let mut f = u32::MAX.to_be_bytes().to_vec();
    f.extend(0u32.to_be_bytes());
    f.extend(128i16.to_be_bytes());
    let first = m68(&mut a, 0, &f, 10, true);
    assert_ne!(first, 0);
    let second = invoke(&mut a, 2, &[128, 0, u32::MAX]);
    assert_ne!(second, 0);
    assert_ne!(first, second);
    assert_eq!(a.runtime.memory.u32(slot).unwrap(), previous);
    for w in [first, second] {
        assert_eq!(a.runtime.memory.u16(GuestPtr(w + 108)).unwrap(), 2);
        assert_eq!(a.runtime.memory.u16(GuestPtr(w + 164)).unwrap(), 0xffff);
        assert_eq!(a.runtime.memory.u16(GuestPtr(w + 168)).unwrap(), 1);
        assert_eq!(a.runtime.process.pointer_size(w), 170);
        assert_ne!(a.runtime.memory.u32(GuestPtr(w + 156)).unwrap(), source);
    }
    let (kind, button, _) = item(&mut a, first, 1, true);
    assert_eq!(kind, 4);
    assert_ne!(button, 0);
    let (kind, text, rect) = item(&mut a, first, 2, false);
    assert_eq!(kind, 0x88);
    assert_eq!(handle_data(&a, text, 13), b"Shared dialog");
    assert_eq!(rect, [0, 10, 0, 10, 0, 40, 0, 220]);
    let (_, other, _) = item(&mut a, second, 2, true);
    assert_ne!(text, other);
    let ptr = a.runtime.memory.u32(GuestPtr(text)).unwrap();
    a.runtime.memory.write(GuestPtr(ptr), b"X").unwrap();
    assert_eq!(handle_data(&a, other, 13), b"Shared dialog");
    assert_eq!(handle_data(&a, source, 46), original);
    invoke(&mut a, 4, &[first]);
    assert!(!a
        .runtime
        .process
        .windows
        .contains(&a.runtime.memory, first)
        .unwrap());
    assert_eq!(a.runtime.process.pointer_size(first), 170);
    assert_eq!(
        a.runtime.memory.read(GuestPtr(first + 156), 14).unwrap(),
        [0; 14]
    );
    m68(&mut a, 6, &first.to_be_bytes(), 4, false);
    assert_eq!(a.runtime.process.pointer_size(first) as i32, -111);
    invoke(&mut a, 5, &[second]);
    assert_eq!(a.runtime.process.pointer_size(second) as i32, -111);
    assert_eq!(handle_data(&a, source, 46), original);
    // Caller storage has exactly the documented size; guard bytes survive CloseDialog.
    let storage = 0x51000020;
    assert_eq!(invoke(&mut a, 2, &[128, storage, u32::MAX]), storage);
    m68(&mut a, 4, &storage.to_be_bytes(), 4, false);
    assert_eq!(
        a.runtime.memory.read(GuestPtr(storage + 170), 8).unwrap(),
        [0x5a; 8]
    );
    let free = a.runtime.process.free_memory();
    let held = a
        .runtime
        .process
        .new_pointer(&mut a.runtime.memory, free - 128, false)
        .unwrap();
    assert_ne!(held, 0);
    assert_eq!(invoke(&mut a, 2, &[128, 0, u32::MAX]), 0);
    assert_eq!(a.runtime.memory.u16(GuestPtr(0x220)).unwrap() as i16, -108);
    assert_eq!(a.runtime.process.free_memory(), 128);
    a.runtime.process.dispose_pointer(held);
    assert_eq!(a.runtime.process.free_memory(), free);
    assert_eq!(invoke(&mut a, 2, &[999, 0, u32::MAX]), 0);
    assert_eq!(a.runtime.memory.u16(GuestPtr(0xa60)).unwrap() as i16, -192);
}
#[test]
#[ignore = "requires a real wgpu adapter"]
fn unsupported_items_fail_before_publishing_a_window() {
    let mut a = setup(7);
    let before = a.runtime.memory.u32(GuestPtr(0x9d6)).unwrap();
    let slot = a.runtime.process.ports.unwrap().current_slot;
    let port = a.runtime.memory.u32(slot).unwrap();
    let tv = a
        .runtime
        .memory
        .u32(GuestPtr(a.info.toc_or_a5 + 16))
        .unwrap();
    let pc = a.runtime.memory.u32(GuestPtr(tv)).unwrap();
    a.runtime.cpu = Cpu::new(Architecture::PowerPc, pc, STACK_BASE + 0x8000);
    if let Cpu::PowerPc(c) = &mut a.runtime.cpu {
        c.gpr[3] = 128;
        c.gpr[4] = 0;
        c.gpr[5] = u32::MAX;
    }
    assert!(a
        .runtime
        .advance(1)
        .unwrap_err()
        .to_string()
        .contains("unsupported dialog item"));
    assert_eq!(a.runtime.memory.u32(GuestPtr(0x9d6)).unwrap(), before);
    assert_eq!(a.runtime.memory.u32(slot).unwrap(), port);
}

#[test]
fn native_dialog_results_and_outputs_are_preflighted_before_stack_changes() {
    use systemless_future::{abi::Abi, memory::Memory};
    for (len, permissions) in [(13, Permissions::RW), (14, Permissions::RX)] {
        let mut m = Memory::default();
        m.map(GuestPtr(0x1000), vec![0; len], permissions).unwrap();
        let mut cpu = Cpu::new(Architecture::M68k, 0, 0x1000);
        let Cpu::M68k(c) = &mut cpu else {
            unreachable!()
        };
        assert!(Abi::trap(0xa97c, c, &m).is_err());
        assert_eq!(c.dar[15], 0x1000);
    }
    let mut m = Memory::default();
    let mut frame = vec![0; 18];
    support::w32(&mut frame, 0, 0x2000);
    support::w32(&mut frame, 4, 0x3000);
    support::w32(&mut frame, 8, 0x4000);
    m.map(GuestPtr(0x1000), frame, Permissions::RW).unwrap();
    m.map(GuestPtr(0x2000), vec![0x5a; 8], Permissions::RW)
        .unwrap();
    m.map(GuestPtr(0x3000), vec![0; 4], Permissions::RW)
        .unwrap();
    m.map(GuestPtr(0x4000), vec![0; 2], Permissions::RX)
        .unwrap();
    let mut cpu = Cpu::new(Architecture::M68k, 0, 0x1000);
    let Cpu::M68k(c) = &mut cpu else {
        unreachable!()
    };
    assert!(Abi::trap(0xa98d, c, &m).is_err());
    assert_eq!(c.dar[15], 0x1000);
    assert_eq!(m.read(GuestPtr(0x2000), 8).unwrap(), [0x5a; 8]);
}

fn native_default(a: &mut LoadedApplication, w: u32, item: i16) -> i16 {
    let sp = STACK_BASE + 0x8000;
    let mut frame = item.to_be_bytes().to_vec();
    frame.extend(w.to_be_bytes());
    frame.extend([0xa5; 6]);
    a.runtime.memory.write(GuestPtr(sp), &frame).unwrap();
    a.runtime.cpu = Cpu::new(Architecture::M68k, 0x50000008, sp);
    if let Cpu::M68k(c) = &mut a.runtime.cpu {
        c.dar[0] = 0xbeef0304;
    }
    assert_eq!(a.runtime.advance(1).unwrap(), Stop::Budget);
    let Cpu::M68k(c) = &a.runtime.cpu else {
        panic!()
    };
    assert_eq!(c.dar[15], sp + 6);
    assert_eq!(a.runtime.memory.u32(GuestPtr(sp + 8)).unwrap(), 0xa5a5a5a5);
    a.runtime.memory.u16(GuestPtr(sp + 6)).unwrap() as i16
}
fn screen(a: &LoadedApplication) -> Vec<u8> {
    a.runtime
        .memory
        .read(systemless_future::quickdraw::SCREEN, 64 * 342)
        .unwrap()
        .to_vec()
}
fn at(a: &LoadedApplication, w: u32, x: i32, y: i32) -> bool {
    let x = x - i32::from(a.runtime.memory.u16(GuestPtr(w + 10)).unwrap() as i16);
    let y = y - i32::from(a.runtime.memory.u16(GuestPtr(w + 8)).unwrap() as i16);
    let bytes = screen(a);
    bytes[y as usize * 64 + x as usize / 8] & (0x80 >> (x as usize % 8)) != 0
}
#[test]
#[ignore = "requires a real wgpu adapter"]
fn default_button_has_real_outline_shared_state_and_preserved_ports_errors_and_hidden_lifetime() {
    let mut a = setup(0x88);
    let w = invoke(&mut a, 2, &[128, 0, u32::MAX]);
    let slot = a.runtime.process.ports.unwrap().current_slot;
    // Nondefault pen state must not influence the standard outline or leak changes.
    a.runtime
        .memory
        .write(GuestPtr(w + 52), &[0, 5, 0, 7, 0, 10])
        .unwrap();
    a.runtime
        .memory
        .write(GuestPtr(w + 58), &[0x55; 8])
        .unwrap();
    let port = a.runtime.memory.read(GuestPtr(w), 108).unwrap().to_vec();
    let previous = a.runtime.memory.u32(slot).unwrap();
    let baseline = screen(&a);
    a.runtime.process.mem_error = -108;
    a.runtime.process.res_error = -192;
    a.runtime
        .memory
        .write(GuestPtr(0x220), &(-44i16).to_be_bytes())
        .unwrap();
    a.runtime
        .memory
        .write(GuestPtr(0xa60), &(-43i16).to_be_bytes())
        .unwrap();
    assert_eq!(native_default(&mut a, w, 1), 0);
    assert_eq!(a.runtime.memory.u16(GuestPtr(w + 168)).unwrap(), 1);
    let outlined = screen(&a);
    assert_ne!(outlined, baseline);
    for y in 76..79 {
        assert!(at(&a, w, 180, y));
    }
    assert!(!at(&a, w, 180, 79));
    assert!(at(&a, w, 180, 80));
    for x in 146..149 {
        assert!(at(&a, w, x, 90));
    }
    assert!(!at(&a, w, 149, 90));
    assert_eq!(a.runtime.memory.read(GuestPtr(w), 108).unwrap(), port);
    assert_eq!(a.runtime.memory.u32(slot).unwrap(), previous);
    assert_eq!(
        (a.runtime.process.mem_error, a.runtime.process.res_error),
        (-108, -192)
    );
    assert_eq!(a.runtime.memory.u16(GuestPtr(0x220)).unwrap() as i16, -44);
    assert_eq!(a.runtime.memory.u16(GuestPtr(0xa60)).unwrap() as i16, -43);
    assert_eq!(invoke(&mut a, 7, &[w, 2]) as i32, -50);
    assert_eq!(screen(&a), outlined);
    assert_eq!(invoke(&mut a, 7, &[0, 1]) as i32, -50);
    assert_eq!(invoke(&mut a, 7, &[w, 0]), 0);
    assert_eq!(screen(&a), baseline);
    assert_eq!(native_default(&mut a, w, 1), 0);
    assert_eq!(screen(&a), outlined);
    invoke(&mut a, 9, &[w]);
    let hidden = screen(&a);
    let gpu = a.runtime.graphics.take();
    assert_eq!(invoke(&mut a, 7, &[w, 0]), 0);
    assert_eq!(invoke(&mut a, 7, &[w, 1]), 0);
    assert_eq!(screen(&a), hidden);
    a.runtime.graphics = gpu;
    invoke(&mut a, 8, &[w]);
    assert_eq!(screen(&a), outlined);
    // GPU readback must agree with guest bitmap at a straight outline edge.
    let rgba = a.runtime.graphics.as_ref().unwrap().read_rgba().unwrap();
    let x = 180 - i32::from(a.runtime.memory.u16(GuestPtr(w + 10)).unwrap() as i16);
    let y = 76 - i32::from(a.runtime.memory.u16(GuestPtr(w + 8)).unwrap() as i16);
    let offset = (y as usize * 512 + x as usize) * 4;
    assert_eq!(&rgba[offset..offset + 4], &[0, 0, 0, 255]);
    invoke(&mut a, 5, &[w]);
    assert_eq!(invoke(&mut a, 7, &[w, 1]) as i32, -50);
}
#[test]
fn native_default_dispatch_rejects_bad_selector_or_result_before_popping() {
    use systemless_future::{abi::Abi, memory::Memory};
    for selector in [0x0304, 0x0204, 0x0305] {
        let mut m = Memory::default();
        m.map(GuestPtr(0x1000), vec![0; 8], Permissions::RX)
            .unwrap();
        let mut cpu = Cpu::new(Architecture::M68k, 0, 0x1000);
        let Cpu::M68k(c) = &mut cpu else { panic!() };
        c.dar[0] = selector;
        assert!(Abi::trap(0xaa68, c, &m).is_err());
        assert_eq!(c.dar[15], 0x1000);
    }
}

#[test]
#[ignore = "requires a real wgpu adapter"]
fn changing_the_default_button_erases_the_previous_outline_without_erasing_the_button() {
    let mut a = setup(4);
    let w = invoke(&mut a, 2, &[128, 0, u32::MAX]);
    let baseline = screen(&a);
    assert_eq!(native_default(&mut a, w, 1), 0);
    assert!(at(&a, w, 180, 76));
    assert!(!at(&a, w, 100, 6));
    assert_eq!(invoke(&mut a, 7, &[w, 2]), 0);
    assert!(!at(&a, w, 180, 76));
    assert!(at(&a, w, 180, 80));
    assert!(at(&a, w, 100, 6));
    assert_eq!(a.runtime.memory.u16(GuestPtr(w + 168)).unwrap(), 2);
    assert_eq!(native_default(&mut a, w, 0), 0);
    assert_eq!(screen(&a), baseline);
    invoke(&mut a, 5, &[w]);
}

#[test]
#[ignore = "requires a real wgpu adapter"]
fn draw_dialog_uses_live_text_parameters_icons_pictures_and_both_cpu_void_routes() {
    let mut a = setup(0x88);
    let w = invoke(&mut a, 2, &[129, 0, u32::MAX]);
    a.runtime
        .memory
        .write(GuestPtr(0x51000000), b"\x01A")
        .unwrap();
    a.runtime
        .memory
        .write(GuestPtr(0x51000008), b"\x01B")
        .unwrap();
    invoke(&mut a, 11, &[0x51000000, 0x51000008, 0, 0]);
    let (_, h, _) = item(&mut a, w, 2, false);
    let before = handle_data(&a, h, 17);
    let slot = a.runtime.process.ports.unwrap().current_slot;
    let previous = a.runtime.memory.u32(slot).unwrap();
    let port = a.runtime.memory.read(GuestPtr(w), 108).unwrap().to_vec();
    let baseline = screen(&a);
    a.runtime.process.mem_error = -51;
    a.runtime.process.res_error = -52;
    assert_eq!(invoke(&mut a, 10, &[w]), w);
    let drawn = screen(&a);
    assert_ne!(drawn, baseline);
    let rgba = a.runtime.graphics.as_ref().unwrap().read_rgba().unwrap();
    for (i, pixel) in rgba.as_chunks::<4>().0.iter().enumerate() {
        let x = i % 512;
        let y = i / 512;
        let black = drawn[y * 64 + x / 8] & (0x80 >> (x % 8)) != 0;
        assert_eq!(
            pixel,
            if black { &[0, 0, 0, 255] } else { &[255; 4] },
            "GPU pixel {x},{y}"
        );
    }
    assert!(at(&a, w, 12, 47));
    assert!(!at(&a, w, 22, 47));
    assert!(at(&a, w, 22, 57));
    assert!(at(&a, w, 40, 50));
    assert!(!at(&a, w, 41, 50));
    assert!((5..40).any(|y| (5..125).any(|x| at(&a, w, x, y))));
    assert_eq!(a.runtime.memory.u32(slot).unwrap(), previous);
    assert_eq!(a.runtime.memory.read(GuestPtr(w), 108).unwrap(), port);
    assert_eq!(
        (a.runtime.process.mem_error, a.runtime.process.res_error),
        (-51, -52)
    );
    assert_eq!(handle_data(&a, h, 17), before);
    invoke(&mut a, 9, &[w]);
    invoke(&mut a, 8, &[w]);
    // Window visibility resizes update regions; measure only DrawDialog's lifetime.
    let free = a.runtime.process.free_memory();
    m68(&mut a, 10, &w.to_be_bytes(), 4, false);
    assert_eq!(screen(&a), drawn);
    assert_eq!(a.runtime.process.free_memory(), free);
    invoke(&mut a, 9, &[w]);
    let hidden = screen(&a);
    let gpu = a.runtime.graphics.take();
    invoke(&mut a, 10, &[w]);
    assert_eq!(screen(&a), hidden);
    a.runtime.graphics = gpu;
    invoke(&mut a, 8, &[w]);
    // An invalid visible picture must fail before the earlier text item is painted.
    let picture = a
        .runtime
        .process
        .get_resource(&mut a.runtime.memory, *b"PICT", 500)
        .unwrap();
    let ptr = a.runtime.memory.u32(GuestPtr(picture)).unwrap();
    a.runtime.memory.write(GuestPtr(ptr + 12), &[0xee]).unwrap();
    let blank = screen(&a);
    let free = a.runtime.process.free_memory();
    let tv = a
        .runtime
        .memory
        .u32(GuestPtr(a.info.toc_or_a5 + 8 + 10 * 4))
        .unwrap();
    let pc = a.runtime.memory.u32(GuestPtr(tv)).unwrap();
    a.runtime.cpu = Cpu::new(Architecture::PowerPc, pc, STACK_BASE + 0x8000);
    if let Cpu::PowerPc(c) = &mut a.runtime.cpu {
        c.gpr[3] = w;
    }
    assert!(a
        .runtime
        .advance(1)
        .unwrap_err()
        .to_string()
        .contains("PICT"));
    assert_eq!(screen(&a), blank);
    assert_eq!(a.runtime.process.free_memory(), free);
    assert_eq!(a.runtime.memory.u32(slot).unwrap(), previous);
    // Runtime faults are terminal. Verify manager restoration and ownership through
    // the shared service directly rather than attempting to resume a stopped CPU.
    let mut dialogs = std::mem::take(&mut a.runtime.process.dialogs);
    dialogs
        .dispose(
            &mut a.runtime.memory,
            &mut a.runtime.process,
            a.runtime.graphics.as_ref().unwrap(),
            w,
            false,
        )
        .unwrap();
    a.runtime.process.dialogs = dialogs;
}

#[test]
#[ignore = "requires a real wgpu adapter"]
fn item_text_replacement_redraws_only_the_text_item_and_shares_live_payloads_on_both_cpus() {
    let mut a = setup(0x88);
    a.runtime
        .memory
        .map(GuestPtr(0x52000000), vec![0xcc; 258], Permissions::RW)
        .unwrap();
    let w = invoke(&mut a, 2, &[129, 0, u32::MAX]);
    invoke(&mut a, 10, &[w]);
    let (_, h, _) = item(&mut a, w, 2, false);
    let before = screen(&a);
    let slot = a.runtime.process.ports.unwrap().current_slot;
    let previous = a.runtime.memory.u32(slot).unwrap();
    let port = a.runtime.memory.read(GuestPtr(w), 108).unwrap().to_vec();
    let input = b"\x0aMMMMMMMMMM";
    a.runtime.memory.write(GuestPtr(0x51000000), input).unwrap();
    a.runtime.process.res_error = -52;
    let mut frame = 0x51000000u32.to_be_bytes().to_vec();
    frame.extend(h.to_be_bytes());
    m68(&mut a, 12, &frame, 8, false);
    assert_eq!(a.runtime.memory.u16(GuestPtr(0x220)).unwrap(), 0);
    assert_eq!(a.runtime.process.res_error, -52);
    let after = screen(&a);
    assert_ne!(after, before);
    let gx = -i32::from(a.runtime.memory.u16(GuestPtr(w + 10)).unwrap() as i16);
    let gy = -i32::from(a.runtime.memory.u16(GuestPtr(w + 8)).unwrap() as i16);
    for y in 0..342 {
        for x in 0..512 {
            if x >= gx + 5 && x < gx + 125 && y >= gy + 5 && y < gy + 40 {
                continue;
            }
            let index = y as usize * 64 + x as usize / 8;
            let mask = 0x80 >> (x as usize % 8);
            assert_eq!(
                after[index] & mask,
                before[index] & mask,
                "outside text {x},{y}"
            );
        }
    }
    assert_eq!(invoke(&mut a, 13, &[h, 0x52000001]), h);
    assert_eq!(
        a.runtime.memory.read(GuestPtr(0x52000001), 11).unwrap(),
        input
    );
    assert_eq!(
        a.runtime.memory.read(GuestPtr(0x52000000), 1).unwrap(),
        [0xcc]
    );
    assert_eq!(
        a.runtime.memory.read(GuestPtr(0x5200000c), 1).unwrap(),
        [0xcc]
    );
    a.runtime.memory.write(GuestPtr(0x51000000), &[0]).unwrap();
    assert_eq!(invoke(&mut a, 12, &[h, 0x51000000]), h);
    assert!((5..40).all(|y| (5..125).all(|x| !at(&a, w, x, y))));
    assert_eq!(a.runtime.memory.read(GuestPtr(w), 108).unwrap(), port);
    assert_eq!(a.runtime.memory.u32(slot).unwrap(), previous);
    let mut frame = 0x52000001u32.to_be_bytes().to_vec();
    frame.extend(h.to_be_bytes());
    m68(&mut a, 14, &frame, 8, false);
    assert_eq!(
        a.runtime.memory.read(GuestPtr(0x52000001), 2).unwrap(),
        [0, b'M']
    );
    invoke(&mut a, 9, &[w]);
    let hidden = screen(&a);
    let gpu = a.runtime.graphics.take();
    a.runtime
        .memory
        .write(GuestPtr(0x51000000), b"\x01A")
        .unwrap();
    invoke(&mut a, 12, &[h, 0x51000000]);
    assert_eq!(screen(&a), hidden);
    invoke(&mut a, 13, &[h, 0x52000001]);
    assert_eq!(
        a.runtime.memory.read(GuestPtr(0x52000001), 2).unwrap(),
        b"\x01A"
    );
    a.runtime.graphics = gpu;
    invoke(&mut a, 8, &[w]);
    invoke(&mut a, 10, &[w]);
    assert!((5..40).any(|y| (5..125).any(|x| at(&a, w, x, y))));
    invoke(&mut a, 5, &[w]);
}
#[test]
fn item_text_native_frames_validate_initial_output_and_input_before_popping() {
    use systemless_future::{abi::Abi, memory::Memory};
    for opcode in [0xa98f, 0xa990] {
        let mut m = Memory::default();
        let mut frame = 0x2000u32.to_be_bytes().to_vec();
        frame.extend(0x3000u32.to_be_bytes());
        m.map(GuestPtr(0x1000), frame, Permissions::RW).unwrap();
        m.map(GuestPtr(0x2000), vec![255], Permissions::RX).unwrap();
        let mut cpu = Cpu::new(Architecture::M68k, 0, 0x1000);
        let Cpu::M68k(c) = &mut cpu else { panic!() };
        assert!(Abi::trap(opcode, c, &m).is_err());
        assert_eq!(c.dar[15], 0x1000);
    }
}

#[test]
#[ignore = "requires a real wgpu adapter"]
fn editable_items_share_one_textedit_record_with_canonical_text_and_gpu_selection() {
    let mut a = setup(16);
    let slot = a.runtime.process.ports.unwrap().current_slot;
    let previous = a.runtime.memory.u32(slot).unwrap();
    let first = invoke(&mut a, 2, &[128, 0, u32::MAX]);
    let mut frame = u32::MAX.to_be_bytes().to_vec();
    frame.extend(0u32.to_be_bytes());
    frame.extend(128i16.to_be_bytes());
    let second = m68(&mut a, 0, &frame, 10, true);
    assert_ne!(first, 0);
    assert_ne!(second, 0);
    let (_, text, _) = item(&mut a, first, 2, false);
    let (_, other, _) = item(&mut a, first, 3, true);
    let (_, independent, _) = item(&mut a, second, 2, true);
    let editor = a.runtime.memory.u32(GuestPtr(first + 160)).unwrap();
    let second_editor = a.runtime.memory.u32(GuestPtr(second + 160)).unwrap();
    assert_ne!(editor, second_editor);
    assert_eq!(a.runtime.memory.u16(GuestPtr(first + 164)).unwrap(), 1);
    assert_eq!(handle_data(&a, editor, 66)[62..66], text.to_be_bytes());
    assert_eq!(handle_data(&a, independent, 13), b"Shared dialog");
    invoke(&mut a, 9, &[second]); // expose first dialog
    let before = screen(&a);
    invoke(&mut a, 14, &[first, 2, 0, 32767]);
    let selected = screen(&a);
    assert_ne!(selected, before);
    assert_eq!(&handle_data(&a, editor, 38)[32..38], &[0, 0, 0, 13, 0, 1]);
    assert_eq!(a.runtime.memory.u32(slot).unwrap(), previous);
    // Both TEKey and Dialog Manager access the exact same guest text handle.
    invoke(&mut a, 16, &[u32::from(b'Z'), editor]);
    assert_eq!(handle_data(&a, text, 1), b"Z");
    invoke(&mut a, 13, &[text, 0x51000080]);
    assert_eq!(
        a.runtime.memory.read(GuestPtr(0x51000080), 2).unwrap(),
        b"\x01Z"
    );
    // Select another disabled editText: disabled suppresses item reports, not editing.
    let mut f = 32767i16.to_be_bytes().to_vec();
    f.extend(0i16.to_be_bytes());
    f.extend(3i16.to_be_bytes());
    f.extend(first.to_be_bytes());
    m68(&mut a, 16, &f, 10, false);
    assert_eq!(a.runtime.memory.u32(GuestPtr(first + 160)).unwrap(), editor);
    assert_eq!(a.runtime.memory.u16(GuestPtr(first + 164)).unwrap(), 2);
    assert_eq!(&handle_data(&a, editor, 66)[62..66], &other.to_be_bytes());
    assert_eq!(&handle_data(&a, editor, 32)[28..32], &[0, 45, 0, 10]);
    assert_eq!(handle_data(&a, text, 1), b"Z");
    a.runtime
        .memory
        .write(GuestPtr(0x51000080), b"\x03New")
        .unwrap();
    invoke(&mut a, 12, &[other, 0x51000080]);
    assert_eq!(handle_data(&a, other, 3), b"New");
    assert_eq!(&handle_data(&a, editor, 62)[60..62], &[0, 3]);
    invoke(&mut a, 15, &[first, 2, 0, 1]);
    assert_eq!(&handle_data(&a, editor, 66)[62..66], &text.to_be_bytes());
    // Hiding an active field must still permit switching with the available GPU.
    invoke(&mut a, 9, &[first]);
    invoke(&mut a, 14, &[first, 3, 0, 3]);
    assert_eq!(a.runtime.memory.u16(GuestPtr(first + 164)).unwrap(), 2);
    // Edits and selection are per-dialog; borrowed item text is not double-freed.
    assert_eq!(handle_data(&a, independent, 13), b"Shared dialog");
    invoke(&mut a, 4, &[first]);
    for h in [editor, text, other] {
        a.runtime.process.handle_size(&a.runtime.memory, h).unwrap();
        assert_ne!(a.runtime.process.mem_error, 0);
    }
    assert_eq!(handle_data(&a, independent, 13), b"Shared dialog");
    invoke(&mut a, 5, &[first]);
    invoke(&mut a, 5, &[second]);
}

#[test]
#[ignore = "requires a real wgpu adapter"]
fn failed_edit_field_rebind_restores_textedit_and_dialog_ownership() {
    let mut a = setup(0x90);
    let w = invoke(&mut a, 2, &[128, 0, u32::MAX]);
    invoke(&mut a, 9, &[w]);
    let editor = a.runtime.memory.u32(GuestPtr(w + 160)).unwrap();
    let te = a.runtime.memory.u32(GuestPtr(editor)).unwrap();
    a.runtime
        .memory
        .write(GuestPtr(te + 38), &1u32.to_be_bytes())
        .unwrap();
    let before = handle_data(&a, editor, 128);
    let state = a
        .runtime
        .memory
        .read(GuestPtr(w + 160), 8)
        .unwrap()
        .to_vec();
    let (_, old, _) = item(&mut a, w, 2, false);
    let (_, next, _) = item(&mut a, w, 3, true);
    let old_bytes = handle_data(&a, old, 13);
    let next_bytes = handle_data(&a, next, 5);
    let mut manager = std::mem::take(&mut a.runtime.process.dialogs);
    let err = manager
        .select_text(
            &mut a.runtime.memory,
            &mut a.runtime.process,
            None,
            w,
            3,
            (0, 32767),
        )
        .unwrap_err();
    assert!(err.to_string().contains("layout callbacks"));
    assert_eq!(handle_data(&a, editor, 128), before);
    assert_eq!(a.runtime.memory.read(GuestPtr(w + 160), 8).unwrap(), state);
    assert_eq!(handle_data(&a, old, 13), old_bytes);
    assert_eq!(handle_data(&a, next, 5), next_bytes);
    let te = a.runtime.memory.u32(GuestPtr(editor)).unwrap();
    a.runtime.memory.write(GuestPtr(te + 38), &[0; 4]).unwrap();
    manager
        .select_text(
            &mut a.runtime.memory,
            &mut a.runtime.process,
            None,
            w,
            3,
            (0, 32767),
        )
        .unwrap();
    assert_eq!(&handle_data(&a, editor, 66)[62..66], &next.to_be_bytes());
    manager
        .dispose(
            &mut a.runtime.memory,
            &mut a.runtime.process,
            a.runtime.graphics.as_ref().unwrap(),
            w,
            false,
        )
        .unwrap();
    a.runtime.process.dialogs = manager;
}

#[test]
fn native_edit_selection_frame_is_read_before_pop_and_keeps_void_registers() {
    use systemless_future::{abi::Abi, cpu::Call, dialogs::Op, memory::Memory};
    let mut m = Memory::default();
    m.map(GuestPtr(0x1000), vec![0; 9], Permissions::RW)
        .unwrap();
    let mut cpu = Cpu::new(Architecture::M68k, 0, 0x1000);
    let Cpu::M68k(c) = &mut cpu else {
        unreachable!()
    };
    c.dar[0] = 0xaabbccdd;
    assert!(Abi::trap(0xa97e, c, &m).is_err());
    assert_eq!(c.dar[15], 0x1000);
    assert_eq!(c.dar[0], 0xaabbccdd);
    let mut m = Memory::default();
    let mut f = 32767i16.to_be_bytes().to_vec();
    f.extend((-1i16).to_be_bytes());
    f.extend(5i16.to_be_bytes());
    f.extend(0x11223344u32.to_be_bytes());
    m.map(GuestPtr(0x1000), f, Permissions::RX).unwrap();
    let (call, reply) = Abi::trap(0xa97e, c, &m).unwrap();
    assert_eq!(
        call,
        Call::Dialog(Op::SelectText {
            dialog: 0x11223344,
            number: 5,
            start: -1,
            end: 32767
        })
    );
    reply.apply(&mut cpu, &mut m, 0, 0).unwrap();
    let Cpu::M68k(c) = &cpu else { unreachable!() };
    assert_eq!(c.dar[15], 0x100a);
    assert_eq!(c.dar[0], 0xaabbccdd);
}
