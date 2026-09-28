//! PowerPC loaded application Resource Manager current file and test error
//! methods on [`PpcLoadedApp`].

use super::*;
#[cfg(test)]
use crate::memory::globals::addr;

impl PpcLoadedApp {
    #[cfg(test)]
    pub(crate) fn set_test_resource_error(&mut self, error: i16) {
        let _ = self.memory.write_u16_be(addr::RES_ERR, error as u16);
    }

    #[cfg(test)]
    pub(crate) fn test_resource_error(&mut self) -> i16 {
        self.memory.read_u16_be(addr::RES_ERR).unwrap_or(0) as i16
    }

    /// Return the process Resource Manager's current resource file.
    pub fn current_resource_refnum(&self) -> i16 {
        *self.process_file_system.current_resource_file
    }

    /// Select the process Resource Manager's current resource file.
    pub fn set_current_resource_refnum(&mut self, refnum: i16) {
        let memory = &mut self.memory;
        self.process_file_system
            .current_resource_file
            .with_mut(|current_file| ppc_set_current_resource_refnum(memory, current_file, refnum));
    }
}
