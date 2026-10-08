//! PowerPC Toolbox startup and runtime state.

use super::dispatch_event::{PpcCarbonEventDispatchRecord, PpcCarbonEventHandlerRecord, PpcCarbonEventRecord, PpcEventLoopTimerRecord};
use super::*;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PpcToolboxStartupState {
    pub(crate) gestalt_values: HashMap<u32, u32>,
    /// Installed physical memory of the process's machine.
    pub(crate) physical_ram_size: u32,
    pub init_graf_count: u32,
    pub init_graf_global_ptr: u32,
    pub fonts_initialized: bool,
    pub(crate) preserve_glyph: bool,
    pub windows_initialized: bool,
    pub menus_initialized: bool,
    pub(crate) menu_item_commands: crate::menu_manager::MenuItemU32Values,
    pub(crate) menu_item_refcons: crate::menu_manager::MenuItemU32Values,
    pub(crate) help_balloons_enabled: bool,
    pub menu_bar_draw_count: u32,
    pub host_menu_bar_hidden: bool,
    pub(crate) pending_native_menu_selection: SharedNativeMenuSelection,
    pub(crate) execution: ExecutionMenuViews,
    /// Process-owned 68k switch marker/gateway and compatibility stack used
    /// whenever native PowerPC enters classic code through Mixed Mode.
    pub(crate) mixed_mode_m68k: SharedProcessMixedModeM68kState,
    pub(super) system_allocations: PpcSystemAllocationPool,
    pub(super) kchr_cache_ptr: u32,
    pub(super) cf_strings: dispatch_core_foundation::PpcCfStringState,
    pub(super) icon_refs: dispatch_icon_services::PpcIconRefState,
    pub(crate) go_away_tracking: Option<PpcGoAwayTrackingState>,
    pub(crate) drag_window_tracking: Option<PpcDragWindowTrackingState>,
    pub(crate) grow_window_tracking: Option<PpcGrowWindowTrackingState>,
    pub(crate) simple_control_tracking: Option<PpcSimpleControlTrackingState>,
    pub(crate) scrollbar_thumb_tracking: Option<PpcScrollbarThumbTrackingState>,
    /// Retained native Standard File calls are resumed at the same import
    /// frame after the host supplies a mouse or keyboard event.
    pub(super) standard_file_get_filtering: Option<PpcStandardFileFilteringState>,
    pub(super) standard_file_get_tracking: Option<PpcStandardFileGetTrackingState>,
    pub(super) standard_file_put_tracking: Option<PpcStandardFilePutTrackingState>,
    pub(crate) next_standard_file_generation: u64,
    pub text_edit_initialized: bool,
    pub(crate) printing_error: i16,
    pub dialogs_initialized: bool,
    pub dialog_resume_proc: u32,
    pub flush_events_count: u32,
    pub last_flush_event_mask: u16,
    pub last_flush_stop_mask: u16,
    /// Power Manager nesting level for AutoSleepControl(false).
    pub(crate) auto_sleep_disable_level: u32,
    pub dispose_dialog_count: u32,
    pub last_disposed_dialog: u32,
    /// Most recent EventRecord exposed through the native event imports.
    pub(crate) last_event_record: Option<EventRecordSnapshot>,
    pub(crate) event_queue_probe: EventQueueProbeSnapshot,
    pub(super) event_loop_timers: Vec<PpcEventLoopTimerRecord>,
    pub(super) next_event_loop_timer_ref: u32,
    pub(super) carbon_event_handlers: Vec<PpcCarbonEventHandlerRecord>,
    pub(super) next_carbon_event_handler_ref: u32,
    pub(super) carbon_events: Vec<PpcCarbonEventRecord>,
    pub(super) next_carbon_event_ref: u32,
    pub(super) carbon_event_queue: VecDeque<(u32, i16)>,
    /// Caller return PC, previous tick, and remaining finite wait ticks.
    pub(super) receive_next_event_deadline: Option<(u32, u32, u64)>,
    pub(super) carbon_event_dispatch_stack: Vec<PpcCarbonEventDispatchRecord>,
    pub(super) next_carbon_event_call_ref: u32,
    pub(super) application_event_loop_context: Option<(u32, u32)>,
    pub(super) application_event_loop_quit_requested: bool,
    /// Last tick through which a Carbon or classic event-loop wait is active.
    pub(super) event_loop_poll_until_tick: Option<u32>,
    pub(crate) last_button_result: Option<bool>,
    pub(crate) last_still_down_result: Option<bool>,
    pub(crate) last_wait_mouse_up_result: Option<bool>,
    pub(crate) activation_event_seen: bool,
    pub(crate) update_event_seen: bool,
    pub delay_deadline: Option<u32>,
    pub next_ct_seed: u32,
    pub(crate) last_quickdraw_error: SharedProcessQuickDrawError,
    pub open_region_port: u32,
    pub open_region_save_handle: u32,
    pub open_region_bounds: Option<(i16, i16, i16, i16)>,
    /// Exact collected scanlines for framed shapes in an open region.
    pub open_region_rows: Option<(i16, Vec<Vec<i16>>)>,
    /// PicHandle, recording port, frame, and PICT v2 command stream.
    pub(crate) open_picture: Option<(u32, u32, (i16, i16, i16, i16), Vec<u8>)>,
    pub(crate) application_palette: u32,
    pub(crate) application_palette_updates: u16,
    /// A DialogRecord uses offset 156 for its DITL handle, so palette
    /// associations for dialogs must stay outside the guest window record.
    pub(crate) dialog_palettes: HashMap<u32, (u32, u16)>,
    pub(crate) palette_allocations: Vec<PpcPaletteAllocation>,
    pub(crate) active_device_palettes: HashMap<u32, u32>,
    pub(crate) known_gdevices: Vec<u32>,
    pub(crate) indexed_screen_ctables: HashMap<u32, u32>,
    pub(crate) indexed_screen_mode: Option<(u32, bool)>,
    pub(crate) gworld_allocations: HashMap<u32, PpcGWorldAllocationRecord>,
    pub(crate) quickdraw_back_indices: HashMap<u32, u8>,
    pub clut_protected: [bool; 256],
    pub clut_reserved: [bool; 256],
    pub(crate) clut_protected_by_device: HashMap<u32, [bool; 256]>,
    pub(crate) clut_reserved_by_device: HashMap<u32, [bool; 256]>,
    pub quickdraw_pen_pattern: [u8; 8],
    /// Current Color QuickDraw background pattern. A set bit selects the
    /// foreground color and a clear bit selects the background color, as in
    /// the classic `Pattern` record used by BackPat.
    pub quickdraw_back_pattern: [u8; 8],
    pub ae_interaction_allowed: u8,
    pub(crate) stdc_signal_state: PpcStdSignalState,
    pub(crate) stdc_jmpbufs: HashMap<u32, dispatch_stdc::PpcStdJmpContext>,
    pub(super) mp_semaphores: dispatch_threads::PpcMpSemaphoreState,
    pub(crate) window_default_buttons: HashMap<u32, u32>,
    pub(crate) window_cancel_buttons: HashMap<u32, u32>,
    pub(crate) user_focus_window: u32,
    pub(crate) window_modified: HashMap<u32, bool>,
    pub(crate) window_proxy_icons: HashMap<u32, u32>,
    pub(crate) window_modality: HashMap<u32, (u32, u32)>,
    pub(crate) windows_without_updates: HashSet<u32>,
    pub(crate) window_properties: HashMap<u32, Vec<crate::window_manager::ProcessWindowProperty>>,
    pub(crate) window_attributes: HashMap<u32, u32>,
}

