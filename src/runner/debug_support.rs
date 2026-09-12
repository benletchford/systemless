use super::*;

impl FixtureRunner {
    pub(super) fn debug_finish_step_if_ready(&mut self) -> bool {
        if self.debug_step_units_remaining() != Some(0) {
            return false;
        }
        let Some(context) = self.debug.step_target_context() else {
            return false;
        };
        let location = crate::debug::adapters::Adapters::for_runner(self)
            .find(context)
            .map(|adapter| adapter.location());
        self.debug_finish_context_step(context, location)
    }

    pub(super) fn debug_stop_at_m68k_breakpoint(&mut self, address: u32) -> bool {
        self.debug_stop_context_at_breakpoint(
            crate::debug::M68K_CONTEXT,
            crate::debug::DebugAddress::new(crate::debug::M68K_SPACE, u64::from(address)),
        )
    }

    pub(super) fn debug_m68k_breakpoint_addresses(&self) -> Vec<u32> {
        self.debug_context_breakpoint_addresses(
            crate::debug::M68K_CONTEXT,
            crate::debug::M68K_SPACE,
        )
        .into_iter()
        .filter_map(|address| u32::try_from(address.offset).ok())
        .collect()
    }

    pub(super) fn debug_note_m68k_executed_units(&mut self, units: usize) {
        self.debug_note_context_executed_units(
            crate::debug::M68K_CONTEXT,
            u32::try_from(units).unwrap_or(u32::MAX),
        );
    }

    pub(super) fn debug_note_terminal(&mut self) {
        let (context, location) = crate::debug::adapters::active_context_and_location(self);
        let guest_tick = self.guest_tick();
        let execution_units = self.total_instructions();
        self.debug
            .fail_pending_captures("runner reached a terminal halt");
        self.debug.stop_at_terminal_halt(
            context,
            location,
            Some(guest_tick),
            Some(execution_units),
        );
    }

    pub(super) fn debug_note_active_context(&mut self) {
        let active = crate::debug::adapters::active_context_id(self);
        self.debug.note_active_context(active);
    }

    #[inline]
    pub(super) fn debug_advance_generation(&mut self) {
        self.debug.advance_generation();
    }

    #[inline]
    pub(super) fn debug_step_units_remaining(&self) -> Option<u32> {
        self.debug.step_units_remaining()
    }

    #[inline]
    pub(super) fn debug_breakpoint_rearming(&self) -> bool {
        self.debug.breakpoint_rearming()
    }

    pub(super) fn debug_note_logical_frame_boundary(&mut self) {
        crate::debug::note_capture_boundary(
            self,
            crate::debug::CaptureBoundaryKind::LogicalFrameComplete,
        );
    }

    pub(super) fn debug_has_pending_logical_frame_capture(&self) -> bool {
        self.debug
            .has_pending_capture_boundary(crate::debug::CaptureBoundaryKind::LogicalFrameComplete)
    }

    pub fn debug_note_context_executed_units(
        &mut self,
        context: crate::debug::ContextId,
        units: u32,
    ) {
        self.debug.note_executed_units(context, units);
    }

    pub fn debug_finish_context_step(
        &mut self,
        context: crate::debug::ContextId,
        location: Option<crate::debug::ExecutionLocation>,
    ) -> bool {
        if self.debug.step_target_context() != Some(context)
            || self.debug.step_units_remaining() != Some(0)
        {
            return false;
        }
        let guest_tick = self.guest_tick();
        let execution_units = self.total_instructions();
        self.debug
            .finish_step(context, location, Some(guest_tick), Some(execution_units))
            .is_some()
    }

    pub fn debug_stop_context_at_breakpoint(
        &mut self,
        context: crate::debug::ContextId,
        address: crate::debug::DebugAddress,
    ) -> bool {
        let Some(id) = self
            .debug
            .breakpoint_at(context, address.space, address.offset)
        else {
            return false;
        };
        let guest_tick = self.guest_tick();
        let execution_units = self.total_instructions();
        self.debug.breakpoint_stop(
            id,
            context,
            Some(crate::debug::ExecutionLocation { context, address }),
            Some(guest_tick),
            Some(execution_units),
        );
        true
    }

