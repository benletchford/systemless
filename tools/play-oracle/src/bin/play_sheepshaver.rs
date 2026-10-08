//! Play script runner targeting SheepShaver inside a Docker container.
//!
//! Drives the same `PlayScript` JSON files as the Systemless runner (`play`)
//! against a real-Mac SheepShaver PowerPC environment so the two runners can be
//! compared screenshot-for-screenshot. Intended as the ground-truth
//! reference for Systemless's PowerPC HLE — whenever Systemless's output disagrees with
//! SheepShaver's, the SheepShaver output wins and a contract test can be authored.
//!
//! Usage: cargo run --bin play-sheepshaver -- <game.sit> <script.json> [output-directory]
//!
//! Requirements:
//!   * `docker` on PATH with the `sheepshaver-play:latest` image, typically
//!     built via `support/sheepshaver/build_play_image.sh`
//!   * optional: set `SYSTEMLESS_SHEEPSHAVER_IMAGE` to use a non-default image tag
//!   * `unar` on PATH (for resource-fork-preserving .sit extraction)
//!
//! Outputs screenshots to `<script_dir>/sheepshaver_output/` (parallel to the
//! Systemless `<script_dir>/output/` and `basilisk_output/` directories).

use std::collections::HashSet;
use std::io::Read;
use std::path::Component;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant};

use systemless_oracle_tools::docker::{build_mpw_builder_image, docker_mount_source};
use systemless_oracle_tools::mpw_builder::compile_with_mpw;
use systemless_oracle_tools::{
    pixel_matches_within, realtime_instructions_per_tick, reject_oracle_environment_overrides,
    sheepshaver_play_mac_time_secs_for_script, strict_oracle_run, Action, PlayScript,
};

const DEFAULT_DOCKER_IMAGE: &str = "sheepshaver-play:latest";
const DEFAULT_DOCKER_CPUS: &str = "2.0";
const CONTAINER_NAME: &str = "systemless_sheepshaver_play";
const PRELAUNCH_CREATE_DIRS_FILE: &str = "_prelaunch_create_dirs";
const PRELAUNCH_DELETE_PATHS_FILE: &str = "_prelaunch_delete_paths";
const PRELAUNCH_BOOT_COPY_MANIFEST_FILE: &str = "_prelaunch_boot_copy_manifest";
const LAUNCH_STATUS_FILE: &str = "_launch_status";
const LAUNCH_READY_FILE: &str = "_launch_ready";
const BOOT_WAIT_SECS: u64 = 600;
const TICK_PROGRESS_TIMEOUT_SECS: u64 = 45;
const POLL_INTERVAL_MILLIS: u64 = 50;
const CLOCK_POLL_INTERVAL_MILLIS: u64 = 2;

struct ScratchLayout {
    root: PathBuf,
    extfs_dir: PathBuf,
    probe_shot: PathBuf,
    trace_path: PathBuf,
    clock_path: PathBuf,
}

struct TimingContext {
    wall_time: bool,
    clock_path: PathBuf,
    tick_zero: u32,
    probe_shot: PathBuf,
    _trace_path: PathBuf,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct OracleClockSample {
    tick: u32,
    retired_instructions: u64,
}

struct OracleClockProgress {
    last_sample: OracleClockSample,
    deadline: Instant,
    timeout: Duration,
}

impl OracleClockProgress {
    fn new(sample: OracleClockSample, now: Instant, timeout: Duration) -> Self {
        Self {
            last_sample: sample,
            deadline: now + timeout,
            timeout,
        }
    }

    fn observe(&mut self, sample: OracleClockSample, now: Instant) {
        if sample.tick > self.last_sample.tick
            && sample.retired_instructions > self.last_sample.retired_instructions
        {
            self.last_sample = sample;
            self.deadline = now + self.timeout;
        }
    }

    fn expired(&self, now: Instant) -> bool {
        now >= self.deadline
    }
}

#[derive(Default)]
struct ScriptState {
    reuse_probe_capture: bool,
    probe_clock_sample: Option<OracleClockSample>,
}

#[derive(Debug, Eq, PartialEq)]
struct ScriptActionFailure {
    index: usize,
    message: String,
}

fn execute_script_actions<F>(
    actions: &[Action],
    mut execute: F,
) -> Result<usize, ScriptActionFailure>
where
    F: FnMut(usize, &Action) -> Result<(), String>,
{
    for (index, action) in actions.iter().enumerate() {
        if let Err(message) = execute(index, action) {
            return Err(ScriptActionFailure { index, message });
        }
    }
    Ok(actions.len())
}

fn docker_image() -> String {
    std::env::var("SYSTEMLESS_SHEEPSHAVER_IMAGE")
        .ok()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| DEFAULT_DOCKER_IMAGE.to_string())
}

fn resolved_sheepshaver_mac_time(host_value: Option<String>, fallback_secs: u32) -> String {
    host_value
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| fallback_secs.to_string())
}

fn resolved_sheepshaver_deterministic_ticks(host_value: Option<String>) -> String {
    host_value
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| "1".to_string())
}

fn resolved_sheepshaver_instructions_per_tick(host_value: Option<String>) -> String {
    host_value
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| realtime_instructions_per_tick().to_string())
}

fn reject_strict_script_overrides(strict_profile: bool, script: &PlayScript) -> Result<(), String> {
    if strict_profile
        && (script.sheepshaver_screen_width.is_some()
            || script.sheepshaver_screen_height.is_some()
            || script.sheepshaver_color_depth.is_some())
    {
        Err("strict oracle run rejects SheepShaver display overrides".to_string())
    } else {
        Ok(())
    }
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 3 {
        eprintln!(
            "Usage: {} <game.sit> <script.json> [output-directory]",
            args[0]
        );
        std::process::exit(1);
    }
    let strict_profile = strict_oracle_run().expect("validate oracle strict mode");
    if strict_profile {
        reject_oracle_environment_overrides(&[
            "SYSTEMLESS_CPU_MHZ",
            "SYSTEMLESS_PLAY_MAC_TIME",
            "SYSTEMLESS_SHEEPSHAVER_CPUS",
            "SYSTEMLESS_SHEEPSHAVER_DETERMINISTIC_TICKS",
            "SYSTEMLESS_SHEEPSHAVER_DISK",
            "SYSTEMLESS_SHEEPSHAVER_IMAGE",
            "SYSTEMLESS_SHEEPSHAVER_INSTRUCTIONS_PER_TICK",
            "SYSTEMLESS_SHEEPSHAVER_MAC_TIME",
            "SYSTEMLESS_SHEEPSHAVER_ROM",
        ])
        .expect("reject oracle SheepShaver overrides");
    }
    let game_path = PathBuf::from(&args[1]).canonicalize().expect("game path");
    let script_path = PathBuf::from(&args[2]).canonicalize().expect("script path");

    let script_text = std::fs::read_to_string(&script_path).expect("read script");
    let script: PlayScript = serde_json::from_str(&script_text).expect("parse script");
    reject_strict_script_overrides(strict_profile, &script)
        .expect("reject oracle SheepShaver script overrides");
    systemless_oracle_tools::validate_script(&script_text, &script)
        .expect("validate oracle script");
    let play_time_secs = sheepshaver_play_mac_time_secs_for_script(&script);
    systemless_oracle_tools::configure_load_from_script(&script);

    let script_dir = script_path.parent().expect("script dir");
    let output_dir = args
        .get(3)
        .map(PathBuf::from)
        .unwrap_or_else(|| script_dir.join("sheepshaver_output"));
    if output_dir.exists()
        && std::fs::read_dir(&output_dir)
            .expect("read output")
            .next()
            .is_some()
    {
        eprintln!("Oracle output directory must be empty");
        std::process::exit(1);
    }
    std::fs::create_dir_all(&output_dir).expect("create sheepshaver_output dir");

    let scratch = setup_scratch(
        &game_path,
        script.executable.as_deref(),
        play_time_secs,
        script.sheepshaver_screen_width.unwrap_or(800),
        script.sheepshaver_screen_height.unwrap_or(600),
        script.sheepshaver_color_depth.unwrap_or(8),
        strict_profile,
    )
    .expect("stage scratch dir");
    if let Some(paths) = script.remove_paths.as_deref() {
        apply_remove_paths(&scratch.extfs_dir, paths).expect("remove_paths");
    }
    apply_application_partition_size(&scratch.extfs_dir, script.application_partition_size)
        .expect("application_partition_size");
    stage_prelaunch_create_dirs(&scratch.extfs_dir, script.prelaunch_create_dirs.as_deref())
        .expect("prelaunch_create_dirs");
    stage_prelaunch_delete_paths(&scratch.extfs_dir, script.prelaunch_delete_paths.as_deref())
        .expect("prelaunch_delete_paths");
    let stage_to_boot = script.sheepshaver_stage_to_boot_disk || script.basilisk_stage_to_boot_disk;
    stage_boot_disk_copy_manifest(&scratch.extfs_dir, stage_to_boot).expect("stage_to_boot_disk");
    if stage_to_boot {
        stage_play_target_to_boot_disk(&scratch).expect("stage _PlayTarget to boot disk");
    }

    systemless_oracle_tools::install_bootstrap(
        Path::new(env!("CARGO_MANIFEST_DIR")),
        &scratch.root,
        "System_PPC.dsk",
        &docker_image(),
    )
    .expect("install startup launcher");
    let _guard =
        ContainerGuard::start(&scratch.root, play_time_secs).expect("start sheepshaver container");
    if script_uses_keypad_digit_alias(&script) {
        prime_x11_numlock_for_keypad_aliases().expect("prime X11 NumLock for keypad aliases");
    }

    let tick_baseline_path = scratch.extfs_dir.join("_tick_baseline");
    eprintln!(
        "[PS] Waiting up to {}s for MacOS PPC boot + PlayLauncher baseline…",
        BOOT_WAIT_SECS
    );
    let tick_baseline = wait_for_u32_file_with_clock_progress(
        &tick_baseline_path,
        &scratch.clock_path,
        Duration::from_secs(BOOT_WAIT_SECS),
        "_tick_baseline",
    )
    .expect("tick baseline");
    capture_shot(&output_dir, "00_post_boot.png", &scratch.clock_path).expect("initial shot");
    eprintln!("[PS] Launcher baseline tick: {}", tick_baseline);
    wait_for_file(
        &scratch.extfs_dir.join(LAUNCH_READY_FILE),
        Duration::from_secs(BOOT_WAIT_SECS),
        LAUNCH_READY_FILE,
    )
    .expect("launch ready");
    std::thread::sleep(Duration::from_secs(2));
    if let Ok(bytes) = std::fs::read(scratch.extfs_dir.join(LAUNCH_STATUS_FILE)) {
        if bytes.len() >= 4 {
            let launch_status = u32::from_be_bytes(bytes[..4].try_into().unwrap());
            if launch_status != 0 {
                panic!(
                    "PlayLauncher could not launch _PlayTarget (Mac OS error {})",
                    launch_status as i32
                );
            }
        }
    }
    let boot_delay_ticks = script.boot_delay_ticks.unwrap_or(0);
    let tick_zero = tick_baseline.wrapping_add(boot_delay_ticks);
    let live_clock = wait_for_live_oracle_clock(&scratch.clock_path, tick_baseline)
        .expect("probe SheepShaver guest clock");
    let wall_time = script.clock.as_deref() == Some("wall_time");
    if !live_clock && !wall_time {
        panic!("No live SheepShaver guest clock; refusing inferred oracle timing");
    }
    eprintln!(
        "[PS] Capture pacing: {}",
        if wall_time {
            "explicit wall time; guest timing unavailable"
        } else {
            "live guest clock"
        }
    );
    if boot_delay_ticks > 0 {
        eprintln!(
            "[PS] Burning {} loader tick(s) to align script tick 0",
            boot_delay_ticks
        );
        wait_until_mac_tick(&scratch.clock_path, tick_zero).expect("boot delay burn");
    }
    capture_shot(&output_dir, "01_post_launch.png", &scratch.clock_path).ok();

    let timing = TimingContext {
        wall_time,
        clock_path: scratch.clock_path.clone(),
        tick_zero,
        probe_shot: scratch.probe_shot.clone(),
        _trace_path: scratch.trace_path.clone(),
    };

    let mut script_state = ScriptState::default();
    match execute_script_actions(&script.actions, |index, action| {
        execute_action(index, action, &output_dir, &timing, &mut script_state)
    }) {
        Ok(completed) => {
            eprintln!("[PS] Script complete after {} action(s)", completed);
        }
        Err(failure) => {
            eprintln!(
                "[PS] Script failed at action {}: {}",
                failure.index, failure.message
            );
            drop(_guard);
            std::process::exit(1);
        }
    }
}

