use crate::catalogue::LaunchModifier;
use base64::{engine::general_purpose::STANDARD as BASE64, Engine as _};
use image::{codecs::png::PngEncoder, ImageEncoder};
use serde::{Deserialize, Serialize};
use systemless::runner::{FixtureRunner, MenuBarPolicy, DEFAULT_REALTIME_CPU_MHZ, DEFAULT_VBL_HZ};
use systemless::sound::OUTPUT_RATE;
use systemless::{display, game};
use wasm_bindgen::prelude::*;

const FIXED_MAC_TIME: u32 = 3_786_912_000;
const STANDARD_CHUNK: usize = 100_000;
const GUI_MAX_STEPS_PER_SLICE: usize = 2_000_000;
const GUI_MAX_TICKS_PER_SLICE: u32 = 2;
const GUI_AUDIO_SAMPLES_PER_SLICE: usize = OUTPUT_RATE as usize / 60;
const GUI_MIN_MOUSE_HOLD_TICKS: u32 = 15;
const DEFAULT_TIMEOUT_TICKS: u32 = 300;
const DEFAULT_TIMEOUT_INSTRUCTIONS: u64 = 500_000_000;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum RunMode {
    Unlocked,
    Realtime,
    Gui,
}

impl RunMode {
    fn parse(mode: &str) -> Result<Self, String> {
        match mode {
            "unlocked" => Ok(Self::Unlocked),
            "realtime" => Ok(Self::Realtime),
            "gui" | "gui-realtime" => Ok(Self::Gui),
            _ => Err(format!("unknown mode '{mode}'")),
        }
    }

    fn uses_realtime_ticks(self) -> bool {
        matches!(self, Self::Realtime | Self::Gui)
    }

    fn enforces_pixel_tick_deadline(self) -> bool {
        !matches!(self, Self::Unlocked)
    }

    fn pixel_poll_deadline_tick(self, tick_deadline: u32) -> Option<u32> {
        self.enforces_pixel_tick_deadline().then_some(tick_deadline)
    }

    fn run_slice(
        self,
        runner: &mut FixtureRunner,
        max_steps: usize,
        deadline_tick: Option<u32>,
    ) -> (usize, bool) {
        match self {
            Self::Gui => {
                let frame_deadline = runner.guest_tick().saturating_add(GUI_MAX_TICKS_PER_SLICE);
                let deadline = deadline_tick
                    .map(|target| target.min(frame_deadline))
                    .unwrap_or(frame_deadline);
                let steps_budget = if deadline_tick.is_some() {
                    max_steps.max(GUI_MAX_STEPS_PER_SLICE)
                } else {
                    max_steps.min(GUI_MAX_STEPS_PER_SLICE)
                };
                runner.run_gui_slice_with_audio(steps_budget, deadline, GUI_AUDIO_SAMPLES_PER_SLICE)
            }
            Self::Unlocked | Self::Realtime => runner.run_steps(max_steps, deadline_tick),
        }
    }
}

#[wasm_bindgen]
pub async fn systemless_bench_run(
    script_json: String,
    game_url: String,
    mode: String,
) -> Result<String, JsValue> {
    run(script_json, game_url, mode)
        .await
        .map_err(|error| JsValue::from_str(&error))
}

