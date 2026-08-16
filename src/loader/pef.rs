//! Preferred Executable Format (PEF) parsing helpers for PowerPC CFM apps.
//!
//! This module intentionally stays pure: it parses container metadata,
//! materializes initialized section bytes, and resolves the main transition
//! vector. Relocation application, CFM import binding, and PPC execution are
//! separate loader phases.

use crate::trap::types::decode_mac_roman;

const PEF_HEADER_SIZE: usize = 40;
const PEF_SECTION_HEADER_SIZE: usize = 28;
const PEF_LOADER_HEADER_SIZE: usize = 56;
const PEF_IMPORTED_LIBRARY_SIZE: usize = 24;
const PEF_IMPORTED_SYMBOL_SIZE: usize = 4;
const PEF_RELOCATION_HEADER_SIZE: usize = 12;

pub const SECTION_KIND_CODE: u8 = 0;
pub const SECTION_KIND_UNPACKED_DATA: u8 = 1;
pub const SECTION_KIND_PATTERN_DATA: u8 = 2;
pub const SECTION_KIND_CONSTANT: u8 = 3;
pub const SECTION_KIND_LOADER: u8 = 4;
pub const SECTION_KIND_DEBUG: u8 = 5;
pub const SECTION_KIND_EXECUTABLE_DATA: u8 = 6;

/// Detect a PEF container in a classic Mac data fork.
pub fn data_fork_is_pef(data: &[u8]) -> bool {
    data.len() >= 8 && &data[0..8] == b"Joy!peff"
}

/// Parsed 40-byte PEF container header.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PefHeader {
    pub architecture: [u8; 4],
    pub format_version: u32,
    pub section_count: u16,
    pub instantiated_section_count: u16,
}

/// One entry from the PEF section header table.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PefSection {
    pub name_offset: i32,
    pub default_address: u32,
    pub total_size: u32,
    pub unpacked_size: u32,
    pub packed_size: u32,
    pub container_offset: u32,
    pub section_kind: u8,
    pub share_kind: u8,
    pub alignment: u8,
}

impl PefSection {
    pub fn kind_name(self) -> &'static str {
        match self.section_kind {
            SECTION_KIND_CODE => "code",
            SECTION_KIND_UNPACKED_DATA => "unpacked_data",
            SECTION_KIND_PATTERN_DATA => "pattern_data",
            SECTION_KIND_CONSTANT => "constant",
            SECTION_KIND_LOADER => "loader",
            SECTION_KIND_DEBUG => "debug",
            SECTION_KIND_EXECUTABLE_DATA => "executable_data",
            _ => "reserved",
        }
    }
}

/// Parsed 56-byte loader-section header.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PefLoaderHeader {
    pub main_section: i32,
    pub main_offset: u32,
    pub init_section: i32,
    pub init_offset: u32,
    pub term_section: i32,
    pub term_offset: u32,
    pub imported_library_count: u32,
    pub total_imported_symbol_count: u32,
    pub reloc_section_count: u32,
    pub reloc_instr_offset: u32,
    pub loader_strings_offset: u32,
    pub export_hash_offset: u32,
    pub export_hash_table_power: u32,
    pub exported_symbol_count: u32,
}

/// One 24-byte imported-library table entry from the loader section.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PefImportedLibrary {
    pub name_offset: u32,
    pub old_imp_version: u32,
    pub current_version: u32,
    pub imported_symbol_count: u32,
    pub first_imported_symbol: u32,
    pub options: u8,
}

impl PefImportedLibrary {
    pub fn init_before_client(self) -> bool {
        (self.options & 0x80) != 0
    }

    pub fn weak_import(self) -> bool {
        (self.options & 0x40) != 0
    }
}

/// One packed 4-byte imported-symbol table entry.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PefImportedSymbol {
    pub class: u8,
    pub name_offset: u32,
    pub weak: bool,
}

impl PefImportedSymbol {
    pub fn class_name(self) -> &'static str {
        match self.class {
            0 => "code",
            1 => "data",
            2 => "tvector",
            3 => "toc",
            4 => "glue",
            _ => "reserved",
        }
    }
}

/// One 12-byte relocation-header table entry from the loader section.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PefRelocHeader {
    pub section_index: u16,
    pub reloc_count: u32,
    pub first_reloc_offset: u32,
}

/// Inputs for applying relocation instructions to one instantiated section.
#[derive(Debug, Clone, Copy)]
pub struct PefRelocContext<'a> {
    pub code_base: u32,
    pub data_base: u32,
    pub section_bases: &'a [Option<u32>],
    pub import_addrs: &'a [u32],
}

/// Relocation failure with the interpreter state needed for loader diagnostics.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PefRelocFailure {
    pub error: PefRelocApplyError,
    pub reloc_offset: u32,
    pub section_position: u32,
    pub import_index: Option<u32>,
}

/// Reasons the relocation interpreter may refuse a chunk stream.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PefRelocApplyError {
    DecodeError(PefRelocDecodeError),
    TruncatedStream,
    OutOfRange { position: u32, section_len: u32 },
    ImportIndexOutOfRange { index: u32, import_count: u32 },
    SectionIndexOutOfRange { index: u32, section_count: u32 },
    SectionBaseUnavailable { index: u32 },
    SmRepeatUnderflow { chunk_count: u32, history_len: u32 },
}

/// One decoded PEF relocation instruction.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PefRelocOp {
    DDat { skip_count: u8, reloc_count: u8 },
    BySectC { run_length: u16 },
    BySectD { run_length: u16 },
    TVector12 { run_length: u16 },
    TVector8 { run_length: u16 },
    VTable8 { run_length: u16 },
    ImportRun { run_length: u16 },
    SmByImport { index: u16 },
    SmSetSectC { index: u16 },
    SmSetSectD { index: u16 },
    SmBySection { index: u16 },
    IncrPosition { offset: u16 },
    SmRepeat { chunk_count: u8, repeat_count: u16 },
    SetPosition { offset: u32 },
    LgByImport { index: u32 },
    LgRepeat { chunk_count: u8, repeat_count: u32 },
    LgBySection { index: u32 },
    LgSetSectC { index: u32 },
    LgSetSectD { index: u32 },
}

/// Decode failure for one relocation instruction.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PefRelocDecodeError {
    UnknownOpcode { first_chunk: u16 },
    TruncatedTwoChunk { first_chunk: u16 },
    UnknownLsecSubopcode { first_chunk: u16, subopcode: u8 },
}

/// Name-resolved import table row, preserving the original symbol index.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PefResolvedImport {
    pub library_index: u32,
    pub symbol_index: u32,
    pub library_name: String,
    pub symbol_name: String,
    pub class: u8,
    pub weak: bool,
}

/// Runtime bytes for one instantiated PEF section.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PefInstantiatedSection {
    pub index: usize,
    pub header: PefSection,
    pub bytes: Vec<u8>,
}

/// Main PowerPC transition vector resolved from the loader header.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PefEntryPoint {
    pub entry_pc: u32,
    pub rtoc: u32,
    pub main_section: i32,
    pub main_offset: u32,
}

pub fn parse_pef_header(data: &[u8]) -> Option<PefHeader> {
    if !data_fork_is_pef(data) || data.len() < PEF_HEADER_SIZE {
        return None;
    }

    Some(PefHeader {
        architecture: data.get(8..12)?.try_into().ok()?,
        format_version: read_u32(data, 12)?,
        section_count: read_u16(data, 32)?,
        instantiated_section_count: read_u16(data, 34)?,
    })
}

pub fn parse_pef_sections(data: &[u8]) -> Option<Vec<PefSection>> {
    let header = parse_pef_header(data)?;
    let count = usize::from(header.section_count);
    let table_end = PEF_HEADER_SIZE.checked_add(count.checked_mul(PEF_SECTION_HEADER_SIZE)?)?;
    if data.len() < table_end {
        return None;
    }

    let mut sections = Vec::with_capacity(count);
    for index in 0..count {
        let base = PEF_HEADER_SIZE + index * PEF_SECTION_HEADER_SIZE;
        sections.push(PefSection {
            name_offset: read_i32(data, base)?,
            default_address: read_u32(data, base + 4)?,
            total_size: read_u32(data, base + 8)?,
            unpacked_size: read_u32(data, base + 12)?,
            packed_size: read_u32(data, base + 16)?,
            container_offset: read_u32(data, base + 20)?,
            section_kind: *data.get(base + 24)?,
            share_kind: *data.get(base + 25)?,
            alignment: *data.get(base + 26)?,
        });
    }

    Some(sections)
}

