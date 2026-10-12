// Shared guest worker: presentation backends must use this execution and command path.
enum Command {
    CommitText(super::input::TextInputOwner, Vec<u8>),
    CommitTextCaret(super::input::TextInputOwner, Vec<u8>, usize),
    SelectText(super::input::TextInputOwner, std::ops::Range<usize>),
    ReplaceText(
        super::input::TextInputOwner,
        std::ops::Range<usize>,
        Vec<u8>,
        Option<usize>,
    ),
    CommittedInput(MacintoshInput, MacintoshInput),
    ImportClipboard(Vec<u8>),
    Foreground(bool, u64),
    Menu(i16, i16, u32, u64),
    Input(MacintoshInput),
    TimedInput(MacintoshInput, u64, Instant),
    Wheel(super::scroll::WheelRequest),
    FileWheel(u32, u64, super::activation::FileAction),
    ActivateControl(u32, u64),
    ActivateDialog(u32, u64, i16, Option<(u32, u64)>),
    ActivateFile(u32, u64, super::activation::FileAction),
    ActivateFileEntry(
        u32,
        u64,
        usize,
        systemless::runner::StandardFileEntrySnapshot,
    ),
    BeginNotificationAlert(systemless::runner::NotificationSnapshot),
    DismissNotificationAlert(systemless::runner::NotificationSnapshot),
    CancelWheel,
    Shutdown,
}

#[derive(Clone, Default)]
struct Update {
    identity: Option<Arc<game::ApplicationIdentity>>,
    input_receipt: Option<(u64, Instant)>,
    menus: GuestMenuSnapshot,
    menu_presentations: Vec<systemless::menu_model::GuestMenuPresentation>,
    menu_presented: bool,
    menu_height: u16,
    guest_menu_tracking: bool,
    guest_popup: Option<systemless::menu_model::GuestPopupSnapshot>,
    windows: Vec<WindowFrameSnapshot>,
    dialogs: Vec<DialogSnapshot>,
    controls: Vec<ControlSnapshot>,
    lists: Vec<ListManagerSnapshot>,

    list_text_plans:
        Vec<std::collections::BTreeMap<(i16, i16), super::text::ClassicListCellPaintPlan>>,
    text_edits: Vec<TextEditSnapshot>,

    styled_text_plans: Vec<Option<super::text::StyledTextEditPaintPlan>>,
    standard_file: Option<StandardFileSnapshot>,
    cursor: Option<systemless::runner::CursorSnapshot>,
    notifications: Vec<systemless::runner::NotificationSnapshot>,
    notification_alert: Option<systemless::runner::NotificationSnapshot>,
    frame: Option<(u32, u32, Vec<u8>)>,
    clipboard_export: Option<(u64, Vec<u8>)>,
    text_commit_rejection: Option<(u64, super::input::TextInputOwner)>,
    status: String,
}

type Updates = super::mailbox::Latest<Update>;

fn configure_realtime_execution(session: &mut MacintoshSession) -> u32 {
    let instructions_per_tick =
        default_realtime_instructions_per_tick(session.runner().is_powerpc_app());
    session
        .runner_mut()
        .set_instructions_per_tick(instructions_per_tick);
    instructions_per_tick
}

fn run_guest(
    args: Args,
    commands: mpsc::Receiver<Command>,
    updates: Arc<Updates>,
    host_services: bool,
) {
    run_guest_with_setup(args, commands, updates, host_services, |_| {});
}

