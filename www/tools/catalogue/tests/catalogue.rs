mod common;

#[test]
fn download_pages_resolve_only_the_pinned_file() {
    use systemless_catalogue_tools::catalogue_tools::network::resolve_download_link;
    let page = "https://example.org/game";
    let target = "https://files.example.org/game.sit";
    let html = r#"<a href="//files.example.org/other.sit">Other</a>
      <a href="https://attacker.example/game.sit">Wrong host</a>
      <a href="//files.example.org/game.sit?expires=123&amp;token=abc">Download</a>
      <a href="//files.example.org/game.sit?expires=123&amp;token=abc">Duplicate</a>"#;
    assert_eq!(
        resolve_download_link(page, target, html).unwrap(),
        "https://files.example.org/game.sit?expires=123&token=abc"
    );
    for html in [
        "<a href='http://files.example.org/game.sit'>Insecure</a>",
        "<a href='https://user:secret@files.example.org/game.sit'>Credentials</a>",
        "<a href='//files.example.org/game.sit?a'>A</a><a href='//files.example.org/game.sit?b'>B</a>",
        "<script>const url = 'https://files.example.org/game.sit';</script>",
    ] {
        assert!(resolve_download_link(page, target, html).is_err());
    }
    assert!(resolve_download_link(page, target, &"x".repeat(512 * 1024 + 1)).is_err());
    let mut entry = common::catalogue().documents.pop().unwrap().entry;
    entry.artifacts[0].source = AssetSource::Url {
        url: target.into(),
        download_page: Some(page.into()),
        expected_sha256: None,
        expected_size: None,
    };
    assert!(validate::entry(&entry).is_err());
    if let AssetSource::Url {
        expected_sha256,
        expected_size,
        ..
    } = &mut entry.artifacts[0].source
    {
        *expected_sha256 = Some("a".repeat(64));
        *expected_size = Some(123);
    }
    validate::entry(&entry).unwrap();
    if let AssetSource::Url { download_page, .. } = &mut entry.artifacts[0].source {
        *download_page = Some("https://127.0.0.1/private".into());
    }
    assert!(validate::entry(&entry).is_err());
}
use std::{collections::BTreeMap, fs, path::Path};
use systemless_catalogue_tools::catalogue_tools::{
    assets::{self, ObjectStore},
    catalogue::{self, CompiledCatalogue},
    r2, validate, *,
};

fn repo() -> tempfile::TempDir {
    let root = tempfile::tempdir().unwrap();
    fs::create_dir(root.path().join("catalogue")).unwrap();
    fs::write(root.path().join("catalogue/.gitkeep"), b"").unwrap();
    root
}
fn save(root: &Path, e: &Entry, markdown: &str) {
    fs::write(
        root.join(format!("catalogue/{}.md", e.id)),
        catalogue::serialize_document(e, markdown).unwrap(),
    )
    .unwrap();
}
fn permission() -> Provenance {
    Provenance {
        redistribution: Redistribution::Permitted,
        original: true,
        content_only: true,
        sources: vec!["https://example.org/license".into()],
        license: Some("CC0-1.0".into()),
        rights_holder: None,
        permission: None,
        notes: None,
    }
}

fn plugin_collection(entry: &str) -> PluginCollection {
    let mut artifact = common::catalogue().documents[0].entry.artifacts[0].clone();
    artifact.id = "optional-content".into();
    artifact.role = ArtifactRole::Supplement;
    artifact.source = AssetSource::External {
        url: "https://example.org/optional-content.sit".into(),
    };
    PluginCollection {
        schema_version: SCHEMA_VERSION,
        entry: entry.into(),
        artifacts: vec![artifact],
        plugins: vec![Plugin {
            id: "optional-content".into(),
            label: "Optional content".into(),
            description: "An independently hosted extension".into(),
            download_artifact: "optional-content".into(),
            install: Vec::new(),
        }],
    }
}

fn save_plugins(root: &Path, name: &str, collection: &PluginCollection) {
    fs::create_dir_all(root.join("catalogue/plugins")).unwrap();
    fs::write(
        root.join(format!("catalogue/plugins/{name}.yaml")),
        serde_saphyr::to_string(collection).unwrap(),
    )
    .unwrap();
}

#[test]
fn derived_launch_assets_and_unattested_captures_are_rejected() {
    let mut entry = hosted("original", &hash(1));
    validate::entry(&entry).unwrap();

    entry.artifacts[0].role = ArtifactRole::WebPack;
    assert!(validate::entry(&entry)
        .unwrap_err()
        .to_string()
        .contains("derived assets"));

    entry.artifacts[0].role = ArtifactRole::Archive;
    entry.artifacts[0].format = FileType::Kpk;
    assert!(validate::entry(&entry)
        .unwrap_err()
        .to_string()
        .contains("derived assets"));

    entry.artifacts[0].format = FileType::Sit;
    entry.artifacts[0].provenance.original = false;
    assert!(validate::entry(&entry)
        .unwrap_err()
        .to_string()
        .contains("original: true"));

    entry.artifacts[0].role = ArtifactRole::Screenshot;
    entry.artifacts[0].format = FileType::Png;
    entry.artifacts[0].provenance.original = true;
    entry.artifacts[0].provenance.content_only = false;
    assert!(validate::entry(&entry)
        .unwrap_err()
        .to_string()
        .contains("content_only: true"));
}

#[test]
fn launch_enabled_entries_require_a_gameplay_screenshot() {
    let root = repo();
    let mut entry = simple("missing-screenshot");
    entry.launch_enabled = true;
    entry
        .artifacts
        .retain(|artifact| artifact.role != ArtifactRole::Screenshot);
    save(root.path(), &entry, "\nGameplay notes.\n");

    assert!(load(root.path(), Mode::Preview).is_ok());
    assert!(load(root.path(), Mode::Production)
        .unwrap_err()
        .to_string()
        .contains("launch-enabled entries require a gameplay screenshot"));
}

fn simple(id: &str) -> Entry {
    let mut e = common::catalogue().documents.remove(0).entry;
    e.id = id.into();
    e.route = None;
    e.aliases.clear();
    e.artifacts.clear();
    e
}
fn screenshot(root: &Path, id: &str) -> (Entry, String) {
    let path = format!("catalogue/incoming/{id}/shot.png");
    let url = format!("incoming/{id}/shot.png");
    fs::create_dir_all(root.join(format!("catalogue/incoming/{id}"))).unwrap();
    image::RgbImage::from_pixel(2, 2, image::Rgb([123, 45, 67]))
        .save(root.join(&path))
        .unwrap();
    let mut e = simple(id);
    e.artifacts.push(Artifact {
        id: "screenshot".into(),
        role: ArtifactRole::Screenshot,
        format: FileType::Png,
        source: AssetSource::Incoming { path: path.clone() },
        provenance: permission(),
    });
    let markdown =
        format!("\n![Title]({url})\n\n[Download][shot]\n\n[shot]: {url} \"Original\"\n\n`{url}`\n");
    save(root, &e, &markdown);
    (e, markdown)
}
fn compiled(c: &Catalogue) -> CompiledCatalogue {
    build(c).unwrap()
}

