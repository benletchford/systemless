mod support;
use systemless_future::{
    cpu::{Architecture, Cpu},
    load::{LoadedApplication, STACK_BASE},
    memory::{GuestPtr, Permissions},
    runtime::Stop,
};
fn app(selector: u16, pointer: u32) -> LoadedApplication {
    let f = support::pef_program(&[0x4e800020], &[], 64);
    let mut a = LoadedApplication::pef(&f.bytes, None).unwrap();
    a.runtime
        .memory
        .map(
            GuestPtr(0x50000000),
            vec![
                0xa9,
                if ([0, 2, 4, 6, 8, 10].contains(&(selector & 0xff))
                    && [0, 0x0800, 0x1000, 0x2000, 0x2800].contains(&(selector & 0xff00)))
                    || [
                        0, 2, 4, 6, 8, 10, 0x000e, 0x0810, 0x080e, 0x200e, 0x280e, 0x2010, 0x2810,
                    ]
                    .contains(&selector)
                {
                    0xeb
                } else {
                    0xec
                },
            ],
            Permissions::RX,
        )
        .unwrap();
    let stack = STACK_BASE + 0x7000;
    let mut frame = selector.to_be_bytes().to_vec();
    frame.extend(pointer.to_be_bytes());
    frame.extend([0xcc; 4]);
    a.runtime.memory.write(GuestPtr(stack), &frame).unwrap();
    a.runtime.cpu = Cpu::new(Architecture::M68k, 0x50000000, stack);
    let Cpu::M68k(c) = &mut a.runtime.cpu else {
        panic!()
    };
    for i in 0..15 {
        c.dar[i] = 0x12340000 + i as u32;
    }
    a
}
#[test]
fn native_sane_arctangent_updates_ten_bytes_and_preserves_registers_and_frame() {
    let pointer = STACK_BASE + 0x6000;
    let mut a = app(0x1e, pointer);
    let input = [0x3f, 0xff, 0x80, 0, 0, 0, 0, 0, 0, 1];
    a.runtime.memory.write(GuestPtr(pointer), &input).unwrap();
    assert_eq!(a.runtime.advance(1).unwrap(), Stop::Budget);
    assert_eq!(
        a.runtime.memory.read(GuestPtr(pointer), 10).unwrap(),
        [0x3f, 0xfe, 0xc9, 0x0f, 0xda, 0xa2, 0x21, 0x68, 0xc2, 0x36]
    );
    let Cpu::M68k(c) = &a.runtime.cpu else {
        panic!()
    };
    for i in 0..15 {
        assert_eq!(c.dar[i], 0x12340000 + i as u32);
    }
    assert_eq!(c.dar[15], STACK_BASE + 0x7006);
    assert_eq!(
        a.runtime.memory.read(GuestPtr(c.dar[15]), 4).unwrap(),
        [0xcc; 4]
    );
    assert!(a.runtime.process.numerics.inexact);
}
#[test]
fn unsupported_selectors_and_bad_destinations_fault_without_popping_the_frame() {
    for (selector, pointer) in [(0x22, STACK_BASE + 0x6000), (0x1e, 0xfffffff0)] {
        let mut a = app(selector, pointer);
        assert!(a.runtime.advance(1).is_err());
        let Cpu::M68k(c) = &a.runtime.cpu else {
            panic!()
        };
        assert_eq!(c.dar[15], STACK_BASE + 0x7000);
        assert!(!a.runtime.process.numerics.inexact);
    }
}

#[test]
fn native_extended_multiply_snapshots_aliases_preserves_registers_and_pops_ten_bytes() {
    let pointer = STACK_BASE + 0x6000;
    let mut a = app(4, pointer);
    a.runtime
        .memory
        .write(GuestPtr(STACK_BASE + 0x7006), &pointer.to_be_bytes())
        .unwrap();
    a.runtime
        .memory
        .write(GuestPtr(STACK_BASE + 0x700a), &[0xcc; 4])
        .unwrap();
    a.runtime
        .memory
        .write(GuestPtr(pointer), &[0x3f, 0xff, 0x80, 0, 0, 0, 0, 0, 0, 1])
        .unwrap();
    assert_eq!(a.runtime.advance(1).unwrap(), Stop::Budget);
    assert_eq!(
        a.runtime.memory.read(GuestPtr(pointer), 10).unwrap(),
        [0x3f, 0xff, 0x80, 0, 0, 0, 0, 0, 0, 2]
    );
    let Cpu::M68k(c) = &a.runtime.cpu else {
        panic!()
    };
    for i in 0..15 {
        assert_eq!(c.dar[i], 0x12340000 + i as u32);
    }
    assert_eq!(c.dar[15], STACK_BASE + 0x700a);
    assert_eq!(
        a.runtime.memory.read(GuestPtr(c.dar[15]), 4).unwrap(),
        [0xcc; 4]
    );
    assert!(a.runtime.process.numerics.inexact);
}
#[test]
fn multiply_bad_source_does_not_mutate_destination_or_pop_the_frame() {
    let pointer = STACK_BASE + 0x6000;
    let mut a = app(4, pointer);
    let input = [0x3f, 0xff, 0x80, 0, 0, 0, 0, 0, 0, 1];
    a.runtime.memory.write(GuestPtr(pointer), &input).unwrap();
    assert!(a.runtime.advance(1).is_err());
    let Cpu::M68k(c) = &a.runtime.cpu else {
        panic!()
    };
    assert_eq!(c.dar[15], STACK_BASE + 0x7000);
    assert_eq!(a.runtime.memory.read(GuestPtr(pointer), 10).unwrap(), input);
    assert!(!a.runtime.process.numerics.inexact);
}

