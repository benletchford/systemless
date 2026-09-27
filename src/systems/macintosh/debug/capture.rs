use super::adapters::active_context_and_location;
use super::error::{DebugError, DebugResult};
use super::ids::{ArtifactId, CaptureId, OperationId, ProviderId};
use super::model::{
    ArtifactDescriptor, CaptureBoundaryKind, CaptureManifest, DebugExecutionState, OperationOutcome,
};
use super::provider::ProviderSnapshot;
use super::request::{CaptureMode, CaptureRequest, DebugReply};
use crate::runner::FixtureRunner;
use std::collections::{BTreeMap, BTreeSet, VecDeque};

pub const DEFAULT_CAPTURE_RETENTION: usize = 16;
pub const MAX_PENDING_CAPTURES: usize = 64;
pub const MAX_CAPTURE_PROVIDERS: usize = 64;
pub const MAX_CAPTURE_ARTIFACTS: usize = 64;
pub const MAX_CAPTURE_ARTIFACT_BYTES: u64 = 64 * 1024 * 1024;

#[derive(Clone, Debug)]
pub struct RetainedCapture {
    pub manifest: super::model::CaptureManifest,
    pub snapshots: Vec<(ProviderId, super::provider::ProviderSnapshot)>,
    pub artifacts: Vec<(ArtifactId, Vec<u8>)>,
}

#[derive(Clone, Debug)]
pub struct CaptureStore {
    captures: BTreeMap<CaptureId, RetainedCapture>,
    capture_order: VecDeque<CaptureId>,
    retention: usize,
    next_capture_id: u64,
    next_artifact_id: u64,
}

impl CaptureStore {
    pub(crate) fn new() -> Self {
        Self {
            captures: BTreeMap::new(),
            capture_order: VecDeque::new(),
            retention: DEFAULT_CAPTURE_RETENTION,
            next_capture_id: 1,
            next_artifact_id: 1,
        }
    }

    pub fn alloc_artifact(&mut self) -> u64 {
        let id = self.next_artifact_id;
        self.next_artifact_id = self.next_artifact_id.saturating_add(1);
        id
    }

    pub fn len(&self) -> usize {
        self.captures.len()
    }

    pub fn is_empty(&self) -> bool {
        self.captures.is_empty()
    }

    pub(crate) fn alloc_capture(&mut self) -> CaptureId {
        let id = CaptureId(self.next_capture_id);
        self.next_capture_id = self.next_capture_id.saturating_add(1);
        id
    }

    pub(crate) fn insert(&mut self, capture: RetainedCapture) -> Option<CaptureId> {
        let id = capture.manifest.id;
        if let Some(next_artifact_id) = capture
            .manifest
            .artifacts
            .iter()
            .map(|artifact| artifact.id.0.saturating_add(1))
            .max()
        {
            self.next_artifact_id = self.next_artifact_id.max(next_artifact_id);
        }
        self.captures.insert(id, capture);
        self.capture_order.push_back(id);
        self.evict_to_retention()
    }

    fn evict_to_retention(&mut self) -> Option<CaptureId> {
        let mut evicted = None;
        while self.capture_order.len() > self.retention {
            if let Some(oldest) = self.capture_order.pop_front() {
                self.captures.remove(&oldest);
                evicted = Some(oldest);
            }
        }
        evicted
    }

    pub(crate) fn get(&self, id: CaptureId) -> Option<&RetainedCapture> {
        self.captures.get(&id)
    }

    pub(crate) fn snapshot(
        &self,
        capture: CaptureId,
        provider: ProviderId,
    ) -> Option<ProviderSnapshot> {
        self.get(capture)?
            .snapshots
            .iter()
            .find(|(id, _)| *id == provider)
            .map(|(_, snapshot)| snapshot.clone())
    }

