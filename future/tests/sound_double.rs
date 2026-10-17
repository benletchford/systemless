mod support;
use systemless_future::{
    cpu::{Architecture, Cpu},
    load::{LoadedApplication, STACK_BASE},
    memory::{GuestPtr, Permissions},
    runtime::Stop,
    sound::{NewChannel, Op},
};
const BASE: u32 = 0x50000000;
const HEADER: u32 = BASE + 64;
const BUFFERS: [u32; 2] = [BASE + 128, BASE + 256];
const STACK: u32 = STACK_BASE + 0x8000;
fn setup(rate: u32, last: bool) -> (LoadedApplication, u32) {
    setup_with_init(rate, last, 0xc4)
}
fn setup_with_init(rate: u32, last: bool, init: u32) -> (LoadedApplication, u32) {
    let f = support::pef_program(&[0x4e800020], &[b"SndPlayDoubleBuffer"], 0);
    let mut a = LoadedApplication::pef(&f.bytes, None).unwrap();
    a.runtime
        .memory
        .map(GuestPtr(BASE), vec![0; 4096], Permissions::RW)
        .unwrap();
    a.runtime
        .memory
        .map(
            GuestPtr(0x51000000),
            vec![0xa8, 0, 0x60, 0xfe],
            Permissions::RX,
        )
        .unwrap();
    a.runtime.memory.write(GuestPtr(0x260), &[7]).unwrap();
    let mut sound = std::mem::take(&mut a.runtime.process.sound);
    assert_eq!(
        sound
            .call(
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
    a.runtime.process.sound = sound;
    let channel = a.runtime.memory.u32(GuestPtr(BASE)).unwrap();
    let mut h = Vec::new();
    for v in [1u16, 8, 0, 0] {
        h.extend(v.to_be_bytes());
    }
    h.extend((rate << 16).to_be_bytes());
    for v in BUFFERS {
        h.extend(v.to_be_bytes());
    }
    h.extend(0u32.to_be_bytes());
    a.runtime.memory.write(GuestPtr(HEADER), &h).unwrap();
    for (i, pointer) in BUFFERS.into_iter().enumerate() {
        let mut b = 2u32.to_be_bytes().to_vec();
        b.extend((if i == 1 && last { 5u32 } else { 1 }).to_be_bytes());
        b.extend([0x5a; 8]);
        b.extend([128 + i as u8 * 2, 129 + i as u8 * 2]);
        a.runtime.memory.write(GuestPtr(pointer), &b).unwrap();
    }
    (a, channel)
}
fn submit(
    a: &mut LoadedApplication,
    ch: u32,
    arch: Architecture,
) -> systemless_future::Result<i16> {
    a.runtime.process.mem_error = -108;
    a.runtime.process.res_error = -192;
    if arch == Architecture::M68k {
        let mut frame = HEADER.to_be_bytes().to_vec();
        frame.extend(ch.to_be_bytes());
        frame.extend([0x5a; 4]);
        a.runtime.memory.write(GuestPtr(STACK), &frame).unwrap();
        a.runtime.cpu = Cpu::new(arch, 0x51000000, STACK);
        let Cpu::M68k(c) = &mut a.runtime.cpu else {
            panic!()
        };
        c.dar[0] = 0x04200008;
    } else {
        let tv = a
            .runtime
            .memory
            .u32(GuestPtr(a.info.toc_or_a5 + 8))
            .unwrap();
        let pc = a.runtime.memory.u32(GuestPtr(tv)).unwrap();
        a.runtime.cpu = Cpu::new(arch, pc, STACK);
        let Cpu::PowerPc(c) = &mut a.runtime.cpu else {
            panic!()
        };
        c.gpr[3] = ch;
        c.gpr[4] = HEADER;
    }
    assert_eq!(a.runtime.advance(1)?, Stop::Budget);
    assert_eq!(
        (a.runtime.process.mem_error, a.runtime.process.res_error),
        (-108, -192)
    );
    Ok(match &a.runtime.cpu {
        Cpu::M68k(c) => {
            assert_eq!(c.dar[15], STACK + 8);
            assert_eq!(c.dar[0], 0x04200008);
            assert_eq!(a.runtime.memory.u16(GuestPtr(STACK + 10)).unwrap(), 0x5a5a);
            a.runtime.memory.u16(GuestPtr(STACK + 8)).unwrap() as i16
        }
        Cpu::PowerPc(c) => {
            assert_eq!(c.gpr[4], HEADER);
            c.gpr[3] as i16
        }
    })
}
#[test]
fn both_adapters_consume_live_double_buffers_clear_ready_and_stop_on_last() {
    for arch in [Architecture::M68k, Architecture::PowerPc] {
        let (mut a, ch) = setup(8000, true);
        assert_eq!(submit(&mut a, ch, arch).unwrap(), 0);
        assert_eq!(a.runtime.render_sound(2, 8000).unwrap(), [[0; 2], [256; 2]]);
        assert_eq!(a.runtime.memory.u32(GuestPtr(BUFFERS[0] + 4)).unwrap(), 0);
        a.runtime
            .memory
            .write(GuestPtr(BUFFERS[1] + 16), &[132, 133])
            .unwrap();
        assert_eq!(
            a.runtime.render_sound(3, 8000).unwrap(),
            [[1024; 2], [1280; 2], [0; 2]]
        );
        assert_eq!(a.runtime.memory.u32(GuestPtr(BUFFERS[1] + 4)).unwrap(), 4);
        assert_eq!(a.runtime.process.sound.busy(ch), Some(false));
        for p in BUFFERS {
            assert_eq!(
                a.runtime.memory.read(GuestPtr(p + 8), 8).unwrap(),
                [0x5a; 8]
            );
        }
    }
}
#[test]
fn fractional_phase_survives_buffer_boundaries_and_silent_output_finishes() {
    let (mut a, ch) = setup(12000, true);
    assert_eq!(submit(&mut a, ch, Architecture::PowerPc).unwrap(), 0);
    assert_eq!(
        a.runtime.render_sound(4, 8000).unwrap(),
        [[0; 2], [256; 2], [768; 2], [0; 2]]
    );
    assert_eq!(a.runtime.process.sound.busy(ch), Some(false));
    let (mut a, ch) = setup(8000, true);
    assert_eq!(submit(&mut a, ch, Architecture::M68k).unwrap(), 0);
    a.runtime.elapse_sound_silently(1).unwrap();
    assert_eq!(a.runtime.process.sound.busy(ch), Some(false));
    assert_eq!(a.runtime.memory.u32(GuestPtr(BUFFERS[0] + 4)).unwrap(), 0);
    assert_eq!(a.runtime.memory.u32(GuestPtr(BUFFERS[1] + 4)).unwrap(), 4);
}
#[test]
fn exhausted_buffers_wait_for_guest_refill_and_quiet_cancels_ownership() {
    let (mut a, ch) = setup(8000, false);
    assert_eq!(submit(&mut a, ch, Architecture::PowerPc).unwrap(), 0);
    assert_eq!(
        a.runtime.render_sound(6, 8000).unwrap(),
        [[0; 2], [256; 2], [512; 2], [768; 2], [0; 2], [0; 2]]
    );
    assert_eq!(a.runtime.process.sound.busy(ch), Some(true));
    a.runtime
        .memory
        .write(GuestPtr(BUFFERS[0] + 4), &5u32.to_be_bytes())
        .unwrap();
    assert_eq!(a.runtime.render_sound(2, 8000).unwrap(), [[0; 2], [256; 2]]);
    assert_eq!(a.runtime.process.sound.busy(ch), Some(false));
    // Refill both, begin again, and cancel through the real immediate quiet command.
    for p in BUFFERS {
        a.runtime
            .memory
            .write(GuestPtr(p + 4), &1u32.to_be_bytes())
            .unwrap();
    }
    assert_eq!(submit(&mut a, ch, Architecture::PowerPc).unwrap(), 0);
    a.runtime
        .memory
        .write(GuestPtr(BASE + 32), &[0, 3, 0, 0, 0, 0, 0, 0])
        .unwrap();
    let mut sound = std::mem::take(&mut a.runtime.process.sound);
    assert_eq!(
        sound
            .immediate(
                &mut a.runtime.memory,
                &mut a.runtime.process,
                ch,
                GuestPtr(BASE + 32)
            )
            .unwrap(),
        0
    );
    a.runtime.process.sound = sound;
    assert_eq!(a.runtime.process.sound.busy(ch), Some(false));
    assert_eq!(a.runtime.render_sound(2, 8000).unwrap(), [[0; 2]; 2]);
}
#[test]
fn invalid_second_buffer_is_rejected_before_channel_or_flags_change() {
    for arch in [Architecture::M68k, Architecture::PowerPc] {
        let (mut a, ch) = setup(8000, true);
        let before = a.runtime.memory.read(GuestPtr(ch), 36).unwrap().to_vec();
        a.runtime
            .memory
            .write(GuestPtr(HEADER + 16), &0xfffffff0u32.to_be_bytes())
            .unwrap();
        assert!(submit(&mut a, ch, arch).is_err());
        assert_eq!(a.runtime.memory.read(GuestPtr(ch), 36).unwrap(), before);
        assert_eq!(a.runtime.memory.u32(GuestPtr(BUFFERS[0] + 4)).unwrap(), 1);
        assert_eq!(a.runtime.process.sound.busy(ch), Some(false));
    }
}
#[test]
fn doubleback_callbacks_receive_exhausted_buffer_and_channel_on_both_cpus() {
    for arch in [Architecture::M68k, Architecture::PowerPc] {
        let (mut a, ch) = setup(8000, false);
        let routine = if arch == Architecture::M68k {
            // exhaustedBuffer=4(sp), channel=8(sp); record buffer, refill ready,
            // deliberately clobber a preserved register, then Pascal-return.
            let words = [
                0x206fu16, 4, 0x226f, 8, 0x2348, 12, 0x217c, 0, 1, 4, 0x7e55, 0x225f, 0x508f,
                0x4ed1,
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
            let words = [
                0x9083000cu32,
                0x38a00001,
                0x90a40004,
                0x39a00055,
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
        a.runtime
            .memory
            .write(GuestPtr(HEADER + 20), &routine.to_be_bytes())
            .unwrap();
        assert_eq!(submit(&mut a, ch, arch).unwrap(), 0);
        assert_eq!(
            a.runtime.render_sound(4, 8000).unwrap(),
            [[0; 2], [256; 2], [512; 2], [768; 2]]
        );
        assert_eq!(a.runtime.services.completed_sound_callbacks, 0);
        for p in BUFFERS {
            assert_eq!(a.runtime.memory.u32(GuestPtr(p + 4)).unwrap(), 0);
        }
        let code = if arch == Architecture::M68k {
            vec![0x60, 0xfe]
        } else {
            0x48000000u32.to_be_bytes().to_vec()
        };
        a.runtime
            .memory
            .map(GuestPtr(0x53000000), code, Permissions::RX)
            .unwrap();
        a.runtime.cpu = Cpu::new(arch, 0x53000000, STACK);
        match &mut a.runtime.cpu {
            Cpu::M68k(c) => {
                c.dar[7] = 0xfeedbeef;
                c.dar[13] = 0x12345678;
            }
            Cpu::PowerPc(c) => c.gpr[13] = 0xfeedbeef,
        }
        assert_eq!(a.runtime.advance(64).unwrap(), Stop::Budget);
        assert_eq!(a.runtime.services.completed_sound_callbacks, 2);
        assert_eq!(a.runtime.memory.u32(GuestPtr(ch + 12)).unwrap(), BUFFERS[1]);
        for p in BUFFERS {
            assert_eq!(a.runtime.memory.u32(GuestPtr(p + 4)).unwrap(), 1);
        }
        assert_eq!(a.runtime.cpu.pc(), 0x53000000);
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
        assert_eq!(a.runtime.render_sound(1, 8000).unwrap(), [[0; 2]]);
    }
}

#[test]
fn stereo_sixteen_bit_buffers_keep_channel_order_and_big_endian_samples() {
    for arch in [Architecture::M68k, Architecture::PowerPc] {
        let (mut a, ch) = setup(8000, true);
        a.runtime
            .memory
            .write(GuestPtr(HEADER), &[0, 2, 0, 16])
            .unwrap();
        for (pointer, samples) in BUFFERS
            .into_iter()
            .zip([[1000i16, -1000, 2000, -2000], [3000, -3000, 4000, -4000]])
        {
            let bytes: Vec<_> = samples.into_iter().flat_map(i16::to_be_bytes).collect();
            a.runtime
                .memory
                .write(GuestPtr(pointer + 16), &bytes)
                .unwrap();
        }
        assert_eq!(submit(&mut a, ch, arch).unwrap(), 0);
        assert_eq!(
            a.runtime.render_sound(5, 8000).unwrap(),
            [
                [1000, -1000],
                [2000, -2000],
                [3000, -3000],
                [4000, -4000],
                [0, 0]
            ]
        );
        assert_eq!(a.runtime.process.sound.busy(ch), Some(false));
    }
}
#[test]
fn output_faults_in_either_buffer_do_not_publish_a_double_buffer_submission() {
    for arch in [Architecture::M68k, Architecture::PowerPc] {
        let (mut a, ch) = setup(8000, true);
        let mut bytes = 2u32.to_be_bytes().to_vec();
        bytes.extend(5u32.to_be_bytes());
        bytes.extend([0; 8]);
        bytes.extend([128, 129]);
        a.runtime
            .memory
            .map(GuestPtr(0x54000000), bytes, Permissions::RX)
            .unwrap();
        a.runtime
            .memory
            .write(GuestPtr(HEADER + 16), &0x54000000u32.to_be_bytes())
            .unwrap();
        let before = a.runtime.memory.read(GuestPtr(ch), 36).unwrap().to_vec();
        assert!(submit(&mut a, ch, arch).is_err());
        assert_eq!(a.runtime.memory.read(GuestPtr(ch), 36).unwrap(), before);
        assert_eq!(a.runtime.memory.u32(GuestPtr(BUFFERS[0] + 4)).unwrap(), 1);
        assert_eq!(a.runtime.process.sound.busy(ch), Some(false));
    }
}
#[test]
fn silent_quantum_splits_preserve_fractional_samples_and_ready_flags() {
    let (mut a, ch) = setup(11025, true);
    let (mut b, other) = setup(11025, true);
    for app in [&mut a, &mut b] {
        for i in 0..2 {
            let mut bytes = 400u32.to_be_bytes().to_vec();
            bytes.extend((if i == 1 { 5u32 } else { 1 }).to_be_bytes());
            bytes.extend([0; 8]);
            bytes.extend(vec![192; 400]);
            // Buffers need separate nonoverlapping extents for this larger workload.
            let address = BASE + 1024 + i as u32 * 512;
            app.runtime.memory.write(GuestPtr(address), &bytes).unwrap();
            app.runtime
                .memory
                .write(GuestPtr(HEADER + 12 + i as u32 * 4), &address.to_be_bytes())
                .unwrap();
        }
    }
    assert_eq!(submit(&mut a, ch, Architecture::M68k).unwrap(), 0);
    assert_eq!(submit(&mut b, other, Architecture::PowerPc).unwrap(), 0);
    a.runtime.elapse_sound_silently(3).unwrap();
    for _ in 0..3 {
        b.runtime.elapse_sound_silently(1).unwrap();
    }
    assert_eq!(
        a.runtime.render_sound(400, 48000).unwrap(),
        b.runtime.render_sound(400, 48000).unwrap()
    );
    for i in 0..2 {
        let p = GuestPtr(BASE + 1028 + i * 512);
        assert_eq!(
            a.runtime.memory.u32(p).unwrap(),
            b.runtime.memory.u32(p).unwrap()
        );
    }
}

#[test]
fn interpolation_uses_the_next_ready_buffer_at_a_continuous_boundary() {
    let (mut a, ch) = setup_with_init(8000, true, 0xc0);
    assert_eq!(submit(&mut a, ch, Architecture::M68k).unwrap(), 0);
    assert_eq!(
        a.runtime.render_sound(9, 16000).unwrap(),
        [[0; 2], [128; 2], [256; 2], [384; 2], [512; 2], [640; 2], [768; 2], [768; 2], [0; 2]]
    );
    assert_eq!(a.runtime.process.sound.busy(ch), Some(false));
}
