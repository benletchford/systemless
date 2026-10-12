//! Identical publication-driven input scripts for native and offscreen hosts.
use crate::{frame_snapshot::GuiFrame, menu::MenuPresenter, runtime_mailbox::RuntimeMailbox};
use std::path::Path;
pub struct Script {
    pub observed: u64,
    pub next: usize,
    pub actions: Vec<serde_json::Value>,
    input_delay: u64,
    pub latencies: Vec<(u64, f64)>,
    last_receipt: u64,
}
impl Script {
    pub fn load(path: Option<&Path>) -> Self {
        Self {
            observed: 0,
            next: 0,
            input_delay: 0,
            latencies: Vec::new(),
            last_receipt: 0,
            actions: path
                .map(|p| {
                    serde_json::from_slice(&std::fs::read(p).expect("read script"))
                        .expect("parse script")
                })
                .unwrap_or_default(),
        }
    }
    pub fn complete(&self) -> bool {
        self.next == self.actions.len()
    }
    pub fn observe(
        &mut self,
        frame: &GuiFrame,
        mailbox: &RuntimeMailbox,
        menus: &mut MenuPresenter,
    ) {
        self.observed += 1;
        if let Some((id, started)) = frame.input_receipt {
            if id > self.last_receipt && self.latencies.len() < 100_000 {
                self.last_receipt = id;
                self.latencies
                    .push((id, started.elapsed().as_secs_f64() * 1000.));
            }
        }

        while let Some(a) = self
            .actions
            .get(self.next)
            .filter(|a| a["after_updates"].as_u64().unwrap() + self.input_delay <= self.observed)
        {
            let ready = (!a["wait_for_no_dialog"].as_bool().unwrap_or(false)
                || frame.dialog.is_none())
                && a["wait_for_dialog_text"].as_str().is_none_or(|text| {
                    frame
                        .dialog
                        .as_ref()
                        .is_some_and(|d| d.items.iter().any(|i| i.text.contains(text)))
                });
            if !ready {
                assert!(
                    self.observed < a["after_updates"].as_u64().unwrap() + self.input_delay + 600,
                    "guest readiness timeout: {a}"
                );
                break;
            }
            self.input_delay = self.observed - a["after_updates"].as_u64().unwrap();
            eprintln!("[comparison-input] observed={} action={a}", self.observed);
            use crate::runtime_protocol::GuiCommand::*;
            let n = |k: &str| a[k].as_i64().unwrap();
            let cmd = match a["type"].as_str().unwrap() {
                "mouse_down" => MouseDown {
                    v: n("v") as i16,
                    h: n("h") as i16,
                },
                "mouse_up" => MouseUp {
                    v: n("v") as i16,
                    h: n("h") as i16,
                },
                "mouse_move" => MouseMove {
                    v: n("v") as i16,
                    h: n("h") as i16,
                },
                "key_down" => KeyDown {
                    key: n("key") as u8,
                    character: n("ch") as u8,
                },
                "key_up" => KeyUp {
                    key: n("key") as u8,
                    character: n("ch") as u8,
                },
                other => panic!("unsupported action {other}"),
            };
            if let Some(cmd) = menus.route(cmd, &frame.menus, frame.dialog.is_some()) {
                mailbox.send_measured(cmd).unwrap();
            }
            self.next += 1;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn readiness_delay_preserves_the_mouse_hold_interval() {
        let mut script = Script {
            observed: 0,
            next: 0,
            input_delay: 0,
            latencies: Vec::new(),
            last_receipt: 0,
            actions: serde_json::from_str(
                r#"[
   {"after_updates":2,"type":"mouse_down","v":10,"h":20,"wait_for_dialog_text":"Ready"},
   {"after_updates":4,"type":"mouse_up","v":10,"h":20}]
  "#,
            )
            .unwrap(),
        };
        let mailbox = RuntimeMailbox::new(|| {});
        let mut menus = MenuPresenter::default();
        let mut frame = GuiFrame::default();
        script.observe(&frame, &mailbox, &mut menus);
        script.observe(&frame, &mailbox, &mut menus);
        assert!(mailbox.next_command().is_none());
        frame.dialog = Some(systemless::runner::ComparisonDialog {
            guest_id: 1,
            bounds: (0, 0, 100, 100),
            font: (0, 12),
            default_item: 0,
            items: vec![systemless::runner::ComparisonDialogItem {
                number: 1,
                kind: 8,
                bounds: (0, 0, 10, 50),
                text: "Ready".into(),
                enabled: false,
                value: 0,
                selection: None,
                caret: false,
                pressed: false,
            }],
        });
        script.observe(&frame, &mailbox, &mut menus);
        assert_eq!(
            mailbox.next_command(),
            Some(crate::runtime_protocol::GuiCommand::MouseDown { v: 10, h: 20 })
        );
        script.observe(&frame, &mailbox, &mut menus);
        assert!(mailbox.next_command().is_none());
        script.observe(&frame, &mailbox, &mut menus);
        assert_eq!(
            mailbox.next_command(),
            Some(crate::runtime_protocol::GuiCommand::MouseUp { v: 10, h: 20 })
        );
        assert!(script.complete());
    }
}
