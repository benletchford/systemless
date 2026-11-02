use std::collections::{HashMap, HashSet};
use std::ffi::OsStr;
use std::fs;
use std::io::{self, Read};

use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use systemless::systems::macintosh::runner::{
    FixtureRunner, VfsFileSnapshot, VfsFileStat, VfsFileSummary,
};

const SAVE_SCAN_FRAME_INTERVAL: u8 = 30;
const METADATA_FILE: &str = "metadata.json";
const DATA_FORK_FILE: &str = "data.fork";
const RESOURCE_FORK_FILE: &str = "resource.fork";

#[derive(Debug)]
pub struct DesktopSaveStore {
    root: PathBuf,
    archive_vfs_stats: HashMap<String, VfsFileStat>,
    last_vfs_fingerprints: HashMap<String, SaveFingerprint>,
    persisted_save_paths: HashSet<String>,
    save_scan_frame: u8,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct SaveFingerprint {
    data_len: usize,
    resource_len: usize,
    data_hash: u64,
    resource_hash: u64,
    file_type: u32,
    creator: u32,
    finder_flags: u16,
    modified_date: u32,
}

impl From<&VfsFileSummary> for SaveFingerprint {
    fn from(summary: &VfsFileSummary) -> Self {
        Self {
            data_len: summary.data_len,
            resource_len: summary.resource_len,
            data_hash: summary.data_hash,
            resource_hash: summary.resource_hash,
            file_type: summary.file_type,
            creator: summary.creator,
            finder_flags: summary.finder_flags,
            modified_date: summary.modified_date,
        }
    }
}

impl From<&VfsFileSnapshot> for SaveFingerprint {
    fn from(snapshot: &VfsFileSnapshot) -> Self {
        Self {
            data_len: snapshot.data_fork.len(),
            resource_len: snapshot.resource_fork.len(),
            data_hash: save_fork_hash(&snapshot.data_fork),
            resource_hash: save_fork_hash(&snapshot.resource_fork),
            file_type: snapshot.file_type,
            creator: snapshot.creator,
            finder_flags: snapshot.finder_flags,
            modified_date: snapshot.modified_date,
        }
    }
}

#[derive(Debug, Deserialize, Serialize)]
struct StoredSaveMetadata {
    version: u8,
    path: String,
    file_type: u32,
    creator: u32,
    finder_flags: u16,
    created_date: u32,
    modified_date: u32,
}

impl DesktopSaveStore {
    pub fn root_for_game_path(game_path: &Path) -> io::Result<PathBuf> {
        Ok(application_data_dir()?
            .join("saves")
            .join(archive_digest(game_path)?))
    }

    pub fn for_loaded_archive(game_path: &Path, runner: &mut FixtureRunner) -> io::Result<Self> {
        Self::for_loaded_archive_in(game_path, &application_data_dir()?, runner)
    }

    pub(super) fn for_loaded_archive_in(
        game_path: &Path,
        data_dir: &Path,
        runner: &mut FixtureRunner,
    ) -> io::Result<Self> {
        Ok(Self::at_root(
            prepare_store(game_path, data_dir, None)?,
            vfs_stats(runner),
        ))
    }

    fn at_root(root: PathBuf, archive_vfs_stats: HashMap<String, VfsFileStat>) -> Self {
        Self {
            root,
            archive_vfs_stats,
            last_vfs_fingerprints: HashMap::new(),
            persisted_save_paths: HashSet::new(),
            save_scan_frame: 0,
        }
    }

    /// Locate and prepare the store without launching a guest. An explicit
    /// import fails on conflicting guest paths and never alters its source.
    pub fn locate(game_path: &Path, import: Option<&Path>) -> io::Result<PathBuf> {
        prepare_store(game_path, &application_data_dir()?, import)
    }

