mod support;
use systemless_future::{
    cpu::{Architecture, Cpu},
    events,
    host_input::Pointer,
    load::{LoadedApplication, STACK_BASE},
    memory::{GuestPtr, Permissions},
    runtime::Stop,
};
const SP: u32 = STACK_BASE + 0x8000;
fn setup(arch: Architecture, mask: u16, sleep: u32, complex: bool) -> (LoadedApplication, u32) {
    let f = support::pef_program(&[0x4e800020], &[b"WaitNextEvent"], 0);
    let mut app = LoadedApplication::pef(&f.bytes, None).unwrap();
    app.runtime
        .memory
        .map(GuestPtr(0x5000), vec![0xa5; 20], Permissions::RW)
        .unwrap();
    let bytes = if complex {
        support::words(&[
            44, 0, 0, 10, 10, 0, 0, 10, 32767, 3, 3, 7, 32767, 7, 3, 7, 32767, 10, 0, 10, 32767,
            32767,
        ])
    } else {
        support::words(&[10, 0, 0, 10, 10])
    };
    let h = app
        .runtime
        .process
        .new_handle(&mut app.runtime.memory, bytes.len() as u32, false)
        .unwrap();
    let data = app.runtime.memory.u32(GuestPtr(h)).unwrap();
    app.runtime.memory.write(GuestPtr(data), &bytes).unwrap();
    app.runtime
        .memory
        .map(
            GuestPtr(0x50000000),
            support::words(&[0xa860, 0x4e71]),
            Permissions::RX,
        )
        .unwrap();
    app.runtime
        .memory
        .map(
            GuestPtr(0x51000000),
            0x60000000u32.to_be_bytes().to_vec(),
            Permissions::RX,
        )
        .unwrap();
    let pc = if arch == Architecture::M68k {
        let mut b = h.to_be_bytes().to_vec();
        b.extend(sleep.to_be_bytes());
        b.extend(0x5000u32.to_be_bytes());
        b.extend(mask.to_be_bytes());
        b.extend([0x5a; 4]);
        app.runtime.memory.write(GuestPtr(SP), &b).unwrap();
        0x50000000
    } else {
        let tv = app
            .runtime
            .memory
            .u32(GuestPtr(app.info.toc_or_a5 + 8))
            .unwrap();
        app.runtime.memory.u32(GuestPtr(tv)).unwrap()
    };
    app.runtime.cpu = Cpu::new(arch, pc, SP);
    match &mut app.runtime.cpu {
        Cpu::M68k(c) => {
            c.dar[0] = 0xfeedbeef;
            c.dar[8] = 0xaabbccdd;
        }
        Cpu::PowerPc(c) => {
            c.gpr[3] = u32::from(mask);
            c.gpr[4] = 0x5000;
            c.gpr[5] = sleep;
            c.gpr[6] = h;
            c.lr = 0x51000000;
        }
    }
    app.runtime
        .memory
        .write(GuestPtr(0x220), &(-108i16).to_be_bytes())
        .unwrap();
    app.runtime
        .memory
        .write(GuestPtr(0xa60), &(-192i16).to_be_bytes())
        .unwrap();
    (app, h)
}
fn motion(app: &mut LoadedApplication, x: i32, y: i32) {
    Pointer::default()
        .motion(
            &mut app.runtime.process.input,
            &mut app.runtime.memory,
            (f64::from(x), f64::from(y)),
            (512, 342),
            (512, 342),
        )
        .unwrap();
}
fn result(app: &LoadedApplication, arch: Architecture, found: bool) {
    match &app.runtime.cpu {
        Cpu::M68k(c) => {
            assert_eq!(c.dar[0], 0xfeedbeef);
            assert_eq!(c.dar[8], 0xaabbccdd);
            assert_eq!(c.dar[15], SP + 14);
            assert_eq!(
                app.runtime.memory.read(GuestPtr(SP + 14), 4).unwrap(),
                [u8::from(found), 0x5a, 0x5a, 0x5a]
            );
        }
        Cpu::PowerPc(c) => {
            assert_eq!(arch, Architecture::PowerPc);
            assert_eq!(c.gpr[3], u32::from(found));
            assert_eq!(c.gpr[4], 0x5000);
        }
    }
    assert_eq!(
        app.runtime.memory.read(GuestPtr(0x5010), 4).unwrap(),
        [0xa5; 4]
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
#[test]
fn both_cpus_sleep_inside_complex_mouse_regions_and_wake_on_holes_without_spinning() {
    for arch in [Architecture::M68k, Architecture::PowerPc] {
        let (mut app, _) = setup(arch, 0x8000, 10, true);
        motion(&mut app, 1, 1);
        assert_eq!(app.runtime.advance(1).unwrap(), Stop::Waiting { ticks: 10 });
        let count = app.runtime.instructions;
        let input = app.runtime.process.input;
        events::post(&mut app.runtime.memory, 7, 0x1234, 0, input).unwrap();
        assert_eq!(
            app.runtime.advance(100).unwrap(),
            Stop::Waiting { ticks: 10 }
        );
        assert_eq!(app.runtime.instructions, count);
        app.runtime.elapse_ticks(3).unwrap();
        motion(&mut app, 2, 2);
        assert_eq!(
            app.runtime.advance(100).unwrap(),
            Stop::Waiting { ticks: 7 }
        );
        assert_eq!(app.runtime.instructions, count);
        motion(&mut app, 5, 5);
        assert_eq!(app.runtime.advance(1).unwrap(), Stop::Budget);
        result(&app, arch, true);
        assert_eq!(app.runtime.memory.u16(GuestPtr(0x5000)).unwrap(), 15);
        assert_eq!(
            app.runtime.memory.u32(GuestPtr(0x5002)).unwrap(),
            0xfa000000
        );
        assert_eq!(app.runtime.memory.u32(GuestPtr(0x5006)).unwrap(), 3);
        assert_eq!(
            app.runtime.memory.read(GuestPtr(0x500a), 4).unwrap(),
            [0, 5, 0, 5]
        );
        assert!(events::available(&app.runtime.memory, 1 << 7).unwrap());
    }
}
#[test]
fn empty_or_masked_mouse_regions_do_not_generate_moves_and_queued_input_keeps_priority() {
    for arch in [Architecture::M68k, Architecture::PowerPc] {
        let (mut app, h) = setup(arch, 0x8000, 5, false);
        let data = app.runtime.memory.u32(GuestPtr(h)).unwrap();
        app.runtime
            .memory
            .write(GuestPtr(data), &[0, 10, 0, 0, 0, 0, 0, 0, 0, 0])
            .unwrap();
        motion(&mut app, 20, 20);
        assert_eq!(app.runtime.advance(1).unwrap(), Stop::Waiting { ticks: 5 });
        app.runtime.elapse_ticks(5).unwrap();
        assert_eq!(app.runtime.advance(1).unwrap(), Stop::Budget);
        result(&app, arch, false);
        assert_eq!(app.runtime.memory.u16(GuestPtr(0x5000)).unwrap(), 0);
        let (mut app, _) = setup(arch, 8, 0, false);
        motion(&mut app, 20, 20);
        assert_eq!(app.runtime.advance(1).unwrap(), Stop::Budget);
        result(&app, arch, false);
        let (mut app, _) = setup(arch, 0x8008, 10, false);
        motion(&mut app, 20, 20);
        let input = app.runtime.process.input;
        events::post(&mut app.runtime.memory, 3, 42, 0, input).unwrap();
        assert_eq!(app.runtime.advance(1).unwrap(), Stop::Budget);
        result(&app, arch, true);
        assert_eq!(app.runtime.memory.u16(GuestPtr(0x5000)).unwrap(), 3);
        assert_eq!(app.runtime.memory.u32(GuestPtr(0x5002)).unwrap(), 42);
        assert!(!events::available(&app.runtime.memory, 8).unwrap());
    }
}
#[test]
fn mouse_waits_follow_live_global_coordinates_and_region_handles_with_atomic_faults() {
    for arch in [Architecture::M68k, Architecture::PowerPc] {
        let (mut app, h) = setup(arch, 0x8000, 10, false);
        motion(&mut app, 1, 1);
        assert_eq!(app.runtime.advance(1).unwrap(), Stop::Waiting { ticks: 10 });
        let data = app.runtime.memory.u32(GuestPtr(h)).unwrap();
        app.runtime
            .memory
            .write(GuestPtr(data + 2), &[0, 20, 0, 20, 0, 30, 0, 30])
            .unwrap();
        assert_eq!(app.runtime.advance(1).unwrap(), Stop::Budget);
        result(&app, arch, true);
        assert_eq!(
            app.runtime.memory.read(GuestPtr(0x500a), 4).unwrap(),
            [0, 1, 0, 1]
        );
        let (mut app, h) = setup(arch, 0x8008, 10, false);
        motion(&mut app, 1, 1);
        assert_eq!(app.runtime.advance(1).unwrap(), Stop::Waiting { ticks: 10 });
        let input = app.runtime.process.input;
        events::post(&mut app.runtime.memory, 3, 42, 0, input).unwrap();
        app.runtime
            .memory
            .write(GuestPtr(h), &0u32.to_be_bytes())
            .unwrap();
        assert!(app.runtime.advance(1).is_err());
        assert_eq!(
            app.runtime.memory.read(GuestPtr(0x5000), 16).unwrap(),
            [0xa5; 16]
        );
        assert!(events::available(&app.runtime.memory, 8).unwrap());
    }
    let (mut app, _) = setup(Architecture::M68k, 0x8000, 10, false);
    motion(&mut app, 1, 1);
    assert_eq!(app.runtime.advance(1).unwrap(), Stop::Waiting { ticks: 10 });
    app.runtime
        .memory
        .write(events::MOUSE, &[0, 11, 0, 11])
        .unwrap();
    assert_eq!(app.runtime.advance(1).unwrap(), Stop::Budget);
    assert_eq!(
        app.runtime.memory.read(GuestPtr(0x500a), 4).unwrap(),
        [0, 11, 0, 11]
    );
}
