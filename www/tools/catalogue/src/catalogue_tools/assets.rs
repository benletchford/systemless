use crate::catalogue_tools::{
    catalogue::{self, Catalogue, Mode},
    model::*,
    network, validate,
};
use anyhow::{bail, ensure, Context, Result};
use fs2::FileExt;
use image::{ImageFormat, ImageReader, Limits};
use pulldown_cmark::{Event, LinkType, Options, Parser, Tag};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs::{self, File, OpenOptions},
    io::{BufReader, Read, Write},
    path::{Path, PathBuf},
};

/// Keep contributed images small; local software staging uses the archive limit.
pub fn incoming_limit(format: FileType) -> u64 {
    if format.media() {
        10 * 1024 * 1024
    } else {
        format.limit()
    }
}
pub const MANAGED_PREFIXES: [&str; 2] = ["catalogue/media/sha256/", "catalogue/objects/sha256/"];

/// Callers validate hashes before using this pure key formatter.
pub fn object_key(hash: &str, format: FileType) -> String {
    let prefix = if format.media() {
        MANAGED_PREFIXES[0]
    } else {
        MANAGED_PREFIXES[1]
    };
    format!(
        "{prefix}{}/{hash}.{}",
        hash.get(..2).unwrap_or(""),
        format.ext()
    )
}

