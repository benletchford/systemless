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
    pub palette: Option<[u8; 256]>,
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
                        .map_or(maximum, |p| u32::from(p[maximum as usize])),
                );
            }
        }
        Some(pixels)
    }
}
