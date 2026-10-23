//! Static PowerPC import coverage audit.
//!
//! The loader binds every strong import, including symbols with no native
//! implementation; those halt the engine only when the guest first calls them.
//! This audit classifies a PEF container's imports with the same resolution
//! order as initial CFM binding so missing Toolbox coverage can be measured
//! without running the application.

use super::import_policies::import_data_address_for;
use super::import_targets::{dispatcher_target_for_import, PpcImportDispatcherTarget};
use crate::loader::pef::{parse_pef_exported_symbols, resolve_pef_imports};
use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet};

/// How the runtime satisfies one imported symbol.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PpcImportAuditStatus {
    /// A bundled CFM library exports the symbol.
    Bundled,
    /// The runtime supplies fixed storage for a data import.
    FixedData,
    /// The native dispatcher implements the symbol.
    Native,
    /// A weak import with no implementation; CFM binds it to
    /// `kUnresolvedSymbolAddress`, which the application can test.
    WeakUnresolved,
    /// A strong import with no implementation; the first call halts the engine.
    Unsupported,
}

impl PpcImportAuditStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Bundled => "bundled",
            Self::FixedData => "fixed_data",
            Self::Native => "native",
            Self::WeakUnresolved => "weak_unresolved",
            Self::Unsupported => "unsupported",
        }
    }

    /// True when calling or reading the import can fail at runtime.
    pub fn is_gap(self) -> bool {
        matches!(self, Self::WeakUnresolved | Self::Unsupported)
    }
}

/// One imported symbol and its classification.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PpcImportAuditEntry {
    pub library: String,
    pub symbol: String,
    pub class: u8,
    pub weak: bool,
    pub status: PpcImportAuditStatus,
}

/// Exported symbol names of the CFM libraries available to a launch.
#[derive(Debug, Clone, Default)]
pub struct PpcBundledLibraryExports {
    libraries: BTreeMap<String, BTreeSet<String>>,
}

impl PpcBundledLibraryExports {
    /// Record a bundled library. The first fragment with a given name wins,
    /// matching launch discovery, which keeps the first candidate per name.
    /// Returns false when the container's export table cannot be parsed.
    pub fn insert(&mut self, library_name: &str, fragment: &[u8]) -> bool {
        let key = library_name.to_ascii_lowercase();
        if self.libraries.contains_key(&key) {
            return true;
        }
        let Some(exports) = parse_pef_exported_symbols(fragment) else {
            return false;
        };
        self.libraries
            .insert(key, exports.into_iter().map(|export| export.name).collect());
        true
    }

    pub fn contains_library(&self, library_name: &str) -> bool {
        self.libraries
            .contains_key(&library_name.to_ascii_lowercase())
    }

    fn exports(&self, library_name: &str, symbol_name: &str) -> bool {
        self.libraries
            .get(&library_name.to_ascii_lowercase())
            .is_some_and(|exports| exports.contains(symbol_name))
    }
}

/// Classify one import using the initial CFM binding order: a bundled
/// connection's export, fixed data storage, then the native dispatcher.
pub fn classify_ppc_import(
    library: &str,
    symbol: &str,
    weak: bool,
    bundled: &PpcBundledLibraryExports,
) -> PpcImportAuditStatus {
    if bundled.exports(library, symbol) {
        PpcImportAuditStatus::Bundled
    } else if import_data_address_for(library, symbol).is_some() {
        PpcImportAuditStatus::FixedData
    } else if dispatcher_target_for_import(library, symbol)
        != PpcImportDispatcherTarget::Unsupported
    {
        PpcImportAuditStatus::Native
    } else if weak {
        PpcImportAuditStatus::WeakUnresolved
    } else {
        PpcImportAuditStatus::Unsupported
    }
}

/// Classify every import of a PEF container. Returns `None` when the loader
/// section cannot be parsed.
pub fn audit_pef_imports(
    fragment: &[u8],
    bundled: &PpcBundledLibraryExports,
) -> Option<Vec<PpcImportAuditEntry>> {
    let imports = resolve_pef_imports(fragment)?;
    Some(
        imports
            .into_iter()
            .map(|import| {
                let status = classify_ppc_import(
                    &import.library_name,
                    &import.symbol_name,
                    import.weak,
                    bundled,
                );
                PpcImportAuditEntry {
                    library: import.library_name,
                    symbol: import.symbol_name,
                    class: import.class,
                    weak: import.weak,
                    status,
                }
            })
            .collect(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bundled(library: &str, symbols: &[&str]) -> PpcBundledLibraryExports {
        let mut exports = PpcBundledLibraryExports::default();
        exports.libraries.insert(
            library.to_ascii_lowercase(),
            symbols.iter().map(|symbol| symbol.to_string()).collect(),
        );
        exports
    }

    #[test]
    fn native_dispatcher_symbols_are_satisfied() {
        let none = PpcBundledLibraryExports::default();
        assert_eq!(
            classify_ppc_import("InterfaceLib", "NewPtr", false, &none),
            PpcImportAuditStatus::Native
        );
        // CarbonLib clients resolve through the InterfaceLib alias.
        assert_eq!(
            classify_ppc_import("CarbonLib", "NewPtr", false, &none),
            PpcImportAuditStatus::Native
        );
    }

    #[test]
    fn fixed_data_precedes_dispatcher_lookup() {
        let none = PpcBundledLibraryExports::default();
        assert_eq!(
            classify_ppc_import("StdCLib", "errno", false, &none),
            PpcImportAuditStatus::FixedData
        );
    }

    #[test]
    fn missing_symbols_distinguish_weak_from_strong() {
        let none = PpcBundledLibraryExports::default();
        assert_eq!(
            classify_ppc_import("InterfaceLib", "SystemlessNoSuchCall", false, &none),
            PpcImportAuditStatus::Unsupported
        );
        assert_eq!(
            classify_ppc_import("InterfaceLib", "SystemlessNoSuchCall", true, &none),
            PpcImportAuditStatus::WeakUnresolved
        );
        assert!(PpcImportAuditStatus::Unsupported.is_gap());
        assert!(!PpcImportAuditStatus::Native.is_gap());
    }

    #[test]
    fn bundled_exports_match_library_names_case_insensitively() {
        let exports = bundled("GameEngineLib", &["EngineStart"]);
        assert!(exports.contains_library("gameenginelib"));
        assert_eq!(
            classify_ppc_import("gameENGINElib", "EngineStart", false, &exports),
            PpcImportAuditStatus::Bundled
        );
        // A bundled library that lacks the symbol falls back to the dispatcher.
        assert_eq!(
            classify_ppc_import("GameEngineLib", "EngineStop", false, &exports),
            PpcImportAuditStatus::Unsupported
        );
    }
}
