use std::collections::{HashMap, HashSet};
use std::ffi::OsStr;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use systemless::runner::{FixtureRunner, VfsFileSnapshot, VfsFileSummary};

const SAVE_SCAN_FRAME_INTERVAL: u8 = 30;
const METADATA_FILE: &str = "metadata.json";
const DATA_FORK_FILE: &str = "data.fork";
const RESOURCE_FORK_FILE: &str = "resource.fork";

#[derive(Debug)]
pub struct DesktopSaveStore {
    root: PathBuf,
    archive_vfs_fingerprints: HashMap<String, SaveFingerprint>,
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
    pub fn for_loaded_archive(game_path: &Path, runner: &mut FixtureRunner) -> Self {
        Self {
            root: save_root_for_game_path(game_path),
            archive_vfs_fingerprints: vfs_fingerprints(runner),
            last_vfs_fingerprints: HashMap::new(),
            persisted_save_paths: HashSet::new(),
            save_scan_frame: 0,
        }
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
        let summaries = runner.vfs_file_summaries();
        let mut next_fingerprints = summaries
            .iter()
            .map(|summary| (summary.path.clone(), SaveFingerprint::from(summary)))
            .collect::<HashMap<_, _>>();
        let mut next_persisted_paths = HashSet::new();

        for summary in summaries {
            if !is_user_save_path(&summary.path) {
                continue;
            }

            let fingerprint = SaveFingerprint::from(&summary);
            if self
                .archive_vfs_fingerprints
                .get(&summary.path)
                .is_some_and(|archive| archive == &fingerprint)
            {
                continue;
            }

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

fn vfs_fingerprints(runner: &mut FixtureRunner) -> HashMap<String, SaveFingerprint> {
    runner
        .vfs_file_summaries()
        .into_iter()
        .map(|summary| (summary.path.clone(), SaveFingerprint::from(&summary)))
        .collect()
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
        || lower.starts_with("system folder/preferences/")
        || lower.starts_with("system folder/temporary items/")
        || lower.starts_with("temporary items/")
        || lower.starts_with("trash/")
    {
        return false;
    }

    let name = lower.rsplit('/').next().unwrap_or(lower.as_str());
    !matches!(name, "desktop db" | "desktop df" | "thevolume")
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
            archive_vfs_fingerprints: HashMap::new(),
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
    fn user_save_filter_skips_system_support_files() {
        assert!(is_user_save_path("Pilots/Rick Hardslab"));
        assert!(is_user_save_path("Games/My Saved Game"));

        assert!(!is_user_save_path(""));
        assert!(!is_user_save_path(
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
