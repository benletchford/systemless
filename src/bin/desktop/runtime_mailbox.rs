//! Bounded transport between the window thread and the guest execution owner.
//! Locks only protect owned values. Guest work, rendering, destruction of
//! discarded frames and host wake callbacks all happen outside the lock.

use super::frame_snapshot::GuiFrame;
use super::runtime_protocol::{GuiCommand, GuiState};
use std::collections::VecDeque;
use std::sync::{Condvar, Mutex};
use std::time::{Duration, Instant};

const COMMAND_CAPACITY: usize = 512;
const RECYCLED_FRAME_CAPACITY: usize = 2;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum RuntimeStatus {
    Starting,
    Ready,
    Stopped {
        error: Option<String>,
        instructions: u64,
    },
}

#[derive(Clone, Copy, Default)]
pub(super) struct PresentationOptions {
    pub debug: Option<systemless::debug_overlay::DebugOverlayFrameStats>,
    pub capture_crop: bool,
    pub learning_crop: bool,
    pub render_headroom: Option<Duration>,
    pub force: bool,
}

pub(super) struct HostUpdate {
    pub frame: Option<Box<GuiFrame>>,
    pub state: Option<GuiState>,
    pub status: RuntimeStatus,
}

struct Shared {
    commands: VecDeque<GuiCommand>,
    shutdown: bool,
    presentation: Option<PresentationOptions>,
    frame: Option<Box<GuiFrame>>,
    recycled: Vec<Box<GuiFrame>>,
    state: Option<GuiState>,
    status: RuntimeStatus,
    wake_pending: bool,
}

pub(super) struct RuntimeMailbox {
    shared: Mutex<Shared>,
    changed: Condvar,
    wake_host: Box<dyn Fn() + Send + Sync>,
}

#[derive(Debug, PartialEq, Eq)]
pub(super) enum CommandError {
    Full(GuiCommand),
    Closed(GuiCommand),
}

impl RuntimeMailbox {
    pub fn new(wake_host: impl Fn() + Send + Sync + 'static) -> Self {
        Self {
            shared: Mutex::new(Shared {
                commands: VecDeque::with_capacity(COMMAND_CAPACITY),
                shutdown: false,
                presentation: None,
                frame: None,
                recycled: Vec::with_capacity(RECYCLED_FRAME_CAPACITY),
                state: None,
                status: RuntimeStatus::Starting,
                wake_pending: false,
            }),
            changed: Condvar::new(),
            wake_host: Box::new(wake_host),
        }
    }

    /// Never wait for guest execution on the host. A full queue is an explicit
    /// error; callers must surface it rather than silently dropping a transition.
    pub fn send(&self, command: GuiCommand) -> Result<(), CommandError> {
        let mut shared = self.shared.lock().unwrap();
        if shared.shutdown || matches!(shared.status, RuntimeStatus::Stopped { .. }) {
            return Err(CommandError::Closed(command));
        }
        if matches!(command, GuiCommand::MouseMove { .. }) {
            if let Some(last @ GuiCommand::MouseMove { .. }) = shared.commands.back_mut() {
                *last = command;
                drop(shared);
                self.changed.notify_one();
                return Ok(());
            }
        }
        if shared.commands.len() == COMMAND_CAPACITY {
            return Err(CommandError::Full(command));
        }
        shared.commands.push_back(command);
        drop(shared);
        self.changed.notify_one();
        Ok(())
    }

    /// Reserved shutdown capacity is independent of the command queue. Already
    /// accepted commands remain available to the owner before its final flush.
    pub fn request_shutdown(&self) {
        self.shared.lock().unwrap().shutdown = true;
        self.changed.notify_one();
    }

    pub fn request_presentation(&self, mut options: PresentationOptions) {
        {
            let mut shared = self.shared.lock().unwrap();
            options.force |= shared.presentation.is_some_and(|old| old.force);
            shared.presentation = Some(options);
        }
        self.changed.notify_one();
    }

    pub fn take_presentation(&self) -> Option<PresentationOptions> {
        self.shared.lock().unwrap().presentation.take()
    }

    pub fn shutdown_requested(&self) -> bool {
        self.shared.lock().unwrap().shutdown
    }

    pub fn next_command(&self) -> Option<GuiCommand> {
        self.shared.lock().unwrap().commands.pop_front()
    }

