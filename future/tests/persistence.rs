mod support;
use std::{
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
};
use systemless_future::{
    cpu::Call, files::Fork, load::LoadedApplication, runtime::Stop, volume::Volume,
};
static NEXT: AtomicU64 = AtomicU64::new(0);
fn directory() -> PathBuf {
    let p = std::env::temp_dir().join(format!(
        "systemless-persistence-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    std::fs::create_dir(&p).unwrap();
    p
}
#[test]
#[ignore = "requires generated MPW compiler fixture"]
fn compiled_guests_preserve_both_forks_across_fresh_cpus_and_real_cli_processes() {
    let fixtures = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("target/mpw-probe");
    let code =
        std::fs::read(fixtures.join("persistence.68k.rsrc")).expect("build MPW fixtures first");
    let pef = std::fs::read(fixtures.join("persistence.ppc")).unwrap();
    let root = directory();
    let path = root.join("shared.slv");
    let mut original = None;
    for (index, mut app) in [
        LoadedApplication::code(&code).unwrap(),
        LoadedApplication::pef(&pef, Some(&code)).unwrap(),
        LoadedApplication::code(&code).unwrap(),
    ]
    .into_iter()
    .enumerate()
    {
        let mut volume = Volume::open(&path, &mut app.runtime.process.files).unwrap();
        assert_eq!(app.runtime.advance(100_000).unwrap(), Stop::Exit(0));
        let marker = if index == 0 { 0x5a01 } else { 0x5a02 };
        assert!(app
            .runtime
            .services
            .calls
            .iter()
            .any(|c| matches!(c,Call::PostEvent{what:7,message} if *message==marker)));
        volume.checkpoint(&app.runtime.process.files).unwrap();
        let view = app.runtime.process.files.directory_view(17).unwrap();
        let file = view.entries.iter().find(|e| e.name == b"Game").unwrap();
        assert_eq!(
            app.runtime
                .process
                .files
                .catalog_fork(file.id, Fork::Data)
                .unwrap(),
            b"PERSIST"
        );
        assert_eq!(
            app.runtime
                .process
                .files
                .catalog_fork(file.id, Fork::Resource)
                .unwrap(),
            b"RSRC"
        );
        let bytes = std::fs::read(&path).unwrap();
        if let Some(expected) = &original {
            assert_eq!(&bytes, expected);
        } else {
            original = Some(bytes);
        }
    }
    for (flag, file) in [
        ("--code", fixtures.join("persistence.68k.rsrc")),
        ("--pef", fixtures.join("persistence.ppc")),
    ] {
        let output = std::process::Command::new(env!("CARGO_BIN_EXE_systemless-future"))
            .arg(flag)
            .arg(file)
            .arg("--volume")
            .arg(&path)
            .arg("--budget")
            .arg("100000")
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
        assert_eq!(std::fs::read(&path).unwrap(), *original.as_ref().unwrap());
    }
    std::fs::remove_dir_all(root).unwrap();
}
#[test]
fn cli_rejects_bad_volume_and_checkpoints_a_valid_image_on_budget_exhaustion() {
    // No private binaries or GPU are needed for launch/error handling.
    let root = directory();
    let pef = root.join("loop.pef");
    let path = root.join("bad.slv");
    let f = support::pef_program(&[0x48000000], &[], 0);
    std::fs::write(&pef, f.bytes).unwrap();
    std::fs::write(&path, b"corrupt").unwrap();
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_systemless-future"))
        .arg("--pef")
        .arg(&pef)
        .arg("--volume")
        .arg(&path)
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert_eq!(std::fs::read(&path).unwrap(), b"corrupt");
    std::fs::remove_file(&path).unwrap();
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_systemless-future"))
        .arg("--pef")
        .arg(&pef)
        .arg("--volume")
        .arg(&path)
        .arg("--budget")
        .arg("5")
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("budget exhausted"));
    let mut app = LoadedApplication::pef(&std::fs::read(pef).unwrap(), None).unwrap();
    let _volume = Volume::open(&path, &mut app.runtime.process.files).unwrap();
    assert!(app
        .runtime
        .process
        .files
        .directory_view(2)
        .unwrap()
        .entries
        .iter()
        .all(|e| e.id == 16));
    drop(_volume);
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn checkpoints_follow_successful_metadata_resize_resource_creation_and_delete_changes() {
    use systemless_future::{
        files::Op,
        memory::{GuestPtr, Permissions},
    };
    let root = directory();
    let path = root.join("mutations.slv");
    let f = support::pef_program(&[0x4e800020], &[], 0);
    let mut app = LoadedApplication::pef(&f.bytes, None).unwrap();
    let pb = GuestPtr(0x50000000);
    let name = GuestPtr(0x50000100);
    app.runtime
        .memory
        .map(pb, vec![0; 512], Permissions::RW)
        .unwrap();
    app.runtime.memory.write(name, b"\x04Game").unwrap();
    let mut block = [0; 80];
    block[18..22].copy_from_slice(&name.0.to_be_bytes());
    block[22..24].copy_from_slice(&(-1i16).to_be_bytes());
    app.runtime.memory.write(pb, &block).unwrap();
    let mut volume = Volume::open(&path, &mut app.runtime.process.files).unwrap();
    assert!(volume.checkpoint(&app.runtime.process.files).unwrap());
    assert_eq!(
        app.runtime
            .process
            .files
            .io(
                &mut app.runtime.memory,
                pb,
                Op::Create {
                    hierarchical: false
                }
            )
            .unwrap(),
        0
    );
    assert!(volume.checkpoint(&app.runtime.process.files).unwrap());
    block[32..36].copy_from_slice(b"SAVE");
    app.runtime.memory.write(pb, &block).unwrap();
    assert_eq!(
        app.runtime
            .process
            .files
            .io(
                &mut app.runtime.memory,
                pb,
                Op::SetInfo {
                    hierarchical: false
                }
            )
            .unwrap(),
        0
    );
    assert!(volume.checkpoint(&app.runtime.process.files).unwrap());
    assert_eq!(
        app.runtime
            .process
            .files
            .create_resource_file(&app.runtime.memory, name)
            .unwrap(),
        0
    );
    assert!(volume.checkpoint(&app.runtime.process.files).unwrap());
    block[27] = 3;
    app.runtime.memory.write(pb, &block).unwrap();
    assert_eq!(
        app.runtime
            .process
            .files
            .io(
                &mut app.runtime.memory,
                pb,
                Op::Open {
                    kind: Fork::Data,
                    hierarchical: false
                }
            )
            .unwrap(),
        0
    );
    assert!(!volume.checkpoint(&app.runtime.process.files).unwrap());
    assert_eq!(
        app.runtime
            .process
            .files
            .io(
                &mut app.runtime.memory,
                pb,
                Op::Delete {
                    hierarchical: false
                }
            )
            .unwrap(),
        -47
    );
    assert!(!volume.checkpoint(&app.runtime.process.files).unwrap());
    app.runtime
        .memory
        .write(GuestPtr(pb.0 + 28), &7u32.to_be_bytes())
        .unwrap();
    assert_eq!(
        app.runtime
            .process
            .files
            .io(&mut app.runtime.memory, pb, Op::SetEof)
            .unwrap(),
        0
    );
    assert!(volume.checkpoint(&app.runtime.process.files).unwrap());
    let mut restored = LoadedApplication::pef(&f.bytes, None).unwrap();
    restored
        .runtime
        .process
        .files
        .restore_volume(&std::fs::read(&path).unwrap())
        .unwrap();
    assert_eq!(
        restored
            .runtime
            .process
            .files
            .catalog_fork(17, Fork::Data)
            .unwrap(),
        [0; 7]
    );
    systemless_future::load::resource::ResourceFork::parse(
        restored
            .runtime
            .process
            .files
            .catalog_fork(17, Fork::Resource)
            .unwrap(),
    )
    .unwrap();
    assert_eq!(
        app.runtime
            .process
            .files
            .io(&mut app.runtime.memory, pb, Op::Close)
            .unwrap(),
        0
    );
    assert!(!volume.checkpoint(&app.runtime.process.files).unwrap());
    assert_eq!(
        app.runtime
            .process
            .files
            .io(
                &mut app.runtime.memory,
                pb,
                Op::Delete {
                    hierarchical: false
                }
            )
            .unwrap(),
        0
    );
    assert!(volume.checkpoint(&app.runtime.process.files).unwrap());
    let mut restored = LoadedApplication::pef(&f.bytes, None).unwrap();
    restored
        .runtime
        .process
        .files
        .restore_volume(&std::fs::read(&path).unwrap())
        .unwrap();
    assert_eq!(
        restored
            .runtime
            .process
            .files
            .directory_view(2)
            .unwrap()
            .entries
            .len(),
        1
    );
    drop(volume);
    std::fs::remove_dir_all(root).unwrap();
}
