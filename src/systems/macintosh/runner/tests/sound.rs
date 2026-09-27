use super::*;
use crate::callback_manager::CallbackTaskArchitecture;
use crate::memory::MacMemoryBus;
use crate::sound::{
    DoubleBufferState, PendingDoubleBackCallback, PendingSoundCallback, SndChannel, SndCommand,
    OUTPUT_RATE,
};

#[test]
fn sound_doubleback_callback_resume_restores_ccr_before_branch() {
    let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
    let interrupted_pc = 0x0001_0000;
    let interrupted_sp = 0x007F_FFC0;
    let header_ptr = 0x0020_0000;
    let exhausted_buf_ptr = 0x0020_1000;

    // BEQ.s -> MOVEQ #2,D0 path should be taken when Z is preserved.
    runner.bus.write_word(interrupted_pc, 0x6704);
    runner.bus.write_word(interrupted_pc + 2, 0x7001);
    runner.bus.write_word(interrupted_pc + 4, 0x6002);
    runner.bus.write_word(interrupted_pc + 6, 0x7002);
    runner.bus.write_word(interrupted_pc + 8, 0x4E71);

    runner.m68k.cpu.write_reg(Register::PC, interrupted_pc);
    runner.m68k.cpu.write_reg(Register::A7, interrupted_sp);
    runner.m68k.cpu.write_reg(Register::D0, 0);
    runner.m68k.cpu.core.set_ccr(0x04);

    runner.bus.write_long(header_ptr + 12, exhausted_buf_ptr);
    runner.bus.write_long(exhausted_buf_ptr + 4, 0x0000_0001);
    runner
        .dispatcher
        .sound_manager
        .queue_doubleback_callback(PendingDoubleBackCallback {
            callback_addr: 0x0004_1234,
            chan_ptr: 0x0039_38C8,
            header_ptr,
            exhausted_buffer_index: 0,
        });

    runner.fire_sound_doubleback_callbacks();

    let active = runner
        .active_interrupt_callback
        .expect("sound callback should have been armed");
    assert!(matches!(
        active.source,
        ActiveInterruptCallbackSource::SoundDoubleBack
    ));
    assert_eq!(active.resume_pc, interrupted_pc);
    assert_eq!(active.resume_sp, interrupted_sp);

    // Simulate the trampoline returning to interrupted code with CCR clobbered.
    runner.m68k.cpu.write_reg(Register::PC, interrupted_pc);
    runner.m68k.cpu.write_reg(Register::A7, interrupted_sp);
    runner.m68k.cpu.core.set_ccr(0);

    let (steps, running) = runner.run_steps(3, None);

    assert!(running);
    assert_eq!(steps, 3);
    assert_eq!(runner.m68k.cpu.read_reg(Register::D0), 2);
    assert!(runner.active_interrupt_callback.is_none());
}

#[test]
fn sound_doubleback_callback_trampoline_stacks_classic_pascal_order() {
    let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
    let interrupted_pc = 0x0001_0000;
    let interrupted_sp = 0x007F_FFC0;
    let callback_addr = runner.bus.alloc(2);
    let header_ptr = 0x0020_0000;
    let chan_ptr = 0x0039_38C8;
    let exhausted_buf_ptr = 0x0020_1000;

    runner.bus.write_word(callback_addr, 0x4E75); // RTS without popping args.
    runner.bus.write_word(interrupted_pc, 0x60FE); // BRA.S *-0
    runner.m68k.cpu.write_reg(Register::PC, interrupted_pc);
    runner.m68k.cpu.write_reg(Register::A7, interrupted_sp);

    runner.bus.write_long(header_ptr + 12, exhausted_buf_ptr);
    runner.bus.write_long(exhausted_buf_ptr + 4, 0x0000_0001);
    runner
        .dispatcher
        .sound_manager
        .queue_doubleback_callback(PendingDoubleBackCallback {
            callback_addr,
            chan_ptr,
            header_ptr,
            exhausted_buffer_index: 0,
        });

    runner.fire_sound_doubleback_callbacks();
    let (_steps, running) = runner.run_steps(24, None);

    assert!(running);
    assert!(runner.active_interrupt_callback.is_none());
    let saved_regs_sp = interrupted_sp - 4 - 32;
    assert_eq!(
        runner.bus.read_long(saved_regs_sp - 4),
        chan_ptr,
        "the first declared Pascal argument is pushed first"
    );
    assert_eq!(
        runner.bus.read_long(saved_regs_sp - 8),
        exhausted_buf_ptr,
        "the last declared Pascal argument is nearest the return address"
    );
}

fn write_double_buffer(bus: &mut MacMemoryBus, ptr: u32, samples: &[u8]) {
    bus.write_long(ptr, samples.len() as u32);
    bus.write_long(ptr + 4, 0x0000_0001);
    for (offset, sample) in samples.iter().copied().enumerate() {
        bus.write_byte(ptr + 16 + offset as u32, sample);
    }
}

