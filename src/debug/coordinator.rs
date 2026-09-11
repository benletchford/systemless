use super::capture::{CaptureStore, PendingCapture, RetainedCapture};
use super::error::{DebugError, DebugResult};
use super::ids::{
    AddressSpaceId, ArtifactId, BreakpointId, CaptureId, ContextId, OperationId, ProviderId,
    SessionId, TypedAllocator,
};
use super::model::{
    CaptureBoundaryKind, DebugExecutionState, DebugNotification, ExecutionLocation,
    NotificationPayload, OperationOutcome, OperationState, OperationStatus, StopInfo, StopReason,
};
use super::request::{BreakpointInfo, BreakpointSpec, CaptureMode, CaptureRequest};
use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::sync::atomic::{AtomicU64, Ordering};
static NEXT_SESSION: AtomicU64 = AtomicU64::new(1);

pub const NOTIFICATION_RETENTION: usize = 1024;
pub const OPERATION_RETENTION: usize = 1024;

#[derive(Clone, Debug)]
struct PendingStep {
    operation: OperationId,
    context: ContextId,
    units_remaining: u32,
}

#[derive(Debug)]
pub struct DebuggerCoordinator {
    session: SessionId,
    generation: u64,
    revision: u64,
    execution_state: DebugExecutionState,
    next_stop_id: u64,
    next_breakpoint_id: TypedAllocator<BreakpointId>,
    next_operation_id: TypedAllocator<OperationId>,
    breakpoints: BTreeMap<BreakpointId, BreakpointInfo>,
    // Stops happen before execution, so resume ignores that breakpoint until
    // one unit in its context retires.
    suppressed_breakpoints: BTreeSet<BreakpointId>,
    pending_step: Option<PendingStep>,
    operations: BTreeMap<OperationId, OperationState>,
    operation_order: VecDeque<OperationId>,
    expired_operation_through: u64,
    providers: Vec<super::provider::ProviderRegistration>,
    adapter_registrations: Vec<super::adapters::AdapterRegistration>,
    capture_store: CaptureStore,
    pending_captures: Vec<PendingCapture>,
    notifications: VecDeque<DebugNotification>,
    next_notification_sequence: u64,
    terminal_announced: bool,
    observed_context: Option<ContextId>,
}

impl DebuggerCoordinator {
    pub fn fresh() -> Self {
        let session = SessionId(NEXT_SESSION.fetch_add(1, Ordering::Relaxed));
        let mut coordinator = Self::new(session);
        coordinator.generation = session.0;
        coordinator
    }

    pub fn new(session: SessionId) -> Self {
        Self {
            session,
            generation: 1,
            revision: 0,
            execution_state: DebugExecutionState::Running,
            next_stop_id: 1,
            next_breakpoint_id: TypedAllocator::new(),
            next_operation_id: TypedAllocator::new(),
            breakpoints: BTreeMap::new(),
            suppressed_breakpoints: BTreeSet::new(),
            pending_step: None,
            operations: BTreeMap::new(),
            operation_order: VecDeque::new(),
            expired_operation_through: 0,
            providers: super::provider::builtin_providers().to_vec(),
            adapter_registrations: super::adapters::builtin_adapter_registrations(),
            capture_store: CaptureStore::new(),
            pending_captures: Vec::new(),
            notifications: VecDeque::new(),
            next_notification_sequence: 1,
            terminal_announced: false,
            observed_context: None,
        }
    }

    pub fn session(&self) -> SessionId {
        self.session
    }

    pub fn generation(&self) -> u64 {
        self.generation
    }

    pub fn revision(&self) -> u64 {
        self.revision
    }

    pub fn execution_state(&self) -> &DebugExecutionState {
        &self.execution_state
    }

