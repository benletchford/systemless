mod support;
use systemless_future::{
    abi::Abi,
    color::Op,
    cpu::{Architecture, Cpu},
    load::{LoadedApplication, STACK_BASE},
    memory::{GuestPtr, Permissions},
    runtime::Stop,
};
const STACK: u32 = STACK_BASE + 0x8000;
fn app() -> LoadedApplication {
    let f = support::pef_program(&[0x4e800020], &[b"CTabChanged", b"GetCTSeed"], 32);
    let mut a = LoadedApplication::pef(&f.bytes, None).unwrap();
    a.runtime
        .memory
        .map(GuestPtr(0x50000000), vec![0xab, 0x1d], Permissions::RX)
        .unwrap();
    a
}
fn call(a: &mut LoadedApplication, op: Op) -> systemless_future::Result<u32> {
    let mut manager = std::mem::take(&mut a.runtime.process.color);
    let result = manager.call(&mut a.runtime.memory, &mut a.runtime.process, op);
    a.runtime.process.color = manager;
    result
}
fn table(a: &mut LoadedApplication) -> u32 {
    let h = a
        .runtime
        .process
        .new_handle(&mut a.runtime.memory, 24, true)
        .unwrap();
    let p = GuestPtr(a.runtime.memory.u32(GuestPtr(h)).unwrap());
    let mut bytes = [0u8; 24];
    bytes[..4].copy_from_slice(&0x12345678u32.to_be_bytes());
    bytes[7] = 1;
    bytes[10..16].fill(255);
    bytes[17] = 1;
    a.runtime.memory.write(p, &bytes).unwrap();
    h
}
#[test]
fn real_cpu_calls_change_only_seed_and_share_seed_authority() {
    let mut a = app();
    let h = table(&mut a);
    let data = GuestPtr(a.runtime.memory.u32(GuestPtr(h)).unwrap());
    let tail = a
        .runtime
        .memory
        .read(GuestPtr(data.0 + 4), 20)
        .unwrap()
        .to_vec();
    let free = a.runtime.process.free_memory();
    let mut previous = 0x12345678;
    for arch in [
        Architecture::M68k,
        Architecture::PowerPc,
        Architecture::M68k,
    ] {
        a.runtime.process.mem_error = -99;
        a.runtime.process.res_error = -98;
        a.runtime
            .memory
            .write(GuestPtr(STACK), &h.to_be_bytes())
            .unwrap();
        a.runtime
            .memory
            .write(GuestPtr(STACK + 4), &[0xa5; 4])
            .unwrap();
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
        a.runtime.cpu = Cpu::new(arch, pc, STACK);
        match &mut a.runtime.cpu {
            Cpu::M68k(c) => c.dar[0] = 0x00040007,
            Cpu::PowerPc(c) => c.gpr[3] = h,
        }
        assert_eq!(a.runtime.advance(1).unwrap(), Stop::Budget);
        match &a.runtime.cpu {
            Cpu::M68k(c) => {
                assert_eq!(c.dar[15], STACK + 4);
                assert_eq!(c.dar[0], 0x00040007);
            }
            Cpu::PowerPc(c) => assert_eq!(c.gpr[3], h),
        }
        assert_eq!(
            (a.runtime.process.mem_error, a.runtime.process.res_error),
            (-99, -98)
        );
        assert_eq!(
            a.runtime.memory.read(GuestPtr(STACK + 4), 4).unwrap(),
            [0xa5; 4]
        );
        let seed = a.runtime.memory.u32(data).unwrap();
        assert!(seed > previous);
        assert_eq!(
            a.runtime.memory.read(GuestPtr(data.0 + 4), 20).unwrap(),
            tail
        );
        previous = call(&mut a, Op::Seed).unwrap();
        assert!(previous > seed);
        assert_eq!(a.runtime.process.free_memory(), free);
    }
}
#[test]
fn invalid_counts_handles_and_exhausted_seed_are_atomic_and_preserve_errors() {
    let mut a = app();
    let h = table(&mut a);
    let data = GuestPtr(a.runtime.memory.u32(GuestPtr(h)).unwrap());
    for (seed, count) in [(77u32, 256u16), (u32::MAX, 1)] {
        a.runtime.memory.write(data, &seed.to_be_bytes()).unwrap();
        a.runtime
            .memory
            .write(GuestPtr(data.0 + 6), &count.to_be_bytes())
            .unwrap();
        let before = a.runtime.memory.read(data, 24).unwrap().to_vec();
        a.runtime.process.mem_error = -99;
        a.runtime.process.res_error = -98;
        assert!(call(&mut a, Op::Changed(h)).is_err());
        assert_eq!(a.runtime.memory.read(data, 24).unwrap(), before);
        assert_eq!(
            (a.runtime.process.mem_error, a.runtime.process.res_error),
            (-99, -98)
        );
    }
    assert!(call(&mut a, Op::Changed(0)).is_err());
    assert_eq!(call(&mut a, Op::Seed).unwrap(), 1024);
    // Empty tables are valid records too; no entry storage is required.
    a.runtime.memory.write(data, &12u32.to_be_bytes()).unwrap();
    a.runtime
        .memory
        .write(GuestPtr(data.0 + 6), &0xffffu16.to_be_bytes())
        .unwrap();
    call(&mut a, Op::Changed(h)).unwrap();
    assert_eq!(a.runtime.memory.u32(data).unwrap(), 1025);
}
#[test]
fn native_short_frame_and_unknown_selector_do_not_pop_arguments() {
    let mut m = systemless_future::memory::Memory::default();
    m.map(GuestPtr(0x1000), vec![0; 2], Permissions::RW)
        .unwrap();
    let mut cpu = Cpu::new(Architecture::M68k, 0, 0x1000);
    let Cpu::M68k(c) = &mut cpu else { panic!() };
    c.dar[0] = 0x00040007;
    assert!(Abi::trap(0xab1d, c, &m).is_err());
    assert_eq!(c.dar[15], 0x1000);
    c.dar[0] = 0x00040077;
    assert!(Abi::trap(0xab1d, c, &m).is_err());
    assert_eq!(c.dar[15], 0x1000);
}