#[test]
fn mix_audio_loads_ready_double_buffer_without_boundary_silence() {
    let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
    let chan_ptr = 0x0039_38C8;
    let callback_addr = 0x0004_1234;
    let header_ptr = runner.bus.alloc(24);
    let buf0_ptr = runner.bus.alloc(18);
    let buf1_ptr = runner.bus.alloc(18);

    runner.bus.write_word(header_ptr, 1);
    runner.bus.write_word(header_ptr + 2, 8);
    runner.bus.write_long(header_ptr + 8, OUTPUT_RATE << 16);
    runner.bus.write_long(header_ptr + 12, buf0_ptr);
    runner.bus.write_long(header_ptr + 16, buf1_ptr);
    runner.bus.write_long(header_ptr + 20, callback_addr);
    write_double_buffer(&mut runner.bus, buf0_ptr, &[0x90, 0x91]);
    write_double_buffer(&mut runner.bus, buf1_ptr, &[0xA0, 0xA1]);

    let mut chan = SndChannel::new(chan_ptr, false);
    chan.double_buffer = Some(DoubleBufferState {
        header_ptr,
        current_buffer: 0,
        callback_addr,
        chan_ptr,
        sample_rate: OUTPUT_RATE << 16,
        num_channels: 1,
        sample_size: 8,
        last_buffer_seen: false,
        waiting_for_callback: false,
        pending_callback_buffers: [false; 2],
    });
    crate::trap::TrapDispatcher::load_double_buffer_samples(
        &mut runner.bus,
        &mut chan,
        buf0_ptr,
        OUTPUT_RATE << 16,
        1,
        8,
    );
    runner.dispatcher.sound_manager.add_channel(chan);

    runner.mix_audio(3);

    assert_eq!(
        runner.audio_buffer,
        vec![0x90, 0x91, 0xA0],
        "host mixing must continue into the ready paired buffer, not emit boundary silence"
    );
    assert_eq!(
        runner.bus.read_long(buf0_ptr + 4) & 0x01,
        0x01,
        "dbBufferReady stays set until the doubleback callback starts"
    );
    assert_eq!(
        runner.bus.read_long(buf1_ptr + 4) & 0x01,
        0x01,
        "the paired buffer is still marked ready while it is playing"
    );
    assert_eq!(
        runner.dispatcher.sound_manager.pending_callbacks.len(),
        1,
        "exhausting buffer 0 still queues its doubleback refill"
    );
    assert_eq!(
        runner.dispatcher.sound_manager.pending_callbacks[0].exhausted_buffer_index,
        0
    );

    let chan = &runner.dispatcher.sound_manager.channels[0];
    assert!(chan.is_playing(), "buffer 1 should still be playing");
    let db = chan.double_buffer.as_ref().expect("double-buffer active");
    assert_eq!(db.current_buffer, 1);
    assert!(db.waiting_for_callback);
}

#[test]
fn mix_audio_can_queue_other_doubleback_while_callback_is_active() {
    let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
    let chan_ptr = 0x0039_38C8;
    let callback_addr = 0x0004_1234;
    let header_ptr = runner.bus.alloc(24);
    let buf0_ptr = runner.bus.alloc(17);
    let buf1_ptr = runner.bus.alloc(17);
    let interrupted_pc = 0x0001_0000;
    let interrupted_sp = 0x007F_FFC0;

    runner.bus.write_word(interrupted_pc, 0x4E71);
    runner.m68k.cpu.write_reg(Register::PC, interrupted_pc);
    runner.m68k.cpu.write_reg(Register::A7, interrupted_sp);

    runner.bus.write_word(header_ptr, 1);
    runner.bus.write_word(header_ptr + 2, 8);
    runner.bus.write_long(header_ptr + 8, OUTPUT_RATE << 16);
    runner.bus.write_long(header_ptr + 12, buf0_ptr);
    runner.bus.write_long(header_ptr + 16, buf1_ptr);
    runner.bus.write_long(header_ptr + 20, callback_addr);
    write_double_buffer(&mut runner.bus, buf0_ptr, &[0xA0]);
    runner.bus.write_long(buf1_ptr, 1);
    runner.bus.write_long(buf1_ptr + 4, 0);

    let mut chan = SndChannel::new(chan_ptr, false);
    chan.double_buffer = Some(DoubleBufferState {
        header_ptr,
        current_buffer: 0,
        callback_addr,
        chan_ptr,
        sample_rate: OUTPUT_RATE << 16,
        num_channels: 1,
        sample_size: 8,
        last_buffer_seen: false,
        waiting_for_callback: false,
        pending_callback_buffers: [false; 2],
    });
    crate::trap::TrapDispatcher::load_double_buffer_samples(
        &mut runner.bus,
        &mut chan,
        buf0_ptr,
        OUTPUT_RATE << 16,
        1,
        8,
    );
    runner.dispatcher.sound_manager.add_channel(chan);

    runner.mix_audio(1);
    assert_eq!(runner.dispatcher.sound_manager.pending_callbacks.len(), 1);
    assert!(
        runner.dispatcher.sound_manager.channels[0]
            .double_buffer
            .as_ref()
            .expect("double-buffer active")
            .waiting_for_callback
    );

    runner.fire_sound_doubleback_callbacks();
    assert!(matches!(
        runner
            .active_interrupt_callback
            .expect("doubleback callback should be active")
            .source,
        ActiveInterruptCallbackSource::SoundDoubleBack
    ));
    assert!(
        runner.dispatcher.sound_manager.channels[0]
            .double_buffer
            .as_ref()
            .expect("double-buffer active")
            .waiting_for_callback,
        "callback remains outstanding until guest refills a buffer"
    );

    runner.mix_audio(16);

    assert_eq!(
        runner.dispatcher.sound_manager.pending_callbacks.len(),
        1,
        "the paired unready buffer may queue its own callback while buffer 0 is active"
    );
    assert_eq!(
        runner.dispatcher.sound_manager.pending_callbacks[0].exhausted_buffer_index, 1,
        "buffer 0 must not be duplicated; buffer 1 gets the new callback"
    );
    let db = runner.dispatcher.sound_manager.channels[0]
        .double_buffer
        .as_ref()
        .expect("double-buffer active");
    assert!(db.waiting_for_callback);
    assert_eq!(db.pending_callback_buffers, [true, true]);
}

