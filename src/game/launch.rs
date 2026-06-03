//! Shared game loading and initialization for all Systemless frontends.
//!
//! Consolidates ROM loading (MacBinary + StuffIt), runner initialization,
//! and post-load configuration so all frontends behave identically.

use crate::loader::LoadedApp;
use crate::managers::resource::ResourceFork;
use crate::memory::MemoryBus;
use crate::runner::{FixtureRunner, FixtureRunnerConfig};
use stuffit::SitArchive;

const WEB_PACK_MAGIC: &[u8; 4] = b"KPK1";

/// Standard RAM size for all frontends (32 MB).
pub const RAM_SIZE: u32 = crate::machine_profile::ORACLE_MACHINE_PROFILE.ram_size_bytes;

/// Max instructions to execute per GUI/WASM frame.
/// Must be large enough to complete a full PICT draw (~500K instructions)
/// in one frame, otherwise the user sees partially-rendered intermediate states.
pub const MAX_INSTRUCTIONS_PER_FRAME: usize = 2_000_000;

/// Create a new FixtureRunner with standard configuration.
pub fn new_runner() -> FixtureRunner {
    FixtureRunner::new(
        RAM_SIZE as usize,
        FixtureRunnerConfig {
            load_address: 0x10000,
            max_instructions: MAX_INSTRUCTIONS_PER_FRAME,
            ..FixtureRunnerConfig::default()
        },
    )
}

/// Load a game ROM from raw file bytes (MacBinary, StuffIt archive, or raw resource fork).
///
/// Handles StuffIt archives (populates VFS with all entries, finds executable),
/// MacBinary files, and macOS resource fork paths. Returns the LoadedApp on success.
pub fn load_game(runner: &mut FixtureRunner, file_data: &[u8]) -> Result<LoadedApp, String> {
    if file_data.starts_with(WEB_PACK_MAGIC) {
        load_web_pack(runner, file_data)
    } else if is_stuffit_archive(file_data) {
        load_stuffit(runner, file_data)
    } else if crate::disk_image::looks_like_dc42_or_hfs(file_data) {
        load_disk_image(runner, file_data)
    } else {
        load_macbinary(runner, file_data)
    }
}

/// Prepack a StuffIt archive into a lightweight format for faster web startup.
///
/// The packed format stores fully decompressed data/resource forks for each file,
/// so loading avoids runtime archive decompression in Wasm.
pub fn pack_stuffit_for_web(file_data: &[u8]) -> Result<Vec<u8>, String> {
    let archive =
        SitArchive::parse(file_data).map_err(|e| format!("Failed to parse StuffIt: {:?}", e))?;

    let file_entries = collect_stuffit_payload_files(&archive)?;
    let mut out = Vec::new();
    out.extend_from_slice(WEB_PACK_MAGIC);
    out.extend_from_slice(&(file_entries.len() as u32).to_be_bytes());

    for entry in file_entries {
        let name_bytes = entry.name.as_bytes();
        if name_bytes.len() > u16::MAX as usize {
            return Err(format!(
                "Entry name too long for web pack: {} ({} bytes)",
                entry.name,
                name_bytes.len()
            ));
        }

        out.extend_from_slice(&(name_bytes.len() as u16).to_be_bytes());
        out.extend_from_slice(name_bytes);
        out.extend_from_slice(&entry.file_type);
        out.extend_from_slice(&(entry.data.len() as u32).to_be_bytes());
        out.extend_from_slice(&entry.data);
        out.extend_from_slice(&(entry.rsrc.len() as u32).to_be_bytes());
        out.extend_from_slice(&entry.rsrc);
    }

    Ok(out)
}

