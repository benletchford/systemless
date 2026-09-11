use super::capabilities::{CapabilitySet, CapabilityTarget};
use super::ids::{
    AddressSpaceId, ArtifactId, BreakpointId, CaptureId, ContextId, OperationId, ProviderId,
    SessionId,
};
use super::model::{
    AddressSpaceDescriptor, ArtifactDescriptor, CaptureBoundaryKind, CaptureManifest,
    ContextSelector, DebugAddress, DebugExecutionState, DecodedArtifact, DisassemblyLine,
    ExecutionContextDescriptor, InspectionProviderDescriptor, MemoryReadResult, OperationStatus,
    RegisterSnapshot, StackPreview,
};
use serde::{Deserialize, Serialize};

/// Serialized as `{"kind":"program_counter"}`; currently the only breakpoint kind.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum BreakpointKind {
    ProgramCounter,
}

/// A PC stop before execution. Duplicate context/address targets are rejected.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct BreakpointSpec {
    pub kind: BreakpointKind,
    /// Explicit executable space, or space `0` when the context has exactly one.
    pub address: DebugAddress,
    /// `None` resolves to the active context when installed.
    pub context: Option<ContextId>,
    /// Disabled breakpoints remain listed but do not stop execution.
    pub enabled: bool,
    /// Remove this breakpoint after its first hit.
    pub one_shot: bool,
}

impl BreakpointSpec {
    pub fn program_counter(address: u64) -> Self {
        Self {
            kind: BreakpointKind::ProgramCounter,
            address: DebugAddress::new(AddressSpaceId::UNSPECIFIED, address),
            context: None,
            enabled: true,
            one_shot: false,
        }
    }

