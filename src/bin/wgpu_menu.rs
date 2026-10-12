//! One interaction path for live and offscreen standard menus.
use crate::runtime_protocol::GuiCommand;
use systemless::menu_model::ComparisonMenu;
#[derive(Default)]
pub struct MenuPresenter {
    pub open: Option<ComparisonMenu>,
    pub selected: i16,
    captured_press: bool,
    captured_keys: std::collections::BTreeSet<u8>,
}
fn contains(r: (i16, i16, i16, i16), p: (i16, i16)) -> bool {
    p.0 >= r.0 && p.0 < r.2 && p.1 >= r.1 && p.1 < r.3
}
impl MenuPresenter {
    pub fn reconcile(&mut self, menus: &[ComparisonMenu], dialog: bool) {
        if dialog || self.open.as_ref().is_some_and(|m| !menus.contains(m)) {
            self.open = None;
            self.selected = 0;
        }
    }
    fn move_to(&mut self, p: (i16, i16), menus: &[ComparisonMenu]) {
        if let Some(m) = menus
            .iter()
            .find(|m| m.menu.enabled && contains(m.title_bounds, p))
        {
            self.open = Some(m.clone());
            self.selected = 0;
            return;
        }
        self.selected = 0;
        if let Some(m) = &self.open {
            if contains(m.bounds, p) {
                for (i, r) in m.menu.items.iter().zip(&m.rows) {
                    if p.0 >= r.top
                        && p.0 < r.top + r.height
                        && i.enabled
                        && !i.separator
                        && i.submenu_id.is_none()
                    {
                        self.selected = i.number;
                        break;
                    }
                }
            }
        }
    }
    fn choose(&mut self) -> Option<GuiCommand> {
        let menu = self.open.take()?;
        let item = self.selected;
        self.selected = 0;
        menu.menu
            .items
            .iter()
            .any(|i| i.number == item && i.enabled && !i.separator && i.submenu_id.is_none())
            .then_some(GuiCommand::ComparisonMenu {
                expected: menu,
                item,
            })
    }
    pub fn route(
        &mut self,
        command: GuiCommand,
        menus: &[ComparisonMenu],
        dialog: bool,
    ) -> Option<GuiCommand> {
        self.reconcile(menus, dialog);
        match command {
            GuiCommand::MouseDown { v, h } => {
                if !dialog {
                    if let Some(m) = menus.iter().find(|m| contains(m.title_bounds, (v, h))) {
                        self.captured_press = true;
                        if m.menu.enabled {
                            self.open = Some(m.clone());
                            self.selected = 0;
                        }
                        return None;
                    }
                }
                if self.open.is_some() {
                    self.captured_press = true;
                    self.move_to((v, h), menus);
                    return None;
                }
            }
            GuiCommand::MouseMove { v, h } if self.open.is_some() => {
                self.move_to((v, h), menus);
                return None;
            }
            GuiCommand::MouseUp { v, h } if self.open.is_some() || self.captured_press => {
                self.captured_press = false;
                if self.open.is_some() && menus.iter().any(|m| contains(m.title_bounds, (v, h))) {
                    return None;
                }
                self.move_to((v, h), menus);
                return self.choose();
            }
            GuiCommand::KeyUp { key, .. } if self.captured_keys.remove(&key) => return None,
            GuiCommand::KeyDown { key, .. } if self.open.is_some() => {
                self.captured_keys.insert(key);
                match key {
                    53 => {
                        self.open = None;
                        self.selected = 0;
                    }
                    36 | 76 => return self.choose(),
                    125 | 126 => {
                        let m = self.open.as_ref().unwrap();
                        let items: Vec<_> = m
                            .menu
                            .items
                            .iter()
                            .filter(|i| i.enabled && !i.separator && i.submenu_id.is_none())
                            .map(|i| i.number)
                            .collect();
                        if !items.is_empty() {
                            let at = items.iter().position(|n| *n == self.selected);
                            let index = if key == 125 {
                                at.map_or(0, |i| (i + 1) % items.len())
                            } else {
                                at.map_or(items.len() - 1, |i| (i + items.len() - 1) % items.len())
                            };
                            self.selected = items[index];
                        }
                    }
                    _ => {}
                }
                return None;
            }
            _ => {}
        }
        Some(command)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use systemless::menu_model::{ComparisonMenuRow, GuestMenu, GuestMenuItem};
    fn fixture() -> ComparisonMenu {
        ComparisonMenu {
            handle: 100,
            pointer: 200,
            menu: GuestMenu {
                id: 128,
                title: "File".into(),
                enabled: true,
                hierarchical: false,
                visible_in_menu_bar: true,
                items: vec![GuestMenuItem {
                    number: 1,
                    text: "Quit".into(),
                    enabled: true,
                    checked: false,
                    key_equivalent: Some('q'),
                    submenu_id: None,
                    separator: false,
                }],
            },
            title_bounds: (0, 35, 20, 66),
            title_origin: (42, 14),
            bounds: (20, 35, 36, 125),
            rows: vec![ComparisonMenuRow {
                top: 20,
                height: 16,
                text_origin: (50, 32),
                command_left: 100,
                mark_left: 38,
                separator_y: 27,
            }],
        }
    }
    #[test]
    fn click_delivers_guarded_menu_command_and_consumes_guest_mouse_transitions() {
        let m = fixture();
        let mut state = MenuPresenter::default();
        assert!(state
            .route(GuiCommand::MouseDown { v: 8, h: 50 }, &[m.clone()], false)
            .is_none());
        assert!(state
            .route(GuiCommand::MouseUp { v: 8, h: 50 }, &[m.clone()], false)
            .is_none());
        assert!(state
            .route(GuiCommand::MouseMove { v: 27, h: 60 }, &[m.clone()], false)
            .is_none());
        assert_eq!(
            state.route(GuiCommand::MouseUp { v: 27, h: 60 }, &[m.clone()], false),
            Some(GuiCommand::ComparisonMenu {
                expected: m,
                item: 1
            })
        );
    }
    #[test]
    fn changed_guest_menu_cancels_tracking_before_selection() {
        let m = fixture();
        let mut state = MenuPresenter::default();
        state.route(GuiCommand::MouseDown { v: 8, h: 50 }, &[m.clone()], false);
        let mut changed = m;
        changed.pointer = 300;
        state.reconcile(&[changed.clone()], false);
        assert!(state.open.is_none());
        assert!(state
            .route(GuiCommand::MouseUp { v: 27, h: 60 }, &[changed], false)
            .is_none());
    }
    #[test]
    fn keyboard_escape_cancels_without_leaking_key_release() {
        let m = fixture();
        let mut state = MenuPresenter::default();
        state.route(GuiCommand::MouseDown { v: 8, h: 50 }, &[m.clone()], false);
        assert!(state
            .route(
                GuiCommand::KeyDown {
                    key: 53,
                    character: 27
                },
                &[m.clone()],
                false
            )
            .is_none());
        assert!(state.open.is_none());
        assert!(state
            .route(
                GuiCommand::KeyUp {
                    key: 53,
                    character: 27
                },
                &[m],
                false
            )
            .is_none());
    }
}
