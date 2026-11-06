use super::super::dispatch::TrapDispatcher;
use super::super::test_helpers::{setup, MockCpu, TEST_SP};
use crate::cpu::{CpuOps, Register};
use crate::memory::{MacMemoryBus, MemoryBus};
fn classic_controller_fixture() -> (TrapDispatcher, MockCpu, MacMemoryBus, u32, u32, u32) {
    use super::super::movie_media::{MediaSample, VideoTrack};
    let (mut disp, cpu, mut bus) = setup();
    let (movie, port, handle, pixmap, pixels) = (0x300000, 0x300100, 0x300200, 0x300300, 0x310000);
    bus.write_word(port + 6, 0xC000);
    bus.write_long(port + 2, handle);
    bus.write_long(handle, pixmap);
    bus.write_long(pixmap, pixels);
    bus.write_word(pixmap + 4, 0x8000 | 64);
    bus.write_word(pixmap + 10, 10);
    bus.write_word(pixmap + 12, 16);
    bus.write_word(pixmap + 32, 32);
    let frame = |colors: [u8; 2]| {
        let mut bytes = vec![0, 0, 0, 20, 0, 0];
        for color in colors {
            bytes.extend([1, 1, color, color, color, color, 0xFF]);
        }
        bytes
    };
    let mut state = super::MovieState::new(0, -1, 1, (0, 0, 2, 4), 60, 60);
    state.gworld_port = port;
    state.data_fork = [frame([1, 2]), frame([2, 1])].concat();
    state.media = Some(VideoTrack {
        codec: *b"rle ",
        width: 4,
        height: 2,
        depth: 8,
        time_scale: 60,
        samples: vec![
            MediaSample {
                offset: 0,
                size: 20,
                start_time: 0,
                duration: 3,
                sync: true,
            },
            MediaSample {
                offset: 20,
                size: 20,
                start_time: 3,
                duration: 57,
                sync: true,
            },
        ],
        clut: Some(vec![[0, 0, 0], [240, 16, 8], [8, 32, 224]]),
    });
    disp.movie_states.insert(movie, state);
    (disp, cpu, bus, movie, port, pixels)
}

fn classic_controller_create(
    disp: &mut TrapDispatcher,
    cpu: &mut MockCpu,
    bus: &mut MacMemoryBus,
    movie: u32,
    bounds: (i16, i16, i16, i16),
    flags: u32,
) -> u32 {
    let rect = 0x300700;
    super::write_movie_box(bus, rect, bounds);
    cpu.write_reg(Register::A7, TEST_SP);
    cpu.write_reg(Register::D0, 0x018A);
    bus.write_long(TEST_SP, flags);
    bus.write_long(TEST_SP + 4, rect);
    bus.write_long(TEST_SP + 8, movie);
    bus.write_long(TEST_SP + 12, 0xBAD);
    bus.write_long(TEST_SP + 16, 0xCAFE_BABE);
    disp.dispatch_toolbox(true, 0x2AA, cpu, bus)
        .unwrap()
        .unwrap();
    assert_eq!(cpu.read_reg(Register::A7), TEST_SP + 12);
    assert_eq!(bus.read_long(TEST_SP + 16), 0xCAFE_BABE);
    assert_eq!(super::read_movie_box(bus, rect), Some(bounds));
    let instance = bus.read_long(TEST_SP + 12);
    assert_eq!(cpu.read_reg(Register::D0), instance);
    instance
}

