use std::collections::{HashMap, HashSet};
use std::ffi::OsStr;
use std::fs;
use std::io;
use std::io::Write;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use systemless::runner::{FixtureRunner, VfsDirectorySnapshot, VfsFileSnapshot, VfsFileStat, VfsFileSummary};

const SAVE_SCAN_FRAME_INTERVAL: u8 = 30;
// SLSAVE01 + three big-endian u64 lengths + metadata JSON + raw data/resource
// forks. One rename publishes a complete generation. Legacy three-file saves
// remain readable, but are never combined with a published snapshot.
const SNAPSHOT_FILE: &str = "snapshot.bin";
const METADATA_FILE: &str = "metadata.json";
const DATA_FORK_FILE: &str = "data.fork";
const RESOURCE_FORK_FILE: &str = "resource.fork";

#[derive(Debug)]
pub struct DesktopSaveStore {
    root: PathBuf,
    archive_vfs_stats: HashMap<String, VfsFileStat>,
    archive_directories: Vec<VfsDirectorySnapshot>,
    persisted_directories: Vec<VfsDirectorySnapshot>,
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
    pub fn root_for_game_path(game_path: &Path) -> PathBuf {
        save_root_for_game_path(game_path)
    }

    pub fn for_loaded_archive(game_path: &Path, runner: &mut FixtureRunner) -> Self {
        Self {
            root: Self::root_for_game_path(game_path),
            archive_vfs_stats: vfs_stats(runner),
            archive_directories: runner.vfs_directory_snapshots(),
            persisted_directories: Vec::new(),
            last_vfs_fingerprints: HashMap::new(),
            persisted_save_paths: HashSet::new(),
            save_scan_frame: 0,
        }
    }

    /// Delete the persisted System Folder preferences for this archive, so
    /// preferences written under an emulation bug don't outlive its fix.
    pub fn reset_preferences(game_path: &Path) -> io::Result<usize> {
        let root = save_root_for_game_path(game_path);
        let mut removed = 0;
        for system in matching_child_dirs(&root, "system folder")? {
            for prefs in matching_child_dirs(&system, "preferences")? {
                fs::remove_dir_all(&prefs)?;
                removed += 1;
            }
        }
        Ok(removed)
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn restore_saved_state(&mut self, runner: &mut FixtureRunner) {
        let path = self.root.join("directories.json");
        match fs::read(&path).and_then(|bytes| serde_json::from_slice::<Vec<VfsDirectorySnapshot>>(&bytes)
            .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))) {
            Ok(directories) => {
                self.persisted_directories = directories;
                for directory in &self.persisted_directories { runner.import_vfs_directory(directory); }
            }
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => eprintln!("[SYSTEMLESS] Could not restore directories {}: {error}", path.display()),
        }
        for file in self.load_saved_files() { runner.import_vfs_file(&file); }
    }

    fn sync_directories(&mut self, runner: &FixtureRunner) {
        let mut directories: Vec<_> = runner.vfs_directory_snapshots().into_iter()
            .filter(|directory| is_user_save_path(&directory.path)
                && !self.archive_directories.contains(directory)).collect();
        directories.sort_by(|left, right| left.path.cmp(&right.path));
        if directories == self.persisted_directories { return; }
        let result = (|| -> io::Result<()> {
            fs::create_dir_all(&self.root)?;
            let bytes = serde_json::to_vec_pretty(&directories)
                .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
            let temporary = self.root.join("directories.json.tmp");
            fs::write(&temporary, bytes)?;
            fs::rename(temporary, self.root.join("directories.json"))
        })();
        match result {
            Ok(()) => self.persisted_directories = directories,
            Err(error) => eprintln!("[SYSTEMLESS] Could not persist directories: {error}"),
        }
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
        self.sync_directories(runner);
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
        self.persist_save_file_before_publish(file, |_| Ok(()))
    }

    fn persist_save_file_before_publish(&self, file: &VfsFileSnapshot,
        before_publish: impl FnOnce(&Path) -> io::Result<()>) -> io::Result<()> {
        let dir = self.save_dir_for_vfs_path(&file.path);
        fs::create_dir_all(&dir)?;

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
        let nonce = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH)
            .map_err(io::Error::other)?.as_nanos();
        let temporary = dir.join(format!(".snapshot-{}-{nonce}.tmp", std::process::id()));
        // Only clean up a staging file this invocation successfully created.
        let mut output = fs::OpenOptions::new().write(true).create_new(true).open(&temporary)?;
        let result = (|| {
            output.write_all(b"SLSAVE01")?;
            for length in [metadata.len(), file.data_fork.len(), file.resource_fork.len()] {
                output.write_all(&(length as u64).to_be_bytes())?;
            }
            output.write_all(&metadata)?;
            output.write_all(&file.data_fork)?;
            output.write_all(&file.resource_fork)?;
            output.sync_all()?;
            drop(output);
            before_publish(&temporary)?;
            fs::rename(&temporary, dir.join(SNAPSHOT_FILE))?;
            fs::File::open(&dir)?.sync_all()
        })();
        if result.is_err() { let _ = fs::remove_file(&temporary); }
        result
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
        } else if file_type.is_file() && (entry.file_name() == OsStr::new(SNAPSHOT_FILE)
            || (entry.file_name() == OsStr::new(METADATA_FILE) && !dir.join(SNAPSHOT_FILE).exists())) {
            out.push(path);
        }
    }
    Ok(())
}

