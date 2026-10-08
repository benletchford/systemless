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
        // Visible describes the viewport capacity, including a clipped cell beyond
        // dataBounds. More Macintosh Toolbox, pp. 4-6--4-7; native Mac OS 8.1
        // reports rows 6..13 for a 114-pixel view of twelve 18-pixel rows.
        self.visible = (
            top,
            left,
            top.saturating_add(rows),
            left.saturating_add(columns),
        );
    }
}

/// Retained standard list-scrollbar tracking. LClick owns the
/// call until release and scrolls without changing selection.
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
    pub part: u8,
    pub last_tick: u32,
    pub start_mouse: (i16, i16),
    pub start_limits: (i16, i16, i16),
}

impl ListScrollbarTracking {
    pub(crate) fn part(
        bounds: (i16, i16, i16, i16),
        point: (i16, i16),
        vertical: bool,
        limits: (i16, i16, i16),
    ) -> Option<u8> {
        let (top, left, bottom, right) = bounds;
        if point.0 < top || point.0 >= bottom || point.1 < left || point.1 >= right {
            return None;
        }
        let (start, end, axis) = if vertical {
            (top, bottom, point.0)
        } else {
            (left, right, point.1)
        };
        let (start, end, axis) = (i32::from(start), i32::from(end), i32::from(axis));
        let (value, minimum, maximum) = limits;
        if end - start < 48 || minimum >= maximum {
            return None;
        }
        // Standard CDEF arrows and scroll boxes occupy sixteen pixels.
        // Macintosh Toolbox Essentials (1992), pp. 5-10--5-12, 5-57--5-61.
        if axis < start + 16 {
            return Some(20);
        }
        if axis >= end - 16 {
            return Some(21);
        }
        let range = i32::from(maximum) - i32::from(minimum);
        let value = i32::from(value.clamp(minimum, maximum)) - i32::from(minimum);
        let thumb =
            start + 16 + (i64::from(value) * i64::from(end - start - 48) / i64::from(range)) as i32;
        if axis < thumb {
            Some(22)
        } else if axis >= thumb + 16 {
            Some(23)
        } else {
            Some(129)
        }
    }

    pub(crate) fn hit(&self, point: (i16, i16), record: &ProcessListRecord) -> bool {
        if self.part == 129 {
            return self.release_delta(point, record).is_some();
        }
        Self::part(
            self.bounds,
            point,
            self.vertical,
            record.scrollbar_limits(self.vertical),
        ) == Some(self.part)
    }

    /// Commit a scroll-box drag only on release. MTE (1992), pp. 5-89--5-90;
    /// native Mac OS 8.1 PPC LClick preserves list content during the drag.
    pub(crate) fn release_delta(
        &self,
        point: (i16, i16),
        record: &ProcessListRecord,
    ) -> Option<i32> {
        if self.part != 129 || record.scrollbar_limits(self.vertical) != self.start_limits {
            return None;
        }
        let (top, left, bottom, right) = self.bounds;
        if point.0 < top.saturating_sub(30)
            || point.0 >= bottom.saturating_add(30)
            || point.1 < left.saturating_sub(30)
            || point.1 >= right.saturating_add(30)
        {
            return None;
        }
        let (start, end, delta) = if self.vertical {
            (
                top,
                bottom,
                i32::from(point.0) - i32::from(self.start_mouse.0),
            )
        } else {
            (
                left,
                right,
                i32::from(point.1) - i32::from(self.start_mouse.1),
            )
        };
        let travel = i32::from(end) - i32::from(start) - 48;
        let (value, minimum, maximum) = self.start_limits;
        let range = i32::from(maximum) - i32::from(minimum);
        if travel <= 0 || range <= 0 {
            return None;
        }
        let initial =
            i64::from(i32::from(value) - i32::from(minimum)) * i64::from(travel) / i64::from(range);
        let position = (initial + i64::from(delta)).clamp(0, i64::from(travel));
        let target = i32::from(minimum)
            + ((position * i64::from(range) + i64::from(travel / 2)) / i64::from(travel)) as i32;
        Some(target - i32::from(value))
    }

