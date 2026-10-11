//! Virtual marked text with untouched guest spans retained by source offset.
use std::ops::Range;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct MarkedSpan {
    pub guest: Range<usize>,
    pub text: String,
}

#[derive(Clone, Debug)]
pub(crate) struct MarkedDocument {
    source: Vec<u16>,
    spans: Vec<MarkedSpan>,
}

impl MarkedDocument {
    pub fn new(source: &str) -> Self {
        Self {
            source: source.encode_utf16().collect(),
            spans: Vec::new(),
        }
    }

    pub fn replace_guest(&mut self, guest: Range<usize>, text: &str) -> bool {
        if guest.start > guest.end
            || guest.end > self.source.len()
            || String::from_utf16(&self.source[..guest.start]).is_err()
            || String::from_utf16(&self.source[guest.end..]).is_err()
            || self.spans.iter().any(|span| {
                guest.start < span.guest.end && span.guest.start < guest.end
                    || guest.is_empty() && span.guest.contains(&guest.start)
                    || span.guest.is_empty() && guest.contains(&span.guest.start)
                    || guest == span.guest
            })
        {
            return false;
        }
        self.spans.push(MarkedSpan {
            guest,
            text: text.into(),
        });
        self.spans.sort_by_key(|span| span.guest.start);
        true
    }

    pub fn text(&self) -> Option<String> {
        let mut units = Vec::new();
        let mut start = 0;
        for span in &self.spans {
            units.extend_from_slice(&self.source[start..span.guest.start]);
            units.extend(span.text.encode_utf16());
            start = span.guest.end;
        }
        units.extend_from_slice(&self.source[start..]);
        String::from_utf16(&units).ok()
    }

    pub fn marked_range(&self, guest: &Range<usize>) -> Option<Range<usize>> {
        let (mut guest_start, mut virtual_start) = (0, 0);
        for span in &self.spans {
            let start = virtual_start + span.guest.start - guest_start;
            let end = start + span.text.encode_utf16().count();
            if &span.guest == guest {
                return Some(start..end);
            }
            virtual_start = end;
            guest_start = span.guest.end;
        }
        None
    }

    /// Expand a replacement only across candidates it actually intersects.
    /// Untouched guest text outside the requested range remains independent.
    pub fn replacement_plan(
        &self,
        range: Range<usize>,
        text: &str,
    ) -> Option<(Range<usize>, String, usize, Vec<Range<usize>>)> {
        self.geometry_ranges(range.clone())?;
        let touched: Vec<_> = self
            .spans
            .iter()
            .filter_map(|span| {
                let virtual_range = self.marked_range(&span.guest)?;
                (range.start < virtual_range.end && virtual_range.start < range.end
                    || range.is_empty() && virtual_range.contains(&range.start))
                .then_some((span, virtual_range))
            })
            .collect();
        let (first, first_virtual) = touched.first()?;
        let (last, last_virtual) = touched.last()?;
        let start = range.start.min(first_virtual.start);
        let end = range.end.max(last_virtual.end);
        let guest_start = if range.start < first_virtual.start {
            self.guest_range(range.start..first_virtual.start)?.start
        } else {
            first.guest.start
        };
        let guest_end = if range.end > last_virtual.end {
            self.guest_range(last_virtual.end..range.end)?.end
        } else {
            last.guest.end
        };
        let units: Vec<_> = self.text()?.encode_utf16().collect();
        let prefix = String::from_utf16(units.get(start..range.start)?).ok()?;
        let suffix = String::from_utf16(units.get(range.end..end)?).ok()?;
        let offset = prefix.encode_utf16().count();
        Some((
            guest_start..guest_end,
            prefix + text + &suffix,
            offset,
            touched
                .into_iter()
                .map(|(span, _)| span.guest.clone())
                .collect(),
        ))
    }

    /// Guest glyph positions retain their original ownership; replaced glyphs
    /// have no corresponding surrounding position in the virtual document.
    pub fn virtual_index(&self, index: usize) -> Option<usize> {
        if index > self.source.len() {
            return None;
        }
        let (mut guest_start, mut virtual_start) = (0, 0);
        for span in &self.spans {
            if index < span.guest.start {
                return Some(virtual_start + index - guest_start);
            }
            if index < span.guest.end {
                return None;
            }
            virtual_start += span.guest.start - guest_start + span.text.encode_utf16().count();
            guest_start = span.guest.end;
        }
        Some(virtual_start + index - guest_start)
    }

