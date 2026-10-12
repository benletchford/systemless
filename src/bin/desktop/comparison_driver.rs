// Identical live-scene action driver for both presentation backends.
// Actions enter the production worker; no guest state is patched by this driver.
struct ComparisonDriver {
    actions: Vec<serde_json::Value>,
    observed: u64,
    next: usize,
    started: Instant,
    metrics: bool,
    last_receipt: u64,
    latencies: Vec<(u64, f64)>,
}
impl ComparisonDriver {
    fn from_env() -> Self {
        let actions: Vec<serde_json::Value> = std::env::var_os("SYSTEMLESS_COMPARISON_SCRIPT")
            .map(|path| {
                serde_json::from_slice(&std::fs::read(path).expect("read comparison script"))
                    .expect("parse comparison script")
            })
            .unwrap_or_default();
        let metrics =
            std::env::var_os("SYSTEMLESS_COMPARISON_METRICS").is_some() || !actions.is_empty();
        Self {
            metrics,
            actions,
            observed: 0,
            next: 0,
            started: Instant::now(),
            last_receipt: 0,
            latencies: Vec::new(),
        }
    }
    fn observe(&mut self, update: &Update, commands: &mpsc::Sender<Command>) {
        self.observed += 1;
        if let Some((id, received)) = update.input_receipt {
            if self.metrics && id > self.last_receipt && self.latencies.len() < 10_000 {
                self.last_receipt = id;
                let elapsed = received.elapsed().as_secs_f64() * 1000.;
                self.latencies.push((id, elapsed));
                eprintln!(
                    "[comparison-input] {}",
                    serde_json::json!({"id":id,"event_to_acknowledged_snapshot_ms":elapsed})
                );
            }
        }
        let Some(action) = self.actions.get(self.next) else {
            return;
        };
        if self.observed < action["after_updates"].as_u64().expect("after_updates") {
            return;
        }
        let ready =
            !action["wait_for_no_dialog"].as_bool().unwrap_or(false) || update.dialogs.is_empty();
        let ready = ready
            && (!action["wait_for_no_file"].as_bool().unwrap_or(false)
                || update.standard_file.is_none());
        let ready = ready
            && action["wait_for_dialog_text"].as_str().is_none_or(|text| {
                update
                    .dialogs
                    .iter()
                    .any(|dialog| dialog.items.iter().any(|item| item.text.contains(text)))
            });
        if !ready {
            assert!(
                self.observed < action["after_updates"].as_u64().unwrap() + 600,
                "comparison readiness timeout: {action}"
            );
            return;
        }
        let integer = |key: &str| {
            action[key]
                .as_i64()
                .unwrap_or_else(|| panic!("missing {key}: {action}"))
        };
        let command = match action["type"].as_str().expect("action type") {
            "mouse_down" => Some(Command::Input(MacintoshInput::MouseDown {
                vertical: integer("v") as i16,
                horizontal: integer("h") as i16,
            })),
            "mouse_up" => Some(Command::Input(MacintoshInput::MouseUp {
                vertical: integer("v") as i16,
                horizontal: integer("h") as i16,
            })),
            "mouse_move" => Some(Command::Input(MacintoshInput::MouseMove {
                vertical: integer("v") as i16,
                horizontal: integer("h") as i16,
            })),
            "key_down" => Some(Command::Input(MacintoshInput::KeyDown {
                mac_key: integer("key") as u8,
                character: integer("ch") as u8,
            })),
            "key_up" => Some(Command::Input(MacintoshInput::KeyUp {
                mac_key: integer("key") as u8,
                character: integer("ch") as u8,
            })),
            "activate_dialog_button" => {
                let dialog = update
                    .dialogs
                    .iter()
                    .find(|d| d.active && d.visible)
                    .expect("active dialog");
                let item = dialog
                    .items
                    .iter()
                    .find(|i| i.number == integer("number") as i16)
                    .expect("dialog item");
                Some(Command::ActivateDialog(
                    dialog.guest_id,
                    dialog.generation,
                    item.number,
                    item.control_identity,
                ))
            }
            "commit_dialog_text" => {
                let owner = comparison_text_owner(update).expect("dialog text owner");
                let bytes = action["text"]
                    .as_str()
                    .expect("commit text")
                    .chars()
                    .map(systemless::systems::macintosh::mac_roman::encode_mac_roman_char)
                    .collect::<Option<Vec<_>>>()
                    .expect("Mac Roman text");
                Some(Command::CommitText(owner, bytes))
            }
            "select_dialog_text" => Some(Command::SelectText(
                comparison_text_owner(update).expect("dialog text owner"),
                integer("start") as usize..integer("end") as usize,
            )),
            "wait_for_state" => None,
            unknown => panic!("unsupported comparison action {unknown}"),
        };
        if let Some(command) = command {
            let command = match command {
                Command::Input(input) => timed_input(input),
                other => other,
            };
            commands.send(command).expect("guest worker alive");
        }
        eprintln!(
            "[comparison-action] {}",
            serde_json::json!({"index": self.next, "observed": self.observed,
            "elapsed_ms": self.started.elapsed().as_secs_f64()*1000., "action": action})
        );
        self.next += 1;
    }
}
fn comparison_text_owner(update: &Update) -> Option<super::input::TextInputOwner> {
    let owner = super::input::dialog_text_owner_with_records(
        &update.dialogs,
        &update.windows,
        &update.text_edits,
        &update.controls,
    )?;
    Some(super::input::TextInputOwner {
        identity: owner.identity,
        target: super::input::TextInputTarget::Dialog {
            item: owner.item,
            content_revision: owner.content_revision,
        },
        text: owner.text,
        selection: owner.selection,
    })
}

fn timed_input(input: MacintoshInput) -> Command {
    static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
    Command::TimedInput(
        input,
        NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed),
        Instant::now(),
    )
}
fn comparison_input(input: MacintoshInput) -> Command {
    if std::env::var_os("SYSTEMLESS_COMPARISON_METRICS").is_some() {
        timed_input(input)
    } else {
        Command::Input(input)
    }
}
