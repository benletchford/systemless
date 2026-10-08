//! Docker integration for building and running Mac apps.

use crate::container_runtime;
use std::path::Path;
use std::process::{Command, Stdio};

pub fn docker_mount_source(path: &Path) -> String {
    path.display().to_string()
}

/// Docker image names
pub const RUNNER_IMAGE: &str = "basiliskii:latest";
pub const MPW_BUILDER_IMAGE: &str = "mpw-builder:latest";

/// Run BasiliskII in Docker container
/// - basiliskii_dir: contains System_68K.dsk and MacRom.rom (mounted as /mnt/shared for prefs)
/// - output_dir: test output with FixtureGen (mounted as /mnt/extfs for Unix volume)
pub fn run_basilisk(output_dir: &Path, basiliskii_dir: &Path) {
    let runtime = container_runtime();
    let shared_src = docker_mount_source(basiliskii_dir);
    let extfs_src = docker_mount_source(output_dir);
    // Booting to Startup Items and running the fixture takes well over 30s on a
    // cold container; SYSTEMLESS_FIXGEN_TIMEOUT overrides the default.
    let exit_timeout = format!(
        "EXIT_TIMEOUT={}",
        std::env::var("SYSTEMLESS_FIXGEN_TIMEOUT").unwrap_or_else(|_| "180".to_string())
    );
    let status = Command::new(&runtime)
        .args([
            "run",
            "--rm",
            "--privileged",
            "-e",
            exit_timeout.as_str(),
            "-v",
            &format!("{}:/mnt/shared", shared_src), // Remove :ro - need write access for builds
            "-v",
            &format!("{}:/mnt/extfs", extfs_src),
            RUNNER_IMAGE,
        ])
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .status()
        .expect("Failed to run BasiliskII container");

    if !status.success() {
        eprintln!("Warning: BasiliskII container exited with error");
    }
}

/// Run the MPW builder container (using mps)
pub fn run_mpw(workspace_dir: &Path, args: &[&str]) {
    let runtime = container_runtime();
    let mount_src = docker_mount_source(workspace_dir);
    let status = Command::new(&runtime)
        .args([
            "run",
            "--rm",
            "-v",
            &format!("{}:/workspace", mount_src),
            MPW_BUILDER_IMAGE,
        ])
        .args(args)
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .status()
        .expect("Failed to run MPW builder");

    if !status.success() {
        panic!("MPW build failed");
    }
}

/// Run SimpleRez to convert .rdump to binary resources
pub fn run_simplerez(workspace_dir: &Path, input_path: &str, output_path: &str) {
    let runtime = container_runtime();
    let mount_src = docker_mount_source(workspace_dir);
    let status = Command::new(&runtime)
        .args([
            "run",
            "--rm",
            "-v",
            &format!("{}:/workspace", mount_src),
            "--entrypoint",
            "SimpleRez",
            MPW_BUILDER_IMAGE,
            input_path,
            "-o",
            output_path,
        ])
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .status()
        .expect("Failed to run SimpleRez");

    if !status.success() {
        panic!("SimpleRez conversion failed");
    }
}

/// Check if a docker image exists locally
fn image_exists(image_name: &str) -> bool {
    let runtime = container_runtime();
    let status = Command::new(&runtime)
        .args(["image", "inspect", image_name])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .unwrap_or_else(|_| std::process::ExitStatus::default()); // handle error gracefully
    status.success()
}

/// Build the runner Docker image
pub fn build_runner_image(runner_docker: &Path, force_rebuild: bool) {
    if !force_rebuild && image_exists(RUNNER_IMAGE) {
        println!(
            "Runner image '{}' already exists. Skipping build.",
            RUNNER_IMAGE
        );
        return;
    }

    let runtime = container_runtime();
    let status = Command::new(&runtime)
        .args(["build", "-t", RUNNER_IMAGE, runner_docker.to_str().unwrap()])
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .status()
        .expect("Failed to build runner Docker image");

    if !status.success() {
        panic!("Docker build failed");
    }
}

/// Build the MPW builder Docker image (mps)
pub fn build_mpw_builder_image(mpw_docker_dir: &Path, force_rebuild: bool) {
    if !force_rebuild && image_exists(MPW_BUILDER_IMAGE) {
        println!(
            "MPW builder image '{}' already exists. Skipping build.",
            MPW_BUILDER_IMAGE
        );
        return;
    }

    let runtime = container_runtime();
    println!(
        "Building MPW builder image from {}",
        mpw_docker_dir.display()
    );
    let status = Command::new(&runtime)
        .args([
            "build",
            "-t",
            MPW_BUILDER_IMAGE,
            mpw_docker_dir.to_str().unwrap(),
        ])
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .status()
        .expect("Failed to build MPW builder Docker image");

    if !status.success() {
        panic!("MPW Builder Docker build failed");
    }
}

/// Run an arbitrary shell command inside the BasiliskII container (without starting emulation)
pub fn run_shell_command(basiliskii_dir: &Path, command: &str) {
    let runtime = container_runtime();
    let shared_src = docker_mount_source(basiliskii_dir);
    let extfs_src = docker_mount_source(&basiliskii_dir.join("extfs"));

    // We override entrypoint to /bin/bash to run commands
    let status = Command::new(&runtime)
        .args([
            "run",
            "--rm",
            "--entrypoint",
            "/bin/bash",
            "-v",
            &format!("{}:/mnt/shared", shared_src),
            "-v",
            &format!("{}:/mnt/extfs", extfs_src),
            RUNNER_IMAGE,
            "-c",
            command,
        ])
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .status()
        .expect("Failed to run shell command in container");

    if !status.success() {
        panic!("Shell command failed: {}", command);
    }
}