    pub(crate) fn step(
        &mut self,
        point: (i16, i16),
        tick: u32,
        record: &ProcessListRecord,
    ) -> Option<i16> {
        if self.part == 129
            || !self.hit(point, record)
            || tick.wrapping_sub(self.last_tick)
                < super::control_manager::SCROLLBAR_ACTION_REPEAT_TICKS
        {
            return None;
        }
        self.last_tick = tick;
        let units = if self.part >= 22 {
            // Page by visible capacity less one cell, keeping the overlap even
            // when the final page has fewer cells. MTE, pp. 5-57--5-61;
            // More Macintosh Toolbox, pp. 4-21--4-22, 4-84--4-85.
            let (pixels, cell) = if self.vertical {
                (
                    i32::from(record.view_rect.2) - i32::from(record.view_rect.0),
                    record.cell_size.0,
                )
            } else {
                (
                    i32::from(record.view_rect.3) - i32::from(record.view_rect.1),
                    record.cell_size.1,
                )
            };
            ((pixels.max(0) + i32::from(cell.max(1)) - 1) / i32::from(cell.max(1)) - 1)
                .clamp(1, i32::from(i16::MAX)) as i16
        } else {
            1
        };
        Some(if matches!(self.part, 20 | 22) {
            -units
        } else {
            units
        })
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
            part: 21,
            last_tick: u32::MAX - 2,
            start_mouse: (0, 0),
            start_limits: (0, 0, 10),
        };
        let record = ProcessListRecord {
            handle: 1,
            generation: 1,
            definition_id: 0,
            cells_handle: 0,
            view_rect: (0, 0, 100, 100),
            data_bounds: (0, 0, 20, 20),
            cell_size: (10, 10),
            visible: (0, 0, 10, 10),
            port: 0,
            draw_enabled: true,
            active: true,
            cells: HashMap::new(),
            selected: BTreeSet::new(),
            last_click: (0, 0),
            last_click_tick: 0,
        };
        assert_eq!(tracking.step((102, 28), 0, &record), Some(1));
        assert_eq!(tracking.step((102, 28), 1, &record), None);
        assert_eq!(tracking.step((102, 28), 3, &record), Some(1));
        assert_eq!(tracking.step((102, 40), 6, &record), None);
        assert_eq!(tracking.step((18, 28), 6, &record), None);
        assert_eq!(tracking.step((102, 28), 6, &record), Some(1));
        assert_eq!(
            ListScrollbarTracking::part((20, 10, 36, 110), (28, 102), false, (0, 0, 10)),
            Some(21)
        );
        let mut manager = ProcessListManagerState::default();
        manager.scroll_tracking = Some(tracking);
        manager.remove_record(1);
        assert!(manager.scroll_tracking.is_none());
    }

    #[test]
    fn list_page_tracks_current_thumb_and_keeps_one_cell_overlap() {
        let mut record = ProcessListRecord {
            handle: 1,
            generation: 1,
            definition_id: 0,
            cells_handle: 0,
            view_rect: (0, 0, 100, 100),
            data_bounds: (0, 0, 20, 20),
            cell_size: (10, 10),
            visible: (0, 0, 10, 10),
            port: 0,
            draw_enabled: true,
            active: true,
            cells: HashMap::new(),
            selected: BTreeSet::new(),
            last_click: (0, 0),
            last_click_tick: 0,
        };
        record.view_rect = (0, 0, 114, 450);
        record.cell_size = (18, 450);
        record.data_bounds = (0, 0, 12, 1);
        record.set_visible_origin(0, 0);
        let mut tracking = ListScrollbarTracking {
            list: 1,
            generation: 1,
            control: 2,
            pointer: 3,
            control_generation: 1,
            vertical: true,
            classic: true,
            frame: (4, 0),
            bounds: (128, 514, 242, 530),
            part: 23,
            last_tick: 0,
            start_mouse: (150, 522),
            start_limits: (0, 0, 6),
        };
        let mut thumb = tracking.clone();
        thumb.part = 129;
        assert_eq!(
            ListScrollbarTracking::part(thumb.bounds, (150, 522), true, (0, 0, 6)),
            Some(129)
        );
        assert_eq!(thumb.step((218, 522), 3, &record), None);
        assert_eq!(thumb.release_delta((218, 522), &record), Some(6));
        assert_eq!(thumb.release_delta((150, 522), &record), Some(0));
        assert_eq!(thumb.release_delta((218, 650), &record), None);
        let mut mutated = record.clone();
        mutated.set_visible_origin(1, 0);
        assert_eq!(thumb.release_delta((218, 522), &mutated), None);
        let mut wide = record.clone();
        wide.data_bounds = (i16::MIN, 0, i16::MAX, 1);
        wide.set_visible_origin(i16::MIN, 0);
        thumb.start_limits = wide.scrollbar_limits(true);
        assert_eq!(thumb.release_delta((218, 522), &wide), Some(65529));
        thumb.start_limits = record.scrollbar_limits(true);
        thumb.vertical = false;
        thumb.bounds = (514, 128, 530, 242);
        thumb.start_mouse = (522, 150);
        let mut horizontal = record.clone();
        horizontal.view_rect = (0, 0, 450, 114);
        horizontal.cell_size = (450, 18);
        horizontal.data_bounds = (0, 0, 1, 12);
        horizontal.set_visible_origin(0, 0);
        assert_eq!(thumb.release_delta((522, 218), &horizontal), Some(6));
        assert_eq!(tracking.step((208, 522), 3, &record), Some(6));
        record.set_visible_origin(6, 0);
        assert_eq!(tracking.step((208, 522), 6, &record), None);
        tracking.part = 22;
        assert_eq!(tracking.step((160, 522), 6, &record), Some(-6));
        record.set_visible_origin(0, 0);
        assert_eq!(tracking.step((160, 522), 9, &record), None);
        assert_eq!(
            ListScrollbarTracking::part(
                (i16::MIN, 0, i16::MAX, 16),
                (0, 8),
                true,
                (i16::MAX, i16::MIN, i16::MAX)
            ),
            Some(22)
        );
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
        assert_eq!(list.visible, (4, 0, 13, 1));
        list.set_visible_origin(100, 0);
        assert_eq!(list.visible, (4, 0, 13, 1));
        list.set_visible_origin(0, 0);
        assert_eq!(list.visible, (0, 0, 9, 1));
        list.view_rect = (78, 24, 192, 474);
        list.set_visible_origin(4, 0);
        assert_eq!(list.visible, (4, 0, 11, 1));
        assert_eq!(list.scrollbar_limits(true), (4, 0, 6));
        list.set_visible_origin(100, 100);
        assert_eq!(list.visible, (6, 0, 13, 1));
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