pub fn parse_pef_loader_header(data: &[u8]) -> Option<PefLoaderHeader> {
    let sections = parse_pef_sections(data)?;
    let loader = loader_section(&sections)?;
    let off = usize::try_from(loader.container_offset).ok()?;
    let end = off.checked_add(PEF_LOADER_HEADER_SIZE)?;
    if data.len() < end {
        return None;
    }

    Some(PefLoaderHeader {
        main_section: read_i32(data, off)?,
        main_offset: read_u32(data, off + 4)?,
        init_section: read_i32(data, off + 8)?,
        init_offset: read_u32(data, off + 12)?,
        term_section: read_i32(data, off + 16)?,
        term_offset: read_u32(data, off + 20)?,
        imported_library_count: read_u32(data, off + 24)?,
        total_imported_symbol_count: read_u32(data, off + 28)?,
        reloc_section_count: read_u32(data, off + 32)?,
        reloc_instr_offset: read_u32(data, off + 36)?,
        loader_strings_offset: read_u32(data, off + 40)?,
        export_hash_offset: read_u32(data, off + 44)?,
        export_hash_table_power: read_u32(data, off + 48)?,
        exported_symbol_count: read_u32(data, off + 52)?,
    })
}

pub fn parse_pef_imported_libraries(data: &[u8]) -> Option<Vec<PefImportedLibrary>> {
    let sections = parse_pef_sections(data)?;
    let loader = loader_section(&sections)?;
    let header = parse_pef_loader_header(data)?;
    let count = usize::try_from(header.imported_library_count).ok()?;
    let loader_base = usize::try_from(loader.container_offset).ok()?;
    let table_start = loader_base.checked_add(PEF_LOADER_HEADER_SIZE)?;
    let table_end = table_start.checked_add(count.checked_mul(PEF_IMPORTED_LIBRARY_SIZE)?)?;
    if data.len() < table_end {
        return None;
    }

    let mut libraries = Vec::with_capacity(count);
    for index in 0..count {
        let base = table_start + index * PEF_IMPORTED_LIBRARY_SIZE;
        libraries.push(PefImportedLibrary {
            name_offset: read_u32(data, base)?,
            old_imp_version: read_u32(data, base + 4)?,
            current_version: read_u32(data, base + 8)?,
            imported_symbol_count: read_u32(data, base + 12)?,
            first_imported_symbol: read_u32(data, base + 16)?,
            options: *data.get(base + 20)?,
        });
    }

    Some(libraries)
}

pub fn parse_pef_imported_symbols(data: &[u8]) -> Option<Vec<PefImportedSymbol>> {
    let sections = parse_pef_sections(data)?;
    let loader = loader_section(&sections)?;
    let header = parse_pef_loader_header(data)?;
    let total = usize::try_from(header.total_imported_symbol_count).ok()?;
    let lib_count = usize::try_from(header.imported_library_count).ok()?;
    let loader_base = usize::try_from(loader.container_offset).ok()?;
    let table_start = loader_base
        .checked_add(PEF_LOADER_HEADER_SIZE)?
        .checked_add(lib_count.checked_mul(PEF_IMPORTED_LIBRARY_SIZE)?)?;
    let table_end = table_start.checked_add(total.checked_mul(PEF_IMPORTED_SYMBOL_SIZE)?)?;
    if data.len() < table_end {
        return None;
    }

    let mut symbols = Vec::with_capacity(total);
    for index in 0..total {
        let base = table_start + index * PEF_IMPORTED_SYMBOL_SIZE;
        let class_byte = *data.get(base)?;
        let name_offset = (u32::from(*data.get(base + 1)?) << 16)
            | (u32::from(*data.get(base + 2)?) << 8)
            | u32::from(*data.get(base + 3)?);
        symbols.push(PefImportedSymbol {
            class: class_byte & 0x0f,
            name_offset,
            weak: (class_byte & 0x80) != 0,
        });
    }

    Some(symbols)
}

pub fn parse_pef_reloc_headers(data: &[u8]) -> Option<Vec<PefRelocHeader>> {
    let sections = parse_pef_sections(data)?;
    let loader = loader_section(&sections)?;
    let header = parse_pef_loader_header(data)?;
    let count = usize::try_from(header.reloc_section_count).ok()?;
    let lib_count = usize::try_from(header.imported_library_count).ok()?;
    let symbol_count = usize::try_from(header.total_imported_symbol_count).ok()?;
    let loader_base = usize::try_from(loader.container_offset).ok()?;
    let table_start = loader_base
        .checked_add(PEF_LOADER_HEADER_SIZE)?
        .checked_add(lib_count.checked_mul(PEF_IMPORTED_LIBRARY_SIZE)?)?
        .checked_add(symbol_count.checked_mul(PEF_IMPORTED_SYMBOL_SIZE)?)?;
    let table_end = table_start.checked_add(count.checked_mul(PEF_RELOCATION_HEADER_SIZE)?)?;
    if data.len() < table_end {
        return None;
    }

    let mut headers = Vec::with_capacity(count);
    for index in 0..count {
        let base = table_start + index * PEF_RELOCATION_HEADER_SIZE;
        headers.push(PefRelocHeader {
            section_index: read_u16(data, base)?,
            reloc_count: read_u32(data, base + 4)?,
            first_reloc_offset: read_u32(data, base + 8)?,
        });
    }
    Some(headers)
}

pub fn pef_reloc_chunk_stream<'a>(data: &'a [u8], reloc: &PefRelocHeader) -> Option<&'a [u8]> {
    let sections = parse_pef_sections(data)?;
    let loader = loader_section(&sections)?;
    let header = parse_pef_loader_header(data)?;
    let loader_base = usize::try_from(loader.container_offset).ok()?;
    let reloc_instr_offset = usize::try_from(header.reloc_instr_offset).ok()?;
    let first_reloc_offset = usize::try_from(reloc.first_reloc_offset).ok()?;
    let chunk_count = usize::try_from(reloc.reloc_count).ok()?;
    let start = loader_base
        .checked_add(reloc_instr_offset)?
        .checked_add(first_reloc_offset)?;
    let end = start.checked_add(chunk_count.checked_mul(2)?)?;
    data.get(start..end)
}

pub fn resolve_pef_imports(data: &[u8]) -> Option<Vec<PefResolvedImport>> {
    let libraries = parse_pef_imported_libraries(data)?;
    let symbols = parse_pef_imported_symbols(data)?;
    let mut imports = Vec::new();

    for (library_index, library) in libraries.iter().enumerate() {
        let library_name = pef_loader_string_at(data, library.name_offset)?;
        let first = usize::try_from(library.first_imported_symbol).ok()?;
        let count = usize::try_from(library.imported_symbol_count).ok()?;
        let end = first.checked_add(count)?;
        if end > symbols.len() {
            return None;
        }

        for (offset, symbol) in symbols[first..end].iter().enumerate() {
            imports.push(PefResolvedImport {
                library_index: u32::try_from(library_index).ok()?,
                symbol_index: u32::try_from(first + offset).ok()?,
                library_name: library_name.clone(),
                symbol_name: pef_loader_string_at(data, symbol.name_offset)?,
                class: symbol.class,
                weak: symbol.weak,
            });
        }
    }

    Some(imports)
}

pub fn apply_pef_relocations(
    section_bytes: &mut [u8],
    chunk_stream: &[u8],
    ctx: &PefRelocContext<'_>,
) -> Result<(), PefRelocApplyError> {
    apply_pef_relocations_detailed(section_bytes, chunk_stream, ctx)
        .map_err(|failure| failure.error)
}

