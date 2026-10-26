//! Movie Toolbox queries and setters, matrix utilities, media handler
//! calls, and standard movie controller state for the single-movie model.
//!
//! Signatures follow the Universal Interfaces Movies.h and
//! ImageCompression.h headers; behaviour follows Inside Macintosh:
//! QuickTime and Inside Macintosh: QuickTime Components.

use super::super::{
    ppc_point_in_region_storage, ppc_process_new_rgn, ppc_region_combine_rows,
    ppc_region_rows_for_band, ppc_region_storage, ppc_region_storage_bbox,
    ppc_region_storage_from_rows, ppc_resolve_callback_target, ppc_write_pstring_bytes,
    PpcRegionBooleanOp, PPC_MAIN_GDEVICE,
};
use super::*;

/// invalidTrack (MacErrors.h).
const INVALID_TRACK: i16 = -2009;
/// invalidMovie (MacErrors.h).
const INVALID_MOVIE: i16 = -2010;
/// invalidMedia (MacErrors.h).
const INVALID_MEDIA: i16 = -2008;
/// badTrackIndex (MacErrors.h).
const BAD_TRACK_INDEX: i16 = -2028;
/// noCodecErr (MacErrors.h): no decompressor is available for the data.
const NO_CODEC_ERR: i16 = -8961;
/// badComponentInstance (Components.h), returned as a ComponentResult.
const BAD_COMPONENT_INSTANCE: u32 = 0x8000_8001;
/// badComponentSelector (Components.h), returned as a ComponentResult.
const BAD_COMPONENT_SELECTOR: u32 = 0x8000_8002;

/// The user data reference GetMovieUserData hands out for the movie's udta.
pub(crate) const PPC_QT_MOVIE_USER_DATA: u32 = PPC_QT_MOVIE + 0x20;
/// Offset from a track reference to its track user data reference.
const TRACK_USER_DATA_OFFSET: u32 = 2;
/// Offset from a track reference to its media handler instance.
const TRACK_MEDIA_HANDLER_OFFSET: u32 = 3;

const FIXED_ONE: f64 = 65_536.0;
const FRACT_ONE: f64 = 1_073_741_824.0;
const IDENTITY_MATRIX: [u32; 9] = [0x0001_0000, 0, 0, 0, 0x0001_0000, 0, 0, 0, 0x4000_0000];

type Rect = (i16, i16, i16, i16);

/// Header fields a trak atom contributes to its track record.
pub(crate) struct PpcQuickTimeTrackDetails {
    pub(crate) duration: u64,
    pub(crate) offset: u64,
    pub(crate) layer: i16,
    pub(crate) volume: i16,
    pub(crate) matrix: [u32; 9],
    pub(crate) width: u32,
    pub(crate) height: u32,
}

/// Reads the tkhd header and the leading empty edits of a trak atom.
/// QuickTime File Format: tkhd stores duration, layer, volume, matrix and
/// the Fixed track dimensions; an elst entry whose media time is -1 is an
/// empty edit that delays the start of the track.
pub(crate) fn ppc_qt_track_details(
    data: &[u8],
    start: usize,
    end: usize,
) -> PpcQuickTimeTrackDetails {
    let mut details = PpcQuickTimeTrackDetails {
        duration: 0,
        offset: 0,
        layer: 0,
        volume: 0,
        matrix: IDENTITY_MATRIX,
        width: 0,
        height: 0,
    };
    if let Some((content, atom_end)) = ppc_qt_child_atom_range(data, start, end, b"tkhd") {
        let read_u32 = |offset: usize| -> Option<u32> {
            (offset.checked_add(4)? <= atom_end)
                .then(|| data.get(offset..offset + 4))
                .flatten()
                .map(|bytes| u32::from_be_bytes(bytes.try_into().unwrap()))
        };
        let (duration, base) = match data.get(content) {
            Some(0) => (read_u32(content + 20).map(u64::from), content + 32),
            Some(1) => (
                read_u32(content + 28)
                    .zip(read_u32(content + 32))
                    .map(|(high, low)| (u64::from(high) << 32) | u64::from(low)),
                content + 44,
            ),
            _ => (None, usize::MAX - 128),
        };
        details.duration = duration.unwrap_or(0);
        if let Some(word) = read_u32(base) {
            details.layer = (word >> 16) as i16;
        }
        if let Some(word) = read_u32(base + 4) {
            details.volume = (word >> 16) as i16;
        }
        for (index, value) in details.matrix.iter_mut().enumerate() {
            if let Some(word) = read_u32(base + 8 + index * 4) {
                *value = word;
            }
        }
        details.width = read_u32(base + 44).unwrap_or(0);
        details.height = read_u32(base + 48).unwrap_or(0);
    }
    if let Some((offset, _)) = ppc_qt_track_edit_start(data, start, end) {
        details.offset = offset;
    }
    details
}

/// Returns the leading empty-edit time (movie scale) and the media time the
/// first real edit begins at.
fn ppc_qt_track_edit_start(data: &[u8], start: usize, end: usize) -> Option<(u64, u64)> {
    let (edts_start, edts_end) = ppc_qt_child_atom_range(data, start, end, b"edts")?;
    let (elst_start, elst_end) = ppc_qt_child_atom_range(data, edts_start, edts_end, b"elst")?;
    let count = u32::from_be_bytes(data.get(elst_start + 4..elst_start + 8)?.try_into().ok()?);
    let mut offset = 0u64;
    let mut entry = elst_start.checked_add(8)?;
    for _ in 0..count.min(4096) {
        if entry.checked_add(12)? > elst_end {
            break;
        }
        let duration = u32::from_be_bytes(data.get(entry..entry + 4)?.try_into().ok()?);
        let media_time = i32::from_be_bytes(data.get(entry + 4..entry + 8)?.try_into().ok()?);
        if media_time != -1 {
            return Some((offset, u64::from(media_time.max(0) as u32)));
        }
        offset = offset.saturating_add(u64::from(duration));
        entry += 12;
    }
    Some((offset, 0))
}

/// Sample timing and sizes of one track's media.
struct PpcQuickTimeTrackSamples {
    media_time_scale: u32,
    media_start: u64,
    durations: Vec<u32>,
    sizes: Vec<u32>,
    sync: Option<Vec<u32>>,
}

/// Finds the trak atom for a 1-based track ordinal, counting only tracks
/// that `ppc_qt_movie_tracks` records.
fn ppc_qt_track_atom(data: &[u8], ordinal: usize) -> Option<(usize, usize)> {
    let (moov_start, moov_end) = ppc_qt_child_atom_range(data, 0, data.len(), b"moov")?;
    let mut offset = moov_start;
    let mut number = 0;
    while offset.checked_add(8)? <= moov_end {
        let (kind, track_start, track_end) = ppc_qt_atom_range(data, offset, moov_end)?;
        if kind == b"trak" && ppc_qt_track_metadata(data, track_start, track_end).is_some() {
            number += 1;
            if number == ordinal {
                return Some((track_start, track_end));
            }
        }
        offset = track_end;
    }
    None
}

fn ppc_qt_track_samples(data: &[u8], ordinal: usize) -> Option<PpcQuickTimeTrackSamples> {
    let (track_start, track_end) = ppc_qt_track_atom(data, ordinal)?;
    let (mdia_start, mdia_end) = ppc_qt_child_atom_range(data, track_start, track_end, b"mdia")?;
    let (mdhd_start, mdhd_end) = ppc_qt_child_atom_range(data, mdia_start, mdia_end, b"mdhd")?;
    let (media_time_scale, _) = ppc_qt_mvhd_duration(data, mdhd_start, mdhd_end)?;
    let (minf_start, minf_end) = ppc_qt_child_atom_range(data, mdia_start, mdia_end, b"minf")?;
    let (stbl_start, stbl_end) = ppc_qt_child_atom_range(data, minf_start, minf_end, b"stbl")?;
    let (stts_start, stts_end) = ppc_qt_child_atom_range(data, stbl_start, stbl_end, b"stts")?;
    let durations = ppc_qt_stts_sample_durations(data, stts_start, stts_end)?;
    let sizes = ppc_qt_child_atom_range(data, stbl_start, stbl_end, b"stsz")
        .and_then(|(start, end)| ppc_qt_stsz_sample_sizes(data, start, end))
        .unwrap_or_default();
    let sync =
        ppc_qt_child_atom_range(data, stbl_start, stbl_end, b"stss").and_then(|(start, end)| {
            let count = u32::from_be_bytes(data.get(start + 4..start + 8)?.try_into().ok()?);
            (0..count.min(1_000_000) as usize)
                .map(|index| {
                    let entry = start + 8 + index * 4;
                    (entry + 4 <= end)
                        .then(|| data.get(entry..entry + 4))
                        .flatten()
                        .map(|bytes| u32::from_be_bytes(bytes.try_into().unwrap()))
                })
                .collect::<Option<Vec<u32>>>()
        });
    let media_start = ppc_qt_track_edit_start(data, track_start, track_end)
        .map_or(0, |(_, media_start)| media_start);
    Some(PpcQuickTimeTrackSamples {
        media_time_scale,
        media_start,
        durations,
        sizes,
        sync,
    })
}