#[test]
fn native_extended_to_double_snapshots_in_place_source_and_writes_only_eight_bytes() {
    let pointer = STACK_BASE + 0x6000;
    let mut a = app(0x0810, pointer);
    a.runtime
        .memory
        .write(GuestPtr(STACK_BASE + 0x7006), &pointer.to_be_bytes())
        .unwrap();
    a.runtime
        .memory
        .write(
            GuestPtr(pointer),
            &[0x3f, 0xfe, 0xc9, 0x0f, 0xda, 0xa2, 0x21, 0x68, 0xc2, 0x35],
        )
        .unwrap();
    assert_eq!(a.runtime.advance(1).unwrap(), Stop::Budget);
    assert_eq!(
        a.runtime.memory.read(GuestPtr(pointer), 10).unwrap(),
        [0x3f, 0xe9, 0x21, 0xfb, 0x54, 0x44, 0x2d, 0x18, 0xc2, 0x35]
    );
    let Cpu::M68k(c) = &a.runtime.cpu else {
        panic!()
    };
    assert_eq!(c.dar[15], STACK_BASE + 0x700a);
    for i in 0..15 {
        assert_eq!(c.dar[i], 0x12340000 + i as u32);
    }
}

#[test]
fn native_double_to_extended_reads_eight_bytes_and_expands_into_ten() {
    let source = 0x52000000u32;
    let destination = STACK_BASE + 0x6000;
    let mut a = app(0x080e, destination);
    a.runtime
        .memory
        .map(
            GuestPtr(source),
            1u64.to_be_bytes().to_vec(),
            Permissions::RX,
        )
        .unwrap();
    a.runtime
        .memory
        .write(GuestPtr(STACK_BASE + 0x7006), &source.to_be_bytes())
        .unwrap();
    assert_eq!(a.runtime.advance(1).unwrap(), Stop::Budget);
    assert_eq!(
        a.runtime.memory.read(GuestPtr(destination), 10).unwrap(),
        [0x3b, 0xcd, 0x80, 0, 0, 0, 0, 0, 0, 0]
    );
    let Cpu::M68k(c) = &a.runtime.cpu else {
        panic!()
    };
    assert_eq!(c.dar[15], STACK_BASE + 0x700a);
    assert!(!a.runtime.process.numerics.inexact);
}

#[test]
fn native_extended_division_preserves_operand_order_and_cpu_state() {
    let destination = STACK_BASE + 0x6000;
    let source = destination + 16;
    let mut a = app(6, destination);
    a.runtime
        .memory
        .write(GuestPtr(STACK_BASE + 0x7006), &source.to_be_bytes())
        .unwrap();
    a.runtime
        .memory
        .write(
            GuestPtr(destination),
            &[0x3f, 0xff, 0x80, 0, 0, 0, 0, 0, 0, 0],
        )
        .unwrap();
    a.runtime
        .memory
        .write(GuestPtr(source), &[0x40, 0, 0xc0, 0, 0, 0, 0, 0, 0, 0])
        .unwrap();
    assert_eq!(a.runtime.advance(1).unwrap(), Stop::Budget);
    assert_eq!(
        a.runtime.memory.read(GuestPtr(destination), 10).unwrap(),
        [0x3f, 0xfd, 0xaa, 0xaa, 0xaa, 0xaa, 0xaa, 0xaa, 0xaa, 0xab]
    );
    let Cpu::M68k(c) = &a.runtime.cpu else {
        panic!()
    };
    assert_eq!(c.dar[15], STACK_BASE + 0x700a);
    for i in 0..15 {
        assert_eq!(c.dar[i], 0x12340000 + i as u32);
    }
}

#[test]
fn native_signed_integer_conversion_reads_exactly_its_source_width() {
    for (selector, input, expected) in [
        (
            0x200e,
            (-32768i16).to_be_bytes().to_vec(),
            [0xc0, 0x0e, 0x80, 0, 0, 0, 0, 0, 0, 0],
        ),
        (
            0x280e,
            i32::MIN.to_be_bytes().to_vec(),
            [0xc0, 0x1e, 0x80, 0, 0, 0, 0, 0, 0, 0],
        ),
    ] {
        let source = 0x52000000u32;
        let destination = STACK_BASE + 0x6000;
        let mut a = app(selector, destination);
        a.runtime
            .memory
            .map(GuestPtr(source), input, Permissions::RX)
            .unwrap();
        a.runtime
            .memory
            .write(GuestPtr(STACK_BASE + 0x7006), &source.to_be_bytes())
            .unwrap();
        assert_eq!(a.runtime.advance(1).unwrap(), Stop::Budget);
        assert_eq!(
            a.runtime.memory.read(GuestPtr(destination), 10).unwrap(),
            expected
        );
        let Cpu::M68k(c) = &a.runtime.cpu else {
            panic!()
        };
        assert_eq!(c.dar[15], STACK_BASE + 0x700a);
        assert!(!a.runtime.process.numerics.inexact);
    }
}

#[test]
fn native_trig_uses_six_byte_frames_and_preserves_registers() {
    for (selector, expected) in [
        (
            0x18,
            [0x3f, 0xfe, 0xd7, 0x6a, 0xa4, 0x78, 0x48, 0x67, 0x70, 0x21],
        ),
        (
            0x1a,
            [0x3f, 0xfe, 0x8a, 0x51, 0x40, 0x7d, 0xa8, 0x34, 0x5c, 0x92],
        ),
        (
            0x1c,
            [0x3f, 0xff, 0xc7, 0x59, 0x22, 0xe5, 0xf7, 0x1d, 0x2d, 0xc5],
        ),
    ] {
        let pointer = STACK_BASE + 0x6000;
        let mut a = app(selector, pointer);
        a.runtime
            .memory
            .write(GuestPtr(pointer), &[0x3f, 0xff, 0x80, 0, 0, 0, 0, 0, 0, 0])
            .unwrap();
        assert_eq!(a.runtime.advance(1).unwrap(), Stop::Budget);
        assert_eq!(
            a.runtime.memory.read(GuestPtr(pointer), 10).unwrap(),
            expected
        );
        let Cpu::M68k(c) = &a.runtime.cpu else {
            panic!()
        };
        assert_eq!(c.dar[15], STACK_BASE + 0x7006);
        for i in 0..15 {
            assert_eq!(c.dar[i], 0x12340000 + i as u32);
        }
    }
}