fn classic_controller_component(
    disp: &mut TrapDispatcher,
    cpu: &mut MockCpu,
    bus: &mut MacMemoryBus,
    instance: u32,
    selector: u16,
    arguments: &[u8],
) -> i32 {
    let size = arguments.len() as u32;
    cpu.write_reg(Register::A7, TEST_SP);
    cpu.write_reg(Register::D0, 0);
    bus.write_word(TEST_SP, size as u16);
    bus.write_word(TEST_SP + 2, selector);
    bus.write_bytes(TEST_SP + 4, arguments);
    bus.write_long(TEST_SP + 4 + size, instance);
    bus.write_long(TEST_SP + 8 + size, 0xBAD);
    bus.write_long(TEST_SP + 12 + size, 0xCAFE_BABE);
    disp.dispatch_toolbox(true, 0x02A, cpu, bus)
        .unwrap()
        .unwrap();
    assert_eq!(cpu.read_reg(Register::A7), TEST_SP + 8 + size);
    assert_eq!(bus.read_long(TEST_SP + 12 + size), 0xCAFE_BABE);
    let result = bus.read_long(TEST_SP + 8 + size) as i32;
    assert_eq!(cpu.read_reg(Register::D0), result as u32);
    result
}

fn classic_controller_action(
    disp: &mut TrapDispatcher,
    cpu: &mut MockCpu,
    bus: &mut MacMemoryBus,
    instance: u32,
    action: u16,
    param: u32,
) -> i32 {
    let arguments = [
        param.to_be_bytes().as_slice(),
        action.to_be_bytes().as_slice(),
    ]
    .concat();
    classic_controller_component(disp, cpu, bus, instance, 9, &arguments)
}

#[test]
fn classic_controller_creation_owns_state_and_fits_the_movie() {
    let (mut disp, mut cpu, mut bus, movie, port, _) = classic_controller_fixture();
    let controller =
        classic_controller_create(&mut disp, &mut cpu, &mut bus, movie, (2, 3, 6, 11), 11);
    assert_ne!(controller, 0);
    assert!(disp.synthetic_component_instances.contains(&controller));
    let state = disp.movie_controllers[&controller];
    assert_eq!(
        (state.movie, state.port, state.bounds, state.creation_flags),
        (movie, port, (2, 3, 6, 11), 11)
    );
    assert!(!state.keys_enabled);
    assert_eq!(state.control_flags, 0);
    assert_eq!(disp.movie_states[&movie].box_rect, (2, 3, 6, 11));
    assert_eq!(disp.movie_states[&movie].rate, 0);
    // Default creation centers the unscaled movie when enlargement is disabled.
    disp.movie_states.get_mut(&movie).unwrap().box_rect = (0, 0, 2, 4);
    let second = classic_controller_create(&mut disp, &mut cpu, &mut bus, movie, (2, 3, 6, 11), 0);
    assert_ne!(second, controller);
    assert_eq!(disp.movie_states[&movie].box_rect, (3, 5, 5, 9));
}

#[test]
fn classic_controller_draw_repairs_pixels_without_changing_time_or_rate() {
    let (mut disp, mut cpu, mut bus, movie, port, pixels) = classic_controller_fixture();
    let controller =
        classic_controller_create(&mut disp, &mut cpu, &mut bus, movie, (2, 3, 6, 11), 11);
    assert_eq!(
        classic_controller_action(&mut disp, &mut cpu, &mut bus, controller, 8, 65536),
        0
    );
    assert_eq!(
        classic_controller_action(&mut disp, &mut cpu, &mut bus, controller, 2, port),
        0
    );
    assert_eq!(bus.read_long(pixels + 2 * 64 + 3 * 4), 0x00F01008);
    assert_eq!(bus.read_long(pixels + 5 * 64 + 10 * 4), 0x000820E0);
    assert_eq!(bus.read_long(pixels + 2 * 64 + 2 * 4), 0);
    assert_eq!(bus.read_long(pixels + 6 * 64 + 3 * 4), 0);
    assert_eq!(
        (
            disp.movie_states[&movie].current_time,
            disp.movie_states[&movie].rate
        ),
        (0, 65536)
    );
    bus.write_long(pixels + 2 * 64 + 3 * 4, 0xBAD);
    assert_eq!(
        classic_controller_component(
            &mut disp,
            &mut cpu,
            &mut bus,
            controller,
            0x18,
            &port.to_be_bytes()
        ),
        1
    );
    assert_eq!(bus.read_long(pixels + 2 * 64 + 3 * 4), 0x00F01008);
    bus.write_long(pixels + 2 * 64 + 3 * 4, 0xBAD);
    assert_eq!(
        classic_controller_action(&mut disp, &mut cpu, &mut bus, controller, 2, port + 0x1000),
        0
    );
    assert_eq!(bus.read_long(pixels + 2 * 64 + 3 * 4), 0xBAD);
    assert_eq!(disp.movie_states[&movie].rate, 65536);
    super::write_movie_box(&mut bus, 0x300700, (0, 0, 2, 4));
    cpu.write_reg(Register::D0, 0x00FA);
    cpu.write_reg(Register::A7, TEST_SP);
    bus.write_long(TEST_SP, 0x300700);
    bus.write_long(TEST_SP + 4, movie);
    disp.dispatch_toolbox(true, 0x2AA, &mut cpu, &mut bus)
        .unwrap()
        .unwrap();
    assert_eq!(
        classic_controller_action(&mut disp, &mut cpu, &mut bus, controller, 1, 0),
        0
    );
    assert_eq!(bus.read_long(pixels), 0x00F01008);
    disp.movie_states.get_mut(&movie).unwrap().active = false;
    assert_eq!(
        classic_controller_action(&mut disp, &mut cpu, &mut bus, controller, 2, port),
        0
    );
    assert_eq!(bus.read_long(pixels + 2 * 64 + 3 * 4), 0xBAD);
}

