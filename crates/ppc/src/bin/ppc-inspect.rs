use std::collections::BTreeMap;
use std::env;
use std::fmt::Write as _;
use std::fs;
use std::path::Path;

use ppc::{decode, PpcDecodeError};

#[derive(Debug)]
struct PefContainer<'a> {
    bytes: &'a [u8],
    architecture: String,
    format_version: u32,
    section_count: u16,
    instantiated_section_count: u16,
    sections: Vec<PefSection>,
    loader: Option<PefLoader>,
}

#[derive(Debug, Clone)]
struct PefSection {
    name_offset: i32,
    default_address: u32,
    total_size: u32,
    unpacked_size: u32,
    packed_size: u32,
    container_offset: u32,
    kind: u8,
    share_kind: u8,
    alignment: u8,
}

#[derive(Debug)]
struct PefLoader {
    main_section: i32,
    main_offset: u32,
    init_section: i32,
    init_offset: u32,
    term_section: i32,
    term_offset: u32,
    imported_library_count: u32,
    total_imported_symbol_count: u32,
    reloc_section_count: u32,
    reloc_instr_offset: u32,
    loader_strings_offset: u32,
    export_hash_offset: u32,
    export_hash_table_power: u32,
    exported_symbol_count: u32,
    imported_libraries: Vec<PefImportedLibrary>,
}

#[derive(Debug)]
struct PefImportedLibrary {
    name: String,
    old_imp_version: u32,
    current_version: u32,
    imported_symbol_count: u32,
    first_imported_symbol: u32,
    options: u8,
    symbols: Vec<PefImportedSymbol>,
}

#[derive(Debug)]
struct PefImportedSymbol {
    index: u32,
    class: u8,
    weak: bool,
    name: String,
}

#[derive(Debug)]
struct CodeHistogram {
    total_words: u32,
    decoded_words: u32,
    unsupported_primary: BTreeMap<u8, u32>,
    unsupported_secondary: BTreeMap<SecondaryOpcode, u32>,
    primary: BTreeMap<u8, u32>,
    op19_secondary: BTreeMap<u16, u32>,
    op31_secondary: BTreeMap<u16, u32>,
    op59_secondary: BTreeMap<u16, u32>,
    op63_secondary: BTreeMap<u16, u32>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
struct SecondaryOpcode {
    primary: u8,
    secondary: u16,
}

fn main() {
    let mut args = env::args().skip(1);
    let mut include_path = true;
    let first = args.next();
    let path = match first.as_deref() {
        Some("--no-path") => {
            include_path = false;
            args.next()
        }
        _ => first,
    };
    let Some(path) = path else {
        eprintln!("Usage: ppc-inspect [--no-path] <pef-file>");
        std::process::exit(2);
    };
    if args.next().is_some() {
        eprintln!("Usage: ppc-inspect [--no-path] <pef-file>");
        std::process::exit(2);
    }

    let path = Path::new(&path);
    let bytes = match fs::read(path) {
        Ok(bytes) => bytes,
        Err(err) => {
            eprintln!("failed to read {}: {err}", path.display());
            std::process::exit(1);
        }
    };
    let pef = match PefContainer::parse(&bytes) {
        Ok(pef) => pef,
        Err(err) => {
            eprintln!("failed to parse {}: {err}", path.display());
            std::process::exit(1);
        }
    };
    print!(
        "{}",
        pef.to_json(if include_path { Some(path) } else { None })
    );
}

impl<'a> PefContainer<'a> {
    fn parse(bytes: &'a [u8]) -> Result<Self, String> {
        if bytes.len() < 40 {
            return Err(format!("PEF header is truncated: {} bytes", bytes.len()));
        }
        if &bytes[0..4] != b"Joy!" || &bytes[4..8] != b"peff" {
            return Err("missing PEF magic Joy!/peff".to_string());
        }

        let architecture = ostype(read_u32(bytes, 8)?);
        let format_version = read_u32(bytes, 12)?;
        let section_count = read_u16(bytes, 32)?;
        let instantiated_section_count = read_u16(bytes, 34)?;
        let section_table_len = usize::from(section_count)
            .checked_mul(28)
            .ok_or_else(|| "section table length overflow".to_string())?;
        checked_range(bytes, 40, section_table_len, "section table")?;

        let mut sections = Vec::with_capacity(usize::from(section_count));
        for index in 0..section_count {
            let off = 40 + usize::from(index) * 28;
            sections.push(PefSection {
                name_offset: read_i32(bytes, off)?,
                default_address: read_u32(bytes, off + 4)?,
                total_size: read_u32(bytes, off + 8)?,
                unpacked_size: read_u32(bytes, off + 12)?,
                packed_size: read_u32(bytes, off + 16)?,
                container_offset: read_u32(bytes, off + 20)?,
                kind: read_u8(bytes, off + 24)?,
                share_kind: read_u8(bytes, off + 25)?,
                alignment: read_u8(bytes, off + 26)?,
            });
        }

        let loader = sections
            .iter()
            .enumerate()
            .find(|(_, section)| section.kind == 4)
            .map(|(index, section)| PefLoader::parse(bytes, index, section))
            .transpose()?;

        Ok(Self {
            bytes,
            architecture,
            format_version,
            section_count,
            instantiated_section_count,
            sections,
            loader,
        })
    }

