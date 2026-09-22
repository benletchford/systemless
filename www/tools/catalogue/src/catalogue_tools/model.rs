use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Authoring version; publication and storage/journal versions are independent.
pub const SCHEMA_VERSION: u32 = 1;
/// Resolved entries, asset records, launch policy and generic plugin declarations.
pub const COMPILED_SCHEMA_VERSION: u32 = 2;
/// Stable category labels exposed by the public library filters.
pub const CATEGORIES: &[&str] = &["Arcade", "FPS", "Puzzle", "Space Trading", "Strategy"];

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    pub schema_version: u32,
    pub repository: String,
    pub branch: String,
    pub asset_base_url: String,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            schema_version: SCHEMA_VERSION,
            repository: "https://github.com/benletchford/systemless".into(),
            branch: "master".into(),
            asset_base_url: "https://assets.systemless.org".into(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Entry {
    pub id: String,
    pub kind: Kind,
    pub title: String,
    pub summary: String,
    pub developer: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub publisher: Option<String>,
    pub year: u16,
    pub architectures: Vec<Architecture>,
    pub default_architecture: Architecture,
    pub category: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub route: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub aliases: Vec<String>,
    /// Enables the launcher, independently of listing and compatibility verification.
    #[serde(default, skip_serializing_if = "is_default")]
    pub launch_enabled: bool,
    pub compatibility: Compatibility,
    #[serde(
        default,
        skip_serializing_if = "is_default",
        serialize_with = "serialize_overrides"
    )]
    pub runtime: Runtime,
    #[serde(
        default,
        skip_serializing_if = "is_default",
        serialize_with = "serialize_overrides"
    )]
    pub controls: Controls,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub artifacts: Vec<Artifact>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub plugins: Vec<Plugin>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub references: Vec<String>,
}

/// Optional downloads and installation files are declared assets, so provenance,
/// promotion and reconciliation apply to plugin files exactly as to the game.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Plugin {
    pub id: String,
    pub label: String,
    pub description: String,
    pub download_artifact: String,
    #[serde(default)]
    pub install: Vec<PluginInstall>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PluginInstall {
    pub artifact: String,
    pub mount_path: String,
}

/// A chunk of plugins for one catalogue entry. Multiple files may target the
/// same entry so large collections can stay reviewable.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PluginCollection {
    pub schema_version: u32,
    pub entry: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub artifacts: Vec<Artifact>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub plugins: Vec<Plugin>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LaunchModifier {
    Command,
    Shift,
    Option,
    Control,
}

fn is_default<T: Default + PartialEq>(value: &T) -> bool {
    value == &T::default()
}

