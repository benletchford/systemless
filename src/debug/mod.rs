//! Opt-in, runner-local inspection and execution control (`debug` feature).
//!
//! Call [`handle_debug_request`] between runner calls on the owning thread.
//! Requests copy state or record intent; they never execute guest code or wait
//! for a future boundary. Advance execution through scheduler APIs such as
//! [`FixtureRunner::run_steps`](crate::runner::FixtureRunner::run_steps), not
//! the low-level `step()` or `run()` paths. Pause cannot interrupt synchronous
//! traps or host callbacks already in progress.
//!
//! The 68K adapter supports registers, bounded RAM reads, approximate disassembly,
//! raw stack previews, stepping, and PC breakpoints. PowerPC currently supports
//! register inspection. Query [`DebugRequest::GetCapabilities`] before relying
//! on an operation. [`ArchitectureAdapter`] and [`InspectionProvider`] describe
//! the extension contracts; providers currently cover graphics, windows, and events.
//!
//! # Socket usage
//!
//! On Unix, build with `cargo build --features debug-server` (which implies
//! `debug`), then run:
//!
//! ```text
//! systemless game.sit --headless --max-ticks 600 --debug-socket /tmp/systemless.sock
//! ```
//!
//! Both headless clocks support the socket and start paused before scripted
//! inputs; GUI runs start normally. Pauses consume no frontend ticks. Reaching
//! the configured headless budget ends the process; a terminal guest halt keeps
//! inspection available until Ctrl-C (or window close in the GUI).
//!
//! One controlling client sends newline-delimited UTF-8 JSON. Each input line
//! is limited to 1 MiB. Envelope `id` is an echoed correlation number (default
//! `0`), independent of debugger operation IDs. Requests are serviced between
//! frames; notifications must be polled, not received as unsolicited messages.
//! For example, send these lines over a Unix socket client such as `nc -U`:
//!
//! ```json
//! {"id":1,"request":{"op":"session_info"}}
//! {"id":2,"request":{"op":"read_registers","context":{"kind":"active"},"registers":null}}
//! {"id":3,"request":{"op":"step","context":{"kind":"active"}}}
//! ```
//!
//! Success envelopes contain `{"id":3,"reply":{"reply":"accepted","value":...}}`;
//! failures contain `{"id":3,"error":{"error":"invalid_state","detail":...}}`.
//! Use the returned `operation_id` in `{"id":4,"request":{"op":"get_operation","id":1}}`
//! (replace `1` with the actual ID). An accepted operation may still be pending
//! or later fail. [`DebugRequest`] documents fields, units, and control semantics.
//!
//! Only one client is served at a time. A second connection is refused with an
//! `invalid_state` envelope and closed, rather than left waiting.
//!
//! # Reply shapes
//!
//! [`DebugReply`] serializes as `{"reply":"<variant>","value":<payload>}`. The
//! payloads a client must destructure to make progress:
//!
//! ```json
//! {"reply":"accepted","value":{"state":{"state":"stepping","operation_id":2},"operation_id":2}}
//! {"reply":"operation","value":{"id":2,"state":{"state":"completed",
//!   "result":{"outcome":"completed","stop":{"id":3,"revision":6,"context":1,
//!     "location":{"context":1,"address":{"space":1,"offset":2163734}},
//!     "reason":{"reason":"step_completed"},"guest_tick":1704,"execution_units":239650435}}}}}
//! {"reply":"operation","value":{"id":15,"state":{"state":"completed",
//!   "result":{"outcome":"captured","capture_id":2}}}}
//! {"reply":"memory","value":{"address":{"space":1,"offset":66584576},"bytes":[255,255],"truncated":false}}
//! {"reply":"stack","value":{"context":1,"address":{"space":1,"offset":66518950},
//!   "bytes":[3,246,255,210],"stride":4,"truncated":false}}
//! ```
//!
//! Three details that clients get wrong:
//!
//! - A capture ID arrives as `result.capture_id` on a `captured` outcome, not as
//!   the operation ID. The operation ID cannot be used to fetch a capture.
//! - A `completed` step outcome carries `result.stop`, which is the same
//!   [`StopInfo`] that [`DebugRequest::ExecutionStatus`] would report. Read the
//!   new location from there instead of issuing a follow-up status request.
//! - `bytes` fields are JSON arrays of integers, not base64 strings, in every
//!   reply that carries them.
//!
//! IDs and addresses are JSON numbers; byte buffers are arrays of integers,
//! not base64. Register values wider than 32 bits use big-endian byte arrays to
//! preserve exact bits. Context selectors use `{"kind":"active"}` or
//! `{"kind":"id","id":1}`; addresses use `{"space":1,"offset":131072}`.
//! Discover IDs through session/context/address-space/provider requests. Wrap
//! live requests in [`DebugRequest::InSession`] to reject stale generations.
//!
//! # Captures and embedding
//!
//! [`CaptureRequest`] returns an operation ID even for an immediate capture.
//! Poll until [`OperationOutcome::Captured`] supplies a capture ID, then fetch
//! its manifest, provider snapshots, and artifact bytes. Captures own their data
//! and survive resume; inspect manifest omissions/truncation before using them.
//! The default store retains 16 captures; release them explicitly when finished.
//! Pending captures fail on terminal halt or generation change. Retained captures
//! can move to a replacement runner using `debug_extract_capture_store` and
//! `debug_adopt_capture_store`; their manifests retain the original session.
//!
//! GUI and simulated-time frontends call
//! [`FixtureRunner::finish_gui_frame`](crate::runner::FixtureRunner::finish_gui_frame)
//! once after each outer frame's audio and compositing. Sound-only slices do not
//! complete captures. Logical-frame completion guarantees runner finalization,
//! not physical-display or GPU completion.

