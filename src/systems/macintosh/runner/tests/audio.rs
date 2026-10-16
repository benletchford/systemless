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
fn legacy_sound_reaches_host_and_browser_outputs_at_guest_frame_boundaries() {
    use crate::memory::globals::addr;
    let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
    let (audio_backend, stereo_samples) = CapturingAudioBackend::new();
    runner.set_audio(Box::new(audio_backend));
    let sound_base = runner.bus.read_long(addr::SOUND_BASE);
    let via = runner.bus.read_long(addr::VIA_BASE);
    runner.bus.write_byte(via, 0);
    runner
        .bus
        .write_byte(via + crate::memory::bus::LEGACY_SOUND_PORT_A_OFFSET, 15);
    for offset in (0..740).step_by(2) {
        runner.bus.write_word(sound_base + offset, 0xA5FF);
    }
    runner.force_advance_guest_tick();
    // Replacing the RAM contents after capture must not alter the queued frame.
    for offset in (0..740).step_by(2) {
        runner.bus.write_byte(sound_base + offset, 0x55);
    }
    runner.mix_audio(32);
    assert_eq!(runner.drain_audio(), vec![0xA5; 32]);
    assert_eq!(*stereo_samples.borrow(), vec![0xA5; 64]);

    // Disabling sound stops capture, but the frame already queued is guest
    // time that has passed, so it still plays out.
    runner.bus.write_byte(via, 0x80);
    runner.force_advance_guest_tick();
    runner.mix_audio(400);
    let drained = runner.drain_audio();
    assert_eq!(drained.len(), 400);
    assert!(drained[..300].iter().all(|&sample| sample == 0xA5));
    assert!(drained[340..].iter().all(|&sample| sample == 0x80));
    runner.mix_audio(32);
    assert!(runner.drain_audio().is_empty());
    let stereo = stereo_samples.borrow();
    assert!(stereo[stereo.len() - 64..]
        .iter()
        .all(|&sample| sample == 128));
}

#[test]
fn zero_first_legacy_frame_is_silent_in_host_and_browser_outputs() {
    use crate::memory::globals::addr;
    let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
    let (audio_backend, stereo_samples) = CapturingAudioBackend::new();
    runner.set_audio(Box::new(audio_backend));
    let sound_base = runner.bus.read_long(addr::SOUND_BASE);
    let via = runner.bus.read_long(addr::VIA_BASE);
    runner.bus.write_byte(via, 0);
    runner
        .bus
        .write_byte(via + crate::memory::bus::LEGACY_SOUND_PORT_A_OFFSET, 15);
    for offset in (0..740).step_by(2) {
        runner.bus.write_word(sound_base + offset, 0x00ff);
    }
    runner.force_advance_guest_tick();
    runner.force_advance_guest_tick(); // same zero RAM, now a valid later frame
    runner.mix_audio(32);
    let mut mono = runner.drain_audio();
    runner.mix_audio(668);
    mono.extend(runner.drain_audio());
    let first_frame_outputs = (f64::from(crate::sound::OUTPUT_RATE) / DEFAULT_VBL_HZ).ceil() as usize;
    assert_eq!(mono.len(), 700);
    assert_eq!(&mono[..first_frame_outputs], vec![128; first_frame_outputs]);
    assert_eq!(&mono[first_frame_outputs..], vec![0; 700 - first_frame_outputs]);
    let expected_stereo: Vec<u8> = mono.iter().flat_map(|&sample| [sample; 2]).collect();
    assert_eq!(*stereo_samples.borrow(), expected_stereo);
    assert_eq!(runner.bus.read_word(sound_base), 0x00ff);
}

#[test]
fn legacy_sound_api_mute_silences_the_device_stream() {
    use crate::memory::globals::addr;
    let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
    let sound_base = runner.bus.read_long(addr::SOUND_BASE);
    let via = runner.bus.read_long(addr::VIA_BASE);
    runner.bus.write_byte(via, 0);
    for offset in (0..740).step_by(2) {
        runner.bus.write_word(sound_base + offset, 0xA5FF);
    }
    runner
        .dispatcher
        .sound_manager
        .with_mut(|manager| manager.legacy_speaker_muted = true);
    runner.force_advance_guest_tick();
    runner.mix_audio(32);
    assert_eq!(runner.drain_audio(), vec![0x80; 32]);
    assert_eq!(
        runner.bus.read_byte(via),
        0,
        "the device itself stays enabled"
    );
}

