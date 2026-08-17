//! Ephemeral macOS application bundle used for the Dock-facing guest identity.

use std::ffi::OsStr;
use std::fs;
use std::io;
use std::os::unix::ffi::OsStrExt;
use std::os::unix::fs::symlink;
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::UNIX_EPOCH;

const RELAUNCH_ENV: &str = "SYSTEMLESS_NATIVE_APP_BUNDLE";
const CACHE_DIRECTORY: &str = "systemless-native-apps-v2";
const CACHE_RECORD: &str = "bundle-name";
const BUNDLE_EXECUTABLE: &str = "systemless";

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NativeBundle {
    pub bundle_path: PathBuf,
    pub executable_path: PathBuf,
}

pub fn already_relaunched() -> bool {
    relaunch_marker_present(std::env::var_os(RELAUNCH_ENV).as_deref())
}

fn relaunch_marker_present(value: Option<&OsStr>) -> bool {
    value.is_some()
}

/// Return a complete cached bundle for this archive and runner executable.
pub fn cached_bundle(game_path: &Path) -> io::Result<Option<NativeBundle>> {
    let source = bundle_source(game_path)?;
    let bundle_directory = match fs::read_to_string(source.cache_root.join(CACHE_RECORD)) {
        Ok(name) if valid_bundle_directory(&name) => name,
        Ok(_) => return Ok(None),
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error),
    };
    let layout = bundle_layout(source, &bundle_directory);
    let plist = match fs::read_to_string(&layout.info_plist) {
        Ok(plist) => plist,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error),
    };
    if !plist.contains("<key>CFBundleDisplayName</key>")
        || !plist.contains(&format!("<string>{}</string>", layout.bundle_identifier))
        || !plist.contains(&format!("<string>{BUNDLE_EXECUTABLE}</string>"))
    {
        return Ok(None);
    }
    let target = match fs::read_link(&layout.bundle.executable_path) {
        Ok(target) => target,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error),
    };
    Ok((target == layout.current_exe).then_some(layout.bundle))
}

/// Create or refresh the tiny bundle that gives Launch Services the guest name.
pub fn prepare_bundle(game_path: &Path, guest_name: &str) -> io::Result<NativeBundle> {
    let display_name = normalize_display_name(guest_name);
    let bundle_directory = bundle_directory(&display_name);
    let layout = bundle_layout(bundle_source(game_path)?, &bundle_directory);
    let contents = layout.bundle.bundle_path.join("Contents");
    let macos = contents.join("MacOS");
    fs::create_dir_all(&macos)?;

    let plist = info_plist(&display_name, &layout.bundle_identifier);
    write_atomic_if_changed(&layout.info_plist, plist.as_bytes())?;
    replace_symlink(&layout.bundle.executable_path, &layout.current_exe)?;
    write_atomic_if_changed(&layout.cache_record, bundle_directory.as_bytes())?;

    Ok(layout.bundle)
}

/// Replace the current process with the executable entered through `bundle`.
///
/// A successful call never returns. The returned error is safe to report before
/// continuing with the ordinary unbundled GUI startup.
pub fn exec_bundle(bundle: &NativeBundle) -> io::Error {
    let mut command = Command::new(&bundle.executable_path);
    command
        .args(std::env::args_os().skip(1))
        .env(RELAUNCH_ENV, "1");
    command.exec()
}

struct BundleLayout {
    bundle: NativeBundle,
    info_plist: PathBuf,
    current_exe: PathBuf,
    bundle_identifier: String,
    cache_record: PathBuf,
}

struct BundleSource {
    cache_root: PathBuf,
    current_exe: PathBuf,
    bundle_identifier: String,
}

fn bundle_source(game_path: &Path) -> io::Result<BundleSource> {
    let current_exe = std::env::current_exe()?;
    let fingerprint = source_fingerprint(game_path, &current_exe)?;
    Ok(BundleSource {
        cache_root: std::env::temp_dir()
            .join(CACHE_DIRECTORY)
            .join(format!("{fingerprint:016x}")),
        current_exe,
        bundle_identifier: format!("org.systemless.guest.{fingerprint:016x}"),
    })
}

fn bundle_layout(source: BundleSource, bundle_directory: &str) -> BundleLayout {
    let bundle_path = source.cache_root.join(bundle_directory);
    let executable_path = bundle_path
        .join("Contents")
        .join("MacOS")
        .join(BUNDLE_EXECUTABLE);
    let info_plist = bundle_path.join("Contents").join("Info.plist");
    BundleLayout {
        bundle: NativeBundle {
            bundle_path,
            executable_path,
        },
        info_plist,
        current_exe: source.current_exe,
        bundle_identifier: source.bundle_identifier,
        cache_record: source.cache_root.join(CACHE_RECORD),
    }
}

