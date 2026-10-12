//! Embedding entry point for one Macintosh guest world.
//!
//! The session owns the existing runner. Loading, input and scheduling retain
//! their canonical implementations; no guest state is copied at this boundary.

use std::path::Path;

use crate::api::{
    AdvanceResult, AudioChunk, AudioFormat, InstructionBudget, PixelFormat, VideoFrame,
};
use crate::loader::LoadedApp;
use crate::runner::FixtureRunner;
use crate::systems::macintosh::{display, game};

/// Macintosh key values are guest key codes and character codes, not host keys.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MacintoshInput {
    MouseMove { vertical: i16, horizontal: i16 },
    MouseDown { vertical: i16, horizontal: i16 },
    MouseUp { vertical: i16, horizontal: i16 },
    KeyDown { mac_key: u8, character: u8 },
    KeyUp { mac_key: u8, character: u8 },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MacintoshStatus {
    pub loaded: bool,
    pub running: bool,
    pub guest_tick: u32,
    pub powerpc_application: bool,
}

/// One Macintosh application session, with a shared world for 68K and PPC.
pub struct MacintoshSession {
    runner: FixtureRunner,
    loaded: bool,
}

impl MacintoshSession {
    /// Use the canonical frontend machine configuration.
    pub fn new(addressing_32_bit: bool, screen_depth: Option<u16>) -> Self {
        let runner = match screen_depth {
            Some(depth) => game::new_runner_with_configuration(addressing_32_bit, depth),
            None => game::new_runner_with_addressing(addressing_32_bit),
        };
        Self {
            runner,
            loaded: false,
        }
    }

    /// Load a supported application container and its content from bytes.
    pub fn load_bytes(&mut self, bytes: &[u8]) -> Result<LoadedApp, String> {
        self.loaded = false;
        let app = game::load_game(&mut self.runner, bytes)?;
        self.loaded = true;
        Ok(app)
    }

    /// Load a supported application container and its content from a path.
    pub fn load_path(&mut self, path: &Path) -> Result<LoadedApp, String> {
        self.loaded = false;
        let app = game::load_game_from_path(&mut self.runner, path)?;
        self.loaded = true;
        Ok(app)
    }

    /// Complete canonical post-load initialization after optional save import.
    pub fn initialize(&mut self, app: &LoadedApp) {
        game::init_game(&mut self.runner, app);
    }

    /// Advance by at most the requested number of guest instructions.
    /// The runner's configured instruction cadence owns guest time.
    pub fn advance(&mut self, budget: InstructionBudget) -> AdvanceResult {
        if !self.loaded {
            return AdvanceResult {
                instructions: 0,
                running: false,
                guest_tick: self.runner.guest_tick(),
            };
        }
        let (instructions, running) = self.runner.run_steps(budget.0, None);
        AdvanceResult {
            instructions,
            running,
            guest_tick: self.runner.guest_tick(),
        }
    }

    pub fn deliver_input(&mut self, input: MacintoshInput) {
        match input {
            MacintoshInput::MouseMove {
                vertical,
                horizontal,
            } => self.runner.set_mouse_position(vertical, horizontal),
            MacintoshInput::MouseDown {
                vertical,
                horizontal,
            } => self.runner.push_mouse_down(vertical, horizontal),
            MacintoshInput::MouseUp {
                vertical,
                horizontal,
            } => self.runner.push_mouse_up(vertical, horizontal),
            MacintoshInput::KeyDown { mac_key, character } => {
                self.runner.push_key_down(mac_key, character)
            }
            MacintoshInput::KeyUp { mac_key, character } => {
                self.runner.push_key_up(mac_key, character)
            }
        }
    }

    /// Import changed host clipboard text as Macintosh Roman bytes with CR
    /// line endings, before requesting resume. Guest code owns private scrap
    /// conversion in response to the resulting resume notification.
    /// An already foreground application receives a conversion-only resume at
    /// an eligible guest yield; no suspend or window activation is synthesized.
    pub fn import_clipboard_text(&mut self, text: Vec<u8>) {
        self.runner.import_clipboard_text(text);
    }

    /// Global TEXT after the guest has finished suspend handling and yielded.
    /// Outer `None` denotes an unsettled/foreground process; inner `None`
    /// denotes a settled clipboard without TEXT. Bytes are Macintosh Roman.
    pub fn clipboard_text_after_suspend(&self) -> Option<Option<Vec<u8>>> {
        self.runner.clipboard_text_after_suspend()
    }

    /// Request a foreground change without bypassing the guest Event Manager.
    /// The request takes effect at an eligible scheduling call, respecting
    /// modality and the application's SIZE suspend/resume policy.
    pub fn request_foreground(&mut self, foreground: bool) {
        self.runner.request_foreground(foreground);
    }

    /// Composite host presentation and return an owned RGBA8 frame.
    pub fn video_frame(&mut self) -> Option<VideoFrame> {
        self.runner.composite_frame();
        let mode = self.runner.dispatcher().screen_mode;
        if mode.2 == 0 || mode.3 == 0 {
            return None;
        }
        let dispatcher = self.runner.dispatcher();
        let gamma = dispatcher.device_gamma();
        let pixels = display::render_screen_with_gamma(
            self.runner.bus(),
            mode,
            &dispatcher.device_clut,
            &gamma,
        );
        Some(VideoFrame {
            width: u32::from(mode.2),
            height: u32::from(mode.3),
            format: PixelFormat::Rgba8,
            pixels,
        })
    }

    /// Drain unsigned 8-bit mono PCM at 22,050 Hz; silence is 0x80.
    pub fn drain_audio(&mut self) -> AudioChunk {
        AudioChunk {
            format: AudioFormat::Unsigned8BitMono {
                sample_rate_hz: 22_050,
            },
            samples: self.runner.drain_audio(),
        }
    }

    pub fn status(&self) -> MacintoshStatus {
        MacintoshStatus {
            loaded: self.loaded,
            running: self.loaded && !self.runner.is_halted(),
            guest_tick: self.runner.guest_tick(),
            powerpc_application: self.runner.is_powerpc_app(),
        }
    }

    /// Access richer Macintosh operations during incremental frontend migration.
    pub fn runner(&self) -> &FixtureRunner {
        &self.runner
    }

    /// Access richer Macintosh operations during incremental frontend migration.
    pub fn runner_mut(&mut self) -> &mut FixtureRunner {
        &mut self.runner
    }
}
