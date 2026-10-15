mod support;
use systemless_future::{
    cpu::{Architecture, Cpu},
    devices::{self, Op},
    load::{LoadedApplication, STACK_BASE},
    memory::{GuestPtr, Permissions},
    process::HEAP_SIZE,
    runtime::Stop,
};
fn app() -> LoadedApplication {
    let f = support::pef_program(
        &[0x4e800020],
        &[
            b"GetGDevice",
            b"GetMainDevice",
            b"GetDeviceList",
            b"GetNextDevice",
            b"TestDeviceAttribute",
            b"Index2Color",
            b"InitGraf",
            b"OpenPort",
            b"SetPortBits",
            b"CopyBits",
            b"SetGDevice",
            b"SetDeviceAttribute",
        ],
        64,
    );
    let mut a = LoadedApplication::pef(&f.bytes, None).unwrap();
    a.runtime
        .memory
        .map(
            GuestPtr(0x50000000),
            [
                0xaa32u16, 0xaa2a, 0xaa29, 0xaa2b, 0xaa2c, 0xaa34, 0xa8ec, 0xaa31, 0xaa2d,
            ]
            .into_iter()
            .flat_map(u16::to_be_bytes)
            .collect(),
            Permissions::RX,
        )
        .unwrap();
    a
}
fn ppc(a: &mut LoadedApplication, index: u32, args: &[u32]) -> u32 {
    let tv = a
        .runtime
        .memory
        .u32(GuestPtr(a.info.toc_or_a5 + 8 + index * 4))
        .unwrap();
    let pc = a.runtime.memory.u32(GuestPtr(tv)).unwrap();
    a.runtime.cpu = Cpu::new(Architecture::PowerPc, pc, STACK_BASE + 0x8000);
    let Cpu::PowerPc(c) = &mut a.runtime.cpu else {
        panic!()
    };
    for (i, &arg) in args.iter().enumerate() {
        c.gpr[3 + i] = arg;
    }
    assert_eq!(a.runtime.advance(1).unwrap(), Stop::Budget);
    let Cpu::PowerPc(c) = &a.runtime.cpu else {
        panic!()
    };
    c.gpr[3]
}
fn native(a: &mut LoadedApplication, index: u32, args: &[u8], result: usize) -> Vec<u8> {
    let stack = STACK_BASE + 0x8000;
    a.runtime
        .memory
        .write(GuestPtr(stack), &[0xcc; 32])
        .unwrap();
    a.runtime.memory.write(GuestPtr(stack), args).unwrap();
    a.runtime.cpu = Cpu::new(Architecture::M68k, 0x50000000 + index * 2, stack);
    let Cpu::M68k(c) = &mut a.runtime.cpu else {
        panic!()
    };
    c.dar[0] = 0x12345678;
    assert_eq!(a.runtime.advance(1).unwrap(), Stop::Budget);
    let Cpu::M68k(c) = &a.runtime.cpu else {
        panic!()
    };
    assert_eq!(c.dar[15], stack + args.len() as u32);
    assert_eq!(c.dar[0], 0x12345678);
    a.runtime
        .memory
        .read(GuestPtr(stack + args.len() as u32), result)
        .unwrap()
        .to_vec()
}
fn call(a: &mut LoadedApplication, op: Op) -> systemless_future::Result<u32> {
    let mut manager = std::mem::take(&mut a.runtime.process.devices);
    let result = manager.call(&mut a.runtime.memory, &mut a.runtime.process, op);
    a.runtime.process.devices = manager;
    result
}
#[test]
fn both_cpus_share_device_records_screen_storage_palette_and_live_attributes() {
    let mut a = app();
    a.runtime
        .memory
        .write(GuestPtr(0x220), &(-42i16).to_be_bytes())
        .unwrap();
    a.runtime.process.mem_error = -99;
    let device = ppc(&mut a, 0, &[]);
    assert_ne!(device, 0);
    for index in 0..3 {
        assert_eq!(ppc(&mut a, index, &[]), device);
        assert_eq!(native(&mut a, index, &[], 4), device.to_be_bytes());
    }
    for pointer in [devices::MAIN, devices::FIRST, devices::CURRENT] {
        assert_eq!(a.runtime.memory.u32(pointer).unwrap(), device);
    }
    assert_eq!(a.runtime.process.mem_error, -99);
    assert_eq!(a.runtime.memory.u16(GuestPtr(0x220)).unwrap() as i16, -42);
    assert_eq!(ppc(&mut a, 3, &[device]), 0);
    assert_eq!(native(&mut a, 3, &device.to_be_bytes(), 4), [0; 4]);
    let gd = a.runtime.memory.u32(GuestPtr(device)).unwrap();
    assert_eq!(
        a.runtime.memory.read(GuestPtr(gd + 34), 8).unwrap(),
        [0, 0, 0, 0, 1, 86, 2, 0]
    );
    let pixels = a.runtime.memory.u32(GuestPtr(gd + 22)).unwrap();
    let pm = a.runtime.memory.u32(GuestPtr(pixels)).unwrap();
    assert_eq!(a.runtime.memory.u16(GuestPtr(pm + 32)).unwrap(), 1);
    assert_eq!(a.runtime.memory.u16(GuestPtr(pm + 4)).unwrap(), 0x8040);
    let base = a.runtime.memory.u32(GuestPtr(pm)).unwrap();
    assert_eq!(base, systemless_future::quickdraw::screen_bitmap().base.0);
    a.runtime.memory.write(GuestPtr(base), &[0x80]).unwrap();
    assert_eq!(
        a.runtime
            .memory
            .read(systemless_future::quickdraw::screen_bitmap().base, 1)
            .unwrap(),
        [0x80]
    );
    for (bit, expected) in [(0, 0), (11, 1), (13, 1), (14, 1), (15, 1)] {
        assert_eq!(ppc(&mut a, 4, &[device, bit]), expected);
        let mut args = (bit as i16).to_be_bytes().to_vec();
        args.extend(device.to_be_bytes());
        assert_eq!(native(&mut a, 4, &args, 2), [expected as u8, 0xcc]);
    }
    a.runtime
        .memory
        .write(GuestPtr(gd + 20), &0u16.to_be_bytes())
        .unwrap();
    assert_eq!(ppc(&mut a, 4, &[device, 15]), 0);
    let destination = STACK_BASE + 0x7000;
    assert_eq!(ppc(&mut a, 5, &[0, destination]), 0);
    assert_eq!(
        a.runtime.memory.read(GuestPtr(destination), 6).unwrap(),
        [255; 6]
    );
    let mut args = destination.to_be_bytes().to_vec();
    args.extend(1i32.to_be_bytes());
    native(&mut a, 5, &args, 0);
    assert_eq!(
        a.runtime.memory.read(GuestPtr(destination), 6).unwrap(),
        [0; 6]
    );
    let table = a.runtime.memory.u32(GuestPtr(pm + 42)).unwrap();
    let ct = a.runtime.memory.u32(GuestPtr(table)).unwrap();
    a.runtime
        .memory
        .write(GuestPtr(ct + 10), &[1, 2, 3, 4, 5, 6])
        .unwrap();
    ppc(&mut a, 5, &[0, destination]);
    assert_eq!(
        a.runtime.memory.read(GuestPtr(destination), 6).unwrap(),
        [1, 2, 3, 4, 5, 6]
    );
}
#[test]
fn partial_initialization_failure_rolls_back_all_owned_storage_and_preserves_globals() {
    let mut a = app();
    let fill = a
        .runtime
        .process
        .new_pointer(&mut a.runtime.memory, HEAP_SIZE - 40, false)
        .unwrap();
    assert_ne!(fill, 0);
    a.runtime.process.mem_error = -77;
    assert!(call(&mut a, Op::Current).is_err());
    assert_eq!(a.runtime.process.mem_error, -77);
    for pointer in [devices::MAIN, devices::FIRST, devices::CURRENT] {
        assert_eq!(a.runtime.memory.u32(pointer).unwrap(), 0);
    }
    assert_ne!(
        a.runtime
            .process
            .new_pointer(&mut a.runtime.memory, 40, false)
            .unwrap(),
        0
    );
}
#[test]
fn bad_rgb_outputs_fault_before_stack_or_allocator_changes_and_corruption_is_explicit() {
    let mut a = app();
    let stack = STACK_BASE + 0x8000;
    a.runtime
        .memory
        .write(GuestPtr(stack), &[0xde, 0xad, 0xbe, 0xef, 0, 0, 0, 0])
        .unwrap();
    a.runtime.cpu = Cpu::new(Architecture::M68k, 0x5000000a, stack);
    assert!(a.runtime.advance(1).is_err());
    let Cpu::M68k(c) = &a.runtime.cpu else {
        panic!()
    };
    assert_eq!(c.dar[15], stack);
    assert_ne!(
        a.runtime
            .process
            .new_pointer(&mut a.runtime.memory, HEAP_SIZE, false)
            .unwrap(),
        0
    );
    let mut a = app();
    let device = call(&mut a, Op::Current).unwrap();
    let gd = a.runtime.memory.u32(GuestPtr(device)).unwrap();
    a.runtime
        .memory
        .write(GuestPtr(gd + 30), &device.to_be_bytes())
        .unwrap();
    assert!(call(&mut a, Op::Next(device)).is_err());
    assert!(call(&mut a, Op::Attribute { device, bit: 16 }).is_err());
    a.runtime
        .memory
        .write(devices::CURRENT, &0u32.to_be_bytes())
        .unwrap();
    assert!(call(&mut a, Op::Current).is_err());
    assert!(call(
        &mut a,
        Op::Index {
            index: 0,
            destination: GuestPtr(STACK_BASE + 0x7000)
        }
    )
    .is_err());
}

