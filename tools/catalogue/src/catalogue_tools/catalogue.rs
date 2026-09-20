use crate::catalogue_tools::{assets, community, model::*, validate};
use anyhow::{ensure, Context, Result};
use pulldown_cmark::{CowStr, Event, Options, Parser, Tag};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs::{self, File},
    io::{Read, Write},
    path::{Path, PathBuf},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Preview,
    NoIncoming,
    Production,
}
#[derive(Debug, Clone)]
pub struct Document {
    pub entry: Entry,
    pub markdown: String,
    pub original: String,
    pub path: PathBuf,
}
#[derive(Debug, Clone)]
pub struct PluginDocument {
    pub collection: PluginCollection,
    pub original: String,
    pub path: PathBuf,
}
#[derive(Debug, Clone)]
pub struct Catalogue {
    pub config: Config,
    pub documents: Vec<Document>,
    pub plugin_documents: Vec<PluginDocument>,
    pub root: PathBuf,
}

pub fn parse_document(text: &str) -> Result<(Entry, String)> {
    ensure!(
        text.starts_with("---\n") || text.starts_with("---\r\n"),
        "entry must start with a YAML front matter delimiter"
    );
    let start = text.find('\n').unwrap() + 1;
    let mut offset = start;
    for line in text[start..].split_inclusive('\n') {
        if line.trim_end_matches(['\r', '\n']) == "---" {
            let entry: Entry = crate::catalogue_tools::yaml::from_str(&text[start..offset])
                .context("invalid typed YAML front matter")?;
            return Ok((entry, text[offset + line.len()..].to_string()));
        }
        offset += line.len();
    }
    anyhow::bail!("missing closing YAML front matter delimiter")
}

pub fn serialize_document(entry: &Entry, markdown: &str) -> Result<String> {
    Ok(format!(
        "---\n{}---\n{}",
        serde_saphyr::to_string(entry)?,
        markdown
    ))
}

pub fn serialize_entry_document(catalogue: &Catalogue, document: &Document) -> Result<String> {
    let mut entry = document.entry.clone();
    let plugin_ids: BTreeSet<_> = catalogue
        .plugin_documents
        .iter()
        .filter(|d| d.collection.entry == entry.id)
        .flat_map(|d| d.collection.plugins.iter().map(|p| &p.id))
        .collect();
    let artifact_ids: BTreeSet<_> = catalogue
        .plugin_documents
        .iter()
        .filter(|d| d.collection.entry == entry.id)
        .flat_map(|d| d.collection.artifacts.iter().map(|a| &a.id))
        .collect();
    entry.plugins.retain(|p| !plugin_ids.contains(&p.id));
    entry.artifacts.retain(|a| !artifact_ids.contains(&a.id));
    serialize_document(&entry, &document.markdown)
}

pub fn load(root: &Path, mode: Mode) -> Result<Catalogue> {
    ensure!(
        !root.join("catalogue/.promotion/transaction.json").exists(),
        "unfinished promotion; run assets recover before checking or building"
    );
    let config = Config::default();
    validate::config(&config)?;
    let entries = root.join("catalogue");
    ensure!(
        !fs::symlink_metadata(&entries)?.file_type().is_symlink(),
        "catalogue must not be a symlink"
    );
    let mut paths = fs::read_dir(&entries)?
        .map(|p| p.map(|p| p.path()))
        .collect::<std::io::Result<Vec<_>>>()?;
    paths.sort();
    let mut documents = Vec::new();
    for path in paths {
        if !validate::is_entry_file(&path)? {
            continue;
        }
        let filename = path
            .file_name()
            .and_then(|p| p.to_str())
            .context("non UTF-8 entry filename")?;
        let path = validate::safe_file(root, &format!("catalogue/{filename}"))?;
        let original = read_text(&path, 8 * 1024 * 1024)?;
        let (entry, markdown) =
            parse_document(&original).with_context(|| path.display().to_string())?;
        ensure!(
            entry.plugins.is_empty(),
            "{}: plugins must be declared in plugins/*.yaml",
            path.display()
        );
        ensure!(
            filename == format!("{}.md", entry.id),
            "filename must match entry ID: {filename}"
        );
        for a in &entry.artifacts {
            match &a.source {
                AssetSource::Incoming { path } => {
                    ensure!(
                        mode == Mode::Preview,
                        "{}: incoming assets are forbidden in this mode",
                        entry.id
                    );
                    let file = validate::safe_file(root, path)?;
                    ensure!(
                        file.metadata()?.len() <= assets::incoming_limit(a.format),
                        "incoming file exceeds its {} byte limit",
                        assets::incoming_limit(a.format)
                    );
                    assets::inspect(&file, a.format)?;
                }
                AssetSource::Url { .. } => ensure!(
                    mode != Mode::Production,
                    "{}: pending URL artifact {} must be promoted before production",
                    entry.id,
                    a.id
                ),
                _ => {}
            }
        }
        validate_markdown(&config, &entry, &markdown)
            .with_context(|| format!("{} Markdown", entry.id))?;
        documents.push(Document {
            entry,
            markdown,
            original,
            path,
        });
    }
    let plugin_documents = load_plugins(root, &mut documents)?;
    let catalogue = Catalogue {
        config,
        documents,
        plugin_documents,
        root: root.canonicalize()?,
    };
    validate_catalogue(&catalogue)?;
    validate_incoming(&catalogue, mode)?;
    Ok(catalogue)
}

