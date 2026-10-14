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