    /// Delete persisted preferences after migrating any discoverable legacy
    /// store, so the next launch cannot reintroduce the old preferences.
    pub fn reset_preferences(game_path: &Path) -> io::Result<usize> {
        reset_preferences_in_root(&Self::locate(game_path, None)?)
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn load_saved_files(&mut self) -> Vec<VfsFileSnapshot> {
        let files = match load_saved_files_from_root(&self.root) {
            Ok(files) => files,
            Err(err) => {
                eprintln!(
                    "[SYSTEMLESS] Could not load desktop saves from {}: {}",
                    self.root.display(),
                    err
                );
                Vec::new()
            }
        };
        self.persisted_save_paths = files.iter().map(|file| file.path.clone()).collect();
        self.last_vfs_fingerprints = files
            .iter()
            .map(|file| (file.path.clone(), SaveFingerprint::from(file)))
            .collect();
        files
    }

    pub fn sync_save_files(&mut self, runner: &mut FixtureRunner) {
        self.save_scan_frame = self.save_scan_frame.wrapping_add(1);
        if self.save_scan_frame % SAVE_SCAN_FRAME_INTERVAL != 0 {
            return;
        }
        self.sync_save_files_now(runner);
    }

    pub fn sync_save_files_now(&mut self, runner: &mut FixtureRunner) {
        let stats = runner.vfs_file_stats_where(is_user_save_path);
        let mut next_fingerprints = HashMap::new();
        let mut next_persisted_paths = HashSet::new();

        for stat in stats {
            if !self.persisted_save_paths.contains(&stat.path)
                && self
                    .archive_vfs_stats
                    .get(&stat.path)
                    .is_some_and(|archive| vfs_stats_match(archive, &stat))
            {
                continue;
            }

            let Some(summary) = runner.vfs_file_summary(&stat.path) else {
                continue;
            };
            let fingerprint = SaveFingerprint::from(&summary);
            next_fingerprints.insert(summary.path.clone(), fingerprint.clone());

            let Some(snapshot) = runner.vfs_file_snapshot(&summary.path) else {
                continue;
            };
            if self.last_vfs_fingerprints.get(&summary.path) != Some(&fingerprint)
                || !self.persisted_save_paths.contains(&summary.path)
            {
                match self.persist_save_file(&snapshot) {
                    Ok(()) => {
                        eprintln!(
                            "[SYSTEMLESS] Saved desktop file: {}",
                            self.save_dir_for_vfs_path(&snapshot.path).display()
                        );
                        next_persisted_paths.insert(summary.path.clone());
                    }
                    Err(err) => {
                        eprintln!(
                            "[SYSTEMLESS] Could not persist desktop save {}: {}",
                            snapshot.path, err
                        );
                        next_fingerprints.remove(&summary.path);
                        if self.persisted_save_paths.contains(&summary.path) {
                            next_persisted_paths.insert(summary.path.clone());
                        }
                    }
                }
            } else {
                next_persisted_paths.insert(summary.path.clone());
            }
        }

        let stale_paths = self
            .persisted_save_paths
            .difference(&next_persisted_paths)
            .cloned()
            .collect::<Vec<_>>();
        for path in stale_paths {
            if let Err(err) = self.delete_save_file(&path) {
                eprintln!(
                    "[SYSTEMLESS] Could not remove desktop save {}: {}",
                    path, err
                );
                next_fingerprints.remove(&path);
                next_persisted_paths.insert(path.clone());
            }
        }

        self.persisted_save_paths = next_persisted_paths;
        self.last_vfs_fingerprints = next_fingerprints;
    }

    fn persist_save_file(&self, file: &VfsFileSnapshot) -> io::Result<()> {
        let dir = self.save_dir_for_vfs_path(&file.path);
        fs::create_dir_all(&dir)?;
        fs::write(dir.join(DATA_FORK_FILE), &file.data_fork)?;
        fs::write(dir.join(RESOURCE_FORK_FILE), &file.resource_fork)?;

        let metadata = StoredSaveMetadata {
            version: 1,
            path: file.path.clone(),
            file_type: file.file_type,
            creator: file.creator,
            finder_flags: file.finder_flags,
            created_date: file.created_date,
            modified_date: file.modified_date,
        };
        let metadata = serde_json::to_vec_pretty(&metadata)
            .map_err(|err| io::Error::new(io::ErrorKind::InvalidData, err))?;
        fs::write(dir.join(METADATA_FILE), metadata)?;
        Ok(())
    }

    fn delete_save_file(&self, path: &str) -> io::Result<()> {
        let dir = self.save_dir_for_vfs_path(path);
        match fs::remove_dir_all(&dir) {
            Ok(()) => {
                if let Some(parent) = dir.parent() {
                    self.prune_empty_parent_dirs(parent.to_path_buf());
                }
                Ok(())
            }
            Err(err) if err.kind() == io::ErrorKind::NotFound => Ok(()),
            Err(err) => Err(err),
        }
    }

    fn save_dir_for_vfs_path(&self, path: &str) -> PathBuf {
        let mut dir = self.root.clone();
        let mut added = false;
        for component in path.trim_matches('/').split('/') {
            if component.is_empty() {
                continue;
            }
            dir.push(encode_host_component(component));
            added = true;
        }
        if !added {
            dir.push("%00");
        }
        dir
    }

    fn prune_empty_parent_dirs(&self, mut dir: PathBuf) {
        while dir != self.root {
            match fs::remove_dir(&dir) {
                Ok(()) => {
                    let Some(parent) = dir.parent() else {
                        break;
                    };
                    dir = parent.to_path_buf();
                }
                Err(_) => break,
            }
        }
    }
}

fn reset_preferences_in_root(root: &Path) -> io::Result<usize> {
    let mut removed = 0;
    for system in matching_child_dirs(root, "system folder")? {
        for prefs in matching_child_dirs(&system, "preferences")? {
            fs::remove_dir_all(&prefs)?;
            removed += 1;
        }
    }
    Ok(removed)
}

#[derive(Debug, Deserialize, Serialize)]
struct StoreManifest {
    version: u8,
    sha256: String,
    display_name: String,
    source_paths: Vec<String>,
    imports: Vec<StoreImport>,
}

#[derive(Debug, Deserialize, Serialize)]
struct StoreImport {
    source: String,
    sha256: Option<String>,
}

fn application_data_dir() -> io::Result<PathBuf> {
    // Native per-user data locations; an unavailable home is an error rather
    // than permission to write private state beside an archive or in cwd.
    let absolute_env = |name| {
        std::env::var_os(name)
            .map(PathBuf::from)
            .filter(|p| p.is_absolute())
    };
    #[cfg(target_os = "windows")]
    let root = absolute_env("LOCALAPPDATA").map(|p| p.join("Systemless"));
    #[cfg(target_os = "macos")]
    let root = absolute_env("HOME").map(|p| p.join("Library/Application Support/Systemless"));
    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    let root = absolute_env("XDG_DATA_HOME")
        .map(|p| p.join("systemless"))
        .or_else(|| absolute_env("HOME").map(|p| p.join(".local/share/systemless")));
    root.ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::NotFound,
            "per-user application-data directory is unavailable",
        )
    })
}

