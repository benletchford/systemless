mod support;
use systemless_future::{
    cpu::{Architecture, Cpu},
    files::{Files, WorkingOp},
    load::{LoadedApplication, STACK_BASE},
    memory::{GuestPtr, Memory, Permissions},
    runtime::Stop,
};
const PB: GuestPtr = GuestPtr(0x50000000);
const NAME: GuestPtr = GuestPtr(0x50000100);
fn application() -> LoadedApplication {
    let f = support::pef_program(&[0x4e800020], &[b"PBGetVolSync", b"PBHGetVolSync"], 32);
    let mut a = LoadedApplication::pef(&f.bytes, None).unwrap();
    a.runtime
        .memory
        .map(PB, vec![0xaa; 512], Permissions::RW)
        .unwrap();
    a.runtime
        .memory
        .map(
            GuestPtr(0x51000000),
            vec![0xa0, 0x14, 0xa2, 0x14],
            Permissions::RX,
        )
        .unwrap();
    a
}
fn invoke(a: &mut LoadedApplication, arch: Architecture, hierarchical: bool) -> i16 {
    let stack = STACK_BASE + 0x8000;
    match arch {
        Architecture::M68k => {
            a.runtime.cpu = Cpu::new(arch, 0x51000000 + if hierarchical { 2 } else { 0 }, stack);
            let Cpu::M68k(c) = &mut a.runtime.cpu else {
                panic!()
            };
            c.dar[8] = PB.0;
            c.dar[1] = 0x12345678;
        }
        Architecture::PowerPc => {
            let tv = a
                .runtime
                .memory
                .u32(GuestPtr(a.info.toc_or_a5 + 8 + u32::from(hierarchical) * 4))
                .unwrap();
            let pc = a.runtime.memory.u32(GuestPtr(tv)).unwrap();
            a.runtime.cpu = Cpu::new(arch, pc, stack);
            let Cpu::PowerPc(c) = &mut a.runtime.cpu else {
                panic!()
            };
            c.gpr[3] = PB.0;
        }
    }
    assert_eq!(a.runtime.advance(1).unwrap(), Stop::Budget);
    match &a.runtime.cpu {
        Cpu::M68k(c) => {
            assert_eq!(c.dar[8], PB.0);
            assert_eq!(c.dar[15], stack);
            assert_eq!(c.dar[1], 0x12345678);
            c.dar[0] as i16
        }
        Cpu::PowerPc(c) => c.gpr[3] as i16,
    }
}
#[test]
fn both_cpus_query_one_actual_volume_without_allocating_working_directories() {
    let mut a = application();
    // An unrelated working directory reference must not become the default.
    a.runtime
        .process
        .files
        .open_working_directory(-1, 2, 42)
        .unwrap();
    for arch in [Architecture::M68k, Architecture::PowerPc] {
        for hierarchical in [false, true] {
            for destination in [0, NAME.0] {
                a.runtime.memory.write(PB, &[0xaa; 52]).unwrap();
                a.runtime
                    .memory
                    .write(GuestPtr(PB.0 + 18), &destination.to_be_bytes())
                    .unwrap();
                let before = a.runtime.memory.read(PB, 52).unwrap().to_vec();
                assert_eq!(invoke(&mut a, arch, hierarchical), 0);
                let mut expected = before;
                expected[16..18].copy_from_slice(&0i16.to_be_bytes());
                expected[22..24].copy_from_slice(&(-1i16).to_be_bytes());
                if hierarchical {
                    expected[28..32].copy_from_slice(&0u32.to_be_bytes());
                    expected[32..34].copy_from_slice(&(-1i16).to_be_bytes());
                    expected[48..52].copy_from_slice(&2u32.to_be_bytes());
                }
                assert_eq!(a.runtime.memory.read(PB, 52).unwrap(), expected);
                if destination != 0 {
                    assert_eq!(
                        a.runtime.memory.read(NAME, 19).unwrap(),
                        b"\x12Application Volume"
                    );
                    assert_eq!(
                        a.runtime.memory.read(GuestPtr(NAME.0 + 19), 1).unwrap(),
                        &[0xaa]
                    );
                }
            }
        }
    }
}
#[test]
fn absent_volume_reports_no_such_volume_and_preserves_outputs() {
    let mut m = Memory::default();
    m.map(PB, vec![0xaa; 52], Permissions::RW).unwrap();
    let mut files = Files::default();
    let before = m.read(PB, 52).unwrap().to_vec();
    assert_eq!(
        files
            .working_directory(&mut m, PB, WorkingOp::Current { hierarchical: true })
            .unwrap(),
        -35
    );
    let mut expected = before;
    expected[16..18].copy_from_slice(&(-35i16).to_be_bytes());
    assert_eq!(m.read(PB, 52).unwrap(), expected);
}
#[test]
fn bad_name_or_parameter_buffers_fault_without_partial_writes() {
    let mut a = application();
    a.runtime
        .memory
        .map(GuestPtr(0x52000000), vec![0xbb; 32], Permissions::RX)
        .unwrap();
    a.runtime
        .memory
        .write(GuestPtr(PB.0 + 18), &0x52000000u32.to_be_bytes())
        .unwrap();
    let before = a.runtime.memory.read(PB, 52).unwrap().to_vec();
    assert!(a
        .runtime
        .process
        .files
        .working_directory(
            &mut a.runtime.memory,
            PB,
            WorkingOp::Current { hierarchical: true }
        )
        .is_err());
    assert_eq!(a.runtime.memory.read(PB, 52).unwrap(), before);
    assert_eq!(
        a.runtime.memory.read(GuestPtr(0x52000000), 32).unwrap(),
        &[0xbb; 32]
    );
    a.runtime
        .memory
        .map(GuestPtr(0x53000000), vec![0; 51], Permissions::RW)
        .unwrap();
    assert!(a
        .runtime
        .process
        .files
        .working_directory(
            &mut a.runtime.memory,
            GuestPtr(0x53000000),
            WorkingOp::Current { hierarchical: true }
        )
        .is_err());
    let mut readonly = vec![0; 52];
    readonly[18..22].copy_from_slice(&NAME.0.to_be_bytes());
    a.runtime
        .memory
        .map(GuestPtr(0x55000000), readonly.clone(), Permissions::RX)
        .unwrap();
    let before_name = a.runtime.memory.read(NAME, 32).unwrap().to_vec();
    assert!(a
        .runtime
        .process
        .files
        .working_directory(
            &mut a.runtime.memory,
            GuestPtr(0x55000000),
            WorkingOp::Current { hierarchical: true },
        )
        .is_err());
    assert_eq!(
        a.runtime.memory.read(GuestPtr(0x55000000), 52).unwrap(),
        readonly
    );
    assert_eq!(a.runtime.memory.read(NAME, 32).unwrap(), before_name);
    // Classic GetVol only requires its actual 24-byte record prefix.
    a.runtime
        .memory
        .map(GuestPtr(0x54000000), vec![0; 24], Permissions::RW)
        .unwrap();
    assert_eq!(
        a.runtime
            .process
            .files
            .working_directory(
                &mut a.runtime.memory,
                GuestPtr(0x54000000),
                WorkingOp::Current {
                    hierarchical: false
                }
            )
            .unwrap(),
        0
    );
}
