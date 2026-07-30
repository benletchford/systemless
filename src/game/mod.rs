//! Shared game loading helpers.
//!
//! [`launch`] handles runner setup and ROM/archive ingestion (MacBinary,
//! StuffIt, web-pack), VFS population, executable selection, post-load
//! init.

pub mod launch;

pub use launch::{
    init_game, load_game, load_game_from_path, new_runner, pack_game_sources_for_web,
    pack_stuffit_for_web, WebPackLoader, MAX_INSTRUCTIONS_PER_FRAME, RAM_SIZE,
};