#[test]
fn empty_directory_marker_cannot_hide_content_or_missing_entries() {
    let root = repo();
    let marker = root.path().join("catalogue/.gitkeep");
    let with_marker = compiled(&load(root.path(), Mode::Production).unwrap());
    fs::remove_file(&marker).unwrap();
    let without_marker = compiled(&load(root.path(), Mode::Production).unwrap());
    assert_eq!(
        catalogue::json_bytes(&with_marker).unwrap(),
        catalogue::json_bytes(&without_marker).unwrap()
    );

    fs::write(&marker, "unparsed content").unwrap();
    assert!(load(root.path(), Mode::Preview).is_err());
    fs::remove_file(&marker).unwrap();
    fs::create_dir(&marker).unwrap();
    assert!(load(root.path(), Mode::Preview).is_err());
    fs::remove_dir(&marker).unwrap();
    #[cfg(unix)]
    {
        let empty = root.path().join("empty");
        fs::write(&empty, b"").unwrap();
        std::os::unix::fs::symlink(&empty, &marker).unwrap();
        assert!(load(root.path(), Mode::Preview).is_err());
        fs::remove_file(&marker).unwrap();
    }
    fs::remove_dir(root.path().join("catalogue")).unwrap();
    assert!(load(root.path(), Mode::Preview).is_err());
}

#[test]
fn strict_yaml_and_commonmark_round_trip() {
    let e = simple("test-title");
    let body = "\n# Notes\n\n```rust\nfn main() {}\n```\n\n{plain text, not MDX}\n";
    let text = catalogue::serialize_document(&e, body).unwrap();
    assert_eq!(parse_document(&text).unwrap(), (e, body.into()));
    let crlf = text.replace('\n', "\r\n");
    assert_eq!(parse_document(&crlf).unwrap().1, body.replace('\n', "\r\n"));
    assert!(parse_document(&text.replacen("id:", "unexpected:", 1)).is_err());
    assert!(parse_document(&text.replacen("---\n", "---\nunexpected: yes\n", 1)).is_err());
    assert!(parse_document("# no front matter").is_err());
    assert!(parse_document("---\nid: broken\n").is_err());
    assert!(
        parse_document(&text.replacen("id: test-title", "id: test-title\nid: duplicate", 1))
            .is_err()
    );
}

#[test]
fn build_is_portable_deterministic_and_round_trips() {
    let mut c = common::catalogue();
    let first = catalogue::json_bytes(&compiled(&c)).unwrap();
    c.documents.reverse();
    c.root = "/a/different/checkout".into();
    assert_eq!(first, catalogue::json_bytes(&compiled(&c)).unwrap());
    assert_eq!(
        serde_json::from_slice::<CompiledCatalogue>(&first).unwrap(),
        compiled(&c)
    );
}

#[test]
fn authoring_defaults_are_compact_but_compiled_defaults_are_resolved() {
    let c = common::catalogue();
    let entry = &c.documents[0].entry;
    let source = catalogue::serialize_document(entry, "").unwrap();
    for field in [
        "route:",
        "runtime:",
        "controls:",
        "launch_enabled:",
        "joystick:",
    ] {
        assert!(!source.contains(field), "unexpected default {field}");
    }
    let explicit = source.replacen(
        "---\n", "---\nruntime:\n  show_menu_bar: false\n  runtime_pacing:\n    cpu_mhz: 25\ncontrols:\n  mobile:\n    enabled: false\n", 1,
    );
    let (parsed, _) = parse_document(&explicit).unwrap();
    assert_eq!(parsed, *entry);
    let mut explicit_catalogue = c.clone();
    explicit_catalogue.documents[0].entry = parsed;
    assert_eq!(compiled(&c), compiled(&explicit_catalogue));

    let json = serde_json::to_value(compiled(&c)).unwrap();
    let entry = &json["entries"][0];
    assert_eq!(
        entry["runtime"]["runtime_pacing"],
        serde_json::json!({
            "cpu_mhz":25,"max_ticks_per_paint":2,"reset_slack_ticks":4
        })
    );
    assert_eq!(entry["runtime"]["show_menu_bar"], false);
    assert_eq!(entry["controls"]["mobile"]["enabled"], false);
    assert_eq!(entry["controls"]["mobile"]["joystick"]["up"], "ArrowUp");
    assert_eq!(entry["launch_enabled"], false);
}

#[test]
fn promotion_preserves_nested_overrides_without_materializing_defaults() {
    let root = repo();
    let (mut entry, body) = screenshot(root.path(), "overrides");
    entry.runtime.runtime_pacing.cpu_mhz = 40;
    entry.runtime.application_partition_size = Some(8 * 1024 * 1024);
    entry.controls.mobile.enabled = true;
    entry.controls.mobile.joystick.up = "w".into();
    entry.controls.mobile.buttons.push(Button {
        label: "A".into(),
        key: "Space".into(),
    });
    entry
        .controls
        .key_mappings
        .insert("ArrowDown".into(), "s".into());
    save(root.path(), &entry, &body);
    let before = fs::read_to_string(root.path().join("catalogue/overrides.md")).unwrap();
    assert_eq!(parse_document(&before).unwrap().0, entry);
    let store = tempfile::tempdir().unwrap();
    assets::promote(
        root.path(),
        None,
        true,
        &mut assets::DirectoryStore {
            root: store.path().into(),
        },
    )
    .unwrap();
    let after = fs::read_to_string(root.path().join("catalogue/overrides.md")).unwrap();
    let (promoted, _) = parse_document(&after).unwrap();
    assert_eq!(promoted.runtime, entry.runtime);
    assert_eq!(promoted.controls, entry.controls);
    for field in [
        "show_menu_bar:",
        "max_ticks_per_paint:",
        "reset_slack_ticks:",
        "arrows_as_numpad:",
        "ArrowUp",
        "ArrowLeft",
        "ArrowRight",
    ] {
        assert!(!after.contains(field), "promotion materialized {field}");
    }
    let built = compiled(&load(root.path(), Mode::Production).unwrap());
    assert_eq!(built.entries[0].runtime.runtime_pacing.cpu_mhz, 40);
    assert_eq!(built.entries[0].controls.mobile.joystick.down, "ArrowDown");
}

