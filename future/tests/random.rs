mod support;
use support::*;
use systemless_future::{
    cpu::{Architecture, Cpu},
    load::{LoadedApplication, STACK_BASE},
    memory::{GuestPtr, Memory, Permissions},
    quickdraw::Ports,
    runtime::Stop,
};
#[test]
fn both_cpu_adapters_advance_one_guest_seed_without_a_port() {
    let f = pef_program(&[0x4e800020], &[b"InitGraf", b"Random"], 512);
    let mut app = LoadedApplication::pef(&f.bytes, None).unwrap();
    let base = app.info.toc_or_a5 + 64;
    let stack = STACK_BASE + 0x8000;
    let gate = |app: &LoadedApplication, index: u32| {
        let vector = app
            .runtime
            .memory
            .u32(GuestPtr(app.info.toc_or_a5 + 8 + index * 4))
            .unwrap();
        app.runtime.memory.u32(GuestPtr(vector)).unwrap()
    };
    app.runtime.cpu = Cpu::new(Architecture::PowerPc, gate(&app, 0), stack);
    if let Cpu::PowerPc(c) = &mut app.runtime.cpu {
        c.gpr[3] = base + 202;
    }
    assert_eq!(app.runtime.advance(1).unwrap(), Stop::Budget);
    app.runtime
        .memory
        .map(GuestPtr(0x50000000), vec![0xa8, 0x61], Permissions::RX)
        .unwrap();
    app.runtime
        .memory
        .write(GuestPtr(0x156), &0x12345678u32.to_be_bytes())
        .unwrap();
    for (i, (seed, value)) in [
        (16807u32, 16807i16),
        (282475249, 15089),
        (1622650073, -21287),
        (984943658, 3114),
        (1144108930, -18558),
    ]
    .into_iter()
    .enumerate()
    {
        let arch = if i % 2 == 0 {
            Architecture::PowerPc
        } else {
            Architecture::M68k
        };
        let pc = if i % 2 == 0 {
            gate(&app, 1)
        } else {
            0x50000000
        };
        app.runtime
            .memory
            .write(GuestPtr(stack), &[0x55; 4])
            .unwrap();
        app.runtime.cpu = Cpu::new(arch, pc, stack);
        assert_eq!(app.runtime.advance(1).unwrap(), Stop::Budget);
        match &app.runtime.cpu {
            Cpu::PowerPc(c) => assert_eq!(c.gpr[3], value as i32 as u32),
            Cpu::M68k(c) => {
                assert_eq!(c.dar[15], stack);
                assert_eq!(
                    app.runtime.memory.u16(GuestPtr(stack)).unwrap() as i16,
                    value
                );
                assert_eq!(app.runtime.memory.u16(GuestPtr(stack + 2)).unwrap(), 0x5555);
            }
        }
        assert_eq!(app.runtime.memory.u32(GuestPtr(base + 76)).unwrap(), seed);
        assert_eq!(app.runtime.memory.u32(GuestPtr(base + 202)).unwrap(), 0);
        assert_eq!(app.runtime.memory.u32(GuestPtr(0x156)).unwrap(), 0x12345678);
    }
}
#[test]
fn direct_seed_writes_unsigned_edges_and_invalid_storage() {
    let mut m = Memory::default();
    m.map(GuestPtr(0x1000), vec![0x5a; 8], Permissions::RW)
        .unwrap();
    let ports = Ports {
        current_slot: GuestPtr(0x1002 + 126),
        screen_bitmap: GuestPtr(0),
    };
    for (seed, next, value) in [
        (0u32, 0u32, 0i16),
        (0x7fffffff, 0, 0),
        (0x80000000, 16807, 16807),
        (0xffffffff, 16807, 16807),
        (0xfffffffe, 0, 0),
        (958682087, 32768, 0),
    ] {
        m.write(GuestPtr(0x1002), &seed.to_be_bytes()).unwrap();
        assert_eq!(ports.random(&mut m).unwrap(), value);
        assert_eq!(m.u32(GuestPtr(0x1002)).unwrap(), next);
        assert_eq!(m.u16(GuestPtr(0x1000)).unwrap(), 0x5a5a);
        assert_eq!(m.u16(GuestPtr(0x1006)).unwrap(), 0x5a5a);
    }
    m.map(
        GuestPtr(0x2000),
        1u32.to_be_bytes().to_vec(),
        Permissions::RX,
    )
    .unwrap();
    for slot in [0, 0x2000 + 126, 0x2002 + 126] {
        let invalid = Ports {
            current_slot: GuestPtr(slot),
            screen_bitmap: GuestPtr(0),
        };
        assert!(invalid.random(&mut m).is_err());
    }
    assert_eq!(m.u32(GuestPtr(0x2000)).unwrap(), 1);
}
