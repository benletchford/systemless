//! PowerPC loaded application guest menu snapshot and native menu selection staging
//! methods on [`PpcLoadedApp`].

use super::*;

impl PpcLoadedApp {
    /// Copy the live PowerPC Menu Manager list into the same frontend-neutral
    /// model used by the 68k Toolbox implementation.
    pub fn guest_menu_snapshot(&mut self) -> GuestMenuSnapshot {
        let menu_list = ppc_current_menu_list(&mut self.memory);
        self.process_file_system.with_mut(|file_system| {
            ppc_guest_menu_snapshot_with_resources(
                &mut self.memory,
                menu_list,
                &file_system.resource_manager.vfs_resources,
            )
        })
    }

    /// Validate and stage a command selected through a host-native menu.
    pub fn queue_native_menu_selection(&mut self, menu_id: i16, item_number: i16) -> bool {
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