#[test]
fn compiled_contract_resolves_assets_without_publishing_ingestion_or_storage_state() {
    let mut c = common::catalogue();
    let mut entry = hosted("one", &hash(1));
    let mut external = entry.artifacts[0].clone();
    external.id = "extra".into();
    external.role = ArtifactRole::Supplement;
    external.source = AssetSource::External {
        url: "https://example.org/extra.sit".into(),
    };
    entry.artifacts.push(external.clone());
    external.id = "pending".into();
    external.source = AssetSource::Url {
        url: "https://example.org/pending.sit".into(),
        download_page: None,
        expected_sha256: Some(hash(2)),
        expected_size: Some(123),
    };
    entry.artifacts.push(external);
    c.documents.truncate(1);
    c.documents[0].entry = entry.clone();
    c.documents[0].markdown = "# Details\n".into();
    let json = serde_json::to_value(compiled(&c)).unwrap();
    assert_eq!(json.as_object().unwrap().len(), 4);
    assert_eq!(json["schema_version"], 2);
    let e = &json["entries"][0];
    assert_eq!(e["id"], "one");
    assert_eq!(e["path"], "/one");
    assert_eq!(e["content_html"], "<h1>Details</h1>\n");
    for unsupported in [
        "metadata",
        "markdown",
        "html",
        "canonical_path",
        "canonical_url",
        "asset_urls",
        "artifacts",
        "provenance",
        "approved",
    ] {
        assert!(
            e.get(unsupported).is_none(),
            "unexpected compiled field: {unsupported}"
        );
    }
    let a = &e["assets"][0];
    assert_eq!(a["sha256"], hash(1));
    assert_eq!(a["size_bytes"], 123);
    assert_eq!(
        a["url"],
        catalogue::artifact_url(&c.config, &entry.artifacts[0])
    );
    assert_eq!(a["rights"], serde_json::to_value(permission()).unwrap());
    for a in e["assets"].as_array().unwrap() {
        for private in [
            "source",
            "key",
            "references",
            "download_page",
            "expected_sha256",
        ] {
            assert!(a.get(private).is_none(), "storage field leaked: {private}");
        }
    }
    for index in [1, 2] {
        assert!(e["assets"][index].get("sha256").is_none());
        assert!(e["assets"][index].get("size_bytes").is_none());
    }
    // The independent tooling manifest still includes unlaunched, non-inline assets.
    let desired = assets::desired(&c).unwrap();
    assert_eq!(desired.len(), 1);
    assert_eq!(desired[0].references, ["one:archive"]);
}

#[test]
fn route_overrides_and_aliases_share_the_default_route_namespace() {
    let mut c = common::catalogue();
    c.documents[0].entry.route = Some("/software/paint".into());
    assert_eq!(compiled(&c).entries[0].path, "/software/paint");
    c.documents[0].entry.route = Some("/sample-game".into());
    assert!(build(&c).is_err());
    c.documents[0].entry.route = None;
    c.documents[0].entry.aliases.push("/sample-game".into());
    assert!(build(&c).is_err());
    c.documents[0].entry.aliases = vec!["/sample-app".into()];
    assert!(build(&c).is_err());
    // The embedded compiler owns the website preview namespace.
    assert!(validate::route("/next/example").is_err());
}

#[test]
fn unsupported_versions_and_fields_are_rejected_and_references_are_validated() {
    let mut c = common::catalogue();
    for unsupported in [0, 2, u32::MAX] {
        c.config.schema_version = unsupported;
        assert!(build(&c).is_err());
    }
    let entry = &mut c.documents[0].entry;
    let source = catalogue::serialize_document(entry, "").unwrap();
    for unsupported in ["approved: true", "provenance: {redistribution: unknown}"] {
        assert!(
            parse_document(&source.replacen("---\n", &format!("---\n{unsupported}\n"), 1)).is_err()
        );
    }
    for role in ["plugin_download", "plugin_asset"] {
        assert!(serde_json::from_value::<ArtifactRole>(serde_json::json!(role)).is_err());
    }
    entry.references = vec!["https://example.org/history".into()];
    validate::entry(entry).unwrap();
    entry.references = vec!["javascript:alert(1)".into()];
    assert!(validate::entry(entry).is_err());
}

#[test]
fn canonical_routes_launch_gate_aliases_and_community_links() {
    let mut c = common::catalogue();
    c.config.repository = "https://github.com/example/catalogue-fork/".into();
    let doc = c
        .documents
        .iter_mut()
        .find(|d| d.entry.id == "sample-game")
        .unwrap();
    doc.entry.launch_enabled = false;
    doc.entry.title = "Sample Game & friends? #1".into();
    doc.entry.aliases = vec!["/m1".into()];
    let b = compiled(&c);
    assert_eq!(b.entries.len(), 2);
    let e = b.entries.iter().find(|e| e.id == "sample-game").unwrap();
    assert_eq!(e.path, "/sample-game");
    assert_eq!(e.aliases, ["/m1"]);
    assert!(!e.launch_enabled);
    for (link, template, label) in [
        (
            &e.community.suggest_controls_config,
            "controls-config.yml",
            "Controls/config",
        ),
        (
            &e.community.takedown_request,
            "takedown.yml",
            "Takedown request",
        ),
    ] {
        let issue = url::Url::parse(link).unwrap();
        assert_eq!(issue.scheme(), "https");
        assert_eq!(issue.host_str(), Some("github.com"));
        assert_eq!(issue.path(), "/example/catalogue-fork/issues/new");
        assert!(issue.fragment().is_none());
        let pairs: BTreeMap<_, _> = issue.query_pairs().into_owned().collect();
        assert_eq!(pairs.len(), 3);
        assert_eq!(pairs["entry"], "sample-game");
        assert_eq!(pairs["template"], template);
        assert_eq!(
            pairs["title"],
            format!("[sample-game] {label}: Sample Game & friends? #1")
        );
    }
    let compatibility_issue = url::Url::parse(&e.community.compatibility_issue).unwrap();
    assert_eq!(
        compatibility_issue.path(),
        "/benletchford/systemless/issues/new"
    );
    let pairs: BTreeMap<_, _> = compatibility_issue.query_pairs().into_owned().collect();
    assert_eq!(pairs["labels"], "sample-game");
    assert_eq!(pairs["title"], "Sample Game & friends? #1 compatibility: ");
    assert!(pairs["body"].contains("Game ID: `sample-game`"));
    let reports = url::Url::parse(&e.community.open_reports).unwrap();
    assert_eq!(reports.path(), "/benletchford/systemless/issues");
    assert_eq!(
        reports.query_pairs().find(|(key, _)| key == "q").unwrap().1,
        "is:issue is:open label:sample-game"
    );
    assert!(e
        .community
        .history
        .ends_with("/commits/master/www/catalogue/sample-game.md"));
}

#[test]
fn general_takedown_link_uses_configured_repository_without_entries() {
    let mut c = common::catalogue();
    c.documents.clear();
    for repository in [
        "https://github.com/example/catalogue-fork",
        "https://github.com/another-owner/another-catalogue/",
    ] {
        c.config.repository = repository.into();
        let b = compiled(&c);
        assert!(b.entries.is_empty());
        let json = serde_json::to_value(b).unwrap();
        let link = json["community"]["takedown_request"].as_str().unwrap();
        let mut issue = url::Url::parse(link).unwrap();
        let pairs: BTreeMap<_, _> = issue.query_pairs().into_owned().collect();
        assert_eq!(pairs.len(), 1);
        assert_eq!(pairs["template"], "takedown.yml");
        issue.set_query(None);
        assert_eq!(
            issue.as_str(),
            format!("{}/issues/new", repository.trim_end_matches('/'))
        );
    }
}

