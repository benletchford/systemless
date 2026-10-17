mod support;
use systemless_future::{
    cpu::{Architecture, Cpu},
    display::Display,
    drivers::VIDEO_REFERENCE,
    graphics::Graphics,
    load::{LoadedApplication, STACK_BASE},
    memory::{GuestPtr, Permissions},
    quickdraw::SCREEN,
    runtime::Stop,
};
const DATA: u32 = 0x51000000;
const CODE: u32 = 0x50000000;
const STACK: u32 = STACK_BASE + 0x7000;
fn app() -> LoadedApplication {
    setup(Some(Display::indexed8(7, 3).unwrap()))
}
fn setup(display: Option<Display>) -> LoadedApplication {
    let f = support::pef_program(&[0x4e800020], &[b"PBControlSync", b"SetEntries"], 32);
    let mut a = LoadedApplication::pef(&f.bytes, None).unwrap();
    if let Some(display) = display {
        a = a.with_display(display).unwrap();
    }
    a.runtime
        .memory
        .map(GuestPtr(DATA), vec![0xa5; 256], Permissions::RW)
        .unwrap();
    a.runtime
        .memory
        .map(
            GuestPtr(CODE),
            vec![0xa0, 0x04, 0xaa, 0x3f],
            Permissions::RX,
        )
        .unwrap();
    a
}
fn pb(a: &mut LoadedApplication, reference: i16, code: i16, start: i16, count: i16) {
    let mut bytes = [0xa5; 32];
    bytes[24..26].copy_from_slice(&reference.to_be_bytes());
    bytes[26..28].copy_from_slice(&code.to_be_bytes());
    bytes[28..32].copy_from_slice(&(DATA + 64).to_be_bytes());
    a.runtime.memory.write(GuestPtr(DATA), &bytes).unwrap();
    let mut r = (DATA + 80).to_be_bytes().to_vec();
    r.extend(start.to_be_bytes());
    r.extend(count.to_be_bytes());
    a.runtime.memory.write(GuestPtr(DATA + 64), &r).unwrap();
}
fn vector(a: &LoadedApplication, index: u32) -> u32 {
    let tv = a
        .runtime
        .memory
        .u32(GuestPtr(a.info.toc_or_a5 + 8 + index * 4))
        .unwrap();
    a.runtime.memory.u32(GuestPtr(tv)).unwrap()
}
fn control(a: &mut LoadedApplication, arch: Architecture) -> i16 {
    let pc = if arch == Architecture::M68k {
        CODE
    } else {
        vector(a, 0)
    };
    a.runtime.cpu = Cpu::new(arch, pc, STACK);
    match &mut a.runtime.cpu {
        Cpu::M68k(c) => {
            for i in 0..15 {
                c.dar[i] = 0x12340000 + i as u32;
            }
            c.dar[8] = DATA;
        }
        Cpu::PowerPc(c) => c.gpr[3] = DATA,
    }
    a.runtime.process.mem_error = -99;
    a.runtime.process.res_error = -98;
    assert_eq!(a.runtime.advance(1).unwrap(), Stop::Budget);
    let value = match &a.runtime.cpu {
        Cpu::M68k(c) => {
            for i in 1..15 {
                assert_eq!(c.dar[i], if i == 8 { DATA } else { 0x12340000 + i as u32 });
            }
            assert_eq!(c.dar[15], STACK);
            c.dar[0]
        }
        Cpu::PowerPc(c) => c.gpr[3],
    };
    let error = a.runtime.memory.u16(GuestPtr(DATA + 16)).unwrap() as i16;
    assert_eq!(value, error as i32 as u32);
    assert_eq!(
        (a.runtime.process.mem_error, a.runtime.process.res_error),
        (-99, -98)
    );
    error
}
fn entries(a: &mut LoadedApplication, values: &[(u16, [u16; 3])]) {
    let b: Vec<_> = values
        .iter()
        .flat_map(|(i, c)| [*i, c[0], c[1], c[2]])
        .flat_map(u16::to_be_bytes)
        .collect();
    a.runtime.memory.write(GuestPtr(DATA + 80), &b).unwrap();
}
fn colors(a: &LoadedApplication) -> u32 {
    let gd = a
        .runtime
        .memory
        .u32(systemless_future::devices::MAIN)
        .unwrap();
    let d = a.runtime.memory.u32(GuestPtr(gd)).unwrap();
    assert_eq!(
        a.runtime.memory.u16(GuestPtr(d)).unwrap() as i16,
        VIDEO_REFERENCE
    );
    let pm = a.runtime.memory.u32(GuestPtr(d + 22)).unwrap();
    let p = a.runtime.memory.u32(GuestPtr(pm)).unwrap();
    let t = a.runtime.memory.u32(GuestPtr(p + 42)).unwrap();
    a.runtime.memory.u32(GuestPtr(t)).unwrap()
}
#[test]
fn both_cpus_publish_driver_errors_and_validate_whole_requests_before_palette_changes() {
    for arch in [Architecture::M68k, Architecture::PowerPc] {
        let mut a = app();
        colors(&a);
        for (reference, code, start, count, expected) in [
            (0, 3, 0, 0, -21),
            (7, 3, 0, 0, -21),
            (-4, 3, 0, 0, -17),
            (VIDEO_REFERENCE, 9, 0, 0, -17),
            (VIDEO_REFERENCE, 3, 255, 1, -50),
            (VIDEO_REFERENCE, 3, 0, 256, -50),
            (VIDEO_REFERENCE, 3, 0, -2, -50),
            (VIDEO_REFERENCE, 3, 256, -1, -50),
        ] {
            pb(&mut a, reference, code, start, count);
            assert_eq!(control(&mut a, arch), expected);
        }
        pb(&mut a, VIDEO_REFERENCE, 3, -1, 1);
        entries(&mut a, &[(0, [65535, 0, 0]), (256, [0, 65535, 0])]);
        assert_eq!(control(&mut a, arch), -50);
        pb(&mut a, VIDEO_REFERENCE, 3, 0, -1);
        a.runtime
            .memory
            .write(GuestPtr(DATA + 64), &0u32.to_be_bytes())
            .unwrap();
        assert_eq!(control(&mut a, arch), 0);
        // A missing driver ignores even an unusable driver-specific pointer.
        pb(&mut a, 0, 3, 0, 0);
        a.runtime
            .memory
            .write(GuestPtr(DATA + 28), &u32::MAX.to_be_bytes())
            .unwrap();
        assert_eq!(control(&mut a, arch), -21);
    }
}
#[test]
#[ignore = "requires a real wgpu adapter"]
fn gpu_video_palette_is_independent_of_guest_matching_table_and_high_level_updates_patch_both() {
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
    for arch in [Architecture::M68k, Architecture::PowerPc] {
        let mut a = app();
        let table = colors(&a);
        let original = a
            .runtime
            .memory
            .read(GuestPtr(table), 2064)
            .unwrap()
            .to_vec();
        let pixels: Vec<_> = (0..3).flat_map(|_| [0u8, 1, 0, 1, 0, 1, 0, 0xa5]).collect();
        a.runtime.memory.write(SCREEN, &pixels).unwrap();
        a.runtime.graphics = Some(
            pollster::block_on(Graphics::new(&instance, None, 7, 3, GuestPtr(0x4000))).unwrap(),
        );
        // Driver reference selects the physical screen even with an offscreen
        // GDevice selected; no driver call may silently change that selection.
        let main = a
            .runtime
            .memory
            .u32(systemless_future::devices::MAIN)
            .unwrap();
        let mut devices = std::mem::take(&mut a.runtime.process.devices);
        let off = devices
            .call(
                &mut a.runtime.memory,
                &mut a.runtime.process,
                systemless_future::devices::Op::New {
                    reference: 0,
                    mode: -1,
                },
            )
            .unwrap();
        devices
            .call(
                &mut a.runtime.memory,
                &mut a.runtime.process,
                systemless_future::devices::Op::Select(off),
            )
            .unwrap();
        a.runtime.process.devices = devices;
        let free = a.runtime.process.free_memory();
        pb(&mut a, VIDEO_REFERENCE, 3, 0, 1);
        entries(&mut a, &[(255, [65535, 0, 0]), (255, [0, 65535, 0])]);
        assert_eq!(control(&mut a, arch), 0);
        assert_eq!(
            a.runtime
                .memory
                .u32(systemless_future::devices::CURRENT)
                .unwrap(),
            off
        );
        assert_eq!(a.runtime.process.free_memory(), free);
        let mut devices = std::mem::take(&mut a.runtime.process.devices);
        devices
            .call(
                &mut a.runtime.memory,
                &mut a.runtime.process,
                systemless_future::devices::Op::Select(main),
            )
            .unwrap();
        a.runtime.process.devices = devices;
        assert_eq!(
            a.runtime.memory.read(GuestPtr(table), 2064).unwrap(),
            original
        );
        assert_eq!(a.runtime.memory.read(SCREEN, pixels.len()).unwrap(), pixels);
        a.runtime.refresh_screen().unwrap();
        let image = a.runtime.graphics.as_ref().unwrap().read_rgba().unwrap();
        for (i, p) in image.chunks_exact(4).enumerate() {
            assert_eq!(
                p,
                if i % 7 % 2 == 0 {
                    [255, 0, 0, 255]
                } else {
                    [0, 255, 0, 255]
                }
            );
        }
        // Direct guest matching-table edits do not reprogram the physical CLUT.
        a.runtime
            .memory
            .write(GuestPtr(table + 10), &[0, 0, 0, 0, 255, 255])
            .unwrap();
        a.runtime.refresh_screen().unwrap();
        assert_eq!(
            a.runtime.graphics.as_ref().unwrap().read_rgba().unwrap(),
            image
        );
        // Indexed duplicate writes use the last value; a later invalid index is atomic.
        pb(&mut a, VIDEO_REFERENCE, 3, -1, 1);
        entries(&mut a, &[(1, [0, 0, 65535]), (1, [65535, 65535, 0])]);
        assert_eq!(control(&mut a, arch), 0);
        a.runtime.refresh_screen().unwrap();
        let changed = a.runtime.graphics.as_ref().unwrap().read_rgba().unwrap();
        assert_eq!(&changed[4..8], &[255, 255, 0, 255]);
        entries(&mut a, &[(0, [0, 0, 0]), (256, [0, 0, 0])]);
        assert_eq!(control(&mut a, arch), -50);
        a.runtime.refresh_screen().unwrap();
        assert_eq!(
            a.runtime.graphics.as_ref().unwrap().read_rgba().unwrap(),
            changed
        );
        // High-level SetEntries changes matching metadata and the corresponding hardware entry.
        entries(&mut a, &[(0, [65535, 0, 65535])]);
        let pc = if arch == Architecture::M68k {
            CODE + 2
        } else {
            vector(&a, 1)
        };
        a.runtime.cpu = Cpu::new(arch, pc, STACK);
        match &mut a.runtime.cpu {
            Cpu::M68k(_) => {
                let mut f = (DATA + 80).to_be_bytes().to_vec();
                f.extend(0u16.to_be_bytes());
                f.extend(1u16.to_be_bytes());
                a.runtime.memory.write(GuestPtr(STACK), &f).unwrap();
            }
            Cpu::PowerPc(c) => {
                c.gpr[3] = 1;
                c.gpr[4] = 0;
                c.gpr[5] = DATA + 80;
            }
        }
        assert_eq!(a.runtime.advance(1).unwrap(), Stop::Budget);
        assert_eq!(
            a.runtime.memory.read(GuestPtr(table + 18), 6).unwrap(),
            [255, 255, 0, 0, 255, 255]
        );
        a.runtime.refresh_screen().unwrap();
        let last = a.runtime.graphics.as_ref().unwrap().read_rgba().unwrap();
        assert_eq!(&last[..4], &[255, 0, 0, 255]);
        assert_eq!(&last[4..8], &[255, 0, 255, 255]);
        assert_eq!(a.runtime.memory.read(SCREEN, pixels.len()).unwrap(), pixels);
        // The result word may alias the input ColorSpec.value: snapshot first.
        pb(&mut a, VIDEO_REFERENCE, 3, -1, 0);
        a.runtime
            .memory
            .write(GuestPtr(DATA + 64), &(DATA + 16).to_be_bytes())
            .unwrap();
        let alias: Vec<_> = [1u16, 0, 65535, 65535]
            .into_iter()
            .flat_map(u16::to_be_bytes)
            .collect();
        a.runtime.memory.write(GuestPtr(DATA + 16), &alias).unwrap();
        assert_eq!(control(&mut a, arch), 0);
        a.runtime.refresh_screen().unwrap();
        let alias_image = a.runtime.graphics.as_ref().unwrap().read_rgba().unwrap();
        assert_eq!(&alias_image[4..8], &[0, 255, 255, 255]);
        // Read-only ioResult must fail before dereferencing bad driver params.
        let mut readonly = [0u8; 32];
        readonly[24..26].copy_from_slice(&VIDEO_REFERENCE.to_be_bytes());
        readonly[26..28].copy_from_slice(&3i16.to_be_bytes());
        readonly[28..].copy_from_slice(&u32::MAX.to_be_bytes());
        a.runtime
            .memory
            .map(GuestPtr(0x52000000), readonly.to_vec(), Permissions::RX)
            .unwrap();
        let error = a
            .runtime
            .process
            .drivers
            .control(
                &mut a.runtime.memory,
                Display::indexed8(7, 3).unwrap(),
                GuestPtr(0x52000000),
            )
            .unwrap_err();
        assert!(matches!(
            error,
            systemless_future::Error::Memory {
                address: 0x52000010,
                access: systemless_future::memory::Access::Write
            }
        ));
        a.runtime.refresh_screen().unwrap();
        assert_eq!(
            a.runtime.graphics.as_ref().unwrap().read_rgba().unwrap(),
            alias_image
        );
    }
}

