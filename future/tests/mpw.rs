//! Explicit acceptance for the real MPW compiler/startup libraries. Generate the
//! ignored artifacts with `sh future/fixtures/mpw/build.sh` before running.
use systemless_future::{cpu::Call, load::LoadedApplication, runtime::Stop};
#[test]
#[ignore = "requires generated MPW compiler fixture; see fixtures/mpw/README.md"]
fn compiled_68k_application_completes_resource_and_memory_checks() {
    let path =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("target/mpw-probe/probe.68k.rsrc");
    let bytes = std::fs::read(path).expect("build the MPW probe first");
    let mut app = LoadedApplication::code(&bytes).unwrap();
    assert_eq!(app.runtime.advance(10_000).unwrap(), Stop::Exit(0));
    assert_resource_path(&app.runtime.services.calls);
    let before = app.runtime.instructions;
    assert_eq!(app.runtime.advance(100).unwrap(), Stop::Exit(0));
    assert_eq!(app.runtime.instructions, before);
}

fn assert_resource_path(calls: &[Call]) {
    let resource = calls
        .iter()
        .position(|c| matches!(c,Call::GetResource{kind,id:128} if kind==b"DATA"))
        .unwrap();
    let size = calls
        .iter()
        .position(|c| matches!(c, Call::HandleSize(_)))
        .unwrap();
    let allocate = calls
        .iter()
        .position(|c| {
            matches!(
                c,
                Call::NewHandle {
                    size: 12,
                    clear: true
                }
            )
        })
        .unwrap();
    let dispose = calls
        .iter()
        .position(|c| matches!(c, Call::DisposeHandle(_)))
        .unwrap();
    assert!(resource < size && size < allocate && allocate < dispose);
    assert_eq!(calls.last(), Some(&Call::Exit(0)));
    let releases: Vec<_> = calls
        .iter()
        .enumerate()
        .filter(|(_, c)| matches!(c, Call::ReleaseResource(_)))
        .collect();
    assert_eq!(releases.len(), 5);
    assert!(allocate < releases[0].0 && releases[0].0 < dispose);
    assert!(dispose < releases[1].0 && releases[1].0 < releases[2].0);
    assert_eq!(
        calls
            .iter()
            .filter(|c| matches!(c, Call::GetResource {kind, id: 128} if kind == b"DATA"))
            .count(),
        4
    );
    assert_eq!(
        calls
            .iter()
            .filter(|c| matches!(c, Call::DetachResource(_)))
            .count(),
        2
    );
    let lock_states: Vec<_> = calls
        .iter()
        .filter_map(|c| match c {
            Call::SetHandleLock { locked, .. } => Some(*locked),
            _ => None,
        })
        .collect();
    assert_eq!(lock_states, [true, true, false]);
    // Final release is reached only after reload and lock/resize checks succeed.
}

#[test]
#[ignore = "requires generated MPW compiler fixture; see fixtures/mpw/README.md"]
fn compiled_ppc_application_completes_resource_and_memory_checks() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("target/mpw-probe");
    let bytes = std::fs::read(root.join("probe.ppc")).expect("build the MPW probe first");
    let resources = std::fs::read(root.join("probe.ppc.rsrc")).unwrap();
    let mut app = LoadedApplication::pef(&bytes, Some(&resources)).unwrap();
    let exported_main = app
        .info
        .exports
        .iter()
        .find(|e| e.name == b"main")
        .expect("PPCLink must export main for export-table acceptance");
    assert_eq!(exported_main.class, 2);
    let entry = app.runtime.memory.u32(exported_main.address).unwrap();
    app.runtime
        .memory
        .fetch(systemless_future::memory::GuestPtr(entry), 4, 4)
        .unwrap();
    assert_eq!(app.runtime.advance(10_000).unwrap(), Stop::Exit(0));
    assert_resource_path(&app.runtime.services.calls);
    assert!(matches!(
        app.runtime.services.calls.first(),
        Some(Call::FragmentQuery(systemless_future::cfm::Query::Count {
            connection: 1,
            ..
        }))
    ));
    assert!(app
        .runtime
        .services
        .calls
        .iter()
        .any(|call| matches!(call, Call::SetJump(_))));
}

#[test]
#[ignore = "requires generated MPW compiler fixture; see fixtures/mpw/README.md"]
fn compiled_ppc_nonlocal_jumps_and_main_return_complete() {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("target/mpw-probe/jump.ppc");
    let bytes = std::fs::read(path).expect("build the MPW probe first");
    let mut app = LoadedApplication::pef(&bytes, None).unwrap();
    assert_eq!(app.runtime.advance(10_000).unwrap(), Stop::Exit(37));
    let calls = &app.runtime.services.calls;
    assert_eq!(
        calls
            .iter()
            .filter(|c| matches!(c, Call::SetJump(_)))
            .count(),
        3
    );
    let values: Vec<_> = calls
        .iter()
        .filter_map(|c| match c {
            Call::LongJump { value, .. } => Some(*value),
            _ => None,
        })
        .collect();
    assert_eq!(values, [7, 0]);
    assert_eq!(calls.last(), Some(&Call::CExit(37)));
    let before = app.runtime.instructions;
    assert_eq!(app.runtime.advance(100).unwrap(), Stop::Exit(37));
    assert_eq!(app.runtime.instructions, before);
}

#[test]
#[ignore = "requires generated MPW compiler fixture; see fixtures/mpw/README.md"]
fn compiled_event_program_passes_on_both_architectures() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("target/mpw-probe");
    let code = std::fs::read(root.join("events.68k.rsrc")).expect("build the MPW probe first");
    let pef = std::fs::read(root.join("events.ppc")).unwrap();
    for mut app in [
        LoadedApplication::code(&code).unwrap(),
        LoadedApplication::pef(&pef, None).unwrap(),
    ] {
        assert_eq!(app.runtime.advance(10_000).unwrap(), Stop::Exit(0));
        let calls = &app.runtime.services.calls;
        assert_eq!(
            &calls[calls.len() - 2..],
            &[
                Call::PostEvent {
                    what: 7,
                    message: 0x600d
                },
                Call::Exit(0)
            ]
        );
    }
}

