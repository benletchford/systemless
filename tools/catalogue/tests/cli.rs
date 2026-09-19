mod common;
use std::{
    fs,
    process::{Command, Output},
};
fn cli(root: &std::path::Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_catalogue"))
        .arg("--root")
        .arg(root)
        .args(args)
        .env_remove("R2_ACCESS_KEY_ID")
        .env_remove("R2_SECRET_ACCESS_KEY")
        .env_remove("R2_ACCOUNT_ID")
        .env_remove("R2_BUCKET")
        .output()
        .unwrap()
}

#[test]
fn empty_checkout_builds_for_production_without_downloads_or_deletions() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    fs::create_dir(root.join("catalogue")).unwrap();
    fs::write(
        root.join("catalogue/.gitkeep"),
        include_bytes!("../../../catalogue/.gitkeep"),
    )
    .unwrap();

    for args in [
        vec!["check"],
        vec!["check", "--no-incoming"],
        vec!["check", "--production"],
    ] {
        let result = cli(root, &args);
        assert!(
            result.status.success(),
            "{args:?}: {}",
            String::from_utf8_lossy(&result.stderr)
        );
    }
    let stats = cli(root, &["stats"]);
    assert!(stats.status.success());
    let stats: serde_json::Value = serde_json::from_slice(&stats.stdout).unwrap();
    for field in [
        "entries",
        "unique_objects",
        "unique_bytes",
        "pending_assets",
    ] {
        assert_eq!(stats[field], 0);
    }
    let first = root.join("first.rs");
    let second = root.join("second.rs");
    for output in [&first, &second] {
        let result = cli(
            root,
            &[
                "build",
                "--production",
                "--output",
                output.to_str().unwrap(),
            ],
        );
        assert!(
            result.status.success(),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
    }
    assert_eq!(fs::read(&first).unwrap(), fs::read(&second).unwrap());
    assert_eq!(
        fs::read_to_string(&first).unwrap(),
        "pub static GAMES: &[Game] = &[\n];\n"
    );

    let fetch = cli(root, &["assets", "fetch"]);
    assert!(fetch.status.success());
    assert_eq!(
        serde_json::from_slice::<serde_json::Value>(&fetch.stdout).unwrap(),
        serde_json::json!([])
    );
    assert!(!root.join("catalogue/.downloads").exists());
    assert!(!root.join("catalogue/incoming").exists());

    let inventory = root.join("inventory.json");
    fs::write(&inventory, include_str!("fixtures/empty-inventory.json")).unwrap();
    let result = cli(
        root,
        &["r2", "plan", "--inventory", inventory.to_str().unwrap()],
    );
    assert!(!result.status.success());
    let plan: serde_json::Value = serde_json::from_slice(&result.stdout).unwrap();
    assert_eq!(plan["delete"], serde_json::json!([]));
    assert!(plan["blockers"]
        .as_array()
        .unwrap()
        .iter()
        .any(|reason| reason.as_str().unwrap().contains("empty desired set")));
}

#[test]
fn commands_work_without_credentials_and_outputs_are_deterministic() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    fs::create_dir(root.join("catalogue")).unwrap();
    fs::write(root.join("catalogue/.gitkeep"), b"").unwrap();
    for document in common::catalogue().documents {
        fs::write(
            root.join(format!("catalogue/{}.md", document.entry.id)),
            document.original,
        )
        .unwrap();
    }
    for args in [
        vec!["check"],
        vec!["check", "--no-incoming"],
        vec!["stats"],
        vec!["assets"],
    ] {
        let r = cli(root, &args);
        assert!(
            r.status.success(),
            "{:?}: {}",
            args,
            String::from_utf8_lossy(&r.stderr)
        );
    }
    assert!(!cli(root, &["check", "--production"]).status.success());
    let first = root.join("first.rs");
    let second = root.join("second.rs");
    for output in [&first, &second] {
        assert!(cli(root, &["build", "--output", output.to_str().unwrap()])
            .status
            .success());
    }
    assert_eq!(fs::read(&first).unwrap(), fs::read(&second).unwrap());
    let inventory = root.join("inventory.json");
    fs::write(&inventory, include_str!("fixtures/empty-inventory.json")).unwrap();
    let result = cli(
        root,
        &["r2", "plan", "--inventory", inventory.to_str().unwrap()],
    );
    assert!(!result.status.success());
    let plan: serde_json::Value = serde_json::from_slice(&result.stdout).unwrap();
    assert!(!plan["blockers"].as_array().unwrap().is_empty());
    assert!(
        !cli(root, &["media", "promote", "--entry", "does-not-exist"])
            .status
            .success()
    );
}

