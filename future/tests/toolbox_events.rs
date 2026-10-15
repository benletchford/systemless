mod support;
use systemless_future::{
    cpu::{Architecture, Cpu},
    load::{LoadedApplication, STACK_BASE},
    memory::{GuestPtr, Permissions},
    runtime::Stop,
};
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
    for (i, &arg) in args.iter().enumerate() {
        c.gpr[3 + i] = arg;
    }
    assert_eq!(app.runtime.advance(1).unwrap(), Stop::Budget);
    let Cpu::PowerPc(c) = &app.runtime.cpu else {
        panic!()
    };
    c.gpr[3]
}
#[test]
fn toolbox_event_peeking_and_consumption_share_os_records_across_cpu_adapters() {
    let f = support::pef_program(
        &[0x4e800020],
        &[b"PostEvent", b"EventAvail", b"GetNextEvent"],
        32,
    );
    let mut app = LoadedApplication::pef(&f.bytes, None).unwrap();
    assert_eq!(invoke(&mut app, 0, &[3, 0x123456]), 0);
    let queue = app
        .runtime
        .memory
        .read(GuestPtr(0x14a), 10)
        .unwrap()
        .to_vec();
    assert_eq!(invoke(&mut app, 1, &[8, 0x300]), 1);
    assert_eq!(app.runtime.memory.read(GuestPtr(0x14a), 10).unwrap(), queue);
    let event = app
        .runtime
        .memory
        .read(GuestPtr(0x300), 16)
        .unwrap()
        .to_vec();
    assert_eq!(app.runtime.memory.u32(GuestPtr(0x302)).unwrap(), 0x123456);
    let stack = STACK_BASE + 0x8000;
    app.runtime
        .memory
        .map(
            GuestPtr(0x50000000),
            vec![0xa9, 0x71, 0xa9, 0x70],
            Permissions::RX,
        )
        .unwrap();
    let mut args = 0x320u32.to_be_bytes().to_vec();
    args.extend(8u16.to_be_bytes());
    args.extend([0xa5; 2]);
    app.runtime.memory.write(GuestPtr(stack), &args).unwrap();
    app.runtime.cpu = Cpu::new(Architecture::M68k, 0x50000000, stack);
    assert_eq!(app.runtime.advance(1).unwrap(), Stop::Budget);
    let Cpu::M68k(c) = &app.runtime.cpu else {
        panic!()
    };
    assert_eq!(c.dar[15], stack + 6);
    assert_eq!(
        app.runtime.memory.read(GuestPtr(stack + 6), 2).unwrap(),
        [1, 0xa5]
    );
    assert_eq!(app.runtime.memory.read(GuestPtr(0x320), 16).unwrap(), event);
    assert_eq!(app.runtime.memory.read(GuestPtr(0x14a), 10).unwrap(), queue);
    assert_eq!(invoke(&mut app, 2, &[8, 0x340]), 1);
    assert_eq!(app.runtime.memory.read(GuestPtr(0x340), 16).unwrap(), event);
    assert_eq!(invoke(&mut app, 1, &[8, 0x360]), 0);
    assert_eq!(app.runtime.memory.u16(GuestPtr(0x360)).unwrap(), 0);
}