    pub(crate) fn artifact(&self, id: ArtifactId) -> Option<(ArtifactDescriptor, Vec<u8>)> {
        for capture in self.captures.values() {
            if let Some(descriptor) = capture
                .manifest
                .artifacts
                .iter()
                .find(|descriptor| descriptor.id == id)
            {
                let bytes = capture
                    .artifacts
                    .iter()
                    .find(|(artifact, _)| *artifact == id)
                    .map(|(_, bytes)| bytes.clone())
                    .unwrap_or_default();
                return Some((descriptor.clone(), bytes));
            }
        }
        None
    }

    pub(crate) fn release(&mut self, id: CaptureId) -> bool {
        let removed = self.captures.remove(&id).is_some();
        if removed {
            self.capture_order.retain(|candidate| *candidate != id);
        }
        removed
    }

    pub(crate) fn manifests(&self) -> Vec<CaptureManifest> {
        self.capture_order
            .iter()
            .filter_map(|id| self.captures.get(id))
            .map(|capture| capture.manifest.clone())
            .collect()
    }

    pub(crate) fn set_retention(&mut self, retention: usize) {
        self.retention = retention.max(1);
        self.evict_to_retention();
    }

    pub(crate) fn drain(&mut self) -> CaptureStore {
        CaptureStore {
            captures: std::mem::take(&mut self.captures),
            capture_order: std::mem::take(&mut self.capture_order),
            retention: self.retention,
            next_capture_id: self.next_capture_id,
            next_artifact_id: self.next_artifact_id,
        }
    }

    pub(crate) fn adopt(&mut self, store: CaptureStore) {
        let store_next_artifact_id = store
            .captures
            .values()
            .flat_map(|capture| capture.manifest.artifacts.iter())
            .map(|artifact| artifact.id.0.saturating_add(1))
            .max()
            .unwrap_or(1);
        self.captures = store.captures;
        self.capture_order = store.capture_order;
        self.retention = store.retention.max(1);
        self.next_capture_id = self.next_capture_id.max(store.next_capture_id);
        self.next_artifact_id = self
            .next_artifact_id
            .max(store.next_artifact_id)
            .max(store_next_artifact_id);
        self.evict_to_retention();
    }
}

impl Default for CaptureStore {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Clone, Debug)]
pub struct PendingCapture {
    pub operation: OperationId,
    pub capture: CaptureId,
    pub request: CaptureRequest,
    pub boundary: CaptureBoundaryKind,
    pub pause: bool,
}

pub(crate) fn request_capture(
    runner: &mut FixtureRunner,
    mut request: CaptureRequest,
) -> DebugResult<DebugReply> {
    request.providers = effective_providers(&request)?;
    match request.mode {
        CaptureMode::CurrentState => {
            let pending = runner.debug.begin_immediate_capture(request);
            let capture = build_capture(runner, &pending, CaptureBoundaryKind::CommandSafePoint);
            runner.debug.insert_capture(capture);
            runner.debug.complete_operation(
                pending.operation,
                OperationOutcome::Captured {
                    capture_id: pending.capture,
                },
            );
            Ok(DebugReply::Accepted {
                state: runner.debug.execution_state().clone(),
                operation_id: Some(pending.operation),
            })
        }
        CaptureMode::NextBoundary | CaptureMode::PauseAtBoundary => {
            if runner.debug.pending_capture_count() >= MAX_PENDING_CAPTURES {
                return Err(DebugError::TooLarge {
                    limit: MAX_PENDING_CAPTURES as u64,
                    requested: runner.debug.pending_capture_count() as u64 + 1,
                });
            }
            let boundary = request
                .boundary
                .unwrap_or(CaptureBoundaryKind::LogicalFrameComplete);
            if !capture_boundary_supported(boundary) {
                return Err(DebugError::BoundaryUnavailable {
                    detail: format!("capture boundary {boundary:?} is not observed by this runner"),
                });
            }
            if runner.is_halted() {
                return Err(DebugError::BoundaryUnavailable {
                    detail: "a terminal runner cannot reach a future capture boundary".into(),
                });
            }
            if runner.debug.execution_state().is_stepping() {
                return Err(DebugError::invalid_state(
                    "cannot wait for a capture boundary while a step is pending",
                ));
            }
            if boundary == CaptureBoundaryKind::CommandSafePoint {
                let pending = runner.debug.begin_immediate_capture(request);
                if pending.pause {
                    let (context, location) = active_context_and_location(runner);
                    let guest_tick = runner.guest_tick();
                    let execution_units = runner.total_instructions();
                    runner.debug.stop_at_capture_boundary(
                        pending.capture,
                        context,
                        location,
                        Some(guest_tick),
                        Some(execution_units),
                    );
                }
                let capture = build_capture(runner, &pending, boundary);
                runner.debug.insert_capture(capture);
                runner.debug.complete_operation(
                    pending.operation,
                    OperationOutcome::Captured {
                        capture_id: pending.capture,
                    },
                );
                return Ok(DebugReply::Accepted {
                    state: runner.debug.execution_state().clone(),
                    operation_id: Some(pending.operation),
                });
            }
            if runner.debug.resume() {
                runner.debug_release_held_input();
            }
            let pending = runner.debug.begin_capture(request, boundary);
            Ok(DebugReply::Accepted {
                state: runner.debug.execution_state().clone(),
                operation_id: Some(pending.operation),
            })
        }
    }
}

