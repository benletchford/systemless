//! Owned QuickTime callback registrations. Delivery removes the registration
//! before entering guest code, so a callback can safely rearm itself.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Registration {
    pub procedure: u32,
    pub rtoc: u32,
    pub refcon: u32,
    pub parameters: [i32; 3],
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Callback {
    pub address: u32,
    pub time_base: u32,
    pub kind: u16,
    registration: Option<Registration>,
    due: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct Callbacks {
    records: Vec<Callback>,
    /// One-shot deliveries that are not QTCallBack records, such as a
    /// movie's drawing-complete procedure: (first argument, registration).
    queued: Vec<(u32, Registration)>,
}

/// A single continuation stack preserves ordering when a controller filter
/// calls a movie import which in turn delivers a time/drawing callback.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ContinuationFrame {
    Delivery(DeliveryFrame),
    ControllerFilter(super::controller::FilterFrame),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct DeliveryFrame {
    pub import_pc: u32,
    pub final_pc: u32,
    pub restore_rtoc: u32,
    pub result: u32,
    pub dispose_movie: bool,
    pub dispose_time_base: Option<u32>,
}

#[derive(Clone, Copy)]
pub(crate) enum Event {
    Rate { before: i32, after: i32 },
    Jump,
    Extremes { start: bool, stop: bool },
    Disposed,
}

impl Callbacks {
    pub fn create(&mut self, address: u32, time_base: u32, kind: u16) -> bool {
        if address == 0
            || time_base == 0
            || !(1..=5).contains(&(kind & 0x3fff))
            || self.records.iter().any(|record| record.address == address)
        {
            return false;
        }
        self.records.push(Callback {
            address,
            time_base,
            kind,
            registration: None,
            due: false,
        });
        true
    }

    pub fn get(&self, address: u32) -> Option<&Callback> {
        self.records.iter().find(|record| record.address == address)
    }

    pub fn arm(&mut self, address: u32, registration: Registration) -> bool {
        if registration.procedure == 0 {
            return false;
        }
        let Some(record) = self
            .records
            .iter_mut()
            .find(|record| record.address == address)
        else {
            return false;
        };
        if record.time_base == 0 {
            return false;
        }
        record.registration = Some(registration);
        record.due = false;
        true
    }

    pub fn cancel(&mut self, address: u32) {
        if let Some(record) = self
            .records
            .iter_mut()
            .find(|record| record.address == address)
        {
            record.registration = None;
            record.due = false;
        }
    }

    pub fn dispose(&mut self, address: u32) -> bool {
        let before = self.records.len();
        self.records.retain(|record| record.address != address);
        self.records.len() != before
    }

    pub fn retire_time_base(&mut self, time_base: u32) {
        for record in &mut self.records {
            if record.time_base == time_base {
                record.time_base = 0;
                record.registration = None;
                record.due = false;
            }
        }
    }

    pub fn clear_queued(&mut self) {
        self.queued.clear();
    }

    // Inside Macintosh: QuickTime (1993), pp. 2-337–2-338:
    // param1 contains direction flags, param2 the signed time, param3 its scale.
    // Cross-products preserve signed fractional times without floating rounding.
    pub fn mark_time_due(&mut self, time_base: u32, before: i64, after: i64, scale: u32) {
        if scale == 0 {
            return;
        }
        for record in &mut self.records {
            if record.time_base != time_base || record.kind & 0x3fff != 1 {
                continue;
            }
            let Some(registration) = record.registration else {
                continue;
            };
            let [direction, time, trigger_scale] = registration.parameters;
            if trigger_scale <= 0 {
                continue;
            }
            let trigger = i128::from(time) * i128::from(scale);
            let before = i128::from(before) * i128::from(trigger_scale);
            let after = i128::from(after) * i128::from(trigger_scale);
            // Movies.h defines the forward/backward triggers as passing the
            // specified time, not merely reaching it. At-extremes is separate.
            let forward = direction & 1 != 0 && before <= trigger && after > trigger;
            let backward = direction & 2 != 0 && before >= trigger && after < trigger;
            if forward || backward {
                record.due = true;
            }
        }
    }

    pub fn mark_loop_time_due(
        &mut self,
        time_base: u32,
        before: i64,
        after: i64,
        start: i64,
        stop: i64,
        scale: u32,
    ) {
        let width = i128::from(stop) - i128::from(start);
        if width <= 0 || before == after {
            return;
        }
        let relative_before = i128::from(before) - i128::from(start);
        let relative_after = i128::from(after) - i128::from(start);
        let cycles = relative_after.div_euclid(width) - relative_before.div_euclid(width);
        let first = (i128::from(start) + relative_before.rem_euclid(width)) as i64;
        let last = (i128::from(start) + relative_after.rem_euclid(width)) as i64;
        if cycles == 0 {
            self.mark_time_due(time_base, first, last, scale);
        } else if after > before {
            self.mark_time_due(time_base, first, stop, scale);
            if cycles > 1 {
                self.mark_time_due(time_base, start, stop, scale);
            }
            self.mark_time_due(time_base, start, last, scale);
        } else {
            self.mark_time_due(time_base, first, start, scale);
            if cycles < -1 {
                self.mark_time_due(time_base, stop, start, scale);
            }
            self.mark_time_due(time_base, stop, last, scale);
        }
    }

    pub fn mark_palindrome_time_due(
        &mut self,
        time_base: u32,
        before: i64,
        after: i64,
        start: i64,
        stop: i64,
        scale: u32,
    ) {
        let width = i128::from(stop) - i128::from(start);
        if width <= 0 || before == after {
            return;
        }
        let target = i128::from(after) - i128::from(start);
        let mut position = i128::from(before) - i128::from(start);
        let forward = target > position;
        let display = |position: i128| {
            let phase = position.rem_euclid(width * 2);
            (i128::from(start)
                + if phase <= width {
                    phase
                } else {
                    width * 2 - phase
                }) as i64
        };
        // Three complete legs cover both directions and every trigger even
        // when a single service interval spans arbitrarily many periods.
        for _ in 0..4 {
            if position == target {
                break;
            }
            let quotient = position.div_euclid(width);
            let boundary = if forward {
                (quotient + 1) * width
            } else if position.rem_euclid(width) == 0 {
                (quotient - 1) * width
            } else {
                quotient * width
            };
            let next = if forward {
                target.min(boundary)
            } else {
                target.max(boundary)
            };
            self.mark_time_due(time_base, display(position), display(next), scale);
            position = next;
        }
    }

    pub fn mark_event(&mut self, time_base: u32, event: Event) {
        for record in &mut self.records {
            if record.time_base != time_base {
                continue;
            }
            let Some(registration) = record.registration else {
                continue;
            };
            let flags = registration.parameters[0];
            let matches = match (record.kind & 0x3fff, event) {
                (2, Event::Rate { before, after }) if before != after => {
                    let trigger = registration.parameters[1];
                    if flags == 0x1c {
                        after != trigger
                    } else {
                        flags == 0
                            || (flags & 4 != 0 && after < trigger)
                            || (flags & 8 != 0 && after > trigger)
                            || (flags & 16 != 0 && after == trigger)
                    }
                }
                (3, Event::Jump) | (5, Event::Disposed) => true,
                (4, Event::Extremes { start, stop }) => {
                    (start && flags & 1 != 0) || (stop && flags & 2 != 0)
                }
                _ => false,
            };
            record.due |= matches;
        }
    }

    /// Queues a one-shot delivery of `registration` with `argument` as the
    /// procedure's first parameter. A delivery already pending is not
    /// duplicated.
    pub fn queue(&mut self, argument: u32, registration: Registration) {
        if !self.queued.contains(&(argument, registration)) {
            self.queued.push((argument, registration));
        }
    }

    pub fn take_ready(&mut self) -> Option<(u32, Registration)> {
        if !self.queued.is_empty() {
            return Some(self.queued.remove(0));
        }
        let record = self.records.iter_mut().find(|record| record.due)?;
        record.due = false;
        Some((record.address, record.registration.take()?))
    }

    pub fn take_disposal_ready(&mut self, time_base: u32) -> Option<(u32, Registration)> {
        let record = self.records.iter_mut().find(|record| {
            record.time_base == time_base && record.kind & 0x3fff == 5 && record.due
        })?;
        record.due = false;
        Some((record.address, record.registration.take()?))
    }

    #[cfg(test)]
    fn take_time_due(
        &mut self,
        time_base: u32,
        before: i64,
        after: i64,
        scale: u32,
    ) -> Option<(u32, Registration)> {
        self.mark_time_due(time_base, before, after, scale);
        self.take_ready()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn registration(direction: i32) -> Registration {
        Registration {
            procedure: 0x1000,
            rtoc: 0x2000,
            refcon: 0xdeadbeef,
            parameters: [direction, 1, 3],
        }
    }

    #[test]
    fn rate_callback_comparisons_include_not_equal() {
        for (flags, expected) in [
            (0, [true, true, true]),
            (4, [true, false, false]),
            (8, [false, false, true]),
            (16, [false, true, false]),
            (20, [true, true, false]),
            (24, [false, true, true]),
            (28, [true, false, true]),
        ] {
            for (after, should_fire) in [-1, 0, 1].into_iter().zip(expected) {
                let mut callbacks = Callbacks::default();
                let mut trigger = registration(flags);
                trigger.parameters[1] = 0;
                assert!(callbacks.create(1, 2, 2));
                assert!(callbacks.arm(1, trigger));
                callbacks.mark_event(2, Event::Rate { before: 2, after });
                assert_eq!(
                    callbacks.take_ready().is_some(),
                    should_fire,
                    "flags={flags}, after={after}"
                );
            }
        }
    }

    #[test]
    fn looping_time_base_crossings_follow_travel_direction() {
        for (before, after, direction, expected) in [
            (8, 12, 1, true),
            (8, 12, 2, false),
            (2, -2, 2, true),
            (2, -2, 1, false),
            (2, 22, 1, true),
            (8, -12, 2, true),
            (2, 3, 1, false),
        ] {
            let mut callbacks = Callbacks::default();
            assert!(callbacks.create(1, 2, 1));
            let mut trigger = registration(direction);
            trigger.parameters = [direction, 1, 10];
            assert!(callbacks.arm(1, trigger));
            callbacks.mark_loop_time_due(2, before, after, 0, 10, 10);
            assert_eq!(
                callbacks.take_ready().is_some(),
                expected,
                "before={before}, after={after}, direction={direction}"
            );
        }
    }

    #[test]
    fn palindrome_time_base_crossings_reverse_at_each_end() {
        for (before, after, direction, expected) in [
            (2, 18, 1, true),
            (2, 18, 2, true),
            (7, 13, 1, false),
            (7, 13, 2, false),
            (18, 2, 1, true),
            (18, 2, 2, true),
            (18, 14, 1, true),
            (18, 14, 2, false),
            (2, 102, 3, true),
            (2, -98, 3, true),
        ] {
            let mut callbacks = Callbacks::default();
            assert!(callbacks.create(1, 2, 1));
            let mut trigger = registration(direction);
            trigger.parameters = [direction, 5, 10];
            assert!(callbacks.arm(1, trigger));
            callbacks.mark_palindrome_time_due(2, before, after, 0, 10, 10);
            assert_eq!(
                callbacks.take_ready().is_some(),
                expected,
                "before={before}, after={after}, direction={direction}"
            );
        }
    }

    #[test]
    fn crossing_uses_trigger_scale_and_delivers_once() {
        let mut callbacks = Callbacks::default();
        assert!(callbacks.create(0x3000, 0x4000, 1));
        assert!(callbacks.arm(0x3000, registration(1)));
        assert_eq!(callbacks.take_time_due(0x4000, 0, 2, 6), None);
        assert_eq!(
            callbacks.take_time_due(0x4000, 2, 3, 6),
            Some((0x3000, registration(1)))
        );
        assert_eq!(callbacks.take_time_due(0x4000, 2, 4, 6), None);
        assert!(callbacks.get(0x3000).is_some());
    }

    #[test]
    fn time_triggers_require_passing_the_target_in_the_selected_direction() {
        let mut callbacks = Callbacks::default();
        assert!(callbacks.create(0x3000, 0x4000, 1));
        let mut endpoint = registration(1);
        endpoint.parameters = [1, 600, 600];
        assert!(callbacks.arm(0x3000, endpoint));
        assert_eq!(callbacks.take_time_due(0x4000, 500, 500, 600), None);
        assert_eq!(callbacks.take_time_due(0x4000, 500, 600, 600), None);
        assert_eq!(
            callbacks.take_time_due(0x4000, 600, 601, 600),
            Some((0x3000, endpoint))
        );
        assert_eq!(callbacks.take_time_due(0x4000, 600, 601, 600), None);
        let mut backward = registration(2);
        backward.parameters = [2, -1, 3];
        assert!(callbacks.arm(0x3000, backward));
        assert_eq!(callbacks.take_time_due(0x4000, 0, -2, 6), None);
        assert_eq!(
            callbacks.take_time_due(0x4000, -2, -3, 6),
            Some((0x3000, backward))
        );
        assert!(callbacks.arm(0x3000, backward));
        assert_eq!(callbacks.take_time_due(0x4000, -2, -2, 6), None);
    }

    #[test]
    fn time_base_and_direction_are_isolated_and_signed_times_are_preserved() {
        let mut callbacks = Callbacks::default();
        assert!(callbacks.create(0x3000, 0x4000, 1));
        let mut registration = registration(2);
        registration.parameters = [2, -1, 3];
        assert!(callbacks.arm(0x3000, registration));
        assert_eq!(callbacks.take_time_due(0x5000, 0, -3, 6), None);
        assert_eq!(callbacks.take_time_due(0x4000, -3, 0, 6), None);
        assert_eq!(callbacks.take_time_due(0x4000, 0, -3, 0), None);
        assert_eq!(
            callbacks.take_time_due(0x4000, 0, -3, 6),
            Some((0x3000, registration))
        );
        // Delivery disarms before guest entry; callback code can register again.
        assert!(callbacks.arm(0x3000, registration));
        assert_eq!(
            callbacks.take_time_due(0x4000, 0, -3, 6),
            Some((0x3000, registration))
        );
    }

    #[test]
    fn rate_jump_extremes_and_disposal_mark_only_their_registered_events() {
        let mut callbacks = Callbacks::default();
        for kind in 2..=5 {
            assert!(callbacks.create(0x3000 + u32::from(kind), 0x4000, kind));
            let mut registration = registration(1);
            registration.parameters = if kind == 2 {
                [16, 0x10000, 0]
            } else {
                [2, 0, 0]
            };
            assert!(callbacks.arm(0x3000 + u32::from(kind), registration));
        }
        callbacks.mark_event(
            0x4000,
            Event::Rate {
                before: 0,
                after: 0x8000,
            },
        );
        assert_eq!(callbacks.take_ready(), None);
        callbacks.mark_event(
            0x4000,
            Event::Rate {
                before: 0x8000,
                after: 0x10000,
            },
        );
        assert_eq!(callbacks.take_ready().unwrap().0, 0x3002);
        callbacks.mark_event(0x4000, Event::Jump);
        assert_eq!(callbacks.take_ready().unwrap().0, 0x3003);
        callbacks.mark_event(
            0x4000,
            Event::Extremes {
                start: true,
                stop: false,
            },
        );
        assert_eq!(callbacks.take_ready(), None);
        callbacks.mark_event(
            0x4000,
            Event::Extremes {
                start: false,
                stop: true,
            },
        );
        callbacks.cancel(0x3004);
        assert_eq!(callbacks.take_ready(), None);
        callbacks.mark_event(0x5000, Event::Disposed);
        assert_eq!(callbacks.take_ready(), None);
        callbacks.mark_event(0x4000, Event::Disposed);
        assert_eq!(callbacks.take_ready().unwrap().0, 0x3005);
        assert_eq!(callbacks.take_ready(), None);
    }

    #[test]
    fn retired_time_base_cannot_deliver_into_a_replacement_movie() {
        let mut callbacks = Callbacks::default();
        assert!(callbacks.create(0x3000, 0x4000, 1));
        assert!(callbacks.arm(0x3000, registration(1)));
        callbacks.mark_time_due(0x4000, 0, 3, 6);
        callbacks.retire_time_base(0x4000);
        assert_eq!(callbacks.take_ready(), None);
        assert!(!callbacks.arm(0x3000, registration(1)));
        assert!(callbacks.create(0x3004, 0x4000, 1));
        assert!(callbacks.arm(0x3004, registration(1)));
        callbacks.mark_time_due(0x4000, 0, 3, 6);
        assert_eq!(callbacks.take_ready().unwrap().0, 0x3004);
        assert!(callbacks.dispose(0x3000));
    }

    #[test]
    fn retiring_one_time_base_preserves_unrelated_queued_callbacks() {
        let mut callbacks = Callbacks::default();
        assert!(callbacks.create(0x3000, 0x4000, 1));
        assert!(callbacks.arm(0x3000, registration(1)));
        callbacks.queue(0x5000, registration(2));
        callbacks.retire_time_base(0x4000);
        assert_eq!(callbacks.take_ready().unwrap().0, 0x5000);
        assert_eq!(callbacks.take_ready(), None);
        assert!(!callbacks.arm(0x3000, registration(1)));
    }

    #[test]
    fn cancellation_preserves_reusable_record_and_disposal_prevents_late_delivery() {
        let mut callbacks = Callbacks::default();
        assert!(callbacks.create(0x3000, 0x4000, 1));
        assert!(callbacks.arm(0x3000, registration(3)));
        callbacks.cancel(0x3000);
        assert_eq!(callbacks.take_time_due(0x4000, 0, 3, 6), None);
        assert!(callbacks.arm(0x3000, registration(2)));
        assert_eq!(
            callbacks.take_time_due(0x4000, 3, 1, 6),
            Some((0x3000, registration(2)))
        );
        assert!(callbacks.arm(0x3000, registration(3)));
        assert!(callbacks.dispose(0x3000));
        assert!(!callbacks.arm(0x3000, registration(3)));
        assert_eq!(callbacks.take_time_due(0x4000, 0, 3, 6), None);
    }
}
