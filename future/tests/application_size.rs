mod support;
use systemless_future::{
    cpu::Cpu,
    load::{resource::ResourceFork, LoadedApplication, A5_BASE, STACK_BASE, STACK_SIZE},
    memory::GuestPtr,
    runtime::Stop,
};
fn size(preferred: u32, minimum: u32) -> Vec<u8> {
    [
        0x5880u16.to_be_bytes().as_slice(),
        preferred.to_be_bytes().as_slice(),
        minimum.to_be_bytes().as_slice(),
    ]
    .concat()
}
fn fork(sizes: Vec<(i16, Vec<u8>)>) -> Vec<u8> {
    let parsed = ResourceFork::parse(&support::code_program(&[0x702a, 0x4e75])).unwrap();
    let mut entries = parsed
        .entries()
        .map(|r| (r.kind, r.id, r.name.clone(), r.data.clone()))
        .collect::<Vec<_>>();
    entries.extend(
        sizes
            .into_iter()
            .map(|(id, data)| (*b"SIZE", id, None, data)),
    );
    support::resource(entries)
}
#[test]
fn code_uses_declared_partition_and_moves_stack_and_eager_segments_without_overlap() {
    for preferred in [8499200, 20 * 1024 * 1024] {
        let mut a =
            LoadedApplication::code(&fork(vec![(-1, size(preferred, 512 * 1024))])).unwrap();
        let request = a.info.application_size.unwrap();
        assert_eq!(
            (request.flags, request.preferred, request.resource_id),
            (0x5880, preferred, -1)
        );
        let stack = a.info.stack.0;
        assert_eq!(a.info.stack.1, STACK_SIZE);
        assert_eq!(stack + STACK_SIZE, 0x10000 + preferred);
        let world = a.runtime.memory.u32(GuestPtr(0x908)).unwrap();
        assert_eq!(a.runtime.memory.u32(GuestPtr(0x114)).unwrap(), world);
        assert_eq!(a.runtime.memory.u32(GuestPtr(0x130)).unwrap(), world);
        assert_eq!(a.runtime.process.free_memory(), world - 0x10000);
        assert_eq!(
            a.runtime.memory.u32(GuestPtr(0x904)).unwrap(),
            a.info.toc_or_a5
        );
        assert!(world + 256 + 40 <= stack);
        for &(base, len) in &a.info.sections {
            assert!(base >= stack + STACK_SIZE || base + len <= world);
        }
        let Cpu::M68k(c) = &a.runtime.cpu else {
            panic!()
        };
        assert_eq!(c.dar[15], stack + STACK_SIZE - 0x100);
        assert_eq!(a.runtime.advance(20).unwrap(), Stop::Exit(0));
    }
}
#[test]
fn size_zero_overrides_original_and_both_loaders_reject_invalid_requests() {
    let bytes = fork(vec![
        (-1, size(8499200, 512 * 1024)),
        (0, size(2 * 1024 * 1024, 1024 * 1024)),
    ]);
    let a = LoadedApplication::code(&bytes).unwrap();
    assert_eq!(a.info.application_size.unwrap().resource_id, 0);
    assert_eq!(a.info.stack.0 + STACK_SIZE, 0x10000 + 2 * 1024 * 1024);
    let pef = support::pef_program(&[0x4e800020], &[], 16);
    let a = LoadedApplication::pef(&pef.bytes, Some(&bytes)).unwrap();
    assert_eq!(a.info.application_size.unwrap().preferred, 2 * 1024 * 1024);
    assert_eq!(
        a.runtime.process.free_memory(),
        2 * 1024 * 1024 - STACK_SIZE
    );
    assert_eq!(a.info.stack.0, STACK_BASE);
    for data in [
        vec![],
        vec![0; 9],
        vec![0; 11],
        size(0, 0),
        size(1024 * 1024, 2 * 1024 * 1024),
        size(0x80000000, 512 * 1024),
        size(64 * 1024 * 1024 + 16, 512 * 1024),
    ] {
        let bytes = fork(vec![(-1, size(8499200, 512 * 1024)), (0, data)]);
        assert!(LoadedApplication::code(&bytes).is_err());
        assert!(LoadedApplication::pef(&pef.bytes, Some(&bytes)).is_err());
    }
}
#[test]
fn absent_size_keeps_explicit_runtime_defaults_and_tiny_declared_world_is_rejected() {
    let a = LoadedApplication::code(&fork(vec![])).unwrap();
    assert!(a.info.application_size.is_none());
    assert_eq!(a.info.toc_or_a5, A5_BASE);
    assert_eq!(a.info.stack.0, STACK_BASE);
    assert!(LoadedApplication::code(&fork(vec![(-1, size(STACK_SIZE + 272, 1))])).is_err());
}
