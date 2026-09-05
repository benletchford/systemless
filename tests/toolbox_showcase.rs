//! Integration test exercising Toolbox Showcase for issues #1078, #1081,
//! #1264, #1265, #1266, #1267, #1268, #1269, #1338, #1353, and #1368.
#![allow(dead_code)]

use std::path::{Path, PathBuf};

use systemless::display::render_screen_with_gamma;
use systemless::game::{init_game, load_game, new_runner_with_screen_depth};
use systemless::menu_model::GuestMenuSnapshot;
use systemless::runner::{
    FixtureRunner, ResourceManagerSnapshot, TextEditSnapshot, WindowSnapshot,
};
use systemless::ui_theme::UiThemeId;

const SHOWCASE_SIT: &[u8] = include_bytes!("toolbox-showcase/toolbox-showcase.sit");

const MENU_APPLE: i16 = 128;
const MENU_PAGES: i16 = 129;
const MENU_STATE: i16 = 130;
const MENU_FILE: i16 = 131;
const MENU_OPTIONS: i16 = 132;

const MENU_DIFFICULTY: i16 = 140;
const MENU_SOUND: i16 = 141;
const MENU_RENDERER: i16 = 142;

/* Pages menu items */
const ITEM_PAGE_GRAPHICS: i16 = 1;
const ITEM_PAGE_CONTROLS: i16 = 2;
const ITEM_PAGE_WINDOWS: i16 = 3;
const ITEM_PAGE_DRAWING: i16 = 4;
const ITEM_PAGE_PREFERENCES: i16 = 5;
const ITEM_PAGE_DIALOGS: i16 = 6;
const ITEM_PAGE_TEXTEDIT: i16 = 7;
const ITEM_PAGE_PALETTES: i16 = 8;
const ITEM_PAGE_LISTS: i16 = 9;
const ITEM_PAGE_SOUND: i16 = 10;
const ITEM_PAGE_STYLED_TEXT: i16 = 11;
const ITEM_PAGE_STANDARD_FILE: i16 = 12;
const ITEM_PAGE_RESOURCES: i16 = 13;
const ITEM_PAGE_SPRITES: i16 = 14;
const ITEM_PAGE_EVENTS_CURSORS: i16 = 15;
const ITEM_PAGE_POPUP_LISTS: i16 = 16;

const MENU_POPUP_LOADOUT: i16 = 143;
const MENU_POPUP_THEME: i16 = 144;
const ITEM_POPUP_LOADOUT_SCOUT: i16 = 1;
const ITEM_POPUP_LOADOUT_VETERAN: i16 = 2;
const ITEM_POPUP_LOADOUT_SEPARATOR: i16 = 3;
const ITEM_POPUP_LOADOUT_LONG: i16 = 4;
const ITEM_POPUP_LOADOUT_DISABLED: i16 = 5;
const ITEM_POPUP_LOADOUT_HEAVY: i16 = 6;
const ITEM_POPUP_THEME_CLASSIC: i16 = 1;
const ITEM_POPUP_THEME_DISABLED: i16 = 2;
const ITEM_POPUP_THEME_SEPARATOR: i16 = 3;
const ITEM_POPUP_THEME_NIGHT: i16 = 4;
const ITEM_POPUP_THEME_DEEP_FIELD: i16 = 36;

/* State menu items */
const ITEM_STATE_BUTTON: i16 = 1;
const ITEM_STATE_CHECKBOX: i16 = 2;
const ITEM_STATE_SCROLLBAR: i16 = 3;
const ITEM_STATE_AUX_WINDOW: i16 = 4;
const ITEM_STATE_SOUND_COMPLETE: i16 = 5;

/* Options menu items */
const ITEM_OPT_DIFFICULTY: i16 = 1;
const ITEM_OPT_SOUND: i16 = 2;
const ITEM_OPT_RENDERER: i16 = 3;
const ITEM_OPT_RESET_PREFS: i16 = 5;
const ITEM_OPT_LAUNCH_DIALOG: i16 = 6;

/* Difficulty submenu items */
const ITEM_DIFF_EASY: i16 = 1;
const ITEM_DIFF_NORMAL: i16 = 2;
const ITEM_DIFF_HARD: i16 = 3;

/* Sound submenu items */
const ITEM_SOUND_MUTE: i16 = 1;
const ITEM_SOUND_FX_ONLY: i16 = 2;
const ITEM_SOUND_MUSIC_ONLY: i16 = 3;
const ITEM_SOUND_FULL: i16 = 4;

/* Renderer submenu items */
const ITEM_RENDERER_FLAT: i16 = 1;
const ITEM_RENDERER_BEVEL: i16 = 2;
const ITEM_RENDERER_CONTRAST: i16 = 3;

/* File menu items */
const ITEM_FILE_PREFERENCES: i16 = 1;
const ITEM_FILE_OPTIONS: i16 = 2;
const ITEM_FILE_QUIT: i16 = 4;

/* Apple menu items */
const ITEM_APPLE_ABOUT: i16 = 1;

const REFERENCE_UPDATE_ENV: &str = "SYSTEMLESS_UPDATE_TOOLBOX_REFERENCES";

fn prefer_powerpc() -> bool {
    matches!(
        std::env::var("SYSTEMLESS_PREFER_POWERPC").ok().as_deref(),
        Some("1" | "true" | "True" | "TRUE" | "yes" | "Yes" | "YES")
    )
}

fn menu_item_checked(snapshot: &GuestMenuSnapshot, menu_id: i16, item_number: i16) -> bool {
    snapshot
        .menus
        .iter()
        .find(|menu| menu.id == menu_id)
        .and_then(|menu| menu.items.iter().find(|item| item.number == item_number))
        .map(|item| item.checked)
        .unwrap_or(false)
}

fn assert_window_stack(runner: &mut FixtureRunner, expected_titles: &[&str]) {
    let snapshots = runner.window_stack_snapshot();
    let titles = snapshots
        .iter()
        .map(|window| window.title.as_str())
        .collect::<Vec<_>>();
    assert_eq!(
        titles, expected_titles,
        "Window Manager order must be front-to-back"
    );
    assert_eq!(
        snapshots.iter().filter(|window| window.active).count(),
        usize::from(!snapshots.is_empty()),
        "exactly one live document window must be active"
    );
    if let Some(front) = snapshots.first() {
        assert!(front.active, "front window must be active");
    }
}

fn window_snapshot<'a>(snapshots: &'a [WindowSnapshot], title: &str) -> &'a WindowSnapshot {
    snapshots
        .iter()
        .find(|window| window.title == title)
        .unwrap_or_else(|| panic!("missing window {title:?} in {snapshots:?}"))
}

fn assert_window_geometry(
    runner: &mut FixtureRunner,
    title: &str,
    bounds: (i16, i16, i16, i16),
    structure_bounds: (i16, i16, i16, i16),
) {
    let snapshots = runner.window_stack_snapshot();
    let window = window_snapshot(&snapshots, title);
    assert_eq!(window.bounds, bounds, "{title} content geometry differs");
    assert_eq!(
        window.structure_bounds,
        Some(structure_bounds),
        "{title} structure geometry differs"
    );
    assert!(
        window.visible_region.is_some(),
        "{title} must retain a visible content region"
    );
}

fn assert_windows_repainted(runner: &mut FixtureRunner, context: &str) {
    let snapshots = runner.window_stack_snapshot();
    for window in snapshots.iter().filter(|window| window.visible) {
        assert_eq!(
            window.update_region, None,
            "{context}: {} still has a pending update region",
            window.title
        );
    }
}

fn step_until<F>(runner: &mut FixtureRunner, label: &str, mut condition: F)
where
    F: FnMut(&mut FixtureRunner) -> bool,
{
    const BATCH_STEPS: usize = 50_000;
    const MAX_ITERATIONS: usize = 200;

    for iteration in 0..MAX_ITERATIONS {
        if condition(runner) {
            return;
        }
        let (_steps, still_running) = runner.run_steps(BATCH_STEPS, None);
        if !still_running || runner.is_halted() {
            panic!(
                "Emulation halted unexpectedly while waiting for '{label}' at iteration {iteration}:\n\
                 PC: {:08X?}\n\
                 Trap: {:04X?}\n\
                 SP: {:08X?}\n\
                 D0: {:08X?}\n\
                 ExitToShell: {}\n\
                 Total instructions: {}",
                runner.halted_pc(),
                runner.halted_trap(),
                runner.halted_sp(),
                runner.halted_d0(),
                runner.halted_by_exit_to_shell(),
                runner.total_instructions(),
            );
        }
    }
    panic!(
        "Timed out waiting for '{label}' after {} instructions",
        runner.total_instructions()
    );
}

fn step_until_gui<F>(runner: &mut FixtureRunner, label: &str, mut condition: F)
where
    F: FnMut(&mut FixtureRunner) -> bool,
{
    const BATCH_STEPS: usize = 50_000;
    const MAX_ITERATIONS: usize = 200;

    for iteration in 0..MAX_ITERATIONS {
        if condition(runner) {
            return;
        }
        let target_tick = runner.guest_tick().saturating_add(1);
        let (_steps, still_running) =
            runner.run_steps_with_audio(BATCH_STEPS, Some(target_tick), 0);
        if !still_running || runner.is_halted() {
            panic!(
                "Emulation halted unexpectedly while waiting for '{label}' at iteration {iteration}:\n\
                 PC: {:08X?}\n\
                 Trap: {:04X?}\n\
                 SP: {:08X?}\n\
                 D0: {:08X?}\n\
                 ExitToShell: {}\n\
                 Total instructions: {}",
                runner.halted_pc(),
                runner.halted_trap(),
                runner.halted_sp(),
                runner.halted_d0(),
                runner.halted_by_exit_to_shell(),
                runner.total_instructions(),
            );
        }
    }
    panic!(
        "Timed out waiting for '{label}' after {} instructions",
        runner.total_instructions()
    );
}

fn showcase_textedit(runner: &mut FixtureRunner) -> TextEditSnapshot {
    runner
        .text_edit_snapshot()
        .records
        .into_iter()
        .find(|record| record.view_rect == (76, 34, 211, 326))
        .expect("showcase must own its TextEdit buffer")
}

fn click_point(runner: &mut FixtureRunner, v: i16, h: i16) {
    runner.set_mouse_position(v, h);
    runner.push_mouse_down(v, h);
    runner.push_mouse_up(v, h);
}

fn tracking_click(runner: &mut FixtureRunner, v: i16, h: i16) {
    // TrackGoAway owns the button between its initial mouseDown and the
    // later mouseUp.  Queueing both records before the guest sees the first
    // one would clear the held-button state too early and correctly produce
    // a rejected close-box click.
    runner.set_mouse_position(v, h);
    runner.push_mouse_down(v, h);
    run_ticks(runner, "tracking click down registered", 1);
    runner.push_mouse_up(v, h);
    run_ticks(runner, "tracking click up registered", 1);
}

fn drag_mouse(runner: &mut FixtureRunner, from_v: i16, from_h: i16, to_v: i16, to_h: i16) {
    runner.set_mouse_position(from_v, from_h);
    runner.push_mouse_down(from_v, from_h);
    run_ticks(runner, "mouse down registered", 1);
    let steps = 4;
    for step in 1..=steps {
        let v = from_v + (to_v - from_v) * step / steps;
        let h = from_h + (to_h - from_h) * step / steps;
        runner.set_mouse_position(v, h);
        run_ticks(runner, "mouse drag step", 1);
    }
    runner.push_mouse_up(to_v, to_h);
    run_ticks(runner, "mouse up registered", 1);
}

fn sound_page_rendered(runner: &mut FixtureRunner, win_top: i16, win_left: i16) -> bool {
    // SetPage updates the menu state before DrawMainWindow paints the page.
    // The right-hand lifecycle panel is only present after DrawSoundPage has
    // completed, so its border is a semantic rendering gate for page actions.
    // Include the filled panel, its heading, and the final Dispose button:
    // outer panel borders are drawn before their contents and DrawControls,
    // so border-only checks can still catch a partial frame.
    [
        (82, 420),
        (160, 420),
        (94, 318),
        (237, 420),
        (160, 305),
        (160, 534),
        (290, 153),
        (313, 153),
    ]
    .iter()
    .all(|(v, h)| {
        let [red, green, blue] = screen_rgb(runner, (win_top + v) as u16, (win_left + h) as u16);
        red < 250 || green < 250 || blue < 250
    })
}

fn screen_rgb(runner: &mut FixtureRunner, v: u16, h: u16) -> [u8; 3] {
    runner.composite_frame();
    let screen_mode = runner.dispatcher().screen_mode;
    let (_, _, width, height, _) = screen_mode;
    assert!(h < width && v < height, "sample point must be on screen");
    let rgba = render_screen_with_gamma(
        runner.bus(),
        screen_mode,
        &runner.dispatcher().device_clut,
        &runner.dispatcher().device_gamma,
    );
    let offset = (usize::from(v) * usize::from(width) + usize::from(h)) * 4;
    [rgba[offset], rgba[offset + 1], rgba[offset + 2]]
}

fn is_dark_chrome(rgb: [u8; 3]) -> bool {
    rgb.into_iter().all(|channel| channel < 64)
}

fn reference_path(powerpc: bool, filename: &str) -> PathBuf {
    let profile = if powerpc {
        "systemless-classic-ppc"
    } else {
        "systemless-classic-68k"
    };
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/toolbox-showcase/reference")
        .join(profile)
        .join(filename)
}

fn update_references() -> bool {
    matches!(
        std::env::var(REFERENCE_UPDATE_ENV).ok().as_deref(),
        Some("1" | "true" | "True" | "TRUE" | "yes" | "Yes" | "YES")
    )
}

fn resource_count(snapshot: &ResourceManagerSnapshot, res_type: [u8; 4]) -> usize {
    snapshot
        .counts
        .iter()
        .find(|(candidate, _)| *candidate == res_type)
        .map(|(_, count)| *count)
        .unwrap_or(0)
}

fn assert_resource_browser_snapshot(runner: &mut FixtureRunner, loaded_id: Option<i16>) {
    let snapshot = runner.resource_manager_snapshot();
    assert_eq!(
        snapshot.current_file, 0,
        "application resource file must be current"
    );
    assert_eq!(resource_count(&snapshot, *b"DATA"), 3);
    assert_eq!(resource_count(&snapshot, *b"MENU"), 9);
    assert_eq!(resource_count(&snapshot, *b"WIND"), 1);

    let expected = [
        (201i16, "Browser Seed", 15usize),
        (202i16, "Deferred Payload", 19usize),
        (203i16, "Mutable Record", 17usize),
    ];
    assert_eq!(
        snapshot.data_entries.len(),
        expected.len(),
        "Resource Browser must enumerate exactly the three DATA records"
    );
    for (entry, (id, name, size)) in snapshot.data_entries.iter().zip(expected) {
        assert_eq!(entry.res_type, *b"DATA");
        assert_eq!(entry.id, id);
        assert_eq!(entry.name.as_deref(), Some(name));
        assert_eq!(entry.attrs, 0, "fixture DATA resources must remain clean");
        assert_eq!(entry.size, size);
        assert_eq!(entry.loaded, loaded_id == Some(id));
    }
}

fn rendered_rgb(runner: &mut FixtureRunner) -> (u32, u32, Vec<u8>) {
    runner.composite_frame();
    let screen_mode = runner.dispatcher().screen_mode;
    let (_, _, width, height, _) = screen_mode;
    let rgba = render_screen_with_gamma(
        runner.bus(),
        screen_mode,
        &runner.dispatcher().device_clut,
        &runner.dispatcher().device_gamma,
    );
    let rgb = rgba
        .chunks_exact(4)
        .flat_map(|pixel| [pixel[0], pixel[1], pixel[2]])
        .collect();
    (u32::from(width), u32::from(height), rgb)
}

fn write_rgb(path: &Path, width: u32, height: u32, rgb: Vec<u8>) {
    let image = image::RgbImage::from_raw(width, height, rgb)
        .expect("rendered RGB buffer must match its dimensions");
    image
        .save(path)
        .unwrap_or_else(|error| panic!("failed to write {}: {error}", path.display()));
}

