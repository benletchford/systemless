use super::*;

#[derive(Clone, Default)]
pub(super) struct CapturingAudioBackend {
    pub(super) stereo_samples: Rc<RefCell<Vec<u8>>>,
}

impl CapturingAudioBackend {
    pub(super) fn new() -> (Self, Rc<RefCell<Vec<u8>>>) {
        let stereo_samples = Rc::new(RefCell::new(Vec::new()));
        (
            Self {
                stereo_samples: stereo_samples.clone(),
            },
            stereo_samples,
        )
    }
}

impl AudioBackend for CapturingAudioBackend {
    fn queue_samples(&mut self, samples: &[u8]) {
        self.stereo_samples.borrow_mut().extend(samples);
    }

    fn queue_stereo_samples(&mut self, samples: &[u8]) {
        self.stereo_samples.borrow_mut().extend(samples);
    }

    fn stop(&mut self) {}
}

#[test]
fn headless_run_steps_does_not_implicitly_mix_audio() {
    let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
    let program_start = 0x0001_0000;
    for offset in (0..16).step_by(2) {
        runner.bus.write_word(program_start + offset, 0x4E71); // NOP
    }
    runner.m68k.cpu.write_reg(Register::PC, program_start);
    runner.m68k.cpu.write_reg(Register::A7, 0x007F_FFC0);

    let mut chan = SndChannel::new(0x0039_38C8, false);
    chan.play_buffer(
        vec![0x90, 0x91, 0x92],
        OUTPUT_RATE << 16,
        PlaybackKind::Buffer,
        0,
    );
    runner.dispatcher.sound_manager.add_channel(chan);

    let (steps, running) = runner.run_steps(2, None);

    assert!(running);
    assert_eq!(steps, 2);
    assert_eq!(
        runner.audio_buffer_len(),
        0,
        "plain headless stepping must not consume sound buffers"
    );
    assert_eq!(runner.dispatcher.sound_manager.debug_samples_mixed, 0);

    runner.mix_audio(2);
    assert_eq!(runner.drain_audio(), vec![0x90, 0x91]);
    assert_eq!(runner.dispatcher.sound_manager.debug_samples_mixed, 2);
}

#[test]
fn headless_run_steps_advances_playback_needed_for_sound_callbacks() {
    let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
    let program_start = 0x0001_0000;
    runner.bus.write_word(program_start, 0x60FE); // BRA.S *
    runner.m68k.cpu.write_reg(Register::PC, program_start);
    runner.m68k.cpu.write_reg(Register::A7, 0x007F_FFC0);
    runner.set_instructions_per_tick(1);

    let chan_ptr = 0x0039_38C8;
    let callback_addr = 0x0001_1000;
    let callback_cmd = SndCommand {
        cmd: crate::sound::cmd::CALLBACK,
        param1: 0x1234,
        param2: 0x0056_7890,
    };
    let mut chan = SndChannel::new(chan_ptr, false);
    chan.callback_addr = callback_addr;
    chan.play_buffer(vec![0x90; 500], OUTPUT_RATE << 16, PlaybackKind::Buffer, 0);
    chan.queue_callback(callback_cmd.clone());
    runner.dispatcher.sound_manager.add_channel(chan);

    let (steps, running) = runner.run_steps(2, None);

    assert!(running);
    assert_eq!(steps, 2);
    assert_eq!(
        runner
            .dispatcher
            .sound_manager
            .pending_sound_callbacks
            .len(),
        1
    );
    assert!(matches!(
        &runner.dispatcher.sound_manager.pending_sound_callbacks[0],
        PendingSoundCallback::Command {
            architecture: actual_architecture,
            callback_addr: actual_callback,
            chan_ptr: actual_channel,
            cmd,
        } if *actual_architecture == CallbackTaskArchitecture::M68k
            && *actual_callback == callback_addr
            && *actual_channel == chan_ptr
            && cmd.cmd == callback_cmd.cmd
            && cmd.param1 == callback_cmd.param1
            && cmd.param2 == callback_cmd.param2
    ));
    assert_eq!(
        runner.audio_buffer_len(),
        500,
        "the complete callback-gated buffer should be captured before completion"
    );
}

#[test]
fn host_audio_backend_receives_silence_while_sound_manager_idle() {
    let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
    let (audio_backend, stereo_samples) = CapturingAudioBackend::new();
    runner.set_audio(Box::new(audio_backend));

    runner.mix_audio(4);

    assert_eq!(
        stereo_samples.borrow().as_slice(),
        &[0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0x80]
    );
    assert_eq!(
        runner.audio_buffer_len(),
        0,
        "host silence must not become captured guest audio"
    );
    assert_eq!(runner.dispatcher.sound_manager.debug_samples_mixed, 0);
}

