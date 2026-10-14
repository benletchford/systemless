pub mod abi;
pub mod cpu;
pub mod fixtures;
pub mod graphics;
mod lifecycle;
pub mod load;
pub mod memory;
pub mod process;
pub mod runtime;

pub type Result<T> = std::result::Result<T, Error>;
#[derive(Debug, thiserror::Error)]
pub enum Error {
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
