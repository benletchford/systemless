mod support;
use systemless_future::{
    cpu::{Architecture, Cpu},
    load::{LoadedApplication, STACK_BASE},
    memory::{GuestPtr, Permissions},
    runtime::Stop,
};
const BASE: u32 = 0x50000000;
const STACK: u32 = STACK_BASE + 0x8000;
fn app() -> LoadedApplication {
    let f = support::pef_program(
        &[0x4e800020],
        &[b"MakeITable", b"QDError", b"GetGDevice"],
        32,
    );
    let mut a = LoadedApplication::pef(&f.bytes, None).unwrap();
    a.runtime
        .memory
        .map(GuestPtr(BASE), vec![0; 16], Permissions::RW)
        .unwrap();
    a.runtime
        .memory
        .map(
            GuestPtr(BASE + 4096),
            vec![0xaa, 0x39, 0xaa, 0x40, 0xaa, 0x32],
            Permissions::RX,
        )
        .unwrap();
    a
}
fn call(a: &mut LoadedApplication, arch: Architecture, index: u32, args: &[u32]) -> u32 {
    a.runtime.process.mem_error = -99;
    a.runtime.process.res_error = -98;
    a.runtime
        .memory
        .write(GuestPtr(0x220), &(-77i16).to_be_bytes())
        .unwrap();
    a.runtime
        .memory
        .write(GuestPtr(0xa60), &(-78i16).to_be_bytes())
        .unwrap();
    let consumed = if index == 0 { 10 } else { 0 };
    if arch == Architecture::M68k {
        let mut frame = Vec::new();
        if index == 0 {
            frame.extend((args[2] as u16).to_be_bytes());
            frame.extend(args[1].to_be_bytes());
            frame.extend(args[0].to_be_bytes());
        }
        frame.extend([0xa5; 8]);
        a.runtime.memory.write(GuestPtr(STACK), &frame).unwrap();
        a.runtime.cpu = Cpu::new(arch, BASE + 4096 + index * 2, STACK);
        let Cpu::M68k(c) = &mut a.runtime.cpu else {
            unreachable!()
        };
        c.dar[0] = 0x12345678;
    } else {
        let tv = a
            .runtime
            .memory
            .u32(GuestPtr(a.info.toc_or_a5 + 8 + index * 4))
            .unwrap();
        let pc = a.runtime.memory.u32(GuestPtr(tv)).unwrap();
        a.runtime.cpu = Cpu::new(arch, pc, STACK);
        let Cpu::PowerPc(c) = &mut a.runtime.cpu else {
            unreachable!()
        };
        for (i, arg) in args.iter().enumerate() {
            c.gpr[3 + i] = *arg;
        }
    }
    assert_eq!(a.runtime.advance(1).unwrap(), Stop::Budget);
    assert_eq!(
        (a.runtime.process.mem_error, a.runtime.process.res_error),
        (-99, -98)
    );
    assert_eq!(a.runtime.memory.u16(GuestPtr(0x220)).unwrap() as i16, -77);
    assert_eq!(a.runtime.memory.u16(GuestPtr(0xa60)).unwrap() as i16, -78);
    match &a.runtime.cpu {
        Cpu::M68k(c) => {
            assert_eq!(c.dar[15], STACK + consumed);
            assert_eq!(c.dar[0], 0x12345678);
            if index == 1 {
                a.runtime.memory.u16(GuestPtr(STACK)).unwrap() as i16 as i32 as u32
            } else if index == 2 {
                a.runtime.memory.u32(GuestPtr(STACK)).unwrap()
            } else {
                assert_eq!(
                    a.runtime.memory.u32(GuestPtr(STACK + 10)).unwrap(),
                    0xa5a5a5a5
                );
                0
            }
        }
        Cpu::PowerPc(c) => {
            if index == 0 {
                assert_eq!(c.gpr[3], args[0]);
                0
            } else {
                c.gpr[3]
            }
        }
    }
}
fn handle(a: &mut LoadedApplication, bytes: &[u8]) -> u32 {
    let h = a
        .runtime
        .process
        .new_handle(&mut a.runtime.memory, bytes.len() as u32, false)
        .unwrap();
    let p = a.runtime.memory.u32(GuestPtr(h)).unwrap();
    a.runtime.memory.write(GuestPtr(p), bytes).unwrap();
    h
}
fn palette(a: &mut LoadedApplication, device: bool, entries: &[(u16, [u16; 3])]) -> u32 {
    let mut b = 0x12345678u32.to_be_bytes().to_vec();
    b.extend((if device { 0x8000u16 } else { 0 }).to_be_bytes());
    b.extend(((entries.len() - 1) as u16).to_be_bytes());
    for (index, color) in entries {
        b.extend(index.to_be_bytes());
        for channel in color {
            b.extend(channel.to_be_bytes());
        }
    }
    handle(a, &b)
}
fn bytes(a: &mut LoadedApplication, h: u32) -> Vec<u8> {
    let n = a.runtime.process.handle_size(&a.runtime.memory, h).unwrap();
    let p = a.runtime.memory.u32(GuestPtr(h)).unwrap();
    a.runtime
        .memory
        .read(GuestPtr(p), n as usize)
        .unwrap()
        .to_vec()
}
#[test]
fn both_cpus_build_all_resolutions_with_real_seed_layout_and_analytic_gray_boundary() {
    for arch in [Architecture::M68k, Architecture::PowerPc] {
        let mut a = app();
        let source = palette(&mut a, true, &[(99, [65535; 3]), (77, [0; 3])]);
        let before = bytes(&mut a, source);
        let out = handle(&mut a, &[0xa5; 2]);
        for res in [3, 4, 5] {
            call(&mut a, arch, 0, &[source, out, res]);
            assert_eq!(call(&mut a, arch, 1, &[]), 0);
            let b = bytes(&mut a, out);
            assert_eq!(&b[..4], &0x12345678u32.to_be_bytes());
            assert_eq!(&b[4..6], &(res as u16).to_be_bytes());
            let levels = 1usize << res;
            assert!(b.len() >= 6 + levels.pow(3));
            for (cell, &pixel) in b[6..6 + levels.pow(3)].iter().enumerate() {
                let r = cell / (levels * levels);
                let g = (cell / levels) % levels;
                let blue = cell % levels;
                assert_eq!(pixel, u8::from(2 * (r + g + blue) <= 3 * (levels - 1)));
            }
            assert_eq!(bytes(&mut a, source), before);
        }
    }
}
#[test]
fn sparse_color_values_and_default_device_resolution_produce_real_tables_without_changing_selection(
) {
    for arch in [Architecture::M68k, Architecture::PowerPc] {
        let mut a = app();
        let source = palette(
            &mut a,
            false,
            &[
                (17, [65535, 0, 0]),
                (23, [0, 65535, 0]),
                (31, [0, 0, 65535]),
                (42, [0; 3]),
                (53, [65535; 3]),
            ],
        );
        let out = handle(&mut a, &[]);
        call(&mut a, arch, 0, &[source, out, 3]);
        let b = bytes(&mut a, out);
        for (cell, pixel) in [(0, 42), (7 << 6, 17), (7 << 3, 23), (7, 31), (511, 53)] {
            assert_eq!(b[6 + cell], pixel);
        }
        let gd = call(&mut a, arch, 2, &[]);
        let record = a.runtime.memory.u32(GuestPtr(gd)).unwrap();
        assert_eq!(a.runtime.memory.u16(GuestPtr(record + 10)).unwrap(), 4);
        call(&mut a, arch, 0, &[0, 0, 0]);
        assert_eq!(call(&mut a, arch, 1, &[]), 0);
        let inverse = a.runtime.memory.u32(GuestPtr(record + 6)).unwrap();
        assert_ne!(inverse, 0);
        let b = bytes(&mut a, inverse);
        assert!(b.len() >= 4102);
        assert_eq!(b[6], 1);
        assert_eq!(b[4101], 0);
        let free = a.runtime.process.free_memory();
        call(&mut a, arch, 0, &[0, 0, 0]);
        assert_eq!(a.runtime.process.free_memory(), free);
        assert_eq!(a.runtime.memory.u32(GuestPtr(record + 6)).unwrap(), inverse);
        assert_eq!(
            a.runtime
                .memory
                .u32(systemless_future::devices::CURRENT)
                .unwrap(),
            gd
        );
        let main_table = bytes(&mut a, inverse);
        let off = a
            .runtime
            .process
            .copy_handle(&mut a.runtime.memory, gd)
            .unwrap();
        let off_data = a.runtime.memory.u32(GuestPtr(off)).unwrap();
        let main_pm = a.runtime.memory.u32(GuestPtr(record + 22)).unwrap();
        let off_pm = a
            .runtime
            .process
            .copy_handle(&mut a.runtime.memory, main_pm)
            .unwrap();
        let off_pixels = a.runtime.memory.u32(GuestPtr(off_pm)).unwrap();
        let off_colors = palette(&mut a, true, &[(0, [65535, 0, 0]), (1, [0, 65535, 0])]);
        a.runtime
            .memory
            .write(GuestPtr(off_pixels + 42), &off_colors.to_be_bytes())
            .unwrap();
        a.runtime
            .memory
            .write(GuestPtr(off_data + 22), &off_pm.to_be_bytes())
            .unwrap();
        a.runtime
            .memory
            .write(GuestPtr(off_data + 6), &0u32.to_be_bytes())
            .unwrap();
        a.runtime
            .memory
            .write(GuestPtr(off_data + 10), &3u16.to_be_bytes())
            .unwrap();
        let mut manager = std::mem::take(&mut a.runtime.process.devices);
        manager
            .call(
                &mut a.runtime.memory,
                &mut a.runtime.process,
                systemless_future::devices::Op::Select(off),
            )
            .unwrap();
        a.runtime.process.devices = manager;
        assert_eq!(call(&mut a, arch, 2, &[]), off);
        call(&mut a, arch, 0, &[0, 0, 0]);
        assert_eq!(call(&mut a, arch, 1, &[]), 0);
        let off_inverse = a.runtime.memory.u32(GuestPtr(off_data + 6)).unwrap();
        assert_ne!(off_inverse, inverse);
        let off_table = bytes(&mut a, off_inverse);
        assert_eq!(&off_table[..6], &[0x12, 0x34, 0x56, 0x78, 0, 3]);
        assert_eq!(off_table[6 + (7 << 6)], 0);
        assert_eq!(off_table[6 + (7 << 3)], 1);
        assert_eq!(bytes(&mut a, inverse), main_table);
    }
}
#[test]
fn resolution_and_allocation_errors_publish_qderror_without_partial_table_or_error_global_changes()
{
    for arch in [Architecture::M68k, Architecture::PowerPc] {
        let mut a = app();
        let free = a.runtime.process.free_memory();
        for res in [2, 6, u32::MAX] {
            call(&mut a, arch, 0, &[0, 0, res]);
            let other = if arch == Architecture::M68k {
                Architecture::PowerPc
            } else {
                Architecture::M68k
            };
            for reader in [arch, other] {
                assert_eq!(call(&mut a, reader, 1, &[]) as i32, -156);
            }
            assert_eq!(a.runtime.process.free_memory(), free);
        }
        let source = palette(&mut a, true, &[(0, [65535; 3]), (1, [0; 3])]);
        let out = handle(&mut a, &[0xa5; 2]);
        let data = a.runtime.memory.u32(GuestPtr(out)).unwrap();
        let n = a.runtime.process.free_memory() - 32;
        assert_ne!(
            a.runtime
                .process
                .new_pointer(&mut a.runtime.memory, n, false)
                .unwrap(),
            0
        );
        let free = a.runtime.process.free_memory();
        call(&mut a, arch, 0, &[source, out, 5]);
        assert_eq!(call(&mut a, arch, 1, &[]) as i32, -108);
        assert_eq!(a.runtime.memory.u32(GuestPtr(out)).unwrap(), data);
        assert_eq!(bytes(&mut a, out), [0xa5; 2]);
        assert_eq!(a.runtime.process.free_memory(), free);
    }
}
#[test]
fn malformed_tables_fault_before_output_resize_or_payload_mutation() {
    let mut a = app();
    let source = palette(&mut a, false, &[(17, [0; 3]), (17, [65535; 3])]);
    let out = handle(&mut a, &[0xa5; 32]);
    let free = a.runtime.process.free_memory();
    let mut manager = std::mem::take(&mut a.runtime.process.color);
    assert!(manager
        .call(
            &mut a.runtime.memory,
            &mut a.runtime.process,
            systemless_future::color::Op::Make {
                source,
                destination: out,
                resolution: 4
            }
        )
        .is_err());
    a.runtime.process.color = manager;
    assert_eq!(bytes(&mut a, out), [0xa5; 32]);
    assert_eq!(a.runtime.process.free_memory(), free);
}