#[test]
fn host_audio_backend_receives_low_rate_sample_hold_output() {
    let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
    let (audio_backend, stereo_samples) = CapturingAudioBackend::new();
    runner.set_audio(Box::new(audio_backend));

    let mut chan = SndChannel::new(0x0039_38C8, false);
    chan.play_buffer(
        vec![0x90, 0xA0],
        (OUTPUT_RATE / 2) << 16,
        PlaybackKind::Buffer,
        0,
    );
    runner.dispatcher.sound_manager.add_channel(chan);

    runner.mix_audio(4);

    assert_eq!(
        stereo_samples.borrow().as_slice(),
        &[
            0x90, 0x90, // source[0] at position 0.0
            0x90, 0x90, // source[0] held at position 0.5
            0xA0, 0xA0, // source[1] at position 1.0
            0xA0, 0xA0, // source[1] held at position 1.5
        ],
        "GUI/host audio path must receive the sample-hold low-rate output, not the old linear midpoint"
    );
    assert_eq!(runner.dispatcher.sound_manager.debug_samples_mixed, 4);
}

#[test]
fn ppc_double_buffer_playback_feeds_consecutive_host_audio_buffers() {
    let channel = 0x0500_1000;
    let header = PPC_DATA_BASE + 0x100;
    let buffer0 = PPC_DATA_BASE + 0x200;
    let buffer1 = PPC_DATA_BASE + 0x300;
    let expected = vec![0x80, 0x90, 0x70, 0xa0];
    let sound = PpcSoundState::default();
    sound
        .manager
        .replace_double_buffer_playbacks(vec![PpcSoundDoubleBufferPlaybackRecord {
            channel,
            header,
            buffers: [buffer0, buffer1],
            callback: 0,
            callback_architecture: CallbackTaskArchitecture::PowerPc,
            sample_rate_fixed: crate::sound::OUTPUT_RATE << 16,
            num_channels: 1,
            sample_size: 8,
            compression_id: 0,
            format: 0,
            packet_size: 0,
            current_buffer_index: 0,
            callback_pending_mask: 0,
            active: true,
            host_initialized: false,
            host_buffer_loaded: false,
        }]);
    let mut app = halted_ppc_app_with_sound(sound);
    {
        let ppc_app = app.ppc.as_mut().expect("PPC app");
        ppc_app.memory.add_region(PPC_DATA_BASE, vec![0; 0x400]);
        ppc_app.memory.write_u32_be(buffer0, 2).unwrap();
        ppc_app.memory.write_u32_be(buffer0 + 4, 0x01).unwrap();
        ppc_app
            .memory
            .write_bytes(buffer0 + 16, &expected[..2])
            .unwrap();
        ppc_app.memory.write_u32_be(buffer1, 2).unwrap();
        ppc_app.memory.write_u32_be(buffer1 + 4, 0x05).unwrap();
        ppc_app
            .memory
            .write_bytes(buffer1 + 16, &expected[2..])
            .unwrap();
    }
    let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
    runner.init_app(&app);

    let (steps, running) = runner.run_steps_with_audio(8, None, expected.len());

    assert_eq!(steps, 1);
    assert!(!running);
    assert_eq!(runner.drain_audio(), expected);
    assert_eq!(
        runner.dispatcher().sound_manager.debug_double_buffer_count,
        1
    );
    assert_eq!(runner.dispatcher().sound_manager.debug_samples_mixed, 4);
    let channel = runner
        .dispatcher()
        .sound_manager
        .channels
        .iter()
        .find(|candidate| candidate.guest_ptr == channel)
        .expect("process Sound Manager channel");
    assert_eq!(channel.debug_double_buffer_loads, 2);
    assert_eq!(channel.debug_double_buffer_non_silent_loads, 2);
    assert_eq!(channel.debug_double_buffer_frames_loaded, 4);
    assert_eq!(channel.debug_double_buffer_non_silent_frames, 3);
    assert_eq!(channel.debug_double_buffer_captured_samples, expected);
    let playback = runner
        .native
        .application()
        .expect("PPC app should stay loaded")
        .sound
        .manager
        .double_buffer_playbacks[0];
    assert!(!playback.active);
    assert!(!playback.host_buffer_loaded);
    assert_eq!(playback.current_buffer_index, 1);
}

