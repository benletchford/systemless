mod support;
use support::*;
use systemless_future::{
    cpu::{Architecture, Cpu},
    graphics::{Graphics, Rect},
    load::{LoadedApplication, STACK_BASE},
    memory::{GuestPtr, Permissions},
    quickdraw::SCREEN,
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
fn rect(app: &mut LoadedApplication, address: u32, top: i16, left: i16, bottom: i16, right: i16) {
    let bytes: Vec<_> = [top, left, bottom, right]
        .into_iter()
        .flat_map(i16::to_be_bytes)
        .collect();
    app.runtime.memory.write(GuestPtr(address), &bytes).unwrap();
}
fn region(app: &LoadedApplication, w: u32, offset: u32) -> Vec<Rect> {
    let handle = app.runtime.memory.u32(GuestPtr(w + offset)).unwrap();
    let data = GuestPtr(app.runtime.memory.u32(GuestPtr(handle)).unwrap());
    let size = app.runtime.memory.u16(data).unwrap();
    systemless_future::region_geometry::decode(
        app.runtime.memory.read(data, size as usize).unwrap(),
    )
    .unwrap()
}
fn contains(rs: &[Rect], x: i32, y: i32) -> bool {
    rs.iter()
        .any(|r| x >= r.left && x < r.right && y >= r.top && y < r.bottom)
}
fn pixel(app: &LoadedApplication, x: u32, y: u32) -> bool {
    app.runtime
        .memory
        .read(GuestPtr(SCREEN.0 + y * 64 + x / 8), 1)
        .unwrap()[0]
        & (0x80 >> (x % 8))
        != 0
}
#[test]
#[ignore = "requires a real wgpu adapter"]
fn windows_share_records_visibility_pixels_and_ownership_across_cpu_adapters() {
    let f = pef_program(
        &[0x4e800020],
        &[
            b"InitGraf",
            b"InitWindows",
            b"NewWindow",
            b"FrontWindow",
            b"CloseWindow",
            b"DisposeWindow",
        ],
        512,
    );
    let mut app = LoadedApplication::pef(&f.bytes, None).unwrap();
    let data = app.info.toc_or_a5;
    app.runtime
        .memory
        .map(
            GuestPtr(0x50000000),
            vec![0xa9, 0x13, 0xa9, 0x2d, 0xa9, 0x14, 0xa9, 0x24],
            Permissions::RX,
        )
        .unwrap();
    app.runtime
        .memory
        .map(GuestPtr(0x51000000), vec![0x5a; 160], Permissions::RW)
        .unwrap();
    invoke(&mut app, 0, &[data + 128 + 202]);
    let slot = app.runtime.process.ports.unwrap().current_slot;
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
    app.runtime.graphics = Some(
        pollster::block_on(Graphics::new(&instance, None, 512, 342, GuestPtr(0x4000))).unwrap(),
    );
    invoke(&mut app, 1, &[]);
    assert_ne!(app.runtime.memory.u32(GuestPtr(0x9de)).unwrap(), 0);
    assert_ne!(app.runtime.memory.u32(GuestPtr(0x9ee)).unwrap(), 0);
    assert_eq!(app.runtime.memory.u32(slot).unwrap(), 0);
    rect(&mut app, data + 64, 40, 40, 60, 60);
    app.runtime
        .memory
        .write(GuestPtr(data + 80), b"\x04test")
        .unwrap();
    let first = invoke(
        &mut app,
        2,
        &[0, data + 64, data + 80, 1, 2, u32::MAX, 0, 0x12345678],
    );
    assert_ne!(first, 0);
    assert_eq!(app.runtime.memory.u32(slot).unwrap(), first);
    assert_eq!(
        app.runtime.memory.u32(GuestPtr(first + 152)).unwrap(),
        0x12345678
    );
    assert_eq!(app.runtime.memory.u16(GuestPtr(first + 108)).unwrap(), 8);
    assert!(!pixel(&app, 40, 40));
    assert!(pixel(&app, 39, 40));
    rect(&mut app, data + 64, 50, 50, 70, 70);
    let mut frame = 0xabcdef01u32.to_be_bytes().to_vec();
    frame.extend([0, 0xa5]);
    frame.extend(u32::MAX.to_be_bytes());
    frame.extend(2i16.to_be_bytes());
    frame.extend([1, 0xa5]);
    frame.extend((data + 80).to_be_bytes());
    frame.extend((data + 64).to_be_bytes());
    frame.extend(0u32.to_be_bytes());
    let second = m68(&mut app, 0, &frame, 26, true);
    assert_ne!(second, first);
    assert_eq!(invoke(&mut app, 3, &[]), second);
    assert_eq!(
        app.runtime.memory.u32(GuestPtr(second + 144)).unwrap(),
        first
    );
    assert_eq!(
        app.runtime.memory.read(GuestPtr(second + 112), 1).unwrap(),
        [0]
    );
    assert!(contains(&region(&app, first, 24), 0, 0));
    assert!(!contains(&region(&app, first, 24), 15, 15));
    let ports = app.runtime.process.ports.unwrap();
    ports.set(&mut app.runtime.memory, GuestPtr(first)).unwrap();
    ports
        .paint_rect(
            app.runtime.graphics.as_ref().unwrap(),
            &mut app.runtime.memory,
            Rect::new(0, 0, 20, 20),
        )
        .unwrap();
    assert!(pixel(&app, 41, 41));
    assert!(!pixel(&app, 55, 55));
    let update = app.runtime.memory.u32(GuestPtr(first + 122)).unwrap();
    let update_data = app.runtime.memory.u32(GuestPtr(update)).unwrap();
    app.runtime
        .memory
        .write(GuestPtr(update_data), &[0, 10, 0, 0, 0, 0, 0, 0, 0, 0])
        .unwrap();
    invoke(&mut app, 4, &[second]);
    assert_eq!(invoke(&mut app, 3, &[]), first);
    assert!(pixel(&app, 41, 41));
    assert!(!pixel(&app, 55, 55));
    assert!(contains(&region(&app, first, 122), 55, 55));
    assert!(!contains(&region(&app, first, 122), 41, 41));
    m68(&mut app, 4, &second.to_be_bytes(), 4, false);
    m68(&mut app, 4, &first.to_be_bytes(), 4, false);
    assert_eq!(invoke(&mut app, 3, &[]), 0);
    rect(&mut app, data + 64, 30, 30, 40, 40);
    let caller = invoke(
        &mut app,
        2,
        &[0x51000000, data + 64, data + 80, 0, 2, 0, 1, 7],
    );
    assert_eq!(caller, 0x51000000);
    let old = app.runtime.memory.u32(slot).unwrap();
    assert_ne!(old, caller);
    assert_eq!(invoke(&mut app, 3, &[]), 0);
    m68(&mut app, 2, &caller.to_be_bytes(), 4, false);
    assert_eq!(
        app.runtime.memory.read(GuestPtr(caller + 156), 4).unwrap(),
        [0x5a; 4]
    );
    let reused = invoke(&mut app, 2, &[caller, data + 64, data + 80, 0, 2, 0, 0, 8]);
    assert_eq!(reused, caller);
    invoke(&mut app, 4, &[caller]);
    let heap_storage = app
        .runtime
        .process
        .new_pointer(&mut app.runtime.memory, 156, true)
        .unwrap();
    assert_eq!(
        invoke(
            &mut app,
            2,
            &[heap_storage, data + 64, data + 80, 0, 2, 0, 0, 13]
        ),
        heap_storage
    );
    invoke(&mut app, 4, &[heap_storage]);
    invoke(&mut app, 5, &[heap_storage]);
    assert_eq!(
        app.runtime.process.pointer_size(heap_storage),
        (-111i32) as u32
    );
    let mut windows = std::mem::take(&mut app.runtime.process.windows);
    let gpu = app.runtime.graphics.as_ref().unwrap();
    let request = systemless_future::windows::NewWindow {
        storage: GuestPtr(0),
        bounds: GuestPtr(data + 64),
        title: GuestPtr(data + 80),
        visible: true,
        proc_id: 2,
        behind: u32::MAX,
        go_away: false,
        ref_con: 0,
    };
    let before = app
        .runtime
        .memory
        .read(GuestPtr(caller), 160)
        .unwrap()
        .to_vec();
    for invalid in [
        systemless_future::windows::NewWindow {
            proc_id: 0,
            ..request
        },
        systemless_future::windows::NewWindow {
            behind: 1,
            ..request
        },
        systemless_future::windows::NewWindow {
            title: GuestPtr(0xdeadbeef),
            ..request
        },
        systemless_future::windows::NewWindow {
            storage: GuestPtr(app.runtime.memory.u32(GuestPtr(0x9de)).unwrap()),
            ..request
        },
    ] {
        assert!(windows
            .create(
                &mut app.runtime.memory,
                &mut app.runtime.process,
                gpu,
                invalid
            )
            .is_err());
        assert_eq!(
            app.runtime
                .memory
                .u32(systemless_future::windows::LIST)
                .unwrap(),
            0
        );
        assert_eq!(
            app.runtime.memory.read(GuestPtr(caller), 160).unwrap(),
            before
        );
    }
    let mut held = Vec::new();
    let mut size = systemless_future::process::HEAP_SIZE;
    while size >= 4 {
        loop {
            let p = app
                .runtime
                .process
                .new_pointer(&mut app.runtime.memory, size, false)
                .unwrap();
            if p == 0 {
                break;
            }
            held.push(p);
        }
        size /= 2;
    }
    assert_eq!(
        windows
            .create(
                &mut app.runtime.memory,
                &mut app.runtime.process,
                gpu,
                request
            )
            .unwrap(),
        0
    );
    assert_eq!(app.runtime.process.mem_error, -108);
    assert_eq!(
        windows
            .create(
                &mut app.runtime.memory,
                &mut app.runtime.process,
                gpu,
                systemless_future::windows::NewWindow {
                    storage: GuestPtr(caller),
                    ..request
                }
            )
            .unwrap(),
        0
    );
    assert_eq!(app.runtime.process.mem_error, -108);
    assert_eq!(
        app.runtime.memory.read(GuestPtr(caller), 160).unwrap(),
        before
    );
    for p in held {
        app.runtime.process.dispose_pointer(p);
    }
    app.runtime.process.windows = windows;
}

#[test]
fn new_window_adapter_checks_result_storage_before_popping_arguments() {
    use systemless_future::{abi::Abi, memory::Memory};
    let mut memory = Memory::default();
    memory
        .map(GuestPtr(0x1000), vec![0; 32], Permissions::RX)
        .unwrap();
    let Cpu::M68k(mut cpu) = Cpu::new(Architecture::M68k, 0, 0x1000) else {
        panic!()
    };
    assert!(Abi::trap(0xa913, &mut cpu, &memory).is_err());
    assert_eq!(cpu.dar[15], 0x1000);
}

#[test]
#[ignore = "requires a real wgpu adapter"]
fn wind_resources_share_live_templates_document_chrome_and_native_result_slots() {
    let mut template = vec![0; 22];
    for (offset, value) in [(0, 80), (2, 100), (4, 140), (6, 260), (8, 4)] {
        w16(&mut template, offset, value);
    }
    template[10] = 1;
    template[12] = 1;
    w32(&mut template, 14, 0x12345678);
    template[18..].copy_from_slice(b"\x03DOC");
    let fork = resource(vec![
        (*b"WIND", 321, None, template),
        (*b"WIND", 322, None, vec![0; 18]),
    ]);
    let f = pef_program(
        &[0x4e800020],
        &[
            b"InitGraf",
            b"InitFonts",
            b"InitWindows",
            b"GetNewWindow",
            b"DisposeWindow",
            b"CloseWindow",
        ],
        512,
    );
    let mut app = LoadedApplication::pef(&f.bytes, Some(&fork)).unwrap();
    let data = app.info.toc_or_a5;
    app.runtime
        .memory
        .map(GuestPtr(0x50000000), vec![0xa9, 0xbd], Permissions::RX)
        .unwrap();
    invoke(&mut app, 0, &[data + 330]);
    invoke(&mut app, 1, &[]);
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
    app.runtime.graphics = Some(
        pollster::block_on(Graphics::new(&instance, None, 512, 342, GuestPtr(0x4000))).unwrap(),
    );
    invoke(&mut app, 2, &[]);
    let first = invoke(&mut app, 3, &[321, 0, u32::MAX]);
    assert_ne!(first, 0);
    assert_eq!(
        app.runtime.memory.u32(GuestPtr(first + 152)).unwrap(),
        0x12345678
    );
    assert_eq!(region(&app, first, 114), [Rect::new(99, 61, 262, 142)]);
    assert_eq!(region(&app, first, 118), [Rect::new(100, 80, 260, 140)]);
    assert!(pixel(&app, 99, 61)); // title outline
    assert!(pixel(&app, 108, 65)); // close-box top
    assert!(!pixel(&app, 109, 66)); // close-box interior
    assert!(pixel(&app, 261, 100)); // shadow
    assert!(!pixel(&app, 150, 100)); // content is white
                                     // Glyph pixels lie inside the title gap, away from its stripes.
    assert!((155..205).any(|x| (64..77).any(|y| pixel(&app, x, y))));
    let first_screen = app.runtime.memory.read(SCREEN, 64 * 342).unwrap().to_vec();
    invoke(&mut app, 4, &[first]);
    let mut frame = vec![0; 10];
    w32(&mut frame, 0, u32::MAX);
    w16(&mut frame, 8, 321);
    let second = m68(&mut app, 0, &frame, 10, true);
    assert_ne!(second, 0);
    assert_eq!(
        app.runtime.memory.read(SCREEN, 64 * 342).unwrap(),
        first_screen
    );
    invoke(&mut app, 4, &[second]);
    assert_eq!(invoke(&mut app, 3, &[322, 0, u32::MAX]), 0); // truncated WIND
    assert_eq!(invoke(&mut app, 3, &[999, 0, u32::MAX]), 0);
    assert_eq!(
        app.runtime
            .memory
            .u32(systemless_future::windows::LIST)
            .unwrap(),
        0
    );
    // The cached resource handle is guest state: edit the live title and refCon.
    let h = app
        .runtime
        .process
        .get_resource(&mut app.runtime.memory, *b"WIND", 321)
        .unwrap();
    let bytes = GuestPtr(app.runtime.memory.u32(GuestPtr(h)).unwrap());
    app.runtime
        .memory
        .write(GuestPtr(bytes.0 + 14), &7u32.to_be_bytes())
        .unwrap();
    app.runtime
        .memory
        .write(GuestPtr(bytes.0 + 19), b"RAM")
        .unwrap();
    let changed = invoke(&mut app, 3, &[321, 0, u32::MAX]);
    assert_eq!(app.runtime.memory.u32(GuestPtr(changed + 152)).unwrap(), 7);
    let title = app.runtime.memory.u32(GuestPtr(changed + 134)).unwrap();
    let title = GuestPtr(app.runtime.memory.u32(GuestPtr(title)).unwrap());
    assert_eq!(app.runtime.memory.read(title, 4).unwrap(), b"\x03RAM");
    app.runtime
        .memory
        .write(GuestPtr(bytes.0 + 8), &1u16.to_be_bytes())
        .unwrap();
    rect(&mut app, bytes.0, 180, 300, 240, 400);
    app.runtime
        .memory
        .map(GuestPtr(0x51000000), vec![0x5a; 160], Permissions::RW)
        .unwrap();
    w32(&mut frame, 4, 0x51000000);
    let modal = m68(&mut app, 0, &frame, 10, true);
    assert_eq!(modal, 0x51000000);
    assert_eq!(region(&app, modal, 114), [Rect::new(292, 172, 408, 248)]);
    assert!(pixel(&app, 292, 172));
    assert!(!pixel(&app, 293, 173));
    assert!(pixel(&app, 295, 175));
    assert!(!pixel(&app, 297, 177));
    assert_eq!(
        app.runtime.memory.read(GuestPtr(modal + 156), 4).unwrap(),
        [0x5a; 4]
    );
    assert!(!pixel(&app, 108, 65)); // previous document's close box is inactive.
    invoke(&mut app, 5, &[modal]);
    assert!(pixel(&app, 108, 65)); // restored active chrome.
    invoke(&mut app, 4, &[changed]);
    assert_eq!(
        app.runtime
            .memory
            .u32(systemless_future::windows::LIST)
            .unwrap(),
        0
    );
}

#[test]
fn size_visibility_and_reference_adapters_use_pascal_words_and_preserve_padding() {
    use systemless_future::{
        abi::{Abi, Reply, Service},
        windows::Change,
    };
    let mut memory = systemless_future::memory::Memory::default();
    let mut frame = vec![0x5a; 18];
    frame[0] = 255;
    frame[1] = 0xa5;
    w16(&mut frame, 2, 33);
    w16(&mut frame, 4, 44);
    w32(&mut frame, 6, 0x12345678);
    memory
        .map(GuestPtr(0x1000), frame, Permissions::RW)
        .unwrap();
    let mut cpu = Cpu::new(Architecture::M68k, 0, 0x1000);
    let Cpu::M68k(c) = &mut cpu else { panic!() };
    let (call, reply) = Abi::trap(0xa91d, c, &memory).unwrap();
    assert_eq!(
        call,
        systemless_future::cpu::Call::WindowChange {
            pointer: 0x12345678,
            change: Change::Size {
                width: 44,
                height: 33,
                update: true
            }
        }
    );
    assert_eq!(reply, Reply::None);
    assert_eq!(c.dar[15], 0x100a);
    assert_eq!(memory.read(GuestPtr(0x100a), 8).unwrap(), [0x5a; 8]);
    let mut cpu = Cpu::new(Architecture::PowerPc, 0, 0x1000);
    let Cpu::PowerPc(c) = &mut cpu else { panic!() };
    c.gpr[3] = 0x12345678;
    c.gpr[4] = 44;
    c.gpr[5] = 33;
    c.gpr[6] = 255;
    assert_eq!(
        Abi::import(Service::SizeWindow, c, &memory).unwrap(),
        (call, Reply::None)
    );
    memory
        .write(GuestPtr(0x1002), &0x12345678u32.to_be_bytes())
        .unwrap();
    let mut cpu = Cpu::new(Architecture::M68k, 0, 0x1000);
    let Cpu::M68k(c) = &mut cpu else { panic!() };
    assert_eq!(
        Abi::trap(0xa908, c, &memory).unwrap().0,
        systemless_future::cpu::Call::WindowChange {
            pointer: 0x12345678,
            change: Change::Visibility(true)
        }
    );
    assert_eq!(c.dar[15], 0x1006);
    memory
        .map(
            GuestPtr(0x2000),
            0x12345678u32.to_be_bytes().to_vec(),
            Permissions::RW,
        )
        .unwrap();
    memory
        .map(GuestPtr(0x2004), vec![0; 4], Permissions::RX)
        .unwrap();
    let mut cpu = Cpu::new(Architecture::M68k, 0, 0x2000);
    let Cpu::M68k(c) = &mut cpu else { panic!() };
    assert!(Abi::trap(0xa917, c, &memory).is_err());
    assert_eq!(c.dar[15], 0x2000);
}

#[test]
#[ignore = "requires a real wgpu adapter"]
fn sizing_preserves_pixels_clip_and_control_state_and_visibility_queues_exposure() {
    use systemless_future::{
        controls::NewControl,
        windows::{Change, Op},
    };
    let f = pef_program(
        &[0x4e800020],
        &[
            b"InitGraf",
            b"InitFonts",
            b"InitWindows",
            b"NewWindow",
            b"SizeWindow",
            b"ShowHide",
            b"SetWRefCon",
            b"GetWRefCon",
        ],
        512,
    );
    let mut app = LoadedApplication::pef(&f.bytes, None).unwrap();
    let data = app.info.toc_or_a5;
    app.runtime
        .memory
        .map(GuestPtr(0x50000000), vec![0xa9, 0x1d], Permissions::RX)
        .unwrap();
    app.runtime
        .memory
        .map(GuestPtr(0x51000000), vec![0; 512], Permissions::RW)
        .unwrap();
    invoke(&mut app, 0, &[data + 330]);
    invoke(&mut app, 1, &[]);
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
    app.runtime.graphics = Some(
        pollster::block_on(Graphics::new(&instance, None, 512, 342, GuestPtr(0x4000))).unwrap(),
    );
    invoke(&mut app, 2, &[]);
    rect(&mut app, 0x51000000, 80, 100, 180, 300);
    app.runtime
        .memory
        .write(GuestPtr(0x51000010), b"\x04back")
        .unwrap();
    let back = invoke(
        &mut app,
        3,
        &[0, 0x51000000, 0x51000010, 1, 2, u32::MAX, 0, 0],
    );
    rect(&mut app, 0x51000000, 100, 120, 140, 180);
    let front = invoke(
        &mut app,
        3,
        &[0, 0x51000000, 0x51000010, 1, 2, u32::MAX, 0, 0],
    );
    invoke(&mut app, 6, &[front, 0xfedcba98]);
    assert_eq!(invoke(&mut app, 7, &[front]), 0xfedcba98);
    let clip = app.runtime.memory.u32(GuestPtr(front + 28)).unwrap();
    let clip_ptr = GuestPtr(app.runtime.memory.u32(GuestPtr(clip)).unwrap());
    let clip_size = app
        .runtime
        .process
        .handle_size(&app.runtime.memory, clip)
        .unwrap();
    let old_clip = app
        .runtime
        .memory
        .read(clip_ptr, clip_size as usize)
        .unwrap()
        .to_vec();
    // Clear prior invalidation, then mark retained content with a direct guest write.
    for w in [back, front] {
        let h = app.runtime.memory.u32(GuestPtr(w + 122)).unwrap();
        app.runtime
            .process
            .set_handle_size(&mut app.runtime.memory, h, 10)
            .unwrap();
        let p = GuestPtr(app.runtime.memory.u32(GuestPtr(h)).unwrap());
        app.runtime
            .memory
            .write(p, &[0, 10, 0, 0, 0, 0, 0, 0, 0, 0])
            .unwrap();
    }
    let address = GuestPtr(SCREEN.0 + 110 * 64 + 130 / 8);
    let old = app.runtime.memory.read(address, 1).unwrap()[0];
    app.runtime
        .memory
        .write(address, &[old | (0x80 >> (130 % 8))])
        .unwrap();
    invoke(&mut app, 4, &[front, 80, 60, 1]);
    assert!(pixel(&app, 130, 110));
    let grown_pixels = app.runtime.memory.read(SCREEN, 64 * 342).unwrap().to_vec();
    assert_eq!(region(&app, front, 118), [Rect::new(120, 100, 200, 160)]);
    assert!(contains(&region(&app, front, 122), 190, 110));
    assert!(!contains(&region(&app, front, 122), 130, 110));
    assert!(!contains(&region(&app, back, 24), 100, 40));
    assert_eq!(
        app.runtime
            .memory
            .read(
                GuestPtr(app.runtime.memory.u32(GuestPtr(clip)).unwrap()),
                old_clip.len()
            )
            .unwrap(),
        old_clip
    );
    // Compare native size arguments against PPC at the same prior state.
    invoke(&mut app, 4, &[front, 60, 40, 0]);
    let mut frame = vec![0; 10];
    frame[0] = 255;
    frame[1] = 0xa5;
    w16(&mut frame, 2, 60);
    w16(&mut frame, 4, 80);
    w32(&mut frame, 6, front);
    m68(&mut app, 0, &frame, 10, false);
    assert_eq!(
        app.runtime.memory.read(SCREEN, 64 * 342).unwrap(),
        grown_pixels
    );
    assert_eq!(region(&app, front, 118), [Rect::new(120, 100, 200, 160)]);
    assert!(pixel(&app, 130, 110));
    // Growing with fUpdate false does not generate a new content invalidation.
    let h = app.runtime.memory.u32(GuestPtr(front + 122)).unwrap();
    app.runtime
        .process
        .set_handle_size(&mut app.runtime.memory, h, 10)
        .unwrap();
    app.runtime
        .memory
        .write(
            GuestPtr(app.runtime.memory.u32(GuestPtr(h)).unwrap()),
            &[0, 10, 0, 0, 0, 0, 0, 0, 0, 0],
        )
        .unwrap();
    invoke(&mut app, 4, &[front, 90, 70, 0]);
    assert!(region(&app, front, 122).is_empty());
    // Controls stay owned while hidden, and redraw using the new visible region.
    rect(&mut app, 0x51000000, 5, 5, 25, 65);
    app.runtime
        .memory
        .write(GuestPtr(0x51000010), b"\x02OK")
        .unwrap();
    let request = NewControl {
        owner: GuestPtr(front),
        bounds: GuestPtr(0x51000000),
        title: GuestPtr(0x51000010),
        visible: true,
        value: 0,
        min: 0,
        max: 1,
        proc_id: 0,
        ref_con: 7,
    };
    let mut controls = std::mem::take(&mut app.runtime.process.controls);
    let control = controls
        .create(
            &mut app.runtime.memory,
            &mut app.runtime.process,
            app.runtime.graphics.as_ref(),
            request,
        )
        .unwrap();
    app.runtime.process.controls = controls;
    let list = app
        .runtime
        .memory
        .u32(systemless_future::windows::LIST)
        .unwrap();
    let highlight = app.runtime.memory.read(GuestPtr(front + 111), 1).unwrap()[0];
    invoke(&mut app, 5, &[front, 0]);
    assert!(region(&app, front, 24).is_empty());
    assert_ne!(app.runtime.memory.u32(GuestPtr(control)).unwrap(), 0);
    assert_eq!(
        app.runtime.memory.read(GuestPtr(front + 111), 1).unwrap()[0],
        highlight
    );
    assert!(contains(&region(&app, back, 122), 130, 110));
    assert_eq!(
        app.runtime
            .memory
            .u32(systemless_future::windows::LIST)
            .unwrap(),
        list
    );
    invoke(&mut app, 5, &[front, 1]);
    assert!(contains(&region(&app, front, 122), 130, 110));
    let mut controls = std::mem::take(&mut app.runtime.process.controls);
    controls
        .call(
            &mut app.runtime.memory,
            &mut app.runtime.process,
            app.runtime.graphics.as_ref(),
            systemless_future::controls::Op::DrawAll(front),
        )
        .unwrap();
    app.runtime.process.controls = controls;
    assert!(pixel(&app, 145, 105));
    // A malformed shared region handle must fail the whole staged change.
    let port_before = app
        .runtime
        .memory
        .read(GuestPtr(front + 16), 8)
        .unwrap()
        .to_vec();
    let wrong = app.runtime.memory.u32(GuestPtr(front + 24)).unwrap();
    app.runtime
        .memory
        .write(GuestPtr(front + 122), &wrong.to_be_bytes())
        .unwrap();
    let mut windows = std::mem::take(&mut app.runtime.process.windows);
    assert!(windows
        .change(
            &mut app.runtime.memory,
            &mut app.runtime.process,
            app.runtime.graphics.as_ref().unwrap(),
            front,
            Change::Size {
                width: 100,
                height: 80,
                update: true
            }
        )
        .is_err());
    assert_eq!(
        app.runtime.memory.read(GuestPtr(front + 16), 8).unwrap(),
        port_before
    );
    app.runtime
        .memory
        .write(GuestPtr(front + 122), &h.to_be_bytes())
        .unwrap();
    windows
        .call(
            &mut app.runtime.memory,
            &mut app.runtime.process,
            app.runtime.graphics.as_ref().unwrap(),
            Op::Dispose,
            front,
        )
        .unwrap();
    windows
        .call(
            &mut app.runtime.memory,
            &mut app.runtime.process,
            app.runtime.graphics.as_ref().unwrap(),
            Op::Dispose,
            back,
        )
        .unwrap();
    app.runtime.process.windows = windows;
}

#[test]
#[ignore = "requires a real wgpu adapter"]
fn moves_copy_overlapping_pixels_and_selection_visibility_project_activation_slots() {
    let f = pef_program(
        &[0x4e800020],
        &[
            b"InitGraf",
            b"InitFonts",
            b"InitWindows",
            b"NewWindow",
            b"MoveWindow",
            b"SelectWindow",
            b"HideWindow",
            b"ShowWindow",
            b"DisposeWindow",
            b"SetPort",
            b"InvalRect",
            b"ValidRect",
        ],
        512,
    );
    let mut app = LoadedApplication::pef(&f.bytes, None).unwrap();
    let data = app.info.toc_or_a5;
    app.runtime
        .memory
        .map(GuestPtr(0x50000000), vec![0xa9, 0x1b], Permissions::RX)
        .unwrap();
    app.runtime
        .memory
        .map(GuestPtr(0x51000000), vec![0; 512], Permissions::RW)
        .unwrap();
    invoke(&mut app, 0, &[data + 330]);
    invoke(&mut app, 1, &[]);
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
    app.runtime.graphics = Some(
        pollster::block_on(Graphics::new(&instance, None, 512, 342, GuestPtr(0x4000))).unwrap(),
    );
    invoke(&mut app, 2, &[]);
    rect(&mut app, 0x51000000, 80, 100, 180, 300);
    app.runtime
        .memory
        .write(GuestPtr(0x51000010), b"\x04back")
        .unwrap();
    let back = invoke(
        &mut app,
        3,
        &[0, 0x51000000, 0x51000010, 1, 2, u32::MAX, 0, 7],
    );
    rect(&mut app, 0x51000000, 100, 120, 140, 180);
    let front = invoke(
        &mut app,
        3,
        &[0, 0x51000000, 0x51000010, 1, 2, u32::MAX, 0, 9],
    );
    let slots = app
        .runtime
        .memory
        .read(GuestPtr(0xa64), 8)
        .unwrap()
        .to_vec();
    let local = app
        .runtime
        .memory
        .read(GuestPtr(front + 16), 8)
        .unwrap()
        .to_vec();
    for (x, y) in [(130u32, 110u32), (160, 125)] {
        let p = GuestPtr(SCREEN.0 + y * 64 + x / 8);
        let byte = app.runtime.memory.read(p, 1).unwrap()[0];
        app.runtime
            .memory
            .write(p, &[byte | (0x80 >> (x % 8))])
            .unwrap();
    }
    invoke(&mut app, 4, &[front, 130, 110, 0]);
    assert!(pixel(&app, 140, 120));
    assert!(pixel(&app, 170, 135));
    assert!(!pixel(&app, 130, 110));
    assert!(!pixel(&app, 119, 99));
    assert_eq!(
        app.runtime.memory.read(GuestPtr(front + 16), 8).unwrap(),
        local
    );
    assert_eq!(region(&app, front, 118), [Rect::new(130, 110, 190, 150)]);
    assert_eq!(app.runtime.memory.read(GuestPtr(0xa64), 8).unwrap(), slots);
    let moved = app.runtime.memory.read(SCREEN, 64 * 342).unwrap().to_vec();
    invoke(&mut app, 4, &[front, 120, 100, 0]);
    let mut frame = vec![0; 10];
    frame[1] = 0xa5;
    w16(&mut frame, 2, 110);
    w16(&mut frame, 4, 130);
    w32(&mut frame, 6, front);
    m68(&mut app, 0, &frame, 10, false);
    assert_eq!(app.runtime.memory.read(SCREEN, 64 * 342).unwrap(), moved);
    invoke(&mut app, 5, &[back]);
    assert_eq!(
        app.runtime
            .memory
            .u32(systemless_future::windows::LIST)
            .unwrap(),
        back
    );
    assert_eq!(app.runtime.memory.u32(GuestPtr(0xa68)).unwrap(), front);
    assert_eq!(app.runtime.memory.u32(GuestPtr(0xa64)).unwrap(), back);
    assert_eq!(
        app.runtime.memory.read(GuestPtr(front + 111), 1).unwrap(),
        [0]
    );
    assert!(region(&app, front, 24).is_empty());
    assert!(contains(&region(&app, back, 122), 140, 120));
    assert!(!pixel(&app, 140, 120));
    // front=TRUE moves and selects without reading covered source pixels.
    invoke(&mut app, 4, &[front, 140, 120, 1]);
    assert_eq!(
        app.runtime
            .memory
            .u32(systemless_future::windows::LIST)
            .unwrap(),
        front
    );
    assert_eq!(app.runtime.memory.u32(GuestPtr(0xa68)).unwrap(), back);
    assert_eq!(app.runtime.memory.u32(GuestPtr(0xa64)).unwrap(), front);
    assert!(contains(&region(&app, front, 122), 150, 130));
    // Hide fronts the next visible window; showing does not restore old order.
    invoke(&mut app, 6, &[front]);
    assert_eq!(
        app.runtime
            .memory
            .u32(systemless_future::windows::LIST)
            .unwrap(),
        back
    );
    invoke(&mut app, 7, &[front]);
    assert_eq!(
        app.runtime
            .memory
            .u32(systemless_future::windows::LIST)
            .unwrap(),
        back
    );
    assert_eq!(
        app.runtime.memory.read(GuestPtr(front + 111), 1).unwrap(),
        [0]
    );
    invoke(&mut app, 5, &[front]);
    invoke(&mut app, 9, &[front]);
    rect(&mut app, 0x51000000, 1, 2, 8, 9);
    let h = app.runtime.memory.u32(GuestPtr(front + 122)).unwrap();
    app.runtime
        .process
        .set_handle_size(&mut app.runtime.memory, h, 10)
        .unwrap();
    app.runtime
        .memory
        .write(
            GuestPtr(app.runtime.memory.u32(GuestPtr(h)).unwrap()),
            &[0, 10, 0, 0, 0, 0, 0, 0, 0, 0],
        )
        .unwrap();
    invoke(&mut app, 10, &[0x51000000]);
    assert_eq!(region(&app, front, 122), [Rect::new(142, 121, 149, 128)]);
    invoke(&mut app, 11, &[0x51000000]);
    assert!(region(&app, front, 122).is_empty());
    invoke(&mut app, 8, &[front]);
    assert_ne!(app.runtime.memory.u32(GuestPtr(0xa64)).unwrap(), front);
    assert_ne!(app.runtime.memory.u32(GuestPtr(0xa68)).unwrap(), front);
    invoke(&mut app, 8, &[back]);
    assert_eq!(app.runtime.memory.u32(GuestPtr(0xa64)).unwrap(), 0);
    assert_eq!(app.runtime.memory.u32(GuestPtr(0xa68)).unwrap(), 0);
}

#[test]
#[ignore = "requires a real wgpu adapter for window creation"]
fn find_window_matches_live_regions_front_order_and_chrome_on_both_cpus_without_drawing() {
    let f = pef_program(
        &[0x4e800020],
        &[
            b"InitGraf",
            b"InitWindows",
            b"NewWindow",
            b"FindWindow",
            b"InitFonts",
            b"NewControl",
            b"FindControl",
        ],
        512,
    );
    let mut app = LoadedApplication::pef(&f.bytes, None).unwrap();
    let data = app.info.toc_or_a5;
    app.runtime
        .memory
        .map(
            GuestPtr(0x50000000),
            vec![0xa9, 0x2c, 0xa9, 0x6c],
            Permissions::RX,
        )
        .unwrap();
    invoke(&mut app, 0, &[data + 330]);
    invoke(&mut app, 4, &[]);
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
    app.runtime.graphics = Some(
        pollster::block_on(Graphics::new(&instance, None, 512, 342, GuestPtr(0x4000))).unwrap(),
    );
    invoke(&mut app, 1, &[]);
    rect(&mut app, data + 64, 40, 40, 200, 200);
    app.runtime
        .memory
        .write(GuestPtr(data + 80), b"\x04test")
        .unwrap();
    let back = invoke(
        &mut app,
        2,
        &[0, data + 64, data + 80, 1, 2, u32::MAX, 0, 0],
    );
    rect(&mut app, data + 64, 80, 80, 160, 160);
    let front = invoke(
        &mut app,
        2,
        &[0, data + 64, data + 80, 1, 4, u32::MAX, 1, 0],
    );
    rect(&mut app, data + 64, 10, 10, 30, 40);
    let button = invoke(
        &mut app,
        5,
        &[front, data + 64, data + 80, 1, 0, 0, 1, 0, 0],
    );
    let checkbox = invoke(
        &mut app,
        5,
        &[front, data + 64, data + 80, 1, 0, 0, 1, 1, 0],
    );
    // Querying requires no graphics device and never changes current port or order.
    app.runtime.graphics = None;
    app.runtime
        .memory
        .write(GuestPtr(0xbaa), &20u16.to_be_bytes())
        .unwrap();
    let slot = app.runtime.process.ports.unwrap().current_slot;
    let selected = app.runtime.memory.u32(slot).unwrap();
    let screen = app.runtime.memory.read(SCREEN, 64 * 342).unwrap().to_vec();
    let output = data + 100;
    fn query(app: &mut LoadedApplication, data: u32, point: [i16; 2], part: u16, window: u32) {
        for arch in [Architecture::M68k, Architecture::PowerPc] {
            app.runtime.process.mem_error = -108;
            app.runtime
                .memory
                .write(GuestPtr(data + 98), &[0xa5; 8])
                .unwrap();
            let packed = ((point[0] as u16 as u32) << 16) | point[1] as u16 as u32;
            let result = if arch == Architecture::PowerPc {
                invoke(app, 3, &[packed, data + 100])
            } else {
                let mut frame = (data + 100).to_be_bytes().to_vec();
                frame.extend(packed.to_be_bytes());
                let stack = STACK_BASE + 0x8000;
                frame.extend([0xa5; 4]);
                app.runtime.memory.write(GuestPtr(stack), &frame).unwrap();
                app.runtime.cpu = Cpu::new(arch, 0x50000000, stack);
                assert_eq!(app.runtime.advance(1).unwrap(), Stop::Budget);
                let Cpu::M68k(c) = &app.runtime.cpu else {
                    panic!()
                };
                assert_eq!(c.dar[15], stack + 8);
                assert_eq!(
                    app.runtime.memory.u16(GuestPtr(stack + 10)).unwrap(),
                    0xa5a5
                );
                u32::from(app.runtime.memory.u16(GuestPtr(stack + 8)).unwrap())
            };
            assert_eq!(app.runtime.process.mem_error, -108);
            assert_eq!(result, u32::from(part), "point {point:?} {arch:?}");
            assert_eq!(
                app.runtime.memory.u32(GuestPtr(data + 100)).unwrap(),
                window
            );
            assert_eq!(app.runtime.memory.u16(GuestPtr(data + 98)).unwrap(), 0xa5a5);
            assert_eq!(
                app.runtime.memory.u16(GuestPtr(data + 104)).unwrap(),
                0xa5a5
            );
        }
    }
    for (point, part, w) in [
        ([90, 100], 3, front),
        ([70, 120], 4, front),
        ([65, 88], 6, front),
        ([75, 98], 6, front),
        ([76, 88], 4, front),
        ([81, 79], 0, 0),
        ([160, 90], 0, 0),
        ([90, 160], 0, 0),
        ([50, 50], 3, back),
        ([10, 10], 1, 0),
        ([20, 20], 0, 0),
        ([-1, 50], 0, 0),
        ([342, 10], 0, 0),
        ([100, 512], 0, 0),
    ] {
        query(&mut app, data, point, part, w);
    }
    app.runtime
        .memory
        .write(GuestPtr(front + 111), &[0])
        .unwrap();
    query(&mut app, data, [66, 90], 4, front);
    app.runtime
        .memory
        .write(GuestPtr(front + 111), &[1])
        .unwrap();
    app.runtime
        .memory
        .write(GuestPtr(front + 112), &[0])
        .unwrap();
    query(&mut app, data, [66, 90], 4, front);
    app.runtime
        .memory
        .write(GuestPtr(front + 110), &[0])
        .unwrap();
    query(&mut app, data, [90, 100], 3, back);
    app.runtime
        .memory
        .write(GuestPtr(back + 108), &(-1i16).to_be_bytes())
        .unwrap();
    query(&mut app, data, [90, 100], 2, back);
    app.runtime
        .memory
        .write(GuestPtr(back + 108), &8u16.to_be_bytes())
        .unwrap();
    app.runtime
        .memory
        .write(GuestPtr(front + 110), &[1])
        .unwrap();
    app.runtime
        .memory
        .write(GuestPtr(0xbaa), &0u16.to_be_bytes())
        .unwrap();
    query(&mut app, data, [10, 10], 0, 0);
    // A hole in the front structure exposes the next window; a content hole doesn't.
    fn shape(a: Rect, b: Rect) -> Vec<u8> {
        let encoded = |r: Rect| {
            [
                10u16,
                r.top as u16,
                r.left as u16,
                r.bottom as u16,
                r.right as u16,
            ]
            .into_iter()
            .flat_map(u16::to_be_bytes)
            .collect::<Vec<_>>()
        };
        systemless_future::region_geometry::combine(
            &encoded(a),
            &encoded(b),
            systemless_future::region_geometry::Boolean::Union,
        )
        .unwrap()
    }
    let structure = shape(Rect::new(80, 80, 100, 160), Rect::new(120, 80, 160, 160));
    let h = app.runtime.memory.u32(GuestPtr(front + 114)).unwrap();
    app.runtime
        .process
        .set_handle_size(&mut app.runtime.memory, h, structure.len() as u32)
        .unwrap();
    let pointer = GuestPtr(app.runtime.memory.u32(GuestPtr(h)).unwrap());
    app.runtime.memory.write(pointer, &structure).unwrap();
    query(&mut app, data, [90, 110], 3, back);
    query(&mut app, data, [90, 90], 3, front);
    let shape = shape(Rect::new(80, 80, 85, 160), Rect::new(95, 80, 160, 160));
    let content = app.runtime.memory.u32(GuestPtr(front + 118)).unwrap();
    app.runtime
        .process
        .set_handle_size(&mut app.runtime.memory, content, shape.len() as u32)
        .unwrap();
    let pointer = GuestPtr(app.runtime.memory.u32(GuestPtr(content)).unwrap());
    app.runtime.memory.write(pointer, &shape).unwrap();
    // A content hole over a solid structure does not expose the back window.
    query(&mut app, data, [90, 90], 0, 0);
    fn find_control(
        app: &mut LoadedApplication,
        data: u32,
        owner: u32,
        point: [i16; 2],
        part: u16,
        handle: u32,
    ) {
        let packed = ((point[0] as u16 as u32) << 16) | point[1] as u16 as u32;
        for arch in [Architecture::M68k, Architecture::PowerPc] {
            app.runtime
                .memory
                .write(GuestPtr(data + 98), &[0xa5; 8])
                .unwrap();
            let result = if arch == Architecture::PowerPc {
                invoke(app, 6, &[packed, owner, data + 100])
            } else {
                let mut frame = (data + 100).to_be_bytes().to_vec();
                frame.extend(owner.to_be_bytes());
                frame.extend(packed.to_be_bytes());
                frame.extend([0xa5; 4]);
                let stack = STACK_BASE + 0x8000;
                app.runtime.memory.write(GuestPtr(stack), &frame).unwrap();
                app.runtime.cpu = Cpu::new(arch, 0x50000002, stack);
                assert_eq!(app.runtime.advance(1).unwrap(), Stop::Budget);
                let Cpu::M68k(c) = &app.runtime.cpu else {
                    panic!()
                };
                assert_eq!(c.dar[15], stack + 12);
                assert_eq!(
                    app.runtime.memory.u16(GuestPtr(stack + 14)).unwrap(),
                    0xa5a5
                );
                u32::from(app.runtime.memory.u16(GuestPtr(stack + 12)).unwrap())
            };
            assert_eq!(result, u32::from(part));
            assert_eq!(
                app.runtime.memory.u32(GuestPtr(data + 100)).unwrap(),
                handle
            );
            assert_eq!(app.runtime.memory.u16(GuestPtr(data + 98)).unwrap(), 0xa5a5);
            assert_eq!(
                app.runtime.memory.u16(GuestPtr(data + 104)).unwrap(),
                0xa5a5
            );
        }
    }
    let button_data = app.runtime.memory.u32(GuestPtr(button)).unwrap();
    let check_data = app.runtime.memory.u32(GuestPtr(checkbox)).unwrap();
    find_control(&mut app, data, front, [10, 10], 11, checkbox);
    app.runtime
        .memory
        .write(GuestPtr(check_data + 16), &[0])
        .unwrap();
    find_control(&mut app, data, front, [10, 10], 10, button);
    app.runtime
        .memory
        .write(GuestPtr(button_data + 17), &[255])
        .unwrap();
    find_control(&mut app, data, front, [10, 10], 0, 0);
    app.runtime
        .memory
        .write(GuestPtr(button_data + 17), &[0])
        .unwrap();
    for point in [[30, 10], [10, 40], [-1, 10]] {
        find_control(&mut app, data, front, point, 0, 0);
    }
    app.runtime
        .memory
        .write(GuestPtr(button_data + 8), &[0, 40, 0, 50, 0, 60, 0, 80])
        .unwrap();
    find_control(&mut app, data, front, [40, 50], 10, button);
    // Validate the complete control list before writing any result.
    app.runtime
        .memory
        .write(GuestPtr(check_data), &checkbox.to_be_bytes())
        .unwrap();
    app.runtime
        .memory
        .write(GuestPtr(output), &[0xa5; 4])
        .unwrap();
    let mut controls = std::mem::take(&mut app.runtime.process.controls);
    assert!(controls
        .call(
            &mut app.runtime.memory,
            &mut app.runtime.process,
            None,
            systemless_future::controls::Op::Find {
                owner: front,
                point: [40, 50],
                destination: GuestPtr(output)
            }
        )
        .is_err());
    assert_eq!(
        app.runtime.memory.read(GuestPtr(output), 4).unwrap(),
        [0xa5; 4]
    );
    app.runtime
        .memory
        .write(GuestPtr(check_data), &button.to_be_bytes())
        .unwrap();
    app.runtime.process.controls = controls;
    app.runtime
        .memory
        .map(GuestPtr(0x53000000), vec![0xa5; 4], Permissions::RX)
        .unwrap();
    let mut controls = std::mem::take(&mut app.runtime.process.controls);
    assert!(controls
        .call(
            &mut app.runtime.memory,
            &mut app.runtime.process,
            None,
            systemless_future::controls::Op::Find {
                owner: front,
                point: [40, 50],
                destination: GuestPtr(0x53000000)
            }
        )
        .is_err());
    app.runtime.process.controls = controls;
    let windows = std::mem::take(&mut app.runtime.process.windows);
    assert!(windows
        .find(
            &mut app.runtime.memory,
            &mut app.runtime.process,
            [90, 90],
            GuestPtr(0x53000000)
        )
        .is_err());
    app.runtime.process.windows = windows;
    assert_eq!(
        app.runtime.memory.read(GuestPtr(0x53000000), 4).unwrap(),
        [0xa5; 4]
    );
    let before = app
        .runtime
        .memory
        .read(GuestPtr(front), 156)
        .unwrap()
        .to_vec();
    assert_eq!(app.runtime.memory.u32(slot).unwrap(), selected);
    assert_eq!(app.runtime.memory.read(SCREEN, 64 * 342).unwrap(), screen);
    assert_eq!(
        app.runtime
            .memory
            .u32(systemless_future::windows::LIST)
            .unwrap(),
        front
    );
    assert_eq!(app.runtime.memory.u32(GuestPtr(front + 144)).unwrap(), back);
    app.runtime
        .memory
        .write(GuestPtr(front + 126), &1u32.to_be_bytes())
        .unwrap();
    let windows = std::mem::take(&mut app.runtime.process.windows);
    app.runtime
        .memory
        .write(GuestPtr(output), &[0xa5; 4])
        .unwrap();
    assert!(windows
        .find(
            &mut app.runtime.memory,
            &mut app.runtime.process,
            [90, 90],
            GuestPtr(output)
        )
        .is_err());
    assert_eq!(
        app.runtime.memory.read(GuestPtr(output), 4).unwrap(),
        [0xa5; 4]
    );
    app.runtime
        .memory
        .write(GuestPtr(front + 126), &[0; 4])
        .unwrap();
    assert_eq!(
        app.runtime.memory.read(GuestPtr(front), 156).unwrap(),
        before
    );
    app.runtime
        .memory
        .write(GuestPtr(front + 144), &front.to_be_bytes())
        .unwrap();
    assert!(windows
        .find(
            &mut app.runtime.memory,
            &mut app.runtime.process,
            [90, 90],
            GuestPtr(output)
        )
        .is_err());
    assert_eq!(
        app.runtime.memory.read(GuestPtr(output), 4).unwrap(),
        [0xa5; 4]
    );
    app.runtime.process.windows = windows;
}

#[test]
fn find_window_adapter_validates_word_result_before_popping_and_unpacks_ppc_point() {
    use systemless_future::{
        abi::{Abi, Reply, Service},
        cpu::Call,
        memory::Memory,
    };
    let mut memory = Memory::default();
    memory
        .map(
            GuestPtr(0x1000),
            vec![0x12, 0x34, 0x56, 0x78, 0xff, 0xf9, 0, 13, 0xa5, 0x5a],
            Permissions::RW,
        )
        .unwrap();
    let expected = Call::FindWindow {
        point: [-7, 13],
        destination: GuestPtr(0x12345678),
    };
    let mut cpu = m68k::CpuCore::default();
    cpu.dar[15] = 0x1000;
    assert_eq!(
        Abi::trap(0xa92c, &mut cpu, &memory).unwrap(),
        (expected, Reply::StackWord(GuestPtr(0x1008)))
    );
    assert_eq!(cpu.dar[15], 0x1008);
    cpu.dar[15] = 0x1001;
    assert!(Abi::trap(0xa92c, &mut cpu, &memory).is_err());
    assert_eq!(cpu.dar[15], 0x1001);
    cpu.dar[15] = 0x1003;
    assert!(Abi::trap(0xa92c, &mut cpu, &memory).is_err());
    assert_eq!(cpu.dar[15], 0x1003);
    let mut cpu = ppc::PpcCpu::default();
    cpu.gpr[3] = 0xfff9000d;
    cpu.gpr[4] = 0x12345678;
    assert_eq!(
        Abi::import(Service::FindWindow, &cpu, &memory).unwrap(),
        (expected, Reply::PpcR3)
    );
}
