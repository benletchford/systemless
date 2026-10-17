//! Native Color QuickDraw arithmetic on snapshotted logical pixels.
#[derive(Clone, Debug)]
pub struct QuickDrawGpuArithmetic {
    pub depth: u32,
    pub mode: u32,
    pub op: [u16; 3],
    pub source_clut: Vec<[u16; 3]>,
    pub destination_clut: Vec<[u16; 3]>,
    pub background: Vec<u32>,
}
impl QuickDrawGpuArithmetic {
    pub(super) fn valid(&self, count: usize, source: &[u32]) -> bool {
        if !matches!(self.depth, 1 | 2 | 4 | 8 | 16 | 32)
            || !matches!(self.mode, 32 | 33 | 34 | 35 | 37 | 38 | 39)
        {
            return false;
        }
        let colors = if matches!(self.depth, 2 | 4 | 8) {
            1usize << self.depth
        } else {
            0
        };
        let limit = if self.depth == 32 {
            u32::MAX
        } else {
            (1 << self.depth) - 1
        };
        self.background.len() == count
            && self.source_clut.len() == colors
            && self.destination_clut.len() == colors
            && source.iter().chain(&self.background).all(|v| *v <= limit)
    }
    #[cfg(test)]
    pub(super) fn reference(&self, source: u32, destination: u32) -> u32 {
        if self.depth == 1 {
            return match self.mode {
                32 => source,
                33 | 37 => destination & !source,
                34 | 38 => destination ^ source,
                35 | 39 => destination | source,
                _ => unreachable!(),
            };
        }
        let rgb = |pixel: u32, clut: &[[u16; 3]]| -> [u16; 3] {
            match self.depth {
                2 | 4 | 8 => clut[pixel as usize],
                16 => [pixel >> 10 & 31, pixel >> 5 & 31, pixel & 31]
                    .map(|v| ((v * 65535 + 15) / 31) as u16),
                32 => [pixel >> 16 & 255, pixel >> 8 & 255, pixel & 255].map(|v| (v * 257) as u16),
                _ => unreachable!(),
            }
        };
        let source = rgb(source, &self.source_clut);
        let destination = rgb(destination, &self.destination_clut);
        let color: [u16; 3] = std::array::from_fn(|i| {
            let (s, d, op) = (source[i], destination[i], self.op[i]);
            match self.mode {
                32 => {
                    ((u64::from(s) * u64::from(op) + u64::from(d) * (65536 - u64::from(op))) >> 16)
                        as u16
                }
                33 => s.saturating_add(d).min(op),
                34 => s.wrapping_add(d),
                35 => s.saturating_sub(d).max(op),
                37 => s.max(d),
                38 => s.wrapping_sub(d),
                39 => s.min(d),
                _ => unreachable!(),
            }
        });
        match self.depth {
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
            _ => {
                self.destination_clut
                    .iter()
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
}
