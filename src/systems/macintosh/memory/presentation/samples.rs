//! Retained subpixel storage. Plain guest pixels have no tile; their palette
//! index already lives in guest_values. Stable slabs avoid copying all existing
//! text samples when a new dialog needs more detail. Freed tiles are reused.

const NO_TILE: u32 = u32::MAX;
const SLAB_TILES: usize = 256;
pub(super) const TILE_SAMPLES: usize = 16;

#[derive(Clone, Default)]
pub(super) struct SampleTile {
    // The supported retained scales are 2, 3 and 4. Only scale² entries are
    // used. The common 4x tile occupies exactly 64 bytes.
    pub(super) rgb: [[u8; 3]; TILE_SAMPLES],
    pub(super) indices: [u8; TILE_SAMPLES],
}

pub(super) struct DetailSamples {
    slots: Vec<u32>,
    slabs: Vec<Box<[SampleTile]>>,
    free: Vec<u32>,
}

impl DetailSamples {
    pub(super) fn new(cells: usize) -> Self {
        Self {
            slots: vec![NO_TILE; cells],
            slabs: Vec::new(),
            free: Vec::new(),
        }
    }

    #[inline]
    pub(super) fn get(&self, cell: usize) -> &SampleTile {
        let slot = self.slots[cell] as usize;
        &self.slabs[slot / SLAB_TILES][slot % SLAB_TILES]
    }

    #[inline]
    pub(super) fn get_mut(&mut self, cell: usize) -> &mut SampleTile {
        let slot = self.slots[cell] as usize;
        &mut self.slabs[slot / SLAB_TILES][slot % SLAB_TILES]
    }

    pub(super) fn ensure(&mut self, cell: usize) -> &mut SampleTile {
        if self.slots[cell] == NO_TILE {
            if self.free.is_empty() {
                self.grow();
            }
            self.slots[cell] = self.free.pop().unwrap();
        }
        self.get_mut(cell)
    }

    #[cold]
    fn grow(&mut self) {
        let start = self.slabs.len().checked_mul(SLAB_TILES).unwrap();
        assert!(start + SLAB_TILES < NO_TILE as usize);
        // Reserve the free-list capacity while growing, so erasing a whole
        // page of text never needs a fresh allocation.
        self.free.reserve(start + SLAB_TILES);
        self.slabs
            .push(vec![SampleTile::default(); SLAB_TILES].into_boxed_slice());
        self.free
            .extend((start as u32..(start + SLAB_TILES) as u32).rev());
    }

    pub(super) fn release(&mut self, cell: usize) {
        let slot = std::mem::replace(&mut self.slots[cell], NO_TILE);
        if slot != NO_TILE {
            self.free.push(slot);
        }
    }

    #[cfg(test)]
    pub(super) fn allocated_bytes(&self) -> usize {
        self.slots.capacity() * std::mem::size_of::<u32>()
            + self.slabs.capacity() * std::mem::size_of::<Box<[SampleTile]>>()
            + self
                .slabs
                .iter()
                .map(|slab| std::mem::size_of_val(&**slab))
                .sum::<usize>()
            + self.free.capacity() * std::mem::size_of::<u32>()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn text_tiles_are_stable_and_reused_without_full_screen_storage() {
        let mut pool = DetailSamples::new(800 * 600);
        assert!(pool.allocated_bytes() < 2 * 1024 * 1024);
        pool.ensure(0).rgb[0] = [1, 2, 3];
        let first = pool.get(0) as *const SampleTile;
        for cell in 1..5000 {
            pool.ensure(cell).indices[15] = (cell % 256) as u8;
        }
        assert_eq!(pool.get(0) as *const SampleTile, first);
        assert_eq!(pool.get(0).rgb[0], [1, 2, 3]);
        assert!(pool.allocated_bytes() < 3 * 1024 * 1024);
        let slabs = pool.slabs.len();
        let allocated = pool.allocated_bytes();
        for cell in 0..5000 {
            pool.release(cell);
        }
        for cell in 6000..11000 {
            pool.ensure(cell);
        }
        assert_eq!(pool.slabs.len(), slabs);
        assert_eq!(pool.allocated_bytes(), allocated);
        assert_eq!(pool.get(10999) as *const SampleTile, first);
    }
}