async fn run(script_json: String, game_url: String, mode: String) -> Result<String, String> {
    let script: PlayScript =
        serde_json::from_str(&script_json).map_err(|e| format!("parse script: {e}"))?;
    let game_bytes = fetch_bytes(&game_url).await?;
    let run_mode = RunMode::parse(&mode)?;

    let started_at = performance_now();
    let mut runner = game::new_runner();
    runner.set_menu_bar_policy(MenuBarPolicy::InitialKiosk);
    runner.set_app_start_time(FIXED_MAC_TIME);
    if run_mode.uses_realtime_ticks() {
        let ipt = (DEFAULT_REALTIME_CPU_MHZ * 1_000_000.0 / DEFAULT_VBL_HZ).round() as u32;
        runner.set_instructions_per_tick(ipt);
        runner.set_wait_sleep_cap_in_headless(Some(0));
    }

    let app = game::load_game(&mut runner, &game_bytes)?;
    // Match the live player: modifier-only launch conditions must be present
    // when init_app publishes the current key state to KeyMap low memory.
    for modifier in &script.launch_modifiers {
        runner.push_key_down(modifier.mac_key_code(), 0);
    }
    game::init_game(&mut runner, &app);
    let mut state = BenchState::default();
    let mut error = None;
    if let Err(e) = execute(&mut runner, &script, run_mode, &mut state) {
        capture_failure_checkpoint(&mut runner, &mut state);
        error = Some(e);
    }

    let wall_time_secs = (performance_now() - started_at) / 1000.0;
    let total_guest_instructions = runner.total_instructions();
    let final_guest_tick = runner.guest_tick();
    let passed = error.is_none() && state.assertion_failures == 0 && !state.checkpoints.is_empty();

    let report = BrowserBenchReport {
        target: "browser_wasm",
        mode,
        wall_time_secs,
        final_guest_tick,
        total_guest_instructions,
        effective_mips: if wall_time_secs > 0.0 {
            total_guest_instructions as f64 / wall_time_secs / 1_000_000.0
        } else {
            0.0
        },
        ticks_per_sec: if wall_time_secs > 0.0 {
            final_guest_tick as f64 / wall_time_secs
        } else {
            0.0
        },
        trap_count: runner.dispatcher().trap_count,
        game_trap_count: runner.dispatcher().game_trap_count,
        assertions: state.assertions,
        assertion_failures: state.assertion_failures,
        audio_checks: state.audio_checks,
        checkpoints: state.checkpoints,
        milestones: state.milestones,
        passed,
        error,
    };

    serde_json::to_string_pretty(&report).map_err(|e| format!("serialize report: {e}"))
}

fn execute(
    runner: &mut FixtureRunner,
    script: &PlayScript,
    run_mode: RunMode,
    state: &mut BenchState,
) -> Result<(), String> {
    for action in &script.actions {
        match action {
            Action::Run {
                instructions,
                ticks,
            } => match (instructions, ticks) {
                (Some(n), None) => {
                    let _ = run_instructions(runner, *n, run_mode);
                }
                (None, Some(t)) => {
                    let target_tick = runner.guest_tick().wrapping_add(*t);
                    let _ = run_until_tick(runner, target_tick, run_mode);
                }
                _ => return Err("run requires exactly one of instructions or ticks".to_string()),
            },
            Action::RunUntilTick { tick } => {
                let _ = run_until_tick(runner, *tick, run_mode);
            }
            Action::RunUntilPixel {
                x,
                y,
                rgb,
                not,
                tolerance,
                poll_chunk,
                timeout_ticks,
                timeout_instructions,
                label,
            } => {
                run_until_pixel(
                    runner,
                    *x,
                    *y,
                    *rgb,
                    *not,
                    *tolerance,
                    *poll_chunk,
                    *timeout_ticks,
                    *timeout_instructions,
                    label.as_deref(),
                    run_mode,
                )?;
            }
            Action::MouseMove { v, h, at_tick } => {
                enforce_at_tick(runner, *at_tick, run_mode)?;
                runner.set_mouse_position(*v, *h);
            }
            Action::MouseDown { v, h, at_tick } => {
                enforce_at_tick(runner, *at_tick, run_mode)?;
                runner.set_mouse_position(*v, *h);
                runner.push_mouse_down(*v, *h);
                if run_mode == RunMode::Gui {
                    state.pending_mouse_down_tick = Some(runner.guest_tick());
                }
            }
            Action::MouseUp { v, h, at_tick } => {
                enforce_at_tick(runner, *at_tick, run_mode)?;
                if run_mode == RunMode::Gui {
                    hold_gui_mouse_down_before_release(runner, run_mode, state);
                }
                runner.push_mouse_up(*v, *h);
                state.pending_mouse_down_tick = None;
            }
            Action::KeyDown { key, at_tick } => {
                enforce_at_tick(runner, *at_tick, run_mode)?;
                let (mac_key, char_code) = resolve_key(key)?;
                runner.push_key_down(mac_key, char_code);
            }
            Action::KeyUp { key, at_tick } => {
                enforce_at_tick(runner, *at_tick, run_mode)?;
                let (mac_key, char_code) = resolve_key(key)?;
                runner.push_key_up(mac_key, char_code);
            }
            Action::Screenshot { path, at_tick } => {
                enforce_at_tick(runner, *at_tick, run_mode)?;
                let name = path
                    .clone()
                    .unwrap_or_else(|| format!("{:04}.png", runner.guest_tick()));
                let png_base64 = capture_png_base64(runner)?;
                state.checkpoints.push(CheckpointReport {
                    name,
                    tick: runner.guest_tick(),
                    trap_count: runner.dispatcher().trap_count,
                    game_trap_count: runner.dispatcher().game_trap_count,
                    png_base64,
                });
            }
            Action::Log { .. } => {}
            Action::Milestone { name } => {
                state.milestones.push(MilestoneReport {
                    name: name.clone(),
                    tick: runner.guest_tick(),
                    trap_count: runner.dispatcher().trap_count,
                    game_trap_count: runner.dispatcher().game_trap_count,
                });
            }
            Action::AssertTickRange { min, max, .. } => {
                state.assertions += 1;
                let tick = runner.guest_tick();
                if tick < *min || tick > *max {
                    state.assertion_failures += 1;
                }
            }
            Action::AssertPixel { x, y, rgb, not, .. } => {
                state.assertions += 1;
                let actual = current_pixel(runner, *x, *y)?;
                if !pixel_matches_within(actual, *rgb, *not, 0) {
                    state.assertion_failures += 1;
                }
            }
            Action::AssertAudio {
                samples,
                min_non_silent,
                label,
            } => {
                state.assertions += 1;
                let report = assert_audio(runner, *samples, *min_non_silent, label.clone());
                if !report.passed {
                    state.assertion_failures += 1;
                }
                state.audio_checks.push(report);
            }
        }
    }
    Ok(())
}

