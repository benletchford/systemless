//! Partial-coverage ink stored inline with each cell.
//!
//! Antialiased text leaves ink on a cell's edge samples: a foreground index,
//! a coverage alpha and the background it was drawn over. In practice the
//! background is one palette index and alpha fits a byte, so each sample's
//! ink packs into four bytes and a cell's ink is a fixed block beside its
//! samples, with no heap list to chase or free per cell. A background that
//! is itself a blend (text drawn over text of another colour, or a transfer
//! mode combining two texts) is rare; such ink lives whole in a side table
//! owned by the store, keyed by the cell's slot and sample.

use super::{IndexedColor, Ink, TILE_SAMPLES};
use std::collections::HashMap;

/// The ink of one sample: foreground, alpha and solid background indices,
/// or (with `COMPLEX` set) a marker that the ink lives in the side table.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct Packed(u32);

const COMPLEX: u32 = 1 << 24;

impl Packed {
    /// The packed form of `ink`, or `None` if it needs the side table.
    fn of(ink: &Ink) -> Option<Self> {
        match ink.background {
            IndexedColor::Solid(background) if ink.alpha <= 255 => Some(Self(
                u32::from(ink.foreground) | ink.alpha << 8 | u32::from(background) << 16,
            )),
            _ => None,
        }
    }

    fn complex(self) -> bool {
        self.0 & COMPLEX != 0
    }

    fn ink(self) -> Ink {
        Ink {
            foreground: self.0 as u8,
            alpha: (self.0 >> 8) & 0xFF,
            background: IndexedColor::Solid((self.0 >> 16) as u8),
        }
    }
}

/// Side table for ink that does not pack, keyed by (slot, sample).
pub(super) type ComplexInk = HashMap<(u32, u8), Ink>;

/// One cell's ink: which samples have ink, and their packed values.
#[derive(Clone, Copy, Debug, Default)]
pub(super) struct CellInk {
    present: u16,
    complex: u16,
    packed: [Packed; TILE_SAMPLES],
}

impl CellInk {
    pub(super) const EMPTY: Self = Self {
        present: 0,
        complex: 0,
        packed: [Packed(0); TILE_SAMPLES],
    };

    pub(super) fn mask(&self) -> u16 {
        self.present
    }

    pub(super) fn is_empty(&self) -> bool {
        self.present == 0
    }
}

/// Read access to one cell's ink.
#[derive(Clone, Copy)]
pub(super) struct InkView<'a> {
    cell: &'a CellInk,
    complex: &'a ComplexInk,
    slot: u32,
}

impl<'a> InkView<'a> {
    pub(super) fn new(cell: &'a CellInk, complex: &'a ComplexInk, slot: u32) -> Self {
        Self {
            cell,
            complex,
            slot,
        }
    }

    /// Presence bits, one per sample.
    pub(super) fn mask(&self) -> u16 {
        self.cell.present
    }

    pub(super) fn is_empty(&self) -> bool {
        self.cell.present == 0
    }

    pub(super) fn len(&self) -> usize {
        self.cell.present.count_ones() as usize
    }

    /// The ink of `sample`, if any.
    pub(super) fn get(&self, sample: usize) -> Option<Ink> {
        (self.cell.present & (1 << sample) != 0).then(|| self.at(sample))
    }

    fn at(&self, sample: usize) -> Ink {
        let packed = self.cell.packed[sample];
        if packed.complex() {
            self.complex[&(self.slot, sample as u8)].clone()
        } else {
            packed.ink()
        }
    }

    /// Each inked sample and its ink, in sample order.
    pub(super) fn iter(&self) -> impl Iterator<Item = (usize, Ink)> + 'a {
        let view = *self;
        Bits(self.cell.present).map(move |sample| (sample, view.at(sample)))
    }

    /// Whether this is exactly the ink `list` (sorted by sample) describes.
    pub(super) fn eq_list(&self, list: &[(u8, Ink)]) -> bool {
        self.len() == list.len()
            && list.iter().all(|(sample, ink)| {
                let sample = usize::from(*sample);
                self.cell.present & (1 << sample) != 0
                    && match Packed::of(ink) {
                        Some(packed) => self.cell.packed[sample] == packed,
                        None => self.cell.packed[sample].complex() && self.at(sample) == *ink,
                    }
            })
    }
}

/// Mutable access to one cell's ink.
pub(super) struct InkViewMut<'a> {
    cell: &'a mut CellInk,
    complex: &'a mut ComplexInk,
    slot: u32,
}