/// Compact only the authoring representation. Runtime/Controls serialize with
/// resolved defaults everywhere else, including the dedicated website model.
fn serialize_overrides<T: Serialize + Default, S: serde::Serializer>(
    value: &T,
    serializer: S,
) -> Result<S::Ok, S::Error> {
    fn retain_overrides(value: &mut serde_json::Value, defaults: &serde_json::Value) {
        if let (Some(fields), Some(defaults)) = (value.as_object_mut(), defaults.as_object()) {
            fields.retain(|key, value| {
                if let Some(default) = defaults.get(key) {
                    if value == default {
                        return false;
                    }
                    retain_overrides(value, default);
                }
                true
            });
        }
    }
    let mut value = serde_json::to_value(value).map_err(serde::ser::Error::custom)?;
    let defaults = serde_json::to_value(T::default()).map_err(serde::ser::Error::custom)?;
    retain_overrides(&mut value, &defaults);
    value.serialize(serializer)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    Game,
    Application,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Architecture {
    #[serde(rename = "68k")]
    M68k,
    #[serde(rename = "ppc")]
    Ppc,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Compatibility {
    pub status: Status,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub verified: Vec<Verification>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Status {
    Unknown,
    Boots,
    Playable,
    Works,
    Broken,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Verification {
    pub date: String,
    pub tester: String,
    pub systemless_version: String,
    pub architecture: Architecture,
    pub environment: String,
    pub status: Status,
    pub evidence: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Runtime {
    pub worker: bool,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub launch_modifiers: Vec<LaunchModifier>,
    pub show_menu_bar: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub application_partition_size: Option<u32>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub remove_paths: Vec<String>,
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub file_mappings: BTreeMap<String, String>,
    pub runtime_pacing: RuntimePacing,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct RuntimePacing {
    pub max_ticks_per_paint: u32,
    pub reset_slack_ticks: u32,
    pub cpu_mhz: u32,
}
impl Default for RuntimePacing {
    fn default() -> Self {
        Self {
            max_ticks_per_paint: 2,
            reset_slack_ticks: 4,
            cpu_mhz: 25,
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Controls {
    pub arrows_as_numpad: bool,
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub key_mappings: BTreeMap<String, String>,
    pub mobile: MobileControls,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct MobileControls {
    pub enabled: bool,
    pub joystick: Joystick,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub buttons: Vec<Button>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub button_groups: Vec<ButtonGroup>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Joystick {
    pub up: String,
    pub down: String,
    pub left: String,
    pub right: String,
}
impl Default for Joystick {
    fn default() -> Self {
        Self {
            up: "ArrowUp".into(),
            down: "ArrowDown".into(),
            left: "ArrowLeft".into(),
            right: "ArrowRight".into(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Button {
    pub label: String,
    pub key: String,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ButtonGroup {
    pub label: String,
    pub buttons: Vec<Button>,
}

/// Every managed screenshot and software file uses this same ingestion model.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Artifact {
    pub id: String,
    pub role: ArtifactRole,
    pub format: FileType,
    pub source: AssetSource,
    pub provenance: Provenance,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ArtifactRole {
    Screenshot,
    Archive,
    WebPack,
    Supplement,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum AssetSource {
    Incoming {
        path: String,
    },
    Url {
        url: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        download_page: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        expected_sha256: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        expected_size: Option<u64>,
    },
    Sha256 {
        sha256: String,
        size_bytes: u64,
    },
    /// A deliberate link to a third-party download, outside R2 management.
    External {
        url: String,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum FileType {
    Png,
    Jpg,
    Gif,
    Webp,
    Sit,
    Sitx,
    Zip,
    Kpk,
    Bin,
    Hqx,
    Img,
    Gz,
    Opaque,
}
impl FileType {
    pub fn ext(self) -> &'static str {
        match self {
            Self::Png => "png",
            Self::Jpg => "jpg",
            Self::Gif => "gif",
            Self::Webp => "webp",
            Self::Sit => "sit",
            Self::Sitx => "sitx",
            Self::Zip => "zip",
            Self::Kpk => "kpk",
            Self::Bin => "bin",
            Self::Hqx => "hqx",
            Self::Img => "img",
            Self::Gz => "gz",
            Self::Opaque => "dat",
        }
    }
    pub fn media(self) -> bool {
        matches!(self, Self::Png | Self::Jpg | Self::Gif | Self::Webp)
    }
    pub fn mime(self) -> &'static str {
        match self {
            Self::Png => "image/png",
            Self::Jpg => "image/jpeg",
            Self::Gif => "image/gif",
            Self::Webp => "image/webp",
            Self::Zip => "application/zip",
            Self::Gz => "application/gzip",
            Self::Sit => "application/x-stuffit",
            Self::Sitx => "application/x-stuffitx",
            Self::Hqx => "application/mac-binhex40",
            _ => "application/octet-stream",
        }
    }
    pub fn limit(self) -> u64 {
        if self.media() {
            20 * 1024 * 1024
        } else {
            2 * 1024 * 1024 * 1024
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Provenance {
    pub redistribution: Redistribution,
    /// The hosted software bytes are the unchanged upstream distributable.
    #[serde(default, skip_serializing_if = "is_default")]
    pub original: bool,
    /// A screenshot contains only the software's own content surface.
    #[serde(default, skip_serializing_if = "is_default")]
    pub content_only: bool,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub sources: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub license: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rights_holder: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub permission: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub notes: Option<String>,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Redistribution {
    Unknown,
    Permitted,
    Forbidden,
}
