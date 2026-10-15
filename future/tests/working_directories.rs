mod support;
use systemless_future::{
    cpu::{Architecture, Cpu},
    files::{Fork, Op, WorkingOp},
    load::{LoadedApplication, STACK_BASE},
    memory::{GuestPtr, Permissions},
    runtime::Stop,
};
const PB: GuestPtr = GuestPtr(0x50000000);
const NAME: GuestPtr = GuestPtr(0x50000100);
fn app() -> LoadedApplication {
    let f = support::pef_program(
        &[0x4e800020],
        &[
            b"PBOpenWDSync",
            b"PBCloseWDSync",
            b"PBGetWDInfoSync",
            b"PBSetVolSync",
            b"PBHSetVolSync",
            b"PBGetVolSync",
            b"PBHGetVolSync",
        ],
        0,
    );
    let mut a = LoadedApplication::pef(&f.bytes, None).unwrap();
    a.runtime
        .memory
        .map(PB, vec![0; 1024], Permissions::RW)
        .unwrap();
    a.runtime
        .memory
        .map(
            GuestPtr(0x51000000),
            vec![0xa2, 0x60, 0xa0, 0x15, 0xa2, 0x15, 0xa0, 0x14, 0xa2, 0x14],
            Permissions::RX,
        )
        .unwrap();
    a
}
fn word(a: &mut LoadedApplication, at: u32, n: i16) {
    a.runtime
        .memory
        .write(GuestPtr(PB.0 + at), &n.to_be_bytes())
        .unwrap();
}
fn long(a: &mut LoadedApplication, at: u32, n: u32) {
    a.runtime
        .memory
        .write(GuestPtr(PB.0 + at), &n.to_be_bytes())
        .unwrap();
}
fn get(a: &LoadedApplication, at: u32) -> u32 {
    a.runtime.memory.u32(GuestPtr(PB.0 + at)).unwrap()
}
fn reference(a: &LoadedApplication) -> i16 {
    a.runtime.memory.u16(GuestPtr(PB.0 + 22)).unwrap() as i16
}
fn fresh(a: &mut LoadedApplication, volume: i16, directory: u32, name: &[u8]) {
    a.runtime.memory.write(PB, &[0; 128]).unwrap();
    word(a, 22, volume);
    long(a, 48, directory);
    if !name.is_empty() {
        long(a, 18, NAME.0);
        let mut s = vec![name.len() as u8];
        s.extend(name);
        a.runtime.memory.write(NAME, &s).unwrap();
    }
}
fn call(a: &mut LoadedApplication, cpu: Architecture, op: WorkingOp) -> i16 {
    let stack = STACK_BASE + 0x8000;
    a.runtime.memory.write(GuestPtr(stack), &[0x5a; 8]).unwrap();
    if cpu == Architecture::M68k {
        let offset = match op {
            WorkingOp::Set {
                hierarchical: false,
            } => 2,
            WorkingOp::Set { hierarchical: true } => 4,
            WorkingOp::Current {
                hierarchical: false,
            } => 6,
            WorkingOp::Current { hierarchical: true } => 8,
            _ => 0,
        };
        a.runtime.cpu = Cpu::new(cpu, 0x51000000 + offset, stack);
        let Cpu::M68k(c) = &mut a.runtime.cpu else {
            panic!()
        };
        c.dar[0] = 0x12340000
            | match op {
                WorkingOp::Open => 1,
                WorkingOp::Close => 2,
                WorkingOp::Info => 7,
                WorkingOp::Current { .. } | WorkingOp::Set { .. } => 0,
            };
        c.dar[8] = PB.0;
        c.dar[1] = 0x34567890;
    } else {
        let index = match op {
            WorkingOp::Open => 0,
            WorkingOp::Close => 1,
            WorkingOp::Info => 2,
            WorkingOp::Set {
                hierarchical: false,
            } => 3,
            WorkingOp::Set { hierarchical: true } => 4,
            WorkingOp::Current {
                hierarchical: false,
            } => 5,
            WorkingOp::Current { hierarchical: true } => 6,
        };
        let v = a
            .runtime
            .memory
            .u32(GuestPtr(a.info.toc_or_a5 + 8 + index * 4))
            .unwrap();
        let pc = a.runtime.memory.u32(GuestPtr(v)).unwrap();
        a.runtime.cpu = Cpu::new(cpu, pc, stack);
        let Cpu::PowerPc(c) = &mut a.runtime.cpu else {
            panic!()
        };
        c.gpr[3] = PB.0;
        c.gpr[4] = 0x34567890;
    }
    assert_eq!(a.runtime.advance(1).unwrap(), Stop::Budget);
    assert_eq!(
        a.runtime.memory.read(GuestPtr(stack), 8).unwrap(),
        [0x5a; 8]
    );
    let result = match &a.runtime.cpu {
        Cpu::M68k(c) => {
            assert_eq!(c.dar[15], stack);
            assert_eq!(c.dar[1], 0x34567890);
            c.dar[0]
        }
        Cpu::PowerPc(c) => {
            assert_eq!(c.gpr[4], 0x34567890);
            c.gpr[3]
        }
    };
    let error = a.runtime.memory.u16(GuestPtr(PB.0 + 16)).unwrap() as i16;
    assert_eq!(result, error as i32 as u32);
    error
}
fn directory(a: &mut LoadedApplication, name: &[u8]) -> u32 {
    fresh(a, -1, 2, name);
    assert_eq!(
        a.runtime
            .process
            .files
            .create_directory(&mut a.runtime.memory, PB)
            .unwrap(),
        0
    );
    get(a, 48)
}
#[test]
fn working_directory_identity_routes_classic_and_hierarchical_files_and_survives_cross_cpu_queries()
{
    let mut a = app();
    let folder = directory(&mut a, b"Saves");
    fresh(&mut a, -1, folder, b"");
    long(&mut a, 28, 0x46555452);
    assert_eq!(call(&mut a, Architecture::M68k, WorkingOp::Open), 0);
    let wd = reference(&a);
    assert!(wd < -32000);
    fresh(&mut a, wd, 0, b"");
    long(&mut a, 28, 0x46555452);
    assert_eq!(call(&mut a, Architecture::PowerPc, WorkingOp::Open), 0);
    assert_eq!(reference(&a), wd);
    fresh(&mut a, wd, 0, b"Game");
    assert_eq!(
        a.runtime
            .process
            .files
            .io(
                &mut a.runtime.memory,
                PB,
                Op::Create {
                    hierarchical: false
                }
            )
            .unwrap(),
        0
    );
    fresh(&mut a, -1, 2, b"Game");
    assert_eq!(
        a.runtime
            .process
            .files
            .file_info(&mut a.runtime.memory, PB, false)
            .unwrap(),
        -43
    );
    fresh(&mut a, wd, 0, b"Game");
    assert_eq!(
        a.runtime
            .process
            .files
            .file_info(&mut a.runtime.memory, PB, false)
            .unwrap(),
        0
    );
    fresh(&mut a, wd, 0, b"Game");
    assert_eq!(
        a.runtime
            .process
            .files
            .io(
                &mut a.runtime.memory,
                PB,
                Op::Open {
                    kind: Fork::Data,
                    hierarchical: false
                }
            )
            .unwrap(),
        0
    );
    let file = a.runtime.memory.u16(GuestPtr(PB.0 + 24)).unwrap() as i16;
    fresh(&mut a, wd, 0, b"");
    word(&mut a, 28, 1);
    assert_eq!(
        a.runtime
            .process
            .files
            .fcb_info(&mut a.runtime.memory, PB)
            .unwrap(),
        0
    );
    assert_eq!(get(&a, 58), folder);
    fresh(&mut a, wd, 0, b"Nested");
    assert_eq!(
        a.runtime
            .process
            .files
            .create_directory(&mut a.runtime.memory, PB)
            .unwrap(),
        0
    );
    let nested = get(&a, 48);
    fresh(&mut a, wd, nested, b"");
    word(&mut a, 28, -1);
    assert_eq!(
        a.runtime
            .process
            .files
            .catalog_info(&mut a.runtime.memory, PB)
            .unwrap(),
        0
    );
    assert_eq!(get(&a, 100), folder);

    fresh(&mut a, -1, folder, b"Game");
    assert_eq!(
        a.runtime
            .process
            .files
            .file_info(&mut a.runtime.memory, PB, true)
            .unwrap(),
        0
    );
    fresh(&mut a, wd, 2, b"Game");
    assert_eq!(
        a.runtime
            .process
            .files
            .file_info(&mut a.runtime.memory, PB, true)
            .unwrap(),
        -43
    );
    fresh(&mut a, wd, 0, b"");
    word(&mut a, 28, 1);
    long(&mut a, 18, NAME.0);
    assert_eq!(
        a.runtime
            .process
            .files
            .catalog_info(&mut a.runtime.memory, PB)
            .unwrap(),
        0
    );
    assert_eq!(a.runtime.memory.read(NAME, 5).unwrap(), b"\x04Game");
    fresh(&mut a, wd, 0, b"");
    long(&mut a, 18, NAME.0);
    a.runtime.process.mem_error = -108;
    a.runtime.process.res_error = -192;
    a.runtime
        .memory
        .write(GuestPtr(0x220), &(-50i16).to_be_bytes())
        .unwrap();
    assert_eq!(call(&mut a, Architecture::PowerPc, WorkingOp::Info), 0);
    assert_eq!(reference(&a), wd);
    assert_eq!(get(&a, 48), folder);
    assert_eq!(get(&a, 28), 0x46555452);
    assert_eq!(
        a.runtime.memory.u16(GuestPtr(PB.0 + 32)).unwrap() as i16,
        -1
    );
    assert_eq!(
        a.runtime.memory.read(NAME, 19).unwrap(),
        b"\x12Application Volume"
    );
    assert_eq!(
        (a.runtime.process.mem_error, a.runtime.process.res_error),
        (-108, -192)
    );
    assert_eq!(a.runtime.memory.u16(GuestPtr(0x220)).unwrap() as i16, -50);
    fresh(&mut a, wd, 0, b"");
    assert_eq!(call(&mut a, Architecture::M68k, WorkingOp::Close), 0);
    fresh(&mut a, wd, 0, b"Game");
    assert_eq!(
        a.runtime
            .process
            .files
            .file_info(&mut a.runtime.memory, PB, false)
            .unwrap(),
        -51
    );
    fresh(&mut a, 0, 0, b"");
    word(&mut a, 24, file);
    assert_eq!(
        a.runtime
            .process
            .files
            .io(&mut a.runtime.memory, PB, Op::Close)
            .unwrap(),
        0
    );
}
#[test]
fn working_directory_paths_owners_enumeration_and_bounded_lifetimes_are_real_catalog_state() {
    let mut a = app();
    let folder = directory(&mut a, b"Saves");
    for cpu in [Architecture::M68k, Architecture::PowerPc] {
        fresh(&mut a, -1, 2, b":Saves:");
        long(&mut a, 28, 1);
        assert_eq!(call(&mut a, cpu, WorkingOp::Open), 0);
        let first = reference(&a);
        fresh(&mut a, -1, folder, b"");
        long(&mut a, 28, 2);
        assert_eq!(call(&mut a, cpu, WorkingOp::Open), 0);
        assert_ne!(reference(&a), first);
        fresh(&mut a, 0, 0, b"");
        word(&mut a, 26, 2);
        assert_eq!(call(&mut a, cpu, WorkingOp::Info), 0);
        assert_eq!(reference(&a), -1);
        assert_eq!(get(&a, 28), 2);
        assert_eq!(get(&a, 48), folder);
    }
    for owner in 3..=128 {
        assert!(a
            .runtime
            .process
            .files
            .open_working_directory(-1, folder, owner)
            .is_ok());
    }
    assert_eq!(
        a.runtime
            .process
            .files
            .open_working_directory(-1, folder, 129),
        Err(-121)
    );
    fresh(&mut a, -1, folder, b"");
    long(&mut a, 28, 129);
    assert_eq!(call(&mut a, Architecture::PowerPc, WorkingOp::Open), -121);
    a.runtime
        .process
        .files
        .close_working_directory(-32767)
        .unwrap();
    assert_eq!(
        a.runtime
            .process
            .files
            .open_working_directory(-1, folder, 129),
        Ok(-32767)
    );
    for v in [-1, 0, 1] {
        assert_eq!(a.runtime.process.files.close_working_directory(v), Ok(()));
    }
    assert_eq!(
        a.runtime.process.files.open_working_directory(-1, 9999, 0),
        Err(-120)
    );
    assert_eq!(
        a.runtime.process.files.open_working_directory(-123, 2, 0),
        Err(-35)
    );
}
#[test]
fn parameter_faults_publish_no_working_directory_or_partial_info_and_errors_preserve_fields() {
    let mut a = app();
    let folder = directory(&mut a, b"Saves");
    let read_only = GuestPtr(0x52000000);
    fresh(&mut a, -1, folder, b"");
    let bytes = a.runtime.memory.read(PB, 52).unwrap().to_vec();
    a.runtime
        .memory
        .map(read_only, bytes, Permissions::RX)
        .unwrap();
    assert!(a
        .runtime
        .process
        .files
        .working_directory(&mut a.runtime.memory, read_only, WorkingOp::Open)
        .is_err());
    assert_eq!(
        a.runtime
            .process
            .files
            .open_working_directory(-1, folder, 0),
        Ok(-32767)
    );
    fresh(&mut a, -32767, 0, b"");
    long(&mut a, 18, read_only.0);
    let before = a.runtime.memory.read(PB, 52).unwrap().to_vec();
    assert!(a
        .runtime
        .process
        .files
        .working_directory(&mut a.runtime.memory, PB, WorkingOp::Info)
        .is_err());
    assert_eq!(a.runtime.memory.read(PB, 52).unwrap(), before);
    fresh(&mut a, -32766, 0, b"");
    assert_eq!(call(&mut a, Architecture::M68k, WorkingOp::Info), -51);
    assert_eq!(reference(&a), -32766);
    fresh(&mut a, 0, 0, b"");
    word(&mut a, 26, -1);
    assert_eq!(call(&mut a, Architecture::PowerPc, WorkingOp::Info), -50);
    fresh(&mut a, 0, 0, b"");
    word(&mut a, 26, 2);
    assert_eq!(call(&mut a, Architecture::M68k, WorkingOp::Info), -51);
}