fn archive_digest(game_path: &Path) -> io::Result<String> {
    let mut file = fs::File::open(game_path)?;
    let mut hash = Sha256::new();
    let mut bytes = [0; 64 * 1024];
    loop {
        let len = file.read(&mut bytes)?;
        if len == 0 {
            break;
        }
        hash.update(&bytes[..len]);
    }
    Ok(format!("{:x}", hash.finalize()))
}

fn manifest_bytes(manifest: &StoreManifest) -> io::Result<Vec<u8>> {
    serde_json::to_vec_pretty(manifest)
        .map_err(|err| io::Error::new(io::ErrorKind::InvalidData, err))
}

fn read_manifest(root: &Path) -> io::Result<StoreManifest> {
    let manifest: StoreManifest = serde_json::from_slice(&fs::read(root.join("manifest.json"))?)
        .map_err(|err| io::Error::new(io::ErrorKind::InvalidData, err))?;
    if manifest.version != 1 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "unsupported save-store manifest version",
        ));
    }
    Ok(manifest)
}

fn strict_snapshots(root: &Path) -> io::Result<Vec<VfsFileSnapshot>> {
    let mut paths = Vec::new();
    collect_metadata_files(root, &mut paths)?;
    paths.sort();
    paths.iter().map(|p| read_saved_file(p)).collect()
}