/// One media sample expressed in the movie's creation time scale.
struct PpcQuickTimeMovieSample {
    start: i64,
    duration: i64,
    size: u32,
    sync: bool,
}

fn ppc_qt_track_movie_samples(
    quicktime: &PpcQuickTimeState,
    ordinal: usize,
    track: &PpcQuickTimeTrackRecord,
) -> Vec<PpcQuickTimeMovieSample> {
    let Some(samples) = ppc_qt_track_samples(&quicktime.movie_file_data, ordinal) else {
        return Vec::new();
    };
    let movie_scale = i128::from(ppc_qt_creation_time_scale(quicktime));
    let media_scale = i128::from(samples.media_time_scale.max(1));
    let to_movie = |media: i128| -> i64 {
        let movie = i128::from(track.offset) + (media * movie_scale) / media_scale;
        movie.clamp(i128::from(i64::MIN), i128::from(i64::MAX)) as i64
    };
    let mut media_time = 0i128;
    let mut result = Vec::with_capacity(samples.durations.len());
    for (index, duration) in samples.durations.iter().enumerate() {
        let relative = media_time - i128::from(samples.media_start);
        let end = relative + i128::from(*duration);
        media_time += i128::from(*duration);
        if end <= 0 {
            continue;
        }
        let start = to_movie(relative.max(0));
        let number = index as u32 + 1;
        result.push(PpcQuickTimeMovieSample {
            start,
            duration: to_movie(end) - start,
            size: samples.sizes.get(index).copied().unwrap_or(0),
            sync: samples
                .sync
                .as_ref()
                .map_or(true, |sync| sync.contains(&number)),
        });
    }
    result
}

/// The time scale the current movie was created with; stored track and
/// movie times are kept in this scale.
pub(crate) fn ppc_qt_creation_time_scale(quicktime: &PpcQuickTimeState) -> u32 {
    quicktime.movie_creation_time_scale.max(1)
}

/// Converts a creation-scale time into the movie's current time scale.
pub(crate) fn ppc_qt_to_movie_scale(quicktime: &PpcQuickTimeState, value: i64) -> i64 {
    let scaled = i128::from(value) * i128::from(quicktime.movie_time_scale.max(1))
        / i128::from(ppc_qt_creation_time_scale(quicktime));
    scaled.clamp(i128::from(i64::MIN), i128::from(i64::MAX)) as i64
}

/// Converts a time in the movie's current time scale to the creation scale.
pub(crate) fn ppc_qt_from_movie_scale(quicktime: &PpcQuickTimeState, value: i64) -> i64 {
    let scaled = i128::from(value) * i128::from(ppc_qt_creation_time_scale(quicktime))
        / i128::from(quicktime.movie_time_scale.max(1));
    scaled.clamp(i128::from(i64::MIN), i128::from(i64::MAX)) as i64
}

/// Clears the per-movie Movie Toolbox and controller state when a new movie
/// replaces the single modelled movie.
pub(crate) fn ppc_qt_reset_movie_toolbox_state(quicktime: &mut PpcQuickTimeState) {
    quicktime.movie_creation_time_scale = quicktime.movie_time_scale.max(1);
    quicktime.movie_matrix = None;
    quicktime.movie_display_clip = None;
    quicktime.movie_drawing_complete = None;
    quicktime.movie_controller_clips.clear();
    quicktime.detached_movie_controllers.clear();
}

/// Queues the movie's drawing-complete procedure after MoviesTask or a
/// controller idle. Movies.h: movieDrawingCallWhenChanged (0) calls the
/// procedure only when the movie drew; movieDrawingCallAlways (1) calls it
/// on every task.
pub(crate) fn ppc_qt_mark_drawing_complete(quicktime: &mut PpcQuickTimeState, drew: bool) {
    if let Some((flags, registration)) = quicktime.movie_drawing_complete {
        if drew || flags & 1 != 0 {
            quicktime.callbacks.queue(PPC_QT_MOVIE, registration);
        }
    }
}

/// Byte range of a user data list: the movie's or a track's udta atom.
pub(crate) fn ppc_qt_user_data_range(
    quicktime: &PpcQuickTimeState,
    user_data: u32,
) -> Option<(usize, usize)> {
    if quicktime.movie_disposed {
        return None;
    }
    let data = &quicktime.movie_file_data;
    if user_data == PPC_QT_MOVIE_USER_DATA {
        let (moov_start, moov_end) = ppc_qt_child_atom_range(data, 0, data.len(), b"moov")?;
        return ppc_qt_child_atom_range(data, moov_start, moov_end, b"udta");
    }
    let ordinal = quicktime
        .movie_tracks
        .iter()
        .position(|track| track.handle + TRACK_USER_DATA_OFFSET == user_data)?
        + 1;
    let (track_start, track_end) = ppc_qt_track_atom(data, ordinal)?;
    ppc_qt_child_atom_range(data, track_start, track_end, b"udta")
}

/// The indexed item of one type in a user data list.
pub(crate) fn ppc_qt_user_data_item(
    data: &[u8],
    (mut offset, end): (usize, usize),
    kind: u32,
    index: u32,
) -> Option<&[u8]> {
    let mut matching = 0;
    while offset + 8 <= end {
        let (item_kind, content_start, item_end) = ppc_qt_atom_range(data, offset, end)?;
        if item_kind == kind.to_be_bytes() {
            matching += 1;
            if matching == index {
                return data.get(content_start..item_end);
            }
        }
        offset = item_end;
    }
    None
}

fn ppc_qt_count_user_data_items(data: &[u8], (mut offset, end): (usize, usize), kind: u32) -> u32 {
    let mut count = 0;
    while offset + 8 <= end {
        let Some((item_kind, _, item_end)) = ppc_qt_atom_range(data, offset, end) else {
            break;
        };
        if item_kind == kind.to_be_bytes() {
            count += 1;
        }
        offset = item_end;
    }
    count
}

fn matrix_to_f64(matrix: &[u32; 9]) -> [f64; 9] {
    std::array::from_fn(|index| {
        let value = f64::from(matrix[index] as i32);
        if index % 3 == 2 {
            value / FRACT_ONE
        } else {
            value / FIXED_ONE
        }
    })
}

fn matrix_from_f64(matrix: &[f64; 9]) -> Option<[u32; 9]> {
    let mut result = [0u32; 9];
    for (index, value) in matrix.iter().enumerate() {
        let one = if index % 3 == 2 { FRACT_ONE } else { FIXED_ONE };
        let scaled = (value * one).round();
        if !scaled.is_finite() || scaled < f64::from(i32::MIN) || scaled > f64::from(i32::MAX) {
            return None;
        }
        result[index] = scaled as i32 as u32;
    }
    Some(result)
}

/// ImageCompression.h: a MatrixRecord maps (x, y, 1) by row-vector
/// multiplication, with the third column holding Fract values.
fn transform_point(matrix: &[f64; 9], x: f64, y: f64) -> Option<(f64, f64)> {
    let w = x * matrix[2] + y * matrix[5] + matrix[8];
    if w == 0.0 {
        return None;
    }
    Some((
        (x * matrix[0] + y * matrix[3] + matrix[6]) / w,
        (x * matrix[1] + y * matrix[4] + matrix[7]) / w,
    ))
}

