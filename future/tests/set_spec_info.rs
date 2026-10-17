mod support;
use systemless_future::{
    abi::Abi,
    cpu::{Architecture, Cpu},
    files::{CreateSpecResource, GetSpecInfo},
    load::{LoadedApplication, STACK_BASE},
    memory::{GuestPtr, Memory, Permissions},
    process::resources::Op,
    runtime::Stop,
};
const SPEC: GuestPtr = GuestPtr(STACK_BASE + 0x6000);
const SOURCE: GuestPtr = GuestPtr(0x51000000);
const FINDER: [u8; 16] = *b"PILTTEST\x80\x00abcdef";
fn spec(volume: i16, parent: u32, name: &[u8]) -> [u8; 70] {
    let mut b = [0; 70];
    b[..2].copy_from_slice(&volume.to_be_bytes());
    b[2..6].copy_from_slice(&parent.to_be_bytes());
    b[6] = name.len() as u8;
    b[7..7 + name.len()].copy_from_slice(name);
    b
}
fn setup() -> LoadedApplication {
    let f = support::pef_program(&[0x4e800020], &[b"FSpSetFInfo"], 16);
    let mut a = LoadedApplication::pef(&f.bytes, None).unwrap();
    a.runtime
        .memory
        .map(GuestPtr(0x50000000), vec![0xaa, 0x52], Permissions::RX)
        .unwrap();
    a.runtime
        .memory
        .map(SOURCE, FINDER.to_vec(), Permissions::RX)
        .unwrap();
    a.runtime.memory.write(SPEC, &spec(-1, 2, b"Test")).unwrap();
    assert_eq!(
        a.runtime
            .process
            .files
            .create_spec_resource(
                &a.runtime.memory,
                CreateSpecResource {
                    spec: SPEC,
                    creator: *b"TEST",
                    kind: *b"pref",
                    script: 0
                }
            )
            .unwrap(),
        0
    );
    a
}
fn call(a: &mut LoadedApplication, native: bool) -> i16 {
    let stack = STACK_BASE + 0x7000;
    let vector = a
        .runtime
        .memory
        .u32(GuestPtr(a.info.toc_or_a5 + 8))
        .unwrap();
    let gate = a.runtime.memory.u32(GuestPtr(vector)).unwrap();
    let mut frame = SOURCE.0.to_be_bytes().to_vec();
    frame.extend(SPEC.0.to_be_bytes());
    frame.extend([0xa5; 6]);
    a.runtime.memory.write(GuestPtr(stack), &frame).unwrap();
    a.runtime.cpu = Cpu::new(
        if native {
            Architecture::M68k
        } else {
            Architecture::PowerPc
        },
        if native { 0x50000000 } else { gate },
        stack,
    );
    match &mut a.runtime.cpu {
        Cpu::M68k(c) => {
            for (i, r) in c.dar[..15].iter_mut().enumerate() {
                *r = 0xaabb0000 + i as u32;
            }
            c.dar[0] = 0x12340008;
        }
        Cpu::PowerPc(c) => {
            for (i, r) in c.gpr[2..].iter_mut().enumerate() {
                *r = 0xaabb0000 + i as u32;
            }
            c.gpr[3] = SPEC.0;
            c.gpr[4] = SOURCE.0;
        }
    }
    let saved = match &a.runtime.cpu {
        Cpu::M68k(c) => c.dar.to_vec(),
        Cpu::PowerPc(c) => c.gpr.to_vec(),
    };
    assert_eq!(a.runtime.advance(1).unwrap(), Stop::Budget);
    match &a.runtime.cpu {
        Cpu::M68k(c) => {
            assert_eq!(&c.dar[..15], &saved[..15]);
            assert_eq!(c.dar[15], stack + 8);
            assert_eq!(
                a.runtime.memory.read(GuestPtr(stack + 10), 4).unwrap(),
                [0xa5; 4]
            );
            a.runtime.memory.u16(GuestPtr(stack + 8)).unwrap() as i16
        }
        Cpu::PowerPc(c) => {
            assert_eq!(&c.gpr[4..], &saved[4..]);
            assert_eq!(c.gpr[1], stack);
            c.gpr[3] as i32 as i16
        }
    }
}
#[test]
fn both_cpus_update_exact_finder_bytes_on_open_files_and_reload_the_saved_catalog() {
    for native in [true, false] {
        let mut a = setup();
        let reference = a
            .runtime
            .process
            .resource_file(
                &mut a.runtime.memory,
                Op::OpenSpec {
                    spec: SPEC,
                    permission: 3,
                },
            )
            .unwrap();
        assert!(reference > 0);
        let before = a.runtime.process.files.export_volume().unwrap();
        a.runtime.process.res_error = -192;
        a.runtime.process.mem_error = -108;
        assert_eq!(call(&mut a, native), 0);
        let after = a.runtime.process.files.export_volume().unwrap();
        assert_eq!(before.len(), after.len());
        let original = [b"prefTEST".as_slice(), &[0; 8]].concat();
        let at = before.windows(16).position(|b| b == original).unwrap();
        assert_eq!(&after[at..at + 16], FINDER);
        assert_eq!(&before[..at], &after[..at]);
        assert_eq!(
            &before[at + 16..before.len() - 4],
            &after[at + 16..after.len() - 4]
        );
        assert_eq!(a.runtime.process.res_error, -192);
        assert_eq!(a.runtime.process.mem_error, -108);
        assert_eq!(a.runtime.memory.read(SOURCE, 16).unwrap(), FINDER);
        assert_eq!(call(&mut a, !native), 0);
        assert_eq!(a.runtime.process.files.export_volume().unwrap(), after);
        let fixture = support::pef_program(&[0x4e800020], &[], 16);
        let mut fresh = LoadedApplication::pef(&fixture.bytes, None).unwrap();
        fresh
            .runtime
            .memory
            .write(SPEC, &spec(-1, 2, b"Test"))
            .unwrap();
        fresh.runtime.process.files.restore_volume(&after).unwrap();
        let dest = GuestPtr(STACK_BASE + 0x6100);
        assert_eq!(
            fresh
                .runtime
                .process
                .files
                .get_spec_info(
                    &mut fresh.runtime.memory,
                    GetSpecInfo {
                        spec: SPEC,
                        destination: dest
                    }
                )
                .unwrap(),
            0
        );
        assert_eq!(fresh.runtime.memory.read(dest, 16).unwrap(), FINDER);
    }
}
#[test]
fn both_cpus_report_catalog_errors_without_changing_any_file_or_input() {
    for native in [true, false] {
        let mut a = setup();
        let before = a.runtime.process.files.export_volume().unwrap();
        for (v, p, n, error) in [
            (-1, 2, b"Gone".as_slice(), -43),
            (7, 2, b"Test".as_slice(), -35),
            (-1, 999, b"Test".as_slice(), -120),
            (-1, 2, b"Bad:".as_slice(), -37),
            (-1, 2, b"Application".as_slice(), -45),
        ] {
            let input = spec(v, p, n);
            a.runtime.memory.write(SPEC, &input).unwrap();
            assert_eq!(call(&mut a, native), error);
            assert_eq!(a.runtime.memory.read(SPEC, 70).unwrap(), input);
            assert_eq!(a.runtime.process.files.export_volume().unwrap(), before);
        }
        a.runtime
            .memory
            .write(SPEC, &spec(-1, 1, b"Application Volume"))
            .unwrap();
        assert!(a
            .runtime
            .process
            .files
            .set_spec_info(
                &a.runtime.memory,
                systemless_future::files::SetSpecInfo {
                    spec: SPEC,
                    source: SOURCE
                }
            )
            .is_err());
        assert_eq!(a.runtime.process.files.export_volume().unwrap(), before);
    }
}
#[test]
fn native_set_info_preflights_nil_short_inputs_and_readonly_result_before_pop() {
    for case in 0..6 {
        let mut m = Memory::default();
        let source = if case == 0 { 0u32 } else { 0x3000 };
        let target = if case == 1 { 0u32 } else { 0x2000 };
        let mut f = source.to_be_bytes().to_vec();
        f.extend(target.to_be_bytes());
        f.extend([0xa5; 2]);
        if case == 5 {
            f.truncate(9);
        }
        m.map(
            GuestPtr(0x1000),
            f.clone(),
            if case == 4 {
                Permissions::RX
            } else {
                Permissions::RW
            },
        )
        .unwrap();
        m.map(
            GuestPtr(0x2000),
            vec![0; if case == 2 { 69 } else { 70 }],
            Permissions::RX,
        )
        .unwrap();
        m.map(
            GuestPtr(0x3000),
            vec![0; if case == 3 { 15 } else { 16 }],
            Permissions::RX,
        )
        .unwrap();
        let mut cpu = Cpu::new(Architecture::M68k, 0, 0x1000);
        let Cpu::M68k(c) = &mut cpu else {
            unreachable!()
        };
        c.dar[0] = 8;
        let saved = c.dar;
        assert!(Abi::trap(0xaa52, c, &m).is_err(), "case{case}");
        assert_eq!(c.dar, saved);
        assert_eq!(m.read(GuestPtr(0x1000), f.len()).unwrap(), f);
    }
}
