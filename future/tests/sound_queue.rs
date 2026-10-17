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
    let f = support::pef_program(&[0x4e800020], &[b"SndDoCommand", b"SndDoImmediate"], 32);
    let mut a = LoadedApplication::pef(&f.bytes, None).unwrap();
    a.runtime.memory.write(GuestPtr(0x260), &[7]).unwrap();
    a.runtime
        .memory
        .map(GuestPtr(BASE), vec![0; 4096], Permissions::RW)
        .unwrap();
    // Queue trap, immediate trap, a stable guest loop.
    a.runtime
        .memory
        .map(
            GuestPtr(0x51000000),
            vec![0xa8, 3, 0xa8, 4, 0x60, 0xfe],
            Permissions::RX,
        )
        .unwrap();
    a
}
fn channel(a: &mut LoadedApplication, length: u16, callback: u32) -> u32 {
    if length != 128 {
        a.runtime
            .memory
            .write(GuestPtr(BASE), &(BASE + 1536).to_be_bytes())
            .unwrap();
        a.runtime
            .memory
            .write(GuestPtr(BASE + 1536 + 30), &length.to_be_bytes())
            .unwrap();
    } else {
        a.runtime.memory.write(GuestPtr(BASE), &[0; 4]).unwrap();
    }
    let mut manager = std::mem::take(&mut a.runtime.process.sound);
    let e = manager
        .call(
            &mut a.runtime.memory,
            &mut a.runtime.process,
            Op::New(NewChannel {
                destination: GuestPtr(BASE),
                synth: 5,
                init: 4,
                callback,
            }),
        )
        .unwrap();
    a.runtime.process.sound = manager;
    assert_eq!(e, 0);
    a.runtime.memory.u32(GuestPtr(BASE)).unwrap()
}
fn bytes(code: u16, param: u16, value: u32) -> [u8; 8] {
    let mut b = [0; 8];
    b[..2].copy_from_slice(&code.to_be_bytes());
    b[2..4].copy_from_slice(&param.to_be_bytes());
    b[4..].copy_from_slice(&value.to_be_bytes());
    b
}
fn submit(
    a: &mut LoadedApplication,
    arch: Architecture,
    ch: u32,
    b: [u8; 8],
    no_wait: bool,
    immediate: bool,
) -> Stop {
    a.runtime.memory.write(GuestPtr(BASE + 16), &b).unwrap();
    a.runtime.process.mem_error = -108;
    a.runtime.process.res_error = -192;
    if arch == Architecture::M68k {
        let mut frame = Vec::new();
        if !immediate {
            frame.extend([u8::from(no_wait), 0xa5]);
        }
        frame.extend((BASE + 16).to_be_bytes());
        frame.extend(ch.to_be_bytes());
        frame.extend([0x5a; 4]);
        a.runtime.memory.write(GuestPtr(STACK), &frame).unwrap();
        a.runtime.cpu = Cpu::new(arch, 0x51000000 + if immediate { 2 } else { 0 }, STACK);
        if let Cpu::M68k(c) = &mut a.runtime.cpu {
            c.dar[0] = 0x12345678;
            c.dar[13] = 0xfeed1234;
        }
    } else {
        let tv = a
            .runtime
            .memory
            .u32(GuestPtr(
                a.info.toc_or_a5 + 8 + if immediate { 4 } else { 0 },
            ))
            .unwrap();
        let pc = a.runtime.memory.u32(GuestPtr(tv)).unwrap();
        a.runtime.cpu = Cpu::new(arch, pc, STACK);
        if let Cpu::PowerPc(c) = &mut a.runtime.cpu {
            c.gpr[3] = ch;
            c.gpr[4] = BASE + 16;
            c.gpr[5] = u32::from(no_wait);
            c.gpr[13] = 0xfeed1234;
        }
    }
    a.runtime.advance(1).unwrap()
}
fn result(a: &LoadedApplication, arch: Architecture, immediate: bool) -> i16 {
    assert_eq!(
        (a.runtime.process.mem_error, a.runtime.process.res_error),
        (-108, -192)
    );
    match &a.runtime.cpu {
        Cpu::M68k(c) => {
            assert_eq!(arch, Architecture::M68k);
            let size = if immediate { 8 } else { 10 };
            assert_eq!(c.dar[15], STACK + size);
            assert_eq!(c.dar[0], 0x12345678);
            assert_eq!(
                a.runtime.memory.u16(GuestPtr(STACK + size + 2)).unwrap(),
                0x5a5a
            );
            a.runtime.memory.u16(GuestPtr(STACK + size)).unwrap() as i16
        }
        Cpu::PowerPc(c) => c.gpr[3] as i16,
    }
}
fn command(
    a: &mut LoadedApplication,
    arch: Architecture,
    ch: u32,
    b: [u8; 8],
    immediate: bool,
) -> i16 {
    assert_eq!(submit(a, arch, ch, b, true, immediate), Stop::Budget);
    result(a, arch, immediate)
}
fn header(a: &mut LoadedApplication, at: u32, samples: &[u8]) -> u32 {
    let mut h = vec![0; 22];
    h[4..8].copy_from_slice(&(samples.len() as u32).to_be_bytes());
    h[8..12].copy_from_slice(&(8000u32 << 16).to_be_bytes());
    h[21] = 60;
    h.extend(samples);
    a.runtime.memory.write(GuestPtr(at), &h).unwrap();
    at
}
#[test]
fn both_cpus_queue_live_commands_render_across_buffers_and_flush_without_quieting() {
    for arch in [Architecture::M68k, Architecture::PowerPc] {
        let mut a = app();
        let ch = channel(&mut a, 128, 0);
        let first = header(&mut a, BASE + 64, &[0, 128]);
        let second = header(&mut a, BASE + 256, &[255, 64]);
        assert_eq!(command(&mut a, arch, ch, bytes(81, 0, first), false), 0);
        assert_eq!(command(&mut a, arch, ch, bytes(43, 128, 0), false), 0);
        assert_eq!(command(&mut a, arch, ch, bytes(81, 0, second), false), 0);
        assert_eq!(a.runtime.memory.u16(GuestPtr(ch + 32)).unwrap(), 1);
        assert_eq!(a.runtime.memory.u16(GuestPtr(ch + 34)).unwrap(), 3);
        assert_eq!(
            a.runtime.memory.read(GuestPtr(ch + 44), 8).unwrap(),
            bytes(43, 128, 0)
        );
        // Submitted command records are copied; sample buffers remain live.
        a.runtime
            .memory
            .write(GuestPtr(BASE + 16), &[0xff; 8])
            .unwrap();
        a.runtime
            .memory
            .write(GuestPtr(second + 23), &[192])
            .unwrap();
        let out = a.runtime.render_sound(5, 8000).unwrap();
        assert_eq!(out, [[-32768; 2], [0; 2], [16319; 2], [8224; 2], [0; 2]]);
        assert_eq!(a.runtime.process.sound.busy(ch), Some(false));
        assert_eq!(a.runtime.memory.u16(GuestPtr(ch + 32)).unwrap(), 3);
        assert_eq!(command(&mut a, arch, ch, bytes(81, 0, first), false), 0);
        assert_eq!(command(&mut a, arch, ch, bytes(81, 0, second), false), 0);
        assert_eq!(command(&mut a, arch, ch, bytes(4, 0, 0), true), 0);
        assert!(a.runtime.process.sound.busy(ch).unwrap());
        assert_eq!(
            a.runtime.render_sound(4, 8000).unwrap(),
            [[-16448; 2], [0; 2], [0; 2], [0; 2]]
        );
    }
}
#[test]
fn full_queue_no_wait_is_atomic_and_waiting_caller_resumes_without_guest_spin() {
    for arch in [Architecture::M68k, Architecture::PowerPc] {
        let mut a = app();
        let ch = channel(&mut a, 2, 0);
        let h = header(&mut a, BASE + 64, &[192; 16]);
        for b in [bytes(81, 0, h), bytes(43, 20, 0), bytes(43, 30, 0)] {
            assert_eq!(command(&mut a, arch, ch, b, false), 0);
        }
        let before = a.runtime.memory.read(GuestPtr(ch), 52).unwrap().to_vec();
        assert_eq!(command(&mut a, arch, ch, bytes(43, 40, 0), false), -203);
        assert_eq!(a.runtime.memory.read(GuestPtr(ch), 52).unwrap(), before);
        assert_eq!(
            submit(&mut a, arch, ch, bytes(43, 50, 0), false, false),
            Stop::Waiting { ticks: 1 }
        );
        a.runtime
            .memory
            .map(
                GuestPtr(0x53000000),
                if arch == Architecture::M68k {
                    vec![0x60, 0xfe]
                } else {
                    0x48000000u32.to_be_bytes().to_vec()
                },
                Permissions::RX,
            )
            .unwrap();
        match &mut a.runtime.cpu {
            Cpu::M68k(c) => c.pc = 0x53000000,
            Cpu::PowerPc(c) => c.pc = 0x53000000,
        }
        let instructions = a.runtime.instructions;
        assert_eq!(
            a.runtime.advance(10000).unwrap(),
            Stop::Waiting { ticks: 1 }
        );
        assert_eq!(a.runtime.instructions, instructions);
        a.runtime.elapse_sound_silently(u32::MAX).unwrap();
        // The copied command survives arbitrary caller edits while suspended.
        a.runtime
            .memory
            .write(GuestPtr(BASE + 16), &[0xff; 8])
            .unwrap();
        assert_eq!(a.runtime.advance(1).unwrap(), Stop::Budget);
        assert_eq!(result(&a, arch, false), 0);
        assert_eq!(command(&mut a, arch, ch, bytes(45, 0, BASE + 512), true), 0);
        assert_eq!(a.runtime.memory.u16(GuestPtr(BASE + 512)).unwrap(), 50);
    }
}
#[test]
fn queued_silent_clock_matches_rendered_output_and_corrupt_pending_storage_faults_before_advancement(
) {
    let mut a = app();
    let ch = channel(&mut a, 128, 0);
    let first = header(&mut a, BASE + 64, &[192; 4]);
    let second = header(&mut a, BASE + 256, &[224; 4]);
    for h in [first, second] {
        assert_eq!(
            command(&mut a, Architecture::PowerPc, ch, bytes(81, 0, h), false),
            0
        );
    }
    let head = a.runtime.memory.u16(GuestPtr(ch + 32)).unwrap();
    a.runtime
        .memory
        .write(GuestPtr(ch + 36 + u32::from(head) * 8), &[0xff])
        .unwrap();
    assert!(a.runtime.render_sound(100, 48000).is_err());
    a.runtime
        .memory
        .write(
            GuestPtr(ch + 36 + u32::from(head) * 8),
            &bytes(81, 0, second),
        )
        .unwrap();
    assert_eq!(a.runtime.render_sound(1, 8000).unwrap(), [[16384; 2]]);
    a.runtime.elapse_sound_silently(1).unwrap();
    assert_eq!(a.runtime.process.sound.busy(ch), Some(false));
    assert_eq!(a.runtime.memory.read(GuestPtr(ch + 20), 8).unwrap(), [0; 8]);
}
#[test]
fn callbacks_are_budgeted_guest_execution_and_restore_interrupted_cpu_state() {
    for arch in [Architecture::M68k, Architecture::PowerPc] {
        let mut a = app();
        let routine = if arch == Architecture::M68k {
            // channel=8(sp), cmd=4(sp); copy param2 to userInfo; trash preserved
            // registers deliberately, then Pascal-return (pop eight bytes).
            let words = [
                0x206fu16, 8, 0x226f, 4, 0x2169, 4, 12, 0x4251, 0x7e55, 0x225f, 0x508f, 0x4ed1,
            ];
            a.runtime
                .memory
                .map(
                    GuestPtr(0x52000000),
                    words.into_iter().flat_map(u16::to_be_bytes).collect(),
                    Permissions::RX,
                )
                .unwrap();
            0x52000000
        } else {
            // lwz r5,4(r4); stw r5,12(r3); li r13,85; blr
            let words = [
                0x80a40004u32,
                0x90a3000c,
                0x39a00055,
                0xb1a40000,
                0x4e800020,
            ];
            a.runtime
                .memory
                .map(
                    GuestPtr(0x52000000),
                    words.into_iter().flat_map(u32::to_be_bytes).collect(),
                    Permissions::RX,
                )
                .unwrap();
            let mut rd = [0; 32];
            rd[..4].copy_from_slice(&[0xaa, 0xfe, 7, 0]);
            rd[12..16].copy_from_slice(&0x3c0u32.to_be_bytes());
            rd[17] = 1;
            rd[20..24].copy_from_slice(&(BASE + 768).to_be_bytes());
            a.runtime.memory.write(GuestPtr(BASE + 736), &rd).unwrap();
            a.runtime
                .memory
                .write(GuestPtr(BASE + 768), &0x52000000u32.to_be_bytes())
                .unwrap();
            a.runtime
                .memory
                .write(GuestPtr(BASE + 772), &0xfeed0000u32.to_be_bytes())
                .unwrap();
            BASE + 736
        };
        let ch = channel(&mut a, 128, routine);
        let h = header(&mut a, BASE + 64, &[192]);
        assert_eq!(command(&mut a, arch, ch, bytes(81, 0, h), false), 0);
        assert_eq!(
            command(&mut a, arch, ch, bytes(13, 42, 0x1234abcd), false),
            0
        );
        assert_eq!(
            a.runtime.render_sound(2, 8000).unwrap(),
            [[16384; 2], [0; 2]]
        );
        assert_eq!(a.runtime.memory.u32(GuestPtr(ch + 12)).unwrap(), 0);
        // Audio rendering only schedules. The callback has not executed yet.
        a.runtime.cpu = Cpu::new(
            arch,
            if arch == Architecture::M68k {
                0x51000004
            } else {
                0x52000010
            },
            STACK,
        );
        match &mut a.runtime.cpu {
            Cpu::M68k(c) => {
                c.dar[7] = 0xfeedbeef;
                c.dar[13] = 0x12345678;
            }
            Cpu::PowerPc(c) => {
                c.gpr[13] = 0xfeedbeef;
                c.lr = 0x52000010;
            }
        }
        let pc = a.runtime.cpu.pc();
        assert_eq!(a.runtime.advance(1).unwrap(), Stop::Budget);
        assert_ne!(a.runtime.cpu.pc(), pc);
        assert_eq!(a.runtime.process.sound.busy(ch), Some(true));
        let steps = if arch == Architecture::M68k { 9 } else { 6 };
        assert_eq!(a.runtime.advance(steps - 1).unwrap(), Stop::Budget);
        assert_eq!(a.runtime.cpu.pc(), pc);
        assert_eq!(a.runtime.memory.u32(GuestPtr(ch + 12)).unwrap(), 0x1234abcd);
        assert_eq!(a.runtime.process.sound.busy(ch), Some(false));
        assert_eq!(
            (a.runtime.process.mem_error, a.runtime.process.res_error),
            (-108, -192)
        );
        match &a.runtime.cpu {
            Cpu::M68k(c) => {
                assert_eq!(c.dar[7], 0xfeedbeef);
                assert_eq!(c.dar[13], 0x12345678);
                assert_eq!(c.dar[15], STACK);
            }
            Cpu::PowerPc(c) => {
                assert_eq!(c.gpr[13], 0xfeedbeef);
                assert_eq!(c.gpr[1], STACK);
            }
        }
    }
}