impl Default for PpcToolboxStartupState {
    fn default() -> Self {
        Self {
            gestalt_values: HashMap::new(),
            physical_ram_size: REFERENCE_MACHINE_PROFILE.ram_size_bytes,
            init_graf_count: 0,
            init_graf_global_ptr: 0,
            fonts_initialized: false,
            preserve_glyph: false,
            windows_initialized: false,
            menus_initialized: false,
            menu_item_commands: crate::menu_manager::MenuItemU32Values::default(),
            menu_item_refcons: crate::menu_manager::MenuItemU32Values::default(),
            help_balloons_enabled: false,
            menu_bar_draw_count: 0,
            host_menu_bar_hidden: false,
            pending_native_menu_selection: SharedNativeMenuSelection::default(),
            execution: ExecutionMenuViews::detached(),
            mixed_mode_m68k: SharedProcessMixedModeM68kState::default(),
            system_allocations: PpcSystemAllocationPool::default(),
            kchr_cache_ptr: 0,
            cf_strings: dispatch_core_foundation::PpcCfStringState::default(),
            icon_refs: dispatch_icon_services::PpcIconRefState::default(),
            go_away_tracking: None,
            drag_window_tracking: None,
            grow_window_tracking: None,
            simple_control_tracking: None,
            scrollbar_thumb_tracking: None,
            standard_file_get_filtering: None,
            standard_file_get_tracking: None,
            standard_file_put_tracking: None,
            next_standard_file_generation: 0,
            text_edit_initialized: false,
            printing_error: 0,
            dialogs_initialized: false,
            dialog_resume_proc: 0,
            flush_events_count: 0,
            last_flush_event_mask: 0,
            last_flush_stop_mask: 0,
            auto_sleep_disable_level: 0,
            dispose_dialog_count: 0,
            last_disposed_dialog: 0,
            last_event_record: None,
            event_queue_probe: EventQueueProbeSnapshot::default(),
            event_loop_timers: Vec::new(),
            next_event_loop_timer_ref: 0x100,
            carbon_event_handlers: Vec::new(),
            next_carbon_event_handler_ref: 0x4000_0000,
            carbon_events: Vec::new(),
            next_carbon_event_ref: 0x5000_0000,
            carbon_event_queue: VecDeque::new(),
            receive_next_event_deadline: None,
            carbon_event_dispatch_stack: Vec::new(),
            next_carbon_event_call_ref: 0x6000_0000,
            application_event_loop_context: None,
            application_event_loop_quit_requested: false,
            event_loop_poll_until_tick: None,
            last_button_result: None,
            last_still_down_result: None,
            last_wait_mouse_up_result: None,
            activation_event_seen: false,
            update_event_seen: false,
            delay_deadline: None,
            next_ct_seed: 0,
            last_quickdraw_error: SharedProcessQuickDrawError::default(),
            open_region_port: 0,
            open_region_save_handle: 0,
            open_region_bounds: None,
            open_region_rows: None,
            open_picture: None,
            application_palette: 0,
            application_palette_updates: 0,
            dialog_palettes: HashMap::new(),
            palette_allocations: Vec::new(),
            active_device_palettes: HashMap::new(),
            known_gdevices: vec![PPC_MAIN_GDEVICE],
            indexed_screen_ctables: HashMap::new(),
            indexed_screen_mode: Some((PPC_MAIN_PIXEL_DEPTH, true)),
            gworld_allocations: HashMap::new(),
            quickdraw_back_indices: HashMap::new(),
            clut_protected: [false; 256],
            clut_reserved: [false; 256],
            clut_protected_by_device: HashMap::new(),
            clut_reserved_by_device: HashMap::new(),
            quickdraw_pen_pattern: [0xff; 8],
            quickdraw_back_pattern: [0x00; 8],
            ae_interaction_allowed: 1,
            stdc_signal_state: PpcStdSignalState::default(),
            stdc_jmpbufs: HashMap::new(),
            mp_semaphores: dispatch_threads::PpcMpSemaphoreState::default(),
            window_default_buttons: HashMap::new(),
            window_cancel_buttons: HashMap::new(),
            user_focus_window: 0,
            window_modified: HashMap::new(),
            window_proxy_icons: HashMap::new(),
            window_modality: HashMap::new(),
            windows_without_updates: HashSet::new(),
            window_properties: HashMap::new(),
            window_attributes: HashMap::new(),
        }
    }
}