impl<'a> InkViewMut<'a> {
    pub(super) fn new(cell: &'a mut CellInk, complex: &'a mut ComplexInk, slot: u32) -> Self {
        Self {
            cell,
            complex,
            slot,
        }
    }

    pub(super) fn view(&self) -> InkView<'_> {
        InkView {
            cell: self.cell,
            complex: self.complex,
            slot: self.slot,
        }
    }

    pub(super) fn mask(&self) -> u16 {
        self.cell.present
    }

    /// Remove every sample's ink.
    pub(super) fn clear(&mut self) {
        if self.cell.complex != 0 {
            for sample in Bits(self.cell.complex) {
                self.complex.remove(&(self.slot, sample as u8));
            }
        }
        self.cell.present = 0;
        self.cell.complex = 0;
    }

    pub(super) fn remove(&mut self, sample: usize) {
        let bit = 1 << sample;
        if self.cell.complex & bit != 0 {
            self.complex.remove(&(self.slot, sample as u8));
            self.cell.complex &= !bit;
        }
        self.cell.present &= !bit;
    }

    pub(super) fn set(&mut self, sample: usize, ink: Ink) {
        let bit = 1 << sample;
        self.cell.present |= bit;
        match Packed::of(&ink) {
            Some(packed) => {
                if self.cell.complex & bit != 0 {
                    self.complex.remove(&(self.slot, sample as u8));
                    self.cell.complex &= !bit;
                }
                self.cell.packed[sample] = packed;
            }
            None => {
                self.cell.complex |= bit;
                self.cell.packed[sample] = Packed(COMPLEX);
                self.complex.insert((self.slot, sample as u8), ink);
            }
        }
    }

    /// Apply `change` to the ink of `sample`, starting from `ink()` when it
    /// has none.
    pub(super) fn update(
        &mut self,
        sample: usize,
        ink: impl FnOnce() -> Ink,
        change: impl FnOnce(&mut Ink),
    ) {
        let mut held = self.view().get(sample).unwrap_or_else(ink);
        change(&mut held);
        self.set(sample, held);
    }
}

/// The set bits of a mask, lowest first.
struct Bits(u16);

impl Iterator for Bits {
    type Item = usize;

    fn next(&mut self) -> Option<usize> {
        if self.0 == 0 {
            return None;
        }
        let bit = self.0.trailing_zeros() as usize;
        self.0 &= self.0 - 1;
        Some(bit)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn solid(foreground: u8, alpha: u32, background: u8) -> Ink {
        Ink {
            foreground,
            alpha,
            background: IndexedColor::Solid(background),
        }
    }

    #[test]
    fn packed_and_complex_ink_round_trip() {
        let mut cell = CellInk::default();
        let mut complex = ComplexInk::new();
        let blended = Ink {
            foreground: 4,
            alpha: 90,
            background: IndexedColor::Solid(2).over(7, 128),
        };
        {
            let mut ink = InkViewMut::new(&mut cell, &mut complex, 3);
            ink.set(5, solid(1, 200, 9));
            ink.set(0, blended.clone());
            ink.update(5, || unreachable!(), |ink| ink.alpha = 255);
            ink.update(7, || solid(6, 0, 3), |ink| ink.alpha = 40);
        }
        let view = InkView::new(&cell, &complex, 3);
        assert_eq!(view.mask(), 0b1010_0001);
        let listed: Vec<(usize, Ink)> = view.iter().collect();
        assert_eq!(
            listed,
            vec![
                (0, blended.clone()),
                (5, solid(1, 255, 9)),
                (7, solid(6, 40, 3))
            ]
        );
        assert!(view.eq_list(&[
            (0, blended.clone()),
            (5, solid(1, 255, 9)),
            (7, solid(6, 40, 3))
        ]));
        assert!(!view.eq_list(&[(0, blended), (5, solid(1, 255, 9))]));
        {
            let mut ink = InkViewMut::new(&mut cell, &mut complex, 3);
            ink.set(0, solid(2, 3, 4));
            assert!(complex.is_empty(), "replacing complex ink drops its entry");
        }
        let mut ink = InkViewMut::new(&mut cell, &mut complex, 3);
        ink.set(
            1,
            Ink {
                foreground: 1,
                alpha: 1,
                background: IndexedColor::Solid(1).over(2, 3),
            },
        );
        ink.clear();
        assert!(cell.is_empty() && complex.is_empty());
    }
}
