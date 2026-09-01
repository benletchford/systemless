//! Architecture-neutral Control Manager records and list operations.

/// Host metadata for one guest `ControlRecord`.
///
/// The relocatable record and its window-list link remain canonical guest
/// memory. This process-owned entry retains only information that the HLE
/// cannot recover reliably from the record, including the original control
/// definition ID and pop-up definition private values. Inside Macintosh
/// Volume I (1985), pp. I-316--I-319 and I-328--I-333.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct ProcessControlRecord {
    pub(crate) handle: u32,
    pub(crate) pointer: u32,
    pub(crate) proc_id: i16,
    pub(crate) popup_menu_id: i16,
    pub(crate) popup_title_width: Option<i16>,
}

/// Canonical Control Manager metadata for one Macintosh process.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(crate) struct ProcessControlManagerState {
    records: Vec<ProcessControlRecord>,
}

impl ProcessControlManagerState {
    pub(crate) fn is_pristine(&self) -> bool {
        self.records.is_empty()
    }

    pub(crate) fn register(&mut self, handle: u32, pointer: u32, proc_id: i16, popup_menu_id: i16) {
        self.records
            .retain(|record| handle == 0 || record.handle != handle || record.pointer == pointer);
        if let Some(record) = self
            .records
            .iter_mut()
            .find(|record| record.pointer == pointer && pointer != 0)
        {
            if handle != 0 {
                record.handle = handle;
            }
            record.proc_id = proc_id;
            record.popup_menu_id = popup_menu_id;
            return;
        }
        self.records.push(ProcessControlRecord {
            handle,
            pointer,
            proc_id,
            popup_menu_id,
            popup_title_width: None,
        });
    }

    pub(crate) fn proc_id(&self, pointer: u32) -> i16 {
        self.records
            .iter()
            .find(|record| record.pointer == pointer)
            .map_or(0, |record| record.proc_id)
    }

    #[cfg(test)]
    pub(crate) fn contains_pointer(&self, pointer: u32) -> bool {
        self.records.iter().any(|record| record.pointer == pointer)
    }

    pub(crate) fn set_proc_id(&mut self, pointer: u32, proc_id: i16) {
        if let Some(record) = self
            .records
            .iter_mut()
            .find(|record| record.pointer == pointer)
        {
            record.proc_id = proc_id;
        } else {
            self.register(0, pointer, proc_id, 0);
        }
    }

    pub(crate) fn associate_handle(&mut self, handle: u32, pointer: u32) {
        self.records
            .retain(|record| record.handle != handle || record.pointer == pointer);
        if let Some(record) = self
            .records
            .iter_mut()
            .find(|record| record.pointer == pointer)
        {
            record.handle = handle;
        } else {
            self.register(handle, pointer, 0, 0);
        }
    }

    pub(crate) fn set_popup_title_width(&mut self, pointer: u32, width: i16) {
        if !self.records.iter().any(|record| record.pointer == pointer) {
            self.register(0, pointer, 0, 0);
        }
        if let Some(record) = self
            .records
            .iter_mut()
            .find(|record| record.pointer == pointer)
        {
            record.popup_title_width = Some(width);
        }
    }

    pub(crate) fn popup_title_width(&self, pointer: u32, fallback: i16) -> i16 {
        self.records
            .iter()
            .find(|record| record.pointer == pointer)
            .and_then(|record| record.popup_title_width)
            .unwrap_or(fallback)
    }

    pub(crate) fn remove_pointer(&mut self, pointer: u32) {
        self.records.retain(|record| record.pointer != pointer);
    }

    #[cfg(test)]
    pub(crate) fn remove_handle(&mut self, handle: u32) {
        self.records.retain(|record| record.handle != handle);
    }
}

impl std::ops::Deref for ProcessControlManagerState {
    type Target = Vec<ProcessControlRecord>;

    fn deref(&self) -> &Self::Target {
        &self.records
    }
}

impl std::ops::DerefMut for ProcessControlManagerState {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.records
    }
}

/// Maximum number of controls accepted from one live guest `wControlList`.
///
/// This is a defensive corruption bound, not a guest-visible Control Manager
/// limit. A repeated handle terminates traversal before this bound is reached.
const MAX_CONTROL_LIST_ENTRIES: usize = 4096;