    pub(crate) fn advance_generation(&mut self) {
        self.generation = self.generation.saturating_add(1);
        self.revision = self.revision.saturating_add(1);
        self.execution_state = DebugExecutionState::Running;
        self.breakpoints.clear();
        self.suppressed_breakpoints.clear();
        self.cancel_pending_step(OperationOutcome::Unavailable {
            reason: "runner generation changed".into(),
        });
        self.fail_pending_captures("runner generation changed");
        self.operations.clear();
        self.operation_order.clear();
        self.expired_operation_through = 0;
        self.terminal_announced = false;
        self.observed_context = None;
        self.push_notification(NotificationPayload::Invalidated {
            session: self.session,
        });
    }

    pub(crate) fn touch(&mut self) -> u64 {
        self.revision = self.revision.saturating_add(1);
        self.revision
    }

    fn stop(
        &mut self,
        reason: StopReason,
        context: Option<ContextId>,
        location: Option<ExecutionLocation>,
        guest_tick: Option<u32>,
        execution_units: Option<u64>,
    ) -> StopInfo {
        self.revision = self.revision.saturating_add(1);
        let stop = StopInfo {
            id: self.next_stop_id,
            revision: self.revision,
            context,
            location,
            reason,
            guest_tick,
            execution_units,
        };
        self.next_stop_id = self.next_stop_id.saturating_add(1);
        self.execution_state = DebugExecutionState::Paused { stop: stop.clone() };
        self.push_notification(NotificationPayload::ExecutionStateChanged {
            state: self.execution_state.clone(),
        });
        stop
    }

    pub fn pause(
        &mut self,
        context: Option<ContextId>,
        location: Option<ExecutionLocation>,
    ) -> StopInfo {
        self.pause_at(context, location, None, None)
    }

    pub fn pause_at(
        &mut self,
        context: Option<ContextId>,
        location: Option<ExecutionLocation>,
        guest_tick: Option<u32>,
        execution_units: Option<u64>,
    ) -> StopInfo {
        self.cancel_pending_step(OperationOutcome::Cancelled);
        if let DebugExecutionState::Paused { stop } = &self.execution_state {
            return stop.clone();
        }
        self.stop(
            StopReason::UserPause,
            context,
            location,
            guest_tick,
            execution_units,
        )
    }

    pub fn resume(&mut self) -> bool {
        if !self.execution_state.is_paused() {
            return false;
        }
        if let DebugExecutionState::Paused { stop } = &self.execution_state {
            if let StopReason::PcBreakpoint { id } = stop.reason {
                if self.breakpoints.contains_key(&id) {
                    self.suppressed_breakpoints.insert(id);
                }
            }
        }
        self.cancel_pending_step(OperationOutcome::Cancelled);
        self.execution_state = DebugExecutionState::Running;
        self.push_notification(NotificationPayload::ExecutionStateChanged {
            state: self.execution_state.clone(),
        });
        true
    }

    pub fn is_paused(&self) -> bool {
        self.execution_state.is_paused()
    }

    pub fn is_stepping(&self) -> bool {
        self.pending_step.is_some()
    }

    pub(crate) fn begin_step(&mut self, context: ContextId) -> OperationId {
        self.cancel_pending_step(OperationOutcome::Cancelled);
        if let DebugExecutionState::Paused { stop } = &self.execution_state {
            if let StopReason::PcBreakpoint { id } = stop.reason {
                if self.breakpoints.contains_key(&id) {
                    self.suppressed_breakpoints.insert(id);
                }
            }
        }
        let operation = self.next_operation_id.alloc();
        self.operations.insert(operation, OperationState::Pending);
        self.operation_order.push_back(operation);
        self.pending_step = Some(PendingStep {
            operation,
            context,
            units_remaining: 1,
        });
        self.execution_state = DebugExecutionState::Stepping {
            operation_id: operation,
        };
        self.touch();
        self.push_notification(NotificationPayload::ExecutionStateChanged {
            state: self.execution_state.clone(),
        });
        operation
    }

    pub fn step_target_context(&self) -> Option<ContextId> {
        self.pending_step.as_ref().map(|step| step.context)
    }

