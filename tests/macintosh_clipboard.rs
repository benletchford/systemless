use std::path::PathBuf;
use systemless::systems::macintosh::session::{MacintoshInput, MacintoshSession};

#[path = "../src/bin/gpui_demo_clipboard.rs"]
mod clipboard;
use clipboard::{GuestClipboard, HostClipboard, HostSample};

fn settle(session: &mut MacintoshSession) {
    let start = session.runner().guest_tick();
    for _ in 0..100 {
        session.runner_mut().run_steps(10_000, None);
        if session.runner().guest_tick().wrapping_sub(start) >= 2 {
            return;
        }
    }
    panic!("guest input did not settle");
}

fn await_guest(
    session: &mut MacintoshSession,
    mut ready: impl FnMut(&mut MacintoshSession) -> bool,
) {
    for _ in 0..10_000 {
        session.runner_mut().run_steps(100, None);
        if ready(session) {
            return;
        }
        assert!(session.status().running);
    }
    panic!("guest clipboard transition did not settle");
}

fn click(session: &mut MacintoshSession, title: &str) {
    let control = session
        .runner_mut()
        .control_snapshot()
        .into_iter()
        .find(|c| c.title == title && c.visible)
        .expect("fixture control");
    let (top, left, bottom, right) = control.bounds;
    let vertical = (top + bottom) / 2;
    let horizontal = (left + right) / 2;
    session.deliver_input(MacintoshInput::MouseDown {
        vertical,
        horizontal,
    });
    settle(session);
    session.deliver_input(MacintoshInput::MouseUp {
        vertical,
        horizontal,
    });
    settle(session);
}

fn assert_documents_unchanged(
    session: &mut MacintoshSession,
    before: &systemless::runner::TextEditManagerSnapshot,
) {
    let after = session.runner_mut().text_edit_snapshot();
    assert_eq!(after.records.len(), before.records.len());
    for (a, b) in before.records.iter().zip(&after.records) {
        assert_eq!(a.guest_id, b.guest_id);
        assert_eq!(a.text, b.text);
        assert_eq!(a.selection, b.selection);
        assert_eq!(a.generation, b.generation);
        assert_eq!(a.owner_port, b.owner_port);
        assert_eq!(a.dest_rect, b.dest_rect);
        assert_eq!(a.view_rect, b.view_rect);
        assert_eq!(a.justification, b.justification);
        assert_eq!((a.font, a.face, a.size), (b.font, b.face, b.size));
        assert_eq!(a.style_runs, b.style_runs);
        assert_eq!(a.line_starts, b.line_starts);
        assert_eq!(a.line_metrics, b.line_metrics);
        assert_eq!(a.line_layout_policy, b.line_layout_policy);
    }
}

fn textedit_session(powerpc: bool, depth: u16) -> MacintoshSession {
    let mut session = MacintoshSession::new(true, Some(if powerpc { 8 } else { depth }));
    if powerpc {
        session
            .runner_mut()
            .set_powerpc_screen_depth(depth)
            .unwrap();
    }
    session.runner_mut().set_prefer_powerpc_executables(powerpc);
    let app = session
        .load_path(
            &PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("tests/toolbox-showcase/toolbox-showcase.sit"),
        )
        .unwrap();
    session.initialize(&app);
    await_guest(&mut session, |s| {
        s.runner_mut()
            .guest_menu_snapshot()
            .menus
            .iter()
            .any(|m| m.id == 129 && m.items.iter().any(|i| i.number == 1 && i.checked))
    });
    assert_eq!(session.runner().is_powerpc_app(), powerpc);
    assert_eq!(
        session.runner().presented_screen_depth(),
        Some(u32::from(depth))
    );
    assert!(session.runner_mut().select_guest_menu_item(129, 7));
    await_guest(&mut session, |s| {
        s.runner_mut()
            .guest_menu_snapshot()
            .menus
            .iter()
            .any(|m| m.id == 129 && m.items.iter().any(|i| i.number == 7 && i.checked))
    });
    settle(&mut session);
    session
}