#[test]
fn mix_audio_does_not_load_ready_double_buffer_while_callback_is_active() {
    let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
    let chan_ptr = 0x0039_38C8;
    let callback_addr = 0x0004_1234;
    let header_ptr = runner.bus.alloc(24);
    let buf0_ptr = runner.bus.alloc(17);

    runner.bus.write_word(header_ptr, 1);
    runner.bus.write_word(header_ptr + 2, 8);
    runner.bus.write_long(header_ptr + 8, OUTPUT_RATE << 16);
    runner.bus.write_long(header_ptr + 12, buf0_ptr);
    runner.bus.write_long(header_ptr + 20, callback_addr);
    write_double_buffer(&mut runner.bus, buf0_ptr, &[0xA0]);

    let mut chan = SndChannel::new(chan_ptr, false);
    chan.double_buffer = Some(DoubleBufferState {
        header_ptr,
        current_buffer: 0,
        callback_addr,
        chan_ptr,
        sample_rate: OUTPUT_RATE << 16,
        num_channels: 1,
        sample_size: 8,
        last_buffer_seen: false,
        waiting_for_callback: true,
        pending_callback_buffers: [true, false],
    });
    runner.dispatcher.sound_manager.add_channel(chan);
    runner.active_interrupt_callback = Some(ActiveInterruptCallback {
        source: ActiveInterruptCallbackSource::SoundDoubleBack,
        resume_pc: 0x0001_0000,
        resume_sp: 0x007F_FFC0,
        d_regs: [0; 8],
        a_regs: [0; 8],
        sr: 0x2000,
        ccr: 0,
        restore_port: None,
    });

    runner.mix_audio(1);

    assert_eq!(
        runner.bus.read_long(buf0_ptr + 4) & 0x01,
        0x01,
        "ready buffer must not be consumed before the callback returns"
    );
    assert!(
        !runner.dispatcher.sound_manager.channels[0].is_playing(),
        "callback-active buffer load should be deferred"
    );
    assert_eq!(
        runner.audio_buffer,
        vec![0x80],
        "the host stream stays alive with silence while waiting"
    );

    runner.active_interrupt_callback = None;
    runner.try_load_pending_double_buffers();

    assert_eq!(
        runner.bus.read_long(buf0_ptr + 4) & 0x01,
        0x01,
        "returned callback buffer stays marked ready while playback owns it"
    );
    assert!(
        runner.dispatcher.sound_manager.channels[0].is_playing(),
        "returned callback makes the refilled buffer available to the mixer"
    );
    let db = runner.dispatcher.sound_manager.channels[0]
        .double_buffer
        .as_ref()
        .expect("double-buffer active");
    assert_eq!(db.pending_callback_buffers, [false, false]);

    runner.mix_audio(1);
    assert_eq!(runner.audio_buffer, vec![0x80, 0xA0]);
}

#[test]
fn try_load_pending_double_buffers_recovers_ready_alternate_after_underrun() {
    let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
    let chan_ptr = 0x0039_38C8;
    let callback_addr = 0x0004_1234;
    let header_ptr = runner.bus.alloc(24);
    let buf0_ptr = runner.bus.alloc(18);
    let buf1_ptr = runner.bus.alloc(18);

    runner.bus.write_word(header_ptr, 1);
    runner.bus.write_word(header_ptr + 2, 8);
    runner.bus.write_long(header_ptr + 8, OUTPUT_RATE << 16);
    runner.bus.write_long(header_ptr + 12, buf0_ptr);
    runner.bus.write_long(header_ptr + 16, buf1_ptr);
    runner.bus.write_long(header_ptr + 20, callback_addr);
    write_double_buffer(&mut runner.bus, buf0_ptr, &[0xA0, 0xA1]);
    runner.bus.write_long(buf1_ptr, 2);
    runner.bus.write_long(buf1_ptr + 4, 0);

    let mut chan = SndChannel::new(chan_ptr, false);
    chan.double_buffer = Some(DoubleBufferState {
        header_ptr,
        current_buffer: 1,
        callback_addr,
        chan_ptr,
        sample_rate: OUTPUT_RATE << 16,
        num_channels: 1,
        sample_size: 8,
        last_buffer_seen: false,
        waiting_for_callback: false,
        pending_callback_buffers: [false; 2],
    });
    runner.dispatcher.sound_manager.add_channel(chan);

    runner.try_load_pending_double_buffers();

    let chan = &runner.dispatcher.sound_manager.channels[0];
    assert!(chan.is_playing(), "ready alternate buffer should load");
    let db = chan.double_buffer.as_ref().expect("double-buffer active");
    assert_eq!(db.current_buffer, 0);
    assert!(
        !db.waiting_for_callback,
        "loading a ready buffer completes the outstanding refill wait"
    );
    assert_eq!(
        runner.bus.read_long(buf0_ptr + 4) & 0x01,
        0x01,
        "loading a ready alternate must not clear dbBufferReady before playback exhausts"
    );
}

