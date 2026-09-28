//! PowerPC loaded application startup probe and import trace methods on [`PpcLoadedApp`].

use super::*;

impl PpcLoadedApp {
    pub fn run_import_trace(&mut self, max_cycles: u64) -> (PpcRunResult, Vec<u32>) {
        let mut trace = Vec::new();
        let result = self.cpu.run_with_import_trace(
            &mut self.memory,
            max_cycles,
            self.halt_pc,
            self.import_trap_base,
            self.import_count,
            &mut trace,
        );
        (result, trace)
    }

    pub fn run_until_import_or_fault(&mut self, max_cycles: u64) -> PpcStartupProbe {
        let mut first_import_index = None;
        let result = self.cpu.run_with_imports(
            &mut self.memory,
            max_cycles,
            self.halt_pc,
            self.import_trap_base,
            self.import_count,
            |index, _cpu, _memory| {
                first_import_index = Some(index);
                PpcImportAction::Halt
            },
        );
        PpcStartupProbe {
            result,
            first_import_index,
        }
    }

    pub fn import_binding(&self, symbol_index: u32) -> Option<&PpcImportBinding> {
        self.imports
            .iter()
            .find(|binding| binding.symbol_index == symbol_index)
    }
}