#[test]
fn guest_copy_resume_conversion_and_paste_across_cpu_depths() {
    for (powerpc, depth) in [(false, 1u16), (false, 8), (true, 8), (true, 16)] {
        let mut session = textedit_session(powerpc, depth);
        click(&mut session, "Reset");
        click(&mut session, "Copy");
        let before = session.runner_mut().text_edit_snapshot();
        let copied = before.private_scrap.clone();
        assert_eq!(
            copied.len(),
            14,
            "guest Copy must execute through Toolbox tracking"
        );
        let mut worker = GuestClipboard::default();
        let mut host = HostClipboard::default();
        let initial = HostSample {
            text: Some("old host".into()),
            text_only: true,
            revision: Some(1),
        };
        host.suspend(1, initial.clone());
        worker.foreground(false, 1);
        session.request_foreground(false);
        worker.observe(&session);
        assert!(
            worker.export.is_none(),
            "blur must wait for guest suspend conversion"
        );
        await_guest(&mut session, |s| {
            worker.observe(s);
            worker.export.is_some()
        });
        assert_eq!(worker.export, Some((1, copied.clone())));
        assert_eq!(
            session.clipboard_text_after_suspend(),
            Some(Some(copied.clone()))
        );
        assert_documents_unchanged(&mut session, &before);
        let exported = host.export(1, &copied, initial).unwrap();
        let written = HostSample {
            text: Some(exported),
            text_only: true,
            revision: Some(2),
        };
        host.record_export(written.clone());
        assert_eq!(
            host.changed_sample(written),
            None,
            "export must not echo as import"
        );
        worker.foreground(true, 2);
        host.resume();
        session.request_foreground(true);
        await_guest(&mut session, |s| {
            s.runner_mut()
                .text_edit_snapshot()
                .records
                .iter()
                .any(|r| r.active)
        });

        let sample = HostSample {
            text: Some("café\r\nsecond\nlast".into()),
            text_only: true,
            revision: Some(3),
        };
        let payload = b"caf\x8e\rsecond\rlast".to_vec();
        for (cycle, changed) in [(3, true), (5, false)] {
            worker.foreground(false, cycle);
            session.request_foreground(false);
            await_guest(&mut session, |s| s.clipboard_text_after_suspend().is_some());
            assert_documents_unchanged(&mut session, &before);
            let imported = host.changed_sample(sample.clone());
            assert_eq!(imported.is_some(), changed);
            if let Some(bytes) = imported {
                assert_eq!(bytes, payload);
                worker.imported(&bytes);
                session.import_clipboard_text(bytes);
                assert_eq!(
                    session.runner_mut().text_edit_snapshot().private_scrap,
                    copied,
                    "global import must not overwrite private scrap before guest resume"
                );
            }
            worker.foreground(true, cycle + 1);
            host.resume();
            session.request_foreground(true);
            let expected_message = if changed { 0x0100_0003 } else { 0x0100_0001 };
            let mut saw_resume = false;
            await_guest(&mut session, |s| {
                saw_resume |= s
                    .runner()
                    .event_manager_snapshot()
                    .last_record
                    .is_some_and(|e| e.what == 15 && e.message == expected_message);
                let state = s.runner_mut().text_edit_snapshot();
                saw_resume
                    && state.private_scrap == payload
                    && state.records.iter().any(|r| r.active)
            });
            assert_documents_unchanged(&mut session, &before);
            assert!(
                worker.export.is_none(),
                "resume must clear retained export candidates"
            );
        }
        let original = before
            .records
            .iter()
            .find(|r| r.active && r.selection.1 > r.selection.0)
            .unwrap();
        let mut expected = original.text[..original.selection.0].to_vec();
        expected.extend_from_slice(&payload);
        expected.extend_from_slice(&original.text[original.selection.1..]);
        click(&mut session, "Paste");
        let after = session.runner_mut().text_edit_snapshot();
        let pasted = after
            .records
            .iter()
            .find(|r| r.guest_id == original.guest_id)
            .unwrap();
        assert_eq!(
            pasted.text, expected,
            "guest Paste must replace the retained selection"
        );
        let insertion = original.selection.0 + payload.len();
        assert_eq!(pasted.selection, (insertion, insertion));
        println!("PASS clipboard Copy/export/import/resume/Paste: powerpc={powerpc} depth={depth}");
    }
}

#[test]
fn initial_foreground_host_clipboard_import_reaches_guest_private_scrap() {
    for (powerpc, depth) in [(false, 1u16), (false, 8), (true, 8), (true, 16)] {
        let mut session = textedit_session(powerpc, depth);
        click(&mut session, "Reset");
        click(&mut session, "Copy");
        let before = session.runner_mut().text_edit_snapshot();
        assert_eq!(before.private_scrap.len(), 14);
        let payload = b"initial caf\x8e\rclipboard".to_vec();
        session.import_clipboard_text(payload.clone());
        assert_eq!(
            session.runner_mut().text_edit_snapshot().private_scrap,
            before.private_scrap,
            "import must leave private conversion to the guest handler"
        );
        let mut saw_conversion = false;
        await_guest(&mut session, |s| {
            if let Some(event) = s.runner().event_manager_snapshot().last_record {
                assert_ne!(
                    event.message, 0x0100_0000,
                    "initial handoff must not suspend"
                );
                saw_conversion |= event.what == 15 && event.message == 0x0100_0003;
            }
            let state = s.runner_mut().text_edit_snapshot();
            saw_conversion
                && state.private_scrap == payload
                && state.records.iter().any(|r| r.active)
        });
        assert_documents_unchanged(&mut session, &before);
        println!("PASS initial active clipboard handoff: powerpc={powerpc} depth={depth}");
    }
}
