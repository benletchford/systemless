//! Snapshotted pixel transfers and compatibility scaling groups.
#[derive(Clone, Debug)]
pub struct QuickDrawGpuBlit {
    pub width: u32,
    pub height: u32,
    /// Raw logical pixels, including unused bits in direct-color copies.
    pub pixels: Vec<u32>,
    /// Half-open source groups for each destination column and row.
    /// A singleton samples directly; larger groups reduce by maximum index.
    pub columns: Vec<[u32; 2]>,
    pub rows: Vec<[u32; 2]>,
    /// Indexed source colors mapped into raw destination pixels.
    pub palette: Option<[u32; 256]>,
    pub direct: Option<QuickDrawGpuDirectColor>,
    pub clip: Option<QuickDrawGpuBlitClip>,
    pub arithmetic: Option<super::QuickDrawGpuArithmetic>,
}
impl QuickDrawGpuBlit {
    pub fn valid(&self) -> bool {
        let source = u64::from(self.width) * u64::from(self.height);
        let destination = self.columns.len().saturating_mul(self.rows.len());
        let valid_groups = |groups: &[[u32; 2]], limit| {
            !groups.is_empty()
                && groups.len() <= 32768
                && groups.iter().all(|g| g[0] < g[1] && g[1] <= limit)
        };
        if self.width == 0
            || self.height == 0
            || source > 4_194_304
            || source != self.pixels.len() as u64
            || destination > 4_194_304
            || !valid_groups(&self.columns, self.width)
            || !valid_groups(&self.rows, self.height)
            || (self.palette.is_some() && self.pixels.iter().any(|v| *v > 255))
            || self.clip.as_ref().is_some_and(|clip| {
                clip.coverage
                    .as_ref()
                    .is_some_and(|mask| mask.len() != destination)
                    || clip
                        .transparent
                        .is_some_and(|[key, mask]| mask == 0 || key & !mask != 0)
            })
            || self.arithmetic.as_ref().is_some_and(|a| {
                self.palette.is_some()
                    || self.direct.is_some()
                    || !a.valid(destination, &self.pixels)
                    || self
                        .columns
                        .iter()
                        .chain(&self.rows)
                        .any(|g| g[1] - g[0] != 1)
            })
            || self.direct.as_ref().is_some_and(|d| {
                !d.valid()
                    || self.palette.is_some()
                    || (d.source_depth == 16 && self.pixels.iter().any(|v| *v > 65535))
                    || self
                        .columns
                        .iter()
                        .chain(&self.rows)
                        .any(|g| g[1] - g[0] != 1)
            })
        {
            return false;
        }
        let x: u64 = self.columns.iter().map(|g| u64::from(g[1] - g[0])).sum();
        let y: u64 = self.rows.iter().map(|g| u64::from(g[1] - g[0])).sum();
        x.saturating_mul(y) <= 64_000_000
    }
    #[cfg(test)]
    pub(crate) fn reference_cells(&self) -> Option<Vec<super::QuickDrawGpuPixel>> {
        let values = self.reference()?;
        Some(
            values
                .into_iter()
                .enumerate()
                .map(|(i, value)| {
                    let column = self.columns[i % self.columns.len()];
                    let row = self.rows[i / self.columns.len()];
                    let mut source = 0;
                    for y in row[0]..row[1] {
                        for x in column[0]..column[1] {
                            source = source.max(self.pixels[(y * self.width + x) as usize]);
                        }
                    }
                    let covered = self.clip.as_ref().is_none_or(|clip| {
                        clip.coverage.as_ref().is_none_or(|mask| mask[i] != 0)
                            && clip
                                .transparent
                                .is_none_or(|[key, mask]| source & mask != key)
                    });
                    super::QuickDrawGpuPixel { value, covered }
                })
                .collect(),
        )
    }
    #[cfg(test)]
    pub(crate) fn reference(&self) -> Option<Vec<u32>> {
        if !self.valid() {
            return None;
        }
        let mut pixels = Vec::with_capacity(self.columns.len() * self.rows.len());
        for row in &self.rows {
            for column in &self.columns {
                let mut maximum = 0;
                for y in row[0]..row[1] {
                    for x in column[0]..column[1] {
                        maximum = maximum.max(self.pixels[(y * self.width + x) as usize]);
                    }
                }
                let value = self
                    .palette
                    .as_ref()
                    .map_or(maximum, |p| p[maximum as usize]);
                let value = self.arithmetic.as_ref().map_or_else(
                    || self.direct.as_ref().map_or(value, |d| d.convert(value)),
                    |a| a.reference(maximum, a.background[pixels.len()]),
                );
                pixels.push(value);
            }
        }
        Some(pixels)
    }
}