fn run_instructions(runner: &mut FixtureRunner, count: usize, run_mode: RunMode) -> (usize, bool) {
    let mut remaining = count;
    let mut total = 0;
    while remaining > 0 && !runner.is_halted() {
        let batch = remaining.min(STANDARD_CHUNK);
        let (steps, running) = run_mode.run_slice(runner, batch, None);
        let effective = if steps == 0 && running { 1 } else { steps };
        total += effective;
        remaining -= effective.min(remaining);
        if !running {
            break;
        }
    }
    (total, !runner.is_halted())
}

fn run_until_tick(runner: &mut FixtureRunner, target_tick: u32, run_mode: RunMode) -> u64 {
    let current_tick = runner.guest_tick();
    let delta = target_tick.saturating_sub(current_tick) as u64;
    let ipt = runner.instructions_per_tick() as u64;
    let max_instructions = (delta * ipt * 2).max(50_000_000);
    let mut total = 0u64;

    while runner.guest_tick() < target_tick && !runner.is_halted() && total < max_instructions {
        let (steps, running) = run_mode.run_slice(runner, STANDARD_CHUNK, Some(target_tick));
        total += steps as u64;
        if !running {
            break;
        }
    }
    total
}

fn run_until_pixel(
    runner: &mut FixtureRunner,
    x: u32,
    y: u32,
    rgb: [u8; 3],
    not: bool,
    tolerance: u8,
    poll_chunk: Option<usize>,
    timeout_ticks: Option<u32>,
    timeout_instructions: Option<u64>,
    label: Option<&str>,
    run_mode: RunMode,
) -> Result<u64, String> {
    let chunk = poll_chunk.unwrap_or(STANDARD_CHUNK).max(1);
    let start_tick = runner.guest_tick();
    let tick_deadline = start_tick.wrapping_add(timeout_ticks.unwrap_or(DEFAULT_TIMEOUT_TICKS));
    let instruction_deadline = timeout_instructions.unwrap_or(DEFAULT_TIMEOUT_INSTRUCTIONS);
    let mut total = 0u64;

    loop {
        let actual = current_pixel(runner, x, y)?;
        if pixel_matches_within(actual, rgb, not, tolerance) {
            return Ok(total);
        }

        let current_tick = runner.guest_tick();
        if total >= instruction_deadline {
            return Err(format!(
                "run_until_pixel timed out after {total} instructions at tick {current_tick}: pixel ({x},{y}) = [{},{},{}], expected {} [{},{},{}]{}{}",
                actual[0],
                actual[1],
                actual[2],
                if not { "!=" } else { "==" },
                rgb[0],
                rgb[1],
                rgb[2],
                format_label(label),
                format_event_debug(runner)
            ));
        }
        if run_mode.enforces_pixel_tick_deadline() && current_tick >= tick_deadline {
            return Err(format!(
                "run_until_pixel timed out at tick {current_tick} after {total} instructions: pixel ({x},{y}) = [{},{},{}], expected {} [{},{},{}]{}{}",
                actual[0],
                actual[1],
                actual[2],
                if not { "!=" } else { "==" },
                rgb[0],
                rgb[1],
                rgb[2],
                format_label(label),
                format_event_debug(runner)
            ));
        }

        let (steps, running) = run_mode.run_slice(
            runner,
            chunk,
            run_mode.pixel_poll_deadline_tick(tick_deadline),
        );
        total += steps as u64;
        if !running {
            return Err(format!(
                "run_until_pixel halted at tick {} after {} instructions before pixel matched",
                runner.guest_tick(),
                total
            ));
        }
    }
}