#[test]
#[ignore = "requires generated MPW compiler fixture; see fixtures/mpw/README.md"]
fn compiled_wait_program_yields_times_out_and_wakes_on_input_on_both_cpus() {
    use systemless_future::{events, memory::GuestPtr};
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("target/mpw-probe");
    let code = std::fs::read(root.join("wait.68k.rsrc")).expect("build the MPW probe first");
    let pef = std::fs::read(root.join("wait.ppc")).unwrap();
    for mut app in [
        LoadedApplication::code(&code).unwrap(),
        LoadedApplication::pef(&pef, None).unwrap(),
    ] {
        assert_eq!(
            app.runtime.advance(10_000).unwrap(),
            Stop::Waiting { ticks: 3 }
        );
        let before = app.runtime.instructions;
        assert_eq!(
            app.runtime.advance(10_000).unwrap(),
            Stop::Waiting { ticks: 3 }
        );
        assert_eq!(app.runtime.instructions, before);
        app.runtime.elapse_ticks(2).unwrap();
        assert_eq!(
            app.runtime.advance(10_000).unwrap(),
            Stop::Waiting { ticks: 1 }
        );
        app.runtime.elapse_ticks(1).unwrap();
        assert_eq!(
            app.runtime.advance(10_000).unwrap(),
            Stop::Waiting { ticks: 100 }
        );
        let ticks = app.runtime.memory.u32(GuestPtr(0x16a)).unwrap();
        events::post(
            &mut app.runtime.memory,
            3,
            42,
            ticks,
            app.runtime.process.input,
        )
        .unwrap();
        assert_eq!(
            app.runtime.advance(10_000).unwrap(),
            Stop::Waiting { ticks: 50 }
        );
        let before = app.runtime.instructions;
        assert_eq!(
            app.runtime.advance(10_000).unwrap(),
            Stop::Waiting { ticks: 50 }
        );
        assert_eq!(app.runtime.instructions, before);
        systemless_future::host_input::Pointer::default()
            .motion(
                &mut app.runtime.process.input,
                &mut app.runtime.memory,
                (20.0, 20.0),
                (512, 342),
                (512, 342),
            )
            .unwrap();
        assert_eq!(app.runtime.advance(10_000).unwrap(), Stop::Exit(0));
        let calls = &app.runtime.services.calls;
        assert_eq!(
            calls
                .iter()
                .filter(|c| matches!(c, Call::WaitEvent { .. }))
                .count(),
            4
        );
        assert_eq!(
            &calls[calls.len() - 2..],
            &[
                Call::PostEvent {
                    what: 7,
                    message: 0x600d
                },
                Call::Exit(0)
            ]
        );
    }
}

#[test]
#[ignore = "requires MPW fixture and real wgpu adapter"]
fn compiled_resource_drawing_and_event_app_has_pixel_parity() {
    use systemless_future::{graphics::Graphics, memory::GuestPtr};
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("target/mpw-probe");
    let code = std::fs::read(root.join("draw.68k.rsrc")).expect("build MPW fixtures first");
    let pef = std::fs::read(root.join("draw.ppc")).unwrap();
    let resources = std::fs::read(root.join("draw.ppc.rsrc")).unwrap();
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
    let mut results = Vec::new();
    for mut app in [
        LoadedApplication::code(&code).unwrap(),
        LoadedApplication::pef(&pef, Some(&resources)).unwrap(),
    ] {
        let mut gpu =
            pollster::block_on(Graphics::new(&instance, None, 8, 4, GuestPtr(0x75000000))).unwrap();
        gpu.palette[0] = [255, 255, 255, 255];
        gpu.palette[1] = [0, 0, 0, 255];
        app.runtime.graphics = Some(gpu);
        assert_eq!(app.runtime.advance(10_000).unwrap(), Stop::Exit(0));
        let calls = &app.runtime.services.calls;
        assert_eq!(
            calls
                .iter()
                .filter(|call| matches!(call, Call::CopyMask(_)))
                .count(),
            1
        );
        assert_eq!(
            &calls[calls.len() - 2..],
            &[
                Call::PostEvent {
                    what: 7,
                    message: 0x600d
                },
                Call::Exit(0)
            ]
        );
        assert_eq!(
            calls
                .iter()
                .filter(|call| matches!(
                    call,
                    Call::Port {
                        op: systemless_future::quickdraw::PortOp::Close,
                        ..
                    }
                ))
                .count(),
            3
        );
        assert_eq!(
            calls
                .iter()
                .filter_map(|call| match call {
                    Call::RoundRect { frame, .. } => Some(*frame),
                    _ => None,
                })
                .collect::<Vec<_>>(),
            [true, true, false, true]
        );
        results.push(app.runtime.graphics.as_ref().unwrap().read_rgba().unwrap());
        let mut command = std::process::Command::new(env!("CARGO_BIN_EXE_systemless-future"));
        match app.info.architecture {
            systemless_future::cpu::Architecture::M68k => {
                command.arg("--code").arg(root.join("draw.68k.rsrc"));
            }
            systemless_future::cpu::Architecture::PowerPc => {
                command
                    .arg("--pef")
                    .arg(root.join("draw.ppc"))
                    .arg("--resources")
                    .arg(root.join("draw.ppc.rsrc"));
            }
        }
        let output = command
            .args(["--graphics", "--budget", "10000"])
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let stdout = String::from_utf8(output.stdout).unwrap();
        assert!(
            stdout.contains(&format!(
                "Exit(0), instructions={}, service_calls={},",
                app.runtime.instructions,
                calls.len()
            )),
            "{stdout}"
        );
    }
    assert_eq!(results[0], results[1]);
    let expected: Vec<u8> = [0x55u8, 0xaa, 0x55, 0xaa]
        .iter()
        .flat_map(|row| {
            (0..8).flat_map(move |x| {
                if row & (0x80 >> x) != 0 {
                    [0, 0, 0, 255]
                } else {
                    [255; 4]
                }
            })
        })
        .collect();
    assert_eq!(results[0], expected);
}

#[test]
#[ignore = "requires generated MPW compiler fixture; see fixtures/mpw/README.md"]
fn compiled_resource_plugin_initializes_executes_and_terminates_in_one_runtime() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("target/mpw-probe");
    let bytes = std::fs::read(root.join("dynamic.ppc")).expect("build MPW fixtures first");
    let resources = std::fs::read(root.join("dynamic.ppc.rsrc")).unwrap();
    let mut app = LoadedApplication::pef(&bytes, Some(&resources)).unwrap();
    assert_eq!(app.runtime.advance(20_000).unwrap(), Stop::Exit(0));
    assert!(app.runtime.services.calls.iter().any(|call| matches!(
        call,
        Call::PostEvent {
            message: 0x600D,
            ..
        }
    )));
    assert!(app
        .runtime
        .services
        .calls
        .iter()
        .any(|call| matches!(call, Call::LoadFragment(_))));
    assert!(app
        .runtime
        .services
        .calls
        .iter()
        .any(|call| matches!(call, Call::CloseFragment(_))));
    assert!(app
        .runtime
        .memory
        .read(systemless_future::memory::GuestPtr(0x80000000), 1)
        .is_err());
}

