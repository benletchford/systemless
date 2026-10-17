mod support;
use systemless_future::{
    calendar::{self, TICKS, TIME},
    cpu::{Architecture, Cpu},
    load::{LoadedApplication, STACK_BASE},
    memory::{GuestPtr, Memory, Permissions},
    runtime::Stop,
    traps,
};
const CASES: &[(u32, [u16; 7])] = &[
    (0, [1904, 1, 1, 0, 0, 0, 6]),
    (1, [1904, 1, 1, 0, 0, 1, 6]),
    (86399, [1904, 1, 1, 23, 59, 59, 6]),
    (86400, [1904, 1, 2, 0, 0, 0, 7]),
    (5097599, [1904, 2, 28, 23, 59, 59, 1]),
    (5097600, [1904, 2, 29, 0, 0, 0, 2]),
    (5184000, [1904, 3, 1, 0, 0, 0, 3]),
    (2082844800, [1970, 1, 1, 0, 0, 0, 5]),
    (2147483647, [1972, 1, 19, 3, 14, 7, 4]),
    (2147483648, [1972, 1, 19, 3, 14, 8, 4]),
    (3029529600, [2000, 1, 1, 0, 0, 0, 7]),
    (3034627200, [2000, 2, 29, 0, 0, 0, 3]),
    (3029529619, [2000, 1, 1, 0, 0, 19, 7]),
    (u32::MAX, [2040, 2, 6, 6, 28, 15, 2]),
];
#[test]
fn epoch_leap_days_signed_boundary_and_maximum_match_independent_calendar_values() {
    // Expected records cross-checked with Python datetime (Gregorian proleptic calendar).
    for &(seconds, expected) in CASES {
        assert_eq!(calendar::date(seconds), expected);
    }
}
// Closed-form Gregorian ordinal, independent of the service's year/month iteration.
fn ordinal([year, month, day, ..]: [u16; 7]) -> u32 {
    let y = u32::from(year) - 1;
    let before = [0, 31, 59, 90, 120, 151, 181, 212, 243, 273, 304, 334];
    365 * y + y / 4 - y / 100 + y / 400 + before[usize::from(month) - 1] + u32::from(day) - 1
        + u32::from(
            month > 2
                && year.is_multiple_of(4)
                && (!year.is_multiple_of(100) || year.is_multiple_of(400)),
        )
}
#[test]
fn every_representable_day_start_and_end_has_the_correct_date_weekday_and_time() {
    let epoch = ordinal([1904, 1, 1, 0, 0, 0, 0]);
    for day in 0..=u32::MAX / 86400 {
        for seconds in [
            day * 86400,
            (u64::from(day) * 86400 + 86399).min(u64::from(u32::MAX)) as u32,
        ] {
            let record = calendar::date(seconds);
            assert!((1904..=2040).contains(&record[0]));
            assert!((1..=12).contains(&record[1]));
            assert!((1..=31).contains(&record[2]));
            assert_eq!(ordinal(record) - epoch, day);
            assert_eq!(record[6], ((ordinal(record) + 1) % 7 + 1) as u16);
            assert_eq!(
                u32::from(record[3]) * 3600 + u32::from(record[4]) * 60 + u32::from(record[5]),
                seconds % 86400
            );
        }
    }
}
#[test]
fn real_cpu_adapters_write_exact_records_preserve_registers_stack_clock_and_errors() {
    let f = support::pef_program(&[0x4e800020], &[b"SecondsToDate", b"Secs2Date"], 16);
    let mut a = LoadedApplication::pef(&f.bytes, None).unwrap();
    a.runtime
        .memory
        .map(GuestPtr(0x50000000), vec![0xa9, 0xc6], Permissions::RX)
        .unwrap();
    let mut gates = Vec::new();
    for offset in [8, 12] {
        let vector = a
            .runtime
            .memory
            .u32(GuestPtr(a.info.toc_or_a5 + offset))
            .unwrap();
        gates.push(a.runtime.memory.u32(GuestPtr(vector)).unwrap());
    }
    let stack = STACK_BASE + 0x7000;
    let pointer = GuestPtr(STACK_BASE + 0x6001);
    let free = a.runtime.process.free_memory();
    a.runtime.process.mem_error = -108;
    a.runtime.process.res_error = -192;
    a.runtime
        .memory
        .write(TIME, &123456u32.to_be_bytes())
        .unwrap();
    a.runtime
        .memory
        .write(TICKS, &100u32.to_be_bytes())
        .unwrap();
    for (seconds, expected) in CASES {
        for gate in std::iter::once(0x50000000).chain(gates.iter().copied()) {
            a.runtime
                .memory
                .write(GuestPtr(pointer.0 - 1), &[0x5a; 16])
                .unwrap();
            a.runtime
                .memory
                .write(GuestPtr(stack), &[0xcc; 16])
                .unwrap();
            let arch = if gate == 0x50000000 {
                Architecture::M68k
            } else {
                Architecture::PowerPc
            };
            a.runtime.cpu = Cpu::new(arch, gate, stack);
            match &mut a.runtime.cpu {
                Cpu::M68k(c) => {
                    for i in 0..15 {
                        c.dar[i] = 0x12340000 + i as u32;
                    }
                    c.dar[0] = *seconds;
                    c.dar[8] = pointer.0;
                }
                Cpu::PowerPc(c) => {
                    c.gpr[3] = *seconds;
                    c.gpr[4] = pointer.0;
                    c.gpr[5] = 0xaabbccdd;
                }
            }
            assert_eq!(a.runtime.advance(1).unwrap(), Stop::Budget);
            match &a.runtime.cpu {
                Cpu::M68k(c) => {
                    assert_eq!(c.dar[15], stack);
                    for i in 0..15 {
                        assert_eq!(
                            c.dar[i],
                            if i == 0 {
                                *seconds
                            } else if i == 8 {
                                pointer.0
                            } else {
                                0x12340000 + i as u32
                            }
                        );
                    }
                }
                Cpu::PowerPc(c) => assert_eq!(
                    (c.gpr[3], c.gpr[4], c.gpr[5]),
                    (*seconds, pointer.0, 0xaabbccdd)
                ),
            }
            let bytes: Vec<_> = expected.iter().flat_map(|n| n.to_be_bytes()).collect();
            assert_eq!(a.runtime.memory.read(pointer, 14).unwrap(), bytes);
            assert_eq!(
                a.runtime.memory.read(GuestPtr(pointer.0 - 1), 1).unwrap(),
                [0x5a]
            );
            assert_eq!(
                a.runtime.memory.read(GuestPtr(pointer.0 + 14), 1).unwrap(),
                [0x5a]
            );
            assert_eq!(
                a.runtime.memory.read(GuestPtr(stack), 16).unwrap(),
                [0xcc; 16]
            );
            // A single service boundary does not advance the shared calendar clock.
            assert_eq!(a.runtime.memory.u32(TIME).unwrap(), 123456);
        }
    }
    assert_eq!(a.runtime.process.free_memory(), free);
    assert_eq!(
        (a.runtime.process.mem_error, a.runtime.process.res_error),
        (-108, -192)
    );
    let entry = traps::lookup(&a.runtime.memory, 0x1c6, true).unwrap();
    assert_ne!(entry, traps::UNIMPLEMENTED);
    assert_eq!(a.runtime.memory.u16(GuestPtr(entry)).unwrap(), 0xa9c6);
}
#[test]
fn nil_readonly_and_short_records_fail_without_partial_writes() {
    for (address, size, permissions) in [
        (0, 14, Permissions::RW),
        (0x1000, 14, Permissions::RX),
        (0x2000, 13, Permissions::RW),
    ] {
        let mut m = Memory::default();
        m.map(GuestPtr(address), vec![0x5a; size], permissions)
            .unwrap();
        assert!(calendar::seconds_to_date(&mut m, u32::MAX, GuestPtr(address)).is_err());
        assert_eq!(m.read(GuestPtr(address), size).unwrap(), vec![0x5a; size]);
    }
}