#[test]
fn silent_queued_advancement_matches_a_48khz_sink_across_fractional_buffer_boundaries() {
    let mut apps = [app(), app()];
    let mut channels = Vec::new();
    for a in &mut apps {
        let ch = channel(a, 128, 0);
        channels.push(ch);
        let first = header(a, BASE + 64, &[192; 250]);
        let samples: Vec<u8> = (0..500).map(|i| i as u8).collect();
        let second = header(a, BASE + 512, &samples);
        a.runtime
            .memory
            .write(GuestPtr(first + 8), &(22050u32 << 16).to_be_bytes())
            .unwrap();
        a.runtime
            .memory
            .write(GuestPtr(second + 8), &(11025u32 << 16).to_be_bytes())
            .unwrap();
        for h in [first, second] {
            assert_eq!(
                command(a, Architecture::PowerPc, ch, bytes(81, 0, h), false),
                0
            );
        }
    }
    apps[0].runtime.elapse_sound_silently(1).unwrap();
    apps[1].runtime.render_sound(800, 48000).unwrap();
    let tail0 = apps[0].runtime.render_sound(3000, 48000).unwrap();
    let tail1 = apps[1].runtime.render_sound(3000, 48000).unwrap();
    assert_eq!(tail0, tail1);
    for (a, ch) in apps.iter().zip(channels) {
        assert_eq!(a.runtime.process.sound.busy(ch), Some(false));
    }
}
#[test]
fn queue_abis_validate_complete_pascal_results_and_use_low_boolean_byte_on_ppc() {
    use systemless_future::{
        abi::{Abi, Reply, Service},
        cpu::Call,
        memory::Memory,
    };
    for size in [10, 11] {
        let mut m = Memory::default();
        m.map(GuestPtr(0x1000), vec![0; size], Permissions::RW)
            .unwrap();
        let mut cpu = Cpu::new(Architecture::M68k, 0, 0x1000);
        let Cpu::M68k(c) = &mut cpu else { panic!() };
        assert!(Abi::trap(0xa803, c, &m).is_err());
        assert_eq!(c.dar[15], 0x1000);
    }
    let mut m = Memory::default();
    let mut frame = vec![0x01, 0x55];
    frame.extend(0x10203040u32.to_be_bytes());
    frame.extend(0x50607080u32.to_be_bytes());
    frame.extend([0; 2]);
    m.map(GuestPtr(0x1000), frame, Permissions::RW).unwrap();
    let mut cpu = Cpu::new(Architecture::M68k, 0, 0x1000);
    let Cpu::M68k(c) = &mut cpu else { panic!() };
    assert_eq!(
        Abi::trap(0xa803, c, &m).unwrap(),
        (
            Call::SoundQueue {
                channel: 0x50607080,
                command: GuestPtr(0x10203040),
                no_wait: true
            },
            Reply::StackWord(GuestPtr(0x100a))
        )
    );
    let mut cpu = Cpu::new(Architecture::PowerPc, 0, 0x1000);
    let Cpu::PowerPc(c) = &mut cpu else { panic!() };
    c.gpr[3] = 0x50607080;
    c.gpr[4] = 0x10203040;
    c.gpr[5] = 0x12340100;
    assert_eq!(
        Abi::import(Service::SndDoCommand, c, &m).unwrap(),
        (
            Call::SoundQueue {
                channel: 0x50607080,
                command: GuestPtr(0x10203040),
                no_wait: false
            },
            Reply::PpcR3
        )
    );
}

