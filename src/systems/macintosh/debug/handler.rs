use super::adapters::{active_context_and_location, Adapters, MAX_ARTIFACT_TRANSFER};
use super::capabilities::{CapabilitySet, CapabilityTarget, DecoderCapabilities};
use super::capture::{boundary_synchronization_notes, capture_boundary_supported, request_capture};
use super::error::{DebugError, DebugResult};
use super::ids::ProviderId;
use super::model::{CaptureBoundaryCapabilities, DebugExecutionState, OperationOutcome};
#[cfg(test)]
use super::provider::builtin_providers;
use super::request::DebugRequest;
use super::request::{DebugReply, ExecutionStatusReply};
use crate::runner::FixtureRunner;

fn execution_status(runner: &FixtureRunner) -> DebugResult<ExecutionStatusReply> {
    let coordinator = &runner.debug;
    let adapters = Adapters::for_runner(runner);
    adapters.validate()?;
    Ok(ExecutionStatusReply {
        state: coordinator.execution_state().clone(),
        terminal: runner.is_halted(),
        active_context: adapters.active().map(|adapter| adapter.context().id),
        guest_tick: Some(runner.guest_tick()),
        execution_units: Some(runner.total_instructions()),
        revision: coordinator.revision(),
    })
}

/// Apply a request between scheduler calls on the runner's owning thread.
/// Returns owned data or records execution intent without running guest code.
/// For accepted operations, advance the scheduler and poll
/// [`DebugRequest::GetOperation`]; terminal runners remain inspectable.
pub fn handle_debug_request(
    runner: &mut FixtureRunner,
    request: DebugRequest,
) -> crate::debug::error::DebugResult<DebugReply> {
    use DebugRequest::*;
    match request {
        InSession {
            session,
            generation,
            request,
        } => {
            if session != runner.debug.session() || generation != runner.debug.generation() {
                return Err(DebugError::StaleReference {
                    detail: "request belongs to another runner session or generation".into(),
                });
            }
            if matches!(*request, InSession { .. }) {
                return Err(DebugError::InvalidValue {
                    detail: "nested session envelopes are unsupported".into(),
                });
            }
            handle_debug_request(runner, *request)
        }
        ReadProvider { id } => {
            let provider = runner
                .debug
                .provider(id)
                .ok_or_else(|| DebugError::unsupported("provider snapshot"))?;
            Ok(DebugReply::ProviderSnapshot(provider.snapshot_at(
                runner,
                super::model::CaptureBoundaryKind::CommandSafePoint,
            )?))
        }
        ReadCaptureProvider { capture, provider } => {
            if runner.debug.capture(capture).is_none() {
                return Err(DebugError::CaptureExpired { id: capture });
            }
            let snapshot = runner.debug.capture_snapshot(capture, provider).ok_or(
                DebugError::InvalidValue {
                    detail: format!("capture {capture} did not retain provider {provider}"),
                },
            )?;
            Ok(DebugReply::ProviderSnapshot(snapshot))
        }
        SessionInfo => Ok(DebugReply::SessionInfo(super::request::SessionInfo {
            session: runner.debug.session(),
            generation: runner.debug.generation(),
            active_context: execution_status(runner)?.active_context,
            terminal: runner.is_halted(),
        })),
        ListContexts => {
            let adapters = Adapters::for_runner(runner);
            adapters.validate()?;
            Ok(DebugReply::Contexts(adapters.contexts()))
        }
        ListAddressSpaces { context } => {
            let adapters = Adapters::for_runner(runner);
            adapters.validate()?;
            let context = adapters.resolve(context)?;
            let adapter = adapters
                .find(context)
                .ok_or(DebugError::UnknownContext { id: context })?;
            Ok(DebugReply::AddressSpaces(adapter.address_spaces()))
        }
        ListProviders => Ok(DebugReply::Providers(
            runner
                .debug
                .providers()
                .iter()
                .map(|provider| provider.descriptor())
                .collect(),
        )),
        GetCapabilities { target } => capability_reply(runner, target),
        Notifications { after } => {
            if !runner.debug.notification_history_available_after(after) {
                return Err(DebugError::StaleReference {
                    detail: "notification cursor predates retained history; refresh debugger state"
                        .into(),
                });
            }
            Ok(DebugReply::Notifications(
                runner.debug.notifications_since(after),
            ))
        }
        ExecutionStatus => Ok(DebugReply::ExecutionStatus(execution_status(runner)?)),
        Pause => {
            let (context, location) = active_context_and_location(runner);
            let guest_tick = runner.guest_tick();
            let execution_units = runner.total_instructions();
            runner
                .debug
                .pause_at(context, location, Some(guest_tick), Some(execution_units));
            Ok(DebugReply::Accepted {
                state: runner.debug.execution_state().clone(),
                operation_id: None,
            })
        }
        Resume => {
            if runner.is_halted() {
                return Err(DebugError::invalid_state("cannot resume a terminal runner"));
            }
            if runner.debug.execution_state().is_stepping() {
                return Err(DebugError::invalid_state(
                    "cannot resume while a step is pending; pause to cancel the step",
                ));
            }
            if runner.debug.resume() {
                runner.debug_release_held_input();
            }
            Ok(DebugReply::Accepted {
                state: runner.debug.execution_state().clone(),
                operation_id: None,
            })
        }
        Step { context } => {
            if runner.is_halted() {
                return Err(DebugError::invalid_state("cannot step a terminal runner"));
            }
            if !runner.debug.is_paused() {
                return Err(DebugError::invalid_state("step requires a paused runner"));
            }
            let adapters = Adapters::for_runner(runner);
            adapters.validate()?;
            let context = adapters.resolve(context)?;
            let adapter = adapters
                .find(context)
                .ok_or(DebugError::UnknownContext { id: context })?;
            if !adapter.context().capabilities.step {
                return Err(DebugError::unsupported("step for this execution context"));
            }
            if adapters.active().map(|adapter| adapter.context().id) != Some(context) {
                return Err(DebugError::invalid_state(
                    "step requires the selected execution context to be active",
                ));
            }
            drop(adapters);
            let operation_id = runner.debug.begin_step(context);
            Ok(DebugReply::Accepted {
                state: DebugExecutionState::Stepping { operation_id },
                operation_id: Some(operation_id),
            })
        }
        ReadRegisters { context, registers } => {
            let adapters = Adapters::for_runner(runner);
            adapters.validate()?;
            let context = adapters.resolve(context)?;
            let adapter = adapters
                .find(context)
                .ok_or(DebugError::UnknownContext { id: context })?;
            let mut snapshots = adapter.registers(context)?;
            if let Some(filter) = registers {
                snapshots.retain(|snapshot| filter.contains(&snapshot.descriptor.id));
            }
            Ok(DebugReply::Registers {
                context,
                registers: snapshots,
            })
        }
        ReadMemory {
            space,
            address,
            length,
        } => {
            let adapters = Adapters::for_runner(runner);
            adapters.validate()?;
            let adapter = adapters.by_space(space).ok_or(DebugError::Inaccessible {
                space,
                detail: "unknown address space".to_string(),
            })?;
            Ok(DebugReply::Memory(
                adapter.read_memory(space, address, length)?,
            ))
        }
        Disassemble {
            context,
            address,
            count,
        } => {
            let adapters = Adapters::for_runner(runner);
            adapters.validate()?;
            let context = adapters.resolve(context)?;
            let adapter = adapters
                .find(context)
                .ok_or(DebugError::UnknownContext { id: context })?;
            Ok(DebugReply::Disassembly {
                context,
                lines: adapter.disassemble(context, address, count)?,
            })
        }
        StackPreview { context, words } => {
            let adapters = Adapters::for_runner(runner);
            adapters.validate()?;
            let context = adapters.resolve(context)?;
            let adapter = adapters
                .find(context)
                .ok_or(DebugError::UnknownContext { id: context })?;
            Ok(DebugReply::Stack(adapter.stack_preview(context, words)?))
        }
        ListBreakpoints => Ok(DebugReply::Breakpoints(runner.debug.breakpoints())),
        SetBreakpoint { mut spec } => {
            let adapters = Adapters::for_runner(runner);
            adapters.validate()?;
            let enforced_context = match spec.context {
                Some(id) => id,
                None => adapters
                    .active()
                    .map(|adapter| adapter.context().id)
                    .ok_or_else(|| DebugError::invalid_state("no active execution context"))?,
            };
            let adapter = adapters
                .find(enforced_context)
                .ok_or(DebugError::UnknownContext {
                    id: enforced_context,
                })?;
            let descriptor = adapter.context();
            if !descriptor.capabilities.set_breakpoint {
                return Err(DebugError::unsupported(format!(
                    "breakpoint enforcement for context {enforced_context}"
                )));
            }
            let spaces = adapter.address_spaces();
            if spec.address.space == super::ids::AddressSpaceId::UNSPECIFIED {
                let mut executable = spaces.iter().filter(|space| space.access.execute);
                let space = executable.next().ok_or_else(|| {
                    DebugError::invalid_state("context has no executable address space")
                })?;
                if executable.next().is_some() {
                    return Err(DebugError::InvalidValue {
                        detail: "breakpoint must name an address space for this context".into(),
                    });
                }
                spec.address.space = space.id;
            }
            let space = spaces
                .iter()
                .find(|space| space.id == spec.address.space && space.access.execute)
                .ok_or(DebugError::Inaccessible {
                    space: spec.address.space,
                    detail: "address space is not executable in this context".into(),
                })?;
            if space.address_bits < 64 && spec.address.offset >= (1u64 << space.address_bits) {
                return Err(DebugError::InvalidValue {
                    detail: format!("breakpoint address must fit in {} bits", space.address_bits),
                });
            }
            drop(adapters);
            spec.context = Some(enforced_context);
            if runner.debug.breakpoints().iter().any(|existing| {
                existing.spec.kind == spec.kind
                    && existing.spec.address == spec.address
                    && existing.spec.context == spec.context
            }) {
                return Err(DebugError::InvalidValue {
                    detail: "a breakpoint already targets this context and address".to_string(),
                });
            }
            Ok(DebugReply::Breakpoint(runner.debug.insert_breakpoint(spec)))
        }
        RemoveBreakpoint { id } => {
            if runner.debug.remove_breakpoint(id) {
                Ok(DebugReply::BreakpointRemoved { id })
            } else {
                Err(DebugError::InvalidValue {
                    detail: format!("unknown breakpoint {id}"),
                })
            }
        }
        SetBreakpointEnabled { id, enabled } => runner
            .debug
            .set_breakpoint_enabled(id, enabled)
            .map(DebugReply::Breakpoint)
            .ok_or(DebugError::InvalidValue {
                detail: format!("unknown breakpoint {id}"),
            }),
        ListCaptures => Ok(DebugReply::Captures(runner.debug.captures())),
        RequestCapture { request } => request_capture(runner, request),
        CancelCapture { operation } => {
            if runner.debug.cancel_capture(operation) {
                Ok(DebugReply::Operation(super::model::OperationStatus {
                    id: operation,
                    state: super::model::OperationState::Completed {
                        result: OperationOutcome::Cancelled,
                    },
                }))
            } else {
                Err(DebugError::UnknownOperation { id: operation })
            }
        }
        GetCapture { id } => match runner.debug.capture(id) {
            Some(capture) => Ok(DebugReply::Capture(capture.manifest.clone())),
            None => Err(DebugError::CaptureExpired { id }),
        },
        ReleaseCapture { id } => {
            if runner.debug.release_capture(id) {
                Ok(DebugReply::CaptureReleased { id })
            } else {
                Err(DebugError::CaptureExpired { id })
            }
        }
        GetOperation { id } => runner
            .debug
            .operation_status(id)
            .map(DebugReply::Operation)
            .ok_or(DebugError::UnknownOperation { id }),
        GetArtifact { id } => match runner.debug.artifact(id) {
            Some((descriptor, bytes)) => {
                if bytes.len() as u64 > MAX_ARTIFACT_TRANSFER {
                    return Err(DebugError::TooLarge {
                        limit: MAX_ARTIFACT_TRANSFER,
                        requested: bytes.len() as u64,
                    });
                }
                Ok(DebugReply::ArtifactData { descriptor, bytes })
            }
            None => Err(DebugError::InvalidValue {
                detail: format!("unknown artifact {id}"),
            }),
        },
        DecodeArtifact { id } => Err(DebugError::unsupported(format!(
            "artifact decoding for {id}"
        ))),
        ResolveSymbol {
            context,
            address,
            window,
        } => {
            symbol_context(runner, context)?;
            let window = window.unwrap_or(super::symbols::DEFAULT_RESOLVE_WINDOW);
            Ok(DebugReply::Symbol(super::symbols::resolve_containing(
                runner,
                address.offset,
                window,
            )?))
        }
        LookupSymbol { context, name } => {
            symbol_context(runner, context)?;
            Ok(DebugReply::Symbols(super::symbols::lookup(
                runner,
                super::provider::SYMBOL_SCAN_START,
                super::provider::SYMBOL_SCAN_LENGTH,
                &name,
            )?))
        }
    }
}

