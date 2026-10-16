mod support;
use systemless_future::{
    cpu::{Architecture, Cpu},
    files::CreateSpecResource,
    load::{LoadedApplication, STACK_BASE},
    memory::{GuestPtr, Permissions},
    runtime::Stop,
    volume::{checkpoint_runtime, Volume},
};
fn application(architecture: Architecture, import: &[u8]) -> LoadedApplication {
    let fixture = support::pef_program(&[0x4e800020], &[import], 16);
    let mut app = LoadedApplication::pef(&fixture.bytes, None).unwrap();
    app.runtime
        .memory
        .map(
            GuestPtr(0x50000000),
            vec![0xa0, 0x13, 0x70, 0x7b],
            Permissions::RX,
        )
        .unwrap();
    let vector = app
        .runtime
        .memory
        .u32(GuestPtr(app.info.toc_or_a5 + 8))
        .unwrap();
    let gate = app.runtime.memory.u32(GuestPtr(vector)).unwrap();
    app.runtime.cpu = Cpu::new(
        architecture,
        if architecture == Architecture::M68k {
            0x50000000
        } else {
            gate
        },
        STACK_BASE + 0x7000,
    );
    match &mut app.runtime.cpu {
        Cpu::M68k(c) => {
            c.dar[0] = 0x12345678;
            c.dar[1] = 0xfeedface;
            c.dar[8] = STACK_BASE + 0x6000;
        }
        Cpu::PowerPc(c) => {
            c.gpr[3] = STACK_BASE + 0x6000;
            c.gpr[4] = 0x100;
            c.gpr[5] = 0xfeedface;
        }
    }
    let mut pb = [0x5a; 24];
    pb[18..24].fill(0);
    app.runtime
        .memory
        .write(GuestPtr(STACK_BASE + 0x6000), &pb)
        .unwrap();
    app
}
#[test]
fn both_cpus_suspend_until_real_checkpoint_and_receive_host_io_failures() {
    let root = std::env::temp_dir().join(format!(
        "systemless-flush-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir(&root).unwrap();
    for architecture in [Architecture::M68k, Architecture::PowerPc] {
        for fail in [false, true] {
            let mut app = application(architecture, b"PBFlushVolSync");
            let image = root.join(format!("{architecture:?}-{fail}.slv"));
            let mut volume = Volume::open(&image, &mut app.runtime.process.files).unwrap();
            let spec = GuestPtr(STACK_BASE + 0x6100);
            let mut bytes = [0; 70];
            bytes[..2].copy_from_slice(&(-1i16).to_be_bytes());
            bytes[2..6].copy_from_slice(&2u32.to_be_bytes());
            bytes[6] = 4;
            bytes[7..11].copy_from_slice(b"Test");
            app.runtime.memory.write(spec, &bytes).unwrap();
            assert_eq!(
                app.runtime
                    .process
                    .files
                    .create_spec_resource(
                        &app.runtime.memory,
                        CreateSpecResource {
                            spec,
                            creator: *b"TEST",
                            kind: *b"pref",
                            script: 0
                        }
                    )
                    .unwrap(),
                0
            );
            let expected = app.runtime.process.files.export_volume().unwrap();
            if fail {
                std::fs::create_dir(&image).unwrap();
            }
            assert_eq!(app.runtime.advance(100).unwrap(), Stop::FlushVolume);
            let instructions = app.runtime.instructions;
            assert_eq!(instructions, 1);
            assert_eq!(app.runtime.advance(0).unwrap(), Stop::FlushVolume);
            assert_eq!(app.runtime.advance(100).unwrap(), Stop::FlushVolume);
            assert_eq!(app.runtime.instructions, instructions);
            let pb = GuestPtr(STACK_BASE + 0x6000);
            assert_eq!(app.runtime.memory.u16(GuestPtr(pb.0 + 16)).unwrap(), 1);
            assert_eq!(app.runtime.memory.u32(GuestPtr(pb.0 + 12)).unwrap(), 0);
            checkpoint_runtime(Some(&mut volume), &mut app.runtime).unwrap();
            let error = if fail { -36i16 } else { 0 };
            assert!(!app.runtime.volume_flush_pending());
            assert_eq!(
                app.runtime.memory.u16(GuestPtr(pb.0 + 16)).unwrap() as i16,
                error
            );
            match &app.runtime.cpu {
                Cpu::M68k(c) => {
                    assert_eq!(c.dar[0] as i32, i32::from(error));
                    assert_eq!(c.dar[1], 0xfeedface);
                    assert_eq!(c.dar[8], pb.0);
                    assert_eq!(c.pc, 0x50000002);
                }
                Cpu::PowerPc(c) => {
                    assert_eq!(c.gpr[3] as i32, i32::from(error));
                    assert_eq!(c.gpr[5], 0xfeedface);
                }
            }
            assert!(app.runtime.complete_volume_flush(0).is_err());
            if fail {
                assert!(image.is_dir());
                std::fs::remove_dir(&image).unwrap();
                // A failed flush retains changed canonical files for a later retry.
                assert!(volume.checkpoint(&app.runtime.process.files).unwrap());
            }
            assert_eq!(std::fs::read(&image).unwrap(), expected);
            assert!(!volume.checkpoint(&app.runtime.process.files).unwrap());
            if architecture == Architecture::M68k {
                assert_eq!(app.runtime.advance(1).unwrap(), Stop::Budget);
                let Cpu::M68k(c) = &app.runtime.cpu else {
                    unreachable!()
                };
                assert_eq!(c.dar[0], 123);
            }
        }
    }
    std::fs::remove_dir_all(root).unwrap();
}
#[test]
fn invalid_volume_is_immediate_and_memory_only_flush_requires_explicit_acknowledgment() {
    for architecture in [Architecture::M68k, Architecture::PowerPc] {
        let mut app = application(architecture, b"PBFlushVol");
        let pb = GuestPtr(STACK_BASE + 0x6000);
        app.runtime
            .memory
            .write(GuestPtr(pb.0 + 22), &(-10i16).to_be_bytes())
            .unwrap();
        assert_eq!(app.runtime.advance(1).unwrap(), Stop::Budget);
        assert!(!app.runtime.volume_flush_pending());
        assert_eq!(
            app.runtime.memory.u16(GuestPtr(pb.0 + 16)).unwrap() as i16,
            -35
        );
        let mut app = application(architecture, b"PBFlushVol");
        assert_eq!(app.runtime.advance(1).unwrap(), Stop::FlushVolume);
        checkpoint_runtime(None, &mut app.runtime).unwrap();
        assert_eq!(app.runtime.memory.u16(GuestPtr(pb.0 + 16)).unwrap(), 0);
        assert!(!app.runtime.volume_flush_pending());
    }
}
#[test]
fn async_import_and_bad_guest_memory_fault_before_a_flush_is_requested() {
    let mut app = application(Architecture::PowerPc, b"PBFlushVol");
    let Cpu::PowerPc(c) = &mut app.runtime.cpu else {
        unreachable!()
    };
    c.gpr[4] = 1;
    assert!(app
        .runtime
        .advance(1)
        .unwrap_err()
        .to_string()
        .contains("asynchronous volume"));
    assert!(!app.runtime.volume_flush_pending());
    for architecture in [Architecture::M68k, Architecture::PowerPc] {
        let mut app = application(architecture, b"PBFlushVolSync");
        match &mut app.runtime.cpu {
            Cpu::M68k(c) => c.dar[8] = 0,
            Cpu::PowerPc(c) => c.gpr[3] = 0,
        }
        assert!(app.runtime.advance(1).is_err());
        assert!(!app.runtime.volume_flush_pending());
    }
}
