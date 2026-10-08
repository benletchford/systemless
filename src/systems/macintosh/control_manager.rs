//! Architecture-neutral Control Manager records and list operations.

use std::sync::atomic::{AtomicU64, Ordering};

/// Shared cadence for standard scrollbar action procedures, in guest ticks.
pub(crate) const SCROLLBAR_ACTION_REPEAT_TICKS: u32 = 3;

static NEXT_CONTROL_GENERATION: AtomicU64 = AtomicU64::new(1);

pub(crate) fn new_control_generation() -> u64 {
    NEXT_CONTROL_GENERATION
        .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |next| next.checked_add(1))
        .expect("control lifetime generation exhausted")
}

/// Read-only, frontend-neutral state of a guest Control Manager control.
/// Bounds are in global screen coordinates; `local_bounds` retains the
/// canonical `contrlRect` for cases where a port origin needs more context.
/// Macintosh Toolbox Essentials (1992), pp. 5-60--5-64.
#[doc(hidden)]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ControlSnapshot {
    pub guest_id: u32,
    /// Changes when a disposed control's guest handle is reused.
    pub generation: u64,
    pub owner_id: u32,
    pub proc_id: i16,
    pub local_bounds: (i16, i16, i16, i16),
    pub bounds: (i16, i16, i16, i16),
    pub owner_visible: bool,
    pub visible: bool,
    pub enabled: bool,
    pub hilite: u8,
    pub value: i16,
    pub minimum: i16,
    pub maximum: i16,
    pub title: String,
    /// Standard popup CDEF's associated menu and label width, if applicable.
    pub popup_menu_id: Option<i16>,
    pub popup_title_width: Option<i16>,
}

/// Read the live ControlRecord through the architecture's byte adapter. The
/// process registry identifies a CDEF but never substitutes for guest fields.
/// ControlRecord layout: Macintosh Toolbox Essentials (1992), pp. 5-60--5-64.
pub(crate) fn snapshot_control_record(
    handle: u32,
    expected_pointer: u32,
    generation: u64,
    proc_id: i16,
    popup_menu_id: i16,
    popup_title_width: Option<i16>,
    owner_state: impl Fn(u32) -> Option<((i16, i16, i16, i16), bool)>,
    mut read: impl FnMut(u32) -> Option<u8>,
) -> Option<ControlSnapshot> {
    fn word(read: &mut impl FnMut(u32) -> Option<u8>, address: u32) -> Option<i16> {
        Some(i16::from_be_bytes([
            read(address)?,
            read(address.wrapping_add(1))?,
        ]))
    }
    fn long(read: &mut impl FnMut(u32) -> Option<u8>, address: u32) -> Option<u32> {
        Some(u32::from_be_bytes([
            read(address)?,
            read(address.wrapping_add(1))?,
            read(address.wrapping_add(2))?,
            read(address.wrapping_add(3))?,
        ]))
    }
    let pointer = long(&mut read, handle)?;
    if pointer == 0 || pointer != expected_pointer {
        return None;
    }
    let owner_id = long(&mut read, pointer.wrapping_add(4))?;
    let (owner, owner_visible) = owner_state(owner_id)?;
    let local_bounds = (
        word(&mut read, pointer.wrapping_add(8))?,
        word(&mut read, pointer.wrapping_add(10))?,
        word(&mut read, pointer.wrapping_add(12))?,
        word(&mut read, pointer.wrapping_add(14))?,
    );
    let visible = read(pointer.wrapping_add(16))? != 0;
    let hilite = read(pointer.wrapping_add(17))?;
    let value = word(&mut read, pointer.wrapping_add(18))?;
    let minimum = word(&mut read, pointer.wrapping_add(20))?;
    let maximum = word(&mut read, pointer.wrapping_add(22))?;
    // popupMenuProc stores the MENU ID in contrlMin and reserves contrlMax
    // pixels for its title, but later repurposes these fields for item range.
    // The CDEF's contrlData private record retains MENU handle and ID.
    // MTE (1992), pp. 5-25--5-27 and 5-77.
    let popup = (1008..=1023).contains(&proc_id);
    let private_popup_menu_id = if popup {
        long(&mut read, pointer.wrapping_add(28))
            .filter(|handle| *handle != 0)
            .and_then(|handle| long(&mut read, handle))
            .filter(|pointer| *pointer != 0)
            .and_then(|pointer| word(&mut read, pointer.wrapping_add(4)))
            .filter(|id| *id != 0)
    } else {
        None
    };
    let length = usize::from(read(pointer.wrapping_add(40))?);
    let title = (0..length)
        .map(|index| read(pointer.wrapping_add(41).wrapping_add(index as u32)))
        .collect::<Option<Vec<_>>>()?;
    Some(ControlSnapshot {
        guest_id: handle,
        generation,
        owner_id,
        proc_id,
        local_bounds,
        bounds: crate::dialog_manager::dialog_rect_to_global(owner, local_bounds),
        owner_visible,
        visible,
        enabled: hilite != 255,
        hilite,
        value,
        minimum,
        maximum,
        title: crate::mac_roman::decode_mac_roman(&title),
        popup_menu_id: popup
            .then(|| private_popup_menu_id.or((popup_menu_id != 0).then_some(popup_menu_id)))
            .flatten(),
        popup_title_width: popup.then_some(popup_title_width.unwrap_or(0)),
    })
}

