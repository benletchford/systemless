//! Shared game loading and initialization for all Systemless frontends.
//!
//! Consolidates application loading (BinHex, MacBinary, StuffIt, ZIP, and web
//! packs), runner initialization, and post-load configuration so all
//! frontends behave identically.

use crate::loader::LoadedApp;
use crate::managers::resource::ResourceFork;
use crate::memory::MemoryBus;
use crate::runner::{FixtureRunner, FixtureRunnerConfig};
use std::io::{Cursor, Read};
use stuffit::SitArchive;

const LEGACY_WEB_PACK_MAGIC: &[u8; 4] = b"KPK1";
const WEB_PACK_MAGIC: &[u8; 4] = b"KPK2";
const WEB_PACK_INITIAL_FORK_RESERVE_BYTES: usize = 1024 * 1024;
const MAX_ZIP_UNCOMPRESSED_BYTES: u64 = 512 * 1024 * 1024;

fn is_web_pack(file_data: &[u8]) -> bool {
    file_data.starts_with(WEB_PACK_MAGIC) || file_data.starts_with(LEGACY_WEB_PACK_MAGIC)
}

fn is_zip_archive(file_data: &[u8]) -> bool {
    matches!(
        file_data.get(..4),
        Some(b"PK\x03\x04" | b"PK\x05\x06" | b"PK\x07\x08")
    )
}

/// Standard frontend RAM size from the canonical machine profile.
pub const RAM_SIZE: u32 = crate::machine_profile::REFERENCE_MACHINE_PROFILE.ram_size_bytes;

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

/// Load an application from BinHex, MacBinary, StuffIt, ZIP, web-pack, or raw
/// resource-fork bytes.
///
/// Handles multi-file StuffIt and ZIP archives (populates VFS with all entries,
/// finds an executable), MacBinary files, and macOS resource fork paths.
/// Returns the LoadedApp on success.
pub fn load_game(runner: &mut FixtureRunner, file_data: &[u8]) -> Result<LoadedApp, String> {
    if is_web_pack(file_data) {
        load_web_pack(runner, file_data)
    } else if is_zip_archive(file_data) {
        load_zip(runner, file_data)
    } else if is_stuffit_archive(file_data) {
        load_stuffit(runner, file_data)
    } else if crate::binhex::looks_like_binhex(file_data) {
        load_binhex(runner, file_data)
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
    pack_payload_files_for_web(file_entries)
}

/// Prepack one or more supported game containers into a single web pack.
///
/// This is useful for CD-based games whose application and read-only data
/// volume were distributed separately. Optional path prefixes retain only the
/// runtime files needed from large compilation discs.
pub fn pack_game_sources_for_web(
    sources: &[&[u8]],
    include_prefixes: &[&str],
) -> Result<Vec<u8>, String> {
    let mut file_entries = Vec::new();
    for source in sources {
        if is_stuffit_archive(source) {
            let archive = SitArchive::parse(source)
                .map_err(|e| format!("Failed to parse StuffIt: {:?}", e))?;
            file_entries.extend(collect_stuffit_payload_files(&archive)?);
        } else if let Some(image) = crate::disk_image::extract_dc42_or_hfs(source)? {
            file_entries.extend(payload_from_disk_image(image)?.files);
        } else {
            return Err("Game source is not a StuffIt archive or HFS disk image".to_string());
        }
    }

    if !include_prefixes.is_empty() {
        let normalized_prefixes = include_prefixes
            .iter()
            .map(|prefix| crate::trap::dispatch::TrapDispatcher::normalize_vfs_path(prefix))
            .collect::<Vec<_>>();
        file_entries.retain(|entry| {
            let path = crate::trap::dispatch::TrapDispatcher::normalize_vfs_path(&entry.name);
            normalized_prefixes
                .iter()
                .any(|prefix| vfs_path_matches_remove(&path, prefix))
        });
    }

    if file_entries.is_empty() {
        return Err("No files matched the requested game sources".to_string());
    }

    pack_payload_files_for_web(file_entries)
}

fn pack_payload_files_for_web(file_entries: Vec<PayloadFile>) -> Result<Vec<u8>, String> {
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
        out.extend_from_slice(&entry.creator);
        out.extend_from_slice(&entry.finder_flags.to_be_bytes());
        out.extend_from_slice(&(entry.data.len() as u32).to_be_bytes());
        out.extend_from_slice(&entry.data);
        out.extend_from_slice(&(entry.rsrc.len() as u32).to_be_bytes());
        out.extend_from_slice(&entry.rsrc);
    }

    Ok(out)
}

/// Load a game from a file path, trying explicit containers before macOS resource forks.
pub fn load_game_from_path(
    runner: &mut FixtureRunner,
    path: &std::path::Path,
) -> Result<LoadedApp, String> {
    let file_data =
        std::fs::read(path).map_err(|e| format!("Failed to read {}: {}", path.display(), e))?;

    // Explicit containers in the data fork win over any host macOS resource
    // fork. DiskCopy images extracted by unar, for example, can carry a small
    // Finder metadata resource fork on the host; that is not the launchable app.
    if is_web_pack(&file_data)
        || is_stuffit_archive(&file_data)
        || is_zip_archive(&file_data)
        || crate::binhex::looks_like_binhex(&file_data)
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

fn load_zip(runner: &mut FixtureRunner, file_data: &[u8]) -> Result<LoadedApp, String> {
    let cursor = Cursor::new(file_data);
    let mut archive =
        zip::ZipArchive::new(cursor).map_err(|e| format!("Failed to parse ZIP: {e}"))?;
    let mut executable_entry: Option<ExecutableCandidate> = None;
    let mut skipped_disk_image_errors = Vec::new();
    let mut dirs = Vec::new();
    let mut payloads = Vec::new();
    let mut uncompressed_bytes = 0u64;

    for index in 0..archive.len() {
        let mut entry = archive
            .by_index(index)
            .map_err(|e| format!("Failed to read ZIP entry {index}: {e}"))?;
        let raw_name = entry.name().to_string();
        let enclosed = entry
            .enclosed_name()
            .ok_or_else(|| format!("Unsafe ZIP entry path: {raw_name}"))?;
        if raw_name.contains('\\')
            || raw_name
                .split('/')
                .any(|component| matches!(component, "." | ".."))
        {
            return Err(format!("Unsafe ZIP entry path: {raw_name}"));
        }
        let name = enclosed
            .components()
            .map(|component| component.as_os_str().to_string_lossy())
            .collect::<Vec<_>>()
            .join("/");
        if name.is_empty() {
            return Err("ZIP entry has an empty path".to_string());
        }
        if entry.is_dir() {
            dirs.push(crate::trap::dispatch::TrapDispatcher::normalize_vfs_path(
                &name,
            ));
            continue;
        }

        uncompressed_bytes = uncompressed_bytes
            .checked_add(entry.size())
            .ok_or_else(|| "ZIP uncompressed size overflow".to_string())?;
        if uncompressed_bytes > MAX_ZIP_UNCOMPRESSED_BYTES {
            return Err(format!(
                "ZIP expands beyond the {} byte safety limit",
                MAX_ZIP_UNCOMPRESSED_BYTES
            ));
        }
        let capacity =
            usize::try_from(entry.size()).map_err(|_| format!("ZIP entry is too large: {name}"))?;
        let mut bytes = Vec::with_capacity(capacity);
        entry
            .read_to_end(&mut bytes)
            .map_err(|e| format!("Failed to decompress ZIP entry {name}: {e}"))?;

        let payload = if looks_like_macbinary(&bytes) {
            payload_from_macbinary(&name, &bytes)?
        } else {
            payload_from_forks(&name, bytes, Vec::new(), *b"????", *b"????", 0)?
        };
        skipped_disk_image_errors.extend(payload.skipped_disk_image_errors.iter().cloned());
        payloads.push(payload);
    }

    // Do not mutate the VFS until every ZIP entry has passed path validation
    // and decompression. A malformed trailing entry therefore cannot leave a
    // partially mounted archive behind.
    for dir in dirs {
        runner.dispatcher_mut().ensure_vfs_directory(&dir);
    }
    for payload in payloads {
        insert_payload_into_vfs(runner, payload, &mut executable_entry);
    }

    log_vfs(runner);
    let executable =
        executable_entry.ok_or_else(|| no_executable_archive_error(&skipped_disk_image_errors))?;
    if crate::runner::trace_load_enabled() {
        eprintln!("[LOAD] Selected executable: {}", executable.name);
    }
    load_selected_executable(runner, &executable)
}

/// Initialize a runner after loading: run init_app then clear the
/// screen so the initial framebuffer is a known state for screenshots.
pub fn init_game(runner: &mut FixtureRunner, app: &LoadedApp) {
    runner.init_app(app);

    {
        if runner.menu_bar_visible() {
            let (scrn_base, row_bytes, screen_width, screen_height, pixel_size) =
                runner.dispatcher().screen_mode;
            crate::trap::TrapDispatcher::fb_fill_pattern_rect(
                runner.bus_mut(),
                scrn_base,
                row_bytes,
                pixel_size,
                screen_width as i16,
                screen_height as i16,
                0,
                0,
                screen_height as i16,
                screen_width as i16,
                [0xAA, 0x55, 0xAA, 0x55, 0xAA, 0x55, 0xAA, 0x55],
            );
            return;
        }

        // Clear screen memory to black.
        // For 8bpp, index 255 = black in the standard Mac CLUT.
        // For 1bpp, 0xFF = black (all bits set).
        let (scrn_base, row_bytes, _, scrn_height, _) = runner.dispatcher().screen_mode;
        runner
            .bus_mut()
            .fill_bytes(scrn_base, row_bytes * scrn_height as u32, 0xFF);
    }
}

fn load_stuffit(runner: &mut FixtureRunner, file_data: &[u8]) -> Result<LoadedApp, String> {
    let archive =
        SitArchive::parse(file_data).map_err(|e| format!("Failed to parse StuffIt: {:?}", e))?;

    let mut executable_entry: Option<ExecutableCandidate> = None;
    let mut skipped_disk_image_errors = Vec::new();

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
        skipped_disk_image_errors.extend(payload.skipped_disk_image_errors.iter().cloned());
        insert_payload_into_vfs(runner, payload, &mut executable_entry);
    }

    log_vfs(runner);

    let executable =
        executable_entry.ok_or_else(|| no_executable_archive_error(&skipped_disk_image_errors))?;
    if crate::runner::trace_load_enabled() {
        eprintln!("[LOAD] Selected executable: {}", executable.name);
    }
    load_selected_executable(runner, &executable)
}