#[test]
#[ignore = "requires MPW fixture and real wgpu adapter"]
fn compiled_plain_window_lifecycle_has_record_and_screen_parity() {
    use systemless_future::{graphics::Graphics, memory::GuestPtr};
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("target/mpw-probe");
    let code = std::fs::read(root.join("windows.68k.rsrc")).expect("build MPW fixtures first");
    let pef = std::fs::read(root.join("windows.ppc")).unwrap();
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
    let mut screens = Vec::new();
    for mut app in [
        LoadedApplication::code(&code).unwrap(),
        LoadedApplication::pef(&pef, Some(&code)).unwrap(),
    ] {
        app.runtime.graphics = Some(
            pollster::block_on(Graphics::new(&instance, None, 512, 342, GuestPtr(0x4000))).unwrap(),
        );
        assert_eq!(app.runtime.advance(20_000).unwrap(), Stop::Exit(0));
        assert_eq!(
            app.runtime
                .services
                .calls
                .iter()
                .filter(|c| matches!(c, Call::GetWMgrPort(_)))
                .count(),
            2
        );
        assert_eq!(
            app.runtime
                .services
                .calls
                .iter()
                .filter(|call| matches!(call, Call::StillDown))
                .count(),
            2
        );
        assert_eq!(
            app.runtime
                .services
                .calls
                .iter()
                .filter(|call| matches!(
                    call,
                    Call::Control(systemless_future::controls::Op::Find { .. })
                ))
                .count(),
            3
        );
        assert_eq!(
            app.runtime
                .services
                .calls
                .iter()
                .filter(|call| matches!(call, Call::FindWindow { .. }))
                .count(),
            3
        );
        assert_eq!(
            app.runtime
                .memory
                .u32(systemless_future::windows::LIST)
                .unwrap(),
            0
        );
        assert_eq!(
            app.runtime
                .services
                .calls
                .iter()
                .filter(|c| matches!(c, Call::Dialog(systemless_future::dialogs::Op::IsEvent(_))))
                .count(),
            4
        );
        assert_eq!(
            app.runtime
                .services
                .calls
                .iter()
                .filter(|c| matches!(c, Call::EmptyRect(_)))
                .count(),
            4
        );
        assert_eq!(
            app.runtime
                .services
                .calls
                .iter()
                .filter(|c| matches!(c, Call::NewWindow(_)))
                .count(),
            4
        );
        assert_eq!(
            app.runtime
                .services
                .calls
                .iter()
                .filter(|c| matches!(
                    c,
                    Call::Control(systemless_future::controls::Op::Geometry { .. })
                ))
                .count(),
            4
        );
        assert_eq!(
            &app.runtime.services.calls[app.runtime.services.calls.len() - 2..],
            &[
                Call::PostEvent {
                    what: 7,
                    message: 0x600d
                },
                Call::Exit(0)
            ]
        );
        screens.push(
            app.runtime
                .memory
                .read(systemless_future::quickdraw::SCREEN, 64 * 342)
                .unwrap()
                .to_vec(),
        );
    }
    assert_eq!(screens[0], screens[1]);
    for y in 0..342 {
        for x in 0..64 {
            assert_eq!(screens[0][y * 64 + x], if y % 2 == 0 { 0xaa } else { 0x55 });
        }
    }
}

#[test]
#[ignore = "requires generated MPW fixture and real wgpu adapter"]
fn compiled_textedit_records_and_style_lifetime_complete_on_both_cpus() {
    use systemless_future::{graphics::Graphics, memory::GuestPtr};
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("target/mpw-probe");
    let code = std::fs::read(root.join("textedit.68k.rsrc")).expect("build MPW fixtures first");
    let pef = std::fs::read(root.join("textedit.ppc")).unwrap();
    for mut app in [
        LoadedApplication::code(&code).unwrap(),
        LoadedApplication::pef(&pef, None).unwrap(),
    ] {
        app.runtime.graphics = Some(
            pollster::block_on(Graphics::new(&instance, None, 512, 342, GuestPtr(0x4000))).unwrap(),
        );
        assert_eq!(app.runtime.advance(20000).unwrap(), Stop::Exit(0));
        assert_eq!(
            app.runtime
                .services
                .calls
                .iter()
                .filter(|call| matches!(
                    call,
                    Call::TextEdit(systemless_future::textedit::Op::New { .. })
                ))
                .count(),
            3
        );
        assert_eq!(
            app.runtime
                .services
                .calls
                .iter()
                .filter(|call| matches!(
                    call,
                    Call::TextEdit(systemless_future::textedit::Op::TextBox(_))
                ))
                .count(),
            2
        );
        assert_eq!(
            app.runtime
                .services
                .calls
                .iter()
                .filter(|call| matches!(call, Call::GetMouse(_)))
                .count(),
            1
        );
        assert_eq!(
            app.runtime
                .services
                .calls
                .iter()
                .filter(|call| matches!(call, Call::PointInRect { .. }))
                .count(),
            3
        );
        assert_eq!(
            app.runtime
                .services
                .calls
                .iter()
                .filter(|c| matches!(
                    c,
                    Call::TextEdit(systemless_future::textedit::Op::Height { .. })
                ))
                .count(),
            4
        );
        assert_eq!(
            &app.runtime.services.calls[app.runtime.services.calls.len() - 2..],
            &[
                Call::PostEvent {
                    what: 7,
                    message: 0x600d
                },
                Call::Exit(0)
            ]
        );
    }
}

#[test]
#[ignore = "requires generated MPW menus fixture and real wgpu adapter"]
fn compiled_menu_initialization_colors_and_snapshot_lifetime_match_on_both_cpus() {
    use systemless_future::{graphics::Graphics, memory::GuestPtr, quickdraw::SCREEN};
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("target/mpw-probe");
    let code = std::fs::read(root.join("menus.68k.rsrc")).unwrap();
    let pef = std::fs::read(root.join("menus.ppc")).unwrap();
    let resources = std::fs::read(root.join("menus.ppc.rsrc")).unwrap();
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
    let mut outputs = Vec::new();
    for mut app in [
        LoadedApplication::code(&code).unwrap(),
        LoadedApplication::pef(&pef, Some(&resources)).unwrap(),
    ] {
        let mut gpu =
            pollster::block_on(Graphics::new(&instance, None, 512, 342, GuestPtr(0x4000))).unwrap();
        gpu.palette[0] = [255; 4];
        gpu.palette[1] = [0, 0, 0, 255];
        app.runtime.graphics = Some(gpu);
        assert_eq!(app.runtime.advance(20000).unwrap(), Stop::Exit(0));
        assert_eq!(
            app.runtime
                .services
                .calls
                .iter()
                .filter(|call| matches!(call, Call::Menu(systemless_future::menus::Op::Handle(_))))
                .count(),
            8
        );
        assert_eq!(
            app.runtime
                .services
                .calls
                .iter()
                .filter(|c| matches!(c, Call::Menu(systemless_future::menus::Op::Enable { .. })))
                .count(),
            6
        );
        assert_eq!(
            &app.runtime.services.calls[app.runtime.services.calls.len() - 2..],
            &[
                Call::PostEvent {
                    what: 7,
                    message: 0x600d
                },
                Call::Exit(0)
            ]
        );
        assert!(app.runtime.process.cursor.visible());
        app.runtime.refresh_screen().unwrap();
        let rgba = app.runtime.graphics.as_ref().unwrap().read_rgba().unwrap();
        for (x, black) in [true, false, false, true, true, false, false, false]
            .into_iter()
            .enumerate()
        {
            assert_eq!(
                &rgba[x * 4..x * 4 + 4],
                if black { &[0, 0, 0, 255] } else { &[255; 4] }
            );
        }
        assert_eq!(app.runtime.memory.read(SCREEN, 1).unwrap(), [0xf8]);
        outputs.push(app.runtime.memory.read(SCREEN, 64 * 342).unwrap().to_vec());
    }
    assert_eq!(outputs[0], outputs[1]);
}