/// Tagged property associated with a ControlRef in Appearance Manager / Carbon.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(crate) struct ProcessControlProperty {
    pub(crate) creator: u32,
    pub(crate) tag: u32,
    pub(crate) attributes: u32,
    pub(crate) data: Vec<u8>,
}

/// Host metadata for one guest `ControlRecord`.
///
/// The relocatable record and its window-list link remain canonical guest
/// memory. This process-owned entry retains only information that the HLE
/// cannot recover reliably from the record, including the original control
/// definition ID, pop-up definition private values, embedding hierarchy,
/// and Carbon custom properties. Inside Macintosh Volume I (1985),
/// pp. I-316--I-319 and I-328--I-333; Mac OS 8.5 Appearance Manager (1998).
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(crate) struct ProcessControlRecord {
    pub(crate) handle: u32,
    pub(crate) pointer: u32,
    pub(crate) generation: u64,
    pub(crate) proc_id: i16,
    pub(crate) popup_menu_id: i16,
    pub(crate) popup_title_width: Option<i16>,
    pub(crate) active: bool,
    pub(crate) font_style: Option<ControlFontStyle>,
    pub(crate) is_root: bool,
    pub(crate) parent: u32,
    pub(crate) sub_controls: Vec<u32>,
    pub(crate) properties: Vec<ProcessControlProperty>,
    pub(crate) color_proc: u32,
    pub(crate) control_id: (u32, i32),
    pub(crate) command_id: u32,
    pub(crate) has_focus: bool,
    pub(crate) focus_part: i16,
    pub(crate) drag_tracking_enabled: bool,
}

