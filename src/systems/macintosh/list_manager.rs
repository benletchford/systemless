//! Architecture-neutral List Manager records.

use std::collections::{BTreeSet, HashMap};
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT_LIST_GENERATION: AtomicU64 = AtomicU64::new(1);

pub(crate) fn new_list_generation() -> u64 {
    NEXT_LIST_GENERATION
        .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |next| {
            next.checked_add(1)
        })
        .expect("list lifetime generation exhausted")
}

/// Canonical host-side state for one guest `ListRec`.
///
/// The relocatable list record and cell-data handle remain guest-visible, but
/// the List Manager's logical cells, selection, geometry, and click state
/// belong to the Macintosh process rather than either CPU adapter. More
/// Macintosh Toolbox (1993), pp. 4-3--4-7 and 4-70--4-76.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProcessListRecord {
    pub(crate) handle: u32,
    pub(crate) generation: u64,
    pub(crate) definition_id: i16,
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

impl ProcessListRecord {
    /// LScroll is bounded by fully visible cells; a clipped last row must
    /// still be scrollable into full view. More Macintosh Toolbox, pp. 4-89--4-90;
    /// confirmed with 150-pixel views and 18-pixel rows on Mac OS 8.1.
    pub(crate) fn scrollbar_limits(&self, vertical: bool) -> (i16, i16, i16) {
        let (start, end, origin, pixels, cell) = if vertical {
            (
                self.data_bounds.0,
                self.data_bounds.2,
                self.visible.0,
                self.view_rect.2.saturating_sub(self.view_rect.0),
                self.cell_size.0,
            )
        } else {
            (
                self.data_bounds.1,
                self.data_bounds.3,
                self.visible.1,
                self.view_rect.3.saturating_sub(self.view_rect.1),
                self.cell_size.1,
            )
        };
        let page = (pixels.max(0) / cell.max(1)).max(1);
        let max = end.saturating_sub(page).max(start);
        (origin.clamp(start, max), start, max)
    }

    pub(crate) fn set_visible_origin(&mut self, row: i16, column: i16) {
        let (_, min_row, max_row) = self.scrollbar_limits(true);
        let (_, min_column, max_column) = self.scrollbar_limits(false);
        let top = row.clamp(min_row, max_row);
        let left = column.clamp(min_column, max_column);
        let extent = |pixels: i16, cell: i16| {
            ((i32::from(pixels.max(0)) + i32::from(cell.max(1)) - 1) / i32::from(cell.max(1)))
                .max(1)
                .min(i32::from(i16::MAX)) as i16
        };
        let rows = extent(
            self.view_rect.2.saturating_sub(self.view_rect.0),
            self.cell_size.0,
        );
        let columns = extent(
            self.view_rect.3.saturating_sub(self.view_rect.1),
            self.cell_size.1,
        );
        self.visible = (
            top,
            left,
            top.saturating_add(rows).min(self.data_bounds.2),
            left.saturating_add(columns).min(self.data_bounds.3),
        );
    }
}

/// Retained standard list-scrollbar arrow tracking. LClick owns the call until
/// release; each arrow action scrolls one cell without changing selection.
/// More Macintosh Toolbox (1993), pp. 4-84--4-85.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ListScrollbarTracking {
    pub list: u32,
    pub generation: u64,
    pub control: u32,
    pub pointer: u32,
    pub control_generation: u64,
    pub vertical: bool,
    pub classic: bool,
    pub frame: (u32, u32),
    pub bounds: (i16, i16, i16, i16),
    pub direction: i16,
    pub last_tick: u32,
}

impl ListScrollbarTracking {
    pub(crate) fn arrow(
        bounds: (i16, i16, i16, i16),
        point: (i16, i16),
        vertical: bool,
    ) -> Option<i16> {
        let (top, left, bottom, right) = bounds;
        if point.0 < top || point.0 >= bottom || point.1 < left || point.1 >= right {
            return None;
        }
        let (start, end, axis) = if vertical {
            (top, bottom, point.0)
        } else {
            (left, right, point.1)
        };
        // Standard scrollbars reserve sixteen pixels for each arrow.
        // Macintosh Toolbox Essentials (1992), pp. 5-10--5-12.
        if i32::from(end) - i32::from(start) < 48 {
            return None;
        }
        if i32::from(axis) < i32::from(start) + 16 {
            Some(-1)
        } else if i32::from(axis) >= i32::from(end) - 16 {
            Some(1)
        } else {
            None
        }
    }

    pub(crate) fn step(&mut self, point: (i16, i16), tick: u32) -> Option<i16> {
        if Self::arrow(self.bounds, point, self.vertical) != Some(self.direction)
            || tick.wrapping_sub(self.last_tick)
                < super::control_manager::SCROLLBAR_ACTION_REPEAT_TICKS
        {
            return None;
        }
        self.last_tick = tick;
        Some(self.direction)
    }
}

/// Process-owned List Manager state keyed by guest `ListHandle`.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ProcessListManagerState {
    records: HashMap<u32, ProcessListRecord>,
    pub(crate) scroll_tracking: Option<ListScrollbarTracking>,
}

impl ProcessListManagerState {
    pub(crate) fn is_pristine(&self) -> bool {
        self.records.is_empty()
    }

    pub(crate) fn insert_record(&mut self, handle: u32, record: ProcessListRecord) {
        self.records.insert(handle, record);
    }