fn read_saved_file(metadata_path: &Path) -> io::Result<VfsFileSnapshot> {
    let bytes = fs::read(metadata_path)?;
    let bundled = metadata_path.file_name() == Some(OsStr::new(SNAPSHOT_FILE));
    let (metadata_bytes, forks) = if bundled {
        if bytes.len() < 32 || &bytes[..8] != b"SLSAVE01" {
            return Err(io::Error::new(io::ErrorKind::InvalidData, "invalid save snapshot header"));
        }
        let mut cursor = 32usize;
        let mut parts = Vec::new();
        for offset in [8, 16, 24] {
            let length = usize::try_from(u64::from_be_bytes(bytes[offset..offset+8].try_into().unwrap()))
                .map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "save length overflow"))?;
            let end = cursor.checked_add(length).filter(|end| *end <= bytes.len())
                .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "truncated save snapshot"))?;
            parts.push(&bytes[cursor..end]); cursor = end;
        }
        if cursor != bytes.len() { return Err(io::Error::new(io::ErrorKind::InvalidData, "trailing save bytes")); }
        (parts[0], Some((parts[1], parts[2])))
    } else { (bytes.as_slice(), None) };
    let metadata: StoredSaveMetadata = serde_json::from_slice(metadata_bytes)
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
        data_fork: if let Some((data, _)) = forks { data.to_vec() } else { read_fork_file(&dir.join(DATA_FORK_FILE))? },
        resource_fork: if let Some((_, resource)) = forks { resource.to_vec() } else { read_fork_file(&dir.join(RESOURCE_FORK_FILE))? },
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