/// Prepare a scratch directory with:
///   scratch/shared/rom/PowerMac.rom
///   scratch/shared/disk/System_PPC.dsk
///   scratch/extfs/_PlayTarget + FixtureGen + launcher control files
///   scratch/container_home/.sheepshaver_prefs
fn setup_scratch(
    game_path: &Path,
    executable_override: Option<&str>,
    play_time_secs: u32,
    screen_width: u32,
    screen_height: u32,
    color_depth: u32,
    strict_profile: bool,
) -> Result<ScratchLayout, String> {
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let scratch = tempfile::Builder::new()
        .prefix("systemless-sheepshaver-")
        .tempdir()
        .map_err(|e| e.to_string())?
        .keep();
    remove_existing_scratch(&scratch)?;
    std::fs::create_dir_all(scratch.join("shared/rom")).map_err(|e| format!("mkdir rom: {}", e))?;
    std::fs::create_dir_all(scratch.join("shared/disk"))
        .map_err(|e| format!("mkdir disk: {}", e))?;
    std::fs::create_dir_all(scratch.join("extfs")).map_err(|e| format!("mkdir extfs: {}", e))?;
    std::fs::create_dir_all(scratch.join("container_home"))
        .map_err(|e| format!("mkdir container home: {}", e))?;
    std::fs::create_dir_all(scratch.join("launcher_play_work/launcher_play"))
        .map_err(|e| format!("mkdir launcher work sub: {}", e))?;
    std::fs::create_dir_all(scratch.join("launcher_play_output"))
        .map_err(|e| format!("mkdir launcher output: {}", e))?;

    let rom_src = PathBuf::from(
        std::env::var_os("SYSTEMLESS_SHEEPSHAVER_ROM")
            .ok_or("set SYSTEMLESS_SHEEPSHAVER_ROM to your ROM file")?,
    );
    std::fs::copy(&rom_src, scratch.join("shared/rom/PowerMac.rom")).map_err(|e| e.to_string())?;
    let disk_src = PathBuf::from(
        std::env::var_os("SYSTEMLESS_SHEEPSHAVER_DISK")
            .ok_or("set SYSTEMLESS_SHEEPSHAVER_DISK to your system disk")?,
    );
    let disk_out = scratch.join("shared/disk/System_PPC.dsk");
    if disk_src.extension().and_then(|s| s.to_str()) == Some("gz") {
        let mut decoder = flate2::read::GzDecoder::new(
            std::fs::File::open(&disk_src).map_err(|e| e.to_string())?,
        );
        let mut output = std::fs::File::create(&disk_out).map_err(|e| e.to_string())?;
        std::io::copy(&mut decoder, &mut output).map_err(|e| e.to_string())?;
    } else {
        std::fs::copy(&disk_src, &disk_out).map_err(|e| e.to_string())?;
    }

    // Extract game archive via unar
    let extfs_dir = scratch.join("extfs");
    let status = Command::new("unar")
        .arg("-force-overwrite")
        .arg("-output-directory")
        .arg(&extfs_dir)
        .arg(game_path)
        .status()
        .map_err(|e| format!("unar: {}", e))?;
    if !status.success() {
        return Err("unar failed".into());
    }

    let nested_disk_images = extract_nested_disk_images(&extfs_dir)?;

    let mut used_loaded_archive_staging = false;
    if nested_disk_images > 0 {
        if let Some(wanted) = executable_override {
            let candidates = collect_appl_candidates(&extfs_dir, true)?;
            if !candidates
                .iter()
                .any(|path| candidate_matches_executable(path, &extfs_dir, wanted))
            {
                materialize_loaded_archive(&extfs_dir, game_path)?;
                used_loaded_archive_staging = true;
            }
        }
    }

    if !used_loaded_archive_staging {
        materialize_resource_only_data_forks(&extfs_dir)?;
    }

    flatten_and_rename_main_app(&extfs_dir, "_PlayTarget", executable_override)?;

    if !used_loaded_archive_staging {
        convert_xattrs_to_basilisk_sidecars(&extfs_dir)?;
    }
    ensure_play_target_finf(&extfs_dir)?;
    stage_play_launcher(&manifest, &scratch, &extfs_dir)?;
    write_u32_be_file(&extfs_dir.join("_play_time"), play_time_secs)?;

    // Stage prefs
    let prefs_src = manifest
        .join("support/sheepshaver")
        .join(".sheepshaver_prefs.play");
    let prefs_raw = if prefs_src.exists() {
        std::fs::read_to_string(&prefs_src).map_err(|e| format!("read prefs: {}", e))?
    } else {
        include_str!("../../support/sheepshaver/.sheepshaver_prefs.play").to_string()
    };
    let prefs = rewrite_play_prefs(
        &prefs_raw,
        screen_width,
        screen_height,
        color_depth,
        strict_profile,
    );
    std::fs::write(scratch.join("container_home/.sheepshaver_prefs"), prefs)
        .map_err(|e| format!("write prefs: {}", e))?;

    eprintln!("[PS] Scratch staged at {}", scratch.display());
    Ok(ScratchLayout {
        root: scratch.clone(),
        extfs_dir: extfs_dir.clone(),
        probe_shot: scratch.join("probe_pixel.png"),
        trace_path: scratch.join("shared/sheepshaver_trace.jsonl"),
        // The guest launcher writes the same big-endian `_ticks` stream used
        // by the BasiliskII flow.  This remains available on stock
        // SheepShaver builds and keeps script timing independent of an
        // emulator-specific host clock patch.
        clock_path: extfs_dir.join("_ticks"),
    })
}

fn remove_existing_scratch(scratch: &Path) -> Result<(), String> {
    match std::fs::remove_dir_all(scratch) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(format!(
            "remove stale scratch directory {}: {}",
            scratch.display(),
            error
        )),
    }
}

fn rewrite_play_prefs(
    prefs_raw: &str,
    screen_width: u32,
    screen_height: u32,
    color_depth: u32,
    strict_profile: bool,
) -> String {
    let mut saw_display_depth = false;
    let mut lines = Vec::new();

    for line in prefs_raw.lines() {
        let trimmed = line.trim_start();
        if trimmed.starts_with("screen ") {
            lines.push(format!("screen win/{screen_width}/{screen_height}"));
        } else if trimmed.starts_with("displaycolordepth ") {
            saw_display_depth = true;
            lines.push(format!("displaycolordepth {color_depth}"));
        } else if strict_profile
            && (trimmed.starts_with("ignoresegv ")
                || trimmed.starts_with("ignoreillegal ")
                || trimmed.starts_with("jit68k "))
        {
            let key = trimmed.split_whitespace().next().unwrap();
            lines.push(format!("{key} false"));
        } else {
            lines.push(line.to_string());
        }
    }

    if !saw_display_depth {
        lines.push(format!("displaycolordepth {color_depth}"));
    }

    lines.join("\n")
}

fn extract_nested_disk_images(root: &Path) -> Result<usize, String> {
    let mut candidates = Vec::new();
    let mut stack = vec![root.to_path_buf()];

    while let Some(dir) = stack.pop() {
        for entry in
            std::fs::read_dir(&dir).map_err(|e| format!("read {}: {}", dir.display(), e))?
        {
            let entry = entry.map_err(|e| format!("entry in {}: {}", dir.display(), e))?;
            let path = entry.path();
            let file_type = entry
                .file_type()
                .map_err(|e| format!("file_type {}: {}", path.display(), e))?;
            if file_type.is_symlink() {
                return Err(format!(
                    "refusing symlink in extracted archive: {}",
                    path.display()
                ));
            }
            if file_type.is_dir() {
                let name = entry.file_name();
                let name = name.to_string_lossy();
                if name != ".finf" && name != ".rsrc" {
                    stack.push(path);
                }
            } else if file_type.is_file() && !is_rsrc_companion(&path) {
                candidates.push(path);
            }
        }
    }

    let mut extracted = 0usize;
    for image_path in candidates {
        // Self-mounting Disk Copy images are applications whose data fork also
        // contains an HFS image. Launch the APPL wrapper in the guest; treating
        // its data fork as a plain nested image can produce an empty volume and
        // discard the executable installer stub.
        if read_finder_info(&image_path)?
            .is_some_and(|finder_info| finder_info.starts_with(b"APPL"))
        {
            continue;
        }
        let bytes = std::fs::read(&image_path)
            .map_err(|e| format!("read possible disk image {}: {}", image_path.display(), e))?;
        let Some(image) = systemless::disk_image::extract_dc42_or_hfs(&bytes)
            .map_err(|e| format!("extract nested disk image {}: {}", image_path.display(), e))?
        else {
            continue;
        };
        let parent = image_path
            .parent()
            .ok_or_else(|| format!("disk image has no parent: {}", image_path.display()))?;
        let volume_name = image.volume_name.clone();
        let file_count = image.files.len();
        materialize_disk_image(parent, image)?;
        std::fs::remove_file(&image_path)
            .map_err(|e| format!("remove staged disk image {}: {}", image_path.display(), e))?;
        let companion_rsrc = sidecar_rsrc_path(&image_path);
        if companion_rsrc.is_file() {
            std::fs::remove_file(&companion_rsrc).map_err(|e| {
                format!(
                    "remove disk-image resource companion {}: {}",
                    companion_rsrc.display(),
                    e
                )
            })?;
        }
        eprintln!(
            "[PS] Extracted nested disk image {} as volume {:?} ({} files)",
            image_path.display(),
            volume_name,
            file_count
        );
        extracted += 1;
    }

    Ok(extracted)
}

