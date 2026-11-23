//! Application-created QuickTime time bases (Inside Macintosh: QuickTime,
//! "Time Base Functions"). A time base is a clock with a rate, an optional
//! master time base, and optional start and stop limits. The movie's own time
//! base keeps its dedicated token; these records cover `NewTimeBase` clocks.

use super::callbacks::{Callbacks, Event};
use super::{PpcCpu, PpcImportAction, PpcSectionMem};
use crate::process_context::ProcessNativeMemoryManager;
use ppc::PpcMemory;

/// Subticks per second for the PowerPC QuickTime clock (60 ticks).
const SUBTICKS_PER_SECOND: i128 = 60 * crate::time_manager::SUBTICKS_PER_TICK as i128;
const DEFAULT_SCALE: u32 = 600;
const LOOP_TIME_BASE: u32 = 1;
const PALINDROME_LOOP_TIME_BASE: u32 = 2;
const MAX_MASTER_DEPTH: usize = 8;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Operation {
    NewTimeBase,
    DisposeTimeBase,
    GetTimeBaseTime,
    SetTimeBaseTime,
    SetTimeBaseValue,
    GetTimeBaseRate,
    SetTimeBaseRate,
    GetTimeBaseEffectiveRate,
    GetTimeBaseStartTime,
    SetTimeBaseStartTime,
    GetTimeBaseStopTime,
    SetTimeBaseStopTime,
    GetTimeBaseFlags,
    SetTimeBaseMasterTimeBase,
    GetTimeBaseMasterTimeBase,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Record {
    address: u32,
    /// Time at `anchor`, in `scale` units, kept as a fraction over
    /// `SUBTICKS_PER_SECOND << 16` so repeated reads never drift.
    value: i128,
    scale: u32,
    anchor: u64,
    rate: i32,
    flags: u32,
    master: u32,
    start: Option<(i64, u32)>,
    stop: Option<(i64, u32)>,
    /// Time (in `scale`) when callbacks were last serviced.
    serviced: Option<i64>,
    at_start: bool,
    at_stop: bool,
}

const FRACTION: i128 = SUBTICKS_PER_SECOND << 16;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct TimeBases {
    records: Vec<Record>,
}

fn rescale(value: i128, from: u32, to: u32) -> i128 {
    if from == to || from == 0 {
        value
    } else {
        value * i128::from(to) / i128::from(from)
    }
}

fn clamp_i32(value: i128) -> u32 {
    value.clamp(i128::from(i32::MIN), i128::from(i32::MAX)) as i32 as u32
}

impl TimeBases {
    fn index(&self, address: u32) -> Option<usize> {
        if address == 0 {
            return None;
        }
        self.records
            .iter()
            .position(|record| record.address == address)
    }

    pub fn contains(&self, address: u32) -> bool {
        self.index(address).is_some()
    }

    pub fn set_flags(&mut self, address: u32, flags: u32, now: u64) -> bool {
        let Some(index) = self.index(address) else {
            return false;
        };
        self.reanchor_all(now);
        self.records[index].flags = flags;
        true
    }

    fn effective_rate(&self, address: u32) -> i32 {
        let mut rate: i64 = 0x1_0000;
        let mut current = address;
        for _ in 0..MAX_MASTER_DEPTH {
            let Some(index) = self.index(current) else {
                break;
            };
            let record = &self.records[index];
            rate = (rate * i64::from(record.rate)) >> 16;
            if record.master == 0 {
                return rate.clamp(i64::from(i32::MIN), i64::from(i32::MAX)) as i32;
            }
            current = record.master;
        }
        // An unknown or cyclic master stops the clock.
        0
    }

    /// Raw (unclamped) time at `now`, in scaled fractional units.
    fn raw_time(&self, index: usize, now: u64) -> i128 {
        let record = &self.records[index];
        let elapsed = i128::from(now.saturating_sub(record.anchor));
        record.value
            + elapsed * i128::from(self.effective_rate(record.address)) * i128::from(record.scale)
    }

    /// Time at `now` in the record's own scale, honouring start/stop limits
    /// and the loop flags.
    fn time(&self, index: usize, now: u64) -> i128 {
        let record = &self.records[index];
        let time = self.raw_time(index, now) / FRACTION;
        let start = record
            .start
            .map(|(value, scale)| rescale(i128::from(value), scale, record.scale));
        let stop = record
            .stop
            .map(|(value, scale)| rescale(i128::from(value), scale, record.scale));
        match (start, stop) {
            (Some(start), Some(stop))
                if stop > start
                    && record.flags & (LOOP_TIME_BASE | PALINDROME_LOOP_TIME_BASE) != 0 =>
            {
                let span = stop - start;
                let offset = time - start;
                if record.flags & PALINDROME_LOOP_TIME_BASE != 0 {
                    let phase = offset.rem_euclid(2 * span);
                    if phase <= span {
                        start + phase
                    } else {
                        stop - (phase - span)
                    }
                } else {
                    start + offset.rem_euclid(span)
                }
            }
            _ => {
                let time = start.map_or(time, |start| time.max(start));
                stop.map_or(time, |stop| time.min(stop))
            }
        }
    }

    fn reanchor_all(&mut self, now: u64) {
        let values: Vec<i128> = (0..self.records.len())
            .map(|index| self.time(index, now) * FRACTION)
            .collect();
        for (record, value) in self.records.iter_mut().zip(values) {
            record.value = value;
            record.anchor = now;
        }
    }

    fn create(&mut self, address: u32, now: u64) {
        self.records.push(Record {
            address,
            value: 0,
            scale: DEFAULT_SCALE,
            anchor: now,
            rate: 0,
            flags: 0,
            master: 0,
            start: None,
            stop: None,
            serviced: None,
            at_start: false,
            at_stop: false,
        });
    }

    fn get_time(&self, index: usize, scale: u32, now: u64) -> i64 {
        let record = &self.records[index];
        let time = rescale(self.time(index, now), record.scale, scale.max(1));
        time.clamp(i128::from(i64::MIN), i128::from(i64::MAX)) as i64
    }

    fn set_time(&mut self, index: usize, value: i64, scale: u32, now: u64) {
        self.reanchor_all(now);
        let record = &mut self.records[index];
        record.scale = if scale == 0 { DEFAULT_SCALE } else { scale };
        record.value = i128::from(value) * FRACTION;
        record.anchor = now;
        record.serviced = None;
    }

    /// Marks QTCallBacks on these time bases that became due since the last
    /// service: time triggers passed, and arrival at the start or stop time
    /// while running toward it (Inside Macintosh: QuickTime, CallMeWhen).
    pub fn service(&mut self, callbacks: &mut Callbacks, now: u64) {
        for index in 0..self.records.len() {
            let time = self.time(index, now);
            let rate = self.effective_rate(self.records[index].address);
            let record = &self.records[index];
            let limit = |limit: Option<(i64, u32)>| {
                limit.map(|(value, scale)| rescale(i128::from(value), scale, record.scale))
            };
            let looping = record.flags & (LOOP_TIME_BASE | PALINDROME_LOOP_TIME_BASE) != 0;
            let at_stop =
                !looping && rate > 0 && limit(record.stop).is_some_and(|stop| time >= stop);
            let at_start =
                !looping && rate < 0 && limit(record.start).is_some_and(|start| time <= start);
            let time = time.clamp(i128::from(i64::MIN), i128::from(i64::MAX)) as i64;
            if let Some(before) = record.serviced {
                callbacks.mark_time_due(record.address, before, time, record.scale);
            }
            if (at_stop && !record.at_stop) || (at_start && !record.at_start) {
                callbacks.mark_event(
                    record.address,
                    Event::Extremes {
                        start: at_start && !record.at_start,
                        stop: at_stop && !record.at_stop,
                    },
                );
            }
            let record = &mut self.records[index];
            record.serviced = Some(time);
            record.at_stop = at_stop;
            record.at_start = at_start;
        }
    }
}

fn write_time_record(memory: &mut PpcSectionMem, record: u32, value: i64, scale: u32, base: u32) {
    if record != 0 {
        let _ = memory.write_u64_be(record, value as u64);
        let _ = memory.write_u32_be(record + 8, scale);
        let _ = memory.write_u32_be(record + 12, base);
    }
}

fn read_time_record(memory: &mut PpcSectionMem, record: u32) -> Option<(i64, u32)> {
    if record == 0 {
        return None;
    }
    let value = memory.read_u64_be(record)? as i64;
    let scale = memory.read_u32_be(record + 8)?;
    Some((value, scale))
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn dispatch(
    operation: Operation,
    cpu: &PpcCpu,
    memory: &mut PpcSectionMem,
    process_memory_manager: &mut ProcessNativeMemoryManager,
    heap_cursor: &mut u32,
    last_mem_error: &mut i16,
    bases: &mut TimeBases,
    callbacks: &mut Callbacks,
    now: u64,
) -> PpcImportAction {
    bases.service(callbacks, now);
    let rates_before: Vec<(u32, i32)> = bases
        .records
        .iter()
        .map(|record| (record.address, bases.effective_rate(record.address)))
        .collect();
    let action = dispatch_operation(
        operation,
        cpu,
        memory,
        process_memory_manager,
        heap_cursor,
        last_mem_error,
        bases,
        callbacks,
        now,
    );
    for (address, before) in rates_before {
        if !bases.contains(address) {
            continue;
        }
        let after = bases.effective_rate(address);
        if before != after {
            callbacks.mark_event(address, Event::Rate { before, after });
        }
    }
    if !matches!(operation, Operation::DisposeTimeBase) {
        bases.service(callbacks, now);
    }
    action
}

#[allow(clippy::too_many_arguments)]
fn dispatch_operation(
    operation: Operation,
    cpu: &PpcCpu,
    memory: &mut PpcSectionMem,
    process_memory_manager: &mut ProcessNativeMemoryManager,
    heap_cursor: &mut u32,
    last_mem_error: &mut i16,
    bases: &mut TimeBases,
    callbacks: &mut Callbacks,
    now: u64,
) -> PpcImportAction {
    let base = cpu.gpr[3];
    if operation == Operation::NewTimeBase {
        // The TimeBase is opaque; an owned native allocation provides a
        // unique, valid guest pointer for its identity.
        let address = process_memory_manager.new_native_ptr(memory, 16, true);
        super::super::ppc_apply_process_native_allocator(
            process_memory_manager,
            memory,
            heap_cursor,
            last_mem_error,
        );
        if address != 0 {
            bases.create(address, now);
        }
        return PpcImportAction::Return(address);
    }
    let Some(index) = bases.index(base) else {
        return match operation {
            Operation::DisposeTimeBase
            | Operation::SetTimeBaseTime
            | Operation::SetTimeBaseValue
            | Operation::SetTimeBaseRate
            | Operation::SetTimeBaseStartTime
            | Operation::SetTimeBaseStopTime
            | Operation::SetTimeBaseMasterTimeBase => PpcImportAction::ReturnPreserve,
            _ => PpcImportAction::Return(0),
        };
    };
    match operation {
        Operation::NewTimeBase => unreachable!(),
        Operation::DisposeTimeBase => {
            bases.reanchor_all(now);
            bases.records.remove(index);
            callbacks.mark_event(base, Event::Disposed);
            // Slaves of a disposed master revert to the system clock.
            for record in &mut bases.records {
                if record.master == base {
                    record.master = 0;
                }
            }
            process_memory_manager.dispose_native_ptr(base);
            super::super::ppc_apply_process_native_allocator(
                process_memory_manager,
                memory,
                heap_cursor,
                last_mem_error,
            );
            PpcImportAction::ReturnPreserve
        }
        Operation::GetTimeBaseTime => {
            let scale = cpu.gpr[4];
            let scale = if scale as i32 > 0 {
                scale
            } else {
                bases.records[index].scale
            };
            let time = bases.get_time(index, scale, now);
            write_time_record(memory, cpu.gpr[5], time, scale, base);
            PpcImportAction::Return(clamp_i32(i128::from(time)))
        }
        Operation::SetTimeBaseTime => {
            if let Some((value, scale)) = read_time_record(memory, cpu.gpr[4]) {
                bases.set_time(index, value, scale, now);
                callbacks.mark_event(base, Event::Jump);
            }
            PpcImportAction::ReturnPreserve
        }
        Operation::SetTimeBaseValue => {
            bases.set_time(index, cpu.gpr[4] as i32 as i64, cpu.gpr[5], now);
            callbacks.mark_event(base, Event::Jump);
            PpcImportAction::ReturnPreserve
        }
        Operation::GetTimeBaseRate => PpcImportAction::Return(bases.records[index].rate as u32),
        Operation::SetTimeBaseRate => {
            bases.reanchor_all(now);
            bases.records[index].rate = cpu.gpr[4] as i32;
            PpcImportAction::ReturnPreserve
        }
        Operation::GetTimeBaseEffectiveRate => {
            PpcImportAction::Return(bases.effective_rate(base) as u32)
        }
        Operation::GetTimeBaseStartTime | Operation::GetTimeBaseStopTime => {
            let record = &bases.records[index];
            let limit = if operation == Operation::GetTimeBaseStartTime {
                record.start
            } else {
                record.stop
            };
            let scale = if cpu.gpr[4] as i32 > 0 {
                cpu.gpr[4]
            } else {
                record.scale
            };
            let value = limit.map_or(0, |(value, from)| {
                rescale(i128::from(value), from, scale)
                    .clamp(i128::from(i64::MIN), i128::from(i64::MAX)) as i64
            });
            write_time_record(memory, cpu.gpr[5], value, scale, base);
            PpcImportAction::Return(clamp_i32(i128::from(value)))
        }
        Operation::SetTimeBaseStartTime | Operation::SetTimeBaseStopTime => {
            if let Some((value, scale)) = read_time_record(memory, cpu.gpr[4]) {
                bases.reanchor_all(now);
                let scale = if scale == 0 {
                    bases.records[index].scale
                } else {
                    scale
                };
                let record = &mut bases.records[index];
                if operation == Operation::SetTimeBaseStartTime {
                    record.start = Some((value, scale));
                } else {
                    record.stop = Some((value, scale));
                }
            }
            PpcImportAction::ReturnPreserve
        }
        Operation::GetTimeBaseFlags => PpcImportAction::Return(bases.records[index].flags),
        Operation::SetTimeBaseMasterTimeBase => {
            let master = cpu.gpr[4];
            if master == base || (master != 0 && !bases.contains(master)) {
                return PpcImportAction::ReturnPreserve;
            }
            bases.reanchor_all(now);
            bases.records[index].master = master;
            // slaveZero: the master time that corresponds to this base's
            // zero. The slave's time becomes master time - slaveZero.
            if master != 0 {
                if let Some((zero, zero_scale)) = read_time_record(memory, cpu.gpr[5]) {
                    let master_index = bases.index(master).unwrap();
                    let scale = bases.records[index].scale;
                    let master_time = bases.get_time(master_index, scale, now);
                    let zero = rescale(i128::from(zero), zero_scale, scale) as i64;
                    bases.records[index].value = i128::from(master_time - zero) * FRACTION;
                }
            }
            PpcImportAction::ReturnPreserve
        }
        Operation::GetTimeBaseMasterTimeBase => {
            PpcImportAction::Return(bases.records[index].master)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SECOND: u64 = SUBTICKS_PER_SECOND as u64;

    fn base_with(rate: i32) -> (TimeBases, usize) {
        let mut bases = TimeBases::default();
        bases.create(0x1000, 0);
        bases.records[0].rate = rate;
        (bases, 0)
    }

    #[test]
    fn a_new_time_base_is_stopped_at_zero() {
        let (bases, index) = base_with(0);
        assert_eq!(bases.get_time(index, 600, 10 * SECOND), 0);
        assert_eq!(bases.effective_rate(0x1000), 0);
    }

    #[test]
    fn time_advances_at_the_rate_in_the_requested_scale() {
        let (mut bases, index) = base_with(0x1_0000);
        assert_eq!(bases.get_time(index, 600, 2 * SECOND), 1200);
        assert_eq!(bases.get_time(index, 30, 2 * SECOND), 60);
        bases.reanchor_all(2 * SECOND);
        bases.records[index].rate = 0x2_0000;
        assert_eq!(bases.get_time(index, 600, 3 * SECOND), 1200 + 1200);
    }

    #[test]
    fn set_value_restarts_from_the_new_time() {
        let (mut bases, index) = base_with(0x1_0000);
        bases.set_time(index, 100, 10, SECOND);
        assert_eq!(bases.get_time(index, 10, SECOND), 100);
        assert_eq!(bases.get_time(index, 10, 2 * SECOND), 110);
    }

    #[test]
    fn the_stop_time_holds_the_clock_unless_looping() {
        let (mut bases, index) = base_with(0x1_0000);
        bases.records[index].start = Some((0, 600));
        bases.records[index].stop = Some((600, 600));
        assert_eq!(bases.get_time(index, 600, 5 * SECOND), 600);
        bases.records[index].flags = LOOP_TIME_BASE;
        assert_eq!(bases.get_time(index, 600, SECOND + SECOND / 2), 300);
    }

    #[test]
    fn reaching_the_stop_time_marks_an_extremes_callback_once() {
        use super::super::callbacks::Registration;
        let (mut bases, index) = base_with(0x1_0000);
        bases.records[index].stop = Some((600, 600));
        let mut callbacks = Callbacks::default();
        assert!(callbacks.create(0x5000, 0x1000, 4));
        let registration = Registration {
            procedure: 0x9000,
            rtoc: 0,
            refcon: 7,
            parameters: [2, 0, 0],
        };
        assert!(callbacks.arm(0x5000, registration));
        bases.service(&mut callbacks, SECOND / 2);
        assert_eq!(callbacks.take_ready(), None);
        bases.service(&mut callbacks, SECOND);
        assert_eq!(callbacks.take_ready(), Some((0x5000, registration)));
        assert!(callbacks.arm(0x5000, registration));
        bases.service(&mut callbacks, 2 * SECOND);
        assert_eq!(callbacks.take_ready(), None, "already at the stop time");
    }

    #[test]
    fn passing_a_trigger_time_marks_a_time_callback() {
        use super::super::callbacks::Registration;
        let (mut bases, _) = base_with(0x1_0000);
        let mut callbacks = Callbacks::default();
        assert!(callbacks.create(0x5000, 0x1000, 1));
        let registration = Registration {
            procedure: 0x9000,
            rtoc: 0,
            refcon: 0,
            parameters: [1, 300, 600],
        };
        assert!(callbacks.arm(0x5000, registration));
        bases.service(&mut callbacks, 0);
        bases.service(&mut callbacks, SECOND / 4);
        assert_eq!(callbacks.take_ready(), None);
        bases.service(&mut callbacks, SECOND);
        assert_eq!(callbacks.take_ready(), Some((0x5000, registration)));
    }

    #[test]
    fn a_slave_runs_at_its_rate_times_the_master_rate() {
        let mut bases = TimeBases::default();
        bases.create(0x1000, 0);
        bases.create(0x2000, 0);
        bases.records[0].rate = 0x2_0000;
        bases.records[1].rate = 0x1_0000;
        bases.records[1].master = 0x1000;
        assert_eq!(bases.effective_rate(0x2000), 0x2_0000);
        assert_eq!(bases.get_time(1, 600, SECOND), 1200);
        bases.records[0].master = 0x2000;
        assert_eq!(bases.effective_rate(0x2000), 0, "cyclic masters stop");
    }
}
