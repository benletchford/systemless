//! Systemless CLI: GPUI presentation and deterministic headless execution.

#[cfg(target_os = "macos")]
#[path = "gpui_demo.rs"]
mod gpui_frontend;
#[cfg(target_os = "macos")]
#[path = "desktop/native_bundle.rs"]
mod native_bundle;
#[path = "desktop/desktop_save_store.rs"]
mod desktop_save_store;
#[path = "desktop/headless_time.rs"]
mod headless_time;
#[path = "desktop/play.rs"]
mod play;
#[cfg(test)]
#[path = "desktop/cpu_frame.rs"]
mod cpu_frame;

use std::path::PathBuf;
use clap::Parser;
use desktop_save_store::DesktopSaveStore;
use systemless::api::{InstructionBudget, VideoFrame};
use systemless::runner::FixtureRunner;
use systemless::systems::macintosh::session::{MacintoshInput, MacintoshSession};
use systemless::systems::macintosh::{display, game};
use systemless::ui_theme::UiThemeId;

#[cfg(all(feature = "debug-server", unix))]
#[path = "desktop/debug_server.rs"]
mod debug_server;
#[cfg(not(all(feature = "debug-server", unix)))]
mod debug_server {
    use std::path::Path;

    pub struct DebugServer;

    impl DebugServer {
        pub fn bind(_path: &Path) -> std::io::Result<Self> {
            Err(std::io::Error::new(
                std::io::ErrorKind::Unsupported,
                "debug socket transport requires the `debug-server` feature on a Unix platform",
            ))
        }

        pub fn pump(&mut self, _runner: &mut systemless::runner::FixtureRunner) -> usize {
            0
        }
    }
}

/// Frame duration at 60.15 Hz (Compact Mac VBL rate).
const FRAME_DURATION: std::time::Duration = std::time::Duration::from_micros(16_625);
/// Foreground GUI work is checked against the host deadline only between
/// batches. Keep each slice well below a realtime VBL so heavy startup loads
/// can still present intermediate drawing and service Sound Manager callbacks.
const CPU_BATCH_INSTRUCTIONS: usize = 10_000;
/// PPC HLE slices move process-owned collections into the dispatch closure
/// and restore them afterward, so every slice boundary has a fixed cost and
/// batches stay coarser than the 68K instruction batches. Subdividing the
/// guest tick (rather than running one full-tick batch) keeps that cost
/// bounded while letting the GUI loop re-check its wall-clock CPU deadline
/// between sub-tick batches: draw-heavy imports used to let a single
/// full-tick batch run for ~1 s before `cpu_deadline` was consulted again.
const PPC_GUI_BATCH_TICK_DIVISOR: usize = 16;
const SOUND_CALLBACK_SLICE_INSTRUCTIONS: usize = CPU_BATCH_INSTRUCTIONS;
const SOUND_CALLBACK_RESERVED_INSTRUCTIONS_PER_FRAME: usize = 25_000;
const AUDIO_CALLBACK_CHUNK_SAMPLES: usize = 32;
const DEFAULT_GUI_ARROWS_AS_NUMPAD: bool = false;
fn foreground_cpu_batch_instructions(powerpc: bool, instructions_per_tick: u32) -> usize {
    if powerpc {
        (instructions_per_tick.max(1) as usize).div_ceil(PPC_GUI_BATCH_TICK_DIVISOR)
    } else {
        CPU_BATCH_INSTRUCTIONS
    }
}

/// Both realtime frontends must use the loaded architecture's execution rate.
/// Calling GUI slice methods alone does not change the fixture's much smaller
/// default budget, which would put identical tick-script inputs at different
/// points in application startup.
fn configure_realtime_execution_rate(runner: &mut FixtureRunner) -> u32 {
    let instructions =
        systemless::runner::default_realtime_instructions_per_tick(runner.is_powerpc_app());
    runner.set_instructions_per_tick(instructions);
    instructions
}

#[derive(Debug, Parser)]
#[command(name = "systemless", version, about)]
struct Cli {
    /// Application or game archive to launch
    #[arg(value_name = "GAME")]
    game: PathBuf,

