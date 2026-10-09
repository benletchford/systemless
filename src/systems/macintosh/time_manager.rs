//! Architecture-neutral evaluation helpers and canonical structures for
//! Macintosh Time Manager and Vertical Retrace (VBL) Manager operations.
//!
//! Inside Macintosh: Processes (1994), Chapters 3 ("Time Manager") & 4 ("Vertical Retrace Manager")
//! Inside Macintosh: Operating System Utilities (1994), Chapter 4 ("Date, Time, and Measurement Utilities")
//! Inside Macintosh: Volume II (1985), pp. II-379--384.

#![allow(dead_code)]

/// Standard VBL task record size in bytes (`qLink: 4, qType: 2, vblAddr: 4, vblCount: 2, vblPhase: 2`).
pub const VBL_TASK_RECORD_SIZE: usize = 14;

/// Standard Time Manager task record size in bytes (`qLink: 4, qType: 2, tmAddr: 4, tmCount: 4`).
pub const TM_TASK_RECORD_SIZE: usize = 14;

/// Extended Time Manager task record size in bytes (`+ tmWakeUp: 4, tmReserved: 4`).
pub const TM_EXTENDED_TASK_RECORD_SIZE: usize = 22;

/// Standard DateTimeRec size in bytes (`year: 2, month: 2, day: 2, hour: 2, minute: 2, second: 2, dayOfWeek: 2`).
pub const DATE_TIME_REC_SIZE: usize = 14;

/// Offset of `qLink` in VBLTask / TMTask (longword).
pub const TASK_Q_LINK_OFFSET: u32 = 0;

/// Offset of `qType` in VBLTask / TMTask (word).
pub const TASK_Q_TYPE_OFFSET: u32 = 4;

/// Offset of `vblAddr` / `tmAddr` (longword).
pub const TASK_ADDR_OFFSET: u32 = 6;

/// Offset of `vblCount` in VBLTask (word) / `tmCount` in TMTask (longword).
pub const VBL_COUNT_OFFSET: u32 = 10;
pub const TM_COUNT_OFFSET: u32 = 10;

/// Offset of `vblPhase` in VBLTask (word).
pub const VBL_PHASE_OFFSET: u32 = 12;

/// Offset of `tmWakeUp` in extended TMTask (longword).
pub const TM_WAKE_UP_OFFSET: u32 = 14;

/// Offset of `tmReserved` in extended TMTask (longword).
pub const TM_RESERVED_OFFSET: u32 = 18;

/// Expected `qType` for vertical retrace tasks: ORD(vType) = 1.
pub const VBL_ORD_V_TYPE: u16 = 1;

/// High-bit mask for `qType` indicating Time Manager task activation.
pub const TM_ACTIVE_Q_TYPE_BIT: u16 = 0x8000;

/// Mask to clear the high bit of `qType` (deactivation).
pub const TM_INACTIVE_Q_TYPE_MASK: u16 = 0x7FFF;

/// Number of subticks per 60 Hz guest tick (1 million).
pub const SUBTICKS_PER_TICK: u64 = 1_000_000;

/// Microseconds per 60.15 Hz tick (VBL frequency).
pub const MICROSECONDS_PER_TICK: u64 = 16_625;

/// Mac epoch base year (1904).
pub const MAC_EPOCH_YEAR: u16 = 1904;

/// Seconds per standard day (86,400).
pub const SECONDS_PER_DAY: u32 = 86_400;

/// Seconds per hour (3,600).
pub const SECONDS_PER_HOUR: u32 = 3_600;

/// Seconds per minute (60).
pub const SECONDS_PER_MINUTE: u32 = 60;

/// Days in Gregorian calendar months (index 1..=12).
pub const DAYS_IN_MONTH: [u32; 13] = [0, 31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31];

/// OSErr: No error (0).
pub const NO_ERR: i16 = 0;

