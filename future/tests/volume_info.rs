mod support;
use systemless_future::{
    cpu::{Architecture, Cpu},
    files::WorkingOp,
    load::{LoadedApplication, STACK_BASE},
    memory::{GuestPtr, Permissions},
    runtime::Stop,
};
const PB: GuestPtr = GuestPtr(0x50000000);
const NAME: GuestPtr = GuestPtr(0x50000200);
const CODE: u32 = 0x51000000;
const STACK: u32 = STACK_BASE + 0x8000;
fn app() -> LoadedApplication {
    let f = support::pef_program(&[0x4e800020], &[b"PBGetVInfoSync", b"PBHGetVInfoSync"], 0);
    let mut a = LoadedApplication::pef(&f.bytes, None).unwrap();
    a.runtime
        .memory
        .map(PB, vec![0xa5; 1024], Permissions::RW)
        .unwrap();
    a.runtime
        .memory
        .map(GuestPtr(CODE), vec![0xa0, 7, 0xa2, 7], Permissions::RX)
        .unwrap();
    a
}
fn word(a: &mut LoadedApplication, offset: u32, n: i16) {
    a.runtime
        .memory
        .write(GuestPtr(PB.0 + offset), &n.to_be_bytes())
        .unwrap();
}
fn long(a: &mut LoadedApplication, offset: u32, n: u32) {
    a.runtime
        .memory
        .write(GuestPtr(PB.0 + offset), &n.to_be_bytes())
        .unwrap();
}
fn fresh(a: &mut LoadedApplication, index: i16, reference: i16, name: Option<&[u8]>) {
    a.runtime.memory.write(PB, &[0xa5; 128]).unwrap();
    word(a, 28, index);
    word(a, 22, reference);
    long(a, 18, if name.is_some() { NAME.0 } else { 0 });
    if let Some(name) = name {
        let mut out = vec![name.len() as u8];
        out.extend(name);
        a.runtime.memory.write(NAME, &out).unwrap();
    }
}
fn call(a: &mut LoadedApplication, arch: Architecture, hfs: bool) -> i16 {
    let pc = if arch == Architecture::M68k {
        CODE + u32::from(hfs) * 2
    } else {
        let tv = a
            .runtime
            .memory
            .u32(GuestPtr(a.info.toc_or_a5 + 8 + u32::from(hfs) * 4))
            .unwrap();
        a.runtime.memory.u32(GuestPtr(tv)).unwrap()
    };
    a.runtime.cpu = Cpu::new(arch, pc, STACK);
    match &mut a.runtime.cpu {
        Cpu::M68k(c) => {
            c.dar[8] = PB.0;
            c.dar[1] = 0x12345678;
        }
        Cpu::PowerPc(c) => {
            c.gpr[3] = PB.0;
            c.gpr[4] = 0x12345678;
        }
    }
    a.runtime.process.mem_error = -99;
    a.runtime.process.res_error = -98;
    a.runtime.memory.write(GuestPtr(STACK), &[0x5a; 8]).unwrap();
    assert_eq!(a.runtime.advance(1).unwrap(), Stop::Budget);
    let error = a.runtime.memory.u16(GuestPtr(PB.0 + 16)).unwrap() as i16;
    match &a.runtime.cpu {
        Cpu::M68k(c) => {
            assert_eq!(c.dar[0], error as i32 as u32);
            assert_eq!(c.dar[1], 0x12345678);
            assert_eq!(c.dar[8], PB.0);
            assert_eq!(c.dar[15], STACK);
        }
        Cpu::PowerPc(c) => {
            assert_eq!(c.gpr[3], error as i32 as u32);
            assert_eq!(c.gpr[4], 0x12345678);
        }
    }
    assert_eq!(
        a.runtime.memory.read(GuestPtr(STACK), 8).unwrap(),
        [0x5a; 8]
    );
    assert_eq!(
        (a.runtime.process.mem_error, a.runtime.process.res_error),
        (-99, -98)
    );
    error
}
fn u16_at(a: &LoadedApplication, offset: u32) -> u16 {
    a.runtime.memory.u16(GuestPtr(PB.0 + offset)).unwrap()
}
fn u32_at(a: &LoadedApplication, offset: u32) -> u32 {
    a.runtime.memory.u32(GuestPtr(PB.0 + offset)).unwrap()
}
#[test]
fn both_cpus_return_packed_classic_and_hfs_records_without_touching_reserved_fields() {
    for arch in [Architecture::M68k, Architecture::PowerPc] {
        let mut a = app();
        for hfs in [false, true] {
            fresh(&mut a, 0, -1, Some(b"ignored input"));
            let before = a.runtime.memory.read(PB, 128).unwrap().to_vec();
            assert_eq!(call(&mut a, arch, hfs), 0);
            let b = a.runtime.memory.read(PB, 128).unwrap();
            for range in [0..12, 18..22, 24..30] {
                assert_eq!(&b[range.clone()], &before[range]);
            }
            assert_eq!(&b[12..18], &[0; 6]);
            let end = if hfs { 122 } else { 64 };
            assert_eq!(&b[end..], &before[end..]);
            assert_eq!(u16_at(&a, 22) as i16, -1);
            assert_eq!(u16_at(&a, 46), 32768);
            assert_eq!(u16_at(&a, 62), 32768);
            assert_eq!(u32_at(&a, 48), 2048);
            assert_eq!(u32_at(&a, 52), 2048);
            assert_eq!(u16_at(&a, 40), 1);
            assert_eq!(u32_at(&a, 58), 17);
            if hfs {
                assert_eq!(u16_at(&a, 64), 0x4244);
                assert_eq!(u16_at(&a, 66), 1);
                assert_eq!(u16_at(&a, 68) as i16, -1);
                assert_eq!(u32_at(&a, 82), 1);
                assert_eq!(u32_at(&a, 86), 1);
            }
            assert_eq!(
                a.runtime.memory.read(NAME, 19).unwrap(),
                b"\x12Application Volume"
            );
        }
    }
}
#[test]
fn volume_index_and_name_selection_preserve_failed_outputs_on_both_cpus() {
    for arch in [Architecture::M68k, Architecture::PowerPc] {
        let mut a = app();
        for (index, reference, name, expected) in [
            (1, 99, b"ignored".as_slice(), 0),
            (2, -1, b"unchanged", -35),
            (0, 99, b"unchanged", -35),
            (-1, 99, b"application volume:folder", 0),
            (-1, -1, b"Missing:", -35),
            (-1, -1, b"", 0),
        ] {
            fresh(&mut a, index, reference, Some(name));
            let before = a.runtime.memory.read(PB, 128).unwrap().to_vec();
            let old_name = a.runtime.memory.read(NAME, 32).unwrap().to_vec();
            assert_eq!(call(&mut a, arch, true), expected);
            if expected != 0 {
                let after = a.runtime.memory.read(PB, 128).unwrap();
                assert_eq!(&after[..12], &before[..12]);
                assert_eq!(&after[18..], &before[18..]);
                assert_eq!(a.runtime.memory.read(NAME, 32).unwrap(), old_name);
            }
        }
        fresh(&mut a, 0, 99, None);
        long(&mut a, 18, 0xdead0000);
        assert_eq!(call(&mut a, arch, true), -35);
        a.runtime.process.files = systemless_future::files::Files::default();
        fresh(&mut a, -1, 0, Some(b"Application Volume:"));
        assert_eq!(call(&mut a, arch, true), -35);
        fresh(&mut a, -1, 0, None);
        assert_eq!(call(&mut a, arch, true), -50);
        for (index, reference, error) in [(0, 0, -50), (1, 0, -35), (0, 99, -35)] {
            fresh(&mut a, index, reference, None);
            assert_eq!(call(&mut a, arch, true), error);
        }
    }
}
#[test]
fn working_directory_and_default_directory_valence_are_real_catalog_state() {
    let mut a = app();
    fresh(&mut a, 0, -1, Some(b"Folder"));
    long(&mut a, 48, 2);
    assert_eq!(
        a.runtime
            .process
            .files
            .create_directory(&mut a.runtime.memory, PB)
            .unwrap(),
        0
    );
    let folder = u32_at(&a, 48);
    let wd = a
        .runtime
        .process
        .files
        .open_working_directory(-1, folder, 0)
        .unwrap();
    fresh(&mut a, 0, wd, Some(b"Child"));
    long(&mut a, 48, 0);
    assert_eq!(
        a.runtime
            .process
            .files
            .create_directory(&mut a.runtime.memory, PB)
            .unwrap(),
        0
    );
    for arch in [Architecture::M68k, Architecture::PowerPc] {
        fresh(&mut a, 0, wd, None);
        assert_eq!(call(&mut a, arch, true), 0);
        assert_eq!(u16_at(&a, 40), 1);
        fresh(&mut a, 1, wd, None);
        assert_eq!(call(&mut a, arch, true), 0);
        assert_eq!(u16_at(&a, 40), 2);
    }
    fresh(&mut a, 0, wd, None);
    long(&mut a, 48, 0);
    assert_eq!(
        a.runtime
            .process
            .files
            .working_directory(
                &mut a.runtime.memory,
                PB,
                WorkingOp::Set { hierarchical: true }
            )
            .unwrap(),
        0
    );
    fresh(&mut a, 0, 0, None);
    assert_eq!(call(&mut a, Architecture::PowerPc, true), 0);
    assert_eq!(u16_at(&a, 40), 1);
}
#[test]
fn readonly_name_and_truncated_hfs_output_fail_before_any_publication() {
    let mut a = app();
    fresh(&mut a, 0, -1, None);
    let ro = GuestPtr(0x52000000);
    a.runtime
        .memory
        .map(ro, vec![0xa5; 32], Permissions::RX)
        .unwrap();
    long(&mut a, 18, ro.0);
    let before = a.runtime.memory.read(PB, 128).unwrap().to_vec();
    assert!(a
        .runtime
        .process
        .files
        .volume_info(&mut a.runtime.memory, PB, true)
        .is_err());
    assert_eq!(a.runtime.memory.read(PB, 128).unwrap(), before);
    assert_eq!(a.runtime.memory.read(ro, 32).unwrap(), [0xa5; 32]);
    let short = GuestPtr(0x53000000);
    let mut bytes = vec![0xa5; 64];
    bytes[18..22].fill(0);
    bytes[22..24].copy_from_slice(&(-1i16).to_be_bytes());
    bytes[28..30].fill(0);
    a.runtime
        .memory
        .map(short, bytes.clone(), Permissions::RW)
        .unwrap();
    assert!(a
        .runtime
        .process
        .files
        .volume_info(&mut a.runtime.memory, short, true)
        .is_err());
    assert_eq!(a.runtime.memory.read(short, 64).unwrap(), bytes);
    assert_eq!(
        a.runtime
            .process
            .files
            .volume_info(&mut a.runtime.memory, short, false)
            .unwrap(),
        0
    );
}
