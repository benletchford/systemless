use super::{globals::LowMemGlobals, GuestAddressSpace, MacMemoryBus, MemoryBus};
use m68k::{BatchExit, CpuCore, CpuType, FastMemRegion, MAX_FAST_MEM_REGIONS};

// Exercise the browser's region contract on the portable core in native tests,
// without enabling the native writable/ tracked-JIT memory paths.
struct ReadRegions<'a> {
    bus: &'a mut MacMemoryBus,
    enabled: bool,
}

impl m68k::AddressBus for ReadRegions<'_> {
    fn read_byte(&mut self, a: u32) -> u8 {
        MemoryBus::read_byte(self.bus, a)
    }
    fn read_word(&mut self, a: u32) -> u16 {
        MemoryBus::read_word(self.bus, a)
    }
    fn read_long(&mut self, a: u32) -> u32 {
        MemoryBus::read_long(self.bus, a)
    }
    fn write_byte(&mut self, a: u32, v: u8) {
        MemoryBus::write_byte(self.bus, a, v);
    }
    fn write_word(&mut self, a: u32, v: u16) {
        MemoryBus::write_word(self.bus, a, v);
    }
    fn write_long(&mut self, a: u32, v: u32) {
        MemoryBus::write_long(self.bus, a, v);
    }
    fn fast_mem_regions(&mut self) -> [Option<FastMemRegion>; MAX_FAST_MEM_REGIONS] {
        let mut regions = [None; MAX_FAST_MEM_REGIONS];
        if self.enabled {
            regions[0] = self.bus.portable_read_region();
        }
        regions
    }
}

fn cpu() -> CpuCore {
    let mut cpu = CpuCore::new();
    cpu.set_cpu_type(CpuType::M68040);
    cpu.pc = 0x200;
    cpu.set_a(0, 0x1000);
    cpu.set_a(1, 0x2000);
    cpu
}

fn program(bus: &mut MacMemoryBus, words: &[u16]) {
    for (i, &word) in words.iter().enumerate() {
        bus.write_word(0x200 + i as u32 * 2, word);
    }
}

#[test]
fn portable_reads_keep_stores_rmw_protection_and_presentation_on_the_bus() {
    for protected in [false, true] {
        let mut results = Vec::new();
        for enabled in [false, true] {
            let mut bus = MacMemoryBus::new(64 * 1024);
            // MOVE.L (A0),D1; MOVE.L D1,(A1); ADDQ.L #1,(A1); MOVE.L (A1),D2.
            program(&mut bus, &[0x2210, 0x2281, 0x5291, 0x2411]);
            bus.write_long(0x1000, 0x11223344);
            bus.write_long(0x2000, 0x55aa0055);
            bus.enable_outline_presentation(
                (0x2000, 8, 8, 8, 8),
                std::array::from_fn(|i| [i as u8; 3]),
                2,
            );
            if protected {
                bus.protect_readonly_code(0x2000, 4);
            }
            assert!(bus.fast_mem_window().is_none());
            let region = bus.portable_read_region().unwrap();
            assert!(!region.writable);
            let epoch = bus.presentation_epoch();
            bus.begin_write_probe();
            bus.begin_cpu_drawing();
            let mut cpu = cpu();
            let result = cpu.run_batch(
                &mut ReadRegions {
                    bus: &mut bus,
                    enabled,
                },
                4,
                &[],
            );
            bus.end_cpu_drawing(true);
            assert_eq!(result.instructions, 4);
            assert_eq!(result.exit, BatchExit::BudgetExhausted);
            let unchanged = bus.finish_write_probe_unchanged();
            let expected = if protected { 0x55aa0055 } else { 0x11223345 };
            assert_eq!(bus.read_long(0x2000), expected);
            assert_eq!(cpu.d(2), expected);
            assert_eq!(unchanged, protected);
            assert_eq!(bus.presentation_epoch() == epoch, protected);
            results.push((cpu.dar, cpu.get_sr(), cpu.pc, bus.read_bytes(0x2000, 64)));
        }
        assert_eq!(results[0], results[1]);
    }
}