/// The Appearance Manager style override associated with a ControlRef.
/// RGBColor components are stored in guest byte order as decoded host words.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct ControlFontStyle {
    pub(crate) flags: i16,
    pub(crate) font: i16,
    pub(crate) size: i16,
    pub(crate) style: i16,
    pub(crate) mode: i16,
    pub(crate) justification: i16,
    pub(crate) foreground: [u16; 3],
    pub(crate) background: [u16; 3],
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
            generation: new_control_generation(),
            proc_id,
            popup_menu_id,
            popup_title_width: None,
            active: true,
            font_style: None,
            is_root: false,
            parent: 0,
            sub_controls: Vec::new(),
            properties: Vec::new(),
            color_proc: 0,
            control_id: (0, 0),
            command_id: 0,
            has_focus: false,
            focus_part: 0,
            drag_tracking_enabled: false,
        });
    }

    #[allow(dead_code)]
    pub(crate) fn color_proc(&self, handle: u32) -> u32 {
        self.records
            .iter()
            .find(|record| record.handle == handle)
            .map_or(0, |record| record.color_proc)
    }

    #[allow(dead_code)]
    pub(crate) fn set_color_proc(&mut self, handle: u32, proc: u32) {
        if let Some(record) = self.records.iter_mut().find(|record| record.handle == handle) {
            record.color_proc = proc;
        }
    }

    #[allow(dead_code)]
    pub(crate) fn control_id(&self, handle: u32) -> (u32, i32) {
        self.records
            .iter()
            .find(|record| record.handle == handle)
            .map_or((0, 0), |record| record.control_id)
    }

    #[allow(dead_code)]
    pub(crate) fn set_control_id(&mut self, handle: u32, signature: u32, id: i32) {
        if let Some(record) = self.records.iter_mut().find(|record| record.handle == handle) {
            record.control_id = (signature, id);
        }
    }

    #[allow(dead_code)]
    pub(crate) fn command_id(&self, handle: u32) -> u32 {
        self.records
            .iter()
            .find(|record| record.handle == handle)
            .map_or(0, |record| record.command_id)
    }

    #[allow(dead_code)]
    pub(crate) fn set_command_id(&mut self, handle: u32, command_id: u32) {
        if let Some(record) = self.records.iter_mut().find(|record| record.handle == handle) {
            record.command_id = command_id;
        }
    }

    #[allow(dead_code)]
    pub(crate) fn has_focus(&self, handle: u32) -> bool {
        self.records
            .iter()
            .find(|record| record.handle == handle)
            .is_some_and(|record| record.has_focus)
    }

    #[allow(dead_code)]
    pub(crate) fn set_has_focus(&mut self, handle: u32, focus: bool) {
        if let Some(record) = self.records.iter_mut().find(|record| record.handle == handle) {
            record.has_focus = focus;
        }
    }

    #[allow(dead_code)]
    pub(crate) fn focus_part(&self, handle: u32) -> i16 {
        self.records
            .iter()
            .find(|record| record.handle == handle)
            .map_or(0, |record| record.focus_part)
    }

    #[allow(dead_code)]
    pub(crate) fn set_focus_part(&mut self, handle: u32, part: i16) {
        if let Some(record) = self.records.iter_mut().find(|record| record.handle == handle) {
            record.focus_part = part;
            record.has_focus = part != 0;
        }
    }

    #[allow(dead_code)]
    pub(crate) fn drag_tracking_enabled(&self, handle: u32) -> bool {
        self.records
            .iter()
            .find(|record| record.handle == handle)
            .is_some_and(|record| record.drag_tracking_enabled)
    }

    #[allow(dead_code)]
    pub(crate) fn set_drag_tracking_enabled(&mut self, handle: u32, enabled: bool) {
        if let Some(record) = self.records.iter_mut().find(|record| record.handle == handle) {
            record.drag_tracking_enabled = enabled;
        }
    }

    pub(crate) fn proc_id(&self, pointer: u32) -> i16 {
        self.records
            .iter()
            .find(|record| record.pointer == pointer)
            .map_or(0, |record| record.proc_id)
    }

    pub(crate) fn contains_pointer(&self, pointer: u32) -> bool {
        self.records.iter().any(|record| record.pointer == pointer)
    }

    pub(crate) fn set_font_style(&mut self, pointer: u32, style: Option<ControlFontStyle>) {
        if let Some(record) = self.records.iter_mut().find(|record| record.pointer == pointer) {
            record.font_style = style;
        }
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
        let handle = self
            .records
            .iter()
            .find(|record| record.pointer == pointer)
            .map_or(0, |record| record.handle);
        if handle != 0 {
            self.remove_handle(handle);
        } else {
            self.records.retain(|record| record.pointer != pointer);
        }
    }

    pub(crate) fn remove_handle(&mut self, handle: u32) {
        let parent = self.parent(handle);
        if parent != 0 {
            if let Some(parent_rec) = self.records.iter_mut().find(|r| r.handle == parent) {
                parent_rec.sub_controls.retain(|h| *h != handle);
            }
        }
        for record in self.records.iter_mut() {
            if record.parent == handle {
                record.parent = 0;
            }
            record.sub_controls.retain(|h| *h != handle);
        }
        self.records.retain(|record| record.handle != handle);
    }

    #[allow(dead_code)]
    pub(crate) fn is_root(&self, handle: u32) -> bool {
        self.records
            .iter()
            .find(|record| record.handle == handle)
            .is_some_and(|record| record.is_root)
    }

    #[allow(dead_code)]
    pub(crate) fn set_is_root(&mut self, handle: u32, is_root: bool) {
        if let Some(record) = self.records.iter_mut().find(|record| record.handle == handle) {
            record.is_root = is_root;
        }
    }

    pub(crate) fn parent(&self, handle: u32) -> u32 {
        self.records
            .iter()
            .find(|record| record.handle == handle)
            .map_or(0, |record| record.parent)
    }

    #[allow(dead_code)]
    pub(crate) fn count_sub_controls(&self, handle: u32) -> u16 {
        self.records
            .iter()
            .find(|record| record.handle == handle)
            .map_or(0, |record| record.sub_controls.len() as u16)
    }

    #[allow(dead_code)]
    pub(crate) fn indexed_sub_control(&self, handle: u32, index: u16) -> Option<u32> {
        if index == 0 {
            return None;
        }
        self.records
            .iter()
            .find(|record| record.handle == handle)
            .and_then(|record| record.sub_controls.get((index - 1) as usize).copied())
    }

    #[allow(dead_code)]
    pub(crate) fn embed_control(&mut self, control: u32, container: u32) -> Result<(), i16> {
        if control == 0 {
            return Err(-30582);
        }
        if control == container {
            return Err(-30594);
        }
        let mut curr = container;
        while curr != 0 {
            if curr == control {
                return Err(-30594);
            }
            curr = self.parent(curr);
        }

        let old_parent = self.parent(control);
        if old_parent != 0 {
            if let Some(old_rec) = self.records.iter_mut().find(|r| r.handle == old_parent) {
                old_rec.sub_controls.retain(|h| *h != control);
            }
        }

        if let Some(rec) = self.records.iter_mut().find(|r| r.handle == control) {
            rec.parent = container;
        } else {
            self.records.push(ProcessControlRecord {
                handle: control,
                parent: container,
                ..Default::default()
            });
        }

        if container != 0 {
            if let Some(container_rec) = self.records.iter_mut().find(|r| r.handle == container) {
                if !container_rec.sub_controls.contains(&control) {
                    container_rec.sub_controls.push(control);
                }
            } else {
                self.records.push(ProcessControlRecord {
                    handle: container,
                    sub_controls: vec![control],
                    ..Default::default()
                });
            }
        }
        Ok(())
    }

    #[allow(dead_code)]
    pub(crate) fn set_property(
        &mut self,
        control: u32,
        creator: u32,
        tag: u32,
        attributes: u32,
        data: Vec<u8>,
    ) {
        if control == 0 {
            return;
        }
        let record = if let Some(rec) = self.records.iter_mut().find(|r| r.handle == control) {
            rec
        } else {
            self.records.push(ProcessControlRecord {
                handle: control,
                ..Default::default()
            });
            self.records.last_mut().unwrap()
        };

        if let Some(prop) = record
            .properties
            .iter_mut()
            .find(|p| p.creator == creator && p.tag == tag)
        {
            prop.attributes = attributes;
            prop.data = data;
        } else {
            record.properties.push(ProcessControlProperty {
                creator,
                tag,
                attributes,
                data,
            });
        }
    }

    #[allow(dead_code)]
    pub(crate) fn get_property(
        &self,
        control: u32,
        creator: u32,
        tag: u32,
    ) -> Option<&ProcessControlProperty> {
        self.records
            .iter()
            .find(|r| r.handle == control)
            .and_then(|r| r.properties.iter().find(|p| p.creator == creator && p.tag == tag))
    }

    #[allow(dead_code)]
    pub(crate) fn remove_property(&mut self, control: u32, creator: u32, tag: u32) -> bool {
        if let Some(rec) = self.records.iter_mut().find(|r| r.handle == control) {
            let before = rec.properties.len();
            rec.properties.retain(|p| !(p.creator == creator && p.tag == tag));
            rec.properties.len() < before
        } else {
            false
        }
    }

    #[allow(dead_code)]
    pub(crate) fn change_property_attributes(
        &mut self,
        control: u32,
        creator: u32,
        tag: u32,
        set: u32,
        clear: u32,
    ) -> Result<u32, i16> {
        if let Some(rec) = self.records.iter_mut().find(|r| r.handle == control) {
            if let Some(prop) = rec
                .properties
                .iter_mut()
                .find(|p| p.creator == creator && p.tag == tag)
            {
                prop.attributes = (prop.attributes | set) & !clear;
                return Ok(prop.attributes);
            }
        }
        Err(-5604)
    }

    #[allow(dead_code)]
    pub(crate) fn set_control_data(
        &mut self,
        control: u32,
        _part: i16,
        tag: u32,
        data: Vec<u8>,
    ) {
        if control == 0 {
            return;
        }
        let record = if let Some(rec) = self.records.iter_mut().find(|r| r.handle == control) {
            rec
        } else {
            self.records.push(ProcessControlRecord {
                handle: control,
                ..Default::default()
            });
            self.records.last_mut().unwrap()
        };

        if let Some(prop) = record.properties.iter_mut().find(|p| p.tag == tag) {
            prop.data = data;
        } else {
            record.properties.push(ProcessControlProperty {
                creator: 0,
                tag,
                attributes: 0,
                data,
            });
        }
    }

    #[allow(dead_code)]
    pub(crate) fn get_control_data(
        &self,
        control: u32,
        _part: i16,
        tag: u32,
    ) -> Option<&[u8]> {
        self.records
            .iter()
            .find(|r| r.handle == control)
            .and_then(|r| {
                r.properties
                    .iter()
                    .find(|p| p.creator == 0 && p.tag == tag)
                    .or_else(|| r.properties.iter().find(|p| p.tag == tag))
            })
            .map(|p| p.data.as_slice())
    }

    #[allow(dead_code)]
    pub(crate) fn get_control_data_size(
        &self,
        control: u32,
        part: i16,
        tag: u32,
    ) -> Option<usize> {
        self.get_control_data(control, part, tag).map(|d| d.len())
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
/// reverse order of creation: the newest-first guest chain is already in
/// draw order, leaving the first-created overlapping control frontmost.
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
    newest_first
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    #[test]
    fn draw_order_preserves_the_newest_first_guest_chain() {
        let next = HashMap::from([(3u32, 2u32), (2, 1), (1, 0)]);
        assert_eq!(
            control_draw_order(3, |handle| next.get(&handle).copied()),
            [3, 2, 1]
        );
    }

    #[test]
    fn draw_order_stops_at_a_corrupt_cycle() {
        let next = HashMap::from([(3u32, 2u32), (2, 3)]);
        assert_eq!(
            control_draw_order(3, |handle| next.get(&handle).copied()),
            [3, 2]
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

    #[test]
    fn process_control_manager_state_evaluates_embedding_hierarchy() {
        let mut state = ProcessControlManagerState::default();
        state.register(10, 0x1000, 0, 0);
        state.register(20, 0x2000, 0, 0);
        state.register(30, 0x3000, 0, 0);

        state.set_is_root(10, true);
        assert!(state.is_root(10));
        assert!(!state.is_root(20));

        assert_eq!(state.count_sub_controls(10), 0);
        assert_eq!(state.embed_control(20, 10), Ok(()));
        assert_eq!(state.embed_control(30, 10), Ok(()));
        assert_eq!(state.embed_control(20, 20), Err(-30594));

        assert_eq!(state.count_sub_controls(10), 2);
        assert_eq!(state.indexed_sub_control(10, 1), Some(20));
        assert_eq!(state.indexed_sub_control(10, 2), Some(30));
        assert_eq!(state.indexed_sub_control(10, 3), None);

        assert_eq!(state.parent(20), 10);
        assert_eq!(state.parent(10), 0);

        state.remove_handle(20);
        assert_eq!(state.count_sub_controls(10), 1);
        assert_eq!(state.indexed_sub_control(10, 1), Some(30));
    }

    #[test]
    fn control_generation_changes_only_for_a_new_lifetime() {
        let mut state = ProcessControlManagerState::default();
        state.register(10, 0x1000, 1, 0);
        let first = state
            .iter()
            .find(|record| record.handle == 10)
            .unwrap()
            .generation;
        assert_ne!(first, 0);
        state.register(10, 0x1000, 1, 0);
        assert_eq!(state[0].generation, first);
        state.remove_handle(10);
        state.register(10, 0x1000, 1, 0);
        assert!(state[0].generation > first);
    }

    #[test]
    fn process_control_manager_state_evaluates_tagged_properties() {
        let mut state = ProcessControlManagerState::default();
        state.register(10, 0x1000, 0, 0);

        let creator = 0x5445_5354;
        let tag = 0x5441_4731;
        state.set_property(10, creator, tag, 0x01, vec![1, 2, 3]);

        let prop = state.get_property(10, creator, tag).unwrap();
        assert_eq!(prop.creator, creator);
        assert_eq!(prop.tag, tag);
        assert_eq!(prop.attributes, 0x01);
        assert_eq!(prop.data, vec![1, 2, 3]);

        assert_eq!(
            state.change_property_attributes(10, creator, tag, 0x10, 0x01),
            Ok(0x10)
        );
        let prop = state.get_property(10, creator, tag).unwrap();
        assert_eq!(prop.attributes, 0x10);

        assert!(state.remove_property(10, creator, tag));
        assert!(state.get_property(10, creator, tag).is_none());
        assert!(!state.remove_property(10, creator, tag));
    }

    #[test]
    fn process_control_manager_state_evaluates_focus_part_and_drag_tracking() {
        let mut state = ProcessControlManagerState::default();
        state.register(10, 0x1000, 0, 0);

        assert_eq!(state.focus_part(10), 0);
        assert!(!state.has_focus(10));
        assert!(!state.drag_tracking_enabled(10));

        state.set_focus_part(10, 1);
        assert_eq!(state.focus_part(10), 1);
        assert!(state.has_focus(10));

        state.set_focus_part(10, 0);
        assert_eq!(state.focus_part(10), 0);
        assert!(!state.has_focus(10));

        state.set_drag_tracking_enabled(10, true);
        assert!(state.drag_tracking_enabled(10));
    }
}
