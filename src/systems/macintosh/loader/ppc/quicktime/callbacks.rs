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
        self.queued.clear();
        for record in &mut self.records {
            if record.time_base == time_base {
                record.time_base = 0;
                record.registration = None;
                record.due = false;
            }
        }
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
                    flags == 0
                        || (flags & 4 != 0 && after < trigger)
                        || (flags & 8 != 0 && after > trigger)
                        || (flags & 16 != 0 && after == trigger)
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
