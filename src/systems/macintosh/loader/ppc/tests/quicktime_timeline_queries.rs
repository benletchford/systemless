use super::*;

fn repeated_movie() -> PpcLoadedApp {
    let mut loaded = movie_toolbox_fixture();
    let mut bytes = loaded.quicktime.movie_data.clone();
    let payload =
        |data: &[u8], kind: &[u8]| data.windows(4).position(|part| part == kind).unwrap() + 4;
    let stts = payload(&bytes, b"stts");
    bytes[stts + 12..stts + 16].copy_from_slice(&30u32.to_be_bytes());
    let mdhd = payload(&bytes, b"mdhd");
    bytes[mdhd + 16..mdhd + 20].copy_from_slice(&60u32.to_be_bytes());
    let mvhd = payload(&bytes, b"mvhd");
    bytes[mvhd + 16..mvhd + 20].copy_from_slice(&360u32.to_be_bytes());
    let tkhd = payload(&bytes, b"tkhd");
    bytes[tkhd + 20..tkhd + 24].copy_from_slice(&360u32.to_be_bytes());
    let mut rows = vec![(30u32, 0i32); 7];
    rows.push((60, 0));
    rows.extend([(30, 30); 3]);
    let mut elst = vec![0, 0, 0, 0, 0, 0, 0, 11];
    for (duration, start) in rows {
        elst.extend(duration.to_be_bytes());
        elst.extend(start.to_be_bytes());
        elst.extend(65_536i32.to_be_bytes());
    }
    let mut edit = Vec::new();
    push_quicktime_atom(&mut edit, b"elst", &elst);
    let mut edts = Vec::new();
    push_quicktime_atom(&mut edts, b"edts", &edit);
    let track = payload(&bytes, b"trak") - 8;
    let moov = payload(&bytes, b"moov") - 8;
    let track_size = u32::from_be_bytes(bytes[track..track + 4].try_into().unwrap());
    let moov_size = u32::from_be_bytes(bytes[moov..moov + 4].try_into().unwrap());
    let end = track + track_size as usize;
    bytes.splice(end..end, edts.iter().copied());
    bytes[track..track + 4].copy_from_slice(&(track_size + edts.len() as u32).to_be_bytes());
    bytes[moov..moov + 4].copy_from_slice(&(moov_size + edts.len() as u32).to_be_bytes());
    loaded.quicktime.movie_tracks =
        crate::systems::macintosh::loader::ppc::quicktime::ppc_qt_movie_tracks(&bytes);
    loaded.quicktime.movie_tracks[1].enabled = false;
    loaded.quicktime.movie_data = bytes;
    loaded.quicktime.movie_duration = 360;
    loaded
}

#[test]
fn quicktime_movie_data_size_counts_every_repeated_sample_use() {
    let mut loaded = repeated_movie();
    // Two four-byte media samples have twelve presentations across eleven
    // edits. Apple IM: QuickTime (1993) 2-223 counts every use, not unique bytes.
    for (start, duration, expected) in [(0, 360, 48), (0, 240, 32), (240, 120, 16), (245, 1, 4)] {
        assert_eq!(
            call_movie_toolbox(
                &mut loaded,
                "GetMovieDataSize",
                &[PPC_QT_MOVIE, start, duration]
            ),
            expected
        );
    }
}

#[test]
fn quicktime_interesting_times_include_all_repeated_sample_and_edit_boundaries() {
    let mut loaded = repeated_movie();
    let base = PPC_DATA_BASE + 0x4000;
    loaded
        .memory
        .write_u32_be(base, u32::from_be_bytes(*b"vide"))
        .unwrap();
    for (flags, time, rate, expected, duration) in [
        (1, 0, 0x10000, 30i32, 30),
        (1, 225, 0x10000, 240, 30),
        (1, 245, 0xffff0000, 240, 30),
        (2, 200, 0x10000, 210, 60),
        (4, 250, 0x10000, 270, 30),
        (4 | 0x4000, 300, 0x10000, 300, 30),
        (1, 355, 0x10000, -1, 0),
    ] {
        call_movie_toolbox(
            &mut loaded,
            "GetMovieNextInterestingTime",
            &[PPC_QT_MOVIE, flags, 1, base, time, rate, base + 4, base + 8],
        );
        assert_eq!(
            loaded.memory.read_u32_be(base + 4),
            Some(expected as u32),
            "flags {flags} time {time}"
        );
        assert_eq!(loaded.memory.read_u32_be(base + 8), Some(duration));
    }
}
