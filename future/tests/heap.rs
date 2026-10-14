use systemless_future::{
    memory::{GuestPtr, Memory},
    process::{Process, HEAP_SIZE},
};
#[test]
fn resize_moves_data_preserves_handle_and_shrinks_then_grows_in_place() {
    let mut memory = Memory::default();
    let mut process = Process::default();
    let handle = process.new_handle(&mut memory, 3, true).unwrap();
    let original = memory.u32(GuestPtr(handle)).unwrap();
    memory.write(GuestPtr(original), &[17, 34, 51]).unwrap();
    process.set_handle_size(&mut memory, handle, 64).unwrap();
    assert_eq!(process.mem_error, 0);
    let moved = memory.u32(GuestPtr(handle)).unwrap();
    assert_ne!(original, moved); // master pointer blocks adjacent growth
    assert_eq!(memory.read(GuestPtr(moved), 3).unwrap(), &[17, 34, 51]);
    assert_eq!(process.handle_size(&memory, handle).unwrap(), 64);
    process.set_handle_size(&mut memory, handle, 1).unwrap();
    assert_eq!(memory.u32(GuestPtr(handle)).unwrap(), moved);
    process.set_handle_size(&mut memory, handle, 32).unwrap();
    assert_eq!(memory.u32(GuestPtr(handle)).unwrap(), moved);
    assert_eq!(memory.read(GuestPtr(moved), 1).unwrap(), &[17]);
    process.set_handle_size(&mut memory, handle, 0).unwrap();
    assert_eq!(process.handle_size(&memory, handle).unwrap(), 0);
    assert_eq!(process.mem_error, 0);
    process.dispose_handle(&mut memory, handle).unwrap();
    // All split/relocated extents coalesce back into one usable heap.
    assert_ne!(
        process
            .new_handle(&mut memory, HEAP_SIZE - 4, false)
            .unwrap(),
        0
    );
}
#[test]
fn failed_growth_preserves_master_size_and_contents() {
    let mut memory = Memory::default();
    let mut process = Process::default();
    let handle = process.new_handle(&mut memory, 8, true).unwrap();
    let data = memory.u32(GuestPtr(handle)).unwrap();
    memory.write(GuestPtr(data), &[0xab; 8]).unwrap();
    for size in [HEAP_SIZE, u32::MAX] {
        process.set_handle_size(&mut memory, handle, size).unwrap();
        assert_eq!(process.mem_error, -108);
        assert_eq!(memory.u32(GuestPtr(handle)).unwrap(), data);
        assert_eq!(process.handle_size(&memory, handle).unwrap(), 8);
        assert_eq!(memory.read(GuestPtr(data), 8).unwrap(), &[0xab; 8]);
    }
    memory.write(GuestPtr(handle), &0u32.to_be_bytes()).unwrap();
    assert!(process.set_handle_size(&mut memory, handle, 16).is_err());
    memory.write(GuestPtr(handle), &data.to_be_bytes()).unwrap();
    assert_eq!(process.handle_size(&memory, handle).unwrap(), 8);
}
#[test]
fn resize_reports_nil_and_disposed_handles_without_allocating() {
    let mut memory = Memory::default();
    let mut process = Process::default();
    process.set_handle_size(&mut memory, 0, 16).unwrap();
    assert_eq!(process.mem_error, -109);
    let handle = process.new_handle(&mut memory, 4, false).unwrap();
    process.dispose_handle(&mut memory, handle).unwrap();
    process.set_handle_size(&mut memory, handle, 16).unwrap();
    assert_eq!(process.mem_error, -111);
}
