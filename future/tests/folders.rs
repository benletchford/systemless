mod support;
use systemless_future::{
    cpu::{Architecture, Cpu},
    files::FindFolder,
    load::{LoadedApplication, STACK_BASE},
    memory::{GuestPtr, Permissions},
    runtime::Stop,
};
const DATA: GuestPtr = GuestPtr(0x50000000);
const CODE: u32 = 0x51000000;
const STACK: u32 = STACK_BASE + 0x8000;
fn app(boot: bool) -> LoadedApplication {
    let f = support::pef_program(&[0x4e800020], &[b"FindFolder", b"SysEnvirons"], 0);
    let mut a = LoadedApplication::pef(&f.bytes, None).unwrap();
    a.runtime
        .memory
        .map(DATA, vec![0xa5; 512], Permissions::RW)
        .unwrap();
    a.runtime
        .memory
        .map(
            GuestPtr(CODE),
            vec![0xa8, 0x23, 0xa0, 0x90],
            Permissions::RX,
        )
        .unwrap();
    if boot {
        a.runtime.prepare_application_volume().unwrap();
    }
    a
}
fn request(kind: [u8; 4], create: bool) -> FindFolder {
    FindFolder {
        volume: i16::MIN,
        kind,
        create,
        found_volume: DATA,
        directory: GuestPtr(DATA.0 + 4),
    }
}
fn call(
    a: &mut LoadedApplication,
    arch: Architecture,
    r: FindFolder,
) -> systemless_future::Result<i16> {
    let pc = if arch == Architecture::M68k {
        CODE
    } else {
        let tv = a
            .runtime
            .memory
            .u32(GuestPtr(a.info.toc_or_a5 + 8))
            .unwrap();
        a.runtime.memory.u32(GuestPtr(tv)).unwrap()
    };
    a.runtime.cpu = Cpu::new(arch, pc, STACK);
    a.runtime
        .memory
        .write(GuestPtr(STACK), &[0x5a; 24])
        .unwrap();
    match &mut a.runtime.cpu {
        Cpu::M68k(c) => {
            let mut args = r.directory.0.to_be_bytes().to_vec();
            args.extend(r.found_volume.0.to_be_bytes());
            args.extend([u8::from(r.create), 0xa5]);
            args.extend(r.kind);
            args.extend(r.volume.to_be_bytes());
            a.runtime.memory.write(GuestPtr(STACK), &args).unwrap();
            c.dar[0] = 0;
            c.dar[1] = 0x12345678;
            c.dar[8] = 0xabcdef98;
        }
        Cpu::PowerPc(c) => {
            c.gpr[3] = r.volume as i32 as u32;
            c.gpr[4] = u32::from_be_bytes(r.kind);
            c.gpr[5] = u32::from(r.create);
            c.gpr[6] = r.found_volume.0;
            c.gpr[7] = r.directory.0;
        }
    }
    a.runtime.process.mem_error = -99;
    a.runtime.process.res_error = -98;
    assert_eq!(a.runtime.advance(1)?, Stop::Budget);
    assert_eq!(
        (a.runtime.process.mem_error, a.runtime.process.res_error),
        (-99, -98)
    );
    let error = match &a.runtime.cpu {
        Cpu::M68k(c) => {
            assert_eq!(c.dar[15], STACK + 16);
            assert_eq!(c.dar[1], 0x12345678);
            assert_eq!(c.dar[8], 0xabcdef98);
            assert_eq!(
                a.runtime.memory.read(GuestPtr(STACK + 18), 6).unwrap(),
                [0x5a; 6]
            );
            a.runtime.memory.u16(GuestPtr(STACK + 16)).unwrap() as i16
        }
        Cpu::PowerPc(c) => {
            assert_eq!(c.gpr[4], u32::from_be_bytes(r.kind));
            assert_eq!(c.gpr[5], u32::from(r.create));
            assert_eq!(c.gpr[6], r.found_volume.0);
            assert_eq!(c.gpr[7], r.directory.0);
            c.gpr[3] as i16
        }
    };
    Ok(error)
}
fn id(a: &LoadedApplication) -> u32 {
    assert_eq!(a.runtime.memory.u16(DATA).unwrap() as i16, -1);
    a.runtime.memory.u32(GuestPtr(DATA.0 + 4)).unwrap()
}
#[test]
fn both_cpus_find_create_and_reuse_all_supported_folder_types_in_one_catalog() {
    assert_eq!(
        systemless_future::environment::gestalt(u32::from_be_bytes(*b"fold")),
        Ok(1)
    );
    let mut a = app(true);
    assert_eq!(
        call(&mut a, Architecture::M68k, request(*b"macs", false)).unwrap(),
        0
    );
    let system = id(&a);
    for kind in [
        *b"pref", *b"amnu", *b"ctrl", *b"extn", *b"font", *b"prnt", *b"strt", *b"desk", *b"temp",
        *b"trsh", *b"empt",
    ] {
        let sys = matches!(
            &kind,
            b"pref" | b"amnu" | b"ctrl" | b"extn" | b"font" | b"prnt" | b"strt"
        );
        assert_eq!(
            call(&mut a, Architecture::M68k, request(kind, true)).unwrap(),
            0
        );
        let expected = id(&a);
        let before = a.runtime.process.files.export_volume().unwrap();
        assert_eq!(
            call(&mut a, Architecture::PowerPc, request(kind, false)).unwrap(),
            0
        );
        assert_eq!(id(&a), expected);
        assert_eq!(a.runtime.process.files.export_volume().unwrap(), before);
        assert_eq!(
            a.runtime
                .process
                .files
                .directory_view(expected)
                .unwrap()
                .parent,
            if sys { system } else { 2 }
        );
    }
}
#[test]
fn lookup_never_creates_system_folder_and_failures_preserve_catalog_and_outputs() {
    for arch in [Architecture::M68k, Architecture::PowerPc] {
        let mut a = app(false);
        for r in [
            request(*b"macs", true),
            request(*b"pref", true),
            request(*b"none", true),
        ] {
            let before = a.runtime.process.files.export_volume().unwrap();
            let output = a.runtime.memory.read(DATA, 8).unwrap().to_vec();
            assert_eq!(call(&mut a, arch, r).unwrap(), -43);
            assert_eq!(a.runtime.process.files.export_volume().unwrap(), before);
            assert_eq!(a.runtime.memory.read(DATA, 8).unwrap(), output);
        }
        a.runtime.prepare_application_volume().unwrap();
        let mut r = request(*b"pref", true);
        r.volume = 99;
        assert_eq!(call(&mut a, arch, r).unwrap(), -35);
        assert_eq!(call(&mut a, arch, request(*b"pref", false)).unwrap(), -43);
    }
}
#[test]
fn output_faults_do_not_create_directories_or_pop_native_arguments() {
    for arch in [Architecture::M68k, Architecture::PowerPc] {
        let mut a = app(true);
        let ro = GuestPtr(0x52000000);
        a.runtime
            .memory
            .map(ro, vec![0xbb; 4], Permissions::RX)
            .unwrap();
        let before = a.runtime.process.files.export_volume().unwrap();
        let output = a.runtime.memory.read(DATA, 8).unwrap().to_vec();
        let mut r = request(*b"pref", true);
        r.directory = ro;
        assert!(call(&mut a, arch, r).is_err());
        assert_eq!(a.runtime.process.files.export_volume().unwrap(), before);
        assert_eq!(a.runtime.memory.read(DATA, 8).unwrap(), output);
        assert_eq!(a.runtime.memory.read(ro, 4).unwrap(), [0xbb; 4]);
        if let Cpu::M68k(c) = &a.runtime.cpu {
            assert_eq!(c.dar[15], STACK);
        }
    }
}
#[test]
fn folder_identity_round_trips_and_is_the_same_volume_finder_blessing() {
    let mut a = app(true);
    call(&mut a, Architecture::PowerPc, request(*b"macs", false)).unwrap();
    let system = id(&a);
    call(&mut a, Architecture::M68k, request(*b"pref", true)).unwrap();
    let prefs = id(&a);
    let image = a.runtime.process.files.export_volume().unwrap();
    assert_eq!(u32::from_be_bytes(image[8..12].try_into().unwrap()), 3);
    assert_eq!(
        u32::from_be_bytes(image[32..36].try_into().unwrap()),
        system
    );
    let mut b = app(false);
    b.runtime.process.files.restore_volume(&image).unwrap();
    b.runtime.prepare_application_volume().unwrap();
    call(&mut b, Architecture::M68k, request(*b"macs", false)).unwrap();
    assert_eq!(id(&b), system);
    call(&mut b, Architecture::PowerPc, request(*b"pref", false)).unwrap();
    assert_eq!(id(&b), prefs);
    assert_eq!(b.runtime.process.files.export_volume().unwrap(), image);
    let pb = GuestPtr(DATA.0 + 128);
    b.runtime.memory.write(pb, &[0; 122]).unwrap();
    assert_eq!(
        b.runtime
            .process
            .files
            .volume_info(&mut b.runtime.memory, pb, true)
            .unwrap(),
        0
    );
    assert_eq!(b.runtime.memory.u32(GuestPtr(pb.0 + 90)).unwrap(), system);
}

