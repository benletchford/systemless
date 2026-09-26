//! Ordered host commands and bounded state handoff for the desktop worker.

use std::collections::VecDeque;
use std::sync::{Condvar, Mutex};
use std::time::{Duration, Instant};

use super::InputAction;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum WorkerCommand {
    Input(InputAction),
    MenuSelection {
        menu_id: i16,
        item_number: i16,
        generation: u32,
    },
    ProbeInputService {
        sent: Instant,
    },
    SetOutlineScale(u32),
    Shutdown,
}

#[derive(Default)]
pub(super) struct CommandMailbox {
    pending: Mutex<VecDeque<WorkerCommand>>,
    changed: Condvar,
}

impl CommandMailbox {
    pub(super) fn push(&self, command: WorkerCommand) {
        let mut pending = self.pending.lock().unwrap();
        // Only consecutive pointer positions or presentation scales are
        // replaceable. A transition or menu selection is an ordering boundary.
        if (matches!(command, WorkerCommand::Input(InputAction::MouseMove { .. }))
            && matches!(
                pending.back(),
                Some(WorkerCommand::Input(InputAction::MouseMove { .. }))
            ))
            || (matches!(command, WorkerCommand::SetOutlineScale(_))
                && matches!(pending.back(), Some(WorkerCommand::SetOutlineScale(_))))
        {
            *pending.back_mut().unwrap() = command;
        } else {
            pending.push_back(command);
        }
        self.changed.notify_one();
    }

    pub(super) fn drain(&self) -> Vec<WorkerCommand> {
        self.pending.lock().unwrap().drain(..).collect()
    }

    pub(super) fn wait_until(&self, timeout: Duration) -> Vec<WorkerCommand> {
        let pending = self.pending.lock().unwrap();
        let (mut pending, _) = self
            .changed
            .wait_timeout_while(pending, timeout, |queue| queue.is_empty())
            .unwrap();
        pending.drain(..).collect()
    }
}

/// One pending immutable state value. Production may replace an unconsumed
/// frame without extending a queue or waiting for presentation.
pub(super) struct LatestSlot<T> {
    pending: Mutex<Option<T>>,
}

impl<T> Default for LatestSlot<T> {
    fn default() -> Self {
        Self {
            pending: Mutex::new(None),
        }
    }
}

impl<T> LatestSlot<T> {
    pub(super) fn replace(&self, value: T) {
        let replaced = self.pending.lock().unwrap().replace(value);
        drop(replaced);
    }

    pub(super) fn take(&self) -> Option<T> {
        self.pending.lock().unwrap().take()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    #[test]
    fn transitions_and_menu_selections_keep_their_order() {
        let commands = CommandMailbox::default();
        commands.push(WorkerCommand::Input(InputAction::MouseMove { v: 1, h: 2 }));
        commands.push(WorkerCommand::Input(InputAction::MouseMove { v: 3, h: 4 }));
        commands.push(WorkerCommand::Input(InputAction::MouseDown { v: 3, h: 4 }));
        commands.push(WorkerCommand::Input(InputAction::MouseMove { v: 5, h: 6 }));
        commands.push(WorkerCommand::MenuSelection {
            menu_id: 1,
            item_number: 2,
            generation: 3,
        });
        commands.push(WorkerCommand::Input(InputAction::MouseUp { v: 5, h: 6 }));
        assert_eq!(
            commands.drain(),
            vec![
                WorkerCommand::Input(InputAction::MouseMove { v: 3, h: 4 }),
                WorkerCommand::Input(InputAction::MouseDown { v: 3, h: 4 }),
                WorkerCommand::Input(InputAction::MouseMove { v: 5, h: 6 }),
                WorkerCommand::MenuSelection {
                    menu_id: 1,
                    item_number: 2,
                    generation: 3
                },
                WorkerCommand::Input(InputAction::MouseUp { v: 5, h: 6 }),
            ]
        );
    }

    #[test]
    fn shutdown_wakes_an_idle_worker() {
        let commands = Arc::new(CommandMailbox::default());
        let worker = Arc::clone(&commands);
        let handle = std::thread::spawn(move || worker.wait_until(Duration::from_secs(2)));
        commands.push(WorkerCommand::Shutdown);
        assert_eq!(handle.join().unwrap(), vec![WorkerCommand::Shutdown]);
    }

    #[test]
    fn latest_snapshot_replaces_an_unconsumed_frame() {
        let slot = LatestSlot::default();
        slot.replace(1);
        slot.replace(2);
        assert_eq!(slot.take(), Some(2));
        assert_eq!(slot.take(), None);
    }

    #[test]
    fn presentation_scale_coalesces_without_crossing_an_input_transition() {
        let commands = CommandMailbox::default();
        commands.push(WorkerCommand::SetOutlineScale(2));
        commands.push(WorkerCommand::SetOutlineScale(3));
        commands.push(WorkerCommand::Input(InputAction::KeyDown {
            key: 4,
            ch: 65,
        }));
        commands.push(WorkerCommand::SetOutlineScale(1));
        assert_eq!(
            commands.drain(),
            vec![
                WorkerCommand::SetOutlineScale(3),
                WorkerCommand::Input(InputAction::KeyDown { key: 4, ch: 65 }),
                WorkerCommand::SetOutlineScale(1),
            ]
        );
    }
}