fn load_binhex(runner: &mut FixtureRunner, file_data: &[u8]) -> Result<LoadedApp, String> {
    let file = crate::binhex::decode(file_data)?.ok_or_else(|| "Not a BinHex file".to_string())?;

    if crate::runner::trace_load_enabled() {
        eprintln!("[LOAD] Decoded BinHex file: {}", file.name);
    }
    if is_stuffit_archive(&file.data) {
        return load_stuffit(runner, &file.data);
    }
    if crate::disk_image::looks_like_dc42_or_hfs(&file.data) {
        if crate::runner::trace_load_enabled() {
            eprintln!("[LOAD] BinHex data fork contains HFS disk image");
        }
        return load_disk_image(runner, &file.data);
    }

    let mut executable_entry: Option<ExecutableCandidate> = None;
    insert_payload_into_vfs(
        runner,
        payload_from_forks(
            &file.name,
            file.data,
            file.rsrc,
            file.file_type,
            file.creator,
            file.finder_flags,
        )?,
        &mut executable_entry,
    );
    log_vfs(runner);

    let executable = executable_entry.ok_or("No executable found in BinHex file")?;
    if crate::runner::trace_load_enabled() {
        eprintln!("[LOAD] Selected executable: {}", executable.name);
    }
    load_selected_executable(runner, &executable)
}

fn load_macbinary(runner: &mut FixtureRunner, file_data: &[u8]) -> Result<LoadedApp, String> {
    let decoded = decode_macbinary(file_data)?;
    let data_fork = decoded.data.as_slice();
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

    let app_name = decoded.name.as_str();
    insert_forks_into_vfs(
        runner,
        app_name,
        decoded.data.clone(),
        decoded.rsrc.clone(),
        decoded.file_type,
        decoded.creator,
        decoded.finder_flags,
    );

    let executable = ExecutableCandidate {
        name: app_name.to_string(),
        vfs_key: crate::trap::dispatch::TrapDispatcher::normalize_vfs_path(app_name),
        is_appl: decoded.file_type == *b"APPL",
        has_data_fork: !data_fork.is_empty(),
        score: data_fork.len().max(decoded.rsrc.len()),
        creator: decoded.creator,
    };
    load_selected_executable(runner, &executable)
}

struct MacBinaryFile {
    name: String,
    data: Vec<u8>,
    rsrc: Vec<u8>,
    file_type: [u8; 4],
    creator: [u8; 4],
    finder_flags: u16,
}

fn looks_like_macbinary(file_data: &[u8]) -> bool {
    file_data.len() >= 128
        && file_data[0] == 0
        && (1..=63).contains(&file_data[1])
        && file_data[74] == 0
}

fn decode_macbinary(file_data: &[u8]) -> Result<MacBinaryFile, String> {
    if file_data.len() < 128 {
        return Err("File too small for MacBinary".to_string());
    }
    if !looks_like_macbinary(file_data) {
        return Err("Invalid MacBinary header".to_string());
    }

    let data_len =
        u32::from_be_bytes([file_data[83], file_data[84], file_data[85], file_data[86]]) as usize;
    let rsrc_len =
        u32::from_be_bytes([file_data[87], file_data[88], file_data[89], file_data[90]]) as usize;
    let data_start = 128usize;
    let data_end = data_start
        .checked_add(data_len)
        .ok_or_else(|| "MacBinary data offset overflow".to_string())?;
    let rsrc_start = data_start
        .checked_add(
            data_len
                .checked_add(127)
                .ok_or("MacBinary data length overflow")?
                & !127,
        )
        .ok_or_else(|| "MacBinary resource offset overflow".to_string())?;
    let rsrc_end = rsrc_start
        .checked_add(rsrc_len)
        .ok_or_else(|| "MacBinary resource length overflow".to_string())?;
    if data_end > file_data.len() || rsrc_end > file_data.len() {
        return Err("MacBinary truncated".to_string());
    }

    let name_len = file_data[1] as usize;
    let name = String::from_utf8_lossy(&file_data[2..2 + name_len]).into_owned();
    Ok(MacBinaryFile {
        name,
        data: file_data[data_start..data_end].to_vec(),
        rsrc: file_data[rsrc_start..rsrc_end].to_vec(),
        file_type: file_data[65..69].try_into().unwrap(),
        creator: file_data[69..73].try_into().unwrap(),
        finder_flags: (u16::from(file_data[73]) << 8) | u16::from(file_data[101]),
    })
}

fn payload_from_macbinary(container_name: &str, file_data: &[u8]) -> Result<Payload, String> {
    let decoded = decode_macbinary(file_data)?;
    let parent = container_name
        .rsplit_once('/')
        .map_or("", |(parent, _)| parent);
    let name = if parent.is_empty() {
        decoded.name
    } else {
        format!("{parent}/{}", decoded.name)
    };
    payload_from_forks(
        &name,
        decoded.data,
        decoded.rsrc,
        decoded.file_type,
        decoded.creator,
        decoded.finder_flags,
    )
}

fn no_executable_archive_error(skipped_disk_image_errors: &[String]) -> String {
    skipped_disk_image_errors.first().map_or_else(
        || "No executable found in archive".to_string(),
        |err| format!("No executable found in archive; skipped nested disk image: {err}"),
    )
}

fn load_disk_image(runner: &mut FixtureRunner, file_data: &[u8]) -> Result<LoadedApp, String> {
    let image = crate::disk_image::extract_dc42_or_hfs(file_data)?
        .ok_or_else(|| "Not a DC42/raw HFS disk image".to_string())?;

    let mut executable_entry: Option<ExecutableCandidate> = None;
    insert_payload_into_vfs(
        runner,
        payload_from_disk_image(image)?,
        &mut executable_entry,
    );
    log_vfs(runner);

    let executable = executable_entry.ok_or("No executable found in disk image")?;
    if crate::runner::trace_load_enabled() {
        eprintln!("[LOAD] Selected executable: {}", executable.name);
    }
    load_selected_executable(runner, &executable)
}

fn load_web_pack(runner: &mut FixtureRunner, file_data: &[u8]) -> Result<LoadedApp, String> {
    let mut loader =
        WebPackLoader::new(runner, file_data)?.ok_or_else(|| "Not a web pack".to_string())?;
    while !loader.load_next_chunk(runner, usize::MAX)? {}
    loader.finish(runner)
}

/// Incremental loader for Systemless web packs (`KPK1` and `KPK2`).
///
/// The standard `load_game` path consumes the whole pack synchronously. Browser
/// frontends can use this loader to copy large data/resource forks in bounded
/// chunks and yield to the event loop between calls.
pub struct WebPackLoader<'a> {
    file_data: &'a [u8],
    offset: usize,
    total_entries: usize,
    loaded_entries: usize,
    executable_entry: Option<ExecutableCandidate>,
    pending: Option<WebPackPendingEntry>,
    remove_paths: Vec<String>,
    has_finder_metadata: bool,
}

impl<'a> WebPackLoader<'a> {
    pub fn new(runner: &mut FixtureRunner, file_data: &'a [u8]) -> Result<Option<Self>, String> {
        Self::new_with_remove_paths(runner, file_data, &[])
    }

    pub fn new_with_remove_paths(
        runner: &mut FixtureRunner,
        file_data: &'a [u8],
        remove_paths: &[&str],
    ) -> Result<Option<Self>, String> {
        if !is_web_pack(file_data) {
            return Ok(None);
        }

        let mut offset = WEB_PACK_MAGIC.len();
        let has_finder_metadata = file_data.starts_with(WEB_PACK_MAGIC);
        let total_entries = read_u32_be(file_data, &mut offset)? as usize;
        {
            let dispatcher = runner.dispatcher_mut();
            dispatcher.vfs.reserve(total_entries);
            dispatcher.vfs_rsrc.reserve(total_entries);
            dispatcher.vfs_metadata.reserve(total_entries);
        }

        Ok(Some(Self {
            file_data,
            offset,
            total_entries,
            loaded_entries: 0,
            executable_entry: None,
            pending: None,
            has_finder_metadata,
            remove_paths: remove_paths
                .iter()
                .map(|path| crate::trap::dispatch::TrapDispatcher::normalize_vfs_path(path))
                .filter(|path| !path.is_empty())
                .collect(),
        }))
    }

    pub fn total_entries(&self) -> usize {
        self.total_entries
    }

    pub fn loaded_entries(&self) -> usize {
        self.loaded_entries
    }

    pub fn archive_bytes_total(&self) -> usize {
        self.file_data.len()
    }

    pub fn archive_bytes_loaded(&self) -> usize {
        self.pending
            .as_ref()
            .map_or(self.offset, WebPackPendingEntry::archive_bytes_loaded)
    }

