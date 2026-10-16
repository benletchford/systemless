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
                if [4, 6, 0x000e, 0x0810, 0x080e, 0x200e, 0x280e].contains(&selector) {
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
