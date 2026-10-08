//! Play script runner targeting BasiliskII inside a Docker container.
//!
//! Drives the same `PlayScript` JSON files as the Systemless runner (`play`)
//! against a real-Mac BasiliskII environment so the two runners can be
//! compared screenshot-for-screenshot. Intended as the ground-truth
//! reference for Systemless's HLE — whenever Systemless's output disagrees with
//! BasiliskII's, the BasiliskII output wins and a new contract test should
//! be authored to pin that behavior.
//!
//! Usage: cargo run --bin play-basilisk -- <game.sit> <script.json> [output-directory]
//!
//! Requirements:
//!   * `docker` on PATH with the `basiliskii-play:latest` image, typically
//!     built via `support/basiliskii/build_play_image.sh`
//!   * optional: set `SYSTEMLESS_BASILISK_IMAGE` to use a non-default image tag
//!   * `unar` on PATH (for resource-fork-preserving .sit extraction)
//!
//! Outputs screenshots to `<script_dir>/basilisk_output/` (parallel to the
//! Systemless `<script_dir>/output/` directory).

use std::collections::HashSet;
use std::io::{Read, Seek, SeekFrom};
use std::path::Component;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant};

use systemless_oracle_tools::docker::{build_mpw_builder_image, docker_mount_source};
use systemless_oracle_tools::mpw_builder::compile_with_mpw;
use systemless_oracle_tools::{
    basilisk_play_mac_time_secs_for_script, pixel_matches_within, realtime_instructions_per_tick,
    reject_oracle_environment_overrides, strict_oracle_run, Action, PlayScript,
};

const DEFAULT_DOCKER_IMAGE: &str = "basiliskii-play:latest";
const DEFAULT_DOCKER_CPUS: &str = "0.2";
const CONTAINER_NAME: &str = "systemless_basilisk_play";
const PRELAUNCH_CREATE_DIRS_FILE: &str = "_prelaunch_create_dirs";
const PRELAUNCH_DELETE_PATHS_FILE: &str = "_prelaunch_delete_paths";
const PRELAUNCH_BOOT_COPY_MANIFEST_FILE: &str = "_prelaunch_boot_copy_manifest";
const BOOT_WAIT_SECS: u64 = 30;
const TICK_PROGRESS_TIMEOUT_SECS: u64 = 45;
const POLL_INTERVAL_MILLIS: u64 = 50;
const CLOCK_POLL_INTERVAL_MILLIS: u64 = 2;

struct ScratchLayout {
    root: PathBuf,
    extfs_dir: PathBuf,
    probe_shot: PathBuf,
    /// Host path where the container writes its JSONL trap-trace stream
    /// when SYSTEMLESS_BASILISK_TRACE is set. Corresponds to
    /// `/mnt/shared/basilisk_trace.jsonl` inside the container.
    trace_path: PathBuf,
    /// Host-visible fixed-width clock sample written directly by BasiliskII.
    clock_path: PathBuf,
}

struct TimingContext {
    clock_path: PathBuf,
    tick_zero: u32,
    probe_shot: PathBuf,
    /// Same-path copy from ScratchLayout so screenshot handlers can read
    /// the tail Mac-tick from the trace stream for the capture sidecar.
    trace_path: PathBuf,
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
    std::env::var("SYSTEMLESS_BASILISK_IMAGE")
        .ok()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| {
            // If the host set SYSTEMLESS_BASILISK_TRACE, it only has effect
            // against the patched `:trace` image — `:latest` ignores the env
            // var silently. Auto-select `:trace` in that case so the caller
            // doesn't have to remember to export SYSTEMLESS_BASILISK_IMAGE.
            if std::env::var_os("SYSTEMLESS_BASILISK_TRACE").is_some() {
                "basiliskii-play:trace".to_string()
            } else {
                DEFAULT_DOCKER_IMAGE.to_string()
            }
        })
}

fn resolved_basilisk_mac_time(host_value: Option<String>, fallback_secs: u32) -> String {
    host_value
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| fallback_secs.to_string())
}

fn resolved_basilisk_deterministic_ticks(host_value: Option<String>) -> String {
    host_value
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| "1".to_string())
}

fn resolved_basilisk_instructions_per_tick(host_value: Option<String>) -> String {
    host_value
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| realtime_instructions_per_tick().to_string())
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
            "SYSTEMLESS_BASILISK_CPUS",
            "SYSTEMLESS_BASILISK_DETERMINISTIC_TICKS",
            "SYSTEMLESS_BASILISK_IMAGE",
            "SYSTEMLESS_BASILISK_INSTRUCTIONS_PER_TICK",
            "SYSTEMLESS_BASILISK_MAC_TIME",
            "SYSTEMLESS_BASILISK_TRACE",
            "SYSTEMLESS_CPU_MHZ",
            "SYSTEMLESS_PLAY_MAC_TIME",
        ])
        .expect("reject oracle BasiliskII overrides");
    }
    let game_path = PathBuf::from(&args[1]).canonicalize().expect("game path");
    let script_path = PathBuf::from(&args[2]).canonicalize().expect("script path");

    let script_text = std::fs::read_to_string(&script_path).expect("read script");
    let script: PlayScript = serde_json::from_str(&script_text).expect("parse script");
    systemless_oracle_tools::validate_script(&script_text, &script)
        .expect("validate oracle script");
    assert_eq!(
        script.clock.as_deref(),
        Some("guest_ticks"),
        "BasiliskII requires clock=guest_ticks"
    );
    let play_time_secs = basilisk_play_mac_time_secs_for_script(&script);
    systemless_oracle_tools::configure_load_from_script(&script);

    let script_dir = script_path.parent().expect("script dir");
    let output_dir = args
        .get(3)
        .map(PathBuf::from)
        .unwrap_or_else(|| script_dir.join("basilisk_output"));
    if output_dir.exists()
        && std::fs::read_dir(&output_dir)
            .expect("read output")
            .next()
            .is_some()
    {
        eprintln!("Oracle output directory must be empty");
        std::process::exit(1);
    }
    std::fs::create_dir_all(&output_dir).expect("create basilisk_output dir");

    let scratch = setup_scratch(&game_path, script.executable.as_deref(), play_time_secs)
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
    stage_boot_disk_copy_manifest(&scratch.extfs_dir, script.basilisk_stage_to_boot_disk)
        .expect("basilisk_stage_to_boot_disk");
    systemless_oracle_tools::install_bootstrap(
        Path::new(env!("CARGO_MANIFEST_DIR")),
        &scratch.root,
        "System_68K.dsk",
        &docker_image(),
    )
    .expect("install startup launcher");
    let _guard =
        ContainerGuard::start(&scratch.root, play_time_secs).expect("start basilisk container");
    if script_uses_keypad_digit_alias(&script) {
        prime_x11_numlock_for_keypad_aliases().expect("prime X11 NumLock for keypad aliases");
    }

    let tick_baseline_path = scratch.extfs_dir.join("_tick_baseline");
    eprintln!(
        "[PB] Waiting up to {}s for MacOS boot + PlayLauncher baseline…",
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
    eprintln!("[PB] Launcher baseline tick: {}", tick_baseline);

    let boot_delay_ticks = script.boot_delay_ticks.unwrap_or(0);
    let tick_zero = tick_baseline.wrapping_add(boot_delay_ticks);
    wait_for_live_oracle_clock(&scratch.clock_path, tick_baseline)
        .expect("live BasiliskII guest clock");
    eprintln!("[PB] Live emulator clock detected");
    if boot_delay_ticks > 0 {
        eprintln!(
            "[PB] Burning {} loader tick(s) to align script tick 0",
            boot_delay_ticks
        );
        wait_until_mac_tick(&scratch.clock_path, tick_zero).expect("boot delay burn");
    }
    capture_shot(&output_dir, "01_post_launch.png", &scratch.clock_path).ok();

    let timing = TimingContext {
        clock_path: scratch.clock_path.clone(),
        tick_zero,
        probe_shot: scratch.probe_shot.clone(),
        trace_path: scratch.trace_path.clone(),
    };

    let mut script_state = ScriptState::default();
    match execute_script_actions(&script.actions, |index, action| {
        execute_action(index, action, &output_dir, &timing, &mut script_state)
    }) {
        Ok(completed) => {
            eprintln!("[PB] Script complete after {} action(s)", completed);
        }
        Err(failure) => {
            eprintln!(
                "[PB] Script failed at action {}: {}",
                failure.index, failure.message
            );
            drop(_guard);
            std::process::exit(1);
        }
    }

    // Container guard stops the container on drop.
}