pub(crate) fn capture_boundary_supported(boundary: CaptureBoundaryKind) -> bool {
    matches!(
        boundary,
        CaptureBoundaryKind::CommandSafePoint | CaptureBoundaryKind::LogicalFrameComplete
    )
}

pub(crate) fn note_capture_boundary(runner: &mut FixtureRunner, boundary: CaptureBoundaryKind) {
    let pending = runner.debug.take_pending_captures(boundary);
    for capture in pending {
        if capture.pause {
            let (context, location) = active_context_and_location(runner);
            let guest_tick = runner.guest_tick();
            let execution_units = runner.total_instructions();
            runner.debug.stop_at_capture_boundary(
                capture.capture,
                context,
                location,
                Some(guest_tick),
                Some(execution_units),
            );
        }
        let record = build_capture(runner, &capture, boundary);
        runner.debug.insert_capture(record);
        runner.debug.complete_operation(
            capture.operation,
            OperationOutcome::Captured {
                capture_id: capture.capture,
            },
        );
    }
}

fn effective_providers(request: &CaptureRequest) -> DebugResult<Vec<ProviderId>> {
    let providers = if request.providers.is_empty() {
        vec![ProviderId(super::provider::provider_ids::GRAPHICS)]
    } else {
        request.providers.clone()
    };
    if providers.len() > MAX_CAPTURE_PROVIDERS {
        return Err(DebugError::TooLarge {
            limit: MAX_CAPTURE_PROVIDERS as u64,
            requested: providers.len() as u64,
        });
    }
    let mut seen = BTreeSet::new();
    Ok(providers
        .into_iter()
        .filter(|provider| seen.insert(*provider))
        .collect())
}

pub(crate) fn boundary_synchronization_notes(boundary: CaptureBoundaryKind) -> Vec<String> {
    match boundary {
        CaptureBoundaryKind::CommandSafePoint => vec![
            "selected provider state is copied at one command-service safe point".to_string(),
            "host presentation and GPU completion are not implied".to_string(),
        ],
        CaptureBoundaryKind::LogicalFrameComplete => vec![
            "runner frame finalization (chrome redraw and guest audio) has completed".to_string(),
            "this is not proof that a guest produced a drawing or that a physical display updated"
                .to_string(),
        ],
        _ => vec!["boundary has no synchronization guarantee".to_string()],
    }
}

