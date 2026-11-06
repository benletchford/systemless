//! Clock-independent arithmetic for QuickTime time bases.

use super::*;
use std::collections::HashMap;

// The common unit is a 16.16 fraction of a frontend subtick. Keeping the
// fraction across updates avoids making time depend on import frequency.
const UNITS_PER_SECOND: i128 = 60 * crate::time_manager::SUBTICKS_PER_TICK as i128 * 65536;
const FIXED_ONE: i128 = 65536;

pub(super) fn clock_units(subtick: u64) -> i128 {
    i128::from(subtick) * FIXED_ONE
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct TimeBase {
    pub value: i128,
    pub source: i128,
    pub remainder: i128,
    pub rate: i32,
    pub start: Option<i128>,
    pub stop: Option<i128>,
    pub flags: u32,
    pub master: Option<u32>,
}

impl TimeBase {
    pub(super) fn new(source: i128, rate: i32) -> Self {
        Self {
            value: 0,
            source,
            remainder: 0,
            rate,
            start: None,
            stop: None,
            flags: 0,
            master: None,
        }
    }

    pub(super) fn advance(&mut self, source: i128) {
        let delta = source.saturating_sub(self.source);
        self.source = source;
        let numerator = delta
            .saturating_mul(i128::from(self.rate))
            .saturating_add(self.remainder);
        self.remainder = numerator % FIXED_ONE;
        self.value = self.value.saturating_add(numerator / FIXED_ONE);
    }

    pub(super) fn displayed_value(&self) -> i128 {
        match (self.start, self.stop) {
            (Some(start), Some(stop)) if start < stop && self.flags & 1 != 0 => {
                let width = stop - start;
                let phase = self
                    .value
                    .saturating_sub(start)
                    .rem_euclid(if self.flags & 2 != 0 {
                        width.saturating_mul(2)
                    } else {
                        width
                    });
                if phase > width {
                    stop - (phase - width)
                } else {
                    start + phase
                }
            }
            _ => {
                let value = self.start.map_or(self.value, |start| self.value.max(start));
                self.stop.map_or(value, |stop| value.min(stop))
            }
        }
    }

    pub(super) fn crossed_extremes(&self, before: i128) -> (bool, bool) {
        if self.flags & 1 == 0 {
            return (
                self.start
                    .is_some_and(|start| before > start && self.value <= start),
                self.stop
                    .is_some_and(|stop| before < stop && self.value >= stop),
            );
        }
        let (Some(start), Some(stop)) = (self.start, self.stop) else {
            return (false, false);
        };
        let Some(width) = stop.checked_sub(start).filter(|width| *width > 0) else {
            return (false, false);
        };
        let (Some(before), Some(after)) =
            (before.checked_sub(start), self.value.checked_sub(start))
        else {
            return (false, false);
        };
        let ceil = |value: i128| value.div_euclid(width) + i128::from(value.rem_euclid(width) != 0);
        let (first, last) = if after > before {
            (before.div_euclid(width) + 1, after.div_euclid(width))
        } else if after < before {
            (ceil(after), ceil(before) - 1)
        } else {
            return (false, false);
        };
        if first > last {
            return (false, false);
        }
        if self.flags & 2 == 0 || first != last {
            return (true, true);
        }
        (first.rem_euclid(2) == 0, first.rem_euclid(2) != 0)
    }

    pub(super) fn effective_rate(&self, source_rate: i32) -> i32 {
        let rate = (i128::from(self.rate) * i128::from(source_rate) / FIXED_ONE)
            .clamp(i128::from(i32::MIN), i128::from(i32::MAX)) as i32;
        if let (Some(start), Some(stop)) = (self.start, self.stop) {
            if start < stop && self.flags & 1 != 0 {
                if self.flags & 2 != 0 {
                    let width = stop - start;
                    let phase = self
                        .value
                        .saturating_sub(start)
                        .rem_euclid(width.saturating_mul(2));
                    if phase > width || (phase == width && rate > 0) || (phase == 0 && rate < 0) {
                        return rate.saturating_neg();
                    }
                }
                return rate;
            }
        }
        if self
            .start
            .is_some_and(|start| self.value < start || (self.value == start && rate < 0))
            || self
                .stop
                .is_some_and(|stop| self.value > stop || (self.value == stop && rate > 0))
        {
            return 0;
        }
        rate
    }
}

pub(super) fn from_scaled(value: i64, scale: u32) -> Option<i128> {
    if scale == 0 || scale > i32::MAX as u32 {
        return None;
    }
    let numerator = i128::from(value) * UNITS_PER_SECOND;
    let denominator = i128::from(scale);
    let quotient = numerator / denominator;
    let remainder = numerator % denominator;
    Some(quotient + remainder.signum())
}

pub(super) fn to_scaled(value: i128, scale: u32) -> Option<i64> {
    if scale == 0 || scale > i32::MAX as u32 {
        return None;
    }
    Some(
        (value.saturating_mul(i128::from(scale)) / UNITS_PER_SECOND)
            .clamp(i128::from(i64::MIN), i128::from(i64::MAX)) as i64,
    )
}

pub(super) fn record_scale(quicktime: &PpcQuickTimeState, scale: u32, base: u32) -> Option<u32> {
    if scale != 0 {
        return (scale <= i32::MAX as u32).then_some(scale);
    }
    if base == PPC_QT_TIME_BASE && !quicktime.movie_disposed {
        Some(quicktime.movie_time_scale.max(1))
    } else if quicktime.time_bases.records.contains_key(&base) {
        Some(600)
    } else {
        None
    }
}

pub(super) fn convert_to_movie_time(
    quicktime: &PpcQuickTimeState,
    value: i64,
    scale: u32,
    base: u32,
) -> Option<i64> {
    let clock = clock_units(quicktime.movie_clock_subtick.unwrap_or(0));
    let movie_time = from_scaled(
        super::super::dispatch_quicktime::quicktime_callback_time(quicktime),
        quicktime.movie_time_scale.max(1),
    )?;
    let rate = if quicktime.movie_started && quicktime.movie_active {
        quicktime.movie_playback_rate
    } else {
        0
    };
    let anchor = TimeBase {
        value: movie_time,
        source: clock,
        remainder: 0,
        rate,
        start: None,
        stop: None,
        flags: 0,
        master: None,
    };
    let value = quicktime.time_bases.convert(
        from_scaled(value, scale)?,
        base,
        PPC_QT_TIME_BASE,
        PPC_QT_TIME_BASE,
        Some(anchor),
    )?;
    to_scaled(value, quicktime.movie_time_scale)
}

pub(in super::super) fn dispose_time_base(
    id: u32,
    quicktime: &mut PpcQuickTimeState,
    manager: &mut ProcessNativeMemoryManager,
    memory: &mut PpcSectionMem,
    heap_cursor: &mut u32,
    last_mem_error: &mut i16,
) {
    if quicktime.time_bases.records.remove(&id).is_some() {
        quicktime.callbacks.retire_time_base(id);
        let clock = clock_units(quicktime.movie_clock_subtick.unwrap_or(0));
        for record in quicktime.time_bases.records.values_mut() {
            if record.master == Some(id) {
                record.master = None;
                record.source = clock;
            }
        }
        manager.dispose_native_ptr(id);
        super::super::ppc_apply_process_native_allocator(
            manager,
            memory,
            heap_cursor,
            last_mem_error,
        );
    }
}

#[allow(clippy::too_many_arguments)]
pub(super) fn dispatch(
    operation: PpcQuickTimeCompatibilityOperation,
    cpu: &mut PpcCpu,
    memory: &mut PpcSectionMem,
    manager: &mut ProcessNativeMemoryManager,
    heap_cursor: &mut u32,
    last_mem_error: &mut i16,
    quicktime: &mut PpcQuickTimeState,
    sound: &mut PpcSoundState,
    gworlds: &[PpcGWorldRecord],
    current_gworld: u32,
) -> PpcImportAction {
    use PpcQuickTimeCompatibilityOperation as Op;
    let id = cpu.gpr[3];
    let clock = clock_units(quicktime.movie_clock_subtick.unwrap_or(0));
    let movie_time = from_scaled(
        super::super::dispatch_quicktime::quicktime_callback_time(quicktime),
        quicktime.movie_time_scale.max(1),
    )
    .unwrap_or(0);
    let movie_rate = if quicktime.movie_started && quicktime.movie_active {
        quicktime.movie_playback_rate
    } else {
        0
    };
    let movie_effective_rate = ppc_qt_movie_effective_rate(quicktime, movie_rate);
    let movie_anchor = (!quicktime.movie_disposed).then_some(TimeBase {
        value: movie_time,
        source: clock,
        remainder: 0,
        rate: movie_effective_rate,
        start: None,
        stop: None,
        flags: 0,
        master: None,
    });
    match operation {
        Op::NewTimeBase => {
            let address = manager.new_native_ptr(memory, 4, true);
            super::super::ppc_apply_process_native_allocator(
                manager,
                memory,
                heap_cursor,
                last_mem_error,
            );
            if address != 0 {
                quicktime
                    .time_bases
                    .records
                    .insert(address, TimeBase::new(clock, 0));
            }
            PpcImportAction::Return(address)
        }
        Op::DisposeTimeBase => {
            dispose_time_base(id, quicktime, manager, memory, heap_cursor, last_mem_error);
            PpcImportAction::ReturnPreserve
        }
        Op::GetTimeBaseRate | Op::GetTimeBaseEffectiveRate => {
            let rate = if id == PPC_QT_TIME_BASE && !quicktime.movie_disposed {
                if operation == Op::GetTimeBaseEffectiveRate {
                    movie_effective_rate
                } else {
                    movie_rate
                }
            } else if operation == Op::GetTimeBaseEffectiveRate {
                quicktime
                    .time_bases
                    .effective_rate(id, PPC_QT_TIME_BASE, movie_effective_rate)
                    .unwrap_or(0)
            } else {
                quicktime
                    .time_bases
                    .records
                    .get(&id)
                    .map_or(0, |record| record.rate)
            };
            PpcImportAction::Return(rate as u32)
        }
        Op::GetTimeBaseFlags => {
            let flags = if id == PPC_QT_TIME_BASE && !quicktime.movie_disposed {
                quicktime.movie_time_base_flags
            } else {
                quicktime
                    .time_bases
                    .records
                    .get(&id)
                    .map_or(0, |record| record.flags)
            };
            PpcImportAction::Return(flags)
        }
        Op::GetTimeBaseMasterTimeBase => PpcImportAction::Return(
            quicktime
                .time_bases
                .records
                .get(&id)
                .and_then(|record| record.master)
                .unwrap_or(0),
        ),
        Op::GetTimeBaseTime | Op::GetTimeBaseStartTime | Op::GetTimeBaseStopTime => {
            let scale = if cpu.gpr[4] != 0 {
                cpu.gpr[4]
            } else if id == PPC_QT_TIME_BASE && !quicktime.movie_disposed {
                quicktime.movie_time_scale.max(1)
            } else {
                600
            };
            let value = if id == PPC_QT_TIME_BASE && !quicktime.movie_disposed {
                match operation {
                    Op::GetTimeBaseTime => Some(movie_time),
                    Op::GetTimeBaseStartTime => Some(quicktime.movie_time_base_start.unwrap_or(0)),
                    _ => quicktime.movie_time_base_stop.or_else(|| {
                        from_scaled(
                            ppc_qt_movie_duration_value(quicktime) as i64,
                            quicktime.movie_time_scale,
                        )
                    }),
                }
            } else {
                quicktime
                    .time_bases
                    .records
                    .get(&id)
                    .map(|record| match operation {
                        Op::GetTimeBaseTime => record.displayed_value(),
                        Op::GetTimeBaseStartTime => record.start.unwrap_or(i128::MIN),
                        _ => record.stop.unwrap_or(i128::MAX),
                    })
            };
            let time = value.and_then(|value| to_scaled(value, scale)).unwrap_or(0);
            let out = cpu.gpr[5];
            if out != 0 && super::super::ppc_memory_can_write_bytes(memory, out, 16) {
                let _ = memory.write_u64_be(out, time as u64);
                let _ = memory.write_u32_be(out + 8, scale);
                let _ = memory.write_u32_be(out + 12, id);
            }
            PpcImportAction::Return(time.clamp(i64::from(i32::MIN), i64::from(i32::MAX)) as u32)
        }
        Op::SetTimeBaseValue | Op::SetTimeBaseTime => {
            let value = if operation == Op::SetTimeBaseValue {
                from_scaled(i64::from(cpu.gpr[4] as i32), cpu.gpr[5])
            } else {
                let ptr = cpu.gpr[4];
                memory
                    .read_u64_be(ptr)
                    .zip(ptr.checked_add(8).and_then(|p| memory.read_u32_be(p)))
                    .zip(ptr.checked_add(12).and_then(|p| memory.read_u32_be(p)))
                    .and_then(|((value, scale), base)| {
                        let scale = record_scale(quicktime, scale, base)?;
                        let value = from_scaled(value as i64, scale)?;
                        if base == 0 || base == id {
                            Some(value)
                        } else {
                            quicktime.time_bases.convert(
                                value,
                                base,
                                id,
                                PPC_QT_TIME_BASE,
                                movie_anchor,
                            )
                        }
                    })
            };
            if let Some(value) = value {
                if id == PPC_QT_TIME_BASE && !quicktime.movie_disposed {
                    let value = to_scaled(value, quicktime.movie_time_scale)
                        .unwrap_or(0)
                        .max(0) as u64;
                    ppc_qt_seek_movie(quicktime, sound, value);
                } else if let Some(record) = quicktime.time_bases.records.get_mut(&id) {
                    record.value = value;
                    record.remainder = 0;
                    quicktime.callbacks.mark_event(id, callbacks::Event::Jump);
                }
            }
            PpcImportAction::ReturnPreserve
        }
        Op::SetTimeBaseRate => {
            let rate = cpu.gpr[4] as i32;
            if id == PPC_QT_TIME_BASE && !quicktime.movie_disposed {
                let original = cpu.gpr[3];
                cpu.gpr[3] = PPC_QT_MOVIE;
                let _ = ppc_qt_set_movie_rate(
                    cpu,
                    memory,
                    gworlds,
                    current_gworld,
                    quicktime,
                    sound,
                    rate,
                );
                cpu.gpr[3] = original;
            } else if let Some(record) = quicktime.time_bases.records.get_mut(&id) {
                let before = record.rate;
                if record.flags & 3 == 1 {
                    record.value = record.displayed_value();
                    record.remainder = 0;
                }
                record.rate = rate;
                quicktime.callbacks.mark_event(
                    id,
                    callbacks::Event::Rate {
                        before,
                        after: rate,
                    },
                );
            }
            PpcImportAction::ReturnPreserve
        }
        Op::SetTimeBaseStartTime | Op::SetTimeBaseStopTime => {
            let ptr = cpu.gpr[4];
            let value = memory.read_u64_be(ptr).map(|value| value as i64);
            let scale = ptr.checked_add(8).and_then(|p| memory.read_u32_be(p));
            let base = ptr.checked_add(12).and_then(|p| memory.read_u32_be(p));
            if let (Some(value), Some(scale), Some(base)) = (value, scale, base) {
                let scale = record_scale(quicktime, scale, base).unwrap_or(0);
                let value = from_scaled(value, scale).and_then(|value| {
                    if base == 0 || base == id {
                        Some(value)
                    } else {
                        quicktime.time_bases.convert(
                            value,
                            base,
                            id,
                            PPC_QT_TIME_BASE,
                            movie_anchor,
                        )
                    }
                });
                if let Some(value) = value {
                    if id == PPC_QT_TIME_BASE && !quicktime.movie_disposed {
                        quicktime.movie_time_base_phase = None;
                        if operation == Op::SetTimeBaseStartTime {
                            quicktime.movie_time_base_start = Some(value);
                        } else {
                            quicktime.movie_time_base_stop = Some(value);
                        }
                        if quicktime.movie_started && quicktime.movie_active {
                            let _ = ppc_qt_start_movie_audio(quicktime, sound);
                        }
                    } else if let Some(record) = quicktime.time_bases.records.get_mut(&id) {
                        if operation == Op::SetTimeBaseStartTime {
                            record.start = Some(value);
                        } else {
                            record.stop = Some(value);
                        }
                    }
                }
            }
            PpcImportAction::ReturnPreserve
        }
        Op::SetTimeBaseMasterTimeBase => {
            let master = cpu.gpr[4];
            let zero_ptr = cpu.gpr[5];
            let zero = if zero_ptr == 0 {
                Some(0)
            } else {
                memory
                    .read_u64_be(zero_ptr)
                    .zip(zero_ptr.checked_add(8).and_then(|p| memory.read_u32_be(p)))
                    .zip(zero_ptr.checked_add(12).and_then(|p| memory.read_u32_be(p)))
                    .and_then(|((value, scale), base)| {
                        let scale = record_scale(quicktime, scale, base)?;
                        let value = from_scaled(value as i64, scale)?;
                        if base == 0 || base == master {
                            Some(value)
                        } else {
                            quicktime.time_bases.convert(
                                value,
                                base,
                                master,
                                PPC_QT_TIME_BASE,
                                movie_anchor,
                            )
                        }
                    })
            };
            if let Some(zero) = zero {
                quicktime.time_bases.set_master(
                    id,
                    master,
                    zero,
                    clock,
                    PPC_QT_TIME_BASE,
                    movie_time,
                );
            }
            PpcImportAction::ReturnPreserve
        }
        Op::ConvertTime => {
            let ptr = id;
            let target = cpu.gpr[4];
            let fields = memory
                .read_u64_be(ptr)
                .zip(ptr.checked_add(8).and_then(|p| memory.read_u32_be(p)))
                .zip(ptr.checked_add(12).and_then(|p| memory.read_u32_be(p)));
            if let Some(((value, scale), source)) = fields {
                let scale = record_scale(quicktime, scale, source);
                let converted = scale
                    .and_then(|scale| from_scaled(value as i64, scale))
                    .and_then(|value| {
                        quicktime.time_bases.convert(
                            value,
                            source,
                            target,
                            PPC_QT_TIME_BASE,
                            movie_anchor,
                        )
                    })
                    .and_then(|value| to_scaled(value, scale?));
                if let Some(value) =
                    converted.filter(|_| super::super::ppc_memory_can_write_bytes(memory, ptr, 16))
                {
                    let _ = memory.write_u64_be(ptr, value as u64);
                    let _ = memory.write_u32_be(ptr + 8, scale.unwrap());
                    let _ = memory.write_u32_be(ptr + 12, target);
                }
            }
            PpcImportAction::ReturnPreserve
        }
        _ => unreachable!(),
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct TimeBases {
    pub records: HashMap<u32, TimeBase>,
}

impl TimeBases {
    pub(super) fn set_master(
        &mut self,
        slave: u32,
        master: u32,
        zero: i128,
        clock: i128,
        movie_base: u32,
        movie_time: i128,
    ) -> bool {
        if !self.accepts_master(slave, master, movie_base) {
            return false;
        }
        self.service(clock, movie_base, movie_time);
        let source = if master == 0 {
            clock
        } else if master == movie_base {
            movie_time
        } else {
            self.records[&master].displayed_value()
        };
        let record = self.records.get_mut(&slave).unwrap();
        record.master = (master != 0).then_some(master);
        record.source = source;
        record.value = source
            .saturating_sub(zero)
            .saturating_mul(i128::from(record.rate))
            / FIXED_ONE;
        record.remainder = 0;
        true
    }

    /// Convert an absolute time through a common source using each base's
    /// current affine mapping. A stopped base cannot be inverted.
    pub(super) fn convert(
        &self,
        value: i128,
        from: u32,
        to: u32,
        movie_base: u32,
        movie_anchor: Option<TimeBase>,
    ) -> Option<i128> {
        fn ancestry(
            bases: &TimeBases,
            mut id: u32,
            movie: u32,
            anchor: Option<TimeBase>,
        ) -> Option<(u32, Vec<(u32, TimeBase)>)> {
            let mut path = Vec::new();
            for _ in 0..=bases.records.len() + 1 {
                if id == 0 {
                    return Some((id, path));
                }
                let record = if id == movie {
                    anchor?
                } else {
                    *bases.records.get(&id)?
                };
                path.push((id, record));
                id = record.master.unwrap_or(0);
            }
            None
        }
        let (source_root, mut source_path) = ancestry(self, from, movie_base, movie_anchor)?;
        let (target_root, mut target_path) = ancestry(self, to, movie_base, movie_anchor)?;
        if source_root != target_root {
            return None;
        }
        while source_path
            .last()
            .zip(target_path.last())
            .is_some_and(|(a, b)| a.0 == b.0)
        {
            source_path.pop();
            target_path.pop();
        }
        let mut result = value;
        for (_, record) in source_path {
            if record.rate == 0 {
                return None;
            }
            result = record.source.checked_add(
                result.checked_sub(record.value)?.checked_mul(FIXED_ONE)? / i128::from(record.rate),
            )?;
        }
        for (_, record) in target_path.into_iter().rev() {
            result = record.value.checked_add(
                result
                    .checked_sub(record.source)?
                    .checked_mul(i128::from(record.rate))?
                    / FIXED_ONE,
            )?;
        }
        Some(result)
    }

    pub(super) fn service(&mut self, clock: i128, movie_base: u32, movie_time: i128) {
        let mut completed = HashMap::new();
        let ids: Vec<u32> = self.records.keys().copied().collect();
        for id in ids {
            self.service_one(id, clock, movie_base, movie_time, &mut completed);
        }
    }

    fn service_one(
        &mut self,
        id: u32,
        clock: i128,
        movie_base: u32,
        movie_time: i128,
        completed: &mut HashMap<u32, i128>,
    ) -> Option<i128> {
        let mut path = Vec::new();
        let mut current = id;
        let mut source = None;
        for _ in 0..=self.records.len() {
            if let Some(value) = completed.get(&current) {
                source = Some(*value);
                break;
            }
            let record = self.records.get(&current)?;
            path.try_reserve(1).ok()?;
            path.push(current);
            match record.master {
                None => {
                    source = Some(clock);
                    break;
                }
                Some(master) if master == movie_base => {
                    source = Some(movie_time);
                    break;
                }
                Some(master) => current = master,
            }
        }
        let mut value = source?;
        completed.try_reserve(path.len()).ok()?;
        for current in path.into_iter().rev() {
            let record = self.records.get_mut(&current)?;
            record.advance(value);
            value = record.displayed_value();
            completed.insert(current, value);
        }
        Some(value)
    }

    pub(super) fn effective_rate(&self, id: u32, movie_base: u32, movie_rate: i32) -> Option<i32> {
        let mut path = Vec::new();
        let mut current = id;
        for _ in 0..=self.records.len() {
            if current == movie_base {
                return Some(
                    path.into_iter()
                        .rev()
                        .fold(movie_rate, |rate, record: TimeBase| {
                            record.effective_rate(rate)
                        }),
                );
            }
            let record = self.records.get(&current)?;
            path.push(*record);
            let Some(master) = record.master else {
                return Some(
                    path.into_iter()
                        .rev()
                        .fold(0x10000, |rate, record| record.effective_rate(rate)),
                );
            };
            current = master;
        }
        None
    }

    pub(super) fn depends_on(&self, id: u32, master: u32) -> bool {
        let mut current = id;
        for _ in 0..=self.records.len() {
            if current == master {
                return true;
            }
            let Some(next) = self.records.get(&current).and_then(|record| record.master) else {
                return false;
            };
            current = next;
        }
        false
    }

    /// A master chain must terminate at the guest clock or movie time base.
    pub(super) fn accepts_master(&self, slave: u32, master: u32, movie_base: u32) -> bool {
        if !self.records.contains_key(&slave) {
            return false;
        }
        let mut current = master;
        for _ in 0..=self.records.len() {
            if current == 0 || current == movie_base {
                return true;
            }
            if current == slave {
                return false;
            }
            let Some(record) = self.records.get(&current) else {
                return false;
            };
            current = record.master.unwrap_or(0);
        }
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn service_walks_deep_master_chains_without_recursion() {
        let mut bases = TimeBases::default();
        for id in 1..=10_000u32 {
            let mut base = TimeBase::new(0, 0x10000);
            base.master = (id > 1).then_some(id - 1);
            bases.records.insert(id, base);
        }
        bases.service(UNITS_PER_SECOND, u32::MAX, 0);
        assert_eq!(bases.records[&10_000].value, UNITS_PER_SECOND);
        bases.service(UNITS_PER_SECOND * 2, u32::MAX, 0);
        assert_eq!(bases.records[&10_000].value, UNITS_PER_SECOND * 2);
    }

    #[test]
    fn palindrome_effective_rate_points_inward_at_exact_endpoints() {
        let mut time = TimeBase::new(0, 0x10000);
        time.start = Some(0);
        time.stop = Some(10);
        time.flags = 3;
        for (value, rate, expected) in [
            (0, 0x10000, 0x10000),
            (0, -0x10000, 0x10000),
            (10, 0x10000, -0x10000),
            (10, -0x10000, -0x10000),
            (20, -0x10000, 0x10000),
            (-10, -0x10000, -0x10000),
        ] {
            time.value = value;
            time.rate = rate;
            assert_eq!(time.effective_rate(0x10000), expected, "{value} {rate}");
        }
    }

    #[test]
    fn looping_extremes_include_wraps_and_palindrome_turns() {
        let mut time = TimeBase::new(0, 0x10000);
        time.start = Some(0);
        time.stop = Some(10);
        for (flags, before, after, expected) in [
            (1, 8, 10, (true, true)),
            (1, 2, -1, (true, true)),
            (1, 2, 9, (false, false)),
            (3, 8, 10, (false, true)),
            (3, 10, 12, (false, false)),
            (3, 18, 20, (true, false)),
            (3, 12, 8, (false, true)),
            (3, 2, -2, (true, false)),
            (3, 2, 32, (true, true)),
            (3, 20, 20, (false, false)),
        ] {
            time.flags = flags;
            time.value = after;
            assert_eq!(
                time.crossed_extremes(before),
                expected,
                "{flags} {before} {after}"
            );
        }
    }

    #[test]
    fn scaled_values_round_trip_when_subtick_fraction_is_not_exact() {
        for value in [-7, -1, 0, 1, 7] {
            assert_eq!(to_scaled(from_scaled(value, 7).unwrap(), 7), Some(value));
        }
    }

    #[test]
    fn fixed_rate_keeps_fraction_across_frequent_updates() {
        let mut sparse = TimeBase::new(0, 0x8000);
        let mut frequent = sparse;
        sparse.advance(UNITS_PER_SECOND);
        for step in 1..=1000 {
            frequent.advance(UNITS_PER_SECOND * step / 1000);
        }
        assert_eq!(to_scaled(sparse.displayed_value(), 600), Some(300));
        assert_eq!(frequent.value, sparse.value);
        assert_eq!(frequent.remainder, sparse.remainder);
    }

    #[test]
    fn bounds_pin_or_loop_without_losing_the_unpinned_value() {
        let mut time = TimeBase::new(0, 0x10000);
        time.start = from_scaled(2, 1);
        time.stop = from_scaled(5, 1);
        time.advance(UNITS_PER_SECOND * 6);
        assert_eq!(to_scaled(time.displayed_value(), 1), Some(5));
        time.flags = 1;
        assert_eq!(to_scaled(time.displayed_value(), 1), Some(3));
        time.flags = 3;
        assert_eq!(to_scaled(time.displayed_value(), 1), Some(4));
        assert_eq!(time.effective_rate(0x10000), -0x10000);
        time.flags = 0;
        assert_eq!(to_scaled(time.value, 1), Some(6));
        assert_eq!(time.effective_rate(0x10000), 0);
        assert_eq!(time.effective_rate(-0x10000), 0);
        time.value = time.stop.unwrap();
        assert_eq!(time.effective_rate(-0x10000), -0x10000);
    }

    #[test]
    fn master_chains_reject_missing_nodes_and_cycles() {
        let mut bases = TimeBases::default();
        bases.records.insert(1, TimeBase::new(0, 0));
        bases.records.insert(2, TimeBase::new(0, 0));
        bases.records.get_mut(&2).unwrap().master = Some(1);
        assert!(!bases.accepts_master(1, 2, 99));
        assert!(!bases.accepts_master(1, 3, 99));
        assert!(bases.accepts_master(1, 99, 99));
        assert!(bases.accepts_master(2, 0, 99));
    }

    #[test]
    fn master_chain_advances_from_the_masters_position_and_rate() {
        let mut bases = TimeBases::default();
        bases.records.insert(1, TimeBase::new(0, 0x8000));
        let mut slave = TimeBase::new(0, 0x20000);
        slave.master = Some(1);
        bases.records.insert(2, slave);
        bases.service(UNITS_PER_SECOND * 3, 99, 0);
        assert_eq!(
            to_scaled(bases.records[&1].displayed_value(), 600),
            Some(900)
        );
        assert_eq!(
            to_scaled(bases.records[&2].displayed_value(), 600),
            Some(1800)
        );
        assert_eq!(bases.effective_rate(2, 99, 0), Some(0x10000));
        bases.records.get_mut(&1).unwrap().rate = 0;
        bases.service(UNITS_PER_SECOND * 4, 99, 0);
        assert_eq!(
            to_scaled(bases.records[&2].displayed_value(), 600),
            Some(1800)
        );
    }

    #[test]
    fn conversion_uses_master_offsets_and_rates() {
        let mut bases = TimeBases::default();
        bases.records.insert(1, TimeBase::new(0, 0x10000));
        bases.records.insert(2, TimeBase::new(0, 0x8000));
        let second = UNITS_PER_SECOND;
        bases.service(second * 10, 99, 0);
        assert!(bases.set_master(2, 1, second * 2, second * 10, 99, 0));
        assert_eq!(to_scaled(bases.records[&2].value, 1), Some(4));
        assert_eq!(
            to_scaled(bases.convert(second * 5, 2, 1, 99, None).unwrap(), 1),
            Some(12)
        );
        assert_eq!(
            to_scaled(bases.convert(second * 12, 1, 2, 99, None).unwrap(), 1),
            Some(5)
        );
        assert_eq!(bases.convert(second, 2, 99, 99, None), None);
        bases.records.get_mut(&2).unwrap().rate = 0;
        assert_eq!(bases.convert(second, 2, 1, 99, None), None);
    }

    #[test]
    fn conversion_uses_the_movie_clock_anchor() {
        let mut bases = TimeBases::default();
        let second = UNITS_PER_SECOND;
        let mut independent = TimeBase::new(second * 10, 0x10000);
        independent.value = second * 4;
        bases.records.insert(1, independent);
        let mut movie = TimeBase::new(second * 10, 0x8000);
        movie.value = second * 3;
        assert_eq!(
            bases.convert(second * 4, 1, 99, 99, Some(movie)),
            Some(second * 3)
        );
        assert_eq!(
            bases.convert(second * 4, 99, 1, 99, Some(movie)),
            Some(second * 6)
        );
        movie.rate = 0;
        assert_eq!(bases.convert(second * 4, 99, 1, 99, Some(movie)), None);
    }
}
