mod support;
use systemless_future::{
    abi::{Abi, Reply, Service},
    cpu::{Architecture, Call, Cpu},
    graphics::{Graphics, Rect},
    lines,
    load::{LoadedApplication, STACK_BASE},
    memory::{GuestPtr, Memory, Permissions},
    process::Process,
    quickdraw::Ports,
    runtime::Stop,
};
fn setup() -> (Memory, Process, Ports) {
    let mut m = Memory::default();
    m.map(GuestPtr(0x1000), vec![0; 512], Permissions::RW)
        .unwrap();
    m.map(GuestPtr(0x2000), vec![0; 16], Permissions::RW)
        .unwrap();
    let mut b = 0x2000u32.to_be_bytes().to_vec();
    b.extend([0, 2, 0, 0, 0, 0, 0, 8, 0, 8]);
    m.write(GuestPtr(0x1000), &b).unwrap();
    let ports = Ports {
        current_slot: GuestPtr(0x1020),
        screen_bitmap: GuestPtr(0x1000),
    };
    let mut p = Process::default();
    ports.open(&mut m, &mut p, GuestPtr(0x1080)).unwrap();
    (m, p, ports)
}
fn word(m: &mut Memory, p: u32, v: i16) {
    m.write(GuestPtr(p), &v.to_be_bytes()).unwrap();
}
fn position(m: &mut Memory, p: u32, h: i16, v: i16) {
    word(m, p + 48, v);
    word(m, p + 50, h);
}
#[test]
fn signed_endpoint_geometry_is_bounded_and_negative_sizes_are_errors() {
    let g = lines::geometry((i16::MIN, i16::MIN), (i16::MAX, i16::MAX), 32767, 32767).unwrap();
    assert_eq!(g.steps, 65535);
    assert_eq!(g.minor_delta, 65535);
    assert_eq!(lines::bounds(g), Rect::new(-32768, -32768, 65534, 65534));
    assert!(lines::geometry((0, 0), (1, 1), -1, 1).is_err());
}
#[test]
fn line_abis_decode_words_void_results_and_fault_before_popping_short_arguments() {
    let mut m = Memory::default();
    m.map(GuestPtr(0x1000), vec![0xff, 0xfe, 0x80, 1], Permissions::RX)
        .unwrap();
    for (op, relative) in [(0xa891, false), (0xa892, true)] {
        let mut cpu = Cpu::new(Architecture::M68k, 0, 0x1000);
        let Cpu::M68k(c) = &mut cpu else { panic!() };
        assert_eq!(
            Abi::trap(op, c, &m).unwrap(),
            (
                Call::PenLine {
                    h: -32767,
                    v: -2,
                    relative
                },
                Reply::None
            )
        );
        assert_eq!(c.dar[15], 0x1004);
    }
    let mut cpu = Cpu::new(Architecture::PowerPc, 0, 0x8000);
    let Cpu::PowerPc(c) = &mut cpu else { panic!() };
    c.gpr[3] = 0x12348001;
    c.gpr[4] = 0xabcdfffe;
    for (service, relative) in [(Service::LineTo, false), (Service::Line, true)] {
        assert_eq!(
            Abi::import(service, c, &m).unwrap(),
            (
                Call::PenLine {
                    h: -32767,
                    v: -2,
                    relative
                },
                Reply::None
            )
        );
    }
    m.map(GuestPtr(0x2000), vec![0; 3], Permissions::RW)
        .unwrap();
    let mut cpu = Cpu::new(Architecture::M68k, 0, 0x2000);
    let Cpu::M68k(c) = &mut cpu else { panic!() };
    assert!(Abi::trap(0xa891, c, &m).is_err());
    assert_eq!(c.dar[15], 0x2000);
}
#[test]
fn invisible_and_zero_size_pens_advance_without_gpu_and_output_faults_preserve_state() {
    let (mut m, _p, ports) = setup();
    word(&mut m, 0x1080 + 66, -1);
    ports.line(&mut m, None, i16::MIN, i16::MAX, false).unwrap();
    ports.line(&mut m, None, -1, 1, true).unwrap();
    assert_eq!(
        m.read(GuestPtr(0x1080 + 48), 4).unwrap(),
        [0x7f, 0xff, 0x80, 0]
    );
    word(&mut m, 0x1080 + 66, 0);
    word(&mut m, 0x1080 + 52, 0);
    ports.line(&mut m, None, 3, 4, false).unwrap();
    assert_eq!(m.read(GuestPtr(0x1080 + 48), 4).unwrap(), [0, 4, 0, 3]);
    let record = m.read(GuestPtr(0x1080), 108).unwrap().to_vec();
    m.map(GuestPtr(0x3000), record.clone(), Permissions::RX)
        .unwrap();
    ports.set(&mut m, GuestPtr(0x3000)).unwrap();
    assert!(ports.line(&mut m, None, 5, 6, false).is_err());
    assert_eq!(m.read(GuestPtr(0x3000), 108).unwrap(), record);
    assert_eq!(m.read(GuestPtr(0x2000), 16).unwrap(), [0; 16]);
}
// Independent floating-point point placement is used only by the test oracle.
fn coverage(from: (i16, i16), to: (i16, i16), pw: i32, ph: i32) -> [[bool; 8]; 8] {
    let ((sx, sy), (ex, ey)) = if from.1 <= to.1 {
        (from, to)
    } else {
        (to, from)
    };
    let dx = i32::from(ex) - i32::from(sx);
    let dy = i32::from(ey) - i32::from(sy);
    let steps = dx.abs().max(dy);
    let mut result = [[false; 8]; 8];
    for t in 0..=steps {
        let ratio = if steps == 0 {
            0.0
        } else {
            f64::from(t) / f64::from(steps)
        };
        let x = i32::from(sx) + (f64::from(dx) * ratio).round() as i32;
        let y = i32::from(sy) + (f64::from(dy) * ratio).round() as i32;
        for (yy, row) in result.iter_mut().enumerate() {
            for (xx, hit) in row.iter_mut().enumerate() {
                *hit |= (xx as i32) >= x
                    && (xx as i32) < x + pw
                    && (yy as i32) >= y
                    && (yy as i32) < y + ph;
            }
        }
    }
    result
}
#[test]
#[ignore = "requires a real wgpu adapter"]
fn gpu_lines_match_point_union_in_all_octants_without_repeated_xor_and_clip_holes() {
    let (mut m, mut process, ports) = setup();
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
    let gpu = pollster::block_on(Graphics::new(&instance, None, 8, 8, GuestPtr(0x4000))).unwrap();
    let pattern = [0xaa, 0x55, 0xf0, 0x0f, 0x33, 0xcc, 0xff, 0];
    m.write(GuestPtr(0x1080 + 58), &pattern).unwrap();
    let pairs = [
        ((0, 0), (7, 7)),
        ((7, 0), (0, 7)),
        ((0, 4), (7, 4)),
        ((4, 7), (4, 0)),
        ((4, 4), (4, 4)),
        ((1, 0), (4, 7)),
        ((4, 0), (1, 7)),
        ((0, 1), (7, 4)),
        ((7, 1), (0, 4)),
        ((-3, 2), (6, 5)),
    ];
    for (pw, ph) in [(1, 1), (3, 2), (2, 4)] {
        word(&mut m, 0x1080 + 54, pw);
        word(&mut m, 0x1080 + 52, ph);
        for mode in [8, 10] {
            word(&mut m, 0x1080 + 56, mode);
            for (from, to) in pairs.into_iter().flat_map(|(a, b)| [(a, b), (b, a)]) {
                let initial = [
                    0x5a, 0xa5, 0x5a, 0xa5, 0x5a, 0xa5, 0x5a, 0xa5, 0x5a, 0xa5, 0x5a, 0xa5, 0x5a,
                    0xa5, 0x5a, 0xa5,
                ];
                m.write(GuestPtr(0x2000), &initial).unwrap();
                position(&mut m, 0x1080, from.0, from.1);
                ports.line(&mut m, Some(&gpu), to.0, to.1, false).unwrap();
                let hit = coverage(from, to, i32::from(pw), i32::from(ph));
                let mut expected = initial;
                for y in 0..8 {
                    for (x, &covered) in hit[y].iter().enumerate() {
                        if covered {
                            let bit = (pattern[y] >> (7 - x)) & 1;
                            let old = (initial[y * 2] >> (7 - x)) & 1;
                            let value = if mode == 10 { old ^ bit } else { bit };
                            expected[y * 2] =
                                (expected[y * 2] & !(1 << (7 - x))) | (value << (7 - x));
                        }
                    }
                }
                assert_eq!(
                    m.read(GuestPtr(0x2000), 16).unwrap(),
                    expected,
                    "{from:?}->{to:?} pen {pw},{ph} mode {mode}"
                );
            }
        }
    }
    let clip = m.u32(GuestPtr(0x1080 + 28)).unwrap();
    let words = [
        44i16, 0, 0, 8, 8, 0, 0, 8, 32767, 2, 2, 6, 32767, 6, 2, 6, 32767, 8, 0, 8, 32767, 32767,
    ];
    process.set_handle_size(&mut m, clip, 44).unwrap();
    let data = m.u32(GuestPtr(clip)).unwrap();
    m.write(
        GuestPtr(data),
        &words
            .iter()
            .flat_map(|w| w.to_be_bytes())
            .collect::<Vec<_>>(),
    )
    .unwrap();
    m.write(GuestPtr(0x2000), &[0; 16]).unwrap();
    m.write(GuestPtr(0x1080 + 58), &[0xff; 8]).unwrap();
    word(&mut m, 0x1080 + 56, 10);
    word(&mut m, 0x1080 + 54, 3);
    word(&mut m, 0x1080 + 52, 3);
    for iteration in 0..2 {
        position(&mut m, 0x1080, 0, 0);
        ports.line(&mut m, Some(&gpu), 7, 7, false).unwrap();
        if iteration == 0 {
            let pixels = m.read(GuestPtr(0x2000), 16).unwrap();
            assert_ne!(pixels[0] & 0x80, 0);
            for y in 2..6 {
                for x in 2..6 {
                    assert_eq!(pixels[y * 2] & (0x80 >> x), 0, "hole {x},{y}");
                }
            }
        }
    }
    assert_eq!(m.read(GuestPtr(0x2000), 16).unwrap(), [0; 16]);
}
fn invoke(app: &mut LoadedApplication, index: u32, args: &[u32]) -> u32 {
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
    for (i, &a) in args.iter().enumerate() {
        c.gpr[3 + i] = a;
    }
    assert_eq!(app.runtime.advance(1).unwrap(), Stop::Budget);
    let Cpu::PowerPc(c) = &app.runtime.cpu else {
        panic!()
    };
    c.gpr[3]
}
#[test]
#[ignore = "requires a real wgpu adapter"]
fn cpu_adapters_share_line_pixels_relative_pen_state_and_extreme_endpoints() {
    let f = support::pef_program(
        &[0x4e800020],
        &[b"InitGraf", b"OpenPort", b"SetPortBits", b"LineTo", b"Line"],
        0,
    );
    let mut app = LoadedApplication::pef(&f.bytes, None).unwrap();
    app.runtime
        .memory
        .map(GuestPtr(0x1000), vec![0; 0x1000], Permissions::RW)
        .unwrap();
    invoke(&mut app, 0, &[0x600]);
    invoke(&mut app, 1, &[0x1000]);
    let mut bits = 0x1200u32.to_be_bytes().to_vec();
    bits.extend([0, 2, 0, 0, 0, 0, 0, 8, 0, 8]);
    app.runtime.memory.write(GuestPtr(0x1100), &bits).unwrap();
    invoke(&mut app, 2, &[0x1100]);
    let ports = app.runtime.process.ports.unwrap();
    ports.port_size(&mut app.runtime.memory, 8, 8).unwrap();
    word(&mut app.runtime.memory, 0x1000 + 52, 2);
    word(&mut app.runtime.memory, 0x1000 + 54, 3);
    word(&mut app.runtime.memory, 0x1000 + 56, 10);
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
    app.runtime.graphics =
        Some(pollster::block_on(Graphics::new(&instance, None, 8, 8, GuestPtr(0x4000))).unwrap());
    assert_eq!(invoke(&mut app, 3, &[7, 7]), 7);
    assert_eq!(invoke(&mut app, 4, &[(-7i32) as u32, 0]), (-7i32) as u32);
    let expected = app
        .runtime
        .memory
        .read(GuestPtr(0x1200), 16)
        .unwrap()
        .to_vec();
    assert_eq!(
        app.runtime.memory.read(GuestPtr(0x1030), 4).unwrap(),
        [0, 7, 0, 0]
    );
    app.runtime
        .memory
        .write(GuestPtr(0x1200), &[0; 16])
        .unwrap();
    position(&mut app.runtime.memory, 0x1000, 0, 0);
    app.runtime
        .memory
        .map(
            GuestPtr(0x50000000),
            vec![0xa8, 0x91, 0xa8, 0x92],
            Permissions::RX,
        )
        .unwrap();
    let stack = STACK_BASE + 0x8000;
    app.runtime
        .memory
        .write(GuestPtr(stack), &[0, 7, 0, 7, 0, 0, 0xff, 0xf9, 0xa5, 0x5a])
        .unwrap();
    app.runtime.cpu = Cpu::new(Architecture::M68k, 0x50000000, stack);
    let Cpu::M68k(c) = &mut app.runtime.cpu else {
        panic!()
    };
    c.dar[0] = 0x12345678;
    assert_eq!(app.runtime.advance(2).unwrap(), Stop::Budget);
    let Cpu::M68k(c) = &app.runtime.cpu else {
        panic!()
    };
    assert_eq!(c.dar[15], stack + 8);
    assert_eq!(c.dar[0], 0x12345678);
    assert_eq!(
        app.runtime.memory.read(GuestPtr(stack + 8), 2).unwrap(),
        [0xa5, 0x5a]
    );
    assert_eq!(
        app.runtime.memory.read(GuestPtr(0x1200), 16).unwrap(),
        expected
    );
    assert_eq!(
        app.runtime.memory.read(GuestPtr(0x1030), 4).unwrap(),
        [0, 7, 0, 0]
    );
    word(&mut app.runtime.memory, 0x1000 + 52, 1);
    word(&mut app.runtime.memory, 0x1000 + 54, 1);
    word(&mut app.runtime.memory, 0x1000 + 56, 8);
    position(&mut app.runtime.memory, 0x1000, i16::MIN, i16::MIN);
    app.runtime
        .memory
        .write(GuestPtr(0x1200), &[0; 16])
        .unwrap();
    ports
        .line(
            &mut app.runtime.memory,
            app.runtime.graphics.as_ref(),
            i16::MAX,
            i16::MAX,
            false,
        )
        .unwrap();
    let mut diagonal = [0; 16];
    for y in 0..8 {
        diagonal[y * 2] = 0x80 >> y;
    }
    assert_eq!(
        app.runtime.memory.read(GuestPtr(0x1200), 16).unwrap(),
        diagonal
    );
}
