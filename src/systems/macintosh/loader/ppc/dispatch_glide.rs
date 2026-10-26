//! Glide 2.x entry points for the machine profile without a Voodoo board.
use super::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PpcGlideOperation {
    QueryHardware,
    NoDeviceStateChange,
    Unavailable,
    EmptyDeviceValue,
    TextureMemoryRequired,
    TextureInfoMemoryRequired,
}

// Glide 2.x ditex.c describes hardware mip levels with a minimum two texels
// in each dimension, byte widths selected by format, and eight-byte alignment.
fn texture_bytes(small: u32, large: u32, aspect: u32, format: u32, levels: u32) -> u32 {
    if small > 8 || large > small || aspect > 6 || format > 15 || !(1..=3).contains(&levels) {
        return 0;
    }
    let shift = aspect.abs_diff(3);
    let mut texels = 0;
    for lod in large..=small {
        if levels & (1 << (lod & 1)) == 0 {
            continue;
        }
        let side = 256u32 >> lod;
        texels += side.max(2) * (side >> shift).max(2);
    }
    let bytes = texels * if format >= 8 { 2 } else { 1 };
    (bytes + 7) & !7
}

pub(super) fn dispatch(
    operation: PpcGlideOperation,
    cpu: &PpcCpu,
    memory: &mut PpcSectionMem,
) -> PpcImportAction {
    match operation {
        PpcGlideOperation::QueryHardware => {
            // The only meaningful configuration field with no boards is
            // num_sst. Never invent a board, framebuffer or texture memory.
            let output = cpu.gpr[3];
            if output != 0 && ppc_memory_can_write_bytes(memory, output, 4) {
                let _ = memory.write_u32_be(output, 0);
            }
            PpcImportAction::Return(0) // FXFALSE
        }
        PpcGlideOperation::NoDeviceStateChange => {
            // No grSstWinOpen call can succeed in this profile. Lifecycle
            // calls are safe; state/draw calls have no active device to affect.
            PpcImportAction::ReturnPreserve
        }
        PpcGlideOperation::Unavailable | PpcGlideOperation::EmptyDeviceValue => {
            PpcImportAction::Return(0)
        }
        PpcGlideOperation::TextureMemoryRequired => PpcImportAction::Return(texture_bytes(
            cpu.gpr[3], cpu.gpr[4], cpu.gpr[5], cpu.gpr[6], 3,
        )),
        PpcGlideOperation::TextureInfoMemoryRequired => {
            let info = cpu.gpr[4];
            let result = if info != 0 && ppc_memory_can_read_bytes(memory, info, 16) {
                texture_bytes(
                    memory.read_u32_be(info).unwrap(),
                    memory.read_u32_be(info + 4).unwrap(),
                    memory.read_u32_be(info + 8).unwrap(),
                    memory.read_u32_be(info + 12).unwrap(),
                    cpu.gpr[3],
                )
            } else {
                0
            };
            PpcImportAction::Return(result)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn texture_sizes_include_hardware_padding_and_selected_mip_levels() {
        assert_eq!(texture_bytes(0, 0, 3, 0, 3), 65536);
        assert_eq!(texture_bytes(0, 0, 3, 11, 3), 131072);
        assert_eq!(texture_bytes(8, 0, 3, 0, 3), 87384);
        assert_eq!(texture_bytes(8, 0, 0, 0, 3), 10944);
        assert_eq!(texture_bytes(8, 0, 3, 0, 1), 69912);
        assert_eq!(texture_bytes(8, 0, 3, 0, 2), 17480);
        assert_eq!(texture_bytes(5, 5, 0, 0, 3), 16); // 8x1 occupies 8x2.
        assert_eq!(texture_bytes(6, 6, 0, 0, 3), 8); // 4x1 occupies 4x2.
        assert_eq!(texture_bytes(8, 8, 3, 0, 3), 8); // 1x1 occupies 2x2, aligned.
        assert_eq!(texture_bytes(8, 8, 3, 0, 2), 0); // No selected odd level.
        for aspect in 0..=6 {
            assert_eq!(
                texture_bytes(8, 0, aspect, 0, 3),
                texture_bytes(8, 0, 6 - aspect, 0, 3)
            );
        }
        for args in [
            (9, 0, 3, 0, 3),
            (0, 1, 3, 0, 3),
            (0, 0, 7, 0, 3),
            (0, 0, 3, 16, 3),
            (0, 0, 3, 0, 0),
            (0, 0, 3, 0, 4),
        ] {
            assert_eq!(texture_bytes(args.0, args.1, args.2, args.3, args.4), 0);
        }
    }
}