    /// Copy and mount up to `max_bytes` of fork payload. Returns `true` once
    /// all entries have been mounted and `finish` can be called.
    pub fn load_next_chunk(
        &mut self,
        runner: &mut FixtureRunner,
        max_bytes: usize,
    ) -> Result<bool, String> {
        let mut remaining = max_bytes.max(1);

        while remaining > 0 && self.loaded_entries < self.total_entries {
            if self.pending.is_none() {
                let header = self.read_next_entry_header()?;
                if self.should_skip_entry(&header.name) {
                    self.loaded_entries += 1;
                    continue;
                }
                self.pending = Some(WebPackPendingEntry::new(header));
            }

            let copied = {
                let pending = self.pending.as_mut().expect("pending web-pack entry");
                pending.copy_next_chunk(self.file_data, remaining)
            };
            remaining = remaining.saturating_sub(copied);

            if self
                .pending
                .as_ref()
                .is_some_and(WebPackPendingEntry::is_complete)
            {
                let pending = self.pending.take().expect("complete web-pack entry");
                maybe_select_executable(
                    &mut self.executable_entry,
                    &pending.name,
                    &pending.rsrc,
                    pending.is_appl,
                    pending.data_len,
                    pending.creator_code,
                );
                insert_forks_into_vfs(
                    runner,
                    &pending.name,
                    pending.data,
                    pending.rsrc,
                    pending.file_type_code,
                    pending.creator_code,
                    pending.finder_flags,
                );
                self.loaded_entries += 1;
            } else if copied == 0 {
                break;
            }
        }

        Ok(self.loaded_entries == self.total_entries && self.pending.is_none())
    }

    pub fn finish(self, runner: &mut FixtureRunner) -> Result<LoadedApp, String> {
        if self.loaded_entries != self.total_entries || self.pending.is_some() {
            return Err("Web pack load is not complete".to_string());
        }

        log_vfs(runner);

        let executable = self
            .executable_entry
            .ok_or("No executable found in web pack")?;
        if crate::runner::trace_load_enabled() {
            eprintln!("[LOAD] Selected executable: {}", executable.name);
        }
        load_selected_executable(runner, &executable)
    }

    fn read_next_entry_header(&mut self) -> Result<WebPackEntryHeader, String> {
        let name_len = read_u16_be(self.file_data, &mut self.offset)? as usize;
        let name_bytes = read_exact(self.file_data, &mut self.offset, name_len)?;
        let name = String::from_utf8(name_bytes.to_vec())
            .map_err(|_| "Invalid UTF-8 in web pack entry name".to_string())?;

        let file_type = read_exact(self.file_data, &mut self.offset, 4)?;
        let mut file_type_code = [0u8; 4];
        file_type_code.copy_from_slice(file_type);
        let (creator_code, finder_flags) = if self.has_finder_metadata {
            let creator = read_exact(self.file_data, &mut self.offset, 4)?;
            let mut creator_code = [0u8; 4];
            creator_code.copy_from_slice(creator);
            let finder_flags = read_u16_be(self.file_data, &mut self.offset)?;
            (creator_code, finder_flags)
        } else {
            (*b"????", 0)
        };

        let data_len = read_u32_be(self.file_data, &mut self.offset)? as usize;
        let data_start = self.offset;
        read_exact(self.file_data, &mut self.offset, data_len)?;

        let rsrc_len = read_u32_be(self.file_data, &mut self.offset)? as usize;
        let rsrc_start = self.offset;
        read_exact(self.file_data, &mut self.offset, rsrc_len)?;

        Ok(WebPackEntryHeader {
            name,
            file_type_code,
            creator_code,
            finder_flags,
            is_appl: file_type_code == *b"APPL",
            data_start,
            data_len,
            rsrc_start,
            rsrc_len,
        })
    }

    fn should_skip_entry(&self, name: &str) -> bool {
        if self.remove_paths.is_empty() {
            return false;
        }

        let normalized = crate::trap::dispatch::TrapDispatcher::normalize_vfs_path(name);
        for remove_path in &self.remove_paths {
            if vfs_path_matches_remove(&normalized, remove_path) {
                return true;
            }

            let Some(executable) = self.executable_entry.as_ref() else {
                continue;
            };
            let parent =
                crate::trap::dispatch::TrapDispatcher::vfs_parent_path(&executable.vfs_key);
            if parent.is_empty() {
                if vfs_path_matches_remove(&normalized, remove_path) {
                    return true;
                }
                continue;
            }

            let mut resolved = String::with_capacity(parent.len() + 1 + remove_path.len());
            resolved.push_str(parent);
            resolved.push('/');
            resolved.push_str(remove_path);
            if vfs_path_matches_remove(&normalized, &resolved) {
                return true;
            }
        }

        false
    }
}

struct WebPackEntryHeader {
    name: String,
    file_type_code: [u8; 4],
    creator_code: [u8; 4],
    finder_flags: u16,
    is_appl: bool,
    data_start: usize,
    data_len: usize,
    rsrc_start: usize,
    rsrc_len: usize,
}

struct WebPackPendingEntry {
    name: String,
    file_type_code: [u8; 4],
    creator_code: [u8; 4],
    finder_flags: u16,
    is_appl: bool,
    data_start: usize,
    data_len: usize,
    data_copied: usize,
    data: Vec<u8>,
    rsrc_start: usize,
    rsrc_len: usize,
    rsrc_copied: usize,
    rsrc: Vec<u8>,
}

impl WebPackPendingEntry {
    fn new(header: WebPackEntryHeader) -> Self {
        Self {
            name: header.name,
            file_type_code: header.file_type_code,
            creator_code: header.creator_code,
            finder_flags: header.finder_flags,
            is_appl: header.is_appl,
            data_start: header.data_start,
            data_len: header.data_len,
            data_copied: 0,
            data: Vec::with_capacity(initial_web_pack_fork_capacity(header.data_len)),
            rsrc_start: header.rsrc_start,
            rsrc_len: header.rsrc_len,
            rsrc_copied: 0,
            rsrc: Vec::with_capacity(initial_web_pack_fork_capacity(header.rsrc_len)),
        }
    }

    fn copy_next_chunk(&mut self, file_data: &[u8], max_bytes: usize) -> usize {
        let mut remaining = max_bytes;

        if self.data_copied < self.data_len && remaining > 0 {
            let chunk = remaining.min(self.data_len - self.data_copied);
            let start = self.data_start + self.data_copied;
            self.data
                .extend_from_slice(&file_data[start..start + chunk]);
            self.data_copied += chunk;
            remaining -= chunk;
        }

        if self.rsrc_copied < self.rsrc_len && remaining > 0 {
            let chunk = remaining.min(self.rsrc_len - self.rsrc_copied);
            let start = self.rsrc_start + self.rsrc_copied;
            self.rsrc
                .extend_from_slice(&file_data[start..start + chunk]);
            self.rsrc_copied += chunk;
            remaining -= chunk;
        }

        max_bytes - remaining
    }

    fn is_complete(&self) -> bool {
        self.data_copied == self.data_len && self.rsrc_copied == self.rsrc_len
    }

    fn archive_bytes_loaded(&self) -> usize {
        if self.data_copied < self.data_len {
            self.data_start + self.data_copied
        } else {
            self.rsrc_start + self.rsrc_copied
        }
    }
}

fn vfs_path_matches_remove(path: &str, remove_path: &str) -> bool {
    if path.eq_ignore_ascii_case(remove_path) {
        return true;
    }
    if path.as_bytes().get(remove_path.len()) != Some(&b'/') {
        return false;
    }
    path.get(..remove_path.len())
        .is_some_and(|prefix| prefix.eq_ignore_ascii_case(remove_path))
}

