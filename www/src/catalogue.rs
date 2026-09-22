use crate::paths::normalized_path;
use serde::Deserialize;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Game {
    pub id: &'static str,
    pub title: &'static str,
    pub description: &'static str,
    pub developer: &'static str,
    pub year: &'static str,
    pub architectures: &'static [GameArchitecture],
    pub default_architecture: GameArchitecture,
    pub category: &'static str,
    pub route: &'static str,
    pub approved: bool,
    pub route_aliases: &'static [&'static str],
    pub assets: GameAssets,
    pub settings: GameSettings,
    pub community: CommunityLinks,
    pub content_html: &'static str,
    pub license_html: &'static str,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GameArchitecture {
    M68k,
    PowerPc,
}

impl GameArchitecture {
    pub const fn label(self) -> &'static str {
        match self {
            Self::M68k => "68K",
            Self::PowerPc => "PowerPC",
        }
    }

    pub const fn key(self) -> &'static str {
        match self {
            Self::M68k => "68k",
            Self::PowerPc => "ppc",
        }
    }
}

impl Game {
    pub fn supports_architecture(self, architecture: GameArchitecture) -> bool {
        self.architectures.contains(&architecture)
    }
}

pub const GAME_ARCHITECTURES: &[GameArchitecture] =
    &[GameArchitecture::M68k, GameArchitecture::PowerPc];
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GameAssets {
    pub archive_path: &'static str,
    pub archive_download_name: &'static str,
    pub web_pack_path: Option<&'static str>,
    pub screenshot_path: &'static str,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GameSettings {
    pub worker: bool,
    pub key_mappings: &'static [(&'static str, &'static str)],
    pub arrows_as_numpad: bool,
    pub launch_modifiers: &'static [LaunchModifier],
    pub mobile_controls: MobileControls,
    pub show_menu_bar: bool,
    pub application_partition_size: Option<u32>,
    pub remove_paths: &'static [&'static str],
    pub file_mappings: &'static [(&'static str, &'static str)],
    pub runtime_pacing: RuntimePacing,
    pub plugins: &'static [GamePlugin],
}

#[derive(Clone, Copy, Debug, serde::Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum LaunchModifier {
    Command,
    Shift,
    Option,
    Control,
}

impl LaunchModifier {
    /// Macintosh ADB virtual keycode for this modifier.
    ///
    /// Inside Macintosh Volume V (1986), Toolbox Event Manager, p. V-190.
    pub const fn mac_key_code(self) -> u8 {
        match self {
            Self::Command => 0x37,
            Self::Shift => 0x38,
            Self::Option => 0x3A,
            Self::Control => 0x3B,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GamePlugin {
    pub id: &'static str,
    pub label: &'static str,
    pub description: &'static str,
    pub download_path: &'static str,
    pub download_name: &'static str,
    pub size_bytes: u32,
    pub install_assets: &'static [GamePluginAsset],
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GamePluginAsset {
    pub asset_path: &'static str,
    pub mount_path: &'static str,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MobileControls {
    pub enabled: bool,
    pub joystick: MobileJoystickControls,
    pub buttons: &'static [MobileControlButton],
    pub button_groups: &'static [MobileControlButtonGroup],
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MobileJoystickControls {
    pub up: &'static str,
    pub down: &'static str,
    pub left: &'static str,
    pub right: &'static str,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MobileControlButton {
    pub label: &'static str,
    pub key: &'static str,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MobileControlButtonGroup {
    pub label: &'static str,
    pub buttons: &'static [MobileControlButton],
}

#[derive(Clone, Copy, Debug, serde::Serialize, Deserialize, PartialEq, Eq)]
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

include!(concat!(env!("OUT_DIR"), "/games.rs"));

pub fn games() -> &'static [Game] {
    GAMES
}

pub fn categories(show_unapproved: bool) -> Vec<&'static str> {
    library_games(show_unapproved)
        .map(|g| g.category)
        .collect::<std::collections::BTreeSet<_>>()
        .into_iter()
        .collect()
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CommunityLinks {
    pub source: &'static str,
    pub edit: &'static str,
    pub metadata_issue: &'static str,
    pub compatibility_issue: &'static str,
    pub takedown_request: &'static str,
    pub open_reports: &'static str,
    pub suggest_controls_config: &'static str,
}

pub const NEXT_LIBRARY_ROUTE: &str = "/next";

pub fn default_game() -> &'static Game {
    games()
        .iter()
        .find(|game| game.approved)
        .unwrap_or(&games()[0])
}

pub fn library_games(show_unapproved: bool) -> impl Iterator<Item = &'static Game> {
    games()
        .iter()
        .filter(move |game| show_unapproved || game.approved)
}

pub fn is_next_library_path(path: &str) -> bool {
    normalized_path(path) == NEXT_LIBRARY_ROUTE
}

pub fn library_path_for_game(game: &Game, _show_unapproved: bool) -> String {
    normalized_path(game.route)
}

pub fn canonical_path_for_game(game: &Game) -> String {
    normalized_path(game.route)
}

pub fn game_from_path(path: &str) -> Option<&'static Game> {
    let normalized = normalized_path(path);
    if let Some(next_route) = next_route_from_path(&normalized) {
        return games().iter().find(|game| route_matches(game, &next_route));
    }

    games().iter().find(|game| route_matches(game, &normalized))
}

fn route_matches(game: &Game, normalized_route: &str) -> bool {
    normalized_route == normalized_path(game.route)
        || game
            .route_aliases
            .iter()
            .any(|alias| normalized_route == normalized_path(alias))
}

fn next_route_from_path(normalized_path: &str) -> Option<String> {
    if normalized_path == NEXT_LIBRARY_ROUTE {
        return None;
    }
    normalized_path
        .strip_prefix(&format!("{NEXT_LIBRARY_ROUTE}/"))
        .map(|route| format!("/{route}"))
}
