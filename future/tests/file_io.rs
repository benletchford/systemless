mod support;
use systemless_future::{
    cpu::{Architecture, Cpu},
    files::{Fork, Op},
    load::{LoadedApplication, STACK_BASE},
    memory::{GuestPtr, Permissions},
    runtime::Stop,
};
const PB: GuestPtr = GuestPtr(0x50000000);
const NAME: GuestPtr = GuestPtr(0x50000080);
const BUFFER: GuestPtr = GuestPtr(0x50000180);
fn app() -> LoadedApplication {
    let fixture = support::pef_program(
        &[0x4e800020],
        &[
            b"PBHCreateSync",
            b"PBHOpenSync",
            b"PBWriteSync",
            b"PBReadSync",
            b"PBCloseSync",
            b"PBGetEOFSync",
            b"PBSetEOFSync",
            b"PBGetFPosSync",
            b"PBSetFPosSync",
            b"PBHOpenRFSync",
            b"PBFlushFileSync",
            b"PBHSetFInfoSync",
        ],
        16,
    );
    let mut app = LoadedApplication::pef(&fixture.bytes, Some(&support::code())).unwrap();
    app.runtime
        .memory
        .map(PB, vec![0xa5; 1024], Permissions::RW)
        .unwrap();
    let traps = [
        0xa208u16, 0xa200, 0xa003, 0xa002, 0xa001, 0xa011, 0xa012, 0xa018, 0xa044, 0xa20a, 0xa045,
        0xa20d,
    ];
    app.runtime
        .memory
        .map(
            GuestPtr(0x51000000),
            traps.into_iter().flat_map(u16::to_be_bytes).collect(),
            Permissions::RX,
        )
        .unwrap();
    app
}
fn fresh(app: &mut LoadedApplication, name: &[u8], parent: u32, permission: u8) {
    let mut pb = vec![0; 80];
    pb[18..22].copy_from_slice(&NAME.0.to_be_bytes());
    pb[27] = permission;
    pb[48..52].copy_from_slice(&parent.to_be_bytes());
    app.runtime.memory.write(PB, &pb).unwrap();
    let mut string = vec![name.len() as u8];
    string.extend(name);
    app.runtime.memory.write(NAME, &string).unwrap();
}
fn write16(app: &mut LoadedApplication, at: u32, value: i16) {
    app.runtime
        .memory
        .write(GuestPtr(PB.0 + at), &value.to_be_bytes())
        .unwrap();
}
fn write32(app: &mut LoadedApplication, at: u32, value: u32) {
    app.runtime
        .memory
        .write(GuestPtr(PB.0 + at), &value.to_be_bytes())
        .unwrap();
}
fn direct(app: &mut LoadedApplication, op: Op) -> i16 {
    app.runtime
        .process
        .files
        .io(&mut app.runtime.memory, PB, op)
        .unwrap()
}
fn native(app: &mut LoadedApplication, index: u32, arch: Architecture) -> i16 {
    let pc = if arch == Architecture::M68k {
        0x51000000 + index * 2
    } else {
        let vector = app
            .runtime
            .memory
            .u32(GuestPtr(app.info.toc_or_a5 + 8 + index * 4))
            .unwrap();
        app.runtime.memory.u32(GuestPtr(vector)).unwrap()
    };
    let stack = STACK_BASE + 0x8000;
    app.runtime.cpu = Cpu::new(arch, pc, stack);
    match &mut app.runtime.cpu {
        Cpu::M68k(c) => {
            c.dar[8] = PB.0;
            c.dar[0] = 0xa5a55a5a;
        }
        Cpu::PowerPc(c) => c.gpr[3] = PB.0,
    }
    assert_eq!(app.runtime.advance(1).unwrap(), Stop::Budget);
    let error = app.runtime.memory.u16(GuestPtr(PB.0 + 16)).unwrap() as i16;
    match &app.runtime.cpu {
        Cpu::M68k(c) => {
            assert_eq!(c.dar[0], error as i32 as u32);
            assert_eq!(c.dar[8], PB.0);
            assert_eq!(c.dar[15], stack);
            assert_eq!(
                c.get_sr() & 0xf,
                if error < 0 {
                    8
                } else if error == 0 {
                    4
                } else {
                    0
                }
            );
        }
        Cpu::PowerPc(c) => assert_eq!(c.gpr[3], error as i32 as u32),
    }
    error
}
fn open(
    app: &mut LoadedApplication,
    name: &[u8],
    permission: u8,
    kind: Fork,
) -> std::result::Result<i16, i16> {
    fresh(app, name, 2, permission);
    let error = direct(
        app,
        Op::Open {
            kind,
            hierarchical: true,
        },
    );
    if error == 0 {
        Ok(app.runtime.memory.u16(GuestPtr(PB.0 + 24)).unwrap() as i16)
    } else {
        Err(error)
    }
}
fn io(app: &mut LoadedApplication, reference: i16, count: i32, mode: i16, offset: i32) {
    fresh(app, b"", 0, 0);
    write16(app, 24, reference);
    write32(app, 32, BUFFER.0);
    write32(app, 36, count as u32);
    write16(app, 44, mode);
    write32(app, 46, offset as u32);
}
#[test]
fn real_cpus_share_created_file_bytes_forks_marks_catalog_and_finder_updates() {
    let mut app = app();
    fresh(&mut app, b"Settings", 2, 3);
    assert_eq!(native(&mut app, 0, Architecture::M68k), 0);
    assert_eq!(native(&mut app, 1, Architecture::PowerPc), 0);
    let data = app.runtime.memory.u16(GuestPtr(PB.0 + 24)).unwrap() as i16;
    app.runtime.memory.write(BUFFER, b"abcdef").unwrap();
    io(&mut app, data, 6, 0, 0);
    assert_eq!(native(&mut app, 2, Architecture::M68k), 0);
    assert_eq!(app.runtime.memory.u32(GuestPtr(PB.0 + 40)).unwrap(), 6);
    io(&mut app, data, 0, 0, 0);
    assert_eq!(native(&mut app, 7, Architecture::PowerPc), 0);
    assert_eq!(app.runtime.memory.u32(GuestPtr(PB.0 + 46)).unwrap(), 6);
    io(&mut app, data, 0, 1, 1);
    assert_eq!(native(&mut app, 8, Architecture::M68k), 0);
    io(&mut app, data, 8, 0, 0);
    assert_eq!(native(&mut app, 3, Architecture::PowerPc), -39);
    assert_eq!(app.runtime.memory.read(BUFFER, 5).unwrap(), b"bcdef");
    assert_eq!(app.runtime.memory.u32(GuestPtr(PB.0 + 40)).unwrap(), 5);
    io(&mut app, data, 0, 0, 0);
    assert_eq!(native(&mut app, 5, Architecture::M68k), 0);
    assert_eq!(app.runtime.memory.u32(GuestPtr(PB.0 + 28)).unwrap(), 6);
    write32(&mut app, 28, 3);
    assert_eq!(native(&mut app, 6, Architecture::PowerPc), 0);
    assert_eq!(native(&mut app, 10, Architecture::M68k), 0);
    fresh(&mut app, b"Settings", 2, 3);
    assert_eq!(native(&mut app, 9, Architecture::M68k), 0);
    let resource = app.runtime.memory.u16(GuestPtr(PB.0 + 24)).unwrap() as i16;
    app.runtime.memory.write(BUFFER, b"R").unwrap();
    io(&mut app, resource, 1, 0, 0);
    assert_eq!(native(&mut app, 2, Architecture::PowerPc), 0);
    let second = open(&mut app, b"settings", 1, Fork::Data).unwrap();
    io(&mut app, second, 8, 0, 0);
    assert_eq!(native(&mut app, 3, Architecture::M68k), -39);
    assert_eq!(app.runtime.memory.read(BUFFER, 3).unwrap(), b"abc");
    fresh(&mut app, b"Settings", 2, 0);
    app.runtime
        .memory
        .write(GuestPtr(PB.0 + 32), b"TEXTTEST")
        .unwrap();
    write32(&mut app, 72, 123);
    write32(&mut app, 76, 456);
    assert_eq!(native(&mut app, 11, Architecture::PowerPc), 0);
    fresh(&mut app, b"Settings", 2, 0);
    assert_eq!(
        app.runtime
            .process
            .files
            .catalog_info(&mut app.runtime.memory, PB)
            .unwrap(),
        0
    );
    let id = app.runtime.memory.u32(GuestPtr(PB.0 + 48)).unwrap();
    assert_eq!(id, 17);
    assert_eq!(
        app.runtime.memory.read(GuestPtr(PB.0 + 32), 8).unwrap(),
        b"TEXTTEST"
    );
    assert_eq!(app.runtime.memory.u32(GuestPtr(PB.0 + 54)).unwrap(), 3);
    assert_eq!(app.runtime.memory.u32(GuestPtr(PB.0 + 64)).unwrap(), 1);
    assert_eq!(app.runtime.memory.u32(GuestPtr(PB.0 + 76)).unwrap(), 456);
    for (reference, arch) in [
        (data, Architecture::M68k),
        (resource, Architecture::PowerPc),
        (second, Architecture::M68k),
    ] {
        io(&mut app, reference, 0, 0, 0);
        assert_eq!(native(&mut app, 4, arch), 0);
        assert_eq!(native(&mut app, 4, arch), -51);
    }
    let reopened = open(&mut app, b"Settings", 1, Fork::Resource).unwrap();
    io(&mut app, reopened, 1, 0, 0);
    assert_eq!(direct(&mut app, Op::Read), 0);
    assert_eq!(app.runtime.memory.read(BUFFER, 1).unwrap(), b"R");
    // The loader's resource path remains distinct from additional file forks.
    assert_eq!(app.runtime.process.current_resource_file(), 1);
}
#[test]
fn permissions_current_access_sharing_and_locked_application_are_substantive() {
    let mut app = app();
    fresh(&mut app, b"Settings", 2, 0);
    assert_eq!(direct(&mut app, Op::Create { hierarchical: true }), 0);
    let writer = open(&mut app, b"Settings", 3, Fork::Data).unwrap();
    assert_eq!(open(&mut app, b"Settings", 3, Fork::Data), Err(-49));
    assert_eq!(open(&mut app, b"Settings", 4, Fork::Data), Err(-49));
    let reader = open(&mut app, b"Settings", 0, Fork::Data).unwrap();
    io(&mut app, reader, 1, 0, 0);
    assert_eq!(direct(&mut app, Op::Write), -54);
    io(&mut app, writer, 0, 0, 0);
    assert_eq!(direct(&mut app, Op::Close), 0);
    let a = open(&mut app, b"Settings", 4, Fork::Data).unwrap();
    let b = open(&mut app, b"Settings", 4, Fork::Data).unwrap();
    assert_ne!(a, b);
    assert_eq!(open(&mut app, b"Settings", 255, Fork::Data), Err(-50));
    assert_eq!(open(&mut app, b"Missing", 3, Fork::Data), Err(-43));
    assert_eq!(open(&mut app, b"Bad\0Name", 1, Fork::Data), Err(-37));
    assert_eq!(open(&mut app, &[b'A'; 32], 1, Fork::Data), Err(-37));
    assert_eq!(open(&mut app, b"Application", 3, Fork::Data), Err(-45));
    assert!(open(&mut app, b"Application", 0, Fork::Data).is_ok());
    io(&mut app, a, 0, 0, 0);
    assert_eq!(direct(&mut app, Op::Close), 0);
    io(&mut app, b, 0, 0, 0);
    assert_eq!(direct(&mut app, Op::Close), 0);
    let alias = open(&mut app, b"Settings", 2, Fork::Data).unwrap();
    app.runtime.memory.write(BUFFER, b"X").unwrap();
    io(&mut app, alias, 1, 0, 0);
    assert_eq!(direct(&mut app, Op::Write), 0);
    io(&mut app, alias, 1, 1, 0);
    assert_eq!(direct(&mut app, Op::Read), 0); // fsWrPerm is read/write, per IM.
}
#[test]
fn file_io_faults_preserve_bytes_marks_and_open_identity_and_bound_storage() {
    let mut app = app();
    fresh(&mut app, b"Settings", 2, 0);
    let before = app.runtime.memory.read(PB, 80).unwrap().to_vec();
    app.runtime
        .memory
        .map(GuestPtr(0x52000000), before.clone(), Permissions::RX)
        .unwrap();
    assert!(app
        .runtime
        .process
        .files
        .io(
            &mut app.runtime.memory,
            GuestPtr(0x52000000),
            Op::Create { hierarchical: true }
        )
        .is_err());
    assert_eq!(direct(&mut app, Op::Create { hierarchical: true }), 0);
    assert_eq!(direct(&mut app, Op::Create { hierarchical: true }), -48);
    let reference = open(&mut app, b"Settings", 3, Fork::Data).unwrap();
    app.runtime.memory.write(BUFFER, b"abc\ndef").unwrap();
    io(&mut app, reference, 7, 0, 0);
    assert_eq!(direct(&mut app, Op::Write), 0);
    io(&mut app, reference, 3, 1, 0);
    write32(&mut app, 32, 0x52000000);
    let pb = app.runtime.memory.read(PB, 50).unwrap().to_vec();
    assert!(app
        .runtime
        .process
        .files
        .io(&mut app.runtime.memory, PB, Op::Read)
        .is_err());
    assert_eq!(app.runtime.memory.read(PB, 50).unwrap(), pb);
    io(&mut app, reference, 0, 0, 0);
    assert_eq!(direct(&mut app, Op::GetPosition), 0);
    assert_eq!(app.runtime.memory.u32(GuestPtr(PB.0 + 46)).unwrap(), 7);
    io(&mut app, reference, 1, 1, 0);
    write32(&mut app, 32, 0x60000000);
    assert!(app
        .runtime
        .process
        .files
        .io(&mut app.runtime.memory, PB, Op::Write)
        .is_err());
    io(&mut app, reference, 100, 0x0a81, 0);
    assert_eq!(direct(&mut app, Op::Read), 0);
    assert_eq!(app.runtime.memory.u32(GuestPtr(PB.0 + 40)).unwrap(), 4);
    assert_eq!(app.runtime.memory.read(BUFFER, 4).unwrap(), b"abc\n");
    io(&mut app, reference, 1, 1, -1);
    assert_eq!(direct(&mut app, Op::Read), -40);
    io(&mut app, reference, -1, 0, 0);
    assert_eq!(direct(&mut app, Op::Write), -50);
    io(&mut app, reference, i32::MAX, 0, 0);
    assert_eq!(direct(&mut app, Op::Write), -34);
    io(&mut app, reference, 0, 1, 20);
    assert_eq!(direct(&mut app, Op::SetPosition), -39);
    assert_eq!(app.runtime.memory.u32(GuestPtr(PB.0 + 46)).unwrap(), 7);
    write32(&mut app, 28, 0x04000001);
    assert_eq!(direct(&mut app, Op::SetEof), -34);
    write32(&mut app, 28, 10);
    assert_eq!(direct(&mut app, Op::SetEof), 0);
    io(&mut app, reference, 10, 1, 0);
    assert_eq!(direct(&mut app, Op::Read), 0);
    assert_eq!(
        app.runtime.memory.read(BUFFER, 10).unwrap(),
        b"abc\ndef\0\0\0"
    );
}
#[test]
fn nonroot_file_queries_enumerate_and_share_ids_with_fcb_metadata() {
    let mut app = app();
    fresh(&mut app, b"Prefs", 2, 0);
    assert_eq!(
        app.runtime
            .process
            .files
            .create_directory(&mut app.runtime.memory, PB)
            .unwrap(),
        0
    );
    let parent = app.runtime.memory.u32(GuestPtr(PB.0 + 48)).unwrap();
    fresh(&mut app, b"Settings", parent, 0);
    assert_eq!(direct(&mut app, Op::Create { hierarchical: true }), 0);
    assert_eq!(
        direct(
            &mut app,
            Op::Open {
                kind: Fork::Data,
                hierarchical: true
            }
        ),
        0
    );
    let reference = app.runtime.memory.u16(GuestPtr(PB.0 + 24)).unwrap() as i16;
    fresh(&mut app, b"Settings", parent, 0);
    assert_eq!(
        app.runtime
            .process
            .files
            .catalog_info(&mut app.runtime.memory, PB)
            .unwrap(),
        0
    );
    let node = app.runtime.memory.u32(GuestPtr(PB.0 + 48)).unwrap();
    assert_eq!(
        app.runtime.memory.u32(GuestPtr(PB.0 + 100)).unwrap(),
        parent
    );
    fresh(&mut app, b"", parent, 0);
    write16(&mut app, 28, 1);
    assert_eq!(
        app.runtime
            .process
            .files
            .file_info(&mut app.runtime.memory, PB, true)
            .unwrap(),
        0
    );
    assert_eq!(app.runtime.memory.u32(GuestPtr(PB.0 + 48)).unwrap(), node);
    fresh(&mut app, b"", 0, 0);
    write16(&mut app, 24, reference);
    assert_eq!(
        app.runtime
            .process
            .files
            .fcb_info(&mut app.runtime.memory, PB)
            .unwrap(),
        0
    );
    assert_eq!(app.runtime.memory.u32(GuestPtr(PB.0 + 32)).unwrap(), node);
    assert_eq!(app.runtime.memory.u32(GuestPtr(PB.0 + 58)).unwrap(), parent);
    assert_eq!(
        app.runtime.memory.u16(GuestPtr(PB.0 + 36)).unwrap() & 0x2100,
        0x100
    );
    fresh(&mut app, b"Settings", 16, 0);
    assert_eq!(
        app.runtime
            .process
            .files
            .catalog_info(&mut app.runtime.memory, PB)
            .unwrap(),
        -120
    );
}

