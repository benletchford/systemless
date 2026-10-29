//! Frontend draft ownership and ordered synchronization with guest TextEdit.
use super::super::runtime_protocol::DialogTextAck;
use std::collections::{HashMap, HashSet};
use systemless::systems::macintosh::host_ui::{DialogReply, DialogSnapshot, DialogTextReply};
use systemless_ui::{dialog::EditorField, editor::Editor};

/// A child may obscure a parent without ending the parent's guest lifetime.
/// Only the active session can flush commands; acknowledgements follow their ID.
#[derive(Default)]
pub(super) struct DialogEditorSessions {
    pub active: DialogEditors,
    suspended: HashMap<u64, DialogEditors>,
}
impl DialogEditorSessions {
    pub fn sync(&mut self, snapshot: Option<&DialogSnapshot>, live: &[u64]) {
        self.suspended.retain(|id, _| live.contains(id));
        let id = snapshot.map(|dialog| dialog.id);
        let switched = self.active.id != id;
        if switched {
            let previous = std::mem::take(&mut self.active);
            if let Some(id) = previous.id.filter(|id| live.contains(id)) {
                self.suspended.insert(id, previous);
            }
            self.active = id
                .and_then(|id| self.suspended.remove(&id))
                .unwrap_or_default();
        }
        self.active.sync(snapshot);
        if switched {
            self.active.restore_focus();
        }
    }
    pub fn acknowledge(
        &mut self,
        reply: Option<&DialogTextAck>,
        snapshot: Option<&DialogSnapshot>,
    ) -> bool {
        let Some(reply) = reply else {
            return false;
        };
        if self.active.id == Some(reply.id) {
            self.active.acknowledge(Some(reply), snapshot)
        } else {
            self.suspended
                .get_mut(&reply.id)
                .is_some_and(|state| state.acknowledge(Some(reply), None))
        }
    }
}

