mod support;
use systemless_future::{
    cpu::{Architecture, Cpu},
    load::{LoadedApplication, STACK_BASE},
    memory::{GuestPtr, Permissions},
    quickdraw::QD_COLORS,
    runtime::Stop,
};
const DATA: u32 = 0x50000000;
const CODE: u32 = 0x51000000;
const STACK: u32 = STACK_BASE + 0x8000;
fn app() -> LoadedApplication {
    let f = support::pef_program(
        &[0x4e800020],
        &[
            b"InitGraf",
            b"OpenPort",
            b"OpenCPort",
            b"SetPort",
            b"GetForeColor",
            b"GetBackColor",
        ],
        32,
    );
    let mut a = LoadedApplication::pef(&f.bytes, None).unwrap();
    a.runtime
        .memory
        .map(GuestPtr(DATA), vec![0xa5; 512], Permissions::RW)
        .unwrap();
    a.runtime
        .memory
        .map(
            GuestPtr(CODE),
            vec![0xaa, 0x19, 0xaa, 0x1a],
            Permissions::RX,
        )
        .unwrap();
    ppc(&mut a, 0, 0x600);
    a
}
fn ppc(a: &mut LoadedApplication, index: u32, arg: u32) {
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
    c.gpr[3] = arg;
    assert_eq!(a.runtime.advance(1).unwrap(), Stop::Budget);
}
fn prepare(a: &mut LoadedApplication, arch: Architecture, background: bool, output: u32) {
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
    if arch == Architecture::M68k {
        let mut frame = output.to_be_bytes().to_vec();
        frame.extend([0x5a; 8]);
        a.runtime.memory.write(GuestPtr(STACK), &frame).unwrap();
        a.runtime.cpu = Cpu::new(arch, CODE + u32::from(background) * 2, STACK);
        let Cpu::M68k(c) = &mut a.runtime.cpu else {
            unreachable!()
        };
        c.dar[0] = 0x12345678;
        c.dar[1] = 0x87654321;
    } else {
        let tv = a
            .runtime
            .memory
            .u32(GuestPtr(
                a.info.toc_or_a5 + 8 + (4 + u32::from(background)) * 4,
            ))
            .unwrap();
        let pc = a.runtime.memory.u32(GuestPtr(tv)).unwrap();
        a.runtime.cpu = Cpu::new(arch, pc, STACK);
        let Cpu::PowerPc(c) = &mut a.runtime.cpu else {
            unreachable!()
        };
        c.gpr[3] = output;
        c.gpr[4] = 0x87654321;
    }
}
fn query(a: &mut LoadedApplication, arch: Architecture, background: bool, output: u32) {
    let free = a.runtime.process.free_memory();
    prepare(a, arch, background, output);
    assert_eq!(a.runtime.advance(1).unwrap(), Stop::Budget);
    assert_eq!(
        (a.runtime.process.mem_error, a.runtime.process.res_error),
        (-99, -98)
    );
    assert_eq!(a.runtime.memory.u16(GuestPtr(0x220)).unwrap() as i16, -77);
    assert_eq!(a.runtime.memory.u16(GuestPtr(0xa60)).unwrap() as i16, -78);
    assert_eq!(a.runtime.process.free_memory(), free);
    match &a.runtime.cpu {
        Cpu::M68k(c) => {
            assert_eq!(&c.dar[..2], &[0x12345678, 0x87654321]);
            assert_eq!(c.dar[15], STACK + 4);
            assert_eq!(
                a.runtime.memory.read(GuestPtr(STACK + 4), 8).unwrap(),
                [0x5a; 8]
            );
        }
        Cpu::PowerPc(c) => assert_eq!(&c.gpr[3..5], &[output, 0x87654321]),
    }
}
fn write(a: &mut LoadedApplication, at: u32, bytes: &[u8]) {
    a.runtime.memory.write(GuestPtr(at), bytes).unwrap();
}
#[test]
fn all_eight_basic_colors_and_live_relocated_qdcolors_are_shared() {
    let constants = [33u32, 69, 137, 205, 273, 341, 409, 30];
    let expected: [[u16; 3]; 8] = [
        [0; 3],
        [0xfc00, 0xf37d, 0x052f],
        [0xf2d7, 0x0856, 0x84ec],
        [0xdd6b, 0x08c2, 0x06a2],
        [0x0241, 0xab54, 0xeaff],
        [0, 0x64af, 0x11b0],
        [0, 0, 0xd400],
        [65535; 3],
    ];
    for arch in [Architecture::M68k, Architecture::PowerPc] {
        let mut a = app();
        ppc(&mut a, 1, DATA);
        ppc(&mut a, 3, DATA);
        for (value, rgb) in constants.into_iter().zip(expected) {
            for background in [false, true] {
                write(
                    &mut a,
                    DATA + if background { 84 } else { 80 },
                    &value.to_be_bytes(),
                );
                write(&mut a, DATA + 256, &[0xa5; 10]);
                query(&mut a, arch, background, DATA + 258);
                let wanted: Vec<_> = rgb.into_iter().flat_map(u16::to_be_bytes).collect();
                assert_eq!(
                    a.runtime.memory.read(GuestPtr(DATA + 258), 6).unwrap(),
                    wanted
                );
                assert_eq!(
                    a.runtime.memory.read(GuestPtr(DATA + 256), 2).unwrap(),
                    [0xa5; 2]
                );
                assert_eq!(
                    a.runtime.memory.read(GuestPtr(DATA + 264), 2).unwrap(),
                    [0xa5; 2]
                );
            }
        }
        let handle = a.runtime.memory.u32(QD_COLORS).unwrap();
        let old = a.runtime.memory.u32(GuestPtr(handle)).unwrap();
        a.runtime
            .process
            .set_handle_size(&mut a.runtime.memory, handle, 128)
            .unwrap();
        let moved = a.runtime.memory.u32(GuestPtr(handle)).unwrap();
        assert_ne!(moved, old);
        write(&mut a, moved + 10, &[1, 2, 3, 4, 5, 6]);
        write(&mut a, DATA + 80, &33u32.to_be_bytes());
        query(&mut a, arch, false, DATA + 258);
        assert_eq!(
            a.runtime.memory.read(GuestPtr(DATA + 258), 6).unwrap(),
            [1, 2, 3, 4, 5, 6]
        );
    }
}
#[test]
fn selected_color_port_returns_full_requested_rgb_without_resolving_pixels_or_devices() {
    for arch in [Architecture::M68k, Architecture::PowerPc] {
        let mut a = app();
        ppc(&mut a, 2, DATA);
        ppc(&mut a, 2, DATA + 128);
        write(&mut a, DATA + 36, &[1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12]);
        write(
            &mut a,
            DATA + 128 + 36,
            &[21, 22, 23, 24, 25, 26, 27, 28, 29, 30, 31, 32],
        );
        // Queries use rgbFgColor/rgbBkColor, even if indexed fields or surface
        // descriptors cannot be used for drawing.
        write(&mut a, DATA + 2, &[0; 4]);
        write(&mut a, DATA + 80, &[0xff; 8]);
        for (port, fg, bg) in [
            (DATA, [1, 2, 3, 4, 5, 6], [7, 8, 9, 10, 11, 12]),
            (
                DATA + 128,
                [21, 22, 23, 24, 25, 26],
                [27, 28, 29, 30, 31, 32],
            ),
        ] {
            ppc(&mut a, 3, port);
            let before = a.runtime.memory.read(GuestPtr(port), 108).unwrap().to_vec();
            query(&mut a, arch, false, DATA + 258);
            assert_eq!(a.runtime.memory.read(GuestPtr(DATA + 258), 6).unwrap(), fg);
            query(&mut a, arch, true, DATA + 258);
            assert_eq!(a.runtime.memory.read(GuestPtr(DATA + 258), 6).unwrap(), bg);
            assert_eq!(a.runtime.memory.read(GuestPtr(port), 108).unwrap(), before);
        }
        ppc(&mut a, 3, DATA + 128);
        // Overlap must read all source bytes before publishing the result.
        query(&mut a, arch, false, DATA + 128 + 38);
        assert_eq!(
            a.runtime.memory.read(GuestPtr(DATA + 128 + 38), 6).unwrap(),
            [21, 22, 23, 24, 25, 26]
        );
    }
}
#[test]
fn invalid_outputs_are_rejected_before_abi_stack_mutation_or_partial_write() {
    for arch in [Architecture::M68k, Architecture::PowerPc] {
        for background in [false, true] {
            for output in [0, 0x52000000, 0x52000002, 0xfffffffc] {
                let mut a = app();
                ppc(&mut a, 1, DATA);
                ppc(&mut a, 3, DATA);
                a.runtime
                    .memory
                    .map(GuestPtr(0x52000000), vec![0xa5; 4], Permissions::RW)
                    .unwrap();
                a.runtime
                    .memory
                    .map(GuestPtr(0x52000004), vec![0xa5; 4], Permissions::RX)
                    .unwrap();
                prepare(&mut a, arch, background, output);
                assert!(a.runtime.advance(1).is_err());
                for at in [0x52000000, 0x52000004] {
                    assert_eq!(a.runtime.memory.read(GuestPtr(at), 4).unwrap(), [0xa5; 4]);
                }
                if let Cpu::M68k(c) = &a.runtime.cpu {
                    assert_eq!(c.dar[15], STACK);
                }
            }
        }
    }
}

#[test]
fn malformed_live_source_does_not_publish_an_output_or_allocate() {
    for arch in [Architecture::M68k, Architecture::PowerPc] {
        for defect in 0..4 {
            let mut a = app();
            ppc(&mut a, 1, DATA);
            ppc(&mut a, 3, DATA);
            let handle = a.runtime.memory.u32(QD_COLORS).unwrap();
            let table = a.runtime.memory.u32(GuestPtr(handle)).unwrap();
            match defect {
                0 => write(&mut a, QD_COLORS.0, &[0; 4]),
                1 => write(&mut a, handle, &[0; 4]),
                2 => write(&mut a, table + 6, &6u16.to_be_bytes()),
                _ => write(&mut a, DATA + 80, &999u32.to_be_bytes()),
            }
            let free = a.runtime.process.free_memory();
            prepare(&mut a, arch, false, DATA + 258);
            assert!(a.runtime.advance(1).is_err());
            assert_eq!(
                a.runtime.memory.read(GuestPtr(DATA + 258), 6).unwrap(),
                [0xa5; 6]
            );
            assert_eq!(a.runtime.process.free_memory(), free);
        }
    }
}