#[test]
fn caller_sized_rings_use_the_declared_extent_and_preserve_the_following_guard() {
    let mut a = app();
    let ch = channel(&mut a, 129, 0);
    let guard = ch + 36 + 129 * 8;
    a.runtime.memory.write(GuestPtr(guard), &[0xa5; 4]).unwrap();
    let h = header(&mut a, BASE + 64, &[192; 16]);
    assert_eq!(
        command(&mut a, Architecture::PowerPc, ch, bytes(81, 0, h), false),
        0
    );
    for _ in 0..129 {
        assert_eq!(
            command(
                &mut a,
                Architecture::PowerPc,
                ch,
                bytes(46, 0, 0x01000100),
                false
            ),
            0
        );
    }
    assert_eq!(
        command(&mut a, Architecture::PowerPc, ch, bytes(0, 0, 0), false),
        -203
    );
    assert_eq!(
        a.runtime.memory.read(GuestPtr(guard), 4).unwrap(),
        [0xa5; 4]
    );
    a.runtime.elapse_sound_silently(1).unwrap();
    assert_eq!(a.runtime.process.sound.busy(ch), Some(false));
    assert_eq!(
        a.runtime.memory.read(GuestPtr(guard), 4).unwrap(),
        [0xa5; 4]
    );
}