#[derive(Default)]
pub(super) struct DialogEditors {
    id: Option<u64>,
    active_item: i16,
    focused: Option<i16>,
    control_focus: Option<i16>,
    pub fields: Vec<EditorField>,
    dirty: HashSet<i16>,
    source: HashMap<i16, String>,
    pending: Option<DialogTextReply>,
    serial: u64,
    pub action: Option<i16>,
    pub error: Option<String>,
}
impl DialogEditors {
    pub fn sync(&mut self, snapshot: Option<&DialogSnapshot>) {
        let id = snapshot.map(|s| s.id);
        if self.id != id {
            self.id = id;
            self.active_item = 0;
            self.focused = None;
            self.fields.clear();
            self.source.clear();
            self.dirty.clear();
            self.pending = None;
            self.action = None;
            self.error = None;
        }
        let Some(snapshot) = snapshot else {
            return;
        };
        self.fields.retain(|e| {
            snapshot
                .items
                .iter()
                .any(|i| i.number == e.id && i.kind == 16)
        });
        for item in snapshot.items.iter().filter(|i| i.kind == 16) {
            if !self.dirty.contains(&item.number) {
                self.source.insert(item.number, item.text.clone());
            }
            let selection = item.selection.unwrap_or((0, 0));
            if let Some(editor) = self.fields.iter_mut().find(|e| e.id == item.number) {
                if self.active_item != snapshot.edit_item && item.number == snapshot.edit_item {
                    editor.editor.request_focus = true;
                }
                if !self.dirty.contains(&item.number) {
                    editor.editor.text.clone_from(&item.text);
                    let old = editor.editor.selection;
                    if item.selection.is_some()
                        && (old.0.min(old.1), old.0.max(old.1))
                            != (selection.0 as usize, selection.1 as usize)
                    {
                        editor.editor.selection = (selection.0 as usize, selection.1 as usize);
                    }
                }
            } else {
                self.fields.push(EditorField {
                    id: item.number,
                    editor: Editor {
                        text: item.text.clone(),
                        selection: (selection.0 as usize, selection.1 as usize),
                        request_focus: item.number == snapshot.edit_item,
                        multiline: item.rect.2 as i32 - item.rect.0 as i32 > 30
                            || item.text.contains('\n'),
                    },
                    max_chars: i16::MAX as usize,
                });
            }
        }
        self.active_item = snapshot.edit_item;
        self.dirty
            .retain(|id| self.fields.iter().any(|e| e.id == *id));
    }
    /// No draft or text transaction remains outstanding.
    pub fn synchronized(&self) -> bool {
        self.pending.is_none() && self.dirty.is_empty()
    }
    pub fn focus_control(&mut self, id: i16) {
        self.focused = Some(id);
    }
    pub fn take_control_focus(&mut self) -> Option<i16> {
        self.control_focus.take()
    }
    pub fn restore_focus(&mut self) {
        let focused = self.focused.unwrap_or(self.active_item);
        self.control_focus =
            (!self.fields.iter().any(|field| field.id == focused)).then_some(focused);
        for field in &mut self.fields {
            field.editor.request_focus = field.id == focused;
        }
    }
    pub fn edited(&mut self, id: i16) {
        self.focused = Some(id);
        self.dirty.insert(id);
        self.error = None;
    }
    pub fn acknowledge(
        &mut self,
        reply: Option<&DialogTextAck>,
        snapshot: Option<&DialogSnapshot>,
    ) -> bool {
        let Some(reply) = reply else {
            return false;
        };
        if !self
            .pending
            .as_ref()
            .is_some_and(|p| p.serial == reply.serial && p.id == reply.id)
        {
            return false;
        }
        let pending = self.pending.take().unwrap();
        if reply.accepted && reply.text.is_some() {
            self.source
                .insert(pending.item, reply.text.clone().unwrap());
            if self.fields.iter().any(|e| {
                e.id == pending.item
                    && e.editor.text == pending.text
                    && e.editor.selection
                        == (pending.selection.0 as usize, pending.selection.1 as usize)
            }) {
                self.dirty.remove(&pending.item);
            }
        } else {
            self.dirty.clear();
            self.action = None;
            self.error = Some("The application updated this field. Please try again.".into());
        }
        if snapshot.is_some_and(|snapshot| Some(snapshot.id) == self.id) {
            self.sync(snapshot);
        }
        true
    }
    /// Only one text transaction may be outstanding. Newer local edits stay
    /// dirty until its acknowledgement; actions use the resulting fresh revision.
    pub fn flush(
        &mut self,
        snapshot: &DialogSnapshot,
    ) -> (Option<DialogTextReply>, Option<DialogReply>) {
        if self.pending.is_some() || snapshot.busy {
            return (None, None);
        }
        if let Some(editor) = self.fields.iter().find(|e| self.dirty.contains(&e.id)) {
            self.serial = self.serial.checked_add(1).expect("text sequence exhausted");
            let reply = DialogTextReply {
                id: snapshot.id,
                revision: snapshot.revision,
                serial: self.serial,
                item: editor.id,
                expected: self.source.get(&editor.id).cloned().unwrap_or_default(),
                text: editor.editor.text.clone(),
                selection: (
                    editor.editor.selection.0.min(u16::MAX as usize) as u16,
                    editor.editor.selection.1.min(u16::MAX as usize) as u16,
                ),
            };
            self.pending = Some(reply.clone());
            return (Some(reply), None);
        }
        let action = self.action.take().filter(|id| {
            snapshot
                .items
                .iter()
                .any(|i| i.number == *id && i.enabled && matches!(i.kind, 4..=7 | 17 | 32 | 64))
        });
        (
            None,
            action.map(|item| DialogReply {
                id: snapshot.id,
                revision: snapshot.revision,
                item,
            }),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use systemless::systems::macintosh::host_ui::DialogItemSnapshot;
    fn snapshot() -> DialogSnapshot {
        DialogSnapshot {
            movable: false,
            closable: false,
            id: 1,
            revision: 1,
            title: "Preferences".into(),
            size: (200, 100),
            default_item: 1,
            cancel_item: 1,
            edit_item: 2,
            busy: false,
            items: vec![
                DialogItemSnapshot {
                    range: None,
                    choices: Vec::new(),
                    artwork: None,
                    guest_rect: None,
                    number: 1,
                    kind: 4,
                    enabled: true,
                    rect: (50, 0, 80, 80),
                    text: "Save".into(),
                    value: 0,
                    selection: None,
                },
                DialogItemSnapshot {
                    range: None,
                    choices: Vec::new(),
                    artwork: None,
                    guest_rect: None,
                    number: 2,
                    kind: 16,
                    enabled: true,
                    rect: (0, 0, 20, 100),
                    text: "old".into(),
                    value: 0,
                    selection: Some((0, 3)),
                },
            ],
            help: Vec::new(),
            guest_filter: false,
        }
    }
    #[test]
    fn newer_draft_survives_ack_and_save_waits_for_final_revision() {
        let mut source = snapshot();
        let mut state = DialogEditors::default();
        state.sync(Some(&source));
        state.fields[0].editor.text = "first".into();
        state.edited(2);
        let (first, action) = state.flush(&source);
        let first = first.unwrap();
        assert!(action.is_none());
        state.fields[0].editor.text = "second".into();
        state.action = Some(1);
        assert_eq!(state.flush(&source), (None, None));
        source.revision = 2;
        source.items[1].text = "first".into();
        state.sync(Some(&source));
        assert_eq!(state.fields[0].editor.text, "second");
        assert!(state.acknowledge(
            Some(&DialogTextAck {
                serial: first.serial,
                id: source.id,
                accepted: true,
                text: Some(source.items[1].text.clone())
            }),
            Some(&source)
        ));
        let (second, action) = state.flush(&source);
        let second = second.unwrap();
        assert_eq!(second.text, "second");
        assert_eq!(second.expected, "first");
        assert!(action.is_none());
        source.revision = 3;
        source.items[1].text = "second".into();
        state.sync(Some(&source));
        state.acknowledge(
            Some(&DialogTextAck {
                serial: second.serial,
                id: source.id,
                accepted: true,
                text: Some(source.items[1].text.clone()),
            }),
            Some(&source),
        );
        let (text, action) = state.flush(&source);
        assert!(text.is_none());
        assert_eq!(action.unwrap().revision, 3);
    }
    #[test]
    fn rejection_restores_guest_text_and_cancels_deferred_save() {
        let mut source = snapshot();
        let mut state = DialogEditors::default();
        state.sync(Some(&source));
        state.fields[0].editor.text = "draft".into();
        state.edited(2);
        state.action = Some(1);
        let pending = state.flush(&source).0.unwrap();
        source.revision += 1;
        source.items[1].text = "guest".into();
        state.sync(Some(&source));
        assert!(state.acknowledge(
            Some(&DialogTextAck {
                serial: pending.serial,
                id: 1,
                accepted: false,
                text: None
            }),
            Some(&source)
        ));
        assert_eq!(state.fields[0].editor.text, "guest");
        assert!(state.error.is_some());
        assert_eq!(state.flush(&source), (None, None));
        assert!(!state.acknowledge(
            Some(&DialogTextAck {
                serial: pending.serial,
                id: 1,
                accepted: false,
                text: None
            }),
            Some(&source)
        ));
    }
    #[test]
    fn replacement_dialog_discards_pending_action_and_old_ack() {
        let mut source = snapshot();
        let mut state = DialogEditors::default();
        state.sync(Some(&source));
        state.edited(2);
        state.action = Some(1);
        let pending = state.flush(&source).0.unwrap();
        source.id = 2;
        state.sync(Some(&source));
        assert!(!state.acknowledge(
            Some(&DialogTextAck {
                serial: pending.serial,
                id: 1,
                accepted: true,
                text: Some(source.items[1].text.clone())
            }),
            Some(&source)
        ));
        assert_eq!(state.flush(&source), (None, None));
    }

    #[test]
    fn parent_draft_selection_and_focus_survive_children_and_guest_fallback() {
        let parent = snapshot();
        let mut child = snapshot();
        child.id = 2;
        let mut sessions = DialogEditorSessions::default();
        sessions.sync(Some(&parent), &[1]);
        sessions.active.fields[0].editor.text = "unsent parent".into();
        sessions.active.fields[0].editor.selection = (8, 2);
        sessions.active.edited(2);
        sessions.sync(Some(&child), &[1, 2]);
        assert_eq!(sessions.active.fields[0].editor.text, "old");
        sessions.active.fields[0].editor.text = "child".into();
        sessions.active.edited(2);
        sessions.sync(None, &[1, 2]);
        assert!(sessions.active.fields.is_empty());
        sessions.sync(Some(&parent), &[1, 2]);
        let field = &sessions.active.fields[0];
        assert_eq!(field.editor.text, "unsent parent");
        assert_eq!(field.editor.selection, (8, 2));
        assert!(field.editor.request_focus);
        let text = sessions.active.flush(&parent).0.unwrap();
        assert_eq!(text.expected, "old");
        assert_eq!(text.text, "unsent parent");
        sessions.sync(Some(&child), &[1, 2]);
        assert_eq!(sessions.active.fields[0].editor.text, "child");
    }

    #[test]
    fn obscured_parent_ack_uses_canonical_text_without_rebasing_over_guest_changes() {
        let mut parent = snapshot();
        let mut child = snapshot();
        child.id = 2;
        let mut sessions = DialogEditorSessions::default();
        sessions.sync(Some(&parent), &[1]);
        sessions.active.fields[0].editor.text = "first\tvalue".into();
        sessions.active.edited(2);
        let first = sessions.active.flush(&parent).0.unwrap();
        sessions.active.fields[0].editor.text = "newer draft".into();
        sessions.active.action = Some(1);
        sessions.sync(Some(&child), &[1, 2]);
        sessions.active.edited(2);
        let child_pending = sessions.active.flush(&child).0.unwrap();
        assert_eq!(first.serial, child_pending.serial);
        let ack = DialogTextAck {
            serial: first.serial,
            id: 1,
            accepted: true,
            text: Some("first value".into()),
        };
        assert!(sessions.acknowledge(Some(&ack), Some(&child)));
        assert!(!sessions.acknowledge(Some(&ack), Some(&child)));
        assert!(
            sessions.active.pending.is_some(),
            "parent ACK cannot release child edit"
        );
        parent.revision = 3;
        parent.items[1].text = "changed by guest while child was open".into();
        sessions.sync(Some(&parent), &[1, 2]);
        assert_eq!(sessions.active.fields[0].editor.text, "newer draft");
        let (next, action) = sessions.active.flush(&parent);
        let next = next.unwrap();
        assert_eq!(
            next.expected, "first value",
            "runtime must reject a changed guest source"
        );
        assert_eq!(next.text, "newer draft");
        assert!(action.is_none(), "Save still waits for the newer edit");
        sessions.acknowledge(
            Some(&DialogTextAck {
                serial: next.serial,
                id: 1,
                accepted: false,
                text: None,
            }),
            Some(&parent),
        );
        assert_eq!(sessions.active.fields[0].editor.text, parent.items[1].text);
        assert!(sessions.active.error.is_some());
        assert!(sessions.active.action.is_none());
    }

    #[test]
    fn disposed_parent_sessions_drop_drafts_pending_actions_and_late_acknowledgements() {
        let parent = snapshot();
        let mut child = snapshot();
        child.id = 2;
        let mut sessions = DialogEditorSessions::default();
        sessions.sync(Some(&parent), &[1]);
        sessions.active.fields[0].editor.text = "retired draft".into();
        sessions.active.edited(2);
        sessions.active.action = Some(1);
        let pending = sessions.active.flush(&parent).0.unwrap();
        sessions.sync(Some(&child), &[1, 2]);
        sessions.sync(Some(&child), &[2]);
        assert!(sessions.suspended.is_empty());
        assert!(!sessions.acknowledge(
            Some(&DialogTextAck {
                serial: pending.serial,
                id: 1,
                accepted: true,
                text: Some(pending.text),
            }),
            Some(&child)
        ));
        let mut replacement = parent;
        replacement.id = 3;
        sessions.sync(Some(&replacement), &[2, 3]);
        assert_eq!(sessions.active.fields[0].editor.text, "old");
        assert!(sessions.active.pending.is_none());
        assert!(sessions.active.action.is_none());
    }
    #[test]
    fn popup_and_picture_actions_use_the_current_ready_revision() {
        for kind in [7, 17, 64] {
            let mut source = snapshot();
            source.items[0].kind = kind;
            let mut state = DialogEditors::default();
            state.sync(Some(&source));
            state.action = Some(1);
            let (text, action) = state.flush(&source);
            assert!(text.is_none());
            assert_eq!(
                action,
                Some(DialogReply {
                    id: source.id,
                    revision: source.revision,
                    item: 1
                })
            );
            source.items[0].enabled = false;
            state.action = Some(1);
            assert_eq!(state.flush(&source), (None, None));
        }
    }
}
