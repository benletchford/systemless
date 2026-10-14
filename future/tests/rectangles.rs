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
