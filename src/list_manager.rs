//! Architecture-neutral List Manager records.

use std::collections::{BTreeSet, HashMap};

/// Canonical host-side state for one guest `ListRec`.
///
/// The relocatable list record and cell-data handle remain guest-visible, but
/// the List Manager's logical cells, selection, geometry, and click state
/// belong to the Macintosh process rather than either CPU adapter. More
/// Macintosh Toolbox (1993), pp. 4-3--4-7 and 4-70--4-76.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProcessListRecord {
    pub(crate) handle: u32,
    pub(crate) cells_handle: u32,
    pub(crate) view_rect: (i16, i16, i16, i16),
    pub(crate) data_bounds: (i16, i16, i16, i16),
    pub(crate) cell_size: (i16, i16),
    pub(crate) visible: (i16, i16, i16, i16),
    pub(crate) port: u32,
    pub(crate) draw_enabled: bool,
    pub(crate) active: bool,
    pub(crate) cells: HashMap<(i16, i16), Vec<u8>>,
    pub(crate) selected: BTreeSet<(i16, i16)>,
    pub(crate) last_click: (i16, i16),
    pub(crate) last_click_tick: u32,
}

/// Process-owned List Manager state keyed by guest `ListHandle`.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ProcessListManagerState {
    records: HashMap<u32, ProcessListRecord>,
}

impl ProcessListManagerState {
    pub(crate) fn is_pristine(&self) -> bool {
        self.records.is_empty()
    }
}

impl std::ops::Deref for ProcessListManagerState {
    type Target = HashMap<u32, ProcessListRecord>;

    fn deref(&self) -> &Self::Target {
        &self.records
    }
}

impl std::ops::DerefMut for ProcessListManagerState {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.records
    }
}