/// Load a game from a file path, trying macOS resource fork first, then MacBinary/StuffIt.
pub fn load_game_from_path(
    runner: &mut FixtureRunner,
    path: &std::path::Path,
) -> Result<LoadedApp, String> {
    let file_data =
        std::fs::read(path).map_err(|e| format!("Failed to read {}: {}", path.display(), e))?;

    // Explicit containers in the data fork win over any host macOS resource
    // fork. DiskCopy images extracted by unar, for example, can carry a small
    // Finder metadata resource fork on the host; that is not the launchable app.
    if file_data.starts_with(WEB_PACK_MAGIC)
        || is_stuffit_archive(&file_data)
        || crate::disk_image::looks_like_dc42_or_hfs(&file_data)
    {
        return load_game(runner, &file_data);
    }

    // Try loading resource fork from macOS extended attribute path first
    let rsrc_path = path.join("..namedfork/rsrc");
    if let Ok(rsrc_data) = std::fs::read(&rsrc_path) {
        if !rsrc_data.is_empty() {
            if crate::runner::trace_load_enabled() {
                eprintln!("[LOAD] Loading resource fork from {}", rsrc_path.display());
            }
            let app_name = path
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or("FixtureGen");
            runner.dispatcher_mut().set_launched_app_path(app_name);
            let fork = ResourceFork::parse(&rsrc_data).ok_or("Failed to parse Resource Fork")?;
            return runner
                .load_app(&fork)
                .ok_or_else(|| "Failed to load app".to_string());
        }
    }

    // Fall back to detecting MacBinary/raw resource-fork style payloads.
    load_game(runner, &file_data)
}

/// Initialize a runner after loading: run init_app then clear the
/// screen so the initial framebuffer is a known state for screenshots.
pub fn init_game(runner: &mut FixtureRunner, app: &LoadedApp) {
    runner.init_app(app);

    // Clear screen memory to black.
    // For 8bpp, index 255 = black in the standard Mac CLUT.
    // For 1bpp, 0xFF = black (all bits set).
    {
        let (scrn_base, row_bytes, _, scrn_height, _) = runner.dispatcher().screen_mode;
        let bus = runner.bus_mut();
        for i in 0..(row_bytes * scrn_height as u32) {
            bus.write_byte(scrn_base + i, 0xFF);
        }
    }
}

fn load_stuffit(runner: &mut FixtureRunner, file_data: &[u8]) -> Result<LoadedApp, String> {
    let archive =
        SitArchive::parse(file_data).map_err(|e| format!("Failed to parse StuffIt: {:?}", e))?;

    let mut executable_entry: Option<(String, Vec<u8>, bool, usize)> = None;

    for entry in &archive.entries {
        if entry.is_folder {
            // Register folder in VFS so directory lookups (e.g. Plug-Ins) succeed.
            let normalized = crate::trap::dispatch::TrapDispatcher::normalize_vfs_path(&entry.name);
            runner.dispatcher_mut().ensure_vfs_directory(&normalized);
            continue;
        }

        let (data, rsrc) = entry
            .decompressed_forks()
            .map_err(|e| format!("Decompress error: {:?}", e))?;

        let payload = payload_from_forks(
            &entry.name,
            data,
            rsrc,
            entry.file_type,
            entry.creator,
            entry.finder_flags,
        )?;
        insert_payload_into_vfs(runner, payload, &mut executable_entry);
    }

    log_vfs(runner);

    let (exe_name, rsrc_data, _, _) = executable_entry.ok_or("No executable found in archive")?;
    if crate::runner::trace_load_enabled() {
        eprintln!("[LOAD] Selected executable: {}", exe_name);
    }
    runner.dispatcher_mut().set_launched_app_path(&exe_name);

    let fork = ResourceFork::parse(&rsrc_data).ok_or("Failed to parse resource fork")?;
    runner
        .load_app(&fork)
        .ok_or_else(|| "Failed to load app".to_string())
}