#[test]
fn a_suspended_submission_does_not_target_a_replacement_at_the_same_channel_address() {
    for arch in [Architecture::M68k, Architecture::PowerPc] {
        let mut a = app();
        let ch = channel(&mut a, 2, 0);
        let h = header(&mut a, BASE + 64, &[192; 16]);
        for b in [bytes(81, 0, h), bytes(43, 20, 0), bytes(43, 30, 0)] {
            assert_eq!(command(&mut a, arch, ch, b, false), 0);
        }
        assert_eq!(
            submit(&mut a, arch, ch, bytes(43, 50, 0), false, false),
            Stop::Waiting { ticks: 1 }
        );
        let mut manager = std::mem::take(&mut a.runtime.process.sound);
        assert_eq!(
            manager
                .call(
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
        // Dispose clears internal header fields; the caller initializes its
        // chosen qLength again before reusing caller-owned record storage.
        a.runtime
            .memory
            .write(GuestPtr(ch + 30), &2u16.to_be_bytes())
            .unwrap();
        assert_eq!(
            manager
                .call(
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
        a.runtime.process.sound = manager;
        assert_eq!(a.runtime.memory.u32(GuestPtr(BASE)).unwrap(), ch);
        a.runtime
            .memory
            .map(
                GuestPtr(0x53000000),
                if arch == Architecture::M68k {
                    vec![0x60, 0xfe]
                } else {
                    0x48000000u32.to_be_bytes().to_vec()
                },
                Permissions::RX,
            )
            .unwrap();
        match &mut a.runtime.cpu {
            Cpu::M68k(c) => c.pc = 0x53000000,
            Cpu::PowerPc(c) => c.pc = 0x53000000,
        }
        assert_eq!(a.runtime.advance(1).unwrap(), Stop::Budget);
        assert_eq!(result(&a, arch, false), -205);
        assert_eq!(command(&mut a, arch, ch, bytes(45, 0, BASE + 512), true), 0);
        assert_eq!(a.runtime.memory.u16(GuestPtr(BASE + 512)).unwrap(), 255);
    }
}