fn invert_matrix(matrix: &[f64; 9]) -> Option<[f64; 9]> {
    let [a, b, c, d, e, f, g, h, i] = *matrix;
    let determinant = a * (e * i - f * h) - b * (d * i - f * g) + c * (d * h - e * g);
    if determinant.abs() < 1e-12 {
        return None;
    }
    let inverse = [
        (e * i - f * h) / determinant,
        (c * h - b * i) / determinant,
        (b * f - c * e) / determinant,
        (f * g - d * i) / determinant,
        (a * i - c * g) / determinant,
        (c * d - a * f) / determinant,
        (d * h - e * g) / determinant,
        (b * g - a * h) / determinant,
        (a * e - b * d) / determinant,
    ];
    // Normalise so the bottom-right Fract stays representable.
    let scale = inverse[8];
    if scale.abs() < 1e-12 {
        return Some(inverse);
    }
    Some(std::array::from_fn(|index| inverse[index] / scale))
}

fn rect_matrix(src: Rect, dst: Rect) -> [f64; 9] {
    let src_width = f64::from(src.3) - f64::from(src.1);
    let src_height = f64::from(src.2) - f64::from(src.0);
    let scale_x = if src_width == 0.0 {
        1.0
    } else {
        (f64::from(dst.3) - f64::from(dst.1)) / src_width
    };
    let scale_y = if src_height == 0.0 {
        1.0
    } else {
        (f64::from(dst.2) - f64::from(dst.0)) / src_height
    };
    [
        scale_x,
        0.0,
        0.0,
        0.0,
        scale_y,
        0.0,
        f64::from(dst.1) - f64::from(src.1) * scale_x,
        f64::from(dst.0) - f64::from(src.0) * scale_y,
        1.0,
    ]
}

/// Transforms a rectangle's corners, returning the bounding rectangle, the
/// four transformed corners, and whether the result is still an upright
/// rectangle (TransformRect's Boolean result).
fn transform_rect(matrix: &[f64; 9], rect: Rect) -> Option<(Rect, [(f64, f64); 4], bool)> {
    let (top, left, bottom, right) = (
        f64::from(rect.0),
        f64::from(rect.1),
        f64::from(rect.2),
        f64::from(rect.3),
    );
    let corners = [
        transform_point(matrix, left, top)?,
        transform_point(matrix, right, top)?,
        transform_point(matrix, right, bottom)?,
        transform_point(matrix, left, bottom)?,
    ];
    let clamp = |value: f64| value.round().clamp(-32768.0, 32767.0) as i16;
    let min_x = corners.iter().map(|point| point.0).fold(f64::MAX, f64::min);
    let max_x = corners.iter().map(|point| point.0).fold(f64::MIN, f64::max);
    let min_y = corners.iter().map(|point| point.1).fold(f64::MAX, f64::min);
    let max_y = corners.iter().map(|point| point.1).fold(f64::MIN, f64::max);
    let upright = matrix[1] == 0.0 && matrix[3] == 0.0 && matrix[2] == 0.0 && matrix[5] == 0.0;
    Some((
        (clamp(min_y), clamp(min_x), clamp(max_y), clamp(max_x)),
        corners,
        upright,
    ))
}

fn read_matrix(memory: &mut PpcSectionMem, address: u32) -> Option<[u32; 9]> {
    let mut matrix = [0u32; 9];
    for (index, value) in matrix.iter_mut().enumerate() {
        *value = memory.read_u32_be(address.checked_add(index as u32 * 4)?)?;
    }
    Some(matrix)
}

fn write_matrix(memory: &mut PpcSectionMem, address: u32, matrix: &[u32; 9]) -> Option<()> {
    if !ppc_memory_can_write_bytes(memory, address, 36) {
        return None;
    }
    for (index, value) in matrix.iter().enumerate() {
        memory.write_u32_be(address + index as u32 * 4, *value)?;
    }
    Some(())
}

/// The movie's natural bounds: the union of its track rectangles before the
/// movie matrix is applied.
fn natural_bounds(quicktime: &PpcQuickTimeState) -> Rect {
    quicktime.movie_file_bounds.unwrap_or(quicktime.movie_box)
}

/// The movie matrix. SetMovieBox adjusts the movie matrix so the natural
/// bounds map to the new box; an explicit SetMovieMatrix is kept verbatim.
fn movie_matrix(quicktime: &PpcQuickTimeState) -> [f64; 9] {
    quicktime.movie_matrix.map_or_else(
        || rect_matrix(natural_bounds(quicktime), quicktime.movie_box),
        |matrix| matrix_to_f64(&matrix),
    )
}

/// SetMovieBox composes a display-space resize with the current matrix,
/// preserving any existing track rotation or shear.
pub(crate) fn ppc_qt_resize_movie_matrix(quicktime: &mut PpcQuickTimeState, rect: Rect) -> bool {
    let matrix = movie_matrix(quicktime);
    let resize = rect_matrix(quicktime.movie_box, rect);
    let result = std::array::from_fn(|index| {
        let (row, column) = (index / 3, index % 3);
        (0..3)
            .map(|k| matrix[row * 3 + k] * resize[k * 3 + column])
            .sum()
    });
    let Some(result) = matrix_from_f64(&result) else {
        return false;
    };
    quicktime.movie_matrix = Some(result);
    true
}

fn track_rect(track: &PpcQuickTimeTrackRecord) -> Rect {
    let width = (track.width >> 16).min(0x7fff) as i16;
    let height = (track.height >> 16).min(0x7fff) as i16;
    (0, 0, height, width)
}

fn rect_is_empty(rect: Rect) -> bool {
    rect.2 <= rect.0 || rect.3 <= rect.1
}

fn intersect_rect(lhs: Rect, rhs: Rect) -> Rect {
    let result = (
        lhs.0.max(rhs.0),
        lhs.1.max(rhs.1),
        lhs.2.min(rhs.2),
        lhs.3.min(rhs.3),
    );
    if rect_is_empty(result) {
        (0, 0, 0, 0)
    } else {
        result
    }
}

/// The track's bounds in the movie coordinate system (track matrix only).
fn track_movie_rect(track: &PpcQuickTimeTrackRecord) -> Rect {
    let rect = track_rect(track);
    if rect_is_empty(rect) {
        return (0, 0, 0, 0);
    }
    transform_rect(&matrix_to_f64(&track.matrix), rect).map_or((0, 0, 0, 0), |(bounds, ..)| bounds)
}

/// The track's bounds in display coordinates (track and movie matrices).
fn track_display_rect(quicktime: &PpcQuickTimeState, track: &PpcQuickTimeTrackRecord) -> Rect {
    if !track.enabled {
        return (0, 0, 0, 0);
    }
    let rect = track_movie_rect(track);
    if rect_is_empty(rect) {
        return rect;
    }
    let display =
        transform_rect(&movie_matrix(quicktime), rect).map_or((0, 0, 0, 0), |(bounds, ..)| bounds);
    match quicktime
        .movie_display_clip
        .as_deref()
        .and_then(ppc_region_storage_bbox)
    {
        Some(clip) => intersect_rect(display, clip),
        None => display,
    }
}

fn rect_region_storage((top, left, bottom, right): Rect) -> Vec<u8> {
    let rect = if rect_is_empty((top, left, bottom, right)) {
        (0, 0, 0, 0)
    } else {
        (top, left, bottom, right)
    };
    let mut storage = 10u16.to_be_bytes().to_vec();
    for value in [rect.0, rect.1, rect.2, rect.3] {
        storage.extend_from_slice(&value.to_be_bytes());
    }
    storage
}

fn display_region_storage(rects: &[Rect], clip: Option<&[u8]>) -> Option<Vec<u8>> {
    let Some(top) = rects.iter().map(|rect| rect.0).min() else {
        return Some(rect_region_storage((0, 0, 0, 0)));
    };
    let bottom = rects.iter().map(|rect| rect.2).max()?;
    let mut rows = vec![Vec::new(); (i32::from(bottom) - i32::from(top)) as usize];
    for &(rect_top, left, rect_bottom, right) in rects {
        for y in rect_top..rect_bottom {
            let row = &mut rows[(i32::from(y) - i32::from(top)) as usize];
            *row = ppc_region_combine_rows(row, &[left, right], PpcRegionBooleanOp::Union);
        }
    }
    if let Some(clip) = clip {
        let clip_rows = ppc_region_rows_for_band(clip, top, bottom)?;
        for (row, clip_row) in rows.iter_mut().zip(clip_rows) {
            *row = ppc_region_combine_rows(row, &clip_row, PpcRegionBooleanOp::Intersection);
        }
    }
    ppc_region_storage_from_rows(top, &rows)
}

struct ToolboxContext<'a> {
    cpu: &'a mut PpcCpu,
    memory: &'a mut PpcSectionMem,
    process_memory_manager: &'a mut ProcessNativeMemoryManager,
    heap_cursor: &'a mut u32,
    last_mem_error: &'a mut i16,
    handles: &'a mut Vec<PpcHandleRecord>,
}

