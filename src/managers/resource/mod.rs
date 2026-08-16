//! Resource fork parsing

mod ajcp;
mod compressed;
mod parser;

pub use parser::{
    serialize_resource_fork, serialize_resource_fork_with_attrs, ResourceFork, ResourceForkEntry,
};