fn build_capture(
    runner: &mut FixtureRunner,
    pending: &PendingCapture,
    boundary: CaptureBoundaryKind,
) -> RetainedCapture {
    let request = &pending.request;
    let mut manifest = CaptureManifest {
        id: pending.capture,
        session: runner.debug.session(),
        boundary,
        providers: Vec::new(),
        revision: runner.debug.revision(),
        stop: match runner.debug.execution_state() {
            DebugExecutionState::Paused { stop } => Some(stop.id),
            _ => None,
        },
        guest_tick: Some(runner.guest_tick()),
        execution_units: Some(runner.total_instructions()),
        artifacts: Vec::new(),
        omitted: Vec::new(),
        truncated: false,
        atomic: true,
        synchronization: boundary_synchronization_notes(boundary),
    };

    let mut snapshots: Vec<(ProviderId, ProviderSnapshot)> = Vec::new();
    let mut artifacts: Vec<(ArtifactId, Vec<u8>)> = Vec::new();
    let mut artifact_bytes = 0u64;
    for &provider_id in &request.providers {
        let Some(provider) = runner.debug.provider(provider_id) else {
            manifest.atomic = false;
            manifest
                .omitted
                .push(format!("provider {provider_id}: unsupported provider"));
            continue;
        };
        if let Err(error) = provider.validate_boundary(boundary) {
            manifest.atomic = false;
            manifest
                .omitted
                .push(format!("provider {provider_id}: {error}"));
            continue;
        }
        match provider.snapshot_at(runner, boundary) {
            Ok(snapshot) => {
                manifest.truncated |= snapshot.is_truncated();
                manifest.providers.push(provider_id);
                snapshots.push((provider_id, snapshot));
            }
            Err(error) => {
                manifest.atomic = false;
                manifest
                    .omitted
                    .push(format!("provider {provider_id}: {error}"));
            }
        }
        if request.include_artifacts {
            match provider.artifacts_at(runner, boundary) {
                Ok(provider_artifacts) => {
                    for artifact in provider_artifacts {
                        if artifacts.len() >= MAX_CAPTURE_ARTIFACTS {
                            manifest.atomic = false;
                            manifest.omitted.push(format!(
                                "provider {provider_id} artifacts exceed the capture limit of {MAX_CAPTURE_ARTIFACTS}"
                            ));
                            break;
                        }
                        let limit = provider.capabilities().max_artifact_bytes;
                        let byte_len = artifact.bytes.len() as u64;
                        if byte_len > limit {
                            manifest.atomic = false;
                            manifest.omitted.push(format!(
                                "provider {provider_id} artifact is {byte_len} bytes, above its {limit}-byte limit"
                            ));
                            continue;
                        }
                        let Some(next_artifact_bytes) = artifact_bytes.checked_add(byte_len) else {
                            manifest.atomic = false;
                            manifest.omitted.push(format!(
                                "provider {provider_id} artifacts exceed the capture byte limit"
                            ));
                            break;
                        };
                        if next_artifact_bytes > MAX_CAPTURE_ARTIFACT_BYTES {
                            manifest.atomic = false;
                            manifest.omitted.push(format!(
                                "provider {provider_id} artifacts exceed the {MAX_CAPTURE_ARTIFACT_BYTES}-byte capture limit"
                            ));
                            break;
                        }
                        let id = ArtifactId(runner.debug.alloc_artifact());
                        manifest.artifacts.push(ArtifactDescriptor {
                            id,
                            provider: Some(provider_id),
                            format: artifact.format,
                            byte_len,
                            provenance: artifact.provenance,
                            related: artifact.related,
                        });
                        artifacts.push((id, artifact.bytes));
                        artifact_bytes = next_artifact_bytes;
                    }
                }
                Err(error) => {
                    manifest.atomic = false;
                    manifest
                        .omitted
                        .push(format!("provider {provider_id} artifacts: {error}"));
                }
            }
        }
    }

    RetainedCapture {
        manifest,
        snapshots,
        artifacts,
    }
}
