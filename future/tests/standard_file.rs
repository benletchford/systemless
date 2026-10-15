mod support;
use systemless_future::{
    abi::{Abi, Reply, Service},
    cpu::{Architecture, Call, Cpu},
    events,
    files::{Op, WorkingOp},
    graphics::Graphics,
    host_input::Pointer,
    load::{LoadedApplication, STACK_BASE},
    memory::{GuestPtr, Memory, Permissions},
    quickdraw::SCREEN,
    runtime::Stop,
    standard_file::PutFile,
};
const PROMPT: GuestPtr = GuestPtr(0x50000000);
const NAME: GuestPtr = GuestPtr(0x50000100);
const REPLY: GuestPtr = GuestPtr(0x50000200);
const STACK: u32 = STACK_BASE + 0x8000;
fn frame(reply: GuestPtr, hook: u32) -> Vec<u8> {
    let mut b = vec![0, 1];
    for v in [reply.0, hook, NAME.0, PROMPT.0] {
        b.extend(v.to_be_bytes());
    }
    b.extend(60i16.to_be_bytes());
    b.extend(70i16.to_be_bytes());
    b
}
#[test]
fn classic_save_abis_decode_exact_frames_packed_points_void_results_and_reject_bad_outputs() {
    let mut m = Memory::default();
    m.map(REPLY, vec![0xa5; 80], Permissions::RW).unwrap();
    m.map(GuestPtr(0x2000), frame(REPLY, 0), Permissions::RW)
        .unwrap();
    let mut cpu = Cpu::new(Architecture::M68k, 0, 0x2000);
    let Cpu::M68k(c) = &mut cpu else { panic!() };
    assert_eq!(
        Abi::trap(0xa9ea, c, &m).unwrap(),
        (
            Call::StandardPutFile(PutFile {
                point: [60, 70],
                prompt: PROMPT,
                name: NAME,
                hook: 0,
                reply: REPLY
            }),
            Reply::None
        )
    );
    assert_eq!(c.dar[15], 0x2016);
    for bytes in [frame(REPLY, 1), {
        let mut b = frame(REPLY, 0);
        b[1] = 2;
        b
    }] {
        m.write(GuestPtr(0x2000), &bytes).unwrap();
        c.dar[15] = 0x2000;
        assert!(Abi::trap(0xa9ea, c, &m).is_err());
        assert_eq!(c.dar[15], 0x2000);
    }
    m.map(
        GuestPtr(0x3000),
        frame(REPLY, 0)[..21].to_vec(),
        Permissions::RX,
    )
    .unwrap();
    c.dar[15] = 0x3000;
    assert!(Abi::trap(0xa9ea, c, &m).is_err());
    assert_eq!(c.dar[15], 0x3000);
    m.map(GuestPtr(0x4000), vec![0; 74], Permissions::RX)
        .unwrap();
    m.write(GuestPtr(0x2000), &frame(GuestPtr(0x4000), 0))
        .unwrap();
    c.dar[15] = 0x2000;
    assert!(Abi::trap(0xa9ea, c, &m).is_err());
    assert_eq!(c.dar[15], 0x2000);
    let mut cpu = Cpu::new(Architecture::PowerPc, 0, STACK);
    let Cpu::PowerPc(c) = &mut cpu else { panic!() };
    c.gpr[3] = 0xfffe8001;
    c.gpr[4] = PROMPT.0;
    c.gpr[5] = NAME.0;
    c.gpr[6] = 0;
    c.gpr[7] = REPLY.0;
    assert_eq!(
        Abi::resolve(b"InterfaceLib", b"SFPutFile"),
        Some(Service::SFPutFile)
    );
    assert_eq!(
        Abi::import(Service::SFPutFile, c, &m).unwrap(),
        (
            Call::StandardPutFile(PutFile {
                point: [-2, -32767],
                prompt: PROMPT,
                name: NAME,
                hook: 0,
                reply: REPLY
            }),
            Reply::None
        )
    );
    assert_ne!(
        systemless_future::traps::address(0x1ea, true),
        systemless_future::traps::UNIMPLEMENTED
    );
    assert_eq!(m.read(REPLY, 80).unwrap(), [0xa5; 80]);
}
fn gate(a: &LoadedApplication, index: u32) -> u32 {
    let v = a
        .runtime
        .memory
        .u32(GuestPtr(a.info.toc_or_a5 + 8 + index * 4))
        .unwrap();
    a.runtime.memory.u32(GuestPtr(v)).unwrap()
}
fn invoke(a: &mut LoadedApplication, index: u32, args: &[u32]) {
    a.runtime.cpu = Cpu::new(Architecture::PowerPc, gate(a, index), STACK);
    let Cpu::PowerPc(c) = &mut a.runtime.cpu else {
        panic!()
    };
    for (i, &arg) in args.iter().enumerate() {
        c.gpr[3 + i] = arg;
    }
    assert_eq!(a.runtime.advance(1).unwrap(), Stop::Budget);
}
fn string(a: &mut LoadedApplication, p: GuestPtr, s: &[u8]) {
    let mut b = vec![s.len() as u8];
    b.extend(s);
    a.runtime.memory.write(p, &b).unwrap();
}
fn app() -> LoadedApplication {
    let f = support::pef_program(
        &[0x4e800020],
        &[
            b"InitGraf",
            b"InitWindows",
            b"SFPutFile",
            b"InitFonts",
            b"SFGetFile",
        ],
        0,
    );
    let mut a = LoadedApplication::pef(&f.bytes, None).unwrap();
    a.runtime
        .memory
        .map(PROMPT, vec![0xa5; 4096], Permissions::RW)
        .unwrap();
    let mut native = vec![0xa9, 0xea];
    native.extend([0x4e, 0x71].repeat(16));
    a.runtime
        .memory
        .map(GuestPtr(0x51000000), native, Permissions::RX)
        .unwrap();
    a.runtime
        .memory
        .map(
            GuestPtr(0x52000000),
            [0x60, 0, 0, 0].repeat(16),
            Permissions::RX,
        )
        .unwrap();
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
    let mut gpu =
        pollster::block_on(Graphics::new(&instance, None, 512, 342, GuestPtr(0x4000))).unwrap();
    gpu.palette[0] = [255; 4];
    gpu.palette[1] = [0, 0, 0, 255];
    a.runtime.graphics = Some(gpu);
    invoke(&mut a, 0, &[0x50000800]);
    invoke(&mut a, 3, &[]);
    invoke(&mut a, 1, &[]);
    string(&mut a, PROMPT, b"Save game as:");
    string(&mut a, NAME, b"Initial");
    a
}
fn begin(a: &mut LoadedApplication, arch: Architecture) -> systemless_future::Result<Stop> {
    a.runtime
        .memory
        .write(GuestPtr(STACK), &[0x5a; 30])
        .unwrap();
    a.runtime.process.mem_error = -108;
    a.runtime.process.res_error = -192;
    a.runtime
        .memory
        .write(GuestPtr(0x220), &(-50i16).to_be_bytes())
        .unwrap();
    a.runtime
        .memory
        .write(GuestPtr(0xa60), &(-49i16).to_be_bytes())
        .unwrap();
    match arch {
        Architecture::M68k => {
            a.runtime
                .memory
                .write(GuestPtr(STACK), &frame(REPLY, 0))
                .unwrap();
            a.runtime.cpu = Cpu::new(arch, 0x51000000, STACK);
            if let Cpu::M68k(c) = &mut a.runtime.cpu {
                c.dar[0] = 0x12345678;
            }
        }
        Architecture::PowerPc => {
            a.runtime.cpu = Cpu::new(arch, gate(a, 2), STACK);
            if let Cpu::PowerPc(c) = &mut a.runtime.cpu {
                c.gpr[3] = (60 << 16) | 70;
                c.gpr[4] = PROMPT.0;
                c.gpr[5] = NAME.0;
                c.gpr[6] = 0;
                c.gpr[7] = REPLY.0;
                c.gpr[8] = 0x12345678;
                c.lr = 0x52000000;
            }
        }
    }
    a.runtime.advance(1)
}
fn click(a: &mut LoadedApplication, ptr: &mut Pointer, x: f64, y: f64) -> Stop {
    ptr.motion(
        &mut a.runtime.process.input,
        &mut a.runtime.memory,
        (x, y),
        (512, 342),
        (512, 342),
    )
    .unwrap();
    ptr.button(&mut a.runtime.memory, &mut a.runtime.process.input, true)
        .unwrap();
    assert_eq!(a.runtime.advance(1).unwrap(), Stop::Waiting { ticks: 1 });
    ptr.button(&mut a.runtime.memory, &mut a.runtime.process.input, false)
        .unwrap();
    a.runtime.advance(1).unwrap()
}
fn key(a: &mut LoadedApplication, byte: u8) -> Stop {
    let input = a.runtime.process.input;
    events::post(&mut a.runtime.memory, 3, u32::from(byte), 0, input).unwrap();
    a.runtime.advance(1).unwrap()
}
fn check_restored(a: &LoadedApplication, arch: Architecture) {
    assert_eq!(
        a.runtime
            .memory
            .u32(systemless_future::windows::LIST)
            .unwrap(),
        0
    );
    assert_eq!(
        a.runtime
            .memory
            .u32(a.runtime.process.ports.unwrap().current_slot)
            .unwrap(),
        0
    );
    assert_eq!(
        (a.runtime.process.mem_error, a.runtime.process.res_error),
        (-108, -192)
    );
    assert_eq!(a.runtime.memory.u16(GuestPtr(0x220)).unwrap() as i16, -50);
    assert_eq!(a.runtime.memory.u16(GuestPtr(0xa60)).unwrap() as i16, -49);
    assert_eq!(
        a.runtime.memory.read(GuestPtr(REPLY.0 + 74), 6).unwrap(),
        [0xa5; 6]
    );
    match &a.runtime.cpu {
        Cpu::M68k(c) => {
            assert_eq!(arch, Architecture::M68k);
            assert_eq!(c.dar[15], STACK + 22);
            assert_eq!(c.dar[0], 0x12345678);
            assert_eq!(
                a.runtime.memory.read(GuestPtr(STACK + 22), 8).unwrap(),
                [0x5a; 8]
            );
        }
        Cpu::PowerPc(c) => {
            assert_eq!(arch, Architecture::PowerPc);
            assert_eq!(c.gpr[3], (60 << 16) | 70);
            assert_eq!(c.gpr[8], 0x12345678);
        }
    }
}
#[test]
#[ignore = "requires a real wgpu adapter"]
fn both_cpus_show_the_same_save_ui_wait_without_guest_spin_and_cancel_without_fake_outputs() {
    let mut images = Vec::new();
    for arch in [Architecture::M68k, Architecture::PowerPc] {
        let mut a = app();
        let before = a.runtime.memory.read(SCREEN, 64 * 342).unwrap().to_vec();
        assert_eq!(begin(&mut a, arch).unwrap(), Stop::Waiting { ticks: 1 });
        images.push(a.runtime.memory.read(SCREEN, 64 * 342).unwrap().to_vec());
        assert_eq!(a.runtime.memory.read(REPLY, 80).unwrap(), [0xa5; 80]);
        let n = a.runtime.instructions;
        assert_eq!(
            a.runtime.advance(100_000).unwrap(),
            Stop::Waiting { ticks: 1 }
        );
        assert_eq!(a.runtime.instructions, n);
        assert_eq!(key(&mut a, 27), Stop::Budget);
        assert_eq!(a.runtime.memory.read(REPLY, 2).unwrap(), [0, 0xa5]);
        assert_eq!(
            a.runtime.memory.read(GuestPtr(REPLY.0 + 1), 79).unwrap(),
            [0xa5; 79]
        );
        assert_eq!(a.runtime.memory.read(SCREEN, 64 * 342).unwrap(), before);
        check_restored(&a, arch);
    }
    assert_eq!(images[0], images[1]);
}
fn fresh_pb(a: &mut LoadedApplication, dir: u32, name: &[u8]) {
    let pb = GuestPtr(PROMPT.0 + 1024);
    a.runtime.memory.write(pb, &[0; 128]).unwrap();
    a.runtime
        .memory
        .write(GuestPtr(pb.0 + 48), &dir.to_be_bytes())
        .unwrap();
    a.runtime
        .memory
        .write(GuestPtr(pb.0 + 18), &(PROMPT.0 + 1280).to_be_bytes())
        .unwrap();
    string(a, GuestPtr(PROMPT.0 + 1280), name);
}
#[test]
#[ignore = "requires a real wgpu adapter"]
fn both_cpus_browse_edit_names_confirm_replacement_and_return_real_folder_identity_without_creating_files(
) {
    for arch in [Architecture::M68k, Architecture::PowerPc] {
        let mut a = app();
        let pb = GuestPtr(PROMPT.0 + 1024);
        fresh_pb(&mut a, 2, b"Saves");
        assert_eq!(
            a.runtime
                .process
                .files
                .create_directory(&mut a.runtime.memory, pb)
                .unwrap(),
            0
        );
        let folder = a.runtime.memory.u32(GuestPtr(pb.0 + 48)).unwrap();
        fresh_pb(&mut a, 2, b"Existing");
        assert_eq!(
            a.runtime
                .process
                .files
                .io(&mut a.runtime.memory, pb, Op::Create { hierarchical: true })
                .unwrap(),
            0
        );
        assert_eq!(begin(&mut a, arch).unwrap(), Stop::Waiting { ticks: 1 });
        let mut pointer = Pointer::default();
        // Application, Existing, Saves, sorted by the shared catalog.
        assert_eq!(
            click(&mut a, &mut pointer, 110., 145.),
            Stop::Waiting { ticks: 1 }
        );
        assert_eq!(
            click(&mut a, &mut pointer, 110., 145.),
            Stop::Waiting { ticks: 1 }
        );
        for b in b"Game" {
            assert_eq!(key(&mut a, *b), Stop::Waiting { ticks: 1 });
        }
        assert_eq!(click(&mut a, &mut pointer, 377., 259.), Stop::Budget);
        assert_eq!(
            a.runtime.memory.read(GuestPtr(REPLY.0 + 10), 5).unwrap(),
            b"\x04Game"
        );
        let wd = a.runtime.memory.u16(GuestPtr(REPLY.0 + 6)).unwrap() as i16;
        fresh_pb(&mut a, 0, b"");
        a.runtime
            .memory
            .write(GuestPtr(pb.0 + 22), &wd.to_be_bytes())
            .unwrap();
        assert_eq!(
            a.runtime
                .process
                .files
                .working_directory(&mut a.runtime.memory, pb, WorkingOp::Info)
                .unwrap(),
            0
        );
        assert_eq!(a.runtime.memory.u32(GuestPtr(pb.0 + 48)).unwrap(), folder);
        assert!(a
            .runtime
            .process
            .files
            .directory_view(folder)
            .unwrap()
            .entries
            .is_empty());
        check_restored(&a, arch);
        // The application can now create the chosen name using that SFReply WD.
        fresh_pb(&mut a, 0, b"Game");
        a.runtime
            .memory
            .write(GuestPtr(pb.0 + 22), &wd.to_be_bytes())
            .unwrap();
        assert_eq!(
            a.runtime
                .process
                .files
                .io(
                    &mut a.runtime.memory,
                    pb,
                    Op::Create {
                        hierarchical: false
                    }
                )
                .unwrap(),
            0
        );
        assert_eq!(
            a.runtime
                .process
                .files
                .directory_view(folder)
                .unwrap()
                .entries[0]
                .name,
            b"Game"
        );
        // Repeat in root: Save existing file must wait for explicit replacement.
        a.runtime.memory.write(REPLY, &[0xa5; 80]).unwrap();
        string(&mut a, NAME, b"Existing");
        assert_eq!(begin(&mut a, arch).unwrap(), Stop::Waiting { ticks: 1 });
        assert_eq!(key(&mut a, 13), Stop::Waiting { ticks: 1 });
        assert_eq!(a.runtime.memory.read(REPLY, 80).unwrap(), [0xa5; 80]);
        assert_eq!(key(&mut a, 27), Stop::Waiting { ticks: 1 });
        assert_eq!(key(&mut a, 13), Stop::Waiting { ticks: 1 });
        assert_eq!(key(&mut a, 13), Stop::Budget);
        assert_eq!(
            a.runtime.memory.read(GuestPtr(REPLY.0 + 10), 9).unwrap(),
            b"\x08Existing"
        );
        check_restored(&a, arch);
    }
}
#[test]
#[ignore = "requires a real wgpu adapter"]
fn save_constraints_paging_navigation_and_exhausted_directory_storage_keep_the_dialog_interactive()
{
    let mut a = app();
    let pb = GuestPtr(PROMPT.0 + 1024);
    for n in 0..7 {
        fresh_pb(&mut a, 2, format!("A{n}").as_bytes());
        assert_eq!(
            a.runtime
                .process
                .files
                .create_directory(&mut a.runtime.memory, pb)
                .unwrap(),
            0
        );
    }
    assert_eq!(
        begin(&mut a, Architecture::PowerPc).unwrap(),
        Stop::Waiting { ticks: 1 }
    );
    let initial = a.runtime.memory.read(SCREEN, 64 * 342).unwrap().to_vec();
    let mut pointer = Pointer::default();
    assert_eq!(
        click(&mut a, &mut pointer, 377., 171.),
        Stop::Waiting { ticks: 1 }
    );
    assert_ne!(a.runtime.memory.read(SCREEN, 64 * 342).unwrap(), initial);
    assert_eq!(
        click(&mut a, &mut pointer, 110., 112.),
        Stop::Waiting { ticks: 1 }
    );
    assert_eq!(
        click(&mut a, &mut pointer, 110., 112.),
        Stop::Waiting { ticks: 1 }
    );
    assert_eq!(
        click(&mut a, &mut pointer, 377., 111.),
        Stop::Waiting { ticks: 1 }
    );
    assert_eq!(a.runtime.memory.read(SCREEN, 64 * 342).unwrap(), initial);
    assert_eq!(key(&mut a, 27), Stop::Budget);
    for name in [b"".as_slice(), b"Application".as_slice()] {
        string(&mut a, NAME, name);
        a.runtime.memory.write(REPLY, &[0xa5; 80]).unwrap();
        assert_eq!(
            begin(&mut a, Architecture::PowerPc).unwrap(),
            Stop::Waiting { ticks: 1 }
        );
        assert_eq!(key(&mut a, 13), Stop::Waiting { ticks: 1 });
        assert_eq!(
            click(&mut a, &mut pointer, 377., 259.),
            Stop::Waiting { ticks: 1 }
        );
        assert_eq!(a.runtime.memory.read(REPLY, 80).unwrap(), [0xa5; 80]);
        assert_eq!(key(&mut a, 27), Stop::Budget);
    }
    for owner in 1..=128 {
        a.runtime
            .process
            .files
            .open_working_directory(-1, 2, owner)
            .unwrap();
    }
    string(&mut a, NAME, b"Initial");
    a.runtime.memory.write(REPLY, &[0xa5; 80]).unwrap();
    assert_eq!(
        begin(&mut a, Architecture::PowerPc).unwrap(),
        Stop::Waiting { ticks: 1 }
    );
    assert_eq!(key(&mut a, 13), Stop::Waiting { ticks: 1 });
    assert_eq!(a.runtime.memory.read(REPLY, 80).unwrap(), [0xa5; 80]);
    assert_eq!(key(&mut a, 27), Stop::Budget);
    for reference in -32767..=-32640 {
        a.runtime
            .process
            .files
            .close_working_directory(reference)
            .unwrap();
    }
    string(&mut a, NAME, &[b'W'; 31]);
    a.runtime.memory.write(REPLY, &[0xa5; 80]).unwrap();
    assert_eq!(
        begin(&mut a, Architecture::M68k).unwrap(),
        Stop::Waiting { ticks: 1 }
    );
    assert_eq!(key(&mut a, 0x1d), Stop::Waiting { ticks: 1 });
    assert_eq!(key(&mut a, b'Z'), Stop::Waiting { ticks: 1 });
    assert_eq!(key(&mut a, 13), Stop::Budget);
    assert_eq!(
        a.runtime.memory.read(GuestPtr(REPLY.0 + 11), 31).unwrap(),
        [b'W'; 31]
    );
    check_restored(&a, Architecture::M68k);
}