impl PpcToolboxStartupState {
    pub(crate) fn standard_file_snapshot(
        &self,
    ) -> Option<crate::standard_file_ui::StandardFileSnapshot> {
        use crate::standard_file_ui::{
            StandardFileEntrySnapshot, StandardFileGetLayout, StandardFileKind,
            StandardFilePutLayout,
            StandardFileSnapshot,
        };

        if let Some(tracking) = self.standard_file_get_tracking.as_ref().or_else(|| {
            self.standard_file_get_filtering
                .as_ref()
                .map(|filtering| &filtering.tracking)
        }) {
            return Some(StandardFileSnapshot {
                guest_id: tracking.call.reply,
                generation: tracking.generation,
                kind: StandardFileKind::Get,
                standard_entry_point: tracking.standard_entry_point,
                bounds: tracking.bounds,
                directory_id: tracking.current_dir_id,
                entries: Some(
                    tracking
                        .entries
                        .iter()
                        .map(|entry| StandardFileEntrySnapshot {
                            name: crate::mac_roman::decode_mac_roman(&entry.name),
                            directory_id: entry.dir_id,
                            is_directory: entry.is_directory,
                            file_type: entry.file_type,
                        })
                        .collect(),
                ),
                selected: (tracking.selected < tracking.entries.len())
                    .then_some(tracking.selected),
                prompt: None,
                name: None,
                name_selection: None,
                name_has_focus: None,
                directory_label: Some(crate::trap::dispatch::BOOT_VOLUME_NAME.to_string()),
                get_layout: Some({
                    use super::dispatch_standard_file::{
                        PPC_STANDARD_FILE_GET_CANCEL_RECT, PPC_STANDARD_FILE_GET_DESKTOP_RECT,
                        PPC_STANDARD_FILE_GET_EJECT_RECT, PPC_STANDARD_FILE_GET_LIST_RECT,
                        PPC_STANDARD_FILE_GET_OPEN_RECT, PPC_STANDARD_FILE_GET_ROW_HEIGHT,
                        PPC_STANDARD_FILE_GET_SCROLL_RECT, PPC_STANDARD_FILE_GET_VOLUME_LABEL_RECT,
                        PPC_STANDARD_FILE_GET_VOLUME_RECT,
                    };
                    let global = |rect| StandardFilePutLayout::global_rect(tracking.bounds, rect);
                    StandardFileGetLayout {
                        volume: global(PPC_STANDARD_FILE_GET_VOLUME_RECT),
                        directory_label: global(PPC_STANDARD_FILE_GET_VOLUME_LABEL_RECT),
                        list: global(PPC_STANDARD_FILE_GET_LIST_RECT),
                        scroll: global(PPC_STANDARD_FILE_GET_SCROLL_RECT),
                        eject: global(PPC_STANDARD_FILE_GET_EJECT_RECT),
                        desktop: global(PPC_STANDARD_FILE_GET_DESKTOP_RECT),
                        cancel: global(PPC_STANDARD_FILE_GET_CANCEL_RECT),
                        open: global(PPC_STANDARD_FILE_GET_OPEN_RECT),
                        row_height: PPC_STANDARD_FILE_GET_ROW_HEIGHT,
                        first_visible: tracking.selected.saturating_sub(7),
                        visible_rows: 8,
                    }
                }),
                put_layout: None,
            });
        }
        let tracking = self.standard_file_put_tracking.as_ref()?;
        Some(StandardFileSnapshot {
            guest_id: tracking.call.reply,
            generation: tracking.generation,
            kind: StandardFileKind::Put,
            standard_entry_point: tracking.standard_entry_point,
            bounds: tracking.bounds,
            directory_id: tracking.dir_id,
            entries: Some(
                tracking
                    .entries
                    .iter()
                    .map(|entry| StandardFileEntrySnapshot {
                        name: crate::mac_roman::decode_mac_roman(&entry.name),
                        directory_id: entry.dir_id,
                        is_directory: entry.is_directory,
                        file_type: entry.file_type,
                    })
                    .collect(),
            ),
            selected: tracking.selected,
            prompt: Some(crate::mac_roman::decode_mac_roman(&tracking.prompt)),
            name: Some(crate::mac_roman::decode_mac_roman(&tracking.name)),
            name_selection: Some((tracking.sel_start, tracking.sel_end)),
            name_has_focus: Some(!tracking.list_has_focus),
            directory_label: Some(crate::mac_roman::decode_mac_roman(&tracking.directory_name)),
            get_layout: None,
            put_layout: Some({
                use super::dispatch_standard_file::{
                    PPC_STANDARD_FILE_GET_ROW_HEIGHT, PPC_STANDARD_FILE_PUT_CANCEL_RECT,
                    PPC_STANDARD_FILE_PUT_DESKTOP_RECT,
                    PPC_STANDARD_FILE_PUT_DIRECTORY_LABEL_RECT, PPC_STANDARD_FILE_PUT_LIST_RECT,
                    PPC_STANDARD_FILE_PUT_NAME_RECT, PPC_STANDARD_FILE_PUT_PROMPT_RECT,
                    PPC_STANDARD_FILE_PUT_SAVE_RECT, PPC_STANDARD_FILE_PUT_SCROLL_RECT,
                };
                let global = |rect| StandardFilePutLayout::global_rect(tracking.bounds, rect);
                StandardFilePutLayout {
                    directory_label: global(PPC_STANDARD_FILE_PUT_DIRECTORY_LABEL_RECT),
                    list: global(PPC_STANDARD_FILE_PUT_LIST_RECT),
                    scroll: global(PPC_STANDARD_FILE_PUT_SCROLL_RECT),
                    prompt: global(PPC_STANDARD_FILE_PUT_PROMPT_RECT),
                    name: global(PPC_STANDARD_FILE_PUT_NAME_RECT),
                    desktop: global(PPC_STANDARD_FILE_PUT_DESKTOP_RECT),
                    cancel: global(PPC_STANDARD_FILE_PUT_CANCEL_RECT),
                    save: global(PPC_STANDARD_FILE_PUT_SAVE_RECT),
                    row_height: PPC_STANDARD_FILE_GET_ROW_HEIGHT,
                    first_visible: tracking.selected.unwrap_or(0).saturating_sub(7),
                    visible_rows: 8,
                }
            }),
        })
    }

