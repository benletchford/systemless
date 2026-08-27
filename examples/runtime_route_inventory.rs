use regex::Regex;
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::env;
use std::fs;
use std::path::{Path, PathBuf};

const TRAP_DISPATCHERS: &[&str] = &[
    "dispatch_memory",
    "dispatch_event",
    "dispatch_resource",
    "dispatch_quickdraw",
    "dispatch_menu",
    "dispatch_window",
    "dispatch_control",
    "dispatch_dialog",
    "dispatch_sound",
    "dispatch_toolbox",
    "dispatch_sane",
];

#[derive(Clone, Debug)]
struct TrapClaim {
    dispatcher: String,
    file: String,
    line: usize,
    is_toolbox: bool,
    slot: u16,
}

impl TrapClaim {
    fn canonical_word(&self) -> u16 {
        (if self.is_toolbox { 0xA800 } else { 0xA000 }) | self.slot
    }
}

#[derive(Serialize)]
struct Report {
    schema_version: u8,
    report_kind: &'static str,
    coverage_warning: &'static str,
    source_sha256: BTreeMap<String, String>,
    m68k: M68kInventory,
    powerpc: PpcInventory,
}

#[derive(Serialize)]
struct M68kInventory {
    dispatcher_order: &'static [&'static str],
    canonical_claim_count: usize,
    unique_canonical_claim_count: usize,
    os_unique_claim_count: usize,
    toolbox_unique_claim_count: usize,
    unmatched_canonical_slot_count: usize,
    raw_word_count_routing_to_claimed_slots: usize,
    duplicate_or_unreachable_claims: Vec<DuplicateClaim>,
    fallback_signals: M68kFallbackSignals,
}

#[derive(Serialize)]
struct DuplicateClaim {
    canonical_word: String,
    claims: Vec<ClaimLocation>,
}

#[derive(Serialize)]
struct ClaimLocation {
    dispatcher: String,
    file: String,
    line: usize,
    reachable: bool,
}

#[derive(Default, Serialize)]
struct M68kFallbackSignals {
    unknown_selector_mentions: usize,
    return_noerr_calls: usize,
}

#[derive(Serialize)]
struct PpcInventory {
    source: String,
    enum_variant_count: usize,
    mapping_arm_count_including_wildcard: usize,
    exact_library_symbol_tuple_count: usize,
    distinct_mapped_target_count_excluding_unsupported: usize,
    helper_disabled_arm_count: usize,
    fallback_target: Option<&'static str>,
    compatibility_targets: Vec<String>,
    production_target_reference_counts: BTreeMap<&'static str, usize>,
}

