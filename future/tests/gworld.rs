mod support;
use systemless_future::{
    cpu::{Architecture, Cpu},
    devices,
    graphics::Graphics,
    load::{LoadedApplication, STACK_BASE},
    memory::{GuestPtr, Permissions},
    runtime::Stop,
};
const DATA: u32 = 0x50000000;
const STACK: u32 = STACK_BASE + 0x7000;
fn app() -> LoadedApplication {
    let f = support::pef_program(
        &[0x4e800020],
        &[
            b"InitGraf",
            b"OpenPort",
            b"GetGDevice",
            b"NewGWorld",
            b"DisposeGWorld",
            b"GetGWorld",
            b"SetGWorld",
            b"GetGWorldPixMap",
            b"GetGWorldDevice",
            b"GetPixBaseAddr",
            b"LockPixels",
            b"UnlockPixels",
            b"PaintRect",
        ],
        64,
    );
    let mut a = LoadedApplication::pef(&f.bytes, None).unwrap();
    a.runtime
        .memory
        .map(GuestPtr(DATA), vec![0; 512], Permissions::RW)
        .unwrap();
    a.runtime
        .memory
        .map(GuestPtr(DATA + 0x1000), vec![0xab, 0x1d], Permissions::RX)
        .unwrap();
    ppc(&mut a, 0, &[0x600]).unwrap();
    ppc(&mut a, 1, &[DATA + 128]).unwrap();
    ppc(&mut a, 2, &[]).unwrap();
    rect(&mut a, [3, 5, 20, 42]);
    a
}
fn rect(a: &mut LoadedApplication, values: [i16; 4]) {
    let bytes: Vec<_> = values.into_iter().flat_map(i16::to_be_bytes).collect();
    a.runtime.memory.write(GuestPtr(DATA + 16), &bytes).unwrap();
}
fn ppc(a: &mut LoadedApplication, index: u32, args: &[u32]) -> systemless_future::Result<u32> {
    let tv = a
        .runtime
        .memory
        .u32(GuestPtr(a.info.toc_or_a5 + 8 + index * 4))
        .unwrap();
    let pc = a.runtime.memory.u32(GuestPtr(tv)).unwrap();
    a.runtime.cpu = Cpu::new(Architecture::PowerPc, pc, STACK);
    let Cpu::PowerPc(c) = &mut a.runtime.cpu else {
        unreachable!()
    };
    c.gpr[15] = 0xabcdef01;
    for (i, value) in args.iter().enumerate() {
        c.gpr[3 + i] = *value;
    }
    assert_eq!(a.runtime.advance(1)?, Stop::Budget);
    let Cpu::PowerPc(c) = &a.runtime.cpu else {
        unreachable!()
    };
    assert_eq!(c.gpr[15], 0xabcdef01);
    if matches!(index, 4..=6 | 11) {
        assert_eq!(c.gpr[3], args[0]);
    }
    Ok(c.gpr[3])
}
fn native(
    a: &mut LoadedApplication,
    selector: u32,
    args: &[u8],
    result: usize,
) -> systemless_future::Result<u32> {
    let mut frame = args.to_vec();
    frame.extend([0xa5; 8]);
    a.runtime.memory.write(GuestPtr(STACK), &frame).unwrap();
    a.runtime.cpu = Cpu::new(Architecture::M68k, DATA + 0x1000, STACK);
    let Cpu::M68k(c) = &mut a.runtime.cpu else {
        unreachable!()
    };
    for i in 0..15 {
        c.dar[i] = 0xabcd0000 + i as u32;
    }
    c.dar[0] = selector;
    assert_eq!(a.runtime.advance(1)?, Stop::Budget);
    let Cpu::M68k(c) = &a.runtime.cpu else {
        unreachable!()
    };
    assert_eq!(c.dar[0], selector);
    for i in 1..15 {
        assert_eq!(c.dar[i], 0xabcd0000 + i as u32);
    }
    assert_eq!(c.dar[15], STACK + args.len() as u32);
    let end = STACK + args.len() as u32;
    assert_eq!(
        a.runtime
            .memory
            .read(GuestPtr(end + result as u32), 8 - result)
            .unwrap(),
        vec![0xa5; 8 - result]
    );
    Ok(match result {
        1 => u32::from(a.runtime.memory.read(GuestPtr(end), 1).unwrap()[0]),
        2 => a.runtime.memory.u16(GuestPtr(end)).unwrap() as i16 as i32 as u32,
        4 => a.runtime.memory.u32(GuestPtr(end)).unwrap(),
        _ => 0,
    })
}
fn new(
    a: &mut LoadedApplication,
    arch: Architecture,
    depth: i16,
    table: u32,
    gd: u32,
    flags: u32,
) -> systemless_future::Result<u32> {
    if arch == Architecture::PowerPc {
        return ppc(
            a,
            3,
            &[
                DATA,
                0xabcd0000 | u32::from(depth as u16),
                DATA + 16,
                table,
                gd,
                flags,
            ],
        );
    }
    let mut args = Vec::new();
    for value in [flags, gd, table, DATA + 16] {
        args.extend(value.to_be_bytes());
    }
    args.extend(depth.to_be_bytes());
    args.extend(DATA.to_be_bytes());
    native(a, 0x160000, &args, 2)
}
fn one(a: &mut LoadedApplication, arch: Architecture, index: u32, value: u32) -> u32 {
    if arch == Architecture::PowerPc {
        return ppc(a, index, &[value]).unwrap();
    }
    let (selector, size) = match index {
        4 => (0x40004, 0),
        7 => (0x40017, 4),
        8 => (0x40012, 4),
        9 => (0x4000f, 4),
        10 => (0x40001, 1),
        11 => (0x40002, 0),
        _ => panic!(),
    };
    native(a, selector, &value.to_be_bytes(), size).unwrap()
}
fn set(a: &mut LoadedApplication, arch: Architecture, pointer: u32, gd: u32) {
    if arch == Architecture::PowerPc {
        ppc(a, 6, &[pointer, gd]).unwrap();
    } else {
        let args: Vec<_> = [gd, pointer]
            .into_iter()
            .flat_map(u32::to_be_bytes)
            .collect();
        native(a, 0x80006, &args, 0).unwrap();
    }
}
fn get(a: &mut LoadedApplication, arch: Architecture) -> (u32, u32) {
    if arch == Architecture::PowerPc {
        ppc(a, 5, &[DATA + 32, DATA + 36]).unwrap();
    } else {
        let args: Vec<_> = [DATA + 36, DATA + 32]
            .into_iter()
            .flat_map(u32::to_be_bytes)
            .collect();
        native(a, 0x80005, &args, 0).unwrap();
    }
    (
        a.runtime.memory.u32(GuestPtr(DATA + 32)).unwrap(),
        a.runtime.memory.u32(GuestPtr(DATA + 36)).unwrap(),
    )
}
fn data(a: &LoadedApplication, h: u32) -> u32 {
    a.runtime.memory.u32(GuestPtr(h)).unwrap()
}
#[test]
fn indexed_worlds_share_ownership_and_state_across_both_real_cpu_adapters() {
    for arch in [Architecture::M68k, Architecture::PowerPc] {
        for depth in [1, 2, 4, 8] {
            let mut a = app();
            let old = get(&mut a, arch);
            let free = a.runtime.process.free_memory();
            assert_eq!(new(&mut a, arch, depth, 0, 0xdeadbeef, 8).unwrap(), 0);
            let world = data(&a, DATA);
            assert_ne!(world, 0);
            assert_eq!(get(&mut a, arch), old);
            let other = if arch == Architecture::M68k {
                Architecture::PowerPc
            } else {
                Architecture::M68k
            };
            let pm = one(&mut a, other, 7, world);
            let gd = one(&mut a, arch, 8, world);
            assert_ne!(gd, old.1);
            let pm_data = data(&a, pm);
            let gd_data = data(&a, gd);
            let device_pm = data(&a, gd_data + 22);
            assert_ne!(pm, device_pm);
            assert_eq!(
                a.runtime.memory.read(GuestPtr(pm_data), 50).unwrap(),
                a.runtime
                    .memory
                    .read(GuestPtr(data(&a, device_pm)), 50)
                    .unwrap()
            );
            let row = (37u32 * depth as u32).div_ceil(32) * 4;
            assert_eq!(
                a.runtime.memory.u16(GuestPtr(pm_data + 4)).unwrap(),
                row as u16 | 0x8000
            );
            assert_eq!(
                a.runtime.memory.u16(GuestPtr(pm_data + 32)).unwrap(),
                depth as u16
            );
            let image = data(&a, pm_data);
            assert_eq!(
                a.runtime
                    .process
                    .handle_size(&a.runtime.memory, image)
                    .unwrap(),
                row * 17
            );
            let base = one(&mut a, other, 9, pm);
            assert_eq!(base, data(&a, image));
            assert_eq!(one(&mut a, arch, 10, pm), 1);
            assert_eq!(data(&a, pm_data), base);
            set(&mut a, other, world, 0xdeadbeef);
            assert_eq!(get(&mut a, arch), (world, gd));
            one(&mut a, other, 11, pm);
            assert_eq!(data(&a, pm_data), image);
            set(&mut a, arch, old.0, old.1);
            one(&mut a, other, 4, world);
            assert_eq!(a.runtime.process.free_memory(), free);
            assert_eq!(a.runtime.process.pointer_size(world) as i32, -111);
            assert_eq!(
                a.runtime
                    .process
                    .handle_size(&a.runtime.memory, image)
                    .unwrap(),
                0
            );
            assert_eq!(get(&mut a, arch), old);
        }
    }
}
#[test]
fn depth_zero_ignores_custom_table_and_device_and_does_not_own_screen_storage() {
    let mut a = app();
    let old = get(&mut a, Architecture::PowerPc);
    assert_eq!(
        new(&mut a, Architecture::M68k, 0, 0xdeadbeef, 0xdeadbeef, 2).unwrap(),
        0
    );
    let world = data(&a, DATA);
    let gd = one(&mut a, Architecture::PowerPc, 8, world);
    assert_eq!(gd, old.1);
    let pm = one(&mut a, Architecture::M68k, 7, world);
    let pm_data = data(&a, pm);
    let device_pm = data(&a, data(&a, gd) + 22);
    let screen_table = data(&a, data(&a, device_pm) + 42);
    let world_table = data(&a, pm_data + 42);
    assert_ne!(world_table, screen_table);
    assert_eq!(
        a.runtime
            .memory
            .read(GuestPtr(data(&a, world_table)), 8)
            .unwrap(),
        a.runtime
            .memory
            .read(GuestPtr(data(&a, screen_table)), 8)
            .unwrap()
    );
    one(&mut a, Architecture::PowerPc, 4, world);
    assert_eq!(get(&mut a, Architecture::M68k), old);
    assert_eq!(ppc(&mut a, 2, &[]).unwrap(), gd);
}
#[test]
fn custom_palette_is_copied_and_no_new_device_uses_supplied_device_format() {
    let mut a = app();
    let old = get(&mut a, Architecture::M68k);
    let ct = a
        .runtime
        .process
        .new_handle(&mut a.runtime.memory, 16, true)
        .unwrap();
    let at = data(&a, ct);
    a.runtime
        .memory
        .write(
            GuestPtr(at + 8),
            &[0, 0, 0x12, 0x34, 0x56, 0x78, 0x9a, 0xbc],
        )
        .unwrap();
    assert_eq!(new(&mut a, Architecture::PowerPc, 8, ct, 0, 0).unwrap(), 0);
    let world = data(&a, DATA);
    let pm = one(&mut a, Architecture::M68k, 7, world);
    let copy = data(&a, data(&a, pm) + 42);
    a.runtime
        .memory
        .write(GuestPtr(at + 10), &[0xff; 6])
        .unwrap();
    assert_eq!(
        a.runtime
            .memory
            .read(GuestPtr(data(&a, copy) + 10), 6)
            .unwrap(),
        [0x12, 0x34, 0x56, 0x78, 0x9a, 0xbc]
    );
    one(&mut a, Architecture::M68k, 4, world);
    assert_eq!(
        a.runtime
            .process
            .handle_size(&a.runtime.memory, ct)
            .unwrap(),
        16
    );
    assert_eq!(
        new(&mut a, Architecture::M68k, 8, 0xdeadbeef, old.1, 2).unwrap(),
        0
    );
    let world = data(&a, DATA);
    assert_eq!(one(&mut a, Architecture::PowerPc, 8, world), old.1);
    let pm = one(&mut a, Architecture::PowerPc, 7, world);
    assert_eq!(
        a.runtime.memory.u16(GuestPtr(data(&a, pm) + 32)).unwrap(),
        1
    );
    one(&mut a, Architecture::PowerPc, 4, world);
}
fn selection(a: &LoadedApplication) -> (u32, u32) {
    (
        data(a, a.runtime.process.ports.unwrap().current_slot.0),
        a.runtime.memory.u32(devices::CURRENT).unwrap(),
    )
}
#[test]
fn creation_rejects_invalid_inputs_before_allocating_or_selecting() {
    for arch in [Architecture::M68k, Architecture::PowerPc] {
        for case in 0..5 {
            // Runtime faults are terminal; every fault case uses a fresh process.
            let mut a = app();
            let old = get(&mut a, arch);
            let free = a.runtime.process.free_memory();
            match case {
                0 => assert_eq!(new(&mut a, arch, 3, 0, 0, 0).unwrap() as i32, -157),
                1 => assert!(new(&mut a, arch, 16, 0, 0, 0).is_err()),
                2 => assert!(new(&mut a, arch, 8, 0, 0, 1).is_err()),
                3 => assert!(new(&mut a, arch, 8, 0xdeadbeef, 0, 0).is_err()),
                _ => {
                    rect(&mut a, [0, 0, 0, 4]);
                    assert_eq!(new(&mut a, arch, 8, 0, 0, 0).unwrap() as i32, -50);
                }
            }
            assert_eq!(a.runtime.process.free_memory(), free);
            assert_eq!(selection(&a), old);
            assert_eq!(data(&a, DATA), 0);
        }
    }
}
#[test]
fn failed_creation_and_output_faults_preserve_heap_selection_and_output() {
    for output_fault in [true, false] {
        let mut a = app();
        let old = get(&mut a, Architecture::PowerPc);
        a.runtime
            .memory
            .write(GuestPtr(DATA), &0x12345678u32.to_be_bytes())
            .unwrap();
        let free = a.runtime.process.free_memory();
        let expected_free = if output_fault {
            assert!(ppc(&mut a, 3, &[DATA + 0x1000, 8, DATA + 16, 0, 0, 0]).is_err());
            free
        } else {
            // Enough room for the image and port, but not the complete graph.
            let filler = a
                .runtime
                .process
                .new_pointer(&mut a.runtime.memory, free - 1600, false)
                .unwrap();
            assert_ne!(filler, 0);
            assert_eq!(
                new(&mut a, Architecture::M68k, 8, 0, 0, 0).unwrap() as i32,
                -108
            );
            1600
        };
        assert_eq!(data(&a, DATA), 0x12345678);
        assert_eq!(a.runtime.process.free_memory(), expected_free);
        assert_eq!(selection(&a), old);
    }
}
#[test]
fn selection_output_fault_and_locked_disposal_are_atomic() {
    for case in 0..4 {
        let mut a = app();
        assert_eq!(new(&mut a, Architecture::PowerPc, 8, 0, 0, 0).unwrap(), 0);
        let world = data(&a, DATA);
        let pm = one(&mut a, Architecture::M68k, 7, world);
        if case == 2 {
            one(&mut a, Architecture::M68k, 10, pm);
        }
        if case == 3 {
            set(&mut a, Architecture::M68k, world, 0);
        }
        let old = selection(&a);
        let free = a.runtime.process.free_memory();
        let records = a
            .runtime
            .memory
            .read(GuestPtr(world), 108)
            .unwrap()
            .to_vec();
        a.runtime
            .memory
            .write(GuestPtr(DATA + 32), &0xabcdef01u32.to_be_bytes())
            .unwrap();
        match case {
            0 => assert!(ppc(&mut a, 6, &[DATA + 128, 0xdeadbeef]).is_err()),
            1 => assert!(ppc(&mut a, 5, &[DATA + 32, DATA + 0x1000]).is_err()),
            _ => assert!(ppc(&mut a, 4, &[world]).is_err()),
        }
        assert_eq!(data(&a, DATA + 32), 0xabcdef01);
        assert_eq!(selection(&a), old);
        assert_eq!(a.runtime.process.free_memory(), free);
        assert_eq!(
            a.runtime.memory.read(GuestPtr(world), 108).unwrap(),
            records
        );
        assert_ne!(data(&a, pm), 0);
    }
}
#[test]
fn offscreen_creation_does_not_advertise_incomplete_32_bit_quickdraw() {
    assert_eq!(
        systemless_future::traps::address(0xab03, true),
        systemless_future::traps::UNIMPLEMENTED
    );
}
#[test]
#[ignore = "requires a real wgpu adapter"]
fn locked_offscreen_pixels_mix_direct_guest_writes_and_shared_gpu_drawing() {
    let mut a = app();
    rect(&mut a, [0, 0, 4, 8]);
    assert_eq!(new(&mut a, Architecture::M68k, 8, 0, 0, 0).unwrap(), 0);
    let world = data(&a, DATA);
    let pm = one(&mut a, Architecture::PowerPc, 7, world);
    one(&mut a, Architecture::M68k, 10, pm);
    let base = one(&mut a, Architecture::PowerPc, 9, pm);
    set(&mut a, Architecture::PowerPc, world, 0);
    a.runtime.memory.write(GuestPtr(base), &[7; 32]).unwrap();
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
    let gpu = pollster::block_on(Graphics::new(
        &instance,
        None,
        512,
        342,
        systemless_future::quickdraw::SCREEN,
    ))
    .unwrap();
    a.runtime.graphics = Some(gpu);
    rect(&mut a, [1, 2, 3, 6]);
    ppc(&mut a, 12, &[DATA + 16]).unwrap();
    let mut expected = [7; 32];
    expected[10..14].fill(255);
    expected[18..22].fill(255);
    assert_eq!(a.runtime.memory.read(GuestPtr(base), 32).unwrap(), expected);
    assert_eq!(
        a.runtime.memory.u32(devices::CURRENT).unwrap(),
        one(&mut a, Architecture::M68k, 8, world)
    );
    // The GPU draw preserves the lock and metadata required by direct rendering.
    assert_eq!(data(&a, data(&a, pm)), base);
    assert_eq!(
        a.runtime.memory.u16(GuestPtr(data(&a, pm) + 4)).unwrap(),
        0x8008
    );
}

