mod support;
use systemless_future::{
    color_ports::Action,
    cpu::{Architecture, Cpu},
    load::{LoadedApplication, STACK_BASE},
    memory::{GuestPtr, Permissions},
    process::HEAP_SIZE,
    runtime::Stop,
};
const PORT: u32 = 0x50000000;
const FIELDS: [u32; 7] = [2, 24, 28, 32, 58, 62, 8];
fn app() -> LoadedApplication {
    let fixture = support::pef_program(
        &[0x4e800020],
        &[
            b"InitGraf",
            b"OpenCPort",
            b"InitCPort",
            b"CloseCPort",
            b"GetGDevice",
            b"OpenPort",
            b"PaintRect",
        ],
        64,
    );
    let mut a = LoadedApplication::pef(&fixture.bytes, None).unwrap();
    a.runtime
        .memory
        .map(GuestPtr(PORT), vec![0xcc; 512], Permissions::RW)
        .unwrap();
    a.runtime
        .memory
        .map(
            GuestPtr(0x51000000),
            vec![0xaa, 0, 0xaa, 1, 0xaa, 2, 0xa8, 0xa2],
            Permissions::RX,
        )
        .unwrap();
    ppc(&mut a, 0, 0x600);
    a
}
fn ppc(a: &mut LoadedApplication, index: u32, argument: u32) -> u32 {
    let tv = a
        .runtime
        .memory
        .u32(GuestPtr(a.info.toc_or_a5 + 8 + index * 4))
        .unwrap();
    let pc = a.runtime.memory.u32(GuestPtr(tv)).unwrap();
    a.runtime.cpu = Cpu::new(Architecture::PowerPc, pc, STACK_BASE + 0x8000);
    let Cpu::PowerPc(c) = &mut a.runtime.cpu else {
        panic!()
    };
    c.gpr[3] = argument;
    assert_eq!(a.runtime.advance(1).unwrap(), Stop::Budget);
    let Cpu::PowerPc(c) = &a.runtime.cpu else {
        panic!()
    };
    c.gpr[3]
}
fn native(a: &mut LoadedApplication, action: u32, port: u32) {
    let stack = STACK_BASE + 0x8000;
    a.runtime
        .memory
        .write(GuestPtr(stack), &port.to_be_bytes())
        .unwrap();
    a.runtime
        .memory
        .write(GuestPtr(stack + 4), &[0x5a; 4])
        .unwrap();
    a.runtime.cpu = Cpu::new(Architecture::M68k, 0x51000000 + action * 2, stack);
    let Cpu::M68k(c) = &mut a.runtime.cpu else {
        panic!()
    };
    c.dar[0] = 0x12345678;
    assert_eq!(a.runtime.advance(1).unwrap(), Stop::Budget);
    let Cpu::M68k(c) = &a.runtime.cpu else {
        panic!()
    };
    assert_eq!(c.dar[15], stack + 4);
    assert_eq!(c.dar[0], 0x12345678);
    assert_eq!(
        a.runtime.memory.read(GuestPtr(stack + 4), 4).unwrap(),
        [0x5a; 4]
    );
}
fn handles(a: &LoadedApplication, port: u32) -> [u32; 7] {
    FIELDS.map(|offset| a.runtime.memory.u32(GuestPtr(port + offset)).unwrap())
}
fn largest(a: &mut LoadedApplication) -> u32 {
    let (mut low, mut high) = (0, HEAP_SIZE);
    while low < high {
        let mid = low + (high - low).div_ceil(2);
        let pointer = a
            .runtime
            .process
            .new_pointer(&mut a.runtime.memory, mid, false)
            .unwrap();
        if pointer == 0 {
            high = mid - 1;
        } else {
            a.runtime.process.dispose_pointer(pointer);
            low = mid;
        }
    }
    low
}
fn call(a: &mut LoadedApplication, action: Action, port: u32) -> systemless_future::Result<()> {
    let mut manager = std::mem::take(&mut a.runtime.process.color_ports);
    let result = manager.call(
        &mut a.runtime.memory,
        &mut a.runtime.process,
        action,
        GuestPtr(port),
    );
    a.runtime.process.color_ports = manager;
    result
}
#[test]
fn real_cpus_share_color_port_graph_reset_without_allocation_and_preserve_borrowed_tables() {
    let mut a = app();
    a.runtime
        .memory
        .write(GuestPtr(0xda0), &[1, 2, 3, 4, 5, 6])
        .unwrap();
    native(&mut a, 0, PORT);
    assert_eq!(a.runtime.memory.u32(GuestPtr(0x600)).unwrap(), PORT);
    assert_eq!(a.runtime.memory.u16(GuestPtr(PORT + 6)).unwrap(), 0xc000);
    assert_eq!(a.runtime.memory.u16(GuestPtr(PORT + 14)).unwrap(), 0x8000);
    let first = handles(&a, PORT);
    assert_eq!(
        first
            .into_iter()
            .collect::<std::collections::BTreeSet<_>>()
            .len(),
        7
    );
    for (handle, size) in first.into_iter().zip([50, 10, 10, 28, 28, 28, 26]) {
        assert_eq!(
            a.runtime
                .process
                .handle_size(&a.runtime.memory, handle)
                .unwrap(),
            size
        );
    }
    let device = ppc(&mut a, 4, 0);
    let gd = a.runtime.memory.u32(GuestPtr(device)).unwrap();
    let device_pm = a.runtime.memory.u32(GuestPtr(gd + 22)).unwrap();
    let device_data = a.runtime.memory.u32(GuestPtr(device_pm)).unwrap();
    let table = a.runtime.memory.u32(GuestPtr(device_data + 42)).unwrap();
    let pm = a.runtime.memory.u32(GuestPtr(first[0])).unwrap();
    assert_ne!(first[0], device_pm);
    assert_eq!(
        a.runtime.memory.read(GuestPtr(pm), 50).unwrap(),
        a.runtime.memory.read(GuestPtr(device_data), 50).unwrap()
    );
    let vars = a.runtime.memory.u32(GuestPtr(first[6])).unwrap();
    assert_eq!(
        a.runtime.memory.read(GuestPtr(vars), 12).unwrap(),
        [0, 0, 0, 0, 0, 0, 1, 2, 3, 4, 5, 6]
    );
    for (index, value) in [(3, 0), (4, 255), (5, 255)] {
        let data = a.runtime.memory.u32(GuestPtr(first[index])).unwrap();
        assert_eq!(
            a.runtime.memory.read(GuestPtr(data + 20), 8).unwrap(),
            [value; 8]
        );
    }
    assert_eq!(ppc(&mut a, 1, PORT + 256), PORT + 256);
    assert_ne!(handles(&a, PORT + 256), first);
    a.runtime
        .memory
        .write(GuestPtr(PORT + 74), &99u16.to_be_bytes())
        .unwrap();
    let capacity = largest(&mut a);
    let fill = a
        .runtime
        .process
        .new_pointer(&mut a.runtime.memory, capacity, false)
        .unwrap();
    assert_ne!(fill, 0);
    assert_eq!(ppc(&mut a, 2, PORT), PORT);
    assert_eq!(handles(&a, PORT), first);
    assert_eq!(a.runtime.memory.u16(GuestPtr(PORT + 74)).unwrap(), 0);
    assert_eq!(a.runtime.memory.u32(GuestPtr(0x600)).unwrap(), PORT);
    a.runtime.process.dispose_pointer(fill);
    let custom = a
        .runtime
        .process
        .new_handle(&mut a.runtime.memory, 24, true)
        .unwrap();
    a.runtime
        .memory
        .write(GuestPtr(pm + 42), &custom.to_be_bytes())
        .unwrap();
    a.runtime
        .process
        .ports
        .unwrap()
        .set(&mut a.runtime.memory, GuestPtr(PORT + 256))
        .unwrap();
    let before = a.runtime.memory.read(GuestPtr(PORT), 108).unwrap().to_vec();
    native(&mut a, 2, PORT);
    assert_eq!(a.runtime.memory.read(GuestPtr(PORT), 108).unwrap(), before);
    assert_eq!(a.runtime.memory.u32(GuestPtr(0x600)).unwrap(), PORT + 256);
    for handle in first {
        assert_eq!(
            a.runtime
                .process
                .handle_size(&a.runtime.memory, handle)
                .unwrap(),
            0
        );
    }
    assert_eq!(
        a.runtime
            .process
            .handle_size(&a.runtime.memory, custom)
            .unwrap(),
        24
    );
    assert_eq!(
        a.runtime
            .process
            .handle_size(&a.runtime.memory, table)
            .unwrap(),
        24
    );
    assert!(call(&mut a, Action::Init, PORT).is_err());
    assert_eq!(ppc(&mut a, 3, PORT + 256), PORT + 256);
    assert_eq!(
        a.runtime
            .process
            .handle_size(&a.runtime.memory, table)
            .unwrap(),
        24
    );
}
#[test]
fn basic_init_is_inert_and_pen_normal_preserves_color_handles() {
    let mut a = app();
    ppc(&mut a, 5, PORT);
    let before = a.runtime.memory.read(GuestPtr(PORT), 108).unwrap().to_vec();
    a.runtime
        .memory
        .write(GuestPtr(0x220), &(-42i16).to_be_bytes())
        .unwrap();
    a.runtime.process.mem_error = -7;
    native(&mut a, 1, PORT);
    assert_eq!(a.runtime.memory.read(GuestPtr(PORT), 108).unwrap(), before);
    assert_eq!(a.runtime.memory.u16(GuestPtr(0x220)).unwrap() as i16, -42);
    a.runtime
        .memory
        .map(GuestPtr(0x52000000), vec![0; 108], Permissions::RX)
        .unwrap();
    native(&mut a, 1, 0x52000000);
    assert_eq!(a.runtime.memory.u32(GuestPtr(0x600)).unwrap(), PORT);
    ppc(&mut a, 1, PORT + 256);
    let before = a
        .runtime
        .memory
        .read(GuestPtr(PORT + 256), 108)
        .unwrap()
        .to_vec();
    let ports = a.runtime.process.ports.unwrap();
    ports.pen_normal(&mut a.runtime.memory).unwrap();
    assert!(ports
        .set_bits(&mut a.runtime.memory, GuestPtr(PORT + 2))
        .is_err());
    assert_eq!(
        a.runtime.memory.read(GuestPtr(PORT + 256), 108).unwrap(),
        before
    );
}
#[test]
fn partial_oom_rolls_back_without_publishing_and_bad_frames_preserve_stack_and_graph() {
    let mut a = app();
    ppc(&mut a, 4, 0);
    let capacity = largest(&mut a);
    let fill = a
        .runtime
        .process
        .new_pointer(&mut a.runtime.memory, capacity - 80, false)
        .unwrap();
    assert_ne!(fill, 0);
    let before = a.runtime.memory.read(GuestPtr(PORT), 108).unwrap().to_vec();
    native(&mut a, 0, PORT);
    assert_eq!(a.runtime.memory.u16(GuestPtr(0x220)).unwrap() as i16, -108);
    assert_eq!(a.runtime.memory.read(GuestPtr(PORT), 108).unwrap(), before);
    assert_eq!(a.runtime.memory.u32(GuestPtr(0x600)).unwrap(), 0);
    let remaining = a
        .runtime
        .process
        .new_pointer(&mut a.runtime.memory, 80, false)
        .unwrap();
    assert_ne!(remaining, 0);
    a.runtime.process.dispose_pointer(remaining);
    a.runtime.process.dispose_pointer(fill);
    assert_eq!(ppc(&mut a, 1, PORT), PORT);
    let graph = handles(&a, PORT);
    a.runtime
        .memory
        .write(GuestPtr(PORT + 24), &graph[2].to_be_bytes())
        .unwrap();
    assert!(call(&mut a, Action::Close, PORT).is_err());
    for (handle, size) in graph.into_iter().zip([50, 10, 10, 28, 28, 28, 26]) {
        assert_eq!(
            a.runtime
                .process
                .handle_size(&a.runtime.memory, handle)
                .unwrap(),
            size
        );
    }
    a.runtime
        .memory
        .write(GuestPtr(PORT + 24), &graph[1].to_be_bytes())
        .unwrap();
    let pattern = a.runtime.memory.u32(GuestPtr(graph[3])).unwrap();
    a.runtime
        .memory
        .write(GuestPtr(pattern), &1u16.to_be_bytes())
        .unwrap();
    assert!(call(&mut a, Action::Close, PORT).is_err());
    assert_eq!(
        a.runtime
            .process
            .handle_size(&a.runtime.memory, graph[0])
            .unwrap(),
        50
    );
    a.runtime
        .memory
        .write(GuestPtr(pattern), &0u16.to_be_bytes())
        .unwrap();
    a.runtime
        .memory
        .map(GuestPtr(0x53000000), vec![0; 107], Permissions::RW)
        .unwrap();
    let low_memory = a.runtime.memory.read(GuestPtr(0), 108).unwrap().to_vec();
    for action in [Action::Open, Action::Init, Action::Close] {
        assert!(call(&mut a, action, 0).is_err());
    }
    let stack = STACK_BASE + 0x8000;
    for port in [0u32, 0x53000000] {
        a.runtime
            .memory
            .write(GuestPtr(stack), &port.to_be_bytes())
            .unwrap();
        a.runtime.cpu = Cpu::new(Architecture::M68k, 0x51000000, stack);
        assert!(a.runtime.advance(1).is_err());
        let Cpu::M68k(c) = &a.runtime.cpu else {
            panic!()
        };
        assert_eq!(c.dar[15], stack);
    }
    assert_eq!(a.runtime.memory.read(GuestPtr(0), 108).unwrap(), low_memory);
}

