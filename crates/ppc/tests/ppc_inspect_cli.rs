use std::fs;
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

#[test]
fn ppc_inspect_reports_pef_sections_imports_and_code_histogram() {
    let exe = env!("CARGO_BIN_EXE_ppc-inspect");
    let path = std::env::temp_dir().join(format!(
        "ppc-inspect-test-{}-{}.pef",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    fs::write(&path, synthetic_pef()).unwrap();

    let output = Command::new(exe)
        .arg("--no-path")
        .arg(&path)
        .output()
        .unwrap();
    let _ = fs::remove_file(&path);
    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8(output.stdout).unwrap();

    assert!(!stdout.contains("\"path\""), "{stdout}");
    assert!(stdout.contains("\"architecture\": \"pwpc\""), "{stdout}");
    assert!(stdout.contains("\"section_count\": 3"), "{stdout}");
    assert!(stdout.contains("\"kind_name\": \"code\""), "{stdout}");
    assert!(
        stdout.contains("\"kind_name\": \"pattern_data\""),
        "{stdout}"
    );
    assert!(stdout.contains("\"kind_name\": \"loader\""), "{stdout}");
    assert!(stdout.contains("\"main_section\": 1"), "{stdout}");
    assert!(stdout.contains("\"main_offset\": 16"), "{stdout}");
    assert!(stdout.contains("\"imported_library_count\": 1"), "{stdout}");
    assert!(
        stdout.contains("\"total_imported_symbol_count\": 2"),
        "{stdout}"
    );
    assert!(stdout.contains("\"name\": \"InterfaceLib\""), "{stdout}");
    assert!(stdout.contains("\"name\": \"Gestalt\""), "{stdout}");
    assert!(
        stdout.contains("\"name\": \"GetSharedLibrary\""),
        "{stdout}"
    );
    assert!(stdout.contains("\"class_name\": \"tvector\""), "{stdout}");
    assert!(stdout.contains("\"total_words\": 3"), "{stdout}");
    assert!(stdout.contains("\"decoded_words\": 2"), "{stdout}");
    assert!(stdout.contains("\"unsupported_words\": 1"), "{stdout}");
    assert!(stdout.contains("\"unsupported_primary\""), "{stdout}");
    assert!(stdout.contains("\"0\": 1"), "{stdout}");
    assert!(stdout.contains("\"14\": 1"), "{stdout}");
    assert!(stdout.contains("\"19\": 1"), "{stdout}");
    assert!(stdout.contains("\"16\": 1"), "{stdout}");
}

#[test]
fn pef_inventory_fixtures_capture_known_surface() {
    let default_app = include_str!("fixtures/pef/default.json");
    let low_memory_app = include_str!("fixtures/pef/low-memory.json");

    assert_pef_fixture(
        default_app,
        8120,
        43065,
        2155,
        "\"unsupported_primary\": {\n          \"0\": 2069,\n          \"2\": 2,\n          \"4\": 1,\n          \"9\": 1,\n          \"30\": 45\n        }",
    );
    assert_pef_fixture(
        low_memory_app,
        8104,
        43460,
        279,
        "\"unsupported_primary\": {\n          \"0\": 273,\n          \"2\": 2,\n          \"9\": 1,\n          \"30\": 2\n        }",
    );
}

fn assert_pef_fixture(
    json: &str,
    main_offset: u32,
    decoded_words: u32,
    unsupported_words: u32,
    unsupported_primary: &str,
) {
    assert!(!json.contains("\"path\""), "{json}");
    assert!(json.contains("\"tag1\": \"Joy!\""), "{json}");
    assert!(json.contains("\"tag2\": \"peff\""), "{json}");
    assert!(json.contains("\"architecture\": \"pwpc\""), "{json}");
    assert!(json.contains("\"format_version\": 1"), "{json}");
    assert!(json.contains("\"section_count\": 3"), "{json}");
    assert!(json.contains("\"kind_name\": \"code\""), "{json}");
    assert!(json.contains("\"kind_name\": \"pattern_data\""), "{json}");
    assert!(json.contains("\"kind_name\": \"loader\""), "{json}");
    assert!(json.contains("\"decode\": {"), "{json}");
    assert!(
        json.contains(&format!("\"decoded_words\": {decoded_words}")),
        "{json}"
    );
    assert!(
        json.contains(&format!("\"unsupported_words\": {unsupported_words}")),
        "{json}"
    );
    assert!(json.contains(unsupported_primary), "{json}");
    assert!(
        !json.contains("{ \"primary\": 59, \"secondary\": 24"),
        "fres should decode in the static inventory: {json}"
    );
    assert!(
        !json.contains("{ \"primary\": 63, \"secondary\": 26"),
        "frsqrte should decode in the static inventory: {json}"
    );
    assert!(
        json.contains(&format!("\"main_offset\": {main_offset}")),
        "{json}"
    );
    assert!(json.contains("\"imported_library_count\": 7"), "{json}");
    assert!(
        json.contains("\"total_imported_symbol_count\": 283"),
        "{json}"
    );

    for library in [
        "MathLib",
        "InterfaceLib",
        "QuickDraw",
        "3D Accelerator",
        "DrawSprocketLib",
        "QuickTimeLib",
        "InputSprocketLib",
    ] {
        assert!(json.contains(library), "missing {library} in {json}");
    }
}

fn synthetic_pef() -> Vec<u8> {
    let loader_offset = 0x80usize;
    let code_offset = 0x100usize;
    let data_offset = 0x110usize;
    let loader = synthetic_loader();
    let total_len = data_offset + 4;
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
            total_size: 12,
            unpacked_size: 12,
            packed_size: 12,
            container_offset: code_offset as u32,
            kind: 0,
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
            total_size: 32,
            unpacked_size: 4,
            packed_size: 4,
            container_offset: data_offset as u32,
            kind: 2,
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
            kind: 4,
            share_kind: 4,
            alignment: 4,
        },
    );

    bytes[loader_offset..loader_offset + loader.len()].copy_from_slice(&loader);
    write_u32(&mut bytes, code_offset, 14u32 << 26); // addi r0, r0, 0
    write_u32(&mut bytes, code_offset + 4, 0x4e80_0020); // blr, op19/xo16
    write_u32(&mut bytes, code_offset + 8, 0); // unsupported primary opcode 0
    bytes
}