    pub fn program_counter_at(address: DebugAddress) -> Self {
        Self {
            kind: BreakpointKind::ProgramCounter,
            address,
            context: None,
            enabled: true,
            one_shot: false,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct BreakpointInfo {
    pub id: BreakpointId,
    pub spec: BreakpointSpec,
    pub hit_count: u64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CaptureMode {
    /// Copy at the current command safe point without changing execution state.
    CurrentState,
    /// Resume if necessary and copy at the next selected boundary.
    NextBoundary,
    /// Like `NextBoundary`, but pause at the selected boundary before copying.
    PauseAtBoundary,
}

/// Select a capture. Rust `Default` does not supply missing required JSON fields:
/// send `mode`, `providers`, and `include_artifacts`; `boundary` may be null/omitted.
/// Future-boundary modes reject terminal runners and pending steps.
/// At most 64 future captures may be pending concurrently.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct CaptureRequest {
    pub mode: CaptureMode,
    /// Ignored for `CurrentState`; otherwise defaults to `LogicalFrameComplete`.
    /// `CommandSafePoint` completes immediately, without resuming a paused runner.
    pub boundary: Option<CaptureBoundaryKind>,
    /// Empty selects graphics only. At most 64 IDs; duplicates are coalesced.
    pub providers: Vec<ProviderId>,
    /// Retain provider artifacts as well as snapshots; failures appear in omissions.
    pub include_artifacts: bool,
}

impl Default for CaptureRequest {
    fn default() -> Self {
        Self {
            mode: CaptureMode::CurrentState,
            boundary: None,
            providers: Vec::new(),
            include_artifacts: true,
        }
    }
}

/// Shared in-process and socket requests, serialized with a snake-case `op` tag.
/// Optional fields accept null/omission; other fields are required. IDs come from
/// discovery replies, not transport envelope IDs. See the [module](crate::debug)
/// for socket framing and a minimal client workflow.
///
/// Example payloads (inside the socket envelope's `request` field):
/// ```json
/// {"op":"in_session","session":1,"generation":1,"request":{"op":"list_contexts"}}
/// {"op":"set_breakpoint","spec":{"kind":{"kind":"program_counter"},"address":{"space":1,"offset":131072},"context":1,"enabled":true,"one_shot":false}}
/// {"op":"request_capture","request":{"mode":"pause_at_boundary","boundary":"logical_frame_complete","providers":[],"include_artifacts":true}}
/// {"op":"get_capabilities","target":{"kind":"context","id":1}}
/// ```
/// Substitute discovered IDs and the desired address for these examples.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum DebugRequest {
    /// Discover session/generation identity, active context, and terminal status.
    SessionInfo,
    /// Reject a request if either identity differs from the current runner.
    /// Obtain both from `SessionInfo`; nesting envelopes is unsupported.
    InSession {
        session: SessionId,
        generation: u64,
        request: Box<DebugRequest>,
    },
    /// Live observational snapshot at the command safe point.
    ReadProvider {
        id: ProviderId,
    },
    /// Read an owned snapshot retained by a capture, even after resume.
    ReadCaptureProvider {
        capture: CaptureId,
        provider: ProviderId,
    },
    /// Discover installed contexts and their current capabilities.
    ListContexts,
    /// Discover independently addressable mappings for the selected context.
    ListAddressSpaces {
        context: ContextSelector,
    },
    /// Discover provider IDs, schemas, collection limits, and boundary requirements.
    ListProviders,
    /// Query a context, address space, provider, boundary, or decoder target.
    /// Targets are tagged with `kind`, like `ContextSelector`:
    /// `{"kind":"context","id":1}`, `{"kind":"boundary","boundary":"logical_frame_complete"}`.
    GetCapabilities {
        target: CapabilityTarget,
    },
    /// Current execution state, active context, and guest counters.
    ExecutionStatus,
    /// Poll retained notifications with sequence strictly greater than `after`.
    /// Start at 0 and retain the last received sequence. History is bounded to
    /// 1024 entries; an expired cursor returns `StaleReference`.
    Notifications {
        after: u64,
    },
    /// Idempotently pause; cancel a pending step. Pending captures remain queued.
    Pause,
    /// Resume and release held host input. Reject terminal runners/pending steps.
    /// A hit PC breakpoint is suppressed until one unit in its context retires.
    Resume,
    /// Require a paused runner and an active context with step support.
    /// Return an operation ID; the scheduler executes one unit then pauses.
    /// A 68K unit may dispatch a trap, so this is not exact instruction tracing.
    /// The completed operation carries the resulting stop, so polling
    /// `GetOperation` is sufficient to learn the new location.
    Step {
        context: ContextSelector,
    },
    ReadRegisters {
        context: ContextSelector,
        /// Descriptor IDs, not register names. Null/omitted selects all; an empty
        /// array selects none. Unknown IDs are silently excluded.
        registers: Option<Vec<u32>>,
    },
    /// Observational bytes from a named space; inspect the reply truncation flag.
    /// 68K reads cover mapped RAM and are limited to 16 MiB per request.
    ReadMemory {
        space: AddressSpaceId,
        /// Byte offset within `space`.
        address: u64,
        /// Requested byte count.
        length: u64,
    },
    /// Decode from an explicit executable address space (68K output is approximate).
    Disassemble {
        context: ContextSelector,
        address: DebugAddress,
        /// Maximum instruction lines, at most 4096 for 68K.
        count: u32,
    },
    /// Raw stack bytes, not an unwound call stack.
    StackPreview {
        context: ContextSelector,
        /// Number of four-byte entries for 68K, at most 16384 (64 KiB).
        words: u32,
    },
    ListBreakpoints,
    SetBreakpoint {
        spec: BreakpointSpec,
    },
    RemoveBreakpoint {
        id: BreakpointId,
    },
    SetBreakpointEnabled {
        id: BreakpointId,
        enabled: bool,
    },
    ListCaptures,
    /// Return `Accepted` with an operation ID; poll `GetOperation` for the capture ID.
    RequestCapture {
        request: CaptureRequest,
    },
    /// Cancel a pending capture without changing the execution state.
    CancelCapture {
        operation: OperationId,
    },
    /// Retained manifest, including artifacts, omissions, and truncation flags.
    GetCapture {
        id: CaptureId,
    },
    /// Release the manifest, snapshots, and all artifacts owned by this capture.
    ReleaseCapture {
        id: CaptureId,
    },
    /// Poll completion; the last 1024 completed operations are retained.
    GetOperation {
        id: OperationId,
    },
    /// Return owned bytes and their descriptor (up to 32 MiB); no server file I/O.
    GetArtifact {
        id: ArtifactId,
    },
    /// Reserved; currently returns `Unsupported`.
    DecodeArtifact {
        id: ArtifactId,
    },
    /// Attribute `address` to a routine using recovered MacsBug symbols.
    /// Scans forward, because a symbol trails the routine it names. The reply
    /// distinguishes a real attribution from an address in an unsymbolised
    /// routine, which must not be reported under a later routine's name.
    /// Best effort: see the `symbols` provider's declared limitations.
    ResolveSymbol {
        context: ContextSelector,
        address: DebugAddress,
        /// Bytes to scan ahead. Null selects the default window.
        window: Option<u64>,
    },
    /// Every symbol exactly matching `name`, in ascending address order.
    /// Deliberately multi-valued: guest code is routinely resident more than
    /// once (a loaded CODE segment and the Resource Manager's cached copy of
    /// the same resource), so a name does not identify one address. Only the
    /// executing copy is a valid breakpoint target; an empty vector means the
    /// name was not found.
    LookupSymbol {
        context: ContextSelector,
        name: String,
    },
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct SessionInfo {
    pub session: SessionId,
    pub generation: u64,
    pub active_context: Option<ContextId>,
    pub terminal: bool,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ExecutionStatusReply {
    pub state: DebugExecutionState,
    pub terminal: bool,
    pub active_context: Option<ContextId>,
    pub guest_tick: Option<u32>,
    pub execution_units: Option<u64>,
    pub revision: u64,
}

/// Replies serialize as `{"reply":"snake_case_variant","value":...}`.
/// Socket transport wraps this inside its own `{"id":...,"reply":...}` envelope.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
// Adjacent tagging preserves sequence-valued newtype replies in JSON.
#[serde(tag = "reply", content = "value", rename_all = "snake_case")]
pub enum DebugReply {
    /// Carries the IDs to echo in `InSession`. `active_context` is null before
    /// first install; terminal runners still reply but accept no mutations.
    SessionInfo(SessionInfo),
    /// May be empty. Descriptor IDs are stable within a session generation.
    Contexts(Vec<ExecutionContextDescriptor>),
    /// Empty when the context exposes no independently addressable mappings.
    AddressSpaces(Vec<AddressSpaceDescriptor>),
    /// Each descriptor declares schema, collection limits, and boundary needs.
    Providers(Vec<InspectionProviderDescriptor>),
    /// Observational only; omissions and partial collections are inside the
    /// snapshot, not a transport error.
    ProviderSnapshot(super::provider::ProviderSnapshot),
    /// Per-target capability set; queries against an unknown context error.
    Capabilities(CapabilitySet),
    /// `guest_tick`/`execution_units` are null when the runner has not run yet.
    /// `revision` changes whenever state relevant to clients changes.
    ExecutionStatus(ExecutionStatusReply),
    /// Notifications with sequence strictly greater than the requested cursor,
    /// ascending. May be empty; expired cursors error with `StaleReference`.
    Notifications(Vec<super::model::DebugNotification>),
    /// Request accepted; a non-null operation ID must be polled for its outcome.
    Accepted {
        state: DebugExecutionState,
        operation_id: Option<OperationId>,
    },
    /// Snapshots carry their descriptor IDs. Requested but unknown IDs are
    /// silently omitted, so compare against the request rather than expecting
    /// an error.
    Registers {
        context: ContextId,
        registers: Vec<RegisterSnapshot>,
    },
    /// May return fewer bytes than requested; only the `truncated` flag
    /// distinguishes a short read from an exact fit.
    Memory(MemoryReadResult),
    /// May contain fewer lines than requested near unmapped or invalid code.
    Disassembly {
        context: ContextId,
        lines: Vec<DisassemblyLine>,
    },
    /// Raw stack bytes, not an unwound call stack; interpret per context.
    Stack(StackPreview),
    /// Includes disabled breakpoints; order is not specified.
    Breakpoints(Vec<BreakpointInfo>),
    /// Echoes the stored breakpoint, including its server-assigned ID, which
    /// may differ from identity fields in the spec when duplicates are merged.
    Breakpoint(BreakpointInfo),
    /// Echoes the removed ID.
    BreakpointRemoved {
        id: BreakpointId,
    },
    /// Manifests only; snapshots and artifacts are fetched per provider.
    Captures(Vec<CaptureManifest>),
    /// Full manifest including artifacts, omissions, and truncation flags.
    /// Pending captures only reply here once the operation completes.
    Capture(CaptureManifest),
    /// Release is final; further access to the capture errors with
    /// `CaptureExpired`.
    CaptureReleased {
        id: CaptureId,
    },
    /// Owned bytes up to 32 MiB; larger artifacts error with `TooLarge`.
    ArtifactData {
        descriptor: ArtifactDescriptor,
        bytes: Vec<u8>,
    },
    /// Poll until terminal. `Expired` means it left the 1024-entry retention
    /// window; `Failed`/`Unavailable` carry reasons in the state, not as
    /// transport errors. `Completed` carries the resulting stop, if any.
    Operation(OperationStatus),
    /// Descriptor only; bytes come from `GetArtifactData`.
    Artifact(ArtifactDescriptor),
    /// Reserved; requests currently error with `Unsupported` before this reply
    /// is ever produced.
    Decoded(DecodedArtifact),
    /// Carries whether the address was actually attributed to a routine; an
    /// unattributed address is a normal outcome for code built or stripped
    /// without symbols.
    Symbol(super::symbols::SymbolResolution),
    /// Every match for a looked-up name, ascending by address. May be empty,
    /// and may legitimately hold several entries; see `LookupSymbol`.
    Symbols(Vec<super::symbols::SymbolEntry>),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sequence_replies_serialize_through_transports() {
        for (reply, tag) in [
            (DebugReply::Contexts(Vec::new()), "contexts"),
            (DebugReply::Providers(Vec::new()), "providers"),
            (DebugReply::Captures(Vec::new()), "captures"),
            (DebugReply::Breakpoints(Vec::new()), "breakpoints"),
        ] {
            let json = serde_json::to_value(&reply).expect("sequence reply serializes");
            assert_eq!(
                json.get("reply").and_then(|value| value.as_str()),
                Some(tag)
            );
            assert!(json.get("value").is_some_and(|value| value.is_array()));
        }
    }
}