    pub fn step_units_remaining(&self) -> Option<u32> {
        self.pending_step.as_ref().map(|step| step.units_remaining)
    }

    pub(crate) fn breakpoint_rearming(&self) -> bool {
        !self.suppressed_breakpoints.is_empty()
    }

    pub(crate) fn note_executed_units(&mut self, context: ContextId, units: u32) {
        if units == 0 {
            return;
        }
        self.suppressed_breakpoints.retain(|id| {
            self.breakpoints
                .get(id)
                .is_some_and(|breakpoint| breakpoint.spec.context != Some(context))
        });
        if let Some(step) = self.pending_step.as_mut() {
            if step.context == context {
                step.units_remaining = step.units_remaining.saturating_sub(units);
            }
        }
    }

    pub(crate) fn finish_step(
        &mut self,
        context: ContextId,
        location: Option<ExecutionLocation>,
        guest_tick: Option<u32>,
        execution_units: Option<u64>,
    ) -> Option<StopInfo> {
        let operation = self.pending_step.take()?.operation;
        let stop = self.stop(
            StopReason::StepCompleted,
            Some(context),
            location,
            guest_tick,
            execution_units,
        );
        self.complete_operation(
            operation,
            OperationOutcome::Completed {
                stop: Some(stop.clone()),
            },
        );
        Some(stop)
    }

    pub fn abort_step(&mut self, reason: impl Into<String>) {
        if let Some(step) = self.pending_step.take() {
            self.suppressed_breakpoints.clear();
            self.complete_operation(
                step.operation,
                OperationOutcome::Unavailable {
                    reason: reason.into(),
                },
            );
            self.execution_state = DebugExecutionState::Running;
            self.push_notification(NotificationPayload::ExecutionStateChanged {
                state: self.execution_state.clone(),
            });
        }
    }

    fn cancel_pending_step(&mut self, outcome: OperationOutcome) -> bool {
        let Some(step) = self.pending_step.take() else {
            return false;
        };
        self.suppressed_breakpoints.clear();
        self.complete_operation(step.operation, outcome);
        true
    }

    pub fn insert_breakpoint(&mut self, spec: BreakpointSpec) -> BreakpointInfo {
        let id = self.next_breakpoint_id.alloc();
        let info = BreakpointInfo {
            id,
            spec,
            hit_count: 0,
        };
        self.breakpoints.insert(id, info.clone());
        self.touch();
        info
    }

    pub fn remove_breakpoint(&mut self, id: BreakpointId) -> bool {
        let removed = self.breakpoints.remove(&id).is_some();
        self.suppressed_breakpoints.remove(&id);
        if removed {
            self.touch();
        }
        removed
    }

    pub fn set_breakpoint_enabled(
        &mut self,
        id: BreakpointId,
        enabled: bool,
    ) -> Option<BreakpointInfo> {
        let info = self.breakpoints.get_mut(&id)?;
        info.spec.enabled = enabled;
        let cloned = info.clone();
        if !enabled {
            self.suppressed_breakpoints.remove(&id);
        }
        self.touch();
        Some(cloned)
    }

    pub fn breakpoints(&self) -> Vec<BreakpointInfo> {
        self.breakpoints.values().cloned().collect()
    }

    pub(crate) fn breakpoint_watch_addresses(
        &self,
        context: ContextId,
        space: super::ids::AddressSpaceId,
    ) -> Vec<u64> {
        let mut addresses = Vec::new();
        for (id, info) in &self.breakpoints {
            if !info.spec.enabled
                || info.spec.context != Some(context)
                || info.spec.address.space != space
                || self.suppressed_breakpoints.contains(id)
            {
                continue;
            }
            addresses.push(info.spec.address.offset);
        }
        addresses
    }