#[test]
#[ignore = "requires a real wgpu adapter"]
fn color_port_drawing_uses_pixmap_and_attached_patterns_without_corrupting_graph() {
    use systemless_future::{
        graphics::{Graphics, Rect},
        quickdraw::{RectangleOp, SCREEN},
    };
    for use_native in [false, true] {
        let mut a = app();
        if use_native {
            native(&mut a, 0, PORT);
        } else {
            ppc(&mut a, 1, PORT);
        }
        let ports = a.runtime.process.ports.unwrap();
        let graph = handles(&a, PORT);
        let map = a.runtime.memory.u32(GuestPtr(graph[0])).unwrap();
        let pattern = a.runtime.memory.u32(GuestPtr(graph[3])).unwrap();
        let pen = a.runtime.memory.u32(GuestPtr(graph[4])).unwrap();
        let fill = a.runtime.memory.u32(GuestPtr(graph[5])).unwrap();
        let vars = a
            .runtime
            .memory
            .read(GuestPtr(PORT + 8), 8)
            .unwrap()
            .to_vec();
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
        let mut gpu = pollster::block_on(Graphics::new(&instance, None, 512, 342, SCREEN)).unwrap();
        gpu.palette[0] = [255; 4];
        gpu.palette[1] = [0, 0, 0, 255];
        a.runtime
            .memory
            .write(GuestPtr(0x300), &[0, 0, 0, 0, 0, 1, 0, 8])
            .unwrap();
        a.runtime.graphics = Some(gpu);
        let original_map = a.runtime.memory.read(GuestPtr(map), 50).unwrap().to_vec();
        a.runtime.memory.write(GuestPtr(map + 6), &[0; 8]).unwrap();
        a.runtime.memory.write(GuestPtr(0x300), &[0; 8]).unwrap();
        let empty_map = a.runtime.memory.read(GuestPtr(map), 50).unwrap().to_vec();
        let port_before = a.runtime.memory.read(GuestPtr(PORT), 108).unwrap().to_vec();
        let pixels_before = a.runtime.memory.read(SCREEN, 64).unwrap().to_vec();
        let gpu_before = a.runtime.graphics.as_ref().unwrap().read_rgba().unwrap();
        if use_native {
            native(&mut a, 3, 0x300);
        } else {
            ppc(&mut a, 6, 0x300);
        }
        assert_eq!(
            a.runtime.memory.read(GuestPtr(PORT), 108).unwrap(),
            port_before
        );
        assert_eq!(a.runtime.memory.read(GuestPtr(map), 50).unwrap(), empty_map);
        assert_eq!(a.runtime.memory.read(SCREEN, 64).unwrap(), pixels_before);
        assert_eq!(
            a.runtime.graphics.as_ref().unwrap().read_rgba().unwrap(),
            gpu_before
        );
        // A nonempty request still validates and rejects an empty bitmap.
        assert!(ports
            .paint_rect(
                a.runtime.graphics.as_ref().unwrap(),
                &mut a.runtime.memory,
                Rect::new(0, 0, 8, 1)
            )
            .is_err());
        a.runtime
            .memory
            .write(GuestPtr(map), &original_map)
            .unwrap();
        a.runtime
            .memory
            .write(GuestPtr(0x300), &[0, 0, 0, 0, 0, 1, 0, 8])
            .unwrap();
        if use_native {
            native(&mut a, 3, 0x300);
        } else {
            ppc(&mut a, 6, 0x300);
        }
        let gpu = a.runtime.graphics.take().unwrap();
        assert_eq!(a.runtime.memory.read(SCREEN, 2).unwrap(), [255, 0]);
        // Erase uses the attached background pattern, not the handle's bytes.
        a.runtime
            .memory
            .write(GuestPtr(pattern + 20), &[0xaa; 8])
            .unwrap();
        a.runtime
            .memory
            .write(GuestPtr(0x300), &[0, 0, 0, 0, 0, 1, 0, 8])
            .unwrap();
        ports
            .draw_rectangle(
                &mut a.runtime.memory,
                &gpu,
                GuestPtr(0x300),
                RectangleOp::Erase,
            )
            .unwrap();
        assert_eq!(a.runtime.memory.read(SCREEN, 2).unwrap(), [0xaa, 0]);
        a.runtime
            .memory
            .write(GuestPtr(pen + 20), &[0x55; 8])
            .unwrap();
        ports
            .paint_rect(&gpu, &mut a.runtime.memory, Rect::new(8, 0, 16, 1))
            .unwrap();
        assert_eq!(a.runtime.memory.read(SCREEN, 2).unwrap(), [0xaa, 0x55]);
        a.runtime
            .memory
            .write(GuestPtr(0x300), &[0, 0, 0, 16, 0, 1, 0, 24])
            .unwrap();
        ports
            .draw_rectangle(
                &mut a.runtime.memory,
                &gpu,
                GuestPtr(0x300),
                RectangleOp::Fill(GuestPtr(pen + 20)),
            )
            .unwrap();
        assert_eq!(
            a.runtime.memory.read(GuestPtr(fill + 20), 8).unwrap(),
            [0x55; 8]
        );
        assert_eq!(
            a.runtime.memory.read(GuestPtr(pen + 20), 8).unwrap(),
            [0x55; 8]
        );
        ports.pen_normal(&mut a.runtime.memory).unwrap();
        assert_eq!(
            a.runtime.memory.read(GuestPtr(pen + 20), 8).unwrap(),
            [255; 8]
        );
        ports.set_color(&mut a.runtime.memory, false, 30).unwrap();
        assert_eq!(
            a.runtime.memory.read(GuestPtr(PORT + 36), 6).unwrap(),
            [255; 6]
        );
        assert_eq!(a.runtime.memory.u32(GuestPtr(PORT + 80)).unwrap(), 0);
        ports
            .paint_rect(&gpu, &mut a.runtime.memory, Rect::new(0, 0, 8, 1))
            .unwrap();
        assert_eq!(a.runtime.memory.read(SCREEN, 3).unwrap(), [0, 0x55, 0x55]);
        let pixels = gpu.read_rgba().unwrap();
        assert_eq!(&pixels[..4], &[255; 4]);
        assert_eq!(&pixels[9 * 4..10 * 4], &[0, 0, 0, 255]);
        ports.set_color(&mut a.runtime.memory, false, 33).unwrap();
        // Snapshot overlap through a tagged destination PixMap, not a BitMap.
        a.runtime
            .memory
            .write(GuestPtr(0x300), &[0, 0, 0, 16, 0, 1, 0, 24])
            .unwrap();
        a.runtime
            .memory
            .write(GuestPtr(0x310), &[0, 0, 0, 24, 0, 1, 0, 32])
            .unwrap();
        ports
            .copy_bits(
                &mut a.runtime.memory,
                &gpu,
                systemless_future::quickdraw::CopyBits {
                    source: GuestPtr(map),
                    destination: GuestPtr(map),
                    source_rect: GuestPtr(0x300),
                    destination_rect: GuestPtr(0x310),
                    mode: 0,
                    mask: GuestPtr(0),
                },
            )
            .unwrap();
        assert_eq!(
            a.runtime.memory.read(SCREEN, 4).unwrap(),
            [0, 0x55, 0x55, 0x55]
        );
        ports.set_color(&mut a.runtime.memory, false, 33).unwrap();
        systemless_future::font::initialize(&mut a.runtime.memory, &mut a.runtime.process).unwrap();
        systemless_future::text::set(
            &mut a.runtime.memory,
            ports,
            systemless_future::text::Field::Size,
            12,
        )
        .unwrap();
        systemless_future::text::move_to(&mut a.runtime.memory, ports, 0, 24).unwrap();
        a.runtime
            .memory
            .write(GuestPtr(pen + 20), &[0x69; 8])
            .unwrap();
        systemless_future::text::draw(
            &mut a.runtime.memory,
            &mut a.runtime.process,
            ports,
            &gpu,
            systemless_future::text::Input::Character(b'A'),
        )
        .unwrap();
        assert!(a
            .runtime
            .memory
            .read(GuestPtr(SCREEN.0 + 64 * 8), 64 * 20)
            .unwrap()
            .iter()
            .any(|byte| *byte != 0));
        assert_eq!(
            a.runtime.memory.read(GuestPtr(pen + 20), 8).unwrap(),
            [0x69; 8]
        );
        assert!(a.runtime.memory.u16(GuestPtr(PORT + 50)).unwrap() > 0);
        ports
            .set_origin(&mut a.runtime.memory, &mut a.runtime.process, 4, 6)
            .unwrap();
        assert_eq!(
            a.runtime.memory.read(GuestPtr(map + 6), 4).unwrap(),
            [0, 6, 0, 4]
        );
        assert_eq!(a.runtime.memory.read(GuestPtr(PORT + 8), 8).unwrap(), vars);
        assert_eq!(handles(&a, PORT), graph);
        // Unsupported color destinations fail before touching pixels or handles.
        let before = a.runtime.memory.read(SCREEN, 64).unwrap().to_vec();
        a.runtime
            .memory
            .write(GuestPtr(map + 32), &8u16.to_be_bytes())
            .unwrap();
        assert!(ports
            .paint_rect(&gpu, &mut a.runtime.memory, Rect::new(4, 6, 12, 7))
            .is_err());
        assert_eq!(a.runtime.memory.read(SCREEN, 64).unwrap(), before);
        assert_eq!(handles(&a, PORT), graph);
    }
}

