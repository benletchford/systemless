//! Archive-level PowerPC import coverage audit.
//!
//! Loads an archive into a runner's launch VFS with the normal ingestion path,
//! then classifies the imports of every PowerPC fragment it contains without
//! executing guest code. Bundled CFM libraries are discovered exactly as the
//! launcher does, so imports they satisfy are not reported as gaps.

use super::launch::{
    discover_ppc_cfm_library_fragments_with_locations, load_game_from_path, new_runner,
    ppc_cfrg_is_runtime_library_installer, ppc_diagnostic_vfs, vfs_path_sort_key,
};
use crate::loader::cfrg::{parse_cfrg_resource, ARCH_POWERPC, USAGE_APP, USAGE_DROP_IN, USAGE_LIB};
use crate::loader::pef::{data_fork_is_pef, parse_pef_header};
use crate::loader::ppc::import_audit::{
    audit_pef_imports, PpcBundledLibraryExports, PpcImportAuditEntry,
};
use crate::managers::resource::ResourceFork;
use serde::Serialize;

/// Report format identifier for consumers of the JSON output.
pub const PPC_IMPORT_AUDIT_FORMAT: &str = "systemless_ppc_import_audit_v1";

/// Import coverage of every PowerPC fragment in one archive.
#[derive(Debug, Clone, Serialize)]
pub struct PpcImportAuditReport {
    pub format: &'static str,
    /// Executable load failure, if any. The VFS is still audited because
    /// ingestion precedes executable selection.
    pub load_error: Option<String>,
    pub fragments: Vec<PpcFragmentImportAudit>,
}

/// One PowerPC fragment found in the launch VFS.
#[derive(Debug, Clone, Serialize)]
pub struct PpcFragmentImportAudit {
    pub path: String,
    /// Logical CFM name from `'cfrg'`, or `None` for an unregistered data fork.
    pub name: Option<String>,
    /// `application`, `library`, `drop_in`, or `unregistered`.
    pub usage: &'static str,
    /// False when the PEF loader section could not be parsed.
    pub parsed: bool,
    pub imports: Vec<PpcImportAuditEntry>,
}

impl PpcImportAuditReport {
    /// Imports that can fail at runtime, deduplicated across fragments.
    pub fn gaps(&self) -> Vec<&PpcImportAuditEntry> {
        let mut gaps: Vec<_> = self
            .fragments
            .iter()
            .flat_map(|fragment| fragment.imports.iter())
            .filter(|entry| entry.status.is_gap())
            .collect();
        gaps.sort_by(|left, right| {
            (left.status, &left.library, &left.symbol).cmp(&(
                right.status,
                &right.library,
                &right.symbol,
            ))
        });
        gaps.dedup_by(|left, right| {
            left.status == right.status
                && left.library == right.library
                && left.symbol == right.symbol
        });
        gaps
    }
}

/// Audit the PowerPC imports of the application or archive at `path`.
pub fn audit_ppc_imports_from_path(path: &std::path::Path) -> PpcImportAuditReport {
    let mut runner = new_runner();
    let load_error = load_game_from_path(&mut runner, path).err();
    let application = runner.dispatcher().launched_app_path().map(str::to_string);
    let vfs = ppc_diagnostic_vfs(&mut runner, None);

    let mut bundled = PpcBundledLibraryExports::default();
    for library in discover_ppc_cfm_library_fragments_with_locations(&vfs, application.as_deref()).0
    {
        if !is_native_replacement_library(&library.name) {
            bundled.insert(&library.name, &library.bytes);
        }
    }

    let mut fragments = Vec::new();
    let mut registered_paths = Vec::new();
    let mut resource_files: Vec<_> = vfs.resource_files.iter().collect();
    resource_files.sort_by_key(|file| vfs_path_sort_key(&file.path));
    for resource_file in resource_files {
        let Some(resource_data) = resource_file.raw_data.as_deref() else {
            continue;
        };
        let Some(cfrg) = ResourceFork::parse(resource_data)
            .and_then(|fork| parse_cfrg_resource(&fork.get(*b"cfrg", 0)?.data))
        else {
            continue;
        };
        // Launch skips system-component installers; so does the audit.
        if ppc_cfrg_is_runtime_library_installer(&cfrg, &resource_file.path, application.as_deref())
        {
            registered_paths.push(resource_file.path.to_ascii_lowercase());
            continue;
        }
        let Some(data_file) = vfs
            .files
            .iter()
            .find(|file| file.path.eq_ignore_ascii_case(&resource_file.path))
        else {
            continue;
        };
        for fragment in cfrg.fragments {
            if fragment.architecture != ARCH_POWERPC {
                continue;
            }
            let Some(bytes) = fragment
                .data_fork_range(data_file.data.len())
                .and_then(|range| data_file.data.get(range))
            else {
                continue;
            };
            if !is_powerpc_pef(bytes) {
                continue;
            }
            registered_paths.push(resource_file.path.to_ascii_lowercase());
            if fragment.usage == USAGE_LIB && is_native_replacement_library(&fragment.name) {
                continue;
            }
            let usage = match fragment.usage {
                USAGE_APP => "application",
                USAGE_LIB => "library",
                USAGE_DROP_IN => "drop_in",
                _ => "unregistered",
            };
            fragments.push(fragment_audit(
                &resource_file.path,
                Some(fragment.name),
                usage,
                bytes,
                &bundled,
            ));
        }
    }

    // Plug-ins loaded through GetDiskFragment need no 'cfrg' resource.
    let mut files: Vec<_> = vfs.files.iter().collect();
    files.sort_by_key(|file| vfs_path_sort_key(&file.path));
    for file in files {
        if registered_paths.contains(&file.path.to_ascii_lowercase()) || !is_powerpc_pef(&file.data)
        {
            continue;
        }
        fragments.push(fragment_audit(
            &file.path,
            None,
            "unregistered",
            &file.data,
            &bundled,
        ));
    }

    PpcImportAuditReport {
        format: PPC_IMPORT_AUDIT_FORMAT,
        load_error,
        fragments,
    }
}

/// GameSprockets copies are replaced by the native dispatcher at launch, so
/// neither their exports nor their own imports participate in binding.
fn is_native_replacement_library(name: &str) -> bool {
    name.eq_ignore_ascii_case("DrawSprocketLib") || name.eq_ignore_ascii_case("InputSprocketLib")
}

fn is_powerpc_pef(bytes: &[u8]) -> bool {
    data_fork_is_pef(bytes)
        && parse_pef_header(bytes).is_some_and(|header| header.architecture == ARCH_POWERPC)
}

fn fragment_audit(
    path: &str,
    name: Option<String>,
    usage: &'static str,
    bytes: &[u8],
    bundled: &PpcBundledLibraryExports,
) -> PpcFragmentImportAudit {
    let imports = audit_pef_imports(bytes, bundled);
    PpcFragmentImportAudit {
        path: path.to_string(),
        name,
        usage,
        parsed: imports.is_some(),
        imports: imports.unwrap_or_default(),
    }
}
