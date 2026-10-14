mod support;
use support::*;
use systemless_future::{
    files::Fork,
    load::LoadedApplication,
    memory::{GuestPtr, Permissions},
    runtime::Stop,
};
const PB: u32 = 0x5000_0000;
fn setup(app: &mut LoadedApplication, reference: i16, index: i16) {
    let mut data = vec![0; 256];
    w32(&mut data, 18, PB + 128);
    w16(&mut data, 24, reference as u16);
    w16(&mut data, 28, index as u16);
    app.runtime
        .memory
        .map(GuestPtr(PB), data, Permissions::RW)
        .unwrap();
}
#[test]
fn loaded_forks_back_fcb_metadata_through_both_cpu_abis() {
    let fork = code_program(&[0x207c, 0x5000, 0, 0x7008, 0xa260, 0xa9f4]);
    let mut m = LoadedApplication::code(&fork).unwrap();
    let f = pef_program(
        &[
            0x7fc802a6, 0x3c605000, 0x81820008, 0x800c0000, 0x804c0004, 0x7c0903a6, 0x4e800421,
            0x7fc803a6, 0x4e800020,
        ],
        &[b"PBGetFCBInfoSync"],
        16,
    );
    let mut p = LoadedApplication::pef(&f.bytes, Some(&fork)).unwrap();
    assert_eq!(
        p.runtime.process.files.fork_bytes(Fork::Data).unwrap(),
        f.bytes
    );
    for app in [&mut m, &mut p] {
        assert_eq!(
            app.runtime
                .process
                .files
                .fork_bytes(Fork::Resource)
                .unwrap(),
            fork
        );
        app.runtime.process.files.set_name(b"Probe").unwrap();
        setup(app, app.runtime.process.current_resource_file(), 0);
        let stop = app.runtime.advance(100).unwrap();
        assert!(matches!(stop, Stop::Exit(0) | Stop::Returned(0)));
        let mem = &app.runtime.memory;
        assert_eq!(mem.u16(GuestPtr(PB + 16)).unwrap(), 0);
        assert_eq!(mem.u32(GuestPtr(PB + 32)).unwrap(), 16);
        assert_eq!(mem.u16(GuestPtr(PB + 36)).unwrap(), 0x2200);
        assert_eq!(mem.u32(GuestPtr(PB + 40)).unwrap(), fork.len() as u32);
        assert_eq!(
            mem.u32(GuestPtr(PB + 44)).unwrap(),
            (fork.len() as u32).div_ceil(4096) * 4096
        );
        assert_eq!(mem.u16(GuestPtr(PB + 52)).unwrap() as i16, -1);
        assert_eq!(mem.u32(GuestPtr(PB + 58)).unwrap(), 2);
        assert_eq!(mem.read(GuestPtr(PB + 128), 6).unwrap(), b"\x05Probe");
    }
}
#[test]
fn fcb_indexing_distinguishes_forks_and_reports_errors_without_partial_outputs() {
    let f = pef();
    let fork = code();
    let mut app = LoadedApplication::pef(&f.bytes, Some(&fork)).unwrap();
    let dataref = app.runtime.process.files.open(Fork::Data).unwrap();
    setup(&mut app, 999, 2);
    assert_eq!(
        app.runtime
            .process
            .files
            .fcb_info(&mut app.runtime.memory, GuestPtr(PB))
            .unwrap(),
        0
    );
    assert_eq!(
        app.runtime.memory.u16(GuestPtr(PB + 24)).unwrap() as i16,
        dataref
    );
    assert_eq!(app.runtime.memory.u16(GuestPtr(PB + 36)).unwrap(), 0x2000);
    assert_eq!(
        app.runtime.memory.u32(GuestPtr(PB + 40)).unwrap(),
        f.bytes.len() as u32
    );
    for (index, volume, reference, error) in [
        (0, 0, 999, -51),
        (3, 0, 0, -51),
        (-1, 0, 0, -50),
        (1, 99, 0, -35),
    ] {
        let mut pb = vec![0xab; 62];
        w16(&mut pb, 28, index as u16);
        w16(&mut pb, 22, volume as u16);
        w16(&mut pb, 24, reference as u16);
        app.runtime.memory.write(GuestPtr(PB), &pb).unwrap();
        assert_eq!(
            app.runtime
                .process
                .files
                .fcb_info(&mut app.runtime.memory, GuestPtr(PB))
                .unwrap(),
            error
        );
        w16(&mut pb, 16, error as u16);
        assert_eq!(app.runtime.memory.read(GuestPtr(PB), 62).unwrap(), pb);
    }
}
#[test]
fn fcb_output_validation_is_atomic_and_missing_forks_do_not_open() {
    let f = pef();
    let mut app = LoadedApplication::pef(&f.bytes, None).unwrap();
    assert_eq!(app.runtime.process.current_resource_file(), 0);
    assert_eq!(app.runtime.process.files.open(Fork::Resource), Err(-43));
    let reference = app.runtime.process.files.open(Fork::Data).unwrap();
    setup(&mut app, reference, 0);
    app.runtime
        .memory
        .write(GuestPtr(PB + 18), &0xfffffff0u32.to_be_bytes())
        .unwrap();
    let before = app.runtime.memory.read(GuestPtr(PB), 62).unwrap().to_vec();
    assert!(app
        .runtime
        .process
        .files
        .fcb_info(&mut app.runtime.memory, GuestPtr(PB))
        .is_err());
    assert_eq!(app.runtime.memory.read(GuestPtr(PB), 62).unwrap(), before);
    assert_eq!(
        app.runtime.process.files.set_name(b"invalid:name"),
        Err(-37)
    );
}
