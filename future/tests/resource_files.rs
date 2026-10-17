mod support;
use systemless_future::{
    abi::{Abi, Reply, Service},
    cpu::{Architecture, Call, Cpu},
    files::{Fork, Op},
    load::{resource::ResourceFork, LoadedApplication, STACK_BASE},
    memory::{GuestPtr, Memory, Permissions},
    runtime::Stop,
};
const NAME: GuestPtr = GuestPtr(0x50000000);
const PB: GuestPtr = GuestPtr(0x50000100);
const BUFFER: GuestPtr = GuestPtr(0x50000200);
fn app() -> LoadedApplication {
    let p = support::pef_program(&[0x4e800020], &[b"CreateResFile"], 32);
    let mut a = LoadedApplication::pef(&p.bytes, Some(&support::code())).unwrap();
    a.runtime
        .memory
        .map(NAME, vec![0; 4096], Permissions::RW)
        .unwrap();
    a.runtime
        .memory
        .map(GuestPtr(0x51000000), vec![0xa9, 0xb1], Permissions::RX)
        .unwrap();
    a
}
fn name(a: &mut LoadedApplication, s: &[u8]) {
    let mut p = vec![s.len() as u8];
    p.extend(s);
    a.runtime.memory.write(NAME, &p).unwrap();
}
fn pb(a: &mut LoadedApplication, reference: i16) {
    let mut b = vec![0; 80];
    b[18..22].copy_from_slice(&NAME.0.to_be_bytes());
    b[24..26].copy_from_slice(&reference.to_be_bytes());
    b[27] = 3;
    b[32..36].copy_from_slice(&BUFFER.0.to_be_bytes());
    a.runtime.memory.write(PB, &b).unwrap();
}
fn io(a: &mut LoadedApplication, op: Op) -> i16 {
    a.runtime
        .process
        .files
        .io(&mut a.runtime.memory, PB, op)
        .unwrap()
}
fn open(a: &mut LoadedApplication, kind: Fork) -> i16 {
    pb(a, 0);
    assert_eq!(
        io(
            a,
            Op::Open {
                kind,
                hierarchical: false
            }
        ),
        0
    );
    a.runtime.memory.u16(GuestPtr(PB.0 + 24)).unwrap() as i16
}
fn create(a: &mut LoadedApplication) -> i16 {
    a.runtime
        .process
        .files
        .create_resource_file(&a.runtime.memory, NAME)
        .unwrap()
}
#[test]
fn serialized_resource_forks_roundtrip_names_ids_attributes_payloads_and_empty_maps() {
    let mut bytes = support::resource(vec![
        (*b"DATA", -7, Some(vec![0x80, b'x']), vec![1, 2, 3]),
        (*b"DATA", 2, Some(vec![]), vec![]),
        (*b"STR ", 32767, None, vec![4, 5]),
    ]);
    let at = u32::from_be_bytes(bytes[4..8].try_into().unwrap()) as usize;
    let types = at + u16::from_be_bytes(bytes[at + 24..at + 26].try_into().unwrap()) as usize;
    let reference =
        types + u16::from_be_bytes(bytes[types + 8..types + 10].try_into().unwrap()) as usize;
    bytes[reference + 4] = 0x50;
    let mut parsed = ResourceFork::parse(&bytes).unwrap();
    parsed.attributes = 0x40;
    let encoded = parsed.encode().unwrap();
    let again = ResourceFork::parse(&encoded).unwrap();
    assert_eq!(parsed.attributes, again.attributes);
    assert_eq!(
        parsed.entries().collect::<Vec<_>>(),
        again.entries().collect::<Vec<_>>()
    );
    assert_eq!(again.encode().unwrap(), encoded);
    let map = u32::from_be_bytes(encoded[4..8].try_into().unwrap()) as usize;
    assert_eq!(&encoded[..16], &encoded[map..map + 16]);
    assert_eq!(map % 2, 0);
    let empty = ResourceFork::default().encode().unwrap();
    assert_eq!(empty.len(), 286);
    assert_eq!(&empty[284..286], &[255, 255]);
    assert_eq!(ResourceFork::parse(&empty).unwrap().entries().count(), 0);
}
#[test]
fn resource_fork_serialization_rejects_unrepresentable_name_offsets() {
    let mut bytes = support::resource(
        (0..300)
            .map(|id| (*b"DATA", id, Some(vec![b'x'; 255]), vec![]))
            .collect(),
    );
    let map = u32::from_be_bytes(bytes[4..8].try_into().unwrap()) as usize;
    let types = map + u16::from_be_bytes(bytes[map + 24..map + 26].try_into().unwrap()) as usize;
    let refs =
        types + u16::from_be_bytes(bytes[types + 8..types + 10].try_into().unwrap()) as usize;
    for i in 0..300 {
        bytes[refs + i * 12 + 2..refs + i * 12 + 4].fill(0);
    }
    let fork = ResourceFork::parse(&bytes).unwrap();
    assert!(fork.encode().is_err());
}
#[test]
fn both_real_cpus_create_parseable_forks_publish_reserr_and_preserve_void_results() {
    for arch in [Architecture::M68k, Architecture::PowerPc] {
        let mut a = app();
        name(&mut a, b"Scores");
        let old_ref = a.runtime.process.current_resource_file();
        for expected in [0, -48] {
            let stack = STACK_BASE + 0x8000;
            match arch {
                Architecture::M68k => {
                    a.runtime
                        .memory
                        .write(GuestPtr(stack), &NAME.0.to_be_bytes())
                        .unwrap();
                    a.runtime.cpu = Cpu::new(arch, 0x51000000, stack);
                    let Cpu::M68k(c) = &mut a.runtime.cpu else {
                        panic!()
                    };
                    c.dar[0] = 0xdeadbeef;
                }
                Architecture::PowerPc => {
                    let tv = a
                        .runtime
                        .memory
                        .u32(GuestPtr(a.info.toc_or_a5 + 8))
                        .unwrap();
                    let pc = a.runtime.memory.u32(GuestPtr(tv)).unwrap();
                    a.runtime.cpu = Cpu::new(arch, pc, stack);
                    let Cpu::PowerPc(c) = &mut a.runtime.cpu else {
                        panic!()
                    };
                    c.gpr[3] = NAME.0;
                }
            }
            a.runtime.process.mem_error = -117;
            a.runtime
                .memory
                .write(GuestPtr(0x220), &(-117i16).to_be_bytes())
                .unwrap();
            a.runtime
                .memory
                .write(GuestPtr(0xa60), &(-192i16).to_be_bytes())
                .unwrap();
            assert_eq!(a.runtime.advance(1).unwrap(), Stop::Budget);
            assert_eq!(
                a.runtime.memory.u16(GuestPtr(0xa60)).unwrap() as i16,
                expected
            );
            assert_eq!(a.runtime.memory.u16(GuestPtr(0x220)).unwrap() as i16, -117);
            assert_eq!(a.runtime.process.current_resource_file(), old_ref);
            match &a.runtime.cpu {
                Cpu::M68k(c) => {
                    assert_eq!(c.dar[0], 0xdeadbeef);
                    assert_eq!(c.dar[15], stack + 4);
                }
                Cpu::PowerPc(c) => assert_eq!(c.gpr[3], NAME.0),
            }
        }
        assert!(a
            .runtime
            .services
            .calls
            .contains(&Call::CreateResourceFile(NAME)));
        let reference = open(&mut a, Fork::Resource);
        pb(&mut a, reference);
        a.runtime
            .memory
            .write(GuestPtr(PB.0 + 36), &286u32.to_be_bytes())
            .unwrap();
        assert_eq!(io(&mut a, Op::Read), 0);
        let bytes = a.runtime.memory.read(BUFFER, 286).unwrap();
        assert_eq!(ResourceFork::parse(bytes).unwrap().entries().count(), 0);
    }
}
#[test]
fn resource_creation_preserves_data_metadata_and_existing_open_fork_identity() {
    let mut a = app();
    name(&mut a, b"Existing");
    pb(&mut a, 0);
    assert_eq!(
        io(
            &mut a,
            Op::Create {
                hierarchical: false
            }
        ),
        0
    );
    let data = open(&mut a, Fork::Data);
    pb(&mut a, data);
    a.runtime.memory.write(BUFFER, b"KEEP").unwrap();
    a.runtime
        .memory
        .write(GuestPtr(PB.0 + 36), &4u32.to_be_bytes())
        .unwrap();
    assert_eq!(io(&mut a, Op::Write), 0);
    pb(&mut a, 0);
    a.runtime
        .memory
        .write(GuestPtr(PB.0 + 32), b"TEXTFUTR")
        .unwrap();
    a.runtime
        .memory
        .write(GuestPtr(PB.0 + 72), &1234u32.to_be_bytes())
        .unwrap();
    a.runtime
        .memory
        .write(GuestPtr(PB.0 + 76), &5678u32.to_be_bytes())
        .unwrap();
    assert_eq!(
        io(
            &mut a,
            Op::SetInfo {
                hierarchical: false
            }
        ),
        0
    );

    let resource = open(&mut a, Fork::Resource);
    pb(&mut a, resource);
    assert_eq!(io(&mut a, Op::GetEof), 0);
    assert_eq!(a.runtime.memory.u32(GuestPtr(PB.0 + 28)).unwrap(), 0);
    assert_eq!(create(&mut a), 0);
    pb(&mut a, resource);
    assert_eq!(io(&mut a, Op::GetEof), 0);
    assert_eq!(a.runtime.memory.u32(GuestPtr(PB.0 + 28)).unwrap(), 286);
    pb(&mut a, 0);
    assert_eq!(
        a.runtime
            .process
            .files
            .file_info(&mut a.runtime.memory, PB, false)
            .unwrap(),
        0
    );
    assert_eq!(
        a.runtime.memory.read(GuestPtr(PB.0 + 32), 8).unwrap(),
        b"TEXTFUTR"
    );
    assert_eq!(a.runtime.memory.u32(GuestPtr(PB.0 + 72)).unwrap(), 1234);
    assert_eq!(a.runtime.memory.u32(GuestPtr(PB.0 + 76)).unwrap(), 5678);
    assert_eq!(create(&mut a), -48);
    pb(&mut a, data);
    a.runtime
        .memory
        .write(GuestPtr(PB.0 + 44), &1u16.to_be_bytes())
        .unwrap();
    a.runtime
        .memory
        .write(GuestPtr(PB.0 + 36), &4u32.to_be_bytes())
        .unwrap();
    assert_eq!(io(&mut a, Op::Read), 0);
    assert_eq!(a.runtime.memory.read(BUFFER, 4).unwrap(), b"KEEP");
    name(&mut a, b"Application");
    assert_eq!(create(&mut a), -45);
    name(&mut a, b"");
    assert_eq!(create(&mut a), -37);
    name(&mut a, b":Missing:Scores");
    assert_eq!(create(&mut a), -120);
    assert!(a
        .runtime
        .process
        .files
        .create_resource_file(&a.runtime.memory, GuestPtr(0xdead0000))
        .is_err());
}
#[test]
fn resource_creation_disk_exhaustion_does_not_leave_a_partial_catalog_node() {
    let mut a = app();
    name(&mut a, b"Large");
    pb(&mut a, 0);
    assert_eq!(
        io(
            &mut a,
            Op::Create {
                hierarchical: false
            }
        ),
        0
    );
    let r = open(&mut a, Fork::Data);
    pb(&mut a, r);
    a.runtime
        .memory
        .write(GuestPtr(PB.0 + 28), &(64u32 * 1024 * 1024).to_be_bytes())
        .unwrap();
    assert_eq!(io(&mut a, Op::SetEof), 0);
    name(&mut a, b"Scores");
    assert_eq!(create(&mut a), -34);
    pb(&mut a, 0);
    assert_eq!(
        io(
            &mut a,
            Op::Open {
                kind: Fork::Data,
                hierarchical: false
            }
        ),
        -43
    );
}
#[test]
fn create_resource_file_adapters_validate_stack_and_do_not_return_errors_as_values() {
    let mut m = Memory::default();
    m.map(
        GuestPtr(0x1000),
        NAME.0.to_be_bytes().to_vec(),
        Permissions::RW,
    )
    .unwrap();
    let mut c = m68k::CpuCore::default();
    c.dar[15] = 0x1000;
    assert_eq!(
        Abi::trap(0xa9b1, &mut c, &m).unwrap(),
        (Call::CreateResourceFile(NAME), Reply::None)
    );
    assert_eq!(c.dar[15], 0x1004);
    assert!(Abi::trap(0xa9b1, &mut c, &m).is_err());
    assert_eq!(c.dar[15], 0x1004);
    let mut c = ppc::PpcCpu::default();
    c.gpr[3] = NAME.0;
    let (call, reply) = Abi::import(Service::CreateResFile, &c, &m).unwrap();
    assert_eq!(call, Call::CreateResourceFile(NAME));
    let mut cpu = Cpu::PowerPc(Box::new(c));
    reply.apply(&mut cpu, &mut m, 0, -48).unwrap();
    let Cpu::PowerPc(c) = cpu else { panic!() };
    assert_eq!(c.gpr[3], NAME.0);
}
