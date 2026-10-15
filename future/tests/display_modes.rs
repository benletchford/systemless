mod support;
use systemless_future::{
    abi::Abi,
    cpu::{Architecture, Cpu},
    devices,
    display::Display,
    graphics::Graphics,
    load::{LoadedApplication, STACK_BASE},
    memory::{GuestPtr, Memory, Permissions},
    quickdraw::SCREEN,
    runtime::Stop,
};
const STACK: u32 = STACK_BASE + 0x8000;
const CODE: u32 = 0x50000000;
fn app(color: bool) -> LoadedApplication {
    let fixture = support::pef_program(
        &[0x4e800020],
        &[b"GetMainDevice", b"HasDepth", b"SetDepth"],
        32,
    );
    let a = LoadedApplication::pef(&fixture.bytes, None).unwrap();
    let mut a = if color {
        a.with_display(Display::indexed8(32, 24).unwrap()).unwrap()
    } else {
        a
    };
    a.runtime
        .memory
        .map(GuestPtr(CODE), vec![0xaa, 0xa2], Permissions::RX)
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
    a.runtime.cpu = Cpu::new(Architecture::PowerPc, pc, STACK);
    let Cpu::PowerPc(c) = &mut a.runtime.cpu else {
        panic!()
    };
    for (i, &arg) in args.iter().enumerate() {
        c.gpr[3 + i] = arg;
    }
    c.gpr[7] = 0xabcdef01;
    assert_eq!(a.runtime.advance(1).unwrap(), Stop::Budget);
    let Cpu::PowerPc(c) = &a.runtime.cpu else {
        panic!()
    };
    assert_eq!(c.gpr[7], 0xabcdef01);
    assert_eq!(c.gpr[1], STACK);
    c.gpr[3]
}
fn native(
    a: &mut LoadedApplication,
    set: bool,
    device: u32,
    depth: i16,
    mask: u16,
    flags: u16,
) -> u32 {
    let mut frame = Vec::new();
    frame.extend(flags.to_be_bytes());
    frame.extend(mask.to_be_bytes());
    frame.extend(depth.to_be_bytes());
    frame.extend(device.to_be_bytes());
    frame.extend([0xa5; 6]);
    a.runtime.memory.write(GuestPtr(STACK), &frame).unwrap();
    a.runtime.cpu = Cpu::new(Architecture::M68k, CODE, STACK);
    let Cpu::M68k(c) = &mut a.runtime.cpu else {
        panic!()
    };
    c.dar[0] = 0xcafe0000 | if set { 0x0a13 } else { 0x0a14 };
    c.dar[1] = 0x12345678;
    assert_eq!(a.runtime.advance(1).unwrap(), Stop::Budget);
    let Cpu::M68k(c) = &a.runtime.cpu else {
        panic!()
    };
    assert_eq!(c.dar[15], STACK + 10);
    assert_eq!(c.dar[0], 0xcafe0000 | if set { 0x0a13 } else { 0x0a14 });
    assert_eq!(c.dar[1], 0x12345678);
    assert_eq!(
        a.runtime.memory.read(GuestPtr(STACK + 12), 4).unwrap(),
        [0xa5; 4]
    );
    a.runtime.memory.u16(GuestPtr(STACK + 10)).unwrap() as i16 as i32 as u32
}
#[test]
fn both_abis_query_and_select_only_modes_backed_by_the_actual_screen() {
    for color in [false, true] {
        let mut a = app(color);
        let device = ppc(&mut a, 0, &[]);
        let gd = a.runtime.memory.u32(GuestPtr(device)).unwrap();
        let depth = a.runtime.display().depth();
        let mode = a.runtime.memory.u32(GuestPtr(gd + 42)).unwrap();
        assert_ne!(mode, 0);
        let pm_handle = a.runtime.memory.u32(GuestPtr(gd + 22)).unwrap();
        let pm = a.runtime.memory.u32(GuestPtr(pm_handle)).unwrap();
        assert_eq!(a.runtime.memory.u16(GuestPtr(pm + 32)).unwrap(), depth);
        assert_eq!(a.runtime.memory.u32(GuestPtr(pm)).unwrap(), SCREEN.0);
        a.runtime.memory.write(SCREEN, &[0x91, 0x27]).unwrap();
        let record = a.runtime.memory.read(GuestPtr(gd), 62).unwrap().to_vec();
        let pixels = a.runtime.memory.read(GuestPtr(pm), 50).unwrap().to_vec();
        a.runtime.process.mem_error = -99;
        a.runtime.process.res_error = -98;
        for (wanted, mask, flags, supported) in [
            (depth as i16, 1, u16::from(color), true),
            (depth as i16, 1, u16::from(!color), false),
            (depth as i16, 0, 0xffff, true),
            (depth as i16, 2, 0, false),
            (if color { 1 } else { 8 }, 0, 0, false),
            (16, 0, 0, false),
            (32, 0, 0, false),
            (-1, 0, 0, false),
        ] {
            let args = [device, wanted as i32 as u32, mask as u32, flags as u32];
            let expected = if supported { mode } else { 0 };
            assert_eq!(ppc(&mut a, 1, &args), expected);
            assert_eq!(native(&mut a, false, device, wanted, mask, flags), expected);
            let expected = if supported { 0 } else { -157i32 as u32 };
            assert_eq!(ppc(&mut a, 2, &args), expected);
            assert_eq!(native(&mut a, true, device, wanted, mask, flags), expected);
        }
        assert_eq!(ppc(&mut a, 2, &[device, mode, 1, u32::from(color)]), 0);
        assert_eq!(
            native(&mut a, true, device, mode as i16, 1, u16::from(color)),
            0
        );
        assert_eq!(a.runtime.memory.read(GuestPtr(gd), 62).unwrap(), record);
        assert_eq!(a.runtime.memory.read(GuestPtr(pm), 50).unwrap(), pixels);
        assert_eq!(a.runtime.memory.read(SCREEN, 2).unwrap(), [0x91, 0x27]);
        assert_eq!(
            (a.runtime.process.mem_error, a.runtime.process.res_error),
            (-99, -98)
        );
        assert_eq!(a.runtime.memory.u32(devices::CURRENT).unwrap(), device);
    }
}
#[test]
fn copied_devices_do_not_acquire_video_modes_and_corrupt_main_storage_is_rejected() {
    let mut a = app(true);
    let device = ppc(&mut a, 0, &[]);
    let gd = a.runtime.memory.u32(GuestPtr(device)).unwrap();
    let copy = a
        .runtime
        .process
        .new_handle(&mut a.runtime.memory, 62, false)
        .unwrap();
    let data = a.runtime.memory.u32(GuestPtr(copy)).unwrap();
    let bytes = a.runtime.memory.read(GuestPtr(gd), 62).unwrap().to_vec();
    a.runtime.memory.write(GuestPtr(data), &bytes).unwrap();
    assert_eq!(ppc(&mut a, 1, &[copy, 8, 1, 1]), 0);
    assert_eq!(ppc(&mut a, 2, &[copy, 8, 1, 1]), -157i32 as u32);
    assert_eq!(native(&mut a, false, copy, 8, 1, 1), 0);
    a.runtime
        .memory
        .write(GuestPtr(gd + 42), &0u32.to_be_bytes())
        .unwrap();
    let mut manager = std::mem::take(&mut a.runtime.process.devices);
    let result = manager.call(
        &mut a.runtime.memory,
        &mut a.runtime.process,
        devices::Op::Depth {
            device,
            depth: 8,
            which_flags: 1,
            flags: 1,
            set: false,
        },
    );
    a.runtime.process.devices = manager;
    assert!(result
        .unwrap_err()
        .to_string()
        .contains("actual screen storage"));
}
#[test]
fn selector_and_frame_faults_do_not_pop_native_arguments() {
    for selector in [0x0a13, 0x0a14, 0x0a15] {
        let mut m = Memory::default();
        m.map(GuestPtr(STACK), vec![0; 10], Permissions::RW)
            .unwrap();
        let mut c = m68k::CpuCore::default();
        c.dar[15] = STACK;
        c.dar[0] = selector;
        assert!(Abi::trap(0xaaa2, &mut c, &m).is_err());
        assert_eq!(c.dar[15], STACK);
    }
}
#[test]
fn read_only_result_slots_reject_before_changing_native_registers() {
    for selector in [0x0a13, 0x0a14] {
        let mut m = Memory::default();
        m.map(GuestPtr(STACK), vec![0xa5; 12], Permissions::RX)
            .unwrap();
        let mut c = m68k::CpuCore::default();
        c.dar[15] = STACK;
        c.dar[0] = selector;
        assert!(Abi::trap(0xaaa2, &mut c, &m).is_err());
        assert_eq!(c.dar[15], STACK);
        assert_eq!(c.dar[0], selector);
        assert_eq!(m.read(GuestPtr(STACK + 10), 2).unwrap(), [0xa5; 2]);
    }
}

