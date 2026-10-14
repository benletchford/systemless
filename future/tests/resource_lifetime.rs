mod support;
use support::*;
use systemless_future::{
    load::resource::ResourceFork,
    memory::{GuestPtr, Memory},
    process::{Process, HEAP_SIZE},
};

fn loaded(changed: bool) -> (Process, Memory) {
    let mut bytes = resource(vec![(*b"DATA", 128, None, vec![1, 2, 3])]);
    if changed {
        let map = u32::from_be_bytes(bytes[4..8].try_into().unwrap()) as usize;
        let types =
            map + u16::from_be_bytes(bytes[map + 24..map + 26].try_into().unwrap()) as usize;
        let reference =
            types + u16::from_be_bytes(bytes[types + 8..types + 10].try_into().unwrap()) as usize;
        bytes[reference + 4] = 2; // resChanged
    }
    let mut process = Process::default();
    process.install_application_resources(ResourceFork::parse(&bytes).unwrap());
    (process, Memory::default())
}

#[test]
fn release_invalidates_identity_reclaims_memory_and_reload_restores_fork_bytes() {
    let (mut process, mut memory) = loaded(false);
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
    let (mut process, mut memory) = loaded(true);
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
