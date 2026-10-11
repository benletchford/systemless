//! PowerPC PEF application loader and relocation execution.

use super::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PpcLoadConfig {
    pub stack_size: u32,
    pub screen_depth: u32,
}

pub(crate) struct PpcDiskFragment<'a> {
    pub vref: i16,
    pub dir_id: u32,
    pub filename: &'a [u8],
    pub offset: u32,
    pub length: u32,
}

pub(crate) struct PpcDiskLibraryFragment {
    pub name: String,
    pub vref: i16,
    pub dir_id: u32,
    pub filename: Vec<u8>,
    pub offset: u32,
    pub length: u32,
}

impl PpcDiskLibraryFragment {
    fn locator(&self) -> PpcDiskFragment<'_> {
        PpcDiskFragment {
            vref: self.vref,
            dir_id: self.dir_id,
            filename: &self.filename,
            offset: self.offset,
            length: self.length,
        }
    }
}

fn ppc_create_disk_fragment_init_block(
    memory: &mut PpcSectionMem,
    heap_cursor: &mut u32,
    heap_limit: u32,
    connection_id: u32,
    disk: PpcDiskFragment<'_>,
    fragment_name: &str,
) -> Result<u32, PpcLoadError> {
    let block = crate::cfm::CfmInitBlock::on_disk_flat(
        CfmLoadId(connection_id),
        disk.vref,
        disk.dir_id,
        disk.filename,
        disk.offset,
        disk.length,
        fragment_name,
    )
    .ok_or(PpcLoadError::AddressOverflow)?;
    let address = ppc_heap_alloc(memory, heap_cursor, heap_limit, block.size(), true);
    if address == 0 || block.publish(memory, address).is_err() {
        return Err(PpcLoadError::AddressOverflow);
    }
    Ok(address)
}

impl PpcLoadConfig {
    pub fn from_cfrg_app_stack_size(app_stack_size: u32) -> Self {
        if app_stack_size == 0 {
            Self::default()
        } else {
            Self {
                stack_size: app_stack_size,
                ..Self::default()
            }
        }
    }
}