    pub(crate) fn window_default_button(&self, window: u32) -> u32 {
        self.window_default_buttons.get(&window).copied().unwrap_or(0)
    }

    pub(crate) fn set_window_default_button(&mut self, window: u32, button: u32) {
        if button != 0 {
            self.window_default_buttons.insert(window, button);
        } else {
            self.window_default_buttons.remove(&window);
        }
    }

    pub(crate) fn window_cancel_button(&self, window: u32) -> u32 {
        self.window_cancel_buttons.get(&window).copied().unwrap_or(0)
    }

    pub(crate) fn set_window_cancel_button(&mut self, window: u32, button: u32) {
        if button != 0 {
            self.window_cancel_buttons.insert(window, button);
        } else {
            self.window_cancel_buttons.remove(&window);
        }
    }

    pub(crate) fn user_focus_window(&self) -> u32 {
        self.user_focus_window
    }

    pub(crate) fn set_user_focus_window(&mut self, window: u32) {
        self.user_focus_window = window;
    }

    pub(crate) fn is_window_modified(&self, window: u32) -> bool {
        self.window_modified.get(&window).copied().unwrap_or(false)
    }

    pub(crate) fn set_window_modified(&mut self, window: u32, modified: bool) {
        if modified {
            self.window_modified.insert(window, true);
        } else {
            self.window_modified.remove(&window);
        }
    }

