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

#[test]
fn catalog_metadata_matches_loaded_forks_and_native_cpu_results() {
    let fork = code_program(&[0x207c, 0x5000, 0, 0x7009, 0xa260, 0xa9f4]);
    let mut m = LoadedApplication::code(&fork).unwrap();
    let f = pef_program(
        &[
            0x7fc802a6, 0x3c605000, 0x81820008, 0x800c0000, 0x804c0004, 0x7c0903a6, 0x4e800421,
            0x7fc803a6, 0x4e800020,
        ],
        &[b"PBGetCatInfoSync"],
        16,
    );
    let mut p = LoadedApplication::pef(&f.bytes, Some(&fork)).unwrap();
    for app in [&mut m, &mut p] {
        app.runtime.process.files.set_name(b"Probe").unwrap();
        setup(app, 0, 1);
        // Nonnegative lookup ignores ioDirID, which may hold an earlier file ID.
        app.runtime
            .memory
            .write(GuestPtr(PB + 48), &16u32.to_be_bytes())
            .unwrap();
        let expected_data = app
            .runtime
            .process
            .files
            .fork_bytes(Fork::Data)
            .unwrap()
            .len() as u32;
        assert!(matches!(
            app.runtime.advance(100).unwrap(),
            Stop::Exit(0) | Stop::Returned(0)
        ));
        let mem = &app.runtime.memory;
        assert_eq!(mem.u16(GuestPtr(PB + 16)).unwrap(), 0);
        assert_eq!(mem.u32(GuestPtr(PB + 48)).unwrap(), 16);
        assert_eq!(mem.u32(GuestPtr(PB + 54)).unwrap(), expected_data);
        assert_eq!(mem.u32(GuestPtr(PB + 64)).unwrap(), fork.len() as u32);
        assert_eq!(mem.u32(GuestPtr(PB + 100)).unwrap(), 2);
        assert_eq!(mem.read(GuestPtr(PB + 30), 1).unwrap(), [0x85]);
        assert_eq!(mem.read(GuestPtr(PB + 128), 6).unwrap(), b"\x05Probe");
    }
}

#[test]
fn catalog_names_directories_errors_and_output_faults_are_checked() {
    let mut app = LoadedApplication::code(&code()).unwrap();
    setup(&mut app, 0, 0);
    app.runtime.process.files.set_name(b"Probe").unwrap();
    let (m, files) = (&mut app.runtime.memory, &app.runtime.process.files);
    m.write(GuestPtr(PB + 128), b"\x06:pRoBe").unwrap();
    assert_eq!(files.catalog_info(m, GuestPtr(PB)).unwrap(), 0);
    m.write(GuestPtr(PB + 28), &(-1i16).to_be_bytes()).unwrap();
    m.write(GuestPtr(PB + 48), &2u32.to_be_bytes()).unwrap();
    m.write(GuestPtr(PB + 18), &0u32.to_be_bytes()).unwrap();
    assert_eq!(files.catalog_info(m, GuestPtr(PB)).unwrap(), 0);
    assert_eq!(m.read(GuestPtr(PB + 30), 1).unwrap(), [16]);
    assert_eq!(m.u32(GuestPtr(PB + 48)).unwrap(), 2);
    assert_eq!(m.u16(GuestPtr(PB + 52)).unwrap(), 1);
    assert_eq!(m.u32(GuestPtr(PB + 100)).unwrap(), 1);
    for (volume, index, dir, error) in [(99, 1, 2, -35i16), (0, -2, 123, -120), (0, 2, 2, -43)] {
        m.write(GuestPtr(PB + 22), &(volume as i16).to_be_bytes())
            .unwrap();
        m.write(GuestPtr(PB + 28), &(index as i16).to_be_bytes())
            .unwrap();
        m.write(GuestPtr(PB + 48), &(dir as u32).to_be_bytes())
            .unwrap();
        let mut before = m.read(GuestPtr(PB), 108).unwrap().to_vec();
        before[12..16].fill(0);
        w16(&mut before, 16, error as u16);
        assert_eq!(files.catalog_info(m, GuestPtr(PB)).unwrap(), error);
        assert_eq!(m.read(GuestPtr(PB), 108).unwrap(), before);
    }
    m.map(GuestPtr(PB + 512), vec![0; 32], Permissions::RX)
        .unwrap();
    m.write(GuestPtr(PB + 22), &0u16.to_be_bytes()).unwrap();
    m.write(GuestPtr(PB + 28), &1u16.to_be_bytes()).unwrap();
    m.write(GuestPtr(PB + 18), &(PB + 512).to_be_bytes())
        .unwrap();
    let before = m.read(GuestPtr(PB), 108).unwrap().to_vec();
    assert!(files.catalog_info(m, GuestPtr(PB)).is_err());
    assert_eq!(m.read(GuestPtr(PB), 108).unwrap(), before);
}