#[test]
#[ignore = "requires generated MPW Dialog Manager fixture"]
fn compiled_dialog_startup_and_parameter_strings_complete_on_both_cpus() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("target/mpw-probe");
    let code = std::fs::read(root.join("dialogs.68k.rsrc")).unwrap();
    let pef = std::fs::read(root.join("dialogs.ppc")).unwrap();
    for mut app in [
        LoadedApplication::code(&code).unwrap(),
        LoadedApplication::pef(&pef, None).unwrap(),
    ] {
        assert_eq!(app.runtime.advance(20000).unwrap(), Stop::Exit(0));
        assert_eq!(
            &app.runtime.services.calls[app.runtime.services.calls.len() - 2..],
            &[
                Call::PostEvent {
                    what: 7,
                    message: 0x600d
                },
                Call::Exit(0)
            ]
        );
    }
}

#[test]
#[ignore = "requires generated MPW environment fixture"]
fn compiled_environment_and_maximum_heap_contracts_complete_on_both_cpus() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("target/mpw-probe");
    let code = std::fs::read(root.join("environment.68k.rsrc")).unwrap();
    let pef = std::fs::read(root.join("environment.ppc")).unwrap();
    for mut app in [
        LoadedApplication::code(&code).unwrap(),
        LoadedApplication::pef(&pef, None).unwrap(),
    ] {
        assert_eq!(app.runtime.advance(20000).unwrap(), Stop::Exit(0));
        assert_eq!(
            app.runtime
                .services
                .calls
                .iter()
                .filter(|call| matches!(call, Call::Gestalt(_)))
                .count(),
            3
        );
        assert_eq!(
            app.runtime
                .services
                .calls
                .iter()
                .filter(|call| matches!(call, Call::Decimal(_)))
                .count(),
            5
        );
        assert_eq!(
            app.runtime
                .services
                .calls
                .iter()
                .filter(|call| matches!(call, Call::SetApplLimit(_)))
                .count(),
            2
        );
        let io_calls: Vec<_> = app
            .runtime
            .services
            .calls
            .iter()
            .filter_map(|call| {
                if let Call::FileIo { op, .. } = call {
                    Some(*op)
                } else {
                    None
                }
            })
            .collect();
        for expected in [
            systemless_future::files::Op::Create { hierarchical: true },
            systemless_future::files::Op::Open {
                kind: systemless_future::files::Fork::Data,
                hierarchical: true,
            },
            systemless_future::files::Op::Open {
                kind: systemless_future::files::Fork::Resource,
                hierarchical: true,
            },
            systemless_future::files::Op::Read,
            systemless_future::files::Op::Write,
            systemless_future::files::Op::GetEof,
            systemless_future::files::Op::SetEof,
            systemless_future::files::Op::GetPosition,
            systemless_future::files::Op::SetPosition,
            systemless_future::files::Op::Flush,
            systemless_future::files::Op::SetInfo { hierarchical: true },
            systemless_future::files::Op::Close,
        ] {
            assert!(io_calls.contains(&expected), "missing {expected:?}");
        }

        assert_eq!(
            &app.runtime.services.calls[app.runtime.services.calls.len() - 2..],
            &[
                Call::PostEvent {
                    what: 7,
                    message: 0x600d
                },
                Call::Exit(0)
            ]
        );
    }
}

#[test]
#[ignore = "requires rebuilt MPW tracking applications and a real wgpu adapter"]
fn compiled_tracking_waits_for_real_host_release_and_completes_on_both_cpus() {
    use systemless_future::{
        controls::TrackControl, graphics::Graphics, host_input::Pointer, memory::GuestPtr,
    };
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("target/mpw-probe");
    let code = std::fs::read(root.join("tracking.68k.rsrc")).unwrap();
    let pef = std::fs::read(root.join("tracking.ppc")).unwrap();
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
    for mut app in [
        LoadedApplication::code(&code).unwrap(),
        LoadedApplication::pef(&pef, None).unwrap(),
    ] {
        app.runtime.graphics = Some(
            pollster::block_on(Graphics::new(&instance, None, 512, 342, GuestPtr(0x4000))).unwrap(),
        );
        let mut pointer = Pointer::default();
        pointer
            .motion(
                &mut app.runtime.process.input,
                &mut app.runtime.memory,
                (70., 70.),
                (512, 342),
                (512, 342),
            )
            .unwrap();
        pointer
            .button(
                &mut app.runtime.memory,
                &mut app.runtime.process.input,
                true,
            )
            .unwrap();
        assert_eq!(
            app.runtime.advance(30000).unwrap(),
            Stop::Waiting { ticks: 1 }
        );
        assert!(app.runtime.services.calls.contains(&Call::PostEvent {
            what: 7,
            message: 0xf1
        }));
        let handle = app
            .runtime
            .services
            .calls
            .iter()
            .find_map(|call| {
                if let Call::TrackControl(TrackControl {
                    handle,
                    point: [20, 20],
                    action: 0,
                }) = call
                {
                    Some(*handle)
                } else {
                    None
                }
            })
            .unwrap();
        let record = app.runtime.memory.u32(GuestPtr(handle)).unwrap();
        assert_eq!(
            app.runtime.memory.read(GuestPtr(record + 17), 1).unwrap(),
            [10]
        );
        let instructions = app.runtime.instructions;
        assert_eq!(
            app.runtime.advance(30000).unwrap(),
            Stop::Waiting { ticks: 1 }
        );
        assert_eq!(app.runtime.instructions, instructions);
        pointer
            .motion(
                &mut app.runtime.process.input,
                &mut app.runtime.memory,
                (300., 250.),
                (512, 342),
                (512, 342),
            )
            .unwrap();
        assert_eq!(
            app.runtime.advance(30000).unwrap(),
            Stop::Waiting { ticks: 1 }
        );
        assert_eq!(
            app.runtime.memory.read(GuestPtr(record + 17), 1).unwrap(),
            [0]
        );
        pointer
            .motion(
                &mut app.runtime.process.input,
                &mut app.runtime.memory,
                (70., 70.),
                (512, 342),
                (512, 342),
            )
            .unwrap();
        assert_eq!(
            app.runtime.advance(30000).unwrap(),
            Stop::Waiting { ticks: 1 }
        );
        assert_eq!(
            app.runtime.memory.read(GuestPtr(record + 17), 1).unwrap(),
            [10]
        );
        pointer
            .button(
                &mut app.runtime.memory,
                &mut app.runtime.process.input,
                false,
            )
            .unwrap();
        assert_eq!(app.runtime.advance(30000).unwrap(), Stop::Exit(0));
        assert_eq!(
            &app.runtime.services.calls[app.runtime.services.calls.len() - 2..],
            &[
                Call::PostEvent {
                    what: 7,
                    message: 0x600d
                },
                Call::Exit(0)
            ]
        );
        assert_eq!(
            app.runtime
                .services
                .calls
                .iter()
                .filter(|call| matches!(call, Call::TrackControl(_)))
                .count(),
            1
        );
    }
}

