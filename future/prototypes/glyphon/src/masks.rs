//! Bounded content identities for single-cell preparation. A reset invalidates the
//! entire atlas before an ID is reused. Metrics and placement are not cached.
pub struct Masks {
    entries: Vec<(u16, u16, Vec<u8>)>,
    bytes: usize,
    max_entries: usize,
    max_bytes: usize,
}
impl Masks {
    pub fn new(max_entries: usize, max_bytes: usize) -> Self {
        assert!((1..=65536).contains(&max_entries));
        Self {
            entries: Vec::new(),
            bytes: 0,
            max_entries,
            max_bytes,
        }
    }
    pub fn resolve(
        &mut self,
        width: u16,
        height: u16,
        bits: &[u8],
    ) -> Result<(u16, bool), &'static str> {
        if width == 0
            || height == 0
            || width > 4096
            || height > 4096
            || bits.len() != usize::from(width) * usize::from(height)
            || bits.len() > self.max_bytes
            || bits.iter().any(|b| *b > 1)
        {
            return Err("invalid or oversized binary glyph mask");
        }
        if let Some(id) = self
            .entries
            .iter()
            .position(|(w, h, b)| *w == width && *h == height && b == bits)
        {
            return Ok((id as u16, false));
        }
        let reset =
            self.entries.len() == self.max_entries || bits.len() > self.max_bytes - self.bytes;
        if reset {
            self.entries.clear();
            self.bytes = 0;
        }
        let id = self.entries.len() as u16;
        self.entries.push((width, height, bits.to_vec()));
        self.bytes += bits.len();
        Ok((id, reset))
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn content_dimensions_and_budgets() {
        let mut cache = Masks::new(2, 4);
        assert_eq!(cache.resolve(2, 1, &[0, 1]), Ok((0, false)));
        assert_eq!(cache.resolve(2, 1, &[0, 1]), Ok((0, false)));
        assert_eq!(cache.resolve(1, 2, &[0, 1]), Ok((1, false)));
        assert!(cache.resolve(2, 1, &[0, 2]).is_err());
        assert!(cache.resolve(3, 2, &[0; 6]).is_err());
        assert_eq!(cache.entries.len(), 2);
        assert_eq!(cache.resolve(2, 1, &[1, 1]), Ok((0, true)));
        assert_eq!(cache.bytes, 2);
        assert_eq!(cache.resolve(1, 1, &[0]), Ok((1, false)));
    }
    #[test]
    fn byte_budget_resets_before_entry_limit() {
        let mut cache = Masks::new(8, 3);
        assert_eq!(cache.resolve(2, 1, &[0, 0]), Ok((0, false)));
        assert_eq!(cache.resolve(2, 1, &[1, 0]), Ok((0, true)));
        assert_eq!(cache.bytes, 2);
    }
}
