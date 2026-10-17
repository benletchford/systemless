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
            b"UpdateGWorld",
            b"QDError",
            b"GetPixelsState",
            b"SetPixelsState",
            b"AllowPurgePixels",
            b"NoPurgePixels",
            b"HGetState",
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
        .map(
            GuestPtr(DATA + 0x1000),
            vec![0xab, 0x1d, 0xa0, 0x69, 0xa8, 0xa2],
            Permissions::RX,
        )
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
    if matches!(index, 4..=6 | 11 | 16..=18) {
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
                1 => assert_eq!(new(&mut a, arch, 24, 0, 0, 0).unwrap() as i32, -157),
                2 => assert!(new(&mut a, arch, 8, 0, 0, 4).is_err()),
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

fn update(
    a: &mut LoadedApplication,
    arch: Architecture,
    depth: i16,
    table: u32,
    gd: u32,
    flags: u32,
) -> u32 {
    if arch == Architecture::PowerPc {
        ppc(
            a,
            13,
            &[
                DATA,
                0xabcd0000 | u32::from(depth as u16),
                DATA + 16,
                table,
                gd,
                flags,
            ],
        )
        .unwrap()
    } else {
        let mut args = Vec::new();
        for v in [flags, gd, table, DATA + 16] {
            args.extend(v.to_be_bytes());
        }
        args.extend(depth.to_be_bytes());
        args.extend(DATA.to_be_bytes());
        native(a, 0x160003, &args, 4).unwrap()
    }
}
#[test]
fn marathon_size_shrink_updates_both_pixmaps_image_region_and_port_on_both_cpus() {
    for arch in [Architecture::M68k, Architecture::PowerPc] {
        let mut a = app();
        let old = get(&mut a, arch);
        rect(&mut a, [0, 0, 272, 448]);
        assert_eq!(new(&mut a, arch, 8, 0, 0, 0).unwrap(), 0);
        let world = data(&a, DATA);
        let pm = one(&mut a, arch, 7, world);
        let gd = one(&mut a, arch, 8, world);
        let image = data(&a, data(&a, pm));
        set(&mut a, arch, world, 0);
        rect(&mut a, [0, 0, 136, 224]);
        assert_eq!(update(&mut a, arch, 8, 0, 0, 0), (1 << 19) | (1 << 20));
        assert_eq!(get(&mut a, arch), (world, gd));
        assert_eq!(data(&a, DATA), world);
        let map = data(&a, pm);
        let device_map = data(&a, data(&a, gd) + 22);
        assert_eq!(
            a.runtime.memory.read(GuestPtr(map), 50).unwrap(),
            a.runtime
                .memory
                .read(GuestPtr(data(&a, device_map)), 50)
                .unwrap()
        );
        assert_eq!(data(&a, map), image);
        assert_eq!(a.runtime.memory.u16(GuestPtr(map + 4)).unwrap(), 0x80e0);
        assert_eq!(
            a.runtime
                .process
                .handle_size(&a.runtime.memory, image)
                .unwrap(),
            30464
        );
        for at in [
            map + 6,
            world + 16,
            data(&a, data(&a, world + 24)) + 2,
            data(&a, gd) + 34,
        ] {
            assert_eq!(
                a.runtime.memory.read(GuestPtr(at), 8).unwrap(),
                [0, 0, 0, 0, 0, 136, 0, 224]
            );
        }
        assert_eq!(ppc(&mut a, 14, &[]).unwrap(), 0);
        set(&mut a, arch, old.0, old.1);
        one(&mut a, arch, 4, world);
    }
}
#[test]
fn failed_updates_publish_qderror_without_changing_world_or_allocator() {
    for arch in [Architecture::M68k, Architecture::PowerPc] {
        for case in 0..4 {
            let mut a = app();
            assert_eq!(new(&mut a, arch, 8, 0, 0, 0).unwrap(), 0);
            let world = data(&a, DATA);
            let pm = one(&mut a, arch, 7, world);
            if case == 2 {
                one(&mut a, arch, 10, pm);
            }
            let port = a
                .runtime
                .memory
                .read(GuestPtr(world), 108)
                .unwrap()
                .to_vec();
            let map = a
                .runtime
                .memory
                .read(GuestPtr(data(&a, pm)), 50)
                .unwrap()
                .to_vec();
            let free = a.runtime.process.free_memory();
            let selected = selection(&a);
            let depth = if case == 1 { 3 } else { 8 };
            rect(
                &mut a,
                if case == 0 {
                    [0, 0, 0, 1]
                } else if case == 3 {
                    [0, 0, 2000, 2000]
                } else {
                    [0, 0, 20, 40]
                },
            );
            if case == 3 {
                let fill = a
                    .runtime
                    .process
                    .new_pointer(&mut a.runtime.memory, free - 100, false)
                    .unwrap();
                assert_ne!(fill, 0);
            }
            let before = a.runtime.process.free_memory();
            assert_eq!(update(&mut a, arch, depth, 0, 0, 0), 1 << 31);
            assert_eq!(
                ppc(&mut a, 14, &[]).unwrap() as i32,
                match case {
                    0 => -50,
                    1 => -157,
                    _ => -108,
                }
            );
            assert_eq!(a.runtime.process.free_memory(), before);
            assert_eq!(selection(&a), selected);
            assert_eq!(a.runtime.memory.read(GuestPtr(world), 108).unwrap(), port);
            assert_eq!(
                a.runtime.memory.read(GuestPtr(data(&a, pm)), 50).unwrap(),
                map
            );
        }
    }
}
#[test]
fn depth_zero_update_copies_device_without_mutating_screen_and_retains_owned_lifetime() {
    let mut a = app();
    let old = get(&mut a, Architecture::PowerPc);
    let free = a.runtime.process.free_memory();
    let screen = a
        .runtime
        .memory
        .read(GuestPtr(data(&a, old.1)), 62)
        .unwrap()
        .to_vec();
    assert_eq!(new(&mut a, Architecture::M68k, 0, 0, 0, 0).unwrap(), 0);
    let world = data(&a, DATA);
    set(&mut a, Architecture::PowerPc, world, 0);
    rect(&mut a, [0, 0, 10, 32]);
    update(&mut a, Architecture::M68k, 0, 0xdeadbeef, 0, 0);
    let gd = one(&mut a, Architecture::PowerPc, 8, world);
    assert_ne!(gd, old.1);
    assert_eq!(selection(&a), (world, gd));
    assert_eq!(
        a.runtime
            .memory
            .read(GuestPtr(data(&a, old.1)), 62)
            .unwrap(),
        screen
    );
    set(&mut a, Architecture::M68k, old.0, old.1);
    one(&mut a, Architecture::PowerPc, 4, world);
    assert_eq!(a.runtime.process.free_memory(), free);
}
#[test]
#[ignore = "requires a real wgpu adapter"]
fn gpu_updates_clip_stretch_and_convert_indexed_depth_with_live_port_colors() {
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
    let mut gpu = Some(
        pollster::block_on(Graphics::new(
            &instance,
            None,
            512,
            342,
            systemless_future::quickdraw::SCREEN,
        ))
        .unwrap(),
    );
    for arch in [Architecture::M68k, Architecture::PowerPc] {
        for mode in 0..6 {
            let flags = match mode {
                0 | 5 => 1 << 28,
                1 => 1 << 29,
                4 => (1 << 28) | (1 << 30),
                _ => 0,
            };
            let mut a = app();
            rect(&mut a, [0, 0, 2, 4]);
            assert_eq!(new(&mut a, arch, 8, 0, 0, 0).unwrap(), 0);
            let world = data(&a, DATA);
            let pm = one(&mut a, arch, 7, world);
            let base = one(&mut a, arch, 9, pm);
            a.runtime
                .memory
                .write(GuestPtr(base), &[0, 255, 0, 255, 255, 0, 255, 0])
                .unwrap();
            let table = if mode == 5 {
                let table = a
                    .runtime
                    .process
                    .new_handle(&mut a.runtime.memory, 24, true)
                    .unwrap();
                let at = data(&a, table);
                let mut colors = vec![0; 24];
                support::w32(&mut colors, 0, 0x1234);
                support::w16(&mut colors, 6, 1);
                support::w16(&mut colors, 10, 40000);
                support::w16(&mut colors, 16, 1);
                support::w16(&mut colors, 18, 60000);
                a.runtime.memory.write(GuestPtr(at), &colors).unwrap();
                a.runtime
                    .memory
                    .write(GuestPtr(base), &[72, 0, 255, 72, 0, 72, 255, 0])
                    .unwrap();
                table
            } else {
                0
            };
            a.runtime.graphics = gpu.take();
            rect(
                &mut a,
                match mode {
                    1 => [0, 0, 4, 8],
                    3 => [5, 7, 7, 11],
                    _ => [0, 0, 2, 2],
                },
            );
            let depth = if mode == 2 { 4 } else { 8 };
            let result = update(&mut a, arch, depth, table, 0, flags);
            assert_eq!(result >> 31, 0);
            let base = one(&mut a, arch, 9, pm);
            match mode {
                0 | 4 => {
                    assert_eq!(a.runtime.memory.read(GuestPtr(base), 2).unwrap(), [0, 255]);
                    assert_eq!(
                        a.runtime.memory.read(GuestPtr(base + 4), 2).unwrap(),
                        [255, 0]
                    );
                    assert_ne!(result & (1 << 28), 0);
                }
                1 => {
                    assert_eq!(
                        a.runtime.memory.read(GuestPtr(base), 8).unwrap(),
                        [0, 0, 255, 255, 0, 0, 255, 255]
                    );
                    assert_eq!(
                        a.runtime.memory.read(GuestPtr(base + 16), 8).unwrap(),
                        [255, 255, 0, 0, 255, 255, 0, 0]
                    );
                    assert_ne!(result & (1 << 29), 0);
                }
                5 => {
                    assert_eq!(a.runtime.memory.read(GuestPtr(base), 2).unwrap(), [0, 1]);
                    assert_eq!(
                        a.runtime.memory.read(GuestPtr(base + 4), 2).unwrap(),
                        [1, 0]
                    );
                    assert_eq!(data(&a, world + 80), 0);
                    assert_eq!(data(&a, world + 84), 1);
                    assert_ne!(result & (1 << 16), 0);
                }
                3 => {
                    assert_eq!(
                        a.runtime.memory.read(GuestPtr(base), 8).unwrap(),
                        [0, 255, 0, 255, 255, 0, 255, 0]
                    );
                    assert_eq!(result, 1 << 18);
                }
                _ => {
                    assert_eq!(a.runtime.memory.read(GuestPtr(base), 1).unwrap(), [0x0f]);
                    assert_eq!(
                        a.runtime.memory.read(GuestPtr(base + 4), 1).unwrap(),
                        [0xf0]
                    );
                    assert_eq!(data(&a, world + 80), 15);
                    assert_eq!(data(&a, world + 84), 0);
                    assert_ne!(result & (1 << 17), 0);
                }
            }
            if mode == 4 {
                assert_ne!(result & (1 << 30), 0);
            }
            gpu = a.runtime.graphics.take();
        }
    }
}

#[test]
fn update_parameter_aliases_device_precedence_and_borrowed_noop_preserve_pixels() {
    let mut a = app();
    assert_eq!(new(&mut a, Architecture::M68k, 8, 0, 0, 0).unwrap(), 0);
    let world = data(&a, DATA);
    let pm = one(&mut a, Architecture::PowerPc, 7, world);
    let gd = one(&mut a, Architecture::M68k, 8, world);
    let map = data(&a, pm);
    let free = a.runtime.process.free_memory();
    assert_eq!(
        ppc(&mut a, 13, &[DATA, 0xdeadffff, map + 6, 0xdeadbeef, gd, 0]).unwrap(),
        0
    );
    assert_eq!(a.runtime.process.free_memory(), free);
    assert_eq!(data(&a, DATA), world);
    one(&mut a, Architecture::PowerPc, 4, world);
    assert_eq!(new(&mut a, Architecture::PowerPc, 0, 0, 0, 0).unwrap(), 0);
    let world = data(&a, DATA);
    let pm = one(&mut a, Architecture::M68k, 7, world);
    let base = one(&mut a, Architecture::PowerPc, 9, pm);
    a.runtime.memory.write(GuestPtr(base), &[0x5a; 8]).unwrap();
    assert_eq!(update(&mut a, Architecture::M68k, 0, 0, 0, 0), 0);
    let base = one(&mut a, Architecture::PowerPc, 9, pm);
    assert_eq!(a.runtime.memory.read(GuestPtr(base), 8).unwrap(), [0x5a; 8]);
    one(&mut a, Architecture::M68k, 4, world);
}

#[test]
fn purged_world_images_are_reallocated_and_reported_lost_even_for_unchanged_bounds() {
    for arch in [Architecture::M68k, Architecture::PowerPc] {
        let mut a = app();
        assert_eq!(new(&mut a, arch, 8, 0, 0, 0).unwrap(), 0);
        let world = data(&a, DATA);
        let pm = one(&mut a, arch, 7, world);
        let image = data(&a, data(&a, pm));
        a.runtime
            .process
            .set_handle_state(&a.runtime.memory, image, 0x40)
            .unwrap();
        a.runtime
            .process
            .purge_and_compact_memory(&mut a.runtime.memory)
            .unwrap();
        assert_eq!(one(&mut a, arch, 9, pm), 0);
        assert_eq!(update(&mut a, arch, 8, 0, 0, 0), 1 << 20);
        assert_ne!(one(&mut a, arch, 9, pm), 0);
        assert_eq!(data(&a, data(&a, pm)), image);
        assert_eq!(
            a.runtime
                .process
                .handle_size(&a.runtime.memory, image)
                .unwrap(),
            680
        );
        assert_eq!(ppc(&mut a, 14, &[]).unwrap(), 0);
        one(&mut a, arch, 4, world);
    }
}

fn pixel_state(
    a: &mut LoadedApplication,
    arch: Architecture,
    index: u32,
    pm: u32,
    state: u32,
) -> systemless_future::Result<u32> {
    if arch == Architecture::PowerPc {
        let result = ppc(a, index, &[pm, state]);
        if result.is_ok() {
            let Cpu::PowerPc(c) = &a.runtime.cpu else {
                unreachable!()
            };
            assert_eq!(c.gpr[4], state);
        }
        return result;
    }
    let selector = match index {
        15 => 0x4000d,
        16 => 0x8000e,
        17 => 0x4000b,
        18 => 0x4000c,
        _ => panic!(),
    };
    let mut args = Vec::new();
    if index == 16 {
        args.extend(state.to_be_bytes());
    }
    args.extend(pm.to_be_bytes());
    native(a, selector, &args, if index == 15 { 4 } else { 0 })
}
fn handle_flags(a: &mut LoadedApplication, arch: Architecture, image: u32) -> u8 {
    if arch == Architecture::PowerPc {
        return ppc(a, 19, &[image]).unwrap() as u8;
    }
    a.runtime.cpu = Cpu::new(Architecture::M68k, DATA + 0x1002, STACK);
    let Cpu::M68k(c) = &mut a.runtime.cpu else {
        unreachable!()
    };
    c.dar[8] = image;
    c.dar[0] = 0xaabbcc00;
    assert_eq!(a.runtime.advance(1).unwrap(), Stop::Budget);
    let Cpu::M68k(c) = &a.runtime.cpu else {
        unreachable!()
    };
    assert_eq!(c.dar[8], image);
    assert_eq!(c.dar[15], STACK);
    c.dar[0] as u8
}
#[test]
fn pixel_state_save_restore_is_shared_by_both_cpus_and_preserves_other_handle_flags() {
    for arch in [Architecture::M68k, Architecture::PowerPc] {
        let mut a = app();
        assert_eq!(new(&mut a, arch, 8, 0, 0, 1).unwrap(), 0);
        let world = data(&a, DATA);
        let pm = one(&mut a, arch, 7, world);
        let image = data(&a, data(&a, pm));
        a.runtime
            .process
            .set_handle_attribute(&a.runtime.memory, image, true, true)
            .unwrap();
        assert_eq!(pixel_state(&mut a, arch, 15, pm, 0).unwrap(), 0x40);
        let other = if arch == Architecture::M68k {
            Architecture::PowerPc
        } else {
            Architecture::M68k
        };
        let free = a.runtime.process.free_memory();
        let selected = selection(&a);
        for state in [0, 0x40, 0x80, 0xc0, 0, 0xc0, 0x40] {
            a.runtime.process.mem_error = -42;
            a.runtime.process.res_error = -43;
            pixel_state(&mut a, arch, 16, pm, state).unwrap();
            assert_eq!(pixel_state(&mut a, other, 15, pm, 0).unwrap(), state);
            assert_eq!(
                (a.runtime.process.mem_error, a.runtime.process.res_error),
                (-42, -43)
            );
            assert_eq!(handle_flags(&mut a, other, image), state as u8 | 0x20);
            let stored = data(&a, data(&a, pm));
            assert_eq!(
                stored,
                if state & 0x80 != 0 {
                    data(&a, image)
                } else {
                    image
                }
            );
            assert_eq!(a.runtime.process.free_memory(), free);
            assert_eq!(selection(&a), selected);
        }
    }
}
#[test]
fn public_pixel_purge_policy_controls_loss_and_update_recovery_without_losing_handle_identity() {
    for arch in [Architecture::M68k, Architecture::PowerPc] {
        let mut a = app();
        assert_eq!(new(&mut a, arch, 8, 0, 0, 1).unwrap(), 0);
        let world = data(&a, DATA);
        let pm = one(&mut a, arch, 7, world);
        let image = data(&a, data(&a, pm));
        pixel_state(&mut a, arch, 16, pm, 0xc0).unwrap();
        a.runtime
            .process
            .purge_and_compact_memory(&mut a.runtime.memory)
            .unwrap();
        assert_ne!(one(&mut a, arch, 9, pm), 0);
        pixel_state(&mut a, arch, 18, pm, 0).unwrap();
        assert_eq!(pixel_state(&mut a, arch, 15, pm, 0).unwrap(), 0x80);
        pixel_state(&mut a, arch, 16, pm, 0).unwrap();
        pixel_state(&mut a, arch, 17, pm, 0).unwrap();
        a.runtime
            .process
            .purge_and_compact_memory(&mut a.runtime.memory)
            .unwrap();
        assert_eq!(one(&mut a, arch, 9, pm), 0);
        assert_eq!(one(&mut a, arch, 10, pm), 0);
        assert_eq!(pixel_state(&mut a, arch, 15, pm, 0).unwrap(), 0x40);
        pixel_state(&mut a, arch, 18, pm, 0).unwrap();
        assert_eq!(pixel_state(&mut a, arch, 15, pm, 0).unwrap(), 0);
        assert_eq!(update(&mut a, arch, 8, 0, 0, 0), 1 << 20);
        assert_eq!(pixel_state(&mut a, arch, 15, pm, 0).unwrap(), 0);
        assert_eq!(data(&a, data(&a, pm)), image);
        assert_ne!(data(&a, image), 0);
        one(&mut a, arch, 4, world);
    }
}
#[test]
fn state_restoration_does_not_release_another_pixmaps_image_pin() {
    let mut a = app();
    assert_eq!(new(&mut a, Architecture::PowerPc, 8, 0, 0, 0).unwrap(), 0);
    let world = data(&a, DATA);
    let pm = one(&mut a, Architecture::M68k, 7, world);
    let gd = one(&mut a, Architecture::PowerPc, 8, world);
    let alias = data(&a, data(&a, gd) + 22);
    pixel_state(&mut a, Architecture::M68k, 16, pm, 0x80).unwrap();
    pixel_state(&mut a, Architecture::PowerPc, 16, alias, 0x80).unwrap();
    pixel_state(&mut a, Architecture::M68k, 16, pm, 0x40).unwrap();
    assert_eq!(
        pixel_state(&mut a, Architecture::PowerPc, 15, pm, 0).unwrap(),
        0xc0
    );
    let before = one(&mut a, Architecture::M68k, 9, alias);
    a.runtime
        .process
        .purge_and_compact_memory(&mut a.runtime.memory)
        .unwrap();
    assert_eq!(one(&mut a, Architecture::PowerPc, 9, pm), before);
    pixel_state(&mut a, Architecture::PowerPc, 16, alias, 0x40).unwrap();
    assert_eq!(
        pixel_state(&mut a, Architecture::M68k, 15, pm, 0).unwrap(),
        0x40
    );
    a.runtime
        .process
        .purge_and_compact_memory(&mut a.runtime.memory)
        .unwrap();
    assert_eq!(one(&mut a, Architecture::M68k, 9, pm), 0);
}
#[test]
fn invalid_pixel_state_and_locked_purge_requests_do_not_partially_change_storage() {
    for arch in [Architecture::M68k, Architecture::PowerPc] {
        for case in 0..3 {
            let mut a = app();
            assert_eq!(new(&mut a, arch, 8, 0, 0, 1).unwrap(), 0);
            let world = data(&a, DATA);
            let pm = one(&mut a, arch, 7, world);
            let image = data(&a, data(&a, pm));
            if case == 0 {
                pixel_state(&mut a, arch, 16, pm, 0x80).unwrap();
            }
            if case == 2 {
                a.runtime
                    .process
                    .purge_and_compact_memory(&mut a.runtime.memory)
                    .unwrap();
            }
            let map = a
                .runtime
                .memory
                .read(GuestPtr(data(&a, pm)), 50)
                .unwrap()
                .to_vec();
            let master = data(&a, image);
            let free = a.runtime.process.free_memory();
            a.runtime.process.mem_error = -42;
            a.runtime.process.res_error = -43;
            let result = if case == 0 {
                pixel_state(&mut a, arch, 17, pm, 0)
            } else {
                pixel_state(&mut a, arch, 16, pm, if case == 1 { 1 } else { 0x80 })
            };
            assert!(result.is_err());
            assert_eq!(
                (a.runtime.process.mem_error, a.runtime.process.res_error),
                (-42, -43)
            );
            assert_eq!(
                a.runtime.memory.read(GuestPtr(data(&a, pm)), 50).unwrap(),
                map
            );
            assert_eq!(data(&a, image), master);
            assert_eq!(a.runtime.process.free_memory(), free);
        }
    }
}
#[test]
fn native_pixel_state_result_and_argument_frames_are_preflighted_before_stack_changes() {
    for get in [true, false] {
        let mut a = app();
        assert_eq!(new(&mut a, Architecture::PowerPc, 8, 0, 0, 0).unwrap(), 0);
        let world = data(&a, DATA);
        let pm = one(&mut a, Architecture::PowerPc, 7, world);
        let frame = if get {
            let mut x = pm.to_be_bytes().to_vec();
            x.extend([0x5a; 2]);
            x
        } else {
            0xc0u32.to_be_bytes().to_vec()
        };
        let sp = 0x52000000;
        a.runtime
            .memory
            .map(GuestPtr(sp), frame, Permissions::RW)
            .unwrap();
        let free = a.runtime.process.free_memory();
        let old = a
            .runtime
            .memory
            .read(GuestPtr(data(&a, pm)), 50)
            .unwrap()
            .to_vec();
        a.runtime.cpu = Cpu::new(Architecture::M68k, DATA + 0x1000, sp);
        let Cpu::M68k(c) = &mut a.runtime.cpu else {
            unreachable!()
        };
        c.dar[0] = if get { 0x4000d } else { 0x8000e };
        assert!(a.runtime.advance(1).is_err());
        let Cpu::M68k(c) = &a.runtime.cpu else {
            unreachable!()
        };
        assert_eq!(c.dar[15], sp);
        assert_eq!(
            a.runtime.memory.read(GuestPtr(data(&a, pm)), 50).unwrap(),
            old
        );
        assert_eq!(a.runtime.process.free_memory(), free);
    }
}
#[test]
fn pixel_state_lock_reporting_is_coherent_with_explicit_handle_locks() {
    let mut a = app();
    assert_eq!(new(&mut a, Architecture::M68k, 8, 0, 0, 0).unwrap(), 0);
    let world = data(&a, DATA);
    let pm = one(&mut a, Architecture::PowerPc, 7, world);
    let image = data(&a, data(&a, pm));
    a.runtime
        .process
        .set_handle_lock(&a.runtime.memory, image, true)
        .unwrap();
    assert_eq!(
        pixel_state(&mut a, Architecture::M68k, 15, pm, 0).unwrap(),
        0x80
    );
    pixel_state(&mut a, Architecture::PowerPc, 16, pm, 0).unwrap();
    assert_eq!(handle_flags(&mut a, Architecture::M68k, image), 0);
    pixel_state(&mut a, Architecture::M68k, 16, pm, 0x80).unwrap();
    assert_eq!(handle_flags(&mut a, Architecture::PowerPc, image), 0x80);
    a.runtime
        .process
        .set_handle_lock(&a.runtime.memory, image, false)
        .unwrap();
    assert_eq!(
        pixel_state(&mut a, Architecture::PowerPc, 15, pm, 0).unwrap(),
        0x80
    );
    pixel_state(&mut a, Architecture::PowerPc, 16, pm, 0).unwrap();
    assert_eq!(handle_flags(&mut a, Architecture::M68k, image), 0);
}

#[test]
#[ignore = "requires a real wgpu adapter"]
fn public_pixel_states_keep_gpu_drawing_coherent_across_purge_and_reconstruction() {
    let mut a = app();
    let old = get(&mut a, Architecture::PowerPc);
    rect(&mut a, [0, 0, 4, 8]);
    assert_eq!(new(&mut a, Architecture::M68k, 8, 0, 0, 1).unwrap(), 0);
    let world = data(&a, DATA);
    let pm = one(&mut a, Architecture::PowerPc, 7, world);
    set(&mut a, Architecture::PowerPc, world, 0);
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
    a.runtime.graphics = Some(
        pollster::block_on(Graphics::new(
            &instance,
            None,
            512,
            342,
            systemless_future::quickdraw::SCREEN,
        ))
        .unwrap(),
    );
    let mut expected = [0; 32];
    expected[10..14].fill(255);
    expected[18..22].fill(255);
    for native_draw in [false, true] {
        pixel_state(&mut a, Architecture::M68k, 16, pm, 0xc0).unwrap();
        rect(&mut a, [1, 2, 3, 6]);
        if native_draw {
            a.runtime
                .memory
                .write(GuestPtr(STACK), &(DATA + 16).to_be_bytes())
                .unwrap();
            a.runtime.cpu = Cpu::new(Architecture::M68k, DATA + 0x1004, STACK);
            assert_eq!(a.runtime.advance(1).unwrap(), Stop::Budget);
        } else {
            ppc(&mut a, 12, &[DATA + 16]).unwrap();
        }
        let base = one(&mut a, Architecture::PowerPc, 9, pm);
        assert_eq!(a.runtime.memory.read(GuestPtr(base), 32).unwrap(), expected);
        assert_eq!(
            pixel_state(&mut a, Architecture::PowerPc, 15, pm, 0).unwrap(),
            0xc0
        );
        pixel_state(&mut a, Architecture::PowerPc, 16, pm, 0x40).unwrap();
        if !native_draw {
            a.runtime
                .process
                .purge_and_compact_memory(&mut a.runtime.memory)
                .unwrap();
            assert_eq!(one(&mut a, Architecture::M68k, 10, pm), 0);
            rect(&mut a, [0, 0, 4, 8]);
            assert_eq!(update(&mut a, Architecture::PowerPc, 8, 0, 0, 0), 1 << 20);
            assert_eq!(
                pixel_state(&mut a, Architecture::M68k, 15, pm, 0).unwrap(),
                0x40
            );
        }
    }
    set(&mut a, Architecture::M68k, old.0, old.1);
    one(&mut a, Architecture::PowerPc, 4, world);
}

#[test]
#[ignore = "requires a real wgpu adapter"]
fn direct_worlds_share_owned_lifecycle_device_format_and_gpu_pixels_on_both_cpus() {
    for depth in [16i16, 32] {
        for arch in [Architecture::M68k, Architecture::PowerPc] {
            let other = if arch == Architecture::M68k {
                Architecture::PowerPc
            } else {
                Architecture::M68k
            };
            let mut a = app();
            let old = get(&mut a, arch);
            let free = a.runtime.process.free_memory();
            rect(&mut a, [0, 0, 4, 8]);
            assert_eq!(new(&mut a, arch, depth, 0xdeadbeef, 0, 1).unwrap(), 0);
            assert_eq!(get(&mut a, other), old);
            let world = data(&a, DATA);
            let pm = one(&mut a, other, 7, world);
            let gd = one(&mut a, arch, 8, world);
            let pm_data = data(&a, pm);
            let gd_data = data(&a, gd);
            let ct = data(&a, pm_data + 42);
            let ct_data = data(&a, ct);
            let bits = if depth == 16 { 5u32 } else { 8 };
            assert_eq!(data(&a, ct_data), 3 * bits);
            assert_eq!(
                a.runtime
                    .process
                    .handle_size(&a.runtime.memory, ct)
                    .unwrap(),
                16
            );
            assert_eq!(
                a.runtime.memory.read(GuestPtr(ct_data + 4), 12).unwrap(),
                [0; 12]
            );
            assert_eq!(a.runtime.memory.u16(GuestPtr(gd_data + 4)).unwrap(), 2);
            assert_eq!(
                a.runtime.memory.u16(GuestPtr(gd_data + 20)).unwrap(),
                0x4001
            );
            assert_eq!(data(&a, gd_data + 42), u32::MAX);
            let inverse = data(&a, gd_data + 6);
            assert_eq!(
                a.runtime
                    .process
                    .handle_size(&a.runtime.memory, inverse)
                    .unwrap(),
                6
            );
            assert_eq!(
                a.runtime
                    .memory
                    .read(GuestPtr(data(&a, inverse)), 6)
                    .unwrap(),
                [0; 6]
            );
            assert_eq!(a.runtime.memory.u16(GuestPtr(pm_data + 30)).unwrap(), 16);
            assert_eq!(a.runtime.memory.u16(GuestPtr(pm_data + 34)).unwrap(), 3);
            assert_eq!(
                a.runtime.memory.u16(GuestPtr(pm_data + 36)).unwrap(),
                bits as u16
            );
            let device_pm = data(&a, gd_data + 22);
            assert_ne!(device_pm, pm);
            assert_eq!(
                a.runtime.memory.read(GuestPtr(pm_data), 50).unwrap(),
                a.runtime
                    .memory
                    .read(GuestPtr(data(&a, device_pm)), 50)
                    .unwrap()
            );
            assert_eq!(data(&a, world + 80), 0);
            assert_eq!(
                data(&a, world + 84),
                if depth == 16 { 0x7fff } else { 0xffffff }
            );
            assert_eq!(one(&mut a, other, 10, pm), 1);
            let base = one(&mut a, arch, 9, pm);
            let row = 8 * depth as usize / 8;
            let instance =
                wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
            a.runtime.graphics = Some(
                pollster::block_on(Graphics::new(&instance, None, 8, 4, GuestPtr(0x4000))).unwrap(),
            );
            set(&mut a, other, world, 0xdeadbeef);
            a.runtime
                .memory
                .write(GuestPtr(DATA + 64), &[255; 6])
                .unwrap();
            a.runtime
                .process
                .ports
                .unwrap()
                .set_rgb(&mut a.runtime.memory, false, GuestPtr(DATA + 64))
                .unwrap();
            rect(&mut a, [1, 2, 3, 6]);
            if arch == Architecture::PowerPc {
                ppc(&mut a, 12, &[DATA + 16]).unwrap();
            } else {
                a.runtime
                    .memory
                    .write(GuestPtr(STACK), &(DATA + 16).to_be_bytes())
                    .unwrap();
                a.runtime.cpu = Cpu::new(Architecture::M68k, DATA + 0x1004, STACK);
                assert_eq!(a.runtime.advance(1).unwrap(), Stop::Budget);
            }
            let pixel = if depth == 16 {
                vec![0x7f, 0xff]
            } else {
                vec![0, 255, 255, 255]
            };
            let mut expected = vec![0; row * 4];
            for y in 1..3 {
                for x in 2..6 {
                    let at = y * row + x * pixel.len();
                    expected[at..at + pixel.len()].copy_from_slice(&pixel);
                }
            }
            assert_eq!(
                a.runtime
                    .memory
                    .read(GuestPtr(base), expected.len())
                    .unwrap(),
                expected
            );
            let rgba = a.runtime.graphics.as_ref().unwrap().read_rgba().unwrap();
            for y in 0..4 {
                for x in 0..8 {
                    let value = if (1..3).contains(&y) && (2..6).contains(&x) {
                        255
                    } else {
                        0
                    };
                    assert_eq!(
                        &rgba[(y * 8 + x) * 4..(y * 8 + x + 1) * 4],
                        &[value, value, value, 255]
                    );
                }
            }
            let parent_device = a
                .runtime
                .memory
                .read(GuestPtr(gd_data), 62)
                .unwrap()
                .to_vec();
            let parent_map = a
                .runtime
                .memory
                .read(GuestPtr(data(&a, device_pm)), 50)
                .unwrap()
                .to_vec();
            let remaining = a.runtime.process.free_memory();
            // noNewDevice takes the supplied direct format, ignoring depth/table.
            rect(&mut a, [0, 0, 2, 3]);
            assert_eq!(new(&mut a, other, 8, 0xdeadbeef, gd, 2).unwrap(), 0);
            let borrowed = data(&a, DATA);
            assert_eq!(one(&mut a, arch, 8, borrowed), gd);
            let borrowed_pm = one(&mut a, other, 7, borrowed);
            let borrowed_data = data(&a, borrowed_pm);
            assert_eq!(
                a.runtime.memory.u16(GuestPtr(borrowed_data + 32)).unwrap(),
                depth as u16
            );
            assert_ne!(data(&a, borrowed_data + 42), ct);
            assert_eq!(get(&mut a, arch), (world, gd));
            one(&mut a, arch, 4, borrowed);
            assert_eq!(a.runtime.process.free_memory(), remaining);
            assert_eq!(
                a.runtime.memory.read(GuestPtr(gd_data), 62).unwrap(),
                parent_device
            );
            assert_eq!(
                a.runtime
                    .memory
                    .read(GuestPtr(data(&a, device_pm)), 50)
                    .unwrap(),
                parent_map
            );
            one(&mut a, other, 11, pm);
            set(&mut a, arch, old.0, old.1);
            one(&mut a, other, 4, world);
            assert_eq!(a.runtime.process.free_memory(), free);
        }
    }
}

#[test]
fn direct_creation_low_memory_preserves_environment_and_releases_partial_graphs() {
    for depth in [16, 32] {
        let mut failures = 0;
        let mut successes = 0;
        for available in [100, 800, 1600, 3200, 6400, 12000] {
            let mut a = app();
            let old = get(&mut a, Architecture::M68k);
            let free = a.runtime.process.free_memory();
            let filler = a
                .runtime
                .process
                .new_pointer(&mut a.runtime.memory, free - available, false)
                .unwrap();
            let result = new(&mut a, Architecture::PowerPc, depth, 0, 0, 0).unwrap() as i32;
            if result == 0 {
                successes += 1;
                let world = data(&a, DATA);
                one(&mut a, Architecture::M68k, 4, world);
            } else {
                assert_eq!(result, -108);
                failures += 1;
                assert_eq!(data(&a, DATA), 0);
            }
            assert_eq!(get(&mut a, Architecture::M68k), old);
            assert_eq!(
                a.runtime.process.free_memory(),
                available,
                "depth{depth} available{available}"
            );
            a.runtime.process.dispose_pointer(filler);
        }
        assert!(failures > 0 && successes > 0);
    }
}

#[test]
fn direct_world_updates_reject_before_image_or_environment_mutation() {
    for depth in [16, 32] {
        for requested in [0, 8] {
            let mut a = app();
            assert_eq!(new(&mut a, Architecture::M68k, depth, 0, 0, 0).unwrap(), 0);
            let world = data(&a, DATA);
            let pm = one(&mut a, Architecture::M68k, 7, world);
            let before = a
                .runtime
                .memory
                .read(GuestPtr(data(&a, pm)), 50)
                .unwrap()
                .to_vec();
            let free = a.runtime.process.free_memory();
            let selected = selection(&a);
            let error = ppc(&mut a, 13, &[DATA, requested, DATA + 16, 0, 0, 0]).unwrap_err();
            assert!(error.to_string().contains("direct-color GWorld update"));
            assert_eq!(data(&a, DATA), world);
            assert_eq!(
                a.runtime.memory.read(GuestPtr(data(&a, pm)), 50).unwrap(),
                before
            );
            assert_eq!(a.runtime.process.free_memory(), free);
            assert_eq!(selection(&a), selected);
        }
    }
}

#[test]
fn borrowed_direct_devices_keep_their_world_owner_alive() {
    for depth in [16, 32] {
        let mut a = app();
        let old = get(&mut a, Architecture::PowerPc);
        assert_eq!(new(&mut a, Architecture::M68k, depth, 0, 0, 0).unwrap(), 0);
        let parent = data(&a, DATA);
        let gd = one(&mut a, Architecture::PowerPc, 8, parent);
        assert_eq!(new(&mut a, Architecture::M68k, 8, 0, gd, 2).unwrap(), 0);
        let child = data(&a, DATA);
        let free = a.runtime.process.free_memory();
        let before = a
            .runtime
            .memory
            .read(GuestPtr(data(&a, gd)), 62)
            .unwrap()
            .to_vec();
        let error = ppc(&mut a, 4, &[parent]).unwrap_err();
        assert!(error.to_string().contains("borrowers"));
        assert_eq!(a.runtime.process.pointer_size(parent), 108);
        assert_eq!(a.runtime.process.pointer_size(child), 108);
        assert_eq!(
            a.runtime.memory.read(GuestPtr(data(&a, gd)), 62).unwrap(),
            before
        );
        assert_eq!(a.runtime.process.free_memory(), free);
        assert_eq!(selection(&a), old);
    }
}
