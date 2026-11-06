use super::*;

fn atom(kind: &[u8; 4], payload: &[u8]) -> Vec<u8> {
    [
        (payload.len() as u32 + 8).to_be_bytes().as_slice(),
        kind,
        payload,
    ]
    .concat()
}
fn words(values: &[u32]) -> Vec<u8> {
    values
        .iter()
        .flat_map(|value| value.to_be_bytes())
        .collect()
}
pub(crate) fn fixture(
    codec: &[u8; 4],
    channels: u16,
    bits: u16,
    samples: &[u8],
    cuts: &[usize],
    edits: Option<&[(u32, i32, i32)]>,
) -> (Vec<u8>, Vec<u8>) {
    let frame_bytes = usize::from(channels * bits / 8);
    let count: usize = cuts.iter().sum();
    assert_eq!(samples.len(), count * frame_bytes);
    let mut data = vec![0x55; 5];
    let mut offsets = vec![0, cuts.len() as u32];
    let mut mappings = vec![0, cuts.len() as u32];
    let mut cursor = 0;
    for (index, &n) in cuts.iter().enumerate() {
        offsets.push(data.len() as u32);
        mappings.extend([index as u32 + 1, n as u32, 1]);
        data.extend_from_slice(&samples[cursor..cursor + n * frame_bytes]);
        cursor += n * frame_bytes;
        data.extend([0x77; 3]);
    }
    let mut description = vec![0; 36];
    description[..4].copy_from_slice(&36u32.to_be_bytes());
    description[4..8].copy_from_slice(codec);
    description[14..16].copy_from_slice(&1u16.to_be_bytes());
    description[24..26].copy_from_slice(&channels.to_be_bytes());
    description[26..28].copy_from_slice(&bits.to_be_bytes());
    description[32..36].copy_from_slice(&(4u32 << 16).to_be_bytes());
    let stsd = [words(&[0, 1]), description].concat();
    let stbl = [
        atom(b"stsd", &stsd),
        atom(b"stsc", &words(&mappings)),
        atom(b"stco", &words(&offsets)),
        atom(b"stsz", &words(&[0, frame_bytes as u32, count as u32])),
        atom(b"stts", &words(&[0, 1, count as u32, 1])),
    ]
    .concat();
    let mut mdhd = vec![0; 24];
    mdhd[12..16].copy_from_slice(&4u32.to_be_bytes());
    let mut hdlr = vec![0; 24];
    hdlr[8..12].copy_from_slice(b"soun");
    let mdia = [
        atom(b"mdhd", &mdhd),
        atom(b"hdlr", &hdlr),
        atom(b"minf", &atom(b"stbl", &stbl)),
    ]
    .concat();
    let mut tkhd = vec![0; 84];
    tkhd[3] = 1;
    tkhd[36..38].copy_from_slice(&256u16.to_be_bytes());
    let mut trak = [atom(b"tkhd", &tkhd), atom(b"mdia", &mdia)].concat();
    if let Some(edits) = edits {
        let mut elst = words(&[0, edits.len() as u32]);
        for &(duration, media, rate) in edits {
            elst.extend(words(&[duration, media as u32, rate as u32]));
        }
        trak.extend(atom(b"edts", &atom(b"elst", &elst)));
    }
    (atom(b"moov", &atom(b"trak", &trak)), data)
}

#[test]
fn pcm_decodes_signed_unsigned_big_and_little_endian_without_downmixing() {
    for (codec, channels, bits, bytes, expected) in [
        (
            b"raw ",
            1,
            8,
            vec![0, 128, 255],
            vec![[-32768; 2], [0; 2], [32512; 2]],
        ),
        (
            b"twos",
            1,
            8,
            vec![128, 0, 127],
            vec![[-32768; 2], [0; 2], [32512; 2]],
        ),
        (
            b"twos",
            2,
            16,
            vec![0x80, 0, 0x7f, 0xff],
            vec![[-32768, 32767]],
        ),
        (
            b"sowt",
            2,
            16,
            vec![0, 0x80, 0xff, 0x7f],
            vec![[-32768, 32767]],
        ),
    ] {
        let n = expected.len();
        let (moov, data) = fixture(codec, channels, bits, &bytes, &[n], None);
        let tracks = parse_tracks(&moov, &data, 4);
        assert_eq!(tracks.len(), 1);
        assert_eq!(tracks[0].frames, expected);
    }
}