/// Serialize migration/import decisions across processes. Build a complete
/// sibling directory before publishing it, so an interrupted copy cannot
/// become an authoritative, partially migrated store on the next launch.
fn prepare_store(game_path: &Path, data_dir: &Path, import: Option<&Path>) -> io::Result<PathBuf> {
    let digest = archive_digest(game_path)?;
    let stores = data_dir.join("saves");
    fs::create_dir_all(&stores)?;
    let lock = fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .open(stores.join(".migration.lock"))?;
    lock.lock()?;
    let root = stores.join(&digest);
    let existed = root.try_exists()?;
    let source_path = fs::canonicalize(game_path)?.to_string_lossy().into_owned();
    let display_name = game_path
        .file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .into_owned();
    let mut manifest = if existed && root.join("manifest.json").try_exists()? {
        let manifest = read_manifest(&root)?;
        if manifest.sha256 != digest {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "save-store identity does not match archive",
            ));
        }
        manifest
    } else {
        StoreManifest {
            version: 1,
            sha256: digest,
            display_name: String::new(),
            source_paths: Vec::new(),
            imports: Vec::new(),
        }
    };
    manifest.display_name = display_name;
    if !manifest.source_paths.contains(&source_path) {
        manifest.source_paths.push(source_path);
    }

    let legacy = legacy_root_for_game_path(game_path);
    let source = match import {
        Some(source) => Some(source.to_path_buf()),
        None if !existed && legacy.try_exists()? => Some(legacy),
        None => None,
    };
    let snapshots = if let Some(source) = source.as_deref() {
        let source = fs::canonicalize(source)?;
        if existed && source == fs::canonicalize(&root)? {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "cannot import a save store into itself",
            ));
        }
        let incoming = strict_snapshots(&source)?;
        let mut names = HashSet::new();
        if existed {
            for file in strict_snapshots(&root)? {
                names.insert(file.path.to_ascii_lowercase());
            }
        }
        for file in &incoming {
            if !names.insert(file.path.to_ascii_lowercase()) {
                return Err(io::Error::new(
                    io::ErrorKind::AlreadyExists,
                    format!("save import conflict: {}", file.path),
                ));
            }
        }
        let source_digest = if source.join("manifest.json").try_exists()? {
            Some(read_manifest(&source)?.sha256)
        } else {
            None
        };
        manifest.imports.push(StoreImport {
            source: source.to_string_lossy().into_owned(),
            sha256: source_digest,
        });
        incoming
    } else {
        Vec::new()
    };

    if existed {
        let live = DesktopSaveStore::at_root(root.clone(), HashMap::new());
        for snapshot in &snapshots {
            if live.save_dir_for_vfs_path(&snapshot.path).try_exists()? {
                return Err(io::Error::new(
                    io::ErrorKind::AlreadyExists,
                    format!("save import destination already exists: {}", snapshot.path),
                ));
            }
        }
    }

    // A unique staging sibling also makes incomplete work easy to distinguish
    // from a live store. Never overwrite or remove an existing destination.
    let staging = stores.join(format!(
        ".staging-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos()
    ));
    fs::create_dir(&staging)?;
    let result = (|| {
        let store = DesktopSaveStore::at_root(staging.clone(), HashMap::new());
        for (index, snapshot) in snapshots.iter().enumerate() {
            if existed {
                let isolated =
                    DesktopSaveStore::at_root(staging.join(index.to_string()), HashMap::new());
                isolated.persist_save_file(snapshot)?;
            } else {
                store.persist_save_file(snapshot)?;
            }
        }
        fs::write(staging.join("manifest.json"), manifest_bytes(&manifest)?)?;
        if existed {
            // Imports are preflighted before touching the live store. Publish
            // new file directories individually; retain existing files on errors.
            let live = DesktopSaveStore::at_root(root.clone(), HashMap::new());
            let mut published = Vec::new();
            let publish = (|| -> io::Result<()> {
                for (index, snapshot) in snapshots.iter().enumerate() {
                    let source =
                        DesktopSaveStore::at_root(staging.join(index.to_string()), HashMap::new())
                            .save_dir_for_vfs_path(&snapshot.path);
                    let target = live.save_dir_for_vfs_path(&snapshot.path);
                    if target.try_exists()? {
                        return Err(io::Error::new(
                            io::ErrorKind::AlreadyExists,
                            format!(
                                "save import destination already exists: {}",
                                target.display()
                            ),
                        ));
                    }
                    if let Some(parent) = target.parent() {
                        fs::create_dir_all(parent)?;
                    }
                    fs::rename(source, &target)?;
                    published.push(target);
                }
                fs::rename(staging.join("manifest.json"), root.join("manifest.json"))
            })();
            if let Err(error) = publish {
                for target in published.into_iter().rev() {
                    if let Err(rollback) = fs::remove_dir_all(&target) {
                        return Err(io::Error::other(format!(
                            "{error}; import rollback at {} failed: {rollback}",
                            target.display()
                        )));
                    }
                }
                return Err(error);
            }
        } else {
            fs::rename(&staging, &root)?;
        }
        Ok(root.clone())
    })();
    let _ = fs::remove_dir_all(&staging);
    result
}

fn vfs_stats(runner: &mut FixtureRunner) -> HashMap<String, VfsFileStat> {
    runner
        .vfs_file_stats_where(|_| true)
        .into_iter()
        .map(|stat| (stat.path.clone(), stat))
        .collect()
}

fn vfs_stats_match(left: &VfsFileStat, right: &VfsFileStat) -> bool {
    left.data_len == right.data_len
        && left.resource_len == right.resource_len
        && left.file_type == right.file_type
        && left.creator == right.creator
        && left.finder_flags == right.finder_flags
        && left.modified_date == right.modified_date
}

