mod support;
use systemless_future::{
    cpu::{Architecture, Cpu},
    load::{LoadedApplication, STACK_BASE},
    memory::{GuestPtr, Permissions},
    runtime::Stop,
    sound::{NewChannel, Op},
};
const BASE: u32 = 0x50000000;
const STACK: u32 = STACK_BASE + 0x8000;
fn app() -> LoadedApplication {
    let f = support::pef_program(
        &[0x4e800020],
        &[
            b"SndPlay",
            b"SndDoCommand",
            b"SndDoImmediate",
            b"SndStartFilePlay",
            b"SndPauseFilePlay",
        ],
        32,
    );
    let mut a = LoadedApplication::pef(&f.bytes, None).unwrap();
    a.runtime.memory.write(GuestPtr(0x260), &[7]).unwrap();
    a.runtime
        .memory
        .map(GuestPtr(BASE), vec![0; 4096], Permissions::RW)
        .unwrap();
    a.runtime
        .memory
        .map(
            GuestPtr(0x51000000),
            vec![0xa8, 5, 0xa8, 3, 0xa8, 4, 0x60, 0xfe],
            Permissions::RX,
        )
        .unwrap();
    a.runtime
        .memory
        .map(
            GuestPtr(0x53000000),
            0x48000000u32.to_be_bytes().to_vec(),
            Permissions::RX,
        )
        .unwrap();
    a
}
fn command(code: u16, param: u16, value: u32) -> [u8; 8] {
    let mut b = [0; 8];
    b[..2].copy_from_slice(&code.to_be_bytes());
    b[2..4].copy_from_slice(&param.to_be_bytes());
    b[4..].copy_from_slice(&value.to_be_bytes());
    b
}
fn payload(format: u16, samples: &[u8], count: usize) -> Vec<u8> {
    let mut b = if format == 1 {
        vec![0, 1, 0, 1, 0, 5, 0, 0, 0, 0xa0]
    } else {
        vec![0, 2, 0xff, 0xff]
    };
    b.extend((count as u16).to_be_bytes());
    let offset = (b.len() + count * 8) as u32;
    for _ in 0..count {
        b.extend(command(0x8051, 0, offset));
    }
    let mut h = vec![0; 22];
    h[4..8].copy_from_slice(&(samples.len() as u32).to_be_bytes());
    h[8..12].copy_from_slice(&(8000u32 << 16).to_be_bytes());
    h[21] = 60;
    b.extend(h);
    b.extend(samples);
    b
}
fn handle(a: &mut LoadedApplication, bytes: &[u8]) -> u32 {
    let h = a
        .runtime
        .process
        .new_handle(&mut a.runtime.memory, bytes.len() as u32, false)
        .unwrap();
    assert_ne!(h, 0);
    let data = a.runtime.memory.u32(GuestPtr(h)).unwrap();
    a.runtime.memory.write(GuestPtr(data), bytes).unwrap();
    h
}
fn channel(a: &mut LoadedApplication, length: u16) -> u32 {
    a.runtime
        .memory
        .write(
            GuestPtr(BASE),
            &if length == 128 { 0u32 } else { BASE + 1536 }.to_be_bytes(),
        )
        .unwrap();
    if length != 128 {
        a.runtime
            .memory
            .write(GuestPtr(BASE + 1536 + 30), &length.to_be_bytes())
            .unwrap();
    }
    let mut sound = std::mem::take(&mut a.runtime.process.sound);
    let e = sound
        .call(
            &mut a.runtime.memory,
            &mut a.runtime.process,
            Op::New(NewChannel {
                destination: GuestPtr(BASE),
                synth: 5,
                init: 4,
                callback: 0,
            }),
        )
        .unwrap();
    a.runtime.process.sound = sound;
    assert_eq!(e, 0);
    a.runtime.memory.u32(GuestPtr(BASE)).unwrap()
}
fn submit(
    a: &mut LoadedApplication,
    arch: Architecture,
    ch: u32,
    h: u32,
    asynchronous: bool,
) -> Stop {
    a.runtime.process.mem_error = -108;
    a.runtime.process.res_error = -192;
    a.runtime
        .memory
        .write(GuestPtr(0x220), &0xbeefu16.to_be_bytes())
        .unwrap();
    a.runtime
        .memory
        .write(GuestPtr(0xa60), &0xcafeu16.to_be_bytes())
        .unwrap();
    if arch == Architecture::M68k {
        let mut args = vec![u8::from(asynchronous), 0x94];
        args.extend(h.to_be_bytes());
        args.extend(ch.to_be_bytes());
        args.extend([0x5a; 4]);
        a.runtime.memory.write(GuestPtr(STACK), &args).unwrap();
        a.runtime.cpu = Cpu::new(arch, 0x51000000, STACK);
        if let Cpu::M68k(c) = &mut a.runtime.cpu {
            c.dar[0] = 0x12345678;
        }
    } else {
        let vector = a
            .runtime
            .memory
            .u32(GuestPtr(a.info.toc_or_a5 + 8))
            .unwrap();
        let pc = a.runtime.memory.u32(GuestPtr(vector)).unwrap();
        a.runtime.cpu = Cpu::new(arch, pc, STACK);
        if let Cpu::PowerPc(c) = &mut a.runtime.cpu {
            c.gpr[3] = ch;
            c.gpr[4] = h;
            c.gpr[5] = u32::from(asynchronous);
        }
    }
    a.runtime.advance(1).unwrap()
}
fn loop_cpu(a: &mut LoadedApplication) {
    match &mut a.runtime.cpu {
        Cpu::M68k(c) => c.pc = 0x51000006,
        Cpu::PowerPc(c) => c.pc = 0x53000000,
    }
}
fn result(a: &LoadedApplication) -> i16 {
    assert_eq!(
        (a.runtime.process.mem_error, a.runtime.process.res_error),
        (-108, -192)
    );
    assert_eq!(a.runtime.memory.u16(GuestPtr(0x220)).unwrap(), 0xbeef);
    assert_eq!(a.runtime.memory.u16(GuestPtr(0xa60)).unwrap(), 0xcafe);
    match &a.runtime.cpu {
        Cpu::M68k(c) => {
            assert_eq!(c.dar[15], STACK + 10);
            assert_eq!(c.dar[0], 0x12345678);
            assert_eq!(a.runtime.memory.u16(GuestPtr(STACK + 12)).unwrap(), 0x5a5a);
            a.runtime.memory.u16(GuestPtr(STACK + 10)).unwrap() as i16
        }
        Cpu::PowerPc(c) => c.gpr[3] as i16,
    }
}
fn queue(a: &mut LoadedApplication, ch: u32, bytes: [u8; 8]) {
    let mut s = std::mem::take(&mut a.runtime.process.sound);
    assert_eq!(
        s.enqueue(&mut a.runtime.memory, &mut a.runtime.process, ch, bytes)
            .unwrap(),
        Some(0)
    );
    a.runtime.process.sound = s;
}
#[test]
fn both_formats_and_cpus_play_live_resource_samples_without_mutating_source_commands_or_flags() {
    for arch in [Architecture::M68k, Architecture::PowerPc] {
        for format in [1, 2] {
            let mut a = app();
            let ch = channel(&mut a, 128);
            let source = payload(format, &[0, 128, 255, 64], 1);
            let h = handle(&mut a, &source);
            a.runtime
                .process
                .set_handle_state(&a.runtime.memory, h, 0xc0)
                .unwrap();
            assert_eq!(submit(&mut a, arch, ch, h, true), Stop::Budget);
            assert_eq!(result(&a), 0);
            assert_eq!(a.runtime.process.sound.busy(ch), Some(true));
            let data = a.runtime.memory.u32(GuestPtr(h)).unwrap();
            assert_eq!(
                a.runtime.memory.read(GuestPtr(data), source.len()).unwrap(),
                source
            );
            assert_eq!(
                a.runtime
                    .process
                    .handle_state(&a.runtime.memory, h)
                    .unwrap(),
                0xc0
            );
            let at = data + source.len() as u32 - 2;
            a.runtime.memory.write(GuestPtr(at), &[192, 255]).unwrap();
            assert_eq!(
                a.runtime.render_sound(5, 8000).unwrap(),
                [[-32768; 2], [0; 2], [16384; 2], [32512; 2], [0; 2]]
            );
            assert_eq!(a.runtime.process.sound.busy(ch), Some(false));
        }
    }
}
#[test]
fn nil_channel_forces_real_synchronous_play_and_releases_all_internal_allocations() {
    for arch in [Architecture::M68k, Architecture::PowerPc] {
        for format in [1, 2] {
            for asynchronous in [false, true] {
                let mut a = app();
                let h = handle(&mut a, &payload(format, &[0, 128, 255, 64], 1));
                let free = a.runtime.process.free_memory();
                assert_eq!(
                    submit(&mut a, arch, 0, h, asynchronous),
                    Stop::Waiting { ticks: 1 }
                );
                loop_cpu(&mut a);
                let instructions = a.runtime.instructions;
                assert_eq!(
                    a.runtime.advance(10000).unwrap(),
                    Stop::Waiting { ticks: 1 }
                );
                assert_eq!(a.runtime.instructions, instructions);
                assert!(a.runtime.process.free_memory() < free);
                assert_eq!(
                    a.runtime.render_sound(4, 8000).unwrap(),
                    [[-32768; 2], [0; 2], [32512; 2], [-16384; 2]]
                );
                assert_eq!(a.runtime.advance(1).unwrap(), Stop::Budget);
                assert_eq!(result(&a), 0);
                assert_eq!(a.runtime.process.free_memory(), free);
                assert!(!a.runtime.process.sound.playing());
                assert_eq!(
                    a.runtime
                        .process
                        .handle_state(&a.runtime.memory, h)
                        .unwrap(),
                    0
                );
            }
        }
    }
}
#[test]
fn synchronous_completion_belongs_to_the_resource_not_later_unrelated_work() {
    for arch in [Architecture::M68k, Architecture::PowerPc] {
        let mut a = app();
        let ch = channel(&mut a, 128);
        let h = handle(&mut a, &payload(1, &[192], 1));
        let other = handle(&mut a, &payload(2, &[224], 1));
        let header = a.runtime.memory.u32(GuestPtr(other)).unwrap() + 14;
        assert_eq!(
            submit(&mut a, arch, ch, h, false),
            Stop::Waiting { ticks: 1 }
        );
        loop_cpu(&mut a);
        queue(&mut a, ch, command(81, 0, header));
        assert_eq!(a.runtime.render_sound(1, 8000).unwrap(), [[16384; 2]]);
        assert_eq!(a.runtime.advance(1).unwrap(), Stop::Budget);
        assert_eq!(result(&a), 0);
        assert_eq!(a.runtime.process.sound.busy(ch), Some(true));
        assert_eq!(a.runtime.render_sound(1, 8000).unwrap(), [[24576; 2]]);
    }
}
#[test]
fn asynchronous_submission_waits_for_queue_space_then_returns_before_playback_finishes() {
    for arch in [Architecture::M68k, Architecture::PowerPc] {
        let mut a = app();
        let ch = channel(&mut a, 2);
        let h = handle(&mut a, &payload(1, &[192], 4));
        a.runtime
            .process
            .set_handle_lock(&a.runtime.memory, h, true)
            .unwrap();
        assert_eq!(
            submit(&mut a, arch, ch, h, true),
            Stop::Waiting { ticks: 1 }
        );
        loop_cpu(&mut a);
        assert_eq!(a.runtime.render_sound(2, 8000).unwrap(), [[16384; 2]; 2]);
        assert_eq!(a.runtime.advance(1).unwrap(), Stop::Budget);
        assert_eq!(result(&a), 0);
        assert_eq!(a.runtime.process.sound.busy(ch), Some(true));
        assert_eq!(a.runtime.render_sound(2, 8000).unwrap(), [[16384; 2]; 2]);
        assert_eq!(a.runtime.process.sound.busy(ch), Some(false));
    }
}
#[test]
fn resource_bounds_and_missing_handles_fail_without_allocating_or_publishing_commands() {
    for arch in [Architecture::M68k, Architecture::PowerPc] {
        let mut a = app();
        let valid = payload(1, &[192], 1);
        let mut broken = Vec::new();
        broken.push(vec![]);
        broken.push(vec![0, 3, 0, 0]);
        broken.push(valid[..19].to_vec());
        let mut offset = valid.clone();
        offset[16..20].copy_from_slice(&u32::MAX.to_be_bytes());
        broken.push(offset);
        let mut samples = valid.clone();
        samples[24..28].copy_from_slice(&100u32.to_be_bytes());
        broken.push(samples);
        for data in broken {
            let h = handle(&mut a, &data);
            let free = a.runtime.process.free_memory();
            assert_eq!(submit(&mut a, arch, 0, h, true), Stop::Budget);
            assert_eq!(result(&a), -206);
            assert_eq!(a.runtime.process.free_memory(), free);
            assert!(!a.runtime.process.sound.playing());
        }
        for h in [0, 0xfffffeed] {
            assert_eq!(submit(&mut a, arch, 0, h, true), Stop::Budget);
            assert_eq!(result(&a), -204);
        }
        let h = handle(&mut a, &valid);
        a.runtime.memory.write(GuestPtr(h), &[0; 4]).unwrap();
        assert_eq!(submit(&mut a, arch, 0, h, true), Stop::Budget);
        assert_eq!(result(&a), -204);
    }
}
#[test]
fn buffer_playback_ignores_unused_pitch_and_loop_metadata_and_accepts_absolute_headers() {
    for arch in [Architecture::M68k, Architecture::PowerPc] {
        let mut a = app();
        let ch = channel(&mut a, 128);
        let mut source = payload(1, &[192, 224], 1);
        source[32..36].copy_from_slice(&9000u32.to_be_bytes());
        source[36..40].copy_from_slice(&9000u32.to_be_bytes());
        source[41] = 0;
        let h = handle(&mut a, &source);
        assert_eq!(submit(&mut a, arch, ch, h, true), Stop::Budget);
        assert_eq!(result(&a), 0);
        assert_eq!(
            a.runtime.render_sound(3, 8000).unwrap(),
            [[16384; 2], [24576; 2], [0; 2]]
        );
        let source = payload(2, &[0, 255], 1);
        a.runtime
            .memory
            .write(GuestPtr(BASE + 512), &source[14..])
            .unwrap();
        let mut absolute = vec![0, 2, 0, 0, 0, 1];
        absolute.extend(command(81, 0, BASE + 512));
        let h = handle(&mut a, &absolute);
        assert_eq!(submit(&mut a, arch, ch, h, true), Stop::Budget);
        assert_eq!(result(&a), 0);
        assert_eq!(
            a.runtime.render_sound(2, 8000).unwrap(),
            [[-32768; 2], [32512; 2]]
        );
    }
}
#[test]
fn flush_and_quiet_complete_only_the_resource_commands_they_cancel() {
    let mut a = app();
    let ch = channel(&mut a, 128);
    let h = handle(&mut a, &payload(1, &[192; 8], 2));
    assert_eq!(
        submit(&mut a, Architecture::M68k, ch, h, false),
        Stop::Waiting { ticks: 1 }
    );
    loop_cpu(&mut a);
    for code in [4, 3] {
        a.runtime
            .memory
            .write(GuestPtr(BASE + 16), &command(code, 0, 0))
            .unwrap();
        let mut s = std::mem::take(&mut a.runtime.process.sound);
        assert_eq!(
            s.immediate(
                &mut a.runtime.memory,
                &mut a.runtime.process,
                ch,
                GuestPtr(BASE + 16)
            )
            .unwrap(),
            0
        );
        a.runtime.process.sound = s;
        if code == 4 {
            assert_eq!(a.runtime.advance(1).unwrap(), Stop::Waiting { ticks: 1 });
        }
    }
    assert_eq!(a.runtime.advance(1).unwrap(), Stop::Budget);
    assert_eq!(result(&a), 0);
}