/// Symbol recovery reads 68K guest memory, so it requires a resolvable 68K
/// context. Validating here keeps the scanner free of context concerns.
fn symbol_context(
    runner: &FixtureRunner,
    context: super::model::ContextSelector,
) -> DebugResult<super::ids::ContextId> {
    let adapters = Adapters::for_runner(runner);
    adapters.validate()?;
    let context = adapters.resolve(context)?;
    if context != super::adapters::M68K_CONTEXT {
        return Err(DebugError::unsupported(
            "MacsBug symbol recovery is available on the 68K context only",
        ));
    }
    Ok(context)
}

fn capability_reply(runner: &FixtureRunner, target: CapabilityTarget) -> DebugResult<DebugReply> {
    let set = match target {
        CapabilityTarget::Context { id } => {
            let adapters = Adapters::for_runner(runner);
            adapters.validate()?;
            let adapter = adapters.find(id).ok_or(DebugError::UnknownContext { id })?;
            CapabilitySet::Context(adapter.context().capabilities)
        }
        CapabilityTarget::AddressSpace { id } => {
            let adapters = Adapters::for_runner(runner);
            adapters.validate()?;
            let adapter = adapters.by_space(id).ok_or(DebugError::Inaccessible {
                space: id,
                detail: "unknown address space".to_string(),
            })?;
            CapabilitySet::AddressSpace(adapter.address_space_capabilities(id)?)
        }
        CapabilityTarget::Provider { id } => {
            let provider = runner
                .debug
                .providers()
                .iter()
                .find(|provider| provider.descriptor().id == id)
                .ok_or(DebugError::InvalidValue {
                    detail: format!("unknown provider {id}"),
                })?;
            CapabilitySet::Provider(provider.capabilities())
        }
        CapabilityTarget::Boundary { boundary } => {
            if !capture_boundary_supported(boundary) {
                return Err(DebugError::BoundaryUnavailable {
                    detail: format!("capture boundary {boundary:?} is not observed by this runner"),
                });
            }
            let providers = runner
                .debug
                .providers()
                .iter()
                .filter(|provider| {
                    provider
                        .descriptor()
                        .required_boundary
                        .is_none_or(|required| required == boundary)
                })
                .map(|provider| provider.descriptor().id)
                .collect();
            CapabilitySet::Boundary(CaptureBoundaryCapabilities {
                kind: boundary,
                providers,
                atomic_across_providers: true,
                limitations: boundary_synchronization_notes(boundary),
            })
        }
        CapabilityTarget::Decoder { .. } => CapabilitySet::Decoder(DecoderCapabilities::default()),
    };
    Ok(DebugReply::Capabilities(set))
}

pub fn builtin_provider_ids() -> Vec<ProviderId> {
    super::provider::builtin_providers()
        .iter()
        .map(|provider| provider.descriptor().id)
        .collect()
}

#[cfg(test)]

#[cfg(test)]
#[path = "handler_tests.rs"]
mod tests;