impl ToolboxContext<'_> {
    /// A new handle holding a copy of region storage, as the Movie Toolbox
    /// returns from its Get...Rgn calls. The caller disposes it.
    fn new_region(&mut self, storage: &[u8]) -> u32 {
        let handle = ppc_process_new_rgn(
            self.process_memory_manager,
            self.memory,
            self.heap_cursor,
            self.last_mem_error,
            self.handles,
        );
        if handle == 0 {
            return 0;
        }
        if storage.len() != 10 {
            let mut allocator = PpcProcessAllocatorView {
                memory_manager: self.process_memory_manager,
            };
            let error = allocator.resize_handle(
                self.memory,
                self.heap_cursor,
                self.last_mem_error,
                self.handles,
                handle,
                storage.len() as u32,
            );
            if error != PPC_NO_ERR {
                self.process_memory_manager
                    .dispose_process_handle_from_native_import(self.memory, handle);
                self.handles.retain(|record| record.handle != handle);
                return 0;
            }
        }
        if let Some(ptr) = self.memory.read_u32_be(handle).filter(|ptr| *ptr != 0) {
            for (offset, byte) in storage.iter().enumerate() {
                let _ = self.memory.write_u8(ptr + offset as u32, *byte);
            }
        }
        handle
    }

    fn new_handle_with_bytes(&mut self, bytes: &[u8]) -> u32 {
        ppc_process_alloc_handle_with_bytes(
            self.process_memory_manager,
            self.memory,
            self.heap_cursor,
            self.last_mem_error,
            self.handles,
            bytes,
        )
    }

    fn copy_region(&mut self, handle: u32) -> Option<Vec<u8>> {
        (handle != 0)
            .then(|| ppc_region_storage(self.memory, handle))
            .flatten()
    }
}

fn track_index(quicktime: &PpcQuickTimeState, track: u32) -> Option<usize> {
    if quicktime.movie_disposed {
        return None;
    }
    quicktime
        .movie_tracks
        .iter()
        .position(|record| record.handle == track)
}

fn media_handler_index(quicktime: &PpcQuickTimeState, handler: u32) -> Option<usize> {
    if quicktime.movie_disposed {
        return None;
    }
    quicktime
        .movie_tracks
        .iter()
        .position(|record| record.handle + TRACK_MEDIA_HANDLER_OFFSET == handler)
}

fn controller_index(quicktime: &PpcQuickTimeState, controller: u32) -> Option<usize> {
    quicktime
        .movie_controllers
        .iter()
        .position(|record| record.handle == controller)
}

fn is_visual_media(media_type: u32) -> bool {
    ppc_qt_track_matches(
        &PpcQuickTimeTrackRecord {
            media_type,
            enabled: true,
            ..PpcQuickTimeTrackRecord::default()
        },
        u32::from_be_bytes(*b"eyes"),
        2,
    )
}

fn is_sound_media(media_type: u32) -> bool {
    media_type == u32::from_be_bytes(*b"soun")
}

/// Movie time a GetMoviePict should show, converted to a task count.
fn task_count_for_time(quicktime: &PpcQuickTimeState, time: i64) -> u32 {
    let duration = ppc_qt_movie_duration_value(quicktime).max(1);
    let tasks = u64::from(quicktime.movie_tasks_until_done.max(1));
    let time = (time.max(0) as u64).min(duration);
    (u128::from(time) * u128::from(tasks) / u128::from(duration)) as u32
}

/// Builds a version 2 PICT containing one unpacked 32-bit DirectBitsRect
/// (Imaging With QuickDraw, Appendix A, picture opcodes).
fn direct_bits_pict(frame: &PpcQuickTimeDecodedVideoFrame, dst: Rect) -> Option<Vec<u8>> {
    let width = u16::try_from(frame.width)
        .ok()
        .filter(|width| *width < 0x0fff)?;
    let height = u16::try_from(frame.height)
        .ok()
        .filter(|height| *height < 0x7fff)?;
    let row_bytes = width.checked_mul(4)?;
    let mut pict = vec![0, 0];
    let push_rect = |pict: &mut Vec<u8>, (top, left, bottom, right): Rect| {
        for value in [top, left, bottom, right] {
            pict.extend_from_slice(&value.to_be_bytes());
        }
    };
    push_rect(&mut pict, dst);
    pict.extend_from_slice(&[0x00, 0x11, 0x02, 0xff, 0x0c, 0x00]);
    pict.extend_from_slice(&[0xff, 0xfe, 0, 0, 0, 0x48, 0, 0, 0, 0x48, 0, 0]);
    push_rect(&mut pict, dst);
    pict.extend_from_slice(&[0; 4]);
    pict.extend_from_slice(&[0x00, 0x01, 0x00, 0x0a]);
    push_rect(&mut pict, dst);
    pict.extend_from_slice(&[0x00, 0x9a, 0x00, 0x00, 0x00, 0xff]);
    pict.extend_from_slice(&(row_bytes | 0x8000).to_be_bytes());
    let bounds = (0, 0, height as i16, width as i16);
    push_rect(&mut pict, bounds);
    // pmVersion 0, packType 1 (unpacked), packSize 0, 72 dpi, RGBDirect,
    // 32-bit pixels with three 8-bit components.
    pict.extend_from_slice(&[0, 0, 0, 1, 0, 0, 0, 0, 0, 0x48, 0, 0, 0, 0x48, 0, 0]);
    pict.extend_from_slice(&[0, 16, 0, 32, 0, 3, 0, 8]);
    pict.extend_from_slice(&[0; 12]);
    push_rect(&mut pict, bounds);
    push_rect(&mut pict, dst);
    pict.extend_from_slice(&[0, 0]);
    for rgb in frame.rgb.chunks_exact(3) {
        pict.extend_from_slice(&[0, rgb[0], rgb[1], rgb[2]]);
    }
    pict.extend_from_slice(&[0x00, 0xff]);
    let size = (pict.len() as u32 & 0xffff) as u16;
    pict[0..2].copy_from_slice(&size.to_be_bytes());
    Some(pict)
}

/// A picture with only a frame and an empty clip, for movies with no
/// visual media at the requested time.
fn empty_pict(dst: Rect) -> Vec<u8> {
    let mut pict = vec![0, 0];
    for value in [dst.0, dst.1, dst.2, dst.3] {
        pict.extend_from_slice(&value.to_be_bytes());
    }
    pict.extend_from_slice(&[0x00, 0x11, 0x02, 0xff, 0x0c, 0x00]);
    pict.extend_from_slice(&[0xff, 0xfe, 0, 0, 0, 0x48, 0, 0, 0, 0x48, 0, 0]);
    for value in [dst.0, dst.1, dst.2, dst.3] {
        pict.extend_from_slice(&value.to_be_bytes());
    }
    pict.extend_from_slice(&[0; 4]);
    pict.extend_from_slice(&[0x00, 0xff]);
    let size = pict.len() as u16;
    pict[0..2].copy_from_slice(&size.to_be_bytes());
    pict
}