    fn to_json(&self, path: Option<&Path>) -> String {
        let mut out = String::new();
        writeln!(out, "{{").unwrap();
        if let Some(path) = path {
            writeln!(
                out,
                "  \"path\": {},",
                json_string(&path.display().to_string())
            )
            .unwrap();
        }
        writeln!(out, "  \"size\": {},", self.bytes.len()).unwrap();
        writeln!(out, "  \"tag1\": \"Joy!\",").unwrap();
        writeln!(out, "  \"tag2\": \"peff\",").unwrap();
        writeln!(
            out,
            "  \"architecture\": {},",
            json_string(&self.architecture)
        )
        .unwrap();
        writeln!(out, "  \"format_version\": {},", self.format_version).unwrap();
        writeln!(out, "  \"section_count\": {},", self.section_count).unwrap();
        writeln!(
            out,
            "  \"instantiated_section_count\": {},",
            self.instantiated_section_count
        )
        .unwrap();
        writeln!(out, "  \"sections\": [").unwrap();
        for (index, section) in self.sections.iter().enumerate() {
            let comma = if index + 1 == self.sections.len() {
                ""
            } else {
                ","
            };
            writeln!(out, "    {{").unwrap();
            writeln!(out, "      \"index\": {},", index).unwrap();
            writeln!(out, "      \"name_offset\": {},", section.name_offset).unwrap();
            writeln!(
                out,
                "      \"default_address\": {},",
                section.default_address
            )
            .unwrap();
            writeln!(out, "      \"total_size\": {},", section.total_size).unwrap();
            writeln!(out, "      \"unpacked_size\": {},", section.unpacked_size).unwrap();
            writeln!(out, "      \"packed_size\": {},", section.packed_size).unwrap();
            writeln!(
                out,
                "      \"container_offset\": {},",
                section.container_offset
            )
            .unwrap();
            writeln!(out, "      \"kind\": {},", section.kind).unwrap();
            writeln!(
                out,
                "      \"kind_name\": {},",
                json_string(section_kind_name(section.kind))
            )
            .unwrap();
            writeln!(out, "      \"share_kind\": {},", section.share_kind).unwrap();
            writeln!(out, "      \"alignment\": {}", section.alignment).unwrap();
            writeln!(out, "    }}{comma}").unwrap();
        }
        writeln!(out, "  ],").unwrap();

        if let Some(loader) = &self.loader {
            loader.write_json(&mut out, &self.sections, self.bytes);
            writeln!(out).unwrap();
        } else {
            writeln!(out, "  \"loader\": null").unwrap();
        }
        writeln!(out, "}}").unwrap();
        out
    }
}

impl PefLoader {
    fn parse(bytes: &[u8], section_index: usize, section: &PefSection) -> Result<Self, String> {
        let base = usize::try_from(section.container_offset)
            .map_err(|_| "loader container offset does not fit usize".to_string())?;
        checked_range(bytes, base, 56, "loader header")?;
        let imported_library_count = read_u32(bytes, base + 24)?;
        let total_imported_symbol_count = read_u32(bytes, base + 28)?;
        let loader_strings_offset = read_u32(bytes, base + 40)?;
        let imported_library_count_usize = usize::try_from(imported_library_count)
            .map_err(|_| "imported library count does not fit usize".to_string())?;
        let total_imported_symbol_count_usize = usize::try_from(total_imported_symbol_count)
            .map_err(|_| "imported symbol count does not fit usize".to_string())?;
        let library_table_len = imported_library_count_usize
            .checked_mul(24)
            .ok_or_else(|| "imported library table length overflow".to_string())?;
        let library_table = checked_range(
            bytes,
            base + 56,
            library_table_len,
            "imported library table",
        )?;
        let symbol_table_off = library_table.end;
        let symbol_table_len = total_imported_symbol_count_usize
            .checked_mul(4)
            .ok_or_else(|| "imported symbol table length overflow".to_string())?;
        checked_range(
            bytes,
            symbol_table_off,
            symbol_table_len,
            "imported symbol table",
        )?;

        let strings_base = base
            .checked_add(
                usize::try_from(loader_strings_offset)
                    .map_err(|_| "loader strings offset does not fit usize".to_string())?,
            )
            .ok_or_else(|| "loader strings offset overflow".to_string())?;
        if strings_base > bytes.len() {
            return Err(format!(
                "loader strings base for section {section_index} is outside file: 0x{strings_base:x}"
            ));
        }

        let mut imported_libraries = Vec::with_capacity(imported_library_count_usize);
        for lib_index in 0..imported_library_count_usize {
            let off = base + 56 + lib_index * 24;
            let name_offset = read_u32(bytes, off)?;
            let old_imp_version = read_u32(bytes, off + 4)?;
            let current_version = read_u32(bytes, off + 8)?;
            let imported_symbol_count = read_u32(bytes, off + 12)?;
            let first_imported_symbol = read_u32(bytes, off + 16)?;
            let options = read_u8(bytes, off + 20)?;
            let name = read_loader_string(bytes, strings_base, name_offset)?;

            let start = usize::try_from(first_imported_symbol)
                .map_err(|_| format!("first import index for library {name} does not fit usize"))?;
            let count = usize::try_from(imported_symbol_count)
                .map_err(|_| format!("symbol count for library {name} does not fit usize"))?;
            let end = start
                .checked_add(count)
                .ok_or_else(|| format!("symbol range overflow for library {name}"))?;
            if end > total_imported_symbol_count_usize {
                return Err(format!(
                    "symbol range for library {name} exceeds imported symbol table: {start}..{end}"
                ));
            }

            let mut symbols = Vec::with_capacity(count);
            for symbol_index in start..end {
                let symbol_off = symbol_table_off + symbol_index * 4;
                let class_and_flags = read_u8(bytes, symbol_off)?;
                let name_offset = read_u24(bytes, symbol_off + 1)?;
                symbols.push(PefImportedSymbol {
                    index: symbol_index as u32,
                    class: class_and_flags & 0x0f,
                    weak: (class_and_flags & 0x80) != 0,
                    name: read_loader_string(bytes, strings_base, name_offset)?,
                });
            }

            imported_libraries.push(PefImportedLibrary {
                name,
                old_imp_version,
                current_version,
                imported_symbol_count,
                first_imported_symbol,
                options,
                symbols,
            });
        }

        Ok(Self {
            main_section: read_i32(bytes, base)?,
            main_offset: read_u32(bytes, base + 4)?,
            init_section: read_i32(bytes, base + 8)?,
            init_offset: read_u32(bytes, base + 12)?,
            term_section: read_i32(bytes, base + 16)?,
            term_offset: read_u32(bytes, base + 20)?,
            imported_library_count,
            total_imported_symbol_count,
            reloc_section_count: read_u32(bytes, base + 32)?,
            reloc_instr_offset: read_u32(bytes, base + 36)?,
            loader_strings_offset,
            export_hash_offset: read_u32(bytes, base + 44)?,
            export_hash_table_power: read_u32(bytes, base + 48)?,
            exported_symbol_count: read_u32(bytes, base + 52)?,
            imported_libraries,
        })
    }

