//! Structured headless scenarios. Execution shares the frontend scheduler.
use super::{
    configure_realtime_execution_rate, display, game, headless_time, FixtureRunner,
    HostMouseReleaseLatch, InputAction, UiThemeId,
};
use image::RgbImage;
use serde::{Deserialize, Serialize};
use std::{collections::HashSet, fs, path::Path};
use systemless::memory::MemoryBus;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Script {
    version: u32,
    clock: String,
    max_ticks: u32,
    #[serde(default)]
    reference_tolerance: u8,
    actions: Vec<Action>,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
enum Action {
    Run {
        ticks: u32,
    },
    MouseMove {
        v: i16,
        h: i16,
    },
    MouseDown {
        v: i16,
        h: i16,
    },
    MouseUp {
        v: i16,
        h: i16,
    },
    KeyDown {
        key: u8,
        ch: u8,
    },
    KeyUp {
        key: u8,
        ch: u8,
    },
    Screenshot {
        name: String,
    },
    RunUntilPixel {
        x: u32,
        y: u32,
        rgb: [u8; 3],
        #[serde(default)]
        not: bool,
        #[serde(default)]
        tolerance: u8,
        timeout_ticks: u32,
    },
    AssertPixel {
        x: u32,
        y: u32,
        rgb: [u8; 3],
        #[serde(default)]
        not: bool,
        #[serde(default)]
        tolerance: u8,
    },
    AssertMemoryWord {
        address: u32,
        value: u16,
    },
    AssertAudioDuring {
        ticks: u32,
        min_non_silent: usize,
    },
    Log {
        message: String,
    },
    Milestone {
        name: String,
    },
}

impl Script {
    fn parse(text: &str) -> Result<Self, String> {
        let script: Self = serde_json::from_str(text).map_err(|e| e.to_string())?;
        if script.version != 1 || script.clock != "frontend_ticks" {
            return Err("expected version=1 and clock=frontend_ticks; guest/instruction-clock scripts require manual conversion".into());
        }
        if script.max_ticks == 0 || script.actions.is_empty() {
            return Err("max_ticks and actions must be nonempty".into());
        }
        let mut names = HashSet::new();
        for action in &script.actions {
            match action {
                Action::Screenshot { name } => {
                    if name.eq_ignore_ascii_case("failure")
                        || name.is_empty()
                        || !name
                            .bytes()
                            .all(|c| c.is_ascii_alphanumeric() || c == b'_' || c == b'-')
                        || !names.insert(name.to_ascii_lowercase())
                    {
                        return Err(format!("capture name must be unique and contain only letters, digits, '_' or '-': {name:?}"));
                    }
                }
                Action::RunUntilPixel {
                    timeout_ticks: 0, ..
                }
                | Action::AssertAudioDuring { ticks: 0, .. } => {
                    return Err("wait/audio duration must be positive".into())
                }
                Action::AssertMemoryWord { address, .. }
                    if address % 2 != 0 || *address > u32::MAX - 1 =>
                {
                    return Err("memory word address must be aligned and in range".into())
                }
                _ => {}
            }
        }
        Ok(script)
    }
}

#[derive(Default, Serialize)]
struct Report {
    version: u32,
    systemless_version: String,
    archive: String,
    script: String,
    reference_directory: Option<String>,
    mac_time_secs: u32,
    architecture: Option<String>,
    status: String,
    error: Option<String>,
    failed_action: Option<usize>,
    frontend_ticks: u32,
    guest_tick: u32,
    budget_exhausted_frames: u32,
    assertions: usize,
    checkpoints: Vec<Checkpoint>,
}
#[derive(Serialize)]
struct Checkpoint {
    name: String,
    frontend_ticks: u32,
    guest_tick: u32,
    differing_pixels: Option<usize>,
}

struct Execution<'a> {
    runner: FixtureRunner,
    mouse: HostMouseReleaseLatch,
    remainder: f64,
    max_ticks: u32,
    report: &'a mut Report,
}
impl Execution<'_> {
    fn advance(&mut self, ticks: u32) -> Result<usize, String> {
        if ticks > self.max_ticks.saturating_sub(self.report.frontend_ticks) {
            return Err(format!("scenario exceeds max_ticks={}", self.max_ticks));
        }
        let mut non_silent = 0;
        let mut audio = Vec::new();
        for _ in 0..ticks {
            let samples = systemless::sound::OUTPUT_RATE as f64
                / systemless::runner::DEFAULT_VBL_HZ
                + self.remainder;
            self.remainder = samples.fract();
            let work = headless_time::frame(&mut self.runner, samples.floor() as usize);
            self.report.frontend_ticks += 1;
            self.report.guest_tick = self.runner.guest_tick();
            self.report.budget_exhausted_frames += u32::from(work.budget_exhausted);
            if work.foreground > 0 {
                self.mouse.observe_guest_progress();
            }
            if let Some((v, h)) = self.mouse.take_ready_release() {
                self.runner.push_mouse_up(v, h);
            }
            audio.clear();
            self.runner.drain_audio_into(&mut audio);
            non_silent += audio.iter().filter(|&&v| v != 128).count();
            if self.runner.is_halted() {
                return Err("guest halted before scenario completed".into());
            }
        }
        Ok(non_silent)
    }
    fn image(&self) -> Result<RgbImage, String> {
        let mode = self.runner.dispatcher().screen_mode;
        if mode.2 == 0 || mode.3 == 0 {
            return Err("screen not initialized".into());
        }
        let rgba = display::render_screen_with_gamma(
            self.runner.bus(),
            mode,
            &self.runner.dispatcher().device_clut,
            &self.runner.dispatcher().device_gamma(),
        );
        let rgb = rgba
            .chunks_exact(4)
            .flat_map(|p| [p[0], p[1], p[2]])
            .collect();
        RgbImage::from_raw(mode.2.into(), mode.3.into(), rgb)
            .ok_or_else(|| "invalid framebuffer".into())
    }
    fn pixel(
        &self,
        x: u32,
        y: u32,
        rgb: [u8; 3],
        not: bool,
        tolerance: u8,
    ) -> Result<bool, String> {
        let frame = self.image()?;
        if x >= frame.width() || y >= frame.height() {
            return Err(format!(
                "pixel ({x},{y}) outside {}x{}",
                frame.width(),
                frame.height()
            ));
        }
        Ok(pixel_matches(frame.get_pixel(x, y).0, rgb, tolerance) != not)
    }
    fn actions(
        &mut self,
        script: &Script,
        output: &Path,
        reference: Option<&Path>,
    ) -> Result<(), String> {
        for (index, action) in script.actions.iter().enumerate() {
            self.report.failed_action = Some(index);
            eprintln!(
                "[PLAY] action={index} frontend_tick={} {action:?}",
                self.report.frontend_ticks
            );
            let input = match action {
                Action::MouseMove { v, h } => Some(InputAction::MouseMove { v: *v, h: *h }),
                Action::MouseDown { v, h } => Some(InputAction::MouseDown { v: *v, h: *h }),
                Action::MouseUp { v, h } => Some(InputAction::MouseUp { v: *v, h: *h }),
                Action::KeyDown { key, ch } => Some(InputAction::KeyDown { key: *key, ch: *ch }),
                Action::KeyUp { key, ch } => Some(InputAction::KeyUp { key: *key, ch: *ch }),
                _ => None,
            };
            if let Some(input) = input {
                headless_time::deliver(&mut self.runner, &mut self.mouse, input);
                continue;
            }
            match action {
                Action::Run { ticks } => {
                    self.advance(*ticks)?;
                }
                Action::Screenshot { name } => {
                    let frame = self.image()?;
                    frame
                        .save(output.join(format!("{name}.png")))
                        .map_err(|e| e.to_string())?;
                    let differing_pixels = reference
                        .map(|dir| {
                            compare(
                                &frame,
                                &dir.join(format!("{name}.png")),
                                script.reference_tolerance,
                            )
                        })
                        .transpose()?;
                    self.report.checkpoints.push(Checkpoint {
                        name: name.clone(),
                        frontend_ticks: self.report.frontend_ticks,
                        guest_tick: self.runner.guest_tick(),
                        differing_pixels,
                    });
                    if let Some(count) = differing_pixels {
                        self.report.assertions += 1;
                        if count > 0 {
                            return Err(format!(
                                "checkpoint {name}: {count} pixels differ from reference"
                            ));
                        }
                    }
                }
                Action::RunUntilPixel {
                    x,
                    y,
                    rgb,
                    not,
                    tolerance,
                    timeout_ticks,
                } => {
                    let mut waited = 0;
                    while !self.pixel(*x, *y, *rgb, *not, *tolerance)? {
                        if waited == *timeout_ticks {
                            return Err(format!(
                                "pixel wait timed out after {waited} frontend ticks"
                            ));
                        }
                        self.advance(1)?;
                        waited += 1;
                    }
                }
                Action::AssertPixel {
                    x,
                    y,
                    rgb,
                    not,
                    tolerance,
                } => {
                    self.report.assertions += 1;
                    if !self.pixel(*x, *y, *rgb, *not, *tolerance)? {
                        return Err(format!("pixel assertion failed at ({x},{y})"));
                    }
                }
                Action::AssertMemoryWord { address, value } => {
                    self.report.assertions += 1;
                    let actual = self.runner.bus_mut().read_word(*address);
                    if actual != *value {
                        return Err(format!(
                            "memory {address:#x}: expected {value:#x}, got {actual:#x}"
                        ));
                    }
                }
                Action::AssertAudioDuring {
                    ticks,
                    min_non_silent,
                } => {
                    self.report.assertions += 1;
                    let actual = self.advance(*ticks)?;
                    if actual < *min_non_silent {
                        return Err(format!("audio: expected at least {min_non_silent} non-silent samples, got {actual}"));
                    }
                }
                Action::Log { message } => eprintln!("[PLAY] {message}"),
                Action::Milestone { name } => eprintln!(
                    "[PLAY] milestone={name} guest_tick={}",
                    self.runner.guest_tick()
                ),
                _ => unreachable!(),
            }
        }
        self.report.failed_action = None;
        Ok(())
    }
}
fn pixel_matches(actual: [u8; 3], expected: [u8; 3], tolerance: u8) -> bool {
    actual
        .iter()
        .zip(expected)
        .all(|(a, b)| a.abs_diff(b) <= tolerance)
}
fn compare(frame: &RgbImage, path: &Path, tolerance: u8) -> Result<usize, String> {
    let expected = image::open(path)
        .map_err(|e| format!("reference {}: {e}", path.display()))?
        .to_rgb8();
    if frame.dimensions() != expected.dimensions() {
        return Err(format!(
            "reference {} has different dimensions",
            path.display()
        ));
    }
    Ok(frame
        .pixels()
        .zip(expected.pixels())
        .filter(|(a, b)| !pixel_matches(a.0, b.0, tolerance))
        .count())
}

