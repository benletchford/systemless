//! Shared game loading helpers.
//!
//! [`launch`] handles runner setup, application/archive ingestion (BinHex,
//! MacBinary, StuffIt, and web packs), VFS population, executable selection,
//! and post-load initialization.

mod application_icon;
pub mod installer_maker;
pub mod launch;
pub(crate) mod vise;

pub use application_icon::{
    application_icon_from_fork, loaded_application_identity, ApplicationIcon,
    ApplicationIconRepresentation, ApplicationIdentity,
};
pub use launch::{
    init_game, load_game, load_game_from_path, new_runner, pack_game_sources_for_web,
    pack_stuffit_for_web, WebPackLoader, MAX_INSTRUCTIONS_PER_FRAME, RAM_SIZE,
};
