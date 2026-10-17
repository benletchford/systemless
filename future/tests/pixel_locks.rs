mod support;
use systemless_future::{
    abi::Abi,
    cpu::{Architecture, Cpu},
    graphics::Graphics,
    load::{LoadedApplication, STACK_BASE},
    memory::{GuestPtr, Memory, Permissions},
    runtime::Stop,
};
fn app() -> LoadedApplication {
    let f = support::pef_program(
        &[0x4e800020],
        &[
            b"LockPixels",
            b"UnlockPixels",
            b"InitGraf",
            b"OpenPort",
            b"GetPixBaseAddr",
        ],
        32,
    );
    let mut a = LoadedApplication::pef(&f.bytes, None).unwrap();
    a.runtime
        .memory
        .map(GuestPtr(0x50000000), vec![0xab, 0x1d], Permissions::RX)
        .unwrap();
    a
}
fn ppc(a: &mut LoadedApplication, index: u32, arg: u32) -> u32 {
    let tv = a
        .runtime
        .memory
        .u32(GuestPtr(a.info.toc_or_a5 + 8 + index * 4))
        .unwrap();
    let pc = a.runtime.memory.u32(GuestPtr(tv)).unwrap();
    a.runtime.cpu = Cpu::new(Architecture::PowerPc, pc, STACK_BASE + 0x7000);
    let Cpu::PowerPc(c) = &mut a.runtime.cpu else {
        panic!()
    };
    c.gpr[3] = arg;
    c.gpr[4] = 0x12345678;
    assert_eq!(a.runtime.advance(1).unwrap(), Stop::Budget);
    let Cpu::PowerPc(c) = &a.runtime.cpu else {
        panic!()
    };
    assert_eq!(c.gpr[4], 0x12345678);
    c.gpr[3]
}
fn call(a: &mut LoadedApplication, arch: Architecture, pm: u32, lock: bool) -> bool {
    if arch == Architecture::PowerPc {
        return ppc(a, u32::from(!lock), pm) != 0;
    }
    let stack = STACK_BASE + 0x7000;
    let mut frame = pm.to_be_bytes().to_vec();
    frame.extend([0xa5; 4]);
    a.runtime.memory.write(GuestPtr(stack), &frame).unwrap();
    a.runtime.cpu = Cpu::new(arch, 0x50000000, stack);
    let Cpu::M68k(c) = &mut a.runtime.cpu else {
        panic!()
    };
    for i in 0..15 {
        c.dar[i] = 0xabc00000 + i as u32;
    }
    c.dar[0] = if lock { 0x40001 } else { 0x40002 };
    assert_eq!(a.runtime.advance(1).unwrap(), Stop::Budget);
    let Cpu::M68k(c) = &a.runtime.cpu else {
        panic!()
    };
    assert_eq!(c.dar[15], stack + 4);
    assert_eq!(c.dar[0], if lock { 0x40001 } else { 0x40002 });
    for i in 1..15 {
        assert_eq!(c.dar[i], 0xabc00000 + i as u32);
    }
    assert_eq!(
        a.runtime.memory.read(GuestPtr(stack + 5), 3).unwrap(),
        [0xa5; 3]
    );
    if lock {
        a.runtime.memory.read(GuestPtr(stack + 4), 1).unwrap()[0] != 0
    } else {
        false
    }
}
fn handle(a: &mut LoadedApplication, size: u32) -> u32 {
    a.runtime
        .process
        .new_handle(&mut a.runtime.memory, size, true)
        .unwrap()
}
fn pm(a: &mut LoadedApplication, base: u32) -> u32 {
    let h = handle(a, 50);
    let data = a.runtime.memory.u32(GuestPtr(h)).unwrap();
    let mut bytes = [0; 50];
    bytes[..4].copy_from_slice(&base.to_be_bytes());
    for (at, value) in [(4, 0x8002u16), (10, 2), (12, 8), (32, 1), (34, 1), (36, 1)] {
        bytes[at..at + 2].copy_from_slice(&value.to_be_bytes());
    }
    a.runtime.memory.write(GuestPtr(data), &bytes).unwrap();
    h
}
fn base(a: &LoadedApplication, h: u32) -> u32 {
    let data = a.runtime.memory.u32(GuestPtr(h)).unwrap();
    a.runtime.memory.u32(GuestPtr(data)).unwrap()
}
#[test]
fn both_cpus_pin_movable_pixels_across_compaction_purge_and_recover_the_handle() {
    for arch in [Architecture::M68k, Architecture::PowerPc] {
        let mut a = app();
        let gap = a
            .runtime
            .process
            .new_pointer(&mut a.runtime.memory, 1024, false)
            .unwrap();
        let image = handle(&mut a, 4);
        let data = a.runtime.memory.u32(GuestPtr(image)).unwrap();
        a.runtime.memory.write(GuestPtr(data), &[0xa5; 4]).unwrap();
        let map = pm(&mut a, image);
        a.runtime
            .process
            .set_handle_state(&a.runtime.memory, image, 0x40)
            .unwrap();
        a.runtime.process.dispose_pointer(gap);
        assert!(call(&mut a, arch, map, true));
        assert_eq!(base(&a, map), data);
        assert!(call(&mut a, arch, map, true)); // state, not a recursive lock count
        a.runtime
            .process
            .set_handle_lock(&a.runtime.memory, image, false)
            .unwrap();
        a.runtime
            .process
            .move_handle_high(&mut a.runtime.memory, image)
            .unwrap();
        assert_eq!(a.runtime.process.mem_error, -117);
        assert!(a
            .runtime
            .process
            .dispose_handle(&mut a.runtime.memory, image)
            .is_err());
        a.runtime
            .process
            .purge_and_compact_memory(&mut a.runtime.memory)
            .unwrap();
        assert_eq!(a.runtime.memory.u32(GuestPtr(image)).unwrap(), data);
        assert_eq!(a.runtime.memory.read(GuestPtr(data), 4).unwrap(), [0xa5; 4]);
        assert_eq!(base(&a, map), data); // PixMap itself may have moved
        call(&mut a, arch, map, false);
        assert_eq!(base(&a, map), image);
        assert_eq!(
            a.runtime
                .process
                .handle_state(&a.runtime.memory, image)
                .unwrap(),
            0x40
        );
        a.runtime
            .process
            .purge_and_compact_memory(&mut a.runtime.memory)
            .unwrap();
        assert_eq!(a.runtime.memory.u32(GuestPtr(image)).unwrap(), 0);
        assert!(!call(&mut a, arch, map, true));
        assert_eq!(base(&a, map), image);
        call(&mut a, arch, map, false); // purged unlock is harmless
    }
}
#[test]
fn fixed_newptr_images_preserve_bytes_errors_and_free_memory_on_both_cpus() {
    let mut a = app();
    let data = a
        .runtime
        .process
        .new_pointer(&mut a.runtime.memory, 4, true)
        .unwrap();
    let map = pm(&mut a, data);
    let free = a.runtime.process.free_memory();
    for arch in [Architecture::M68k, Architecture::PowerPc] {
        a.runtime.process.mem_error = -42;
        a.runtime.process.res_error = -43;
        assert!(call(&mut a, arch, map, true));
        call(&mut a, arch, map, false);
        assert_eq!(base(&a, map), data);
        assert_eq!(a.runtime.process.free_memory(), free);
        assert_eq!(a.runtime.process.mem_error, -42);
        assert_eq!(a.runtime.process.res_error, -43);
    }
}
#[test]
fn separate_pixmaps_keep_a_shared_image_pinned_until_each_is_unlocked() {
    let mut a = app();
    let image = handle(&mut a, 4);
    let first = pm(&mut a, image);
    let second = pm(&mut a, image);
    a.runtime
        .process
        .set_handle_state(&a.runtime.memory, image, 0x40)
        .unwrap();
    assert!(call(&mut a, Architecture::M68k, first, true));
    assert!(call(&mut a, Architecture::PowerPc, second, true));
    call(&mut a, Architecture::M68k, first, false);
    a.runtime
        .process
        .purge_and_compact_memory(&mut a.runtime.memory)
        .unwrap();
    assert_ne!(a.runtime.memory.u32(GuestPtr(image)).unwrap(), 0);
    call(&mut a, Architecture::PowerPc, second, false);
    a.runtime
        .process
        .purge_and_compact_memory(&mut a.runtime.memory)
        .unwrap();
    assert_eq!(a.runtime.memory.u32(GuestPtr(image)).unwrap(), 0);
}
#[test]
fn malformed_images_and_native_results_fault_before_pixel_or_stack_changes() {
    let mut a = app();
    let image = handle(&mut a, 2);
    let map = pm(&mut a, image);
    assert!(a
        .runtime
        .process
        .pixels(&mut a.runtime.memory, map, true)
        .is_err());
    assert_eq!(base(&a, map), image);
    for selector in [0x40001, 0x50001, 0x40003] {
        let mut memory = Memory::default();
        memory
            .map(GuestPtr(0x1000), vec![0; 4], Permissions::RW)
            .unwrap();
        let mut cpu = Cpu::new(Architecture::M68k, 0, 0x1000);
        let Cpu::M68k(c) = &mut cpu else { panic!() };
        c.dar[0] = selector;
        assert!(Abi::trap(0xab1d, c, &memory).is_err());
        assert_eq!(c.dar[15], 0x1000);
    }
}
#[test]
#[ignore = "requires a real wgpu adapter"]
fn locked_image_guest_edits_and_gpu_copybits_share_canonical_pixels() {
    use systemless_future::quickdraw::CopyBits;
    let mut a = app();
    a.runtime
        .memory
        .map(GuestPtr(0x1800), vec![0; 512], Permissions::RW)
        .unwrap();
    ppc(&mut a, 2, 0x600);
    ppc(&mut a, 3, 0x1800);
    let image = handle(&mut a, 4);
    let map = pm(&mut a, image);
    let table = handle(&mut a, 24);
    let ct = a.runtime.memory.u32(GuestPtr(table)).unwrap();
    let mut colors = [0; 24];
    colors[6..8].copy_from_slice(&1u16.to_be_bytes());
    colors[10..16].fill(0xff);
    colors[16..18].copy_from_slice(&1u16.to_be_bytes());
    a.runtime.memory.write(GuestPtr(ct), &colors).unwrap();
    let descriptor = a.runtime.memory.u32(GuestPtr(map)).unwrap();
    a.runtime
        .memory
        .write(GuestPtr(descriptor + 42), &table.to_be_bytes())
        .unwrap();
    let destination = a
        .runtime
        .process
        .new_pointer(&mut a.runtime.memory, 4, true)
        .unwrap();
    let mut bitmap = [0; 14];
    bitmap[..4].copy_from_slice(&destination.to_be_bytes());
    for (at, value) in [(4, 2u16), (10, 2), (12, 8)] {
        bitmap[at..at + 2].copy_from_slice(&value.to_be_bytes());
    }
    a.runtime.memory.write(GuestPtr(0x1900), &bitmap).unwrap();
    a.runtime
        .memory
        .write(GuestPtr(0x1920), &[0, 0, 0, 0, 0, 2, 0, 8])
        .unwrap();
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
    let gpu =
        pollster::block_on(Graphics::new(&instance, None, 8, 2, GuestPtr(0x51000000))).unwrap();
    let ports = a.runtime.process.ports.unwrap();
    for (arch, pattern) in [(Architecture::M68k, 0xa5), (Architecture::PowerPc, 0x5a)] {
        assert!(call(&mut a, arch, map, true));
        let data = base(&a, map);
        a.runtime
            .memory
            .write(GuestPtr(data), &[pattern, 0xcc, pattern, 0xdd])
            .unwrap();
        ports
            .copy_bits(
                &mut a.runtime.memory,
                &gpu,
                CopyBits {
                    source: GuestPtr(descriptor),
                    destination: GuestPtr(0x1900),
                    source_rect: GuestPtr(0x1920),
                    destination_rect: GuestPtr(0x1920),
                    mode: 0,
                    mask: GuestPtr(0),
                },
            )
            .unwrap();
        assert_eq!(
            a.runtime.memory.read(GuestPtr(destination), 4).unwrap(),
            [pattern, 0, pattern, 0]
        );
        call(&mut a, arch, map, false);
    }
}