#[test]
#[ignore = "requires a real wgpu adapter"]
fn fixed_monochrome_presentation_ignores_matching_metadata_and_transient_gpu_palette() {
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
    let mut a = setup(None);
    let mut devices = std::mem::take(&mut a.runtime.process.devices);
    devices
        .call(
            &mut a.runtime.memory,
            &mut a.runtime.process,
            systemless_future::devices::Op::Main,
        )
        .unwrap();
    a.runtime.process.devices = devices;
    let table = colors(&a);
    a.runtime.memory.write(SCREEN, &[0x80]).unwrap();
    let mut gpu =
        pollster::block_on(Graphics::new(&instance, None, 512, 342, GuestPtr(0x4000))).unwrap();
    gpu.palette[0] = [255, 0, 0, 255];
    gpu.palette[1] = [0, 255, 0, 255];
    a.runtime.graphics = Some(gpu);
    a.runtime.refresh_screen().unwrap();
    let image = a.runtime.graphics.as_ref().unwrap().read_rgba().unwrap();
    assert_eq!(&image[..4], &[0, 0, 0, 255]);
    assert_eq!(&image[4..8], &[255; 4]);
    // Even corrupt QuickDraw matching metadata cannot change fixed hardware colors.
    a.runtime
        .memory
        .write(GuestPtr(table + 4), &[0xff; 4])
        .unwrap();
    a.runtime.refresh_screen().unwrap();
    assert_eq!(
        a.runtime.graphics.as_ref().unwrap().read_rgba().unwrap(),
        image
    );
    pb(&mut a, VIDEO_REFERENCE, 3, 0, 0);
    a.runtime
        .memory
        .write(GuestPtr(DATA + 28), &u32::MAX.to_be_bytes())
        .unwrap();
    assert_eq!(control(&mut a, Architecture::M68k), -17);
    a.runtime.refresh_screen().unwrap();
    assert_eq!(
        a.runtime.graphics.as_ref().unwrap().read_rgba().unwrap(),
        image
    );
}
