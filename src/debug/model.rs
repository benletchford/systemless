use super::ids::{AddressSpaceId, ArtifactId, CaptureId, ContextId, OperationId, ProviderId};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ByteOrder {
    Big,
    Little,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AddressMappingKind {
    Ram,
    Rom,
    Mmio,
    Unmapped,
    Unknown,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct AddressAccess {
    pub read: bool,
    pub write: bool,
    pub execute: bool,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct AddressSpaceDescriptor {
    pub id: AddressSpaceId,
    pub name: String,
    pub address_bits: u16,
    pub byte_order: ByteOrder,
    pub mapping: AddressMappingKind,
    pub access: AddressAccess,
    pub supports_translation: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ContextLifecycle {
    Active,
    Suspended,
    Unavailable,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ExecutionContextDescriptor {
    pub id: ContextId,
    pub architecture: String,
    pub task: Option<String>,
    pub lifecycle: ContextLifecycle,
    pub address_spaces: Vec<AddressSpaceId>,
    pub capabilities: ContextCapabilities,
}

/// `{"kind":"active"}` resolves at request time; `{"kind":"id","id":1}` names
/// an installed context. IDs remain stable within a session generation.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ContextSelector {
    Active,
    Id { id: ContextId },
}

impl ContextSelector {
    pub fn active() -> Self {
        Self::Active
    }

    pub fn explicit(id: ContextId) -> Self {
        Self::Id { id }
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct ContextCapabilities {
    pub pause: bool,
    pub resume: bool,
    pub step: bool,
    pub exact_step: bool,
    pub set_breakpoint: bool,
    pub read_registers: bool,
    pub disassembly: bool,
    pub exact_disassembly: bool,
    pub stack_preview: bool,
    pub write_registers: bool,
    pub read_memory: bool,
    pub write_memory: bool,
    pub precise_breakpoints: bool,
    pub watchpoints: bool,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct AddressSpaceCapabilities {
    pub read: bool,
    pub write: bool,
    pub observational_reads: bool,
    pub code_cache_invalidation: bool,
    pub max_transfer_bytes: u64,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "reason", rename_all = "snake_case")]
pub enum StopReason {
    UserPause,
    StepCompleted,
    PcBreakpoint { id: super::ids::BreakpointId },
    CaptureBoundary { id: CaptureId },
    UnsupportedTransition,
    Watchpoint { id: super::ids::WatchpointId },
    TerminalHalt,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct StopInfo {
    pub id: u64,
    pub revision: u64,
    pub context: Option<ContextId>,
    pub location: Option<ExecutionLocation>,
    pub reason: StopReason,
    pub guest_tick: Option<u32>,
    pub execution_units: Option<u64>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum DebugExecutionState {
    Running,
    Paused { stop: StopInfo },
    Stepping { operation_id: OperationId },
}

impl DebugExecutionState {
    pub fn is_paused(&self) -> bool {
        matches!(self, DebugExecutionState::Paused { .. })
    }

    pub fn is_running(&self) -> bool {
        matches!(self, DebugExecutionState::Running)
    }

    pub fn is_stepping(&self) -> bool {
        matches!(self, DebugExecutionState::Stepping { .. })
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
pub struct DebugAddress {
    /// Address-space ID discovered through `ListAddressSpaces`.
    pub space: AddressSpaceId,
    /// Byte offset, serialized as a JSON number (not a hexadecimal string).
    pub offset: u64,
}

impl DebugAddress {
    pub fn new(space: AddressSpaceId, offset: u64) -> Self {
        Self { space, offset }
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
pub struct ExecutionLocation {
    pub context: ContextId,
    pub address: DebugAddress,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RegisterWidth {
    Bits8,
    Bits16,
    Bits32,
    Bits64,
    Bits128,
    Other(u16),
}

impl RegisterWidth {
    pub fn bits(self) -> u16 {
        match self {
            RegisterWidth::Bits8 => 8,
            RegisterWidth::Bits16 => 16,
            RegisterWidth::Bits32 => 32,
            RegisterWidth::Bits64 => 64,
            RegisterWidth::Bits128 => 128,
            RegisterWidth::Other(bits) => bits,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RegisterCategory {
    General,
    Address,
    ProgramCounter,
    StackPointer,
    Status,
    FloatingPoint,
    Vector,
    Control,
    Other,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RegisterRole {
    ProgramCounter,
    StackPointer,
    FramePointer,
    LinkRegister,
    ConditionCodes,
    Other,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct RegisterDescriptor {
    pub id: u32,
    pub name: String,
    pub width: RegisterWidth,
    pub category: RegisterCategory,
    pub role: Option<RegisterRole>,
    pub readable: bool,
    pub writable: bool,
    pub flags: Vec<RegisterFlag>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct RegisterFlag {
    pub bit: u16,
    pub name: String,
    pub description: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "encoding", rename_all = "snake_case")]
pub enum RegisterValue {
    Unsigned { value: u64, width_bits: u16 },
    // Big-endian register bytes.
    Bytes { bytes: Vec<u8>, width_bits: u16 },
}

impl RegisterValue {
    pub fn unsigned(value: u64, width_bits: u16) -> Self {
        if width_bits > 32 {
            Self::Bytes {
                bytes: value.to_be_bytes().to_vec(),
                width_bits,
            }
        } else {
            Self::Unsigned { value, width_bits }
        }
    }

    pub fn bytes(bytes: Vec<u8>, width_bits: u16) -> Self {
        Self::Bytes { bytes, width_bits }
    }

    pub fn as_u64(&self) -> Option<u64> {
        match self {
            RegisterValue::Unsigned { value, .. } => Some(*value),
            RegisterValue::Bytes { bytes, width_bits } if *width_bits <= 64 && bytes.len() <= 8 => {
                Some(
                    bytes
                        .iter()
                        .fold(0, |value, byte| (value << 8) | u64::from(*byte)),
                )
            }
            RegisterValue::Bytes { .. } => None,
        }
    }

    pub fn width_bits(&self) -> u16 {
        match self {
            RegisterValue::Unsigned { width_bits, .. }
            | RegisterValue::Bytes { width_bits, .. } => *width_bits,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct RegisterSnapshot {
    pub descriptor: RegisterDescriptor,
    pub value: RegisterValue,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct DisassemblyLine {
    pub address: DebugAddress,
    pub text: String,
    pub size: u8,
    pub approximate: bool,
    pub bytes: Vec<u8>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct StackPreview {
    pub context: ContextId,
    /// Lowest address covered by `bytes`, in the context's stack space.
    pub address: DebugAddress,
    pub bytes: Vec<u8>,
    pub stride: u8,
    pub truncated: bool,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct MemoryReadResult {
    pub address: DebugAddress,
    pub bytes: Vec<u8>,
    pub truncated: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SurfaceKind {
    GuestFramebuffer,
    HostPresentationMirror,
    Texture,
    BackingBuffer,
    Other,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PixelFormat {
    Mono1,
    Indexed2,
    Indexed4,
    Indexed8,
    Rgb555,
    Xrgb1555,
    Rgb888,
    Xrgb8888,
    Argb8888,
    Unknown,
}

impl PixelFormat {
    pub fn bits(self) -> Option<u16> {
        match self {
            PixelFormat::Mono1 => Some(1),
            PixelFormat::Indexed2 => Some(2),
            PixelFormat::Indexed4 => Some(4),
            PixelFormat::Indexed8 => Some(8),
            PixelFormat::Rgb555 | PixelFormat::Xrgb1555 => Some(16),
            PixelFormat::Rgb888 => Some(24),
            PixelFormat::Xrgb8888 | PixelFormat::Argb8888 => Some(32),
            PixelFormat::Unknown => None,
        }
    }

    pub fn is_indexed(self) -> bool {
        matches!(
            self,
            PixelFormat::Mono1
                | PixelFormat::Indexed2
                | PixelFormat::Indexed4
                | PixelFormat::Indexed8
        )
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct DisplayOutputDescriptor {
    pub id: u64,
    pub name: String,
    pub width: u32,
    pub height: u32,
    pub depth: u16,
    pub surfaces: Vec<u64>,
    pub palette_entries: u32,
    pub source: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct GraphicsSurfaceDescriptor {
    pub id: u64,
    pub name: String,
    pub kind: SurfaceKind,
    pub width: u32,
    pub height: u32,
    pub stride_bytes: u32,
    pub pixel_format: PixelFormat,
    pub byte_order: ByteOrder,
    pub base: Option<DebugAddress>,
    pub scale: u32,
    pub has_palette: bool,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct GraphicsSnapshot {
    pub schema_version: u32,
    pub outputs: Vec<DisplayOutputDescriptor>,
    pub surfaces: Vec<GraphicsSurfaceDescriptor>,
    pub palette_argb: Option<Vec<u32>>,
    pub truncated: bool,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ProviderObjectDescriptor {
    pub id: u64,
    pub kind: String,
    pub label: String,
    pub related: Vec<u64>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProviderSnapshotKind {
    State,
    Collection,
    Timeline,
    Image,
    Buffer,
    Record,
    Diagnostic,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct InspectionProviderDescriptor {
    pub id: ProviderId,
    pub name: String,
    pub schema_version: u32,
    pub observational: bool,
    pub snapshots: Vec<ProviderSnapshotKind>,
    pub collection_limit: usize,
    pub required_boundary: Option<CaptureBoundaryKind>,
    pub limitations: Vec<String>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CaptureBoundaryKind {
    /// Between runner calls; no presentation synchronization is implied.
    CommandSafePoint,
    /// After outer-frame chrome redraw and guest audio finalization.
    LogicalFrameComplete,
    /// Reserved; currently returns `BoundaryUnavailable` when requested.
    GuestPresentationRequest,
    /// Reserved; currently returns `BoundaryUnavailable` when requested.
    BackendReadbackReady,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct CaptureBoundaryCapabilities {
    pub kind: CaptureBoundaryKind,
    pub providers: Vec<ProviderId>,
    pub atomic_across_providers: bool,
    pub limitations: Vec<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ArtifactDescriptor {
    pub id: ArtifactId,
    pub provider: Option<ProviderId>,
    pub format: Option<String>,
    pub byte_len: u64,
    pub provenance: String,
    pub related: Vec<u64>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct DecodedArtifact {
    pub artifact: ArtifactId,
    pub format: Option<String>,
    pub metadata: Vec<(String, String)>,
    pub warnings: Vec<String>,
    pub preview_dimensions: Option<(u32, u32)>,
    pub preview_rgba8: Option<Vec<u8>>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct CaptureManifest {
    pub id: CaptureId,
    pub session: super::ids::SessionId,
    pub boundary: CaptureBoundaryKind,
    pub providers: Vec<ProviderId>,
    pub revision: u64,
    pub stop: Option<u64>,
    pub guest_tick: Option<u32>,
    pub execution_units: Option<u64>,
    pub artifacts: Vec<ArtifactDescriptor>,
    pub omitted: Vec<String>,
    pub truncated: bool,
    pub atomic: bool,
    pub synchronization: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct DebugNotification {
    pub sequence: u64,
    pub payload: NotificationPayload,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum NotificationPayload {
    ExecutionStateChanged {
        state: DebugExecutionState,
    },
    OperationCompleted {
        operation_id: OperationId,
        result: Box<OperationOutcome>,
    },
    CaptureAvailable {
        id: CaptureId,
    },
    ContextChanged {
        active: Option<ContextId>,
    },
    Invalidated {
        session: super::ids::SessionId,
    },
    TerminalState {
        halted: bool,
    },
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "outcome", rename_all = "snake_case")]
pub enum OperationOutcome {
    /// `stop` carries the stop this operation produced, so a stepping client
    /// does not need a follow-up `ExecutionStatus` to learn the new location.
    /// It is null for operations that complete without stopping execution.
    Completed { stop: Option<StopInfo> },
    Captured { capture_id: CaptureId },
    Failed { message: String },
    Cancelled,
    Unavailable { reason: String },
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct OperationStatus {
    pub id: OperationId,
    pub state: OperationState,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum OperationState {
    Pending,
    Completed { result: OperationOutcome },
    Expired,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn register_value_preserves_exact_bits() {
        let value = RegisterValue::unsigned(u64::MAX, 64);
        assert_eq!(value.as_u64(), Some(u64::MAX));

        let wide = RegisterValue::bytes(vec![0xAB; 16], 128);
        assert_eq!(wide.width_bits(), 128);
        assert_eq!(wide.as_u64(), None);
    }

    #[test]
    fn execution_state_classification() {
        let stop = StopInfo {
            id: 1,
            revision: 0,
            context: None,
            location: None,
            reason: StopReason::UserPause,
            guest_tick: None,
            execution_units: None,
        };
        assert!(DebugExecutionState::Paused { stop }.is_paused());
        assert!(DebugExecutionState::Running.is_running());
        assert!(DebugExecutionState::Stepping {
            operation_id: OperationId(1)
        }
        .is_stepping());
    }
}
