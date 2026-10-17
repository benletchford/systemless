//! ROM-free standard cursor artwork, matching the legacy Systemless reference.
//! Resource IDs/layout follow Imaging With QuickDraw, Cursor Utilities, ch. 8.
pub(crate) fn standard(id: i16) -> Option<[u8; 68]> {
    let (data, mask, v, h): ([u16; 16], [u16; 16], i16, i16) = match id {
        1 => {
            let data = [
                0x0ee0, 0x0440, 0x0100, 0x0100, 0x0100, 0x0100, 0x0100, 0x0100, 0x0100, 0x0100,
                0x0100, 0x0100, 0x0100, 0x0100, 0x0440, 0x0ee0,
            ];
            (data, data, 8, 4)
        }
        2 => (
            [
                0x0100, 0x0100, 0x0100, 0x0100, 0x0100, 0x0100, 0, 0xfc7e, 0, 0x0100, 0x0100,
                0x0100, 0x0100, 0x0100, 0x0100, 0,
            ],
            [
                0x0380, 0x0380, 0x0380, 0x0380, 0x0380, 0x0380, 0xfffe, 0xffff, 0xfffe, 0x0380,
                0x0380, 0x0380, 0x0380, 0x0380, 0x0380, 0,
            ],
            7,
            7,
        ),
        3 => {
            let data = [
                0, 0x07c0, 0x07c0, 0x07c0, 0x07c0, 0x07c0, 0xfffe, 0xfffe, 0xfffe, 0xfffe, 0xfffe,
                0x07c0, 0x07c0, 0x07c0, 0x07c0, 0x07c0,
            ];
            (data, data, 8, 8)
        }
        4 => {
            let data = [
                0x07c0, 0x07c0, 0x1ff0, 0x3ff8, 0x3ff8, 0x3ff8, 0x3e78, 0x3e18, 0x3f18, 0x3ff8,
                0x3ff8, 0x3ff8, 0x3ff8, 0x1ff0, 0x07c0, 0x07c0,
            ];
            (data, data, 8, 8)
        }
        _ => return None,
    };
    let mut bytes = [0; 68];
    for (i, word) in data.into_iter().chain(mask).enumerate() {
        bytes[i * 2..i * 2 + 2].copy_from_slice(&word.to_be_bytes());
    }
    bytes[64..66].copy_from_slice(&v.to_be_bytes());
    bytes[66..].copy_from_slice(&h.to_be_bytes());
    Some(bytes)
}