#[test]
fn native_extended_copy_preserves_payload_and_quiets_signaling_nan() {
    for input in [
        [0x3f, 0xfe, 0xd7, 0x6a, 0xa4, 0x78, 0x48, 0x67, 0x70, 0x21],
        [0x7f, 0xff, 0x80, 0, 0, 0, 0, 0, 0, 1],
    ] {
        let source = 0x52000000u32;
        let destination = STACK_BASE + 0x6000;
        let mut a = app(0x000e, destination);
        a.runtime
            .memory
            .map(GuestPtr(source), input.to_vec(), Permissions::RX)
            .unwrap();
        a.runtime
            .memory
            .write(GuestPtr(STACK_BASE + 0x7006), &source.to_be_bytes())
            .unwrap();
        assert_eq!(a.runtime.advance(1).unwrap(), Stop::Budget);
        let mut expected = input;
        if input[0] == 0x7f {
            expected[2] |= 0x40;
        }
        assert_eq!(
            a.runtime.memory.read(GuestPtr(destination), 10).unwrap(),
            expected
        );
        assert_eq!(a.runtime.process.numerics.invalid, input[0] == 0x7f);
        assert!(!a.runtime.process.numerics.inexact);
        let Cpu::M68k(c) = &a.runtime.cpu else {
            panic!()
        };
        assert_eq!(c.dar[15], STACK_BASE + 0x700a);
    }
}

#[test]
fn native_extended_classification_returns_signed_word_and_never_quiets_nan() {
    for sign in [0u16, 0x8000] {
        for (e, m, class) in [
            (0x7fff, 0x8000_0000_0000_0001u64, 1i16),
            (0x7fff, 0xc000_0000_0000_0001, 2),
            (0x7fff, 0x8000_0000_0000_0000, 3),
            (0, 0, 4),
            (0x3fff, 0x8000_0000_0000_0000, 5),
            (0, 1, 6),
            (0, 1 << 63, 5),
            (0x4000, 1 << 62, 6),
            (0x2000, 0, 4),
            (0x1ff9, 0x1006_49b8_b82a_1010, 6),
        ] {
            let source = 0x52000000u32;
            let destination = STACK_BASE + 0x6000;
            let mut a = app(0x1c, destination);
            a.runtime
                .memory
                .map(GuestPtr(0x53000000), vec![0xa9, 0xeb], Permissions::RX)
                .unwrap();
            let Cpu::M68k(c) = &mut a.runtime.cpu else {
                panic!()
            };
            c.pc = 0x53000000;
            let mut bytes = (e | sign).to_be_bytes().to_vec();
            bytes.extend(m.to_be_bytes());
            a.runtime
                .memory
                .map(GuestPtr(source), bytes.clone(), Permissions::RX)
                .unwrap();
            a.runtime
                .memory
                .write(GuestPtr(STACK_BASE + 0x7006), &source.to_be_bytes())
                .unwrap();
            a.runtime
                .memory
                .write(GuestPtr(destination), &[0xcc; 4])
                .unwrap();
            assert_eq!(a.runtime.advance(1).unwrap(), Stop::Budget);
            let expected = if sign == 0 { class } else { -class };
            assert_eq!(
                a.runtime.memory.u16(GuestPtr(destination)).unwrap(),
                expected as u16
            );
            assert_eq!(
                a.runtime.memory.read(GuestPtr(destination + 2), 2).unwrap(),
                [0xcc; 2]
            );
            assert_eq!(a.runtime.memory.read(GuestPtr(source), 10).unwrap(), bytes);
            assert!(!a.runtime.process.numerics.invalid && !a.runtime.process.numerics.inexact);
            let Cpu::M68k(c) = &a.runtime.cpu else {
                panic!()
            };
            assert_eq!(c.dar[15], STACK_BASE + 0x700a);
        }
    }
}

