mod support;
use systemless_future::{
    cpu::{Architecture, Cpu},
    devices::Op,
    load::{LoadedApplication, STACK_BASE},
    memory::{GuestPtr, Permissions},
    runtime::Stop,
};
const STACK: u32 = STACK_BASE + 0x8000;
fn table() -> Vec<u8> {
    let mut bytes = 0x12345678u32.to_be_bytes().to_vec();
    bytes.extend([0x80, 0, 0, 1]);
    bytes.extend([0, 3, 0x11, 0x22, 0x33, 0x44, 0x55, 0x66]);
    bytes.extend([0, 9, 0xaa, 0xbb, 0xcc, 0xdd, 0xee, 0xff]);
    bytes
}
fn app() -> LoadedApplication {
    let mut bad_count = table();
    bad_count[6..8].copy_from_slice(&256u16.to_be_bytes());
    let fork = support::resource(vec![
        (*b"clut", 130, None, table()),
        (*b"clut", 131, None, vec![0; 7]),
        (*b"clut", 132, None, table()[..8].to_vec()),
        (*b"clut", 133, None, bad_count),
        (*b"clut", 134, None, vec![0, 0, 0, 0, 0, 0, 0xff, 0xff]),
        (*b"clut", -130, None, table()),
    ]);
    let fixture = support::pef_program(&[0x4e800020], &[b"GetCTable", b"DisposeCTable"], 32);
    let mut a = LoadedApplication::pef(&fixture.bytes, Some(&fork)).unwrap();
    a.runtime
        .memory
        .map(GuestPtr(0x50000000), vec![0xaa, 0x18], Permissions::RX)
        .unwrap();
    a
}
fn ppc(a: &mut LoadedApplication, index: u32, argument: u32) -> u32 {
    let tv = a
        .runtime
        .memory
        .u32(GuestPtr(a.info.toc_or_a5 + 8 + index * 4))
        .unwrap();
    let pc = a.runtime.memory.u32(GuestPtr(tv)).unwrap();
    a.runtime.cpu = Cpu::new(Architecture::PowerPc, pc, STACK);
    let Cpu::PowerPc(c) = &mut a.runtime.cpu else {
        panic!()
    };
    c.gpr[3] = argument;
    assert_eq!(a.runtime.advance(1).unwrap(), Stop::Budget);
    let Cpu::PowerPc(c) = &a.runtime.cpu else {
        panic!()
    };
    c.gpr[3]
}
fn get(a: &mut LoadedApplication, arch: Architecture, id: i16) -> u32 {
    if arch == Architecture::PowerPc {
        return ppc(a, 0, 0xa5a50000 | u32::from(id as u16));
    }
    let mut frame = id.to_be_bytes().to_vec();
    frame.extend([0xcc; 4]);
    a.runtime.memory.write(GuestPtr(STACK), &frame).unwrap();
    a.runtime.cpu = Cpu::new(arch, 0x50000000, STACK);
    let Cpu::M68k(c) = &mut a.runtime.cpu else {
        panic!()
    };
    for i in 0..15 {
        c.dar[i] = 0x12340000 + i as u32;
    }
    assert_eq!(a.runtime.advance(1).unwrap(), Stop::Budget);
    let Cpu::M68k(c) = &a.runtime.cpu else {
        panic!()
    };
    for i in 0..15 {
        assert_eq!(c.dar[i], 0x12340000 + i as u32);
    }
    assert_eq!(c.dar[15], STACK + 2);
    a.runtime.memory.u32(GuestPtr(STACK + 2)).unwrap()
}
fn bytes(a: &mut LoadedApplication, handle: u32) -> Vec<u8> {
    let n = a
        .runtime
        .process
        .handle_size(&a.runtime.memory, handle)
        .unwrap();
    assert_eq!(a.runtime.process.mem_error, 0);
    let data = a.runtime.memory.u32(GuestPtr(handle)).unwrap();
    a.runtime
        .memory
        .read(GuestPtr(data), n as usize)
        .unwrap()
        .to_vec()
}
fn direct(a: &mut LoadedApplication, id: i16) -> systemless_future::Result<u32> {
    let mut manager = std::mem::take(&mut a.runtime.process.devices);
    let result = manager.call(
        &mut a.runtime.memory,
        &mut a.runtime.process,
        Op::GetTable(id),
    );
    a.runtime.process.devices = manager;
    result
}
#[test]
fn both_cpus_get_independent_tables_with_live_resource_bytes_and_preserved_seed() {
    for arch in [Architecture::M68k, Architecture::PowerPc] {
        let mut a = app();
        let source = a
            .runtime
            .process
            .get_resource(&mut a.runtime.memory, *b"clut", 130)
            .unwrap();
        a.runtime
            .process
            .set_handle_state(&a.runtime.memory, source, 0xe0)
            .unwrap();
        let first = get(&mut a, arch, 130);
        assert_ne!(first, source);
        assert_eq!(bytes(&mut a, first), table());
        assert_eq!(
            a.runtime
                .process
                .handle_state(&a.runtime.memory, first)
                .unwrap(),
            0
        );
        let data = a.runtime.memory.u32(GuestPtr(source)).unwrap();
        a.runtime
            .memory
            .write(GuestPtr(data + 10), &[0x98, 0x76])
            .unwrap();
        let second = get(&mut a, arch, 130);
        assert_ne!(first, second);
        assert_eq!(bytes(&mut a, first), table());
        let mut changed = table();
        changed[10..12].copy_from_slice(&[0x98, 0x76]);
        assert_eq!(bytes(&mut a, second), changed);
        a.runtime
            .process
            .release_resource(&mut a.runtime.memory, source)
            .unwrap();
        assert_eq!(bytes(&mut a, second), changed);
        ppc(&mut a, 1, first);
        assert_eq!(a.runtime.memory.u32(GuestPtr(first)).unwrap(), 0);
        assert_eq!(bytes(&mut a, second), changed);
        let negative = get(&mut a, arch, -130);
        assert_eq!(bytes(&mut a, negative), table());
        assert_eq!(a.runtime.memory.u16(GuestPtr(0xa60)).unwrap(), 0);
    }
}
#[test]
fn table_missing_empty_malformed_and_system_requests_have_explicit_results() {
    let mut a = app();
    for arch in [Architecture::M68k, Architecture::PowerPc] {
        assert_eq!(get(&mut a, arch, 999), 0);
        assert_eq!(a.runtime.memory.u16(GuestPtr(0xa60)).unwrap() as i16, -192);
        let empty = get(&mut a, arch, 134);
        assert_eq!(bytes(&mut a, empty), [0, 0, 0, 0, 0, 0, 0xff, 0xff]);
    }
    for id in [131, 132, 133] {
        // Preload so the malformed-table operation cannot allocate a copy.
        a.runtime
            .process
            .get_resource(&mut a.runtime.memory, *b"clut", id)
            .unwrap();
        let before = a.runtime.process.free_memory();
        assert!(direct(&mut a, id).is_err());
        assert_eq!(a.runtime.process.free_memory(), before);
    }
    assert!(direct(&mut a, 8)
        .unwrap_err()
        .to_string()
        .contains("standard system color tables"));
}
#[test]
fn copy_allocation_failure_returns_nil_with_real_memory_error_and_preserves_resource() {
    let mut a = app();
    let source = a
        .runtime
        .process
        .get_resource(&mut a.runtime.memory, *b"clut", 130)
        .unwrap();
    let free = a.runtime.process.free_memory();
    let filler = a
        .runtime
        .process
        .new_pointer(&mut a.runtime.memory, free, false)
        .unwrap();
    assert_ne!(filler, 0);
    assert_eq!(a.runtime.process.free_memory(), 0);
    assert_eq!(get(&mut a, Architecture::M68k, 130), 0);
    assert_eq!(a.runtime.process.mem_error, -108);
    assert_eq!(a.runtime.memory.u16(GuestPtr(0x220)).unwrap() as i16, -108);
    assert_eq!(a.runtime.memory.u16(GuestPtr(0xa60)).unwrap(), 0);
    assert_eq!(bytes(&mut a, source), table());
}
