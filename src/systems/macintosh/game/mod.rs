//! Shared game loading helpers.
//!
//! [`launch`] handles runner setup, application/archive ingestion (BinHex,
//! MacBinary, StuffIt, and web packs), VFS population, executable selection,
//! and post-load initialization.

mod application_icon;
pub mod installer_maker;
pub mod launch;
pub mod ppc_import_audit;
pub(crate) mod smaller_installer;
mod stuffit_paths;
pub(crate) mod vise;

pub use application_icon::{
    application_icon_from_fork, loaded_application_identity, ApplicationIcon,
    ApplicationIconRepresentation, ApplicationIdentity,
};
pub use launch::{
    init_game, init_game_owned, load_game, load_game_from_path, new_runner, new_runner_with_addressing,
    new_runner_with_configuration, new_runner_with_display, new_runner_with_screen_depth, pack_game_sources_for_web,
    pack_stuffit_for_web, WebPackLoader, MAX_INSTRUCTIONS_PER_FRAME, RAM_SIZE,
};