/// A packed srcCopy, one GPU invocation per destination byte. Source and
/// background rows are snapshots, so in-place copies cannot race their reads.
#[derive(Clone, Debug)]
pub struct QuickDrawGpuPackedCopy {
    pub depth: u32,
    pub width: u32,
    pub height: u32,
    pub source_stride: u32,
    pub source_bit: u32,
    pub source: Vec<u8>,
    pub destination_stride: u32,
    pub destination_bit: u32,
    pub background: Vec<u8>,
    pub palette: Option<[u8; 256]>,
}
impl QuickDrawGpuPackedCopy {
    pub fn valid(&self) -> bool {
        if !matches!(self.depth, 1 | 2 | 4)
            || self.width == 0
            || self.width > 65535
            || self.height == 0
            || self.height > 32768
            || self.source_bit >= 8
            || self.destination_bit >= 8
            || self.source_bit % self.depth != 0
            || self.destination_bit % self.depth != 0
        {
            return false;
        }
        let bits = u64::from(self.width) * u64::from(self.depth);
        let source = u64::from(self.source_stride) * u64::from(self.height);
        let destination = u64::from(self.destination_stride) * u64::from(self.height);
        source <= 4_194_304
            && destination <= 4_194_304
            && source == self.source.len() as u64
            && destination == self.background.len() as u64
            && u64::from(self.source_bit) + bits <= u64::from(self.source_stride) * 8
            && u64::from(self.destination_bit) + bits <= u64::from(self.destination_stride) * 8
            && self.palette.as_ref().is_none_or(|p| {
                p[..1usize << self.depth]
                    .iter()
                    .all(|v| u32::from(*v) < 1 << self.depth)
            })
    }
    #[cfg(test)]
    pub(crate) fn reference(&self) -> Option<Vec<u8>> {
        if !self.valid() {
            return None;
        }
        let mut output = self.background.clone();
        let mask = ((1 << self.depth) - 1) as u8;
        for y in 0..self.height {
            for x in 0..self.width {
                let bit = self.source_bit + x * self.depth;
                let value = (self.source[(y * self.source_stride + bit / 8) as usize]
                    >> (8 - self.depth - bit % 8))
                    & mask;
                let value = self.palette.as_ref().map_or(value, |p| p[value as usize]);
                let bit = self.destination_bit + x * self.depth;
                let shift = 8 - self.depth - bit % 8;
                let byte = &mut output[(y * self.destination_stride + bit / 8) as usize];
                *byte = (*byte & !(mask << shift)) | (value << shift);
            }
        }
        Some(output)
    }
}

/// Direct source conversion using QuickDraw's five-bit inverse-table matching.
#[derive(Clone, Debug)]
pub struct QuickDrawGpuDirectColor {
    pub source_depth: u32,
    pub destination_depth: u32,
    /// Destination colors as RGB555, in index order. Empty for direct output.
    pub palette: Vec<u32>,
}
impl QuickDrawGpuDirectColor {
    fn valid(&self) -> bool {
        matches!(self.source_depth, 16 | 32)
            && matches!(self.destination_depth, 1 | 2 | 4 | 8 | 16 | 32)
            && self.source_depth != self.destination_depth
            && self.palette.len()
                == if self.destination_depth <= 8 {
                    1usize << self.destination_depth
                } else {
                    0
                }
            && self.palette.iter().all(|p| *p <= 32767)
    }
    #[cfg(test)]
    fn convert(&self, pixel: u32) -> u32 {
        let rgb = if self.source_depth == 16 {
            [pixel >> 10 & 31, pixel >> 5 & 31, pixel & 31].map(|v| ((v * 65535 + 15) / 31) >> 8)
        } else {
            [pixel >> 16 & 255, pixel >> 8 & 255, pixel & 255]
        };
        match self.destination_depth {
            32 => (rgb[0] << 16) | (rgb[1] << 8) | rgb[2],
            16 => ((rgb[0] >> 3) << 10) | ((rgb[1] >> 3) << 5) | (rgb[2] >> 3),
            1 if self.source_depth == 32 => u32::from(rgb.iter().sum::<u32>() <= 382),
            _ => {
                let target = rgb.map(|v| (v >> 3) as i32);
                self.palette
                    .iter()
                    .enumerate()
                    .min_by_key(|(_, p)| {
                        let color = [
                            (*p >> 10 & 31) as i32,
                            (*p >> 5 & 31) as i32,
                            (*p & 31) as i32,
                        ];
                        (0..3).map(|i| (target[i] - color[i]).pow(2)).sum::<i32>()
                    })
                    .map_or(0, |(i, _)| i as u32)
            }
        }
    }
}

/// Destination coverage and a transparent key in the source pixel format.
#[derive(Clone, Debug)]
pub struct QuickDrawGpuBlitClip {
    pub coverage: Option<Vec<u8>>,
    /// [key, mask], allowing xRGB's unused high byte to be ignored.
    pub transparent: Option<[u32; 2]>,
}