fn format_label(label: Option<&str>) -> String {
    label
        .filter(|label| !label.is_empty())
        .map(|label| format!(" label=\"{label}\""))
        .unwrap_or_default()
}

fn format_event_debug(runner: &FixtureRunner) -> String {
    let dispatcher = runner.dispatcher();
    format!(
        " events={{wne:{}, gne:{}, key:{}, mouse_moved:{}, get_mouse:{}, still_down:{}/{}, button:{}/{}, wait_mouse_up:{}/{}}}",
        dispatcher.debug_wait_next_event_count,
        dispatcher.debug_get_next_event_count,
        dispatcher.debug_key_event_delivery_count,
        dispatcher.debug_mouse_moved_event_count,
        dispatcher.debug_get_mouse_count,
        dispatcher.debug_still_down_true_count,
        dispatcher.debug_still_down_false_count,
        dispatcher.debug_button_true_count,
        dispatcher.debug_button_false_count,
        dispatcher.debug_wait_mouse_up_true_count,
        dispatcher.debug_wait_mouse_up_false_count,
    )
}

fn enforce_at_tick(
    runner: &mut FixtureRunner,
    at_tick: Option<u32>,
    run_mode: RunMode,
) -> Result<(), String> {
    let Some(target) = at_tick else {
        return Ok(());
    };
    let current = runner.guest_tick();
    if current > target {
        return Err(format!(
            "at_tick={target} but runner is already at tick {current}"
        ));
    }
    if current < target {
        let _ = run_until_tick(runner, target, run_mode);
    }
    Ok(())
}

fn hold_gui_mouse_down_before_release(
    runner: &mut FixtureRunner,
    run_mode: RunMode,
    state: &BenchState,
) {
    let Some(target) = gui_mouse_release_tick(state.pending_mouse_down_tick) else {
        return;
    };
    if runner.guest_tick() < target {
        run_until_tick(runner, target, run_mode);
    }
}

fn gui_mouse_release_tick(mouse_down_tick: Option<u32>) -> Option<u32> {
    mouse_down_tick.map(|tick| tick.saturating_add(GUI_MIN_MOUSE_HOLD_TICKS))
}

fn current_pixel(runner: &mut FixtureRunner, x: u32, y: u32) -> Result<[u8; 3], String> {
    let (_, _, scrn_width, scrn_height, _) = runner.dispatcher().screen_mode;
    let w = scrn_width as u32;
    let h = scrn_height as u32;
    if x >= w || y >= h {
        return Err(format!("pixel ({x},{y}) out of bounds ({w}x{h})"));
    }

    runner.composite_frame();
    let screen_clut = *runner.dispatcher().device_clut;
    display::screen_pixel_rgb(
        runner.bus(),
        runner.dispatcher().screen_mode,
        &screen_clut,
        x,
        y,
    )
    .ok_or_else(|| format!("unsupported screen mode for pixel ({x},{y})"))
}

fn capture_png_base64(runner: &mut FixtureRunner) -> Result<String, String> {
    runner.composite_frame();
    let (_, _, width, height, _) = runner.dispatcher().screen_mode;
    let screen_clut = *runner.dispatcher().device_clut;
    let rgba = display::render_screen(runner.bus(), runner.dispatcher().screen_mode, &screen_clut);
    let mut png = Vec::new();
    PngEncoder::new(&mut png)
        .write_image(
            &rgba,
            width as u32,
            height as u32,
            image::ColorType::Rgba8.into(),
        )
        .map_err(|e| format!("encode png: {e}"))?;
    Ok(BASE64.encode(png))
}

fn capture_failure_checkpoint(runner: &mut FixtureRunner, state: &mut BenchState) {
    if let Ok(png_base64) = capture_png_base64(runner) {
        state.checkpoints.push(CheckpointReport {
            name: "failure.png".to_string(),
            tick: runner.guest_tick(),
            trap_count: runner.dispatcher().trap_count,
            game_trap_count: runner.dispatcher().game_trap_count,
            png_base64,
        });
    }
}