fn materialize_disk_image(
    root: &Path,
    image: systemless::disk_image::DiskImageContents,
) -> Result<(), String> {
    for dir in image.dirs {
        let relative = validated_archive_relative_path(&dir)?;
        let path = root.join(relative);
        if path
            .symlink_metadata()
            .is_ok_and(|metadata| !metadata.is_dir())
        {
            return Err(format!(
                "disk-image directory collides with non-directory {}",
                path.display()
            ));
        }
        std::fs::create_dir_all(&path)
            .map_err(|e| format!("create disk-image directory {}: {}", path.display(), e))?;
    }

    for file in image.files {
        let relative = validated_archive_relative_path(&file.path)?;
        let path = root.join(relative);
        if path.exists() {
            return Err(format!(
                "disk-image file collides with existing archive entry {}",
                path.display()
            ));
        }
        let parent = path
            .parent()
            .ok_or_else(|| format!("disk-image file has no parent: {}", path.display()))?;
        std::fs::create_dir_all(parent)
            .map_err(|e| format!("create disk-image parent {}: {}", parent.display(), e))?;
        std::fs::write(&path, file.data)
            .map_err(|e| format!("write disk-image data fork {}: {}", path.display(), e))?;

        let finf_path = extfs_sidecar_path(&path, ".finf")?;
        let finf_parent = finf_path
            .parent()
            .ok_or_else(|| format!("Finder-info sidecar has no parent: {}", finf_path.display()))?;
        std::fs::create_dir_all(finf_parent)
            .map_err(|e| format!("create {}: {}", finf_parent.display(), e))?;
        let mut finf = [0u8; 32];
        finf[0..4].copy_from_slice(&file.file_type);
        finf[4..8].copy_from_slice(&file.creator);
        finf[8..10].copy_from_slice(&file.finder_flags.to_be_bytes());
        std::fs::write(&finf_path, finf)
            .map_err(|e| format!("write Finder-info sidecar {}: {}", finf_path.display(), e))?;

        if !file.rsrc.is_empty() {
            let rsrc_path = extfs_sidecar_path(&path, ".rsrc")?;
            let rsrc_parent = rsrc_path.parent().ok_or_else(|| {
                format!(
                    "resource-fork sidecar has no parent: {}",
                    rsrc_path.display()
                )
            })?;
            std::fs::create_dir_all(rsrc_parent)
                .map_err(|e| format!("create {}: {}", rsrc_parent.display(), e))?;
            std::fs::write(&rsrc_path, file.rsrc)
                .map_err(|e| format!("write resource fork {}: {}", rsrc_path.display(), e))?;
        }
    }

    Ok(())
}

fn materialize_loaded_archive(root: &Path, game_path: &Path) -> Result<(), String> {
    let mut runner = systemless::game::new_runner();
    systemless::game::load_game_from_path(&mut runner, game_path)
        .map_err(|e| format!("load archive for SheepShaver staging: {}", e))?;
    let snapshots = runner
        .vfs_file_summaries()
        .into_iter()
        .filter_map(|summary| runner.vfs_file_snapshot(&summary.path))
        .collect::<Vec<_>>();
    if snapshots.is_empty() {
        return Err("generic game loader produced no files for SheepShaver staging".into());
    }

    std::fs::remove_dir_all(root)
        .map_err(|e| format!("clear raw installer staging {}: {}", root.display(), e))?;
    let materialized_root = root.join("Materialized Game");
    std::fs::create_dir_all(&materialized_root)
        .map_err(|e| format!("create {}: {}", materialized_root.display(), e))?;

    for file in snapshots {
        let relative = validated_archive_relative_path(&file.path)?;
        let path = materialized_root.join(relative);
        let parent = path
            .parent()
            .ok_or_else(|| format!("materialized file has no parent: {}", path.display()))?;
        std::fs::create_dir_all(parent)
            .map_err(|e| format!("create materialized parent {}: {}", parent.display(), e))?;
        std::fs::write(&path, &file.data_fork)
            .map_err(|e| format!("write materialized data fork {}: {}", path.display(), e))?;

        let finf_path = extfs_sidecar_path(&path, ".finf")?;
        let finf_parent = finf_path
            .parent()
            .ok_or_else(|| format!("Finder-info sidecar has no parent: {}", finf_path.display()))?;
        std::fs::create_dir_all(finf_parent)
            .map_err(|e| format!("create {}: {}", finf_parent.display(), e))?;
        let mut finf = [0u8; 32];
        finf[0..4].copy_from_slice(&file.file_type.to_be_bytes());
        finf[4..8].copy_from_slice(&file.creator.to_be_bytes());
        finf[8..10].copy_from_slice(&file.finder_flags.to_be_bytes());
        std::fs::write(&finf_path, finf)
            .map_err(|e| format!("write Finder-info sidecar {}: {}", finf_path.display(), e))?;

        if !file.resource_fork.is_empty() {
            let rsrc_path = extfs_sidecar_path(&path, ".rsrc")?;
            let rsrc_parent = rsrc_path.parent().ok_or_else(|| {
                format!(
                    "resource-fork sidecar has no parent: {}",
                    rsrc_path.display()
                )
            })?;
            std::fs::create_dir_all(rsrc_parent)
                .map_err(|e| format!("create {}: {}", rsrc_parent.display(), e))?;
            std::fs::write(&rsrc_path, file.resource_fork)
                .map_err(|e| format!("write resource fork {}: {}", rsrc_path.display(), e))?;
        }
    }

    Ok(())
}

fn validated_archive_relative_path(path: &str) -> Result<PathBuf, String> {
    let path = Path::new(path);
    if path.as_os_str().is_empty() || path.is_absolute() {
        return Err(format!("unsafe disk-image path {:?}", path));
    }
    let mut relative = PathBuf::new();
    for component in path.components() {
        match component {
            Component::Normal(name) => relative.push(name),
            Component::Prefix(_)
            | Component::RootDir
            | Component::CurDir
            | Component::ParentDir => {
                return Err(format!("unsafe disk-image path {:?}", path));
            }
        }
    }
    Ok(relative)
}

fn flatten_and_rename_main_app(
    extfs_dir: &Path,
    target_app_name: &str,
    executable_override: Option<&str>,
) -> Result<(), String> {
    let subdirs: Vec<PathBuf> = std::fs::read_dir(extfs_dir)
        .map_err(|e| format!("read extfs_dir: {}", e))?
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| p.is_dir())
        .collect();
    let game_folder = if subdirs.len() == 1 {
        subdirs.into_iter().next().unwrap()
    } else if subdirs.is_empty() {
        eprintln!(
            "[PS] No subdirectory under {}; treating extfs_dir as the game folder",
            extfs_dir.display()
        );
        extfs_dir.to_path_buf()
    } else {
        return Err(format!(
            "expected exactly 1 extracted folder under {}, found {}",
            extfs_dir.display(),
            subdirs.len()
        ));
    };
    let folder_name_lower = game_folder
        .file_name()
        .map(|n| n.to_string_lossy().to_lowercase())
        .unwrap_or_default();

    let mut appl_candidates = collect_appl_candidates(&game_folder, false)?;
    if let Some(wanted) = executable_override {
        if !appl_candidates
            .iter()
            .any(|path| candidate_matches_executable(path, &game_folder, wanted))
        {
            let recursive_candidates = collect_appl_candidates(&game_folder, true)?;
            if recursive_candidates
                .iter()
                .any(|path| candidate_matches_executable(path, &game_folder, wanted))
            {
                appl_candidates = recursive_candidates;
            }
        }
    }
    if appl_candidates.is_empty() {
        appl_candidates = collect_appl_candidates(&game_folder, true)?;
    }
    if appl_candidates.is_empty() {
        return Err(format!(
            "no executable/APPL application found in {}",
            game_folder.display()
        ));
    }

    let override_match = executable_override.and_then(|wanted| {
        appl_candidates
            .iter()
            .filter_map(|p| {
                let name = p.file_name()?.to_string_lossy().to_lowercase();
                if candidate_matches_executable(p, &game_folder, wanted) {
                    Some((p, name))
                } else {
                    None
                }
            })
            .min_by_key(|(_, name)| {
                let wanted_lower = wanted.to_lowercase();
                let exact_rank = if *name == wanted_lower { 0 } else { 1 };
                let starts_rank = if name.starts_with(&wanted_lower) {
                    0
                } else {
                    1
                };
                (exact_rank, starts_rank, name.len())
            })
            .map(|(p, _)| p)
            .cloned()
    });

    let main_app = if let Some(p) = override_match {
        p
    } else if appl_candidates.len() == 1 {
        appl_candidates.into_iter().next().unwrap()
    } else {
        let folder_words: HashSet<String> = tokenize_alpha_words(&folder_name_lower)
            .into_iter()
            .collect();
        let mut best: Option<(PathBuf, usize)> = None;
        for path in &appl_candidates {
            let fname = path
                .file_name()
                .map(|n| n.to_string_lossy().to_lowercase())
                .unwrap_or_default();
            if folder_name_lower.contains(&fname) {
                let len = fname.len();
                if best.as_ref().map(|(_, l)| len > *l).unwrap_or(true) {
                    best = Some((path.clone(), len));
                }
            }
        }
        if let Some((path, _)) = best {
            path
        } else {
            let mut scored: Vec<(PathBuf, usize, usize, usize)> = appl_candidates
                .iter()
                .map(|path| {
                    let fname_lower = path
                        .file_name()
                        .map(|n| n.to_string_lossy().to_lowercase())
                        .unwrap_or_default();
                    let candidate_words = tokenize_alpha_words(&fname_lower);
                    let overlap = candidate_words
                        .iter()
                        .filter(|word| folder_words.contains(*word))
                        .count();
                    let extra_words = candidate_words.len().saturating_sub(overlap);
                    (path.clone(), overlap, extra_words, fname_lower.len())
                })
                .collect();
            scored.sort_by(|a, b| {
                b.1.cmp(&a.1)
                    .then_with(|| a.2.cmp(&b.2))
                    .then_with(|| a.3.cmp(&b.3))
            });
            if let Some((path, overlap, _, _)) = scored.first() {
                if *overlap > 0 {
                    path.clone()
                } else {
                    let mut sorted = appl_candidates;
                    sorted.sort_by_key(|p| p.file_name().map(|n| n.len()).unwrap_or(usize::MAX));
                    sorted.into_iter().next().unwrap()
                }
            } else {
                let mut sorted = appl_candidates;
                sorted.sort_by_key(|p| p.file_name().map(|n| n.len()).unwrap_or(usize::MAX));
                sorted.into_iter().next().unwrap()
            }
        }
    };

    let main_app_name = main_app
        .file_name()
        .ok_or("main app has no name")?
        .to_os_string();
    let mut main_app_rsrc_name = main_app_name.clone();
    main_app_rsrc_name.push(".rsrc");
    let mut target_app_rsrc_name = std::ffi::OsString::from(target_app_name);
    target_app_rsrc_name.push(".rsrc");
    eprintln!(
        "[PS] Main application detected: {}",
        main_app_name.to_string_lossy()
    );

    let source_folder = main_app.parent().unwrap_or(&game_folder);
    let same_dir = source_folder == extfs_dir;
    for entry in
        std::fs::read_dir(source_folder).map_err(|e| format!("re-read game_folder: {}", e))?
    {
        let entry = entry.map_err(|e| format!("entry: {}", e))?;
        let from = entry.path();
        let base_name = entry.file_name();
        if base_name == ".finf" || base_name == ".rsrc" {
            continue;
        }
        let target_name = if base_name == main_app_name {
            std::ffi::OsString::from(target_app_name)
        } else if base_name == main_app_rsrc_name {
            target_app_rsrc_name.clone()
        } else if same_dir {
            continue;
        } else {
            base_name.clone()
        };
        let to = extfs_dir.join(&target_name);
        if from == to {
            continue;
        }
        std::fs::rename(&from, &to)
            .map_err(|e| format!("rename {} -> {}: {}", from.display(), to.display(), e))?;
    }
    move_extfs_sidecars(source_folder, extfs_dir, &main_app_name, target_app_name)?;
    if !same_dir && source_folder.is_dir() {
        std::fs::remove_dir(source_folder)
            .map_err(|e| format!("remove empty game folder: {}", e))?;
    }

    Ok(())
}

