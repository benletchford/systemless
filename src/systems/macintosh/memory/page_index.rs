//! A conservative page filter over sets of guest addresses.
//!
//! Several hot paths ask "could this access touch one of the ranges I am
//! tracking?" for every guest access, behind answers that are linear in the
//! number of tracked ranges. An envelope alone cannot reject an address that
//! falls between two distant ranges, which is the normal case: the ranges are
//! scattered through the same heap the access walks. One bit per page is what
//! keeps those questions constant-time.

/// Page granularity of [`PageIndex`]. The ranges these filters track are
/// page-aligned in practice, so 4 KiB keeps the filter precise without
/// making the bitmap larger than a CPU cache can hold.
pub(crate) const PAGE_SHIFT: u32 = 12;
pub(crate) const PAGE_COUNT: usize = 1 << (32 - PAGE_SHIFT);
const INDEX_WORDS: usize = PAGE_COUNT / 64;

/// A conservative page filter over a set of guest ranges.
///
/// One bit per 4 KiB page records whether a tracked range touches it. A clear
/// bit is a proof of no overlap; a set bit only means the authoritative
/// structure must be consulted, so a filter may over-report — dropping a
/// range need not clear its bit.
///
/// The bitmap is allocated on the first marked range: a filter that never
/// receives one stays at its default empty state.
#[derive(Debug, Default, Clone)]
pub(crate) struct PageIndex {
    words: Option<Box<[u64]>>,
}

impl PageIndex {
    /// Record that `start..end` holds a tracked range.
    pub(crate) fn mark(&mut self, start: u64, end: u64) {
        if end <= start {
            return;
        }
        let words = self
            .words
            .get_or_insert_with(|| vec![0u64; INDEX_WORDS].into_boxed_slice());
        let first = (start >> PAGE_SHIFT) as usize;
        let last = ((end - 1) >> PAGE_SHIFT) as usize;
        let last = last.min(PAGE_COUNT - 1);
        for page in first..=last {
            words[page / 64] |= 1u64 << (page % 64);
        }
    }

    /// Whether any page of `start..end` may hold a tracked range.
    #[inline]
    pub(crate) fn may_overlap(&self, start: u64, end: u64) -> bool {
        let Some(words) = self.words.as_deref() else {
            return false;
        };
        if end <= start {
            return false;
        }
        let first = (start >> PAGE_SHIFT) as usize;
        let last = (((end - 1) >> PAGE_SHIFT) as usize).min(PAGE_COUNT - 1);
        let (first_word, last_word) = (first / 64, last / 64);
        // Masks select the bits of the first and last word that the range
        // actually reaches. Both shift amounts stay below 64.
        let head_mask = u64::MAX << (first % 64);
        let tail_mask = u64::MAX >> (63 - last % 64);
        if first_word == last_word {
            // The overwhelmingly common case: a scalar access inside one page.
            return words[first_word] & head_mask & tail_mask != 0;
        }
        words[first_word] & head_mask != 0
            || words[last_word] & tail_mask != 0
            || words[first_word + 1..last_word].iter().any(|word| *word != 0)
    }
}

#[cfg(test)]
mod tests {
    use super::{PageIndex, PAGE_SHIFT};

    const PAGE: u64 = 1 << PAGE_SHIFT;

    #[test]
    fn empty_index_rejects_every_range() {
        let index = PageIndex::default();
        assert!(!index.may_overlap(0, 1));
        assert!(!index.may_overlap(0, 1 << 32));
    }

    #[test]
    fn page_index_reports_marked_pages_only() {
        let mut index = PageIndex::default();
        index.mark(8 * PAGE, 8 * PAGE + 16);
        // The whole page containing the mapping answers yes, since the filter
        // is page-granular and deliberately conservative.
        assert!(index.may_overlap(8 * PAGE, 8 * PAGE + 1));
        assert!(index.may_overlap(8 * PAGE + 4000, 8 * PAGE + 4004));
        assert!(!index.may_overlap(7 * PAGE, 8 * PAGE));
        assert!(!index.may_overlap(9 * PAGE, 9 * PAGE + 4));
        // A range straddling the boundary still sees the marked page.
        assert!(index.may_overlap(7 * PAGE, 8 * PAGE + 1));
        assert!(index.may_overlap(8 * PAGE + 4095, 9 * PAGE + 1));
    }

    #[test]
    fn page_index_spans_word_boundaries() {
        let mut index = PageIndex::default();
        // Page 200 sits in word 3; probe ranges whose head, tail, and interior
        // words are each the only place the bit could be found.
        index.mark(200 * PAGE, 201 * PAGE);
        assert!(index.may_overlap(200 * PAGE, 200 * PAGE + 4));
        assert!(index.may_overlap(0, 1 << 32));
        assert!(index.may_overlap(199 * PAGE, 400 * PAGE));
        assert!(index.may_overlap(64 * PAGE, 201 * PAGE));
        assert!(index.may_overlap(200 * PAGE, 4096 * PAGE));
        assert!(!index.may_overlap(201 * PAGE, 400 * PAGE));
        assert!(!index.may_overlap(0, 200 * PAGE));
    }

    #[test]
    fn page_index_clamps_the_top_of_the_address_space() {
        let mut index = PageIndex::default();
        // A mapping whose length runs past the end of the address space must
        // mark the last page rather than index out of bounds.
        index.mark((1 << 32) - PAGE, (1 << 32) + 4096);
        assert!(index.may_overlap((1 << 32) - 4, 1 << 32));
        assert!(!index.may_overlap(0, (1 << 32) - PAGE));
    }

    #[test]
    fn an_empty_range_allocates_no_bitmap() {
        let mut index = PageIndex::default();
        index.mark(16 * PAGE, 16 * PAGE);
        assert!(index.words.is_none());
    }
}