    /// Run without opening a window
    #[arg(long)]
    headless: bool,

    /// Disable host-native desktop integrations
    #[arg(long)]
    no_native_integrations: bool,

    /// Map arrow keys to the numeric keypad
    #[arg(long, conflicts_with = "literal_arrows")]
    arrows_as_numpad: bool,

    /// Keep arrow keys mapped as literal arrow keys
    #[arg(
        long,
        visible_alias = "no-arrows-as-numpad",
        conflicts_with = "arrows_as_numpad"
    )]
    literal_arrows: bool,

    /// Stop a headless run after this many instructions
    #[arg(long, value_name = "N")]
    max_instructions: Option<usize>,

    /// Run headlessly for this many simulated frontend ticks (60.15 Hz),
    /// using GUI wait/callback scheduling. Defaults to 600 without legacy options.
    #[arg(
        long,
        requires = "headless",
        conflicts_with_all = ["max_instructions", "input_script"],
        value_parser = clap::value_parser!(u32).range(1..)
    )]
    max_ticks: Option<u32>,

    /// Input script timestamped in elapsed frontend ticks, not instructions
    #[arg(
        long,
        requires = "max_ticks",
        conflicts_with = "input_script",
        value_name = "FILE"
    )]
    tick_input_script: Option<PathBuf>,

    /// Deterministic Mac-epoch startup seconds for simulated-time headless runs
    #[arg(
        long,
        requires = "headless",
        conflicts_with_all = ["max_instructions", "input_script"]
    )]
    headless_start_time: Option<u32>,

    /// Execute a structured JSON scenario using simulated frontend ticks
    #[arg(long, requires = "headless", conflicts_with_all = ["max_ticks", "max_instructions", "input_script", "tick_input_script", "debug_socket", "reset_preferences"])]
    play_script: Option<PathBuf>,

    /// New or empty directory for play captures and report.json
    #[arg(long, requires = "play_script", value_name = "DIR")]
    play_output: Option<PathBuf>,

    /// Directory of independently captured, exactly named reference PNGs
    #[arg(long, requires = "play_script", value_name = "DIR")]
    play_reference: Option<PathBuf>,

    /// Prefer a native PowerPC slice when a classic 68K slice is also available (the default)
    #[arg(
        long,
        visible_alias = "prefer-ppc",
        conflicts_with = "prefer_classic_68k"
    )]
    prefer_powerpc: bool,

    /// Prefer the classic 68K slice of a fat application
    #[arg(long, visible_alias = "prefer-68k", conflicts_with = "prefer_powerpc")]
    prefer_classic_68k: bool,

    /// Start with classic 24-bit guest address translation
    #[arg(long)]
    addressing_24_bit: bool,

    /// Override the guest framebuffer depth (defaults to 8-bit for 68K and 16-bit for PPC)
    #[arg(long, value_name = "BITS", value_parser = parse_screen_depth)]
    screen_depth: Option<u16>,

    /// Override automatic window sizing with an integer physical-pixel scale
    #[arg(long, value_name = "N", value_parser = parse_display_scale)]
    display_scale: Option<u32>,

    /// Delete this game's persisted System Folder preferences before launch
    #[arg(long)]
    reset_preferences: bool,

    /// Guest chrome theme
    #[arg(
        long,
        value_name = "THEME",
        default_value = "classic-system7",
        value_parser = UiThemeId::parse
    )]
    ui_theme: UiThemeId,
    /// Start in a borderless fullscreen space. On systems where macOS selects
    /// direct scan-out for the fullscreen surface this measurably reduced
    /// pointer-to-screen latency in testing; the benefit depends on the
    /// machine and compositor state.
    #[arg(long)]
    fullscreen: bool,

    /// Legacy diagnostic input script timestamped in retired instructions.
    /// Not a matched-time workload: wait optimizations change when inputs land.
    #[arg(long, value_name = "FILE")]
    input_script: Option<PathBuf>,

    /// Expose the debugger over a Unix-domain socket. One controlling client
    /// sends one JSON request per line and receives one JSON reply per line;
    /// requests are applied on the runner thread between frames. Headless
    /// runs start paused until the client resumes or steps execution.
    #[arg(long, value_name = "PATH")]
    debug_socket: Option<PathBuf>,
}