fn pixel_matches_within(actual: [u8; 3], expected: [u8; 3], not: bool, tolerance: u8) -> bool {
    let max_diff = actual
        .iter()
        .zip(expected.iter())
        .map(|(a, e)| a.abs_diff(*e))
        .max()
        .unwrap_or(0);
    if not {
        max_diff > tolerance
    } else {
        max_diff <= tolerance
    }
}

fn assert_audio(
    runner: &mut FixtureRunner,
    samples: usize,
    min_non_silent: usize,
    label: Option<String>,
) -> AudioCheckReport {
    runner.mix_audio(samples);
    let audio = runner.drain_audio();
    let non_silent_samples = audio.iter().filter(|&&sample| sample != 0x80).count();
    let peak_delta_from_silence = audio
        .iter()
        .map(|&sample| sample.abs_diff(0x80))
        .max()
        .unwrap_or(0);
    AudioCheckReport {
        label,
        requested_samples: samples,
        mixed_samples: audio.len(),
        non_silent_samples,
        min_non_silent,
        peak_delta_from_silence,
        tick: runner.guest_tick(),
        trap_count: runner.dispatcher().trap_count,
        game_trap_count: runner.dispatcher().game_trap_count,
        passed: audio.len() >= samples && non_silent_samples >= min_non_silent,
    }
}

async fn fetch_bytes(url: &str) -> Result<Vec<u8>, String> {
    let resp = gloo_net::http::Request::get(url)
        .send()
        .await
        .map_err(|e| e.to_string())?;
    if !resp.ok() {
        return Err(format!("HTTP {}", resp.status()));
    }
    resp.binary().await.map_err(|e| e.to_string())
}

fn performance_now() -> f64 {
    web_sys::window()
        .and_then(|w| w.performance())
        .map(|p| p.now())
        .unwrap_or_else(js_sys::Date::now)
}

fn resolve_key(key: &str) -> Result<(u8, u8), String> {
    match key.to_ascii_lowercase().as_str() {
        "return" | "enter" => return Ok((0x24, 13)),
        "tab" => return Ok((0x30, 9)),
        "space" => return Ok((0x31, 32)),
        "delete" | "backspace" => return Ok((0x33, 8)),
        "escape" | "esc" => return Ok((0x35, 27)),
        "left" => return Ok((0x7B, 28)),
        "right" => return Ok((0x7C, 29)),
        "down" => return Ok((0x7D, 31)),
        "up" => return Ok((0x7E, 30)),
        "cmd" | "command" | "meta" => return Ok((0x37, 0)),
        "shift" => return Ok((0x38, 0)),
        "shift_right" | "rshift" => return Ok((0x3C, 0)),
        "option" | "opt" | "alt" => return Ok((0x3A, 0)),
        "option_right" | "ropt" | "ralt" => return Ok((0x3D, 0)),
        "control" | "ctrl" => return Ok((0x3B, 0)),
        "control_right" | "rctrl" => return Ok((0x3E, 0)),
        "caps_lock" | "capslock" => return Ok((0x39, 0)),
        "numpad_decimal" | "kp_decimal" => return Ok((0x41, 0)),
        "numpad_multiply" | "kp_multiply" => return Ok((0x43, 0)),
        "numpad_add" | "kp_add" => return Ok((0x45, 0)),
        "numpad_divide" | "kp_divide" => return Ok((0x4B, 0)),
        "numpad_enter" | "kp_enter" => return Ok((0x4C, 13)),
        "numpad_subtract" | "kp_subtract" => return Ok((0x4E, 0)),
        "numpad_equal" | "kp_equal" => return Ok((0x51, 0)),
        "numpad0" | "kp0" => return Ok((0x52, b'0')),
        "numpad1" | "kp1" => return Ok((0x53, b'1')),
        "numpad2" | "kp2" => return Ok((0x54, b'2')),
        "numpad3" | "kp3" => return Ok((0x55, b'3')),
        "numpad4" | "kp4" => return Ok((0x56, b'4')),
        "numpad5" | "kp5" => return Ok((0x57, b'5')),
        "numpad6" | "kp6" => return Ok((0x58, b'6')),
        "numpad7" | "kp7" => return Ok((0x59, b'7')),
        "numpad8" | "kp8" => return Ok((0x5B, b'8')),
        "numpad9" | "kp9" => return Ok((0x5C, b'9')),
        _ => {}
    }

    if key.len() == 1 {
        let ch = key.as_bytes()[0];
        let lower = ch.to_ascii_lowercase();
        let vk = match lower {
            b'a' => 0x00,
            b's' => 0x01,
            b'd' => 0x02,
            b'f' => 0x03,
            b'h' => 0x04,
            b'g' => 0x05,
            b'z' => 0x06,
            b'x' => 0x07,
            b'c' => 0x08,
            b'v' => 0x09,
            b'b' => 0x0B,
            b'q' => 0x0C,
            b'w' => 0x0D,
            b'e' => 0x0E,
            b'r' => 0x0F,
            b'y' => 0x10,
            b't' => 0x11,
            b'o' => 0x1F,
            b'u' => 0x20,
            b'i' => 0x22,
            b'p' => 0x23,
            b'l' => 0x25,
            b'j' => 0x26,
            b'k' => 0x28,
            b'n' => 0x2D,
            b'm' => 0x2E,
            b'1' => 0x12,
            b'2' => 0x13,
            b'3' => 0x14,
            b'4' => 0x15,
            b'5' => 0x17,
            b'6' => 0x16,
            b'7' => 0x1A,
            b'8' => 0x1C,
            b'9' => 0x19,
            b'0' => 0x1D,
            b'.' => 0x2F,
            b',' => 0x2B,
            b'-' => 0x1B,
            b'[' => 0x21,
            b']' => 0x1E,
            b'\\' => 0x2A,
            _ => return Err(format!("unknown key '{key}'")),
        };
        Ok((vk, ch))
    } else {
        Err(format!("unknown key '{key}'"))
    }
}

