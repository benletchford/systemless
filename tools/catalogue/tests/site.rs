mod common;
use systemless_catalogue_tools::catalogue_tools::{
    catalogue, site, ArtifactRole, Plugin, PluginInstall,
};
const TEMPLATE: &str = "<html><head><!-- systemless-seo --><!-- /systemless-seo --></head><body><div id=\"root\"><!-- systemless-fallback --><!-- /systemless-fallback --></div><script src=\"/app-123.js\"></script></body></html>";

#[test]
fn pages_and_browser_records_are_independent_of_document_order() {
    let mut source = common::catalogue();
    let a = catalogue::build(&source).unwrap();
    source.documents.reverse();
    let b = catalogue::build(&source).unwrap();
    assert_eq!(site::rust_games(&a).unwrap(), site::rust_games(&b).unwrap());
    let first = site::pages(&a, TEMPLATE, "https://systemless.org").unwrap();
    assert_eq!(
        first,
        site::pages(&b, TEMPLATE, "https://systemless.org").unwrap()
    );
    assert_eq!(
        first,
        site::pages(&b, &first["index.html"], "https://systemless.org").unwrap()
    );
    assert!(first.keys().all(|p| !p.ends_with(".json")));
}

#[test]
fn aliases_metadata_plugins_and_contribution_links_exist_before_javascript() {
    let mut source = common::catalogue();
    source.documents.truncate(1);
    let e = &mut source.documents[0].entry;
    e.launch_enabled = true;
    e.route = Some("/example".into());
    e.aliases = vec!["/old-example".into()];
    e.title = "A <script>alert(1)</script> & \"quoted\" title".into();
    e.artifacts[0].role = ArtifactRole::Archive;
    e.artifacts[0].provenance.original = true;
    let mut addon = e.artifacts[0].clone();
    addon.id = "addon".into();
    addon.role = ArtifactRole::Supplement;
    addon.format = systemless_catalogue_tools::catalogue_tools::FileType::Bin;
    addon.source = systemless_catalogue_tools::catalogue_tools::AssetSource::External {
        url: "https://example.org/addon.bin".into(),
    };
    e.artifacts.push(addon);
    e.plugins = vec![Plugin {
        id: "extra".into(),
        label: "Extra & more".into(),
        description: "Optional content".into(),
        download_artifact: "addon".into(),
        install: vec![PluginInstall {
            artifact: "addon".into(),
            mount_path: "Plug-ins".into(),
        }],
    }];
    let c = catalogue::build(&source).unwrap();
    let pages = site::pages(&c, TEMPLATE, "https://systemless.org").unwrap();
    let game = &pages["example/index.html"];
    assert_eq!(game, &pages["old-example/index.html"]);
    assert!(game.contains("A &lt;script&gt;alert(1)&lt;/script&gt; &amp; &quot;quoted&quot; title"));
    assert!(!game.contains("<script>alert(1)</script>"));
    assert!(game.contains("\\u003cscript>"));
    assert!(game.contains("/edit/master/catalogue/sample-app.md"));
    assert!(game.contains("metadata.yml") && game.contains("takedown.yml"));
    assert!(game.contains("/benletchford/systemless/issues"));
    assert!(game.contains("label%3Asample-app"));
    assert!(game.contains("Extra &amp; more"));
    assert!(game.contains("download=\"sample-app.png\""));
    assert!(game.contains("download=\"sample-app-extra.bin\""));
    assert!(game.contains("/tree/master/plugins"));
    assert!(game.contains("/app-123.js"));
    assert!(pages["_redirects"].contains("/old-example/ /example/ 301"));
    assert!(pages["sitemap.xml"].contains("https://systemless.org/example/"));
    assert!(!pages["sitemap.xml"].contains("old-example"));
    let rust = site::rust_games(&c).unwrap();
    assert!(rust.contains("mount_path: \"Plug-ins\""));
    assert!(rust.contains("archive_download_name: \"sample-app.png\""));
    assert!(rust.contains("download_name: \"sample-app-extra.bin\""));
    assert!(!rust.contains("CATALOGUE_URL"));
    source.documents.clear();
    let empty = site::pages(
        &catalogue::build(&source).unwrap(),
        TEMPLATE,
        "https://systemless.org",
    )
    .unwrap();
    assert!(!empty.contains_key("example/index.html"));
    assert!(!empty["index.html"].contains("sample-app"));
}

#[test]
fn invalid_routes_and_missing_templates_fail_the_build() {
    let mut source = common::catalogue();
    for route in ["/next", "/assets/game", "/catalogue/entries", "/../outside"] {
        source.documents[0].entry.route = Some(route.into());
        assert!(catalogue::build(&source).is_err());
    }
    let c = catalogue::build(&common::catalogue()).unwrap();
    assert!(site::pages(&c, "no markers", "https://systemless.org").is_err());
    assert!(site::pages(&c, TEMPLATE, "https://systemless.org/path").is_err());
}

