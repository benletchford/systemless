//! Compile checked-in catalogue documents directly into the website and browser.
use crate::catalogue_tools::{
    catalogue::{CompiledAsset, CompiledCatalogue, CompiledEntry},
    model::*,
};
use anyhow::{ensure, Context, Result};
use std::{collections::BTreeMap, fmt::Write as _, path::Path};

fn arch(a: Architecture) -> &'static str {
    match a {
        Architecture::M68k => "GameArchitecture::M68k",
        Architecture::Ppc => "GameArchitecture::PowerPc",
    }
}
fn strings(values: &[String]) -> String {
    format!(
        "&[{}]",
        values
            .iter()
            .map(|s| format!("{s:?}"))
            .collect::<Vec<_>>()
            .join(",")
    )
}
fn buttons(values: &[Button]) -> String {
    format!(
        "&[{}]",
        values
            .iter()
            .map(|b| format!(
                "MobileControlButton {{ label: {:?}, key: {:?} }}",
                b.label, b.key
            ))
            .collect::<Vec<_>>()
            .join(",")
    )
}
fn artifact(e: &CompiledEntry, role: ArtifactRole) -> Option<&CompiledAsset> {
    e.assets.iter().find(|a| a.role == role)
}
fn asset(e: &CompiledEntry, role: ArtifactRole) -> Option<&str> {
    artifact(e, role).map(|a| a.url.as_str())
}
fn download_name(entry_id: &str, item_id: Option<&str>, format: FileType) -> String {
    match item_id {
        Some(item_id) => format!("{entry_id}-{item_id}.{}", format.ext()),
        None => format!("{entry_id}.{}", format.ext()),
    }
}
/// Rust literals avoid a second wire schema, runtime decoder, or network request.
pub fn rust_games(c: &CompiledCatalogue) -> Result<String> {
    let mut out = String::from("pub static GAMES: &[Game] = &[\n");
    for e in &c.entries {
        let r = &e.runtime;
        let p = &r.runtime_pacing;
        let m = &e.controls.mobile;
        let j = &m.joystick;
        let links = &e.community;
        write!(
            out,
            "Game {{ id: {:?}, title: {:?}, description: {:?}, developer: {:?}, year: {:?}, architectures: &[{}], default_architecture: {}, category: {:?}, route: {:?}, approved: {}, route_aliases: {}, content_html: {:?}, license_html: {:?},",
            e.id,
            e.title,
            e.summary,
            e.developer,
            e.year.to_string(),
            e.architectures
                .iter()
                .map(|a| arch(*a))
                .collect::<Vec<_>>()
                .join(","),
            arch(e.default_architecture),
            e.category,
            e.path,
            e.launch_enabled,
            strings(&e.aliases),
            e.content_html,
            license_html(e)?
        )?;
        let archive = artifact(e, ArtifactRole::Archive);
        write!(
            out,
            "assets: GameAssets {{ archive_path: {:?}, archive_download_name: {:?}, web_pack_path: {:?}, screenshot_path: {:?} }},",
            archive.map(|a| a.url.as_str()).unwrap_or(""),
            archive
                .map(|a| download_name(&e.id, None, a.format))
                .unwrap_or_default(),
            asset(e, ArtifactRole::WebPack),
            asset(e, ArtifactRole::Screenshot).unwrap_or("")
        )?;
        write!(
            out,
            "community: CommunityLinks {{ source: {:?}, edit: {:?}, metadata_issue: {:?}, compatibility_issue: {:?}, takedown_request: {:?}, open_reports: {:?}, suggest_controls_config: {:?} }},",
            links.source,
            links.edit,
            links.metadata_issue,
            links.compatibility_issue,
            links.takedown_request,
            links.open_reports,
            links.suggest_controls_config
        )?;
        write!(
            out,
            "settings: GameSettings {{ worker: {}, arrows_as_numpad: {}, show_menu_bar: {}, screen_depth: {:?}, application_partition_size: {:?}, remove_paths: {}, file_mappings: &[{}], key_mappings: &[{}], launch_modifiers: &[{}], runtime_pacing: RuntimePacing {{max_ticks_per_paint: {}, reset_slack_ticks: {}, cpu_mhz: {}}},",
            r.worker,
            e.controls.arrows_as_numpad,
            r.show_menu_bar,
            r.screen_depth,
            r.application_partition_size,
            strings(&r.remove_paths),
            r.file_mappings
                .iter()
                .map(|(from, to)| format!("({from:?},{to:?})"))
                .collect::<Vec<_>>()
                .join(","),
            e.controls
                .key_mappings
                .iter()
                .map(|(k, v)| format!("({k:?},{v:?})"))
                .collect::<Vec<_>>()
                .join(","),
            r.launch_modifiers
                .iter()
                .map(|v| format!("LaunchModifier::{v:?}"))
                .collect::<Vec<_>>()
                .join(","),
            p.max_ticks_per_paint,
            p.reset_slack_ticks,
            p.cpu_mhz
        )?;
        write!(
            out,
            "mobile_controls: MobileControls {{enabled: {}, joystick: MobileJoystickControls {{up: {:?}, down: {:?}, left: {:?}, right: {:?}}}, buttons: {}, button_groups: &[{}]}}, plugins: &[",
            m.enabled,
            j.up,
            j.down,
            j.left,
            j.right,
            buttons(&m.buttons),
            m.button_groups
                .iter()
                .map(|g| format!(
                    "MobileControlButtonGroup {{label: {:?}, buttons: {}}}",
                    g.label,
                    buttons(&g.buttons)
                ))
                .collect::<Vec<_>>()
                .join(",")
        )?;
        for plugin in &e.plugins {
            let download = e
                .assets
                .iter()
                .find(|a| a.id == plugin.download_artifact)
                .context("missing plugin download")?;
            let size = u32::try_from(download.size_bytes.unwrap_or(0))
                .context("plugin exceeds browser size limit")?;
            write!(
                out,
                "GamePlugin {{id: {:?}, label: {:?}, description: {:?}, download_path: {:?}, download_name: {:?}, size_bytes: {}, install_assets: &[",
                plugin.id,
                plugin.label,
                plugin.description,
                download.url,
                download_name(&e.id, Some(&plugin.id), download.format),
                size
            )?;
            for install in &plugin.install {
                let file = e
                    .assets
                    .iter()
                    .find(|a| a.id == install.artifact)
                    .context("missing plugin installation asset")?;
                write!(
                    out,
                    "GamePluginAsset {{asset_path: {:?}, mount_path: {:?}}},",
                    file.url, install.mount_path
                )?;
            }
            out.push_str("]},");
        }
        out.push_str("]}},\n");
    }
    out.push_str("];\n");
    Ok(out)
}