#[allow(clippy::too_many_arguments)]
pub(super) fn dispatch_movie_toolbox(
    operation: PpcQuickTimeCompatibilityOperation,
    cpu: &mut PpcCpu,
    memory: &mut PpcSectionMem,
    process_memory_manager: &mut ProcessNativeMemoryManager,
    heap_cursor: &mut u32,
    last_mem_error: &mut i16,
    handles: &mut Vec<PpcHandleRecord>,
    gworlds: &[PpcGWorldRecord],
    current_gworld: u32,
    quicktime: &mut PpcQuickTimeState,
) -> PpcImportAction {
    use PpcQuickTimeCompatibilityOperation as Op;
    let mut context = ToolboxContext {
        cpu,
        memory,
        process_memory_manager,
        heap_cursor,
        last_mem_error,
        handles,
    };
    let gpr = context.cpu.gpr;
    let movie_valid = compatibility_valid_movie(quicktime, gpr[3]);
    match operation {
        Op::GetMovieTrackCount => {
            if !movie_valid {
                ppc_qt_record_error(quicktime, INVALID_MOVIE);
                return PpcImportAction::Return(0);
            }
            ppc_qt_record_error(quicktime, PPC_NO_ERR);
            PpcImportAction::Return(quicktime.movie_tracks.len() as u32)
        }
        Op::GetMovieIndTrack => {
            if !movie_valid {
                ppc_qt_record_error(quicktime, INVALID_MOVIE);
                return PpcImportAction::Return(0);
            }
            let index = gpr[4] as i32;
            let track = index
                .checked_sub(1)
                .and_then(|index| usize::try_from(index).ok())
                .and_then(|index| quicktime.movie_tracks.get(index))
                .map(|track| track.handle);
            ppc_qt_record_error(
                quicktime,
                if track.is_some() {
                    PPC_NO_ERR
                } else {
                    BAD_TRACK_INDEX
                },
            );
            PpcImportAction::Return(track.unwrap_or(0))
        }
        Op::GetMovieRate => {
            if !movie_valid {
                ppc_qt_record_error(quicktime, INVALID_MOVIE);
                return PpcImportAction::Return(0);
            }
            ppc_qt_record_error(quicktime, PPC_NO_ERR);
            let playing = quicktime.movie_started;
            PpcImportAction::Return(if playing {
                quicktime.movie_playback_rate as u32
            } else {
                0
            })
        }
        Op::SetMoviePreferredRate => {
            if movie_valid {
                quicktime.movie_preferred_rate = gpr[4] as i32;
            }
            ppc_qt_record_error(
                quicktime,
                if movie_valid {
                    PPC_NO_ERR
                } else {
                    INVALID_MOVIE
                },
            );
            PpcImportAction::ReturnPreserve
        }
        Op::SetMovieTimeScale => {
            let scale = gpr[4] as i32;
            let error = if !movie_valid {
                INVALID_MOVIE
            } else if scale <= 0 {
                PPC_PARAM_ERR
            } else {
                quicktime.movie_time_scale = scale as u32;
                PPC_NO_ERR
            };
            ppc_qt_record_error(quicktime, error);
            PpcImportAction::ReturnPreserve
        }
        Op::GetMovieSelection | Op::GetMoviePosterTime => {
            if !movie_valid {
                ppc_qt_record_error(quicktime, INVALID_MOVIE);
                return PpcImportAction::Return(0);
            }
            let (poster, selection, selection_duration) =
                ppc_qt_mvhd_times(&quicktime.movie_file_data).unwrap_or((0, 0, 0));
            ppc_qt_record_error(quicktime, PPC_NO_ERR);
            if operation == Op::GetMoviePosterTime {
                let poster = ppc_qt_to_movie_scale(quicktime, i64::from(poster));
                return PpcImportAction::Return(poster as u32);
            }
            let selection = ppc_qt_to_movie_scale(quicktime, i64::from(selection)) as u32;
            let duration = ppc_qt_to_movie_scale(quicktime, i64::from(selection_duration)) as u32;
            if gpr[4] != 0 {
                let _ = context.memory.write_u32_be(gpr[4], selection);
            }
            if gpr[5] != 0 {
                let _ = context.memory.write_u32_be(gpr[5], duration);
            }
            PpcImportAction::ReturnPreserve
        }
        Op::GetMovieDataSize => {
            // Movies.h: the number of bytes of sample data the movie's
            // enabled tracks use in [startTime, startTime + duration).
            if !movie_valid {
                ppc_qt_record_error(quicktime, INVALID_MOVIE);
                return PpcImportAction::Return(0);
            }
            let start = ppc_qt_from_movie_scale(quicktime, i64::from(gpr[4] as i32));
            let end =
                start.saturating_add(ppc_qt_from_movie_scale(quicktime, i64::from(gpr[5] as i32)));
            let mut total = 0u64;
            for (ordinal, track) in quicktime.movie_tracks.iter().enumerate() {
                if !track.enabled {
                    continue;
                }
                for sample in ppc_qt_track_movie_samples(quicktime, ordinal + 1, track) {
                    if sample.start < end && sample.start + sample.duration > start {
                        total = total.saturating_add(u64::from(sample.size));
                    }
                }
            }
            ppc_qt_record_error(quicktime, PPC_NO_ERR);
            PpcImportAction::Return(total.min(i32::MAX as u64) as u32)
        }
        Op::GetMovieNextInterestingTime => {
            ppc_qt_next_interesting_time(context.memory, quicktime, gpr);
            PpcImportAction::ReturnPreserve
        }
        Op::GetMovieGWorld => {
            if !movie_valid {
                ppc_qt_record_error(quicktime, INVALID_MOVIE);
                return PpcImportAction::ReturnPreserve;
            }
            let (port, device) = if quicktime.movie_gworld != 0 {
                (quicktime.movie_gworld, quicktime.movie_gdevice)
            } else {
                // Movies are created in the current graphics port and device.
                (current_gworld, PPC_MAIN_GDEVICE)
            };
            if gpr[4] != 0 {
                let _ = context.memory.write_u32_be(gpr[4], port);
            }
            if gpr[5] != 0 {
                let _ = context.memory.write_u32_be(gpr[5], device);
            }
            ppc_qt_record_error(quicktime, PPC_NO_ERR);
            PpcImportAction::ReturnPreserve
        }
        Op::GetMovieMatrix => {
            if !movie_valid {
                ppc_qt_record_error(quicktime, INVALID_MOVIE);
                return PpcImportAction::ReturnPreserve;
            }
            let matrix = quicktime
                .movie_matrix
                .or_else(|| matrix_from_f64(&movie_matrix(quicktime)))
                .unwrap_or(IDENTITY_MATRIX);
            let error = if gpr[4] != 0 && write_matrix(context.memory, gpr[4], &matrix).is_some() {
                PPC_NO_ERR
            } else {
                PPC_PARAM_ERR
            };
            ppc_qt_record_error(quicktime, error);
            PpcImportAction::ReturnPreserve
        }
        Op::SetMovieMatrix => {
            // A nil matrix restores the identity matrix. The movie box
            // follows the matrix: it is the transformed natural bounds.
            if !movie_valid {
                ppc_qt_record_error(quicktime, INVALID_MOVIE);
                return PpcImportAction::ReturnPreserve;
            }
            let matrix = if gpr[4] == 0 {
                Some(IDENTITY_MATRIX)
            } else {
                read_matrix(context.memory, gpr[4])
            };
            let Some(matrix) = matrix else {
                ppc_qt_record_error(quicktime, PPC_PARAM_ERR);
                return PpcImportAction::ReturnPreserve;
            };
            if let Some((bounds, ..)) =
                transform_rect(&matrix_to_f64(&matrix), natural_bounds(quicktime))
            {
                quicktime.movie_box = bounds;
            }
            quicktime.movie_matrix = Some(matrix);
            ppc_qt_record_error(quicktime, PPC_NO_ERR);
            PpcImportAction::ReturnPreserve
        }
        Op::GetMovieDisplayBoundsRgn => {
            if !movie_valid {
                ppc_qt_record_error(quicktime, INVALID_MOVIE);
                return PpcImportAction::Return(0);
            }
            // Preserve gaps between tracks and holes in the display clip.
            let rects: Vec<_> = quicktime
                .movie_tracks
                .iter()
                .filter(|track| is_visual_media(track.media_type))
                .map(|track| track_display_rect(quicktime, track))
                .filter(|rect| !rect_is_empty(*rect))
                .collect();
            let storage = display_region_storage(&rects, quicktime.movie_display_clip.as_deref());
            let region = storage.map_or(0, |storage| context.new_region(&storage));
            ppc_qt_record_error(
                quicktime,
                if region == 0 {
                    PPC_MEM_FULL_ERR
                } else {
                    PPC_NO_ERR
                },
            );
            PpcImportAction::Return(region)
        }
        Op::SetMovieDisplayClipRgn => {
            if !movie_valid {
                ppc_qt_record_error(quicktime, INVALID_MOVIE);
                return PpcImportAction::ReturnPreserve;
            }
            // The Movie Toolbox keeps its own copy; nil removes the clip.
            quicktime.movie_display_clip = context.copy_region(gpr[4]);
            ppc_qt_record_error(quicktime, PPC_NO_ERR);
            PpcImportAction::ReturnPreserve
        }
        Op::SetMovieDrawingCompleteProc => {
            if !movie_valid {
                ppc_qt_record_error(quicktime, INVALID_MOVIE);
                return PpcImportAction::ReturnPreserve;
            }
            let procedure = gpr[5];
            quicktime.movie_drawing_complete = None;
            if procedure != 0 {
                if ppc_resolve_callback_target(context.memory, procedure, gpr[2], None).is_none() {
                    ppc_qt_record_error(quicktime, PPC_PARAM_ERR);
                    return PpcImportAction::ReturnPreserve;
                }
                let registration = callbacks::Registration {
                    procedure,
                    rtoc: gpr[2],
                    refcon: gpr[6],
                    parameters: [0; 3],
                };
                quicktime.movie_drawing_complete = Some((gpr[4], registration));
            }
            ppc_qt_record_error(quicktime, PPC_NO_ERR);
            PpcImportAction::ReturnPreserve
        }
        Op::GetMoviePict => {
            if !movie_valid {
                ppc_qt_record_error(quicktime, INVALID_MOVIE);
                return PpcImportAction::Return(0);
            }
            let has_video = quicktime
                .movie_tracks
                .iter()
                .any(|track| track.enabled && is_visual_media(track.media_type));
            let saved_task_count = quicktime.movie_task_count;
            let saved_fraction = quicktime.movie_task_fraction;
            quicktime.movie_task_fraction = 0;
            quicktime.movie_task_count = task_count_for_time(quicktime, i64::from(gpr[4] as i32));
            let frame = ppc_qt_decode_current_movie_video_frame(quicktime);
            quicktime.movie_task_count = saved_task_count;
            quicktime.movie_task_fraction = saved_fraction;
            let pict = match frame {
                Some(frame) => direct_bits_pict(&frame, quicktime.movie_box),
                None if !has_video => Some(empty_pict(quicktime.movie_box)),
                // Only Cinepak video is decoded by this runtime.
                None => None,
            };
            let Some(pict) = pict else {
                ppc_qt_record_error(quicktime, NO_CODEC_ERR);
                return PpcImportAction::Return(0);
            };
            let handle = context.new_handle_with_bytes(&pict);
            ppc_qt_record_error(
                quicktime,
                if handle == 0 {
                    PPC_MEM_FULL_ERR
                } else {
                    PPC_NO_ERR
                },
            );
            PpcImportAction::Return(handle)
        }
        Op::ConvertTimeScale => {
            // TimeRecord: CompTimeValue value (hi, lo), TimeScale scale,
            // TimeBase base.
            let record = gpr[3];
            let new_scale = gpr[4] as i32;
            let (Some(high), Some(low), Some(scale)) = (
                context.memory.read_u32_be(record),
                context.memory.read_u32_be(record.wrapping_add(4)),
                context.memory.read_u32_be(record.wrapping_add(8)),
            ) else {
                return PpcImportAction::ReturnPreserve;
            };
            let scale = scale as i32;
            if record == 0 || scale <= 0 || new_scale <= 0 {
                return PpcImportAction::ReturnPreserve;
            }
            let value = (i64::from(high as i32) << 32) | i64::from(low);
            let converted = (i128::from(value) * i128::from(new_scale) / i128::from(scale))
                .clamp(i128::from(i64::MIN), i128::from(i64::MAX))
                as i64;
            let _ = context
                .memory
                .write_u32_be(record, (converted >> 32) as u32);
            let _ = context.memory.write_u32_be(record + 4, converted as u32);
            let _ = context.memory.write_u32_be(record + 8, new_scale as u32);
            PpcImportAction::ReturnPreserve
        }
        Op::GetTrackDuration
        | Op::GetTrackOffset
        | Op::GetTrackLayer
        | Op::GetTrackVolume
        | Op::GetTrackEnabled
        | Op::GetTrackUserData => {
            let Some(index) = track_index(quicktime, gpr[3]) else {
                ppc_qt_record_error(quicktime, INVALID_TRACK);
                return PpcImportAction::Return(0);
            };
            let track = quicktime.movie_tracks[index];
            let value = match operation {
                Op::GetTrackDuration => {
                    ppc_qt_to_movie_scale(quicktime, track.duration as i64) as u32
                }
                Op::GetTrackOffset => ppc_qt_to_movie_scale(quicktime, track.offset as i64) as u32,
                Op::GetTrackLayer => track.layer as u16 as u32,
                Op::GetTrackVolume => track.volume as u16 as u32,
                Op::GetTrackEnabled => u32::from(track.enabled),
                _ => {
                    let user_data = track.handle + TRACK_USER_DATA_OFFSET;
                    if ppc_qt_user_data_range(quicktime, user_data).is_some() {
                        user_data
                    } else {
                        0
                    }
                }
            };
            ppc_qt_record_error(quicktime, PPC_NO_ERR);
            PpcImportAction::Return(value)
        }
        Op::SetTrackEnabled | Op::SetTrackLayer | Op::SetTrackOffset | Op::SetTrackVolume => {
            let Some(index) = track_index(quicktime, gpr[3]) else {
                ppc_qt_record_error(quicktime, INVALID_TRACK);
                return PpcImportAction::ReturnPreserve;
            };
            let offset = ppc_qt_from_movie_scale(quicktime, i64::from(gpr[4] as i32)).max(0) as u64;
            let track = &mut quicktime.movie_tracks[index];
            match operation {
                Op::SetTrackEnabled => track.enabled = gpr[4] as u8 != 0,
                Op::SetTrackLayer => track.layer = gpr[4] as u16 as i16,
                Op::SetTrackVolume => track.volume = gpr[4] as u16 as i16,
                _ => {
                    // The track's duration includes its offset.
                    track.duration = track.duration.saturating_sub(track.offset) + offset;
                    track.offset = offset;
                }
            }
            ppc_qt_record_error(quicktime, PPC_NO_ERR);
            PpcImportAction::ReturnPreserve
        }
        Op::GetTrackDisplayBoundsRgn | Op::GetTrackMovieBoundsRgn => {
            let Some(index) = track_index(quicktime, gpr[3]) else {
                ppc_qt_record_error(quicktime, INVALID_TRACK);
                return PpcImportAction::Return(0);
            };
            let track = quicktime.movie_tracks[index];
            let rect = if operation == Op::GetTrackDisplayBoundsRgn {
                track_display_rect(quicktime, &track)
            } else {
                track_movie_rect(&track)
            };
            let region = context.new_region(&rect_region_storage(rect));
            ppc_qt_record_error(
                quicktime,
                if region == 0 {
                    PPC_MEM_FULL_ERR
                } else {
                    PPC_NO_ERR
                },
            );
            PpcImportAction::Return(region)
        }
        Op::PtInTrack => {
            // Point is passed by value: v in the high word, h in the low.
            let Some(index) = track_index(quicktime, gpr[3]) else {
                ppc_qt_record_error(quicktime, INVALID_TRACK);
                return PpcImportAction::Return(0);
            };
            let (top, left, bottom, right) =
                track_display_rect(quicktime, &quicktime.movie_tracks[index]);
            let v = (gpr[4] >> 16) as i16;
            let h = gpr[4] as i16;
            ppc_qt_record_error(quicktime, PPC_NO_ERR);
            PpcImportAction::Return(u32::from(
                v >= top
                    && v < bottom
                    && h >= left
                    && h < right
                    && ppc_qt_display_clip_contains(quicktime, i32::from(h), i32::from(v)),
            ))
        }
        Op::CountUserDataType => {
            let Some(range) = ppc_qt_user_data_range(quicktime, gpr[3]) else {
                return PpcImportAction::Return(0);
            };
            let count = ppc_qt_count_user_data_items(&quicktime.movie_file_data, range, gpr[4]);
            PpcImportAction::Return(count.min(0x7fff))
        }
        Op::GetMediaHandler => {
            let handler = quicktime
                .movie_tracks
                .iter()
                .find(|track| track.handle + 1 == gpr[3] && !quicktime.movie_disposed)
                .map(|track| track.handle + TRACK_MEDIA_HANDLER_OFFSET);
            ppc_qt_record_error(
                quicktime,
                if handler.is_some() {
                    PPC_NO_ERR
                } else {
                    INVALID_MEDIA
                },
            );
            PpcImportAction::Return(handler.unwrap_or(0))
        }
        Op::GetMediaHandlerDescription => {
            let Some(ordinal) = quicktime
                .movie_tracks
                .iter()
                .position(|track| track.handle + 1 == gpr[3] && !quicktime.movie_disposed)
            else {
                ppc_qt_record_error(quicktime, INVALID_MEDIA);
                return PpcImportAction::ReturnPreserve;
            };
            let media_type = quicktime.movie_tracks[ordinal].media_type;
            let (manufacturer, name) =
                ppc_qt_media_handler_identity(&quicktime.movie_file_data, ordinal + 1)
                    .unwrap_or((0, Vec::new()));
            if gpr[4] != 0 {
                let _ = context.memory.write_u32_be(gpr[4], media_type);
            }
            if gpr[5] != 0 {
                let _ = ppc_write_pstring_bytes(context.memory, gpr[5], &name);
            }
            if gpr[6] != 0 {
                let _ = context.memory.write_u32_be(gpr[6], manufacturer);
            }
            ppc_qt_record_error(quicktime, PPC_NO_ERR);
            PpcImportAction::ReturnPreserve
        }
        Op::MediaHasCharacteristic
        | Op::MediaGetSoundBalance
        | Op::MediaSetSoundBalance
        | Op::MediaSetGraphicsMode => {
            let Some(index) = media_handler_index(quicktime, gpr[3]) else {
                return PpcImportAction::Return(BAD_COMPONENT_INSTANCE);
            };
            let track = &mut quicktime.movie_tracks[index];
            match operation {
                Op::MediaHasCharacteristic => {
                    let enabled_track = PpcQuickTimeTrackRecord {
                        enabled: true,
                        ..*track
                    };
                    let has_it = ppc_qt_track_matches(&enabled_track, gpr[4], 2);
                    let result = if gpr[5] != 0
                        && context.memory.write_u8(gpr[5], u8::from(has_it)).is_some()
                    {
                        PPC_NO_ERR
                    } else {
                        PPC_PARAM_ERR
                    };
                    PpcImportAction::Return(ppc_i16_result(result))
                }
                // Only the sound media handler implements the balance calls
                // and only visual media handlers the graphics mode call.
                Op::MediaGetSoundBalance | Op::MediaSetSoundBalance
                    if !is_sound_media(track.media_type) =>
                {
                    PpcImportAction::Return(BAD_COMPONENT_SELECTOR)
                }
                Op::MediaSetGraphicsMode if !is_visual_media(track.media_type) => {
                    PpcImportAction::Return(BAD_COMPONENT_SELECTOR)
                }
                Op::MediaGetSoundBalance => {
                    if gpr[4] == 0
                        || context
                            .memory
                            .write_u16_be(gpr[4], track.balance as u16)
                            .is_none()
                    {
                        return PpcImportAction::Return(ppc_i16_result(PPC_PARAM_ERR));
                    }
                    PpcImportAction::Return(0)
                }
                Op::MediaSetSoundBalance => {
                    track.balance = gpr[4] as u16 as i16;
                    PpcImportAction::Return(0)
                }
                _ => {
                    track.graphics_mode = gpr[4];
                    PpcImportAction::Return(0)
                }
            }
        }
        Op::SetIdentityMatrix => {
            let _ = write_matrix(context.memory, gpr[3], &IDENTITY_MATRIX);
            PpcImportAction::ReturnPreserve
        }
        Op::InverseMatrix => {
            let Some(matrix) = read_matrix(context.memory, gpr[3]) else {
                return PpcImportAction::Return(0);
            };
            let Some(inverse) = invert_matrix(&matrix_to_f64(&matrix))
                .and_then(|inverse| matrix_from_f64(&inverse))
            else {
                return PpcImportAction::Return(0);
            };
            PpcImportAction::Return(u32::from(
                write_matrix(context.memory, gpr[4], &inverse).is_some(),
            ))
        }
        Op::RectMatrix => {
            let (Some(src), Some(dst)) = (
                ppc_read_rect(context.memory, gpr[4]),
                ppc_read_rect(context.memory, gpr[5]),
            ) else {
                return PpcImportAction::ReturnPreserve;
            };
            if let Some(matrix) = matrix_from_f64(&rect_matrix(src, dst)) {
                let _ = write_matrix(context.memory, gpr[3], &matrix);
            }
            PpcImportAction::ReturnPreserve
        }
        Op::TransformRect => {
            let (Some(matrix), Some(rect)) = (
                read_matrix(context.memory, gpr[3]),
                ppc_read_rect(context.memory, gpr[4]),
            ) else {
                return PpcImportAction::Return(0);
            };
            let Some((bounds, corners, upright)) = transform_rect(&matrix_to_f64(&matrix), rect)
            else {
                return PpcImportAction::Return(0);
            };
            let _ = ppc_write_rect(
                context.memory,
                gpr[4],
                bounds.0,
                bounds.1,
                bounds.2,
                bounds.3,
            );
            if gpr[5] != 0 && ppc_memory_can_write_bytes(context.memory, gpr[5], 32) {
                // FixedPoint is {Fixed x; Fixed y;}.
                for (index, (x, y)) in corners.iter().enumerate() {
                    let fixed = |value: f64| {
                        (value * FIXED_ONE)
                            .round()
                            .clamp(f64::from(i32::MIN), f64::from(i32::MAX))
                            as i32 as u32
                    };
                    let _ = context
                        .memory
                        .write_u32_be(gpr[5] + index as u32 * 8, fixed(*x));
                    let _ = context
                        .memory
                        .write_u32_be(gpr[5] + index as u32 * 8 + 4, fixed(*y));
                }
            }
            PpcImportAction::Return(u32::from(upright))
        }
        Op::MCClick
        | Op::MCDraw
        | Op::MCGetClip
        | Op::MCSetClip
        | Op::MCGetControllerBoundsRgn
        | Op::MCGetControllerInfo
        | Op::MCGetVisible
        | Op::MCSetVisible
        | Op::MCIsControllerAttached
        | Op::MCSetControllerAttached
        | Op::MCMovieChanged => {
            dispatch_movie_controller(operation, &mut context, gworlds, current_gworld, quicktime)
        }
        _ => unreachable!("{operation:?} is dispatched by dispatch_quicktime_compatibility"),
    }
}