#[derive(Default)]
struct BenchState {
    checkpoints: Vec<CheckpointReport>,
    milestones: Vec<MilestoneReport>,
    audio_checks: Vec<AudioCheckReport>,
    assertions: usize,
    assertion_failures: usize,
    pending_mouse_down_tick: Option<u32>,
}

#[derive(Deserialize)]
struct PlayScript {
    #[serde(default)]
    launch_modifiers: Vec<LaunchModifier>,
    actions: Vec<Action>,
}

#[derive(Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum Action {
    Run {
        instructions: Option<usize>,
        ticks: Option<u32>,
    },
    RunUntilTick {
        tick: u32,
    },
    RunUntilPixel {
        x: u32,
        y: u32,
        rgb: [u8; 3],
        #[serde(default)]
        not: bool,
        #[serde(default)]
        tolerance: u8,
        poll_chunk: Option<usize>,
        timeout_ticks: Option<u32>,
        timeout_instructions: Option<u64>,
        #[allow(dead_code)]
        label: Option<String>,
    },
    MouseMove {
        v: i16,
        h: i16,
        #[serde(default)]
        at_tick: Option<u32>,
    },
    MouseDown {
        v: i16,
        h: i16,
        #[serde(default)]
        at_tick: Option<u32>,
    },
    MouseUp {
        v: i16,
        h: i16,
        #[serde(default)]
        at_tick: Option<u32>,
    },
    KeyDown {
        key: String,
        #[serde(default)]
        at_tick: Option<u32>,
    },
    KeyUp {
        key: String,
        #[serde(default)]
        at_tick: Option<u32>,
    },
    Screenshot {
        #[serde(default)]
        path: Option<String>,
        #[serde(default)]
        at_tick: Option<u32>,
    },
    Log {
        #[allow(dead_code)]
        message: String,
    },
    Milestone {
        name: String,
    },
    AssertTickRange {
        min: u32,
        max: u32,
        #[allow(dead_code)]
        label: Option<String>,
    },
    AssertPixel {
        x: u32,
        y: u32,
        rgb: [u8; 3],
        #[serde(default)]
        not: bool,
        #[allow(dead_code)]
        label: Option<String>,
    },
    AssertAudio {
        samples: usize,
        min_non_silent: usize,
        #[serde(default)]
        label: Option<String>,
    },
}

#[derive(Serialize)]
struct BrowserBenchReport {
    target: &'static str,
    mode: String,
    wall_time_secs: f64,
    final_guest_tick: u32,
    total_guest_instructions: u64,
    effective_mips: f64,
    ticks_per_sec: f64,
    trap_count: u64,
    game_trap_count: u64,
    assertions: usize,
    assertion_failures: usize,
    audio_checks: Vec<AudioCheckReport>,
    checkpoints: Vec<CheckpointReport>,
    milestones: Vec<MilestoneReport>,
    passed: bool,
    error: Option<String>,
}

