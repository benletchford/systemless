//! Shared debug overlay data and text formatting.

#[derive(Clone, Copy, Debug, Default)]
pub struct DebugOverlayFrameStats {
    pub host_fps: Option<f64>,
    pub frame_ms: Option<f64>,
    pub guest_mips: Option<f64>,
    pub guest_ticks_per_sec: Option<f64>,
    pub ticks_behind: Option<u32>,
    pub last_steps: Option<usize>,
    pub cpu_budget_ms: Option<f64>,
    pub audio_queue_ms: Option<f64>,
}

#[derive(Clone, Debug)]
pub struct DebugOverlaySnapshot {
    pub frame_stats: DebugOverlayFrameStats,
    pub guest_tick: u32,
    pub total_instructions: u64,
    pub trap_count: u64,
    pub game_trap_count: u64,
    pub cursor_visible: bool,
    pub cursor_level: i16,
    pub cursor_data_present: bool,
    pub cursor_mask_nonzero_bytes: Option<usize>,
    pub cursor_hotspot: Option<(i16, i16)>,
    pub cursor_position: (i16, i16),
    pub mouse_button: bool,
    pub fullscreen_locked: bool,
    pub mbar_height: u16,
    pub screen_width: u16,
    pub screen_height: u16,
    pub pixel_size: u16,
    pub front_window: u32,
    pub window_bounds: (i16, i16, i16, i16),
    pub window_count: usize,
    pub menu_count: usize,
    pub halted: bool,
    pub halted_trap: Option<u16>,
    pub halted_pc: Option<u32>,
}

impl DebugOverlaySnapshot {
    pub fn lines(&self) -> Vec<String> {
        let mut lines = vec![
            "systemless debug".to_string(),
            format!("host_fps {}", fmt_f64(self.frame_stats.host_fps, 1)),
            format!("frame_ms {}", fmt_f64(self.frame_stats.frame_ms, 1)),
            format!("guest_mips {}", fmt_f64(self.frame_stats.guest_mips, 1)),
            format!(
                "guest_ticks_per_sec {}",
                fmt_f64(self.frame_stats.guest_ticks_per_sec, 1)
            ),
            format!("guest_tick {}", self.guest_tick),
            format!("tick_debt {}", fmt_u32(self.frame_stats.ticks_behind)),
            format!("last_steps {}", fmt_usize(self.frame_stats.last_steps)),
            format!(
                "cpu_budget_ms {}",
                fmt_f64(self.frame_stats.cpu_budget_ms, 1)
            ),
            format!(
                "audio_queue_ms {}",
                fmt_f64(self.frame_stats.audio_queue_ms, 1)
            ),
            format!("instructions {}", self.total_instructions),
            format!("trap_count {}", self.trap_count),
            format!("game_trap_count {}", self.game_trap_count),
            format!("cursor_visible {}", bool_text(self.cursor_visible)),
            format!("cursor_level {}", self.cursor_level),
            format!(
                "cursor_data_present {}",
                bool_text(self.cursor_data_present)
            ),
            format!(
                "cursor_mask_bytes {}",
                fmt_usize(self.cursor_mask_nonzero_bytes)
            ),
            format!(
                "cursor_hotspot {}",
                self.cursor_hotspot
                    .map(|(v, h)| format!("{v},{h}"))
                    .unwrap_or_else(|| "n/a".to_string())
            ),
            format!(
                "cursor_position {},{}",
                self.cursor_position.0, self.cursor_position.1
            ),
            format!("mouse_button {}", bool_text(self.mouse_button)),
            format!("fullscreen_locked {}", bool_text(self.fullscreen_locked)),
            format!("mbar_height {}", self.mbar_height),
            format!(
                "screen {}x{}x{}",
                self.screen_width, self.screen_height, self.pixel_size
            ),
            format!("front_window {:08X}", self.front_window),
            format!(
                "window_bounds [{},{},{},{}]",
                self.window_bounds.0,
                self.window_bounds.1,
                self.window_bounds.2,
                self.window_bounds.3
            ),
            format!("window_count {}", self.window_count),
            format!("menu_count {}", self.menu_count),
            format!("halted {}", bool_text(self.halted)),
        ];

        if let Some(trap) = self.halted_trap {
            lines.push(format!("halted_trap {:04X}", trap));
        }
        if let Some(pc) = self.halted_pc {
            lines.push(format!("halted_pc {:08X}", pc));
        }
        lines
    }

    pub fn text(&self) -> String {
        self.lines().join("\n")
    }
}

fn bool_text(value: bool) -> &'static str {
    if value {
        "true"
    } else {
        "false"
    }
}

fn fmt_f64(value: Option<f64>, precision: usize) -> String {
    match value.filter(|value| value.is_finite()) {
        Some(value) => format!("{value:.precision$}"),
        None => "n/a".to_string(),
    }
}

fn fmt_u32(value: Option<u32>) -> String {
    value
        .map(|value| value.to_string())
        .unwrap_or_else(|| "n/a".to_string())
}

fn fmt_usize(value: Option<usize>) -> String {
    value
        .map(|value| value.to_string())
        .unwrap_or_else(|| "n/a".to_string())
}
