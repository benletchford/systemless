//! Simulated frontend time, without a window or wall-clock sleeps.
//!
//! An instruction-bounded run is useful for interpreter diagnostics, but a
//! retained Toolbox wait is not useful guest computation. Drive the existing
//! GUI scheduling path with one-tick deadlines instead of re-firing that wait
//! until millions of synthetic A-line instructions have retired.

use super::{
    configure_realtime_execution_rate, foreground_cpu_batch_instructions, game, save_screenshot,
    service_pending_sound_work_budgeted, DesktopSaveStore, FixtureRunner, HostMouseReleaseLatch,
    InputAction, ScriptedInput, UiThemeId, AUDIO_CALLBACK_CHUNK_SAMPLES,
};

fn deliver(runner: &mut FixtureRunner, mouse: &mut HostMouseReleaseLatch, action: InputAction) {
    match action {
        InputAction::MouseMove { v, h } => runner.set_mouse_position(v, h),
        InputAction::MouseDown { v, h } => {
            mouse.press();
            runner.push_mouse_down(v, h);
        }
        InputAction::MouseUp { v, h } => {
            if let Some((v, h)) = mouse.release((v, h)) {
                runner.push_mouse_up(v, h);
            }
        }
        InputAction::KeyDown { key, ch } => runner.push_key_down(key, ch),
        InputAction::KeyUp { key, ch } => runner.push_key_up(key, ch),
    }
}

#[derive(Debug, Default)]
pub(super) struct FrameWork {
    instructions: usize,
    foreground: usize,
    budget_exhausted: bool,
}

fn service_sound(runner: &mut FixtureRunner, total: &mut usize, reserve_used: &mut usize) {
    if let Some(steps) = service_pending_sound_work_budgeted(
        runner,
        game::MAX_INSTRUCTIONS_PER_FRAME,
        *total,
        reserve_used,
    ) {
        *total = total.saturating_add(steps);
    }
}

/// One virtual frontend tick. The frontend clock keeps moving while a menu
/// freezes guest TickCount, just as the real host clock and audio do. A short
/// retained-tracking slice is a yield boundary, not an invitation to re-fire
/// the same wait. Other short slices can be ordinary engine/task handoffs.
pub(super) fn frame(runner: &mut FixtureRunner, audio_samples: usize) -> FrameWork {
    runner.advance_menu_presentation_clock(super::FRAME_DURATION);
    let target = runner.guest_tick().saturating_add(1);
    let batch =
        foreground_cpu_batch_instructions(runner.is_powerpc_app(), runner.instructions_per_tick());
    let mut work = FrameWork::default();
    let mut audio_mixed = 0;
    let mut reserve_used = 0;
    while work.instructions < game::MAX_INSTRUCTIONS_PER_FRAME && !runner.is_halted() {
        let remaining = game::MAX_INSTRUCTIONS_PER_FRAME - work.instructions;
        let requested = batch.min(remaining);
        let batches_left = remaining.div_ceil(batch).max(1);
        let batch_audio = (audio_samples - audio_mixed).div_ceil(batches_left);
        // Match the desktop's deferred presentation: CPU batches do not
        // repaint chrome. Audio still runs between batches, including its
        // guest callbacks; the frame's presentation pass follows below.
        let (steps, running) = runner.run_gui_cpu_slice(requested, target);
        work.foreground += steps;
        work.instructions += steps;
        audio_mixed += batch_audio;
        if batch_audio > 0 {
            runner.mix_gui_audio_slice(batch_audio);
            service_sound(runner, &mut work.instructions, &mut reserve_used);
        }
        if !running
            || runner.guest_tick() >= target
            || steps == 0
            || (steps < requested && runner.is_ui_tracking_active())
        {
            break;
        }
    }
    work.budget_exhausted =
        work.instructions >= game::MAX_INSTRUCTIONS_PER_FRAME && runner.guest_tick() < target;

    // Audio is driven by frontend time even when guest ticks are frozen.
    // Mix in the same bounded chunks used by the GUI, giving double-buffer
    // callbacks an opportunity to refill between chunks. No host device is
    // attached, but guest-visible playback and callback work still executes.
    let mut remaining_audio = audio_samples - audio_mixed;
    if remaining_audio > 0 {
        service_sound(runner, &mut work.instructions, &mut reserve_used);
    }
    while remaining_audio > 0 && !runner.is_halted() {
        let chunk = remaining_audio.min(AUDIO_CALLBACK_CHUNK_SAMPLES);
        runner.mix_gui_audio_slice(chunk);
        remaining_audio -= chunk;
        service_sound(runner, &mut work.instructions, &mut reserve_used);
    }
    service_sound(runner, &mut work.instructions, &mut reserve_used);
    runner.prepare_text_presentation();
    runner.composite_frame();
    work
}

