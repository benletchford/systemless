mod support;
use systemless_future::{
    cpu::{Architecture, Cpu},
    cursor::Image,
    graphics::Draw,
    load::resource::ResourceFork,
    load::{LoadedApplication, STACK_BASE},
    memory::{GuestPtr, Memory, Permissions},
    process::{Process, HEAP_SIZE},
    runtime::Stop,
};
#[test]
fn standard_cursor_handles_share_identity_and_restore_bytes_after_release_or_detach() {
    let mut p = Process::default();
    let mut m = Memory::default();
    for (id, hot, first) in [
        (1, [8i16, 4], 0x0ee0),
        (2, [7, 7], 0x0100),
        (3, [8, 8], 0),
        (4, [8, 8], 0x07c0),
    ] {
        let h = p.get_resource(&mut m, *b"CURS", id).unwrap();
        assert_ne!(h, 0);
        assert_eq!(p.res_error, 0);
        assert_eq!(p.handle_size(&m, h).unwrap(), 68);
        assert_eq!(p.get_resource(&mut m, *b"CURS", id).unwrap(), h);
        let data = m.u32(GuestPtr(h)).unwrap();
        let image = Image::read(&m, GuestPtr(data)).unwrap();
        assert_eq!([image.vertical, image.horizontal], hot);
        assert_eq!(image.data[0], first);
        let original = m.read(GuestPtr(data), 68).unwrap().to_vec();
        m.write(GuestPtr(data), &[0xff; 68]).unwrap();
        p.detach_resource(&m, h).unwrap();
        assert_eq!(p.res_error, 0);
        let next = p.get_resource(&mut m, *b"CURS", id).unwrap();
        assert_ne!(next, h);
        let next_data = m.u32(GuestPtr(next)).unwrap();
        assert_eq!(m.read(GuestPtr(next_data), 68).unwrap(), original);
        p.release_resource(&mut m, next).unwrap();
        assert_eq!(m.u32(GuestPtr(next)).unwrap(), 0);
        assert_eq!(p.handle_size(&m, h).unwrap(), 68);
        p.dispose_handle(&mut m, h).unwrap();
    }
    assert_eq!(p.get_resource(&mut m, *b"CURS", 999).unwrap(), 0);
    assert_eq!(p.res_error, -192);
    assert_eq!(p.get_resource(&mut m, *b"NONE", 1).unwrap(), 0);
    assert_eq!(p.res_error, 0);
    m.map(GuestPtr(0x1000), vec![0], Permissions::RW).unwrap();
    assert_eq!(
        p.get_named_resource(&mut m, *b"CURS", GuestPtr(0x1000), false)
            .unwrap(),
        0
    );
    assert_eq!(p.res_error, -192);
    assert_eq!(
        p.get_named_resource(&mut m, *b"CURS", GuestPtr(0x1000), true)
            .unwrap(),
        0
    );
    assert_eq!(p.res_error, 0);
    let all = p.new_handle(&mut m, HEAP_SIZE - 4, false).unwrap();
    assert_ne!(all, 0);
    assert_eq!(p.get_resource(&mut m, *b"CURS", 4).unwrap(), 0);
    assert_eq!(p.res_error, -108);
    p.dispose_handle(&mut m, all).unwrap();
    assert_ne!(p.get_resource(&mut m, *b"CURS", 4).unwrap(), 0);
}
#[test]
fn application_cursor_records_take_precedence_even_when_malformed_or_compressed() {
    let bytes = support::resource(vec![(*b"CURS", 4, None, vec![0x5a; 68])]);
    let mut p = Process::default();
    p.install_application_resources(ResourceFork::parse(&bytes).unwrap());
    let mut m = Memory::default();
    let h = p.get_resource(&mut m, *b"CURS", 4).unwrap();
    let data = m.u32(GuestPtr(h)).unwrap();
    assert_eq!(m.read(GuestPtr(data), 68).unwrap(), [0x5a; 68]);
    assert_ne!(p.get_resource(&mut m, *b"CURS", 1).unwrap(), 0);
    assert!(p.resources.get(*b"CURS", 1).is_none());
    p.release_resource(&mut m, h).unwrap();
    assert_ne!(p.get_resource(&mut m, *b"CURS", 4).unwrap(), 0);
    for compressed in [false, true] {
        let mut bytes = support::resource(vec![(*b"CURS", 4, None, vec![1, 2])]);
        if compressed {
            let map = u32::from_be_bytes(bytes[4..8].try_into().unwrap()) as usize;
            let types =
                map + u16::from_be_bytes(bytes[map + 24..map + 26].try_into().unwrap()) as usize;
            let reference = types
                + u16::from_be_bytes(bytes[types + 8..types + 10].try_into().unwrap()) as usize;
            bytes[reference + 4] = 1;
        }
        let mut p = Process::default();
        p.install_application_resources(ResourceFork::parse(&bytes).unwrap());
        let mut m = Memory::default();
        let result = p.get_resource(&mut m, *b"CURS", 4);
        if compressed {
            assert!(result.is_err());
        } else {
            let h = result.unwrap();
            assert_eq!(p.handle_size(&m, h).unwrap(), 2);
            let data = m.u32(GuestPtr(h)).unwrap();
            assert_eq!(m.read(GuestPtr(data), 2).unwrap(), [1, 2]);
        }
    }
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
    for (i, &v) in args.iter().enumerate() {
        c.gpr[i + 3] = v;
    }
    assert_eq!(app.runtime.advance(1).unwrap(), Stop::Budget);
    let Cpu::PowerPc(c) = &app.runtime.cpu else {
        panic!()
    };
    c.gpr[3]
}
#[test]
fn both_cpus_get_the_same_watch_handle_and_cursor_keeps_its_copy_after_release() {
    let f = support::pef_program(
        &[0x4e800020],
        &[
            b"GetCursor",
            b"GetResource",
            b"SetCursor",
            b"ReleaseResource",
        ],
        0,
    );
    let mut app = LoadedApplication::pef(&f.bytes, None).unwrap();
    let h = invoke(&mut app, 0, &[4]);
    assert_ne!(h, 0);
    assert_eq!(invoke(&mut app, 1, &[u32::from_be_bytes(*b"CURS"), 4]), h);
    let sp = STACK_BASE + 0x8000;
    app.runtime
        .memory
        .map(GuestPtr(0x50000000), vec![0xa9, 0xb9], Permissions::RX)
        .unwrap();
    app.runtime
        .memory
        .write(GuestPtr(sp), &[0, 4, 0xaa, 0xaa, 0xaa, 0xaa, 0x5a, 0xa5])
        .unwrap();
    app.runtime.cpu = Cpu::new(Architecture::M68k, 0x50000000, sp);
    let Cpu::M68k(c) = &mut app.runtime.cpu else {
        panic!()
    };
    c.dar[0] = 0xfeedbeef;
    assert_eq!(app.runtime.advance(1).unwrap(), Stop::Budget);
    let Cpu::M68k(c) = &app.runtime.cpu else {
        panic!()
    };
    assert_eq!(c.dar[0], 0xfeedbeef);
    assert_eq!(c.dar[15], sp + 2);
    assert_eq!(app.runtime.memory.u32(GuestPtr(sp + 2)).unwrap(), h);
    assert_eq!(
        app.runtime.memory.read(GuestPtr(sp + 6), 2).unwrap(),
        [0x5a, 0xa5]
    );
    let data = app.runtime.memory.u32(GuestPtr(h)).unwrap();
    invoke(&mut app, 2, &[data]);
    invoke(&mut app, 3, &[h]);
    assert_eq!(app.runtime.memory.u32(GuestPtr(h)).unwrap(), 0);
    match app.runtime.process.cursor.draw(app.runtime.process.input) {
        Draw::Cursor { data, mask, x, y } => {
            assert_eq!(data[0], 0x07c0);
            assert_eq!(data, mask);
            assert_eq!((x, y), (-8, -8));
        }
        _ => panic!(),
    }
    let reloaded = invoke(&mut app, 0, &[4]);
    assert_ne!(reloaded, 0);
    assert_eq!(
        Image::read(
            &app.runtime.memory,
            GuestPtr(app.runtime.memory.u32(GuestPtr(reloaded)).unwrap())
        )
        .unwrap()
        .data[0],
        0x07c0
    );
}