#[test]
fn classic_controller_idle_services_only_its_movie_and_zero_rate_stops() {
    let (mut disp, mut cpu, mut bus, movie, _, pixels) = classic_controller_fixture();
    let controller =
        classic_controller_create(&mut disp, &mut cpu, &mut bus, movie, (0, 0, 2, 4), 11);
    let mut other = disp.movie_states[&movie].clone();
    other.rate = 65536;
    other.last_service_tick = Some(0);
    disp.movie_states.insert(0x300800, other);
    disp.set_tick_count_for_test(&mut bus, 0);
    assert_eq!(
        classic_controller_action(&mut disp, &mut cpu, &mut bus, controller, 8, 65536),
        0
    );
    disp.set_tick_count_for_test(&mut bus, 6);
    assert_eq!(
        classic_controller_component(&mut disp, &mut cpu, &mut bus, controller, 0x1A, &[]),
        1
    );
    assert_eq!(disp.movie_states[&movie].current_time, 6);
    assert_eq!(disp.movie_states[&0x300800].current_time, 0);
    assert_eq!(bus.read_long(pixels), 0x000820E0);
    disp.set_tick_count_for_test(&mut bus, 8);
    assert_eq!(
        classic_controller_action(&mut disp, &mut cpu, &mut bus, controller, 1, 0),
        0
    );
    assert_eq!(disp.movie_states[&movie].current_time, 8);
    assert_eq!(
        classic_controller_action(&mut disp, &mut cpu, &mut bus, controller, 8, 0),
        0
    );
    disp.set_tick_count_for_test(&mut bus, 10);
    assert_eq!(
        classic_controller_action(&mut disp, &mut cpu, &mut bus, controller, 1, 0),
        0
    );
    assert_eq!(disp.movie_states[&movie].current_time, 8);
}