#[test]
#[ignore = "requires generated MPW menu tracking fixture and real wgpu adapter"]
fn compiled_menu_tracking_waits_for_host_motion_and_returns_selection_on_both_cpus() {
    use systemless_future::{graphics::Graphics, host_input::Pointer, memory::GuestPtr};
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("target/mpw-probe");
    let code = std::fs::read(root.join("menu_tracking.68k.rsrc")).unwrap();
    let pef = std::fs::read(root.join("menu_tracking.ppc")).unwrap();
    let resources = std::fs::read(root.join("menu_tracking.ppc.rsrc")).unwrap();
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
    for mut a in [
        LoadedApplication::code(&code).unwrap(),
        LoadedApplication::pef(&pef, Some(&resources)).unwrap(),
    ] {
        a.runtime.graphics = Some(
            pollster::block_on(Graphics::new(&instance, None, 512, 342, GuestPtr(0x4000))).unwrap(),
        );
        let mut pointer = Pointer::default();
        pointer
            .motion(
                &mut a.runtime.process.input,
                &mut a.runtime.memory,
                (15., 10.),
                (512, 342),
                (512, 342),
            )
            .unwrap();
        pointer
            .button(&mut a.runtime.memory, &mut a.runtime.process.input, true)
            .unwrap();
        systemless_future::events::flush(&mut a.runtime.memory, 1 << 1, 0).unwrap();
        assert_eq!(
            a.runtime.advance(20000).unwrap(),
            Stop::Waiting { ticks: 1 }
        );
        let ins = a.runtime.instructions;
        assert_eq!(
            a.runtime.advance(20000).unwrap(),
            Stop::Waiting { ticks: 1 }
        );
        assert_eq!(a.runtime.instructions, ins);
        pointer
            .motion(
                &mut a.runtime.process.input,
                &mut a.runtime.memory,
                (25., 25.),
                (512, 342),
                (512, 342),
            )
            .unwrap();
        assert_eq!(a.runtime.advance(1).unwrap(), Stop::Waiting { ticks: 1 });
        pointer
            .button(&mut a.runtime.memory, &mut a.runtime.process.input, false)
            .unwrap();
        assert_eq!(a.runtime.advance(20000).unwrap(), Stop::Exit(0));
        assert_eq!(
            a.runtime
                .services
                .calls
                .iter()
                .filter(|c| matches!(c, Call::MenuSelect(_)))
                .count(),
            1
        );
        assert_eq!(
            a.runtime
                .services
                .calls
                .iter()
                .filter(|c| matches!(c, Call::MenuChoice))
                .count(),
            1
        );
        assert_eq!(
            a.runtime
                .services
                .calls
                .iter()
                .filter(|c| matches!(c, Call::HiliteMenu(0)))
                .count(),
            1
        );
        assert_eq!(
            &a.runtime.services.calls[a.runtime.services.calls.len() - 2..],
            &[
                Call::PostEvent {
                    what: 7,
                    message: 0x600d
                },
                Call::Exit(0)
            ]
        );
    }
}

#[test]
#[ignore = "requires generated MPW compiler fixture; see fixtures/mpw/README.md"]
fn compiled_working_directories_create_files_in_the_selected_folder_on_both_cpus() {
    use systemless_future::files::WorkingOp;
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("target/mpw-probe");
    let code = std::fs::read(root.join("working.68k.rsrc")).expect("build MPW fixtures first");
    let pef = std::fs::read(root.join("working.ppc")).unwrap();
    for mut app in [
        LoadedApplication::code(&code).unwrap(),
        LoadedApplication::pef(&pef, Some(&code)).unwrap(),
    ] {
        assert_eq!(app.runtime.advance(20_000).unwrap(), Stop::Exit(0));
        for (op, count) in [
            (WorkingOp::Open, 2),
            (WorkingOp::Info, 2),
            (WorkingOp::Close, 1),
        ] {
            assert_eq!(
                app.runtime
                    .services
                    .calls
                    .iter()
                    .filter(|c| matches!(c,Call::WorkingDirectory{op:got,..} if *got==op))
                    .count(),
                count
            );
        }
        assert_eq!(
            &app.runtime.services.calls[app.runtime.services.calls.len() - 2..],
            &[
                Call::PostEvent {
                    what: 7,
                    message: 0x600d
                },
                Call::Exit(0)
            ]
        );
    }
}

#[test]
#[ignore = "requires generated MPW compiler fixture and a real wgpu adapter"]
fn compiled_standard_file_dialogs_suspend_both_cpus_and_return_save_or_cancel_to_real_file_io() {
    use systemless_future::{events, files::Fork, graphics::Graphics, memory::GuestPtr};
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("target/mpw-probe");
    let code =
        std::fs::read(root.join("standard_file.68k.rsrc")).expect("build MPW fixtures first");
    let pef = std::fs::read(root.join("standard_file.ppc")).unwrap();
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
    for cancel in [false, true] {
        for mut app in [
            LoadedApplication::code(&code).unwrap(),
            LoadedApplication::pef(&pef, Some(&code)).unwrap(),
        ] {
            app.runtime.graphics = Some(
                pollster::block_on(Graphics::new(&instance, None, 512, 342, GuestPtr(0x4000)))
                    .unwrap(),
            );
            assert_eq!(
                app.runtime.advance(20_000).unwrap(),
                Stop::Waiting { ticks: 1 }
            );
            let n = app.runtime.instructions;
            assert_eq!(
                app.runtime.advance(20_000).unwrap(),
                Stop::Waiting { ticks: 1 }
            );
            assert_eq!(app.runtime.instructions, n);
            let input = app.runtime.process.input;
            events::post(
                &mut app.runtime.memory,
                3,
                if cancel { 27 } else { 13 },
                0,
                input,
            )
            .unwrap();
            assert_eq!(app.runtime.advance(20_000).unwrap(), Stop::Exit(0));
            let marker = if cancel { 0x600c } else { 0x600d };
            assert!(app
                .runtime
                .services
                .calls
                .iter()
                .any(|c| matches!(c,Call::PostEvent{what:7,message} if *message==marker)));
            let view = app.runtime.process.files.directory_view(2).unwrap();
            let file = view.entries.iter().find(|e| e.name == b"Fixture");
            if cancel {
                assert!(file.is_none());
            } else {
                assert_eq!(
                    app.runtime
                        .process
                        .files
                        .catalog_fork(file.unwrap().id, Fork::Data)
                        .unwrap(),
                    b"ABC"
                );
            }
            assert_eq!(
                app.runtime
                    .memory
                    .u32(systemless_future::windows::LIST)
                    .unwrap(),
                0
            );
        }
    }
}