#[test]
fn validation_catches_semantic_errors() {
    let mut e = simple("one");
    for category in CATEGORIES {
        e.category = (*category).into();
        validate::entry(&e).unwrap();
    }
    e.category = "Action Adventure".into();
    assert!(validate::entry(&e)
        .unwrap_err()
        .to_string()
        .contains("unsupported category"));
    e.category = "Space Trading".into();
    e.runtime.runtime_pacing.cpu_mhz = 500;
    assert!(validate::entry(&e).is_err());
    e.runtime.runtime_pacing.cpu_mhz = 25;
    e.runtime.remove_paths = vec!["../../etc".into()];
    assert!(validate::entry(&e).is_err());
    e.runtime.remove_paths.clear();
    e.default_architecture = Architecture::Ppc;
    e.architectures = vec![Architecture::M68k];
    assert!(validate::entry(&e).is_err());
    e.default_architecture = Architecture::M68k;
    e.compatibility.status = Status::Works;
    assert!(validate::entry(&e).is_err());
    e.compatibility.status = Status::Unknown;
    e.controls.mobile.buttons = vec![Button {
        label: "A".into(),
        key: "Unsupported".into(),
    }];
    assert!(validate::entry(&e).is_err());
    for path in ["/assets/foo", "/foo/", "/../etc", "/foo?x", "//foo"] {
        assert!(validate::route(path).is_err(), "{path}");
    }
    for path in ["/etc/passwd", "a/../b", "a\\b", "C:foo", "./foo"] {
        assert!(validate::relative_path(path).is_err());
    }
}

#[test]
fn global_collisions_are_rejected() {
    let mut c = common::catalogue();
    let route = systemless_catalogue_tools::catalogue_tools::community::canonical_path(
        &c.documents[0].entry,
    );
    c.documents[1].entry.aliases.push(route);
    assert!(build(&c).is_err());
    let root = repo();
    let mut e = simple("valid");
    save(root.path(), &e, "");
    fs::rename(
        root.path().join("catalogue/valid.md"),
        root.path().join("catalogue/wrong.md"),
    )
    .unwrap();
    assert!(load(root.path(), Mode::Preview).is_err());
    e.id = "../unsafe".into();
    assert!(validate::entry(&e).is_err());
}

#[test]
fn raw_html_is_inert_and_images_must_be_registered() {
    let root = repo();
    let e = simple("test");
    save(
        root.path(),
        &e,
        "<script>alert('x')</script>\n\n<p onclick=\"evil()\">Hi</p>\n",
    );
    let b = compiled(&load(root.path(), Mode::Preview).unwrap());
    assert!(!b.entries[0].content_html.contains("<script>"));
    assert!(b.entries[0].content_html.contains("&lt;script&gt;"));
    save(
        root.path(),
        &e,
        "![Unmanaged](https://example.org/shot.png)\n",
    );
    assert!(load(root.path(), Mode::Preview).is_err());
    save(root.path(), &e, "[click](javascript:alert(1))\n");
    assert!(load(root.path(), Mode::Preview).is_err());
}

#[test]
fn promotion_uploads_hashes_rewrites_and_cleans_up() {
    let root = repo();
    let (_, body) = screenshot(root.path(), "test");
    let preview = compiled(&load(root.path(), Mode::Preview).unwrap());
    assert!(preview.entries[0]
        .content_html
        .contains("/raw/master/www/catalogue/incoming/test/shot.png"));
    let store = tempfile::tempdir().unwrap();
    let before = fs::read(root.path().join("catalogue/test.md")).unwrap();
    let dry = assets::promote(root.path(), None, false, &mut assets::NoUpload).unwrap();
    assert!(!dry.applied);
    assert_eq!(dry.artifacts.len(), 1);
    assert_eq!(
        before,
        fs::read(root.path().join("catalogue/test.md")).unwrap()
    );
    let mut backend = assets::DirectoryStore {
        root: store.path().into(),
    };
    let report = assets::promote(root.path(), None, true, &mut backend).unwrap();
    assert!(report.applied);
    assert_eq!(report.removed, vec!["catalogue/incoming/test/shot.png"]);
    assert!(store.path().join(&report.artifacts[0].object.key).exists());
    assert!(!root
        .path()
        .join("catalogue/incoming/test/shot.png")
        .exists());
    let c = load(root.path(), Mode::Production).unwrap();
    assert!(c.documents[0]
        .markdown
        .contains("https://assets.systemless.org/catalogue/media/sha256/"));
    assert!(c.documents[0].markdown.contains("`incoming/test/shot.png`"));
    assert_ne!(c.documents[0].markdown, body);
    assert!(assets::promote(root.path(), None, true, &mut backend)
        .unwrap()
        .artifacts
        .is_empty());
}

struct FailingStore;
impl ObjectStore for FailingStore {
    fn put_if_absent(&mut self, _: &assets::DesiredObject, _: &Path) -> anyhow::Result<()> {
        anyhow::bail!("simulated upload failure")
    }
}
#[test]
fn promotion_failure_preserves_source_and_incoming() {
    let root = repo();
    screenshot(root.path(), "one");
    screenshot(root.path(), "two");
    let before = fs::read(root.path().join("catalogue/one.md")).unwrap();
    assert!(assets::promote(root.path(), None, true, &mut FailingStore).is_err());
    assert_eq!(
        before,
        fs::read(root.path().join("catalogue/one.md")).unwrap()
    );
    assert!(root.path().join("catalogue/incoming/one/shot.png").exists());
    assert!(root.path().join("catalogue/incoming/two/shot.png").exists());
}

#[test]
fn promotion_requires_rights_and_correct_content() {
    let root = repo();
    let (mut e, body) = screenshot(root.path(), "one");
    e.artifacts[0].provenance.redistribution = Redistribution::Unknown;
    save(root.path(), &e, &body);
    assert!(assets::promote(root.path(), None, false, &mut assets::NoUpload).is_err());
    e.artifacts[0].provenance = permission();
    save(root.path(), &e, &body);
    fs::write(
        root.path().join("catalogue/incoming/one/shot.png"),
        "<html>not a png</html>",
    )
    .unwrap();
    assert!(load(root.path(), Mode::Preview).is_err());
}

#[test]
fn default_branch_rejects_all_incoming_including_orphans() {
    let root = repo();
    screenshot(root.path(), "one");
    assert!(load(root.path(), Mode::Preview).is_ok());
    assert!(load(root.path(), Mode::NoIncoming).is_err());
    fs::write(root.path().join("catalogue/incoming/orphan"), "test").unwrap();
    assert!(load(root.path(), Mode::Preview).is_err());
}

#[test]
fn empty_incoming_marker_is_allowed_but_cannot_hide_content() {
    let root = repo();
    let incoming = root.path().join("catalogue/incoming");
    fs::create_dir_all(&incoming).unwrap();
    fs::write(incoming.join(".gitkeep"), b"").unwrap();
    assert!(load(root.path(), Mode::NoIncoming).is_ok());
    fs::write(incoming.join(".gitkeep"), b"not empty").unwrap();
    assert!(load(root.path(), Mode::NoIncoming).is_err());
}

