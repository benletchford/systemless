    use super::*;
    use crate::cpu::Register;
    use crate::debug::{BreakpointSpec, SessionId};
    use crate::memory::MemoryBus;
    use crate::runner::{FixtureRunner, FixtureRunnerConfig};

    use super::super::adapters::{
        m68k_disassembly, read_m68k_memory, ArchitectureAdapter, PpcAdapter, M68K_CONTEXT,
        M68K_SPACE, MAX_DISASSEMBLY_COUNT, MAX_MEMORY_TRANSFER, PPC_CONTEXT, PPC_SPACE,
    };
    use super::super::ids::AddressSpaceId;
    use super::super::model::{ContextSelector, DebugAddress, RegisterRole, RegisterValue};
    use super::super::request::CaptureRequest;

    fn runner() -> FixtureRunner {
        FixtureRunner::new(0x100000, FixtureRunnerConfig::default())
    }

    #[test]
    fn session_and_context_reply_identifies_the_68k_context() {
        let mut runner = runner();
        let info = handle_debug_request(&mut runner, DebugRequest::SessionInfo).unwrap();
        match info {
            DebugReply::SessionInfo(info) => {
                assert_ne!(info.session, SessionId::UNSPECIFIED);
                assert!(info.generation >= 1);
                assert_eq!(info.active_context, Some(M68K_CONTEXT));
                assert!(!info.terminal);
            }
            other => panic!("unexpected reply: {other:?}"),
        }

        let contexts = handle_debug_request(&mut runner, DebugRequest::ListContexts).unwrap();
        match contexts {
            DebugReply::Contexts(contexts) => {
                assert!(contexts.iter().any(|context| context.id == M68K_CONTEXT));
                assert!(!contexts.iter().any(|context| context.id == PPC_CONTEXT));
            }
            other => panic!("unexpected reply: {other:?}"),
        }
    }

    #[test]
    fn register_reply_reports_exact_values() {
        let mut runner = runner();
        runner.cpu_mut().write_reg(Register::D0, 0xDEAD_BEEF);
        runner.cpu_mut().write_reg(Register::A7, 0x0002_0000);
        let reply = handle_debug_request(
            &mut runner,
            DebugRequest::ReadRegisters {
                context: ContextSelector::Active,
                registers: None,
            },
        )
        .unwrap();
        let DebugReply::Registers { context, registers } = reply else {
            panic!("expected registers");
        };
        assert_eq!(context, M68K_CONTEXT);
        let d0 = registers
            .iter()
            .find(|snapshot| snapshot.descriptor.name == "D0")
            .unwrap();
        assert_eq!(d0.value.as_u64(), Some(0xDEAD_BEEF));
        let a7 = registers
            .iter()
            .find(|snapshot| snapshot.descriptor.name == "A7")
            .unwrap();
        assert_eq!(a7.value.as_u64(), Some(0x0002_0000));
        assert_eq!(a7.descriptor.role, Some(RegisterRole::StackPointer));
    }

    #[test]
    fn memory_read_is_observational_and_truncates_at_ram_end() {
        let mut runner = runner();
        runner.bus_mut().write_long(0x0F_FFFC, 0x1122_3344);
        let reply = handle_debug_request(
            &mut runner,
            DebugRequest::ReadMemory {
                space: M68K_SPACE,
                address: 0x0F_FFFC,
                length: 8,
            },
        )
        .unwrap();
        let DebugReply::Memory(result) = reply else {
            panic!("expected memory");
        };
        assert_eq!(result.bytes.len(), 4);
        assert!(result.truncated);
        assert_eq!(&result.bytes, &[0x11, 0x22, 0x33, 0x44]);
    }

    #[test]
    fn disassembly_reports_nop_with_byte_payload() {
        let mut runner = runner();
        runner.bus_mut().write_word(0x2_0000, 0x4E71);
        runner.bus_mut().write_word(0x2_0002, 0x4E75);
        let reply = handle_debug_request(
            &mut runner,
            DebugRequest::Disassemble {
                context: ContextSelector::Active,
                address: DebugAddress::new(M68K_SPACE, 0x2_0000),
                count: 2,
            },
        )
        .unwrap();
        let DebugReply::Disassembly { lines, .. } = reply else {
            panic!("expected disassembly");
        };
        assert_eq!(lines.len(), 2);
        assert_eq!(lines[0].bytes, vec![0x4E, 0x71]);
        assert_eq!(lines[1].text, "RTS");
    }

    #[test]
    fn pause_stops_run_steps_until_resumed() {
        let mut runner = runner();
        runner.bus_mut().write_word(0x2_0000, 0x4E71); // NOP
        runner.cpu_mut().write_reg(Register::PC, 0x2_0000);

        let accepted = handle_debug_request(&mut runner, DebugRequest::Pause).unwrap();
        assert!(matches!(accepted, DebugReply::Accepted { .. }));
        let (steps, running) = runner.run_steps(10, None);
        assert_eq!(steps, 0);
        assert!(running);

        handle_debug_request(&mut runner, DebugRequest::Resume).unwrap();
        let (steps, _) = runner.run_steps(1, None);
        assert!(steps >= 1);
    }

    #[test]
    fn pause_and_resume_are_idempotent() {
        let mut runner = runner();
        handle_debug_request(&mut runner, DebugRequest::Resume).unwrap();
        let first = handle_debug_request(&mut runner, DebugRequest::Pause).unwrap();
        let second = handle_debug_request(&mut runner, DebugRequest::Pause).unwrap();
        assert_eq!(first, second);
        handle_debug_request(&mut runner, DebugRequest::Resume).unwrap();
        handle_debug_request(&mut runner, DebugRequest::Resume).unwrap();
    }

    #[test]
    fn step_requires_a_paused_runner_and_retires_one_instruction() {
        let mut runner = runner();
        runner.bus_mut().write_word(0x2_0000, 0x4E71); // NOP
        runner.bus_mut().write_word(0x2_0002, 0x4E71); // NOP
        runner.cpu_mut().write_reg(Register::PC, 0x2_0000);

        let error = handle_debug_request(
            &mut runner,
            DebugRequest::Step {
                context: ContextSelector::Active,
            },
        )
        .unwrap_err();
        assert!(matches!(error, DebugError::InvalidState { .. }));

        handle_debug_request(&mut runner, DebugRequest::Pause).unwrap();
        let accepted = handle_debug_request(
            &mut runner,
            DebugRequest::Step {
                context: ContextSelector::Active,
            },
        )
        .unwrap();
        let DebugReply::Accepted {
            state:
                DebugExecutionState::Stepping {
                    operation_id: state_operation,
                },
            operation_id: Some(operation_id),
        } = accepted
        else {
            panic!("expected an accepted step");
        };
        assert_eq!(state_operation, operation_id);

        // The embedding advances the normal scheduler; the runner executes
        // exactly the one instruction the step authorized and then pauses.
        let (steps, running) = runner.run_steps(1, None);
        assert_eq!(steps, 1);
        assert!(running);
        assert_eq!(runner.cpu().core.pc, 0x2_0002);
        assert!(runner.debug.is_paused());
        match runner.debug.execution_state() {
            DebugExecutionState::Paused { stop } => {
                assert_eq!(stop.reason, super::super::model::StopReason::StepCompleted);
                assert_eq!(stop.execution_units, Some(1));
                assert_eq!(
                    stop.location.map(|location| location.address.offset),
                    Some(0x2_0002)
                );
            }
            other => panic!("expected a paused step, got {other:?}"),
        }
        let status =
            handle_debug_request(&mut runner, DebugRequest::GetOperation { id: operation_id })
                .unwrap();
        assert!(matches!(
            status,
            DebugReply::Operation(super::super::model::OperationStatus {
                state: super::super::model::OperationState::Completed { .. },
                ..
            })
        ));
    }

    #[test]
    fn resume_rejects_a_pending_step_and_pause_cancels_it() {
        let mut runner = runner();
        handle_debug_request(&mut runner, DebugRequest::Pause).unwrap();
        let accepted = handle_debug_request(
            &mut runner,
            DebugRequest::Step {
                context: ContextSelector::Active,
            },
        )
        .unwrap();
        let DebugReply::Accepted {
            operation_id: Some(operation),
            ..
        } = accepted
        else {
            panic!("expected step operation");
        };
        assert!(matches!(
            handle_debug_request(&mut runner, DebugRequest::Resume),
            Err(DebugError::InvalidState { .. })
        ));
        handle_debug_request(&mut runner, DebugRequest::Pause).unwrap();
        let status =
            handle_debug_request(&mut runner, DebugRequest::GetOperation { id: operation })
                .unwrap();
        assert!(matches!(
            status,
            DebugReply::Operation(super::super::model::OperationStatus {
                state: super::super::model::OperationState::Completed {
                    result: super::super::model::OperationOutcome::Cancelled
                },
                ..
            })
        ));
    }

    #[test]
    fn step_dispatch_through_a_trap_is_one_unit() {
        // An A-line trap counts as the step's one unit. Here the dispatch
        // terminates the runner, so the step operation reports an unavailable
        // outcome rather than a bogus completion, and no callback body runs.
        let mut runner = runner();
        runner.bus_mut().write_word(0x2_0000, 0xA9F4); // _ExitToShell
        runner.cpu_mut().write_reg(Register::PC, 0x2_0000);
        handle_debug_request(&mut runner, DebugRequest::Pause).unwrap();
        let accepted = handle_debug_request(
            &mut runner,
            DebugRequest::Step {
                context: ContextSelector::Active,
            },
        )
        .unwrap();
        let DebugReply::Accepted {
            operation_id: Some(operation_id),
            ..
        } = accepted
        else {
            panic!("expected an accepted step");
        };
        let (steps, running) = runner.run_steps(100, None);
        assert!(!running);
        assert_eq!(steps, 1);
        assert!(runner.is_halted());
        let status =
            handle_debug_request(&mut runner, DebugRequest::GetOperation { id: operation_id })
                .unwrap();
        assert!(matches!(
            status,
            DebugReply::Operation(super::super::model::OperationStatus {
                state: super::super::model::OperationState::Completed {
                    result: super::super::model::OperationOutcome::Unavailable { .. }
                },
                ..
            })
        ));
    }

    #[test]
    fn program_counter_breakpoints_stop_before_the_instruction() {
        let mut runner = runner();
        runner.bus_mut().write_word(0x2_0000, 0x4E71); // NOP  (skipped)
        runner.bus_mut().write_word(0x2_0002, 0x4E71); // NOP  (target)
        runner.bus_mut().write_word(0x2_0004, 0x4E71); // NOP
        runner.cpu_mut().write_reg(Register::PC, 0x2_0000);

        let reply = handle_debug_request(
            &mut runner,
            DebugRequest::SetBreakpoint {
                spec: BreakpointSpec::program_counter(0x2_0002),
            },
        )
        .unwrap();
        let DebugReply::Breakpoint(info) = reply else {
            panic!("expected a breakpoint");
        };

        let (steps, running) = runner.run_steps(100, None);
        assert_eq!(steps, 1, "the NOP before the breakpoint retires");
        assert!(running);
        assert_eq!(runner.cpu().core.pc, 0x2_0002, "stop before executing");
        match runner.debug.execution_state() {
            DebugExecutionState::Paused { stop } => match &stop.reason {
                super::super::model::StopReason::PcBreakpoint { id } => assert_eq!(*id, info.id),
                other => panic!("expected a PC breakpoint, got {other:?}"),
            },
            other => panic!("expected a pause, got {other:?}"),
        }
        let listed = handle_debug_request(&mut runner, DebugRequest::ListBreakpoints).unwrap();
        let DebugReply::Breakpoints(breakpoints) = listed else {
            panic!("expected breakpoints");
        };
        assert_eq!(breakpoints[0].hit_count, 1);

        // Resume suppresses the breakpoint for exactly one unit so the
        // guarded instruction retires and the PC advances.
        handle_debug_request(&mut runner, DebugRequest::Resume).unwrap();
        let (resumed_steps, _) = runner.run_steps(1, None);
        assert_eq!(resumed_steps, 1);
        assert_eq!(runner.cpu().core.pc, 0x2_0004);
    }

    #[test]
    fn stepping_from_a_breakpoint_executes_the_guarded_instruction() {
        let mut runner = runner();
        runner.bus_mut().write_word(0x2_0000, 0x4E71); // NOP  (target)
        runner.bus_mut().write_word(0x2_0002, 0x4E71); // NOP
        runner.cpu_mut().write_reg(Register::PC, 0x2_0000);
        handle_debug_request(
            &mut runner,
            DebugRequest::SetBreakpoint {
                spec: BreakpointSpec::program_counter(0x2_0000),
            },
        )
        .unwrap();

        let (_, running) = runner.run_steps(100, None);
        assert!(running);
        assert_eq!(runner.cpu().core.pc, 0x2_0000);
        assert!(runner.debug.is_paused());

        handle_debug_request(
            &mut runner,
            DebugRequest::Step {
                context: ContextSelector::Active,
            },
        )
        .unwrap();
        let (steps, _) = runner.run_steps(100, None);
        assert_eq!(steps, 1, "the guarded instruction must retire exactly once");
        assert_ne!(runner.cpu().core.pc, 0x2_0000);
        assert!(runner.debug.is_paused());
        match runner.debug.execution_state() {
            DebugExecutionState::Paused { stop } => {
                assert_eq!(stop.reason, super::super::model::StopReason::StepCompleted)
            }
            other => panic!("expected a paused step, got {other:?}"),
        }
    }

    #[test]
    fn breakpoint_suppression_rearms_after_a_self_looping_instruction() {
        let mut runner = runner();
        runner.bus_mut().write_word(0x2_0000, 0x60fe); // BRA.S $20000
        runner.cpu_mut().write_reg(Register::PC, 0x2_0000);
        handle_debug_request(
            &mut runner,
            DebugRequest::SetBreakpoint {
                spec: BreakpointSpec::program_counter(0x2_0000),
            },
        )
        .unwrap();

        let (initial_steps, _) = runner.run_steps(8, None);
        assert_eq!(initial_steps, 0);
        handle_debug_request(&mut runner, DebugRequest::Resume).unwrap();
        let (resumed_steps, _) = runner.run_steps(8, None);
        assert_eq!(resumed_steps, 1);
        assert_eq!(runner.cpu().core.pc, 0x2_0000);

        let (second_stop_steps, _) = runner.run_steps(8, None);
        assert_eq!(second_stop_steps, 0);
        assert!(matches!(
            runner.debug.execution_state(),
            DebugExecutionState::Paused {
                stop: super::super::model::StopInfo {
                    reason: super::super::model::StopReason::PcBreakpoint { .. },
                    ..
                }
            }
        ));
    }

    #[test]
    fn breakpoints_require_an_installed_context_with_control_support() {
        let mut runner = runner();
        let reply = handle_debug_request(
            &mut runner,
            DebugRequest::SetBreakpoint {
                spec: BreakpointSpec::program_counter(0x2_0000),
            },
        )
        .unwrap();
        assert!(matches!(reply, DebugReply::Breakpoint(_)));
        let mut spec = BreakpointSpec::program_counter(0x2_0000);
        spec.context = Some(PPC_CONTEXT);
        assert!(matches!(
            handle_debug_request(&mut runner, DebugRequest::SetBreakpoint { spec }),
            Err(DebugError::UnknownContext { id }) if id == PPC_CONTEXT
        ));
        assert_eq!(runner.debug.breakpoints().len(), 1);
    }

    #[test]
    fn duplicate_breakpoint_targets_are_rejected() {
        let mut runner = runner();
        let spec = BreakpointSpec::program_counter(0x2_0000);
        handle_debug_request(
            &mut runner,
            DebugRequest::SetBreakpoint { spec: spec.clone() },
        )
        .unwrap();
        assert!(matches!(
            handle_debug_request(&mut runner, DebugRequest::SetBreakpoint { spec }),
            Err(DebugError::InvalidValue { .. })
        ));
    }

    #[test]
    fn cached_references_reject_replacement_sessions_and_generations() {
        let mut first = runner();
        let mut replacement = runner();
        let request = DebugRequest::InSession {
            session: first.debug.session(),
            generation: first.debug.generation(),
            request: Box::new(DebugRequest::ReadRegisters {
                context: ContextSelector::explicit(M68K_CONTEXT),
                registers: None,
            }),
        };
        handle_debug_request(&mut first, request.clone()).unwrap();
        assert!(matches!(
            handle_debug_request(&mut replacement, request.clone()),
            Err(DebugError::StaleReference { .. })
        ));
        first.debug.advance_generation();
        assert!(matches!(
            handle_debug_request(&mut first, request),
            Err(DebugError::StaleReference { .. })
        ));
    }

    #[test]
    fn pause_blocks_direct_execution_audio_and_frame_work() {
        let mut runner = runner();
        runner.bus_mut().write_word(0x20000, 0x4e71);
        runner.cpu_mut().write_reg(Register::PC, 0x20000);
        handle_debug_request(&mut runner, DebugRequest::Pause).unwrap();
        let tick = runner.guest_tick();
        let memory = runner.bus().ram_slice(0, runner.bus().ram_size()).to_vec();
        assert!(matches!(runner.step(), crate::cpu::StepResult::Blocked));
        runner.run().unwrap();
        assert_eq!(runner.run_pending_sound_work(100), (0, true));
        runner.mix_audio(100);
        runner.mix_gui_audio_slice(100);
        runner.composite_frame();
        assert_eq!(runner.cpu().core.pc, 0x20000);
        assert_eq!(runner.guest_tick(), tick);
        assert_eq!(runner.bus().ram_slice(0, runner.bus().ram_size()), memory);
    }

    #[test]
    fn providers_are_owned_bounded_and_observational() {
        let mut runner = runner();
        let before = runner.event_manager_snapshot();
        let memory = runner.bus().ram_slice(0, runner.bus().ram_size()).to_vec();
        for provider in builtin_providers() {
            let reply = handle_debug_request(
                &mut runner,
                DebugRequest::ReadProvider {
                    id: provider.descriptor().id,
                },
            )
            .unwrap();
            serde_json::to_string(&reply).unwrap();
        }
        assert_eq!(runner.event_manager_snapshot(), before);
        assert_eq!(runner.bus().ram_slice(0, runner.bus().ram_size()), memory);
        for _ in 0..300 {
            runner
                .dispatcher_mut()
                .event_queue
                .push_back(crate::event_queue::QueuedEvent {
                    what: 3,
                    message: 0,
                    when: 0,
                    where_v: 0,
                    where_h: 0,
                    modifiers: 0,
                });
        }
        let reply = handle_debug_request(
            &mut runner,
            DebugRequest::ReadProvider { id: ProviderId(3) },
        )
        .unwrap();
        let DebugReply::ProviderSnapshot(super::super::provider::ProviderSnapshot::Events {
            state,
            truncated,
            ..
        }) = reply
        else {
            panic!("events");
        };
        assert!(truncated);
        assert_eq!(state.queued_event_types.len(), 256);
        let count = state.queue_len;
        runner.push_key_down(1, b'b');
        assert_eq!(state.queue_len, count);
    }

    #[test]
    fn execution_publishes_context_and_terminal_notifications() {
        let mut runner = runner();
        runner.bus_mut().write_word(0x20000, 0xa9f4);
        runner.cpu_mut().write_reg(Register::PC, 0x20000);
        runner.run_steps(1, None);
        assert!(runner.is_halted());

        let reply =
            handle_debug_request(&mut runner, DebugRequest::Notifications { after: 0 }).unwrap();
        let DebugReply::Notifications(notifications) = reply else {
            panic!("expected notifications");
        };
        let context_changes = notifications
            .iter()
            .filter(|notification| {
                matches!(
                    notification.payload,
                    super::super::model::NotificationPayload::ContextChanged { .. }
                )
            })
            .count();
        let terminals = notifications
            .iter()
            .filter(|notification| {
                matches!(
                    notification.payload,
                    super::super::model::NotificationPayload::TerminalState { halted: true }
                )
            })
            .count();
        assert_eq!(context_changes, 1);
        assert_eq!(terminals, 1);
    }

    #[test]
    fn terminal_runner_can_be_inspected_but_not_resumed() {
        let mut runner = runner();
        runner.bus_mut().write_word(0x20000, 0xa9f4);
        runner.cpu_mut().write_reg(Register::PC, 0x20000);
        runner.run_steps(1, None);
        assert!(runner.is_halted());
        handle_debug_request(&mut runner, DebugRequest::Pause).unwrap();
        assert!(matches!(
            handle_debug_request(&mut runner, DebugRequest::Resume),
            Err(DebugError::InvalidState { .. })
        ));
        handle_debug_request(
            &mut runner,
            DebugRequest::ReadRegisters {
                context: ContextSelector::Active,
                registers: None,
            },
        )
        .unwrap();
        assert!(runner.debug.is_paused());
    }

    #[test]
    fn pause_withholds_input_and_resume_releases_held_keys() {
        let mut runner = runner();
        runner.push_key_down(0, b'a');
        handle_debug_request(&mut runner, DebugRequest::Pause).unwrap();
        let state = runner.event_manager_snapshot();
        runner.push_key_up(0, b'a');
        runner.push_key_down(1, b'b');
        runner.push_mouse_down(10, 20);
        runner.set_mouse_position(30, 40);
        assert_eq!(state, runner.event_manager_snapshot());
        handle_debug_request(&mut runner, DebugRequest::Resume).unwrap();
        assert_eq!(runner.event_manager_snapshot().key_map, [0; 16]);
    }

    #[test]
    fn unavailable_capabilities_and_exact_wide_registers() {
        let mut runner = runner();
        let reply = handle_debug_request(
            &mut runner,
            DebugRequest::GetCapabilities {
                target: CapabilityTarget::AddressSpace { id: M68K_SPACE },
            },
        )
        .unwrap();
        assert!(
            matches!(reply, DebugReply::Capabilities(CapabilitySet::AddressSpace(caps)) if !caps.write)
        );
        assert!(matches!(
            handle_debug_request(
                &mut runner,
                DebugRequest::GetCapabilities {
                    target: CapabilityTarget::Boundary {
                        boundary: super::super::model::CaptureBoundaryKind::BackendReadbackReady
                    }
                }
            ),
            Err(DebugError::BoundaryUnavailable { .. })
        ));
        let value = RegisterValue::unsigned(0xffff_ffff_ffff_fffe, 64);
        let json = serde_json::to_string(&value).unwrap();
        assert!(json.contains("bytes"));
        let decoded: RegisterValue = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.as_u64(), Some(0xffff_ffff_ffff_fffe));
    }

    #[test]
    fn unknown_space_is_rejected() {
        let mut runner = runner();
        let error = handle_debug_request(
            &mut runner,
            DebugRequest::ReadMemory {
                space: AddressSpaceId(999),
                address: 0,
                length: 1,
            },
        )
        .unwrap_err();
        assert!(matches!(error, DebugError::Inaccessible { .. }));
    }

    #[test]
    fn ppc_adapter_exposes_registers_and_reports_unsupported_inspection() {
        let mut cpu = ppc::PpcCpu::new();
        cpu.gpr[3] = 0x1234_5678;
        cpu.pc = 0x0000_4000;
        cpu.fpr[2] = 0x3FF0_0000_0000_0000;
        let adapter = PpcAdapter::new(&cpu, PPC_CONTEXT, PPC_SPACE, "native-application");

        assert_eq!(adapter.context().id, PPC_CONTEXT);
        assert_eq!(adapter.address_spaces()[0].id, PPC_SPACE);
        let registers = adapter.registers(PPC_CONTEXT).unwrap();
        let r3 = registers
            .iter()
            .find(|snapshot| snapshot.descriptor.name == "r3")
            .unwrap();
        assert_eq!(r3.value.as_u64(), Some(0x1234_5678));
        let pc = registers
            .iter()
            .find(|snapshot| snapshot.descriptor.name == "pc")
            .unwrap();
        assert_eq!(pc.value.as_u64(), Some(0x0000_4000));
        let f2 = registers
            .iter()
            .find(|snapshot| snapshot.descriptor.name == "f2")
            .unwrap();
        assert_eq!(f2.value.as_u64(), Some(0x3FF0_0000_0000_0000));

        assert!(adapter.registers(M68K_CONTEXT).is_err());
        assert!(adapter
            .disassemble(PPC_CONTEXT, DebugAddress::new(PPC_SPACE, 0), 1)
            .is_err());
        assert!(adapter.read_memory(PPC_SPACE, 0, 1).is_err());
    }

    #[test]
    fn memory_alias_identity_and_disassembly_limits_are_explicit() {
        let mut runner = runner();
        runner.bus_mut().set_addressing_32_bit(false);
        runner.bus_mut().write_word(0x20000, 0xf200);
        let memory = read_m68k_memory(&runner, 0xff020000, 2).unwrap();
        assert_eq!(memory.address.offset, 0xff020000);
        assert_eq!(memory.bytes, vec![0xf2, 0x00]);
        let lines = m68k_disassembly(&runner, 0x20000, 1).unwrap();
        assert!(lines[0].approximate);
        assert_eq!(
            lines[0].text,
            m68k::dasm::disassemble(0x20000, 0xf200, runner.cpu().core.cpu_type).0
        );
        assert!(m68k_disassembly(&runner, 0, MAX_DISASSEMBLY_COUNT + 1).is_err());
        assert!(read_m68k_memory(&runner, 0, MAX_MEMORY_TRANSFER + 1).is_err());
    }

    #[test]
    fn capabilities_report_68k_limits() {
        let mut runner = runner();
        let reply = handle_debug_request(
            &mut runner,
            DebugRequest::GetCapabilities {
                target: CapabilityTarget::Context { id: M68K_CONTEXT },
            },
        )
        .unwrap();
        let DebugReply::Capabilities(CapabilitySet::Context(capabilities)) = reply else {
            panic!("expected context capabilities");
        };
        assert!(capabilities.read_registers);
        assert!(capabilities.pause);
        assert!(capabilities.step);
        assert!(!capabilities.exact_step);
        assert!(capabilities.set_breakpoint);
        assert!(capabilities.precise_breakpoints);
        assert!(!capabilities.write_registers);
    }

    fn capture_request(
        mode: super::super::request::CaptureMode,
        boundary: Option<super::super::model::CaptureBoundaryKind>,
    ) -> CaptureRequest {
        CaptureRequest {
            mode,
            boundary,
            providers: Vec::new(),
            include_artifacts: true,
        }
    }

    #[test]
    fn graphics_provider_reports_surfaces_and_immediate_capture_is_retained() {
        use super::super::model::{CaptureBoundaryKind, OperationOutcome, OperationState};
        use super::super::provider::provider_ids;
        let mut runner = runner();
        runner.bus_mut().write_byte(0, 0xAB);

        let reply = handle_debug_request(
            &mut runner,
            DebugRequest::ReadProvider {
                id: ProviderId(provider_ids::GRAPHICS),
            },
        )
        .unwrap();
        let DebugReply::ProviderSnapshot(super::super::provider::ProviderSnapshot::Graphics {
            state,
            ..
        }) = reply
        else {
            panic!("expected a graphics snapshot");
        };
        assert_eq!(state.surfaces.len(), 1);
        assert_eq!(state.outputs.len(), 1);
        assert!(state.surfaces[0].width > 0);
        assert_eq!(state.outputs[0].surfaces, vec![state.surfaces[0].id]);
        assert!(state.palette_argb.is_some());

        let accepted = handle_debug_request(
            &mut runner,
            DebugRequest::RequestCapture {
                request: capture_request(super::super::request::CaptureMode::CurrentState, None),
            },
        )
        .unwrap();
        let DebugReply::Accepted {
            operation_id: Some(operation),
            ..
        } = accepted
        else {
            panic!("expected an accepted capture");
        };
        let status =
            handle_debug_request(&mut runner, DebugRequest::GetOperation { id: operation })
                .unwrap();
        assert!(matches!(
            status,
            DebugReply::Operation(super::super::model::OperationStatus {
                state: OperationState::Completed {
                    result: OperationOutcome::Captured { .. }
                },
                ..
            })
        ));

        let list = handle_debug_request(&mut runner, DebugRequest::ListCaptures).unwrap();
        let DebugReply::Captures(captures) = list else {
            panic!("expected captures");
        };
        assert_eq!(captures.len(), 1);
        assert_eq!(captures[0].boundary, CaptureBoundaryKind::CommandSafePoint);
        assert_eq!(captures[0].artifacts.len(), 1);
        let artifact = captures[0].artifacts[0].clone();
        assert_eq!(artifact.format.as_deref(), Some("rgba8"));
        assert!(artifact.provenance.starts_with("guest-framebuffer:"));

        let data = handle_debug_request(&mut runner, DebugRequest::GetArtifact { id: artifact.id })
            .unwrap();
        let DebugReply::ArtifactData { descriptor, bytes } = data else {
            panic!("expected artifact data");
        };
        assert_eq!(descriptor.id, artifact.id);
        assert_eq!(bytes.len() as u64, descriptor.byte_len);

        let snapshot = handle_debug_request(
            &mut runner,
            DebugRequest::ReadCaptureProvider {
                capture: captures[0].id,
                provider: ProviderId(provider_ids::GRAPHICS),
            },
        )
        .unwrap();
        assert!(matches!(
            snapshot,
            DebugReply::ProviderSnapshot(super::super::provider::ProviderSnapshot::Graphics { .. })
        ));
    }

    #[test]
    fn capture_only_requests_artifacts_from_selected_providers() {
        use super::super::provider::provider_ids;
        let mut runner = runner();
        let request = CaptureRequest {
            mode: super::super::request::CaptureMode::CurrentState,
            boundary: None,
            providers: vec![ProviderId(provider_ids::EVENTS)],
            include_artifacts: true,
        };
        handle_debug_request(&mut runner, DebugRequest::RequestCapture { request }).unwrap();
        let DebugReply::Captures(captures) =
            handle_debug_request(&mut runner, DebugRequest::ListCaptures).unwrap()
        else {
            panic!("expected captures");
        };
        assert_eq!(
            captures[0].providers,
            vec![ProviderId(provider_ids::EVENTS)]
        );
        assert!(captures[0].artifacts.is_empty());
    }

    #[test]
    fn capture_completes_at_the_logical_frame_boundary() {
        use super::super::model::{CaptureBoundaryKind, OperationOutcome, OperationState};
        let mut runner = runner();
        runner.bus_mut().write_word(0x20000, 0x4e71);
        runner.cpu_mut().write_reg(Register::PC, 0x20000);

        let accepted = handle_debug_request(
            &mut runner,
            DebugRequest::RequestCapture {
                request: capture_request(
                    super::super::request::CaptureMode::NextBoundary,
                    Some(CaptureBoundaryKind::LogicalFrameComplete),
                ),
            },
        )
        .unwrap();
        let DebugReply::Accepted {
            operation_id: Some(operation),
            ..
        } = accepted
        else {
            panic!("expected an accepted capture");
        };
        let list = handle_debug_request(&mut runner, DebugRequest::ListCaptures).unwrap();
        assert!(matches!(list, DebugReply::Captures(captures) if captures.is_empty()));
        let status =
            handle_debug_request(&mut runner, DebugRequest::GetOperation { id: operation })
                .unwrap();
        assert!(matches!(
            status,
            DebugReply::Operation(super::super::model::OperationStatus {
                state: OperationState::Pending,
                ..
            })
        ));

        runner.run_steps(1, None);

        let list = handle_debug_request(&mut runner, DebugRequest::ListCaptures).unwrap();
        let DebugReply::Captures(captures) = list else {
            panic!("expected captures");
        };
        assert_eq!(captures.len(), 1);
        assert_eq!(
            captures[0].boundary,
            CaptureBoundaryKind::LogicalFrameComplete
        );
        let status =
            handle_debug_request(&mut runner, DebugRequest::GetOperation { id: operation })
                .unwrap();
        assert!(matches!(
            status,
            DebugReply::Operation(super::super::model::OperationStatus {
                state: OperationState::Completed {
                    result: OperationOutcome::Captured { .. }
                },
                ..
            })
        ));
    }

    #[test]
    fn pause_at_boundary_captures_then_halts_execution() {
        use super::super::model::{CaptureBoundaryKind, StopReason};
        let mut runner = runner();
        runner.bus_mut().write_word(0x20000, 0x4e71);
        runner.bus_mut().write_word(0x20002, 0x4e71);
        runner.cpu_mut().write_reg(Register::PC, 0x20000);
        handle_debug_request(
            &mut runner,
            DebugRequest::RequestCapture {
                request: capture_request(
                    super::super::request::CaptureMode::PauseAtBoundary,
                    Some(CaptureBoundaryKind::LogicalFrameComplete),
                ),
            },
        )
        .unwrap();

        runner.run_steps(4, None);
        assert!(runner.debug.is_paused());
        let stop_id = match runner.debug.execution_state() {
            DebugExecutionState::Paused { stop } => {
                assert!(matches!(stop.reason, StopReason::CaptureBoundary { .. }));
                stop.id
            }
            other => panic!("expected a pause, got {other:?}"),
        };
        let list = handle_debug_request(&mut runner, DebugRequest::ListCaptures).unwrap();
        let DebugReply::Captures(captures) = list else {
            panic!("expected captures");
        };
        assert_eq!(captures[0].stop, Some(stop_id));
        let pc = runner.cpu().core.pc;
        let (steps, _) = runner.run_steps(4, None);
        assert_eq!(steps, 0);
        assert_eq!(runner.cpu().core.pc, pc);
    }

    #[test]
    fn captured_artifact_is_immutable_after_resume() {
        let mut runner = runner();
        runner.bus_mut().write_byte(0, 0x11);
        handle_debug_request(
            &mut runner,
            DebugRequest::RequestCapture {
                request: capture_request(super::super::request::CaptureMode::CurrentState, None),
            },
        )
        .unwrap();
        let list = handle_debug_request(&mut runner, DebugRequest::ListCaptures).unwrap();
        let DebugReply::Captures(captures) = list else {
            panic!("expected captures");
        };
        let artifact = captures[0].artifacts[0].id;
        let before =
            handle_debug_request(&mut runner, DebugRequest::GetArtifact { id: artifact }).unwrap();

        runner.bus_mut().write_byte(0, 0x99);
        let after =
            handle_debug_request(&mut runner, DebugRequest::GetArtifact { id: artifact }).unwrap();
        assert_eq!(before, after);
    }

    #[test]
    fn pending_capture_can_be_cancelled() {
        use super::super::model::{CaptureBoundaryKind, OperationOutcome, OperationState};
        let mut runner = runner();
        let accepted = handle_debug_request(
            &mut runner,
            DebugRequest::RequestCapture {
                request: capture_request(
                    super::super::request::CaptureMode::NextBoundary,
                    Some(CaptureBoundaryKind::LogicalFrameComplete),
                ),
            },
        )
        .unwrap();
        let DebugReply::Accepted {
            operation_id: Some(operation),
            ..
        } = accepted
        else {
            panic!("expected an accepted capture");
        };
        handle_debug_request(&mut runner, DebugRequest::CancelCapture { operation }).unwrap();
        let status =
            handle_debug_request(&mut runner, DebugRequest::GetOperation { id: operation })
                .unwrap();
        assert!(matches!(
            status,
            DebugReply::Operation(super::super::model::OperationStatus {
                state: OperationState::Completed {
                    result: OperationOutcome::Cancelled
                },
                ..
            })
        ));
        runner.run_steps(1, None);
        let list = handle_debug_request(&mut runner, DebugRequest::ListCaptures).unwrap();
        assert!(matches!(list, DebugReply::Captures(captures) if captures.is_empty()));
    }

    #[test]
    fn boundary_capture_rejects_a_pending_step() {
        use super::super::model::CaptureBoundaryKind;
        let mut runner = runner();
        handle_debug_request(&mut runner, DebugRequest::Pause).unwrap();
        handle_debug_request(
            &mut runner,
            DebugRequest::Step {
                context: ContextSelector::Active,
            },
        )
        .unwrap();
        let error = handle_debug_request(
            &mut runner,
            DebugRequest::RequestCapture {
                request: capture_request(
                    super::super::request::CaptureMode::PauseAtBoundary,
                    Some(CaptureBoundaryKind::CommandSafePoint),
                ),
            },
        )
        .unwrap_err();
        assert!(matches!(error, DebugError::InvalidState { .. }));
        assert!(runner.debug.is_stepping());
    }

    #[test]
    fn captures_survive_frontend_runner_replacement() {
        let mut original = runner();
        handle_debug_request(
            &mut original,
            DebugRequest::RequestCapture {
                request: capture_request(super::super::request::CaptureMode::CurrentState, None),
            },
        )
        .unwrap();
        let list = handle_debug_request(&mut original, DebugRequest::ListCaptures).unwrap();
        let DebugReply::Captures(captures) = list else {
            panic!("expected captures");
        };
        let capture_id = captures[0].id;
        let session = captures[0].session;

        let store = original.debug_extract_capture_store();
        assert!(matches!(
            handle_debug_request(&mut original, DebugRequest::ListCaptures),
            Ok(DebugReply::Captures(captures)) if captures.is_empty()
        ));

        let mut replacement = runner();
        replacement.debug_adopt_capture_store(store);
        let reply = handle_debug_request(
            &mut replacement,
            DebugRequest::GetCapture { id: capture_id },
        )
        .unwrap();
        let DebugReply::Capture(manifest) = reply else {
            panic!("expected a capture");
        };
        assert_eq!(manifest.session, session);
        assert_eq!(manifest.id, capture_id);
    }

    #[test]
    fn unsupported_capture_boundaries_are_rejected() {
        use super::super::model::CaptureBoundaryKind;
        let mut runner = runner();
        assert!(matches!(
            handle_debug_request(
                &mut runner,
                DebugRequest::RequestCapture {
                    request: capture_request(
                        super::super::request::CaptureMode::NextBoundary,
                        Some(CaptureBoundaryKind::BackendReadbackReady),
                    ),
                }
            ),
            Err(DebugError::BoundaryUnavailable { .. })
        ));
        let supported = handle_debug_request(
            &mut runner,
            DebugRequest::GetCapabilities {
                target: CapabilityTarget::Boundary {
                    boundary: CaptureBoundaryKind::LogicalFrameComplete,
                },
            },
        )
        .unwrap();
        let DebugReply::Capabilities(CapabilitySet::Boundary(capabilities)) = supported else {
            panic!("expected boundary capabilities");
        };
        assert!(capabilities.atomic_across_providers);
        assert!(capabilities
            .providers
            .contains(&ProviderId(super::super::provider::provider_ids::GRAPHICS)));
    }

    #[test]
    fn terminal_halt_fails_a_pending_capture() {
        use super::super::model::{CaptureBoundaryKind, OperationOutcome, OperationState};
        let mut runner = runner();
        runner.bus_mut().write_word(0x20000, 0xa9f4); // _ExitToShell
        runner.cpu_mut().write_reg(Register::PC, 0x20000);
        let accepted = handle_debug_request(
            &mut runner,
            DebugRequest::RequestCapture {
                request: capture_request(
                    super::super::request::CaptureMode::NextBoundary,
                    Some(CaptureBoundaryKind::LogicalFrameComplete),
                ),
            },
        )
        .unwrap();
        let DebugReply::Accepted {
            operation_id: Some(operation),
            ..
        } = accepted
        else {
            panic!("expected an accepted capture");
        };
        runner.run_steps(1, None);
        assert!(runner.is_halted());
        let status =
            handle_debug_request(&mut runner, DebugRequest::GetOperation { id: operation })
                .unwrap();
        assert!(matches!(
            status,
            DebugReply::Operation(super::super::model::OperationStatus {
                state: OperationState::Completed {
                    result: OperationOutcome::Unavailable { .. }
                },
                ..
            })
        ));
    }
