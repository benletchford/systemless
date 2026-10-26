use super::*;

fn tec(operation: PpcTextEncodingOperation, loaded: &mut PpcLoadedApp) -> u32 {
    match dispatch_text_encoding::ppc_dispatch_text_encoding(
        operation,
        &loaded.cpu,
        &mut loaded.memory,
        &mut loaded.toolbox_startup.text_encoding,
    ) {
        PpcImportAction::Return(value) => value,
        other => panic!("unexpected {other:?}"),
    }
}

fn set_stack_arguments(loaded: &mut PpcLoadedApp, values: [u32; 4]) {
    for (offset, value) in values.into_iter().enumerate() {
        let slot = ppc_parameter_area_slot_addr(loaded.cpu.gpr[1], 8 + offset).unwrap();
        loaded.memory.write_u32_be(slot, value).unwrap();
    }
}

#[test]
fn text_encoding_converter_symbols_resolve() {
    for library in ["CarbonLib", "TextEncodingConverter"] {
        assert_eq!(
            dispatcher_target_for_import(library, "ConvertFromTextToUnicode"),
            PpcImportDispatcherTarget::TextEncoding(
                PpcTextEncodingOperation::ConvertFromTextToUnicode
            )
        );
    }
}

#[test]
fn mac_roman_round_trips_through_unicode() {
    use PpcTextEncodingOperation as Op;
    let mut loaded =
        load_pef_application(&synthetic_pef_with_import(b"ConvertFromTextToUnicode")).unwrap();
    let scratch = PPC_DATA_BASE + 0x1000;
    loaded.memory.add_region(scratch, vec![0; 0x200]);
    // Leave room above the stack pointer for the caller's parameter area.
    loaded.cpu.gpr[1] = PPC_STACK_TOP - 0x400;
    let (info_out, source, output, read, length) = (
        scratch,
        scratch + 0x10,
        scratch + 0x40,
        scratch + 0x80,
        scratch + 0x84,
    );

    loaded.cpu.gpr[3] = 0;
    loaded.cpu.gpr[4] = info_out;
    assert_eq!(tec(Op::CreateTextToUnicodeInfo, &mut loaded), 0);
    let to_unicode = loaded.memory.read_u32_be(info_out).unwrap();
    loaded.cpu.gpr[3] = 0x0100; // kTextEncodingMacJapanese
    assert_eq!(
        tec(Op::CreateTextToUnicodeInfo, &mut loaded),
        (-8738i32) as u32
    );

    // "café" in Mac OS Roman: é is 0x8E.
    loaded.memory.write_bytes(source, b"caf\x8E").unwrap();
    loaded.cpu.gpr[3] = to_unicode;
    loaded.cpu.gpr[4] = 4;
    loaded.cpu.gpr[5] = source;
    loaded.cpu.gpr[9] = 0;
    set_stack_arguments(&mut loaded, [64, read, length, output]);
    assert_eq!(tec(Op::ConvertFromTextToUnicode, &mut loaded), 0);
    assert_eq!(loaded.memory.read_u32_be(read), Some(4));
    assert_eq!(loaded.memory.read_u32_be(length), Some(8));
    assert_eq!(loaded.memory.read_u16_be(output + 6), Some(0x00E9));

    loaded.cpu.gpr[3] = 0;
    loaded.cpu.gpr[4] = info_out;
    assert_eq!(tec(Op::CreateUnicodeToTextInfo, &mut loaded), 0);
    let to_text = loaded.memory.read_u32_be(info_out).unwrap();
    // "é→" : the arrow has no Mac OS Roman form and falls back to '?'.
    for (index, unit) in [0x00E9u16, 0x2192].into_iter().enumerate() {
        loaded
            .memory
            .write_u16_be(source + index as u32 * 2, unit)
            .unwrap();
    }
    loaded.cpu.gpr[3] = to_text;
    loaded.cpu.gpr[4] = 4;
    loaded.cpu.gpr[5] = source;
    set_stack_arguments(&mut loaded, [64, read, length, output]);
    assert_eq!(tec(Op::ConvertFromUnicodeToText, &mut loaded), 0);
    assert_eq!(loaded.memory.read_u32_be(length), Some(2));
    assert_eq!(loaded.memory.read_u8(output), Some(0x8E));
    assert_eq!(loaded.memory.read_u8(output + 1), Some(b'?'));

    // A one-byte output buffer converts one character and reports it full.
    set_stack_arguments(&mut loaded, [1, read, length, output]);
    assert_eq!(
        tec(Op::ConvertFromUnicodeToText, &mut loaded),
        (-8785i32) as u32
    );
    assert_eq!(loaded.memory.read_u32_be(read), Some(2));

    loaded.cpu.gpr[7] = info_out;
    assert_eq!(tec(Op::UpgradeScriptInfoToTextEncoding, &mut loaded), 0);
    assert_eq!(loaded.memory.read_u32_be(info_out), Some(0));
}