fn move_extfs_sidecars(
    source_folder: &Path,
    extfs_dir: &Path,
    main_app_name: &std::ffi::OsStr,
    target_app_name: &str,
) -> Result<(), String> {
    for sidecar_dir_name in [".finf", ".rsrc"] {
        let source_dir = source_folder.join(sidecar_dir_name);
        if !source_dir.is_dir() {
            continue;
        }
        let target_dir = extfs_dir.join(sidecar_dir_name);
        std::fs::create_dir_all(&target_dir)
            .map_err(|e| format!("mkdir {}: {}", target_dir.display(), e))?;
        for entry in std::fs::read_dir(&source_dir)
            .map_err(|e| format!("read {}: {}", source_dir.display(), e))?
        {
            let entry = entry.map_err(|e| format!("entry in {}: {}", source_dir.display(), e))?;
            let from = entry.path();
            let file_type = entry
                .file_type()
                .map_err(|e| format!("file_type {}: {}", from.display(), e))?;
            if !file_type.is_file() {
                continue;
            }
            let target_name = if entry.file_name() == main_app_name {
                std::ffi::OsString::from(target_app_name)
            } else {
                entry.file_name()
            };
            let to = target_dir.join(target_name);
            if from == to {
                continue;
            }
            if to.exists() {
                return Err(format!("extfs sidecar collision at {}", to.display()));
            }
            std::fs::rename(&from, &to)
                .map_err(|e| format!("rename {} -> {}: {}", from.display(), to.display(), e))?;
        }
        if source_dir != target_dir {
            std::fs::remove_dir(&source_dir)
                .map_err(|e| format!("remove empty sidecar dir {}: {}", source_dir.display(), e))?;
        }
    }
    Ok(())
}

fn is_pef_executable(path: &Path) -> bool {
    let Ok(mut file) = std::fs::File::open(path) else {
        return false;
    };
    let mut header = [0u8; 8];
    if file.read_exact(&mut header).is_ok() {
        // PEF magic "Joy!" + "peff"
        &header[0..4] == b"Joy!"
    } else {
        false
    }
}

fn collect_appl_candidates(root: &Path, recursive: bool) -> Result<Vec<PathBuf>, String> {
    let mut appl_candidates: Vec<PathBuf> = Vec::new();
    let mut pef_candidates: Vec<PathBuf> = Vec::new();
    let mut forked_file_candidates: Vec<PathBuf> = Vec::new();
    let mut stack = vec![root.to_path_buf()];

    while let Some(dir) = stack.pop() {
        for entry in
            std::fs::read_dir(&dir).map_err(|e| format!("read {}: {}", dir.display(), e))?
        {
            let entry = entry.map_err(|e| format!("entry: {}", e))?;
            let path = entry.path();
            let file_type = entry
                .file_type()
                .map_err(|e| format!("file_type {}: {}", path.display(), e))?;
            if file_type.is_dir() {
                if recursive {
                    let name = entry.file_name();
                    let name = name.to_string_lossy();
                    if name != ".finf" && name != ".rsrc" {
                        stack.push(path);
                    }
                }
                continue;
            }
            if !file_type.is_file() || is_rsrc_companion(&path) {
                continue;
            }
            if is_pef_executable(&path) {
                pef_candidates.push(path.clone());
            }
            if sidecar_rsrc_path(&path).is_file() || extfs_sidecar_path(&path, ".rsrc")?.is_file() {
                forked_file_candidates.push(path.clone());
            }
            let Some(finder_info) = read_finder_info(&path)? else {
                continue;
            };
            if finder_info.len() >= 4 && &finder_info[0..4] == b"APPL" {
                appl_candidates.push(path);
            }
        }
    }

    if !pef_candidates.is_empty() || !appl_candidates.is_empty() {
        for candidate in appl_candidates {
            if !pef_candidates.contains(&candidate) {
                pef_candidates.push(candidate);
            }
        }
        Ok(pef_candidates)
    } else {
        Ok(forked_file_candidates)
    }
}

fn candidate_matches_executable(path: &Path, root: &Path, wanted: &str) -> bool {
    let wanted_lower = wanted.replace('\\', "/").to_lowercase();
    let file_name = path
        .file_name()
        .map(|name| name.to_string_lossy().to_lowercase())
        .unwrap_or_default();
    if file_name.contains(&wanted_lower) {
        return true;
    }
    if !wanted_lower.contains('/') {
        return false;
    }
    let relative = path
        .strip_prefix(root)
        .ok()
        .map(|path| {
            path.components()
                .map(|component| component.as_os_str().to_string_lossy())
                .collect::<Vec<_>>()
                .join("/")
                .to_lowercase()
        })
        .unwrap_or_default();
    relative.contains(&wanted_lower)
}

fn tokenize_alpha_words(input: &str) -> Vec<String> {
    let mut words: Vec<String> = Vec::new();
    let mut current = String::new();
    for ch in input.chars() {
        if ch.is_ascii_alphanumeric() {
            current.push(ch.to_ascii_lowercase());
        } else if !current.is_empty() {
            if current.chars().any(|c| c.is_ascii_alphabetic()) {
                words.push(std::mem::take(&mut current));
            } else {
                current.clear();
            }
        }
    }
    if !current.is_empty() && current.chars().any(|c| c.is_ascii_alphabetic()) {
        words.push(current);
    }
    words
}

fn materialize_resource_only_data_forks(root: &Path) -> Result<(), String> {
    let mut created = 0usize;
    let mut stack: Vec<PathBuf> = vec![root.to_path_buf()];

    while let Some(dir) = stack.pop() {
        let entries =
            std::fs::read_dir(&dir).map_err(|e| format!("read_dir {}: {}", dir.display(), e))?;
        for entry in entries {
            let entry = entry.map_err(|e| format!("read entry: {}", e))?;
            let path = entry.path();
            let name = entry.file_name();
            let name_str = name.to_string_lossy();

            if entry
                .file_type()
                .map_err(|e| format!("file_type {}: {}", path.display(), e))?
                .is_dir()
            {
                if name_str != ".finf" && name_str != ".rsrc" {
                    stack.push(path);
                }
                continue;
            }

            if !name_str.ends_with(".rsrc") {
                continue;
            }
            let Some(base_name) = name_str.strip_suffix(".rsrc") else {
                continue;
            };
            if base_name.is_empty() {
                continue;
            }

            let data_path = dir.join(base_name);
            if !data_path.exists() {
                std::fs::write(&data_path, [])
                    .map_err(|e| format!("create {}: {}", data_path.display(), e))?;
                created += 1;
            }
        }
    }

    if created > 0 {
        eprintln!("[PS] Materialized {} resource-only data fork(s)", created);
    }
    Ok(())
}

fn is_rsrc_companion(path: &Path) -> bool {
    path.file_name()
        .map(|n| n.to_string_lossy().ends_with(".rsrc"))
        .unwrap_or(false)
}

fn sidecar_rsrc_path(path: &Path) -> PathBuf {
    let mut name = path
        .file_name()
        .map(|n| n.to_os_string())
        .unwrap_or_default();
    name.push(".rsrc");
    path.with_file_name(name)
}

fn extfs_sidecar_path(path: &Path, sidecar_dir: &str) -> Result<PathBuf, String> {
    let parent = path
        .parent()
        .ok_or_else(|| format!("file has no parent: {}", path.display()))?;
    let name = path
        .file_name()
        .ok_or_else(|| format!("file has no name: {}", path.display()))?;
    Ok(parent.join(sidecar_dir).join(name))
}

fn read_xattr(file_path: &Path, attr_name: &str) -> Result<Option<Vec<u8>>, String> {
    #[cfg(target_os = "macos")]
    if attr_name == "com.apple.ResourceFork" {
        let named_fork = file_path.join("..namedfork/rsrc");
        match std::fs::read(&named_fork) {
            Ok(bytes) => return Ok(Some(bytes)),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(error) => {
                return Err(format!(
                    "read resource fork {}: {}",
                    file_path.display(),
                    error
                ));
            }
        }
    }
    xattr::get(file_path, attr_name)
        .map_err(|e| format!("read xattr {} on {}: {}", attr_name, file_path.display(), e))
}

fn read_finder_info(file_path: &Path) -> Result<Option<Vec<u8>>, String> {
    if let Some(finder_info) = read_xattr(file_path, "com.apple.FinderInfo")? {
        return Ok(Some(finder_info));
    }
    let sidecar = extfs_sidecar_path(file_path, ".finf")?;
    if sidecar.is_file() {
        return std::fs::read(&sidecar)
            .map(Some)
            .map_err(|e| format!("read Finder-info sidecar {}: {}", sidecar.display(), e));
    }
    Ok(None)
}

fn convert_xattrs_to_basilisk_sidecars(root: &Path) -> Result<(), String> {
    let mut stack: Vec<PathBuf> = vec![root.to_path_buf()];

    while let Some(dir) = stack.pop() {
        let entries =
            std::fs::read_dir(&dir).map_err(|e| format!("read_dir {}: {}", dir.display(), e))?;
        for entry in entries {
            let entry = entry.map_err(|e| format!("read entry: {}", e))?;
            let path = entry.path();
            let name = entry.file_name();
            let name_str = name.to_string_lossy();

            if entry
                .file_type()
                .map_err(|e| format!("file_type {}: {}", path.display(), e))?
                .is_dir()
            {
                if name_str != ".finf" && name_str != ".rsrc" {
                    stack.push(path);
                }
                continue;
            }

            if is_rsrc_companion(&path) {
                let Some(base_name) = name_str.strip_suffix(".rsrc") else {
                    continue;
                };
                if base_name.is_empty() {
                    continue;
                }
                let logical_path = dir.join(base_name);
                let rsrc_target = extfs_sidecar_path(&logical_path, ".rsrc")?;
                if !rsrc_target.is_file() {
                    if let Some(parent) = rsrc_target.parent() {
                        std::fs::create_dir_all(parent)
                            .map_err(|e| format!("mkdir {}: {}", parent.display(), e))?;
                    }
                    std::fs::copy(&path, &rsrc_target)
                        .map_err(|e| format!("copy rsrc sidecar {}: {}", path.display(), e))?;
                }
                let finf_target = extfs_sidecar_path(&logical_path, ".finf")?;
                if !finf_target.is_file() {
                    if let Some(parent) = finf_target.parent() {
                        std::fs::create_dir_all(parent)
                            .map_err(|e| format!("mkdir {}: {}", parent.display(), e))?;
                    }
                    let mut finf = vec![0u8; 32];
                    finf[0..4].copy_from_slice(b"APPL");
                    finf[4..8].copy_from_slice(b"????");
                    std::fs::write(&finf_target, finf)
                        .map_err(|e| format!("write finf {}: {}", finf_target.display(), e))?;
                }
                continue;
            }

            if let Some(finder_info) = read_xattr(&path, "com.apple.FinderInfo")? {
                let finf_path = extfs_sidecar_path(&path, ".finf")?;
                if let Some(parent) = finf_path.parent() {
                    std::fs::create_dir_all(parent)
                        .map_err(|e| format!("mkdir {}: {}", parent.display(), e))?;
                }
                std::fs::write(&finf_path, finder_info)
                    .map_err(|e| format!("write {}: {}", finf_path.display(), e))?;
            }

            if let Some(rsrc) = read_xattr(&path, "com.apple.ResourceFork")? {
                if !rsrc.is_empty() {
                    let rsrc_path = extfs_sidecar_path(&path, ".rsrc")?;
                    if let Some(parent) = rsrc_path.parent() {
                        std::fs::create_dir_all(parent)
                            .map_err(|e| format!("mkdir {}: {}", parent.display(), e))?;
                    }
                    std::fs::write(&rsrc_path, rsrc)
                        .map_err(|e| format!("write {}: {}", rsrc_path.display(), e))?;
                }
            }
        }
    }

    Ok(())
}

