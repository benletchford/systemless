mod support;
use systemless_future::{
    abi::{Abi, Reply, Service},
    cpu::{Architecture, Call, Cpu},
    load::{LoadedApplication, STACK_BASE},
    memory::{GuestPtr, Memory, Permissions},
    process::{Process, HEAP_SIZE},
    regions::{self, Op},
    runtime::Stop,
};
#[test]
fn clearing_destroys_old_structure_shrinks_locked_storage_and_needs_no_allocation() {
    let mut p = Process::default();
    let mut m = Memory::default();
    let h = p.new_handle(&mut m, 64, false).unwrap();
    let data = m.u32(GuestPtr(h)).unwrap();
    m.write(GuestPtr(data), &[0xff; 64]).unwrap();
    let neighbor = p.new_pointer(&mut m, 16, false).unwrap();
    m.write(GuestPtr(neighbor), &[0xa5; 16]).unwrap();
    p.set_handle_lock(&m, h, true).unwrap();
    let fill = p.new_pointer(&mut m, HEAP_SIZE - 84, false).unwrap();
    assert_ne!(fill, 0);
    assert_eq!(p.new_pointer(&mut m, 4, false).unwrap(), 0);
    assert_eq!(p.mem_error, -108);
    p.res_error = -192;
    regions::call(&mut m, &mut p, Op::Clear, h, 0).unwrap();
    assert_eq!(p.mem_error, -108);
    assert_eq!(p.res_error, -192);
    assert_eq!(m.u32(GuestPtr(h)).unwrap(), data);
    assert_eq!(
        m.read(GuestPtr(data), 10).unwrap(),
        [0, 10, 0, 0, 0, 0, 0, 0, 0, 0]
    );
    assert_eq!(m.read(GuestPtr(neighbor), 16).unwrap(), [0xa5; 16]);
    assert_eq!(p.handle_size(&m, h).unwrap(), 10);
    assert_eq!(regions::call(&mut m, &mut p, Op::Empty, h, 0).unwrap(), 1);
    assert_ne!(p.new_pointer(&mut m, 52, false).unwrap(), 0);
    p.mem_error = -111;
    regions::call(&mut m, &mut p, Op::Clear, 0, 0).unwrap();
    assert_eq!(p.mem_error, -111);
    m.write(GuestPtr(h), &0u32.to_be_bytes()).unwrap();
    let before = m.read(GuestPtr(data), 10).unwrap().to_vec();
    assert!(regions::call(&mut m, &mut p, Op::Clear, h, 0).is_err());
    assert_eq!(m.read(GuestPtr(data), 10).unwrap(), before);
    assert_eq!(p.mem_error, -111);
    m.write(GuestPtr(h), &data.to_be_bytes()).unwrap();
    for invalid in [u32::MAX, neighbor] {
        assert!(regions::call(&mut m, &mut p, Op::Clear, invalid, 0).is_err());
        assert_eq!(p.mem_error, -111);
    }
    let mut p = Process::default();
    let mut m = Memory::default();
    let small = p.new_handle(&mut m, 8, false).unwrap();
    assert!(regions::call(&mut m, &mut p, Op::Clear, small, 0).is_err());
    assert_eq!(p.handle_size(&m, small).unwrap(), 8);
}
#[test]
fn both_cpu_routes_clear_the_same_region_with_void_results_and_preserved_error_globals() {
    let f = support::pef_program(&[0x4e800020], &[b"SetEmptyRgn"], 0);
    let mut app = LoadedApplication::pef(&f.bytes, None).unwrap();
    let h = app
        .runtime
        .process
        .new_handle(&mut app.runtime.memory, 36, false)
        .unwrap();
    let sp = STACK_BASE + 0x8000;
    let mut m = Memory::default();
    m.map(GuestPtr(0x1000), h.to_be_bytes().to_vec(), Permissions::RX)
        .unwrap();
    let mut cpu = Cpu::new(Architecture::M68k, 0, 0x1000);
    let Cpu::M68k(c) = &mut cpu else { panic!() };
    assert_eq!(
        Abi::trap(0xa8dd, c, &m).unwrap(),
        (
            Call::Region {
                op: Op::Clear,
                a: h,
                b: 0
            },
            Reply::None
        )
    );
    assert_eq!(c.dar[15], 0x1004);
    c.dar[15] = 0x1001;
    assert!(Abi::trap(0xa8dd, c, &m).is_err());
    assert_eq!(c.dar[15], 0x1001);
    app.runtime
        .memory
        .map(
            GuestPtr(0x50000000),
            support::words(&[0xa8dd]),
            Permissions::RX,
        )
        .unwrap();
    for arch in [Architecture::M68k, Architecture::PowerPc] {
        app.runtime
            .process
            .set_handle_size(&mut app.runtime.memory, h, 36)
            .unwrap();
        let data = app.runtime.memory.u32(GuestPtr(h)).unwrap();
        app.runtime
            .memory
            .write(GuestPtr(data), &[0xff; 36])
            .unwrap();
        app.runtime
            .memory
            .write(GuestPtr(0x220), &(-108i16).to_be_bytes())
            .unwrap();
        app.runtime
            .memory
            .write(GuestPtr(0xa60), &(-192i16).to_be_bytes())
            .unwrap();
        let pc = if arch == Architecture::M68k {
            let mut args = h.to_be_bytes().to_vec();
            args.extend([0x5a; 4]);
            app.runtime.memory.write(GuestPtr(sp), &args).unwrap();
            0x50000000
        } else {
            let tv = app
                .runtime
                .memory
                .u32(GuestPtr(app.info.toc_or_a5 + 8))
                .unwrap();
            app.runtime.memory.u32(GuestPtr(tv)).unwrap()
        };
        app.runtime.cpu = Cpu::new(arch, pc, sp);
        match &mut app.runtime.cpu {
            Cpu::M68k(c) => {
                c.dar[0] = 0xfeedbeef;
                c.dar[8] = 0xaabbccdd;
            }
            Cpu::PowerPc(c) => {
                c.gpr[3] = h;
                c.gpr[4] = 0xfeedbeef;
            }
        }
        assert_eq!(app.runtime.advance(1).unwrap(), Stop::Budget);
        match &app.runtime.cpu {
            Cpu::M68k(c) => {
                assert_eq!(c.dar[15], sp + 4);
                assert_eq!(c.dar[0], 0xfeedbeef);
                assert_eq!(c.dar[8], 0xaabbccdd);
                assert_eq!(
                    app.runtime.memory.read(GuestPtr(sp + 4), 4).unwrap(),
                    [0x5a; 4]
                );
            }
            Cpu::PowerPc(c) => {
                assert_eq!(c.gpr[3], h);
                assert_eq!(c.gpr[4], 0xfeedbeef);
            }
        }
        assert_eq!(
            app.runtime.memory.read(GuestPtr(data), 10).unwrap(),
            [0, 10, 0, 0, 0, 0, 0, 0, 0, 0]
        );
        assert_eq!(
            app.runtime.memory.u16(GuestPtr(0x220)).unwrap(),
            (-108i16) as u16
        );
        assert_eq!(
            app.runtime.memory.u16(GuestPtr(0xa60)).unwrap(),
            (-192i16) as u16
        );
    }
    let mut cpu = Cpu::new(Architecture::PowerPc, 0, sp);
    let Cpu::PowerPc(c) = &mut cpu else { panic!() };
    c.gpr[3] = h;
    assert_eq!(
        Abi::import(Service::Region(Op::Clear), c, &app.runtime.memory).unwrap(),
        (
            Call::Region {
                op: Op::Clear,
                a: h,
                b: 0
            },
            Reply::None
        )
    );
    let entry = systemless_future::traps::lookup(&app.runtime.memory, 0xa8dd, true).unwrap();
    assert_ne!(entry, systemless_future::traps::UNIMPLEMENTED);
    assert_eq!(systemless_future::traps::opcode(entry), Some(0xa8dd));
}