#[test]
fn internal_channel_allocation_failure_releases_temporary_storage_and_preserves_sound_errors() {
    for arch in [Architecture::M68k, Architecture::PowerPc] {
        let mut a = app();
        let h = handle(&mut a, &payload(1, &[192], 1));
        let bytes = a.runtime.process.free_memory() - 8;
        assert_ne!(
            a.runtime
                .process
                .new_pointer(&mut a.runtime.memory, bytes, false)
                .unwrap(),
            0
        );
        let free = a.runtime.process.free_memory();
        assert_eq!(submit(&mut a, arch, 0, h, false), Stop::Budget);
        assert_eq!(result(&a), -108);
        assert_eq!(a.runtime.process.free_memory(), free);
        assert!(!a.runtime.process.sound.playing());
    }
}
#[test]
fn resource_play_abis_validate_native_results_before_pop_and_use_the_low_ppc_boolean() {
    use systemless_future::{
        abi::{Abi, Reply, Service},
        cpu::Call,
        memory::Memory,
    };
    for size in [9, 10, 11] {
        let mut m = Memory::default();
        m.map(GuestPtr(0x1000), vec![0; size], Permissions::RW)
            .unwrap();
        let mut c = Cpu::new(Architecture::M68k, 0, 0x1000);
        let Cpu::M68k(c) = &mut c else { panic!() };
        assert!(Abi::trap(0xa805, c, &m).is_err());
        assert_eq!(c.dar[15], 0x1000);
    }
    let mut m = Memory::default();
    let mut bytes = vec![1, 0x55];
    bytes.extend(0x10203040u32.to_be_bytes());
    bytes.extend(0x50607080u32.to_be_bytes());
    bytes.extend([0; 2]);
    m.map(GuestPtr(0x1000), bytes, Permissions::RW).unwrap();
    let mut c = Cpu::new(Architecture::M68k, 0, 0x1000);
    let Cpu::M68k(c) = &mut c else { panic!() };
    assert_eq!(
        Abi::trap(0xa805, c, &m).unwrap(),
        (
            Call::SoundPlay {
                channel: 0x50607080,
                handle: 0x10203040,
                asynchronous: true
            },
            Reply::StackWord(GuestPtr(0x100a))
        )
    );
    let mut c = Cpu::new(Architecture::PowerPc, 0, 0x1000);
    let Cpu::PowerPc(c) = &mut c else { panic!() };
    c.gpr[3] = 0x50607080;
    c.gpr[4] = 0x10203040;
    c.gpr[5] = 0x12340100;
    assert_eq!(
        Abi::import(Service::SndPlay, c, &m).unwrap(),
        (
            Call::SoundPlay {
                channel: 0x50607080,
                handle: 0x10203040,
                asynchronous: false
            },
            Reply::PpcR3
        )
    );
}
#[test]
fn disposal_during_resource_wait_reports_bad_channel_instead_of_targeting_a_replacement() {
    let mut a = app();
    let ch = channel(&mut a, 2);
    let h = handle(&mut a, &payload(1, &[192; 8], 1));
    assert_eq!(
        submit(&mut a, Architecture::M68k, ch, h, false),
        Stop::Waiting { ticks: 1 }
    );
    loop_cpu(&mut a);
    let mut s = std::mem::take(&mut a.runtime.process.sound);
    assert_eq!(
        s.call(
            &mut a.runtime.memory,
            &mut a.runtime.process,
            Op::Dispose {
                channel: ch,
                quiet_now: true
            }
        )
        .unwrap(),
        0
    );
    a.runtime
        .memory
        .write(GuestPtr(ch + 30), &2u16.to_be_bytes())
        .unwrap();
    assert_eq!(
        s.call(
            &mut a.runtime.memory,
            &mut a.runtime.process,
            Op::New(NewChannel {
                destination: GuestPtr(BASE),
                synth: 5,
                init: 4,
                callback: 0
            })
        )
        .unwrap(),
        0
    );
    a.runtime.process.sound = s;
    assert_eq!(a.runtime.memory.u32(GuestPtr(BASE)).unwrap(), ch);
    assert_eq!(a.runtime.advance(1).unwrap(), Stop::Budget);
    assert_eq!(result(&a), -205);
    assert_eq!(a.runtime.process.sound.busy(ch), Some(false));
}

