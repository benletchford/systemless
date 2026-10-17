mod support;
use systemless_future::{
    cpu::{Architecture, Cpu},
    load::{LoadedApplication, STACK_BASE},
    memory::{GuestPtr, Permissions},
    runtime::Stop,
};
fn invoke(app: &mut LoadedApplication, index: u32, args: &[u32]) -> u32 {
    let vector = app
        .runtime
        .memory
        .u32(GuestPtr(app.info.toc_or_a5 + 8 + index * 4))
        .unwrap();
    let pc = app.runtime.memory.u32(GuestPtr(vector)).unwrap();
    app.runtime.cpu = Cpu::new(Architecture::PowerPc, pc, STACK_BASE + 0x8000);
    let Cpu::PowerPc(c) = &mut app.runtime.cpu else {
        panic!()
    };
    for (i, &value) in args.iter().enumerate() {
        c.gpr[3 + i] = value;
    }
    assert_eq!(app.runtime.advance(1).unwrap(), Stop::Budget);
    let Cpu::PowerPc(c) = &app.runtime.cpu else {
        panic!()
    };
    c.gpr[3]
}
#[test]
fn get_picture_and_icon_share_resource_identity_release_and_checked_cpu_results() {
    for (symbol, kind, opcode) in [
        (b"GetPicture".as_slice(), *b"PICT", 0xbcu8),
        (b"GetIcon".as_slice(), *b"ICON", 0xbb),
    ] {
        let f = support::pef_program(
            &[0x4e800020],
            &[symbol, b"GetResource", b"ReleaseResource"],
            0,
        );
        let picture = if kind == *b"ICON" {
            (0..128).map(|n| n as u8).collect()
        } else {
            vec![0, 12, 0, 0, 0, 0, 0, 8, 0, 8, 0x11, 1]
        };
        let fork = support::resource(vec![(kind, -7, None, picture.clone())]);
        let mut app = LoadedApplication::pef(&f.bytes, Some(&fork)).unwrap();
        if kind == *b"ICON" {
            assert_ne!(
                systemless_future::traps::address(0xa900 | u16::from(opcode), true),
                systemless_future::traps::UNIMPLEMENTED
            );
        }
        assert_eq!(invoke(&mut app, 0, &[0]), 0);
        assert_eq!(invoke(&mut app, 0, &[123]), 0);
        assert_eq!(
            app.runtime.memory.u16(GuestPtr(0xa60)).unwrap() as i16,
            -192
        );
        let h = invoke(&mut app, 0, &[0xfffffff9]);
        assert_ne!(h, 0);
        assert_eq!(app.runtime.memory.u16(GuestPtr(0xa60)).unwrap(), 0);
        assert_eq!(
            invoke(&mut app, 1, &[u32::from_be_bytes(kind), 0xfffffff9]),
            h
        );
        let data = app.runtime.memory.u32(GuestPtr(h)).unwrap();
        assert_eq!(
            app.runtime
                .memory
                .read(GuestPtr(data), picture.len())
                .unwrap(),
            picture
        );
        app.runtime
            .memory
            .write(GuestPtr(data + 10), &[0x55])
            .unwrap();
        app.runtime
            .memory
            .map(GuestPtr(0x50000000), vec![0xa9, opcode], Permissions::RX)
            .unwrap();
        let stack = STACK_BASE + 0x8000;
        app.runtime
            .memory
            .write(
                GuestPtr(stack),
                &[0xff, 0xf9, 0xa5, 0xa5, 0xa5, 0xa5, 0x5a, 0x5a],
            )
            .unwrap();
        app.runtime.cpu = Cpu::new(Architecture::M68k, 0x50000000, stack);
        assert_eq!(app.runtime.advance(1).unwrap(), Stop::Budget);
        let Cpu::M68k(c) = &app.runtime.cpu else {
            panic!()
        };
        assert_eq!(c.dar[15], stack + 2);
        assert_eq!(app.runtime.memory.u32(GuestPtr(stack + 2)).unwrap(), h);
        assert_eq!(
            app.runtime.memory.read(GuestPtr(stack + 6), 2).unwrap(),
            [0x5a; 2]
        );
        invoke(&mut app, 2, &[h]);
        assert_eq!(app.runtime.memory.u32(GuestPtr(h)).unwrap(), 0);
        let fresh = invoke(&mut app, 0, &[0xfffffff9]);
        let fresh_data = app.runtime.memory.u32(GuestPtr(fresh)).unwrap();
        assert_eq!(
            app.runtime
                .memory
                .read(GuestPtr(fresh_data), picture.len())
                .unwrap(),
            picture
        );
        app.runtime
            .memory
            .map(
                GuestPtr(0x50001000),
                vec![0xff, 0xf9, 0, 0, 0, 0],
                Permissions::RX,
            )
            .unwrap();
        app.runtime.cpu = Cpu::new(Architecture::M68k, 0x50000000, 0x50001000);
        assert!(app.runtime.advance(1).is_err());
        let Cpu::M68k(c) = &app.runtime.cpu else {
            panic!()
        };
        assert_eq!(c.dar[15], 0x50001000);
    }
}
#[test]
fn port_size_and_mapping_adapters_preserve_clip_origin_and_bitmap_extent() {
    let f = support::pef_program(
        &[0x4e800020],
        &[
            b"InitGraf",
            b"OpenPort",
            b"SetPort",
            b"PortSize",
            b"MovePortTo",
        ],
        0,
    );
    let mut app = LoadedApplication::pef(&f.bytes, None).unwrap();
    invoke(&mut app, 0, &[0x600]);
    invoke(&mut app, 1, &[0x800]);
    invoke(&mut app, 2, &[0x800]);
    app.runtime
        .memory
        .write(GuestPtr(0x810), &[0, 10, 0, 20, 0, 40, 0, 60])
        .unwrap();
    let initial = app
        .runtime
        .memory
        .read(GuestPtr(0x800), 108)
        .unwrap()
        .to_vec();
    let regions = app
        .runtime
        .memory
        .read(GuestPtr(0x818), 8)
        .unwrap()
        .to_vec();
    assert_eq!(invoke(&mut app, 3, &[0xabcd0008, 0x12340006]), 0xabcd0008);
    assert_eq!(
        app.runtime.memory.read(GuestPtr(0x810), 8).unwrap(),
        [0, 10, 0, 20, 0, 16, 0, 28]
    );
    assert_eq!(invoke(&mut app, 4, &[5, 7]), 5);
    let bounds = app
        .runtime
        .memory
        .read(GuestPtr(0x808), 8)
        .unwrap()
        .to_vec();
    assert_eq!(bounds, [0, 3, 0, 15, 1, 89, 2, 15]);
    assert_eq!(
        app.runtime.memory.read(GuestPtr(0x818), 8).unwrap(),
        regions
    );
    let expected = app
        .runtime
        .memory
        .read(GuestPtr(0x800), 108)
        .unwrap()
        .to_vec();
    app.runtime.memory.write(GuestPtr(0x800), &initial).unwrap();
    app.runtime
        .memory
        .map(
            GuestPtr(0x50000000),
            vec![0xa8, 0x76, 0xa8, 0x77],
            Permissions::RX,
        )
        .unwrap();
    let stack = STACK_BASE + 0x8000;
    app.runtime
        .memory
        .write(GuestPtr(stack), &[0, 6, 0, 8, 0, 7, 0, 5, 0xa5, 0x5a])
        .unwrap();
    app.runtime.cpu = Cpu::new(Architecture::M68k, 0x50000000, stack);
    assert_eq!(app.runtime.advance(2).unwrap(), Stop::Budget);
    let Cpu::M68k(c) = &app.runtime.cpu else {
        panic!()
    };
    assert_eq!(c.dar[15], stack + 8);
    assert_eq!(app.runtime.memory.read(GuestPtr(0x808), 8).unwrap(), bounds);
    assert_eq!(
        app.runtime.memory.read(GuestPtr(0x800), 108).unwrap(),
        expected
    );
    assert_eq!(
        app.runtime.memory.read(GuestPtr(0x818), 8).unwrap(),
        regions
    );
}
