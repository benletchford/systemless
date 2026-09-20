use crate::catalogue_tools::model::*;
use anyhow::{bail, ensure, Context, Result};
use std::{
    collections::BTreeSet,
    path::{Component, Path, PathBuf},
};
use url::Url;

pub fn nonempty(field: &str, value: &str) -> Result<()> {
    ensure!(
        !value.trim().is_empty() && !value.contains('\0'),
        "{field} must be nonempty and contain no NUL"
    );
    Ok(())
}
pub fn slug(value: &str) -> Result<()> {
    ensure!(
        !value.is_empty()
            && value.len() <= 160
            && value.split('-').all(|part| !part.is_empty()
                && part
                    .bytes()
                    .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit())),
        "invalid stable ID {value:?}; use lowercase ASCII words separated by hyphens"
    );
    Ok(())
}
pub fn sha256(value: &str) -> Result<()> {
    ensure!(
        value.len() == 64
            && value
                .bytes()
                .all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c)),
        "SHA-256 must be 64 lowercase hex characters"
    );
    Ok(())
}
pub fn https(value: &str) -> Result<Url> {
    let url = Url::parse(value).context("invalid URL")?;
    ensure!(
        url.scheme() == "https"
            && url.host_str().is_some()
            && url.username().is_empty()
            && url.password().is_none()
            && url.fragment().is_none(),
        "expected an HTTPS URL without credentials or fragment: {value}"
    );
    ensure!(
        !value.chars().any(char::is_whitespace),
        "URL must percent-encode whitespace"
    );
    Ok(url)
}
pub fn config(config: &Config) -> Result<()> {
    ensure!(
        config.schema_version == SCHEMA_VERSION,
        "unsupported schema_version {}",
        config.schema_version
    );
    let repo = https(&config.repository)?;
    ensure!(
        repo.host_str() == Some("github.com") && repo.port().is_none() && repo.query().is_none(),
        "repository must be a github.com repository URL"
    );
    let parts: Vec<_> = repo.path().trim_matches('/').split('/').collect();
    ensure!(
        parts.len() == 2
            && parts.iter().all(|p| !p.is_empty()
                && p.bytes()
                    .all(|c| c.is_ascii_alphanumeric() || b"-_.".contains(&c))),
        "repository must identify owner/repo"
    );
    ensure!(
        config.branch.split('/').all(|s| !s.is_empty()
            && s != "."
            && s != ".."
            && s.bytes()
                .all(|c| c.is_ascii_alphanumeric() || b"-_.".contains(&c))),
        "invalid source branch"
    );
    let url = https(&config.asset_base_url)?;
    ensure!(
        url.query().is_none() && url.path() == "/" && url.port().is_none(),
        "asset_base_url must be an HTTPS origin"
    );
    Ok(())
}
pub fn route(value: &str) -> Result<()> {
    ensure!(
        value.starts_with('/') && value.len() > 1 && !value.ends_with('/'),
        "route must be a canonical non-root path without trailing slash: {value}"
    );
    for part in value[1..].split('/') {
        slug(part)?;
    }
    ensure!(
        !matches!(
            value.split('/').nth(1),
            Some("assets" | "next" | "catalogue")
        ),
        "reserved route {value}"
    );
    Ok(())
}
pub fn relative_path(value: &str) -> Result<()> {
    nonempty("relative path", value)?;
    ensure!(
        !value.contains('\\') && !value.contains(':') && !value.chars().any(char::is_control),
        "invalid relative path {value:?}"
    );
    ensure!(
        value
            .split('/')
            .all(|s| !s.is_empty() && s != "." && s != ".."),
        "unsafe relative path {value:?}"
    );
    ensure!(
        Path::new(value)
            .components()
            .all(|c| matches!(c, Component::Normal(_))),
        "unsafe relative path {value:?}"
    );
    Ok(())
}
/// Reject symlinks component-by-component; canonicalize as a second containment check.
pub fn safe_file(root: &Path, relative: &str) -> Result<PathBuf> {
    relative_path(relative)?;
    let root = root.canonicalize()?;
    let mut path = root.clone();
    for component in Path::new(relative).components() {
        path.push(component);
        ensure!(
            !std::fs::symlink_metadata(&path)?.file_type().is_symlink(),
            "symlink forbidden: {}",
            path.display()
        );
    }
    ensure!(
        path.is_file() && path.canonicalize()?.starts_with(&root),
        "expected a contained regular file: {relative}"
    );
    Ok(path)
}
pub fn provenance(p: &Provenance) -> Result<()> {
    for url in &p.sources {
        https(url)?;
    }
    for s in [&p.license, &p.rights_holder, &p.permission, &p.notes]
        .into_iter()
        .flatten()
    {
        nonempty("provenance", s)?;
    }
    if p.redistribution == Redistribution::Permitted {
        ensure!(
            !p.sources.is_empty() && (p.license.is_some() || p.permission.is_some()),
            "permitted redistribution needs a source and license or permission evidence"
        );
    }
    Ok(())
}
pub fn key(key: &str) -> Result<()> {
    ensure!(
        matches!(
            key,
            "ArrowUp"
                | "ArrowDown"
                | "ArrowLeft"
                | "ArrowRight"
                | "Space"
                | "Enter"
                | "Tab"
                | "Escape"
                | "Shift"
                | "None"
        ) || (key.len() == 1
            && key
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b".,-=[]\\".contains(&b))),
        "unsupported control key {key:?}"
    );
    Ok(())
}
fn buttons(buttons: &[Button]) -> Result<()> {
    ensure!(
        buttons.len() <= 6,
        "at most six mobile buttons are supported"
    );
    let mut labels = BTreeSet::new();
    for b in buttons {
        nonempty("button label", &b.label)?;
        ensure!(
            b.label.chars().count() <= 4 && labels.insert(&b.label),
            "button labels must be unique and at most four characters"
        );
        key(&b.key)?;
    }
    Ok(())
}
pub fn entry(e: &Entry) -> Result<()> {
    slug(&e.id)?;
    for (name, val) in [
        ("title", &e.title),
        ("summary", &e.summary),
        ("developer", &e.developer),
        ("category", &e.category),
    ] {
        nonempty(name, val)?;
    }
    ensure!(
        CATEGORIES.contains(&e.category.as_str()),
        "unsupported category {:?}; expected one of {}",
        e.category,
        CATEGORIES.join(", ")
    );
    if let Some(p) = &e.publisher {
        nonempty("publisher", p)?;
    }
    ensure!(
        (1970..=2100).contains(&e.year),
        "year must be between 1970 and 2100"
    );
    ensure!(
        !e.architectures.is_empty()
            && e.architectures.iter().collect::<BTreeSet<_>>().len() == e.architectures.len()
            && e.architectures.contains(&e.default_architecture),
        "architectures must be unique and include default_architecture"
    );
    route(&crate::catalogue_tools::community::canonical_path(e))?;
    for alias in &e.aliases {
        route(alias)?;
    }
    for v in &e.compatibility.verified {
        let _: jiff::civil::Date = v
            .date
            .parse()
            .context("verification date must be YYYY-MM-DD")?;
        ensure!(
            v.date.len() == 10 && e.architectures.contains(&v.architecture),
            "invalid verification date or architecture"
        );
        for s in [&v.tester, &v.systemless_version, &v.environment] {
            nonempty("verification", s)?;
        }
        ensure!(
            v.status != Status::Unknown,
            "verification must describe an observed status"
        );
        https(&v.evidence)?;
    }
    if e.compatibility.status != Status::Unknown {
        ensure!(
            e.compatibility
                .verified
                .iter()
                .any(|v| v.status == e.compatibility.status),
            "compatibility status needs matching verification evidence"
        );
    }
    let mut plugin_ids = BTreeSet::new();
    for plugin in &e.plugins {
        slug(&plugin.id)?;
        ensure!(plugin_ids.insert(&plugin.id), "duplicate plugin ID");
        ensure!(
            !plugin.label.trim().is_empty(),
            "plugin label must not be empty"
        );
        ensure!(
            !plugin.description.trim().is_empty(),
            "plugin description must not be empty"
        );
        for id in std::iter::once(&plugin.download_artifact)
            .chain(plugin.install.iter().map(|install| &install.artifact))
        {
            ensure!(
                e.artifacts
                    .iter()
                    .any(|a| &a.id == id && a.role == ArtifactRole::Supplement),
                "plugin must reference a declared supplement artifact: {id}"
            );
        }
        let mut installations = BTreeSet::new();
        for install in &plugin.install {
            relative_path(&install.mount_path)?;
            ensure!(
                e.artifacts
                    .iter()
                    .any(|a| a.id == install.artifact && a.format == FileType::Bin),
                "plugin installation requires a MacBinary (bin) artifact"
            );
            ensure!(
                installations.insert((&install.artifact, &install.mount_path)),
                "duplicate plugin installation"
            );
        }
    }
    for (i, modifier) in e.runtime.launch_modifiers.iter().enumerate() {
        ensure!(
            !e.runtime.launch_modifiers[..i].contains(modifier),
            "duplicate launch modifier"
        );
    }
    let pacing = &e.runtime.runtime_pacing;
    ensure!(
        (1..=4).contains(&pacing.max_ticks_per_paint)
            && (pacing.max_ticks_per_paint..=12).contains(&pacing.reset_slack_ticks)
            && (4..=120).contains(&pacing.cpu_mhz),
        "runtime pacing requires 1–4 paint ticks, paint ticks–12 slack ticks, and 4–120 MHz"
    );
    ensure!(
        e.runtime
            .application_partition_size
            .is_none_or(|n| n >= 128 * 1024),
        "application_partition_size must be at least 128 KiB"
    );
    let mut paths = BTreeSet::new();
    for path in &e.runtime.remove_paths {
        relative_path(path)?;
        ensure!(paths.insert(path), "duplicate remove path");
    }
    for (from, to) in &e.controls.key_mappings {
        key(from)?;
        key(to)?;
    }
    let mobile = &e.controls.mobile;
    for k in [
        &mobile.joystick.up,
        &mobile.joystick.down,
        &mobile.joystick.left,
        &mobile.joystick.right,
    ] {
        key(k)?;
    }
    ensure!(
        mobile.buttons.is_empty() || mobile.button_groups.is_empty(),
        "use mobile buttons or button_groups, not both"
    );
    ensure!(
        mobile.button_groups.len() <= 4,
        "at most four mobile button groups"
    );
    buttons(&mobile.buttons)?;
    let mut groups = BTreeSet::new();
    for group in &mobile.button_groups {
        nonempty("button group", &group.label)?;
        ensure!(
            group.label.chars().count() <= 4 && groups.insert(&group.label),
            "invalid or duplicate button group label"
        );
        buttons(&group.buttons)?;
    }
    for reference in &e.references {
        https(reference)?;
    }
    let mut artifacts = BTreeSet::new();
    for a in &e.artifacts {
        slug(&a.id)?;
        ensure!(artifacts.insert(&a.id), "duplicate artifact ID {}", a.id);
        ensure!(
            a.role != ArtifactRole::WebPack && a.format != FileType::Kpk,
            "generated web packs are derived assets; reference the original distributable"
        );
        ensure!(
            a.role != ArtifactRole::Screenshot || a.format.media(),
            "screenshot must use an image format"
        );
        ensure!(
            a.role != ArtifactRole::Screenshot || a.provenance.content_only,
            "screenshot provenance must attest content_only: true"
        );
        let managed_software =
            a.role != ArtifactRole::Screenshot && !matches!(a.source, AssetSource::External { .. });
        ensure!(
            !managed_software || a.provenance.original,
            "hosted software must attest original: true for the unchanged distributable"
        );
        provenance(&a.provenance)?;
        match &a.source {
            AssetSource::Incoming { path } => {
                relative_path(path)?;
                ensure!(
                    path.starts_with(&format!("catalogue/incoming/{}/", e.id)),
                    "incoming asset must live under catalogue/incoming/{}/",
                    e.id
                );
            }
            AssetSource::Url {
                url,
                download_page,
                expected_sha256,
                expected_size,
            } => {
                crate::catalogue_tools::network::public_url(&https(url)?)?;
                if let Some(page) = download_page {
                    crate::catalogue_tools::network::public_url(&https(page)?)?;
                    ensure!(
                        expected_sha256.is_some() && expected_size.is_some(),
                        "download_page requires pinned SHA-256 and size"
                    );
                }
                if let Some(hash) = expected_sha256 {
                    sha256(hash)?;
                }
                if let Some(size) = expected_size {
                    ensure!(
                        *size > 0 && *size <= a.format.limit(),
                        "invalid expected asset size"
                    );
                }
            }
            AssetSource::External { url } => {
                https(url)?;
            }
            AssetSource::Sha256 {
                sha256: hash,
                size_bytes,
            } => {
                sha256(hash)?;
                ensure!(
                    *size_bytes > 0 && *size_bytes <= a.format.limit(),
                    "invalid asset size"
                );
                ensure!(
                    a.provenance.redistribution == Redistribution::Permitted,
                    "hosted asset requires permitted redistribution"
                );
                ensure!(
                    a.format != FileType::Opaque,
                    "opaque assets cannot be hosted"
                );
            }
        }
    }
    ensure!(
        e.artifacts
            .iter()
            .filter(|a| a.role == ArtifactRole::Archive)
            .count()
            <= 1,
        "at most one Archive artifact per entry"
    );
    Ok(())
}

pub(crate) fn is_entry_file(path: &Path) -> Result<bool> {
    let name = path
        .file_name()
        .and_then(|name| name.to_str())
        .context("non UTF-8 path in catalogue")?;
    if name == ".gitkeep" {
        let metadata = std::fs::symlink_metadata(path)?;
        ensure!(
            metadata.is_file() && metadata.len() == 0,
            "catalogue/.gitkeep must be an empty regular file"
        );
        return Ok(false);
    }
    if matches!(name, "incoming" | ".promotion" | ".downloads") {
        let metadata = std::fs::symlink_metadata(path)?;
        ensure!(
            metadata.is_dir() && !metadata.file_type().is_symlink(),
            "catalogue operational path must be a real directory: {}",
            path.display()
        );
        return Ok(false);
    }
    if name == ".lock" {
        let metadata = std::fs::symlink_metadata(path)?;
        ensure!(
            metadata.is_file() && !metadata.file_type().is_symlink(),
            "catalogue lock must be a regular file"
        );
        return Ok(false);
    }
    if path.extension().is_none_or(|e| e != "md") {
        bail!(
            "catalogue/ may only contain <id>.md files, reserved operational directories, and an empty .gitkeep: {}",
            path.display()
        );
    }
    Ok(true)
}