#[test]
fn classic_controller_parameter_widths_flags_and_time_record_are_checked() {
    let (mut disp, mut cpu, mut bus, movie, _, _) = classic_controller_fixture();
    let controller =
        classic_controller_create(&mut disp, &mut cpu, &mut bus, movie, (0, 0, 2, 4), 11);
    let out = 0x300900;
    for (set, get, value, width) in [(32, 33, 1, 1), (38, 39, 0xA5A55A5A, 4), (14, 15, 0xFF80, 2)] {
        bus.write_long(out, 0xCCCCCCCC);
        assert_eq!(
            classic_controller_action(&mut disp, &mut cpu, &mut bus, controller, set, value),
            0
        );
        assert_eq!(
            classic_controller_action(&mut disp, &mut cpu, &mut bus, controller, get, out),
            0
        );
        let expected = match width {
            1 => 0x01CCCCCC,
            2 => 0xFF80CCCC,
            _ => value,
        };
        assert_eq!(bus.read_long(out), expected);
        let bad = bus.ram_size() - width + 1;
        assert_eq!(
            classic_controller_action(&mut disp, &mut cpu, &mut bus, controller, get, bad),
            -50
        );
    }
    assert_eq!(disp.movie_controllers[&controller].creation_flags, 11);
    assert_eq!(disp.movie_states[&movie].volume, -128);
    bus.write_long(out, 0);
    bus.write_long(out + 4, 12);
    bus.write_long(out + 8, 120);
    bus.write_long(out + 12, movie);
    assert_eq!(
        classic_controller_action(&mut disp, &mut cpu, &mut bus, controller, 12, out),
        0
    );
    assert_eq!(disp.movie_states[&movie].current_time, 6);
    bus.write_long(out + 8, 0);
    assert_eq!(
        classic_controller_action(&mut disp, &mut cpu, &mut bus, controller, 12, out),
        -50
    );
    assert_eq!(disp.movie_states[&movie].current_time, 6);
    assert_eq!(
        classic_controller_action(&mut disp, &mut cpu, &mut bus, controller, 123, 0),
        -3000
    );
}

#[test]
fn classic_controller_invalid_inputs_and_disposal_preserve_movie_ownership() {
    let (mut disp, mut cpu, mut bus, movie, _, _) = classic_controller_fixture();
    assert_eq!(
        classic_controller_create(&mut disp, &mut cpu, &mut bus, 0xBAD, (0, 0, 2, 4), 11),
        0
    );
    assert_eq!(disp.movie_error, -2010);
    assert_eq!(
        classic_controller_create(&mut disp, &mut cpu, &mut bus, movie, (2, 0, 1, 4), 11),
        0
    );
    assert!(disp.movie_controllers.is_empty());
    let controller =
        classic_controller_create(&mut disp, &mut cpu, &mut bus, movie, (0, 0, 2, 4), 11);
    assert_eq!(
        classic_controller_component(&mut disp, &mut cpu, &mut bus, controller, 9, &[0; 4]),
        -50
    );
    cpu.write_reg(Register::A7, TEST_SP);
    cpu.write_reg(Register::D0, 0x018B);
    bus.write_long(TEST_SP, controller);
    disp.dispatch_toolbox(true, 0x2AA, &mut cpu, &mut bus)
        .unwrap()
        .unwrap();
    assert_eq!(cpu.read_reg(Register::A7), TEST_SP + 4);
    assert!(!disp.movie_controllers.contains_key(&controller));
    assert!(!disp.synthetic_component_instances.contains(&controller));
    assert!(disp.movie_states.contains_key(&movie));
    assert_eq!(
        classic_controller_action(&mut disp, &mut cpu, &mut bus, controller, 8, 65536),
        -3000
    );
}

#[test]
fn classic_controller_rgb_blit_scales_and_clips_without_writing_adjacent_rows() {
    let (disp, _, mut bus, _, port, pixels) = classic_controller_fixture();
    let rgb = [255, 0, 0, 0, 0, 255];
    disp.blit_rgb_into_port(&mut bus, port, (-1, -2, 1, 2), 2, 1, &rgb);
    assert_eq!(bus.read_long(pixels), 0x000000FF);
    assert_eq!(bus.read_long(pixels + 4), 0x000000FF);
    assert_eq!(bus.read_long(pixels + 8), 0);
    assert_eq!(bus.read_long(pixels + 64), 0);
    // Truncated rowBytes must not spill a declared wide surface into the next row.
    bus.write_long(pixels + 4, 0x00ABCDEF);
    bus.write_word(0x300300 + 4, 0x8000 | 4);
    disp.blit_rgb_into_port(&mut bus, port, (0, 0, 1, 4), 2, 1, &rgb);
    assert_eq!(bus.read_long(pixels), 0x00FF0000);
    assert_eq!(bus.read_long(pixels + 4), 0x00ABCDEF);
}