#[test]
#[ignore = "requires a real wgpu adapter"]
fn device_pixmap_is_a_real_copybits_source_for_both_cpus_and_gpu_readback() {
    use systemless_future::graphics::Graphics;
    for (arch, indexed) in [Architecture::M68k, Architecture::PowerPc]
        .into_iter()
        .flat_map(|arch| [false, true].map(|indexed| (arch, indexed)))
    {
        let mut a = app();
        let device = call(&mut a, Op::Current).unwrap();
        let gd = a.runtime.memory.u32(GuestPtr(device)).unwrap();
        let pixels = a.runtime.memory.u32(GuestPtr(gd + 22)).unwrap();
        let pm = a.runtime.memory.u32(GuestPtr(pixels)).unwrap();
        let base = a.runtime.memory.u32(GuestPtr(pm)).unwrap();
        a.runtime.memory.write(GuestPtr(base), &[0xa5]).unwrap();
        a.runtime
            .memory
            .map(GuestPtr(0x52000000), vec![0; 1024], Permissions::RW)
            .unwrap();
        ppc(&mut a, 6, &[0x600]);
        ppc(&mut a, 7, &[0x52000000]);
        let bitmap = 0x52000100u32;
        let bits = 0x52000200u32;
        let mut bytes = bits.to_be_bytes().to_vec();
        bytes.extend([0, 2, 0, 0, 0, 0, 0, 1, 0, 8]);
        a.runtime.memory.write(GuestPtr(bitmap), &bytes).unwrap();
        a.runtime.memory.write(GuestPtr(bits), &[0, 0x5a]).unwrap();
        ppc(&mut a, 8, &[bitmap]);
        a.runtime
            .process
            .ports
            .unwrap()
            .port_size(&mut a.runtime.memory, 8, 1)
            .unwrap();
        a.runtime
            .memory
            .write(GuestPtr(0x300), &[0, 0, 0, 0, 0, 1, 0, 8])
            .unwrap();
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
        let mut gpu =
            pollster::block_on(Graphics::new(&instance, None, 8, 1, GuestPtr(0x54000000))).unwrap();
        gpu.palette[0] = [255; 4];
        gpu.palette[1] = [0, 0, 0, 255];
        a.runtime.graphics = Some(gpu);
        let pm = if indexed {
            let mut storage = vec![0; 1024];
            storage[..50].copy_from_slice(a.runtime.memory.read(GuestPtr(pm), 50).unwrap());
            storage[..4].copy_from_slice(&0x53000100u32.to_be_bytes());
            storage[4..6].copy_from_slice(&0x800au16.to_be_bytes());
            storage[6..14].copy_from_slice(&[0, 0, 0, 0, 0, 1, 0, 8]);
            storage[32..34].copy_from_slice(&8u16.to_be_bytes());
            storage[36..38].copy_from_slice(&8u16.to_be_bytes());
            storage[42..46].copy_from_slice(&0x53000200u32.to_be_bytes());
            storage[0x100..0x10a].copy_from_slice(&[1, 0, 1, 0, 0, 1, 0, 1, 0x5a, 0x5a]);
            storage[0x200..0x204].copy_from_slice(&0x53000204u32.to_be_bytes());
            let table = a.runtime.memory.u32(GuestPtr(pm + 42)).unwrap();
            let table_data = a.runtime.memory.u32(GuestPtr(table)).unwrap();
            storage[0x204..0x21c]
                .copy_from_slice(a.runtime.memory.read(GuestPtr(table_data), 24).unwrap());
            // Red maps through the existing documented monochrome policy.
            storage[0x216..0x21c].copy_from_slice(&[255, 255, 0, 0, 0, 0]);
            a.runtime
                .memory
                .map(GuestPtr(0x53000000), storage, Permissions::RX)
                .unwrap();
            0x53000000
        } else {
            pm
        };
        match arch {
            Architecture::PowerPc => {
                ppc(&mut a, 9, &[pm, bitmap, 0x300, 0x300, 0, 0]);
            }
            Architecture::M68k => {
                let mut args = 0u32.to_be_bytes().to_vec();
                args.extend(0u16.to_be_bytes());
                for pointer in [0x300u32, 0x300, bitmap, pm] {
                    args.extend(pointer.to_be_bytes());
                }
                native(&mut a, 6, &args, 0);
            }
        }
        assert_eq!(
            a.runtime.memory.read(GuestPtr(bits), 2).unwrap(),
            [0xa5, 0x5a]
        );
        let rgba = a.runtime.graphics.as_ref().unwrap().read_rgba().unwrap();
        for x in 0..8 {
            let expected = if 0xa5 & (0x80 >> x) != 0 {
                [0, 0, 0, 255]
            } else {
                [255; 4]
            };
            assert_eq!(rgba[x * 4..x * 4 + 4], expected);
        }
    }
}

