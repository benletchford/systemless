use std::{
    env, fs,
    path::{Path, PathBuf},
    process::Command,
};

use systemless_catalogue_tools::catalogue_tools;

const UNKNOWN: &str = "unknown";
const SYSTEMLESS_REPOSITORY: &str = "https://github.com/benletchford/systemless";

fn main() {
    let manifest_dir = PathBuf::from(
        env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR should be set by Cargo"),
    );
    let repository_root = manifest_dir
        .parent()
        .expect("www must be directly below the repository root");

    let catalogue_dir = manifest_dir.join("catalogue");
    println!("cargo:rerun-if-changed={}", catalogue_dir.display());
    println!(
        "cargo:rerun-if-changed={}",
        repository_root.join("Cargo.toml").display()
    );
    println!("cargo:rerun-if-env-changed=SYSTEMLESS_GIT_SHA");
    println!("cargo:rerun-if-env-changed=GITHUB_SHA");
    println!("cargo:rerun-if-env-changed=SYSTEMLESS_CATALOGUE_PRODUCTION");

    let mode = match env::var("SYSTEMLESS_CATALOGUE_PRODUCTION").as_deref() {
        Ok("1") => catalogue_tools::Mode::Production,
        Err(env::VarError::NotPresent) => catalogue_tools::Mode::Preview,
        _ => panic!("SYSTEMLESS_CATALOGUE_PRODUCTION must be 1 or unset"),
    };
    let catalogue = catalogue_tools::load(&manifest_dir, mode).expect("invalid embedded catalogue");
    let compiled = catalogue_tools::build(&catalogue).expect("catalogue compilation failed");
    let generated =
        catalogue_tools::site::rust_games(&compiled).expect("invalid game configuration");
    fs::write(
        PathBuf::from(env::var_os("OUT_DIR").expect("OUT_DIR should be set by Cargo"))
            .join("games.rs"),
        generated,
    )
    .expect("failed to generate browser catalogue");

    let manifest = fs::read_to_string(repository_root.join("Cargo.toml"))
        .expect("failed to read the Systemless manifest");
    let version = package_value(&manifest, "version").unwrap_or_else(|| UNKNOWN.to_string());
    let repository =
        package_value(&manifest, "repository").unwrap_or_else(|| SYSTEMLESS_REPOSITORY.to_string());
    let sha = env_sha("SYSTEMLESS_GIT_SHA")
        .or_else(|| env_sha("GITHUB_SHA"))
        .or_else(|| checkout_sha(repository_root))
        .unwrap_or_else(|| UNKNOWN.to_string());

    cargo_env("SYSTEMLESS_VERSION", &version);
    cargo_env("SYSTEMLESS_REPOSITORY", &repository);
    cargo_env("SYSTEMLESS_GIT_SHA", &sha);
}

fn package_value(manifest: &str, key: &str) -> Option<String> {
    let package = manifest.split_once("[package]")?.1;
    let package = package
        .split_once("\n[")
        .map_or(package, |(section, _)| section);
    package
        .lines()
        .find_map(|line| toml_string_value(line, key))
}

fn toml_string_value(line: &str, key: &str) -> Option<String> {
    let rest = line.trim().strip_prefix(key)?.trim_start();
    let rest = rest.strip_prefix('=')?.trim_start();
    let rest = rest.strip_prefix('"')?;
    let end = rest.find('"')?;
    Some(rest[..end].to_string())
}

fn env_sha(name: &str) -> Option<String> {
    let sha = env::var(name).ok()?;
    is_git_sha(&sha).then_some(sha)
}

fn checkout_sha(repository_root: &Path) -> Option<String> {
    let output = Command::new("git")
        .args(["rev-parse", "HEAD"])
        .current_dir(repository_root)
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let sha = String::from_utf8(output.stdout).ok()?.trim().to_string();
    is_git_sha(&sha).then_some(sha)
}

fn is_git_sha(value: &str) -> bool {
    (7..=40).contains(&value.len()) && value.chars().all(|ch| ch.is_ascii_hexdigit())
}

fn cargo_env(key: &str, value: &str) {
    println!("cargo:rustc-env={key}={}", value.replace(['\n', '\r'], ""));
}