pub(crate) mod adapters;
mod capabilities;
mod capture;
mod coordinator;
mod error;
mod handler;
mod ids;
mod model;
mod provider;
mod request;
mod symbols;

pub use adapters::{
    ActiveArchitecture, AdapterFactory, AdapterRegistration, ArchitectureAdapter, M68K_CONTEXT,
    M68K_SPACE, PPC_COMPANION_CONTEXT, PPC_COMPANION_SPACE, PPC_CONTEXT, PPC_SPACE,
};
pub use capabilities::{
    CapabilitySet, CapabilityTarget, DecoderCapabilities, ProviderCapabilities,
};
pub(crate) use capture::note_capture_boundary;
pub use capture::{CaptureStore, PendingCapture, RetainedCapture};
pub use coordinator::DebuggerCoordinator;
pub use error::{DebugError, DebugResult};
pub use handler::{builtin_provider_ids, handle_debug_request};
pub use ids::{
    AddressSpaceId, ArtifactId, BreakpointId, CaptureId, ContextId, DecodeId, OperationId,
    ProviderId, SessionId, WatchpointId,
};
pub use model::{
    AddressAccess, AddressMappingKind, AddressSpaceCapabilities, AddressSpaceDescriptor,
    ArtifactDescriptor, ByteOrder, CaptureBoundaryCapabilities, CaptureBoundaryKind,
    CaptureManifest, ContextCapabilities, ContextLifecycle, ContextSelector, DebugAddress,
    DebugExecutionState, DebugNotification, DecodedArtifact, DisassemblyLine,
    DisplayOutputDescriptor, ExecutionContextDescriptor, ExecutionLocation, GraphicsSnapshot,
    GraphicsSurfaceDescriptor, InspectionProviderDescriptor, MemoryReadResult, NotificationPayload,
    OperationOutcome, OperationState, OperationStatus, PixelFormat, ProviderObjectDescriptor,
    ProviderSnapshotKind, RegisterCategory, RegisterDescriptor, RegisterFlag, RegisterRole,
    RegisterSnapshot, RegisterValue, RegisterWidth, StackPreview, StopInfo, StopReason,
    SurfaceKind,
};
pub use provider::{
    builtin_providers, DeclaredProvider, InspectionProvider, ProviderArtifact,
    ProviderRegistration, ProviderSnapshot,
};
pub use symbols::{SymbolEntry, SymbolResolution, DEFAULT_RESOLVE_WINDOW};
pub use request::{
    BreakpointInfo, BreakpointKind, BreakpointSpec, CaptureMode, CaptureRequest, DebugReply,
    DebugRequest, ExecutionStatusReply, SessionInfo,
};