#[test]
fn directory_creation_is_shared_across_cpu_abis_and_catalog_queries() {
    let fork = code_program(&[0x207c, 0x5000, 0, 0x7006, 0xa260, 0xa9f4]);
    let mut m = LoadedApplication::code(&fork).unwrap();
    let f = pef_program(
        &[
            0x7fc802a6, 0x3c605000, 0x81820008, 0x800c0000, 0x804c0004, 0x7c0903a6, 0x4e800421,
            0x7fc803a6, 0x4e800020,
        ],
        &[b"PBDirCreateSync"],
        16,
    );
    let mut p = LoadedApplication::pef(&f.bytes, Some(&fork)).unwrap();
    for app in [&mut m, &mut p] {
        setup(app, 0, 0);
        app.runtime
            .memory
            .write(GuestPtr(PB + 128), b"\x05Prefs")
            .unwrap();
        app.runtime
            .memory
            .write(GuestPtr(0x20c), &12345u32.to_be_bytes())
            .unwrap();
        assert!(matches!(
            app.runtime.advance(100).unwrap(),
            Stop::Exit(0) | Stop::Returned(0)
        ));
        assert_eq!(app.runtime.memory.u16(GuestPtr(PB + 16)).unwrap(), 0);
        let id = app.runtime.memory.u32(GuestPtr(PB + 48)).unwrap();
        assert_eq!(id, 17);
        app.runtime
            .memory
            .write(GuestPtr(PB + 28), &(-1i16).to_be_bytes())
            .unwrap();
        let (files, memory) = (&app.runtime.process.files, &mut app.runtime.memory);
        assert_eq!(files.catalog_info(memory, GuestPtr(PB)).unwrap(), 0);
        assert_eq!(memory.u32(GuestPtr(PB + 48)).unwrap(), id);
        assert_eq!(memory.u32(GuestPtr(PB + 100)).unwrap(), 2);
        assert_eq!(memory.u32(GuestPtr(PB + 72)).unwrap(), 12345);
        assert_eq!(memory.u16(GuestPtr(PB + 52)).unwrap(), 0);
        assert_eq!(memory.read(GuestPtr(PB + 128), 6).unwrap(), b"\x05Prefs");
    }
}