/// OSErr: Queue error - element not in queue (-1).
pub const Q_ERR: i16 = -1;

/// OSErr: Invalid queue element type (-2).
pub const V_TYP_ERR: i16 = -2;

/// OSErr: Invalid slot number (-360).
pub const SLOT_NUM_ERR: i16 = -360;

/// Returns true if a Gregorian calendar year is a leap year.
pub fn is_leap_year(year: u16) -> bool {
    year % 4 == 0 && (year % 100 != 0 || year % 400 == 0)
}

/// Returns the number of days in a given Gregorian month (1-based).
pub fn days_in_month(year: u16, month: u16) -> u32 {
    let mut dim = *DAYS_IN_MONTH.get(month as usize).unwrap_or(&0);
    if month == 2 && is_leap_year(year) {
        dim += 1;
    }
    dim
}

/// Canonical representation of the classic Mac OS `DateTimeRec`.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct DateTimeRecord {
    pub year: u16,
    pub month: u16,
    pub day: u16,
    pub hour: u16,
    pub minute: u16,
    pub second: u16,
    pub day_of_week: u16,
}

/// Convert seconds since Jan 1, 1904 00:00:00 into a canonical `DateTimeRecord`.
pub fn evaluate_seconds_to_date(seconds: u32) -> DateTimeRecord {
    let day_of_week = ((seconds / SECONDS_PER_DAY + 5) % 7 + 1) as u16;
    let seconds_today = seconds % SECONDS_PER_DAY;
    let mut days = seconds / SECONDS_PER_DAY;

    let mut year = MAC_EPOCH_YEAR;
    loop {
        let days_in_year = if is_leap_year(year) { 366 } else { 365 };
        if days < days_in_year {
            break;
        }
        days -= days_in_year;
        year += 1;
    }

    let mut month = 1u16;
    loop {
        let dim = days_in_month(year, month);
        if days < dim {
            break;
        }
        days -= dim;
        month += 1;
    }

    DateTimeRecord {
        year,
        month,
        day: (days + 1) as u16,
        hour: (seconds_today / SECONDS_PER_HOUR) as u16,
        minute: ((seconds_today % SECONDS_PER_HOUR) / SECONDS_PER_MINUTE) as u16,
        second: (seconds_today % SECONDS_PER_MINUTE) as u16,
        day_of_week,
    }
}

/// Convert a `DateTimeRecord` into seconds since Jan 1, 1904 00:00:00.
pub fn evaluate_date_to_seconds(record: &DateTimeRecord) -> u32 {
    let mut days = 0u32;
    for y in MAC_EPOCH_YEAR..record.year {
        days += if is_leap_year(y) { 366 } else { 365 };
    }
    for m in 1..record.month {
        days += days_in_month(record.year, m);
    }
    days += u32::from(record.day.saturating_sub(1));
    days * SECONDS_PER_DAY
        + u32::from(record.hour) * SECONDS_PER_HOUR
        + u32::from(record.minute) * SECONDS_PER_MINUTE
        + u32::from(record.second)
}

/// Canonical evaluation result for VInstall / SlotVInstall.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct VInstallEvaluation {
    pub result: i16,
    pub initial_count: u16,
}

pub fn evaluate_v_install(
    task_ptr: u32,
    q_type: u16,
    vbl_count: u16,
    vbl_phase: u16,
    slot: Option<i16>,
) -> VInstallEvaluation {
    if task_ptr == 0 || q_type != VBL_ORD_V_TYPE {
        return VInstallEvaluation {
            result: V_TYP_ERR,
            initial_count: vbl_count,
        };
    }
    if matches!(slot, Some(s) if s < -1) {
        return VInstallEvaluation {
            result: SLOT_NUM_ERR,
            initial_count: vbl_count,
        };
    }
    VInstallEvaluation {
        result: NO_ERR,
        initial_count: vbl_count.wrapping_add(vbl_phase),
    }
}