fn save_root_for_game_path(game_path: &Path) -> PathBuf {
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
    use systemless::runner::{FixtureRunner, FixtureRunnerConfig};

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
    fn empty_directories_restore_metadata_and_removed_paths_do_not_return() {
        let temp = tempfile::tempdir().unwrap();
        let game = temp.path().join("Showcase.sit");
        let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
        let packaged = VfsDirectorySnapshot { path: "Packaged".into(), creator: 1,
            file_type: 2, finder_flags: 3 };
        runner.import_vfs_directory(&packaged);
        let mut store = DesktopSaveStore::for_loaded_archive(&game, &mut runner);
        store.sync_save_files_now(&mut runner);
        assert!(!store.root().exists());
        let created = VfsDirectorySnapshot { path: "Packaged/Emptyé".into(),
            creator: 4, file_type: 5, finder_flags: 6 };
        runner.import_vfs_directory(&created);
        store.sync_save_files_now(&mut runner);
        let mut restored = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
        restored.import_vfs_directory(&packaged);
        let mut reloaded = DesktopSaveStore::for_loaded_archive(&game, &mut restored);
        reloaded.restore_saved_state(&mut restored);
        assert!(restored.vfs_directory_snapshots().contains(&created));
        assert!(restored.vfs_directory_snapshots().contains(&packaged));
        assert!(reloaded.load_saved_files().is_empty());
        // A fresh guest state without the created directory represents its deletion.
        let mut deleted = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
        deleted.import_vfs_directory(&packaged);
        reloaded.sync_save_files_now(&mut deleted);
        let mut reopened = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
        reopened.import_vfs_directory(&packaged);
        let mut store = DesktopSaveStore::for_loaded_archive(&game, &mut reopened);
        store.restore_saved_state(&mut reopened);
        assert!(!reopened.vfs_directory_snapshots().contains(&created));
    }

    #[test]
    fn save_root_sits_next_to_archive() {
        assert_eq!(
            save_root_for_game_path(Path::new("/Games/EV Override 1.0.1.sit")),
            Path::new("/Games/.systemless/saves/EV Override 1.0.1")
        );
        assert_eq!(
            save_root_for_game_path(Path::new("EVO.sit")),
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
            archive_directories: Vec::new(),
            persisted_directories: Vec::new(),
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
    fn malformed_snapshots_are_rejected_without_partial_forks() {
        let temp = tempfile::tempdir().unwrap();
        let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
        let store = DesktopSaveStore::for_loaded_archive(&temp.path().join("Game.sit"), &mut runner);
        let original = snapshot("Pilots/Test");
        store.persist_save_file(&original).unwrap();
        let path = store.save_dir_for_vfs_path(&original.path).join(SNAPSHOT_FILE);
        let bytes = fs::read(&path).unwrap();
        for length in [0, 7, 8, 31, 32, 33, bytes.len() - 1] {
            fs::write(&path, &bytes[..length]).unwrap();
            assert_eq!(read_saved_file(&path).unwrap_err().kind(), io::ErrorKind::InvalidData);
        }
        let mut overflow = bytes.clone(); overflow[8..16].copy_from_slice(&u64::MAX.to_be_bytes());
        fs::write(&path, overflow).unwrap();
        assert_eq!(read_saved_file(&path).unwrap_err().kind(), io::ErrorKind::InvalidData);
        let mut trailing = bytes.clone(); trailing.push(0);
        fs::write(&path, trailing).unwrap();
        assert_eq!(read_saved_file(&path).unwrap_err().kind(), io::ErrorKind::InvalidData);
        fs::write(&path, bytes).unwrap();
        assert_eq!(read_saved_file(&path).unwrap(), original);
    }

    #[test]
    fn abrupt_process_exit_preserves_complete_snapshot_at_publication_boundary() {
        let temp = tempfile::tempdir().unwrap();
        let root = std::env::var_os("SYSTEMLESS_ATOMIC_SAVE_ROOT").map(PathBuf::from)
            .unwrap_or_else(|| temp.path().to_path_buf());
        let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
        let store = DesktopSaveStore::for_loaded_archive(&root.join("Game.sit"), &mut runner);
        let old = snapshot("Pilots/Test");
        let mut new = old.clone(); new.data_fork = vec![42; 19];
        new.resource_fork = vec![255, 0, 128]; new.modified_date += 7;
        if let Ok(stage) = std::env::var("SYSTEMLESS_ATOMIC_SAVE_EXIT") {
            if stage == "before" {
                store.persist_save_file_before_publish(&new, |_| std::process::exit(77)).unwrap();
            } else {
                store.persist_save_file(&new).unwrap();
                std::process::exit(78);
            }
            panic!("abrupt exit must not return");
        }
        store.persist_save_file(&old).unwrap();
        for (stage, code, expected) in [("before", 77, old), ("after", 78, new)] {
            let status = std::process::Command::new(std::env::current_exe().unwrap())
                .args(["--exact", "desktop_save_store::tests::abrupt_process_exit_preserves_complete_snapshot_at_publication_boundary"])
                .env("SYSTEMLESS_ATOMIC_SAVE_ROOT", &root).env("SYSTEMLESS_ATOMIC_SAVE_EXIT", stage)
                .status().unwrap();
            assert_eq!(status.code(), Some(code));
            assert_eq!(load_saved_files_from_root(store.root()).unwrap(), vec![expected]);
        }
    }

    #[test]
    fn legacy_save_migrates_atomically_and_interrupted_stage_keeps_complete_save() {
        let temp = tempfile::tempdir().unwrap();
        let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
        let store = DesktopSaveStore::for_loaded_archive(&temp.path().join("Game.sit"), &mut runner);
        let old = snapshot("Pilots/Test");
        let dir = store.save_dir_for_vfs_path(&old.path);
        fs::create_dir_all(&dir).unwrap();
        let metadata = StoredSaveMetadata { version: 1, path: old.path.clone(),
            file_type: old.file_type, creator: old.creator, finder_flags: old.finder_flags,
            created_date: old.created_date, modified_date: old.modified_date };
        fs::write(dir.join(METADATA_FILE), serde_json::to_vec(&metadata).unwrap()).unwrap();
        fs::write(dir.join(DATA_FORK_FILE), &old.data_fork).unwrap();
        fs::write(dir.join(RESOURCE_FORK_FILE), &old.resource_fork).unwrap();
        assert_eq!(load_saved_files_from_root(store.root()).unwrap(), vec![old.clone()]);
        let mut new = old.clone(); new.data_fork = vec![9; 71];
        new.resource_fork = vec![128, 255, 0]; new.modified_date += 1;
        assert!(store.persist_save_file_before_publish(&new, |temporary| {
            assert!(temporary.exists());
            assert_eq!(load_saved_files_from_root(store.root()).unwrap(), vec![old.clone()]);
            Err(io::Error::other("interrupted before publication"))
        }).is_err());
        assert_eq!(load_saved_files_from_root(store.root()).unwrap(), vec![old.clone()]);
        store.persist_save_file(&new).unwrap();
        assert_eq!(load_saved_files_from_root(store.root()).unwrap(), vec![new.clone()]);
        // Stale legacy components must never mix with the published snapshot.
        fs::write(dir.join(DATA_FORK_FILE), b"stale legacy data").unwrap();
        fs::write(dir.join(".snapshot-abandoned.tmp"), b"incomplete stage").unwrap();
        assert_eq!(load_saved_files_from_root(store.root()).unwrap(), vec![new.clone()]);
        let mut next = new.clone(); next.data_fork = vec![42];
        assert!(store.persist_save_file_before_publish(&next, |_| Err(io::Error::other("interrupted"))).is_err());
        assert_eq!(load_saved_files_from_root(store.root()).unwrap(), vec![new]);
    }

    #[test]
    fn sync_skips_unchanged_archive_files_but_persists_modified_saves() {
        let temp = tempfile::tempdir().unwrap();
        let game_path = temp.path().join("EVO.sit");
        let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
        let mut packaged = snapshot("EV Override 1.0.1/Pilots/Packaged Pilot");
        runner.import_vfs_file(&packaged);

        let mut store = DesktopSaveStore::for_loaded_archive(&game_path, &mut runner);
        store.sync_save_files_now(&mut runner);
        assert!(!store.root().exists());

        packaged.resource_fork.push(8);
        packaged.modified_date += 1;
        runner.import_vfs_file(&packaged);
        store.sync_save_files_now(&mut runner);

        let loaded = load_saved_files_from_root(store.root()).unwrap();
        assert_eq!(loaded, vec![packaged]);
    }

    #[test]
    fn saves_restore_into_loaded_68k_and_powerpc_sessions() {
        use systemless::systems::macintosh::{game, session::MacintoshSession};
        let archive = include_bytes!("../../../tests/toolbox-showcase/toolbox-showcase.sit");
        for (powerpc, depth) in [(false, 1), (false, 8), (true, 8)] {
            let temp = tempfile::tempdir().unwrap();
            let game_path = temp.path().join("Showcase.sit");
            let mut session = MacintoshSession::new(true, Some(depth));
            session.runner_mut().set_prefer_powerpc_executables(powerpc);
            let app = session.load_bytes(archive).unwrap();
            session.initialize(&app);
            assert_eq!(session.runner().is_powerpc_app(), powerpc);
            let mut store = DesktopSaveStore::for_loaded_archive(&game_path, session.runner_mut());
            let saved = snapshot("Toolbox Showcase/Pilots/Persistence Probe");
            session.runner_mut().import_vfs_file(&saved);
            store.sync_save_files_now(session.runner_mut());
            drop(session);

            let mut restored = game::new_runner_with_screen_depth(depth);
            restored.set_prefer_powerpc_executables(powerpc);
            let app = game::load_game(&mut restored, archive).unwrap();
            let mut store = DesktopSaveStore::for_loaded_archive(&game_path, &mut restored);
            let files = store.load_saved_files();
            assert_eq!(files, vec![saved.clone()]);
            for file in files {
                restored.import_vfs_file(&file);
            }
            game::init_game(&mut restored, &app);
            assert_eq!(restored.is_powerpc_app(), powerpc);
            store.sync_save_files_now(&mut restored);
            assert_eq!(store.load_saved_files(), vec![saved]);
        }
    }

    #[test]
    fn reset_preferences_removes_only_system_folder_preferences() {
        let base = std::env::temp_dir().join(format!(
            "systemless-reset-prefs-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&base);
        let game_path = base.join("Deimos Rising.sit");
        let root = save_root_for_game_path(&game_path);
        let store = DesktopSaveStore {
            root: root.clone(),
            archive_vfs_stats: HashMap::new(),
            archive_directories: Vec::new(),
            persisted_directories: Vec::new(),
            last_vfs_fingerprints: HashMap::new(),
            persisted_save_paths: HashSet::new(),
            save_scan_frame: 0,
        };
        store
            .persist_save_file(&snapshot("System Folder/Preferences/Deimos Prefs"))
            .unwrap();
        store.persist_save_file(&snapshot("Pilots/Rick Hardslab")).unwrap();

        assert_eq!(DesktopSaveStore::reset_preferences(&game_path).unwrap(), 1);
        let loaded = load_saved_files_from_root(&root).unwrap();
        assert_eq!(loaded, vec![snapshot("Pilots/Rick Hardslab")]);
        assert_eq!(DesktopSaveStore::reset_preferences(&game_path).unwrap(), 0);

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
}