fn save_fork_hash(bytes: &[u8]) -> u64 {
    let mut hash = 0xcbf2_9ce4_8422_2325u64;
    for &byte in bytes {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}

fn load_saved_files_from_root(root: &Path) -> io::Result<Vec<VfsFileSnapshot>> {
    if !root.exists() {
        return Ok(Vec::new());
    }

    let mut metadata_files = Vec::new();
    collect_metadata_files(root, &mut metadata_files)?;
    let mut files = Vec::new();
    for metadata_path in metadata_files {
        match read_saved_file(&metadata_path) {
            Ok(file) => files.push(file),
            Err(err) => eprintln!(
                "[SYSTEMLESS] Skipping unreadable desktop save {}: {}",
                metadata_path.display(),
                err
            ),
        }
    }
    files.sort_by(|left, right| {
        left.path
            .to_ascii_lowercase()
            .cmp(&right.path.to_ascii_lowercase())
    });
    Ok(files)
}

fn collect_metadata_files(dir: &Path, out: &mut Vec<PathBuf>) -> io::Result<()> {
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        let file_type = entry.file_type()?;
        if file_type.is_dir() {
            collect_metadata_files(&path, out)?;
        } else if file_type.is_file() && entry.file_name() == OsStr::new(METADATA_FILE) {
            out.push(path);
        }
    }
    Ok(())
}

fn read_saved_file(metadata_path: &Path) -> io::Result<VfsFileSnapshot> {
    let metadata = fs::read(metadata_path)?;
    let metadata: StoredSaveMetadata = serde_json::from_slice(&metadata)
        .map_err(|err| io::Error::new(io::ErrorKind::InvalidData, err))?;
    if metadata.version != 1 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("unsupported save metadata version {}", metadata.version),
        ));
    }
    if !is_user_save_path(&metadata.path) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "stored path is not a user save path",
        ));
    }

    let dir = metadata_path.parent().ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            "save metadata path does not have a parent directory",
        )
    })?;

    Ok(VfsFileSnapshot {
        path: metadata.path,
        data_fork: read_fork_file(&dir.join(DATA_FORK_FILE))?,
        resource_fork: read_fork_file(&dir.join(RESOURCE_FORK_FILE))?,
        file_type: metadata.file_type,
        creator: metadata.creator,
        finder_flags: metadata.finder_flags,
        created_date: metadata.created_date,
        modified_date: metadata.modified_date,
    })
}

fn read_fork_file(path: &Path) -> io::Result<Vec<u8>> {
    match fs::read(path) {
        Ok(bytes) => Ok(bytes),
        Err(err) if err.kind() == io::ErrorKind::NotFound => Ok(Vec::new()),
        Err(err) => Err(err),
    }
}

fn legacy_root_for_game_path(game_path: &Path) -> PathBuf {
    let archive_dir = game_path
        .parent()
        .filter(|path| !path.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    let archive_stem = game_path
        .file_stem()
        .and_then(OsStr::to_str)
        .filter(|stem| !stem.trim().is_empty())
        .map(encode_host_component)
        .unwrap_or_else(|| "game".to_string());
    archive_dir
        .join(".systemless")
        .join("saves")
        .join(archive_stem)
}

fn is_user_save_path(path: &str) -> bool {
    let normalized = path.trim_matches('/');
    if normalized.is_empty() {
        return false;
    }

    let lower = normalized.to_ascii_lowercase();
    if lower.starts_with("__rsrc__/")
        || lower.starts_with("system folder/temporary items/")
        || lower.starts_with("temporary items/")
        || lower.starts_with("trash/")
    {
        return false;
    }

    // System Folder preferences are persisted: some games (Deimos Rising)
    // quit after their first-run settings panel and only get further once
    // they find their preferences on the next launch.
    let name = lower.rsplit('/').next().unwrap_or(lower.as_str());
    !matches!(name, "desktop db" | "desktop df" | "thevolume")
}

// Guest paths keep the application's own casing, so match the System Folder
// and Preferences components case-insensitively like the save filter does.
fn matching_child_dirs(dir: &Path, lower_name: &str) -> io::Result<Vec<PathBuf>> {
    let entries = match fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(err) if err.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(err) => return Err(err),
    };
    let mut out = Vec::new();
    for entry in entries {
        let entry = entry?;
        if entry.file_type()?.is_dir()
            && entry.file_name().to_string_lossy().to_ascii_lowercase() == lower_name
        {
            out.push(entry.path());
        }
    }
    Ok(out)
}

