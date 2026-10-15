mod support;
use systemless_future::{
    abi::Abi,
    cpu::{Architecture, Cpu},
    load::{LoadedApplication, STACK_BASE},
    memory::{GuestPtr, Memory, Permissions},
    rectangles::{self, Op},
    runtime::Stop,
};
#[test]
fn rectangle_mutation_preserves_signed_wrapping_and_is_atomic() {
    let mut m = Memory::default();
    m.map(GuestPtr(0x1000), vec![0xaa; 12], Permissions::RW)
        .unwrap();
    m.map(GuestPtr(0x2000), vec![0xbb; 8], Permissions::RX)
        .unwrap();
    rectangles::call(
        &mut m,
        Op::Set {
            pointer: GuestPtr(0x1000),
            edges: [32767, -32768, -7, 6],
        },
    )
    .unwrap();
    rectangles::call(
        &mut m,
        Op::Offset {
            pointer: GuestPtr(0x1000),
            dh: -1,
            dv: 1,
        },
    )
    .unwrap();
    assert_eq!(
        m.read(GuestPtr(0x1000), 8).unwrap(),
        [0x80, 0, 0x7f, 0xff, 0xff, 0xfa, 0, 5]
    );
    rectangles::call(
        &mut m,
        Op::Inset {
            pointer: GuestPtr(0x1000),
            dh: 3,
            dv: 4,
        },
    )
    .unwrap();
    assert_eq!(
        m.read(GuestPtr(0x1000), 8).unwrap(),
        [0x80, 4, 0x80, 2, 0xff, 0xf6, 0, 2]
    );
    assert_eq!(m.read(GuestPtr(0x1008), 4).unwrap(), [0xaa; 4]);
    assert!(rectangles::call(
        &mut m,
        Op::Set {
            pointer: GuestPtr(0x2000),
            edges: [1; 4]
        }
    )
    .is_err());
    assert_eq!(m.read(GuestPtr(0x2000), 8).unwrap(), [0xbb; 8]);
    let mut cpu = Cpu::new(Architecture::M68k, 0, 0x1000);
    let Cpu::M68k(c) = &mut cpu else { panic!() };
    m.write(GuestPtr(0x1008), &0x2000u32.to_be_bytes()).unwrap();
    assert!(Abi::trap(0xa8a7, c, &m).is_err());
    assert_eq!(c.dar[15], 0x1000);
}
#[test]
fn rectangle_abis_share_live_bytes_and_preserve_pascal_argument_order() {
    let f = support::pef_program(&[0x4e800020], &[b"SetRect"], 32);
    let mut app = LoadedApplication::pef(&f.bytes, None).unwrap();
    let tv = app
        .runtime
        .memory
        .u32(GuestPtr(app.info.toc_or_a5 + 8))
        .unwrap();
    let pc = app.runtime.memory.u32(GuestPtr(tv)).unwrap();
    let stack = STACK_BASE + 0x8000;
    app.runtime.cpu = Cpu::new(Architecture::PowerPc, pc, stack);
    let Cpu::PowerPc(c) = &mut app.runtime.cpu else {
        panic!()
    };
    c.gpr[3] = 0x300;
    c.gpr[4] = (-3i32) as u32;
    c.gpr[5] = (-2i32) as u32;
    c.gpr[6] = 9;
    c.gpr[7] = 8;
    assert_eq!(app.runtime.advance(1).unwrap(), Stop::Budget);
    let expected = app
        .runtime
        .memory
        .read(GuestPtr(0x300), 8)
        .unwrap()
        .to_vec();
    let Cpu::PowerPc(c) = &app.runtime.cpu else {
        panic!()
    };
    assert_eq!(c.gpr[3], 0x300);
    app.runtime
        .memory
        .map(GuestPtr(0x50000000), vec![0xa8, 0xa7], Permissions::RX)
        .unwrap();
    let mut frame = [8i16, 9, -2, -3]
        .into_iter()
        .flat_map(i16::to_be_bytes)
        .collect::<Vec<_>>();
    frame.extend(0x320u32.to_be_bytes());
    frame.extend([0xa5; 4]);
    app.runtime.memory.write(GuestPtr(stack), &frame).unwrap();
    app.runtime.cpu = Cpu::new(Architecture::M68k, 0x50000000, stack);
    assert_eq!(app.runtime.advance(1).unwrap(), Stop::Budget);
    let Cpu::M68k(c) = &app.runtime.cpu else {
        panic!()
    };
    assert_eq!(c.dar[15], stack + 12);
    assert_eq!(
        app.runtime.memory.read(GuestPtr(0x320), 8).unwrap(),
        expected
    );
    assert_eq!(
        app.runtime.memory.u32(GuestPtr(stack + 12)).unwrap(),
        0xa5a5a5a5
    );
}

