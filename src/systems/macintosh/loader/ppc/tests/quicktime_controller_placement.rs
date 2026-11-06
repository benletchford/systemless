use super::*;

#[test]
fn quicktime_controller_creation_places_movie_and_survives_movie_changed() {
    let mut loaded = movie_toolbox_fixture();
    let rect = PPC_DATA_BASE + 0x4000;
    ppc_write_rect(&mut loaded.memory, rect, 110, 80, 350, 400).unwrap();
    let before_time = loaded.quicktime.movie_task_count;
    let controller =
        call_movie_toolbox(&mut loaded, "NewMovieController", &[PPC_QT_MOVIE, rect, 11]);
    assert_ne!(controller, 0);
    assert_eq!(loaded.quicktime.movie_box, (110, 80, 350, 400));
    assert_eq!(
        loaded.quicktime.movie_controllers[0].rect,
        (110, 80, 350, 400)
    );
    assert!(!loaded.quicktime.movie_started);
    assert_eq!(loaded.quicktime.movie_task_count, before_time);
    assert_eq!(
        call_movie_toolbox(&mut loaded, "MCMovieChanged", &[controller, PPC_QT_MOVIE]),
        0
    );
    assert_eq!(
        loaded.quicktime.movie_controllers[0].rect,
        (110, 80, 350, 400)
    );
    loaded.cpu.gpr[3] = PPC_QT_MOVIE;
    loaded.cpu.gpr[4] = rect + 8;
    assert_eq!(
        ppc_qt_get_movie_box(&mut loaded.cpu, &mut loaded.memory, &loaded.quicktime),
        0
    );
    assert_eq!(
        ppc_read_rect(&mut loaded.memory, rect + 8),
        Some((110, 80, 350, 400))
    );
}

#[test]
fn quicktime_controller_fit_preserves_aspect_and_centres_or_uses_top_left() {
    for (flags, expected) in [(8, (10, 40, 100, 160)), (11, (10, 10, 100, 130))] {
        let mut loaded = movie_toolbox_fixture();
        let rect = PPC_DATA_BASE + 0x4000;
        // 320x240 movie in a 180x90 display rectangle becomes 120x90.
        ppc_write_rect(&mut loaded.memory, rect, 10, 10, 100, 190).unwrap();
        let controller = call_movie_toolbox(
            &mut loaded,
            "NewMovieController",
            &[PPC_QT_MOVIE, rect, flags],
        );
        assert_ne!(controller, 0);
        assert_eq!(loaded.quicktime.movie_box, expected);
        assert_eq!(loaded.quicktime.movie_controllers[0].rect, expected);
    }
}

#[test]
fn quicktime_controller_creation_composes_translation_without_moving_source_bounds() {
    let (mut loaded, gworld, pixels) = cinepak_movie_draw_fixture();
    loaded.quicktime.movie_gworld = gworld;
    let rect = PPC_DATA_BASE + 0x4000;
    loaded.memory.add_region(rect, vec![0; 8]);
    // A 4x4 source, drawn at 2,2 into the existing 4x4 surface. The visible
    // upper-left source block is black; everything outside the box is intact.
    ppc_write_rect(&mut loaded.memory, rect, 2, 2, 6, 6).unwrap();
    let controller =
        call_movie_toolbox(&mut loaded, "NewMovieController", &[PPC_QT_MOVIE, rect, 11]);
    assert_eq!(loaded.quicktime.movie_natural_bounds, Some((0, 0, 4, 4)));
    assert_eq!(
        call_movie_toolbox(&mut loaded, "MCMovieChanged", &[controller, PPC_QT_MOVIE]),
        0
    );
    assert_eq!(
        call_movie_toolbox(&mut loaded, "MCDoAction", &[controller, 2, gworld]),
        0
    );
    assert_eq!(loaded.memory.read_u16_be(pixels), Some(0xeeee));
    assert_eq!(loaded.memory.read_u16_be(pixels + 2 * 8 + 2 * 2), Some(0));
    assert_eq!(loaded.memory.read_u16_be(pixels + 3 * 8 + 3 * 2), Some(0));
}

#[test]
fn quicktime_controller_invalid_rect_does_not_change_movie_or_allocate_handle() {
    let mut loaded = movie_toolbox_fixture();
    let rect = PPC_DATA_BASE + 0x4000;
    let before = loaded.quicktime.clone();
    ppc_write_rect(&mut loaded.memory, rect, 20, 30, 20, 90).unwrap();
    assert_eq!(
        call_movie_toolbox(&mut loaded, "NewMovieController", &[PPC_QT_MOVIE, rect, 11]),
        0
    );
    assert_eq!(loaded.quicktime.movie_box, before.movie_box);
    assert_eq!(loaded.quicktime.movie_matrix, before.movie_matrix);
    assert_eq!(
        loaded.quicktime.next_movie_controller_handle,
        before.next_movie_controller_handle
    );
    assert!(loaded.quicktime.movie_controllers.is_empty());
}
