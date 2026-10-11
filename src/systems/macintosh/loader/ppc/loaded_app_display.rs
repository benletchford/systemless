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
                    let edit_text_layout = (kind == crate::dialog_manager::DialogItemKind::EditText).then(|| {
                        if active_edit_index != Some(index) {
                            return Some(crate::dialog_manager::DialogEditTextLayout {
                                font: (0, 0), baseline: 12, line_height: 16,
                                wrap: true, text_edit_geometry: false,
                            });
                        }
                        let handle = self.memory.read_u32_be(dialog + crate::dialog_manager::DIALOG_TEXT_HANDLE_OFFSET)?;
                        let record = crate::text_edit::snapshot_guest_records(&[(handle, 0)],
                            &mut |address| self.memory.read_u8(address)).records.into_iter().next()?;
                        // Only the ordinary single-line TE painter is qualified
                        // here. Preserve guest pixels for styled/scrolled fields.
                        if record.styled || record.face != 0 || record.justification != 0
                            || ppc_current_text_style(&mut self.memory, dialog) != 0
                            || record.dest_rect != item.rect || record.view_rect != item.rect
                            || record.display_lines()?.len() != 1 || record.line_height <= 0
                            || decode_mac_roman(&record.text) != text
                            || crate::quickdraw::fonts::get_font_face_or_default(record.font, record.size).size
                                != if record.size == 0 { 12 } else { record.size }
                        {
                            return None;
                        }
                        Some(crate::dialog_manager::DialogEditTextLayout {
                            font: (record.font, record.size), baseline: record.font_ascent,
                            line_height: record.line_height, wrap: false, text_edit_geometry: true,
                        })
                    }).flatten();
                    crate::dialog_manager::DialogItemSnapshot {
                        edit_text_layout,
                        static_text_layout: Some(crate::dialog_manager::DialogStaticTextLayout {
                            wrap_advance_extra: 0, face: 0, font: (0, 0), origin: (1, 12), line_height: 16, inclusive_bottom: false,
                        }),
                        control_identity: self.controls.with_ref(|state| {
                            state.iter().find(|record| record.handle == item.handle
                                && record.pointer != 0
                                && self.memory.read_u32_be(item.handle) == Some(record.pointer)
                                && self.memory.read_u32_be(record.pointer + PPC_CONTROL_OWNER_OFFSET) == Some(dialog))
                                .map(|record| (record.handle, record.generation))
                        }),
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

impl PpcLoadedApp {
    /// Inspect styled TextEdit destination colours; do not mutate its port or
    /// borrow the caller's current foreground, which is restored after drawing.
    pub(crate) fn text_edit_paint_snapshot(
        &mut self, record: &crate::text_edit::TextEditSnapshot,
    ) -> Option<crate::text_edit::TextEditPaintSnapshot> {
        use crate::text_edit::{TextEditCharExtraSnapshot, TextEditInkSnapshot, TextEditPaintSnapshot};
        let runs = record.style_runs.as_ref()?;
        let front = self.presented_front_buffer()?;
        let surface = ppc_live_quickdraw_surface(&mut self.memory, &self.gworlds, record.owner_port)?;
        if !matches!(front.depth, 8 | 16) || surface.front_buffer.base_addr != front.base_addr
            || surface.front_buffer.row_bytes != front.row_bytes || surface.front_buffer.depth != front.depth {
            return None;
        }
        let te = self.memory.read_u32_be(record.guest_id)?;
        let mode = self.memory.read_u16_be(te.checked_add(0x4e)?)? as i16;
        let palette = crate::display::rgba_palette_from_clut_with_gamma(&self.screen_clut, &self.display_gamma.table());
        let rgb_at = |pixel: u16| {
            if front.depth == 16 { return crate::display::rgb555_to_rgb888(pixel); }
            let [r, g, b, _] = palette[usize::from(pixel)].to_le_bytes();
            [r, g, b]
        };
        let mut style_ink = Vec::with_capacity(runs.len());
        for run in runs {
            let pixel = ppc_quickdraw_surface_fore_pixel(&mut self.memory, surface,
                PpcRgbColor { red: run.color.0, green: run.color.1, blue: run.color.2 }, None)?;
            style_ink.push(TextEditInkSnapshot { pixel, rgb: rgb_at(pixel), inverted_rgb: rgb_at(pixel ^ if front.depth == 16 { 0xffff } else { 0xff }) });
        }
        Some(TextEditPaintSnapshot { depth: front.depth as u16, mode,
            char_extra: TextEditCharExtraSnapshot::PpcPacked(ppc_port_char_extra_packed(&mut self.memory, record.owner_port)),
            space_extra: self.memory.read_u32_be(record.owner_port.checked_add(76)?)? as i32,
            style_ink })
    }
}
