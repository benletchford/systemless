//! Integration test exercising Toolbox Showcase for issues #1078, #1081, #1264, and #1265.
#![allow(dead_code)]

use std::path::{Path, PathBuf};

use systemless::display::render_screen_with_gamma;
use systemless::game::{init_game, load_game, new_runner_with_screen_depth};
use systemless::menu_model::GuestMenuSnapshot;
use systemless::runner::FixtureRunner;

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

fn click_point(runner: &mut FixtureRunner, v: i16, h: i16) {
    runner.set_mouse_position(v, h);
    runner.push_mouse_down(v, h);
    runner.push_mouse_up(v, h);
}

fn sound_page_rendered(runner: &mut FixtureRunner, win_top: i16, win_left: i16) -> bool {
    // SetPage updates the menu state before DrawMainWindow paints the page.
    // The right-hand lifecycle panel is only present after DrawSoundPage has
    // completed, so its border is a semantic rendering gate for page actions.
    // Include the filled panel, its heading, and the final Dispose button:
    // outer panel borders are drawn before their contents and DrawControls,
    // so border-only checks can still catch a partial frame.
    [(82, 420), (160, 420), (94, 318), (237, 420), (160, 305), (160, 534),
     (290, 153), (313, 153)]
        .iter()
        .all(|(v, h)| {
            let [red, green, blue] = screen_rgb(
                runner,
                (win_top + v) as u16,
                (win_left + h) as u16,
            );
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

fn reference_path(powerpc: bool, filename: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/toolbox-showcase/reference")
        .join(if powerpc {
            "systemless-ppc"
        } else {
            "systemless-68k"
        })
        .join(filename)
}

fn update_references() -> bool {
    matches!(
        std::env::var(REFERENCE_UPDATE_ENV).ok().as_deref(),
        Some("1" | "true" | "True" | "TRUE" | "yes" | "Yes" | "YES")
    )
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
    if expected.as_raw() == &actual {
        return;
    }

    let differing_pixels = expected
        .as_raw()
        .chunks_exact(3)
        .zip(actual.chunks_exact(3))
        .filter(|(expected, actual)| expected != actual)
        .count();
    let actual_path = std::env::temp_dir().join(format!(
        "systemless-toolbox-showcase-{}-{filename}",
        if runner.is_powerpc_app() { "ppc" } else { "68k" }
    ));
    write_rgb(&actual_path, width, height, actual);
    panic!(
        "{} differs at {differing_pixels} of {} pixels; actual frame written to {}. Set {REFERENCE_UPDATE_ENV}=1 to accept new references",
        reference.display(),
        width * height,
        actual_path.display()
    );
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

fn run_audio(runner: &mut FixtureRunner, label: &str, samples: usize) {
    let (_steps, still_running) = runner.run_steps_with_audio(50_000, None, samples);
    assert!(
        still_running && !runner.is_halted(),
        "emulation halted while waiting for {label}"
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

    // Sunken gauge well green bar at local (110, 175)
    let [gauge_r, gauge_g, gauge_b] =
        screen_rgb(runner, (win_top + 110) as u16, (win_left + 175) as u16);
    assert!(
        gauge_g > gauge_r.saturating_add(30) && gauge_g > gauge_b.saturating_add(30),
        "Drawing page sunken gauge bar was not rendered: rgb=({gauge_r}, {gauge_g}, {gauge_b})"
    );
}

#[test]
fn test_toolbox_showcase() {
    let mut runner = new_runner_with_screen_depth(8);
    let powerpc = prefer_powerpc();
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
        has_pages_menu && has_options_menu && has_window && has_bounds && graphics_checked
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
        !menu_item_checked(&snapshot, MENU_PAGES, ITEM_PAGE_SOUND),
        "Sound & Channels page must not be checked initially"
    );
    assert!(
        !menu_item_checked(&snapshot, MENU_STATE, ITEM_STATE_SOUND_COMPLETE),
        "Sound completion must not be checked initially"
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

    // 3. Switch to Windows and create the auxiliary document window.
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
            let two_windows = r.window_count() == 2;
            page_checked && aux_state && two_windows
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
        2,
        "auxiliary window must increase window count to 2"
    );
    assert!(
        menu_item_checked(&snapshot, MENU_STATE, ITEM_STATE_AUX_WINDOW),
        "Auxiliary window state checkmark must be set"
    );
    run_ticks(&mut runner, "Windows page to settle", 1);
    runner.set_mouse_position(550, 760);
    assert_reference_frame(&mut runner, "03-windows.png");

    // 4. Switch to Drawing & 3D Bevels page (disposing auxiliary window).
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
    step_until(&mut runner, "Drawing page and native QuickDraw 3D to finish", |r| {
        let [red, green, blue] =
            screen_rgb(r, (win_top + 110) as u16, (win_left + 345) as u16);
        let drawing_finished = red > 150 && green > 100 && blue < 100;
        drawing_finished
            && (!powerpc || r.completed_qd3d_frame_count() > completed_qd3d_frames)
    });
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

    // Move volume scrollbar
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
        menu_item_checked(
            &r.guest_menu_snapshot(),
            MENU_PAGES,
            ITEM_PAGE_TEXTEDIT,
        )
    });
    // Click Center alignment radio: local (178, 445)
    click_point(&mut runner, win_top + 178, win_left + 445);
    run_ticks(&mut runner, "center alignment to settle", 2);

    runner.set_mouse_position(550, 760);
    assert_reference_frame(&mut runner, "10-textedit.png");

    // 11. Activate a mixed-usage palette, draw with palette entries, then
    // animate three explicit device indexes without redrawing the swatches.
    assert!(
        runner.select_guest_menu_item(MENU_PAGES, ITEM_PAGE_PALETTES),
        "failed to queue selection of Palettes page"
    );
    step_until(&mut runner, "switch to Palettes page", |r| {
        menu_item_checked(
            &r.guest_menu_snapshot(),
            MENU_PAGES,
            ITEM_PAGE_PALETTES,
        )
    });
    step_until(&mut runner, "indexed PICT transfer to render", |r| {
        screen_rgb(r, (win_top + 280) as u16, (win_left + 340) as u16)
            != [255, 255, 255]
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
    let initial_device_rgb = screen_rgb(
        &mut runner,
        (win_top + 130) as u16,
        (win_left + 100) as u16,
    );
    assert_ne!(
        initial_device_rgb, [0, 0, 0],
        "mixed-usage palette must populate the indexed device CLUT with non-zero RGB"
    );
    let sample_x = win_left + 350;
    let sample_y = win_top + 130;
    let pict_band_sample = screen_rgb(&mut runner, sample_y as u16, sample_x as u16);
    assert!(
        pict_band_sample != [0, 0, 0],
        "indexed PICT band must resolve to non-black indexed pixels via offscreen GWorld and destination palette"
    );
    let same_device_left = screen_rgb(
        &mut runner,
        (win_top + 310) as u16,
        (win_left + 345) as u16,
    );
    let same_device_middle = screen_rgb(
        &mut runner,
        (win_top + 310) as u16,
        (win_left + 410) as u16,
    );
    let same_device_right = screen_rgb(
        &mut runner,
        (win_top + 310) as u16,
        (win_left + 505) as u16,
    );
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
    let inverse_table_band = screen_rgb(
        &mut runner,
        (win_top + 331) as u16,
        (win_left + 450) as u16,
    );
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
        menu_item_checked(
            &r.guest_menu_snapshot(),
            MENU_PAGES,
            ITEM_PAGE_LISTS,
        )
    });
    let snapshot = runner.guest_menu_snapshot();
    assert!(menu_item_checked(&snapshot, MENU_PAGES, ITEM_PAGE_LISTS));
    assert!(!menu_item_checked(&snapshot, MENU_PAGES, ITEM_PAGE_GRAPHICS));
    assert!(!menu_item_checked(&snapshot, MENU_PAGES, ITEM_PAGE_SOUND));

    runner.set_mouse_position(550, 760);
    assert_reference_frame(&mut runner, "15-lists.png");

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

    // Update Selected Row appends data obtained through LGetCell and writes
    // it back with LSetCell.
    click_point(&mut runner, win_top + 254, win_left + 205);
    run_ticks(&mut runner, "mutate selected inventory row", 1);
    runner.set_mouse_position(550, 760);
    let mutated_frame = rendered_rgb(&mut runner).2;
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
    assert_ne!(
        mutated_frame, scrolled_frame,
        "LScroll must change the rendered list/status state"
    );

    click_point(&mut runner, win_top + 254, win_left + 442);
    run_ticks(&mut runner, "resize inventory list", 1);
    runner.set_mouse_position(550, 760);
    let resized_frame = rendered_rgb(&mut runner).2;
    assert_ne!(
        scrolled_frame, resized_frame,
        "LSize must change the rendered list/status state"
    );

    // Toggle activation off and on so both LActivate paths are exercised.
    click_point(&mut runner, win_top + 286, win_left + 97);
    run_ticks(&mut runner, "deactivate inventory list", 1);
    runner.set_mouse_position(550, 760);
    let inactive_frame = rendered_rgb(&mut runner).2;
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
    assert_non_silent_audio(&play_audio, "SndPlay output");
    assert!(
        runner.dispatcher().sound_manager().debug_buffer_cmd_count >= 1,
        "SndPlay must submit the resource's bufferCmd"
    );

    click_point(&mut runner, win_top + 267, win_left + 262);
    run_audio(&mut runner, "queued Sound Manager commands", 16);
    click_point(&mut runner, win_top + 267, win_left + 352);
    run_audio(&mut runner, "immediate flush", 4);
    click_point(&mut runner, win_top + 267, win_left + 422);
    run_audio(&mut runner, "immediate quiet", 4);
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
    step_until_with_audio(&mut runner, "Sound Manager callback completion", 512, |r| {
        menu_item_checked(
            &r.guest_menu_snapshot(),
            MENU_STATE,
            ITEM_STATE_SOUND_COMPLETE,
        )
    });
    let completion_audio = runner.drain_audio();
    assert_non_silent_audio(&completion_audio, "completed SndPlay output");
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
}
