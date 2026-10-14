mod support;
use support::*;
use systemless_future::{
    load::resource::ResourceFork,
    memory::{GuestPtr, Memory},
    process::{Process, HEAP_SIZE},
};

fn loaded(attributes: u8) -> (Process, Memory) {
    let mut bytes = resource(vec![(*b"DATA", 128, None, vec![1, 2, 3])]);
    if attributes != 0 {
        let map = u32::from_be_bytes(bytes[4..8].try_into().unwrap()) as usize;
        let types =
            map + u16::from_be_bytes(bytes[map + 24..map + 26].try_into().unwrap()) as usize;
        let reference =
            types + u16::from_be_bytes(bytes[types + 8..types + 10].try_into().unwrap()) as usize;
        bytes[reference + 4] = attributes;
    }
    let mut process = Process::default();
    process.install_application_resources(ResourceFork::parse(&bytes).unwrap());
    (process, Memory::default())
}

#[test]
fn release_invalidates_identity_reclaims_memory_and_reload_restores_fork_bytes() {
    let (mut process, mut memory) = loaded(0);
    let handle = process.get_resource(&mut memory, *b"DATA", 128).unwrap();
    let data = memory.u32(GuestPtr(handle)).unwrap();
    memory.write(GuestPtr(data), &[99, 88, 77]).unwrap();
    process.release_resource(&mut memory, handle).unwrap();
    assert_eq!(process.res_error, 0);
    assert_eq!(memory.u32(GuestPtr(handle)).unwrap(), 0);
    process.release_resource(&mut memory, handle).unwrap();
    assert_eq!(process.res_error, -192);
    // Reuse all of the heap, proving both data and master allocations were freed.
    let allocation = process
        .new_handle(&mut memory, HEAP_SIZE - 4, false)
        .unwrap();
    assert_ne!(allocation, 0);
    process.dispose_handle(&mut memory, allocation).unwrap();
    let reloaded = process.get_resource(&mut memory, *b"DATA", 128).unwrap();
    assert_ne!(reloaded, 0);
    assert_eq!(process.res_error, 0);
    let data = memory.u32(GuestPtr(reloaded)).unwrap();
    assert_eq!(memory.read(GuestPtr(data), 3).unwrap(), [1, 2, 3]);
}

#[test]
fn release_keeps_changed_resources_and_rejects_nonresource_handles() {
    let (mut process, mut memory) = loaded(2);
    let handle = process.get_resource(&mut memory, *b"DATA", 128).unwrap();
    let data = memory.u32(GuestPtr(handle)).unwrap();
    memory.write(GuestPtr(data), &[42]).unwrap();
    process.release_resource(&mut memory, handle).unwrap();
    assert_eq!(process.res_error, 0);
    assert_eq!(
        process.get_resource(&mut memory, *b"DATA", 128).unwrap(),
        handle
    );
    assert_eq!(memory.read(GuestPtr(data), 1).unwrap(), [42]);
    let ordinary = process.new_handle(&mut memory, 9, true).unwrap();
    for invalid in [0, 0xffff_ffff, ordinary] {
        process.release_resource(&mut memory, invalid).unwrap();
        assert_eq!(process.res_error, -192);
    }
    assert_eq!(process.handle_size(&memory, ordinary).unwrap(), 9);
}

#[test]
fn resource_lock_attribute_prevents_relocation_until_explicit_unlock() {
    let (mut process, mut memory) = loaded(0x10);
    let handle = process.get_resource(&mut memory, *b"DATA", 128).unwrap();
    let original = memory.u32(GuestPtr(handle)).unwrap();
    process.set_handle_size(&mut memory, handle, 64).unwrap();
    assert_eq!(process.mem_error, -108);
    assert_eq!(memory.u32(GuestPtr(handle)).unwrap(), original);
    process.set_handle_lock(&memory, handle, false).unwrap();
    process.set_handle_size(&mut memory, handle, 64).unwrap();
    assert_eq!(process.mem_error, 0);
    assert_ne!(memory.u32(GuestPtr(handle)).unwrap(), original);
}