fn mask_non_code(source: &str, keep_strings: bool) -> String {
    let mut bytes = source.as_bytes().to_vec();
    let mut index = 0;
    let mut block_depth = 0usize;
    let mut state = State::Code;
    while index < bytes.len() {
        let current = bytes[index];
        let following = bytes.get(index + 1).copied().unwrap_or_default();
        match state {
            State::Code => {
                if current == b'/' && following == b'/' {
                    bytes[index] = b' ';
                    bytes[index + 1] = b' ';
                    index += 2;
                    state = State::LineComment;
                    continue;
                }
                if current == b'/' && following == b'*' {
                    bytes[index] = b' ';
                    bytes[index + 1] = b' ';
                    index += 2;
                    block_depth = 1;
                    state = State::BlockComment;
                    continue;
                }
                if current == b'"' {
                    if !keep_strings {
                        bytes[index] = b' ';
                    }
                    state = State::String;
                } else if current == b'\'' && bytes.get(index + 2).copied() == Some(b'\'') {
                    if !keep_strings {
                        bytes[index] = b' ';
                    }
                    state = State::Character;
                }
            }
            State::LineComment => {
                if current == b'\n' {
                    state = State::Code;
                } else {
                    bytes[index] = b' ';
                }
            }
            State::BlockComment => {
                if current == b'/' && following == b'*' {
                    bytes[index] = b' ';
                    bytes[index + 1] = b' ';
                    index += 2;
                    block_depth += 1;
                    continue;
                }
                if current == b'*' && following == b'/' {
                    bytes[index] = b' ';
                    bytes[index + 1] = b' ';
                    index += 2;
                    block_depth -= 1;
                    if block_depth == 0 {
                        state = State::Code;
                    }
                    continue;
                }
                if current != b'\n' {
                    bytes[index] = b' ';
                }
            }
            State::String | State::Character => {
                if !keep_strings && current != b'\n' {
                    bytes[index] = b' ';
                }
                if current == b'\\' {
                    if !keep_strings && index + 1 < bytes.len() {
                        bytes[index + 1] = b' ';
                    }
                    index += 2;
                    continue;
                }
                let terminator = if state == State::String { b'"' } else { b'\'' };
                if current == terminator {
                    state = State::Code;
                }
            }
        }
        index += 1;
    }
    String::from_utf8(bytes).expect("masking replaces source bytes with ASCII")
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum State {
    Code,
    LineComment,
    BlockComment,
    String,
    Character,
}

fn matching_brace(masked: &str, opening: usize) -> Result<usize, String> {
    let mut depth = 0usize;
    for (offset, byte) in masked.as_bytes()[opening..].iter().enumerate() {
        match byte {
            b'{' => depth += 1,
            b'}' => {
                depth = depth
                    .checked_sub(1)
                    .ok_or_else(|| format!("unexpected closing brace at {opening}"))?;
                if depth == 0 {
                    return Ok(opening + offset);
                }
            }
            _ => {}
        }
    }
    Err(format!("unclosed brace at {opening}"))
}

fn depth_at_offsets(masked: &str, offsets: impl Iterator<Item = usize>) -> Vec<usize> {
    let offsets: Vec<_> = offsets.collect();
    let mut result = Vec::with_capacity(offsets.len());
    let mut braces = 0usize;
    let mut cursor = 0usize;
    for offset in offsets {
        for byte in &masked.as_bytes()[cursor..offset] {
            match byte {
                b'{' => braces += 1,
                b'}' => braces = braces.saturating_sub(1),
                _ => {}
            }
        }
        result.push(braces);
        cursor = offset;
    }
    result
}

fn top_level_token_count(masked: &str, token: &[u8]) -> usize {
    let bytes = masked.as_bytes();
    let mut braces = 0usize;
    let mut parentheses = 0usize;
    let mut brackets = 0usize;
    let mut count = 0usize;
    let mut index = 0usize;
    while index < bytes.len() {
        match bytes[index] {
            b'{' => braces += 1,
            b'}' => braces = braces.saturating_sub(1),
            b'(' => parentheses += 1,
            b')' => parentheses = parentheses.saturating_sub(1),
            b'[' => brackets += 1,
            b']' => brackets = brackets.saturating_sub(1),
            _ => {}
        }
        if braces == 0 && parentheses == 0 && brackets == 0 && bytes[index..].starts_with(token) {
            count += 1;
            index += token.len();
        } else {
            index += 1;
        }
    }
    count
}

fn named_body(source: &str, declaration: &Regex) -> Result<(usize, usize), String> {
    let masked = mask_non_code(source, false);
    let found = declaration
        .find(&masked)
        .ok_or_else(|| format!("declaration not found: {}", declaration.as_str()))?;
    let opening = masked[found.end()..]
        .find('{')
        .map(|relative| found.end() + relative)
        .ok_or_else(|| format!("body not found: {}", declaration.as_str()))?;
    Ok((opening + 1, matching_brace(&masked, opening)?))
}

fn rust_trap_files(root: &Path) -> Result<Vec<PathBuf>, String> {
    let mut paths = fs::read_dir(root.join("src/trap"))
        .map_err(|error| error.to_string())?
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.extension().and_then(|value| value.to_str()) == Some("rs"))
        .collect::<Vec<_>>();
    paths.sort();
    Ok(paths)
}

fn relative(root: &Path, path: &Path) -> Result<String, String> {
    Ok(path
        .strip_prefix(root)
        .map_err(|error| error.to_string())?
        .to_string_lossy()
        .replace('\\', "/"))
}

