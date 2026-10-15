mod support;
use systemless_future::{
    abi::Abi,
    cpu::{Architecture, Cpu},
    load::{LoadedApplication, STACK_BASE},
    memory::{GuestPtr, Memory, Permissions},
    runtime::Stop,
};
const BASE: u32 = 0x50000000;
fn app() -> LoadedApplication {
    let f = support::pef_program(
        &[0x4e800020],
        &[b"SndNewChannel", b"SndDoImmediate", b"SndDisposeChannel"],
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
            vec![0xa8, 7, 0xa8, 4, 0xa8, 1],
            Permissions::RX,
        )
        .unwrap();
    a
}
fn invoke(a: &mut LoadedApplication, arch: Architecture, index: u32, args: &[u32]) -> i16 {
    let stack = STACK_BASE + 0x8000;
    a.runtime
        .memory
        .write(GuestPtr(0x220), &0xbeefu16.to_be_bytes())
        .unwrap();
    a.runtime
        .memory
        .write(GuestPtr(0xa60), &0xcafeu16.to_be_bytes())
        .unwrap();
    a.runtime.process.mem_error = -108;
    a.runtime.process.res_error = -192;
    let count = if arch == Architecture::M68k {
        let mut bytes = Vec::new();
        match index {
            0 => {
                bytes.extend(args[3].to_be_bytes());
                bytes.extend(args[2].to_be_bytes());
                bytes.extend((args[1] as u16).to_be_bytes());
                bytes.extend(args[0].to_be_bytes());
            }
            1 => {
                bytes.extend(args[1].to_be_bytes());
                bytes.extend(args[0].to_be_bytes());
            }
            2 => {
                bytes.extend([args[1] as u8, 0x94]);
                bytes.extend(args[0].to_be_bytes());
            }
            _ => panic!(),
        }
        let count = bytes.len() as u32;
        bytes.extend([0x5a; 4]);
        a.runtime.memory.write(GuestPtr(stack), &bytes).unwrap();
        a.runtime.cpu = Cpu::new(arch, 0x51000000 + index * 2, stack);
        if let Cpu::M68k(c) = &mut a.runtime.cpu {
            c.dar[0] = 0x12345678;
        }
        count
    } else {
        let tv = a
            .runtime
            .memory
            .u32(GuestPtr(a.info.toc_or_a5 + 8 + index * 4))
            .unwrap();
        let pc = a.runtime.memory.u32(GuestPtr(tv)).unwrap();
        a.runtime.cpu = Cpu::new(arch, pc, stack);
        let Cpu::PowerPc(c) = &mut a.runtime.cpu else {
            panic!()
        };
        for (i, arg) in args.iter().enumerate() {
            c.gpr[3 + i] = *arg;
        }
        0
    };
    assert_eq!(a.runtime.advance(1).unwrap(), Stop::Budget);
    let result = match &a.runtime.cpu {
        Cpu::M68k(c) => {
            assert_eq!(c.dar[15], stack + count);
            assert_eq!(c.dar[0], 0x12345678);
            assert_eq!(
                a.runtime.memory.u16(GuestPtr(stack + count + 2)).unwrap(),
                0x5a5a
            );
            a.runtime.memory.u16(GuestPtr(stack + count)).unwrap() as i16
        }
        Cpu::PowerPc(c) => c.gpr[3] as i16,
    };
    assert_eq!(a.runtime.memory.u16(GuestPtr(0x220)).unwrap(), 0xbeef);
    assert_eq!(a.runtime.memory.u16(GuestPtr(0xa60)).unwrap(), 0xcafe);
    assert_eq!(a.runtime.process.mem_error, -108);
    assert_eq!(a.runtime.process.res_error, -192);
    result
}
fn channel(a: &mut LoadedApplication, arch: Architecture, init: u32) -> u32 {
    a.runtime
        .memory
        .write(GuestPtr(BASE), &0u32.to_be_bytes())
        .unwrap();
    assert_eq!(invoke(a, arch, 0, &[BASE, 5, init, 0]), 0);
    a.runtime.memory.u32(GuestPtr(BASE)).unwrap()
}
fn command(
    a: &mut LoadedApplication,
    arch: Architecture,
    ch: u32,
    code: u16,
    param: u16,
    value: u32,
) -> i16 {
    let mut bytes = code.to_be_bytes().to_vec();
    bytes.extend(param.to_be_bytes());
    bytes.extend(value.to_be_bytes());
    a.runtime.memory.write(GuestPtr(BASE + 16), &bytes).unwrap();
    invoke(a, arch, 1, &[ch, BASE + 16])
}
fn header(a: &mut LoadedApplication, rate: u32, samples: &[u8], external: bool) -> u32 {
    let address = BASE + 64;
    let mut bytes = if external {
        (BASE + 1024).to_be_bytes().to_vec()
    } else {
        0u32.to_be_bytes().to_vec()
    };
    bytes.extend((samples.len() as u32).to_be_bytes());
    bytes.extend((rate << 16).to_be_bytes());
    bytes.extend([0; 8]);
    bytes.extend([0, 60]);
    if external {
        a.runtime
            .memory
            .write(GuestPtr(BASE + 1024), samples)
            .unwrap();
    } else {
        bytes.extend(samples);
    }
    a.runtime.memory.write(GuestPtr(address), &bytes).unwrap();
    address
}
#[test]
fn both_cpus_play_live_buffers_apply_gain_quiet_and_preserve_channel_lifetime() {
    for arch in [Architecture::M68k, Architecture::PowerPc] {
        let mut a = app();
        let ch = channel(&mut a, arch, 4);
        let h = header(&mut a, 8000, &[0, 128, 255, 64], true);
        assert_eq!(command(&mut a, arch, ch, 81, 0, h), 0);
        assert_eq!(a.runtime.process.sound.busy(ch), Some(true));
        assert_eq!(
            a.runtime.render_sound(2, 8000).unwrap(),
            [[-32768; 2], [0; 2]]
        );
        a.runtime
            .memory
            .write(GuestPtr(BASE + 1026), &[192, 255])
            .unwrap();
        assert_eq!(
            a.runtime.render_sound(3, 8000).unwrap(),
            [[16384; 2], [32512; 2], [0; 2]]
        );
        assert_eq!(a.runtime.process.sound.busy(ch), Some(false));
        assert_eq!(a.runtime.memory.read(GuestPtr(ch + 20), 8).unwrap(), [0; 8]);
        assert_eq!(command(&mut a, arch, ch, 47, 0, BASE + 512), 0);
        assert_eq!(
            a.runtime.memory.u32(GuestPtr(BASE + 512)).unwrap(),
            0x01000100
        );
        assert_eq!(command(&mut a, arch, ch, 43, 128, 0), 0);
        assert_eq!(command(&mut a, arch, ch, 45, 0, BASE + 512), 0);
        assert_eq!(a.runtime.memory.u16(GuestPtr(BASE + 512)).unwrap(), 128);
        assert_eq!(command(&mut a, arch, ch, 46, 0, 0x00800100), 0);
        let h = header(&mut a, 8000, &[0, 255, 0, 255], false);
        assert_eq!(command(&mut a, arch, ch, 81, 0, h), 0);
        assert_eq!(a.runtime.render_sound(1, 8000).unwrap(), [[-16448, -8224]]);
        let available = a.runtime.process.free_memory();
        assert_eq!(invoke(&mut a, arch, 2, &[ch, 0]), -209);
        assert_eq!(a.runtime.process.free_memory(), available);
        assert_eq!(command(&mut a, arch, ch, 3, 0, 0), 0);
        assert!(!a.runtime.process.sound.playing());
        assert_eq!(a.runtime.render_sound(4, 8000).unwrap(), [[0; 2]; 4]);
        assert_eq!(invoke(&mut a, arch, 2, &[ch, 1]), 0);
        assert_eq!(a.runtime.process.sound.busy(ch), None);
    }
}
#[test]
fn resampling_is_chunk_independent_and_no_interp_and_pan_are_observed() {
    for (init, expected) in [
        (0, vec![0, 4096, 8192, 12288, 16384, 20480, 24576, 24576]),
        (4, vec![0, 0, 8192, 8192, 16384, 16384, 24576, 24576]),
    ] {
        for split in [false, true] {
            let mut a = app();
            let ch = channel(&mut a, Architecture::PowerPc, init);
            let h = header(&mut a, 8000, &[128, 160, 192, 224], false);
            assert_eq!(command(&mut a, Architecture::PowerPc, ch, 81, 0, h), 0);
            let out = if split {
                let mut out = a.runtime.render_sound(3, 16000).unwrap();
                out.extend(a.runtime.render_sound(5, 16000).unwrap());
                out
            } else {
                a.runtime.render_sound(8, 16000).unwrap()
            };
            assert_eq!(out, expected.iter().map(|&s| [s; 2]).collect::<Vec<_>>());
        }
    }
    let mut a = app();
    let ch = channel(&mut a, Architecture::M68k, 2);
    let h = header(&mut a, 8000, &[192], false);
    assert_eq!(command(&mut a, Architecture::M68k, ch, 81, 0, h), 0);
    assert_eq!(a.runtime.render_sound(1, 8000).unwrap(), [[16384, 0]]);
}
#[test]
fn bad_headers_and_render_boundaries_do_not_advance_or_publish_partial_playback() {
    let mut a = app();
    let ch = channel(&mut a, Architecture::PowerPc, 4);
    let h = header(&mut a, 8000, &[192, 224], false);
    let before = a.runtime.memory.read(GuestPtr(ch), 36).unwrap().to_vec();
    for (offset, bytes) in [
        (20, vec![0xff]),
        (8, vec![0; 4]),
        (4, u32::MAX.to_be_bytes().to_vec()),
        (21, vec![0]),
    ] {
        let saved = a
            .runtime
            .memory
            .read(GuestPtr(h + offset), bytes.len())
            .unwrap()
            .to_vec();
        a.runtime
            .memory
            .write(GuestPtr(h + offset), &bytes)
            .unwrap();
        assert_eq!(command(&mut a, Architecture::PowerPc, ch, 81, 0, h), -206);
        assert_eq!(a.runtime.memory.read(GuestPtr(ch), 36).unwrap(), before);
        a.runtime
            .memory
            .write(GuestPtr(h + offset), &saved)
            .unwrap();
    }
    assert_eq!(command(&mut a, Architecture::PowerPc, ch, 81, 0, h), 0);
    assert_eq!(command(&mut a, Architecture::PowerPc, ch, 81, 0, h), -209);
    assert!(a.runtime.render_sound(65537, 8000).is_err());
    assert!(a.runtime.render_sound(1, 0).is_err());
    let second = channel(&mut a, Architecture::PowerPc, 4);
    a.runtime
        .memory
        .write(GuestPtr(second + 32), &1u16.to_be_bytes())
        .unwrap();
    assert!(a.runtime.render_sound(1, 8000).is_err());
    a.runtime
        .memory
        .write(GuestPtr(second + 32), &0u16.to_be_bytes())
        .unwrap();
    assert_eq!(a.runtime.render_sound(1, 8000).unwrap(), [[16384; 2]]);
}
#[test]
fn immediate_native_frames_validate_result_storage_before_popping() {
    for length in [7, 8, 9] {
        let mut m = Memory::default();
        m.map(GuestPtr(0x1000), vec![0; length], Permissions::RW)
            .unwrap();
        let Cpu::M68k(mut c) = Cpu::new(Architecture::M68k, 0, 0x1000) else {
            panic!()
        };
        assert!(Abi::trap(0xa804, &mut c, &m).is_err());
        assert_eq!(c.dar[15], 0x1000);
    }
    let mut m = Memory::default();
    m.map(GuestPtr(0x1000), vec![0; 10], Permissions::RX)
        .unwrap();
    let Cpu::M68k(mut c) = Cpu::new(Architecture::M68k, 0, 0x1000) else {
        panic!()
    };
    assert!(Abi::trap(0xa804, &mut c, &m).is_err());
    assert_eq!(c.dar[15], 0x1000);
}