    /// Sleep only on the execution owner, waking promptly for input or shutdown.
    #[cfg(test)]
    pub fn wait_until(&self, deadline: Instant) {
        self.wait_until_accepting(deadline, true);
    }

    pub fn wait_until_accepting(&self, deadline: Instant, accept_commands: bool) {
        let mut shared = self.shared.lock().unwrap();
        while (!accept_commands || shared.commands.is_empty())
            && shared.presentation.is_none()
            && !shared.shutdown
        {
            let now = Instant::now();
            if now >= deadline {
                break;
            }
            let (next, timeout) = self.changed.wait_timeout(shared, deadline - now).unwrap();
            shared = next;
            if timeout.timed_out() {
                break;
            }
        }
    }

    pub fn frame_buffer(&self) -> Box<GuiFrame> {
        let frame = self.shared.lock().unwrap().recycled.pop();
        frame.unwrap_or_default()
    }

    pub fn recycle(&self, frame: Box<GuiFrame>) {
        let mut frame = Some(frame);
        {
            let mut shared = self.shared.lock().unwrap();
            if shared.recycled.len() < RECYCLED_FRAME_CAPACITY {
                shared.recycled.push(frame.take().unwrap());
            }
        }
        drop(frame);
    }

    fn arm_wake(shared: &mut Shared) -> bool {
        !std::mem::replace(&mut shared.wake_pending, true)
    }

    pub fn publish(&self, frame: Option<Box<GuiFrame>>, state: GuiState) {
        let (discarded_frame, discarded_state, wake) = {
            let mut shared = self.shared.lock().unwrap();
            let mut previous = frame.and_then(|frame| shared.frame.replace(frame));
            if shared.recycled.len() < RECYCLED_FRAME_CAPACITY {
                if let Some(frame) = previous.take() {
                    shared.recycled.push(frame);
                }
            }
            let old_state = shared.state.replace(state);
            (previous, old_state, Self::arm_wake(&mut shared))
        };
        drop((discarded_frame, discarded_state));
        if wake {
            (self.wake_host)();
        }
    }

    pub fn set_status(&self, status: RuntimeStatus) {
        let (old, wake) = {
            let mut shared = self.shared.lock().unwrap();
            let old = std::mem::replace(&mut shared.status, status);
            (old, Self::arm_wake(&mut shared))
        };
        drop(old);
        if wake {
            (self.wake_host)();
        }
    }