#[test]
fn activation_slots_are_guest_authority_peeked_then_consumed_without_os_queue_mutation() {
    let f = support::pef_program(
        &[0x4e800020],
        &[
            b"EventAvail",
            b"GetNextEvent",
            b"OSEventAvail",
            b"WaitNextEvent",
        ],
        32,
    );
    let mut app = LoadedApplication::pef(&f.bytes, None).unwrap();
    app.runtime
        .memory
        .write(GuestPtr(0xa68), &0x11223344u32.to_be_bytes())
        .unwrap();
    app.runtime
        .memory
        .write(GuestPtr(0xa64), &0x55667788u32.to_be_bytes())
        .unwrap();
    app.runtime.process.input.modifiers = 0x180;
    let queue = app
        .runtime
        .memory
        .read(GuestPtr(0x14a), 10)
        .unwrap()
        .to_vec();
    assert_eq!(invoke(&mut app, 2, &[1 << 8, 0x300]), 0);
    assert_eq!(invoke(&mut app, 0, &[1 << 8, 0x300]), 1);
    assert_eq!(invoke(&mut app, 0, &[1 << 8, 0x300]), 1);
    assert_eq!(app.runtime.memory.u16(GuestPtr(0x300)).unwrap(), 8);
    assert_eq!(app.runtime.memory.u32(GuestPtr(0x302)).unwrap(), 0x11223344);
    assert_eq!(app.runtime.memory.u16(GuestPtr(0x30e)).unwrap(), 0x180);
    assert_eq!(app.runtime.memory.u32(GuestPtr(0xa68)).unwrap(), 0x11223344);
    app.runtime
        .memory
        .map(GuestPtr(0x50000000), vec![0xa9, 0x70], Permissions::RX)
        .unwrap();
    let stack = STACK_BASE + 0x8000;
    let mut frame = 0x320u32.to_be_bytes().to_vec();
    frame.extend(0x100u16.to_be_bytes());
    frame.extend([0xa5; 2]);
    app.runtime.memory.write(GuestPtr(stack), &frame).unwrap();
    app.runtime.cpu = Cpu::new(Architecture::M68k, 0x50000000, stack);
    assert_eq!(app.runtime.advance(1).unwrap(), Stop::Budget);
    assert_eq!(
        app.runtime.memory.read(GuestPtr(stack + 6), 2).unwrap(),
        [1, 0xa5]
    );
    assert_eq!(app.runtime.memory.u32(GuestPtr(0xa68)).unwrap(), 0);
    // A pending activation wakes the wait immediately, with activeFlag set.
    assert_eq!(invoke(&mut app, 3, &[1 << 8, 0x340, 100, 0]), 1);
    assert_eq!(app.runtime.memory.u32(GuestPtr(0x342)).unwrap(), 0x55667788);
    assert_eq!(app.runtime.memory.u16(GuestPtr(0x34e)).unwrap(), 0x181);
    assert_eq!(app.runtime.memory.u32(GuestPtr(0xa64)).unwrap(), 0);
    assert_eq!(app.runtime.memory.read(GuestPtr(0x14a), 10).unwrap(), queue);
    app.runtime
        .memory
        .write(GuestPtr(0xa64), &99u32.to_be_bytes())
        .unwrap();
    app.runtime
        .memory
        .map(GuestPtr(0x51000000), vec![0; 16], Permissions::RX)
        .unwrap();
    let tv = app
        .runtime
        .memory
        .u32(GuestPtr(app.info.toc_or_a5 + 12))
        .unwrap();
    let pc = app.runtime.memory.u32(GuestPtr(tv)).unwrap();
    app.runtime.cpu = Cpu::new(Architecture::PowerPc, pc, stack);
    if let Cpu::PowerPc(c) = &mut app.runtime.cpu {
        c.gpr[3] = 1 << 8;
        c.gpr[4] = 0x51000000;
    }
    assert!(app.runtime.advance(1).is_err());
    assert_eq!(app.runtime.memory.u32(GuestPtr(0xa64)).unwrap(), 99);
}

#[test]
fn system_task_preserves_registers_events_and_guest_state_for_both_cpus() {
    let f = support::pef_program(&[0x4e800020], &[b"SystemTask", b"PostEvent"], 0);
    let mut app = LoadedApplication::pef(&f.bytes, None).unwrap();
    assert_eq!(invoke(&mut app, 1, &[3, 0x12345678]), 0);
    app.runtime
        .memory
        .write(GuestPtr(0xa64), &0x11112222u32.to_be_bytes())
        .unwrap();
    app.runtime
        .memory
        .write(GuestPtr(0xa68), &0x33334444u32.to_be_bytes())
        .unwrap();
    let tv = app
        .runtime
        .memory
        .u32(GuestPtr(app.info.toc_or_a5 + 8))
        .unwrap();
    let pc = app.runtime.memory.u32(GuestPtr(tv)).unwrap();
    app.runtime.cpu = Cpu::new(Architecture::PowerPc, pc, STACK_BASE + 0x8000);
    let Cpu::PowerPc(c) = &mut app.runtime.cpu else {
        panic!()
    };
    for i in 2..32 {
        c.gpr[i] = 0xaabb0000 + i as u32;
    }
    c.lr = 0x12340000;
    c.cr = 0x87654321;
    let registers = c.gpr;
    let low = app
        .runtime
        .memory
        .read(GuestPtr(0), 0x1000)
        .unwrap()
        .to_vec();
    assert_eq!(app.runtime.advance(1).unwrap(), Stop::Budget);
    let Cpu::PowerPc(c) = &app.runtime.cpu else {
        panic!()
    };
    assert_eq!(c.gpr, registers);
    assert_eq!(c.cr, 0x87654321);
    assert_eq!(c.pc, 0x12340000);
    assert_eq!(c.lr, 0x12340000);
    assert_eq!(app.runtime.memory.read(GuestPtr(0), 0x1000).unwrap(), low);
    app.runtime
        .memory
        .map(GuestPtr(0x50000000), vec![0xa9, 0xb4], Permissions::RX)
        .unwrap();
    app.runtime.cpu = Cpu::new(Architecture::M68k, 0x50000000, STACK_BASE + 0x8000);
    let Cpu::M68k(c) = &mut app.runtime.cpu else {
        panic!()
    };
    for i in 0..15 {
        c.dar[i] = 0xccdd0000 + i as u32;
    }
    let registers = c.dar;
    assert_eq!(app.runtime.advance(1).unwrap(), Stop::Budget);
    let Cpu::M68k(c) = &app.runtime.cpu else {
        panic!()
    };
    assert_eq!(c.dar, registers);
    assert_eq!(c.pc, 0x50000002);
    assert_eq!(app.runtime.memory.read(GuestPtr(0), 0x1000).unwrap(), low);
    assert_eq!(
        app.runtime
            .services
            .calls
            .iter()
            .filter(|c| matches!(c, systemless_future::cpu::Call::SystemTask))
            .count(),
        2
    );
}