fn assert_reference_frame(runner: &mut FixtureRunner, filename: &str) {
    let (width, height, actual) = rendered_rgb(runner);
    let reference = reference_path(runner.is_powerpc_app(), filename);

    if update_references() {
        std::fs::create_dir_all(reference.parent().unwrap())
            .unwrap_or_else(|error| panic!("failed to create {}: {error}", reference.display()));
        write_rgb(&reference, width, height, actual);
        eprintln!("updated {}", reference.display());
        return;
    }

    let expected = image::open(&reference)
        .unwrap_or_else(|error| panic!("failed to read {}: {error}", reference.display()))
        .to_rgb8();
    assert_eq!(
        expected.dimensions(),
        (width, height),
        "reference dimensions differ for {}",
        reference.display()
    );
    let mut comparison_actual = actual.clone();
    if filename.contains("events-") {
        // The raw values are asserted semantically below. Their timestamp,
        // pointer-valued message, local mouse coordinates, and prior click
        // counts legitimately vary with the host build profile and earlier
        // window-lifecycle probes. Exclude only those rendered value spans
        // while retaining strict comparison of labels and surrounding UI.
        for (top, left, bottom, right) in [
            (148_u32, 100_u32, 182_u32, 252_u32),
            (136, 430, 150, 492),
            (198, 174, 211, 252),
        ] {
            for v in top..bottom {
                for h in left..right {
                    let offset = ((v * width + h) * 3) as usize;
                    comparison_actual[offset..offset + 3]
                        .copy_from_slice(&expected.as_raw()[offset..offset + 3]);
                }
            }
        }
    }
    if expected.as_raw() == &comparison_actual {
        return;
    }

    let differing_pixels = expected
        .as_raw()
        .chunks_exact(3)
        .zip(comparison_actual.chunks_exact(3))
        .filter(|(expected, actual)| expected != actual)
        .count();
    let actual_path = std::env::temp_dir().join(format!(
        "systemless-toolbox-showcase-{}-{filename}",
        if runner.is_powerpc_app() {
            "ppc"
        } else {
            "68k"
        }
    ));
    write_rgb(&actual_path, width, height, actual);
    panic!(
        "{} differs at {differing_pixels} of {} pixels; actual frame written to {}. Set {REFERENCE_UPDATE_ENV}=1 to accept new references",
        reference.display(),
        width * height,
        actual_path.display()
    );
}

fn wait_for_page_event_loop(runner: &mut FixtureRunner, label: &str) {
    let previous = runner.event_manager_snapshot().last_record;
    // Sprite presentation precedes the validation text and DrawControls.
    // A fresh null event proves the guest finished that entire draw and
    // returned to its top-level WaitNextEvent loop on either CPU adapter.
    step_until(runner, label, |runner| {
        let current = runner.event_manager_snapshot().last_record;
        current != previous && current.is_some_and(|event| event.what == 0)
    });
}

fn run_ticks(runner: &mut FixtureRunner, label: &str, ticks: u32) {
    let target = runner.guest_tick().saturating_add(ticks);
    while runner.guest_tick() < target {
        let (_steps, still_running) = runner.run_steps(50_000, None);
        assert!(
            still_running && !runner.is_halted(),
            "emulation halted while waiting for {label}"
        );
    }
}

fn run_until_frame_changes(runner: &mut FixtureRunner, label: &str, before: &[u8]) {
    for _ in 0..200 {
        // Use small instruction slices here: a larger slice can let the
        // guest's key-repeat timer enqueue autoKey before we release the
        // physical key after the first keyDown redraw starts.
        let (_steps, still_running) = runner.run_steps(100, None);
        assert!(
            still_running && !runner.is_halted(),
            "emulation halted while waiting for {label}"
        );
        if rendered_rgb(runner).2 != before {
            return;
        }
    }
    panic!("timed out waiting for {label} to change the framebuffer");
}

fn run_until_frame_marker(
    runner: &mut FixtureRunner,
    label: &str,
    marker_v: u16,
    marker_h: u16,
    expected: [u8; 3],
) {
    // A redraw changes the framebuffer as soon as the window is erased, but
    // the guest may still be part-way through DrawEventsPage at that point.
    // Wait for a pixel in the final Show button border to be restored so the
    // checkpoint cannot capture a partially painted page.
    for _ in 0..200 {
        let (_steps, still_running) = runner.run_steps(5_000, None);
        assert!(
            still_running && !runner.is_halted(),
            "emulation halted while waiting for {label}"
        );
        if screen_rgb(runner, marker_v, marker_h) == expected {
            return;
        }
    }
    panic!("timed out waiting for {label} to finish the framebuffer redraw");
}

fn run_audio(runner: &mut FixtureRunner, label: &str, samples: usize) {
    let (_steps, still_running) = runner.run_steps_with_audio(50_000, None, samples);
    assert!(
        still_running && !runner.is_halted(),
        "emulation halted while waiting for {label}"
    );
}

fn save_audio_evidence(audio: &[u8], checkpoint: &str) {
    let Some(directory) = std::env::var_os("SYSTEMLESS_TOOLBOX_AUDIO_OUTPUT") else {
        return;
    };
    let directory = Path::new(&directory).join(if prefer_powerpc() { "ppc" } else { "m68k" });
    std::fs::create_dir_all(&directory).expect("create audio evidence directory");
    std::fs::write(directory.join(format!("{checkpoint}.u8")), audio)
        .expect("save unsigned 8-bit mono PCM at 22050 Hz");
}

fn assert_native_sample_audio(audio: &[u8], checkpoint: &str) {
    let native = std::fs::read(Path::new(env!("CARGO_MANIFEST_DIR")).join(format!(
        "tests/toolbox-showcase/reference/native-audio/sndplay-{checkpoint}-44100.u8"
    )))
    .expect("native sampled-sound evidence must exist");
    // Both native devices produced identical mono waveforms at 44100 Hz.
    // Systemless outputs 22050 Hz. Compare every second native sample;
    // allow one final sample of duration rounding and three 8-bit levels for
    // differences in interpolation phase/rounding, without shifting the audio.
    let expected: Vec<u8> = native.into_iter().step_by(2).collect();
    assert!(
        audio.len().abs_diff(expected.len()) <= 1,
        "{checkpoint} duration differs: {} Systemless samples, {} native samples at 22050 Hz",
        audio.len(),
        expected.len()
    );
    let mut total_error = 0usize;
    for (index, (&actual, &native)) in audio.iter().zip(&expected).enumerate() {
        let difference = actual.abs_diff(native);
        assert!(
            difference <= 3,
            "{checkpoint} waveform differs at sample {index}: Systemless {actual}, native {native}"
        );
        total_error += usize::from(difference);
    }
    assert!(
        total_error <= expected.len(),
        "{checkpoint} average PCM error exceeds one level"
    );
}

fn assert_non_silent_audio(audio: &[u8], label: &str) {
    let minimum = *audio
        .iter()
        .min()
        .unwrap_or_else(|| panic!("{label} must contain mixed PCM samples"));
    let maximum = *audio
        .iter()
        .max()
        .unwrap_or_else(|| panic!("{label} must contain mixed PCM samples"));
    assert!(
        maximum.saturating_sub(minimum) >= 32,
        "{label} must contain a non-silent waveform, min={minimum} max={maximum}"
    );
}

fn step_until_with_audio<F>(
    runner: &mut FixtureRunner,
    label: &str,
    audio_samples: usize,
    mut condition: F,
) where
    F: FnMut(&mut FixtureRunner) -> bool,
{
    const MAX_ITERATIONS: usize = 200;

    for iteration in 0..MAX_ITERATIONS {
        if condition(runner) {
            return;
        }
        let (_steps, still_running) = runner.run_steps_with_audio(50_000, None, audio_samples);
        assert!(
            still_running && !runner.is_halted(),
            "emulation halted while waiting for {label} at iteration {iteration}"
        );
    }
    panic!(
        "Timed out waiting for '{label}' after {} instructions",
        runner.total_instructions()
    );
}

fn run_gui_ticks(runner: &mut FixtureRunner, label: &str, ticks: u32) {
    let target = runner.guest_tick().saturating_add(ticks);
    while runner.guest_tick() < target {
        let frame_target = runner.guest_tick().saturating_add(1).min(target);
        let (_steps, still_running) = runner.run_steps_with_audio(50_000, Some(frame_target), 0);
        assert!(
            still_running && !runner.is_halted(),
            "emulation halted while waiting for {label}"
        );
    }
}

fn run_popup_tracking_tick(runner: &mut FixtureRunner, label: &str) {
    if runner.is_powerpc_app() {
        // Native PopUpMenuSelect yields at a GUI frame boundary, while the
        // classic TrackControl refire deliberately freezes guest ticks until
        // mouse-up. Keep the same visible frame cadence without waiting on a
        // frozen 68K clock.
        run_gui_ticks(runner, label, 1);
    } else {
        run_ticks(runner, label, 1);
    }
}

fn assert_graphics_page_rendered(runner: &mut FixtureRunner) {
    // WIND 128 begins at (50, 40); this samples the center of the red oval
    // drawn at local Rect(205, 55, 325, 135).
    let [red, green, blue] = screen_rgb(runner, 145, 305);
    assert!(
        red > green.saturating_add(80) && red > blue.saturating_add(80),
        "Graphics page red oval was not rendered: rgb=({red}, {green}, {blue})"
    );
}

fn assert_drawing_page_rendered(runner: &mut FixtureRunner, win_top: i16, win_left: i16) {
    // Star polygon center at local (110, 345) -> global (win_top + 110, win_left + 345)
    // Gold fill: red/green prominent
    let [star_r, star_g, star_b] =
        screen_rgb(runner, (win_top + 110) as u16, (win_left + 345) as u16);
    assert!(
        star_r > 150 && star_g > 100 && star_b < 100,
        "Drawing page star polygon was not rendered: rgb=({star_r}, {star_g}, {star_b})"
    );

    // Pie chart arc red slice at local (105, 470)
    let [pie_r, pie_g, pie_b] = screen_rgb(runner, (win_top + 105) as u16, (win_left + 470) as u16);
    assert!(
        pie_r > pie_g.saturating_add(40) && pie_r > pie_b.saturating_add(40),
        "Drawing page pie chart red arc was not rendered: rgb=({pie_r}, {pie_g}, {pie_b})"
    );

    // Raised 3D bevel white highlight at local (50, 22)
    let [bev_r, bev_g, bev_b] = screen_rgb(runner, (win_top + 50) as u16, (win_left + 22) as u16);
    assert!(
        bev_r > 200 && bev_g > 200 && bev_b > 200,
        "Drawing page 3D bevel highlight was not rendered: rgb=({bev_r}, {bev_g}, {bev_b})"
    );

    if runner.is_powerpc_app() {
        let [r, g, b] = screen_rgb(runner, (win_top + 82) as u16, (win_left + 32) as u16);
        assert!(
            r < 60 && g < 70 && (40..100).contains(&b),
            "native QD3D pane must retain its dark clear color: {r},{g},{b}"
        );
        let oracle = image::open(
            Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("tests/toolbox-showcase/reference/sheepshaver-ppc/04-drawing.png"),
        )
        .expect("native QD3D oracle capture must decode")
        .to_rgb8();
        let (width, _, actual) = rendered_rgb(runner);
        let geometry = |r: u8, g: u8, b: u8| b > 120 && g > 70 && b > r.saturating_add(50);
        let mut intersection = 0;
        let mut union = 0;
        let mut native_pixels = 0;
        let mut lighting_pixels = 0;
        for v in 80..150 {
            for h in 30..125 {
                let offset = ((win_top as u32 + v) * width + win_left as u32 + h) as usize * 3;
                let actual_geometry =
                    geometry(actual[offset], actual[offset + 1], actual[offset + 2]);
                let native = oracle.get_pixel(40 + h, 50 + v).0;
                let native_geometry = geometry(native[0], native[1], native[2]);
                native_pixels += usize::from(native_geometry);
                intersection += usize::from(actual_geometry && native_geometry);
                union += usize::from(actual_geometry || native_geometry);
                // Ignore the one-pixel contour, where rasterization differs.
                // Across the face interior, retain the native lighting gradient;
                // eight RGB levels allow one 5-bit display quantization step.
                let interior = native_geometry
                    && (0..3).all(|dv| {
                        (0..3).all(|dh| {
                            let pixel = oracle.get_pixel(40 + h + dh - 1, 50 + v + dv - 1).0;
                            geometry(pixel[0], pixel[1], pixel[2])
                        })
                    });
                if interior {
                    lighting_pixels += 1;
                    for channel in 0..3 {
                        assert!(
                            actual[offset + channel].abs_diff(native[channel]) <= 8,
                            "native QD3D lighting differs at local ({v},{h}), channel {channel}: actual {}, native {}",
                            actual[offset + channel], native[channel]
                        );
                    }
                }
            }
        }
        assert!(
            native_pixels > 400,
            "oracle must contain a substantial rendered face"
        );
        assert!(
            lighting_pixels > 400,
            "oracle must have a substantial face interior"
        );
        // The small face has one-pixel rasterization differences between renderers.
        assert!(
            intersection * 100 >= union * 85,
            "native QD3D silhouette differs: {intersection} shared pixels out of {union}"
        );
    } else {
        // Sunken gauge well green bar at local (110, 175)
        let [gauge_r, gauge_g, gauge_b] =
            screen_rgb(runner, (win_top + 110) as u16, (win_left + 175) as u16);
        assert!(
            gauge_g > gauge_r.saturating_add(30) && gauge_g > gauge_b.saturating_add(30),
            "Drawing page sunken gauge bar was not rendered: rgb=({gauge_r}, {gauge_g}, {gauge_b})"
        );
    }
}

fn region_contains_color<F>(
    runner: &mut FixtureRunner,
    top: i16,
    left: i16,
    bottom: i16,
    right: i16,
    matches: F,
) -> bool
where
    F: Fn([u8; 3]) -> bool,
{
    let (width, height, rgb) = rendered_rgb(runner);
    let top = u32::from(top.max(0) as u16).min(height);
    let left = u32::from(left.max(0) as u16).min(width);
    let bottom = u32::from(bottom.max(0) as u16).min(height);
    let right = u32::from(right.max(0) as u16).min(width);
    (top..bottom).any(|v| {
        (left..right).any(|h| {
            let offset = ((v * width + h) * 3) as usize;
            matches([rgb[offset], rgb[offset + 1], rgb[offset + 2]])
        })
    })
}

fn assert_styled_text_page_rendered(runner: &mut FixtureRunner, win_top: i16, win_left: i16) {
    // The upper well is the actual TEStyleNew output. Restrict the probes to
    // its text rectangle so a decorative legend cannot satisfy the check.
    let text_top = win_top + 76;
    let text_left = win_left + 34;
    let text_bottom = win_top + 114;
    let text_right = win_left + 521;
    assert!(
        region_contains_color(
            runner,
            text_top,
            text_left,
            text_bottom,
            text_right,
            |rgb| { rgb[2] > rgb[0].saturating_add(20) && rgb[2] > rgb[1].saturating_add(10) }
        ),
        "styled TextEdit blue run was not rendered"
    );
    assert!(
        region_contains_color(
            runner,
            text_top,
            text_left,
            text_bottom,
            text_right,
            |rgb| { rgb[0] > rgb[1].saturating_add(25) && rgb[0] > rgb[2].saturating_add(25) }
        ),
        "styled TextEdit red run was not rendered"
    );
    assert!(
        region_contains_color(
            runner,
            text_top,
            text_left,
            text_bottom,
            text_right,
            |rgb| { rgb[1] > rgb[0].saturating_add(15) && rgb[1] > rgb[2].saturating_add(15) }
        ),
        "styled TextEdit green run was not rendered"
    );
    assert!(
        region_contains_color(
            runner,
            text_top,
            text_left,
            text_bottom,
            text_right,
            |rgb| { rgb[0] > 45 && rgb[2] > 45 && rgb[1].saturating_add(20) < rgb[0].min(rgb[2]) }
        ),
        "styled TextEdit purple run was not rendered"
    );

    // These three rows are lengths returned by CharWidth, TextWidth, and
    // MeasureText, respectively. Their colors and non-zero extents are
    // generated from those API results by the fixture.
    assert!(
        region_contains_color(
            runner,
            win_top + 263,
            win_left + 399,
            win_top + 270,
            win_left + 450,
            |rgb| { rgb[2] > rgb[0].saturating_add(20) }
        ),
        "CharWidth result ruler was not rendered"
    );
    assert!(
        region_contains_color(
            runner,
            win_top + 278,
            win_left + 399,
            win_top + 285,
            win_left + 500,
            |rgb| { rgb[1] > rgb[0].saturating_add(15) }
        ),
        "TextWidth result ruler was not rendered"
    );
    assert!(
        region_contains_color(
            runner,
            win_top + 293,
            win_left + 399,
            win_top + 300,
            win_left + 500,
            |rgb| { rgb[0] > 45 && rgb[2] > 45 }
        ),
        "MeasureText result ruler was not rendered"
    );
}