    /// Split geometry queries at every guest/marked ownership boundary.
    pub fn geometry_ranges(&self, range: Range<usize>) -> Option<Vec<Range<usize>>> {
        let text: Vec<_> = self.text()?.encode_utf16().collect();
        if range.start > range.end
            || range.end > text.len()
            || String::from_utf16(&text[..range.start]).is_err()
            || String::from_utf16(&text[range.end..]).is_err()
        {
            return None;
        }
        if range.is_empty() {
            return Some(vec![range]);
        }
        let mut boundaries = vec![range.start];
        let (mut guest_start, mut virtual_start) = (0, 0);
        for span in &self.spans {
            let start = virtual_start + span.guest.start - guest_start;
            let end = start + span.text.encode_utf16().count();
            for boundary in [start, end] {
                if boundary > range.start
                    && boundary < range.end
                    && boundaries.last() != Some(&boundary)
                {
                    boundaries.push(boundary);
                }
            }
            virtual_start = end;
            guest_start = span.guest.end;
        }
        boundaries.push(range.end);
        Some(boundaries.windows(2).map(|pair| pair[0]..pair[1]).collect())
    }

    /// Only untouched virtual ranges can become independent guest edits.
    /// Marked ranges keep their own Unicode and must be resolved separately.
    pub fn guest_range(&self, virtual_range: Range<usize>) -> Option<Range<usize>> {
        let text: Vec<_> = self.text()?.encode_utf16().collect();
        if virtual_range.start > virtual_range.end
            || virtual_range.end > text.len()
            || String::from_utf16(&text[..virtual_range.start]).is_err()
            || String::from_utf16(&text[virtual_range.end..]).is_err()
        {
            return None;
        }
        let (mut guest_start, mut virtual_start) = (0, 0);
        for span in &self.spans {
            let untouched_end = virtual_start + span.guest.start - guest_start;
            let marked_end = untouched_end + span.text.encode_utf16().count();
            if virtual_range.is_empty()
                && (untouched_end..=marked_end).contains(&virtual_range.start)
            {
                return None;
            }
            if virtual_range.start >= virtual_start && virtual_range.end <= untouched_end {
                return Some(
                    guest_start + virtual_range.start - virtual_start
                        ..guest_start + virtual_range.end - virtual_start,
                );
            }
            virtual_start = untouched_end + span.text.encode_utf16().count();
            guest_start = span.guest.end;
        }
        (virtual_range.start >= virtual_start).then_some(
            guest_start + virtual_range.start.saturating_sub(virtual_start)
                ..guest_start + virtual_range.end.saturating_sub(virtual_start),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn disjoint_unicode_edits_keep_intervening_guest_offsets() {
        let mut document = MarkedDocument::new("abcdef");
        assert!(document.replace_guest(1..2, "😀"));
        assert_eq!(document.guest_range(3..5), Some(2..4));
        assert!(document.replace_guest(4..5, "éΩ"));
        assert_eq!(document.text().as_deref(), Some("a😀cdéΩf"));
        assert_eq!(document.guest_range(3..5), Some(2..4));
        assert_eq!(document.guest_range(7..8), Some(5..6));
        assert_eq!(document.guest_range(1..3), None);
        assert_eq!(document.guest_range(2..2), None);
        assert_eq!(document.guest_range(1..1), None);
        assert_eq!(document.guest_range(3..3), None);
        assert_eq!(document.virtual_index(2), Some(3));
        assert_eq!(document.virtual_index(4), None);
        assert_eq!(document.virtual_index(5), Some(7));
        assert_eq!(
            document.geometry_ranges(0..8),
            Some(vec![0..1, 1..3, 3..5, 5..7, 7..8])
        );
        assert!(document.geometry_ranges(2..5).is_none());
        let before = document.text();
        assert!(!document.replace_guest(0..3, "X"));
        assert_eq!(document.text(), before);
    }
}