#[test]
fn tec_objects_convert_resume_flush_and_reject_disposed_handles() {
    let mut loaded = load_pef_application(&synthetic_pef_with_library_import(
        b"TextEncodingConverter",
        b"TECCreateConverter",
    ))
    .unwrap();
    let base = PPC_STACK_TOP - 512;
    let (input, output, consumed, produced) = (base + 32, base + 64, base + 128, base + 132);
    loaded.cpu.gpr[3..6].copy_from_slice(&[0x100, 0, 2]);
    assert_eq!(
        tec(PpcTextEncodingOperation::CreateEncoding, &mut loaded),
        0x0800_0100
    );
    loaded.cpu.gpr[3..6].copy_from_slice(&[0x1234, 0x3ff, 0x3f]);
    assert_eq!(
        tec(PpcTextEncodingOperation::CreateEncoding, &mut loaded),
        0x3fff_1234
    );
    loaded.cpu.gpr[3..6].copy_from_slice(&[base, 0, 0x0800_0100]);
    assert_eq!(
        tec(PpcTextEncodingOperation::CreateConverter, &mut loaded),
        0
    );
    let handle = loaded.memory.read_u32_be(base).unwrap();
    loaded
        .memory
        .write_bytes(input, &[b'A', 0x8e, b'Z'])
        .unwrap();
    loaded.memory.write_bytes(output, &[0xa5; 12]).unwrap();
    loaded.cpu.gpr[3..10].copy_from_slice(&[handle, input, 3, consumed, output, 2, produced]);
    assert_eq!(
        tec(PpcTextEncodingOperation::ConvertText, &mut loaded),
        (-8785i32) as u32
    );
    assert_eq!(loaded.memory.read_u32_be(consumed), Some(1));
    assert_eq!(loaded.memory.read_u32_be(produced), Some(1));
    assert_eq!(loaded.memory.read_u8(output), Some(b'A'));
    assert_eq!(loaded.memory.read_u8(output + 1), Some(0xa5));
    loaded.cpu.gpr[3..10].copy_from_slice(&[
        handle,
        input + 1,
        2,
        consumed,
        output + 1,
        3,
        produced,
    ]);
    assert_eq!(tec(PpcTextEncodingOperation::ConvertText, &mut loaded), 0);
    assert_eq!(loaded.memory.read_u32_be(consumed), Some(2));
    assert_eq!(loaded.memory.read_u32_be(produced), Some(3));
    assert_eq!(
        loaded.memory.read_u32_be(output),
        Some(u32::from_be_bytes(*b"A\xc3\xa9Z"))
    );
    loaded.cpu.gpr[3..7].copy_from_slice(&[handle, output, 12, produced]);
    assert_eq!(tec(PpcTextEncodingOperation::FlushText, &mut loaded), 0);
    assert_eq!(loaded.memory.read_u32_be(produced), Some(0));
    assert_eq!(
        tec(PpcTextEncodingOperation::DisposeConverter, &mut loaded),
        0
    );
    assert_eq!(
        tec(PpcTextEncodingOperation::DisposeConverter, &mut loaded),
        (-8748i32) as u32
    );
    assert_eq!(
        tec(PpcTextEncodingOperation::FlushText, &mut loaded),
        (-8748i32) as u32
    );
}

