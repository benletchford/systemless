use super::ids::{AddressSpaceId, ContextId, ProviderId};
use super::model::{
    AddressSpaceCapabilities, CaptureBoundaryCapabilities, CaptureBoundaryKind, ContextCapabilities,
};
use serde::{Deserialize, Serialize};

/// Tagged with `kind`, matching [`ContextSelector`](super::model::ContextSelector):
/// `{"kind":"context","id":1}` (also `address_space` and `provider`),
/// `{"kind":"boundary","boundary":"logical_frame_complete"}`, or
/// `{"kind":"decoder","format":"rgba8"}`. Decoder support is currently absent.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum CapabilityTarget {
    Context { id: ContextId },
    AddressSpace { id: AddressSpaceId },
    Provider { id: ProviderId },
    Boundary { boundary: CaptureBoundaryKind },
    Decoder { format: String },
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ProviderCapabilities {
    pub observational: bool,
    pub snapshots: Vec<super::model::ProviderSnapshotKind>,
    /// Maximum entries a collection-valued snapshot may carry. A per-provider
    /// ergonomic bound: high enough that a real collection survives intact.
    /// Memory is bounded by `max_snapshot_bytes`, not by this count.
    pub collection_limit: usize,
    /// Byte budget for a snapshot, mirroring `max_artifact_bytes` for artifacts.
    /// This is the bound that actually prevents memory blowup; a provider
    /// truncates on whichever of the two limits trips first.
    pub max_snapshot_bytes: u64,
    pub max_artifact_bytes: u64,
    pub relationships: Vec<String>,
    pub limitations: Vec<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct DecoderCapabilities {
    pub formats: Vec<String>,
    pub max_output_bytes: u64,
    pub max_recursion: u16,
    pub max_decode_millis: u32,
    pub supports_preview: bool,
    pub limitations: Vec<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum CapabilitySet {
    Context(ContextCapabilities),
    AddressSpace(AddressSpaceCapabilities),
    Provider(ProviderCapabilities),
    Boundary(CaptureBoundaryCapabilities),
    Decoder(DecoderCapabilities),
}

impl Default for ProviderCapabilities {
    fn default() -> Self {
        Self {
            observational: true,
            snapshots: Vec::new(),
            collection_limit: super::provider::DEFAULT_COLLECTION_LIMIT,
            max_snapshot_bytes: super::provider::DEFAULT_SNAPSHOT_BYTES,
            max_artifact_bytes: 16 * 1024 * 1024,
            relationships: Vec::new(),
            limitations: Vec::new(),
        }
    }
}

impl Default for DecoderCapabilities {
    fn default() -> Self {
        Self {
            formats: Vec::new(),
            max_output_bytes: 64 * 1024 * 1024,
            max_recursion: 32,
            max_decode_millis: 2_000,
            supports_preview: false,
            limitations: Vec::new(),
        }
    }
}