fn trap_claims(root: &Path) -> Result<Vec<TrapClaim>, String> {
    let tuple = Regex::new(r"\(\s*(true|false)\s*,\s*(0x[0-9A-Fa-f]+)\s*\)").unwrap();
    let trap_match = Regex::new(r"\bmatch\s*\(\s*is_tool\s*,\s*trap_num\s*\)\s*\{").unwrap();
    let mut claims = Vec::new();
    let mut remaining: BTreeSet<_> = TRAP_DISPATCHERS.iter().copied().collect();
    for path in rust_trap_files(root)? {
        let source = fs::read_to_string(&path).map_err(|error| error.to_string())?;
        let code = mask_non_code(&source, false);
        for dispatcher in TRAP_DISPATCHERS {
            let declaration = Regex::new(&format!(
                r"\bfn\s+{}\s*(?:<[^{{>]*>)?\s*\(",
                regex::escape(dispatcher)
            ))
            .unwrap();
            let Some(found) = declaration.find(&code) else {
                continue;
            };
            remaining.remove(dispatcher);
            let function_open = code[found.end()..]
                .find('{')
                .map(|relative| found.end() + relative)
                .ok_or_else(|| format!("body not found for {dispatcher}"))?;
            let function_close = matching_brace(&code, function_open)?;
            let function_code = &code[function_open + 1..function_close];
            let found_match = trap_match
                .find(function_code)
                .ok_or_else(|| format!("trap match not found in {dispatcher}"))?;
            let match_open = function_open + 1 + found_match.end() - 1;
            let match_close = matching_brace(&code, match_open)?;
            let match_code = &code[match_open + 1..match_close];
            let found_claims = tuple.find_iter(match_code).collect::<Vec<_>>();
            let depths =
                depth_at_offsets(match_code, found_claims.iter().map(|found| found.start()));
            for (found_claim, depth) in found_claims.into_iter().zip(depths) {
                if depth != 0 {
                    continue;
                }
                let captures = tuple.captures(found_claim.as_str()).unwrap();
                let absolute = match_open + 1 + found_claim.start();
                claims.push(TrapClaim {
                    dispatcher: (*dispatcher).to_string(),
                    file: relative(root, &path)?,
                    line: source.as_bytes()[..absolute]
                        .iter()
                        .filter(|byte| **byte == b'\n')
                        .count()
                        + 1,
                    is_toolbox: &captures[1] == "true",
                    slot: u16::from_str_radix(&captures[2][2..], 16)
                        .map_err(|error| error.to_string())?,
                });
            }
        }
    }
    if !remaining.is_empty() {
        return Err(format!("missing trap dispatchers: {remaining:?}"));
    }
    claims.sort_by_key(|claim| {
        (
            TRAP_DISPATCHERS
                .iter()
                .position(|dispatcher| *dispatcher == claim.dispatcher)
                .expect("claim dispatcher comes from the configured chain"),
            claim.file.clone(),
            claim.line,
        )
    });
    Ok(claims)
}

fn before_tests(source: &str) -> &str {
    Regex::new(r"(?m)^#\[cfg\(test\)\]\s*\nmod\s+tests\s*\{")
        .unwrap()
        .find(source)
        .map_or(source, |found| &source[..found.start()])
}