/// Standard movie controller calls (Inside Macintosh: QuickTime
/// Components, Movie Controller Components). This runtime draws no
/// controller bar, so the controller's bounds are its movie's display area.
fn dispatch_movie_controller(
    operation: PpcQuickTimeCompatibilityOperation,
    context: &mut ToolboxContext<'_>,
    gworlds: &[PpcGWorldRecord],
    current_gworld: u32,
    quicktime: &mut PpcQuickTimeState,
) -> PpcImportAction {
    use PpcQuickTimeCompatibilityOperation as Op;
    /// mcNotVisible creation flag (Movies.h).
    const MC_NOT_VISIBLE: u32 = 8;
    let gpr = context.cpu.gpr;
    let Some(index) = controller_index(quicktime, gpr[3]) else {
        return PpcImportAction::Return(match operation {
            Op::MCGetControllerBoundsRgn => 0,
            _ => ppc_i16_result(PPC_INVALID_COMPONENT_ID),
        });
    };
    let controller = quicktime.movie_controllers[index];
    let movie_valid = controller.movie == PPC_QT_MOVIE && !quicktime.movie_disposed;
    let visible = controller.flags & MC_NOT_VISIBLE == 0;
    let attached = !quicktime
        .detached_movie_controllers
        .contains(&controller.handle);
    match operation {
        Op::MCClick => {
            // No controller bar is drawn, so no controller part can be hit.
            PpcImportAction::Return(0)
        }
        Op::MCDraw => {
            if movie_valid && quicktime.movie_active {
                let _ = ppc_qt_draw_movie_frame(
                    context.memory,
                    gworlds,
                    current_gworld,
                    quicktime,
                    0x40,
                );
            }
            PpcImportAction::Return(0)
        }
        Op::MCGetClip => {
            let (controller_clip, movie_clip) = quicktime
                .movie_controller_clips
                .iter()
                .find(|(handle, ..)| *handle == controller.handle)
                .map(|(_, controller_clip, movie_clip)| {
                    (controller_clip.clone(), movie_clip.clone())
                })
                .unwrap_or((None, None));
            for (out, clip) in [(gpr[4], controller_clip), (gpr[5], movie_clip)] {
                if out == 0 {
                    continue;
                }
                let region = clip.map_or(0, |storage| context.new_region(&storage));
                let _ = context.memory.write_u32_be(out, region);
            }
            PpcImportAction::Return(0)
        }
        Op::MCSetClip => {
            let controller_clip = context.copy_region(gpr[4]);
            let movie_clip = context.copy_region(gpr[5]);
            quicktime
                .movie_controller_clips
                .retain(|(handle, ..)| *handle != controller.handle);
            if controller_clip.is_some() || movie_clip.is_some() {
                quicktime.movie_controller_clips.push((
                    controller.handle,
                    controller_clip,
                    movie_clip,
                ));
            }
            PpcImportAction::Return(0)
        }
        Op::MCGetControllerBoundsRgn => {
            let rect = if visible {
                controller.rect
            } else {
                (0, 0, 0, 0)
            };
            PpcImportAction::Return(context.new_region(&rect_region_storage(rect)))
        }
        Op::MCGetControllerInfo => {
            // Movies.h: mcInfoHasSound = 32, mcInfoIsPlaying = 64.
            let has_sound = movie_valid
                && quicktime.movie_tracks.iter().any(|track| {
                    track.enabled && ppc_qt_track_matches(track, u32::from_be_bytes(*b"ears"), 2)
                });
            let playing = movie_valid && quicktime.movie_started;
            let flags = if has_sound { 32 } else { 0 } | if playing { 64 } else { 0 };
            if gpr[4] == 0 || context.memory.write_u32_be(gpr[4], flags).is_none() {
                return PpcImportAction::Return(ppc_i16_result(PPC_PARAM_ERR));
            }
            PpcImportAction::Return(0)
        }
        Op::MCGetVisible => PpcImportAction::Return(u32::from(visible)),
        Op::MCSetVisible => {
            let record = &mut quicktime.movie_controllers[index];
            if gpr[4] as u8 != 0 {
                record.flags &= !MC_NOT_VISIBLE;
            } else {
                record.flags |= MC_NOT_VISIBLE;
            }
            PpcImportAction::Return(0)
        }
        Op::MCIsControllerAttached => PpcImportAction::Return(u32::from(attached)),
        Op::MCSetControllerAttached => {
            quicktime
                .detached_movie_controllers
                .retain(|handle| *handle != controller.handle);
            if gpr[4] as u8 == 0 {
                quicktime.detached_movie_controllers.push(controller.handle);
            } else if movie_valid {
                // Attaching places the controller around the movie.
                quicktime.movie_controllers[index].rect = quicktime.movie_box;
            }
            PpcImportAction::Return(0)
        }
        _ => {
            // MCMovieChanged: an attached controller follows the movie box.
            if gpr[4] != controller.movie || !movie_valid {
                return PpcImportAction::Return(ppc_i16_result(INVALID_MOVIE));
            }
            if attached {
                quicktime.movie_controllers[index].rect = quicktime.movie_box;
            }
            PpcImportAction::Return(0)
        }
    }
}