#[test]
fn both_cpus_open_color_ports_before_offscreen_pixels_are_attached() {
    use systemless_future::devices::Op;
    for architecture in [Architecture::M68k, Architecture::PowerPc] {
        let mut a = app();
        let main = ppc(&mut a, 4, 0);
        let gd = a.runtime.memory.u32(GuestPtr(main)).unwrap();
        let pm = a.runtime.memory.u32(GuestPtr(gd + 22)).unwrap();
        let pm_data = a.runtime.memory.u32(GuestPtr(pm)).unwrap();
        let mut copied = a
            .runtime
            .memory
            .read(GuestPtr(pm_data), 50)
            .unwrap()
            .to_vec();
        copied[..4].fill(0);
        let off_pm = a
            .runtime
            .process
            .new_handle(&mut a.runtime.memory, 50, true)
            .unwrap();
        let off_pm_data = a.runtime.memory.u32(GuestPtr(off_pm)).unwrap();
        a.runtime
            .memory
            .write(GuestPtr(off_pm_data), &copied)
            .unwrap();
        let off = a
            .runtime
            .process
            .new_handle(&mut a.runtime.memory, 62, true)
            .unwrap();
        let off_data = a.runtime.memory.u32(GuestPtr(off)).unwrap();
        a.runtime
            .memory
            .write(GuestPtr(off_data + 22), &off_pm.to_be_bytes())
            .unwrap();
        let mut manager = std::mem::take(&mut a.runtime.process.devices);
        manager
            .call(
                &mut a.runtime.memory,
                &mut a.runtime.process,
                Op::Select(off),
            )
            .unwrap();
        a.runtime.process.devices = manager;
        match architecture {
            Architecture::PowerPc => {
                ppc(&mut a, 1, PORT);
            }
            Architecture::M68k => native(&mut a, 0, PORT),
        }
        let port_pm = handles(&a, PORT)[0];
        let data = a.runtime.memory.u32(GuestPtr(port_pm)).unwrap();
        assert_eq!(a.runtime.memory.u32(GuestPtr(data)).unwrap(), 0);
        assert_eq!(a.runtime.memory.u32(GuestPtr(PORT + 80)).unwrap(), 1);
        assert_eq!(a.runtime.memory.u32(GuestPtr(PORT + 84)).unwrap(), 0);
        call(&mut a, Action::Close, PORT).unwrap();
    }
}
