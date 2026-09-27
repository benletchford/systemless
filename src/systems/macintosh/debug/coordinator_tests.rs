    use super::*;
    use crate::debug::ids::{AddressSpaceId, ContextId};
    use crate::debug::model::{DebugAddress, ExecutionLocation};
    use crate::debug::request::BreakpointSpec;

    fn location(pc: u32) -> ExecutionLocation {
        ExecutionLocation {
            context: ContextId(1),
            address: DebugAddress::new(AddressSpaceId(1), u64::from(pc)),
        }
    }

    #[test]
    fn pause_and_resume_are_idempotent() {
        let mut coordinator = DebuggerCoordinator::new(SessionId(1));
        assert!(
            coordinator.resume() == false,
            "running runner is not paused"
        );
        coordinator.pause(None, None);
        assert!(coordinator.is_paused());
        coordinator.pause(None, None);
        assert!(coordinator.is_paused());
        assert!(coordinator.resume());
        assert!(coordinator.execution_state().is_running());
        assert!(!coordinator.resume());
    }

    #[test]
    fn breakpoints_have_stable_identity_and_counters() {
        let mut coordinator = DebuggerCoordinator::new(SessionId(1));
        let a = coordinator.insert_breakpoint(BreakpointSpec::program_counter(0x1000));
        let b = coordinator.insert_breakpoint(BreakpointSpec::program_counter(0x2000));
        assert_ne!(a.id, b.id);
        assert_eq!(coordinator.breakpoints().len(), 2);
        assert!(!coordinator.note_breakpoint_hit(a.id));
        assert_eq!(coordinator.breakpoints()[0].hit_count, 1);
        assert!(coordinator.remove_breakpoint(a.id));
        assert_eq!(coordinator.breakpoints().len(), 1);
    }

    #[test]
    fn notification_sequences_are_gapless() {
        let mut coordinator = DebuggerCoordinator::new(SessionId(1));
        let first =
            coordinator.push_notification(NotificationPayload::TerminalState { halted: false });
        let second =
            coordinator.push_notification(NotificationPayload::TerminalState { halted: true });
        assert_eq!(second, first + 1);
        assert_eq!(coordinator.notifications_since(first).len(), 1);
    }

    #[test]
    fn expired_notification_cursor_is_detectable() {
        let mut coordinator = DebuggerCoordinator::new(SessionId(1));
        for _ in 0..=NOTIFICATION_RETENTION {
            coordinator.push_notification(NotificationPayload::TerminalState { halted: false });
        }
        assert!(!coordinator.notification_history_available_after(0));
        assert!(coordinator.notification_history_available_after(1));
    }

    #[test]
    fn capture_retention_evicts_oldest() {
        let mut coordinator = DebuggerCoordinator::new(SessionId(1));
        coordinator.set_capture_retention(2);
        for id in 1..=3u64 {
            let manifest = super::super::model::CaptureManifest {
                id: CaptureId(id),
                session: SessionId(1),
                boundary: super::super::model::CaptureBoundaryKind::CommandSafePoint,
                providers: Vec::new(),
                revision: 0,
                stop: None,
                guest_tick: None,
                execution_units: None,
                artifacts: Vec::new(),
                omitted: Vec::new(),
                truncated: false,
                atomic: true,
                synchronization: Vec::new(),
            };
            coordinator.insert_capture(RetainedCapture {
                manifest,
                snapshots: Vec::new(),
                artifacts: Vec::new(),
            });
        }
        assert_eq!(coordinator.captures().len(), 2);
        assert!(coordinator.capture(CaptureId(1)).is_none());
        assert!(coordinator.capture(CaptureId(3)).is_some());
    }

    #[test]
    fn step_completes_after_one_unit_and_reports_an_operation() {
        let mut coordinator = DebuggerCoordinator::new(SessionId(1));
        coordinator.pause(None, None);
        let operation = coordinator.begin_step(ContextId(1));
        assert!(coordinator.is_stepping());
        assert_eq!(coordinator.step_units_remaining(), Some(1));
        assert!(matches!(
            coordinator.execution_state(),
            DebugExecutionState::Stepping { operation_id } if *operation_id == operation
        ));

        coordinator.note_executed_units(ContextId(1), 1);
        assert_eq!(coordinator.step_units_remaining(), Some(0));
        let stop = coordinator
            .finish_step(ContextId(1), Some(location(0x2000)), Some(7), Some(1))
            .expect("pending step");
        assert_eq!(stop.reason, StopReason::StepCompleted);
        assert!(!coordinator.is_stepping());
        assert!(coordinator.is_paused());
        // The completed step carries its own stop, so a client can learn the new
        // location without a follow-up ExecutionStatus request.
        let Some(OperationState::Completed {
            result: OperationOutcome::Completed { stop: carried },
        }) = coordinator
            .operation_status(operation)
            .map(|status| status.state)
        else {
            panic!("step operation completes with a stop");
        };
        let carried = carried.expect("step completion carries its stop");
        assert_eq!(carried.reason, StopReason::StepCompleted);
        assert_eq!(carried.id, stop.id);
        assert_eq!(carried.location, stop.location);
    }

    #[test]
    fn completed_steps_obey_operation_retention() {
        let mut coordinator = DebuggerCoordinator::new(SessionId(1));
        coordinator.pause(None, None);
        let mut first = OperationId::UNSPECIFIED;
        for index in 0..=OPERATION_RETENTION {
            let operation = coordinator.begin_step(ContextId(1));
            if index == 0 {
                first = operation;
            }
            coordinator.note_executed_units(ContextId(1), 1);
            coordinator.finish_step(ContextId(1), None, None, None);
        }
        assert!(matches!(
            coordinator
                .operation_status(first)
                .map(|status| status.state),
            Some(OperationState::Expired)
        ));
    }

    #[test]
    fn pausing_a_pending_step_cancels_its_operation() {
        let mut coordinator = DebuggerCoordinator::new(SessionId(1));
        coordinator.pause(None, None);
        let operation = coordinator.begin_step(ContextId(1));
        coordinator.pause(None, None);
        assert!(!coordinator.is_stepping());
        assert!(matches!(
            coordinator
                .operation_status(operation)
                .map(|status| status.state),
            Some(OperationState::Completed {
                result: OperationOutcome::Cancelled
            })
        ));
    }

    #[test]
    fn resume_suppresses_a_breakpoint_for_exactly_one_unit() {
        let mut coordinator = DebuggerCoordinator::new(SessionId(1));
        let mut spec = BreakpointSpec::program_counter(0x1234);
        spec.context = Some(ContextId(1));
        let info = coordinator.insert_breakpoint(spec);
        assert_eq!(
            coordinator.breakpoint_watch_addresses(ContextId(1), AddressSpaceId::UNSPECIFIED),
            vec![0x1234]
        );
        coordinator.breakpoint_stop(info.id, ContextId(1), Some(location(0x1234)), None, None);
        assert!(coordinator.is_paused());

        assert!(coordinator.resume());
        assert!(
            coordinator
                .breakpoint_watch_addresses(ContextId(1), AddressSpaceId::UNSPECIFIED)
                .is_empty(),
            "breakpoint is suppressed until the guarded unit retires"
        );
        coordinator.note_executed_units(ContextId(1), 1);
        assert_eq!(
            coordinator.breakpoint_watch_addresses(ContextId(1), AddressSpaceId::UNSPECIFIED),
            vec![0x1234],
            "breakpoint is restored after the guarded unit retires"
        );
        assert_eq!(
            coordinator.breakpoint_at(ContextId(1), AddressSpaceId::UNSPECIFIED, 0x1234),
            Some(info.id)
        );
    }

    #[test]
    fn another_context_does_not_consume_breakpoint_suppression() {
        let mut coordinator = DebuggerCoordinator::new(SessionId(1));
        let mut spec = BreakpointSpec::program_counter(0x1234);
        spec.context = Some(ContextId(1));
        let info = coordinator.insert_breakpoint(spec);
        coordinator.breakpoint_stop(info.id, ContextId(1), None, None, None);
        coordinator.resume();
        coordinator.note_executed_units(ContextId(2), 1);
        assert!(coordinator
            .breakpoint_watch_addresses(ContextId(1), AddressSpaceId::UNSPECIFIED)
            .is_empty());
        coordinator.note_executed_units(ContextId(1), 1);
        assert_eq!(
            coordinator.breakpoint_watch_addresses(ContextId(1), AddressSpaceId::UNSPECIFIED),
            vec![0x1234]
        );
    }

    #[test]
    fn stepping_from_a_breakpoint_suppresses_it_for_the_step() {
        let mut coordinator = DebuggerCoordinator::new(SessionId(1));
        let mut spec = BreakpointSpec::program_counter(0x1234);
        spec.context = Some(ContextId(1));
        let info = coordinator.insert_breakpoint(spec);
        coordinator.breakpoint_stop(info.id, ContextId(1), Some(location(0x1234)), None, None);
        coordinator.begin_step(ContextId(1));
        assert!(
            coordinator
                .breakpoint_watch_addresses(ContextId(1), AddressSpaceId::UNSPECIFIED)
                .is_empty(),
            "the step must be able to retire the instruction the breakpoint guards"
        );
        coordinator.note_executed_units(ContextId(1), 1);
        assert_eq!(
            coordinator.breakpoint_watch_addresses(ContextId(1), AddressSpaceId::UNSPECIFIED),
            vec![0x1234]
        );
    }

    #[test]
    fn disabled_breakpoints_are_not_watched() {
        let mut coordinator = DebuggerCoordinator::new(SessionId(1));
        let mut spec = BreakpointSpec::program_counter(0x1000);
        spec.context = Some(ContextId(1));
        let info = coordinator.insert_breakpoint(spec);
        assert!(coordinator.set_breakpoint_enabled(info.id, false).is_some());
        assert!(coordinator
            .breakpoint_watch_addresses(ContextId(1), AddressSpaceId::UNSPECIFIED)
            .is_empty());
        assert_eq!(
            coordinator.breakpoint_at(ContextId(1), AddressSpaceId::UNSPECIFIED, 0x1000),
            None
        );
    }

    #[test]
    fn abort_step_completes_the_operation_as_unavailable() {
        let mut coordinator = DebuggerCoordinator::new(SessionId(1));
        coordinator.pause(None, None);
        let operation = coordinator.begin_step(ContextId(1));
        coordinator.abort_step("runner halted");
        assert!(!coordinator.is_stepping());
        assert_eq!(coordinator.execution_state(), &DebugExecutionState::Running);
        assert!(matches!(
            coordinator
                .operation_status(operation)
                .map(|status| status.state),
            Some(OperationState::Completed {
                result: OperationOutcome::Unavailable { .. }
            })
        ));
    }

    #[test]
    fn finishing_a_step_without_one_pending_has_no_side_effect() {
        let mut coordinator = DebuggerCoordinator::new(SessionId(1));
        let revision = coordinator.revision();
        let stop = coordinator.finish_step(ContextId(1), Some(location(0x2000)), None, None);
        assert!(stop.is_none());
        assert_eq!(coordinator.revision(), revision);
        assert!(coordinator.execution_state().is_running());
        assert!(coordinator.notifications_since(0).is_empty());
    }

    #[test]
    fn one_shot_breakpoint_is_removed_after_its_hit() {
        let mut coordinator = DebuggerCoordinator::new(SessionId(1));
        let mut spec = BreakpointSpec::program_counter(0x1000);
        spec.one_shot = true;
        let info = coordinator.insert_breakpoint(spec);
        coordinator.breakpoint_stop(info.id, ContextId(1), Some(location(0x1000)), None, None);
        assert!(coordinator.breakpoints().is_empty());
    }

    #[test]
    fn terminal_notification_is_latched() {
        let mut coordinator = DebuggerCoordinator::new(SessionId(1));
        let first = coordinator.stop_at_terminal_halt(
            Some(ContextId(1)),
            Some(location(0x1000)),
            Some(7),
            Some(12),
        );
        let second = coordinator.stop_at_terminal_halt(None, None, None, None);
        assert!(first.is_some());
        assert!(second.is_none());
        assert!(matches!(
            coordinator.execution_state(),
            DebugExecutionState::Paused {
                stop: StopInfo {
                    reason: StopReason::TerminalHalt,
                    execution_units: Some(12),
                    ..
                }
            }
        ));
        let terminal: Vec<_> = coordinator
            .notifications_since(0)
            .into_iter()
            .filter(|notification| {
                matches!(
                    notification.payload,
                    NotificationPayload::TerminalState { halted: true }
                )
            })
            .collect();
        assert_eq!(terminal.len(), 1);
    }

    #[test]
    fn pending_captures_are_taken_only_at_their_boundary() {
        let mut coordinator = DebuggerCoordinator::new(SessionId(1));
        let request = super::super::request::CaptureRequest {
            mode: CaptureMode::NextBoundary,
            boundary: Some(super::super::model::CaptureBoundaryKind::LogicalFrameComplete),
            providers: Vec::new(),
            include_artifacts: true,
        };
        let pending = coordinator.begin_capture(
            request.clone(),
            super::super::model::CaptureBoundaryKind::LogicalFrameComplete,
        );
        assert_eq!(coordinator.pending_capture_count(), 1);
        assert!(coordinator
            .take_pending_captures(super::super::model::CaptureBoundaryKind::CommandSafePoint)
            .is_empty());
        let taken = coordinator
            .take_pending_captures(super::super::model::CaptureBoundaryKind::LogicalFrameComplete);
        assert_eq!(taken.len(), 1);
        assert_eq!(taken[0].capture, pending.capture);

        let second = coordinator.begin_capture(
            request,
            super::super::model::CaptureBoundaryKind::LogicalFrameComplete,
        );
        assert!(coordinator.cancel_capture(second.operation));
        assert!(!coordinator.cancel_capture(second.operation));
        assert!(matches!(
            coordinator
                .operation_status(second.operation)
                .map(|status| status.state),
            Some(OperationState::Completed {
                result: OperationOutcome::Cancelled
            })
        ));
    }

    #[test]
    fn capture_store_round_trips_and_preserves_session_identity() {
        let mut coordinator = DebuggerCoordinator::new(SessionId(7));
        let manifest = super::super::model::CaptureManifest {
            id: coordinator.alloc_capture(),
            session: SessionId(7),
            boundary: super::super::model::CaptureBoundaryKind::CommandSafePoint,
            providers: Vec::new(),
            revision: 0,
            stop: None,
            guest_tick: None,
            execution_units: None,
            artifacts: Vec::new(),
            omitted: Vec::new(),
            truncated: false,
            atomic: true,
            synchronization: Vec::new(),
        };
        let id = manifest.id;
        coordinator.insert_capture(RetainedCapture {
            manifest,
            snapshots: Vec::new(),
            artifacts: Vec::new(),
        });
        let store = coordinator.extract_capture_store();
        assert_eq!(store.len(), 1);
        assert!(coordinator.captures().is_empty());

        let mut replacement = DebuggerCoordinator::new(SessionId(8));
        replacement.adopt_capture_store(store);
        assert_eq!(replacement.captures().len(), 1);
        assert_eq!(
            replacement
                .capture(id)
                .map(|capture| capture.manifest.session),
            Some(SessionId(7))
        );
        assert_ne!(replacement.alloc_capture(), id);
        let first_artifact = replacement.alloc_artifact();
        let store = replacement.extract_capture_store();
        let mut second_replacement = DebuggerCoordinator::new(SessionId(9));
        second_replacement.adopt_capture_store(store);
        assert!(second_replacement.alloc_artifact() > first_artifact);
    }
