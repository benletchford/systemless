//! PowerPC loaded application process context attachment, migrated service adoption,
//! and process memory manager leasing methods on [`PpcLoadedApp`].

use super::*;

impl PpcLoadedApp {
    #[cfg(test)]
    pub(crate) fn guest_calls(&self) -> &SharedGuestCallStack {
        self.toolbox_startup.execution.calls()
    }

    #[cfg(test)]
    pub(crate) fn is_constructed_from_migrated_handles(
        &self,
        handles: &crate::process_context::MigratedProcessHandles,
    ) -> bool {
        self.tick_state.ptr_eq(&handles.ticks)
            && self
                .toolbox_startup
                .execution
                .calls()
                .ptr_eq(&handles.execution)
            && self.toolbox_startup.execution.is_coherent()
    }

    pub(crate) fn preflight_migrated_services(
        &self,
        context: &ProcessContext,
        expected_process_tick_at_commit: u32,
    ) -> Result<
        crate::process_context::MigratedServiceAdoption,
        crate::process_context::MigratedServiceConflict,
    > {
        context.preflight_migrated_adoption(
            &self.tick_state,
            &self.toolbox_startup.execution,
            expected_process_tick_at_commit,
        )
    }

    pub(crate) fn commit_migrated_services(
        &mut self,
        context: &ProcessContext,
        plan: crate::process_context::MigratedServiceAdoption,
    ) {
        context.commit_migrated_adoption(
            plan,
            &mut self.tick_state,
            &mut self.toolbox_startup.execution,
        );
    }

    pub(crate) fn attach_unconverted_process_services(&mut self, context: &mut ProcessContext) {
        let mut attached_memory_manager = None;
        context.attach_memory_manager(&mut attached_memory_manager);
        let attached_memory_manager =
            attached_memory_manager.expect("process context supplies a Memory Manager");
        // A runner shares low memory before attaching the native adapter. If
        // a classic adapter has already initialized a compatible ApplLimit,
        // adopt that value as the process seed instead of allowing the
        // detached native default to overwrite it. Reject values outside the
        // native heap's physical range: a classic-only partition may use a
        // different address ceiling and is not a valid native allocation
        // boundary. Inside Macintosh: Memory (1992), pp. 2-83--2-85.
        let low_memory_application_limit = self
            .memory
            .read_u32_be(crate::memory::globals::addr::APPL_LIMIT)
            .filter(|limit| *limit != 0)
            .filter(|limit| {
                self.process_memory_manager
                    .0
                    .borrow()
                    .native_heap_state()
                    .is_none_or(|heap| *limit >= heap.heap_cursor && *limit <= heap.heap_limit)
            });
        if let Some(low_memory_application_limit) = low_memory_application_limit {
            let mut memory_manager = attached_memory_manager.borrow_mut();
            if !memory_manager.application_heap_limit_is_set() {
                memory_manager.set_application_heap_limit(low_memory_application_limit);
            }
        }
        if !self.process_memory_manager.ptr_eq(&attached_memory_manager) {
            let standalone_memory_manager = self.process_memory_manager.0.borrow();
            let memory_manager = attached_memory_manager.borrow();
            memory_manager.assert_can_adopt_process_memory_manager(&standalone_memory_manager);
        }
        context.attach_file_system(&mut self.process_file_system);
        self.process_file_system
            .with_mut(ProcessFileSystemState::publish_native_vfs_catalogue);
        context.attach_sound_manager(&mut self.sound.manager);
        context.attach_callback_tasks(
            &mut self.timer_tasks,
            &mut self.vbl_tasks,
            &mut self.callback_scheduling,
        );
        context.attach_scrap_state(&mut self.scrap.desktop);
        context.attach_text_edit_manager(&mut self.scrap.text_edit);
        context.attach_control_manager(&mut self.controls);
        context.attach_list_manager(&mut self.list_manager);
        context.attach_collection_manager(&mut self.collections);
        context.attach_notification_queue(&mut self.toolbox_startup.notification_requests);
        context.attach_dialog_text(&mut self.param_text);
        context.attach_cursor_state(&mut self.cursor_state);
        context.activate_quickdraw_selection(&mut self.current_gworld, &mut self.current_gdevice);
        context.attach_quickdraw_op_colors(&mut self.quickdraw_op_colors);
        context.attach_quickdraw_hilite_colors(&mut self.quickdraw_hilite_colors);
        self.process_quickdraw_port_state_attached = true;
        context.attach_quickdraw_error(&mut self.toolbox_startup.last_quickdraw_error);
        context.attach_quickdraw_pixel_states(&mut self.gworld_pixel_states);
        context.attach_display_color_state(
            &mut self.screen_clut,
            &mut self.color_manager_clut,
            &mut self.display_gamma,
        );
        context.attach_event_queue(&mut self.event_queue);
        context.attach_window_list(&mut self.window_list);
        context.attach_input_state(&mut self.process_input);
        if !self.process_memory_manager.ptr_eq(&attached_memory_manager) {
            {
                let standalone_memory_manager = self.process_memory_manager.0.clone();
                let mut standalone_memory_manager = standalone_memory_manager.borrow_mut();
                let mut memory_manager = attached_memory_manager.borrow_mut();
                memory_manager.adopt_process_memory_manager(&mut standalone_memory_manager);
                if let Some(heap) = memory_manager.native_heap_state() {
                    let allocation_limit = memory_manager.native_allocation_limit(heap.heap_limit);
                    self.refresh_process_native_zone(heap, allocation_limit);
                }
            }
            self.process_memory_manager
                .attach_to(attached_memory_manager);
        }
        // APPL_LIMIT is a low-memory projection of the process-owned value.
        // Synchronize it on attachment so a nested 68K callback and a later
        // native import observe the same boundary immediately. Inside
        // Macintosh: Memory (1992), pp. 2-83--2-85.
        let application_heap_limit = self
            .process_memory_manager
            .application_heap_limit(self.stack_base);
        let _ = self.memory.write_u32_be(
            crate::memory::globals::addr::APPL_LIMIT,
            application_heap_limit,
        );
        context
            .attach_native_menu_selection(&mut self.toolbox_startup.pending_native_menu_selection);
        context.attach_mixed_mode_m68k_state(&mut self.toolbox_startup.mixed_mode_m68k);
        context.attach_apple_event_handlers(&mut self.apple_events.handlers);
        context.attach_application_size(&mut self.application_size);
        context.attach_apple_event_launch_state(&mut self.apple_events.apple_event_launch_state);
        context.attach_apple_event_descriptors(&mut self.apple_events.descriptors);
    }

    /// Run one native operation with every process manager continuously attached.
    #[cfg(test)]
    pub(crate) fn with_process_state<R>(&mut self, f: impl FnOnce(&mut Self) -> R) -> R {
        f(self)
    }

    /// Run one native operation with the continuously attached process Memory
    /// Manager as its sole allocator owner.
    #[cfg(test)]
    pub(crate) fn with_process_memory_manager<R>(
        &mut self,
        f: impl FnOnce(&mut Self, &mut ProcessMemoryManager) -> R,
    ) -> R {
        let memory_manager = self.process_memory_manager.0.clone();
        self.with_process_state(|app| {
            let mut memory_manager = memory_manager.borrow_mut();
            f(app, &mut memory_manager)
        })
    }
}