#[test]
fn try_load_pending_double_buffers_does_not_replay_callback_pending_slot() {
    let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
    let chan_ptr = 0x0039_38C8;
    let callback_addr = 0x0004_1234;
    let header_ptr = runner.bus.alloc(24);
    let buf0_ptr = runner.bus.alloc(17);
    let buf1_ptr = runner.bus.alloc(17);

    runner.bus.write_word(header_ptr, 1);
    runner.bus.write_word(header_ptr + 2, 8);
    runner.bus.write_long(header_ptr + 8, OUTPUT_RATE << 16);
    runner.bus.write_long(header_ptr + 12, buf0_ptr);
    runner.bus.write_long(header_ptr + 16, buf1_ptr);
    runner.bus.write_long(header_ptr + 20, callback_addr);
    write_double_buffer(&mut runner.bus, buf0_ptr, &[0xA0]);
    runner.bus.write_long(buf1_ptr, 1);
    runner.bus.write_long(buf1_ptr + 4, 0);

    let mut chan = SndChannel::new(chan_ptr, false);
    chan.double_buffer = Some(DoubleBufferState {
        header_ptr,
        current_buffer: 0,
        callback_addr,
        chan_ptr,
        sample_rate: OUTPUT_RATE << 16,
        num_channels: 1,
        sample_size: 8,
        last_buffer_seen: false,
        waiting_for_callback: true,
        pending_callback_buffers: [true, false],
    });
    runner.dispatcher.sound_manager.add_channel(chan);
    runner
        .dispatcher
        .sound_manager
        .queue_doubleback_callback(PendingDoubleBackCallback {
            callback_addr,
            chan_ptr,
            header_ptr,
            exhausted_buffer_index: 0,
        });

    runner.try_load_pending_double_buffers();

    assert!(
        !runner.dispatcher.sound_manager.channels[0].is_playing(),
        "an exhausted slot must not replay just because dbBufferReady remains set"
    );
    assert_eq!(
        runner.bus.read_long(buf0_ptr + 4) & 0x01,
        0x01,
        "the flag remains ready until fire_sound_doubleback_callbacks clears it"
    );
}

#[test]
fn sound_command_callback_trampoline_passes_sndcommand_pointer() {
    let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
    let interrupted_pc = 0x0001_0000;
    let interrupted_sp = 0x007F_FFC0;
    runner.m68k.cpu.write_reg(Register::PC, interrupted_pc);
    runner.m68k.cpu.write_reg(Register::A7, interrupted_sp);

    runner.dispatcher.sound_manager.queue_sound_callback(
        crate::sound::PendingSoundCallback::Command {
            architecture: CallbackTaskArchitecture::M68k,
            callback_addr: 0x0004_5678,
            chan_ptr: 0x0039_38C8,
            cmd: crate::sound::SndCommand {
                cmd: crate::sound::cmd::CALLBACK,
                param1: 0x1234,
                param2: 0x0001_43FC,
            },
        },
    );

    runner.fire_sound_callbacks();

    let active = runner
        .active_interrupt_callback
        .expect("sound callback should have been armed");
    assert!(matches!(
        active.source,
        ActiveInterruptCallbackSource::SoundCallback
    ));
    assert_eq!(active.resume_pc, interrupted_pc);
    assert_eq!(active.resume_sp, interrupted_sp);

    let tramp = runner.sound_callback_trampoline;
    let cmd_ptr = tramp + 34;
    let saved_regs_sp = interrupted_sp - 4 - 32;
    assert_eq!(runner.bus.read_long(tramp + 6), 0x0039_38C8);
    assert_eq!(runner.bus.read_long(tramp + 12), cmd_ptr);
    assert_eq!(runner.bus.read_long(tramp + 18), 0x0004_5678);
    assert_eq!(runner.bus.read_long(tramp + 24), saved_regs_sp);
    assert_eq!(runner.bus.read_word(cmd_ptr), crate::sound::cmd::CALLBACK);
    assert_eq!(runner.bus.read_word(cmd_ptr + 2), 0x1234);
    assert_eq!(runner.bus.read_long(cmd_ptr + 4), 0x0001_43FC);
    assert_eq!(
        runner.bus.get_alloc_size(tramp),
        None,
        "Systemless-owned command callback trampoline must stay outside the guest heap"
    );
}

#[test]
fn sound_command_callback_trampoline_does_not_perturb_guest_allocations() {
    let mut baseline = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
    let _baseline_callback = baseline.bus.alloc(2);
    let expected_next_guest_ptr = baseline.bus.alloc(64);

    let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
    let callback_addr = runner.bus.alloc(2);
    runner.m68k.cpu.write_reg(Register::PC, 0x0001_0000);
    runner.m68k.cpu.write_reg(Register::A7, 0x007F_FFC0);
    runner.dispatcher.sound_manager.queue_sound_callback(
        crate::sound::PendingSoundCallback::Command {
            architecture: CallbackTaskArchitecture::M68k,
            callback_addr,
            chan_ptr: 0x0039_38C8,
            cmd: crate::sound::SndCommand {
                cmd: crate::sound::cmd::CALLBACK,
                param1: 0,
                param2: 0,
            },
        },
    );

    runner.fire_sound_callbacks();
    let actual_next_guest_ptr = runner.bus.alloc(64);

    assert_eq!(
        actual_next_guest_ptr, expected_next_guest_ptr,
        "lazy callback setup must not consume application-visible heap space"
    );
}