fn pause_file(a: &mut LoadedApplication, arch: Architecture, ch: u32, expected: i16) {
    let errors = (a.runtime.process.mem_error, a.runtime.process.res_error);
    if arch == Architecture::M68k {
        let mut frame = ch.to_be_bytes().to_vec();
        frame.extend([0x5a; 4]);
        a.runtime.memory.write(GuestPtr(STACK), &frame).unwrap();
        a.runtime.cpu = Cpu::new(arch, 0x52000000, STACK);
        if let Cpu::M68k(c) = &mut a.runtime.cpu {
            c.dar[0] = 0x02040008;
            c.dar[1] = 0x12345678;
        }
    } else {
        let tv = a
            .runtime
            .memory
            .u32(GuestPtr(a.info.toc_or_a5 + 24))
            .unwrap();
        let pc = a.runtime.memory.u32(GuestPtr(tv)).unwrap();
        a.runtime.cpu = Cpu::new(arch, pc, STACK);
        if let Cpu::PowerPc(c) = &mut a.runtime.cpu {
            c.gpr[3] = ch;
            c.gpr[4] = 0x12345678;
        }
    }
    assert_eq!(a.runtime.advance(1).unwrap(), Stop::Budget);
    match &a.runtime.cpu {
        Cpu::M68k(c) => {
            assert_eq!(
                (c.dar[0], c.dar[1], c.dar[15]),
                (0x02040008, 0x12345678, STACK + 4)
            );
            assert_eq!(
                a.runtime.memory.u16(GuestPtr(STACK + 4)).unwrap() as i16,
                expected
            );
            assert_eq!(
                a.runtime.memory.read(GuestPtr(STACK + 6), 2).unwrap(),
                [0x5a; 2]
            );
        }
        Cpu::PowerPc(c) => assert_eq!(
            (c.gpr[3] as i32, c.gpr[4]),
            (i32::from(expected), 0x12345678)
        ),
    }
    assert_eq!(
        (a.runtime.process.mem_error, a.runtime.process.res_error),
        errors
    );
}
#[test]
fn file_play_resource_uses_shared_mixer_through_both_cpu_adapters() {
    use systemless_future::{
        files::CreateSpecResource,
        process::resources::{AddResource, Op as ResourceOp},
    };
    for arch in [Architecture::M68k, Architecture::PowerPc] {
        let mut a = app();
        let ch = channel(&mut a, 128);
        let spec = GuestPtr(BASE + 512);
        let mut b = [0; 70];
        b[..2].copy_from_slice(&(-1i16).to_be_bytes());
        b[2..6].copy_from_slice(&2u32.to_be_bytes());
        b[6] = 5;
        b[7..12].copy_from_slice(b"Sound");
        a.runtime.memory.write(spec, &b).unwrap();
        assert_eq!(
            a.runtime
                .process
                .files
                .create_spec_resource(
                    &a.runtime.memory,
                    CreateSpecResource {
                        spec,
                        creator: *b"TEST",
                        kind: *b"rsrc",
                        script: 0
                    }
                )
                .unwrap(),
            0
        );
        assert!(
            a.runtime
                .process
                .resource_file(
                    &mut a.runtime.memory,
                    ResourceOp::OpenSpec {
                        spec,
                        permission: 3
                    }
                )
                .unwrap()
                > 0
        );
        let h = handle(&mut a, &payload(1, &[128, 192, 64], 2));
        a.runtime.memory.write(GuestPtr(BASE + 600), &[0]).unwrap();
        a.runtime
            .process
            .add_resource(
                &a.runtime.memory,
                AddResource {
                    handle: h,
                    kind: *b"snd ",
                    id: 30000,
                    name: GuestPtr(BASE + 600),
                },
            )
            .unwrap();
        assert_eq!(a.runtime.process.res_error, 0);
        a.runtime.process.mem_error = -108;
        a.runtime.process.res_error = -192;
        if arch == Architecture::M68k {
            a.runtime
                .memory
                .map(GuestPtr(0x52000000), vec![0xa8, 0], Permissions::RX)
                .unwrap();
            let mut frame = vec![1, 0];
            frame.extend([0; 12]);
            frame.extend(65534u32.to_be_bytes());
            frame.extend(30000i16.to_be_bytes());
            frame.extend(0i16.to_be_bytes());
            frame.extend(ch.to_be_bytes());
            frame.extend([0x5a; 4]);
            a.runtime.memory.write(GuestPtr(STACK), &frame).unwrap();
            a.runtime.cpu = Cpu::new(arch, 0x52000000, STACK);
            if let Cpu::M68k(c) = &mut a.runtime.cpu {
                c.dar[0] = 0x0d000008;
            }
        } else {
            let vector = a
                .runtime
                .memory
                .u32(GuestPtr(a.info.toc_or_a5 + 8 + 3 * 4))
                .unwrap();
            let pc = a.runtime.memory.u32(GuestPtr(vector)).unwrap();
            a.runtime.cpu = Cpu::new(arch, pc, STACK);
            if let Cpu::PowerPc(c) = &mut a.runtime.cpu {
                c.gpr[3] = ch;
                c.gpr[4] = 0;
                c.gpr[5] = 30000;
                c.gpr[6] = 65534;
                c.gpr[7] = 0;
                c.gpr[8] = 0;
                c.gpr[9] = 0;
                c.gpr[10] = 1;
            }
        }
        assert_eq!(a.runtime.advance(1).unwrap(), Stop::Budget);
        match &a.runtime.cpu {
            Cpu::M68k(c) => {
                assert_eq!((c.dar[0], c.dar[15]), (0x0d000008, STACK + 26));
                assert_eq!(
                    a.runtime.memory.read(GuestPtr(STACK + 26), 4).unwrap(),
                    [0, 0, 0x5a, 0x5a]
                );
            }
            Cpu::PowerPc(c) => assert_eq!(c.gpr[3], 0),
        }
        assert_eq!(
            (a.runtime.process.mem_error, a.runtime.process.res_error),
            (-108, -192)
        );
        assert_eq!(a.runtime.render_sound(1, 8000).unwrap(), [[0; 2]]);
        pause_file(&mut a, arch, ch, 0);
        assert_eq!(a.runtime.render_sound(10, 8000).unwrap(), [[0; 2]; 10]);
        a.runtime.elapse_sound_silently(600).unwrap();
        let sound = std::mem::take(&mut a.runtime.process.sound);
        assert_eq!(
            sound
                .status(
                    &mut a.runtime.memory,
                    &mut a.runtime.process,
                    systemless_future::sound::Status {
                        channel: Some(ch),
                        length: 24,
                        destination: GuestPtr(BASE + 700)
                    }
                )
                .unwrap(),
            0
        );
        a.runtime.process.sound = sound;
        assert_eq!(a.runtime.memory.read(GuestPtr(BASE + 714), 1).unwrap(), [1]);
        pause_file(&mut a, arch, ch, 0);
        assert_eq!(
            a.runtime.render_sound(2, 8000).unwrap(),
            [[16384; 2], [-16384; 2]]
        );
        pause_file(&mut a, arch, ch, 0);
        a.runtime.elapse_sound_silently(600).unwrap();
        assert_eq!(a.runtime.render_sound(3, 8000).unwrap(), [[0; 2]; 3]);
        pause_file(&mut a, arch, ch, 0);
        assert_eq!(
            a.runtime.render_sound(3, 8000).unwrap(),
            [[0; 2], [16384; 2], [-16384; 2]]
        );
        pause_file(&mut a, arch, ch, -211);
    }
}

