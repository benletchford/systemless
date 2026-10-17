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
        {
            return false;
        }
        let x: u64 = self.columns.iter().map(|g| u64::from(g[1] - g[0])).sum();
        let y: u64 = self.rows.iter().map(|g| u64::from(g[1] - g[0])).sum();
        x.saturating_mul(y) <= 64_000_000
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
                pixels.push(
                    self.palette
                        .as_ref()
                        .map_or(maximum, |p| p[maximum as usize]),
                );
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
