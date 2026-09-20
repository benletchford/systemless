//! One bounded YAML policy for every typed catalogue input.
use anyhow::{ensure, Result};
use serde::Deserialize;

pub fn from_str<'de, T: Deserialize<'de>>(input: &'de str) -> Result<T> {
    ensure!(input.len() <= 8 * 1024 * 1024, "YAML exceeds 8 MiB");
    let options = serde_saphyr::options! {
        duplicate_keys: serde_saphyr::DuplicateKeyPolicy::Error,
        strict_booleans: true,
        reject_unsupported_tags: true,
        merge_keys: serde_saphyr::MergeKeyPolicy::Error,
        alias_limits: serde_saphyr::alias_limits! {
            max_total_replayed_events: 50_000,
            max_replay_stack_depth: 16,
            max_alias_expansions_per_anchor: 128,
        },
        budget: serde_saphyr::budget! {
            max_depth: 32,
            max_events: 300_000,
            max_nodes: 150_000,
            max_total_scalar_bytes: 8 * 1024 * 1024,
            max_anchors: 128,
            max_aliases: 128,
            max_documents: 1,
        },
    };
    Ok(serde_saphyr::from_str_with_options(input, options)?)
}