    pub(crate) fn breakpoint_at(
        &self,
        context: ContextId,
        space: super::ids::AddressSpaceId,
        address: u64,
    ) -> Option<BreakpointId> {
        self.breakpoints.iter().find_map(|(id, info)| {
            (info.spec.enabled
                && !self.suppressed_breakpoints.contains(id)
                && info.spec.context == Some(context)
                && info.spec.address.space == space
                && info.spec.address.offset == address)
                .then_some(*id)
        })
    }

    fn note_breakpoint_hit(&mut self, id: BreakpointId) -> bool {
        let Some(info) = self.breakpoints.get_mut(&id) else {
            return false;
        };
        info.hit_count = info.hit_count.saturating_add(1);
        info.spec.one_shot
    }

    pub(crate) fn breakpoint_stop(
        &mut self,
        id: BreakpointId,
        context: ContextId,
        location: Option<ExecutionLocation>,
        guest_tick: Option<u32>,
        execution_units: Option<u64>,
    ) -> StopInfo {
        let operation = self.pending_step.take().map(|step| step.operation);
        let remove = self.note_breakpoint_hit(id);
        if remove {
            self.breakpoints.remove(&id);
            self.suppressed_breakpoints.remove(&id);
        }
        let stop = self.stop(
            StopReason::PcBreakpoint { id },
            Some(context),
            location,
            guest_tick,
            execution_units,
        );
        if let Some(operation) = operation {
            self.complete_operation(
                operation,
                OperationOutcome::Completed {
                    stop: Some(stop.clone()),
                },
            );
        }
        stop
    }

    pub(crate) fn note_active_context(&mut self, context: Option<ContextId>) {
        if self.observed_context != context {
            self.observed_context = context;
            self.push_notification(NotificationPayload::ContextChanged { active: context });
        }
    }

    pub(crate) fn stop_at_terminal_halt(
        &mut self,
        context: Option<ContextId>,
        location: Option<ExecutionLocation>,
        guest_tick: Option<u32>,
        execution_units: Option<u64>,
    ) -> Option<StopInfo> {
        if self.terminal_announced {
            return None;
        }
        if let Some(step) = self.pending_step.take() {
            self.suppressed_breakpoints.clear();
            self.complete_operation(
                step.operation,
                OperationOutcome::Unavailable {
                    reason: "runner reached a terminal halt".into(),
                },
            );
        }
        let stop = self.stop(
            StopReason::TerminalHalt,
            context,
            location,
            guest_tick,
            execution_units,
        );
        self.terminal_announced = true;
        self.push_notification(NotificationPayload::TerminalState { halted: true });
        Some(stop)
    }

    fn create_operation(&mut self) -> OperationId {
        let id = self.next_operation_id.alloc();
        self.operations.insert(id, OperationState::Pending);
        self.operation_order.push_back(id);
        id
    }

    pub(crate) fn complete_operation(&mut self, id: OperationId, result: OperationOutcome) {
        if let Some(state) = self.operations.get_mut(&id) {
            let cloned = result.clone();
            *state = OperationState::Completed { result };
            self.push_notification(NotificationPayload::OperationCompleted {
                operation_id: id,
                result: Box::new(cloned),
            });
            self.prune_operations();
        }
    }

    fn prune_operations(&mut self) {
        while self
            .operations
            .values()
            .filter(|state| matches!(state, OperationState::Completed { .. }))
            .count()
            > OPERATION_RETENTION
        {
            let Some(index) = self.operation_order.iter().position(|id| {
                matches!(
                    self.operations.get(id),
                    Some(OperationState::Completed { .. })
                )
            }) else {
                break;
            };
            let id = self
                .operation_order
                .remove(index)
                .expect("located operation exists");
            self.operations.remove(&id);
            self.expired_operation_through = self.expired_operation_through.max(id.0);
        }
    }

    pub fn operation_status(&self, id: OperationId) -> Option<OperationStatus> {
        self.operations
            .get(&id)
            .cloned()
            .map(|state| OperationStatus { id, state })
            .or_else(|| {
                (id.0 <= self.expired_operation_through).then_some(OperationStatus {
                    id,
                    state: OperationState::Expired,
                })
            })
    }