fn ensure_play_target_finf(extfs_dir: &Path) -> Result<(), String> {
    let play_target = extfs_dir.join("_PlayTarget");
    if !play_target.is_file() {
        return Ok(());
    }
    let finf_dir = extfs_dir.join(".finf");
    let finf_path = finf_dir.join("_PlayTarget");
    if finf_path.is_file() {
        return Ok(());
    }
    std::fs::create_dir_all(&finf_dir)
        .map_err(|e| format!("mkdir {}: {}", finf_dir.display(), e))?;
    let mut finf = vec![0u8; 32];
    finf[0..4].copy_from_slice(b"APPL");
    finf[4..8].copy_from_slice(b"????");
    std::fs::write(&finf_path, finf).map_err(|e| format!("write {}: {}", finf_path.display(), e))
}

fn stage_play_launcher(
    manifest_dir: &Path,
    scratch: &Path,
    extfs_dir: &Path,
) -> Result<(), String> {
    let mpw_dir = manifest_dir.join("support/mpw");
    let launcher_source = manifest_dir.join("support/launcher_play");
    let launcher_output = scratch.join("launcher_play_output");
    let launcher_work = scratch.join("launcher_play_work");

    eprintln!(
        "[PS] Building PlayLauncher from {}",
        launcher_source.display()
    );
    build_mpw_builder_image(&mpw_dir, false);
    compile_with_mpw(
        "launcher_play",
        &launcher_source,
        &launcher_output,
        &launcher_work,
    );
    copy_appledouble_file(&launcher_output, "FixtureGen", extfs_dir, "FixtureGen")?;
    Ok(())
}

fn copy_appledouble_file(
    src_dir: &Path,
    src_name: &str,
    dst_dir: &Path,
    dst_name: &str,
) -> Result<(), String> {
    std::fs::copy(src_dir.join(src_name), dst_dir.join(dst_name))
        .map_err(|e| format!("copy {} data fork: {}", src_name, e))?;

    for sidecar_dir in [".finf", ".rsrc"] {
        let src = src_dir.join(sidecar_dir).join(src_name);
        if src.exists() {
            let dst_root = dst_dir.join(sidecar_dir);
            std::fs::create_dir_all(&dst_root)
                .map_err(|e| format!("mkdir {}: {}", dst_root.display(), e))?;
            std::fs::copy(&src, dst_root.join(dst_name))
                .map_err(|e| format!("copy {} {}: {}", sidecar_dir, src_name, e))?;
        }
    }

    Ok(())
}

fn write_u32_be_file(path: &Path, value: u32) -> Result<(), String> {
    std::fs::write(path, value.to_be_bytes())
        .map_err(|e| format!("write {}: {}", path.display(), e))
}

fn apply_remove_paths(extfs_dir: &Path, paths: &[String]) -> Result<(), String> {
    for rel in paths {
        let target = extfs_dir.join(rel);
        if target.is_dir() {
            std::fs::remove_dir_all(&target)
                .map_err(|e| format!("remove_paths: rmdir {}: {}", target.display(), e))?;
            eprintln!("[PS] Removed directory {}", target.display());
        } else if target.is_file() {
            std::fs::remove_file(&target)
                .map_err(|e| format!("remove_paths: rm {}: {}", target.display(), e))?;
            eprintln!("[PS] Removed file {}", target.display());
        }
        if let (Some(parent), Some(name)) = (target.parent(), target.file_name()) {
            for sidecar in [".finf", ".rsrc"] {
                let sc = parent.join(sidecar).join(name);
                if sc.exists() {
                    let _ = std::fs::remove_file(&sc);
                }
            }
        }
    }
    Ok(())
}

fn apply_application_partition_size(extfs_dir: &Path, bytes: Option<u32>) -> Result<(), String> {
    let Some(bytes) = bytes else {
        return Ok(());
    };
    if bytes < 128 * 1024 {
        return Err(format!(
            "application_partition_size {} is below the 128K Process Manager floor",
            bytes
        ));
    }

    let rsrc_path = extfs_dir.join(".rsrc").join("_PlayTarget");
    if !rsrc_path.is_file() {
        return Ok(());
    }
    let mut fork =
        std::fs::read(&rsrc_path).map_err(|e| format!("read {}: {}", rsrc_path.display(), e))?;
    if patch_size_resource_in_fork(&mut fork, bytes)? {
        std::fs::write(&rsrc_path, fork)
            .map_err(|e| format!("write {}: {}", rsrc_path.display(), e))?;
        eprintln!(
            "[PS] Patched _PlayTarget SIZE -1 partition size to {} bytes",
            bytes
        );
    }
    Ok(())
}

fn patch_size_resource_in_fork(fork: &mut [u8], bytes: u32) -> Result<bool, String> {
    if fork.len() < 16 {
        return Ok(false);
    }
    let data_offset = u32::from_be_bytes(fork[0..4].try_into().unwrap()) as usize;
    let map_offset = u32::from_be_bytes(fork[4..8].try_into().unwrap()) as usize;
    if map_offset + 30 > fork.len() {
        return Ok(false);
    }
    let type_list_rel =
        u16::from_be_bytes(fork[map_offset + 24..map_offset + 26].try_into().unwrap()) as usize;
    let type_list_offset = map_offset + type_list_rel;
    if type_list_offset + 2 > fork.len() {
        return Ok(false);
    }
    let type_count = u16::from_be_bytes(
        fork[type_list_offset..type_list_offset + 2]
            .try_into()
            .unwrap(),
    ) as usize
        + 1;
    for type_idx in 0..type_count {
        let entry_offset = type_list_offset + 2 + type_idx * 8;
        if entry_offset + 8 > fork.len() {
            return Ok(false);
        }
        if &fork[entry_offset..entry_offset + 4] != b"SIZE" {
            continue;
        }
        let ref_list_rel =
            u16::from_be_bytes(fork[entry_offset + 6..entry_offset + 8].try_into().unwrap())
                as usize;
        let ref_list_offset = type_list_offset + ref_list_rel;
        if ref_list_offset + 12 > fork.len() {
            return Ok(false);
        }
        let res_id = i16::from_be_bytes(
            fork[ref_list_offset..ref_list_offset + 2]
                .try_into()
                .unwrap(),
        );
        if res_id == -1 || res_id == 0 {
            let data_rel = u32::from_be_bytes([
                0,
                fork[ref_list_offset + 5],
                fork[ref_list_offset + 6],
                fork[ref_list_offset + 7],
            ]) as usize;
            let target_data_offset = data_offset + data_rel;
            if target_data_offset + 4 + 10 <= fork.len() {
                fork[target_data_offset + 4 + 2..target_data_offset + 4 + 6]
                    .copy_from_slice(&bytes.to_be_bytes());
                fork[target_data_offset + 4 + 6..target_data_offset + 4 + 10]
                    .copy_from_slice(&bytes.to_be_bytes());
                return Ok(true);
            }
        }
    }
    Ok(false)
}

fn stage_prelaunch_create_dirs(extfs_dir: &Path, dirs: Option<&[String]>) -> Result<(), String> {
    let file = extfs_dir.join(PRELAUNCH_CREATE_DIRS_FILE);
    let Some(dirs) = dirs else {
        if file.exists() {
            let _ = std::fs::remove_file(&file);
        }
        return Ok(());
    };
    let content = dirs.join("\r\n");
    std::fs::write(&file, content).map_err(|e| format!("write {}: {}", file.display(), e))
}

fn stage_prelaunch_delete_paths(extfs_dir: &Path, paths: Option<&[String]>) -> Result<(), String> {
    let file = extfs_dir.join(PRELAUNCH_DELETE_PATHS_FILE);
    let Some(paths) = paths else {
        if file.exists() {
            let _ = std::fs::remove_file(&file);
        }
        return Ok(());
    };
    let content = paths.join("\r\n");
    std::fs::write(&file, content).map_err(|e| format!("write {}: {}", file.display(), e))
}

fn stage_boot_disk_copy_manifest(extfs_dir: &Path, enabled: bool) -> Result<(), String> {
    let file = extfs_dir.join(PRELAUNCH_BOOT_COPY_MANIFEST_FILE);
    if !enabled {
        if file.exists() {
            let _ = std::fs::remove_file(&file);
        }
        return Ok(());
    }
    let mut manifest_lines = Vec::new();
    let mut stack = vec![extfs_dir.to_path_buf()];
    while let Some(dir) = stack.pop() {
        for entry in
            std::fs::read_dir(&dir).map_err(|e| format!("read {}: {}", dir.display(), e))?
        {
            let entry = entry.map_err(|e| format!("entry: {}", e))?;
            let path = entry.path();
            let name = entry.file_name().to_string_lossy().to_string();
            if name == ".finf"
                || name == ".rsrc"
                || name.starts_with('_')
                || name == "FixtureGen"
                || name == "Launcher"
            {
                continue;
            }
            let rel = path
                .strip_prefix(extfs_dir)
                .unwrap()
                .to_string_lossy()
                .replace('/', ":");
            if entry.file_type().unwrap().is_dir() {
                manifest_lines.push(format!("D\t{}", rel));
                stack.push(path);
            } else {
                manifest_lines.push(format!("F\t{}", rel));
            }
        }
    }
    let content = manifest_lines.join("\r\n");
    std::fs::write(&file, content).map_err(|e| format!("write {}: {}", file.display(), e))
}

fn stage_play_target_to_boot_disk(scratch: &ScratchLayout) -> Result<(), String> {
    let target = scratch.extfs_dir.join("_PlayTarget");
    let finf = std::fs::read(scratch.extfs_dir.join(".finf/_PlayTarget"))
        .map_err(|e| format!("read _PlayTarget Finder info: {}", e))?;
    let data = std::fs::read(&target).map_err(|e| format!("read _PlayTarget data: {}", e))?;
    let resource = read_xattr(&target, "com.apple.ResourceFork")?
        .or_else(|| std::fs::read(scratch.extfs_dir.join(".rsrc/_PlayTarget")).ok())
        .unwrap_or_default();
    let file_type: [u8; 4] = finf
        .get(0..4)
        .unwrap_or(b"APPL")
        .try_into()
        .map_err(|_| "invalid _PlayTarget file type")?;
    let creator: [u8; 4] = finf
        .get(4..8)
        .unwrap_or(b"????")
        .try_into()
        .map_err(|_| "invalid _PlayTarget creator")?;
    let macbinary_path = scratch.root.join("PlayTarget.bin");
    let mut output = std::fs::File::create(&macbinary_path)
        .map_err(|e| format!("create {}: {}", macbinary_path.display(), e))?;
    systemless_oracle_tools::macbinary::encode_macbinary(
        &mut output,
        b"_PlayTarget",
        &file_type,
        &creator,
        &data,
        &resource,
    )
    .map_err(|e| format!("encode _PlayTarget MacBinary: {}", e))?;

    let disk = scratch.root.join("shared/disk/System_PPC.dsk");
    let status = Command::new("docker")
        .args([
            "run",
            "--rm",
            "--entrypoint",
            "/bin/bash",
            "-v",
            &format!("{}:/mnt/disk.dsk", disk.display()),
            "-v",
            &format!("{}:/mnt/PlayTarget.bin:ro", macbinary_path.display()),
            &docker_image(),
            "-c",
            "hmount /mnt/disk.dsk && hcd : && (hmkdir 'Systemless Play' 2>/dev/null || true) && hcd 'Systemless Play' && (hdel _PlayTarget 2>/dev/null || true) && hcopy -m /mnt/PlayTarget.bin :_PlayTarget",
        ])
        .status()
        .map_err(|e| format!("stage _PlayTarget with hfsutils: {}", e))?;
    if !status.success() {
        return Err("hfsutils could not stage _PlayTarget on the SheepShaver boot disk".into());
    }
    eprintln!(
        "[PS] Staged _PlayTarget on writable boot disk ({} data bytes, {} resource bytes)",
        data.len(),
        resource.len()
    );
    Ok(())
}

