//! Architecture-neutral ownership for Event Manager queue and redraw state.

use std::collections::VecDeque;
use std::ops::{Deref, DerefMut};

/// A queued Mac event (mouseDown, mouseUp, keyDown, etc.).
///
/// The Operating System Event Manager owns one queue, and `EventRecord.where`
/// uses global coordinates. Inside Macintosh Volume I, I-244 and I-259.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct QueuedEvent {
    /// Event type (1=mouseDown, 2=mouseUp, 3=keyDown, etc.).
    pub what: u16,
    /// Event message (key code for key events, window ptr for activate, etc.).
    pub message: u32,
    /// Mouse location at event time, in global Macintosh coordinates.
    pub where_v: i16,
    pub where_h: i16,
    /// Modifier flags.
    pub modifiers: u16,
}

/// Architecture-neutral Event Manager state. It owns the OS event queue and the
/// menu-bar-invalid bit consumed during Toolbox event scans.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct EventQueue {
    events: VecDeque<QueuedEvent>,
    menu_bar_invalid: bool,
}

impl EventQueue {
    #[allow(dead_code)]
    pub fn new() -> Self {
        Self::default()
    }

    pub(crate) fn is_pristine(&self) -> bool {
        self.events.is_empty() && !self.menu_bar_invalid
    }

    /// Mark the menu bar for one deferred redraw by the Toolbox Event
    /// Manager. Repeated invalidations coalesce until the next event scan.
    /// Macintosh Toolbox Essentials (1992), pp. 3-93 and 3-114.
    pub fn invalidate_menu_bar(&mut self) {
        self.menu_bar_invalid = true;
    }

    /// Consume the deferred menu-bar redraw request at an event scan.
    pub fn take_menu_bar_invalidation(&mut self) -> bool {
        std::mem::take(&mut self.menu_bar_invalid)
    }

    #[cfg(test)]
    pub fn menu_bar_is_invalid(&self) -> bool {
        self.menu_bar_invalid
    }

    /// Merge another detached queue into this one by preserving all existing events,
    /// appending the other queue's events, and OR-ing their menu bar invalidation flags.
    pub fn merge(&mut self, mut other: Self) {
        if other.take_menu_bar_invalidation() {
            self.invalidate_menu_bar();
        }
        self.events.append(&mut other.events);
    }
}

impl Deref for EventQueue {
    type Target = VecDeque<QueuedEvent>;

    fn deref(&self) -> &Self::Target {
        &self.events
    }
}

impl DerefMut for EventQueue {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.events
    }
}

impl FromIterator<QueuedEvent> for EventQueue {
    fn from_iter<T: IntoIterator<Item = QueuedEvent>>(iter: T) -> Self {
        Self {
            events: iter.into_iter().collect(),
            menu_bar_invalid: false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clone_is_a_detached_snapshot() {
        let mut live = EventQueue::default();
        live.push_back(QueuedEvent {
            what: 3,
            message: 0x1122_3344,
            where_v: 10,
            where_h: 20,
            modifiers: 0x0100,
        });

        let mut snapshot = live.clone();
        snapshot.front_mut().unwrap().message = 0x5566_7788;
        snapshot.invalidate_menu_bar();

        assert_eq!(live.front().unwrap().message, 0x1122_3344);
        assert_eq!(snapshot.front().unwrap().message, 0x5566_7788);
        assert!(!live.menu_bar_is_invalid());
        assert!(snapshot.menu_bar_is_invalid());
    }

    #[test]
    fn repeated_menu_bar_invalidations_coalesce_until_consumed() {
        let mut queue = EventQueue::default();

        queue.invalidate_menu_bar();
        queue.invalidate_menu_bar();

        assert!(queue.take_menu_bar_invalidation());
        assert!(!queue.take_menu_bar_invalidation());
    }

    #[test]
    fn queue_ordering_and_mutations_are_preserved() {
        let mut queue = EventQueue::default();
        queue.push_back(QueuedEvent {
            what: 1,
            message: 0x100,
            where_v: 1,
            where_h: 2,
            modifiers: 0,
        });
        queue.push_back(QueuedEvent {
            what: 2,
            message: 0x200,
            where_v: 3,
            where_h: 4,
            modifiers: 0,
        });

        assert_eq!(queue.len(), 2);
        assert_eq!(queue.pop_front().map(|e| e.message), Some(0x100));
        assert_eq!(queue.pop_front().map(|e| e.message), Some(0x200));
        assert!(queue.is_empty());
    }

    #[test]
    fn merge_preserves_order_and_combines_invalidation() {
        let mut target = EventQueue::default();
        target.push_back(QueuedEvent {
            what: 1,
            message: 0x111,
            where_v: 1,
            where_h: 2,
            modifiers: 0,
        });

        let mut source = EventQueue::default();
        source.push_back(QueuedEvent {
            what: 2,
            message: 0x222,
            where_v: 3,
            where_h: 4,
            modifiers: 0,
        });
        source.invalidate_menu_bar();

        assert!(!target.menu_bar_is_invalid());
        assert!(source.menu_bar_is_invalid());

        target.merge(source);

        assert_eq!(target.len(), 2);
        assert_eq!(target[0].message, 0x111);
        assert_eq!(target[1].message, 0x222);
        assert!(target.menu_bar_is_invalid());
    }
}
