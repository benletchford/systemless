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
        &[b"PBOpenWDSync", b"PBCloseWDSync", b"PBGetWDInfoSync"],
        0,
    );
    let mut a = LoadedApplication::pef(&f.bytes, None).unwrap();
    a.runtime
        .memory
        .map(PB, vec![0; 1024], Permissions::RW)
        .unwrap();
    a.runtime
        .memory
        .map(GuestPtr(0x51000000), vec![0xa2, 0x60], Permissions::RX)
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
        a.runtime.cpu = Cpu::new(cpu, 0x51000000, stack);
        let Cpu::M68k(c) = &mut a.runtime.cpu else {
            panic!()
        };
        c.dar[0] = 0x12340000
            | match op {
                WorkingOp::Open => 1,
                WorkingOp::Close => 2,
                WorkingOp::Info => 7,
            };
        c.dar[8] = PB.0;
        c.dar[1] = 0x34567890;
    } else {
        let index = match op {
            WorkingOp::Open => 0,
            WorkingOp::Close => 1,
            WorkingOp::Info => 2,
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
