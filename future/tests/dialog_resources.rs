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
    support::resource(vec![
        (*b"DLOG", 128, None, dlog),
        (*b"DITL", 128, None, ditl),
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
            vec![0xa9, 0x7c, 0xa9, 0x8d, 0xa9, 0x82, 0xa9, 0x83],
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
    a.runtime.graphics = Some(
        pollster::block_on(Graphics::new(&instance, None, 512, 342, GuestPtr(0x4000))).unwrap(),
    );
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
    let mut a = setup(16);
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