    pub fn providers(&self) -> &[super::provider::ProviderRegistration] {
        &self.providers
    }

    pub fn provider(&self, id: ProviderId) -> Option<super::provider::ProviderRegistration> {
        self.providers
            .iter()
            .find(|provider| provider.descriptor().id == id)
            .cloned()
    }

    pub fn register_provider<P>(&mut self, provider: P) -> DebugResult<()>
    where
        P: super::provider::InspectionProvider + 'static,
    {
        self.register_provider_arc(std::sync::Arc::new(provider))
    }

    pub fn register_provider_arc(
        &mut self,
        provider: super::provider::ProviderRegistration,
    ) -> DebugResult<()> {
        super::provider::validate_provider_contract(provider.as_ref())?;
        let id = provider.descriptor().id;
        if self.provider(id).is_some() {
            return Err(DebugError::InvalidValue {
                detail: format!("provider {id} is already registered"),
            });
        }
        self.providers.push(provider);
        Ok(())
    }

    pub fn adapter_registrations(&self) -> &[super::adapters::AdapterRegistration] {
        &self.adapter_registrations
    }

    pub fn register_adapter(
        &mut self,
        registration: super::adapters::AdapterRegistration,
    ) -> DebugResult<()> {
        if registration.context == ContextId::UNSPECIFIED
            || registration
                .address_spaces
                .iter()
                .any(|space| *space == AddressSpaceId::UNSPECIFIED)
            || registration
                .address_spaces
                .iter()
                .enumerate()
                .any(|(index, space)| registration.address_spaces[..index].contains(space))
        {
            return Err(DebugError::InvalidValue {
                detail: "adapter context and address-space IDs must be non-zero and unique within a registration".into(),
            });
        }
        if self.adapter_registrations.iter().any(|existing| {
            existing.context == registration.context
                || existing
                    .address_spaces
                    .iter()
                    .any(|space| registration.address_spaces.contains(space))
        }) {
            return Err(DebugError::InvalidValue {
                detail: "adapter context and address-space IDs must be unique".into(),
            });
        }
        self.adapter_registrations.push(registration);
        Ok(())
    }

    pub(crate) fn alloc_artifact(&mut self) -> u64 {
        self.capture_store.alloc_artifact()
    }

    fn alloc_capture(&mut self) -> CaptureId {
        self.capture_store.alloc_capture()
    }

    pub fn begin_capture(
        &mut self,
        request: CaptureRequest,
        boundary: CaptureBoundaryKind,
    ) -> PendingCapture {
        let pending = self.make_capture(request, boundary);
        self.pending_captures.push(pending.clone());
        pending
    }

    pub fn begin_immediate_capture(&mut self, request: CaptureRequest) -> PendingCapture {
        self.make_capture(request, CaptureBoundaryKind::CommandSafePoint)
    }

    fn make_capture(
        &mut self,
        request: CaptureRequest,
        boundary: CaptureBoundaryKind,
    ) -> PendingCapture {
        let operation = self.create_operation();
        let capture = self.alloc_capture();
        let pause = request.mode == CaptureMode::PauseAtBoundary;
        PendingCapture {
            operation,
            capture,
            request,
            boundary,
            pause,
        }
    }

    pub(crate) fn take_pending_captures(
        &mut self,
        boundary: CaptureBoundaryKind,
    ) -> Vec<PendingCapture> {
        let mut taken = Vec::new();
        let mut remaining = Vec::with_capacity(self.pending_captures.len());
        for pending in std::mem::take(&mut self.pending_captures) {
            if pending.boundary == boundary {
                taken.push(pending);
            } else {
                remaining.push(pending);
            }
        }
        self.pending_captures = remaining;
        taken
    }