fn assert_sprites_page_rendered(
    runner: &mut FixtureRunner,
    win_top: i16,
    win_left: i16,
) -> ([u8; 3], [u8; 3]) {
    // The page copies the 320×128 GWorld into local Rect(24, 80, 344, 208).
    // The first sprite's body center is world (62, 70), and its matte corner
    // at (38, 38) must leave the scene background untouched.
    let first_body = screen_rgb(
        runner,
        (win_top + 80 + 70) as u16,
        (win_left + 24 + 62) as u16,
    );
    let matte_outside = screen_rgb(
        runner,
        (win_top + 80 + 38) as u16,
        (win_left + 24 + 38) as u16,
    );
    let scene_background = screen_rgb(
        runner,
        (win_top + 80 + 8) as u16,
        (win_left + 24 + 8) as u16,
    );
    assert_ne!(
        first_body, scene_background,
        "CopyMask sprite body must differ from the offscreen scene background"
    );
    assert_eq!(
        matte_outside, scene_background,
        "CopyMask's transparent matte pixels must preserve the scene"
    );

    // The second sprite is centered at world (238, 70). Its center is both
    // inside the deep mask and inside the BitMapToRegion-derived clip.
    let second_body = screen_rgb(
        runner,
        (win_top + 80 + 70) as u16,
        (win_left + 24 + 238) as u16,
    );
    assert_ne!(
        second_body, scene_background,
        "CopyDeepMask's deep-mask center must render inside the region clip"
    );

    // SetCPixel/GetCPixel writes a red probe at world (15, 12), outside both
    // sprites, and the page copies that pixel to the visible scene.
    let probe = screen_rgb(
        runner,
        (win_top + 80 + 12) as u16,
        (win_left + 24 + 15) as u16,
    );
    assert!(
        probe[0] > probe[1].saturating_add(30),
        "SetCPixel/GetCPixel probe must retain its red component: rgb={probe:?}"
    );

    (first_body, second_body)
}

fn assert_popup_menu_contract(snapshot: &GuestMenuSnapshot) {
    let loadout = snapshot
        .menus
        .iter()
        .find(|menu| menu.id == MENU_POPUP_LOADOUT)
        .expect("resource-backed popup menu must be present");
    assert!(
        loadout.hierarchical && !loadout.visible_in_menu_bar,
        "resource-backed popup menu must live in the hierarchical partition"
    );
    assert_eq!(loadout.items.len(), 6);
    assert_eq!(
        loadout.items[ITEM_POPUP_LOADOUT_LONG as usize - 1].text,
        "Long-range Expedition Loadout"
    );
    assert!(
        loadout.items[ITEM_POPUP_LOADOUT_SEPARATOR as usize - 1].separator,
        "popup separator must remain a non-selectable row"
    );
    assert!(
        !loadout.items[ITEM_POPUP_LOADOUT_DISABLED as usize - 1].enabled,
        "popup disabled item must remain unavailable"
    );

    let theme = snapshot
        .menus
        .iter()
        .find(|menu| menu.id == MENU_POPUP_THEME)
        .expect("programmatic popup menu must be present");
    assert!(
        theme.hierarchical && !theme.visible_in_menu_bar,
        "programmatic popup menu must live in the hierarchical partition"
    );
    assert_eq!(theme.items.len(), 55);
    assert_eq!(
        theme.items[ITEM_POPUP_THEME_NIGHT as usize - 1].text,
        "Night Operations"
    );
    assert_eq!(
        theme.items[ITEM_POPUP_THEME_DEEP_FIELD as usize - 1].text,
        "Deep Field Archive"
    );
    assert!(
        theme.items[ITEM_POPUP_THEME_SEPARATOR as usize - 1].separator,
        "programmatic popup separator must remain a non-selectable row"
    );
    assert!(
        !theme.items[ITEM_POPUP_THEME_DISABLED as usize - 1].enabled,
        "programmatic popup disabled item must remain unavailable"
    );
}

fn assert_popup_page_rendered(runner: &mut FixtureRunner, win_top: i16, win_left: i16) {
    // DrawControls passes the full control rect to the standard popup CDEF;
    // its left borders land at local rows 100..123 and 136..159, with both
    // popup title widths place the resource/programmatic box borders at
    // local columns 250 and 242 respectively. These samples stay
    // inside each rect so they ensure the real CDEF output is present,
    // while menu snapshots below provide the semantic value assertions.
    for (v, h) in [(100, 250), (110, 250), (136, 242), (146, 242)] {
        let [red, green, blue] = screen_rgb(runner, (win_top + v) as u16, (win_left + h) as u16);
        assert!(
            red < 250 || green < 250 || blue < 250,
            "popup control chrome must be visible at local ({v},{h}): rgb=({red},{green},{blue})"
        );
    }
}

fn assert_popup_selected_title_pixels(
    runner: &mut FixtureRunner,
    win_top: i16,
    win_left: i16,
    label: &str,
) {
    // The programmatic popup begins at local column 190 and reserves 52
    // pixels for "Theme:". Inspect only
    // the selected-title interior, away from the frame and arrow; this is a
    // small architecture-neutral raster assertion that catches a missing
    // live NewMenu title without depending on font glyph coordinates.
    let (width, _height, rgb) = rendered_rgb(runner);
    let mut dark_pixels = 0usize;
    let title_left = 247;
    for v in (win_top + 140)..(win_top + 157) {
        for h in (win_left + title_left)..(win_left + 390) {
            let offset = (usize::try_from(v).unwrap() * width as usize
                + usize::try_from(h).unwrap())
                * 3;
            let pixel = &rgb[offset..offset + 3];
            if pixel.iter().all(|channel| *channel < 200) {
                dark_pixels += 1;
            }
        }
    }
    assert!(
        dark_pixels >= 4,
        "programmatic popup must repaint the {label} selected title (only {dark_pixels} dark pixels)"
    );
}