fn script_uses_keypad_digit_alias(_script: &PlayScript) -> bool {
    true
}

fn prime_x11_numlock_for_keypad_aliases() -> Result<(), String> {
    Ok(())
}

struct ContainerGuard;

fn sheepshaver_container_run_args(scratch: &Path, container_user: Option<&str>) -> Vec<String> {
    let shared_src = docker_mount_source(&scratch.join("shared"));
    let extfs_src = docker_mount_source(&scratch.join("extfs"));
    let home_src = docker_mount_source(&scratch.join("container_home"));
    let mut args = vec![
        "run".to_string(),
        "-d".to_string(),
        "--rm".to_string(),
        "--network".to_string(),
        "none".to_string(),
        "--privileged".to_string(),
    ];
    if let Some(container_user) = container_user {
        args.extend(["--user".to_string(), container_user.to_string()]);
    }
    args.extend([
        "-e".to_string(),
        "HOME=/tmp".to_string(),
        "--name".to_string(),
        CONTAINER_NAME.to_string(),
        "-v".to_string(),
        format!("{}:/mnt/shared", shared_src),
        "-v".to_string(),
        format!("{}:/mnt/extfs", extfs_src),
        "-v".to_string(),
        format!("{}:/root:ro", home_src),
    ]);
    args
}

#[cfg(unix)]
fn scratch_container_user(scratch: &Path) -> Result<Option<String>, String> {
    use std::os::unix::fs::MetadataExt;
    let metadata = std::fs::metadata(scratch)
        .map_err(|error| format!("inspect scratch owner {}: {}", scratch.display(), error))?;
    Ok(Some(format!("{}:{}", metadata.uid(), metadata.gid())))
}

#[cfg(not(unix))]
fn scratch_container_user(_scratch: &Path) -> Result<Option<String>, String> {
    Ok(None)
}

impl ContainerGuard {
    fn start(scratch: &Path, play_time_secs: u32) -> Result<Self, String> {
        let _ = Command::new("docker")
            .args(["rm", "-f", CONTAINER_NAME])
            .output();

        let mut command = Command::new("docker");
        let container_user = scratch_container_user(scratch)?;
        command.args(sheepshaver_container_run_args(
            scratch,
            container_user.as_deref(),
        ));
        let cpus = std::env::var("SYSTEMLESS_SHEEPSHAVER_CPUS")
            .ok()
            .filter(|value| !value.trim().is_empty())
            .unwrap_or_else(|| DEFAULT_DOCKER_CPUS.to_string());
        command.args(["--cpus", &cpus]);

        let mac_time = resolved_sheepshaver_mac_time(
            std::env::var("SYSTEMLESS_SHEEPSHAVER_MAC_TIME").ok(),
            play_time_secs,
        );
        command.args([
            "-e",
            &format!("SYSTEMLESS_SHEEPSHAVER_MAC_TIME={}", mac_time),
        ]);
        command.args(["-e", &format!("SYSTEMLESS_BASILISK_MAC_TIME={}", mac_time)]);
        let deterministic_ticks = resolved_sheepshaver_deterministic_ticks(
            std::env::var("SYSTEMLESS_SHEEPSHAVER_DETERMINISTIC_TICKS").ok(),
        );
        command.args([
            "-e",
            &format!(
                "SYSTEMLESS_SHEEPSHAVER_DETERMINISTIC_TICKS={}",
                deterministic_ticks
            ),
        ]);
        command.args([
            "-e",
            "SYSTEMLESS_SHEEPSHAVER_CLOCK=/mnt/shared/sheepshaver_clock",
        ]);
        command.args([
            "-e",
            "SYSTEMLESS_BASILISK_CLOCK=/mnt/shared/sheepshaver_clock",
        ]);

        let instructions_per_tick = resolved_sheepshaver_instructions_per_tick(
            std::env::var("SYSTEMLESS_SHEEPSHAVER_INSTRUCTIONS_PER_TICK").ok(),
        );
        command.args([
            "-e",
            &format!(
                "SYSTEMLESS_SHEEPSHAVER_INSTRUCTIONS_PER_TICK={}",
                instructions_per_tick
            ),
        ]);

        if std::env::var_os("SYSTEMLESS_SHEEPSHAVER_TRACE").is_some() {
            command.args([
                "-e",
                "SYSTEMLESS_SHEEPSHAVER_TRACE=/mnt/shared/sheepshaver_trace.jsonl",
            ]);
        }

        let output = command
            .arg(docker_image())
            .output()
            .map_err(|e| format!("docker run: {}", e))?;
        if !output.status.success() {
            return Err(format!(
                "docker run failed: {}",
                String::from_utf8_lossy(&output.stderr)
            ));
        }
        eprintln!("[PS] SheepShaver Container started");
        Ok(Self)
    }
}

impl Drop for ContainerGuard {
    fn drop(&mut self) {
        if let Ok(out) = Command::new("docker")
            .args(["logs", CONTAINER_NAME])
            .output()
        {
            let stdout = String::from_utf8_lossy(&out.stdout);
            let stderr = String::from_utf8_lossy(&out.stderr);
            let combined = format!("{}\n{}", stdout, stderr);
            let combined = combined.trim();
            if !combined.is_empty() {
                eprintln!("[PS] ---- container logs ----");
                for line in combined.lines() {
                    eprintln!("[PS] | {}", line);
                }
                eprintln!("[PS] ------------------------");
            }
        }
        eprintln!("[PS] Stopping SheepShaver container…");
        let _ = Command::new("docker")
            .args(["stop", CONTAINER_NAME])
            .output();
    }
}

fn container_running() -> bool {
    let output = Command::new("docker")
        .args([
            "ps",
            "--filter",
            &format!("name={}", CONTAINER_NAME),
            "--format",
            "{{.Names}}",
        ])
        .output();
    matches!(output, Ok(out) if out.status.success()
        && !String::from_utf8_lossy(&out.stdout).trim().is_empty())
}