#[test]
fn classic_controller_attach_uses_the_window_and_point_and_close_releases_state() {
    let (mut disp, mut cpu, mut bus, movie, port, pixels) = classic_controller_fixture();
    cpu.write_reg(Register::D0, 0x21);
    cpu.write_reg(Register::A7, TEST_SP);
    bus.write_long(TEST_SP, 0);
    bus.write_long(TEST_SP + 4, u32::from_be_bytes(*b"play"));
    disp.dispatch_toolbox(true, 0x02A, &mut cpu, &mut bus)
        .unwrap()
        .unwrap();
    let controller = bus.read_long(TEST_SP + 8);
    assert_ne!(controller, 0);
    let arguments = [
        (-1i16).to_be_bytes().as_slice(),
        2i16.to_be_bytes().as_slice(),
        port.to_be_bytes().as_slice(),
        movie.to_be_bytes().as_slice(),
    ]
    .concat();
    assert_eq!(
        classic_controller_component(&mut disp, &mut cpu, &mut bus, controller, 0x17, &arguments),
        0
    );
    assert_eq!(disp.movie_controllers[&controller].bounds, (-1, 2, 1, 6));
    assert_eq!(disp.movie_states[&movie].gworld_port, port);
    assert_eq!(
        classic_controller_action(&mut disp, &mut cpu, &mut bus, controller, 2, port),
        0
    );
    assert_eq!(bus.read_long(pixels + 2 * 4), 0x000820E0);
    cpu.write_reg(Register::D0, 8);
    cpu.write_reg(Register::A7, TEST_SP);
    bus.write_long(TEST_SP, controller);
    disp.dispatch_toolbox(true, 0x02A, &mut cpu, &mut bus)
        .unwrap()
        .unwrap();
    assert!(!disp.movie_controllers.contains_key(&controller));
    assert!(!disp.synthetic_component_instances.contains(&controller));
    assert!(disp.movie_states.contains_key(&movie));
}

#[test]
fn classic_controller_drawing_matches_indexed_and_rgb555_surfaces() {
    let (mut disp, mut cpu, mut bus, movie, port, pixels) = classic_controller_fixture();
    let controller =
        classic_controller_create(&mut disp, &mut cpu, &mut bus, movie, (0, 0, 2, 4), 11);
    let (pixmap, table_handle, table) = (0x300300, 0x300A00, 0x300B00);
    bus.write_word(pixmap + 32, 8);
    bus.write_word(pixmap + 4, 0x8000 | 16);
    bus.write_long(pixmap + 42, table_handle);
    bus.write_long(table_handle, table);
    bus.write_word(table + 4, 0x8000);
    bus.write_word(table + 6, 2);
    for (i, color) in [[0u8, 0, 0], [240, 16, 8], [8, 32, 224]].iter().enumerate() {
        let entry = table + 8 + i as u32 * 8;
        bus.write_word(entry, i as u16);
        for (j, &value) in color.iter().enumerate() {
            bus.write_word(entry + 2 + j as u32 * 2, u16::from(value) * 257);
        }
    }
    assert_eq!(
        classic_controller_action(&mut disp, &mut cpu, &mut bus, controller, 2, port),
        0
    );
    assert_eq!(bus.read_byte(pixels), 1);
    assert_eq!(bus.read_byte(pixels + 16), 2);
    assert_eq!(bus.read_byte(pixels + 4), 0);
    bus.fill_zeros(pixels, 640);
    bus.write_word(pixmap + 32, 16);
    bus.write_word(pixmap + 4, 0x8000 | 32);
    assert_eq!(
        classic_controller_action(&mut disp, &mut cpu, &mut bus, controller, 2, port),
        0
    );
    assert_eq!(bus.read_word(pixels), 0x7841);
    assert_eq!(bus.read_word(pixels + 32), 0x049C);
    assert_eq!(bus.read_word(pixels + 8), 0);
}

