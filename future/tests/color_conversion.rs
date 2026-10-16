mod support;
use systemless_future::{
    abi::Abi,
    color,
    cpu::{Architecture, Cpu},
    load::{LoadedApplication, STACK_BASE},
    memory::{GuestPtr, Permissions},
    runtime::Stop,
};
const DATA: u32 = 0x50000000;
const CODE: u32 = 0x51000000;
const STACK: u32 = STACK_BASE + 0x8000;
fn app() -> LoadedApplication {
    let fixture = support::pef_program(&[0x4e800020], &[b"RGB2HSL", b"QDError"], 32);
    let mut a = LoadedApplication::pef(&fixture.bytes, None).unwrap();
    a.runtime
        .memory
        .map(GuestPtr(DATA), vec![0xa5; 64], Permissions::RW)
        .unwrap();
    a.runtime
        .memory
        .map(GuestPtr(CODE), vec![0xa8, 0x2e], Permissions::RX)
        .unwrap();
    a
}
fn call(a: &mut LoadedApplication, arch: Architecture, source: u32, destination: u32) {
    a.runtime.process.mem_error = -99;
    a.runtime.process.res_error = -98;
    let pc = if arch == Architecture::M68k {
        let mut frame = 6u16.to_be_bytes().to_vec();
        frame.extend(destination.to_be_bytes());
        frame.extend(source.to_be_bytes());
        frame.extend([0x5a; 4]);
        a.runtime.memory.write(GuestPtr(STACK), &frame).unwrap();
        CODE
    } else {
        let vector = a
            .runtime
            .memory
            .u32(GuestPtr(a.info.toc_or_a5 + 8))
            .unwrap();
        a.runtime.memory.u32(GuestPtr(vector)).unwrap()
    };
    a.runtime.cpu = Cpu::new(arch, pc, STACK);
    match &mut a.runtime.cpu {
        Cpu::M68k(c) => c.dar[0] = 0x12345678,
        Cpu::PowerPc(c) => {
            c.gpr[3] = source;
            c.gpr[4] = destination;
        }
    }
    assert_eq!(a.runtime.advance(1).unwrap(), Stop::Budget);
    assert_eq!(
        (a.runtime.process.mem_error, a.runtime.process.res_error),
        (-99, -98)
    );
    match &a.runtime.cpu {
        Cpu::M68k(c) => {
            assert_eq!(c.dar[15], STACK + 10);
            assert_eq!(c.dar[0], 0x12345678);
            assert_eq!(
                a.runtime.memory.read(GuestPtr(STACK + 10), 4).unwrap(),
                [0x5a; 4]
            );
        }
        Cpu::PowerPc(c) => assert_eq!(&c.gpr[3..5], &[source, destination]),
    }
}
fn run(a: &mut LoadedApplication, arch: Architecture, rgb: [u16; 3], offset: u32) -> [u16; 3] {
    a.runtime.memory.write(GuestPtr(DATA), &[0xa5; 64]).unwrap();
    let input: Vec<_> = rgb.into_iter().flat_map(u16::to_be_bytes).collect();
    a.runtime.memory.write(GuestPtr(DATA + 4), &input).unwrap();
    let before = a.runtime.memory.read(GuestPtr(DATA), 64).unwrap().to_vec();
    call(a, arch, DATA + 4, DATA + offset);
    let data = a.runtime.memory.read(GuestPtr(DATA), 64).unwrap();
    for i in 0..64 {
        if !(offset as usize..offset as usize + 6).contains(&i) {
            assert_eq!(data[i], before[i]);
        }
    }
    std::array::from_fn(|i| {
        u16::from_be_bytes([
            data[offset as usize + i * 2],
            data[offset as usize + i * 2 + 1],
        ])
    })
}
#[test]
fn documented_hues_endpoints_rounding_and_overlaps_are_shared_by_both_cpus() {
    let cases = [
        ([0, 0, 0], [0, 0, 0]),
        ([65535; 3], [0, 0, 65535]),
        ([65535, 0, 0], [0, 65535, 32768]),
        ([0, 65535, 0], [21845, 65535, 32768]),
        ([0, 0, 65535], [43690, 65535, 32768]),
        ([65535, 65535, 0], [10923, 65535, 32768]),
        ([0, 65535, 65535], [32768, 65535, 32768]),
        ([65535, 0, 65535], [54613, 65535, 32768]),
        ([32768, 0, 0], [0, 65535, 16384]),
        ([32768, 32767, 32767], [0, 1, 32768]),
        ([0, 1, 0], [21845, 65535, 1]),
        ([65535, 65534, 65534], [0, 65535, 65535]),
    ];
    for arch in [Architecture::M68k, Architecture::PowerPc] {
        let mut a = app();
        let free = a.runtime.process.free_memory();
        for (rgb, hsl) in cases {
            for offset in [4, 6, 16] {
                assert_eq!(run(&mut a, arch, rgb, offset), hsl);
            }
        }
        for gray in [1, 2, 0x1234, 0x7fff, 0x8000, 0xfffe] {
            assert_eq!(run(&mut a, arch, [gray; 3], 16), [0, 0, gray]);
        }
        assert_eq!(a.runtime.process.free_memory(), free);
    }
}
#[test]
fn conversion_preserves_qderror_and_faults_without_output_mutation() {
    let mut a = app();
    let mut manager = std::mem::take(&mut a.runtime.process.color);
    manager
        .call(
            &mut a.runtime.memory,
            &mut a.runtime.process,
            color::Op::Make {
                source: 0,
                destination: 0,
                resolution: 2,
            },
        )
        .unwrap();
    assert_eq!(
        manager
            .call(
                &mut a.runtime.memory,
                &mut a.runtime.process,
                color::Op::Error
            )
            .unwrap() as i32,
        -156
    );
    a.runtime.process.color = manager;
    for arch in [Architecture::M68k, Architecture::PowerPc] {
        assert_eq!(run(&mut a, arch, [65535, 0, 0], 16), [0, 65535, 32768]);
        let mut manager = std::mem::take(&mut a.runtime.process.color);
        assert_eq!(
            manager
                .call(
                    &mut a.runtime.memory,
                    &mut a.runtime.process,
                    color::Op::Error
                )
                .unwrap() as i32,
            -156
        );
        let before = a.runtime.memory.read(GuestPtr(DATA), 64).unwrap().to_vec();
        for (source, destination) in [
            (GuestPtr(0), GuestPtr(DATA + 16)),
            (GuestPtr(0xdead0000), GuestPtr(DATA + 16)),
            (GuestPtr(DATA + 4), GuestPtr(0)),
        ] {
            assert!(manager
                .call(
                    &mut a.runtime.memory,
                    &mut a.runtime.process,
                    color::Op::RgbToHsl {
                        source,
                        destination
                    }
                )
                .is_err());
            assert_eq!(a.runtime.memory.read(GuestPtr(DATA), 64).unwrap(), before);
        }
        a.runtime.process.color = manager;
    }
}
#[test]
fn native_adapter_preflights_records_before_popping_the_selector_and_arguments() {
    let mut a = app();
    a.runtime
        .memory
        .map(GuestPtr(0x52000000), vec![0; 6], Permissions::RX)
        .unwrap();
    for (selector, source, destination) in [
        (6u16, DATA + 4, 0x52000000u32),
        (6, 0xdead0000, DATA + 16),
        (5, DATA + 4, DATA + 16),
    ] {
        let mut frame = selector.to_be_bytes().to_vec();
        frame.extend(destination.to_be_bytes());
        frame.extend(source.to_be_bytes());
        a.runtime.memory.write(GuestPtr(STACK), &frame).unwrap();
        let Cpu::M68k(mut c) = Cpu::new(Architecture::M68k, CODE, STACK) else {
            unreachable!()
        };
        assert!(Abi::trap(0xa82e, &mut c, &a.runtime.memory).is_err());
        assert_eq!(c.dar[15], STACK);
    }
}
