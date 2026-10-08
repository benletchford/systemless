//! Exercise the public command with a redistributable application and external scripts.
#![cfg(feature = "gui")]
use serde_json::{json, Value};
use std::{
    fs,
    path::{Path, PathBuf},
    process::{Command, Output},
};

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}
fn fixture() -> PathBuf {
    root().join("tests/toolbox-showcase/toolbox-showcase.sit")
}
fn invoke(archive: &Path, script: &Path, output: &Path, reference: Option<&Path>) -> Output {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_systemless"));
    cmd.arg(archive)
        .args(["--headless", "--prefer-classic-68k", "--play-script"])
        .arg(script)
        .arg("--play-output")
        .arg(output);
    if let Some(reference) = reference {
        cmd.arg("--play-reference").arg(reference);
    }
    cmd.output().expect("run Systemless")
}
fn report(output: &Path) -> Value {
    serde_json::from_slice(&fs::read(output.join("report.json")).unwrap()).unwrap()
}

#[test]
fn showcase_inputs_captures_and_exact_replay() {
    let temp = tempfile::tempdir().unwrap();
    let script = root().join("tests/play/toolbox-showcase.json");
    let first = temp.path().join("first");
    let output = invoke(&fixture(), &script, &first, None);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let result = report(&first);
    assert_eq!(result["status"], "passed");
    assert_eq!(result["assertions"], 2);
    assert_eq!(result["architecture"], "68k");
    let graphics = image::open(first.join("graphics.png")).unwrap().to_rgb8();
    let controls = image::open(first.join("controls.png")).unwrap().to_rgb8();
    assert_ne!(
        graphics, controls,
        "the input sequence must change the page"
    );
    let replay = temp.path().join("replay");
    let output = invoke(&fixture(), &script, &replay, Some(&first));
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(report(&replay)["assertions"], 4);
    for checkpoint in report(&replay)["checkpoints"].as_array().unwrap() {
        assert_eq!(checkpoint["differing_pixels"], 0);
    }
    let retained = fs::read(first.join("report.json")).unwrap();
    assert!(!invoke(&fixture(), &script, &first, None).status.success());
    assert_eq!(retained, fs::read(first.join("report.json")).unwrap());
    fs::remove_file(first.join("graphics.png")).unwrap();
    let missing = temp.path().join("missing-reference");
    assert!(!invoke(&fixture(), &script, &missing, Some(&first))
        .status
        .success());
    assert!(report(&missing)["error"]
        .as_str()
        .unwrap()
        .contains("reference"));
}

#[test]
fn failures_are_nonzero_with_machine_readable_reports() {
    let temp = tempfile::tempdir().unwrap();
    let cases = [
        (
            "assertion",
            json!([{"type":"assert_pixel","x":100000,"y":0,"rgb":[0,0,0]}]),
            "outside",
        ),
        (
            "timeout",
            json!([{"type":"run_until_pixel","x":0,"y":0,"rgb":[1,2,3],"timeout_ticks":1}]),
            "timed out",
        ),
        ("budget", json!([{"type":"run","ticks":3}]), "max_ticks"),
        (
            "unknown",
            json!([{"type":"run","ticks":1,"ignored_typo":true}]),
            "unknown field",
        ),
    ];
    for (name, actions, expected) in cases {
        let script = temp.path().join(format!("{name}.json"));
        fs::write(
            &script,
            serde_json::to_vec(
                &json!({"version":1,"clock":"frontend_ticks","max_ticks":2,"actions":actions}),
            )
            .unwrap(),
        )
        .unwrap();
        let output = temp.path().join(name);
        assert!(
            !invoke(&fixture(), &script, &output, None).status.success(),
            "{name}"
        );
        let result = report(&output);
        assert_eq!(result["status"], "failed");
        assert!(
            result["error"].as_str().unwrap().contains(expected),
            "{result}"
        );
    }
    let output = temp.path().join("missing-archive");
    assert!(!invoke(
        &temp.path().join("missing.sit"),
        &root().join("tests/play/toolbox-showcase.json"),
        &output,
        None
    )
    .status
    .success());
    assert!(report(&output)["error"].as_str().unwrap().contains("load"));
}