#[test]
#[ignore = "requires generated MPW compiler fixture and a real wgpu adapter"]
fn compiled_open_file_filters_both_cpus_and_reads_selected_file_or_cancels() {
    use systemless_future::{events, graphics::Graphics, memory::GuestPtr};
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("target/mpw-probe");
    let code = std::fs::read(root.join("open_file.68k.rsrc")).expect("build MPW fixtures first");
    let pef = std::fs::read(root.join("open_file.ppc")).unwrap();
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
    for cancel in [false, true] {
        let mut pixels = None;
        for mut app in [
            LoadedApplication::code(&code).unwrap(),
            LoadedApplication::pef(&pef, Some(&code)).unwrap(),
        ] {
            app.runtime.graphics = Some(
                pollster::block_on(Graphics::new(&instance, None, 512, 342, GuestPtr(0x4000)))
                    .unwrap(),
            );
            assert_eq!(
                app.runtime.advance(20_000).unwrap(),
                Stop::Waiting { ticks: 1 }
            );
            assert_eq!(
                app.runtime
                    .services
                    .calls
                    .iter()
                    .filter(|c| matches!(
                        c,
                        Call::PostEvent {
                            what: 7,
                            message: 0xf17e
                        }
                    ))
                    .count(),
                2
            );
            let snapshot = app.runtime.graphics.as_ref().unwrap().read_rgba().unwrap();
            if let Some(expected) = &pixels {
                assert_eq!(&snapshot, expected);
            } else {
                pixels = Some(snapshot);
            }
            let n = app.runtime.instructions;
            assert_eq!(
                app.runtime.advance(20_000).unwrap(),
                Stop::Waiting { ticks: 1 }
            );
            assert_eq!(app.runtime.instructions, n);
            let input = app.runtime.process.input;
            if !cancel {
                events::post(&mut app.runtime.memory, 3, 0x1f, 0, input).unwrap();
                assert_eq!(
                    app.runtime.advance(20_000).unwrap(),
                    Stop::Waiting { ticks: 1 }
                );
            }
            events::post(
                &mut app.runtime.memory,
                3,
                if cancel { 27 } else { 13 },
                0,
                input,
            )
            .unwrap();
            assert_eq!(app.runtime.advance(20_000).unwrap(), Stop::Exit(0));
            let marker = if cancel { 0x0bec } else { 0x0bed };
            assert!(app
                .runtime
                .services
                .calls
                .iter()
                .any(|c| matches!(c,Call::PostEvent{what:7,message} if *message==marker)));
            assert_eq!(
                app.runtime
                    .memory
                    .u32(systemless_future::windows::LIST)
                    .unwrap(),
                0
            );
        }
    }
}

#[test]
#[ignore = "requires generated MPW compiler fixture"]
fn compiled_sound_channels_preserve_caller_storage_and_release_owned_lifetimes_on_both_cpus() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("target/mpw-probe");
    let code =
        std::fs::read(root.join("sound_channels.68k.rsrc")).expect("build MPW fixtures first");
    let pef = std::fs::read(root.join("sound_channels.ppc")).unwrap();
    for mut app in [
        LoadedApplication::code(&code).unwrap(),
        LoadedApplication::pef(&pef, Some(&code)).unwrap(),
    ] {
        assert_eq!(app.runtime.advance(10_000).unwrap(), Stop::Exit(0));
        assert!(app.runtime.services.calls.iter().any(|c| matches!(
            c,
            Call::PostEvent {
                what: 7,
                message: 0x50cd
            }
        )));
        assert_eq!(
            app.runtime
                .services
                .calls
                .iter()
                .filter(|c| matches!(c, Call::SoundChannel(systemless_future::sound::Op::New(_))))
                .count(),
            2
        );
        assert_eq!(
            app.runtime
                .services
                .calls
                .iter()
                .filter(|c| matches!(
                    c,
                    Call::SoundChannel(systemless_future::sound::Op::Dispose { .. })
                ))
                .count(),
            4
        );
    }
}

#[test]
#[ignore = "requires generated MPW compiler fixture"]
fn compiled_devices_share_screen_records_flags_and_palette_on_both_cpus() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("target/mpw-probe");
    let code = std::fs::read(root.join("devices.68k.rsrc")).expect("build MPW fixtures first");
    let pef = std::fs::read(root.join("devices.ppc")).unwrap();
    for mut app in [
        LoadedApplication::code(&code).unwrap(),
        LoadedApplication::pef(&pef, Some(&code)).unwrap(),
    ] {
        assert_eq!(app.runtime.advance(20000).unwrap(), Stop::Exit(0));
        assert!(app.runtime.services.calls.iter().any(|call| matches!(
            call,
            Call::PostEvent {
                what: 7,
                message: 0xd311
            }
        )));
        assert_eq!(
            app.runtime
                .services
                .calls
                .iter()
                .filter(|call| matches!(
                    call,
                    Call::Device(systemless_future::devices::Op::Index { .. })
                ))
                .count(),
            4
        );
    }
}

#[test]
#[ignore = "requires generated MPW compiler fixture"]
fn compiled_color_ports_own_attached_records_but_borrow_device_tables_on_both_cpus() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("target/mpw-probe");
    let code = std::fs::read(root.join("color_ports.68k.rsrc")).expect("build MPW fixtures first");
    let pef = std::fs::read(root.join("color_ports.ppc")).unwrap();
    for mut app in [
        LoadedApplication::code(&code).unwrap(),
        LoadedApplication::pef(&pef, Some(&code)).unwrap(),
    ] {
        assert_eq!(app.runtime.advance(30000).unwrap(), Stop::Exit(0));
        assert!(app.runtime.services.calls.iter().any(|call| matches!(
            call,
            Call::PostEvent {
                what: 7,
                message: 0xc907
            }
        )));
        for (action, count) in [
            (systemless_future::color_ports::Action::Open, 2),
            (systemless_future::color_ports::Action::Init, 2),
            (systemless_future::color_ports::Action::Close, 2),
        ] {
            assert_eq!(
                app.runtime
                    .services
                    .calls
                    .iter()
                    .filter(
                        |call| matches!(call,Call::ColorPort{action:actual,..} if *actual==action)
                    )
                    .count(),
                count
            );
        }
    }
}

