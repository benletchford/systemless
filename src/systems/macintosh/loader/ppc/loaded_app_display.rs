//! PowerPC loaded application front buffer inspection and window geometry methods on [`PpcLoadedApp`].

use super::*;

impl PpcLoadedApp {
    /// Read the live DITL and control records for frontend presentation.
    /// Macintosh Toolbox Essentials (1992), pp. 6-120--6-124.
    pub(crate) fn dialog_items_snapshot(
        &mut self,
        dialog: u32,
        bounds: (i16, i16, i16, i16),
    ) -> Option<Vec<crate::dialog_manager::DialogItemSnapshot>> {
        let handles = self.handles();
        let items = ppc_dialog_items_for_dialog(&mut self.memory, &handles, dialog)?;
        // DialogRecord.textH belongs to editField (a zero-based DITL index).
        // Read the live TERec selection rather than inferring it from the
        // item's text or the host focus. Macintosh Toolbox Essentials (1992),
        // pp. 6-101--6-102; Text (1993), pp. 2-72, 2-78, 2-85--2-86.
        let active_edit_index = self
            .memory
            .read_u16_be(dialog + crate::dialog_manager::DIALOG_EDIT_FIELD_OFFSET)
            .filter(|index| *index != u16::MAX)
            .map(usize::from);
        let caret_visible = self.memory
            .read_u32_be(dialog + crate::dialog_manager::DIALOG_TEXT_HANDLE_OFFSET)
            .and_then(|handle| ppc_te_record_ptr(&mut self.memory, handle))
            .and_then(|ptr| self.memory.read_u16_be(ptr + PPC_TE_CARET_STATE_OFFSET))
            .map(|state| state == 0);
        let active_selection = self
            .memory
            .read_u32_be(dialog + crate::dialog_manager::DIALOG_TEXT_HANDLE_OFFSET)
            .and_then(|handle| ppc_te_record_ptr(&mut self.memory, handle))
            .and_then(|te_ptr| {
                let length = self.memory.read_u16_be(te_ptr + PPC_TE_LENGTH_OFFSET)?;
                let start = self.memory.read_u16_be(te_ptr + PPC_TE_SEL_START_OFFSET)?;
                let end = self.memory.read_u16_be(te_ptr + PPC_TE_SEL_END_OFFSET)?;
                (start <= end && end <= length)
                    .then_some((i16::try_from(start).ok()?, i16::try_from(end).ok()?))
            });
        Some(
            items
                .iter()
                .enumerate()
                .map(|(index, item)| {
                    let kind = item.kind();
                    let text = decode_mac_roman(&ppc_dialog_item_title(
                        &mut self.memory,
                        &handles,
                        item,
                    ));
                    let value = matches!(
                        kind,
                        crate::dialog_manager::DialogItemKind::Checkbox
                            | crate::dialog_manager::DialogItemKind::RadioButton
                    )
                    .then(|| {
                        self.memory
                            .read_u32_be(item.handle)
                            .filter(|control| *control != 0)
                            .and_then(|control| {
                                self.memory
                                    .read_u16_be(control + PPC_CONTROL_VALUE_OFFSET)
                            })
                            .map(|value| value as i16)
                    })
                    .flatten();
                    crate::dialog_manager::DialogItemSnapshot {
                        pressed: matches!(kind, crate::dialog_manager::DialogItemKind::Button
                            | crate::dialog_manager::DialogItemKind::Checkbox
                            | crate::dialog_manager::DialogItemKind::RadioButton)
                            && ppc_control_hilited(&mut self.memory, item.handle),
                        number: (index + 1) as i16,
                        kind,
                        bounds: crate::dialog_manager::dialog_rect_to_global(bounds, item.rect),
                        text,
                        enabled: item.is_enabled(),
                        visible: !crate::dialog_manager::is_dialog_item_rect_hidden(item.rect),
                        value,
                        caret_visible: (active_edit_index == Some(index)).then_some(caret_visible).flatten(),
                        selection: (kind == crate::dialog_manager::DialogItemKind::EditText
                            && active_edit_index == Some(index))
                            .then_some(active_selection)
                            .flatten(),
                    }
                })
                .collect(),
        )
    }

    pub(crate) fn window_definition_id(&mut self, window: u32) -> i16 {
        ppc_window_proc_id(&mut self.memory, window)
    }
    pub fn current_front_buffer(&self) -> Option<PpcFrontBuffer> {
        ppc_front_buffer_for_gworld(&self.gworlds, *self.current_gworld)
    }

    pub fn presented_front_buffer(&self) -> Option<PpcFrontBuffer> {
        if self.draw_sprocket.active_context.is_some() || self.draw_sprocket.swap_count > 0 {
            ppc_front_buffer_for_gworld(&self.gworlds, self.draw_sprocket.front_buffer_gworld)
                .or_else(|| ppc_front_buffer_for_gworld(&self.gworlds, PPC_MAIN_GWORLD))
        } else {
            ppc_front_buffer_for_gworld(&self.gworlds, PPC_MAIN_GWORLD)
        }
    }

    pub fn read_front_buffer_row(
        &mut self,
        front_buffer: PpcFrontBuffer,
        row: u32,
        dst: &mut [u8],
    ) -> Option<()> {
        if row >= front_buffer.height || dst.len() < front_buffer.row_bytes as usize {
            return None;
        }
        let row_addr = front_buffer
            .base_addr
            .checked_add(row.checked_mul(front_buffer.row_bytes)?)?;
        self.memory
            .read_bytes_into(row_addr, &mut dst[..front_buffer.row_bytes as usize])?;
        Some(())
    }

    /// Number of live Window Manager records, excluding screen and offscreen
    /// graphics worlds.
    pub fn window_count(&mut self) -> usize {
        self.gworlds
            .iter()
            .filter(|record| {
                !matches!(record.port, PPC_MAIN_GWORLD | PPC_DSP_BACK_GWORLD)
                    && self
                        .memory
                        .read_u16_be(record.port.wrapping_add(PPC_CWINDOW_WINDOW_KIND_OFFSET))
                        .is_some_and(|kind| kind != 0)
            })
            .count()
    }

    /// Global content bounds of the frontmost visible Window Manager record.
    pub fn window_bounds(&mut self) -> (i16, i16, i16, i16) {
        ppc_front_visible_window(&mut self.memory, &self.gworlds)
            .and_then(|window| {
                ppc_window_global_content_bounds(&mut self.memory, &self.gworlds, window)
            })
            .unwrap_or((0, 0, 0, 0))
    }
}
