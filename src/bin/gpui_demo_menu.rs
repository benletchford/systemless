// Retained presentation state for standard guest menus. GPUI Kit's PopupMenu
// clears its private selected row on rebuild, so selection lives alongside
// guest item identities here and survives live Menu Manager updates.

use gpui_kit::base::actions::{Cancel, Confirm, SelectDown, SelectLeft, SelectRight, SelectUp};
use gpui_kit::component::scroll::ScrollableElement;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct Selection {
    menu_id: i16,
    guest_id: u32,
    generation: u64,
    item_number: i16,
}

struct GuestMenuPopup {
    root_id: i16,
    snapshot: GuestMenuSnapshot,
    selection: Vec<Selection>,
    active_depth: usize,
    scroll_handles: HashMap<(u32, u64), ScrollHandle>,
    focus: FocusHandle,
    commands: mpsc::Sender<Command>,
}

impl GuestMenuPopup {
    fn new(
        root_id: i16,
        snapshot: GuestMenuSnapshot,
        commands: mpsc::Sender<Command>,
        cx: &mut Context<Self>,
    ) -> Self {
        Self {
            root_id,
            snapshot,
            selection: Vec::new(),
            active_depth: 0,
            scroll_handles: HashMap::new(),
            focus: cx.focus_handle(),
            commands,
        }
    }

    fn update_snapshot(
        &mut self,
        root_id: i16,
        snapshot: GuestMenuSnapshot,
        cx: &mut Context<Self>,
    ) {
        self.root_id = root_id;
        self.snapshot = snapshot;
        self.scroll_handles.retain(|&(guest_id, generation), _| {
            self.snapshot
                .menus
                .iter()
                .any(|menu| menu.guest_id == guest_id && menu.generation == generation)
        });
        self.reconcile();
        cx.notify();
    }

    fn menu(&self, id: i16) -> Option<&GuestMenu> {
        self.snapshot.menus.iter().find(|menu| menu.id == id)
    }

    fn selectable(&self, menu: &GuestMenu, item: &systemless::menu_model::GuestMenuItem) -> bool {
        menu.enabled
            && item.enabled
            && !item.separator
            && item
                .submenu_id
                .is_none_or(|id| self.menu(id).is_some_and(|submenu| submenu.enabled))
    }

    fn reconcile(&mut self) {
        let mut menu_id = self.root_id;
        let mut valid = 0;
        for selected in &self.selection {
            let Some(menu) = self.menu(menu_id) else {
                break;
            };
            if selected.menu_id != menu_id
                || selected.guest_id != menu.guest_id
                || selected.generation != menu.generation
            {
                break;
            }
            let Some(item) = menu
                .items
                .iter()
                .find(|item| item.number == selected.item_number)
            else {
                break;
            };
            if !self.selectable(menu, item) {
                break;
            }
            valid += 1;
            let Some(next) = item.submenu_id else { break };
            if !self.menu(next).is_some_and(|submenu| submenu.enabled) {
                break;
            }
            menu_id = next;
        }
        self.selection.truncate(valid);
        self.active_depth = self
            .active_depth
            .min(self.selection.len().saturating_sub(1));
    }

    fn select(
        &mut self,
        depth: usize,
        menu_id: i16,
        item_number: i16,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(menu) = self.menu(menu_id) else {
            return false;
        };
        let Some(item) = menu.items.iter().find(|item| item.number == item_number) else {
            return false;
        };
        if !self.selectable(menu, item) {
            return false;
        }
        let selected = Selection {
            menu_id,
            guest_id: menu.guest_id,
            generation: menu.generation,
            item_number,
        };
        let index = menu
            .items
            .iter()
            .position(|item| item.number == item_number);
        self.selection.truncate(depth);
        self.selection.push(selected);
        self.active_depth = depth;
        if let Some(index) = index {
            self.scroll_handles
                .entry((selected.guest_id, selected.generation))
                .or_default()
                .scroll_to_item(index);
        }
        cx.notify();
        true
    }

    fn active_menu_id(&self) -> i16 {
        if self.active_depth == 0 {
            return self.root_id;
        }
        self.selection
            .get(self.active_depth - 1)
            .and_then(|selected| {
                self.menu(selected.menu_id)?
                    .items
                    .iter()
                    .find(|item| item.number == selected.item_number)?
                    .submenu_id
                    .filter(|id| self.menu(*id).is_some_and(|menu| menu.enabled))
            })
            .unwrap_or(self.root_id)
    }

    fn move_selection(&mut self, down: bool, cx: &mut Context<Self>) {
        let menu_id = self.active_menu_id();
        let Some(menu) = self.menu(menu_id) else {
            return;
        };
        let items: Vec<i16> = menu
            .items
            .iter()
            .filter(|item| self.selectable(menu, item))
            .map(|item| item.number)
            .collect();
        if items.is_empty() {
            return;
        }
        let current = self.selection.get(self.active_depth).and_then(|selected| {
            items
                .iter()
                .position(|number| *number == selected.item_number)
        });
        let index = match current {
            Some(index) if down => (index + 1) % items.len(),
            Some(0) if !down => items.len() - 1,
            Some(index) => index - 1,
            None if down => 0,
            None => items.len() - 1,
        };
        self.select(self.active_depth, menu_id, items[index], cx);
    }