const HOME_DESCRIPTION: &str = "Play classic 68K and PowerPC Macintosh games in the browser with Systemless, a ROM-free classic Mac runtime for playable preservation.";
/// Present the game archive's recorded rights without applying them to other artifacts.
fn license_html(entry: &CompiledEntry) -> Result<String> {
    let mut html = String::from("<section><h2>License</h2><p>Licensing information recorded for this game archive. These terms are separate from the Systemless runtime and website licenses.</p>");
    let archives: Vec<_> = entry
        .assets
        .iter()
        .filter(|a| a.role == ArtifactRole::Archive)
        .collect();
    if archives.is_empty() {
        html.push_str("<p>No game archive licensing information is recorded for this entry.</p>");
    }
    for archive in archives {
        let rights = &archive.rights;
        html.push_str("<h3>Game archive</h3><dl>");
        for (label, value) in [
            (
                "License",
                rights.license.as_deref().unwrap_or("No license recorded"),
            ),
            (
                "Rights holder",
                rights.rights_holder.as_deref().unwrap_or("Not recorded"),
            ),
            (
                "Redistribution",
                match rights.redistribution {
                    Redistribution::Unknown => "Not established in this entry",
                    Redistribution::Permitted => "Recorded as permitted",
                    Redistribution::Forbidden => "Recorded as forbidden",
                },
            ),
        ] {
            write!(html, "<dt>{label}</dt><dd>{}</dd>", escape(value))?;
        }
        html.push_str("</dl>");
        if let Some(permission) = &rights.permission {
            write!(html, "<h4>Permission</h4><p>{}</p>", escape(permission))?;
        }
        if let Some(notes) = &rights.notes {
            write!(html, "<h4>Notes</h4><p>{}</p>", escape(notes))?;
        }
        if !rights.sources.is_empty() {
            html.push_str("<h4>Sources</h4><ul>");
            for (index, url) in rights.sources.iter().enumerate() {
                write!(html, "<li><a href=\"{}\" target=\"_blank\" rel=\"noopener noreferrer\">Source {}</a></li>", escape(url), index + 1)?;
            }
            html.push_str("</ul>");
        }
    }
    let records = entry
        .community
        .source
        .rsplit_once("/catalogue/")
        .map(|(prefix, _)| format!("{}/catalogue/plugins", prefix.replace("/blob/", "/tree/")))
        .filter(|_| !entry.plugins.is_empty())
        .unwrap_or_else(|| entry.community.source.clone());
    write!(html, "<p>Screenshots, plugins and other files may have different terms. See their individual catalogue records in the <a href=\"{}\" target=\"_blank\" rel=\"noopener noreferrer\">repository source</a>.</p></section>", escape(&records))?;
    Ok(html)
}