pub fn apply_pef_relocations_detailed(
    section_bytes: &mut [u8],
    chunk_stream: &[u8],
    ctx: &PefRelocContext<'_>,
) -> Result<(), PefRelocFailure> {
    let mut position = 0u32;
    let mut import_index = 0u32;
    let mut code_base = ctx.code_base;
    let mut data_base = ctx.data_base;
    let mut chunk_cursor = 0usize;
    let section_len = section_bytes.len() as u32;
    let mut instruction_starts = vec![false; chunk_stream.len() / 2 + 1];
    let mut pending_repeats: Vec<(usize, usize, usize, u32)> = Vec::new();

    while chunk_cursor < chunk_stream.len() {
        let this_start = chunk_cursor;
        instruction_starts[this_start / 2] = true;
        macro_rules! try_reloc {
            ($expr:expr) => {
                match $expr {
                    Ok(value) => value,
                    Err(error) => {
                        return Err(reloc_failure(error, this_start, position, None));
                    }
                }
            };
            ($expr:expr, $import_index:expr) => {
                match $expr {
                    Ok(value) => value,
                    Err(error) => {
                        return Err(reloc_failure(
                            error,
                            this_start,
                            position,
                            Some($import_index),
                        ));
                    }
                }
            };
        }

        let first = read_chunk(chunk_stream, chunk_cursor).ok_or_else(|| {
            reloc_failure(
                PefRelocApplyError::TruncatedStream,
                this_start,
                position,
                None,
            )
        })?;
        let second = read_chunk(chunk_stream, chunk_cursor + 2);
        let (op, consumed) = decode_pef_reloc(first, second).map_err(|error| {
            reloc_failure(
                PefRelocApplyError::DecodeError(error),
                this_start,
                position,
                None,
            )
        })?;
        chunk_cursor = chunk_cursor
            .checked_add(usize::from(consumed) * 2)
            .ok_or_else(|| {
                reloc_failure(
                    PefRelocApplyError::TruncatedStream,
                    this_start,
                    position,
                    None,
                )
            })?;

        let repeat = match op {
            PefRelocOp::SmRepeat {
                chunk_count,
                repeat_count,
            } => Some((chunk_count, u32::from(repeat_count))),
            PefRelocOp::LgRepeat {
                chunk_count,
                repeat_count,
            } => Some((chunk_count, repeat_count)),
            _ => None,
        };
        if let Some((chunk_count, repeat_count)) = repeat {
            let count = usize::from(chunk_count);
            let rewind_bytes = count.saturating_mul(2);
            let Some(rewind_to) = this_start.checked_sub(rewind_bytes) else {
                return Err(reloc_failure(
                    PefRelocApplyError::SmRepeatUnderflow {
                        chunk_count: u32::from(chunk_count),
                        history_len: (this_start / 2) as u32,
                    },
                    this_start,
                    position,
                    None,
                ));
            };
            // Mac OS Runtime Architectures (1997), pp. 8-32 and 8-34:
            // blockCount is a count of 16-bit relocation blocks, not decoded
            // instructions. Reject a rewind into the second half of a long op.
            if count == 0 || !instruction_starts[rewind_to / 2] {
                return Err(reloc_failure(
                    PefRelocApplyError::SmRepeatUnderflow {
                        chunk_count: u32::from(chunk_count),
                        history_len: (this_start / 2) as u32,
                    },
                    this_start,
                    position,
                    None,
                ));
            }

            let post_rpt = chunk_cursor;
            if repeat_count > 0 {
                pending_repeats.push((rewind_to, this_start, post_rpt, repeat_count));
                chunk_cursor = rewind_to;
            }
            continue;
        }
        match op {
            PefRelocOp::DDat {
                skip_count,
                reloc_count,
            } => {
                position = try_reloc!(checked_position_add(
                    position,
                    u32::from(skip_count) * 4,
                    section_len
                ));
                for _ in 0..reloc_count {
                    let value = try_reloc!(read_slot_u32(section_bytes, position, section_len));
                    try_reloc!(write_slot_u32(
                        section_bytes,
                        position,
                        value.wrapping_add(data_base),
                        section_len,
                    ));
                    position = try_reloc!(checked_position_add(position, 4, section_len));
                }
            }
            PefRelocOp::BySectC { run_length } => {
                for _ in 0..run_length {
                    let value = try_reloc!(read_slot_u32(section_bytes, position, section_len));
                    try_reloc!(write_slot_u32(
                        section_bytes,
                        position,
                        value.wrapping_add(code_base),
                        section_len,
                    ));
                    position = try_reloc!(checked_position_add(position, 4, section_len));
                }
            }
            PefRelocOp::BySectD { run_length } => {
                for _ in 0..run_length {
                    let value = try_reloc!(read_slot_u32(section_bytes, position, section_len));
                    try_reloc!(write_slot_u32(
                        section_bytes,
                        position,
                        value.wrapping_add(data_base),
                        section_len,
                    ));
                    position = try_reloc!(checked_position_add(position, 4, section_len));
                }
            }
            PefRelocOp::TVector8 { run_length } => {
                for _ in 0..run_length {
                    let pc = try_reloc!(read_slot_u32(section_bytes, position, section_len));
                    try_reloc!(write_slot_u32(
                        section_bytes,
                        position,
                        pc.wrapping_add(code_base),
                        section_len,
                    ));
                    let rtoc = try_reloc!(read_slot_u32(
                        section_bytes,
                        position.wrapping_add(4),
                        section_len
                    ));
                    try_reloc!(write_slot_u32(
                        section_bytes,
                        position.wrapping_add(4),
                        rtoc.wrapping_add(data_base),
                        section_len,
                    ));
                    position = try_reloc!(checked_position_add(position, 8, section_len));
                }
            }
            PefRelocOp::TVector12 { run_length } => {
                for _ in 0..run_length {
                    let pc = try_reloc!(read_slot_u32(section_bytes, position, section_len));
                    try_reloc!(write_slot_u32(
                        section_bytes,
                        position,
                        pc.wrapping_add(code_base),
                        section_len,
                    ));
                    let rtoc = try_reloc!(read_slot_u32(
                        section_bytes,
                        position.wrapping_add(4),
                        section_len
                    ));
                    try_reloc!(write_slot_u32(
                        section_bytes,
                        position.wrapping_add(4),
                        rtoc.wrapping_add(data_base),
                        section_len,
                    ));
                    position = try_reloc!(checked_position_add(position, 12, section_len));
                }
            }
            PefRelocOp::VTable8 { run_length } => {
                for _ in 0..run_length {
                    let tvector = try_reloc!(read_slot_u32(section_bytes, position, section_len));
                    // Mac OS Runtime Architectures (1997), p. 8-30: a
                    // RelocVTable8 entry's first word points to a transition
                    // vector in sectionD; its second word remains unchanged.
                    // See also PPC System Software (1994), p. 1-27, which
                    // specifies transition vectors as PPC procedure pointers.
                    try_reloc!(write_slot_u32(
                        section_bytes,
                        position,
                        tvector.wrapping_add(data_base),
                        section_len,
                    ));
                    position = try_reloc!(checked_position_add(position, 8, section_len));
                }
            }
            PefRelocOp::ImportRun { run_length } => {
                for _ in 0..run_length {
                    let addr = try_reloc!(import_addr(ctx, import_index), import_index);
                    try_reloc!(
                        write_slot_u32(section_bytes, position, addr, section_len),
                        import_index
                    );
                    position =
                        try_reloc!(checked_position_add(position, 4, section_len), import_index);
                    import_index = import_index.wrapping_add(1);
                }
            }
            PefRelocOp::SmByImport { index } => {
                let index = u32::from(index);
                let addr = try_reloc!(import_addr(ctx, index), index);
                try_reloc!(
                    write_slot_u32(section_bytes, position, addr, section_len),
                    index
                );
                position = try_reloc!(checked_position_add(position, 4, section_len), index);
                import_index = index.wrapping_add(1);
            }
            PefRelocOp::SmSetSectC { index } => {
                code_base = try_reloc!(section_base(ctx, u32::from(index)));
            }
            PefRelocOp::SmSetSectD { index } => {
                data_base = try_reloc!(section_base(ctx, u32::from(index)));
            }
            PefRelocOp::SmBySection { index } => {
                let base = try_reloc!(section_base(ctx, u32::from(index)));
                try_reloc!(write_slot_u32(section_bytes, position, base, section_len));
                position = try_reloc!(checked_position_add(position, 4, section_len));
            }
            PefRelocOp::LgByImport { index } => {
                let addr = try_reloc!(import_addr(ctx, index), index);
                try_reloc!(
                    write_slot_u32(section_bytes, position, addr, section_len),
                    index
                );
                position = try_reloc!(checked_position_add(position, 4, section_len), index);
                import_index = index.wrapping_add(1);
            }
            PefRelocOp::LgBySection { index } => {
                let base = try_reloc!(section_base(ctx, index));
                try_reloc!(write_slot_u32(section_bytes, position, base, section_len));
                position = try_reloc!(checked_position_add(position, 4, section_len));
            }
            PefRelocOp::LgSetSectC { index } => {
                code_base = try_reloc!(section_base(ctx, index));
            }
            PefRelocOp::LgSetSectD { index } => {
                data_base = try_reloc!(section_base(ctx, index));
            }
            PefRelocOp::IncrPosition { offset } => {
                position = try_reloc!(checked_position_add(
                    position,
                    u32::from(offset),
                    section_len
                ));
            }
            PefRelocOp::SetPosition { offset } => {
                position = try_reloc!(checked_position_add(0, offset, section_len));
            }
            PefRelocOp::SmRepeat { .. } | PefRelocOp::LgRepeat { .. } => unreachable!(),
        }

        while let Some(&last) = pending_repeats.last() {
            if chunk_cursor < last.1 {
                break;
            }
            let remaining = last.3 - 1;
            if remaining == 0 {
                chunk_cursor = last.2;
                pending_repeats.pop();
            } else {
                chunk_cursor = last.0;
                let index = pending_repeats.len() - 1;
                pending_repeats[index].3 = remaining;
                break;
            }
        }
    }

    Ok(())
}