fn synthetic_loader() -> Vec<u8> {
    let mut strings = Vec::new();
    let interface_lib = push_c_string(&mut strings, b"InterfaceLib");
    let gestalt = push_c_string(&mut strings, b"Gestalt");
    let get_shared_library = push_c_string(&mut strings, b"GetSharedLibrary");

    let strings_offset = 56 + 24 + 8;
    let mut bytes = vec![0u8; strings_offset + strings.len()];
    write_i32(&mut bytes, 0, 1);
    write_u32(&mut bytes, 4, 0x10);
    write_i32(&mut bytes, 8, -1);
    write_i32(&mut bytes, 16, -1);
    write_u32(&mut bytes, 24, 1);
    write_u32(&mut bytes, 28, 2);
    write_u32(&mut bytes, 32, 1);
    write_u32(&mut bytes, 36, 0x40);
    write_u32(&mut bytes, 40, strings_offset as u32);

    let lib = 56;
    write_u32(&mut bytes, lib, interface_lib);
    write_u32(&mut bytes, lib + 12, 2);
    write_u32(&mut bytes, lib + 16, 0);

    let symbols = 56 + 24;
    write_symbol(&mut bytes, symbols, 2, gestalt);
    write_symbol(&mut bytes, symbols + 4, 2, get_shared_library);

    bytes[strings_offset..].copy_from_slice(&strings);
    bytes
}

fn push_c_string(strings: &mut Vec<u8>, value: &[u8]) -> u32 {
    let offset = strings.len() as u32;
    strings.extend_from_slice(value);
    strings.push(0);
    offset
}

struct SectionSpec {
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

fn write_section(bytes: &mut [u8], index: usize, spec: SectionSpec) {
    let off = 40 + index * 28;
    write_i32(bytes, off, spec.name_offset);
    write_u32(bytes, off + 4, spec.default_address);
    write_u32(bytes, off + 8, spec.total_size);
    write_u32(bytes, off + 12, spec.unpacked_size);
    write_u32(bytes, off + 16, spec.packed_size);
    write_u32(bytes, off + 20, spec.container_offset);
    bytes[off + 24] = spec.kind;
    bytes[off + 25] = spec.share_kind;
    bytes[off + 26] = spec.alignment;
}

fn write_symbol(bytes: &mut [u8], off: usize, class: u8, name_offset: u32) {
    bytes[off] = class;
    bytes[off + 1] = ((name_offset >> 16) & 0xff) as u8;
    bytes[off + 2] = ((name_offset >> 8) & 0xff) as u8;
    bytes[off + 3] = (name_offset & 0xff) as u8;
}

fn write_i32(bytes: &mut [u8], off: usize, value: i32) {
    bytes[off..off + 4].copy_from_slice(&value.to_be_bytes());
}

fn write_u32(bytes: &mut [u8], off: usize, value: u32) {
    bytes[off..off + 4].copy_from_slice(&value.to_be_bytes());
}

fn write_u16(bytes: &mut [u8], off: usize, value: u16) {
    bytes[off..off + 2].copy_from_slice(&value.to_be_bytes());
}