fn escape(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}
fn route(path: &str) -> String {
    let p = path.trim_matches('/');
    if p.is_empty() {
        "/".into()
    } else {
        format!("/{p}/")
    }
}
fn replace_block(template: &str, name: &str, body: &str) -> Result<String> {
    let start = format!("<!-- systemless-{name} -->");
    let end = format!("<!-- /systemless-{name} -->");
    let (before, tail) = template.split_once(&start).context("missing page marker")?;
    let (_, after) = tail
        .split_once(&end)
        .context("missing closing page marker")?;
    Ok(format!("{before}{start}\n{body}\n{end}{after}"))
}
struct Meta<'a> {
    title: &'a str,
    description: &'a str,
    canonical: Option<String>,
    image: String,
    robots: &'a str,
    schema: Option<serde_json::Value>,
}
fn page(template: &str, meta: Meta<'_>, body: &str) -> Result<String> {
    let title = escape(meta.title);
    let description = escape(meta.description);
    let image = escape(&meta.image);
    let mut seo = format!(
        "<title>{title}</title>\n<meta name=\"description\" content=\"{description}\">\n<meta name=\"robots\" content=\"{}\">\n<meta property=\"og:type\" content=\"website\">\n<meta property=\"og:site_name\" content=\"Systemless\">\n<meta property=\"og:title\" content=\"{title}\">\n<meta property=\"og:description\" content=\"{description}\">\n<meta property=\"og:image\" content=\"{image}\">\n<meta name=\"twitter:card\" content=\"summary_large_image\">\n<meta name=\"twitter:title\" content=\"{title}\">\n<meta name=\"twitter:description\" content=\"{description}\">\n<meta name=\"twitter:image\" content=\"{image}\">",
        meta.robots
    );
    if let Some(url) = meta.canonical {
        write!(
            seo,
            "\n<link rel=\"canonical\" href=\"{}\">\n<meta property=\"og:url\" content=\"{}\">",
            escape(&url),
            escape(&url)
        )?;
    }
    if let Some(schema) = meta.schema {
        write!(
            seo,
            "\n<script type=\"application/ld+json\" data-systemless-schema=\"true\">{}</script>",
            serde_json::to_string(&schema)?.replace('<', "\\u003c")
        )?;
    }
    let html = replace_block(template, "seo", &seo)?;
    replace_block(
        &html,
        "fallback",
        &format!("<main class=\"seo-fallback\">{body}</main>"),
    )
}
/// A pure ordered file map: no network, clocks, random ordering or machine paths.
pub fn pages(
    c: &CompiledCatalogue,
    template: &str,
    origin: &str,
) -> Result<BTreeMap<String, String>> {
    let origin = url::Url::parse(origin)?;
    ensure!(
        origin.scheme() == "https"
            && origin.username().is_empty()
            && origin.password().is_none()
            && origin.path() == "/"
            && origin.query().is_none()
            && origin.fragment().is_none(),
        "site origin must be an HTTPS origin"
    );
    for e in &c.entries {
        crate::catalogue_tools::validate::route(&e.path)?;
        for alias in &e.aliases {
            crate::catalogue_tools::validate::route(alias)?;
        }
    }
    let absolute = |p: &str| -> String { origin.join(p).expect("validated route").to_string() };
    let default_image = absolute("/assets/icons/favicon.svg");
    let mut files = BTreeMap::new();
    for (path, robots) in [("/", "index,follow"), ("/next", "noindex,nofollow")] {
        let mut home = format!(
            "<h1>Classic 68K and PowerPC Macintosh games in your browser</h1><p>{HOME_DESCRIPTION}</p><ul>"
        );
        for e in c
            .entries
            .iter()
            .filter(|entry| path == "/next" || entry.launch_enabled)
        {
            write!(
                home,
                "<li><a href=\"{}\">{}</a><p>{}</p></li>",
                escape(&route(&e.path)),
                escape(&e.title),
                escape(&e.summary)
            )?;
        }
        home.push_str("</ul>");
        let html = page(
            template,
            Meta {
                title: "Systemless | Play classic 68K and PowerPC Mac games in your browser",
                description: HOME_DESCRIPTION,
                canonical: Some(absolute(&route(path))),
                image: default_image.clone(),
                robots,
                schema: if path == "/" {
                    Some(
                        serde_json::json!({"@context":"https://schema.org","@type":"WebSite","name":"Systemless","url":absolute("/")}),
                    )
                } else {
                    None
                },
            },
            &home,
        )?;
        files.insert(
            format!("{}index.html", route(path).trim_start_matches('/')),
            html,
        );
    }
    let mut redirects = String::new();
    let mut sitemap = String::from(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<urlset xmlns=\"http://www.sitemaps.org/schemas/sitemap/0.9\">\n",
    );
    writeln!(sitemap, "<url><loc>{}</loc></url>", escape(&absolute("/")))?;
    for e in &c.entries {
        let canonical = absolute(&route(&e.path));
        let image = asset(e, ArtifactRole::Screenshot)
            .unwrap_or(&default_image)
            .to_string();
        let title = format!(
            "{}{} ({}) | Systemless",
            if e.launch_enabled { "Play " } else { "" },
            e.title,
            e.year
        );
        let mut body = format!(
            "<p><a href=\"/\">Systemless game library</a></p><h1>{}</h1><p>{}</p><p>{}, {}</p><img src=\"{}\" alt=\"{} gameplay\">",
            escape(&e.title),
            escape(&e.summary),
            escape(&e.developer),
            e.year,
            escape(&image),
            escape(&e.title)
        );
        if let Some(download) = artifact(e, ArtifactRole::Archive) {
            write!(
                body,
                "<p><a href=\"{}\" download=\"{}\">Download {}</a></p>",
                escape(&download.url),
                escape(&download_name(&e.id, None, download.format)),
                escape(&e.title)
            )?;
        }
        body.push_str(&e.content_html);
        body.push_str(&license_html(e)?);
        body.push_str("<nav aria-label=\"Contribute to this catalogue entry\">");
        for (label, url) in [
            ("Report compatibility", &e.community.compatibility_issue),
            ("Issues", &e.community.open_reports),
            ("Edit metadata / propose a PR", &e.community.edit),
            ("Suggest metadata correction", &e.community.metadata_issue),
            ("Suggest controls", &e.community.suggest_controls_config),
            ("Request takedown", &e.community.takedown_request),
        ] {
            write!(body, "<a href=\"{}\">{label}</a> ", escape(url))?;
        }
        body.push_str("</nav>");
        if !e.plugins.is_empty() {
            body.push_str("<section><h2>Plugins</h2><ul>");
        }
        for plugin in &e.plugins {
            let download = e
                .assets
                .iter()
                .find(|a| a.id == plugin.download_artifact)
                .context("missing plugin download")?;
            write!(
                body,
                "<li><a href=\"{}\" download=\"{}\">{}</a><p>{}</p></li>",
                escape(&download.url),
                escape(&download_name(&e.id, Some(&plugin.id), download.format)),
                escape(&plugin.label),
                escape(&plugin.description)
            )?;
        }
        if !e.plugins.is_empty() {
            body.push_str("</ul></section>");
        }
        let html = page(
            template,
            Meta {
                title: &title,
                description: &e.summary,
                canonical: Some(canonical.clone()),
                image: image.clone(),
                robots: if e.launch_enabled { "index,follow" } else { "noindex,nofollow" },
                schema: e.launch_enabled.then(||
                    serde_json::json!({"@context":"https://schema.org","@type":"VideoGame","name":e.title,"description":e.summary,"url":canonical,"image":image,"datePublished":e.year.to_string(),"gamePlatform":"Classic Macintosh","operatingSystem":"Classic Mac OS","creator":{"@type":"Organization","name":e.developer}}),
                ),
            },
            &body,
        )?;
        for p in std::iter::once(&e.path).chain(&e.aliases) {
            files.insert(
                format!("{}index.html", route(p).trim_start_matches('/')),
                html.clone(),
            );
            writeln!(
                redirects,
                "{} {} 301",
                p.trim_end_matches('/'),
                route(&e.path)
            )?;
            if p != &e.path {
                writeln!(redirects, "{} {} 301", route(p), route(&e.path))?;
            }
        }
        if e.launch_enabled {
            writeln!(sitemap, "<url><loc>{}</loc></url>", escape(&canonical))?;
        }
    }
    sitemap.push_str("</urlset>\n");
    files.insert("sitemap.xml".into(), sitemap);
    files.insert("_redirects".into(), redirects);
    files.insert(
        "robots.txt".into(),
        format!(
            "User-agent: *\nAllow: /\nSitemap: {}\n",
            absolute("/sitemap.xml")
        ),
    );
    files.insert(
        "404.html".into(),
        page(
            template,
            Meta {
                title: "Page not found | Systemless",
                description: HOME_DESCRIPTION,
                canonical: None,
                image: default_image,
                robots: "noindex,follow",
                schema: None,
            },
            "<h1>Page not found</h1><p><a href=\"/\">Systemless game library</a></p>",
        )?,
    );
    Ok(files)
}

pub fn write_pages(
    c: &CompiledCatalogue,
    template: &Path,
    output: &Path,
    origin: &str,
) -> Result<()> {
    let files = pages(c, &std::fs::read_to_string(template)?, origin)?;
    for (path, contents) in files {
        crate::catalogue_tools::catalogue::atomic_write(&output.join(path), contents.as_bytes())?;
    }
    Ok(())
}
