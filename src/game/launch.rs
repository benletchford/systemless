//! Shared game loading and initialization for all Systemless frontends.
//!
//! Consolidates application loading (BinHex, MacBinary, StuffIt, and web
//! packs), runner initialization, and post-load configuration so all
//! frontends behave identically.

use crate::loader::cfrg::{
    parse_cfrg_resource, select_powerpc_application_fragment, ARCH_POWERPC, LOCATION_ON_DISK_FLAT,
    USAGE_LIB, WHOLE_FORK,
};
use crate::loader::pef::{parse_pef_header, parse_pef_loader_header, resolve_pef_main_entry};
use crate::loader::ppc::{
    PpcCfmLibraryFragment, PpcVfsDirectory, PpcVfsFileRecord, PpcVfsResourceFileRecord,
    PpcVfsResourceRecord, PpcVfsVolumeRecord,
};
use crate::loader::LoadedApp;
use crate::managers::resource::ResourceFork;
use crate::memory::MemoryBus;
use crate::runner::{FixtureRunner, FixtureRunnerConfig};
use std::io::{Cursor, Read};
use std::path::Component;
use stuffit::{SitArchive, SitEntry};

const LEGACY_WEB_PACK_MAGIC: &[u8; 4] = b"KPK1";
const WEB_PACK_MAGIC: &[u8; 4] = b"KPK2";
const WEB_PACK_INITIAL_FORK_RESERVE_BYTES: usize = 1024 * 1024;
const MAX_ZIP_ENTRIES: usize = 4096;
const MAX_ZIP_ENTRY_BYTES: u64 = 512 * 1024 * 1024;
const MAX_ZIP_TOTAL_BYTES: u64 = 1024 * 1024 * 1024;

fn is_web_pack(file_data: &[u8]) -> bool {
    file_data.starts_with(WEB_PACK_MAGIC) || file_data.starts_with(LEGACY_WEB_PACK_MAGIC)
}

fn is_zip_archive(file_data: &[u8]) -> bool {
    file_data.starts_with(b"PK\x03\x04")
        || file_data.starts_with(b"PK\x05\x06")
        || file_data.starts_with(b"PK\x07\x08")
}

/// Standard frontend RAM size from the canonical machine profile.
pub const RAM_SIZE: u32 = crate::machine_profile::REFERENCE_MACHINE_PROFILE.ram_size_bytes;

/// Max instructions to execute per GUI/WASM frame.
/// Must be large enough to complete a full PICT draw (~500K instructions)
/// in one frame, otherwise the user sees partially-rendered intermediate states.
pub const MAX_INSTRUCTIONS_PER_FRAME: usize = 2_000_000;

/// Create a new FixtureRunner with standard configuration.
pub fn new_runner() -> FixtureRunner {
    new_runner_with_addressing(true)
}

/// Create a runner in either the default 32-bit addressing mode or classic
/// 24-bit mode, where the upper address byte is ignored by memory accesses.
pub fn new_runner_with_addressing(addressing_32_bit: bool) -> FixtureRunner {
    let config = FixtureRunnerConfig {
        load_address: 0x10000,
        max_instructions: MAX_INSTRUCTIONS_PER_FRAME,
        addressing_32_bit,
        ..FixtureRunnerConfig::default()
    };
    FixtureRunner::new(RAM_SIZE as usize, config)
}

/// Create a standard runner with one explicit indexed depth for both 68K and
/// native PowerPC launches.
pub fn new_runner_with_screen_depth(screen_depth: u16) -> FixtureRunner {
    new_runner_with_configuration(true, screen_depth)
}

/// Create a standard runner with explicit addressing and one indexed display
/// depth applied to both 68K and native PowerPC launches.
pub fn new_runner_with_configuration(addressing_32_bit: bool, screen_depth: u16) -> FixtureRunner {
    let config = FixtureRunnerConfig {
        load_address: 0x10000,
        max_instructions: MAX_INSTRUCTIONS_PER_FRAME,
        addressing_32_bit,
        ..FixtureRunnerConfig::default()
    }
    .with_screen_depth(screen_depth)
    .expect("frontend selected an unsupported screen depth");
    let mut runner = FixtureRunner::new(RAM_SIZE as usize, config);
    runner
        .set_powerpc_screen_depth(screen_depth)
        .expect("frontend selected an unsupported PowerPC screen depth");
    runner
}