#[test]
fn file_play_missing_resource_and_nil_async_channel_return_signed_errors() {
    for arch in [Architecture::M68k, Architecture::PowerPc] {
        for (asynchronous, expected) in [(false, -192i16), (true, -205)] {
            let mut a = app();
            a.runtime.process.mem_error = -108;
            a.runtime.process.res_error = -199;
            a.runtime
                .memory
                .write(GuestPtr(0x220), &0xbeefu16.to_be_bytes())
                .unwrap();
            a.runtime
                .memory
                .write(GuestPtr(0xa60), &0xcafeu16.to_be_bytes())
                .unwrap();
            if arch == Architecture::M68k {
                a.runtime
                    .memory
                    .map(GuestPtr(0x52000000), vec![0xa8, 0], Permissions::RX)
                    .unwrap();
                let mut frame = vec![0; 30];
                frame[0] = u8::from(asynchronous);
                frame[18..20].copy_from_slice(&30000i16.to_be_bytes());
                frame[26..].fill(0x5a);
                a.runtime.memory.write(GuestPtr(STACK), &frame).unwrap();
                a.runtime.cpu = Cpu::new(arch, 0x52000000, STACK);
                if let Cpu::M68k(c) = &mut a.runtime.cpu {
                    c.dar[0] = 0x0d000008;
                }
            } else {
                let vector = a
                    .runtime
                    .memory
                    .u32(GuestPtr(a.info.toc_or_a5 + 20))
                    .unwrap();
                let pc = a.runtime.memory.u32(GuestPtr(vector)).unwrap();
                a.runtime.cpu = Cpu::new(arch, pc, STACK);
                if let Cpu::PowerPc(c) = &mut a.runtime.cpu {
                    c.gpr[3..11].fill(0);
                    c.gpr[5] = 30000;
                    c.gpr[10] = u32::from(asynchronous);
                }
            }
            assert_eq!(a.runtime.advance(1).unwrap(), Stop::Budget);
            match &a.runtime.cpu {
                Cpu::M68k(c) => {
                    assert_eq!(c.dar[15], STACK + 26);
                    assert_eq!(
                        a.runtime.memory.u16(GuestPtr(STACK + 26)).unwrap() as i16,
                        expected
                    );
                    assert_eq!(
                        a.runtime.memory.read(GuestPtr(STACK + 28), 2).unwrap(),
                        [0x5a; 2]
                    );
                }
                Cpu::PowerPc(c) => assert_eq!(c.gpr[3] as i32, i32::from(expected)),
            }
            assert_eq!(
                (a.runtime.process.mem_error, a.runtime.process.res_error),
                (-108, -199)
            );
            assert_eq!(a.runtime.memory.u16(GuestPtr(0x220)).unwrap(), 0xbeef);
            assert_eq!(a.runtime.memory.u16(GuestPtr(0xa60)).unwrap(), 0xcafe);
            assert_eq!(a.runtime.render_sound(1, 8000).unwrap(), [[0; 2]]);
            let idle = channel(&mut a, 128);
            pause_file(&mut a, arch, idle, -211);
            pause_file(&mut a, arch, 0, -205);
        }
    }
}

#[test]
fn pause_file_rejects_ordinary_resource_play_without_stopping_its_audio() {
    for arch in [Architecture::M68k, Architecture::PowerPc] {
        let mut a = app();
        a.runtime
            .memory
            .map(GuestPtr(0x52000000), vec![0xa8, 0], Permissions::RX)
            .unwrap();
        let ch = channel(&mut a, 128);
        let h = handle(&mut a, &payload(1, &[192, 64], 1));
        assert_eq!(submit(&mut a, arch, ch, h, true), Stop::Budget);
        pause_file(&mut a, arch, ch, -211);
        assert_eq!(
            a.runtime.render_sound(2, 8000).unwrap(),
            [[16384; 2], [-16384; 2]]
        );
    }
}