#[test]
fn pcm_chunk_mappings_skip_padding_and_keep_all_frames() {
    let (moov, data) = fixture(b"raw ", 1, 8, &[1, 2, 3, 4, 5], &[2, 3], None);
    let tracks = parse_tracks(&moov, &data, 4);
    assert_eq!(
        tracks[0]
            .frames
            .iter()
            .map(|frame| ((frame[0] >> 8) + 128) as u8)
            .collect::<Vec<_>>(),
        vec![1, 2, 3, 4, 5]
    );
}

#[test]
fn pcm_edits_keep_sub_tick_precision_and_handle_empty_repeat_reverse_and_dwell() {
    let edits = [
        (4, -1, 65536),
        (4, 0, 65536),
        (4, -1, 65536),
        (4, 0, 65536),
        (4, 3, -65536),
        (4, 1, 0),
    ];
    let (moov, data) = fixture(b"raw ", 1, 8, &[129, 130, 131, 132], &[2, 2], Some(&edits));
    let track = parse_tracks(&moov, &data, 4).remove(0);
    for (time, expected) in [
        (0.0, None),
        (0.999, None),
        (1.0, Some(256)),
        (1.249, Some(256)),
        (1.25, Some(512)),
        (2.0, None),
        (3.0, Some(256)),
        (4.0, Some(1024)),
        (4.25, Some(768)),
        (5.0, Some(512)),
        (5.75, Some(512)),
        (6.0, None),
    ] {
        assert_eq!(
            track.sample(time).map(|frame| frame[0]),
            expected,
            "at {time}"
        );
    }
    // Fractional movie units must not hold each audio sample for a whole tick.
    let (moov, data) = fixture(
        b"raw ",
        1,
        8,
        &[129, 130, 131, 132],
        &[4],
        Some(&[(600, 0, 65536)]),
    );
    let track = parse_tracks(&moov, &data, 600).remove(0);
    assert_eq!(track.sample(0.25), Some([512; 2]));
}

#[test]
fn pcm_mix_preserves_stereo_gain_existing_audio_and_silence() {
    let (moov, data) = fixture(b"raw ", 2, 8, &[32, 224], &[1], None);
    let mut track = parse_tracks(&moov, &data, 4).remove(0);
    let mut output = vec![128; 2];
    track.mix(0.0, 1.0, 256, &mut output);
    assert_eq!(output, [32, 224]);
    track.volume = 128;
    let mut output = vec![128; 2];
    track.mix(0.0, 1.0, 128, &mut output);
    assert_eq!(output, [104, 152]);
    for volume in [0, -1] {
        let mut output = vec![77, 88];
        track.mix(0.0, 1.0, volume, &mut output);
        assert_eq!(output, [77, 88]);
    }
    let mut output = vec![16, 240];
    track.mix(0.0, 1.0, 32767, &mut output);
    assert_eq!(output, [0, 255]);
}

#[test]
fn pcm_rejects_unsupported_truncated_disabled_and_unbounded_tables() {
    let (moov, data) = fixture(b"raw ", 1, 8, &[129, 130], &[2], None);
    for (kind, offset, bytes) in [
        (b"stsz", 8, u32::MAX.to_be_bytes().to_vec()),
        (b"stco", 8, u32::MAX.to_be_bytes().to_vec()),
        (b"stsc", 12, 0u32.to_be_bytes().to_vec()),
        (b"tkhd", 0, vec![0; 4]),
        (b"stsd", 12, b"ima4".to_vec()),
    ] {
        let mut bad = moov.clone();
        let start = bad.windows(4).position(|value| value == kind).unwrap() + 4 + offset;
        bad[start..start + bytes.len()].copy_from_slice(&bytes);
        assert!(parse_tracks(&bad, &data, 4).is_empty(), "{kind:?}");
    }
    assert!(parse_tracks(&moov, &data[..5], 4).is_empty());
    assert!(parse_tracks(&moov[..moov.len() - 1], &data, 4).is_empty());
}

#[test]
fn pcm_external_movie_matches_independent_unsigned_pcm_when_configured() {
    let Ok(path) = std::env::var("SYSTEMLESS_TEST_PCM_MOVIE") else {
        return;
    };
    let oracle = std::env::var("SYSTEMLESS_TEST_PCM_ORACLE").expect("PCM oracle path");
    let data = std::fs::read(path).unwrap();
    let expected = std::fs::read(oracle).unwrap();
    let tracks = parse_tracks(&data, &data, 600);
    assert_eq!(tracks.len(), 1);
    let actual = tracks[0]
        .frames
        .iter()
        .map(|frame| {
            assert_eq!(frame[0], frame[1], "mono source");
            ((frame[0] >> 8) + 128) as u8
        })
        .collect::<Vec<_>>();
    assert_eq!(actual.len(), expected.len());
    assert_eq!(actual, expected);
}