#[test]
fn sound_command_callback_trampoline_tolerates_one_long_pascal_cleanup() {
    let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
    let interrupted_pc = 0x0001_0000;
    let interrupted_sp = 0x007F_FFC0;
    let callback_addr = runner.bus.alloc(4);

    // Some Pascal callback epilogues pop one long argument by copying the
    // return address over it, then RTS.
    runner.bus.write_word(callback_addr, 0x2E9F); // MOVE.L (SP)+,(SP)
    runner.bus.write_word(callback_addr + 2, 0x4E75); // RTS
    runner.bus.write_word(interrupted_pc, 0x4E71); // NOP
    runner.m68k.cpu.write_reg(Register::PC, interrupted_pc);
    runner.m68k.cpu.write_reg(Register::A7, interrupted_sp);

    runner.dispatcher.sound_manager.queue_sound_callback(
        crate::sound::PendingSoundCallback::Command {
            architecture: CallbackTaskArchitecture::M68k,
            callback_addr,
            chan_ptr: 0x0039_38C8,
            cmd: crate::sound::SndCommand {
                cmd: crate::sound::cmd::CALLBACK,
                param1: 0x1234,
                param2: 0x0001_43FC,
            },
        },
    );

    runner.fire_sound_callbacks();
    let (steps, running) = runner.run_steps(10, None);

    assert!(running, "callback trampoline should resume foreground code");
    assert_eq!(steps, 10);
    assert!(runner.active_interrupt_callback.is_none());
    assert_eq!(runner.m68k.cpu.read_reg(Register::PC), interrupted_pc + 2);
    assert_eq!(runner.m68k.cpu.read_reg(Register::A7), interrupted_sp);
    assert!(!runner.is_halted());
    assert_eq!(
        runner
            .bus
            .get_alloc_size(runner.sound_file_completion_trampoline),
        None,
        "Systemless-owned file completion trampoline must stay outside the guest heap"
    );
}

#[test]
fn sound_file_completion_callback_trampoline_tolerates_c_style_cleanup() {
    let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
    let interrupted_pc = 0x0001_0000;
    let interrupted_sp = 0x007F_FFC0;
    let callback_addr = runner.bus.alloc(2);

    runner.bus.write_word(callback_addr, 0x4E75); // RTS without popping chan.
    runner.bus.write_word(interrupted_pc, 0x4E71); // NOP
    runner.m68k.cpu.write_reg(Register::PC, interrupted_pc);
    runner.m68k.cpu.write_reg(Register::A7, interrupted_sp);

    runner.dispatcher.sound_manager.queue_sound_callback(
        crate::sound::PendingSoundCallback::FileCompletion {
            architecture: CallbackTaskArchitecture::M68k,
            callback_addr,
            chan_ptr: 0x0039_38C8,
        },
    );

    runner.fire_sound_callbacks();
    let (steps, running) = runner.run_steps(10, None);

    assert!(
        running,
        "file completion trampoline should resume foreground code"
    );
    assert_eq!(steps, 10);
    assert!(runner.active_interrupt_callback.is_none());
    assert_eq!(runner.m68k.cpu.read_reg(Register::A7), interrupted_sp);
    assert!(!runner.is_halted());
    assert_eq!(
        runner
            .bus
            .get_alloc_size(runner.sound_doubleback_trampoline),
        None,
        "Systemless-owned double-back trampoline must stay outside the guest heap"
    );
}

#[test]
fn sound_doubleback_callback_trampoline_tolerates_c_style_cleanup() {
    let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
    let interrupted_pc = 0x0001_0000;
    let interrupted_sp = 0x007F_FFC0;
    let callback_addr = runner.bus.alloc(2);
    let header_ptr = 0x0020_0000;
    let exhausted_buf_ptr = 0x0020_1000;

    runner.bus.write_word(callback_addr, 0x4E75); // RTS without popping args.
    runner.bus.write_word(interrupted_pc, 0x4E71); // NOP
    runner.m68k.cpu.write_reg(Register::PC, interrupted_pc);
    runner.m68k.cpu.write_reg(Register::A7, interrupted_sp);

    runner.bus.write_long(header_ptr + 12, exhausted_buf_ptr);
    runner.bus.write_long(exhausted_buf_ptr + 4, 0x0000_0001);
    runner
        .dispatcher
        .sound_manager
        .queue_doubleback_callback(PendingDoubleBackCallback {
            callback_addr,
            chan_ptr: 0x0039_38C8,
            header_ptr,
            exhausted_buffer_index: 0,
        });

    runner.fire_sound_doubleback_callbacks();
    let (steps, running) = runner.run_steps(12, None);

    assert!(
        running,
        "doubleback trampoline should resume foreground code"
    );
    assert_eq!(steps, 12);
    assert!(runner.active_interrupt_callback.is_none());
    assert_eq!(runner.m68k.cpu.read_reg(Register::A7), interrupted_sp);
    assert!(!runner.is_halted());
}

