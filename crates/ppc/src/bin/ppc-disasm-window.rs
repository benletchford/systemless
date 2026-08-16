use std::env;
use std::fs;
use std::path::Path;

use ppc::decode;

fn main() {
    let mut args = env::args().skip(1);
    let Some(path) = args.next() else {
        eprintln!("Usage: ppc-disasm-window [--base <code-base-hex>] <pef-file> <pc-hex> <count>");
        std::process::exit(2);
    };

    let (code_base, path, pc_s, count_s) = if path == "--base" {
        let Some(base_s) = args.next() else {
            die("missing --base value");
        };
        let Some(path) = args.next() else {
            die("missing pef file path");
        };
        let Some(pc_s) = args.next() else {
            die("missing pc value");
        };
        let Some(count_s) = args.next() else {
            die("missing count value");
        };
        if args.next().is_some() {
            die("too many arguments");
        }
        (
            parse_hex_u32(&base_s).unwrap_or_else(|err| die(&err)),
            path,
            pc_s,
            count_s,
        )
    } else {
        let Some(pc_s) = args.next() else {
            eprintln!(
                "Usage: ppc-disasm-window [--base <code-base-hex>] <pef-file> <pc-hex> <count>"
            );
            std::process::exit(2);
        };
        let Some(count_s) = args.next() else {
            eprintln!(
                "Usage: ppc-disasm-window [--base <code-base-hex>] <pef-file> <pc-hex> <count>"
            );
            std::process::exit(2);
        };
        if args.next().is_some() {
            eprintln!(
                "Usage: ppc-disasm-window [--base <code-base-hex>] <pef-file> <pc-hex> <count>"
            );
            std::process::exit(2);
        }
        (0x0100_0000u32, path, pc_s, count_s)
    };

    let pc = parse_hex_u32(&pc_s).unwrap_or_else(|err| die(&err));
    let count = count_s
        .parse::<usize>()
        .unwrap_or_else(|err| die(&format!("invalid count {count_s:?}: {err}")));

    let path = Path::new(&path);
    let bytes = fs::read(path)
        .unwrap_or_else(|err| die(&format!("failed to read {}: {err}", path.display())));
    let code_section = find_first_code_section(&bytes).unwrap_or_else(|err| die(&err));
    if pc < code_base {
        die(&format!(
            "pc ${pc:08X} is below the assumed code base ${code_base:08X}"
        ));
    }
    let file_offset = code_section
        .checked_add((pc - code_base) as usize)
        .unwrap_or_else(|| die("file offset overflow"));
    let needed = count
        .checked_mul(4)
        .and_then(|n| file_offset.checked_add(n))
        .unwrap_or_else(|| die("requested window overflows"));
    if needed > bytes.len() {
        die(&format!(
            "window runs past file: file_off=${file_offset:08X} count={} file_len=${:08X}",
            count,
            bytes.len()
        ));
    }

    for i in 0..count {
        let cur_pc = pc.wrapping_add((i as u32) * 4);
        let off = file_offset + i * 4;
        let word = u32::from_be_bytes(bytes[off..off + 4].try_into().unwrap());
        match decode(word) {
            Ok(instr) => {
                println!("{cur_pc:08X}: {word:08X}  {instr:?}");
            }
            Err(err) => {
                println!("{cur_pc:08X}: {word:08X}  {err:?}");
            }
        }
    }
}

fn find_first_code_section(bytes: &[u8]) -> Result<usize, String> {
    if bytes.len() < 40 {
        return Err(format!("PEF header is truncated: {} bytes", bytes.len()));
    }
    if &bytes[0..4] != b"Joy!" || &bytes[4..8] != b"peff" {
        return Err("missing PEF magic Joy!/peff".to_string());
    }
    let section_count = u16::from_be_bytes([bytes[32], bytes[33]]) as usize;
    let table_start = 40usize;
    let table_len = section_count
        .checked_mul(28)
        .ok_or_else(|| "section table length overflow".to_string())?;
    if table_start + table_len > bytes.len() {
        return Err("section table runs past file".to_string());
    }

    for index in 0..section_count {
        let off = table_start + index * 28;
        let container_offset =
            u32::from_be_bytes(bytes[off + 20..off + 24].try_into().unwrap()) as usize;
        let kind = bytes[off + 24];
        if kind == 0 {
            return Ok(container_offset);
        }
    }

    Err("no code section found".to_string())
}

fn parse_hex_u32(value: &str) -> Result<u32, String> {
    let trimmed = value.trim();
    let hex = trimmed
        .strip_prefix("0x")
        .or_else(|| trimmed.strip_prefix("0X"))
        .or_else(|| trimmed.strip_prefix('$'))
        .unwrap_or(trimmed);
    u32::from_str_radix(hex, 16).map_err(|err| format!("invalid hex value {value:?}: {err}"))
}

fn die(message: &str) -> ! {
    eprintln!("{message}");
    std::process::exit(1);
}
