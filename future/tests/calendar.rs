mod support;
use systemless_future::{
    abi::{Abi, Reply, Service},
    calendar::{self, Clock, DEFAULT_TIME, TICKS, TIME},
    cpu::{Architecture, Call, Cpu},
    load::{LoadedApplication, STACK_BASE},
    memory::{GuestPtr, Memory, Permissions},
    runtime::Stop,
};
fn memory() -> Memory {
    let mut m = Memory::default();
    m.map(GuestPtr(0), vec![0xa5; 1024], Permissions::RW)
        .unwrap();
    m.write(TICKS, &0u32.to_be_bytes()).unwrap();
    m.write(TIME, &DEFAULT_TIME.to_be_bytes()).unwrap();
    m
}
#[test]
fn fractional_seconds_follow_ticks_wrap_and_respect_guest_calendar_writes() {
    let mut m = memory();
    let mut clock = Clock::default();
    clock.advance(&mut m, 59).unwrap();
    assert_eq!(m.u32(TIME).unwrap(), DEFAULT_TIME);
    clock.advance(&mut m, 1).unwrap();
    assert_eq!(m.u32(TIME).unwrap(), DEFAULT_TIME + 1);
    assert_eq!(m.u32(TICKS).unwrap(), 60);
    m.write(TIME, &u32::MAX.to_be_bytes()).unwrap();
    m.write(TICKS, &(u32::MAX - 2).to_be_bytes()).unwrap();
    clock.advance(&mut m, 60).unwrap();
    assert_eq!(m.u32(TIME).unwrap(), 0);
    assert_eq!(m.u32(TICKS).unwrap(), 57);
    clock.set(&mut m, 100).unwrap();
    clock.advance(&mut m, u32::MAX).unwrap();
    assert_eq!(m.u32(TIME).unwrap(), 100 + 71_582_788);
    clock.advance(&mut m, 45).unwrap();
    assert_eq!(m.u32(TIME).unwrap(), 100 + 71_582_789);
    assert_eq!(m.read(GuestPtr(TIME.0 - 1), 1).unwrap(), [0xa5]);
    assert_eq!(m.read(GuestPtr(TIME.0 + 4), 1).unwrap(), [0xa5]);
    assert_eq!(DEFAULT_TIME as u64, calendar::UNIX_OFFSET + 946_684_800);
}
#[test]
fn failed_clock_publication_preserves_both_words_and_fractional_phase() {
    let mut good = memory();
    let mut clock = Clock::default();
    clock.advance(&mut good, 59).unwrap();
    for readonly_time in [false, true] {
        let mut bad = Memory::default();
        bad.map(
            TICKS,
            10u32.to_be_bytes().to_vec(),
            if readonly_time {
                Permissions::RW
            } else {
                Permissions::RX
            },
        )
        .unwrap();
        bad.map(
            TIME,
            20u32.to_be_bytes().to_vec(),
            if readonly_time {
                Permissions::RX
            } else {
                Permissions::RW
            },
        )
        .unwrap();
        assert!(clock.advance(&mut bad, 1).is_err());
        assert_eq!(bad.u32(TICKS).unwrap(), 10);
        assert_eq!(bad.u32(TIME).unwrap(), 20);
        if readonly_time {
            assert!(clock.set(&mut bad, 123).is_err());
        }
    }
    clock.advance(&mut good, 1).unwrap();
    assert_eq!(good.u32(TIME).unwrap(), DEFAULT_TIME + 1);
    clock.advance(&mut good, 59).unwrap();
    let tick = good.u32(TICKS).unwrap();
    clock.set(&mut good, 123).unwrap();
    assert_eq!(good.u32(TICKS).unwrap(), tick);
    clock.advance(&mut good, 1).unwrap();
    assert_eq!(good.u32(TIME).unwrap(), 123);
}
#[test]
fn real_cpu_execution_and_idle_host_ticks_share_calendar_progress() {
    let f = support::pef_program(&[0x48000000], &[], 0);
    for arch in [Architecture::M68k, Architecture::PowerPc] {
        let mut app = LoadedApplication::pef(&f.bytes, None).unwrap();
        if arch == Architecture::M68k {
            app.runtime
                .memory
                .map(
                    GuestPtr(0x50000000),
                    support::words(&[0x60fe]),
                    Permissions::RX,
                )
                .unwrap();
            app.runtime.cpu = Cpu::new(arch, 0x50000000, STACK_BASE + 0x8000);
        }
        assert_eq!(app.runtime.memory.u32(TIME).unwrap(), DEFAULT_TIME);
        assert_eq!(app.runtime.advance(60_000).unwrap(), Stop::Budget);
        assert_eq!(app.runtime.memory.u32(TIME).unwrap(), DEFAULT_TIME + 1);
        assert_eq!(app.runtime.memory.u32(TICKS).unwrap(), 60);
        let instructions = app.runtime.instructions;
        app.runtime.elapse_ticks(59).unwrap();
        assert_eq!(app.runtime.instructions, instructions);
        assert_eq!(app.runtime.memory.u32(TIME).unwrap(), DEFAULT_TIME + 1);
        assert_eq!(app.runtime.advance(1000).unwrap(), Stop::Budget);
        assert_eq!(app.runtime.memory.u32(TIME).unwrap(), DEFAULT_TIME + 2);
        app.runtime.set_date_time(42).unwrap();
        assert_eq!(app.runtime.memory.u32(TICKS).unwrap(), 120);
        assert_eq!(app.runtime.memory.u32(TIME).unwrap(), 42);
    }
}
fn invoke(app: &mut LoadedApplication, index: u32, p: u32) -> u32 {
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
    c.gpr[3] = p;
    assert_eq!(app.runtime.advance(1).unwrap(), Stop::Budget);
    let Cpu::PowerPc(c) = &app.runtime.cpu else {
        panic!()
    };
    c.gpr[3]
}
#[test]
fn date_time_adapters_preserve_native_pointer_and_ppc_void_result_with_checked_outputs() {
    let f = support::pef_program(
        &[0x4e800020],
        &[b"GetDateTime", b"ReadDateTime", b"InitGraf", b"Random"],
        0,
    );
    let mut app = LoadedApplication::pef(&f.bytes, None).unwrap();
    app.runtime.set_date_time(0x81234567).unwrap();
    app.runtime
        .memory
        .write(GuestPtr(0x220), &(-108i16).to_be_bytes())
        .unwrap();
    app.runtime
        .memory
        .write(GuestPtr(0xa60), &(-192i16).to_be_bytes())
        .unwrap();
    assert_eq!(invoke(&mut app, 0, 0x301), 0x301);
    assert_eq!(app.runtime.memory.u32(GuestPtr(0x301)).unwrap(), 0x81234567);
    assert_eq!(invoke(&mut app, 1, 0x305), 0);
    assert_eq!(app.runtime.memory.u32(GuestPtr(0x305)).unwrap(), 0x81234567);
    app.runtime
        .memory
        .map(
            GuestPtr(0x50000000),
            support::words(&[0xa039]),
            Permissions::RX,
        )
        .unwrap();
    let sp = STACK_BASE + 0x8000;
    app.runtime.cpu = Cpu::new(Architecture::M68k, 0x50000000, sp);
    let Cpu::M68k(c) = &mut app.runtime.cpu else {
        panic!()
    };
    c.dar[0] = 0xfeedbeef;
    c.dar[8] = 0x309;
    assert_eq!(app.runtime.advance(1).unwrap(), Stop::Budget);
    let Cpu::M68k(c) = &app.runtime.cpu else {
        panic!()
    };
    assert_eq!(c.dar[0], 0);
    assert_eq!(c.dar[8], 0x309);
    assert_eq!(c.dar[15], sp);
    assert_eq!(app.runtime.memory.u32(GuestPtr(0x309)).unwrap(), 0x81234567);
    assert_eq!(
        app.runtime.memory.u16(GuestPtr(0x220)).unwrap(),
        (-108i16) as u16
    );
    assert_eq!(
        app.runtime.memory.u16(GuestPtr(0xa60)).unwrap(),
        (-192i16) as u16
    );
    let mut cpu = Cpu::new(Architecture::PowerPc, 0, 0);
    let Cpu::PowerPc(c) = &mut cpu else { panic!() };
    c.gpr[3] = 0x300;
    assert_eq!(
        Abi::import(Service::GetDateTime, c, &app.runtime.memory).unwrap(),
        (Call::ReadDateTime(GuestPtr(0x300)), Reply::None)
    );
    // Reproduce the original application's direct calendar-to-seed copy.
    app.runtime
        .memory
        .map(GuestPtr(0x1000), vec![0; 1024], Permissions::RW)
        .unwrap();
    invoke(&mut app, 2, 0x1100);
    app.runtime
        .memory
        .map(
            GuestPtr(0x50000100),
            support::words(&[0x41ed, 0xff7e, 0x20b8, 0x020c, 0x4267, 0xa861]),
            Permissions::RX,
        )
        .unwrap();
    app.runtime.cpu = Cpu::new(Architecture::M68k, 0x50000100, sp);
    let Cpu::M68k(c) = &mut app.runtime.cpu else {
        panic!()
    };
    c.dar[13] = 0x1104;
    assert_eq!(app.runtime.advance(2).unwrap(), Stop::Budget);
    assert_eq!(
        app.runtime.memory.u32(GuestPtr(0x1082)).unwrap(),
        0x81234567
    );
    assert_eq!(app.runtime.advance(2).unwrap(), Stop::Budget);
    assert_eq!(
        app.runtime.memory.u32(GuestPtr(0x1082)).unwrap(),
        0x32a1af6d
    );
    assert_eq!(app.runtime.memory.u16(GuestPtr(sp - 2)).unwrap(), 0xaf6d);
    let mut m = memory();
    m.map(GuestPtr(0x1000), vec![0x5a; 8], Permissions::RX)
        .unwrap();
    for p in [0x1000, 0x3fe, u32::MAX] {
        let before = m.read(GuestPtr(0), 1024).unwrap().to_vec();
        assert!(calendar::read(&mut m, GuestPtr(p)).is_err());
        assert_eq!(m.read(GuestPtr(0), 1024).unwrap(), before);
    }
    calendar::read(&mut m, TIME).unwrap();
    assert_eq!(m.u32(TIME).unwrap(), DEFAULT_TIME);
}