#[test]
fn native_result_preflight_fault_preserves_argument_stack_and_heap() {
    let mut a = app();
    let free = a.runtime.process.free_memory();
    let at = 0x52000000;
    let mut args = Vec::new();
    for v in [0u32, 0, 0, DATA + 16] {
        args.extend(v.to_be_bytes());
    }
    args.extend(8i16.to_be_bytes());
    args.extend(DATA.to_be_bytes());
    a.runtime
        .memory
        .map(GuestPtr(at), args, Permissions::RW)
        .unwrap();
    // The 22-byte argument frame is readable, but its Pascal result is unmapped.
    a.runtime.cpu = Cpu::new(Architecture::M68k, DATA + 0x1000, at);
    let Cpu::M68k(c) = &mut a.runtime.cpu else {
        unreachable!()
    };
    c.dar[0] = 0x160000;
    assert!(a.runtime.advance(1).is_err());
    let Cpu::M68k(c) = &a.runtime.cpu else {
        unreachable!()
    };
    assert_eq!(c.dar[15], at);
    assert_eq!(a.runtime.process.free_memory(), free);
    assert_eq!(data(&a, DATA), 0);
}
#[test]
fn changed_supporting_handle_is_rejected_before_partial_world_disposal() {
    let mut a = app();
    assert_eq!(new(&mut a, Architecture::M68k, 8, 0, 0, 0).unwrap(), 0);
    let world = data(&a, DATA);
    let pm = one(&mut a, Architecture::PowerPc, 7, world);
    let gd = one(&mut a, Architecture::M68k, 8, world);
    let gd_data = data(&a, gd);
    let free = a.runtime.process.free_memory();
    a.runtime
        .memory
        .write(GuestPtr(gd_data + 6), &pm.to_be_bytes())
        .unwrap();
    assert!(ppc(&mut a, 4, &[world]).is_err());
    assert_eq!(data(&a, world + 2), pm);
    assert_ne!(data(&a, pm), 0);
    assert_ne!(data(&a, gd), 0);
    assert_eq!(a.runtime.process.pointer_size(world), 108);
    assert_eq!(a.runtime.process.free_memory(), free);
}

