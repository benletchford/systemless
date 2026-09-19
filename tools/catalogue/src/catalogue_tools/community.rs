use crate::catalogue_tools::model::{Config, Entry};
use serde::{Deserialize, Serialize};
use url::Url;

const SYSTEMLESS_REPOSITORY: &str = "https://github.com/benletchford/systemless";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CatalogueCommunityLinks {
    pub takedown_request: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CommunityLinks {
    pub source: String,
    pub edit: String,
    pub metadata_issue: String,
    pub compatibility_issue: String,
    pub suggest_controls_config: String,
    pub takedown_request: String,
    pub open_reports: String,
    pub history: String,
}

fn issue(config: &Config, entry: Option<&Entry>, template: &str, label: &str) -> String {
    let repo = config.repository.trim_end_matches('/');
    let mut url = Url::parse(&format!("{repo}/issues/new")).expect("validated repository URL");
    url.query_pairs_mut().append_pair("template", template);
    if let Some(entry) = entry {
        url.query_pairs_mut()
            .append_pair(
                "title",
                &format!("[{}] {}: {}", entry.id, label, entry.title),
            )
            .append_pair("entry", &entry.id);
    }
    url.to_string()
}

/// Config must first pass catalogue validation. These links also work without entries.
pub fn catalogue_links(config: &Config) -> CatalogueCommunityLinks {
    CatalogueCommunityLinks {
        takedown_request: issue(config, None, "takedown.yml", "Takedown request"),
    }
}

/// Config and entry must first pass catalogue validation.
pub fn links(config: &Config, entry: &Entry) -> CommunityLinks {
    let repo = config.repository.trim_end_matches('/');
    let path = format!("{}/catalogue/{}.md", config.branch, entry.id);
    let issue = |template, label| issue(config, Some(entry), template, label);
    let mut compatibility_issue = Url::parse(&format!("{SYSTEMLESS_REPOSITORY}/issues/new"))
        .expect("constant repository URL is valid");
    compatibility_issue
        .query_pairs_mut()
        .append_pair("labels", &entry.id)
        .append_pair("title", &format!("{} compatibility: ", entry.title))
        .append_pair(
            "body",
            &format!(
                "Game ID: `{}`\n\nWhat happened?\n\nHow can it be reproduced?\n",
                entry.id
            ),
        );
    let mut reports = Url::parse(&format!("{SYSTEMLESS_REPOSITORY}/issues"))
        .expect("constant repository URL is valid");
    reports
        .query_pairs_mut()
        .append_pair("q", &format!("is:issue is:open label:{}", entry.id));
    CommunityLinks {
        source: format!("{repo}/blob/{path}"),
        edit: format!("{repo}/edit/{path}"),
        metadata_issue: issue("metadata.yml", "Metadata"),
        history: format!("{repo}/commits/{path}"),
        compatibility_issue: compatibility_issue.to_string(),
        suggest_controls_config: issue("controls-config.yml", "Controls/config"),
        takedown_request: issue("takedown.yml", "Takedown request"),
        open_reports: reports.to_string(),
    }
}

pub fn canonical_path(entry: &Entry) -> String {
    entry
        .route
        .clone()
        .unwrap_or_else(|| format!("/{}", entry.id))
}