#[test]
fn default_selection_changes_real_relative_files_and_distinguishes_classic_and_hierarchical_queries(
) {
    let mut a = app();
    let saves = directory(&mut a, b"Saves");
    let other = directory(&mut a, b"Other");
    fresh(&mut a, -1, saves, b"");
    long(&mut a, 28, 0x46555452);
    assert_eq!(call(&mut a, Architecture::M68k, WorkingOp::Open), 0);
    let wd = reference(&a);
    fresh(&mut a, wd, other, b"");
    assert_eq!(
        call(
            &mut a,
            Architecture::PowerPc,
            WorkingOp::Set { hierarchical: true }
        ),
        0
    );
    assert_eq!(a.runtime.process.files.default_directory(), saves);
    fresh(&mut a, 0, 0, b"");
    assert_eq!(
        call(
            &mut a,
            Architecture::M68k,
            WorkingOp::Current { hierarchical: true }
        ),
        0
    );
    assert_eq!(get(&a, 48), saves);
    assert_eq!(reference(&a), wd);
    assert_eq!(get(&a, 28), 0x46555452);
    fresh(&mut a, 0, 0, b"");
    assert_eq!(
        call(
            &mut a,
            Architecture::PowerPc,
            WorkingOp::Current {
                hierarchical: false
            }
        ),
        0
    );
    assert_eq!(reference(&a), -1);
    fresh(&mut a, 0, 0, b"Game");
    assert_eq!(
        a.runtime
            .process
            .files
            .io(
                &mut a.runtime.memory,
                PB,
                Op::Create {
                    hierarchical: false
                }
            )
            .unwrap(),
        0
    );
    fresh(&mut a, -1, saves, b"Game");
    assert_eq!(
        a.runtime
            .process
            .files
            .file_info(&mut a.runtime.memory, PB, true)
            .unwrap(),
        0
    );
    fresh(&mut a, -1, 0, b"Game");
    assert_eq!(
        a.runtime
            .process
            .files
            .file_info(&mut a.runtime.memory, PB, false)
            .unwrap(),
        -43
    );
    fresh(&mut a, -1, 0, b"Game");
    assert_eq!(
        a.runtime
            .process
            .files
            .file_info(&mut a.runtime.memory, PB, true)
            .unwrap(),
        0
    );
    fresh(&mut a, -1, 0, b"");
    word(&mut a, 28, -1);
    assert_eq!(
        a.runtime
            .process
            .files
            .catalog_info(&mut a.runtime.memory, PB)
            .unwrap(),
        0
    );
    assert_eq!(get(&a, 48), 2);
    fresh(&mut a, 0, 0, b"");
    word(&mut a, 28, -1);
    assert_eq!(
        a.runtime
            .process
            .files
            .catalog_info(&mut a.runtime.memory, PB)
            .unwrap(),
        0
    );
    assert_eq!(get(&a, 48), saves);
    fresh(&mut a, -1, 0, b"");
    word(&mut a, 28, 1);
    assert_eq!(
        a.runtime
            .process
            .files
            .catalog_info(&mut a.runtime.memory, PB)
            .unwrap(),
        0
    );
    assert_eq!(get(&a, 100), 2);
    fresh(&mut a, 0, 0, b"");
    word(&mut a, 28, 1);
    assert_eq!(
        a.runtime
            .process
            .files
            .catalog_info(&mut a.runtime.memory, PB)
            .unwrap(),
        0
    );
    assert_eq!(get(&a, 100), saves);
    fresh(&mut a, -1, other, b"");
    assert_eq!(
        call(
            &mut a,
            Architecture::M68k,
            WorkingOp::Set { hierarchical: true }
        ),
        0
    );
    fresh(&mut a, 0, 0, b"Game");
    assert_eq!(
        a.runtime
            .process
            .files
            .file_info(&mut a.runtime.memory, PB, true)
            .unwrap(),
        -43
    );
    fresh(&mut a, wd, 0, b"");
    assert_eq!(
        call(
            &mut a,
            Architecture::M68k,
            WorkingOp::Set {
                hierarchical: false
            }
        ),
        0
    );
    fresh(&mut a, 0, 0, b"");
    long(&mut a, 18, NAME.0);
    assert_eq!(
        call(
            &mut a,
            Architecture::PowerPc,
            WorkingOp::Current {
                hierarchical: false
            }
        ),
        0
    );
    assert_eq!(reference(&a), wd);
    assert_eq!(a.runtime.memory.read(NAME, 6).unwrap(), b"\x05Saves");
}
#[test]
fn full_paths_override_bad_references_and_failed_selection_preserves_default_and_parameter_fields()
{
    let mut a = app();
    let folder = directory(&mut a, b"Folder");
    fresh(&mut a, -32001, 0xdeadbeef, b"Application Volume:Folder:");
    assert_eq!(
        call(
            &mut a,
            Architecture::M68k,
            WorkingOp::Set { hierarchical: true }
        ),
        0
    );
    assert_eq!(a.runtime.process.files.default_directory(), folder);
    for (volume, dir, name, error) in [
        (-32001, 0, &b""[..], -35),
        (-1, 0, &b":Missing:"[..], -43),
        (-1, 16, &b""[..], -43),
        (-1, 0, &b"Other Volume:"[..], -35),
    ] {
        fresh(&mut a, volume, dir, name);
        let before = a.runtime.memory.read(PB, 52).unwrap().to_vec();
        assert_eq!(
            call(
                &mut a,
                Architecture::PowerPc,
                WorkingOp::Set { hierarchical: true }
            ),
            error
        );
        let after = a.runtime.memory.read(PB, 52).unwrap();
        assert_eq!(&after[..16], &before[..16]);
        assert_eq!(&after[18..], &before[18..]);
        assert_eq!(a.runtime.process.files.default_directory(), folder);
    }
    fresh(&mut a, 0, 2, b"");
    long(&mut a, 18, 0x60000000);
    assert!(a
        .runtime
        .process
        .files
        .working_directory(
            &mut a.runtime.memory,
            PB,
            WorkingOp::Set { hierarchical: true }
        )
        .is_err());
    assert_eq!(a.runtime.process.files.default_directory(), folder);
    let mut readonly = vec![0; 52];
    support::w16(&mut readonly, 22, 0xffff);
    support::w32(&mut readonly, 48, 2);
    a.runtime
        .memory
        .map(GuestPtr(0x60000000), readonly.clone(), Permissions::RX)
        .unwrap();
    assert!(a
        .runtime
        .process
        .files
        .working_directory(
            &mut a.runtime.memory,
            GuestPtr(0x60000000),
            WorkingOp::Set { hierarchical: true }
        )
        .is_err());
    assert_eq!(a.runtime.process.files.default_directory(), folder);
    assert_eq!(
        a.runtime.memory.read(GuestPtr(0x60000000), 52).unwrap(),
        readonly
    );
    fresh(&mut a, -32001, 0, b"application volume:");
    assert_eq!(
        call(
            &mut a,
            Architecture::PowerPc,
            WorkingOp::Set {
                hierarchical: false
            }
        ),
        0
    );
    assert_eq!(a.runtime.process.files.default_directory(), 2);
}
#[test]
fn selected_directory_survives_closed_wd_is_protected_and_is_not_serialized_as_volume_state() {
    let mut a = app();
    let folder = directory(&mut a, b"Folder");
    fresh(&mut a, -1, folder, b"");
    assert_eq!(call(&mut a, Architecture::PowerPc, WorkingOp::Open), 0);
    let wd = reference(&a);
    fresh(&mut a, wd, 0, b"");
    assert_eq!(
        call(
            &mut a,
            Architecture::M68k,
            WorkingOp::Set {
                hierarchical: false
            }
        ),
        0
    );
    fresh(&mut a, wd, 0, b"");
    assert_eq!(call(&mut a, Architecture::PowerPc, WorkingOp::Close), 0);
    assert_eq!(a.runtime.process.files.default_directory(), folder);
    fresh(&mut a, -1, 2, b"Folder");
    assert_eq!(
        a.runtime
            .process
            .files
            .io(&mut a.runtime.memory, PB, Op::Delete { hierarchical: true })
            .unwrap(),
        -47
    );
    let image = a.runtime.process.files.export_volume().unwrap();
    let mut b = app();
    b.runtime.process.files.restore_volume(&image).unwrap();
    assert_eq!(b.runtime.process.files.default_directory(), 2);
    assert_eq!(
        b.runtime.process.files.directory_view(folder).unwrap().name,
        b"Folder"
    );
}