fn bundle_directory(display_name: &str) -> String {
    format!("{}.app", display_name.replace('/', "∕"))
}

fn valid_bundle_directory(name: &str) -> bool {
    !name.is_empty() && !name.contains('/') && name.ends_with(".app")
}

fn source_fingerprint(game_path: &Path, current_exe: &Path) -> io::Result<u64> {
    let canonical_game = game_path.canonicalize()?;
    let metadata = canonical_game.metadata()?;
    let modified = metadata
        .modified()
        .ok()
        .and_then(|value| value.duration_since(UNIX_EPOCH).ok());
    let mut hash = Fnv64::new();
    hash.write(canonical_game.as_os_str().as_bytes());
    hash.write(&metadata.len().to_le_bytes());
    if let Some(modified) = modified {
        hash.write(&modified.as_secs().to_le_bytes());
        hash.write(&modified.subsec_nanos().to_le_bytes());
    }
    hash.write(current_exe.as_os_str().as_bytes());
    hash.write(env!("CARGO_PKG_VERSION").as_bytes());
    for key in ["SYSTEMLESS_LOAD_EXECUTABLE", "SYSTEMLESS_PREFER_POWERPC"] {
        hash.write(key.as_bytes());
        if let Some(value) = std::env::var_os(key) {
            hash.write(value.as_bytes());
        }
    }
    Ok(hash.finish())
}

#[derive(Clone, Copy)]
struct Fnv64(u64);

impl Fnv64 {
    const fn new() -> Self {
        Self(0xcbf2_9ce4_8422_2325)
    }

    fn write(&mut self, bytes: &[u8]) {
        for &byte in bytes {
            self.0 ^= u64::from(byte);
            self.0 = self.0.wrapping_mul(0x0000_0100_0000_01b3);
        }
    }

    const fn finish(self) -> u64 {
        self.0
    }
}

fn normalize_display_name(name: &str) -> String {
    let normalized = name
        .chars()
        .map(|character| {
            if matches!(character, '\t' | '\n' | '\r')
                || ('\u{20}'..='\u{d7ff}').contains(&character)
                || ('\u{e000}'..='\u{fffd}').contains(&character)
                || ('\u{10000}'..='\u{10ffff}').contains(&character)
            {
                character
            } else {
                ' '
            }
        })
        .collect::<String>();
    let normalized = normalized.trim();
    if normalized.is_empty() {
        "Systemless".to_owned()
    } else {
        normalized.to_owned()
    }
}

fn info_plist(display_name: &str, bundle_identifier: &str) -> String {
    let display_name = escape_xml(display_name);
    let bundle_identifier = escape_xml(bundle_identifier);
    format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n\
<!DOCTYPE plist PUBLIC \"-//Apple//DTD PLIST 1.0//EN\" \
\"http://www.apple.com/DTDs/PropertyList-1.0.dtd\">\n\
<plist version=\"1.0\">\n\
<dict>\n\
    <key>CFBundleDisplayName</key>\n\
    <string>{display_name}</string>\n\
    <key>CFBundleExecutable</key>\n\
    <string>{BUNDLE_EXECUTABLE}</string>\n\
    <key>CFBundleIdentifier</key>\n\
    <string>{bundle_identifier}</string>\n\
    <key>CFBundleInfoDictionaryVersion</key>\n\
    <string>6.0</string>\n\
    <key>CFBundleName</key>\n\
    <string>{display_name}</string>\n\
    <key>CFBundlePackageType</key>\n\
    <string>APPL</string>\n\
    <key>CFBundleShortVersionString</key>\n\
    <string>{version}</string>\n\
    <key>CFBundleVersion</key>\n\
    <string>{version}</string>\n\
</dict>\n\
</plist>\n",
        version = env!("CARGO_PKG_VERSION")
    )
}

fn escape_xml(value: &str) -> String {
    let mut escaped = String::with_capacity(value.len());
    for character in value.chars() {
        match character {
            '&' => escaped.push_str("&amp;"),
            '<' => escaped.push_str("&lt;"),
            '>' => escaped.push_str("&gt;"),
            '\"' => escaped.push_str("&quot;"),
            '\'' => escaped.push_str("&apos;"),
            _ => escaped.push(character),
        }
    }
    escaped
}

