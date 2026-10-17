mod support;
use systemless_future::{
    cpu::{Architecture, Cpu},
    load::{LoadedApplication, STACK_BASE},
    memory::{GuestPtr, Memory, Permissions},
    runtime::Stop,
    sound::{self, Manager, NewChannel, Op},
};
const BASE: u32 = 0x50000000;
const STACK: u32 = STACK_BASE + 0x8000;
fn app() -> LoadedApplication {
    let f = support::pef_program(
        &[0x4e800020],
        &[b"GetDefaultOutputVolume", b"SetDefaultOutputVolume"],
        0,
    );
    let mut a = LoadedApplication::pef(&f.bytes, None).unwrap();
    a.runtime
        .memory
        .map(GuestPtr(BASE), vec![0xa5; 4096], Permissions::RW)
        .unwrap();
    a.runtime
        .memory
        .map(GuestPtr(0x51000000), vec![0xa8, 0], Permissions::RX)
        .unwrap();
    a.runtime.memory.write(GuestPtr(0x260), &[0xff]).unwrap();
    a
}
fn invoke(
    a: &mut LoadedApplication,
    arch: Architecture,
    get: bool,
    value: u32,
) -> systemless_future::Result<i16> {
    if arch == Architecture::M68k {
        let mut f = value.to_be_bytes().to_vec();
        f.extend([0x5a; 4]);
        a.runtime.memory.write(GuestPtr(STACK), &f).unwrap();
        a.runtime.cpu = Cpu::new(arch, 0x51000000, STACK);
        let Cpu::M68k(c) = &mut a.runtime.cpu else {
            panic!()
        };
        c.dar[0] = if get { 0x022c0018 } else { 0x02300018 };
    } else {
        let tv = a
            .runtime
            .memory
            .u32(GuestPtr(a.info.toc_or_a5 + 8 + if get { 0 } else { 4 }))
            .unwrap();
        let pc = a.runtime.memory.u32(GuestPtr(tv)).unwrap();
        a.runtime.cpu = Cpu::new(arch, pc, STACK);
        let Cpu::PowerPc(c) = &mut a.runtime.cpu else {
            panic!()
        };
        c.gpr[3] = value;
        c.gpr[4] = 0xfeedbeef;
    }
    a.runtime.process.mem_error = -108;
    a.runtime.process.res_error = -192;
    assert_eq!(a.runtime.advance(1)?, Stop::Budget);
    assert_eq!(
        (a.runtime.process.mem_error, a.runtime.process.res_error),
        (-108, -192)
    );
    Ok(match &a.runtime.cpu {
        Cpu::M68k(c) => {
            assert_eq!(c.dar[15], STACK + 4);
            assert_eq!(c.dar[0], if get { 0x022c0018 } else { 0x02300018 });
            assert_eq!(a.runtime.memory.u16(GuestPtr(STACK + 6)).unwrap(), 0x5a5a);
            a.runtime.memory.u16(GuestPtr(STACK + 4)).unwrap() as i16
        }
        Cpu::PowerPc(c) => {
            assert_eq!(c.gpr[4], 0xfeedbeef);
            c.gpr[3] as i16
        }
    })
}
fn read(a: &mut LoadedApplication, arch: Architecture) -> u32 {
    assert_eq!(invoke(a, arch, true, BASE).unwrap(), 0);
    a.runtime.memory.u32(GuestPtr(BASE)).unwrap()
}
fn voice(a: &mut LoadedApplication) -> u32 {
    a.runtime
        .memory
        .write(GuestPtr(BASE + 16), &[0; 4])
        .unwrap();
    let mut s = std::mem::take(&mut a.runtime.process.sound);
    assert_eq!(
        s.call(
            &mut a.runtime.memory,
            &mut a.runtime.process,
            Op::New(NewChannel {
                destination: GuestPtr(BASE + 16),
                synth: 5,
                init: 0xc4,
                callback: 0
            })
        )
        .unwrap(),
        0
    );
    let ch = a.runtime.memory.u32(GuestPtr(BASE + 16)).unwrap();
    let mut header = vec![0; 22];
    header[4..8].copy_from_slice(&4u32.to_be_bytes());
    header[8..12].copy_from_slice(&(8000u32 << 16).to_be_bytes());
    header.extend([192; 4]);
    a.runtime
        .memory
        .write(GuestPtr(BASE + 128), &header)
        .unwrap();
    let mut cmd = vec![0, 81, 0, 0];
    cmd.extend((BASE + 128).to_be_bytes());
    a.runtime.memory.write(GuestPtr(BASE + 64), &cmd).unwrap();
    assert_eq!(
        s.immediate(
            &mut a.runtime.memory,
            &mut a.runtime.process,
            ch,
            GuestPtr(BASE + 64)
        )
        .unwrap(),
        0
    );
    a.runtime.process.sound = s;
    ch
}
#[test]
fn both_cpu_queries_share_real_stereo_gain_that_changes_an_existing_voice() {
    for arch in [Architecture::M68k, Architecture::PowerPc] {
        let mut a = app();
        let ch = voice(&mut a);
        assert_eq!(read(&mut a, arch), 0x01000100);
        assert_eq!(invoke(&mut a, arch, false, 0x00800100).unwrap(), 0);
        let other = if arch == Architecture::M68k {
            Architecture::PowerPc
        } else {
            Architecture::M68k
        };
        assert_eq!(read(&mut a, other), 0x00800100);
        assert_eq!(a.runtime.render_sound(1, 8000).unwrap(), [[16384, 8192]]);
        assert_eq!(invoke(&mut a, other, false, 0x00000040).unwrap(), 0);
        assert_eq!(a.runtime.render_sound(1, 8000).unwrap(), [[4096, 0]]);
        assert_eq!(invoke(&mut a, arch, false, 0).unwrap(), 0);
        assert_eq!(a.runtime.render_sound(1, 8000).unwrap(), [[0, 0]]);
        assert_eq!(a.runtime.process.sound.busy(ch), Some(true));
        assert_eq!(invoke(&mut a, arch, false, 0x01000100).unwrap(), 0);
        assert_eq!(a.runtime.render_sound(1, 8000).unwrap(), [[16384; 2]]);
        assert_eq!(a.runtime.process.sound.busy(ch), Some(false));
    }
}
#[test]
fn legacy_master_attenuation_is_reported_and_new_device_level_overrides_prior_mute() {
    for arch in [Architecture::M68k, Architecture::PowerPc] {
        let mut a = app();
        sound::set_volume(&mut a.runtime.memory, 1).unwrap();
        assert_eq!(read(&mut a, arch), 0x00240024);
        sound::set_volume(&mut a.runtime.memory, 0).unwrap();
        assert_eq!(read(&mut a, arch), 0);
        assert_eq!(invoke(&mut a, arch, false, 0x00400080).unwrap(), 0);
        assert_eq!(read(&mut a, arch), 0x00400080);
        assert_eq!(sound::volume(&a.runtime.memory).unwrap(), 7);
        assert_eq!(a.runtime.memory.read(GuestPtr(0x260), 1).unwrap(), [0xff]);
        sound::set_volume(&mut a.runtime.memory, 3).unwrap();
        assert_eq!(read(&mut a, arch), 0x001b0036);
    }
}
#[test]
fn invalid_levels_and_nil_outputs_return_errors_without_mutating_gain_or_legacy_state() {
    for arch in [Architecture::M68k, Architecture::PowerPc] {
        let mut a = app();
        assert_eq!(invoke(&mut a, arch, false, 0x00800040).unwrap(), 0);
        sound::set_volume(&mut a.runtime.memory, 2).unwrap();
        let expected = read(&mut a, arch);
        for value in [0x01010000, 0x00000101, u32::MAX] {
            assert_eq!(invoke(&mut a, arch, false, value).unwrap(), -50);
            assert_eq!(read(&mut a, arch), expected);
            assert_eq!(sound::volume(&a.runtime.memory).unwrap(), 2);
        }
        assert_eq!(invoke(&mut a, arch, true, 0).unwrap(), -50);
        assert_eq!(read(&mut a, arch), expected);
    }
}
#[test]
fn read_only_output_faults_before_native_argument_pop_or_ppc_result_replacement() {
    for arch in [Architecture::M68k, Architecture::PowerPc] {
        let mut a = app();
        a.runtime
            .memory
            .map(GuestPtr(0x52000000), vec![0x5a; 4], Permissions::RX)
            .unwrap();
        assert!(invoke(&mut a, arch, true, 0x52000000).is_err());
        assert_eq!(
            a.runtime.memory.read(GuestPtr(0x52000000), 4).unwrap(),
            [0x5a; 4]
        );
        match &a.runtime.cpu {
            Cpu::M68k(c) => assert_eq!(c.dar[15], STACK),
            Cpu::PowerPc(c) => assert_eq!(c.gpr[3], 0x52000000),
        }
        // A guest memory fault is terminal; inspect retained state without
        // attempting to execute the stopped application again.
        assert_eq!(
            a.runtime
                .process
                .sound
                .get_output_volume(&mut a.runtime.memory, GuestPtr(BASE))
                .unwrap(),
            0
        );
        assert_eq!(a.runtime.memory.u32(GuestPtr(BASE)).unwrap(), 0x01000100);
    }
}
#[test]
fn setter_preflights_legacy_storage_before_publishing_new_gain() {
    let mut m = Memory::default();
    m.map(GuestPtr(0x260), vec![7], Permissions::RX).unwrap();
    m.map(GuestPtr(BASE), vec![0; 4], Permissions::RW).unwrap();
    let mut s = Manager::default();
    assert!(s.set_output_volume(&mut m, 0x00800040).is_err());
    assert_eq!(s.get_output_volume(&mut m, GuestPtr(BASE)).unwrap(), 0);
    assert_eq!(m.u32(GuestPtr(BASE)).unwrap(), 0x01000100);
}
#[test]
fn device_attenuation_applies_to_the_summed_signal_before_saturation() {
    let mut a = app();
    voice(&mut a);
    voice(&mut a);
    assert_eq!(
        invoke(&mut a, Architecture::M68k, false, 0x00800080).unwrap(),
        0
    );
    assert_eq!(a.runtime.render_sound(1, 8000).unwrap(), [[16384; 2]]);
    assert_eq!(
        invoke(&mut a, Architecture::PowerPc, false, 0x01000100).unwrap(),
        0
    );
    assert_eq!(a.runtime.render_sound(1, 8000).unwrap(), [[32767; 2]]);
}