fn environment_call(a: &mut LoadedApplication, selector: u16, pointer: u32) {
    let stack = STACK_BASE + 0x7000;
    a.runtime
        .memory
        .write(GuestPtr(stack), &selector.to_be_bytes())
        .unwrap();
    a.runtime
        .memory
        .write(GuestPtr(stack + 2), &pointer.to_be_bytes())
        .unwrap();
    a.runtime.cpu = Cpu::new(Architecture::M68k, 0x54000000, stack);
    let Cpu::M68k(c) = &mut a.runtime.cpu else {
        panic!()
    };
    c.dar[0] = 0x12345678;
    assert_eq!(a.runtime.advance(1).unwrap(), Stop::Budget);
    let Cpu::M68k(c) = &a.runtime.cpu else {
        panic!()
    };
    assert_eq!(c.dar[15], stack + 6);
    assert_eq!(c.dar[0], 0x12345678);
}
#[test]
fn native_environment_entry_saves_real_flags_exit_merges_and_set_replaces() {
    let pointer = STACK_BASE + 0x6000;
    let mut a = app(0, pointer);
    a.runtime
        .memory
        .map(GuestPtr(0x54000000), vec![0xa9, 0xeb], Permissions::RX)
        .unwrap();
    a.runtime
        .memory
        .write(GuestPtr(pointer), &0x1f00u16.to_be_bytes())
        .unwrap();
    environment_call(&mut a, 1, pointer);
    environment_call(&mut a, 0x17, pointer + 2);
    assert_eq!(a.runtime.memory.u16(GuestPtr(pointer + 2)).unwrap(), 0x1f00);
    assert_eq!(a.runtime.process.numerics.environment_word(), 0);
    // A real zero divisor inside the procedure must survive restoration.
    let source = pointer + 32;
    let destination = pointer + 48;
    a.runtime.memory.write(GuestPtr(source), &[0; 10]).unwrap();
    a.runtime
        .memory
        .write(
            GuestPtr(destination),
            &[0x3f, 0xff, 0x80, 0, 0, 0, 0, 0, 0, 0],
        )
        .unwrap();
    a.runtime
        .process
        .numerics
        .divide_extended(
            &mut a.runtime.memory,
            GuestPtr(source),
            GuestPtr(destination),
        )
        .unwrap();
    a.runtime
        .memory
        .write(GuestPtr(pointer + 4), &0x0100u16.to_be_bytes())
        .unwrap();
    environment_call(&mut a, 0x19, pointer + 4);
    assert_eq!(a.runtime.process.numerics.environment_word(), 0x0900);
    a.runtime
        .memory
        .write(GuestPtr(pointer + 6), &0u16.to_be_bytes())
        .unwrap();
    environment_call(&mut a, 1, pointer + 6);
    assert_eq!(a.runtime.process.numerics.environment_word(), 0);
    environment_call(&mut a, 0x19, pointer + 2);
    environment_call(&mut a, 3, pointer + 8);
    assert_eq!(a.runtime.memory.u16(GuestPtr(pointer + 8)).unwrap(), 0x1f00);
}
#[test]
fn environment_faults_preserve_live_status_and_readonly_entry_does_not_pop() {
    use systemless_future::numerics::Environment;
    let pointer = 0x52000000;
    let mut a = app(0, pointer);
    a.runtime
        .memory
        .map(
            GuestPtr(pointer),
            0x2000u16.to_be_bytes().to_vec(),
            Permissions::RX,
        )
        .unwrap();
    a.runtime.process.numerics.invalid = true;
    for op in [Environment::Set, Environment::Exit, Environment::Entry] {
        assert!(a
            .runtime
            .process
            .numerics
            .environment(&mut a.runtime.memory, GuestPtr(pointer), op)
            .is_err());
        assert_eq!(a.runtime.process.numerics.environment_word(), 0x0100);
    }
    a.runtime
        .memory
        .map(GuestPtr(0x54000000), vec![0xa9, 0xeb], Permissions::RX)
        .unwrap();
    a.runtime
        .memory
        .write(GuestPtr(STACK_BASE + 0x7000), &0x17u16.to_be_bytes())
        .unwrap();
    a.runtime.cpu = Cpu::new(Architecture::M68k, 0x54000000, STACK_BASE + 0x7000);
    assert!(a.runtime.advance(1).is_err());
    let Cpu::M68k(c) = &a.runtime.cpu else {
        panic!()
    };
    assert_eq!(c.dar[15], STACK_BASE + 0x7000);
    assert_eq!(a.runtime.process.numerics.environment_word(), 0x0100);
}

#[test]
fn native_comparison_returns_sane_condition_codes_without_writing_readonly_operands() {
    for (ae, am, be, bm, selector, cc, invalid) in [
        (0x3fff, 1u64 << 63, 0x4000, 1u64 << 63, 8, 0x19, false),
        (0x4000, 1 << 63, 0x3fff, 1 << 63, 8, 0, false),
        (0, 0, 0x8000, 0, 8, 4, false),
        (0xbfff, 1 << 63, 0xc000, 1 << 63, 8, 0, false),
        (0x7fff, 0xc000_0000_0000_0001, 0x3fff, 1 << 63, 8, 2, false),
        (0x7fff, 0xc000_0000_0000_0001, 0x3fff, 1 << 63, 10, 2, true),
        (0x7fff, 0x8000_0000_0000_0001, 0x3fff, 1 << 63, 8, 2, true),
    ] {
        let source = 0x52000000u32;
        let destination = 0x53000000;
        let mut a = app(selector, destination);
        let mut first = (ae as u16).to_be_bytes().to_vec();
        first.extend(am.to_be_bytes());
        let mut second = (be as u16).to_be_bytes().to_vec();
        second.extend(bm.to_be_bytes());
        a.runtime
            .memory
            .map(GuestPtr(destination), first.clone(), Permissions::RX)
            .unwrap();
        a.runtime
            .memory
            .map(GuestPtr(source), second.clone(), Permissions::RX)
            .unwrap();
        a.runtime
            .memory
            .write(GuestPtr(STACK_BASE + 0x7006), &source.to_be_bytes())
            .unwrap();
        let Cpu::M68k(c) = &mut a.runtime.cpu else {
            panic!()
        };
        let preserved = (c.get_sr() & !0x1f) | 0x0300;
        c.set_sr(preserved | 0x1f);
        assert_eq!(a.runtime.advance(1).unwrap(), Stop::Budget);
        let Cpu::M68k(c) = &a.runtime.cpu else {
            panic!()
        };
        assert_eq!(c.get_sr(), preserved | cc);
        assert_eq!(c.dar[15], STACK_BASE + 0x700a);
        for i in 0..15 {
            assert_eq!(c.dar[i], 0x12340000 + i as u32);
        }
        assert_eq!(a.runtime.process.numerics.invalid, invalid);
        assert!(!a.runtime.process.numerics.inexact);
        assert_eq!(
            a.runtime.memory.read(GuestPtr(destination), 10).unwrap(),
            first
        );
        assert_eq!(a.runtime.memory.read(GuestPtr(source), 10).unwrap(), second);
    }
}