fn initial_web_pack_fork_capacity(len: usize) -> usize {
    len.min(WEB_PACK_INITIAL_FORK_RESERVE_BYTES)
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
    if data.is_empty() && !rsrc.is_empty() && !ResourceFork::has_valid_layout(&rsrc) {
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

    let data_backed_rsrc = rsrc.is_empty()
        && name.to_ascii_lowercase().ends_with(".rsrc")
        && ResourceFork::has_valid_layout(
            runner
                .dispatcher()
                .vfs
                .get(&normalized_name)
                .map_or(&[][..], |bytes| bytes.as_slice()),
        );

    if !rsrc.is_empty() {
        runner
            .dispatcher_mut()
            .vfs_rsrc
            .insert(normalized_name.clone(), rsrc);
    } else if data_backed_rsrc {
        let data = runner
            .dispatcher()
            .vfs
            .get(&normalized_name)
            .cloned()
            .unwrap_or_default();
        runner
            .dispatcher_mut()
            .vfs_rsrc
            .insert(normalized_name.clone(), data);
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
    skipped_disk_image_errors: Vec<String>,
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

fn expand_squz_payload_file(
    name: &str,
    data: &[u8],
    file_type: [u8; 4],
    creator: [u8; 4],
    finder_flags: u16,
) -> Result<Option<PayloadFile>, String> {
    if file_type != *b"SQUZ" || creator != *b"BrSq" {
        return Ok(None);
    }

    let Some(magic_pos) = data.windows(2).position(|window| window == b"KG") else {
        return Ok(None);
    };
    if magic_pos < 12 || magic_pos + 12 > data.len() {
        return Err(format!("SQUZ {name}: invalid header"));
    }
    let method = [data[magic_pos + 2], data[magic_pos + 3]];

    let mut target_type = [0u8; 4];
    target_type.copy_from_slice(&data[4..8]);
    let mut target_creator = [0u8; 4];
    target_creator.copy_from_slice(&data[8..12]);
    let header_finder_flags = u16::from_be_bytes([data[12], data[13]]);
    let target_finder_flags = if header_finder_flags != 0 {
        header_finder_flags
    } else {
        finder_flags
    };
    let resource_like_file = name.to_ascii_lowercase().ends_with(".rsrc");

    let uncompressed_len = u32::from_be_bytes([
        data[magic_pos + 4],
        data[magic_pos + 5],
        data[magic_pos + 6],
        data[magic_pos + 7],
    ]) as usize;
    let compressed_len = u32::from_be_bytes([
        data[magic_pos + 8],
        data[magic_pos + 9],
        data[magic_pos + 10],
        data[magic_pos + 11],
    ]) as usize;
    let stream_start = magic_pos + 12;
    let stream_end = stream_start
        .checked_add(compressed_len)
        .ok_or_else(|| format!("SQUZ {name}: compressed length overflow"))?;
    if stream_end > data.len() {
        return Err(format!(
            "SQUZ {name}: compressed stream truncated ({} > {})",
            stream_end,
            data.len()
        ));
    }

    let expanded = match method {
        [0x00, 0x00] => {
            if compressed_len != uncompressed_len {
                return Err(format!(
                    "SQUZ {name}: uncompressed stream length mismatch ({compressed_len} != {uncompressed_len})"
                ));
            }
            data[stream_start..stream_end].to_vec()
        }
        [0x03, 0x03] => {
            decode_broderbund_squz_0303_stream(&data[stream_start..stream_end], uncompressed_len)
                .map_err(|err| format!("SQUZ {name}: {err}"))?
        }
        [0x03, 0x04] => {
            decode_broderbund_squz_0304_stream(&data[stream_start..stream_end], uncompressed_len)
                .map_err(|err| format!("SQUZ {name}: {err}"))?
        }
        [0x03, 0x05] => {
            decode_broderbund_squz_0305_stream(&data[stream_start..stream_end], uncompressed_len)
                .map_err(|err| format!("SQUZ {name}: {err}"))?
        }
        _ => {
            if target_type == *b"APPL" {
                return Err(format!(
                    "SQUZ {name}: unsupported KG method {:02X}{:02X}",
                    method[0], method[1]
                ));
            }
            if resource_like_file {
                if crate::runner::trace_load_enabled() {
                    eprintln!(
                        "[LOAD] Mounting SQUZ \"{}\" as empty resource fork: unsupported KG method {:02X}{:02X}",
                        name, method[0], method[1]
                    );
                }
                return Ok(Some(PayloadFile {
                    name: name.to_string(),
                    data: Vec::new(),
                    rsrc: empty_resource_fork_bytes(),
                    file_type: target_type,
                    creator: target_creator,
                    finder_flags: target_finder_flags,
                }));
            }
            if crate::runner::trace_load_enabled() {
                eprintln!(
                    "[LOAD] Leaving SQUZ \"{}\" packed: unsupported KG method {:02X}{:02X}",
                    name, method[0], method[1]
                );
            }
            return Ok(None);
        }
    };
    let expanded_is_rsrc = ResourceFork::parse(&expanded).is_some();
    if target_type == *b"APPL" && !expanded_is_rsrc {
        return Err(format!(
            "SQUZ {name}: decoded application resource fork is invalid"
        ));
    }
    let rsrc = if expanded_is_rsrc {
        expanded.clone()
    } else if resource_like_file {
        if crate::runner::trace_load_enabled() {
            eprintln!(
                "[LOAD] Mounting SQUZ \"{}\" as empty resource fork: decoded payload is not a parseable resource fork",
                name
            );
        }
        empty_resource_fork_bytes()
    } else {
        Vec::new()
    };
    let data = if expanded_is_rsrc || resource_like_file {
        Vec::new()
    } else {
        expanded
    };

    if crate::runner::trace_load_enabled() {
        eprintln!(
            "[LOAD] Expanded SQUZ \"{}\" {} -> {} bytes",
            name, compressed_len, uncompressed_len
        );
    }

    Ok(Some(PayloadFile {
        name: name.to_string(),
        data,
        rsrc,
        file_type: target_type,
        creator: target_creator,
        finder_flags: target_finder_flags,
    }))
}

fn decode_broderbund_squz_0303_stream(
    stream: &[u8],
    expected_len: usize,
) -> Result<Vec<u8>, String> {
    const WINDOW_SIZE: usize = 8192;
    const LOOKAHEAD_SIZE: usize = 10;

    decode_broderbund_squz_lzss_stream(
        stream,
        expected_len,
        WINDOW_SIZE,
        LOOKAHEAD_SIZE,
        |first, second| {
            let copy_pos = (((first & 0x1F) as usize) << 8) | second as usize;
            let copy_len = ((first >> 5) as usize) + 3;
            (copy_pos, copy_len)
        },
    )
}

fn decode_broderbund_squz_0304_stream(
    stream: &[u8],
    expected_len: usize,
) -> Result<Vec<u8>, String> {
    const WINDOW_SIZE: usize = 4096;
    const LOOKAHEAD_SIZE: usize = 18;

    decode_broderbund_squz_lzss_stream(
        stream,
        expected_len,
        WINDOW_SIZE,
        LOOKAHEAD_SIZE,
        |first, second| {
            let copy_pos = (((first & 0x0F) as usize) << 8) | second as usize;
            let copy_len = ((first >> 4) as usize) + 3;
            (copy_pos, copy_len)
        },
    )
}

fn decode_broderbund_squz_0305_stream(
    stream: &[u8],
    expected_len: usize,
) -> Result<Vec<u8>, String> {
    const WINDOW_SIZE: usize = 2048;
    const LOOKAHEAD_SIZE: usize = 34;

    decode_broderbund_squz_lzss_stream(
        stream,
        expected_len,
        WINDOW_SIZE,
        LOOKAHEAD_SIZE,
        |first, second| {
            let copy_pos = (((first & 0x07) as usize) << 8) | second as usize;
            let copy_len = ((first >> 3) as usize) + 3;
            (copy_pos, copy_len)
        },
    )
}

fn decode_broderbund_squz_lzss_stream<F>(
    stream: &[u8],
    expected_len: usize,
    window_size: usize,
    lookahead_size: usize,
    decode_ref: F,
) -> Result<Vec<u8>, String>
where
    F: Fn(u8, u8) -> (usize, usize),
{
    let mut window = vec![0u8; window_size];
    let window_mask = window_size - 1;
    let mut write_pos = window_size - lookahead_size;
    let mut out = Vec::with_capacity(expected_len);
    let mut pos = 0usize;

    while pos < stream.len() && out.len() < expected_len {
        let flags = stream[pos];
        pos += 1;

        for bit in 0..8 {
            if out.len() >= expected_len {
                break;
            }

            if (flags & (1 << bit)) != 0 {
                let Some(&byte) = stream.get(pos) else {
                    return Err("literal truncated".to_string());
                };
                pos += 1;
                out.push(byte);
                window[write_pos] = byte;
                write_pos = (write_pos + 1) & window_mask;
            } else {
                if pos + 1 >= stream.len() {
                    return Err("back-reference truncated".to_string());
                }
                let first = stream[pos];
                let second = stream[pos + 1];
                pos += 2;

                let (copy_pos, copy_len) = decode_ref(first, second);
                for i in 0..copy_len {
                    if out.len() >= expected_len {
                        break;
                    }
                    let byte = window[(copy_pos + i) & window_mask];
                    out.push(byte);
                    window[write_pos] = byte;
                    write_pos = (write_pos + 1) & window_mask;
                }
            }
        }
    }

    if out.len() != expected_len {
        return Err(format!(
            "decoded {} bytes, expected {}",
            out.len(),
            expected_len
        ));
    }

    Ok(out)
}

fn empty_resource_fork_bytes() -> Vec<u8> {
    let data_offset = 16u32;
    let data_length = 0u32;
    let map_offset = 16u32;
    let map_length = 32u32;

    let mut bytes = vec![0u8; (map_offset + map_length) as usize];
    let mut header = [0u8; 16];
    header[0..4].copy_from_slice(&data_offset.to_be_bytes());
    header[4..8].copy_from_slice(&map_offset.to_be_bytes());
    header[8..12].copy_from_slice(&data_length.to_be_bytes());
    header[12..16].copy_from_slice(&map_length.to_be_bytes());
    bytes[0..16].copy_from_slice(&header);

    let map_start = map_offset as usize;
    bytes[map_start..map_start + 16].copy_from_slice(&header);
    bytes[map_start + 24..map_start + 26].copy_from_slice(&30u16.to_be_bytes());
    bytes[map_start + 26..map_start + 28].copy_from_slice(&32u16.to_be_bytes());
    bytes[map_start + 28..map_start + 30].copy_from_slice(&0xFFFFu16.to_be_bytes());

    bytes
}

fn payload_from_forks(
    name: &str,
    data: Vec<u8>,
    rsrc: Vec<u8>,
    file_type: [u8; 4],
    creator: [u8; 4],
    finder_flags: u16,
) -> Result<Payload, String> {
    if let Some(file) = expand_squz_payload_file(name, &data, file_type, creator, finder_flags)? {
        return Ok(Payload {
            dirs: Vec::new(),
            files: vec![file],
            skipped_disk_image_errors: Vec::new(),
        });
    }

    let mut skipped_disk_image_errors = Vec::new();
    match crate::disk_image::extract_dc42_or_hfs(&data) {
        Ok(Some(image)) => {
            if crate::runner::trace_load_enabled() {
                eprintln!(
                    "[LOAD] Extracting HFS disk image \"{}\" from data fork: volume \"{}\", {} files",
                    name,
                    image.volume_name,
                    image.files.len()
                );
            }
            return payload_from_disk_image(image);
        }
        Ok(None) => {}
        Err(err) => {
            let err = format!("Disk image {name} data fork: {err}");
            if crate::runner::trace_load_enabled() {
                eprintln!("[LOAD] Skipping nested disk image: {err}");
            }
            skipped_disk_image_errors.push(err);
        }
    }

    match crate::disk_image::extract_dc42_or_hfs(&rsrc) {
        Ok(Some(image)) => {
            if crate::runner::trace_load_enabled() {
                eprintln!(
                    "[LOAD] Extracting HFS disk image \"{}\" from resource fork: volume \"{}\", {} files",
                    name,
                    image.volume_name,
                    image.files.len()
                );
            }
            return payload_from_disk_image(image);
        }
        Ok(None) => {}
        Err(err) => {
            let err = format!("Disk image {name} resource fork: {err}");
            if crate::runner::trace_load_enabled() {
                eprintln!("[LOAD] Skipping nested disk image: {err}");
            }
            skipped_disk_image_errors.push(err);
        }
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
        skipped_disk_image_errors,
    })
}

fn payload_from_disk_image(image: crate::disk_image::DiskImageContents) -> Result<Payload, String> {
    let mut files = Vec::new();
    for file in image.files {
        if let Some(expanded) = expand_squz_payload_file(
            &file.path,
            &file.data,
            file.file_type,
            file.creator,
            file.finder_flags,
        )? {
            files.push(expanded);
        } else {
            files.push(PayloadFile {
                name: file.path,
                data: file.data,
                rsrc: file.rsrc,
                file_type: file.file_type,
                creator: file.creator,
                finder_flags: file.finder_flags,
            });
        }
    }
    Ok(Payload {
        dirs: image.dirs,
        files,
        skipped_disk_image_errors: Vec::new(),
    })
}

fn insert_payload_into_vfs(
    runner: &mut FixtureRunner,
    payload: Payload,
    executable_entry: &mut Option<ExecutableCandidate>,
) {
    for dir in payload.dirs {
        let normalized = crate::trap::dispatch::TrapDispatcher::normalize_vfs_path(&dir);
        runner.dispatcher_mut().ensure_vfs_directory(&normalized);
    }

    for file in payload.files {
        let data_len = file.data.len();
        let is_appl = file.file_type == *b"APPL";
        maybe_select_executable(
            executable_entry,
            &file.name,
            &file.rsrc,
            is_appl,
            data_len,
            file.creator,
        );
        insert_forks_into_vfs(
            runner,
            &file.name,
            file.data,
            file.rsrc,
            file.file_type,
            file.creator,
            file.finder_flags,
        );
    }
}

fn load_selected_executable(
    runner: &mut FixtureRunner,
    executable: &ExecutableCandidate,
) -> Result<LoadedApp, String> {
    runner
        .dispatcher_mut()
        .set_launched_app_path(&executable.name);

    let rsrc = runner
        .dispatcher()
        .vfs_rsrc
        .get(&executable.vfs_key)
        .ok_or_else(|| {
            format!(
                "Selected executable resource fork missing: {}",
                executable.name
            )
        })?;
    let fork = ResourceFork::parse(rsrc).ok_or("Failed to parse resource fork")?;
    let app = runner
        .load_app(&fork)
        .ok_or_else(|| "Failed to load app".to_string())?;
    merge_launch_resource_companions(runner, executable)?;
    Ok(app)
}

fn merge_launch_resource_companions(
    runner: &mut FixtureRunner,
    executable: &ExecutableCandidate,
) -> Result<(), String> {
    let companion_keys = launch_resource_companion_keys(runner.dispatcher(), executable);
    for key in companion_keys {
        let rsrc = runner
            .dispatcher()
            .vfs_rsrc
            .get(&key)
            .cloned()
            .unwrap_or_default();
        let fork = ResourceFork::parse(&rsrc)
            .ok_or_else(|| format!("Failed to parse launch resource companion {key}"))?;
        let count = runner.merge_resources_into_application(&fork);
        if crate::runner::trace_load_enabled() {
            eprintln!(
                "[LOAD] Merged launch resource companion \"{}\" into application resource map ({} resources)",
                key, count
            );
        }
    }
    Ok(())
}

fn launch_resource_companion_keys(
    dispatcher: &crate::trap::TrapDispatcher,
    executable: &ExecutableCandidate,
) -> Vec<String> {
    let executable_path =
        crate::trap::dispatch::TrapDispatcher::normalize_vfs_path(&executable.name);
    let executable_dir = crate::trap::dispatch::TrapDispatcher::vfs_parent_path(&executable_path);
    let executable_base = executable_path
        .rsplit('/')
        .next()
        .unwrap_or(executable_path.as_str());
    let executable_base_lower = executable_base.to_ascii_lowercase();

    let mut keys: Vec<String> = dispatcher
        .vfs_rsrc
        .keys()
        .filter_map(|key| {
            let normalized = crate::trap::dispatch::TrapDispatcher::normalize_vfs_path(key);
            let dir = crate::trap::dispatch::TrapDispatcher::vfs_parent_path(&normalized);
            if dir != executable_dir {
                return None;
            }

            let base = normalized.rsplit('/').next().unwrap_or(normalized.as_str());
            let base_lower = base.to_ascii_lowercase();
            let companion_stem = base_lower.strip_suffix(" (r)")?;
            if companion_stem != executable_base_lower {
                return None;
            }

            if dispatcher
                .vfs
                .get(&normalized)
                .is_some_and(|data| !data.is_empty())
            {
                return None;
            }

            let rsrc = dispatcher.vfs_rsrc.get(key)?;
            if ResourceFork::parse(rsrc).is_none() {
                return None;
            }

            let creator = dispatcher
                .vfs_metadata
                .get(&normalized)
                .map(|metadata| metadata.creator.to_be_bytes())
                .unwrap_or(*b"????");
            if !creator_matches(executable.creator, creator) {
                return None;
            }

            Some(key.clone())
        })
        .collect();
    keys.sort_unstable();
    keys
}

fn creator_matches(executable: [u8; 4], companion: [u8; 4]) -> bool {
    executable == companion || executable == *b"????" || companion == *b"????"
}

fn maybe_select_executable(
    executable_entry: &mut Option<ExecutableCandidate>,
    name: &str,
    rsrc: &[u8],
    is_appl: bool,
    data_len: usize,
    creator: [u8; 4],
) {
    if rsrc.is_empty() {
        return;
    }

    if !ResourceFork::contains_code(rsrc, 0) {
        return;
    }

    // SYSTEMLESS_LOAD_EXECUTABLE: case-sensitive substring match against the
    // archive entry name. When the env var is set and a candidate matches
    // it wins outright over the size/APPL heuristic, which is needed for
    // archives that contain multiple bootable executables and where size
    // alone cannot distinguish the user-facing runtime from tooling.
    let override_match = executable_name_override()
        .map(|needle| name.contains(needle.as_str()))
        .unwrap_or(false);
    let prev_override_match = match (executable_entry.as_ref(), executable_name_override()) {
        (Some(prev), Some(needle)) => prev.name.contains(needle.as_str()),
        _ => false,
    };

    let candidate = ExecutableCandidate {
        name: name.to_string(),
        vfs_key: crate::trap::dispatch::TrapDispatcher::normalize_vfs_path(name),
        is_appl,
        has_data_fork: data_len > 0,
        score: data_len.max(rsrc.len()),
        creator,
    };

    let take = if override_match && !prev_override_match {
        true
    } else if !override_match && prev_override_match {
        false
    } else {
        match executable_entry.as_ref() {
            Some(prev) => candidate.selection_key() > prev.selection_key(),
            None => true,
        }
    };

    if take {
        *executable_entry = Some(candidate);
    }
}

fn executable_name_override() -> Option<String> {
    std::env::var("SYSTEMLESS_LOAD_EXECUTABLE")
        .ok()
        .filter(|s| !s.is_empty())
}

#[derive(Clone, Debug)]
struct ExecutableCandidate {
    name: String,
    vfs_key: String,
    is_appl: bool,
    has_data_fork: bool,
    score: usize,
    creator: [u8; 4],
}

impl ExecutableCandidate {
    fn selection_key(&self) -> (bool, bool, bool, usize) {
        (
            self.is_appl,
            !is_system_folder_path(&self.name),
            self.has_data_fork,
            self.score,
        )
    }
}

fn is_system_folder_path(path: &str) -> bool {
    path.split(['/', ':'])
        .any(|component| component.eq_ignore_ascii_case("System Folder"))
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn make_zip(entries: &[(&str, &[u8])]) -> Vec<u8> {
        let mut out = Cursor::new(Vec::new());
        {
            let mut zip = zip::ZipWriter::new(&mut out);
            let options = zip::write::SimpleFileOptions::default()
                .compression_method(zip::CompressionMethod::Deflated);
            for (name, bytes) in entries {
                zip.start_file(*name, options).unwrap();
                zip.write_all(bytes).unwrap();
            }
            zip.finish().unwrap();
        }
        out.into_inner()
    }

    #[test]
    fn zip_archive_mounts_macbinary_application_and_companion_files() {
        let rsrc = make_single_resource_fork_bytes(*b"CODE", 0, &[0; 128]);
        let macbinary = make_macbinary_application("Demo", b"application data", &rsrc);
        let archive = make_zip(&[
            ("Game/Demo.bin", &macbinary),
            ("Game/Data/Level 1", b"level data"),
        ]);
        let mut runner = new_runner();

        load_game(&mut runner, &archive).expect("ZIP application should load");

        assert_eq!(
            runner.dispatcher().launched_app_path.as_deref(),
            Some("Game/Demo")
        );
        assert_eq!(
            runner.dispatcher().vfs.get("Game/Data/Level 1"),
            Some(&b"level data".to_vec())
        );
        assert_eq!(
            runner.dispatcher().vfs.get("Game/Demo"),
            Some(&b"application data".to_vec())
        );
        let metadata = runner.dispatcher().vfs_metadata.get("Game/Demo").unwrap();
        assert_eq!(metadata.file_type.to_be_bytes(), *b"APPL");
    }

    #[test]
    fn zip_archive_rejects_parent_traversal_entry() {
        let archive = make_zip(&[("safe", b"mounted first"), ("../escape", b"outside")]);
        let mut runner = new_runner();

        let error = match load_game(&mut runner, &archive) {
            Ok(_) => panic!("unsafe ZIP should be rejected"),
            Err(error) => error,
        };

        assert!(error.contains("Unsafe ZIP entry path"), "{error}");
        assert!(runner.dispatcher().vfs.is_empty());
    }

    #[test]
    fn web_pack_sources_accept_hfs_images_and_filter_by_classic_mac_path() {
        let mut builder = hfsplus::testutil::HfsPlusImageBuilder::new();
        builder.add_file("keep.dat", b"runtime data", 0o100644);
        builder.add_file("drop.dat", b"unrelated demo", 0o100644);
        let image = builder.build();

        let packed = pack_game_sources_for_web(&[&image], &["HFS+ Disk Image:keep.dat"])
            .expect("HFS source should pack");

        assert_eq!(&packed[0..4], WEB_PACK_MAGIC);
        assert_eq!(u32::from_be_bytes(packed[4..8].try_into().unwrap()), 1);
        let mut offset = 8;
        let name_len = read_u16_be(&packed, &mut offset).unwrap() as usize;
        let name = read_exact(&packed, &mut offset, name_len).unwrap();
        assert_eq!(name, b"HFS+ Disk Image/keep.dat");
    }

    #[test]
    fn web_pack_sources_merge_files_from_multiple_images() {
        let mut application_builder = hfsplus::testutil::HfsPlusImageBuilder::new();
        application_builder.add_file("Application", b"application fork", 0o100644);
        let application_image = application_builder.build();
        let mut data_builder = hfsplus::testutil::HfsPlusImageBuilder::new();
        data_builder.add_file("Level001", b"level data", 0o100644);
        let data_image = data_builder.build();

        let packed = pack_game_sources_for_web(&[&application_image, &data_image], &[])
            .expect("multiple HFS sources should merge");

        assert_eq!(&packed[0..4], WEB_PACK_MAGIC);
        assert_eq!(u32::from_be_bytes(packed[4..8].try_into().unwrap()), 2);
        assert!(packed
            .windows(b"HFS+ Disk Image/Application".len())
            .any(|window| window == b"HFS+ Disk Image/Application"));
        assert!(packed
            .windows(b"HFS+ Disk Image/Level001".len())
            .any(|window| window == b"HFS+ Disk Image/Level001"));
    }

    fn make_single_resource_fork_bytes(res_type: [u8; 4], res_id: i16, data: &[u8]) -> Vec<u8> {
        let data_offset = 16u32;
        let data_length = (4 + data.len()) as u32;
        let map_offset = data_offset + data_length;
        let type_list_offset = 30u16;
        let ref_list_offset = 10u16;
        let name_list_offset = 40u16;
        let map_length = 52u32;

        let mut bytes = vec![0u8; (map_offset + map_length) as usize];
        let mut header = [0u8; 16];
        header[0..4].copy_from_slice(&data_offset.to_be_bytes());
        header[4..8].copy_from_slice(&map_offset.to_be_bytes());
        header[8..12].copy_from_slice(&data_length.to_be_bytes());
        header[12..16].copy_from_slice(&map_length.to_be_bytes());
        bytes[0..16].copy_from_slice(&header);

        let data_start = data_offset as usize;
        bytes[data_start..data_start + 4].copy_from_slice(&(data.len() as u32).to_be_bytes());
        bytes[data_start + 4..data_start + 4 + data.len()].copy_from_slice(data);

        let map_start = map_offset as usize;
        bytes[map_start..map_start + 16].copy_from_slice(&header);
        bytes[map_start + 24..map_start + 26].copy_from_slice(&type_list_offset.to_be_bytes());
        bytes[map_start + 26..map_start + 28].copy_from_slice(&name_list_offset.to_be_bytes());

        let type_list_start = map_start + type_list_offset as usize;
        bytes[type_list_start..type_list_start + 2].copy_from_slice(&0u16.to_be_bytes());
        bytes[type_list_start + 2..type_list_start + 6].copy_from_slice(&res_type);
        bytes[type_list_start + 6..type_list_start + 8].copy_from_slice(&0u16.to_be_bytes());
        bytes[type_list_start + 8..type_list_start + 10]
            .copy_from_slice(&ref_list_offset.to_be_bytes());

        let ref_list_start = map_start + type_list_offset as usize + ref_list_offset as usize;
        bytes[ref_list_start..ref_list_start + 2].copy_from_slice(&(res_id as u16).to_be_bytes());
        bytes[ref_list_start + 2..ref_list_start + 4].copy_from_slice(&0xFFFFu16.to_be_bytes());
        bytes[ref_list_start + 5..ref_list_start + 8].copy_from_slice(&0u32.to_be_bytes()[1..4]);

        bytes
    }

    fn make_macbinary_application(name: &str, data: &[u8], rsrc: &[u8]) -> Vec<u8> {
        assert!(name.len() <= 63);
        let data_padded_len = (data.len() + 127) & !127;
        let rsrc_padded_len = (rsrc.len() + 127) & !127;
        let mut bytes = vec![0; 128 + data_padded_len + rsrc_padded_len];
        bytes[1] = name.len() as u8;
        bytes[2..2 + name.len()].copy_from_slice(name.as_bytes());
        bytes[65..69].copy_from_slice(b"APPL");
        bytes[69..73].copy_from_slice(b"TEST");
        bytes[83..87].copy_from_slice(&(data.len() as u32).to_be_bytes());
        bytes[87..91].copy_from_slice(&(rsrc.len() as u32).to_be_bytes());
        bytes[128..128 + data.len()].copy_from_slice(data);
        let rsrc_start = 128 + data_padded_len;
        bytes[rsrc_start..rsrc_start + rsrc.len()].copy_from_slice(rsrc);
        bytes
    }

    struct TestWebPackEntry<'a> {
        name: &'a str,
        file_type: [u8; 4],
        creator: [u8; 4],
        finder_flags: u16,
        data: &'a [u8],
        rsrc: &'a [u8],
    }

    fn make_web_pack(entries: &[TestWebPackEntry<'_>]) -> Vec<u8> {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(WEB_PACK_MAGIC);
        bytes.extend_from_slice(&(entries.len() as u32).to_be_bytes());
        for entry in entries {
            bytes.extend_from_slice(&(entry.name.len() as u16).to_be_bytes());
            bytes.extend_from_slice(entry.name.as_bytes());
            bytes.extend_from_slice(&entry.file_type);
            bytes.extend_from_slice(&entry.creator);
            bytes.extend_from_slice(&entry.finder_flags.to_be_bytes());
            bytes.extend_from_slice(&(entry.data.len() as u32).to_be_bytes());
            bytes.extend_from_slice(entry.data);
            bytes.extend_from_slice(&(entry.rsrc.len() as u32).to_be_bytes());
            bytes.extend_from_slice(entry.rsrc);
        }
        bytes
    }

    fn make_legacy_web_pack(entry: &TestWebPackEntry<'_>) -> Vec<u8> {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(LEGACY_WEB_PACK_MAGIC);
        bytes.extend_from_slice(&1u32.to_be_bytes());
        bytes.extend_from_slice(&(entry.name.len() as u16).to_be_bytes());
        bytes.extend_from_slice(entry.name.as_bytes());
        bytes.extend_from_slice(&entry.file_type);
        bytes.extend_from_slice(&(entry.data.len() as u32).to_be_bytes());
        bytes.extend_from_slice(entry.data);
        bytes.extend_from_slice(&(entry.rsrc.len() as u32).to_be_bytes());
        bytes.extend_from_slice(entry.rsrc);
        bytes
    }

    fn minimal_raw_filesystem_image(signature: u16) -> Vec<u8> {
        let mut bytes = vec![0; 2048];
        bytes[1024..1026].copy_from_slice(&signature.to_be_bytes());
        bytes
    }

    #[test]
    fn web_pack_loader_is_opt_in_for_kpk_payloads() {
        let mut runner = new_runner();

        assert!(WebPackLoader::new(&mut runner, b"not a web pack")
            .unwrap()
            .is_none());
    }

    #[test]
    fn web_pack_loader_keeps_legacy_kpk1_compatibility() {
        let entry = TestWebPackEntry {
            name: "Folder/Legacy",
            file_type: *b"TEXT",
            creator: *b"ttxt",
            finder_flags: 0x4000,
            data: b"legacy",
            rsrc: &[],
        };
        let pack = make_legacy_web_pack(&entry);
        let mut runner = new_runner();
        let mut loader = WebPackLoader::new(&mut runner, &pack).unwrap().unwrap();

        while !loader.load_next_chunk(&mut runner, 2).unwrap() {}

        assert_eq!(
            runner.dispatcher().vfs.get("Folder/Legacy"),
            Some(&b"legacy".to_vec())
        );
        let metadata = runner
            .dispatcher_mut()
            .vfs_file_metadata("Folder/Legacy")
            .expect("legacy metadata");
        assert_eq!(metadata.file_type, u32::from_be_bytes(*b"TEXT"));
        assert_eq!(metadata.creator, u32::from_be_bytes(*b"????"));
        assert_eq!(metadata.finder_flags, 0);
    }

    #[test]
    fn web_pack_loader_mounts_forks_incrementally() {
        let data = b"abcdefghijkl".to_vec();
        let rsrc = make_single_resource_fork_bytes(*b"DLOG", 4000, b"dialog");
        let pack = make_web_pack(&[
            TestWebPackEntry {
                name: "Folder/Data",
                file_type: *b"TEXT",
                creator: *b"ttxt",
                finder_flags: 0x4000,
                data: &data,
                rsrc: &[],
            },
            TestWebPackEntry {
                name: "Folder/Sidecar.rsrc",
                file_type: *b"rsrc",
                creator: *b"RSED",
                finder_flags: 0,
                data: &rsrc,
                rsrc: &[],
            },
        ]);
        let mut runner = new_runner();
        let mut loader = WebPackLoader::new(&mut runner, &pack).unwrap().unwrap();

        assert_eq!(loader.total_entries(), 2);
        assert_eq!(loader.loaded_entries(), 0);
        assert!(loader.archive_bytes_total() > data.len() + rsrc.len());

        assert!(!loader.load_next_chunk(&mut runner, 4).unwrap());
        assert_eq!(loader.loaded_entries(), 0);
        assert!(runner.dispatcher().vfs.is_empty());
        assert!(loader.archive_bytes_loaded() > WEB_PACK_MAGIC.len());

        let mut calls = 1;
        while !loader.load_next_chunk(&mut runner, 4).unwrap() {
            calls += 1;
            assert!(calls < 64, "incremental web-pack load did not finish");
        }

        assert_eq!(loader.loaded_entries(), 2);
        assert_eq!(runner.dispatcher().vfs.get("Folder/Data"), Some(&data));
        assert_eq!(
            runner.dispatcher().vfs.get("Folder/Sidecar.rsrc"),
            Some(&rsrc)
        );
        assert_eq!(
            runner.dispatcher().vfs_rsrc.get("Folder/Sidecar.rsrc"),
            Some(&rsrc)
        );
        let data_metadata = runner
            .dispatcher_mut()
            .vfs_file_metadata("Folder/Data")
            .expect("packed data metadata");
        assert_eq!(data_metadata.file_type, u32::from_be_bytes(*b"TEXT"));
        assert_eq!(data_metadata.creator, u32::from_be_bytes(*b"ttxt"));
        assert_eq!(data_metadata.finder_flags, 0x4000);
        match loader.finish(&mut runner) {
            Ok(_) => panic!("non-executable web pack should not finish as a loaded app"),
            Err(err) => assert!(err.contains("No executable found in web pack")),
        }
    }

    #[test]
    fn web_pack_loader_skips_relative_remove_paths_after_executable_parent_known() {
        let app_data = b"app-data".to_vec();
        let app_rsrc = make_single_resource_fork_bytes(*b"CODE", 0, b"code");
        let skipped_rsrc = vec![7; 64];
        let kept_data = b"keep".to_vec();
        let pack = make_web_pack(&[
            TestWebPackEntry {
                name: "Game/Game App",
                file_type: *b"APPL",
                creator: *b"TEST",
                finder_flags: 0,
                data: &app_data,
                rsrc: &app_rsrc,
            },
            TestWebPackEntry {
                name: "Game/Plug-Ins/MAGMA",
                file_type: *b"DATA",
                creator: *b"TEST",
                finder_flags: 0,
                data: &[],
                rsrc: &skipped_rsrc,
            },
            TestWebPackEntry {
                name: "Game/Plug-Ins/Keep",
                file_type: *b"DATA",
                creator: *b"TEST",
                finder_flags: 0,
                data: &kept_data,
                rsrc: &[],
            },
        ]);
        let mut runner = new_runner();
        let mut loader =
            WebPackLoader::new_with_remove_paths(&mut runner, &pack, &["Plug-Ins/MAGMA"])
                .unwrap()
                .unwrap();

        while !loader.load_next_chunk(&mut runner, 4).unwrap() {}

        assert_eq!(loader.loaded_entries(), 3);
        assert_eq!(
            runner.dispatcher().vfs.get("Game/Game App"),
            Some(&app_data)
        );
        assert_eq!(
            runner.dispatcher().vfs_rsrc.get("Game/Game App"),
            Some(&app_rsrc)
        );
        assert!(!runner.dispatcher().vfs.contains_key("Game/Plug-Ins/MAGMA"));
        assert!(!runner
            .dispatcher()
            .vfs_rsrc
            .contains_key("Game/Plug-Ins/MAGMA"));
        assert_eq!(
            runner.dispatcher().vfs.get("Game/Plug-Ins/Keep"),
            Some(&kept_data)
        );
    }

    #[test]
    fn unsupported_nested_disk_image_is_preserved_as_payload_file() {
        let image = minimal_raw_filesystem_image(0x482B);
        let payload = payload_from_forks(
            "Extras/Unsupported.img",
            image.clone(),
            Vec::new(),
            *b"dImg",
            *b"ddsk",
            0,
        )
        .expect("unsupported nested image should not abort archive payload loading");

        assert!(payload.dirs.is_empty());
        assert_eq!(payload.files.len(), 1);
        assert_eq!(payload.files[0].name, "Extras/Unsupported.img");
        assert_eq!(payload.files[0].data, image);
        assert_eq!(payload.files[0].file_type, *b"dImg");
        assert_eq!(payload.files[0].creator, *b"ddsk");
        assert_eq!(payload.skipped_disk_image_errors.len(), 1);
        assert!(
            payload.skipped_disk_image_errors[0].contains("HFS+"),
            "error should preserve the unsupported filesystem detail"
        );
    }

    #[test]
    fn no_executable_archive_error_mentions_skipped_nested_disk_image() {
        let errors =
            vec!["Disk image Extras/Unsupported.img data fork: Image is HFS+, not HFS".to_string()];

        assert_eq!(
            no_executable_archive_error(&errors),
            "No executable found in archive; skipped nested disk image: Disk image Extras/Unsupported.img data fork: Image is HFS+, not HFS"
        );
    }

    #[test]
    fn web_pack_loader_caps_initial_large_fork_reserve() {
        assert_eq!(initial_web_pack_fork_capacity(128), 128);
        assert_eq!(
            initial_web_pack_fork_capacity(WEB_PACK_INITIAL_FORK_RESERVE_BYTES + 1),
            WEB_PACK_INITIAL_FORK_RESERVE_BYTES
        );
    }

    #[test]
    fn executable_selection_prefers_real_data_fork_app_over_larger_manual() {
        let manual_rsrc = make_single_resource_fork_bytes(*b"CODE", 0, &[0; 1024]);
        let app_rsrc = make_single_resource_fork_bytes(*b"CODE", 0, &[0; 128]);
        let mut selected = None;

        maybe_select_executable(
            &mut selected,
            "Sample App/Sample Manual",
            &manual_rsrc,
            true,
            0,
            *b"????",
        );
        maybe_select_executable(
            &mut selected,
            "Sample App/Sample Runtime",
            &app_rsrc,
            true,
            322_352,
            *b"????",
        );

        let selected = selected.expect("expected an executable candidate");
        assert_eq!(selected.name, "Sample App/Sample Runtime");
    }

    #[test]
    fn executable_selection_prefers_user_app_over_system_folder_utility() {
        let utility_rsrc = make_single_resource_fork_bytes(*b"CODE", 0, &[0; 256]);
        let game_rsrc = make_single_resource_fork_bytes(*b"CODE", 0, &[0; 128]);
        let mut selected = None;

        maybe_select_executable(
            &mut selected,
            "Demo Disk/System Folder/Apple Menu Items/Stickies",
            &utility_rsrc,
            true,
            38,
            *b"notz",
        );
        maybe_select_executable(
            &mut selected,
            "Demo Disk/Pathways into Darkness",
            &game_rsrc,
            true,
            0,
            *b"p.th",
        );

        let selected = selected.expect("expected an executable candidate");
        assert_eq!(selected.name, "Demo Disk/Pathways into Darkness");
    }

    #[test]
    fn macbinary_application_mounts_its_forks_under_the_decoded_filename() {
        let data = b"self-readable data fork";
        let rsrc = make_single_resource_fork_bytes(*b"CODE", 0, &[0; 128]);
        let macbinary = make_macbinary_application("Self Opening App", data, &rsrc);
        let mut runner = new_runner();

        load_macbinary(&mut runner, &macbinary).expect("MacBinary application should load");

        assert_eq!(
            runner.dispatcher().vfs.get("Self Opening App"),
            Some(&data.to_vec())
        );
        assert_eq!(
            runner.dispatcher().vfs_rsrc.get("Self Opening App"),
            Some(&rsrc)
        );
    }

    #[test]
    fn launch_resource_companion_matches_same_folder_suffix_and_creator() {
        let app_rsrc = make_single_resource_fork_bytes(*b"CODE", 0, &[0; 128]);
        let companion_rsrc = make_single_resource_fork_bytes(*b"DLOG", 4000, b"dialog");
        let other_rsrc = make_single_resource_fork_bytes(*b"STR ", 128, b"string");
        let mut runner = new_runner();

        insert_forks_into_vfs(
            &mut runner,
            "Folder/Runtime",
            vec![1, 2, 3],
            app_rsrc.clone(),
            *b"APPL",
            *b"ABCD",
            0,
        );
        insert_forks_into_vfs(
            &mut runner,
            "Folder/runtime (r)",
            Vec::new(),
            companion_rsrc,
            *b"HeHe",
            *b"ABCD",
            0,
        );
        insert_forks_into_vfs(
            &mut runner,
            "Folder/runtime (i)",
            Vec::new(),
            other_rsrc.clone(),
            *b"pref",
            *b"ABCD",
            0,
        );
        insert_forks_into_vfs(
            &mut runner,
            "Other/Runtime (r)",
            Vec::new(),
            other_rsrc.clone(),
            *b"HeHe",
            *b"ABCD",
            0,
        );
        insert_forks_into_vfs(
            &mut runner,
            "Folder/Runtime Mismatch (r)",
            Vec::new(),
            other_rsrc,
            *b"HeHe",
            *b"WXYZ",
            0,
        );

        let executable = ExecutableCandidate {
            name: "Folder/Runtime".to_string(),
            vfs_key: "Folder/Runtime".to_string(),
            is_appl: true,
            has_data_fork: true,
            score: 128,
            creator: *b"ABCD",
        };

        assert_eq!(
            launch_resource_companion_keys(runner.dispatcher(), &executable),
            vec!["Folder/runtime (r)".to_string()]
        );
    }

    #[test]
    fn launch_resource_companion_requires_empty_data_fork() {
        let companion_rsrc = make_single_resource_fork_bytes(*b"DLOG", 4000, b"dialog");
        let mut runner = new_runner();

        insert_forks_into_vfs(
            &mut runner,
            "Folder/Runtime (r)",
            vec![1],
            companion_rsrc,
            *b"HeHe",
            *b"ABCD",
            0,
        );

        let executable = ExecutableCandidate {
            name: "Folder/Runtime".to_string(),
            vfs_key: "Folder/Runtime".to_string(),
            is_appl: true,
            has_data_fork: true,
            score: 128,
            creator: *b"ABCD",
        };

        assert!(launch_resource_companion_keys(runner.dispatcher(), &executable).is_empty());
    }

    #[test]
    fn launch_resource_companion_rejects_exact_name_creator_mismatch() {
        let companion_rsrc = make_single_resource_fork_bytes(*b"DLOG", 4000, b"dialog");
        let mut runner = new_runner();

        insert_forks_into_vfs(
            &mut runner,
            "Folder/Runtime (r)",
            Vec::new(),
            companion_rsrc,
            *b"HeHe",
            *b"WXYZ",
            0,
        );

        let executable = ExecutableCandidate {
            name: "Folder/Runtime".to_string(),
            vfs_key: "Folder/Runtime".to_string(),
            is_appl: true,
            has_data_fork: true,
            score: 128,
            creator: *b"ABCD",
        };

        assert!(launch_resource_companion_keys(runner.dispatcher(), &executable).is_empty());
    }

    #[test]
    fn data_backed_rsrc_sidecar_is_mounted_as_resource_fork() {
        let sidecar = make_single_resource_fork_bytes(*b"DLOG", 4000, b"dialog");
        let mut runner = new_runner();

        insert_forks_into_vfs(
            &mut runner,
            "Folder/Runtime.rsrc",
            sidecar.clone(),
            Vec::new(),
            *b"rsrc",
            *b"ABCD",
            0,
        );

        assert_eq!(
            runner.dispatcher().vfs.get("Folder/Runtime.rsrc"),
            Some(&sidecar)
        );
        assert_eq!(
            runner.dispatcher().vfs_rsrc.get("Folder/Runtime.rsrc"),
            Some(&sidecar)
        );
    }

    #[test]
    fn swapped_non_resource_fork_bytes_remain_available_as_data() {
        let swapped = b"not a resource fork".to_vec();
        let mut runner = new_runner();

        insert_forks_into_vfs(
            &mut runner,
            "Folder/Read Me",
            Vec::new(),
            swapped.clone(),
            *b"TEXT",
            *b"ttxt",
            0,
        );

        assert_eq!(
            runner.dispatcher().vfs.get("Folder/Read Me"),
            Some(&swapped)
        );
        assert_eq!(
            runner.dispatcher().vfs_rsrc.get("Folder/Read Me"),
            Some(&swapped)
        );
    }

    #[test]
    fn broderbund_squz_0304_decodes_literals_and_backrefs() {
        let stream = [
            0xFF, b'A', b'B', b'C', b'D', b'E', b'F', b'G', b'H', 0x00, 0xFF, 0xEE,
        ];

        let decoded = decode_broderbund_squz_0304_stream(&stream, 26).unwrap();
        assert_eq!(decoded, b"ABCDEFGHABCDEFGHABCDEFGHAB");
    }

    #[test]
    fn broderbund_squz_0305_uses_2k_window_and_5_bit_lengths() {
        let stream = [
            0xFF, b'A', b'B', b'C', b'D', b'E', b'F', b'G', b'H', 0x00, 0x2F, 0xDE, 0x2F, 0xDE,
            0x2F, 0xDE,
        ];

        let decoded = decode_broderbund_squz_0305_stream(&stream, 26).unwrap();
        assert_eq!(decoded, b"ABCDEFGHABCDEFGHABCDEFGHAB");
    }

    #[test]
    fn broderbund_squz_0303_uses_8k_window_and_13_bit_offsets() {
        let stream = [
            0x4E, 0x1F, 0xF5, 0x02, 0x00, 0x01, 0x1F, 0xFA, 0x7F, 0xF3, 0x01,
        ];

        let decoded = decode_broderbund_squz_0303_stream(&stream, 16).unwrap();
        assert_eq!(decoded, [0, 0, 0, 2, 0, 1, 0, 1, 0, 0, 0, 0, 0, 0, 0, 1]);
    }

    #[test]
    fn broderbund_squz_uncompressed_payload_becomes_resource_fork() {
        let rsrc = make_single_resource_fork_bytes(*b"TEST", 128, b"payload");
        let mut file = Vec::new();
        file.extend_from_slice(&(rsrc.len() as u32).to_be_bytes());
        file.extend_from_slice(b"PLR1");
        file.extend_from_slice(b"PLRM");
        file.extend_from_slice(&0x0500u16.to_be_bytes());
        file.extend_from_slice(&[0; 42]);
        file.extend_from_slice(b"KG\0\0");
        file.extend_from_slice(&(rsrc.len() as u32).to_be_bytes());
        file.extend_from_slice(&(rsrc.len() as u32).to_be_bytes());
        file.extend_from_slice(&rsrc);

        let expanded = expand_squz_payload_file("AllSounds1.rsrc", &file, *b"SQUZ", *b"BrSq", 0)
            .unwrap()
            .unwrap();

        assert!(expanded.data.is_empty());
        assert_eq!(expanded.rsrc, rsrc);
        assert_eq!(expanded.file_type, *b"PLR1");
        assert_eq!(expanded.creator, *b"PLRM");
        assert_eq!(expanded.finder_flags, 0x0500);
        assert!(ResourceFork::parse(&expanded.rsrc).is_some());
    }

    #[test]
    fn broderbund_squz_0305_resource_payload_becomes_resource_fork() {
        let rsrc = make_single_resource_fork_bytes(*b"TEST", 128, b"payload");
        let mut stream = Vec::new();
        for chunk in rsrc.chunks(8) {
            stream.push(((1u16 << chunk.len()) - 1) as u8);
            stream.extend_from_slice(chunk);
        }

        let mut file = Vec::new();
        file.extend_from_slice(&(rsrc.len() as u32).to_be_bytes());
        file.extend_from_slice(b"PLR2");
        file.extend_from_slice(b"PLRM");
        file.extend_from_slice(&0x0500u16.to_be_bytes());
        file.extend_from_slice(&[0; 42]);
        file.extend_from_slice(b"KG\x03\x05");
        file.extend_from_slice(&(rsrc.len() as u32).to_be_bytes());
        file.extend_from_slice(&(stream.len() as u32).to_be_bytes());
        file.extend_from_slice(&stream);

        let expanded = expand_squz_payload_file("Activity.rsrc", &file, *b"SQUZ", *b"BrSq", 0)
            .unwrap()
            .unwrap();

        assert!(expanded.data.is_empty());
        assert_eq!(expanded.rsrc, rsrc);
        assert_eq!(expanded.file_type, *b"PLR2");
        assert_eq!(expanded.creator, *b"PLRM");
        assert_eq!(expanded.finder_flags, 0x0500);
        assert!(ResourceFork::parse(&expanded.rsrc).is_some());
    }

    #[test]
    fn broderbund_squz_compressed_non_resource_payload_stays_data_fork() {
        let stream = [0xFF, b'H', b'e', b'l', b'l', b'o'];
        let mut file = Vec::new();
        file.extend_from_slice(&5u32.to_be_bytes());
        file.extend_from_slice(b"TEXT");
        file.extend_from_slice(b"PLRM");
        file.extend_from_slice(&0x0100u16.to_be_bytes());
        file.extend_from_slice(&[0; 42]);
        file.extend_from_slice(b"KG\x03\x03");
        file.extend_from_slice(&5u32.to_be_bytes());
        file.extend_from_slice(&(stream.len() as u32).to_be_bytes());
        file.extend_from_slice(&stream);

        let expanded = expand_squz_payload_file("Document Scrapbook", &file, *b"SQUZ", *b"BrSq", 0)
            .unwrap()
            .unwrap();

        assert_eq!(expanded.data, b"Hello");
        assert!(expanded.rsrc.is_empty());
        assert_eq!(expanded.file_type, *b"TEXT");
        assert_eq!(expanded.creator, *b"PLRM");
        assert_eq!(expanded.finder_flags, 0x0100);
    }

    #[test]
    fn empty_resource_fork_is_parseable() {
        assert!(ResourceFork::parse(&empty_resource_fork_bytes()).is_some());
    }

    #[test]
    fn broderbund_squz_unparseable_rsrc_payload_mounts_empty_resource_fork() {
        let stream = [0xFF, b'H', b'e', b'l', b'l', b'o'];
        let mut file = Vec::new();
        file.extend_from_slice(&5u32.to_be_bytes());
        file.extend_from_slice(b"PLR2");
        file.extend_from_slice(b"PLRM");
        file.extend_from_slice(&0x0500u16.to_be_bytes());
        file.extend_from_slice(&[0; 42]);
        file.extend_from_slice(b"KG\x03\x03");
        file.extend_from_slice(&5u32.to_be_bytes());
        file.extend_from_slice(&(stream.len() as u32).to_be_bytes());
        file.extend_from_slice(&stream);

        let expanded = expand_squz_payload_file("Activity.rsrc", &file, *b"SQUZ", *b"BrSq", 0)
            .unwrap()
            .unwrap();

        assert!(expanded.data.is_empty());
        assert!(ResourceFork::parse(&expanded.rsrc).is_some());
    }
}
