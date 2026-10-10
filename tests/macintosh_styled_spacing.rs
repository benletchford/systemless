use std::path::PathBuf;
use systemless::systems::macintosh::{
    session::{MacintoshInput, MacintoshSession},
    runner::TextEditSnapshot,
};

fn await_styled(
    session: &mut MacintoshSession,
    ready: impl Fn(&TextEditSnapshot) -> bool,
) -> TextEditSnapshot {
    for _ in 0..400 {
        session.runner_mut().run_steps(10_000, None);
        let settled = session
            .runner()
            .event_manager_snapshot()
            .last_record
            .is_some_and(|event| event.what == 0);
        if let Some(record) = session
            .runner_mut()
            .text_edit_snapshot()
            .records
            .into_iter()
            .find(|record| settled && record.styled && record.drawing_intact && ready(record))
        {
            return record;
        }
        assert!(session.status().running);
    }
    panic!("guest styled spacing transition did not settle");
}

fn key(session: &mut MacintoshSession, mac_key: u8, character: u8) {
    session.deliver_input(MacintoshInput::KeyDown { mac_key, character });
    session.deliver_input(MacintoshInput::KeyUp { mac_key, character });
}

#[test]
fn guest_spacing_styles_keep_font_intent_selection_and_editing_across_modes() {
    for (powerpc, depth) in [(false, 1u16), (false, 8), (true, 8), (true, 16)] {
        let mut session = MacintoshSession::new(true, Some(if powerpc { 8 } else { depth }));
        session.runner_mut().set_prefer_powerpc_executables(powerpc);
        if powerpc {
            session
                .runner_mut()
                .set_powerpc_screen_depth(depth)
                .unwrap();
        }
        let app = session
            .load_path(
                &PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                    .join("tests/toolbox-showcase/toolbox-showcase.sit"),
            )
            .unwrap();
        session.initialize(&app);
        for _ in 0..400 {
            session.runner_mut().run_steps(10_000, None);
            if session
                .runner_mut()
                .guest_menu_snapshot()
                .menus
                .iter()
                .any(|menu| menu.id == 129 && !menu.items.is_empty())
            {
                break;
            }
        }
        assert_eq!(session.runner().is_powerpc_app(), powerpc);
        assert_eq!(
            session.runner().presented_screen_depth(),
            Some(u32::from(depth))
        );
        assert!(session.runner_mut().select_guest_menu_item(129, 11));
        let original = await_styled(&mut session, |record| {
            record
                .style_runs
                .as_ref()
                .is_some_and(|runs| runs.iter().any(|run| run.start == 26))
        });
        let runs = original.style_runs.as_ref().unwrap();
        let mut record = original.clone();
        for (character, mac_key, spacing) in [
            (b'c', 0x08, 32),
            (b'e', 0x0e, 64),
            (b'b', 0x0b, 96),
            (b'n', 0x2d, 0),
        ] {
            session.deliver_input(MacintoshInput::KeyDown {
                mac_key: 0x3a,
                character: 0,
            });
            key(&mut session, mac_key, character);
            session.deliver_input(MacintoshInput::KeyUp {
                mac_key: 0x3a,
                character: 0,
            });
            record = await_styled(&mut session, |next| {
                next.guest_id == original.guest_id
                    && next
                        .style_runs
                        .as_ref()
                        .is_some_and(|runs| runs.iter().all(|run| run.face & 96 == spacing))
            });
            assert_eq!(
                (record.generation, record.owner_port),
                (original.generation, original.owner_port)
            );
            assert_eq!(
                (&record.text, record.selection, record.active),
                (&original.text, original.selection, original.active)
            );
            assert_eq!(
                (record.dest_rect, record.view_rect, record.justification),
                (
                    original.dest_rect,
                    original.view_rect,
                    original.justification
                )
            );
            let mut expected = runs.clone();
            for run in &mut expected {
                run.face = (run.face & !96) | spacing;
            }
            assert_eq!(
                record.style_runs.as_ref().unwrap(),
                &expected,
                "PPC={powerpc} depth={depth} spacing={spacing}"
            );
        }
        for (character, mac_key, spacing) in [(b'c', 0x08, 32), (b'e', 0x0e, 64)] {
            session.deliver_input(MacintoshInput::KeyDown {
                mac_key: 0x3a,
                character: 0,
            });
            key(&mut session, mac_key, character);
            session.deliver_input(MacintoshInput::KeyUp {
                mac_key: 0x3a,
                character: 0,
            });
            record = await_styled(&mut session, |next| {
                next.style_runs
                    .as_ref()
                    .unwrap()
                    .iter()
                    .all(|run| run.face & 96 == spacing)
            });
            let geometry = record.guest_styled_line_geometry(0).unwrap().0;
            let dest = record.global_dest_rect.unwrap();
            let vertical = dest.0 - record.dest_rect.0 + geometry.top + geometry.ascent;
            let horizontal = dest.1 - record.dest_rect.1
                + geometry.left
                + record.guest_styled_range_width(0..20).unwrap();
            for input in [
                MacintoshInput::MouseDown {
                    vertical,
                    horizontal,
                },
                MacintoshInput::MouseUp {
                    vertical,
                    horizontal,
                },
            ] {
                session.deliver_input(input);
                for _ in 0..20 {
                    session.runner_mut().run_steps(10_000, None);
                }
            }
            await_styled(&mut session, |record| {
                record.active && record.selection == (20, 20)
            });
            key(&mut session, 0x0c, b'Q');
            let mut expected = original.text.clone();
            expected.insert(20, b'Q');
            await_styled(&mut session, |record| {
                record.text == expected
                    && record.selection == (21, 21)
                    && record
                        .style_runs
                        .as_ref()
                        .unwrap()
                        .iter()
                        .all(|run| run.face & 96 == spacing)
            });
            key(&mut session, 0x33, 8);
            await_styled(&mut session, |record| {
                record.text == original.text && record.selection == (20, 20)
            });
        }
        println!("PASS guest spacing style and editing: powerpc={powerpc} depth={depth}");
    }
}