#[test]
#[ignore = "requires a real wgpu adapter"]
fn accepting_the_backed_mode_preserves_guest_pixels_and_gpu_presentation() {
    let mut a = app(true);
    let device = ppc(&mut a, 0, &[]);
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
    a.runtime.graphics =
        Some(pollster::block_on(Graphics::new(&instance, None, 32, 24, GuestPtr(0x4000))).unwrap());
    a.runtime
        .memory
        .write(SCREEN, &[0, 255, 0b00011111])
        .unwrap();
    a.runtime.refresh_screen().unwrap();
    let before = a.runtime.graphics.as_ref().unwrap().read_rgba().unwrap();
    assert_eq!(
        &before[..12],
        &[255, 255, 255, 255, 0, 0, 0, 255, 255, 0, 0, 255]
    );
    let mode = ppc(&mut a, 1, &[device, 8, 1, 1]);
    assert_ne!(mode, 0);
    assert_eq!(native(&mut a, true, device, mode as i16, 1, 1), 0);
    a.runtime.refresh_screen().unwrap();
    assert_eq!(
        a.runtime.graphics.as_ref().unwrap().read_rgba().unwrap(),
        before
    );
    assert_eq!(
        a.runtime.memory.read(SCREEN, 3).unwrap(),
        [0, 255, 0b00011111]
    );
}
