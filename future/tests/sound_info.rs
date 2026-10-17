mod support;
use systemless_future::{
    cpu::{Architecture, Cpu},
    load::{LoadedApplication, STACK_BASE},
    memory::{GuestPtr, Permissions},
    runtime::Stop,
    sound::{self, NewChannel, Op},
};
const BASE: u32 = 0x50000000;
const STACK: u32 = STACK_BASE + 0x8000;
fn app() -> LoadedApplication {
    let f = support::pef_program(
        &[0x4e800020],
        &[
            b"SndSoundManagerVersion",
            b"SndChannelStatus",
            b"SndManagerStatus",
            b"SndDoImmediate",
        ],
        32,
    );
    let mut a = LoadedApplication::pef(&f.bytes, None).unwrap();
    a.runtime
        .memory
        .map(GuestPtr(BASE), vec![0xa5; 4096], Permissions::RW)
        .unwrap();
    a.runtime
        .memory
        .map(
            GuestPtr(0x51000000),
            vec![0xa8, 0, 0xa8, 4],
            Permissions::RX,
        )
        .unwrap();
    a.runtime.memory.write(GuestPtr(0x260), &[7]).unwrap();
    a
}
fn channel(a: &mut LoadedApplication, init: u32) -> u32 {
    a.runtime.memory.write(GuestPtr(BASE), &[0; 4]).unwrap();
    let mut s = std::mem::take(&mut a.runtime.process.sound);
    assert_eq!(
        s.call(
            &mut a.runtime.memory,
            &mut a.runtime.process,
            Op::New(NewChannel {
                destination: GuestPtr(BASE),
                synth: 5,
                init,
                callback: 0
            })
        )
        .unwrap(),
        0
    );
    a.runtime.process.sound = s;
    a.runtime.memory.u32(GuestPtr(BASE)).unwrap()
}
fn invoke(a: &mut LoadedApplication, arch: Architecture, index: u32, args: &[u32]) -> u32 {
    a.runtime.process.mem_error = -108;
    a.runtime.process.res_error = -192;
    let size = match index {
        0 => 0,
        1 => 10,
        2 => 6,
        3 => 8,
        _ => panic!(),
    };
    if arch == Architecture::M68k {
        let mut bytes = Vec::new();
        match index {
            0 => {}
            1 => {
                bytes.extend(args[2].to_be_bytes());
                bytes.extend((args[1] as u16).to_be_bytes());
                bytes.extend(args[0].to_be_bytes());
            }
            2 => {
                bytes.extend(args[1].to_be_bytes());
                bytes.extend((args[0] as u16).to_be_bytes());
            }
            3 => {
                bytes.extend(args[1].to_be_bytes());
                bytes.extend(args[0].to_be_bytes());
            }
            _ => unreachable!(),
        };
        bytes.extend([0x5a; 8]);
        a.runtime.memory.write(GuestPtr(STACK), &bytes).unwrap();
        a.runtime.cpu = Cpu::new(arch, 0x51000000 + if index == 3 { 2 } else { 0 }, STACK);
        let Cpu::M68k(c) = &mut a.runtime.cpu else {
            panic!()
        };
        c.dar[0] = match index {
            0 => 0x000c0008,
            1 => 0x05100008,
            2 => 0x03140008,
            _ => 0x12345678,
        };
    } else {
        let tv = a
            .runtime
            .memory
            .u32(GuestPtr(a.info.toc_or_a5 + 8 + index * 4))
            .unwrap();
        let pc = a.runtime.memory.u32(GuestPtr(tv)).unwrap();
        a.runtime.cpu = Cpu::new(arch, pc, STACK);
        let Cpu::PowerPc(c) = &mut a.runtime.cpu else {
            panic!()
        };
        for (i, arg) in args.iter().enumerate() {
            c.gpr[3 + i] = *arg;
        }
        if index == 0 {
            c.gpr[3] = BASE + 768;
        }
    }
    assert_eq!(a.runtime.advance(1).unwrap(), Stop::Budget);
    assert_eq!(
        (a.runtime.process.mem_error, a.runtime.process.res_error),
        (-108, -192)
    );
    match &a.runtime.cpu {
        Cpu::M68k(c) => {
            assert_eq!(c.dar[15], STACK + size);
            if index == 0 {
                a.runtime.memory.u32(GuestPtr(STACK)).unwrap()
            } else {
                assert_eq!(
                    a.runtime.memory.u16(GuestPtr(STACK + size + 2)).unwrap(),
                    0x5a5a
                );
                a.runtime.memory.u16(GuestPtr(STACK + size)).unwrap() as i16 as i32 as u32
            }
        }
        Cpu::PowerPc(c) => {
            if index == 0 {
                a.runtime.memory.u32(GuestPtr(BASE + 768)).unwrap()
            } else {
                c.gpr[3]
            }
        }
    }
}
fn extended(
    a: &mut LoadedApplication,
    channels: u32,
    bits: u16,
    samples: &[u8],
    external: bool,
) -> u32 {
    let at = BASE + 128;
    let mut b = vec![0; 64];
    if external {
        b[..4].copy_from_slice(&(BASE + 1024).to_be_bytes());
    }
    b[4..8].copy_from_slice(&channels.to_be_bytes());
    b[8..12].copy_from_slice(&(8000u32 << 16).to_be_bytes());
    b[20] = 0xff;
    b[21] = 60;
    b[22..26].copy_from_slice(
        &((samples.len() * 8 / (channels as usize * usize::from(bits))) as u32).to_be_bytes(),
    );
    b[48..50].copy_from_slice(&bits.to_be_bytes());
    if external {
        a.runtime
            .memory
            .write(GuestPtr(BASE + 1024), samples)
            .unwrap();
    } else {
        b.extend(samples);
    }
    a.runtime.memory.write(GuestPtr(at), &b).unwrap();
    at
}
fn start(a: &mut LoadedApplication, arch: Architecture, ch: u32, header: u32) -> i16 {
    let mut cmd = 81u16.to_be_bytes().to_vec();
    cmd.extend([0; 2]);
    cmd.extend(header.to_be_bytes());
    a.runtime.memory.write(GuestPtr(BASE + 32), &cmd).unwrap();
    invoke(a, arch, 3, &[ch, BASE + 32]) as i16
}
fn status(a: &mut LoadedApplication, arch: Architecture, ch: u32) -> Vec<u8> {
    assert_eq!(invoke(a, arch, 1, &[ch, 28, BASE + 512]), 0);
    a.runtime
        .memory
        .read(GuestPtr(BASE + 512), 28)
        .unwrap()
        .to_vec()
}
#[test]
fn both_cpu_status_apis_report_registered_channels_real_busy_state_and_truthful_profile() {
    for arch in [Architecture::M68k, Architecture::PowerPc] {
        let mut a = app();
        assert_eq!(invoke(&mut a, arch, 0, &[]), 0x03008000);
        assert_eq!(
            systemless_future::environment::gestalt(u32::from_be_bytes(*b"snd ")).unwrap(),
            0x1c01
        );
        assert_eq!(invoke(&mut a, arch, 2, &[8, BASE + 512]), 0);
        assert_eq!(
            a.runtime.memory.read(GuestPtr(BASE + 512), 8).unwrap(),
            [0, 0, 0, 0, 0, 0, 0xa5, 0xa5]
        );
        let ch = channel(&mut a, 0xc4);
        let other = channel(&mut a, 0x80);
        let free = a.runtime.process.free_memory();
        let idle = status(&mut a, arch, ch);
        assert_eq!(idle[12], 0);
        assert_eq!(&idle[16..20], &0xc4u32.to_be_bytes());
        assert_eq!(&idle[24..], [0xa5; 4]);
        let data: [u8; 8] = [-32768i16, 32767, 1000, 3000]
            .into_iter()
            .flat_map(i16::to_be_bytes)
            .collect::<Vec<_>>()
            .try_into()
            .unwrap();
        let h = extended(&mut a, 2, 16, &data, false);
        assert_eq!(start(&mut a, arch, ch, h), 0);
        assert_eq!(status(&mut a, arch, ch)[12], 1);
        assert_eq!(a.runtime.render_sound(1, 8000).unwrap(), [[-32768, 32767]]);
        assert_eq!(status(&mut a, arch, ch)[12], 1);
        assert_eq!(a.runtime.render_sound(1, 8000).unwrap(), [[1000, 3000]]);
        assert_eq!(status(&mut a, arch, ch)[12], 0);
        assert_eq!(invoke(&mut a, arch, 2, &[6, BASE + 512]), 0);
        assert_eq!(a.runtime.memory.u16(GuestPtr(BASE + 514)).unwrap(), 2);
        assert_eq!(a.runtime.process.free_memory(), free);
        let mut s = std::mem::take(&mut a.runtime.process.sound);
        assert_eq!(
            s.call(
                &mut a.runtime.memory,
                &mut a.runtime.process,
                Op::Dispose {
                    channel: other,
                    quiet_now: true
                }
            )
            .unwrap(),
            0
        );
        a.runtime.process.sound = s;
        assert_eq!(invoke(&mut a, arch, 2, &[6, BASE + 512]), 0);
        assert_eq!(a.runtime.memory.u16(GuestPtr(BASE + 514)).unwrap(), 1);
    }
}
#[test]
fn extended_pcm_is_interleaved_big_endian_live_and_resampled_without_channel_bleed() {
    for arch in [Architecture::M68k, Architecture::PowerPc] {
        let mut a = app();
        let ch = channel(&mut a, 0xc0);
        let data: [u8; 8] = [0i16, 10000, 20000, -10000]
            .into_iter()
            .flat_map(i16::to_be_bytes)
            .collect::<Vec<_>>()
            .try_into()
            .unwrap();
        let h = extended(&mut a, 2, 16, &data, true);
        assert_eq!(start(&mut a, arch, ch, h), 0);
        assert_eq!(
            a.runtime.render_sound(2, 16000).unwrap(),
            [[0, 10000], [10000, 0]]
        );
        a.runtime
            .memory
            .write(GuestPtr(BASE + 1028), &3000i16.to_be_bytes())
            .unwrap();
        assert_eq!(
            a.runtime.render_sound(2, 16000).unwrap(),
            [[3000, -10000], [3000, -10000]]
        );
        let h = extended(&mut a, 2, 8, &[0, 255, 128, 64], false);
        assert_eq!(start(&mut a, arch, ch, h), 0);
        assert_eq!(
            a.runtime.render_sound(4, 16000).unwrap(),
            [[-32768, 32512], [-16384, 8064], [0, -16384], [0, -16384]]
        );
        let mono = channel(&mut a, 0x84);
        let data: Vec<_> = [1000i16, 3000, -2000, -4000]
            .into_iter()
            .flat_map(i16::to_be_bytes)
            .collect();
        let h = extended(&mut a, 2, 16, &data, false);
        assert_eq!(start(&mut a, arch, mono, h), 0);
        assert_eq!(
            a.runtime.render_sound(2, 8000).unwrap(),
            [[2000; 2], [-3000; 2]]
        );
    }
}
#[test]
fn status_errors_and_extended_bounds_do_not_publish_partial_outputs_or_advance_playback() {
    for arch in [Architecture::M68k, Architecture::PowerPc] {
        let mut a = app();
        let ch = channel(&mut a, 0xc4);
        for (c, len, p, e) in [
            (ch, 23, BASE + 512, -50),
            (ch, 24, 0, -50),
            (0, 24, BASE + 512, -205),
            (0xfffffeed, 24, BASE + 512, -205),
        ] {
            assert_eq!(invoke(&mut a, arch, 1, &[c, len, p]) as i16, e);
            assert_eq!(
                a.runtime.memory.read(GuestPtr(BASE + 512), 24).unwrap(),
                [0xa5; 24]
            );
        }
        assert_eq!(invoke(&mut a, arch, 2, &[5, BASE + 512]) as i16, -50);
        let h = extended(&mut a, 2, 16, &[0; 8], false);
        for (at, value) in [
            (4, 3u32.to_be_bytes().to_vec()),
            (22, u32::MAX.to_be_bytes().to_vec()),
            (48, 24u16.to_be_bytes().to_vec()),
        ] {
            let saved = a
                .runtime
                .memory
                .read(GuestPtr(h + at), value.len())
                .unwrap()
                .to_vec();
            a.runtime.memory.write(GuestPtr(h + at), &value).unwrap();
            assert_eq!(start(&mut a, arch, ch, h), -206);
            assert!(!a.runtime.process.sound.playing());
            a.runtime.memory.write(GuestPtr(h + at), &saved).unwrap();
        }
        let mut m = systemless_future::memory::Memory::default();
        m.map(GuestPtr(0x1000), vec![0; 24], Permissions::RX)
            .unwrap();
        let mut p = systemless_future::process::Process::default();
        let s = sound::Manager::default();
        assert!(s
            .status(
                &mut m,
                &mut p,
                sound::Status {
                    channel: None,
                    length: 6,
                    destination: GuestPtr(0x1000)
                }
            )
            .is_err());
        assert_eq!(m.read(GuestPtr(0x1000), 24).unwrap(), [0; 24]);
    }
}
#[test]
fn sound_dispatch_validates_complete_frames_and_does_not_alias_other_selector_families() {
    use systemless_future::{abi::Abi, memory::Memory};
    for (selector, size) in [(0x000c0008, 3), (0x05100008, 11), (0x03140008, 7)] {
        let mut m = Memory::default();
        m.map(GuestPtr(0x1000), vec![0; size], Permissions::RW)
            .unwrap();
        let mut c = Cpu::new(Architecture::M68k, 0, 0x1000);
        let Cpu::M68k(c) = &mut c else { panic!() };
        c.dar[0] = selector;
        assert!(Abi::trap(0xa800, c, &m).is_err());
        assert_eq!(c.dar[15], 0x1000);
    }
    let m = Memory::default();
    let mut c = Cpu::new(Architecture::M68k, 0, 0x1000);
    let Cpu::M68k(c) = &mut c else { panic!() };
    for selector in [0x060c0018, 0x01100008, 0x04140008] {
        c.dar[0] = selector;
        assert!(Abi::trap(0xa800, c, &m).is_err());
        assert_eq!(c.dar[15], 0x1000);
    }
}

