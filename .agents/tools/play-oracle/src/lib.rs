use serde::Deserialize;
pub mod docker;
pub mod macbinary;
pub mod mpw_builder;
pub const DEFAULT_PLAY_MAC_TIME_SECS: u32 = 3_871_497_600;
pub fn play_mac_time_secs() -> u32 {
    std::env::var("SYSTEMLESS_PLAY_MAC_TIME")
        .ok()
        .and_then(|value| value.parse::<u32>().ok())
        .unwrap_or(DEFAULT_PLAY_MAC_TIME_SECS)
}

pub fn play_mac_time_secs_for_script(script: &PlayScript) -> u32 {
    script.mac_time_secs.unwrap_or_else(play_mac_time_secs)
}

pub fn basilisk_play_mac_time_secs_for_script(script: &PlayScript) -> u32 {
    script
        .basilisk_mac_time_secs
        .or(script.mac_time_secs)
        .unwrap_or_else(play_mac_time_secs)
}

pub fn sheepshaver_play_mac_time_secs_for_script(script: &PlayScript) -> u32 {
    script
        .sheepshaver_mac_time_secs
        .or(script.mac_time_secs)
        .unwrap_or_else(play_mac_time_secs)
}

pub fn realtime_instructions_per_tick() -> u32 {
    let mhz: f64 = std::env::var("SYSTEMLESS_CPU_MHZ")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(systemless::runner::DEFAULT_REALTIME_CPU_MHZ);
    (mhz * 1_000_000.0 / systemless::runner::DEFAULT_VBL_HZ).round() as u32
}

pub const STRICT_ORACLE_ENV: &str = "SYSTEMLESS_ORACLE_STRICT";

pub fn strict_oracle_run() -> Result<bool, String> {
    match std::env::var(STRICT_ORACLE_ENV) {
        Ok(value) => parse_strict_switch(&value),
        Err(std::env::VarError::NotPresent) => Ok(false),
        Err(error) => Err(format!("read {STRICT_ORACLE_ENV}: {error}")),
    }
}

pub fn reject_oracle_environment_overrides(names: &[&str]) -> Result<(), String> {
    let conflicts: Vec<&str> = names
        .iter()
        .copied()
        .filter(|name| std::env::var_os(name).is_some())
        .collect();
    if conflicts.is_empty() {
        Ok(())
    } else {
        Err(format!(
            "strict oracle run rejects environment override(s): {}",
            conflicts.join(", ")
        ))
    }
}