#[allow(clippy::too_many_arguments)]
pub(super) fn run(
    archive: &Path,
    script_path: &Path,
    output: &Path,
    reference: Option<&Path>,
    start_time: u32,
    addressing_24_bit: bool,
    depth: Option<u16>,
    theme: UiThemeId,
) -> Result<(), String> {
    // Never reuse stale captures or modify adjacent saves during a reproduction.
    fs::create_dir_all(output).map_err(|e| e.to_string())?;
    if fs::read_dir(output)
        .map_err(|e| e.to_string())?
        .next()
        .is_some()
    {
        return Err("play output directory must be empty".into());
    }
    let mut report = Report {
        version: 1,
        systemless_version: env!("CARGO_PKG_VERSION").into(),
        archive: archive.display().to_string(),
        script: script_path.display().to_string(),
        reference_directory: reference.map(|p| p.display().to_string()),
        mac_time_secs: start_time,
        status: "failed".into(),
        ..Report::default()
    };
    let result = (|| {
        let script = Script::parse(&fs::read_to_string(script_path).map_err(|e| e.to_string())?)?;
        let mut runner = match depth {
            Some(d) => game::new_runner_with_configuration(!addressing_24_bit, d),
            None => game::new_runner_with_addressing(!addressing_24_bit),
        };
        runner.set_ui_theme(theme);
        runner.set_app_start_time(start_time);
        let app = game::load_game_from_path(&mut runner, archive)
            .map_err(|e| format!("load {}: {e}", archive.display()))?;
        game::init_game(&mut runner, &app);
        runner.prepare_text_presentation();
        configure_realtime_execution_rate(&mut runner);
        runner.composite_frame();
        report.architecture = Some(
            if runner.is_powerpc_app() {
                "ppc"
            } else {
                "68k"
            }
            .into(),
        );
        report.guest_tick = runner.guest_tick();
        let mut execution = Execution {
            runner,
            mouse: HostMouseReleaseLatch::default(),
            remainder: 0.0,
            max_ticks: script.max_ticks,
            report: &mut report,
        };
        let result = execution.actions(&script, output, reference);
        if result.is_err() {
            if let Ok(frame) = execution.image() {
                let _ = frame.save(output.join("failure.png"));
            }
        }
        result
    })();
    match &result {
        Ok(()) => report.status = "passed".into(),
        Err(e) => report.error = Some(e.clone()),
    }
    fs::write(
        output.join("report.json"),
        serde_json::to_vec_pretty(&report).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn cli_requires_headless_and_rejects_other_replay_clocks() {
        use super::super::Cli;
        use clap::Parser;
        assert!(
            Cli::try_parse_from(["systemless", "app.sit", "--play-script", "script.json"]).is_err()
        );
        assert!(Cli::try_parse_from([
            "systemless",
            "app.sit",
            "--headless",
            "--play-script",
            "script.json",
            "--max-ticks",
            "10"
        ])
        .is_err());
        assert!(Cli::try_parse_from([
            "systemless",
            "app.sit",
            "--headless",
            "--play-script",
            "script.json",
            "--play-output",
            "captures"
        ])
        .is_ok());
    }
    #[test]
    fn rejects_ambiguous_or_unsafe_scripts() {
        for text in [
            r#"{"actions":[]}"#,
            r#"{"version":1,"clock":"guest_ticks","max_ticks":1,"actions":[{"type":"run","ticks":1}]}"#,
            r#"{"version":1,"clock":"frontend_ticks","max_ticks":1,"actions":[{"type":"screenshot","name":"../escape"}]}"#,
            r#"{"version":1,"clock":"frontend_ticks","max_ticks":1,"actions":[{"type":"run","ticks":1,"instructions":100}]}"#,
        ] {
            assert!(Script::parse(text).is_err(), "{text}");
        }
    }
    #[test]
    fn reference_requires_exact_file_dimensions_and_pixels() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("title.png");
        let a = RgbImage::from_pixel(2, 2, image::Rgb([10, 20, 30]));
        assert!(compare(&a, &path, 0).is_err());
        let mut b = a.clone();
        b.put_pixel(1, 1, image::Rgb([11, 20, 30]));
        b.save(&path).unwrap();
        assert_eq!(compare(&a, &path, 0).unwrap(), 1);
        assert_eq!(compare(&a, &path, 1).unwrap(), 0);
        RgbImage::new(1, 1).save(&path).unwrap();
        assert!(compare(&a, &path, 0).is_err());
    }
}