    pub fn cancel_capture(&mut self, operation: OperationId) -> bool {
        let Some(index) = self
            .pending_captures
            .iter()
            .position(|pending| pending.operation == operation)
        else {
            return false;
        };
        self.pending_captures.remove(index);
        self.complete_operation(operation, OperationOutcome::Cancelled);
        true
    }

    pub(crate) fn pending_capture_count(&self) -> usize {
        self.pending_captures.len()
    }

    pub(crate) fn has_pending_capture_boundary(&self, boundary: CaptureBoundaryKind) -> bool {
        self.pending_captures
            .iter()
            .any(|capture| capture.boundary == boundary)
    }

    pub(crate) fn fail_pending_captures(&mut self, reason: impl Into<String>) {
        let reason = reason.into();
        for pending in std::mem::take(&mut self.pending_captures) {
            self.complete_operation(
                pending.operation,
                OperationOutcome::Unavailable {
                    reason: reason.clone(),
                },
            );
        }
    }

    pub(crate) fn stop_at_capture_boundary(
        &mut self,
        capture: CaptureId,
        context: Option<ContextId>,
        location: Option<ExecutionLocation>,
        guest_tick: Option<u32>,
        execution_units: Option<u64>,
    ) -> StopInfo {
        self.stop(
            StopReason::CaptureBoundary { id: capture },
            context,
            location,
            guest_tick,
            execution_units,
        )
    }

    pub fn insert_capture(&mut self, capture: RetainedCapture) -> Option<CaptureId> {
        let id = capture.manifest.id;
        let evicted = self.capture_store.insert(capture);
        self.push_notification(NotificationPayload::CaptureAvailable { id });
        evicted
    }

    pub fn extract_capture_store(&mut self) -> CaptureStore {
        self.fail_pending_captures("runner replacing capture store");
        self.capture_store.drain()
    }

    pub fn adopt_capture_store(&mut self, store: CaptureStore) {
        self.capture_store.adopt(store);
    }

    pub fn capture(&self, id: CaptureId) -> Option<&RetainedCapture> {
        self.capture_store.get(id)
    }

    pub fn capture_snapshot(
        &self,
        capture: CaptureId,
        provider: ProviderId,
    ) -> Option<super::provider::ProviderSnapshot> {
        self.capture_store.snapshot(capture, provider)
    }

    pub fn artifact(&self, id: ArtifactId) -> Option<(super::model::ArtifactDescriptor, Vec<u8>)> {
        self.capture_store.artifact(id)
    }

    pub fn release_capture(&mut self, id: CaptureId) -> bool {
        self.capture_store.release(id)
    }

    pub fn captures(&self) -> Vec<super::model::CaptureManifest> {
        self.capture_store.manifests()
    }

    pub fn set_capture_retention(&mut self, retention: usize) {
        self.capture_store.set_retention(retention);
    }

    pub fn push_notification(&mut self, payload: NotificationPayload) -> u64 {
        let sequence = self.next_notification_sequence;
        self.next_notification_sequence = self.next_notification_sequence.saturating_add(1);
        self.notifications
            .push_back(DebugNotification { sequence, payload });
        while self.notifications.len() > NOTIFICATION_RETENTION {
            self.notifications.pop_front();
        }
        sequence
    }

    pub fn notifications_since(&self, after: u64) -> Vec<DebugNotification> {
        self.notifications
            .iter()
            .filter(|notification| notification.sequence > after)
            .cloned()
            .collect()
    }

    pub fn notification_history_available_after(&self, after: u64) -> bool {
        self.notifications
            .front()
            .is_none_or(|first| first.sequence <= after.saturating_add(1))
    }

    pub fn last_notification_sequence(&self) -> Option<u64> {
        self.notifications
            .back()
            .map(|notification| notification.sequence)
    }
}

impl Default for DebuggerCoordinator {
    fn default() -> Self {
        Self::new(SessionId(1))
    }
}


#[cfg(test)]
#[path = "coordinator_tests.rs"]
mod tests;