pub fn decode_pef_reloc(
    first_chunk: u16,
    second_chunk: Option<u16>,
) -> Result<(PefRelocOp, u8), PefRelocDecodeError> {
    let dispatch = (first_chunk >> 9) as u8;

    if dispatch <= 0x1f {
        let skip_count = ((first_chunk >> 6) & 0xff) as u8;
        let reloc_count = (first_chunk & 0x3f) as u8;
        return Ok((
            PefRelocOp::DDat {
                skip_count,
                reloc_count,
            },
            1,
        ));
    }

    match dispatch {
        0x20..=0x25 => {
            let run_length = (first_chunk & 0x01ff) + 1;
            let op = match dispatch {
                0x20 => PefRelocOp::BySectC { run_length },
                0x21 => PefRelocOp::BySectD { run_length },
                0x22 => PefRelocOp::TVector12 { run_length },
                0x23 => PefRelocOp::TVector8 { run_length },
                0x24 => PefRelocOp::VTable8 { run_length },
                0x25 => PefRelocOp::ImportRun { run_length },
                _ => unreachable!(),
            };
            Ok((op, 1))
        }
        0x30..=0x33 => {
            let index = first_chunk & 0x01ff;
            let op = match dispatch {
                0x30 => PefRelocOp::SmByImport { index },
                0x31 => PefRelocOp::SmSetSectC { index },
                0x32 => PefRelocOp::SmSetSectD { index },
                0x33 => PefRelocOp::SmBySection { index },
                _ => unreachable!(),
            };
            Ok((op, 1))
        }
        0x40..=0x47 => {
            let offset = (first_chunk & 0x0fff) + 1;
            Ok((PefRelocOp::IncrPosition { offset }, 1))
        }
        0x48..=0x4f => {
            let chunk_count = (((first_chunk >> 8) & 0x0f) + 1) as u8;
            let repeat_count = (first_chunk & 0xff) + 1;
            Ok((
                PefRelocOp::SmRepeat {
                    chunk_count,
                    repeat_count,
                },
                1,
            ))
        }
        0x50 | 0x51 => {
            let second =
                second_chunk.ok_or(PefRelocDecodeError::TruncatedTwoChunk { first_chunk })?;
            let offset = ((u32::from(first_chunk) & 0x03ff) << 16) | u32::from(second);
            Ok((PefRelocOp::SetPosition { offset }, 2))
        }
        0x52 | 0x53 => {
            let second =
                second_chunk.ok_or(PefRelocDecodeError::TruncatedTwoChunk { first_chunk })?;
            let index = ((u32::from(first_chunk) & 0x03ff) << 16) | u32::from(second);
            Ok((PefRelocOp::LgByImport { index }, 2))
        }
        0x58 | 0x59 => {
            let second =
                second_chunk.ok_or(PefRelocDecodeError::TruncatedTwoChunk { first_chunk })?;
            let chunk_count = (((first_chunk >> 6) & 0x0f) + 1) as u8;
            let repeat_count = ((u32::from(first_chunk) & 0x003f) << 16) | u32::from(second);
            Ok((
                PefRelocOp::LgRepeat {
                    chunk_count,
                    repeat_count,
                },
                2,
            ))
        }
        0x5a | 0x5b => {
            let second =
                second_chunk.ok_or(PefRelocDecodeError::TruncatedTwoChunk { first_chunk })?;
            let subopcode = ((first_chunk >> 6) & 0x0f) as u8;
            let index = ((u32::from(first_chunk) & 0x003f) << 16) | u32::from(second);
            let op = match subopcode {
                0 => PefRelocOp::LgBySection { index },
                1 => PefRelocOp::LgSetSectC { index },
                2 => PefRelocOp::LgSetSectD { index },
                _ => {
                    return Err(PefRelocDecodeError::UnknownLsecSubopcode {
                        first_chunk,
                        subopcode,
                    });
                }
            };
            Ok((op, 2))
        }
        _ => Err(PefRelocDecodeError::UnknownOpcode { first_chunk }),
    }
}

/// Read a NUL-terminated MacRoman loader string by string-table offset.
pub fn pef_loader_string_at(data: &[u8], string_offset: u32) -> Option<String> {
    let sections = parse_pef_sections(data)?;
    let loader = loader_section(&sections)?;
    let header = parse_pef_loader_header(data)?;
    let loader_base = usize::try_from(loader.container_offset).ok()?;
    let string_table =
        loader_base.checked_add(usize::try_from(header.loader_strings_offset).ok()?)?;
    let abs = string_table.checked_add(usize::try_from(string_offset).ok()?)?;
    let loader_end = loader_section_end(data, loader)?;
    if abs >= loader_end {
        return None;
    }

    let tail = data.get(abs..loader_end)?;
    let nul = tail.iter().position(|&byte| byte == 0)?;
    Some(decode_mac_roman(&tail[..nul]))
}

pub fn instantiate_pef_section(data: &[u8], section: &PefSection) -> Option<Vec<u8>> {
    let total_size = usize::try_from(section.total_size).ok()?;
    let mut bytes = match section.section_kind {
        SECTION_KIND_CODE
        | SECTION_KIND_UNPACKED_DATA
        | SECTION_KIND_CONSTANT
        | SECTION_KIND_EXECUTABLE_DATA => {
            let start = usize::try_from(section.container_offset).ok()?;
            let initialized_size = usize::try_from(section.unpacked_size).ok()?;
            let end = start.checked_add(initialized_size)?;
            if end > data.len() {
                return None;
            }
            data[start..end].to_vec()
        }
        SECTION_KIND_PATTERN_DATA => {
            let start = usize::try_from(section.container_offset).ok()?;
            let packed_size = usize::try_from(section.packed_size).ok()?;
            let end = start.checked_add(packed_size)?;
            if end > data.len() {
                return None;
            }
            unpack_pef_pattern_data(&data[start..end], section.unpacked_size)?
        }
        _ => return None,
    };

    if bytes.len() > total_size {
        return None;
    }
    bytes.resize(total_size, 0);
    Some(bytes)
}

pub fn instantiate_pef_sections(data: &[u8]) -> Option<Vec<PefInstantiatedSection>> {
    let header = parse_pef_header(data)?;
    let sections = parse_pef_sections(data)?;
    let count = usize::from(header.instantiated_section_count);
    if count > sections.len() {
        return None;
    }

    let mut instantiated = Vec::with_capacity(count);
    for (index, section) in sections.iter().take(count).enumerate() {
        instantiated.push(PefInstantiatedSection {
            index,
            header: *section,
            bytes: instantiate_pef_section(data, section)?,
        });
    }
    Some(instantiated)
}

pub fn resolve_pef_main_entry(data: &[u8]) -> Option<PefEntryPoint> {
    let loader = parse_pef_loader_header(data)?;
    if loader.main_section < 0 {
        return None;
    }

    let sections = parse_pef_sections(data)?;
    let main_section = usize::try_from(loader.main_section).ok()?;
    let section = sections.get(main_section)?;
    let bytes = instantiate_pef_section(data, section)?;
    let offset = usize::try_from(loader.main_offset).ok()?;
    let end = offset.checked_add(8)?;
    if end > bytes.len() {
        return None;
    }

    Some(PefEntryPoint {
        entry_pc: read_u32(&bytes, offset)?,
        rtoc: read_u32(&bytes, offset + 4)?,
        main_section: loader.main_section,
        main_offset: loader.main_offset,
    })
}

/// Unpack one pattern-initialized data section's initialized bytes.
pub fn unpack_pef_pattern_data(packed: &[u8], unpacked_size: u32) -> Option<Vec<u8>> {
    let target = usize::try_from(unpacked_size).ok()?;
    let mut out = Vec::with_capacity(target);
    let mut pc = 0usize;

    while pc < packed.len() && out.len() < target {
        let instr = *packed.get(pc)?;
        pc += 1;
        let opcode = (instr >> 5) & 0x07;
        let inline = instr & 0x1f;

        match opcode {
            0 => {
                let count = usize::try_from(read_count(inline, packed, &mut pc)?).ok()?;
                let new_len = out.len().checked_add(count)?;
                if new_len > target {
                    return None;
                }
                out.resize(new_len, 0);
            }
            1 => {
                let block_size = usize::try_from(read_count(inline, packed, &mut pc)?).ok()?;
                let end = pc.checked_add(block_size)?;
                if end > packed.len() || out.len().checked_add(block_size)? > target {
                    return None;
                }
                out.extend_from_slice(&packed[pc..end]);
                pc = end;
            }
            2 => {
                let block_size = usize::try_from(read_count(inline, packed, &mut pc)?).ok()?;
                let repeats = usize::try_from(read_vle(packed, &mut pc)?)
                    .ok()?
                    .checked_add(1)?;
                let end = pc.checked_add(block_size)?;
                let produced = block_size.checked_mul(repeats)?;
                if end > packed.len() || out.len().checked_add(produced)? > target {
                    return None;
                }
                let block = &packed[pc..end];
                for _ in 0..repeats {
                    out.extend_from_slice(block);
                }
                pc = end;
            }
            3 => {
                let common_size = usize::try_from(read_count(inline, packed, &mut pc)?).ok()?;
                let custom_size = usize::try_from(read_vle(packed, &mut pc)?).ok()?;
                let repeat_count = usize::try_from(read_vle(packed, &mut pc)?).ok()?;
                let raw_needed = common_size.checked_add(custom_size.checked_mul(repeat_count)?)?;
                let raw_end = pc.checked_add(raw_needed)?;
                let produced = common_size
                    .checked_mul(repeat_count.checked_add(1)?)?
                    .checked_add(custom_size.checked_mul(repeat_count)?)?;
                if raw_end > packed.len() || out.len().checked_add(produced)? > target {
                    return None;
                }

                let common = packed[pc..pc + common_size].to_vec();
                let mut raw = pc + common_size;
                for _ in 0..repeat_count {
                    out.extend_from_slice(&common);
                    let custom_end = raw.checked_add(custom_size)?;
                    out.extend_from_slice(&packed[raw..custom_end]);
                    raw = custom_end;
                }
                out.extend_from_slice(&common);
                pc = raw_end;
            }
            4 => {
                let common_size = usize::try_from(read_count(inline, packed, &mut pc)?).ok()?;
                let custom_size = usize::try_from(read_vle(packed, &mut pc)?).ok()?;
                let repeat_count = usize::try_from(read_vle(packed, &mut pc)?).ok()?;
                let raw_needed = custom_size.checked_mul(repeat_count)?;
                let raw_end = pc.checked_add(raw_needed)?;
                let produced = common_size
                    .checked_mul(repeat_count.checked_add(1)?)?
                    .checked_add(custom_size.checked_mul(repeat_count)?)?;
                if raw_end > packed.len() || out.len().checked_add(produced)? > target {
                    return None;
                }

                let mut raw = pc;
                for _ in 0..repeat_count {
                    let new_len = out.len().checked_add(common_size)?;
                    out.resize(new_len, 0);
                    let custom_end = raw.checked_add(custom_size)?;
                    out.extend_from_slice(&packed[raw..custom_end]);
                    raw = custom_end;
                }
                let new_len = out.len().checked_add(common_size)?;
                out.resize(new_len, 0);
                pc = raw_end;
            }
            _ => return None,
        }
    }

    (out.len() == target).then_some(out)
}