fn load_macbinary(runner: &mut FixtureRunner, file_data: &[u8]) -> Result<LoadedApp, String> {
    if file_data.len() < 128 {
        return Err("File too small for MacBinary".to_string());
    }

    let data_len =
        u32::from_be_bytes([file_data[83], file_data[84], file_data[85], file_data[86]]) as usize;
    let rsrc_len =
        u32::from_be_bytes([file_data[87], file_data[88], file_data[89], file_data[90]]) as usize;

    let data_start = 128;
    let data_end_padded = data_start + ((data_len + 127) & !127);
    let rsrc_start = data_end_padded;

    if rsrc_start + rsrc_len > file_data.len() {
        return Err("MacBinary truncated".to_string());
    }

    let data_end = data_start
        .checked_add(data_len)
        .ok_or_else(|| "MacBinary data offset overflow".to_string())?;
    let data_fork = &file_data[data_start..data_end];
    if is_stuffit_archive(data_fork) {
        if crate::runner::trace_load_enabled() {
            eprintln!("[LOAD] MacBinary data fork contains StuffIt archive");
        }
        return load_stuffit(runner, data_fork);
    }
    if crate::disk_image::looks_like_dc42_or_hfs(data_fork) {
        if crate::runner::trace_load_enabled() {
            eprintln!("[LOAD] MacBinary data fork contains HFS disk image");
        }
        return load_disk_image(runner, data_fork);
    }

    if crate::runner::trace_load_enabled() {
        eprintln!("[LOAD] Loading from MacBinary format");
    }

    // Extract the filename from MacBinary header (length at offset 1,
    // bytes at offset 2..65, max 63 chars) so GetAppParms / CurApName
    // have the app name to report.
    let name_len = (file_data[1] as usize).min(63);
    let name_bytes = &file_data[2..2 + name_len];
    let app_name = std::str::from_utf8(name_bytes).unwrap_or("FixtureGen");
    runner.dispatcher_mut().set_launched_app_path(app_name);

    let rsrc_data = &file_data[rsrc_start..rsrc_start + rsrc_len];
    let fork = ResourceFork::parse(rsrc_data).ok_or("Failed to parse resource fork")?;
    runner
        .load_app(&fork)
        .ok_or_else(|| "Failed to load app".to_string())
}

fn load_disk_image(runner: &mut FixtureRunner, file_data: &[u8]) -> Result<LoadedApp, String> {
    let image = crate::disk_image::extract_dc42_or_hfs(file_data)?
        .ok_or_else(|| "Not a DC42/raw HFS disk image".to_string())?;

    let mut executable_entry: Option<(String, Vec<u8>, bool, usize)> = None;
    insert_payload_into_vfs(
        runner,
        payload_from_disk_image(image),
        &mut executable_entry,
    );
    log_vfs(runner);

    let (exe_name, rsrc_data, _, _) =
        executable_entry.ok_or("No executable found in disk image")?;
    if crate::runner::trace_load_enabled() {
        eprintln!("[LOAD] Selected executable: {}", exe_name);
    }
    runner.dispatcher_mut().set_launched_app_path(&exe_name);

    let fork = ResourceFork::parse(&rsrc_data).ok_or("Failed to parse resource fork")?;
    runner
        .load_app(&fork)
        .ok_or_else(|| "Failed to load app".to_string())
}

fn load_web_pack(runner: &mut FixtureRunner, file_data: &[u8]) -> Result<LoadedApp, String> {
    let mut offset = WEB_PACK_MAGIC.len();
    let entry_count = read_u32_be(file_data, &mut offset)? as usize;
    let mut executable_entry: Option<(String, Vec<u8>, bool, usize)> = None;

    for _ in 0..entry_count {
        let name_len = read_u16_be(file_data, &mut offset)? as usize;
        let name_bytes = read_exact(file_data, &mut offset, name_len)?;
        let name = String::from_utf8(name_bytes.to_vec())
            .map_err(|_| "Invalid UTF-8 in web pack entry name".to_string())?;

        let file_type = read_exact(file_data, &mut offset, 4)?;
        let mut file_type_code = [0u8; 4];
        file_type_code.copy_from_slice(file_type);
        let is_appl = file_type_code == *b"APPL";

        let data_len = read_u32_be(file_data, &mut offset)? as usize;
        let data = read_exact(file_data, &mut offset, data_len)?.to_vec();

        let rsrc_len = read_u32_be(file_data, &mut offset)? as usize;
        let rsrc = read_exact(file_data, &mut offset, rsrc_len)?.to_vec();

        insert_forks_into_vfs(
            runner,
            &name,
            data,
            rsrc.clone(),
            file_type_code,
            *b"????",
            0,
        );
        maybe_select_executable(&mut executable_entry, &name, &rsrc, is_appl, data_len);
    }

    log_vfs(runner);

    let (exe_name, rsrc_data, _, _) = executable_entry.ok_or("No executable found in web pack")?;
    if crate::runner::trace_load_enabled() {
        eprintln!("[LOAD] Selected executable: {}", exe_name);
    }
    runner.dispatcher_mut().set_launched_app_path(&exe_name);

    let fork = ResourceFork::parse(&rsrc_data).ok_or("Failed to parse resource fork")?;
    runner
        .load_app(&fork)
        .ok_or_else(|| "Failed to load app".to_string())
}

