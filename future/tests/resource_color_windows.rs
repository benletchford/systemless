mod support;
use support::{pef_program, resource, w16, w32};
use systemless_future::{
    abi::Abi,
    cpu::{Architecture, Cpu},
    display::Display,
    graphics::Graphics,
    load::{LoadedApplication, STACK_BASE},
    memory::{GuestPtr, Memory, Permissions},
    runtime::Stop,
    windows::{ResourceWindow, LIST},
};
const STACK: u32 = STACK_BASE + 0x8000;
fn wind(proc_id: u16) -> Vec<u8> {
    let mut b = vec![0; 22];
    for (at, value) in [(0, 40), (2, 40), (4, 80), (6, 100), (8, proc_id)] {
        w16(&mut b, at, value);
    }
    b[10] = 1;
    w32(&mut b, 14, 0x12345678);
    b[18..].copy_from_slice(b"\x03RGB");
    b
}
fn colors() -> Vec<u8> {
    let mut b = vec![0; 24];
    w16(&mut b, 6, 1);
    w16(&mut b, 10, 65535); // content red
    w16(&mut b, 16, 1);
    w16(&mut b, 20, 65535); // frame green
    b
}
fn setup() -> LoadedApplication {
    let mut duplicate = colors();
    w16(&mut duplicate, 16, 0);
    let fork = resource(vec![
        (*b"WIND", 42, None, wind(2)),
        (*b"wctb", 42, None, colors()),
        (*b"WIND", 43, None, wind(2)),
        (*b"WIND", 44, None, wind(2)),
        (*b"wctb", 44, None, duplicate),
        (*b"WIND", 45, None, wind(2)),
        (*b"wctb", 45, None, vec![0; 7]),
        (*b"WIND", 46, None, wind(4)),
        (*b"wctb", 46, None, colors()),
        (*b"WIND", 47, None, vec![0; 18]),
    ]);
    let f = pef_program(
        &[0x4e800020],
        &[
            b"InitGraf",
            b"InitWindows",
            b"GetNewCWindow",
            b"DisposeWindow",
            b"CloseWindow",
            b"HideWindow",
            b"ShowWindow",
            b"GetNewWindow",
        ],
        512,
    );
    let mut a = LoadedApplication::pef(&f.bytes, Some(&fork))
        .unwrap()
        .with_display(Display::indexed8(128, 100).unwrap())
        .unwrap();
    a.runtime
        .memory
        .map(GuestPtr(0x50000000), vec![0xaa, 0x46], Permissions::RX)
        .unwrap();
    invoke(&mut a, 0, &[0x600]);
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
    a.runtime.graphics = Some(
        pollster::block_on(Graphics::new(&instance, None, 128, 100, GuestPtr(0x4000))).unwrap(),
    );
    invoke(&mut a, 1, &[]);
    a
}
fn invoke(a: &mut LoadedApplication, index: u32, args: &[u32]) -> u32 {
    let tv = a
        .runtime
        .memory
        .u32(GuestPtr(a.info.toc_or_a5 + 8 + index * 4))
        .unwrap();
    let pc = a.runtime.memory.u32(GuestPtr(tv)).unwrap();
    a.runtime.cpu = Cpu::new(Architecture::PowerPc, pc, STACK);
    if let Cpu::PowerPc(c) = &mut a.runtime.cpu {
        for (i, arg) in args.iter().enumerate() {
            c.gpr[3 + i] = *arg;
        }
    }
    assert_eq!(a.runtime.advance(1).unwrap(), Stop::Budget);
    let Cpu::PowerPc(c) = &a.runtime.cpu else {
        panic!()
    };
    c.gpr[3]
}
fn native(a: &mut LoadedApplication, id: u16, storage: u32) -> u32 {
    let mut frame = Vec::new();
    frame.extend(u32::MAX.to_be_bytes());
    frame.extend(storage.to_be_bytes());
    frame.extend(id.to_be_bytes());
    frame.extend([0xa5; 8]);
    a.runtime.memory.write(GuestPtr(STACK), &frame).unwrap();
    a.runtime.cpu = Cpu::new(Architecture::M68k, 0x50000000, STACK);
    if let Cpu::M68k(c) = &mut a.runtime.cpu {
        c.dar[0] = 0x12345678;
    }
    assert_eq!(a.runtime.advance(1).unwrap(), Stop::Budget);
    let Cpu::M68k(c) = &a.runtime.cpu else {
        panic!()
    };
    assert_eq!(c.dar[15], STACK + 10);
    assert_eq!(c.dar[0], 0x12345678);
    assert_eq!(
        a.runtime.memory.read(GuestPtr(STACK + 14), 4).unwrap(),
        [0xa5; 4]
    );
    a.runtime.memory.u32(GuestPtr(STACK + 10)).unwrap()
}
fn pixel(a: &mut LoadedApplication, x: usize, y: usize) -> Vec<u8> {
    a.runtime.refresh_screen().unwrap();
    a.runtime.graphics.as_ref().unwrap().read_rgba().unwrap()
        [(y * 128 + x) * 4..(y * 128 + x) * 4 + 4]
        .to_vec()
}
#[test]
#[ignore = "requires a real wgpu adapter"]
fn both_abis_create_owned_color_graphs_and_copy_live_resource_colors_into_gpu_drawing() {
    let mut a = setup();
    let first = invoke(&mut a, 2, &[42, 0, u32::MAX]);
    assert_ne!(first, 0);
    assert_eq!(a.runtime.memory.u16(GuestPtr(first + 6)).unwrap(), 0xc000);
    assert_eq!(
        a.runtime.memory.u32(GuestPtr(first + 152)).unwrap(),
        0x12345678
    );
    assert_eq!(
        a.runtime.memory.read(GuestPtr(first + 42), 6).unwrap(),
        [255, 255, 0, 0, 0, 0]
    );
    assert_eq!(pixel(&mut a, 60, 60), [255, 0, 0, 255]);
    assert_eq!(pixel(&mut a, 39, 60), [0, 255, 0, 255]);
    let title = a.runtime.memory.u32(GuestPtr(first + 134)).unwrap();
    let pm = a.runtime.memory.u32(GuestPtr(first + 2)).unwrap();
    let clut = a
        .runtime
        .memory
        .u32(GuestPtr(a.runtime.memory.u32(GuestPtr(pm)).unwrap() + 42))
        .unwrap();
    let h = a
        .runtime
        .process
        .get_resource(&mut a.runtime.memory, *b"wctb", 42)
        .unwrap();
    let data = a.runtime.memory.u32(GuestPtr(h)).unwrap();
    a.runtime
        .memory
        .write(GuestPtr(data + 10), &[0, 0, 0, 0, 255, 255])
        .unwrap(); // new content blue
    invoke(&mut a, 5, &[first]);
    invoke(&mut a, 6, &[first]);
    assert_eq!(pixel(&mut a, 60, 60), [255, 0, 0, 255]); // existing window owns its settings
    invoke(&mut a, 3, &[first]);
    for handle in [pm, title] {
        a.runtime
            .process
            .handle_size(&a.runtime.memory, handle)
            .unwrap();
        assert_ne!(a.runtime.process.mem_error, 0);
    }
    a.runtime
        .process
        .handle_size(&a.runtime.memory, clut)
        .unwrap();
    assert_eq!(a.runtime.process.mem_error, 0);
    let storage = a
        .runtime
        .process
        .new_pointer(&mut a.runtime.memory, 156, true)
        .unwrap();
    let second = native(&mut a, 42, storage);
    assert_eq!(second, storage);
    assert_eq!(pixel(&mut a, 60, 60), [0, 0, 255, 255]);
    assert_eq!(pixel(&mut a, 39, 60), [0, 255, 0, 255]);
    invoke(&mut a, 4, &[second]);
    assert_eq!(a.runtime.process.pointer_size(storage), 156);
    assert_eq!(a.runtime.memory.u32(LIST).unwrap(), 0);
    a.runtime.process.dispose_pointer(storage);
    let defaults = invoke(&mut a, 2, &[43, 0, u32::MAX]);
    assert_ne!(defaults, 0);
    assert_eq!(pixel(&mut a, 60, 60), [255, 255, 255, 255]);
    invoke(&mut a, 3, &[defaults]);
    let basic = invoke(&mut a, 7, &[42, 0, u32::MAX]);
    assert_eq!(a.runtime.memory.u16(GuestPtr(basic + 6)).unwrap(), 0xc000);
    assert_eq!(pixel(&mut a, 60, 60), [0, 0, 255, 255]);
    invoke(&mut a, 3, &[basic]);
}
#[test]
#[ignore = "requires a real wgpu adapter"]
fn malformed_colors_and_unsupported_document_themes_fail_before_publishing_a_window() {
    let mut a = setup();
    for (id, expected) in [(44, "duplicate"), (45, "header"), (46, "title/frame")] {
        let mut manager = std::mem::take(&mut a.runtime.process.windows);
        let result = manager.from_resource(
            &mut a.runtime.memory,
            &mut a.runtime.process,
            a.runtime.graphics.as_ref().unwrap(),
            ResourceWindow {
                id,
                storage: GuestPtr(0),
                behind: u32::MAX,
                color: true,
            },
        );
        a.runtime.process.windows = manager;
        assert!(result.unwrap_err().to_string().contains(expected));
        assert_eq!(a.runtime.memory.u32(LIST).unwrap(), 0);
    }
    assert_eq!(invoke(&mut a, 2, &[47, 0, u32::MAX]), 0);
    assert_eq!(native(&mut a, 999, 0), 0);
    assert_eq!(a.runtime.memory.u32(LIST).unwrap(), 0);
}
#[test]
fn resource_window_frame_faults_preserve_native_stack_and_result_bytes() {
    for opcode in [0xaa46, 0xa9bd] {
        for size in [9, 13, 14] {
            let mut m = Memory::default();
            m.map(GuestPtr(STACK), vec![0xa5; size], Permissions::RX)
                .unwrap();
            let mut c = m68k::CpuCore::default();
            c.dar[15] = STACK;
            assert!(Abi::trap(opcode, &mut c, &m).is_err());
            assert_eq!(c.dar[15], STACK);
            assert_eq!(m.read(GuestPtr(STACK), size).unwrap(), vec![0xa5; size]);
        }
    }
}