fn loader_section(sections: &[PefSection]) -> Option<&PefSection> {
    sections
        .iter()
        .find(|section| section.section_kind == SECTION_KIND_LOADER)
}

fn loader_section_end(data: &[u8], loader: &PefSection) -> Option<usize> {
    let start = usize::try_from(loader.container_offset).ok()?;
    let packed_size = usize::try_from(loader.packed_size).ok()?;
    let end = if packed_size == 0 {
        data.len()
    } else {
        start.checked_add(packed_size)?
    };
    (end <= data.len()).then_some(end)
}

fn read_count(inline: u8, packed: &[u8], pc: &mut usize) -> Option<u32> {
    if inline != 0 {
        Some(u32::from(inline))
    } else {
        read_vle(packed, pc)
    }
}

fn read_vle(packed: &[u8], pc: &mut usize) -> Option<u32> {
    let mut result = 0u64;
    loop {
        let byte = *packed.get(*pc)?;
        *pc += 1;
        result = (result << 7) | u64::from(byte & 0x7f);
        if result > u64::from(u32::MAX) {
            return None;
        }
        if (byte & 0x80) == 0 {
            return Some(result as u32);
        }
    }
}

fn import_addr(ctx: &PefRelocContext<'_>, index: u32) -> Result<u32, PefRelocApplyError> {
    ctx.import_addrs
        .get(index as usize)
        .copied()
        .ok_or(PefRelocApplyError::ImportIndexOutOfRange {
            index,
            import_count: ctx.import_addrs.len() as u32,
        })
}

fn reloc_failure(
    error: PefRelocApplyError,
    reloc_offset: usize,
    fallback_position: u32,
    import_index: Option<u32>,
) -> PefRelocFailure {
    PefRelocFailure {
        error,
        reloc_offset: u32::try_from(reloc_offset).unwrap_or(u32::MAX),
        section_position: error.section_position().unwrap_or(fallback_position),
        import_index: import_index.or_else(|| error.import_index()),
    }
}

impl PefRelocApplyError {
    fn section_position(self) -> Option<u32> {
        match self {
            PefRelocApplyError::OutOfRange { position, .. } => Some(position),
            _ => None,
        }
    }

    fn import_index(self) -> Option<u32> {
        match self {
            PefRelocApplyError::ImportIndexOutOfRange { index, .. } => Some(index),
            _ => None,
        }
    }
}

fn section_base(ctx: &PefRelocContext<'_>, index: u32) -> Result<u32, PefRelocApplyError> {
    match ctx.section_bases.get(index as usize) {
        Some(Some(base)) => Ok(*base),
        Some(None) => Err(PefRelocApplyError::SectionBaseUnavailable { index }),
        None => Err(PefRelocApplyError::SectionIndexOutOfRange {
            index,
            section_count: ctx.section_bases.len() as u32,
        }),
    }
}

fn checked_position_add(
    position: u32,
    delta: u32,
    section_len: u32,
) -> Result<u32, PefRelocApplyError> {
    let next = position
        .checked_add(delta)
        .ok_or(PefRelocApplyError::OutOfRange {
            position,
            section_len,
        })?;
    if next > section_len {
        return Err(PefRelocApplyError::OutOfRange {
            position: next,
            section_len,
        });
    }
    Ok(next)
}

fn read_slot_u32(bytes: &[u8], position: u32, section_len: u32) -> Result<u32, PefRelocApplyError> {
    if position
        .checked_add(4)
        .map(|end| end > section_len)
        .unwrap_or(true)
    {
        return Err(PefRelocApplyError::OutOfRange {
            position,
            section_len,
        });
    }
    let offset = position as usize;
    Ok(u32::from_be_bytes([
        bytes[offset],
        bytes[offset + 1],
        bytes[offset + 2],
        bytes[offset + 3],
    ]))
}

fn write_slot_u32(
    bytes: &mut [u8],
    position: u32,
    value: u32,
    section_len: u32,
) -> Result<(), PefRelocApplyError> {
    if position
        .checked_add(4)
        .map(|end| end > section_len)
        .unwrap_or(true)
    {
        return Err(PefRelocApplyError::OutOfRange {
            position,
            section_len,
        });
    }
    let offset = position as usize;
    bytes[offset..offset + 4].copy_from_slice(&value.to_be_bytes());
    Ok(())
}

fn read_chunk(bytes: &[u8], offset: usize) -> Option<u16> {
    Some(u16::from_be_bytes(
        bytes.get(offset..offset + 2)?.try_into().ok()?,
    ))
}

fn read_u16(data: &[u8], offset: usize) -> Option<u16> {
    Some(u16::from_be_bytes(
        data.get(offset..offset + 2)?.try_into().ok()?,
    ))
}

fn read_i32(data: &[u8], offset: usize) -> Option<i32> {
    Some(i32::from_be_bytes(
        data.get(offset..offset + 4)?.try_into().ok()?,
    ))
}

