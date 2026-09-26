//! The desktop guest is constructed, executed and destroyed on one owner thread.
//! Only configuration, commands and owned snapshots cross this boundary.

use super::runtime_driver::GuiDriver;
use super::runtime_mailbox::{PresentationOptions, RuntimeMailbox, RuntimeStatus};
use super::runtime_protocol::{GuiCommand, GuiState};
use std::path::PathBuf;
use std::sync::Arc;
use std::thread::JoinHandle;
use std::time::Instant;

#[derive(Clone)]
pub(super) struct RuntimeConfig {
    #[cfg(target_os = "macos")]
    pub native_preflight: bool,
    pub game_path: PathBuf,
    pub arrows_as_numpad: bool,
    pub native_integrations: bool,
    pub addressing_24_bit: bool,
    pub screen_depth: Option<u16>,
    pub ui_theme: systemless::ui_theme::UiThemeId,
    pub debug_socket: Option<PathBuf>,
}

pub(super) struct RuntimeOwner {
    pub mailbox: Arc<RuntimeMailbox>,
    thread: Option<JoinHandle<()>>,
}

fn panic_message(panic: Box<dyn std::any::Any + Send>) -> String {
    panic
        .downcast_ref::<String>()
        .cloned()
        .or_else(|| {
            panic
                .downcast_ref::<&str>()
                .map(|message| (*message).to_owned())
        })
        .unwrap_or_else(|| "desktop runtime panicked".to_owned())
}

impl RuntimeOwner {
    pub fn spawn(
        config: RuntimeConfig,
        wake: impl Fn() + Send + Sync + 'static,
    ) -> std::io::Result<Self> {
        Self::spawn_with(config, wake, |driver| {
            driver.init_game();
            Ok(())
        })
    }

    fn spawn_with(
        config: RuntimeConfig,
        wake: impl Fn() + Send + Sync + 'static,
        initialize: impl FnOnce(&mut GuiDriver) -> Result<(), String> + Send + 'static,
    ) -> std::io::Result<Self> {
        let mailbox = Arc::new(RuntimeMailbox::new(wake));
        let shared = mailbox.clone();
        let thread = std::thread::Builder::new()
            .name("systemless-runtime".into())
            .spawn(move || {
                #[cfg(target_os = "macos")]
                let mut native_bundle = None;
                #[cfg(target_os = "macos")]
                let preflight_path = config.game_path.clone();
                // Never construct a runner, CPAL stream or debugger on the host and
                // then move it here. GuiDriver itself need not implement Send.
                let mut driver = GuiDriver::new(
                    config.game_path,
                    config.arrows_as_numpad,
                    config.native_integrations,
                    config.addressing_24_bit,
                    config.screen_depth,
                    config.ui_theme,
                );
                let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(
                    || -> Result<(), String> {
                        #[cfg(target_os = "macos")]
                        if config.native_preflight && !shared.shutdown_requested() {
                            match super::native_bundle::prepare_for_game(&preflight_path) {
                                Ok(Some(bundle)) => {
                                    native_bundle = Some(bundle);
                                    return Ok(());
                                }
                                Ok(None) => {}
                                Err(error) => {
                                    eprintln!("[SYSTEMLESS] Native startup fallback: {error}")
                                }
                            }
                        }
                        if shared.shutdown_requested() {
                            return Ok(());
                        }
                        if let Some(path) = config.debug_socket {
                            driver.debug_server =
                                Some(super::debug_server::DebugServer::bind(&path).map_err(
                                    |error| format!("cannot bind debug socket: {error}"),
                                )?);
                        }
                        initialize(&mut driver)?;
                        let mut state = GuiState::default();
                        driver.capture_state(&mut state, true);
                        shared.publish(None, state);
                        shared.set_status(RuntimeStatus::Ready);
                        run(&mut driver, &shared, config.native_integrations);
                        Ok(())
                    },
                ));
                let mut error = match outcome {
                    Ok(result) => result.err(),
                    Err(panic) => Some(panic_message(panic)),
                };
                // Final persistence runs on the same owner, including after a
                // recoverable runtime panic. Report failure rather than abandoning
                // the window's asynchronous shutdown wait.
                if let Err(panic) = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    driver.sync_save_files(true)
                })) {
                    let message = format!("final save flush failed: {}", panic_message(panic));
                    error = Some(error.map_or_else(
                        || message.clone(),
                        |previous| format!("{previous}; {message}"),
                    ));
                }
                let instructions = driver.total_instructions;
                if let Err(panic) =
                    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| drop(driver)))
                {
                    let message = format!("runtime teardown failed: {}", panic_message(panic));
                    error = Some(error.map_or_else(
                        || message.clone(),
                        |previous| format!("{previous}; {message}"),
                    ));
                }
                #[cfg(target_os = "macos")]
                if error.is_none() && !shared.shutdown_requested() {
                    if let Some(bundle) = native_bundle {
                        shared.finish_native_bootstrap(bundle);
                        return;
                    }
                }
                shared.set_status(RuntimeStatus::Stopped {
                    error,
                    instructions,
                });
            })?;
        Ok(Self {
            mailbox,
            thread: Some(thread),
        })
    }

    /// A window callback must never join a running worker. A stopped status is
    /// published just before thread return, so callers can poll this briefly.
    pub fn join_finished(&mut self) -> Result<bool, String> {
        if self
            .thread
            .as_ref()
            .is_some_and(|thread| !thread.is_finished())
        {
            return Ok(false);
        }
        if let Some(thread) = self.thread.take() {
            thread.join().map_err(panic_message)?;
        }
        Ok(true)
    }
}