#[test]
fn sysenvirons_reports_the_same_system_volume_after_host_initialization() {
    for arch in [Architecture::M68k, Architecture::PowerPc] {
        let mut a = app(false);
        for expected in [0i16, -1] {
            if expected == -1 {
                a.runtime.prepare_application_volume().unwrap();
            }
            let output = GuestPtr(DATA.0 + 64);
            let pc = if arch == Architecture::M68k {
                CODE + 2
            } else {
                let tv = a
                    .runtime
                    .memory
                    .u32(GuestPtr(a.info.toc_or_a5 + 12))
                    .unwrap();
                a.runtime.memory.u32(GuestPtr(tv)).unwrap()
            };
            a.runtime.cpu = Cpu::new(arch, pc, STACK);
            match &mut a.runtime.cpu {
                Cpu::M68k(c) => {
                    c.dar[0] = 2;
                    c.dar[8] = output.0;
                }
                Cpu::PowerPc(c) => {
                    c.gpr[3] = 2;
                    c.gpr[4] = output.0;
                }
            }
            assert_eq!(a.runtime.advance(1).unwrap(), Stop::Budget);
            assert_eq!(
                a.runtime.memory.u16(GuestPtr(output.0 + 14)).unwrap() as i16,
                expected
            );
        }
    }
}