#[test]
fn point_in_rectangle_edges_and_boolean_results_match_both_real_cpu_adapters() {
    let fixture = support::pef_program(&[0x4e800020], &[b"PtInRect"], 16);
    let mut app = LoadedApplication::pef(&fixture.bytes, None).unwrap();
    app.runtime
        .memory
        .map(GuestPtr(0x50000000), vec![0xa8, 0xad], Permissions::RX)
        .unwrap();
    let vector = app
        .runtime
        .memory
        .u32(GuestPtr(app.info.toc_or_a5 + 8))
        .unwrap();
    let gate = app.runtime.memory.u32(GuestPtr(vector)).unwrap();
    let stack = STACK_BASE + 0x8000;
    for (edges, point, expected) in [
        ([-7i16, -9, 11, 13], [-7i16, -9], true),
        ([-7, -9, 11, 13], [10, 12], true),
        ([-7, -9, 11, 13], [11, 12], false),
        ([-7, -9, 11, 13], [10, 13], false),
        ([-7, -9, 11, 13], [-8, -9], false),
        ([-7, -9, 11, 13], [-7, -10], false),
        ([0, 0, 0, 4], [0, 1], false),
        ([4, 3, 0, -1], [2, 1], false),
        ([i16::MIN, i16::MIN, i16::MAX, i16::MAX], [0, -1], true),
    ] {
        let bytes = edges
            .into_iter()
            .flat_map(i16::to_be_bytes)
            .collect::<Vec<_>>();
        app.runtime.memory.write(GuestPtr(0x320), &bytes).unwrap();
        for arch in [Architecture::M68k, Architecture::PowerPc] {
            let mut args = 0x320u32.to_be_bytes().to_vec();
            args.extend(point.into_iter().flat_map(i16::to_be_bytes));
            args.extend([0xa5; 4]);
            app.runtime.memory.write(GuestPtr(stack), &args).unwrap();
            app.runtime.cpu = Cpu::new(
                arch,
                if arch == Architecture::M68k {
                    0x50000000
                } else {
                    gate
                },
                stack,
            );
            if let Cpu::PowerPc(c) = &mut app.runtime.cpu {
                c.gpr[3] = ((point[0] as u16 as u32) << 16) | point[1] as u16 as u32;
                c.gpr[4] = 0x320;
            }
            assert_eq!(app.runtime.advance(1).unwrap(), Stop::Budget);
            match &app.runtime.cpu {
                Cpu::M68k(c) => {
                    assert_eq!(c.dar[15], stack + 8);
                    assert_eq!(
                        app.runtime.memory.read(GuestPtr(stack + 8), 4).unwrap(),
                        [u8::from(expected), 0xa5, 0xa5, 0xa5]
                    );
                }
                Cpu::PowerPc(c) => assert_eq!(c.gpr[3], u32::from(expected)),
            }
            assert_eq!(app.runtime.memory.read(GuestPtr(0x320), 8).unwrap(), bytes);
        }
    }
}
#[test]
fn point_in_rectangle_rejects_invalid_result_storage_before_stack_pop() {
    let mut m = Memory::default();
    m.map(GuestPtr(0x1000), vec![0; 8], Permissions::RW)
        .unwrap();
    let mut cpu = m68k::CpuCore::default();
    cpu.dar[15] = 0x1000;
    assert!(Abi::trap(0xa8ad, &mut cpu, &m).is_err());
    assert_eq!(cpu.dar[15], 0x1000);
    assert!(rectangles::contains(&m, [0, 0], GuestPtr(0x2000)).is_err());
}
