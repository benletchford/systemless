mod support;
use systemless_future::{
    abi::Abi,
    cpu::{Architecture, Cpu},
    load::{LoadedApplication, STACK_BASE},
    memory::{GuestPtr, Memory, Permissions},
    runtime::Stop,
    traps,
};
#[test]
fn both_adapters_construct_the_same_spec_and_return_errors_without_register_or_frame_damage() {
    let f = support::pef_program(&[0x4e800020], &[b"FSMakeFSSpec"], 16);
    let mut a = LoadedApplication::pef(&f.bytes, None).unwrap();
    a.runtime
        .memory
        .map(GuestPtr(0x50000000), vec![0xaa, 0x52], Permissions::RX)
        .unwrap();
    let vector = a
        .runtime
        .memory
        .u32(GuestPtr(a.info.toc_or_a5 + 8))
        .unwrap();
    let gate = a.runtime.memory.u32(GuestPtr(vector)).unwrap();
    let stack = STACK_BASE + 0x7000;
    let name = GuestPtr(STACK_BASE + 0x6000);
    let destination = GuestPtr(STACK_BASE + 0x6101);
    let free = a.runtime.process.free_memory();
    a.runtime.process.mem_error = -108;
    a.runtime.process.res_error = -192;
    for (volume, directory, input, error, parent, leaf) in [
        (
            -1i16,
            2u32,
            b"aPpLiCaTiOn".as_slice(),
            0,
            2,
            b"Application".as_slice(),
        ),
        (
            -1,
            2,
            b"New File".as_slice(),
            -43,
            2,
            b"New File".as_slice(),
        ),
        (
            -1,
            2,
            b"".as_slice(),
            0,
            1,
            b"Application Volume".as_slice(),
        ),
        (
            7,
            999,
            b"Application Volume:Application".as_slice(),
            0,
            2,
            b"Application".as_slice(),
        ),
        (7, 2, b"Application".as_slice(), -35, 0, b"".as_slice()),
        (-1, 999, b"Application".as_slice(), -120, 0, b"".as_slice()),
    ] {
        let mut expected = [0; 70];
        if error == 0 || error == -43 {
            expected[..2].copy_from_slice(&(-1i16).to_be_bytes());
            expected[2..6].copy_from_slice(&u32::to_be_bytes(parent));
            expected[6] = leaf.len() as u8;
            expected[7..7 + leaf.len()].copy_from_slice(leaf);
        }
        for architecture in [Architecture::M68k, Architecture::PowerPc] {
            let mut string = vec![input.len() as u8];
            string.extend(input);
            a.runtime.memory.write(name, &string).unwrap();
            a.runtime
                .memory
                .write(GuestPtr(destination.0 - 1), &[0x5a; 72])
                .unwrap();
            let mut frame = destination.0.to_be_bytes().to_vec();
            frame.extend(name.0.to_be_bytes());
            frame.extend(directory.to_be_bytes());
            frame.extend(volume.to_be_bytes());
            frame.extend([0x5a; 6]);
            a.runtime.memory.write(GuestPtr(stack), &frame).unwrap();
            if architecture == Architecture::M68k {
                a.runtime.cpu = Cpu::new(architecture, 0x50000000, stack);
                let Cpu::M68k(c) = &mut a.runtime.cpu else {
                    unreachable!()
                };
                c.dar[0] = 0x12340001;
                c.dar[1] = 0xaabbccdd;
            } else {
                a.runtime.cpu = Cpu::new(architecture, gate, stack);
                let Cpu::PowerPc(c) = &mut a.runtime.cpu else {
                    unreachable!()
                };
                c.gpr[3] = 0xabcd0000 | u32::from(volume as u16);
                c.gpr[4] = directory;
                c.gpr[5] = name.0;
                c.gpr[6] = destination.0;
                c.gpr[7] = 0xaabbccdd;
            }
            assert_eq!(a.runtime.advance(1).unwrap(), Stop::Budget);
            match &a.runtime.cpu {
                Cpu::M68k(c) => {
                    assert_eq!(
                        (c.dar[0], c.dar[1], c.dar[15]),
                        (0x12340001, 0xaabbccdd, stack + 14)
                    );
                    assert_eq!(
                        a.runtime.memory.u16(GuestPtr(stack + 14)).unwrap() as i16,
                        error
                    );
                    assert_eq!(
                        a.runtime.memory.read(GuestPtr(stack), 14).unwrap(),
                        &frame[..14]
                    );
                    assert_eq!(
                        a.runtime.memory.read(GuestPtr(stack + 16), 4).unwrap(),
                        [0x5a; 4]
                    );
                }
                Cpu::PowerPc(c) => assert_eq!(
                    (c.gpr[3] as i32, c.gpr[4], c.gpr[5], c.gpr[6], c.gpr[7]),
                    (
                        i32::from(error),
                        directory,
                        name.0,
                        destination.0,
                        0xaabbccdd
                    )
                ),
            }
            assert_eq!(a.runtime.memory.read(destination, 70).unwrap(), expected);
            assert_eq!(a.runtime.memory.read(name, string.len()).unwrap(), string);
            assert_eq!(
                a.runtime
                    .memory
                    .read(GuestPtr(destination.0 - 1), 1)
                    .unwrap(),
                [0x5a]
            );
            assert_eq!(
                a.runtime
                    .memory
                    .read(GuestPtr(destination.0 + 70), 1)
                    .unwrap(),
                [0x5a]
            );
        }
    }
    assert_eq!(a.runtime.process.free_memory(), free);
    assert_eq!(
        (a.runtime.process.mem_error, a.runtime.process.res_error),
        (-108, -192)
    );
    let entry = traps::lookup(&a.runtime.memory, 0x252, true).unwrap();
    assert_ne!(entry, traps::UNIMPLEMENTED);
    assert_eq!(a.runtime.memory.u16(GuestPtr(entry)).unwrap(), 0xaa52);
}
#[test]
fn frame_name_and_destination_faults_preserve_stack_and_output() {
    for which in 0..5 {
        let mut m = Memory::default();
        m.map(
            GuestPtr(0x1000),
            vec![0; if which == 0 { 15 } else { 20 }],
            Permissions::RW,
        )
        .unwrap();
        m.map(
            GuestPtr(0x2000),
            vec![0x5a; if which == 1 { 69 } else { 70 }],
            if which == 2 {
                Permissions::RX
            } else {
                Permissions::RW
            },
        )
        .unwrap();
        m.map(
            GuestPtr(0x3000),
            vec![4, b'T', b'e', b's', b't'],
            Permissions::RX,
        )
        .unwrap();
        let mut frame = (if which == 3 { 0u32 } else { 0x2000u32 })
            .to_be_bytes()
            .to_vec();
        frame.extend((if which == 4 { 0x3003u32 } else { 0x3000u32 }).to_be_bytes());
        frame.extend(2u32.to_be_bytes());
        frame.extend((-1i16).to_be_bytes());
        m.write(GuestPtr(0x1000), &frame).unwrap();
        let Cpu::M68k(mut c) = Cpu::new(Architecture::M68k, 0, 0x1000) else {
            unreachable!()
        };
        c.dar[0] = 1;
        assert!(Abi::trap(0xaa52, &mut c, &m).is_err());
        assert_eq!(c.dar[15], 0x1000);
        assert_eq!(m.read(GuestPtr(0x1000), 14).unwrap(), frame);
        assert_eq!(
            m.read(GuestPtr(0x2000), if which == 1 { 69 } else { 70 })
                .unwrap(),
            vec![0x5a; if which == 1 { 69 } else { 70 }]
        );
    }
}