#[test]
fn access_path_limit_failed_open_and_async_traps_preserve_their_boundaries() {
    use systemless_future::abi::{Abi, Service};
    let mut app = app();
    let mut references = Vec::new();
    for _ in 0..255 {
        references.push(open(&mut app, b"Application", 1, Fork::Data).unwrap());
    }
    assert_eq!(open(&mut app, b"Application", 1, Fork::Data), Err(-42));
    io(&mut app, references[0], 0, 0, 0);
    assert_eq!(direct(&mut app, Op::Close), 0);
    let reference = open(&mut app, b"Application", 1, Fork::Data).unwrap();
    assert!(!references.contains(&reference));
    let before = app.runtime.memory.read(PB, 80).unwrap().to_vec();
    let mut cpu = m68k::CpuCore::default();
    cpu.dar[8] = PB.0;
    cpu.dar[15] = STACK_BASE + 0x8000;
    for opcode in [0xa400, 0xa600, 0xa402, 0xa403, 0xa608] {
        assert!(Abi::trap(opcode, &mut cpu, &app.runtime.memory).is_err());
        assert_eq!(cpu.dar[8], PB.0);
        assert_eq!(cpu.dar[15], STACK_BASE + 0x8000);
    }
    let mut cpu = ppc::PpcCpu::default();
    cpu.gpr[3] = PB.0;
    cpu.gpr[4] = 1;
    assert!(Abi::import(
        Service::FileIoWithAsync(Op::Read),
        &cpu,
        &app.runtime.memory
    )
    .is_err());
    cpu.gpr[4] = 0;
    assert!(matches!(
        Abi::import(
            Service::FileIoWithAsync(Op::Read),
            &cpu,
            &app.runtime.memory
        )
        .unwrap()
        .0,
        systemless_future::cpu::Call::FileIo { op: Op::Read, .. }
    ));
    assert_eq!(app.runtime.memory.read(PB, 80).unwrap(), before);
}