fn parse_screen_depth(value: &str) -> Result<u16, String> {
    match value {
        "1" => Ok(1),
        "2" => Ok(2),
        "4" => Ok(4),
        "8" => Ok(8),
        _ => Err("screen depth must be 1, 2, 4, or 8".to_string()),
    }
}

fn parse_display_scale(value: &str) -> Result<u32, String> {
    let scale = value
        .parse::<u32>()
        .map_err(|_| "display scale must be an integer from 1 through 8".to_string())?;
    if (1..=8).contains(&scale) {
        Ok(scale)
    } else {
        Err("display scale must be an integer from 1 through 8".to_string())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ScriptedInput {
    at: usize,
    action: InputAction,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum InputAction {
    MouseMove { v: i16, h: i16 },
    MouseDown { v: i16, h: i16 },
    MouseUp { v: i16, h: i16 },
    KeyDown { key: u8, ch: u8 },
    KeyUp { key: u8, ch: u8 },
}

/// Parse an input script: one `<at> <action> [args]` per line, `#`
/// comments and blank lines ignored. `click` and `press` expand to a
/// down/up pair at the same instant, which is what a guest sees for an
/// ordinary click or keystroke.
///
/// Numbers accept `0x` prefixes so key codes can be written the way the
/// Mac key tables list them.
fn parse_input_script(text: &str) -> Result<Vec<ScriptedInput>, String> {
    fn num<T: TryFrom<u64>>(tok: &str, line: usize) -> Result<T, String> {
        let raw = tok.strip_prefix("0x").map_or_else(
            || tok.parse::<u64>().map_err(|e| e.to_string()),
            |hex| u64::from_str_radix(hex, 16).map_err(|e| e.to_string()),
        );
        let value = raw.map_err(|e| format!("line {line}: bad number {tok:?}: {e}"))?;
        T::try_from(value).map_err(|_| format!("line {line}: {tok:?} out of range"))
    }

    let mut out = Vec::new();
    for (index, raw) in text.lines().enumerate() {
        let line = index + 1;
        let body = raw.split('#').next().unwrap_or("").trim();
        if body.is_empty() {
            continue;
        }
        let tok: Vec<&str> = body.split_whitespace().collect();
        if tok.len() < 2 {
            return Err(format!("line {line}: expected `<at> <action> [args]`"));
        }
        let at: usize = num(tok[0], line)?;
        let args = &tok[2..];
        let expect = |n: usize| -> Result<(), String> {
            if args.len() == n {
                Ok(())
            } else {
                Err(format!(
                    "line {line}: `{}` takes {n} argument(s), got {}",
                    tok[1],
                    args.len()
                ))
            }
        };
        let mut push = |action| out.push(ScriptedInput { at, action });
        match tok[1] {
            "mousemove" => {
                expect(2)?;
                push(InputAction::MouseMove {
                    v: num(args[0], line)?,
                    h: num(args[1], line)?,
                });
            }
            "mousedown" => {
                expect(2)?;
                push(InputAction::MouseDown {
                    v: num(args[0], line)?,
                    h: num(args[1], line)?,
                });
            }
            "mouseup" => {
                expect(2)?;
                push(InputAction::MouseUp {
                    v: num(args[0], line)?,
                    h: num(args[1], line)?,
                });
            }
            "click" => {
                expect(2)?;
                let (v, h) = (num(args[0], line)?, num(args[1], line)?);
                push(InputAction::MouseDown { v, h });
                push(InputAction::MouseUp { v, h });
            }
            "keydown" => {
                expect(2)?;
                push(InputAction::KeyDown {
                    key: num(args[0], line)?,
                    ch: num(args[1], line)?,
                });
            }
            "keyup" => {
                expect(2)?;
                push(InputAction::KeyUp {
                    key: num(args[0], line)?,
                    ch: num(args[1], line)?,
                });
            }
            "press" => {
                expect(2)?;
                let (key, ch) = (num(args[0], line)?, num(args[1], line)?);
                push(InputAction::KeyDown { key, ch });
                push(InputAction::KeyUp { key, ch });
            }
            other => return Err(format!("line {line}: unknown action {other:?}")),
        }
    }
    out.sort_by_key(|event| event.at);
    Ok(out)
}

/// How many instructions to run before the next scheduled event.
///
/// Without this the loop would overshoot by up to a whole chunk and the
/// delivery point would depend on chunk size rather than on the script,
/// which would break the determinism the schedule exists to provide.
fn steps_until_next_event(
    chunk: usize,
    remaining: usize,
    total: usize,
    next_at: Option<usize>,
) -> usize {
    let bounded = chunk.min(remaining);
    match next_at {
        Some(at) => bounded.min(at.saturating_sub(total).max(1)),
        None => bounded,
    }
}

fn service_pending_sound_work_budgeted(
    runner: &mut FixtureRunner,
    slice_budget: usize,
    total_steps: usize,
    reserved_sound_steps: &mut usize,
) -> Option<usize> {
    if !runner.has_pending_sound_work() || runner.is_halted() {
        return None;
    }

    // Double-buffer callbacks are Sound Manager interrupt work, not foreground
    // application execution. Give them reserved time even when the GUI frame
    // has spent its foreground budget, but cap that reserve per host frame so
    // audio refills cannot monopolize the single-threaded event loop.
    let remaining = slice_budget.saturating_sub(total_steps);
    let using_reserved_slice = remaining == 0;
    let callback_budget = if using_reserved_slice {
        let reserved_remaining =
            SOUND_CALLBACK_RESERVED_INSTRUCTIONS_PER_FRAME.saturating_sub(*reserved_sound_steps);
        if reserved_remaining == 0 {
            return None;
        }
        reserved_remaining.min(SOUND_CALLBACK_SLICE_INSTRUCTIONS)
    } else {
        remaining.min(SOUND_CALLBACK_SLICE_INSTRUCTIONS)
    };

    // This slice services audio interrupts, not a presentation frame. The
    // outer presentation pass restores native chrome once.
    let (steps, _running) = runner.run_gui_pending_sound_work(callback_budget);
    if using_reserved_slice {
        *reserved_sound_steps = reserved_sound_steps.saturating_add(steps);
    }
    Some(steps)
}


#[derive(Default)]
struct HostMouseReleaseLatch {
    pressed: bool,
    guest_observed_press: bool,
    guest_needs_release_observation: bool,
    pending_release: Option<(i16, i16)>,
}

impl HostMouseReleaseLatch {
    fn press(&mut self) {
        self.pressed = true;
        self.guest_observed_press = false;
        self.guest_needs_release_observation = false;
        self.pending_release = None;
    }

    fn release(&mut self, position: (i16, i16)) -> Option<(i16, i16)> {
        if self.pressed && !self.guest_observed_press {
            self.pending_release = Some(position);
            None
        } else {
            let had_press = self.pressed;
            self.pressed = false;
            self.guest_observed_press = false;
            self.guest_needs_release_observation = had_press;
            Some(position)
        }
    }

    fn observe_guest_progress(&mut self) {
        if self.pressed && !self.guest_observed_press {
            self.guest_observed_press = true;
        } else if self.guest_needs_release_observation {
            self.guest_needs_release_observation = false;
        }
    }

    fn take_ready_release(&mut self) -> Option<(i16, i16)> {
        if !self.guest_observed_press || self.pending_release.is_none() {
            return None;
        }
        self.pressed = false;
        self.guest_observed_press = false;
        self.guest_needs_release_observation = true;
        self.pending_release.take()
    }
}

fn save_frame(frame: &VideoFrame, ticks: u32, num: usize) {
    let w = frame.width;
    let h = frame.height;
    if w == 0 || h == 0 {
        eprintln!(
            "[HEADLESS] Screenshot #{}: skipped (screen not initialized)",
            num
        );
        return;
    }
    let rgba = &frame.pixels;

    let img = image::RgbImage::from_fn(w, h, |x, y| {
        let idx = ((y * w + x) * 4) as usize;
        image::Rgb([rgba[idx], rgba[idx + 1], rgba[idx + 2]])
    });

    let path = std::env::temp_dir().join(format!("systemless_headless_{:04}.png", num));
    img.save(&path).expect("Failed to save screenshot");
    eprintln!(
        "[HEADLESS] Screenshot #{}: {} (ticks={})",
        num,
        path.display(),
        ticks
    );
}

fn save_screenshot(runner: &FixtureRunner, num: usize) {
    let mode = runner.dispatcher().screen_mode;
    if mode.2 == 0 || mode.3 == 0 {
        eprintln!(
            "[HEADLESS] Screenshot #{}: skipped (screen not initialized)",
            num
        );
        return;
    }
    let gamma = runner.dispatcher().device_gamma();
    let frame = VideoFrame {
        width: u32::from(mode.2),
        height: u32::from(mode.3),
        format: systemless::api::PixelFormat::Rgba8,
        pixels: display::render_screen_with_gamma(
            runner.bus(),
            mode,
            &runner.dispatcher().device_clut,
            &gamma,
        ),
    };
    save_frame(&frame, runner.guest_tick(), num);
}

// Both headless clocks use the same transport and command-safe point. A debug
// run starts paused so the simulated clock cannot outrun client attachment.
fn bind_headless_debug_server(
    path: Option<PathBuf>,
    runner: &mut FixtureRunner,
) -> Option<debug_server::DebugServer> {
    let path = path?;
    let server = debug_server::DebugServer::bind(&path).unwrap_or_else(|error| {
        eprintln!(
            "Error: cannot bind debug socket {}: {error}",
            path.display()
        );
        std::process::exit(1);
    });
    #[cfg(all(feature = "debug-server", unix))]
    systemless::debug::handle_debug_request(runner, systemless::debug::DebugRequest::Pause)
        .expect("initial debugger pause");
    let _ = runner;
    eprintln!("[HEADLESS] Debugger paused at startup; connect and resume to execute");
    Some(server)
}

fn wait_for_debug_resume(server: &mut debug_server::DebugServer, runner: &mut FixtureRunner) {
    loop {
        server.pump(runner);
        if !runner.debug_is_paused() && !runner.is_halted() {
            return;
        }
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
}

fn run_headless(
    game_path: &std::path::Path,
    max_instructions: usize,
    addressing_24_bit: bool,
    screen_depth: Option<u16>,
    script: &[ScriptedInput],
    ui_theme: UiThemeId,
    debug_socket: Option<PathBuf>,
) {
    eprintln!(
        "[HEADLESS] Legacy instruction-budget diagnostic mode: retained Toolbox waits may re-fire repeatedly; do not use these totals as GUI CPU measurements. Use --max-ticks with --tick-input-script for time-based runs."
    );
    eprintln!("[HEADLESS] Starting: {}", game_path.display());
    eprintln!("[HEADLESS] Max instructions: {}", max_instructions);

    let mut session = MacintoshSession::new(!addressing_24_bit, screen_depth);
    session.runner_mut().set_ui_theme(ui_theme);
    let app = session.load_path(game_path).expect("Failed to load game");
    let mut save_store = DesktopSaveStore::for_loaded_archive(game_path, session.runner_mut());
    eprintln!(
        "[SYSTEMLESS] Desktop save dir: {}",
        save_store.root().display()
    );
    let restored_saves = save_store.load_saved_files();
    for file in &restored_saves {
        session.runner_mut().import_vfs_file(file);
    }
    if !restored_saves.is_empty() {
        eprintln!(
            "[SYSTEMLESS] Restored {} desktop save file(s)",
            restored_saves.len()
        );
    }
    session.initialize(&app);

    let mut debug_server = bind_headless_debug_server(debug_socket, session.runner_mut());

    let chunk = 100_000;
    let mut total: usize = 0;
    let mut last_screenshot = 0usize;
    let mut next_event = 0usize;

    while total < max_instructions {
        if let Some(server) = debug_server.as_mut() {
            wait_for_debug_resume(server, session.runner_mut());
        }
        // Deliver everything the script has scheduled at or before this
        // point, then run only as far as the next event so its delivery
        // point comes from the script and not from the chunk size.
        while let Some(event) = script.get(next_event) {
            if event.at > total {
                break;
            }
            match event.action {
                InputAction::MouseMove { v, h } => {
                    session.deliver_input(MacintoshInput::MouseMove {
                        vertical: v,
                        horizontal: h,
                    })
                }
                InputAction::MouseDown { v, h } => {
                    session.deliver_input(MacintoshInput::MouseDown {
                        vertical: v,
                        horizontal: h,
                    })
                }
                InputAction::MouseUp { v, h } => session.deliver_input(MacintoshInput::MouseUp {
                    vertical: v,
                    horizontal: h,
                }),
                InputAction::KeyDown { key, ch } => {
                    session.deliver_input(MacintoshInput::KeyDown {
                        mac_key: key,
                        character: ch,
                    })
                }
                InputAction::KeyUp { key, ch } => session.deliver_input(MacintoshInput::KeyUp {
                    mac_key: key,
                    character: ch,
                }),
            }
            eprintln!("[HEADLESS] input @{}: {:?}", event.at, event.action);
            next_event += 1;
        }
        let steps_to_run = steps_until_next_event(
            chunk,
            max_instructions - total,
            total,
            script.get(next_event).map(|event| event.at),
        );
        let advance = session.advance(InstructionBudget(steps_to_run));
        total += advance.instructions;

        let screenshot_num = total / 500_000;
        if screenshot_num > last_screenshot {
            last_screenshot = screenshot_num;
            // Measurement-only switch: timing A/Bs suppress the periodic
            // PNG encodes (a constant ~7s of host work per census run)
            // while keeping the final screenshot and its tick check.
            if std::env::var("SYSTEMLESS_HEADLESS_PERIODIC_SCREENSHOTS")
                .map(|v| v != "0")
                .unwrap_or(true)
            {
                if let Some(frame) = session.video_frame() {
                    save_frame(&frame, session.status().guest_tick, screenshot_num);
                }
            }
        }

        if !advance.running {
            eprintln!("[HEADLESS] CPU stopped after {} instructions", total);
            break;
        }
    }

    eprintln!("[HEADLESS] Completed {} instructions", total);
    save_store.sync_save_files_now(session.runner_mut());
    if let Some(frame) = session.video_frame() {
        save_frame(&frame, session.status().guest_tick, 9999);
    }
    // Measurement-only: prints nothing unless SYSTEMLESS_WAIT_STATS is set.
    systemless::runner::dump_wait_stats();
    if debug_server.is_some() && session.runner().is_halted() {
        eprintln!(
            "[HEADLESS] Debugger remains available after terminal stop (press Ctrl-C to exit)"
        );
        loop {
            if let Some(server) = debug_server.as_mut() {
                server.pump(session.runner_mut());
            }
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
    }
}

fn main() {
    let cli = Cli::parse();
    if !cli.prefer_classic_68k {
        // SAFETY: the runner has not started and no worker threads exist yet.
        unsafe { std::env::set_var("SYSTEMLESS_PREFER_POWERPC", "1") };
        eprintln!("[SYSTEMLESS] Native PowerPC slice preferred when available");
    } else {
        // An explicit CLI choice takes precedence over a preference inherited
        // from the parent process or native application relaunch.
        unsafe { std::env::set_var("SYSTEMLESS_PREFER_POWERPC", "0") };
        eprintln!("[SYSTEMLESS] Classic 68K slice preferred when available");
    }
    let game_path = cli.game;
    let arrows_as_numpad = if cli.literal_arrows {
        false
    } else if cli.arrows_as_numpad {
        true
    } else {
        DEFAULT_GUI_ARROWS_AS_NUMPAD
    };

    if let Some(script) = cli.play_script.as_deref() {
        let output = cli
            .play_output
            .as_deref()
            .unwrap_or_else(|| std::path::Path::new("play-output"));
        if let Err(error) = play::run(
            &game_path,
            script,
            output,
            cli.play_reference.as_deref(),
            cli.headless_start_time.unwrap_or(3_871_497_600),
            cli.addressing_24_bit,
            cli.screen_depth,
            cli.ui_theme,
        ) {
            eprintln!("[PLAY] {error}");
            std::process::exit(1);
        }
        return;
    }

    if !game_path.exists() {
        eprintln!("Error: Game file not found: {}", game_path.display());
        std::process::exit(1);
    }

    if cli.reset_preferences {
        match DesktopSaveStore::reset_preferences(&game_path) {
            Ok(0) => eprintln!("[SYSTEMLESS] No persisted preferences to reset"),
            Ok(_) => eprintln!("[SYSTEMLESS] Reset persisted System Folder preferences"),
            Err(err) => {
                eprintln!("Error: Could not reset preferences: {}", err);
                std::process::exit(1);
            }
        }
    }

    eprintln!("[SYSTEMLESS] Starting emulator...");
    eprintln!("[SYSTEMLESS] Game: {}", game_path.display());

    if cli.headless {
        let timed = cli.max_instructions.is_none() && cli.input_script.is_none();
        let script_path = if timed {
            cli.tick_input_script.as_deref()
        } else {
            cli.input_script.as_deref()
        };
        let script = match script_path {
            Some(path) => {
                let text = std::fs::read_to_string(path).unwrap_or_else(|e| {
                    eprintln!("Error: cannot read input script {}: {e}", path.display());
                    std::process::exit(1);
                });
                parse_input_script(&text).unwrap_or_else(|e| {
                    eprintln!("Error in input script {}: {e}", path.display());
                    std::process::exit(1);
                })
            }
            None => Vec::new(),
        };
        if timed {
            headless_time::run(
                &game_path,
                cli.max_ticks.unwrap_or(600),
                cli.headless_start_time.unwrap_or(3_871_497_600),
                cli.addressing_24_bit,
                cli.screen_depth,
                &script,
                cli.ui_theme,
                cli.debug_socket,
            );
        } else {
            run_headless(
                &game_path,
                cli.max_instructions.unwrap_or(5_000_000),
                cli.addressing_24_bit,
                cli.screen_depth,
                &script,
                cli.ui_theme,
                cli.debug_socket,
            );
        }
    } else {
        if cli.input_script.is_some() {
            eprintln!("Error: --input-script requires --headless");
            std::process::exit(1);
        }
        #[cfg(not(all(feature = "debug-server", unix)))]
        if cli.debug_socket.is_some() {
            eprintln!("Error: --debug-socket requires the debug-server feature on a Unix host");
            std::process::exit(1);
        }
        #[cfg(target_os = "macos")]
        if !cli.no_native_integrations && !native_bundle::already_relaunched() {
            match native_bundle::prepare_for_game(&game_path) {
                Ok(Some(bundle)) => {
                    let error = native_bundle::exec_bundle(&bundle);
                    eprintln!("[SYSTEMLESS] Could not relaunch with guest identity: {error}");
                }
                Ok(None) => {}
                Err(error) => eprintln!("[SYSTEMLESS] Could not prepare guest identity: {error}"),
            }
        }
        #[cfg(target_os = "macos")]
        gpui_frontend::launch(gpui_frontend::LaunchOptions {
            game: game_path,
            prefer_powerpc: !cli.prefer_classic_68k,
            screen_depth: cli.screen_depth,
            addressing_24_bit: cli.addressing_24_bit,
            arrows_as_numpad,
            display_scale: cli.display_scale,
            ui_theme: cli.ui_theme,
            fullscreen: cli.fullscreen,
            debug_socket: cli.debug_socket,
            native_integrations: !cli.no_native_integrations,
        });
        #[cfg(not(target_os = "macos"))]
        {
            eprintln!("Error: GPUI desktop presentation currently supports macOS only; use --headless on this host");
            std::process::exit(1);
        }
    }
}

#[cfg(test)]
mod input_script_tests {
    use super::*;

    #[test]
    fn parses_actions_comments_and_blank_lines() {
        let script = parse_input_script(
            "# get into the game\n\
             \n\
             1000 mousemove 10 20\n\
             2000 click 100 200   # dismiss the splash\n\
             3000 press 0x24 13\n\
             4000 keydown 0x7B 28\n\
             4500 keyup 0x7B 28\n\
             5000 mousedown 1 2\n\
             5500 mouseup 1 2\n",
        )
        .expect("script parses");
        assert_eq!(
            script,
            vec![
                ScriptedInput {
                    at: 1000,
                    action: InputAction::MouseMove { v: 10, h: 20 }
                },
                // click expands to the down/up pair a guest sees
                ScriptedInput {
                    at: 2000,
                    action: InputAction::MouseDown { v: 100, h: 200 }
                },
                ScriptedInput {
                    at: 2000,
                    action: InputAction::MouseUp { v: 100, h: 200 }
                },
                ScriptedInput {
                    at: 3000,
                    action: InputAction::KeyDown { key: 0x24, ch: 13 }
                },
                ScriptedInput {
                    at: 3000,
                    action: InputAction::KeyUp { key: 0x24, ch: 13 }
                },
                ScriptedInput {
                    at: 4000,
                    action: InputAction::KeyDown { key: 0x7B, ch: 28 }
                },
                ScriptedInput {
                    at: 4500,
                    action: InputAction::KeyUp { key: 0x7B, ch: 28 }
                },
                ScriptedInput {
                    at: 5000,
                    action: InputAction::MouseDown { v: 1, h: 2 }
                },
                ScriptedInput {
                    at: 5500,
                    action: InputAction::MouseUp { v: 1, h: 2 }
                },
            ]
        );
    }

    #[test]
    fn out_of_order_lines_are_sorted_by_instruction_count() {
        // The loop consumes the schedule in order, so a script written out
        // of order must not silently drop its early events.
        let script = parse_input_script("900 click 1 1\n100 press 2 2\n").expect("parses");
        assert_eq!(script.first().map(|e| e.at), Some(100));
        assert_eq!(script.last().map(|e| e.at), Some(900));
    }

    #[test]
    fn bad_lines_name_the_line_and_the_problem() {
        for (text, needle) in [
            ("1000 click 1\n", "takes 2 argument"),
            ("1000 wiggle 1 2\n", "unknown action"),
            ("abc click 1 2\n", "bad number"),
            ("1000\n", "expected"),
            ("1000 click 99999 1\n", "out of range"),
        ] {
            let err = parse_input_script(text).expect_err("must reject");
            assert!(
                err.contains(needle) && err.contains("line 1"),
                "error {err:?} should mention line 1 and {needle:?}"
            );
        }
    }

    #[test]
    fn the_run_stops_exactly_at_the_next_event() {
        // Overshooting would make delivery depend on chunk size rather than
        // on the script, which is precisely the determinism being bought.
        assert_eq!(
            steps_until_next_event(100_000, 500_000, 0, Some(1_500)),
            1_500
        );
        // Never zero: a zero-step run would spin without ever advancing.
        assert_eq!(
            steps_until_next_event(100_000, 500_000, 1_500, Some(1_500)),
            1
        );
        // No events left, or none near: the ordinary chunk applies.
        assert_eq!(steps_until_next_event(100_000, 500_000, 0, None), 100_000);
        assert_eq!(
            steps_until_next_event(100_000, 500_000, 0, Some(900_000)),
            100_000
        );
        // The instruction budget still wins over both.
        assert_eq!(steps_until_next_event(100_000, 250, 0, None), 250);
        assert_eq!(steps_until_next_event(100_000, 250, 0, Some(1_000)), 250);
    }
}
