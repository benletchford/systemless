mod support;
use systemless_future::{
    cpu::{Architecture, Cpu},
    files::Fork,
    load::{resource::ResourceFork, LoadedApplication, STACK_BASE},
    memory::{GuestPtr, Permissions},
    runtime::Stop,
};
#[test]
fn both_cpus_create_real_resource_maps_publish_reserror_and_preserve_void_abi() {
    for arch in [Architecture::M68k, Architecture::PowerPc] {
        let fixture = support::pef_program(&[0x4e800020], &[b"FSpCreateResFile"], 16);
        let mut a = LoadedApplication::pef(&fixture.bytes, None).unwrap();
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
        let spec = GuestPtr(STACK_BASE + 0x6000);
        let stack = STACK_BASE + 0x7000;
        let mut bytes = [0; 70];
        bytes[..2].copy_from_slice(&(-1i16).to_be_bytes());
        bytes[2..6].copy_from_slice(&2u32.to_be_bytes());
        bytes[6] = 5;
        bytes[7..12].copy_from_slice(b"Prefs");
        a.runtime.memory.write(spec, &bytes).unwrap();
        let free = a.runtime.process.free_memory();
        a.runtime.process.mem_error = -108;
        let current = a.runtime.memory.u16(GuestPtr(0xa5a)).unwrap();
        for expected in [0, -48] {
            let mut frame = (-1i16).to_be_bytes().to_vec();
            frame.extend(b"pref");
            frame.extend(b"TEST");
            frame.extend(spec.0.to_be_bytes());
            frame.extend([0x5a; 6]);
            a.runtime.memory.write(GuestPtr(stack), &frame).unwrap();
            a.runtime.cpu = Cpu::new(
                arch,
                if arch == Architecture::M68k {
                    0x50000000
                } else {
                    gate
                },
                stack,
            );
            match &mut a.runtime.cpu {
                Cpu::M68k(c) => {
                    c.dar[0] = 14;
                    c.dar[1] = 0xaabbccdd;
                }
                Cpu::PowerPc(c) => {
                    c.gpr[3] = spec.0;
                    c.gpr[4] = u32::from_be_bytes(*b"TEST");
                    c.gpr[5] = u32::from_be_bytes(*b"pref");
                    c.gpr[6] = 0xabcdffff;
                    c.gpr[7] = 0xaabbccdd;
                }
            }
            assert_eq!(a.runtime.advance(1).unwrap(), Stop::Budget);
            match &a.runtime.cpu {
                Cpu::M68k(c) => assert_eq!(
                    (c.dar[0], c.dar[1], c.dar[15]),
                    (14, 0xaabbccdd, stack + 14)
                ),
                Cpu::PowerPc(c) => assert_eq!(
                    (c.gpr[3], c.gpr[4], c.gpr[5], c.gpr[6], c.gpr[7]),
                    (
                        spec.0,
                        u32::from_be_bytes(*b"TEST"),
                        u32::from_be_bytes(*b"pref"),
                        0xabcdffff,
                        0xaabbccdd
                    )
                ),
            }
            assert_eq!(a.runtime.memory.read(GuestPtr(stack), 20).unwrap(), frame);
            assert_eq!(a.runtime.process.res_error, expected);
            assert_eq!(
                a.runtime.memory.u16(GuestPtr(0xa60)).unwrap() as i16,
                expected
            );
            assert_eq!(a.runtime.memory.read(spec, 70).unwrap(), bytes);
            assert_eq!(a.runtime.memory.u16(GuestPtr(0xa5a)).unwrap(), current);
        }
        let view = a.runtime.process.files.directory_view(2).unwrap();
        let node = view.entries.iter().find(|e| e.name == b"Prefs").unwrap().id;
        assert!(a
            .runtime
            .process
            .files
            .catalog_fork(node, Fork::Data)
            .unwrap()
            .is_empty());
        let fork = a
            .runtime
            .process
            .files
            .catalog_fork(node, Fork::Resource)
            .unwrap();
        assert!(fork.len() >= 256);
        assert_eq!(ResourceFork::parse(fork).unwrap().entries().count(), 0);
        let pb = GuestPtr(STACK_BASE + 0x6200);
        let mut block = [0; 108];
        block[18..22].copy_from_slice(&(spec.0 + 6).to_be_bytes());
        block[22..24].copy_from_slice(&(-1i16).to_be_bytes());
        a.runtime.memory.write(pb, &block).unwrap();
        assert_eq!(
            a.runtime
                .process
                .files
                .file_info(&mut a.runtime.memory, pb, false)
                .unwrap(),
            0
        );
        assert_eq!(
            a.runtime.memory.read(GuestPtr(pb.0 + 32), 8).unwrap(),
            b"prefTEST"
        );
        assert_eq!(a.runtime.process.free_memory(), free);
        assert_eq!(a.runtime.process.mem_error, -108);
    }
}
#[test]
fn native_preflight_rejects_bad_frames_specs_and_readonly_reserror_before_popping() {
    use systemless_future::{abi::Abi, memory::Memory};
    for which in 0..4 {
        let mut m = Memory::default();
        m.map(
            GuestPtr(0x1000),
            vec![0; if which == 0 { 13 } else { 20 }],
            Permissions::RW,
        )
        .unwrap();
        m.map(
            GuestPtr(0x2000),
            vec![0; if which == 1 { 69 } else { 70 }],
            Permissions::RX,
        )
        .unwrap();
        m.map(
            GuestPtr(0xa60),
            vec![0x5a; 2],
            if which == 2 {
                Permissions::RX
            } else {
                Permissions::RW
            },
        )
        .unwrap();
        let pointer = if which == 3 { 0u32 } else { 0x2000u32 };
        let mut frame = 0i16.to_be_bytes().to_vec();
        frame.extend(b"pref");
        frame.extend(b"TEST");
        frame.extend(pointer.to_be_bytes());
        if which == 0 {
            m.write(GuestPtr(0x1000), &frame[..13]).unwrap();
        } else {
            m.write(GuestPtr(0x1000), &frame).unwrap();
        }
        let Cpu::M68k(mut c) = Cpu::new(Architecture::M68k, 0, 0x1000) else {
            unreachable!()
        };
        c.dar[0] = 14;
        assert!(Abi::trap(0xaa52, &mut c, &m).is_err());
        assert_eq!(c.dar[15], 0x1000);
        assert_eq!(m.read(GuestPtr(0xa60), 2).unwrap(), [0x5a; 2]);
        assert_eq!(
            m.read(GuestPtr(0x2000), if which == 1 { 69 } else { 70 })
                .unwrap(),
            vec![0; if which == 1 { 69 } else { 70 }]
        );
    }
}