fn write_atomic_if_changed(path: &Path, bytes: &[u8]) -> io::Result<()> {
    if fs::read(path).ok().as_deref() == Some(bytes) {
        return Ok(());
    }
    let temporary = sibling_temporary_path(path);
    fs::write(&temporary, bytes)?;
    fs::rename(temporary, path)
}

fn replace_symlink(path: &Path, target: &Path) -> io::Result<()> {
    if fs::read_link(path).ok().as_deref() == Some(target) {
        return Ok(());
    }
    let temporary = sibling_temporary_path(path);
    match fs::remove_file(&temporary) {
        Ok(()) => {}
        Err(error) if error.kind() == io::ErrorKind::NotFound => {}
        Err(error) => return Err(error),
    }
    symlink(target, &temporary)?;
    fs::rename(temporary, path)
}

fn sibling_temporary_path(path: &Path) -> PathBuf {
    let mut name = path
        .file_name()
        .unwrap_or_else(|| OsStr::new("bundle"))
        .to_os_string();
    name.push(format!(".{}.tmp", std::process::id()));
    path.with_file_name(name)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn display_name_preserves_unicode_and_replaces_xml_controls() {
        assert_eq!(
            normalize_display_name("  Tomb Raider I Demo™\u{0}  "),
            "Tomb Raider I Demo™"
        );
        assert_eq!(normalize_display_name("\u{1}\u{2}"), "Systemless");
    }

    #[test]
    fn bundle_directory_uses_the_guest_name() {
        assert_eq!(
            bundle_directory("Tomb Raider I Demo"),
            "Tomb Raider I Demo.app"
        );
        assert_eq!(bundle_directory("A/B"), "A∕B.app");
        assert!(valid_bundle_directory("Tomb Raider I Demo.app"));
        assert!(!valid_bundle_directory("../Tomb Raider I Demo.app"));
    }

    #[test]
    fn plist_escapes_guest_name_and_declares_application_bundle() {
        let plist = info_plist("Myst & Riven <Demo>", "org.systemless.guest.0123");
        assert!(plist.contains("<string>Myst &amp; Riven &lt;Demo&gt;</string>"));
        assert!(plist.contains("<key>CFBundlePackageType</key>\n<string>APPL</string>"));
        assert!(plist.contains("<key>CFBundleExecutable</key>\n<string>systemless</string>"));
    }

    #[test]
    fn fnv_fingerprint_is_stable_and_order_sensitive() {
        let mut first = Fnv64::new();
        first.write(b"archive");
        first.write(b"runner");
        let mut second = Fnv64::new();
        second.write(b"archive");
        second.write(b"runner");
        let mut reversed = Fnv64::new();
        reversed.write(b"runner");
        reversed.write(b"archive");
        assert_eq!(first.finish(), second.finish());
        assert_ne!(second.finish(), reversed.finish());
    }

    #[test]
    fn any_internal_relaunch_marker_prevents_another_exec() {
        assert!(!relaunch_marker_present(None));
        assert!(relaunch_marker_present(Some(OsStr::new("1"))));
    }

    #[test]
    fn prepared_bundle_contains_plist_and_runner_symlink() {
        let temporary = tempfile::tempdir().unwrap();
        let game_path = temporary.path().join("game.sit");
        fs::write(&game_path, b"archive").unwrap();

        let bundle = prepare_bundle(&game_path, "Tomb Raider I Demo").unwrap();
        let plist = fs::read_to_string(bundle.bundle_path.join("Contents/Info.plist")).unwrap();

        assert_eq!(
            bundle.bundle_path.file_name().unwrap(),
            "Tomb Raider I Demo.app"
        );
        assert!(plist.contains("<string>Tomb Raider I Demo</string>"));
        assert_eq!(
            fs::read_link(&bundle.executable_path).unwrap(),
            std::env::current_exe().unwrap()
        );
        assert_eq!(cached_bundle(&game_path).unwrap(), Some(bundle.clone()));
        fs::remove_dir_all(bundle.bundle_path.parent().unwrap()).unwrap();
    }

    #[test]
    fn incomplete_cached_plist_is_rejected() {
        let temporary = tempfile::tempdir().unwrap();
        let game_path = temporary.path().join("game.sit");
        fs::write(&game_path, b"archive").unwrap();
        let bundle = prepare_bundle(&game_path, "Glider Demo").unwrap();
        fs::write(bundle.bundle_path.join("Contents/Info.plist"), b"<plist/>").unwrap();

        assert_eq!(cached_bundle(&game_path).unwrap(), None);
        fs::remove_dir_all(bundle.bundle_path.parent().unwrap()).unwrap();
    }
}