#[cfg(unix)]
#[test]
fn symlinks_cannot_escape_repository_or_store() {
    use std::os::unix::fs::symlink;
    let root = repo();
    let outside = tempfile::tempdir().unwrap();
    fs::write(outside.path().join("secret.png"), "secret").unwrap();
    fs::create_dir_all(root.path().join("catalogue/incoming/one")).unwrap();
    symlink(
        outside.path().join("secret.png"),
        root.path().join("catalogue/incoming/one/shot.png"),
    )
    .unwrap();
    assert!(validate::safe_file(root.path(), "catalogue/incoming/one/shot.png").is_err());
    symlink(outside.path(), root.path().join("catalogue/incoming/two")).unwrap();
    assert!(validate::safe_file(root.path(), "catalogue/incoming/two/secret.png").is_err());
}

fn hosted(id: &str, hash: &str) -> Entry {
    let mut e = simple(id);
    e.artifacts.push(Artifact {
        id: "archive".into(),
        role: ArtifactRole::Archive,
        format: FileType::Sit,
        source: AssetSource::Sha256 {
            sha256: hash.into(),
            size_bytes: 123,
        },
        provenance: permission(),
    });
    e
}
fn hash(n: u8) -> String {
    format!("{n:064x}")
}
fn inventory(objects: Vec<r2::RemoteObject>) -> r2::Inventory {
    r2::Inventory {
        schema_version: 1,
        complete: true,
        prefixes: assets::MANAGED_PREFIXES.map(String::from).to_vec(),
        objects,
    }
}
fn remote(hash: &str) -> r2::RemoteObject {
    r2::RemoteObject {
        key: assets::object_key(hash, FileType::Sit),
        size_bytes: 123,
        last_modified: "2025-01-01T00:00:00Z".into(),
    }
}
fn now() -> jiff::Timestamp {
    "2026-09-12T00:00:00Z".parse().unwrap()
}
fn wide_policy() -> r2::DeletePolicy {
    r2::DeletePolicy {
        max_delete: 10,
        max_delete_percent: 100,
        min_age_hours: 24,
        allow_empty: true,
    }
}

#[test]
fn shared_hashes_deduplicate_and_last_reference_becomes_orphan() {
    let root = repo();
    save(root.path(), &hosted("one", &hash(1)), "");
    save(root.path(), &hosted("two", &hash(1)), "");
    let c = load(root.path(), Mode::Production).unwrap();
    let objects = assets::desired(&c).unwrap();
    assert_eq!(objects.len(), 1);
    assert_eq!(objects[0].references, vec!["one:archive", "two:archive"]);
    let inv = inventory(vec![remote(&hash(1))]);
    let p = r2::plan(&c, &inv, wide_policy(), now()).unwrap();
    assert!(p.delete.is_empty());
    fs::remove_file(root.path().join("catalogue/one.md")).unwrap();
    assert!(r2::plan(
        &load(root.path(), Mode::Production).unwrap(),
        &inv,
        wide_policy(),
        now()
    )
    .unwrap()
    .delete
    .is_empty());
    fs::remove_file(root.path().join("catalogue/two.md")).unwrap();
    let p = r2::plan(
        &load(root.path(), Mode::Production).unwrap(),
        &inv,
        wide_policy(),
        now(),
    )
    .unwrap();
    assert_eq!(p.delete, vec![assets::object_key(&hash(1), FileType::Sit)]);
    assert!(p.blockers.is_empty());
}

#[test]
fn reconciliation_protects_prefixes_thresholds_grace_period_and_completeness() {
    let root = repo();
    save(root.path(), &hosted("one", &hash(1)), "");
    let c = load(root.path(), Mode::Production).unwrap();
    let mut recent = remote(&hash(3));
    recent.last_modified = now().to_string();
    let mut inv = inventory(vec![
        remote(&hash(1)),
        remote(&hash(2)),
        recent,
        r2::RemoteObject {
            key: "unmanaged/example.sit".into(),
            size_bytes: 123,
            last_modified: "invalid but unmanaged".into(),
        },
        r2::RemoteObject {
            key: "catalogue/objects/sha256/../../secret".into(),
            size_bytes: 0,
            last_modified: String::new(),
        },
    ]);
    let p = r2::plan(&c, &inv, r2::DeletePolicy::default(), now()).unwrap();
    assert_eq!(p.delete, vec![assets::object_key(&hash(2), FileType::Sit)]);
    assert_eq!(p.protected.len(), 3);
    assert!(p.blockers.iter().any(|b| b.contains("percentage")));
    inv.complete = false;
    assert!(r2::plan(&c, &inv, wide_policy(), now()).is_err());
    inv.complete = true;
    inv.prefixes.pop();
    assert!(r2::plan(&c, &inv, wide_policy(), now()).is_err());
}

#[test]
fn pending_assets_and_missing_objects_block_deletion() {
    let c = common::catalogue();
    let p = r2::plan(&c, &inventory(vec![remote(&hash(1))]), wide_policy(), now()).unwrap();
    assert!(p
        .blockers
        .iter()
        .any(|b| b.starts_with("pending promotion:")));
    let root = repo();
    save(root.path(), &hosted("one", &hash(1)), "");
    let p = r2::plan(
        &load(root.path(), Mode::Production).unwrap(),
        &inventory(vec![remote(&hash(2))]),
        wide_policy(),
        now(),
    )
    .unwrap();
    assert_eq!(p.missing, vec![assets::object_key(&hash(1), FileType::Sit)]);
    assert!(!p.blockers.is_empty());
}

#[test]
fn immutable_local_store_rejects_conflicting_bytes() {
    let root = tempfile::tempdir().unwrap();
    let source = tempfile::NamedTempFile::new().unwrap();
    fs::write(source.path(), "payload").unwrap();
    let inspect = assets::hash_file(source.path()).unwrap();
    let object = assets::DesiredObject {
        key: assets::object_key(&inspect.sha256, FileType::Sit),
        sha256: inspect.sha256,
        size_bytes: inspect.size_bytes,
        content_type: "application/x-stuffit".into(),
        references: vec![],
    };
    let mut store = assets::DirectoryStore {
        root: root.path().into(),
    };
    store.put_if_absent(&object, source.path()).unwrap();
    store.put_if_absent(&object, source.path()).unwrap();
    fs::write(root.path().join(&object.key), "corrupt").unwrap();
    assert!(store.put_if_absent(&object, source.path()).is_err());
}

