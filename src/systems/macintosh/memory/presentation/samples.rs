//! Retained subpixel storage. Plain guest pixels have no tile; their palette
//! index already lives in guest_values. Stable slabs avoid copying all existing
//! text samples when a new dialog needs more detail. Freed tiles are reused.
//! Each tile slot also owns its cell's partial-coverage ink, a short list
//! sorted by sample, so text copies and redraws touch the tile's own list
//! instead of a screen-wide map.

use super::Ink;

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

/// A cell's ink as `(sample, ink)`, sorted by sample.
pub(super) type InkList = Vec<(u8, Ink)>;

pub(super) struct DetailSamples {
    slots: Vec<u32>,
    slabs: Vec<Box<[SampleTile]>>,
    /// Ink per tile slot. A released slot's list is emptied, keeping its
    /// capacity for the next cell.
    inks: Vec<InkList>,
    free: Vec<u32>,
}

/// The ink of `sample` in `list`.
#[inline]
pub(super) fn ink_get(list: &[(u8, Ink)], sample: usize) -> Option<&Ink> {
    list.iter().find(|(held, _)| usize::from(*held) == sample).map(|(_, ink)| ink)
}

/// Remove the ink of `sample` from `list`, if any.
#[inline]
pub(super) fn ink_remove(list: &mut InkList, sample: usize) {
    if let Some(at) = list.iter().position(|(held, _)| usize::from(*held) == sample) {
        list.remove(at);
    }
}

/// The ink of `sample` in `list`, inserting `ink()` in order when absent.
pub(super) fn ink_entry(list: &mut InkList, sample: usize, ink: impl FnOnce() -> Ink) -> &mut Ink {
    let at = match list.iter().position(|(held, _)| usize::from(*held) >= sample) {
        Some(at) if usize::from(list[at].0) == sample => at,
        Some(at) => {
            list.insert(at, (sample as u8, ink()));
            at
        }
        None => {
            list.push((sample as u8, ink()));
            list.len() - 1
        }
    };
    &mut list[at].1
}

impl DetailSamples {
    pub(super) fn new(cells: usize) -> Self {
        Self {
            slots: vec![NO_TILE; cells],
            slabs: Vec::new(),
            inks: Vec::new(),
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
        self.ensure_with_ink(cell).0
    }

    /// The cell's ink; empty when it has no tile.
    #[inline]
    pub(super) fn ink(&self, cell: usize) -> &[(u8, Ink)] {
        match self.slots[cell] {
            NO_TILE => &[],
            slot => &self.inks[slot as usize],
        }
    }

    /// The tile of a cell that has one, and its ink.
    #[inline]
    pub(super) fn get_mut_with_ink(&mut self, cell: usize) -> (&mut SampleTile, &mut InkList) {
        let slot = self.slots[cell] as usize;
        (
            &mut self.slabs[slot / SLAB_TILES][slot % SLAB_TILES],
            &mut self.inks[slot],
        )
    }

    /// The cell's tile and ink, first giving it a tile (with no ink).
    pub(super) fn ensure_with_ink(&mut self, cell: usize) -> (&mut SampleTile, &mut InkList) {
        if self.slots[cell] == NO_TILE {
            if self.free.is_empty() {
                self.grow();
            }
            self.slots[cell] = self.free.pop().unwrap();
        }
        self.get_mut_with_ink(cell)
    }

    /// Visit every held ink sample with its tile. Walks the tile slots, not
    /// the screen: a released slot's list is empty, so this costs the text
    /// the screen holds rather than its size.
    pub(super) fn for_each_ink_mut(&mut self, mut visit: impl FnMut(&mut SampleTile, usize, &Ink)) {
        for (slot, ink) in self.inks.iter().enumerate() {
            if ink.is_empty() {
                continue;
            }
            let tile = &mut self.slabs[slot / SLAB_TILES][slot % SLAB_TILES];
            for (sample, held) in ink {
                visit(tile, usize::from(*sample), held);
            }
        }
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
        self.inks.resize_with(start + SLAB_TILES, Vec::new);
        self.free
            .extend((start as u32..(start + SLAB_TILES) as u32).rev());
    }

    pub(super) fn release(&mut self, cell: usize) {
        let slot = std::mem::replace(&mut self.slots[cell], NO_TILE);
        if slot != NO_TILE {
            self.inks[slot as usize].clear();
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
            + self.inks.capacity() * std::mem::size_of::<InkList>()
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