    fn activate(&mut self, allow_leaf: bool, cx: &mut Context<Self>) {
        let Some(selected) = self.selection.get(self.active_depth).copied() else {
            return;
        };
        let Some(menu) = self.menu(selected.menu_id) else {
            return;
        };
        let Some(item) = menu
            .items
            .iter()
            .find(|item| item.number == selected.item_number)
        else {
            return;
        };
        if !self.selectable(menu, item) {
            return;
        }
        if let Some(submenu_id) = item.submenu_id {
            if !self.selection[..=self.active_depth]
                .iter()
                .any(|selected| selected.menu_id == submenu_id)
                && self.menu(submenu_id).is_some_and(|submenu| submenu.enabled)
            {
                self.active_depth += 1;
                self.move_selection(true, cx);
            }
            return;
        }
        if !allow_leaf {
            return;
        }
        let _ = self.commands.send(Command::Menu(
            selected.menu_id,
            selected.item_number,
            selected.guest_id,
            selected.generation,
        ));
        cx.emit(DismissEvent);
    }
}

impl EventEmitter<DismissEvent> for GuestMenuPopup {}

impl Focusable for GuestMenuPopup {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus.clone()
    }
}

impl Render for GuestMenuPopup {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let mut panel = div()
            .id("guest-popup-menu")
            .test_support()
            .flex()
            .relative()
            .occlude()
            .role(Role::Menu)
            .key_context("PopupMenu")
            .track_focus(&self.focus)
            .bg(cx.theme().background)
            .border_1()
            .border_color(cx.theme().border)
            .on_action(cx.listener(|this, _: &SelectDown, _, cx| {
                this.move_selection(true, cx);
                cx.stop_propagation();
            }))
            .on_action(cx.listener(|this, _: &SelectUp, _, cx| {
                this.move_selection(false, cx);
                cx.stop_propagation();
            }))
            .on_action(cx.listener(|this, _: &SelectRight, _, cx| {
                this.activate(false, cx);
                cx.stop_propagation();
            }))
            .on_action(cx.listener(|this, _: &SelectLeft, _, cx| {
                if this.active_depth > 0 {
                    this.active_depth -= 1;
                    this.selection.truncate(this.active_depth + 1);
                    cx.notify();
                }
                cx.stop_propagation();
            }))
            .on_action(cx.listener(|this, _: &Confirm, _, cx| {
                this.activate(true, cx);
                cx.stop_propagation();
            }))
            .on_action(cx.listener(|_, _: &Cancel, _, cx| {
                cx.emit(DismissEvent);
                cx.stop_propagation();
            }));
        let mut menu_id = self.root_id;
        let mut visited = Vec::new();
        for depth in 0..self.snapshot.menus.len() {
            if visited.contains(&menu_id) {
                break;
            }
            visited.push(menu_id);
            let Some(menu) = self.snapshot.menus.iter().find(|menu| menu.id == menu_id) else {
                break;
            };
            let scroll = self
                .scroll_handles
                .entry((menu.guest_id, menu.generation))
                .or_default()
                .clone();
            let mut column = div()
                .id(format!(
                    "guest-menu-column-{}-{}",
                    menu.guest_id, menu.generation
                ))
                .flex()
                .flex_col()
                .min_w(px(180.))
                .max_h(px(420.))
                .overflow_y_scroll()
                .track_scroll(&scroll)
                .when(depth > 0, |column| {
                    column.border_l_1().border_color(cx.theme().border)
                })
                .p(px(4.));
            for item in &menu.items {
                if item.separator {
                    column = column.child(div().h(px(1.)).my(px(3.)).bg(cx.theme().border));
                    continue;
                }
                let selected = self.selection.get(depth).is_some_and(|selection| {
                    selection.menu_id == menu_id && selection.item_number == item.number
                });
                let enabled = self.selectable(menu, item);
                let item_number = item.number;
                let row_menu_id = menu_id;
                let label = match item.key_equivalent {
                    Some(key) => format!("{}    ⌘{}", item.text, key.to_uppercase()),
                    None => item.text.clone(),
                };
                column = column.child(
                    div()
                        .id(format!("guest-popup-item-{menu_id}-{item_number}"))
                        .test_support()
                        .role(Role::MenuItem)
                        .aria_label(label.clone())
                        .aria_selected(selected)
                        .flex()
                        .items_center()
                        .h(px(26.))
                        .px(px(8.))
                        .when(selected, |row| row.bg(cx.theme().selection))
                        .when(!enabled, |row| row.text_color(cx.theme().muted_foreground))
                        .when(enabled, |row| {
                            row.on_hover(cx.listener(move |this, hovered: &bool, _, cx| {
                                if *hovered {
                                    this.select(depth, row_menu_id, item_number, cx);
                                }
                            }))
                            .on_mouse_down(MouseButton::Left, |_, _, cx| {
                                cx.stop_propagation();
                            })
                            .on_click(cx.listener(
                                move |this, _, _, cx| {
                                    if this.select(depth, row_menu_id, item_number, cx) {
                                        this.activate(true, cx);
                                    }
                                },
                            ))
                        })
                        .child(if item.checked { "✓ " } else { "  " })
                        .child(label)
                        .when(item.submenu_id.is_some(), |row| row.child("  ›")),
                );
            }
            panel = panel.child(column.vertical_scrollbar(&scroll));
            let Some(next) = self.selection.get(depth).and_then(|selected| {
                menu.items
                    .iter()
                    .find(|item| item.number == selected.item_number)?
                    .submenu_id
            }) else {
                break;
            };
            if !self.menu(next).is_some_and(|submenu| submenu.enabled) {
                break;
            }
            menu_id = next;
        }
        panel
    }
}