#[test]
fn portable_reads_withdraw_for_translation_mappings_and_access_observation() {
    let mut bus = MacMemoryBus::new(64 * 1024);
    assert!(bus.portable_read_region().is_some());
    bus.set_addressing_32_bit(false);
    assert!(bus.portable_read_region().is_none());
    bus.set_addressing_32_bit(true);
    let memory = GuestAddressSpace::new();
    bus.attach_guest_address_space(memory.shared_view());
    assert!(bus.portable_read_region().is_none());
    bus.detach_guest_address_space();
    program(&mut bus, &[0x2210]); // MOVE.L (A0),D1.
    bus.write_long(0x1000, 0x12345678);
    bus.begin_write_probe();
    bus.arm_access_watch(0x1000, 0x1004);
    assert!(bus.portable_read_region().is_none());
    let mut cpu = cpu();
    assert_eq!(
        cpu.run_batch(
            &mut ReadRegions {
                bus: &mut bus,
                enabled: true
            },
            1,
            &[]
        )
        .instructions,
        1
    );
    let hits = bus.take_access_watch().unwrap();
    assert!(hits
        .iter()
        .any(|hit| !hit.write && hit.address == 0x1000 && hit.len == 4));
    assert_eq!(cpu.d(1), 0x12345678);
    assert!(bus.finish_write_probe_unchanged());
    assert!(bus.portable_read_region().is_some());
    let _shared = bus.shared_ram_region(0, 64 * 1024).unwrap();
    assert!(bus.portable_read_region().is_none());
}

#[test]
fn portable_reads_use_stable_external_ram_without_owning_or_copying_it() {
    let mut ram = vec![0u8; 64 * 1024];
    // SAFETY: the allocation remains stable and outlives the bus; access is serial.
    let mut bus =
        unsafe { MacMemoryBus::wrap_external(ram.as_mut_ptr(), ram.len(), LowMemGlobals::new()) };
    let region = bus.portable_read_region().unwrap();
    assert_eq!(region.ptr, ram.as_mut_ptr());
    assert_eq!(region.len, ram.len() as u32);
    assert!(!region.writable);
    bus.write_long(0x1000, 0x12345678);
    assert_eq!(&ram[0x1000..0x1004], &[0x12, 0x34, 0x56, 0x78]);
}

#[test]
#[ignore = "local host throughput diagnostic, not game/browser pacing"]
fn portable_read_region_throughput() {
    for enabled in [false, true, false, true] {
        let mut bus = MacMemoryBus::new(64 * 1024);
        // MOVE.L (A0),D1; ADD.L (A0),D1; MOVE.L D1,(A1); BRA.S loop.
        program(&mut bus, &[0x2210, 0xd290, 0x2281, 0x60f8]);
        bus.write_long(0x1000, 0x10203040);
        let mut cpu = cpu();
        // Warm both paths equally before timing; repeat in alternating order
        // so shared portable trace caches cannot give only the second case a head start.
        assert_eq!(
            cpu.run_batch(
                &mut ReadRegions {
                    bus: &mut bus,
                    enabled
                },
                400_000,
                &[]
            )
            .instructions,
            400_000,
        );
        let start = std::time::Instant::now();
        let result = cpu.run_batch(
            &mut ReadRegions {
                bus: &mut bus,
                enabled,
            },
            4_000_000,
            &[],
        );
        eprintln!(
            "portable_read_region enabled={enabled} elapsed_us={}",
            start.elapsed().as_micros()
        );
        assert_eq!(result.instructions, 4_000_000);
        assert_eq!(cpu.pc, 0x200);
        assert_eq!(cpu.d(1), 0x20406080);
        assert_eq!(bus.read_long(0x2000), 0x20406080);
    }
}