    pub(crate) fn remove_record(&mut self, handle: u32) -> Option<ProcessListRecord> {
        if self
            .scroll_tracking
            .as_ref()
            .is_some_and(|tracking| tracking.list == handle)
        {
            self.scroll_tracking = None;
        }
        self.records.remove(&handle)
    }

    pub(crate) fn with_record_mut<R>(
        &mut self,
        handle: u32,
        f: impl FnOnce(&mut ProcessListRecord) -> R,
    ) -> Option<R> {
        self.records.get_mut(&handle).map(f)
    }

    pub(crate) fn with_record_ref<R>(
        &self,
        handle: u32,
        f: impl FnOnce(&ProcessListRecord) -> R,
    ) -> Option<R> {
        self.records.get(&handle).map(f)
    }

    pub(crate) fn get_record(&self, handle: u32) -> Option<ProcessListRecord> {
        self.records.get(&handle).cloned()
    }

    #[cfg(test)]
    pub(crate) fn contains_handle(&self, handle: u32) -> bool {
        self.records.contains_key(&handle)
    }

    pub(crate) fn records(&self) -> Vec<ProcessListRecord> {
        self.records.values().cloned().collect()
    }

    #[cfg(test)]
    pub(crate) fn len(&self) -> usize {
        self.records.len()
    }

    #[cfg(test)]
    pub(crate) fn is_empty(&self) -> bool {
        self.records.is_empty()
    }

    #[allow(dead_code)]
    pub(crate) fn clear(&mut self) {
        self.records.clear();
        self.scroll_tracking = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn retained_list_arrow_uses_ticks_and_exposed_hit_region() {
        let mut tracking = ListScrollbarTracking {
            list: 1,
            generation: 1,
            control: 2,
            pointer: 3,
            control_generation: 1,
            vertical: true,
            classic: true,
            frame: (4, 0),
            bounds: (10, 20, 110, 36),
            direction: 1,
            last_tick: u32::MAX - 2,
        };
        assert_eq!(tracking.step((102, 28), 0), Some(1));
        assert_eq!(tracking.step((102, 28), 1), None);
        assert_eq!(tracking.step((102, 28), 3), Some(1));
        assert_eq!(tracking.step((102, 40), 6), None);
        assert_eq!(tracking.step((18, 28), 6), None);
        assert_eq!(tracking.step((102, 28), 6), Some(1));
        assert_eq!(
            ListScrollbarTracking::arrow((20, 10, 36, 110), (28, 102), false),
            Some(1)
        );
        let mut manager = ProcessListManagerState::default();
        manager.scroll_tracking = Some(tracking);
        manager.remove_record(1);
        assert!(manager.scroll_tracking.is_none());
    }

    #[test]
    fn clipped_cells_can_scroll_fully_into_view_and_back() {
        let mut list = ProcessListRecord {
            handle: 0,
            generation: new_list_generation(),
            definition_id: 0,
            cells_handle: 0,
            view_rect: (78, 24, 228, 528),
            data_bounds: (0, 0, 12, 1),
            cell_size: (18, 504),
            visible: (0, 0, 9, 1),
            port: 0,
            draw_enabled: true,
            active: true,
            cells: HashMap::new(),
            selected: BTreeSet::new(),
            last_click: (0, 0),
            last_click_tick: 0,
        };
        assert_eq!(list.scrollbar_limits(true), (0, 0, 4));
        list.set_visible_origin(4, 0);
        assert_eq!(list.visible, (4, 0, 12, 1));
        list.set_visible_origin(100, 0);
        assert_eq!(list.visible, (4, 0, 12, 1));
        list.set_visible_origin(0, 0);
        assert_eq!(list.visible, (0, 0, 9, 1));
        list.view_rect = (78, 24, 192, 474);
        list.set_visible_origin(4, 0);
        assert_eq!(list.visible, (4, 0, 11, 1));
        assert_eq!(list.scrollbar_limits(true), (4, 0, 6));
        list.set_visible_origin(100, 100);
        assert_eq!(list.visible, (6, 0, 12, 1));
    }

    #[test]
    fn process_list_manager_state_encapsulation() {
        let mut state = ProcessListManagerState::default();
        assert!(state.is_pristine());
        assert!(state.is_empty());
        assert_eq!(state.len(), 0);

        let record = ProcessListRecord {
            handle: 0x1000,
            generation: new_list_generation(),
            definition_id: 0,
            cells_handle: 0x2000,
            view_rect: (0, 0, 40, 100),
            data_bounds: (0, 0, 2, 1),
            cell_size: (20, 100),
            visible: (0, 0, 2, 1),
            port: 0,
            draw_enabled: true,
            active: true,
            cells: HashMap::new(),
            selected: BTreeSet::new(),
            last_click: (0, 0),
            last_click_tick: 0,
        };
        state.insert_record(0x1000, record.clone());
        assert!(!state.is_pristine());
        assert!(!state.is_empty());
        assert_eq!(state.len(), 1);
        assert!(state.contains_handle(0x1000));
        assert_eq!(state.get_record(0x1000), Some(record.clone()));
        assert_eq!(
            state.with_record_ref(0x1000, |rec| rec.cells_handle),
            Some(0x2000)
        );
        state.with_record_mut(0x1000, |rec| rec.active = false);
        assert!(!state.get_record(0x1000).unwrap().active);
        assert_eq!(state.records().len(), 1);
        assert_eq!(state.remove_record(0x1000).unwrap().handle, 0x1000);
        assert!(state.is_empty());
    }
}