/// Canonical evaluation result for VRemove / SlotVRemove.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct VRemoveEvaluation {
    pub result: i16,
}

pub fn evaluate_v_remove(
    task_ptr: u32,
    q_type: u16,
    was_in_queue: bool,
    slot: Option<i16>,
) -> VRemoveEvaluation {
    if task_ptr == 0 || q_type != VBL_ORD_V_TYPE {
        return VRemoveEvaluation { result: V_TYP_ERR };
    }
    if matches!(slot, Some(s) if s < -1) {
        return VRemoveEvaluation { result: SLOT_NUM_ERR };
    }
    if !was_in_queue {
        return VRemoveEvaluation { result: Q_ERR };
    }
    VRemoveEvaluation { result: NO_ERR }
}

/// Canonical action describing an `InsTime` / `InsXTime` installation request.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct InsTimeAction {
    pub task_ptr: u32,
    pub extended: bool,
    pub cleared_q_type: u16,
}

impl InsTimeAction {
    pub fn is_valid(&self) -> bool {
        self.task_ptr != 0
    }
}

pub fn evaluate_ins_time(task_ptr: u32, current_q_type: u16, extended: bool) -> InsTimeAction {
    InsTimeAction {
        task_ptr,
        extended,
        cleared_q_type: current_q_type & TM_INACTIVE_Q_TYPE_MASK,
    }
}

/// Canonical evaluation result for `PrimeTime`.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PrimeTimeEvaluation {
    pub task_ptr: u32,
    pub count: i32,
    pub requested_delay_subticks: u64,
    pub current_subtick: u64,
    pub fire_at_subtick: u64,
    pub fire_at_tick: u32,
    pub primed_q_type: u16,
    pub intended_wakeup: Option<u64>,
    pub opaque_wakeup: Option<u32>,
}

impl PrimeTimeEvaluation {
    pub fn is_valid(&self) -> bool {
        self.task_ptr != 0
    }
}

pub fn evaluate_prime_time(
    task_ptr: u32,
    current_q_type: u16,
    count: i32,
    current_tick: u32,
    current_subtick: u64,
    is_extended: bool,
    prior_wakeup: Option<u64>,
) -> PrimeTimeEvaluation {
    let requested_delay_subticks = if count == 0 {
        0
    } else if count > 0 {
        u64::from(count as u32) * 60_000
    } else {
        (u64::from(count.unsigned_abs()) * 60).max(1)
    };

    let clock = current_subtick.max(u64::from(current_tick) * SUBTICKS_PER_TICK);

    let (fire_at_subtick, intended_wakeup, opaque_wakeup) = if is_extended {
        let intended = prior_wakeup
            .unwrap_or(clock)
            .saturating_add(requested_delay_subticks);
        let opaque = ((intended / 60) as u32).max(1);
        (intended.max(clock), Some(intended), Some(opaque))
    } else {
        let delay = if count == 0 {
            SUBTICKS_PER_TICK
        } else {
            requested_delay_subticks
        };
        (clock.saturating_add(delay), None, None)
    };

    let fire_at_tick = fire_at_subtick.div_ceil(SUBTICKS_PER_TICK) as u32;

    PrimeTimeEvaluation {
        task_ptr,
        count,
        requested_delay_subticks,
        current_subtick: clock,
        fire_at_subtick,
        fire_at_tick,
        primed_q_type: current_q_type | TM_ACTIVE_Q_TYPE_BIT,
        intended_wakeup,
        opaque_wakeup,
    }
}

/// Canonical evaluation result for `RmvTime`.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RmvTimeEvaluation {
    pub task_ptr: u32,
    pub remaining_subticks: u64,
    pub remaining_count: i32,
    pub cleared_q_type: u16,
}

impl RmvTimeEvaluation {
    pub fn is_valid(&self) -> bool {
        self.task_ptr != 0
    }
}