#[test]
fn detach_keeps_handle_payload_and_lock_while_reload_is_independent() {
    let (mut process, mut memory) = loaded(0x10);
    let handle = process.get_resource(&mut memory, *b"DATA", 128).unwrap();
    let data = memory.u32(GuestPtr(handle)).unwrap();
    memory.write(GuestPtr(data), &[55, 66, 77]).unwrap();
    process.mem_error = -111;
    process.detach_resource(&memory, handle).unwrap();
    assert_eq!(process.res_error, 0);
    assert_eq!(process.mem_error, -111);
    assert_eq!(memory.u32(GuestPtr(handle)).unwrap(), data);
    assert_eq!(memory.read(GuestPtr(data), 3).unwrap(), [55, 66, 77]);
    let fresh = process.get_resource(&mut memory, *b"DATA", 128).unwrap();
    assert_ne!(fresh, handle);
    let fresh_data = memory.u32(GuestPtr(fresh)).unwrap();
    assert_ne!(fresh_data, data);
    assert_eq!(memory.read(GuestPtr(fresh_data), 3).unwrap(), [1, 2, 3]);
    process.set_handle_size(&mut memory, handle, 64).unwrap();
    assert_eq!(process.mem_error, -108); // existing lock survives detachment.
    process.set_handle_lock(&memory, handle, false).unwrap();
    process.set_handle_size(&mut memory, handle, 64).unwrap();
    assert_eq!(process.mem_error, 0);
    let moved = memory.u32(GuestPtr(handle)).unwrap();
    assert_eq!(memory.read(GuestPtr(moved), 3).unwrap(), [55, 66, 77]);
    process.release_resource(&mut memory, handle).unwrap();
    assert_eq!(process.res_error, -192);
    assert_eq!(process.handle_size(&memory, handle).unwrap(), 64);
    process.dispose_handle(&mut memory, handle).unwrap();
    assert_eq!(memory.read(GuestPtr(fresh_data), 3).unwrap(), [1, 2, 3]);
    assert_eq!(
        process.get_resource(&mut memory, *b"DATA", 128).unwrap(),
        fresh
    );
}

#[test]
fn detach_rejects_changed_invalid_and_already_detached_handles_without_mutation() {
    let (mut process, mut memory) = loaded(2);
    let handle = process.get_resource(&mut memory, *b"DATA", 128).unwrap();
    let data = memory.u32(GuestPtr(handle)).unwrap();
    process.detach_resource(&memory, handle).unwrap();
    assert_eq!(process.res_error, -198);
    assert_eq!(
        process.get_resource(&mut memory, *b"DATA", 128).unwrap(),
        handle
    );
    assert_eq!(memory.u32(GuestPtr(handle)).unwrap(), data);
    let ordinary = process.new_handle(&mut memory, 9, true).unwrap();
    for invalid in [0, u32::MAX, ordinary] {
        process.detach_resource(&memory, invalid).unwrap();
        assert_eq!(process.res_error, -192);
    }
    assert_eq!(process.handle_size(&memory, ordinary).unwrap(), 9);
    let (mut process, mut memory) = loaded(0);
    let handle = process.get_resource(&mut memory, *b"DATA", 128).unwrap();
    process.detach_resource(&memory, handle).unwrap();
    process.detach_resource(&memory, handle).unwrap();
    assert_eq!(process.res_error, -192);
    assert_eq!(process.handle_size(&memory, handle).unwrap(), 3);
}