pub fn is_managed_key(key: &str) -> bool {
    let Some(tail) = MANAGED_PREFIXES.iter().find_map(|p| key.strip_prefix(p)) else {
        return false;
    };
    let Some((shard, filename)) = tail.split_once('/') else {
        return false;
    };
    let Some((hash, extension)) = filename.split_once('.') else {
        return false;
    };
    if validate::sha256(hash).is_err() || hash.get(..2) != Some(shard) {
        return false;
    }
    let formats = [
        FileType::Png,
        FileType::Jpg,
        FileType::Gif,
        FileType::Webp,
        FileType::Sit,
        FileType::Sitx,
        FileType::Zip,
        FileType::Kpk,
        FileType::Bin,
        FileType::Hqx,
        FileType::Img,
        FileType::Gz,
    ];
    formats
        .into_iter()
        .any(|f| f.ext() == extension && object_key(hash, f) == key)
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DesiredObject {
    pub key: String,
    pub sha256: String,
    pub size_bytes: u64,
    pub content_type: String,
    pub references: Vec<String>,
}

pub fn desired(c: &Catalogue) -> Result<Vec<DesiredObject>> {
    let mut objects: BTreeMap<String, DesiredObject> = BTreeMap::new();
    for d in &c.documents {
        for a in &d.entry.artifacts {
            if let AssetSource::Sha256 { sha256, size_bytes } = &a.source {
                validate::sha256(sha256)?;
                let key = object_key(sha256, a.format);
                let object = objects.entry(key.clone()).or_insert_with(|| DesiredObject {
                    key,
                    sha256: sha256.clone(),
                    size_bytes: *size_bytes,
                    content_type: a.format.mime().into(),
                    references: Vec::new(),
                });
                ensure!(
                    object.size_bytes == *size_bytes,
                    "conflicting size for {}",
                    object.key
                );
                object.references.push(format!("{}:{}", d.entry.id, a.id));
            }
        }
    }
    for object in objects.values_mut() {
        object.references.sort();
        object.references.dedup();
    }
    Ok(objects.into_values().collect())
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Inspection {
    pub sha256: String,
    pub size_bytes: u64,
}

pub fn hash_file(path: &Path) -> Result<Inspection> {
    let mut file = BufReader::new(File::open(path)?);
    let mut hasher = Sha256::new();
    let mut size_bytes = 0;
    let mut chunk = [0_u8; 64 * 1024];
    loop {
        let n = file.read(&mut chunk)?;
        if n == 0 {
            break;
        }
        hasher.update(&chunk[..n]);
        size_bytes += n as u64;
    }
    Ok(Inspection {
        sha256: hex::encode(hasher.finalize()),
        size_bytes,
    })
}

/// Check the actual format, not HTTP metadata or filename. Software is never executed or extracted.
pub fn inspect(path: &Path, format: FileType) -> Result<Inspection> {
    let size = fs::metadata(path)?.len();
    ensure!(
        size > 0 && size <= format.limit(),
        "asset must be nonempty and at most {} bytes",
        format.limit()
    );
    let mut file = File::open(path)?;
    let mut header = vec![0_u8; size.min(8192) as usize];
    file.read_exact(&mut header)?;
    if format.media() {
        let expected = match format {
            FileType::Png => ImageFormat::Png,
            FileType::Jpg => ImageFormat::Jpeg,
            FileType::Gif => ImageFormat::Gif,
            _ => ImageFormat::WebP,
        };
        ensure!(
            image::guess_format(&header)? == expected,
            "image content does not match declared format"
        );
        let mut reader = ImageReader::with_format(BufReader::new(File::open(path)?), expected);
        let mut limits = Limits::default();
        limits.max_image_width = Some(4096);
        limits.max_image_height = Some(4096);
        limits.max_alloc = Some(64 * 1024 * 1024);
        reader.limits(limits);
        reader
            .decode()
            .context("invalid image or image exceeds 4096×4096 / 64 MiB decode limit")?;
    } else {
        let valid = match format {
            FileType::Zip => [b"PK\x03\x04", b"PK\x05\x06", b"PK\x07\x08"]
                .iter()
                .any(|m| header.starts_with(*m)),
            FileType::Kpk => {
                size >= 8
                    && [b"KPK1", b"KPK2", b"KPK3"]
                        .iter()
                        .any(|m| header.starts_with(*m))
            }
            FileType::Sit => recognized_sit(&header, size) || macbinary_wrapped_sit(&header, size),
            FileType::Sitx => header.starts_with(b"StuffIt!"),
            FileType::Gz => size >= 18 && header.starts_with(&[0x1f, 0x8b, 8]),
            FileType::Bin => {
                size >= 128
                    && header[0] == 0
                    && (1..=63).contains(&header[1])
                    && header[74] == 0
                    && header[82] == 0
            }
            FileType::Hqx => String::from_utf8_lossy(&header)
                .contains("(This file must be converted with BinHex 4.0)"),
            FileType::Img => {
                let diskcopy = size >= 84
                    && header[0] <= 63
                    && header[82..84] == [1, 0]
                    && (u64::from(u32::from_be_bytes(header[64..68].try_into().unwrap()))
                        + u64::from(u32::from_be_bytes(header[68..72].try_into().unwrap()))
                        + 84
                        == size);
                let raw = size.is_multiple_of(512)
                    && (header.starts_with(b"ER")
                        || header
                            .get(1024..1026)
                            .is_some_and(|m| m == b"BD" || m == b"H+" || m == b"HX"));
                diskcopy || raw
            }
            _ => false,
        };
        ensure!(
            valid,
            "content is not recognized as a supported {} file",
            format.ext()
        );
    }
    hash_file(path)
}

fn recognized_sit(header: &[u8], size: u64) -> bool {
    let classic = size >= 22
        && (header.starts_with(b"SIT!")
            || header.starts_with(b"ST46")
            || header.starts_with(b"ST50"));
    // StuffIt 5 places its binary signature after an 80-byte text header.
    let sit5 = size >= 114
        && header.len() >= 88
        && header.starts_with(b"StuffIt (c)")
        && header[80..83] == [0x1a, 0x00, 0x05]
        && u64::from(u32::from_be_bytes(header[84..88].try_into().unwrap())) == size;
    classic || sit5
}

fn macbinary_wrapped_sit(header: &[u8], size: u64) -> bool {
    // MacBinary stores the data and resource forks after 128-byte-aligned boundaries.
    if header.len() < 128
        || header[0] != 0
        || !(1..=63).contains(&header[1])
        || header[74] != 0
        || header[82] != 0
    {
        return false;
    }
    let data_size = u64::from(u32::from_be_bytes(header[83..87].try_into().unwrap()));
    let resource_size = u64::from(u32::from_be_bytes(header[87..91].try_into().unwrap()));
    let padded = |n: u64| n.div_ceil(128) * 128;
    size == 128 + padded(data_size) + padded(resource_size)
        && recognized_sit(&header[128..], data_size)
}

/// A store must never replace an existing key with different content.
pub trait ObjectStore {
    fn put_if_absent(&mut self, object: &DesiredObject, file: &Path) -> Result<()>;
}

/// Credential-free fixture backend, also useful for a local end-to-end rehearsal.
pub struct DirectoryStore {
    pub root: PathBuf,
}
impl ObjectStore for DirectoryStore {
    fn put_if_absent(&mut self, object: &DesiredObject, file: &Path) -> Result<()> {
        validate_object(object)?;
        fs::create_dir_all(&self.root)?;
        let path = self.root.join(&object.key);
        let parent = path.parent().unwrap();
        // A local store is a dedicated directory, never a symlink-backed tree.
        let mut current = self.root.clone();
        ensure!(
            !fs::symlink_metadata(&current)?.file_type().is_symlink(),
            "store root symlink forbidden"
        );
        for part in Path::new(&object.key).parent().unwrap().components() {
            current.push(part);
            if current.exists() {
                ensure!(
                    !fs::symlink_metadata(&current)?.file_type().is_symlink(),
                    "store symlink forbidden"
                );
            } else {
                fs::create_dir(&current)?;
            }
        }
        let mut temp = tempfile::NamedTempFile::new_in(parent)?;
        let actual = hash_file(file)?;
        ensure!(
            actual.sha256 == object.sha256 && actual.size_bytes == object.size_bytes,
            "staged content changed"
        );
        std::io::copy(&mut File::open(file)?, &mut temp)?;
        temp.as_file().sync_all()?;
        match temp.persist_noclobber(&path) {
            Ok(_) => Ok(()),
            Err(e) if e.error.kind() == std::io::ErrorKind::AlreadyExists => {
                ensure!(
                    !fs::symlink_metadata(&path)?.file_type().is_symlink(),
                    "store object symlink forbidden"
                );
                ensure!(
                    hash_file(&path)? == actual,
                    "immutable object already exists with different content"
                );
                Ok(())
            }
            Err(e) => Err(e.error.into()),
        }
    }
}

pub struct RepoLock {
    _file: File,
}
impl RepoLock {
    pub fn acquire(root: &Path) -> Result<Self> {
        let path = root.join("catalogue/.lock");
        if let Ok(meta) = fs::symlink_metadata(&path) {
            ensure!(!meta.file_type().is_symlink(), "lock symlink forbidden");
        }
        let file = OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(path)?;
        file.try_lock_exclusive()
            .context("another catalogue mutation is in progress")?;
        Ok(Self { _file: file })
    }
}

/// Replace only parsed link destinations and reference definitions. Never rewrite prose/code.
pub fn rewrite_markdown(markdown: &str, replacements: &BTreeMap<String, String>) -> Result<String> {
    let parser = Parser::new_ext(markdown, Options::empty());
    let mut edits: BTreeMap<usize, (usize, String)> = BTreeMap::new();
    for (_, def) in parser.reference_definitions().iter() {
        if let Some(new) = replacements.get(def.dest.as_ref()) {
            let raw = &markdown[def.span.clone()];
            // The destination follows the definition's "]:" delimiter; preserve label and title.
            let start = raw.find("]:").context("unsupported reference definition")? + 2;
            let relative = raw[start..].find(def.dest.as_ref()).context(
                "escaped reference destinations must be written literally before promotion",
            )? + start;
            edits.insert(
                def.span.start + relative,
                (def.span.start + relative + def.dest.len(), new.clone()),
            );
        }
    }
    for (event, range) in parser.into_offset_iter() {
        if let Event::Start(
            Tag::Link {
                dest_url,
                link_type,
                ..
            }
            | Tag::Image {
                dest_url,
                link_type,
                ..
            },
        ) = event
        {
            if link_type != LinkType::Inline && link_type != LinkType::Autolink {
                continue;
            }
            if let Some(new) = replacements.get(dest_url.as_ref()) {
                let raw = &markdown[range.clone()];
                let offset = if link_type == LinkType::Autolink {
                    0
                } else {
                    raw.rfind("](").context("unsupported inline link syntax")? + 2
                };
                let relative = raw[offset..].find(dest_url.as_ref()).context(
                    "escaped link destinations must be written literally before promotion",
                )? + offset;
                edits.insert(
                    range.start + relative,
                    (range.start + relative + dest_url.len(), new.clone()),
                );
            }
        }
    }
    let mut result = markdown.to_string();
    for (start, (end, new)) in edits.into_iter().rev() {
        result.replace_range(start..end, &new);
    }
    Ok(result)
}

#[derive(Debug, Clone, Serialize)]
pub struct PromotionReport {
    pub applied: bool,
    pub artifacts: Vec<PromotedAsset>,
    pub removed: Vec<String>,
}
#[derive(Debug, Clone, Serialize)]
pub struct PromotedAsset {
    pub entry: String,
    pub artifact: String,
    pub object: DesiredObject,
}

/// Stage and validate everything before any upload. Upload everything before rewriting sources.
/// A durable transaction journal makes interrupted source rewrites recoverable without reuploading.
pub fn promote(
    root: &Path,
    entry_id: Option<&str>,
    apply: bool,
    store: &mut dyn ObjectStore,
) -> Result<PromotionReport> {
    let _lock = RepoLock::acquire(root)?;
    ensure!(
        !root.join("catalogue/.promotion/transaction.json").exists(),
        "unfinished promotion; run assets recover before starting another"
    );
    let c = catalogue::load(root, Mode::Preview)?;
    if let Some(id) = entry_id {
        ensure!(
            c.documents.iter().any(|d| d.entry.id == id),
            "unknown entry {id}"
        );
    }
    let staging = tempfile::tempdir()?;
    let mut files = BTreeMap::<String, PathBuf>::new();
    let mut updated = c.clone();
    let mut report = PromotionReport {
        applied: apply,
        artifacts: Vec::new(),
        removed: Vec::new(),
    };
    let mut incoming: BTreeMap<String, (PathBuf, Inspection)> = BTreeMap::new();
    for d in &mut updated.documents {
        if entry_id.is_some_and(|id| d.entry.id != id) {
            continue;
        }
        let mut replacements = BTreeMap::new();
        for a in &mut d.entry.artifacts {
            if matches!(
                a.source,
                AssetSource::Sha256 { .. } | AssetSource::External { .. }
            ) {
                continue;
            }
            ensure!(
                a.provenance.redistribution == Redistribution::Permitted,
                "{}:{}: promotion requires permitted redistribution with evidence",
                d.entry.id,
                a.id
            );
            let old_url = catalogue::artifact_url(&c.config, a);
            let mut temp = tempfile::NamedTempFile::new_in(staging.path())?;
            let inspection = read_asset(root, &c.config, a, &mut temp)
                .with_context(|| format!("{}:{}", d.entry.id, a.id))?;
            let local = match &a.source {
                AssetSource::Incoming { path } => {
                    Some((path.clone(), validate::safe_file(root, path)?))
                }
                _ => None,
            };
            if let Some((relative, path)) = local {
                incoming.insert(relative, (path, inspection.clone()));
            }
            let key = object_key(&inspection.sha256, a.format);
            let staged = staging
                .path()
                .join(format!("{}.{}", inspection.sha256, a.format.ext()));
            if !files.contains_key(&key) {
                temp.persist(&staged).map_err(|e| e.error)?;
                files.insert(key.clone(), staged);
            }
            let object = DesiredObject {
                key,
                sha256: inspection.sha256.clone(),
                size_bytes: inspection.size_bytes,
                content_type: a.format.mime().into(),
                references: vec![format!("{}:{}", d.entry.id, a.id)],
            };
            a.source = AssetSource::Sha256 {
                sha256: inspection.sha256,
                size_bytes: inspection.size_bytes,
            };
            replacements.insert(old_url, catalogue::artifact_url(&c.config, a));
            report.artifacts.push(PromotedAsset {
                entry: d.entry.id.clone(),
                artifact: a.id.clone(),
                object,
            });
        }
        d.markdown = rewrite_markdown(&d.markdown, &replacements)?;
    }
    catalogue::validate_catalogue(&updated)?;
    let mut edits = Vec::new();
    for (old, new) in c.documents.iter().zip(&updated.documents) {
        if old.entry != new.entry || old.markdown != new.markdown {
            edits.push(EntryEdit {
                id: old.entry.id.clone(),
                before: old.original.clone(),
                after: catalogue::serialize_entry_document(&updated, new)?,
            });
        }
    }
    if !apply {
        return Ok(report);
    }
    let mut uploaded = BTreeSet::new();
    for a in &report.artifacts {
        if uploaded.insert(&a.object.key) {
            store.put_if_absent(&a.object, &files[&a.object.key])?;
        }
    }
    // Detect changes to the complete input set during network I/O.
    let fresh = catalogue::load(root, Mode::Preview)?;
    ensure!(
        fresh.config == c.config
            && fresh.documents.len() == c.documents.len()
            && fresh.plugin_documents.len() == c.plugin_documents.len()
            && fresh
                .documents
                .iter()
                .zip(&c.documents)
                .all(|(a, b)| a.original == b.original)
            && fresh
                .plugin_documents
                .iter()
                .zip(&c.plugin_documents)
                .all(|(a, b)| a.original == b.original),
        "catalogue changed during promotion; retry"
    );
    for (path, inspection) in incoming.values() {
        ensure!(
            &hash_file(path)? == inspection,
            "incoming asset changed during promotion; retry"
        );
    }
    if !edits.is_empty() {
        let transaction = PromotionTransaction {
            schema_version: 1,
            config: c.config.clone(),
            edits,
            removals: incoming
                .into_iter()
                .map(|(path, (_, inspection))| Removal { path, inspection })
                .collect(),
        };
        write_journal(root, &transaction)?;
        report.removed = finish_transaction(root, &transaction)?;
    }
    Ok(report)
}

pub struct NoUpload;
impl ObjectStore for NoUpload {
    fn put_if_absent(&mut self, _: &DesiredObject, _: &Path) -> Result<()> {
        bail!("no upload backend configured")
    }
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct EntryEdit {
    id: String,
    before: String,
    after: String,
}
#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Removal {
    path: String,
    inspection: Inspection,
}
#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct PromotionTransaction {
    schema_version: u32,
    config: Config,
    edits: Vec<EntryEdit>,
    removals: Vec<Removal>,
}

fn write_journal(root: &Path, transaction: &PromotionTransaction) -> Result<()> {
    let dir = root.join("catalogue/.promotion");
    if dir.exists() {
        ensure!(
            !fs::symlink_metadata(&dir)?.file_type().is_symlink(),
            "promotion directory must not be a symlink"
        );
    } else {
        fs::create_dir(&dir)?;
    }
    catalogue::atomic_write(
        &dir.join("transaction.json"),
        &catalogue::json_bytes(transaction)?,
    )
}

/// Finish an interrupted, already-uploaded promotion. No network access is needed.
/// Refuse recovery if any input has been edited since the journal was recorded.
pub fn recover(root: &Path) -> Result<Vec<String>> {
    let _lock = RepoLock::acquire(root)?;
    let path = validate::safe_file(root, "catalogue/.promotion/transaction.json")?;
    let transaction: PromotionTransaction = serde_json::from_slice(&fs::read(path)?)?;
    finish_transaction(root, &transaction)
}

fn finish_transaction(root: &Path, transaction: &PromotionTransaction) -> Result<Vec<String>> {
    ensure!(
        transaction.schema_version == 1,
        "unsupported promotion journal"
    );
    let config = Config::default();
    ensure!(
        config == transaction.config,
        "configuration changed; restore it before recovery"
    );
    let mut replacements = BTreeMap::new();
    let mut removable = BTreeSet::new();
    for edit in &transaction.edits {
        validate::slug(&edit.id)?;
        let path = validate::safe_file(root, &format!("catalogue/{}.md", edit.id))?;
        let actual = fs::read_to_string(path)?;
        ensure!(
            actual == edit.before || actual == edit.after,
            "{} changed; restore its journal version before recovery",
            edit.id
        );
        let (before, _) = catalogue::parse_document(&edit.before)?;
        let (after, _) = catalogue::parse_document(&edit.after)?;
        validate::entry(&before)?;
        validate::entry(&after)?;
        ensure!(
            before.id == edit.id && after.id == edit.id,
            "journal ID mismatch"
        );
        for a in before.artifacts {
            if let AssetSource::Incoming { path } = a.source {
                removable.insert(path);
            }
        }
        ensure!(
            replacements.insert(edit.id.clone(), &edit.after).is_none(),
            "duplicate journal entry"
        );
    }
    let mut documents = Vec::new();
    for item in fs::read_dir(root.join("catalogue"))? {
        let path = item?.path();
        if !validate::is_entry_file(&path)? {
            continue;
        }
        let name = path
            .file_name()
            .and_then(|n| n.to_str())
            .context("invalid entry filename")?;
        let path = validate::safe_file(root, &format!("catalogue/{name}"))?;
        let original = fs::read_to_string(&path)?;
        let (current, _) = catalogue::parse_document(&original)?;
        let proposed = replacements.get(&current.id).copied().unwrap_or(&original);
        let (entry, markdown) = catalogue::parse_document(proposed)?;
        ensure!(
            name == format!("{}.md", entry.id),
            "filename must match entry ID during recovery"
        );
        documents.push(catalogue::Document {
            entry,
            markdown,
            original: proposed.clone(),
            path,
        });
    }
    let plugin_documents = catalogue::load_plugins(root, &mut documents)?;
    let proposed = Catalogue {
        config,
        documents,
        plugin_documents,
        root: root.into(),
    };
    catalogue::validate_catalogue(&proposed)?;
    for removal in &transaction.removals {
        ensure!(
            removable.contains(&removal.path),
            "journal removal was not an incoming artifact"
        );
        ensure!(
            !proposed
                .documents
                .iter()
                .flat_map(|d| &d.entry.artifacts)
                .any(|a| matches!(&a.source,AssetSource::Incoming{path} if path == &removal.path)),
            "incoming file still has a reference"
        );
        let path = root.join(&removal.path);
        if fs::symlink_metadata(&path).is_ok() {
            let path = validate::safe_file(root, &removal.path)?;
            ensure!(
                hash_file(&path)? == removal.inspection,
                "incoming file changed; restore it before recovery"
            );
        }
    }
    // All proposed entries are verified before the first rewrite, and all rewrites finish before cleanup.
    for edit in &transaction.edits {
        catalogue::atomic_write(
            &root.join(format!("catalogue/{}.md", edit.id)),
            edit.after.as_bytes(),
        )?;
    }
    let mut removed = Vec::new();
    for removal in &transaction.removals {
        let path = root.join(&removal.path);
        if path.exists() {
            fs::remove_file(path)?;
        }
        removed.push(removal.path.clone());
    }
    fs::remove_file(root.join("catalogue/.promotion/transaction.json"))?;
    Ok(removed)
}

/// Enforce agreement between a store request and its content-addressed key.
pub fn validate_object(object: &DesiredObject) -> Result<()> {
    validate::sha256(&object.sha256)?;
    ensure!(is_managed_key(&object.key), "unmanaged object key");
    let expected_file = object.key.rsplit('/').next().unwrap();
    ensure!(
        expected_file
            .split_once('.')
            .is_some_and(|(hash, _)| hash == object.sha256),
        "object key and SHA-256 disagree"
    );
    ensure!(object.size_bytes > 0, "empty object");
    Ok(())
}

/// Read an artifact into staging and validate the same bytes for fetch and promotion.
fn read_asset(
    root: &Path,
    config: &Config,
    asset: &Artifact,
    staged: &mut tempfile::NamedTempFile,
) -> Result<Inspection> {
    match &asset.source {
        AssetSource::Incoming { path } => {
            let source = validate::safe_file(root, path)?;
            let limit = incoming_limit(asset.format);
            std::io::copy(&mut File::open(source)?.take(limit + 1), staged)?;
            ensure!(
                staged.as_file().metadata()?.len() <= limit,
                "incoming file exceeds its {} byte limit",
                limit
            );
        }
        AssetSource::Url {
            url, download_page, ..
        } => {
            if let Some(page) = download_page {
                network::download_via_page(url, page, staged, asset.format.limit())?;
            } else {
                network::download(url, staged, asset.format.limit())?;
            }
        }
        AssetSource::Sha256 { .. } => {
            network::download(
                &catalogue::artifact_url(config, asset),
                staged,
                asset.format.limit(),
            )?;
        }
        AssetSource::External { .. } => bail!("external links are not managed downloads"),
    }
    staged.flush()?;
    let inspection = inspect(staged.path(), asset.format)?;
    verify_integrity(asset, &inspection)?;
    Ok(inspection)
}

/// Check declared integrity assertions without treating a checksum as redistribution permission.
pub fn verify_integrity(asset: &Artifact, actual: &Inspection) -> Result<()> {
    let (hash, size) = match &asset.source {
        AssetSource::Url {
            expected_sha256,
            expected_size,
            ..
        } => (expected_sha256.as_deref(), *expected_size),
        AssetSource::Sha256 { sha256, size_bytes } => (Some(sha256.as_str()), Some(*size_bytes)),
        _ => (None, None),
    };
    ensure!(
        hash.is_none_or(|h| h == actual.sha256),
        "download SHA-256 mismatch"
    );
    ensure!(
        size.is_none_or(|s| s == actual.size_bytes),
        "download size mismatch"
    );
    Ok(())
}

#[derive(Debug, Clone, Serialize)]
pub struct FetchedAsset {
    pub entry: String,
    pub artifact: String,
    pub path: PathBuf,
    pub sha256: String,
    pub size_bytes: u64,
}

/// Download hosted/pending artifacts to a local content-addressed cache. No R2 access,
/// source rewrites, cleanup or redistribution approval is involved. External links are skipped.
pub fn fetch(root: &Path, entry_id: Option<&str>, directory: &Path) -> Result<Vec<FetchedAsset>> {
    let c = catalogue::load(root, Mode::Preview)?;
    if let Some(id) = entry_id {
        ensure!(
            c.documents.iter().any(|d| d.entry.id == id),
            "unknown entry {id}"
        );
    }
    let mut cache = DirectoryStore {
        root: directory.to_path_buf(),
    };
    let mut report = Vec::new();
    for doc in &c.documents {
        if entry_id.is_some_and(|id| doc.entry.id != id) {
            continue;
        }
        for asset in &doc.entry.artifacts {
            if matches!(asset.source, AssetSource::External { .. }) {
                continue;
            }
            let mut staged = tempfile::NamedTempFile::new()?;
            let actual = read_asset(root, &c.config, asset, &mut staged)
                .with_context(|| format!("{}:{}", doc.entry.id, asset.id))?;
            let object = DesiredObject {
                key: object_key(&actual.sha256, asset.format),
                sha256: actual.sha256.clone(),
                size_bytes: actual.size_bytes,
                content_type: asset.format.mime().into(),
                references: vec![format!("{}:{}", doc.entry.id, asset.id)],
            };
            cache.put_if_absent(&object, staged.path())?;
            report.push(FetchedAsset {
                entry: doc.entry.id.clone(),
                artifact: asset.id.clone(),
                path: directory.canonicalize()?.join(object.key),
                sha256: actual.sha256,
                size_bytes: actual.size_bytes,
            });
        }
    }
    Ok(report)
}