    fn write_json(&self, out: &mut String, sections: &[PefSection], bytes: &[u8]) {
        writeln!(out, "  \"loader\": {{").unwrap();
        writeln!(out, "    \"main_section\": {},", self.main_section).unwrap();
        writeln!(out, "    \"main_offset\": {},", self.main_offset).unwrap();
        writeln!(out, "    \"init_section\": {},", self.init_section).unwrap();
        writeln!(out, "    \"init_offset\": {},", self.init_offset).unwrap();
        writeln!(out, "    \"term_section\": {},", self.term_section).unwrap();
        writeln!(out, "    \"term_offset\": {},", self.term_offset).unwrap();
        writeln!(
            out,
            "    \"imported_library_count\": {},",
            self.imported_library_count
        )
        .unwrap();
        writeln!(
            out,
            "    \"total_imported_symbol_count\": {},",
            self.total_imported_symbol_count
        )
        .unwrap();
        writeln!(
            out,
            "    \"reloc_section_count\": {},",
            self.reloc_section_count
        )
        .unwrap();
        writeln!(
            out,
            "    \"reloc_instr_offset\": {},",
            self.reloc_instr_offset
        )
        .unwrap();
        writeln!(
            out,
            "    \"loader_strings_offset\": {},",
            self.loader_strings_offset
        )
        .unwrap();
        writeln!(
            out,
            "    \"export_hash_offset\": {},",
            self.export_hash_offset
        )
        .unwrap();
        writeln!(
            out,
            "    \"export_hash_table_power\": {},",
            self.export_hash_table_power
        )
        .unwrap();
        writeln!(
            out,
            "    \"exported_symbol_count\": {},",
            self.exported_symbol_count
        )
        .unwrap();
        writeln!(out, "    \"imported_libraries\": [").unwrap();
        for (index, library) in self.imported_libraries.iter().enumerate() {
            library.write_json(out, index + 1 == self.imported_libraries.len());
        }
        writeln!(out, "    ]").unwrap();
        writeln!(out, "  }},").unwrap();
        writeln!(out, "  \"code_histograms\": [").unwrap();
        let code_indices = sections
            .iter()
            .enumerate()
            .filter(|(_, section)| section.kind == 0)
            .map(|(index, _)| index)
            .collect::<Vec<_>>();
        for (pos, section_index) in code_indices.iter().copied().enumerate() {
            let section = &sections[section_index];
            let histogram = CodeHistogram::from_section(bytes, section);
            let comma = if pos + 1 == code_indices.len() {
                ""
            } else {
                ","
            };
            histogram.write_json(out, section_index, comma);
        }
        writeln!(out, "  ]").unwrap();
    }
}

impl PefImportedLibrary {
    fn write_json(&self, out: &mut String, last: bool) {
        let comma = if last { "" } else { "," };
        writeln!(out, "      {{").unwrap();
        writeln!(out, "        \"name\": {},", json_string(&self.name)).unwrap();
        writeln!(
            out,
            "        \"old_imp_version\": {},",
            self.old_imp_version
        )
        .unwrap();
        writeln!(
            out,
            "        \"current_version\": {},",
            self.current_version
        )
        .unwrap();
        writeln!(
            out,
            "        \"imported_symbol_count\": {},",
            self.imported_symbol_count
        )
        .unwrap();
        writeln!(
            out,
            "        \"first_imported_symbol\": {},",
            self.first_imported_symbol
        )
        .unwrap();
        writeln!(out, "        \"options\": {},", self.options).unwrap();
        writeln!(out, "        \"symbols\": [").unwrap();
        for (index, symbol) in self.symbols.iter().enumerate() {
            symbol.write_json(out, index + 1 == self.symbols.len());
        }
        writeln!(out, "        ]").unwrap();
        writeln!(out, "      }}{comma}").unwrap();
    }
}

impl PefImportedSymbol {
    fn write_json(&self, out: &mut String, last: bool) {
        let comma = if last { "" } else { "," };
        writeln!(out, "          {{").unwrap();
        writeln!(out, "            \"index\": {},", self.index).unwrap();
        writeln!(out, "            \"class\": {},", self.class).unwrap();
        writeln!(
            out,
            "            \"class_name\": {},",
            json_string(symbol_class_name(self.class))
        )
        .unwrap();
        writeln!(out, "            \"weak\": {},", self.weak).unwrap();
        writeln!(out, "            \"name\": {}", json_string(&self.name)).unwrap();
        writeln!(out, "          }}{comma}").unwrap();
    }
}

impl CodeHistogram {
    fn from_section(bytes: &[u8], section: &PefSection) -> Self {
        let mut histogram = Self {
            total_words: 0,
            decoded_words: 0,
            unsupported_primary: BTreeMap::new(),
            unsupported_secondary: BTreeMap::new(),
            primary: BTreeMap::new(),
            op19_secondary: BTreeMap::new(),
            op31_secondary: BTreeMap::new(),
            op59_secondary: BTreeMap::new(),
            op63_secondary: BTreeMap::new(),
        };
        let Ok(start) = usize::try_from(section.container_offset) else {
            return histogram;
        };
        let Ok(size) = usize::try_from(section.packed_size) else {
            return histogram;
        };
        let Some(end) = start.checked_add(size) else {
            return histogram;
        };
        let Some(section_bytes) = bytes.get(start..end.min(bytes.len())) else {
            return histogram;
        };
        for chunk in section_bytes.chunks_exact(4) {
            let word = u32::from_be_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]);
            histogram.total_words = histogram.total_words.saturating_add(1);
            match decode(word) {
                Ok(_) => {
                    histogram.decoded_words = histogram.decoded_words.saturating_add(1);
                }
                Err(PpcDecodeError::UnsupportedPrimaryOpcode(primary)) => {
                    *histogram.unsupported_primary.entry(primary).or_insert(0) += 1;
                }
                Err(PpcDecodeError::UnsupportedSecondaryOpcode { primary, secondary }) => {
                    *histogram
                        .unsupported_secondary
                        .entry(SecondaryOpcode { primary, secondary })
                        .or_insert(0) += 1;
                }
            }