#[test]
fn native_extended_add_and_subtract_preserve_low_bits_operand_order_and_frame() {
    for (selector, expected) in [
        (0, [0x40, 0, 0x80, 0, 0, 0, 0, 0, 0, 0]),
        (2, [0x3f, 0xbf, 0x80, 0, 0, 0, 0, 0, 0, 0]),
    ] {
        let source = STACK_BASE + 0x6100;
        let destination = STACK_BASE + 0x6000;
        let mut a = app(selector, destination);
        a.runtime
            .memory
            .write(
                GuestPtr(destination),
                &[0x3f, 0xff, 0x80, 0, 0, 0, 0, 0, 0, 0],
            )
            .unwrap();
        a.runtime
            .memory
            .write(
                GuestPtr(source),
                &[0x3f, 0xfe, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff],
            )
            .unwrap();
        a.runtime
            .memory
            .write(GuestPtr(STACK_BASE + 0x7006), &source.to_be_bytes())
            .unwrap();
        assert_eq!(a.runtime.advance(1).unwrap(), Stop::Budget);
        assert_eq!(
            a.runtime.memory.read(GuestPtr(destination), 10).unwrap(),
            expected
        );
        let Cpu::M68k(c) = &a.runtime.cpu else {
            panic!()
        };
        assert_eq!(c.dar[15], STACK_BASE + 0x700a);
        for i in 0..15 {
            assert_eq!(c.dar[i], 0x12340000 + i as u32);
        }
    }
}

#[test]
fn native_extended_square_root_uses_six_byte_frame_and_preserves_registers() {
    let pointer = STACK_BASE + 0x6000;
    let mut a = app(0x12, pointer);
    a.runtime
        .memory
        .map(GuestPtr(0x54000000), vec![0xa9, 0xeb], Permissions::RX)
        .unwrap();
    let Cpu::M68k(c) = &mut a.runtime.cpu else {
        panic!()
    };
    c.pc = 0x54000000;
    a.runtime
        .memory
        .write(GuestPtr(pointer), &[0x40, 0, 0x80, 0, 0, 0, 0, 0, 0, 0])
        .unwrap();
    assert_eq!(a.runtime.advance(1).unwrap(), Stop::Budget);
    assert_eq!(
        a.runtime.memory.read(GuestPtr(pointer), 10).unwrap(),
        [0x3f, 0xff, 0xb5, 0x04, 0xf3, 0x33, 0xf9, 0xde, 0x64, 0x84]
    );
    let Cpu::M68k(c) = &a.runtime.cpu else {
        panic!()
    };
    assert_eq!(c.dar[15], STACK_BASE + 0x7006);
    for i in 0..15 {
        assert_eq!(c.dar[i], 0x12340000 + i as u32);
    }
}

#[test]
fn native_binary_source_formats_share_arithmetic_and_read_exact_source_extents() {
    for (format, bytes) in [
        (0u16, vec![0x40, 0, 0x80, 0, 0, 0, 0, 0, 0, 0]),
        (0x0800, 2f64.to_bits().to_be_bytes().to_vec()),
        (0x1000, 2f32.to_bits().to_be_bytes().to_vec()),
        (0x2000, 2i16.to_be_bytes().to_vec()),
        (0x2800, 2i32.to_be_bytes().to_vec()),
    ] {
        for (op, exponent, mantissa) in [
            (0, 0x4001u16, 0xa000_0000_0000_0000u64),
            (2, 0x3fff, 1 << 63),
            (4, 0x4001, 0xc000_0000_0000_0000),
            (6, 0x3fff, 0xc000_0000_0000_0000),
        ] {
            let source = 0x52000000u32;
            let destination = STACK_BASE + 0x6000;
            let mut a = app(format | op, destination);
            a.runtime
                .memory
                .map(GuestPtr(source), bytes.clone(), Permissions::RX)
                .unwrap();
            a.runtime
                .memory
                .write(
                    GuestPtr(destination),
                    &[0x40, 0, 0xc0, 0, 0, 0, 0, 0, 0, 0, 0xcc, 0xcc],
                )
                .unwrap();
            a.runtime
                .memory
                .write(GuestPtr(STACK_BASE + 0x7006), &source.to_be_bytes())
                .unwrap();
            assert_eq!(a.runtime.advance(1).unwrap(), Stop::Budget);
            let mut expected = exponent.to_be_bytes().to_vec();
            expected.extend(mantissa.to_be_bytes());
            assert_eq!(
                a.runtime.memory.read(GuestPtr(destination), 10).unwrap(),
                expected
            );
            assert_eq!(
                a.runtime
                    .memory
                    .read(GuestPtr(destination + 10), 2)
                    .unwrap(),
                [0xcc; 2]
            );
            assert_eq!(
                a.runtime
                    .memory
                    .read(GuestPtr(source), bytes.len())
                    .unwrap(),
                bytes
            );
            assert!(!a.runtime.process.numerics.inexact);
            let Cpu::M68k(c) = &a.runtime.cpu else {
                panic!()
            };
            assert_eq!(c.dar[15], STACK_BASE + 0x700a);
        }
    }
}