/// Prepare a scratch directory with:
///   scratch/shared/rom/MacRom.rom
///   scratch/shared/disk/System_68K.dsk  (gunzipped)
///   scratch/extfs/_PlayTarget + FixtureGen + launcher control files
///   scratch/prefs
fn setup_scratch(
    game_path: &Path,
    executable_override: Option<&str>,
    play_time_secs: u32,
) -> Result<ScratchLayout, String> {
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let scratch = tempfile::Builder::new()
        .prefix("systemless-basilisk-")
        .tempdir()
        .map_err(|e| e.to_string())?
        .keep();
    // Clean and re-create.
    remove_existing_scratch(&scratch)?;
    std::fs::create_dir_all(scratch.join("shared/rom")).map_err(|e| format!("mkdir rom: {}", e))?;
    std::fs::create_dir_all(scratch.join("shared/disk"))
        .map_err(|e| format!("mkdir disk: {}", e))?;
    std::fs::create_dir_all(scratch.join("extfs")).map_err(|e| format!("mkdir extfs: {}", e))?;
    std::fs::create_dir_all(scratch.join("container_home"))
        .map_err(|e| format!("mkdir container home: {}", e))?;

    // Resolve support/basiliskii paths relative to this crate.

    // Copy ROM.
    let rom_src = PathBuf::from(
        std::env::var_os("SYSTEMLESS_BASILISK_ROM")
            .ok_or("set SYSTEMLESS_BASILISK_ROM to your ROM file")?,
    );
    std::fs::copy(&rom_src, scratch.join("shared/rom/MacRom.rom"))
        .map_err(|e| format!("copy rom: {}", e))?;

    // Decompress disk image.
    let disk_gz = PathBuf::from(
        std::env::var_os("SYSTEMLESS_BASILISK_DISK")
            .ok_or("set SYSTEMLESS_BASILISK_DISK to your gzip system disk")?,
    );
    let disk_out = scratch.join("shared/disk/System_68K.dsk");
    let input = std::fs::File::open(&disk_gz).map_err(|e| e.to_string())?;
    let mut decoder = flate2::read::GzDecoder::new(input);
    let mut output = std::fs::File::create(&disk_out).map_err(|e| e.to_string())?;
    std::io::copy(&mut decoder, &mut output).map_err(|e| e.to_string())?;

    // Extract the game archive via unar on the host, then convert the macOS
    // xattrs (com.apple.FinderInfo / com.apple.ResourceFork) into the
    // BasiliskII extfs sidecar format (.finf/FILE for metadata,
    // .rsrc/FILE for resource fork). This is required because:
    //   (a) macOS-native xattrs don't survive the bind-mount into the
    //       container's Linux extfs, so BasiliskII sees the extracted
    //       application as a document without a creator;
    //   (b) letting StuffIt Expander inside BasiliskII expand the archive
    //       hits a "File Not Found" disk error — BasiliskII's extfs can't
    //       create the resource fork sidecars StuffIt needs during
    //       expansion in an already-populated mount.
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

    // Multi-disk installers can store the actual application in a compressed
    // payload whose Finder type is not APPL until the generic game loader has
    // expanded it. When a script explicitly names such an application, stage
    // the loader's fork-preserving VFS rather than accidentally launching the
    // installer application that happened to be visible in the raw images.
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

    // Linux `unar` can emit resource-fork-only files as `<name>.rsrc`
    // without a sibling data fork. BasiliskII extfs expects the logical
    // file name to exist (even if the data fork is empty), so create
    // zero-byte placeholders now before app detection / renaming.
    if !used_loaded_archive_staging {
        materialize_resource_only_data_forks(&extfs_dir)?;
    }

    // Flatten the extracted game folder into `extfs_dir` and rename the
    // main application to `_PlayTarget`. The Startup Items launcher on the
    // boot disk launches `Unix:FixtureGen`; we stage a dedicated PlayLauncher
    // under that name so it can seed time, write tick files, and then launch
    // the actual game in a deterministic way.
    //
    //   1. System_68K.dsk has a permanent `Launcher.bin` pre-installed in
    //      Mac OS 8.1 Startup Items that runs on boot.
    //   2. That launcher calls LaunchApplication on `Unix:FixtureGen`.
    //   3. `FixtureGen` is our host-built PlayLauncher, which launches
    //      `Unix:_PlayTarget` and reports `_tick_baseline` / `_ticks`.
    //   4. The real game keeps all its sibling resources (`EV Data`,
    //      `EV Graphics`,
    //      sub-folders like `EV Plug-Ins`, etc.) in the same directory
    //      so the running app finds them via its normal relative lookups.
    flatten_and_rename_main_app(&extfs_dir, "_PlayTarget", executable_override)?;

    if !used_loaded_archive_staging {
        convert_xattrs_to_basilisk_sidecars(&extfs_dir)?;
    }
    ensure_play_target_finf(&extfs_dir)?;
    stage_play_launcher(&manifest, &scratch, &extfs_dir)?;
    write_u32_be_file(&extfs_dir.join("_play_time"), play_time_secs)?;

    // Stage prefs file: the .play variant ships with 32MB RAM (full games
    // like Escape Velocity and Marathon refuse to launch in 8MB) and a
    // live sound channel (Marathon allocates one on startup). We write a
    // fresh copy so we can tweak screen size without mutating the
    // canonical fixture.
    let prefs_src = manifest
        .join("support/basiliskii")
        .join(".basilisk_ii_prefs.play");
    let prefs_raw =
        std::fs::read_to_string(&prefs_src).map_err(|e| format!("read prefs: {}", e))?;
    // Force 800x600 8bpp to match the Systemless default screen mode.
    let prefs = rewrite_play_prefs_for_800x600_8bpp(&prefs_raw);
    std::fs::write(scratch.join("container_home/.basilisk_ii_prefs"), prefs)
        .map_err(|e| format!("write prefs: {}", e))?;

    eprintln!("[PB] Scratch staged at {}", scratch.display());
    Ok(ScratchLayout {
        root: scratch.clone(),
        extfs_dir: extfs_dir.clone(),
        probe_shot: scratch.join("probe_pixel.png"),
        trace_path: scratch.join("shared/basilisk_trace.jsonl"),
        clock_path: scratch.join("shared/basilisk_clock"),
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

fn rewrite_play_prefs_for_800x600_8bpp(prefs_raw: &str) -> String {
    let mut saw_display_depth = false;
    let mut lines = Vec::new();

    for line in prefs_raw.lines() {
        let trimmed = line.trim_start();
        if trimmed.starts_with("screen ") {
            lines.push("screen win/800/600".to_string());
        } else if trimmed.starts_with("displaycolordepth ") {
            saw_display_depth = true;
            lines.push("displaycolordepth 8".to_string());
        } else {
            lines.push(line.to_string());
        }
    }

    if !saw_display_depth {
        lines.push("displaycolordepth 8".to_string());
    }

    lines.join("\n")
}

/// Extract supported HFS/DC42 files nested inside the host archive into the
/// extfs tree before application selection. The public Systemless disk-image
/// reader supplies the original data/resource forks and Finder metadata; this
/// function only lowers that payload into BasiliskII's sidecar layout.
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
            "[PB] Extracted nested disk image {} as volume {:?} ({} files)",
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
        .map_err(|e| format!("load archive for BasiliskII staging: {}", e))?;
    let snapshots = runner
        .vfs_file_summaries()
        .into_iter()
        .filter_map(|summary| runner.vfs_file_snapshot(&summary.path))
        .collect::<Vec<_>>();
    if snapshots.is_empty() {
        return Err("generic game loader produced no files for BasiliskII staging".into());
    }

    let mut installer_destinations = snapshots
        .iter()
        .filter(|file| file.file_type.to_be_bytes() == *b"bbkr")
        .flat_map(|file| pascal_installer_destinations(&file.resource_fork))
        .collect::<Vec<_>>();
    if let Some(install_root) = installer_destinations.iter().find_map(|destination| {
        let destination_name = destination.rsplit('/').next()?;
        snapshots
            .iter()
            .find(|file| {
                file.file_type.to_be_bytes() == *b"APPL"
                    && file.path.rsplit('/').next() == Some(destination_name)
            })
            .and_then(|_| destination.split('/').next())
            .map(str::to_owned)
    }) {
        installer_destinations
            .retain(|destination| destination.split('/').next() == Some(install_root.as_str()));
    } else {
        installer_destinations.clear();
    }
    let files_to_materialize = if installer_destinations.is_empty() {
        snapshots
            .iter()
            .map(|file| Ok((validated_archive_relative_path(&file.path)?, file)))
            .collect::<Result<Vec<_>, String>>()?
    } else {
        installer_destinations
            .iter()
            .filter_map(|destination| {
                let destination_name = destination.rsplit('/').next()?;
                let source_name = destination_name
                    .strip_suffix(".rsrc")
                    .unwrap_or(destination_name);
                snapshots
                    .iter()
                    .find(|file| {
                        let file_name = file.path.rsplit('/').next();
                        file_name == Some(destination_name) || file_name == Some(source_name)
                    })
                    .map(|file| (PathBuf::from(destination), file))
            })
            .collect::<Vec<_>>()
    };
    if files_to_materialize.is_empty() {
        return Err("installer manifest matched no expanded files for BasiliskII staging".into());
    }

    std::fs::remove_dir_all(root)
        .map_err(|e| format!("clear raw installer staging {}: {}", root.display(), e))?;
    let materialized_root = root.join("Materialized Game");
    std::fs::create_dir_all(&materialized_root)
        .map_err(|e| format!("create {}: {}", materialized_root.display(), e))?;

    let file_count = files_to_materialize.len();
    for (relative, file) in files_to_materialize {
        let squz_placeholder = file.file_type.to_be_bytes() == *b"SQUZ";
        let placeholder_resource_fork;
        let data_fork = if squz_placeholder {
            &[][..]
        } else {
            file.data_fork.as_slice()
        };
        let resource_fork = if squz_placeholder {
            placeholder_resource_fork = empty_resource_fork_bytes();
            placeholder_resource_fork.as_slice()
        } else {
            file.resource_fork.as_slice()
        };
        let file_type = if squz_placeholder && file.data_fork.len() >= 12 {
            u32::from_be_bytes(file.data_fork[4..8].try_into().unwrap())
        } else {
            file.file_type
        };
        let creator = if squz_placeholder && file.data_fork.len() >= 12 {
            u32::from_be_bytes(file.data_fork[8..12].try_into().unwrap())
        } else {
            file.creator
        };
        let path = materialized_root.join(relative);
        let parent = path
            .parent()
            .ok_or_else(|| format!("materialized file has no parent: {}", path.display()))?;
        std::fs::create_dir_all(parent)
            .map_err(|e| format!("create materialized parent {}: {}", parent.display(), e))?;
        std::fs::write(&path, data_fork)
            .map_err(|e| format!("write materialized data fork {}: {}", path.display(), e))?;

        let finf_path = extfs_sidecar_path(&path, ".finf")?;
        let finf_parent = finf_path
            .parent()
            .ok_or_else(|| format!("Finder-info sidecar has no parent: {}", finf_path.display()))?;
        std::fs::create_dir_all(finf_parent)
            .map_err(|e| format!("create {}: {}", finf_parent.display(), e))?;
        let mut finf = [0u8; 32];
        finf[0..4].copy_from_slice(&file_type.to_be_bytes());
        finf[4..8].copy_from_slice(&creator.to_be_bytes());
        finf[8..10].copy_from_slice(&file.finder_flags.to_be_bytes());
        std::fs::write(&finf_path, finf)
            .map_err(|e| format!("write Finder-info sidecar {}: {}", finf_path.display(), e))?;

        if !resource_fork.is_empty() {
            let rsrc_path = extfs_sidecar_path(&path, ".rsrc")?;
            let rsrc_parent = rsrc_path.parent().ok_or_else(|| {
                format!(
                    "resource-fork sidecar has no parent: {}",
                    rsrc_path.display()
                )
            })?;
            std::fs::create_dir_all(rsrc_parent)
                .map_err(|e| format!("create {}: {}", rsrc_parent.display(), e))?;
            std::fs::write(&rsrc_path, resource_fork)
                .map_err(|e| format!("write resource fork {}: {}", rsrc_path.display(), e))?;
        }
    }

    eprintln!(
        "[PB] Materialized {} generically expanded files for BasiliskII",
        file_count
    );
    Ok(())
}

fn pascal_installer_destinations(bytes: &[u8]) -> Vec<String> {
    let mut destinations = Vec::new();
    for offset in 0..bytes.len() {
        let len = bytes[offset] as usize;
        let Some(end) = offset.checked_add(1 + len) else {
            continue;
        };
        if len < 3 || end > bytes.len() {
            continue;
        }
        let text = &bytes[offset + 1..end];
        if !text
            .iter()
            .all(|byte| byte.is_ascii_graphic() || *byte == b' ')
        {
            continue;
        }
        let Ok(text) = std::str::from_utf8(text) else {
            continue;
        };
        if !text.starts_with(':') || text.matches(':').count() < 2 {
            continue;
        }
        let components = text
            .split(':')
            .filter(|component| !component.is_empty())
            .collect::<Vec<_>>();
        if components.len() < 2
            || components
                .iter()
                .any(|component| *component == "." || *component == ".." || component.contains('/'))
        {
            continue;
        }
        let destination = components.join("/");
        if !destinations.contains(&destination) {
            destinations.push(destination);
        }
    }
    destinations
}

fn empty_resource_fork_bytes() -> Vec<u8> {
    let mut bytes = vec![0u8; 48];
    let header = [
        0, 0, 0, 16, // data offset
        0, 0, 0, 16, // map offset
        0, 0, 0, 0, // data length
        0, 0, 0, 32, // map length
    ];
    bytes[0..16].copy_from_slice(&header);
    bytes[16..32].copy_from_slice(&header);
    bytes[40..42].copy_from_slice(&30u16.to_be_bytes());
    bytes[42..44].copy_from_slice(&32u16.to_be_bytes());
    bytes[44..46].copy_from_slice(&0xffffu16.to_be_bytes());
    bytes
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

/// After `unar` has populated `extfs_dir` with a single game folder,
/// move the folder's contents up into `extfs_dir` itself and rename the
/// main application to `target_app_name`.
///
/// The "main application" is identified by scanning for files whose
/// `com.apple.FinderInfo` xattr starts with the FourCC `APPL`. If more
/// than one application is present (a common case — e.g. Escape Velocity
/// ships alongside a `Register Escape Velocity` helper), we pick the
/// candidate whose file name is the longest substring of the enclosing
/// folder name (case-insensitive). That heuristic is deliberately
/// conservative: for `Escape Velocity 1.0.5 ƒ/` it picks
/// `Escape Velocity` rather than `Register Escape Velocity`.
fn flatten_and_rename_main_app(
    extfs_dir: &Path,
    target_app_name: &str,
    executable_override: Option<&str>,
) -> Result<(), String> {
    // .sit archives unpack a single application file directly
    // under extfs_dir (no wrapping folder) — Mars Rising,
    // Missile Command, Stunt Copter all do this. Detect that
    // case and treat extfs_dir itself as the game folder.
    let subdirs: Vec<PathBuf> = std::fs::read_dir(extfs_dir)
        .map_err(|e| format!("read extfs_dir: {}", e))?
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| p.is_dir())
        .collect();
    let game_folder = if subdirs.len() == 1 {
        subdirs.into_iter().next().unwrap()
    } else if subdirs.is_empty() {
        // Single-file extraction: the application IS at the top level
        // of extfs_dir. Use extfs_dir itself as the "game folder" and
        // skip the move loop later (entries are already in the right
        // place).
        eprintln!(
            "[PB] No subdirectory under {}; treating extfs_dir as the game folder",
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
            "no APPL application found in {}",
            game_folder.display()
        ));
    }
    // PlayScript `executable` substring match takes precedence over
    // auto-detection. POD ships both `MARS™ Player` and `MARS™ Master (sw)`;
    // without honoring the script override the auto-detect picks Player
    // (the network-only client) and gameplay scripts targeting Master
    // never reach the right binary.
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
                let exact_rank = if *name == wanted_lower {
                    0usize
                } else {
                    1usize
                };
                let starts_rank = if name.starts_with(&wanted_lower) {
                    0usize
                } else {
                    1usize
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
        // Prefer the longest file name that is a substring of the
        // folder name. Fall back to the shortest-named app if nothing
        // matches.
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
            // (e.g., "King of Parking 1.1" under "King of Parking"),
            // score candidates by alpha-word overlap against the folder
            // name. This avoids choosing short metadata apps like
            // "Icon\r" simply because they have the shortest filename.
            let mut scored: Vec<(PathBuf, usize, usize, usize)> = appl_candidates
                .iter()
                .map(|path| {
                    let fname_lower = path
                        .file_name()
                        .map(|n| n.to_string_lossy().to_lowercase())
                        .unwrap_or_default();
                    let (overlap, extra_words, name_len) =
                        score_app_name(&folder_words, &fname_lower);
                    (path.clone(), overlap, extra_words, name_len)
                })
                .collect();
            scored.sort_by(|a, b| {
                b.1.cmp(&a.1) // more overlap first
                    .then_with(|| a.2.cmp(&b.2)) // fewer non-matching words
                    .then_with(|| a.3.cmp(&b.3)) // shorter name on tie
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
                return Err(format!("no APPL candidates in {}", game_folder.display()));
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
        "[PB] Main application detected: {}",
        main_app_name.to_string_lossy()
    );

    // Move every entry from the selected app's containing folder into `extfs_dir`,
    // that source folder is `extfs_dir` (single-file extraction case) we
    // only need to rename the main app in-place; no move-up loop.
    let source_folder = main_app.parent().unwrap_or(&game_folder);
    if source_folder != game_folder {
        eprintln!(
            "[PB] Flattening nested application folder: {}",
            source_folder.display()
        );
    }
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
            // Already in place; skip.
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
                return Err(format!(
                    "unexpected non-file extfs sidecar {}",
                    from.display()
                ));
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

fn collect_appl_candidates(root: &Path, recursive: bool) -> Result<Vec<PathBuf>, String> {
    let mut appl_candidates: Vec<PathBuf> = Vec::new();
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

    if appl_candidates.is_empty() {
        // Linux `unar` can materialize forks as sibling `*.rsrc` files
        // rather than xattrs. In that mode FinderInfo/APPL is absent, so
        // use the "has resource fork companion" set and keep the existing
        // name-based chooser below.
        Ok(forked_file_candidates)
    } else {
        Ok(appl_candidates)
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

fn score_app_name(
    folder_words: &HashSet<String>,
    candidate_name_lower: &str,
) -> (usize, usize, usize) {
    let candidate_words = tokenize_alpha_words(candidate_name_lower);
    let overlap = candidate_words
        .iter()
        .filter(|word| folder_words.contains(*word))
        .count();
    let extra_words = candidate_words.len().saturating_sub(overlap);
    (overlap, extra_words, candidate_name_lower.len())
}

/// Walk `root` and materialize empty data forks for any `*.rsrc` file that
/// lacks a same-name sibling without the suffix.
///
/// Example:
///   `Koji The Frog 2.0.1.rsrc` exists, `Koji The Frog 2.0.1` missing
///   => create empty `Koji The Frog 2.0.1`.
///
/// This keeps resource-only executables discoverable by the `_PlayTarget`
/// chooser and by BasiliskII's extfs sidecar loader.
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
                // Skip Basilisk sidecar dirs if present in re-used trees.
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
        eprintln!("[PB] Materialized {} resource-only data fork(s)", created);
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
        "[PB] Building PlayLauncher from {}",
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

/// Honour the script's `remove_paths` field by deleting each entry from
/// the extracted game folder. Also removes the matching BasiliskII extfs
/// sidecars under `.finf/` and `.rsrc/` so a half-deleted entry doesn't
/// linger (BasiliskII would still see the metadata sidecar even after
/// the data fork is gone).
fn apply_remove_paths(extfs_dir: &Path, paths: &[String]) -> Result<(), String> {
    for rel in paths {
        let target = extfs_dir.join(rel);
        if target.is_dir() {
            std::fs::remove_dir_all(&target)
                .map_err(|e| format!("remove_paths: rmdir {}: {}", target.display(), e))?;
            eprintln!("[PB] Removed directory {}", target.display());
        } else if target.is_file() {
            std::fs::remove_file(&target)
                .map_err(|e| format!("remove_paths: rm {}: {}", target.display(), e))?;
            eprintln!("[PB] Removed file {}", target.display());
        } else if target.exists() {
            return Err(format!(
                "remove_paths: {} exists but is neither a file nor a directory",
                target.display()
            ));
        } else {
            eprintln!(
                "[PB] remove_paths: {} not present (skipping)",
                target.display()
            );
            continue;
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
    let mut fork =
        std::fs::read(&rsrc_path).map_err(|e| format!("read {}: {}", rsrc_path.display(), e))?;
    let patches = patch_application_size_resource_partition(&mut fork, bytes)?;
    if patches.is_empty() {
        return Err(format!(
            "{} has no SIZE resources to patch for application_partition_size",
            rsrc_path.display()
        ));
    }
    let patched_ids = patches
        .iter()
        .map(|(id, old_preferred, old_minimum)| {
            format!("{id}: {old_preferred}/{old_minimum} -> {bytes}")
        })
        .collect::<Vec<_>>()
        .join(", ");
    std::fs::write(&rsrc_path, &fork)
        .map_err(|e| format!("write {}: {}", rsrc_path.display(), e))?;
    eprintln!(
        "[PB] Patched _PlayTarget SIZE partition resources: {}",
        patched_ids
    );
    Ok(())
}

fn patch_application_size_resource_partition(
    fork: &mut [u8],
    partition_size: u32,
) -> Result<Vec<(i16, u32, u32)>, String> {
    if fork.len() < 16 {
        return Err("resource fork is shorter than its header".to_string());
    }

    let data_offset = read_u32_be(fork, 0)? as usize;
    let map_offset = read_u32_be(fork, 4)? as usize;
    let data_length = read_u32_be(fork, 8)? as usize;
    let map_length = read_u32_be(fork, 12)? as usize;
    checked_range(fork.len(), data_offset, data_length, "resource data")?;
    checked_range(fork.len(), map_offset, map_length, "resource map")?;
    if map_length < 30 {
        return Err("resource map is shorter than its fixed header".to_string());
    }

    let type_list_offset = read_u16_be(fork, map_offset + 24)? as usize;
    let num_types = read_u16_be(fork, map_offset + 28)? as usize + 1;
    let type_list_abs = map_offset
        .checked_add(type_list_offset)
        .ok_or_else(|| "resource type list offset overflow".to_string())?;
    checked_range(fork.len(), type_list_abs, 2, "resource type list count")?;

    let mut patches = Vec::new();
    for i in 0..num_types {
        let type_entry = type_list_abs
            .checked_add(2 + i * 8)
            .ok_or_else(|| "resource type entry offset overflow".to_string())?;
        checked_range(fork.len(), type_entry, 8, "resource type entry")?;
        if &fork[type_entry..type_entry + 4] != b"SIZE" {
            continue;
        }

        let num_resources = read_u16_be(fork, type_entry + 4)? as usize + 1;
        let ref_list_offset = read_u16_be(fork, type_entry + 6)? as usize;
        let ref_list_abs = type_list_abs
            .checked_add(ref_list_offset)
            .ok_or_else(|| "resource reference list offset overflow".to_string())?;
        for j in 0..num_resources {
            let ref_entry = ref_list_abs
                .checked_add(j * 12)
                .ok_or_else(|| "resource reference entry offset overflow".to_string())?;
            checked_range(fork.len(), ref_entry, 12, "resource reference entry")?;
            let id = i16::from_be_bytes([fork[ref_entry], fork[ref_entry + 1]]);
            let attrs = fork[ref_entry + 4];
            if attrs & 0x01 != 0 {
                return Err(format!(
                    "SIZE {} is compressed and cannot be patched in place",
                    id
                ));
            }
            let res_data_offset = ((fork[ref_entry + 5] as usize) << 16)
                | ((fork[ref_entry + 6] as usize) << 8)
                | fork[ref_entry + 7] as usize;
            let length_offset = data_offset
                .checked_add(res_data_offset)
                .ok_or_else(|| "SIZE data offset overflow".to_string())?;
            checked_range(fork.len(), length_offset, 4, "SIZE length")?;
            let res_len = read_u32_be(fork, length_offset)? as usize;
            if res_len < 10 {
                return Err(format!("SIZE {} is too short: {} bytes", id, res_len));
            }
            let res_data = length_offset
                .checked_add(4)
                .ok_or_else(|| "SIZE data start overflow".to_string())?;
            checked_range(fork.len(), res_data, res_len, "SIZE data")?;
            let old_preferred = read_u32_be(fork, res_data + 2)?;
            let minimum = read_u32_be(fork, res_data + 6)?;
            fork[res_data + 2..res_data + 6].copy_from_slice(&partition_size.to_be_bytes());
            fork[res_data + 6..res_data + 10].copy_from_slice(&partition_size.to_be_bytes());
            patches.push((id, old_preferred, minimum));
        }
    }

    Ok(patches)
}

fn checked_range(total_len: usize, start: usize, len: usize, label: &str) -> Result<(), String> {
    let end = start
        .checked_add(len)
        .ok_or_else(|| format!("{label} range overflow"))?;
    if end > total_len {
        Err(format!(
            "{label} range {}..{} exceeds resource fork length {}",
            start, end, total_len
        ))
    } else {
        Ok(())
    }
}

fn write_u32_be_file(path: &Path, value: u32) -> Result<(), String> {
    std::fs::write(path, value.to_be_bytes())
        .map_err(|e| format!("write {}: {}", path.display(), e))
}

fn stage_prelaunch_create_dirs(extfs_dir: &Path, paths: Option<&[String]>) -> Result<(), String> {
    stage_prelaunch_path_list(
        extfs_dir,
        paths,
        PRELAUNCH_CREATE_DIRS_FILE,
        "prelaunch_create_dirs",
        "directory request",
    )
}

fn stage_prelaunch_delete_paths(extfs_dir: &Path, paths: Option<&[String]>) -> Result<(), String> {
    stage_prelaunch_path_list(
        extfs_dir,
        paths,
        PRELAUNCH_DELETE_PATHS_FILE,
        "prelaunch_delete_paths",
        "delete request",
    )
}

fn stage_boot_disk_copy_manifest(extfs_dir: &Path, enabled: bool) -> Result<(), String> {
    let manifest_path = extfs_dir.join(PRELAUNCH_BOOT_COPY_MANIFEST_FILE);
    if !enabled {
        let _ = std::fs::remove_file(&manifest_path);
        return Ok(());
    }

    let mut lines = Vec::new();
    collect_boot_disk_copy_entries(extfs_dir, Path::new(""), &mut lines)?;
    let mut body = String::new();
    for line in &lines {
        body.push_str(line);
        body.push('\n');
    }
    std::fs::write(&manifest_path, body)
        .map_err(|e| format!("write {}: {}", manifest_path.display(), e))?;
    eprintln!(
        "[PB] Staged boot-disk copy manifest with {} entries",
        lines.len()
    );
    Ok(())
}

fn collect_boot_disk_copy_entries(
    dir: &Path,
    rel: &Path,
    lines: &mut Vec<String>,
) -> Result<(), String> {
    let mut children = std::fs::read_dir(dir)
        .map_err(|e| format!("read {}: {}", dir.display(), e))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| format!("read_dir entry {}: {}", dir.display(), e))?;
    children.sort_by_key(|entry| entry.file_name().to_string_lossy().into_owned());

    for entry in children {
        let name = entry.file_name();
        let name = name
            .to_str()
            .ok_or_else(|| format!("non-UTF-8 staged path under {}", dir.display()))?;
        if should_skip_boot_disk_copy_entry(rel, name) {
            continue;
        }

        let child_rel = rel.join(name);
        let child_path = entry.path();
        let mac_rel = boot_copy_mac_relative_path(&child_rel)?;
        let file_type = entry
            .file_type()
            .map_err(|e| format!("file_type {}: {}", child_path.display(), e))?;
        if file_type.is_dir() {
            lines.push(format!("D\t{mac_rel}"));
            collect_boot_disk_copy_entries(&child_path, &child_rel, lines)?;
        } else if file_type.is_file() {
            lines.push(format!("F\t{mac_rel}"));
        }
    }
    Ok(())
}

fn should_skip_boot_disk_copy_entry(parent_rel: &Path, name: &str) -> bool {
    if name == ".rsrc" || name == ".finf" {
        return true;
    }
    if name == "Icon\r" {
        return true;
    }
    if parent_rel.as_os_str().is_empty()
        && (name == "FixtureGen"
            || name == "_play_time"
            || name == "_ticks"
            || name == "_tick_baseline"
            || name.starts_with("_prelaunch_"))
    {
        return true;
    }
    false
}

fn boot_copy_mac_relative_path(rel: &Path) -> Result<String, String> {
    let mut parts = Vec::new();
    for component in rel.components() {
        let std::path::Component::Normal(part) = component else {
            return Err(format!(
                "unsupported staged relative path {}",
                rel.display()
            ));
        };
        let part = part
            .to_str()
            .ok_or_else(|| format!("non-UTF-8 staged relative path {}", rel.display()))?;
        if part.is_empty()
            || part.contains(':')
            || part.contains('\r')
            || part.contains('\n')
            || part.contains('\t')
        {
            return Err(format!(
                "staged path component {:?} cannot be represented in boot-copy manifest",
                part
            ));
        }
        parts.push(part);
    }
    if parts.is_empty() {
        Err("empty staged relative path".to_string())
    } else {
        Ok(parts.join(":"))
    }
}

fn stage_prelaunch_path_list(
    extfs_dir: &Path,
    paths: Option<&[String]>,
    control_file: &str,
    field_name: &str,
    log_name: &str,
) -> Result<(), String> {
    let Some(paths) = paths else {
        return Ok(());
    };
    if paths.is_empty() {
        return Ok(());
    }

    let mut body = String::new();
    for path in paths {
        let trimmed = path.trim();
        if trimmed.is_empty() {
            return Err(format!("{field_name} contains an empty path"));
        }
        if trimmed.contains('\r') || trimmed.contains('\n') {
            return Err(format!("{field_name} path contains a newline: {path:?}"));
        }
        body.push_str(trimmed);
        body.push('\n');
    }

    let control_path = extfs_dir.join(control_file);
    std::fs::write(&control_path, body)
        .map_err(|e| format!("write {}: {}", control_path.display(), e))?;
    eprintln!(
        "[PB] Staged {} prelaunch {}(s) in {}",
        paths.len(),
        log_name,
        control_path.display()
    );
    Ok(())
}

/// Walk a directory tree produced by `unar` on macOS and convert each
/// file's macOS-native Mac metadata xattrs into the sidecar format that
/// BasiliskII's Linux extfs driver expects.
///
/// BasiliskII extfs format (per macemu src/Unix/extfs_unix.cpp):
///   * File data fork:      `<dir>/FILE`
///   * File resource fork:  `<dir>/.rsrc/FILE`
///   * File Finder info:    `<dir>/.finf/FILE`  (32 bytes: 16 FInfo + 16 FXInfo)
///
/// macOS xattrs produced by `unar`:
///   * `com.apple.FinderInfo`  — 32 bytes (16 FInfo + 16 FXInfo)
///   * `com.apple.ResourceFork` — raw resource fork bytes
///
/// Read xattrs directly via the filesystem API (`xattr` crate) so this
/// works on Linux CI/containers where the macOS `xattr` CLI is absent.
fn convert_xattrs_to_basilisk_sidecars(root: &Path) -> Result<(), String> {
    let mut converted = 0usize;
    let mut stack: Vec<PathBuf> = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let entries =
            std::fs::read_dir(&dir).map_err(|e| format!("read_dir {}: {}", dir.display(), e))?;
        // Collect entries first to avoid iterating while mutating the dir.
        let mut file_paths: Vec<PathBuf> = Vec::new();
        for entry in entries {
            let entry = entry.map_err(|e| format!("read entry: {}", e))?;
            let path = entry.path();
            let file_name = entry.file_name();
            let name_str = file_name.to_string_lossy();
            // Skip the sidecar directories we're about to create so the
            // recursive walk doesn't descend into them.
            if name_str == ".finf" || name_str == ".rsrc" {
                continue;
            }
            // Linux `unar` fallback mode stores forks in sibling
            // `<name>.rsrc` files. Those are source fork payloads,
            // not logical game files to convert recursively.
            if name_str.ends_with(".rsrc") {
                continue;
            }
            if entry
                .file_type()
                .map_err(|e| format!("file_type: {}", e))?
                .is_dir()
            {
                stack.push(path.clone());
            } else {
                file_paths.push(path);
            }
        }

        for path in file_paths {
            if convert_single_file(&dir, &path)? {
                converted += 1;
            }
        }
    }
    eprintln!(
        "[PB] Converted {} files to BasiliskII extfs sidecar format",
        converted
    );
    Ok(())
}

/// Convert the xattrs on one file into BasiliskII sidecars next to it.
/// Returns true if any sidecar was written.
fn convert_single_file(dir: &Path, file_path: &Path) -> Result<bool, String> {
    let file_name = file_path
        .file_name()
        .ok_or_else(|| "file has no name".to_string())?
        .to_os_string();

    // FinderInfo is present on macOS/xattr extraction paths.
    let finder_info = read_xattr(file_path, "com.apple.FinderInfo")?;
    // Linux `unar` fallback path materializes forks as sibling `*.rsrc` files.
    let companion_rsrc = sidecar_rsrc_path(file_path);
    let companion_rsrc_bytes = if companion_rsrc.is_file() {
        let raw = std::fs::read(&companion_rsrc)
            .map_err(|e| format!("read companion rsrc {}: {}", companion_rsrc.display(), e))?;
        let fork = match maybe_extract_appledouble_resource_fork(&raw)? {
            Some(resource_fork) => resource_fork,
            None => raw,
        };
        Some(fork)
    } else {
        None
    };
    let xattr_rsrc_bytes = if companion_rsrc_bytes.is_some() {
        None
    } else {
        read_resource_fork(file_path)?
    };
    if finder_info.is_none() && companion_rsrc_bytes.is_none() && xattr_rsrc_bytes.is_none() {
        return Ok(false);
    }

    // Ensure .finf/ and .rsrc/ sidecar directories exist when needed.
    let finf_dir = dir.join(".finf");
    let rsrc_dir = dir.join(".rsrc");
    if let Some(mut finf_bytes) = finder_info {
        std::fs::create_dir_all(&finf_dir).map_err(|e| format!("mkdir .finf: {}", e))?;
        // BasiliskII expects 32 bytes (16 FInfo + 16 FXInfo).
        finf_bytes.resize(32, 0);
        let finf_path = finf_dir.join(&file_name);
        std::fs::write(&finf_path, &finf_bytes).map_err(|e| format!("write finf: {}", e))?;
    }

    if companion_rsrc_bytes.is_some() || xattr_rsrc_bytes.is_some() {
        std::fs::create_dir_all(&rsrc_dir).map_err(|e| format!("mkdir .rsrc: {}", e))?;
    }
    if let Some(rsrc_bytes) = companion_rsrc_bytes.or(xattr_rsrc_bytes) {
        let rsrc_path = rsrc_dir.join(&file_name);
        std::fs::write(&rsrc_path, &rsrc_bytes).map_err(|e| format!("write rsrc: {}", e))?;
    }
    if companion_rsrc.is_file() {
        let _ = std::fs::remove_file(&companion_rsrc);
    }

    Ok(true)
}

fn read_u16_be(bytes: &[u8], offset: usize) -> Result<u16, String> {
    let end = offset
        .checked_add(2)
        .ok_or_else(|| "u16 read overflow".to_string())?;
    let slice = bytes
        .get(offset..end)
        .ok_or_else(|| format!("u16 read out of bounds at {}", offset))?;
    let arr: [u8; 2] = slice
        .try_into()
        .map_err(|_| format!("u16 decode failed at {}", offset))?;
    Ok(u16::from_be_bytes(arr))
}

fn read_u32_be(bytes: &[u8], offset: usize) -> Result<u32, String> {
    let end = offset
        .checked_add(4)
        .ok_or_else(|| "u32 read overflow".to_string())?;
    let slice = bytes
        .get(offset..end)
        .ok_or_else(|| format!("u32 read out of bounds at {}", offset))?;
    let arr: [u8; 4] = slice
        .try_into()
        .map_err(|_| format!("u32 decode failed at {}", offset))?;
    Ok(u32::from_be_bytes(arr))
}

/// `unar` on Linux can emit companion `*.rsrc` files in AppleDouble format
/// (`magic = 0x00051607`) instead of raw resource-fork bytes.
///
/// BasiliskII expects `.rsrc/FILE` sidecars to contain the raw resource
/// fork payload. If we pass AppleDouble bytes through unchanged, apps fail
/// to launch because their fork header starts with AppleDouble metadata
/// instead of a resource-map header.
fn maybe_extract_appledouble_resource_fork(bytes: &[u8]) -> Result<Option<Vec<u8>>, String> {
    const APPLEDOUBLE_MAGIC: u32 = 0x0005_1607;
    const APPLESINGLE_MAGIC: u32 = 0x0005_1600;
    const HEADER_LEN: usize = 26;
    const ENTRY_LEN: usize = 12;
    const ENTRY_ID_RESOURCE_FORK: u32 = 2;

    if bytes.len() < HEADER_LEN {
        return Ok(None);
    }
    let magic = read_u32_be(bytes, 0)?;
    if magic != APPLEDOUBLE_MAGIC && magic != APPLESINGLE_MAGIC {
        return Ok(None);
    }
    let entry_count = read_u16_be(bytes, 24)? as usize;
    let table_len = entry_count
        .checked_mul(ENTRY_LEN)
        .ok_or_else(|| "appledouble entry table overflow".to_string())?;
    let table_end = HEADER_LEN
        .checked_add(table_len)
        .ok_or_else(|| "appledouble header overflow".to_string())?;
    if table_end > bytes.len() {
        return Err(format!(
            "appledouble header truncated: need {} bytes, have {}",
            table_end,
            bytes.len()
        ));
    }

    for i in 0..entry_count {
        let base = HEADER_LEN + i * ENTRY_LEN;
        let entry_id = read_u32_be(bytes, base)?;
        let entry_offset = read_u32_be(bytes, base + 4)? as usize;
        let entry_length = read_u32_be(bytes, base + 8)? as usize;
        if entry_id != ENTRY_ID_RESOURCE_FORK {
            continue;
        }
        let entry_end = entry_offset
            .checked_add(entry_length)
            .ok_or_else(|| "appledouble resource range overflow".to_string())?;
        if entry_end > bytes.len() {
            return Err(format!(
                "appledouble resource fork out of bounds: {}..{} of {} bytes",
                entry_offset,
                entry_end,
                bytes.len()
            ));
        }
        return Ok(Some(bytes[entry_offset..entry_end].to_vec()));
    }

    Err("appledouble file missing resource-fork entry (id=2)".to_string())
}

/// Read a single xattr from a file. Returns `None` if the xattr is not
/// present, `Some(bytes)` if it is.
fn read_xattr(file_path: &Path, name: &str) -> Result<Option<Vec<u8>>, String> {
    match xattr::get(file_path, name) {
        Ok(v) => Ok(v),
        Err(e) => {
            // Some filesystems (common in Linux CI containers) report
            // ENOTSUP for xattrs. That's equivalent to "attribute not
            // readable here", so let companion-fork handling continue.
            if e.raw_os_error() == Some(95) {
                Ok(None)
            } else {
                Err(format!(
                    "xattr get {} on {}: {}",
                    name,
                    file_path.display(),
                    e
                ))
            }
        }
    }
}

fn read_resource_fork(file_path: &Path) -> Result<Option<Vec<u8>>, String> {
    // On macOS, resource forks are exposed as `<file>/..namedfork/rsrc`.
    // The xattr API can return a truncated payload for larger forks, which
    // leaves BasiliskII with unlaunchable applications.
    let named_fork = file_path.join("..namedfork/rsrc");
    match std::fs::read(&named_fork) {
        Ok(bytes) => {
            if bytes.is_empty() {
                Ok(None)
            } else {
                Ok(Some(bytes))
            }
        }
        Err(err)
            if matches!(
                err.kind(),
                std::io::ErrorKind::NotFound | std::io::ErrorKind::NotADirectory
            ) =>
        {
            // Linux reports ENOTDIR when the macOS-only named-fork suffix is
            // appended to an ordinary file. That means the named fork is
            // unavailable, so continue with the portable xattr fallback.
            read_xattr(file_path, "com.apple.ResourceFork")
        }
        Err(err) => Err(format!(
            "read resource fork {}: {}",
            named_fork.display(),
            err
        )),
    }
}

fn read_u32_be_file(path: &Path) -> Result<u32, String> {
    let bytes = std::fs::read(path).map_err(|e| format!("read {}: {}", path.display(), e))?;
    let buf: [u8; 4] = bytes
        .get(0..4)
        .ok_or_else(|| format!("{} did not contain 4 bytes", path.display()))?
        .try_into()
        .map_err(|_| format!("{} did not contain a valid u32", path.display()))?;
    Ok(u32::from_be_bytes(buf))
}

fn read_u32_be_file_retry(path: &Path, timeout: Duration) -> Result<u32, String> {
    let deadline = Instant::now() + timeout;
    loop {
        match read_u32_be_file(path) {
            Ok(value) => return Ok(value),
            Err(_) if Instant::now() < deadline => {
                std::thread::sleep(Duration::from_millis(POLL_INTERVAL_MILLIS));
            }
            Err(err) => return Err(err),
        }
    }
}

fn wait_for_u32_file_with_clock_progress(
    path: &Path,
    clock_path: &Path,
    timeout: Duration,
    label: &str,
) -> Result<u32, String> {
    let initial_deadline = Instant::now() + timeout;
    let mut progress: Option<OracleClockProgress> = None;
    loop {
        if path.exists() {
            if let Ok(value) = read_u32_be_file_retry(path, Duration::from_millis(250)) {
                return Ok(value);
            }
        }
        if !container_running() {
            return Err(format!("{} did not appear before BasiliskII exited", label));
        }
        let now = Instant::now();
        if let Ok(sample) = read_oracle_clock_sample(clock_path) {
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
                "timed out waiting for {} at {} without guest-clock progress",
                label,
                path.display()
            ));
        }
        std::thread::sleep(Duration::from_millis(POLL_INTERVAL_MILLIS));
    }
}

fn parse_oracle_clock_sample(bytes: &[u8]) -> Result<OracleClockSample, String> {
    if bytes.len() != 52 || bytes.last() != Some(&b'\n') {
        return Err("oracle clock sample is not a fixed-width record".to_string());
    }
    let text = std::str::from_utf8(bytes).map_err(|e| format!("oracle clock utf8: {e}"))?;
    let fields: Vec<&str> = text.trim_end().split(' ').collect();
    if fields.len() != 4
        || fields[0].len() != 8
        || fields[1].len() != 16
        || fields[2].len() != 8
        || fields[3].len() != 16
        || fields[0] != fields[2]
        || fields[1] != fields[3]
    {
        return Err("oracle clock sample is incomplete or inconsistent".to_string());
    }
    let tick = u32::from_str_radix(fields[0], 16).map_err(|e| format!("oracle clock tick: {e}"))?;
    let retired_instructions = u64::from_str_radix(fields[1], 16)
        .map_err(|e| format!("oracle clock instruction count: {e}"))?;
    Ok(OracleClockSample {
        tick,
        retired_instructions,
    })
}

fn read_oracle_clock_sample(path: &Path) -> Result<OracleClockSample, String> {
    let mut file = std::fs::File::open(path)
        .map_err(|e| format!("open emulator clock {}: {e}", path.display()))?;
    let len = file
        .seek(SeekFrom::End(0))
        .map_err(|e| format!("size emulator clock {}: {e}", path.display()))?;
    if len == 0 || len > 128 {
        return Err(format!(
            "emulator clock {} has invalid length {len}",
            path.display()
        ));
    }
    file.seek(SeekFrom::Start(0))
        .map_err(|e| format!("rewind emulator clock {}: {e}", path.display()))?;
    let mut bytes = Vec::with_capacity(len as usize);
    file.read_to_end(&mut bytes)
        .map_err(|e| format!("read emulator clock {}: {e}", path.display()))?;
    parse_oracle_clock_sample(&bytes)
}

fn current_mac_clock(clock_path: &Path) -> Result<OracleClockSample, String> {
    let deadline = Instant::now() + Duration::from_millis(250);
    loop {
        match read_oracle_clock_sample(clock_path) {
            Ok(sample) => return Ok(sample),
            Err(_) if Instant::now() < deadline => {
                std::thread::sleep(Duration::from_millis(POLL_INTERVAL_MILLIS));
            }
            Err(err) => return Err(err),
        }
    }
}

fn wait_for_live_oracle_clock(clock_path: &Path, baseline: u32) -> Result<(), String> {
    let deadline = Instant::now() + Duration::from_secs(2);
    let mut last_seen = None;
    while Instant::now() < deadline {
        if let Ok(sample) = current_mac_clock(clock_path) {
            last_seen = Some(sample.tick);
            if sample.tick >= baseline.saturating_add(3) {
                return Ok(());
            }
        }
        if !container_running() {
            return Err("BasiliskII exited before its guest clock advanced".to_string());
        }
        std::thread::sleep(Duration::from_millis(POLL_INTERVAL_MILLIS));
    }
    Err(format!(
        "BasiliskII guest clock did not advance beyond baseline {baseline}; last sample {last_seen:?}"
    ))
}

fn current_script_tick(timing: &TimingContext) -> Result<u32, String> {
    Ok(current_mac_clock(&timing.clock_path)?
        .tick
        .wrapping_sub(timing.tick_zero))
}

fn wait_script_ticks(timing: &TimingContext, ticks: u32) -> Result<u32, String> {
    let current = current_mac_clock(&timing.clock_path)?.tick;
    let target = current.saturating_add(ticks);
    let reached = wait_until_mac_tick(&timing.clock_path, target)?;
    Ok(reached.wrapping_sub(timing.tick_zero))
}

fn wait_until_script_tick(timing: &TimingContext, target: u32) -> Result<u32, String> {
    let reached = wait_until_mac_tick(&timing.clock_path, timing.tick_zero.saturating_add(target))?;
    Ok(reached.wrapping_sub(timing.tick_zero))
}

fn script_uses_keypad_digit_alias(script: &PlayScript) -> bool {
    script.actions.iter().any(|action| match action {
        Action::KeyDown { key, .. } | Action::KeyUp { key, .. } => matches!(
            key.to_ascii_lowercase().as_str(),
            "kp0"
                | "kp1"
                | "kp2"
                | "kp3"
                | "kp4"
                | "kp5"
                | "kp6"
                | "kp7"
                | "kp8"
                | "kp9"
                | "numpad0"
                | "numpad1"
                | "numpad2"
                | "numpad3"
                | "numpad4"
                | "numpad5"
                | "numpad6"
                | "numpad7"
                | "numpad8"
                | "numpad9"
                | "kp_decimal"
                | "numpad_decimal"
        ),
        _ => false,
    })
}

fn prime_x11_numlock_for_keypad_aliases() -> Result<(), String> {
    eprintln!("[PB] Priming X11 NumLock for raw keypad keycodes");
    let output = Command::new("docker")
        .args([
            "exec",
            CONTAINER_NAME,
            "bash",
            "-c",
            "export DISPLAY=:99; \
             for i in $(seq 1 100); do \
               if xdotool getdisplaygeometry >/dev/null 2>&1; then break; fi; \
               sleep 0.1; \
             done; \
             xdotool key Num_Lock",
        ])
        .output()
        .map_err(|e| format!("docker exec NumLock prime: {}", e))?;
    if output.status.success() {
        Ok(())
    } else {
        Err(format!(
            "NumLock prime failed: {}",
            String::from_utf8_lossy(&output.stderr)
        ))
    }
}

fn wait_until_mac_tick(clock_path: &Path, target_tick: u32) -> Result<u32, String> {
    let initial = current_mac_clock(clock_path)?;
    let mut progress = OracleClockProgress::new(
        initial,
        Instant::now(),
        Duration::from_secs(TICK_PROGRESS_TIMEOUT_SECS),
    );
    loop {
        let current = current_mac_clock(clock_path)?;
        if current.tick >= target_tick {
            return Ok(current.tick);
        }
        if !container_running() {
            return Err(format!(
                "BasiliskII exited while waiting for mac tick {} (last seen {})",
                target_tick, current.tick
            ));
        }
        let now = Instant::now();
        progress.observe(current, now);
        if progress.expired(now) {
            return Err(format!(
                "timed out waiting for mac tick {} after {}s without guest-clock progress (last seen tick {}, retired instructions {})",
                target_tick,
                TICK_PROGRESS_TIMEOUT_SECS,
                current.tick,
                current.retired_instructions
            ));
        }
        std::thread::sleep(Duration::from_millis(CLOCK_POLL_INTERVAL_MILLIS));
    }
}

// the strict-match call sites that don't care about tolerance.
fn pixel_matches(actual: [u8; 3], expected: [u8; 3], not: bool) -> bool {
    pixel_matches_within(actual, expected, not, 0)
}

fn capture_host_shot(path: &Path, clock_path: &Path) -> Result<OracleClockSample, String> {
    let container_path = "/tmp/basilisk_shot.png";
    // save_screenshot. docker cp doesn't auto-mkdir; without this,
    // any script targeting a subdir errors with "invalid output path".
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent)
                .map_err(|e| format!("create parent {}: {}", parent.display(), e))?;
        }
    }

    let _pause = ContainerPauseGuard::start()?;
    let mut last_err = String::new();
    for attempt in 1..=3 {
        let output = Command::new("docker")
            .args([
                "exec",
                CONTAINER_NAME,
                "bash",
                "-c",
                &format!(
                    "export DISPLAY=:99; \
                     WID=$(xdotool search --onlyvisible --name 'Basilisk II' | head -1); \
                     if [ -z \"$WID\" ]; then WID=$(xdotool getactivewindow 2>/dev/null || true); fi; \
                     if [ -z \"$WID\" ]; then echo 'Basilisk II window not found' >&2; exit 120; fi; \
                     timeout 5 import -window \"$WID\" {}; \
                     DIMS=$(identify -format '%w %h' {} 2>/dev/null || true); \
                     if [ \"$DIMS\" != '800 600' ]; then \
                       timeout 5 import -window root -crop 800x600+0+0 {}; \
                       DIMS=$(identify -format '%w %h' {} 2>/dev/null || true); \
                       if [ \"$DIMS\" != '800 600' ]; then \
                         timeout 5 convert {} -background black -gravity NorthWest -extent 800x600 {}; \
                       fi; \
                     fi",
                    container_path,
                    container_path,
                    container_path,
                    container_path,
                    container_path,
                    container_path
                ),
            ])
            .output()
            .map_err(|e| format!("docker exec import: {}", e))?;
        if !output.status.success() {
            last_err = format!(
                "import failed (attempt {}/3): {}",
                attempt,
                String::from_utf8_lossy(&output.stderr)
            );
            std::thread::sleep(Duration::from_millis(100));
            continue;
        }

        let copy = Command::new("docker")
            .args([
                "cp",
                &format!("{}:{}", CONTAINER_NAME, container_path),
                path.to_str().unwrap(),
            ])
            .output()
            .map_err(|e| format!("docker cp: {}", e))?;
        if copy.status.success() {
            return current_mac_clock(clock_path);
        }

        last_err = format!(
            "docker cp failed (attempt {}/3): {}",
            attempt,
            String::from_utf8_lossy(&copy.stderr)
        );
        std::thread::sleep(Duration::from_millis(100));
    }

    Err(last_err)
}

fn current_pixel(
    timing: &TimingContext,
    x: u32,
    y: u32,
) -> Result<([u8; 3], OracleClockSample), String> {
    let sample = capture_host_shot(&timing.probe_shot, &timing.clock_path)?;
    let image = image::open(&timing.probe_shot)
        .map_err(|e| {
            format!(
                "open probe screenshot {}: {}",
                timing.probe_shot.display(),
                e
            )
        })?
        .to_rgb8();
    let (width, height) = image.dimensions();
    if x >= width || y >= height {
        return Err(format!(
            "pixel ({},{}) out of bounds for {}x{} screenshot",
            x, y, width, height
        ));
    }
    let pixel = image.get_pixel(x, y);
    Ok(([pixel[0], pixel[1], pixel[2]], sample))
}

/// Pauses only the BasiliskII process while Xvfb captures its stable window.
struct ContainerPauseGuard {
    pid: String,
}

impl ContainerPauseGuard {
    fn start() -> Result<Self, String> {
        let output = Command::new("docker")
            .args(["exec", CONTAINER_NAME, "cat", "/tmp/basilisk.pid"])
            .output()
            .map_err(|e| format!("read BasiliskII pid: {e}"))?;
        if !output.status.success() {
            return Err(format!(
                "read BasiliskII pid failed: {}",
                String::from_utf8_lossy(&output.stderr)
            ));
        }
        let pid = String::from_utf8(output.stdout)
            .map_err(|e| format!("BasiliskII pid utf8: {e}"))?
            .trim()
            .to_string();
        let parsed = pid
            .parse::<u32>()
            .map_err(|e| format!("invalid BasiliskII pid {pid:?}: {e}"))?;
        if parsed <= 1 {
            return Err(format!("refusing to pause invalid BasiliskII pid {parsed}"));
        }
        let pause = Command::new("docker")
            .args(["exec", CONTAINER_NAME, "kill", "-STOP", &pid])
            .output()
            .map_err(|e| format!("pause BasiliskII: {e}"))?;
        if !pause.status.success() {
            return Err(format!(
                "pause BasiliskII failed: {}",
                String::from_utf8_lossy(&pause.stderr)
            ));
        }
        Ok(Self { pid })
    }
}

impl Drop for ContainerPauseGuard {
    fn drop(&mut self) {
        let _ = Command::new("docker")
            .args(["exec", CONTAINER_NAME, "kill", "-CONT", &self.pid])
            .output();
    }
}

/// RAII handle that stops the container on drop.
struct ContainerGuard;

fn basilisk_container_run_args(scratch: &Path, container_user: Option<&str>) -> Vec<String> {
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
        // Stop any previous instance (best-effort).
        let _ = Command::new("docker")
            .args(["rm", "-f", CONTAINER_NAME])
            .output();

        let mut command = Command::new("docker");
        let container_user = scratch_container_user(scratch)?;
        command.args(basilisk_container_run_args(
            scratch,
            container_user.as_deref(),
        ));
        let cpus = std::env::var("SYSTEMLESS_BASILISK_CPUS")
            .ok()
            .filter(|value| !value.trim().is_empty())
            .unwrap_or_else(|| DEFAULT_DOCKER_CPUS.to_string());
        command.args(["--cpus", &cpus]);

        // The play image's BasiliskII patches are opt-in at the emulator
        // level. `play-basilisk` is the deterministic oracle runner, so
        // enable them here while still letting callers override or disable
        // the values via the host environment.
        let mac_time = resolved_basilisk_mac_time(
            std::env::var("SYSTEMLESS_BASILISK_MAC_TIME").ok(),
            play_time_secs,
        );
        command.args(["-e", &format!("SYSTEMLESS_BASILISK_MAC_TIME={}", mac_time)]);
        let deterministic_ticks = resolved_basilisk_deterministic_ticks(
            std::env::var("SYSTEMLESS_BASILISK_DETERMINISTIC_TICKS").ok(),
        );
        command.args([
            "-e",
            &format!(
                "SYSTEMLESS_BASILISK_DETERMINISTIC_TICKS={}",
                deterministic_ticks
            ),
        ]);
        command.args(["-e", "SYSTEMLESS_BASILISK_CLOCK=/mnt/shared/basilisk_clock"]);
        let instructions_per_tick = resolved_basilisk_instructions_per_tick(
            std::env::var("SYSTEMLESS_BASILISK_INSTRUCTIONS_PER_TICK").ok(),
        );
        command.args([
            "-e",
            &format!(
                "SYSTEMLESS_BASILISK_INSTRUCTIONS_PER_TICK={}",
                instructions_per_tick
            ),
        ]);

        // When SYSTEMLESS_BASILISK_TRACE is set on the host, forward it into the
        // container as a path under /mnt/shared so the patched BasiliskII
        // emits the JSONL trap-trace stream where the host can read it after
        // the run. The host-side path is the host-mapped equivalent.
        if std::env::var_os("SYSTEMLESS_BASILISK_TRACE").is_some() {
            command.args([
                "-e",
                "SYSTEMLESS_BASILISK_TRACE=/mnt/shared/basilisk_trace.jsonl",
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
        eprintln!("[PB] Container started");
        Ok(Self)
    }
}

impl Drop for ContainerGuard {
    fn drop(&mut self) {
        // Dump container logs before stopping to aid debugging — the
        // BasiliskII / Xvfb entrypoint writes useful startup info to
        // stdout/stderr that is otherwise lost when we --rm the container.
        if let Ok(out) = Command::new("docker")
            .args(["logs", CONTAINER_NAME])
            .output()
        {
            let stdout = String::from_utf8_lossy(&out.stdout);
            let stderr = String::from_utf8_lossy(&out.stderr);
            let combined = format!("{}\n{}", stdout, stderr);
            let combined = combined.trim();
            if !combined.is_empty() {
                eprintln!("[PB] ---- container logs ----");
                for line in combined.lines() {
                    eprintln!("[PB] | {}", line);
                }
                eprintln!("[PB] ------------------------");
            }
        }
        eprintln!("[PB] Stopping container…");
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

/// Run an xdotool command inside the container via `docker exec`.
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
         WID=$(xdotool search --onlyvisible --name 'Basilisk II' | head -1); \
         if [ -z \"$WID\" ]; then WID=$(xdotool getactivewindow 2>/dev/null || true); fi; \
         if [ -z \"$WID\" ]; then echo 'Basilisk II window not found' >&2; exit 120; fi; ",
    );
    if focus_before {
        shell.push_str("xdotool windowfocus \"$WID\" 2>/dev/null || true; ");
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

/// Capture the BasiliskII display region as a PNG into `output_dir`.
///
/// Normal path captures the BasiliskII window by name. When the guest
/// switches to a smaller host window (for example 640x480 mode), we
/// re-capture from the Xvfb root and then force a top-left anchored
/// 800x600 extent so Systemless-vs-Basilisk screenshot dimensions stay
/// comparable across scripts.
fn capture_shot(
    output_dir: &Path,
    filename: &str,
    clock_path: &Path,
) -> Result<OracleClockSample, String> {
    let host_path = output_dir.join(filename);
    let sample = capture_host_shot(&host_path, clock_path)?;
    eprintln!("[PB] Saved {}", host_path.display());
    Ok(sample)
}

/// Read the `tick` field (Mac Ticks at 0x16A) from the last complete
/// JSONL line in the container's trap-trace stream. Returns `None` if
/// the trace file is absent (SYSTEMLESS_BASILISK_TRACE not set, or the
/// `:latest` image is running and ignores the env var) or empty.
///
/// Reads the last 8 KiB so we don't scan the full 35 MB file on every
/// screenshot. The trace file uses `fflush` per event so the tail is
/// consistent with the BasiliskII process's actual last write.
fn read_last_mac_tick(trace_path: &Path) -> Option<u64> {
    let data = std::fs::read(trace_path).ok()?;
    if data.is_empty() {
        return None;
    }
    let tail = if data.len() > 8192 {
        &data[data.len() - 8192..]
    } else {
        &data[..]
    };
    let tail_str = std::str::from_utf8(tail).ok()?;
    let last_complete_line = tail_str.lines().rfind(|l| !l.is_empty())?;
    // The trace schema (see support/basiliskii/macemu-patches/0001-*.patch)
    // always emits `"tick":N` as the 4th field; scan for the literal and
    // parse the following integer.
    let start = last_complete_line.find("\"tick\":")? + "\"tick\":".len();
    let rest = &last_complete_line[start..];
    let end = rest.find(|c: char| !c.is_ascii_digit())?;
    rest[..end].parse::<u64>().ok()
}

fn write_basilisk_capture_context(
    output_dir: &Path,
    screenshot_filename: &str,
    tick_zero: u32,
    sample: OracleClockSample,
    trace_path: &Path,
) -> Result<(), String> {
    let png_path = output_dir.join(screenshot_filename);
    let ctx_path = png_path.with_extension("ctx.json");
    // Add trace_tick_at_capture when a trace stream is available. Lets
    // downstream tools (e.g. basilisk_setentries_diff.py replay) lookup
    // palette[245] at the exact capture Mac-tick, closing the hour-22
    // capture-timing asymmetry question.
    let trace_tick_field = match read_last_mac_tick(trace_path) {
        Some(t) => format!(",\n  \"trace_tick_at_capture\": {}", t),
        None => String::new(),
    };
    let body = format!(
        "{{\n  \"source\": \"basilisk\",\n  \"screenshot\": \"{}\",\n  \"tick\": {},\n  \"guest_tick\": {},\n  \"retired_instructions\": {}{}\n}}\n",
        screenshot_filename,
        sample.tick.wrapping_sub(tick_zero),
        sample.tick,
        sample.retired_instructions,
        trace_tick_field
    );
    std::fs::write(&ctx_path, body).map_err(|e| {
        format!(
            "write basilisk capture context {}: {}",
            ctx_path.display(),
            e
        )
    })?;
    eprintln!(
        "[PB]   ctx {} tick={} guest_tick={} retired_instructions={}",
        ctx_path.display(),
        sample.tick.wrapping_sub(tick_zero),
        sample.tick,
        sample.retired_instructions
    );
    Ok(())
}

fn save_probe_shot(output_dir: &Path, filename: &str, probe_shot: &Path) -> Result<(), String> {
    let host_path = output_dir.join(filename);
    std::fs::copy(probe_shot, &host_path).map_err(|e| {
        format!(
            "copy probe screenshot {} -> {}: {}",
            probe_shot.display(),
            host_path.display(),
            e
        )
    })?;
    eprintln!("[PB] Saved {} (reused matched probe)", host_path.display());
    Ok(())
}

/// Execute one PlayScript action against the running container.
fn execute_action(
    idx: usize,
    action: &Action,
    output_dir: &Path,
    timing: &TimingContext,
    state: &mut ScriptState,
) -> Result<(), String> {
    match action {
        Action::Run {
            instructions,
            ticks,
        } => match (instructions, ticks) {
            (Some(n), None) => {
                state.reuse_probe_capture = false;
                let ipt = realtime_instructions_per_tick() as u64;
                let tick_delta = ((*n as u64) + ipt.saturating_sub(1)) / ipt;
                eprintln!(
                    "[PB] Action {}: run {} instructions (~{} tick(s))",
                    idx, n, tick_delta
                );
                let reached = wait_script_ticks(timing, tick_delta as u32)?;
                eprintln!("[PB]   reached script tick {}", reached);
            }
            (None, Some(t)) => {
                state.reuse_probe_capture = false;
                eprintln!("[PB] Action {}: run {} ticks", idx, t);
                let reached = wait_script_ticks(timing, *t)?;
                eprintln!("[PB]   reached script tick {}", reached);
            }
            _ => {
                return Err(format!(
                    "action {}: run needs exactly one of instructions/ticks",
                    idx
                ))
            }
        },
        Action::RunUntilTick { tick } => {
            state.reuse_probe_capture = false;
            eprintln!("[PB] Action {}: run_until_tick {}", idx, tick);
            let reached = wait_until_script_tick(timing, *tick)?;
            eprintln!("[PB]   reached script tick {}", reached);
        }
        Action::RunUntilPixel {
            x,
            y,
            rgb,
            not,
            tolerance,
            timeout_ticks,
            label,
            ..
        } => {
            let start_tick = current_script_tick(timing)?;
            let deadline = start_tick.saturating_add(timeout_ticks.unwrap_or(300));
            eprintln!(
                "[PB] Action {}: run_until_pixel ({},{}) {} [{},{},{}] tol={} timeout_ticks={:?} - {}",
                idx,
                x,
                y,
                if *not { "!=" } else { "==" },
                rgb[0],
                rgb[1],
                rgb[2],
                tolerance,
                timeout_ticks,
                label.as_deref().unwrap_or("(no label)")
            );
            loop {
                let (actual, sample) = current_pixel(timing, *x, *y)?;
                if pixel_matches_within(actual, *rgb, *not, *tolerance) {
                    let reached = sample.tick.wrapping_sub(timing.tick_zero);
                    eprintln!(
                        "[PB]   matched at script tick {} with pixel [{},{},{}]",
                        reached, actual[0], actual[1], actual[2]
                    );
                    state.reuse_probe_capture = true;
                    state.probe_clock_sample = Some(sample);
                    break;
                }

                let current = current_script_tick(timing)?;
                if current >= deadline {
                    return Err(format!(
                        "run_until_pixel timed out at script tick {}: pixel ({},{}) = [{},{},{}], expected {} [{},{},{}]",
                        current,
                        x,
                        y,
                        actual[0],
                        actual[1],
                        actual[2],
                        if *not { "!=" } else { "==" },
                        rgb[0],
                        rgb[1],
                        rgb[2]
                    ));
                }

                // Advance by one measured guest tick between pixel probes so
                // capture overhead does not consume the script timeout.
                wait_script_ticks(timing, 1)?;
            }
        }
        Action::MouseMove { v, h, at_tick: _ } => {
            state.reuse_probe_capture = false;
            eprintln!("[PB] Action {}: mouse_move ({}, {})", idx, v, h);
            xdotool(&["mousemove", &h.to_string(), &v.to_string()])?;
        }
        Action::MouseDown { v, h, at_tick: _ } => {
            state.reuse_probe_capture = false;
            eprintln!("[PB] Action {}: mouse_down ({}, {})", idx, v, h);
            xdotool(&["mousemove", &h.to_string(), &v.to_string()])?;
            xdotool(&["mousedown", "1"])?;
        }
        Action::MouseUp { v, h, at_tick: _ } => {
            state.reuse_probe_capture = false;
            eprintln!("[PB] Action {}: mouse_up ({}, {})", idx, v, h);
            xdotool(&["mousemove", &h.to_string(), &v.to_string()])?;
            xdotool(&["mouseup", "1"])?;
        }
        Action::KeyDown { key, at_tick: _ } => {
            state.reuse_probe_capture = false;
            let x11 = mac_key_to_x11(key)?;
            eprintln!("[PB] Action {}: key_down '{}' -> {}", idx, key, x11);
            xdotool(&["keydown", &x11])?;
        }
        Action::KeyUp { key, at_tick: _ } => {
            state.reuse_probe_capture = false;
            let x11 = mac_key_to_x11(key)?;
            eprintln!("[PB] Action {}: key_up '{}' -> {}", idx, key, x11);
            xdotool(&["keyup", &x11])?;
        }
        Action::Screenshot { path, at_tick: _ } => {
            let filename = path.clone().unwrap_or_else(|| format!("{:04}.png", idx));
            eprintln!("[PB] Action {}: screenshot -> {}", idx, filename);
            let sample = if state.reuse_probe_capture {
                save_probe_shot(output_dir, &filename, &timing.probe_shot)?;
                state
                    .probe_clock_sample
                    .ok_or_else(|| "reused probe capture is missing its clock sample".to_string())?
            } else {
                capture_shot(output_dir, &filename, &timing.clock_path)?
            };
            state.reuse_probe_capture = false;
            state.probe_clock_sample = None;
            write_basilisk_capture_context(
                output_dir,
                &filename,
                timing.tick_zero,
                sample,
                &timing.trace_path,
            )?;
        }
        Action::RecordFrames { .. } => {
            return Err("record_frames is not supported by this adapter".into())
        }
        Action::Log { message } => {
            eprintln!("[PB] Action {}: {}", idx, message);
        }
        Action::Milestone { name } => {
            eprintln!(
                "[PB] Action {}: milestone '{}' tick={}",
                idx,
                name,
                current_script_tick(timing)?
            );
        }
        Action::AssertTickRange { min, max, label } => {
            let tick = current_script_tick(timing)?;
            let tag = label.as_deref().unwrap_or("(no label)");
            if (*min..=*max).contains(&tick) {
                eprintln!(
                    "[PB] Action {}: assert_tick_range PASS {} - {}",
                    idx, tick, tag
                );
            } else {
                return Err(format!(
                    "assert_tick_range failed at action {}: tick {} not in [{}..={}] - {}",
                    idx, tick, min, max, tag
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
            let (actual, sample) = current_pixel(timing, *x, *y)?;
            let tag = label.as_deref().unwrap_or("(no label)");
            if pixel_matches(actual, *rgb, *not) {
                state.reuse_probe_capture = true;
                state.probe_clock_sample = Some(sample);
                eprintln!(
                    "[PB] Action {}: assert_pixel PASS ({},{}) = [{},{},{}] - {}",
                    idx, x, y, actual[0], actual[1], actual[2], tag
                );
            } else {
                return Err(format!(
                    "assert_pixel failed at action {}: pixel ({},{}) = [{},{},{}], expected {} [{},{},{}] - {}",
                    idx,
                    x,
                    y,
                    actual[0],
                    actual[1],
                    actual[2],
                    if *not { "!=" } else { "==" },
                    rgb[0],
                    rgb[1],
                    rgb[2],
                    tag
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

/// Map a PlayScript key name (matches `systemless_oracle_tools::resolve_key`) to a
/// key token accepted by xdotool.
fn mac_key_to_x11(key: &str) -> Result<String, String> {
    let lower = key.to_ascii_lowercase();
    let mapped = match lower.as_str() {
        "return" | "enter" => "Return",
        "tab" => "Tab",
        "space" => "space",
        "delete" | "backspace" => "BackSpace",
        "escape" | "esc" => "Escape",
        "left" => "Left",
        "right" => "Right",
        "down" => "Down",
        "up" => "Up",
        "f1" => "F1",
        "f2" => "F2",
        "f3" => "F3",
        "f4" => "F4",
        "f5" => "F5",
        "f6" => "F6",
        "f7" => "F7",
        "f8" => "F8",
        "f9" => "F9",
        "f10" => "F10",
        "f11" => "F11",
        "f12" => "F12",
        // With NumLock primed once before replay, raw X keycodes send the
        // XFree86 keypad keycode BasiliskII maps to the classic Mac keypad.
        // Using KP_8 directly makes xdotool synthesize a NumLock/Clear event
        // around every press, which breaks games that poll the keypad state.
        "numpad0" | "kp0" => "90",
        "numpad1" | "kp1" => "87",
        "numpad2" | "kp2" => "88",
        "numpad3" | "kp3" => "89",
        "numpad4" | "kp4" => "83",
        "numpad5" | "kp5" => "84",
        "numpad6" | "kp6" => "85",
        "numpad7" | "kp7" => "79",
        "numpad8" | "kp8" => "80",
        "numpad9" | "kp9" => "81",
        "numpad_enter" | "kp_enter" => "KP_Enter",
        "numpad_add" | "kp_add" => "KP_Add",
        "numpad_subtract" | "kp_subtract" => "KP_Subtract",
        "numpad_multiply" | "kp_multiply" => "KP_Multiply",
        "numpad_divide" | "kp_divide" => "KP_Divide",
        "numpad_decimal" | "kp_decimal" => "91",
        // Symbols: xdotool's --clearmodifiers parser rejects raw `.` and a
        // few other punctuation characters as ambiguous; map them to the
        // explicit X11 keysym names so Cmd-. (canonical Mac dialog cancel)
        // and similar shortcuts work via key_down/key_up.
        "." | "period" => "period",
        "," | "comma" => "comma",
        "/" | "slash" => "slash",
        "\\" | "backslash" => "backslash",
        ";" | "semicolon" => "semicolon",
        "'" | "apostrophe" => "apostrophe",
        "[" | "bracketleft" => "bracketleft",
        "]" | "bracketright" => "bracketright",
        "-" | "minus" => "minus",
        "=" | "equal" => "equal",
        "`" | "grave" => "grave",
        // BasiliskII's X11 keycode → Mac keycode table (Unix/keycodes,
        // XFree86 vendor section, also active under Xvfb in the play
        // image) maps X11 Alt_L (keycode 64) to Mac Cmd 55, and X11
        // Super_L / Logo_L (keycode 115) to Mac Option 58. So Mac Cmd
        // is sent via X11 Alt_L, and Mac Option via X11 Super_L —
        // not the intuitive `cmd → Super_L` mapping. The previous
        // assignment was flipped, which silently sent Mac Option in
        // place of Cmd; games using Cmd-N (Koji, Munchies) saw the
        // hotkey land in their unmapped Option-N slot and the menu
        // command never fired.
        "cmd" | "command" => "Alt_L",
        "shift" => "Shift_L",
        "caps_lock" | "capslock" => "Caps_Lock",
        "control" | "ctrl" => "Control_L",
        "option" | "alt" => "Super_L",
        other if other.len() == 1 => {
            // Single-character ASCII — xdotool accepts the raw character for
            // a-z / 0-9 / most symbols. Preserve the original case so that
            // uppercase maps to the shifted keysym (xdotool's default).
            return Ok(key.to_string());
        }
        _ => return Err(format!("unknown key name '{}'", key)),
    };
    Ok(mapped.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn script_action_failures_stop_the_route_and_propagate_the_reason() {
        let cases = [
            (
                r#"{"actions":[{"type":"assert_pixel","x":1,"y":2,"rgb":[3,4,5]}]}"#,
                "assert_pixel failed at action 0",
            ),
            (
                r#"{"actions":[{"type":"run_until_pixel","x":1,"y":2,"rgb":[3,4,5]}]}"#,
                "run_until_pixel timed out at script tick 10",
            ),
            (
                r#"{"actions":[{"type":"key_down","key":"unsupported"}]}"#,
                "unknown key name 'unsupported'",
            ),
        ];

        for (script_json, expected_message) in cases {
            let script: PlayScript = serde_json::from_str(script_json).unwrap();
            let result = execute_script_actions(&script.actions, |index, _| {
                assert_eq!(index, 0);
                Err(expected_message.to_string())
            });

            assert_eq!(
                result,
                Err(ScriptActionFailure {
                    index: 0,
                    message: expected_message.to_string(),
                })
            );
        }
    }

    #[test]
    fn successful_script_execution_reports_the_completed_action_count() {
        let script: PlayScript = serde_json::from_str(
            r#"{"actions":[{"type":"log","message":"one"},{"type":"log","message":"two"}]}"#,
        )
        .unwrap();
        let mut visited = Vec::new();

        let completed = execute_script_actions(&script.actions, |index, _| {
            visited.push(index);
            Ok(())
        })
        .unwrap();

        assert_eq!(completed, 2);
        assert_eq!(visited, vec![0, 1]);
    }

    #[test]
    fn oracle_clock_sample_requires_matching_copies() {
        let sample =
            parse_oracle_clock_sample(b"0000002a 0000000000003039 0000002a 0000000000003039\n")
                .unwrap();
        assert_eq!(
            sample,
            OracleClockSample {
                tick: 42,
                retired_instructions: 12_345,
            }
        );

        let torn =
            parse_oracle_clock_sample(b"0000002a 0000000000003039 0000002b 0000000000003039\n")
                .unwrap_err();
        assert!(torn.contains("incomplete or inconsistent"));

        let truncated =
            parse_oracle_clock_sample(b"0000002a 0000000000003039 0000002a 0000000000003039")
                .unwrap_err();
        assert!(truncated.contains("fixed-width record"));
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
    fn play_prefs_insert_8bpp_depth_without_screen_suffix() {
        let prefs = rewrite_play_prefs_for_800x600_8bpp(
            "# test prefs\nscreen win/640/480\nramsize 67108864\n",
        );

        assert!(prefs.contains("screen win/800/600"));
        assert!(prefs.contains("displaycolordepth 8"));
        assert!(
            !prefs.contains("screen win/800/600/8"),
            "SDL BasiliskII ignores the slash depth suffix; use displaycolordepth"
        );
    }

    #[test]
    fn play_prefs_replace_existing_display_depth() {
        let prefs = rewrite_play_prefs_for_800x600_8bpp(
            "screen win/1024/768\ndisplaycolordepth 0\nnogui true\n",
        );

        assert!(prefs.contains("screen win/800/600"));
        assert!(prefs.contains("displaycolordepth 8"));
        assert!(!prefs.contains("displaycolordepth 0"));
        assert_eq!(prefs.matches("displaycolordepth ").count(), 1);
    }

    #[test]
    fn basilisk_container_run_disables_networking() {
        let args = basilisk_container_run_args(Path::new("/task"), Some("501:1000"));
        let network_modes: Vec<&str> = args
            .windows(2)
            .filter(|pair| pair[0] == "--network")
            .map(|pair| pair[1].as_str())
            .collect();

        assert_eq!(network_modes, vec!["none"]);
        assert!(!args.iter().any(|arg| arg.starts_with("--network=")));
    }

    #[test]
    fn basilisk_container_runs_as_the_scratch_owner_with_a_readable_home() {
        let args = basilisk_container_run_args(Path::new("/task"), Some("501:1000"));

        assert!(args.windows(2).any(|pair| pair == ["--user", "501:1000"]));
        assert!(args.windows(2).any(|pair| pair == ["-e", "HOME=/tmp"]));
        assert!(args
            .windows(2)
            .any(|pair| pair == ["-v", "/task/container_home:/root:ro"]));
    }

    #[test]
    fn stale_scratch_cleanup_errors_are_propagated() {
        let scratch = std::env::temp_dir().join(format!(
            "systemless-basilisk-scratch-file-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_file(&scratch);
        std::fs::write(&scratch, b"not a directory").unwrap();

        let error = remove_existing_scratch(&scratch).unwrap_err();

        assert!(error.contains("remove stale scratch directory"));
        std::fs::remove_file(scratch).unwrap();
    }

    #[test]
    fn keypad_digit_aliases_use_raw_xfree86_keycodes() {
        assert_eq!(mac_key_to_x11("kp0").unwrap(), "90");
        assert_eq!(mac_key_to_x11("kp5").unwrap(), "84");
        assert_eq!(mac_key_to_x11("kp8").unwrap(), "80");
        assert_eq!(mac_key_to_x11("numpad_decimal").unwrap(), "91");
        assert_eq!(mac_key_to_x11("numpad_enter").unwrap(), "KP_Enter");
    }

    #[test]
    fn function_keys_use_x11_keysyms() {
        assert_eq!(mac_key_to_x11("f1").unwrap(), "F1");
        assert_eq!(mac_key_to_x11("F11").unwrap(), "F11");
        assert_eq!(mac_key_to_x11("f12").unwrap(), "F12");
    }

    #[test]
    fn caps_lock_aliases_use_the_x11_lock_keysym() {
        assert_eq!(mac_key_to_x11("caps_lock").unwrap(), "Caps_Lock");
        assert_eq!(mac_key_to_x11("capslock").unwrap(), "Caps_Lock");
    }

    #[test]
    fn bare_executable_override_does_not_match_parent_folder() {
        let root = Path::new("Game");
        let installer = root.join("Color Playroom/Installer");

        assert!(!candidate_matches_executable(&installer, root, "PlayRoom"));
        assert!(candidate_matches_executable(
            &installer,
            root,
            "Color Playroom/Installer"
        ));
    }

    #[test]
    fn installer_destinations_decode_pascal_paths() {
        let mut bytes = vec![0, 0xff, 0];
        let destination = b":Installed Game:Resources:Scene.rsrc";
        bytes.push(destination.len() as u8);
        bytes.extend_from_slice(destination);
        bytes.push(0);
        bytes.extend_from_slice(b"not a Pascal path");

        assert_eq!(
            pascal_installer_destinations(&bytes),
            vec!["Installed Game/Resources/Scene.rsrc"]
        );
    }

    #[test]
    fn appledouble_resource_fork_is_extracted() {
        // Minimal AppleDouble v2 with one entry (id=2) pointing to
        // a 4-byte synthetic resource-fork payload.
        let mut bytes = Vec::new();
        bytes.extend_from_slice(&0x0005_1607u32.to_be_bytes()); // magic
        bytes.extend_from_slice(&0x0002_0000u32.to_be_bytes()); // version
        bytes.extend_from_slice(&[0u8; 16]); // filler
        bytes.extend_from_slice(&1u16.to_be_bytes()); // entry count
        bytes.extend_from_slice(&2u32.to_be_bytes()); // entry id = rsrc fork
        bytes.extend_from_slice(&38u32.to_be_bytes()); // offset
        bytes.extend_from_slice(&4u32.to_be_bytes()); // length
        bytes.extend_from_slice(&[0xDE, 0xAD, 0xBE, 0xEF]); // payload

        let extracted = maybe_extract_appledouble_resource_fork(&bytes)
            .expect("parse appledouble")
            .expect("resource fork entry");
        assert_eq!(extracted, vec![0xDE, 0xAD, 0xBE, 0xEF]);
    }

    #[test]
    fn non_appledouble_bytes_passthrough() {
        let bytes = vec![0, 1, 2, 3, 4, 5];
        assert!(maybe_extract_appledouble_resource_fork(&bytes)
            .expect("parse non-appledouble")
            .is_none());
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn resource_fork_probe_treats_namedfork_enotdir_as_absent() {
        let dir = std::env::temp_dir().join(format!(
            "systemless-basilisk-resource-fork-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("ReadMe!");
        std::fs::write(&file, b"plain data fork").unwrap();
        let named_fork_error = std::fs::read(file.join("..namedfork/rsrc")).unwrap_err();
        assert_eq!(named_fork_error.kind(), std::io::ErrorKind::NotADirectory);

        let resource_fork = read_resource_fork(&file).expect("probe ordinary Linux file");

        assert!(resource_fork.is_none());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn flatten_honors_nested_executable_override() {
        let dir = std::env::temp_dir().join(format!(
            "systemless-basilisk-nested-app-{}-{}",
            std::process::id(),
            "override"
        ));
        let _ = std::fs::remove_dir_all(&dir);
        let app_dir = dir.join("Pararena Demo").join("Pararena");
        std::fs::create_dir_all(&app_dir).unwrap();
        std::fs::write(app_dir.join("Pararena Demo 2.01"), []).unwrap();
        std::fs::write(app_dir.join("Pararena Demo 2.01.rsrc"), [1, 2, 3]).unwrap();
        std::fs::write(app_dir.join("Para Sounds"), []).unwrap();
        std::fs::write(app_dir.join("Para Sounds.rsrc"), [4, 5, 6]).unwrap();
        std::fs::write(dir.join("Pararena Demo").join("ReadMe"), b"read me").unwrap();

        flatten_and_rename_main_app(
            &dir,
            "_PlayTarget",
            Some("Pararena Demo/Pararena/Pararena Demo 2.01"),
        )
        .unwrap();

        assert!(dir.join("_PlayTarget").is_file());
        assert!(dir.join("_PlayTarget.rsrc").is_file());
        assert!(dir.join("Para Sounds").is_file());
        assert!(dir.join("Para Sounds.rsrc").is_file());
        assert!(!dir
            .join("Pararena Demo")
            .join("Pararena")
            .join("Pararena Demo 2.01")
            .exists());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn nested_disk_staging_preserves_forks_and_selects_the_appl() {
        let dir = std::env::temp_dir().join(format!(
            "systemless-basilisk-disk-image-{}-{}",
            std::process::id(),
            "staging"
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let image = systemless::disk_image::DiskImageContents {
            volume_name: "Fixture Disk".to_string(),
            volume_info: Default::default(),
            dirs: vec![
                "Fixture Disk".to_string(),
                "Fixture Disk/Game Folder".to_string(),
            ],
            files: vec![
                systemless::disk_image::DiskImageFile {
                    path: "Fixture Disk/Game Folder/Fixture Game".to_string(),
                    data: b"application data".to_vec(),
                    rsrc: b"application resources".to_vec(),
                    file_type: *b"APPL",
                    creator: *b"TEST",
                    finder_flags: 0x0400,
                },
                systemless::disk_image::DiskImageFile {
                    path: "Fixture Disk/Game Folder/Read Me".to_string(),
                    data: b"instructions".to_vec(),
                    rsrc: Vec::new(),
                    file_type: *b"TEXT",
                    creator: *b"ttxt",
                    finder_flags: 0,
                },
            ],
        };

        materialize_disk_image(&dir, image).unwrap();

        let app = dir.join("Fixture Disk/Game Folder/Fixture Game");
        assert_eq!(std::fs::read(&app).unwrap(), b"application data");
        assert_eq!(
            std::fs::read(extfs_sidecar_path(&app, ".rsrc").unwrap()).unwrap(),
            b"application resources"
        );
        let candidates = collect_appl_candidates(&dir, true).unwrap();
        assert_eq!(candidates, vec![app]);

        flatten_and_rename_main_app(&dir, "_PlayTarget", None).unwrap();

        assert_eq!(
            std::fs::read(dir.join("_PlayTarget")).unwrap(),
            b"application data"
        );
        assert_eq!(
            std::fs::read(dir.join(".rsrc/_PlayTarget")).unwrap(),
            b"application resources"
        );
        let finf = std::fs::read(dir.join(".finf/_PlayTarget")).unwrap();
        assert_eq!(&finf[0..4], b"APPL");
        assert_eq!(&finf[4..8], b"TEST");
        assert_eq!(&finf[8..10], &0x0400u16.to_be_bytes());
        assert_eq!(std::fs::read(dir.join("Read Me")).unwrap(), b"instructions");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn nested_disk_staging_rejects_parent_paths() {
        let err = validated_archive_relative_path("Fixture Disk/../escape").unwrap_err();

        assert!(err.contains("unsafe disk-image path"));
    }

    #[test]
    fn application_partition_patch_updates_size_minus_one_partition_sizes() {
        let mut fork = make_size_only_resource_fork(0x0030_0000, 0x0020_0000);

        let patched = patch_application_size_resource_partition(&mut fork, 0x0040_0000)
            .expect("patch SIZE resource");

        assert_eq!(patched, vec![(-1, 0x0030_0000, 0x0020_0000)]);
        assert_eq!(&fork[0x104 + 2..0x104 + 6], &0x0040_0000u32.to_be_bytes());
        assert_eq!(&fork[0x104 + 6..0x104 + 10], &0x0040_0000u32.to_be_bytes());
    }

    #[test]
    fn application_partition_patch_updates_all_size_variants() {
        let mut fork = make_size_resource_fork(&[
            (-1, 0x002D_5000, 0x002D_5000),
            (1, 0x002E_CC00, 0x002D_5000),
            (0, 0x002E_CC00, 0x002D_5000),
        ]);

        let patched = patch_application_size_resource_partition(&mut fork, 0x0040_0000)
            .expect("patch SIZE resources");

        assert_eq!(
            patched,
            vec![
                (-1, 0x002D_5000, 0x002D_5000),
                (1, 0x002E_CC00, 0x002D_5000),
                (0, 0x002E_CC00, 0x002D_5000),
            ]
        );
        for idx in 0..3 {
            let data = 0x100 + idx * 14 + 4;
            assert_eq!(&fork[data + 2..data + 6], &0x0040_0000u32.to_be_bytes());
            assert_eq!(&fork[data + 6..data + 10], &0x0040_0000u32.to_be_bytes());
        }
    }

    #[test]
    fn application_partition_patch_reports_missing_size_resources() {
        let mut fork = make_single_resource_fork(*b"CODE", 0, &[0, 1, 2, 3]);

        let patched = patch_application_size_resource_partition(&mut fork, 0x0040_0000)
            .expect("scan resource fork");

        assert_eq!(patched, Vec::<(i16, u32, u32)>::new());
    }

    #[test]
    fn prelaunch_create_dirs_writes_line_control_file() {
        let dir = std::env::temp_dir().join(format!(
            "systemless-prelaunch-dirs-{}-{}",
            std::process::id(),
            "writes"
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let paths = vec![
            " MacintoshHD:System Folder:Preferences ".to_string(),
            "MacintoshHD:System Folder:Preferences:Sierra".to_string(),
        ];

        stage_prelaunch_create_dirs(&dir, Some(&paths)).unwrap();

        let control = std::fs::read_to_string(dir.join(PRELAUNCH_CREATE_DIRS_FILE)).unwrap();
        assert_eq!(
            control,
            "MacintoshHD:System Folder:Preferences\nMacintoshHD:System Folder:Preferences:Sierra\n"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn prelaunch_create_dirs_rejects_newlines() {
        let dir = std::env::temp_dir().join(format!(
            "systemless-prelaunch-dirs-{}-{}",
            std::process::id(),
            "rejects"
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let paths = vec!["MacintoshHD:System Folder\nPreferences".to_string()];

        let err = stage_prelaunch_create_dirs(&dir, Some(&paths)).unwrap_err();

        assert!(err.contains("newline"));
        assert!(!dir.join(PRELAUNCH_CREATE_DIRS_FILE).exists());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn prelaunch_delete_paths_writes_line_control_file() {
        let dir = std::env::temp_dir().join(format!(
            "systemless-prelaunch-delete-{}-{}",
            std::process::id(),
            "writes"
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let paths = vec![
            " MacintoshHD:System Folder:Preferences:EV Override License ".to_string(),
            "MacintoshHD:System Folder:Preferences:Override Prefs".to_string(),
        ];

        stage_prelaunch_delete_paths(&dir, Some(&paths)).unwrap();

        let control = std::fs::read_to_string(dir.join(PRELAUNCH_DELETE_PATHS_FILE)).unwrap();
        assert_eq!(
            control,
            "MacintoshHD:System Folder:Preferences:EV Override License\nMacintoshHD:System Folder:Preferences:Override Prefs\n"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn prelaunch_delete_paths_rejects_newlines() {
        let dir = std::env::temp_dir().join(format!(
            "systemless-prelaunch-delete-{}-{}",
            std::process::id(),
            "rejects"
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let paths = vec!["MacintoshHD:System Folder:Preferences\nEV Override License".to_string()];

        let err = stage_prelaunch_delete_paths(&dir, Some(&paths)).unwrap_err();

        assert!(err.contains("newline"));
        assert!(!dir.join(PRELAUNCH_DELETE_PATHS_FILE).exists());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn boot_disk_copy_manifest_lists_logical_game_files() {
        let dir = std::env::temp_dir().join(format!(
            "systemless-boot-copy-manifest-{}-{}",
            std::process::id(),
            "writes"
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join(".rsrc")).unwrap();
        std::fs::create_dir_all(dir.join(".finf")).unwrap();
        std::fs::create_dir_all(dir.join("Data Folder")).unwrap();
        std::fs::write(dir.join("_PlayTarget"), b"app").unwrap();
        std::fs::write(dir.join("Demo House"), b"house").unwrap();
        std::fs::write(dir.join("Icon\r"), b"finder icon").unwrap();
        std::fs::write(dir.join("Data Folder").join("Level One"), b"level").unwrap();
        std::fs::write(dir.join(".rsrc").join("_PlayTarget"), b"rsrc").unwrap();
        std::fs::write(dir.join(".finf").join("_PlayTarget"), b"finf").unwrap();
        std::fs::write(dir.join("FixtureGen"), b"launcher").unwrap();
        std::fs::write(dir.join("_play_time"), [0, 0, 0, 0]).unwrap();
        std::fs::write(dir.join("_prelaunch_create_dirs"), b"ignored").unwrap();

        stage_boot_disk_copy_manifest(&dir, true).unwrap();

        let manifest =
            std::fs::read_to_string(dir.join(PRELAUNCH_BOOT_COPY_MANIFEST_FILE)).expect("manifest");
        assert_eq!(
            manifest,
            "D\tData Folder\nF\tData Folder:Level One\nF\tDemo House\nF\t_PlayTarget\n"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn boot_disk_copy_manifest_rejects_control_char_paths() {
        let err = boot_copy_mac_relative_path(Path::new("Bad\rName")).unwrap_err();

        assert!(err.contains("cannot be represented"));
    }

    #[test]
    fn boot_disk_copy_manifest_removes_stale_file_when_disabled() {
        let dir = std::env::temp_dir().join(format!(
            "systemless-boot-copy-manifest-{}-{}",
            std::process::id(),
            "disabled"
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let manifest = dir.join(PRELAUNCH_BOOT_COPY_MANIFEST_FILE);
        std::fs::write(&manifest, b"F\t_PlayTarget\n").unwrap();

        stage_boot_disk_copy_manifest(&dir, false).unwrap();

        assert!(!manifest.exists());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn basilisk_determinism_env_defaults_to_oracle_pins() {
        assert_eq!(
            resolved_basilisk_mac_time(None, systemless_oracle_tools::play_mac_time_secs()),
            systemless_oracle_tools::play_mac_time_secs().to_string()
        );
        assert_eq!(resolved_basilisk_deterministic_ticks(None), "1");
        assert_eq!(
            resolved_basilisk_instructions_per_tick(None),
            realtime_instructions_per_tick().to_string()
        );
    }

    #[test]
    fn basilisk_determinism_env_respects_host_overrides() {
        assert_eq!(
            resolved_basilisk_mac_time(Some("123456".to_string()), 3_786_911_998),
            "123456"
        );
        assert_eq!(
            resolved_basilisk_deterministic_ticks(Some("0".to_string())),
            "0"
        );
        assert_eq!(
            resolved_basilisk_instructions_per_tick(Some("123456".to_string())),
            "123456"
        );
    }

    #[test]
    fn app_name_scoring_prefers_real_title_over_icon_stub() {
        let folder_words: HashSet<String> = tokenize_alpha_words("king of parking")
            .into_iter()
            .collect();
        let king = score_app_name(&folder_words, "king of parking 1.1");
        let icon = score_app_name(&folder_words, "icon");
        assert!(
            king.0 > icon.0,
            "expected title overlap {} > icon overlap {}",
            king.0,
            icon.0
        );
    }

    #[test]
    fn app_name_scoring_penalizes_register_helper_extras() {
        let folder_words: HashSet<String> = tokenize_alpha_words("escape velocity 1.0.5")
            .into_iter()
            .collect();
        let main = score_app_name(&folder_words, "escape velocity");
        let helper = score_app_name(&folder_words, "register escape velocity");
        assert_eq!(main.0, helper.0, "expected equal overlap");
        assert!(
            main.1 < helper.1,
            "expected fewer extra words for main app ({} < {})",
            main.1,
            helper.1
        );
    }

    fn make_size_only_resource_fork(preferred_size: u32, minimum_size: u32) -> Vec<u8> {
        make_size_resource_fork(&[(-1, preferred_size, minimum_size)])
    }

    fn make_size_resource_fork(resources: &[(i16, u32, u32)]) -> Vec<u8> {
        let data_offset = 0x100usize;
        let resource_data_len = 10usize;
        let serialized_resource_len = 4 + resource_data_len;
        let data_len = serialized_resource_len * resources.len();
        let map_offset = data_offset + data_len;
        let map_len = 30 + 8 + 12 * resources.len();
        let total_len = map_offset + map_len;
        let mut bytes = vec![0u8; total_len];

        bytes[0..4].copy_from_slice(&(data_offset as u32).to_be_bytes());
        bytes[4..8].copy_from_slice(&(map_offset as u32).to_be_bytes());
        bytes[8..12].copy_from_slice(&(data_len as u32).to_be_bytes());
        bytes[12..16].copy_from_slice(&(map_len as u32).to_be_bytes());

        for (idx, (_id, preferred_size, minimum_size)) in resources.iter().enumerate() {
            let entry = data_offset + idx * serialized_resource_len;
            bytes[entry..entry + 4].copy_from_slice(&(resource_data_len as u32).to_be_bytes());
            let data = entry + 4;
            bytes[data..data + 2].copy_from_slice(&0x0000u16.to_be_bytes());
            bytes[data + 2..data + 6].copy_from_slice(&preferred_size.to_be_bytes());
            bytes[data + 6..data + 10].copy_from_slice(&minimum_size.to_be_bytes());
        }

        let header_copy: Vec<u8> = bytes[0..16].to_vec();
        bytes[map_offset..map_offset + 16].copy_from_slice(&header_copy);
        bytes[map_offset + 24..map_offset + 26].copy_from_slice(&28u16.to_be_bytes());
        bytes[map_offset + 26..map_offset + 28].copy_from_slice(&0u16.to_be_bytes());
        bytes[map_offset + 28..map_offset + 30].copy_from_slice(&0u16.to_be_bytes());

        let type_entry = map_offset + 30;
        bytes[type_entry..type_entry + 4].copy_from_slice(b"SIZE");
        bytes[type_entry + 4..type_entry + 6]
            .copy_from_slice(&((resources.len() as u16) - 1).to_be_bytes());
        bytes[type_entry + 6..type_entry + 8].copy_from_slice(&10u16.to_be_bytes());

        for (idx, (id, _preferred_size, _minimum_size)) in resources.iter().enumerate() {
            let ref_entry = map_offset + 38 + idx * 12;
            let data_rel = idx * serialized_resource_len;
            bytes[ref_entry..ref_entry + 2].copy_from_slice(&id.to_be_bytes());
            bytes[ref_entry + 2..ref_entry + 4].copy_from_slice(&0xFFFFu16.to_be_bytes());
            bytes[ref_entry + 4] = 0;
            bytes[ref_entry + 5] = ((data_rel >> 16) & 0xFF) as u8;
            bytes[ref_entry + 6] = ((data_rel >> 8) & 0xFF) as u8;
            bytes[ref_entry + 7] = (data_rel & 0xFF) as u8;
        }

        bytes
    }

    fn make_single_resource_fork(res_type: [u8; 4], id: i16, data: &[u8]) -> Vec<u8> {
        let data_offset = 0x100usize;
        let map_offset = data_offset + 4 + data.len();
        let map_len = 30 + 8 + 12;
        let total_len = map_offset + map_len;
        let mut bytes = vec![0u8; total_len];

        bytes[0..4].copy_from_slice(&(data_offset as u32).to_be_bytes());
        bytes[4..8].copy_from_slice(&(map_offset as u32).to_be_bytes());
        bytes[8..12].copy_from_slice(&((4 + data.len()) as u32).to_be_bytes());
        bytes[12..16].copy_from_slice(&(map_len as u32).to_be_bytes());

        bytes[data_offset..data_offset + 4].copy_from_slice(&(data.len() as u32).to_be_bytes());
        bytes[data_offset + 4..data_offset + 4 + data.len()].copy_from_slice(data);

        let header_copy: Vec<u8> = bytes[0..16].to_vec();
        bytes[map_offset..map_offset + 16].copy_from_slice(&header_copy);
        bytes[map_offset + 24..map_offset + 26].copy_from_slice(&28u16.to_be_bytes());
        bytes[map_offset + 26..map_offset + 28].copy_from_slice(&0u16.to_be_bytes());
        bytes[map_offset + 28..map_offset + 30].copy_from_slice(&0u16.to_be_bytes());

        let type_entry = map_offset + 30;
        bytes[type_entry..type_entry + 4].copy_from_slice(&res_type);
        bytes[type_entry + 4..type_entry + 6].copy_from_slice(&0u16.to_be_bytes());
        bytes[type_entry + 6..type_entry + 8].copy_from_slice(&10u16.to_be_bytes());

        let ref_entry = map_offset + 38;
        bytes[ref_entry..ref_entry + 2].copy_from_slice(&id.to_be_bytes());
        bytes[ref_entry + 2..ref_entry + 4].copy_from_slice(&0xFFFFu16.to_be_bytes());
        bytes[ref_entry + 4] = 0;
        bytes[ref_entry + 5] = 0;
        bytes[ref_entry + 6] = 0;
        bytes[ref_entry + 7] = 0;

        bytes
    }
}