            let primary = ((word >> 26) & 0x3f) as u8;
            *histogram.primary.entry(primary).or_insert(0) += 1;
            match primary {
                19 => {
                    let xo = ((word >> 1) & 0x03ff) as u16;
                    *histogram.op19_secondary.entry(xo).or_insert(0) += 1;
                }
                31 => {
                    let xo = ((word >> 1) & 0x03ff) as u16;
                    *histogram.op31_secondary.entry(xo).or_insert(0) += 1;
                }
                59 => {
                    let xo = ((word >> 1) & 0x001f) as u16;
                    *histogram.op59_secondary.entry(xo).or_insert(0) += 1;
                }
                63 => {
                    let xo = ((word >> 1) & 0x03ff) as u16;
                    *histogram.op63_secondary.entry(xo).or_insert(0) += 1;
                }
                _ => {}
            }
        }
        histogram
    }

    fn write_json(&self, out: &mut String, section_index: usize, comma: &str) {
        writeln!(out, "    {{").unwrap();
        writeln!(out, "      \"section_index\": {},", section_index).unwrap();
        self.write_decode_json(out);
        writeln!(out, ",").unwrap();
        write_map(out, "primary", &self.primary, 6);
        writeln!(out, ",").unwrap();
        write_map(out, "op19_secondary", &self.op19_secondary, 6);
        writeln!(out, ",").unwrap();
        write_map(out, "op31_secondary", &self.op31_secondary, 6);
        writeln!(out, ",").unwrap();
        write_map(out, "op59_secondary", &self.op59_secondary, 6);
        writeln!(out, ",").unwrap();
        write_map(out, "op63_secondary", &self.op63_secondary, 6);
        writeln!(out).unwrap();
        writeln!(out, "    }}{comma}").unwrap();
    }

    fn write_decode_json(&self, out: &mut String) {
        let unsupported_words = self.total_words.saturating_sub(self.decoded_words);
        writeln!(out, "      \"decode\": {{").unwrap();
        writeln!(out, "        \"total_words\": {},", self.total_words).unwrap();
        writeln!(out, "        \"decoded_words\": {},", self.decoded_words).unwrap();
        writeln!(out, "        \"unsupported_words\": {},", unsupported_words).unwrap();
        write_map(out, "unsupported_primary", &self.unsupported_primary, 8);
        writeln!(out, ",").unwrap();
        write_secondary_map(out, "unsupported_secondary", &self.unsupported_secondary, 8);
        writeln!(out).unwrap();
        write!(out, "      }}").unwrap();
    }
}