#[test]
fn extended_resource_bounds_use_frame_count_channel_count_and_sample_width() {
    for arch in [Architecture::M68k, Architecture::PowerPc] {
        let mut a = app();
        let mut raw = vec![
            0, 1, 0, 1, 0, 5, 0, 0, 0, 0xc4, 0, 1, 0x80, 0x51, 0, 0, 0, 0, 0, 20,
        ];
        let mut h = vec![0; 64];
        h[4..8].copy_from_slice(&2u32.to_be_bytes());
        h[8..12].copy_from_slice(&(8000u32 << 16).to_be_bytes());
        h[20] = 0xff;
        h[22..26].copy_from_slice(&2u32.to_be_bytes());
        h[48..50].copy_from_slice(&16u16.to_be_bytes());
        raw.extend(h);
        raw.extend([0; 4]);
        let resource = a
            .runtime
            .process
            .new_handle(&mut a.runtime.memory, raw.len() as u32, false)
            .unwrap();
        let data = a.runtime.memory.u32(GuestPtr(resource)).unwrap();
        a.runtime.memory.write(GuestPtr(data), &raw).unwrap();
        let free = a.runtime.process.free_memory();
        // A 2-frame, 2-channel, 16-bit payload needs eight bytes, not the four
        // supplied. The surrounding heap is writable, but not part of this handle.
        let stack = STACK;
        let mut args = vec![0; 2];
        args.extend(resource.to_be_bytes());
        args.extend(0u32.to_be_bytes());
        args.extend([0x5a; 2]);
        if arch == Architecture::M68k {
            a.runtime
                .memory
                .map(GuestPtr(0x52000000), vec![0xa8, 5], Permissions::RX)
                .unwrap();
            a.runtime.memory.write(GuestPtr(stack), &args).unwrap();
            a.runtime.cpu = Cpu::new(arch, 0x52000000, stack);
        } else {
            a.runtime
                .abi
                .imports
                .insert(0x52000000, systemless_future::abi::Service::SndPlay);
            a.runtime.cpu = Cpu::new(arch, 0x52000000, stack);
            let Cpu::PowerPc(c) = &mut a.runtime.cpu else {
                panic!()
            };
            c.gpr[3] = 0;
            c.gpr[4] = resource;
        }
        assert_eq!(a.runtime.advance(1).unwrap(), Stop::Budget);
        let result = match &a.runtime.cpu {
            Cpu::M68k(_) => a.runtime.memory.u16(GuestPtr(stack + 10)).unwrap() as i16,
            Cpu::PowerPc(c) => c.gpr[3] as i16,
        };
        assert_eq!(result, -206);
        assert_eq!(a.runtime.process.free_memory(), free);
        assert!(!a.runtime.process.sound.playing());
    }
}
#[test]
fn extended_silent_time_counts_frames_and_keeps_same_tail_as_pcm_rendering() {
    let mut apps = [app(), app()];
    let mut channels = Vec::new();
    for a in &mut apps {
        let ch = channel(a, 0xc4);
        channels.push(ch);
        let bytes: Vec<_> = (0..400)
            .flat_map(|i| [i as i16, -(i as i16)])
            .flat_map(i16::to_be_bytes)
            .collect();
        let h = extended(a, 2, 16, &bytes, true);
        assert_eq!(start(a, Architecture::PowerPc, ch, h), 0);
    }
    apps[0].runtime.elapse_sound_silently(1).unwrap();
    apps[1].runtime.render_sound(800, 48000).unwrap();
    assert_eq!(
        apps[0].runtime.render_sound(2000, 48000).unwrap(),
        apps[1].runtime.render_sound(2000, 48000).unwrap()
    );
    for (a, ch) in apps.iter().zip(channels) {
        assert_eq!(a.runtime.process.sound.busy(ch), Some(false));
    }
}