fn encode_host_component(component: &str) -> String {
    if component == "." || component == ".." {
        return percent_encode_all(component.as_bytes());
    }

    let mut encoded = String::new();
    for &byte in component.as_bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b' ' | b'.' | b'-' | b'_' => {
                encoded.push(byte as char)
            }
            _ => push_percent_encoded(&mut encoded, byte),
        }
    }

    if encoded.is_empty() {
        "%00".to_string()
    } else {
        encoded
    }
}

fn percent_encode_all(bytes: &[u8]) -> String {
    let mut encoded = String::new();
    for &byte in bytes {
        push_percent_encoded(&mut encoded, byte);
    }
    encoded
}

fn push_percent_encoded(out: &mut String, byte: u8) {
    const HEX: &[u8; 16] = b"0123456789ABCDEF";
    out.push('%');
    out.push(HEX[(byte >> 4) as usize] as char);
    out.push(HEX[(byte & 0x0F) as usize] as char);
}

#[cfg(test)]
mod tests {
    use super::*;
    use systemless::systems::macintosh::runner::{FixtureRunner, FixtureRunnerConfig};

    fn snapshot(path: &str) -> VfsFileSnapshot {
        VfsFileSnapshot {
            path: path.to_string(),
            data_fork: vec![1, 2, 3],
            resource_fork: vec![4, 5, 6, 7],
            file_type: u32::from_be_bytes(*b"PIL "),
            creator: u32::from_be_bytes(*b"EVO!"),
            finder_flags: 0x4000,
            created_date: 123,
            modified_date: 456,
        }
    }

    #[test]
    fn legacy_root_sits_next_to_archive() {
        assert_eq!(
            legacy_root_for_game_path(Path::new("/Games/EV Override 1.0.1.sit")),
            Path::new("/Games/.systemless/saves/EV Override 1.0.1")
        );
        assert_eq!(
            legacy_root_for_game_path(Path::new("EVO.sit")),
            Path::new("./.systemless/saves/EVO")
        );
    }

    #[test]
    fn host_components_are_percent_encoded() {
        assert_eq!(encode_host_component("Pilots"), "Pilots");
        assert_eq!(encode_host_component("EV:Override%"), "EV%3AOverride%25");
        assert_eq!(encode_host_component("."), "%2E");
        assert_eq!(encode_host_component(".."), "%2E%2E");
    }

    #[test]
    fn native_snapshot_round_trips_both_forks_and_metadata() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join(".systemless/saves/EVO");
        let store = DesktopSaveStore {
            root: root.clone(),
            archive_vfs_stats: HashMap::new(),
            last_vfs_fingerprints: HashMap::new(),
            persisted_save_paths: HashSet::new(),
            save_scan_frame: 0,
        };
        let original = snapshot("EV Override 1.0.1/Pilots/Rick Hardslab");

        store.persist_save_file(&original).unwrap();