#[test]
fn a_file_in_place_of_a_folder_returns_dupfnerr_without_partial_creation() {
    for arch in [Architecture::M68k, Architecture::PowerPc] {
        let mut a = app(true);
        assert_eq!(call(&mut a, arch, request(*b"macs", false)).unwrap(), 0);
        let system = id(&a);
        let pb = GuestPtr(DATA.0 + 128);
        let name = GuestPtr(DATA.0 + 256);
        a.runtime.memory.write(pb, &[0; 64]).unwrap();
        a.runtime.memory.write(name, b"\x0bpreferences").unwrap();
        for (offset, bytes) in [
            (18, name.0.to_be_bytes().to_vec()),
            (22, (-1i16).to_be_bytes().to_vec()),
            (48, system.to_be_bytes().to_vec()),
        ] {
            a.runtime
                .memory
                .write(GuestPtr(pb.0 + offset), &bytes)
                .unwrap();
        }
        assert_eq!(
            a.runtime
                .process
                .files
                .io(
                    &mut a.runtime.memory,
                    pb,
                    systemless_future::files::Op::Create { hierarchical: true }
                )
                .unwrap(),
            0
        );
        let before = a.runtime.process.files.export_volume().unwrap();
        let output = a.runtime.memory.read(DATA, 8).unwrap().to_vec();
        for create in [false, true] {
            assert_eq!(call(&mut a, arch, request(*b"pref", create)).unwrap(), -48);
            assert_eq!(a.runtime.process.files.export_volume().unwrap(), before);
            assert_eq!(a.runtime.memory.read(DATA, 8).unwrap(), output);
        }
    }
}