#[test]
fn tec_unicode_preserves_scalar_boundaries_and_reports_conversion_errors() {
    let mut loaded =
        load_pef_application(&synthetic_pef_with_import(b"TECCreateConverter")).unwrap();
    let base = PPC_STACK_TOP - 512;
    let (input, output, consumed, produced) = (base + 32, base + 64, base + 128, base + 132);
    for (source, target, bytes, capacity, error, expected) in [
        (
            0x0800_0100,
            0x100,
            &b"\xf0\x9f\x98\x80"[..],
            4,
            0,
            &b"\xd8\x3d\xde\x00"[..],
        ),
        (
            0x100,
            0x0c00_0100,
            &b"\xd8\x3d\xde\x00"[..],
            4,
            0,
            &b"\x00\x01\xf6\x00"[..],
        ),
        (0x500, 0x0800_0100, &b"\x80"[..], 3, 0, &b"\xe2\x82\xac"[..]),
        (0x201, 0x0800_0100, &b"\xe9"[..], 2, 0, &b"\xc3\xa9"[..]),
        (0x0800_0100, 0x100, &b"\xf0\x9f"[..], 4, -8753, &b""[..]),
        (0x0800_0100, 0x100, &b"\xf0\x80"[..], 4, -8739, &b""[..]),
        (0x0800_0100, 0x100, &b"\xc0\x80"[..], 4, -8739, &b""[..]),
        (0x100, 0x0800_0100, &b"\xdc\x00"[..], 4, -8739, &b""[..]),
        (
            0x0800_0100,
            0x100,
            &b"\xf0\x9f\x98\x80"[..],
            2,
            -8750,
            &b""[..],
        ),
        (0x0800_0100, 0x600, &b"\xc3\xa9"[..], 4, -8754, &b""[..]),
    ] {
        loaded.cpu.gpr[3..6].copy_from_slice(&[base, source, target]);
        assert_eq!(
            tec(PpcTextEncodingOperation::CreateConverter, &mut loaded),
            0
        );
        let handle = loaded.memory.read_u32_be(base).unwrap();
        loaded.memory.write_bytes(input, bytes).unwrap();
        loaded.memory.write_bytes(output, &[0xa5; 12]).unwrap();
        loaded.cpu.gpr[3..10].copy_from_slice(&[
            handle,
            input,
            bytes.len() as u32,
            consumed,
            output,
            capacity,
            produced,
        ]);
        assert_eq!(
            tec(PpcTextEncodingOperation::ConvertText, &mut loaded),
            error as u32
        );
        assert_eq!(
            loaded.memory.read_u32_be(consumed),
            Some(if error == 0 { bytes.len() as u32 } else { 0 })
        );
        assert_eq!(
            loaded.memory.read_u32_be(produced),
            Some(expected.len() as u32)
        );
        for (index, &byte) in expected.iter().enumerate() {
            assert_eq!(loaded.memory.read_u8(output + index as u32), Some(byte));
        }
        assert_eq!(
            loaded.memory.read_u8(output + expected.len() as u32),
            Some(0xa5)
        );
        loaded.cpu.gpr[3] = handle;
        assert_eq!(
            tec(PpcTextEncodingOperation::DisposeConverter, &mut loaded),
            0
        );
    }
    loaded.cpu.gpr[3..6].copy_from_slice(&[base, 1, 0x100]);
    assert_eq!(
        tec(PpcTextEncodingOperation::CreateConverter, &mut loaded),
        (-8738i32) as u32
    );
    assert_eq!(loaded.memory.read_u32_be(base), Some(0));
}