#[test]
fn both_cpus_select_the_registered_device_restore_guest_selection_and_preserve_port_and_errors() {
    let mut a = app();
    assert!(call(&mut a, Op::Select(0)).is_err());
    assert_eq!(a.runtime.memory.u32(devices::CURRENT).unwrap(), 0);
    let device = ppc(&mut a, 0, &[]);
    a.runtime
        .memory
        .write(GuestPtr(0x220), &(-42i16).to_be_bytes())
        .unwrap();
    a.runtime
        .memory
        .write(GuestPtr(0xa60), &(-43i16).to_be_bytes())
        .unwrap();
    for native_cpu in [false, true] {
        a.runtime
            .memory
            .write(devices::CURRENT, &0u32.to_be_bytes())
            .unwrap();
        if native_cpu {
            assert!(native(&mut a, 7, &device.to_be_bytes(), 0).is_empty());
        } else {
            assert_eq!(ppc(&mut a, 10, &[device]), device);
        }
        for address in [devices::CURRENT, devices::MAIN, devices::FIRST] {
            assert_eq!(a.runtime.memory.u32(address).unwrap(), device);
        }
        assert!(a.runtime.process.ports.is_none());
        assert_eq!(a.runtime.memory.u16(GuestPtr(0x220)).unwrap() as i16, -42);
        assert_eq!(a.runtime.memory.u16(GuestPtr(0xa60)).unwrap() as i16, -43);
        assert!(call(&mut a, Op::Select(device + 4)).is_err());
        assert_eq!(a.runtime.memory.u32(devices::CURRENT).unwrap(), device);
    }
}