#[test]
fn classic_open_abis_validate_the_full_frame_types_hooks_and_reply_before_popping() {
    use systemless_future::standard_file::GetFile;
    let mut m = Memory::default();
    m.map(REPLY, vec![0xa5; 80], Permissions::RW).unwrap();
    m.map(NAME, b"GAME".to_vec(), Permissions::RX).unwrap();
    let mut bytes = Vec::new();
    bytes.extend(2u16.to_be_bytes());
    for v in [REPLY.0, 0, NAME.0] {
        bytes.extend(v.to_be_bytes());
    }
    bytes.extend(1i16.to_be_bytes());
    for v in [0x4002f2u32, 0xffff_ffff] {
        bytes.extend(v.to_be_bytes());
    }
    bytes.extend((-2i16).to_be_bytes());
    bytes.extend(100i16.to_be_bytes());
    assert_eq!(bytes.len(), 28);
    m.map(GuestPtr(0x2000), bytes.clone(), Permissions::RW)
        .unwrap();
    let request = GetFile {
        point: [-2, 100],
        filter: 0x4002f2,
        count: 1,
        types: NAME,
        hook: 0,
        reply: REPLY,
    };
    let mut cpu = Cpu::new(Architecture::M68k, 0, 0x2000);
    let Cpu::M68k(c) = &mut cpu else { panic!() };
    assert_eq!(
        Abi::trap(0xa9ea, c, &m).unwrap(),
        (Call::StandardGetFile(request), Reply::None)
    );
    assert_eq!(c.dar[15], 0x201c);
    for (offset, value) in [(6, 1u32), (10, NAME.0 + 1), (2, REPLY.0 + 7)] {
        let mut bad = bytes.clone();
        bad[offset..offset + 4].copy_from_slice(&value.to_be_bytes());
        m.write(GuestPtr(0x2000), &bad).unwrap();
        c.dar[15] = 0x2000;
        assert!(Abi::trap(0xa9ea, c, &m).is_err());
        assert_eq!(c.dar[15], 0x2000);
    }
    for count in [-2i16, 5] {
        let mut bad = bytes.clone();
        bad[14..16].copy_from_slice(&count.to_be_bytes());
        m.write(GuestPtr(0x2000), &bad).unwrap();
        c.dar[15] = 0x2000;
        assert!(Abi::trap(0xa9ea, c, &m).is_err());
        assert_eq!(c.dar[15], 0x2000);
    }
    m.map(GuestPtr(0x3000), bytes[..27].to_vec(), Permissions::RW)
        .unwrap();
    c.dar[15] = 0x3000;
    assert!(Abi::trap(0xa9ea, c, &m).is_err());
    assert_eq!(c.dar[15], 0x3000);
    let mut cpu = Cpu::new(Architecture::PowerPc, 0, STACK);
    let Cpu::PowerPc(c) = &mut cpu else { panic!() };
    c.gpr[3] = 0xfffe0064;
    c.gpr[4] = u32::MAX;
    c.gpr[5] = request.filter;
    c.gpr[6] = 1;
    c.gpr[7] = NAME.0;
    c.gpr[8] = 0;
    c.gpr[9] = REPLY.0;
    assert_eq!(
        Abi::resolve(b"InterfaceLib", b"SFGetFile"),
        Some(Service::SFGetFile)
    );
    assert_eq!(
        Abi::import(Service::SFGetFile, c, &m).unwrap(),
        (Call::StandardGetFile(request), Reply::None)
    );
}

