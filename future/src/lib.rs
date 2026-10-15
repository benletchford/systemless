pub mod abi;
pub mod bitmap;
pub mod calendar;
mod callback;
pub mod cfm;
pub mod controls;
pub mod cpu;
pub mod cursor;
pub mod decimal;
pub mod dialogs;
pub mod environment;
pub mod events;
pub mod files;
pub mod fixtures;
pub mod font;
mod font_fallback;
pub mod graphics;
pub mod host_input;
pub mod integer;
mod jump;
mod lifecycle;
pub mod lines;
pub mod load;
pub mod memory;
pub mod menus;
pub mod picture;
pub mod polygons;
pub mod process;
pub mod quickdraw;
pub mod rectangles;
pub mod region_geometry;
pub mod regions;
pub mod rounded;
pub mod runtime;
pub mod sound;
pub mod standard_file;
mod stdlib;
pub mod text;
pub mod textedit;
pub mod traps;
pub mod windows;

pub type Result<T> = std::result::Result<T, Error>;
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("application load: unresolved import {library}:{symbol}")]
    UnresolvedImport {
        library: String,
        symbol: String,
        library_bytes: Vec<u8>,
    },
    #[error("application load: {0}")]
    Load(String),
    #[error("invalid/overlapping mapping at {0:#x}")]
    Mapping(u32),
    #[error("guest {access:?} fault at {address:#x}")]
    Memory {
        address: u32,
        access: memory::Access,
    },
    #[error("unaligned instruction at {0:#x}")]
    Alignment(u32),
    #[error("CPU fault: {0}")]
    Cpu(String),
    #[error("unsupported service {0:#x}")]
    Service(u32),
    #[error("graphics: {0}")]
    Graphics(String),
    #[error("invalid fixture: {0}")]
    Fixture(String),
}
