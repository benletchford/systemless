//! PowerPC loaded application trap default gateway and CFM symbol bindings methods
//! on [`PpcLoadedApp`].

use super::*;

impl PpcLoadedApp {
    pub(crate) fn attach_trap_default_gateway(&mut self, trap_word: u16, gateway: u32) {
        self.trap_default_gateways.insert(
            crate::trap::manager::raw_trap_route(trap_word).canonical_word,
            gateway,
        );
    }

    pub(crate) fn cfm_symbol_bindings(&mut self) -> impl crate::cfm::CfmSymbolBindings + '_ {
        PpcPersistedSymbolBindings::new(
            &mut self.imports,
            &mut self.import_count,
            ppc_import_layout(),
            &SystemlessPpcImportBindingPolicy,
        )
    }
}
