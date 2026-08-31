use systemless::debug_overlay::DebugOverlayFrameStats;
use systemless::game;
use systemless::menu_model::{GuestMenu, GuestMenuItem};
use systemless::runner::FixtureRunner;

const SHOWCASE: &[u8] = include_bytes!("fixtures/toolbox-showcase/ToolboxShowcase.sit");
const RUN_SLICE: usize = 500_000;
const MAX_SLICES: usize = 40;
const MAIN_WINDOW_TOP: i16 = 40;
const MAIN_WINDOW_LEFT: i16 = 20;

fn architecture_name(prefer_powerpc: bool) -> &'static str {
    if prefer_powerpc {
        "PowerPC"
    } else {
        "68K"
    }
}

fn diagnostics(runner: &FixtureRunner) -> String {
    runner
        .debug_overlay_snapshot(DebugOverlayFrameStats::default())
        .text()
}

fn run_until<F>(runner: &mut FixtureRunner, context: &str, mut condition: F)
where
    F: FnMut(&mut FixtureRunner) -> bool,
{
    if condition(runner) {
        return;
    }

    for _ in 0..MAX_SLICES {
        let (_, still_running) = runner.run_steps(RUN_SLICE, None);
        if condition(runner) {
            return;
        }
        assert!(
            still_running,
            "guest halted while {context}\n{}",
            diagnostics(runner)
        );
    }

    panic!(
        "guest did not reach {context} after {} instructions\n{}",
        RUN_SLICE * MAX_SLICES,
        diagnostics(runner)
    );
}

fn menu(runner: &mut FixtureRunner, menu_id: i16) -> GuestMenu {
    runner
        .guest_menu_snapshot()
        .menus
        .into_iter()
        .find(|menu| menu.id == menu_id)
        .unwrap_or_else(|| panic!("menu {menu_id} is missing\n{}", diagnostics(runner)))
}

fn item(runner: &mut FixtureRunner, menu_id: i16, item_number: i16) -> GuestMenuItem {
    menu(runner, menu_id)
        .items
        .into_iter()
        .find(|item| item.number == item_number)
        .unwrap_or_else(|| {
            panic!(
                "item {item_number} is missing from menu {menu_id}\n{}",
                diagnostics(runner)
            )
        })
}

fn select_menu_item<F>(
    runner: &mut FixtureRunner,
    menu_id: i16,
    item_number: i16,
    context: &str,
    condition: F,
) where
    F: FnMut(&mut FixtureRunner) -> bool,
{
    assert!(
        runner.select_guest_menu_item(menu_id, item_number),
        "could not select item {item_number} from menu {menu_id}\n{}",
        diagnostics(runner)
    );
    run_until(runner, context, condition);
}

fn assert_menu_contract(runner: &mut FixtureRunner) {
    for (menu_id, title) in [(129, "File"), (132, "Edit"), (130, "Pages"), (131, "Demo")] {
        let guest_menu = menu(runner, menu_id);
        assert_eq!(guest_menu.title, title);
        assert!(guest_menu.enabled);
        assert!(guest_menu.visible_in_menu_bar);
        assert!(!guest_menu.hierarchical);
    }

    let pages = menu(runner, 130);
    assert_eq!(
        pages
            .items
            .iter()
            .map(|item| item.text.as_str())
            .collect::<Vec<_>>(),
        [
            "Overview",
            "QuickDraw",
            "Controls",
            "TextEdit",
            "Windows",
            "Resources & Events",
        ]
    );
    assert!(pages.items[0].checked);
    assert!(pages.items.iter().skip(1).all(|item| !item.checked));

    let edit_separator = item(runner, 132, 2);
    assert!(edit_separator.separator);
    let demo_separator = item(runner, 131, 4);
    assert!(demo_separator.separator);
    assert!(!item(runner, 132, 1).enabled);
    assert!(!item(runner, 131, 5).enabled);
    assert!(!item(runner, 131, 6).enabled);
    assert!(!item(runner, 131, 7).enabled);

    let palette_parent = item(runner, 131, 8);
    assert_eq!(palette_parent.submenu_id, Some(200));
    let palette = menu(runner, 200);
    assert!(palette.hierarchical);
    assert!(!palette.visible_in_menu_bar);
    assert_eq!(
        palette
            .items
            .iter()
            .map(|item| item.text.as_str())
            .collect::<Vec<_>>(),
        ["Red", "Green", "Blue"]
    );
}