impl Drop for RuntimeOwner {
    fn drop(&mut self) {
        self.mailbox.request_shutdown();
        // Ordinary close waits asynchronously and calls join_finished first.
        // An exceptional host unwind must not block on guest execution.
    }
}

fn run(driver: &mut GuiDriver, mailbox: &RuntimeMailbox, native_integrations: bool) {
    let mut options = PresentationOptions {
        capture_crop: native_integrations,
        learning_crop: true,
        ..Default::default()
    };
    loop {
        if let Some(next) = mailbox.take_presentation() {
            if let Some(headroom) = next.render_headroom {
                driver.render_headroom = headroom;
            }
            driver.force_next_render |= next.force;
            options = next;
        }
        // Do not collapse queued click transitions before the guest observes
        // each press/release. Preserve the existing driver's observation latch.
        if mailbox.shutdown_requested() {
            while let Some(command) = mailbox.next_command() {
                driver.apply_command(command);
            }
            break;
        }
        for _ in 0..32 {
            if driver.mouse_release_latch.requires_guest_progress() {
                break;
            }
            let Some(command) = mailbox.next_command() else {
                break;
            };
            driver.apply_command(command);
        }
        driver.pump_debugger();
        let now = Instant::now();
        let next = driver.next_frame_time.unwrap_or(now);
        if now < next {
            mailbox
                .wait_until_accepting(next, !driver.mouse_release_latch.requires_guest_progress());
            continue;
        }
        let (target, _) = GuiDriver::next_frame_target(now, next);
        driver.next_frame_time = Some(target);
        let mut acknowledgements = Vec::with_capacity(32);
        let mut input_applied = false;
        driver.step_frame_with_safe_points(Instant::now, |runner, latch| {
            if mailbox.shutdown_requested() {
                return false;
            }
            // Work per safe point is bounded even while a producer stays busy.
            for _ in 0..32 {
                if latch.requires_guest_progress() || acknowledgements.len() == 32 {
                    break;
                }
                let Some(command) = mailbox.next_command() else {
                    break;
                };
                if matches!(command, GuiCommand::AcknowledgeWarp { .. }) {
                    acknowledgements.push(command);
                } else {
                    GuiDriver::apply_guest_command(runner, latch, command);
                    input_applied = true;
                }
            }
            true
        });
        driver.force_next_render |= input_applied;
        for command in acknowledgements {
            driver.apply_command(command);
        }
        driver.flush_ready_mouse_release();
        if driver.guest_requested_exit() {
            if !driver.guest_exit_reported {
                driver.sync_save_files(true);
                driver.guest_exit_reported = true;
            }
            if driver.debug_server.is_none() {
                break;
            }
        }
        if mailbox.shutdown_requested() {
            continue;
        }
        driver.sync_save_files(false);
        let frame = if driver.should_render_frame(options.debug.is_some()) {
            let mut frame = mailbox.frame_buffer();
            if driver.capture_frame(
                &mut frame,
                options.debug,
                options.capture_crop,
                options.learning_crop,
            ) {
                // The mailbox now owns a complete image; its newest replacement
                // is safe to drop independently of presentation acknowledgements.
                driver.last_presented_guest_tick = Some(frame.guest_tick);
                driver.force_next_render = false;
                Some(frame)
            } else {
                mailbox.recycle(frame);
                None
            }
        } else {
            None
        };
        let mut state = GuiState::default();
        driver.capture_state(&mut state, true);
        mailbox.publish(frame, state);
        driver.finish_frame();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc;
    use std::time::Duration;
    use systemless::memory::MemoryBus;

    fn config() -> RuntimeConfig {
        RuntimeConfig {
            #[cfg(target_os = "macos")]
            native_preflight: false,
            game_path: "owner-test".into(),
            arrows_as_numpad: false,
            native_integrations: false,
            addressing_24_bit: false,
            screen_depth: Some(8),
            ui_theme: systemless::ui_theme::UiThemeId::ClassicSystem7,
            debug_socket: None,
        }
    }

    fn wait_stopped(owner: &mut RuntimeOwner, wakes: &mpsc::Receiver<()>) -> RuntimeStatus {
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            let status = owner.mailbox.poll().status;
            if matches!(status, RuntimeStatus::Stopped { .. }) {
                while !owner.join_finished().unwrap() {
                    assert!(Instant::now() < deadline);
                    std::thread::yield_now();
                }
                return status;
            }
            wakes
                .recv_timeout(deadline.saturating_duration_since(Instant::now()))
                .unwrap();
        }
    }

    struct ThreadCheckedAudio {
        owner: std::thread::ThreadId,
        dropped: mpsc::Sender<std::thread::ThreadId>,
    }
    impl systemless::audio::AudioBackend for ThreadCheckedAudio {
        fn queue_samples(&mut self, _: &[u8]) {
            assert_eq!(std::thread::current().id(), self.owner);
        }
        fn stop(&mut self) {
            assert_eq!(std::thread::current().id(), self.owner);
        }
    }
    impl Drop for ThreadCheckedAudio {
        fn drop(&mut self) {
            self.dropped.send(std::thread::current().id()).unwrap();
        }
    }

    #[test]
    fn runner_and_audio_are_created_used_and_destroyed_on_the_owner() {
        let host = std::thread::current().id();
        let (wake, wakes) = mpsc::channel();
        let (created, creation) = mpsc::channel();
        let (dropped, destruction) = mpsc::channel();
        let mut owner = RuntimeOwner::spawn_with(
            config(),
            move || {
                let _ = wake.send(());
            },
            move |driver| {
                use systemless::cpu::Register;
                let thread = std::thread::current().id();
                let mut runner =
                    systemless::runner::FixtureRunner::new(8 * 1024 * 1024, Default::default());
                runner.bus_mut().write_word(0x10000, 0x60fe); // BRA.S self
                runner.cpu_mut().write_reg(Register::PC, 0x10000);
                runner.set_audio(Box::new(ThreadCheckedAudio {
                    owner: thread,
                    dropped,
                }));
                driver.runner = Some(runner);
                driver.initialized = true;
                created.send(thread).unwrap();
                Ok(())
            },
        )
        .unwrap();
        let runtime = creation.recv_timeout(Duration::from_secs(5)).unwrap();
        assert_ne!(runtime, host);
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            let update = owner.mailbox.poll();
            if let Some(frame) = update.frame {
                assert!(frame.sequence > 0);
                assert!(frame.generation > 0);
                owner.mailbox.recycle(frame);
                break;
            }
            wakes
                .recv_timeout(deadline.saturating_duration_since(Instant::now()))
                .unwrap();
        }
        owner.mailbox.request_shutdown();
        assert!(matches!(
            wait_stopped(&mut owner, &wakes),
            RuntimeStatus::Stopped { error: None, .. }
        ));
        assert_eq!(
            destruction.recv_timeout(Duration::from_secs(1)).unwrap(),
            runtime
        );
    }

    #[test]
    fn stalled_initialization_never_blocks_host_commands_or_join_polling() {
        let (wake, wakes) = mpsc::channel();
        let (entered, entry) = mpsc::channel();
        let (resume, paused) = mpsc::channel();
        let mut owner = RuntimeOwner::spawn_with(
            config(),
            move || {
                let _ = wake.send(());
            },
            move |_| {
                entered.send(()).unwrap();
                paused.recv_timeout(Duration::from_secs(5)).unwrap();
                Ok(())
            },
        )
        .unwrap();
        entry.recv_timeout(Duration::from_secs(5)).unwrap();
        let start = Instant::now();
        owner
            .mailbox
            .send(GuiCommand::MouseDown { v: 11, h: 13 })
            .unwrap();
        owner.mailbox.request_shutdown();
        assert!(!owner.join_finished().unwrap());
        assert_eq!(owner.mailbox.poll().status, RuntimeStatus::Starting);
        assert!(start.elapsed() < Duration::from_millis(100));
        resume.send(()).unwrap();
        assert!(matches!(
            wait_stopped(&mut owner, &wakes),
            RuntimeStatus::Stopped { error: None, .. }
        ));
    }

    #[test]
    fn startup_failure_and_panic_reach_terminal_status() {
        for panic in [false, true] {
            let (wake, wakes) = mpsc::channel();
            let mut owner = RuntimeOwner::spawn_with(
                config(),
                move || {
                    let _ = wake.send(());
                },
                move |_| {
                    if panic {
                        panic!("controlled owner startup panic");
                    }
                    Err("controlled owner startup failure".into())
                },
            )
            .unwrap();
            let RuntimeStatus::Stopped { error, .. } = wait_stopped(&mut owner, &wakes) else {
                unreachable!()
            };
            assert!(error.unwrap().contains("controlled owner startup"));
        }
    }

    struct FailingRuntimeAudio {
        owner: std::thread::ThreadId,
        dropped: mpsc::Sender<std::thread::ThreadId>,
    }

    impl systemless::audio::AudioBackend for FailingRuntimeAudio {
        fn queue_samples(&mut self, _: &[u8]) {
            assert_eq!(std::thread::current().id(), self.owner);
            panic!("controlled runtime audio failure");
        }
        fn stop(&mut self) {}
    }

    impl Drop for FailingRuntimeAudio {
        fn drop(&mut self) {
            let _ = self.dropped.send(std::thread::current().id());
        }
    }

    #[test]
    fn guest_exit_host_close_and_runtime_failure_flush_saves_before_stopped() {
        use super::super::desktop_save_store::DesktopSaveStore;
        use systemless::cpu::Register;
        use systemless::runner::{FixtureRunner, VfsFileSnapshot};

        for (fail_audio, host_close) in [(false, false), (true, false), (false, true)] {
            let temporary = tempfile::tempdir().unwrap();
            let game_path = temporary.path().join("Game.sit");
            let mut config = config();
            config.game_path = game_path.clone();
            let original = VfsFileSnapshot {
                path: "Game/Pilots/Shutdown Test".into(),
                data_fork: vec![0, 1, 255, 3],
                resource_fork: vec![4, 0, 6, 255],
                file_type: u32::from_be_bytes(*b"PIL "),
                creator: u32::from_be_bytes(*b"TEST"),
                finder_flags: 0x4000,
                created_date: 123,
                modified_date: 456,
            };
            let saved = original.clone();
            let archive = game_path.clone();
            let (wake, wakes) = mpsc::channel();
            let (dropped, destruction) = mpsc::channel();
            let (created, creation) = mpsc::channel();
            let mut owner = RuntimeOwner::spawn_with(
                config,
                move || {
                    let _ = wake.send(());
                },
                move |driver| {
                    let thread = std::thread::current().id();
                    let mut runner = FixtureRunner::new(8 * 1024 * 1024, Default::default());
                    // Actual guest ExitToShell versus an executing guest whose
                    // host audio callback fails after initialization succeeded.
                    runner.bus_mut().write_word(
                        0x10000,
                        if fail_audio || host_close {
                            0x60fe
                        } else {
                            0xa9f4
                        },
                    );
                    runner.cpu_mut().write_reg(Register::PC, 0x10000);
                    runner.cpu_mut().write_reg(Register::A7, 0x700000);
                    let store = DesktopSaveStore::for_loaded_archive(&archive, &mut runner);
                    runner.import_vfs_file(&saved);
                    if fail_audio {
                        runner.set_audio(Box::new(FailingRuntimeAudio {
                            owner: thread,
                            dropped,
                        }));
                    } else {
                        // The normal ExitToShell case has no host audio device.
                        drop(dropped);
                    }
                    driver.runner = Some(runner);
                    driver.save_store = Some(store);
                    driver.initialized = true;
                    created.send(thread).unwrap();
                    Ok(())
                },
            )
            .unwrap();
            let runtime_thread = creation.recv_timeout(Duration::from_secs(5)).unwrap();
            if host_close {
                owner.mailbox.request_shutdown();
            }
            let RuntimeStatus::Stopped {
                error,
                instructions,
            } = wait_stopped(&mut owner, &wakes)
            else {
                unreachable!()
            };
            if fail_audio {
                assert!(error.unwrap().contains("controlled runtime audio failure"));
                assert_eq!(
                    destruction.recv_timeout(Duration::from_secs(1)).unwrap(),
                    runtime_thread
                );
            } else {
                assert_eq!(error, None);
                if !host_close {
                    assert!(instructions > 0);
                }
            }
            // Read from disk only after the public terminal acknowledgement.
            // No helper is allowed to trigger an extra owner-side flush here.
            let mut reader = FixtureRunner::new(8 * 1024 * 1024, Default::default());
            let mut store = DesktopSaveStore::for_loaded_archive(&game_path, &mut reader);
            let persisted = store.load_saved_files();
            assert_eq!(persisted.len(), 1);
            let actual = &persisted[0];
            assert_eq!(actual.path, original.path);
            assert_eq!(actual.data_fork, original.data_fork);
            assert_eq!(actual.resource_fork, original.resource_fork);
            assert_eq!(actual.file_type, original.file_type);
            assert_eq!(actual.creator, original.creator);
            assert_eq!(actual.finder_flags, original.finder_flags);
            assert_eq!(actual.created_date, original.created_date);
            assert_eq!(actual.modified_date, original.modified_date);
        }
    }

    #[test]
    fn missing_archive_load_reports_terminal_failure() {
        let temporary = tempfile::tempdir().unwrap();
        let mut config = config();
        config.game_path = temporary.path().join("missing.sit");
        let (wake, wakes) = mpsc::channel();
        let mut owner = RuntimeOwner::spawn(config, move || {
            let _ = wake.send(());
        })
        .unwrap();
        let RuntimeStatus::Stopped { error, .. } = wait_stopped(&mut owner, &wakes) else {
            unreachable!()
        };
        assert!(error.unwrap().contains("Failed to load game"));
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn cached_native_bootstrap_finishes_without_starting_the_guest() {
        let temporary = tempfile::tempdir().unwrap();
        let game_path = temporary.path().join("game.sit");
        // Deliberately not a valid archive: a cached bundle must not decode it.
        std::fs::write(&game_path, b"cached bootstrap archive").unwrap();
        let expected =
            super::super::native_bundle::prepare_bundle(&game_path, "Bootstrap Test").unwrap();
        let mut config = config();
        config.game_path = game_path;
        config.native_preflight = true;
        let (wake, wakes) = mpsc::channel();
        let mut owner = RuntimeOwner::spawn_with(
            config,
            move || {
                let _ = wake.send(());
            },
            |_| panic!("bootstrap must not initialize or execute a guest before relaunch"),
        )
        .unwrap();
        let deadline = Instant::now() + Duration::from_secs(5);
        let received = loop {
            let update = owner.mailbox.poll();
            if let RuntimeStatus::Stopped {
                error,
                instructions,
            } = update.status
            {
                assert_eq!(error, None);
                assert_eq!(instructions, 0);
                break update.native_bundle;
            }
            wakes
                .recv_timeout(deadline.saturating_duration_since(Instant::now()))
                .unwrap();
        };
        while !owner.join_finished().unwrap() {
            assert!(Instant::now() < deadline);
            std::thread::yield_now();
        }
        assert_eq!(received, Some(expected.clone()));
        std::fs::remove_dir_all(expected.bundle_path.parent().unwrap()).unwrap();
    }
}