#[test]
fn run_pending_sound_work_does_not_advance_ticks_or_foreground_code() {
    let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
    let interrupted_pc = 0x0001_0000;
    let interrupted_sp = 0x007F_FFC0;
    let callback_addr = runner.bus.alloc(2);

    runner.bus.write_word(callback_addr, 0x4E75); // RTS without popping args.
    runner.bus.write_word(interrupted_pc, 0x4E71); // foreground NOP
    runner.m68k.cpu.write_reg(Register::PC, interrupted_pc);
    runner.m68k.cpu.write_reg(Register::A7, interrupted_sp);
    runner.bus.write_long(0x016A, 41);
    runner.set_guest_tick_for_test(41);
    runner.set_instructions_per_tick(1);
    runner.tick_budget = 0;

    runner
        .dispatcher
        .sound_manager
        .queue_sound_callback(PendingSoundCallback::Command {
            architecture: CallbackTaskArchitecture::M68k,
            callback_addr,
            chan_ptr: 0x0039_38C8,
            cmd: SndCommand {
                cmd: crate::sound::cmd::CALLBACK,
                param1: 0,
                param2: 0,
            },
        });

    let (steps, running) = runner.run_pending_sound_work(32);

    assert!(running);
    assert!(steps > 0, "sound callback trampoline should execute");
    assert_eq!(
        runner.guest_tick(),
        41,
        "callback-only slices must not advance application-visible ticks"
    );
    assert_eq!(
        runner.m68k.cpu.read_reg(Register::PC),
        interrupted_pc,
        "sound callback service must stop before resumed foreground code runs"
    );
    assert_eq!(runner.m68k.cpu.read_reg(Register::A7), interrupted_sp);
    assert!(runner.active_interrupt_callback.is_none());
    assert!(!runner.has_pending_sound_work());
}

#[test]
fn gui_cpu_slice_does_not_finalize_host_frame() {
    let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
    let callback_addr = runner.bus.alloc(2);

    runner.bus.write_word(callback_addr, 0x4E75); // RTS
    runner
        .dispatcher
        .sound_manager
        .queue_sound_callback(PendingSoundCallback::Command {
            architecture: CallbackTaskArchitecture::M68k,
            callback_addr,
            chan_ptr: 0x0039_38C8,
            cmd: SndCommand {
                cmd: crate::sound::cmd::CALLBACK,
                param1: 0,
                param2: 0,
            },
        });

    let (steps, running) = runner.run_gui_cpu_slice(0, 0);

    assert!(running);
    assert_eq!(steps, 0);
    assert!(
        runner.active_interrupt_callback.is_none(),
        "CPU-only GUI slices must not fire host-frame sound callbacks"
    );
    assert!(runner.has_pending_sound_work());
}

fn sound_chrome_runner() -> FixtureRunner {
    let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
    let screen_base = 0x0040_0000;
    runner.dispatcher.screen_mode = (screen_base, 256, 256, 64, 8);
    runner.bus.write_long(0x0824, screen_base);
    runner.bus.write_word(0x0BAA, 20);
    // Menu titles come from the guest MenuList, not just the host cache.
    // Install a real menu so this oracle actually paints outline glyphs.
    let title = runner.bus.alloc(5);
    runner.bus.write_bytes(title, b"\x04File");
    let sp = 0x007F_FF80;
    runner.m68k.cpu.write_reg(Register::A7, sp);
    runner.bus.write_long(sp, title);
    runner.bus.write_word(sp + 4, 128);
    runner
        .dispatcher
        .dispatch(0xA931, &mut runner.m68k.cpu, &mut runner.bus)
        .unwrap(); // NewMenu
    let menu = runner.bus.read_long(sp + 6);
    assert_ne!(menu, 0);
    runner.m68k.cpu.write_reg(Register::A7, sp);
    runner.bus.write_word(sp, 0);
    runner.bus.write_long(sp + 2, menu);
    runner
        .dispatcher
        .dispatch(0xA935, &mut runner.m68k.cpu, &mut runner.bus)
        .unwrap(); // InsertMenu
    runner.dispatcher.menu_bar_hidden = false;
    runner.prepare_text_presentation();
    runner.composite_frame();
    assert!(runner.bus.has_visible_outline_detail());
    let pixel = screen_base + 5 * 256 + 100;
    assert_ne!(runner.bus.read_byte(pixel), 0xAA);
    runner.bus.write_byte(pixel, 0xAA);
    runner.m68k.cpu.write_reg(Register::PC, 0x0001_0000);
    runner.m68k.cpu.write_reg(Register::A7, 0x007F_FFC0);
    runner.bus.write_word(0x0001_0000, 0x4E71); // foreground NOP
    runner.set_guest_tick_for_test(41);
    runner.set_instructions_per_tick(1);
    runner.tick_budget = 0;
    runner
}