fn insert_forks_into_vfs(
    runner: &mut FixtureRunner,
    name: &str,
    data: Vec<u8>,
    rsrc: Vec<u8>,
    file_type: [u8; 4],
    creator: [u8; 4],
    finder_flags: u16,
) {
    let normalized_name = crate::trap::dispatch::TrapDispatcher::normalize_vfs_path(name);
    // If data fork is empty but resource fork doesn't parse as a resource fork,
    // use resource fork bytes as data fork (some archives have forks swapped).
    if data.is_empty() && !rsrc.is_empty() && ResourceFork::parse(&rsrc).is_none() {
        runner
            .dispatcher_mut()
            .vfs
            .insert(normalized_name.clone(), rsrc.clone());
    } else {
        runner
            .dispatcher_mut()
            .vfs
            .insert(normalized_name.clone(), data);
    }

    if !rsrc.is_empty() {
        runner
            .dispatcher_mut()
            .vfs_rsrc
            .insert(normalized_name.clone(), rsrc);
    }

    runner.dispatcher_mut().set_vfs_entry_metadata(
        &normalized_name,
        file_type,
        creator,
        finder_flags,
    );
}

#[derive(Debug)]
struct Payload {
    dirs: Vec<String>,
    files: Vec<PayloadFile>,
}

#[derive(Debug)]
struct PayloadFile {
    name: String,
    data: Vec<u8>,
    rsrc: Vec<u8>,
    file_type: [u8; 4],
    creator: [u8; 4],
    finder_flags: u16,
}

fn collect_stuffit_payload_files(archive: &SitArchive) -> Result<Vec<PayloadFile>, String> {
    let mut files = Vec::new();
    for entry in archive.entries.iter().filter(|entry| !entry.is_folder) {
        let (data, rsrc) = entry
            .decompressed_forks()
            .map_err(|e| format!("Decompress error: {:?}", e))?;
        let payload = payload_from_forks(
            &entry.name,
            data,
            rsrc,
            entry.file_type,
            entry.creator,
            entry.finder_flags,
        )?;
        files.extend(payload.files);
    }
    Ok(files)
}

fn payload_from_forks(
    name: &str,
    data: Vec<u8>,
    rsrc: Vec<u8>,
    file_type: [u8; 4],
    creator: [u8; 4],
    finder_flags: u16,
) -> Result<Payload, String> {
    if let Some(image) = crate::disk_image::extract_dc42_or_hfs(&data)
        .map_err(|e| format!("Disk image {name}: {e}"))?
    {
        if crate::runner::trace_load_enabled() {
            eprintln!(
                "[LOAD] Extracting HFS disk image \"{}\" from data fork: volume \"{}\", {} files",
                name,
                image.volume_name,
                image.files.len()
            );
        }
        return Ok(payload_from_disk_image(image));
    }

    if let Some(image) = crate::disk_image::extract_dc42_or_hfs(&rsrc)
        .map_err(|e| format!("Disk image {name}: {e}"))?
    {
        if crate::runner::trace_load_enabled() {
            eprintln!(
                "[LOAD] Extracting HFS disk image \"{}\" from resource fork: volume \"{}\", {} files",
                name,
                image.volume_name,
                image.files.len()
            );
        }
        return Ok(payload_from_disk_image(image));
    }

    Ok(Payload {
        dirs: Vec::new(),
        files: vec![PayloadFile {
            name: name.to_string(),
            data,
            rsrc,
            file_type,
            creator,
            finder_flags,
        }],
    })
}

fn payload_from_disk_image(image: crate::disk_image::DiskImageContents) -> Payload {
    Payload {
        dirs: image.dirs,
        files: image
            .files
            .into_iter()
            .map(|file| PayloadFile {
                name: file.path,
                data: file.data,
                rsrc: file.rsrc,
                file_type: file.file_type,
                creator: file.creator,
                finder_flags: file.finder_flags,
            })
            .collect(),
    }
}

