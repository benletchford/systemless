//! Foreground switch sequencing, independent of either Event Manager ABI.

use crate::loader::ApplicationSizeResource;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ActivationNotification {
    OperatingSystem {
        resume: bool,
        convert_clipboard: bool,
    },
    Window {
        active: bool,
    },
}

impl ActivationNotification {
    pub(crate) fn os_message(self) -> Option<u32> {
        match self {
            Self::OperatingSystem {
                resume,
                convert_clipboard,
            } => {
                // Toolbox Essentials (1992), pp. 2-58--2-59: clipboard
                // conversion is meaningful on resume, not on suspend.
                Some(
                    0x0100_0000 | u32::from(resume) | (u32::from(resume && convert_clipboard) << 1),
                )
            }
            Self::Window { .. } => None,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct Transition {
    foreground: bool,
    operating_system_pending: bool,
    activation_pending: bool,
    primary_delivered: bool,
    yielded: bool,
    convert_clipboard: bool,
}

/// A host request is not itself a guest context switch. The Event Manager
/// starts it at a switching opportunity and acknowledges notifications only
/// when delivered, never when EventAvail merely peeks at them.
/// Processes (1994), "Process Scheduling"; Toolbox Essentials (1992),
/// pp. 2-59--2-61 and 2-117--2-118.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ProcessActivation {
    foreground: bool,
    requested_foreground: bool,
    clipboard_changed: bool,
    clipboard_resume_pending: bool,
    transition: Option<Transition>,
    os_event: Option<crate::event_queue::QueuedEvent>,
}

impl Default for ProcessActivation {
    fn default() -> Self {
        Self {
            foreground: true,
            requested_foreground: true,
            clipboard_changed: false,
            clipboard_resume_pending: false,
            transition: None,
            os_event: None,
        }
    }
}

impl ProcessActivation {
    pub(crate) fn needs_event_service(&self) -> bool {
        self.transition.is_some()
            || self.requested_foreground != self.foreground
            || (self.clipboard_resume_pending && self.foreground && self.requested_foreground)
    }
    pub(crate) fn reset_for_launch(&mut self, policy: Option<ApplicationSizeResource>) {
        *self = Self::default();
        if policy.is_some_and(ApplicationSizeResource::is_background_only) {
            self.foreground = false;
            self.requested_foreground = false;
        }
    }

    pub(crate) fn is_pristine(&self) -> bool {
        self == &Self::default()
    }

    pub(crate) fn request(&mut self, foreground: bool) {
        self.requested_foreground = foreground;
    }

    pub(crate) fn clipboard_changed(&mut self) {
        self.clipboard_changed = true;
        self.clipboard_resume_pending = true;
        // A not-yet-delivered resume can convert the latest imported global
        // scrap. Preserve its original time, pointer and modifier snapshot.
        if let Some(transition) = self.transition.as_mut() {
            if transition.foreground && transition.operating_system_pending {
                transition.convert_clipboard = true;
                if let Some(event) = self.os_event.as_mut() {
                    event.message |= 2;
                }
                self.clipboard_changed = false;
                self.clipboard_resume_pending = false;
            }
        }
    }

    pub(crate) fn is_foreground(&self) -> bool {
        self.foreground
    }

    /// Materialize once at the Event Manager boundary. Repeated peeks must
    /// retain the posting time and pointer/modifier snapshot (Toolbox
    /// Essentials (1992), EventRecord, pp. 2-79--2-80).
    pub(crate) fn prepare_os_event(&mut self, when: u32, position: (i16, i16), modifiers: u16) {
        if self.os_event.is_some() {
            return;
        }
        if let Some(message) = self.peek().and_then(ActivationNotification::os_message) {
            self.os_event = Some(crate::event_queue::QueuedEvent {
                what: 15,
                message,
                when,
                where_v: position.0,
                where_h: position.1,
                modifiers,
            });
        }
    }

    pub(crate) fn peek_os_event(&self, mask: u16) -> Option<crate::event_queue::QueuedEvent> {
        if mask & 0x8000 == 0 {
            None
        } else {
            self.os_event
        }
    }

    /// Called before an event scan. The caller identifies scheduling calls
    /// and defers major switching while ordinary modality or tracking forbids
    /// it. After a suspend has been delivered, the next yielding call commits
    /// the switch; returning that suspend must not stop guest execution first.
    pub(crate) fn begin_event_call(
        &mut self,
        policy: Option<ApplicationSizeResource>,
        yields: bool,
        switching_allowed: bool,
    ) {
        if let Some(transition) = self.transition.as_mut() {
            if yields && switching_allowed && transition.primary_delivered {
                transition.yielded = true;
                self.foreground = transition.foreground;
            }
            if transition.yielded
                && !transition.operating_system_pending
                && !transition.activation_pending
            {
                self.transition = None;
            }
        }
        if !yields || !switching_allowed || self.transition.is_some() {
            return;
        }
        if policy.is_some_and(ApplicationSizeResource::is_background_only) {
            self.foreground = false;
            self.requested_foreground = false;
            self.clipboard_resume_pending = false;
            return;
        }
        let accepts = policy.is_some_and(ApplicationSizeResource::accepts_suspend_resume);
        if self.requested_foreground == self.foreground {
            // The first active host window can import TEXT without a process
            // switch. Let the guest perform its private-scrap conversion using
            // the same resume notification, without suspend or window events.
            if self.clipboard_resume_pending {
                self.clipboard_resume_pending = false;
                if self.foreground && accepts && self.clipboard_changed {
                    self.clipboard_changed = false;
                    self.transition = Some(Transition {
                        foreground: true,
                        operating_system_pending: true,
                        activation_pending: false,
                        primary_delivered: false,
                        yielded: true,
                        convert_clipboard: true,
                    });
                }
            }
            return;
        }
        let needs_activation =
            policy.is_none_or(ApplicationSizeResource::needs_foreground_activation_events);
        let foreground = self.requested_foreground;
        // Resume is delivered after the process returns to the foreground.
        // A legacy application without suspend notification switches here too.
        let yielded = foreground || !accepts;
        if yielded {
            self.foreground = foreground;
        }
        self.transition = Some(Transition {
            foreground,
            operating_system_pending: accepts,
            activation_pending: needs_activation,
            primary_delivered: false,
            yielded,
            convert_clipboard: foreground && self.clipboard_changed,
        });
        if foreground {
            self.clipboard_changed = false;
            self.clipboard_resume_pending = false;
        }
    }

    /// Keep the activation unavailable until its preceding osEvt was consumed.
    /// Queuing both at once would let ordinary activateEvt priority reverse
    /// the required suspend/resume-then-activation order.
    pub(crate) fn peek(&self) -> Option<ActivationNotification> {
        let transition = self.transition?;
        if transition.operating_system_pending {
            Some(ActivationNotification::OperatingSystem {
                resume: transition.foreground,
                convert_clipboard: transition.convert_clipboard,
            })
        } else if transition.activation_pending {
            Some(ActivationNotification::Window {
                active: transition.foreground,
            })
        } else {
            None
        }
    }

    pub(crate) fn consume(&mut self) -> Option<ActivationNotification> {
        let notification = self.peek()?;
        let transition = self.transition.as_mut().unwrap();
        match notification {
            ActivationNotification::OperatingSystem { .. } => {
                transition.operating_system_pending = false;
                self.os_event = None;
            }
            ActivationNotification::Window { .. } => transition.activation_pending = false,
        }
        transition.primary_delivered = true;
        Some(notification)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn policy(flags: u16) -> Option<ApplicationSizeResource> {
        Some(ApplicationSizeResource {
            flags,
            preferred_size: 0,
            minimum_size: 0,
        })
    }

    #[test]
    fn suspend_is_handled_before_yield_and_activation_cannot_overtake_it() {
        let mut state = ProcessActivation::default();
        state.request(false);
        state.begin_event_call(policy(0x4000), true, true);
        let suspend = ActivationNotification::OperatingSystem {
            resume: false,
            convert_clipboard: false,
        };
        assert_eq!(state.peek(), Some(suspend));
        assert_eq!(state.peek(), Some(suspend));
        state.begin_event_call(policy(0x4000), true, true);
        assert!(state.is_foreground(), "peeking did not deliver suspend");
        assert_eq!(state.consume(), Some(suspend));
        assert!(
            state.is_foreground(),
            "the guest must handle suspend before yielding"
        );
        state.begin_event_call(policy(0x4000), false, true);
        assert!(
            state.is_foreground(),
            "a non-yielding call does not suspend execution"
        );
        state.begin_event_call(policy(0x4000), true, true);
        assert!(!state.is_foreground());
        assert_eq!(
            state.consume(),
            Some(ActivationNotification::Window { active: false })
        );
        assert_eq!(state.consume(), None);
    }

    #[test]
    fn owning_activation_requires_suspend_support_and_both_switch_directions_work() {
        for (flags, os, activation) in [
            (0, false, true),
            (0x0800, false, true),
            (0x4000, true, true),
            (0x4800, true, false),
        ] {
            let mut state = ProcessActivation::default();
            for foreground in [false, true] {
                state.request(foreground);
                state.begin_event_call(policy(flags), true, true);
                if os {
                    assert_eq!(
                        state.consume(),
                        Some(ActivationNotification::OperatingSystem {
                            resume: foreground,
                            convert_clipboard: false
                        })
                    );
                }
                if activation {
                    assert_eq!(
                        state.consume(),
                        Some(ActivationNotification::Window { active: foreground })
                    );
                }
                assert_eq!(state.consume(), None);
                state.begin_event_call(policy(flags), true, true);
                assert_eq!(state.is_foreground(), foreground);
            }
        }
    }

    #[test]
    fn cancelled_requests_coalesce_but_delivered_suspend_requires_resume() {
        let mut state = ProcessActivation::default();
        state.request(false);
        state.request(true);
        state.begin_event_call(policy(0x4800), true, true);
        assert_eq!(state.peek(), None);
        state.request(false);
        state.begin_event_call(policy(0x4800), true, true);
        assert_eq!(state.consume().unwrap().os_message(), Some(0x0100_0000));
        state.request(true);
        state.begin_event_call(policy(0x4800), true, true);
        assert_eq!(state.consume().unwrap().os_message(), Some(0x0100_0001));
        assert!(state.is_foreground());
    }

    #[test]
    fn modality_can_defer_a_switch_after_suspend_delivery() {
        let mut state = ProcessActivation::default();
        state.request(false);
        state.begin_event_call(policy(0x4800), true, true);
        state.consume();
        state.begin_event_call(policy(0x4800), true, false);
        assert!(state.is_foreground());
        state.begin_event_call(policy(0x4800), true, true);
        assert!(!state.is_foreground());
    }

    #[test]
    fn background_only_process_cannot_become_foreground() {
        let mut state = ProcessActivation::default();
        state.begin_event_call(policy(0x0400), true, true);
        assert!(!state.is_foreground());
        state.request(true);
        state.begin_event_call(policy(0x0400), true, true);
        assert!(!state.is_foreground());
        assert_eq!(state.peek(), None);
    }

    #[test]
    fn blocked_switch_waits_and_clipboard_conversion_is_reported_only_on_resume() {
        let mut state = ProcessActivation::default();
        state.clipboard_changed();
        state.request(false);
        state.begin_event_call(policy(0x4800), true, false);
        assert_eq!(state.peek(), None);
        assert!(state.is_foreground());
        state.begin_event_call(policy(0x4800), true, true);
        assert_eq!(state.consume().unwrap().os_message(), Some(0x0100_0000));
        state.begin_event_call(policy(0x4800), true, true);
        state.request(true);
        state.begin_event_call(policy(0x4800), true, true);
        assert_eq!(state.consume().unwrap().os_message(), Some(0x0100_0003));
        state.begin_event_call(policy(0x4800), true, true);
        state.request(false);
        state.begin_event_call(policy(0x4800), true, true);
        state.consume();
        state.begin_event_call(policy(0x4800), true, true);
        state.request(true);
        state.begin_event_call(policy(0x4800), true, true);
        assert_eq!(state.consume().unwrap().os_message(), Some(0x0100_0001));
    }

    #[test]
    fn active_clipboard_conversion_defers_and_delivers_once_without_activation() {
        let mut state = ProcessActivation::default();
        state.clipboard_changed();
        assert!(state.needs_event_service());
        for (yields, allowed) in [(false, true), (true, false)] {
            state.begin_event_call(policy(0x4000), yields, allowed);
            assert_eq!(state.peek(), None);
            assert!(state.is_foreground());
            assert!(state.needs_event_service());
        }
        state.begin_event_call(policy(0x4000), true, true);
        assert!(state.is_foreground());
        assert_eq!(state.consume().unwrap().os_message(), Some(0x0100_0003));
        assert_eq!(state.consume(), None);
        state.begin_event_call(policy(0x4000), true, true);
        assert!(!state.needs_event_service());
        assert!(state.is_foreground());
    }

    #[test]
    fn clipboard_import_upgrades_pending_resume_without_reposting() {
        let mut state = ProcessActivation::default();
        state.request(false);
        state.begin_event_call(policy(0x4800), true, true);
        state.consume();
        state.begin_event_call(policy(0x4800), true, true);
        assert!(!state.needs_event_service());
        state.request(true);
        state.begin_event_call(policy(0x4800), true, true);
        state.prepare_os_event(123, (17, 29), 0x100);
        let original = state.peek_os_event(0x8000).unwrap();
        assert_eq!(original.message, 0x0100_0001);
        for _ in 0..2 {
            state.clipboard_changed();
            state.prepare_os_event(456, (31, 43), 0);
        }
        let upgraded = state.peek_os_event(0x8000).unwrap();
        assert_eq!(upgraded.message, 0x0100_0003);
        assert_eq!(upgraded.when, original.when);
        assert_eq!(upgraded.where_v, original.where_v);
        assert_eq!(upgraded.where_h, original.where_h);
        assert_eq!(upgraded.modifiers, original.modifiers);
        state.consume();
        state.begin_event_call(policy(0x4800), true, true);
        assert!(!state.needs_event_service());
    }

    #[test]
    fn unsupported_clipboard_notifications_do_not_keep_event_service_busy() {
        for flags in [0, 0x0800, 0x0400, 0x4400] {
            let mut state = ProcessActivation::default();
            state.clipboard_changed();
            state.begin_event_call(policy(flags), true, true);
            assert_eq!(state.peek(), None);
            assert!(!state.needs_event_service(), "SIZE flags {flags:#06x}");
            assert_eq!(state.is_foreground(), flags & 0x0400 == 0);
        }
    }
}