#[test]
fn attribute_abis_update_owned_offscreen_records_without_allocating_or_selecting_displays() {
    let mut a = app();
    let h = a
        .runtime
        .process
        .new_handle(&mut a.runtime.memory, 64, false)
        .unwrap();
    let data = a.runtime.memory.u32(GuestPtr(h)).unwrap();
    a.runtime.memory.write(GuestPtr(data), &[0x5a; 64]).unwrap();
    let free = a.runtime.process.free_memory();
    let globals: Vec<_> = [devices::CURRENT, devices::MAIN, devices::FIRST]
        .into_iter()
        .map(|p| a.runtime.memory.u32(p).unwrap())
        .collect();
    a.runtime.process.mem_error = -108;
    a.runtime
        .memory
        .write(GuestPtr(0x220), &(-42i16).to_be_bytes())
        .unwrap();
    // Only the high byte is Boolean; the low byte is deliberately dirty.
    let mut frame = vec![1, 0xa5, 0, 14];
    frame.extend(h.to_be_bytes());
    assert!(native(&mut a, 8, &frame, 0).is_empty());
    assert_eq!(
        a.runtime.memory.u16(GuestPtr(data + 20)).unwrap(),
        0x5a5a | 0x4000
    );
    let mut frame = vec![0, 0xff, 0, 14];
    frame.extend(h.to_be_bytes());
    native(&mut a, 8, &frame, 0);
    assert_eq!(a.runtime.memory.u16(GuestPtr(data + 20)).unwrap(), 0x1a5a);
    assert_eq!(ppc(&mut a, 11, &[h, 0xabcd0000, 0xffffff01]), h);
    assert_eq!(ppc(&mut a, 4, &[h, 0]), 1);
    assert_eq!(ppc(&mut a, 11, &[h, 0, 0xffffff00]), h);
    assert_eq!(ppc(&mut a, 4, &[h, 0]), 0);
    let bytes = a.runtime.memory.read(GuestPtr(data), 64).unwrap();
    assert_eq!(&bytes[..20], &[0x5a; 20]);
    assert_eq!(&bytes[22..], &[0x5a; 42]);
    assert_eq!(a.runtime.process.free_memory(), free);
    assert_eq!(a.runtime.process.mem_error, -108);
    assert_eq!(a.runtime.memory.u16(GuestPtr(0x220)).unwrap() as i16, -42);
    assert_eq!(
        [devices::CURRENT, devices::MAIN, devices::FIRST]
            .into_iter()
            .map(|p| a.runtime.memory.u32(p).unwrap())
            .collect::<Vec<_>>(),
        globals
    );
}
#[test]
fn invalid_attribute_records_and_bits_leave_flags_and_heap_unchanged() {
    let mut a = app();
    let h = a
        .runtime
        .process
        .new_handle(&mut a.runtime.memory, 61, true)
        .unwrap();
    let free = a.runtime.process.free_memory();
    for (device, bit) in [(0, 0), (h, 0), (h, -1), (h, 16)] {
        assert!(call(
            &mut a,
            Op::SetAttribute {
                device,
                bit,
                value: true
            }
        )
        .is_err());
        assert_eq!(a.runtime.process.free_memory(), free);
    }
    let h = a
        .runtime
        .process
        .new_handle(&mut a.runtime.memory, 62, true)
        .unwrap();
    a.runtime
        .memory
        .write(GuestPtr(h), &0xdeadbeefu32.to_be_bytes())
        .unwrap();
    assert!(call(
        &mut a,
        Op::SetAttribute {
            device: h,
            bit: 14,
            value: true
        }
    )
    .is_err());
}