fn insert_payload_into_vfs(
    runner: &mut FixtureRunner,
    payload: Payload,
    executable_entry: &mut Option<(String, Vec<u8>, bool, usize)>,
) {
    for dir in payload.dirs {
        let normalized = crate::trap::dispatch::TrapDispatcher::normalize_vfs_path(&dir);
        runner.dispatcher_mut().ensure_vfs_directory(&normalized);
    }

    for file in payload.files {
        let data_len = file.data.len();
        let is_appl = file.file_type == *b"APPL";
        let rsrc = file.rsrc;
        insert_forks_into_vfs(
            runner,
            &file.name,
            file.data,
            rsrc.clone(),
            file.file_type,
            file.creator,
            file.finder_flags,
        );
        maybe_select_executable(executable_entry, &file.name, &rsrc, is_appl, data_len);
    }
}

fn maybe_select_executable(
    executable_entry: &mut Option<(String, Vec<u8>, bool, usize)>,
    name: &str,
    rsrc: &[u8],
    is_appl: bool,
    data_len: usize,
) {
    if rsrc.is_empty() {
        return;
    }

    let is_executable = if let Some(fork) = ResourceFork::parse(rsrc) {
        fork.get_code(0).is_some()
    } else {
        false
    };
    if !is_executable {
        return;
    }

    // SYSTEMLESS_LOAD_EXECUTABLE: case-sensitive substring match against the
    // archive entry name. When the env var is set and a candidate matches
    // it wins outright over the size/APPL heuristic — needed for archives
    // that contain multiple bootable executables (e.g. Prince of
    // Destruction ships both "MARS™ Player" and a larger "MARS™ Master
    // (sw)" authoring variant; the heuristic picks the larger Master,
    // but the user-facing runtime is Player).
    let override_match = executable_name_override()
        .map(|needle| name.contains(needle.as_str()))
        .unwrap_or(false);
    let prev_override_match = match (executable_entry.as_ref(), executable_name_override()) {
        (Some((prev_name, _, _, _)), Some(needle)) => prev_name.contains(needle.as_str()),
        _ => false,
    };

    let score = data_len.max(rsrc.len());
    let (prev_is_appl, prev_score) = executable_entry
        .as_ref()
        .map(|(_, _, appl, dlen)| (*appl, *dlen))
        .unwrap_or((false, 0));

    let take = if override_match && !prev_override_match {
        true
    } else if !override_match && prev_override_match {
        false
    } else {
        executable_entry.is_none()
            || (is_appl && !prev_is_appl)
            || (is_appl == prev_is_appl && score > prev_score)
    };

    if take {
        *executable_entry = Some((name.to_string(), rsrc.to_vec(), is_appl, score));
    }
}

fn executable_name_override() -> Option<String> {
    std::env::var("SYSTEMLESS_LOAD_EXECUTABLE")
        .ok()
        .filter(|s| !s.is_empty())
}

fn is_stuffit_archive(bytes: &[u8]) -> bool {
    bytes.len() >= 80 && (&bytes[0..4] == b"SIT!" || &bytes[0..7] == b"StuffIt")
}

fn log_vfs(runner: &FixtureRunner) {
    if !crate::runner::trace_load_enabled() {
        return;
    }
    eprintln!("[VFS] Data fork entries:");
    for key in runner.dispatcher().vfs.keys() {
        let size = runner
            .dispatcher()
            .vfs
            .get(key)
            .map(|v| v.len())
            .unwrap_or(0);
        eprintln!("  \"{}\" ({} bytes)", key, size);
    }
    eprintln!("[VFS] Resource fork entries:");
    for key in runner.dispatcher().vfs_rsrc.keys() {
        let size = runner
            .dispatcher()
            .vfs_rsrc
            .get(key)
            .map(|v| v.len())
            .unwrap_or(0);
        eprintln!("  \"{}\" ({} bytes)", key, size);
    }
}
fn read_exact<'a>(buf: &'a [u8], offset: &mut usize, len: usize) -> Result<&'a [u8], String> {
    let end = offset
        .checked_add(len)
        .ok_or_else(|| "Web pack offset overflow".to_string())?;
    if end > buf.len() {
        return Err("Web pack truncated".to_string());
    }
    let slice = &buf[*offset..end];
    *offset = end;
    Ok(slice)
}

fn read_u16_be(buf: &[u8], offset: &mut usize) -> Result<u16, String> {
    let bytes = read_exact(buf, offset, 2)?;
    Ok(u16::from_be_bytes([bytes[0], bytes[1]]))
}

fn read_u32_be(buf: &[u8], offset: &mut usize) -> Result<u32, String> {
    let bytes = read_exact(buf, offset, 4)?;
    Ok(u32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
}