#[test]
fn public_download_policy_rejects_private_special_and_mapped_addresses() {
    for ip in [
        "127.0.0.1",
        "10.0.0.1",
        "172.16.1.2",
        "192.168.1.1",
        "169.254.169.254",
        "100.64.0.1",
        "198.18.0.1",
        "192.0.2.1",
        "224.0.0.1",
        "::1",
        "::ffff:127.0.0.1",
        "fc00::1",
        "fe80::1",
        "2001:db8::1",
        "2002:7f00:1::1",
    ] {
        assert!(!network::public_ip(ip.parse().unwrap()), "{ip}");
    }
    for ip in ["1.1.1.1", "8.8.8.8", "2606:4700:4700::1111"] {
        assert!(network::public_ip(ip.parse().unwrap()), "{ip}");
    }
    for url in [
        "http://example.org/a",
        "https://user:pass@example.org/a",
        "https://localhost/a",
        "https://example.org:444/a",
    ] {
        assert!(network::public_url(&url::Url::parse(url).unwrap()).is_err());
    }
}

#[test]
fn diff_reports_add_remove_and_changes() {
    let c = common::catalogue();
    let before = compiled(&c);
    let mut after = before.clone();
    let removed = after.entries.remove(0).id;
    after.entries[0].summary = "Updated".into();
    let mut added = after.entries[0].clone();
    added.id = "new-title".into();
    after.entries.push(added);
    let d = catalogue::diff(&before, &after);
    assert_eq!(d.added, vec!["new-title"]);
    assert_eq!(d.removed, vec![removed]);
    assert_eq!(d.changed["sample-game"], vec!["summary"]);
}

#[test]
fn yaml_resource_limits_duplicates_and_extensions_are_enforced() {
    let nested = format!("{}0{}", "[".repeat(40), "]".repeat(40));
    assert!(yaml::from_str::<serde_json::Value>(&nested).is_err());
    assert!(yaml::from_str::<serde_json::Value>("a: 1\na: 2\n").is_err());
    assert!(yaml::from_str::<serde_json::Value>("a: &base {x: 1}\nb: {<<: *base}\n").is_err());
    assert!(yaml::from_str::<bool>("yes").is_err());
    assert!(yaml::from_str::<String>("!include /etc/passwd").is_err());
    let many_aliases = format!("a: &x 1\nb: [{}]", vec!["*x"; 129].join(","));
    assert!(yaml::from_str::<serde_json::Value>(&many_aliases).is_err());
    let file = tempfile::NamedTempFile::new().unwrap();
    file.as_file().set_len(1025).unwrap();
    assert!(catalogue::read_text(file.path(), 1024).is_err());
}

#[test]
fn one_pipeline_promotes_software_and_deduplicates_across_titles() {
    let root = repo();
    let store = tempfile::tempdir().unwrap();
    // A locally staged archive can exceed the image-contribution limit.
    let mut bytes = vec![0; 11 * 1024 * 1024];
    bytes[1] = 4;
    bytes[2..6].copy_from_slice(b"Test");
    for id in ["one", "two"] {
        let mut e = simple(id);
        let path = format!("catalogue/incoming/{id}/app.bin");
        fs::create_dir_all(root.path().join(format!("catalogue/incoming/{id}"))).unwrap();
        fs::write(root.path().join(&path), &bytes).unwrap();
        e.artifacts = vec![Artifact {
            id: "archive".into(),
            role: ArtifactRole::Archive,
            format: FileType::Bin,
            source: AssetSource::Incoming { path: path.clone() },
            provenance: permission(),
        }];
        save(root.path(), &e, &format!("[Download](../{path})\n"));
    }
    let report = assets::promote(
        root.path(),
        None,
        true,
        &mut assets::DirectoryStore {
            root: store.path().into(),
        },
    )
    .unwrap();
    assert_eq!(report.artifacts.len(), 2);
    assert_eq!(
        report.artifacts[0].object.key,
        report.artifacts[1].object.key
    );
    assert!(report.artifacts[0]
        .object
        .key
        .starts_with("catalogue/objects/sha256/"));
    assert_eq!(
        assets::desired(&load(root.path(), Mode::Production).unwrap())
            .unwrap()
            .len(),
        1
    );
}

#[test]
fn incoming_archives_and_images_reject_oversized_files_before_reading() {
    for (format, role, size) in [
        (
            FileType::Sit,
            ArtifactRole::Archive,
            2 * 1024 * 1024 * 1024 + 1,
        ),
        (
            FileType::Png,
            ArtifactRole::Screenshot,
            10 * 1024 * 1024 + 1,
        ),
    ] {
        let root = repo();
        let mut e = simple("one");
        let path = format!("catalogue/incoming/one/asset.{}", format.ext());
        fs::create_dir_all(root.path().join("catalogue/incoming/one")).unwrap();
        fs::File::create(root.path().join(&path))
            .unwrap()
            .set_len(size)
            .unwrap();
        e.artifacts = vec![Artifact {
            id: "asset".into(),
            role,
            format,
            source: AssetSource::Incoming { path },
            provenance: permission(),
        }];
        save(root.path(), &e, "");
        let error = load(root.path(), Mode::Preview).err().unwrap();
        assert!(format!("{error:#}").contains("incoming file exceeds"));
    }
}

#[test]
fn unknown_nested_fields_and_unsafe_source_urls_fail_offline() {
    let root = repo();
    let (e, body) = screenshot(root.path(), "one");
    let text = catalogue::serialize_document(&e, &body).unwrap();
    assert!(parse_document(&text.replace(
        "type: incoming",
        "type: incoming\n      unknown_field: true"
    ))
    .is_err());
    let mut e = e;
    for url in [
        "https://127.0.0.1/a.png",
        "https://[::1]/a.png",
        "https://localhost/a.png",
        "https://example.org:444/a.png",
    ] {
        e.artifacts[0].source = AssetSource::Url {
            download_page: None,
            url: url.into(),
            expected_sha256: None,
            expected_size: None,
        };
        assert!(validate::entry(&e).is_err(), "{url}");
    }
}

#[test]
fn shared_hash_conflicts_empty_sets_and_count_thresholds_are_blocked() {
    let root = repo();
    save(root.path(), &hosted("one", &hash(1)), "");
    let mut e = hosted("two", &hash(1));
    e.artifacts[0].format = FileType::Bin;
    save(root.path(), &e, "");
    assert!(load(root.path(), Mode::Production).is_err());
    fs::remove_file(root.path().join("catalogue/two.md")).unwrap();
    let mut policy = wide_policy();
    policy.max_delete = 0;
    let inv = inventory(vec![remote(&hash(1)), remote(&hash(2))]);
    let p = r2::plan(
        &load(root.path(), Mode::Production).unwrap(),
        &inv,
        policy,
        now(),
    )
    .unwrap();
    assert!(p.blockers.iter().any(|b| b.contains("count")));
    fs::remove_file(root.path().join("catalogue/one.md")).unwrap();
    let p = r2::plan(
        &load(root.path(), Mode::Production).unwrap(),
        &inv,
        r2::DeletePolicy::default(),
        now(),
    )
    .unwrap();
    assert!(p.blockers.iter().any(|b| b.contains("empty desired")));
    for key in [
        "catalogue/objects/sha256/ab/invalid.sit",
        "catalogue/objects/sha256/ab/../../other",
        "unmanaged/example.sit",
    ] {
        assert!(!assets::is_managed_key(key));
    }
}