fn xdotool(args: &[&str]) -> Result<(), String> {
    if args.is_empty() {
        return Err("xdotool called with empty args".to_string());
    }
    let verb = args[0];
    let window_relative = matches!(verb, "mousemove");
    let focus_before = matches!(
        verb,
        "mousedown" | "mouseup" | "click" | "keydown" | "keyup" | "key"
    );
    let mut shell = String::from(
        "export DISPLAY=:99; \
         WID=$(xdotool search --onlyvisible --name 'SheepShaver' | head -1); \
         if [ -z \"$WID\" ]; then WID=$(xdotool getactivewindow 2>/dev/null || true); fi; \
         if [ -z \"$WID\" ]; then echo 'SheepShaver window not found' >&2; exit 120; fi; \
         INPUT_WID=$(xwininfo -id \"$WID\" -children 2>/dev/null \
             | awk '$1 ~ /^0x[0-9a-fA-F]+$/ { print $1; exit }'); \
         if [ -z \"$INPUT_WID\" ]; then INPUT_WID=$WID; fi; ",
    );
    if focus_before {
        // Reasserting focus before every mouse-down/up breaks Finder's
        // double-click state. Focus once when needed, then leave it intact.
        shell.push_str(
            "FOCUS=$(xdotool getwindowfocus 2>/dev/null || true); \
             if [ \"$FOCUS\" != \"$INPUT_WID\" ]; then \
                 xdotool windowfocus \"$INPUT_WID\" 2>/dev/null || true; \
             fi; ",
        );
    }
    shell.push_str("xdotool ");
    shell.push_str(verb);
    if window_relative {
        shell.push_str(" --window \"$WID\"");
    }
    for arg in &args[1..] {
        shell.push(' ');
        shell.push_str(arg);
    }
    let output = Command::new("docker")
        .args(["exec", CONTAINER_NAME, "bash", "-c", &shell])
        .output()
        .map_err(|e| format!("docker exec xdotool: {}", e))?;
    if !output.status.success() {
        return Err(format!(
            "xdotool {:?} failed: {}",
            args,
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    Ok(())
}

fn capture_shot(
    output_dir: &Path,
    filename: &str,
    clock_path: &Path,
) -> Result<OracleClockSample, String> {
    let host_path = output_dir.join(filename);
    let sample = capture_host_shot(&host_path, clock_path)?;
    eprintln!("[PS] Saved {}", host_path.display());
    Ok(sample)
}

fn read_oracle_clock(clock_path: &Path) -> Result<Option<OracleClockSample>, String> {
    let mut file = match std::fs::File::open(clock_path) {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => {
            return Err(format!(
                "open oracle clock {}: {}",
                clock_path.display(),
                error
            ));
        }
    };
    let mut buffer = [0u8; 64];
    let count = file
        .read(&mut buffer)
        .map_err(|e| format!("read clock {}: {}", clock_path.display(), e))?;
    if count == 4 {
        let tick = u32::from_be_bytes(buffer[..4].try_into().unwrap());
        return Ok(Some(OracleClockSample {
            tick,
            retired_instructions: tick as u64,
        }));
    }
    let text = std::str::from_utf8(&buffer[..count]).map_err(|e| format!("utf8 clock: {}", e))?;
    let parts: Vec<&str> = text.split_whitespace().collect();
    if parts.len() < 4 {
        return Ok(None);
    }
    let tick1 = u32::from_str_radix(parts[0], 16).map_err(|e| format!("tick1 parse: {}", e))?;
    let ret1 = u64::from_str_radix(parts[1], 16).map_err(|e| format!("ret1 parse: {}", e))?;
    let tick2 = u32::from_str_radix(parts[2], 16).map_err(|e| format!("tick2 parse: {}", e))?;
    let ret2 = u64::from_str_radix(parts[3], 16).map_err(|e| format!("ret2 parse: {}", e))?;
    if tick1 == tick2 && ret1 == ret2 {
        Ok(Some(OracleClockSample {
            tick: tick1,
            retired_instructions: ret1,
        }))
    } else {
        Ok(None)
    }
}

fn wait_for_live_oracle_clock(clock_path: &Path, baseline_tick: u32) -> Result<bool, String> {
    let initial_tick = read_oracle_clock(clock_path)?
        .map(|sample| sample.tick)
        .unwrap_or(baseline_tick);
    let start = Instant::now();
    while start.elapsed() < Duration::from_secs(2) {
        if let Some(sample) = read_oracle_clock(clock_path)? {
            if sample.tick > initial_tick {
                return Ok(true);
            }
        }
        std::thread::sleep(Duration::from_millis(CLOCK_POLL_INTERVAL_MILLIS));
    }
    Ok(false)
}

fn wait_until_mac_tick(clock_path: &Path, target_tick: u32) -> Result<OracleClockSample, String> {
    let start = Instant::now();
    let mut last_sample = read_oracle_clock(clock_path)?.unwrap_or(OracleClockSample {
        tick: 0,
        retired_instructions: 0,
    });
    while start.elapsed() < Duration::from_secs(TICK_PROGRESS_TIMEOUT_SECS) {
        if let Some(sample) = read_oracle_clock(clock_path)? {
            last_sample = sample;
            if sample.tick >= target_tick {
                return Ok(sample);
            }
        }
        std::thread::sleep(Duration::from_millis(CLOCK_POLL_INTERVAL_MILLIS));
    }
    Err(format!(
        "SheepShaver clock stalled at tick {} before target {}",
        last_sample.tick, target_tick
    ))
}

fn capture_host_shot(host_path: &Path, clock_path: &Path) -> Result<OracleClockSample, String> {
    // Capture the fixed Xvfb canvas rather than the SDL child window. Games
    // can change the guest mode at runtime (for example 800x600 -> 640x480),
    // while play-script checkpoints and BasiliskII references retain the
    // configured oracle viewport dimensions.
    let shell = "export DISPLAY=:99; import -window root png:/tmp/shot.png";
    let status = Command::new("docker")
        .args(["exec", CONTAINER_NAME, "bash", "-c", shell])
        .status()
        .map_err(|e| format!("docker exec import: {}", e))?;
    if !status.success() {
        return Err("import command failed in container".into());
    }
    let cp_status = Command::new("docker")
        .args([
            "cp",
            &format!("{}:/tmp/shot.png", CONTAINER_NAME),
            host_path.to_str().unwrap(),
        ])
        .status()
        .map_err(|e| format!("docker cp shot: {}", e))?;
    if !cp_status.success() {
        return Err("docker cp shot failed".into());
    }
    let sample = read_oracle_clock(clock_path)?.unwrap_or(OracleClockSample {
        tick: 0,
        retired_instructions: 0,
    });
    Ok(sample)
}

fn wait_for_u32_file_with_clock_progress(
    file_path: &Path,
    clock_path: &Path,
    timeout: Duration,
    label: &str,
) -> Result<u32, String> {
    let initial_deadline = Instant::now() + timeout;
    let mut progress: Option<OracleClockProgress> = None;
    loop {
        if file_path.exists() {
            if let Ok(bytes) = std::fs::read(file_path) {
                if bytes.len() >= 4 {
                    let val = u32::from_be_bytes(bytes[0..4].try_into().unwrap());
                    return Ok(val);
                }
            }
        }
        if !container_running() {
            return Err(format!(
                "container exited before writing {} at {}",
                label,
                file_path.display()
            ));
        }
        let now = Instant::now();
        if let Ok(Some(sample)) = read_oracle_clock(clock_path) {
            match &mut progress {
                Some(tracker) => tracker.observe(sample, now),
                None => {
                    progress = Some(OracleClockProgress::new(
                        sample,
                        now,
                        Duration::from_secs(TICK_PROGRESS_TIMEOUT_SECS),
                    ));
                }
            }
        }
        let timed_out = progress
            .as_ref()
            .map_or(now >= initial_deadline, |tracker| tracker.expired(now));
        if timed_out {
            return Err(format!(
                "timed out waiting for {} at {}",
                label,
                file_path.display()
            ));
        }
        std::thread::sleep(Duration::from_millis(POLL_INTERVAL_MILLIS));
    }
}

fn wait_for_file(file_path: &Path, timeout: Duration, label: &str) -> Result<(), String> {
    let deadline = Instant::now() + timeout;
    loop {
        if file_path.exists() {
            return Ok(());
        }
        if !container_running() {
            return Err(format!(
                "container exited before creating {} at {}",
                label,
                file_path.display()
            ));
        }
        if Instant::now() >= deadline {
            return Err(format!(
                "timed out waiting for {} at {}",
                label,
                file_path.display()
            ));
        }
        std::thread::sleep(Duration::from_millis(POLL_INTERVAL_MILLIS));
    }
}

fn write_sheepshaver_capture_context(
    output_dir: &Path,
    screenshot_filename: &str,
    tick_zero: u32,
    sample: OracleClockSample,
    observed: bool,
) -> Result<(), String> {
    let body = serde_json::json!({
        "source": "sheepshaver", "screenshot": screenshot_filename,
        "clock": if observed { "guest_ticks" } else { "wall_time" },
        "tick": observed.then_some(sample.tick.wrapping_sub(tick_zero)),
        "guest_tick": observed.then_some(sample.tick),
        "retired_instructions": serde_json::Value::Null
    });
    std::fs::write(
        output_dir
            .join(screenshot_filename)
            .with_extension("ctx.json"),
        serde_json::to_vec_pretty(&body).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())
}

fn execute_action(
    idx: usize,
    action: &Action,
    output_dir: &Path,
    timing: &TimingContext,
    state: &mut ScriptState,
) -> Result<(), String> {
    if !matches!(action, Action::Screenshot { .. }) {
        state.reuse_probe_capture = false;
        state.probe_clock_sample = None;
    }
    match action {
        Action::Run {
            instructions,
            ticks,
        } => match (instructions, ticks) {
            (Some(n), None) => {
                state.reuse_probe_capture = false;
                let ipt = realtime_instructions_per_tick() as u64;
                let tick_delta = ((*n as u64) + ipt.saturating_sub(1)) / ipt;
                let reached = wait_script_ticks(timing, tick_delta as u32)?;
                eprintln!(
                    "[PS] Action {}: run {} inst (~{} ticks), reached {}",
                    idx, n, tick_delta, reached
                );
            }
            (None, Some(t)) => {
                state.reuse_probe_capture = false;
                let reached = wait_script_ticks(timing, *t)?;
                eprintln!("[PS] Action {}: run {} ticks, reached {}", idx, t, reached);
            }
            _ => return Err("Run must specify either instructions or ticks".into()),
        },
        Action::RunUntilTick { tick } => {
            state.reuse_probe_capture = false;
            let sample = wait_for_script_tick(timing, *tick)?;
            eprintln!(
                "[PS] Action {}: RunUntilTick {}, reached guest tick {}",
                idx, tick, sample.tick
            );
        }
        Action::RunUntilPixel {
            x,
            y,
            rgb,
            not,
            tolerance,
            poll_chunk: _,
            timeout_ticks,
            timeout_instructions: _,
            label,
        } => {
            let timeout =
                Duration::from_secs(timeout_ticks.map(|t| (t as u64 / 60) + 10).unwrap_or(30));
            let start = Instant::now();
            let mut matched = false;
            while start.elapsed() < timeout {
                let sample = capture_host_shot(&timing.probe_shot, &timing.clock_path)?;
                if let Ok(img) = image::open(&timing.probe_shot) {
                    let rgb8 = img.to_rgb8();
                    if *x < rgb8.width() && *y < rgb8.height() {
                        let p = rgb8.get_pixel(*x, *y);
                        if pixel_matches_within(p.0, *rgb, *not, *tolerance) {
                            state.reuse_probe_capture = true;
                            state.probe_clock_sample = Some(sample);
                            matched = true;
                            break;
                        }
                    }
                }
                std::thread::sleep(Duration::from_millis(POLL_INTERVAL_MILLIS));
            }
            if !matched {
                return Err(format!(
                    "RunUntilPixel {} timed out at ({}, {}) for RGB {:?}",
                    label.as_deref().unwrap_or("(no label)"),
                    x,
                    y,
                    rgb
                ));
            }
        }
        Action::MouseMove { v, h, at_tick } => {
            if let Some(tick) = at_tick {
                wait_for_script_tick(timing, *tick)?;
            }
            xdotool(&["mousemove", &h.to_string(), &v.to_string()])?;
        }
        Action::MouseDown { v, h, at_tick } => {
            if let Some(tick) = at_tick {
                wait_for_script_tick(timing, *tick)?;
            }
            xdotool(&["mousemove", &h.to_string(), &v.to_string()])?;
            xdotool(&["mousedown", "1"])?;
        }
        Action::MouseUp { v, h, at_tick } => {
            if let Some(tick) = at_tick {
                wait_for_script_tick(timing, *tick)?;
            }
            xdotool(&["mousemove", &h.to_string(), &v.to_string()])?;
            xdotool(&["mouseup", "1"])?;
        }
        Action::KeyDown { key, at_tick } => {
            if let Some(tick) = at_tick {
                wait_for_script_tick(timing, *tick)?;
            }
            let xkey = map_mac_key_to_x11(key);
            xdotool(&["keydown", &xkey])?;
        }
        Action::KeyUp { key, at_tick } => {
            if let Some(tick) = at_tick {
                wait_for_script_tick(timing, *tick)?;
            }
            let xkey = map_mac_key_to_x11(key);
            xdotool(&["keyup", &xkey])?;
        }
        Action::Screenshot { path, at_tick } => {
            if let Some(tick) = at_tick {
                wait_for_script_tick(timing, *tick)?;
            }
            let filename = path.clone().unwrap_or_else(|| format!("{:04}.png", idx));
            let sample = if state.reuse_probe_capture && timing.probe_shot.exists() {
                state.reuse_probe_capture = false;
                let s = state.probe_clock_sample.unwrap_or_else(|| {
                    read_oracle_clock(&timing.clock_path)
                        .ok()
                        .flatten()
                        .unwrap_or(OracleClockSample {
                            tick: 0,
                            retired_instructions: 0,
                        })
                });
                std::fs::copy(&timing.probe_shot, output_dir.join(&filename))
                    .map_err(|e| format!("copy probe: {}", e))?;
                s
            } else {
                let sample = capture_shot(output_dir, &filename, &timing.clock_path)?;
                sample
            };
            write_sheepshaver_capture_context(
                output_dir,
                &filename,
                timing.tick_zero,
                sample,
                !timing.wall_time,
            )?;
        }
        Action::RecordFrames { .. } => {
            return Err("record_frames is not supported by this adapter".into());
        }
        Action::Log { message } => {
            eprintln!("[PS] Log: {}", message);
        }
        Action::Milestone { name } => {
            eprintln!("[PS] Milestone: {}", name);
        }
        Action::AssertTickRange { min, max, label } => {
            let sample = timing_sample(timing)?;
            let script_tick = sample.tick.wrapping_sub(timing.tick_zero);
            if script_tick < *min || script_tick > *max {
                return Err(format!(
                    "AssertTickRange {} failed: script tick {} not in [{}..={}]",
                    label.as_deref().unwrap_or("(no label)"),
                    script_tick,
                    min,
                    max
                ));
            }
        }
        Action::AssertPixel {
            x,
            y,
            rgb,
            not,
            label,
        } => {
            let shot = output_dir.join(format!(
                "_assert_{}_{}.png",
                idx,
                label.as_deref().unwrap_or("")
            ));
            capture_host_shot(&shot, &timing.clock_path)?;
            let rgb8 = image::open(&shot).map_err(|e| e.to_string())?.to_rgb8();
            if *x >= rgb8.width() || *y >= rgb8.height() {
                return Err("assert_pixel coordinate outside capture".into());
            }
            let actual = rgb8.get_pixel(*x, *y).0;
            if !pixel_matches_within(actual, *rgb, *not, 0) {
                return Err(format!(
                    "assert_pixel failed: got {actual:?}, expected {rgb:?}"
                ));
            }
        }

        Action::AssertAudio { .. }
        | Action::AssertAudioDuring { .. }
        | Action::AssertMemoryWord { .. } => {
            return Err("assertion is not supported by this adapter".into())
        }
    }
    Ok(())
}

fn wait_script_ticks(timing: &TimingContext, delta_ticks: u32) -> Result<u32, String> {
    if timing.wall_time {
        std::thread::sleep(Duration::from_secs_f64(delta_ticks as f64 / 60.15));
        return Ok(0); // No observed guest tick in this explicitly selected mode.
    }

    let current_sample = read_oracle_clock(&timing.clock_path)?.unwrap_or(OracleClockSample {
        tick: timing.tick_zero,
        retired_instructions: 0,
    });
    let target = current_sample.tick.wrapping_add(delta_ticks);
    let reached_sample = wait_until_mac_tick(&timing.clock_path, target)?;
    Ok(reached_sample.tick.wrapping_sub(timing.tick_zero))
}

fn wait_for_script_tick(
    timing: &TimingContext,
    script_tick: u32,
) -> Result<OracleClockSample, String> {
    wait_until_mac_tick(
        &timing.clock_path,
        timing.tick_zero.wrapping_add(script_tick),
    )
}
fn timing_sample(timing: &TimingContext) -> Result<OracleClockSample, String> {
    read_oracle_clock(&timing.clock_path)?.ok_or_else(|| "guest clock is unavailable".into())
}

fn map_mac_key_to_x11(key: &str) -> String {
    match key.to_ascii_lowercase().as_str() {
        "return" | "enter" => "Return".to_string(),
        "space" => "space".to_string(),
        "tab" => "Tab".to_string(),
        "escape" | "esc" => "Escape".to_string(),
        "backspace" | "delete" => "BackSpace".to_string(),
        "left" => "Left".to_string(),
        "right" => "Right".to_string(),
        "up" => "Up".to_string(),
        "down" => "Down".to_string(),
        "f1" => "F1".to_string(),
        "f2" => "F2".to_string(),
        "f3" => "F3".to_string(),
        "f4" => "F4".to_string(),
        "f5" => "F5".to_string(),
        "f6" => "F6".to_string(),
        "f7" => "F7".to_string(),
        "f8" => "F8".to_string(),
        "f9" => "F9".to_string(),
        "f10" => "F10".to_string(),
        "f11" => "F11".to_string(),
        "f12" => "F12".to_string(),
        "." | "period" => "period".to_string(),
        "," | "comma" => "comma".to_string(),
        "/" | "slash" => "slash".to_string(),
        "\\" | "backslash" => "backslash".to_string(),
        ";" | "semicolon" => "semicolon".to_string(),
        "'" | "apostrophe" => "apostrophe".to_string(),
        "[" | "bracketleft" => "bracketleft".to_string(),
        "]" | "bracketright" => "bracketright".to_string(),
        "-" | "minus" => "minus".to_string(),
        "=" | "equal" => "equal".to_string(),
        "`" | "grave" => "grave".to_string(),
        // SheepShaver uses the same XFree86 keycode map as BasiliskII in
        // the oracle images: X11 Alt is classic Mac Command, while X11
        // Super/Logo is classic Mac Option.
        "cmd" | "command" => "Alt_L".to_string(),
        "shift" => "Shift_L".to_string(),
        "caps_lock" | "capslock" => "Caps_Lock".to_string(),
        "control" | "ctrl" => "Control_L".to_string(),
        "option" | "alt" => "Super_L".to_string(),
        other => other.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sheepshaver_container_run_disables_networking() {
        let args = sheepshaver_container_run_args(Path::new("/task"), Some("501:1000"));
        let network_modes: Vec<&str> = args
            .windows(2)
            .filter(|pair| pair[0] == "--network")
            .map(|pair| pair[1].as_str())
            .collect();

        assert_eq!(network_modes, vec!["none"]);
        assert!(!args.iter().any(|arg| arg.starts_with("--network=")));
    }

    #[test]
    fn sheepshaver_container_runs_as_scratch_owner_with_a_readable_home() {
        let args = sheepshaver_container_run_args(Path::new("/task"), Some("501:1000"));

        assert!(args.windows(2).any(|pair| pair == ["--user", "501:1000"]));
        assert!(args.windows(2).any(|pair| pair == ["-e", "HOME=/tmp"]));
        assert!(args
            .windows(2)
            .any(|pair| pair == ["-v", "/task/container_home:/root:ro"]));
    }

    #[test]
    fn play_prefs_insert_8bpp_depth_without_screen_suffix() {
        let prefs = rewrite_play_prefs(
            "# test prefs\nscreen win/640/480\nramsize 67108864\n",
            800,
            600,
            8,
            false,
        );

        assert!(prefs.contains("screen win/800/600"));
        assert!(prefs.contains("displaycolordepth 8"));
    }

    #[test]
    fn play_prefs_replace_existing_display_depth() {
        let prefs = rewrite_play_prefs(
            "screen win/1024/768\ndisplaycolordepth 0\nnogui true\n",
            640,
            480,
            16,
            false,
        );

        assert!(prefs.contains("screen win/640/480"));
        assert!(prefs.contains("displaycolordepth 16"));
        assert!(!prefs.contains("displaycolordepth 0"));
        assert_eq!(prefs.matches("displaycolordepth ").count(), 1);
    }

    #[test]
    fn strict_profile_disables_exception_and_68k_jit_convenience_flags() {
        let prefs = rewrite_play_prefs(
            "screen win/800/600\nignoresegv true\nignoreillegal true\njit68k true\n",
            800,
            600,
            8,
            true,
        );

        assert!(prefs.contains("ignoresegv false"));
        assert!(prefs.contains("ignoreillegal false"));
        assert!(prefs.contains("jit68k false"));
    }

    #[test]
    fn strict_profile_rejects_script_display_overrides() {
        let script: PlayScript =
            serde_json::from_str(r#"{"sheepshaver_screen_width":640,"actions":[]}"#).unwrap();

        assert!(reject_strict_script_overrides(true, &script).is_err());
        assert!(reject_strict_script_overrides(false, &script).is_ok());
    }

    #[test]
    fn stale_scratch_cleanup_errors_are_propagated() {
        let scratch = std::env::temp_dir().join(format!(
            "systemless-sheepshaver-scratch-file-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_file(&scratch);
        std::fs::write(&scratch, b"not a directory").unwrap();

        let error = remove_existing_scratch(&scratch).unwrap_err();

        assert!(error.contains("remove stale scratch directory"));
        std::fs::remove_file(scratch).unwrap();
    }

    #[test]
    fn pef_executable_detection() {
        let temp_dir = std::env::temp_dir().join(format!("pef_test_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&temp_dir);
        std::fs::create_dir_all(&temp_dir).unwrap();

        let pef_file = temp_dir.join("PpcApp");
        std::fs::write(&pef_file, b"Joy!peffpwpc").unwrap();
        assert!(is_pef_executable(&pef_file));

        let non_pef_file = temp_dir.join("68kApp");
        std::fs::write(&non_pef_file, b"NotPefBinary").unwrap();
        assert!(!is_pef_executable(&non_pef_file));

        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn prelaunch_create_dirs_writes_line_control_file() {
        let dir = std::env::temp_dir().join(format!(
            "systemless-sheepshaver-prelaunch-create-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();

        stage_prelaunch_create_dirs(&dir, Some(&["System Folder:Preferences".to_string()]))
            .unwrap();

        let file = dir.join(PRELAUNCH_CREATE_DIRS_FILE);
        assert_eq!(
            std::fs::read_to_string(file).unwrap(),
            "System Folder:Preferences"
        );
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn prelaunch_delete_paths_writes_line_control_file() {
        let dir = std::env::temp_dir().join(format!(
            "systemless-sheepshaver-prelaunch-delete-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();

        stage_prelaunch_delete_paths(
            &dir,
            Some(&["System Folder:Preferences:OldPref".to_string()]),
        )
        .unwrap();

        let file = dir.join(PRELAUNCH_DELETE_PATHS_FILE);
        assert_eq!(
            std::fs::read_to_string(file).unwrap(),
            "System Folder:Preferences:OldPref"
        );
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn oracle_clock_progress_allows_waits_longer_than_the_fixed_timeout() {
        let start = Instant::now();
        let timeout = Duration::from_secs(TICK_PROGRESS_TIMEOUT_SECS);
        let mut progress = OracleClockProgress::new(
            OracleClockSample {
                tick: 1_000,
                retired_instructions: 10_000,
            },
            start,
            timeout,
        );

        progress.observe(
            OracleClockSample {
                tick: 1_001,
                retired_instructions: 20_000,
            },
            start + Duration::from_secs(44),
        );
        progress.observe(
            OracleClockSample {
                tick: 1_002,
                retired_instructions: 30_000,
            },
            start + Duration::from_secs(88),
        );

        assert!(!progress.expired(start + Duration::from_secs(132)));
        assert!(progress.expired(start + Duration::from_secs(133)));
    }

    #[test]
    fn oracle_clock_progress_requires_ticks_and_instructions_to_advance() {
        let start = Instant::now();
        let timeout = Duration::from_secs(TICK_PROGRESS_TIMEOUT_SECS);
        let mut progress = OracleClockProgress::new(
            OracleClockSample {
                tick: 1_000,
                retired_instructions: 10_000,
            },
            start,
            timeout,
        );

        progress.observe(
            OracleClockSample {
                tick: 1_000,
                retired_instructions: 20_000,
            },
            start + Duration::from_secs(20),
        );
        progress.observe(
            OracleClockSample {
                tick: 1_001,
                retired_instructions: 10_000,
            },
            start + Duration::from_secs(40),
        );

        assert!(progress.expired(start + timeout));
    }

    #[test]
    fn key_mapping_translates_known_keys() {
        assert_eq!(map_mac_key_to_x11("Return"), "Return");
        assert_eq!(map_mac_key_to_x11("space"), "space");
        assert_eq!(map_mac_key_to_x11("Escape"), "Escape");
        assert_eq!(map_mac_key_to_x11("F1"), "F1");
        assert_eq!(map_mac_key_to_x11("f12"), "F12");
        assert_eq!(map_mac_key_to_x11("command"), "Alt_L");
        assert_eq!(map_mac_key_to_x11("option"), "Super_L");
        assert_eq!(map_mac_key_to_x11("period"), "period");
    }

    #[test]
    fn script_action_failures_stop_the_route_and_propagate_the_reason() {
        let actions = vec![
            Action::Log {
                message: "step 0".to_string(),
            },
            Action::Log {
                message: "step 1".to_string(),
            },
            Action::Log {
                message: "step 2".to_string(),
            },
        ];

        let failure = execute_script_actions(&actions, |index, _| match index {
            1 => Err("action 1 failed".to_string()),
            _ => Ok(()),
        })
        .unwrap_err();

        assert_eq!(
            failure,
            ScriptActionFailure {
                index: 1,
                message: "action 1 failed".to_string(),
            }
        );
    }

    #[test]
    fn successful_script_execution_reports_the_completed_action_count() {
        let actions = vec![
            Action::Log {
                message: "step 0".to_string(),
            },
            Action::Log {
                message: "step 1".to_string(),
            },
        ];

        let completed = execute_script_actions(&actions, |_, _| Ok(())).unwrap();

        assert_eq!(completed, 2);
    }
}