#[test]
fn plugin_sizes_that_the_browser_cannot_represent_fail_at_build_time() {
    let mut c = catalogue::build(&common::catalogue()).unwrap();
    let e = &mut c.entries[0];
    e.assets[0].role = ArtifactRole::Supplement;
    e.assets[0].size_bytes = Some(u64::from(u32::MAX) + 1);
    e.plugins.push(Plugin {
        id: "too-large".into(),
        label: "Too large".into(),
        description: "Example".into(),
        download_artifact: e.assets[0].id.clone(),
        install: vec![],
    });
    assert!(site::rust_games(&c).is_err());
}

#[test]
fn compatibility_reports_are_rejected_from_catalogue_prose() {
    let mut source = common::catalogue();
    source.documents[0].markdown =
        "## About\nA game.\n\n## Compatibility\nA browser-specific defect.\n".into();
    let error = catalogue::build(&source).unwrap_err().to_string();
    assert!(error.contains("labelled Systemless issues"));
}

#[test]
fn license_evidence_is_escaped_and_present_before_javascript() {
    let mut compiled = catalogue::build(&common::catalogue()).unwrap();
    let entry = &mut compiled.entries[0];
    entry.launch_enabled = false;
    let archive = &mut entry.assets[0];
    archive.role = ArtifactRole::Archive;
    archive.rights.license = Some("Custom <license> & terms".into());
    archive.rights.rights_holder = Some("Example & Company".into());
    archive.rights.permission = Some("Permission <script>alert(1)</script>".into());
    archive.rights.sources = vec!["https://example.org/terms?a=1&b=2".into()];
    let path = format!("{}/index.html", entry.path.trim_matches('/'));
    let pages = site::pages(&compiled, TEMPLATE, "https://systemless.org").unwrap();
    let html = &pages[&path];
    assert!(html.contains("<h2>License</h2>"));
    assert!(html.contains("Custom &lt;license&gt; &amp; terms"));
    assert!(html.contains("Example &amp; Company"));
    assert!(html.contains("Permission &lt;script&gt;alert(1)&lt;/script&gt;"));
    assert!(!html.contains("<script>alert(1)</script>"));
    assert!(html.contains("https://example.org/terms?a=1&amp;b=2"));
    assert!(html.contains("Screenshots, plugins and other files may have different terms"));
    let rust = site::rust_games(&compiled).unwrap();
    assert!(rust.contains("license_html:"));
    assert!(rust.contains("Custom &lt;license&gt; &amp; terms"));
}

#[test]
fn missing_license_does_not_imply_permission_or_an_open_source_license() {
    let mut compiled = catalogue::build(&common::catalogue()).unwrap();
    let entry = &mut compiled.entries[0];
    let archive = &mut entry.assets[0];
    archive.role = ArtifactRole::Archive;
    archive.rights.license = None;
    archive.rights.rights_holder = None;
    archive.rights.permission = None;
    archive.rights.redistribution =
        systemless_catalogue_tools::catalogue_tools::Redistribution::Unknown;
    let path = format!("{}/index.html", entry.path.trim_matches('/'));
    let pages = site::pages(&compiled, TEMPLATE, "https://systemless.org").unwrap();
    assert!(pages[&path].contains("No license recorded"));
    assert!(pages[&path].contains("Not established in this entry"));
    compiled.entries[0]
        .assets
        .retain(|a| a.role != ArtifactRole::Archive);
    let pages = site::pages(&compiled, TEMPLATE, "https://systemless.org").unwrap();
    assert!(pages[&path].contains("No game archive licensing information is recorded"));
}

#[test]
fn disabled_games_are_not_advertised_in_the_public_library_or_sitemap() {
    let mut compiled = catalogue::build(&common::catalogue()).unwrap();
    compiled.entries[0].launch_enabled = false;
    compiled.entries[1].launch_enabled = true;
    let disabled = &compiled.entries[0];
    let enabled = &compiled.entries[1];
    let pages = site::pages(&compiled, TEMPLATE, "https://systemless.org").unwrap();
    assert!(!pages["index.html"].contains(&disabled.title));
    assert!(pages["index.html"].contains(&enabled.title));
    assert!(pages["next/index.html"].contains(&disabled.title));
    assert!(pages["next/index.html"].contains(&enabled.title));
    assert!(!pages["sitemap.xml"].contains(&format!("{}/", disabled.path)));
    assert!(pages["sitemap.xml"].contains(&format!("{}/", enabled.path)));
    let page = &pages[&format!("{}/index.html", disabled.path.trim_matches('/'))];
    assert!(page.contains("noindex,nofollow"));
}