#[test]
#[ignore = "requires a real wgpu adapter"]
fn file_filter_guest_loops_remain_budgeted_on_both_architectures() {
    for arch in [Architecture::M68k, Architecture::PowerPc] {
        let mut a = app();
        let code = match arch {
            Architecture::M68k => vec![0x60, 0xfe],
            Architecture::PowerPc => 0x48000000u32.to_be_bytes().to_vec(),
        };
        a.runtime
            .memory
            .map(GuestPtr(0x53000000), code, Permissions::RX)
            .unwrap();
        // A real static single-record PowerPC UPP points to a transition vector.
        let mut rd = [0; 40];
        rd[..4].copy_from_slice(&[0xaa, 0xfe, 7, 0]);
        rd[12..16].copy_from_slice(&0xd0u32.to_be_bytes());
        rd[17] = 1;
        rd[20..24].copy_from_slice(&0x54000020u32.to_be_bytes());
        rd[32..36].copy_from_slice(&0x53000000u32.to_be_bytes());
        rd[36..40].copy_from_slice(&0xdead1000u32.to_be_bytes());
        a.runtime
            .memory
            .map(GuestPtr(0x54000000), rd.to_vec(), Permissions::RX)
            .unwrap();
        match arch {
            Architecture::M68k => {
                let mut b = vec![0, 2];
                for v in [REPLY.0, 0, 0] {
                    b.extend(v.to_be_bytes());
                }
                b.extend((-1i16).to_be_bytes());
                for v in [0x53000000u32, 0] {
                    b.extend(v.to_be_bytes());
                }
                b.extend([0, 60, 0, 70]);
                a.runtime.memory.write(GuestPtr(STACK), &b).unwrap();
                a.runtime.cpu = Cpu::new(arch, 0x51000000, STACK);
            }
            Architecture::PowerPc => {
                a.runtime.cpu = Cpu::new(arch, gate(&a, 4), STACK);
                if let Cpu::PowerPc(c) = &mut a.runtime.cpu {
                    c.gpr[3] = (60 << 16) | 70;
                    c.gpr[4] = u32::MAX;
                    c.gpr[5] = 0x54000000;
                    c.gpr[6] = u32::MAX;
                    c.gpr[7] = 0;
                    c.gpr[8] = 0;
                    c.gpr[9] = REPLY.0;
                }
            }
        }
        let before = a.runtime.instructions;
        assert_eq!(a.runtime.advance(17).unwrap(), Stop::Budget);
        assert_eq!(a.runtime.instructions, before + 17);
        assert_eq!(a.runtime.cpu.pc(), 0x53000000);
        assert_eq!(a.runtime.advance(19).unwrap(), Stop::Budget);
        assert_eq!(a.runtime.instructions, before + 36);
        assert_eq!(a.runtime.memory.read(REPLY, 74).unwrap(), [0xa5; 74]);
    }
}