struct EditingStore {
    path: std::path::PathBuf,
}
impl ObjectStore for EditingStore {
    fn put_if_absent(&mut self, _: &assets::DesiredObject, _: &Path) -> anyhow::Result<()> {
        use std::io::Write;
        let mut file = fs::OpenOptions::new().append(true).open(&self.path)?;
        file.write_all(b"\nConcurrent community edit\n")?;
        Ok(())
    }
}
#[test]
fn concurrent_edit_during_upload_is_preserved() {
    let root = repo();
    screenshot(root.path(), "one");
    let path = root.path().join("catalogue/one.md");
    assert!(assets::promote(
        root.path(),
        None,
        true,
        &mut EditingStore { path: path.clone() }
    )
    .is_err());
    assert!(fs::read_to_string(path)
        .unwrap()
        .ends_with("Concurrent community edit\n"));
    assert!(root.path().join("catalogue/incoming/one/shot.png").exists());
    assert!(!root
        .path()
        .join("catalogue/.promotion/transaction.json")
        .exists());
}

struct FakeR2 {
    objects: std::cell::RefCell<Vec<r2::RemoteObject>>,
    deleted: std::cell::RefCell<Vec<String>>,
}
impl r2::ReconciliationStore for FakeR2 {
    fn inventory(&self) -> anyhow::Result<r2::Inventory> {
        Ok(inventory(self.objects.borrow().clone()))
    }
    fn delete(&self, key: &str) -> anyhow::Result<()> {
        assert!(assets::is_managed_key(key));
        self.objects.borrow_mut().retain(|o| o.key != key);
        self.deleted.borrow_mut().push(key.into());
        Ok(())
    }
}
#[test]
fn reconcile_executes_only_reviewed_orphans_and_refuses_stale_plans() {
    use r2::ReconciliationStore;
    let root = repo();
    save(root.path(), &hosted("one", &hash(1)), "");
    save(root.path(), &hosted("two", &hash(1)), "");
    let store = FakeR2 {
        objects: std::cell::RefCell::new(vec![remote(&hash(1)), remote(&hash(2))]),
        deleted: std::cell::RefCell::new(vec![]),
    };
    let c = load(root.path(), Mode::Production).unwrap();
    let p = r2::plan(
        &c,
        &store.inventory().unwrap(),
        wide_policy(),
        jiff::Timestamp::now(),
    )
    .unwrap();
    // A changed remote set invalidates the reviewed plan before the first deletion.
    store.objects.borrow_mut().push(remote(&hash(3)));
    assert!(r2::reconcile(root.path(), &p, &store).is_err());
    assert!(store.deleted.borrow().is_empty());
    store.objects.borrow_mut().pop();
    assert_eq!(r2::reconcile(root.path(), &p, &store).unwrap(), 1);
    assert_eq!(
        *store.deleted.borrow(),
        vec![assets::object_key(&hash(2), FileType::Sit)]
    );
    // The shared object survives one removed reference, and is collected only after the last one.
    fs::remove_file(root.path().join("catalogue/one.md")).unwrap();
    let c = load(root.path(), Mode::Production).unwrap();
    let p = r2::plan(
        &c,
        &store.inventory().unwrap(),
        wide_policy(),
        jiff::Timestamp::now(),
    )
    .unwrap();
    assert_eq!(r2::reconcile(root.path(), &p, &store).unwrap(), 0);
    fs::remove_file(root.path().join("catalogue/two.md")).unwrap();
    let c = load(root.path(), Mode::Production).unwrap();
    let p = r2::plan(
        &c,
        &store.inventory().unwrap(),
        wide_policy(),
        jiff::Timestamp::now(),
    )
    .unwrap();
    assert_eq!(r2::reconcile(root.path(), &p, &store).unwrap(), 1);
    assert!(store.objects.borrow().is_empty());
}

#[test]
fn managed_object_requests_must_bind_key_to_content_hash() {
    let object = assets::DesiredObject {
        key: assets::object_key(&hash(1), FileType::Sit),
        sha256: hash(2),
        size_bytes: 123,
        content_type: "application/x-stuffit".into(),
        references: vec![],
    };
    assert!(assets::validate_object(&object).is_err());
}

#[test]
fn fetch_caches_selected_assets_without_rewriting_or_requiring_redistribution() {
    let root = repo();
    let cache = tempfile::tempdir().unwrap();
    let (mut e, body) = screenshot(root.path(), "one");
    e.artifacts[0].provenance.redistribution = Redistribution::Unknown;
    e.artifacts[0].provenance.license = None;
    e.artifacts.push(Artifact {
        id: "external".into(),
        role: ArtifactRole::Supplement,
        format: FileType::Opaque,
        source: AssetSource::External {
            url: "https://example.org/download".into(),
        },
        provenance: e.artifacts[0].provenance.clone(),
    });
    save(root.path(), &e, &body);
    screenshot(root.path(), "two");
    let before = fs::read(root.path().join("catalogue/one.md")).unwrap();
    let report = assets::fetch(root.path(), Some("one"), cache.path()).unwrap();
    assert_eq!(report.len(), 1);
    assert_eq!(report[0].entry, "one");
    assert_eq!(report[0].artifact, "screenshot");
    assert!(report[0].path.exists());
    assert_eq!(
        assets::hash_file(&report[0].path).unwrap().sha256,
        report[0].sha256
    );
    assert_eq!(
        before,
        fs::read(root.path().join("catalogue/one.md")).unwrap()
    );
    assert!(root.path().join("catalogue/incoming/one/shot.png").exists());
    assert!(!root.path().join("catalogue/.promotion").exists());
    let both = assets::fetch(root.path(), None, cache.path()).unwrap();
    assert_eq!(both.len(), 2);
    assert_eq!(both[0].path, both[1].path);
    assert!(assets::fetch(root.path(), Some("missing"), cache.path()).is_err());
}

#[test]
fn downloads_verify_both_pending_and_promoted_integrity_assertions() {
    let mut a = hosted("one", &hash(1)).artifacts.remove(0);
    let actual = assets::Inspection {
        sha256: hash(1),
        size_bytes: 123,
    };
    assets::verify_integrity(&a, &actual).unwrap();
    assert!(assets::verify_integrity(
        &a,
        &assets::Inspection {
            sha256: hash(2),
            size_bytes: 123
        }
    )
    .is_err());
    a.source = AssetSource::Url {
        download_page: None,
        url: "https://example.org/archive.sit".into(),
        expected_sha256: Some(hash(1)),
        expected_size: Some(123),
    };
    assets::verify_integrity(&a, &actual).unwrap();
    assert!(assets::verify_integrity(
        &a,
        &assets::Inspection {
            sha256: hash(2),
            size_bytes: 123
        }
    )
    .is_err());
    assert!(assets::verify_integrity(
        &a,
        &assets::Inspection {
            sha256: hash(1),
            size_bytes: 124
        }
    )
    .is_err());
}

