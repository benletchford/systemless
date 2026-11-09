use super::*;

fn atom(kind: &[u8; 4], payload: &[u8]) -> Vec<u8> {
    let mut bytes = ((payload.len() + 8) as u32).to_be_bytes().to_vec();
    bytes.extend_from_slice(kind);
    bytes.extend_from_slice(payload);
    bytes
}

fn edited_movie() -> Vec<u8> {
    let mut description = vec![0; 86];
    description[0..4].copy_from_slice(&86u32.to_be_bytes());
    description[4..8].copy_from_slice(b"cvid");
    description[32..34].copy_from_slice(&4u16.to_be_bytes());
    description[34..36].copy_from_slice(&4u16.to_be_bytes());
    description[82..84].copy_from_slice(&24u16.to_be_bytes());
    description[84..86].copy_from_slice(&(-1i16).to_be_bytes());
    let mut stsd = vec![0, 0, 0, 0, 0, 0, 0, 1];
    stsd.extend(description);
    let words = |values: &[u32]| {
        values
            .iter()
            .flat_map(|value| value.to_be_bytes())
            .collect::<Vec<_>>()
    };
    let mut stbl = atom(b"stsd", &stsd);
    stbl.extend(atom(b"stts", &words(&[0, 1, 2, 300])));
    stbl.extend(atom(b"stsc", &words(&[0, 1, 1, 2, 1])));
    stbl.extend(atom(b"stsz", &words(&[0, 1, 2])));
    stbl.extend(atom(b"stco", &words(&[0, 1, 8])));
    let mut mdhd = vec![0; 24];
    mdhd[12..16].copy_from_slice(&600u32.to_be_bytes());
    mdhd[16..20].copy_from_slice(&600u32.to_be_bytes());
    let mut handler = vec![0; 24];
    handler[8..12].copy_from_slice(b"vide");
    let mut mdia = atom(b"mdhd", &mdhd);
    mdia.extend(atom(b"hdlr", &handler));
    mdia.extend(atom(b"minf", &atom(b"stbl", &stbl)));
    let mut elst = vec![0, 0, 0, 0, 0, 0, 0, 11];
    let mut rows = vec![(300u32, 0i32); 7];
    rows.push((600, 0));
    rows.extend([(300, 300); 3]);
    for (duration, media_time) in rows {
        elst.extend(duration.to_be_bytes());
        elst.extend(media_time.to_be_bytes());
        elst.extend(65_536i32.to_be_bytes());
    }
    let mut trak = atom(b"mdia", &mdia);
    trak.extend(atom(b"edts", &atom(b"elst", &elst)));
    let mut mvhd = vec![0; 100];
    mvhd[12..16].copy_from_slice(&600u32.to_be_bytes());
    mvhd[16..20].copy_from_slice(&3600u32.to_be_bytes());
    let mut moov = atom(b"mvhd", &mvhd);
    moov.extend(atom(b"trak", &trak));
    let mut file = atom(b"mdat", &[0, 0]);
    file.extend(atom(b"moov", &moov));
    file
}

#[test]
fn powerpc_movie_timeline_parses_repeats_and_selects_at_original_logo_hold_boundary() {
    let bytes = edited_movie();
    let mut samples = ppc_qt_scan_movie_video_samples(&bytes, 0, bytes.len(), 0).unwrap();
    assert_eq!(samples.timeline.as_ref().unwrap().edits.len(), 11);
    let mut movie = PpcQuickTimeState {
        movie_duration: 3600,
        movie_time_scale: 600,
        movie_creation_time_scale: 600,
        movie_tasks_until_done: 3600,
        ..PpcQuickTimeState::default()
    };
    for time in 0..3600 {
        movie.movie_task_count = time;
        assert_eq!(
            ppc_qt_movie_timed_sample_index(&movie, &samples),
            Some(usize::from(time >= 2400)),
            "time {time}"
        );
    }
    for time in [2400, 299, 300, 2399] {
        movie.movie_task_count = time;
        assert_eq!(
            ppc_qt_movie_timed_sample_index(&movie, &samples),
            Some(usize::from(time >= 2400))
        );
    }
    // Without edits, media advances at its own time scale, rather than being
    // stretched to the six-second duration of the overall movie.
    samples.timeline = None;
    movie.movie_task_count = 300;
    assert_eq!(ppc_qt_movie_timed_sample_index(&movie, &samples), Some(1));
}