#[test]
fn exit_and_cpu_fault_cancel_owned_playback_and_prevent_later_rendering() {
    for opcode in [0xa9f4u16, 0x4afc] {
        let mut a = app();
        let ch = channel(&mut a, Architecture::M68k, 4);
        let h = header(&mut a, 8000, &[192; 32], false);
        assert_eq!(command(&mut a, Architecture::M68k, ch, 81, 0, h), 0);
        a.runtime
            .memory
            .map(
                GuestPtr(0x52000000),
                opcode.to_be_bytes().to_vec(),
                Permissions::RX,
            )
            .unwrap();
        a.runtime.cpu = Cpu::new(Architecture::M68k, 0x52000000, STACK_BASE + 0x8000);
        let result = a.runtime.advance(1000);
        if opcode == 0xa9f4 {
            assert_eq!(result.unwrap(), Stop::Exit(0));
        } else {
            assert!(result.is_err());
        }
        assert!(!a.runtime.process.sound.playing());
        assert!(a.runtime.render_sound(1, 8000).is_err());
    }
}

#[test]
fn headless_sound_time_is_chunk_independent_and_large_waits_remain_bounded() {
    for split in [false, true] {
        let mut a = app();
        let ch = channel(&mut a, Architecture::PowerPc, 4);
        let data: Vec<u8> = (0..800).map(|index| index as u8).collect();
        let h = header(&mut a, 8000, &data, false);
        assert_eq!(command(&mut a, Architecture::PowerPc, ch, 81, 0, h), 0);
        if split {
            for _ in 0..3 {
                a.runtime.elapse_sound_silently(1).unwrap();
            }
        } else {
            a.runtime.elapse_sound_silently(3).unwrap();
        }
        assert_eq!(a.runtime.render_sound(1, 8000).unwrap(), [[4096; 2]]);
        assert_eq!(a.runtime.process.sound.busy(ch), Some(true));
        a.runtime.elapse_sound_silently(u32::MAX).unwrap();
        assert_eq!(a.runtime.process.sound.busy(ch), Some(false));
        assert_eq!(a.runtime.memory.read(GuestPtr(ch + 20), 8).unwrap(), [0; 8]);
    }
}

