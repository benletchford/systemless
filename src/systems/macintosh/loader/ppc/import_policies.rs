//! PowerPC PEF CFM import binding policies, layouts, and error mappers.

use super::*;

#[derive(Debug, Clone, Copy)]
pub(crate) struct SystemlessPpcImportBindingPolicy;

impl PpcImportBindingPolicy for SystemlessPpcImportBindingPolicy {
    fn dispatcher_target(&self, library: &str, symbol: &str) -> PpcImportDispatcherTarget {
        dispatcher_target_for_import(library, symbol)
    }

    fn fixed_data_address(&self, library: &str, symbol: &str) -> Option<u32> {
        import_data_address_for(library, symbol)
    }

    fn is_explicit_hle_library(&self, library: &str) -> bool {
        ppc_is_explicit_hle_cfm_library(library)
    }
}

pub(crate) struct PpcConnectedCfmBindingPolicy<'a> {
    pub(crate) connections: &'a [PpcCfmConnection],
}

impl PpcImportBindingPolicy for PpcConnectedCfmBindingPolicy<'_> {
    fn dispatcher_target(&self, library: &str, symbol: &str) -> PpcImportDispatcherTarget {
        dispatcher_target_for_import(library, symbol)
    }

    fn fixed_data_address(&self, library: &str, symbol: &str) -> Option<u32> {
        import_data_address_for(library, symbol)
    }

    fn resolved_import_address(&self, library: &str, symbol: &str, _class: u8) -> Option<u32> {
        self.connections
            .iter()
            .find(|connection| connection.library_name.eq_ignore_ascii_case(library))
            // PEF symbol classes annotate imports and exports; CFM binding
            // resolves the symbol by name within the selected library.
            .and_then(|connection| {
                connection
                    .exports
                    .iter()
                    .find(|export| export.name == symbol)
            })
            .map(|export| export.address)
            .or_else(|| self.fixed_data_address(library, symbol))
    }

    fn is_explicit_hle_library(&self, library: &str) -> bool {
        ppc_is_explicit_hle_cfm_library(library)
    }
}

pub(crate) fn ppc_import_layout() -> PpcImportLayout {
    PpcImportLayout {
        capacity: PPC_IMPORT_CAPACITY,
        tvector_base: PPC_IMPORT_TVECTOR_BASE,
        trap_base: PPC_IMPORT_TRAP_BASE,
    }
}

pub(super) fn ppc_initial_import_error(error: PpcImportBindingError) -> PpcLoadError {
    match error {
        PpcImportBindingError::SymbolIndexOutOfRange {
            symbol_index,
            import_count,
        } => PpcLoadError::ImportBindingOutOfRange {
            symbol_index,
            import_count,
        },
        PpcImportBindingError::CapacityExceeded {
            import_count,
            capacity,
        } => PpcLoadError::ImportCapacityExceeded {
            import_count,
            capacity,
        },
        PpcImportBindingError::CountOverflow
        | PpcImportBindingError::BindingAddressOverflow
        | PpcImportBindingError::AddressTableOutOfRange => PpcLoadError::AddressOverflow,
        PpcImportBindingError::RegistryChanged => unreachable!("fresh import plan has no registry"),
    }
}

pub(in crate::systems::macintosh::loader::ppc) fn ppc_dynamic_import_error(
    error: PpcImportBindingError,
) -> i16 {
    match error {
        PpcImportBindingError::CountOverflow
        | PpcImportBindingError::CapacityExceeded { .. }
        | PpcImportBindingError::AddressTableOutOfRange => PPC_FRAG_NO_MEM,
        PpcImportBindingError::SymbolIndexOutOfRange { .. }
        | PpcImportBindingError::BindingAddressOverflow
        | PpcImportBindingError::RegistryChanged => PPC_FRAG_CORRUPT_ERR,
    }
}

pub(crate) fn ppc_initial_process_file_system() -> SharedProcessFileSystem {
    let mut state = ProcessFileSystemState::default();
    state.stdio_streams = ppc_initial_stdio_streams();
    state.next_file_ref_num = PPC_FIRST_FILE_REF_NUM;
    state.vfs_directories.replace(initial_ppc_vfs_directories());
    state
        .next_vfs_dir_id
        .with_mut(|next_dir_id| *next_dir_id = PPC_FIRST_DYNAMIC_DIR_ID);
    state
        .default_dir_id
        .with_mut(|default_dir_id| *default_dir_id = PPC_ROOT_DIR_ID);
    SharedProcessFileSystem::from_state(state)
}