#[test]
fn every_partial_allocation_stage_rolls_back_without_publishing_a_world() {
    // Image, port/device, palette, inverse-table and port-attachment failures.
    for available in [100, 800, 1600, 3200, 3600] {
        let mut a = app();
        let old = get(&mut a, Architecture::M68k);
        let free = a.runtime.process.free_memory();
        let filler = a
            .runtime
            .process
            .new_pointer(&mut a.runtime.memory, free - available, false)
            .unwrap();
        assert_ne!(filler, 0);
        assert_eq!(a.runtime.process.free_memory(), available);
        assert_eq!(
            new(&mut a, Architecture::PowerPc, 8, 0, 0, 0).unwrap() as i32,
            -108,
            "available={available}"
        );
        assert_eq!(
            a.runtime.process.free_memory(),
            available,
            "available={available}"
        );
        assert_eq!(get(&mut a, Architecture::M68k), old);
        assert_eq!(data(&a, DATA), 0);
        a.runtime.process.dispose_pointer(filler);
    }
}
#[test]
fn world_handles_follow_compaction_while_locked_pixels_remain_pinned() {
    let mut a = app();
    let gap = a
        .runtime
        .process
        .new_pointer(&mut a.runtime.memory, 4096, false)
        .unwrap();
    assert_eq!(new(&mut a, Architecture::M68k, 8, 0, 0, 0).unwrap(), 0);
    let world = data(&a, DATA);
    let pm = one(&mut a, Architecture::PowerPc, 7, world);
    let before = one(&mut a, Architecture::M68k, 9, pm);
    a.runtime
        .memory
        .write(GuestPtr(before), &[0x5a; 16])
        .unwrap();
    a.runtime.process.dispose_pointer(gap);
    a.runtime
        .process
        .compact_memory(&mut a.runtime.memory)
        .unwrap();
    assert_eq!(one(&mut a, Architecture::M68k, 7, world), pm);
    let after = one(&mut a, Architecture::PowerPc, 9, pm);
    assert_ne!(before, after);
    assert_eq!(
        a.runtime.memory.read(GuestPtr(after), 16).unwrap(),
        [0x5a; 16]
    );
    one(&mut a, Architecture::M68k, 10, pm);
    a.runtime
        .process
        .compact_memory(&mut a.runtime.memory)
        .unwrap();
    assert_eq!(one(&mut a, Architecture::PowerPc, 9, pm), after);
    one(&mut a, Architecture::PowerPc, 11, pm);
    one(&mut a, Architecture::M68k, 4, world);
}