pub fn evaluate_rmv_time(
    task_ptr: u32,
    current_q_type: u16,
    current_subtick: u64,
    task_fire_at_subtick: Option<u64>,
) -> RmvTimeEvaluation {
    let remaining_subticks = task_fire_at_subtick
        .map(|fire_at| fire_at.saturating_sub(current_subtick))
        .unwrap_or(0);

    let remaining_count = if remaining_subticks == 0 {
        0
    } else {
        let remaining_us = remaining_subticks.div_ceil(60);
        if remaining_us <= i32::MAX as u64 {
            -(remaining_us as i32)
        } else {
            remaining_us.div_ceil(1_000).min(i32::MAX as u64) as i32
        }
    };

    RmvTimeEvaluation {
        task_ptr,
        remaining_subticks,
        remaining_count,
        cleared_q_type: current_q_type & TM_INACTIVE_Q_TYPE_MASK,
    }
}

/// Canonical evaluation result for `Microseconds`.
pub fn evaluate_microseconds(tick_count: u32) -> u64 {
    u64::from(tick_count).saturating_mul(MICROSECONDS_PER_TICK)
}

/// Evaluate `Microseconds` returning `(low_32_bits, high_32_bits)` as placed in `(D0, A0)`.
pub fn evaluate_microseconds_registers(tick_count: u32) -> (u32, u32) {
    let usecs = evaluate_microseconds(tick_count);
    (usecs as u32, (usecs >> 32) as u32)
}

/// Canonical parameters for `Delay`.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DelayParameters {
    pub num_ticks: u32,
    pub final_ticks_ptr: u32,
    pub current_tick: u32,
}

impl DelayParameters {
    pub fn is_immediate(&self) -> bool {
        self.num_ticks == 0
    }

    pub fn deadline(&self) -> u32 {
        self.current_tick.wrapping_add(self.num_ticks)
    }

    pub fn is_reached(&self, deadline: u32) -> bool {
        self.current_tick.wrapping_sub(deadline) < 0x8000_0000
    }
}

pub fn evaluate_delay_parameters(
    num_ticks: u32,
    final_ticks_ptr: u32,
    current_tick: u32,
) -> DelayParameters {
    DelayParameters {
        num_ticks,
        final_ticks_ptr,
        current_tick,
    }
}

pub fn evaluate_delay_deadline(current_tick: u32, num_ticks: u32) -> u32 {
    current_tick.wrapping_add(num_ticks)
}