#[test]
fn mixed_subnormals_are_exact_and_failed_operations_do_not_commit_exception_state() {
    use systemless_future::numerics::{Binary, SourceFormat};
    for (format, source_bytes, e) in [
        (SourceFormat::Double, 1u64.to_be_bytes().to_vec(), 0x3bcdu16),
        (SourceFormat::Single, 1u32.to_be_bytes().to_vec(), 0x3f6a),
    ] {
        let source = 0x52000000;
        let destination = STACK_BASE + 0x6000;
        let mut a = app(0, destination);
        a.runtime
            .memory
            .map(GuestPtr(source), source_bytes, Permissions::RX)
            .unwrap();
        a.runtime
            .memory
            .write(
                GuestPtr(destination),
                &[0x3f, 0xff, 0x80, 0, 0, 0, 0, 0, 0, 0],
            )
            .unwrap();
        a.runtime
            .process
            .numerics
            .binary(
                &mut a.runtime.memory,
                GuestPtr(source),
                GuestPtr(destination),
                format,
                Binary::Multiply,
            )
            .unwrap();
        assert_eq!(a.runtime.memory.u16(GuestPtr(destination)).unwrap(), e);
        assert!(!a.runtime.process.numerics.inexact && !a.runtime.process.numerics.underflow);
    }
    let source = 0x52000000;
    let destination = STACK_BASE + 0x6000;
    let mut a = app(0, destination);
    a.runtime
        .memory
        .map(
            GuestPtr(source),
            0x7f800001u32.to_be_bytes().to_vec(),
            Permissions::RX,
        )
        .unwrap();
    let bad = [0x3f, 0xff, 0, 0, 0, 0, 0, 0, 0, 1];
    a.runtime.memory.write(GuestPtr(destination), &bad).unwrap();
    assert!(a
        .runtime
        .process
        .numerics
        .binary(
            &mut a.runtime.memory,
            GuestPtr(source),
            GuestPtr(0x57000000),
            SourceFormat::Single,
            Binary::Multiply
        )
        .is_err());
    assert!(!a.runtime.process.numerics.invalid);
    assert_eq!(
        a.runtime.memory.read(GuestPtr(destination), 10).unwrap(),
        bad
    );

    a.runtime
        .memory
        .write(
            GuestPtr(destination),
            &[0x3f, 0xff, 0x80, 0, 0, 0, 0, 0, 0, 0],
        )
        .unwrap();
    a.runtime
        .process
        .numerics
        .binary(
            &mut a.runtime.memory,
            GuestPtr(source),
            GuestPtr(destination),
            SourceFormat::Single,
            Binary::Multiply,
        )
        .unwrap();
    assert!(a.runtime.process.numerics.invalid);
    assert_eq!(
        a.runtime.memory.read(GuestPtr(destination), 10).unwrap(),
        [0x7f, 0xff, 0xc0, 0, 1, 0, 0, 0, 0, 0]
    );
}

#[test]
fn native_integral_rounding_has_real_values_flags_and_six_byte_frames() {
    for (selector, expected) in [
        (0x14, [0xc0, 0, 0x80, 0, 0, 0, 0, 0, 0, 0]),
        (0x16, [0xbf, 0xff, 0x80, 0, 0, 0, 0, 0, 0, 0]),
    ] {
        let pointer = STACK_BASE + 0x6000;
        let mut a = app(selector, pointer);
        a.runtime
            .memory
            .map(GuestPtr(0x54000000), vec![0xa9, 0xeb], Permissions::RX)
            .unwrap();
        let Cpu::M68k(c) = &mut a.runtime.cpu else {
            panic!()
        };
        c.pc = 0x54000000;
        a.runtime
            .memory
            .write(GuestPtr(pointer), &[0xbf, 0xff, 0xe0, 0, 0, 0, 0, 0, 0, 0])
            .unwrap();
        assert_eq!(a.runtime.advance(1).unwrap(), Stop::Budget);
        assert_eq!(
            a.runtime.memory.read(GuestPtr(pointer), 10).unwrap(),
            expected
        );
        assert!(a.runtime.process.numerics.inexact);
        let Cpu::M68k(c) = &a.runtime.cpu else {
            panic!()
        };
        assert_eq!(c.dar[15], STACK_BASE + 0x7006);
        for i in 0..15 {
            assert_eq!(c.dar[i], 0x12340000 + i as u32);
        }
    }
}

#[test]
fn native_integer_storage_conversion_rounds_ties_and_checks_range() {
    for (selector, width) in [(0x2010, 2usize), (0x2810, 4)] {
        let mut cases = vec![
            (0x3ffeu16, 1u64 << 63, 0i32),
            (0x3fff, 0xc000_0000_0000_0000, 2),
            (0xc000, 0xa000_0000_0000_0000, -2),
            (0xc00e, 1 << 63, -32768),
            (0x400d, 0xfffe_0000_0000_0000, 32767),
            (0x8000, 0, 0),
            (0, 1, 0),
        ];
        if width == 4 {
            cases.extend([
                (0xc01e, 1 << 63, i32::MIN),
                (0x401d, 0xffff_fffe_0000_0000, i32::MAX),
            ]);
        }
        for (se, m, expected) in cases {
            let destination = STACK_BASE + 0x6000;
            let source = 0x55000000;
            let mut a = app(selector, destination);
            let mut input = se.to_be_bytes().to_vec();
            input.extend(m.to_be_bytes());
            a.runtime
                .memory
                .map(
                    GuestPtr(source),
                    input.clone(),
                    Permissions {
                        read: true,
                        write: false,
                        execute: false,
                    },
                )
                .unwrap();
            a.runtime
                .memory
                .write(GuestPtr(STACK_BASE + 0x7006), &source.to_be_bytes())
                .unwrap();
            a.runtime
                .memory
                .write(GuestPtr(destination), &[0xcc; 6])
                .unwrap();
            assert_eq!(a.runtime.advance(1).unwrap(), Stop::Budget);
            assert_eq!(
                a.runtime.memory.read(GuestPtr(destination), width).unwrap(),
                &expected.to_be_bytes()[4 - width..]
            );
            assert_eq!(
                a.runtime
                    .memory
                    .read(GuestPtr(destination + width as u32), 2)
                    .unwrap(),
                &[0xcc; 2]
            );
            assert_eq!(a.runtime.memory.read(GuestPtr(source), 10).unwrap(), input);
            assert!(!a.runtime.process.numerics.invalid);
            let Cpu::M68k(c) = &a.runtime.cpu else {
                panic!()
            };
            assert_eq!(c.dar[15], STACK_BASE + 0x700a);
            for i in 0..15 {
                assert_eq!(c.dar[i], 0x12340000 + i as u32);
            }
        }
    }
}