#[test]
fn classic_controller_movie_completion_stays_drawable_and_exit_clears_controllers() {
    let (mut disp, mut cpu, mut bus, movie, port, _) = classic_controller_fixture();
    let controller =
        classic_controller_create(&mut disp, &mut cpu, &mut bus, movie, (0, 0, 2, 4), 11);
    disp.set_tick_count_for_test(&mut bus, 0);
    assert_eq!(
        classic_controller_action(&mut disp, &mut cpu, &mut bus, controller, 8, 65536),
        0
    );
    disp.set_tick_count_for_test(&mut bus, 80);
    assert_eq!(
        classic_controller_action(&mut disp, &mut cpu, &mut bus, controller, 1, 0),
        0
    );
    assert_eq!(
        (
            disp.movie_states[&movie].current_time,
            disp.movie_states[&movie].rate
        ),
        (60, 0)
    );
    assert!(disp.movie_states[&movie].active);
    assert_eq!(
        classic_controller_action(&mut disp, &mut cpu, &mut bus, controller, 2, port),
        0
    );
    cpu.write_reg(Register::D0, 2);
    cpu.write_reg(Register::A7, TEST_SP);
    disp.dispatch_toolbox(true, 0x2AA, &mut cpu, &mut bus)
        .unwrap()
        .unwrap();
    assert!(disp.movie_states.is_empty());
    assert!(disp.movie_controllers.is_empty());
    assert!(!disp.synthetic_component_instances.contains(&controller));
}

fn classic_controller_indexed_window(disp: &mut TrapDispatcher, bus: &mut MacMemoryBus, port: u32) {
    let device = disp.ensure_main_gdevice(bus);
    let device_ptr = bus.read_long(device);
    let device_pixmap = bus.read_long(bus.read_long(device_ptr + 22));
    let table = bus.read_long(device_pixmap + 42);
    let pixmap = bus.read_long(bus.read_long(port + 2));
    bus.write_word(pixmap + 32, 8);
    bus.write_word(pixmap + 4, 0x8000 | 16);
    bus.write_long(pixmap + 42, table);
    disp.window_list.push(port);
    disp.front_window = port;
}

#[test]
fn classic_controller_requests_movie_colors_and_draws_through_the_live_device_table() {
    let (mut disp, mut cpu, mut bus, movie, port, pixels) = classic_controller_fixture();
    classic_controller_indexed_window(&mut disp, &mut bus, port);
    let controller =
        classic_controller_create(&mut disp, &mut cpu, &mut bus, movie, (0, 0, 2, 4), 11);
    let palette = disp.movie_controllers[&controller].palette.unwrap();
    assert_eq!(disp.window_palettes[&port].0, palette.handle);
    assert!(disp.device_clut.contains(&[0xf0f0, 0x1010, 0x0808]));
    assert!(disp.device_clut.contains(&[0x0808, 0x2020, 0xe0e0]));
    assert_eq!(
        classic_controller_action(&mut disp, &mut cpu, &mut bus, controller, 2, port),
        0
    );
    assert_eq!(
        disp.device_clut[bus.read_byte(pixels) as usize],
        [0xf0f0, 0x1010, 0x0808]
    );
    assert_eq!(
        disp.device_clut[bus.read_byte(pixels + 16) as usize],
        [0x0808, 0x2020, 0xe0e0]
    );
    // Idle ownership checks must not recreate palettes or change the seed.
    let pixmap = bus.read_long(bus.read_long(port + 2));
    let table = bus.read_long(bus.read_long(pixmap + 42));
    let seed = bus.read_long(table);
    disp.prepare_classic_controller_palette(&mut bus, controller);
    assert_eq!(
        disp.movie_controllers[&controller].palette.unwrap().handle,
        palette.handle
    );
    assert_eq!(bus.read_long(table), seed);
}