#[test]
fn ppc_decoded_buffer_command_feeds_host_audio_buffer() {
    let channel = 0x0500_1000;
    let samples = vec![0x80, 0x90, 0x70, 0xa0];
    let sound = PpcSoundState::default();
    sound.manager.play_buffer_command_for_architecture(
        channel,
        samples.clone(),
        crate::sound::OUTPUT_RATE << 16,
        CallbackTaskArchitecture::M68k,
    );
    let app = halted_ppc_app_with_sound(sound);
    let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
    runner.init_app(&app);

    let (steps, running) = runner.run_steps_with_audio(8, None, samples.len());

    assert_eq!(steps, 1);
    assert!(!running);
    assert_eq!(runner.drain_audio(), samples);
    assert_eq!(runner.dispatcher().sound_manager.debug_cmd_count, 1);
    assert_eq!(runner.dispatcher().sound_manager.debug_buffer_cmd_count, 1);
    assert_eq!(runner.dispatcher().sound_manager.debug_samples_mixed, 4);
}

#[test]
fn ppc_decoded_file_playback_feeds_host_audio_buffer() {
    let channel = 0x0500_1000;
    let callback_entry = PPC_CODE_BASE + 0x40;
    let completion = PPC_DATA_BASE + 0x20;
    let completion_tvector = PPC_DATA_BASE + 0x80;
    let samples = vec![0x80, 0x90, 0x70, 0x80];
    let mut preview = [0; 16];
    preview[..samples.len()].copy_from_slice(&samples);
    let mut app = halted_ppc_app_with_sound(PpcSoundState {
        manager: Default::default(),
        queued_commands: Vec::new(),
        immediate_commands: Vec::new(),
        file_playbacks: vec![PpcSoundFilePlaybackRecord {
            channel,
            ref_num: 128,
            resource_id: -1,
            buffer_size: 20_480,
            buffer: 0,
            selection: 0,
            completion,
            completion_command: None,
            async_play: true,
            aiff: None,
            decoded_aiff: Some(PpcDecodedAiffSamples {
                sample_rate_fixed: crate::sound::OUTPUT_RATE << 16,
                sample_count: samples.len() as u32,
                preview_len: samples.len() as u8,
                preview,
            }),
        }],
        decoded_file_playbacks: vec![PpcDecodedAiffPlaybackRecord {
            file_playback_index: 0,
            channel,
            sample_rate_fixed: crate::sound::OUTPUT_RATE << 16,
            samples: samples.clone(),
        }],
        completion_invocations: Vec::new(),
        sys_beep_count: 0,
        last_sys_beep_duration: 0,
        start_count: 1,
        pause_count: 0,
        stop_count: 0,
        double_buffer_play_count: 0,
        last_double_buffer_channel: 0,
        last_double_buffer_header: 0,
    });
    {
        let ppc_app = app.ppc.as_mut().expect("PPC app");
        ppc_app.sound.manager.play_file_buffer(
            channel,
            samples.clone(),
            crate::sound::OUTPUT_RATE << 16,
            Some((CallbackTaskArchitecture::PowerPc, completion)),
        );
        let stw_r3_0_r2 = (36u32 << 26) | (3u32 << 21) | (2u32 << 16);
        let mut callback_bytes = Vec::new();
        callback_bytes.extend_from_slice(&stw_r3_0_r2.to_be_bytes());
        callback_bytes.extend_from_slice(&0x4e80_0020u32.to_be_bytes());
        ppc_app.memory.add_region(callback_entry, callback_bytes);
        ppc_app.memory.add_region(PPC_DATA_BASE, vec![0; 0x100]);
        ppc_app.rtoc = PPC_DATA_BASE;
        ppc_app.cpu.gpr[2] = PPC_DATA_BASE;

        ppc_app
            .memory
            .write_u16_be(completion, 0xAAFE)
            .expect("write goMixedModeTrap");
        ppc_app
            .memory
            .write_u8(completion + 2, 7)
            .expect("write descriptor version");
        ppc_app
            .memory
            .write_u16_be(completion + 10, 0)
            .expect("write routine count");
        let record = completion + 12;
        ppc_app
            .memory
            .write_u8(record + 5, 1)
            .expect("write PowerPC ISA");
        ppc_app
            .memory
            .write_u16_be(record + 6, 0x0004)
            .expect("write routine flags");
        ppc_app
            .memory
            .write_u32_be(record + 8, completion_tvector)
            .expect("write proc descriptor");
        ppc_app
            .memory
            .write_u32_be(completion_tvector, callback_entry)
            .expect("write callback TVector entry");
        ppc_app
            .memory
            .write_u32_be(completion_tvector + 4, PPC_DATA_BASE)
            .expect("write callback TVector RTOC");
    }
    let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
    runner.init_app(&app);

    let (steps, running) = runner.run_steps_with_audio(8, None, samples.len());

    assert_eq!(steps, 1);
    assert!(!running);
    assert_eq!(runner.drain_audio(), samples);
    assert_eq!(runner.dispatcher().sound_manager.debug_file_play_count, 1);
    assert_eq!(runner.dispatcher().sound_manager.debug_samples_mixed, 4);
    let ppc_app = runner
        .native
        .application()
        .expect("PPC app should stay loaded");
    assert_eq!(ppc_app.sound.manager.file_playback_paused(channel), None);
    assert!(ppc_app.sound.manager.pending_sound_callbacks.is_empty());
    let mut memory = ppc_app.memory.clone();
    assert_eq!(memory.read_u32_be(PPC_DATA_BASE), Some(channel));
    assert_eq!(ppc_app.sound.completion_invocations.len(), 1);
    let invocation = ppc_app.sound.completion_invocations[0];
    assert_eq!(invocation.file_playback_index, 0);
    assert_eq!(invocation.channel, channel);
    assert_eq!(invocation.completion, completion);
    assert_eq!(invocation.callback_entry, callback_entry);
    assert_eq!(invocation.callback_rtoc, PPC_DATA_BASE);
    assert_eq!(invocation.tick, 0);
    assert_eq!(invocation.instruction_count, 1);
    assert_eq!(invocation.scheduled_tick, invocation.tick);
    assert_eq!(
        invocation.scheduled_instruction_count,
        invocation.instruction_count
    );
    assert_eq!(invocation.cycles, 2);
    assert_eq!(
        invocation.result,
        PpcRunResult::Halted {
            pc: PPC_HALT_PC,
            cycles: 2
        }
    );
    assert_eq!(invocation.unsupported_import_index, None);
}