#[test]
fn directory_paths_duplicates_errors_and_failed_outputs_preserve_catalog() {
    let mut app = LoadedApplication::code(&code()).unwrap();
    setup(&mut app, 0, 0);
    let (files, m) = (&mut app.runtime.process.files, &mut app.runtime.memory);
    m.write(GuestPtr(PB + 128), b"\x05Prefs").unwrap();
    assert_eq!(files.create_directory(m, GuestPtr(PB)).unwrap(), 0);
    let parent = m.u32(GuestPtr(PB + 48)).unwrap();
    m.write(GuestPtr(PB + 48), &0u32.to_be_bytes()).unwrap();
    m.write(GuestPtr(PB + 128), b"\x0d:Prefs:Nested").unwrap();
    // Correct Pascal length is 13 (including two separators).
    assert_eq!(files.create_directory(m, GuestPtr(PB)).unwrap(), 0);
    let child = m.u32(GuestPtr(PB + 48)).unwrap();
    m.write(GuestPtr(PB + 28), &(-1i16).to_be_bytes()).unwrap();
    assert_eq!(files.catalog_info(m, GuestPtr(PB)).unwrap(), 0);
    assert_eq!(m.u32(GuestPtr(PB + 100)).unwrap(), parent);
    m.write(GuestPtr(PB + 28), &0u16.to_be_bytes()).unwrap();
    m.write(GuestPtr(PB + 128), b"\x0d:pReFs:nEsTeD").unwrap();
    assert_eq!(files.catalog_info(m, GuestPtr(PB)).unwrap(), 0);
    assert_eq!(m.u32(GuestPtr(PB + 48)).unwrap(), child);
    m.write(GuestPtr(PB + 128), b"\x1fApplication Volume:Prefs:Nested")
        .unwrap();
    assert_eq!(files.catalog_info(m, GuestPtr(PB)).unwrap(), 0);
    assert_eq!(m.u32(GuestPtr(PB + 48)).unwrap(), child);
    m.write(GuestPtr(PB + 128), b"\x0f:Prefs:Nested::").unwrap();
    assert_eq!(files.catalog_info(m, GuestPtr(PB)).unwrap(), 0);
    assert_eq!(m.u32(GuestPtr(PB + 48)).unwrap(), parent);
    m.write(GuestPtr(PB + 28), &2u16.to_be_bytes()).unwrap();
    assert_eq!(files.catalog_info(m, GuestPtr(PB)).unwrap(), 0);
    assert_eq!(m.u32(GuestPtr(PB + 48)).unwrap(), parent);
    m.write(GuestPtr(PB + 128), b"\x06Nested").unwrap();
    m.write(GuestPtr(PB + 48), &parent.to_be_bytes()).unwrap();
    assert_eq!(files.create_directory(m, GuestPtr(PB)).unwrap(), -48);
    for (volume, dir, name, error) in [
        (99, 2, b"\x01X".as_slice(), -35),
        (0, 999, b"\x01X", -120),
        (0, 2, b"\x00", -37),
    ] {
        m.write(GuestPtr(PB + 22), &(volume as i16).to_be_bytes())
            .unwrap();
        m.write(GuestPtr(PB + 48), &(dir as u32).to_be_bytes())
            .unwrap();
        m.write(GuestPtr(PB + 128), name).unwrap();
        let mut before = m.read(GuestPtr(PB), 52).unwrap().to_vec();
        before[12..16].fill(0);
        w16(&mut before, 16, error as u16);
        assert_eq!(files.create_directory(m, GuestPtr(PB)).unwrap(), error);
        assert_eq!(m.read(GuestPtr(PB), 52).unwrap(), before);
    }
    let mut pb = m.read(GuestPtr(PB), 52).unwrap().to_vec();
    w16(&mut pb, 22, 0);
    w32(&mut pb, 48, 2);
    m.write(GuestPtr(PB + 128), b"\x06Atomic").unwrap();
    m.map(GuestPtr(PB + 512), pb, Permissions::RX).unwrap();
    assert!(files.create_directory(m, GuestPtr(PB + 512)).is_err());
    m.write(GuestPtr(PB + 22), &0u16.to_be_bytes()).unwrap();
    m.write(GuestPtr(PB + 28), &0u16.to_be_bytes()).unwrap();
    assert_eq!(files.catalog_info(m, GuestPtr(PB)).unwrap(), -43);
    m.write(GuestPtr(PB + 48), &parent.to_be_bytes()).unwrap();
    m.write(GuestPtr(PB + 28), &(-1i16).to_be_bytes()).unwrap();
    assert_eq!(files.catalog_info(m, GuestPtr(PB)).unwrap(), 0);
    assert_eq!(m.u16(GuestPtr(PB + 52)).unwrap(), 1);
}
