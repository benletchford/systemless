//! Movie frames described by an ImageDescription and decoded by the ICM codecs.
//! Keep temporal codec state in the existing movie cache, including palette
//! indices/codebooks, rather than treating a delta sample as a standalone image.
use super::*;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct State {
    description: decompression::Description,
    decoder: decompression::Decoder,
}

pub(super) fn decode(
    quicktime: &mut PpcQuickTimeState,
    samples: &PpcQuickTimeVideoSampleTableRecord,
) -> Option<PpcQuickTimeDecodedVideoFrame> {
    let track = quicktime
        .movie_file_tracks
        .iter()
        .position(|track| track.media_type == u32::from_be_bytes(*b"vide"))?
        .checked_add(1)?;
    let description_index = quicktime
        .movie_video_track
        .as_ref()
        .map_or(1, |track| track.sample_description_id);
    let bytes =
        ppc_qt_movie_sample_description(&quicktime.movie_file_data, track, description_index)?;
    let description = decompression::Description::parse(bytes).ok()?;
    if description.codec != samples.codec {
        return None;
    }
    let target = ppc_qt_movie_timed_sample_index(quicktime, samples)?;
    let cached = quicktime.movie_video_decode_cache.as_ref().filter(|cache| {
        cache.codec == samples.codec
            && cache.width == description.width
            && cache.height == description.height
            && cache.rgb.len() == description.width * description.height * 3
            && cache.sample_index <= target
            && cache
                .described_state
                .as_ref()
                .is_some_and(|state| state.description == description)
    });
    if let Some(cache) = cached.filter(|cache| cache.sample_index == target) {
        return Some(PpcQuickTimeDecodedVideoFrame {
            width: cache.width,
            height: cache.height,
            rgb: cache.rgb.clone(),
        });
    }
    let (mut state, start) = if let Some(cache) = cached {
        (
            cache.described_state.clone()?,
            cache.sample_index.checked_add(1)?,
        )
    } else {
        let decoder = decompression::Decoder::new(&description).ok()?;
        (
            State {
                description,
                decoder,
            },
            0,
        )
    };
    let sample_data = quicktime
        .movie_file_sample_fork
        .as_ref()
        .map(|fork| fork.as_slice())
        .unwrap_or(&quicktime.movie_file_data);
    let mut decoded = None;
    for index in start..=target {
        let data = ppc_qt_movie_video_sample_bytes(sample_data, samples, index)?;
        decoded = Some(state.decoder.decode(&state.description, data).ok()?);
    }
    let image = decoded?;
    let rgb: Vec<u8> = image
        .rgba
        .chunks_exact(4)
        .flat_map(|pixel| [pixel[0], pixel[1], pixel[2]])
        .collect();
    let frame = PpcQuickTimeDecodedVideoFrame {
        width: image.width as usize,
        height: image.height as usize,
        rgb: rgb.clone(),
    };
    // Publish only after every intervening sample has decoded successfully.
    // A backward seek rebuilds from the first sample; failed deltas retain the
    // previous valid cache instead of poisoning subsequent redraws.
    quicktime.movie_video_decode_cache = Some(PpcQuickTimeVideoDecodeCacheRecord {
        codec: samples.codec,
        width: frame.width,
        height: frame.height,
        sample_index: target,
        rgb,
        cinepak_strips: Vec::new(),
        described_state: Some(state),
    });
    Some(frame)
}