fn ppc_inventory(root: &Path) -> Result<PpcInventory, String> {
    let path = root.join("src/loader/ppc.rs");
    let source = fs::read_to_string(&path).map_err(|error| error.to_string())?;
    let (enum_start, enum_end) = named_body(
        &source,
        &Regex::new(r"\bpub\s+enum\s+PpcImportDispatcherTarget\b").unwrap(),
    )?;
    let enum_body = mask_non_code(&source[enum_start..enum_end], false);
    let variant = Regex::new(r"(?m)^\s*([A-Z][A-Za-z0-9_]*)\b").unwrap();
    let variants = variant
        .captures_iter(&enum_body)
        .map(|captures| captures[1].to_string())
        .collect::<Vec<_>>();

    let (function_start, function_end) = named_body(
        &source,
        &Regex::new(r"\bfn\s+dispatcher_target_for_import\s*\(").unwrap(),
    )?;
    let function_code = mask_non_code(&source[function_start..function_end], false);
    let import_match =
        Regex::new(r"\bmatch\s*\(\s*library_name\s*,\s*symbol_name\s*\)\s*\{").unwrap();
    let found_match = import_match
        .find(&function_code)
        .ok_or_else(|| "PowerPC import mapper match not found".to_string())?;
    let match_open = function_start + found_match.end() - 1;
    let source_code = mask_non_code(&source, false);
    let match_close = matching_brace(&source_code, match_open)?;
    let match_source = &source[match_open + 1..match_close];
    let match_code = mask_non_code(match_source, true);
    let match_structure = mask_non_code(match_source, false);

    let pair =
        Regex::new(r#"\(\s*"([^"\\]*(?:\\.[^"\\]*)*)"\s*,\s*"([^"\\]*(?:\\.[^"\\]*)*)"\s*\)"#)
            .unwrap();
    let found_pairs = pair.find_iter(&match_code).collect::<Vec<_>>();
    let pair_depths = depth_at_offsets(
        &match_structure,
        found_pairs.iter().map(|found| found.start()),
    );
    let exact_pair_count = pair_depths.into_iter().filter(|depth| *depth == 0).count();

    let target = Regex::new(r"PpcImportDispatcherTarget::([A-Za-z0-9_]+)").unwrap();
    let mapped_targets = target
        .captures_iter(&match_code)
        .map(|captures| captures[1].to_string())
        .collect::<BTreeSet<_>>();
    let compatibility_targets = variants
        .iter()
        .filter(|name| name.ends_with("Compatibility"))
        .cloned()
        .collect();
    let production = mask_non_code(before_tests(&source), false);
    let mut production_target_reference_counts = BTreeMap::new();
    for name in ["ReturnNoErr", "NoOpPreserve", "Unsupported"] {
        let expression = Regex::new(&format!(
            r"PpcImportDispatcherTarget::{}\b",
            regex::escape(name)
        ))
        .unwrap();
        production_target_reference_counts.insert(name, expression.find_iter(&production).count());
    }
    Ok(PpcInventory {
        source: relative(root, &path)?,
        enum_variant_count: variants.len(),
        mapping_arm_count_including_wildcard: top_level_token_count(&match_structure, b"=>"),
        exact_library_symbol_tuple_count: exact_pair_count,
        distinct_mapped_target_count_excluding_unsupported: mapped_targets
            .iter()
            .filter(|name| name.as_str() != "Unsupported")
            .count(),
        helper_disabled_arm_count: Regex::new(r"\bis_quickdraw_3d_status_success_import\s*\(")
            .unwrap()
            .find_iter(&match_code)
            .count(),
        fallback_target: mapped_targets
            .contains("Unsupported")
            .then_some("Unsupported"),
        compatibility_targets,
        production_target_reference_counts,
    })
}

fn generate(root: &Path) -> Result<Report, String> {
    let claims = trap_claims(root)?;
    let mut occurrences: BTreeMap<u16, Vec<&TrapClaim>> = BTreeMap::new();
    let mut os_claims = BTreeSet::new();
    let mut toolbox_claims = BTreeSet::new();
    for claim in &claims {
        occurrences
            .entry(claim.canonical_word())
            .or_default()
            .push(claim);
        if claim.is_toolbox {
            toolbox_claims.insert(claim.slot);
        } else {
            os_claims.insert(claim.slot);
        }
    }
    let duplicate_or_unreachable_claims = occurrences
        .iter()
        .filter(|(_, claims)| claims.len() > 1)
        .map(|(word, claims)| DuplicateClaim {
            canonical_word: format!("0x{word:04X}"),
            claims: claims
                .iter()
                .enumerate()
                .map(|(index, claim)| ClaimLocation {
                    dispatcher: claim.dispatcher.clone(),
                    file: claim.file.clone(),
                    line: claim.line,
                    reachable: index == 0,
                })
                .collect(),
        })
        .collect();

    let mut source_paths = rust_trap_files(root)?;
    source_paths.push(root.join("src/loader/ppc.rs"));
    source_paths.push(root.join("examples/runtime_route_inventory.rs"));
    source_paths.sort();
    let mut source_sha256 = BTreeMap::new();
    let mut fallback_signals = M68kFallbackSignals::default();
    let unknown_selector = Regex::new(r"(?i)unknown[^\n]{0,80}selector").unwrap();
    let return_noerr = Regex::new(r"\breturn_noerr\s*\(").unwrap();
    for path in source_paths {
        let bytes = fs::read(&path).map_err(|error| error.to_string())?;
        source_sha256.insert(
            relative(root, &path)?,
            format!("{:x}", Sha256::digest(&bytes)),
        );
        if path.parent() == Some(root.join("src/trap").as_path()) {
            let source = String::from_utf8(bytes).map_err(|error| error.to_string())?;
            let production = before_tests(&source);
            fallback_signals.unknown_selector_mentions +=
                unknown_selector.find_iter(production).count();
            fallback_signals.return_noerr_calls += return_noerr
                .find_iter(&mask_non_code(production, false))
                .count();
        }
    }

    Ok(Report {
        schema_version: 1,
        report_kind: "legacy_runtime_route_inventory",
        coverage_warning:
            "Source-route counts are not API catalogue, semantic coverage, or completion evidence.",
        source_sha256,
        m68k: M68kInventory {
            dispatcher_order: TRAP_DISPATCHERS,
            canonical_claim_count: claims.len(),
            unique_canonical_claim_count: occurrences.len(),
            os_unique_claim_count: os_claims.len(),
            toolbox_unique_claim_count: toolbox_claims.len(),
            unmatched_canonical_slot_count: 1280 - occurrences.len(),
            raw_word_count_routing_to_claimed_slots: os_claims.len() * 8 + toolbox_claims.len() * 2,
            duplicate_or_unreachable_claims,
            fallback_signals,
        },
        powerpc: ppc_inventory(root)?,
    })
}

fn repository_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).to_path_buf()
}

