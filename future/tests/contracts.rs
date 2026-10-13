use systemless_future::{
    cpu::{Architecture, Cpu, IMPORT_BASE},
    fixtures,
    graphics::{Draw, Graphics, Rect, Transfer},
    memory::{Access, GuestPtr, Memory, Permissions},
    runtime::Stop,
    Error,
};
#[test]
fn shared_service_from_real_cpu_instructions() {
    for arch in [Architecture::M68k, Architecture::PowerPc] {
        let mut rt = fixtures::runtime(arch, false).unwrap();
        assert_eq!(rt.advance(0).unwrap(), Stop::Budget);
        assert_eq!(rt.advance(1).unwrap(), Stop::Budget);
        assert_eq!(rt.advance(20).unwrap(), Stop::Exit(0));
        assert_eq!(rt.services.accumulator, 7);
        assert_eq!(rt.services.calls.len(), 2);
        let ticks = rt.instructions;
        assert_eq!(rt.advance(10).unwrap(), Stop::Exit(0));
        assert_eq!(rt.instructions, ticks);
    }
}
#[test]
fn memory_endian_permissions_boundaries_and_atomic_write() {
    let mut m = Memory::default();
    m.map(GuestPtr(0x1000), vec![1, 2, 3, 4], Permissions::RW)
        .unwrap();
    assert_eq!(m.u32(GuestPtr(0x1000)).unwrap(), 0x01020304);
    assert!(m.write(GuestPtr(0x1003), &[8, 9]).is_err());
    assert_eq!(m.read(GuestPtr(0x1003), 1).unwrap(), [4]);
    assert!(m
        .map(GuestPtr(0x1002), vec![0; 4], Permissions::RW)
        .is_err());
    assert!(m.fetch(GuestPtr(0x1000), 2, 2).is_err());
    m.map(GuestPtr(u32::MAX - 1), vec![9, 8], Permissions::RX)
        .unwrap();
    assert!(m.u32(GuestPtr(u32::MAX - 1)).is_err());
    assert!(m.write(GuestPtr(u32::MAX - 1), &[0]).is_err());
    assert!(m.fetch(GuestPtr(u32::MAX), 1, 2).is_err());
}
#[test]
fn both_cpus_fault_on_unmapped_store_and_stop() {
    for arch in [Architecture::M68k, Architecture::PowerPc] {
        let code = match arch {
            Architecture::M68k => vec![0x13, 0xfc, 0, 5, 0, 0, 0x70, 0],
            Architecture::PowerPc => 0x98607000u32.to_be_bytes().to_vec(), // stb r3,0x7000(0)
        };
        let mut rt = fixtures::runtime(arch, false).unwrap();
        rt.memory
            .map(GuestPtr(0x9000), code, Permissions::RX)
            .unwrap();
        rt.cpu = Cpu::new(arch, 0x9000, 0x2ff0);
        assert!(matches!(
            rt.advance(1),
            Err(Error::Memory {
                address: 0x7000,
                access: Access::Write
            })
        ));
        assert!(rt.advance(1).is_err());
    }
}
#[test]
fn loops_are_bounded_and_unknown_instructions_fault() {
    for arch in [Architecture::M68k, Architecture::PowerPc] {
        let loop_code = if arch == Architecture::M68k {
            vec![0x60, 0xfe]
        } else {
            0x48000000u32.to_be_bytes().to_vec()
        };
        let mut rt = fixtures::runtime(arch, false).unwrap();
        rt.memory
            .map(GuestPtr(0x9000), loop_code, Permissions::RX)
            .unwrap();
        rt.cpu = Cpu::new(arch, 0x9000, 0x2ff0);
        assert_eq!(rt.advance(73).unwrap(), Stop::Budget);
        assert_eq!(rt.instructions, 73);
        rt.memory
            .map(
                GuestPtr(0xa000),
                if arch == Architecture::M68k {
                    vec![0x4a, 0xfc, 0xff, 0xff]
                } else {
                    vec![0; 4]
                },
                Permissions::RX,
            )
            .unwrap();
        rt.cpu = Cpu::new(arch, 0xa000, 0x2ff0);
        assert!(rt.advance(1).is_err());
    }
}
#[test]
fn nested_cross_architecture_callbacks_preserve_caller_and_shared_state() {
    // 68K calls PPC, which calls 68K. Both return through real RTS/BLR.
    let mut rt = fixtures::runtime(Architecture::M68k, false).unwrap();
    let outer: [u16; 6] = [0x2f3c, 0, 0x6000, 0xa002, 0x4e75, 0x4e71];
    rt.memory
        .map(
            GuestPtr(0x9000),
            outer.into_iter().flat_map(u16::to_be_bytes).collect(),
            Permissions::RX,
        )
        .unwrap();
    // PPC saves LR in r30 across its nested import and restores it before BLR.
    let middle = [
        0x7fc802a6,
        0x38606010,
        fixtures::branch_link(0xa008, IMPORT_BASE + 8),
        0x7fc803a6,
        0x4e800020,
    ];
    rt.memory
        .map(
            GuestPtr(0xa000),
            middle.into_iter().flat_map(u32::to_be_bytes).collect(),
            Permissions::RX,
        )
        .unwrap();
    let inner: [u16; 6] = [0x2f3c, 0, 11, 0xa000, 0x4e75, 0x4e71];
    rt.memory
        .map(
            GuestPtr(0xb000),
            inner.into_iter().flat_map(u16::to_be_bytes).collect(),
            Permissions::RX,
        )
        .unwrap();
    let mut descriptors = vec![0; 32];
    descriptors[0..4].copy_from_slice(&1u32.to_be_bytes());
    descriptors[4..8].copy_from_slice(&0xa000u32.to_be_bytes());
    descriptors[8..12].copy_from_slice(&0x12345678u32.to_be_bytes());
    descriptors[20..24].copy_from_slice(&0xb000u32.to_be_bytes());
    rt.memory
        .map(GuestPtr(0x6000), descriptors, Permissions::RW)
        .unwrap();
    rt.cpu = Cpu::new(Architecture::M68k, 0x9000, 0x2ff0);
    if let Cpu::M68k(c) = &mut rt.cpu {
        c.dar[5] = 0xabcdef;
        c.set_sr(0x10);
    }
    for _ in 0..100 {
        if rt.advance(1).unwrap() == Stop::Returned(11) {
            break;
        }
    }
    assert_eq!(rt.advance(1).unwrap(), Stop::Returned(11));
    assert_eq!(rt.services.accumulator, 11);
    if let Cpu::M68k(c) = &rt.cpu {
        assert_eq!(c.dar[5], 0xabcdef);
        assert_eq!(c.dar[15], 0x2ff4);
    } else {
        panic!("caller not restored")
    }
}
fn gpu() -> Graphics {
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
    let gpu = pollster::block_on(Graphics::new(
        &instance,
        None,
        8,
        8,
        GuestPtr(fixtures::PIXELS),
    ))
    .expect("GPU acceptance requires a real adapter");
    eprintln!("adapter: {:?}", gpu.adapter.get_info());
    gpu
}
#[test]
#[ignore = "requires a real GPU; run explicitly with --ignored --nocapture"]
fn gpu_dual_cpu_direct_write_then_draw_and_rgba_parity() {
    let mut outputs = Vec::new();
    for arch in [Architecture::M68k, Architecture::PowerPc] {
        let mut rt = fixtures::runtime(arch, true).unwrap();
        let mut g = gpu();
        g.palette[6] = [12, 34, 56, 255];
        rt.graphics = Some(g);
        assert_eq!(rt.advance(100).unwrap(), Stop::Exit(0));
        let indices = rt.memory.read(GuestPtr(fixtures::PIXELS), 64).unwrap();
        assert_eq!(indices[0], 5 ^ 3);
        assert_eq!(indices[1], 3);
        assert_eq!(indices[4], 0);
        let mut expected = vec![0u8; 64];
        for y in 0..4 {
            for x in 0..4 {
                expected[y * 8 + x] = 3;
            }
        }
        expected[0] = 6;
        assert_eq!(indices, expected);
        let rgba = rt.graphics.as_ref().unwrap().read_rgba().unwrap();
        assert_eq!(&rgba[..4], &[12, 34, 56, 255]);
        outputs.push(rgba);
    }
    assert_eq!(outputs[0], outputs[1]);
}
#[test]
#[ignore = "requires a real GPU; run explicitly with --ignored --nocapture"]
fn gpu_clip_patterns_transfers_overlap_and_palette() {
    let mut rt = fixtures::runtime(Architecture::M68k, false).unwrap();
    let mut g = gpu();
    let all = Rect::new(0, 0, 8, 8);
    g.execute(
        &mut rt.memory,
        Draw::Fill {
            rect: all,
            clip: Rect::new(1, 1, 7, 7),
            pattern: [0xaa; 8],
            foreground: 3,
            background: 5,
            mode: Transfer::Copy,
        },
    )
    .unwrap();
    let b = rt.memory.read(g.base, 64).unwrap();
    assert_eq!(b[0], 0);
    assert_eq!(b[9], 5);
    assert_eq!(b[10], 3);
    assert_eq!(b[15], 0);
    for (mode, expected) in [(Transfer::Or, 7), (Transfer::Xor, 1), (Transfer::Bic, 1)] {
        g.execute(
            &mut rt.memory,
            Draw::Fill {
                rect: all,
                clip: Rect::new(1, 1, 2, 2),
                pattern: [255; 8],
                foreground: 6,
                background: 0,
                mode,
            },
        )
        .unwrap();
        assert_eq!(rt.memory.read(g.base, 64).unwrap()[9], expected);
    }
    rt.memory.write(g.base, &[1, 2, 3, 4, 5, 6, 7, 8]).unwrap();
    g.execute(
        &mut rt.memory,
        Draw::Copy {
            rect: Rect::new(1, 0, 8, 1),
            clip: all,
            source_x: 0,
            source_y: 0,
            mode: Transfer::Copy,
        },
    )
    .unwrap();
    assert_eq!(rt.memory.read(g.base, 8).unwrap(), [1, 1, 2, 3, 4, 5, 6, 7]);
    // Out-of-source pixels are clipped even for XOR, not treated as copies of dst.
    let before = rt.memory.read(g.base, 64).unwrap().to_vec();
    g.execute(
        &mut rt.memory,
        Draw::Copy {
            rect: all,
            clip: all,
            source_x: -10,
            source_y: -10,
            mode: Transfer::Xor,
        },
    )
    .unwrap();
    assert_eq!(rt.memory.read(g.base, 64).unwrap(), before);
    g.palette[1] = [200, 100, 50, 255];
    g.execute(&mut rt.memory, Draw::Present).unwrap();
    assert_eq!(&g.read_rgba().unwrap()[..4], &[200, 100, 50, 255]);
}
#[test]
fn instruction_extension_must_be_executable() {
    let mut rt = fixtures::runtime(Architecture::M68k, false).unwrap();
    rt.memory
        .map(GuestPtr(0x9000), vec![0x20, 0x3c], Permissions::RX)
        .unwrap(); // move.l #imm,d0
    rt.memory
        .map(GuestPtr(0x9002), vec![0; 4], Permissions::RW)
        .unwrap();
    rt.cpu = Cpu::new(Architecture::M68k, 0x9000, 0x2ff0);
    assert!(matches!(
        rt.advance(1),
        Err(Error::Memory {
            access: Access::Execute,
            ..
        })
    ));
}
#[test]
fn successive_architectures_share_one_live_service_and_address_space() {
    let mut rt = fixtures::runtime(Architecture::M68k, false).unwrap();
    // Single-step through the first Add, then select the second architecture
    // without replacing the Macintosh memory or service state.
    assert_eq!(rt.advance(2).unwrap(), Stop::Budget);
    assert_eq!(rt.services.accumulator, 7);
    let code = [0x38600009u32, fixtures::branch_link(0x9004, IMPORT_BASE)];
    rt.memory
        .map(
            GuestPtr(0x9000),
            code.into_iter().flat_map(u32::to_be_bytes).collect(),
            Permissions::RX,
        )
        .unwrap();
    rt.cpu = Cpu::new(Architecture::PowerPc, 0x9000, 0x2ff0);
    assert_eq!(rt.advance(3).unwrap(), Stop::Budget);
    assert_eq!(rt.services.accumulator, 16);
    assert_eq!(rt.cpu.result(), 16);
    assert_eq!(rt.services.calls.len(), 2);
}
#[test]
fn ppc_callback_restores_toc_stack_lr_and_nonvolatile_registers() {
    let mut rt = fixtures::runtime(Architecture::PowerPc, false).unwrap();
    let code = [
        0x38606000u32,
        fixtures::branch_link(0x9004, IMPORT_BASE + 8),
        0x60000000,
    ];
    rt.memory
        .map(
            GuestPtr(0x9000),
            code.into_iter().flat_map(u32::to_be_bytes).collect(),
            Permissions::RX,
        )
        .unwrap();
    let callee: [u16; 8] = [0x2f3c, 0, 19, 0xa000, 0x2c3c, 0xdead, 0xbeef, 0x4e75];
    rt.memory
        .map(
            GuestPtr(0xa000),
            callee.into_iter().flat_map(u16::to_be_bytes).collect(),
            Permissions::RX,
        )
        .unwrap();
    let descriptor = [0u32, 0xa000, 0];
    rt.memory
        .map(
            GuestPtr(0x6000),
            descriptor.into_iter().flat_map(u32::to_be_bytes).collect(),
            Permissions::RW,
        )
        .unwrap();
    rt.cpu = Cpu::new(Architecture::PowerPc, 0x9000, 0x2ff0);
    if let Cpu::PowerPc(c) = &mut rt.cpu {
        c.gpr[2] = 0x12345678;
        c.gpr[30] = 0xabcdef;
        c.cr = 0xfedcba98;
    }
    assert_eq!(rt.advance(8).unwrap(), Stop::Budget);
    if let Cpu::PowerPc(c) = &rt.cpu {
        assert_eq!(c.gpr[1], 0x2ff0);
        assert_eq!(c.gpr[2], 0x12345678);
        assert_eq!(c.gpr[30], 0xabcdef);
        assert_eq!(c.gpr[3], 19);
        assert_eq!(c.cr, 0xfedcba98);
        assert_eq!(c.lr, 0x9008);
        assert_eq!(c.pc, 0x9008);
    } else {
        panic!("PPC caller not restored")
    }
}
#[test]
fn unsupported_gate_and_bad_stack_fault_without_dispatch() {
    let mut rt = fixtures::runtime(Architecture::M68k, false).unwrap();
    rt.memory
        .map(GuestPtr(0x9000), vec![0xaf, 0xff], Permissions::RX)
        .unwrap();
    rt.cpu = Cpu::new(Architecture::M68k, 0x9000, 0x2ff0);
    assert!(matches!(rt.advance(1), Err(Error::Service(0xfff))));
    assert!(rt.services.calls.is_empty());
    let mut rt = fixtures::runtime(Architecture::M68k, false).unwrap();
    rt.memory
        .map(GuestPtr(0x9000), vec![0xa0, 0x00], Permissions::RX)
        .unwrap();
    rt.cpu = Cpu::new(Architecture::M68k, 0x9000, 0xfffffff0);
    assert!(matches!(rt.advance(1), Err(Error::Memory { .. })));
    assert!(rt.services.calls.is_empty());
}
