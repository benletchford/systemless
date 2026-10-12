//! Minimal native accessibility probe for the pilot dialog.
//! Nodes keep guest lifetime identity; actions still pass through worker guards.
use super::{comparison_text_owner, Command, Update};
use accesskit::{Action, ActionData, ActionRequest, Node, NodeId, Role, Tree, TreeId, TreeUpdate};
use std::collections::BTreeMap;
#[derive(Default)]
pub(super) struct Accessibility {
    ids: BTreeMap<(u32, u64, u64, i16), NodeId>,
    current: BTreeMap<NodeId, (u32, u64, u64, i16)>,
    next: u64,
}
impl Accessibility {
    pub fn tree(&mut self, update: Option<&Update>, viewport: (f32, f32, f32, f32)) -> TreeUpdate {
        let mut root = Node::new(Role::Window);
        root.set_label("Systemless wgpu comparison");
        let mut nodes = Vec::new();
        let mut children = Vec::new();
        let mut focus = NodeId(0);
        self.current.clear();
        if let Some(update) = update {
            let (width, height) = update
                .frame
                .as_ref()
                .map(|(w, h, _)| (*w as f64, *h as f64))
                .unwrap_or((640., 480.));
            for dialog in update.dialogs.iter().filter(|d| d.visible && d.active) {
                for item in dialog.items.iter().filter(|i| i.visible) {
                    use systemless::runner::DialogItemKind::*;
                    let role = match item.kind {
                        Button => Role::Button,
                        Checkbox => Role::CheckBox,
                        EditText => Role::TextInput,
                        StaticText => Role::Label,
                        _ => continue,
                    };
                    let key = (
                        dialog.guest_id,
                        dialog.generation,
                        dialog.content_revision,
                        item.number,
                    );
                    let id = *self.ids.entry(key).or_insert_with(|| {
                        self.next += 1;
                        NodeId(self.next)
                    });
                    self.current.insert(id, key);
                    let mut node = Node::new(role);
                    node.set_label(item.text.clone());
                    let r = item.bounds;
                    node.set_bounds(accesskit::Rect::new(
                        viewport.0 as f64 + r.1 as f64 * viewport.2 as f64 / width,
                        viewport.1 as f64 + r.0 as f64 * viewport.3 as f64 / height,
                        viewport.0 as f64 + r.3 as f64 * viewport.2 as f64 / width,
                        viewport.1 as f64 + r.2 as f64 * viewport.3 as f64 / height,
                    ));
                    if !item.enabled {
                        node.set_disabled();
                    }
                    match item.kind {
                        Button | Checkbox if item.enabled => node.add_action(Action::Click),
                        EditText => {
                            node.set_value(item.text.clone());
                            if item.enabled && dialog.edit_field == Some(item.number) {
                                node.add_action(Action::SetValue);
                                focus = id;
                            }
                        }
                        _ => {}
                    }
                    if item.kind == Checkbox {
                        node.set_toggled(if item.value.unwrap_or(0) != 0 {
                            accesskit::Toggled::True
                        } else {
                            accesskit::Toggled::False
                        });
                    }
                    nodes.push((id, node));
                    children.push(id);
                }
            }
        }
        root.set_children(children);
        nodes.push((NodeId(0), root));
        TreeUpdate {
            nodes,
            tree: Some(Tree::new(NodeId(0))),
            tree_id: TreeId::ROOT,
            focus,
        }
    }
    pub fn action(&self, request: ActionRequest, update: &Update) -> Option<Command> {
        let &(id, generation, revision, number) = self.current.get(&request.target_node)?;
        let dialog = update.dialogs.iter().find(|d| {
            d.visible
                && d.active
                && (d.guest_id, d.generation, d.content_revision) == (id, generation, revision)
        })?;
        let item = dialog
            .items
            .iter()
            .find(|i| i.number == number && i.visible && i.enabled)?;
        match request.action {
            Action::Click
                if matches!(
                    item.kind,
                    systemless::runner::DialogItemKind::Button
                        | systemless::runner::DialogItemKind::Checkbox
                ) =>
            {
                Some(Command::ActivateDialog(
                    id,
                    generation,
                    number,
                    item.control_identity,
                ))
            }
            Action::SetValue => {
                let ActionData::Value(value) = request.data? else {
                    return None;
                };
                let owner = comparison_text_owner(update)?;
                if owner.identity != (id, generation) || dialog.edit_field != Some(number) {
                    return None;
                }
                let (owner, range, bytes) =
                    crate::input::accessibility_text_replacement(owner, &value, false)?;
                Some(Command::ReplaceText(owner, range, bytes, None))
            }
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn accessible_actions_cannot_target_closed_or_recycled_dialogs() {
        use systemless::runner::{DialogItemKind, DialogItemSnapshot, DialogSnapshot};
        let item = DialogItemSnapshot {
            control_identity: Some((200, 4)),
            static_text_layout: None,
            edit_text_layout: None,
            pressed: false,
            number: 1,
            kind: DialogItemKind::Button,
            bounds: (10, 10, 30, 80),
            text: "OK".into(),
            enabled: true,
            visible: true,
            value: Some(0),
            selection: None,
            caret_visible: None,
        };
        let mut update = Update {
            dialogs: vec![DialogSnapshot {
                content_revision: 1,
                guest_id: 100,
                generation: 3,
                bounds: (0, 0, 100, 100),
                visible: true,
                active: true,
                default_item: Some(1),
                cancel_item: None,
                edit_field: None,
                items: vec![item],
            }],
            ..Default::default()
        };
        let mut accessibility = Accessibility::default();
        let tree = accessibility.tree(Some(&update), (0., 0., 640., 480.));
        let id = tree
            .nodes
            .iter()
            .find(|(_, node)| node.role() == Role::Button)
            .unwrap()
            .0;
        let request = || ActionRequest {
            action: Action::Click,
            target_tree: TreeId::ROOT,
            target_node: id,
            data: None,
        };
        assert!(matches!(
            accessibility.action(request(), &update),
            Some(Command::ActivateDialog(100, 3, 1, Some((200, 4))))
        ));
        update.dialogs[0].generation = 4;
        assert!(accessibility.action(request(), &update).is_none());
        accessibility.tree(Some(&update), (0., 0., 640., 480.));
        assert!(accessibility.action(request(), &update).is_none());
        update.dialogs.clear();
        assert!(accessibility.action(request(), &update).is_none());
    }
}