fn run() -> Result<(), String> {
    let mut root = repository_root();
    let mut output = None;
    let mut check = None;
    let mut arguments = env::args().skip(1);
    while let Some(argument) = arguments.next() {
        match argument.as_str() {
            "--root" => root = PathBuf::from(arguments.next().ok_or("--root needs a path")?),
            "--output" => {
                output = Some(PathBuf::from(
                    arguments.next().ok_or("--output needs a path")?,
                ))
            }
            "--check" => {
                check = Some(PathBuf::from(
                    arguments.next().ok_or("--check needs a path")?,
                ))
            }
            "--help" | "-h" => {
                println!("runtime-route-inventory [--root PATH] [--output PATH] [--check PATH]");
                return Ok(());
            }
            _ => return Err(format!("unknown argument: {argument}")),
        }
    }
    let rendered =
        serde_json::to_string_pretty(&generate(&root)?).map_err(|error| error.to_string())? + "\n";
    if let Some(path) = check {
        let expected = fs::read_to_string(&path).map_err(|error| error.to_string())?;
        if expected != rendered {
            return Err(format!("inventory differs from {}", path.display()));
        }
    }
    if let Some(path) = output {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|error| error.to_string())?;
        }
        fs::write(path, rendered).map_err(|error| error.to_string())?;
    } else {
        print!("{rendered}");
    }
    Ok(())
}

fn main() {
    if let Err(error) = run() {
        eprintln!("runtime-route-inventory: {error}");
        std::process::exit(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn masks_nested_comments_without_changing_offsets() {
        let source = "fn f() { /* first { /* nested */ } */ \"text\"; }";
        let masked = mask_non_code(source, false);
        assert_eq!(source.len(), masked.len());
        let opening = masked.find('{').unwrap();
        assert_eq!(matching_brace(&masked, opening), Ok(source.len() - 1));
    }

    #[test]
    fn counts_only_top_level_tokens() {
        let source = "one => value, { nested => value }, two => value";
        assert_eq!(top_level_token_count(source, b"=>"), 2);
    }

    #[test]
    fn preserves_dispatch_order_and_alternative_claims() {
        let temporary =
            env::temp_dir().join(format!("systemless-route-inventory-{}", std::process::id()));
        let trap_directory = temporary.join("src/trap");
        fs::create_dir_all(&trap_directory).unwrap();
        let mut source = String::new();
        for dispatcher in TRAP_DISPATCHERS {
            let arms = if *dispatcher == TRAP_DISPATCHERS[0] {
                "(false, 0x70) | (true, 0x001) => {},"
            } else {
                "_ => {},"
            };
            source.push_str(&format!(
                "fn {dispatcher}(is_tool: bool, trap_num: u16) {{ match (is_tool, trap_num) {{ {arms} }} }}\n"
            ));
        }
        fs::write(trap_directory.join("dispatch.rs"), source).unwrap();
        let claims = trap_claims(&temporary).unwrap();
        fs::remove_dir_all(&temporary).unwrap();
        assert_eq!(
            claims
                .iter()
                .map(TrapClaim::canonical_word)
                .collect::<Vec<_>>(),
            vec![0xA070, 0xA801]
        );
    }
}
