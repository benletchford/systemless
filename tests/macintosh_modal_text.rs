use std::path::PathBuf;
use systemless::systems::macintosh::session::{MacintoshInput, MacintoshSession};

fn wait_for_menu(session: &mut MacintoshSession, id: i16, item: i16, checked: bool) {
    for _ in 0..300 {
        session.runner_mut().run_steps(100_000, None);
        if session.runner_mut().guest_menu_snapshot().menus.iter().any(|menu|
            menu.id == id && menu.items.iter().any(|row| row.number == item && row.checked == checked)) {
            return;
        }
        assert!(session.status().running);
    }
    panic!("guest menu did not settle");
}

fn settle(session: &mut MacintoshSession) {
    let start = session.runner().guest_tick();
    for _ in 0..100 {
        session.runner_mut().run_steps(10_000, None);
        if session.runner().guest_tick().wrapping_sub(start) >= 2 { return; }
    }
    panic!("guest did not advance while tracking input");
}

#[test]
fn nested_modal_text_isolation_and_focus_restore_across_cpu_depths() {
    for (powerpc, depth) in [(false, 1u16), (false, 8), (true, 8), (true, 16)] {
        let mut session = MacintoshSession::new(true, Some(if powerpc { 8 } else { depth }));
        if powerpc { session.runner_mut().set_powerpc_screen_depth(depth).unwrap(); }
        session.runner_mut().set_prefer_powerpc_executables(powerpc);
        let app = session
            .load_path(
                &PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                    .join("tests/toolbox-showcase/toolbox-showcase.sit"),
            )
            .unwrap();
        session.initialize(&app);
        wait_for_menu(&mut session, 129, 1, true);
        assert_eq!(session.runner().is_powerpc_app(), powerpc);
        assert_eq!(session.runner().presented_screen_depth(), Some(u32::from(depth)));
        assert!(session.runner_mut().select_guest_menu_item(132, 7));
        let modeless = (0..300)
            .find_map(|_| {
                session.runner_mut().run_steps(100_000, None);
                session
                    .runner_mut()
                    .dialog_snapshot()
                    .into_iter()
                    .find(|dialog| dialog.visible && dialog.items.len() == 4)
            })
            .expect("modeless dialog should open");
        assert_eq!(modeless.items[3].text, "Pilot");

        assert!(session.runner_mut().select_guest_menu_item(132, 6));
        let modal = (0..300)
            .find_map(|_| {
                session.runner_mut().run_steps(100_000, None);
                session
                    .runner_mut()
                    .dialog_snapshot()
                    .into_iter()
                    .find(|dialog| dialog.visible && dialog.items.len() == 10)
            })
            .expect("nested modal dialog should open");
        assert!(modal.active);
        assert!(!session.runner().guest_menu_tracking_active());
        assert!(session.runner_mut().dialog_snapshot().iter().any(|current| {
            current.guest_id == modeless.guest_id
                        && current.generation == modeless.generation && current.visible && !current.active
        }), "covered modeless dialog must not present focus on {powerpc:?}");
        session.deliver_input(MacintoshInput::KeyDown {
            mac_key: 0x07,
            character: b'X',
        });
        session.deliver_input(MacintoshInput::KeyUp {
            mac_key: 0x07,
            character: b'X',
        });
        assert!((0..100).any(|_| {
            session.runner_mut().run_steps(100_000, None);
            let dialogs = session.runner_mut().dialog_snapshot();
            dialogs.iter().any(|current| {
                current.guest_id == modal.guest_id
                    && current.items[6].text == "XCade Connelly"
            }) && dialogs.iter().any(|current| {
                current.guest_id == modeless.guest_id
                        && current.generation == modeless.generation
                    && current.items[3].text == "Pilot"
            })
        }), "nested modal key input reached the wrong dialog on {powerpc:?}");

        let cancel = &modal.items[1];
        let cancel_point = (
            (cancel.bounds.0 + cancel.bounds.2) / 2,
            (cancel.bounds.1 + cancel.bounds.3) / 2,
        );
        session.deliver_input(MacintoshInput::MouseDown {
            vertical: cancel_point.0,
            horizontal: cancel_point.1,
        });
        settle(&mut session);
        session.deliver_input(MacintoshInput::MouseUp {
            vertical: cancel_point.0,
            horizontal: cancel_point.1,
        });
        assert!((0..100).any(|_| {
            session.runner_mut().run_steps(100_000, None);
            let dialogs = session.runner_mut().dialog_snapshot();
            dialogs.iter().all(|current| current.guest_id != modal.guest_id)
                && dialogs.iter().any(|current| {
                    current.guest_id == modeless.guest_id
                        && current.generation == modeless.generation
                        && current.visible
                        && current.active
                        && current.items[3].text == "Pilot"
                })
        }), "modeless dialog should regain focus after nested modal dismissal on {powerpc:?}");
        session.deliver_input(MacintoshInput::KeyDown {
            mac_key: 0x06,
            character: b'z',
        });
        session.deliver_input(MacintoshInput::KeyUp {
            mac_key: 0x06,
            character: b'z',
        });
        assert!((0..100).any(|_| {
            session.runner_mut().run_steps(100_000, None);
            session.runner_mut().dialog_snapshot().iter().any(|current| {
                current.guest_id == modeless.guest_id
                        && current.generation == modeless.generation
                    && current.active
                    && current.items[3].text == "zPilot"
            })
        }), "modeless edit should accept input after nested modal dismissal on {powerpc:?}");

        assert!(session.runner_mut().select_guest_menu_item(132, 6));
        let reopened = (0..300).find_map(|_| {
            session.runner_mut().run_steps(100_000, None);
            session.runner_mut().dialog_snapshot().into_iter()
                .find(|dialog| dialog.visible && dialog.items.len() == 10)
        }).expect("modal dialog should reopen");
        assert_ne!(reopened.generation, modal.generation,
            "a new modal lifetime must not inherit the disposed identity");
        assert!(reopened.active);
        assert!(session.runner_mut().dialog_snapshot().iter().any(|current|
            current.guest_id == modeless.guest_id && current.generation == modeless.generation
                && current.visible && !current.active && current.items[3].text == "zPilot"));
        let bounds = reopened.items[1].bounds;
        let vertical = (bounds.0 + bounds.2) / 2;
        let horizontal = (bounds.1 + bounds.3) / 2;
        session.deliver_input(MacintoshInput::MouseDown { vertical, horizontal });
        settle(&mut session);
        session.deliver_input(MacintoshInput::MouseUp { vertical, horizontal });
        assert!((0..100).any(|_| {
            session.runner_mut().run_steps(100_000, None);
            let dialogs = session.runner_mut().dialog_snapshot();
            dialogs.iter().all(|current| current.generation != reopened.generation)
                && dialogs.iter().any(|current| current.guest_id == modeless.guest_id
                    && current.generation == modeless.generation && current.active
                    && current.items[3].text == "zPilot")
        }), "reopened modal dismissal must restore the original modeless lifetime and text");
        eprintln!("PPC={powerpc} depth={depth}: modal lifetimes {} -> {}; pointer reused={}",
            modal.generation, reopened.generation, modal.guest_id == reopened.guest_id);

    }
}