/// Standard `pushButProc` corner oval used by the classic control definition.
pub(crate) const STANDARD_BUTTON_OVAL: i16 = 10;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct CheckboxLayout {
    pub(crate) indicator: (i16, i16, i16, i16),
    pub(crate) label_left: i16,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct RadioButtonLayout {
    pub(crate) indicator: (i16, i16, i16, i16),
    pub(crate) label_left: i16,
}

/// Resolve standard checkbox indicator and label geometry from `contrlRect`.
///
/// The checkbox CDEF owns a 12-pixel indicator, inset two pixels from the
/// control's leading edge, with four pixels before its title. Macintosh
/// Toolbox Essentials (1992), pp. 5-15--5-16.
pub(crate) fn standard_checkbox_layout(
    (top, left, bottom, _right): (i16, i16, i16, i16),
) -> CheckboxLayout {
    let indicator_size = 12.min(bottom.saturating_sub(top).max(1));
    let indicator_top =
        top.saturating_add(bottom.saturating_sub(top).saturating_sub(indicator_size) / 2);
    let indicator_left = left.saturating_add(2);
    CheckboxLayout {
        indicator: (
            indicator_top,
            indicator_left,
            indicator_top.saturating_add(indicator_size),
            indicator_left.saturating_add(indicator_size),
        ),
        label_left: indicator_left
            .saturating_add(indicator_size)
            .saturating_add(4),
    }
}

/// Resolve standard radio-button indicator and label geometry from
/// `contrlRect`.
///
/// The standard radio-button CDEF uses a 12-pixel round indicator, inset two
/// pixels from the control's leading edge, with four pixels before its title.
/// Inside Macintosh Volume I (1985), p. I-322.
pub(crate) fn standard_radio_button_layout(
    (top, left, bottom, _right): (i16, i16, i16, i16),
) -> RadioButtonLayout {
    let indicator_size = 12.min(bottom.saturating_sub(top).max(1));
    let indicator_top =
        top.saturating_add(bottom.saturating_sub(top).saturating_sub(indicator_size) / 2);
    let indicator_left = left.saturating_add(2);
    RadioButtonLayout {
        indicator: (
            indicator_top,
            indicator_left,
            indicator_top.saturating_add(indicator_size),
            indicator_left.saturating_add(indicator_size),
        ),
        label_left: indicator_left
            .saturating_add(indicator_size)
            .saturating_add(4),
    }
}

/// Visit the two diagonal pixels for each row of a selected checkbox mark.
/// Inside Macintosh Volume I (1985), p. I-322, Figure 27.
pub(crate) fn for_each_standard_checkbox_mark_pixel(
    size: i16,
    mut visit: impl FnMut(i16, i16),
) {
    for offset in 1..size.saturating_sub(1) {
        visit(offset, offset);
        visit(size.saturating_sub(1).saturating_sub(offset), offset);
    }
}

/// Center one system-font label inside a standard control rectangle.
pub(crate) fn centered_control_label_origin(
    (top, left, bottom, right): (i16, i16, i16, i16),
    text_advance: i16,
    ascent: i16,
    descent: i16,
) -> (i16, i16) {
    (
        left.saturating_add(right.saturating_sub(left).saturating_sub(text_advance) / 2),
        top.saturating_add(
            bottom
                .saturating_sub(top)
                .saturating_sub(ascent.saturating_add(descent))
                / 2,
        )
        .saturating_add(ascent),
    )
}

/// Resolve the handles that `DrawControls` must present, in draw order.
///
/// `NewControl` prepends records to `wControlList`. `DrawControls` draws in
/// reverse order of creation, so the architecture-neutral manager reverses
/// that newest-first guest chain and returns the first-created control first.
/// CPU adapters remain responsible only for reading the live next-handle field
/// and presenting or invoking the resulting control. Macintosh Toolbox
/// Essentials (1992), pp. 5-82 and 5-87--5-88.
pub(crate) fn control_draw_order<Handle>(
    head: Handle,
    mut next: impl FnMut(Handle) -> Option<Handle>,
) -> Vec<Handle>
where
    Handle: Copy + Eq + Default,
{
    let nil = Handle::default();
    let mut newest_first = Vec::new();
    let mut handle = head;
    while handle != nil
        && newest_first.len() < MAX_CONTROL_LIST_ENTRIES
        && !newest_first.contains(&handle)
    {
        newest_first.push(handle);
        handle = next(handle).unwrap_or(nil);
    }
    newest_first.reverse();
    newest_first
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    #[test]
    fn draw_order_reverses_the_newest_first_guest_chain() {
        let next = HashMap::from([(3u32, 2u32), (2, 1), (1, 0)]);
        assert_eq!(
            control_draw_order(3, |handle| next.get(&handle).copied()),
            [1, 2, 3]
        );
    }

    #[test]
    fn draw_order_stops_at_a_corrupt_cycle() {
        let next = HashMap::from([(3u32, 2u32), (2, 3)]);
        assert_eq!(
            control_draw_order(3, |handle| next.get(&handle).copied()),
            [2, 3]
        );
    }

    #[test]
    fn standard_checkbox_layout_is_shared_guest_geometry() {
        assert_eq!(
            standard_checkbox_layout((255, 185, 279, 315)),
            CheckboxLayout {
                indicator: (261, 187, 273, 199),
                label_left: 203,
            }
        );
    }

    #[test]
    fn standard_checkbox_mark_is_the_two_interior_diagonals() {
        let mut pixels = Vec::new();
        for_each_standard_checkbox_mark_pixel(5, |x, y| pixels.push((x, y)));
        assert_eq!(pixels, [(1, 1), (3, 1), (2, 2), (2, 2), (3, 3), (1, 3)]);
    }

    #[test]
    fn standard_radio_button_layout_is_shared_guest_geometry() {
        assert_eq!(
            standard_radio_button_layout((70, 250, 90, 390)),
            RadioButtonLayout {
                indicator: (74, 252, 86, 264),
                label_left: 268,
            }
        );
    }
}