#[test]
fn test_toolbox_showcase() {
    let mut runner = new_runner_with_screen_depth(8);
    let powerpc = prefer_powerpc();
    runner.set_ui_theme(UiThemeId::ClassicSystem7);
    runner
        .set_powerpc_screen_depth(if powerpc { 16 } else { 8 })
        .expect("selected PowerPC fixture screen depth must be supported");
    runner.set_app_start_time(3_786_912_000);
    runner.set_menu_bar_visible(true);

    let app = load_game(&mut runner, SHOWCASE_SIT).expect("failed to load toolbox showcase");
    assert_eq!(
        app.is_powerpc(),
        powerpc,
        "expected PowerPC executable match with SYSTEMLESS_PREFER_POWERPC"
    );

    init_game(&mut runner, &app);
    assert_eq!(
        runner.is_powerpc_app(),
        powerpc,
        "initialized runner must use the selected executable slice"
    );

    // 1. Run until menus and initial window are ready.
    step_until(&mut runner, "initial menu and window readiness", |r| {
        let snapshot = r.guest_menu_snapshot();
        let has_pages_menu = snapshot.menus.iter().any(|m| m.id == MENU_PAGES);
        let has_options_menu = snapshot.menus.iter().any(|m| m.id == MENU_OPTIONS);
        let has_window = r.window_count() >= 1;
        let has_bounds = r.window_bounds() != (0, 0, 0, 0);
        let graphics_checked = menu_item_checked(&snapshot, MENU_PAGES, ITEM_PAGE_GRAPHICS);
        let [red, green, blue] = screen_rgb(r, 145, 305);
        let graphics_rendered =
            red > green.saturating_add(80) && red > blue.saturating_add(80);
        has_pages_menu
            && has_options_menu
            && has_window
            && has_bounds
            && graphics_checked
            && graphics_rendered
    });

    let snapshot = runner.guest_menu_snapshot();
    assert!(
        menu_item_checked(&snapshot, MENU_PAGES, ITEM_PAGE_GRAPHICS),
        "initial page must be Graphics"
    );
    assert!(
        !menu_item_checked(&snapshot, MENU_PAGES, ITEM_PAGE_CONTROLS),
        "Controls page must not be checked initially"
    );
    assert!(
        !menu_item_checked(&snapshot, MENU_PAGES, ITEM_PAGE_WINDOWS),
        "Windows page must not be checked initially"
    );
    assert!(
        !menu_item_checked(&snapshot, MENU_PAGES, ITEM_PAGE_DRAWING),
        "Drawing page must not be checked initially"
    );
    assert!(
        !menu_item_checked(&snapshot, MENU_PAGES, ITEM_PAGE_PREFERENCES),
        "Preferences page must not be checked initially"
    );
    assert!(
        !menu_item_checked(&snapshot, MENU_PAGES, ITEM_PAGE_DIALOGS),
        "Dialogs page must not be checked initially"
    );
    assert!(
        !menu_item_checked(&snapshot, MENU_PAGES, ITEM_PAGE_PALETTES),
        "Palettes page must not be checked initially"
    );
    assert!(
        !menu_item_checked(&snapshot, MENU_PAGES, ITEM_PAGE_TEXTEDIT),
        "TextEdit page must not be checked initially"
    );
    assert!(
        !menu_item_checked(&snapshot, MENU_PAGES, ITEM_PAGE_LISTS),
        "Lists & Inventory page must not be checked initially"
    );
    assert!(
        !menu_item_checked(&snapshot, MENU_PAGES, ITEM_PAGE_EVENTS_CURSORS),
        "Events & Cursors page must not be checked initially"
    );
    assert!(
        !menu_item_checked(&snapshot, MENU_PAGES, ITEM_PAGE_SOUND),
        "Sound & Channels page must not be checked initially"
    );
    assert!(
        !menu_item_checked(&snapshot, MENU_STATE, ITEM_STATE_SOUND_COMPLETE),
        "Sound completion must not be checked initially"
    );
    assert!(
        !menu_item_checked(&snapshot, MENU_PAGES, ITEM_PAGE_STYLED_TEXT),
        "Styled Text & Fonts page must not be checked initially"
    );
    assert!(
        !menu_item_checked(&snapshot, MENU_PAGES, ITEM_PAGE_STANDARD_FILE),
        "Standard File page must not be checked initially"
    );
    assert!(
        !menu_item_checked(&snapshot, MENU_PAGES, ITEM_PAGE_RESOURCES),
        "Resource Browser page must not be checked initially"
    );
    assert!(
        !menu_item_checked(&snapshot, MENU_PAGES, ITEM_PAGE_SPRITES),
        "Sprites page must not be checked initially"
    );
    assert!(
        !menu_item_checked(&snapshot, MENU_PAGES, ITEM_PAGE_POPUP_LISTS),
        "Popup & Dropdown Lists page must not be checked initially"
    );

    // Validate hierarchical menu structure in snapshot
    let file_menu = snapshot
        .menus
        .iter()
        .find(|m| m.id == MENU_FILE)
        .expect("File menu must be present in menu snapshot");
    assert!(
        file_menu.visible_in_menu_bar,
        "File menu must be visible in menu bar"
    );
    assert!(
        !file_menu.hierarchical,
        "File menu must be a top-level menu"
    );
    assert_eq!(
        file_menu.items[(ITEM_FILE_OPTIONS - 1) as usize].submenu_id,
        Some(MENU_OPTIONS),
        "Game Options must attach submenu 132"
    );

    let options_menu = snapshot
        .menus
        .iter()
        .find(|m| m.id == MENU_OPTIONS)
        .expect("Options menu must be present in menu snapshot");
    assert!(
        !options_menu.visible_in_menu_bar,
        "Options submenu must not be visible in the menu bar"
    );
    assert!(
        options_menu.hierarchical,
        "Options menu must be registered as a hierarchical submenu"
    );
    assert_eq!(
        options_menu.items[0].submenu_id,
        Some(MENU_DIFFICULTY),
        "Difficulty item must attach submenu 140"
    );
    assert_eq!(
        options_menu.items[1].submenu_id,
        Some(MENU_SOUND),
        "Sound item must attach submenu 141"
    );
    assert_eq!(
        options_menu.items[2].submenu_id,
        Some(MENU_RENDERER),
        "Renderer item must attach submenu 142"
    );

    let diff_menu = snapshot
        .menus
        .iter()
        .find(|m| m.id == MENU_DIFFICULTY)
        .expect("Difficulty submenu must be registered in snapshot");
    assert!(
        diff_menu.hierarchical,
        "Difficulty submenu must be flagged hierarchical"
    );
    assert!(
        !diff_menu.visible_in_menu_bar,
        "Difficulty submenu must not be visible on the top menu bar"
    );
    assert!(
        menu_item_checked(&snapshot, MENU_DIFFICULTY, ITEM_DIFF_NORMAL),
        "Veteran (Normal) difficulty must be checked initially"
    );
    assert!(
        menu_item_checked(&snapshot, MENU_SOUND, ITEM_SOUND_FULL),
        "Full Audio must be checked initially"
    );
    assert!(
        menu_item_checked(&snapshot, MENU_RENDERER, ITEM_RENDERER_BEVEL),
        "QD3D Bevels renderer must be checked initially"
    );

    assert_eq!(runner.window_count(), 1, "initial window count must be 1");
    assert!(
        !menu_item_checked(&snapshot, MENU_STATE, ITEM_STATE_AUX_WINDOW),
        "Auxiliary window state must be false initially"
    );
    assert_graphics_page_rendered(&mut runner);
    runner.set_mouse_position(550, 760);
    assert_reference_frame(&mut runner, "01-graphics.png");

    // 2. Switch to Controls and exercise every control.
    assert!(
        runner.select_guest_menu_item(MENU_PAGES, ITEM_PAGE_CONTROLS),
        "failed to queue selection of Controls page"
    );
    step_until(&mut runner, "switch to Controls page", |r| {
        let snapshot = r.guest_menu_snapshot();
        menu_item_checked(&snapshot, MENU_PAGES, ITEM_PAGE_CONTROLS)
    });
    let snapshot = runner.guest_menu_snapshot();
    assert!(!menu_item_checked(
        &snapshot,
        MENU_PAGES,
        ITEM_PAGE_GRAPHICS
    ));
    assert!(menu_item_checked(&snapshot, MENU_PAGES, ITEM_PAGE_CONTROLS));
    assert!(!menu_item_checked(&snapshot, MENU_PAGES, ITEM_PAGE_WINDOWS));
    assert_eq!(runner.window_count(), 1);
    assert!(!menu_item_checked(
        &snapshot,
        MENU_STATE,
        ITEM_STATE_AUX_WINDOW
    ));

    let (win_top, win_left, _win_bottom, _win_right) = runner.window_bounds();

    // Button: Rect (top=255, left=40, bottom=279, right=150)
    let button_v = win_top + (255 + 279) / 2;
    let button_h = win_left + (40 + 150) / 2;
    click_point(&mut runner, button_v, button_h);
    step_until(&mut runner, "activate button control", |r| {
        let snapshot = r.guest_menu_snapshot();
        menu_item_checked(&snapshot, MENU_STATE, ITEM_STATE_BUTTON)
    });
    let snapshot = runner.guest_menu_snapshot();
    assert!(
        menu_item_checked(&snapshot, MENU_STATE, ITEM_STATE_BUTTON),
        "Button state checkmark must be set"
    );

    // Checkbox: Rect (top=255, left=185, bottom=279, right=315)
    let checkbox_v = win_top + (255 + 279) / 2;
    let checkbox_h = win_left + (185 + 315) / 2;
    click_point(&mut runner, checkbox_v, checkbox_h);
    step_until(&mut runner, "activate checkbox control", |r| {
        let snapshot = r.guest_menu_snapshot();
        menu_item_checked(&snapshot, MENU_STATE, ITEM_STATE_CHECKBOX)
    });
    let snapshot = runner.guest_menu_snapshot();
    assert!(
        menu_item_checked(&snapshot, MENU_STATE, ITEM_STATE_CHECKBOX),
        "Checkbox state checkmark must be set"
    );

    // Scrollbar: Rect (top=310, left=40, bottom=326, right=500)
    let scrollbar_v = win_top + (310 + 326) / 2;
    let scrollbar_h = win_left + 492;
    click_point(&mut runner, scrollbar_v, scrollbar_h);
    step_until(&mut runner, "activate scrollbar control", |r| {
        let snapshot = r.guest_menu_snapshot();
        menu_item_checked(&snapshot, MENU_STATE, ITEM_STATE_SCROLLBAR)
    });
    let snapshot = runner.guest_menu_snapshot();
    assert!(
        menu_item_checked(&snapshot, MENU_STATE, ITEM_STATE_SCROLLBAR),
        "Scrollbar state checkmark must be set"
    );

    // Hold the State menu open so the visual baseline includes all three
    // completed control actions as checkmarks as well as the control values.
    runner.set_mouse_position(10, 108);
    runner.push_mouse_down(10, 108);
    run_ticks(&mut runner, "State menu to open", 4);
    assert_reference_frame(&mut runner, "02-controls.png");
    runner.push_mouse_up(10, 108);
    run_ticks(&mut runner, "State menu to close", 1);

    // 3. Switch to Windows and create two overlapping auxiliary document
    // windows.  The main window plus these two records give us a real
    // front-to-back stack to exercise rather than a screenshot-only demo.
    assert!(
        runner.select_guest_menu_item(MENU_PAGES, ITEM_PAGE_WINDOWS),
        "failed to queue selection of Windows page"
    );
    step_until(
        &mut runner,
        "switch to Windows page and open auxiliary window",
        |r| {
            let snapshot = r.guest_menu_snapshot();
            let page_checked = menu_item_checked(&snapshot, MENU_PAGES, ITEM_PAGE_WINDOWS);
            let aux_state = menu_item_checked(&snapshot, MENU_STATE, ITEM_STATE_AUX_WINDOW);
            let three_windows = r.window_count() == 3;
            page_checked && aux_state && three_windows
        },
    );
    let snapshot = runner.guest_menu_snapshot();
    assert!(!menu_item_checked(
        &snapshot,
        MENU_PAGES,
        ITEM_PAGE_GRAPHICS
    ));
    assert!(!menu_item_checked(
        &snapshot,
        MENU_PAGES,
        ITEM_PAGE_CONTROLS
    ));
    assert!(menu_item_checked(&snapshot, MENU_PAGES, ITEM_PAGE_WINDOWS));
    assert_eq!(
        runner.window_count(),
        3,
        "two auxiliary windows must increase window count to 3"
    );
    assert!(
        menu_item_checked(&snapshot, MENU_STATE, ITEM_STATE_AUX_WINDOW),
        "Auxiliary window state checkmark must be set"
    );
    run_ticks(&mut runner, "Windows page to settle", 1);
    assert_window_stack(
        &mut runner,
        &["Stacked Inspector", "Auxiliary Window", "Toolbox Showcase"],
    );
    assert_window_geometry(
        &mut runner,
        "Stacked Inspector",
        (250, 330, 495, 575),
        (231, 329, 497, 577),
    );
    assert_window_geometry(
        &mut runner,
        "Auxiliary Window",
        (155, 180, 400, 500),
        (136, 179, 402, 502),
    );
    assert_window_geometry(
        &mut runner,
        "Toolbox Showcase",
        (50, 40, 420, 600),
        (31, 39, 422, 602),
    );
    assert_windows_repainted(&mut runner, "initial stacked windows");
    assert!(
        is_dark_chrome(screen_rgb(&mut runner, 231, 400)),
        "active zoomDocProc frame must draw its upper title-bar border"
    );
    assert!(
        is_dark_chrome(screen_rgb(&mut runner, 240, 572)),
        "active zoomDocProc frame must align its zoom-box edge with the scrollbar column"
    );
    let stripe_edge_pixels = [
        screen_rgb(&mut runner, 234, 559),
        screen_rgb(&mut runner, 235, 559),
    ];
    assert!(
        stripe_edge_pixels[0] != stripe_edge_pixels[1],
        "active zoomDocProc frame must extend its title stripes to the control column"
    );
    assert!(
        is_dark_chrome(screen_rgb(&mut runner, 493, 562)),
        "DrawGrowIcon must draw the active window's diagonal size grip"
    );

    // The overlap at (260, 360) must show the front stacked inspector, while
    // these two probes sample each window's unique colored body.  This is an
    // occlusion assertion independent of the exact font/chrome raster.
    let initial_overlap = screen_rgb(&mut runner, 330, 400);
    let initial_stack_body = screen_rgb(&mut runner, 430, 520);
    let initial_aux_body = screen_rgb(&mut runner, 220, 240);
    let initial_aux_edge = screen_rgb(&mut runner, 210, 230);
    assert_eq!(
        initial_overlap, initial_stack_body,
        "front stacked inspector must occlude the auxiliary window"
    );
    assert_ne!(
        initial_overlap, initial_aux_body,
        "overlap must not leak pixels from the window behind it"
    );
    assert_eq!(
        screen_rgb(&mut runner, 330, 485),
        initial_stack_body,
        "rear window chrome must not paint through the front window's content"
    );
    runner.set_mouse_position(550, 760);
    assert_reference_frame(&mut runner, "03-windows.png");

    // 3b. Hit-test the exposed left side of Auxiliary Window.  It starts
    // behind Stacked Inspector, so this click must activate and promote it.
    click_point(&mut runner, 240, 210);
    step_until(
        &mut runner,
        "activate auxiliary window through exposed content",
        |r| {
            r.window_stack_snapshot()
                .first()
                .is_some_and(|window| window.title == "Auxiliary Window")
        },
    );
    assert_window_stack(
        &mut runner,
        &["Auxiliary Window", "Stacked Inspector", "Toolbox Showcase"],
    );
    assert_windows_repainted(&mut runner, "auxiliary activation");
    let promoted_aux_overlap = screen_rgb(&mut runner, 330, 400);
    assert_eq!(
        promoted_aux_overlap, initial_aux_body,
        "activating Auxiliary Window must change the overlap's front pixels"
    );
    assert_reference_frame(&mut runner, "03-windows-aux-activated.png");

    // 3c. Drag the active auxiliary window by its title bar.  The old
    // structure area exposes the main/inspector windows and must be repainted.
    drag_mouse(&mut runner, 143, 200, 173, 235);
    run_ticks(&mut runner, "auxiliary window dragged", 1);
    assert_window_stack(
        &mut runner,
        &["Auxiliary Window", "Stacked Inspector", "Toolbox Showcase"],
    );
    assert_window_geometry(
        &mut runner,
        "Auxiliary Window",
        (185, 215, 430, 535),
        (166, 214, 432, 537),
    );
    assert_windows_repainted(&mut runner, "auxiliary move exposed regions");
    assert_eq!(
        screen_rgb(&mut runner, 390, 270),
        initial_aux_body,
        "moving Auxiliary Window must repaint its new visible body"
    );
    assert_ne!(
        screen_rgb(&mut runner, 210, 230),
        initial_aux_edge,
        "moving Auxiliary Window must repaint the body it exposed"
    );
    assert_reference_frame(&mut runner, "03-windows-moved.png");

    // 3d. Resize the same front window through its grow box and assert that
    // both its geometry and the behind-window visible regions are refreshed.
    drag_mouse(&mut runner, 425, 525, 450, 550);
    run_ticks(&mut runner, "auxiliary window resized", 1);
    assert_window_geometry(
        &mut runner,
        "Auxiliary Window",
        (185, 215, 455, 560),
        (166, 214, 457, 562),
    );
    assert_windows_repainted(&mut runner, "auxiliary resize exposed regions");
    assert_eq!(
        screen_rgb(&mut runner, 420, 520),
        initial_aux_body,
        "resizing Auxiliary Window must repaint its newly exposed body"
    );
    assert_reference_frame(&mut runner, "03-windows-resized.png");

    // 3e. The inspector's right-hand body is not covered by the resized
    // auxiliary window.  Clicking there must hit-test the inspector and move
    // it to the front, changing the overlap pixel to the inspector color.
    click_point(&mut runner, 480, 500);
    step_until(
        &mut runner,
        "activate stacked inspector through exposed content",
        |r| {
            r.window_stack_snapshot()
                .first()
                .is_some_and(|window| window.title == "Stacked Inspector")
        },
    );
    assert_window_stack(
        &mut runner,
        &["Stacked Inspector", "Auxiliary Window", "Toolbox Showcase"],
    );
    assert_windows_repainted(&mut runner, "inspector activation");
    let promoted_stack_overlap = screen_rgb(&mut runner, 330, 400);
    let promoted_stack_body = screen_rgb(&mut runner, 430, 520);
    let promoted_aux_body = screen_rgb(&mut runner, 240, 250);
    assert_eq!(
        promoted_stack_overlap, promoted_stack_body,
        "activating Stacked Inspector must restore its pixels over the overlap"
    );
    assert_ne!(
        promoted_stack_overlap, promoted_aux_body,
        "inspector overlap must occlude the moved auxiliary window"
    );
    assert_reference_frame(&mut runner, "03-windows-hit-test.png");

    // 3f. Close the front inspector through its go-away box.  DisposeWindow
    // must promote Auxiliary Window, repaint its newly exposed content, and
    // remove the closed record from the semantic stack.
    tracking_click(&mut runner, 240, 340);
    step_until(
        &mut runner,
        "close front inspector and promote auxiliary",
        |r| {
            let stack = r.window_stack_snapshot();
            r.window_count() == 2
                && stack
                    .first()
                    .is_some_and(|window| window.title == "Auxiliary Window")
                && stack
                    .iter()
                    .all(|window| window.title != "Stacked Inspector")
        },
    );
    assert_window_stack(&mut runner, &["Auxiliary Window", "Toolbox Showcase"]);
    assert_windows_repainted(&mut runner, "front inspector disposal");
    assert_reference_frame(&mut runner, "03-windows-promoted.png");

    // Dispose the promoted auxiliary too.  The original main document must
    // become active/frontmost, proving close/dispose promotion through the
    // complete stack and leaving the later pages' one-window contract intact.
    tracking_click(&mut runner, 175, 223);
    step_until(&mut runner, "dispose auxiliary and promote main", |r| {
        let stack = r.window_stack_snapshot();
        r.window_count() == 1
            && stack
                .first()
                .is_some_and(|window| window.title == "Toolbox Showcase")
    });
    assert_window_stack(&mut runner, &["Toolbox Showcase"]);
    assert_windows_repainted(&mut runner, "auxiliary disposal");
    let snapshot = runner.guest_menu_snapshot();
    assert!(
        !menu_item_checked(&snapshot, MENU_STATE, ITEM_STATE_AUX_WINDOW),
        "window state checkmark must clear after both auxiliary windows close"
    );
    let main_heading_pixel = screen_rgb(&mut runner, 75, 65);
    assert!(
        main_heading_pixel[0] < 250 || main_heading_pixel[1] < 250 || main_heading_pixel[2] < 250,
        "promoted main window must repaint representative page content"
    );
    assert_reference_frame(&mut runner, "03-windows-main-promoted.png");

    // 4. Switch to Drawing & 3D Bevels page after the complete window-stack
    // lifecycle has promoted the main document again.
    let completed_qd3d_frames = runner.completed_qd3d_frame_count();
    assert!(
        runner.select_guest_menu_item(MENU_PAGES, ITEM_PAGE_DRAWING),
        "failed to queue selection of Drawing page"
    );
    step_until(&mut runner, "switch to Drawing page", |r| {
        let snapshot = r.guest_menu_snapshot();
        let page_checked = menu_item_checked(&snapshot, MENU_PAGES, ITEM_PAGE_DRAWING);
        let aux_cleared = !menu_item_checked(&snapshot, MENU_STATE, ITEM_STATE_AUX_WINDOW);
        let one_window = r.window_count() == 1;
        page_checked && aux_cleared && one_window
    });
    step_until(
        &mut runner,
        "Drawing page and native QuickDraw 3D to finish",
        |r| {
            let [red, green, blue] = screen_rgb(r, (win_top + 110) as u16, (win_left + 345) as u16);
            let drawing_finished = red > 150 && green > 100 && blue < 100;
            drawing_finished && (!powerpc || r.completed_qd3d_frame_count() > completed_qd3d_frames)
        },
    );
    let snapshot = runner.guest_menu_snapshot();
    assert!(menu_item_checked(&snapshot, MENU_PAGES, ITEM_PAGE_DRAWING));
    runner.set_mouse_position(550, 760);
    assert_reference_frame(&mut runner, "04-drawing.png");
    assert_drawing_page_rendered(&mut runner, win_top, win_left);

    // 5. Switch to Game Preferences and test bidirectional control & submenu synchronization.
    assert!(
        runner.select_guest_menu_item(MENU_PAGES, ITEM_PAGE_PREFERENCES),
        "failed to queue selection of Game Preferences page"
    );
    step_until(&mut runner, "switch to Game Preferences page", |r| {
        let snapshot = r.guest_menu_snapshot();
        menu_item_checked(&snapshot, MENU_PAGES, ITEM_PAGE_PREFERENCES)
    });

    // 5a. Click "Recruit (Easy)" radio button: local (80, 320)
    click_point(&mut runner, win_top + 80, win_left + 320);
    step_until(&mut runner, "select Easy difficulty radio", |r| {
        let snapshot = r.guest_menu_snapshot();
        menu_item_checked(&snapshot, MENU_DIFFICULTY, ITEM_DIFF_EASY)
            && !menu_item_checked(&snapshot, MENU_DIFFICULTY, ITEM_DIFF_NORMAL)
    });
    let snapshot = runner.guest_menu_snapshot();
    assert!(menu_item_checked(
        &snapshot,
        MENU_DIFFICULTY,
        ITEM_DIFF_EASY
    ));

    // 5b. Click "Nightmare (Hard)" radio button: local (130, 320)
    click_point(&mut runner, win_top + 130, win_left + 320);
    step_until(&mut runner, "select Hard difficulty radio", |r| {
        let snapshot = r.guest_menu_snapshot();
        menu_item_checked(&snapshot, MENU_DIFFICULTY, ITEM_DIFF_HARD)
            && !menu_item_checked(&snapshot, MENU_DIFFICULTY, ITEM_DIFF_EASY)
    });

    // 5c. Toggle SFX checkbox off: local (80, 122)
    click_point(&mut runner, win_top + 80, win_left + 122);
    step_until(&mut runner, "toggle SFX checkbox off", |r| {
        let snapshot = r.guest_menu_snapshot();
        menu_item_checked(&snapshot, MENU_SOUND, ITEM_SOUND_MUSIC_ONLY)
    });

    // 5d. Toggle Music checkbox off: local (105, 122)
    click_point(&mut runner, win_top + 105, win_left + 122);
    step_until(&mut runner, "toggle Music checkbox off", |r| {
        let snapshot = r.guest_menu_snapshot();
        menu_item_checked(&snapshot, MENU_SOUND, ITEM_SOUND_MUTE)
    });

    // 5e. Click "Classic 2D Flat" renderer radio: local (205, 335)
    click_point(&mut runner, win_top + 205, win_left + 335);
    step_until(&mut runner, "select Flat renderer radio", |r| {
        let snapshot = r.guest_menu_snapshot();
        menu_item_checked(&snapshot, MENU_RENDERER, ITEM_RENDERER_FLAT)
    });

    // 5f. Test hierarchical submenu command directly updates preferences state
    assert!(
        runner.select_guest_menu_item(MENU_DIFFICULTY, ITEM_DIFF_NORMAL),
        "failed to select Veteran (Normal) from Difficulty submenu"
    );
    step_until(&mut runner, "submenu select Normal difficulty", |r| {
        let snapshot = r.guest_menu_snapshot();
        menu_item_checked(&snapshot, MENU_DIFFICULTY, ITEM_DIFF_NORMAL)
    });

    assert!(
        runner.select_guest_menu_item(MENU_SOUND, ITEM_SOUND_FULL),
        "failed to select Full Audio from Sound submenu"
    );
    step_until(&mut runner, "submenu select Full Audio", |r| {
        let snapshot = r.guest_menu_snapshot();
        menu_item_checked(&snapshot, MENU_SOUND, ITEM_SOUND_FULL)
    });

    assert!(
        runner.select_guest_menu_item(MENU_RENDERER, ITEM_RENDERER_BEVEL),
        "failed to select QD3D Bevels from Renderer submenu"
    );
    step_until(&mut runner, "submenu select QD3D Bevels", |r| {
        let snapshot = r.guest_menu_snapshot();
        menu_item_checked(&snapshot, MENU_RENDERER, ITEM_RENDERER_BEVEL)
    });

    // Exercise volume scrollbar: step left arrow, drag thumb, and step right arrow
    click_point(&mut runner, win_top + 203, win_left + 43);
    run_ticks(&mut runner, "step volume left", 1);
    drag_mouse(
        &mut runner,
        win_top + 203,
        win_left + 158,
        win_top + 203,
        win_left + 110,
    );
    run_ticks(&mut runner, "drag volume thumb left", 1);
    drag_mouse(
        &mut runner,
        win_top + 203,
        win_left + 110,
        win_top + 203,
        win_left + 158,
    );
    run_ticks(&mut runner, "drag volume thumb right", 1);
    click_point(&mut runner, win_top + 203, win_left + 207);
    run_ticks(&mut runner, "Preferences page to settle", 1);
    runner.set_mouse_position(550, 760);
    assert_reference_frame(&mut runner, "05-preferences.png");

    // 6. Hold open the nested File > Game Options parent so the hierarchical
    // indicator is part of the visual baseline, then exercise a leaf command.
    runner.set_mouse_position(10, 146);
    runner.push_mouse_down(10, 146);
    run_ticks(&mut runner, "File menu to open", 2);
    runner.set_mouse_position(49, 170);
    run_ticks(&mut runner, "Game Options parent to highlight", 4);
    assert_reference_frame(&mut runner, "06-nested-menus.png");
    runner.push_mouse_up(49, 170);
    run_ticks(&mut runner, "File menu to close", 1);
    assert!(runner.select_guest_menu_item(MENU_RENDERER, ITEM_RENDERER_CONTRAST));
    step_until(&mut runner, "nested menu select High Contrast", |r| {
        menu_item_checked(
            &r.guest_menu_snapshot(),
            MENU_RENDERER,
            ITEM_RENDERER_CONTRAST,
        )
    });

    // Restore QD3D Bevels through the deterministic semantic selector.
    assert!(runner.select_guest_menu_item(MENU_RENDERER, ITEM_RENDERER_BEVEL));
    step_until(&mut runner, "restore QD3D Bevels renderer", |r| {
        menu_item_checked(&r.guest_menu_snapshot(), MENU_RENDERER, ITEM_RENDERER_BEVEL)
    });

    // 7. Switch to Dialogs & Alerts page and exercise modal dialog and alert sessions.
    assert!(
        runner.select_guest_menu_item(MENU_PAGES, ITEM_PAGE_DIALOGS),
        "failed to queue selection of Dialogs page"
    );
    step_until(&mut runner, "switch to Dialogs page", |r| {
        let snapshot = r.guest_menu_snapshot();
        menu_item_checked(&snapshot, MENU_PAGES, ITEM_PAGE_DIALOGS)
    });

    // 6a. Open Modal Preferences Dialog via button: local (317, 130)
    click_point(&mut runner, win_top + 317, win_left + 130);
    run_ticks(&mut runner, "Modal preferences dialog to open", 2);
    assert_reference_frame(&mut runner, "07-modal-dialog.png");
    // Dialog bounds: {100, 130, 290, 470}
    // Click Checkbox item 4 (Enable 3D): global (155, 300)
    click_point(&mut runner, 155, 300);
    // Click OK button item 1: global (260, 405)
    click_point(&mut runner, 260, 405);
    run_ticks(&mut runner, "Modal dialog to close", 2);

    // 6b. Display About Alert via button: local (317, 325)
    click_point(&mut runner, win_top + 317, win_left + 325);
    run_ticks(&mut runner, "About alert to open", 2);
    assert_reference_frame(&mut runner, "08-alert.png");
    // Alert bounds: {130, 150, 260, 450}
    // Click OK button item 1: global (230, 395)
    click_point(&mut runner, 230, 395);
    run_ticks(&mut runner, "Alert to close", 2);

    runner.set_mouse_position(550, 760);
    assert_reference_frame(&mut runner, "09-dialogs.png");

    // 10. Switch to TextEdit page and exercise live buffer, justification & layout.
    assert!(
        runner.select_guest_menu_item(MENU_PAGES, ITEM_PAGE_TEXTEDIT),
        "failed to queue selection of TextEdit page"
    );
    step_until(&mut runner, "switch to TextEdit page", |r| {
        menu_item_checked(&r.guest_menu_snapshot(), MENU_PAGES, ITEM_PAGE_TEXTEDIT)
    });
    run_ticks(&mut runner, "TextEdit page to settle", 1);
    let original_text = concat!(
        "TextEdit manages styled and plain text formatting, automatic word wrapping, ",
        "selection highlighting, and clipboard scrap operations.\r\r",
        "Click to move the insertion point or drag across characters to select text."
    )
    .as_bytes()
    .to_vec();
    let initial_te = showcase_textedit(&mut runner);
    assert_eq!(initial_te.text, original_text);
    assert_eq!(initial_te.selection, (0, 0));
    assert!(initial_te.active);
    assert_eq!(initial_te.line_count, 6);
    assert_reference_frame(&mut runner, "10-te-initial.png");

    drag_mouse(
        &mut runner,
        win_top + 82,
        win_left + 35,
        win_top + 82,
        win_left + 110,
    );
    run_ticks(&mut runner, "TextEdit mouse selection", 1);
    let mouse_selection = showcase_textedit(&mut runner);
    // Both native Mac OS 8.1 oracles select offsets 0 through 15 for this drag.
    assert_eq!(mouse_selection.selection, (0, 15));
    assert_eq!(mouse_selection.text, original_text);
    assert!(mouse_selection.active);
    assert_reference_frame(&mut runner, "10-te-mouse-selected.png");
    let (width, _, rgb) = rendered_rgb(&mut runner);
    // The top edge is highlighted by both classic inverse video and the theme outline.
    let highlight_pixel = ((win_top as usize + 76) * width as usize + win_left as usize + 50) * 3;
    assert_ne!(
        &rgb[highlight_pixel..highlight_pixel + 3],
        &[255, 255, 255],
        "the selected text must be visibly highlighted"
    );

    click_point(&mut runner, win_top + 268, win_left + 297);
    step_until(&mut runner, "TextEdit reset selects first 14 bytes", |r| {
        showcase_textedit(r).selection == (0, 14)
    });
    assert_reference_frame(&mut runner, "10-te-selected.png");
    click_point(&mut runner, win_top + 268, win_left + 144);
    run_ticks(&mut runner, "TextEdit copy", 1);
    let copied = runner.text_edit_snapshot();
    assert_eq!(copied.private_scrap_length, 14);
    assert_eq!(copied.private_scrap, original_text[..14]);
    assert_eq!(showcase_textedit(&mut runner).text, original_text);
    assert_eq!(showcase_textedit(&mut runner).selection, (0, 14));
    assert_reference_frame(&mut runner, "10-te-copied.png");

    click_point(&mut runner, win_top + 268, win_left + 65);
    step_until(&mut runner, "TextEdit cut", |r| {
        showcase_textedit(r).text.len() == original_text.len() - 14
    });
    assert_eq!(showcase_textedit(&mut runner).text, original_text[14..]);
    assert_eq!(showcase_textedit(&mut runner).selection, (0, 0));
    assert_eq!(
        runner.text_edit_snapshot().private_scrap,
        original_text[..14]
    );
    assert_reference_frame(&mut runner, "10-te-cut.png");

    click_point(&mut runner, win_top + 268, win_left + 220);
    step_until(&mut runner, "TextEdit paste", |r| {
        showcase_textedit(r).text == original_text
    });
    assert_eq!(showcase_textedit(&mut runner).selection, (14, 14));
    assert_eq!(
        runner.text_edit_snapshot().private_scrap,
        original_text[..14]
    );
    assert_reference_frame(&mut runner, "10-te-pasted.png");

    click_point(&mut runner, win_top + 268, win_left + 297);
    step_until(&mut runner, "TextEdit select before typing", |r| {
        showcase_textedit(r).selection == (0, 14)
    });
    runner.push_key_down(0x07, b'x');
    runner.push_key_up(0x07, b'x');
    let mut typed_text = vec![b'x'];
    typed_text.extend_from_slice(&original_text[14..]);
    step_until(&mut runner, "TextEdit typing replaces selection", |r| {
        showcase_textedit(r).text == typed_text
    });
    assert_eq!(showcase_textedit(&mut runner).selection, (1, 1));
    assert_eq!(
        runner.text_edit_snapshot().private_scrap,
        original_text[..14]
    );
    assert_reference_frame(&mut runner, "10-te-typed.png");

    click_point(&mut runner, win_top + 268, win_left + 297);
    step_until(&mut runner, "TextEdit reset restores text", |r| {
        showcase_textedit(r).text == original_text
    });
    assert_eq!(showcase_textedit(&mut runner).selection, (0, 14));
    assert_reference_frame(&mut runner, "10-te-reset.png");

    // Click Center alignment radio: local (178, 445)
    click_point(&mut runner, win_top + 178, win_left + 445);
    run_ticks(&mut runner, "center alignment to settle", 2);
    assert_eq!(showcase_textedit(&mut runner).justification, 1);

    runner.set_mouse_position(550, 760);
    assert_reference_frame(&mut runner, "10-textedit.png");

    // 11. Activate a mixed-usage palette, draw with palette entries, then
    // animate three explicit device indexes without redrawing the swatches.
    assert!(
        runner.select_guest_menu_item(MENU_PAGES, ITEM_PAGE_PALETTES),
        "failed to queue selection of Palettes page"
    );
    step_until(&mut runner, "switch to Palettes page", |r| {
        menu_item_checked(&r.guest_menu_snapshot(), MENU_PAGES, ITEM_PAGE_PALETTES)
    });
    step_until(&mut runner, "indexed PICT transfer to render", |r| {
        screen_rgb(r, (win_top + 280) as u16, (win_left + 340) as u16) != [255, 255, 255]
    });
    let indexed_picture_rgb = [340, 365, 382, 400, 421, 450, 480, 500, 520]
        .map(|x| screen_rgb(&mut runner, (win_top + 280) as u16, (win_left + x) as u16));
    assert_eq!(
        indexed_picture_rgb,
        if powerpc {
            [
                [49, 255, 49],
                [49, 206, 49],
                [99, 206, 49],
                [99, 206, 99],
                [99, 156, 99],
                [156, 156, 99],
                [156, 99, 99],
                [206, 99, 156],
                [206, 49, 49],
            ]
        } else {
            [
                [84, 255, 84],
                [84, 218, 84],
                [135, 218, 84],
                [37, 27, 138],
                [135, 179, 135],
                [179, 179, 135],
                [179, 135, 135],
                [218, 135, 179],
                [218, 84, 84],
            ]
        },
        "DrawPicture and CopyBits must preserve the exact architecture-specific indexed PICT color sequence across CTables"
    );
    let initial_device_rgb =
        screen_rgb(&mut runner, (win_top + 130) as u16, (win_left + 100) as u16);
    assert_ne!(
        initial_device_rgb,
        [0, 0, 0],
        "mixed-usage palette must populate the indexed device CLUT with non-zero RGB"
    );
    let sample_x = win_left + 350;
    let sample_y = win_top + 130;
    let pict_band_sample = screen_rgb(&mut runner, sample_y as u16, sample_x as u16);
    assert!(
        pict_band_sample != [0, 0, 0],
        "indexed PICT band must resolve to non-black indexed pixels via offscreen GWorld and destination palette"
    );
    let same_device_left = screen_rgb(&mut runner, (win_top + 310) as u16, (win_left + 345) as u16);
    let same_device_middle =
        screen_rgb(&mut runner, (win_top + 310) as u16, (win_left + 410) as u16);
    let same_device_right =
        screen_rgb(&mut runner, (win_top + 310) as u16, (win_left + 505) as u16);
    assert!(
        same_device_left != [0, 0, 0]
            && same_device_middle != [0, 0, 0]
            && same_device_right != [0, 0, 0],
        "same-device indexed CopyBits must not remap a transient black device CTable to black"
    );
    assert!(
        same_device_left != same_device_middle
            && same_device_middle != same_device_right
            && same_device_left != same_device_right,
        "same-device indexed CopyBits must preserve three distinct positional device indexes"
    );
    let inverse_table_band =
        screen_rgb(&mut runner, (win_top + 331) as u16, (win_left + 450) as u16);
    if powerpc {
        assert!(
            inverse_table_band[0] < 16
                && inverse_table_band[1] < 16
                && inverse_table_band[2] > 80,
            "direct-color RGBForeColor must render the requested dark blue; got {inverse_table_band:?}"
        );
    } else {
        assert_eq!(
            inverse_table_band,
            [154, 147, 161],
            "8-bit RGBForeColor must use the screen GDevice inverse-table index and default gamma transfer when logical and hardware CLUTs differ"
        );
    }
    let initial_palette_rgb = [
        screen_rgb(&mut runner, (win_top + 130) as u16, (win_left + 100) as u16),
        screen_rgb(&mut runner, (win_top + 130) as u16, (win_left + 280) as u16),
        screen_rgb(&mut runner, (win_top + 130) as u16, (win_left + 440) as u16),
    ];
    assert_eq!(
        initial_palette_rgb,
        if powerpc {
            [[255, 255, 0], [255, 99, 0], [0, 206, 173]]
        } else {
            [[255, 255, 0], [255, 135, 0], [0, 218, 192]]
        },
        "initial palette swatches must retain their exact architecture-specific RGB values"
    );
    runner.set_mouse_position(550, 760);
    assert_reference_frame(&mut runner, "11-palette.png");

    // 12. Animate Palette button: local Rect (342, 40, 366, 230).
    click_point(&mut runner, win_top + 354, win_left + 135);
    run_ticks(&mut runner, "palette animation to settle", 1);
    let animated_palette_rgb = [
        screen_rgb(&mut runner, (win_top + 130) as u16, (win_left + 100) as u16),
        screen_rgb(&mut runner, (win_top + 130) as u16, (win_left + 280) as u16),
        screen_rgb(&mut runner, (win_top + 130) as u16, (win_left + 440) as u16),
    ];
    if powerpc {
        assert_eq!(
            animated_palette_rgb, initial_palette_rgb,
            "AnimateEntry must leave already-drawn pixels unchanged on a direct-color device"
        );
    } else {
        assert_eq!(
            animated_palette_rgb,
            [[255, 39, 179], [39, 231, 119], [63, 119, 255]],
            "AnimateEntry must apply the exact replacement CLUT colors without a redraw"
        );
    }
    runner.set_mouse_position(550, 760);
    assert_reference_frame(&mut runner, "12-palette-animated.png");

    // 13. Menu-bar hover selection: press mouse down in File, hover/drag
    // to Pages menu, and release over the Graphics item to select it.
    runner.set_mouse_position(10, 146); // Options menu bar cell
    runner.push_mouse_down(10, 146);
    run_ticks(&mut runner, "Menu tracking start", 2);

    runner.set_mouse_position(10, 56); // Drag across menu bar to Pages menu
    run_ticks(&mut runner, "Menu tracking hover to Pages", 2);

    runner.set_mouse_position(28, 56); // Hover down to Graphics item
    run_ticks(&mut runner, "Menu tracking hover to Graphics item", 2);
    assert_reference_frame(&mut runner, "13-menu-hover.png");
    runner.push_mouse_up(28, 56); // Release mouse to trigger selection

    step_until(&mut runner, "hover-select switch to Graphics page", |r| {
        let snapshot = r.guest_menu_snapshot();
        let graphics_checked = menu_item_checked(&snapshot, MENU_PAGES, ITEM_PAGE_GRAPHICS);
        let aux_cleared = !menu_item_checked(&snapshot, MENU_STATE, ITEM_STATE_AUX_WINDOW);
        let one_window = r.window_count() == 1;
        graphics_checked && aux_cleared && one_window
    });

    let snapshot = runner.guest_menu_snapshot();
    assert!(menu_item_checked(&snapshot, MENU_PAGES, ITEM_PAGE_GRAPHICS));
    assert!(!menu_item_checked(
        &snapshot,
        MENU_PAGES,
        ITEM_PAGE_CONTROLS
    ));
    assert!(!menu_item_checked(&snapshot, MENU_PAGES, ITEM_PAGE_WINDOWS));
    assert!(!menu_item_checked(&snapshot, MENU_PAGES, ITEM_PAGE_DRAWING));
    assert!(!menu_item_checked(
        &snapshot,
        MENU_PAGES,
        ITEM_PAGE_PREFERENCES
    ));
    assert!(!menu_item_checked(&snapshot, MENU_PAGES, ITEM_PAGE_DIALOGS));
    assert!(!menu_item_checked(
        &snapshot,
        MENU_PAGES,
        ITEM_PAGE_PALETTES
    ));
    assert!(!menu_item_checked(
        &snapshot,
        MENU_PAGES,
        ITEM_PAGE_TEXTEDIT
    ));
    assert!(!menu_item_checked(&snapshot, MENU_PAGES, ITEM_PAGE_LISTS));
    assert_eq!(
        runner.window_count(),
        1,
        "window count must remain 1 after returning to Graphics"
    );
    assert!(
        !menu_item_checked(&snapshot, MENU_STATE, ITEM_STATE_AUX_WINDOW),
        "Auxiliary window state checkmark must remain cleared"
    );
    run_ticks(&mut runner, "returned Graphics page to settle", 1);
    assert_graphics_page_rendered(&mut runner);
    runner.set_mouse_position(550, 760);
    assert_reference_frame(&mut runner, "14-graphics-return.png");

    // 15. Switch to Lists & Inventory and exercise selection, cell access,
    // mutation, scrolling, resizing, and activation through the List Manager.
    assert!(
        runner.select_guest_menu_item(MENU_PAGES, ITEM_PAGE_LISTS),
        "failed to queue selection of Lists & Inventory page"
    );
    step_until(&mut runner, "switch to Lists & Inventory page", |r| {
        menu_item_checked(&r.guest_menu_snapshot(), MENU_PAGES, ITEM_PAGE_LISTS)
    });
    let snapshot = runner.guest_menu_snapshot();
    assert!(menu_item_checked(&snapshot, MENU_PAGES, ITEM_PAGE_LISTS));
    assert!(!menu_item_checked(
        &snapshot,
        MENU_PAGES,
        ITEM_PAGE_GRAPHICS
    ));
    assert!(!menu_item_checked(&snapshot, MENU_PAGES, ITEM_PAGE_SOUND));

    runner.set_mouse_position(550, 760);
    assert_reference_frame(&mut runner, "15-lists.png");
    let lists = runner.list_manager_snapshot();
    assert_eq!(lists.len(), 1);
    let initial_list = lists[0].clone();
    assert_eq!(initial_list.data_bounds, (0, 0, 12, 1));
    assert_eq!(initial_list.view_rect, (78, 24, 228, 528));
    assert_eq!(initial_list.cell_size, (18, 504));
    assert_eq!(initial_list.visible, (0, 0, 9, 1));
    assert!(initial_list.draw_enabled && initial_list.active);
    assert_eq!(initial_list.vertical_scrollbar, Some((true, 0)));
    assert!(initial_list.selected.is_empty());
    assert_eq!(initial_list.cells.len(), 12);
    assert_eq!(initial_list.cells[&(7, 0)], b"Signal Beacon       01  ready");


    // Select row 8 (zero-based row 7) in the initial 18-pixel cell layout.
    // The sample is in the row's empty right-hand background, so it observes
    // the List Manager's selection highlight rather than text rasterization.
    let list_row_v = win_top + 78 + (7 * 18) + 9;
    let list_row_h = win_left + 24 + 480;
    let unselected_row_rgb = screen_rgb(&mut runner, list_row_v as u16, list_row_h as u16);
    click_point(&mut runner, list_row_v, list_row_h);
    step_until(&mut runner, "select inventory row", |r| {
        screen_rgb(r, list_row_v as u16, list_row_h as u16) != unselected_row_rgb
    });

    // Inspect Selection calls LGetSelect/LGetCell again and keeps the same
    // selected-cell result visible in the inspector readout.
    click_point(&mut runner, win_top + 254, win_left + 80);
    run_ticks(&mut runner, "inspect selected inventory row", 1);
    runner.set_mouse_position(550, 760);
    let selected_frame = rendered_rgb(&mut runner).2;
    let selected_list = runner.list_manager_snapshot().remove(0);
    assert_eq!(selected_list.selected.iter().copied().collect::<Vec<_>>(), vec![(7, 0)]);
    assert_eq!(selected_list.cells, initial_list.cells);
    assert_reference_frame(&mut runner, "15-lists-selected.png");


    // Update Selected Row appends data obtained through LGetCell and writes
    // it back with LSetCell.
    click_point(&mut runner, win_top + 254, win_left + 205);
    run_ticks(&mut runner, "mutate selected inventory row", 1);
    runner.set_mouse_position(550, 760);
    let mutated_frame = rendered_rgb(&mut runner).2;
    let mutated_list = runner.list_manager_snapshot().remove(0);
    let mut expected_cells = initial_list.cells.clone();
    expected_cells.get_mut(&(7, 0)).unwrap().extend_from_slice(b"  * updated");
    assert_eq!(mutated_list.cells, expected_cells);
    assert_eq!(mutated_list.selected, selected_list.selected);
    assert_reference_frame(&mut runner, "15-lists-mutated.png");

    assert_ne!(
        selected_frame, mutated_frame,
        "LSetCell mutation must change the rendered list/status state"
    );

    // The four-row request is intentionally bounded by List Manager data
    // bounds when the visible page cannot expose a full four-row displacement.
    click_point(&mut runner, win_top + 254, win_left + 334);
    run_ticks(&mut runner, "scroll inventory list", 1);
    runner.set_mouse_position(550, 760);
    let scrolled_frame = rendered_rgb(&mut runner).2;
    let scrolled_list = runner.list_manager_snapshot().remove(0);
    assert_eq!(scrolled_list.visible, (4, 0, 12, 1));
    assert_eq!(scrolled_list.cells, expected_cells);
    assert_eq!(scrolled_list.selected, selected_list.selected);
    assert_reference_frame(&mut runner, "15-lists-scrolled.png");

    assert_ne!(
        mutated_frame, scrolled_frame,
        "LScroll must change the rendered list/status state"
    );

    click_point(&mut runner, win_top + 254, win_left + 442);
    run_ticks(&mut runner, "resize inventory list", 1);
    runner.set_mouse_position(550, 760);
    let resized_frame = rendered_rgb(&mut runner).2;
    let resized_list = runner.list_manager_snapshot().remove(0);
    assert_eq!(resized_list.view_rect, (78, 24, 192, 474));
    assert_eq!(resized_list.visible, (4, 0, 11, 1));
    assert_eq!(resized_list.cells, expected_cells);
    assert_reference_frame(&mut runner, "15-lists-resized.png");

    assert_ne!(
        scrolled_frame, resized_frame,
        "LSize must change the rendered list/status state"
    );

    // Toggle activation off and on so both LActivate paths are exercised.
    click_point(&mut runner, win_top + 286, win_left + 97);
    run_ticks(&mut runner, "deactivate inventory list", 1);
    runner.set_mouse_position(550, 760);
    let inactive_frame = rendered_rgb(&mut runner).2;
    let inactive_list = runner.list_manager_snapshot().remove(0);
    assert!(!inactive_list.active);
    assert_eq!(inactive_list.vertical_scrollbar, Some((true, 254)));
    assert_eq!(inactive_list.cells, expected_cells);
    assert_eq!(inactive_list.selected, selected_list.selected);
    assert_reference_frame(&mut runner, "15-lists-inactive.png");

    assert_ne!(
        resized_frame, inactive_frame,
        "LActivate(FALSE) must change the rendered list/status state"
    );

    click_point(&mut runner, win_top + 286, win_left + 97);
    run_ticks(&mut runner, "reactivate inventory list", 1);
    runner.set_mouse_position(550, 760);
    let active_frame = rendered_rgb(&mut runner).2;
    assert_ne!(
        inactive_frame, active_frame,
        "LActivate(TRUE) must change the rendered list/status state"
    );
    assert_reference_frame(&mut runner, "16-lists-interacted.png");
    let active_list = runner.list_manager_snapshot().remove(0);
    assert!(active_list.active);
    assert_eq!(active_list.vertical_scrollbar, Some((true, 0)));
    assert_eq!(active_list.cells, expected_cells);
    assert_eq!(active_list.selected, selected_list.selected);


    // 17. Activate Sound & Channels and exercise the high- and low-level
    // Sound Manager paths, including deterministic PCM output.
    assert!(
        runner.select_guest_menu_item(MENU_PAGES, ITEM_PAGE_SOUND),
        "failed to queue selection of Sound & Channels page"
    );
    let mut sound_page_rendered_streak = 0;
    step_until(&mut runner, "switch to Sound & Channels page", |r| {
        let ready = menu_item_checked(&r.guest_menu_snapshot(), MENU_PAGES, ITEM_PAGE_SOUND)
            && sound_page_rendered(r, win_top, win_left);
        if ready {
            sound_page_rendered_streak += 1;
        } else {
            sound_page_rendered_streak = 0;
        }
        // One frame can observe the panel/control drawing between guest
        // toolbox calls. Require the same semantic state after another slice
        // so the page's idle redraw has completed before input is sent.
        sound_page_rendered_streak >= 2
    });
    // Let the guest finish the redraw after the last semantic sample; the
    // condition itself is evaluated before the next emulation slice.
    run_ticks(&mut runner, "finish Sound & Channels page redraw", 1);
    step_until(&mut runner, "render Sound & Channels controls", |r| {
        sound_page_rendered(r, win_top, win_left)
    });
    assert!(
        sound_page_rendered(&mut runner, win_top, win_left),
        "Sound & Channels controls must be rendered before interaction"
    );
    assert_eq!(
        runner.dispatcher().sound_manager().channels.len(),
        1,
        "entering Sound & Channels must allocate one channel"
    );
    let sound_snapshot = runner.guest_menu_snapshot();
    assert!(
        !menu_item_checked(&sound_snapshot, MENU_STATE, ITEM_STATE_SOUND_COMPLETE),
        "sound completion must start unchecked"
    );
    runner.set_mouse_position(550, 760);
    assert_reference_frame(&mut runner, "17-sound-controls.png");

    // SysBeep uses the Sound Manager's internal alert channel and must produce
    // captured PCM without changing the page's explicitly allocated channel.
    click_point(&mut runner, win_top + 267, win_left + 62);
    run_audio(&mut runner, "SysBeep output", 4096);
    let beep_audio = runner.drain_audio();
    save_audio_evidence(&beep_audio, "beep");
    assert_non_silent_audio(&beep_audio, "SysBeep output");
    assert_eq!(
        runner.dispatcher().sound_manager().channels.len(),
        1,
        "completed SysBeep must release its temporary channel"
    );

    // SndPlay decodes the resource-backed format-1 sample into bufferCmd
    // playback. Queue a volume command and callback behind the sample, then
    // exercise immediate flush and quiet. The immediate pair intentionally
    // cancels the queued callback; the completion pass below queues a fresh
    // callback and verifies its guest-visible effect.
    click_point(&mut runner, win_top + 267, win_left + 157);
    run_audio(&mut runner, "SndPlay output", 16);
    let play_audio = runner.drain_audio();
    save_audio_evidence(&play_audio, "play");
    assert_non_silent_audio(&play_audio, "SndPlay output");
    assert!(
        runner.dispatcher().sound_manager().debug_buffer_cmd_count >= 1,
        "SndPlay must submit the resource's bufferCmd"
    );

    click_point(&mut runner, win_top + 267, win_left + 262);
    run_audio(&mut runner, "queued Sound Manager commands", 16);
    assert!(
        runner.dispatcher().sound_manager().channels[0].has_active_playback(),
        "queued commands must wait behind the sustained sample"
    );
    let queued_audio = runner.drain_audio();
    assert!(
        queued_audio.iter().any(|&sample| sample > 220)
            && queued_audio.iter().any(|&sample| sample < 36),
        "queued half-volume command must not reduce the currently playing sample"
    );
    click_point(&mut runner, win_top + 267, win_left + 352);
    run_audio(&mut runner, "immediate flush", 4);
    assert!(
        runner.dispatcher().sound_manager().channels[0].has_active_playback(),
        "flush must preserve current playback while removing queued commands"
    );
    click_point(&mut runner, win_top + 267, win_left + 422);
    run_audio(&mut runner, "immediate quiet", 4);
    assert!(
        !runner.dispatcher().sound_manager().channels[0].has_active_playback(),
        "quiet must stop the active sample"
    );
    runner.drain_audio();
    run_audio(&mut runner, "silence after quiet", 512);
    assert!(
        runner.drain_audio().iter().all(|&sample| sample == 128),
        "quiet must leave no audible output"
    );
    assert!(
        !menu_item_checked(
            &runner.guest_menu_snapshot(),
            MENU_STATE,
            ITEM_STATE_SOUND_COMPLETE
        ),
        "flush must cancel the queued completion callback"
    );
    let sound_manager = runner.dispatcher().sound_manager();
    assert!(
        sound_manager.debug_cmd_codes_seen.contains(&46),
        "queued volumeCmd must reach the Sound Manager"
    );
    assert!(
        sound_manager.debug_cmd_codes_seen.contains(&4),
        "immediate flushCmd must reach the Sound Manager"
    );
    assert!(
        sound_manager.debug_cmd_codes_seen.contains(&3),
        "immediate quietCmd must reach the Sound Manager"
    );
    // Play again and wait for a fresh callback flag to be reflected in the
    // State menu. The audio assertion covers the host PCM boundary; the menu
    // assertion covers guest-visible completion semantics.
    click_point(&mut runner, win_top + 302, win_left + 82);
    step_until_with_audio(
        &mut runner,
        "Sound Manager callback completion",
        1024,
        |r| {
            menu_item_checked(
                &r.guest_menu_snapshot(),
                MENU_STATE,
                ITEM_STATE_SOUND_COMPLETE,
            )
        },
    );
    let completion_audio = runner.drain_audio();
    save_audio_evidence(&completion_audio, "completion");
    assert_native_sample_audio(&completion_audio, "full");
    let sound_manager = runner.dispatcher().sound_manager();
    assert!(
        sound_manager.debug_buffer_cmd_count >= 2,
        "completion pass must submit a second resource bufferCmd"
    );
    assert!(
        sound_manager.debug_samples_mixed > 0,
        "Sound Manager playback must contribute mixed samples"
    );
    assert!(
        sound_manager.pending_sound_callbacks.is_empty(),
        "completion callback must be consumed"
    );
    assert!(
        sound_manager
            .channels
            .iter()
            .all(|channel| !channel.has_active_playback()),
        "completion pass must leave the channel idle"
    );
    let sound_snapshot = runner.guest_menu_snapshot();
    assert!(menu_item_checked(
        &sound_snapshot,
        MENU_STATE,
        ITEM_STATE_SOUND_COMPLETE
    ));

    // The first completion applied its queued 75% volume only after playback.
    // Replay at that volume, then queue 50% while idle and prove that gain too.
    for (checkpoint, half_volume) in [("volume75", false), ("volume50", true)] {
        run_ticks(&mut runner, "finish completion redraw before replay", 4);
        if half_volume {
            let previous_commands = runner.dispatcher().sound_manager().debug_cmd_count;
            click_point(&mut runner, win_top + 267, win_left + 262);
            step_until(&mut runner, "queue idle volume and callback", |r| {
                r.dispatcher().sound_manager().debug_cmd_count >= previous_commands + 2
            });
            // The headless fixture pumps audio explicitly, including idle
            // channels whose queued volume/callback commands need servicing.
            run_audio(&mut runner, "apply idle half-volume command", 512);
            run_ticks(&mut runner, "finish idle callback redraw", 4);
        }
        runner.drain_audio();
        let previous_buffers = runner.dispatcher().sound_manager().debug_buffer_cmd_count;
        click_point(&mut runner, win_top + 302, win_left + 82);
        step_until_with_audio(&mut runner, checkpoint, 1024, |r| {
            r.dispatcher().sound_manager().debug_buffer_cmd_count > previous_buffers
                && menu_item_checked(
                    &r.guest_menu_snapshot(),
                    MENU_STATE,
                    ITEM_STATE_SOUND_COMPLETE,
                )
        });
        let audio = runner.drain_audio();
        save_audio_evidence(&audio, checkpoint);
        assert_native_sample_audio(&audio, checkpoint);
    }

    // Hold State open so the callback checkmark is part of the exact frame,
    // then dispose the explicitly allocated channel through its button.
    runner.set_mouse_position(10, 108);
    runner.push_mouse_down(10, 108);
    run_ticks(&mut runner, "State menu to show sound completion", 4);
    assert_reference_frame(&mut runner, "18-sound-complete.png");
    runner.push_mouse_up(10, 108);
    run_ticks(&mut runner, "State menu to close after sound completion", 1);
    click_point(&mut runner, win_top + 302, win_left + 200);
    step_until(&mut runner, "dispose Sound Manager channel", |r| {
        r.dispatcher().sound_manager().channels.is_empty()
    });
    assert!(
        menu_item_checked(
            &runner.guest_menu_snapshot(),
            MENU_STATE,
            ITEM_STATE_SOUND_COMPLETE
        ),
        "sound completion checkmark must survive channel disposal"
    );

    // 19. Switch to Styled Text & Fonts and verify the live styled TextEdit,
    // Font Manager lookups, and measurement rulers.
    assert!(
        runner.select_guest_menu_item(MENU_PAGES, ITEM_PAGE_STYLED_TEXT),
        "failed to queue selection of Styled Text & Fonts page"
    );
    step_until(&mut runner, "switch to Styled Text & Fonts page", |r| {
        menu_item_checked(&r.guest_menu_snapshot(), MENU_PAGES, ITEM_PAGE_STYLED_TEXT)
    });
    let snapshot = runner.guest_menu_snapshot();
    assert!(menu_item_checked(
        &snapshot,
        MENU_PAGES,
        ITEM_PAGE_STYLED_TEXT
    ));
    assert!(!menu_item_checked(&snapshot, MENU_PAGES, ITEM_PAGE_LISTS));
    run_ticks(&mut runner, "Styled Text & Fonts page to settle", 1);
    assert_styled_text_page_rendered(&mut runner, win_top, win_left);
    runner.set_mouse_position(550, 760);
    assert_reference_frame(&mut runner, "19-styled-text.png");

    // 20. Exercise Standard File's modern and legacy Open/Save entry points.
    // The archive contains a folder with one TEXT and one DATA file. The
    // modern Open filter must enter the folder and return the TEXT FSSpec;
    // the other paths prove cancellation and editable Save names.
    assert!(
        runner.select_guest_menu_item(MENU_PAGES, ITEM_PAGE_STANDARD_FILE),
        "failed to queue selection of Standard File page"
    );
    step_until(&mut runner, "switch to Standard File page", |r| {
        menu_item_checked(
            &r.guest_menu_snapshot(),
            MENU_PAGES,
            ITEM_PAGE_STANDARD_FILE,
        )
    });
    runner.set_mouse_position(550, 760);
    let page_dialog_sample = screen_rgb(&mut runner, 212, 223);
    let page_save_dialog_sample = screen_rgb(&mut runner, 227, 221);
    let legacy_get_sample_point = if powerpc { (100, 100) } else { (50, 0) };
    let legacy_save_sample_point = if powerpc { (100, 100) } else { (227, 221) };
    let page_legacy_get_sample = screen_rgb(
        &mut runner,
        legacy_get_sample_point.0,
        legacy_get_sample_point.1,
    );
    let page_legacy_save_sample = screen_rgb(
        &mut runner,
        legacy_save_sample_point.0,
        legacy_save_sample_point.1,
    );
    let legacy_get_sample = legacy_get_sample_point;
    assert_reference_frame(&mut runner, "20-standard-file-page.png");

    // Modern StandardGetFile: the fixture folder is the selected first row.
    click_point(&mut runner, win_top + 216, win_left + 86);
    runner.set_mouse_position(550, 760);
    step_until_gui(&mut runner, "modern StandardGetFile dialog", |r| {
        screen_rgb(r, 212, 223) != page_dialog_sample
    });
    runner.set_mouse_position(550, 760);
    assert_reference_frame(&mut runner, "21-standard-file-open.png");

    // Open the selected folder, then accept its filtered TEXT row with Return.
    click_point(&mut runner, 211 + 148, 222 + 298);
    runner.set_mouse_position(550, 760);
    run_gui_ticks(&mut runner, "StandardGetFile folder navigation", 1);
    runner.push_key_down(0x24, b'\r');
    runner.push_key_up(0x24, b'\r');
    step_until_gui(&mut runner, "modern StandardGetFile acceptance", |r| {
        screen_rgb(r, 212, 223) == page_dialog_sample
    });

    // Legacy SFGetFile: cancel from a filtered dialog and verify the page
    // returns to its normal framebuffer before the next operation.
    click_point(&mut runner, win_top + 216, win_left + 226);
    runner.set_mouse_position(550, 760);
    step_until_gui(&mut runner, "legacy SFGetFile dialog", |r| {
        screen_rgb(r, legacy_get_sample.0, legacy_get_sample.1) != page_legacy_get_sample
    });
    runner.push_key_down(0x35, 0);
    runner.push_key_up(0x35, 0);
    step_until_gui(&mut runner, "legacy SFGetFile cancellation", |r| {
        screen_rgb(r, legacy_get_sample.0, legacy_get_sample.1) == page_legacy_get_sample
    });

    // Modern StandardPutFile: the default name is selected, so typing one
    // character replaces it; Return accepts and the app consumes the FSSpec
    // through FSpCreate/FSpDelete.
    click_point(&mut runner, win_top + 216, win_left + 360);
    runner.set_mouse_position(550, 760);
    step_until_gui(&mut runner, "modern StandardPutFile dialog", |r| {
        screen_rgb(r, 227, 221) != page_save_dialog_sample
    });
    runner.push_key_down(0x00, b'S');
    runner.push_key_up(0x00, b'S');
    runner.push_key_down(0x24, b'\r');
    runner.push_key_up(0x24, b'\r');
    step_until_gui(&mut runner, "modern StandardPutFile acceptance", |r| {
        screen_rgb(r, 227, 221) == page_save_dialog_sample
    });

    // Legacy SFPutFile: cancellation must leave its SFReply good bit false.
    click_point(&mut runner, win_top + 216, win_left + 480);
    runner.set_mouse_position(550, 760);
    step_until_gui(&mut runner, "legacy SFPutFile dialog", |r| {
        screen_rgb(r, legacy_save_sample_point.0, legacy_save_sample_point.1)
            != page_legacy_save_sample
    });
    runner.push_key_down(0x35, 0);
    runner.push_key_up(0x35, 0);
    step_until_gui(&mut runner, "legacy SFPutFile cancellation", |r| {
        screen_rgb(r, legacy_save_sample_point.0, legacy_save_sample_point.1)
            == page_legacy_save_sample
    });

    runner.set_mouse_position(550, 760);
    assert_reference_frame(&mut runner, "22-standard-file-complete.png");

    // 23. Switch to Resource Browser and exercise map enumeration, named
    // lookup/deferred loading, handle release, and reload. The controls are
    // local Rects (252..276) from left to right: Refresh Map, Load Named, and
    // Release Handle.
    assert!(
        runner.select_guest_menu_item(MENU_PAGES, ITEM_PAGE_RESOURCES),
        "failed to queue selection of Resource Browser page"
    );
    step_until(&mut runner, "switch to Resource Browser page", |r| {
        menu_item_checked(&r.guest_menu_snapshot(), MENU_PAGES, ITEM_PAGE_RESOURCES)
    });
    let snapshot = runner.guest_menu_snapshot();
    assert!(menu_item_checked(
        &snapshot,
        MENU_PAGES,
        ITEM_PAGE_RESOURCES
    ));
    assert!(!menu_item_checked(
        &snapshot,
        MENU_PAGES,
        ITEM_PAGE_STANDARD_FILE
    ));

    runner.set_mouse_position(550, 760);
    let enumerated_frame = rendered_rgb(&mut runner).2;
    assert_resource_browser_snapshot(&mut runner, None);
    assert_reference_frame(&mut runner, "23-resource-browser.png");

    // Refresh exercises a second Count1Resources/Get1IndResource pass and
    // must keep map-only enumeration deterministic.
    click_point(&mut runner, win_top + 264, win_left + 78);
    run_ticks(&mut runner, "refresh resource map", 1);
    runner.set_mouse_position(550, 760);
    assert_eq!(
        enumerated_frame,
        rendered_rgb(&mut runner).2,
        "refreshing the deferred map must preserve the deterministic frame"
    );
    assert_resource_browser_snapshot(&mut runner, None);

    click_point(&mut runner, win_top + 264, win_left + 200);
    run_ticks(&mut runner, "load named resource", 1);
    runner.set_mouse_position(550, 760);
    let loaded_frame = rendered_rgb(&mut runner).2;
    assert_resource_browser_snapshot(&mut runner, Some(203));
    assert_ne!(
        enumerated_frame, loaded_frame,
        "LoadResource must change the visible resource lifecycle state"
    );
    assert_reference_frame(&mut runner, "24-resource-browser-loaded.png");

    click_point(&mut runner, win_top + 264, win_left + 328);
    run_ticks(&mut runner, "release named resource", 1);
    runner.set_mouse_position(550, 760);
    let released_frame = rendered_rgb(&mut runner).2;
    assert_resource_browser_snapshot(&mut runner, None);
    assert_ne!(
        loaded_frame, released_frame,
        "ReleaseResource must change the visible resource lifecycle state"
    );
    assert_reference_frame(&mut runner, "25-resource-browser-released.png");

    // Reloading the same named record after ReleaseResource should restore
    // the exact loaded frame, proving that the map reference can be reused.
    click_point(&mut runner, win_top + 264, win_left + 200);
    run_ticks(&mut runner, "reload named resource", 1);
    runner.set_mouse_position(550, 760);
    assert_resource_browser_snapshot(&mut runner, Some(203));
    assert_eq!(
        loaded_frame,
        rendered_rgb(&mut runner).2,
        "released resource must reload to the same deterministic loaded frame"
    );

    // 22. Switch to Sprites, Masks & Scrolling and build the scene in
    // offscreen GWorlds. The page uses 1-bit CopyMask, 8-bit CopyDeepMask,
    // BitMapToRegion, and SetCPixel/GetCPixel before presenting through one
    // indexed-to-screen CopyBits transfer.
    assert!(
        runner.select_guest_menu_item(MENU_PAGES, ITEM_PAGE_SPRITES),
        "failed to queue selection of Sprites, Masks & Scrolling page"
    );
    step_until(
        &mut runner,
        "switch to Sprites, Masks & Scrolling page",
        |r| menu_item_checked(&r.guest_menu_snapshot(), MENU_PAGES, ITEM_PAGE_SPRITES),
    );
    let snapshot = runner.guest_menu_snapshot();
    assert!(menu_item_checked(&snapshot, MENU_PAGES, ITEM_PAGE_SPRITES));
    assert!(!menu_item_checked(
        &snapshot,
        MENU_PAGES,
        ITEM_PAGE_RESOURCES
    ));
    run_ticks(&mut runner, "sprite scene to settle", 1);
    runner.set_mouse_position(550, 760);
    let (initial_first_body, initial_second_body) =
        assert_sprites_page_rendered(&mut runner, win_top, win_left);
    assert!(
        initial_first_body[0] > initial_first_body[1].saturating_add(80)
            && initial_first_body[0] > initial_first_body[2].saturating_add(80),
        "initial CopyMask sprite must preserve its red source color: rgb={initial_first_body:?}"
    );
    assert!(
        initial_second_body[0] > initial_second_body[1].saturating_add(80)
            && initial_second_body[0] > initial_second_body[2].saturating_add(80),
        "initial CopyDeepMask sprite must preserve its red source color: rgb={initial_second_body:?}"
    );
    wait_for_page_event_loop(&mut runner, "initial sprite page redraw");
    let initial_frame = rendered_rgb(&mut runner).2;
    assert_reference_frame(&mut runner, "26-sprites.png");

    // 23. Animate the source sprite and rebuild the offscreen scene. This
    // must change the sprite pixels while leaving the matte/region pipeline
    // intact on both CPU slices.
    click_point(&mut runner, win_top + 316, win_left + 90);
    run_ticks(&mut runner, "animated sprite to settle", 1);
    let (animated_first_body, animated_second_body) =
        assert_sprites_page_rendered(&mut runner, win_top, win_left);
    assert!(
        animated_first_body[1] > animated_first_body[0].saturating_add(80)
            && animated_first_body[2] > animated_first_body[0].saturating_add(80),
        "animated CopyMask sprite must preserve its cyan source color: rgb={animated_first_body:?}"
    );
    assert!(
        animated_second_body[1] > animated_second_body[0].saturating_add(80)
            && animated_second_body[2] > animated_second_body[0].saturating_add(80),
        "animated CopyDeepMask sprite must preserve its cyan source color: rgb={animated_second_body:?}"
    );
    assert_ne!(
        initial_first_body, animated_first_body,
        "Animate Sprite must change the CopyMask sprite frame"
    );
    assert_ne!(
        initial_second_body, animated_second_body,
        "Animate Sprite must change the CopyDeepMask sprite frame"
    );
    wait_for_page_event_loop(&mut runner, "animated sprite page redraw");
    let animated_frame = rendered_rgb(&mut runner).2;
    assert_ne!(
        initial_frame, animated_frame,
        "animated sprite scene must change the framebuffer"
    );
    runner.set_mouse_position(550, 760);
    assert_reference_frame(&mut runner, "27-sprites-animated.png");

    // 24. Scroll the existing offscreen raster left. The first sprite's
    // center should move by 24 pixels, and ScrollRect must report nonzero
    // changed bytes plus the right-hand vacated strip.
    click_point(&mut runner, win_top + 316, win_left + 208);
    if powerpc {
        // Native PowerPC owns its QuickDraw state separately from the 68K
        // TrapDispatcher, so use the visible offscreen result as the
        // architecture-neutral ScrollRect checkpoint.
        step_until(&mut runner, "ScrollRect to move sprite scene", |r| {
            let shifted = screen_rgb(r, (win_top + 80 + 70) as u16, (win_left + 24 + 38) as u16);
            let old = screen_rgb(r, (win_top + 80 + 70) as u16, (win_left + 24 + 62) as u16);
            shifted == animated_first_body && old != animated_first_body
        });
    } else {
        let prior_scrolls = runner.dispatcher().debug_scroll_rect_nonzero_delta_count;
        step_until(&mut runner, "ScrollRect to move sprite scene", |r| {
            r.dispatcher().debug_scroll_rect_nonzero_delta_count > prior_scrolls
        });
        assert_eq!(
            runner.dispatcher().debug_scroll_rect_last_delta,
            (-24, 0),
            "first scene scroll must shift pixels left by 24"
        );
        assert!(
            runner.dispatcher().debug_scroll_rect_last_changed_bytes > 0,
            "ScrollRect must change bytes in the offscreen GWorld raster"
        );
    }
    let shifted_body = screen_rgb(
        &mut runner,
        (win_top + 80 + 70) as u16,
        (win_left + 24 + 38) as u16,
    );
    let old_center = screen_rgb(
        &mut runner,
        (win_top + 80 + 70) as u16,
        (win_left + 24 + 62) as u16,
    );
    assert_eq!(
        shifted_body, animated_first_body,
        "ScrollRect must move the sprite center 24 pixels left"
    );
    assert_ne!(
        old_center, animated_first_body,
        "ScrollRect must vacate the sprite's former center"
    );
    runner.set_mouse_position(550, 760);
    wait_for_page_event_loop(&mut runner, "scrolled sprite page redraw");
    assert_reference_frame(&mut runner, "28-sprites-scrolled.png");

    // Rebuild the original source after animation and scrolling. The native
    // full-page replay exposed an unlocked offscreen mask on first entry;
    // the initial scene must already agree with a later Reset Scene.
    click_point(&mut runner, win_top + 316, win_left + 318);
    run_ticks(&mut runner, "reset sprite scene", 2);
    runner.set_mouse_position(550, 760);
    assert_eq!(
        assert_sprites_page_rendered(&mut runner, win_top, win_left),
        (initial_first_body, initial_second_body),
        "reset must restore both masked sprite colors"
    );
    wait_for_page_event_loop(&mut runner, "reset sprite page redraw");
    let (reset_width, _, reset_frame) = rendered_rgb(&mut runner);
    for v in (win_top + 80)..(win_top + 260) {
        let start = (v as usize * reset_width as usize + (win_left + 24) as usize) * 3;
        let end = (v as usize * reset_width as usize + (win_left + 535) as usize) * 3;
        assert!(
            reset_frame[start..end] == initial_frame[start..end],
            "first and reset sprite raster/validation readouts differ at row {v}"
        );
    }
    assert_reference_frame(&mut runner, "28-sprites-reset.png");

    // 25. Visit Windows once more so the Events page records an activation
    // transition when the main window is selected back from the two-window
    // overlap stack. Their update events are retained in the page's lifecycle
    // summary.
    assert!(
        runner.select_guest_menu_item(MENU_PAGES, ITEM_PAGE_WINDOWS),
        "failed to queue selection of Windows page for activation probe"
    );
    step_until(
        &mut runner,
        "reopen Windows page for activation probe",
        |r| {
            let snapshot = r.guest_menu_snapshot();
            menu_item_checked(&snapshot, MENU_PAGES, ITEM_PAGE_WINDOWS)
                && menu_item_checked(&snapshot, MENU_STATE, ITEM_STATE_AUX_WINDOW)
                && r.window_count() == 3
        },
    );
    // Select exposed main-document content above and left of both auxiliary
    // structures to generate an activateEvt transition.
    click_point(&mut runner, 100, 100);
    run_ticks(&mut runner, "main window activation to settle", 2);

    assert!(
        runner.select_guest_menu_item(MENU_PAGES, ITEM_PAGE_EVENTS_CURSORS),
        "failed to queue selection of Events & Cursors page"
    );
    step_until(&mut runner, "switch to Events & Cursors page", |r| {
        menu_item_checked(
            &r.guest_menu_snapshot(),
            MENU_PAGES,
            ITEM_PAGE_EVENTS_CURSORS,
        ) && r.window_count() == 1
    });
    runner.set_mouse_position(550, 760);
    let events_initial = runner.event_manager_snapshot();
    assert!(
        events_initial.lifecycle_activation_seen,
        "Events page must expose an activation lifecycle event"
    );
    assert!(
        events_initial.lifecycle_update_seen,
        "Events page must expose an update lifecycle event"
    );
    assert_eq!(
        events_initial.mouse_position,
        runner.dispatcher().mouse_position(),
        "event snapshot mouse position must match the active input state"
    );
    assert_eq!(
        events_initial.queue_len,
        events_initial.queued_event_types.len(),
        "event snapshot queue length must match its event-type projection"
    );
    assert_reference_frame(&mut runner, "29-events-cursors.png");

    // 26. Hold the probe button down. The page samples GetMouse, Button,
    // StillDown, WaitMouseUp, and the EventAvail/OSEventAvail/GetOSEvent
    // queue sequence while the physical button remains down.
    let event_probe_v = win_top + 296;
    let event_probe_h = win_left + 108;
    runner.set_mouse_position(event_probe_v, event_probe_h);
    runner.push_mouse_down(event_probe_v, event_probe_h);
    run_ticks(&mut runner, "held event probe click", 2);
    runner.set_mouse_position(event_probe_v, event_probe_h);
    let held_events = runner.event_manager_snapshot();
    assert!(
        held_events.mouse_button,
        "held queue probe must expose the physical mouse button"
    );
    assert_eq!(
        held_events.mouse_position,
        (event_probe_v, event_probe_h),
        "held queue probe must preserve its global mouse coordinates"
    );
    assert_eq!(
        held_events.button_result,
        Some(true),
        "Button must report the held physical mouse state"
    );
    assert_eq!(
        held_events.still_down_result,
        Some(true),
        "StillDown must remain true while the probe button is held"
    );
    assert_eq!(
        held_events.wait_mouse_up_result,
        Some(true),
        "WaitMouseUp must remain true while the probe button is held"
    );
    let queue_probe = &held_events.queue_probe;
    assert_eq!(queue_probe.post_result, Some(0));
    let event_avail = queue_probe
        .event_avail
        .as_ref()
        .expect("EventAvail result must be captured");
    assert!(event_avail.available);
    assert_eq!(event_avail.record.what, 3);
    let os_event_avail = queue_probe
        .os_event_avail
        .as_ref()
        .expect("OSEventAvail result must be captured");
    assert!(os_event_avail.available);
    assert_eq!(os_event_avail.record.what, 3);
    let get_os_event = queue_probe
        .get_os_event
        .as_ref()
        .expect("GetOSEvent result must be captured");
    assert!(get_os_event.available);
    assert_eq!(get_os_event.record.what, 3);
    assert_eq!(
        event_avail.record, os_event_avail.record,
        "OSEventAvail must peek the same full EventRecord as EventAvail"
    );
    assert_eq!(
        os_event_avail.record, get_os_event.record,
        "GetOSEvent must consume the peeking EventRecord without mutation"
    );
    assert_eq!(event_avail.record.message, 0xA1B2C3D4);
    assert!(
        event_avail.record.when <= runner.guest_tick(),
        "posted EventRecord must retain a posting tick no later than the current guest tick"
    );
    assert_reference_frame(&mut runner, "30-events-mouse-held.png");
    runner.push_mouse_up(event_probe_v, event_probe_h);
    run_ticks(&mut runner, "event probe release", 2);

    // 27. Hold Shift while sending a printable key. The keyDown record must
    // carry shiftKey and GetKeys must report a nonzero physical map.
    let key_frame_marker_v = (win_top + 282) as u16;
    let key_frame_marker_h = (win_left + 420) as u16;
    let key_frame_marker = screen_rgb(&mut runner, key_frame_marker_v, key_frame_marker_h);
    let before_key_event = rendered_rgb(&mut runner).2;
    runner.push_key_down(0x38, 0);
    runner.push_key_down(0x00, b'E');
    run_until_frame_changes(&mut runner, "shift-modified key event", &before_key_event);
    let key_events = runner.event_manager_snapshot();
    assert!(
        key_events.key_map.iter().any(|byte| *byte != 0),
        "GetKeys semantic snapshot must expose the held Shift/key map"
    );
    assert!(
        key_events.last_record.as_ref().is_some_and(|record| {
            record.message != 0
                && record.when <= runner.guest_tick()
                && (record.modifiers & 0x0200) != 0
        }),
        "key EventRecord must expose the full message, posting tick, and shift modifier"
    );
    // Release as soon as the keyDown redraw starts. KeyUp does not redraw the
    // page, so the completed frame still records the keyDown and held map,
    // without allowing an architecture-dependent run of autoKey repeats.
    runner.push_key_up(0x00, b'E');
    runner.push_key_up(0x38, 0);
    run_until_frame_marker(
        &mut runner,
        "shift-modified key event redraw",
        key_frame_marker_v,
        key_frame_marker_h,
        key_frame_marker,
    );
    runner.set_mouse_position(550, 760);
    assert_reference_frame(&mut runner, "31-events-key-modifiers.png");
    run_ticks(&mut runner, "key release", 2);

    // 28. Switch through standard system cursors and exercise the balanced
    // HideCursor/ShowCursor level pair. The hotspot is part of the cursor
    // contract and is asserted independently of screenshot presentation.
    click_point(&mut runner, win_top + 262, win_left + 350);
    run_ticks(&mut runner, "cross cursor selection", 1);
    let (_, _, cross_hot_v, cross_hot_h) = runner
        .dispatcher()
        .cursor_data()
        .expect("cross cursor must install a cursor image");
    assert_eq!((cross_hot_v, cross_hot_h), (7, 7));
    assert!(runner.dispatcher().cursor_visible());

    click_point(&mut runner, win_top + 262, win_left + 420);
    run_ticks(&mut runner, "watch cursor selection", 1);
    let (_, _, watch_hot_v, watch_hot_h) = runner
        .dispatcher()
        .cursor_data()
        .expect("watch cursor must install a cursor image");
    assert_eq!((watch_hot_v, watch_hot_h), (8, 8));

    click_point(&mut runner, win_top + 294, win_left + 350);
    run_ticks(&mut runner, "hide cursor", 1);
    assert!(!runner.dispatcher().cursor_visible());
    let hidden_events = runner.event_manager_snapshot();
    assert!(!hidden_events.cursor_visible);
    assert!(hidden_events.cursor_level < 0);
    runner.set_mouse_position(550, 760);
    assert_reference_frame(&mut runner, "32-events-cursor-hidden.png");

    click_point(&mut runner, win_top + 294, win_left + 420);
    run_ticks(&mut runner, "show cursor", 1);
    assert!(runner.dispatcher().cursor_visible());
    click_point(&mut runner, win_top + 262, win_left + 490);
    run_ticks(&mut runner, "restore arrow cursor", 1);
    assert!(runner.dispatcher().cursor_visible());
    let final_events = runner.event_manager_snapshot();
    assert!(final_events.cursor_visible);
    assert_eq!(final_events.cursor_level, 0);
    runner.set_mouse_position(550, 760);
    assert_reference_frame(&mut runner, "33-events-cursors-final.png");
    // 28. Switch to Popup & Dropdown Lists. This page combines a resource-
    // backed CNTL/MENU popup with a programmatic NewMenu/NewControl popup.
    // The menu snapshots are the semantic assertions for the Control Manager
    // values and marks; framebuffer references cover the live dropdown and
    // the closed-control repaint after retained tracking ends.
    assert!(
        runner.select_guest_menu_item(MENU_PAGES, ITEM_PAGE_POPUP_LISTS),
        "failed to queue selection of Popup & Dropdown Lists page"
    );
    step_until(&mut runner, "switch to Popup & Dropdown Lists page", |r| {
        menu_item_checked(&r.guest_menu_snapshot(), MENU_PAGES, ITEM_PAGE_POPUP_LISTS)
    });
    let snapshot = runner.guest_menu_snapshot();
    assert!(menu_item_checked(
        &snapshot,
        MENU_PAGES,
        ITEM_PAGE_POPUP_LISTS
    ));
    assert!(!menu_item_checked(&snapshot, MENU_PAGES, ITEM_PAGE_SPRITES));
    assert_popup_menu_contract(&snapshot);
    assert!(menu_item_checked(
        &snapshot,
        MENU_POPUP_LOADOUT,
        ITEM_POPUP_LOADOUT_SCOUT
    ));
    assert!(menu_item_checked(
        &snapshot,
        MENU_POPUP_THEME,
        ITEM_POPUP_THEME_CLASSIC
    ));
    run_ticks(&mut runner, "popup page to settle", 1);
    assert_popup_page_rendered(&mut runner, win_top, win_left);
    assert_popup_selected_title_pixels(&mut runner, win_top, win_left, "Classic");
    runner.set_mouse_position(550, 760);
    let popup_initial_frame = rendered_rgb(&mut runner).2;
    assert_reference_frame(&mut runner, "34-popup-lists.png");
    let hidden_list = runner.list_manager_snapshot().remove(0);
    assert!(!hidden_list.active && !hidden_list.draw_enabled);
    assert_eq!(hidden_list.vertical_scrollbar.map(|bar| bar.0), Some(false));


    // Open the resource-backed popup and move over its separator and then its
    // disabled row. Both rows must remain unhighlighted and release must
    // retain item 1. The exact open frame proves the menu is a live overlay,
    // not just a static redraw of the closed control.
    let resource_popup_v = win_top + 112;
    let resource_popup_h = win_left + 280;
    runner.set_mouse_position(resource_popup_v, resource_popup_h);
    runner.push_mouse_down(resource_popup_v, resource_popup_h);
    run_popup_tracking_tick(&mut runner, "resource popup to open");
    step_until_gui(&mut runner, "resource popup overlay to be painted", |r| {
        rendered_rgb(r).2 != popup_initial_frame
    });
    let resource_open_frame = rendered_rgb(&mut runner).2;
    assert!(
        popup_initial_frame != resource_open_frame,
        "resource popup must paint a live dropdown over the page"
    );
    // Item 3 is the separator: rows 1 and 2 are 16 pixels each.
    runner.set_mouse_position(win_top + 135, resource_popup_h);
    run_popup_tracking_tick(&mut runner, "resource popup separator hover");
    // Item 5 is disabled: two ordinary rows + six-pixel separator + item 4.
    runner.set_mouse_position(win_top + 162, resource_popup_h);
    run_popup_tracking_tick(&mut runner, "resource popup disabled-row hover");
    runner.set_mouse_position(550, 760);
    assert_reference_frame(&mut runner, "35-popup-lists-open.png");
    runner.push_mouse_up(win_top + 162, resource_popup_h);
    run_popup_tracking_tick(&mut runner, "resource popup disabled release");
    step_until(&mut runner, "resource popup no-selection restore", |r| {
        let snapshot = r.guest_menu_snapshot();
        menu_item_checked(&snapshot, MENU_POPUP_LOADOUT, ITEM_POPUP_LOADOUT_SCOUT)
            && !menu_item_checked(&snapshot, MENU_POPUP_LOADOUT, ITEM_POPUP_LOADOUT_LONG)
    });
    let restored_after_disabled = rendered_rgb(&mut runner).2;
    assert_ne!(
        resource_open_frame, restored_after_disabled,
        "resource popup release must restore the closed control"
    );

    // Reopen the same control, choose the long enabled row, and verify its
    // value/mark changed through the Control Manager path.
    runner.set_mouse_position(resource_popup_v, resource_popup_h);
    runner.push_mouse_down(resource_popup_v, resource_popup_h);
    run_popup_tracking_tick(&mut runner, "resource popup to reopen");
    runner.set_mouse_position(win_top + 146, resource_popup_h);
    run_popup_tracking_tick(&mut runner, "resource popup long-row hover");
    runner.push_mouse_up(win_top + 146, resource_popup_h);
    step_until(&mut runner, "resource popup long-row selection", |r| {
        let snapshot = r.guest_menu_snapshot();
        menu_item_checked(&snapshot, MENU_POPUP_LOADOUT, ITEM_POPUP_LOADOUT_LONG)
            && !menu_item_checked(&snapshot, MENU_POPUP_LOADOUT, ITEM_POPUP_LOADOUT_SCOUT)
    });

    // The programmatic fixed-width popup has its own retained tracking path.
    // First release on its disabled item 2, then scroll a genuinely long
    // menu through the real MenuRows indicators and choose item 36. This
    // covers no-selection restoration, content repaint while scrolling, and
    // a successful selection outside the initial viewport.
    let theme_popup_v = win_top + 148;
    let theme_popup_h = win_left + 280;
    runner.set_mouse_position(theme_popup_v, theme_popup_h);
    runner.push_mouse_down(theme_popup_v, theme_popup_h);
    run_popup_tracking_tick(&mut runner, "programmatic popup to open");
    runner.set_mouse_position(win_top + 160, win_left + 180);
    run_popup_tracking_tick(&mut runner, "programmatic popup disabled-row hover");
    runner.push_mouse_up(win_top + 160, win_left + 180);
    // The unchanged mark is already true while TrackControl is still held.
    // Consume mouse-up before queuing another mouse-down for this control.
    run_popup_tracking_tick(&mut runner, "programmatic popup disabled release");
    step_until(
        &mut runner,
        "programmatic popup no-selection restore",
        |r| {
            let snapshot = r.guest_menu_snapshot();
            menu_item_checked(&snapshot, MENU_POPUP_THEME, ITEM_POPUP_THEME_CLASSIC)
                && !menu_item_checked(&snapshot, MENU_POPUP_THEME, ITEM_POPUP_THEME_NIGHT)
        },
    );

    runner.set_mouse_position(theme_popup_v, theme_popup_h);
    runner.push_mouse_down(theme_popup_v, theme_popup_h);
    run_popup_tracking_tick(&mut runner, "programmatic popup to reopen for scrolling");
    // The shared standard popup layout leaves a small bottom shadow on the
    // 800x600 fixture. Holding over its down indicator (y=580) advances one
    // 16-pixel content row per retained tracking update.
    let popup_scroll_h = theme_popup_h;
    runner.set_mouse_position(580, popup_scroll_h);
    for step in 0..40 {
        runner.set_mouse_position(579 + (step % 2), popup_scroll_h);
        run_popup_tracking_tick(&mut runner, "programmatic popup scroll down");
    }
    // Item 36 is the Deep Field Archive row. At the bottom content origin
    // (-284), its row is y=266..282, safely above the down indicator.
    runner.set_mouse_position(274, popup_scroll_h);
    run_popup_tracking_tick(&mut runner, "programmatic popup reveal long-row selection");
    runner.set_mouse_position(550, 760);
    assert_reference_frame(&mut runner, "36-popup-lists-scrolled.png");
    runner.set_mouse_position(274, popup_scroll_h);
    run_popup_tracking_tick(&mut runner, "programmatic popup restore long-row hover");
    runner.push_mouse_up(274, popup_scroll_h);
    step_until(
        &mut runner,
        "programmatic popup long-row selection",
        |r| {
            let snapshot = r.guest_menu_snapshot();
            menu_item_checked(&snapshot, MENU_POPUP_THEME, ITEM_POPUP_THEME_DEEP_FIELD)
                && !menu_item_checked(&snapshot, MENU_POPUP_THEME, ITEM_POPUP_THEME_CLASSIC)
        },
    );

    runner.set_mouse_position(550, 760);
    assert_reference_frame(&mut runner, "36-popup-lists-deep-selected.png");

    // Reopen with the long item selected. The standard layout initially
    // centers that item near the control, so track upward to reveal item 4
    // before choosing Night Operations. This also verifies the indicator
    // direction reverses and the closed control title repaints again.
    runner.set_mouse_position(theme_popup_v, theme_popup_h);
    runner.push_mouse_down(theme_popup_v, theme_popup_h);
    run_popup_tracking_tick(&mut runner, "programmatic popup to reopen for Night Operations");
    let popup_scroll_up_h = theme_popup_h;
    runner.set_mouse_position(6, popup_scroll_up_h);
    for step in 0..40 {
        runner.set_mouse_position(6 + (step % 2), popup_scroll_up_h);
        run_popup_tracking_tick(&mut runner, "programmatic popup scroll up");
    }
    runner.set_mouse_position(50, popup_scroll_up_h);
    run_popup_tracking_tick(&mut runner, "programmatic popup item selection");
    runner.push_mouse_up(50, popup_scroll_up_h);
    step_until(
        &mut runner,
        "programmatic popup Night Operations selection",
        |r| {
            let snapshot = r.guest_menu_snapshot();
            menu_item_checked(&snapshot, MENU_POPUP_THEME, ITEM_POPUP_THEME_NIGHT)
                && !menu_item_checked(&snapshot, MENU_POPUP_THEME, ITEM_POPUP_THEME_CLASSIC)
        },
    );
    let final_snapshot = runner.guest_menu_snapshot();
    assert_popup_menu_contract(&final_snapshot);
    assert!(menu_item_checked(
        &final_snapshot,
        MENU_POPUP_LOADOUT,
        ITEM_POPUP_LOADOUT_LONG
    ));
    assert!(menu_item_checked(
        &final_snapshot,
        MENU_POPUP_THEME,
        ITEM_POPUP_THEME_NIGHT
    ));
    assert!(!menu_item_checked(
        &final_snapshot,
        MENU_POPUP_THEME,
        ITEM_POPUP_THEME_DEEP_FIELD
    ));
    assert_popup_selected_title_pixels(&mut runner, win_top, win_left, "Night Operations");
    runner.set_mouse_position(550, 760);
    assert_reference_frame(&mut runner, "37-popup-lists-selected.png");
}