#[test]
#[ignore = "requires generated MPW compiler fixture and real wgpu adapter"]
fn compiled_color_windows_share_hidden_geometry_drawing_and_attached_disposal_on_both_cpus() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("target/mpw-probe");
    let code =
        std::fs::read(root.join("color_windows.68k.rsrc")).expect("build MPW fixtures first");
    let pef = std::fs::read(root.join("color_windows.ppc")).unwrap();
    for mut app in [
        LoadedApplication::code(&code).unwrap(),
        LoadedApplication::pef(&pef, Some(&code)).unwrap(),
    ] {
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
        app.runtime.graphics = Some(
            pollster::block_on(systemless_future::graphics::Graphics::new(
                &instance,
                None,
                512,
                342,
                systemless_future::quickdraw::SCREEN,
            ))
            .unwrap(),
        );
        assert_eq!(app.runtime.advance(50000).unwrap(), Stop::Exit(0));
        assert!(app.runtime.services.calls.iter().any(|call| matches!(
            call,
            Call::PostEvent {
                what: 7,
                message: 0xc945
            }
        )));
        assert_eq!(
            app.runtime
                .services
                .calls
                .iter()
                .filter(|call| matches!(call, Call::NewColorWindow(_)))
                .count(),
            2
        );
        assert_eq!(
            app.runtime
                .memory
                .u32(systemless_future::windows::LIST)
                .unwrap(),
            0
        );
    }
}

#[test]
#[ignore = "requires generated MPW compiler fixture"]
fn compiled_sampled_sound_is_rendered_from_real_commands_and_completes_on_both_cpus() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("target/mpw-probe");
    let code =
        std::fs::read(root.join("sampled_sound.68k.rsrc")).expect("build MPW fixtures first");
    let pef = std::fs::read(root.join("sampled_sound.ppc")).unwrap();
    for mut app in [
        LoadedApplication::code(&code).unwrap(),
        LoadedApplication::pef(&pef, Some(&code)).unwrap(),
    ] {
        assert_eq!(
            app.runtime.advance(50000).unwrap(),
            Stop::Waiting { ticks: 1 }
        );
        assert!(app.runtime.services.calls.iter().any(|c| matches!(
            c,
            Call::PostEvent {
                what: 7,
                message: 0xa804
            }
        )));
        assert_eq!(
            app.runtime.render_sound(4, 8000).unwrap(),
            [[-32768; 2], [0; 2], [32512; 2], [-16384; 2]]
        );
        app.runtime.elapse_ticks(1).unwrap();
        assert_eq!(app.runtime.advance(50000).unwrap(), Stop::Exit(0));
        assert!(!app.runtime.process.sound.playing());
        assert!(app.runtime.services.calls.iter().any(|c| matches!(
            c,
            Call::PostEvent {
                what: 7,
                message: 0xa805
            }
        )));
    }
}

#[test]
#[ignore = "requires generated MPW compiler fixture"]
fn headless_cli_advances_compiled_sampled_sound_during_guest_wait_without_a_host_device() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("target/mpw-probe");
    for (flag, path) in [
        ("--code", root.join("sampled_sound.68k.rsrc")),
        ("--pef", root.join("sampled_sound.ppc")),
    ] {
        let output = std::process::Command::new(env!("CARGO_BIN_EXE_systemless-future"))
            .arg(flag)
            .arg(path)
            .args(["--budget", "50000"])
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(
            String::from_utf8_lossy(&output.stdout).contains("Exit(0)"),
            "{}",
            String::from_utf8_lossy(&output.stdout)
        );
    }
}

#[test]
#[ignore = "requires generated MPW compiler fixture"]
fn compiled_handle_flags_control_real_blocks_and_resource_lifetimes_on_both_cpus() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("target/mpw-probe");
    let code = std::fs::read(root.join("handle_state.68k.rsrc")).expect("build MPW fixtures first");
    let pef = std::fs::read(root.join("handle_state.ppc")).unwrap();
    for mut app in [
        LoadedApplication::code(&code).unwrap(),
        LoadedApplication::pef(&pef, Some(&code)).unwrap(),
    ] {
        assert_eq!(app.runtime.advance(50000).unwrap(), Stop::Exit(0));
        assert!(app.runtime.services.calls.iter().any(|call| matches!(
            call,
            Call::PostEvent {
                what: 7,
                message: 0xa069
            }
        )));
        assert!(app
            .runtime
            .services
            .calls
            .iter()
            .any(|call| matches!(call, Call::HandleState { value: Some(_), .. })));
    }
}

#[test]
#[ignore = "requires generated MPW compiler fixture"]
fn compiled_queued_sound_runs_callbacks_on_both_cpus_and_restores_the_event_wait() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("target/mpw-probe");
    let code = std::fs::read(root.join("queued_sound.68k.rsrc")).expect("build MPW fixtures first");
    let pef = std::fs::read(root.join("queued_sound.ppc")).unwrap();
    for mut a in [
        LoadedApplication::code(&code).unwrap(),
        LoadedApplication::pef(&pef, None).unwrap(),
    ] {
        assert_eq!(
            a.runtime.advance(50000).unwrap(),
            Stop::Waiting { ticks: 1 }
        );
        let out = a.runtime.render_sound(9, 8000).unwrap();
        assert_eq!(
            out,
            [
                [-32768; 2],
                [0; 2],
                [32512; 2],
                [-16384; 2],
                [-16448; 2],
                [0; 2],
                [16319; 2],
                [-8224; 2],
                [0; 2]
            ]
        );
        let callback = |a: &LoadedApplication| {
            a.runtime
                .services
                .calls
                .iter()
                .filter(|call| {
                    matches!(
                        call,
                        Call::PostEvent {
                            what: 7,
                            message: 0xa803
                        }
                    )
                })
                .count()
        };
        assert_eq!(callback(&a), 0);
        assert_eq!(
            a.runtime.advance(50000).unwrap(),
            Stop::Waiting { ticks: 1 }
        );
        assert_eq!(callback(&a), 1);
        // A callback crossing an instruction-derived tick may already have
        // expired the first EventWait and entered the second phase.
        if !a.runtime.services.calls.iter().any(|call| {
            matches!(
                call,
                Call::PostEvent {
                    what: 7,
                    message: 0xa808
                }
            )
        }) {
            a.runtime.elapse_ticks(1).unwrap();
            assert_eq!(
                a.runtime.advance(50000).unwrap(),
                Stop::Waiting { ticks: 1 }
            );
        }
        let source = [[-32768; 2], [0; 2], [32512; 2], [-16384; 2]];
        assert_eq!(a.runtime.render_sound(4, 8000).unwrap(), source);
        assert_eq!(
            a.runtime.advance(50000).unwrap(),
            Stop::Waiting { ticks: 1 }
        );
        // recycle disposed the callback's channel, allocated another at the same
        // address, and started its voice. Returning the old callback must not
        // clear the replacement's cmdInProgress or playback phase.
        let created: Vec<_> = a
            .runtime
            .services
            .calls
            .iter()
            .filter_map(|call| match call {
                Call::SoundChannel(systemless_future::sound::Op::New(r)) => Some(r.destination),
                _ => None,
            })
            .collect();
        assert_eq!(created.len(), 3);
        let replacement = a.runtime.memory.u32(created[2]).unwrap();
        assert_eq!(a.runtime.memory.u32(created[1]).unwrap(), replacement);
        assert_eq!(a.runtime.process.sound.busy(replacement), Some(true));
        assert_eq!(a.runtime.render_sound(4, 8000).unwrap(), source);
        a.runtime.elapse_ticks(1).unwrap();
        assert_eq!(a.runtime.advance(50000).unwrap(), Stop::Exit(0));
        assert!(a.runtime.services.calls.iter().any(|call| matches!(
            call,
            Call::PostEvent {
                what: 7,
                message: 0xa806
            }
        )));
    }
}