fn read_u32(data: &[u8], offset: usize) -> Option<u32> {
    Some(u32::from_be_bytes(
        data.get(offset..offset + 4)?.try_into().ok()?,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_pef_sections_and_loader_header() {
        let pef = synthetic_pef(
            synthetic_loader(1, 8, b"InterfaceLib", &[b"Gestalt", b"GetSharedLibrary"]),
            block_copy_section(b"0123456789abcdef"),
            16,
            24,
        );

        let header = parse_pef_header(&pef).unwrap();
        assert_eq!(header.architecture, *b"pwpc");
        assert_eq!(header.format_version, 1);
        assert_eq!(header.section_count, 3);
        assert_eq!(header.instantiated_section_count, 2);

        let sections = parse_pef_sections(&pef).unwrap();
        assert_eq!(sections.len(), 3);
        assert_eq!(sections[0].kind_name(), "code");
        assert_eq!(sections[1].kind_name(), "pattern_data");
        assert_eq!(sections[2].kind_name(), "loader");
        assert_eq!(sections[1].unpacked_size, 16);
        assert_eq!(sections[1].total_size, 24);

        let loader = parse_pef_loader_header(&pef).unwrap();
        assert_eq!(loader.main_section, 1);
        assert_eq!(loader.main_offset, 8);
        assert_eq!(loader.imported_library_count, 1);
        assert_eq!(loader.total_imported_symbol_count, 2);
    }

    #[test]
    fn unpack_pef_pattern_data_supports_core_opcodes() {
        let packed = [
            0x03, // zero(3)
            0x23, b'A', b'B', b'C', // blockCopy(3)
            0x42, 0x02, b'X', b'Y', // repeatedBlock(2, 3 repeats)
            0x61, 0x01, 0x02, b'C', b'1', b'2', 0x81, 0x01, 0x02, b'a', b'b',
        ];

        let unpacked = unpack_pef_pattern_data(&packed, 22).unwrap();
        assert_eq!(unpacked, b"\0\0\0ABCXYXYXYC1C2C\0a\0b\0".to_vec());
    }

    #[test]
    fn instantiate_pef_section_zero_fills_pattern_data_to_total_size() {
        let pef = synthetic_pef(
            synthetic_loader(1, 0, b"InterfaceLib", &[b"Gestalt"]),
            block_copy_section(b"ABCD"),
            4,
            8,
        );
        let section = parse_pef_sections(&pef).unwrap()[1];

        let bytes = instantiate_pef_section(&pef, &section).unwrap();
        assert_eq!(bytes, b"ABCD\0\0\0\0".to_vec());

        let instantiated = instantiate_pef_sections(&pef).unwrap();
        assert_eq!(instantiated.len(), 2);
        assert_eq!(instantiated[0].header.kind_name(), "code");
        assert_eq!(instantiated[1].bytes, bytes);
    }

    #[test]
    fn resolve_pef_main_entry_reads_pattern_data_tvector() {
        let mut initialized = [0u8; 16];
        initialized[8..12].copy_from_slice(&0x1234_5678u32.to_be_bytes());
        initialized[12..16].copy_from_slice(&0x9abc_def0u32.to_be_bytes());
        let pef = synthetic_pef(
            synthetic_loader(1, 8, b"InterfaceLib", &[b"Gestalt"]),
            block_copy_section(&initialized),
            initialized.len() as u32,
            24,
        );

        let entry = resolve_pef_main_entry(&pef).unwrap();
        assert_eq!(
            entry,
            PefEntryPoint {
                entry_pc: 0x1234_5678,
                rtoc: 0x9abc_def0,
                main_section: 1,
                main_offset: 8,
            }
        );
    }

    #[test]
    fn loader_strings_decode_mac_roman_trademark() {
        let pef = synthetic_pef(
            synthetic_loader(1, 0, b"QuickDraw\xAA 3D", &[b"Q3Initialize"]),
            block_copy_section(b"\0\0\0\0\0\0\0\0"),
            8,
            8,
        );
        let library = parse_pef_imported_libraries(&pef).unwrap()[0];

        assert_eq!(
            pef_loader_string_at(&pef, library.name_offset).unwrap(),
            "QuickDraw\u{2122} 3D"
        );
    }

    #[test]
    fn parse_imported_libraries_and_symbols() {
        let pef = synthetic_pef(
            synthetic_loader(1, 0, b"InterfaceLib", &[b"Gestalt", b"GetSharedLibrary"]),
            block_copy_section(b"\0\0\0\0\0\0\0\0"),
            8,
            8,
        );

        let libraries = parse_pef_imported_libraries(&pef).unwrap();
        assert_eq!(libraries.len(), 1);
        assert_eq!(libraries[0].imported_symbol_count, 2);
        assert_eq!(libraries[0].first_imported_symbol, 0);

        let symbols = parse_pef_imported_symbols(&pef).unwrap();
        assert_eq!(symbols.len(), 2);
        assert_eq!(symbols[0].class_name(), "tvector");
        assert!(!symbols[0].weak);
        assert_eq!(symbols[1].class_name(), "tvector");
        assert!(symbols[1].weak);

        let imports = resolve_pef_imports(&pef).unwrap();
        assert_eq!(imports.len(), 2);
        assert_eq!(imports[0].library_name, "InterfaceLib");
        assert_eq!(imports[0].symbol_name, "Gestalt");
        assert_eq!(imports[0].class, 2);
        assert_eq!(imports[1].symbol_name, "GetSharedLibrary");
        assert!(imports[1].weak);
    }

    #[test]
    fn parse_pef_reloc_headers_and_chunk_stream() {
        let chunks = [ddat(1, 2), run_reloc(0x21, 1), sm_index_reloc(0x30, 0)];
        let pef = synthetic_pef(
            synthetic_loader_with_reloc_header(&chunks),
            block_copy_section(b"\0\0\0\0\0\0\0\0"),
            8,
            8,
        );

        let loader = parse_pef_loader_header(&pef).unwrap();
        assert_eq!(loader.reloc_section_count, 1);
        assert!(loader.reloc_instr_offset > 0);

        let headers = parse_pef_reloc_headers(&pef).unwrap();
        assert_eq!(
            headers,
            vec![PefRelocHeader {
                section_index: 1,
                reloc_count: 3,
                first_reloc_offset: 0,
            }]
        );

        assert_eq!(
            pef_reloc_chunk_stream(&pef, &headers[0]).unwrap(),
            reloc_stream(&chunks)
        );
    }

    #[test]
    fn decode_pef_reloc_supports_first_seven_opcode_family() {
        assert_eq!(
            decode_pef_reloc(ddat(2, 3), None).unwrap(),
            (
                PefRelocOp::DDat {
                    skip_count: 2,
                    reloc_count: 3,
                },
                1
            )
        );
        assert_eq!(
            decode_pef_reloc(run_reloc(0x21, 4), None).unwrap(),
            (PefRelocOp::BySectD { run_length: 4 }, 1)
        );
        assert_eq!(
            decode_pef_reloc(run_reloc(0x20, 2), None).unwrap(),
            (PefRelocOp::BySectC { run_length: 2 }, 1)
        );
        assert_eq!(
            decode_pef_reloc(run_reloc(0x23, 1), None).unwrap(),
            (PefRelocOp::TVector8 { run_length: 1 }, 1)
        );
        assert_eq!(
            decode_pef_reloc(run_reloc(0x22, 2), None).unwrap(),
            (PefRelocOp::TVector12 { run_length: 2 }, 1)
        );
        assert_eq!(
            decode_pef_reloc(run_reloc(0x24, 3), None).unwrap(),
            (PefRelocOp::VTable8 { run_length: 3 }, 1)
        );
        assert_eq!(
            decode_pef_reloc(sm_index_reloc(0x30, 17), None).unwrap(),
            (PefRelocOp::SmByImport { index: 17 }, 1)
        );
        assert_eq!(
            decode_pef_reloc(sm_index_reloc(0x31, 3), None).unwrap(),
            (PefRelocOp::SmSetSectC { index: 3 }, 1)
        );
        assert_eq!(
            decode_pef_reloc(sm_index_reloc(0x32, 4), None).unwrap(),
            (PefRelocOp::SmSetSectD { index: 4 }, 1)
        );
        assert_eq!(
            decode_pef_reloc(sm_index_reloc(0x33, 5), None).unwrap(),
            (PefRelocOp::SmBySection { index: 5 }, 1)
        );
        assert_eq!(
            decode_pef_reloc(delt(12), None).unwrap(),
            (PefRelocOp::IncrPosition { offset: 12 }, 1)
        );
        assert_eq!(
            decode_pef_reloc(rpt(2, 3), None).unwrap(),
            (
                PefRelocOp::SmRepeat {
                    chunk_count: 2,
                    repeat_count: 3,
                },
                1
            )
        );
        let (first, second) = lg_by_import(0x0002_3456);
        assert_eq!(
            decode_pef_reloc(first, Some(second)).unwrap(),
            (PefRelocOp::LgByImport { index: 0x0002_3456 }, 2)
        );
        let (first, second) = lg_repeat(3, 0x0003_4567);
        assert_eq!(
            decode_pef_reloc(first, Some(second)).unwrap(),
            (
                PefRelocOp::LgRepeat {
                    chunk_count: 3,
                    repeat_count: 0x0003_4567,
                },
                2
            )
        );
        let (first, second) = lg_section_reloc(0, 0x0003_4567);
        assert_eq!(
            decode_pef_reloc(first, Some(second)).unwrap(),
            (PefRelocOp::LgBySection { index: 0x0003_4567 }, 2)
        );
        let (first, second) = lg_section_reloc(1, 0x0002_3456);
        assert_eq!(
            decode_pef_reloc(first, Some(second)).unwrap(),
            (PefRelocOp::LgSetSectC { index: 0x0002_3456 }, 2)
        );
        let (first, second) = lg_section_reloc(2, 0x0001_2345);
        assert_eq!(
            decode_pef_reloc(first, Some(second)).unwrap(),
            (PefRelocOp::LgSetSectD { index: 0x0001_2345 }, 2)
        );
    }

    #[test]
    fn apply_pef_relocations_handles_first_seven_opcode_family() {
        let chunks = [
            ddat(1, 1),
            run_reloc(0x20, 1),
            run_reloc(0x21, 1),
            run_reloc(0x23, 1),
            sm_index_reloc(0x30, 1),
            delt(4),
            run_reloc(0x21, 1),
            rpt(1, 2),
        ];
        let mut section = word_section(16);
        let ctx = PefRelocContext {
            code_base: 0x1000,
            data_base: 0x2000,
            section_bases: &[],
            import_addrs: &[0xaaaa_0000, 0xbbbb_0000],
        };

        apply_pef_relocations(&mut section, &reloc_stream(&chunks), &ctx).unwrap();
        let words = read_words(&section);

        assert_eq!(words[0], 1);
        assert_eq!(words[1], 2 + 0x2000);
        assert_eq!(words[2], 3 + 0x1000);
        assert_eq!(words[3], 4 + 0x2000);
        assert_eq!(words[4], 5 + 0x1000);
        assert_eq!(words[5], 6 + 0x2000);
        assert_eq!(words[6], 0xbbbb_0000);
        assert_eq!(words[7], 8);
        assert_eq!(words[8], 9 + 0x2000);
        assert_eq!(words[9], 10 + 0x2000);
        assert_eq!(words[10], 11 + 0x2000);
    }

    #[test]
    fn apply_pef_relocations_reports_import_index_out_of_range() {
        let mut section = word_section(2);
        let ctx = PefRelocContext {
            code_base: 0x1000,
            data_base: 0x2000,
            section_bases: &[],
            import_addrs: &[0xaaaa_0000],
        };

        let err = apply_pef_relocations(
            &mut section,
            &reloc_stream(&[sm_index_reloc(0x30, 2)]),
            &ctx,
        )
        .unwrap_err();

        assert_eq!(
            err,
            PefRelocApplyError::ImportIndexOutOfRange {
                index: 2,
                import_count: 1,
            }
        );
    }

    #[test]
    fn apply_pef_relocations_detailed_reports_failure_context() {
        let mut section = word_section(2);
        let ctx = PefRelocContext {
            code_base: 0x1000,
            data_base: 0x2000,
            section_bases: &[],
            import_addrs: &[0xaaaa_0000],
        };

        let failure = apply_pef_relocations_detailed(
            &mut section,
            &reloc_stream(&[delt(4), sm_index_reloc(0x30, 2)]),
            &ctx,
        )
        .unwrap_err();

        assert_eq!(
            failure,
            PefRelocFailure {
                error: PefRelocApplyError::ImportIndexOutOfRange {
                    index: 2,
                    import_count: 1,
                },
                reloc_offset: 2,
                section_position: 4,
                import_index: Some(2),
            }
        );
    }

    #[test]
    fn apply_pef_relocations_handles_long_import_and_repeat_forms() {
        let (lg_import_first, lg_import_second) = lg_by_import(1);
        let (lg_repeat_first, lg_repeat_second) = lg_repeat(1, 2);
        let chunks = [
            lg_import_first,
            lg_import_second,
            run_reloc(0x25, 1),
            run_reloc(0x20, 1),
            lg_repeat_first,
            lg_repeat_second,
        ];
        let mut section = word_section(8);
        let ctx = PefRelocContext {
            code_base: 0x1000,
            data_base: 0x2000,
            section_bases: &[],
            import_addrs: &[0xaaaa_0000, 0xbbbb_0000, 0xcccc_0000],
        };

        apply_pef_relocations(&mut section, &reloc_stream(&chunks), &ctx).unwrap();
        let words = read_words(&section);

        assert_eq!(words[0], 0xbbbb_0000);
        assert_eq!(words[1], 0xcccc_0000);
        assert_eq!(words[2], 3 + 0x1000);
        assert_eq!(words[3], 4 + 0x1000);
        assert_eq!(words[4], 5 + 0x1000);
        assert_eq!(words[5], 6);
    }

    #[test]
    fn apply_pef_relocations_counts_repeat_blocks_not_decoded_instructions() {
        let (lg_import_first, lg_import_second) = lg_by_import(1);
        let chunks = [lg_import_first, lg_import_second, rpt(2, 2)];
        let mut section = word_section(4);
        let ctx = PefRelocContext {
            code_base: 0x1000,
            data_base: 0x2000,
            section_bases: &[],
            import_addrs: &[0xaaaa_0000, 0xbbbb_0000],
        };

        apply_pef_relocations(&mut section, &reloc_stream(&chunks), &ctx).unwrap();
        let words = read_words(&section);

        assert_eq!(words[0], 0xbbbb_0000);
        assert_eq!(words[1], 0xbbbb_0000);
        assert_eq!(words[2], 0xbbbb_0000);
        assert_eq!(words[3], 4);
    }

    #[test]
    fn apply_pef_relocations_rejects_repeat_into_long_instruction_tail() {
        let (lg_import_first, lg_import_second) = lg_by_import(1);
        let chunks = [lg_import_first, lg_import_second, rpt(1, 1)];
        let mut section = word_section(2);
        let ctx = PefRelocContext {
            code_base: 0x1000,
            data_base: 0x2000,
            section_bases: &[],
            import_addrs: &[0xaaaa_0000, 0xbbbb_0000],
        };

        let error = apply_pef_relocations(&mut section, &reloc_stream(&chunks), &ctx)
            .expect_err("repeat target inside a two-block instruction must be rejected");

        assert_eq!(
            error,
            PefRelocApplyError::SmRepeatUnderflow {
                chunk_count: 1,
                history_len: 2,
            }
        );
    }

    #[test]
    fn apply_pef_relocations_handles_tvector12_and_vtable8_forms() {
        let chunks = [run_reloc(0x22, 1), run_reloc(0x24, 1)];
        let mut section = word_section(5);
        let ctx = PefRelocContext {
            code_base: 0x1000,
            data_base: 0x2000,
            section_bases: &[],
            import_addrs: &[],
        };

        apply_pef_relocations(&mut section, &reloc_stream(&chunks), &ctx).unwrap();
        let words = read_words(&section);

        assert_eq!(words[0], 1 + 0x1000);
        assert_eq!(words[1], 2 + 0x2000);
        assert_eq!(words[2], 3);
        assert_eq!(words[3], 4 + 0x2000);
        assert_eq!(words[4], 5);
    }

    #[test]
    fn apply_pef_relocations_handles_section_index_forms() {
        let (lg_by_section_first, lg_by_section_second) = lg_section_reloc(0, 1);
        let (lg_set_c_first, lg_set_c_second) = lg_section_reloc(1, 1);
        let (lg_set_d_first, lg_set_d_second) = lg_section_reloc(2, 2);
        let chunks = [
            sm_index_reloc(0x33, 3),
            sm_index_reloc(0x31, 2),
            run_reloc(0x20, 1),
            sm_index_reloc(0x32, 3),
            run_reloc(0x21, 1),
            lg_by_section_first,
            lg_by_section_second,
            lg_set_c_first,
            lg_set_c_second,
            run_reloc(0x20, 1),
            lg_set_d_first,
            lg_set_d_second,
            run_reloc(0x21, 1),
        ];
        let section_bases = [Some(0x1000), Some(0x2000), Some(0x3000), Some(0x4000)];
        let mut section = word_section(7);
        let ctx = PefRelocContext {
            code_base: 0x1111,
            data_base: 0x2222,
            section_bases: &section_bases,
            import_addrs: &[],
        };

        apply_pef_relocations(&mut section, &reloc_stream(&chunks), &ctx).unwrap();
        let words = read_words(&section);

        assert_eq!(words[0], 0x4000);
        assert_eq!(words[1], 2 + 0x3000);
        assert_eq!(words[2], 3 + 0x4000);
        assert_eq!(words[3], 0x2000);
        assert_eq!(words[4], 5 + 0x2000);
        assert_eq!(words[5], 6 + 0x3000);
        assert_eq!(words[6], 7);
    }

    #[test]
    fn apply_pef_relocations_reports_section_index_out_of_range() {
        let section_bases = [Some(0x1000)];
        let mut section = word_section(2);
        let ctx = PefRelocContext {
            code_base: 0x1000,
            data_base: 0x2000,
            section_bases: &section_bases,
            import_addrs: &[],
        };

        let err = apply_pef_relocations(
            &mut section,
            &reloc_stream(&[sm_index_reloc(0x31, 2)]),
            &ctx,
        )
        .unwrap_err();

        assert_eq!(
            err,
            PefRelocApplyError::SectionIndexOutOfRange {
                index: 2,
                section_count: 1,
            }
        );
    }

    #[test]
    fn apply_pef_relocations_reports_unavailable_section_base() {
        let section_bases = [Some(0x1000), None];
        let mut section = word_section(2);
        let ctx = PefRelocContext {
            code_base: 0x1000,
            data_base: 0x2000,
            section_bases: &section_bases,
            import_addrs: &[],
        };

        let err = apply_pef_relocations(
            &mut section,
            &reloc_stream(&[sm_index_reloc(0x33, 1)]),
            &ctx,
        )
        .unwrap_err();

        assert_eq!(err, PefRelocApplyError::SectionBaseUnavailable { index: 1 });
    }

    fn synthetic_pef(
        loader: Vec<u8>,
        pattern_section: Vec<u8>,
        pattern_unpacked_size: u32,
        pattern_total_size: u32,
    ) -> Vec<u8> {
        let loader_offset = 0x80usize;
        let code_offset = 0x100usize;
        let code = [0u8; 8];
        let pattern_offset = code_offset + code.len();
        let total_len = pattern_offset
            .checked_add(pattern_section.len())
            .unwrap()
            .max(loader_offset + loader.len());
        let mut bytes = vec![0u8; total_len];

        bytes[0..4].copy_from_slice(b"Joy!");
        bytes[4..8].copy_from_slice(b"peff");
        bytes[8..12].copy_from_slice(b"pwpc");
        write_u32(&mut bytes, 12, 1);
        write_u16(&mut bytes, 32, 3);
        write_u16(&mut bytes, 34, 2);

        write_section(
            &mut bytes,
            0,
            SectionSpec {
                name_offset: -1,
                default_address: 0,
                total_size: code.len() as u32,
                unpacked_size: code.len() as u32,
                packed_size: code.len() as u32,
                container_offset: code_offset as u32,
                section_kind: SECTION_KIND_CODE,
                share_kind: 4,
                alignment: 4,
            },
        );
        write_section(
            &mut bytes,
            1,
            SectionSpec {
                name_offset: -1,
                default_address: 0,
                total_size: pattern_total_size,
                unpacked_size: pattern_unpacked_size,
                packed_size: pattern_section.len() as u32,
                container_offset: pattern_offset as u32,
                section_kind: SECTION_KIND_PATTERN_DATA,
                share_kind: 1,
                alignment: 4,
            },
        );
        write_section(
            &mut bytes,
            2,
            SectionSpec {
                name_offset: -1,
                default_address: 0,
                total_size: 0,
                unpacked_size: 0,
                packed_size: loader.len() as u32,
                container_offset: loader_offset as u32,
                section_kind: SECTION_KIND_LOADER,
                share_kind: 4,
                alignment: 4,
            },
        );

        bytes[loader_offset..loader_offset + loader.len()].copy_from_slice(&loader);
        bytes[code_offset..code_offset + code.len()].copy_from_slice(&code);
        bytes[pattern_offset..pattern_offset + pattern_section.len()]
            .copy_from_slice(&pattern_section);
        bytes
    }

    fn synthetic_loader_with_reloc_header(chunks: &[u16]) -> Vec<u8> {
        let mut strings = Vec::new();
        let library_name_offset = push_c_string(&mut strings, b"InterfaceLib");
        let symbol_name_offset = push_c_string(&mut strings, b"Gestalt");

        let reloc_header_offset =
            PEF_LOADER_HEADER_SIZE + PEF_IMPORTED_LIBRARY_SIZE + PEF_IMPORTED_SYMBOL_SIZE;
        let reloc_instr_offset = reloc_header_offset + PEF_RELOCATION_HEADER_SIZE;
        let strings_offset = reloc_instr_offset + chunks.len() * 2;
        let mut bytes = vec![0u8; strings_offset + strings.len()];

        write_i32(&mut bytes, 0, 1);
        write_i32(&mut bytes, 8, -1);
        write_i32(&mut bytes, 16, -1);
        write_u32(&mut bytes, 24, 1);
        write_u32(&mut bytes, 28, 1);
        write_u32(&mut bytes, 32, 1);
        write_u32(&mut bytes, 36, reloc_instr_offset as u32);
        write_u32(&mut bytes, 40, strings_offset as u32);

        let library = PEF_LOADER_HEADER_SIZE;
        write_u32(&mut bytes, library, library_name_offset);
        write_u32(&mut bytes, library + 12, 1);
        write_u32(&mut bytes, library + 16, 0);

        let symbol = PEF_LOADER_HEADER_SIZE + PEF_IMPORTED_LIBRARY_SIZE;
        write_symbol(&mut bytes, symbol, 0x02, symbol_name_offset);

        write_u16(&mut bytes, reloc_header_offset, 1);
        write_u32(&mut bytes, reloc_header_offset + 4, chunks.len() as u32);
        write_u32(&mut bytes, reloc_header_offset + 8, 0);

        for (index, chunk) in chunks.iter().enumerate() {
            write_u16(&mut bytes, reloc_instr_offset + index * 2, *chunk);
        }

        bytes[strings_offset..].copy_from_slice(&strings);
        bytes
    }

    fn synthetic_loader(
        main_section: i32,
        main_offset: u32,
        library_name: &[u8],
        symbol_names: &[&[u8]],
    ) -> Vec<u8> {
        let mut strings = Vec::new();
        let library_name_offset = push_c_string(&mut strings, library_name);
        let symbol_name_offsets = symbol_names
            .iter()
            .map(|name| push_c_string(&mut strings, name))
            .collect::<Vec<_>>();

        let strings_offset =
            PEF_LOADER_HEADER_SIZE + PEF_IMPORTED_LIBRARY_SIZE + symbol_names.len() * 4;
        let mut bytes = vec![0u8; strings_offset + strings.len()];

        write_i32(&mut bytes, 0, main_section);
        write_u32(&mut bytes, 4, main_offset);
        write_i32(&mut bytes, 8, -1);
        write_i32(&mut bytes, 16, -1);
        write_u32(&mut bytes, 24, 1);
        write_u32(&mut bytes, 28, symbol_names.len() as u32);
        write_u32(&mut bytes, 40, strings_offset as u32);

        let library = PEF_LOADER_HEADER_SIZE;
        write_u32(&mut bytes, library, library_name_offset);
        write_u32(&mut bytes, library + 12, symbol_names.len() as u32);
        write_u32(&mut bytes, library + 16, 0);

        let symbols = PEF_LOADER_HEADER_SIZE + PEF_IMPORTED_LIBRARY_SIZE;
        for (index, name_offset) in symbol_name_offsets.iter().enumerate() {
            let class_byte = if index == 1 { 0x82 } else { 0x02 };
            write_symbol(&mut bytes, symbols + index * 4, class_byte, *name_offset);
        }

        bytes[strings_offset..].copy_from_slice(&strings);
        bytes
    }

    fn block_copy_section(bytes: &[u8]) -> Vec<u8> {
        assert!(bytes.len() < 32);
        let mut packed = vec![0x20 | bytes.len() as u8];
        packed.extend_from_slice(bytes);
        packed
    }

    fn word_section(words: usize) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(words * 4);
        for word in 1..=words as u32 {
            bytes.extend_from_slice(&word.to_be_bytes());
        }
        bytes
    }

    fn read_words(bytes: &[u8]) -> Vec<u32> {
        bytes
            .chunks_exact(4)
            .map(|chunk| u32::from_be_bytes(chunk.try_into().unwrap()))
            .collect()
    }

    fn reloc_stream(chunks: &[u16]) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(chunks.len() * 2);
        for chunk in chunks {
            bytes.extend_from_slice(&chunk.to_be_bytes());
        }
        bytes
    }

    fn ddat(skip_count: u8, reloc_count: u8) -> u16 {
        (u16::from(skip_count) << 6) | u16::from(reloc_count)
    }

    fn run_reloc(dispatch: u8, run_length: u16) -> u16 {
        (u16::from(dispatch) << 9) | ((run_length - 1) & 0x01ff)
    }

    fn sm_index_reloc(dispatch: u8, index: u16) -> u16 {
        (u16::from(dispatch) << 9) | (index & 0x01ff)
    }

    fn delt(offset: u16) -> u16 {
        (0x40u16 << 9) | ((offset - 1) & 0x0fff)
    }

    fn rpt(chunk_count: u8, repeat_count: u16) -> u16 {
        (0x48u16 << 9) | ((u16::from(chunk_count - 1) & 0x0f) << 8) | ((repeat_count - 1) & 0x00ff)
    }

    fn lg_by_import(index: u32) -> (u16, u16) {
        (
            (0x52u16 << 9) | (((index >> 16) as u16) & 0x03ff),
            index as u16,
        )
    }

    fn lg_repeat(chunk_count: u8, repeat_count: u32) -> (u16, u16) {
        (
            (0x58u16 << 9)
                | ((u16::from(chunk_count - 1) & 0x0f) << 6)
                | (((repeat_count >> 16) as u16) & 0x003f),
            repeat_count as u16,
        )
    }

    fn lg_section_reloc(subopcode: u8, index: u32) -> (u16, u16) {
        (
            (0x5au16 << 9)
                | ((u16::from(subopcode) & 0x0f) << 6)
                | (((index >> 16) as u16) & 0x003f),
            index as u16,
        )
    }

    struct SectionSpec {
        name_offset: i32,
        default_address: u32,
        total_size: u32,
        unpacked_size: u32,
        packed_size: u32,
        container_offset: u32,
        section_kind: u8,
        share_kind: u8,
        alignment: u8,
    }

    fn write_section(bytes: &mut [u8], index: usize, spec: SectionSpec) {
        let off = PEF_HEADER_SIZE + index * PEF_SECTION_HEADER_SIZE;
        write_i32(bytes, off, spec.name_offset);
        write_u32(bytes, off + 4, spec.default_address);
        write_u32(bytes, off + 8, spec.total_size);
        write_u32(bytes, off + 12, spec.unpacked_size);
        write_u32(bytes, off + 16, spec.packed_size);
        write_u32(bytes, off + 20, spec.container_offset);
        bytes[off + 24] = spec.section_kind;
        bytes[off + 25] = spec.share_kind;
        bytes[off + 26] = spec.alignment;
    }

    fn push_c_string(strings: &mut Vec<u8>, value: &[u8]) -> u32 {
        let offset = strings.len() as u32;
        strings.extend_from_slice(value);
        strings.push(0);
        offset
    }

    fn write_symbol(bytes: &mut [u8], offset: usize, class_byte: u8, name_offset: u32) {
        bytes[offset] = class_byte;
        bytes[offset + 1] = ((name_offset >> 16) & 0xff) as u8;
        bytes[offset + 2] = ((name_offset >> 8) & 0xff) as u8;
        bytes[offset + 3] = (name_offset & 0xff) as u8;
    }

    fn write_i32(bytes: &mut [u8], offset: usize, value: i32) {
        bytes[offset..offset + 4].copy_from_slice(&value.to_be_bytes());
    }

    fn write_u32(bytes: &mut [u8], offset: usize, value: u32) {
        bytes[offset..offset + 4].copy_from_slice(&value.to_be_bytes());
    }

    fn write_u16(bytes: &mut [u8], offset: usize, value: u16) {
        bytes[offset..offset + 2].copy_from_slice(&value.to_be_bytes());
    }
}