#[test]
fn gui_sound_work_defers_chrome_but_matches_complete_slices() {
    for budget in [0, 1, 32] {
        let mut complete = sound_chrome_runner();
        let mut deferred = sound_chrome_runner();
        for runner in [&mut complete, &mut deferred] {
            let callback_addr = runner.bus.alloc(2);
            runner.bus.write_word(callback_addr, 0x4E75); // RTS
            for _ in 0..2 {
                runner.dispatcher.sound_manager.queue_sound_callback(
                    PendingSoundCallback::Command {
                        architecture: CallbackTaskArchitecture::M68k,
                        callback_addr,
                        chan_ptr: 0x0039_38C8,
                        cmd: SndCommand {
                            cmd: crate::sound::cmd::CALLBACK,
                            param1: 0,
                            param2: 0,
                        },
                    },
                );
            }
        }
        for _ in 0..64 {
            assert_eq!(
                complete.run_pending_sound_work(budget),
                deferred.run_gui_pending_sound_work(budget),
            );
            for reg in [Register::PC, Register::A7, Register::D0] {
                assert_eq!(
                    complete.m68k.cpu.read_reg(reg),
                    deferred.m68k.cpu.read_reg(reg)
                );
            }
            assert_eq!(deferred.guest_tick(), 41);
            assert_eq!(complete.guest_tick(), deferred.guest_tick());
            assert_eq!(
                complete.has_pending_sound_work(),
                deferred.has_pending_sound_work()
            );
            assert_eq!(
                complete
                    .dispatcher
                    .sound_manager
                    .pending_sound_callbacks
                    .len(),
                deferred
                    .dispatcher
                    .sound_manager
                    .pending_sound_callbacks
                    .len(),
            );
            let pixel = 0x0040_0000 + 5 * 256 + 100;
            assert_ne!(complete.bus.read_byte(pixel), 0xAA);
            assert_eq!(
                deferred.bus.read_byte(pixel),
                0xAA,
                "sound slices must not repaint chrome"
            );
            if budget == 0 || !deferred.has_pending_sound_work() {
                break;
            }
        }
        if budget > 0 {
            assert!(!deferred.has_pending_sound_work());
            assert_eq!(deferred.m68k.cpu.read_reg(Register::PC), 0x0001_0000);
            assert_eq!(deferred.m68k.cpu.read_reg(Register::A7), 0x007F_FFC0);
        }
        complete.composite_frame();
        deferred.composite_frame();
        assert!(
            deferred.bus.has_visible_outline_detail(),
            "fixture must exercise retained glyphs"
        );
        assert_eq!(
            complete.bus.save_pixel_bytes(0x0040_0000, 256 * 64),
            deferred.bus.save_pixel_bytes(0x0040_0000, 256 * 64),
            "logical pixels AND retained subpixel metadata must match",
        );
        let (cw, ch, complete_rgb, complete_draws) =
            complete.bus.outline_presentation_rgb().unwrap();
        let (dw, dh, deferred_rgb, deferred_draws) =
            deferred.bus.outline_presentation_rgb().unwrap();
        assert_eq!((cw, ch), (dw, dh));
        assert!(
            complete_rgb == deferred_rgb,
            "retained visible RGB must match"
        );
        // This cumulative counter is not visible state. Menu-bar caching
        // can eliminate glyph repainting in both paths; the overwritten
        // pixel assertions above still prove that only complete slices
        // restore chrome before the outer presentation pass.
        assert!(
            complete_draws >= deferred_draws,
            "deferred sound slices must not add glyph draws"
        );
        assert!(
            complete.bus.read_bytes(0, 8 * 1024 * 1024)
                == deferred.bus.read_bytes(0, 8 * 1024 * 1024)
        );
    }
}

#[test]
fn gui_sound_work_services_ready_double_buffers_even_with_zero_budget() {
    let mut runner = sound_chrome_runner();
    let chan_ptr = 0x0039_38C8;
    let header_ptr = runner.bus.alloc(24);
    let buf0_ptr = runner.bus.alloc(18);
    let buf1_ptr = runner.bus.alloc(18);
    runner.bus.write_word(header_ptr, 1);
    runner.bus.write_word(header_ptr + 2, 8);
    runner.bus.write_long(header_ptr + 8, OUTPUT_RATE << 16);
    runner.bus.write_long(header_ptr + 12, buf0_ptr);
    runner.bus.write_long(header_ptr + 16, buf1_ptr);
    write_double_buffer(&mut runner.bus, buf0_ptr, &[0xA0, 0xA1]);
    runner.bus.write_long(buf1_ptr, 2);
    runner.bus.write_long(buf1_ptr + 4, 0);
    let mut chan = SndChannel::new(chan_ptr, false);
    chan.double_buffer = Some(DoubleBufferState {
        header_ptr,
        current_buffer: 1,
        callback_addr: 0,
        chan_ptr,
        sample_rate: OUTPUT_RATE << 16,
        num_channels: 1,
        sample_size: 8,
        last_buffer_seen: false,
        waiting_for_callback: false,
        pending_callback_buffers: [false; 2],
    });
    runner.dispatcher.sound_manager.add_channel(chan);
    assert_eq!(runner.run_gui_pending_sound_work(0), (0, true));
    let chan = &runner.dispatcher.sound_manager.channels[0];
    assert!(
        chan.is_playing(),
        "audio-only finalization must load a ready refill"
    );
    assert_eq!(chan.double_buffer.as_ref().unwrap().current_buffer, 0);
    assert_eq!(
        runner.audio_buffer_len(),
        0,
        "servicing is not an extra mix"
    );
    assert_eq!(runner.bus.read_byte(0x0040_0000 + 5 * 256 + 100), 0xAA);
    runner.mix_audio(2);
    assert_eq!(runner.audio_buffer, vec![0xA0, 0xA1]);
}