#[test]
fn exceptional_integer_conversions_return_software_sane_indefinite_and_invalid() {
    for (selector, se, m) in [
        (0x2010, 0x400eu16, 1u64 << 63),
        (0x2810, 0x401e, 1 << 63),
        (0x2010, 0x7fff, 1 << 63),
        (0x2010, 0x7fff, 0xc000_0000_0000_0001),
        (0x2810, 0x7fff, 0x8000_0000_0000_0001),
        (0x2010, 0xc00f, 1 << 63),
        (0x2810, 0x401f, 1 << 63),
        (0x2810, 0xd9fb, 0xb806_837a_b8b8_3a8d),
        (0x2810, 0xffff, 1 << 63),
    ] {
        let destination = STACK_BASE + 0x6000;
        let source = STACK_BASE + 0x6100;
        let mut a = app(selector, destination);
        let mut input = se.to_be_bytes().to_vec();
        input.extend(m.to_be_bytes());
        a.runtime.memory.write(GuestPtr(source), &input).unwrap();
        a.runtime
            .memory
            .write(GuestPtr(STACK_BASE + 0x7006), &source.to_be_bytes())
            .unwrap();
        a.runtime
            .memory
            .write(GuestPtr(destination), &[0xcc; 4])
            .unwrap();
        a.runtime.process.numerics.divide_by_zero = true;
        assert_eq!(a.runtime.advance(1).unwrap(), Stop::Budget);
        let width = if selector == 0x2010 { 2 } else { 4 };
        let expected = if width == 2 {
            i16::MIN.to_be_bytes().to_vec()
        } else {
            i32::MIN.to_be_bytes().to_vec()
        };
        assert_eq!(
            a.runtime.memory.read(GuestPtr(destination), width).unwrap(),
            expected
        );
        if width == 2 {
            assert_eq!(
                a.runtime.memory.read(GuestPtr(destination + 2), 2).unwrap(),
                [0xcc; 2]
            );
        }
        assert!(a.runtime.process.numerics.divide_by_zero);
        assert!(a.runtime.process.numerics.invalid);
        assert!(!a.runtime.process.numerics.inexact);
        assert!(!a.runtime.process.numerics.overflow);
        assert_eq!(a.runtime.memory.read(GuestPtr(source), 10).unwrap(), input);
        let Cpu::M68k(c) = &a.runtime.cpu else {
            panic!()
        };
        assert_eq!(c.dar[15], STACK_BASE + 0x700a);
        for i in 0..15 {
            assert_eq!(c.dar[i], 0x12340000 + i as u32);
        }
    }
}

#[test]
fn typed_comparisons_preserve_readonly_operands_and_extended_low_bits() {
    let formats = [
        (0u16, vec![0x3f, 0xff, 0x80, 0, 0, 0, 0, 0, 0, 0]),
        (0x0800, 1f64.to_bits().to_be_bytes().to_vec()),
        (0x1000, 1f32.to_bits().to_be_bytes().to_vec()),
        (0x2000, 1i16.to_be_bytes().to_vec()),
        (0x2800, 1i32.to_be_bytes().to_vec()),
    ];
    for (format, source_bytes) in formats {
        for op in [8, 10] {
            for (se, m, ccr) in [
                (0xbfffu16, 1u64 << 63, 0x19u16),
                (0x3fff, 1 << 63, 4),
                (0x3fff, (1 << 63) + 1, 0),
            ] {
                let destination = 0x55000000;
                let source = 0x56000000;
                let mut a = app(format | op, destination);
                let mut destination_bytes = se.to_be_bytes().to_vec();
                destination_bytes.extend(m.to_be_bytes());
                for (pointer, bytes) in [(destination, &destination_bytes), (source, &source_bytes)]
                {
                    a.runtime
                        .memory
                        .map(
                            GuestPtr(pointer),
                            bytes.clone(),
                            Permissions {
                                read: true,
                                write: false,
                                execute: false,
                            },
                        )
                        .unwrap();
                }
                a.runtime
                    .memory
                    .write(GuestPtr(STACK_BASE + 0x7006), &source.to_be_bytes())
                    .unwrap();
                let Cpu::M68k(c) = &mut a.runtime.cpu else {
                    panic!()
                };
                let upper = (c.get_sr() & !0x1f) | 0x0300;
                c.set_sr(upper | 0x1f);
                assert_eq!(a.runtime.advance(1).unwrap(), Stop::Budget);
                assert!(!a.runtime.process.numerics.invalid);
                assert!(!a.runtime.process.numerics.inexact);
                let Cpu::M68k(c) = &a.runtime.cpu else {
                    panic!()
                };
                assert_eq!(c.get_sr(), upper | ccr);
                assert_eq!(c.dar[15], STACK_BASE + 0x700a);
                for i in 0..15 {
                    assert_eq!(c.dar[i], 0x12340000 + i as u32);
                }
                assert_eq!(
                    a.runtime
                        .memory
                        .read(GuestPtr(source), source_bytes.len())
                        .unwrap(),
                    source_bytes
                );
                assert_eq!(
                    a.runtime.memory.read(GuestPtr(destination), 10).unwrap(),
                    destination_bytes
                );
            }
        }
    }
}