#[test]
fn stuffit5_header_and_declared_archive_length_are_checked() {
    let file = tempfile::NamedTempFile::new().unwrap();
    let mut bytes = vec![0_u8; 114];
    bytes[..11].copy_from_slice(b"StuffIt (c)");
    bytes[80..83].copy_from_slice(&[0x1a, 0, 5]);
    bytes[84..88].copy_from_slice(&114_u32.to_be_bytes());
    fs::write(file.path(), &bytes).unwrap();
    assert_eq!(
        assets::inspect(file.path(), FileType::Sit)
            .unwrap()
            .size_bytes,
        114
    );
    bytes[82] = 6;
    fs::write(file.path(), &bytes).unwrap();
    assert!(assets::inspect(file.path(), FileType::Sit).is_err());
    bytes[82] = 5;
    bytes.pop();
    fs::write(file.path(), &bytes).unwrap();
    assert!(assets::inspect(file.path(), FileType::Sit).is_err());
    bytes.push(0);
    bytes.push(0);
    fs::write(file.path(), &bytes).unwrap();
    assert!(assets::inspect(file.path(), FileType::Sit).is_err());
}

#[test]
fn plugins_reference_managed_supplements_and_safe_install_paths() {
    let mut c = common::catalogue();
    let e = &mut c.documents[0].entry;
    let mut asset = e.artifacts[0].clone();
    asset.id = "extension".into();
    asset.format = FileType::Bin;
    asset.role = ArtifactRole::Supplement;
    asset.provenance.original = true;
    e.artifacts.push(asset);
    e.plugins.push(Plugin {
        id: "example-extension".into(),
        label: "Example extension".into(),
        description: "Optional extra content".into(),
        download_artifact: "extension".into(),
        install: vec![PluginInstall {
            artifact: "extension".into(),
            mount_path: "Plug-Ins".into(),
        }],
    });
    validate::entry(e).unwrap();
    e.plugins[0].install[0].mount_path = "../outside".into();
    assert!(validate::entry(e).is_err());
    e.plugins[0].install[0].mount_path = "Plug-Ins".into();
    e.plugins[0].download_artifact = "missing".into();
    assert!(validate::entry(e).is_err());
    e.plugins[0].download_artifact = "extension".into();
    e.runtime.launch_modifiers = vec![LaunchModifier::Option, LaunchModifier::Option];
    assert!(validate::entry(e).is_err());
    e.runtime.launch_modifiers.pop();
    validate::entry(e).unwrap();
    let output = build(&c).unwrap();
    assert_eq!(output.entries[0].plugins.len(), 1);
    assert!(output.entries[0]
        .community
        .metadata_issue
        .contains("metadata.yml"));
    assert!(output.entries[0].community.edit.contains("/edit/"));
}

#[test]
fn plugin_collections_are_loaded_separately_and_can_be_chunked() {
    let root = repo();
    let entry = simple("test");
    save(root.path(), &entry, "");
    let collection = plugin_collection("test");
    save_plugins(root.path(), "test-01", &collection);

    let source = load(root.path(), Mode::Production).unwrap();
    assert_eq!(source.plugin_documents.len(), 1);
    assert_eq!(source.documents[0].entry.plugins, collection.plugins);
    assert!(source.documents[0]
        .entry
        .artifacts
        .iter()
        .any(|artifact| artifact.id == "optional-content"));
    assert_eq!(build(&source).unwrap().entries[0].plugins.len(), 1);
    let serialized = catalogue::serialize_entry_document(&source, &source.documents[0]).unwrap();
    assert!(!serialized.contains("optional-content"));

    let mut second = collection.clone();
    second.plugins[0].id = "another-plugin".into();
    second.plugins[0].label = "Another plugin".into();
    second.plugins[0].download_artifact = "another-artifact".into();
    second.artifacts[0].id = "another-artifact".into();
    save_plugins(root.path(), "test-02", &second);
    assert_eq!(
        load(root.path(), Mode::Production).unwrap().documents[0]
            .entry
            .plugins
            .len(),
        2
    );
}

#[test]
fn plugin_collection_boundaries_are_strict() {
    let root = repo();
    let mut entry = simple("test");
    save(root.path(), &entry, "");
    let mut collection = plugin_collection("missing");
    save_plugins(root.path(), "test-01", &collection);
    assert!(load(root.path(), Mode::Production).is_err());

    collection.entry = "test".into();
    collection.artifacts[0].source = AssetSource::Sha256 {
        sha256: hash(1),
        size_bytes: 123,
    };
    save_plugins(root.path(), "test-01", &collection);
    assert!(load(root.path(), Mode::Production).is_err());

    fs::remove_dir_all(root.path().join("catalogue/plugins")).unwrap();
    entry.plugins = plugin_collection("test").plugins;
    save(root.path(), &entry, "");
    assert!(load(root.path(), Mode::Production).is_err());
}

#[test]
fn promotion_does_not_inline_separate_plugins() {
    let root = repo();
    screenshot(root.path(), "test");
    save_plugins(root.path(), "test-01", &plugin_collection("test"));
    let store = tempfile::tempdir().unwrap();
    let mut backend = assets::DirectoryStore {
        root: store.path().into(),
    };
    assets::promote(root.path(), None, true, &mut backend).unwrap();
    let entry_source = fs::read_to_string(root.path().join("catalogue/test.md")).unwrap();
    assert!(!entry_source.contains("optional-content"));
    assert_eq!(
        load(root.path(), Mode::Production).unwrap().documents[0]
            .entry
            .plugins
            .len(),
        1
    );
}

#[test]
fn shipped_games_retain_their_mobile_keyboard_controls() {
    let repository_root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    // This regression inspects committed controls, including while a draft
    // catalogue PR still contains source-pinned entries awaiting promotion.
    // The workflow's explicit `check --production` remains the release gate.
    let source = load(&repository_root, Mode::Preview).unwrap();
    let games = compiled(&source);
    for id in [
        "marathon",
        "marathon-2-durandal",
        "marathon-infinity",
        "escape-velocity",
        "escape-velocity-override",
    ] {
        let game = games.entries.iter().find(|entry| entry.id == id).unwrap();
        let controls = &game.controls;
        assert!(controls.mobile.enabled, "{id} must expose touch controls");
        assert_eq!(controls.mobile.joystick.up, "ArrowUp");
        if id.starts_with("marathon") {
            assert!(
                controls.arrows_as_numpad,
                "{id} uses the classic keypad layout"
            );
            assert_eq!(
                controls
                    .mobile
                    .buttons
                    .iter()
                    .map(|b| b.key.as_str())
                    .collect::<Vec<_>>(),
                ["Space", "Tab", "Enter"]
            );
        } else {
            let keys = controls
                .mobile
                .button_groups
                .iter()
                .flat_map(|g| &g.buttons)
                .map(|b| b.key.as_str())
                .collect::<Vec<_>>();
            for key in ["Space", "J", "L", "M", "Enter", "Escape"] {
                assert!(keys.contains(&key), "{id} is missing {key}");
            }
        }
    }
}
