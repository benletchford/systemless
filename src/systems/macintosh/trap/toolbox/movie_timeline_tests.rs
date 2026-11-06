use super::*;
use crate::systems::macintosh::movie_timeline::{TrackEdit, TrackTimeline};

#[test]
fn classic_movie_timeline_repeats_decoded_samples_and_seeks_without_changing_time() {
    let (mut dispatcher, mut bus, movie, pixels) = classic_smc_movie_fixture();
    let state = dispatcher.movie_states.get_mut(&movie).unwrap();
    state.media.as_mut().unwrap().timeline = Some(TrackTimeline {
        edits: [(1, 0), (1, 0), (1, 0), (1, 0), (1, 1), (1, 1)]
            .into_iter()
            .map(|(duration, media_time)| TrackEdit {
                duration,
                media_time,
                rate: 65_536,
            })
            .collect(),
    });
    let before_time = state.current_time;
    for time in [0, 1, 2, 3, 4, 5, 1, 4] {
        dispatcher.render_movie_frame(&mut bus, movie, time);
        assert_eq!(bus.read_long(pixels), 0x00F01008);
        assert_eq!(
            bus.read_long(pixels + 16),
            if time >= 4 { 0x000820E0 } else { 0x00F01008 }
        );
        assert_eq!(dispatcher.movie_states[&movie].current_time, before_time);
    }
}
