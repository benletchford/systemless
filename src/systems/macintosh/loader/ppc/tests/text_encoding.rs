use super::*;

fn tec(operation: PpcTextEncodingOperation, loaded: &mut PpcLoadedApp) -> u32 {
    match dispatch_text_encoding::ppc_dispatch_text_encoding(
        operation,
        &loaded.cpu,
        &mut loaded.memory,
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
