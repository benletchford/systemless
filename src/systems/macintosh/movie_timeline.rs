//! QuickTime edit-list mapping shared by the classic and PowerPC adapters.
//!
//! Edit durations use the movie time scale; media starts use the media time
//! scale. Repeated edits do not duplicate or reorder the compressed samples.
//! https://developer.apple.com/documentation/quicktime-file-format/playing_with_edit_lists

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct TrackEdit {
    pub duration: u64,
    pub media_time: i64,
    /// Signed 16.16 Fixed, including zero for a dwell edit.
    pub rate: i32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct TrackTimeline {
    pub edits: Vec<TrackEdit>,
}

impl TrackTimeline {
    /// Parse an elst payload (the version/flags word through the last entry).
    /// Reject truncated lists instead of inventing zero-valued edit rows.
    pub fn parse(payload: &[u8]) -> Option<Self> {
        let version = *payload.first()?;
        let width = match version {
            0 => 12usize,
            1 => 20,
            _ => return None,
        };
        let count = u32::from_be_bytes(payload.get(4..8)?.try_into().ok()?) as usize;
        let bytes = payload.get(8..8usize.checked_add(count.checked_mul(width)?)?)?;
        let mut edits = Vec::with_capacity(count);
        let mut total = 0u64;
        for row in bytes.chunks_exact(width) {
            let (duration, media_time, rate_offset) = if version == 0 {
                (
                    u64::from(u32::from_be_bytes(row[0..4].try_into().ok()?)),
                    i64::from(i32::from_be_bytes(row[4..8].try_into().ok()?)),
                    8,
                )
            } else {
                (
                    u64::from_be_bytes(row[0..8].try_into().ok()?),
                    i64::from_be_bytes(row[8..16].try_into().ok()?),
                    16,
                )
            };
            // Only -1 denotes an empty edit. Other negative media starts are
            // invalid, rather than offsets that wrap into the end of media.
            if media_time < -1 {
                return None;
            }
            total = total.checked_add(duration)?;
            edits.push(TrackEdit {
                duration,
                media_time,
                rate: i32::from_be_bytes(row[rate_offset..rate_offset + 4].try_into().ok()?),
            });
        }
        Some(Self { edits })
    }

    /// Map a movie time to media time, or None for an empty edit or a time
    /// outside the track. Half-open edit boundaries select the following edit.
    pub fn media_time(&self, time: i64, movie_scale: u32, media_scale: u32) -> Option<u64> {
        let time = u64::try_from(time).ok()?;
        if movie_scale == 0 || media_scale == 0 {
            return None;
        }
        let mut start = 0u64;
        for edit in &self.edits {
            let end = start.checked_add(edit.duration)?;
            if time < end {
                if edit.media_time == -1 {
                    return None;
                }
                // i128 covers every u64 duration times a u32 scale times a
                // signed Fixed rate without floating point or clock changes.
                let numerator =
                    i128::from(time - start) * i128::from(media_scale) * i128::from(edit.rate);
                let denominator = i128::from(movie_scale) * 65_536;
                let media = i128::from(edit.media_time) + numerator.div_euclid(denominator);
                return u64::try_from(media).ok();
            }
            start = end;
        }
        None
    }
}

/// A track without an edit list starts at media time zero at its natural rate.
pub(crate) fn media_time(
    timeline: Option<&TrackTimeline>,
    time: i64,
    movie_scale: u32,
    media_scale: u32,
) -> Option<u64> {
    if let Some(timeline) = timeline {
        return timeline.media_time(time, movie_scale, media_scale);
    }
    if time < 0 || movie_scale == 0 || media_scale == 0 {
        return None;
    }
    u64::try_from(i128::from(time) * i128::from(media_scale) / i128::from(movie_scale)).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn list(rows: &[(u32, i32, i32)]) -> TrackTimeline {
        let mut payload = vec![0; 4];
        payload.extend_from_slice(&(rows.len() as u32).to_be_bytes());
        for (duration, media, rate) in rows {
            payload.extend_from_slice(&duration.to_be_bytes());
            payload.extend_from_slice(&media.to_be_bytes());
            payload.extend_from_slice(&rate.to_be_bytes());
        }
        TrackTimeline::parse(&payload).unwrap()
    }

    #[test]
    fn logo_repeated_edits_hold_first_sample_until_four_seconds() {
        // Container metadata of the original Logo resource, independently
        // checked against FFmpeg output PTS. No copyrighted media is embedded.
        let mut rows = vec![(300, 0, 65_536); 7];
        rows.push((600, 0, 65_536));
        rows.extend([(300, 300, 65_536); 3]);
        let timeline = list(&rows);
        for t in 0..3600 {
            let media = timeline.media_time(t, 600, 600).unwrap();
            assert_eq!(media / 300, u64::from(t >= 2400), "movie time {t}");
        }
        assert_eq!(timeline.media_time(3600, 600, 600), None);
        // Backward seeks and repeated segments have no hidden cursor state.
        assert_eq!(timeline.media_time(2400, 600, 600), Some(300));
        assert_eq!(timeline.media_time(299, 600, 600), Some(299));
        assert_eq!(timeline.media_time(300, 600, 600), Some(0));
    }

    #[test]
    fn empty_offset_scaled_fast_slow_dwell_and_reverse_edits() {
        let timeline = list(&[
            (600, -1, 65_536),
            (600, 200, 131_072),
            (600, 50, 32_768),
            (300, 123, 0),
            (600, 300, -65_536),
        ]);
        assert_eq!(timeline.media_time(-1, 600, 100), None);
        assert_eq!(timeline.media_time(599, 600, 100), None);
        assert_eq!(timeline.media_time(600, 600, 100), Some(200));
        assert_eq!(timeline.media_time(900, 600, 100), Some(300));
        assert_eq!(timeline.media_time(1200, 600, 100), Some(50));
        assert_eq!(timeline.media_time(1500, 600, 100), Some(75));
        assert_eq!(timeline.media_time(1800, 600, 100), Some(123));
        assert_eq!(timeline.media_time(2099, 600, 100), Some(123));
        assert_eq!(timeline.media_time(2100, 600, 100), Some(300));
        assert_eq!(timeline.media_time(2101, 600, 100), Some(299));
        assert_eq!(timeline.media_time(2400, 600, 100), Some(250));
        assert_eq!(timeline.media_time(2700, 600, 100), None);
        assert_eq!(timeline.media_time(0, 0, 100), None);
    }

    #[test]
    fn absent_and_empty_lists_have_different_meanings() {
        assert_eq!(media_time(None, 1200, 600, 100), Some(200));
        assert_eq!(media_time(Some(&list(&[])), 0, 600, 100), None);
        assert_eq!(media_time(None, -1, 600, 100), None);
        assert_eq!(media_time(None, 0, 600, 0), None);
        let timeline = list(&[(0, 99, 65_536), (600, 10, 65_536)]);
        assert_eq!(timeline.media_time(0, 600, 100), Some(10));
    }

    #[test]
    fn version_one_preserves_large_times_and_rejects_overflow_and_truncation() {
        let duration = u64::from(u32::MAX) + 100;
        let mut payload = vec![1, 0, 0, 0, 0, 0, 0, 1];
        payload.extend_from_slice(&duration.to_be_bytes());
        payload.extend_from_slice(&5_000_000_000i64.to_be_bytes());
        payload.extend_from_slice(&65_536i32.to_be_bytes());
        let timeline = TrackTimeline::parse(&payload).unwrap();
        assert_eq!(timeline.media_time(600, 600, 600), Some(5_000_000_600));
        for len in 0..payload.len() {
            assert!(TrackTimeline::parse(&payload[..len]).is_none());
        }
        payload[0] = 2;
        assert!(TrackTimeline::parse(&payload).is_none());
        payload[0] = 1;
        payload[7] = 2;
        let mut row = u64::MAX.to_be_bytes().to_vec();
        row.extend_from_slice(&0i64.to_be_bytes());
        row.extend_from_slice(&65_536i32.to_be_bytes());
        payload.extend_from_slice(&row);
        assert!(TrackTimeline::parse(&payload).is_none());
    }
}
