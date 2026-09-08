//! Optional host-side PEF loader diagnostics.
//!
//! This module formats immutable loader facts and writes them only when the
//! existing `SYSTEMLESS_PEF_DUMP` environment control selects an output path.

use super::super::pef::{PefHeader, PefLoaderHeader, PefRelocHeader, PefSection};
use super::imports::PpcImportBinding;
use crate::cfm::fragment::CfmSection as MappedSection;
use std::sync::OnceLock;

static PEF_DUMP_PATH: OnceLock<Option<std::path::PathBuf>> = OnceLock::new();

fn pef_dump_path() -> Option<&'static std::path::Path> {
    PEF_DUMP_PATH
        .get_or_init(|| {
            let value = std::env::var_os("SYSTEMLESS_PEF_DUMP")?;
            let path = std::path::PathBuf::from(value);
            (!path.as_os_str().is_empty()).then_some(path)
        })
        .as_deref()
}

fn write_pef_dump(path: &std::path::Path, report: &str) {
    if let Some(parent) = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    {
        if let Err(error) = std::fs::create_dir_all(parent) {
            eprintln!(
                "[PEF-DUMP] failed to create parent {}: {}",
                parent.display(),
                error
            );
            return;
        }
    }
    if let Err(error) = std::fs::write(path, report) {
        eprintln!("[PEF-DUMP] failed to write {}: {}", path.display(), error);
    }
}

pub(super) fn maybe_write(ctx: &PefDumpContext<'_>) {
    let Some(path) = pef_dump_path() else {
        return;
    };
    let report = format_pef_dump_json(ctx);
    write_pef_dump(path, &report);
}

pub(super) struct PefDumpContext<'a> {
    pub(super) data_len: usize,
    pub(super) header: PefHeader,
    pub(super) loader: PefLoaderHeader,
    pub(super) raw_sections: &'a [PefSection],
    pub(super) mapped_sections: &'a [MappedSection],
    pub(super) imports: &'a [PpcImportBinding],
    pub(super) reloc_headers: &'a [PefRelocHeader],
    pub(super) entry_pc: u32,
    pub(super) rtoc: u32,
    pub(super) stack_base: u32,
    pub(super) stack_size: u32,
    pub(super) stack_top: u32,
}

