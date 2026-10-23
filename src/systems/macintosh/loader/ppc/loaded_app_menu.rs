//! PowerPC loaded application guest menu snapshot and native menu selection staging
//! methods on [`PpcLoadedApp`].

use super::*;

impl PpcLoadedApp {
    /// Copy the live PowerPC Menu Manager list into the same frontend-neutral
    /// model used by the 68k Toolbox implementation.
    pub fn guest_menu_snapshot(&mut self) -> GuestMenuSnapshot {
        let menu_list = ppc_current_menu_list(&mut self.memory);
        ppc_guest_menu_snapshot(&mut self.memory, menu_list)
    }

    pub(crate) fn host_popup_menu(
        &self,
    ) -> Option<crate::systems::macintosh::host_ui::PopupMenuSnapshot> {
        self.toolbox_startup.host_ui.popup_snapshot()
    }

    /// Accept a host popup choice validated against the live menu list.
    pub(crate) fn reply_host_popup_menu(
        &mut self,
        reply: crate::systems::macintosh::host_ui::PopupMenuReply,
    ) -> bool {
        let Some(root) = self.host_popup_menu().map(|popup| popup.menu) else {
            return false;
        };
        let menus = self.guest_menu_snapshot();
        self.toolbox_startup
            .host_ui
            .reply_popup(reply, |menu, item| {
                crate::systems::macintosh::host_ui::popup_choice_result(&menus, root, menu, item)
            })
    }

    /// Validate and stage a command selected through a host-native menu.
    pub fn queue_native_menu_selection(&mut self, menu_id: i16, item_number: i16) -> bool {
        let modal = self.modal_status();
        if modal.dialog.is_some() || modal.file {
            return false;
        }
        let menu_list = ppc_current_menu_list(&mut self.memory);
        if ppc_menu_selection_result(&mut self.memory, menu_list, menu_id, item_number).is_none() {
            return false;
        }
        self.toolbox_startup
            .pending_native_menu_selection
            .stage((menu_id, item_number));
        true
    }
}