#[test]
fn ppc_decoded_file_playback_waits_for_process_audio_cursor() {
    let channel = 0x0500_1000;
    let callback_entry = PPC_CODE_BASE + 0x40;
    let samples = vec![0x80, 0x90, 0x70, 0x80];
    let mut preview = [0; 16];
    preview[..samples.len()].copy_from_slice(&samples);
    let mut app = halted_ppc_app_with_sound(PpcSoundState {
        manager: Default::default(),
        queued_commands: Vec::new(),
        immediate_commands: Vec::new(),
        file_playbacks: vec![PpcSoundFilePlaybackRecord {
            channel,
            ref_num: 128,
            resource_id: -1,
            buffer_size: 20_480,
            buffer: 0,
            selection: 0,
            completion: callback_entry,
            completion_command: None,
            async_play: true,
            aiff: None,
            decoded_aiff: Some(PpcDecodedAiffSamples {
                sample_rate_fixed: crate::sound::OUTPUT_RATE << 16,
                sample_count: samples.len() as u32,
                preview_len: samples.len() as u8,
                preview,
            }),
        }],
        decoded_file_playbacks: vec![PpcDecodedAiffPlaybackRecord {
            file_playback_index: 0,
            channel,
            sample_rate_fixed: crate::sound::OUTPUT_RATE << 16,
            samples: samples.clone(),
        }],
        completion_invocations: Vec::new(),
        sys_beep_count: 0,
        last_sys_beep_duration: 0,
        start_count: 1,
        pause_count: 0,
        stop_count: 0,
        double_buffer_play_count: 0,
        last_double_buffer_channel: 0,
        last_double_buffer_header: 0,
    });
    {
        let ppc_app = app.ppc.as_mut().expect("PPC app");
        ppc_app.sound.manager.play_file_buffer(
            channel,
            samples.clone(),
            crate::sound::OUTPUT_RATE << 16,
            Some((CallbackTaskArchitecture::PowerPc, callback_entry)),
        );
        ppc_app
            .memory
            .write_u32_be(PPC_CODE_BASE, 0x4800_0000)
            .expect("rewrite entry as infinite branch");
        let stw_r3_0_r2 = (36u32 << 26) | (3u32 << 21) | (2u32 << 16);
        let mut callback_bytes = Vec::new();
        callback_bytes.extend_from_slice(&stw_r3_0_r2.to_be_bytes());
        callback_bytes.extend_from_slice(&0x4e80_0020u32.to_be_bytes());
        ppc_app.memory.add_region(callback_entry, callback_bytes);
        ppc_app.memory.add_region(PPC_DATA_BASE, vec![0; 0x100]);
        ppc_app.rtoc = PPC_DATA_BASE;
        ppc_app.cpu.gpr[2] = PPC_DATA_BASE;
    }

    let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
    runner.init_app(&app);
    let (steps, running) = runner.run_steps_with_audio(1, None, 0);
    assert_eq!(steps, 1);
    assert!(running);
    {
        let ppc_app = runner
            .native
            .application()
            .expect("PPC app should stay loaded");
        assert_eq!(
            ppc_app.sound.manager.file_playback_paused(channel),
            Some(false)
        );
        assert!(ppc_app.sound.completion_invocations.is_empty());
    }

    let default_budget =
        crate::machine_profile::DEFAULT_HOST_EXECUTION_POLICY.scripted_instructions_per_tick;
    let (steps, running) = runner.run_steps_with_audio(default_budget as usize, None, 0);

    assert_eq!(steps, default_budget as usize);
    assert!(running);
    assert!(runner.drain_audio().is_empty());
    assert_eq!(runner.guest_tick(), 1);
    assert_eq!(runner.dispatcher().sound_manager.debug_file_play_count, 1);
    assert_eq!(runner.dispatcher().sound_manager.debug_samples_mixed, 0);
    let ppc_app = runner
        .native
        .application()
        .expect("PPC app should stay loaded");
    assert_eq!(
        ppc_app.sound.manager.file_playback_paused(channel),
        Some(false)
    );
    assert!(ppc_app.sound.completion_invocations.is_empty());

    let (steps, running) = runner.run_steps_with_audio(1, None, samples.len());
    assert_eq!(steps, 1);
    assert!(running);
    assert_eq!(runner.drain_audio(), samples);
    let ppc_app = runner
        .native
        .application()
        .expect("PPC app should stay loaded");
    assert_eq!(ppc_app.sound.manager.file_playback_paused(channel), None);
    assert!(ppc_app.sound.manager.pending_sound_callbacks.is_empty());
    let mut memory = ppc_app.memory.clone();
    assert_eq!(memory.read_u32_be(PPC_DATA_BASE), Some(channel));
    assert_eq!(ppc_app.sound.completion_invocations.len(), 1);
    let invocation = ppc_app.sound.completion_invocations[0];
    assert_eq!(invocation.file_playback_index, 0);
    assert_eq!(invocation.channel, channel);
    assert_eq!(invocation.completion, callback_entry);
    assert_eq!(invocation.callback_entry, callback_entry);
    assert_eq!(invocation.callback_rtoc, PPC_DATA_BASE);
    assert_eq!(invocation.tick, 1);
    assert_eq!(invocation.scheduled_tick, invocation.tick);
    assert_eq!(
        invocation.scheduled_instruction_count,
        invocation.instruction_count
    );
    assert_eq!(invocation.cycles, 2);
    assert_eq!(
        invocation.result,
        PpcRunResult::Halted {
            pc: PPC_HALT_PC,
            cycles: 2
        }
    );
    assert_eq!(invocation.unsupported_import_index, None);
}