    pub(crate) fn window_proxy_icon(&self, window: u32) -> u32 {
        self.window_proxy_icons.get(&window).copied().unwrap_or(0)
    }

    pub(crate) fn set_window_proxy_icon(&mut self, window: u32, icon: u32) {
        if icon != 0 {
            self.window_proxy_icons.insert(window, icon);
        } else {
            self.window_proxy_icons.remove(&window);
        }
    }

    pub(crate) fn remove_window_proxy_icon(&mut self, window: u32) {
        self.window_proxy_icons.remove(&window);
    }

    pub(crate) fn window_attributes(&self, window: u32) -> u32 {
        self.window_attributes
            .get(&window)
            .copied()
            .unwrap_or(crate::window_manager::WINDOW_ATTRIBUTES_STANDARD)
    }

    pub(crate) fn set_window_attributes(&mut self, window: u32, attrs: u32) {
        self.window_attributes.insert(window, attrs);
    }

    pub(crate) fn retained_host_overlay_rects(&self) -> Vec<(i16, i16, i16, i16)> {
        let mut rects: Vec<_> = self
            .standard_file_get_tracking
            .iter()
            .map(|tracking| tracking.bounds)
            .chain(
                self.standard_file_put_tracking
                    .iter()
                    .map(|tracking| tracking.bounds),
            )
            .collect();
        // Menus own screen pixels outside WindowList too, including the
        // saved shadow strip. Keep the host desktop pass beneath every pane.
        // Macintosh Toolbox Essentials (1992), pp. 3-122--3-123 and 4-118--4-119.
        if let Some(menu) = self.execution.menu().as_ref() {
            rects.push((
                menu.popup_top,
                menu.popup_left,
                menu.popup_top.saturating_add(menu.saved_height),
                menu.popup_left.saturating_add(menu.saved_width),
            ));
            rects.extend(menu.submenus.iter().map(|pane| {
                (
                    pane.popup_top,
                    pane.popup_left,
                    pane.popup_top.saturating_add(pane.saved_height),
                    pane.popup_left.saturating_add(pane.saved_width),
                )
            }));
        }
        rects
    }

    pub(crate) fn active_menu_definition(&self) -> Option<&MenuDefinitionTracking> {
        self.execution
            .menu()
            .as_ref()
            .and_then(PpcMenuTracking::active_definition)
            .or(self.execution.menu().context().definition.as_ref())
    }

    pub(crate) fn with_active_menu_definition_mut<R>(
        &self,
        update: impl FnOnce(&mut MenuDefinitionTracking) -> R,
    ) -> Option<R> {
        if self
            .execution
            .menu()
            .as_ref()
            .and_then(PpcMenuTracking::active_definition)
            .is_some()
        {
            return self
                .execution
                .with_menu_state_mut(|tracking| tracking.active_definition_mut().map(update))
                .flatten();
        }
        self.execution
            .with_existing_menu_context_mut(|context| context.definition.as_mut().map(update))
            .flatten()
    }

    pub(crate) fn clear_active_menu_definition(&mut self) {
        if self
            .execution
            .with_menu_state_mut(|tracking| tracking.take_active_definition().is_some())
            .unwrap_or(false)
        {
            return;
        }
        self.execution
            .with_existing_menu_context_mut(|context| context.definition = None);
    }
}