#[test]
#[ignore = "requires generated MPW compiler fixture"]
fn headless_cli_runs_queued_sound_callbacks_without_a_host_audio_device() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("target/mpw-probe");
    for (flag, path) in [
        ("--code", root.join("queued_sound.68k.rsrc")),
        ("--pef", root.join("queued_sound.ppc")),
    ] {
        let output = std::process::Command::new(env!("CARGO_BIN_EXE_systemless-future"))
            .arg(flag)
            .arg(path)
            .args(["--budget", "50000"])
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(
            String::from_utf8_lossy(&output.stdout).contains("Exit(0)"),
            "{}",
            String::from_utf8_lossy(&output.stdout)
        );
    }
}

#[test]
#[ignore = "requires generated MPW compiler fixture"]
fn compiled_resource_sound_uses_real_sync_waits_scoped_completion_and_callback_deadlines() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("target/mpw-probe");
    let code =
        std::fs::read(root.join("resource_sound.68k.rsrc")).expect("build MPW fixtures first");
    let pef = std::fs::read(root.join("resource_sound.ppc")).unwrap();
    let marker = |a: &LoadedApplication, value| {
        a.runtime
            .services
            .calls
            .iter()
            .any(|call| matches!(call,Call::PostEvent{what:7,message} if *message==value))
    };
    for mut a in [
        LoadedApplication::code(&code).unwrap(),
        LoadedApplication::pef(&pef, Some(&code)).unwrap(),
    ] {
        assert_eq!(
            a.runtime.advance(50000).unwrap(),
            Stop::Waiting { ticks: 1 }
        );
        assert!(!marker(&a, 0xa810));
        let wave = [[-32768; 2], [0; 2], [32512; 2], [-16384; 2]];
        assert_eq!(a.runtime.render_sound(4, 8000).unwrap(), wave);
        assert_eq!(
            a.runtime.advance(50000).unwrap(),
            Stop::Waiting { ticks: 1 }
        );
        assert!(marker(&a, 0xa810));
        assert!(marker(&a, 0xa811));
        assert_eq!(
            a.runtime.render_sound(2, 8000).unwrap(),
            [[16384; 2], [24576; 2]]
        );
        let before = a.runtime.instructions;
        let mut stop = Stop::Budget;
        for _ in 0..4 {
            stop = a.runtime.advance(50000).unwrap();
            if stop != Stop::Budget {
                break;
            }
        }
        assert_eq!(stop, Stop::Waiting { ticks: 1 });
        assert!(a.runtime.instructions - before >= 1000);
        // No host elapse: the long guest callback's own virtual ticks must expire the
        // suspended EventWait and let main reach the third resource's sync wait.
        assert!(marker(&a, 0xa814));
        assert!(!marker(&a, 0xa812));
        assert_eq!(a.runtime.render_sound(4, 8000).unwrap(), wave);
        assert_eq!(
            a.runtime.advance(50000).unwrap(),
            Stop::Waiting { ticks: 1 }
        );
        assert!(marker(&a, 0xa812));
        assert!(a.runtime.process.sound.playing());
        // SndPlay returned although the callback's unrelated tail is still active.
        assert_eq!(
            a.runtime.render_sound(2, 8000).unwrap(),
            [[16384; 2], [24576; 2]]
        );
        a.runtime.elapse_ticks(1).unwrap();
        assert_eq!(a.runtime.advance(50000).unwrap(), Stop::Exit(0));
        assert!(marker(&a, 0xa813));
        assert_eq!(a.runtime.services.completed_sound_callbacks, 2);
        assert!(!a.runtime.process.sound.playing());
    }
}
#[test]
#[ignore = "requires generated MPW compiler fixture"]
fn headless_cli_finishes_resource_sound_and_callbacks_without_a_host_audio_device() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("target/mpw-probe");
    for (flag, path) in [
        ("--code", root.join("resource_sound.68k.rsrc")),
        ("--pef", root.join("resource_sound.ppc")),
    ] {
        let mut command = std::process::Command::new(env!("CARGO_BIN_EXE_systemless-future"));
        command.arg(flag).arg(path);
        if flag == "--pef" {
            command
                .arg("--resources")
                .arg(root.join("resource_sound.68k.rsrc"));
        }
        let output = command.args(["--budget", "250000"]).output().unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(
            String::from_utf8_lossy(&output.stdout).contains("Exit(0)"),
            "{}",
            String::from_utf8_lossy(&output.stdout)
        );
    }
}

#[test]
#[ignore = "requires generated MPW compiler fixture"]
fn compiled_sound_info_reports_live_status_and_plays_signed_stereo_and_extended_resources() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("target/mpw-probe");
    let code = std::fs::read(root.join("sound_info.68k.rsrc")).expect("build MPW fixtures first");
    let pef = std::fs::read(root.join("sound_info.ppc")).unwrap();
    for mut a in [
        LoadedApplication::code(&code).unwrap(),
        LoadedApplication::pef(&pef, Some(&code)).unwrap(),
    ] {
        assert_eq!(
            a.runtime.advance(50000).unwrap(),
            Stop::Waiting { ticks: 1 }
        );
        assert_eq!(
            a.runtime.render_sound(2, 8000).unwrap(),
            [[1000, 3000], [-2000, -4000]]
        );
        a.runtime.elapse_ticks(1).unwrap();
        assert_eq!(
            a.runtime.advance(50000).unwrap(),
            Stop::Waiting { ticks: 1 }
        );
        assert_eq!(a.runtime.services.completed_sound_callbacks, 1);
        assert_eq!(
            a.runtime.render_sound(2, 8000).unwrap(),
            [[2000; 2], [-3000; 2]]
        );
        assert_eq!(a.runtime.advance(50000).unwrap(), Stop::Exit(0));
        assert!(a.runtime.services.calls.iter().any(|call| matches!(
            call,
            Call::PostEvent {
                what: 7,
                message: 0xa823
            }
        )));
    }
}
#[test]
#[ignore = "requires generated MPW compiler fixture"]
fn headless_cli_finishes_sound_status_and_extended_samples_without_a_host_device() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("target/mpw-probe");
    for (flag, path) in [
        ("--code", root.join("sound_info.68k.rsrc")),
        ("--pef", root.join("sound_info.ppc")),
    ] {
        let mut command = std::process::Command::new(env!("CARGO_BIN_EXE_systemless-future"));
        command.arg(flag).arg(path);
        if flag == "--pef" {
            command
                .arg("--resources")
                .arg(root.join("sound_info.68k.rsrc"));
        }
        let output = command.args(["--budget", "100000"]).output().unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(
            String::from_utf8_lossy(&output.stdout).contains("Exit(0)"),
            "{}",
            String::from_utf8_lossy(&output.stdout)
        );
    }
}