#[test]
fn changed_palette_invalidates_inverse_seed_and_rebuilds_with_live_colors() {
    let mut a = app();
    let source = table(&mut a);
    let out = a
        .runtime
        .process
        .new_handle(&mut a.runtime.memory, 0, true)
        .unwrap();
    call(
        &mut a,
        Op::Make {
            source,
            destination: out,
            resolution: 3,
        },
    )
    .unwrap();
    let old = GuestPtr(a.runtime.memory.u32(GuestPtr(out)).unwrap());
    let seed = a.runtime.memory.u32(old).unwrap();
    assert_eq!(a.runtime.memory.read(GuestPtr(old.0 + 6), 1).unwrap(), [1]);
    let data = a.runtime.memory.u32(GuestPtr(source)).unwrap();
    // Swap black/white in guest memory, then report the direct change.
    a.runtime
        .memory
        .write(GuestPtr(data + 10), &[0; 6])
        .unwrap();
    a.runtime
        .memory
        .write(GuestPtr(data + 18), &[255; 6])
        .unwrap();
    call(&mut a, Op::Changed(source)).unwrap();
    let new_seed = a.runtime.memory.u32(GuestPtr(data)).unwrap();
    assert_ne!(seed, new_seed);
    // The stale inverse table is identifiable until explicitly regenerated.
    assert_eq!(a.runtime.memory.u32(old).unwrap(), seed);
    call(
        &mut a,
        Op::Make {
            source,
            destination: out,
            resolution: 3,
        },
    )
    .unwrap();
    let new = GuestPtr(a.runtime.memory.u32(GuestPtr(out)).unwrap());
    assert_eq!(a.runtime.memory.u32(new).unwrap(), new_seed);
    assert_eq!(a.runtime.memory.read(GuestPtr(new.0 + 6), 1).unwrap(), [0]);
    assert_eq!(
        a.runtime.memory.read(GuestPtr(new.0 + 517), 1).unwrap(),
        [1]
    );
    let address = systemless_future::traps::address(0xab1d, true);
    assert_ne!(address, systemless_future::traps::UNIMPLEMENTED);
    // Loading installs the canonical dispatch veneer for saved-trap calls.
    assert!(a.runtime.memory.read(GuestPtr(address), 2).is_ok());
}