impl Default for PpcLoadConfig {
    fn default() -> Self {
        Self {
            stack_size: PPC_DEFAULT_STACK_SIZE,
            screen_depth: PPC_MAIN_PIXEL_DEPTH,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PpcRelocationImportSymbol {
    pub symbol_index: u32,
    pub library_name: String,
    pub symbol_name: String,
    pub class: u8,
    pub weak: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PpcLoadError {
    PefParse,
    SectionInstantiation,
    NoCodeSection,
    NoDataSection,
    MainSectionMissing {
        section_index: i32,
    },
    MainTVectorOutOfRange {
        section_index: i32,
        offset: u32,
    },
    RelocationStream {
        section_index: u16,
    },
    RelocationApply {
        section_index: u16,
        reloc_instr_offset: u32,
        section_position: u32,
        import_index: Option<u32>,
        import_symbol: Option<PpcRelocationImportSymbol>,
        error: PefRelocApplyError,
    },
    ImportBindingOutOfRange {
        symbol_index: u32,
        import_count: u32,
    },
    ImportCapacityExceeded {
        import_count: u32,
        capacity: u32,
    },
    StackSizeOutOfRange {
        requested: u32,
    },
    ScreenDepthOutOfRange {
        requested: u32,
    },
    BundledLibraryLoad {
        library_name: String,
        error: i16,
    },
    AddressOverflow,
}

struct PpcInitialCfmLibraryPlan {
    library_name: String,
    fragment_bytes: Vec<u8>,
    plan: CfmFragmentPlan,
}

struct PpcInitialCfmPlan {
    libraries: Vec<PpcInitialCfmLibraryPlan>,
    connections: Vec<PpcCfmConnection>,
    imports: Vec<PpcImportBinding>,
    import_addresses: Vec<u32>,
    import_count: u32,
    heap_cursor: u32,
    next_connection_id: u32,
}

fn initial_cfm_library_order(
    fragments: &[PpcCfmLibraryFragment],
    application_imports: &[crate::loader::pef::PefResolvedImport],
) -> Vec<usize> {
    fn visit(
        index: usize,
        fragments: &[PpcCfmLibraryFragment],
        visiting: &mut HashSet<usize>,
        visited: &mut HashSet<usize>,
        order: &mut Vec<usize>,
    ) {
        if visited.contains(&index) || !visiting.insert(index) {
            return;
        }
        let mut dependencies: Vec<_> = resolve_pef_imports(&fragments[index].bytes)
            .unwrap_or_default()
            .into_iter()
            .filter_map(|import| {
                fragments
                    .iter()
                    .position(|fragment| fragment.name.eq_ignore_ascii_case(&import.library_name))
            })
            .collect();
        dependencies.sort_unstable();
        dependencies.dedup();
        for dependency in dependencies {
            visit(dependency, fragments, visiting, visited, order);
        }
        visiting.remove(&index);
        if visited.insert(index) {
            order.push(index);
        }
    }

    let mut roots: Vec<_> = application_imports
        .iter()
        .filter_map(|import| {
            fragments
                .iter()
                .position(|fragment| fragment.name.eq_ignore_ascii_case(&import.library_name))
        })
        .collect();
    roots.sort_unstable();
    roots.dedup();
    let mut visiting = HashSet::new();
    let mut visited = HashSet::new();
    let mut order = Vec::new();
    for root in roots {
        visit(root, fragments, &mut visiting, &mut visited, &mut order);
    }
    order
}

fn ppc_plan_initial_cfm_libraries(
    mut fragments: Vec<PpcCfmLibraryFragment>,
    application_imports: Vec<crate::loader::pef::PefResolvedImport>,
    application_import_count: usize,
    initial_imports: Vec<PpcImportBinding>,
    heap_limit: u32,
) -> Result<PpcInitialCfmPlan, PpcLoadError> {
    // GameSprockets are system libraries implemented by the native dispatcher.
    // Installer copies can require on-disk CFM initialization metadata that is
    // not present when the archive is expanded into the in-memory launch VFS.
    fragments.retain(|fragment| {
        !fragment.name.eq_ignore_ascii_case("DrawSprocketLib")
            && !fragment.name.eq_ignore_ascii_case("InputSprocketLib")
    });
    fragments.sort_by(|left, right| {
        left.name
            .to_ascii_lowercase()
            .cmp(&right.name.to_ascii_lowercase())
            .then_with(|| left.name.cmp(&right.name))
    });
    fragments.dedup_by(|left, right| left.name.eq_ignore_ascii_case(&right.name));
    let order = initial_cfm_library_order(&fragments, &application_imports);
    let initial_binding_count = initial_imports.len();
    let mut import_run_state = PpcImportRunState::from_parts(
        initial_imports,
        application_import_count as u32,
        ppc_import_layout(),
    );
    let mut libraries = Vec::new();
    let mut connections = Vec::new();
    let mut heap_cursor = PPC_HEAP_BASE;
    let mut next_connection_id = PPC_FIRST_CFM_CONNECTION_ID;

    for index in order {
        let fragment = &fragments[index];
        let imported_symbols = parse_pef_imported_symbols(&fragment.bytes).ok_or_else(|| {
            PpcLoadError::BundledLibraryLoad {
                library_name: fragment.name.clone(),
                error: PPC_FRAG_CORRUPT_ERR,
            }
        })?;
        let resolved_imports = resolve_pef_imports(&fragment.bytes).ok_or_else(|| {
            PpcLoadError::BundledLibraryLoad {
                library_name: fragment.name.clone(),
                error: PPC_FRAG_CORRUPT_ERR,
            }
        })?;
        let policy = PpcConnectedCfmBindingPolicy {
            connections: &connections,
        };
        let import_plan = import_run_state
            .plan_resolved(resolved_imports, imported_symbols.len(), &policy)
            .map_err(|error| PpcLoadError::BundledLibraryLoad {
                library_name: fragment.name.clone(),
                error: ppc_dynamic_import_error(error),
            })?;
        let pending = import_run_state
            .stage_append(import_plan)
            .map_err(|error| PpcLoadError::BundledLibraryLoad {
                library_name: fragment.name.clone(),
                error: ppc_dynamic_import_error(error),
            })?;
        let plan = CfmFragmentPlan::prepare(
            &fragment.bytes,
            pending.relocation_addresses(),
            heap_cursor,
            heap_limit,
            PPC_HEAP_ALIGNMENT,
            |cursor, size, alignment| {
                let base = cursor.checked_add(alignment.checked_sub(1)?)? & !(alignment - 1);
                let next = base.checked_add(size)?;
                (next < heap_limit).then_some((base, next))
            },
        )
        .map_err(|error| PpcLoadError::BundledLibraryLoad {
            library_name: fragment.name.clone(),
            error: error.os_error(),
        })?;
        if ppc_hle_trace_enabled() {
            eprintln!(
                "[PPC-TRACE] CFM library {:?} bytes={} heap=${heap_cursor:08X}..${:08X}",
                fragment.name,
                fragment.bytes.len(),
                plan.next_heap_cursor()
            );
        }
        heap_cursor = plan.next_heap_cursor();
        pending.commit();
        let prepared = plan.prepared_fragment();
        connections.push(PpcCfmConnection {
            id: next_connection_id,
            library_name: fragment.name.clone(),
            main_addr: prepared.main_addr,
            init_addr: prepared.init_addr,
            term_addr: prepared.term_addr,
            exports: prepared.exports.clone(),
        });
        next_connection_id = next_connection_id
            .checked_add(1)
            .ok_or(PpcLoadError::AddressOverflow)?;
        libraries.push(PpcInitialCfmLibraryPlan {
            library_name: fragment.name.clone(),
            fragment_bytes: fragment.bytes.clone(),
            plan,
        });
    }

    let policy = PpcConnectedCfmBindingPolicy {
        connections: &connections,
    };
    let rebound = PpcImportBindingPlan::prepare(
        application_imports,
        application_import_count,
        0,
        ppc_import_layout(),
        &policy,
    )
    .map_err(ppc_initial_import_error)?;
    let import_addresses = rebound.relocation_addresses().to_vec();
    let rebound_imports = rebound.into_initial_bindings();
    let (mut imports, import_count) = import_run_state.into_parts();
    imports.splice(0..initial_binding_count, rebound_imports);

    Ok(PpcInitialCfmPlan {
        libraries,
        connections,
        imports,
        import_addresses,
        import_count,
        heap_cursor,
        next_connection_id,
    })
}

pub fn load_pef_application(data: &[u8]) -> Result<PpcLoadedApp, PpcLoadError> {
    load_pef_application_with_config(data, PpcLoadConfig::default())
}

pub fn load_pef_application_with_config(
    data: &[u8],
    config: PpcLoadConfig,
) -> Result<PpcLoadedApp, PpcLoadError> {
    load_pef_application_with_config_and_optional_system_reservation(
        data,
        config,
        None,
        Vec::new(),
        None,
    )
}

#[cfg(test)]
pub(crate) fn load_pef_application_with_config_and_system_reservation(
    data: &[u8],
    config: PpcLoadConfig,
    system_reservation: (u32, u32),
) -> Result<PpcLoadedApp, PpcLoadError> {
    load_pef_application_with_config_and_optional_system_reservation(
        data,
        config,
        Some(system_reservation),
        Vec::new(),
        None,
    )
}

pub(crate) fn load_pef_application_with_disk_fragment_and_libraries(
    data: &[u8],
    config: PpcLoadConfig,
    system_reservation: (u32, u32),
    library_fragments: Vec<PpcCfmLibraryFragment>,
    fragment_name: Option<&str>,
    disk_fragment: PpcDiskFragment<'_>,
    disk_libraries: &[PpcDiskLibraryFragment],
) -> Result<PpcLoadedApp, PpcLoadError> {
    load_pef_application_with_optional_disk_fragment(
        data,
        config,
        Some(system_reservation),
        library_fragments,
        fragment_name,
        Some(disk_fragment),
        disk_libraries,
    )
}

pub(crate) fn load_pef_application_with_config_and_optional_system_reservation(
    data: &[u8],
    config: PpcLoadConfig,
    system_reservation: Option<(u32, u32)>,
    library_fragments: Vec<PpcCfmLibraryFragment>,
    application_fragment_name: Option<&str>,
) -> Result<PpcLoadedApp, PpcLoadError> {
    load_pef_application_with_optional_disk_fragment(
        data,
        config,
        system_reservation,
        library_fragments,
        application_fragment_name,
        None,
        &[],
    )
}

fn load_pef_application_with_optional_disk_fragment(
    data: &[u8],
    config: PpcLoadConfig,
    system_reservation: Option<(u32, u32)>,
    library_fragments: Vec<PpcCfmLibraryFragment>,
    application_fragment_name: Option<&str>,
    application_disk_fragment: Option<PpcDiskFragment<'_>>,
    disk_libraries: &[PpcDiskLibraryFragment],
) -> Result<PpcLoadedApp, PpcLoadError> {
    if !matches!(config.screen_depth, 1 | 2 | 4 | 8 | 16) {
        return Err(PpcLoadError::ScreenDepthOutOfRange {
            requested: config.screen_depth,
        });
    }
    let header = parse_pef_header(data).ok_or(PpcLoadError::PefParse)?;
    let raw_sections = parse_pef_sections(data).ok_or(PpcLoadError::PefParse)?;
    let loader = parse_pef_loader_header(data).ok_or(PpcLoadError::PefParse)?;
    let resolved_imports = resolve_pef_imports(data).unwrap_or_default();
    let imported_symbols = parse_pef_imported_symbols(data).unwrap_or_default();
    let stack_size = normalize_stack_size(config.stack_size)?;
    // When physical RAM can hold the native heap and stack, place their
    // contiguous partition below the system reservation. Standalone loaders
    // retain their default layout.
    // Inside Macintosh: Processes (1994), pp. 1-7--1-8.
    let stack_top = system_reservation
        .map(|(base, _)| {
            let candidate = base.min(PPC_DSP_CONTEXT);
            if candidate
                .checked_sub(stack_size)
                .is_some_and(|bottom| bottom > PPC_HEAP_BASE)
            {
                candidate
            } else {
                PPC_STACK_TOP
            }
        })
        .unwrap_or(PPC_STACK_TOP);
    let stack_base =
        stack_top
            .checked_sub(stack_size)
            .ok_or(PpcLoadError::StackSizeOutOfRange {
                requested: config.stack_size,
            })?;
    if stack_base <= PPC_HEAP_BASE {
        return Err(PpcLoadError::StackSizeOutOfRange {
            requested: config.stack_size,
        });
    }
    if system_reservation.is_some_and(|(base, len)| {
        let start = u64::from(base);
        let end = start + u64::from(len);
        start < u64::from(stack_top) && u64::from(PPC_HEAP_BASE) < end
    }) {
        return Err(PpcLoadError::AddressOverflow);
    }
    let import_plan = PpcImportBindingPlan::prepare(
        resolved_imports.clone(),
        imported_symbols.len(),
        0,
        ppc_import_layout(),
        &SystemlessPpcImportBindingPolicy,
    )
    .map_err(ppc_initial_import_error)?;
    let initial_imports = import_plan.into_initial_bindings();
    let mut mapped_sections = map_instantiated_sections(data)?;
    let section_bases = section_bases(&mapped_sections);
    let code_base = first_base_for_kind(&mapped_sections, SECTION_KIND_CODE)
        .ok_or(PpcLoadError::NoCodeSection)?;
    let data_base = first_data_base(&mapped_sections).ok_or(PpcLoadError::NoDataSection)?;
    let initial_cfm = ppc_plan_initial_cfm_libraries(
        library_fragments.clone(),
        resolved_imports,
        imported_symbols.len(),
        initial_imports,
        stack_base,
    )?;
    let import_addrs = &initial_cfm.import_addresses;
    let imports = &initial_cfm.imports;

    let reloc_headers = parse_pef_reloc_headers(data).unwrap_or_default();
    for reloc in &reloc_headers {
        let stream = pef_reloc_chunk_stream(data, reloc).ok_or(PpcLoadError::RelocationStream {
            section_index: reloc.section_index,
        })?;
        let mapped = mapped_sections
            .iter_mut()
            .find(|section| section.index == usize::from(reloc.section_index))
            .ok_or(PpcLoadError::RelocationStream {
                section_index: reloc.section_index,
            })?;
        let ctx = PefRelocContext {
            code_base,
            data_base,
            section_bases: &section_bases,
            import_addrs,
        };
        apply_pef_relocations_detailed(&mut mapped.bytes, stream, &ctx).map_err(|failure| {
            let import_symbol = failure
                .import_index
                .and_then(|index| relocation_import_symbol(imports, index));
            PpcLoadError::RelocationApply {
                section_index: reloc.section_index,
                reloc_instr_offset: reloc
                    .first_reloc_offset
                    .checked_add(failure.reloc_offset)
                    .unwrap_or(u32::MAX),
                section_position: failure.section_position,
                import_index: failure.import_index,
                import_symbol,
                error: failure.error,
            }
        })?;
    }
    let application_exports = if application_fragment_name.is_some() {
        resolve_fragment_exports(data, &mapped_sections, import_addrs)
            .map_err(|_| PpcLoadError::PefParse)?
    } else {
        Vec::new()
    };
    let main_section = mapped_sections
        .iter()
        .find(|section| section.index == usize::try_from(loader.main_section).unwrap_or(usize::MAX))
        .ok_or(PpcLoadError::MainSectionMissing {
            section_index: loader.main_section,
        })?;
    let main_offset =
        usize::try_from(loader.main_offset).map_err(|_| PpcLoadError::MainTVectorOutOfRange {
            section_index: loader.main_section,
            offset: loader.main_offset,
        })?;
    let tvector_end = main_offset
        .checked_add(8)
        .ok_or(PpcLoadError::MainTVectorOutOfRange {
            section_index: loader.main_section,
            offset: loader.main_offset,
        })?;
    if tvector_end > main_section.bytes.len() {
        return Err(PpcLoadError::MainTVectorOutOfRange {
            section_index: loader.main_section,
            offset: loader.main_offset,
        });
    }
    let entry_pc =
        read_u32(&main_section.bytes, main_offset).ok_or(PpcLoadError::MainTVectorOutOfRange {
            section_index: loader.main_section,
            offset: loader.main_offset,
        })?;
    let rtoc = read_u32(&main_section.bytes, main_offset + 4).ok_or(
        PpcLoadError::MainTVectorOutOfRange {
            section_index: loader.main_section,
            offset: loader.main_offset,
        },
    )?;
    let special_tvector = |section_index: i32, offset: u32| -> Option<(u32, u32, u32)> {
        if section_index < 0 {
            return None;
        }
        let section_index = usize::try_from(section_index).ok()?;
        let section = mapped_sections
            .iter()
            .find(|section| section.index == section_index)?;
        let offset = usize::try_from(offset).ok()?;
        let entry = read_u32(&section.bytes, offset)?;
        let rtoc = read_u32(&section.bytes, offset.checked_add(4)?)?;
        let descriptor = section.base.checked_add(offset as u32)?;
        Some((descriptor, entry, rtoc))
    };
    let init_tvector = special_tvector(loader.init_section, loader.init_offset);
    let term_tvector = special_tvector(loader.term_section, loader.term_offset);
    if loader.init_section >= 0 && init_tvector.is_none() {
        return Err(PpcLoadError::PefParse);
    }
    if loader.term_section >= 0 && term_tvector.is_none() {
        return Err(PpcLoadError::PefParse);
    }
    let main_tvector = main_section.base + loader.main_offset;

    maybe_write(&PefDumpContext {
        data_len: data.len(),
        header,
        loader,
        raw_sections: &raw_sections,
        mapped_sections: &mapped_sections,
        imports,
        reloc_headers: &reloc_headers,
        entry_pc,
        rtoc,
        stack_base,
        stack_size,
        stack_top,
    });
    let PpcInitialCfmPlan {
        libraries: initial_library_plans,
        connections: mut cfm_connections,
        imports,
        import_count,
        mut heap_cursor,
        next_connection_id: mut next_cfm_connection_id,
        ..
    } = initial_cfm;
    let has_initial_library_initializer = initial_library_plans
        .iter()
        .any(|library| library.plan.prepared_fragment().init_addr != 0);

    let mut memory = PpcSectionMem::new();
    if let Some((base, len)) = system_reservation {
        memory
            .add_readonly_allocation_exclusion(base, len)
            .ok_or(PpcLoadError::AddressOverflow)?;
    }
    memory.add_region(PPC_HALT_PC, vec![0u8; PPC_LOW_MEMORY_SIZE]);
    let _ = memory.write_u8(
        crate::memory::globals::addr::KBD_TYPE,
        crate::machine_profile::KEYBOARD_PHYSICAL_TYPE,
    );
    let _ = memory.write_u16_be(
        crate::memory::globals::addr::SYS_EVT_MASK,
        crate::memory::globals::DEFAULT_SYS_EVT_MASK,
    );
    let _ = memory.write_u16_be(
        crate::memory::globals::addr::MENU_FLASH,
        crate::memory::globals::DEFAULT_MENU_FLASH_COUNT,
    );
    let _ = memory.write_u16_be(
        crate::memory::globals::addr::KEY_THRESH,
        crate::memory::globals::DEFAULT_AUTO_KEY_THRESHOLD_TICKS,
    );
    let _ = memory.write_u16_be(
        crate::memory::globals::addr::KEY_REP_THRESH,
        crate::memory::globals::DEFAULT_AUTO_KEY_RATE_TICKS,
    );
    let _ = memory.write_u32_be(crate::memory::globals::addr::CARET_TIME, crate::memory::globals::DEFAULT_CARET_TIME_TICKS);
    let _ = memory.write_u16_be(crate::memory::globals::addr::RES_LOAD, 0x0100);
    let _ = memory.write_u32_be(
        crate::memory::globals::addr::DEFLT_STACK,
        crate::memory::globals::DEFAULT_DEFLT_STACK_SIZE,
    );
    let _ = memory.write_u32_be(crate::memory::globals::addr::CUR_STACK_BASE, stack_base);
    let _ = memory.write_u16_be(PPC_MBAR_HEIGHT_ADDR, 20);
    let _ = memory.write_u16_be(PPC_THE_MENU_ADDR, 0);
    // PaintOne normally starts with PaintWhite enabled. Carbon's generated
    // low-memory accessors preserve this flag around window creation.
    let _ = memory.write_u16_be(0x09dc, 1);
    // Inside Macintosh Volume V (1986), pp. V-592--V-593, documents the
    // MMU32Bit low-memory byte at $0CB2. Native PowerPC processes always use
    // 32-bit addressing.
    let _ = memory.write_u8(PPC_MMU_32BIT_ADDR, 1);
    let _ = memory.write_u32_be(
        crate::memory::globals::addr::DOUBLE_TIME,
        PPC_DEFAULT_DOUBLE_TIME_TICKS,
    );
    let _ = memory.write_u8(crate::memory::globals::addr::SD_VOLUME, 1);
    memory.add_region(
        PPC_CLASSIC_APP_MEMORY_BASE,
        vec![0u8; PPC_CLASSIC_APP_MEMORY_SIZE],
    );
    // PowerPC System Software (1994), pp. 1-53--1-60: the application's code
    // stays outside its partition, while its data section (globals) is
    // loaded into the application heap. Initial libraries' code sections and
    // the container copies below are CFM storage outside the partition too.
    let mut launch_partition_storage = PpcLaunchPartitionStorage {
        outside_partition: initial_library_plans
            .iter()
            .map(|library| library.plan.prepared_fragment().code_size)
            .fold(0, u32::saturating_add),
        application_data: mapped_sections
            .iter()
            .filter(|section| section.section_kind != SECTION_KIND_CODE)
            .map(|section| section.bytes.len() as u32)
            .fold(0, u32::saturating_add),
    };
    for section in mapped_sections {
        if section.section_kind == SECTION_KIND_CODE
            || section.section_kind == SECTION_KIND_CONSTANT
        {
            memory.add_readonly_region(section.base, section.bytes);
        } else {
            memory.add_region(section.base, section.bytes);
        }
    }
    // Keep a bounded pool of synthetic import slots mapped from launch so
    // GetMemFragment can bind imports while the CPU is already running.
    memory
        .publish_system_code(
            GuestIsa::PowerPc,
            PPC_IMPORT_TVECTOR_BASE,
            import_tvector_bytes(PPC_IMPORT_SLOT_COUNT as usize),
        )
        .ok_or(PpcLoadError::AddressOverflow)?;
    memory
        .publish_system_code(
            GuestIsa::PowerPc,
            PPC_IMPORT_TRAP_BASE,
            import_trap_bytes(PPC_IMPORT_SLOT_COUNT as usize),
        )
        .ok_or(PpcLoadError::AddressOverflow)?;
    memory.add_region(PPC_IMPORT_DATA_BASE, vec![0; PPC_IMPORT_DATA_SIZE]);
    ppc_seed_import_data(&mut memory);
    memory
        .publish_system_code(
            GuestIsa::PowerPc,
            PPC_CFM_MAIN_STUB_BASE,
            import_trap_bytes(PPC_CFM_MAIN_STUB_COUNT as usize),
        )
        .ok_or(PpcLoadError::AddressOverflow)?;
    memory.add_region(PPC_MAIN_GWORLD, vec![0u8; 256]);
    memory.add_region(PPC_MAIN_GDEVICE, vec![0u8; 256]);
    memory.add_region(PPC_MAIN_GDEVICE_RECORD, vec![0u8; 256]);
    memory.add_region(PPC_MAIN_PIXMAP_HANDLE, vec![0u8; 4]);
    memory.add_region(PPC_MAIN_PIXMAP, vec![0u8; PPC_PIXMAP_SIZE as usize]);
    memory.add_region(PPC_GRAY_RGN_HANDLE, vec![0u8; 4]);
    memory.add_region(PPC_GRAY_RGN, vec![0u8; 10]);
    memory.add_region(PPC_MAIN_DCE_HANDLE, vec![0u8; 4]);
    memory.add_region(PPC_MAIN_DCE, vec![0u8; 52]);
    memory.add_region(PPC_MAIN_CTABLE_HANDLE, vec![0u8; 4]);
    memory.add_region(PPC_MAIN_CTABLE, vec![0u8; PPC_MAIN_CTABLE_SIZE as usize]);
    memory.add_region(PPC_MAIN_VIS_RGN_HANDLE, vec![0u8; 4]);
    memory.add_region(PPC_MAIN_VIS_RGN, vec![0u8; 10]);
    memory.add_region(PPC_MAIN_CLIP_RGN_HANDLE, vec![0u8; 4]);
    memory.add_region(PPC_MAIN_CLIP_RGN, vec![0u8; 10]);
    // Universal Interfaces 3.4 Video.h defines GammaTbl as a six-word
    // header followed by formula bytes and channel data. The main display's
    // device-owned table is the standard one-channel, 8-bit linear ramp.
    let mut gamma_table = vec![0u8; PPC_MAIN_GAMMA_TABLE_SIZE as usize];
    gamma_table[6..8].copy_from_slice(&1u16.to_be_bytes()); // gChanCnt
    gamma_table[8..10].copy_from_slice(&256u16.to_be_bytes()); // gDataCnt
    gamma_table[10..12].copy_from_slice(&8u16.to_be_bytes()); // gDataWidth
    for (value, output) in gamma_table[12..].iter_mut().enumerate() {
        *output = value as u8;
    }
    memory.add_readonly_region(PPC_MAIN_GAMMA_TABLE, gamma_table);
    memory.add_region(PPC_PORT_LIST_HANDLE, vec![0u8; 4]);
    memory.add_region(PPC_PORT_LIST, vec![0u8; 2]);
    memory.add_region(PPC_UNIT_TABLE, vec![0u8; 64 * 4]);
    memory.add_region(PPC_SOUND_DCE_HANDLE, vec![0u8; 4]);
    memory.add_region(PPC_SOUND_DCE, vec![0u8; 52]);
    // PortList is a QuickDraw-owned handle whose first word is the number of
    // registered ports. Keep a valid empty list even though native clients
    // normally reach it through the Window Manager instead of low memory.
    let _ = memory.write_u32_be(PPC_PORT_LIST_ADDR, PPC_PORT_LIST_HANDLE);
    let _ = memory.write_u32_be(PPC_PORT_LIST_HANDLE, PPC_PORT_LIST);
    let _ = memory.write_u16_be(PPC_PORT_LIST, 0);
    let _ = memory.write_u32_be(PPC_SOUND_DCE_HANDLE, PPC_SOUND_DCE);
    let _ = memory.write_u16_be(PPC_SOUND_DCE + 24, (-4i16) as u16);
    let _ = memory.write_u32_be(PPC_UNIT_TABLE + 3 * 4, PPC_SOUND_DCE_HANDLE);
    memory.add_region(PPC_APPLICATION_ZONE, vec![0u8; PPC_ZONE_STORAGE_SIZE]);
    memory.add_region(PPC_SYSTEM_ZONE, vec![0u8; PPC_ZONE_STORAGE_SIZE]);
    ppc_seed_zone_header(
        &mut memory,
        PPC_APPLICATION_ZONE,
        PPC_HEAP_BASE,
        stack_base,
        64,
    );
    ppc_seed_zone_header(&mut memory, PPC_SYSTEM_ZONE, PPC_HEAP_BASE, stack_base, 32);
    let _ = memory.write_u32_be(PPC_THE_ZONE_ADDR, PPC_APPLICATION_ZONE);
    let _ = memory.write_u32_be(PPC_APPL_ZONE_ADDR, PPC_APPLICATION_ZONE);
    let _ = memory.write_u32_be(PPC_SYS_ZONE_ADDR, PPC_SYSTEM_ZONE);
    if !ppc_main_screen_fits() {
        return Err(PpcLoadError::AddressOverflow);
    }
    memory.add_region(
        PPC_MAIN_SCREEN_BASE,
        vec![0u8; ppc_main_screen_buffer_size() as usize],
    );
    memory.add_region(PPC_DSP_BACK_GWORLD, vec![0u8; 256]);
    memory.add_region(PPC_DSP_BACK_PIXMAP_HANDLE, vec![0u8; 4]);
    memory.add_region(PPC_DSP_BACK_PIXMAP, vec![0u8; PPC_PIXMAP_SIZE as usize]);
    memory.add_region(PPC_DSP_BACK_VIS_RGN_HANDLE, vec![0u8; 4]);
    memory.add_region(PPC_DSP_BACK_VIS_RGN, vec![0u8; 10]);
    memory.add_region(PPC_DSP_BACK_CLIP_RGN_HANDLE, vec![0u8; 4]);
    memory.add_region(PPC_DSP_BACK_CLIP_RGN, vec![0u8; 10]);
    memory.add_region(
        PPC_DSP_BACK_SCREEN_BASE,
        vec![0u8; ppc_main_screen_buffer_size() as usize],
    );
    memory.add_region(PPC_DSP_CONTEXT, vec![0u8; PPC_QA_OBJECTS_SIZE]);
    ppc_seed_qa_rave_objects(&mut memory);
    if init_tvector.is_some() && !has_initial_library_initializer {
        memory
            .publish_system_code(
                GuestIsa::PowerPc,
                PPC_APPLICATION_INIT_RETURN_PC,
                ppc_application_init_return_trampoline(entry_pc, rtoc),
            )
            .ok_or(PpcLoadError::AddressOverflow)?;
    }
    let mut gworlds = vec![
        ppc_seed_main_gworld(&mut memory),
        ppc_seed_dsp_back_gworld(&mut memory),
    ];
    // Loader-owned screen worlds remain lazy compatibility projections so two
    // pristine adapters can attach to the same process without presenting
    // duplicate populated registries. The first pixel-state operation adopts
    // their non-purgeable mirror; NewGWorld records are registered eagerly.
    let gworld_pixel_states = SharedProcessQuickDrawPixelStates::default();
    if let Some((base, len)) = system_reservation {
        if memory.mapping_overlaps(base, len) {
            return Err(PpcLoadError::AddressOverflow);
        }
    }
    memory.add_region(
        PPC_HEAP_BASE,
        vec![0u8; usize::try_from(stack_base - PPC_HEAP_BASE).unwrap()],
    );
    for library in &initial_library_plans {
        if !library.plan.publish(&mut memory) {
            return Err(PpcLoadError::BundledLibraryLoad {
                library_name: library.library_name.clone(),
                error: PPC_FRAG_NO_ADDR_SPACE,
            });
        }
    }
    ppc_update_zone_free_bytes(&mut memory, heap_cursor, stack_base);

    let mut startup_initializers = Vec::new();
    for library in &initial_library_plans {
        let connection = cfm_connections
            .iter()
            .find(|connection| connection.library_name == library.library_name)
            .expect("planned CFM library has a connection");
        if connection.init_addr == 0 {
            continue;
        }
        let fragment_size = u32::try_from(library.fragment_bytes.len())
            .map_err(|_| PpcLoadError::AddressOverflow)?;
        let fragment_addr = ppc_heap_alloc(
            &mut memory,
            &mut heap_cursor,
            stack_base,
            fragment_size,
            false,
        );
        if fragment_addr == 0
            || memory
                .write_bytes(fragment_addr, &library.fragment_bytes)
                .is_none()
        {
            return Err(PpcLoadError::BundledLibraryLoad {
                library_name: library.library_name.clone(),
                error: PPC_FRAG_NO_MEM,
            });
        }
        launch_partition_storage.outside_partition = launch_partition_storage
            .outside_partition
            .saturating_add(fragment_size);
        let init_block = if let Some(disk) = disk_libraries.iter().find(|disk| {
            disk.name.eq_ignore_ascii_case(&library.library_name)
        }) {
            ppc_create_disk_fragment_init_block(
                &mut memory,
                &mut heap_cursor,
                stack_base,
                connection.id,
                disk.locator(),
                &library.library_name,
            )?
        } else {
            ppc_create_mem_fragment_init_block(
                None,
                &mut memory,
                &mut heap_cursor,
                stack_base,
                connection.id,
                fragment_addr,
                fragment_size,
                &library.library_name,
            )
            .map_err(|error| PpcLoadError::BundledLibraryLoad {
                library_name: library.library_name.clone(),
                error,
            })?
        };
        startup_initializers.push((connection.init_addr, init_block));
    }
    let library_initializer_count = startup_initializers.len();
    let mut application_startup = None;

    if let Some((init_addr, init_entry, init_rtoc)) = init_tvector {
        // Inside Macintosh: PowerPC System Software (1994), pp. 3-15--3-18
        // requires CFM to call a fragment initializer before its main routine.
        // Retain a guest-visible PEF copy for direct in-memory loads. File-backed
        // launches receive the documented disk locator in their InitBlock.
        let fragment_size = u32::try_from(data.len()).map_err(|_| PpcLoadError::AddressOverflow)?;
        let fragment_addr = ppc_heap_alloc(
            &mut memory,
            &mut heap_cursor,
            stack_base,
            fragment_size,
            false,
        );
        if fragment_addr == 0 || memory.write_bytes(fragment_addr, data).is_none() {
            return Err(PpcLoadError::AddressOverflow);
        }
        launch_partition_storage.outside_partition = launch_partition_storage
            .outside_partition
            .saturating_add(fragment_size);
        let init_block = if let Some(disk) = application_disk_fragment {
            ppc_create_disk_fragment_init_block(
                &mut memory,
                &mut heap_cursor,
                stack_base,
                next_cfm_connection_id,
                disk,
                application_fragment_name.unwrap_or("application"),
            )?
        } else {
            ppc_create_mem_fragment_init_block(
                None,
                &mut memory,
                &mut heap_cursor,
                stack_base,
                next_cfm_connection_id,
                fragment_addr,
                fragment_size,
                application_fragment_name.unwrap_or("application"),
            )
            .map_err(|_| PpcLoadError::AddressOverflow)?
        };
        application_startup = Some((init_addr, init_entry, init_rtoc, init_block));
    }

    // Inside Macintosh: PowerPC System Software (1994), p. 3-6: the
    // application fragment also acts as an import library for other CFM
    // fragments. Its cfrg name and mapped exports must be visible even when
    // the application has no initializer.
    if application_fragment_name.is_some() || init_tvector.is_some() {
        cfm_connections.push(PpcCfmConnection {
            id: next_cfm_connection_id,
            library_name: application_fragment_name
                .unwrap_or("application")
                .to_string(),
            main_addr: main_tvector,
            init_addr: init_tvector.map_or(0, |(addr, _, _)| addr),
            term_addr: term_tvector.map_or(0, |(addr, _, _)| addr),
            exports: application_exports,
        });
        next_cfm_connection_id = next_cfm_connection_id
            .checked_add(1)
            .ok_or(PpcLoadError::AddressOverflow)?;
    }

    let startup = if library_initializer_count == 0 {
        if let Some((_, init_entry, init_rtoc, init_block)) = application_startup {
            Some((
                init_entry,
                init_rtoc,
                init_block,
                PPC_APPLICATION_INIT_RETURN_PC,
            ))
        } else {
            None
        }
    } else {
        if let Some((init_addr, _, _, init_block)) = application_startup {
            startup_initializers.push((init_addr, init_block));
        }
        let bytes = ppc_initializers_trampoline(&startup_initializers, entry_pc, rtoc);
        let size = u32::try_from(bytes.len()).map_err(|_| PpcLoadError::AddressOverflow)?;
        if size > PPC_APPLICATION_INIT_RETURN_PC - PPC_INITIALIZERS_TRAMPOLINE_BASE
            || memory
                .readonly_allocation_overlap_end(PPC_INITIALIZERS_TRAMPOLINE_BASE, size)
                .is_some()
            || memory
                .publish_system_code(GuestIsa::PowerPc, PPC_INITIALIZERS_TRAMPOLINE_BASE, bytes)
                .is_none()
        {
            return Err(PpcLoadError::AddressOverflow);
        }
        Some((PPC_INITIALIZERS_TRAMPOLINE_BASE, rtoc, 0, PPC_HALT_PC))
    };

    let stack_pointer = stack_top - PPC_INITIAL_STACK_FRAME_SIZE;
    let mut stack = vec![0u8; stack_size as usize];
    let sp_offset = usize::try_from(stack_pointer - stack_base).unwrap();
    stack[sp_offset..sp_offset + 4].copy_from_slice(&0u32.to_be_bytes());
    memory.add_region(stack_base, stack);
    let mut cpu = PpcCpu::new();
    cpu.alignment_policy = PpcAlignmentPolicy::EmulateData;
    cpu.pc = startup.map_or(entry_pc, |startup| startup.0);
    cpu.gpr[1] = stack_pointer;
    cpu.gpr[2] = startup.map_or(rtoc, |startup| startup.1);
    cpu.gpr[3] = startup.map_or(0, |startup| startup.2);
    cpu.lr = startup.map_or(PPC_HALT_PC, |startup| startup.3);
    let mut toolbox_startup = PpcToolboxStartupState::default();
    let mut handles = Vec::new();
    let mut screen_clut = TrapDispatcher::standard_mac_8bpp_clut();
    let mut color_manager_clut = screen_clut;
    let process_memory_manager = PpcProcessMemoryManager::with_heap(heap_cursor, stack_base);
    if config.screen_depth != PPC_MAIN_PIXEL_DEPTH {
        let saved_r3 = cpu.gpr[3];
        let saved_r4 = cpu.gpr[4];
        cpu.gpr[3] = PPC_MAIN_GDEVICE;
        cpu.gpr[4] = config.screen_depth;
        let mut last_mem_error = PPC_NO_ERR;
        let shared_memory_manager = process_memory_manager.0.clone();
        let mut memory_manager = shared_memory_manager.borrow_mut();
        let mut allocator = PpcProcessAllocatorView {
            memory_manager: memory_manager.native_mut(),
        };
        let result = ppc_set_depth(
            &cpu,
            Some(&mut allocator),
            &mut memory,
            &mut heap_cursor,
            stack_base,
            &mut last_mem_error,
            &mut handles,
            &mut gworlds,
            &mut toolbox_startup,
            &mut screen_clut,
            &mut color_manager_clut,
        );
        cpu.gpr[3] = saved_r3;
        cpu.gpr[4] = saved_r4;
        if result != PPC_NO_ERR {
            return Err(PpcLoadError::ScreenDepthOutOfRange {
                requested: config.screen_depth,
            });
        }
    }

    let sound = PpcSoundState::default();
    sound
        .manager
        .set_default_output_volume(PPC_DEFAULT_OUTPUT_VOLUME);

    Ok(PpcLoadedApp {
        cpu,
        memory,
        entry_pc,
        rtoc,
        stack_base,
        stack_size,
        stack_pointer,
        launch_partition_storage,
        tick_state: SharedProcessTickState::default(),
        clock_cycles_per_tick: 1,
        clock_cycle_phase: 0,
        trap_default_gateways: HashMap::new(),
        native_exception_handler: 0,
        native_exception_stack: Vec::new(),
        stdc_qsort_stack: Vec::new(),
        dialog_callback_stack: Vec::new(),
        collection_callback_stack: Vec::new(),
        pending_file_completions: VecDeque::new(),
        file_completion_context: None,
        notification_response_context: None,
        parked_interrupt_callback: None,
        interrupt_callback_parks: 0,
        application_size: Default::default(),
        apple_events: PpcAppleEventState::default(),
        cfm: Some(PpcCfmState {
            connections: cfm_connections,
            library_fragments,
            next_connection_id: next_cfm_connection_id,
        }),
        controls: SharedProcessControlManager::default(),
        aliases: Vec::new(),
        gworlds,
        agl: PpcAglState::default(),
        gworld_pixel_states,
        q3_objects: Vec::new(),
        q3_object_refs: Vec::new(),
        next_q3_object: PPC_Q3_OBJECT_BASE,
        q3_error_state: PpcQ3ErrorState::default(),
        q3_lifecycle: PpcQ3LifecycleState::default(),
        q3_memory_storages: Vec::new(),
        q3_files: Vec::new(),
        q3_group_memberships: Vec::new(),
        q3_file_groups: Vec::new(),
        q3_views: Vec::new(),
        q3_submissions: Vec::new(),
        q3_view_transforms: Vec::new(),
        q3_submission_transforms: Vec::new(),
        q3_view_materials: Vec::new(),
        q3_submission_materials: Vec::new(),
        q3_submission_lights: Vec::new(),
        q3_view_state_stack: Vec::new(),
        q3_completed_frames: Vec::new(),
        q3_retained_frames: Vec::new(),
        q3_state_only_completed_frame_batches: Vec::new(),
        q3_fog_styles: Vec::new(),
        q3_immediate_trimeshes: PpcQ3ImmediateTriMeshStore::default(),
        q3_attributes: Vec::new(),
        q3_shader_uv_transforms: Vec::new(),
        q3_shader_boundaries: Vec::new(),
        q3_mipmap_textures: Vec::new(),
        q3_texture_shaders: Vec::new(),
        q3_renderer_preferences: Vec::new(),
        q3_draw_contexts: Vec::new(),
        q3_trimeshes: Vec::new(),
        q3_styles: Vec::new(),
        q3_cameras: Vec::new(),
        q3_lights: Vec::new(),
        input_sprocket: PpcInputSprocketState::default(),
        input_sprocket_virtual_elements: Vec::new(),
        toolbox_startup,
        quicktime: PpcQuickTimeState::default(),
        sound,
        timer_tasks: Default::default(),
        vbl_tasks: Default::default(),
        callback_scheduling: Default::default(),
        process_file_system: ppc_initial_process_file_system(),
        current_gworld: SharedProcessGraphicsPort::from_value(PPC_MAIN_GWORLD),
        current_gdevice: SharedProcessGraphicsDevice::from_value(PPC_MAIN_GDEVICE),
        quickdraw_op_colors: SharedProcessQuickDrawOpColors::default(),
        quickdraw_hilite_colors: SharedProcessQuickDrawHiliteColors::default(),
        screen_clut: SharedProcessDisplayClut::from_value(screen_clut),
        color_manager_clut: SharedProcessDisplayClut::from_value(color_manager_clut),
        display_gamma: SharedProcessDisplayGamma::default(),
        process_quickdraw_port_state_attached: false,
        quickdraw_fore_color: PPC_RGB_BLACK,
        quickdraw_fore_indices: HashMap::new(),
        quickdraw_back_color: PPC_RGB_WHITE,
        quickdraw_pen_h: 0,
        quickdraw_pen_v: 0,
        quickdraw_text_mode: PPC_QD_TEXT_MODE_SRC_OR,
        quickdraw_text_size: PPC_QD_TEXT_SIZE_SYSTEM,
        cursor_state: SharedProcessCursorState::default(),
        param_text: SharedProcessDialogText::default(),
        scrap: PpcScrapState::default(),
        list_manager: PpcListManagerState::default(),
        collections: SharedProcessCollectionManager::default(),
        halt_pc: PPC_HALT_PC,
        import_trap_base: PPC_IMPORT_TRAP_BASE,
        import_count,
        imports,
        section_bases,
        input: PpcInputSnapshot::default(),
        process_input: SharedProcessInputState::default(),
        event_queue: SharedProcessEventQueue::default(),
        window_list: Default::default(),
        process_memory_manager,
        draw_sprocket: PpcDrawSprocketState::default(),
        glm_mode: None,
        glm_callbacks: [None; 8],
        glm_callback_stack: Vec::new(),
        glm_allocations: HashMap::new(),
        glm_page_free_all_queue: VecDeque::new(),
        glm_error: 0,
    })
}

pub(crate) fn map_instantiated_sections(data: &[u8]) -> Result<Vec<MappedSection>, PpcLoadError> {
    let instantiated = instantiate_pef_sections(data).ok_or(PpcLoadError::SectionInstantiation)?;
    let mut mapped = Vec::with_capacity(instantiated.len());
    let mut code_cursor = PPC_CODE_BASE;
    let mut data_cursor = PPC_DATA_BASE;

    for section in instantiated {
        let align = alignment_bytes(section.header.alignment)?;
        let size = section.bytes.len() as u32;
        let base = if section.header.section_kind == SECTION_KIND_CODE {
            code_cursor = align_up(code_cursor, align)?;
            let base = code_cursor;
            code_cursor = code_cursor
                .checked_add(size)
                .ok_or(PpcLoadError::AddressOverflow)?;
            base
        } else {
            data_cursor = align_up(data_cursor, align)?;
            let base = data_cursor;
            data_cursor = data_cursor
                .checked_add(size)
                .ok_or(PpcLoadError::AddressOverflow)?;
            base
        };
        mapped.push(MappedSection {
            index: section.index,
            section_kind: section.header.section_kind,
            base,
            bytes: section.bytes,
        });
    }

    Ok(mapped)
}

fn relocation_import_symbol(
    imports: &[PpcImportBinding],
    symbol_index: u32,
) -> Option<PpcRelocationImportSymbol> {
    let import = imports
        .iter()
        .find(|import| import.symbol_index == symbol_index)?;
    Some(PpcRelocationImportSymbol {
        symbol_index: import.symbol_index,
        library_name: import.library_name.clone(),
        symbol_name: import.symbol_name.clone(),
        class: import.class,
        weak: import.weak,
    })
}

fn import_tvector_bytes(count: usize) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(count * 8);
    for index in 0..count {
        let trap_pc = PPC_IMPORT_TRAP_BASE + (index as u32) * 4;
        bytes.extend_from_slice(&trap_pc.to_be_bytes());
        bytes.extend_from_slice(&0u32.to_be_bytes());
    }
    bytes
}

fn import_trap_bytes(count: usize) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(count * 4);
    for _ in 0..count {
        bytes.extend_from_slice(&BLR.to_be_bytes());
    }
    bytes
}

fn ppc_application_init_return_trampoline(main_entry: u32, main_rtoc: u32) -> Vec<u8> {
    // r3 is the initializer's OSErr. A failed initializer terminates launch;
    // success restores the main routine's TOC and branches to its entry point.
    // The generated code is ordinary PowerPC ABI glue, not an HLE import.
    let words = [
        0x2c03_0000, // cmpwi r3, 0
        0x4082_0020, // bne failure
        0x3c40_0000 | (main_rtoc >> 16),
        0x6042_0000 | (main_rtoc & 0xffff),
        0x3d80_0000 | (main_entry >> 16),
        0x618c_0000 | (main_entry & 0xffff),
        0x7d89_03a6, // mtctr r12
        0x4e80_0420, // bctr
        0x3980_0000, // failure: li r12, 0
        0x7d89_03a6, // mtctr r12
        0x4e80_0420, // bctr
    ];
    words.into_iter().flat_map(u32::to_be_bytes).collect()
}

fn ppc_initializers_trampoline(
    initializers: &[(u32, u32)],
    main_entry: u32,
    main_rtoc: u32,
) -> Vec<u8> {
    // Each initializer receives its InitBlock in r3 through the descriptor's
    // entry/TOC pair. A nonzero OSErr stops launch; only a fully initialized
    // dependency chain reaches the application's main routine.
    let mut words = Vec::new();
    for (descriptor, init_block) in initializers {
        words.extend_from_slice(&[
            0x3c60_0000 | (init_block >> 16),
            0x6063_0000 | (init_block & 0xffff),
            0x3d80_0000 | (descriptor >> 16),
            0x618c_0000 | (descriptor & 0xffff),
            0x800c_0000, // lwz r0, 0(r12)
            0x804c_0004, // lwz r2, 4(r12)
            0x7c09_03a6, // mtctr r0
            0x4e80_0421, // bctrl
            0x2c03_0000, // cmpwi r3, 0
            0x4182_0010, // beq next initializer/main
            0x3980_0000, // li r12, 0
            0x7d89_03a6, // mtctr r12
            0x4e80_0420, // bctr
        ]);
    }
    words.extend_from_slice(&[
        0x3c40_0000 | (main_rtoc >> 16),
        0x6042_0000 | (main_rtoc & 0xffff),
        0x3d80_0000 | (main_entry >> 16),
        0x618c_0000 | (main_entry & 0xffff),
        0x7d89_03a6, // mtctr r12
        0x4e80_0420, // bctr
    ]);
    words.into_iter().flat_map(u32::to_be_bytes).collect()
}

fn alignment_bytes(power: u8) -> Result<u32, PpcLoadError> {
    if power >= 31 {
        return Err(PpcLoadError::AddressOverflow);
    }
    Ok(1u32 << power)
}

pub(crate) fn align_up(value: u32, align: u32) -> Result<u32, PpcLoadError> {
    if align <= 1 {
        return Ok(value);
    }
    let mask = align - 1;
    value
        .checked_add(mask)
        .map(|v| v & !mask)
        .ok_or(PpcLoadError::AddressOverflow)
}

fn normalize_stack_size(requested: u32) -> Result<u32, PpcLoadError> {
    let requested = if requested == 0 {
        PPC_DEFAULT_STACK_SIZE
    } else {
        requested
    };
    let requested = requested.max(PPC_INITIAL_STACK_FRAME_SIZE);
    let aligned =
        align_up(requested, 16).map_err(|_| PpcLoadError::StackSizeOutOfRange { requested })?;
    if aligned > PPC_MAX_STACK_SIZE {
        return Err(PpcLoadError::StackSizeOutOfRange { requested });
    }
    Ok(aligned)
}

fn read_u32(data: &[u8], offset: usize) -> Option<u32> {
    Some(u32::from_be_bytes(
        data.get(offset..offset + 4)?.try_into().ok()?,
    ))
}
