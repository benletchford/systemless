//! A cheap hasher for maps keyed by guest handles, pointers and small ids.
//!
//! The standard `RandomState` (SipHash) is built to resist hash flooding,
//! which none of these internal maps face; on the trap dispatcher's per-call
//! lookups and the execution kernel's task maps it showed up at several
//! percent of EV Override's main thread. Iteration order was already
//! unspecified under `RandomState`, so a fixed hasher cannot change behavior.

use std::hash::{BuildHasherDefault, Hasher};

#[derive(Default, Clone, Copy)]
pub struct IdHasher(u64);

impl Hasher for IdHasher {
    #[inline]
    fn finish(&self) -> u64 {
        self.0
    }

    #[inline]
    fn write(&mut self, bytes: &[u8]) {
        for &byte in bytes {
            self.0 = (self.0 ^ u64::from(byte)).wrapping_mul(0x0000_0100_0000_01b3);
        }
    }

    #[inline]
    fn write_u8(&mut self, value: u8) {
        self.write_u64(u64::from(value));
    }

    #[inline]
    fn write_u16(&mut self, value: u16) {
        self.write_u64(u64::from(value));
    }

    #[inline]
    fn write_u32(&mut self, value: u32) {
        self.write_u64(u64::from(value));
    }

    #[inline]
    fn write_u64(&mut self, value: u64) {
        let mixed = (value ^ self.0.rotate_left(29)).wrapping_mul(0x9E37_79B9_7F4A_7C15);
        self.0 = mixed ^ (mixed >> 32);
    }

    #[inline]
    fn write_usize(&mut self, value: usize) {
        self.write_u64(value as u64);
    }

    #[inline]
    fn write_i16(&mut self, value: i16) {
        self.write_u16(value as u16);
    }

    #[inline]
    fn write_i32(&mut self, value: i32) {
        self.write_u32(value as u32);
    }
}

pub(crate) type FastHashMap<K, V> = std::collections::HashMap<K, V, BuildHasherDefault<IdHasher>>;
pub(crate) type FastHashSet<K> = std::collections::HashSet<K, BuildHasherDefault<IdHasher>>;
