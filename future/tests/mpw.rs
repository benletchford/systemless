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
    assert_eq!(releases.len(), 3);
    assert!(allocate < releases[0].0 && releases[0].0 < dispose);
    assert!(dispose < releases[1].0 && releases[1].0 < releases[2].0);
    assert_eq!(
        calls
            .iter()
            .filter(|c| matches!(c, Call::GetResource {kind, id: 128} if kind == b"DATA"))
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
        assert_eq!(app.runtime.advance(10_000).unwrap(), Stop::Exit(0));
        let calls = &app.runtime.services.calls;
        assert_eq!(
            calls
                .iter()
                .filter(|c| matches!(c, Call::WaitEvent { .. }))
                .count(),
            3
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
        LoadedApplication::pef(&pef, None).unwrap(),
    ] {
        app.runtime.graphics = Some(
            pollster::block_on(Graphics::new(&instance, None, 512, 342, GuestPtr(0x4000))).unwrap(),
        );
        assert_eq!(app.runtime.advance(20_000).unwrap(), Stop::Exit(0));
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
                .filter(|c| matches!(c, Call::NewWindow(_)))
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
#[ignore = "requires generated MPW fixture"]
fn compiled_textedit_records_and_style_lifetime_complete_on_both_cpus() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("target/mpw-probe");
    let code = std::fs::read(root.join("textedit.68k.rsrc")).expect("build MPW fixtures first");
    let pef = std::fs::read(root.join("textedit.ppc")).unwrap();
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
                .filter(|call| matches!(
                    call,
                    Call::TextEdit(systemless_future::textedit::Op::New { .. })
                ))
                .count(),
            3
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