/// Poster time, selection time and selection duration from mvhd (QuickTime
/// File Format: these follow the matrix and preview fields).
fn ppc_qt_mvhd_times(data: &[u8]) -> Option<(u32, u32, u32)> {
    fn read(data: &[u8], start: usize, end: usize) -> Option<(u32, u32, u32)> {
        let base = match *data.get(start)? {
            0 => start + 36,
            1 => start + 48,
            _ => return None,
        } + 44;
        if base.checked_add(12)? > end {
            return None;
        }
        let word = |offset: usize| -> Option<u32> {
            Some(u32::from_be_bytes(
                data.get(offset..offset + 4)?.try_into().ok()?,
            ))
        };
        Some((word(base)?, word(base + 4)?, word(base + 8)?))
    }
    ppc_qt_scan_movie_header(data, 0, data.len(), 0, read)
}

/// The media handler's manufacturer and component name from the mdia hdlr.
fn ppc_qt_media_handler_identity(data: &[u8], ordinal: usize) -> Option<(u32, Vec<u8>)> {
    let (track_start, track_end) = ppc_qt_track_atom(data, ordinal)?;
    let (mdia_start, mdia_end) = ppc_qt_child_atom_range(data, track_start, track_end, b"mdia")?;
    let (hdlr_start, hdlr_end) = ppc_qt_child_atom_range(data, mdia_start, mdia_end, b"hdlr")?;
    let manufacturer = u32::from_be_bytes(
        data.get(hdlr_start + 12..hdlr_start + 16)?
            .try_into()
            .ok()?,
    );
    let name = data
        .get(hdlr_start + 24)
        .and_then(|length| {
            data.get(hdlr_start + 25..(hdlr_start + 25 + usize::from(*length)).min(hdlr_end))
        })
        .map(<[u8]>::to_vec)
        .unwrap_or_default();
    Some((manufacturer, name))
}

