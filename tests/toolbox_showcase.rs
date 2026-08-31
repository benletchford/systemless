//! Integration test exercising Toolbox Showcase for issue #1064.

use systemless::display::render_screen_with_gamma;
use systemless::game::{init_game, load_game, new_runner_with_screen_depth};
use systemless::menu_model::GuestMenuSnapshot;
use systemless::runner::FixtureRunner;

const SHOWCASE_SIT: &[u8] = include_bytes!("toolbox-showcase/toolbox-showcase.sit");

const MENU_PAGES: i16 = 129;
const MENU_STATE: i16 = 130;

const ITEM_PAGE_GRAPHICS: i16 = 1;
const ITEM_PAGE_CONTROLS: i16 = 2;
const ITEM_PAGE_WINDOWS: i16 = 3;

const ITEM_STATE_BUTTON: i16 = 1;
const ITEM_STATE_CHECKBOX: i16 = 2;
const ITEM_STATE_SCROLLBAR: i16 = 3;
const ITEM_STATE_AUX_WINDOW: i16 = 4;

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

fn assert_graphics_page_rendered(runner: &mut FixtureRunner) {
    // WIND 128 begins at (50, 40); this samples the center of the red oval
    // drawn at local Rect(205, 55, 325, 135).
    let [red, green, blue] = screen_rgb(runner, 145, 305);
    assert!(
        red > green.saturating_add(80) && red > blue.saturating_add(80),
        "Graphics page red oval was not rendered: rgb=({red}, {green}, {blue})"
    );
}

#[test]
fn test_toolbox_showcase() {
    let mut runner = new_runner_with_screen_depth(8);
    runner
        .set_powerpc_screen_depth(8)
        .expect("8-bit PowerPC fixture screen must be supported");
    runner.set_app_start_time(0x1234_5678);
    let powerpc = prefer_powerpc();

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
        // Native PowerPC Window Manager state is owned by the PPC adapter;
        // the public dispatcher window probes currently describe only 68K.
        let has_window = powerpc || r.dispatcher().window_count() >= 1;
        let has_bounds = powerpc || r.dispatcher().window_bounds() != (0, 0, 0, 0);
        let graphics_checked = menu_item_checked(&snapshot, MENU_PAGES, ITEM_PAGE_GRAPHICS);
        has_pages_menu && has_window && has_bounds && graphics_checked
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
    if !powerpc {
        assert_eq!(
            runner.dispatcher().window_count(),
            1,
            "initial window count must be 1"
        );
    }
    assert!(
        !menu_item_checked(&snapshot, MENU_STATE, ITEM_STATE_AUX_WINDOW),
        "Auxiliary window state must be false initially"
    );
    assert_graphics_page_rendered(&mut runner);

    // 2. Exercise Pages menu 129: item 2 (Controls), item 3 (Windows), item 1 (Graphics).
    // Switch to Controls page (item 2).
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
    if !powerpc {
        assert_eq!(runner.dispatcher().window_count(), 1);
    }
    assert!(!menu_item_checked(
        &snapshot,
        MENU_STATE,
        ITEM_STATE_AUX_WINDOW
    ));

    // Switch to Windows page (item 3) -> should open auxiliary window.
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
            let two_windows = powerpc || r.dispatcher().window_count() == 2;
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
    if !powerpc {
        assert_eq!(
            runner.dispatcher().window_count(),
            2,
            "auxiliary window must increase window count to 2"
        );
    }
    assert!(
        menu_item_checked(&snapshot, MENU_STATE, ITEM_STATE_AUX_WINDOW),
        "Auxiliary window state checkmark must be set"
    );

    // Switch back to Graphics page (item 1) -> should dispose auxiliary window.
    assert!(
        runner.select_guest_menu_item(MENU_PAGES, ITEM_PAGE_GRAPHICS),
        "failed to queue selection of Graphics page"
    );
    step_until(
        &mut runner,
        "switch to Graphics page and dispose auxiliary window",
        |r| {
            let snapshot = r.guest_menu_snapshot();
            let page_checked = menu_item_checked(&snapshot, MENU_PAGES, ITEM_PAGE_GRAPHICS);
            let aux_cleared = !menu_item_checked(&snapshot, MENU_STATE, ITEM_STATE_AUX_WINDOW);
            let one_window = powerpc || r.dispatcher().window_count() == 1;
            page_checked && aux_cleared && one_window
        },
    );
    let snapshot = runner.guest_menu_snapshot();
    assert!(menu_item_checked(&snapshot, MENU_PAGES, ITEM_PAGE_GRAPHICS));
    assert!(!menu_item_checked(
        &snapshot,
        MENU_PAGES,
        ITEM_PAGE_CONTROLS
    ));
    assert!(!menu_item_checked(&snapshot, MENU_PAGES, ITEM_PAGE_WINDOWS));
    if !powerpc {
        assert_eq!(
            runner.dispatcher().window_count(),
            1,
            "auxiliary window disposal must return window count to 1"
        );
    }
    assert!(
        !menu_item_checked(&snapshot, MENU_STATE, ITEM_STATE_AUX_WINDOW),
        "Auxiliary window state checkmark must be cleared"
    );
    assert_graphics_page_rendered(&mut runner);

    // 3. Switch to Controls page and exercise controls with stable mouse clicks.
    assert!(
        runner.select_guest_menu_item(MENU_PAGES, ITEM_PAGE_CONTROLS),
        "failed to queue selection of Controls page for clicking"
    );
    step_until(
        &mut runner,
        "switch to Controls page before clicking",
        |r| {
            let snapshot = r.guest_menu_snapshot();
            menu_item_checked(&snapshot, MENU_PAGES, ITEM_PAGE_CONTROLS)
        },
    );

    let (win_top, win_left) = if powerpc {
        // Fixed WIND 128 content origin from showcase.r. The PPC adapter does
        // not yet project its window bounds through TrapDispatcher.
        (50, 40)
    } else {
        let (top, left, _bottom, _right) = runner.dispatcher().window_bounds();
        (top, left)
    };

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
}
