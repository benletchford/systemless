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

pub(crate) fn import_data_address_for(library_name: &str, symbol_name: &str) -> Option<u32> {
    match (library_name, symbol_name) {
        ("StdCLib", "_IntEnv") => Some(PPC_IMPORT_DATA_BASE),
        ("StdCLib", "__C_phase") => Some(PPC_IMPORT_DATA_BASE + 0x400),
        ("StdCLib", "__target_for_exit") => Some(PPC_IMPORT_DATA_BASE + 0x404),
        ("StdCLib", "_exit_status") => Some(PPC_IMPORT_DATA_BASE + 0x408),
        ("StdCLib", "_iob") => Some(PPC_STDIO_IOB_ADDR),
        ("StdCLib", "__p_CType") => Some(PPC_IMPORT_CTYPE_POINTER),
        ("StdCLib", "_DBL_EPSILON") => Some(PPC_IMPORT_STD_DBL_EPSILON),
        ("StdCLib", "_DBL_MAX") => Some(PPC_IMPORT_STD_DBL_MAX),
        ("StdCLib", "_DBL_MIN") => Some(PPC_IMPORT_STD_DBL_MIN),
        ("StdCLib", "_FLT_EPSILON") => Some(PPC_IMPORT_STD_FLT_EPSILON),
        ("StdCLib", "_FLT_MAX") => Some(PPC_IMPORT_STD_FLT_MAX),
        ("StdCLib", "_FLT_MIN") => Some(PPC_IMPORT_STD_FLT_MIN),
        ("StdCLib", "errno") => Some(PPC_IMPORT_STD_ERRNO),
        ("StdCLib", "MacOSErr") => Some(PPC_IMPORT_STD_MAC_OS_ERR),
        // PowerPC Numerics exposes `pi` as an addressable MathLib export.
        // Some CFM clients label the import as a transition-vector symbol but
        // dereference it directly as a double, so bind the known export to
        // stable data storage regardless of the PEF class tag.
        ("MathLib", "pi") => Some(PPC_IMPORT_MATH_PI),
        ("MathLib", "_FE_DFL_ENV") => Some(PPC_IMPORT_MATH_FE_DFL_ENV),
        _ => None,
    }
}

pub(crate) fn ppc_seed_import_data(memory: &mut PpcSectionMem) {
    // The Metrowerks runtime imports these as true CFM data symbols, not
    // callable entry points. Give them stable writable storage and point the
    // ctype indirection at a complete 256-entry classification table.
    let _ = memory.write_u32_be(PPC_IMPORT_CTYPE_POINTER, PPC_IMPORT_CTYPE_TABLE);
    let _ = memory.write_u64_be(PPC_IMPORT_MATH_PI, std::f64::consts::PI.to_bits());
    // Inside Macintosh: PowerPC Numerics (1994), Chapter 8 and Appendix C:
    // PowerPC fenv_t is a 32-bit word, with zero selecting round-to-nearest
    // and leaving every floating-point exception flag clear.
    let _ = memory.write_u32_be(PPC_IMPORT_MATH_FE_DFL_ENV, 0);
    // Universal Interfaces 3.4 float.h exposes these values by dereferencing
    // imported StdCLib objects. Seed their exact IEEE single- and
    // double-precision representations in big-endian guest memory.
    let _ = memory.write_u64_be(PPC_IMPORT_STD_DBL_EPSILON, f64::EPSILON.to_bits());
    let _ = memory.write_u64_be(PPC_IMPORT_STD_DBL_MAX, f64::MAX.to_bits());
    let _ = memory.write_u64_be(PPC_IMPORT_STD_DBL_MIN, f64::MIN_POSITIVE.to_bits());
    let _ = memory.write_u32_be(PPC_IMPORT_STD_FLT_EPSILON, f32::EPSILON.to_bits());
    let _ = memory.write_u32_be(PPC_IMPORT_STD_FLT_MAX, f32::MAX.to_bits());
    let _ = memory.write_u32_be(PPC_IMPORT_STD_FLT_MIN, f32::MIN_POSITIVE.to_bits());
    // Universal Interfaces 3.4 errno.h declares StdCLib's writable `errno`
    // as an int and `MacOSErr` as a short. Both begin clear at process launch.
    let _ = memory.write_u32_be(PPC_IMPORT_STD_ERRNO, 0);
    let _ = memory.write_u16_be(PPC_IMPORT_STD_MAC_OS_ERR, 0);
    let _ = memory.write_bytes(
        PPC_STDIO_IOB_ADDR,
        &vec![0; (3 * PPC_STDIO_FILE_SIZE) as usize],
    );
    for byte in 0u16..=255 {
        let _ = memory.write_u8(
            PPC_IMPORT_CTYPE_TABLE + u32::from(byte),
            ppc_ctype_entry(byte as u8),
        );
    }
}