fn checked_range(
    bytes: &[u8],
    start: usize,
    len: usize,
    label: &str,
) -> Result<std::ops::Range<usize>, String> {
    let end = start
        .checked_add(len)
        .ok_or_else(|| format!("{label} range overflows: start={start} len={len}"))?;
    if end > bytes.len() {
        return Err(format!(
            "{label} is truncated: need bytes {start}..{end}, file has {}",
            bytes.len()
        ));
    }
    Ok(start..end)
}

fn read_u8(bytes: &[u8], off: usize) -> Result<u8, String> {
    bytes
        .get(off)
        .copied()
        .ok_or_else(|| format!("read_u8 out of range at 0x{off:x}"))
}

fn read_u16(bytes: &[u8], off: usize) -> Result<u16, String> {
    let range = checked_range(bytes, off, 2, "u16")?;
    Ok(u16::from_be_bytes([
        bytes[range.start],
        bytes[range.start + 1],
    ]))
}

fn read_u24(bytes: &[u8], off: usize) -> Result<u32, String> {
    let range = checked_range(bytes, off, 3, "u24")?;
    Ok((u32::from(bytes[range.start]) << 16)
        | (u32::from(bytes[range.start + 1]) << 8)
        | u32::from(bytes[range.start + 2]))
}

fn read_u32(bytes: &[u8], off: usize) -> Result<u32, String> {
    let range = checked_range(bytes, off, 4, "u32")?;
    Ok(u32::from_be_bytes([
        bytes[range.start],
        bytes[range.start + 1],
        bytes[range.start + 2],
        bytes[range.start + 3],
    ]))
}