fn address(a: &mut LoadedApplication, arch: Architecture, map: u32) -> u32 {
    if arch == Architecture::PowerPc {
        return ppc(a, 4, map);
    }
    let stack = STACK_BASE + 0x7000;
    let mut frame = map.to_be_bytes().to_vec();
    frame.extend([0xa5; 8]);
    a.runtime.memory.write(GuestPtr(stack), &frame).unwrap();
    a.runtime.cpu = Cpu::new(arch, 0x50000000, stack);
    let Cpu::M68k(c) = &mut a.runtime.cpu else {
        panic!()
    };
    c.dar[0] = 0x4000f;
    assert_eq!(a.runtime.advance(1).unwrap(), Stop::Budget);
    let Cpu::M68k(c) = &a.runtime.cpu else {
        panic!()
    };
    assert_eq!(c.dar[15], stack + 4);
    assert_eq!(c.dar[0], 0x4000f);
    assert_eq!(
        a.runtime.memory.read(GuestPtr(stack + 8), 4).unwrap(),
        [0xa5; 4]
    );
    a.runtime.memory.u32(GuestPtr(stack + 4)).unwrap()
}
#[test]
fn base_address_resolves_current_storage_without_locking_and_returns_nil_after_purge() {
    let mut a = app();
    let _seed = handle(&mut a, 1);
    let gap = a
        .runtime
        .process
        .new_pointer(&mut a.runtime.memory, 512, false)
        .unwrap();
    let image = handle(&mut a, 4);
    let map = pm(&mut a, image);
    let old = a.runtime.memory.u32(GuestPtr(image)).unwrap();
    assert_eq!(address(&mut a, Architecture::M68k, map), old);
    assert_eq!(base(&a, map), image);
    a.runtime.process.dispose_pointer(gap);
    a.runtime
        .process
        .compact_memory(&mut a.runtime.memory)
        .unwrap();
    let moved = a.runtime.memory.u32(GuestPtr(image)).unwrap();
    assert_ne!(old, moved);
    assert_eq!(address(&mut a, Architecture::PowerPc, map), moved);
    assert!(call(&mut a, Architecture::M68k, map, true));
    assert_eq!(address(&mut a, Architecture::PowerPc, map), moved);
    call(&mut a, Architecture::PowerPc, map, false);
    a.runtime
        .process
        .set_handle_state(&a.runtime.memory, image, 0x40)
        .unwrap();
    a.runtime
        .process
        .purge_and_compact_memory(&mut a.runtime.memory)
        .unwrap();
    for arch in [Architecture::M68k, Architecture::PowerPc] {
        assert_eq!(address(&mut a, arch, map), 0);
    }
    let fixed = a
        .runtime
        .process
        .new_pointer(&mut a.runtime.memory, 4, true)
        .unwrap();
    let map = pm(&mut a, fixed);
    for arch in [Architecture::M68k, Architecture::PowerPc] {
        assert_eq!(address(&mut a, arch, map), fixed);
    }
}
