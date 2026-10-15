//! Explicit host volume backing. Guest File Manager contains no host paths.
use crate::files::{Files, VOLUME_IMAGE_LIMIT};
use std::{
    fs::{File, OpenOptions},
    io::{self, Read, Write},
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};
static TEMP: AtomicU64 = AtomicU64::new(0);
pub struct Volume {
    path: PathBuf,
    // Stable sidecar inode: atomic replacement of the image cannot drop its lock.
    _lock: File,
    saved_revision: Option<u64>,
}
fn codec(error: crate::Error) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, error)
}
fn read(path: &Path) -> io::Result<Vec<u8>> {
    let mut bytes = Vec::new();
    File::open(path)?
        .take(VOLUME_IMAGE_LIMIT as u64 + 1)
        .read_to_end(&mut bytes)?;
    if bytes.len() > VOLUME_IMAGE_LIMIT {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "volume image exceeds size limit",
        ));
    }
    Ok(bytes)
}
impl Volume {
    /// Open or prepare one explicit writable volume. Parent directory must exist.
    /// Existing corrupt images fail without changing the catalog or image.
    pub fn open(path: impl AsRef<Path>, files: &mut Files) -> io::Result<Self> {
        let requested = path.as_ref();
        let name = requested.file_name().ok_or_else(|| {
            io::Error::new(io::ErrorKind::InvalidInput, "volume needs a file name")
        })?;
        let parent = requested
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .unwrap_or(Path::new("."));
        let parent = parent.canonicalize()?;
        let path = parent.join(name);
        match std::fs::symlink_metadata(&path) {
            Ok(meta) if !meta.is_file() || meta.file_type().is_symlink() => {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "volume must be a regular file",
                ))
            }
            Ok(_) => {}
            Err(e) if e.kind() == io::ErrorKind::NotFound => {}
            Err(e) => return Err(e),
        }
        let mut sidecar = name.to_os_string();
        sidecar.push(".lock");
        let lock = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(parent.join(sidecar))?;
        lock.try_lock()?;
        let saved_revision = match read(&path) {
            Ok(bytes) => {
                files.restore_volume(&bytes).map_err(codec)?;
                Some(files.revision())
            }
            Err(e) if e.kind() == io::ErrorKind::NotFound => None,
            Err(e) => return Err(e),
        };
        Ok(Self {
            path,
            _lock: lock,
            saved_revision,
        })
    }
    /// Checkpoint changed mutable files at a host execution boundary. Open fork
    /// contents are included; transient access paths/WDs are never serialized.
    pub fn checkpoint(&mut self, files: &Files) -> io::Result<bool> {
        if self.saved_revision == Some(files.revision()) {
            return Ok(false);
        }
        let image = files.export_volume().map_err(codec)?;
        let parent = self.path.parent().unwrap();
        let mut name = self.path.file_name().unwrap().to_os_string();
        name.push(format!(
            ".{}.{}.tmp",
            std::process::id(),
            TEMP.fetch_add(1, Ordering::Relaxed)
        ));
        let temporary = parent.join(name);
        let mut output = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)?;
        let result = (|| {
            output.write_all(&image)?;
            output.sync_all()?;
            drop(output);
            std::fs::rename(&temporary, &self.path)?;
            #[cfg(unix)]
            File::open(parent)?.sync_all()?;
            Ok::<_, io::Error>(())
        })();
        if result.is_err() {
            let _ = std::fs::remove_file(&temporary);
        }
        result?;
        self.saved_revision = Some(files.revision());
        Ok(true)
    }
}
#[cfg(test)]
mod tests {
    use super::*;

    fn files() -> Files {
        Files::default()
    }
    fn temp() -> PathBuf {
        let path = std::env::temp_dir().join(format!(
            "systemless-volume-{}-{}",
            std::process::id(),
            TEMP.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir(&path).unwrap();
        path
    }
    // Host tests use the same fresh application catalog as both loaders.
    fn app() -> Files {
        Files::application(b"test application", Some(b"resource fork"))
    }
    #[test]
    fn exclusive_lock_atomic_checkpoint_reload_and_unchanged_generation() {
        let root = temp();
        let path = root.join("catalog.slv");
        let mut a = app();
        let mut volume = Volume::open(&path, &mut a).unwrap();
        let mut b = app();
        assert_eq!(
            Volume::open(&path, &mut b).err().unwrap().kind(),
            io::ErrorKind::WouldBlock
        );
        assert!(volume.checkpoint(&a).unwrap());
        let saved = std::fs::read(&path).unwrap();
        assert!(!volume.checkpoint(&a).unwrap());
        assert_eq!(std::fs::read(&path).unwrap(), saved);
        drop(volume);
        let mut restored = app();
        let mut volume = Volume::open(&path, &mut restored).unwrap();
        assert!(!volume.checkpoint(&restored).unwrap());
        assert_eq!(restored.export_volume().unwrap(), saved);
        drop(volume);
        std::fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn corrupt_or_missing_parent_images_do_not_overwrite_files_or_mutate_catalog() {
        let root = temp();
        let path = root.join("bad.slv");
        std::fs::write(&path, b"corrupt").unwrap();
        let mut a = app();
        let before = a.export_volume().unwrap();
        assert!(Volume::open(&path, &mut a).is_err());
        assert_eq!(std::fs::read(&path).unwrap(), b"corrupt");
        assert_eq!(a.export_volume().unwrap(), before);
        assert!(Volume::open(root.join("missing/volume"), &mut a).is_err());
        assert!(Volume::open(root.join("invalid"), &mut files())
            .unwrap()
            .checkpoint(&files())
            .is_err());
        std::fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn failed_atomic_replacement_cleans_temporary_file_and_can_be_retried() {
        let root = temp();
        let path = root.join("retry.slv");
        let mut f = app();
        let mut volume = Volume::open(&path, &mut f).unwrap();
        std::fs::create_dir(&path).unwrap();
        assert!(volume.checkpoint(&f).is_err());
        assert!(path.is_dir());
        assert_eq!(std::fs::read_dir(&root).unwrap().count(), 2);
        std::fs::remove_dir(&path).unwrap();
        assert!(volume.checkpoint(&f).unwrap());
        assert_eq!(std::fs::read(&path).unwrap(), f.export_volume().unwrap());
        drop(volume);
        std::fs::remove_dir_all(root).unwrap();
    }
}