pub fn evaluate_delay_reached(current_tick: u32, deadline: u32) -> bool {
    current_tick.wrapping_sub(deadline) < 0x8000_0000
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn time_manager_date_conversion_roundtrip() {
        assert!(is_leap_year(1904));
        assert!(!is_leap_year(1905));
        assert!(is_leap_year(2000));
        assert!(!is_leap_year(1900));

        assert_eq!(days_in_month(1904, 2), 29);
        assert_eq!(days_in_month(1905, 2), 28);
        assert_eq!(days_in_month(1904, 1), 31);

        // Jan 1, 1904 00:00:00 is Friday (6)
        let epoch_date = evaluate_seconds_to_date(0);
        assert_eq!(
            epoch_date,
            DateTimeRecord {
                year: 1904,
                month: 1,
                day: 1,
                hour: 0,
                minute: 0,
                second: 0,
                day_of_week: 6,
            }
        );
        assert_eq!(evaluate_date_to_seconds(&epoch_date), 0);

        // A known timestamp
        let secs = 2_082_844_800; // Jan 1, 1970 00:00:00 UTC
        let unix_epoch = evaluate_seconds_to_date(secs);
        assert_eq!(unix_epoch.year, 1970);
        assert_eq!(unix_epoch.month, 1);
        assert_eq!(unix_epoch.day, 1);
        assert_eq!(evaluate_date_to_seconds(&unix_epoch), secs);
    }

    #[test]
    fn vbl_install_and_remove_evaluation() {
        // Null task pointer
        assert_eq!(
            evaluate_v_install(0, 1, 10, 0, None).result,
            V_TYP_ERR
        );
        // Invalid qType
        assert_eq!(
            evaluate_v_install(0x1000, 2, 10, 0, None).result,
            V_TYP_ERR
        );
        // Invalid slot (< -1)
        assert_eq!(
            evaluate_v_install(0x1000, 1, 10, 0, Some(-2)).result,
            SLOT_NUM_ERR
        );
        // Valid install adds phase to count
        let eval = evaluate_v_install(0x1000, 1, 10, 5, Some(-1));
        assert_eq!(eval.result, NO_ERR);
        assert_eq!(eval.initial_count, 15);

        // Remove checks
        assert_eq!(
            evaluate_v_remove(0, 1, true, None).result,
            V_TYP_ERR
        );
        assert_eq!(
            evaluate_v_remove(0x1000, 0, true, None).result,
            V_TYP_ERR
        );
        assert_eq!(
            evaluate_v_remove(0x1000, 1, true, Some(-2)).result,
            SLOT_NUM_ERR
        );
        assert_eq!(
            evaluate_v_remove(0x1000, 1, false, None).result,
            Q_ERR
        );
        assert_eq!(
            evaluate_v_remove(0x1000, 1, true, None).result,
            NO_ERR
        );
    }

    #[test]
    fn time_task_install_prime_and_remove_evaluation() {
        let ins = evaluate_ins_time(0x2000, 0x8001, false);
        assert!(ins.is_valid());
        assert_eq!(ins.cleared_q_type, 0x0001);

        // Positive delay in ms (100 ms = 6,000,000 subticks)
        let prime_pos = evaluate_prime_time(0x2000, 0x0001, 100, 10, 10_000_000, false, None);
        assert_eq!(prime_pos.requested_delay_subticks, 6_000_000);
        assert_eq!(prime_pos.fire_at_subtick, 16_000_000);
        assert_eq!(prime_pos.fire_at_tick, 16);
        assert_eq!(prime_pos.primed_q_type, 0x8001);

        // Negative delay in us (-1,000 us = 60,000 subticks)
        let prime_neg = evaluate_prime_time(0x2000, 0x0001, -1_000, 10, 10_000_000, false, None);
        assert_eq!(prime_neg.requested_delay_subticks, 60_000);
        assert_eq!(prime_neg.fire_at_subtick, 10_060_000);

        // Extended task with prior wakeup
        let prime_ext = evaluate_prime_time(0x2000, 0x0001, 100, 10, 10_000_000, true, Some(9_000_000));
        assert_eq!(prime_ext.intended_wakeup, Some(15_000_000));
        assert_eq!(prime_ext.opaque_wakeup, Some(250_000));
        assert_eq!(prime_ext.fire_at_subtick, 15_000_000);

        // RmvTime calculation
        let rmv = evaluate_rmv_time(0x2000, 0x8001, 10_000_000, Some(10_060_000));
        assert_eq!(rmv.remaining_subticks, 60_000);
        assert_eq!(rmv.remaining_count, -1_000); // -1000 us
        assert_eq!(rmv.cleared_q_type, 0x0001);
    }

    #[test]
    fn microseconds_and_delay_evaluation() {
        assert_eq!(evaluate_microseconds(0), 0);
        assert_eq!(evaluate_microseconds(10), 166_250);
        let (lo, hi) = evaluate_microseconds_registers(10);
        assert_eq!(lo, 166_250);
        assert_eq!(hi, 0);

        let delay = evaluate_delay_parameters(5, 0x3000, 100);
        assert!(!delay.is_immediate());
        assert_eq!(delay.deadline(), 105);
        assert!(!delay.is_reached(105));
        assert!(evaluate_delay_reached(105, 105));
        assert!(evaluate_delay_reached(106, 105));
    }
}