#[test]
fn pen_size_adapters_write_vertical_then_horizontal_live_port_words() {
    let f = support::pef_program(
        &[0x4e800020],
        &[b"InitGraf", b"OpenPort", b"SetPort", b"PenSize"],
        0,
    );
    let mut app = LoadedApplication::pef(&f.bytes, None).unwrap();
    invoke(&mut app, 0, &[0x600]);
    invoke(&mut app, 1, &[0x800]);
    invoke(&mut app, 2, &[0x800]);
    assert_eq!(invoke(&mut app, 3, &[0xabcd0003, 0x12340005]), 0xabcd0003);
    assert_eq!(
        app.runtime.memory.read(GuestPtr(0x834), 4).unwrap(),
        [0, 5, 0, 3]
    );
    let following = app
        .runtime
        .memory
        .read(GuestPtr(0x838), 10)
        .unwrap()
        .to_vec();
    app.runtime
        .memory
        .map(GuestPtr(0x50000000), vec![0xa8, 0x9b], Permissions::RX)
        .unwrap();
    let stack = STACK_BASE + 0x8000;
    app.runtime
        .memory
        .write(GuestPtr(stack), &[0, 7, 0, 2, 0xa5, 0x5a])
        .unwrap();
    app.runtime.cpu = Cpu::new(Architecture::M68k, 0x50000000, stack);
    let Cpu::M68k(c) = &mut app.runtime.cpu else {
        panic!()
    };
    c.dar[0] = 0x12345678;
    assert_eq!(app.runtime.advance(1).unwrap(), Stop::Budget);
    let Cpu::M68k(c) = &app.runtime.cpu else {
        panic!()
    };
    assert_eq!(c.dar[15], stack + 4);
    assert_eq!(c.dar[0], 0x12345678);
    assert_eq!(
        app.runtime.memory.read(GuestPtr(stack + 4), 2).unwrap(),
        [0xa5, 0x5a]
    );
    assert_eq!(
        app.runtime.memory.read(GuestPtr(0x834), 4).unwrap(),
        [0, 7, 0, 2]
    );
    assert_eq!(
        app.runtime.memory.read(GuestPtr(0x838), 10).unwrap(),
        following
    );
}