#[test]
fn classic_controller_palette_opt_out_preserves_manual_window_requests() {
    let (mut disp, mut cpu, mut bus, movie, port, _) = classic_controller_fixture();
    classic_controller_indexed_window(&mut disp, &mut bus, port);
    let controller =
        classic_controller_create(&mut disp, &mut cpu, &mut bus, movie, (0, 0, 2, 4), 11);
    let owned = disp.movie_controllers[&controller].palette.unwrap().handle;
    assert_eq!(
        classic_controller_action(&mut disp, &mut cpu, &mut bus, controller, 38, 8),
        0
    );
    let manual = disp.create_palette_from_ctab(&mut bus, 4, 0, 2, 0);
    disp.set_window_palette_association(port, manual, -0x2000);
    disp.prepare_classic_controller_palette(&mut bus, controller);
    assert_eq!(disp.window_palettes[&port].0, manual);
    disp.dispose_classic_movie_controller(&mut bus, controller);
    assert_eq!(disp.window_palettes[&port].0, manual);
    assert!(bus.get_alloc_size(manual).is_some());
    assert!(bus.get_alloc_size(owned).is_none());
}

#[test]
fn classic_controller_palette_owners_restore_prior_associations_in_either_disposal_order() {
    for first_before_second in [false, true] {
        let (mut disp, mut cpu, mut bus, movie, port, _) = classic_controller_fixture();
        classic_controller_indexed_window(&mut disp, &mut bus, port);
        let prior = disp.create_palette_from_ctab(&mut bus, 4, 0, 2, 0);
        disp.set_window_palette_association(port, prior, -0x4000);
        let first =
            classic_controller_create(&mut disp, &mut cpu, &mut bus, movie, (0, 0, 2, 4), 11);
        let p1 = disp.movie_controllers[&first].palette.unwrap().handle;
        let second =
            classic_controller_create(&mut disp, &mut cpu, &mut bus, movie, (0, 0, 2, 4), 11);
        let p2 = disp.movie_controllers[&second].palette.unwrap().handle;
        if first_before_second {
            disp.dispose_classic_movie_controller(&mut bus, first);
            assert_eq!(disp.window_palettes[&port].0, p2);
            assert_eq!(
                disp.movie_controllers[&second].palette.unwrap().previous,
                Some((prior, -0x4000))
            );
            disp.dispose_classic_movie_controller(&mut bus, second);
        } else {
            disp.dispose_classic_movie_controller(&mut bus, second);
            assert_eq!(disp.window_palettes[&port].0, p1);
            disp.dispose_classic_movie_controller(&mut bus, first);
        }
        assert_eq!(disp.window_palettes[&port], (prior, -0x4000));
        assert!(bus.get_alloc_size(p1).is_none());
        assert!(bus.get_alloc_size(p2).is_none());
        assert!(bus.get_alloc_size(prior).is_some());
    }
}

#[test]
fn classic_controller_background_palette_waits_for_window_activation() {
    let (mut disp, mut cpu, mut bus, movie, port, _) = classic_controller_fixture();
    classic_controller_indexed_window(&mut disp, &mut bus, port);
    disp.front_window = port + 0x1000;
    let before = *disp.device_clut;
    let controller =
        classic_controller_create(&mut disp, &mut cpu, &mut bus, movie, (0, 0, 2, 4), 11);
    assert!(disp.movie_controllers[&controller].palette.is_some());
    assert_eq!(*disp.device_clut, before);
    disp.front_window = port;
    disp.activate_palette_for_window(&mut bus, port);
    assert!(disp.device_clut.contains(&[0xf0f0, 0x1010, 0x0808]));
}

#[test]
fn classic_controller_offscreen_and_direct_color_targets_do_not_claim_window_palettes() {
    for window in [false, true] {
        let (mut disp, mut cpu, mut bus, movie, port, _) = classic_controller_fixture();
        if window {
            disp.window_list.push(port);
            disp.front_window = port;
        }
        let controller =
            classic_controller_create(&mut disp, &mut cpu, &mut bus, movie, (0, 0, 2, 4), 11);
        assert!(disp.movie_controllers[&controller].palette.is_none());
        assert!(!disp.window_palettes.contains_key(&port));
    }
}
