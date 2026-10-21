//! Independently sampled masks and exact Color QuickDraw component blending.
#[derive(Clone, Debug)]
pub struct QuickDrawGpuDeepMask {
    pub source_depth: u32,
    pub destination_depth: u32,
    pub mask_depth: u32,
    pub mode: u32,
    /// Classic traps use replicated RGB555 and full-precision inverse tables.
    pub classic_colors: bool,
    pub source_clut: Vec<[u16; 3]>,
    pub destination_clut: Vec<[u16; 3]>,
    pub mask_clut: Vec<[u16; 3]>,
    pub mask_width: u32,
    pub mask_height: u32,
    pub mask_pixels: Vec<u32>,
    pub mask_columns: Vec<u32>,
    pub mask_rows: Vec<u32>,
    pub background: Vec<u32>,
}
fn limit(depth: u32) -> u32 {
    if depth == 32 {
        u32::MAX
    } else {
        (1 << depth) - 1
    }
}
impl QuickDrawGpuDeepMask {
    pub(super) fn valid(&self, width: usize, height: usize, source: &[u32]) -> bool {
        if [self.source_depth, self.destination_depth, self.mask_depth]
            .iter()
            .any(|d| !matches!(d, 1 | 2 | 4 | 8 | 16 | 32))
            || self.mode > 63
        {
            return false;
        }
        let colors = |d| {
            if matches!(d, 2 | 4 | 8) {
                1usize << d
            } else {
                0
            }
        };
        self.source_clut.len() == colors(self.source_depth)
            && self.destination_clut.len() == colors(self.destination_depth)
            && self.mask_clut.len() == colors(self.mask_depth)
            && self.background.len() == width * height
            && self.mask_columns.len() == width
            && self.mask_rows.len() == height
            && self.mask_width > 0
            && self.mask_height > 0
            && u64::from(self.mask_width) * u64::from(self.mask_height) <= 4_194_304
            && self.mask_pixels.len() as u64
                == u64::from(self.mask_width) * u64::from(self.mask_height)
            && self.mask_columns.iter().all(|x| *x < self.mask_width)
            && self.mask_rows.iter().all(|y| *y < self.mask_height)
            && source.iter().all(|v| *v <= limit(self.source_depth))
            && self
                .mask_pixels
                .iter()
                .all(|v| *v <= limit(self.mask_depth))
            && self
                .background
                .iter()
                .all(|v| *v <= limit(self.destination_depth))
    }
    #[cfg(test)]
    fn mask(&self, i: usize) -> u32 {
        self.mask_pixels[(self.mask_rows[i / self.mask_columns.len()] * self.mask_width
            + self.mask_columns[i % self.mask_columns.len()]) as usize]
    }
    #[cfg(test)]
    pub(super) fn covered(&self, i: usize) -> bool {
        self.mask_depth != 1 || self.mask(i) != 0
    }
    #[cfg(test)]
    pub(super) fn reference(&self, source: u32, i: usize) -> u32 {
        let rgb = |depth, pixel, clut: &[[u16; 3]]| rgb(depth, pixel, clut, self.classic_colors);
        let encode =
            |depth, color, clut: &[[u16; 3]]| encode(depth, color, clut, self.classic_colors);
        let destination = self.background[i];
        let s = rgb(self.source_depth, source, &self.source_clut);
        let d = rgb(self.destination_depth, destination, &self.destination_clut);
        let boolean = |s: u32| {
            let mask = limit(self.destination_depth);
            match self.mode {
                1 => s | destination,
                2 => s ^ destination,
                3 => !s & destination & mask,
                4 => !s & mask,
                5 => (!s & mask) | destination,
                6 => (!s & mask) ^ destination,
                7 => s & destination,
                _ => s,
            }
        };
        if self.mask_depth == 1 {
            return boolean(encode(self.destination_depth, s, &self.destination_clut));
        }
        let s = if (1..=7).contains(&self.mode) {
            rgb(
                self.destination_depth,
                boolean(encode(self.destination_depth, s, &self.destination_clut)),
                &self.destination_clut,
            )
        } else {
            s
        };
        let m = rgb(self.mask_depth, self.mask(i), &self.mask_clut);
        let color = std::array::from_fn(|c| {
            ((u64::from(s[c]) * (65535 - u64::from(m[c]))
                + u64::from(d[c]) * u64::from(m[c])
                + 32767)
                / 65535) as u16
        });
        encode(self.destination_depth, color, &self.destination_clut)
    }
}
#[cfg(test)]
fn rgb(depth: u32, pixel: u32, clut: &[[u16; 3]], classic: bool) -> [u16; 3] {
    match depth {
        1 => {
            if pixel == 0 {
                [65535; 3]
            } else {
                [0; 3]
            }
        }
        2 | 4 | 8 => clut[pixel as usize],
        16 => [pixel >> 10 & 31, pixel >> 5 & 31, pixel & 31].map(|v| {
            if classic {
                ((v << 11) | (v << 6) | (v << 1) | (v >> 4)) as u16
            } else {
                ((v * 65535 + 15) / 31) as u16
            }
        }),
        32 => [pixel >> 16 & 255, pixel >> 8 & 255, pixel & 255].map(|v| (v * 257) as u16),
        _ => unreachable!(),
    }
}
#[cfg(test)]
fn encode(depth: u32, color: [u16; 3], clut: &[[u16; 3]], classic: bool) -> u32 {
    match depth {
        1 => u32::from(color.iter().map(|v| u32::from(*v)).sum::<u32>() <= 98302),
        16 => {
            u32::from(color[0] >> 11) << 10
                | u32::from(color[1] >> 11) << 5
                | u32::from(color[2] >> 11)
        }
        32 => {
            u32::from(color[0] >> 8) << 16
                | u32::from(color[1] >> 8) << 8
                | u32::from(color[2] >> 8)
        }
        _ if classic => {
            let last = clut.len() - 1;
            if color == [65535; 3] && clut[0] == [65535; 3] {
                return 0;
            }
            if color == clut[last] {
                return last as u32;
            }
            if let Some(index) = clut.iter().position(|entry| *entry == color) {
                return index as u32;
            }
            clut.iter()
                .enumerate()
                .min_by_key(|(_, rgb)| {
                    (0..3)
                        .map(|i| (i64::from(color[i]) - i64::from(rgb[i])).pow(2) as u64)
                        .sum::<u64>()
                })
                .unwrap()
                .0 as u32
        }
        _ => {
            clut.iter()
                .enumerate()
                .min_by_key(|(_, rgb)| {
                    (0..3)
                        .map(|i| (i32::from(color[i] >> 11) - i32::from(rgb[i] >> 11)).pow(2))
                        .sum::<i32>()
                })
                .unwrap()
                .0 as u32
        }
    }
}