#[test]
fn typed_comparison_nan_status_is_transactional_without_operand_changes() {
    use systemless_future::numerics::SourceFormat;
    for (format, bits) in [
        (0x0800u16, 0x7ff8_0000_0000_0001u64),
        (0x0800, 0x7ff0_0000_0000_0001),
        (0x1000, 0x7fc0_0001),
        (0x1000, 0x7f80_0001),
    ] {
        let is_single = format == 0x1000;
        let source_bytes = if is_single {
            (bits as u32).to_be_bytes().to_vec()
        } else {
            bits.to_be_bytes().to_vec()
        };
        let snan = if is_single {
            bits & 0x0040_0000 == 0
        } else {
            bits & 0x0008_0000_0000_0000 == 0
        };
        for op in [8, 10] {
            let destination = STACK_BASE + 0x6000;
            let source = 0x56000000;
            let mut a = app(format | op, destination);
            a.runtime
                .memory
                .map(GuestPtr(source), source_bytes.clone(), Permissions::RX)
                .unwrap();
            let good = [0x3f, 0xff, 0x80, 0, 0, 0, 0, 0, 0, 0];
            a.runtime
                .memory
                .write(GuestPtr(destination), &good)
                .unwrap();
            a.runtime
                .memory
                .write(GuestPtr(STACK_BASE + 0x7006), &source.to_be_bytes())
                .unwrap();
            assert_eq!(a.runtime.advance(1).unwrap(), Stop::Budget);
            assert_eq!(a.runtime.process.numerics.invalid, snan || op == 10);
            let Cpu::M68k(c) = &a.runtime.cpu else {
                panic!()
            };
            assert_eq!(c.get_sr() & 0x1f, 2);
            assert_eq!(
                a.runtime
                    .memory
                    .read(GuestPtr(source), source_bytes.len())
                    .unwrap(),
                source_bytes
            );
            assert_eq!(
                a.runtime.memory.read(GuestPtr(destination), 10).unwrap(),
                good
            );
            a.runtime.process.numerics.invalid = false;
            a.runtime
                .memory
                .write(GuestPtr(destination), &[0x3f, 0xff, 0, 0, 0, 0, 0, 0, 0, 1])
                .unwrap();
            assert!(a
                .runtime
                .process
                .numerics
                .compare(
                    &a.runtime.memory,
                    GuestPtr(0x57000000),
                    GuestPtr(destination),
                    if is_single {
                        SourceFormat::Single
                    } else {
                        SourceFormat::Double
                    },
                    true
                )
                .is_err());
            assert!(!a.runtime.process.numerics.invalid);
        }
    }
}

#[test]
fn integer_range_is_checked_after_rounding_and_invalid_preserves_sticky_state() {
    use systemless_future::{memory::Memory, numerics::Numerics};
    for (bits, se, m, invalid, inexact) in [
        (16, 0x400du16, 0xffff_0000_0000_0000u64, true, false), // 32767.5 -> out of range
        (16, 0xc00e, 0x8000_8000_0000_0000, false, true),       // -32768.5 -> even -32768
        (16, 0xc00e, 0x8001_0000_0000_0000, true, false),       // -32769
        (32, 0x401d, 0xffff_ffff_0000_0000, true, false),       // 2147483647.5
    ] {
        for sticky in [false, true] {
            let mut memory = Memory::default();
            memory
                .map(GuestPtr(0x1000), vec![0x5a; 32], Permissions::RW)
                .unwrap();
            let mut bytes = se.to_be_bytes().to_vec();
            bytes.extend(m.to_be_bytes());
            memory.write(GuestPtr(0x1002), &bytes).unwrap();
            let mut n = Numerics {
                inexact: sticky,
                divide_by_zero: true,
                ..Numerics::default()
            };
            n.extended_to_integer(&mut memory, GuestPtr(0x1002), GuestPtr(0x1002), bits)
                .unwrap();
            let width = usize::from(bits / 8);
            assert_eq!(
                memory.read(GuestPtr(0x1002), width).unwrap(),
                if bits == 16 {
                    i16::MIN.to_be_bytes().to_vec()
                } else {
                    i32::MIN.to_be_bytes().to_vec()
                }
            );
            assert_eq!(
                (n.invalid, n.inexact, n.overflow, n.divide_by_zero),
                (invalid, inexact || sticky, false, true)
            );
            assert_eq!(memory.read(GuestPtr(0x1000), 2).unwrap(), [0x5a; 2]);
            assert_eq!(
                memory
                    .read(GuestPtr(0x1002 + width as u32), 10 - width)
                    .unwrap(),
                &bytes[width..]
            );
        }
    }
}

#[test]
fn native_extended_subtraction_accepts_the_observed_readonly_denormal_encoding() {
    let destination = STACK_BASE + 0x6000;
    let source = 0x55000000;
    let mut a = app(0x0002, destination);
    let input = [0x1f, 0xf9, 0x10, 0x06, 0x49, 0xb8, 0xb8, 0x2a, 0x10, 0x10];
    a.runtime
        .memory
        .map(
            GuestPtr(source),
            input.to_vec(),
            Permissions {
                read: true,
                write: false,
                execute: false,
            },
        )
        .unwrap();
    let expected = [0x40, 0x07, 0x92, 0x80, 0, 0, 0, 0, 0, 0]; // 292 minus a tiny positive value rounds to 292
    a.runtime
        .memory
        .write(GuestPtr(destination), &expected)
        .unwrap();
    a.runtime
        .memory
        .write(GuestPtr(STACK_BASE + 0x7006), &source.to_be_bytes())
        .unwrap();
    assert_eq!(a.runtime.advance(1).unwrap(), Stop::Budget);
    assert_eq!(
        a.runtime.memory.read(GuestPtr(destination), 10).unwrap(),
        expected
    );
    assert_eq!(a.runtime.memory.read(GuestPtr(source), 10).unwrap(), input);
    assert!(
        a.runtime.process.numerics.inexact
            && !a.runtime.process.numerics.invalid
            && !a.runtime.process.numerics.underflow
    );
    let Cpu::M68k(c) = &a.runtime.cpu else {
        panic!()
    };
    assert_eq!(c.dar[15], STACK_BASE + 0x700a);
    for i in 0..15 {
        assert_eq!(c.dar[i], 0x12340000 + i as u32);
    }
}