pub(super) fn format_pef_dump_json(ctx: &PefDumpContext<'_>) -> String {
    use std::fmt::Write as _;

    let mut out = String::new();
    let _ = writeln!(out, "{{");
    let _ = writeln!(out, "  \"format\": \"systemless_pef_dump_v1\",");
    let _ = writeln!(out, "  \"data_len\": {},", ctx.data_len);
    let _ = writeln!(out, "  \"header\": {{");
    let _ = writeln!(
        out,
        "    \"architecture\": \"{}\",",
        json_escape(&String::from_utf8_lossy(&ctx.header.architecture))
    );
    let _ = writeln!(
        out,
        "    \"format_version\": {},",
        ctx.header.format_version
    );
    let _ = writeln!(out, "    \"section_count\": {},", ctx.header.section_count);
    let _ = writeln!(
        out,
        "    \"instantiated_section_count\": {}",
        ctx.header.instantiated_section_count
    );
    let _ = writeln!(out, "  }},");
    let _ = writeln!(out, "  \"loader\": {{");
    let _ = writeln!(out, "    \"main_section\": {},", ctx.loader.main_section);
    let _ = writeln!(out, "    \"main_offset\": {},", ctx.loader.main_offset);
    let _ = writeln!(out, "    \"init_section\": {},", ctx.loader.init_section);
    let _ = writeln!(out, "    \"init_offset\": {},", ctx.loader.init_offset);
    let _ = writeln!(out, "    \"term_section\": {},", ctx.loader.term_section);
    let _ = writeln!(out, "    \"term_offset\": {},", ctx.loader.term_offset);
    let _ = writeln!(
        out,
        "    \"imported_library_count\": {},",
        ctx.loader.imported_library_count
    );
    let _ = writeln!(
        out,
        "    \"total_imported_symbol_count\": {},",
        ctx.loader.total_imported_symbol_count
    );
    let _ = writeln!(
        out,
        "    \"reloc_section_count\": {},",
        ctx.loader.reloc_section_count
    );
    let _ = writeln!(
        out,
        "    \"reloc_instr_offset\": {},",
        ctx.loader.reloc_instr_offset
    );
    let _ = writeln!(
        out,
        "    \"loader_strings_offset\": {}",
        ctx.loader.loader_strings_offset
    );
    let _ = writeln!(out, "  }},");
    let _ = writeln!(out, "  \"sections\": [");
    for (index, section) in ctx.raw_sections.iter().enumerate() {
        let mapped = ctx
            .mapped_sections
            .iter()
            .find(|mapped| mapped.index == index);
        let comma = if index + 1 == ctx.raw_sections.len() {
            ""
        } else {
            ","
        };
        let _ = writeln!(out, "    {{");
        let _ = writeln!(out, "      \"index\": {},", index);
        let _ = writeln!(
            out,
            "      \"kind\": \"{}\",",
            json_escape(section.kind_name())
        );
        let _ = writeln!(out, "      \"kind_id\": {},", section.section_kind);
        let _ = writeln!(
            out,
            "      \"default_address\": \"{}\",",
            hex32(section.default_address)
        );
        let _ = writeln!(out, "      \"total_size\": {},", section.total_size);
        let _ = writeln!(out, "      \"unpacked_size\": {},", section.unpacked_size);
        let _ = writeln!(out, "      \"packed_size\": {},", section.packed_size);
        let _ = writeln!(
            out,
            "      \"container_offset\": {},",
            section.container_offset
        );
        let _ = writeln!(out, "      \"alignment\": {},", section.alignment);
        match mapped {
            Some(mapped) => {
                let _ = writeln!(out, "      \"mapped_base\": \"{}\",", hex32(mapped.base));
                let _ = writeln!(out, "      \"mapped_size\": {}", mapped.bytes.len());
            }
            None => {
                let _ = writeln!(out, "      \"mapped_base\": null,");
                let _ = writeln!(out, "      \"mapped_size\": 0");
            }
        }
        let _ = writeln!(out, "    }}{}", comma);
    }
    let _ = writeln!(out, "  ],");
    let _ = writeln!(out, "  \"imports\": [");
    for (index, import) in ctx.imports.iter().enumerate() {
        let comma = if index + 1 == ctx.imports.len() {
            ""
        } else {
            ","
        };
        let _ = writeln!(out, "    {{");
        let _ = writeln!(out, "      \"symbol_index\": {},", import.symbol_index);
        let _ = writeln!(out, "      \"library_index\": {},", import.library_index);
        let _ = writeln!(
            out,
            "      \"library\": \"{}\",",
            json_escape(&import.library_name)
        );
        let _ = writeln!(
            out,
            "      \"symbol\": \"{}\",",
            json_escape(&import.symbol_name)
        );
        let _ = writeln!(out, "      \"class\": {},", import.class);
        let _ = writeln!(
            out,
            "      \"class_name\": \"{}\",",
            pef_import_class_name(import.class)
        );
        let _ = writeln!(out, "      \"weak\": {},", import.weak);
        let _ = writeln!(out, "      \"trap_pc\": \"{}\",", hex32(import.trap_pc));
        match import.tvector_address {
            Some(address) => {
                let _ = writeln!(out, "      \"tvector_address\": \"{}\",", hex32(address));
            }
            None => {
                let _ = writeln!(out, "      \"tvector_address\": null,");
            }
        }
        let _ = writeln!(
            out,
            "      \"dispatcher_target\": \"{}\"",
            json_escape(&format!("{:?}", import.dispatcher_target))
        );
        let _ = writeln!(out, "    }}{}", comma);
    }
    let _ = writeln!(out, "  ],");
    let _ = writeln!(out, "  \"relocations\": [");
    for (index, reloc) in ctx.reloc_headers.iter().enumerate() {
        let comma = if index + 1 == ctx.reloc_headers.len() {
            ""
        } else {
            ","
        };
        let _ = writeln!(out, "    {{");
        let _ = writeln!(out, "      \"section_index\": {},", reloc.section_index);
        let _ = writeln!(out, "      \"reloc_count\": {},", reloc.reloc_count);
        let _ = writeln!(
            out,
            "      \"first_reloc_offset\": {}",
            reloc.first_reloc_offset
        );
        let _ = writeln!(out, "    }}{}", comma);
    }
    let _ = writeln!(out, "  ],");
    let _ = writeln!(out, "  \"entry\": {{");
    let _ = writeln!(out, "    \"entry_pc\": \"{}\",", hex32(ctx.entry_pc));
    let _ = writeln!(out, "    \"rtoc\": \"{}\"", hex32(ctx.rtoc));
    let _ = writeln!(out, "  }},");
    let _ = writeln!(out, "  \"stack\": {{");
    let _ = writeln!(out, "    \"base\": \"{}\",", hex32(ctx.stack_base));
    let _ = writeln!(out, "    \"top\": \"{}\",", hex32(ctx.stack_top));
    let _ = writeln!(out, "    \"size\": {}", ctx.stack_size);
    let _ = writeln!(out, "  }}");
    let _ = writeln!(out, "}}");
    out
}

fn hex32(value: u32) -> String {
    format!("0x{:08X}", value)
}

fn pef_import_class_name(class: u8) -> &'static str {
    match class {
        0 => "code",
        1 => "data",
        2 => "tvector",
        3 => "toc",
        4 => "glue",
        _ => "reserved",
    }
}

fn json_escape(value: &str) -> String {
    let mut out = String::new();
    for ch in value.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            ch if ch.is_control() => {
                use std::fmt::Write as _;
                let _ = write!(out, "\\u{:04X}", ch as u32);
            }
            ch => out.push(ch),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::json_escape;

    #[test]
    fn pef_dump_json_escapes_strings() {
        assert_eq!(json_escape("quote\" slash\\\n"), "quote\\\" slash\\\\\\n");
    }
}
