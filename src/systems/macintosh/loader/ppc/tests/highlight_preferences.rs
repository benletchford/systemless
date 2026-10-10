use super::*;
use crate::memory::globals::addr;

#[test]
fn highlight_preferences_copy_rgb_words_and_preserve_void_argument() {
    for library in [b"InterfaceLib".as_slice(), b"CarbonLib".as_slice()] {
        let mut loaded = load_pef_application(&synthetic_pef_with_library_import(
            library,
            b"LMGetHiliteRGB",
        ))
        .unwrap();
        assert_eq!(
            loaded.imports[0].dispatcher_target,
            PpcImportDispatcherTarget::LMGetHiliteRGB
        );
        let setter = load_pef_application(&synthetic_pef_with_library_import(
            library,
            b"LMSetHiliteRGB",
        ))
        .unwrap();
        assert_eq!(
            setter.imports[0].dispatcher_target,
            PpcImportDispatcherTarget::LMSetHiliteRGB
        );
        let output = loaded.cpu.gpr[1] - 0x100;
        loaded.memory.write_u16_be(output - 2, 0x5a5a).unwrap();
        loaded.memory.write_u16_be(output + 6, 0xa5a5).unwrap();
        loaded.cpu.gpr[3] = output;
        run_test_import(&mut loaded, PpcImportDispatcherTarget::LMGetHiliteRGB);
        assert_eq!(loaded.cpu.gpr[3], output);
        assert_eq!(loaded.memory.read_u16_be(output), Some(0));
        assert_eq!(loaded.memory.read_u16_be(output + 2), Some(0x8000));
        assert_eq!(loaded.memory.read_u16_be(output + 4), Some(0));
        for (offset, value) in [(0, 0x1234), (2, 0x5678), (4, 0x9abc)] {
            loaded.memory.write_u16_be(output + offset, value).unwrap();
        }
        run_test_import(&mut loaded, PpcImportDispatcherTarget::LMSetHiliteRGB);
        assert_eq!(loaded.cpu.gpr[3], output);
        for (offset, value) in [(0, 0x1234), (2, 0x5678), (4, 0x9abc)] {
            assert_eq!(
                loaded.memory.read_u16_be(addr::HILITE_RGB + offset),
                Some(value)
            );
            loaded.memory.write_u16_be(output + offset, 0).unwrap();
        }
        run_test_import(&mut loaded, PpcImportDispatcherTarget::LMGetHiliteRGB);
        for (offset, value) in [(0, 0x1234), (2, 0x5678), (4, 0x9abc)] {
            assert_eq!(loaded.memory.read_u16_be(output + offset), Some(value));
        }
        assert_eq!(loaded.memory.read_u16_be(output - 2), Some(0x5a5a));
        assert_eq!(loaded.memory.read_u16_be(output + 6), Some(0xa5a5));
    }
}
