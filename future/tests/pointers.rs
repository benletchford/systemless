use systemless_future::{
    memory::{GuestPtr, Memory},
    process::{Process, HEAP_SIZE},
};
#[test]
fn pointers_share_handle_heap_but_cannot_resize_or_dispose_handle_storage() {
    let mut memory = Memory::default();
    let mut process = Process::default();
    let pointer = process.new_pointer(&mut memory, 8, true).unwrap();
    let handle = process.new_handle(&mut memory, 8, true).unwrap();
    let data = memory.u32(GuestPtr(handle)).unwrap();
    memory.write(GuestPtr(pointer), &[0x42; 8]).unwrap();
    process.set_pointer_size(pointer, 64);
    assert_eq!(process.mem_error, -108);
    assert_eq!(process.pointer_size(pointer), 8);
    assert_eq!(memory.read(GuestPtr(pointer), 8).unwrap(), [0x42; 8]);
    for wrong in [0, pointer + 1, handle, data] {
        process.dispose_pointer(wrong);
        assert_eq!(process.mem_error, -111);
        process.set_pointer_size(wrong, 32);
        assert_eq!(process.mem_error, -111);
        assert_eq!(process.pointer_size(wrong), (-111i32) as u32);
    }
    assert_eq!(process.handle_size(&memory, handle).unwrap(), 8);
    process.dispose_handle(&mut memory, pointer).unwrap();
    assert_eq!(process.mem_error, -111);
    process.dispose_handle(&mut memory, handle).unwrap();
    process.set_pointer_size(pointer, 64);
    assert_eq!(process.mem_error, 0);
    assert_eq!(process.pointer_size(pointer), 64);
    assert_eq!(memory.read(GuestPtr(pointer), 8).unwrap(), [0x42; 8]);
    process.set_pointer_size(pointer, 1);
    assert_eq!(process.mem_error, 0);
    process.dispose_pointer(pointer);
    assert_eq!(process.mem_error, 0);
    let entire = process.new_pointer(&mut memory, HEAP_SIZE, false).unwrap();
    assert_ne!(entire, 0);
    assert_eq!(process.new_handle(&mut memory, 1, false).unwrap(), 0);
    process.dispose_pointer(entire);
    assert_ne!(
        process
            .new_handle(&mut memory, HEAP_SIZE - 4, false)
            .unwrap(),
        0
    );
}
#[test]
fn pointer_zero_size_reuse_clearing_and_failed_resize_preserve_ownership() {
    let mut memory = Memory::default();
    let mut process = Process::default();
    let pointer = process.new_pointer(&mut memory, 16, false).unwrap();
    memory.write(GuestPtr(pointer), &[0xff; 16]).unwrap();
    process.dispose_pointer(pointer);
    let reused = process.new_pointer(&mut memory, 16, true).unwrap();
    assert_eq!(reused, pointer);
    assert_eq!(memory.read(GuestPtr(reused), 16).unwrap(), [0; 16]);
    for size in [HEAP_SIZE + 1, u32::MAX] {
        process.set_pointer_size(pointer, size);
        assert_eq!(process.mem_error, -108);
        assert_eq!(process.pointer_size(pointer), 16);
        assert_eq!(process.new_pointer(&mut memory, size, false).unwrap(), 0);
        assert_eq!(process.mem_error, -108);
    }
    process.set_pointer_size(pointer, 0);
    assert_eq!(process.pointer_size(pointer), 0);
    assert_eq!(process.mem_error, 0);
    let zero = process.new_pointer(&mut memory, 0, true).unwrap();
    assert_ne!(zero, 0);
    assert_ne!(zero, pointer);
    process.dispose_pointer(zero);
    process.dispose_pointer(pointer);
    process.dispose_pointer(pointer);
    assert_eq!(process.mem_error, -111);
    assert_ne!(
        process.new_pointer(&mut memory, HEAP_SIZE, false).unwrap(),
        0
    );
}

mod support;
#[test]
fn real_cpu_adapters_operate_on_the_same_nonrelocatable_block() {
    use systemless_future::{
        cpu::{Architecture, Cpu},
        load::{LoadedApplication, STACK_BASE},
        memory::Permissions,
        runtime::Stop,
    };
    let f = support::pef_program(&[0x4e800020], &[b"NewPtrClear", b"GetPtrSize"], 32);
    let mut app = LoadedApplication::pef(&f.bytes, None).unwrap();
    let stack = STACK_BASE + 0x8000;
    let gate = |app: &LoadedApplication, index: u32| {
        let vector = app
            .runtime
            .memory
            .u32(GuestPtr(app.info.toc_or_a5 + 8 + index * 4))
            .unwrap();
        app.runtime.memory.u32(GuestPtr(vector)).unwrap()
    };
    app.runtime.cpu = Cpu::new(Architecture::PowerPc, gate(&app, 0), stack);
    if let Cpu::PowerPc(c) = &mut app.runtime.cpu {
        c.gpr[3] = 8;
    }
    assert_eq!(app.runtime.advance(1).unwrap(), Stop::Budget);
    let Cpu::PowerPc(c) = &app.runtime.cpu else {
        panic!()
    };
    let pointer = c.gpr[3];
    assert_ne!(pointer, 0);
    app.runtime
        .memory
        .write(GuestPtr(pointer), &[17; 8])
        .unwrap();
    app.runtime
        .memory
        .map(
            GuestPtr(0x50000000),
            vec![0xa0, 0x20, 0xa0, 0x1f],
            Permissions::RX,
        )
        .unwrap();
    app.runtime.cpu = Cpu::new(Architecture::M68k, 0x50000000, stack);
    if let Cpu::M68k(c) = &mut app.runtime.cpu {
        c.dar[8] = pointer;
        c.dar[0] = 64;
    }
    assert_eq!(app.runtime.advance(1).unwrap(), Stop::Budget);
    let Cpu::M68k(c) = &app.runtime.cpu else {
        panic!()
    };
    assert_eq!(c.dar[0], 0);
    assert_eq!(c.dar[8], pointer);
    assert_eq!(c.dar[15], stack);
    assert_eq!(app.runtime.memory.u16(GuestPtr(0x220)).unwrap(), 0);
    app.runtime.cpu = Cpu::new(Architecture::PowerPc, gate(&app, 1), stack);
    if let Cpu::PowerPc(c) = &mut app.runtime.cpu {
        c.gpr[3] = pointer;
    }
    assert_eq!(app.runtime.advance(1).unwrap(), Stop::Budget);
    let Cpu::PowerPc(c) = &app.runtime.cpu else {
        panic!()
    };
    assert_eq!(c.gpr[3], 64);
    assert_eq!(
        app.runtime.memory.read(GuestPtr(pointer), 8).unwrap(),
        [17; 8]
    );
    app.runtime.cpu = Cpu::new(Architecture::M68k, 0x50000002, stack);
    if let Cpu::M68k(c) = &mut app.runtime.cpu {
        c.dar[8] = pointer;
    }
    assert_eq!(app.runtime.advance(1).unwrap(), Stop::Budget);
    assert_eq!(app.runtime.process.pointer_size(pointer), (-111i32) as u32);
}