#[derive(Serialize)]
struct CheckpointReport {
    name: String,
    tick: u32,
    trap_count: u64,
    game_trap_count: u64,
    png_base64: String,
}

#[derive(Serialize)]
struct MilestoneReport {
    name: String,
    tick: u32,
    trap_count: u64,
    game_trap_count: u64,
}

#[derive(Serialize)]
struct AudioCheckReport {
    label: Option<String>,
    requested_samples: usize,
    mixed_samples: usize,
    non_silent_samples: usize,
    min_non_silent: usize,
    peak_delta_from_silence: u8,
    tick: u32,
    trap_count: u64,
    game_trap_count: u64,
    passed: bool,
}

#[cfg(test)]
mod tests {
    use super::*;
    use systemless::cpu::Register;
    use systemless::memory::MemoryBus;

    fn pixel_wait_test_runner() -> FixtureRunner {
        let mut runner = game::new_runner();
        let program_start = 0x0001_0000;
        runner.bus_mut().write_word(program_start, 0x60FE); // BRA.S to self
        runner.cpu_mut().write_reg(Register::PC, program_start);
        runner.cpu_mut().write_reg(Register::A7, 0x007F_FFC0);
        runner.bus_mut().write_long(0x016A, 0);
        runner.set_instructions_per_tick(1);
        runner
    }

    #[test]
    fn gui_mouse_release_tick_holds_short_click_for_guest_polling() {
        assert_eq!(gui_mouse_release_tick(None), None);
        assert_eq!(gui_mouse_release_tick(Some(190)), Some(205));
        assert_eq!(gui_mouse_release_tick(Some(u32::MAX - 1)), Some(u32::MAX));
    }

    #[test]
    fn launch_modifiers_are_validated_when_parsing_bench_scripts() {
        let script: PlayScript =
            serde_json::from_str(r#"{"launch_modifiers":["option"],"actions":[]}"#).unwrap();
        assert_eq!(script.launch_modifiers, [LaunchModifier::Option]);

        let error = serde_json::from_str::<PlayScript>(
            r#"{"launch_modifiers":["caps_lock"],"actions":[]}"#,
        )
        .err()
        .expect("non-modifier launch keys should be rejected");
        assert!(error.to_string().contains("unknown variant"));
    }

    #[test]
    fn unlocked_pixel_waits_are_instruction_budgeted() {
        let mut runner = pixel_wait_test_runner();

        let error = run_until_pixel(
            &mut runner,
            0,
            0,
            [1, 2, 3],
            false,
            0,
            Some(1),
            Some(2),
            Some(5),
            Some("unlocked missing pixel"),
            RunMode::Unlocked,
        )
        .unwrap_err();

        assert!(
            error.contains("timed out after 5 instructions"),
            "unlocked polling should fail by instruction budget, got: {error}"
        );
        assert!(
            runner.guest_tick() >= 5,
            "unlocked polling should be allowed to pass the script tick deadline"
        );
    }

    #[test]
    fn realtime_pixel_waits_keep_tick_deadline() {
        let mut runner = pixel_wait_test_runner();

        let error = run_until_pixel(
            &mut runner,
            0,
            0,
            [1, 2, 3],
            false,
            0,
            Some(1),
            Some(2),
            Some(5),
            Some("realtime missing pixel"),
            RunMode::Realtime,
        )
        .unwrap_err();

        assert!(
            error.contains("timed out at tick 2"),
            "realtime polling should fail by tick deadline, got: {error}"
        );
        assert_eq!(runner.guest_tick(), 2);
    }

    #[test]
    fn gui_pixel_waits_keep_tick_deadline() {
        let mut runner = pixel_wait_test_runner();

        let error = run_until_pixel(
            &mut runner,
            0,
            0,
            [1, 2, 3],
            false,
            0,
            Some(1),
            Some(2),
            Some(5),
            Some("gui missing pixel"),
            RunMode::Gui,
        )
        .unwrap_err();

        assert!(
            error.contains("timed out at tick 2"),
            "gui polling should fail by tick deadline, got: {error}"
        );
        assert_eq!(runner.guest_tick(), 2);
    }
}