pub(crate) fn load_plugins(root: &Path, documents: &mut [Document]) -> Result<Vec<PluginDocument>> {
    let directory = root.join("plugins");
    if !directory.exists() && fs::symlink_metadata(&directory).is_err() {
        return Ok(Vec::new());
    }
    ensure!(
        !fs::symlink_metadata(&directory)?.file_type().is_symlink() && directory.is_dir(),
        "plugins must be a real directory"
    );
    let mut paths = fs::read_dir(&directory)?
        .map(|p| p.map(|p| p.path()))
        .collect::<std::io::Result<Vec<_>>>()?;
    paths.sort();
    let mut plugin_documents = Vec::new();
    for candidate in paths {
        let name = candidate
            .file_name()
            .and_then(|p| p.to_str())
            .context("non UTF-8 path in plugins")?;
        if name == ".gitkeep" {
            let metadata = fs::symlink_metadata(&candidate)?;
            ensure!(
                metadata.is_file() && metadata.len() == 0,
                "plugins/.gitkeep must be an empty regular file"
            );
            continue;
        }
        ensure!(
            candidate
                .extension()
                .is_some_and(|extension| extension == "yaml"),
            "plugins/ may only contain <chunk-id>.yaml files and an empty .gitkeep: {}",
            candidate.display()
        );
        let stem = candidate
            .file_stem()
            .and_then(|p| p.to_str())
            .context("non UTF-8 plugin filename")?;
        validate::slug(stem)?;
        let path = validate::safe_file(root, &format!("plugins/{name}"))?;
        let original = read_text(&path, 8 * 1024 * 1024)?;
        let collection: PluginCollection = crate::catalogue_tools::yaml::from_str(&original)
            .with_context(|| path.display().to_string())?;
        ensure!(
            collection.schema_version == SCHEMA_VERSION,
            "{}: unsupported schema_version {}",
            path.display(),
            collection.schema_version
        );
        validate::slug(&collection.entry)?;
        ensure!(
            !collection.plugins.is_empty(),
            "{}: plugin collection must not be empty",
            path.display()
        );
        for artifact in &collection.artifacts {
            ensure!(
                artifact.role == ArtifactRole::Supplement
                    && matches!(artifact.source, AssetSource::External { .. }),
                "{}: plugin artifacts must be external supplements",
                path.display()
            );
        }
        let entry = documents
            .iter_mut()
            .find(|d| d.entry.id == collection.entry)
            .with_context(|| format!("{}: unknown entry {}", path.display(), collection.entry))?;
        entry.entry.artifacts.extend(collection.artifacts.clone());
        entry.entry.plugins.extend(collection.plugins.clone());
        plugin_documents.push(PluginDocument {
            collection,
            original,
            path,
        });
    }
    Ok(plugin_documents)
}