/// GetMovieNextInterestingTime(Movie, short flags, short numMediaTypes,
/// const OSType *whichMediaTypes, TimeValue time, Fixed rate,
/// TimeValue *interestingTime, TimeValue *interestingDuration).
fn ppc_qt_next_interesting_time(
    memory: &mut PpcSectionMem,
    quicktime: &mut PpcQuickTimeState,
    gpr: [u32; 32],
) {
    const NEXT_TIME_MEDIA_SAMPLE: u16 = 1;
    const NEXT_TIME_MEDIA_EDIT: u16 = 2;
    const NEXT_TIME_TRACK_EDIT: u16 = 4;
    const NEXT_TIME_SYNC_SAMPLE: u16 = 8;
    const NEXT_TIME_STEP: u16 = 16;
    const NEXT_TIME_EDGE_OK: u16 = 0x4000;
    let write = |memory: &mut PpcSectionMem, address: u32, value: i64| {
        if address != 0 {
            let _ = memory.write_u32_be(
                address,
                value.clamp(i64::from(i32::MIN), i64::from(i32::MAX)) as i32 as u32,
            );
        }
    };
    if !compatibility_valid_movie(quicktime, gpr[3]) {
        ppc_qt_record_error(quicktime, INVALID_MOVIE);
        return;
    }
    let flags = gpr[4] as u16;
    let media_type_count = (gpr[5] as i16).max(0) as u32;
    let media_types: Vec<u32> = (0..media_type_count.min(64))
        .filter_map(|index| memory.read_u32_be(gpr[6].wrapping_add(index * 4)))
        .collect();
    let time = ppc_qt_from_movie_scale(quicktime, i64::from(gpr[7] as i32));
    let backward = (gpr[8] as i32) < 0;
    let edge_ok = flags & NEXT_TIME_EDGE_OK != 0;
    // (boundary time, duration of the segment that starts there)
    let mut candidates: Vec<(i64, i64)> = Vec::new();
    for (ordinal, track) in quicktime.movie_tracks.iter().enumerate() {
        if !track.enabled || (!media_types.is_empty() && !media_types.contains(&track.media_type)) {
            continue;
        }
        let samples = ppc_qt_track_movie_samples(quicktime, ordinal + 1, track);
        if flags & (NEXT_TIME_MEDIA_SAMPLE | NEXT_TIME_STEP | NEXT_TIME_SYNC_SAMPLE) != 0 {
            let sync_only = flags & (NEXT_TIME_MEDIA_SAMPLE | NEXT_TIME_STEP) == 0;
            candidates.extend(
                samples
                    .iter()
                    .filter(|sample| !sync_only || sample.sync)
                    .map(|sample| (sample.start, sample.duration)),
            );
        }
        if flags & (NEXT_TIME_MEDIA_EDIT | NEXT_TIME_TRACK_EDIT) != 0 {
            let offset = track.offset as i64;
            let end = track.duration as i64;
            candidates.push((offset, end - offset));
            candidates.push((end, 0));
            if offset > 0 {
                candidates.push((0, offset));
            }
        }
    }
    let selected = if backward {
        candidates
            .into_iter()
            .filter(|(start, _)| *start < time || (edge_ok && *start == time))
            .max_by_key(|(start, _)| *start)
    } else {
        candidates
            .into_iter()
            .filter(|(start, _)| *start > time || (edge_ok && *start == time))
            .min_by_key(|(start, _)| *start)
    };
    match selected {
        Some((start, duration)) => {
            write(memory, gpr[9], ppc_qt_to_movie_scale(quicktime, start));
            write(memory, gpr[10], ppc_qt_to_movie_scale(quicktime, duration));
        }
        None => {
            // No interesting time: interestingTime is set to -1.
            write(memory, gpr[9], -1);
            write(memory, gpr[10], 0);
        }
    }
    ppc_qt_record_error(quicktime, PPC_NO_ERR);
}

/// Whether a display pixel lies inside the movie's display clip region.
pub(crate) fn ppc_qt_display_clip_contains(quicktime: &PpcQuickTimeState, x: i32, y: i32) -> bool {
    quicktime
        .movie_display_clip
        .as_deref()
        .map_or(true, |clip| {
            ppc_point_in_region_storage(
                clip,
                y.clamp(-32768, 32767) as i16,
                x.clamp(-32768, 32767) as i16,
            )
        })
}