#[test]
fn gui_sound_work_leaves_parked_chrome_validation_to_composition() {
    let mut runner = sound_chrome_runner();
    runner.park_proven_idle_cycle(0x0002_0000, 205);
    assert!(runner.idle_cycle_sleep.is_some());
    // Test the finalization policy separately from guest execution: a
    // zero-budget CPU slice can independently cancel an idle observation.
    runner.finish_host_frame(FrameFinalization::AudioOnly, 0, true);
    assert_eq!(runner.bus.read_byte(0x0040_0000 + 5 * 256 + 100), 0xAA);
    runner.composite_frame();
    assert_ne!(runner.bus.read_byte(0x0040_0000 + 5 * 256 + 100), 0xAA);
    assert!(
        runner.idle_cycle_sleep.is_none(),
        "changed repaint must still revoke the park"
    );
    assert!(runner.bus.suspend_write_probe().is_none());
}

#[test]
fn gui_sound_work_services_guest_written_queue_without_painting() {
    let mut runner = sound_chrome_runner();
    let chan_ptr = runner.bus.alloc(1088);
    runner
        .dispatcher
        .sound_manager
        .add_channel(SndChannel::new(chan_ptr, false));
    // Guest SndChannel: flags, qLength, qHead, qTail, then 8-byte commands.
    runner.bus.write_word(chan_ptr + 28, 0xFFFF);
    runner.bus.write_word(chan_ptr + 30, 128);
    runner.bus.write_word(chan_ptr + 32, 0);
    runner.bus.write_word(chan_ptr + 34, 1);
    runner
        .bus
        .write_word(chan_ptr + 36, crate::sound::cmd::VOLUME);
    runner.bus.write_word(chan_ptr + 38, 0);
    runner.bus.write_long(chan_ptr + 40, 0x0080_0040);
    assert_eq!(runner.run_gui_pending_sound_work(0), (0, true));
    assert_eq!(
        runner.bus.read_word(chan_ptr + 32),
        1,
        "guest queue must drain"
    );
    assert_eq!(
        runner.bus.read_word(chan_ptr + 28),
        0,
        "idle channel state must synchronize"
    );
    assert_eq!(
        runner.bus.read_word(chan_ptr + 20),
        0,
        "completed command must clear"
    );
    assert_eq!(runner.bus.read_byte(0x0040_0000 + 5 * 256 + 100), 0xAA);
}

#[test]
fn run_steps_paces_pending_sound_doublebacks_to_one_per_slice() {
    let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
    let interrupted_pc = 0x0001_0000;
    let interrupted_sp = 0x007F_FFC0;
    let callback_addr = runner.bus.alloc(2);
    let header_ptr = runner.bus.alloc(24);
    let buf0_ptr = runner.bus.alloc(16);
    let buf1_ptr = runner.bus.alloc(16);

    runner.bus.write_word(callback_addr, 0x4E75); // RTS without popping args.
    for offset in (0..512).step_by(2) {
        runner.bus.write_word(interrupted_pc + offset, 0x4E71); // NOP
    }
    runner.m68k.cpu.write_reg(Register::PC, interrupted_pc);
    runner.m68k.cpu.write_reg(Register::A7, interrupted_sp);

    runner.bus.write_long(header_ptr + 12, buf0_ptr);
    runner.bus.write_long(header_ptr + 16, buf1_ptr);
    runner.bus.write_long(buf0_ptr + 4, 0x0000_0001);
    runner.bus.write_long(buf1_ptr + 4, 0x0000_0001);
    runner
        .dispatcher
        .sound_manager
        .queue_doubleback_callback(PendingDoubleBackCallback {
            callback_addr,
            chan_ptr: 0x0039_38C8,
            header_ptr,
            exhausted_buffer_index: 0,
        });
    runner
        .dispatcher
        .sound_manager
        .queue_doubleback_callback(PendingDoubleBackCallback {
            callback_addr,
            chan_ptr: 0x0039_38C8,
            header_ptr,
            exhausted_buffer_index: 1,
        });

    let (_steps, running) = runner.run_steps(96, None);

    assert!(running);
    assert!(runner.active_interrupt_callback.is_none());
    assert_eq!(
        runner.dispatcher.sound_manager.pending_callbacks.len(),
        1,
        "one CPU slice must not drain back-to-back doubleback interrupts"
    );
    assert_eq!(
        runner.dispatcher.sound_manager.pending_callbacks[0].exhausted_buffer_index,
        1
    );

    let (_steps, running) = runner.run_steps(96, None);

    assert!(running);
    assert!(runner.active_interrupt_callback.is_none());
    assert!(
        runner.dispatcher.sound_manager.pending_callbacks.is_empty(),
        "the next CPU slice may dispatch the next pending doubleback"
    );
}

#[test]
fn classic_sound_callback_router_leaves_powerpc_completion_pending() {
    let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
    runner.dispatcher.sound_manager.queue_sound_callback(
        crate::sound::PendingSoundCallback::FileCompletion {
            architecture: CallbackTaskArchitecture::PowerPc,
            callback_addr: 0x0050_1000,
            chan_ptr: 0x0050_2000,
        },
    );

    assert!(!runner.fire_sound_callbacks());
    assert!(!runner.has_pending_sound_work());
    let (steps, running) = runner.run_pending_sound_work(8);
    assert_eq!(steps, 0);
    assert!(running);
    assert!(matches!(
        runner
            .dispatcher
            .sound_manager
            .pending_sound_callbacks
            .as_slice(),
        [crate::sound::PendingSoundCallback::FileCompletion {
            architecture: CallbackTaskArchitecture::PowerPc,
            callback_addr: 0x0050_1000,
            chan_ptr: 0x0050_2000,
        }]
    ));
}
