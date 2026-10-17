mod support;
use systemless_future::{
    abi::{Abi, Service},
    cpu::{Architecture, Cpu},
    events,
    host_input::{virtual_key, Keyboard},
    keyboard::{self, KEY_MAP},
    load::{LoadedApplication, STACK_BASE},
    memory::{GuestPtr, Memory, Permissions},
    runtime::Stop,
};
fn app() -> LoadedApplication {
    let f = support::pef_program(&[0x4e800020], &[b"GetKeys"], 12);
    let mut a = LoadedApplication::pef(&f.bytes, None).unwrap();
    a.runtime
        .memory
        .map(GuestPtr(0x50000000), vec![0xa9, 0x76], Permissions::RX)
        .unwrap();
    a.runtime
        .memory
        .map(GuestPtr(0x5000), vec![0xa5; 20], Permissions::RW)
        .unwrap();
    a.runtime
        .memory
        .map(GuestPtr(0x5200), vec![0; 16], Permissions::RW)
        .unwrap();
    a
}
fn query(a: &mut LoadedApplication, arch: Architecture) -> [u8; 16] {
    let sp = STACK_BASE + 0x7000;
    let destination = 0x5002u32;
    a.runtime
        .memory
        .write(GuestPtr(0x5000), &[0xa5; 20])
        .unwrap();
    let mut frame = destination.to_be_bytes().to_vec();
    frame.extend([0x5a; 4]);
    a.runtime.memory.write(GuestPtr(sp), &frame).unwrap();
    let pc = if arch == Architecture::M68k {
        0x50000000
    } else {
        let tv = a
            .runtime
            .memory
            .u32(GuestPtr(a.info.toc_or_a5 + 8))
            .unwrap();
        a.runtime.memory.u32(GuestPtr(tv)).unwrap()
    };
    a.runtime.cpu = Cpu::new(arch, pc, sp);
    match &mut a.runtime.cpu {
        Cpu::M68k(c) => {
            for i in 0..15 {
                c.dar[i] = 0x12340000 + i as u32;
            }
        }
        Cpu::PowerPc(c) => {
            for i in 2..32 {
                c.gpr[i] = 0x12340000 + i as u32;
            }
            c.gpr[3] = destination;
        }
    }
    let queue = a
        .runtime
        .memory
        .read(events::QUEUE_HEADER, 10)
        .unwrap()
        .to_vec();
    let free = a.runtime.process.free_memory();
    a.runtime.process.mem_error = -108;
    a.runtime.process.res_error = -192;
    assert_eq!(a.runtime.advance(1).unwrap(), Stop::Budget);
    assert_eq!(
        a.runtime.memory.read(events::QUEUE_HEADER, 10).unwrap(),
        queue
    );
    assert_eq!(a.runtime.process.free_memory(), free);
    assert_eq!(a.runtime.process.mem_error, -108);
    assert_eq!(a.runtime.process.res_error, -192);
    match &a.runtime.cpu {
        Cpu::M68k(c) => {
            for i in 0..15 {
                assert_eq!(c.dar[i], 0x12340000 + i as u32);
            }
            assert_eq!(c.dar[15], sp + 4);
        }
        Cpu::PowerPc(c) => {
            assert_eq!(c.gpr[1], sp);
            for i in 2..32 {
                assert_eq!(
                    c.gpr[i],
                    if i == 3 {
                        destination
                    } else {
                        0x12340000 + i as u32
                    }
                );
            }
        }
    }
    assert_eq!(
        a.runtime.memory.read(GuestPtr(sp + 4), 4).unwrap(),
        [0x5a; 4]
    );
    assert_eq!(
        a.runtime.memory.read(GuestPtr(0x5000), 2).unwrap(),
        [0xa5; 2]
    );
    assert_eq!(
        a.runtime.memory.read(GuestPtr(0x5012), 2).unwrap(),
        [0xa5; 2]
    );
    a.runtime
        .memory
        .read(GuestPtr(destination), 16)
        .unwrap()
        .try_into()
        .unwrap()
}
fn down(map: &[u8; 16], code: u8) -> bool {
    map[usize::from(code / 8)] & (1 << (code % 8)) != 0
}
fn key(
    a: &mut LoadedApplication,
    k: &mut Keyboard,
    code: u8,
    text: Option<&str>,
    pressed: bool,
    repeat: bool,
) {
    k.key(
        &mut a.runtime.memory,
        &mut a.runtime.process.input,
        code,
        text,
        pressed,
        repeat,
    )
    .unwrap();
}
fn event(a: &mut LoadedApplication) -> (u16, u32, u32, u16) {
    let input = a.runtime.process.input;
    assert!(events::receive(
        &mut a.runtime.memory,
        0xffff,
        GuestPtr(0x5200),
        true,
        0,
        input
    )
    .unwrap());
    let m = &a.runtime.memory;
    (
        m.u16(GuestPtr(0x5200)).unwrap(),
        m.u32(GuestPtr(0x5202)).unwrap(),
        m.u32(GuestPtr(0x5206)).unwrap(),
        m.u16(GuestPtr(0x520e)).unwrap(),
    )
}
#[test]
fn both_cpu_procedures_copy_all_sixteen_guest_bytes_without_endian_reinterpretation() {
    let mut a = app();
    for arch in [Architecture::M68k, Architecture::PowerPc] {
        assert_eq!(query(&mut a, arch), [0; 16]);
    }
    let bytes = std::array::from_fn(|i| (i as u8).wrapping_mul(17) ^ 0x81);
    a.runtime.memory.write(KEY_MAP, &bytes).unwrap();
    for arch in [Architecture::M68k, Architecture::PowerPc] {
        assert_eq!(query(&mut a, arch), bytes);
        assert_eq!(a.runtime.memory.read(KEY_MAP, 16).unwrap(), bytes);
    }
    let entry = systemless_future::traps::lookup(&a.runtime.memory, 0xa976, true).unwrap();
    assert_ne!(entry, systemless_future::traps::UNIMPLEMENTED);
    assert_eq!(a.runtime.memory.u16(GuestPtr(entry)).unwrap(), 0xa976);
}
#[test]
fn host_all_128_bits_press_release_and_lock_latch_use_direct_code_indices() {
    let mut a = app();
    let mut k = Keyboard::default();
    let mut expected = [0; 16];
    for code in 0..128u8 {
        key(&mut a, &mut k, code, None, true, false);
        expected[usize::from(code / 8)] |= 1 << (code % 8);
        for arch in [Architecture::M68k, Architecture::PowerPc] {
            assert_eq!(query(&mut a, arch), expected, "code{code}");
        }
    }
    for code in 0..128u8 {
        key(&mut a, &mut k, code, None, false, false);
        if code != 57 {
            expected[usize::from(code / 8)] &= !(1 << (code % 8));
        }
        for arch in [Architecture::M68k, Architecture::PowerPc] {
            assert_eq!(query(&mut a, arch), expected);
        }
    }
    assert_eq!(expected, [0, 0, 0, 0, 0, 0, 0, 2, 0, 0, 0, 0, 0, 0, 0, 0]);
    key(&mut a, &mut k, 57, None, true, false);
    key(&mut a, &mut k, 57, None, false, false);
    assert_eq!(query(&mut a, Architecture::PowerPc), [0; 16]);
}
#[test]
fn queued_key_events_and_repeats_do_not_substitute_for_current_physical_state() {
    let mut a = app();
    let mut k = Keyboard::default();
    a.runtime
        .memory
        .write(events::SYSTEM_MASK, &0xffffu16.to_be_bytes())
        .unwrap();
    a.runtime
        .memory
        .write(GuestPtr(0x16a), &31u32.to_be_bytes())
        .unwrap();
    key(&mut a, &mut k, 38, Some("j"), true, false);
    assert!(down(&query(&mut a, Architecture::M68k), 38));
    assert_eq!(event(&mut a), (3, 0x266a, 31, 0x80));
    assert!(down(&query(&mut a, Architecture::PowerPc), 38));
    a.runtime
        .memory
        .write(GuestPtr(0x16a), &32u32.to_be_bytes())
        .unwrap();
    key(&mut a, &mut k, 38, Some("j"), true, true);
    assert_eq!(event(&mut a), (5, 0x266a, 32, 0x80));
    key(&mut a, &mut k, 38, None, false, false);
    assert!(!down(&query(&mut a, Architecture::M68k), 38));
    assert_eq!(event(&mut a), (4, 0x266a, 32, 0x80));
    events::post(&mut a.runtime.memory, 3, 0x266a, 0, a.runtime.process.input).unwrap();
    assert!(!down(&query(&mut a, Architecture::PowerPc), 38));
}
#[test]
fn masked_events_modifiers_left_right_and_focus_loss_keep_polling_coherent() {
    let mut a = app();
    let mut k = Keyboard::default();
    a.runtime
        .memory
        .write(events::SYSTEM_MASK, &0u16.to_be_bytes())
        .unwrap();
    k.modifiers(&mut a.runtime.memory, &mut a.runtime.process.input, 0x200)
        .unwrap();
    assert!(down(&query(&mut a, Architecture::M68k), 56));
    key(&mut a, &mut k, 60, None, true, false);
    let map = query(&mut a, Architecture::PowerPc);
    assert!(!down(&map, 56));
    assert!(down(&map, 60));
    key(&mut a, &mut k, 56, None, true, false);
    key(&mut a, &mut k, 60, None, false, false);
    assert_eq!(a.runtime.process.input.modifiers & 0x200, 0x200);
    key(&mut a, &mut k, 56, None, false, false);
    assert_eq!(a.runtime.process.input.modifiers & 0x200, 0);
    key(&mut a, &mut k, 57, None, true, false);
    key(&mut a, &mut k, 57, None, false, false);
    key(&mut a, &mut k, 0, None, true, false);
    assert!(down(&query(&mut a, Architecture::PowerPc), 0));
    assert!(!events::available(&a.runtime.memory, 0xffff).unwrap());
    k.unfocus(&mut a.runtime.memory, &mut a.runtime.process.input)
        .unwrap();
    let map = query(&mut a, Architecture::M68k);
    assert!(!down(&map, 0));
    assert!(down(&map, 57));
    assert_eq!(a.runtime.process.input.modifiers, 0x480);
    a.runtime
        .memory
        .write(events::SYSTEM_MASK, &0xffffu16.to_be_bytes())
        .unwrap();
    key(&mut a, &mut k, 0, None, true, false);
    assert_eq!(event(&mut a).1, 0x41);
}
#[test]
fn layout_text_uses_shared_macroman_and_unknown_text_does_not_become_ascii() {
    let mut a = app();
    let mut k = Keyboard::default();
    key(&mut a, &mut k, 14, Some("é"), true, false);
    assert_eq!(event(&mut a).1, 0x0e8e);
    key(&mut a, &mut k, 14, None, false, false);
    key(&mut a, &mut k, 0, Some("😀"), true, false);
    assert!(down(&query(&mut a, Architecture::M68k), 0));
    assert!(!events::available(&a.runtime.memory, 0xffff).unwrap());
    key(&mut a, &mut k, 0, None, false, false);
    key(&mut a, &mut k, 0, Some("\0"), true, false);
    assert!(!events::available(&a.runtime.memory, 0xffff).unwrap());
    key(&mut a, &mut k, 0, None, false, false);
    key(&mut a, &mut k, 36, Some("\n"), true, false);
    assert_eq!(event(&mut a).1, 0x240d);
    assert_eq!(virtual_key(winit::keyboard::KeyCode::KeyA), Some(0));
    assert_eq!(virtual_key(winit::keyboard::KeyCode::KeyJ), Some(38));
    assert_eq!(virtual_key(winit::keyboard::KeyCode::ArrowLeft), Some(123));
    assert_eq!(virtual_key(winit::keyboard::KeyCode::AudioVolumeUp), None);
}
#[test]
fn native_and_ppc_destination_preflight_reject_nil_short_readonly_and_unmapped_outputs() {
    let mut a = app();
    a.runtime
        .memory
        .map(GuestPtr(0x6000), vec![0xa5; 15], Permissions::RW)
        .unwrap();
    a.runtime
        .memory
        .map(GuestPtr(0x7000), vec![0xa5; 16], Permissions::RX)
        .unwrap();
    let sp = STACK_BASE + 0x7000;
    for destination in [0u32, 0x6000, 0x7000, 0xfffffff0] {
        a.runtime
            .memory
            .write(GuestPtr(sp), &destination.to_be_bytes())
            .unwrap();
        let mut cpu = Cpu::new(Architecture::M68k, 0, sp);
        let Cpu::M68k(c) = &mut cpu else { panic!() };
        assert!(Abi::trap(0xa976, c, &a.runtime.memory).is_err());
        assert_eq!(c.dar[15], sp);
        let mut cpu = Cpu::new(Architecture::PowerPc, 0, sp);
        let Cpu::PowerPc(c) = &mut cpu else { panic!() };
        c.gpr[3] = destination;
        assert!(Abi::import(Service::GetKeys, c, &a.runtime.memory).is_err());
        assert_eq!(c.gpr[3], destination);
    }
    assert_eq!(
        a.runtime.memory.read(GuestPtr(0x6000), 15).unwrap(),
        [0xa5; 15]
    );
    assert_eq!(
        a.runtime.memory.read(GuestPtr(0x7000), 16).unwrap(),
        [0xa5; 16]
    );
    let mut memory = Memory::default();
    memory
        .map(GuestPtr(0x5000), vec![0xa5; 16], Permissions::RW)
        .unwrap();
    assert!(keyboard::get_keys(&mut memory, GuestPtr(0x5000)).is_err());
    assert_eq!(memory.read(GuestPtr(0x5000), 16).unwrap(), [0xa5; 16]);
}
#[test]
fn guest_map_aliases_snapshot_and_failed_host_delivery_can_retry_without_stuck_keys() {
    let mut a = app();
    let bytes = std::array::from_fn::<_, 16, _>(|i| i as u8 * 13);
    a.runtime.memory.write(KEY_MAP, &bytes).unwrap();
    keyboard::get_keys(&mut a.runtime.memory, GuestPtr(KEY_MAP.0 + 1)).unwrap();
    assert_eq!(
        a.runtime.memory.read(GuestPtr(KEY_MAP.0 + 1), 16).unwrap(),
        bytes
    );
    a.runtime.memory.write(KEY_MAP, &[0; 16]).unwrap();
    let mut k = Keyboard::default();
    a.runtime
        .memory
        .write(
            GuestPtr(events::QUEUE_HEADER.0 + 2),
            &0x1234u32.to_be_bytes(),
        )
        .unwrap();
    let input = a.runtime.process.input.modifiers;
    assert!(k
        .key(
            &mut a.runtime.memory,
            &mut a.runtime.process.input,
            0,
            Some("a"),
            true,
            false
        )
        .is_err());
    assert_eq!(a.runtime.memory.read(KEY_MAP, 16).unwrap(), [0; 16]);
    assert_eq!(a.runtime.process.input.modifiers, input);
    a.runtime
        .memory
        .write(GuestPtr(events::QUEUE_HEADER.0 + 2), &0u32.to_be_bytes())
        .unwrap();
    key(&mut a, &mut k, 0, Some("a"), true, false);
    assert_eq!(event(&mut a).0, 3);
    assert!(down(&query(&mut a, Architecture::PowerPc), 0));
    let mut memory = Memory::default();
    memory.map(KEY_MAP, vec![0; 16], Permissions::RX).unwrap();
    let mut input = events::Input::default();
    let mut k = Keyboard::default();
    assert!(matches!(
        k.key(&mut memory, &mut input, 0, Some("a"), true, false),
        Err(systemless_future::Error::Memory {
            address: 0x174,
            access: systemless_future::memory::Access::Write
        })
    ));
    assert_eq!(input.modifiers, 0x80);
}

#[test]
fn navigation_and_function_keys_use_classic_characters_without_assigning_holes() {
    let mut a = app();
    let mut k = Keyboard::default();
    for (code, character) in [
        (115, 1u32),
        (119, 4),
        (114, 5),
        (116, 11),
        (121, 12),
        (117, 127),
        (123, 28),
        (124, 29),
        (126, 30),
        (125, 31),
        (122, 16),
        (106, 16),
    ] {
        key(&mut a, &mut k, code, None, true, false);
        assert_eq!(event(&mut a).1, u32::from(code) << 8 | character);
        key(&mut a, &mut k, code, None, false, false);
    }
    key(&mut a, &mut k, 102, None, true, false);
    assert!(!events::available(&a.runtime.memory, 0xffff).unwrap());
    assert!(down(&query(&mut a, Architecture::PowerPc), 102));
}