        let loaded = load_saved_files_from_root(&root).unwrap();
        assert_eq!(loaded, vec![original]);
    }

    #[test]
    fn sync_skips_unchanged_archive_files_but_persists_modified_saves() {
        let temp = tempfile::tempdir().unwrap();
        let game_path = temp.path().join("EVO.sit");
        let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
        let mut packaged = snapshot("EV Override 1.0.1/Pilots/Packaged Pilot");
        runner.import_vfs_file(&packaged);

        fs::write(&game_path, b"archive").unwrap();
        let data = temp.path().join("data");
        let root = prepare_store(&game_path, &data, None).unwrap();
        let mut store = DesktopSaveStore::at_root(root, vfs_stats(&mut runner));
        store.sync_save_files_now(&mut runner);
        assert!(load_saved_files_from_root(store.root()).unwrap().is_empty());

        packaged.resource_fork.push(8);
        packaged.modified_date += 1;
        runner.import_vfs_file(&packaged);
        store.sync_save_files_now(&mut runner);

        let loaded = load_saved_files_from_root(store.root()).unwrap();
        assert_eq!(loaded, vec![packaged]);
    }

    #[test]
    fn reset_preferences_removes_only_system_folder_preferences() {
        let base =
            std::env::temp_dir().join(format!("systemless-reset-prefs-{}", std::process::id()));
        let _ = fs::remove_dir_all(&base);
        let game_path = base.join("Deimos Rising.sit");
        let root = legacy_root_for_game_path(&game_path);
        let store = DesktopSaveStore {
            root: root.clone(),
            archive_vfs_stats: HashMap::new(),
            last_vfs_fingerprints: HashMap::new(),
            persisted_save_paths: HashSet::new(),
            save_scan_frame: 0,
        };
        store
            .persist_save_file(&snapshot("System Folder/Preferences/Deimos Prefs"))
            .unwrap();
        store
            .persist_save_file(&snapshot("Pilots/Rick Hardslab"))
            .unwrap();

        assert_eq!(reset_preferences_in_root(&root).unwrap(), 1);
        let loaded = load_saved_files_from_root(&root).unwrap();
        assert_eq!(loaded, vec![snapshot("Pilots/Rick Hardslab")]);
        assert_eq!(reset_preferences_in_root(&root).unwrap(), 0);

        let _ = fs::remove_dir_all(&base);
    }

    #[test]
    fn user_save_filter_skips_system_support_files() {
        assert!(is_user_save_path("Pilots/Rick Hardslab"));
        assert!(is_user_save_path("Games/My Saved Game"));

        assert!(!is_user_save_path(""));
        assert!(is_user_save_path(
            "System Folder/Preferences/EV Override License"
        ));
        assert!(!is_user_save_path("Temporary Items/scratch"));
        assert!(!is_user_save_path("Trash/Old Pilot"));
        assert!(!is_user_save_path("Desktop DB"));
        assert!(!is_user_save_path("Desktop DF"));
        assert!(!is_user_save_path("TheVolume"));
        assert!(!is_user_save_path("__rsrc__/Pilots/Rick Hardslab"));
    }
    fn write_snapshot(root: &Path, file: &VfsFileSnapshot) {
        DesktopSaveStore::at_root(root.to_path_buf(), HashMap::new())
            .persist_save_file(file)
            .unwrap();
    }

    #[test]
    fn content_identity_survives_moves_renames_and_copies_but_not_updates() {
        let temp = tempfile::tempdir().unwrap();
        let data = temp.path().join("data");
        let original = temp.path().join("Original.sit");
        fs::write(&original, b"abc").unwrap();
        let root = prepare_store(&original, &data, None).unwrap();
        assert_eq!(
            root.file_name().unwrap(),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
        write_snapshot(&root, &snapshot("Game/Pilot"));
        let renamed = temp.path().join("Renamed.sit");
        fs::rename(&original, &renamed).unwrap();
        assert_eq!(prepare_store(&renamed, &data, None).unwrap(), root);
        let folder = temp.path().join("Moved");
        fs::create_dir(&folder).unwrap();
        let moved = folder.join("Renamed.sit");
        fs::rename(&renamed, &moved).unwrap();
        assert_eq!(prepare_store(&moved, &data, None).unwrap(), root);
        let copy = temp.path().join("Copy.sit");
        fs::copy(&moved, &copy).unwrap();
        assert_eq!(prepare_store(&copy, &data, None).unwrap(), root);
        assert_eq!(
            load_saved_files_from_root(&root).unwrap(),
            vec![snapshot("Game/Pilot")]
        );
        let manifest = read_manifest(&root).unwrap();
        assert_eq!(manifest.display_name, "Copy.sit");
        assert_eq!(manifest.source_paths.len(), 4);
        fs::write(&copy, b"updated").unwrap();
        let updated = prepare_store(&copy, &data, None).unwrap();
        assert_ne!(updated, root);
        assert!(load_saved_files_from_root(&updated).unwrap().is_empty());
        prepare_store(&copy, &data, Some(&root)).unwrap();
        assert_eq!(
            load_saved_files_from_root(&updated).unwrap(),
            vec![snapshot("Game/Pilot")]
        );
        let manifest = read_manifest(&updated).unwrap();
        assert_eq!(
            manifest.imports[0].sha256.as_deref(),
            Some(read_manifest(&root).unwrap().sha256.as_str())
        );
    }

    #[test]
    fn legacy_migration_preserves_both_forks_metadata_and_the_original() {
        let temp = tempfile::tempdir().unwrap();
        let game = temp.path().join("Game.sit");
        fs::write(&game, b"archive").unwrap();
        let legacy = legacy_root_for_game_path(&game);
        let original = snapshot("Game/Pilots/Rick");
        write_snapshot(&legacy, &original);
        let root = prepare_store(&game, &temp.path().join("data"), None).unwrap();
        assert_eq!(
            load_saved_files_from_root(&root).unwrap(),
            vec![original.clone()]
        );
        assert_eq!(load_saved_files_from_root(&legacy).unwrap(), vec![original]);
        assert_eq!(
            read_manifest(&root).unwrap().imports[0].source,
            fs::canonicalize(legacy).unwrap().to_string_lossy()
        );
    }

    #[test]
    fn new_store_wins_and_explicit_recovery_rejects_all_conflicts_before_writing() {
        let temp = tempfile::tempdir().unwrap();
        let game = temp.path().join("Game.sit");
        fs::write(&game, b"archive").unwrap();
        let data = temp.path().join("data");
        let root = prepare_store(&game, &data, None).unwrap();
        write_snapshot(&root, &snapshot("Game/Pilot"));
        let legacy = legacy_root_for_game_path(&game);
        let mut other = snapshot("game/pilot");
        other.data_fork = b"legacy".to_vec();
        write_snapshot(&legacy, &other);
        write_snapshot(&legacy, &snapshot("Game/Other Pilot"));
        assert_eq!(prepare_store(&game, &data, None).unwrap(), root);
        assert_eq!(
            load_saved_files_from_root(&root).unwrap(),
            vec![snapshot("Game/Pilot")]
        );
        let before = fs::read(root.join("manifest.json")).unwrap();
        assert_eq!(
            prepare_store(&game, &data, Some(&legacy))
                .unwrap_err()
                .kind(),
            io::ErrorKind::AlreadyExists
        );
        assert_eq!(fs::read(root.join("manifest.json")).unwrap(), before);
        assert_eq!(
            load_saved_files_from_root(&root).unwrap(),
            vec![snapshot("Game/Pilot")]
        );
        assert_eq!(load_saved_files_from_root(&legacy).unwrap().len(), 2);
        let recovery = temp.path().join("old-location");
        write_snapshot(&recovery, &snapshot("Game/Recovered"));
        prepare_store(&game, &data, Some(&recovery)).unwrap();
        assert_eq!(load_saved_files_from_root(&root).unwrap().len(), 2);
        assert_eq!(
            load_saved_files_from_root(&recovery).unwrap(),
            vec![snapshot("Game/Recovered")]
        );
    }

    #[test]
    fn failed_migration_does_not_publish_a_partial_store() {
        let temp = tempfile::tempdir().unwrap();
        let game = temp.path().join("Game.sit");
        fs::write(&game, b"archive").unwrap();
        let legacy = legacy_root_for_game_path(&game);
        write_snapshot(&legacy, &snapshot("Game/Pilot"));
        fs::create_dir_all(legacy.join("Broken")).unwrap();
        fs::write(legacy.join("Broken/metadata.json"), b"not json").unwrap();
        let data = temp.path().join("data");
        assert!(prepare_store(&game, &data, None).is_err());
        assert!(!data
            .join("saves")
            .join(archive_digest(&game).unwrap())
            .exists());
        fs::remove_dir_all(legacy.join("Broken")).unwrap();
        let root = prepare_store(&game, &data, None).unwrap();
        assert_eq!(load_saved_files_from_root(&root).unwrap().len(), 1);
    }

    #[test]
    fn unknown_manifest_version_is_not_overwritten() {
        let temp = tempfile::tempdir().unwrap();
        let game = temp.path().join("Game.sit");
        fs::write(&game, b"archive").unwrap();
        let data = temp.path().join("data");
        let root = prepare_store(&game, &data, None).unwrap();
        let mut manifest = read_manifest(&root).unwrap();
        manifest.version = 2;
        let bytes = manifest_bytes(&manifest).unwrap();
        fs::write(root.join("manifest.json"), &bytes).unwrap();
        assert!(prepare_store(&game, &data, None).is_err());
        assert_eq!(fs::read(root.join("manifest.json")).unwrap(), bytes);
    }

    #[cfg(unix)]
    #[test]
    fn read_only_source_directory_does_not_prevent_saving() {
        use std::os::unix::fs::PermissionsExt;
        let temp = tempfile::tempdir().unwrap();
        let source = temp.path().join("source");
        fs::create_dir(&source).unwrap();
        let game = source.join("Game.sit");
        fs::write(&game, b"archive").unwrap();
        fs::set_permissions(&source, fs::Permissions::from_mode(0o555)).unwrap();
        let result = prepare_store(&game, &temp.path().join("data"), None);
        fs::set_permissions(&source, fs::Permissions::from_mode(0o755)).unwrap();
        let root = result.unwrap();
        write_snapshot(&root, &snapshot("Game/Pilot"));
        assert_eq!(load_saved_files_from_root(&root).unwrap().len(), 1);
        assert!(!source.join(".systemless").exists());
    }
}