#[test]
fn rectangle_region_intersection_checks_holes_edges_and_native_boolean_padding() {
    let f = support::pef_program(&[0x4e800020], &[b"RectInRgn"], 0);
    let mut app = LoadedApplication::pef(&f.bytes, None).unwrap();
    let words = [
        0i16, 0, 0, 6, 6, 0, 0, 6, 32767, 2, 2, 4, 32767, 4, 2, 4, 32767, 6, 0, 6, 32767, 32767,
    ];
    let mut bytes: Vec<_> = words.iter().flat_map(|w| w.to_be_bytes()).collect();
    let len = bytes.len() as u16;
    bytes[..2].copy_from_slice(&len.to_be_bytes());
    let handle = app
        .runtime
        .process
        .new_handle(&mut app.runtime.memory, len as u32, false)
        .unwrap();
    let data = app.runtime.memory.u32(GuestPtr(handle)).unwrap();
    app.runtime.memory.write(GuestPtr(data), &bytes).unwrap();
    for (rectangle, expected) in [
        ([2i16, 2, 4, 4], 0),
        ([1, 1, 3, 3], 1),
        ([0, 6, 6, 8], 0),
        ([0, 0, 0, 6], 0),
        ([4, 4, 2, 2], 0),
        ([-2, -2, 1, 1], 1),
    ] {
        let rect: Vec<_> = rectangle.iter().flat_map(|w| w.to_be_bytes()).collect();
        app.runtime.memory.write(GuestPtr(0x300), &rect).unwrap();
        assert_eq!(invoke(&mut app, 0, &[0x300, handle]), expected);
    }
    app.runtime
        .memory
        .write(GuestPtr(0x300), &[0, 1, 0, 1, 0, 3, 0, 3])
        .unwrap();
    app.runtime
        .memory
        .map(GuestPtr(0x50000000), vec![0xa8, 0xe9], Permissions::RX)
        .unwrap();
    let stack = STACK_BASE + 0x8000;
    let mut args = handle.to_be_bytes().to_vec();
    args.extend(0x300u32.to_be_bytes());
    args.extend([0xa5, 0x5a]);
    app.runtime.memory.write(GuestPtr(stack), &args).unwrap();
    app.runtime.cpu = Cpu::new(Architecture::M68k, 0x50000000, stack);
    assert_eq!(app.runtime.advance(1).unwrap(), Stop::Budget);
    let Cpu::M68k(c) = &app.runtime.cpu else {
        panic!()
    };
    assert_eq!(c.dar[15], stack + 8);
    assert_eq!(
        app.runtime.memory.read(GuestPtr(stack + 8), 2).unwrap(),
        [1, 0x5a]
    );
    app.runtime
        .memory
        .map(GuestPtr(0x50001000), args, Permissions::RX)
        .unwrap();
    app.runtime.cpu = Cpu::new(Architecture::M68k, 0x50000000, 0x50001000);
    assert!(app.runtime.advance(1).is_err());
    let Cpu::M68k(c) = &app.runtime.cpu else {
        panic!()
    };
    assert_eq!(c.dar[15], 0x50001000);
    assert_eq!(
        app.runtime
            .memory
            .read(GuestPtr(data), len as usize)
            .unwrap(),
        bytes
    );
}

#[test]
fn pen_normal_resets_only_size_mode_pattern_through_both_real_cpus() {
    let f = support::pef_program(
        &[0x4e800020],
        &[b"InitGraf", b"OpenPort", b"SetPort", b"PenNormal"],
        0,
    );
    let mut app = LoadedApplication::pef(&f.bytes, None).unwrap();
    invoke(&mut app, 0, &[0x600]);
    invoke(&mut app, 1, &[0x800]);
    invoke(&mut app, 2, &[0x800]);
    app.runtime
        .memory
        .map(GuestPtr(0x50000000), vec![0xa8, 0x9e], Permissions::RX)
        .unwrap();
    for arch in [Architecture::PowerPc, Architecture::M68k] {
        app.runtime
            .memory
            .write(GuestPtr(0x830), &[0xff, 0xf9, 0, 23])
            .unwrap();
        app.runtime
            .memory
            .write(GuestPtr(0x834), &[0x55; 14])
            .unwrap();
        app.runtime
            .memory
            .write(GuestPtr(0x842), &(-1i16).to_be_bytes())
            .unwrap();
        let mut expected = app
            .runtime
            .memory
            .read(GuestPtr(0x800), 108)
            .unwrap()
            .to_vec();
        expected[52..66]
            .copy_from_slice(&[0, 1, 0, 1, 0, 8, 255, 255, 255, 255, 255, 255, 255, 255]);
        if arch == Architecture::PowerPc {
            assert_eq!(invoke(&mut app, 3, &[0xdeadbeef]), 0xdeadbeef);
        } else {
            let stack = STACK_BASE + 0x8000;
            app.runtime.cpu = Cpu::new(arch, 0x50000000, stack);
            let Cpu::M68k(c) = &mut app.runtime.cpu else {
                panic!()
            };
            c.dar[0] = 0xdeadbeef;
            assert_eq!(app.runtime.advance(1).unwrap(), Stop::Budget);
            let Cpu::M68k(c) = &app.runtime.cpu else {
                panic!()
            };
            assert_eq!(c.dar[15], stack);
            assert_eq!(c.dar[0], 0xdeadbeef);
        }
        assert_eq!(
            app.runtime.memory.read(GuestPtr(0x800), 108).unwrap(),
            expected
        );
    }
}