pub fn validate_catalogue(c: &Catalogue) -> Result<()> {
    validate::config(&c.config)?;
    let mut routes = BTreeSet::new();
    let mut ids = BTreeSet::new();
    let mut hashes = BTreeMap::new();
    for d in &c.documents {
        let e = &d.entry;
        validate::entry(e)?;
        validate_markdown(&c.config, e, &d.markdown)?;
        ensure!(ids.insert(&e.id), "duplicate entry ID {}", e.id);
        for route in std::iter::once(community::canonical_path(e)).chain(e.aliases.clone()) {
            ensure!(
                routes.insert(route.clone()),
                "duplicate route or alias {route}"
            );
        }
        for a in &e.artifacts {
            if let AssetSource::Sha256 { sha256, size_bytes } = &a.source {
                let identity = (a.format, *size_bytes);
                if let Some(previous) = hashes.insert(sha256, identity) {
                    ensure!(
                        previous == identity,
                        "inconsistent format or size for shared SHA-256 {sha256}"
                    );
                }
            }
        }
    }
    Ok(())
}

fn validate_incoming(c: &Catalogue, mode: Mode) -> Result<()> {
    let incoming = c.root.join("catalogue/incoming");
    if !incoming.exists() && fs::symlink_metadata(&incoming).is_err() {
        return Ok(());
    }
    let marker = incoming.join(".gitkeep");
    let mut queue = vec![incoming];
    let declared: BTreeSet<_> = c
        .documents
        .iter()
        .flat_map(|d| &d.entry.artifacts)
        .filter_map(|a| match &a.source {
            AssetSource::Incoming { path } => Some(c.root.join(path)),
            _ => None,
        })
        .collect();
    while let Some(path) = queue.pop() {
        let meta = fs::symlink_metadata(&path)?;
        ensure!(
            !meta.file_type().is_symlink(),
            "incoming symlinks are forbidden"
        );
        if meta.is_dir() {
            for p in fs::read_dir(path)? {
                queue.push(p?.path());
            }
        } else {
            if path == marker {
                ensure!(
                    meta.is_file() && meta.len() == 0,
                    "catalogue/incoming/.gitkeep must be an empty regular file"
                );
                continue;
            }
            ensure!(
                mode == Mode::Preview,
                "incoming files are forbidden on the default branch: {}",
                path.display()
            );
            ensure!(
                meta.is_file() && declared.contains(&path),
                "unreferenced incoming file: {}",
                path.display()
            );
        }
    }
    Ok(())
}

pub fn artifact_url(config: &Config, artifact: &Artifact) -> String {
    match &artifact.source {
        AssetSource::Incoming { path } => {
            path.strip_prefix("catalogue/").unwrap_or(path).to_string()
        }
        AssetSource::Url { url, .. } | AssetSource::External { url } => url.clone(),
        AssetSource::Sha256 { sha256, .. } => format!(
            "{}/{}",
            config.asset_base_url.trim_end_matches('/'),
            assets::object_key(sha256, artifact.format)
        ),
    }
}

fn registered<'a>(config: &Config, entry: &'a Entry, url: &str) -> Option<&'a Artifact> {
    entry
        .artifacts
        .iter()
        .find(|a| artifact_url(config, a) == url)
}

fn validate_markdown(config: &Config, entry: &Entry, markdown: &str) -> Result<()> {
    let events: Vec<_> = Parser::new_ext(markdown, Options::empty()).collect();
    for (start, event) in events.iter().enumerate() {
        let Event::Start(Tag::Heading { .. }) = event else {
            continue;
        };
        let title = events[start + 1..]
            .iter()
            .take_while(|event| !matches!(event, Event::End(pulldown_cmark::TagEnd::Heading(_))))
            .filter_map(|event| match event {
                Event::Text(text) | Event::Code(text) => Some(text.as_ref()),
                _ => None,
            })
            .collect::<String>();
        ensure!(
            !title.trim().eq_ignore_ascii_case("compatibility"),
            "compatibility reports belong in labelled Systemless issues, not catalogue prose"
        );
    }
    for event in events {
        match event {
            Event::Start(Tag::Image { dest_url, .. }) => {
                ensure!(
                    registered(config, entry, &dest_url).is_some_and(|a| a.format.media()),
                    "image {dest_url:?} must reference a declared image artifact using its exact URL"
                );
            }
            Event::Start(Tag::Link { dest_url, .. }) => {
                let url = dest_url.as_ref();
                if url.starts_with("incoming/")
                    || url.contains("/catalogue/objects/sha256/")
                    || url.contains("/catalogue/media/sha256/")
                {
                    ensure!(
                        registered(config, entry, url).is_some(),
                        "managed link must reference a declared artifact: {url}"
                    );
                } else if url.starts_with('#') { /* CommonMark anchor */
                } else if url.starts_with("https://") {
                    validate::https(url.split('#').next().unwrap())?;
                } else {
                    ensure!(
                        !url.contains(':') && !url.starts_with('/'),
                        "links must use HTTPS, anchors, or repository-relative paths"
                    );
                    validate::relative_path(
                        url.strip_prefix("../")
                            .unwrap_or(url)
                            .split('#')
                            .next()
                            .unwrap(),
                    )?;
                }
            }
            _ => {}
        }
    }
    Ok(())
}

