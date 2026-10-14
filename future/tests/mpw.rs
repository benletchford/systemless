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
    // DisposeHandle is reached only after all C resource/memory checks pass.
}

#[test]
#[ignore = "requires generated MPW compiler fixture; see fixtures/mpw/README.md"]
fn compiled_ppc_application_completes_resource_and_memory_checks() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("target/mpw-probe");
    let bytes = std::fs::read(root.join("probe.ppc")).expect("build the MPW probe first");
    let resources = std::fs::read(root.join("probe.ppc.rsrc")).unwrap();
    let mut app = LoadedApplication::pef(&bytes, Some(&resources)).unwrap();
    assert_eq!(app.runtime.advance(10_000).unwrap(), Stop::Exit(0));
    assert_resource_path(&app.runtime.services.calls);
    assert!(matches!(
        app.runtime.services.calls.first(),
        Some(Call::SetJump(_))
    ));
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
