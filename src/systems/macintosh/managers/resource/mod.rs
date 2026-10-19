//! Resource fork parsing

mod ajcp;
mod bytes;
mod compressed;
mod parser;
pub mod quilt;

pub use bytes::ResourceBytes;

pub use parser::{
    serialize_resource_fork, serialize_resource_fork_with_attrs, ResourceFork, ResourceForkEntry,
};