    pub fn debug_context_breakpoint_addresses(
        &self,
        context: crate::debug::ContextId,
        space: crate::debug::AddressSpaceId,
    ) -> Vec<crate::debug::DebugAddress> {
        self.debug
            .breakpoint_watch_addresses(context, space)
            .into_iter()
            .map(|offset| crate::debug::DebugAddress::new(space, offset))
            .collect()
    }

    pub fn debug_register_adapter(
        &mut self,
        registration: crate::debug::AdapterRegistration,
    ) -> crate::debug::DebugResult<()> {
        self.debug.register_adapter(registration)
    }

    pub fn debug_register_provider<P>(&mut self, provider: P) -> crate::debug::DebugResult<()>
    where
        P: crate::debug::InspectionProvider + 'static,
    {
        self.debug.register_provider(provider)
    }

    pub fn debug_register_provider_arc(
        &mut self,
        provider: crate::debug::ProviderRegistration,
    ) -> crate::debug::DebugResult<()> {
        self.debug.register_provider_arc(provider)
    }

    pub fn debug_active_architecture(&self) -> crate::debug::ActiveArchitecture {
        let route = self
            .dispatcher
            .guest_calls
            .execution_route(self.native.availability());
        if route == ExecutionRoute::Blocked {
            return crate::debug::ActiveArchitecture::Blocked;
        }
        if self.dispatcher.guest_calls.has_m68k_execution() {
            return crate::debug::ActiveArchitecture::M68k;
        }
        match route {
            ExecutionRoute::NativeApplication => crate::debug::ActiveArchitecture::PowerPc,
            ExecutionRoute::NativeCompanion => crate::debug::ActiveArchitecture::PowerPcCompanion,
            ExecutionRoute::PrepareCompanion => crate::debug::ActiveArchitecture::Blocked,
            ExecutionRoute::Classic => crate::debug::ActiveArchitecture::M68k,
            ExecutionRoute::Blocked => crate::debug::ActiveArchitecture::Blocked,
        }
    }

    pub(crate) fn debug_release_held_input(&mut self) {
        let keys = self.dispatcher.input_state.key_map;
        if keys.iter().any(|key| *key != 0) || self.dispatcher.input_state.mouse_button {
            self.debug.touch();
        }
        for key in 0..128u8 {
            if keys[usize::from(key / 8)] & (1 << (key % 8)) != 0 {
                self.push_key_up(key, 0);
            }
        }
        if self.dispatcher.input_state.mouse_button {
            let (v, h) = self.dispatcher.mouse_position();
            self.push_canonical_mouse_up(v, h);
        }
    }

    pub fn debug_ppc_companion_cpu(&self) -> Option<&ppc::PpcCpu> {
        self.native.companion().map(|app| &app.cpu)
    }

    pub fn debug_ppc_cpu(&self) -> Option<&ppc::PpcCpu> {
        self.native.application().map(|app| &app.cpu)
    }

    #[doc(hidden)]
    pub(crate) fn debug_window_snapshot(&self, limit: usize) -> (Vec<WindowSnapshot>, bool) {
        let windows = &self.dispatcher.window_list;
        (
            crate::window_manager::snapshot_window_stack(
                &windows[..windows.len().min(limit)],
                |address| self.bus.read_byte(address),
            ),
            windows.len() > limit,
        )
    }

    pub(crate) fn debug_screen_mode(&self) -> (u32, u32, u16, u16, u16) {
        self.dispatcher.screen_mode
    }

    pub(crate) fn debug_device_clut(&self) -> &[[u16; 3]; 256] {
        &self.dispatcher.device_clut
    }

    pub(crate) fn debug_device_gamma(&self) -> &crate::display::DisplayGamma {
        &self.dispatcher.device_gamma
    }

    pub(crate) fn debug_palette_argb(&self) -> [u32; 256] {
        crate::display::argb_palette_from_clut_with_gamma(
            &self.dispatcher.device_clut,
            &self.dispatcher.device_gamma,
        )
    }

    pub fn debug_extract_capture_store(&mut self) -> crate::debug::CaptureStore {
        self.debug.extract_capture_store()
    }

    pub fn debug_adopt_capture_store(&mut self, store: crate::debug::CaptureStore) {
        self.debug.adopt_capture_store(store);
    }
}