#[test]
fn detach_validates_corrupt_master_before_severing_identity_and_needs_no_allocation() {
    let (mut process, mut memory) = loaded(0);
    let handle = process.get_resource(&mut memory, *b"DATA", 128).unwrap();
    let data = memory.u32(GuestPtr(handle)).unwrap();
    memory.write(GuestPtr(handle), &0u32.to_be_bytes()).unwrap();
    assert!(process.detach_resource(&memory, handle).is_err());
    memory.write(GuestPtr(handle), &data.to_be_bytes()).unwrap();
    assert_eq!(
        process.get_resource(&mut memory, *b"DATA", 128).unwrap(),
        handle
    );
    let remaining = process
        .new_pointer(&mut memory, HEAP_SIZE - 8, false)
        .unwrap();
    assert_ne!(remaining, 0); // DATA rounds to four bytes, plus its four-byte master.
    process.detach_resource(&memory, handle).unwrap();
    assert_eq!(process.res_error, 0);
    assert_eq!(memory.u32(GuestPtr(handle)).unwrap(), data);
    assert_eq!(process.get_resource(&mut memory, *b"DATA", 128).unwrap(), 0);
    assert_eq!(process.res_error, -108);
    process.dispose_pointer(remaining);
    assert_ne!(process.get_resource(&mut memory, *b"DATA", 128).unwrap(), 0);
}

#[test]
fn native_detach_abis_share_resource_identity_and_preserve_void_results() {
    use systemless_future::{
        cpu::{Architecture, Cpu},
        load::{LoadedApplication, STACK_BASE},
        memory::Permissions,
        runtime::Stop,
    };
    let f = pef_program(&[0x4e800020], &[b"DetachResource"], 16);
    let fork = resource(vec![(*b"DATA", 128, None, vec![1, 2, 3])]);
    let mut app = LoadedApplication::pef(&f.bytes, Some(&fork)).unwrap();
    let handle = app
        .runtime
        .process
        .get_resource(&mut app.runtime.memory, *b"DATA", 128)
        .unwrap();
    let vector = app
        .runtime
        .memory
        .u32(GuestPtr(app.info.toc_or_a5 + 8))
        .unwrap();
    let pc = app.runtime.memory.u32(GuestPtr(vector)).unwrap();
    app.runtime.cpu = Cpu::new(Architecture::PowerPc, pc, STACK_BASE + 0x8000);
    if let Cpu::PowerPc(c) = &mut app.runtime.cpu {
        c.gpr[3] = handle;
    }
    assert_eq!(app.runtime.advance(1).unwrap(), Stop::Budget);
    let Cpu::PowerPc(c) = &app.runtime.cpu else {
        panic!()
    };
    assert_eq!(c.gpr[3], handle);
    assert_eq!(app.runtime.memory.u16(GuestPtr(0xa60)).unwrap(), 0);
    let fresh = app
        .runtime
        .process
        .get_resource(&mut app.runtime.memory, *b"DATA", 128)
        .unwrap();
    assert_ne!(fresh, handle);
    app.runtime
        .memory
        .map(GuestPtr(0x50000000), vec![0xa9, 0x92], Permissions::RX)
        .unwrap();
    let stack = STACK_BASE + 0x8000;
    app.runtime
        .memory
        .write(GuestPtr(stack), &fresh.to_be_bytes())
        .unwrap();
    app.runtime
        .memory
        .write(GuestPtr(stack + 4), &[0x5a; 4])
        .unwrap();
    app.runtime.cpu = Cpu::new(Architecture::M68k, 0x50000000, stack);
    if let Cpu::M68k(c) = &mut app.runtime.cpu {
        c.dar[0] = 0x12345678;
    }
    assert_eq!(app.runtime.advance(1).unwrap(), Stop::Budget);
    let Cpu::M68k(c) = &app.runtime.cpu else {
        panic!()
    };
    assert_eq!(c.dar[15], stack + 4);
    assert_eq!(c.dar[0], 0x12345678);
    assert_eq!(
        app.runtime.memory.read(GuestPtr(stack + 4), 4).unwrap(),
        [0x5a; 4]
    );
    assert_eq!(app.runtime.memory.u16(GuestPtr(0xa60)).unwrap(), 0);
    let third = app
        .runtime
        .process
        .get_resource(&mut app.runtime.memory, *b"DATA", 128)
        .unwrap();
    assert_ne!(third, fresh);
    assert_ne!(third, handle);
}
