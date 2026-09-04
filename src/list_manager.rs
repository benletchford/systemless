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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clipped_cells_can_scroll_fully_into_view_and_back() {
        let mut list = ProcessListRecord {
            handle: 0,
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
}
