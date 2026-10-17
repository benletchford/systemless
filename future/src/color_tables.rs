//! ROM-independent standard clut resources. Apple Advanced Color Imaging,
//! Palette Manager: default tables and depth+32 grayscale resources.
//! Exact base palette bytes are compared separately to preserved Systemless.
pub(crate) fn standard(id: i16) -> Option<Vec<u8>> {
    let colors: Vec<[u16; 3]> = match id {
        1 | 2 | 34 | 36 | 40 => {
            let depth = match id {
                1 => 1,
                2 | 34 => 2,
                36 => 4,
                _ => 8,
            };
            let last = (1u32 << depth) - 1;
            (0..=last)
                .map(|i| [((last - i) * 65535 / last) as u16; 3])
                .collect()
        }
        4 => vec![
            [0xffff, 0xffff, 0xffff],
            [0xfc00, 0xf37d, 0x052f],
            [0xffff, 0x648a, 0x028c],
            [0xdd6b, 0x08c2, 0x06a2],
            [0xf2d7, 0x0856, 0x84ec],
            [0x46e3, 0, 0xa53e],
            [0, 0, 0xd400],
            [0x0241, 0xab54, 0xeaff],
            [0x1f21, 0xb793, 0x1431],
            [0, 0x8000, 0x11b0],
            [0x5600, 0x2c9d, 0x0524],
            [0x90d7, 0x7160, 0x3a34],
            [0xc000; 3],
            [0x8000; 3],
            [0x4000; 3],
            [0; 3],
        ],
        8 => {
            // Six levels/channel, omitting cube black until the final slot.
            let mut colors: Vec<_> = (0..215u16)
                .map(|i| {
                    [
                        (5 - i / 36) * 0x3333,
                        (5 - (i / 6) % 6) * 0x3333,
                        (5 - i % 6) * 0x3333,
                    ]
                })
                .collect();
            // Fill each primary/gray ramp with levels absent from the cube.
            for axis in 0..4 {
                for level in (1..15u16).rev().filter(|v| v % 3 != 0) {
                    let mut rgb = [0; 3];
                    if axis == 3 {
                        rgb.fill(level * 0x1111);
                    } else {
                        rgb[axis] = level * 0x1111;
                    }
                    colors.push(rgb);
                }
            }
            colors.push([0; 3]);
            colors
        }
        _ => return None,
    };
    let mut bytes = (id as u32).to_be_bytes().to_vec();
    bytes.extend(0u16.to_be_bytes()); // A source PixMap table, not device metadata.
    bytes.extend(((colors.len() - 1) as u16).to_be_bytes());
    for (index, rgb) in colors.into_iter().enumerate() {
        bytes.extend((index as u16).to_be_bytes());
        for component in rgb {
            bytes.extend(component.to_be_bytes());
        }
    }
    Some(bytes)
}