#[test]
fn multiple_channels_mix_saturate_and_observe_the_live_master_volume() {
    let mut a = app();
    let first = channel(&mut a, Architecture::PowerPc, 4);
    let second = channel(&mut a, Architecture::M68k, 4);
    let h = header(&mut a, 8000, &[192; 4], false);
    assert_eq!(command(&mut a, Architecture::PowerPc, first, 81, 0, h), 0);
    assert_eq!(command(&mut a, Architecture::M68k, second, 81, 0, h), 0);
    assert_eq!(a.runtime.render_sound(1, 8000).unwrap(), [[32767; 2]]);
    assert_eq!(
        command(&mut a, Architecture::PowerPc, first, 46, 0, 0x02000200),
        0
    );
    assert_eq!(a.runtime.render_sound(1, 8000).unwrap(), [[32767; 2]]);
    a.runtime.memory.write(GuestPtr(0x260), &[0xa8]).unwrap();
    assert_eq!(a.runtime.render_sound(1, 8000).unwrap(), [[0; 2]]);
    systemless_future::sound::set_volume(&mut a.runtime.memory, 7).unwrap();
    assert_eq!(a.runtime.memory.read(GuestPtr(0x260), 1).unwrap(), [0xaf]);
    assert_eq!(
        a.runtime.render_sound(2, 8000).unwrap(),
        [[32767; 2], [0; 2]]
    );
    assert!(!a.runtime.process.sound.playing());
}

#[test]
fn fractional_resampling_carries_division_remainders_at_exact_sample_boundaries() {
    let mut a = app();
    let ch = channel(&mut a, Architecture::PowerPc, 4);
    let h = header(&mut a, 8000, &[0, 128, 255, 64], false);
    assert_eq!(command(&mut a, Architecture::PowerPc, ch, 81, 0, h), 0);
    let mut output = Vec::new();
    for count in [5, 1, 7, 3, 8] {
        output.extend(a.runtime.render_sound(count, 48000).unwrap());
    }
    let expected: Vec<_> = [-32768, 0, 32512, -16384]
        .into_iter()
        .flat_map(|sample| std::iter::repeat_n([sample; 2], 6))
        .collect();
    assert_eq!(output, expected);
    assert_eq!(a.runtime.process.sound.busy(ch), Some(false));
}