fn run_guest_with_setup(
    args: Args,
    commands: mpsc::Receiver<Command>,
    updates: Arc<Updates>,
    host_services: bool,
    setup: impl FnOnce(&mut MacintoshSession) + std::panic::UnwindSafe,
) {
    let _publisher = updates.publisher();
    let result = std::panic::catch_unwind(|| {
        let mut session = MacintoshSession::new(
            !args
                .options
                .as_ref()
                .is_some_and(|options| options.addressing_24_bit),
            args.screen_depth,
        );
        if let Some(options) = args.options.as_ref() {
            session.runner_mut().set_ui_theme(options.ui_theme);
        }
        session
            .runner_mut()
            .set_prefer_powerpc_executables(args.prefer_powerpc);
        let app = session.load_path(&args.game)?;
        let mut save_store = host_services.then(|| {
            let mut store = super::desktop_save_store::DesktopSaveStore::for_loaded_archive(
                &args.game,
                session.runner_mut(),
            );
            store.restore_saved_state(session.runner_mut());
            store
        });
        session.initialize(&app);
        setup(&mut session);
        let identity = host_services
            .then(|| game::loaded_application_identity(session.runner()))
            .flatten()
            .filter(|_| {
                args.options
                    .as_ref()
                    .is_none_or(|options| options.native_integrations)
            })
            .map(Arc::new);
        #[cfg(feature = "debug-server")]
        let mut debug_server = args
            .options
            .as_ref()
            .and_then(|options| options.debug_socket.as_ref())
            .map(|path| {
                super::debug_server::DebugServer::bind(path).expect("bind debugger socket")
            });
        // Keep the device and its stream on the guest worker, matching
        // the ordinary desktop runner's stereo delivery and lifetime.
        if host_services {
            if let Some(audio) = systemless::systems::macintosh::audio::CpalAudioBackend::new() {
                session.runner_mut().set_audio(Box::new(audio));
            } else {
                eprintln!("[GPUI] Could not initialize audio output");
            }
        }
        let instructions_per_tick = configure_realtime_execution(&mut session);
        let architecture = if session.status().powerpc_application {
            "PowerPC"
        } else {
            "68k"
        };
        let mut epoch = Instant::now();
        let mut initial_tick = session.runner().guest_tick();
        let mut previous = epoch;
        let mut worker_cadence =
            super::cadence::Cadence::new(epoch, super::cadence::worker_period());
        let mut audio_clock = super::audio_clock::AudioClock::new(epoch);
        let mut queued = std::collections::VecDeque::new();
        let mut text_commit_wait: Option<Instant> = None;
        let mut text_commit_rejection: Option<(u64, super::input::TextInputOwner)> = None;
        let mut wheel: Option<super::scroll::WheelClick> = None;
        let mut activation: Option<super::activation::ControlActivation> = None;
        let mut pointer_down = false;
        let mut input_receipt = None;
        let mut clipboard = super::clipboard::GuestClipboard::default();
        let mut profiler = super::performance::WorkerProfiler::from_env();
        let mut committed_input_wait_logged = false;
        let mut last_profile_dialog = None;
        let mut last_profile_cpu_stall = None;
        loop {
            let mut timing = profiler.begin();
            let start = Instant::now();
            #[cfg(feature = "debug-server")]
            if let Some(server) = debug_server.as_mut() {
                server.pump(session.runner_mut());
            }
            loop {
                match commands.try_recv() {
                    Ok(Command::CancelWheel) => {
                        queued.retain(|command| {
                            !matches!(command, Command::Wheel(_) | Command::FileWheel(..))
                        });
                        if let Some(click) = wheel.as_mut() {
                            click.stop_repeating();
                        }
                    }
                    Ok(Command::Wheel(request)) => {
                        if let Some(Command::Wheel(previous)) = queued.back_mut() {
                            if previous.target == request.target
                                && previous.origin == request.origin
                            {
                                previous.steps =
                                    previous.steps.saturating_add(request.steps).clamp(-4, 4);
                                continue;
                            }
                        }
                        queued.push_back(Command::Wheel(request));
                    }
                    Ok(command) => queued.push_back(command),
                    Err(mpsc::TryRecvError::Empty) => break,
                    Err(mpsc::TryRecvError::Disconnected) => {
                        queued.push_back(Command::Shutdown);
                        break;
                    }
                }
            }
            // A deferred text request or semantic press awaiting guest event
            // consumption must not prevent closing and flushing completed writes.
            if (text_commit_wait.is_some() || activation.is_some())
                && queued
                    .iter()
                    .any(|command| matches!(command, Command::Shutdown))
            {
                if let Some(store) = save_store.as_mut() {
                    store.sync_save_files_now(session.runner_mut());
                }
                return Ok::<(), String>(());
            }
            if let Some(mut click) = wheel.take() {
                if !queued.is_empty() {
                    click.stop_repeating();
                }
                wheel = click.advance(&mut session);
            }
            // New input supersedes a deferred pointer restoration. The
            // physical release has already been delivered; do not move the
            // pointer back over later user input or block a modal handoff.
            if activation.as_ref().is_some_and(|click| click.is_released()) && !queued.is_empty() {
                activation = None;
            }
            if let Some(click) = activation.take() {
                activation = click.advance(&mut session);
            }
            while wheel.is_none() && activation.is_none() {
                match queued.pop_front().ok_or(mpsc::TryRecvError::Empty) {
                    Ok(command)
                        if session.runner_mut().notification_alert_snapshot().is_some()
                            && !matches!(
                                &command,
                                Command::DismissNotificationAlert(..)
                                    | Command::Foreground(..)
                                    | Command::ImportClipboard(..)
                                    | Command::Shutdown
                                    | Command::CancelWheel
                            ) =>
                    {
                        // System alert ownership excludes background input,
                        // including semantic/accessibility and text-service requests.
                    }
                    Ok(Command::CancelWheel) => {}
                    Ok(Command::Menu(menu, item, guest_id, generation)) => {
                        if session
                            .runner_mut()
                            .guest_menu_snapshot()
                            .selectable_result_for_guest(menu, item, guest_id, generation)
                            .is_some()
                        {
                            session.runner_mut().select_guest_menu_item(menu, item);
                        }
                    }
                    Ok(Command::Wheel(request)) => {
                        if !pointer_down && !session.runner().is_ui_tracking_active() {
                            wheel = super::scroll::WheelClick::begin(&mut session, request);
                        }
                    }
                    Ok(Command::BeginNotificationAlert(expected)) => {
                        if !pointer_down && activation.is_none() {
                            session.runner_mut().begin_notification_alert(&expected);
                        }
                    }
                    Ok(Command::DismissNotificationAlert(expected)) => {
                        if !pointer_down && activation.is_none() {
                            session.runner_mut().dismiss_notification_alert(&expected);
                        }
                    }
                    Ok(Command::ActivateControl(id, generation)) => {
                        if !pointer_down {
                            activation = super::activation::ControlActivation::begin(
                                &mut session,
                                id,
                                generation,
                            );
                        }
                    }
                    Ok(Command::ActivateDialog(id, generation, number, identity)) => {
                        if !pointer_down {
                            activation = super::activation::ControlActivation::begin_dialog(
                                &mut session,
                                id,
                                generation,
                                number,
                                identity,
                            );
                        }
                    }
                    Ok(Command::ActivateFileEntry(id, generation, index, expected)) => {
                        if !pointer_down {
                            activation = super::activation::ControlActivation::begin_file_entry(
                                &mut session,
                                id,
                                generation,
                                index,
                                &expected,
                            );
                        }
                    }
                    Ok(
                        Command::ActivateFile(id, generation, action)
                        | Command::FileWheel(id, generation, action),
                    ) => {
                        if !pointer_down {
                            activation = super::activation::ControlActivation::begin_file(
                                &mut session,
                                id,
                                generation,
                                action,
                            );
                        }
                    }
                    Ok(Command::SelectText(owner, range)) => {
                        if pointer_down
                            || !super::input::guest_select_text_range(&mut session, &owner, range)
                        {
                            let revision = text_commit_rejection
                                .as_ref()
                                .map_or(1, |(revision, _)| revision + 1);
                            text_commit_rejection = Some((revision, owner));
                        }
                    }
                    Ok(Command::ReplaceText(owner, range, bytes, caret)) => {
                        let valid = caret.is_none_or(|caret| {
                            caret >= range.start && caret <= range.start.saturating_add(bytes.len())
                        }) && caret.is_none_or(|_| {
                            session.runner().event_manager_snapshot().key_map[0x7b / 8]
                                & (1 << (0x7b % 8))
                                == 0
                        }) && !pointer_down
                            && super::input::guest_commit_inputs(&mut session, &owner, &bytes)
                                .is_some();
                        let selected_range = valid
                            && super::input::guest_select_text_range(
                                &mut session,
                                &owner,
                                range.clone(),
                            );
                        if selected_range {
                            text_commit_wait = None;
                            let mut selected = owner;
                            selected.selection = range;
                            if let Some(caret) = caret {
                                for _ in caret..selected.selection.start + bytes.len() {
                                    queued.push_front(Command::CommittedInput(
                                        MacintoshInput::KeyDown {
                                            mac_key: 0x7b,
                                            character: 0x1c,
                                        },
                                        MacintoshInput::KeyUp {
                                            mac_key: 0x7b,
                                            character: 0x1c,
                                        },
                                    ));
                                }
                            }
                            queued.push_front(Command::CommitText(selected, bytes));
                        } else {
                            let events = session.runner().event_manager_snapshot();
                            let in_event = events.queue_len != 0
                                || events
                                    .last_record
                                    .as_ref()
                                    .is_some_and(|event| event.what != 0);
                            let same_field = match owner.target {
                                super::input::TextInputTarget::Document { port } => session
                                    .runner_mut()
                                    .text_edit_snapshot()
                                    .records
                                    .iter()
                                    .any(|record| {
                                        record.active
                                            && (record.guest_id, record.generation)
                                                == owner.identity
                                            && record.owner_port == port
                                    }),
                                super::input::TextInputTarget::StandardFile { new_folder } => {
                                    session
                                        .runner()
                                        .standard_file_snapshot()
                                        .and_then(|panel| {
                                            super::input::standard_file_text_owner(&panel)
                                        })
                                        .is_some_and(|actual| {
                                            actual.identity == owner.identity
                                                && actual.new_folder == new_folder
                                        })
                                }
                                super::input::TextInputTarget::Dialog { .. } => {
                                    let dialogs = session.runner_mut().dialog_snapshot();
                                    let windows = session.runner_mut().window_frame_snapshot();
                                    super::input::dialog_field_is_current(
                                        &owner, &dialogs, &windows,
                                    )
                                }
                            };
                            // Time spent waiting for an unconsumed guest event is
                            // not evidence that the predicted text owner is stale.
                            // Start the bounded settling window after the queue drains.
                            if events.queue_len != 0 {
                                text_commit_wait = None;
                            }
                            if !pointer_down
                                && same_field
                                && in_event
                                && text_commit_wait.get_or_insert_with(Instant::now).elapsed()
                                    < Duration::from_millis(250)
                            {
                                queued.push_front(Command::ReplaceText(owner, range, bytes, caret));
                                break;
                            }
                            text_commit_wait = None;
                            let revision = text_commit_rejection
                                .as_ref()
                                .map_or(1, |(revision, _)| revision + 1);
                            text_commit_rejection = Some((revision, owner));
                        }
                    }
                    Ok(command @ Command::CommitText(..))
                    | Ok(command @ Command::CommitTextCaret(..)) => {
                        let (owner, bytes, caret) = match command {
                            Command::CommitText(owner, bytes) => (owner, bytes, None),
                            Command::CommitTextCaret(owner, bytes, caret) => {
                                (owner, bytes, Some(caret))
                            }
                            _ => unreachable!(),
                        };
                        let safe_caret = caret.is_none_or(|caret| {
                            caret >= owner.selection.start
                                && caret <= owner.selection.start.saturating_add(bytes.len())
                                && session.runner().event_manager_snapshot().key_map[0x7b / 8]
                                    & (1 << (0x7b % 8))
                                    == 0
                        });
                        let inputs = (!pointer_down && safe_caret)
                            .then(|| {
                                super::input::guest_commit_inputs(&mut session, &owner, &bytes)
                            })
                            .flatten();
                        if let Some(mut inputs) = inputs {
                            if profiler.enabled() {
                                eprintln!(
                                    "[GPUI-TEXT-COMMIT] accepted pairs={} owner={:?}",
                                    inputs.len() / 2,
                                    owner.identity
                                );
                            }
                            if let Some(caret) = caret {
                                for _ in caret..owner.selection.start + bytes.len() {
                                    inputs.extend([
                                        MacintoshInput::KeyDown {
                                            mac_key: 0x7b,
                                            character: 0x1c,
                                        },
                                        MacintoshInput::KeyUp {
                                            mac_key: 0x7b,
                                            character: 0x1c,
                                        },
                                    ]);
                                }
                            }
                            text_commit_wait = None;
                            for pair in inputs.chunks_exact(2).rev() {
                                queued.push_front(Command::CommittedInput(pair[0], pair[1]));
                            }
                        } else {
                            // Mouse-up or an earlier committed character can
                            // still be inside guest Toolbox code. Let that
                            // event complete before validating the next range.
                            let events = session.runner().event_manager_snapshot();
                            let in_event = events.queue_len != 0
                                || events
                                    .last_record
                                    .as_ref()
                                    .is_some_and(|record| record.what != 0)
                                || session.runner().is_ui_tracking_active();
                            let same_field = match owner.target {
                                super::input::TextInputTarget::StandardFile { new_folder } => {
                                    session
                                        .runner()
                                        .standard_file_snapshot()
                                        .and_then(|panel| {
                                            super::input::standard_file_text_owner(&panel)
                                        })
                                        .is_some_and(|actual| {
                                            actual.identity == owner.identity
                                                && actual.new_folder == new_folder
                                        })
                                }
                                super::input::TextInputTarget::Document { port } => session
                                    .runner_mut()
                                    .text_edit_snapshot()
                                    .records
                                    .iter()
                                    .any(|record| {
                                        record.active
                                            && (record.guest_id, record.generation)
                                                == owner.identity
                                            && record.owner_port == port
                                    }),
                                super::input::TextInputTarget::Dialog { .. } => {
                                    let dialogs = session.runner_mut().dialog_snapshot();
                                    let windows = session.runner_mut().window_frame_snapshot();
                                    super::input::dialog_field_is_current(
                                        &owner, &dialogs, &windows,
                                    )
                                }
                            };
                            let document =
                                matches!(
                                    owner.target,
                                    super::input::TextInputTarget::StandardFile { .. }
                                ) || (session.runner().standard_file_snapshot().is_none()
                                    && (matches!(
                                        owner.target,
                                        super::input::TextInputTarget::Dialog { .. }
                                    ) || !session
                                        .runner_mut()
                                        .dialog_snapshot()
                                        .iter()
                                        .any(|dialog| dialog.visible && dialog.active)));
                            // Time spent waiting for an unconsumed guest event is
                            // not evidence that the predicted text owner is stale.
                            // Start the bounded settling window after the queue drains.
                            if events.queue_len != 0 {
                                text_commit_wait = None;
                            }
                            if !pointer_down
                                && same_field
                                && document
                                && in_event
                                && text_commit_wait.get_or_insert_with(Instant::now).elapsed()
                                    < Duration::from_millis(250)
                            {
                                queued.push_front(if let Some(caret) = caret {
                                    Command::CommitTextCaret(owner, bytes, caret)
                                } else {
                                    Command::CommitText(owner, bytes)
                                });
                                break;
                            }
                            text_commit_wait = None;
                            let revision = text_commit_rejection
                                .as_ref()
                                .map_or(1, |(revision, _)| revision + 1);
                            text_commit_rejection = Some((revision, owner));
                        }
                    }
                    Ok(Command::CommittedInput(down, up)) => {
                        // A worker publication is not acknowledgment that the
                        // guest has consumed the previous character. EV can
                        // remain in Toolbox code across several publications.
                        let events = session.runner().event_manager_snapshot();
                        if events
                            .queued_event_types
                            .iter()
                            .any(|what| matches!(*what, 3 | 5))
                        {
                            if profiler.enabled() && !committed_input_wait_logged {
                                eprintln!("[GPUI-TEXT-QUEUE] blocked events={:?} key0={} modal={} last={:?}",
                                        events.queued_event_types, events.key_map[0], session.runner().dispatcher().is_dialog_tracking(), events.last_record);
                            }
                            committed_input_wait_logged = true;
                            // A guest that stops reading keyboard events must
                            // still be closable. Flush only completed writes.
                            if queued
                                .iter()
                                .any(|command| matches!(command, Command::Shutdown))
                            {
                                if let Some(store) = save_store.as_mut() {
                                    store.sync_save_files_now(session.runner_mut());
                                }
                                return Ok::<(), String>(());
                            }
                            queued.push_front(Command::CommittedInput(down, up));
                            break;
                        }
                        committed_input_wait_logged = false;
                        if profiler.enabled() {
                            eprintln!("[GPUI-TEXT-QUEUE] deliver {down:?}");
                        }
                        // A text service commits characters, not physical held
                        // keys. Release before execution to avoid autoKey, and
                        // give the guest a worker cycle between characters.
                        session.deliver_input(down);
                        session.deliver_input(up);
                        break;
                    }
                    Ok(Command::ImportClipboard(text)) => {
                        clipboard.imported(&text);
                        session.import_clipboard_text(text);
                    }
                    Ok(Command::Foreground(active, generation)) => {
                        clipboard.foreground(active, generation);
                        session.request_foreground(active);
                    }
                    Ok(command @ (Command::Input(_) | Command::TimedInput(..))) => {
                        let input = match command {
                            Command::Input(input) => input,
                            Command::TimedInput(input, id, received) => {
                                input_receipt = Some((id, received));
                                input
                            }
                            _ => unreachable!(),
                        };
                        match input {
                            MacintoshInput::MouseDown { .. } => pointer_down = true,
                            MacintoshInput::MouseUp { .. } => pointer_down = false,
                            _ => {}
                        }
                        session.deliver_input(input);
                        // Tracking and autoKey must observe a held input
                        // before a queued release clears it (IM:I, I-246).
                        if matches!(input, MacintoshInput::MouseDown { .. })
                            || matches!(input, MacintoshInput::KeyDown { mac_key, .. } if mac_key != 0x37)
                        {
                            break;
                        }
                    }
                    Err(mpsc::TryRecvError::Empty) => break,
                    Ok(Command::Shutdown) | Err(mpsc::TryRecvError::Disconnected) => {
                        if let Some(store) = save_store.as_mut() {
                            store.sync_save_files_now(session.runner_mut());
                        }
                        return Ok::<(), String>(());
                    }
                }
            }
            session
                .runner_mut()
                .advance_menu_presentation_clock(start.duration_since(previous));
            previous = start;
            let current_tick = session.runner().guest_tick();
            let mut due_tick = initial_tick
                .saturating_add(
                    (epoch.elapsed().as_secs_f64() * super::cadence::GUEST_TICKS_PER_SECOND) as u32,
                )
                .saturating_add(1);
            if due_tick.saturating_sub(current_tick) > 4 {
                // A late frame must not make the worker chase an ever-growing
                // backlog. The ordinary desktop runner also rebases here.
                epoch = start;
                initial_tick = current_tick;
                due_tick = current_tick.saturating_add(1);
            }
            let deadline = due_tick.min(current_tick.saturating_add(2));
            let batch = if session.runner().is_powerpc_app() {
                (instructions_per_tick as usize).div_ceil(16)
            } else {
                10_000
            };
            // Snapshot validation and command handling can exceed a frame's
            // budget at higher guest depths. Always reserve execution time
            // afterward so queued Toolbox events cannot be starved by UI work.
            timing.checkpoint();
            let cpu_deadline = Instant::now() + Duration::from_millis(12);
            let cpu_steps =
                super::cpu_frame::advance(session.runner_mut(), batch, deadline, cpu_deadline);
            if profiler.enabled() {
                let stall =
                    (activation.is_some() && cpu_steps == 0).then_some((current_tick, deadline));
                if stall != last_profile_cpu_stall {
                    eprintln!("[GPUI-CPU] pending_activation_stall={stall:?} steps={cpu_steps} due_tick={due_tick} tracking={}", session.runner().is_ui_tracking_active());
                    last_profile_cpu_stall = stall;
                }
            }
            timing.checkpoint();
            session.runner_mut().advance_notification_system_sounds();
            let mut audio_samples = audio_clock.samples(start);
            let mut sound_budget = 10_000;
            while audio_samples > 0 {
                // Keep refill boundaries short even after a late cycle,
                // retaining the existing per-cycle callback reserve.
                let chunk = audio_samples.min(367);
                session.runner_mut().mix_gui_audio_slice(chunk);
                audio_samples -= chunk;
                session.runner_mut().advance_notification_system_sounds();
                if sound_budget > 0
                    && session.runner().has_pending_sound_work()
                    && Instant::now() < cpu_deadline
                {
                    let (steps, _) = session
                        .runner_mut()
                        .run_gui_pending_sound_work(sound_budget);
                    sound_budget = sound_budget.saturating_sub(steps);
                }
            }
            session.runner_mut().finish_gui_frame();
            clipboard.observe(&session);
            session.drain_audio();
            if let Some(store) = save_store.as_mut() {
                store.sync_save_files(session.runner_mut());
            }
            timing.checkpoint();
            let menus = session.runner_mut().guest_menu_snapshot();
            let menu_presentations = session.runner_mut().comparison_menu_presentations(&menus);
            let menu_presented = session.runner().guest_menu_bar_presented();
            let menu_height = session.runner().bus().read_word(MBAR_HEIGHT);
            let guest_menu_tracking = session.runner().guest_menu_tracking_active();
            let guest_popup = session.runner_mut().guest_popup_snapshot();
            let frame = session.video_frame();
            timing.checkpoint();
            let running = session.status().running;
            let windows = session.runner_mut().window_frame_snapshot();
            let dialogs = session.runner_mut().dialog_snapshot();
            if profiler.enabled() {
                let owner =
                    session
                        .runner()
                        .dispatcher()
                        .dialog_tracking
                        .as_ref()
                        .map(|tracking| {
                            (
                                tracking.dialog_ptr,
                                tracking.edit_item,
                                tracking.edit_text.clone(),
                            )
                        });
                let state = dialogs
                    .first()
                    .map(|dialog| (dialog.guest_id, dialog.active, owner));
                if state != last_profile_dialog {
                    eprintln!("[GPUI-UI] dialog={state:?}");
                    last_profile_dialog = state;
                }
            }
            let controls = session.runner_mut().control_snapshot();
            let lists = session.runner_mut().list_manager_snapshot();
            let text_edits = session.runner_mut().text_edit_snapshot().records;
            let mut standard_file = session.runner_mut().standard_file_snapshot();
            let notifications = session.runner_mut().notification_snapshot();
            let notification_alert = session.runner_mut().notification_alert_snapshot();
            timing.checkpoint();

            let list_text_plans = frame
                .as_ref()
                .map(|frame| {
                    qualify_list_text_fields(&lists, &frame.pixels, frame.width, frame.height)
                })
                .unwrap_or_default();

            let styled_text_plans = frame
                .as_ref()
                .map(|frame| {
                    qualify_styled_text_fields(
                        &text_edits,
                        &frame.pixels,
                        frame.width,
                        frame.height,
                    )
                })
                .unwrap_or_default();
            if let (Some(frame), Some(panel)) = (&frame, &mut standard_file) {
                panel.retain_volume_indicator(&frame.pixels, frame.width, frame.height);
            }
            timing.checkpoint();
            let frame = frame.map(|frame| (frame.width, frame.height, gpui_pixels(frame.pixels)));
            let update = Update {
                input_receipt,
                identity: identity.clone(),
                clipboard_export: clipboard.export.clone(),
                text_commit_rejection: text_commit_rejection.clone(),
                menus,
                menu_presentations,
                menu_presented,
                menu_height,
                guest_menu_tracking,
                guest_popup,
                windows,
                dialogs,
                controls,
                lists,

                list_text_plans,
                text_edits,

                styled_text_plans,
                standard_file,
                cursor: Some(session.runner().cursor_snapshot()),
                notifications,
                notification_alert,
                frame,
                status: format!(
                    "{architecture} · {}",
                    if running { "Running" } else { "Guest stopped" }
                ),
            };
            if running {
                profiler.sleep_before_publication(
                    &mut timing,
                    worker_cadence.publication_delay(Instant::now()),
                );
            }
            updates.publish(update);
            timing.checkpoint();
            profiler.record(timing);
            if !running {
                if let Some(store) = save_store.as_mut() {
                    store.sync_save_files_now(session.runner_mut());
                }
                return Ok(());
            }
        }
    });
    let error = match result {
        Ok(Ok(())) => return,
        Ok(Err(error)) => error,
        Err(_) => "Guest worker stopped unexpectedly; see terminal output".to_owned(),
    };
    updates.publish(Update {
        status: error,
        ..Default::default()
    });
}