fn parse_strict_switch(value: &str) -> Result<bool, String> {
    match value.trim().to_ascii_lowercase().as_str() {
        "1" | "true" => Ok(true),
        "0" | "false" => Ok(false),
        _ => Err(format!(
            "{STRICT_ORACLE_ENV} must be one of 1, true, 0, or false"
        )),
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PlayScript {
    pub version: Option<u32>,
    pub clock: Option<String>,
    pub mac_time_secs: Option<u32>,
    pub basilisk_mac_time_secs: Option<u32>,
    pub sheepshaver_mac_time_secs: Option<u32>,
    pub boot_delay_ticks: Option<u32>,
    pub executable: Option<String>,
    pub patch_bytes: Option<String>,
    pub application_partition_size: Option<u32>,
    #[serde(default)]
    pub basilisk_stage_to_boot_disk: bool,
    #[serde(default)]
    pub sheepshaver_stage_to_boot_disk: bool,
    pub sheepshaver_screen_width: Option<u32>,
    pub sheepshaver_screen_height: Option<u32>,
    pub sheepshaver_color_depth: Option<u32>,
    pub remove_paths: Option<Vec<String>>,
    pub prelaunch_create_dirs: Option<Vec<String>>,
    pub prelaunch_delete_paths: Option<Vec<String>>,
    pub actions: Vec<Action>,
}

#[derive(Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum Action {
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
    RecordFrames {
        #[serde(default)]
        path: Option<String>,
        #[serde(default)]
        instructions: Option<u64>,
        #[serde(default)]
        ticks: Option<u32>,
        #[serde(default)]
        basilisk_ticks: Option<u32>,
        #[serde(default)]
        chunk_instructions: Option<usize>,
        #[serde(default)]
        max_frames: Option<usize>,
        #[serde(default)]
        contact_sheet: Option<String>,
        #[serde(default)]
        label: Option<String>,
    },
    Log {
        message: String,
    },
    Milestone {
        name: String,
    },
    AssertTickRange {
        min: u32,
        max: u32,
        label: Option<String>,
    },
    AssertPixel {
        x: u32,
        y: u32,
        rgb: [u8; 3],
        #[serde(default)]
        not: bool,
        label: Option<String>,
    },
    AssertMemoryWord {
        address: u32,
        value: u16,
        label: Option<String>,
    },
    AssertAudio {
        samples: usize,
        min_non_silent: usize,
        #[serde(default)]
        label: Option<String>,
    },
    AssertAudioDuring {
        ticks: u32,
        min_non_silent: usize,
        #[serde(default)]
        label: Option<String>,
    },
}

pub fn pixel_matches_within(actual: [u8; 3], expected: [u8; 3], not: bool, tolerance: u8) -> bool {
    if tolerance == 0 {
        let matches = actual == expected;
        return if not { !matches } else { matches };
    }
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

pub fn container_runtime() -> String {
    std::env::var("CONTAINER_RUNTIME").unwrap_or_else(|_| "docker".into())
}
pub fn configure_load_from_script(script: &PlayScript) {
    if let Some(name) = &script.executable {
        std::env::set_var("SYSTEMLESS_LOAD_EXECUTABLE", name);
    }
}
pub fn validate_script(text: &str, script: &PlayScript) -> Result<(), String> {
    let value: serde_json::Value = serde_json::from_str(text).map_err(|e| e.to_string())?;
    if !matches!(value["clock"].as_str(), Some("guest_ticks" | "wall_time"))
        || value["version"] != 1
    {
        return Err("oracle scripts require version=1 and clock=guest_ticks or wall_time; frontend tick scripts must be adapted explicitly".into());
    }
    if script.patch_bytes.is_some() {
        return Err("byte patches are not supported by oracle capture tools".into());
    }
    if script.actions.is_empty() {
        return Err("oracle actions must be nonempty".into());
    }
    let wall_time = value["clock"] == "wall_time";
    if wall_time && script.boot_delay_ticks.is_some() {
        return Err("boot_delay_ticks requires guest_ticks".into());
    }
    for action in &script.actions {
        if wall_time
            && matches!(
                action,
                Action::RunUntilTick { .. }
                    | Action::AssertTickRange { .. }
                    | Action::MouseMove {
                        at_tick: Some(_),
                        ..
                    }
                    | Action::MouseDown {
                        at_tick: Some(_),
                        ..
                    }
                    | Action::MouseUp {
                        at_tick: Some(_),
                        ..
                    }
                    | Action::KeyDown {
                        at_tick: Some(_),
                        ..
                    }
                    | Action::KeyUp {
                        at_tick: Some(_),
                        ..
                    }
                    | Action::Screenshot {
                        at_tick: Some(_),
                        ..
                    }
            )
        {
            return Err(
                "guest-tick scheduling/assertions are unavailable in wall_time mode".into(),
            );
        }

        match action {
            Action::AssertMemoryWord { .. }
            | Action::AssertAudio { .. }
            | Action::AssertAudioDuring { .. }
            | Action::RecordFrames { .. } => {
                return Err("this action is not supported by the capture adapters".into())
            }
            Action::Run { ticks: None, .. }
            | Action::Run {
                instructions: Some(_),
                ..
            } => return Err("oracle run requires only ticks".into()),
            Action::Screenshot {
                path: Some(path), ..
            } => {
                if path.is_empty()
                    || !path
                        .bytes()
                        .all(|b| b.is_ascii_alphanumeric() || b"._-".contains(&b))
                    || path == "."
                    || path == ".."
                {
                    return Err("capture path must be a plain filename using letters, digits, dots, underscores or hyphens".into());
                }
            }
            _ => {}
        }
    }
    Ok(())
}
use std::path::Path;

pub fn install_bootstrap(
    manifest: &Path,
    scratch: &Path,
    disk_name: &str,
    image: &str,
) -> Result<(), String> {
    let output = scratch.join("bootstrap_output");
    mpw_builder::compile_with_mpw(
        "bootstrap",
        &manifest.join("support/launcher"),
        &output,
        &scratch.join("bootstrap_work"),
    );
    let status = std::process::Command::new("docker").args(["run", "--rm", "--entrypoint", "/bin/bash", "-v"])
        .arg(format!("{}:/mnt/disk.dsk", scratch.join("shared/disk").join(disk_name).display()))
        .arg("-v").arg(format!("{}:/mnt/launcher.bin:ro", output.join("FixtureGen.bin").display()))
        .arg(image).args(["-c", "hmount /mnt/disk.dsk && hcd ':System Folder:' && (hmkdir 'Startup Items' 2>/dev/null || true) && hcd 'Startup Items' && (hdel Launcher 2>/dev/null || true) && hcopy -m /mnt/launcher.bin :Launcher && humount"])
        .status().map_err(|e| e.to_string())?;
    if !status.success() {
        return Err("could not install startup launcher into scratch disk".into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn preflight_rejects_false_evidence_and_unsafe_capture_paths() {
        for action in [
            r#"{"type":"assert_audio","samples":10,"min_non_silent":1}"#,
            r#"{"type":"assert_memory_word","address":0,"value":0}"#,
            r#"{"type":"record_frames"}"#,
            r#"{"type":"screenshot","path":"../escape.png"}"#,
        ] {
            let text = format!(r#"{{"version":1,"clock":"guest_ticks","actions":[{action}]}}"#);
            let script: PlayScript = serde_json::from_str(&text).unwrap();
            assert!(validate_script(&text, &script).is_err());
        }
        let text = r#"{"version":1,"clock":"wall_time","actions":[{"type":"assert_tick_range","min":0,"max":20}]}"#;
        let script = serde_json::from_str(text).unwrap();
        assert!(validate_script(text, &script).is_err());
        let text = r#"{"version":1,"clock":"wall_time","actions":[{"type":"run","ticks":30},{"type":"screenshot","path":"title.png"}]}"#;
        let script = serde_json::from_str(text).unwrap();
        assert!(validate_script(text, &script).is_ok());
        let text = r#"{"version":1,"clock":"frontend_ticks","actions":[{"type":"run","ticks":1}]}"#;
        let script = serde_json::from_str(text).unwrap();
        assert!(validate_script(text, &script).is_err());
    }
}