fn exercise_slice(prefer_powerpc: bool) {
    let architecture = architecture_name(prefer_powerpc);
    let mut runner = game::new_runner();
    runner.set_prefer_powerpc_executables(prefer_powerpc);
    let app = game::load_game(&mut runner, SHOWCASE)
        .unwrap_or_else(|error| panic!("failed to load {architecture} showcase: {error}"));
    game::init_game(&mut runner, &app);

    assert_eq!(
        runner.is_powerpc_app(),
        prefer_powerpc,
        "loader selected the wrong slice for {architecture}"
    );
    run_until(
        &mut runner,
        "the initial showcase window and menus",
        |runner| {
            let snapshot = runner.guest_menu_snapshot();
            [129, 130, 131, 132]
                .iter()
                .all(|id| snapshot.menus.iter().any(|menu| menu.id == *id))
        },
    );
    assert_menu_contract(&mut runner);

    for page_item in 1..=6 {
        select_menu_item(
            &mut runner,
            130,
            page_item,
            &format!("{architecture} page {page_item}"),
            |runner| {
                let pages = menu(runner, 130);
                pages.items.iter().filter(|item| item.checked).count() == 1
                    && pages.items[(page_item - 1) as usize].checked
            },
        );
    }

    select_menu_item(&mut runner, 130, 3, "the Controls page", |runner| {
        item(runner, 130, 3).checked
    });
    let checkbox_v = MAIN_WINDOW_TOP + 130;
    let checkbox_h = MAIN_WINDOW_LEFT + 173;
    runner.push_mouse_down(checkbox_v, checkbox_h);
    runner.push_mouse_up(checkbox_v, checkbox_h);
    run_until(&mut runner, "the checkbox click", |runner| {
        item(runner, 131, 5).checked
    });

    let scroll_v = MAIN_WINDOW_TOP + 337;
    let scroll_h = MAIN_WINDOW_LEFT + 578;
    runner.push_mouse_down(scroll_v, scroll_h);
    runner.push_mouse_up(scroll_v, scroll_h);
    run_until(&mut runner, "the vertical scrollbar click", |runner| {
        item(runner, 131, 6).checked
    });

    select_menu_item(&mut runner, 130, 4, "the TextEdit page", |runner| {
        item(runner, 130, 4).checked
    });
    runner.push_key_down(0x07, b'x');
    runner.push_key_up(0x07, b'x');
    run_until(&mut runner, "the TextEdit keystroke", |runner| {
        item(runner, 131, 7).checked
    });

    select_menu_item(&mut runner, 130, 5, "the Windows page", |runner| {
        item(runner, 130, 5).checked
    });
    select_menu_item(&mut runner, 129, 1, "the companion window", |runner| {
        item(runner, 129, 2).enabled
    });
    select_menu_item(
        &mut runner,
        129,
        2,
        "the companion window closing",
        |runner| !item(runner, 129, 2).enabled,
    );

    select_menu_item(&mut runner, 129, 4, "ExitToShell", |runner| {
        runner.is_halted()
    });
    assert!(
        runner.halted_by_exit_to_shell(),
        "{architecture} showcase did not quit cleanly\n{}",
        diagnostics(&runner)
    );
}

#[test]
fn toolbox_showcase_runs_on_68k() {
    exercise_slice(false);
}

#[test]
fn toolbox_showcase_runs_on_powerpc() {
    exercise_slice(true);
}