fn rendered_url(config: &Config, entry: &Entry, url: &str) -> String {
    if let Some(a) = registered(config, entry, url) {
        if let AssetSource::Incoming { path } = &a.source {
            return format!(
                "{}/raw/{}/{}",
                config.repository.trim_end_matches('/'),
                config.branch,
                path
            );
        }
        return artifact_url(config, a);
    }
    if url.starts_with("https://") || url.starts_with('#') {
        return url.to_string();
    }
    let base = format!(
        "{}/blob/{}/catalogue/{}.md",
        config.repository.trim_end_matches('/'),
        config.branch,
        entry.id
    );
    url::Url::parse(&base)
        .and_then(|base| base.join(url))
        .map(|u| u.to_string())
        .unwrap_or_default()
}

pub fn render_markdown(config: &Config, entry: &Entry, markdown: &str) -> String {
    render_events(config, entry, Parser::new_ext(markdown, Options::empty()))
}

fn render_events<'a>(
    config: &Config,
    entry: &Entry,
    events: impl Iterator<Item = Event<'a>>,
) -> String {
    let events = events.map(|event| match event {
        // Preserve CommonMark source while making raw HTML inert in the public renderer.
        Event::Html(text) | Event::InlineHtml(text) => Event::Text(text),
        Event::Start(Tag::Image {
            link_type,
            dest_url,
            title,
            id,
        }) => Event::Start(Tag::Image {
            link_type,
            dest_url: CowStr::from(rendered_url(config, entry, &dest_url)),
            title,
            id,
        }),
        Event::Start(Tag::Link {
            link_type,
            dest_url,
            title,
            id,
        }) => Event::Start(Tag::Link {
            link_type,
            dest_url: CowStr::from(rendered_url(config, entry, &dest_url)),
            title,
            id,
        }),
        other => other,
    });
    let mut html = String::new();
    pulldown_cmark::html::push_html(&mut html, events);
    html
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CompiledCatalogue {
    pub schema_version: u32,
    pub source_sha256: String,
    pub community: community::CatalogueCommunityLinks,
    pub entries: Vec<CompiledEntry>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CompiledEntry {
    pub id: String,
    pub kind: Kind,
    pub title: String,
    pub summary: String,
    pub developer: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub publisher: Option<String>,
    pub year: u16,
    pub architectures: Vec<Architecture>,
    pub default_architecture: Architecture,
    pub category: String,
    pub path: String,
    pub aliases: Vec<String>,
    pub launch_enabled: bool,
    pub compatibility: Compatibility,
    pub runtime: Runtime,
    pub controls: Controls,
    pub references: Vec<String>,
    pub assets: Vec<CompiledAsset>,
    #[serde(default)]
    pub plugins: Vec<Plugin>,
    pub content_html: String,
    pub community: community::CommunityLinks,
}

/// Resolved public asset information, independent of ingestion and R2 planning.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CompiledAsset {
    pub id: String,
    pub role: ArtifactRole,
    pub format: FileType,
    pub url: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sha256: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub size_bytes: Option<u64>,
    pub rights: Provenance,
}

pub fn build(c: &Catalogue) -> Result<CompiledCatalogue> {
    validate_catalogue(c)?;
    let mut docs: Vec<_> = c.documents.iter().collect();
    docs.sort_by_key(|d| &d.entry.id);
    let canonical = serde_json::to_vec(&(
        &c.config,
        docs.iter()
            .map(|d| (&d.entry, &d.markdown))
            .collect::<Vec<_>>(),
    ))?;
    let entries = docs
        .into_iter()
        .map(|d| {
            let e = &d.entry;
            CompiledEntry {
                id: e.id.clone(),
                kind: e.kind,
                title: e.title.clone(),
                summary: e.summary.clone(),
                developer: e.developer.clone(),
                publisher: e.publisher.clone(),
                year: e.year,
                architectures: e.architectures.clone(),
                default_architecture: e.default_architecture,
                category: e.category.clone(),
                path: community::canonical_path(e),
                aliases: e.aliases.clone(),
                launch_enabled: e.launch_enabled,
                compatibility: e.compatibility.clone(),
                runtime: e.runtime.clone(),
                plugins: e.plugins.clone(),
                controls: e.controls.clone(),
                references: e.references.clone(),
                content_html: render_markdown(&c.config, e, &d.markdown),
                community: community::links(&c.config, e),
                assets: e
                    .artifacts
                    .iter()
                    .map(|a| {
                        let (sha256, size_bytes) = match &a.source {
                            AssetSource::Sha256 { sha256, size_bytes } => {
                                (Some(sha256.clone()), Some(*size_bytes))
                            }
                            _ => (None, None),
                        };
                        CompiledAsset {
                            id: a.id.clone(),
                            role: a.role,
                            format: a.format,
                            url: rendered_url(&c.config, e, &artifact_url(&c.config, a)),
                            sha256,
                            size_bytes,
                            rights: a.provenance.clone(),
                        }
                    })
                    .collect(),
            }
        })
        .collect();
    Ok(CompiledCatalogue {
        schema_version: COMPILED_SCHEMA_VERSION,
        source_sha256: hex::encode(Sha256::digest(canonical)),
        community: community::catalogue_links(&c.config),
        entries,
    })
}

pub fn json_bytes(value: &impl Serialize) -> Result<Vec<u8>> {
    let mut bytes = serde_json::to_vec_pretty(value)?;
    bytes.push(b'\n');
    Ok(bytes)
}
/// Atomic, durable single-file replacement in the destination filesystem.
pub fn atomic_write(path: &Path, bytes: &[u8]) -> Result<()> {
    let parent = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    fs::create_dir_all(parent)?;
    let mut temp = tempfile::NamedTempFile::new_in(parent)?;
    temp.write_all(bytes)?;
    temp.as_file().sync_all()?;
    temp.persist(path).map_err(|e| e.error)?;
    File::open(parent)?.sync_all()?;
    Ok(())
}

#[derive(Debug, Serialize)]
pub struct Diff {
    pub added: Vec<String>,
    pub removed: Vec<String>,
    pub changed: BTreeMap<String, Vec<String>>,
}
pub fn diff(before: &CompiledCatalogue, after: &CompiledCatalogue) -> Diff {
    let a: BTreeMap<_, _> = before.entries.iter().map(|e| (&e.id, e)).collect();
    let b: BTreeMap<_, _> = after.entries.iter().map(|e| (&e.id, e)).collect();
    let mut result = Diff {
        added: Vec::new(),
        removed: Vec::new(),
        changed: BTreeMap::new(),
    };
    for id in a.keys() {
        if !b.contains_key(id) {
            result.removed.push((*id).clone());
        }
    }
    for (id, entry) in &b {
        match a.get(id) {
            None => result.added.push((*id).clone()),
            Some(old) if old != entry => {
                let old = serde_json::to_value(old).expect("serializable compiled entry");
                let new = serde_json::to_value(entry).expect("serializable compiled entry");
                let fields = new
                    .as_object()
                    .unwrap()
                    .iter()
                    .filter(|(k, v)| old.get(*k) != Some(*v))
                    .map(|(k, _)| k.clone())
                    .collect();
                result.changed.insert((*id).clone(), fields);
            }
            _ => {}
        }
    }
    result
}

/// Bound file reads before allocating input buffers, including concurrently growing files.
pub fn read_text(path: &Path, limit: u64) -> Result<String> {
    let file = File::open(path)?;
    ensure!(
        file.metadata()?.len() <= limit,
        "{} exceeds its {limit}-byte input limit",
        path.display()
    );
    let mut text = String::new();
    file.take(limit + 1).read_to_string(&mut text)?;
    ensure!(text.len() as u64 <= limit, "input grew beyond size limit");
    Ok(text)
}