#[test]
fn promotion_rehearsal_and_recovery_survive_interrupted_source_commit() {
    use systemless_catalogue_tools::catalogue_tools::{assets, catalogue, *};
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let storage = tempfile::tempdir().unwrap();
    fs::create_dir(root.join("catalogue")).unwrap();
    fs::write(root.join("catalogue/.gitkeep"), b"").unwrap();
    fs::create_dir_all(root.join("catalogue/incoming/sample-game")).unwrap();
    image::RgbImage::from_pixel(1, 1, image::Rgb([1, 2, 3]))
        .save(root.join("catalogue/incoming/sample-game/shot.png"))
        .unwrap();
    let config = Config::default();
    let mut e = common::catalogue().documents.pop().unwrap().entry;
    let a = Artifact {
        id: "screenshot".into(),
        role: ArtifactRole::Screenshot,
        format: FileType::Png,
        source: AssetSource::Incoming {
            path: "catalogue/incoming/sample-game/shot.png".into(),
        },
        provenance: Provenance {
            redistribution: Redistribution::Permitted,
            original: true,
            content_only: true,
            sources: vec!["https://example.org/license".into()],
            license: Some("CC0-1.0".into()),
            rights_holder: None,
            permission: None,
            notes: None,
        },
    };
    e.artifacts = vec![a];
    let body = "\n![Screenshot](incoming/sample-game/shot.png)\n";
    let before = catalogue::serialize_document(&e, body).unwrap();
    fs::write(root.join("catalogue/sample-game.md"), &before).unwrap();
    let fetched = cli(root, &["assets", "fetch", "--entry", "sample-game"]);
    assert!(
        fetched.status.success(),
        "{}",
        String::from_utf8_lossy(&fetched.stderr)
    );
    let records: serde_json::Value = serde_json::from_slice(&fetched.stdout).unwrap();
    assert_eq!(records.as_array().unwrap().len(), 1);
    assert!(root.join("catalogue/.downloads").is_dir());
    assert_eq!(
        fs::read_to_string(root.join("catalogue/sample-game.md")).unwrap(),
        before
    );
    let inspection =
        assets::hash_file(&root.join("catalogue/incoming/sample-game/shot.png")).unwrap();
    let image_bytes = fs::read(root.join("catalogue/incoming/sample-game/shot.png")).unwrap();
    let result = cli(
        root,
        &[
            "media",
            "promote",
            "--apply",
            "--local-store",
            storage.path().to_str().unwrap(),
        ],
    );
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let after = fs::read_to_string(root.join("catalogue/sample-game.md")).unwrap();
    assert!(cli(root, &["check", "--production"]).status.success());
    // Simulate a crash after committing source but before deleting incoming and the journal.
    fs::write(
        root.join("catalogue/incoming/sample-game/shot.png"),
        image_bytes,
    )
    .unwrap();
    fs::write(root.join("catalogue/.promotion/transaction.json"),serde_json::to_vec(&serde_json::json!({"schema_version":1,"config":config,"edits":[{"id":"sample-game","before":before,"after":after}],"removals":[{"path":"catalogue/incoming/sample-game/shot.png","inspection":inspection}]})).unwrap()).unwrap();
    assert!(!cli(root, &["check"]).status.success());
    // Recovery must not overwrite an edit made after the crash.
    fs::write(
        root.join("catalogue/sample-game.md"),
        format!("{after}\nCommunity edit\n"),
    )
    .unwrap();
    assert!(!cli(root, &["assets", "recover"]).status.success());
    assert!(root
        .join("catalogue/incoming/sample-game/shot.png")
        .exists());
    fs::write(root.join("catalogue/sample-game.md"), after).unwrap();
    let result = cli(root, &["assets", "recover"]);
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    assert!(!root
        .join("catalogue/incoming/sample-game/shot.png")
        .exists());
    assert!(!root.join("catalogue/.promotion/transaction.json").exists());
    assert!(cli(root, &["check", "--production"]).status.success());
}