/// Load an application from BinHex, MacBinary, StuffIt, web-pack, or raw
/// resource-fork bytes.
///
/// Handles StuffIt archives (populates VFS with all entries, finds executable),
/// MacBinary files, and macOS resource fork paths. Returns the LoadedApp on success.
pub fn load_game(runner: &mut FixtureRunner, file_data: &[u8]) -> Result<LoadedApp, String> {
    if is_web_pack(file_data) {
        load_web_pack(runner, file_data)
    } else if is_stuffit_archive(file_data) {
        load_stuffit(runner, file_data)
    } else if crate::binhex::looks_like_binhex(file_data) {
        load_binhex(runner, file_data)
    } else if crate::disk_image::looks_like_dc42_or_hfs(file_data) {
        load_disk_image(runner, file_data)
    } else if is_zip_archive(file_data) {
        load_zip(runner, file_data)
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
            file_entries.extend(payload_from_disk_image(image, 1)?.files);
        } else if is_zip_archive(source) {
            file_entries.extend(collect_zip_payload(source)?.files);
        } else {
            return Err(
                "Game source is not a StuffIt archive, ZIP archive, or HFS disk image".to_string(),
            );
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
        || crate::binhex::looks_like_binhex(&file_data)
        || crate::disk_image::looks_like_dc42_or_hfs(&file_data)
        || is_zip_archive(&file_data)
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

    if let Some((root, app_rel, rsrc_data)) = find_exported_resource_sidecar(path) {
        if crate::runner::trace_load_enabled() {
            eprintln!(
                "[LOAD] Loading exported host tree from {} with app {}",
                root.display(),
                app_rel
            );
        }
        return load_exported_host_tree(runner, &root, &app_rel, file_data, rsrc_data);
    }

    // Fall back to detecting MacBinary/raw resource-fork style payloads.
    load_game(runner, &file_data)
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

/// Decompress the forks of every file entry, in archive order.
///
/// Fork decompression dominates a desktop launch -- EV Override's 10.8 MB
/// SIT-5 archive costs ~1.5 s of arithmetic decoding on one core, 29% of the
/// whole boot (Instruments, from first instruction) -- and every entry is
/// independent: `SitEntry::decompressed_forks` takes `&self` and is
/// documented for parallel use. Decode with one worker per available core,
/// handing out entries through a shared counter so a few large forks cannot
/// strand the other workers. Results (and the first error, if any) come back
/// in entry order, so callers behave exactly as the sequential loop did. On
/// wasm32 (no threads) and for single-entry archives this is the plain
/// sequential loop.
type DecodedForks = Vec<Result<(Vec<u8>, Vec<u8>), stuffit::SitError>>;
fn decompress_file_entries(entries: &[&SitEntry]) -> DecodedForks {
    #[cfg(not(target_arch = "wasm32"))]
    {
        let workers = std::thread::available_parallelism()
            .map(|cores| cores.get())
            .unwrap_or(1)
            .min(entries.len());
        if workers > 1 {
            let next = std::sync::atomic::AtomicUsize::new(0);
            let mut decoded: DecodedForks = Vec::with_capacity(entries.len());
            decoded.resize_with(entries.len(), || Ok((Vec::new(), Vec::new())));
            std::thread::scope(|scope| {
                let handles: Vec<_> = (0..workers)
                    .map(|_| {
                        scope.spawn(|| {
                            let mut local = Vec::new();
                            loop {
                                let index = next.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                                let Some(entry) = entries.get(index) else {
                                    break;
                                };
                                local.push((index, entry.decompressed_forks()));
                            }
                            local
                        })
                    })
                    .collect();
                for handle in handles {
                    for (index, result) in handle.join().expect("fork-decode worker panicked") {
                        decoded[index] = result;
                    }
                }
            });
            return decoded;
        }
    }
    entries
        .iter()
        .map(|entry| entry.decompressed_forks())
        .collect()
}

fn load_stuffit(runner: &mut FixtureRunner, file_data: &[u8]) -> Result<LoadedApp, String> {
    let archive =
        SitArchive::parse(file_data).map_err(|e| format!("Failed to parse StuffIt: {:?}", e))?;

    let file_entries: Vec<&SitEntry> = archive
        .entries
        .iter()
        .filter(|entry| !entry.is_folder)
        .collect();
    let mut decoded = decompress_file_entries(&file_entries).into_iter();

    let mut executable_entry: Option<ExecutableCandidate> = None;
    let mut skipped_disk_image_errors = Vec::new();
    let mut payload = Payload {
        dirs: Vec::new(),
        files: Vec::new(),
        volumes: Vec::new(),
        skipped_disk_image_errors: Vec::new(),
    };

    for entry in &archive.entries {
        if entry.is_folder {
            payload.dirs.push(entry.name.clone());
            continue;
        }

        let (data, rsrc) = decoded
            .next()
            .expect("one decoded fork pair per file entry")
            .map_err(|e| format!("Decompress error: {:?}", e))?;

        let entry_payload = payload_from_forks(
            &entry.name,
            data,
            rsrc,
            entry.file_type,
            entry.creator,
            entry.finder_flags,
            1,
        )?;
        payload.dirs.extend(entry_payload.dirs);
        payload.files.extend(entry_payload.files);
        payload.volumes.extend(entry_payload.volumes);
        payload
            .skipped_disk_image_errors
            .extend(entry_payload.skipped_disk_image_errors);
    }
    let payload = expand_split_vise_payloads(payload)?;
    skipped_disk_image_errors.extend(payload.skipped_disk_image_errors.iter().cloned());
    insert_payload_into_vfs(runner, payload, &mut executable_entry);

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
            2,
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
    let payload = parse_macbinary_payload(file_data, "", 2)?;
    if is_stuffit_archive(&payload.data) {
        if crate::runner::trace_load_enabled() {
            eprintln!("[LOAD] MacBinary data fork contains StuffIt archive");
        }
        return load_stuffit(runner, &payload.data);
    }
    if crate::disk_image::looks_like_dc42_or_hfs(&payload.data) {
        if crate::runner::trace_load_enabled() {
            eprintln!("[LOAD] MacBinary data fork contains HFS disk image");
        }
        return load_disk_image(runner, &payload.data);
    }

    if crate::runner::trace_load_enabled() {
        eprintln!("[LOAD] Loading from MacBinary format");
    }

    let mut executable = None;
    insert_payload_into_vfs(
        runner,
        Payload {
            dirs: Vec::new(),
            files: vec![payload],
            volumes: Vec::new(),
            skipped_disk_image_errors: Vec::new(),
        },
        &mut executable,
    );
    let executable = executable.ok_or("No executable found in MacBinary file")?;
    load_selected_executable(runner, &executable)
}

fn load_zip(runner: &mut FixtureRunner, file_data: &[u8]) -> Result<LoadedApp, String> {
    let mut executable = None;
    let payload = collect_zip_payload(file_data)?;
    let skipped_disk_image_errors = payload.skipped_disk_image_errors.clone();
    insert_payload_into_vfs(runner, payload, &mut executable);
    log_vfs(runner);

    let executable =
        executable.ok_or_else(|| no_executable_archive_error(&skipped_disk_image_errors))?;
    if crate::runner::trace_load_enabled() {
        eprintln!("[LOAD] Selected executable: {}", executable.name);
    }
    load_selected_executable(runner, &executable)
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
        payload_from_disk_image(image, 1)?,
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
                maybe_select_executable_with_preference(
                    &mut self.executable_entry,
                    &pending.name,
                    &pending.data,
                    &pending.rsrc,
                    pending.is_appl,
                    pending.data_len,
                    pending.creator_code,
                    1,
                    runner.prefers_powerpc_executables() || prefer_powerpc(),
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

fn load_exported_host_tree(
    runner: &mut FixtureRunner,
    root: &std::path::Path,
    app_rel: &str,
    app_data: Vec<u8>,
    app_rsrc: Vec<u8>,
) -> Result<LoadedApp, String> {
    let mut executable_entry: Option<ExecutableCandidate> = None;
    let mut payload = payload_from_exported_host_tree(root, app_rel)?;

    if !payload
        .files
        .iter()
        .any(|file| file.name.eq_ignore_ascii_case(app_rel))
    {
        payload.files.push(PayloadFile {
            name: app_rel.to_string(),
            data: app_data,
            rsrc: app_rsrc,
            file_type: *b"APPL",
            creator: *b"????",
            finder_flags: 0,
            executable_priority: 2,
        });
    }

    insert_payload_into_vfs(runner, payload, &mut executable_entry);
    log_vfs(runner);

    let executable = executable_entry.ok_or("No executable found in exported host tree")?;
    load_selected_executable(runner, &executable)
}

fn payload_from_exported_host_tree(
    root: &std::path::Path,
    app_rel: &str,
) -> Result<Payload, String> {
    let mut dirs = Vec::new();
    let mut files = Vec::new();
    collect_exported_host_tree(root, root, app_rel, &mut dirs, &mut files)?;
    Ok(Payload {
        dirs,
        files,
        volumes: Vec::new(),
        skipped_disk_image_errors: Vec::new(),
    })
}

fn collect_exported_host_tree(
    root: &std::path::Path,
    dir: &std::path::Path,
    app_rel: &str,
    dirs: &mut Vec<String>,
    files: &mut Vec<PayloadFile>,
) -> Result<(), String> {
    let entries = std::fs::read_dir(dir)
        .map_err(|err| format!("read exported host directory {}: {}", dir.display(), err))?;
    for entry in entries {
        let entry = entry.map_err(|err| {
            format!(
                "read exported host directory entry in {}: {}",
                dir.display(),
                err
            )
        })?;
        let path = entry.path();
        let rel = match path.strip_prefix(root) {
            Ok(rel) => rel,
            Err(_) => continue,
        };
        let rel_name = rel.to_string_lossy().to_string();
        if rel_name.is_empty()
            || rel_name.starts_with("__rsrc__")
            || rel
                .components()
                .any(|component| component.as_os_str() == ".rsrc")
        {
            continue;
        }
        let file_type = entry.file_type().map_err(|err| {
            format!(
                "stat exported host directory entry {}: {}",
                path.display(),
                err
            )
        })?;
        if file_type.is_dir() {
            dirs.push(rel_name.clone());
            collect_exported_host_tree(root, &path, app_rel, dirs, files)?;
            continue;
        }
        if !file_type.is_file() || exported_host_file_is_harness_artifact(&rel_name) {
            continue;
        }

        let data = std::fs::read(&path)
            .map_err(|err| format!("read exported host file {}: {}", path.display(), err))?;
        let rsrc = read_exported_resource_sidecar_for_rel(root, rel).unwrap_or_default();
        let is_app = rel_name.eq_ignore_ascii_case(app_rel);
        files.push(PayloadFile {
            name: rel_name,
            data,
            rsrc,
            file_type: if is_app { *b"APPL" } else { *b"????" },
            creator: *b"????",
            finder_flags: 0,
            executable_priority: if is_app { 2 } else { 1 },
        });
    }
    Ok(())
}

fn exported_host_file_is_harness_artifact(rel_name: &str) -> bool {
    let lower = rel_name.to_ascii_lowercase();
    lower.ends_with(".png") || lower.ends_with(".ctx.json")
}

fn find_exported_resource_sidecar(
    path: &std::path::Path,
) -> Option<(std::path::PathBuf, String, Vec<u8>)> {
    let file_name = path.file_name()?;
    if let Some(parent) = path.parent() {
        let sidecar = parent.join(".rsrc").join(file_name);
        if let Ok(bytes) = std::fs::read(&sidecar) {
            if !bytes.is_empty() {
                let root = parent.to_path_buf();
                let app_rel = file_name.to_string_lossy().to_string();
                return Some((root, app_rel, bytes));
            }
        }
    }

    for root in path.ancestors().skip(1) {
        let Ok(rel) = path.strip_prefix(root) else {
            continue;
        };
        let sidecar = root.join(format!("__rsrc__{}", rel.to_string_lossy()));
        if let Ok(bytes) = std::fs::read(&sidecar) {
            if !bytes.is_empty() {
                return Some((root.to_path_buf(), rel.to_string_lossy().to_string(), bytes));
            }
        }
    }
    None
}

fn read_exported_resource_sidecar_for_rel(
    root: &std::path::Path,
    rel: &std::path::Path,
) -> Option<Vec<u8>> {
    let sidecar = root.join(format!("__rsrc__{}", rel.to_string_lossy()));
    if let Ok(bytes) = std::fs::read(sidecar) {
        if !bytes.is_empty() {
            return Some(bytes);
        }
    }
    let parent = rel.parent()?;
    let file_name = rel.file_name()?;
    let sidecar = root.join(parent).join(".rsrc").join(file_name);
    std::fs::read(sidecar)
        .ok()
        .filter(|bytes| !bytes.is_empty())
}

#[derive(Debug)]
struct Payload {
    dirs: Vec<String>,
    files: Vec<PayloadFile>,
    volumes: Vec<(String, crate::disk_image::DiskImageVolumeInfo)>,
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
    executable_priority: u8,
}

fn collect_zip_payload(file_data: &[u8]) -> Result<Payload, String> {
    let reader = Cursor::new(file_data);
    let mut archive = zip::ZipArchive::new(reader)
        .map_err(|err| format!("Failed to parse ZIP archive: {err}"))?;
    if archive.len() > MAX_ZIP_ENTRIES {
        return Err(format!(
            "ZIP archive exceeds the {MAX_ZIP_ENTRIES} entry limit"
        ));
    }
    let mut payload = Payload {
        dirs: Vec::new(),
        files: Vec::new(),
        volumes: Vec::new(),
        skipped_disk_image_errors: Vec::new(),
    };
    let mut total_uncompressed_bytes = 0u64;

    for index in 0..archive.len() {
        let mut entry = archive
            .by_index(index)
            .map_err(|err| format!("Failed to read ZIP entry {index}: {err}"))?;
        if entry.encrypted() {
            return Err(format!(
                "Encrypted ZIP entry is not supported: {}",
                entry.name()
            ));
        }
        let path = safe_zip_entry_path(&entry)?;
        if path.is_empty() {
            continue;
        }
        if entry.is_dir() {
            payload.dirs.push(path.trim_end_matches('/').to_string());
            continue;
        }

        if entry.size() > MAX_ZIP_ENTRY_BYTES {
            return Err(format!(
                "ZIP entry exceeds the {} byte extraction limit: {path}",
                MAX_ZIP_ENTRY_BYTES
            ));
        }
        total_uncompressed_bytes = total_uncompressed_bytes
            .checked_add(entry.size())
            .ok_or_else(|| "ZIP uncompressed size overflow".to_string())?;
        if total_uncompressed_bytes > MAX_ZIP_TOTAL_BYTES {
            return Err(format!(
                "ZIP archive exceeds the {} byte extraction limit",
                MAX_ZIP_TOTAL_BYTES
            ));
        }

        let declared_size = entry.size();
        let entry_size = usize::try_from(declared_size)
            .map_err(|_| format!("ZIP entry is too large for this platform: {path}"))?;
        let mut data = Vec::new();
        data.try_reserve_exact(entry_size)
            .map_err(|_| format!("Failed to allocate memory for ZIP entry: {path}"))?;
        (&mut entry)
            .take(declared_size.saturating_add(1))
            .read_to_end(&mut data)
            .map_err(|err| format!("Failed to decompress ZIP entry {path}: {err}"))?;
        if data.len() as u64 != declared_size {
            return Err(format!(
                "ZIP entry size mismatch for {path}: declared {declared_size}, decoded {}",
                data.len()
            ));
        }

        if looks_like_macbinary(&data) {
            let parent = path.rsplit_once('/').map_or("", |(parent, _)| parent);
            payload
                .files
                .push(parse_macbinary_payload(&data, parent, 1)?);
            continue;
        }

        let nested = payload_from_forks(&path, data, Vec::new(), *b"????", *b"????", 0, 1)?;
        payload.dirs.extend(nested.dirs);
        payload.files.extend(nested.files);
        payload.volumes.extend(nested.volumes);
        payload
            .skipped_disk_image_errors
            .extend(nested.skipped_disk_image_errors);
    }

    expand_split_vise_payloads(payload)
}

fn safe_zip_entry_path<R: std::io::Read>(
    entry: &zip::read::ZipFile<'_, R>,
) -> Result<String, String> {
    let original = entry.name();
    if original.contains('\\') {
        return Err(format!("Unsafe ZIP entry path: {original}"));
    }
    let enclosed = entry
        .enclosed_name()
        .ok_or_else(|| format!("Unsafe ZIP entry path: {original}"))?;
    if enclosed.components().any(|component| {
        matches!(
            component,
            Component::ParentDir | Component::RootDir | Component::Prefix(_)
        )
    }) {
        return Err(format!("Unsafe ZIP entry path: {original}"));
    }

    Ok(enclosed
        .components()
        .filter_map(|component| match component {
            Component::Normal(value) => value.to_str(),
            Component::CurDir => None,
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("/"))
}

fn looks_like_macbinary(file_data: &[u8]) -> bool {
    if file_data.len() < 128 || file_data[0] != 0 || file_data[74] != 0 || file_data[82] != 0 {
        return false;
    }
    let name_len = file_data[1] as usize;
    if !(1..=63).contains(&name_len) || file_data[2..2 + name_len].contains(&0) {
        return false;
    }
    parse_macbinary_payload(file_data, "", 1).is_ok()
}

fn parse_macbinary_payload(
    file_data: &[u8],
    parent: &str,
    executable_priority: u8,
) -> Result<PayloadFile, String> {
    if file_data.len() < 128 {
        return Err("File too small for MacBinary".to_string());
    }

    let data_len =
        u32::from_be_bytes([file_data[83], file_data[84], file_data[85], file_data[86]]) as usize;
    let rsrc_len =
        u32::from_be_bytes([file_data[87], file_data[88], file_data[89], file_data[90]]) as usize;
    let data_end = 128usize
        .checked_add(data_len)
        .ok_or_else(|| "MacBinary data offset overflow".to_string())?;
    let data_padded_len = data_len
        .checked_add(127)
        .map(|len| len & !127)
        .ok_or_else(|| "MacBinary data padding overflow".to_string())?;
    let rsrc_start = 128usize
        .checked_add(data_padded_len)
        .ok_or_else(|| "MacBinary resource offset overflow".to_string())?;
    let rsrc_end = rsrc_start
        .checked_add(rsrc_len)
        .ok_or_else(|| "MacBinary resource offset overflow".to_string())?;
    if data_end > file_data.len() || rsrc_end > file_data.len() {
        return Err("MacBinary truncated".to_string());
    }

    let name_len = (file_data[1] as usize).min(63);
    let name = crate::mac_roman::decode_mac_roman(&file_data[2..2 + name_len]);
    let name = if parent.is_empty() {
        name
    } else {
        format!("{parent}/{name}")
    };
    let file_type = file_data[65..69]
        .try_into()
        .map_err(|_| "MacBinary file type is truncated".to_string())?;
    let creator = file_data[69..73]
        .try_into()
        .map_err(|_| "MacBinary creator is truncated".to_string())?;

    Ok(PayloadFile {
        name,
        data: file_data[128..data_end].to_vec(),
        rsrc: file_data[rsrc_start..rsrc_end].to_vec(),
        file_type,
        creator,
        finder_flags: (u16::from(file_data[73]) << 8) | u16::from(file_data[101]),
        executable_priority,
    })
}

fn collect_stuffit_payload_files(archive: &SitArchive) -> Result<Vec<PayloadFile>, String> {
    Ok(payload_from_stuffit_archive(archive, 1)?.files)
}

fn payload_from_stuffit_archive(
    archive: &SitArchive,
    executable_priority: u8,
) -> Result<Payload, String> {
    let mut payload = Payload {
        dirs: Vec::new(),
        files: Vec::new(),
        volumes: Vec::new(),
        skipped_disk_image_errors: Vec::new(),
    };
    for entry in archive.entries.iter().filter(|entry| !entry.is_folder) {
        let (data, rsrc) = entry
            .decompressed_forks()
            .map_err(|e| format!("Decompress error: {:?}", e))?;
        let entry_payload = payload_from_forks(
            &entry.name,
            data,
            rsrc,
            entry.file_type,
            entry.creator,
            entry.finder_flags,
            executable_priority,
        )?;
        payload.dirs.extend(entry_payload.dirs);
        payload.files.extend(entry_payload.files);
        payload.volumes.extend(entry_payload.volumes);
        payload
            .skipped_disk_image_errors
            .extend(entry_payload.skipped_disk_image_errors);
    }
    for entry in archive.entries.iter().filter(|entry| entry.is_folder) {
        payload.dirs.push(entry.name.clone());
    }
    expand_split_vise_payloads(payload)
}

fn payload_from_stuffit_bytes(
    name: &str,
    bytes: &[u8],
    executable_priority: u8,
) -> Result<Payload, String> {
    let archive = SitArchive::parse(bytes)
        .map_err(|e| format!("Nested StuffIt {name}: failed to parse: {:?}", e))?;
    payload_from_stuffit_archive(&archive, executable_priority)
}

fn expand_squz_payload_file(
    name: &str,
    data: &[u8],
    file_type: [u8; 4],
    creator: [u8; 4],
    finder_flags: u16,
    executable_priority: u8,
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
                    executable_priority,
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
        executable_priority,
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
    executable_priority: u8,
) -> Result<Payload, String> {
    if is_stuffit_archive(&data) {
        if crate::runner::trace_load_enabled() {
            eprintln!("[LOAD] Extracting nested StuffIt archive from data fork \"{name}\"");
        }
        return payload_from_stuffit_bytes(name, &data, executable_priority);
    }

    if is_stuffit_archive(&rsrc) {
        if crate::runner::trace_load_enabled() {
            eprintln!("[LOAD] Extracting nested StuffIt archive from resource fork \"{name}\"");
        }
        return payload_from_stuffit_bytes(name, &rsrc, executable_priority);
    }

    if let Some(payload) = expand_installer_maker_payload(name, &data, executable_priority)? {
        return Ok(payload);
    }

    if let Some(parsed) = crate::game::vise::parse_vise(&data) {
        match parsed {
            Ok(_) => match expand_vise_payload(name, &data, executable_priority) {
                Ok(Some(payload)) => return Ok(payload),
                Ok(None) => {}
                Err(error) => {
                    if crate::runner::trace_load_enabled() {
                        eprintln!(
                        "[LOAD] Preserving Installer VISE payload \"{}\" for possible continuation: {}",
                        name, error
                    );
                    }
                }
            },
            Err(error) => {
                if crate::runner::trace_load_enabled() {
                    eprintln!(
                        "[LOAD] Preserving unexpanded Installer VISE payload \"{}\": {}",
                        name, error
                    );
                }
            }
        }
    }

    if let Some(file) = expand_squz_payload_file(
        name,
        &data,
        file_type,
        creator,
        finder_flags,
        executable_priority,
    )? {
        return Ok(Payload {
            dirs: Vec::new(),
            files: vec![file],
            volumes: Vec::new(),
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
            return payload_from_disk_image(image, executable_priority);
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
            return payload_from_disk_image(image, executable_priority);
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
            executable_priority,
        }],
        volumes: Vec::new(),
        skipped_disk_image_errors,
    })
}

fn expand_installer_maker_payload(
    name: &str,
    data: &[u8],
    executable_priority: u8,
) -> Result<Option<Payload>, String> {
    let Some(container) = crate::game::installer_maker::parse_installer_maker_st46(data) else {
        return Ok(None);
    };

    if crate::runner::trace_load_enabled() {
        eprintln!(
            "[LOAD] Extracting InstallerMaker ST46 payload from \"{}\": {} files",
            name,
            container.entries.len()
        );
    }

    let mut payload = Payload {
        dirs: vec![name.to_string()],
        files: Vec::new(),
        volumes: Vec::new(),
        skipped_disk_image_errors: Vec::new(),
    };
    for entry in container.entries {
        let rsrc = decode_installer_fork(
            name,
            &entry.name,
            "resource",
            entry.rsrc_method,
            entry.rsrc_packed,
            entry.rsrc_unpacked_len,
        )?;
        let data = decode_installer_fork(
            name,
            &entry.name,
            "data",
            entry.data_method,
            entry.data_packed,
            entry.data_unpacked_len,
        )?;
        let embedded_name = format!("{}/{}", name, entry.name);
        if crate::runner::trace_load_enabled() {
            eprintln!(
                "[LOAD] Expanded InstallerMaker \"{}\" data={} rsrc={}",
                embedded_name,
                data.len(),
                rsrc.len()
            );
        }
        let vise_fallback = data
            .starts_with(b"SVCT")
            .then(|| (data.clone(), rsrc.clone()));
        let entry_payload = match payload_from_forks(
            &embedded_name,
            data,
            rsrc,
            entry.file_type,
            entry.creator,
            entry.finder_flags,
            executable_priority,
        ) {
            Ok(entry_payload) => entry_payload,
            Err(error) if vise_fallback.is_some() => {
                if crate::runner::trace_load_enabled() {
                    eprintln!(
                        "[LOAD] Preserving unexpanded nested Installer VISE \"{}\": {}",
                        embedded_name, error
                    );
                }
                let (data, rsrc) = vise_fallback.unwrap();
                Payload {
                    dirs: Vec::new(),
                    files: vec![PayloadFile {
                        name: embedded_name,
                        data,
                        rsrc,
                        file_type: entry.file_type,
                        creator: entry.creator,
                        finder_flags: entry.finder_flags,
                        executable_priority,
                    }],
                    volumes: Vec::new(),
                    skipped_disk_image_errors: Vec::new(),
                }
            }
            Err(error) => return Err(error),
        };
        payload.dirs.extend(entry_payload.dirs);
        payload.files.extend(entry_payload.files);
        payload.volumes.extend(entry_payload.volumes);
        payload
            .skipped_disk_image_errors
            .extend(entry_payload.skipped_disk_image_errors);
    }

    Ok(Some(payload))
}

fn expand_vise_payload(
    name: &str,
    data: &[u8],
    executable_priority: u8,
) -> Result<Option<Payload>, String> {
    let Some(parsed) = crate::game::vise::parse_vise(data) else {
        return Ok(None);
    };
    let archive = parsed.map_err(|error| format!("Installer VISE {name}: {error}"))?;

    if crate::runner::trace_load_enabled() {
        eprintln!(
            "[LOAD] Extracting Installer VISE payload from \"{}\": {} files",
            name,
            archive.entries.len()
        );
    }

    let mut required_by_stream = std::collections::HashMap::<(usize, usize), usize>::new();
    let mut packed_by_stream = std::collections::HashMap::<(usize, usize), &[u8]>::new();
    for entry in &archive.entries {
        for (packed_offset, packed, unpacked_len) in [
            (
                entry.data_packed_offset,
                entry.data_packed,
                entry.data_unpacked_len,
            ),
            (
                entry.rsrc_packed_offset,
                entry.rsrc_packed,
                entry.rsrc_unpacked_len,
            ),
        ] {
            if unpacked_len == 0 {
                continue;
            }
            let required_len =
                entry
                    .unpacked_offset
                    .checked_add(unpacked_len)
                    .ok_or_else(|| {
                        format!(
                            "Installer VISE {name}/{} unpacked fork range overflow",
                            entry.path
                        )
                    })?;
            let key = (packed_offset, packed.len());
            required_by_stream
                .entry(key)
                .and_modify(|current| *current = (*current).max(required_len))
                .or_insert(required_len);
            packed_by_stream.entry(key).or_insert(packed);
        }
    }
    let mut decoded_by_stream = std::collections::HashMap::new();
    for (key, required_len) in required_by_stream {
        let packed = packed_by_stream
            .get(&key)
            .ok_or_else(|| format!("Installer VISE {name}: missing packed stream {key:?}"))?;
        let decoded = crate::game::vise::decode_vise_fork(packed, required_len)
            .map_err(|error| format!("Installer VISE {name} stream at 0x{:X}: {error}", key.0))?;
        decoded_by_stream.insert(key, decoded);
    }

    let mut payload = Payload {
        dirs: std::iter::once(name.to_string())
            .chain(
                archive
                    .dirs
                    .into_iter()
                    .map(|directory| format!("{name}/{directory}")),
            )
            .collect(),
        files: Vec::new(),
        volumes: Vec::new(),
        skipped_disk_image_errors: Vec::new(),
    };
    for entry in archive.entries {
        let extract_fork = |packed_offset: usize,
                            packed_len: usize,
                            unpacked_len: usize,
                            fork_name: &str|
         -> Result<Vec<u8>, String> {
            if unpacked_len == 0 {
                return Ok(Vec::new());
            }
            let key = (packed_offset, packed_len);
            let decoded = decoded_by_stream.get(&key).ok_or_else(|| {
                format!(
                    "Installer VISE {name}/{} {fork_name} fork: missing decoded stream",
                    entry.path
                )
            })?;
            let end = entry
                .unpacked_offset
                .checked_add(unpacked_len)
                .ok_or_else(|| {
                    format!(
                        "Installer VISE {name}/{} {fork_name} fork range overflow",
                        entry.path
                    )
                })?;
            decoded
                .get(entry.unpacked_offset..end)
                .map(<[u8]>::to_vec)
                .ok_or_else(|| {
                    format!(
                        "Installer VISE {name}/{} {fork_name} fork range {}..{} exceeds decoded stream {}",
                        entry.path,
                        entry.unpacked_offset,
                        end,
                        decoded.len()
                    )
                })
        };
        let data = extract_fork(
            entry.data_packed_offset,
            entry.data_packed.len(),
            entry.data_unpacked_len,
            "data",
        )?;
        let rsrc = extract_fork(
            entry.rsrc_packed_offset,
            entry.rsrc_packed.len(),
            entry.rsrc_unpacked_len,
            "resource",
        )?;
        let embedded_name = format!("{name}/{}", entry.path);
        if crate::runner::trace_load_enabled() {
            eprintln!(
                "[LOAD] Expanded Installer VISE \"{}\" data={} rsrc={}",
                embedded_name,
                data.len(),
                rsrc.len()
            );
        }
        let entry_payload = payload_from_forks(
            &embedded_name,
            data,
            rsrc,
            entry.file_type,
            entry.creator,
            0,
            executable_priority,
        )?;
        payload.dirs.extend(entry_payload.dirs);
        payload.files.extend(entry_payload.files);
        payload.volumes.extend(entry_payload.volumes);
        payload
            .skipped_disk_image_errors
            .extend(entry_payload.skipped_disk_image_errors);
    }

    Ok(Some(payload))
}

fn expand_split_vise_payloads(mut payload: Payload) -> Result<Payload, String> {
    loop {
        let mut resolved = None;

        'sources: for source_index in 0..payload.files.len() {
            let source = &payload.files[source_index];
            if crate::game::vise::parse_vise(&source.data).is_none()
                || crate::game::vise::continuation_bytes(&source.data).is_some()
            {
                continue;
            }

            let source_name = source.name.clone();
            let executable_priority = source.executable_priority;
            if let Ok(Some(expanded)) =
                expand_vise_payload(&source_name, &source.data, executable_priority)
            {
                resolved = Some((source_index, Vec::new(), expanded, source_name));
                break;
            }

            // Multi-disk VISE installers store the catalog in the first file
            // and continue its logical byte stream in VIS* data files on
            // later disks. Join those segments in archive order before
            // parsing so multi-volume installers can be opened directly.
            let mut continuation_indices = Vec::new();
            for (index, file) in payload.files.iter().enumerate().skip(source_index + 1) {
                if (file.file_type[..3] != *b"VIS" && file.creator[..3] != *b"VIS")
                    || crate::game::vise::continuation_bytes(&file.data).is_none()
                {
                    continue;
                }
                continuation_indices.push(index);
                let continuation_data = continuation_indices
                    .iter()
                    .map(|&index| payload.files[index].data.as_slice())
                    .collect::<Vec<_>>();
                let Ok(joined) = crate::game::vise::join_segments(
                    &payload.files[source_index].data,
                    &continuation_data,
                ) else {
                    continue;
                };
                let Ok(Some(expanded)) =
                    expand_vise_payload(&source_name, &joined, executable_priority)
                else {
                    continue;
                };
                resolved = Some((source_index, continuation_indices, expanded, source_name));
                break 'sources;
            }
        }

        let Some((source_index, continuation_indices, expanded, source_name)) = resolved else {
            break;
        };
        if crate::runner::trace_load_enabled() && !continuation_indices.is_empty() {
            eprintln!(
                "[LOAD] Joined {} Installer VISE continuation file(s) for \"{}\"",
                continuation_indices.len(),
                source_name
            );
        }

        let mut remove = vec![false; payload.files.len()];
        remove[source_index] = true;
        for index in continuation_indices {
            remove[index] = true;
        }
        payload.files = payload
            .files
            .into_iter()
            .enumerate()
            .filter_map(|(index, file)| (!remove[index]).then_some(file))
            .collect();
        payload.dirs.extend(expanded.dirs);
        payload.files.extend(expanded.files);
        payload.volumes.extend(expanded.volumes);
        payload
            .skipped_disk_image_errors
            .extend(expanded.skipped_disk_image_errors);
    }

    Ok(payload)
}

fn decode_installer_fork(
    container_name: &str,
    entry_name: &str,
    fork_name: &str,
    method: u8,
    packed: &[u8],
    unpacked_len: usize,
) -> Result<Vec<u8>, String> {
    match method {
        0 => {
            if packed.len() != unpacked_len {
                return Err(format!(
                    "InstallerMaker {container_name}/{entry_name} {fork_name} fork: stored length {} != declared {}",
                    packed.len(),
                    unpacked_len
                ));
            }
            Ok(packed.to_vec())
        }
        14 => crate::game::installer_maker::decode_installer_method14(packed, unpacked_len)
            .map_err(|error| {
                format!(
                    "InstallerMaker {container_name}/{entry_name} {fork_name} fork: {error}"
                )
            }),
        other => Err(format!(
            "InstallerMaker {container_name}/{entry_name} {fork_name} fork: unsupported method {other}"
        )),
    }
}

fn payload_from_disk_image(
    image: crate::disk_image::DiskImageContents,
    executable_priority: u8,
) -> Result<Payload, String> {
    let crate::disk_image::DiskImageContents {
        volume_name,
        volume_info,
        dirs,
        files,
    } = image;
    let mut payload = Payload {
        dirs,
        files: Vec::new(),
        volumes: vec![(volume_name, volume_info)],
        skipped_disk_image_errors: Vec::new(),
    };
    for file in files {
        let file_payload = payload_from_forks(
            &file.path,
            file.data,
            file.rsrc,
            file.file_type,
            file.creator,
            file.finder_flags,
            executable_priority,
        )?;
        payload.dirs.extend(file_payload.dirs);
        payload.files.extend(file_payload.files);
        payload.volumes.extend(file_payload.volumes);
        payload
            .skipped_disk_image_errors
            .extend(file_payload.skipped_disk_image_errors);
    }
    Ok(payload)
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

    for (volume, info) in payload.volumes {
        runner.dispatcher_mut().mount_vfs_volume(
            &volume,
            info.attributes,
            info.file_count,
            info.allocation_block_count,
            info.allocation_block_size,
            info.clump_size,
            info.free_blocks,
            info.bitmap_start,
            info.allocation_pointer,
            info.allocation_start,
            info.next_catalog_id,
            info.created_date,
            info.modified_date,
        );
    }

    for file in payload.files {
        let data_len = file.data.len();
        let is_appl = file.file_type == *b"APPL";
        maybe_select_executable_with_preference(
            executable_entry,
            &file.name,
            &file.data,
            &file.rsrc,
            is_appl,
            data_len,
            file.creator,
            file.executable_priority,
            runner.prefers_powerpc_executables() || prefer_powerpc(),
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
        .cloned()
        .ok_or_else(|| {
            format!(
                "Selected executable resource fork missing: {}",
                executable.name
            )
        })?;
    match executable.kind {
        ExecutableKind::Classic68k => {
            let fork = ResourceFork::parse(&rsrc).ok_or("Failed to parse resource fork")?;
            let app = runner
                .load_app(&fork)
                .ok_or_else(|| "Failed to load app".to_string())?;
            merge_launch_resource_companions(runner, executable)?;
            Ok(app)
        }
        ExecutableKind::PowerPcPef {
            architecture,
            fragment_offset,
            fragment_length,
            app_stack_size,
        } => {
            let data = runner
                .dispatcher()
                .vfs
                .get(&executable.vfs_key)
                .cloned()
                .ok_or_else(|| {
                    format!("Selected executable data fork missing: {}", executable.name)
                })?;
            let ppc_vfs = ppc_diagnostic_vfs(runner, Some(&executable.name));
            let pef = pef_fragment_data(&data, fragment_offset, fragment_length).ok_or_else(|| {
                format!(
                    "PowerPC PEF executable \"{}\" selected, but cfrg fragment range is outside the data fork (architecture {})",
                    executable.name,
                    fourcc_lossy(architecture)
                )
            })?;
            if crate::runner::trace_load_enabled() {
                if let Some(details) = pef_diagnostic_details(
                    pef,
                    fragment_offset,
                    fragment_length,
                    app_stack_size,
                    Some(&ppc_vfs),
                ) {
                    eprintln!("[LOAD] PowerPC PEF details: {details}");
                }
            }
            let mut ppc_config =
                crate::loader::ppc::PpcLoadConfig::from_cfrg_app_stack_size(app_stack_size);
            ppc_config.screen_depth = runner.configured_powerpc_screen_depth();
            let system_reservation = runner.powerpc_system_reservation_range().ok_or_else(|| {
                format!(
                    "PowerPC PEF executable \"{}\" selected, but runner RAM cannot hold its system reservation",
                    executable.name
                )
            })?;
            let mut loaded =
                crate::loader::ppc::load_pef_application_with_config_and_system_reservation(
                    pef,
                    ppc_config,
                    system_reservation,
                )
                .map_err(|error| {
                    format!(
                        "PowerPC PEF executable \"{}\" selected, but PPC loading failed: {error:?}",
                        executable.name
                    )
                })?;
            let library_fragments = discover_ppc_cfm_library_fragments(&ppc_vfs);
            loaded.seed_cfm_library_fragments(library_fragments);
            loaded.seed_vfs_volumes(ppc_vfs.volumes);
            loaded.seed_vfs_directories(
                ppc_vfs.directories,
                ppc_vfs.default_dir_id,
                ppc_vfs.next_dir_id,
            );
            loaded.seed_vfs_files_and_resources(
                ppc_vfs.files,
                ppc_vfs.resource_files,
                ppc_vfs.resources,
            );
            loaded.set_launched_app_path(&executable.name);
            Ok(LoadedApp::from_ppc(loaded))
        }
    }
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

#[cfg(test)]
fn maybe_select_executable(
    executable_entry: &mut Option<ExecutableCandidate>,
    name: &str,
    data: &[u8],
    rsrc: &[u8],
    is_appl: bool,
    data_len: usize,
    creator: [u8; 4],
    executable_priority: u8,
) {
    maybe_select_executable_with_preference(
        executable_entry,
        name,
        data,
        rsrc,
        is_appl,
        data_len,
        creator,
        executable_priority,
        prefer_powerpc(),
    );
}

fn maybe_select_executable_with_preference(
    executable_entry: &mut Option<ExecutableCandidate>,
    name: &str,
    data: &[u8],
    rsrc: &[u8],
    is_appl: bool,
    data_len: usize,
    creator: [u8; 4],
    executable_priority: u8,
    prefer_powerpc: bool,
) {
    let executable_override = executable_name_override();
    maybe_select_executable_with_override_and_preference(
        executable_entry,
        name,
        data,
        rsrc,
        is_appl,
        data_len,
        creator,
        executable_priority,
        executable_override.as_deref(),
        prefer_powerpc,
    );
}

#[cfg(test)]
fn maybe_select_executable_with_override(
    executable_entry: &mut Option<ExecutableCandidate>,
    name: &str,
    data: &[u8],
    rsrc: &[u8],
    is_appl: bool,
    data_len: usize,
    creator: [u8; 4],
    executable_priority: u8,
    executable_override: Option<&str>,
) {
    maybe_select_executable_with_override_and_preference(
        executable_entry,
        name,
        data,
        rsrc,
        is_appl,
        data_len,
        creator,
        executable_priority,
        executable_override,
        prefer_powerpc(),
    );
}

fn maybe_select_executable_with_override_and_preference(
    executable_entry: &mut Option<ExecutableCandidate>,
    name: &str,
    data: &[u8],
    rsrc: &[u8],
    is_appl: bool,
    data_len: usize,
    creator: [u8; 4],
    executable_priority: u8,
    executable_override: Option<&str>,
    prefer_powerpc: bool,
) {
    if rsrc.is_empty() {
        return;
    }

    let Some(kind) = classify_executable_with_preference(data, rsrc, is_appl, prefer_powerpc)
    else {
        return;
    };

    // SYSTEMLESS_LOAD_EXECUTABLE: prefer an exact candidate path, then a
    // case-sensitive substring match, then the normal size/APPL heuristic.
    // The substring fallback supports short user-facing names, while exact
    // precedence keeps "Game" from losing to a larger "Game Installer".
    let override_rank = executable_override_match_rank(name, executable_override);
    let prev_override_rank = executable_entry
        .as_ref()
        .map(|prev| executable_override_match_rank(&prev.name, executable_override))
        .unwrap_or(0);

    let candidate = ExecutableCandidate {
        name: name.to_string(),
        vfs_key: crate::trap::dispatch::TrapDispatcher::normalize_vfs_path(name),
        kind,
        is_appl,
        has_data_fork: data_len > 0,
        score: data_len.max(rsrc.len()),
        priority: executable_priority,
        creator,
        is_installer: is_installer_executable(name, creator),
        is_documentation: is_documentation_executable(name),
        is_demo: executable_name_has_role(name, "demo"),
        version: executable_version(rsrc),
    };

    let take = if override_rank != prev_override_rank {
        override_rank > prev_override_rank
    } else {
        match executable_entry.as_ref() {
            Some(prev) => executable_candidate_is_better(&candidate, prev),
            None => true,
        }
    };

    if take {
        *executable_entry = Some(candidate);
    }
}

fn executable_override_match_rank(name: &str, executable_override: Option<&str>) -> u8 {
    match executable_override {
        Some(needle) if name == needle => 2,
        Some(needle) if name.contains(needle) => 1,
        _ => 0,
    }
}

fn executable_name_override() -> Option<String> {
    std::env::var("SYSTEMLESS_LOAD_EXECUTABLE")
        .ok()
        .filter(|s| !s.is_empty())
}

fn prefer_powerpc() -> bool {
    matches!(
        std::env::var("SYSTEMLESS_PREFER_POWERPC").ok().as_deref(),
        Some("1" | "true" | "True" | "TRUE" | "yes" | "Yes" | "YES")
    )
}

#[derive(Clone, Debug)]
struct ExecutableCandidate {
    name: String,
    vfs_key: String,
    kind: ExecutableKind,
    is_appl: bool,
    has_data_fork: bool,
    score: usize,
    priority: u8,
    creator: [u8; 4],
    is_installer: bool,
    is_documentation: bool,
    is_demo: bool,
    version: Option<u32>,
}

impl ExecutableCandidate {
    fn selection_key(&self) -> (u8, bool, bool, bool, bool, bool, bool, bool, usize) {
        (
            self.priority,
            self.is_appl,
            !self.is_installer,
            !self.is_documentation,
            !self.is_demo,
            !is_system_folder_path(&self.name),
            self.kind.is_powerpc(),
            self.has_data_fork,
            self.score,
        )
    }
}

fn executable_candidate_is_better(
    candidate: &ExecutableCandidate,
    previous: &ExecutableCandidate,
) -> bool {
    let candidate_class = candidate.version_selection_class();
    let previous_class = previous.version_selection_class();
    if candidate_class == previous_class && same_application_family(candidate, previous) {
        match (candidate.version, previous.version) {
            (Some(candidate_version), Some(previous_version))
                if candidate_version != previous_version =>
            {
                return candidate_version > previous_version;
            }
            _ => {}
        }
    }

    candidate.selection_key() > previous.selection_key()
}

impl ExecutableCandidate {
    fn version_selection_class(&self) -> (u8, bool, bool, bool, bool, bool) {
        (
            self.priority,
            self.is_appl,
            !self.is_installer,
            !is_system_folder_path(&self.name),
            self.kind.is_powerpc(),
            self.has_data_fork,
        )
    }
}

fn same_application_family(left: &ExecutableCandidate, right: &ExecutableCandidate) -> bool {
    left.creator == right.creator
        && left.creator != *b"????"
        && left.creator != [0; 4]
        && executable_family_name(&left.name) == executable_family_name(&right.name)
}

fn executable_family_name(name: &str) -> String {
    name.rsplit('/')
        .next()
        .unwrap_or(name)
        .to_ascii_lowercase()
        .split(|character: char| !character.is_ascii_alphanumeric())
        .filter(|word| !word.is_empty() && !matches!(*word, "demo" | "trial" | "shareware"))
        .collect::<Vec<_>>()
        .join(" ")
}

fn executable_version(rsrc: &[u8]) -> Option<u32> {
    let fork = ResourceFork::parse(rsrc)?;
    let resource = fork.resources().get(&(*b"vers", 1))?;
    let bytes: [u8; 4] = resource.data.get(..4)?.try_into().ok()?;

    // Macintosh Toolbox Essentials (1992), Finder Interface pp. 7-69–7-70:
    // a file's 'vers' resource ID 1 begins with its four-byte numeric version.
    Some(u32::from_be_bytes(bytes))
}

fn is_installer_executable(name: &str, creator: [u8; 4]) -> bool {
    if creator == *b"VIS3" {
        return true;
    }
    if name
        .rsplit_once('/')
        .map(|(parent, _)| {
            parent.split('/').any(|component| {
                executable_name_has_role(component, "update")
                    || executable_name_has_role(component, "updater")
            })
        })
        .unwrap_or(false)
    {
        // Updater payloads are commonly deltas that only become runnable
        // after being overlaid onto an existing installation. Do not let a
        // nested application from one outrank a complete installer payload.
        return true;
    }
    [
        "install",
        "installer",
        "setup",
        "update",
        "updater",
        "uninstall",
        "uninstaller",
    ]
    .into_iter()
    .any(|role| executable_name_has_role(name, role))
}

fn is_documentation_executable(name: &str) -> bool {
    let file_name = name.rsplit('/').next().unwrap_or(name).to_ascii_lowercase();
    let words = file_name
        .split(|character: char| !character.is_ascii_alphanumeric())
        .filter(|word| !word.is_empty())
        .collect::<Vec<_>>();
    words.iter().any(|word| {
        matches!(
            *word,
            "documentation" | "docs" | "help" | "manual" | "readme"
        )
    }) || words.windows(2).any(|words| words == ["read", "me"])
}

fn executable_name_has_role(name: &str, role: &str) -> bool {
    let file_name = name.rsplit('/').next().unwrap_or(name).to_ascii_lowercase();
    file_name
        .split(|character: char| !character.is_ascii_alphanumeric())
        .filter(|word| !word.is_empty())
        .any(|word| word == role)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ExecutableKind {
    Classic68k,
    PowerPcPef {
        architecture: [u8; 4],
        fragment_offset: u32,
        fragment_length: u32,
        app_stack_size: u32,
    },
}

#[derive(Clone, Debug)]
struct PpcDiagnosticVfs {
    volumes: Vec<PpcVfsVolumeRecord>,
    directories: Vec<PpcVfsDirectory>,
    files: Vec<PpcVfsFileRecord>,
    resource_files: Vec<PpcVfsResourceFileRecord>,
    resources: Vec<PpcVfsResourceRecord>,
    default_dir_id: u32,
    next_dir_id: u32,
}

#[derive(Debug, Clone)]
struct PpcCfmLibraryCandidate {
    path: String,
    fragment_index: usize,
    name: String,
    bytes: Vec<u8>,
}

/// Discover native CFM libraries from complete data/resource fork pairs.
///
/// A launch volume can contain shared libraries independently of the selected
/// application. The Code Fragment Manager identifies those libraries by the
/// logical name in their `'cfrg'` resource, not by their filesystem name. Keep
/// this scan independent of any application or archive layout and validate the
/// advertised flat data-fork range before handing bytes to the PEF loader.
fn discover_ppc_cfm_library_fragments(ppc_vfs: &PpcDiagnosticVfs) -> Vec<PpcCfmLibraryFragment> {
    let mut resource_files: Vec<_> = ppc_vfs.resource_files.iter().collect();
    resource_files.sort_by_key(|file| vfs_path_sort_key(&file.path));

    let mut candidates = Vec::new();
    for resource_file in resource_files {
        let Some(resource_data) = resource_file.raw_data.as_deref() else {
            continue;
        };
        let Some(data_file) = ppc_vfs
            .files
            .iter()
            .filter(|file| file.path.eq_ignore_ascii_case(&resource_file.path))
            .min_by(|left, right| {
                vfs_path_sort_key(&left.path).cmp(&vfs_path_sort_key(&right.path))
            })
        else {
            continue;
        };
        let Some(resource_fork) = ResourceFork::parse(resource_data) else {
            continue;
        };
        let Some(cfrg_resource) = resource_fork.get(*b"cfrg", 0) else {
            continue;
        };
        let Some(cfrg) = parse_cfrg_resource(&cfrg_resource.data) else {
            continue;
        };

        for (fragment_index, fragment) in cfrg.fragments.into_iter().enumerate() {
            if fragment.architecture != ARCH_POWERPC
                || fragment.usage != USAGE_LIB
                || fragment.location != LOCATION_ON_DISK_FLAT
                || fragment.name.is_empty()
            {
                continue;
            }
            let Some(range) = fragment.data_fork_range(data_file.data.len()) else {
                continue;
            };
            let Some(bytes) = data_file.data.get(range) else {
                continue;
            };
            let Some(pef_header) = parse_pef_header(bytes) else {
                continue;
            };
            if pef_header.architecture != ARCH_POWERPC {
                continue;
            }
            candidates.push(PpcCfmLibraryCandidate {
                path: resource_file.path.clone(),
                fragment_index,
                name: fragment.name,
                bytes: bytes.to_vec(),
            });
        }
    }

    candidates.sort_by(|left, right| {
        vfs_path_sort_key(&left.path)
            .cmp(&vfs_path_sort_key(&right.path))
            .then(left.fragment_index.cmp(&right.fragment_index))
            .then(
                left.name
                    .to_ascii_lowercase()
                    .cmp(&right.name.to_ascii_lowercase()),
            )
            .then(left.name.cmp(&right.name))
            .then(left.bytes.cmp(&right.bytes))
    });

    let mut fragments = Vec::new();
    for candidate in candidates {
        if fragments.iter().any(|fragment: &PpcCfmLibraryFragment| {
            fragment.name.eq_ignore_ascii_case(&candidate.name)
        }) {
            continue;
        }
        fragments.push(PpcCfmLibraryFragment {
            name: candidate.name,
            bytes: candidate.bytes,
        });
    }
    fragments
}

fn vfs_path_sort_key(path: &str) -> (String, String) {
    (path.to_ascii_lowercase(), path.to_string())
}

impl ExecutableKind {
    fn is_powerpc(self) -> bool {
        matches!(self, Self::PowerPcPef { .. })
    }
}

#[cfg(test)]
fn classify_executable(data: &[u8], rsrc: &[u8], is_appl: bool) -> Option<ExecutableKind> {
    classify_executable_with_preference(data, rsrc, is_appl, prefer_powerpc())
}

fn classify_executable_with_preference(
    data: &[u8],
    rsrc: &[u8],
    is_appl: bool,
    prefer_powerpc: bool,
) -> Option<ExecutableKind> {
    let fork = ResourceFork::parse(rsrc)?;
    if is_appl && prefer_powerpc {
        if let Some(cfrg_resource) = fork.get(*b"cfrg", 0) {
            if let Some(cfrg) = parse_cfrg_resource(&cfrg_resource.data) {
                if let Some((fragment, range)) =
                    select_powerpc_application_fragment(&cfrg, data.len())
                {
                    if let Some(header) = data.get(range.clone()).and_then(parse_pef_header) {
                        return Some(ExecutableKind::PowerPcPef {
                            architecture: header.architecture,
                            fragment_offset: range.start as u32,
                            fragment_length: (range.end - range.start) as u32,
                            app_stack_size: fragment.app_stack_size,
                        });
                    }
                }
            }
        }
    }

    if fork.get_code(0).is_some() {
        return Some(ExecutableKind::Classic68k);
    }

    if is_appl {
        let cfrg = parse_cfrg_resource(&fork.get(*b"cfrg", 0)?.data)?;
        let (fragment, range) = select_powerpc_application_fragment(&cfrg, data.len())?;
        let header = parse_pef_header(data.get(range.clone())?)?;
        return Some(ExecutableKind::PowerPcPef {
            architecture: header.architecture,
            fragment_offset: range.start as u32,
            fragment_length: (range.end - range.start) as u32,
            app_stack_size: fragment.app_stack_size,
        });
    }

    None
}

fn ppc_diagnostic_vfs(
    runner: &mut FixtureRunner,
    app_resource_path: Option<&str>,
) -> PpcDiagnosticVfs {
    runner.dispatcher_mut().ensure_vfs_catalog();
    let dispatcher = runner.dispatcher();
    let app_resource_path =
        app_resource_path.map(crate::trap::dispatch::TrapDispatcher::normalize_vfs_path);
    let mut directory_entries: Vec<_> = dispatcher.vfs_directory_paths.iter().collect();
    directory_entries.sort_by_key(|(dir_id, _)| **dir_id);

    let directories = directory_entries
        .into_iter()
        .filter_map(|(dir_id, _)| {
            let path = dispatcher.directory_path_for_id(*dir_id)?;
            let directory = dispatcher.directory_entry_for_id(*dir_id)?;
            Some(PpcVfsDirectory {
                dir_id: *dir_id,
                parent_dir_id: directory.parent_dir_id,
                path: path.to_string(),
                creator: 0,
                file_type: 0,
                finder_flags: 0,
                dirty: false,
            })
        })
        .collect();

    let mut volumes = dispatcher
        .vfs_volumes
        .values()
        .map(|volume| PpcVfsVolumeRecord {
            ref_num: volume.ref_num,
            name: volume.name.clone(),
            root_dir_id: volume.root_dir_id,
            attributes: volume.attributes,
            file_count: volume.file_count,
            allocation_block_count: volume.allocation_block_count,
            allocation_block_size: volume.allocation_block_size,
            clump_size: volume.clump_size,
            free_blocks: volume.free_blocks,
            bitmap_start: volume.bitmap_start,
            allocation_pointer: volume.allocation_pointer,
            allocation_start: volume.allocation_start,
            next_catalog_id: volume.next_catalog_id,
            created_date: volume.created_date,
            modified_date: volume.modified_date,
        })
        .collect::<Vec<_>>();
    volumes.sort_by_key(|volume| std::cmp::Reverse(volume.ref_num));

    let mut file_entries: Vec<_> = dispatcher.vfs.iter().collect();
    file_entries.sort_by_key(|(path, _)| *path);
    let mut files = file_entries
        .into_iter()
        .map(|(path, data)| {
            let metadata = dispatcher.vfs_metadata.get(path);
            PpcVfsFileRecord {
                path: path.clone(),
                data: (data.clone()).into(),
                creator: metadata.map_or(0, |value| value.creator),
                file_type: metadata.map_or(0, |value| value.file_type),
                finder_flags: metadata.map_or(0, |value| value.finder_flags),
                dirty: false,
            }
        })
        .collect::<Vec<_>>();

    let mut resource_entries: Vec<_> = dispatcher.vfs_rsrc.iter().collect();
    resource_entries.sort_by_key(|(path, _)| *path);
    let mut resource_files = Vec::new();
    let mut resources = Vec::new();
    for (path, rsrc_data) in resource_entries {
        let metadata = dispatcher.vfs_metadata.get(path);
        let creator = metadata.map_or(0, |value| value.creator);
        let file_type = metadata.map_or(0, |value| value.file_type);
        let parsed_fork = ResourceFork::parse(rsrc_data);
        resource_files.push(PpcVfsResourceFileRecord {
            path: path.clone(),
            creator,
            file_type,
            finder_flags: metadata.map_or(0, |value| value.finder_flags),
            resource_len: u32::try_from(rsrc_data.len()).unwrap_or(u32::MAX),
            raw_data: Some(rsrc_data.to_vec()),
            map_attrs: parsed_fork.as_ref().map_or(0, ResourceFork::map_attrs),
            dirty: false,
        });
        if !files
            .iter()
            .any(|file| file.path.eq_ignore_ascii_case(path))
        {
            files.push(PpcVfsFileRecord {
                path: path.clone(),
                data: Vec::new().into(),
                creator,
                file_type,
                finder_flags: metadata.map_or(0, |value| value.finder_flags),
                dirty: false,
            });
        }

        let seed_all_resources = app_resource_path
            .as_ref()
            .is_some_and(|app_path| app_path.eq_ignore_ascii_case(path));
        if let Some(fork) = parsed_fork.as_ref() {
            let mut sorted_resources: Vec<_> = fork.resources().values().collect();
            sorted_resources.sort_by_key(|resource| (resource.res_type, resource.id));
            for resource in sorted_resources {
                if !seed_all_resources && resource.res_type != *b"alis" {
                    continue;
                }
                resources.push(PpcVfsResourceRecord {
                    ref_num: 0,
                    path: path.clone(),
                    res_type: u32::from_be_bytes(resource.res_type),
                    res_id: resource.id,
                    name: resource.name_bytes.clone().unwrap_or_default(),
                    data: resource.data.clone(),
                    raw_data: resource.raw_data.clone(),
                    raw_attrs: resource.raw_attrs.map(u16::from),
                    attrs: u16::from(resource.attrs),
                    handle: 0,
                });
            }
        }
    }

    PpcDiagnosticVfs {
        volumes,
        directories,
        files,
        resource_files,
        resources,
        default_dir_id: dispatcher.default_dir_id,
        next_dir_id: dispatcher.next_vfs_dir_id,
    }
}

fn pef_fragment_data(data: &[u8], fragment_offset: u32, fragment_length: u32) -> Option<&[u8]> {
    let start = usize::try_from(fragment_offset).ok()?;
    let length = usize::try_from(fragment_length).ok()?;
    data.get(start..start.checked_add(length)?)
}

fn pef_diagnostic_details(
    data: &[u8],
    fragment_offset: u32,
    fragment_length: u32,
    app_stack_size: u32,
    ppc_vfs: Option<&PpcDiagnosticVfs>,
) -> Option<String> {
    let header = parse_pef_header(data)?;
    let mut details = vec![
        format!("architecture {}", fourcc_lossy(header.architecture)),
        format!("cfrg-offset=0x{fragment_offset:x}"),
        format!(
            "cfrg-length={}",
            if fragment_length == WHOLE_FORK {
                "whole-fork".to_string()
            } else {
                fragment_length.to_string()
            }
        ),
        format!("cfrg-stack={app_stack_size}"),
        format!("sections={}", header.section_count),
        format!("instantiated={}", header.instantiated_section_count),
    ];
    if let Some(loader) = parse_pef_loader_header(data) {
        details.push(format!(
            "imports={} libs / {} symbols",
            loader.imported_library_count, loader.total_imported_symbol_count
        ));
    }
    if let Some(entry) = resolve_pef_main_entry(data) {
        details.push(format!(
            "entry=pc 0x{:08x} rtoc 0x{:08x}",
            entry.entry_pc, entry.rtoc
        ));
    }
    if let Some(vfs) = ppc_vfs {
        details.push(format!(
            "vfs={} files / {} resources",
            vfs.files.len(),
            vfs.resources.len()
        ));
    }
    Some(details.join(", "))
}

fn fourcc_lossy(value: [u8; 4]) -> String {
    value
        .iter()
        .map(|&byte| {
            if byte.is_ascii_graphic() || byte == b' ' {
                char::from(byte)
            } else {
                '.'
            }
        })
        .collect()
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

    #[test]
    fn frontend_runner_constructors_preserve_defaults_and_explicit_depths() {
        let default_runner = new_runner();
        assert_eq!(default_runner.configured_screen_depth(), 8);
        assert_eq!(default_runner.configured_powerpc_screen_depth(), 16);

        let addressed_default = new_runner_with_addressing(false);
        assert_eq!(addressed_default.configured_screen_depth(), 8);
        assert_eq!(addressed_default.configured_powerpc_screen_depth(), 16);
        assert!(!addressed_default.bus().addressing_32_bit());

        for depth in [1, 2, 4, 8] {
            let runner = new_runner_with_screen_depth(depth);
            assert_eq!(runner.configured_screen_depth(), depth);
            assert_eq!(runner.configured_powerpc_screen_depth(), u32::from(depth));
        }
    }

    #[test]
    fn parallel_fork_decode_matches_the_sequential_loop() {
        // Build a real SIT-5 archive whose forks exercise compressed and
        // stored entries of assorted sizes; the parallel decode must return
        // exactly what per-entry sequential decompression returns, in entry
        // order, for every worker count the machine picks.
        let mut archive = SitArchive::new();
        for index in 0..17u32 {
            let data: Vec<u8> = (0..(index * 977))
                .map(|byte| (byte * 31 + index) as u8)
                .collect();
            let rsrc: Vec<u8> = (0..(index * 61)).map(|byte| (byte ^ index) as u8).collect();
            archive.add_entry(SitEntry {
                name: format!("file-{index}"),
                data_fork: data,
                resource_fork: rsrc,
                file_type: *b"TEXT",
                creator: *b"ttxt",
                is_folder: false,
                data_method: 0,
                rsrc_method: 0,
                data_ulen: 0,
                rsrc_ulen: 0,
                finder_flags: 0,
                is_compressed: false,
                format: stuffit::ArchiveFormat::Sit5,
            });
        }
        let bytes = archive
            .serialize_compressed()
            .expect("serialize SIT-13 test archive");
        let parsed = SitArchive::parse(&bytes).expect("re-parse test archive");
        let entries: Vec<&SitEntry> = parsed
            .entries
            .iter()
            .filter(|entry| !entry.is_folder)
            .collect();
        assert_eq!(entries.len(), 17);
        assert!(
            entries.iter().any(|entry| entry.is_compressed),
            "test archive must exercise real decompression"
        );

        let parallel = decompress_file_entries(&entries);
        for (entry, decoded) in entries.iter().zip(parallel) {
            let sequential = entry.decompressed_forks().expect("sequential decode");
            assert_eq!(
                decoded.expect("parallel decode"),
                sequential,
                "entry {} must decode identically",
                entry.name
            );
        }
    }

    #[test]
    fn disk_image_payload_registers_a_hardware_locked_file_manager_volume() {
        let mut builder = hfsplus::testutil::HfsPlusImageBuilder::new();
        builder.add_file("Data File", b"contents", 0o100644);
        let image_bytes = builder.build();
        let image = crate::disk_image::extract_dc42_or_hfs(&image_bytes)
            .unwrap()
            .expect("HFS+ image");
        assert_eq!(image.volume_info.attributes & 0x0080, 0);
        let volume_name = image.volume_name.clone();
        let mut runner = new_runner();
        let mut executable = None;

        insert_payload_into_vfs(
            &mut runner,
            payload_from_disk_image(image, 1).unwrap(),
            &mut executable,
        );

        let volume = runner
            .dispatcher()
            .vfs_volume_by_name(&volume_name)
            .expect("mounted File Manager volume");
        assert_eq!(volume.ref_num, -2);
        assert_ne!(volume.attributes & 0x0080, 0, "hardware-locked volume");
        assert_eq!(
            volume.attributes & 0x8000,
            0,
            "software lock must retain the source state"
        );
        assert!(runner
            .dispatcher()
            .vfs_path_is_read_only(&format!("{volume_name}/Data File")));
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

    fn make_versioned_code_resource_fork(version: [u8; 4]) -> Vec<u8> {
        crate::managers::resource::serialize_resource_fork(&[
            crate::managers::resource::ResourceForkEntry {
                res_type: *b"CODE",
                id: 0,
                name: Vec::new(),
                data: vec![0; 128],
                attrs: 0,
            },
            crate::managers::resource::ResourceForkEntry {
                res_type: *b"vers",
                id: 1,
                name: Vec::new(),
                data: version.to_vec(),
                attrs: 0,
            },
        ])
        .expect("serialize versioned application resource fork")
    }

    fn make_fat_application_resource_fork(cfrg: Vec<u8>) -> Vec<u8> {
        crate::managers::resource::serialize_resource_fork(&[
            crate::managers::resource::ResourceForkEntry {
                res_type: *b"CODE",
                id: 0,
                name: Vec::new(),
                data: vec![0; 128],
                attrs: 0,
            },
            crate::managers::resource::ResourceForkEntry {
                res_type: *b"cfrg",
                id: 0,
                name: Vec::new(),
                data: cfrg,
                attrs: 0,
            },
        ])
        .expect("serialize fat application resource fork")
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

    fn make_zip(entries: &[(&str, &[u8])]) -> Vec<u8> {
        let cursor = Cursor::new(Vec::new());
        let mut writer = zip::ZipWriter::new(cursor);
        let options = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Deflated);
        for (name, data) in entries {
            writer.start_file(name, options).unwrap();
            writer.write_all(data).unwrap();
        }
        writer.finish().unwrap().into_inner()
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
            1,
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
            &[],
            &manual_rsrc,
            true,
            0,
            *b"????",
            1,
        );
        maybe_select_executable(
            &mut selected,
            "Sample App/Sample Runtime",
            &[0],
            &app_rsrc,
            true,
            322_352,
            *b"????",
            1,
        );

        let selected = selected.expect("expected an executable candidate");
        assert_eq!(selected.name, "Sample App/Sample Runtime");
    }

    #[test]
    fn executable_selection_prefers_demo_game_over_documentation_app() {
        let game_rsrc = make_single_resource_fork_bytes(*b"CODE", 0, &[0; 128]);
        let docs_rsrc = make_single_resource_fork_bytes(*b"CODE", 0, &[0; 256]);

        for docs_first in [false, true] {
            let mut selected = None;
            let mut candidates = [
                ("Game Folder/Game Demo", &game_rsrc, 300_000usize),
                (
                    "Game Folder/Game Demo Documentation",
                    &docs_rsrc,
                    600_000usize,
                ),
            ];
            if docs_first {
                candidates.reverse();
            }
            for (name, rsrc, data_len) in candidates {
                maybe_select_executable(
                    &mut selected,
                    name,
                    &[0],
                    rsrc,
                    true,
                    data_len,
                    *b"GAME",
                    1,
                );
            }

            assert_eq!(
                selected.expect("expected an executable candidate").name,
                "Game Folder/Game Demo"
            );
        }
    }

    #[test]
    fn executable_selection_prefers_user_app_over_system_folder_utility() {
        let utility_rsrc = make_single_resource_fork_bytes(*b"CODE", 0, &[0; 256]);
        let game_rsrc = make_single_resource_fork_bytes(*b"CODE", 0, &[0; 128]);
        let mut selected = None;

        maybe_select_executable(
            &mut selected,
            "Demo Disk/System Folder/Apple Menu Items/Stickies",
            &[0],
            &utility_rsrc,
            true,
            38,
            *b"notz",
            1,
        );
        maybe_select_executable(
            &mut selected,
            "Demo Disk/Pathways into Darkness",
            &[],
            &game_rsrc,
            true,
            0,
            *b"p.th",
            1,
        );

        let selected = selected.expect("expected an executable candidate");
        assert_eq!(selected.name, "Demo Disk/Pathways into Darkness");
    }

    #[test]
    fn executable_selection_prefers_exact_override_over_larger_substring_match() {
        let app_rsrc = make_single_resource_fork_bytes(*b"CODE", 0, &[0; 128]);
        let installer_rsrc = make_single_resource_fork_bytes(*b"CODE", 0, &[0; 256]);
        let mut selected = None;

        maybe_select_executable_with_override(
            &mut selected,
            "DOOM II/DOOM II",
            &[0],
            &app_rsrc,
            true,
            422_288,
            *b"????",
            1,
            Some("DOOM II/DOOM II"),
        );
        maybe_select_executable_with_override(
            &mut selected,
            "DOOM II/DOOM II Installer",
            &[0],
            &installer_rsrc,
            true,
            9_871_672,
            *b"????",
            1,
            Some("DOOM II/DOOM II"),
        );

        let selected = selected.expect("expected an executable candidate");
        assert_eq!(selected.name, "DOOM II/DOOM II");
    }

    #[test]
    fn executable_selection_prefers_newer_version_of_same_application_family() {
        let full_rsrc = make_versioned_code_resource_fork([0x01, 0x00, 0x80, 0x00]);
        let demo_rsrc = make_versioned_code_resource_fork([0x01, 0x20, 0x80, 0x00]);

        for newest_first in [false, true] {
            let mut selected = None;
            let mut candidates = [
                ("Collection/Product", &full_rsrc, 900_000usize),
                ("Collection/Product Demo", &demo_rsrc, 100_000usize),
            ];
            if newest_first {
                candidates.reverse();
            }

            for (name, rsrc, data_len) in candidates {
                maybe_select_executable(
                    &mut selected,
                    name,
                    &[0],
                    rsrc,
                    true,
                    data_len,
                    *b"GAME",
                    1,
                );
            }

            assert_eq!(
                selected.expect("expected an executable candidate").name,
                "Collection/Product Demo"
            );
        }
    }

    #[test]
    fn executable_selection_does_not_launch_a_nested_updater_payload() {
        let version = [0x01, 0x20, 0x80, 0x00];
        let demo_rsrc = make_versioned_code_resource_fork(version);
        let updater_rsrc = make_versioned_code_resource_fork(version);
        let mut selected = None;

        maybe_select_executable(
            &mut selected,
            "Gridz 1.2/Gridz 1.2 Demo Installer/Gridz Demo ƒ/Gridz™ Demo",
            &[0],
            &demo_rsrc,
            true,
            687_530,
            *b"Grdz",
            1,
        );
        maybe_select_executable(
            &mut selected,
            "Gridz 1.2/Gridz 1.2 Updater/Gridz™",
            &[0],
            &updater_rsrc,
            true,
            697_464,
            *b"Grdz",
            1,
        );

        assert_eq!(
            selected.expect("expected an executable candidate").name,
            "Gridz 1.2/Gridz 1.2 Demo Installer/Gridz Demo ƒ/Gridz™ Demo"
        );
    }

    #[test]
    fn executable_selection_does_not_compare_versions_across_creators() {
        let full_rsrc = make_versioned_code_resource_fork([0x01, 0x00, 0x80, 0x00]);
        let demo_rsrc = make_versioned_code_resource_fork([0x09, 0x00, 0x80, 0x00]);
        let mut selected = None;

        maybe_select_executable(
            &mut selected,
            "Collection/Product",
            &[0],
            &full_rsrc,
            true,
            900_000,
            *b"FULL",
            1,
        );
        maybe_select_executable(
            &mut selected,
            "Collection/Product Demo",
            &[0],
            &demo_rsrc,
            true,
            100_000,
            *b"DEMO",
            1,
        );

        assert_eq!(
            selected.expect("expected an executable candidate").name,
            "Collection/Product"
        );
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
    fn macbinary_application_decodes_mac_roman_filename() {
        let data = b"demo data";
        let rsrc = make_single_resource_fork_bytes(*b"CODE", 0, &[0; 128]);
        let mut macbinary = make_macbinary_application("Gridz Demo", data, &rsrc);
        macbinary[1] = 11;
        macbinary[2..13].copy_from_slice(b"Gridz\xAA Demo");
        let mut runner = new_runner();

        load_macbinary(&mut runner, &macbinary).expect("MacBinary application should load");

        assert_eq!(
            runner.dispatcher().vfs.get("Gridz™ Demo"),
            Some(&data.to_vec())
        );
    }

    #[test]
    fn zip_archive_mounts_macbinary_application_and_companion_files() {
        let app_data = b"self-readable application data";
        let app_rsrc = make_single_resource_fork_bytes(*b"CODE", 0, &[0; 128]);
        let macbinary = make_macbinary_application("Game App", app_data, &app_rsrc);
        let zip = make_zip(&[
            ("Demo/Game App.bin", &macbinary),
            ("Demo/GAME.000", b"runtime companion"),
        ]);
        let mut runner = new_runner();

        load_zip(&mut runner, &zip).expect("ZIP application should load");

        assert_eq!(
            runner.dispatcher().vfs.get("Demo/Game App"),
            Some(&app_data.to_vec())
        );
        assert_eq!(
            runner.dispatcher().vfs_rsrc.get("Demo/Game App"),
            Some(&app_rsrc)
        );
        assert_eq!(
            runner.dispatcher().vfs.get("Demo/GAME.000"),
            Some(&b"runtime companion".to_vec())
        );
        let metadata = runner
            .dispatcher_mut()
            .vfs_file_metadata("Demo/Game App")
            .expect("application metadata");
        assert_eq!(metadata.file_type, u32::from_be_bytes(*b"APPL"));
        assert_eq!(metadata.creator, u32::from_be_bytes(*b"TEST"));
    }

    #[test]
    fn zip_archive_rejects_parent_directory_entries() {
        let zip = make_zip(&[("../escape.dat", b"nope")]);
        let err = collect_zip_payload(&zip).expect_err("path traversal should be rejected");
        assert!(err.contains("Unsafe ZIP entry path"), "{err}");
    }

    #[test]
    fn zip_archive_rejects_excessive_entry_counts() {
        let cursor = Cursor::new(Vec::new());
        let mut writer = zip::ZipWriter::new(cursor);
        let options = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Stored);
        for index in 0..=MAX_ZIP_ENTRIES {
            writer
                .start_file(format!("entry-{index}"), options)
                .unwrap();
        }
        let zip = writer.finish().unwrap().into_inner();

        let err = collect_zip_payload(&zip).expect_err("entry limit should be enforced");
        assert!(err.contains("entry limit"), "{err}");
    }

    #[test]
    fn zip_archive_preserves_nested_disk_image_volume_metadata() {
        let mut builder = hfsplus::testutil::HfsPlusImageBuilder::new();
        builder.add_file("Data File", b"contents", 0o100644);
        let image = builder.build();
        let volume_name = crate::disk_image::extract_dc42_or_hfs(&image)
            .unwrap()
            .expect("HFS+ image")
            .volume_name;
        let zip = make_zip(&[("Demo/Data.img", &image)]);

        let payload = collect_zip_payload(&zip).expect("nested disk image should load");

        assert_eq!(payload.volumes.len(), 1);
        assert_eq!(payload.volumes[0].0, volume_name);
    }

    #[test]
    fn zip_archive_rejects_malformed_input() {
        let err = collect_zip_payload(b"PK\x03\x04not a zip")
            .expect_err("malformed ZIP should be rejected");
        assert!(err.contains("Failed to parse ZIP archive"), "{err}");
    }

    #[test]
    fn web_pack_sources_accept_zip_archives() {
        let zip = make_zip(&[("Demo/GAME.000", b"runtime companion")]);
        let packed = pack_game_sources_for_web(&[&zip], &[]).expect("ZIP source should pack");

        assert_eq!(&packed[..4], WEB_PACK_MAGIC);
        assert!(packed
            .windows(b"Demo/GAME.000".len())
            .any(|window| window == b"Demo/GAME.000"));
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
            kind: ExecutableKind::Classic68k,
            is_appl: true,
            has_data_fork: true,
            score: 128,
            priority: 1,
            creator: *b"ABCD",
            is_installer: false,
            is_documentation: false,
            is_demo: false,
            version: None,
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
            kind: ExecutableKind::Classic68k,
            is_appl: true,
            has_data_fork: true,
            score: 128,
            priority: 1,
            creator: *b"ABCD",
            is_installer: false,
            is_documentation: false,
            is_demo: false,
            version: None,
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
            kind: ExecutableKind::Classic68k,
            is_appl: true,
            has_data_fork: true,
            score: 128,
            priority: 1,
            creator: *b"ABCD",
            is_installer: false,
            is_documentation: false,
            is_demo: false,
            version: None,
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

    fn make_minimal_pef(architecture: [u8; 4]) -> Vec<u8> {
        let mut bytes = vec![0u8; 40];
        bytes[0..4].copy_from_slice(b"Joy!");
        bytes[4..8].copy_from_slice(b"peff");
        bytes[8..12].copy_from_slice(&architecture);
        bytes
    }

    fn make_library_cfrg(
        architecture: [u8; 4],
        usage: u8,
        location: u8,
        fragment_offset: u32,
        fragment_length: u32,
        name: &[u8],
    ) -> Vec<u8> {
        assert!(name.len() <= u8::MAX as usize);
        let name_record = [vec![name.len() as u8], name.to_vec()].concat();
        let record_len = 42 + name_record.len();
        let mut bytes = vec![0u8; 32 + record_len];
        bytes[8..12].copy_from_slice(&1u32.to_be_bytes());
        bytes[28..32].copy_from_slice(&1u32.to_be_bytes());

        let record = 32;
        bytes[record..record + 4].copy_from_slice(&architecture);
        bytes[record + 22] = usage;
        bytes[record + 23] = location;
        bytes[record + 24..record + 28].copy_from_slice(&fragment_offset.to_be_bytes());
        bytes[record + 28..record + 32].copy_from_slice(&fragment_length.to_be_bytes());
        bytes[record + 40..record + 42].copy_from_slice(&(record_len as u16).to_be_bytes());
        bytes[record + 42..].copy_from_slice(&name_record);
        bytes
    }

    fn make_ppc_diagnostic_vfs(
        entries: Vec<(&str, Option<Vec<u8>>, Option<Vec<u8>>)>,
    ) -> PpcDiagnosticVfs {
        let mut files = Vec::new();
        let mut resource_files = Vec::new();
        for (path, data, rsrc) in entries {
            if let Some(data) = data {
                files.push(PpcVfsFileRecord {
                    path: path.to_string(),
                    data: data.into(),
                    creator: 0,
                    file_type: 0,
                    finder_flags: 0,
                    dirty: false,
                });
            }
            if let Some(rsrc) = rsrc {
                resource_files.push(PpcVfsResourceFileRecord {
                    path: path.to_string(),
                    creator: 0,
                    file_type: 0,
                    finder_flags: 0,
                    resource_len: rsrc.len() as u32,
                    raw_data: Some(rsrc),
                    map_attrs: 0,
                    dirty: false,
                });
            }
        }
        PpcDiagnosticVfs {
            volumes: Vec::new(),
            directories: Vec::new(),
            files,
            resource_files,
            resources: Vec::new(),
            default_dir_id: 0,
            next_dir_id: 1,
        }
    }

    fn make_cfrg(fragment_offset: u32, fragment_length: u32) -> Vec<u8> {
        make_cfrg_with_stack(fragment_offset, fragment_length, 0)
    }

    fn make_cfrg_with_stack(
        fragment_offset: u32,
        fragment_length: u32,
        app_stack_size: u32,
    ) -> Vec<u8> {
        let name = b"\x09Test App\xAA";
        let record_len = 42 + name.len();
        let mut bytes = vec![0u8; 32 + record_len];
        bytes[8..12].copy_from_slice(&1u32.to_be_bytes());
        bytes[28..32].copy_from_slice(&1u32.to_be_bytes());

        let record = 32;
        bytes[record..record + 4].copy_from_slice(b"pwpc");
        bytes[record + 16..record + 20].copy_from_slice(&app_stack_size.to_be_bytes());
        bytes[record + 20..record + 22].copy_from_slice(&0u16.to_be_bytes());
        bytes[record + 22] = 1; // kIsApp
        bytes[record + 23] = 1; // kOnDiskFlat
        bytes[record + 24..record + 28].copy_from_slice(&fragment_offset.to_be_bytes());
        bytes[record + 28..record + 32].copy_from_slice(&fragment_length.to_be_bytes());
        bytes[record + 40..record + 42].copy_from_slice(&(record_len as u16).to_be_bytes());
        bytes[record + 42..].copy_from_slice(name);
        bytes
    }

    #[test]
    fn ppc_cfm_discovery_pairs_forks_case_insensitively_and_uses_logical_name() {
        let pef = make_minimal_pef(*b"pwpc");
        let cfrg = make_library_cfrg(
            ARCH_POWERPC,
            USAGE_LIB,
            LOCATION_ON_DISK_FLAT,
            0,
            WHOLE_FORK,
            b"SDL",
        );
        let vfs = PpcDiagnosticVfs {
            volumes: Vec::new(),
            directories: Vec::new(),
            files: vec![PpcVfsFileRecord {
                path: "Volume/SDL".to_string(),
                data: (pef.clone()).into(),
                creator: 0,
                file_type: 0,
                finder_flags: 0,
                dirty: false,
            }],
            resource_files: vec![PpcVfsResourceFileRecord {
                path: "volume/sdl".to_string(),
                creator: 0,
                file_type: 0,
                finder_flags: 0,
                resource_len: 0,
                raw_data: Some(make_single_resource_fork_bytes(*b"cfrg", 0, &cfrg)),
                map_attrs: 0,
                dirty: false,
            }],
            resources: Vec::new(),
            default_dir_id: 0,
            next_dir_id: 1,
        };

        let fragments = discover_ppc_cfm_library_fragments(&vfs);
        assert_eq!(
            fragments,
            vec![PpcCfmLibraryFragment {
                name: "SDL".to_string(),
                bytes: pef,
            }]
        );
    }

    #[test]
    fn ppc_cfm_discovery_rejects_incomplete_invalid_and_non_library_pairs() {
        let valid_pef = make_minimal_pef(*b"pwpc");
        let valid_library = |name: &[u8]| {
            make_single_resource_fork_bytes(
                *b"cfrg",
                0,
                &make_library_cfrg(
                    ARCH_POWERPC,
                    USAGE_LIB,
                    LOCATION_ON_DISK_FLAT,
                    0,
                    WHOLE_FORK,
                    name,
                ),
            )
        };
        let invalid_library = |architecture, usage, location, offset, length| {
            make_single_resource_fork_bytes(
                *b"cfrg",
                0,
                &make_library_cfrg(architecture, usage, location, offset, length, b"Rejected"),
            )
        };

        let vfs = make_ppc_diagnostic_vfs(vec![
            (
                "Valid",
                Some(valid_pef.clone()),
                Some(valid_library(b"Accepted")),
            ),
            (
                "BadArchitecture",
                Some(make_minimal_pef(*b"m68k")),
                Some(valid_library(b"Wrong PEF")),
            ),
            (
                "BadRange",
                Some(valid_pef.clone()),
                Some(invalid_library(
                    ARCH_POWERPC,
                    USAGE_LIB,
                    LOCATION_ON_DISK_FLAT,
                    100,
                    40,
                )),
            ),
            (
                "EmptyName",
                Some(valid_pef.clone()),
                Some(valid_library(b"")),
            ),
            (
                "NotLibrary",
                Some(valid_pef.clone()),
                Some(invalid_library(
                    ARCH_POWERPC,
                    1,
                    LOCATION_ON_DISK_FLAT,
                    0,
                    WHOLE_FORK,
                )),
            ),
            (
                "NotFlat",
                Some(valid_pef.clone()),
                Some(invalid_library(ARCH_POWERPC, USAGE_LIB, 2, 0, WHOLE_FORK)),
            ),
            ("NoDataFork", None, Some(valid_library(b"No Data"))),
            ("NoResourceFork", Some(valid_pef), None),
        ]);

        let fragments = discover_ppc_cfm_library_fragments(&vfs);
        assert_eq!(fragments.len(), 1);
        assert_eq!(fragments[0].name, "Accepted");
    }

    #[test]
    fn ppc_cfm_discovery_resolves_duplicate_logical_names_deterministically() {
        let mut first_pef = make_minimal_pef(*b"pwpc");
        first_pef[39] = 0x11;
        let mut second_pef = make_minimal_pef(*b"pwpc");
        second_pef[39] = 0x22;
        let vfs = make_ppc_diagnostic_vfs(vec![
            (
                "z-library",
                Some(second_pef),
                Some(make_single_resource_fork_bytes(
                    *b"cfrg",
                    0,
                    &make_library_cfrg(
                        ARCH_POWERPC,
                        USAGE_LIB,
                        LOCATION_ON_DISK_FLAT,
                        0,
                        WHOLE_FORK,
                        b"sdl",
                    ),
                )),
            ),
            (
                "A-library",
                Some(first_pef.clone()),
                Some(make_single_resource_fork_bytes(
                    *b"cfrg",
                    0,
                    &make_library_cfrg(
                        ARCH_POWERPC,
                        USAGE_LIB,
                        LOCATION_ON_DISK_FLAT,
                        0,
                        WHOLE_FORK,
                        b"SDL",
                    ),
                )),
            ),
        ]);

        let fragments = discover_ppc_cfm_library_fragments(&vfs);
        assert_eq!(fragments.len(), 1);
        assert_eq!(fragments[0].name, "SDL");
        assert_eq!(fragments[0].bytes, first_pef);
    }

    #[test]
    fn executable_selection_recognizes_powerpc_pef_apps_with_cfrg() {
        let cfrg_rsrc = make_single_resource_fork_bytes(*b"cfrg", 0, &make_cfrg(0, 0));
        let ppc_data = make_minimal_pef(*b"pwpc");

        assert_eq!(
            classify_executable(&ppc_data, &cfrg_rsrc, true),
            Some(ExecutableKind::PowerPcPef {
                architecture: *b"pwpc",
                fragment_offset: 0,
                fragment_length: ppc_data.len() as u32,
                app_stack_size: 0,
            })
        );
    }

    #[test]
    fn executable_selection_prefers_classic_slice_for_fat_apps() {
        let rsrc = make_fat_application_resource_fork(make_cfrg(0, 0));
        let ppc_data = make_minimal_pef(*b"pwpc");

        assert_eq!(
            classify_executable_with_preference(&ppc_data, &rsrc, true, false),
            Some(ExecutableKind::Classic68k)
        );
    }

    #[test]
    fn executable_selection_can_force_powerpc_slice_for_fat_apps() {
        let rsrc = make_fat_application_resource_fork(make_cfrg(0, 0));
        let ppc_data = make_minimal_pef(*b"pwpc");

        assert_eq!(
            classify_executable_with_preference(&ppc_data, &rsrc, true, true),
            Some(ExecutableKind::PowerPcPef {
                architecture: *b"pwpc",
                fragment_offset: 0,
                fragment_length: ppc_data.len() as u32,
                app_stack_size: 0,
            })
        );
    }

    #[test]
    fn executable_candidate_selection_honors_powerpc_preference() {
        let rsrc = make_fat_application_resource_fork(make_cfrg(0, 0));
        let ppc_data = make_minimal_pef(*b"pwpc");
        let mut selected = None;

        maybe_select_executable_with_preference(
            &mut selected,
            "Fat Application",
            &ppc_data,
            &rsrc,
            true,
            ppc_data.len(),
            *b"TEST",
            1,
            true,
        );

        assert!(matches!(
            selected.map(|candidate| candidate.kind),
            Some(ExecutableKind::PowerPcPef { .. })
        ));
    }

    #[test]
    fn executable_selection_uses_nonzero_cfrg_data_fork_offset() {
        let cfrg_rsrc = make_single_resource_fork_bytes(*b"cfrg", 0, &make_cfrg(8, 40));
        let mut ppc_data = b"metadata".to_vec();
        ppc_data.extend_from_slice(&make_minimal_pef(*b"pwpc"));

        assert_eq!(
            classify_executable(&ppc_data, &cfrg_rsrc, true),
            Some(ExecutableKind::PowerPcPef {
                architecture: *b"pwpc",
                fragment_offset: 8,
                fragment_length: 40,
                app_stack_size: 0,
            })
        );
    }

    #[test]
    fn executable_selection_carries_cfrg_application_stack_size() {
        let cfrg = make_cfrg_with_stack(0, 0, 0x32000);
        let cfrg_rsrc = make_single_resource_fork_bytes(*b"cfrg", 0, &cfrg);
        let ppc_data = make_minimal_pef(*b"pwpc");

        assert_eq!(
            classify_executable(&ppc_data, &cfrg_rsrc, true),
            Some(ExecutableKind::PowerPcPef {
                architecture: *b"pwpc",
                fragment_offset: 0,
                fragment_length: ppc_data.len() as u32,
                app_stack_size: 0x32000,
            })
        );
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

        let expanded = expand_squz_payload_file("AllSounds1.rsrc", &file, *b"SQUZ", *b"BrSq", 0, 1)
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

        let expanded = expand_squz_payload_file("Activity.rsrc", &file, *b"SQUZ", *b"BrSq", 0, 1)
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

        let expanded =
            expand_squz_payload_file("Document Scrapbook", &file, *b"SQUZ", *b"BrSq", 0, 1)
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

        let expanded = expand_squz_payload_file("Activity.rsrc", &file, *b"SQUZ", *b"BrSq", 0, 1)
            .unwrap()
            .unwrap();

        assert!(expanded.data.is_empty());
        assert!(ResourceFork::parse(&expanded.rsrc).is_some());
    }
}