const RING_PC: u32 = 0x0002_0000;
const RING_SP: u32 = 0x007F_FFC0;
const RING_TASK: u32 = 0x0020_2000;
const RING_CALLBACK: u32 = 0x0004_1000;
const RING_COUNTER: u32 = 0x0020_3000;

/// Run a VBL task that enables the legacy sound device and fills SoundBase
/// as a ring: 338 samples from word 32, then 32 from word 0, continuing one
/// incrementing sample stream. Returns the device's queued source samples.
fn run_ring_driver(instructions_per_tick: u32, presented: bool, retraces: usize) -> Vec<u8> {
    let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
    runner.set_instructions_per_tick(instructions_per_tick);
    if presented {
        // Presented sessions withdraw direct fast memory, so compiled code
        // stores through the tracked window and its store filter.
        runner.prepare_text_presentation();
    }
    runner.bus.write_word(RING_PC, 0x5287); // ADDQ.L #1,D7
    runner.bus.write_word(RING_PC + 2, 0x60FC); // BRA.S RING_PC
    runner.m68k.cpu.write_reg(crate::cpu::Register::PC, RING_PC);
    runner.m68k.cpu.write_reg(crate::cpu::Register::A7, RING_SP);
    runner.m68k.cpu.core.set_sr_noint_nosp(0x2000);
    #[rustfmt::skip]
    let words = [
        0x317C, 0x0001, 0x000A, // MOVE.W #1,10(A0)       re-arm
        0x2278, 0x01D4, // MOVEA.L $1D4.W,A1            VBase
        0x0891, 0x0007, // BCLR #7,(A1)                 enable sound
        0x45F9, (RING_COUNTER >> 16) as u16, RING_COUNTER as u16, // LEA counter,A2
        0x1012, // MOVE.B (A2),D0
        0x2278, 0x0266, // MOVEA.L $266.W,A1            SoundBase
        0xD2FC, 0x0040, // ADDA.W #$40,A1               word 32
        0x323C, 337, // MOVE.W #337,D1
        0x1280, 0x5489, 0x5200, 0x51C9, 0xFFF8, // MOVE.B D0,(A1); ADDQ.L #2,A1; ADDQ.B #1,D0; DBRA
        0x2278, 0x0266, // MOVEA.L $266.W,A1            word 0
        0x721F, // MOVEQ #31,D1
        0x1280, 0x5489, 0x5200, 0x51C9, 0xFFF8,
        0x1480, // MOVE.B D0,(A2)
        0x4E75, // RTS
    ];
    for (offset, word) in words.into_iter().enumerate() {
        runner
            .bus
            .write_word(RING_CALLBACK + offset as u32 * 2, word);
    }
    runner.bus.write_word(RING_TASK + 4, 1);
    runner.bus.write_long(RING_TASK + 6, RING_CALLBACK);
    runner.bus.write_word(RING_TASK + 10, 1);
    runner
        .dispatcher
        .vbl_tasks
        .push(crate::trap::dispatch::VblTask {
            task_ptr: RING_TASK,
            architecture: crate::callback_manager::CallbackTaskArchitecture::M68k,
            slot: None,
            pending: false,
        });
    for _ in 0..retraces {
        runner.advance_guest_tick();
        let tick = runner.guest_tick();
        let (_, running) = runner.run_steps(4000, Some(tick));
        assert!(running);
        assert!(runner.active_interrupt_callback.is_none());
    }
    assert_eq!(runner.legacy_sound.rotation(), [32, 0]);
    runner.legacy_sound.queued()
}

#[test]
fn vbl_ring_driver_streams_continuously() {
    // Long enough for the fill loops to be compiled where a JIT exists.
    for instructions_per_tick in [1_000, 415_600] {
        let queued = run_ring_driver(instructions_per_tick, false, 400);
        assert_eq!(queued.len(), 370 * 8, "{instructions_per_tick}");
        for (i, pair) in queued.windows(2).enumerate() {
            assert_eq!(
                pair[1],
                pair[0].wrapping_add(1),
                "sample {i} at {instructions_per_tick} instructions per tick"
            );
        }
        assert_eq!(
            run_ring_driver(instructions_per_tick, true, 400),
            queued,
            "presented and unpresented store paths agree"
        );
    }
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