fn validate_script(script: &[ScriptedInput], ticks: u32) -> Result<(), String> {
    if let Some(event) = script.iter().find(|event| event.at >= ticks as usize) {
        return Err(format!(
            "input at tick {} would not execute: --max-ticks {ticks} runs ticks 0 through {}",
            event.at,
            ticks - 1
        ));
    }
    Ok(())
}

pub(super) fn run(
    path: &std::path::Path,
    ticks: u32,
    start_time: u32,
    addressing_24_bit: bool,
    screen_depth: Option<u16>,
    script: &[ScriptedInput],
    theme: UiThemeId,
) {
    if let Err(error) = validate_script(script, ticks) {
        eprintln!("[HEADLESS-TIME] {error}");
        std::process::exit(1);
    }
    let mut runner = match screen_depth {
        Some(depth) => game::new_runner_with_configuration(!addressing_24_bit, depth),
        None => game::new_runner_with_addressing(!addressing_24_bit),
    };
    runner.set_ui_theme(theme);
    runner.set_app_start_time(start_time);
    let app = game::load_game_from_path(&mut runner, path).expect("Failed to load game");
    let mut saves = DesktopSaveStore::for_loaded_archive(path, &mut runner);
    for file in saves.load_saved_files() {
        runner.import_vfs_file(&file);
    }
    game::init_game(&mut runner, &app);
    runner.prepare_text_presentation();
    let instructions_per_tick = configure_realtime_execution_rate(&mut runner);
    let start_tick = runner.guest_tick();
    eprintln!(
        "[HEADLESS-TIME] start frontend_ticks={ticks} guest_tick={start_tick} mac_seconds={start_time} input_clock=frontend_ticks instructions_per_tick={instructions_per_tick}"
    );

    let mut instructions = 0u64;
    let mut next_event = 0;
    let mut mouse = HostMouseReleaseLatch::default();
    let mut same_tick_frames = 0u32;
    let mut budget_exhausted_frames = 0u32;
    let mut audio_remainder = 0.0;
    let mut captured_audio = Vec::new();
    let mut audio_count = 0u64;
    let mut audio_hash = 0xcbf2_9ce4_8422_2325u64;
    let trace = std::env::var_os("SYSTEMLESS_HEADLESS_TIME_TRACE").is_some();
    for elapsed in 0..ticks {
        while let Some(event) = script.get(next_event).filter(|e| e.at <= elapsed as usize) {
            deliver(&mut runner, &mut mouse, event.action);
            eprintln!("[HEADLESS-TIME] input tick={} {:?}", event.at, event.action);
            next_event += 1;
        }
        // Fractional samples carry between frames; do not round away a small
        // amount of audio on every tick.
        let samples = systemless::sound::OUTPUT_RATE as f64 / systemless::runner::DEFAULT_VBL_HZ
            + audio_remainder;
        let whole_samples = samples.floor() as usize;
        audio_remainder = samples - whole_samples as f64;
        let before_tick = runner.guest_tick();
        let work = frame(&mut runner, whole_samples);
        if trace && elapsed.is_multiple_of(60) {
            use systemless::cpu::Register;
            eprintln!(
                "[HEADLESS-TIME-TRACE] frontend={elapsed} guest={} m68k_pc={:08X} sp={:08X} d0={:08X} tracking={} instructions={}",
                runner.guest_tick(),
                runner.cpu().read_reg(Register::PC),
                runner.cpu().read_reg(Register::A7),
                runner.cpu().read_reg(Register::D0),
                runner.is_ui_tracking_active(),
                work.instructions,
            );
        }
        instructions += work.instructions as u64;
        same_tick_frames += u32::from(runner.guest_tick() == before_tick);
        budget_exhausted_frames += u32::from(work.budget_exhausted);
        if work.foreground > 0 {
            mouse.observe_guest_progress();
        }
        if let Some((v, h)) = mouse.take_ready_release() {
            runner.push_mouse_up(v, h);
        }
        captured_audio.clear();
        runner.drain_audio_into(&mut captured_audio);
        audio_count += captured_audio.len() as u64;
        for &sample in &captured_audio {
            audio_hash = (audio_hash ^ u64::from(sample)).wrapping_mul(0x100_0000_01b3);
        }
        if runner.is_halted() {
            eprintln!(
                "[HEADLESS-TIME] incomplete frontend_ticks={} instructions={instructions}",
                elapsed + 1
            );
            save_screenshot(&runner, 9999);
            std::process::exit(1);
        }
    }
    eprintln!(
        "[HEADLESS-TIME] complete frontend_ticks={ticks} guest_tick={} instructions={instructions} input_events={next_event} same_tick_frames={same_tick_frames} budget_exhausted_frames={budget_exhausted_frames} captured_mono_samples={audio_count} audio_hash={audio_hash:016x}",
        runner.guest_tick()
    );
    if budget_exhausted_frames > 0 {
        eprintln!(
            "[HEADLESS-TIME] WARNING: {budget_exhausted_frames} frame(s) exhausted the work budget before reaching their guest tick target; check progress before comparing CPU totals."
        );
    }
    saves.sync_save_files_now(&mut runner);
    save_screenshot(&runner, 9999);
    runner.print_pc_histogram(24);
    runner.print_opcode_histogram(24);
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;
    use systemless::cpu::Register;
    use systemless::memory::MemoryBus;

    fn modal_runner() -> FixtureRunner {
        let mut runner = FixtureRunner::new(
            8 * 1024 * 1024,
            systemless::runner::FixtureRunnerConfig::default(),
        );
        runner.bus_mut().write_word(0x10000, 0xA991);
        runner.cpu_mut().write_reg(Register::PC, 0x10000);
        runner.cpu_mut().write_reg(Register::A7, 0x100000);
        let mut tracking = systemless::trap::dispatch::DialogTrackingState::default();
        tracking.dialog_ptr = 0x200000;
        tracking.bounds = (0, 0, 32, 32);
        tracking.proc_id = 1;
        tracking.draw_procs_done = true;
        tracking.rendered_pixels_final = true;
        runner.dispatcher_mut().dialog_tracking = Some(tracking);
        configure_realtime_execution_rate(&mut runner);
        runner
    }

    #[test]
    fn cli_keeps_instruction_and_tick_scripts_separate() {
        assert!(super::super::Cli::try_parse_from([
            "systemless",
            "--headless",
            "--max-ticks",
            "60",
            "--tick-input-script",
            "ticks.txt",
            "game.sit"
        ])
        .is_ok());
        for conflicting in ["--max-instructions", "--input-script"] {
            assert!(super::super::Cli::try_parse_from([
                "systemless",
                "--headless",
                "--max-ticks",
                "60",
                conflicting,
                "100",
                "game.sit"
            ])
            .is_err());
        }
        assert!(
            super::super::Cli::try_parse_from(["systemless", "--max-ticks", "60", "game.sit"])
                .is_err()
        );
        assert!(super::super::Cli::try_parse_from([
            "systemless",
            "--headless",
            "--max-ticks",
            "0",
            "game.sit"
        ])
        .is_err());
    }

    #[test]
    fn frontend_frame_keeps_running_guest_within_one_tick() {
        let mut runner = FixtureRunner::new(
            8 * 1024 * 1024,
            systemless::runner::FixtureRunnerConfig::default(),
        );
        let pc = runner.bus_mut().alloc(2);
        runner.bus_mut().write_word(pc, 0x60FE); // BRA.S *
        runner.cpu_mut().write_reg(Register::PC, pc);
        runner.cpu_mut().write_reg(Register::A7, 0x0008_0000);
        configure_realtime_execution_rate(&mut runner);
        let start = runner.guest_tick();
        let work = frame(&mut runner, 0);
        assert!(work.foreground > 0);
        assert!(!work.budget_exhausted);
        assert!(!runner.is_halted());
        assert_eq!(runner.guest_tick(), start + 1);
    }

    #[test]
    fn realtime_initialization_replaces_the_fixture_rate() {
        let mut runner = FixtureRunner::new(
            8 * 1024 * 1024,
            systemless::runner::FixtureRunnerConfig::default(),
        );
        let fixture_rate = runner.instructions_per_tick();
        let realtime_rate = configure_realtime_execution_rate(&mut runner);
        assert_eq!(realtime_rate, 415_628);
        assert_ne!(fixture_rate, realtime_rate);
        assert_eq!(runner.instructions_per_tick(), realtime_rate);
        let pc = 0x10000;
        runner.bus_mut().write_word(pc, 0x60FE); // BRA.S *
        runner.cpu_mut().write_reg(Register::PC, pc);
        runner.cpu_mut().write_reg(Register::A7, 0x100000);
        let work = frame(&mut runner, 0);
        assert!(work.foreground > fixture_rate as usize);
        assert!(!work.budget_exhausted);
    }

    #[test]
    fn foreground_batches_defer_menu_painting_until_frame_presentation() {
        let mut runner = FixtureRunner::new(
            8 * 1024 * 1024,
            systemless::runner::FixtureRunnerConfig::default(),
        );
        let pc = runner.bus_mut().alloc(8);
        let stack = 0x0008_0000;
        // Install a menu using guest traps, without presenting it yet.
        let title = runner.bus_mut().alloc(5);
        runner.bus_mut().write_bytes(title, b"\x04File");
        runner.cpu_mut().write_reg(Register::PC, pc);
        runner.cpu_mut().write_reg(Register::A7, stack);
        runner.bus_mut().write_word(pc, 0xA931); // NewMenu
        runner.bus_mut().write_long(stack, title);
        runner.bus_mut().write_word(stack + 4, 1);
        assert!(runner.run_gui_cpu_slice(1, u32::MAX).1);
        let menu = runner.bus().read_long(stack + 6);
        assert_ne!(menu, 0);
        runner.cpu_mut().write_reg(Register::PC, pc);
        runner.cpu_mut().write_reg(Register::A7, stack);
        runner.bus_mut().write_word(pc, 0xA935); // InsertMenu
        runner.bus_mut().write_word(stack, 0);
        runner.bus_mut().write_long(stack + 2, menu);
        assert!(runner.run_gui_cpu_slice(1, u32::MAX).1);

        let screen = runner.bus_mut().alloc(800 * 600);
        runner.dispatcher_mut().screen_mode = (screen, 800, 800, 600, 8);
        runner.bus_mut().write_word(0x0BAA, 20);
        runner.bus_mut().fill_bytes(screen, 800 * 20, 0xAA);
        // Every foreground iteration samples a byte the menu will paint.
        // Batch-level composition would change D0 before the frame ends.
        runner.bus_mut().write_word(pc, 0x1039); // MOVE.B abs.L,D0
        runner.bus_mut().write_long(pc + 2, screen + 400);
        runner.bus_mut().write_word(pc + 6, 0x60F8); // BRA.S back to MOVE
        runner.cpu_mut().write_reg(Register::PC, pc);
        runner.bus_mut().write_long(0x016A, 0);
        runner.set_instructions_per_tick((game::MAX_INSTRUCTIONS_PER_FRAME * 2) as u32);
        let work = frame(&mut runner, 366);
        assert!(work.foreground > super::super::CPU_BATCH_INSTRUCTIONS);
        assert!(!runner.is_halted());
        assert_eq!(runner.cpu().read_reg(Register::D0) & 0xFF, 0xAA);
        assert_ne!(
            runner.bus().read_byte(screen + 400),
            0xAA,
            "the final presentation must still draw the menu"
        );
    }

    #[test]
    fn retained_modal_wait_matches_desktop_tick_scheduling_without_a_window() {
        use super::super::{App, FRAME_DURATION};
        let mut timed = modal_runner();
        let start = timed.guest_tick();
        let mut app = App::new("dummy".into(), false, true, false, 8);
        app.runner = Some(modal_runner());
        let origin = std::time::Instant::now();
        app.start_time = Some(App::wall_clock_origin_for_guest_tick(origin, start));
        for elapsed in 0..10 {
            let now = origin + FRAME_DURATION * elapsed;
            app.next_frame_time = Some(now + FRAME_DURATION);
            app.step_frame_with_clock(|| now);
            let work = frame(&mut timed, 366);
            assert_eq!(work.foreground, 1, "one modal refire per frontend tick");
            assert!(!work.budget_exhausted);
            let gui = app.runner.as_ref().unwrap();
            assert_eq!(timed.guest_tick(), start + elapsed + 1);
            assert_eq!(timed.guest_tick(), gui.guest_tick());
            for register in [Register::PC, Register::A7] {
                assert_eq!(timed.cpu().read_reg(register), gui.cpu().read_reg(register));
            }
        }
        assert_eq!(app.total_instructions, 10);
    }

    #[test]
    fn input_at_or_after_the_endpoint_is_an_error() {
        let script = super::super::parse_input_script("60 click 100 200").unwrap();
        assert!(validate_script(&script, 60).is_err());
        assert!(validate_script(&script, 59).is_err());
        assert!(validate_script(&script, 61).is_ok());
    }

    #[test]
    fn same_tick_click_preserves_button_down_until_guest_progress() {
        let mut runner = FixtureRunner::new(
            8 * 1024 * 1024,
            systemless::runner::FixtureRunnerConfig::default(),
        );
        let mut mouse = HostMouseReleaseLatch::default();
        for event in super::super::parse_input_script("0 click 100 200").unwrap() {
            deliver(&mut runner, &mut mouse, event.action);
        }
        assert_eq!(runner.bus().read_byte(0x0172), 0);
        assert!(mouse.take_ready_release().is_none());
        mouse.observe_guest_progress();
        let (v, h) = mouse.take_ready_release().unwrap();
        runner.push_mouse_up(v, h);
        assert_eq!(runner.bus().read_byte(0x0172), 0x80);
    }
}