    pub fn poll(&self) -> HostUpdate {
        let mut shared = self.shared.lock().unwrap();
        shared.wake_pending = false;
        HostUpdate {
            frame: shared.frame.take(),
            state: shared.state.take(),
            status: shared.status.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    };
    use std::time::Duration;

    #[test]
    fn movement_coalescing_never_crosses_an_ordered_transition() {
        let mailbox = RuntimeMailbox::new(|| {});
        let commands = [
            GuiCommand::MouseMove { v: 1, h: 2 },
            GuiCommand::MouseMove { v: 3, h: 4 },
            GuiCommand::MouseDown { v: 3, h: 4 },
            GuiCommand::MouseMove { v: 5, h: 6 },
            GuiCommand::MouseUp { v: 7, h: 8 },
            GuiCommand::KeyDown {
                key: 12,
                character: b'q',
            },
            GuiCommand::Menu { menu: 128, item: 1 },
            GuiCommand::KeyUp {
                key: 12,
                character: b'q',
            },
            GuiCommand::MouseMove { v: 9, h: 10 },
            GuiCommand::MouseMove { v: 11, h: 12 },
        ];
        for command in &commands {
            mailbox.send(command.clone()).unwrap();
        }
        let mut received = Vec::new();
        while let Some(command) = mailbox.next_command() {
            received.push(command);
        }
        assert_eq!(
            received,
            commands[1..8]
                .iter()
                .cloned()
                .chain([commands[9].clone()])
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn full_queue_preserves_accepted_commands_and_reserves_shutdown() {
        let mailbox = RuntimeMailbox::new(|| {});
        for index in 0..COMMAND_CAPACITY {
            mailbox
                .send(GuiCommand::Menu {
                    menu: index as i16,
                    item: 1,
                })
                .unwrap();
        }
        let extra = GuiCommand::MouseUp { v: 17, h: 19 };
        assert_eq!(
            mailbox.send(extra.clone()),
            Err(CommandError::Full(extra.clone()))
        );
        mailbox.request_shutdown();
        assert!(mailbox.shutdown_requested());
        assert_eq!(
            mailbox.send(extra.clone()),
            Err(CommandError::Closed(extra))
        );
        for index in 0..COMMAND_CAPACITY {
            assert_eq!(
                mailbox.next_command(),
                Some(GuiCommand::Menu {
                    menu: index as i16,
                    item: 1
                })
            );
        }
        assert_eq!(mailbox.next_command(), None);
    }

    #[test]
    fn slow_host_keeps_only_newest_complete_frame_and_one_wake() {
        let wakes = Arc::new(AtomicUsize::new(0));
        let count = wakes.clone();
        let mailbox = RuntimeMailbox::new(move || {
            count.fetch_add(1, Ordering::Relaxed);
        });
        for sequence in 1..=1000 {
            let mut frame = mailbox.frame_buffer();
            frame.generation = 7;
            frame.sequence = sequence;
            frame.screen.pixels = vec![sequence as u8; 16];
            mailbox.publish(Some(frame), GuiState::default());
        }
        assert_eq!(wakes.load(Ordering::Relaxed), 1);
        let frame = mailbox.poll().frame.unwrap();
        assert_eq!((frame.generation, frame.sequence), (7, 1000));
        assert_eq!(frame.screen.pixels, vec![1000u16 as u8; 16]);
        mailbox.recycle(frame);
        for _ in 0..10 {
            mailbox.recycle(Box::default());
        }
        assert_eq!(
            mailbox.shared.lock().unwrap().recycled.len(),
            RECYCLED_FRAME_CAPACITY
        );
        mailbox.set_status(RuntimeStatus::Stopped {
            error: None,
            instructions: 42,
        });
        assert_eq!(wakes.load(Ordering::Relaxed), 2);
        assert_eq!(
            mailbox.poll().status,
            RuntimeStatus::Stopped {
                error: None,
                instructions: 42
            }
        );
        assert!(matches!(
            mailbox.send(GuiCommand::MouseMove { v: 0, h: 0 }),
            Err(CommandError::Closed(_))
        ));
    }

    #[test]
    fn metadata_and_presentation_requests_do_not_discard_pending_pixels() {
        let mailbox = RuntimeMailbox::new(|| {});
        let mut frame = Box::<GuiFrame>::default();
        frame.generation = 3;
        frame.sequence = 7;
        mailbox.publish(Some(frame), GuiState::default());
        let mut state = GuiState::default();
        state.generation = 3;
        mailbox.publish(None, state);
        mailbox.request_presentation(PresentationOptions {
            force: true,
            ..Default::default()
        });
        mailbox.request_presentation(PresentationOptions {
            learning_crop: true,
            ..Default::default()
        });
        let options = mailbox.take_presentation().unwrap();
        assert!(options.force && options.learning_crop);
        assert!(mailbox.take_presentation().is_none());
        let update = mailbox.poll();
        assert_eq!(update.frame.unwrap().sequence, 7);
        assert_eq!(update.state.unwrap().generation, 3);
    }

    #[test]
    fn blocked_mouse_transitions_do_not_turn_owner_sleep_into_a_spin() {
        let mailbox = RuntimeMailbox::new(|| {});
        mailbox.send(GuiCommand::MouseUp { v: 1, h: 2 }).unwrap();
        let deadline = Instant::now() + Duration::from_millis(10);
        mailbox.wait_until_accepting(deadline, false);
        assert!(Instant::now() >= deadline);
        assert_eq!(
            mailbox.next_command(),
            Some(GuiCommand::MouseUp { v: 1, h: 2 })
        );
    }

    #[test]
    fn sleeping_owner_wakes_for_reserved_shutdown() {
        let mailbox = Arc::new(RuntimeMailbox::new(|| {}));
        let worker = mailbox.clone();
        let (ready, wait_ready) = std::sync::mpsc::channel();
        let (done, wait_done) = std::sync::mpsc::channel();
        let thread = std::thread::spawn(move || {
            ready.send(()).unwrap();
            worker.wait_until(Instant::now() + Duration::from_secs(30));
            done.send(worker.shutdown_requested()).unwrap();
        });
        wait_ready.recv_timeout(Duration::from_secs(2)).unwrap();
        mailbox.request_shutdown();
        assert!(wait_done.recv_timeout(Duration::from_secs(2)).unwrap());
        thread.join().unwrap();
    }
}