fn read_i32(bytes: &[u8], off: usize) -> Result<i32, String> {
    Ok(read_u32(bytes, off)? as i32)
}

fn read_loader_string(
    bytes: &[u8],
    strings_base: usize,
    name_offset: u32,
) -> Result<String, String> {
    let start = strings_base
        .checked_add(
            usize::try_from(name_offset)
                .map_err(|_| "loader string name offset does not fit usize".to_string())?,
        )
        .ok_or_else(|| "loader string offset overflow".to_string())?;
    if start >= bytes.len() {
        return Err(format!("loader string offset is outside file: 0x{start:x}"));
    }
    let rest = &bytes[start..];
    let len = rest
        .iter()
        .position(|&byte| byte == 0)
        .ok_or_else(|| format!("unterminated loader string at 0x{start:x}"))?;
    Ok(decode_mac_roman(&rest[..len]))
}

fn decode_mac_roman(bytes: &[u8]) -> String {
    bytes
        .iter()
        .map(|&byte| match byte {
            0x00..=0x7f => char::from(byte),
            0xaa => '\u{2122}',
            _ => '\u{fffd}',
        })
        .collect()
}

fn ostype(value: u32) -> String {
    decode_mac_roman(&value.to_be_bytes())
}

fn section_kind_name(kind: u8) -> &'static str {
    match kind {
        0 => "code",
        1 => "unpacked_data",
        2 => "pattern_data",
        3 => "constant",
        4 => "loader",
        5 => "debug",
        6 => "executable_data",
        7 => "exception",
        8 => "traceback",
        _ => "unknown",
    }
}

fn symbol_class_name(class: u8) -> &'static str {
    match class {
        0 => "code_address",
        1 => "data_address",
        2 => "tvector",
        3 => "toc",
        4 => "glue",
        _ => "unknown",
    }
}

fn json_string(value: &str) -> String {
    let mut out = String::with_capacity(value.len() + 2);
    out.push('"');
    for ch in value.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            ch if ch < ' ' => {
                write!(out, "\\u{:04x}", ch as u32).unwrap();
            }
            ch => out.push(ch),
        }
    }
    out.push('"');
    out
}

fn write_map<K>(out: &mut String, name: &str, map: &BTreeMap<K, u32>, indent: usize)
where
    K: std::fmt::Display + Ord,
{
    let pad = " ".repeat(indent);
    writeln!(out, "{pad}\"{name}\": {{").unwrap();
    let mut iter = map.iter().peekable();
    while let Some((key, value)) = iter.next() {
        let comma = if iter.peek().is_some() { "," } else { "" };
        writeln!(out, "{pad}  \"{key}\": {value}{comma}").unwrap();
    }
    write!(out, "{pad}}}").unwrap();
}

fn write_secondary_map(
    out: &mut String,
    name: &str,
    map: &BTreeMap<SecondaryOpcode, u32>,
    indent: usize,
) {
    let pad = " ".repeat(indent);
    writeln!(out, "{pad}\"{name}\": [").unwrap();
    let mut iter = map.iter().peekable();
    while let Some((key, value)) = iter.next() {
        let comma = if iter.peek().is_some() { "," } else { "" };
        writeln!(
            out,
            "{pad}  {{ \"primary\": {}, \"secondary\": {}, \"count\": {} }}{comma}",
            key.primary, key.secondary, value
        )
        .unwrap();
    }
    write!(out, "{pad}]").unwrap();
}
