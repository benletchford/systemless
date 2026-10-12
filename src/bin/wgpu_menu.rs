//! Retained standard-menu interaction; commands are validated again by the guest worker.
use super::{Command, Update};
#[derive(Default)]
pub(super) struct MenuPresenter {
    pub open: Option<(u32, u64)>,
    pub selected: i16,
    consumed_press: bool,
}
impl MenuPresenter {
    pub fn reconcile(&mut self, update: &Update) {
        if self.open.is_some_and(|id| {
            !update.menu_presentations.iter().any(|p| {
                (p.pane.menu.guest_id, p.pane.menu.generation) == id && p.pane.menu.enabled
            })
        }) {
            self.open = None;
            self.selected = 0;
        }
    }
    pub fn move_pointer(&mut self, position: (i16, i16), update: &Update) -> bool {
        let Some(id) = self.open else {
            return false;
        };
        if let Some(p) = update
            .menu_presentations
            .iter()
            .find(|p| contains(p.title_bounds, position) && p.pane.menu.enabled)
        {
            self.open = Some((p.pane.menu.guest_id, p.pane.menu.generation));
            self.selected = 0;
            return true;
        }
        self.selected = 0;
        if let Some(p) = update
            .menu_presentations
            .iter()
            .find(|p| (p.pane.menu.guest_id, p.pane.menu.generation) == id)
        {
            if contains(p.pane.bounds, position) {
                let mut top = p.pane.content_top;
                for (item, height) in p.pane.menu.items.iter().zip(&p.pane.row_heights) {
                    if position.0 >= top
                        && position.0 < top.saturating_add(*height)
                        && item.enabled
                        && !item.separator
                        && item.submenu_id.is_none()
                    {
                        self.selected = item.number;
                        break;
                    }
                    top = top.saturating_add(*height);
                }
            }
        }
        true
    }
    pub fn button(
        &mut self,
        down: bool,
        position: (i16, i16),
        update: &Update,
    ) -> (bool, Option<Command>) {
        self.reconcile(update);
        if down {
            if let Some(p) = update
                .menu_presentations
                .iter()
                .find(|p| contains(p.title_bounds, position))
            {
                self.consumed_press = true;
                if p.pane.menu.enabled {
                    self.open = Some((p.pane.menu.guest_id, p.pane.menu.generation));
                    self.selected = 0;
                }
                return (true, None);
            }
            if self.open.is_some() {
                self.consumed_press = true;
                self.move_pointer(position, update);
                return (true, None);
            }
            return (false, None);
        }
        if self.open.is_some() || self.consumed_press {
            self.consumed_press = false;
            if update
                .menu_presentations
                .iter()
                .any(|p| contains(p.title_bounds, position))
            {
                return (true, None);
            }
            self.move_pointer(position, update);
            let command = self.command(update);
            self.open = None;
            self.selected = 0;
            return (true, command);
        }
        (false, None)
    }
    pub fn command(&self, update: &Update) -> Option<Command> {
        let (id, generation) = self.open?;
        let p = update
            .menu_presentations
            .iter()
            .find(|p| (p.pane.menu.guest_id, p.pane.menu.generation) == (id, generation))?;
        update
            .menus
            .selectable_result_for_guest(p.pane.menu.id, self.selected, id, generation)?;
        Some(Command::Menu(p.pane.menu.id, self.selected, id, generation))
    }
    pub fn key(&mut self, key: &winit::keyboard::Key, update: &Update) -> Option<Command> {
        use winit::keyboard::{Key, NamedKey};
        let id = self.open?;
        match key {
            Key::Named(NamedKey::Escape) => {
                self.open = None;
                self.selected = 0;
            }
            Key::Named(NamedKey::Enter) => {
                let command = self.command(update);
                self.open = None;
                self.selected = 0;
                return command;
            }
            Key::Named(direction @ (NamedKey::ArrowDown | NamedKey::ArrowUp)) => {
                let p = update
                    .menu_presentations
                    .iter()
                    .find(|p| (p.pane.menu.guest_id, p.pane.menu.generation) == id)?;
                let items: Vec<_> = p
                    .pane
                    .menu
                    .items
                    .iter()
                    .filter(|i| i.enabled && !i.separator && i.submenu_id.is_none())
                    .map(|i| i.number)
                    .collect();
                if !items.is_empty() {
                    let at = items.iter().position(|n| *n == self.selected);
                    let index = if *direction == NamedKey::ArrowDown {
                        at.map_or(0, |i| (i + 1) % items.len())
                    } else {
                        at.map_or(items.len() - 1, |i| (i + items.len() - 1) % items.len())
                    };
                    self.selected = items[index];
                }
            }
            _ => {}
        }
        None
    }
}
fn contains(r: (i16, i16, i16, i16), p: (i16, i16)) -> bool {
    p.0 >= r.0 && p.0 < r.2 && p.1 >= r.1 && p.1 < r.3
}

#[cfg(test)]
mod tests {
    use super::*;
    use systemless::menu_model::*;
    fn fixture() -> Update {
        let menu = GuestMenu {
            guest_id: 100,
            generation: 3,
            id: 128,
            title: "File".into(),
            enabled: true,
            standard_definition: true,
            hierarchical: false,
            visible_in_menu_bar: true,
            items: vec![GuestMenuItem {
                mark: 0,
                style: 0,
                number: 1,
                text: "Open".into(),
                enabled: true,
                checked: false,
                key_equivalent: None,
                submenu_id: None,
                separator: false,
            }],
        };
        Update {
            menus: GuestMenuSnapshot {
                menus: vec![menu.clone()],
                custom_bar_definition: false,
            },
            menu_presentations: vec![GuestMenuPresentation {
                title_bounds: (0, 40, 20, 80),
                title_origin: (47, 14),
                pane: GuestPopupSnapshot {
                    menu,
                    font: Default::default(),
                    bounds: (20, 40, 36, 140),
                    content_top: 20,
                    row_heights: vec![16],
                    highlighted_item: 0,
                },
            }],
            ..Default::default()
        }
    }
    #[test]
    fn menu_click_uses_live_identity_and_rejects_recycled_handle() {
        let update = fixture();
        let mut state = MenuPresenter::default();
        assert!(state.button(true, (8, 50), &update).0);
        assert!(state.button(false, (8, 50), &update).1.is_none());
        state.move_pointer((25, 60), &update);
        assert!(matches!(
            state.command(&update),
            Some(Command::Menu(128, 1, 100, 3))
        ));
        let mut recycled = fixture();
        recycled.menus.menus[0].generation = 4;
        assert!(state.command(&recycled).is_none());
        recycled.menu_presentations[0].pane.menu.generation = 4;
        state.reconcile(&recycled);
        assert!(state.open.is_none());
    }
    #[test]
    fn outside_release_cancels_without_leaking_guest_mouse_up() {
        let update = fixture();
        let mut state = MenuPresenter::default();
        state.button(true, (8, 50), &update);
        let (handled, command) = state.button(false, (90, 200), &update);
        assert!(handled && command.is_none() && state.open.is_none());
        assert!(!state.button(true, (90, 200), &update).0);
    }
}