#[test]
fn ppc_decoded_file_playback_pause_resume_uses_process_cursor() {
    let channel = 0x0500_1000;
    let samples = vec![0x80, 0x90, 0x70, 0x80];
    let mut sound = PpcSoundState::default();
    sound.file_playbacks.push(PpcSoundFilePlaybackRecord {
        channel,
        ref_num: 128,
        resource_id: -1,
        buffer_size: 20_480,
        buffer: 0,
        selection: 0,
        completion: 0,
        completion_command: None,
        async_play: true,
        aiff: None,
        decoded_aiff: None,
    });
    sound.manager.play_file_buffer(
        channel,
        samples.clone(),
        crate::sound::OUTPUT_RATE << 16,
        Some((CallbackTaskArchitecture::PowerPc, 0)),
    );
    assert_eq!(sound.manager.toggle_file_paused(channel), Some(true));
    let app = halted_ppc_app_with_sound(sound);
    let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
    runner.init_app(&app);

    runner.mix_host_audio(samples.len());
    let ppc_app = runner
        .native
        .application()
        .expect("PPC app should stay loaded");
    assert_eq!(
        ppc_app.sound.manager.file_playback_paused(channel),
        Some(true)
    );
    assert!(ppc_app.sound.manager.pending_sound_callbacks.is_empty());

    assert_eq!(
        runner.dispatcher.sound_manager.toggle_file_paused(channel),
        Some(false)
    );
    runner.mix_host_audio(samples.len());

    let ppc_app = runner
        .native
        .application()
        .expect("PPC app should stay loaded");
    assert_eq!(ppc_app.sound.manager.file_playback_paused(channel), None);
    assert!(ppc_app.sound.manager.pending_sound_callbacks.is_empty());
    assert!(ppc_app.sound.completion_invocations.is_empty());
}
